#!/usr/bin/env python3
"""汇总 `examples/hypothesis_sweep.rs` 的分片输出：每档假设的内容命中数 + 翻转集。

用法（在仓库根或任何目录都行，路径给全）：

```bash
python3 tools/hypothesis_sweep_agg.py --dir .agent-work/cg/sweep53/par32 \
        --tsv fixtures/hypothesis_sweep.tsv
```

输入：`--dir` 下的 `*.jsonl`（每个分片一份）。两种行都吃：

* 汇总行：`{"file":..,"config":"H0","batches":N,"content_ok":M,...}`（`error` 行 `config` 为 `"*"`，跳过）
* 逐批行（`--batches` 产出）：`{"file":..,"batch":i,"config":"H0","content_ok":true}`

输出：
1. 每档的 `batches / content_ok / Δ vs H0`；
2. 若给了逐批行，再加 `fix`（基线失败→本档命中）/ `break`（基线命中→本档失败）/ `net`——
   **只有 `net > 0` 才能叫"更好的解释"**，`fix` 大而 `break` 更大的一律不是；
3. `--tsv` 写一张纯文本表（入库用；大 JSONL 不入库）。

判据（任务书 §1）：任何一档要"稳定超过" H0 才算新线索；这里 `net > 0` 是必要条件，
再要求它不是因为个别文件(单文件全对/全错)波动 —— 报告里逐档标注 `files_better/files_worse`。
"""
import argparse
import collections
import glob
import json
import os
import sys


def load(paths):
    """→ (per_config Counter, per_batch dict[(config)][(file, idx)] = bool)"""
    agg = collections.defaultdict(collections.Counter)
    batch = collections.defaultdict(dict)
    seen_batch_detail = False
    for p in paths:
        with open(p, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    d = json.loads(line)
                except json.JSONDecodeError:
                    print(f"[warn] 坏行 {p}: {line[:80]}", file=sys.stderr)
                    continue
                cfg = d.get("config")
                if cfg is None or cfg == "*":
                    agg["__errors__"]["files"] += 1
                    continue
                if "batch" in d:  # 逐批行
                    seen_batch_detail = True
                    batch[cfg][(d["file"], d["batch"])] = bool(d["content_ok"])
                    if d.get("noop"):
                        agg[cfg]["noop"] += 1
                    continue
                a = agg[cfg]
                for k in ("batches", "content_ok", "invalid", "parity_shots", "parity_bad", "emitted_bad"):
                    a[k] += d.get(k, 0)
                a["files"] += 1
                if d.get("hyp"):
                    a["hyp"] = d["hyp"]
    return agg, batch, seen_batch_detail


def field_only(hyp):
    """与 `examples/hypothesis_sweep.rs::field_only` 同一条判据：只换报文里的字段、
    不动点序列的档。只有这种档的 `noop` 计数才有意义（其余档每一步都在改点序列）。"""
    if not hyp:
        return False
    return "c=3dp" in hyp and "step=0.01" in hyp and ("term=stop" in hyp or "term=last_inside" in hyp)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", required=True, help="分片输出目录（*.jsonl）")
    ap.add_argument("--tsv", help="把汇总表写成 TSV")
    ap.add_argument("--base", default="H0", help="基线档名（默认 H0）")
    args = ap.parse_args()

    paths = sorted(glob.glob(os.path.join(args.dir, "*.jsonl")))
    if not paths:
        print(f"没有找到 {args.dir}/*.jsonl", file=sys.stderr)
        return 2
    agg, batch, detail = load(paths)
    if args.base not in agg:
        print(f"缺基线档 {args.base}", file=sys.stderr)
        return 2
    base = agg[args.base]
    n_base = base["content_ok"]
    base_batch = batch.get(args.base, {})

    rows = []
    for cfg, a in agg.items():
        if cfg == "__errors__":
            continue
        row = {
            "config": cfg,
            "batches": a["batches"],
            "content_ok": a["content_ok"],
            "delta": a["content_ok"] - n_base,
            "invalid": a["invalid"],
            "files": a["files"],
            "parity_bad": a["parity_bad"],
            "emitted_bad": a["emitted_bad"],
            "fix": None,
            "break": None,
            "net": None,
            "files_better": None,
            "files_worse": None,
            "noop": a["noop"] if field_only(a.get("hyp")) else None,
        }
        if detail and base_batch:
            bb = batch.get(cfg, {})
            fix = brk = 0
            for k, ok in bb.items():
                b0 = base_batch.get(k)
                if b0 is None:
                    continue
                if ok and not b0:
                    fix += 1
                elif b0 and not ok:
                    brk += 1
            row["fix"], row["break"], row["net"] = fix, brk, fix - brk
        rows.append(row)

    if detail and base_batch:
        # 逐文件净变化（判断"稳定超过"而不是被个别文件带起来）
        per_file = collections.defaultdict(lambda: [0, 0])
        for cfg in batch:
            for (f, _i), ok in batch[cfg].items():
                b0 = base_batch.get((f, _i))
                if b0 is None or cfg == args.base:
                    continue
                if ok != b0:
                    per_file[cfg][0 if ok else 1] += 1
        for row in rows:
            c = row["config"]
            if c in per_file:
                row["files_better"], row["files_worse"] = per_file[c][0], per_file[c][1]

    rows.sort(key=lambda r: (-r["delta"], r["config"]))
    tsv = [
        "# 假设空间整片扫描（fire 轴，1080 结算批 / 647 基线）。"
        "生成：tools/hypothesis_sweep_agg.py（输入 examples/hypothesis_sweep.rs 的 --json/--batches 输出）",
        "config\tcontent_ok\tbatches\tdelta\tbeat_base\tfix\tbreak\tnet\tnoop_batches\tinvalid\tparity_bad\temitted_bad",
    ]
    print(f"基线 {args.base}：{n_base}/{'?'} 内容命中（{base['files']} 份回放）")
    print(f"{'config':24s} {'ok':>5s} {'n':>5s} {'Δ':>5s} {'fix':>5s} {'brk':>5s} {'net':>5s} {'noop':>5s}")
    for r in rows:
        f = "" if r["fix"] is None else f"{r['fix']:5d}"
        b = "" if r["break"] is None else f"{r['break']:5d}"
        nt = "" if r["net"] is None else f"{r['net']:5d}"
        np_ = "" if r["noop"] is None else f"{r['noop']:5d}"
        print(f"{r['config']:24s} {r['content_ok']:5d} {r['batches']:5d} {r['delta']:+5d} {f} {b} {nt} {np_}")
        tsv.append(
            "\t".join(
                str(x)
                for x in [
                    r["config"], r["content_ok"], r["batches"], r["delta"],
                    int(r["delta"] > 0),
                    "" if r["fix"] is None else r["fix"],
                    "" if r["break"] is None else r["break"],
                    "" if r["net"] is None else r["net"],
                    "-" if r["noop"] is None else r["noop"],
                    r["invalid"], r["parity_bad"], r["emitted_bad"],
                ]
            )
        )

    if args.tsv:
        os.makedirs(os.path.dirname(os.path.abspath(args.tsv)), exist_ok=True)
        with open(args.tsv, "w", encoding="utf-8") as f:
            f.write("\n".join(tsv) + "\n")
        print(f"\n汇总表 → {args.tsv}")

    better = [r for r in rows if r["delta"] > 0]
    if better:
        print("\n⚠ 有档超过基线：" + ", ".join(f"{r['config']}({r['delta']:+d})" for r in better))
    else:
        print("\n没有一档超过基线（假设空间对这条判据已扫完）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
