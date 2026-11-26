#!/usr/bin/env python3
"""T14（`docs/00` §3「明确未知」）探针：一次扫描，多种查询。

四条任务共用同一套 ELF + capstone 基础设施（`tools/loc_xrefs.py` 的 `Elf` / `LocXrefs`，
**不依赖上游 `analysis/*.pkl`**，那些文件在本机不存在）。

子命令
------
    strlit  '<text>'                  # `.rodata` 里这个字符串字面量的虚地址（可能多处）
    strrefs '<text>'                  # 谁引用这些字面量（R/I/G 三形态）
    slots   --addend 0x…              # `.rela.dyn` 里 addend==X 的槽（函数指针槽 / GOT 槽）
    slotuse 0x…                       # 谁引用这个槽（含 `call *0x…(%rip)` 与 `mov slot,%reg`+`call *%reg`）
    fn      0x…  [--dis N]            # 函数起点/长度/来源/Location/直接调用者/经槽调用者/被调者
    loc     <file.rs>                 # 该源文件 Location 覆盖的全部函数（按地址）
    dis     0x… --len 0x…             # 反汇编（带函数归属与 rip 目标注解）
    qword   0x… --n 8                 # 读数据
    strx    0x…                       # 读 .rodata 字符串（到 NUL 或 len）

用法（仓库约定 venv）：

    GW2_TARGET_SO=<.so> ../.reference/venv/bin/python tools/unknowns_probe.py fn 0x4b2890

口径：本工具打印的地址**全部是 ELF 虚地址**；Ghidra 地址 = +0x100000。
"""

import argparse
import collections
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def _ix(verbose=True):
    import loc_xrefs
    return loc_xrefs.LocXrefs(so=os.environ.get("GW2_TARGET_SO"), verbose=verbose)


# ---------------------------------------------------------------------------
# 字面量 → 引用
# ---------------------------------------------------------------------------
def find_strlits(ix, text, limit=40):
    """在 `.rodata` 里找 `text` 的每个出现位置（返回虚地址列表）。

    不要求 NUL 结尾，也不要求对齐 —— Rust 的 `&str` 字面量是 (ptr,len) 形式。
    """
    ro = ix.elf.sec(".rodata")
    blob = ix.data[ro["offset"]:ro["offset"] + ro["size"]]
    needle = text.encode()
    out, p = [], blob.find(needle)
    while p >= 0 and len(out) < limit:
        out.append(ro["addr"] + p)
        p = blob.find(needle, p + 1)
    return out


def read_cstr(ix, va, maxlen=200):
    ro = ix.elf.sec(".rodata")
    if not (ro["addr"] <= va < ro["addr"] + ro["size"]):
        return None
    b = ix.data[ro["offset"] + (va - ro["addr"]):][:maxlen]
    z = b.find(b"\x00")
    return (b[:z] if z >= 0 else b).decode("utf-8", "replace")


def cmd_strlit(ix, a):
    for va in find_strlits(ix, a.text, limit=a.limit):
        print("%#x  %r  (ctx %r)" % (va, read_cstr(ix, va, 120), read_cstr(ix, max(va - 40, 0), 200)))
    return 0


def cmd_strrefs(ix, a):
    """谁引用了这个字符串字面量（R=rip 相对 / I=立即数 / G=寄存器携带）。"""
    lits = find_strlits(ix, a.text, limit=a.limit)
    if not lits:
        print("（`.rodata` 里没有 %r）" % a.text)
        return 1
    for va in lits:
        refs = ix.refs_to(va)
        print("LIT %#x %r -> %d 处引用" % (va, a.text, len(refs)))
        byfunc = collections.defaultdict(list)
        for insn, form, mn, op in refs:
            byfunc[ix.func_of(insn)].append((insn, form, mn, op))
        for fn in sorted(x for x in byfunc if x is not None):
            print("  FUNC %#x  (span %#x)  source=%s" % (
                fn, (ix.func_end(fn) - fn) if ix.func_end(fn) else 0, ix.func_source(fn)))
            for insn, form, mn, op in byfunc[fn]:
                print("    %#x [%s] %s %s" % (insn, form, mn, op))
        other = byfunc.get(None, [])
        if other:
            print("  （%d 处不在任何已知函数内）" % len(other))
    return 0


# ---------------------------------------------------------------------------
# 槽（.rela.dyn R_X86_64_RELATIVE）
# ---------------------------------------------------------------------------
def slots_for_addend(ix, addend):
    return sorted(s for s, ad in ix.relocs.items() if ad == addend)


def cmd_slots(ix, a):
    addend = int(a.addend, 0)
    sl = slots_for_addend(ix, addend)
    print("addend %#x -> %d 个槽" % (addend, len(sl)))
    for s in sl:
        sec = "?"
        for lo, hi, name in ix.data_ranges:
            if lo <= s < hi:
                sec = name
        print("  SLOT %#x (%s)" % (s, sec))
    return 0


def cmd_slotuse(ix, a):
    slot = int(a.target, 0)
    refs = ix.refs_to(slot)
    print("SLOT %#x -> %d 处引用" % (slot, len(refs)))
    for insn, form, mn, op in refs:
        fn = ix.func_of(insn)
        print("  %#x [%s] %-6s %-40s in FUNC %#x" % (
            insn, form, mn, op, fn if fn is not None else 0))
    return 0


def callers_of(ix, target):
    """直接 `call rel32` + 经函数指针槽（含 mov slot→reg 再 call *reg）。"""
    direct = sorted(ix.calls.get(target, ()))
    indirect = collections.defaultdict(list)     # slot -> [insn]
    for slot in slots_for_addend(ix, target):
        for insn, form, mn, op in ix.refs_to(slot):
            if mn == "call" or mn in ("mov", "lea"):
                indirect[slot].append((insn, form, mn, op))
    return direct, indirect


def cmd_fn(ix, a):
    target = int(a.addr, 0)
    fn = target if (ix.func_source(target) != "?" or target in ix.start_set) else ix.func_of(target)
    if fn is None:
        print("%#x 不在任何已知函数内" % target)
        return 1
    end = ix.func_end(fn)
    print("FUNC %#x  span %#x (%d B)  source=%s  in_fde_gap=%s"
          % (fn, (end - fn) if end else 0, (end - fn) if end else 0,
             ix.func_source(fn), ix.in_gap(fn)))
    locs = ix.locs_of_func(fn)
    if locs:
        for (f, line, col), hits in sorted(locs.items()):
            print("  LOC %s:%d:%d  <- %s" % (f, line, col,
                  ", ".join("%#x[%s]" % (h[0], h[1]) for h in hits[:6])))
    else:
        print("  LOC （无 panic::Location 引用）")
    direct, indirect = callers_of(ix, fn)
    print("  CALLERS direct=%d" % len(direct))
    for c in direct:
        print("    %#x in FUNC %#x" % (c, ix.func_of(c) or 0))
    print("  CALLERS indirect(slot)=%d" % len(indirect))
    for slot, hits in sorted(indirect.items()):
        print("    SLOT %#x:" % slot)
        for insn, form, mn, op in hits:
            print("      %#x [%s] %s %s in FUNC %#x" % (insn, form, mn, op, ix.func_of(insn) or 0))
    if a.dis:
        dis(ix, fn, min(end, fn + a.dis) if end else fn + a.dis, fn)
    return 0


# ---------------------------------------------------------------------------
# Location → 函数
# ---------------------------------------------------------------------------
def cmd_loc(ix, a):
    pat = a.file
    rows = collections.defaultdict(set)
    for slot, (f, line, col) in ix.locs.items():
        if pat not in f:
            continue
        for fn in ix.funcs_of_loc(slot):
            rows[fn].add((f, line, col))
    print("%d 个函数引用了含 %r 的 Location" % (len(rows), pat))
    for fn in sorted(rows):
        info = sorted(rows[fn])
        print("  %#x  span %#x  %s" % (
            fn, (ix.func_end(fn) - fn) if ix.func_end(fn) else 0,
            "; ".join("%s:%d:%d" % t for t in info)))


# ---------------------------------------------------------------------------
# 反汇编
# ---------------------------------------------------------------------------
def dis(ix, start, end, fnbase=None):
    import capstone
    from capstone import x86 as cx
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.detail = True
    md.syntax = capstone.CS_OPT_SYNTAX_INTEL
    blob = ix.elf.read(start, end - start)
    for ins in md.disasm(blob, start):
        note = ""
        for op in ins.operands:
            if op.type == cx.X86_OP_MEM and op.mem.base == cx.X86_REG_RIP:
                tgt = ins.address + ins.size + op.mem.disp
                note = "   ; -> %#x" % tgt
                if tgt in ix.locs:
                    note += "  LOC %s:%d" % (ix.locs[tgt][0], ix.locs[tgt][1])
                elif tgt in ix.string_slots:
                    note += "  STR %r" % ix.string_slots[tgt][2][:60]
                elif tgt in ix.relocs:
                    note += "  RELA-> %#x" % ix.relocs[tgt]
                    if ix.relocs[tgt] in ix.locs:
                        note += "  LOC %s:%d" % (ix.locs[ix.relocs[tgt]][0],
                                                 ix.locs[ix.relocs[tgt]][1])
            elif op.type == cx.X86_OP_IMM and ix.elf is not None and op.imm > 0x1000:
                if op.imm in ix.loc_hit:
                    note = "   ; LOC %s:%d" % tuple(ix.locs[ix.loc_hit[op.imm]][:2])
        print("%#x  %-8s %-46s%s" % (ins.address, ins.mnemonic, ins.op_str, note))
    return 0


def cmd_dis(ix, a):
    start = int(a.target, 0)
    length = int(a.len, 0) if a.len else 0x100
    return dis(ix, start, start + length)


# ---------------------------------------------------------------------------
# 引擎方法链：字面量 → 包装 → GOT 槽 → 调用点
# ---------------------------------------------------------------------------
def _pairs_in_func(ix, fn, cap=0x600):
    """在函数体里找 `(&str, len)` 对（gdext 的 (类名, 方法名) 常量对）。

    形态（0x8abca0 = `draw_arc` 的包装，实测）：

        0x8abd39 lea rax, [rip-0x60ce13]        ; -> 0x29ef2d 'CanvasItem'
        0x8abd40 mov [rsp+0x48], rax
        0x8abd45 mov qword ptr [rsp+0x50], 0xa  ; len = 10
        0x8abd4e lea rax, [rip-0x70c385]        ; -> 0x19f9d0 'draw_arc'
        0x8abd55 mov [rsp+0x58], rax
        0x8abd5a mov qword ptr [rsp+0x60], 8    ; len = 8
    """
    import capstone
    from capstone import x86 as cx
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.detail = True
    md.syntax = capstone.CS_OPT_SYNTAX_INTEL
    end = ix.func_end(fn) or (fn + cap)
    blob = ix.elf.read(fn, min(end - fn, cap))
    if blob is None:
        return []
    reg2addr = {}
    ptrs = {}          # (base, disp) -> (addr, insn)
    lens = {}          # (base, disp) -> (len, insn)
    out = []
    for ins in md.disasm(blob, fn):
        ops = ins.operands
        if ins.mnemonic == "lea" and len(ops) == 2 and ops[0].type == cx.X86_OP_REG \
                and ops[1].type == cx.X86_OP_MEM and ops[1].mem.base == cx.X86_REG_RIP:
            tgt = ins.address + ins.size + ops[1].mem.disp
            ro = ix.elf.sec(".rodata")
            if ro["addr"] <= tgt < ro["addr"] + ro["size"]:
                reg2addr[ops[0].reg] = tgt
            else:
                reg2addr.pop(ops[0].reg, None)
        elif ins.mnemonic == "mov" and len(ops) == 2 and ops[0].type == cx.X86_OP_MEM:
            m = ops[0].mem
            if m.base == cx.X86_REG_RSP and ops[1].type == cx.X86_OP_REG and ops[1].reg in reg2addr:
                ptrs[(m.base, m.disp)] = (reg2addr[ops[1].reg], ins.address)
            elif m.base == cx.X86_REG_RSP and ops[1].type == cx.X86_OP_IMM \
                    and 0 < ops[1].imm < 4096:
                lens[(m.base, m.disp)] = (ops[1].imm, ins.address)
        elif ins.mnemonic.startswith(("call", "ret")):
            # 调用之后栈槽重写，剩下的对归下一段（仍然保留，gdext 包装通常一次调用）
            pass
        # 其它写 reg 的操作作废污点
        if ops and ops[0].type == cx.X86_OP_REG and ins.mnemonic not in ("lea", "mov", "movzx",
                                                                        "movsx", "movsxd"):
            reg2addr.pop(ops[0].reg, None)
    for (base, disp), (addr, pins) in sorted(ptrs.items(), key=lambda kv: kv[1][1]):
        ln = lens.get((base, disp + 8))
        if not ln:
            continue
        txt = read_cstr(ix, addr, 200)
        if txt is None:
            continue
        # 字面量是打包存放的，长度以 len 立即数为准
        ro = ix.elf.sec(".rodata")
        raw = ix.elf.read(addr, ln[0])
        if raw is None:
            continue
        out.append((pins, addr, ln[0], raw.decode("latin-1"), ln[1]))
    return out


def _slot_use_summary(ix, fn):
    """函数指针槽 → 调用点（两种形态都算）。"""
    rows = []
    for slot in slots_for_addend(ix, fn):
        for insn, form, mn, op in ix.refs_to(slot):
            rows.append((slot, insn, form, mn, op, ix.func_of(insn)))
    return rows


def cmd_chain(ix, a):
    """`chain <method_name>`：字面量 → 包装 → GOT 槽 → 调用点 全链。"""
    name = a.name
    lits = find_strlits(ix, name, limit=a.limit)
    if not lits:
        print("❌ `.rodata` 里没有 %r（全文件字节扫描见 tools/plot_prim_probe.py）" % name)
        return 1
    for lit in lits:
        print("=" * 78)
        print("LIT %#x  %r" % (lit, name))
        refs = ix.refs_to(lit)
        byfn = collections.defaultdict(list)
        for insn, form, mn, op in refs:
            byfn[ix.func_of(insn)].append((insn, form, mn, op))
        if not refs:
            print("  （无代码引用 —— 这个字面量只被数据引用 / 或未被使用）")
        for fn in sorted(x for x in byfn if x is not None):
            print("  WRAP? %#x span %#x source=%s" % (fn, (ix.func_end(fn) or 0) - fn,
                                                      ix.func_source(fn)))
            for insn, form, mn, op in byfn[fn]:
                print("    ref %#x [%s] %s %s" % (insn, form, mn, op))
            for pins, addr, ln, txt, linsn in _pairs_in_func(ix, fn):
                mark = " <<<" if addr == lit else ""
                print("    PAIR (%r, len=%d)  ptr@%#x len@%#x%s" % (txt, ln, pins, linsn, mark))
            rows = _slot_use_summary(ix, fn)
            if not rows:
                print("    SLOT: 无（不是 GOT 槽派发）")
            for slot, insn, form, mn, op, owner in rows:
                print("    SLOT %#x [%s] %#x %s %s   in FUNC %#x"
                      % (slot, form, insn, mn, op, owner or 0))
        others = byfn.get(None, [])
        if others:
            print("  （%d 处引用不在已知函数内）" % len(others))
    return 0


def cmd_wrap(ix, a):
    """`wrap <addr>`：把某个 gdext 包装函数里的 (类名, 方法名) 常量对打印出来。"""
    fn = ix.func_of(int(a.addr, 0))
    if fn is None:
        print("不在任何已知函数内")
        return 1
    print("FUNC %#x span %#x" % (fn, (ix.func_end(fn) or 0) - fn))
    for pins, addr, ln, txt, linsn in _pairs_in_func(ix, fn):
        print("  PAIR (%r, len=%d) ptr@%#x len@%#x" % (txt, ln, pins, linsn))
    for slot, insn, form, mn, op, owner in _slot_use_summary(ix, fn):
        print("  SLOT %#x [%s] %#x %s %s in FUNC %#x" % (slot, form, insn, mn, op, owner or 0))
    return 0


def cmd_qword(ix, a):
    va = int(a.target, 0)
    n = a.n
    for i in range(n):
        v = ix.elf.u64(va + 8 * i)
        extra = ""
        if v is not None:
            if v in ix.locs:
                extra = "LOC %s:%d" % ix.locs[v][:2]
            elif v in ix.string_slots:
                extra = "STR %r" % ix.string_slots[v][2][:80]
        print("%#x: %#018x %s" % (va + 8 * i, v if v else 0, extra))
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("strlit"); p.add_argument("text"); p.add_argument("--limit", type=int, default=40)
    p = sub.add_parser("strrefs"); p.add_argument("text"); p.add_argument("--limit", type=int, default=40)
    p = sub.add_parser("slots"); p.add_argument("--addend", required=True)
    p = sub.add_parser("slotuse"); p.add_argument("target")
    p = sub.add_parser("fn"); p.add_argument("addr"); p.add_argument("--dis", type=lambda s: int(s, 0), default=0)
    p = sub.add_parser("loc"); p.add_argument("file")
    p = sub.add_parser("dis"); p.add_argument("target"); p.add_argument("--len", default=None)
    p = sub.add_parser("qword"); p.add_argument("target"); p.add_argument("--n", type=int, default=8)
    p = sub.add_parser("chain"); p.add_argument("name"); p.add_argument("--limit", type=int, default=8)
    p = sub.add_parser("wrap"); p.add_argument("addr")

    a = ap.parse_args(argv)
    ix = _ix()
    return {
        "strlit": cmd_strlit, "strrefs": cmd_strrefs, "slots": cmd_slots,
        "slotuse": cmd_slotuse, "fn": cmd_fn, "loc": cmd_loc, "dis": cmd_dis,
        "qword": cmd_qword, "chain": cmd_chain, "wrap": cmd_wrap,
    }[a.cmd](ix, a)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
