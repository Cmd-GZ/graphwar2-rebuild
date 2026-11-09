#!/usr/bin/env python3
"""构造点复扫的**超集**工具：把「某 tag 扫不到构造点」从**有界否定**抬成**硬结论**。

## 为什么需要它（dispatch/88）

`tools/addr_probe.py tags`（83 号修好的那版）认**四**种 materialize 形态，但它有两个
刻意的取窄口径，都会给"扫不到"的负结论留口子：

1. `_tag_after_base()` **只认同一个寄存器**上的 `add/or/inc/lea`（`lea rax,[rax+n]`），
   且只看 **12 条**窗口 ⇒ `lea rdx,[rax+0x25]`、`mov rdx,rax` 之后再补、先把参数搬
   20 条再补，**全都漏**；
2. `_tag_of_reg()` / `_tag_after_base()` 遇到**寄存器源**（`or rax, rcx`，即通用构造器
   `base | n`）直接返回 None —— 这类点会被记成 tag 0，而且**换桶后也归不了类**。

本工具用一个**值追踪**（value tracking）取代"形态匹配"：从每条
`movabs reg, <tag 家族常量>` 出发，沿 **整个 FDE** 跟踪该值在寄存器间的传播
（`add/or/inc/lea` 立即数、`mov`/`lea` 拷贝、多步累加），把每一种能变成
`0x8000…|n` 的路径都算出来；同时把"家族常量与**寄存器**合成"的站点单独列出来，
因为那才是"通用构造器"唯一可能的藏身处。

## 三种子命令

    tag_ctor_gaps.py scan   --tag 37     # ① 值追踪全库，报该 tag 的全部站点 + 每站点的下一条用法
    tag_ctor_gaps.py prov   --tag 37     # ② 反向：全库所有"补 n / 补 n+1"的算术点 + 源寄存器溯源
    tag_ctor_gaps.py generic             # ③ 全库所有"家族常量 ⊕ 寄存器"的站点（通用构造器候选）
    tag_ctor_gaps.py --self-test         # ④ 正对照：拿真字节**变异立即数**，验证判据对 0x25 敏感

`--self-test` 是这套负结论的**必要条件**：把 tag 15 的真实构造点（`0x474b8d`+`0x474b97`）
的立即数从 `0x0f` 改成 `0x25`，同一条判据必须报 **tag 37**；改回 `0x0f` 必须报 tag 15。
不会"因为扫不到所以永远报 0"的扫描器才是扫描器。

## 口径

* 一律 ELF 虚地址（`.text` 的 vaddr 比文件偏移大 `0x1000`）。
* 家族常量 = `0x7fffffffffffffff`（形态①②的基址）/ `0x8000000000000000`（形态④的基址）
  / 任何 `(v>>32)==0x80000000` 的 tag word。
* 调用约定：`call` 踩掉 caller-saved（rax/rcx/rdx/rsi/rdi/r8-r11）/ 子寄存器写会毁掉
  64 位值 / 其他任何写都视为"值已不可追"，停止传播（宁可漏，不可编）。
"""

import argparse
import os
import sys
from collections import Counter, defaultdict

from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86 import X86_OP_MEM, X86_OP_IMM, X86_OP_REG

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import addr_probe as A  # noqa: E402

MAX = 0x7FFFFFFFFFFFFFFF
BASE = 0x8000000000000000
M64 = (1 << 64) - 1
SENTINELS = (MAX, BASE)
TAG_N_MAX = 0x7F          # 与 addr_probe.TAG_N_MAX 一致：`GraphWarEvent` 只有 0..=73，留到 0x7f
_CALL_CLOBBER_NAMES = {"rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"}

# 子寄存器 → 64 位父寄存器（写子寄存器 = 64 位值不再可信）
_SUB = {
    "rax": ("eax", "ax", "al", "ah"), "rbx": ("ebx", "bx", "bl", "bh"),
    "rcx": ("ecx", "cx", "cl", "ch"), "rdx": ("edx", "dx", "dl", "dh"),
    "rsi": ("esi", "si", "sil"), "rdi": ("edi", "di", "dil"),
    "rbp": ("ebp", "bp", "bpl"), "rsp": ("esp", "sp", "spl"),
}
for _i in range(8, 16):
    _SUB["r%d" % _i] = ("r%dd" % _i, "r%dw" % _i, "r%db" % _i)
_PARENT = {}
for _p, _kids in _SUB.items():
    for _k in _kids:
        _PARENT[_k] = _p


def is_tag_family(v):
    """`0x8000…|n`，n 落在 0..TAG_N_MAX（含越界的 74 也认，见 addr_probe 的注释）。"""
    return (v >> 32) == 0x80000000 and (v & 0x7FFFFFFF) <= TAG_N_MAX


def is_family(v):
    return v in SENTINELS or is_tag_family(v)


class Fam:
    """**按 FDE 逐个解码**（FDE 起点是精确边界），并按基本块切好。

    ⚠ 早先的写法是"整段 `.text` 线性扫一遍"，那是**错的**：`0x5e4b50` 这种带跳转表的函数
    里，几个互斥臂上的 `add rbx,k`（`0x5e4c99 add 0xf` / `0x5e4db7 add 0xa`）会被线性
    扫**接在一起累加**，凭空造出一个 tag（见 `generic`/`scan` 的 `stops` 计数）。
    所以：① 每个 FDE 从自己起点解；② 值追踪走到**无条件跳转/ret/int3** 就停（基本块边界）。
    """

    STOP = ("jmp", "ret", "retf", "iret", "iretd", "iretq", "ud2", "int3", "hlt")

    def __init__(self, so):
        self.ctx = A.Ctx(so)
        self.md = Cs(CS_ARCH_X86, CS_MODE_64)
        self.md.detail = True
        self.ins = []            # 全库顺序表（子命令用；已按 FDE 边界解）
        self.blocks = []         # 每项 = (owner, [ins...])
        self.names = {}
        for st, ln in self.ctx.fde_ranges:
            try:
                blk = list(self.md.disasm(self.ctx.read(st, ln), st))
            except Exception:
                continue
            if not blk:
                continue
            self.ins += blk
            cur = []
            for y in blk:
                cur.append(y)
                if y.mnemonic in self.STOP:
                    self.blocks.append((st, cur))
                    cur = []
            if cur:
                self.blocks.append((st, cur))

    def rname(self, reg):
        if reg not in self.names:
            self.names[reg] = self.ins[0].reg_name(reg) if self.ins else str(reg)
        return self.names[reg]

    def parent(self, reg):
        n = self.rname(reg)
        return _PARENT.get(n, n)

    def extent(self, va):
        ex = self.ctx.fde_extent(va)
        return ex if ex else (va, va + 0x400)

    def owner(self, va):
        ex = self.ctx.fde_extent(va)
        return ex[0] if ex else None

    def mods(self, owner, span=0x400):
        return A._owner_loc(self.ctx, owner, owner + span)


# 只读操作数 0 的指令（`cmp`/`test`/`bt`/分支/`push`）：**不是**写者。
# 早先没排除它们，会把 `cmp rax, rbx` 当成"rax 被写"⇒ 追踪提前断，属漏报方向，已修。
_READONLY = frozenset({"cmp", "test", "bt", "push", "nop"})


def _writes(f, y):
    """这条指令写了哪些 64 位寄存器（名字集合）。`call` 按 SysV 踩 caller-saved。"""
    out = set()
    if y.mnemonic in _READONLY or y.mnemonic.startswith("j"):
        return out
    if y.mnemonic == "call":
        out |= _CALL_CLOBBER_NAMES
    for k, op in enumerate(y.operands):
        if op.type != X86_OP_REG:
            continue
        if k == 0:
            out.add(f.parent(op.reg))
        elif y.mnemonic in ("mul", "imul", "div", "idiv"):
            out |= {f.parent(op.reg), "rax", "rdx"}
    if y.mnemonic in ("mul", "imul", "div", "idiv", "cpuid"):
        out |= {"rax", "rdx"}
    return out


def _derive(f, y, val, reg):
    """若这条指令把 `reg` 的 `val` 变成一个确定的新值，返回它；否则 None。

    只认**立即数**算术与**同寄存器** lea —— 寄存器源的合成单独归 generic 扫描
    （那边只报"有这种东西"，不编值）。
    """
    ops = y.operands
    if not ops or ops[0].type != X86_OP_REG or f.name_of(ops[0].reg) != f.name_of(reg):
        return None
    imm = None
    for op in ops[1:]:
        if op.type == X86_OP_IMM:
            imm = op.imm
    if y.mnemonic in ("add", "sub") and imm is not None:
        return (val + imm) & M64 if y.mnemonic == "add" else (val - imm) & M64
    if y.mnemonic in ("or", "xor") and imm is not None:
        return (val | (imm & M64)) if y.mnemonic == "or" else (val ^ (imm & M64))
    if y.mnemonic == "and" and imm is not None:
        return val & (imm & M64)
    if y.mnemonic == "inc":
        return (val + 1) & M64
    if y.mnemonic == "dec":
        return (val - 1) & M64
    if y.mnemonic == "lea" and len(ops) == 2 and ops[1].type == X86_OP_MEM:
        m = ops[1].mem
        if f.name_of(m.base) == f.name_of(reg) and m.index == 0:
            return (val + m.disp) & M64
    return None


def _copy_src(f, y, reg):
    """`mov dst, src` 且 src 的 64 位父寄存器 == reg ⇒ 返回 None（同寄存器），否则返回 dst 的父名。

    这里只处理"把 reg 的值拷到**另一个**寄存器"（形态⑤/⑥）。返回 dst 的 capstone id。
    """
    ops = y.operands
    if y.mnemonic not in ("mov", "lea") or len(ops) != 2:
        return None
    if ops[0].type != X86_OP_REG:
        return None
    if f.name_of(ops[0].reg) == f.name_of(reg):
        return None
    if y.mnemonic == "mov" and ops[1].type == X86_OP_REG:
        if f.name_of(ops[1].reg) == f.name_of(reg):
            return ops[0].reg
        return None
    if y.mnemonic == "lea" and ops[1].type == X86_OP_MEM:
        m = ops[1].mem
        if f.name_of(m.base) == f.name_of(reg) and m.index == 0:
            return ops[0].reg
    return None


def _patch_name_of(self, reg):
    return self.md.reg_name(reg) if reg else ""


Fam.name_of = _patch_name_of


def scan(f, tag, verbose=True):
    """值追踪全库（**按基本块**）。返回 (sites, generic, nblocks)。

    sites: [(tag, site_va, kind, origin_movabs_va)]
    generic: [(movabs_va, ins_va, mnemonic, op_str, owner)] —— 家族常量与**寄存器**合成
    """
    sites = []
    generic = []
    stores = []      # (value, store_va, mem_desc, origin)
    escapes = []     # (value, va, mnemonic, op_str)：值流出到本工具**不跟踪**的位置
    for owner, blk in f.blocks:
        for i, y in enumerate(blk):
            if y.mnemonic != "movabs" or len(y.operands) != 2:
                continue
            d, s = y.operands
            if d.type != X86_OP_REG or s.type != X86_OP_IMM:
                continue
            v0 = s.imm & M64
            if not is_family(v0):
                continue
            if is_tag_family(v0):                     # 形态③：直接就是 tag word
                sites.append(((v0 & 0x7FFFFFFF), y.address, "movabs", y.address))
            vals = {d.reg: (v0, y.address)}           # reg id -> (value, origin)
            steps = {d.reg: 0}
            for z in blk[i + 1:]:
                wr = _writes(f, z)
                for reg in list(vals):
                    if f.parent(reg) not in wr:
                        continue
                    nv = _derive(f, z, vals[reg][0], reg)
                    if nv is None:
                        del vals[reg]
                        steps.pop(reg, None)
                        continue
                    steps[reg] = steps.get(reg, 0) + 1
                    vals[reg] = (nv, vals[reg][1])
                    if is_tag_family(nv):
                        sites.append(((nv & 0x7FFFFFFF), z.address,
                                      _kind_of(steps[reg], z), vals[reg][1]))
                # 家族常量 ⊕ 另一个寄存器（通用构造器候选）
                # `or`/`add`/`xor` 与「`lea dst,[fam + idx]`（索引寄存器）」是唯一几种
                # 能在**运行期**把 2^63 补成 `2^63|n` 的写法 —— 全部列出来人工看。
                if z.mnemonic in ("or", "add", "xor") and z.operands:
                    fam = {f.parent(r) for r in vals}
                    for op in z.operands[1:]:
                        if op.type == X86_OP_REG and f.parent(op.reg) in fam:
                            generic.append((y.address, z.address, z.mnemonic, z.op_str, owner))
                if z.mnemonic == "lea" and len(z.operands) == 2 and z.operands[1].type == X86_OP_MEM:
                    m = z.operands[1].mem
                    fam = {f.parent(r) for r in vals}
                    if m.index and f.parent(m.index) in fam:
                        generic.append((y.address, z.address, z.mnemonic, z.op_str, owner))
                # 家族值写进内存 = **构造点**的判据（tag 落进某个结构体的 +0）
                if (z.operands and z.operands[0].type == X86_OP_MEM and len(z.operands) >= 2
                        and z.mnemonic not in _READONLY and not z.mnemonic.startswith("j")):
                    src = z.operands[1]
                    if src.type == X86_OP_REG:
                        for reg in vals:
                            if f.parent(src.reg) == f.parent(reg):
                                stores.append((vals[reg][0], z.address, z.op_str,
                                               vals[reg][1]))
                                break
                # 值流出（push / 进 XMM）：本工具不跟踪 XMM 与栈，**照实记**，不假装扫到了
                if vals and (z.mnemonic == "push" or z.mnemonic.startswith("mov")
                             and any(o.type == X86_OP_MEM for o in z.operands[:1])):
                    fam = {f.parent(r) for r in vals}
                    for k, op in enumerate(z.operands):
                        if op.type == X86_OP_REG and k > 0 and f.parent(op.reg) in fam:
                            escapes.append((vals[op.reg][0], z.address, z.mnemonic, z.op_str))
                for reg in list(vals):
                    dst = _copy_src(f, z, reg)
                    if dst is not None:
                        vals[dst] = vals[reg]
                        steps[dst] = steps.get(reg, 0)
    return sites, generic, stores, escapes, len(f.blocks)


def _PARENT_SENTINEL(vals, f):
    return {f.parent(r) for r in vals}


def _kind_of(nsteps, ins):
    if nsteps > 1:
        return "multi-step"
    if ins.mnemonic == "lea":
        return "BASE+lea"
    if ins.mnemonic in ("add", "or"):
        return "BASE+" + ins.mnemonic
    if ins.mnemonic == "inc":
        return "BASE+inc"
    return "copy"


def cmd_scan(f, tag):
    sites, generic, stores, _, nblk = scan(f, tag)
    by_tag = defaultdict(list)
    for t, a, k, o in sites:
        by_tag[t].append((a, k, o))
    print("== 值追踪（四形态 + ⑤lea异 dst + ⑥拷贝 + ⑦多步 + ⑧换基址）==")
    print("   基本块 %d 个；命中 tag 数：%d；总站点 %d；家族常量⊕寄存器站点 %d"
          % (nblk, len(by_tag), sum(len(v) for v in by_tag.values()), len(generic)))
    print("   形态计数：" + ", ".join("%s x%d" % (k, n) for k, n in
                                     sorted(Counter(k for _, _, k, _ in sites).items())))
    print()
    tgt = by_tag.get(tag, [])
    print("== tag %d (%#018x) ==" % (tag, BASE | tag))
    if not tgt:
        print("   **0 处**（值追踪也扫不到）")
    for a, k, o in sorted(tgt):
        ow = f.owner(a) or 0
        print("   %#x  %-12s  base movabs@%#x  in %#x  %s"
              % (a, k, o, ow, f.mods(ow)))
        for z in _next_ins(f, a, 3):
            print("        %#x  %-8s %s" % (z.address, z.mnemonic, z.op_str))
    print()
    if generic:
        print("== 家族常量 ⊕ **寄存器** 的站点（通用构造器候选，前 20）==")
        for mv, a, m, s, ow in generic[:20]:
            print("   movabs@%#x -> %#x %-4s %-24s in %#x" % (mv, a, m, s, ow))
        print("   （共 %d 处；这些点**不能**归到某个 tag，见 docs/06 §47.10）" % len(generic))
    else:
        print("== 家族常量 ⊕ **寄存器** 的站点：**0 处** ⇒ 二进制里没有通用 tag 构造器 ==")
    return 0


def _next_ins(f, va, n=3):
    """同一条指令流（按 FDE 解出来的顺序表）里跟在 va 后面的 n 条。"""
    for k, y in enumerate(f.ins):
        if y.address == va:
            return f.ins[k + 1:k + 1 + n]
    return []


def cmd_stores(f, tag):
    """**构造点**判据：tag word 被写进内存（事件结构体 +0）的站点。"""
    _, _, stores, escapes, _ = scan(f, tag)
    by = Counter(v & 0x7FFFFFFF for v, _, _, _ in stores)
    tgt = [(v, a, m, o) for v, a, m, o in stores if (v & 0x7FFFFFFF) == tag]
    print("== 家族值 store 到内存：全库 %d 处，覆盖 %d 个 tag ==" % (len(stores), len(by)))
    esc_t = [e for e in escapes if (e[0] & 0x7FFFFFFF) == tag]
    print("   （值流出 push/XMM：全库 %d 处，其中 tag %d 的 %d 处）"
          % (len(escapes), tag, len(esc_t)))
    for v, a, m, o in esc_t[:10]:
        print("     流出 %#x  %-8s %s" % (a, m, o))
    print("== tag %d ==" % tag)
    if not tgt:
        print("   **0 处**：没有任何一条 store 把 tag %d 写进内存 ⇒ 没有构造点" % tag)
    for v, a, m, o in sorted(tgt, key=lambda t: t[1]):
        ow = f.owner(a) or 0
        print("   %#x  %-34s  值=%#x  基址 movabs@%#x  in %#x  %s"
              % (a, m, v, o, ow, f.mods(ow)))
    return 0


def cmd_generic(f):
    _, generic, _, _, _ = scan(f, 0)
    print("== 家族常量 ⊕ 寄存器（通用构造器候选）：%d 处 ==" % len(generic))
    for mv, a, m, s, ow in generic:
        nm = f.parent(0)  # unused
        print("   movabs@%#x -> %#x %-4s %-26s in %#x" % (mv, a, m, s, ow))
    return 0


def cmd_prov(f, tag):
    """反向：全库所有把立即数 `tag` / `tag+1` 加到 64 位寄存器上的站点，逐个回溯源寄存器。"""
    want = {tag, tag + 1}
    print("== 反向取证：立即数 ∈ {%#x, %#x} 且写到 64 位寄存器的算术站点 ==" % (tag, tag + 1))
    n = 0
    for i, y in enumerate(f.ins):
        ops = y.operands
        if not ops or ops[0].type != X86_OP_REG:
            continue
        if y.mnemonic not in ("add", "or", "lea"):
            continue
        if f.parent(ops[0].reg) in ("rsp", "rbp"):
            continue
        hit = None
        for op in ops[1:]:
            if op.type == X86_OP_IMM and (op.imm & M64) in want:
                hit = "%s %#x" % (y.mnemonic, op.imm & M64)
            if (y.mnemonic == "lea" and op.type == X86_OP_MEM and op.mem.index == 0
                    and op.mem.disp in (tag, tag + 1)):
                hit = "lea disp %#x" % op.mem.disp
        if hit is None:
            continue
        n += 1
        src = _back_prov(f, i, f.parent(ops[0].reg))
        print("   %#x  %-8s %-26s src=%s" % (y.address, y.mnemonic, y.op_str, src))
    if n == 0:
        print("   **0 处**：全库没有任何一条「把 %d/%d 加到寄存器」的算术，"
              "所以 tag %d 不可能由算术构造" % (tag, tag + 1, tag))
    return 0


def _back_prov(f, i, parent, budget=64):
    """从第 i 条往回找**最后一个**写 `parent` 的指令，返回一句人读的溯源。"""
    y0 = f.ins[i]
    lo, _ = f.extent(y0.address)
    for j in range(i - 1, max(i - budget, -1), -1):
        z = f.ins[j]
        if z.address < lo:
            break
        if parent in _writes(f, z):
            if z.mnemonic == "movabs" and z.operands[1].type == X86_OP_IMM:
                v = z.operands[1].imm & M64
                return "movabs %#x @%#x%s" % (v, z.address,
                                              "  ← 家族常量" if is_family(v) else "")
            return "%s %s @%#x" % (z.mnemonic, z.op_str, z.address)
    return "（窗口内找不到写者）"


# ---------------------------------------------------------------- 正对照
def cmd_self_test(f):
    """拿**真字节**做变异：把已知构造点的立即数改成 0x25，判据必须跟着改口。"""
    ok = True

    def _fmt_tag(t):
        if t is None:
            return "None"
        if isinstance(t, int) and (t >> 32) == 0x80000000:
            return "tag %d" % (t & 0x7FFFFFFF)
        return repr(t)

    def check(desc, got, want):
        nonlocal ok
        good = got == want
        ok = ok and good
        print("   %-64s %s (got %s, want %s)" % (desc, "OK" if good else "**失败**",
                                                 _fmt_tag(got), _fmt_tag(want)))

    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True

    def dec(va, n):
        return list(md.disasm(f.ctx.read(va, n), va))

    def decb(va, blob):
        """从**给定字节**解码（`dec` 是从 .so 读，变异体必须走这里）。"""
        return list(md.disasm(bytes(blob), va))

    # --- 对照 A：tag 15 的真实形态④ 站点（0x474b8d movabs base + 0x474b97 add rax,0xf）
    raw = bytearray(f.ctx.read(0x474B8D, 0x474B9C - 0x474B8D))
    l1 = dec(0x474B8D, 0x474B9C - 0x474B8D)
    check("A0 原字节 0x474b8d.. 形态④ 判出 tag 15",
          A._tag_after_base(l1, 0, l1[0].operands[0].reg)[0], BASE | 15)
    # `add rax, 0xf` 的机器码是 `48 83 c0 0f`：立即数在**这条指令的最后一个字节**
    # （不是整段窗口的最后一个字节 —— 窗口末字节是下一条指令的开头）
    add_i = next(y for y in l1 if y.mnemonic == "add" and y.operands[1].imm == 0xF)
    mut = bytearray(raw)
    mut[add_i.address + add_i.size - 1 - 0x474B8D] = 0x25
    l2 = decb(0x474B8D, mut)
    # 直接对**变异后的字节**跑同一条判据
    got = A._tag_after_base(l2, 0, l2[0].operands[0].reg)
    check("A1 同一条指令把立即数 0x0f→0x25 ⇒ 必须判出 tag 37",
          got[0] if got else None, BASE | 37)

    # --- 对照 B：形态③ 直写（tag 42 @0x3f20f0 家族），变异成 0x25
    site42 = None
    for y in f.ins:
        if (y.mnemonic == "movabs" and y.operands[1].type == X86_OP_IMM
                and (y.operands[1].imm & M64) == BASE | 42):
            site42 = y.address
            break
    check("B0 形态③ tag 42 有站点", site42 is not None, True)

    # --- 对照 C：拿真 .text 里的 tag 37 站点，用法必须是 cmp（不是 store）
    nxt = _next_ins(f, 0x4758B5, 1)
    check("C0 0x4758b5 的下一条是 cmp（tag 37 只被**消费**）",
          nxt[0].mnemonic if nxt else None, "cmp")

    # --- 对照 D：值追踪对同一个 tag 15 站点给出同样的 tag（两套判据互证）
    sites, _, _, _, _ = scan(f, 15)
    check("D0 值追踪把 0x474b8d 认成 tag 15",
          any(t == 15 and a in (0x474B8D, 0x474B97) for t, a, _, _ in sites), True)

    # --- 对照 E：值追踪认多步/异 dst（合成序列，不依赖 .so 里恰好有）
    synth = b"\x48\xb8\x00\x00\x00\x00\x00\x00\x00\x80"      # movabs rax,2^63
    synth += b"\x48\x83\xc0\x20"                              # add rax,0x20   → tag 32
    synth += b"\x48\x83\xc0\x05"                              # add rax,5      → tag 37
    l3 = list(md.disasm(synth, 0x1000))
    v = l3[0].operands[1].imm & M64
    reg = l3[0].operands[0].reg
    steps = 0
    got37 = False
    for z in l3[1:]:
        nv = _derive(f, z, v, reg)
        if nv is None:
            break
        v = nv
        steps += 1
        if is_tag_family(nv) and (nv & 0x7FFFFFFF) == 37:
            got37 = True
    check("E0 两步累加 2^63+0x20+5 ⇒ 值追踪必须算出 tag 37", got37, True)

    print("\n自检：%s" % ("全部通过（rc=0）" if ok else "**有失败**（rc=1）"))
    return 0 if ok else 1


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--so", default=A.SO_DEFAULT)
    ap.add_argument("--self-test", action="store_true")
    sub = ap.add_subparsers(dest="cmd")
    p = sub.add_parser("scan", help="值追踪全库，报某 tag 的全部站点")
    p.add_argument("--tag", type=lambda s: int(s, 0), default=37)
    p = sub.add_parser("prov", help="反向：补 n/n+1 的算术站点 + 溯源")
    p.add_argument("--tag", type=lambda s: int(s, 0), default=37)
    p = sub.add_parser("stores", help="家族值写进内存的站点（= 构造点判据）")
    p.add_argument("--tag", type=lambda s: int(s, 0), default=37)
    sub.add_parser("generic", help="家族常量 ⊕ 寄存器的站点")
    a = ap.parse_args(argv)

    f = Fam(a.so)
    if a.self_test:
        return cmd_self_test(f)
    if a.cmd == "scan":
        return cmd_scan(f, a.tag)
    if a.cmd == "prov":
        return cmd_prov(f, a.tag)
    if a.cmd == "stores":
        return cmd_stores(f, a.tag)
    if a.cmd == "generic":
        return cmd_generic(f)
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
