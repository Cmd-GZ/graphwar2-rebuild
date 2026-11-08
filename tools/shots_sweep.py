#!/usr/bin/env python3
"""**按文件分片 + 进程池**跑 `examples/replay_attrib`，并把 401 份回放的结果合成一份 JSONL。

`replay_attrib` 没有 `--jobs`（**故意不加**：它是护栏工具，加并行会动到输出顺序与判定口径）。
分片放在外面做：`--dir` 只认一个目录，所以给每个分片造一个**只含符号链接**的目录，
再把进程池的并发度拉满。分片按**字节数**做 LPT 装桶（最长的先装），避免"最慢那片定墙钟"。

```bash
export CARGO_HOME=<WS>/.reference/cargo-home CARGO_TARGET_DIR=<WS>/.agent-work/cg/target62
cargo build --release --offline --example replay_attrib
python3 tools/shots_sweep.py \
    --bin $CARGO_TARGET_DIR/release/examples/replay_attrib \
    --dir <WS>/.reference/replays/gw2-replays-snapshot/replays \
    --out /tmp/shots_sweep --jobs 16
# 产出：/tmp/shots_sweep/fire.jsonl（逐批）+ /tmp/shots_sweep/timing.json（墙钟/加速比）

python3 tools/shots_vs_fail.py --json /tmp/shots_sweep/fire.jsonl
```

**为什么要 `--serial-check`**：并行只是把同一份 `--dir` 切碎喂给同一个二进制，
结果必须与**单进程整目录**跑出来的逐行相同（行内按 `(file, batch)` 排序后比较）。
默认开；关掉只为省 8 秒。分片目录里只有数字文件名的符号链接（`MANIFEST.tsv`/`SHA256SUMS`
不进分片，否则读失败计数会变、与护栏口径对不上）。
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor


def list_replays(d):
    """只取**数字文件名**（快照里另有 MANIFEST.tsv / SHA256SUMS 两份元数据）。"""
    return sorted(f for f in os.listdir(d) if f.isdigit() and os.path.isfile(os.path.join(d, f)))


def lpt_bins(files, sizes, n):
    """LPT：按大小降序，逐个放进当前最轻的桶。"""
    bins = [[] for _ in range(n)]
    load = [0] * n
    for f in sorted(files, key=lambda x: -sizes[x]):
        i = load.index(min(load))
        bins[i].append(f)
        load[i] += sizes[f]
    return bins, load


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True, help="replay_attrib 可执行文件")
    ap.add_argument("--dir", required=True, help="回放快照目录（401 份）")
    ap.add_argument("--out", required=True, help="产物目录（会被清空重建）")
    ap.add_argument("--jobs", type=int, default=16)
    ap.add_argument("--drive", default="fire", choices=["fire", "batch", "conv"])
    ap.add_argument("--policy", default="default", choices=["default", "strict"])
    ap.add_argument("--no-serial-check", action="store_true",
                    help="跳过单进程整目录对照（省时间，但就少了'并行没改变结果'的证明）")
    args = ap.parse_args(argv)

    files = list_replays(args.dir)
    if not files:
        sys.exit(f"{args.dir} 里没有数字文件名的回放")
    sizes = {f: os.path.getsize(os.path.join(args.dir, f)) for f in files}
    os.makedirs(args.out, exist_ok=True)
    shard_root = os.path.join(args.out, "shards")
    shutil.rmtree(shard_root, ignore_errors=True)
    os.makedirs(shard_root)

    bins, loads = lpt_bins(files, sizes, args.jobs)
    for i, b in enumerate(bins):
        d = os.path.join(shard_root, f"s{i:02d}")
        os.makedirs(d)
        for f in b:
            os.symlink(os.path.join(os.path.abspath(args.dir), f), os.path.join(d, f))

    print(f"回放 {len(files)} 份 / {sum(sizes.values())} B → {args.jobs} 片"
          f"（每片 {min(len(b) for b in bins)}–{max(len(b) for b in bins)} 份）")

    def run_shard(i):
        d = os.path.join(shard_root, f"s{i:02d}")
        js = os.path.join(args.out, f"shard{i:02d}.jsonl")
        log = os.path.join(args.out, f"shard{i:02d}.log")
        t0 = time.time()
        with open(log, "w", encoding="utf-8") as lf:
            subprocess.run([args.bin, "--dir", d, "--drive", args.drive,
                            "--policy", args.policy, "--json", js],
                           stderr=lf, check=True)
        return i, time.time() - t0, js

    t0 = time.time()
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        results = list(pool.map(run_shard, range(args.jobs)))
    wall_par = time.time() - t0
    shard_times = {i: t for i, t, _ in results}

    # 合并（各片内部保持 replay_attrib 的 file 排序；合并后按 (file, batch) 重排，便于比对）
    merged = os.path.join(args.out, f"{args.drive}.jsonl")
    rows = []
    errs = 0
    for i in range(args.jobs):
        with open(os.path.join(args.out, f"shard{i:02d}.jsonl"), encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                r = json.loads(line)
                if r.get("batch", -1) >= 0:
                    rows.append(r)
                else:
                    errs += 1
                    rows.append(r)
    rows.sort(key=lambda r: (r["file"], r.get("batch", -1)))
    with open(merged, "w", encoding="utf-8") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")

    batches = sum(1 for r in rows if r.get("batch", -1) >= 0)
    ok = sum(1 for r in rows if r.get("batch", -1) >= 0 and r["content_ok"])
    print(f"结算批 {batches}；内容命中 {ok}；读失败/无批 {errs}")

    timing = {
        "files": len(files),
        "bytes": sum(sizes.values()),
        "jobs": args.jobs,
        "wall_parallel_s": round(wall_par, 3),
        "sum_shard_s": round(sum(shard_times.values()), 3),
        "slowest_shard_s": round(max(shard_times.values()), 3),
        "batches": batches,
        "content_ok": ok,
        "read_failures": errs,
        "drive": args.drive,
        "policy": args.policy,
    }

    if not args.no_serial_check:
        js = os.path.join(args.out, "serial.jsonl")
        print("单进程整目录对照…")
        t1 = time.time()
        subprocess.run([args.bin, "--dir", args.dir, "--drive", args.drive,
                        "--policy", args.policy, "--json", js],
                       stderr=subprocess.DEVNULL, check=True)
        wall_serial = time.time() - t1
        def norm(p):
            out = []
            with open(p, encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if line:
                        out.append(json.loads(line))
            # 串行对照是跑**整个目录**（含 MANIFEST.tsv / SHA256SUMS 两份元数据，
            # 它们只会产错误行）；分片里只有 401 份数字文件。所以只比这 401 份的行。
            names = set(files)
            out = [r for r in out if r["file"] in names]
            out.sort(key=lambda r: (r["file"], r.get("batch", -1)))
            return out
        same = norm(js) == norm(merged)
        timing["wall_serial_s"] = round(wall_serial, 3)
        timing["speedup"] = round(wall_serial / wall_par, 2) if wall_par else None
        timing["identical_to_serial"] = same
        print(f"单进程墙钟 {wall_serial:.2f} s；并行 {wall_par:.2f} s；"
              f"加速比 {wall_serial / wall_par:.2f}×；这 401 份的结果与串行逐行相同 = {same}")
        if not same:
            print("⚠ 并行结果与单进程不一致 —— 先查分片是否漏/重了文件", file=sys.stderr)
            return 1

    with open(os.path.join(args.out, "timing.json"), "w", encoding="utf-8") as f:
        json.dump(timing, f, ensure_ascii=False, indent=2)
        f.write("\n")
    print(f"汇总：{merged}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
