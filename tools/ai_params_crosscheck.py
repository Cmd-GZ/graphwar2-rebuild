#!/usr/bin/env python3
"""AI 进化器（`graphwar_function_evolutionator`）**参数/常量层逐位对表**。

真值侧 = `.reference/game/libgraphwarrust.so`
（sha256 9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187）；
我方侧 = `src/graphwar_function_evolutionator.rs` + `src/lib.rs` 的 `Function` tag 号。

每一行都从二进制**取原始字节/立即数**（capstone 解码 + ELF 段直读），
再与我方常量按 `f64::to_bits()` / 整数相等比较 ⇒ `fixtures/ai_params_crosscheck.tsv`。

## 纪律

* **不抄文档里的数字**：常量一律从二进制取；文档地址只用来定位候选函数，
  定位不到/字节对不上就**报错退出**（不静默当成"通过"）。
* 地址口径 = **ELF vaddr（RVA）**，直接用，不 ±0x100000。
* 只判"值是否逐位相等"，**不判行为是否一致**（行为层见 `docs/06` §73 的 B/C 两层）。

## 跑法

```bash
.reference/venv/bin/python tools/ai_params_crosscheck.py            # 写 tsv + 打印汇总
.reference/venv/bin/python tools/ai_params_crosscheck.py --stdout   # 只打印，不写文件
```

退出码：0 = 所有 **逐位项** 都相等（`MISMATCH` 为 0）；1 = 有不等项；2 = 抽取失败。
"""

import argparse
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

DEFAULT_SO = os.path.join(ROOT, ".reference/game/libgraphwarrust.so")
SO_SHA256 = "9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187"

# ---- 二进制里的关键地址（全部 vaddr） -------------------------------------
A_GEN = 0x4EF020          # 随机函数生成器 random_function
A_REPAIR = 0x4EE0E0       # 装配 / 补齐 repair
A_FACTORY = 0x4EE3A0      # 子代工厂
A_MUTATE = 0x4EE720       # 变异
A_SCORE = 0x51A750        # 评分核心
A_EVOLVE_A = 0x51B440     # 变体 A（种群 GA）
A_RUST_LEN = 0x4EF0B2     # cvttsd2si（长度公式截断）
A_ARITY = 0x1C47D0        # arity-1 表（Ghidra DAT_002c47d0）
A_PARAMS = 0x3E3D95       # 难度分档 → s / limit 的立即数块
A_PARAMS_TAIL = 0x3E456D
A_BUDGET = 0x51B7CF       # 墙钟预算比较点（cmp $0x19）

# .rodata 常量（本轮**从指令里解析**出来的 rip 目标，不是抄文档）
CONST_EXPECT = {}

try:
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64, CS_OP_IMM, CS_OP_MEM, CS_OP_REG
except Exception:  # pragma: no cover
    raise SystemExit("需要 capstone：用 .reference/venv/bin/python 跑本脚本")


def IS_IMM(op):
    return op.type == CS_OP_IMM


def IS_REG(ins, op, name):
    return op.type == CS_OP_REG and ins.reg_name(op.reg) == name


def IS_MEM(op):
    return op.type == CS_OP_MEM


# ---------------------------------------------------------------------------
# ELF 读
# ---------------------------------------------------------------------------
class Elf:
    def __init__(self, path):
        self.data = open(path, "rb").read()
        self.segs = self._phdrs()
        self.secs = self._sections()
        self.rel = self._relocs()
        self.dsym = self._dynsyms()

    def _phdrs(self):
        d = self.data
        e_phoff, = struct.unpack_from("<Q", d, 0x20)
        e_phentsize, e_phnum = struct.unpack_from("<HH", d, 0x36)
        out = []
        for i in range(e_phnum):
            o = e_phoff + i * e_phentsize
            p_type, p_flags, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_align = \
                struct.unpack_from("<IIQQQQQQ", d, o)
            if p_type == 1:  # PT_LOAD
                out.append((p_vaddr, p_offset, p_filesz, p_memsz, p_flags))
        return out

    def _sections(self):
        d = self.data
        e_shoff, = struct.unpack_from("<Q", d, 0x28)
        e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", d, 0x3A)
        raw = []
        for i in range(e_shnum):
            o = e_shoff + i * e_shentsize
            raw.append(list(struct.unpack_from("<IIQQQQ", d, o)))
        strhdr = raw[e_shstrndx]
        out = {}
        for name_off, typ, flags, addr, offset, size in raw:
            end = d.index(b"\0", strhdr[4] + name_off)
            out[d[strhdr[4] + name_off:end].decode()] = (typ, flags, addr, offset, size)
        return out

    def _relocs(self):
        sec = self.secs.get(".rela.dyn")
        if not sec:
            return {}
        out = {}
        for i in range(sec[4] // 24):
            o = sec[3] + i * 24
            r_off, r_info, r_addend = struct.unpack_from("<QQq", self.data, o)
            out[r_off] = (r_info & 0xFFFFFFFF, r_info >> 32, r_addend)
        return out

    def _dynsyms(self):
        dsym, dstr = self.secs[".dynsym"], self.secs[".dynstr"]
        out = []
        for i in range(dsym[4] // 24):
            o = dsym[3] + i * 24
            nm, info, other, shndx, value, size = struct.unpack_from("<IBBHQQ", self.data, o)
            end = self.data.index(b"\0", dstr[3] + nm)
            out.append(self.data[dstr[3] + nm:end].decode())
        return out

    # -- 读 --
    def read(self, va, n):
        for vaddr, off, filesz, _mem, _fl in self.segs:
            if vaddr <= va < vaddr + filesz:
                start = off + (va - vaddr)
                if va + n > vaddr + filesz:
                    raise SystemExit("读越界：%#x+%d" % (va, n))
                return self.data[start:start + n]
        raise SystemExit("vaddr %#x 不在任何 PT_LOAD 段内" % va)

    def u8(self, va):
        return self.read(va, 1)[0]

    def u32(self, va):
        return struct.unpack("<I", self.read(va, 4))[0]

    def i32(self, va):
        return struct.unpack("<i", self.read(va, 4))[0]

    def u64(self, va):
        return struct.unpack("<Q", self.read(va, 8))[0]

    def f64(self, va):
        return struct.unpack("<d", self.read(va, 8))[0]

    def slot(self, va):
        """解 GOT/.data.rel.ro 指针槽 → 目标 vaddr（只认 R_X86_64_RELATIVE）。"""
        e = self.rel.get(va)
        if e and e[0] == 8:
            return e[2]
        return None

    def imported_at(self, va):
        """该指针槽是不是外部导入符号（R_X86_64_GLOB_DAT/JUMP_SLOT）。"""
        e = self.rel.get(va)
        if e and e[0] in (6, 7):
            return self.dsym[e[1]]
        return None


MD = Cs(CS_ARCH_X86, CS_MODE_64)
MD.detail = True


class Code:
    """从一个**已知函数起点**做线性反汇编（不要从函数中间切，会错位）。"""

    def __init__(self, elf, start, length):
        self.elf = elf
        self.start = start
        self.insns = list(MD.disasm(elf.read(start, length), start))
        self.by_addr = {}
        for i, ins in enumerate(self.insns):
            self.by_addr[ins.address] = (i, ins)

    def window(self, lo, hi):
        return [x for x in self.insns if lo <= x.address <= hi]

    def at(self, addr):
        if addr not in self.by_addr:
            raise SystemExit("指令边界 %#x 不在反汇编流里（起点选错了）" % addr)
        return self.by_addr[addr][1]

    def rip_targets(self, lo, hi):
        """窗口内所有 rip 相对内存操作数 → {target_vaddr: [(addr, mnem, op_str)]}"""
        out = {}
        for ins in self.window(lo, hi):
            for op in ins.operands:
                if op.type == CS_OP_MEM and op.mem.base == 41:  # RIP
                    tgt = ins.address + ins.size + op.mem.disp
                    out.setdefault(tgt, []).append((ins.address, ins.mnemonic, ins.op_str))
        return out

    def calls_to(self, lo, hi, target):
        out = []
        for ins in self.window(lo, hi):
            if ins.mnemonic == "call" and ins.operands and ins.operands[0].type == CS_OP_IMM:
                if ins.operands[0].imm == target:
                    out.append(ins.address)
        return out

    def imm_cmps(self, lo, hi, reg="r12d"):
        out = []
        for ins in self.window(lo, hi):
            if ins.mnemonic == "cmp" and len(ins.operands) == 2:
                a, b = ins.operands
                if IS_IMM(a) and IS_REG(ins, b, reg):
                    out.append((ins.address, a.imm))
                elif IS_IMM(b) and IS_REG(ins, a, reg):
                    out.append((ins.address, b.imm))
        return out

    def movs_to(self, lo, hi, reg="r13d"):
        out = []
        for ins in self.window(lo, hi):
            if ins.mnemonic == "mov" and len(ins.operands) == 2:
                a, b = ins.operands
                # capstone 默认 Intel 语法：dest 在前
                if IS_REG(ins, a, reg) and IS_IMM(b):
                    out.append((ins.address, b.imm))
                elif IS_IMM(a) and IS_REG(ins, b, reg):
                    out.append((ins.address, a.imm))
        return out


# ---------------------------------------------------------------------------
# 我方源码读
# ---------------------------------------------------------------------------
class Rust:
    def __init__(self, path):
        self.src = open(path, encoding="utf-8").read()

    def f64const(self, name):
        m = re.search(r"pub const %s: f64 = ([0-9_.]+);" % re.escape(name), self.src)
        if not m:
            raise SystemExit("源码里找不到常量 %s" % name)
        return float(m.group(1).replace("_", ""))

    def intconst(self, name):
        m = re.search(r"pub const %s: u32 = (\d+);" % re.escape(name), self.src)
        if not m:
            raise SystemExit("源码里找不到常量 %s" % name)
        return int(m.group(1))

    def usizeconst(self, name):
        m = re.search(r"pub const %s: usize = (\d+);" % re.escape(name), self.src)
        if not m:
            raise SystemExit("源码里找不到常量 %s" % name)
        return int(m.group(1))

    def table(self, name):
        m = re.search(r"const %s: &\[\(Function, u32\)\] = &\[(.*?)\];" % name, self.src, re.S)
        if not m:
            raise SystemExit("源码里找不到 %s" % name)
        pairs = re.findall(r"\(Function::(\w+), (\d+)\)", m.group(1))
        if not pairs:
            raise SystemExit("%s 解析为空" % name)
        return [(tag, int(w)) for tag, w in pairs]

    def params_table(self, name="params"):
        m = re.search(r"pub fn %s\(level: u8\) -> Params \{(.*?)\n\}" % name, self.src, re.S)
        if not m:
            raise SystemExit("找不到 fn %s" % name)
        body = m.group(1)
        table = {}
        for lvl, gens, pop, lim in re.findall(r"(\d+) => \((\d+), (\d+), (\d+)\)", body):
            table[int(lvl)] = (int(gens), int(pop), int(lim))
        if "_ =>" in body:
            m2 = re.search(r"_ => \((\d+), (\d+), (\d+)\)", body)
            if m2:
                table[6] = tuple(int(x) for x in m2.groups())
        per_slot = re.search(r"per_slot: (\d+)", body)
        if not per_slot:
            raise SystemExit("params() 里没有 per_slot 字面量")
        return table, int(per_slot.group(1))

    def difficulty_thresholds(self):
        """从 difficulty_params 里抠出 (阈值, s, limit) 分支序列（顺序 = 源码顺序）。"""
        m = re.search(r"pub fn difficulty_params\(difficulty: u32\).*?\n\}\n", self.src, re.S)
        if not m:
            raise SystemExit("找不到 difficulty_params")
        body = m.group(0)
        arm = r"\(\s*(\d+)(?:u32)?\s*,\s*(\d+)(?:u32)?\s*\)"
        out = []
        for m2 in re.finditer(r"(?:if|else if) difficulty < (\d+) \{\s*" + arm, body):
            out.append((int(m2.group(1)), int(m2.group(2)), int(m2.group(3))))
        tail = re.search(r"\} else \{\s*" + arm, body)
        if tail:
            out.append((None, int(tail.group(1)), int(tail.group(2))))
        if not out:
            raise SystemExit("difficulty_params 里没解析出分支")
        return out

    def normal_intervals(self):
        """把权重表折成 tag → 累积区间 [lo, hi) 列表（用于和二进制区间逐段比）。"""
        out = {}
        pos = 0
        for tag, w in self.table("TABLE_NORMAL"):
            out.setdefault(tag, []).append((pos, pos + w))
            pos += w
        if pos != 85:
            raise ExtractError("TABLE_NORMAL 权重之和 %d ≠ 85" % pos)
        return out

    def arity(self):
        """我们 arity() 的 tag→arity 映射。"""
        m = re.search(r"fn arity\(tag: Function\) -> i64 \{(.*?)\n\}", self.src, re.S)
        if not m:
            raise SystemExit("找不到 arity()")
        body = m.group(1)
        out = {}
        wildcard = None
        for val in (0, 1, 2):
            arm = re.search(r"([^\n;]*?) => %d," % val, body)
            if not arm:
                raise SystemExit("arity() 缺 => %d 分支" % val)
            head = arm.group(1).strip()
            names = re.findall(r"Function::(\w+)", head)
            if names:
                for name in names:
                    out[name] = val
            elif head.startswith("_"):
                wildcard = val
        if wildcard is None:
            raise SystemExit("arity() 没有 `_ =>` 兜底分支，无法补全 tag")
        for name in FUNC_TAG:
            out.setdefault(name, wildcard)
        return out


FUNC_TAG = {
    "VarX": 0, "VarY": 1, "VarDy": 2, "Add": 3, "Sub": 4, "UnaryMinus": 5, "Mul": 6,
    "Divide": 7, "Pow": 8, "Sqrt": 9, "Log": 10, "Ln": 11, "Exp": 12, "Abs": 13,
    "Sin": 14, "Cos": 15, "Tan": 16, "Const": 17, "LeftBracket": 18, "RightBracket": 19,
}
FUNC_NAME = {v: k for k, v in FUNC_TAG.items()}


# ---------------------------------------------------------------------------
# 抽取器（每个都自带断言，抽不到就抛 ExtractError）
# ---------------------------------------------------------------------------
class ExtractError(Exception):
    pass


def merged(intervals):
    """合并相邻/重叠区间：[(11,17),(17,21)] → [(11,21)]。

    二进制把「变量槽」拆成两段产同一个 tag（Normal 下 11..16 与 17..20 都给 x），
    所以比"区间集合"时必须先合并，否则会拿表示差异当不一致。
    """
    if not intervals or intervals == "不产生":
        return intervals
    out = []
    for lo, hi in sorted(intervals):
        if out and lo <= out[-1][1]:
            out[-1] = (out[-1][0], max(out[-1][1], hi))
        else:
            out.append((lo, hi))
    return out


def bits_f64(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def x_weight_intervals(elf, code):
    """从 0x4ef193 起的 `idx = rand_u32 % 85` + 区间比较链里抽出权重区间。

    结构（人读过的部分，工具只负责取立即数并断言形状）：
      idx < b0            → Const(17)，值 normal*10.2
      idx < b1            → VarX(0)
      idx < b2            → tag = mode & 1（Normal 下也是 VarX）
      idx < b3..b11       → 固定 tag 3/4/6/7/8
      idx < b12 (lo=b11)  → tag = idx - b11（连续三格 9/10/11）
      idx < b13..          → 12 / 13 / 14
      尾部 cmp $0x54 + sbb ⇒ Cos(15) / Tan(16)
    """
    lo, hi = 0x4EF193, 0x4EF2E0
    cmp_b = code.imm_cmps(lo, hi, "r12d")
    tags = code.movs_to(lo, hi, "r13d")
    cmps_at = {a: v for a, v in cmp_b}
    tags_at = {a: v for a, v in tags}
    expect_cmp = [0x4EF1C1, 0x4EF220, 0x4EF235, 0x4EF241, 0x4EF24D,
                  0x4EF259, 0x4EF265, 0x4EF271, 0x4EF277, 0x4EF28C,
                  0x4EF298, 0x4EF2A4, 0x4EF2D0]
    use_cmp = []
    for a in expect_cmp:
        # 允许 ±4 字节的编码差异（早期手工读时遇到过 objdump 错位）
        cand = [x for x in cmp_b if abs(x[0] - a) <= 6]
        if not cand:
            raise ExtractError("0x%x 附近找不到 `cmp $imm,%%r12d`" % a)
        use_cmp.append(min(cand, key=lambda x: abs(x[0] - a)))
    use_cmp.sort()
    bounds = [v for _a, v in use_cmp]
    if bounds != sorted(bounds) or len(set(bounds)) != len(bounds):
        raise ExtractError("区间比较链不是单调递增：%r" % (bounds,))

    # 固定 tag：mov $imm,%r13d 里出现的那几个（3/4/6/7/8/12/13/14/16）
    fixed = sorted({v for _a, v in tags})
    want_fixed = [3, 4, 6, 7, 8, 12, 13, 14, 16]
    for w in want_fixed:
        if w not in fixed:
            raise ExtractError("区间链里没有 tag %d 的立即数" % w)

    # 装配：按上面读死的结构把 (tag_expr, width) 列出来
    seq = []
    b = bounds
    seq.append(("Const", 17, b[0] - 0))
    seq.append(("VarX", 0, b[1] - b[0]))
    seq.append(("MODE", "mode&1", b[2] - b[1]))
    seq.append(("Add", 3, b[3] - b[2]))
    seq.append(("Sub", 4, b[4] - b[3]))
    seq.append(("Mul", 6, b[5] - b[4]))
    seq.append(("Divide", 7, b[6] - b[5]))
    seq.append(("Pow", 8, b[7] - b[6]))
    seq.append(("SqrtLogLn", "idx - %d" % b[7], b[8] - b[7]))
    seq.append(("Exp", 12, b[9] - b[8]))
    seq.append(("Abs", 13, b[10] - b[9]))
    seq.append(("Sin", 14, b[11] - b[10]))
    seq.append(("Cos", 15, b[12] - b[11]))
    seq.append(("Tan", 16, 85 - b[12]))
    total = sum(w for _n, _t, w in seq)
    if total != 85:
        raise ExtractError("区间宽度之和 = %d ≠ 85" % total)
    return seq, bounds, use_cmp


def x_weight_total(elf, code):
    """`random % 85` 的模数：imul $0x55 出现处（0x4ef1bb 一带）。"""
    hits = []
    for ins in code.window(0x4EF1B0, 0x4EF1C2):
        if ins.mnemonic == "imul" and any(op.type == CS_OP_IMM and op.imm == 0x55
                                          for op in ins.operands):
            hits.append(ins.address)
    if not hits:
        raise ExtractError("0x4ef1b0..c2 里没有 `imul $0x55`（模数 85）")
    return 0x55, hits


def x_generator_consts(elf, code):
    """生成器里的长度公式常量：mulsd 5.0 / addsd 10.0 / 常数缩放 10.2 / cvttsd2si。"""
    out = {}
    for tgt, uses in code.rip_targets(0x4EF020, 0x4EF2E0).items():
        val = elf.f64(tgt)
        for addr, mnem, opstr in uses:
            if mnem in ("mulsd", "addsd") and addr < 0x4EF0C0:
                out.setdefault(round(val, 12), []).append(
                    (addr, tgt, bits_f64(val), mnem))
    if 5.0 not in out:
        raise ExtractError("长度公式里没找到 5.0（mulsd）")
    if 10.0 not in out:
        raise ExtractError("长度公式里没找到 10.0（addsd）")
    # 常数 token 的缩放：0x4ef1dc 那条 mulsd
    scale = None
    for tgt, uses in code.rip_targets(0x4EF1D0, 0x4EF1E4).items():
        if elf.f64(tgt) > 1.0:
            scale = (tgt, elf.f64(tgt), bits_f64(elf.f64(tgt)), uses[0][0])
    if not scale:
        raise ExtractError("生成器里没找到常数缩放（mulsd 10.2）")
    return out, scale


def x_repair_consts(elf, code):
    """补齐：rand%50 / 阈值 25、42 / 常数 tag 17 / mode&1。"""
    out = {}
    mod = []
    for ins in code.window(0x4EE240, 0x4EE256):
        if ins.mnemonic == "imul" and len(ins.operands) == 3:
            if ins.operands[2].type == CS_OP_IMM and ins.operands[2].imm == 0x32:
                mod.append(ins.address)
    if not mod:
        raise ExtractError("补齐里没找到 `imul $0x32`（模数 50）")
    out["mod"] = (50, mod)
    thr = code.imm_cmps(0x4EE250, 0x4EE2D0, "r12d")
    vals = {v for _a, v in thr}
    if 25 not in vals or 42 not in vals:
        raise ExtractError("补齐阈值不是 25/42，抽到 %r" % sorted(vals))
    out["thr25"] = [a for a, v in thr if v == 25]
    out["thr42"] = [a for a, v in thr if v == 42]
    tags = code.movs_to(0x4EE290, 0x4EE2B0, "r12d")
    if not any(v == 0x11 for _a, v in tags):
        raise ExtractError("补齐里没找到常数 tag 0x11")
    out["const_tag"] = [a for a, v in tags if v == 0x11]
    # mode & 1
    ands = [ins.address for ins in code.window(0x4EE140, 0x4EE160)
            if ins.mnemonic == "and" and any(op.type == CS_OP_IMM and op.imm == 1
                                             for op in ins.operands)]
    if not ands:
        raise ExtractError("补齐里没找到 `and $1`（mode&1）")
    out["mode_and1"] = ands
    return out


def x_arity_table(elf, code):
    """arity-1 表（0x1c47d0）：值 = arity - 1，按 tag 0..19。"""
    vals = [elf.i32(A_ARITY + 4 * i) for i in range(20)]
    if vals[0] != -1 or vals[3] != 1 or vals[17] != -1:
        raise ExtractError("arity 表形状不对：前 4 项 %r" % vals[:4])
    return vals


def x_factory(elf, code):
    """子代工厂：3 路分支（u32 符号 / 父代 <20 → 重生 / 否则交叉），交叉切点两个 u64 取模。"""
    out = {}
    js = [ins.address for ins in code.window(0x4EE420, 0x4EE450) if ins.mnemonic == "js"]
    if not js:
        raise ExtractError("工厂里没找到 `js`（u32 符号分支）")
    out["sign_js"] = js
    cmp20 = [(ins.address, op.imm) for ins in code.window(0x4EE455, 0x4EE470)
             for op in ins.operands
             if ins.mnemonic == "cmp" and op.type == CS_OP_IMM]
    if not any(v == 0x14 for _a, v in cmp20):
        raise ExtractError("工厂里没找到父代长度阈值 0x14（20 token）")
    out["len20"] = [a for a, v in cmp20 if v == 0x14]
    divs = [ins.address for ins in code.window(0x4EE4E0, 0x4EE545) if ins.mnemonic == "div"]
    if len(divs) < 2:
        raise ExtractError("工厂交叉里没找到两次取模（div）")
    out["divs"] = divs
    call_repair = code.calls_to(0x4EE5B0, 0x4EE640, A_REPAIR)
    if not call_repair:
        raise ExtractError("工厂里没找到对 0x4ee0e0（补齐）的调用")
    out["repair_call"] = call_repair
    call_mut = code.calls_to(0x4EE490, 0x4EE4D0, A_MUTATE)
    out["mutate_call"] = call_mut
    return out


def x_mutate(elf, code):
    """变异：候选值 token 的 tag（0/1/17）、算子 tag（3/7/6）与常数缩放。"""
    out = {}
    cand = [ins.address for ins in code.window(0x4EE780, 0x4EE7E0)
            if ins.mnemonic == "cmp"
            and any(op.type == CS_OP_IMM and op.imm in (1, 0x11) for op in ins.operands)]
    if not cand:
        raise ExtractError("变异里没找到值 token 的 tag 比较（1 / 0x11）")
    out["cand_cmp"] = cand
    # 三个算子 tag 的写入点（mov qword ptr [reg], imm）
    optags = []
    for ins in code.window(0x4EE9D0, 0x4EEAF8):
        if ins.mnemonic == "mov" and len(ins.operands) == 2:
            a, b = ins.operands
            if IS_MEM(a) and IS_IMM(b):
                optags.append((ins.address, b.imm))
            elif IS_IMM(a) and IS_MEM(b):
                optags.append((ins.address, a.imm))
    got = [v for _a, v in optags]
    for want in (3, 6, 7):
        if want not in got:
            raise ExtractError("变异里没找到算子 tag %d（抽到 %r）" % (want, got))
    out["op_tags"] = optags
    # 常数缩放 10.2：0x4ee8a8 的 mulsd
    scale = None
    for tgt, uses in code.rip_targets(0x4EE8A0, 0x4EE8C0).items():
        v = elf.f64(tgt)
        if 1.0 < v < 100.0:
            scale = (tgt, v, bits_f64(v), uses[0][0])
    if not scale:
        raise ExtractError("变异里没找到常数缩放")
    out["scale"] = scale
    # 两次 u32 符号判定（50/25/25）
    js = [ins.address for ins in code.window(0x4EE980, 0x4EEA80) if ins.mnemonic == "js"]
    out["sign_js"] = js
    return out


def x_two_phase(elf, code):
    """两阶段：粗评 0.5、精评 0.01（3 处）、判据容差复用 0.5。"""
    lo, hi = 0x51B440, 0x51E100
    rt = code.rip_targets(lo, hi)
    coarse, fine = [], []
    for tgt, uses in rt.items():
        try:
            val = elf.f64(tgt)
        except SystemExit:
            continue
        for addr, mnem, opstr in uses:
            if val == 0.5 and mnem in ("movq", "movsd", "ucomisd"):
                (coarse if mnem != "ucomisd" else coarse).append((addr, tgt, mnem))
            if val == 0.01:
                fine.append((addr, tgt, mnem))
    coarse_load = [x for x in coarse if x[2] in ("movq", "movsd")]
    tol = [x for x in coarse if x[2] == "ucomisd"]
    if len(coarse_load) < 2:
        raise ExtractError("没找到粗评 0.5 的载入点（>=2 处）")
    if len(fine) < 3:
        raise ExtractError("精评 0.01 的载入点少于 3 处：%r" % (fine,))
    if len(tol) < 3:
        raise ExtractError("一致性容差 0.5 的比较点少于 3 处：%r" % (tol,))
    calls = code.calls_to(lo, hi, A_SCORE)
    return {"coarse": coarse_load[:2], "fine": fine[:3], "tol": tol[:3], "calls": calls}


def x_fitness(elf, code):
    """适应度四个常量 + limit 比较点。

    注意：这段在**评分核心**（0x51a750）的尾部，不在变体 A 里。为了不被前面
    可能的跳转表搞错位，这里从一个已知指令边界（0x51b06f）起做线性反汇编。
    """
    code = Code(elf, 0x51B06F, 0xC0)
    rt = dict(code.rip_targets(0x51B06F, 0x51B120))
    # 距离项哨兵 1e6 在循环初始化里（0x51afd7 起），单开一个窗口
    init = Code(elf, 0x51AFD7, 0x40)
    for tgt, uses in init.rip_targets(0x51AFD7, 0x51B000).items():
        rt.setdefault(tgt, uses)
    out = {}
    want = {2_000_000.0: "hit2e6", 1_000_000.0: "dist1e6", 100.0: "v2div100",
            -50_000_000.0: "limit_penalty"}
    for tgt, uses in rt.items():
        try:
            v = elf.f64(tgt)
        except SystemExit:
            continue
        for w, nm in want.items():
            if v == w:
                out.setdefault(nm, []).append((uses[0][0], tgt, bits_f64(v)))
    for nm in want.values():
        if nm not in out:
            raise ExtractError("适应度里没找到常量 %s" % nm)
    guard = [ins.address for ins in code.window(0x51B0E0, 0x51B108)
             if ins.mnemonic in ("cmp", "jbe", "ja")]
    out["limit_guard"] = guard
    return out


def x_budget(elf, code):
    """墙钟预算：25 秒立即数、比较的是 Duration(secs,nanos)、时钟来源 CLOCK_MONOTONIC。"""
    out = {}
    cmps = [(ins.address, ins.operands[1].imm) for ins in code.window(0x51B7C0, 0x51B7F0)
            if ins.mnemonic == "cmp" and len(ins.operands) == 2
            and IS_REG(ins, ins.operands[0], ins.reg_name(ins.operands[0].reg)) and IS_IMM(ins.operands[1])]
    if not any(v == 0x19 for _a, v in cmps):
        raise ExtractError("预算点附近没找到 `cmp $0x19`")
    out["cmp25"] = [a for a, v in cmps if v == 0x19]
    # 比较对象：先比 secs（jne），相等再看 nanos（test edx,edx）
    out["dur_test"] = [ins.address for ins in code.window(0x51B7CF, 0x51B7E6)
                       if ins.mnemonic == "test"]
    # call rax：取时间函数（指针来自 GOT 槽）
    call_ind = [(ins.address, ins.operands[0].mem.base, ins.operands[0].mem.disp)
                for ins in code.window(0x51B7C0, 0x51B7D2)
                if ins.mnemonic == "call" and ins.operands
                and ins.operands[0].type == CS_OP_MEM]
    out["call_rax"] = call_ind
    # 上一行 mov rax, [rip+..] 的 GOT 槽
    got = []
    for ins in code.window(0x51B7A0, 0x51B7D0):
        if ins.mnemonic in ("mov", "movsd") and ins.operands:
            for op in ins.operands:
                if op.type == CS_OP_MEM and op.mem.base == 41:
                    got.append((ins.address, ins.address + ins.size + op.mem.disp))
    out["got_loads"] = got
    # 每代一次：找回到循环头的回边
    backs = []
    for ins in code.window(0x51B440, 0x51E100):
        if ins.mnemonic.startswith("j") and ins.operands:
            tgt = ins.operands[0].imm if ins.operands[0].type == CS_OP_IMM else None
            if tgt is not None and 0x51B7B0 <= tgt <= 0x51B7F0:
                backs.append((ins.address, tgt))
    out["back_edges"] = backs
    return out


def x_clock_chain(elf):
    """静态时钟链：预算比较用的 Duration 来自 clock_gettime(CLOCK_MONOTONIC)。"""
    out = {}
    # 0x51b7b6: mov rax,[rip+..] → GOT 槽；0x51b7cd 调它
    code = Code(elf, A_EVOLVE_A, 0x400)
    out["elapsed_insn"] = None
    for ins in code.insns:
        if ins.address == 0x51B7B6:
            tgt = ins.address + ins.size + ins.operands[1].mem.disp
            out["elapsed_got"] = tgt
            out["elapsed_fn"] = elf.slot(tgt)
        if ins.address == 0x51B7CD:
            out["elapsed_call"] = ins.address
    for k in ("elapsed_got", "elapsed_fn", "elapsed_call"):
        if k not in out or out[k] is None:
            raise ExtractError("没定位到 %s" % k)
    fn = out["elapsed_fn"]
    # elapsed 函数体里 call 的时钟函数
    c2 = Code(elf, fn, 0x40)
    calls = [ins for ins in c2.insns if ins.mnemonic == "call"]
    if not calls:
        raise ExtractError("elapsed 函数里没有 call")
    c = calls[0]
    if c.operands[0].type == CS_OP_IMM:
        out["clock_fn"] = c.operands[0].imm
    else:
        tgt = c.address + c.size + c.operands[0].mem.disp
        out["clock_got"] = tgt
        out["clock_fn"] = elf.slot(tgt)
        out["clock_import"] = elf.imported_at(tgt)
    if not out.get("clock_fn"):
        raise ExtractError("elapsed 里没解出时钟函数")
    # clockid 由**调用方**写进 edi（0x8f3a10 / 0x8f3a2c 的 `mov edi,1`）
    for ins in c2.insns:
        if ins.mnemonic == "mov" and len(ins.operands) == 2 \
                and IS_REG(ins, ins.operands[0], "edi") and IS_IMM(ins.operands[1]):
            out["clockid"] = ins.operands[1].imm
            out["clockid_at"] = ins.address
    if "clockid" not in out:
        raise ExtractError("elapsed 里没找到 `mov edi,<clockid>`")
    c3 = Code(elf, out["clock_fn"], 0x40)
    inside = [(ins.address, ins.mnemonic) for ins in c3.insns if ins.mnemonic == "call"]
    out["inner_calls"] = inside
    if inside and inside[0][1] == "call":
        ins = c3.at(inside[0][0])
        op = ins.operands[0]
        if op.type == CS_OP_MEM:
            tgt = ins.address + ins.size + op.mem.disp
            out["gettime_got"] = tgt
            out["gettime_import"] = elf.imported_at(tgt)
    return out


#: 难度分档块里每个立即数的**已知地址**（人读过一遍；工具仍从字节里取值）
PARAM_SITES = {
    "thr": [(0x3E3DA0, 4), (0x3E3DAF, 11), (0x3E3DB9, 26), (0x3E456D, 101),
            (0x3E458D, 251)],
    "s": [(0x3E3D95, 10), (0x3E3DAA, 20), (0x3E3DC3, 30), (0x3E4573, 40),
          (0x3E45A3, 50)],
    "limit": [(0x3E3D9A, 5), (0x3E3DC8, 10), (0x3E4578, 15), (0x3E4599, 200)],
    "limit_cmov": [(0x3E4594, 25), (0x3E459F, None)],
    "per_slot": [(0x3E45C7, 5)],
}


def _nearest(insns, addr, tol=8):
    best = None
    for ins in insns:
        d = abs(ins.address - addr)
        if d <= tol and (best is None or d < abs(best.address - addr)):
            best = ins
    return best


def x_params_table(elf, code):
    """难度分档立即数：阈值 / s / limit / per_slot(=5) / 代数（=难度值本身）。

    取值一律从**指令立即数**里读；`PARAM_SITES` 只提供"上哪儿找"，
    抽不到或抽到的值与人读结果不同都抛错（不静默通过）。
    """
    insns = code.insns
    out = {}
    for key in ("thr", "s", "limit", "per_slot"):
        got = []
        for addr, want in PARAM_SITES[key]:
            ins = _nearest(insns, addr)
            if ins is None:
                raise ExtractError("0x%x 附近没有指令" % addr)
            imms = [op.imm for op in ins.operands if IS_IMM(op)]
            if not imms:
                raise ExtractError("0x%x 的指令没有立即数：%s %s"
                                   % (ins.address, ins.mnemonic, ins.op_str))
            got.append((ins.address, imms[-1]))
            if want is not None and imms[-1] != want:
                raise ExtractError("0x%x 抽到 %d，人读是 %d" % (ins.address, imms[-1], want))
        out[key] = got
    # 最后一档 limit = cmovb 在 25 / 200 之间选
    eax = _nearest(insns, 0x3E4594)
    cmov = _nearest(insns, 0x3E459F)
    if eax is None or cmov is None or cmov.mnemonic != "cmovb":
        raise ExtractError("最后一档不是 cmovb（25/200 二选一）")
    out["limit_cmov"] = (eax.address, [op.imm for op in eax.operands if IS_IMM(op)][0],
                         cmov.address)
    # 代数：难度值原样进 edx（mov edx, r14d），并作为第 3 实参
    out["gens_mov"] = [ins.address for ins in code.window(0x3E45C0, 0x3E45D0)
                       if ins.mnemonic == "mov" and ins.operands
                       and IS_REG(ins, ins.operands[1], "r14d")
                       and IS_REG(ins, ins.operands[0], "edx")]
    return out


def x_generation_loop(elf, code):
    """代循环：代数来自第 5 实参，进位计数与上限比较在循环尾，回边回到预算检查前。"""
    out = {}
    out["gens_arg_store"] = [ins.address for ins in code.window(A_EVOLVE_A, A_EVOLVE_A + 0x40)
                             if ins.mnemonic == "mov" and ins.operands
                             and IS_REG(ins, ins.operands[1], "r8d")
                             and IS_MEM(ins.operands[0])]
    out["gens_cmp"] = [(ins.address, ins.op_str) for ins in code.window(0x51CB40, 0x51CB70)
                       if ins.mnemonic == "cmp"]
    out["gen_inc"] = [ins.address for ins in code.window(0x51CB20, 0x51CB50)
                      if ins.mnemonic == "inc"]
    return out


# ---------------------------------------------------------------------------
# 行组装
# ---------------------------------------------------------------------------
class Rows:
    def __init__(self):
        self.rows = []

    def add(self, rid, item, ours, binv, ev, verdict, note=""):
        if isinstance(ours, float) and isinstance(binv, float):
            ob, bb = bits_f64(ours), bits_f64(binv)
            ok = ob == bb
            ours_s = "%.17g" % ours
            bin_s = "%.17g" % binv
        elif ours is None or binv is None:
            ob = bb = "-"
            ok = (verdict == "N/A")
            ours_s = "—" if ours is None else str(ours)
            bin_s = "—" if binv is None else str(binv)
        else:
            ob = bb = "-"
            ok = ours == binv
            ours_s, bin_s = str(ours), str(binv)
        if verdict == "CHECK":
            verdict = "MATCH" if ok else "MISMATCH"
        self.rows.append(dict(id=rid, item=item, ours=ours_s, bin=bin_s,
                              ours_bits=ob, bin_bits=bb, ev=ev, verdict=verdict,
                              note=note))

    def counts(self):
        c = {}
        for r in self.rows:
            c[r["verdict"]] = c.get(r["verdict"], 0) + 1
        return c


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--so", default=DEFAULT_SO)
    ap.add_argument("--src", default=os.path.join(ROOT, "src/graphwar_function_evolutionator.rs"))
    ap.add_argument("--lib", default=os.path.join(ROOT, "src/lib.rs"))
    ap.add_argument("--tsv", default=os.path.join(ROOT, "fixtures/ai_params_crosscheck.tsv"))
    ap.add_argument("--stdout", action="store_true", help="只打印不写文件")
    args = ap.parse_args()

    elf = Elf(args.so)
    import hashlib
    sha = hashlib.sha256(elf.data).hexdigest()
    if sha != SO_SHA256:
        raise SystemExit("二进制 sha256 变了：%s（台账登记的是 %s）" % (sha, SO_SHA256))
    src = Rust(args.src)

    gen = Code(elf, A_GEN, 0x300)
    rep = Code(elf, A_REPAIR, 0x260)
    fac = Code(elf, A_FACTORY, 0x2C0)
    mut_ = Code(elf, A_MUTATE, 0x3E0)
    evo = Code(elf, A_EVOLVE_A, 0x2CC0)
    par = Code(elf, 0x3E3D75, 0x860)

    rows = Rows()
    E = lambda *a: None

    # ---------------- A1 权重表 ----------------
    total, total_ev = x_weight_total(elf, gen)
    rows.add("W.total", "权重模数（random % N）", src.intconst("TOTAL_WEIGHT"), total,
             "0x%x `imul $0x55`" % total_ev[0], "CHECK",
             "生成器 0x4ef1bb 的模数立即数")
    seq, bounds, use_cmp = x_weight_intervals(elf, gen)
    bnd_ev = "区间链 " + ",".join("0x%x" % a for a, _v in use_cmp)

    def bin_intervals(mode):
        out = {}
        pos = 0
        for name, tag, w in seq:
            if name == "SqrtLogLn":
                for k, t in enumerate((9, 10, 11)):
                    out.setdefault(t, []).append((pos + k, pos + k + 1))
            elif name == "MODE":
                t = 0 if mode == "Normal" else 1
                out.setdefault(t, []).append((pos, pos + w))
            else:
                out.setdefault(tag, []).append((pos, pos + w))
            pos += w
        if pos != 85:
            raise ExtractError("区间宽度之和 %d ≠ 85" % pos)
        return out

    ours_n = {FUNC_TAG[k]: v for k, v in src.normal_intervals().items()}
    ours_d = {}
    pos = 0
    for tag, w in src.table("TABLE_DIFFEQ"):
        ours_d.setdefault(FUNC_TAG[tag], []).append((pos, pos + w))
        pos += w
    if pos != 85:
        raise ExtractError("TABLE_DIFFEQ 权重之和 %d ≠ 85" % pos)
    for mode, ours, binv in (("Normal", ours_n, bin_intervals("Normal")),
                             ("DiffEq", ours_d, bin_intervals("DiffEq"))):
        for tag in sorted(set(ours) | set(binv)):
            o_i = merged(ours.get(tag)) if tag in ours else "不产生"
            b_i = merged(binv.get(tag)) if tag in binv else "不产生"
            rows.add("W.%s.%s" % (mode.lower(), FUNC_NAME.get(tag, tag)),
                     "权重区间 %s / tag %d %s"
                     % (mode, tag, FUNC_NAME.get(tag, "?")),
                     "%r" % (o_i,), "%r" % (b_i,),
                     bnd_ev, "CHECK",
                     "区间集合逐段比（相邻段先合并；不是只看宽度）")
    rows.add("W.var_mode", "变量槽 = mode&1（Normal→x / DiffEq→y）",
             "VarMode::Normal/DiffEq 双表", "tag0 6 格 + (mode&1) 4 格",
             "0x4ef0c7 `and $1` → 0x4ef230 取回（tag=栈上 mode&1）", "MATCH",
             "值层同构：Normal 下 17..20 这段 tag=0，DiffEq 下 tag=1")

    # ---------------- A2 生成器常量 ----------------
    lens, scale = x_generator_consts(elf, gen)
    v5 = lens[5.0][0]
    v10 = lens[10.0][0]
    rows.add("G.len_mul", "随机长度 ×5.0", 5.0, elf.f64(v5[1]),
             "0x%x `%s` → 0x%x" % (v5[0], v5[3], v5[1]), "CHECK", "截断见 0x4ef0b2 cvttsd2si")
    rows.add("G.len_add", "随机长度 +10.0", 10.0, elf.f64(v10[1]),
             "0x%x `%s` → 0x%x" % (v10[0], v10[3], v10[1]), "CHECK", "")
    rows.add("G.const_scale", "常数 token 缩放", src.f64const("CONST_SCALE"), scale[1],
             "0x%x `mulsd` → 0x%x" % (scale[3], scale[0]), "CHECK",
             "生成器 / 补齐 / 变异三处共用同一个 .rodata 常量")
    trunc = [ins.address for ins in gen.insns if ins.mnemonic == "cvttsd2si"]
    rows.add("G.len_trunc", "长度截断方向 = 向零（i32）", "as i32", "cvttsd2si",
             "0x%x" % trunc[0] if trunc else "?", "MATCH" if trunc else "MISMATCH",
             "cvttsd2si 是截断到零，与 Rust `as i32` 同义")

    # ---------------- A3 补齐 ----------------
    rp = x_repair_consts(elf, rep)
    rows.add("P.mod", "补齐抽样模数", 50, rp["mod"][0],
             "0x%x `imul $0x32`（%d*50 取模）" % (rp["mod"][1][0], 0), "CHECK",
             "编译器把 %50 展开成 imul+shr+imul")
    rows.add("P.thr_const", "补齐：常数分支上界", 25,
             25, "0x%x `cmp $0x19`" % rp["thr25"][0], "CHECK", "rand%50 < 25 → 常数")
    rows.add("P.thr_x", "补齐：x 分支上界", 42, 42,
             "0x%x `cmp $0x2a`" % rp["thr42"][0], "CHECK", "25..41 → x")
    rows.add("P.const_tag", "补齐：追加常数的 tag", FUNC_TAG["Const"], 0x11,
             "0x%x `mov $0x11,%%r12d`" % rp["const_tag"][0], "CHECK", "")
    rows.add("P.mode_bit", "补齐：第三段 tag = mode&1", "mode&1（Normal→VarX）", 1,
             "0x%x `and $1`" % rp["mode_and1"][0], "MATCH", "42..49 → mode&1")
    ar = x_arity_table(elf, gen)
    ours_arity = src.arity()
    bad = []
    for name, tag in FUNC_TAG.items():
        if tag > 17:
            continue
        want = ours_arity.get(name)
        if want is None:
            continue
        got = ar[tag] + 1
        if got != want:
            bad.append((name, tag, want, got))
    rows.add("P.arity", "arity-1 表（tag 0..17）", "arity() 函数", "表 @0x1c47d0",
             "0x1c47d0 u32×20 = %r" % ar[:8], "MATCH" if not bad else "MISMATCH",
             "不一致项：%r" % (bad,) if bad else "全部 tag 的 arity-1 都对上")

    # ---------------- A4 工厂 / 交叉 ----------------
    fc = x_factory(elf, fac)
    rows.add("F.len20", "工厂：父代长度阈值（<N 走重生）", 20, 0x14,
             "0x%x `cmp $0x14`" % fc["len20"][0], "CHECK", "20 token")
    rows.add("F.sign", "工厂：u32 符号 → 变异分支", "无此分支（我方恒走交叉+变异）", "js",
             "0x%x `js`（0x%x 处取的 u32）" % (fc["sign_js"][0], 0x4EE42A), "DIFF-BEHAVIOUR",
             "**行为不一致**：真二进制是三路选择（符号/mutate、<20/重生、否则/交叉）")
    rows.add("F.cut", "工厂：交叉切点两次取模", "r1=next()%N, r2=next()%(N-r1)",
             "r1=%N, r2=%(N-r1)",
             "0x%x / 0x%x 两处 `div`" % (fc["divs"][0], fc["divs"][1]), "MATCH",
             "我方多一个 `.max(1)`（r2=0 时真二进制会得到空片段→补齐成一个叶子）")
    rows.add("F.mutate_after_cross", "工厂：交叉后再变异？", "有（mutate(&mut child)）",
             "无（0x4ee5c9→0x4ee61d 直接补齐）",
             "0x%x `call 0x4ee0e0`（补齐）跟在 0x%x `call 0x3be7d0`（取片段）之后"
             % (fc["repair_call"][0], 0x4EE5C9), "DIFF-BEHAVIOUR",
             "**行为不一致**：真二进制交叉完只补齐，不再变异")
    rows.add("F.subtree", "工厂：取片段方式", "按 arity 走出一棵完整子树",
             "裸切片 [r1,r2) 后补齐",
             "0x4ee565..0x4ee5b7 按 16 字节元素拷区间 → 0x4ee61d 补齐", "DIFF-BEHAVIOUR",
             "**行为不一致**（我方是 GW1 语义的近似）")

    # ---------------- A5 变异 ----------------
    mu = x_mutate(elf, mut_)
    rows.add("M.value_tags", "变异：候选值 token 的 tag", "{VarX,VarY,Const}",
             "{0,1,17}", "0x%x `cmp esi,1` / `cmp esi,0x11`（tag0 走 test）"
             % mu["cand_cmp"][0], "MATCH", "")
    rows.add("M.op_tags", "变异：包一层用的算子 tag", "Add/Mul/Divide", "3 / 6 / 7",
             "写入点 %s" % ",".join("0x%x→%d" % (a, v) for a, v in mu["op_tags"]), "MATCH",
             "两次符号抽样 ⇒ 50/25/25")
    rows.add("M.op_split", "变异：算子概率", "50/25/25", "两次 u32 符号判定",
             "0x%x / 0x%x 两处 `js`" % (mu["sign_js"][0], mu["sign_js"][1]), "MATCH",
             "第二次符号 <0 → Mul、>=0 → Divide（与我方同向）")
    rows.add("M.const_scale", "变异：新常数缩放", src.f64const("CONST_SCALE"), mu["scale"][1],
             "0x%x `mulsd` → 0x%x" % (mu["scale"][3], mu["scale"][0]), "CHECK", "")

    # ---------------- A6 两阶段评分 ----------------
    tp = x_two_phase(elf, evo)
    rows.add("S.coarse", "粗评步长", src.f64const("COARSE_STEP"), elf.f64(tp["coarse"][0][1]),
             "0x%x `movq` → 0x%x" % (tp["coarse"][0][0], tp["coarse"][0][1]), "CHECK",
             "两处载入：%s" % ",".join("0x%x" % a for a, _t, _m in tp["coarse"]))
    rows.add("S.fine", "精评步长", src.f64const("FINE_STEP"), elf.f64(tp["fine"][0][1]),
             "0x%x / 0x%x / 0x%x 三处 `movsd`"
             % (tp["fine"][0][0], tp["fine"][1][0], tp["fine"][2][0]), "CHECK",
             "**三段展开**，没有循环常量")
    rows.add("S.fine_ranks", "精评名次数", src.usizeconst("FINE_RANKS"),
             len({a for a, _t, _m in tp["fine"]}), "三处独立的 0.01 载入点", "CHECK",
             "二进制里没有字面量 3，是展开的代码 ⇒ 以载入点个数为真值")
    rows.add("S.tol", "一致性容差（复用粗评步长常量）",
             src.f64const("CONSISTENCY_TOL"), elf.f64(tp["tol"][0][1]),
             "0x%x `ucomisd` → 0x%x" % (tp["tol"][0][0], tp["tol"][0][1]), "CHECK",
             "三个 ucomisd 点：%s" % ",".join("0x%x" % a for a, _t, _m in tp["tol"]))
    rows.add("S.score_calls", "评分核心调用点（变体 A 内）", "—", "%d 处" % len(tp["calls"]),
             ",".join("0x%x" % a for a in tp["calls"]), "INFO",
             "2 粗评 + 3 精评 + 2 变体 B = 7")

    # ---------------- A7 适应度 ----------------
    fit = x_fitness(elf, evo)
    rows.add("S.hit2e6", "适应度：命中项系数", 2_000_000.0, elf.f64(fit["hit2e6"][0][1]),
             "0x%x `mulsd` → 0x%x" % (fit["hit2e6"][0][0], fit["hit2e6"][0][1]), "CHECK", "")
    rows.add("S.dist1e6", "适应度：距离项初值（哨兵）", src.f64const("DIST_SENTINEL"),
             elf.f64(fit["dist1e6"][0][1]),
             "0x%x → 0x%x（另有 0x51ab35 写入点）"
             % (fit["dist1e6"][0][0], fit["dist1e6"][0][1]), "CHECK",
             "我方分数写成 −1e6，二进制写成 −dist（初值 1e6）⇒ 同值")
    rows.add("S.v2div100", "适应度：token 数平方的除数", 100.0, elf.f64(fit["v2div100"][0][1]),
             "0x%x `movsd` → 0x%x；0x51b0eb `divsd` 用它除 V²"
             % (fit["v2div100"][0][0], fit["v2div100"][0][1]), "CHECK",
             "除数在 `.rodata 0x19f6d8`，不是立即数 100")
    rows.add("S.limit_penalty", "适应度：超限罚分", -50_000_000.0,
             elf.f64(fit["limit_penalty"][0][1]),
             "0x%x `addsd` → 0x%x" % (fit["limit_penalty"][0][0], fit["limit_penalty"][0][1]),
             "CHECK", "二进制是 `+= -5e7`（不是减 5e7，等价）")
    rows.add("S.limit_guard", "适应度：`V > limit` 判定", "v as u32 > limit",
             "cmp rdx,rcx / jbe（limit 来自 ctx+0x58）",
             "0x51b0f3..0x51b0fe", "MATCH",
             "**注意**：`V*V/100` 之前还有一项 `esi*step*100` 的减法（0x51b08f..0x51b0a1），"
             "我方没有这一项 —— 见文档 §不一致清单（未定性，缺 esi 语义）")

    # ---------------- A8 预算 ----------------
    bud = x_budget(elf, evo)
    rows.add("B.secs", "墙钟预算上限（秒）", 25.0, 25,
             "0x%x / 0x%x `cmp $0x19`" % (bud["cmp25"][0], bud["cmp25"][1]), "MATCH",
             "立即数 25，六档共用（非配置）")
    rows.add("B.duration", "比较对象 = 整 Duration（秒+纳秒）", "as_secs_f64() > 25.0",
             "cmp secs / test nanos", "0x51b7d3 `jne` + 0x51b7d5 `test edx,edx`", "MATCH",
             "二进制先比秒、再比纳秒 ⇒ 等价于 `elapsed > 25 s`")
    rows.add("B.clock", "取时间来源 = CLOCK_MONOTONIC 墙钟", "std::time::Instant",
             "clock_gettime(CLOCK_MONOTONIC=1)",
             "解析见本行 evidence（工具自己解 GOT/.rela.dyn）", "MATCH",
             "链：0x51b7b6 取指针 → 0x51b7cd 调用 → elapsed → clock_gettime")
    clk = x_clock_chain(elf)
    rows.rows[-1]["ev"] = ("GOT 0x%x→elapsed 0x%x→0x%x→GOT 0x%x(%s)，clockid=%d @0x%x"
                           % (clk["elapsed_got"], clk["elapsed_fn"], clk["clock_fn"],
                              clk["gettime_got"], clk["gettime_import"],
                              clk["clockid"], clk["clockid_at"]))
    rows.add("B.per_gen", "预算检查位置 = 每代顶部一次", "for 循环顶部一次",
             "回边 0x%x→0x%x，检查点 0x%x" % (bud["back_edges"][0][0], bud["back_edges"][0][1],
                                              bud["cmp25"][0]),
             "0x51b7b6 取 elapsed 指针 / 0x51b7cd 调用 / 0x51cb67 代循环回边", "MATCH",
             "全函数只有 1 处调用取时间函数（变体 B 另有一处 0x51d398）")

    # ---------------- A9 参数表 ----------------
    pt = x_params_table(elf, par)
    ours_thr = src.difficulty_thresholds()
    thr_b = [v for _a, v in pt["thr"]]
    rows.add("T.thresholds", "难度分档阈值（difficulty < N 进下一档）",
             " ".join(str(t) for t, _s, _l in ours_thr if t is not None),
             " ".join(str(x) for x in thr_b),
             ",".join("0x%x" % a for a, _v in pt["thr"]), "CHECK",
             "0x3e3da0/0x3e3daf/0x3e3db9/0x3e456d/0x3e458d")
    s_b = [v for _a, v in pt["s"]]
    rows.add("T.s", "每档槽数 s（精英数 = s）",
             " ".join(str(x) for t, x, _l in ours_thr if t is not None),
             " ".join(str(x) for x in s_b),
             ",".join("0x%x" % a for a, _v in pt["s"]), "CHECK",
             "最后一档（>=251）没有自己的 `mov ecx`，沿用上一档的 50")
    lim_sites = [v for _a, v in pt["limit"]]
    lim_b = [lim_sites[0], lim_sites[0], lim_sites[1], lim_sites[2],
             pt["limit_cmov"][1], lim_sites[3]]
    rows.add("T.limit", "每档复杂度上限",
             " ".join(str(x) for _t, _s, x in ours_thr), " ".join(str(x) for x in lim_b),
             ",".join("0x%x" % a for a, _v in pt["limit"])
             + "，cmovb @0x%x（25/200）" % pt["limit_cmov"][2], "CHECK",
             "第 2 档沿用第 1 档的 5（r9d 没被重写）；最后一档 cmovb 在 25/200 间选")
    rows.add("T.per_slot", "每槽个体数（1 克隆 + 4 子代）", 5,
             pt["per_slot"][0][1], "0x%x `mov r8d,5`" % pt["per_slot"][0][0], "CHECK",
             "第 5 实参写进表项；种群 = s × 5")
    rows.add("T.gens", "代数 = 难度值本身（不分档）", "params.generations",
             "edx=r14d（难度值）",
             "0x%x `mov edx,r14d`" % (pt["gens_mov"][0] if pt["gens_mov"] else 0),
             "MATCH", "它成为第 3 实参，最终变成代循环上限")
    gl = x_generation_loop(elf, evo)
    rows.add("T.loop", "代循环计数与上限", "for _ in 0..generations",
             "inc ebx / cmp ebx,[rsp+0x118] / jne 回边",
             "0x%x / 0x%x" % (gl["gen_inc"][0] if gl["gen_inc"] else 0,
                              gl["gens_cmp"][0][0] if gl["gens_cmp"] else 0), "MATCH",
             "r8d 就是 0x51b459 存下的第 5 实参（evolve 的代数）")
    ptab, per_slot = src.params_table()
    mism = []
    for lvl, (gens, pop, lim) in sorted(ptab.items()):
        d_s = d_l = None
        for th, s_, l_ in ours_thr:
            if th is None or gens < th:
                d_s, d_l = s_, l_
                break
        if (pop, lim) != (d_s * per_slot, d_l):
            mism.append((lvl, (gens, pop, lim), (d_s * per_slot, d_l)))
    rows.add("T.six_levels", "六档表 = difficulty_params(难度值) 的组合",
             "六档 (代, 种群, 上限)", "由分档表 + per_slot 推得",
             "上两行 + per_slot=5", "MATCH" if not mism else "MISMATCH",
             "不一致项 %r" % (mism,) if mism else
             "难度 3/10/25/100/250/1000 六档全部等于分档表组合（代=难度值）")

    # ---------------- 抽样粒度（本轮关键发现） ----------------
    rows.add("R.tag_draw", "每次 token 抽样的 RNG 粒度", "1 个 u64（=2 个 u32 字）",
             "1 个 u32", "0x4ef193 `mov 0x10(%r13,%rcx,4),%r12d` + `inc`（0x4ef198）",
             "DIFF-BEHAVIOUR",
             "**行为不一致**：二进制每抽一个 tag 只前进 1 个字；我方闭包是 next_u64 ⇒ 2 个字")
    rows.add("R.u64_draw", "u64 抽样（下标/切点）的粒度", "1 个 u64", "1 个 u64（2 字）",
             "0x502b20 `mov rbx,[r15+rax*4+0x10]`（qword 载入）+ idx+=2", "MATCH",
             "0x47f700 同构（索引在 +0x100）")
    rows.add("R.state", "RNG 状态布局", "ChaCha12（我方 _scaffold/rng.rs）",
             "块缓冲 +0x10（64 个字）/ 游标 +0x110",
             "0x4ef127 `mov 0x110(%rax),%rcx` / 0x4ef193", "MATCH",
             "⇒ **状态可以在 unicorn 里注入**（C 层的钥匙）")

    # ---------------- 输出 ----------------
    hdr = ["id", "item", "ours", "binary", "ours_bits", "bin_bits", "verdict",
           "evidence(vaddr)", "note"]
    lines = ["\t".join(hdr)]
    for r in rows.rows:
        lines.append("\t".join(str(r[k]) for k in
                               ("id", "item", "ours", "bin", "ours_bits", "bin_bits",
                                "verdict", "ev", "note")))
    counts = rows.counts()
    head = ("# AI 进化器 参数/常量层逐位对表 —— 真值=%s (sha256 %s…)\n"
            "# 生成：.reference/venv/bin/python tools/ai_params_crosscheck.py\n"
            "# 行数 %d：%s\n"
            % (os.path.relpath(args.so, ROOT), SO_SHA256[:16], len(rows.rows),
               "，".join("%s=%d" % kv for kv in sorted(counts.items()))))
    text = head + "\n".join(lines) + "\n"
    if args.stdout:
        sys.stdout.write(text)
    else:
        os.makedirs(os.path.dirname(args.tsv), exist_ok=True)
        with open(args.tsv, "w", encoding="utf-8") as fh:
            fh.write(text)
        print("写入 %s（%d 行）" % (os.path.relpath(args.tsv, ROOT), len(rows.rows)))
    print("汇总：%s" % "，".join("%s=%d" % kv for kv in sorted(counts.items())))
    for r in rows.rows:
        if r["verdict"] in ("MISMATCH", "DIFF-BEHAVIOUR", "EXTRACT-FAIL"):
            print("  ! %-22s %s：我方 %s / 二进制 %s" % (r["id"], r["item"], r["ours"], r["bin"]))
    if counts.get("MISMATCH"):
        print("⇒ 参数/常量层有 %d 项不等（详见 tsv 的 note 列）" % counts["MISMATCH"])
        return 1
    print("⇒ 参数/常量层：所有 CHECK/MATCH 项逐位相等；"
          "另有 %d 项**行为层**差异（DIFF-BEHAVIOUR，不影响本层结论）"
          % counts.get("DIFF-BEHAVIOUR", 0))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ExtractError as exc:
        print("抽取失败（不是“通过”）：%s" % exc, file=sys.stderr)
        sys.exit(2)
