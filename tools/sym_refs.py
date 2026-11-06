#!/usr/bin/env python3
"""sym_refs.py — 找出「某版 .so 里谁引用了某个动态符号」。

用途：`.dynsym` 集合对比告诉你"新版多了哪些导入符号"，但那只说明它们被**解析**了。
要回答"新功能在哪"，还得找到引用该符号 GOT 槽的代码。

做法（纯静态，不用 Ghidra）：
  1. 在 `.dynsym` 里定位符号下标；
  2. 扫 `.rela.dyn` 找 `r_info` 符号下标相同、类型为 `R_X86_64_GLOB_DAT`(6) 的条目，
     `r_offset` 就是 GOT 槽的虚地址（`.plt`/`.rela.plt` 没变 ⇒ 不是走 PLT 调用，是 GOT 间接）；
  3. 用 FDE 表 + capstone 扫 `.text`，找 `[rip ± disp]` 正好等于该槽的指令，
     报出**包含函数的入口地址**（08-28 虚地址）。

用法：
    ../.venv/bin/python tools/sym_refs.py <so> <fde.txt 或留 - 自动 readelf> isatty _exit
"""
import os
import struct
import sys

from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86 import X86_OP_MEM, X86_REG_RIP

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import version_diff as vd  # noqa: E402

R_X86_64_GLOB_DAT = 6


def main():
    path = sys.argv[1]
    names = [a for a in sys.argv[2:] if not a.startswith("-")]
    d, secs, loads, _ = vd.elf_sections(path)
    ds = vd.sec_by_name(secs, ".dynsym")
    dstr = vd.sec_by_name(secs, ".dynstr")
    sym = {}
    for i in range(ds["size"] // 24):
        nm = struct.unpack_from("<I", d, ds["off"] + i * 24)[0]
        e = d.find(b"\0", dstr["off"] + nm)
        sym[d[dstr["off"] + nm:e].decode("utf-8", "replace")] = i

    rel = vd.sec_by_name(secs, ".rela.dyn")
    slots = {}
    for i in range(rel["size"] // 24):
        off, info, add = struct.unpack_from("<QQq", d, rel["off"] + i * 24)
        typ, si = info & 0xFFFFFFFF, info >> 32
        slots.setdefault(si, []).append((off, typ, add))

    for name in names:
        si = sym.get(name)
        print("=== %s  dynsym[%s]" % (name, si))
        if si is None:
            print("   不在 .dynsym 里")
            continue
        entries = slots.get(si, [])
        for off, typ, add in entries:
            print("   重定位：slot=0x%06x type=%d addend=0x%x" % (off, typ, add))
        got = [off for off, typ, _ in entries if typ == R_X86_64_GLOB_DAT]
        if not got:
            print("   没有 GLOB_DAT 槽（可能走 PLT）")
            continue
        cs = Cs(CS_ARCH_X86, CS_MODE_64)
        cs.detail = True
        ranges = vd.fde_ranges(path)
        hits = []
        for va, end in ranges:
            o, eo = vd.va2off(loads, va), vd.va2off(loads, end - 1)
            if o is None or eo is None:
                continue
            body = d[o:eo + 1]
            off_in = 0
            while off_in < len(body):
                i = next(cs.disasm(body[off_in:], va + off_in, count=1), None)
                if i is None:
                    off_in += 1
                    continue
                off_in = i.address - va + i.size
                for op in i.operands:
                    if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                        tgt = i.address + i.size + op.mem.disp
                        if tgt in got:
                            hits.append((va, i.address, "%s %s" % (i.mnemonic, i.op_str)))
                            break
        print("   引用该槽的指令 %d 条，落在 %d 个函数里："
              % (len(hits), len({h[0] for h in hits})))
        for va, at, txt in hits:
            print("      func=0x%06x  at=0x%06x  %s" % (va, at, txt))


if __name__ == "__main__":
    main()
