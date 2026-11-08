#!/usr/bin/env python3
"""effect_info_pos.py —— `EffectInfo`(tag 44) 的 `pos_x`/`pos_y` 到底是哪条指令写的、从哪读的。

为什么需要：§33 已经把"回放里的 `pos_y` 不是采样器输出"钉住了（真二进制用同一组输入给出
`…845`，回放记 `…844`）。下一步必须回答"报文装配时这个字段从哪来"——
用 §30 的成功套路：**先把写这个字段的指令全列出来 → 按语义筛 → 再谈运行时**。

用法（地址一律 08-28 构建的**虚地址**，`libgraphwarrust.so` sha256 `9a538115…`）：

    # ① tag44（= EffectInfo）的所有生产者：判别式 `0x8000000000000000|44` 的构造点
    ../.venv/bin/python tools/effect_info_pos.py SO tags

    # ② 某个函数里所有写 `[rsp+disp, disp+size)` 的指令（disp 用十六进制）
    ../.venv/bin/python tools/effect_info_pos.py SO writes 0x472865 0x80 8

    # ③ 直接看一段反汇编（给 start_va 与字节数）
    ../.venv/bin/python tools/effect_info_pos.py SO dis 0x472930 0x60

    # ④ 追一个栈槽的"最后一次写"链：谁写了 `[rsp+0xf0]`（16 字节点缓存）
    ../.venv/bin/python tools/effect_info_pos.py SO slot 0x472865 0xf0 16

判别式怎么认（Rust `enum` 变体标记，见 docs/06 §28.1）：
    变体 v 的记录开头 = 0x8000000000000000 | v；编译器通常写成
    `movabs rax,0x7fffffffffffffff` + `add rax, v+1`（本脚本按这个算 v），
    或 `movabs r8,0x8000000000000000` + `lea rax,[r8+v]`。
"""

import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import version_diff as vd  # noqa: E402
from capstone import Cs, CS_ARCH_X86, CS_MODE_64  # noqa: E402
from capstone.x86 import (  # noqa: E402
    X86_OP_IMM, X86_OP_MEM, X86_OP_REG, X86_REG_RSP, X86_REG_RBP,
)

TAG_BIT = 0x8000000000000000
U64 = (1 << 64) - 1


class So:
    def __init__(self, path):
        self.path = path
        self.d, self.secs, self.loads, _ = vd.elf_sections(path)
        self.text = vd.sec_by_name(self.secs, ".text")
        self.md = Cs(CS_ARCH_X86, CS_MODE_64)
        self.md.detail = True
        self.fdes = vd.fde_ranges(path)

    def func_of(self, va):
        for a, b in self.fdes:
            if a <= va < b:
                return a, b
        return None

    def dis(self, va, n):
        o = vd.va2off(self.loads, va)
        return list(self.md.disasm(self.d[o:o + n], va))

    def func_dis(self, va):
        fb = self.func_of(va)
        if fb is None:
            raise SystemExit("0x%x 不在任何 FDE 里" % va)
        a, b = fb
        return fb, self.dis(a, b - a)


def fmt(i):
    return "%06x  %-20s %s %s" % (i.address, i.bytes.hex(), i.mnemonic, i.op_str)


def is_stack(op):
    return op.type == X86_OP_MEM and op.mem.base in (X86_REG_RSP, X86_REG_RBP)


def stack_span(op):
    """-> (disp, size) 相对 rsp（rbp 当 0 处理不了，直接跳过）"""
    if op.mem.base != X86_REG_RSP:
        return None
    disp = op.mem.disp
    if disp < 0:
        disp += 1 << 64
    return disp, op.size


def writes_to(insn, lo, hi):
    """这条指令写的栈字节区间是否与 [lo,hi) 相交 -> 相交区间 or None"""
    # 目标操作数
    if not insn.operands:
        return None
    dst = insn.operands[0]
    if not dst.access & 2:      # CS_AC_WRITE == 2
        return None
    if dst.type != X86_OP_MEM:
        return None
    d = stack_span(dst)
    if d is None:
        return None
    a, sz = d
    if a < hi and lo < a + sz:
        return (a, sz)
    return None


def mode_writes(so, va, lo, size):
    hi = lo + size
    (fa, fb), insns = so.func_dis(va)
    print("函数 [0x%06x, 0x%06x)  查询写 [rsp+0x%x, +0x%x)" % (fa, fb, lo, hi))
    n = 0
    for i in insns:
        w = writes_to(i, lo, hi)
        if w:
            n += 1
            print("  %s   [写 rsp+0x%x, %d 字节]" % (fmt(i), w[0], w[1]))
    print("共 %d 条" % n)


def mode_slot(so, va, disp, size):
    mode_writes(so, va, disp, size)


def mode_tags(so):
    """扫全 .text：所有"top-bit 置位"的判别式立即数 -> 变体号。

    做法是**直线窗口内的常量传播**：`movabs` 给寄存器定值，`add/lea/inc` 推，
    任何分支指令或分支目标都把状态清空（Rust 里判别式都是紧挨着 store 算的）。
    """
    text = so.text
    base = text["addr"]
    body = so.d[vd.va2off(so.loads, base):vd.va2off(so.loads, base) + text["size"]]
    insns = list(so.md.disasm(body, base))
    # 分支目标集合
    targets = set()
    for i in insns:
        if i.mnemonic.startswith("j"):
            for op in i.operands:
                if op.type == X86_OP_IMM:
                    targets.add(op.imm)
    known = {}      # reg -> value
    hits = []
    for i in insns:
        if i.address in targets:
            # 只有"从别处跳进来的"目标才清空；条件跳转的 fallthrough 继续沿用
            known.clear()
        ops = i.operands
        m, o = i.mnemonic, i.op_str
        # 先处理 store（用旧 known）
        if ops and ops[0].type == X86_OP_MEM and (ops[0].access & 2):
            src = ops[1] if len(ops) > 1 else None
            val = None
            if src is not None:
                if src.type == X86_OP_IMM:
                    val = src.imm & U64
                elif src.type == X86_OP_REG and src.reg in known:
                    val = known[src.reg]
            if val is not None and (val & TAG_BIT) and (val & (TAG_BIT - 1)) < 0x200:
                hits.append((val & (TAG_BIT - 1), i.address, fmt(i)))
        # 更新 known
        if m == "movabs" or (m == "mov" and len(ops) == 2 and ops[0].type == X86_OP_REG):
            if len(ops) == 2 and ops[0].type == X86_OP_REG:
                if ops[1].type == X86_OP_IMM:
                    known[ops[0].reg] = ops[1].imm & U64
                elif ops[1].type == X86_OP_REG and ops[1].reg in known:
                    known[ops[0].reg] = known[ops[1].reg]
                else:
                    known.pop(ops[0].reg, None)
        elif m == "add" and len(ops) == 2 and ops[0].type == X86_OP_REG:
            if ops[0].reg in known and ops[1].type == X86_OP_IMM:
                known[ops[0].reg] = (known[ops[0].reg] + ops[1].imm) & U64
            else:
                known.pop(ops[0].reg, None)
        elif m == "sub" and len(ops) == 2 and ops[0].type == X86_OP_REG:
            if ops[0].reg in known and ops[1].type == X86_OP_IMM:
                known[ops[0].reg] = (known[ops[0].reg] - ops[1].imm) & U64
            else:
                known.pop(ops[0].reg, None)
        elif m == "inc" and ops and ops[0].type == X86_OP_REG:
            if ops[0].reg in known:
                known[ops[0].reg] = (known[ops[0].reg] + 1) & U64
        elif m == "lea" and len(ops) == 2 and ops[0].type == X86_OP_REG:
            s = ops[1]
            if s.type == X86_OP_MEM and s.mem.base in known and not s.mem.index:
                known[ops[0].reg] = (known[s.mem.base] + s.mem.disp) & U64
            else:
                known.pop(ops[0].reg, None)
        else:
            # 其它指令：把它写的寄存器从 known 里删掉
            for op in ops:
                if op.type == X86_OP_REG and (op.access & 2):
                    known.pop(op.reg, None)
        # 只有"控制流真的离开这段直线代码"才清空：无条件 jmp / call / ret。
        # 条件跳转的 fallthrough 沿用寄存器值（否则会漏掉跨 `je` 的判别式构造）。
        if m == "jmp" or m.startswith("call") or m in ("ret", "loop"):
            known.clear()
    from collections import Counter
    c = Counter(h[0] for h in hits)
    print("扫到 %d 条判别式 store，%d 个不同变体" % (len(hits), len(c)))
    for v, n in sorted(c.items()):
        print("  变体 %3d (0x%02x)  ×%d" % (v, v, n))
    print()
    for v, at, txt in sorted(hits):
        if v == 44:
            print("  tag44 store: %s" % txt)


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    so = So(sys.argv[1])
    mode = sys.argv[2]
    if mode == "tags":
        mode_tags(so)
    elif mode == "writes":
        mode_writes(so, int(sys.argv[3], 16), int(sys.argv[4], 16), int(sys.argv[5], 0))
    elif mode == "slot":
        mode_slot(so, int(sys.argv[3], 16), int(sys.argv[4], 16), int(sys.argv[5], 0))
    elif mode == "dis":
        va = int(sys.argv[3], 16)
        n = int(sys.argv[4], 16)
        fb = so.func_of(va)
        print("函数 [0x%06x, 0x%06x)" % fb if fb else "（不在 FDE 里）")
        for i in so.dis(va, n):
            print("  " + fmt(i))
    elif mode == "func":
        va = int(sys.argv[3], 16)
        fb = so.func_of(va)
        print("0x%06x -> 函数 [0x%06x, 0x%06x)" % (va, fb[0], fb[1]) if fb else "不在 FDE 里")
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
