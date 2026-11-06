#!/usr/bin/env python3
"""func_refs.py — 一个函数到底碰了哪些东西：直接调用、RIP 相对引用、以及谁引用了某地址。

为什么需要：`.dynsym` 只说"这个导入被解析了"，`.rela.dyn` 只说"这个槽指向那段 rodata"，
都回答不了"新功能的代码在哪、它用了哪些字面量"。本脚本补这一环，纯静态、不用 Ghidra。

三种模式：
    # 1) 这个函数调用/引用了什么（地址一律 08-28 虚地址）
    ../.venv/bin/python tools/func_refs.py <so> func 0x3b5290

    # 2) 谁引用了这个地址（找字面量 / Location 结构体的使用点）
    ../.venv/bin/python tools/func_refs.py <so> refs 0x1a0f50

    # 3) 在 .rela.dyn 里找指向某字面量的槽（得到 panic::Location 的 {ptr,len} 槽）
    ../.venv/bin/python tools/func_refs.py <so> slot 0x1a0f50

Rust 的 `&str` 是 (ptr,len)：`.data.rel.ro` 里 `R_X86_64_RELATIVE` 的 addend 是 ptr、
槽位 +8 就是 len。所以只要槽位可读，就能把 `[rip+X]` 精确还原成字面量（不受串表重排影响）。
`panic::Location` 是 {file.ptr, file.len, line:u32, col:u32} 四元组，跟在 (ptr,len) 槽后面。
"""
import os
import struct
import sys

from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86 import X86_OP_MEM, X86_REG_RIP

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import version_diff as vd  # noqa: E402


def sections_of(path):
    d, secs, loads, _ = vd.elf_sections(path)
    return d, secs, loads


def reloc_maps(path):
    """返回 (ptr_slot -> addend, addend -> [slot...]) 便于双向查找。"""
    d, secs, loads = sections_of(path)
    rel = vd.sec_by_name(secs, ".rela.dyn")
    slot2add, add2slot = {}, {}
    for i in range(rel["size"] // 24):
        off, info, add = struct.unpack_from("<QQq", d, rel["off"] + i * 24)
        if (info & 0xFFFFFFFF) != vd.R_X86_64_RELATIVE:
            continue
        slot2add[off] = add
        add2slot.setdefault(add, []).append(off)
    return d, secs, loads, slot2add, add2slot


def literal_at(d, secs, loads, slot, slot2add):
    """槽位 -> 精确边界的 &str（失败的返回 None）。"""
    add = slot2add.get(slot)
    if add is None:
        return None
    so = vd.va2off(loads, slot + 8)
    if so is None:
        return None
    ln = struct.unpack_from("<Q", d, so)[0]
    p = vd.va2off(loads, add)
    if not (0 < ln <= 4096) or p is None or p + ln > len(d):
        return None
    return d[p:p + ln]


def func_bounds(path):
    d, secs, loads = sections_of(path)
    out = []
    for va, end in vd.fde_ranges(path):
        o, eo = vd.va2off(loads, va), vd.va2off(loads, end - 1)
        if o is None or eo is None:
            continue
        out.append((va, end, d[o:eo + 1]))
    return out


def mode_func(path, va):
    d, secs, loads, slot2add, add2slot = reloc_maps(path)
    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    for fva, fend, body in func_bounds(path):
        if fva != va:
            continue
        print("函数 0x%06x..0x%06x  (%d 字节)" % (fva, fend, len(body)))
        calls, refs = [], []
        off = 0
        while off < len(body):
            i = next(cs.disasm(body[off:], fva + off, count=1), None)
            if i is None:
                off += 1
                continue
            off = i.address - fva + i.size
            if i.mnemonic.startswith("call") or i.mnemonic.startswith("jmp"):
                for op in i.operands:
                    if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                        calls.append((i.address, "indirect@0x%06x" % (i.address + i.size + op.mem.disp)))
            for op in i.operands:
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    tgt = i.address + i.size + op.mem.disp
                    lit = literal_at(d, secs, loads, tgt, slot2add)
                    refs.append((i.address, tgt, lit, "%s %s" % (i.mnemonic, i.op_str)))
        print("  间接调用 %d 条：" % len(calls))
        for at, t in calls:
            print("    at=0x%06x -> %s" % (at, t))
        print("  RIP 相对引用 %d 条：" % len(refs))
        for at, tgt, lit, txt in refs:
            extra = ""
            if lit is not None:
                extra = "  literal=%r" % lit.decode("utf-8", "replace")
            else:
                p = vd.va2off(loads, tgt)
                if p is not None:
                    raw = d[p:p + 24]
                    extra = "  raw=%r" % raw
            print("    at=0x%06x -> 0x%06x  %-46s%s" % (at, tgt, txt, extra))
        return
    print("0x%06x 不是 FDE 表里的函数入口" % va)


def mode_refs(path, target):
    d, secs, loads = sections_of(path)
    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    hits = []
    for fva, fend, body in func_bounds(path):
        off = 0
        while off < len(body):
            i = next(cs.disasm(body[off:], fva + off, count=1), None)
            if i is None:
                off += 1
                continue
            off = i.address - fva + i.size
            for op in i.operands:
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    if i.address + i.size + op.mem.disp == target:
                        hits.append((fva, i.address, "%s %s" % (i.mnemonic, i.op_str)))
    print("引用 0x%06x 的指令 %d 条，落在 %d 个函数：" % (target, len(hits), len({h[0] for h in hits})))
    for fva, at, txt in hits:
        print("  func=0x%06x at=0x%06x  %s" % (fva, at, txt))


def mode_slot(path, target):
    d, secs, loads, slot2add, add2slot = reloc_maps(path)
    slots = add2slot.get(target, [])
    print("指向 0x%06x 的 .rela.dyn RELATIVE 槽 %d 个：" % (target, len(slots)))
    for s in slots:
        lit = literal_at(d, secs, loads, s, slot2add)
        print("  slot=0x%06x  len=%r" % (s, lit))
        so = vd.va2off(loads, s)
        if so is not None:
            print("    槽附近原始字节：%r" % d[so:so + 48])


if __name__ == "__main__":
    path, mode, addr = sys.argv[1], sys.argv[2], int(sys.argv[3], 16)
    {"func": mode_func, "refs": mode_refs, "slot": mode_slot}[mode](path, addr)
