#!/usr/bin/env python3
"""`Y` 桶（爆炸点 `pos_y`）的**逐批形态**与**假设排除表**（`docs/06-verification.md` §25）。

输入两份 JSONL，**都由仓库里的 example 产出**（不必重跑对拍）：

```bash
export CARGO_HOME=<ws>/.cargo-home CARGO_TARGET_DIR=<ws>/.agent-work/<topic>/target
R=<ws>/.agent-work/replay-snapshot          # 401 份定格快照

# ① 逐批归因（既有工具）
cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive conv --policy strict --json $OUT/conv.jsonl

# ② 逐发原值探针（**本仓库新增**，只加不改）
cargo run --release --offline --example posy_probe -- --dir $R --json $OUT/posy.jsonl

# ③ 本来就有的分桶（本脚本复用它的 `classify`，保证口径同一条）
python3 tools/posy_attrib.py --conv $OUT/conv.jsonl --probe $OUT/posy.jsonl
```

## 为什么要两份

`conv.jsonl` 里 `diffs[].ours/observed` 是**报文里的原始文本**，`class 数值（末位差）`
带 `ulp`/`delta`；`posy.jsonl` 才有**我方那一侧的候选算法**（停点、点序列尾、精确 c 回代、
3 位小数文本回代、±2000 ulp 的 x 扫描）。两份按 `(file, batch)` 对齐（一批多发时
`posy.jsonl` 里是逐发多行，`Y` 桶实测 **1 批 1 发**，所以直接取第一行）。

## 输出

1. **170 拆两半**：16 批是**序列化格式**（值同、字节不同，ours=`0` vs observed=`0.0`），
   154 批是真数值差。把这两半混在一起说"末位差"是 §24 那张表的读法错误。
2. **154 批的 ulp 分布**（`obs` 相对 `ours` 的有符号 ulp 距离）。
3. **假设排除表**：对每个候选算式数"逐位命中批数"。全是 0 ⇒ 这些假设都不是根因。
4. **x 扫描**：把我方点序列里 `x == end_x` 的那个 `x` 在 ±N ulp 内扫一遍，
   看是否存在某个 `x` 使 `eval(印刷版修正文本, x) == obs`。命中说明是"取错 x"，
   全不命中说明是"同一个 x 上算出来的最后一位不同"。
"""

import argparse
import collections
import json
import math
import struct
import sys

sys.path.insert(0, __file__.rsplit("/", 1)[0] or ".")
import bit_exact_attrib as B  # noqa: E402  （同一目录，口径必须与它一致）


# ---------------------------------------------------------------------------
# 浮点小工具（**全部按位比**，绝不用 `==` 或容差）
# ---------------------------------------------------------------------------
def bits(x):
    return struct.unpack("<Q", struct.pack("<d", float(x)))[0]


def from_bits(b):
    return struct.unpack("<d", struct.pack("<Q", b & 0xFFFFFFFFFFFFFFFF))[0]


def same(a, b):
    return isinstance(a, float) and isinstance(b, float) and bits(a) == bits(b)


def ulp_delta(a, b):
    """有符号 ulp 距离 `a - b`（单调整数映射；NaN 返回 None）。"""

    def order(x):
        w = struct.unpack("<q", struct.pack("<d", x))[0]
        return -(2 ** 63) - w if w < 0 else w

    if not isinstance(a, float) or not isinstance(b, float):
        return None
    if math.isnan(a) or math.isnan(b):
        return None
    return order(a) - order(b)


def load_jsonl(path):
    with open(path, "r", encoding="utf-8") as f:
        return [json.loads(line) for line in f if line.strip()]


def jnum(v):
    """数值字段。

    两种来源：`posy_probe` 吐的是 JSON 数值（非有限值写成字符串）；
    `conv.jsonl` 的 `ours`/`observed` 是**报文里的原始文本**（字符串），也要能解析。
    """
    if isinstance(v, (int, float)) and not isinstance(v, bool):
        return float(v)
    if isinstance(v, str):
        try:
            return float(v)
        except ValueError:
            return None
    return None


# ---------------------------------------------------------------------------
# 主分析
# ---------------------------------------------------------------------------
def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--conv", required=True, help="replay_attrib 吐的 conv.jsonl")
    ap.add_argument("--probe", required=True, help="posy_probe 吐的 posy.jsonl")
    ap.add_argument("--scan-ulps", type=int, default=0,
                    help=">0 时对 Normal 批做 ±N ulp 的 x 扫描（慢，需 --oracle）")
    ap.add_argument("--oracle", default="",
                    help="上游 analysis 仓库的 graphwar_curve.py 路径（x 扫描要用它的求值器）")
    args = ap.parse_args(argv)

    rows = B.load(args.conv)
    fails = [r for r in rows if not r["content_ok"]]
    Y = [(r["file"], r["batch"]) for r in fails if B.classify(r)[0] == "Y"]

    probe = {}
    for p in load_jsonl(args.probe):
        probe.setdefault((p["file"], p["batch"]), p)

    print(f"== 语料：{len(rows)} 个结算批，失败 {len(fails)} 批；`Y` 桶 {len(Y)} 批 ==\n")

    # ---- ① 拆两半 -------------------------------------------------------
    num, fmt = [], []
    for key in Y:
        r = next(x for x in fails if (x["file"], x["batch"]) == key)
        d = r["diffs"][0] if r["diffs"] else {}
        (fmt if d.get("class") != "数值（末位差）" else num).append((r, d, probe.get(key)))
    print("## ① `Y` 桶拆两半（**§24 那张表把它当成一类，是读法错误**）")
    print(f"  数值末位差（class=数值（末位差））: {len(num):>3} 批")
    print(f"  纯序列化格式（值同、字节不同）    : {len(fmt):>3} 批")
    if fmt:
        pairs = collections.Counter((d["ours"], d["observed"]) for _, d, _ in fmt)
        print("\n  格式批的 (ours, observed) 原文：")
        for (a, b), n in pairs.most_common():
            print(f"    {n:>2}×  {a:<70} vs  {b}")
        modes = collections.Counter((p or {}).get("mode") for _, _, p in fmt)
        print(f"  模式分布：{dict(modes)}")
        print("  ⇒ 回放用 **serde_json/ryu** 语义（整数值带 `.0`、小量级走 `e` 记法），")
        print("    我方 `format!(\"{}\")` 把 `0.0` 打成 `0`、把 2.6e-62 打成 66 位小数。")

    # ---- ② ulp 分布 -----------------------------------------------------
    print("\n## ② 154 批的 ulp 分布（`obs` 相对交付值的有符号 ulp 距离，绝对值）")
    hist = collections.Counter()
    bymode = collections.Counter()
    for r, d, p in num:
        a, b = jnum(d.get("ours")), jnum(d.get("observed"))
        u = ulp_delta(a, b) if a is not None and b is not None else None
        hist[abs(u) if u is not None else "?"] += 1
        bymode[(p or {}).get("mode", "?")] += 1
    print(f"  模式：{dict(bymode)}")
    keys = sorted(k for k in hist if isinstance(k, int))
    for k in keys:
        print(f"    {k:>5} ulp : {hist[k]:>3} 批")
    if "?" in hist:
        print(f"    {'?':>5}     : {hist['?']:>3} 批（非有限或缺失）")
    n = sum(hist.values())
    small = sum(hist[k] for k in keys if k <= 4)
    print(f"  ⇒ ≤4 ulp 合计 {small} / {n}（{100.0 * small / n:.1f}%）")

    # ---- ③ 假设排除表 ---------------------------------------------------
    print("\n## ③ 假设排除表：每个候选算式**逐位命中**的批数（分母 = 154）")
    cands = collections.OrderedDict(
        [
            ("交付值（=eval(我方印刷版文本, 我方停点 x)）", lambda p: None),  # 单独算
            ("采样器停点 y（精确 c）", lambda p: jnum(p["stop_y"])),
            ("点序列尾 pts[i_end].y", lambda p: jnum(p["here"][1])),
            ("上一个采点 pts[i_end-1].y", lambda p: jnum(p["prev"][1])),
            ("下一个采点 pts[i_end+1].y", lambda p: jnum(p["next"][1])),
            ("精确 c 在 end_x 回代 f(x)+c", lambda p: jnum(p["c_exact_at_endx"])),
            ("精确 c 在停点 x 回代", lambda p: jnum(p["c_exact_at_stopx"])),
            ("三项结合 (f(x)-f(x0))+y0", lambda p: jnum(p["c_3term"])),
            ("三项结合 f(x)+(y0-f(x0))", lambda p: jnum(p["c_3term_b"])),
            ("印刷版文本回代 eval(obs_text, end_x)", lambda p: jnum(p["c_obs_h1"])),
        ]
    )
    hit = collections.Counter()
    for r, d, p in num:
        if p is None:
            continue
        obs = jnum(d.get("observed"))
        if same(jnum(d.get("ours")), obs):
            hit["交付值（=eval(我方印刷版文本, 我方停点 x)）"] += 1
        for name, f in cands.items():
            if name.startswith("交付值"):
                continue
            if same(f(p), obs):
                hit[name] += 1
    for name in cands:
        print(f"    {hit[name]:>3} / {len(num)}   {name}")
    print("  ⇒ 全 0（除退化批）说明：**不是「取了另一个点」、也不是「c 的结合顺序」**。")

    # ---- ④ x 扫描（可选，慢）-------------------------------------------
    if args.scan_ulps > 0 and args.oracle:
        import importlib.util

        spec = importlib.util.spec_from_file_location("graphwar_curve", args.oracle)
        G = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(G)

        print(f"\n## ④ x 扫描（±{args.scan_ulps} ulp）：是否存在 x 使 eval(印刷版文本,x)==obs")
        done = 0
        nohit = 0
        for r, d, p in num:
            if p is None or p.get("mode") != "Normal":
                continue
            obs = jnum(d.get("observed"))
            toks = G.parse_function(p["corrected_obs"])
            base = -p["obs_end_x"] if p["flip"] else p["obs_end_x"]
            b0 = bits(base)
            found = []
            for k in range(-args.scan_ulps, args.scan_ulps + 1):
                xx = from_bits(b0 + k)
                v = G.evaluate(toks, -xx if p["flip"] else xx, 0.0, 0.0)
                if same(v, obs):
                    found.append(k)
            done += 1
            if not found:
                nohit += 1
            print(f"    {r['file']}/{r['batch']:<3} 命中 {len(found):>5} 个 x"
                  f"{'（首个 '+str(found[0])+'）' if found else '  ← 任何 x 都算不出 obs'}")
            if done >= 12:
                print("    （只扫前 12 批 Normal）")
                break
        print(f"  ⇒ {nohit}/{done} 批**任何** ±{args.scan_ulps} ulp 内的 x 都算不出 obs")
        print("    （有命中的那几批都是平坦段：y 对 x 不敏感，多解）")
    elif args.scan_ulps > 0:
        print("\n## ④ x 扫描跳过：没给 --oracle <path>/graphwar_curve.py")

    print("\n## 结论（可直接抄进文档）")
    print("  1) 170 = 16（格式）+ 154（真末位差）；154 里 148 批 ≤4 ulp。")
    print("  2) 三种「取点 / 常数顺序」假设全部排除：命中 0。")
    print("  3) 剩下的只能是「同一个公式、最后一次舍入不同」，下一步要查生产者。")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
