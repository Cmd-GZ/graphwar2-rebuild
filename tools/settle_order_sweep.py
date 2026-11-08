#!/usr/bin/env python3
"""批内结算顺序策略 × 三轴 × 语料 —— **一次扫出矩阵**（`dispatch/65` §8）。

为什么要有这个脚本：`dispatch/65` 原来写的是"改代码 → 跑三轴 → 看结果"，那是**串行试错**，
一次只能验一个假设（S1 报文序 / S2 到达序 / S3 到达序+挖洞 / S4 tick 序 / S5 owner 序 …）。
把"顺序"做成 `Match` 里的运行期策略（`GW2_SETTLE_ORDER`，见 `src/game.rs` 的 `SettleOrder`）
之后，策略只是环境变量，于是 5–6 格 × 3 轴 × N 份回放可以**一次并行跑完**。

```text
tools/settle_order_sweep.py --dir <语料> --out <输出目录> [--jobs 16] [--strategies s1,s2,s3,s4,s5,s6]
                            [--bin <target/release/examples>] [--serial]
```

三轴（名字必须一起报，`AGENTS.md` 合并纪律 C）：

| 轴 | 命令 | 驱动 |
|---|---|---|
| `fire` | `replay_attrib --drive fire --policy default` | `replaydiff::Predictor` → `Match::fire_batch` |
| `batch` | `replay_attrib --drive batch --policy default` | `Match::fire_batch`（固定模式） |
| `conv` | `batch_conv --drive batch --policy strict` | `ConvPredictor::fire_batch`（**不经过 `Match`**，策略对它无效） |

判据（每格三件，逐批）：

* `content_ok` —— 与护栏同一个口径（`compare_batch` 的整批内容命中）；
* `death` —— `|obs_deaths| > 0` 的批里，`set(我方 killed) == set(obs_deaths)`；
* `hole`  —— `|obs_destroy| > 0` 的批里，我方 `blocked` 点为真且落在回放的挖洞点上（`obs_destroy` 之一）。

并行口径：语料按 `--jobs` 切成符号链接分片，每格一个进程池；聚合时按 `(file, batch)` 排序。
`--serial` 用同一条代码路径串行跑（用于验证"分片没有改变聚合结果"这条正对照）。
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

AXES = ("fire", "batch", "conv")

# 每格的 (content, 死亡, 挖洞) 三元组，"命中"的定义写死在这里，别在报告里改口径。
#
# 死亡 / 挖洞这两条与 `dispatch/65` §1.3 给的 `366/385`、`207/294` **逐数对齐**：
#   * 死亡分母 = **我方报了至少一条 `LifeQueuedDeath` 的批**（385），分子 = 我方死亡
#     **集合**等于回放死亡集合（366，按集合比，不按顺序）；
#   * 挖洞分母 = 回放里有 `DestroyObstacle` 的批（294），分子 = 我方 `blocked` 点与其中
#     一条**逐位相等**（207 —— 1e-9 容差会虚高到 294，因为两边差的是 `hole_point`
#     的印刷版常数，量级 5e-4；那是 §30 那条链的已知残差，不是顺序）。


def shard_files(files, jobs):
    """把语料切成 `jobs` 份（轮流发牌，保证各片大小均匀）。"""
    shards = [[] for _ in range(max(1, min(jobs, len(files))))]
    for i, f in enumerate(sorted(files)):
        shards[i % len(shards)].append(f)
    return [s for s in shards if s]


def make_shard_dirs(files, shards, root):
    """为每一片建一个只含符号链接的目录（`replay_attrib --dir` 只认目录）。"""
    out = []
    for i, shard in enumerate(shards):
        d = os.path.join(root, f"shard{i:02d}")
        if os.path.isdir(d):
            shutil.rmtree(d)
        os.makedirs(d)
        for f in shard:
            os.symlink(os.path.abspath(f), os.path.join(d, os.path.basename(f)))
        out.append(d)
    return out


def run_cell(bin_dir, axis, strategy, shard_dirs, out_dir, parallel):
    """跑一格 = (轴, 策略)：每片一个进程，产出拼起来的 JSONL。"""
    jobs = []
    tmpdir = os.path.join(out_dir, "raw")
    os.makedirs(tmpdir, exist_ok=True)
    rows = []

    def one(i, d):
        out = os.path.join(tmpdir, f"{axis}-{strategy}-{i:02d}.jsonl")
        env = dict(os.environ)
        env["GW2_SETTLE_ORDER"] = strategy
        if axis == "conv":
            cmd = [
                os.path.join(bin_dir, "batch_conv"),
                "--dir", d, "--drive", "batch", "--policy", "strict", "--json", out,
            ]
        else:
            cmd = [
                os.path.join(bin_dir, "replay_attrib"),
                "--dir", d, "--drive", axis, "--policy", "default", "--json", out,
            ]
        t0 = time.time()
        proc = subprocess.run(cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        dt = time.time() - t0
        if proc.returncode != 0:
            sys.stderr.write(f"[{axis}/{strategy}] 分片 {i} 退出码 {proc.returncode}\n")
            sys.stderr.write(proc.stdout.decode("utf-8", "replace")[-4000:])
        return out, dt

    t0 = time.time()
    if parallel:
        with ThreadPoolExecutor(max_workers=len(shard_dirs)) as ex:
            futs = [ex.submit(one, i, d) for i, d in enumerate(shard_dirs)]
            results = [f.result() for f in futs]
    else:
        results = [one(i, d) for i, d in enumerate(shard_dirs)]
    wall = time.time() - t0
    cpu = sum(dt for _, dt in results)
    for out, _ in results:
        with open(out, "r", encoding="utf-8", errors="replace") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    rows.append(json.loads(line))
                except json.JSONDecodeError:
                    sys.stderr.write(f"[{axis}/{strategy}] 坏行：{line[:200]}\n")
    # 排序键：`(file, batch)` —— 与分片顺序无关，聚合才可与串行对拍
    rows.sort(key=lambda r: (r.get("file", ""), r.get("batch", -1)))
    return rows, wall, cpu


def cell_metrics(rows):
    """一格的 (content, 死亡, 挖洞) 三元组。"""
    batches = [r for r in rows if r.get("content_ok") is not None]
    content = sum(1 for r in batches if r.get("content_ok"))
    death_den = death_ok = 0
    hole_den = hole_ok = 0
    for r in batches:
        killed = r.get("killed") or []
        if killed:
            death_den += 1
            if sorted(set(killed)) == sorted(set(r.get("obs_deaths") or [])):
                death_ok += 1
        od = r.get("obs_destroy") or []
        if od:
            hole_den += 1
            bp = r.get("blocked")
            if bp and any(bp[0] == x and bp[1] == y for x, y in od):
                hole_ok += 1
    return {
        "batches": len(batches),
        "content": content,
        "death": death_ok,
        "death_den": death_den,
        "hole": hole_ok,
        "hole_den": hole_den,
    }


def deltas(base_rows, rows):
    """逐批净变化：以 `content_ok` 为准，给出 变好 / 变差 的批清单。"""
    base = {(r.get("file"), r.get("batch")): r for r in base_rows if r.get("content_ok") is not None}
    gained, lost = [], []
    for r in rows:
        k = (r.get("file"), r.get("batch"))
        b = base.get(k)
        if b is None:
            continue
        if r.get("content_ok") and not b.get("content_ok"):
            gained.append((k[0], k[1], r.get("shots"), r.get("owners")))
        elif b.get("content_ok") and not r.get("content_ok"):
            lost.append((k[0], k[1], r.get("shots"), r.get("owners")))
    return gained, lost


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--jobs", type=int, default=16)
    ap.add_argument("--strategies", default="s1,s2,s3,s4,s5,s6")
    ap.add_argument("--bin", default=None)
    ap.add_argument("--serial", action="store_true")
    a = ap.parse_args()

    bin_dir = a.bin or os.environ.get("CARGO_TARGET_DIR", "target") + "/release/examples"
    files = [
        os.path.join(a.dir, n)
        for n in sorted(os.listdir(a.dir))
        if os.path.isfile(os.path.join(a.dir, n))
    ]
    os.makedirs(a.out, exist_ok=True)
    shards = shard_files(files, 1 if a.serial else a.jobs)
    shard_root = tempfile.mkdtemp(prefix="settle-shards-", dir=a.out)
    shard_dirs = make_shard_dirs(files, shards, shard_root)
    strategies = [s.strip() for s in a.strategies.split(",") if s.strip()]
    print(f"语料 {len(files)} 份 → {len(shard_dirs)} 片；策略 {strategies}；轴 {list(AXES)}")

    t_start = time.time()
    matrix = {}
    per_cell_rows = {}
    for strategy in strategies:
        for axis in AXES:
            rows, wall, cpu = run_cell(bin_dir, axis, strategy, shard_dirs, a.out, not a.serial)
            m = cell_metrics(rows)
            m["wall"] = round(wall, 2)
            m["cpu"] = round(cpu, 2)
            matrix[(axis, strategy)] = m
            per_cell_rows[(axis, strategy)] = rows
            print(
                f"  {axis:5s} {strategy}  墙钟 {wall:6.2f}s  内容 {m['content']}/{m['batches']}"
                f"  死亡 {m['death']}/{m['death_den']}  挖洞 {m['hole']}/{m['hole_den']}"
            )
    total = time.time() - t_start

    # ---- 矩阵表（TSV）----
    tsv = os.path.join(a.out, "matrix.tsv")
    with open(tsv, "w", encoding="utf-8") as fh:
        fh.write("axis\tstrategy\tbatches\tcontent\tdeath\tdeath_den\thole\thole_den\twall\tcpu\n")
        for axis in AXES:
            for strategy in strategies:
                m = matrix[(axis, strategy)]
                fh.write(
                    f"{axis}\t{strategy}\t{m['batches']}\t{m['content']}\t{m['death']}\t"
                    f"{m['death_den']}\t{m['hole']}\t{m['hole_den']}\t{m['wall']}\t{m['cpu']}\n"
                )
    print(f"\n矩阵 → {tsv}")

    # ---- 逐批净变化（以 s1 为基线，逐轴）----
    dpath = os.path.join(a.out, "deltas.txt")
    with open(dpath, "w", encoding="utf-8") as fh:
        for axis in AXES:
            base = per_cell_rows.get((axis, "s1"))
            if base is None:
                continue
            for strategy in strategies:
                if strategy == "s1":
                    continue
                g, l = deltas(base, per_cell_rows[(axis, strategy)])
                fh.write(f"## {axis}  s1 → {strategy}：变好 {len(g)} 批 / 变差 {len(l)} 批\n")
                for tag, items in (("+", g), ("-", l)):
                    for f, b, n, owners in items:
                        fh.write(f"  {tag} {f} #{b}（{n} 发 owners={owners}）\n")
    print(f"逐批净变化 → {dpath}")
    print(f"总墙钟 {total:.2f}s（串行估计 {sum(m['cpu'] for m in matrix.values()):.2f}s CPU）")
    shutil.rmtree(shard_root, ignore_errors=True)


if __name__ == "__main__":
    main()
