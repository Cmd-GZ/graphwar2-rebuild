#!/usr/bin/env python3
"""整局对拍**剩余失败**的归因分桶（`docs/06-verification.md` §24 的那张表就是本脚本的输出）。

输入是 `examples/replay_attrib.rs` 吐的逐批 JSONL（**每一批一行**），
输出是一张**互斥、可加总**的表：各行批数之和 == 失败批数。

为什么分桶不写在 Rust 里：分桶规则要能随时改而不重新编译；
而且规则本身是**结论**，写在文档 §24 里、由本脚本执行，两者对得上才算数。

## 用法

```bash
export CARGO_HOME=... CARGO_TARGET_DIR=$CARGO_HOME/target
R=.reference/replays/gw2-replays-snapshot/replays
# 1) 任务书基线（旧驱动：每发一次 Match::fire，默认口径）
cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive fire --policy default --json /tmp/fire.jsonl
python3 tools/bit_exact_attrib.py --json /tmp/fire.jsonl --check-baseline

# 2) 收敛层（模式感知 + 修正文本 + 爆炸点回代 + 皮肤特效，严格口径）
cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive conv --policy strict --json /tmp/conv.jsonl
python3 tools/bit_exact_attrib.py --json /tmp/conv.jsonl
```

## 分桶规则（**按优先级取第一个命中**，所以互斥）

| 序 | 桶 | 判据 |
|---|---|---|
| 1 | `C` corrected_function 文本 | 首个 diff 的路径是 `*.corrected_function` |
| 2 | `D` 死效块（死亡集合多/少） | 批内**消息种类计数**只差 `EffectInfo/Death|Ash|Spark`（±），不含 `GameStateInfo` |
| 3 | `S` 多发批结构 | ≥2 发，且种类计数只差多出来的 `GameStateInfo`（每发一个窗口 ⇒ +3/发） |
| 4 | `SD` 多发批结构 + 死效块 | ≥2 发，且种类计数同时差 `GameStateInfo`(+) 与死效块(±) |
| 5 | `E1` `end_x` 差一个采样点 | `end_x` 差 ≤ 0.02 且 `pos_x` 与回放逐位/1e-9 内相同 |
| 6 | `E2` `end_x` 真分歧 | 其余 `end_x` 差 |
| 7 | `X` 爆炸点 x（`pos_x ≠ end_x`） | 无 `end_x` 差、有 `pos_x` 差（全部 ≤1e-9 量级） |
| 8 | `Y` 爆炸点 y 末位差 | 无 `end_x`/`pos_x` 差、`pos_y` 差 ≤ 1e-9 |
| 9 | `Y2` 爆炸点 y 中等差 | 同上但 1e-9 < 差 ≤ 1e-3 |
| 10 | `K` 其它（未定性） | 以上全不命中 |

`独占` 列 = **把这一类修好、别的一动不动就会变干净的批数**
（= 该批的 diff 全部落在本类的字段上）。它才是"预计能加多少批"。

## 延迟段（死亡集合 / 挖洞点）是**另一根轴**

它不属于上面那张表：分母是"有死亡/挖洞可比的批"（379/259），不是全部结算批。
本脚本按 `replaydiff::run_file` 的**逐文件**去重规则复算它，用来核对任务书的
`211/379`、`1/259` 两个数。
"""

import argparse
import collections
import json
import sys

DEATH_EFFECTS = ("EffectInfo/DeathEffect", "EffectInfo/AshEffect", "EffectInfo/SparkEffect")
E1_MAX_DELTA = 0.02   # 一个采样步的量级上界（step=0.01，实测最大 0.009375）


def load(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            r = json.loads(line)
            if r.get("batch", -1) >= 0:
                rows.append(r)
    rows.sort(key=lambda r: (r["file"], r["batch"]))
    return rows


def num(s):
    try:
        return float(s)
    except (TypeError, ValueError):
        return None


def get_diffs(r, field):
    return [d for d in r["diffs"] if d["path"].split(".")[-1] == field]


def delta(d):
    a, b = num(d["ours"]), num(d["observed"])
    return abs(a - b) if a is not None and b is not None else None


def kind_deltas(r):
    """批内消息种类计数的差（我方 − 回放）。"""
    a = collections.Counter(r["our_kinds"])
    b = collections.Counter(r["obs_kinds"])
    return {k: a.get(k, 0) - b.get(k, 0) for k in set(a) | set(b) if a.get(k, 0) != b.get(k, 0)}


def classify(r):
    """返回 (桶, 是否独占)。规则见表头，按优先级。"""
    if r["content_ok"]:
        return ("PASS", False)
    dl = kind_deltas(r)
    only_state = dl and all(k == "GameStateInfo" and v > 0 for k, v in dl.items())
    only_death = dl and all(k in DEATH_EFFECTS for k in dl)
    state_and_death = dl and any(k in DEATH_EFFECTS for k in dl) and any(
        k == "GameStateInfo" for k in dl
    )
    multi = r["shots"] > 1

    if not r["kind_seq_ok"]:
        if only_death:
            return ("D", True)
        if multi and only_state:
            return ("S", r["loo"]["state"])
        if multi and state_and_death:
            return ("SD", False)
        if multi:
            return ("S", False)
        return ("K", not r["diffs"])

    cf = get_diffs(r, "corrected_function")
    if cf:
        rest = [d for d in r["diffs"] if d["path"].split(".")[-1] != "corrected_function"]
        return ("C", not rest)

    ex, px, py = get_diffs(r, "end_x"), get_diffs(r, "pos_x"), get_diffs(r, "pos_y")
    if ex:
        de = max((delta(d) or 0.0) for d in ex)
        px_tight = (not px) or max((delta(d) or 0.0) for d in px) <= 1e-9
        if de <= E1_MAX_DELTA and px_tight:
            return ("E1", not px and not py)
        return ("E2", not px and not py and de > E1_MAX_DELTA)
    if px:
        return ("X", not py)
    if py:
        dp = max((delta(d) or 0.0) for d in py)
        if dp <= 1e-9:
            return ("Y", True)
        return ("Y2", True)
    return ("K", False)


NAME = {
    "PASS": "完全一致",
    "C": "corrected_function 文本",
    "D": "死效块数量（死亡集合多/少）",
    "S": "多发批结构（逐发窗口未分组）",
    "SD": "多发批结构 + 死效块",
    "E1": "end_x 差一个采样点（爆炸点与我们一致）",
    "E2": "end_x 真分歧（几何）",
    "X": "爆炸点 x 末位差（pos_x ≠ end_x）",
    "Y": "爆炸点 y 末位差（求值器末位）",
    "Y2": "爆炸点 y 中等差（3 位小数常数回代）",
    "K": "其它（未定性）",
}


def delayed_stats(rows):
    """按 `run_file` 的口径复算延迟段：死亡集合 / 挖洞点。`attributed` **每份回放**重置。"""
    attributed = set()
    cur = None
    d = collections.Counter()
    for r in rows:
        if r["file"] != cur:
            attributed = set()
            cur = r["file"]
        obs = [i for i in r["obs_deaths"] if i not in attributed]
        for i in obs:
            attributed.add(i)
        ours = sorted(r["killed"])
        if obs or ours:
            d["deaths_checked"] += 1
            if ours == sorted(obs):
                d["deaths_ok"] += 1
            elif len(ours) > len(obs):
                d["deaths_extra"] += 1
            elif len(ours) < len(obs):
                d["deaths_missing"] += 1
            else:
                d["deaths_wrong"] += 1
        if r["blocked"] is not None and r["obs_destroy"]:
            d["destroy_checked"] += 1
            ours_pt = (repr(r["blocked"][0]), repr(r["blocked"][1]))
            if ours_pt in {(repr(x), repr(y)) for x, y in r["obs_destroy"]}:
                d["destroy_ok"] += 1
    return d


def pct(a, b):
    return f"{100.0 * a / b:.1f}%" if b else "-"


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", required=True)
    ap.add_argument("--check-baseline", action="store_true",
                    help="核对任务书基线（只有 fire 驱动 + 默认口径的 JSONL 才能对上）")
    args = ap.parse_args(argv)

    rows = load(args.json)
    total = len(rows)
    fails = [r for r in rows if not r["content_ok"]]
    cats = collections.Counter()
    sole = collections.Counter()
    for r in fails:
        c, s = classify(r)
        cats[c] += 1
        if s:
            sole[c] += 1

    print(f"== 语料：{len({r['file'] for r in rows})} 份有结算批的回放 / {total} 个结算批 ==")
    print(f"内容命中 {total - len(fails)} / {total} ({pct(total - len(fails), total)})"
          f"；失败 {len(fails)} 批\n")
    print(f"{'类别':<44}{'批数':>6}{'占失败':>8}{'独占':>6}")
    print("-" * 66)
    for c, n in cats.most_common():
        print(f"{NAME[c] + f' [{c}]':<44}{n:>6}{pct(n, len(fails)):>8}{sole[c]:>6}")
    print("-" * 66)
    print(f"{'合计':<44}{sum(cats.values()):>6}{'100.0%':>8}{sum(sole.values()):>6}")

    ok = sum(cats.values()) == len(fails)
    print(f"\n加总校验：{sum(cats.values())} == 失败 {len(fails)} ⇒ {'OK' if ok else '不相等！'}")

    d = delayed_stats(rows)
    print("\n== 延迟段（另一根轴，分母是有可比对象的批）==")
    print(f"死亡集合 {d['deaths_ok']} / {d['deaths_checked']} ({pct(d['deaths_ok'], d['deaths_checked'])})"
          f"  多报 {d['deaths_extra']} / 漏报 {d['deaths_missing']} / 错人 {d['deaths_wrong']}")
    print(f"挖洞点   {d['destroy_ok']} / {d['destroy_checked']}"
          f" ({pct(d['destroy_ok'], d['destroy_checked'])})")

    if args.check_baseline:
        print("\n== 对护栏基线的校验（401 份定格快照）==")
        kind_seq = sum(1 for r in rows if r["kind_seq_ok"])
        # 护栏值：**会随实现改进而变**，改这里必须同时写明"哪一轮改成什么"。
        # 2026-11-08（第三轮合并后，master 43579c07）实测：
        #   结算批 1080 ｜ 内容命中 331 ｜ 种类序列 819
        #   死亡集合 215/379 ｜ 挖洞点 **fire 轴 139/262**（batch 轴是 140/262，两者不同）
        # 历史：第一轮任务书基线是 21 / 818 / 211/379 / 1/259（当时还没修 09/10/14 那批）。
        want = {
            "结算批数": (total, 1080),
            "结算批内容命中": (total - len(fails), 331),
            "消息种类序列命中": (kind_seq, 819),
            "延迟段死亡集合命中": (d["deaths_ok"], 215),
            "延迟段死亡集合分母": (d["deaths_checked"], 379),
            "延迟段挖洞点命中": (d["destroy_ok"], 139),
            "延迟段挖洞点分母": (d["destroy_checked"], 262),
        }
        bad = 0
        for k, (got, exp) in want.items():
            same = got == exp
            bad += 0 if same else 1
            print(f"  {k:<24} 实测 {got:<6} 任务书 {exp:<6} {'OK' if same else '不一致'}")
        print("  校验结果：", "全部一致" if bad == 0 else f"{bad} 项不一致")
        ok = ok and bad == 0

    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
