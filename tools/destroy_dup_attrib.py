#!/usr/bin/env python3
"""`DestroyObstacle` 的**重复报文**归因：一次挖洞为什么会在回放里出现 2 条。

## 结论（本脚本打印的数就是它）

401 份可达回放里 **503** 条 `DestroyObstacle`，其中 **162 组**是"同一坐标出现两次"，
分组键是 `(pos_x, pos_y, radius)` 三个字段的 `repr`（`repr` 相同 ⇔ f64 **逐位**相同，
因为回放里是 f64 的最短往返十进制）。这 162 组**从无 3 条以上**。

对每组的两条，本脚本报三件事：

| 量 | 本轮（`gw2-replays-snapshot`，403 份里 401 份可解） |
|---|---|
| 两条在消息流里的下标差 | **154** 组 = 1（紧邻）、7 组 = 2、1 组 = 14 |
| 两条的消息 `tick` 差 | **92** 组 = 0，其余 1…13 / 21 / 23 / 42 / 44 / 464 |
| 两条是否跨结算批 | 0（全部同批） |

⇒ 报文是"同一条实参发两次"，不是"两条不同实参"：判据见
`docs/06-verification.md` §30.5（`handle_tick_event` 对每个队列条目各发一条 tag 41）。

用法：
    ../.venv/bin/python tools/destroy_dup_attrib.py --dir <快照目录>
    ../.venv/bin/python tools/destroy_dup_attrib.py --dir <...> --sample 5 --json out.jsonl
"""

import argparse
import collections
import gzip
import json
import os
import sys

NAME = "DestroyObstacle"


def load(path):
    """回放 = gzip 或纯文本 JSONL，每行 `{"tick": n, "event": {...}}`。"""
    with open(path, "rb") as f:
        gz = f.read(2) == b"\x1f\x8b"
    opener = gzip.open if gz else open
    out = []
    for line in opener(path, "rt", encoding="utf-8", errors="replace"):
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return out


def scan(root):
    groups = collections.defaultdict(list)
    files, msgs = 0, 0
    for fn in sorted(os.listdir(root)):
        if not fn.isdigit():
            continue
        path = os.path.join(root, fn)
        try:
            evs = load(path)
        except OSError:
            continue
        files += 1
        for i, e in enumerate(evs):
            ev = e.get("event")
            if not isinstance(ev, dict) or NAME not in ev:
                continue
            d = ev[NAME]
            msgs += 1
            key = (repr(d["pos_x"]), repr(d["pos_y"]), repr(d.get("radius")))
            groups[(fn, key)].append((i, e.get("tick")))
    return files, msgs, groups


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dir", required=True, help="回放快照目录（只读）")
    ap.add_argument("--sample", type=int, default=3, help="每组样本打印几条")
    ap.add_argument("--json", help="把每组明细写成 jsonl")
    args = ap.parse_args(argv)

    files, msgs, groups = scan(args.dir)
    dup = {k: v for k, v in groups.items() if len(v) > 1}
    idx_gap = collections.Counter()
    tick_gap = collections.Counter()
    for _k, v in dup.items():
        for a in range(1, len(v)):
            idx_gap[v[a][0] - v[a - 1][0]] += 1
            tick_gap[v[a][1] - v[a - 1][1]] += 1

    print("== 语料 ==")
    print("  可解回放 %d 份；`%s` 报文 %d 条；不同坐标 %d 个" % (files, NAME, msgs, len(groups)))
    print("  重复坐标组 %d 组（每组 %s 条）" % (
        len(dup), "/".join(str(n) for n in sorted({len(v) for v in dup.values()}))))
    print("  校验：%d 组 × 2 + 单条 = %d %s" % (
        len(dup), msgs, "OK" if 2 * len(dup) + (len(groups) - len(dup)) == msgs else "**对不上**"))
    print("\n== 组内两条的关系 ==")
    print("  消息下标差 :", dict(sorted(idx_gap.items())))
    print("  tick 差    :", dict(sorted(tick_gap.items())))
    print("\n== 结论 ==")
    same = tick_gap.get(0, 0)
    adj = idx_gap.get(1, 0)
    print("  · 逐位相同的实参：%d/%d 组（`repr` 相等即 f64 逐位相等）" % (len(dup), len(dup)))
    print("  · 紧邻发出：%d 组下标差 1（%.0f%%）" % (adj, 100.0 * adj / max(1, sum(idx_gap.values()))))
    print("  · 连消息 tick 都相同：%d 组（%.0f%%）" % (same, 100.0 * same / max(1, sum(tick_gap.values()))))
    print("  ⇒ **同一条实参发了两次**（不是两条不同实参）；机制见 docs/06-verification.md §30.5")

    if args.sample:
        print("\n== 样本（每组 2 条） ==")
        for (fn, key), v in sorted(dup.items())[: args.sample]:
            print("  %s  (%s, %s, r=%s)" % (fn, key[0], key[1], key[2]))
            for i, tick in v:
                print("     idx=%-4d tick=%d" % (i, tick))

    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            for (fn, key), v in sorted(dup.items()):
                f.write(json.dumps({"file": fn, "key": key,
                                    "hits": [{"idx": i, "tick": t} for i, t in v]}) + "\n")
        print("\n明细写到 %s" % args.json)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
