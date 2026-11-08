#!/usr/bin/env python3
"""「打印-回代链」的判据台（`docs/06-verification.md` §34，`dispatch/56`）。

输入两样，**都是只读数据**：

* `--probe`：`examples/print_roundtrip.rs` 吐的逐批 JSONL（每发一排候选值，全部 `to_bits`）；
* `--attrib`：`examples/replay_attrib.rs --drive fire` 吐的逐批 JSONL（用来取**分桶**）。

判据只有一条：**逐位**（`f64::to_bits`）。差 1 ulp 也算差。分桶规则**只认**
`tools/bit_exact_attrib.py`，这里不重写。

```bash
export CARGO_HOME=… CARGO_TARGET_DIR=…
R=<快照>
cargo run --release --offline -q --example replay_attrib -- --dir $R --drive fire --policy default --json /tmp/fire.jsonl
cargo run --release --offline -q --example print_roundtrip -- --dir $R --json /tmp/probe.jsonl
python3 tools/print_roundtrip.py --probe /tmp/probe.jsonl --attrib /tmp/fire.jsonl                # Y+X
python3 tools/print_roundtrip.py --probe /tmp/probe.jsonl --attrib /tmp/fire.jsonl --buckets PASS  # 正对照
```

候选定义见 `examples/print_roundtrip.rs`（A–G）：A/B/D 是"回代"（`eval`），E/F/G 是走一遍**采样器**。

| 名 | 式子 |
|---|---|
| `A` | `eval(parse(回放 corrected_function), ±pos_x)` |
| `B` | `eval(parse(回放 corrected_function), ±end_x)` |
| `C` | `eval(全精度 c 的树, ±pos_x)` —— "不做 3 位小数打印"的对照 |
| `D` | `eval(parse(我方发出去的文本), ±pos_x)` |
| `E` | 我方采样串 → 采样器的**最后一个点** |
| `F` | 全精度 c → 采样器 |
| `G` | 回放自己的文本 → 采样器（真机链的复刻，`§26`） |
"""

import argparse
import collections
import json
import os
import struct
import sys
from decimal import Decimal, ROUND_HALF_EVEN, ROUND_DOWN, getcontext

getcontext().prec = 200  # `c` 可以大到 1e25（`- 23635515112015936427655168.000`），别让它被默认 28 位截掉

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bit_exact_attrib as BEA  # noqa: E402

BUCKET_LABEL = {
    "ok": "逐位命中", "1": "差 1 ulp", "2-4": "差 2–4 ulp",
    "more": "差 >4 ulp", "far": "差 >1e-3", "nan": "NaN 常量", "missing": "算不出",
}


# ---------------------------------------------------------------- 位工具
def bits_of(x):
    return struct.unpack("<Q", struct.pack("<d", float(x)))[0]


def f_of(b):
    return struct.unpack("<d", struct.pack("<Q", int(b)))[0]


def _ord(x):
    b = bits_of(x)
    return b if b < (1 << 63) else -(b - (1 << 63))


def ulp_between(a, b):
    return abs(_ord(a) - _ord(b))


def grade(cand_bits, obs):
    """候选值（位模式）vs 观测值（f64）—— 逐位。"""
    if cand_bits is None or obs is None:
        return "missing"
    a, b = f_of(int(cand_bits)), float(obs)
    if a != a or b != b:
        return "nan"
    if bits_of(a) == bits_of(b):
        return "ok"
    d = abs(a - b)
    if d > 1e-3:
        return "far"
    u = ulp_between(a, b)
    return "1" if u <= 1 else ("2-4" if u <= 4 else "more")


# ---------------------------------------------------------------- 打印规则
def round3_halves_even(c):
    """`format!("{:.3}", |c|)` 的规则：精确十进制四舍五入、半值取偶。"""
    return Decimal(f"{abs(c):.3f}")


def truncate3(c):
    """向零截断到 3 位（用 Decimal 精确做，不经浮点）。"""
    v = Decimal(abs(c)) * 1000
    return (v.to_integral_value(rounding=ROUND_DOWN) / Decimal(1000)).normalize()


def printed_matches(c_full, suffix):
    """`suffix`（`" + 1.197"` / `" - 3.686"`）与两种规则分别比。"""
    body = suffix.strip()
    if "NaN" in body:
        return "nan", "nan"
    if not body or body[0] not in "+-":
        return "?", "?"
    try:
        mag = Decimal(body[1:].strip())
    except Exception:
        return "?", "?"
    rnd = "ok" if mag == round3_halves_even(c_full) else "no"
    trn = "ok" if mag == truncate3(c_full) else "no"
    return rnd, trn


# ---------------------------------------------------------------- 装数据
def load(path):
    out = {}
    for line in open(path, encoding="utf-8"):
        line = line.strip()
        if not line:
            continue
        r = json.loads(line)
        if r.get("batch", -1) >= 0:
            out[(r["file"], r["batch"])] = r
    return out


def pct(a, b):
    return f"{100.0 * a / b:.1f}%" if b else "-"


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--probe", required=True)
    ap.add_argument("--attrib", required=True)
    ap.add_argument("--buckets", default="Y,X")
    ap.add_argument("--show", type=int, default=0)
    ap.add_argument("--tsv", default=None)
    args = ap.parse_args(argv)

    want = [b.strip() for b in args.buckets.split(",") if b.strip()]
    attrib = load(args.attrib)
    probe = load(args.probe)

    buckets = collections.Counter()
    for r in attrib.values():
        buckets[BEA.classify(r)[0]] += 1
    print("== 0. 分桶（`bit_exact_attrib.classify`，与 §32 同一份规则）==")
    for c, n in buckets.most_common():
        print(f"  {BEA.NAME[c][:40]:<42}[{c}]{n:>6}")
    print(f"  合计 {sum(buckets.values())} 批")

    sel = [k for k in sorted(attrib) if BEA.classify(attrib[k])[0] in want and k in probe]
    shots = [(k, i, it) for k in sel for i, it in enumerate(probe[k]["items"])]
    nrm = [(k, i, it) for k, i, it in shots if it["mode"] == "Normal"]
    ode = [(k, i, it) for k, i, it in shots if it["mode"] == "Ode"]
    print(f"\n== 目标桶 {'+'.join(want)}：{len(sel)} 批 / {len(shots)} 发"
          f"（Normal {len(nrm)} 发；Ode {len(ode)} 发）==")
    bm = collections.Counter((BEA.classify(attrib[k])[0], probe[k]["mode"]) for k in sel)
    for k, n in sorted(bm.items()):
        print(f"  {k[0]} / {k[1]:<6}{n:>5} 批")

    # ---------- ① 打印规则 ----------
    rnd, trn = collections.Counter(), collections.Counter()
    cup = collections.Counter()
    for _, _, it in nrm:
        if it["c_full"] is None or it["c_print"] is None:
            continue
        a, b = printed_matches(it["c_full"], it["suffix"])
        rnd[a] += 1
        trn[b] += 1
        cup[ulp_between(it["c_full"], it["c_print"])] += 1
    tot = sum(rnd.values())
    print(f"\n== ① 打印格式：`correction_text` 的 `{{:.3}}`（分母 {tot} 发，Ode 局没有常数）==")
    print(f"  四舍五入（半值取偶）与印出来的串一致：{rnd['ok']} / {tot} ({pct(rnd['ok'], tot)})")
    print(f"  向零截断            与印出来的串一致：{trn['ok']} / {tot} ({pct(trn['ok'], tot)})")
    print(f"  ⇒ 判别力：{tot - trn['ok']} 发上两种规则给出不同的串，其中四舍五入全中"
          f"、截断全不中 ⇒ **四舍五入**（证据档：逐位）")
    if cup:
        dmax = max(abs(it["c_print"] - it["c_full"]) for _, _, it in nrm
                   if it["c_full"] is not None and it["c_print"] is not None)
        print(f"  |c_print − c_full| 的最大值 = {dmax:.3e}（打印这一步把它压到 5e-4 的网格上）")

    # ---------- ② 回代（Normal） ----------
    print(f"\n== ② 回代：`eval(树, ±x)` 与回放 `pos_y` 逐位（分母 {len(nrm)} 发 Normal）==")
    print(f"  {'桶':<3}{'候选':<4}{'逐位命中':>9}{'差1':>6}{'2–4':>6}{'>4':>6}{'>1e-3':>7}{'NaN/缺':>7}")
    for bk in want:
        rows = [(k, i, it) for k, i, it in nrm if BEA.classify(attrib[k])[0] == bk]
        if not rows:
            continue
        for name in "ABCD":
            g = collections.Counter(grade(it[name], it["obs_pos_y"]) for _, _, it in rows)
            print(f"  {bk:<3}{name:<4}{g['ok']:>9}{g['1']:>6}{g['2-4']:>6}{g['more']:>6}"
                  f"{g['far']:>7}{g['nan'] + g['missing']:>7}")
        print()
    print("  A = 回放自己的文本 @ pos_x；B = 同串 @ end_x；C = 全精度常数；D = 我方文本 @ pos_x")
    print("  （E/F/G 是**采样末点**；报文里的爆炸点是「掩膜停点」，被挡住时末点 ≠ 停点，"
          "所以 E/F 只作「3 位小数 vs 全精度」的相对对照，别当成「我方报的值」）")
    same = sum(1 for _, _, it in nrm if it["same_text"])
    print(f"  我方发出去的串与回放的串**逐字节相同**：{same} / {len(nrm)}")
    stray = sum(1 for _, _, it in nrm if it["extra_close"])
    print(f"  其中带「无主 `)`」的发数（两个串才会不同）：{stray}")

    # ---------- ③ 与全精度 c 对照 ----------
    print("\n== ③ 「不做 3 位小数打印」会怎样（同一批、同一发）==")
    d = collections.Counter()
    pair = collections.Counter()
    for _, _, it in nrm:
        e, f = it["E"], it["F"]
        gy, fy = grade(it["G"][1] if it["G"] else None, it["obs_pos_y"]), None
        ge = grade(e[1] if e else None, it["obs_pos_y"])
        gf = grade(f[1] if f else None, it["obs_pos_y"])
        pair[(ge, gf)] += 1
        if e and f:
            du = ulp_between(f_of(e[1]), f_of(f[1]))
            d["0" if du == 0 else ("1" if du <= 1 else ("2-4" if du <= 4 else ">4"))] += 1
    print("  采样末点 y：我方 E（3 位小数） vs F（全精度）")
    for k in ("0", "1", "2-4", ">4"):
        if d[k]:
            print(f"    差 {k:>3} ulp：{d[k]:>4} 发")
    print("  两者分别对回放 pos_y 的判定（E, F）：")
    for (a, b), n in pair.most_common():
        print(f"    E={BUCKET_LABEL[a]:<10} F={BUCKET_LABEL[b]:<10}{n:>4} 发")

    # ---------- ④ 同一个树能解释回放的 y 吗 ----------
    print("\n== ④ 回放那个 `pos_y` 能不能被**同一棵树**解释（把 x 挪 ±8 ulp）==")
    g = collections.Counter()
    for bk in want:
        rows = [(k, i, it) for k, i, it in nrm if BEA.classify(attrib[k])[0] == bk]
        if not rows:
            continue
        c = collections.Counter()
        for _, _, it in rows:
            s = it["Sx"]
            if s is None:
                c["算不出"] += 1
            elif s == 0:
                c["kx=0（就是 pos_x）"] += 1
            elif abs(s) <= 3:
                c["kx=±1–3"] += 1
            else:
                c["kx=±4–8"] += 1
        for kk, nn in c.items():
            g[(bk, kk)] = nn
        line = "，".join(f"{kk} {nn}" for kk, nn in c.most_common())
        print(f"  {bk}: {line}；**±8 ulp 内全都算不出：{c.get('算不出', 0)} / {len(rows)}**")

    # ---------- ⑤ Ode ----------
    if ode:
        print(f"\n== ⑤ Ode（DiffEq）局：`corrected_function == function`，链上**没有**常数（分母 {len(ode)} 发）==")
        same = sum(1 for _, _, it in ode if it["obs_cf"] is not None and it["obs_cf"] == it["obs_fn"])
        print(f"  回放的 `corrected_function` 与原文逐字节相同：{same} / {len(ode)}"
              f" ⇒ 这 {len(ode)} 发**不存在**「打印-回代」这一步")
        c = collections.Counter()
        for _, _, it in ode:
            if it["Gx"] is None:
                c["x 不落在点列（无法复算）"] += 1
                continue
            gb = int(it["Gx"][1])
            c[BUCKET_LABEL[grade(gb, it["obs_pos_y"])]] += 1
        for k, n in c.most_common():
            print(f"    {k:<16}{n:>4} 发")

    # ---------- 明细导出 ----------
    if args.tsv:
        cols = (["file", "batch", "shot", "mode", "obs_x", "obs_y", "Sx", "same_text", "extra_close",
                 "suffix", "c_full", "c_print"] + [f"g{n}" for n in "ABCDEFG"])
        with open(args.tsv, "w", encoding="utf-8") as f:
            f.write("\t".join(cols) + "\n")
            for k, i, it in shots:
                row = {"file": k[0], "batch": k[1], "shot": i, "mode": it["mode"],
                       "obs_x": it["obs_pos_x"], "obs_y": it["obs_pos_y"], "Sx": it["Sx"],
                       "same_text": it["same_text"], "extra_close": it["extra_close"],
                       "suffix": it["suffix"], "c_full": it["c_full"], "c_print": it["c_print"]}
                for n in "ABCD":
                    row[f"g{n}"] = grade(it[n], it["obs_pos_y"])
                for n in "EFG":
                    v = it[n][1] if it[n] else None
                    row[f"g{n}"] = grade(v, it["obs_pos_y"])
                f.write("\t".join(str(row.get(c, "")) for c in cols) + "\n")
        print(f"\n逐发表：{args.tsv}")

    print("\n（真二进制那一环：`cargo run --example eval_oracle -- --json cases.jsonl` 然后 "
          "`tools/eval_oracle.py --cases cases.jsonl` —— 它用 unicorn 直调 0x4ed670 / 0x4eb9d0，"
          "逐字节比 token、逐位比求值结果。）")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
