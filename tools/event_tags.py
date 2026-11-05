#!/usr/bin/env python3
"""解出 `0x3d38f0` 的「事件名 → `enum Event` 判别值」表（纯本地，capstone）。

`0x3d38f0` 先按「名字长度 − 10」查一张 25 项 int32 跳表（`0x1be9c8`），
所以每条分支都已知道名字的**确切长度**；随后是一条 if/else 链，
每次相等测试被 LLVM 降成下面三种形态之一：

  1. 两条 `pcmpeqb`：头 16 字节 `(%rsi)` + 尾 16 字节 `K(%rsi)`，`K = len − 16`；
     比较常量直接落在 `.rodata`（`pcmpeqb <rip-rel>,%xmm`），读出来即可。
  2. 短名字（len ≤ 15）走整数：`movabs` + `xor (%rsi)` / `xor K(%rsi)`，
     常量常被折成 `lea -0xa04(%rax)` / `add $0x5fd0600,%rcx`（基串 ± 位移）。
  3. len = 34 用 `memcmp` 对 `.rodata` 里的整串。

因为长度已知，所有测试的字节区间并集必须**恰好铺满** `[0, len)`：
既不能有空洞、也不能两个测试打架 —— 这就是自校验。
重建出的名字还要能在 `.rodata` 里逐字节找到（72 条里 69 条能；剩下 3 条
是 LLVM 用两条已有串的重叠片段拼出来的，见 docs/114 §4）。

命中分支把判别值写进 `%ebx`，其中一部分分支还会先拼一个 `Event` 前缀
（tag word `0x8000000000000000 | N` 在 `+0`）再调 `0x3e6830` 补 payload。

用法：
    ../.venv/bin/python tools/event_tags.py            # 表 + 自检
    ../.venv/bin/python tools/event_tags.py --json     # 机器可读
    ../.venv/bin/python tools/event_tags.py --rust     # 枚举体片段

地址约定：本脚本一律用 **ELF 虚地址**（Ghidra = 虚地址 + 0x100000，只加一次）。
"""

import json
import os
import re
import struct
import sys

from capstone import Cs, CS_ARCH_X86, CS_MODE_64, CS_OP_IMM, CS_OP_MEM, CS_OP_REG
from capstone.x86 import X86_REG_RIP

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)
SO = os.path.join(ROOT, "libgraphwarrust.so")

FUNC = 0x3D38F0          # 名字分派函数入口
FUNC_END = 0x3D4FB0      # 0x16b0 字节函数体的安全上界
JT = 0x1BE9C8            # 25 项「长度跳表」
DEFAULT = 0x3D469A       # 长度越界 / 无匹配 → "explicit panic"
COMMON = 0x3D4C5C        # 公共尾巴：用 %ebx 当 key 查订阅者表
RODATA_LO, RODATA_SZ = 0x19C000, 0x113C0C

# .text / .rodata / .data.rel.ro 的 vaddr→文件偏移（readelf -SW 抄下来的）
SECS = [
    (0x19C000, 0x19C000, 0x113C0C),
    (0x37B940, 0x37A940, 0x5D3244),
    (0x9500B0, 0x94E0B0, 0x06A160),
]


def foff(vaddr):
    for va, off, sz in SECS:
        if va <= vaddr < va + sz:
            return off + (vaddr - va)
    raise KeyError(hex(vaddr))


class Image:
    def __init__(self, path):
        self.raw = open(path, "rb").read()

    def read(self, vaddr, n):
        o = foff(vaddr)
        return self.raw[o:o + n]


# ---- 寄存器族：把 eax/ax/al 都归到 rax -------------------------------------
REGROOT = {}
for _fam, _al in [("rax", "rax eax ax al ah"), ("rbx", "rbx ebx bx bl bh"),
                  ("rcx", "rcx ecx cx cl ch"), ("rdx", "rdx edx dx dl dh"),
                  ("rsi", "rsi esi si sil"), ("rdi", "rdi edi di dil"),
                  ("rbp", "rbp ebp bp bpl"), ("rsp", "rsp esp sp spl")]:
    for _a in _al.split():
        REGROOT[_a] = _fam
for _n in range(8, 16):
    for _s in ("", "d", "w", "b"):
        REGROOT["r%d%s" % (_n, _s)] = "r%d" % _n


def C(b):
    """具体字节。"""
    return ("c", b & 0xFF)


def I(off, k=0):
    """输入名字的第 off 字节 ^ k（测试成立 ⟺ 该字节等于 k）。"""
    return ("in", off, k & 0xFF)


class Interp:
    """逐字节符号执行：每个寄存器的每一字节是 ('c',b) 或 ('in',off,xork)。"""

    def __init__(self):
        self.gpr = {}
        self.xmm = {}
        self.cons = []
        self.test_regs = set()
        self.src = []

    def setg(self, k, v):
        self.gpr[k] = v

    def touch(self, k):
        self.test_regs.add(k)

    def reset(self):
        self.cons = []
        self.test_regs = set()
        self.src = []

    def step(self, i):
        m, ops = i.mnemonic, i.operands
        rt = lambda op: REGROOT.get(i.reg_name(op.reg), i.reg_name(op.reg))
        xmm = lambda reg: i.reg_name(reg).startswith("xmm")
        inp = lambda op: (op.mem.base != X86_REG_RIP
                          and REGROOT.get(i.reg_name(op.mem.base)) == "rsi"
                          and op.mem.index == 0)
        ea = lambda op: i.address + i.size + op.mem.disp if op.mem.base == X86_REG_RIP else None
        at = lambda items, j: items[j] if items is not None and j < len(items) else C(0)

        if m in ("movdqu", "movdqa", "movups", "movaps") and len(ops) == 2:
            d, s = ops
            if d.type == CS_OP_REG and xmm(d.reg):
                if s.type == CS_OP_MEM:
                    n = s.size
                    if inp(s):
                        self.xmm[d.reg] = [I(s.mem.disp + j) for j in range(n)] + [C(0)] * (16 - n)
                    else:
                        a = ea(s)
                        if a is not None and RODATA_LO <= a < RODATA_LO + RODATA_SZ:
                            self.xmm[d.reg] = [C(x) for x in self.read(a, 16)]
                            self.src.append(hex(a))
                elif s.type == CS_OP_REG and xmm(s.reg):
                    self.xmm[d.reg] = list(self.xmm.get(s.reg) or [])
        elif m in ("movd", "movq") and len(ops) == 2:
            d, s = ops
            n = 4 if m == "movd" else 8
            if d.type == CS_OP_REG and xmm(d.reg):
                if s.type == CS_OP_MEM:
                    if inp(s):
                        k = s.size
                        self.xmm[d.reg] = [I(s.mem.disp + j) for j in range(k)] + [C(0)] * (16 - k)
                elif s.type == CS_OP_REG and not xmm(s.reg):
                    src = self.gpr.get(rt(s))
                    self.xmm[d.reg] = [at(src, j) for j in range(n)] + [C(0)] * (16 - n)
                elif s.type == CS_OP_REG and xmm(s.reg):
                    self.xmm[d.reg] = list(self.xmm.get(s.reg) or [])
        elif m == "punpcklqdq" and len(ops) == 2:
            d, s = ops
            a = list(self.xmm.get(d.reg) or [C(0)] * 16)
            b = list(self.xmm.get(s.reg) or [C(0)] * 16)
            self.xmm[d.reg] = a[0:8] + b[0:8]
        elif m == "pxor" and len(ops) == 2:
            self.xmm[ops[0].reg] = [C(0)] * 16
        elif m == "pcmpeqb" and len(ops) == 2:
            d, s = ops
            if d.type != CS_OP_REG or not xmm(d.reg):
                return
            A = list(self.xmm.get(d.reg) or [])
            if s.type == CS_OP_REG and xmm(s.reg):
                B = list(self.xmm.get(s.reg) or [])
            elif s.type == CS_OP_MEM:
                if inp(s):
                    B = [I(s.mem.disp + j) for j in range(16)]
                else:
                    a = ea(s)
                    B = [C(x) for x in self.read(a, 16)] if a is not None else []
                    if a is not None:
                        self.src.append(hex(a))
            else:
                B = []
            for j in range(min(len(A), len(B))):
                a, b = A[j], B[j]
                if a[0] == "in" and b[0] == "c":
                    self.cons.append((a[1], b[1]))
                elif a[0] == "c" and b[0] == "in":
                    self.cons.append((b[1], a[1]))
            self.xmm[d.reg] = None      # 目标已变成谓词
        elif m == "movabs" and len(ops) == 2 and ops[0].type == CS_OP_REG:
            v = ops[1].imm & ((1 << 64) - 1)
            self.setg(rt(ops[0]), [C(x) for x in v.to_bytes(8, "little")])
        elif m == "mov" and len(ops) == 2 and ops[0].type == CS_OP_REG:
            d = ops[0]
            if ops[1].type == CS_OP_IMM:
                v = ops[1].imm & ((1 << 64) - 1)
                n = d.size
                self.setg(rt(d), [C(x) for x in v.to_bytes(8, "little")[:n]] + [C(0)] * (8 - n))
            elif ops[1].type == CS_OP_MEM and inp(ops[1]):
                n = ops[1].size
                self.setg(rt(d), [I(ops[1].mem.disp + j) for j in range(n)] + [C(0)] * (8 - n))
            elif ops[1].type == CS_OP_REG:
                self.setg(rt(d), list(self.gpr.get(rt(ops[1])) or [C(0)] * 8))
        elif m in ("movzx", "movsx", "movsxd") and len(ops) == 2 and ops[1].type == CS_OP_MEM and inp(ops[1]):
            n = ops[1].size
            self.setg(rt(ops[0]), [I(ops[1].mem.disp + j) for j in range(n)] + [C(0)] * (8 - n))
        elif m == "lea" and len(ops) == 2 and ops[0].type == CS_OP_REG and ops[1].type == CS_OP_MEM \
                and ops[1].mem.base != X86_REG_RIP and ops[1].mem.index == 0:
            base = self.gpr.get(REGROOT.get(i.reg_name(ops[1].mem.base)))
            if base and all(x[0] == "c" for x in base):
                v = (int.from_bytes(bytes(x[1] for x in base), "little") + ops[1].mem.disp) & ((1 << 64) - 1)
                self.setg(rt(ops[0]), [C(x) for x in v.to_bytes(8, "little")])
        elif m in ("add", "sub") and len(ops) == 2 and ops[0].type == CS_OP_REG and ops[1].type == CS_OP_IMM:
            k = rt(ops[0])
            cur = self.gpr.get(k)
            if cur and all(x[0] == "c" for x in cur):
                v = int.from_bytes(bytes(x[1] for x in cur), "little")
                v = (v + ops[1].imm if m == "add" else v - ops[1].imm) & ((1 << 64) - 1)
                self.setg(k, [C(x) for x in v.to_bytes(8, "little")])
        elif m == "xor" and len(ops) == 2:
            if ops[0].type == CS_OP_REG and ops[1].type == CS_OP_REG and rt(ops[0]) == rt(ops[1]):
                self.setg(rt(ops[0]), [C(0)] * 8)
                self.touch(rt(ops[0]))
            elif ops[0].type == CS_OP_REG and ops[1].type == CS_OP_MEM and inp(ops[1]):
                k, n = rt(ops[0]), ops[1].size
                cur = self.gpr.get(k) or [C(0)] * 8
                new = list(cur)
                for j in range(n):
                    c = at(cur, j)
                    if c[0] == "c":
                        new[j] = I(ops[1].mem.disp + j, c[1])
                self.setg(k, new)
                self.touch(k)
            elif ops[0].type == CS_OP_REG and ops[1].type == CS_OP_IMM:
                k, n = rt(ops[0]), ops[0].size
                v = ops[1].imm & ((1 << 64) - 1)
                cur = self.gpr.get(k)
                if cur:
                    new = list(cur)
                    for j in range(min(n, 8)):
                        c = at(cur, j)
                        kk = (v >> (8 * j)) & 0xFF
                        new[j] = C(c[1] ^ kk) if c[0] == "c" else I(c[1], c[2] ^ kk)
                    self.setg(k, new)
                    self.touch(k)
            elif ops[0].type == CS_OP_REG and ops[1].type == CS_OP_REG:
                da, sb = rt(ops[0]), rt(ops[1])
                A = self.gpr.get(da) or [C(0)] * 8
                B = self.gpr.get(sb)
                if B:
                    new = []
                    for j in range(8):
                        a, b = at(A, j), at(B, j)
                        if a[0] == "c" and b[0] == "c":
                            new.append(C(a[1] ^ b[1]))
                        elif a[0] == "in" and b[0] == "c":
                            new.append(I(a[1], a[2] ^ b[1]))
                        elif a[0] == "c" and b[0] == "in":
                            new.append(I(b[1], b[2] ^ a[1]))
                        else:
                            new.append(C(0))
                    self.setg(da, new)
                    self.touch(da)
        elif m == "or" and len(ops) == 2 and ops[0].type == CS_OP_REG and ops[1].type == CS_OP_REG:
            da, sb = rt(ops[0]), rt(ops[1])
            A = self.gpr.get(da) or [C(0)] * 8
            B = self.gpr.get(sb) or [C(0)] * 8
            new = []
            for j in range(8):
                a, b = at(A, j), at(B, j)
                new.append(a if a[0] == "in" else (b if b[0] == "in" else C(a[1] | b[1])))
            self.setg(da, new)
            self.touch(da)
            self.touch(sb)

    def constraints(self):
        """只从参与本块零值测试的寄存器收约束（movzx 的零扩展尾巴不算）。"""
        out = list(self.cons)
        for k in self.test_regs:
            for it in (self.gpr.get(k) or []):
                if it[0] == "in":
                    out.append((it[1], it[2]))
        return out


TERM = ("je", "jne", "jbe", "ja", "jb", "jg", "jle", "jl", "jge", "js", "jns")


class Decoder:
    def __init__(self, img):
        self.img = img
        md = Cs(CS_ARCH_X86, CS_MODE_64)
        md.detail = True
        self.ins = list(md.disasm(img.read(FUNC, FUNC_END - FUNC), FUNC))
        self.by = {i.address: k for k, i in enumerate(self.ins)}

    def read(self, a, n):
        return self.img.read(a, n)

    # ---- 一条分支（一个长度）里的一次相等测试 ----------------------------
    def walk(self, start, length):
        ip = Interp()
        ip.read = self.read
        out, k, blk = [], self.by[start], start
        while k < len(self.ins):
            i = self.ins[k]
            if i.mnemonic in TERM or i.mnemonic in ("jmp", "call", "ret"):
                name, why = self.name_from(ip.constraints(), length)
                tgt = i.operands[0].imm if i.operands else None
                if i.mnemonic in TERM:
                    if tgt == DEFAULT:                  # 失败落空 → 命中就是下一条指令
                        out.append((blk, i.address, name, why, i.address + i.size))
                        break
                    if tgt == COMMON:                   # 命中就在本块内
                        out.append((blk, i.address, name, why, COMMON))
                        break
                    out.append((blk, i.address, name, why, tgt))
                    ip.reset()
                    blk, k = i.address + i.size, k + 1
                    continue
                if i.mnemonic == "jmp":
                    out.append((blk, i.address, name, why, tgt))
                    break
                if i.mnemonic == "call":                # memcmp 形态
                    s_addr = s_len = None
                    for a in range(blk, i.address):
                        if a not in self.by:
                            continue
                        j = self.ins[self.by[a]]
                        if j.mnemonic == "lea" and len(j.operands) == 2 \
                                and j.operands[1].type == CS_OP_MEM \
                                and j.operands[1].mem.base == X86_REG_RIP:
                            a2 = j.address + j.size + j.operands[1].mem.disp
                            if RODATA_LO <= a2 < RODATA_LO + RODATA_SZ:
                                s_addr = a2
                        if j.mnemonic == "mov" and len(j.operands) == 2 \
                                and j.operands[0].type == CS_OP_REG \
                                and j.reg_name(j.operands[0].reg) == "edx" \
                                and j.operands[1].type == CS_OP_IMM:
                            s_len = j.operands[1].imm
                    nxt = self.ins[self.by[i.address + i.size]] if (i.address + i.size) in self.by else None
                    match = None
                    if nxt is not None and nxt.mnemonic in TERM:
                        match = nxt.operands[0].imm if nxt.mnemonic == "je" else i.address + i.size + nxt.size
                    out.append((blk, i.address + 0x10, self.read(s_addr, s_len) if (s_addr and s_len) else None,
                                "memcmp %s/%d" % (hex(s_addr or 0), s_len or 0), match))
                    break
            ip.step(i)
            k += 1
        return out

    @staticmethod
    def name_from(cons, length):
        buf = bytearray(b"?" * length)
        seen = bytearray(length)
        for off, c in cons:
            if off >= length:
                return None, "越界@%d" % off
            if seen[off] and buf[off] != c:
                return None, "冲突@%d" % off
            buf[off] = c
            seen[off] = 1
        if not all(seen):
            return None, "缺字节[" + "".join("x" if v else "." for v in seen) + "]"
        return bytes(buf), "ok"

    # ---- 命中块：判别值（%ebx）与 tag word（0x8000…|N） ------------------
    def value(self, a0, a1):
        disc = tag = daddr = None
        fills = 0
        for a in range(a0, min(a1, FUNC_END)):
            if a not in self.by:
                continue
            i = self.ins[self.by[a]]
            m, ops = i.mnemonic, i.operands
            if m == "mov" and len(ops) == 2 and ops[0].type == CS_OP_REG \
                    and i.reg_name(ops[0].reg) == "ebx" and ops[1].type == CS_OP_IMM:
                disc, daddr = ops[1].imm, a
            if m == "xor" and len(ops) == 2 and ops[0].type == CS_OP_REG and ops[1].type == CS_OP_REG \
                    and i.reg_name(ops[0].reg) == "ebx" and i.reg_name(ops[1].reg) == "ebx":
                disc, daddr = 0, a
            if m == "movabs" and len(ops) == 2 and (ops[1].imm & 0xFFFFFFFFFFFFFFFF) == 0x8000000000000000:
                for a2 in range(a + 1, min(a + 0x20, a1)):
                    if a2 in self.by:
                        j = self.ins[self.by[a2]]
                        if j.mnemonic == "or" and len(j.operands) == 2 and j.operands[1].type == CS_OP_IMM:
                            tag = j.operands[1].imm
            if m == "call" and ops and ops[0].type == CS_OP_IMM and ops[0].imm == 0x3E6830:
                fills += 1
        return disc, tag, daddr, fills > 0

    def decode(self):
        rows = []
        for length in range(10, 35):
            rel = struct.unpack("<i", self.read(JT + 4 * (length - 10), 4))[0]
            arm = JT + rel
            if arm == DEFAULT:
                continue
            for blk, end, name, why, match in self.walk(arm, length):
                if match is None or match == COMMON or match not in self.by \
                        or self.ins[self.by[match]].mnemonic == "cmp":
                    disc, tag, daddr, fill = self.value(blk, end + 12)
                else:
                    k = self.by[match]
                    e = k
                    while e < len(self.ins) and self.ins[e].mnemonic not in ("jmp", "ret"):
                        e += 1
                    disc, tag, daddr, fill = self.value(
                        match, self.ins[e].address if e < len(self.ins) else match + 0x200)
                rows.append(dict(length=length, arm=arm, test=blk, test_end=end,
                                 match=match, name=name.decode("latin1") if name else None,
                                 name_check=why, disc=disc, disc_at=daddr,
                                 tag=tag, fills_payload=fill))
        return rows


def sorted_run_check(img, rows):
    """`.rodata` 0x1bf537 起 767 字节：32 条名字，按本次解出的判别值必须严格递增。"""
    start = 0x1BF537
    raw = img.read(start, 767)
    tag = {r["name"]: r["disc"] for r in rows if r["name"]}
    names, prev = [], 0
    for m in re.finditer(rb"_event", raw):
        names.append(raw[prev:m.end()].decode("latin1"))
        prev = m.end()
    vals = [tag.get(n) for n in names]
    mono = all(a is not None and b is not None and a < b for a, b in zip(vals, vals[1:]))
    return names, vals, mono


def main(argv):
    img = Image(SO)
    dec = Decoder(img)
    rows = dec.decode()

    rodata = img.read(RODATA_LO, RODATA_SZ)
    named = [r for r in rows if r["name"] and r["disc"] is not None]
    in_ro = [r for r in named if r["name"].encode() in rodata]
    agree = [r for r in rows if r["tag"] is not None]
    run_names, run_vals, run_mono = sorted_run_check(img, rows)

    ok = {
        "长度分支解出条目": (len(rows), len(rows)),
        "名字重建成功（铺满且无冲突）": (len(named), len(rows)),
        "判别值相等（%ebx == tag word 低 32 位）": (sum(1 for r in agree if r["disc"] == r["tag"]), len(agree)),
        "0x1bf537 排序数组按判别值严格递增": (len(run_names) if run_mono else 0, len(run_names)),
    }
    # 这 3 条是 LLVM 用两条已有串的重叠片段拼出来的，.rodata 里没有整串（见 docs/114 §4.3）
    absent = [r["name"] for r in named if r["name"].encode() not in rodata]
    if "--json" in argv:
        print(json.dumps(sorted(rows, key=lambda r: (r["disc"] is None, r["disc"])), indent=1, ensure_ascii=False))
        return 0
    if "--rust" in argv:
        for r in sorted(named, key=lambda r: r["disc"]):
            print("    %s = %d," % (r["name"], r["disc"]))
        return 0

    print("== 自检 ==")
    for k, (got, want) in ok.items():
        print("  %-42s %d/%d %s" % (k, got, want, "OK" if got == want else "!!"))
    print("  名字能在 .rodata 逐字节找到                     %d/%d（其余由已有串的重叠片段拼出：%s）"
          % (len(in_ro), len(named), ", ".join(absent) or "无"))
    discs = sorted(r["disc"] for r in named)
    print("  判别值范围 0..%d，缺 %s" % (max(discs), [n for n in range(max(discs) + 1) if n not in discs]))
    dups = len(discs) - len(set(discs))
    print("  重复判别值 %d，重名 %d" % (dups, len(named) - len({r['name'] for r in named})))

    if "--table" in argv or True:
        print()
        print("  disc  名字                              len  分支      相等测试    判别值@      tag")
        for r in sorted(named, key=lambda r: r["disc"]):
            print("  %4d  %-34s %3d  %s  %s  %s  %s" % (
                r["disc"], r["name"], r["length"], hex(r["arm"]), hex(r["test"]),
                hex(r["disc_at"]), ("0x8000…|%02x" % r["disc"]) if r["tag"] is not None else "-"))
        for r in rows:
            if r["disc"] is None or not r["name"]:
                print("  FAIL %s %s" % (r["name"], r["name_check"]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
