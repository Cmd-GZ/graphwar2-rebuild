#!/usr/bin/env python3
"""callgraph_fingerprint.py — 给**没有 `panic::Location`** 的函数做结构身份指纹。

## 这个工具解决的是哪一段

Linux 目标 `.so` 有 13,263 个函数（= Ghidra 起点表口径）：

    527 个引用了 `src/*.rs` 的 `Location` ⇒ 已由 §37/§39/§40 定名或估名（348 + 179）
    12,736 个**一个 `src/` Location 都不引用** ⇒ 三份符号表里都没有它们，名字要不回来了

本节不猜名字，只给这 12,736 个函数**结构身份**：

    (a) 跨构建同源候选（这个 Linux 函数 ≈ Windows / Android 的哪个函数）
    (b) 结构角色（尺寸 / 指令构成 / 常量 / 被调者 / 调用者 / 引用字符串）
    (c) 是否编译器产物（ICF 等体组：`bsig` 相同 ⇒ 链接器把同一份机器码合并了）

## 三条硬规矩（都是本项目实测踩出来的，不照做就是自证）

1. **不用名字、不用地址、不用 `Location` 键做匹配**。名字只在**正对照**里当真值。
   理由：`Location` 键是个巨大的捷径（§40 的候选池就是被它筛过的），一旦用它，
   测出来的就不是本工具在 12,736 上的表现。
2. **正对照的分半与阈值必须先在调参半定，再在留出半报数**（`AGENTS.md` 合并纪律 A4）。
   真值 = `Location` 双向唯一锚定的「Linux 地址 ↔ Windows/Android 地址」对，
   它由编译期常量给出，**完全独立**于本工具的打分。
3. **候选池 = 另一份构建的**整个函数宇宙**（Windows 51,985 / Android 12,609），
   不是"有名函数池"**。12,736 个的对应体大多本身也没名字，池子只放有名的就是作弊。

## 四份构建的宇宙口径（与既有夹具/工具同一套，不是这里另起的）

| 构建 | 口径 | 个数 |
|---|---|---|
| Linux 2026-08-28 | Ghidra 起点表 `funcstarts_ghidra_va.pkl`（与 `name_to_addr.tsv` 同口径） | 13,263 |
| Android 2.0.60 | `.symtab` 的 FUNC 符号 | 12,609 |
| Windows 2.0.60 | `.pdata` 的 `RUNTIME_FUNCTION` + 未被覆盖的 COFF FUNC 符号 | 51,985 |
| Windows early2026 | 同上 | 43,096 |

## 指纹字段（每份构建一份，缓存在 `.agent-work`，**不入库**）

    size / ninsn / hist(助记符) / cls(跨 ISA 归一后的类直方图) / consts(整数与 f64 位型)
    callees(被调者地址) / callee_bsig(ICF 归一后的被调者多重集) / callee_sizes(跨构建可比的多重集)
    callers / caller_sizes / bsig(自身机器码 hash) / locrefs
    strs(引用到的字符串内容 hash，**含泄漏**) / strs_loc(其中是 Rust 源文件路径的那些)
    strs_s(= strs − strs_loc，**默认只用这一份**；本代权重 0，见下)

    ⚠ **`strs` 是泄漏，默认权重 0**：`panic::Location` 前 16 B 是 `(&'static str, len)`，
    被 `string_at` 的 (ptr,len) 路径解成**文件名**（正对照 348/348 带自己的 `src/*.rs`，
    目标集合 0/12,736 带）。扫描时把一切 `.rs` 路径单列进 `strs_loc`，`strs_s` 是扣掉它们之后的
    结构字符串；`strs` 只留给 `ablate` 复现消融表。见 `docs/159` §3.2、`docs/02` §35.3.1。

`callee_bsig` 是"含 ICF 归并后的规范化"：链接器把**逐字节相同**的函数合并成同一份机器码，
所以被调者身份必须用**机器码 hash**表示，否则同一份代码在 A 构建里是 40 个地址、
在 B 构建里是 41 个地址，多重集永远对不上。跨 ISA 则退一步只用 `callee_sizes`（不含偏移/立即数）。

## 用法

    ../.reference/venv/bin/python tools/callgraph_fingerprint.py build         # 扫四份构建（capstone，约 2.5 min）
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py verify        # 正对照（分半，top-1/top-5）
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py tune2         # 窗口召回 + 特征配置（**调参半**）
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py calib         # 门槛 / 档位规则 / 体量分桶
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py run --shard 0/8 --out x.tsv
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py merge x?.tsv  # 合并分片 → 夹具
                                     # （判档 + **全局**单射闸门在这里重算，见 apply_gates）
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py report        # 规模 + 体量/档位分布
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py probe 0x3a2e10

    # 去泄漏重训三件套（都吃 `--jobs N` 进程池；并行**不改数字**，只改墙钟）
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py ablate --jobs 15 --coverage
                                     # 一个特征一个特征清零，**两半都报**（归因表）
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py tune3 --jobs 12
                                     # 权重网格，**只在调参半上选**；--axes/--fixed/--holdout
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py runall --shards 8 --jobs 8
                                     # 8 片并行跑全量 12,736 + merge → 夹具

`--weights "k=v,..."` 可以覆盖默认权重（`verify`/`tune2`/`calib` 都吃）；复现**旧代含泄漏**的
84.2% 用 `verify --weights "hist=0.5,consts=0.5,strs=0.2,strs_s=0"`。

`build` 需要 capstone（仓库 `.reference/venv`）；其余子命令只读缓存，标准库即可。
"""

import argparse
import bisect
import collections
import contextlib
import hashlib
import io
import itertools
import math
import multiprocessing
import os
import pickle
import re
import struct
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import sym_match as SM                       # noqa: E402  Elf / demangle / name_key
import three_way_match as T                  # noqa: E402  Pe / 宇宙 / LocHit / elf_locations

WS = SM.GW2                                  # 仓库根


def _first_existing(env, *cands):
    """按 环境变量 → 候选列表 取第一个存在的路径；都不存在就返回首选，让报错指向真位置。

    为什么要这么写（`docs/159` §1）：这三个路径曾经写死在**已经不存在的**目录里
    （`.reference/windows/graphwarrust-*.dll`、`.reference/upstream/analysis/…pkl`），
    §35.9 的「一条命令复算」原样跑会直接 `T.die`，于是只能靠影子根 + 符号链接复跑 ——
    数字没错，但复算命令是坏的。落盘布局是会变的（`sym_match._reference_root` 为此上溯过），
    所以这里**不再写死任何一处**。
    """
    if os.environ.get(env):
        return os.environ[env]
    for c in cands:
        p = c if os.path.isabs(c) else os.path.join(WS, c)
        if os.path.exists(p):
            return p
    return os.path.join(WS, cands[0])


BUILDS = collections.OrderedDict([
    ("linux", dict(kind="elf", isa="x86", funcstarts=True, names=False,
                   path=_first_existing("GW2_LINUX_SO",
                                        ".reference/steam/2026-08-28/libgraphwarrust.so",
                                        "libgraphwarrust.so"))),
    ("android", dict(kind="elf", isa="arm64", funcstarts=False, names=True,
                     path=_first_existing("GW2_ANDROID_SO",
                                          ".reference/steam/android-2.0.60/libgraphwarrust.so"))),
    ("win", dict(kind="pe", isa="x86", funcstarts=False, names=True,
                 path=_first_existing("GW2_WIN_DLL",
                                      ".reference/steam/windows-2.0.60/graphwarrust.dll",
                                      ".reference/windows/graphwarrust-2.0.60.dll"))),
    ("win_old", dict(kind="pe", isa="x86", funcstarts=False, names=True,
                     path=_first_existing("GW2_WIN_OLD_DLL",
                                          ".reference/steam/windows-early2026/graphwarrust.dll",
                                          ".reference/windows/graphwarrust-early2026.dll"))),
])
# 函数起点表：复用 `sym_match._find_funcstarts()` 的上溯（`GW2_FUNCSTARTS` → 逐级 `analysis/`
# → `.reference/upstream/analysis/` → 旧路径）。全仓库就这一个来源，别在这里另起一份。
FUNCSTARTS = SM.DEFAULT_FUNCSTARTS
FIXTURES = os.path.join(ROOT, "fixtures")
OUT_TSV = os.path.join(FIXTURES, "callgraph_fingerprint.tsv")

# 缓存版本：反汇编 / 特征一改就抬，免得静默吃旧缓存（sym_match §36.0 的坑）
#   v4 → v5：`strs` 拆成「原始」与「去掉 Rust 源文件路径」两份（见 RE_LOC_FILE 的注释）；
#            v4 的缓存**没有** `strs_s`，所以必须重扫，不能混用。
CACHE_VERSION = "cfgfp-v5"

STR_SECTIONS = (".rodata", ".data.rel.ro", ".data.rel.ro.local", ".rdata", ".rdata$r")
MAX_STR = 96

# `panic::Location` 的 `file` 字段长这样：`src/graphwar_root_node.rs`、`library/core/src/x.rs`。
# 它会被 `string_at` 当成普通字符串收进 `strs`（Location 结构体前 16 B 就是 `(&'static str, len)`），
# 而**这一半特征只在正对照一侧 100% 可得**（正对照 348/348 带项目文件名，目标集合只有 11.8%）
# ⇒ 它是泄漏，不是结构（`docs/159` §3.2）。扫描时把命中的 hash 单独记进 `strs_loc`，
# 打分器默认只用 `strs_s = strs - strs_loc`；原始 `strs` 留着**只**用于复现消融表（默认权重 0）。
RE_LOC_FILE = re.compile(rb"^[A-Za-z0-9_./-]+\.rs$")


def is_loc_file(s):
    """这个字符串是不是 Rust 源文件路径（⇒ `panic::Location` 的 `file` 投影）？"""
    return bool(RE_LOC_FILE.match(s))

# ===========================================================================
# 1. 指令 → 类（跨 ISA 可比的那一层）
#
# 为什么需要这一层：arm64 的 `ldr`/x86 的 `mov r,[m]` 是同一个语义，但助记符不同 ⇒
# 原始助记符直方图**不能跨 ISA 比**（§40.2 因此把 Android 排除在结构排序之外）。
# 归一成 12 个类之后才能比；代价是丢掉了「是哪一条具体指令」的信息。
# ===========================================================================
CLS_X86 = {
    "move": "move", "movabs": "move", "movq": "move", "movd": "move", "movzx": "move",
    "movsx": "move", "movsxd": "move", "xchg": "move", "cmove": "sel", "cmovne": "sel",
    "arith": "arith", "add": "arith", "sub": "arith", "adc": "arith", "sbb": "arith",
    "imul": "arith", "mul": "arith", "idiv": "arith", "div": "arith", "neg": "arith",
    "inc": "arith", "dec": "arith", "lea": "arith", "adcx": "arith", "adox": "arith",
    "and": "bit", "or": "bit", "xor": "bit", "not": "bit", "shl": "bit", "shr": "bit",
    "sar": "bit", "sal": "bit", "rol": "bit", "ror": "bit", "bswap": "bit", "bt": "bit",
    "bsf": "bit", "bsr": "bit", "tzcnt": "bit", "lzcnt": "bit", "popcnt": "bit",
    "andn": "bit", "bextr": "bit", "blsi": "bit", "blsr": "bit", "pdep": "bit", "pext": "bit",
    "cmp": "cmp", "test": "cmp", "comis": "cmp", "ucomis": "cmp", "ptest": "cmp",
    "jmp": "jump", "call": "call", "ret": "ret", "retf": "ret",
    "push": "stack", "pop": "stack", "leave": "stack", "enter": "stack",
    "nop": "pad", "int3": "pad", "ud2": "pad", "endbr64": "pad", "hlt": "pad",
    "syscall": "other", "cdqe": "other", "cqo": "other", "cqto": "other", "cltq": "other",
    "cdq": "other", "cwd": "other", "cwde": "other", "movsb": "other",
}

CLS_ARM = {
    "mov": "move", "movz": "move", "movk": "move", "movn": "move", "adr": "move",
    "adrp": "move", "uxtb": "move", "uxth": "move", "uxtw": "move", "sxtb": "move",
    "sxth": "move", "sxtw": "move", "sxtx": "move", "mvn": "move", "fmov": "fpu",
    "add": "arith", "adds": "arith", "sub": "arith", "subs": "arith", "neg": "arith",
    "negs": "arith", "mul": "arith", "madd": "arith", "msub": "arith", "mneg": "arith",
    "sdiv": "arith", "udiv": "arith", "smull": "arith", "umull": "arith",
    "smulh": "arith", "umulh": "arith", "smaddl": "arith", "umaddl": "arith",
    "and": "bit", "ands": "bit", "orr": "bit", "eor": "bit", "bic": "bit", "bics": "bit",
    "orn": "bit", "eon": "bit", "lsl": "bit", "lsr": "bit", "asr": "bit", "ror": "bit",
    "clz": "bit", "rbit": "bit", "rev": "bit", "rev16": "bit", "rev32": "bit",
    "bfi": "bit", "bfxil": "bit", "ubfx": "bit", "sbfx": "bit", "ubfiz": "bit",
    "sbfiz": "bit", "extr": "bit", "ubfz": "bit",
    "cmp": "cmp", "cmn": "cmp", "tst": "cmp", "cbz": "cmp", "cbnz": "cmp",
    "tbz": "cmp", "tbnz": "cmp",
    "csel": "sel", "csinc": "sel", "csinv": "sel", "csneg": "sel", "cset": "sel",
    "csetm": "sel", "cinc": "sel", "cinv": "sel", "cneg": "sel",
    "b": "jump", "br": "jump", "bl": "call", "blr": "call", "ret": "ret",
    "nop": "pad", "brk": "pad", "hlt": "pad", "udf": "pad", "dmb": "pad", "dsb": "pad",
    "isb": "pad", "bti": "pad", "paciasp": "pad", "autiasp": "pad", "pacibsp": "pad",
    "autibsp": "pad",
}


def cls_of(isa, mn, insn):
    """一条指令 → 类。x86 的 `mov` 按操作数区分 load/store，才和 arm64 的 ldr/str 对得上。"""
    if isa == "x86":
        if mn in CLS_X86:
            return CLS_X86[mn]
        if mn.startswith("cmov"):
            return "sel"
        if mn.startswith("set"):
            return "sel"
        if mn.startswith("j"):
            return "jump"
        if mn[-3:] in ("ssd", "sds") or mn[-2:] in ("ss", "sd", "ps", "pd"):
            return "fpu"
        if mn.startswith(("cvt", "sqrt", "ucomi", "comi", "padd", "psub", "pmul",
                          "pxor", "pand", "por", "punpck", "pshuf", "vmov", "vadd",
                          "vsub", "vmul", "vdiv", "vxor", "vcvt")):
            return "fpu"
        if mn in ("movss", "movsd", "movaps", "movups", "movdqa", "movdqu"):
            return "fpu"
        if mn.startswith("mov"):
            # 操作数 0 是内存 ⇒ 写内存（store），否则读内存（load）
            try:
                from capstone.x86 import X86_OP_MEM
                ops = insn.operands
                if ops and ops[0].type == X86_OP_MEM:
                    return "store"
                if len(ops) > 1 and ops[1].type == X86_OP_MEM:
                    return "load"
            except Exception:
                pass
            return "move"
        return "other"
    # arm64
    if mn in CLS_ARM:
        return CLS_ARM[mn]
    if mn.startswith("b.") or mn.startswith("bc."):
        return "jump"
    if mn.startswith("ldr"):
        return "load"
    if mn.startswith("str"):
        return "store"
    if mn.startswith(("ldp", "ldur", "ldar", "ldaxr")):
        return "load"
    if mn.startswith(("stp", "stur", "stlr", "stxr")):
        return "store"
    if mn.startswith("f"):            # 标量 FP / 部分向量指令都是 f 开头
        return "fpu"
    if mn.startswith(("add", "sub", "mul", "div", "neg")):
        return "arith"
    if mn.startswith(("and", "orr", "eor", "lsl", "lsr", "asr")):
        return "bit"
    return "other"


_MN_ID = {}


def mn_id(mn):
    """助记符 → 16 位稳定 id（**跨 run、跨构建一致**：用 sha1 而不是 `hash()`）。

    序列特征按 2 字节/指令存成 `bytes`（x86-64 的 6M 条指令也只有 12 MB），
    代价是 90 来个助记符里可能撞 0~1 对（65536 桶）——撞了只会让相似度略降，
    不会把两个不同助记符当成完全不同类，比按"首次出现顺序"编号强（那种编号跨构建不可比）。
    """
    v = _MN_ID.get(mn)
    if v is None:
        v = int(hashlib.sha1(mn.encode()).hexdigest()[:4], 16)
        _MN_ID[mn] = v
    return v


def stable_hash(items, n=16):
    """**与 PYTHONHASHSEED 无关**的 hash（§39.7 的教训：输出不许随 hash 种子变）。"""
    h = hashlib.sha1()
    for it in sorted(items):
        h.update(str(it).encode())
        h.update(b"\x1f")
    return h.hexdigest()[:n]


# ===========================================================================
# 2. 扫描：一次线性反汇编，产出全部指纹（两份 ISA 各一个实现）
# ===========================================================================
def disasm_all(cs, code, base, end, align=4):
    """容错线性反汇编：解码失败就跳过 `align` 字节重来。

    **为什么必须有这个**：capstone 的 `cs.disasm` 在遇到第一个解不出的字时就结束生成器，
    不会跳过。实测 Android 的 `.text`（5,223,648 B）直接扫只出 673,319 条指令
    （= 51.6% 的字节），**后半个 `.text` 全部函数是零指令** —— 这不是"没有函数"，
    是扫描器自己停了。改成跳洞之后覆盖率见 `build` 打印的 `漏洞` 列。
    """
    addr = base
    holes = 0
    while addr < end:
        got = False
        for insn in cs.disasm(code[addr - base:], addr):
            got = True
            yield insn
            addr = insn.address + insn.size
        if not got:
            holes += 1
            addr += align
    if holes:
        sys.stderr.write("[fp] 线性反汇编跳洞 %d 处（每处 %d 字节）\n" % (holes, align))


def _trigrams(seq):
    """2 字节/指令的助记符序列 → 三元组 id 集合（顺序敏感、抗位移差异）。"""
    n = len(seq) // 2
    out = set()
    m = None
    for i in range(n - 2):
        o = 2 * i
        v = seq[o] | (seq[o + 1] << 8)
        w = seq[o + 2] | (seq[o + 3] << 8)
        x = seq[o + 4] | (seq[o + 5] << 8)
        out.add((v * 65536 + w) * 65536 + x)
        m = x
    if not out and n:
        out.add((seq[0] | (seq[1] << 8)) * 65536)
    return out


def _cstr(obj, va, limit=MAX_STR):
    """`va` 处若是可打印 C 字符串（NUL 结尾），返回内容。"""
    o = obj.va2off(va)
    if o is None:
        return None
    d = obj.data[o:o + limit]
    n = 0
    while n < len(d) and 32 <= d[n] < 127:
        n += 1
    if n < 4 or n >= len(d):        # 不是以 NUL 结尾 ⇒ 读到的不是字符串起始
        return None
    return d[:n]


def _ptr_at(obj, va):
    """读 8 字节：ELF 优先用 `R_*_RELATIVE` 的 addend（文件里那一格是 0），PE 直接读。"""
    if hasattr(obj, "ptr_at"):
        return obj.ptr_at(va)
    raw = obj.read(va, 8)
    return struct.unpack("<Q", raw)[0] if raw and len(raw) == 8 else None


def string_at(obj, va, ro_lo=0, ro_hi=0):
    """`va` 引用的字符串内容 —— 两条路径都算（与 §20.2 的 `&'static str` 口径一致）：

    ① 直接指向字符串字节（`lea rdi,[rip+.rodata]`）；
    ② 指向**太**（ptr,len）槽（`.data.rel.ro` 里 8+8 字节）：Rust 的 `&str` 常量常这样放，
       不解析它就只能看到 2,073 个函数有字符串（实测），解析后覆盖率翻倍（见 `build` 输出）。
    """
    s = _cstr(obj, va)
    if s is not None:
        return s
    p = _ptr_at(obj, va)
    if p is None:
        return None
    ln = _ptr_at(obj, va + 8)
    if ln is None or not (4 < ln <= MAX_STR):
        return None
    if ro_lo and not (ro_lo <= p < ro_hi):
        return None
    o = obj.va2off(p)
    if o is None:
        return None
    b = obj.data[o:o + ln]
    if len(b) != ln or any(c < 32 or c >= 127 for c in b):
        return None
    return b


def scan_x86(obj, universe, locs, verbose=True):
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64
    from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_REG_RIP

    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    tex = obj.sec[".text"]
    code = obj.read(tex["addr"], tex["size"])
    starts = sorted(universe)
    hit = T.LocHit(locs)
    ro = {n: obj.sec[n] for n in STR_SECTIONS if n in obj.sec}
    ro_lo = min((s["addr"] for s in ro.values()), default=0)
    ro_hi = max((s["addr"] + s["size"] for s in ro.values()), default=0)

    def sec_name(va):
        for n, s in ro.items():
            if s["addr"] <= va < s["addr"] + s["size"]:
                return n
        return None

    feats = {va: dict(ninsn=0, hist=collections.Counter(), cls=collections.Counter(),
                      consts=set(), calls=[], icalls=0, strs=set(), strs_loc=set(),
                      locrefs=set(), seq=[])
             for va in universe}
    for insn in disasm_all(cs, code, tex["addr"], tex["addr"] + tex["size"], 1):
        owner = T.owner_of(starts, universe, insn.address)
        if owner is None:
            continue
        fe = feats[owner]
        fe["ninsn"] += 1
        fe["hist"][insn.mnemonic] += 1
        fe["seq"].append(mn_id(insn.mnemonic))
        fe["cls"][cls_of("x86", insn.mnemonic, insn)] += 1
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
                    continue
            else:
                continue
            lv = hit.lookup(tgt)
            if lv is not None:
                fe["locrefs"].add(lv)
            if tgt in universe:                       # 取函数地址（函数指针 / 跳转表）
                # **不**进 consts：地址跨构建毫无意义，塞进去只会稀释 Jaccard。
                continue
            if sec_name(tgt):
                s = string_at(obj, tgt, ro_lo, ro_hi)
                if s is not None:
                    h = hashlib.sha1(s).hexdigest()[:8]
                    fe["strs"].add(h)
                    if is_loc_file(s):          # Location 的 file ⇒ 泄漏，单列（默认不进打分）
                        fe["strs_loc"].add(h)
    if verbose:
        sys.stderr.write("[fp] %-24s %6d 函数, ninsn=%d, 引用字符串的函数 %d"
                         "（其中引用 Rust 源文件路径的 %d ⇒ 泄漏列 strs_loc）\n"
                         % (os.path.basename(obj.path), len(universe),
                            sum(f["ninsn"] for f in feats.values()),
                            sum(1 for f in feats.values() if f["strs"]),
                            sum(1 for f in feats.values() if f["strs_loc"])))
    return feats


ARM64_BARRIER = {"bl", "blr", "br", "ret", "cbz", "cbnz", "tbz", "tbnz", "b"}


def scan_arm64(obj, universe, locs, verbose=True):
    from capstone import Cs, CS_ARCH_ARM64
    from capstone.arm64 import ARM64_OP_IMM, ARM64_OP_MEM, ARM64_OP_REG

    cs = Cs(CS_ARCH_ARM64, 0)
    cs.detail = True
    WRITE = 2
    tex = obj.sec[".text"]
    code = obj.read(tex["addr"], tex["size"])
    starts = sorted(universe)
    hit = T.LocHit(locs)
    ro = {n: obj.sec[n] for n in STR_SECTIONS if n in obj.sec}
    ro_lo = min((s["addr"] for s in ro.values()), default=0)
    ro_hi = max((s["addr"] + s["size"] for s in ro.values()), default=0)

    def sec_name(va):
        for n, s in ro.items():
            if s["addr"] <= va < s["addr"] + s["size"]:
                return n
        return None

    feats = {va: dict(ninsn=0, hist=collections.Counter(), cls=collections.Counter(),
                      consts=set(), calls=[], icalls=0, strs=set(), strs_loc=set(),
                      locrefs=set(), seq=[])
             for va in universe}
    addr = {}
    owner = None
    for insn in disasm_all(cs, code, tex["addr"], tex["addr"] + tex["size"], 4):
        o = T.owner_of(starts, universe, insn.address)
        if o != owner:                        # 跨函数不继承寄存器状态（同 §37.3）
            addr = {}
            owner = o
        if owner is None:
            continue
        fe = feats[owner]
        mn = insn.mnemonic
        fe["ninsn"] += 1
        fe["hist"][mn] += 1
        fe["seq"].append(mn_id(mn))
        fe["cls"][cls_of("arm64", mn, insn)] += 1
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
            if t in universe:      # 同上：函数地址不进 consts
                continue
            if sec_name(t):
                s = string_at(obj, t, ro_lo, ro_hi)
                if s is not None:
                    h = hashlib.sha1(s).hexdigest()[:8]
                    fe["strs"].add(h)
                    if is_loc_file(s):          # 同 x86：Location 的 file 单列，默认不进打分
                        fe["strs_loc"].add(h)
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
            for op in ops:
                if op.type == ARM64_OP_REG and op.access & WRITE:
                    addr.pop(op.reg, None)
    if verbose:
        sys.stderr.write("[fp] %-24s %6d 函数, ninsn=%d, 引用字符串的函数 %d"
                         "（其中引用 Rust 源文件路径的 %d ⇒ 泄漏列 strs_loc）\n"
                         % (os.path.basename(obj.path), len(universe),
                            sum(f["ninsn"] for f in feats.values()),
                            sum(1 for f in feats.values() if f["strs"]),
                            sum(1 for f in feats.values() if f["strs_loc"])))
    return feats


# ===========================================================================
# 3. 一份构建 = 一个 FP（指纹库）
# ===========================================================================
def cache_dir():
    d = os.environ.get("CFGFP_CACHE") or os.path.join(ROOT, ".agent-work/cfgfp")
    os.makedirs(d, exist_ok=True)
    return d


class FP:
    """一份构建的全部指纹。`feats[va]` 的字段见模块 docstring。

    两趟构造（**顺序不能换**）：先把 `size` / `bsig` 给所有函数定下来，再算被调者多重集，
    否则「被调者在字典里还没轮到」时 `size` 是缺的（第一版就死在这）。
    """

    def __init__(self, tag, kind, isa, path, obj, universe, feats, locs, has_names):
        self.tag, self.kind, self.isa, self.path = tag, kind, isa, path
        self.obj, self.universe, self.feats = obj, universe, feats
        self.locs = locs
        self.starts = sorted(universe)
        for va, f in universe.items():
            fe = feats[va]
            fe["size"] = max(0, f["end"] - va)
            body = obj.read(va, fe["size"]) if 0 < fe["size"] <= (1 << 20) else None
            fe["bsig"] = hashlib.sha1(body).hexdigest()[:12] if body else None
        for va in universe:
            fe = feats[va]
            calls = []
            for c in fe["calls"]:
                a = T.owner_of(self.starts, universe, c)
                if a is not None:
                    calls.append(a)
            fe["callees"] = tuple(sorted(calls))
            fe["callee_sizes"] = tuple(sorted(feats[a]["size"] for a in fe["callees"]))
            fe["callee_bsig"] = tuple(sorted(feats[a]["bsig"] or "-" for a in fe["callees"]))
            # `strs` 一分为二（`docs/159` §3.2 的泄漏）：
            #   strs_loc = 引用到的**Rust 源文件路径**（= panic::Location 的 file 字段投影）
            #   strs_s   = 其余字符串（log 文案、贴图名、格式化模板…）——**默认只用这一份**
            #   strs     = 原始并集（含泄漏），只为复现消融表而留，默认权重 0
            loc = set(fe.get("strs_loc") or ())
            allstr = set(fe["strs"])
            fe["strs"] = tuple(sorted(allstr))
            fe["strs_loc"] = tuple(sorted(allstr & loc))
            fe["strs_s"] = tuple(sorted(allstr - loc))
            fe["strs_set"] = frozenset(allstr)
            fe["strs_s_set"] = frozenset(fe["strs_s"])
            fe["hist_hash"] = stable_hash(fe["hist"].items())
            fe["seq"] = b"".join(struct.pack("<H", v) for v in fe.pop("seq", ()))
            fe["seq_hash"] = hashlib.sha1(fe["seq"]).hexdigest()[:16]
            fe["grams"] = frozenset(_trigrams(fe["seq"]))
        self.callers = collections.defaultdict(list)
        for va in feats:
            for c in feats[va]["callees"]:
                self.callers[c].append(va)
        for va in feats:
            cs = sorted(self.callers.get(va, ()))
            feats[va]["callers"] = tuple(cs)
            feats[va]["caller_sizes"] = tuple(sorted(feats[a]["size"] for a in cs))
            feats[va]["caller_bsig"] = tuple(sorted(feats[a]["bsig"] or "-" for a in cs))
        self.names = {}
        if has_names:
            for va, f in universe.items():
                if f.get("name"):
                    self.names[va] = f["name"]

    def state(self):
        return dict(tag=self.tag, kind=self.kind, isa=self.isa, path=self.path,
                    universe=self.universe, feats=self.feats, locs=self.locs,
                    starts=self.starts, names=self.names)

    @classmethod
    def from_state(cls, st):
        s = cls.__new__(cls)
        s.tag, s.kind, s.isa, s.path = st["tag"], st["kind"], st["isa"], st["path"]
        s.universe, s.feats, s.locs = st["universe"], st["feats"], st["locs"]
        s.starts, s.names = st["starts"], st["names"]
        s.obj = None
        s.callers = None
        return s


def load_fp(tag, force=False, verbose=True):
    cfg = BUILDS[tag]
    try:
        size = os.path.getsize(cfg["path"])
    except OSError as e:
        T.die("读不到 %s：%s" % (cfg["path"], e))
    key = "%s-%d-%s.pkl" % (tag, size, CACHE_VERSION)
    cp = os.path.join(cache_dir(), key)
    if os.path.exists(cp) and not force:
        with open(cp, "rb") as fh:
            st = pickle.load(fh)
        probe = next(iter(st["feats"].values()), None)
        if probe is not None and "strs_s" not in probe:
            T.die("%s 是旧口径缓存（特征里没有 strs_s）——抬 CACHE_VERSION 或换 CFGFP_CACHE" % cp)
        return FP.from_state(st)
    if cfg["kind"] == "pe":
        obj = T.Pe(cfg["path"])
        universe = T.pe_universe(obj)
        locs = obj.locations(verbose)
        feats = scan_x86(obj, universe, locs, verbose)
    else:
        obj = SM.Elf(cfg["path"])
        universe = T.elf_universe(obj, FUNCSTARTS if cfg["funcstarts"] else None)
        locs = T.elf_locations(obj, verbose)
        feats = (scan_x86 if obj.em == 62 else scan_arm64)(obj, universe, locs, verbose)
    fp = FP(tag, cfg["kind"], cfg["isa"], cfg["path"], obj, universe, feats,
            locs, cfg["names"])
    with open(cp, "wb") as fh:
        pickle.dump(fp.state(), fh, protocol=4)
    return fp


def load_all(force=False, tags=None):
    """按 tag 取指纹库；同进程内**缓存复用**（进程池 fork 出来的子进程直接共享父进程这份）。

    为什么要缓存：`runall` 的 8 个分片 worker 各自会 `load_all()`，没有这一层就是 8 次
    15 s 的 pickle 反序列化；fork 之后这条缓存是现成的（只读，不写回）。
    """
    key = ("all" if not tags else ",".join(tags), bool(force))
    if not force and key in _FPS_CACHE:
        return _FPS_CACHE[key]
    out = collections.OrderedDict()
    for tag in (tags or BUILDS):
        out[tag] = load_fp(tag, force)
    _FPS_CACHE[key] = out
    return out


_FPS_CACHE = {}


# ===========================================================================
# 4. 打分：**不读名字 / 不读地址 / 不读 Location**，只用结构
# ===========================================================================
# 权重：`tune3` 在**调参半**上选出，留出半只报数（见 docs/02 §35.3 / §35.6）。
# 0 = 该特征关掉；**没写的键也是 0**（`score_same` 用 `.get`），所以"哪些特征参与"一目了然。
#
# ⚠ 四处与发布版（`c1183d29` / 第 38 轮）不同，都是 `docs/159` §3.2 的直接整改：
#   * `strs`（**原始**字符串集）默认 **0** —— 它其实等于 `panic::Location` 的 `file` 字段：
#     正对照 348/348 带自己的 `src/*.rs` 串，目标集合 0/12,736 带 ⇒ 是泄漏，不是结构。
#     留着这个键只是为了 `ablate` 能复现消融表（`W_SAME_LEAK`）。
#   * `strs_s`（**去掉一切 Rust 源文件路径**之后的字符串）是替代品：扫描时把
#     `src/*.rs` / `/rustc/<hash>/library/.../*.rs` / `.cargo/.../*.rs` 全部记进 `strs_loc`
#     并从 `strs_s` 里剔除（它们全是 Location 投影）。剔除后两侧覆盖率重新对齐：
#     正对照 94/348 = 27.0% vs 目标集合 2,201/12,736 = 17.3%（`ablate --coverage` 打印）
#     —— 比"原始 strs 100% vs 0%"对称得多，但实测它不出力，所以权重仍是 0。
#   * `hist`(F2) 在最终配置里权重 **0**：`docs/159` §3.2 实测清零后两半 top-1 一格不动
#     （gram 的三元组集合已经把它的信息吃完）⇒ 留着权重只会让人误以为它有贡献。
#   * `consts` 默认小权重（不是 0）：审计 §3.2 只证实"3/348 的 Location 行号落进 consts"，
#     不构成系统性泄漏；它的收益由 `ablate` 的两半数字决定（见 docs/02 §35.3）。
W_SAME = dict(size=0.25, hist=0.0, gram=2.0, consts=0.25, caller=0.5, strs_s=0.0,
              strs=0.0, callee=0.0, cbsig=0.0, rbsig=0.0, ncal=0.0, ncall=0.0)
# 发布过的旧配置（`docs/02` §35.3 的 84.2%/89.8% 就是它）。**含泄漏**，
# 只用于 `ablate` 的"清零前后"对照；任何新数字都不许用它。
W_SAME_LEAK = dict(size=1.0, hist=0.5, gram=2.0, consts=0.5, caller=0.3, strs=0.2,
                   strs_s=0.0, callee=0.0, cbsig=0.0, rbsig=0.0, ncal=0.0, ncall=0.0)
W_CROSS = dict(size=1.0, cls=0.0, callee=0.0, caller=0.5, ncall=0.3, ncal=0.0)
SCORE_KEYS = ("size", "hist", "gram", "consts", "callee", "caller", "strs", "strs_s",
              "ncal", "ncall", "cbsig", "rbsig", "cls")
SIZE_EPS = 0.4          # log 比的饱和点（同 §40）
WIN_SAME = (0.7, 1.4)   # 候选窗口：保住真配对 97.0%，候选数中位 25,147 → 6,806
WIN_CROSS = (0.7, 1.4)  # 跨 ISA 保住 96.4%（真配对 size 比 p05/p95 = 0.78/1.28）
MARGIN_TIERS = (0.0, 0.02, 0.05, 0.10, 0.20)


def sim_log(a, b, eps=SIZE_EPS):
    return 1.0 - min(abs(math.log((b + 1.0) / (a + 1.0))) / eps, 1.0)


def cos_hist(a, b):
    if not a or not b:
        return 0.0
    num = 0
    for k, v in a.items():
        bv = b.get(k)
        if bv:
            num += v * bv
    if not num:
        return 0.0
    da = math.sqrt(sum(v * v for v in a.values()))
    db = math.sqrt(sum(v * v for v in b.values()))
    return num / (da * db) if da and db else 0.0


def jaccard_sets(a, b):
    """两个多重集（已排序 tuple）按**集合**算 Jaccard：ICF 归一后的调用图身份。"""
    if not a or not b:
        return 0.0
    sa, sb = set(a), set(b)
    return len(sa & sb) / float(len(sa | sb))


def jaccard(a, b):
    if not a or not b:
        return 0.0
    return len(a & b) / float(len(a | b))


def multiset_sim(qs, ds):
    """有序多重集相似度（按 rank 配对，用 log 比）：跨构建、跨 ISA 都能用的"形状"比较。

    为什么不用「多重集相等」：ICF 合并与内联决策在两份构建里不同，一个被调者可能
    在另一侧被合并/内联掉，要求完全相等就把真配对判死。按 rank 配对只要求**分布像**。
    """
    if not qs or not ds:
        return 0.0
    n = min(len(qs), len(ds))
    pair = sum(sim_log(qs[i], ds[i]) for i in range(n)) / n
    cnt = 1.0 - min(abs(math.log((len(ds) + 1.0) / (len(qs) + 1.0))) / SIZE_EPS, 1.0)
    return 0.5 * pair + 0.5 * cnt


def score_same(qf, df, w=None):
    """同 ISA（Linux x86-64 ↔ Windows x86-64）：尺寸 + **原始助记符**直方图 + 常量 + 邻域。"""
    w = w or W_SAME
    tot = wt = 0.0
    def add(weight, val):
        nonlocal tot, wt
        if weight and val is not None:
            tot += weight * val
            wt += weight
    add(w.get("size"), sim_log(qf["size"], df["size"]))
    add(w.get("hist"), cos_hist(qf["hist"], df["hist"]))
    add(w.get("consts"), jaccard(qf["consts"], df["consts"]) if (qf["consts"] and df["consts"]) else None)
    add(w.get("callee"), multiset_sim(qf["callee_sizes"], df["callee_sizes"]))
    add(w.get("caller"), multiset_sim(qf["caller_sizes"], df["caller_sizes"]))
    add(w.get("strs_s"), jaccard(qf["strs_s_set"], df["strs_s_set"])
        if (qf["strs_s"] and df["strs_s"]) else None)
    add(w.get("strs"), jaccard(qf["strs_set"], df["strs_set"])
        if (qf["strs"] and df["strs"]) else None)
    add(w.get("ncal"), sim_log(len(qf["callees"]), len(df["callees"])) if
        (qf["callees"] or df["callees"]) else None)
    add(w.get("ncall"), sim_log(len(qf["callers"]), len(df["callers"])) if
        (qf["callers"] or df["callers"]) else None)
    add(w.get("cbsig"), jaccard_sets(qf["callee_bsig"], df["callee_bsig"]))
    add(w.get("rbsig"), jaccard_sets(qf["caller_bsig"], df["caller_bsig"]))
    add(w.get("gram"), jaccard(qf["grams"], df["grams"]) if (qf["grams"] and df["grams"]) else None)
    # `cls` 是 14 类归一后的直方图（跨 ISA 才用的那一层）。同 ISA 也能用，而且比原始助记符稳：
    # 两份 LLVM 构建的差别（`int3`↔`ud2`、SEH vs DWARF 的 push/pop、寄存器分配）正好落在助记符层，
    # 类直方图把它们吸收掉。**只在调参半上验过**（tune3），留出半只报数。
    add(w.get("cls"), cos_hist(qf["cls"], df["cls"]))
    return tot / wt if wt else 0.0


def score_cross(qf, df, w=None):
    """跨 ISA（Linux x86-64 ↔ Android arm64）：**类**直方图 + 邻域（多重集），不碰偏移/立即数。"""
    w = w or W_CROSS
    tot = wt = 0.0
    def add(weight, val):
        nonlocal tot, wt
        if weight and val is not None:
            tot += weight * val
            wt += weight
    add(w.get("size"), sim_log(qf["size"], df["size"]))
    add(w.get("cls"), cos_hist(qf["cls"], df["cls"]))
    add(w.get("callee"), multiset_sim(qf["callee_sizes"], df["callee_sizes"]))
    add(w.get("caller"), multiset_sim(qf["caller_sizes"], df["caller_sizes"]))
    add(w.get("ncal"), sim_log(len(qf["callees"]), len(df["callees"])) if
        (qf["callees"] or df["callees"]) else None)
    add(w.get("ncall"), sim_log(len(qf["callers"]), len(df["callers"])) if
        (qf["callers"] or df["callers"]) else None)
    return tot / wt if wt else 0.0


def scorer_for(src_tag, dst_tag, w=None):
    same = BUILDS[src_tag]["isa"] == BUILDS[dst_tag]["isa"]
    if same:
        return (lambda q, d: score_same(q, d, w)), (w or W_SAME), WIN_SAME
    return (lambda q, d: score_cross(q, d, w)), (w or W_CROSS), WIN_CROSS


# ===========================================================================
# 5. 排名：**候选池 = 整份构建**（不是"有名函数池"）
# ===========================================================================
class Pool:
    """按尺寸排序的全函数池 + 尺寸窗口候选生成。"""

    def __init__(self, fp):
        self.fp = fp
        items = sorted((fp.feats[va]["size"], va) for va in fp.universe)
        self.sizes = [s for s, _ in items]
        self.vas = [v for _, v in items]

    def window(self, size, win):
        lo = max(8, int(size * win[0]))
        hi = max(lo + 1, int(size * win[1]))
        return self.vas[bisect.bisect_left(self.sizes, lo):bisect.bisect_right(self.sizes, hi)]


def rank(qf, pool, score, win, k=5, cands=None):
    cands = pool.window(qf["size"], win) if cands is None else cands
    sc = [(score(qf, pool.fp.feats[c]), c) for c in cands]
    sc.sort(key=lambda t: (-t[0], t[1]))
    return sc[:k]


def margin_of(ranked):
    if not ranked:
        return 0.0
    return ranked[0][0] - (ranked[1][0] if len(ranked) > 1 else 0.0)


# ===========================================================================
# 6. 正对照的真值：`Location` 双向唯一锚定（编译期常量，与打分器无关）
# ===========================================================================
SRC_PREFIX = "src/"


def loc_keys(fp, va, src_only=True):
    ks = {fp.locs[l] for l in fp.feats[va]["locrefs"] if l in fp.locs}
    if src_only:
        ks = {k for k in ks if k[0].startswith(SRC_PREFIX)}
    return ks


def named_linux():
    """348 个「已定名」的 Linux 地址（§40.1：三份夹具并集 ∩ 527）——**只做正对照的测试集**。"""
    def rows(name):
        p = os.path.join(FIXTURES, name)
        hdr, out = None, []
        if not os.path.exists(p):
            return out
        with open(p, encoding="utf-8") as fh:
            for line in fh:
                line = line.rstrip("\n")
                if not line or line.startswith("#"):
                    continue
                p2 = line.split("\t")
                if hdr is None:
                    hdr = p2
                    continue
                out.append(dict(zip(hdr, p2)))
        return out
    n = set()
    for r in rows("name_to_addr.tsv"):
        if r.get("x86_addr", "").startswith("0x"):
            n.add(int(r["x86_addr"], 16))
    for r in rows("name_to_addr_v2.tsv"):
        if r.get("x86_addr", "").startswith("0x"):
            n.add(int(r["x86_addr"], 16))
    for r in rows("windows_name_to_addr.tsv"):
        if r.get("join_kind") not in ("唯一锚", "集合锚"):
            continue
        if r.get("linux_addr", "").startswith("0x"):
            n.add(int(r["linux_addr"], 16))
    return n


def named_linux_348():
    """§40.1 的 **348**：三份夹具的名字并集 ∩ 527（527 = 引用 `src/*.rs` Location 的函数）。"""
    lin = load_fp("linux")
    return {a for a in named_linux() if loc_keys(lin, a)}


def fixture_pairs_win():
    """Linux ↔ Windows 2.0.60 的**已有真值**：`name_to_addr_v2.tsv` evidence 的 `win_addr=`。

    这些对是 §37/§40 用 `Location` 双向唯一锚定出来的（并经 §40.3 的正对照验证过），
    本节**只读不重算** —— 自己重新实现一遍 join 只会引入第二套口径（§40.3 就是这么踩的）。
    同一 Linux 地址对应多个 Windows 地址的（`0x3a2e10` 等）**丢掉**：真值必须是 1:1。
    """
    by = collections.defaultdict(set)
    path = os.path.join(FIXTURES, "name_to_addr_v2.tsv")
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("#") or not line.strip():
                continue
            p2 = line.split("\t")
            if len(p2) < 10 or not p2[3].startswith("0x"):
                continue
            m = re.search(r"win_addr=(0x[0-9a-f]+)", p2[9])
            if m:
                by[int(p2[3], 16)].add(int(m.group(1), 16))
    return {lin: next(iter(w)) for lin, w in by.items() if len(w) == 1}


def strict_pairs(src_fp, dst_fp, src_only=True):
    """严格版 `Location` join：**每个键**两侧各唯一，且正向/反向都指向同一个函数。

    **为什么不能用"键集合的并"那种松 join**（本工具第一版就是那么写的，那一版所有数字作废）：
    松 join 只看"键集合的并"落到几个函数上，一个被很多函数共享的 `Location`（内联的
    `expect()`）就能把整对带偏 ——
    实测它把 Linux `0x3a2e10`（**9,936 B / 1,848 条指令**）对到了 Windows 的
    **1,019 B** 函数上，体量比 0.10，肉眼就知道错。本函数要求：
    ① 该键在 dst 侧只被一个函数引用；② 该键在 src 侧也只被一个函数引用；
    ③ 这个 src 函数的所有"唯一键"必须指向**同一个** dst 函数；④ 反之亦然。
    """
    dk = collections.defaultdict(set)
    for va in dst_fp.universe:
        for k in loc_keys(dst_fp, va, src_only):
            dk[k].add(va)
    sk = collections.defaultdict(set)
    for va in src_fp.universe:
        for k in loc_keys(src_fp, va, src_only):
            sk[k].add(va)
    u_dst = {k: next(iter(v)) for k, v in dk.items() if len(v) == 1}
    u_src = {k: next(iter(v)) for k, v in sk.items() if len(v) == 1}
    fwd = {}
    for sva in src_fp.universe:
        tgt = {u_dst[k] for k in loc_keys(src_fp, sva, src_only) if k in u_dst}
        if len(tgt) == 1:
            fwd[sva] = next(iter(tgt))
    out = {}
    for sva, dva in fwd.items():
        back = {u_src[k] for k in loc_keys(dst_fp, dva, src_only) if k in u_src}
        if back == {sva}:
            out[dva] = sva
    return out


def control_pairs(src_tag, dst_tag, fps=None):
    """正对照的真值对 {dst(Linux) 地址: src 地址}。

    * Linux↔Windows：**夹具真值 ∪ 严格 join**。两者重叠的 295 个地址**100% 一致**
      （实测，见 `verify` 打印），所以并集是安全的；并集比单用哪个都大。
    * 其余（Android / early2026）：只能用严格 join —— 这两份构建没有"地址级"夹具。
    """
    fps = fps or load_all()
    src, dst = fps[src_tag], fps[dst_tag]
    st = strict_pairs(src, dst)                       # dst -> src
    if (src_tag, dst_tag) == ("win", "linux"):
        fx = fixture_pairs_win()                      # linux -> win
        conflict = [l for l in set(fx) & set(st) if fx[l] != st[l]]
        out = dict(st)
        out.update(fx)                                # 夹具优先（项目已验证过）
        return out, dict(fixture=len(fx), strict=len(st),
                         both=len(set(fx) & set(st)),
                         agree=len(set(fx) & set(st)) - len(conflict),
                         conflict=len(conflict))
    return st, dict(strict=len(st))


def split_half_addrs(addrs):
    """按**地址** sha1 奇偶分半（地址不在特征里，所以与"难不难"无关）。

    不用名字分半：名字键有两套 `parse_name`（§40.3 的教训），会让两半的构成
    随解析口径漂移。
    """
    a, b = [], []
    for va in sorted(addrs):
        h = hashlib.sha1(("%x" % va).encode()).digest()[0]
        (a if h & 1 else b).append(va)
    return a, b


# ===========================================================================
# 7. 子命令
# ===========================================================================
def cmd_build(args):
    fps = load_all(force=args.force)
    W = sys.stdout.write
    W("%-9s %8s %10s %8s %8s %8s %8s\n"
      % ("构建", "函数数", "有指纹", "有被调", "有字符串", "ICF组", "Location"))
    for tag, fp in fps.items():
        feats = fp.feats
        icf = collections.Counter(f["bsig"] for f in feats.values() if f["bsig"])
        dup = sum(1 for _b, c in icf.items() if c > 1)
        W("%-9s %8d %10d %8d %8d %8d %8d\n"
          % (tag, len(fp.universe), sum(1 for f in feats.values() if f["ninsn"]),
             sum(1 for f in feats.values() if f["callees"]),
             sum(1 for f in feats.values() if f["strs"]),
             dup, len(fp.locs)))
    return 0


def fmt_pct(a, b):
    return "%5.1f%%" % (100.0 * a / b) if b else "  n/a"


def cmd_verify(args):
    fps = load_all()
    W = sys.stdout.write
    named = named_linux_348()
    W("已定名测试集（§40.1 的 348 口径，= 三份夹具并集 ∩ 527）：%d 个 Linux 地址\n" % len(named))
    for dst_tag, src_tag in (("linux", "win"), ("linux", "android"), ("linux", "win_old")):
        dst, src = fps[dst_tag], fps[src_tag]
        wgt = weights_for(args, src_tag, dst_tag)
        sc, ww, win = scorer_for(src_tag, dst_tag, wgt)
        pool = Pool(src)
        truth, prov = control_pairs(src_tag, dst_tag, fps)
        W("\n=== %s → %s（%s） ===\n" % (dst_tag, src_tag,
              "同 ISA" if BUILDS[src_tag]["isa"] == BUILDS[dst_tag]["isa"] else "跨 ISA"))
        W("权重：%s\n" % " ".join("%s=%.2g" % (k, ww.get(k, 0.0)) for k in SCORE_KEYS
                                   if ww.get(k)) or "（空）")
        W("真值来源：%s\n" % ", ".join("%s=%d" % kv for kv in prov.items()))
        testable = [a for a in sorted(named) if a in truth and truth[a] in src.universe]
        W("测试集：%d 个（候选池 = %s 的 %d 个函数，**不筛名字**）\n"
          % (len(testable), src_tag, len(src.universe)))
        A, B = split_half_addrs(testable)
        for lab, half in (("调参半", A), ("留出半", B)):
            rows = []
            for a in half:
                sva = truth[a]
                r = rank(dst.feats[a], pool, sc, win, args.topk)
                rows.append(dict(ok1=bool(r) and r[0][1] == sva,
                                 okk=any(c == sva for _s, c in r),
                                 margin=margin_of(r), score=r[0][0] if r else 0.0,
                                 ncand=len(pool.window(dst.feats[a]["size"], win))))
            n = len(rows)
            t1 = sum(1 for r in rows if r["ok1"])
            tk = sum(1 for r in rows if r["okk"])
            nc = sorted(r["ncand"] for r in rows)
            med = nc[len(nc) // 2] if nc else 0
            W("  %s n=%3d top-1=%s top-%d=%s | score 中位 %.3f | 候选数中位 %d（随机基线 ≈ %.3f%%）\n"
              % (lab, n, fmt_pct(t1, n), args.topk, fmt_pct(tk, n),
                 sorted(r["score"] for r in rows)[n // 2] if nc else 0, med,
                 100.0 / max(1, med)))
    return 0


def target_addrs(fps):
    """没有 `src/*.rs` Location 的 Linux 函数（= 名字要不回来的那批）。"""
    lin = fps["linux"]
    out = [va for va in sorted(lin.universe) if not loc_keys(lin, va)]
    return out


CONFIGS = [
    ("size+hist", dict(size=1.0, hist=1.0)),
    ("size+gram", dict(size=1.0, gram=1.0)),
    ("size+hist+gram", dict(size=1.0, hist=1.0, gram=1.0)),
    ("size+hist+consts", dict(size=1.0, hist=1.0, consts=0.5)),
    ("size+hist+callee", dict(size=1.0, hist=1.0, callee=0.5)),
    ("size+hist+caller", dict(size=1.0, hist=1.0, caller=0.5)),
    ("size+hist+strs", dict(size=1.0, hist=1.0, strs=0.5)),
    ("size+hist+cbsig", dict(size=1.0, hist=1.0, cbsig=0.5)),
    ("size+hist+rbsig", dict(size=1.0, hist=1.0, rbsig=0.5)),
    ("size+hist+gram+callee+caller", dict(size=1.0, hist=1.0, gram=1.0, callee=0.3,
                                          caller=0.2)),
    ("size+hist+gram+cbsig+rbsig", dict(size=1.0, hist=1.0, gram=1.0, cbsig=0.5,
                                        rbsig=0.3)),
    ("全特征", dict(size=1.0, hist=1.0, gram=1.0, consts=0.5, callee=0.3, caller=0.2,
                   strs=0.2, cbsig=0.5, rbsig=0.3)),
    ("全特征+计数", dict(size=1.0, hist=1.0, gram=1.0, consts=0.5, callee=0.3, caller=0.2,
                       strs=0.2, cbsig=0.5, rbsig=0.3, ncal=0.3, ncall=0.3)),
    ("gram 主导", dict(size=1.0, hist=0.5, gram=2.0, consts=0.5, caller=0.3, strs=0.2)),
    ("gram+邻域", dict(size=1.0, gram=1.5, caller=0.5, ncall=0.3, consts=0.3)),
    ("去掉 callee(有害)", dict(size=1.0, hist=1.0, gram=1.0, consts=0.5, caller=0.2,
                          strs=0.2, cbsig=0.5, rbsig=0.3, ncal=0.3, ncall=0.3)),
]
XTRA = [
    ("跨ISA:size+ncal", dict(size=1.0, ncal=1.0)),
    ("跨ISA:size+ncall", dict(size=1.0, ncall=1.0)),
    ("跨ISA:size+cls+ncal", dict(size=1.0, cls=1.0, ncal=0.5)),
]
GATES = ((1 / 3., 3.), (1 / 2., 2.), (0.7, 1.4), (0.8, 1.25))


def cmd_tune2(args):
    """**只在调参半**上：① 候选窗口的召回代价 ② 特征配置的 top-1。留出半不碰。"""
    fps = load_all()
    W = sys.stdout.write
    named = named_linux_348()
    pairs = [("linux", "win"), ("linux", "android")]
    for dst_tag, src_tag in pairs:
        dst, src = fps[dst_tag], fps[src_tag]
        truth, prov = control_pairs(src_tag, dst_tag, fps)
        A, B = split_half_addrs([a for a in sorted(named) if a in truth])
        pool = Pool(src)
        W("\n=== %s → %s 调参半 n=%d（留出半 %d 不参与调参）===\n"
          % (dst_tag, src_tag, len(A), len(B)))
        # ① 窗口召回代价（真配对落在窗口内的比例）
        ratios = [(src.feats[truth[a]]["size"] + 1.0) / (dst.feats[a]["size"] + 1.0) for a in A]
        W("  真配对 size 比：%s\n" % " ".join(
            "p%02d=%.2f" % (p, sorted(ratios)[min(len(ratios) - 1, int(len(ratios) * p / 100.0))])
            for p in (5, 25, 50, 75, 95)))
        for g in GATES:
            keep = sum(1 for r in ratios if g[0] <= r <= g[1])
            nc = sorted(len(pool.window(dst.feats[a]["size"], g)) for a in A)
            W("  窗口[%.2f,%.2f] 保住真配对 %d/%d=%s 候选数中位 %d\n"
              % (g[0], g[1], keep, len(ratios), fmt_pct(keep, len(ratios)),
                 nc[len(nc) // 2] if nc else 0))
        gate = GATES[2]
        W("  以下用窗口[%.2f,%.2f]：\n" % gate)
        for label, w in CONFIGS:
            sc, _w, _win = scorer_for(src_tag, dst_tag,
                                      weights_override(w, args.weights))
            ok = tk = 0
            for a in A:
                r = rank(dst.feats[a], pool, sc, gate, args.topk)
                ok += bool(r) and r[0][1] == truth[a]
                tk += any(c == truth[a] for _s, c in r)
            W("    %-30s top-1=%s top-%d=%s\n"
              % (label, fmt_pct(ok, len(A)), args.topk, fmt_pct(tk, len(A))))
    return 0


SCORE_TIERS = (0.40, 0.50, 0.55, 0.60, 0.65, 0.70, 0.75, 0.80, 0.85, 0.90)


def cmd_calib(args):
    """**只在调参半**上定分数门槛，然后把同一个门槛拿到留出半报数（合并纪律 A4）。

    这里的 "准确率" = 该档位里的行，其 top-1 候选等于 `Location` 真值的比例
    —— 也就是"我说高置信同源时，有几次是对的"。
    """
    fps = load_all()
    W = sys.stdout.write
    named = named_linux_348()
    dst_tag, src_tag = "linux", "win"
    dst, src = fps[dst_tag], fps[src_tag]
    sc, ww, win = scorer_for(src_tag, dst_tag, weights_for(args, src_tag, dst_tag))
    pool = Pool(src)
    truth, prov = control_pairs(src_tag, dst_tag, fps)
    testable = [a for a in sorted(named) if a in truth]
    A, B = split_half_addrs(testable)
    W("真值来源：%s\n" % ", ".join("%s=%d" % kv for kv in prov.items()))
    W("分半：调参半 %d / 留出半 %d（按 Linux 地址 sha1 奇偶）\n\n" % (len(A), len(B)))

    def eval_half(half):
        rows = []
        for a in half:
            sva = truth[a]
            r = rank(dst.feats[a], pool, sc, win, args.topk)
            rows.append(dict(addr=a, src=sva, ok=bool(r) and r[0][1] == sva,
                             score=r[0][0] if r else 0.0, margin=margin_of(r),
                             okk=any(c == sva for _s, c in r)))
        return rows

    RA, RB = eval_half(A), eval_half(B)
    W("%-6s | %-28s | %-28s\n" % ("门槛", "调参半 n / top-1 准确率", "留出半 n / top-1 准确率"))
    W("%-6s | %-28s | %-28s\n" % ("score", "n   精度    覆盖", "n   精度    覆盖"))
    chosen = None
    for thr in SCORE_TIERS:
        sa = [r for r in RA if r["score"] >= thr]
        sb = [r for r in RB if r["score"] >= thr]
        pa = sum(1 for r in sa if r["ok"]) / float(len(sa)) if sa else 0.0
        pb = sum(1 for r in sb if r["ok"]) / float(len(sb)) if sb else 0.0
        W("%-6.2f | %3d %s %s | %3d %s %s\n"
          % (thr, len(sa), fmt_pct(sum(1 for r in sa if r["ok"]), len(sa)),
             fmt_pct(len(sa), len(RA)), len(sb),
             fmt_pct(sum(1 for r in sb if r["ok"]), len(sb)), fmt_pct(len(sb), len(RB))))
        if chosen is None and pa >= 0.95 and len(sa) >= 40:
            chosen = (thr, pa, len(sa), pb, len(sb))
    W("\n按「调参半精度>=95%% 且 n>=40」选门槛：score>=%.2f\n" % (chosen[0] if chosen else 0.0))
    if chosen:
        W("  调参半 n=%d 精度=%s；**留出半** n=%d 精度=%s\n"
          % (chosen[2], fmt_pct(chosen[1] * chosen[2], chosen[2]),
             chosen[4], fmt_pct(chosen[3] * chosen[4], chosen[4])))
    W("\n逐档精度（相邻门槛之间那一档，调参半 / 留出半）：\n")
    edges = SCORE_TIERS + (1.01,)
    for i in range(len(edges) - 1):
        lo, hi = edges[i], edges[i + 1]
        sa = [r for r in RA if lo <= r["score"] < hi]
        sb = [r for r in RB if lo <= r["score"] < hi]
        W("  [%.2f,%.2f) 调参半 n=%3d 精度=%s | 留出半 n=%3d 精度=%s\n"
          % (lo, hi, len(sa), fmt_pct(sum(1 for r in sa if r["ok"]), len(sa)),
             len(sb), fmt_pct(sum(1 for r in sb if r["ok"]), len(sb))))
    # margin 是否还有额外增量（在选出的 score 门槛之上）
    thr = chosen[0] if chosen else 0.7
    W("\n在 score>=%.2f 之上再叠 margin 闸门：\n" % thr)
    for mthr in MARGIN_TIERS:
        sa = [r for r in RA if r["score"] >= thr and r["margin"] >= mthr]
        sb = [r for r in RB if r["score"] >= thr and r["margin"] >= mthr]
        W("  margin>=%.2f 调参半 n=%3d 精度=%s | 留出半 n=%3d 精度=%s\n"
          % (mthr, len(sa), fmt_pct(sum(1 for r in sa if r["ok"]), len(sa)),
             len(sb), fmt_pct(sum(1 for r in sb if r["ok"]), len(sb))))
    # 按体量分桶（说明这个准确率是对哪一类函数测的）
    W("\n按函数体积分桶（调参半；解释「这个准确率能不能搬到 12,736」）：\n")
    buckets = ((0, 64), (64, 256), (256, 1024), (1024, 4096), (4096, 1 << 30))
    for lo, hi in buckets:
        sel = [r for r in RA if lo <= dst.feats[r["addr"]]["size"] < hi]
        W("  size[%5d,%6d) n=%3d top-1=%s\n"
          % (lo, hi, len(sel), fmt_pct(sum(1 for r in sel if r["ok"]), len(sel))))
    W("\n候选档位规则（在调参半上比，留出半报数）：\n")
    rules = [
        ("R1 score>=0.80 & margin>=0.05", lambda r: r["score"] >= 0.80 and r["margin"] >= 0.05),
        ("R2 R1 & size>=256", lambda r: (r["score"] >= 0.80 and r["margin"] >= 0.05
                                        and dst.feats[r["addr"]]["size"] >= 256)),
        ("R3 score>=0.80 & size>=256", lambda r: (r["score"] >= 0.80
                                                 and dst.feats[r["addr"]]["size"] >= 256)),
        ("R4 score>=0.65 & margin>=0.05", lambda r: r["score"] >= 0.65 and r["margin"] >= 0.05),
        ("R5 score>=0.65", lambda r: r["score"] >= 0.65),
        ("R6 score>=0.65 & size>=256", lambda r: (r["score"] >= 0.65
                                                 and dst.feats[r["addr"]]["size"] >= 256)),
        ("R7 score>=0.50 & margin>=0.05", lambda r: r["score"] >= 0.50 and r["margin"] >= 0.05),
        ("R8 score>=0.50", lambda r: r["score"] >= 0.50),
    ]
    for label, fn in rules:
        sa = [r for r in RA if fn(r)]
        sb = [r for r in RB if fn(r)]
        W("  %-32s 调参半 n=%3d 精度=%s | 留出半 n=%3d 精度=%s\n"
          % (label, len(sa), fmt_pct(sum(1 for r in sa if r["ok"]), len(sa)),
             len(sb), fmt_pct(sum(1 for r in sb if r["ok"]), len(sb))))
    # 单射闸门：一个 Windows 函数被同一半里多个 Linux 认领 ⇒ 全部降档（保守）
    for lab, rows in (("调参半", RA), ("留出半", RB)):
        claim = collections.Counter(r["src"] for r in rows if r["score"] >= 0.65
                                    and r["margin"] >= 0.05)
        hi = [r for r in rows if r["score"] >= 0.65 and r["margin"] >= 0.05]
        keep = [r for r in hi if claim[r["src"]] == 1]
        W("  单射闸门（%s）：多认领 %d 行、保留 %d 行，保留行精度=%s\n"
          % (lab, len(hi) - len(keep), len(keep),
             fmt_pct(sum(1 for r in keep if r["ok"]), len(keep))))
        W("    ↑ 这一档在正对照上一直是 **0 行**（真值 1:1），所以它从没被真正测过；"
          "证明闸门会开火见下。\n")
    # 负对照：正对照里真值互不重复 ⇒ 单射闸门永远是 0 行，测不出它是否有效
    # （`docs/159` §6：片内计数让它在 12,736 上漏了 25 组 / 56 行）。用合成用例证明它会开火、
    # 也不会乱开火；走的是 `run`/`merge` 用的**同一个** apply_gates。
    W("  单射闸门负对照（合成用例，走 merge/run 同一个 apply_gates）：\n")
    def _g(spec):
        rs = [dict(target=t, score=s, margin=m, size=sz) for t, s, m, sz in spec]
        return [r["tier"] for r in apply_gates(rs)]
    cases = [
        ("2 行抢同一目标（都够高置信）", [(0x180001000, .90, .20, 512)] * 2,
         [TIER_MID, TIER_MID]),
        ("2 行各抢各的目标", [(0x180001000, .90, .20, 512), (0x180002000, .90, .20, 512)],
         [TIER_HI, TIER_HI]),
        ("1 行高置信 + 1 行低分同靶", [(0x180001000, .90, .20, 512),
                                     (0x180001000, .45, .20, 512)],
         [TIER_HI, TIER_UNK]),
        ("1 行高置信 + 1 行同靶但 margin 不够", [(0x180001000, .90, .20, 512),
                                              (0x180001000, .90, .01, 512)],
         [TIER_HI, TIER_WEAK]),
    ]
    ok_all = True
    for lab, spec, want in cases:
        got = _g(spec)
        ok = got == want
        ok_all = ok_all and ok
        W("    %-30s → %-28s %s\n"
          % (lab, "/".join(got), "OK" if ok else "FAIL（期望 %s）" % "/".join(want)))
    W("    闸门%s：一个目标被 ≥2 行认领时必须全部降档，其余情况不许误伤\n"
      % ("有效" if ok_all else "**无效，先修闸门再看下面的数**"))
    sizes = sorted(dst.feats[a]["size"] for a in named)
    W("  已定名 348 个体量分布：中位 %d；12,736 目标见 report\n"
      % sizes[len(sizes) // 2])
    return 0


# ===========================================================================
# 7.5 无泄漏重训：消融 / 权重网格 / 分片并行（`docs/159` §3.2 的整改）
#
# 为什么要有这一段：审计查明发布的 84.2% 是**泄漏撑高**的 —— `strs`(F7) 就是
# `panic::Location` 的 `file` 字段（正对照 348/348 带自己的 `src/*.rs` 串，目标集合
# 0/12,736 带），于是"在看得见文件名的总体上的准确率"被当成了"在看不见的总体上的准确率"。
# 整改分三步，都能在本文件里复跑：
#   ① `ablate`：一个特征一个特征清零，**两半都报**（不用改默认权重也能看见它掉多少）；
#   ② `tune3`  ：**只在调参半**上跑权重网格（`--jobs N` 进程池横向并行），选完再在留出半报一次；
#   ③ `runall` ：8 片并行跑全量 12,736 + `merge`（`--jobs N`）。
# 硬约束：网格只在调参半上选，留出半**不参与**选择（`AGENTS.md` 合并纪律 A4）。
# 并行只用来缩短墙钟，**不改任何数字**：`--jobs 1` 与 `--jobs N` 的输出逐位相同
# （任务要求把这一条实测出来：同子集串行 vs 并行的墙钟与加速比）。
# ===========================================================================
def weights_for(args, src_tag, dst_tag):
    """`--weights` 覆盖到**这一对构建**的默认权重上；没给就是 None（= 用模块默认）。"""
    spec = getattr(args, "weights", "")
    if not spec:
        return None
    same = BUILDS[src_tag]["isa"] == BUILDS[dst_tag]["isa"]
    return weights_override(W_SAME if same else W_CROSS, spec)


def weights_override(base, spec):
    """在 `base` 上覆盖 `k=v,...`（没写的键保持 `base` 的值）。未知特征名直接 `die`。"""
    out = dict(base)
    for kv in (spec or "").replace(";", ",").split(","):
        kv = kv.strip()
        if not kv:
            continue
        k, _, v = kv.partition("=")
        k = k.strip()
        if k not in SCORE_KEYS:
            T.die("未知特征名 %r（可用：%s）" % (k, ", ".join(SCORE_KEYS)))
        out[k] = float(v)
    return out


def wlabel(w):
    """权重 → 紧凑标签（只列非 0 项，按 SCORE_KEYS 顺序）。"""
    return " ".join("%s%.2g" % (k, w[k]) for k in SCORE_KEYS if w.get(k))


class Pair:
    """一对构建的打分上下文：真值 / 分半 / 候选池。串行与进程池共享同一份（fork 继承）。"""

    def __init__(self, src_tag="win", dst_tag="linux", fps=None):
        self.src_tag, self.dst_tag = src_tag, dst_tag
        fps = fps or load_all(tags=[dst_tag, src_tag])
        self.fps = fps
        self.src, self.dst = fps[src_tag], fps[dst_tag]
        named = named_linux_348()
        self.truth, self.prov = control_pairs(src_tag, dst_tag, fps)
        self.testable = [a for a in sorted(named)
                         if a in self.truth and self.truth[a] in self.src.universe]
        self.A, self.B = split_half_addrs(self.testable)
        self.pool = Pool(self.src)

    def ev(self, w, addrs, topk=5):
        """返回 (top-1 命中数, top-k 命中数, n)。"""
        sc, _w, win = scorer_for(self.src_tag, self.dst_tag, w)
        ok1 = okk = 0
        for a in addrs:
            r = rank(self.dst.feats[a], self.pool, sc, win, topk)
            t = self.truth[a]
            ok1 += bool(r) and r[0][1] == t
            okk += any(c == t for _s, c in r)
        return ok1, okk, len(addrs)


_G = {}          # 进程池里由 fork 继承的只读上下文（pair / topk）


def _grid_eval(job):
    label, w = job
    ok1, okk, n = _G["pair"].ev(w, _G["pair"].A, _G["topk"])
    return dict(label=label, w=w, top1=ok1, top5=okk, n=n)


def _holdout_eval(job):
    """把一个（**已经用调参半选出来的**）配置拿到留出半上各报一次。"""
    label, w = job
    ok1, okk, n = _G["pair"].ev(w, _G["pair"].B, _G["topk"])
    return dict(label=label, w=w, top1=ok1, top5=okk, n=n)


# 网格轴（**粗网格**）：不含 `strs`（泄漏列，永远 0），字符串只走 `strs_s`。
# `counts` / `bsig` 是"同进同出"的伪轴：前者同时给 ncal/ncall，后者给 cbsig/rbsig。
GRID_AXES = collections.OrderedDict([
    ("size", (0.5, 1.0, 2.0)),
    ("gram", (1.0, 2.0, 3.0)),
    ("hist", (0.0, 0.5)),
    ("consts", (0.0, 0.25, 0.5)),
    ("caller", (0.0, 0.2, 0.5)),
])                                   # 5 轴 × 3×3×2×3×3 = 162 组（默认"粗网格"）

# 阶段 2（加次要特征）与阶段 3（细化）不写死在这里：它们是在**阶段 1 的赢家**上再扫，
# 所以用 `--fixed` 钉住赢家、用 `--axes` 给要扫的轴（实际命令见 `docs/02` §35.9）。
# 例：`tune3 --fixed "size=0.5,gram=2,consts=0.25,caller=0.2" \
#        --axes "cls=0,0.4;callee=0,0.3;counts=0,0.3;bsig=0,0.5;strs_s=0,0.2"`  # 2^5 = 32 组


def parse_axes(spec):
    """`size=0.5,1,2;gram=1,2` → OrderedDict；`counts`/`bsig` 是伪轴。"""
    axes = collections.OrderedDict()
    for part in (spec or "").split(";"):
        part = part.strip()
        if not part:
            continue
        k, _, vs = part.partition("=")
        k = k.strip()
        if k not in tuple(SCORE_KEYS) + ("counts", "bsig"):
            T.die("未知网格轴 %r" % k)
        axes[k] = tuple(float(v) for v in vs.split(",") if v.strip())
    return axes


def grid_configs(axes=None, fixed=None):
    axes = axes or GRID_AXES
    keys = list(axes)
    out = []
    for combo in itertools.product(*(axes[k] for k in keys)):
        w = dict(fixed or {})
        for k, v in zip(keys, combo):
            if k == "counts":
                w["ncal"] = w["ncall"] = v
            elif k == "bsig":
                w["cbsig"], w["rbsig"] = v, 0.6 * v
            else:
                w[k] = v
        w.setdefault("strs", 0.0)          # 泄漏列永不进网格
        out.append((wlabel(w), w))
    return out


def _run_jobs(fn, jobs, items, label):
    """串行或进程池跑 `items`；返回 (结果, 墙钟秒)。**并行不改结果**，只改墙钟。"""
    t0 = time.time()
    jobs = max(1, int(jobs or 1))
    if jobs == 1 or len(items) <= 1:
        res = [fn(it) for it in items]
    else:
        ctx = multiprocessing.get_context("fork")
        with ctx.Pool(processes=min(jobs, len(items))) as pool:
            res = pool.map(fn, items, chunksize=1)
    dt = time.time() - t0
    sys.stderr.write("[fp] %s：%d 项，jobs=%d，墙钟 %.1f s（%.2f s/项）\n"
                     % (label, len(items), min(jobs, max(1, len(items))), dt, dt / max(1, len(items))))
    return res, dt


def cmd_tune3(args):
    """**只在调参半**上跑权重网格（同 ISA linux→win），`--jobs N` 横向并行。

    输出：墙钟 / 加速比（`--jobs` 与 `--jobs 1` 对同一 `--max-configs` 子集各跑一次即可对比）、
    调参半 top-1 排名前 25、以及**选中配置在留出半上的那一次**报数（只报一次，不回头改）。
    """
    pair = Pair("win", "linux")
    _G["pair"], _G["topk"] = pair, args.topk
    axes = parse_axes(args.axes) if args.axes else GRID_AXES
    cfgs = grid_configs(axes, weights_override({}, args.fixed) if args.fixed else None)
    if args.max_configs:
        cfgs = cfgs[:args.max_configs]
    W = sys.stdout.write
    W("网格：%s\n" % " ; ".join("%s=%s" % (k, ",".join("%.3g" % v for v in vs))
                                for k, vs in axes.items()))
    W("配置数 %d；调参半 n=%d（留出半 n=%d 不参与选择）；候选窗口 %s；topk=%d\n"
      % (len(cfgs), len(pair.A), len(pair.B), WIN_SAME, args.topk))
    res, dt = _run_jobs(_grid_eval, args.jobs, cfgs, "权重网格")
    W("墙钟 %.1f s（jobs=%d）⇒ %.1f 配置/s\n" % (dt, args.jobs, len(cfgs) / max(dt, 1e-9)))
    res.sort(key=lambda r: (-(r["top1"]), -(r["top5"]), r["label"]))
    W("\n调参半 top-1 前 25（同分按 top-5 排；**这张表是唯一允许拿来选权重的依据**）：\n")
    W("  %-6s %-6s  %s\n" % ("top-1", "top-5", "权重（非 0 项）"))
    for r in res[:25]:
        W("  %5.1f%% %5.1f%%  %s\n"
          % (100.0 * r["top1"] / r["n"], 100.0 * r["top5"] / r["n"], r["label"]))
    best = res[0]
    W("\n调参半最高：%s（top-1 %.1f%%）\n"
      % (best["label"], 100.0 * best["top1"] / best["n"]))
    # **选择规则先定好再用**（免得看到留出半再挑）：在"只差 1 个样本以内"的并列桶里，
    # 取**非 0 特征最少**的配置（同样的准确率下越简单越可能是真信号，不是网格噪声）；
    # 再按 top-5、标签排序。1 个样本 = 1/166 = 0.6%，比这更小的差别不选。
    bucket = [r for r in res if r["top1"] >= best["top1"] - 1]
    bucket.sort(key=lambda r: (sum(1 for k in SCORE_KEYS if r["w"].get(k)),
                               -(r["top5"]), r["label"]))
    pick = bucket[0]
    W("并列桶（top-1 ≥ 最高−1 个样本）：%d 组；按「非 0 特征最少」选中：%s\n"
      % (len(bucket), pick["label"]))
    W("选中配置：%s\n" % ", ".join("%s=%.3g" % (k, pick["w"][k]) for k in SCORE_KEYS
                                    if pick["w"].get(k)))
    if args.holdout:
        # 留出半**只报数、不参与选择**：选在上面的调参半已经结束，这里只是把短名单各报一次，
        # 让人看得见"选中的那个在留出半是多少、没选中的又是多少"（敏感性）。
        short = bucket[:args.holdout]
        res2, dt2 = _run_jobs(_holdout_eval, args.jobs,
                             [(r["label"], r["w"]) for r in short], "留出半（只报数）")
        W("\n留出半（n=%d，**这些数不进选择**）：\n" % len(pair.B))
        W("  %-6s %-6s  %s\n" % ("top-1", "top-5", "权重（非 0 项）"))
        for r in res2:
            W("  %5.1f%% %5.1f%%  %s\n"
              % (100.0 * r["top1"] / r["n"], 100.0 * r["top5"] / r["n"], r["label"]))
    return 0


def _ablate_eval(job):
    label, w = job
    pair = _G["pair"]
    a1, a5, an = pair.ev(w, pair.A, _G["topk"])
    b1, b5, bn = pair.ev(w, pair.B, _G["topk"])
    return dict(label=label, w=w, a1=a1, a5=a5, an=an, b1=b1, b5=b5, bn=bn)


def cmd_ablate(args):
    """一个特征一个特征清零 / 加回来，**两半都报**（`docs/159` §3.2 的复算入口）。

    这不是"调参"，是**归因**：每一行只改一个特征，看两半的 top-1 掉多少 ——
    用来回答「84.2% 里有多少是泄漏、多少是结构」。
    """
    pair = Pair("win", "linux")
    _G["pair"], _G["topk"] = pair, args.topk

    def zero(w, *keys):
        out = dict(w)
        for k in keys:
            out[k] = 0.0
        return out

    new = W_SAME
    old = W_SAME_LEAK
    configs = [
        ("① 发布值（含泄漏 strs 原始）", old),
        ("② 本次默认（无泄漏）", new),
        ("③ ② − consts", zero(new, "consts")),
        ("④ ② − caller", zero(new, "caller")),
        ("⑤ ② − size", zero(new, "size")),
        ("⑥ ② − gram", zero(new, "gram")),
        ("⑦ ② + strs_s（干净字符串）", dict(new, strs_s=0.2)),
        ("⑧ ② + cls（类直方图）", dict(new, cls=0.4)),
        ("⑨ ② + callee/counts/bsig", dict(new, callee=0.3, ncal=0.3, ncall=0.3,
                                        cbsig=0.5, rbsig=0.3)),
        ("⑩ ② + strs（把泄漏加回来）", dict(new, strs=0.2)),
        ("⑪ ① − strs（审计 §3.2）", zero(old, "strs")),
        ("⑫ ① − consts", zero(old, "consts")),
        ("⑬ ① − hist", zero(old, "hist")),
        ("⑭ ① − strs − consts", zero(old, "strs", "consts")),
        ("⑮ ① strs=2.0（放大泄漏）", dict(old, strs=2.0)),
    ]
    W = sys.stdout.write
    W("真值来源：%s\n" % ", ".join("%s=%d" % kv for kv in pair.prov.items()))
    W("分半：调参半 %d / 留出半 %d；候选窗口 %s；topk=%d\n\n"
      % (len(pair.A), len(pair.B), WIN_SAME, args.topk))
    jobs = [(label, w) for label, w in configs]
    res, dt = _run_jobs(_ablate_eval, args.jobs, jobs, "消融")
    W("墙钟 %.1f s（jobs=%d）\n" % (dt, args.jobs))
    W("%-30s | %-16s | %-16s\n" % ("配置", "调参半 top-1 / top-5", "留出半 top-1 / top-5"))
    W("%-30s | %-16s | %-16s\n" % ("", "n    top-1  top-5", "n    top-1  top-5"))
    for r in res:
        W("%-30s | %3d %5.1f%% %5.1f%% | %3d %5.1f%% %5.1f%%\n"
          % (r["label"], r["an"], 100.0 * r["a1"] / r["an"], 100.0 * r["a5"] / r["an"],
             r["bn"], 100.0 * r["b1"] / r["bn"], 100.0 * r["b5"] / r["bn"]))
    if args.coverage:
        # 特征两侧覆盖率（正对照 vs 目标集合）——"看得见 / 看不见"的直接量化
        lin = pair.fps["linux"]
        tgt = target_addrs(pair.fps)
        named = named_linux_348()
        for key in ("strs", "strs_s", "consts"):
            pos = sum(1 for a in named if lin.feats[a][key])
            tar = sum(1 for a in tgt if lin.feats[a][key])
            W("  覆盖率 %-7s 正对照 %3d/%d = %5.1f%%  目标 %5d/%d = %5.1f%%\n"
              % (key, pos, len(named), 100.0 * pos / len(named),
                 tar, len(tgt), 100.0 * tar / len(tgt)))
    W("\n口径：top-1 = 第一名候选恰好是 Location 真值；top-5 = 真值在前 5 名里。\n")
    return 0


def _shard_worker(spec):
    """跑一个分片（`run --shard i/N --out …`），把 stdout/stderr 收进 StringIO 免得 8 路互相插。"""
    i, n, out, topk = spec
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
        cmd_run(argparse.Namespace(shard="%d/%d" % (i, n), out=out, topk=topk, limit=0))
    return out


def cmd_runall(args):
    """8 片并行跑全量 12,736 → `merge`（判档 + 全局单射闸门）。

    分片口径与 `run --shard i/N` **完全一样**（`targets[i::N]`）；`--jobs` 只决定同时开几个分片。
    `--jobs 8` 与串行 `run` 的产物逐字节相同（分片文件 + merge 是确定性排序）。
    """
    shards = args.shards
    o = args.out or OUT_TSV
    tmpdir = args.tmp or os.path.join(cache_dir(), "shards")
    os.makedirs(tmpdir, exist_ok=True)
    specs = [(i, shards, os.path.join(tmpdir, "s%d.tsv" % i), args.topk) for i in range(shards)]
    W = sys.stdout.write
    W("全量 12,736 个目标切成 %d 片（`targets[i::N]`），同时开 %d 片\n"
      % (shards, min(args.jobs, shards)))
    outs, dt = _run_jobs(_shard_worker, args.jobs, specs, "分片")
    W("分片全部完成，墙钟 %.1f s（jobs=%d）\n" % (dt, args.jobs))
    return cmd_merge(argparse.Namespace(addr=outs, out=o))


# 档位门槛（**调参半**选出，留出半报数；见 `calib` / docs/02 §35.4）：
#   高置信同源 = score>=0.65 且 margin>=0.05 且**全局**单射
#                ⇒ 去泄漏重训后：调参半 100%（n=74）、留出半 **95.8%**（n=72）
#                ⚠ 这是「该档位里 top-1 对的比例」，**不是**全体准确率：全体留出半 top-1 只有 72.7%
#                （`docs/02` §35.3 / `docs/159` §3.2）。目标集合只有 348 这一批的 1/36 体量，逐条待核。
#   推断同源   = score>=0.50 且 margin>=0.05（不含上一档）      ⇒ 精度约 74%
#   弱候选     = score>=0.50 但 margin<0.05（打平/贴太近）      ⇒ 约 65%
#   未知       = 分数 <0.50 或窗口里根本没有候选
TIER_HI = "高置信同源"
TIER_MID = "推断同源"
TIER_WEAK = "歧义"
TIER_UNK = "未知"
THR_HI = 0.65
THR_MID = 0.50
MARGIN_HI = 0.05
SMALL_FN = 256          # 正对照这一档样本太少（两半合计 20/27 = 74%），不足 256 B 要打注记


def tier_from_score(score, margin):
    """只看**分数**的档位（`None` = 没有候选）。单射与体量注记不在这里，见 `apply_gates`。"""
    if score is None:
        return TIER_UNK
    if score >= THR_HI and margin >= MARGIN_HI:
        return TIER_HI
    if score >= THR_MID and margin >= MARGIN_HI:
        return TIER_MID
    if score >= THR_MID:
        return TIER_WEAK
    return TIER_UNK


def tier_of(best):
    if not best:
        return TIER_UNK, 0.0, 0.0
    score, margin = best[0][0], margin_of(best)
    return tier_from_score(score, margin), score, margin


def apply_gates(rows):
    """**全局**判档 + 单射闸门 + 体量注记（`run` 与 `merge` 共用；写 `r["tier"]` / `r["note"]`）。

    每条 `r` 需要 `score`（float 或 None）、`margin`、`target`（Windows VA 或 None）、`size`。
    函数是**幂等**的：档位一律由 `score`/`margin` **重算**，不相信输入里的 `tier` 列 ——
    否则 `merge` 会把分片内算出来的档位（含片内的错）当成既成事实。

    ## 单射闸门为什么必须**全局**（`docs/159` §6 的硬错误）

    规则：**一个 Windows 目标在整份夹具里只能被一行认领为「高置信同源」**。
    旧实现把 `claim` 计数放在 `cmd_run()` 里，而 `run` 用 `targets[i::8]` **按地址跨步**切分
    ⇒ 结构孪生通常地址相邻，几乎必然落在**不同片**，闸门永远看不见它们。实测后果：
    2,803 行只落在 2,772 个目标上，25 个目标被 ≥2 行认领（56 行），且没有 `multi-claim` 注记；
    其中 Windows `0x1800ead70` 被 **5** 个 Linux 函数认领，同片的 2 个被降了档、
    跨片的 `0x6baef0` 仍是「高置信」。⇒ 至少 31 行必错。所以计数挪到这里，`merge` 时
    对**全部 12,736 行**重算一次（`run` 不分片时它覆盖的也是全集，同一段代码）。

    ## 多行抢同一个目标时怎么处置：**全部**降一档，不做"留分数最高的一个"

    * 抢同一个目标的 ≥2 行里**至多一行**可能是对的，但我们没有任何证据说**是哪一行**
      —— `docs/159` §7 实测的失败模式正是近孪生（`update_names_button_texture` 与
      `update_view_function_button_texture` 只差一个贴图名常量），它们的 `score` 差在噪声内。
      在这种情况下"取分数最高的"等于在一组没有信息的候选里挑一个当高置信，只会把错误
      伪装成有把握。
    * 降一档**不丢候选**：这些行仍以「推断同源」+ `note=multi-claim` 留在夹具里，
      下游照 §35.4 的口径只能当结构同源候选用。
    * 单遍计数、不迭代（同一行集内一次判完；与 `calib` 打印的闸门口径一致）。
    """
    for r in rows:
        r["tier"] = tier_from_score(r["score"], r["margin"])
        r["note"] = ""
    claim = collections.Counter(r["target"] for r in rows if r["tier"] == TIER_HI)
    for r in rows:
        if r["tier"] == TIER_HI and claim[r["target"]] > 1:
            r["tier"] = TIER_MID
            r["note"] = "multi-claim"
    # 体量注记（§35.7 第 2 条）：正对照 <256 B 只有 27 个样本（20/27 = 74%），
    # 而目标集合有 58.8% 在这一档 ⇒ 档位不降，但引用时必须连注记一起引。
    for r in rows:
        if r["size"] < SMALL_FN:
            r["note"] = r["note"] + ",small-uncalibrated" if r["note"] else "small-uncalibrated"
    return rows


PREAMBLE = (
    "# 由 tools/callgraph_fingerprint.py runall/run + merge 生成，不要手改（重跑会覆盖）。\n"
    "# 目标：Linux 2.0.60 里**不引用任何 src/*.rs panic::Location** 的函数"
    "（三份符号表都没有它们，名字要不回来的那批）。\n"
    "# 匹配**只**用结构：size / 指令三元组 gram / 常量 / 调用者；不读名字、不读地址、不读 Location 键。\n"
    "# ⚠ 去泄漏重训（docs/159 §3.2、docs/02 §35.3）：发布版的 strs 特征其实就是 Location 的 file 字段"
    "（正对照 348/348 带自己的 src/*.rs 串，目标集合 0/12736 带）⇒ 已**默认关掉**（权重 0）。\n"
    "#   去掉泄漏后正对照 top-1：留出半 84.2% → 73.3%（只关 strs）/ 72.7%（重训默认）；"
    "top-5 93.3% → 88.5%。调参半 81.9% / 91.6%。\n"
    "#   去掉**一切** Rust 源文件路径之后的字符串（strs_s）实测也不出力（两半 0/331 翻转），权重同样为 0。\n"
    "# tier=高置信同源：分数>=0.65 且 margin>=0.05 且**全局单射**（merge 对全量重算，一个目标只许一行认领）。\n"
    "# 精度口径：**只在「已定名 348」上测过**。高置信档 留出半 95.8%（n=72）/ 调参半 100%（n=74）；"
    "但**全体** top-1 只有 72.7%（留出半）⇒ 引用档位精度时必须同时给覆盖率，"
    "否则会把 72.7% 误读成 95.8%。对 12,736 只能说「结构同源候选」。\n"
    "# note=small-uncalibrated 表示 size<256 B——正对照这一档两半合计只有 20/27 = 74%，精度没有实测支撑。\n"
    "# note=multi-claim 表示有别的 Linux 函数也认领同一个 Windows 目标（≥2 抢 1）⇒ 已降一档。\n"
    "# nstrs = 引用到的**结构**字符串数（已扣掉 Rust 源文件路径）；含泄漏的原始串数要看 `probe`。\n"
    "# android_cand_* 是跨 ISA 候选，实测 top-1 只有 6.5%，**不是证据**，只作结构角色参考。\n"
)
COLS = ("linux_addr", "size", "ninsn", "hist_hash", "ncallees", "ncallers", "nstrs",
        "match_build", "match_addr", "score", "margin", "tier", "android_cand_addr",
        "android_cand_score", "bsig", "note")


def write_tsv(rows, path):
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(PREAMBLE)
        fh.write("\t".join(COLS) + "\n")
        for f in rows:
            w = f["win"]
            a = f["android"]
            fh.write("\t".join([
                "0x%x" % f["addr"], str(f["size"]), str(f["ninsn"]), f["hist_hash"],
                str(f["ncallees"]), str(f["ncallers"]), str(f["nstrs"]),
                "win" if w else "-", ("0x%x" % w[0][1]) if w else "-",
                ("%.4f" % w[0][0]) if w else "-", ("%.4f" % margin_of(w)) if w else "-",
                f["tier"],
                ("0x%x" % a[0][1]) if a else "-", ("%.4f" % a[0][0]) if a else "-",
                f["bsig"], f.get("note") or "-",
            ]) + "\n")


def cmd_run(args):
    fps = load_all()
    lin = fps["linux"]
    targets = target_addrs(fps)
    if args.shard:
        i, n = [int(x) for x in args.shard.split("/")]
        targets = targets[i::n]
    if args.limit:
        targets = targets[:args.limit]
    sys.stderr.write("[fp] 目标 %d 个（无 src/*.rs Location）\n" % len(targets))
    pools = {}
    scorers = {}
    wins = {}
    for src_tag, dst_tag in (("win", "linux"), ("android", "linux")):
        pools[src_tag] = Pool(fps[src_tag])
        scorers[src_tag], _w, wins[src_tag] = scorer_for(src_tag, dst_tag)
    rows = []
    for i, va in enumerate(targets):
        qf = lin.feats[va]
        f = dict(addr=va, size=qf["size"], ninsn=qf["ninsn"], hist_hash=qf["hist_hash"],
                 ncallees=len(qf["callees"]), ncallers=len(qf["callers"]),
                 nstrs=len(qf["strs_s"]), nconsts=len(qf["consts"]), bsig=qf["bsig"] or "-")
        f["win"] = rank(qf, pools["win"], scorers["win"], wins["win"], args.topk)
        f["android"] = rank(qf, pools["android"], scorers["android"], wins["android"],
                            min(args.topk, 3))
        # 档位用**发布精度**（4 位小数）判，不用内存里的全精度浮点：`merge` 是从夹具的
        # score/margin 列重算档位的，若这里用全精度，`0.649996`（内存）与 `0.6500`（夹具）
        # 会给出两个不同的档 —— 实测 12,736 行里就有 3 行落在这个边界上，
        # 于是"夹具自己复算不出自己的档位"。量化一次，run 与 merge 逐行一致。
        _t0, score, margin = tier_of(f["win"])
        f["score"] = float("%.4f" % score)
        f["margin"] = float("%.4f" % margin)
        f["tier"] = tier_from_score(f["score"], f["margin"])
        f["target"] = f["win"][0][1] if f["win"] else None
        rows.append(f)
        if (i + 1) % 500 == 0:
            sys.stderr.write("[fp] %d/%d\n" % (i + 1, len(targets)))
    # 判档 / **单射闸门** / 体量注记 —— 与 `merge` 共用同一个函数（`apply_gates`）。
    # 不分片时这里的行集就是全集，闸门就是全局的；分片时它只看得见本片，
    # 所以**落地夹具必须走 merge**（merge 会对全部行重算，不继承这里的 tier）。
    apply_gates(rows)
    if args.shard or args.limit:
        sys.stderr.write("[fp] 本次只跑了部分目标（--shard/--limit）：单射闸门只在本行集内计数，"
                         "结论以 merge 的全局重算为准\n")
    out = args.out or OUT_TSV
    write_tsv(rows, out)
    W = sys.stdout.write
    W("写出 %s：%d 行\n" % (os.path.relpath(out, ROOT), len(rows)))
    for k, v in collections.Counter(f["tier"] for f in rows).most_common():
        W("  %-10s %6d\n" % (k, v))
    return 0


def _row_from_fields(p2):
    """一行 TSV → `apply_gates` 需要的字段（score/margin/target/size），同时保留原始列。

    档位**从 score/margin 重算**，所以分数就是唯一输入；这样 merge 既不依赖分片里
    算好的 tier，也能把旧口径的产物重新纠正一遍。
    """
    f = list(p2)
    i = {c: k for k, c in enumerate(COLS)}
    sc, mg, ma = f[i["score"]], f[i["margin"]], f[i["match_addr"]]
    return dict(fields=f,
                score=float(sc) if sc != "-" else None,
                margin=float(mg) if mg != "-" else None,
                target=int(ma, 16) if ma.startswith("0x") else None,
                size=int(f[i["size"]]))


def cmd_merge(args):
    """合并分片 → 夹具，并在**这里**做全局判档 + 单射闸门（`docs/159` §6 的修法）。

    为什么闸门必须在这个阶段开：`run` 用 `targets[i::8]` **按地址跨步**切分，而结构孪生
    通常地址相邻 ⇒ 抢同一个 Windows 目标的两行几乎必然落在不同片，片内计数永远数不到它们
    （实测漏 25 组 / 56 行）。`merge` 是唯一看得到全部行的地方，所以闸门只能落在这里；
    档位也在这里按 score/margin 重算，不从分片继承（`apply_gates` 幂等）。
    """
    raw = {}
    for path in args.addr:
        with open(path, encoding="utf-8") as fh:
            hdr = None
            for line in fh:
                if line.startswith("#") or not line.strip():
                    continue
                p2 = line.rstrip("\n").split("\t")
                if hdr is None:
                    hdr = tuple(p2)
                    if hdr != COLS:
                        T.die("%s 的表头与本工具的 COLS 不一致（列口径变了，别混着 merge）"
                              % path)
                    continue
                raw[int(p2[0], 16)] = p2
    rows = [_row_from_fields(raw[va]) for va in sorted(raw)]
    apply_gates(rows)
    out = args.out or OUT_TSV
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    ti, ni = COLS.index("tier"), COLS.index("note")
    with open(out, "w", encoding="utf-8") as fh:
        fh.write(PREAMBLE)
        fh.write("\t".join(COLS) + "\n")
        for r in rows:
            r["fields"][ti] = r["tier"]
            r["fields"][ni] = r["note"] or "-"
            fh.write("\t".join(r["fields"]) + "\n")
    W = sys.stdout.write
    W("合并 %d 个分片 → %s：%d 行，%d 列（档位按 score/margin **全局**重算）\n"
      % (len(args.addr), os.path.relpath(out, ROOT), len(rows), len(COLS)))
    for k, v in collections.Counter(r["tier"] for r in rows).most_common():
        W("  %-10s %6d\n" % (k, v))
    hi = [r for r in rows if r["tier"] == TIER_HI]
    tgts = collections.Counter(r["target"] for r in hi)
    dup = [t for t, c in tgts.items() if c > 1]
    mc = sum(1 for r in rows if "multi-claim" in r["note"])
    W("  单射（全局）：高置信 %d 行落在 %d 个 Windows 目标上；降档 %d 行（multi-claim）\n"
      % (len(hi), len(tgts), mc))
    W("  其中 size<256 B（small-uncalibrated）%d 行\n"
      % sum(1 for r in hi if r["size"] < SMALL_FN))
    if dup:
        W("  ⚠ 仍有 %d 组目标被 ≥2 行认领（闸门失效，不该发生）\n" % len(dup))
    return 0


def cmd_report(args):
    fps = load_all()
    W = sys.stdout.write
    named = named_linux()          # 354：三份夹具的名字并集
    named348 = named_linux_348()   # 348：再 ∩ 527（正对照用的就是这个）
    W("指纹库（四份构建）\n")
    for tag, fp in fps.items():
        feats = fp.feats
        icf = collections.Counter(f["bsig"] for f in feats.values() if f["bsig"])
        grp = [c for c in icf.values() if c > 1]
        W("  %-8s %6d 函数  有指纹 %6d  有被调者 %6d  有调用者 %6d  引用字符串 %5d  "
          "ICF 等体组 %5d 组（含 %d 个地址） Location %4d  名字 %5d\n"
          % (tag, len(fp.universe),
             sum(1 for f in feats.values() if f["ninsn"]),
             sum(1 for f in feats.values() if f["callees"]),
             sum(1 for f in feats.values() if f["callers"]),
             sum(1 for f in feats.values() if f["strs"]),
             len(grp), sum(grp), len(fp.locs), len(fp.names)))
    lin = fps["linux"]
    tgt = target_addrs(fps)
    W("\nLinux 目标 %d 个函数 = 有 src Location %d + 无 %d\n"
      % (len(lin.universe), len(lin.universe) - len(tgt), len(tgt)))
    W("其中三份夹具里有一个名字的 %d 个；正对照用的 348 口径（再 ∩ 527）%d 个"
      "；目标集合里有一个名字的 %d 个\n"
      % (len(named), len(named348), len([a for a in tgt if a in named])))
    # 目标集合的体量分布 —— 决定"正对照那 84.2% 能不能搬到 12,736"
    buckets = ((0, 64), (64, 256), (256, 1024), (1024, 4096), (4096, 1 << 30))
    # 上一版这里用的是 354（`named`），标签却写「348」⇒ 打印 624 B，与 `calib` 的 640 B
    # 自相矛盾（`docs/159` §0 引用的就是那个 624）。改成真正的 348 口径。
    W("\n目标集合（12,736）体量分布（正对照 348 个体量中位 %d B）：\n"
      % sorted(lin.feats[a]["size"] for a in named348)[len(named348) // 2])
    for lo, hi in buckets:
        sel = [a for a in tgt if lo <= lin.feats[a]["size"] < hi]
        W("  size[%5d,%6d) %5d 个（%s）\n"
          % (lo, hi, len(sel), fmt_pct(len(sel), len(tgt))))
    if os.path.exists(OUT_TSV):
        rows = 0
        tiers = collections.Counter()
        with open(OUT_TSV, encoding="utf-8") as fh:
            hdr = None
            for line in fh:
                if line.startswith("#"):
                    continue
                p = line.rstrip("\n").split("\t")
                if hdr is None:
                    hdr = p
                    continue
                rows += 1
                tiers[p[hdr.index("tier")]] += 1
        W("\n夹具 %s：%d 行，%d 列\n" % (os.path.relpath(OUT_TSV, ROOT), rows, len(hdr)))
        for k, v in tiers.most_common():
            W("  %-14s %6d\n" % (k, v))
    return 0


def cmd_probe(args):
    fps = load_all()
    W = sys.stdout.write
    for a in args.addr:
        va = int(a, 16) if not a.isdigit() else int(a)
        for tag, fp in fps.items():
            if va not in fp.feats:
                continue
            f = fp.feats[va]
            W("%-8s 0x%x size=%d ninsn=%d callees=%d callers=%d consts=%d strs=%d "
              "bsig=%s locs=%d\n"
              % (tag, va, f["size"], f["ninsn"], len(f["callees"]), len(f["callers"]),
                 len(f["consts"]), len(f["strs"]), f["bsig"], len(f["locrefs"])))
            W("        hist: %s\n" % ", ".join("%s×%d" % (k, v)
                                               for k, v in f["hist"].most_common(8)))
            W("        cls : %s\n" % ", ".join("%s×%d" % (k, v)
                                               for k, v in f["cls"].most_common(6)))
            if f["callee_sizes"]:
                W("        callee_sizes: %s\n" % list(f["callee_sizes"])[:12])
            if tag == "linux" and f["locrefs"]:
                # 之前这里是 `f["locs"]`（不存在的键）⇒ `probe` 必 KeyError，
                # §35.9 的复算命令因此还有第二处跑不起来。Location 引用在 `locrefs` 里。
                W("        locrefs: %d 个 Location 引用（§35 打分器**不读**这一列）\n"
                  % len(f["locrefs"]))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("cmd", choices=["build", "verify", "tune2", "calib",
                                    "run", "merge", "report", "probe",
                                    "ablate", "tune3", "runall"])
    ap.add_argument("addr", nargs="*")
    ap.add_argument("--force", action="store_true", help="无视缓存重扫")
    ap.add_argument("--topk", type=int, default=5)
    ap.add_argument("--limit", type=int, default=0, help="run 只做前 N 个（分批用）")
    ap.add_argument("--shard", default="", help="run 分片 i/N（多进程并行用）")
    ap.add_argument("--out", default="", help="run 的输出路径（分片时写临时文件）")
    ap.add_argument("--min-score", type=float, default=0.0)
    ap.add_argument("--min-margin", type=float, default=0.0)
    # 无泄漏重训（docs/159 §3.2 的整改）：并行 / 权重覆盖 / 网格轴
    ap.add_argument("--jobs", type=int, default=1,
                    help="进程池并行度（1 = 串行；网格与分片都吃这个参数，结果与串行逐位相同）")
    ap.add_argument("--weights", default="", help="覆盖默认权重，如 `strs=0,consts=0,gram=2`")
    ap.add_argument("--axes", default="", help="tune3 的网格轴，如 `size=.5,1,2;gram=1,2,3`")
    ap.add_argument("--max-configs", type=int, default=0, help="tune3 只跑前 N 组（量加速比用）")
    ap.add_argument("--fixed", default="", help="tune3 把不扫的轴钉在这个权重上，如 `size=1,gram=2`")
    ap.add_argument("--holdout", type=int, default=0,
                    help="tune3 在选完之后，把并列桶前 N 组拿到**留出半**各报一次（只报数，不参与选择）")
    ap.add_argument("--coverage", action="store_true", help="ablate 额外打印两侧特征覆盖率")
    ap.add_argument("--shards", type=int, default=8, help="runall 的分片数")
    ap.add_argument("--tmp", default="", help="runall 分片文件目录（默认缓存目录下 shards/）")
    a = ap.parse_args(argv)
    return dict(build=cmd_build, verify=cmd_verify, tune2=cmd_tune2, calib=cmd_calib,
                run=cmd_run, merge=cmd_merge, report=cmd_report, probe=cmd_probe,
                ablate=cmd_ablate, tune3=cmd_tune3, runall=cmd_runall)[a.cmd](a)


if __name__ == "__main__":
    sys.exit(main())
