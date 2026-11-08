#!/usr/bin/env python3
"""假设空间扫描的**并行驱动**：按代价装桶（LPT）+ 计时 + 汇总。

为什么不是 `--shard i/N` 取模：取模分片让"最慢那一片"决定整体墙钟（实测 128 片取模、
32 槽：墙钟 35.1 s，而 CPU 总量 507 s ÷ 32 = 15.9 s ⇒ 一半时间在等那一片）。
这里先跑一次**成本探针**（只解析回放、不预测，403 份 0.11 s），按
`shots_normal + ode_weight·shots_ode` 估代价，再用 LPT（最长优先、装进当前最轻的桶）
把文件分给 M 个桶；M 比槽位数多一点，槽位空出来就能接下一桶。

```bash
python3 tools/hypothesis_sweep_par.py \
    --bin target53/release/examples/hypothesis_sweep \
    --dir <快照>/replays --out .agent-work/cg/sweep53/par \
    --slots 32 --buckets 64 --configs all --verify-first
```

输出：`<out>/shard-<k>.jsonl`（汇总）、`<out>/batch-<k>.jsonl`（逐批），
以及 `<out>/parallel.json`（墙钟、各桶耗时、串行对照由调用方给）。

⚠ `--verify` 只给第一个桶（正对照只需要跑一遍，29 档全跑一遍白烧 CPU）。
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import time


def cost_probe(binary, corpus):
    t0 = time.time()
    out = subprocess.run(
        [binary, "--dir", corpus, "--cost-probe"],
        capture_output=True, text=True, check=True,
    ).stdout
    costs = {}
    for line in out.splitlines():
        parts = line.split("\t")
        if len(parts) != 4:
            continue
        name, batches, shots, ode = parts
        try:
            costs[name] = (int(batches), int(shots), int(ode))
        except ValueError:
            continue
    return costs, time.time() - t0


def lpt(costs, buckets, ode_weight):
    """最长优先 —— 每份文件装进当前估计负载最轻的桶。"""
    items = sorted(
        costs.items(),
        key=lambda kv: -(kv[1][1] - kv[1][2] + ode_weight * kv[1][2]),
    )
    loads = [0.0] * buckets
    bins = [[] for _ in range(buckets)]
    for name, (batches, shots, ode) in items:
        est = (shots - ode) + ode_weight * ode
        k = min(range(buckets), key=lambda i: loads[i])
        bins[k].append(name)
        loads[k] += est
    return bins, loads


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--dir", required=True, help="回放目录")
    ap.add_argument("--out", required=True, help="输出目录（会被清空）")
    ap.add_argument("--slots", type=int, default=32, help="并发进程数（= 逻辑核数）")
    ap.add_argument("--buckets", type=int, default=None, help="桶数（默认 2×slots）")
    ap.add_argument("--configs", default="all")
    ap.add_argument("--verify-first", action="store_true", help="只给第一个桶开 --verify")
    ap.add_argument("--ode-weight", type=float, default=12.0)
    ap.add_argument("--batches", action="store_true", default=True)
    args = ap.parse_args()

    buckets = args.buckets or args.slots * 2
    if os.path.isdir(args.out):
        shutil.rmtree(args.out)
    os.makedirs(args.out)

    costs, probe_secs = cost_probe(args.bin, args.dir)
    if not costs:
        print("成本探针没读到任何文件", file=sys.stderr)
        return 2
    bins, loads = lpt(costs, buckets, args.ode_weight)
    # 空桶丢掉（文件数 < 桶数时会出现）
    bins = [b for b in bins if b]
    print(f"成本探针 {probe_secs:.2f}s，{len(costs)} 份回放 → {len(bins)} 个桶"
          f"（估计负载 min/max = {min(loads):.0f}/{max(loads):.0f}）")

    running = []
    times = []
    t0 = time.time()
    pending = list(enumerate(bins))
    while pending or running:
        while pending and len(running) < args.slots:
            k, files = pending.pop(0)
            lst = os.path.join(args.out, f"list-{k}.txt")
            with open(lst, "w", encoding="utf-8") as f:
                f.write("\n".join(files) + "\n")
            cmd = [
                args.bin, "--dir", args.dir, "--files", lst,
                "--configs", args.configs,
                "--json", os.path.join(args.out, f"shard-{k}.jsonl"),
                "--batches", os.path.join(args.out, f"batch-{k}.jsonl"),
            ]
            if args.verify_first and k == 0:
                cmd.append("--verify")
            t_start = time.time()
            p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            running.append((p, k, len(files), t_start))
        time.sleep(0.05)
        still = []
        for p, k, n, t_start in running:
            if p.poll() is None:
                still.append((p, k, n, t_start))
            else:
                times.append((k, n, time.time() - t_start, p.returncode))
        running = still
    wall = time.time() - t0

    times.sort(key=lambda x: -x[2])
    meta = {
        "wall_secs": round(wall, 3),
        "slots": args.slots,
        "buckets": len(bins),
        "probe_secs": round(probe_secs, 3),
        "bucket_secs_top5": [
            {"bucket": k, "files": n, "secs": round(s, 2), "rc": rc} for k, n, s, rc in times[:5]
        ],
        "bucket_secs_min": round(times[-1][2], 2) if times else None,
        "bad_rc": [k for k, _, _, rc in times if rc != 0],
    }
    with open(os.path.join(args.out, "parallel.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, ensure_ascii=False, indent=2)
    print(f"墙钟 {wall:.2f}s（{len(bins)} 桶 / {args.slots} 槽）；最慢 5 桶："
          + ", ".join(f"#{k}:{s:.1f}s" for k, _, s, _ in times[:5]))
    if meta["bad_rc"]:
        print(f"⚠ 非零退出的桶：{meta['bad_rc']}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
