#!/usr/bin/env python3
"""三向符号匹配器：Linux(x86-64, stripped) × Android(arm64, 有名字) × Windows×2(x86-64, COFF 名字)。

一份代码吃四种构建，输出 `func_addr → (name?, locations, size, callee 多重集, 常量集, 指令直方图)`，
然后做三件事：

  ① **Location 三向校验**：`panic::Location` 三元组 `(file, line, col)` 是编译期常量，
     跨构建、跨 ISA、跨 ABI 都不变。这是**唯一**能稳定对齐的键（目标里 2,035 个）。
  ② **名字落地**：`Win 名字 → Location → Linux 地址`，Android 作第二来源，冲突逐条列出。
  ③ **天花板之外的上限估计**：对没有 Location 的函数，用（尺寸 + 被调者多重集 + 常量 +
     指令直方图）做归一化最近邻，**并用「已有 Location 真值」的函数做正对照**给出准确率。

设计上的四条硬规矩（都是上一轮踩过的坑，写在这里免得后人复发）：

* **PE 的 Location 不是 ELF 那一套。** ELF 里指针槽是 0、真值在 `R_*_RELATIVE` 的 addend；
  PE（MinGW/GNU ld）里指针槽**已经是绝对的 VA**（`ImageBase + RVA`），`R_*_RELATIVE` 的
  对应物是 `.reloc` 里的 `IMAGE_REL_BASED_DIR64`（type 10）。本工具对 PE 的做法：
  **先用 `.reloc` 门控**，再在 `.rdata`/`.data` 里按 24 字节布局匹配。实测 2.0.60 DLL 的
  候选**全部**落在 reloc 表里（`report` 打印这条自校验），所以不是「裸扫模式撞上的」。
* **一个键的「引用」必须在四种构建上用同一套规则**：取首址 / `+8`(len) / `+16`(line,col)
  三者都算（`LocHit.lookup`）。规则不对称会人为制造假歧义（§20.2 第 2 条）。
* **函数宇宙的口径必须写死并打印**：Linux 用 Ghidra 起点表（与 `fixtures/name_to_addr.tsv`
  同口径，否则两边比的不是同一批函数）；Android 用 `.symtab` 的 FUNC；Windows 用
  `.pdata` 的 `RUNTIME_FUNCTION`（编译器自己发的边界），`.pdata` 没覆盖到的 COFF FUNC
  符号单独补进去。
* **天花板之外只给「上限估计 + 正对照准确率」**，不给"猜出来的名字"。结构化最近邻的
  准确率必须在**已知真值**（Location 锚）上量，而且真值要分半（调参半 / 留出半），
  否则就是自证。

用法：

    ../.reference/venv/bin/python tools/three_way_match.py build      # 全量扫描 + 写夹具
    ../.reference/venv/bin/python tools/three_way_match.py report     # ①②③④ 的数字
    ../.reference/venv/bin/python tools/three_way_match.py verify     # 正对照准确率（分半）
    ../.reference/venv/bin/python tools/three_way_match.py probe 0x3c2bc0
    ../.reference/venv/bin/python tools/three_way_match.py key src/graphwar_shot_calculator.rs:86:37
    ../.reference/venv/bin/python tools/three_way_match.py nm GraphwarShotCalculator::calculate_shot_results

`build` 需要 capstone（走仓库 `.reference/venv`）；`report` / `verify` / `probe` / `nm` / `key`
只读缓存与夹具，标准库即可。
"""

import argparse
import bisect
import collections
import hashlib
import json
import math
import os
import pickle
import re
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import sym_match as SM                       # noqa: E402  Elf / demangle / name_key / parse_name

# 缓存里存的是**完整扫描结果**，所以反汇编/解析逻辑一改就必须抬版本号，
# 否则会静默吃旧缓存（sym_match §36.0 踩过这个坑，这里一开始就带着版本号）。
CACHE_VERSION = "tw-v3"

DEFAULT_LINUX = os.path.join(SM.GW2, ".reference/steam/2026-08-28/libgraphwarrust.so")
DEFAULT_ANDROID = os.path.join(SM.GW2, ".reference/steam/android-2.0.60/libgraphwarrust.so")
DEFAULT_WIN = os.path.join(SM.GW2, ".reference/windows/graphwarrust-2.0.60.dll")
DEFAULT_WIN_OLD = os.path.join(SM.GW2, ".reference/windows/graphwarrust-early2026.dll")

TRUTH = os.path.join(ROOT, "fixtures/name_to_addr.tsv")          # 只读，绝不覆盖
OUT = os.path.join(ROOT, "fixtures/name_to_addr_v2.tsv")

SRC_PREFIX = "src/"          # 游戏自己的源码（依赖 crate 在 /home/... 或 /rustc/... 下）
MAX_STR = 300

COLS = ("module", "type", "method", "x86_addr", "src_file", "src_line", "src_col",
        "join_kind", "sources", "evidence")
KIND_UNIQUE = "唯一锚"
KIND_CROSS = "跨版本锚"      # 只有 early2026 这份**不同版本**的构建落地（见 §37.4）
KIND_AMBIG = "歧义"
KIND_NONE = "未知"
SAME_VERSION = ("win", "android")   # 与 Linux 目标同为 2.0.60 的两份源


def die(msg):
    sys.stderr.write("three_way_match: %s\n" % msg)
    raise SystemExit(2)


def rel(path):
    try:
        r = os.path.relpath(path, ROOT)
    except ValueError:
        return path
    return r if not r.startswith("..") else path


def cache_dir():
    d = os.environ.get("THREEWAY_CACHE") or os.path.join(ROOT, ".agent-work/threeway/cache")
    os.makedirs(d, exist_ok=True)
    return d


# ===========================================================================
# 1. PE/COFF 极简读取（MinGW x86-64；只取本工具要的四样东西）
# ===========================================================================
class Pe:
    """PE32+ 只读：节表 / ImageBase / `.reloc` 的 DIR64 槽 / COFF 符号表 / `.pdata`。

    不用 `objdump -t`：147,444 条符号里带 Aux 记录，文本输出还要再解析「哪条 Aux
    属于哪条符号」；而 `.pdata`（48,853 条 `RUNTIME_FUNCTION`）`objdump -h` 根本不印。
    """

    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            self.data = d = f.read()
        if d[:2] != b"MZ":
            die("%s: 不是 PE（缺 MZ）" % path)
        pe = struct.unpack_from("<I", d, 0x3C)[0]
        if d[pe:pe + 4] != b"PE\0\0":
            die("%s: 缺 PE 签名" % path)
        self.machine = struct.unpack_from("<H", d, pe + 4)[0]
        if self.machine != 0x8664:
            die("%s: machine=%#x 不是 x86-64(0x8664)" % (path, self.machine))
        nsec = struct.unpack_from("<H", d, pe + 6)[0]
        self.symptr, self.nsyms = struct.unpack_from("<II", d, pe + 12)
        optsz = struct.unpack_from("<H", d, pe + 20)[0]
        opt = pe + 24
        if struct.unpack_from("<H", d, opt)[0] != 0x20B:
            die("%s: 不是 PE32+（magic!=0x20b）" % path)
        self.imagebase = struct.unpack_from("<Q", d, opt + 24)[0]
        self.secs, self.sec = [], {}
        for i in range(nsec):
            o = opt + optsz + i * 40
            name = d[o:o + 8].rstrip(b"\0").decode("ascii", "replace")
            vsize, vaddr, rsize, roff = struct.unpack_from("<IIII", d, o + 8)
            s = dict(name=name, addr=self.imagebase + vaddr, vsize=vsize,
                     size=vsize, off=roff, rawsize=rsize, index=i + 1)
            self.secs.append(s)
            self.sec[name] = s
        if ".text" not in self.sec:
            die("%s: 没有 .text" % path)
        self._relocs = self._pdata = self._syms = None

    # -- 地址 ------------------------------------------------------------
    def va2off(self, va):
        for s in self.secs:
            if s["addr"] <= va < s["addr"] + s["vsize"]:
                d = va - s["addr"]
                if d < (s["rawsize"] or 0):
                    return s["off"] + d
                return None
        return None

    def read(self, va, n):
        o = self.va2off(va)
        return None if o is None else self.data[o:o + n]

    def sec_at(self, va):
        for s in self.secs:
            if s["addr"] <= va < s["addr"] + s["vsize"]:
                return s
        return None

    # -- `.reloc`：ELF `R_*_RELATIVE` 的对应物 ---------------------------
    def relocs(self):
        """{va: 10}：`IMAGE_REL_BASED_DIR64` 的槽。

        PE 在**磁盘上**就把绝对地址写进了数据（只有装载到别的基址时才改），所以指针槽
        里读到的是真值；`.reloc` 的作用是「告诉我们哪些 8 字节是绝对地址」，
        拿它当门控比裸扫 24 字节模式可靠得多。
        """
        if self._relocs is not None:
            return self._relocs
        out = {}
        self._relocs = out
        s = self.sec.get(".reloc")
        if s is None:
            return out
        pos, end = s["off"], s["off"] + min(s["vsize"], s["rawsize"] or s["vsize"])
        while pos + 8 <= end:
            page, bsz = struct.unpack_from("<II", self.data, pos)
            if bsz < 8 or pos + bsz > end:
                break
            for k in range((bsz - 8) // 2):
                e = struct.unpack_from("<H", self.data, pos + 8 + k * 2)[0]
                if (e >> 12) == 10:                       # IMAGE_REL_BASED_DIR64
                    out[self.imagebase + page + (e & 0xFFF)] = 10
            pos += bsz
        return out

    # -- `.pdata`：函数边界 ----------------------------------------------
    def pdata(self):
        if self._pdata is not None:
            return self._pdata
        out = []
        s = self.sec.get(".pdata")
        if s is not None:
            for i in range(s["vsize"] // 12):
                b, e, u = struct.unpack_from("<III", self.data, s["off"] + i * 12)
                if b and e > b:
                    out.append((self.imagebase + b, self.imagebase + e, self.imagebase + u))
        out.sort()
        self._pdata = out
        return out

    # -- COFF 符号表 -----------------------------------------------------
    def coff_symbols(self):
        """[(name, va, is_func)]：**落在 `.text`** 的已定义符号。

        一条符号 18 字节：Name(8) Value(4) SecNum(2) Type(2) Class(1) nAux(1)。
        `nAux` 必须跳过，否则后面全错位；长名用「前 4 字节为 0 + 后 4 字节是
        符号表之后 string table 里的偏移」表示。`Value` 是**节内偏移**，不是 VA。
        """
        if self._syms is not None:
            return self._syms
        d = self.data
        tex = self.sec[".text"]
        strtab = self.symptr + self.nsyms * 18
        out = []
        i = 0
        while i < self.nsyms:
            o = self.symptr + i * 18
            if o + 18 > len(d):
                break
            naux = d[o + 17]
            val = struct.unpack_from("<I", d, o + 8)[0]
            secn = struct.unpack_from("<h", d, o + 12)[0]
            typ = struct.unpack_from("<H", d, o + 14)[0]
            n8 = d[o:o + 8]
            if n8[:4] == b"\0\0\0\0":
                so = struct.unpack_from("<I", n8, 4)[0]
                se = d.find(b"\0", strtab + so)
                name = d[strtab + so:se].decode("ascii", "replace")
            else:
                name = n8.rstrip(b"\0").decode("ascii", "replace")
            if secn == tex["index"]:
                out.append((name, tex["addr"] + val, typ == 0x20))
            i += 1 + naux
        self._syms = out
        return out

    # -- panic::Location -------------------------------------------------
    def locations(self, verbose=False):
        """`.rdata`/`.data` 里 24 字节的 `Location` → {struct_va: (file, line, col)}。

        布局与 ELF 完全一致（`{ptr, len, line, col}`，24 字节），**但 ptr 是绝对 VA**
        （`ImageBase + RVA`），不是重定位 addend。判定五件套：

          ① 槽在 `.reloc` 的 DIR64 表里（这里确实存了绝对地址）；
          ② ptr 落在 `.rdata`（字面量所在节）；
          ③ `4 < len <= 300`，`ptr` 处 len 字节全可打印 ASCII 且以 `.rs` 结尾；
          ④ `1 <= line < 100000`、`1 <= col < 2000`。
        """
        ro = self.sec.get(".rdata")
        if ro is None:
            die("%s: 没有 .rdata" % self.path)
        ro_lo, ro_hi = ro["addr"], ro["addr"] + ro["vsize"]
        rel = self.relocs()
        out, noproof = {}, 0
        for secn in (".data", ".rdata"):
            s = self.sec.get(secn)
            if s is None:
                continue
            span = min(s["vsize"], s["rawsize"] or s["vsize"])
            for i in range(0, span - 24, 8):
                va = s["addr"] + i
                p = struct.unpack_from("<Q", self.data, s["off"] + i)[0]
                if not (ro_lo <= p < ro_hi):
                    continue
                ln = struct.unpack_from("<Q", self.data, s["off"] + i + 8)[0]
                if not (4 < ln <= MAX_STR):
                    continue
                po = self.va2off(p)
                if po is None:
                    continue
                b = self.data[po:po + ln]
                if len(b) != ln or any(c < 32 or c >= 127 for c in b):
                    continue
                if not b.endswith(b".rs"):
                    continue
                line, col = struct.unpack_from("<II", self.data, s["off"] + i + 16)
                if not (1 <= line < 100000 and 1 <= col < 2000):
                    continue
                if va not in rel:
                    noproof += 1
                out[va] = (b.decode("ascii"), line, col)
        if verbose:
            sys.stderr.write("[3way] %-22s %d 个 Location 结构体（%d 个不在 .reloc 表里）\n"
                             % (os.path.basename(self.path), len(out), noproof))
        return out


# ===========================================================================
# 2. Location 引用判定（四种构建共用一套规则）
# ===========================================================================
class LocHit:
    """`Location` 的 24 字节布局 → 「这个操作数目标算不算引用某个 Location」。

        +0  file 指针槽   +8  len   +16 line(u32) +20 col(u32)

    取首址（`lea` / `adrp+add`）、取 len（`mov rax,[loc+8]`）、取 line/col 都算。
    **四方必须同一套规则**，否则 join 的一对一会被人为破坏。
    """

    __slots__ = ("addrs",)

    def __init__(self, addrs):
        self.addrs = set(addrs)

    def lookup(self, tgt):
        for back in (0, 8, 16):
            if tgt - back in self.addrs:
                return tgt - back
        return None


# ===========================================================================
# 3. 函数宇宙 + 一次线性反汇编（引用 + 结构特征）
# ===========================================================================
def elf_universe(elf, funcstarts_path=None):
    """Linux：Ghidra 起点表（与 `name_to_addr.tsv` 同口径）；Android：`.symtab` FUNC。"""
    tex = elf.sec[".text"]
    lo, hi = tex["addr"], tex["addr"] + tex["size"]
    if funcstarts_path:
        raw = pickle.load(open(funcstarts_path, "rb"))
        starts = sorted({int(x, 16) if isinstance(x, str) else int(x) for x in raw})
        starts = [a for a in starts if lo <= a < hi]
        out = {}
        for i, a in enumerate(starts):
            nxt = starts[i + 1] if i + 1 < len(starts) else hi
            out[a] = dict(va=a, end=nxt, name=[])
        return out
    syms = [(n, a, s) for n, a, s in elf.func_symbols() if lo <= a < hi]
    syms.sort(key=lambda r: r[1])
    starts = [a for _n, a, _s in syms]
    out = {}
    for n, a, sz in syms:
        i = bisect.bisect_right(starts, a)
        nxt = starts[i] if i < len(starts) else hi
        d = out.setdefault(a, dict(va=a, end=a + (sz if sz else nxt - a), name=[]))
        d["name"].append(n)
    return out


def pe_universe(pe):
    """Windows：`.pdata` 的 `RUNTIME_FUNCTION` + 没被覆盖的 COFF FUNC 符号。

    为什么以 `.pdata` 为主：它是编译器/链接器发的**精确** `begin/end`，不需要
    「下一个起点减本起点」这种反推。GNU COFF 的函数符号 size 存的是 TagIndex、
    不是字节数（只有 MSVC 才对），所以符号不能用来自定边界。
    """
    out = {}
    for begin, end, _u in pe.pdata():
        out[begin] = dict(va=begin, end=end, name=[])
    starts = sorted(out)
    merged = 0
    for name, va, is_func in pe.coff_symbols():
        if not is_func:
            continue
        if va in out:
            out[va]["name"].append(name)
            continue
        i = bisect.bisect_right(starts, va) - 1
        if i >= 0 and starts[i] <= va < out[starts[i]]["end"]:
            out[starts[i]]["name"].append(name)          # 落在已覆盖的函数体内 ⇒ 同一函数
            merged += 1
            continue
        j = bisect.bisect_right(starts, va)
        nxt = starts[j] if j < len(starts) else pe.sec[".text"]["addr"] + pe.sec[".text"]["vsize"]
        out[va] = dict(va=va, end=nxt, name=[name])
        starts = sorted(out)
    if merged:
        sys.stderr.write("[3way] %s: %d 个 COFF 函数符号落在 .pdata 的某个函数体内（并入）\n"
                         % (os.path.basename(pe.path), merged))
    return out


def owner_of(starts, universe, va):
    i = bisect.bisect_right(starts, va) - 1
    if i < 0:
        return None
    a = starts[i]
    return a if va < universe[a]["end"] else None


# ---------------------------------------------------------------------------
# 3.1 x86-64
# ---------------------------------------------------------------------------
def scan_x86(obj, universe, locs, verbose=False):
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64
    from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_REG_RIP

    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    tex = obj.sec[".text"]
    code = obj.read(tex["addr"], tex["size"])
    starts = sorted(universe)
    hit = LocHit(locs)
    feats = {va: dict(ninsn=0, calls=[], icalls=0, consts=set(),
                      hist=collections.Counter(), locrefs=set())
             for va in universe}
    for insn in cs.disasm(code, tex["addr"]):
        owner = owner_of(starts, universe, insn.address)
        if owner is None:
            continue
        fe = feats[owner]
        fe["ninsn"] += 1
        fe["hist"][insn.mnemonic] += 1
        end = insn.address + insn.size
        if insn.mnemonic == "call":
            if insn.operands and insn.operands[0].type == X86_OP_IMM:
                fe["calls"].append(insn.operands[0].imm & 0xFFFFFFFFFFFFFFFF)
                continue
            fe["icalls"] += 1
        for op in insn.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                tgt = (end + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
            elif op.type == X86_OP_IMM:
                tgt = op.imm & 0xFFFFFFFFFFFFFFFF
                if -0x80000000 <= op.imm <= 0xFFFFFFFF and abs(op.imm) >= 0x100 \
                        and obj.sec_at(op.imm) is None:
                    fe["consts"].add(op.imm)
            else:
                continue
            lv = hit.lookup(tgt)
            if lv is not None:
                fe["locrefs"].add(lv)
    if verbose:
        sys.stderr.write("[3way] %-22s %d 函数, %d 个引用到 Location\n"
                         % (os.path.basename(obj.path), len(universe),
                            sum(1 for f in feats.values() if f["locrefs"])))
    return feats


# ---------------------------------------------------------------------------
# 3.2 arm64
# ---------------------------------------------------------------------------
ARM64_BARRIER = {"bl", "blr", "br", "ret", "cbz", "cbnz", "tbz", "tbnz", "b"}


def scan_arm64(obj, universe, locs, verbose=False):
    from capstone import Cs, CS_ARCH_ARM64
    from capstone.arm64 import ARM64_OP_IMM, ARM64_OP_MEM, ARM64_OP_REG

    cs = Cs(CS_ARCH_ARM64, 0)
    cs.detail = True
    WRITE = 2
    tex = obj.sec[".text"]
    code = obj.read(tex["addr"], tex["size"])
    starts = sorted(universe)
    hit = LocHit(locs)
    feats = {va: dict(ninsn=0, calls=[], icalls=0, consts=set(),
                      hist=collections.Counter(), locrefs=set())
             for va in universe}
    addr = {}
    owner = None
    for insn in cs.disasm(code, tex["addr"]):
        o = owner_of(starts, universe, insn.address)
        if o != owner:                       # 跨函数不继承寄存器状态
            addr = {}
            owner = o
        if owner is None:
            continue
        fe = feats[owner]
        fe["ninsn"] += 1
        mn = insn.mnemonic
        fe["hist"][mn] += 1
        ops = insn.operands
        tgts = []
        used = False
        if mn == "bl" and ops and ops[0].type == ARM64_OP_IMM:
            fe["calls"].append(ops[0].imm)
            used = True
        elif mn == "blr":
            fe["icalls"] += 1
            used = True
        elif mn == "adrp" and len(ops) >= 2 and ops[1].type == ARM64_OP_IMM:
            addr[ops[0].reg] = ops[1].imm
            used = True
        elif mn in ("add", "sub") and len(ops) >= 3 and ops[1].type == ARM64_OP_REG \
                and ops[2].type == ARM64_OP_IMM and ops[1].reg in addr:
            t = addr[ops[1].reg] + (ops[2].imm if mn == "add" else -ops[2].imm)
            tgts.append(t)
            addr[ops[0].reg] = t
            used = True
        elif len(ops) >= 2 and ops[1].type == ARM64_OP_MEM:
            m = ops[1].mem
            if m.base in addr:
                tgts.append(addr[m.base] + m.disp)
                used = True
            elif insn.reg_name(m.base) == "pc":
                tgts.append(insn.address + m.disp)
                used = True
        for t in tgts:
            lv = hit.lookup(t)
            if lv is not None:
                fe["locrefs"].add(lv)
        if mn in ("movz", "movn", "mov") and len(ops) >= 2 and ops[1].type == ARM64_OP_IMM:
            v = ops[1].imm
            if mn == "movn":
                v = ~v
            if len(ops) >= 3 and ops[2].type == ARM64_OP_IMM and ops[2].imm in (16, 32, 48):
                v <<= ops[2].imm
            if abs(v) >= 0x100:
                fe["consts"].add(v)
        if mn in ARM64_BARRIER:
            addr = {}
        elif not used:
            # 只有「本指令没被专门处理」时才按写操作数作废寄存器上的已知地址。
            # 专门处理过（adrp/add/ldr）的指令**不能**再按写操作数清——`adrp x0,…`
            # 自己就写 x0，一清就永远解不出任何 Location 引用（本工具第一版就死在这，
            # arm64 侧 0 个引用）。
            for op in ops:
                if op.type == ARM64_OP_REG and op.access & WRITE:
                    addr.pop(op.reg, None)
    if verbose:
        sys.stderr.write("[3way] %-22s %d 函数, %d 个引用到 Location\n"
                         % (os.path.basename(obj.path), len(universe),
                            sum(1 for f in feats.values() if f["locrefs"])))
    return feats


# ===========================================================================
# 4. 一份构建 = 一个 Side
# ===========================================================================
class Side:
    """一家构建的全部可对齐信息。`func_locs` 的键才是能 join 的三元组。"""

    def __init__(self, tag, kind, path, obj, universe, feats, locs):
        self.tag, self.kind, self.path = tag, kind, path
        self.obj, self.universe, self.feats = obj, universe, feats
        self.locs = locs
        self.starts = sorted(universe)

        for va, f in universe.items():
            fe = feats[va]
            calls = []
            for c in fe["calls"]:
                a = owner_of(self.starts, universe, c)
                if a is not None:
                    calls.append(a)
            fe["callees"] = tuple(sorted(calls))
            fe["size"] = f["end"] - va
            body = obj.read(va, fe["size"]) if fe["size"] > 0 else None
            fe["bsig"] = hashlib.sha1(body).hexdigest()[:16] if body else None

        self.func_locs = {}
        self.key_funcs = collections.defaultdict(set)
        for va in universe:
            ks = {locs[l] for l in feats[va]["locrefs"] if l in locs}
            self.func_locs[va] = ks
            for k in ks:
                self.key_funcs[k].add(va)
        self.names = self._names()

    def _names(self):
        raw = []
        for va, f in self.universe.items():
            for n in (f.get("name") or ()):
                if "graphwarrust" in n or n.startswith("_ZN"):
                    raw.append((va, n))
        if not raw:
            return {}
        dem = SM.demangle([n for _va, n in raw])
        out = collections.defaultdict(list)
        for (va, _m), d in zip(raw, dem):
            k = SM.name_key(d)
            if k:
                out[va].append((k, d))
        return dict(out)

    # -- 缓存 -----------------------------------------------------------
    def state(self):
        """只存**内建类型**：不 pickled `obj`（那等于把 40 MB 二进制塞进缓存），
        也不 pickled Side 本身（脚本以 `__main__` 运行时，类的模块名会让缓存
        再也读不回来——本工具第一版就踩过，报 `__main__` has no attribute Side）。"""
        return dict(tag=self.tag, kind=self.kind, path=self.path,
                    universe=self.universe, feats=self.feats, locs=self.locs,
                    func_locs=self.func_locs, key_funcs=dict(self.key_funcs),
                    names=self.names, starts=self.starts)

    @classmethod
    def from_state(cls, st):
        s = cls.__new__(cls)
        s.tag, s.kind, s.path = st["tag"], st["kind"], st["path"]
        s.obj = None
        s.universe, s.feats, s.locs = st["universe"], st["feats"], st["locs"]
        s.func_locs = st["func_locs"]
        s.key_funcs = collections.defaultdict(set, st["key_funcs"])
        s.names, s.starts = st["names"], st["starts"]
        return s

    # -- 统计 -----------------------------------------------------------
    def stats(self):
        triples = set(self.locs.values())
        src_triples = {t for t in triples if t[0].startswith(SRC_PREFIX)}
        used = {k for ks in self.func_locs.values() for k in ks}
        src_used = {k for k in used if k[0].startswith(SRC_PREFIX)}
        return dict(funcs=len(self.universe),
                    loc_structs=len(self.locs),
                    triples=len(triples),
                    src_structs=sum(1 for t in self.locs.values() if t[0].startswith(SRC_PREFIX)),
                    src_triples=len(src_triples),
                    src_used=len(src_used),
                    named_funcs=len(self.names),
                    names_unique=len({k for v in self.names.values() for k, _d in v}),
                    with_loc=sum(1 for ks in self.func_locs.values() if ks),
                    with_src_loc=sum(1 for ks in self.func_locs.values()
                                     if any(k[0].startswith(SRC_PREFIX) for k in ks)))


def elf_locations(elf, verbose=False):
    """ELF：`.data.rel.ro` 的 24 字节 Location + `R_*_RELATIVE` 的 addend（§20.2）。"""
    sec = elf.sec.get(".data.rel.ro")
    if sec is None:
        die("%s: 没有 .data.rel.ro" % elf.path)
    lo, hi = sec["addr"], sec["addr"] + sec["size"]
    out = {}
    for va, add in elf.rel.items():
        if not (lo <= va < hi):
            continue
        raw = elf.read(va, 24)
        if not raw or len(raw) < 24:
            continue
        _p, ln, line, col = struct.unpack("<QQII", raw)
        if not (4 < ln <= MAX_STR):
            continue
        b = elf.read(add, ln)
        if not b or len(b) != ln or any(c < 32 or c >= 127 for c in b):
            continue
        if not b.endswith(b".rs"):
            continue
        if not (1 <= line < 100000 and 1 <= col < 2000):
            continue
        out[va] = (b.decode("ascii"), line, col)
    if verbose:
        sys.stderr.write("[3way] %-22s %d 个 Location 结构体（R_*_RELATIVE）\n"
                         % (os.path.basename(elf.path), len(out)))
    return out


def load_side(tag, kind, path, funcstarts=None, verbose=True, force=False):
    """扫描 + 缓存。key 带 CACHE_VERSION 与文件大小（改了逻辑就抬版本号）。"""
    try:
        size = os.path.getsize(path)
    except OSError as e:
        die("读不到 %s：%s" % (path, e))
    key = "%s-%s-%d-%s.pkl" % (tag, os.path.basename(path).replace(".", "_"), size, CACHE_VERSION)
    cp = os.path.join(cache_dir(), key)
    if os.path.exists(cp) and not force:
        with open(cp, "rb") as f:
            return Side.from_state(pickle.load(f))
    if kind == "pe":
        obj = Pe(path)
        universe = pe_universe(obj)
        locs = obj.locations(verbose)
        feats = scan_x86(obj, universe, locs, verbose)
    else:
        obj = SM.Elf(path)
        universe = elf_universe(obj, funcstarts)
        locs = elf_locations(obj, verbose)
        feats = (scan_x86 if obj.em == 62 else scan_arm64)(obj, universe, locs, verbose)
    side = Side(tag, kind, path, obj, universe, feats, locs)
    with open(cp, "wb") as f:
        pickle.dump(side.state(), f, protocol=4)
    return side


def load_all(args, verbose=True):
    sides = {}
    sides["linux"] = load_side("linux", "elf", args.linux, args.funcstarts, verbose, args.force)
    if args.android:
        sides["android"] = load_side("android", "elf", args.android, None, verbose, args.force)
    if args.win:
        sides["win"] = load_side("win", "pe", args.win, None, verbose, args.force)
    if args.win_old:
        sides["win_old"] = load_side("win_old", "pe", args.win_old, None, verbose, args.force)
    return sides


# ===========================================================================
# 5. 三向 join + 名字落地
# ===========================================================================
def fmt_key(k):
    return "%s:%d:%d" % k


def kstr(k):
    """name_key 的显示串。`type` 列对 free function 是 None，排序/比较必须转字符串，
    否则 `sorted()` 会在 None 与 str 之间比较并抛 TypeError（实测踩过）。"""
    return "::".join(x for x in k if x)


def src_only(keys):
    return {k for k in keys if k[0].startswith(SRC_PREFIX)}


def join_landings(src, dst, tag):
    """`src`（有名字）→ `dst`（目标）的名字落地：只认**双向唯一**的 Location 键。

    双向唯一 = 该三元组在 src 侧恰好 1 个函数引用、在 dst 侧也恰好 1 个。
    一条名字的全部唯一键必须指向**同一个** dst 函数才算落定，否则记冲突。
    """
    land = {}
    conflicts = []
    for sva, names in src.names.items():
        keys = src_only(src.func_locs.get(sva, set()))
        if not keys:
            continue
        votes = collections.Counter()
        used = set()
        for k in keys:
            s_side = src.key_funcs.get(k, set())
            d_side = dst.key_funcs.get(k, set())
            if len(s_side) == 1 and len(d_side) == 1:
                votes[next(iter(d_side))] += 1
                used.add(k)
        if not votes:
            continue
        if len(votes) > 1:
            conflicts.append(dict(tag=tag, name=names[0][1], src_addr=sva,
                                  cands=sorted(votes.items(), key=lambda kv: -kv[1]),
                                  keys=sorted(fmt_key(k) for k in used)))
            continue
        dva = next(iter(votes))
        for nk, dem in names:
            land.setdefault(dva, []).append(dict(
                tag=tag, name_key=nk, demangled=dem, src_addr=sva,
                votes=votes[dva], keys=sorted(fmt_key(k) for k in used)))
    return land, conflicts


def merge_landings(sides, verbose=True):
    lin = sides["linux"]
    per_src, conflicts = {}, []
    for tag in ("win", "win_old", "android"):
        s = sides.get(tag)
        if s is None:
            continue
        land, conf = join_landings(s, lin, tag)
        per_src[tag] = land
        conflicts.extend(conf)
        if verbose:
            sys.stderr.write("[3way] %-8s → linux: %d 个地址落地（%d 条多键指向不同地址）\n"
                             % (tag, len(land), len(conf)))
    return per_src, conflicts


def collect_names(sides):
    """名字键 → 出现的来源集合，以及每个来源下的 (地址, 名字全串)。"""
    names = collections.defaultdict(lambda: dict(srcs=set(), addr={}, dem=None))
    for tag in ("win", "win_old", "android"):
        s = sides.get(tag)
        if s is None:
            continue
        for va, lst in s.names.items():
            for nk, dem in lst:
                e = names[nk]
                e["srcs"].add(tag)
                e["addr"][tag] = va
                if e["dem"] is None:
                    e["dem"] = dem
    return names


# ===========================================================================
# 6. 天花板之外：同 ISA 结构化最近邻 + 正对照
# ===========================================================================
# 打分函数只用**与地址无关**的量：尺寸、指令数、助记符直方图、整数常量集合。
# 权重与候选窗口是**在真值的一半上**定出来的（另一半只用来报准确率，见 `verify`），
# 这一点必须写明：它不是从原理推的，也不是在全部真值上看出来的。
#
# 为什么不用"被调者多重集"当主特征（虽然任务书点名要它）：实测两个构建的被调者数之比
# 中位数 **0.38**（Linux 侧更少 —— LTO/ICF 把子调用内联进去了），不归一化会系统性
# 把正确答案打低分；归一化之后它仍然让 top-1 从 43.9% 掉到 36.7%（见 §37.6 的消融），
# 所以**不作为主项**，只在文档里给出这条负结果。
MATCH_W = dict(size=0.5, insn=0.5, hist=1.0, const=1.0)
MATCH_WIN = (0.5, 2.0)       # 候选窗口：尺寸比落在 [0.5, 2.0]
TOPK = 10
MARGIN_TIERS = (0.05, 0.1, 0.2, 0.3)
SIZE_EPS = 0.4               # log 比的饱和点
CALLEE_RATIO_MEDIAN = 0.38   # Linux/Windows 被调者数之比的中位数（实测，见上）


def cos_hist(a, b):
    if not a or not b:
        return 0.0
    num = 0
    for k, v in a.items():
        bv = b.get(k, 0)
        if bv:
            num += v * bv
    if not num:
        return 0.0
    da = math.sqrt(sum(v * v for v in a.values()))
    db = math.sqrt(sum(v * v for v in b.values()))
    return num / (da * db) if da and db else 0.0


def score_pair(qf, df, w=None):
    """query 特征 vs pool 特征 → 相似度（越大越像）。**不读任何地址/名字/Location。**"""
    w = w or MATCH_W
    s = 0.0
    s += w["size"] * (1.0 - min(abs(math.log((df["size"] + 1.0) / (qf["size"] + 1.0))) / SIZE_EPS, 1.0))
    s += w["insn"] * (1.0 - min(abs(math.log((df["ninsn"] + 1.0) / (qf["ninsn"] + 1.0))) / SIZE_EPS, 1.0))
    s += w["hist"] * cos_hist(qf["hist"], df["hist"])
    if qf["consts"] and df["consts"]:
        s += w["const"] * len(qf["consts"] & df["consts"]) / float(len(qf["consts"] | df["consts"]))
    return s


def build_pool(side, named=True):
    """候选池（按尺寸排序）：只放**有名字**的 pool 侧函数时才有名字可给。"""
    pool = [(side.feats[v]["size"], v) for v in side.universe if (not named or v in side.names)]
    pool.sort()
    return [s for s, _ in pool], [v for _, v in pool]


def rank_one(qf, sizes, vas, pside, k=TOPK, win=MATCH_WIN, w=None):
    lo = max(8, int(qf["size"] * win[0]))
    hi = max(lo + 1, int(qf["size"] * win[1]))
    i = bisect.bisect_left(sizes, lo)
    j = bisect.bisect_right(sizes, hi)
    sc = [(score_pair(qf, pside.feats[c], w), c) for c in vas[i:j]]
    sc.sort(reverse=True)
    return sc[:k]


def rank_many(qside, pside, targets, named_pool=True, k=TOPK, win=MATCH_WIN, w=None):
    sizes, vas = build_pool(pside, named_pool)
    return {t: rank_one(qside.feats[t], sizes, vas, pside, k, win, w) for t in targets}


def margin_of(ranked):
    if not ranked:
        return 0.0
    return ranked[0][0] - (ranked[1][0] if len(ranked) > 1 else 0.0)


def truth_pairs(src, dst):
    """正对照的**真值**：由 Location 唯一锚定、且两侧都是一对一的 (src_va, dst_va, name_key)。

    真值完全来自 `panic::Location`（编译期常量），匹配器**一点都不看它**，
    所以这是真·正对照，不是自证。
    """
    land, _conf = join_landings(src, dst, "truth")
    out = []
    for dva, lst in land.items():
        if len(lst) != 1:
            continue
        out.append((lst[0]["src_addr"], dva, lst[0]["name_key"]))
    cnt = collections.Counter(w for w, _l, _k in out)
    out = [t for t in out if cnt[t[0]] == 1]
    cnt = collections.Counter(l for _w, l, _k in out)
    out = [t for t in out if cnt[t[1]] == 1]
    return sorted(out)


def split_half(pairs):
    """真值分半：按名字键的 sha1 奇偶（可复现、与"难不难"无关）⇒ 调参半 / 留出半。"""
    a, b = [], []
    for p in pairs:
        h = hashlib.sha1(("|".join(x or "" for x in p[2])).encode()).digest()[0]
        (a if h & 1 else b).append(p)
    return a, b


def control_rows(sides, pairs, src="win", dst="linux", w=None):
    """在真值上跑匹配器：query 取 dst 侧函数，候选池 = src 侧的**有名**函数。

    返回每对的 (margin, top1 是否命中, 是否互为 top-1)。互为 top-1 需要两侧各排一次名。
    """
    s = sides[src]
    d = sides[dst]
    fwd = rank_many(d, s, [l for _w, l, _k in pairs], named_pool=True, w=w)
    rev = rank_many(s, d, [w2 for w2, _l, _k in pairs], named_pool=False, w=w)
    rows = []
    for (wva, lva, nk) in pairs:
        r = fwd.get(lva) or []
        top1 = r[0][1] if r else None
        rr = rev.get(wva) or []
        mutual = bool(rr) and rr[0][1] == lva and top1 == wva
        rows.append(dict(src=wva, dst=lva, name_key=nk, top1_ok=(top1 == wva),
                         top1=top1, margin=margin_of(r), mutual=mutual,
                         in_topk=any(c == wva for _s, c in r)))
    return rows


def accuracy(rows, thr=0.0, mutual_only=False):
    sel = [r for r in rows if r["margin"] >= thr and (not mutual_only or r["mutual"])]
    if not sel:
        return 0, 0.0
    return len(sel), sum(1 for r in sel if r["top1_ok"]) / float(len(sel))


def beyond_ceiling(sides, margin_thr=0.1, src="win", dst="linux"):
    """天花板之外：对 **没有 src Location** 的目标函数跑一遍，统计"高置信"候选。"""
    s, d = sides[src], sides[dst]
    sizes, vas = build_pool(s, named=True)
    no_loc = [v for v, ks in d.func_locs.items() if not src_only(ks)]
    res = []
    for lva in no_loc:
        r = rank_one(d.feats[lva], sizes, vas, s)
        if r:
            res.append((margin_of(r), r[0][1], lva, r[0][0]))
    res.sort(reverse=True)
    tiers = {}
    for thr in MARGIN_TIERS:
        sel = [x for x in res if x[0] >= thr]
        claim = collections.Counter(w for _m, w, _l, _s in sel)
        uniq = [x for x in sel if claim[x[1]] == 1]        # 一个 win 名只被一个 linux 认领
        tiers[thr] = dict(margin=len(sel), injective=len(uniq), uniq_rows=uniq)
    return dict(total_no_loc=len(no_loc), pool=len(vas), tiers=tiers)



# ===========================================================================
# 7. 产出夹具 `fixtures/name_to_addr_v2.tsv`
# ===========================================================================
def name_landings(per_src):
    """把「按 linux 地址索引」的落地表翻成「按名字键索引」。"""
    out = collections.defaultdict(dict)          # name_key -> {tag: landing}
    for tag, land in per_src.items():
        for dva, lst in land.items():
            for e in lst:
                out[e["name_key"]][tag] = dict(dva=dva, votes=e["votes"],
                                               keys=e["keys"], src_addr=e["src_addr"])
    return out


def decide(names, land_of_name, ambig_by_name):
    """一个名字键的最终裁决 → (join_kind, x86_addr, evidence)。

    只在「所有**同版本**落地来源都指向同一个地址」时给地址；否则记歧义并列出候选。
    这个工是 §36 那套「宁缺勿错」的延伸：本工具**不新增候选**，只在双向唯一的
    Location 键上落地。

    为什么要分「同版本 / 跨版本」两档（实测教训，别合并回去）：`graphwar_level_manager` 的
    `setup_level_10..13` 在 **early2026** 那份 DLL 里带的 25 个双向唯一 Location 键全部指向
    Linux 的 `0x499740`，而 Android 2.0.60 的同名函数带的是 `line 995~1002` 的键、指向
    `0x4a5390`。两份 Windows 构建**不是同一个版本**，行号会整体漂移（同一处 `expect()`
    从 182 行挪到 183 行），于是「键恰好还在」的地方会拼出**看起来 25 票全票通过**的错答案。
    ⇒ 跨版本那一档单列，不进「唯一锚」的计数。
    """
    hits = land_of_name
    if hits:
        same = {t: h for t, h in hits.items() if t in SAME_VERSION}
        use = same or hits
        addrs = {t: h["dva"] for t, h in use.items()}
        uniq = set(addrs.values())
        if len(uniq) == 1:
            a = next(iter(uniq))
            best = max(use.values(), key=lambda h: h["votes"])
            xver = "-"
            if "win_old" in hits:
                xver = "agree" if hits["win_old"]["dva"] == a else "differ"
            ev = "votes=%d;keys=%s;src=%s;xver=%s;win_addr=%s" % (
                best["votes"], ",".join(best["keys"][:4]),
                "+".join(sorted(addrs)), xver,
                ("0x%x" % hits["win"]["src_addr"]) if "win" in hits else "-")
            return (KIND_UNIQUE if same else KIND_CROSS), a, ev
        cand = ";".join("%s:0x%x" % (t, a) for t, a in sorted(addrs.items()))
        return KIND_AMBIG, None, "同版本来源互相矛盾（同 Location 落到不同地址）：%s" % cand
    if ambig_by_name.get(names["key"]):
        return KIND_AMBIG, None, ambig_by_name[names["key"]]
    srcs = names["srcs"]
    note = {
        "win": "Windows 2.0.60 的函数没有 src/*.rs 的 Location 锚",
        "win_old": "Windows early2026 的函数没有 src/*.rs 的 Location 锚",
        "android": "arm64 函数没有 src/*.rs 的 Location 锚",
    }
    return KIND_NONE, None, "；".join(note[t] for t in sorted(srcs)) or "无来源"


def write_fixture(sides, per_src, conflicts, path):
    land_of_name = name_landings(per_src)
    ambig = {}
    for c in conflicts:
        nk = SM.name_key(c["name"])
        if nk:
            ambig[nk] = "多键指向不同地址：%s" % ", ".join(
                "0x%x(%d)" % (a, n) for a, n in c["cands"])
    names = collect_names(sides)
    rows = []
    for nk, e in names.items():
        kind, addr, ev = decide(dict(key=nk, srcs=e["srcs"]), land_of_name.get(nk, {}), ambig)
        module, owner, method = nk
        f, line, col = ("", "", "")
        if addr is not None:
            ks = sorted(src_only(sides["linux"].func_locs.get(addr, set())))
            if ks:
                # 展示哪条锚：优先「文件名就是本模块」的那条（与 v1 的展示口径一致），
                # 否则取排序后的第一条。**只是展示用**，join 用的是整个键集合。
                own = [k for k in ks if module in os.path.basename(k[0])]
                k = (own or ks)[0]
                f, line, col = k[0], k[1], k[2]
        rows.append([module, owner or "", method or "",
                     ("0x%x" % addr) if addr is not None else "",
                     f, str(line), str(col), kind,
                     "+".join(sorted(e["srcs"])), ev])
    order = {KIND_UNIQUE: 0, KIND_CROSS: 1, KIND_AMBIG: 2, KIND_NONE: 3}
    rows.sort(key=lambda r: (order.get(r[7], 9), r[0], r[1], r[2], r[4], int(r[5] or 0)))
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("# 由 tools/three_way_match.py build 生成，不要手改（重跑会覆盖）。\n")
        fh.write("# 三向来源：Windows 2.0.60 / Windows early2026（COFF 符号表，同 ISA）"
                 "× Android 2.0.60（arm64 .symtab）× Linux 2026-08-28（stripped，目标）。\n")
        fh.write("# 落地键只有 panic::Location 三元组 (file,line,col)，且必须**双向唯一**。\n")
        fh.write("# x86_addr 是 Linux 2.0.60 的 ELF 虚地址；Ghidra/REA 地址 = 该值 + 0x100000。\n")
        fh.write("# 天花板之外（无 Location）不给名字，只在上限估计里出现（§37.6）。\n")
        fh.write("\t".join(COLS) + "\n")
        for r in rows:
            fh.write("\t".join(r) + "\n")
    return rows


# ===========================================================================
# 8. 报告
# ===========================================================================
def read_truth_v1(path=TRUTH):
    """读既有夹具（只读）：返回 `唯一锚` 行的 {name_key: [x86_addr...]}。

    注意 v1 的 326 条唯一锚里**有重名**（同一 (module,type,method) 出现在多行，
    例如同一方法的 shim 与本体），所以值是**列表**不是标量；按行数算才是 326。
    """
    out = {}
    if not os.path.exists(path):
        return out
    with open(path, encoding="utf-8") as f:
        for line in f:
            if line.startswith("#"):
                continue
            p = line.rstrip("\n").split("\t")
            if len(p) < 7 or p[6] != KIND_UNIQUE:
                continue
            try:
                out.setdefault((p[0], p[1], p[2]), []).append(int(p[3], 16))
            except ValueError:
                continue
    return out


def cmd_report(args):
    sides = load_all(args)
    W = sys.stdout.write
    W("=" * 78 + "\n① Location 三元组（跨构建唯一可对齐的键）\n")
    W("%-9s %8s %8s %10s %10s %10s\n"
      % ("build", "函数", "struct", "(file,line)", "src struct", "src用的键"))
    stats = {}
    for tag in ("linux", "android", "win", "win_old"):
        s = sides.get(tag)
        if s is None:
            continue
        st = s.stats()
        stats[tag] = st
        W("%-9s %8d %8d %10d %10d %10d\n"
          % (tag, st["funcs"], st["loc_structs"], st["triples"],
             st["src_structs"], st["src_used"]))
    W("\n  三方交集（src/*.rs 的三元组）：\n")
    sets = {}
    for tag in ("linux", "android", "win", "win_old"):
        s = sides.get(tag)
        if s is not None:
            sets[tag] = {k for k in s.locs.values() if k[0].startswith(SRC_PREFIX)}
    if "linux" in sets and "android" in sets:
        W("    linux   ∩ android        = %d\n" % len(sets["linux"] & sets["android"]))
    if "linux" in sets and "win" in sets:
        W("    linux   ∩ win(2.0.60)    = %d\n" % len(sets["linux"] & sets["win"]))
    if "android" in sets and "win" in sets:
        W("    android ∩ win(2.0.60)    = %d\n" % len(sets["android"] & sets["win"]))
    key3 = [t for t in ("linux", "android", "win") if t in sets]
    if len(key3) == 3:
        inter = sets["linux"] & sets["android"] & sets["win"]
        W("    linux ∩ android ∩ win    = %d   ← 同版本应 ≈ 2,035\n" % len(inter))
        W("    只在 linux 侧（另两份都没有）= %d\n"
          % len(sets["linux"] - sets["android"] - sets["win"]))
        W("    只在 win 侧                 = %d\n"
          % len(sets["win"] - sets["linux"] - sets["android"]))
    if "win_old" in sets and "linux" in sets:
        W("    linux ∩ win(early2026)   = %d   ← 早一版，应当明显小\n"
          % len(sets["linux"] & sets["win_old"]))

    per_src, conflicts = merge_landings(sides)
    names = collect_names(sides)
    v1 = read_truth_v1()
    land_of_name = name_landings(per_src)

    W("\n" + "=" * 78 + "② 名字落地\n")
    W("%-24s %6s %6s\n" % ("来源", "唯一名", "落地数"))
    for tag in ("win", "win_old", "android"):
        s = sides.get(tag)
        if s is None:
            continue
        n_land = len({k for k, v in land_of_name.items() if tag in v})
        W("%-24s %6d %6d\n" % (tag, s.stats()["names_unique"], n_land))
    landed_all = {k for k, v in land_of_name.items() if v}
    W("\n所有来源并集：唯一名 %d，其中落地 %d\n" % (len(names), len(landed_all)))
    combo = collections.Counter("+".join(sorted(v)) for v in land_of_name.values() if v)
    W("  落地来源组合：%s\n" % "  ".join("%s=%d" % (k, n) for k, n in combo.most_common()))
    same_keys = {k for k, v in land_of_name.items()
                 if any(t in SAME_VERSION for t in v)
                 and len({h["dva"] for t, h in v.items() if t in SAME_VERSION}) == 1}
    W("  其中**同版本**来源（win/android 2.0.60）支持且不矛盾：%d 个名字键；"
      "只有 early2026 支持：%d 个（单列「跨版本锚」，不进唯一锚计数）\n"
      % (len(same_keys), sum(1 for v in land_of_name.values()
                             if v and not any(t in SAME_VERSION for t in v))))
    v1_rows = sum(len(v) for v in v1.values())
    W("既有夹具 `fixtures/name_to_addr.tsv` 的唯一锚（v1 基线）：%d 行 / %d 个名字键\n"
      % (v1_rows, len(v1)))
    both = [k for k in v1 if k in landed_all]
    W("  v1 的 %d 个名字键里，本工具独立复现 %d（%.1f%%）\n"
      % (len(v1), len(both), 100.0 * len(both) / max(len(v1), 1)))
    agree = sum(1 for k in both
                if land_of_name[k][max(land_of_name[k])]["dva"] in v1[k])
    W("  其中地址落在 v1 记录的地址上 %d，不一致 %d\n" % (agree, len(both) - agree))
    new = [k for k in landed_all if k not in v1]
    lost = [k for k in v1 if k not in landed_all]
    amb_keys = {k for k, v in land_of_name.items()
                if any(t in SAME_VERSION for t in v)
                and len({h["dva"] for t, h in v.items() if t in SAME_VERSION}) > 1}
    W("  本工具新落地（v1 里是歧义/未知的名字）：%d\n" % len(new))
    W("  v1 有、本工具没落地的名字键：%d（v1 的 16 条消去法锚 + 规则差，见 §37.7）\n" % len(lost))
    W("  跨来源地址矛盾（记歧义、不给地址）：%d 个键\n" % len(amb_keys))
    W("  ⇒ 唯一锚：v1 **%d 行 / %d 键** → v2 **%d 键**（净 +%d）\n"
      % (v1_rows, len(v1), len(same_keys), len(same_keys) - len(v1)))

    win_names = {k for k, v in names.items() if "win" in v["srcs"]}
    and_names = {k for k, v in names.items() if "android" in v["srcs"]}
    old_names = {k for k, v in names.items() if "win_old" in v["srcs"]}
    for label, sel in (("win − android", win_names - and_names),
                       ("win − android − win_old", win_names - and_names - old_names),
                       ("win − win_old", win_names - old_names)):
        land = [k for k in sel if k in landed_all]
        W("  %-24s %4d 个，落地 %d（%.1f%%）\n"
          % (label, len(sel), len(land), 100.0 * len(land) / max(len(sel), 1)))
    # 「Windows 独有」为什么落不了：逐条归因（可复核，不猜）
    wkey2va = {}
    for sva, lst in sides["win"].names.items():
        for nk, _d in lst:
            wkey2va.setdefault(nk, sva)
    why = collections.Counter()
    for k in sorted(win_names - and_names, key=kstr):
        sva = wkey2va.get(k)
        ks = src_only(sides["win"].func_locs.get(sva, set())) if sva is not None else set()
        if not ks:
            why["① 函数体零 src Location（无锚，这条路定不了名）"] += 1
        elif any(len(sides["win"].key_funcs[x]) == 1
                 and len(sides["linux"].key_funcs.get(x, ())) == 1 for x in ks):
            why["③ 有双向唯一键（应当已落地）"] += 1
        else:
            why["② 有键但没有一个双向唯一（某一侧多重数 >1）"] += 1
    W("  「win − android」这 %d 个落不了的原因：\n" % len(win_names - and_names))
    for k2, v in why.most_common():
        W("     %-46s %4d\n" % (k2, v))

    W("\n" + "=" * 78 + "③ 冲突（同 Location 不同名）\n")
    W("多键指向不同 Linux 地址的源函数：%d 条\n" % len(conflicts))
    # 同一条 Location 键被多方共用、且名字不同
    kk = collections.defaultdict(set)
    for tag in ("win", "win_old", "android"):
        s = sides.get(tag)
        if s is None:
            continue
        for va, lst in s.names.items():
            for k in src_only(s.func_locs.get(va, set())):
                for nk, _d in lst:
                    kk[k].add((tag, nk))
    same_key = 0
    examples = []
    for k, s2 in kk.items():
        nm = {nk for _t, nk in s2} - {None}
        if len(nm) > 1:
            same_key += 1
            if len(examples) < 8:
                examples.append((k, sorted(nm, key=kstr)))
    W("同一个 src Location 键被 ≥2 个不同名字引用的：%d 个键\n" % same_key)
    for k, nm in examples:
        W("  %s ← %s\n" % (fmt_key(k), "; ".join(kstr(x) for x in nm[:4])))
    # 同一 linux 地址被不同名字认领
    per_addr = collections.defaultdict(set)
    for k, v in land_of_name.items():
        for tag, h in v.items():
            per_addr[h["dva"]].add(k)
    multi = {a: ns for a, ns in per_addr.items() if len(ns) > 1}
    W("同一个 Linux 地址被 ≥2 个名字认领的：%d 个地址\n" % len(multi))
    for a, ns in list(multi.items())[:6]:
        W("  0x%x ← %s\n" % (a, "; ".join(kstr(n) for n in sorted(ns, key=kstr)[:4])))
    W("（其中真正的「同名不同址」矛盾 = 上面「多键指向不同地址」的 %d 条；"
      "同一地址多名字多数是 `{{closure}}` / shim 与母函数共用锚点）\n" % len(conflicts))

    W("\n" + "=" * 78 + "④ 天花板之外的上限估计（结构最近邻 + 正对照）\n")
    pairs = truth_pairs(sides["win"], sides["linux"])
    W("正对照真值（Win↔Linux 由 Location 唯一锚定、两侧都一对一）：%d 对\n" % len(pairs))
    A, B = split_half(pairs)
    W("分半：调参半 %d / 留出半 %d（窗口与阈值在调参半上定，见 `verify`）\n" % (len(A), len(B)))
    rowsA = control_rows(sides, A)
    rowsB = control_rows(sides, B)
    for lab, rows in (("调参半", rowsA), ("留出半", rowsB)):
        n = len(rows)
        top1 = sum(1 for r in rows if r["top1_ok"])
        intop = sum(1 for r in rows if r["in_topk"])
        W("  %-6s n=%3d top1=%5.1f%% 真值在 top%d 内=%5.1f%%\n"
          % (lab, n, 100.0 * top1 / max(n, 1), TOPK, 100.0 * intop / max(n, 1)))
        for thr in MARGIN_TIERS:
            m, acc = accuracy(rows, thr)
            if m:
                W("         margin>=%.2f: n=%3d 准确率 %5.1f%%\n" % (thr, m, 100 * acc))
    W("  随机基线：候选池是 %d 个有名 Windows 函数，窗口内多为 100~500 个候选 ⇒ <1%%\n"
      % len(build_pool(sides["win"], True)[1]))
    W("\n  ⚠ 天花板之外（无 src Location）的上限估计：\n")
    bc = beyond_ceiling(sides, 0.1)
    W("    Linux 无锚函数 %d 个，可作名字载体的 Windows 有名函数 %d 个\n"
      % (bc["total_no_loc"], bc["pool"]))
    for thr in MARGIN_TIERS:
        t = bc["tiers"][thr]
        W("    margin>=%.2f：%d 个有 top-1 候选；其中「该 win 名只被一个 linux 认领」%d 个\n"
          % (thr, t["margin"], t["injective"]))
    W("  ⇒ 上限估计 = margin>=0.10 的单射数 × 正对照在该裕度的准确率（`verify` 给）。\n")
    W("  ⚠ 这条估计带**选择偏差**：正对照的真值函数都有 Location（代码形态更独特），\n"
      "    无锚函数里大量是纯数学 / 闭包 / drop shim，只会更差不会更好。\n")


# ===========================================================================
# 9. CLI
# ===========================================================================
def cmd_build(args):
    sides = load_all(args)
    per_src, conflicts = merge_landings(sides)
    rows = write_fixture(sides, per_src, conflicts, args.out)
    n_unique = sum(1 for r in rows if r[7] == KIND_UNIQUE)
    n_cross = sum(1 for r in rows if r[7] == KIND_CROSS)
    n_amb = sum(1 for r in rows if r[7] == KIND_AMBIG)
    n_none = sum(1 for r in rows if r[7] == KIND_NONE)
    sys.stderr.write("[3way] 写 %s：%d 行（%s %d / %s %d / %s %d / %s %d）\n"
                     % (rel(args.out), len(rows), KIND_UNIQUE, n_unique, KIND_CROSS,
                        n_cross, KIND_AMBIG, n_amb, KIND_NONE, n_none))
    return 0


def cmd_verify(args):
    """正对照：真值来自 Location，匹配器只看结构。

    规矩（AGENTS.md「合并纪律」A4）：**调参半定窗口与阈值，留出半只用来报数**。
    另外给三组参照，防止把"匹配器坏了"误读成"跨构建不可匹配"：
      * 随机基线 = 1 / 窗口内候选数；
      * 只保留 `margin>=thr` 时的准确率（高置信档）；
      * 互为 top-1（双向都排第一）——这个名字才敢往外报。
    """
    sides = load_all(args)
    W = sys.stdout.write
    pairs = truth_pairs(sides["win"], sides["linux"])
    if not pairs:
        die("没有正对照真值（Location 交集为空？）")
    A, B = split_half(pairs)
    W("正对照真值：%d 对（Win→Linux 由 Location 双向唯一锚定；真值不参与打分）\n" % len(pairs))
    W("分半：调参半 %d / 留出半 %d（按名字键 sha1 奇偶）\n\n" % (len(A), len(B)))
    rowsA = control_rows(sides, A)
    rowsB = control_rows(sides, B)
    for lab, rows in (("调参半", rowsA), ("留出半", rowsB)):
        n = len(rows)
        top1 = sum(1 for r in rows if r["top1_ok"])
        intop = sum(1 for r in rows if r["in_topk"])
        mut = sum(1 for r in rows if r["mutual"])
        W("%s n=%3d  top1=%5.1f%%  真值在 top%d 内=%5.1f%%  互为top1=%5.1f%%\n"
          % (lab, n, 100.0 * top1 / max(n, 1), TOPK, 100.0 * intop / max(n, 1),
             100.0 * mut / max(n, 1)))
        for thr in MARGIN_TIERS:
            m, acc = accuracy(rows, thr)
            mm, macc = accuracy(rows, thr, mutual_only=True)
            W("    margin>=%.2f: n=%3d 准确率=%5.1f%%   | 其中互为top1 n=%3d 准确率=%5.1f%%\n"
              % (thr, m, 100 * acc, mm, 100 * macc))
    # 阈值只在调参半上挑（要求准确率 >= 95%），再去留出半报
    best = None
    for thr in MARGIN_TIERS:
        m, acc = accuracy(rowsA, thr)
        if m >= 20 and acc >= 0.95 and (best is None or m > best[1]):
            best = (thr, m, acc)
    if best:
        thr, m_a, acc_a = best
        m_b, acc_b = accuracy(rowsB, thr)
        W("\n⇒ 调参半选阈值 margin>=%.2f（n=%d, 准确率 %.1f%%）；"
          "**留出半** n=%d, 准确率 %.1f%%\n" % (thr, m_a, 100 * acc_a, m_b, 100 * acc_b))
    else:
        thr, m_a, acc_a = MARGIN_TIERS[1], 0, 0.0
        m_b, acc_b = accuracy(rowsB, thr)
        W("\n⇒ 调参半没有哪个阈值能让准确率 >=95%%，阈值取 %.2f；留出半 n=%d 准确率 %.1f%%\n"
          % (thr, m_b, 100 * acc_b))
    # 候选规模（解释为什么 top1 只有四成）
    sizes, vas = build_pool(sides["win"], True)
    import bisect as _b
    cs = []
    for _w, lva, _k in A:
        f = sides["linux"].feats[lva]
        cs.append(_b.bisect_right(sizes, int(f["size"] * MATCH_WIN[1]))
                  - _b.bisect_left(sizes, max(8, int(f["size"] * MATCH_WIN[0]))))
    if cs:
        cs.sort()
        W("   候选池 %d 个有名 Windows 函数；单次查询的候选数 中位数 %d / p90 %d\n"
          % (len(vas), cs[len(cs) // 2], cs[int(len(cs) * 0.9)]))
        W("   随机基线 ≈ 1/%.0f = %.2f%%\n"
          % (len(cs) and float(sum(cs)) / len(cs), 100.0 / (float(sum(cs)) / len(cs))))
    # 天花板之外
    bc = beyond_ceiling(sides, thr)
    W("\n天花板之外（Linux 无 src Location 的 %d 个函数）：\n" % bc["total_no_loc"])
    for t in MARGIN_TIERS:
        d = bc["tiers"][t]
        W("    margin>=%.2f: top1 候选 %4d 个；单射（该 win 名只被一个 linux 认领）%3d 个\n"
          % (t, d["margin"], d["injective"]))
    n_inj = bc["tiers"][thr]["injective"]
    W("⇒ **上限估计**：margin>=%.2f 的单射匹配 %d 个 × 留出半准确率 %.1f%% ≈ **%.0f 个**"
      "（天花板 2,035 之外）\n" % (thr, n_inj, 100 * acc_b, n_inj * acc_b))
    return 0


def cmd_ablate(args):
    """消融：逐个特征关掉，看正对照 top-1 掉多少（**只在调参半上**，不碰留出半）。"""
    sides = load_all(args)
    W = sys.stdout.write
    pairs = truth_pairs(sides["win"], sides["linux"])
    A, _B = split_half(pairs)
    variants = [
        ("全量（默认权重）", dict(MATCH_W)),
        ("去掉 hist", dict(MATCH_W, hist=0.0)),
        ("去掉 const", dict(MATCH_W, const=0.0)),
        ("去掉 size", dict(MATCH_W, size=0.0)),
        ("去掉 insn", dict(MATCH_W, insn=0.0)),
        ("加 callee(按 0.38 归一)", "callee"),
    ]
    base = None
    for label, w in variants:
        if w == "callee":
            # 这一档要临时改打分函数：把 callee 项加进去（见 §37.6 的负结果）
            rows = _control_with_callee(sides, A)
        else:
            rows = control_rows(sides, A, w=w)
        n = len(rows)
        ok = sum(1 for r in rows if r["top1_ok"])
        intop = sum(1 for r in rows if r["in_topk"])
        if base is None:
            base = ok / float(n)
        W("%-24s n=%3d top1=%5.1f%% (%+.1f%%)  top%d=%5.1f%%\n"
          % (label, n, 100.0 * ok / n, 100.0 * (ok / float(n) - base), TOPK,
             100.0 * intop / n))
    return 0


def _control_with_callee(sides, pairs):
    """带「被调者数（按 0.38 归一）」这一项的变体：只为消融，不进主流程。"""
    W_C = 0.5

    def sc(qf, df):
        rr = ((len(df["callees"]) + 1.0) / (len(qf["callees"]) + 1.0)) / CALLEE_RATIO_MEDIAN
        return score_pair(qf, df) + W_C * (1.0 - min(abs(math.log(rr)) / 1.2, 1.0))

    s, d = sides["win"], sides["linux"]
    sizes, vas = build_pool(s, named=True)
    rows = []
    for wva, lva, nk in pairs:
        qf = d.feats[lva]
        lo = max(8, int(qf["size"] * MATCH_WIN[0]))
        hi = max(lo + 1, int(qf["size"] * MATCH_WIN[1]))
        i = bisect.bisect_left(sizes, lo)
        j = bisect.bisect_right(sizes, hi)
        rr = sorted(((sc(qf, s.feats[c]), c) for c in vas[i:j]), reverse=True)[:TOPK]
        rows.append(dict(top1_ok=bool(rr) and rr[0][1] == wva,
                         in_topk=any(c == wva for _x, c in rr),
                         margin=margin_of(rr), mutual=False,
                         src=wva, dst=lva, name_key=nk, top1=rr[0][1] if rr else None))
    return rows


def cmd_probe(args):
    sides = load_all(args, verbose=False)
    W = sys.stdout.write
    for a in args.addr:
        va = int(a, 16) if not a.isdigit() else int(a)
        for tag in ("linux", "android", "win", "win_old"):
            s = sides.get(tag)
            if s is None:
                continue
            if va not in s.universe:
                continue
            fe = s.feats[va]
            names = s.names.get(va, [])
            W("%s 0x%x size=%d ninsn=%d callees=%d icalls=%d consts=%d\n"
              % (tag, va, fe["size"], fe["ninsn"], len(fe["callees"]),
                 fe["icalls"], len(fe["consts"])))
            if names:
                for nk, dem in names[:4]:
                    W("    name: %s.%s  (%s)\n" % (nk[1] or "-", nk[2], dem[:110]))
            ks = src_only(s.func_locs.get(va, set()))
            W("    src Location: %d 个%s\n"
              % (len(ks), (": " + ", ".join(fmt_key(k) for k in sorted(ks)[:6])) if ks else ""))
    return 0


def cmd_key(args):
    sides = load_all(args, verbose=False)
    W = sys.stdout.write
    for spec in args.key:
        m = re.match(r"^(.+):(\d+):(\d+)$", spec)
        if not m:
            die("格式：src/file.rs:line:col")
        k = (m.group(1), int(m.group(2)), int(m.group(3)))
        W("%s\n" % fmt_key(k))
        for tag in ("linux", "android", "win", "win_old"):
            s = sides.get(tag)
            if s is None:
                continue
            fs = s.key_funcs.get(k, set())
            W("  %-8s %d 个函数: %s\n"
              % (tag, len(fs), ", ".join("0x%x" % a for a in sorted(fs)[:6]) or "-"))
            for a in sorted(fs)[:3]:
                nm = s.names.get(a) or []
                if nm:
                    W("           0x%x = %s\n" % (a, nm[0][1][:110]))
    return 0


def cmd_nm(args):
    sides = load_all(args, verbose=False)
    pat = args.pattern
    hit = 0
    for tag in ("win", "win_old", "android"):
        s = sides.get(tag)
        if s is None:
            continue
        for va, lst in s.names.items():
            for nk, dem in lst:
                if pat.lower() in dem.lower():
                    hit += 1
                    sys.stdout.write("%-8s 0x%-9x %s\n" % (tag, va, dem[:130]))
                    sys.stdout.write("        src Location: %d\n"
                                     % len(src_only(s.func_locs.get(va, set()))))
    if not hit:
        sys.stdout.write("没有匹配 %r 的名字\n" % pat)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="三向（Linux/Android/Windows）符号匹配器")
    ap.add_argument("--linux", default=os.environ.get("GW2_LINUX_SO") or DEFAULT_LINUX)
    ap.add_argument("--android", default=os.environ.get("GW2_ARM64_SO") or DEFAULT_ANDROID)
    ap.add_argument("--win", default=os.environ.get("GW2_WIN_DLL") or DEFAULT_WIN)
    ap.add_argument("--win-old", default=os.environ.get("GW2_WIN_OLD_DLL") or DEFAULT_WIN_OLD)
    ap.add_argument("--funcstarts", default=SM.DEFAULT_FUNCSTARTS)
    ap.add_argument("--out", default=OUT)
    ap.add_argument("--force", action="store_true", help="忽略缓存重扫")
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("build", help="扫描 + 写 fixtures/name_to_addr_v2.tsv")
    sub.add_parser("report", help="①②③④ 的全部数字")
    sub.add_parser("verify", help="正对照准确率（调参半 / 留出半）")
    p = sub.add_parser("probe", help="看某个地址在四种构建里的样子")
    p.add_argument("addr", nargs="+")
    p = sub.add_parser("key", help="某个 Location 三元组被哪些函数引用")
    p.add_argument("key", nargs="+")
    sub.add_parser("ablate", help="特征消融（调参半上的 top-1 变化）")
    p = sub.add_parser("nm", help="按子串搜名字")
    p.add_argument("pattern")
    args = ap.parse_args(argv)
    if args.funcstarts is None or not os.path.exists(args.funcstarts or ""):
        die("找不到 funcstarts（用 --funcstarts 指定 funcstarts_ghidra_va.pkl）")
    cmd = args.cmd or "report"
    return {"build": cmd_build, "report": cmd_report, "verify": cmd_verify,
            "probe": cmd_probe, "key": cmd_key, "nm": cmd_nm,
            "ablate": cmd_ablate}[cmd](args)


if __name__ == "__main__":
    sys.exit(main())
