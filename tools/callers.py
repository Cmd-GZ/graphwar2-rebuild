#!/usr/bin/env python3
"""算某个函数被谁调用（纯本地，不消耗 MCP 载荷）。

比 MCP 的 procedure_callers 快得多，也不受响应体积限制：
用 capstone 索引里的 `call 0x...` 立即数目标，配合 Ghidra 的函数边界
（`analysis/funcstarts_ghidra_va.pkl`，虚地址空间），再查模块归属表。

用法：
    ../.venv/bin/python tools/callers.py 0x453a60 0x453c60
    ../.venv/bin/python tools/callers.py --module graphwar_life_manager.rs

注意：只统计**直接调用**；间接调用（函数指针、虚表）不在内。
"""

import bisect
import collections
import os
import pickle
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)

INSNS = os.path.join(ROOT, "analysis", "insns.pkl")
STARTS = os.path.join(ROOT, "analysis", "funcstarts_ghidra_va.pkl")
MODMAP = os.path.join(SUB, "fixtures", "module_funcs.tsv")


def load():
    starts = sorted(pickle.load(open(STARTS, "rb")))
    modmap = {}
    if os.path.exists(MODMAP):
        for line in open(MODMAP, encoding="utf-8").read().splitlines()[1:]:
            if not line.strip():
                continue
            m, a = line.split("\t")
            modmap[int(a, 16)] = m
    return starts, modmap


def owner(starts, addr):
    i = bisect.bisect_right(starts, addr) - 1
    return starts[i] if i >= 0 else None


def main(argv):
    if not argv:
        print(__doc__)
        return 1
    starts, modmap = load()

    if argv[0] == "--module":
        module = argv[1]
        targets = {a for a, m in modmap.items() if m == module}
        if not targets:
            print("没有该模块的函数记录:", module)
            return 1
    else:
        targets = {int(a, 0) for a in argv}

    insns = pickle.load(open(INSNS, "rb"))
    callers = collections.defaultdict(set)
    for addr, (mnem, op_str, _sz, _b) in insns.items():
        if not mnem.startswith("call") or not op_str.startswith("0x"):
            continue
        try:
            tgt = owner(starts, int(op_str, 16))
        except ValueError:
            continue
        if tgt not in targets:
            continue
        caller = owner(starts, addr)
        if caller is not None and caller != tgt:
            callers[tgt].add(caller)

    tally = collections.Counter()
    for tgt, cs in callers.items():
        for c in cs:
            tally[c] += 1

    print("目标函数 %d 个，外部调用者 %d 个：" % (len(targets), len(tally)))
    for c, n in tally.most_common():
        print("  0x%08x  %2d 次  %s" % (c, n, modmap.get(c, "（未归属）")))
    print("\n注：仅直接调用；间接调用不可见。")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
