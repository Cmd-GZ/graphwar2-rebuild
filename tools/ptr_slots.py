#!/usr/bin/env python3
"""查"谁通过函数指针引用了某个函数"。

直接调用图在 Rust 里经常断掉（`dyn Trait`、闭包、回调都经指针槽）。
本脚本走另一条路：

  1. 在 `.data.rel.ro` 的 `R_X86_64_RELATIVE` 重定位里找 addend == 目标函数的槽位；
  2. 用 capstone 交叉引用找"谁引用了这个槽"；
  3. 把使用者归到模块。

用法：
    ../.venv/bin/python tools/ptr_slots.py 0x3c2bc0 0x3c2530

盲区：经寄存器/结构体字段传递的函数指针不在静态槽位里，看不到。
"""

import bisect
import collections
import os
import pickle
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)


def load():
    relocs = pickle.load(open(os.path.join(ROOT, "analysis", "relocs.pkl"), "rb"))
    xrefs = pickle.load(open(os.path.join(ROOT, "analysis", "xrefs.pkl"), "rb"))
    starts = sorted(pickle.load(open(os.path.join(ROOT, "analysis", "funcstarts_ghidra_va.pkl"), "rb")))
    modmap = {}
    p = os.path.join(SUB, "fixtures", "module_funcs.tsv")
    if os.path.exists(p):
        for line in open(p, encoding="utf-8").read().splitlines()[1:]:
            if line.strip():
                m, a = line.split("\t")
                modmap[int(a, 16)] = m
    return relocs, xrefs, starts, modmap


def owner(starts, addr):
    i = bisect.bisect_right(starts, addr) - 1
    return starts[i] if i >= 0 else None


def main(argv):
    if not argv:
        print(__doc__)
        return 1
    targets = {int(a, 0): a for a in argv}
    relocs, xrefs, starts, modmap = load()

    slots_of = collections.defaultdict(list)
    for slot, entries in relocs.items():
        for typ, addend in entries:
            if typ == 8 and addend in targets:
                slots_of[addend].append(slot)

    for addr, text in sorted(targets.items()):
        slots = slots_of.get(addr, [])
        users = set()
        for s in slots:
            for insn, _k, _m, _o in xrefs.get(s, ()):
                o = owner(starts, insn)
                if o:
                    users.add(o)
        print("%s (0x%x): 指针槽 %d 个, 使用者 %d 个" % (text, addr, len(slots), len(users)))
        for u in sorted(users):
            print("   0x%08x  %s" % (u, modmap.get(u, "（未归属）")))
    print("\n注：未观测到引用 ≠ 没有引用（寄存器传递的指针不可见）。")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
