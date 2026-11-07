#!/usr/bin/env python3
"""爆炸点 `pos` 与 `end_x` 的对齐归因（`docs/06-verification.md` §39 那张表）。

为什么要有它：`Y`（爆炸点 y 末位差）长年被当成**一个**机制，而实际要按
「这条弹道的 y 是怎么来的」先二分 —— `Normal` 局 `y = f(x)`，`DiffEq` 局 `y` 来自 RK4
积分。两份 JSONL 各有一半答案，**谁也不知道对方**：

* `replay_attrib --json f.jsonl`   → 逐批的 `diffs`（我方 vs 回放）+ `tools/bit_exact_attrib.py` 的分桶；
* `posy_mech     --json mech.jsonl` → 逐发的模式、`end_x`、`pos_x/pos_y`、以及
  「三个候选 `x` 上 `eval(回放文本, x)`」与 `pos_y` 的有符号 ulp 差。

本脚本按 `(file, batch)` 把两份对齐（`Y` 桶实测 1 批 1 发，直接取第一条），
输出 §39.3 的那张表。

```bash
cargo run --release --offline --example replay_attrib -- --dir $R --drive fire --policy default --json f.jsonl
cargo run --release --offline --example posy_mech     -- --dir $R --json mech.jsonl
$PY tools/explpos_attrib.py --attrib f.jsonl --mech mech.jsonl
```

退出码 0 = 两份文件都读到了；表里的数就是结论，脚本不做判断。
"""

import argparse
import collections
import json
import sys

sys.path.insert(0, __file__.rsplit("/", 1)[0] or ".")
import bit_exact_attrib as B  # noqa: E402  分桶口径必须与它一致


def load_mech(path):
    out = {}
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            r = json.loads(line)
            out.setdefault((r["file"], r["batch"]), r)
    return out


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--attrib", required=True, help="replay_attrib 吐的 JSONL")
    ap.add_argument("--mech", required=True, help="posy_mech 吐的 JSONL")
    a = ap.parse_args(argv)

    rows = B.load(a.attrib)
    mech = load_mech(a.mech)
    print(f"批 {len(rows)} / 发 {len(mech)}")

    # 1) 每个桶按模式二分，以及「拿回放自己的 pos_x 回代」的命中数
    per = collections.defaultdict(lambda: collections.Counter())
    hit = collections.defaultdict(collections.Counter)
    for r in rows:
        cls, _ = B.classify(r)
        if cls == "PASS":
            continue
        m = mech.get((r["file"], r["batch"]))
        if not m:
            continue
        per[cls][m["mode"]] += 1
        if m["v2"]["self"] == 0:
            hit[cls][m["mode"]] += 1
    print("\n桶 × 模式（批数 / 其中 eval(回放文本,回放 pos_x) 逐位命中）")
    for cls in sorted(per):
        cells = "  ".join(
            f"{mode} {per[cls][mode]}(hit {hit[cls][mode]})" for mode in sorted(per[cls])
        )
        print(f"  {cls:3s} {cells}")

    # 2) Y 桶的「回放自己不自洽」直方图（v2.self 的有符号 ulp 差）
    yb = [
        mech[(r["file"], r["batch"])]
        for r in rows
        if B.classify(r)[0] == "Y" and (r["file"], r["batch"]) in mech
    ]
    for mode in ("Normal", "DiffEq"):
        sel = [m for m in yb if m["mode"] == mode]
        if not sel:
            continue
        c = collections.Counter(m["v2"]["self"] for m in sel)
        print(f"\nY · {mode} n={len(sel)}  v2.self(obs 自洽性) 直方图: {c.most_common(6)}")

    # 3) 越界停占比（§39.6b）：|pos_x|>24.9 或 |pos_y|>14.9
    def boundary(m):
        return abs(m["obs_x"]) > 24.9 or abs(m["obs_y"]) > 14.9

    print("\n越界停占比（基线 = 同一模式全部发）")
    for cls in ("Y", "X"):
        for mode in ("Normal", "DiffEq"):
            sel = [
                mech[(r["file"], r["batch"])]
                for r in rows
                if B.classify(r)[0] == cls
                and (r["file"], r["batch"]) in mech
                and mech[(r["file"], r["batch"])]["mode"] == mode
            ]
            if not sel:
                continue
            n = sum(1 for m in sel if boundary(m))
            print(f"  {cls} · {mode:7s} {n:3d}/{len(sel):3d} = {100 * n / len(sel):.0f}%")
    for mode in ("Normal", "DiffEq"):
        sel = [m for m in mech.values() if m["mode"] == mode]
        if not sel:
            continue
        n = sum(1 for m in sel if boundary(m))
        print(f"  ALL · {mode:7s} {n:3d}/{len(sel):3d} = {100 * n / len(sel):.0f}%")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
