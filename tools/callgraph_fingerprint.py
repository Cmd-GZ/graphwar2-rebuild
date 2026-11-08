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
    ---- 任务 57 的新证据来源（缓存 v6；默认权重全 0，只在 `newev` 里打开）----
    shape/shgrams(操作数形状三元组，抹掉寄存器号/立即数/偏移)    scal(全部 IMM 立即数)
    scal_c(= scal − 本函数自己 Location 的行号/列号/文件名长度，**泄漏探针**)
    wconst(宽常量 ≥0x10000 + f32/f64 位型规范化数值)            stok(字符串内容的语义 token)

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

    # 新证据来源（任务 57 / docs/02 §43）：两侧覆盖率 + 单独/组合消融 + 只在调参半上选权重
    ../.reference/venv/bin/python tools/callgraph_fingerprint.py newev --jobs 16 \
        --bucket-holdout --out fixtures/fingerprint_newev.tsv

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
#   v5 → v6：新增四个**新证据来源**字段（任务 57，`docs/02` §43）——
#            `shape`（操作数形状，机器码级三元组）/ `scal`（全部 IMM 立即数，含 <0x100）/
#            `wconst`（宽常量 + f32/f64 位型的规范化数值）/ `stok`（真字符串内容的语义 token）。
#            v5 的缓存**没有**这四个键，加载会直接报错，不会静默降级。
CACHE_VERSION = "cfgfp-v6"

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


# ===========================================================================
# 1.5 新证据来源（任务 57 / `docs/02` §43）—— 四个字段，全部**与名字/地址/Location 无关**
#
# 为什么 51 号之后必须换证据来源：13 个结构特征怎么组合，留出半都卡在 72.7~73.3%
# （`docs/02` §35.3.1 ⑤），`gram`（助记符三元组）已经把「助记符层面」的信息吃干。
# 新字段刻意避开 `gram` 已经在看的那一层：
#
#   `shape`  机器码级：**操作数形状**（reg/mem/imm + 寄存器宽度类 + 内存基址类），
#            抹掉寄存器号、立即数、偏移 ⇒ `mov eax,17` 与 `mov [rsp+8],rax` 不同，
#            但跨编译器/跨寄存器分配相同。三元组化后与 `gram` 只在「助记符」这一维重叠。
#   `scal`   常量 xref（**全部** IMM 立即数，含 <0x100 的）：现有 `consts` 只收 ≥0x100
#            的 32 位立即数 ⇒ 关卡号/标志位/小计数整段看不见。**明写的风险**：`panic::Location`
#            的行号也是小立即数，同一份源码在两份构建里行号相同 ⇒ 可能是一条泄漏通道，
#            所以另有 `loc_denoise()`，把"本函数自己 Location 的行号/列号/文件名长度"扣掉再测。
#   `wconst` 常量 xref 的另一半：**宽常量**（≥0x10000）与 f32/f64 位型规范化后的数值
#            （π / 黄金比 / murmur 常数这类"魔数"就是 §37 证据轴 ③，但现有 `consts` 收不到：
#            `0x9E3779B97F4A7C15` 超出 32 位范围被 `-0x80000000 <= imm <= 0xFFFFFFFF` 挡掉）。
#   `stok`   真字符串**内容语义**：把非 `.rs` 的字符串拆成小写 token 集合（snake/camel 都拆）。
#            与 `strs_s`（精确内容 sha1）不同，它对 `format!` 展开差异、前缀拼接差异更稳。
#
# 四个字段都**不看名字、不看地址**；`scal` 里凡是落进任何节的立即数（= 地址）一律不收。
# ===========================================================================
_TOK_RE = re.compile(r"[A-Za-z][A-Za-z0-9]{2,}")
_CAMEL_RE = re.compile(r"(?<=[a-z0-9])(?=[A-Z])")


def str_tokens(s):
    """字符串内容 → 语义 token 集合（小写；拆 snake_case 与 camelCase；丢纯数字与 hex 噪声）。

    `s` 是字节串（扫描器读出来的可打印 ASCII）。**不复用 `strs_s` 的 sha1**：
    精确内容跨构建漂移大（§35.3 实测 `strs` 真配对 Jaccard 中位仅 0.500），
    token 层才是"语义"那一层。
    """
    txt = s.decode("ascii", "ignore") if isinstance(s, (bytes, bytearray)) else s
    out = set()
    for w in _TOK_RE.findall(txt):
        for part in _CAMEL_RE.split(w):
            t = part.lower()
            if len(t) >= 3 and not t.isdigit():
                out.add(t)
    return frozenset(out)


_SHAPE_ID = {}


def shape_id(tok):
    """操作数形状 token → 16 位稳定 id（同 `mn_id`：sha1，跨 run/跨构建一致）。"""
    v = _SHAPE_ID.get(tok)
    if v is None:
        v = int(hashlib.sha1(tok.encode()).hexdigest()[:4], 16)
        _SHAPE_ID[tok] = v
    return v


def float_tag(v):
    """整数立即数按 f32 / f64 位型解码 → 规范化数值 token（跨位宽可比），解不出就 None。

    `%.6g` 是关键：`mov eax,0x40490fdb`（f32 π = 3.14159274101257324）与
    `movabs rax,0x400921FB54442D18`（f64 π = 3.141592653589793）规范化后都是 `F3.14159`，
    于是"同一个源码字面量在两份构建里用了不同宽度的指令"不再破坏特征。
    """
    uv = v & 0xFFFFFFFFFFFFFFFF
    cands = []
    if uv <= 0xFFFFFFFF:
        cands.append(struct.unpack("<f", struct.pack("<I", uv))[0])
    cands.append(struct.unpack("<d", struct.pack("<Q", uv))[0])
    for x in cands:
        if x != x or x in (float("inf"), float("-inf")):
            continue
        if x == 0.0 or abs(x) < 1e-6 or abs(x) > 1e15:
            continue
        return "F%.6g" % x
    return None


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
    from capstone.x86 import (X86_OP_IMM, X86_OP_MEM, X86_OP_REG,
                             X86_REG_RIP, X86_REG_RSP, X86_REG_RBP)

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
                      locrefs=set(), seq=[],
                      # 新证据来源（任务 57）：形状序列 / 全部立即数 / 宽常量 / 字符串 token
                      shape=[], scal=set(), wconst=set(), stok=set())
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
        # 新证据来源 F1：**操作数形状**（抹掉寄存器号 / 立即数 / 偏移）。
        # 只编码「种类 + 宽度类 + 内存基址类」，所以跨寄存器分配、跨两份 LLVM 构建不变。
        # `_SHAPE_ID` 是 memo，每条指令只是一次 dict 命中 + 一次 append。
        _parts = []
        for op in insn.operands:
            if op.type == X86_OP_REG:
                _parts.append("r%d" % op.size)
            elif op.type == X86_OP_IMM:
                _parts.append("i")
            elif op.type == X86_OP_MEM:
                _b = op.mem.base
                _parts.append("m" + ("p" if _b == X86_REG_RIP
                                     else ("s" if _b in (X86_REG_RSP, X86_REG_RBP) else "r"))
                              + ("x" if op.mem.index else ""))
            else:
                _parts.append("?")
        fe["shape"].append(shape_id("%s:%s" % (insn.mnemonic, ",".join(_parts))))
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
                # 新证据来源 F2：常量 xref。**凡是落进任何节的立即数都是地址，一律不收**，
                # 也不收函数地址（`tgt in universe` 在下面统一处理）。
                #   scal   = 全部立即数（含 <0x100 —— 关卡号/标志位/小计数就在这一档）
                #   wconst = 宽常量（≥0x10000）+ f32/f64 位型的规范化数值（魔数）
                if abs(op.imm) >= 2 and obj.sec_at(op.imm) is None and tgt not in universe:
                    fe["scal"].add(abs(op.imm))
                    if abs(op.imm) >= 0x10000:
                        fe["wconst"].add("V%d" % op.imm)
                        _ft = float_tag(op.imm)
                        if _ft:
                            fe["wconst"].add(_ft)
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
                    else:                       # 新证据来源 F3：**真字符串内容**的语义 token
                        fe["stok"] |= str_tokens(s)
    if verbose:
        sys.stderr.write("[fp] %-24s %6d 函数, ninsn=%d, 引用字符串的函数 %d"
                         "（其中引用 Rust 源文件路径的 %d ⇒ 泄漏列 strs_loc）\n"
                         % (os.path.basename(obj.path), len(universe),
                            sum(f["ninsn"] for f in feats.values()),
                            sum(1 for f in feats.values() if f["strs"]),
                            sum(1 for f in feats.values() if f["strs_loc"])))
        sys.stderr.write("[fp] %-24s 新证据：有形状 %d / 有立即数 %d / 有宽常量 %d / 有字符串 token %d\n"
                         % (os.path.basename(obj.path),
                            sum(1 for f in feats.values() if len(f["shape"]) >= 3),
                            sum(1 for f in feats.values() if f["scal"]),
                            sum(1 for f in feats.values() if f["wconst"]),
                            sum(1 for f in feats.values() if f["stok"])))
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
                      locrefs=set(), seq=[],
                      # 新证据来源（任务 57）：形状序列 / 全部立即数 / 宽常量 / 字符串 token
                      shape=[], scal=set(), wconst=set(), stok=set())
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
        # 新证据来源 F1（arm64 口径，与 x86 同构）：操作数形状，寄存器号/立即数/偏移全抹掉。
        _parts = []
        for _op in ops:
            if _op.type == ARM64_OP_REG:
                _r = insn.reg_name(_op.reg)
                _parts.append("w" if _r and _r[0] == "w" else
                              ("x" if _r and _r[0] == "x" else
                               ("s" if _r and _r[0] in "sqd" else "r")))
            elif _op.type == ARM64_OP_IMM:
                _parts.append("i")
            elif _op.type == ARM64_OP_MEM:
                _mb = insn.reg_name(_op.mem.base)
                _parts.append("m" + ("p" if _mb == "pc" else ("s" if _mb in ("sp", "wsp") else "r"))
                              + ("x" if _op.mem.index else ""))
            else:
                _parts.append("?")
        fe["shape"].append(shape_id("%s:%s" % (mn, ",".join(_parts))))
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
                    else:                       # 新证据来源 F3：真字符串内容的语义 token
                        fe["stok"] |= str_tokens(s)
        if mn in ("movz", "movn", "mov") and len(ops) >= 2 and ops[1].type == ARM64_OP_IMM:
            v = ops[1].imm
            if mn == "movn":
                v = ~v
            if len(ops) >= 3 and ops[2].type == ARM64_OP_IMM and ops[2].imm in (16, 32, 48):
                v <<= ops[2].imm
            if abs(v) >= 0x100:
                fe["consts"].add(v)
            # 新证据来源 F2（arm64 口径）：同上，落进任何节的立即数都是地址，不收。
            if abs(v) >= 2 and obj.sec_at(v) is None and v not in universe:
                fe["scal"].add(abs(v))
                if abs(v) >= 0x10000:
                    fe["wconst"].add("V%d" % v)
                    _ft = float_tag(v)
                    if _ft:
                        fe["wconst"].add(_ft)
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
        sys.stderr.write("[fp] %-24s 新证据：有形状 %d / 有立即数 %d / 有宽常量 %d / 有字符串 token %d\n"
                         % (os.path.basename(obj.path),
                            sum(1 for f in feats.values() if len(f["shape"]) >= 3),
                            sum(1 for f in feats.values() if f["scal"]),
                            sum(1 for f in feats.values() if f["wconst"]),
                            sum(1 for f in feats.values() if f["stok"])))
    return feats


# ===========================================================================
# 3. 一份构建 = 一个 FP（指纹库）
# ===========================================================================
def cache_dir():
    d = os.environ.get("CFGFP_CACHE") or os.path.join(ROOT, ".agent-work/cfgfp")
    os.makedirs(d, exist_ok=True)
    return d


def loc_denoise(fe, locs):
    """`scal` **扣掉本函数自己那几条 `panic::Location` 会投影出来的立即数**（泄漏探针）。

    为什么要扣：`Location` 结构体是 `{file_ptr, len, line, col}`，构造它的代码会把
    **行号 / 列号 / 文件名长度**当立即数写进指令 —— 而同一份源码在 Linux 与 Windows 两份构建里
    这些数**完全相同**（`docs/02` §35.3.1 的 `strs` 泄漏是同一个来源的另一半：
    文件名内容；这里是它的**数值**投影）。正对照 348 个**全部**引用 `src/*.rs` 的 `Location`，
    目标集合 12,736 个**一个都没有** ⇒ 若只保留 `scal`，这条通道就可能是"改进"的真凶。
    所以另算一份 `scal_c` = `scal` − {行号, 列号, 文件名长度}，**两侧同一规则对称计算**，
    两个都报；**数字只差一点点就说明改进不是行号通道给的**（`docs/02` §43.2 的实测）。
    """
    lv = [locs[l] for l in (fe.get("locrefs") or ()) if l in locs]
    if not lv:
        return frozenset(fe.get("scal") or ())
    drop = set()
    for item in lv:
        line, col = item[1], item[2]
        drop.add(line)
        drop.add(col)
        drop.add(len(item[0]))
    return frozenset(v for v in (fe.get("scal") or ()) if v not in drop)


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
            # 新证据来源 F2/F3 的定稿形态（frozenset 才能进 picklable 的指纹库、才能做 Jaccard）
            fe["scal"] = frozenset(fe.get("scal") or ())
            fe["wconst"] = frozenset(fe.get("wconst") or ())
            fe["stok"] = frozenset(fe.get("stok") or ())
            # **泄漏探针**（任务 57 的纪律要求，见 docs/02 §43.2）：
            # `panic::Location` 的**行号**也是小立即数，而同一份源码在两份构建里行号相同
            # ⇒ `scal` 可能是一条"行号指纹"通道。`scal_c` = 把**本函数自己引用的 Location
            # 行号**从 `scal` 里扣掉的版本；两者准确率之差就是这条通道的贡献（对称计算，
            # 两侧都用同一个规则，只是正对照有 Location、目标集合基本没有 ⇒ 差值即可疑）。
            fe["scal_c"] = loc_denoise(fe, self.locs)
            fe["hist_hash"] = stable_hash(fe["hist"].items())
            fe["seq"] = b"".join(struct.pack("<H", v) for v in fe.pop("seq", ()))
            fe["seq_hash"] = hashlib.sha1(fe["seq"]).hexdigest()[:16]
            fe["grams"] = frozenset(_trigrams(fe["seq"]))
            # 新证据来源 F1：形状序列 → 三元组集合（与 `grams` 同构，但 token 里带操作数种类）
            fe["shape"] = b"".join(struct.pack("<H", v) for v in fe.pop("shape", ()))
            fe["shgrams"] = frozenset(_trigrams(fe["shape"]))
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
        # `scal_c` 是**派生**字段（`scal` − 自己 Location 的行号/列号/文件名长度）：
        # 每次载入都按当前口径重算，改了 denoise 规则就不必重扫 366 MB 缓存。
        if s.feats and "scal" in next(iter(s.feats.values())):
            for _f in s.feats.values():
                _f["scal_c"] = loc_denoise(_f, s.locs)
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
        if probe is not None and "shgrams" not in probe:
            T.die("%s 是 v5 缓存（没有新证据来源字段 shape/scal/wconst/stok）"
                  "——抬 CACHE_VERSION 或换 CFGFP_CACHE" % cp)
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
# 新证据来源（任务 57）的键：`shape` / `scal` / `wconst` / `stok`，外加泄漏探针用的
# `scal_c`（= `scal` 扣掉本函数自己 Location 的行号）。默认全 0（不进 W_SAME），
# 只在 `newev` 的消融表里按需打开，**不许**在没有留出半数之前写进 W_SAME。
SCORE_KEYS = ("size", "hist", "gram", "consts", "callee", "caller", "strs", "strs_s",
              "ncal", "ncall", "cbsig", "rbsig", "cls",
              "shape", "scal", "scal_c", "wconst", "stok")
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
    # ---- 新证据来源（任务 57 / docs/02 §43）----
    # 与上面几项的区别：`shape` 看的是**操作数种类**（`gram` 只看助记符），`scal` 收的是
    # 现有 `consts` 收不到的那一段（<0x100 与 >32 位），`wconst` 只收宽常量/浮点魔数，
    # `stok` 是字符串的**语义 token**（`strs_s` 是精确内容 sha1）。两侧都空时返回 None
    # （= 这一项不参与归一化分母），与 `consts`/`strs_s` 的既有口径一致。
    add(w.get("shape"), jaccard(qf["shgrams"], df["shgrams"])
        if (qf["shgrams"] and df["shgrams"]) else None)
    add(w.get("scal"), jaccard(qf["scal"], df["scal"]) if (qf["scal"] and df["scal"]) else None)
    add(w.get("scal_c"), jaccard(qf["scal_c"], df["scal_c"])
        if (qf["scal_c"] and df["scal_c"]) else None)
    add(w.get("wconst"), jaccard(qf["wconst"], df["wconst"])
        if (qf["wconst"] and df["wconst"]) else None)
    add(w.get("stok"), jaccard(qf["stok"], df["stok"]) if (qf["stok"] and df["stok"]) else None)
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


# ===========================================================================
# 7.6 新证据来源（任务 57 / `docs/02` §43）
#
# 51 号把权重这条路走死了：302 组结构权重网格在调参半最高 82.5%，留出半**全部**落在
# 72.7~73.3%（`docs/02` §35.3.1 ⑤）。`newev` 只做一件事：把四个**新的特征来源**按同一套
# 纪律测一遍 —— ①两侧覆盖率（正对照 / 目标集合，两个都报）；②单独 + 组合消融；
# ③**只在调参半上选权重**，留出半只报数。选不中就是选不中，不许回头改。
# ===========================================================================
NEW_KEYS = ("shape", "scal", "scal_c", "wconst", "stok")
NEW_DESC = dict(
    shape="F1 机器码级：操作数形状三元组（reg/mem/imm + 宽度类 + 内存基址类）",
    scal="F2 常量 xref：全部 IMM 立即数（含 <0x100 的关卡号/标志位/小计数）",
    scal_c="F2' 同上但扣掉本函数自己 Location 的**行号**（泄漏探针）",
    wconst="F2'' 宽常量 ≥0x10000 + f32/f64 位型规范化魔数",
    stok="F3 真字符串内容的语义 token（不是 .rs 文件名）",
)
BUCKETS = ((0, 256, "<256 B"), (256, 1024, "256-1,024 B"),
           (1024, 4096, "1,024-4,096 B"), (4096, 1 << 62, ">=4,096 B"))


def _newev_cov_rows(lin, addrs, keys):
    """一个函数集合上每个新特征的覆盖率与集合大小中位。"""
    out = {}
    for k in keys:
        vals = [len(lin.feats[a][k]) for a in addrs]
        hit = sum(1 for v in vals if v)
        vals.sort()
        out[k] = (hit, vals[len(vals) // 2] if vals else 0)
    return out


def cmd_newev(args):
    pair = Pair("win", "linux")
    _G["pair"], _G["topk"] = pair, args.topk
    W = sys.stdout.write
    lin = pair.fps["linux"]
    named = sorted(named_linux_348())
    tgt = target_addrs(pair.fps)
    keys = ("shape", "scal", "scal_c", "wconst", "stok", "strs_s", "consts", "grams")

    W("=== (1) 新证据来源的两侧覆盖率（**两个都必须报**，docs/159 3.2 的纪律）===\n")
    for k in NEW_KEYS:
        W("  %-8s %s\n" % (k, NEW_DESC[k]))
    W("正对照 = 已定名 %d 个（都引用 `src/*.rs` Location）；目标集合 = 没有 `src/` Location 的 %d 个\n\n"
      % (len(named), len(tgt)))
    W("%-9s %-10s %-17s %-17s %-8s %s\n"
      % ("特征", "两侧都有", "正对照命中", "目标命中", "比值", "集合中位 正/目"))
    cov_p = _newev_cov_rows(lin, named, keys)
    cov_t = _newev_cov_rows(lin, tgt, keys)
    cov_rows = []
    for k in keys:
        hp, mp = cov_p[k]
        ht, mt = cov_t[k]
        pr, tr = hp / float(len(named)), ht / float(len(tgt))
        ratio = (pr / tr) if tr else float("inf")
        W("%-9s %-10s %3d/%3d = %5.1f%%  %5d/%5d = %5.1f%%  %6.2f  %4d / %4d\n"
          % (k, "是" if (hp and ht) else ("**否**" if (hp or ht) else "-"),
             hp, len(named), 100 * pr, ht, len(tgt), 100 * tr, ratio, mp, mt))
        cov_rows.append((k, hp, len(named), ht, len(tgt), mp, mt))

    W("\n--- 按体量分桶（体量混淆检查：同一桶里两侧要贴齐，否则那是「正对照更大」而不是特征）---\n")
    W("%-9s" % "特征" + "".join("  %-24s" % b[2] for b in BUCKETS) + "\n")
    for k in ("shape", "scal", "wconst", "stok"):
        row = "%-9s" % k
        for lo, hi, _lab in BUCKETS:
            ps = [a for a in named if lo <= lin.feats[a]["size"] < hi]
            ts = [a for a in tgt if lo <= lin.feats[a]["size"] < hi]
            ph = sum(1 for a in ps if lin.feats[a][k])
            th = sum(1 for a in ts if lin.feats[a][k])
            row += "  %3d/%-4d %5.1f%%/%-5.1f%%" % (
                ph, len(ps), 100 * ph / len(ps) if ps else 0.0,
                100 * th / len(ts) if ts else 0.0)
        W(row + "\n")
    W("  （每格 = 命中/样本  正对照%/目标%）\n")

    # ---- (2) 消融：新特征单独 / 与旧特征组合（两半都报） ----
    base = W_SAME
    cfgs = [("(0) 51 号基线 W_SAME", dict(base))]
    for k in NEW_KEYS:
        cfgs.append(("(1a) 单独 size+%s" % k, dict(size=0.25, **{k: 2.0})))
        for wt in (0.5, 1.0, 2.0):
            cfgs.append(("(1b) 基线+%s=%.2g" % (k, wt), dict(base, **{k: wt})))
    combo = dict(base, **{"shape": 1.0, "scal": 0.5, "wconst": 0.5, "stok": 0.5})
    cfgs.append(("(2a) 基线+shape/scal/wconst/stok", combo))
    cfgs.append(("(2b) 同上但 scal->scal_c", dict(combo, scal=0.0, scal_c=0.5)))
    cfgs.append(("(2c) 基线+shape+scal", dict(base, shape=1.0, scal=0.5)))
    cfgs.append(("(2d) 基线+shape+scal_c", dict(base, shape=1.0, scal_c=0.5)))
    cfgs.append(("(3a) 基线-gram + shape=2", dict(base, gram=0.0, shape=2.0)))
    cfgs.append(("(3b) 基线-gram + shape=2 + scal=1", dict(base, gram=0.0, shape=2.0,
                                                            scal=1.0)))
    W("\n=== (2) 消融（同真值、同分半、同窗口 [0.70,1.40]；**留出半只报数**）===\n")
    res, dt = _run_jobs(_ablate_eval, args.jobs, cfgs, "新证据消融")
    W("墙钟 %.1f s（jobs=%d）；%d 组配置；调参半 n=%d / 留出半 n=%d\n"
      % (dt, args.jobs, len(cfgs), len(pair.A), len(pair.B)))
    W("%-34s | %-15s | %-15s\n" % ("配置", "调参半 top-1/top-5", "留出半 top-1/top-5"))
    W("%-34s | %-15s | %-15s\n" % ("", "（只这里允许选）", "（只报数）"))
    for r in res:
        W("%-34s | %5.1f%% %5.1f%%   | %5.1f%% %5.1f%%\n"
          % (r["label"], 100.0 * r["a1"] / r["an"], 100.0 * r["a5"] / r["an"],
             100.0 * r["b1"] / r["bn"], 100.0 * r["b5"] / r["bn"]))

    # ---- (3) 选择：**只在调参半上**：取最高的那组（并列桶里取特征更少的），留出半只报一次 ----
    best = max(res, key=lambda r: (r["a1"], r["a5"]))
    bucket = [r for r in res if r["a1"] >= best["a1"] - 1]
    bucket.sort(key=lambda r: (sum(1 for k in SCORE_KEYS if r["w"].get(k)),
                               -(r["a5"]), r["label"]))
    pick = bucket[0]
    W("\n=== (3) 选择（先定规则后用：调参半最高 -1 个样本的并列桶里取非 0 特征最少的）===\n")
    W("调参半最高：%s（%.1f%%）；并列桶 %d 组\n"
      % (best["label"], 100.0 * best["a1"] / best["an"], len(bucket)))
    W("选中：%s\n" % pick["label"])
    W("  -> 调参半 top-1 %.1f%% / top-5 %.1f%%\n"
      % (100.0 * pick["a1"] / pick["an"], 100.0 * pick["a5"] / pick["an"]))
    W("  -> **留出半 top-1 %.1f%% / top-5 %.1f%%**（51 号基线 72.7%% / 88.5%%）\n"
      % (100.0 * pick["b1"] / pick["bn"], 100.0 * pick["b5"] / pick["bn"]))
    W("  ⚠ 这个 tiebreak（非 0 特征最少）是为「只加特征」的网格写的：上面这一组配置里含**删特征**\n"
      "    的项（(3a) 把 gram 关掉了），而 gram 是 51 号实测的主力（关掉掉到 33.3%%）⇒\n"
      "    它把 tiebreak 打偏了。所以下面补一轮**加性网格**，tiebreak 只数**新增特征**。\n")

    # ---- (4) 第二轮选择：**加性**网格（只在 W_SAME 上叠新特征权重） ----
    axes = collections.OrderedDict([("shape", (0.0, 0.5, 1.0)),
                                    ("scal", (0.0, 0.25, 0.5, 1.0)),
                                    ("wconst", (0.0, 0.5))])
    grid = []
    for combo_w in itertools.product(*axes.values()):
        g = dict(base)
        for k, v in zip(axes, combo_w):
            g[k] = v
        if not any(g.get(k) for k in axes):
            continue                          # 全 0 就是基线，已经在 (0) 里报过
        grid.append((wlabel(g), g))
    W("\n=== (4) 加性网格（%d 组，轴 %s；**只在调参半上选**）===\n"
      % (len(grid), " ; ".join("%s=%s" % (k, ",".join("%.3g" % v for v in vs))
                               for k, vs in axes.items())))
    gres, gdt = _run_jobs(_grid_eval, args.jobs, grid, "加性网格")
    W("墙钟 %.1f s（jobs=%d）\n" % (gdt, args.jobs))
    gres.sort(key=lambda r: (-r["top1"], -r["top5"], r["label"]))
    W("  %-6s %-6s  %s\n" % ("top-1", "top-5", "权重"))
    for r in gres[:10]:
        W("  %5.1f%% %5.1f%%  %s\n"
          % (100.0 * r["top1"] / r["n"], 100.0 * r["top5"] / r["n"], r["label"]))
    gbest = gres[0]["top1"]
    gbucket = [r for r in gres if r["top1"] >= gbest - 1]
    gbucket.sort(key=lambda r: (sum(1 for k in axes if r["w"].get(k)),
                                -r["top5"], r["label"]))
    gp = gbucket[0]
    W("并列桶（调参半 top-1 ≥ 最高−1 个样本）：%d 组；按「新增特征最少」选中：%s\n"
      % (len(gbucket), gp["label"]))
    hres, _hdt = _run_jobs(_holdout_eval, args.jobs, [(gp["label"], gp["w"])],
                           "留出半（只报一次）")
    hr = hres[0]
    W("  -> 调参半 top-1 %.1f%% / top-5 %.1f%%\n"
      % (100.0 * gp["top1"] / gp["n"], 100.0 * gp["top5"] / gp["n"]))
    W("  -> **留出半 top-1 %.1f%% / top-5 %.1f%%**（51 号基线 72.7%% / 88.5%%）\n"
      % (100.0 * hr["top1"] / hr["n"], 100.0 * hr["top5"] / hr["n"]))
    if getattr(args, "bucket_holdout", False) and len(gbucket) > 1:
        # 调参半上这些配置**打平**（同一 top-1），所以"选哪一组"本身没有信息。
        # 把整个并列桶的留出半各报一次 —— 这不是选择（选择已经在上面结束），
        # 只是让人看见"打平的那些组在留出半上的散布"，免得把某一个个例当成能力。
        bres, _bdt = _run_jobs(_holdout_eval, args.jobs,
                               [(r["label"], r["w"]) for r in gbucket], "并列桶留出半（只报数）")
        vals = sorted(100.0 * r["top1"] / r["n"] for r in bres)
        base1 = [r for r in res if r["label"].startswith("(0)")][0]
        b0 = 100.0 * base1["b1"] / base1["bn"]
        W("    并列桶 %d 组的留出半 top-1：最低 %.1f%% / 中位 %.1f%% / 最高 %.1f%%"
          "（基线 %.1f%%；桶里 %d 组高于基线）\n"
          % (len(bres), vals[0], vals[len(vals) // 2], vals[-1], b0,
             sum(1 for v in vals if v > b0)))
    if getattr(args, "out", ""):
        # 只落**文本**夹具（AGENTS.md 硬规矩 3：不提交二进制/缓存）。列口径写进表头，
        # 这样任何人拿这份 tsv 都能复核文档里的每个数字，不需要重跑。
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("# 任务 57 · 新证据来源消融（tools/callgraph_fingerprint.py newev）\n")
            fh.write("# 真值：%s；分半：调参半 %d / 留出半 %d；窗口 %s\n"
                     % (", ".join("%s=%d" % kv for kv in pair.prov.items()),
                        len(pair.A), len(pair.B), WIN_SAME))
            fh.write("# 留出半**只报数**，权重只在调参半上选（AGENTS.md 合并纪律 A4）\n")
            fh.write("label\tweights\tn_tune\ttop1_tune\ttop5_tune\tn_hold\ttop1_hold\ttop5_hold\n")
            for r in res:
                fh.write("%s\t%s\t%d\t%.4f\t%.4f\t%d\t%.4f\t%.4f\n"
                         % (r["label"], wlabel(r["w"]) or "(empty)", r["an"],
                            r["a1"] / float(r["an"]), r["a5"] / float(r["an"]),
                            r["bn"], r["b1"] / float(r["bn"]), r["b5"] / float(r["bn"])))
        cov_path = args.out.replace(".tsv", "_coverage.tsv")
        with open(cov_path, "w", encoding="utf-8") as fh:
            fh.write("# 任务 57 · 新特征两侧覆盖率（正对照 348 / 目标集合 12,736）\n")
            fh.write("feature\tpos_hit\tpos_n\ttgt_hit\ttgt_n\tpos_median_size\ttgt_median_size\n")
            for row in cov_rows:
                fh.write("%s\t%d\t%d\t%d\t%d\t%d\t%d\n" % row)
        W("落盘：%s（+ %s）\n" % (args.out, cov_path))
    return 0


# ===========================================================================
# 7.7 正对照 v2（任务 72 / `docs/02` §46）：348 → 965 个可锚定样本
#
# 63 号的判决是「n=165 ⇒ ±6 点 ⇒ 1~2 点的配置差不可判定」。本节只解决**样本量**，
# 不碰打分器、不碰特征定义（AGENTS.md 合并纪律 A1）。
#
# ⚠ 先说清一件事：**348 的天花板不是名字，是锚。**
# `named_linux()` = 三份名字夹具的并集 = 354，其中在 527（引用了 `src/*.rs` Location 的函数）
# 里的就是 348 —— 也就是说"Windows 939 个真名 / Android 524 个真名里还有一堆没进 348"
# 是个误解：那 939 个里能落到 Linux 地址的**已经全在**那三份夹具里了，剩下的 6 个是
# "有名字但不在 527 内"。真正卡住的是 Linux 侧**能被 Location 锚住**的函数只有 527 个，
# 而 §35 的严格 join（每个键两侧各唯一 + 正反向）只覆盖 333 个。
#
# ⇒ 扩样本只能换**锚点判据**，且判据必须只用**编译期常量 / 符号名**，
#   **绝不用任何指纹特征**（否则"用指纹选样本再用指纹评估"= 循环论证，任务书 §1 的红线）。
#   两条新判据（都在本文件里，都只看 `panic::Location` 与符号名）：
#
#   S1 **键集全等锚**：Linux 函数 L 与 Windows 函数 W 引用的 `panic::Location` 三元组集合
#      **完全相同**（`src/*.rs` 与依赖 crate 两族都算），且该键集在 Linux 侧只有一个函数
#      引用、在 Windows 侧也只有一个函数引用。
#      —— 比 §35 的严格 join 松（不要求**每个键**两侧各唯一），但要求**整集相等**，
#      所以仍是"编译期常量相等"，不是相似度、不是分数。
#   S2 **名字链锚**：Windows COFF 名 ≡ Android `.symtab` 名（Rust 哈希归一后**完全相同**），
#      两侧各自只落一个地址，且 Android 地址经**严格 join** 唯一落到一个 Linux 地址。
#
# 入选之后还有两条**复核**（复核只用符号名，不参与入选，见 `cmd_pc` 的输出）：
#   ① 名字三方一致性（S1 的新样本上：Windows COFF 名 vs Android `.symtab` 名）；
#   ② 跨版本复核（early2026 用**同一套 S1** 独立锚一遍，与 win 锚互相比名字）。
# ===========================================================================
PC_NAME_HASH = re.compile(r"17h[0-9a-f]{16}E")
PC_NAME_LLVM = re.compile(r"\.llvm\.\d+$")


def sym_key(name):
    """Rust 旧 mangling 的哈希归一：`…17h<16 hex>E` → `…E`，并去掉 `.llvm.<n>` 后缀。

    为什么不用 `c++filt`：Windows DLL 有 5 万多个函数符号，走 `sym_match.demangle()` 要
    几秒到几十秒；而 `fp.names` 里存的是**原始 mangled 名**，哈希归一在 mangled 串上
    做是等价的（demangle 只是把长度前缀吃掉）。

    ⚠ `.llvm.<n>` 后缀**只在一侧出现**（LLVM 内部化的符号），第一版没剥它，
    于是同一份源码在两份构建里被当成两个名字 ⇒ 38 条假"名字冲突"（§46.3 记了这一条）。
    """
    return PC_NAME_LLVM.sub("", PC_NAME_HASH.sub("E", name))


def fp_names_of(fp):
    """{地址: frozenset(归一后的符号名)}。Linux 是 stripped ⇒ 空表。"""
    out = {}
    for va, names in (fp.names or {}).items():
        ks = frozenset(sym_key(n) for n in names)
        if ks:
            out[va] = ks
    return out


def fp_name_index(fp):
    """{归一后的符号名: {地址…}}（一份构建内；同一个名字可能落多个地址 ⇒ 后面要查重）。"""
    out = collections.defaultdict(set)
    for va, ks in fp_names_of(fp).items():
        for k in ks:
            out[k].add(va)
    return out


def all_loc_keys(fp, va):
    """该函数引用的**全部** `panic::Location` 三元组（`src/*.rs` 与依赖 crate 同列）。

    与 `loc_keys(..., src_only=True)` 的区别是这一条：348 口径只看 `src/`，所以
    依赖 crate（`/rustc/…`、`/home/…/registry/…`）的 Location 整段不在真值里。
    但那些同样是**编译期常量**（`name_bridge.py` 早就把它们算进唯一性判定），
    用它们做锚不引入任何指纹信息。
    """
    return frozenset(loc_keys(fp, va, src_only=False))


def anchor_keyset_eq(src_fp, dst_fp):
    """**S1 键集全等锚** → ({dst_addr: src_addr}, 逐条弃用计数 Counter, {dst: 键数})。

    判据（写死，不看分数）：
      ① dst（Linux）函数的键集 == src（Windows）函数的键集（集合相等，不是包含、不是相似度）；
      ② 该键集在 dst 侧只有一个函数引用，在 src 侧也只有一个函数引用。

    弃用原因分三种，**逐条计数并打印**（任务书 §1 要求"用了几条、弃了几条、为什么弃"）：
      `linux_keyset_shared`  —— 同一个键集被多个 Linux 函数引用（内联/单态化，锚不住）；
      `no_src_keyset`        —— Windows 侧没有任何函数的键集与它相同（对侧被内联/删掉了）；
      `src_keyset_shared`    —— Windows 侧有多个函数同用这一个键集（冷段/垫片，锚不住）。
    """
    S = collections.defaultdict(list)
    for va in src_fp.universe:
        k = all_loc_keys(src_fp, va)
        if k:
            S[k].append(va)
    D = collections.defaultdict(list)
    for va in dst_fp.universe:
        k = all_loc_keys(dst_fp, va)
        if k:
            D[k].append(va)
    out, nk, st = {}, {}, collections.Counter()
    for k, dv in D.items():
        sv = S.get(k, ())
        for va in dv:
            if len(dv) > 1:
                st["linux_keyset_shared"] += 1
            elif not sv:
                st["no_src_keyset"] += 1
            elif len(sv) > 1:
                st["src_keyset_shared"] += 1
            else:
                out[va] = sv[0]
                nk[va] = len(k)
                st["ok"] += 1
    return out, st, nk


def anchor_name_chain(fps, src_tag="win", bridge_tag="android"):
    """**S2 名字链锚** → ({linux_addr: src_addr}, 逐条弃用计数 Counter)。

    判据：src 侧 COFF 名与 bridge 侧 `.symtab` 名**哈希归一后完全相同**，且该名字在
    两份构建里各自只落一个地址；bridge 地址经 `control_pairs(bridge, "linux")` 的
    **严格 Location join** 唯一落到一个 Linux 地址。

    名字相同 ≈ 同一份源码的同一个函数 —— 这条判据**一个指纹特征都没用**，
    而且用的是两份**不同架构、同版本**的构建，独立性比"同构建内再打分"强。
    """
    src_i = fp_name_index(fps[src_tag])
    br_i = fp_name_index(fps[bridge_tag])
    strict, _ = control_pairs(bridge_tag, "linux", fps)      # linux -> bridge
    b2l, dup = {}, set()
    for l, b in strict.items():
        if b in b2l:
            dup.add(b)
        b2l[b] = l
    for b in dup:
        b2l.pop(b, None)                                    # bridge 地址落多个 Linux 函数 ⇒ 弃
    out, st = {}, collections.Counter()
    for name in set(src_i) & set(br_i):
        st["name_in_both"] += 1
        sa, ba = src_i[name], br_i[name]
        if len(sa) != 1:
            st["src_name_not_unique"] += 1
            continue
        if len(ba) != 1:
            st["bridge_name_not_unique"] += 1
            continue
        l = b2l.get(next(iter(ba)))
        if l is None:
            st["no_linux_anchor"] += 1
            continue
        out[l] = next(iter(sa))
        st["ok"] += 1
    st["bridge_addr_multi"] = len(dup)
    return out, st


def linux_side_names(fps):
    """Linux 地址 → 它的**符号名**（归一后），只用 Android `.symtab` + **严格** Location join。

    这条映射**只用于复核**（"同一个 Linux 函数在 Windows 与 Android 两份构建里名字是否一致"），
    不参与入选判据。为什么只走严格 join 而不顺手用 S1 键集全等锚多拿 500 个名字：
    **S1 在跨 ISA 上会锚错**。实测两条口径的"名字冲突"数 —— 严格 join **0** 条，
    S1-eq **5** 条，而那 5 条逐条查过全是 **Android 侧**的错（同一个共享 panic 行
    `oneshot.rs:130` / `linked_list.rs:186` 被另一个函数引用进去了），win 侧都是对的。
    ⇒ 复核要的是**噪声低的那个**，不是覆盖大的那个（§46.3 记了 38 → 5 → 0 的这条排查链）。
    """
    an = fp_names_of(fps["android"])
    out = collections.defaultdict(set)
    strict, _ = control_pairs("android", "linux", fps)      # linux -> android
    for l, a in strict.items():
        out[l] |= an.get(a, frozenset())
    return dict(out)


def positive_control_v2(fps=None, verbose=True):
    """正对照 v2：{linux_addr: win_addr} + 逐类来源 + 弃用计数 + 独立复核。

    组成（**全部只靠 Location 常量与符号名**）：
      S0 = §35 的旧真值（夹具 `win_addr=` ∪ 严格 join）—— 339
      S1 = 键集全等锚的新样本 —— 与 S0 重叠 301（冲突 0），新增 629 里 3 条名字冲突 ⇒ 626
      S2 = 名字链锚的新样本 —— 全部落在 S0/S1 内（实测新增 0）
    S1 上的**名字冲突过滤**：两侧都有名字且**没有任何一个名字重合** ⇒ 弃（记 `name_conflict`）。
    这条同样只看符号名，不看指纹；实测弃 3 条（全是 1 键、键是共享 panic 行的假锚）。
    """
    fps = fps or load_all()
    lin, win = fps["linux"], fps["win"]
    truth0, prov0 = control_pairs("win", "linux", fps)
    eq, eq_st, nk = anchor_keyset_eq(win, lin)
    nc, nc_st = anchor_name_chain(fps)
    wnames = fp_names_of(win)
    lnames = linux_side_names(fps)

    truth, cls, conflict, nkeys = {}, {}, [], {}
    for a, w in sorted(truth0.items()):
        truth[a] = w
        cls[a] = "S0"
    for a, w in sorted(eq.items()):
        if a in truth:
            continue
        wn, ln = wnames.get(w, frozenset()), lnames.get(a, frozenset())
        if wn and ln and not (wn & ln):
            conflict.append(a)                 # 两侧都有名字且完全不重合 ⇒ 假锚
            continue
        truth[a] = w
        cls[a] = "S1"
        nkeys[a] = nk.get(a, 0)
    s2_new = [a for a in nc if a not in truth]
    for a in s2_new:
        truth[a] = nc[a]
        cls[a] = "S2"
    usable = {a: w for a, w in truth.items() if w in win.universe}
    stats = dict(truth0=len(truth0), prov0=prov0, eq=eq_st, name=nc_st,
                 eq_pairs=len(eq), name_pairs=len(nc), s2_new=len(s2_new),
                 name_conflict=len(conflict), total=len(usable))
    if verbose:
        sys.stderr.write("[pc] 正对照 v2：S0=%d S1=%d S2=%d（名字冲突弃 %d）⇒ 可用 %d\n"
                         % (len(truth0), sum(1 for a in cls if cls[a] == "S1"),
                            len(s2_new), len(conflict), len(usable)))
    audit = dict(named_win=len(wnames), n_linux_named=len(lnames),
                 conflict=conflict, nkeys=nkeys, wnames=wnames, lnames=lnames)
    return dict(truth=usable, truth0={a: w for a, w in truth0.items() if w in win.universe},
                cls={a: cls[a] for a in usable}, stats=stats, audit=audit)


PC_CONFIGS = [
    ("BASE（51 号基线 W_SAME）", dict(W_SAME)),
    ("PREREG +scal=0.25（§43.4 预注册选中）", dict(W_SAME, scal=0.25)),
    ("TOP +scal=0.5（并列桶最高）", dict(W_SAME, scal=0.5)),
    ("+scal_c=0.25（泄漏探针）", dict(W_SAME, scal=0.0, scal_c=0.25)),
    ("+scal_c=0.5（泄漏探针）", dict(W_SAME, scal=0.0, scal_c=0.5)),
    ("+shape=1.0（57 号的 F1）", dict(W_SAME, shape=1.0)),
]


def _pc_eval(job):
    """进程池里跑一个（配置 × 人群 × 半）；返回**逐样本**命中向量（配对检验要用）。"""
    label, w, pop, half = job
    ctx = _G["pc"]
    addrs = ctx[pop][half]
    sc, _w, wnd = scorer_for("win", "linux", w)
    pool = _G["pc_pool"]
    h1, h5 = [], []
    for a in addrs:
        r = rank(ctx["fps"]["linux"].feats[a], pool, sc, wnd, _G["topk"])
        t = ctx[pop]["truth"][a]
        h1.append(1 if (r and r[0][1] == t) else 0)
        h5.append(1 if any(c == t for _s, c in r) else 0)
    return dict(label=label, pop=pop, half=half, h1=h1, h5=h5)


def cmd_pc(args):
    """任务 72：正对照 348 → v2，重算留出半、95% CI、与基线的配对差（含 McNemar）。

    **两个口径都要报**（`docs/160` §160.5 的纪律）：`PREREG`（预注册选中 `scal=0.25`）
    与 `TOP`（并列桶最高 `scal=0.5`）。留出半只报数，**不在这里重新选权重**。
    """
    import fingerprint_audit as FA          # 复用 63 号的 CI / McNemar 实现（同一份代码）
    fps = load_all()
    lin, win = fps["linux"], fps["win"]
    pc = positive_control_v2(fps)
    named = named_linux_348()
    # 348 口径 = **旧真值**里落在 348 里的那批（331）；不能用 v2 真值，否则 S1 会把
    # 13 个原本没有真值的已定名函数补进来，72.7 / 76.4 / 78.2 就复现不出来了。
    old = {a: w for a, w in pc["truth0"].items() if a in named}
    oldA, oldB = split_half_addrs(sorted(old))
    A, B = split_half_addrs(sorted(pc["truth"]))
    ctx = dict(fps=fps, v2=dict(truth=pc["truth"], A=A, B=B),
               old348=dict(truth=old, A=oldA, B=oldB))
    _G["pc"], _G["pc_pool"], _G["topk"] = ctx, Pool(win), args.topk
    W = sys.stdout.write
    st = pc["stats"]
    W("=== (1) 正对照 v2：样本从哪来（判据只用 Location 常量/符号名，**不看任何指纹分数**）===\n")
    W("  S0 旧真值（夹具 `win_addr=` ∪ 严格 Location join）            %4d（1:1 可测）\n"
      % st["truth0"])
    s1 = sum(1 for a in pc["cls"] if pc["cls"][a] == "S1")
    W("  S1 键集全等锚（与 S0 重叠 %d、冲突 0；新进 %d；名字冲突弃 %d）      %4d\n"
      % (st["eq_pairs"] - s1, s1, st["name_conflict"], s1))
    W("  S2 名字链锚（新增）                                              %4d\n" % st["s2_new"])
    W("  ------------------------------------------------------------------\n")
    W("  合计可用 %4d（旧口径 348 里的 %d 条都在、另有 %d 条）\n"
      % (st["total"], len(old), st["total"] - len(old)))
    W("  分半：调参半 %d / 留出半 %d（按 Linux 地址 sha1 奇偶；与旧口径同一分半规则）\n"
      % (len(A), len(B)))
    W("\n  S1 的弃用计数（逐条，**按 Linux 函数计**；Linux 侧有 Location 键的函数共 %d 个）：\n"
      % len([a for a in lin.universe if all_loc_keys(lin, a)]))
    st1 = pc["stats"]["eq"]
    W("    用了                     %4d\n" % st1.get("ok", 0))
    W("    弃：键集在 Linux 侧不唯一 %4d（内联/单态化，同一组 Location 被多个函数引用）\n"
      % st1.get("linux_keyset_shared", 0))
    W("    弃：Windows 侧无此键集     %4d（对侧把这段内联掉/删掉了）\n"
      % st1.get("no_src_keyset", 0))
    W("    弃：键集在 Windows 侧不唯一 %4d（冷段 `panic_cold_*` 垫片共用同一组）\n"
      % st1.get("src_keyset_shared", 0))
    W("    另：名字冲突过滤弃         %4d（两侧都有名字、且没有任何名字重合 ⇒ 假锚）\n"
      % st["name_conflict"])
    st2 = pc["stats"]["name"]
    W("\n  S2 的弃用计数：名字两侧都有 %d；弃：win 侧名字不唯一 %d / android 侧不唯一 %d /"
      " android 地址落不到唯一 Linux 函数 %d ⇒ 用了 %d（**新增 %d**）\n"
      % (st2.get("name_in_both", 0), st2.get("src_name_not_unique", 0),
         st2.get("bridge_name_not_unique", 0), st2.get("no_linux_anchor", 0),
         st2.get("ok", 0), st["s2_new"]))
    W("    注：S2 的样本全部落在 S0∪S1 里 ⇒ 名字链这条路是**复核**，不是新增来源。\n")

    # ---- 独立复核 ----
    au = pc["audit"]
    W("\n=== (2) 独立复核（只用符号名，**不参与入选**）===\n")
    chk = collections.Counter()
    chk_cls = collections.Counter()
    for a in pc["truth"]:
        wn = au["wnames"].get(pc["truth"][a], frozenset())
        ln = au["lnames"].get(a, frozenset())
        if not wn:
            chk["win_unnamed"] += 1
        elif not ln:
            chk["linux_name_unknown"] += 1
        elif wn & ln:
            chk["agree"] += 1
        else:
            chk["DISAGREE"] += 1
            chk_cls[pc["cls"][a]] += 1
    W("  ① 名字三方一致：win COFF 名 vs android `.symtab` 名（同一 Linux 地址）\n")
    W("     一致 %d / 冲突 %d / win 侧无名 %d / linux 侧名字未知 %d（n=%d）\n"
      % (chk["agree"], chk["DISAGREE"], chk["win_unnamed"], chk["linux_name_unknown"],
         len(pc["truth"])))
    W("     冲突按来源分：%s（S1/S2 的**新**样本 0 冲突；这 5 条是旧真值 S0 里"
      "跨架构内联差异/Android 侧锚错，逐条见 §46.3）\n"
      % (dict(chk_cls) or "无"))
    eqo, sto, _nko = anchor_keyset_eq(fps["win_old"], lin)
    onames = fp_names_of(fps["win_old"])
    both = [(a, pc["truth"][a], eqo[a]) for a in pc["truth"] if a in eqo]
    bn = [(a, w, o) for a, w, o in both if w in au["wnames"] and o in onames]
    agree = sum(1 for a, w, o in bn if au["wnames"][w] & onames[o])
    shim = sum(1 for a, w, o in bn
               if not (au["wnames"][w] & onames[o]) and fps["win_old"].feats[o]["size"] <= 32)
    W("  ② 跨版本复核：early2026 用**同一套 S1** 独立锚 → %d 条（Linux 侧同一批函数）\n"
      % len(eqo))
    W("     两边都锚到、两边都有名字的 %d 条里：win 名 == old 名 %d 条；不一致 %d 条，"
      "其中 %d 条的 old 侧是 ≤32 B 的 `panic_cold_*` 冷段垫片\n"
      % (len(bn), agree, len(bn) - agree, shim))

    # ---- 评估 ----
    jobs = [(lab, w, pop, half) for lab, w in PC_CONFIGS
            for pop in ("v2", "old348") for half in ("A", "B")]
    res, dt = _run_jobs(_pc_eval, args.jobs, jobs, "正对照 v2 配置评估")
    bykey = {(r["label"], r["pop"], r["half"]): r for r in res}
    W("\n=== (3) 配置评估（墙钟 %.1f s，jobs=%d；留出半**只报数**）===\n" % (dt, args.jobs))
    W("%-38s | %-19s | %-19s | %s\n" % ("配置", "v2 调参半 top-1/5", "v2 留出半 top-1/5",
                                        "旧 348 留出半 top-1/5"))
    rows = []
    for lab, _w in PC_CONFIGS:
        ra, rb = bykey[(lab, "v2", "A")], bykey[(lab, "v2", "B")]
        ro = bykey[(lab, "old348", "B")]
        W("%-38s | %5.1f%% %5.1f%%   | %5.1f%% %5.1f%%   | %5.1f%% %5.1f%%\n"
          % (lab, 100.0 * sum(ra["h1"]) / len(ra["h1"]), 100.0 * sum(ra["h5"]) / len(ra["h5"]),
             100.0 * sum(rb["h1"]) / len(rb["h1"]), 100.0 * sum(rb["h5"]) / len(rb["h5"]),
             100.0 * sum(ro["h1"]) / len(ro["h1"]), 100.0 * sum(ro["h5"]) / len(ro["h5"])))
    # CI + 配对
    nboot = getattr(args, "B", 2000)      # ⚠ 别叫 B：B 是留出半地址表（第一版就撞了这个名字）
    for pop, lab_pop in (("v2", "v2"), ("old348", "旧 348")):
        base = bykey[(PC_CONFIGS[0][0], pop, "B")]["h1"]
        W("\n-- %s 留出半 n=%d：95%% CI + 与基线的**配对**差（bootstrap B=%d；McNemar 精确）--\n"
          % (lab_pop, len(base), nboot))
        W("%-38s %-7s %-18s %-18s %-12s %s\n"
          % ("配置", "top-1", "bootstrap 95% CI", "Wilson 95% CI", "McNemar b/c", "p"))
        for lab, _w in PC_CONFIGS:
            h = bykey[(lab, pop, "B")]["h1"]
            k, n = sum(h), len(h)
            lo, hi = FA.boot_ci(h, B=nboot, seed=72)
            wl, wh = FA.wilson(k, n)
            if pop == "v2" and lab == PC_CONFIGS[0][0]:
                b_, c_, p_ = 0, 0, 1.0
            else:
                b_, c_, p_ = FA.mcnemar_exact(base, h)
            W("%-38s %5.2f%%  [%5.2f%%, %5.2f%%]  [%5.2f%%, %5.2f%%]  b=%d c=%d     p=%.3g%s\n"
              % (lab, 100.0 * k / n, 100.0 * lo, 100.0 * hi, 100.0 * wl, 100.0 * wh, b_, c_, p_,
                 "  **显著**" if (p_ < 0.05 and lab != PC_CONFIGS[0][0]) else ""))
            if lab != PC_CONFIGS[0][0]:
                dlo, dhi = FA.boot_delta_ci(base, h, B=nboot, seed=72)
                W("%-38s %+5.2f pt 配对 CI [%+6.2f, %+6.2f] pt\n"
                  % ("    Δ vs 基线", 100.0 * (k - sum(base)) / n, 100.0 * dlo, 100.0 * dhi))
    # 子群
    W("\n=== (4) v2 留出半按期来源分群（**旧样本有偏没有？**看这张表）===\n")
    W("%-30s %-6s %-22s %s\n" % ("子群", "n", "BASE top-1", "TOP(+scal=.5) top-1"))
    addrs = ctx["v2"]["B"]
    b_h = bykey[(PC_CONFIGS[0][0], "v2", "B")]["h1"]
    t_h = bykey[(PC_CONFIGS[2][0], "v2", "B")]["h1"]
    nk = pc["audit"]["nkeys"]
    groups = [
        ("S0（旧真值那一类）", lambda a: pc["cls"][a] == "S0"),
        ("S1 新增", lambda a: pc["cls"][a] == "S1"),
        ("S1 里带 src/*.rs 键", lambda a: pc["cls"][a] == "S1" and loc_keys(lin, a)),
        ("S1 里只有依赖 crate 键", lambda a: pc["cls"][a] == "S1" and not loc_keys(lin, a)),
        ("S1 里键数=1", lambda a: pc["cls"][a] == "S1" and nk.get(a, 0) == 1),
        ("S1 里键数>=2", lambda a: pc["cls"][a] == "S1" and nk.get(a, 0) >= 2),
    ]
    for label, fn in groups:
        idx = [i for i, a in enumerate(addrs) if fn(a)]
        if not idx:
            continue
        W("%-30s %-6d %5.1f%%                 %5.1f%%\n"
          % (label, len(idx), 100.0 * sum(b_h[i] for i in idx) / len(idx),
             100.0 * sum(t_h[i] for i in idx) / len(idx)))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("# 任务 72 · 正对照 v2（348 → %d）与配置评估\n" % st["total"])
            fh.write("# 判据只用 panic::Location 常量与符号名，**不含任何指纹分数**\n")
            fh.write("# 分半：v2 调参半 %d / 留出半 %d；旧 348 调参半 %d / 留出半 %d\n"
                     % (len(A), len(B), len(oldA), len(oldB)))
            fh.write("# 留出半只报数；PREREG 是 §43.4 预注册选中的 scal=0.25，TOP 是并列桶最高 scal=0.5\n")
            fh.write("pop\thalf\tlabel\tweights\tn\ttop1\ttop5\n")
            for lab, w in PC_CONFIGS:
                for pop in ("v2", "old348"):
                    for half in ("A", "B"):
                        r = bykey[(lab, pop, half)]
                        fh.write("%s\t%s\t%s\t%s\t%d\t%.4f\t%.4f\n"
                                 % (pop, half, lab, wlabel(w), len(r["h1"]),
                                    sum(r["h1"]) / float(len(r["h1"])),
                                    sum(r["h5"]) / float(len(r["h1"]))))
            fh.write("# 也落留出半的**逐样本命中向量**，配对检验/CI 可以原样复算\n")
            for pop in ("v2", "old348"):
                for lab, _w in PC_CONFIGS:
                    r = bykey[(lab, pop, "B")]
                    fh.write("hits\t%s\t%s\t%s\n"
                             % (pop, lab, ",".join(str(x) for x in r["h1"])))
        W("\n落盘：%s\n" % args.out)
        # 逐条样本（文本夹具）：谁进了正对照、来自哪一类、键数几个 —— 任何人都能逐条复核。
        samp = args.out.replace(".tsv", "_samples.tsv")
        half_of = dict([(a, "tune") for a in A] + [(a, "holdout") for a in B])
        with open(samp, "w", encoding="utf-8") as fh:
            fh.write("# 任务 72 · 正对照 v2 逐条样本（%d 行；S0 旧真值 / S1 键集全等锚 / S2 名字链锚）\n"
                     % st["total"])
            fh.write("# 判据只用 panic::Location 常量与符号名，**不含任何指纹分数**（任务书 §2 的红线）\n")
            fh.write("# n_keys = 该 Linux 函数引用的 Location 三元组个数；has_src_key = 是否含 src/*.rs 的键\n")
            fh.write("linux_addr\twin_addr\tclass\thas_src_key\tn_keys\thalf\n")
            for a in sorted(pc["truth"]):
                fh.write("0x%x\t0x%x\t%s\t%s\t%s\t%s\n"
                         % (a, pc["truth"][a], pc["cls"][a],
                            "是" if loc_keys(lin, a) else "否",
                            pc["audit"]["nkeys"].get(a, "-") if pc["cls"][a] == "S1" else "-",
                            half_of.get(a, "-")))
        W("     逐条样本：%s\n" % samp)
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
                                    "ablate", "tune3", "runall", "newev", "pc"])
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
    ap.add_argument("--bucket-holdout", action="store_true",
                    help="newev 把加性网格的**并列桶**在留出半上各报一次（只报数，不参与选择）")
    ap.add_argument("--shards", type=int, default=8, help="runall 的分片数")
    ap.add_argument("--B", type=int, default=2000, help="pc 的 bootstrap 重采样次数（默认 2000）")
    ap.add_argument("--tmp", default="", help="runall 分片文件目录（默认缓存目录下 shards/）")
    a = ap.parse_args(argv)
    return dict(build=cmd_build, verify=cmd_verify, tune2=cmd_tune2, calib=cmd_calib,
                run=cmd_run, merge=cmd_merge, report=cmd_report, probe=cmd_probe,
                ablate=cmd_ablate, tune3=cmd_tune3, runall=cmd_runall,
                newev=cmd_newev, pc=cmd_pc)[a.cmd](a)


if __name__ == "__main__":
    sys.exit(main())
