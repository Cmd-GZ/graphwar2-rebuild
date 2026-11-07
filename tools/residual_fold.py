#!/usr/bin/env python3
"""**「回放自身不自洽」判决台**（`dispatch/35`）：把 `examples/residual_fold.rs` 的逐发复算
与 `examples/replay_attrib.rs` 的逐批归因表 `join` 起来，出两条轴（`Normal` / `DiffEq`）的
「命中 / 不自洽 / 其它」三栏，并跑**正对照**。

```bash
R=.reference/replays/gw2-replays-snapshot/replays
cargo run --release --offline -q --example replay_attrib -- \\
    --dir $R --drive batch --policy default --json /tmp/b.jsonl
cargo run --release --offline -q --example residual_fold -- --dir $R --json /tmp/fold.jsonl
python3 tools/residual_fold.py --fold /tmp/fold.jsonl --attrib /tmp/b.jsonl
```

## 判据（**写死**，不许在报告里改口径）

逐发、**逐位**（`to_bits`）：

| 模式 | 「复算」是什么 | 命中 | 不自洽 | 其它 |
|---|---|---|---|---|
| `Normal` | `eval(parse(corrected_function), flip ? -pos_x : pos_x)` | 与 `pos_y` 逐位相同 | 逐位不同 | 没有爆炸点 / `corrected_function` 缺失 / 不是原文前缀 / `pos_y` 是 NaN |
| `DiffEq` | 内核同一个采样器 `calculate(text, x0, y0, 0.01, flip, Ode)` 积分出的点列里找 `x == pos_x` 的那点 | 找得到且 `y` 逐位相同 | 找得到但 `y` 不同 | **找不到**（回放的点不落在我们积分出的网格上 ⇒ 分不清是我方积分先跑偏还是回放点不对） |

三条输入（文本 / 常数 / `pos_x`）**全部来自回放报文**（`FunctionInfo.function`、
`FunctionInfo.corrected_function` 的后缀、`EffectInfo(Explosion).pos_x`），
所以「我方实现对不对」与这条判据**无关**：它只问回放自身是否闭合。

## 正对照（说"不自洽"之前必须先有的那条）

1. **内容命中的批**（`replay_attrib` 的 `content_ok`，647 批）里抽 N 批：复算必须给出
   **命中**。链路若坏了，这一栏会掉下来 —— 那样"全都不自洽"就只是工具坏了。
2. **纯回放内部**的一条独立判据（不需要任何实现参与）：`FunctionPoints.end_x` 与
   `EffectInfo(Explosion).pos_x` 按 `docs/06` §27/§43 都是 `pts[len-1].x` ⇒ 必须逐位相等。
   这一项单独统计，用来把"回放自己两个字段打架"与"文本复算不出来"分开。
3. `--oracle N` 用**真二进制自己的求值器**（`0x4ed670` 解析 + `0x4eb9d0` 求值，
   unicorn，复用既有 `tools/eval_oracle.py`）在同一个 `x` 上再算一遍，
   证明"复算不出来"不是我们求值器的问题。
"""

import argparse
import collections
import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def _load_attrib():
    p = os.path.join(HERE, "bit_exact_attrib.py")
    spec = importlib.util.spec_from_file_location("bit_exact_attrib", p)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


BEA = _load_attrib()
RESIDUAL_BUCKETS = ("Y", "E1", "X")


def load_jsonl(path):
    rows = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def bits_of(v):
    """JSON 里的数值 → 位串（用 python 的 float 作载体；`NaN`/`inf` 已被工具写成字符串）。"""
    if isinstance(v, str):
        return None
    return float(v).hex()


def pos_is_start(s):
    """回放的 `pos` 是不是**炮口点**（`pts[0]`）—— 数组退化到只剩起点时 `pos == (x0, y0)`。

    声明的那个 `y` 是**士兵站位**的 `y0`，不是任何一次采样求值的结果（`pts[0]` 是
    `(x0, y0)` 直接写进去的，见 `graphwar_path_calculator::calculate` 第一行），
    所以这一档**不构成对 y 的判据**，必须落到「其它」。
    """
    if isinstance(s["pos_x"], str) or isinstance(s["pos_y"], str):
        return False
    return (bits_of(s["pos_x"]) == bits_of(s["x0"])
            and bits_of(s["pos_y"]) == bits_of(s["y0"]))


def shot_verdict(s):
    """(verdict, reason) —— 见模块文档那张表。"""
    if not s["has_expl"]:
        return "其它", "no_explosion"
    if isinstance(s["pos_y"], str):
        return "其它", "nan_pos_y"
    if pos_is_start(s):
        return "其它", "pos_is_start(muzzle)"
    if s["mode"] == "Normal":
        if not s["split_ok"]:
            return "其它", "cf_missing_or_not_prefix"
        return ("命中", "") if s["self_hit"] else ("不自洽", "eval_at_pos_x")
    # DiffEq
    if s["ode_found"]:
        return ("命中", "") if s["ode_hit"] else ("不自洽", "y_not_on_curve")
    return "其它", "pos_x_not_on_our_grid"


def batch_verdict(shots):
    """批级：只要有 1 发「不自洽」就是不自洽；否则有「其它」就是其它；否则命中。"""
    vs = [shot_verdict(s)[0] for s in shots]
    if "不自洽" in vs:
        return "不自洽"
    if "其它" in vs:
        return "其它"
    return "命中"


def is_nan(v):
    return isinstance(v, str) and v == "NaN"


def main(argv):
    ap = argparse.ArgumentParser(description="回放自身不自洽：两条轴判决台")
    ap.add_argument("--fold", required=True, help="examples/residual_fold.rs 的逐发 JSONL")
    ap.add_argument("--attrib", required=True, help="examples/replay_attrib.rs 的逐批 JSONL")
    ap.add_argument("--positive-control", type=int, default=20, help="正对照抽多少批（内容命中里）")
    ap.add_argument("--table", help="逐批表（TSV）写到这个路径")
    ap.add_argument("--show", type=int, default=0, help="顺带打印多少行逐批明细")
    ap.add_argument("--oracle", type=int, default=0,
                    help="用真二进制求值器（unicorn）再算 N 发（0 = 不跑）")
    args = ap.parse_args(argv)

    shots = load_jsonl(args.fold)
    brows = [r for r in load_jsonl(args.attrib) if r.get("batch", -1) >= 0]
    brow = {(r["file"], r["batch"]): r for r in brows}

    by_batch = collections.OrderedDict()
    for s in shots:
        by_batch.setdefault((s["file"], s["batch"]), []).append(s)

    # ---- 0. 分母核对 ----
    only_fold = [k for k in by_batch if k not in brow]
    only_attrib = [k for k in brow if k not in by_batch]
    print("== 分母 ==")
    print(f"逐发 {len(shots)}（Normal {sum(1 for s in shots if s['mode']=='Normal')} / "
          f"DiffEq {sum(1 for s in shots if s['mode']=='DiffEq')}）"
          f"；结算批 {len(brow)}；两边都有的批 {len(set(by_batch) & set(brow))}")
    if only_fold:
        print(f"  ⚠️ 只在逐发表里（{len(only_fold)}）：{only_fold[:3]}")
    if only_attrib:
        print(f"  ⚠️ 只在归因表里（{len(only_attrib)}）：{only_attrib[:3]}")

    # ---- 1. 失败批与残差 407 ----
    fails = [r for r in brows if not r["content_ok"]]
    buckets = collections.defaultdict(list)
    for r in fails:
        c, _ = BEA.classify(r)
        buckets[c].append((r["file"], r["batch"]))
    resid = [k for b in RESIDUAL_BUCKETS for k in buckets[b]]
    print(f"\n== 残差集合 ==")
    print(f"内容命中 {len(brows)-len(fails)} / {len(brows)}；失败 {len(fails)}；"
          f"Y+E1+X = {len(buckets['Y'])}+{len(buckets['E1'])}+{len(buckets['X'])} = {len(resid)}")
    other_buckets = {c: len(v) for c, v in buckets.items() if c not in RESIDUAL_BUCKETS}
    print(f"  407 之外还有：{other_buckets}（合计 {sum(other_buckets.values())}）")
    assert len(resid) == len(fails) - sum(other_buckets.values())

    # ---- 2. 正对照 ----
    print(f"\n== 正对照 ①：内容命中的批里抽 {args.positive_control} 批 ==")
    ok_batches = [r for r in brows if r["content_ok"]]
    ok_batches.sort(key=lambda r: (r["file"], r["batch"]))
    step = max(1, len(ok_batches) // max(1, args.positive_control))
    sample = ok_batches[::step][: args.positive_control]
    pc_hit = pc_all = 0
    pc_rows = []
    for r in sample:
        ss = by_batch.get((r["file"], r["batch"]), [])
        if not ss:
            continue
        vs = collections.Counter(shot_verdict(s)[0] for s in ss)
        pc_hit += vs["命中"]
        pc_all += len(ss)
        pc_rows.append((r["file"], r["batch"], r["mode"], len(ss), dict(vs)))
    print(f"  抽到 {len(pc_rows)} 批 / {pc_all} 发；其中复算「命中」{pc_hit} 发")
    print(f"  ⇒ 命中率 {pct(pc_hit, pc_all)}（链路能得出「自洽」才算正对照通过）")
    for f, b, m, n, vs in pc_rows[:12]:
        print(f"    {f} #{b} {m:<7} {n} 发 → {vs}")
    # 加做**全量**正对照（647 批全跑，不只抽 20）
    full = collections.Counter()
    ok_set0 = {(r["file"], r["batch"]) for r in ok_batches}
    for s in shots:
        if (s["file"], s["batch"]) not in ok_set0:
            continue
        full[shot_verdict(s)[0]] += 1
    nfull = sum(full.values())
    print(f"  加做**全量**正对照（全部 {len(ok_batches)} 个内容命中的批）：{nfull} 发 → "
          f"命中 {full['命中']} / 不自洽 {full['不自洽']} / 其它 {full['其它']}"
          f"（命中率 {pct(full['命中'], nfull)}）")
    print(f"  ⚠️ 「其它」在这 647 批里是 `pos_is_start`（数组退化到只剩炮口点，`pos=(x0,y0)`）")
    print(f"  另：DiffEq 批里**内容命中 {sum(1 for r in brows if r['mode']=='DiffEq' and r['content_ok'])}"
          f" / {sum(1 for r in brows if r['mode']=='DiffEq')}** —— 这是整条 Ode 链（含 RK4）逐位对的底数")

    # ---- 3. 407 的两条轴 ----
    print(f"\n== 407 批：Normal / DiffEq × 命中 / 不自洽 / 其它 ==")
    tbl = collections.defaultdict(collections.Counter)
    shot_tbl = collections.defaultdict(collections.Counter)
    detail = []
    for k in resid:
        r = brow[k]
        ss = by_batch.get(k, [])
        mode = r["mode"]
        v = batch_verdict(ss) if ss else "其它"
        tbl[mode][v] += 1
        for s in ss:
            shot_tbl[mode][shot_verdict(s)[0]] += 1
        c, _ = BEA.classify(r)
        detail.append((k, mode, c, v, ss))
    tot = collections.Counter()
    print(f"{'轴':<8}{'命中':>8}{'不自洽':>10}{'其它':>8}{'合计':>8}   （**批**）")
    for mode in ("Normal", "DiffEq"):
        c = tbl[mode]
        n = sum(c.values())
        tot.update(c)
        print(f"{mode:<8}{c['命中']:>8}{c['不自洽']:>10}{c['其它']:>8}{n:>8}")
    print(f"{'合计':<8}{tot['命中']:>8}{tot['不自洽']:>10}{tot['其它']:>8}{sum(tot.values()):>8}")
    print(f"\n同一口径按**发**：")
    print(f"{'轴':<8}{'命中':>8}{'不自洽':>10}{'其它':>8}{'合计':>8}")
    for mode in ("Normal", "DiffEq"):
        c = shot_tbl[mode]
        print(f"{mode:<8}{c['命中']:>8}{c['不自洽']:>10}{c['其它']:>8}{sum(c.values()):>8}")
    assert sum(tot.values()) == 407, f"两条轴加起来 {sum(tot.values())} != 407"

    # 轴 × 桶 交叉
    print(f"\n== 407 批：轴 × 归因桶 ==")
    cross = collections.defaultdict(collections.Counter)
    for k, mode, c, v, ss in detail:
        cross[mode][c] += 1
    for mode in ("Normal", "DiffEq"):
        print(f"  {mode:<8}" + "  ".join(f"{c}={cross[mode][c]}" for c in RESIDUAL_BUCKETS)
              + f"  合计={sum(cross[mode].values())}")

    # ---- 4. 纯回放内部判据 + 机制候选 ----
    print(f"\n== 交叉证据（全部只由回放报文决定，与我方实现无关）==")
    print(f"{'桶':<5}{'批':>6}{'发':>6}{'pos_x==end_x':>14}{'pos_y==f(end_x)':>18}"
          f"{'pos_y==f+全精度c':>18}")
    for c in RESIDUAL_BUCKETS:
        ks = [k for k, mode, cc, v, ss in detail if cc == c]
        ss = [s for k in ks for s in by_batch.get(k, [])]
        if not ss:
            continue
        eq = sum(1 for s in ss if s["pos_eq_endx"])
        eh = sum(1 for s in ss if s["endx_hit"])
        fh = sum(1 for s in ss if s["fullc_hit"])
        print(f"{c:<5}{len(ks):>6}{len(ss):>6}{eq:>10}/{len(ss):<3}{eh:>14}/{len(ss):<3}"
              f"{fh:>14}/{len(ss):<3}")
    # 全部 1080 批的同两项
    all_shots = shots
    print(f"{'全部':<5}{len(brows):>6}{len(all_shots):>6}"
          f"{sum(1 for s in all_shots if s['pos_eq_endx']):>10}/{len(all_shots):<3}"
          f"{sum(1 for s in all_shots if s['endx_hit']):>14}/{len(all_shots):<3}"
          f"{sum(1 for s in all_shots if s['fullc_hit']):>14}/{len(all_shots):<3}")
    # DiffEq 侧的 endx 归属（`ode_endx_hit`）
    ode = [s for s in all_shots if s["mode"] == "DiffEq"]
    ode_resid = [s for s in ode if (s["file"], s["batch"]) in set(resid)]
    print(f"\nDiffEq 发 {len(ode)}；407 里 {len(ode_resid)} 发；"
          f"其中 `ode_endx_hit`（回放 pos_y 落在**它自己的 end_x** 那点上）"
          f"{sum(1 for s in ode_resid if s['ode_endx_hit'])}/{len(ode_resid)}")

    # ---- 4b. 两条内部矛盾轴的并集 ----
    print(f"\n== 两条「回放自己跟自己打架」的轴，以及它们的并集 ==")
    print("  轴 A = 用回放自己的 `(corrected_function, pos_x)` 复算 ≠ 回放 `pos_y`（逐位）")
    print("  轴 B = 回放自己的 `pos_x` ≠ 它自己的 `end_x`（按 §27/§43 两者都是 `pts[len-1].x`）")
    axisA, axisB = set(), set()
    resid_set = set(resid)
    for k in resid:
        ss = by_batch.get(k, [])
        if any(shot_verdict(s)[0] == "不自洽" for s in ss):
            axisA.add(k)
        if any(not s["pos_eq_endx"] for s in ss):
            axisB.add(k)
    print(f"  {'轴':<8}{'批数':>8}{'占 407':>10}")
    print(f"  {'A':<8}{len(axisA):>8}{pct(len(axisA), len(resid)):>10}")
    print(f"  {'B':<8}{len(axisB):>8}{pct(len(axisB), len(resid)):>10}")
    print(f"  {'A∩B':<8}{len(axisA & axisB):>8}{pct(len(axisA & axisB), len(resid)):>10}")
    print(f"  {'A∪B':<8}{len(axisA | axisB):>8}{pct(len(axisA | axisB), len(resid)):>10}"
          f"   ← 判决就靠这一行")
    print(f"  只被 A 覆盖 {len(axisA - axisB)} 批；只被 B 覆盖 {len(axisB - axisA)} 批；"
          f"两条都不覆盖 {len(resid_set - (axisA | axisB))} 批")
    # 轴 B 的两种形态（`|pos_x − end_x|`）
    forms = collections.Counter()
    for k in resid:
        for s in by_batch.get(k, []):
            if s["pos_eq_endx"]:
                continue
            if isinstance(s["pos_x"], str) or isinstance(s["end_x"], str):
                forms["不可比(NaN)"] += 1
                continue
            d = abs(float(s["pos_x"]) - float(s["end_x"]))
            forms["<1e-12（末几位）" if d < 1e-12 else ("<0.02（一个采样步）" if d < 0.02 else ">=0.02")] += 1
    print(f"  轴 B 的形态（407 里的发）：{dict(forms)}")

    # ---- 5. 逐批表 ----
    if args.table:
        with open(args.table, "w", encoding="utf-8") as f:
            f.write("file\tbatch\taxis\tbucket\tverdict\tshots\tpos_eq_endx\tself_hit\t"
                    "endx_hit\tfullc_hit\tode_found\tode_hit\tmax_abs_ulp\tfn\n")
            for (k, mode, c, v, ss) in detail:
                ulps = [abs(int(s["self_ulp"])) for s in ss
                        if isinstance(s["self_ulp"], (int, str)) and str(s["self_ulp"]).lstrip("-").isdigit()]
                f.write("\t".join([
                    k[0], str(k[1]), mode, c, v, str(len(ss)),
                    f"{sum(1 for s in ss if s['pos_eq_endx'])}/{len(ss)}",
                    f"{sum(1 for s in ss if s['self_hit'])}/{len(ss)}",
                    f"{sum(1 for s in ss if s['endx_hit'])}/{len(ss)}",
                    f"{sum(1 for s in ss if s['fullc_hit'])}/{len(ss)}",
                    f"{sum(1 for s in ss if s['ode_found'])}/{len(ss)}",
                    f"{sum(1 for s in ss if s['ode_hit'])}/{len(ss)}",
                    str(max(ulps) if ulps else "-"),
                    (ss[0]["cf"] if ss and not ss[0]["cf"].startswith("\x00") else "").replace("\t", " "),
                ]) + "\n")
        print(f"\n逐批表已写 {args.table}")

    if args.show:
        print("\n== 逐批明细（前 %d 行）==" % args.show)
        for (k, mode, c, v, ss) in detail[: args.show]:
            ulps = [s["self_ulp"] for s in ss]
            print(f"  {k[0]} #{k[1]:<3} {mode:<7} {c:<3} → {v:<4} "
                  f"发 {len(ss)} pos_eq_endx={sum(1 for s in ss if s['pos_eq_endx'])}/{len(ss)} "
                  f"self_hit={sum(1 for s in ss if s['self_hit'])}/{len(ss)} ulp={ulps}")

    # ---- 7. 真二进制自己的求值器再算一遍（unicorn 直调 `0x4eb9d0`/`0x4ed670`）----
    if args.oracle:
        run_oracle(shots, resid, ok_batches, by_batch, args.oracle)
    return 0


def _hash_bits(v):
    import struct as _s
    return int.from_bytes(_s.pack("<d", v), "little")


def _ulp_int(a_bits, b_bits):
    """两个 double 位串之间的有符号 ulp 距离（0 = 逐位相同）。"""
    def ord_(w):
        return (1 << 63) - w if (w >> 63) else w
    return ord_(a_bits) - ord_(b_bits)


def run_oracle(shots, resid, ok_batches, by_batch, n):
    """用**真二进制自己的**求值器（`0x4ed670` 解析 + `0x4eb9d0` 求值，unicorn）复算，
    把"回放跟不上自己"从"我们的 `eval` 说跟不上"升级成"**原版求值器**说跟不上"。

    正对照：同一套调用在**内容命中的批**上必须给出"自洽"（否则是这条 oracle 链路坏了）。
    """
    sys.path.insert(0, HERE)
    import eval_oracle as EO  # 复用既有对拍台，不重写

    Gw = EO.load_oracle()
    print(f"\n== 真二进制求值器复算（unicorn 直调 `0x4ed670`+`0x4eb9d0`）==")
    print(f"  oracle: {EO.ENV_SO}")

    resid_set = set(resid)

    def one(sel, label):
        step = max(1, len(sel) // max(1, n))
        pick = sel[::step][:n]
        stat = collections.Counter()
        ulps = collections.Counter()
        examples = []
        for s in pick:
            if s["mode"] != "Normal" or not s["split_ok"] or is_nan(s["v_self"]):
                stat["skip"] += 1
                continue
            if pos_is_start(s):
                stat["skip"] += 1
                continue
            x = -s["pos_x"] if s["flip"] else s["pos_x"]
            try:
                EO.reset_heap(Gw)
                toks = EO.call_parse(Gw, s["cf"])
                got = EO.call_eval(Gw, toks, x)
            except Exception as exc:  # noqa: BLE001
                stat["oracle_err"] += 1
                if len(examples) < 3:
                    examples.append((s["file"], s["batch"], "oracle_err", repr(exc)[:80]))
                continue
            gb = _hash_bits(got)
            ours = _hash_bits(float(s["v_self"]))
            obs = _hash_bits(float(s["pos_y"])) if not is_nan(s["pos_y"]) else None
            # ① 真二进制 vs 我方 eval：证明我方求值器在这批文本上逐位对
            stat["bin_eq_ours" if gb == ours else "bin_ne_ours"] += 1
            # ② 真二进制 vs 回放 pos_y：证明"复算不出来"不是我们的问题
            if obs is None:
                stat["no_pos_y"] += 1
                continue
            if gb == obs:
                stat["bin_eq_replay"] += 1
            else:
                stat["bin_ne_replay"] += 1
                u = _ulp_int(obs, gb)
                ulps[u if abs(u) <= 3 else "big"] += 1
                if len(examples) < 3:
                    examples.append((s["file"], s["batch"], f"{u:+d} ulp", s["cf"][:60]))
        print(f"  [{label}] 抽 {len(pick)} 发："
              f"真二进制==我方 {stat['bin_eq_ours']} / !={stat['bin_ne_ours']}；"
              f"真二进制==回放 pos_y {stat['bin_eq_replay']} / !={stat['bin_ne_replay']}")
        print(f"      差值的 ulp 分布 {dict(sorted(ulps.items(), key=lambda x: str(x[0])))}"
              + (f"；跳过 {stat['skip']} / 出错 {stat['oracle_err']}" if stat["skip"] or stat["oracle_err"] else ""))
        for e in examples:
            print(f"      ! {e}")
        return stat

    ok_set = {(r["file"], r["batch"]) for r in ok_batches}
    normal = [s for s in shots
              if shot_verdict(s)[0] != "其它"]  # 炮口 / 缺字段 / NaN 不构成 y 的判据
    # 正对照：**严格**取内容命中的批（`content_ok`）
    one([s for s in normal if (s["file"], s["batch"]) in ok_set], "正对照：内容命中的批（Normal 发）")
    # 目标：407 里的发
    one([s for s in shots if (s["file"], s["batch"]) in resid_set], "407 里的发")
    return 0


def pct(a, b):
    return f"{100.0*a/b:.1f}%" if b else "-"


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
