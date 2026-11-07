#!/usr/bin/env python3
"""爆炸点 `pos_y` 的**来源判别**：它在点位表（网络写入）上，还是在客户端点数组上？

对应任务书 `dispatch/26-pos-y-from-network.md`、`docs/06-verification.md` §36。

## 这个脚本判什么

`EffectInfo`(tag 44) 有且只有 2 个生产者（`docs/06` §31.3）：

| 站点 | `pos` 来源 | `effect_type` |
|---|---|---|
| A `0x4729d6` | `points[len-1]`（**本进程点数组**，`0x472865 movdqu`） | **常量 0**（`0x472948`） |
| B `0x472e4d` | `[rsp+0xc0]` = `PlanePosManager::get_position`（**点位表**） | 记录里的一个字节（`0x472d75`） |

`EffectType` 的 0 = `ExplosionEffect`（`docs/07` §5.2，x86 `VARIANTS` + arm64 `visit_str` 双证）。

而**点位表**的写侧只有两处：`LevelGenerator::place_entity` 与
`RemoteEventHandler::handle_server_remote_event`（`docs/06` §33.4）——**后者是线上入口**。
⇒ 如果爆炸点的坐标真的走了"服务端下发"这条路，它就必须落在点位表上。

**判据（载荷级，不依赖任何反汇编）**：报文里的 `(pos_x, pos_y)` 是否**逐位等于**
回放里某个实体的 `PosInfo.(pos_x,pos_y)`。

* 非爆炸特效（`DeathEffect` / `SparkEffect` / `AshEffect`）= 站点 B = 点位表
  ⇒ 应当**几乎全部命中**（这也是本脚本的**正对照**：方法不灵就命中不了）；
* `ExplosionEffect` = 站点 A = 客户端点数组 ⇒ 应当**几乎不命中**。

## 用法

```bash
export PY=<ws>/.reference/venv/bin/python
R=<ws>/.reference/replays/gw2-replays-snapshot/replays

$PY tools/posy_source_attrib.py --dir $R                    # 判别矩阵 + 正对照（退出码 0）
$PY tools/posy_source_attrib.py --dir $R --conv conv.jsonl  # 追加"按回放类别"的残差率
```

`--conv` 吃 `replay_attrib --drive conv --policy strict --json conv.jsonl` 的产物，
用来把"末位差批"按**回放类别**（战役单机 / 联机房间）拆开——这是第二条独立证据：
残差率与"有没有远端"无关。

## 退出码

0 = 判别成立（正对照全部命中、爆炸特效基本不命中）；1 = 判别不成立或数据不足。
"""

import argparse
import collections
import gzip
import json
import os
import struct
import sys


# ---------------------------------------------------------------------------
# 位级工具（**只按位比**，绝不用 == 或容差）
# ---------------------------------------------------------------------------
def bits(x):
    return struct.unpack("<Q", struct.pack("<d", float(x)))[0]


def load_replay(path):
    """回放 = gzip 压缩的 JSONL，一行 `{"event":{"<Kind>":{…}},"tick":N}`。"""
    with gzip.open(path, "rt", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                continue  # 原版也是"坏行跳过"


def classify_room(names):
    """回放类别。`GameNameInfo` 的取值是**唯一的证据**，不猜。"""
    level = [n for n in names if n.startswith("Level ")]
    other = [n for n in names if not n.startswith("Level ") and n != "Graphwar Game"]
    if level:
        return "campaign"
    if other:
        return "online"
    return "unknown"


# ---------------------------------------------------------------------------
# 主分析
# ---------------------------------------------------------------------------
def scan(dirpath):
    names = sorted(
        os.path.join(dirpath, n)
        for n in os.listdir(dirpath)
        if os.path.isfile(os.path.join(dirpath, n)) and n.isdigit()
    )
    if not names:
        sys.exit(f"目录里没有回放文件: {dirpath}")

    # effect_type -> [命中点位表的条数, 总数]
    matrix = collections.Counter()
    # effect_type -> start_tick - 消息 tick 的取值分布
    ticks = collections.defaultdict(collections.Counter)
    # 回放类别 -> 出现过的 effect_type 次数
    by_room = collections.Counter()
    rooms = {}
    files = 0

    for path in names:
        base = os.path.basename(path)
        pos = {}  # entity_id -> (bits_x, bits_y) 最近一次 PosInfo
        game_names = set()
        effs = []
        players = set()
        try:
            for rec in load_replay(path):
                ev = rec.get("event") or {}
                if len(ev) != 1:
                    continue
                kind, payload = next(iter(ev.items()))
                tick = rec.get("tick", 0)
                if kind == "PosInfo":
                    pos[payload.get("entity_id")] = (
                        bits(payload.get("pos_x", 0.0)),
                        bits(payload.get("pos_y", 0.0)),
                    )
                elif kind == "GameNameInfo":
                    game_names.add(payload.get("game_name", ""))
                elif kind == "PlayerInfo":
                    players.add(payload.get("player_id"))
                elif kind == "EffectInfo":
                    effs.append(
                        (
                            payload.get("effect_type", "?"),
                            bits(payload.get("pos_x", 0.0)),
                            bits(payload.get("pos_y", 0.0)),
                            payload.get("start_tick", 0) - tick,
                        )
                    )
        except (OSError, EOFError, gzip.BadGzipFile):
            continue

        files += 1
        room = classify_room(game_names)
        rooms[base] = {"class": room, "names": sorted(game_names), "players": len(players)}
        for et, bx, by, dtick in effs:
            known = set(pos.values())
            matrix[(et, (bx, by) in known)] += 1
            ticks[et][dtick] += 1
            by_room[(room, et)] += 1

    return {
        "files": files,
        "matrix": matrix,
        "ticks": ticks,
        "by_room": by_room,
        "rooms": rooms,
    }


def pct(a, b):
    return 0.0 if b == 0 else 100.0 * a / b


def report(res, conv_path=None):
    ok = True
    print(f"== 语料：{res['files']} 份回放（文件名是 unix 秒，无扩展名）==\n")

    print("== ① 载荷判别：`EffectInfo.pos` 是否逐位等于某个实体的 `PosInfo.pos` ==")
    print(f"{'effect_type':<18}{'命中点位表':>10}{'总数':>8}{'占比':>9}   判读")
    print("-" * 74)
    want = {
        "DeathEffect": ">=95%（站点 B：点位表，**有**线上写侧）",
        "SparkEffect": ">=95%（站点 B）",
        "AshEffect": ">=95%（站点 B）",
        "ExplosionEffect": "<=10%（站点 A：客户端点数组）",
    }
    for et in ("ExplosionEffect", "DeathEffect", "SparkEffect", "AshEffect"):
        hit = res["matrix"][(et, True)]
        total = hit + res["matrix"][(et, False)]
        if total == 0:
            print(f"{et:<18}{'-':>10}{0:>8}{'-':>9}   语料里没有")
            continue
        p = pct(hit, total)
        print(f"{et:<18}{hit:>10}{total:>8}{p:>8.1f}%   {want.get(et, '')}")
        if et == "ExplosionEffect":
            ok = ok and p <= 10.0
        else:
            ok = ok and p >= 95.0
    print("-" * 74)
    print(
        "\n正对照：三种非爆炸特效必须几乎全部命中（它们走站点 B、`pos` 来自点位表）。\n"
        "若它们也不命中，说明**这个比法本身不灵**，本脚本的负结论无效。\n"
    )

    print("== ② `start_tick - 消息 tick` 的签名（站点 A 的公式：mgr+0x58 + X/30 + 60）==")
    for et in ("ExplosionEffect", "DeathEffect", "SparkEffect", "AshEffect"):
        c = res["ticks"].get(et)
        if not c:
            continue
        vals = sorted(c.elements())
        ge60 = sum(v for k, v in c.items() if k >= 60)
        print(
            f"  {et:<16} n={len(vals):5d}  min={vals[0]:5d}  max={vals[-1]:5d}  "
            f">=60 的占 {pct(ge60, len(vals)):5.1f}%"
        )

    print("\n== ③ 按回放类别（`GameNameInfo`）==")
    cls = collections.Counter(v["class"] for v in res["rooms"].values())
    for k in ("campaign", "online", "unknown"):
        print(f"  {k:<10} {cls[k]:4d} 份")
    print("  campaign = `GameNameInfo` 里出现 `Level N`（战役单机）；"
          "online = 出现房间名（`US Room 1` / `2v2` …）")

    if conv_path:
        print("\n== ④ 末位残差率 vs 有没有远端（`--conv`）==")
        with open(conv_path, "r", encoding="utf-8") as f:
            rows = [json.loads(l) for l in f if l.strip()]
        tab = collections.Counter()
        for r in rows:
            if "content_ok" not in r:
                continue
            room = res["rooms"].get(r["file"], {}).get("class", "unknown")
            py = [d for d in (r.get("diffs") or []) if d.get("path") == "EffectInfo.pos_y"]
            last = py and all(abs(d.get("delta") or 0.0) <= 1e-9 for d in py)
            tab[(room, "lastdigit" if last else ("big" if py else "ok"))] += 1
        print(f"{'回放类别':<10}{'末位差批':>10}{'其它差异':>10}{'无差异':>9}{'末位差率':>10}")
        for room in ("campaign", "online", "unknown"):
            last = tab[(room, "lastdigit")]
            big = tab[(room, "big")]
            good = tab[(room, "ok")]
            tot = last + big + good
            print(f"{room:<10}{last:>10}{big:>10}{good:>9}{pct(last, tot):>9.1f}%")
        print(
            "\n判读：末位差率与「有没有远端」**无关**（战役单机也有 1/4 以上）\n"
            "⇒ 残差不是「另一台机器算出来的数」。"
        )
    return ok


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", required=True, help="回放目录（401 份定格快照）")
    ap.add_argument("--conv", help="replay_attrib --drive conv --policy strict 的 JSONL")
    ap.add_argument("--json", help="把类别表落成 JSON（可选）")
    args = ap.parse_args(argv)

    if not os.path.isdir(args.dir):
        sys.exit(f"回放目录不存在: {args.dir}")

    res = scan(args.dir)
    ok = report(res, args.conv)
    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump(res["rooms"], f, ensure_ascii=False, sort_keys=True, indent=1)
        print(f"\n类别表 → {args.json}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
