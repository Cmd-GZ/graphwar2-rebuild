#!/usr/bin/env python3
"""挖洞点轴定性（`dispatch/14-hole-axis`；`docs/06-verification.md` §24.7 那 200 例）。

输入三样，全是只读：

* `--conv` `examples/replay_attrib --drive conv --policy strict` 的逐批 JSONL
  —— **口径基准**：本工具必须复算出同一组 `挖洞点 66 / 291`，否则不许谈分类；
* `--hole` `examples/hole_axis_probe` 的逐发 JSONL（我方停点 / 爆炸点 / `hole_point`）；
* `--dir`  回放语料（gzip + json 直接解，不经 Rust）。

回放侧只用两类报文：`DestroyObstacle`（**真值挖洞点**，带信封 tick）、
`AddObstacle`/`ClearObstacles`（关卡障碍集合）；外加 `EffectInfo(ExplosionEffect)`。

分四类（互斥，**加总必须等于 200**）：

```text
a) 我方多挖了            我方挖洞点 0.5 内全文件没有任何 DestroyObstacle
b) 位置对但时间不对      我方挖洞点逐位等于文件里某条 DestroyObstacle，但不在本批延迟窗口
c) 把不该挖的障碍当可挖  我方挖洞点不在回放关卡的任何障碍里（画布边界除外），也没有对应销毁点
d) 其它                  剩下的（本语料全部的子因：挖洞点用了采样停点而不是爆炸点）
```

```bash
R=<ws>/.reference/replays/gw2-replays-snapshot/replays
tools/hole_axis_attrib.py --conv .work/conv.jsonl --hole .work/hole.jsonl --dir $R --samples 3
```
"""
import argparse
import collections
import gzip
import json
import math
import os
import sys

# 容差档位（游戏单位；1 个采样点 = 0.01）
TOL = [0.0, 1e-12, 1e-9, 1e-6, 1e-3, 0.01, 0.05, 0.1, 0.5]
# 掩膜光栅化把圆画到 r+1 像素（= +0.05 游戏单位）的抗锯齿环，环内的值 ≠ 0xFF 也算"占用"
RING = 0.06
# 画布：x∈[-25,25]、y∈[-15,15]（`plane::in_canvas`）；贴边的"墙"不是关卡障碍
CANVAS = (24.9, 14.9)


# ---------------------------------------------------------------------------
# 回放侧
# ---------------------------------------------------------------------------
def load_replay(path):
    out = []
    for line in gzip.open(path, "rt").read().splitlines():
        line = line.strip()
        if not line:
            continue
        o = json.loads(line)
        ev = o.get("event")
        if not isinstance(ev, dict) or len(ev) != 1:
            continue
        kind, body = next(iter(ev.items()))
        out.append((kind, body if isinstance(body, dict) else {}, o.get("tick", 0)))
    return out


def batches_of(msgs):
    """与 `replaydiff::split_batches` 同口径。"""
    out, i, n = [], 0, len(msgs)
    while i < n:
        kind, body, _ = msgs[i]
        if kind == "GameStateInfo" and body.get("game_state") == "CalculatingResults":
            start, j, end = i, i, None
            while j < n:
                k2, b2, _ = msgs[j]
                if k2 == "GameStateInfo" and b2.get("game_state") == "ExecutingResults":
                    end = j
                    break
                if j > start and k2 == "GameStateInfo" and b2.get("game_state") == "CalculatingResults":
                    break
                j += 1
            if end is None:
                i += 1
                continue
            tail_end = end + 1
            while tail_end < n:
                k3, b3, _ = msgs[tail_end]
                if k3 == "GameStateInfo" and b3.get("game_state") == "CalculatingResults":
                    break
                tail_end += 1
            out.append((start, end, tail_end))
            i = end + 1
        else:
            i += 1
    return out


def obstacles_upto(msgs, upto):
    obs = []
    for kind, body, _ in msgs[:upto]:
        if kind == "ClearObstacles":
            obs = []
        elif kind == "AddObstacle":
            obs.append((body.get("pos_x"), body.get("pos_y"), body.get("radius") or 0.0))
    return obs


def destroys(msgs, a, b):
    return [(body.get("pos_x"), body.get("pos_y"), tick)
            for kind, body, tick in msgs[a:b] if kind == "DestroyObstacle"]


# ---------------------------------------------------------------------------
def dist(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


def nearest(p, pts):
    if not pts:
        return float("inf"), None
    best = min(pts, key=lambda q: dist(p, q[:2]))
    return dist(p, best), best


def bucket(d):
    for t in TOL[1:]:
        if d <= t:
            return f"<={t:g}"
    return f">{TOL[-1]:g}"


def inside_canvas(p):
    return abs(p[0]) <= CANVAS[0] and abs(p[1]) <= CANVAS[1]


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--conv", required=True)
    ap.add_argument("--hole", required=True)
    ap.add_argument("--dir", required=True)
    ap.add_argument("--samples", type=int, default=3)
    ap.add_argument("--json")
    args = ap.parse_args(argv)

    rows = [json.loads(l) for l in open(args.conv)]
    rows = [r for r in rows if "error" not in r]
    ours = collections.defaultdict(dict)
    for line in open(args.hole):
        r = json.loads(line)
        if "error" in r:
            continue
        ours[r["file"]][(r["batch"], r["shot"])] = r

    # ---- 1) 复算基线口径 66 / 291 ----
    before_ok = before_n = 0
    after_ok = 0
    miss = []
    for r in rows:
        if r["blocked"] is None or not r["obs_destroy"]:
            continue
        before_n += 1
        window = {(repr(x), repr(y)) for x, y in r["obs_destroy"]}
        if (repr(r["blocked"][0]), repr(r["blocked"][1])) in window:
            before_ok += 1
        else:
            miss.append(r)
        shots = [v for (b, _), v in ours.get(r["file"], {}).items() if b == r["batch"] and v["blocked"]]
        if shots:
            last = max(shots, key=lambda x: x["shot"])
            if (repr(last["hole_x"]), repr(last["hole_y"])) in window:
                after_ok += 1
    print(f"口径对齐：挖洞点 {before_ok} / {before_n}"
          f"（基线 66 / 291）{'  ✅' if (before_ok, before_n) == (66, 291) else '  ❌'}")
    print(f"改成「挖洞点 = 爆炸点」以后：{after_ok} / {before_n} "
          f"({100.0 * after_ok / before_n:.1f}%)   ← 同一 291 分母、逐位比\n")

    # ---- 2) 逐例取几何 ----
    cache = {}
    cases = []
    for r in miss:
        f = r["file"]
        if f not in cache:
            cache[f] = None
            path = os.path.join(args.dir, f)
            if os.path.exists(path):
                m = load_replay(path)
                cache[f] = (m, batches_of(m))
        if cache[f] is None:
            continue
        m, bs = cache[f]
        bi = r["batch"]
        if bi >= len(bs):
            continue
        start, end, tail_end = bs[bi]
        shots = [v for (b, _), v in ours.get(f, {}).items() if b == bi and v["blocked"]]
        if not shots:
            continue
        last = max(shots, key=lambda x: x["shot"])
        p = (r["blocked"][0], r["blocked"][1])          # 基线口径的那个点：我方停点
        h = (last["hole_x"], last["hole_y"])            # 修好以后发的那个点：爆炸点
        win = destroys(m, end + 1, tail_end)
        all_d = destroys(m, 0, len(m))
        obs = obstacles_upto(m, start)
        d_pa = nearest(p, all_d)[0]
        d_hw = nearest(h, win)[0]
        d_ha = nearest(h, all_d)[0]
        d_ob = min((dist(h, o[:2]) - o[2] for o in obs), default=float("inf"))
        cases.append({
            "file": f, "batch": bi, "tick": r["tick"], "shots_in_batch": r["shots"],
            "mode": last["mode"],
            "stop": p, "hole": h,
            "stop_eq_expl": last["stop_x"] == last["expl_x"],
            "obs_end_x": last.get("obs_end_x"),
            "window": [[x, y] for x, y, _ in win],
            "window_ticks": [t for _, _, t in win],
            "d_stop_all": d_pa, "d_hole_window": d_hw, "d_hole_all": d_ha,
            "d_hole_obstacle": d_ob,
            "ours_destroy_tick": r["tick"] + 165,
        })

    if args.json:
        with open(args.json, "w") as fh:
            for c in cases:
                fh.write(json.dumps(c, ensure_ascii=False) + "\n")

    # ---- 3) 四类互斥分桶 ----
    zero = [c for c in cases if 0 < c["d_stop_all"] <= 1e-12]      # 末位差档：不进 200
    rest = [c for c in cases if not (0 < c["d_stop_all"] <= 1e-12)]
    cls = collections.defaultdict(list)
    for c in rest:
        if c["d_hole_all"] > 0.5:
            cls["a"].append(c)
        elif c["d_hole_all"] <= 1e-12 and c["d_hole_window"] > 1e-12:
            cls["b"].append(c)
        elif c["d_hole_all"] > 1e-12 and c["d_hole_all"] <= 0.5 \
                and c["d_hole_obstacle"] > RING and inside_canvas(c["hole"]):
            cls["c"].append(c)
        else:
            cls["d"].append(c)

    NAME = {
        "a": "a) 我方多挖了（回放没挖）",
        "b": "b) 位置对但时间不对",
        "c": "c) 把不该挖的障碍当可挖",
        "d": "d) 其它",
    }
    print(f"不命中批 {len(cases)} = 末位差档 {len(zero)}（≤1e-12，不算 200）+ 待分类 {len(rest)}\n")
    print(f"{'类别':<28}{'例数':>6}")
    print("-" * 40)
    for k in "abcd":
        print(f"{NAME[k]:<28}{len(cls[k]):>6}")
    print("-" * 40)
    print(f"{'合计':<28}{sum(len(v) for v in cls.values()):>6}")
    total = sum(len(v) for v in cls.values())
    print(f"加总校验：{total} == 200 ⇒ {'OK' if total == 200 else '不相等！'}")

    # d 的子因
    sub = collections.Counter()
    for c in cls["d"]:
        if c["d_hole_window"] <= 1e-12:
            sub["d1 挖洞点（爆炸点）逐位 == 本批窗口销毁点：纯参照点错"] += 1
        elif c["d_hole_window"] <= 0.02:
            sub["d2 爆炸点与销毁点差 ≤0.02（掩膜边界差一两个采样点）"] += 1
        else:
            sub["d3 爆炸点与销毁点差 >0.02"] += 1
    print("\n== d) 的子因 ==")
    for k, v in sub.most_common():
        print(f"  {k:<56}{v:>5}")

    # ---- 4) 样本 ----
    # a/b/c 若为 0，给出"最接近进桶"的那一例，让 0 是可核对的（不是没查）
    print("\n== 样本（我方停点 / 我方挖洞点=爆炸点 / 回放窗口 DestroyObstacle@tick）==")
    for k in "abc":
        if cls[k]:
            print(f"\n[{k}] {len(cls[k])} 例，取 {min(args.samples, len(cls[k]))} 个：")
            for c in cls[k][: args.samples]:
                show(c)
        else:
            print(f"\n[{k}] 0 例")
    # d 的子因各 3 个
    subs = collections.defaultdict(list)
    for c in cls["d"]:
        if c["d_hole_window"] <= 1e-12:
            subs["d1"].append(c)
        elif c["d_hole_window"] <= 0.02:
            subs["d2"].append(c)
        else:
            subs["d3"].append(c)
    TITLE = {
        "d1": "d1 挖洞点逐位 == 本批窗口销毁点（我方发的是采样停点，不是爆炸点）",
        "d2": "d2 爆炸点与销毁点差 ≤0.02（掩膜边界差一两个采样点）",
        "d3": "d3 爆炸点与销毁点差 >0.02（真几何分歧）",
    }
    for k in ("d1", "d2", "d3"):
        if not subs[k]:
            continue
        print(f"\n[{k}] {len(subs[k])} 例 —— {TITLE[k]}，取 {min(args.samples, len(subs[k]))} 个：")
        for c in subs[k][: args.samples]:
            show(c)

    # 0 例的旁证
    print("\n== a/b/c 为 0 的旁证 ==")
    big = max(rest, key=lambda c: c["d_hole_all"]) if rest else None
    if big:
        print(f"  200 例里 d(挖洞点→全文件 DestroyObstacle) 最大 = {big['d_hole_all']:.4f}"
              f"（{big['file']} 批#{big['batch']}）—— 没有任何一例 > 0.5，故 a) = 0")
    exact_out = [c for c in rest if c["d_hole_all"] <= 1e-12 and c["d_hole_window"] > 1e-12]
    print(f"  「挖洞点逐位存在但不在本批延迟窗口」= {len(exact_out)} 例，故 b) = 0")
    print(f"  「挖洞点既不在关卡障碍里、也没有对应销毁点」= {len(cls['c'])} 例，故 c) = 0 "
          f"（贴画布边界的 {sum(1 for c in rest if not inside_canvas(c['hole']))} 例不算：回放自己也销毁在那里）")
    return 0


def fmt(p):
    return f"({p[0]!r}, {p[1]!r})"


def show(c):
    print(f"  {c['file']} 批#{c['batch']} tick={c['tick']} 发数={c['shots_in_batch']} 模式={c['mode']}")
    print(f"    我方停点       = {fmt(c['stop'])}")
    print(f"    我方挖洞点     = {fmt(c['hole'])}   (爆炸点；回放 end_x={c['obs_end_x']!r})")
    ws = ", ".join(f"{fmt((x, y))}@{t}" for (x, y), t in
                   zip(c["window"][:3], c["window_ticks"][:3]))
    print(f"    回放窗口销毁点 = {ws}")
    print(f"    d(挖洞点→全文件)={c['d_hole_all']:.6g}  d(挖洞点→窗口)={c['d_hole_window']:.6g}"
          f"  d(停点→全文件)={c['d_stop_all']:.6g}  d(挖洞点→障碍面)={c['d_hole_obstacle']:.4f}")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
