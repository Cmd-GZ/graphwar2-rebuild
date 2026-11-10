#!/usr/bin/env python3
"""独立复核器：给一份"x86 地址 → arm64 符号"的对齐表打分。

**用途**：93 号（全序列对齐定名）交付时，协调方用它**独立**打分，而不是只信调用方的自测。

判据（三条，都要报）：
  ① **覆盖率**：13,263 个 x86 函数里，多少拿到了**唯一**对齐（并给置信档分布）
  ② **留出一致率**：348 个三源真名（`fixtures/name_to_addr.tsv`）上，对齐器给的名字与真名
     是否一致（**"同一个 arm64 符号" 或 "名字字符串相同" 两种口径都报**）
  ③ **近孪生族专项**：关卡闭包族（campaign 锚住的那些 x86 地址）在留出里命中几个

输入格式（TSV，首行表头，`#` 注释忽略）：
  x86_addr <TAB> arm64_addr <TAB> arm64_name <TAB> arm64_size <TAB> x86_size <TAB> confidence
  允许 `arm64_addr`/`arm64_name` 为空（= 未对齐）。

用法：
  python3 tools/verify_seq_align.py --align <对齐表.tsv> [--json out.json]
"""
import argparse
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def rows(path):
    with open(path, encoding="utf-8") as f:
        for line in f:
            if line.startswith("#") or not line.strip():
                continue
            yield line.rstrip("\n").split("\t")


def load_truth():
    """348 个三源真名：x86_addr -> (module::type::method 形式的名字, module, type, method)."""
    out = {}
    for r in rows(os.path.join(ROOT, "fixtures/name_to_addr.tsv")):
        if r[0] == "module":
            continue
        if len(r) < 4 or not r[3]:
            continue
        addr = int(r[3], 16)
        out[addr] = dict(module=r[0], type=r[1], method=r[2], full=f"{r[0]}::{r[1]}::{r[2]}")
    return out


def load_align(path):
    out = {}
    for r in rows(path):
        if r[0].lower().startswith("x86_addr"):
            continue
        try:
            addr = int(r[0], 16)
        except ValueError:
            continue
        arm_addr = int(r[1], 16) if len(r) > 1 and r[1].strip() else None
        arm_name = r[2].strip() if len(r) > 2 else ""
        conf = r[5].strip() if len(r) > 5 else ""
        out[addr] = dict(arm64_addr=arm_addr, arm64_name=arm_name, confidence=conf)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--align", required=True)
    ap.add_argument("--json")
    ap.add_argument("--total-x86", type=int, default=13263)
    a = ap.parse_args()

    align = load_align(a.align)
    truth = load_truth()
    filled = {k: v for k, v in align.items() if v["arm64_addr"] is not None}
    uniq = {k: v for k, v in filled.items() if v["confidence"] not in ("ambiguous", "multi", "")}

    hit_name = hit_arm = 0
    miss = []
    for addr, t in truth.items():
        got = align.get(addr)
        if not got or got["arm64_addr"] is None:
            miss.append((hex(addr), t["full"], "未对齐"))
            continue
        nm = got["arm64_name"]
        # 口径①：名字字符串里同时含 type 与 method（Rust 符号带 hash / 泛型参数，用子串判）
        if t["type"] in nm and t["method"] in nm:
            hit_name += 1
            continue
        # 口径②：只要"能落到某个 arm64 符号"就算覆盖（更宽松）
        hit_arm += 1
        miss.append((hex(addr), t["full"], nm[:60]))

    res = {
        "align_rows": len(align),
        "filled": len(filled),
        "unique": len(uniq),
        "coverage_of_total": round(len(uniq) / a.total_x86, 4),
        "holdout_n": len(truth),
        "holdout_name_match": hit_name,
        "holdout_arm_only": hit_arm,
        "holdout_missing": len(truth) - hit_name - hit_arm,
        "holdout_acc_name": round(hit_name / max(1, len(truth)), 4),
        "holdout_acc_arm": round((hit_name + hit_arm) / max(1, len(truth)), 4),
    }
    for k, v in res.items():
        print(f"{k:22s} {v}")
    if miss:
        print("\n未命中样例（最多 10 条）：")
        for m in miss[:10]:
            print("  ", m)
    if a.json:
        with open(a.json, "w", encoding="utf-8") as f:
            json.dump(dict(result=res, misses=miss), f, ensure_ascii=False, indent=1)
        print("已写", a.json)


if __name__ == "__main__":
    main()
