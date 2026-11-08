#!/usr/bin/env python3
"""`examples/batch_cross_probe` 的**并行外壳**（`docs/06-verification.md` §34，`dispatch/55` §1.B）。

为什么要有它：判"串发"要把 **每个 (replay, 批, 发)** 都独立跑一遍采样器，再拿每一发的
回放停点去撞**同一批所有发**的点列。一次 `--all` 串行就能出结果（这批语料约 1 s），
但任务书要求"并行真的吃满核、并给出墙钟与加速比"，所以把 (replay, 批) 切成独立任务，
用进程池铺开跑，并与**同一份任务表**的串行墙钟对比。

```text
python3 tools/batch_cross_driver.py --dir <语料> [--mask-mode arrival] \\
    [--jobs 16] [--min-shots 6] [--bin <batch_cross_probe>] [--json out.jsonl]
```

输出：任务数与墙钟 / 加速比 / 汇总（stdout）。汇总口径与
`examples/batch_cross_probe.rs` 的"汇总"一节逐字一致。

⚠️ 这不是"证据生成器"：判据在 Rust 侧（逐位相等），这里只做任务切分与计数，
并**校验串行与并行产出同一批行**（并行不许改变结论）。
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ProcessPoolExecutor

BIN_DEFAULT = os.environ.get(
    "BATCH_CROSS_PROBE",
    os.path.join(os.environ.get("CARGO_TARGET_DIR", ""), "release", "examples", "batch_cross_probe"),
)


def run_one(job):
    """跑一个 (file, batch) 任务，返回它产出的 JSONL 行。"""
    bin_path, work, corpus, fname, batch, mask_mode = job
    out = os.path.join(work, f"{fname}-{batch}.jsonl")
    cmd = [
        bin_path,
        "--dir", corpus,
        "--file", fname,
        "--batch", str(batch),
        # 单批任务不需要再筛发数，否则 1 发的批会被默认阈值挡掉
        "--min-shots", "1",
        "--mask-mode", mask_mode,
        "--json", out,
    ]
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"{fname} b{batch}: {p.stderr.strip()[:400]}")
    rows = []
    if os.path.exists(out):
        with open(out) as f:
            rows.extend(json.loads(line) for line in f if line.strip())
    return rows


def discover(bin_path, corpus, mask_mode, min_shots):
    """一次串行 `--all`（或 `--min-shots N`）拿任务表 —— 顺带得到单进程参照墙钟。"""
    t0 = time.time()
    sel = ["--all"] if min_shots <= 1 else ["--min-shots", str(min_shots)]
    p = subprocess.run(
        [bin_path, "--dir", corpus, *sel, "--mask-mode", mask_mode],
        capture_output=True, text=True,
    )
    wall = time.time() - t0
    if p.returncode != 0:
        sys.exit(f"发现阶段失败：{p.stderr[:400]}")
    tasks, seen = [], set()
    for line in p.stdout.splitlines():
        if line.startswith("===== ") and len(line.split()) >= 3 and line.split()[2].startswith("批#"):
            parts = line.split()
            fname, batch = parts[1], int(parts[2].lstrip("批#"))
            if (fname, batch) not in seen:
                seen.add((fname, batch))
                tasks.append((fname, batch))
    return tasks, wall


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", required=True)
    ap.add_argument("--bin", default=BIN_DEFAULT)
    ap.add_argument("--jobs", type=int, default=16)
    ap.add_argument("--min-shots", type=int, default=6)
    ap.add_argument("--mask-mode", default="arrival")
    ap.add_argument("--json", default=None)
    a = ap.parse_args()

    if not os.path.isfile(a.bin):
        sys.exit(f"找不到探针二进制：{a.bin}（先 cargo build --release --example batch_cross_probe）")

    tasks, wall_ref = discover(a.bin, a.dir, a.mask_mode, a.min_shots)
    print(f"任务数 {len(tasks)}（replay × 批），并发 {a.jobs}，mask={a.mask_mode}", file=sys.stderr)

    work = tempfile.mkdtemp(prefix="bcp55-")
    try:
        t0 = time.time()
        serial_rows = []
        for fname, batch in tasks:
            serial_rows.extend(parse(run_one((a.bin, work, a.dir, fname, batch, a.mask_mode))))
        wall_serial = time.time() - t0

        t0 = time.time()
        par_rows = []
        with ProcessPoolExecutor(max_workers=a.jobs) as ex:
            jobs = [(a.bin, work, a.dir, f, b, a.mask_mode) for f, b in tasks]
            for rows in ex.map(run_one, jobs):
                par_rows.extend(rows)
        wall_par = time.time() - t0
    finally:
        shutil.rmtree(work, ignore_errors=True)

    ka = sorted((r["file"], r["batch"], r["owner"]) for r in serial_rows)
    kb = sorted((r["file"], r["batch"], r["owner"]) for r in par_rows)
    if ka != kb:
        sys.exit(f"串行/并行行不一致：{len(ka)} vs {len(kb)}")

    def summarize(rows, label):
        pos = {"own-stop": 0, "own-point": 0, "cross": 0, "none": 0}
        end = {"same-as-pos": 0, "own-prev": 0, "own-other": 0, "cross": 0, "none": 0}
        stop_ok = 0
        for r in rows:
            pos[r["cat"]] = pos.get(r["cat"], 0) + 1
            end[r["end_cat"]] = end.get(r["end_cat"], 0) + 1
            stop_ok += 1 if r["stop_reproduced"] else 0
        shots = len(rows)
        batches = len({(r["file"], r["batch"]) for r in rows})
        print(f"\n[{label}] 批 {batches} / 发 {shots}")
        print(f"  pos 侧：own-stop {pos['own-stop']} / own-point {pos['own-point']} / "
              f"cross {pos['cross']} / none {pos['none']}")
        print(f"  停点逐位复现：{stop_ok} / {shots}")
        print(f"  end_x 侧：same-as-pos {end['same-as-pos']} / own-prev {end['own-prev']} / "
              f"own-other {end['own-other']} / cross {end['cross']} / none {end['none']}")
        return shots

    n = summarize(serial_rows, f"串行 jobs=1（{wall_serial:.2f} s）")
    speed = wall_serial / wall_par if wall_par > 0 else float("nan")
    print(f"\n墙钟：串行 {wall_serial:.2f} s / 并行 jobs={a.jobs} {wall_par:.2f} s "
          f"⇒ 加速比 {speed:.2f}×（{n} 发，{os.cpu_count()} 逻辑核）")
    print(f"（另外：单进程 `--all` 一把跑完的墙钟 {wall_ref:.2f} s，仅作参照口径）")

    if a.json:
        with open(a.json, "w") as f:
            for r in par_rows:
                f.write(json.dumps(r, ensure_ascii=False) + "\n")
        print(f"逐发明细写入 {a.json}")


def parse(rows):
    return rows


if __name__ == "__main__":
    main()
