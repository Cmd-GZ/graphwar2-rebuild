#!/usr/bin/env python3
"""`corrected_function` 含 **NaN** 的批：逐发把「真二进制 / 我方 / 回放」三方摆在一起
—— `docs/06-verification.md` §38 的证据生成器（`dispatch/64`）。

```bash
export EMU=<WS>/.reference/analysis-repo
export SO=<WS>/.reference/steam/2026-08-28/libgraphwarrust.so
$WS/.reference/venv/bin/python tools/nan_batches.py \
    --json <nan_export 的输出> [--tsv fixtures/nan_batches.tsv] [--jobs 16]
```

**输入不是手写清单**，而是 `examples/nan_export.rs` 在**全语料**上导出的逐发 JSONL
（`--dir <快照>`）。所以"哪几批含 NaN"是算出来的，不是抄来的。

它调真二进制的**一个**函数（地址与签名见 `tools/nan_c_probe.py`）：

| 地址 | 是什么 |
|---|---|
| `0x3c5f70` | `GraphwarShotCalculator::calculate_function_offset_str` —— 产生 `" + 1.197"` / `" + NaN"` 的那一步 |

对每一发问三个问题（全部逐字节）：

```text
① 真函数喂**报文 start_x**      == 我方 `corrected_function` 的后缀？   ⇒ 我们的公式/求值器有没有差
② 真函数喂 start_x 向零 1 ulp   == 回放的 `corrected_function` 后缀？  ⇒ 回放那个 c 是哪来的
③ 双方都印 NaN 的批            == 同一个串？                          ⇒ NaN 打印本身是不是原版行为
```

## 三类发（本脚本的分桶，`docs/06` §38.2）

| 桶 | 判据 | 含义 |
|---|---|---|
| `both` | 我方与回放**都**含 `NaN` | NaN 传播对了（正对照） |
| `obs` | 只有回放含 `NaN` | 回放的 `c` 是 NaN、我们的是数 |
| `ours` | 只有我方含 `NaN` | 反向 |
| `shift` | `x0` 向零挪 1 ulp 后我方的串**变** | "1 ulp 就能翻转"的放大器（含 13 批与 7 个定标点） |

`--jobs N` 走进程池（每个 worker 各自 `Gw()` 一份 unicorn 实例）。默认 1 = 串行，
便于把两种口径的墙钟都记下来（`dispatch/64` §7 要求报加速比）。
"""

import argparse
import json
import os
import struct
import sys
import time
from multiprocessing import Pool

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)

WS = os.environ.get("EMU")
if not WS:
    raise SystemExit("请先 export EMU=<含 analysis/emu.py 的仓库根>（见 tools/eval_oracle.py 头部）")
os.environ.setdefault("SO", "/home/CG/Myprojects/graphwar2-rebuild/.reference/steam/2026-08-28/libgraphwarrust.so")
sys.path.insert(0, HERE)

import eval_oracle as EO  # noqa: E402
import nan_c_probe as NC  # noqa: E402  （offset_str / fb / fbits / f64_shift 复用同一套姿势）

_GW = None


def gw():
    global _GW
    if _GW is None:
        _GW = EO.load_oracle()
    return _GW


def x0_of(row):
    return struct.unpack("<d", struct.pack("<Q", row["x0"]))[0]


def is_nan_s(s):
    return "NaN" in s


def bucket(row):
    """按**后缀**分桶。

    `Ode`（微分方程）局**没有后缀**（`corrected_function == function`，§33.2），
    两侧的串本来就相同 ⇒ 与 NaN 无关，一律落 `none`。
    """
    if row["mode"] != "Normal":
        return "none"
    o, b = is_nan_s(row["suf"]), is_nan_s(row["obs_suf"])
    if o and b:
        return "both"
    if b:
        return "obs"
    if o:
        return "ours"
    return "none"


def job(row):
    """一次 unicorn 探针（每个 (batch, shot) 独立任务）。"""
    g = gw()
    x0 = x0_of(row)
    y0 = struct.unpack("<d", struct.pack("<Q", row["y0"]))[0]
    flip = 1 if row["flip"] else 0
    at_x0 = NC.offset_str(g, row["text"], x0, y0, flip)
    at_tz = NC.offset_str(g, row["text"], NC.f64_shift(x0, 1), y0, flip)
    return {
        "file": row["file"],
        "batch": row["batch"],
        "tick": row["tick"],
        "shot": row["shot"],
        "x0": "%016x" % row["x0"],
        "bucket": bucket(row),
        "shift": row["mode"] == "Normal" and row["suf"] != row["suf_tz"],
        "text": row["text"],
        "real_x0": at_x0,
        "real_tz": at_tz,
        "ours": row["suf"],
        "ours_tz": row["suf_tz"],
        "obs": row["obs_suf"],
        "match_ours_x0": at_x0 == row["suf"],
        "match_obs_x0": at_x0 == row["obs_suf"],
        "match_obs_tz": at_tz == row["obs_suf"],
        # 回放侧"NaN ⇒ 弹道退化成一个点"的两条判据（`docs/06` §38.5）
        "deg_endx": row["obs_endx"] == row["x0"],
        "deg_expl": row["obs_expl"] == [row["x0"], row["y0"]],
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", required=True, help="examples/nan_export.rs 的产物")
    ap.add_argument("--tsv", default=None, help="写一份纯文本摘要（fixtures/ 用）")
    ap.add_argument("--jobs", type=int, default=1)
    ap.add_argument("--all", action="store_true", help="对全部 1290 发跑（慢：约 25 分钟串行）")
    a = ap.parse_args()

    rows = [json.loads(l) for l in open(a.json, encoding="utf-8") if l.strip()]
    if a.all:
        sel = rows
    else:
        sel = [
            r
            for r in rows
            if bucket(r) != "none" or (r["mode"] == "Normal" and r["suf"] != r["suf_tz"])
        ]
    # 稳定排序：文件、批号、发序
    sel.sort(key=lambda r: (r["file"], r["batch"], r["shot"]))

    t0 = time.time()
    if a.jobs > 1:
        with Pool(a.jobs) as p:
            out = p.map(job, sel, chunksize=max(1, len(sel) // (a.jobs * 4)))
    else:
        out = [job(r) for r in sel]
    wall = time.time() - t0

    # 串行等价估算：每个任务平均耗时 × 任务数（用于报加速比）
    per = wall / max(1, len(sel))

    print(f"## 选中的发：{len(sel)} / 全语料 {len(rows)}（jobs={a.jobs}，墙钟 {wall:.2f}s）")
    print()
    hdr = f"{'批':22s} {'桶':5s} {'真@start_x':>22s} {'真@x0-1ulp':>22s} {'我方':>22s} {'回放':>22s} ① ②"
    print(hdr)
    print("-" * len(hdr))
    for r in out:
        lab = f"{r['file']}#{r['batch']}"
        m1 = "OK" if r["match_ours_x0"] else "DIFF"
        m2 = "OK" if r["match_obs_tz"] else "DIFF"
        print(
            f"{lab:22s} {r['bucket']:5s} {r['real_x0']:>22s} {r['real_tz']:>22s} "
            f"{r['ours']:>22s} {r['obs']:>22s} {m1:3s} {m2:3s} {'$' if r['shift'] else ' '}"
        )
    print()
    print("① = 真函数喂**报文 start_x** == 我方产出   ② = 真函数喂 start_x 向零 1 ulp == 回放")
    print()

    cnt = {}
    for r in out:
        k = r["bucket"]
        c = cnt.setdefault(k, [0, 0, 0])
        c[0] += 1
        c[1] += r["match_ours_x0"]
        c[2] += r["match_obs_tz"]
    print("## 分桶（发数 / ①为 OK / ②为 OK）")
    for k in ("both", "obs", "ours", "none"):
        if k in cnt:
            n, m1, m2 = cnt[k]
            print(f"  {k:5s} {n:3d} 发   ① {m1:3d}   ② {m2:3d}")
    n1 = sum(r["match_ours_x0"] for r in out)
    n2 = sum(r["match_obs_tz"] for r in out)
    print(f"  合计 {len(out):3d} 发   ① {n1:3d}   ② {n2:3d}")
    print()
    fix = [r for r in out if (not r["match_obs_x0"]) and r["match_obs_tz"]]
    brk = [r for r in out if r["match_obs_x0"] and (not r["match_obs_tz"])]
    print(f"## 1 ulp 反事实：修好 {len(fix)} 发 / 弄坏 {len(brk)} 发（净 {len(fix) - len(brk):+d}）")
    print("  修好：" + " ".join(f"{r['file']}#{r['batch']}" for r in fix))
    print("  弄坏：" + " ".join(f"{r['file']}#{r['batch']}" for r in brk))
    print()
    print(
        f"墙钟 {wall:.2f}s（jobs={a.jobs}；每任务 {per * 1000:.0f} ms；任务数 {len(sel)}）。"
        "\n加速比请与另一次 `--jobs 1` 的墙钟相除（本脚本每个任务 2 次 unicorn 调用，纯 CPU 活）。"
    )

    if a.tsv:
        with open(a.tsv, "w", encoding="utf-8") as f:
            f.write("# docs/06 §38 · corrected_function 含 NaN 的批（生成器 tools/nan_batches.py）\n")
            f.write("# 桶：both = 双方都 NaN（NaN 传播对）／obs = 只有回放 NaN／ours = 只有我方 NaN\n")
            f.write("# ① 真 0x3c5f70 喂报文 start_x 与我方一致；② 喂 start_x 向零 1 ulp 与回放一致\n")
            f.write(
                "file\tbatch\ttick\tshot\tbucket\tx0_bits\treal_at_start_x\treal_at_x0_minus_1ulp\t"
                "ours\tobs\tmatch_ours_x0\tmatch_obs_x0\tmatch_obs_tz\tobs_endx_eq_start_x\t"
                "obs_expl_eq_shooter\n"
            )
            for r in out:
                f.write(
                    f"{r['file']}\t{r['batch']}\t{r['tick']}\t{r['shot']}\t{r['bucket']}\t{r['x0']}\t"
                    f"{r['real_x0']}\t{r['real_tz']}\t{r['ours']}\t{r['obs']}\t"
                    f"{int(r['match_ours_x0'])}\t{int(r['match_obs_x0'])}\t{int(r['match_obs_tz'])}\t"
                    f"{int(r['deg_endx'])}\t{int(r['deg_expl'])}\n"
                )
        print(f"已写 {a.tsv}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
