#!/usr/bin/env python3
"""按浮点常量反查"谁在用它"。

思路：坐标变换、缩放、网格都会引用 50.0 / 30.0 / 25.0 / 15.0 这类常量。
与其在 960 行的函数里翻，不如从常量出发找引用点，再归到函数。

三类引用都要看：
  1. 指令里直接 rip-relative 引用该常量地址；
  2. `.data.rel.ro` 里指向它的指针槽，以及引用该槽的指令；
  3. 常量被搬进寄存器时的 `movabs` 立即数（此时常量不落在 .rodata）。

函数边界用 Ghidra 的表（`analysis/funcstarts_ghidra_va.pkl`，虚地址空间）。

用法：
    ../.venv/bin/python tools/const_xrefs.py 50.0 30.0 25.0 15.0
"""

import bisect
import os
import pickle
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)
sys.path.insert(0, os.path.join(ROOT, "analysis"))

from elf import Elf                      # noqa: E402

SO = os.path.join(ROOT, "libgraphwarrust.so")


def load():
    starts = sorted(pickle.load(open(os.path.join(ROOT, "analysis", "funcstarts_ghidra_va.pkl"), "rb")))
    xrefs = pickle.load(open(os.path.join(ROOT, "analysis", "xrefs.pkl"), "rb"))
    relocs = pickle.load(open(os.path.join(ROOT, "analysis", "relocs.pkl"), "rb"))
    return starts, xrefs, relocs


def owner(starts, addr):
    i = bisect.bisect_right(starts, addr) - 1
    return starts[i] if i >= 0 else None


def main(argv):
    values = [float(a) for a in argv] or [50.0, 30.0, 25.0, 15.0]
    starts, xrefs, relocs = load()
    E = Elf(SO)
    data = E.data
    secs = {s.get("sname"): s for s in E.sections if s.get("size") and s.get("addr")}

    # 目标地址 -> 常量名
    slots_of = {}
    for slot, entries in relocs.items():
        for (typ, addend) in entries:
            if typ == 8:
                slots_of.setdefault(addend, []).append(slot)

    for v in values:
        pat = struct.pack("<d", v)
        found = []
        for name, s in secs.items():
            blob = data[s["offset"]:s["offset"] + s["size"]]
            pos = blob.find(pat)
            while pos >= 0:
                found.append((s["addr"] + pos, name))
                pos = blob.find(pat, pos + 1)
        print(f"\n=== {v} ({len(found)} 处出现) ===")
        total_refs = 0
        funcs = {}
        for addr, secname in found:
            refs = list(xrefs.get(addr, []))
            for slot in slots_of.get(addr, ()):
                refs.extend(xrefs.get(slot, []))
            for insn_addr, kind, mnem, ops in refs:
                f = owner(starts, insn_addr)
                if f is None:
                    continue
                total_refs += 1
                funcs.setdefault(f, []).append((insn_addr, secname, mnem, ops))
        for f in sorted(funcs):
            print("  0x%08x (%d 次)" % (f, len(funcs[f])))
        print("  引用合计 %d 次，覆盖 %d 个函数" % (total_refs, len(funcs)))


if __name__ == "__main__":
    main(sys.argv[1:])
