#!/usr/bin/env python3
"""`Ode` 局**停止判据**普查的汇总器（`dispatch/29` 验收 ①）。

输入是 `examples/ode_stop_census.rs` 吐的逐发 JSONL（每发一行），可选再给一份
`examples/replay_attrib.rs` 的逐批 JSONL（用来把「内容命中」接上）。

```bash
export CARGO_HOME=... CARGO_TARGET_DIR=...
R=.reference/replays/gw2-replays-snapshot/replays
cargo run --release --offline -q --example ode_stop_census -- --dir $R --json $OUT/census.jsonl
cargo run --release --offline -q --example replay_attrib   -- --dir $R --drive batch --policy default --json $OUT/b.jsonl
python3 tools/ode_stop_census.py --census $OUT/census.jsonl --attrib $OUT/b.jsonl
```

## 这一张表回答的就是 ①

| 行 | 含义 |
|---|---|
| 「发 / 批 / 内容命中」 | 该模式的分母，以及（给了 `--attrib` 时）内容命中 |
| 「停点逐位相同」 | 我方停点（**掩膜口径**，= `settle::resolve_points_owned` 的 `stop`）与回放 `FunctionPoints.end_x` 在 1e-9 内相同 |
| 「走到边界」 | 我方**没有被障碍挡住**、一路采到 x 的硬边界 25，而回放停在我方点列的**中间**某点，且该点上我方 y 与回放 `pos_y` **逐位相同** ⇒ 这才是「停止判据多走了」 |
| 「前缀相同但回放更早停」 | 上一行的放宽版（不要求不被遮挡）：弹道到回放停点为止逐位相同、回放却先停。**分母最大、也最容易被误读**：里面绝大多数是「我们没识别出障碍」或「回放 `end_x` 与它自己的 `pos_x` 不一致」，不是停止判据 |
| 「回放自身 end_x 与 pos_x」 | 把回放的 `end_x` 与 `pos_x` 分别投到我方点列上；`pos=end+1` = 回放自己报的终点比它的爆炸点**退一个采样点**（`docs/06` §39 的 E1 类） |

⚠️ 采样器**不认识障碍**（`0x3c2530` 只吃函数树），所以「采样器自己的最后一点在边界」
什么都不能证明：所有「多走了」的判断都必须用**管线口径**的停点
（`ours_pipeline_stop_x` = 掩膜第一个命中点，没有命中就是采样器最后一点）。
"""

import argparse
import collections
import json


def load(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def num(v):
    return v if isinstance(v, (int, float)) else None


def _prefix_ok(s):
    """回放停点落在我方点列**中间**，且该点上我方 y 与回放 pos_y 逐位相同。"""
    i = s.get("obs_idx_in_ours", -1)
    n = s.get("npts", 0)
    if s.get("obs_end_x") is None or i < 0 or n <= 1 or i >= n - 1:
        return False
    py = num(s.get("obs_pos_y"))
    oy = num(s.get("our_y_at_idx"))
    return py is not None and oy is not None and py == oy


def stop_same(s):
    p = num(s.get("ours_pipeline_stop_x"))
    e = num(s.get("obs_end_x"))
    return p is not None and e is not None and abs(p - e) <= 1e-9


def walk_to_bound(s):
    """我方没被障碍挡、一路采到 x 的边界 25，回放却在我方点列中间停住。"""
    if not _prefix_ok(s):
        return False
    if s.get("hit_mask") is not None:
        return False
    p = num(s.get("ours_pipeline_stop_x"))
    if p is None:
        return False
    return abs(p - s["obs_end_x"]) > 0.5


def earlier_stop(s):
    """放宽版：前缀逐位相同、回放却先停（含「我们没识别出障碍」）。"""
    if not _prefix_ok(s):
        return False
    p = num(s.get("ours_pipeline_stop_x"))
    if p is None:
        return False
    return abs(p - s["obs_end_x"]) > 0.5


def pos_relation(s):
    """回放自己的 `end_x` 与 `pos_x` 在我方点列上的关系。"""
    e = s.get("obs_idx_in_ours", -1)
    p = s.get("obs_pos_idx_in_ours", -1)
    if s.get("obs_end_x") is None:
        return "无 end_x"
    if e < 0 or p < 0:
        return "落在网格外（亚步差）"
    if e == p:
        return "end == pos"
    if p == e + 1:
        return "pos = end+1（回放 end_x 退一个采样点）"
    return "其它"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--census", required=True)
    ap.add_argument("--attrib", default=None, help="replay_attrib 的逐批 JSONL（可选）")
    args = ap.parse_args()

    shots = load(args.census)
    ok = {}
    if args.attrib:
        for r in load(args.attrib):
            if "content_ok" in r:
                ok[(r["file"], r["batch"])] = r["content_ok"]

    for mode in ("Ode", "Normal"):
        sel = [s for s in shots if s.get("mode") == mode]
        batches = {(s["file"], s["batch"]) for s in sel}
        n_ok = sum(1 for k in batches if ok.get(k))
        head = f"== {mode}：{len(sel)} 发 / {len(batches)} 批"
        if ok:
            head += f"；内容命中 {n_ok} / {len(batches)}"
        print(head)
        rows = (
            ("停点逐位相同（掩膜口径 vs 回放 end_x）", stop_same),
            ("走到边界（无遮挡 / 前缀逐位相同）", walk_to_bound),
            ("前缀相同但回放更早停（含未识别障碍）", earlier_stop),
        )
        for name, fn in rows:
            hit = [s for s in sel if fn(s)]
            hb = {(s["file"], s["batch"]) for s in hit}
            print(f"   {name:36s} 发 {len(hit):4d}  批 {len(hb):4d}")
        rel = collections.Counter(pos_relation(s) for s in sel)
        print("   回放自身 end_x 与 pos_x 的关系（投到我方点列上）：")
        for k, v in rel.most_common():
            print(f"      {k:36s} 发 {v:4d}")
    print()
    print("⚠️ 只有「走到边界」那一行是停止判据的证据；"
          "「前缀相同但回放更早停」里混着「我们没识别出障碍」")
    print("   与「回放 end_x 退一个点」，不要当成停止判据的缺陷数。")


if __name__ == "__main__":
    main()
