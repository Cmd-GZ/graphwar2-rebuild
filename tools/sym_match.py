#!/usr/bin/env python3
"""sym_match.py — 跨架构函数匹配器：把 arm64 有名字的函数对到 stripped 的 x86-64 上。

## 为什么需要它

`tools/name_bridge.py` 只走**一条**桥：`panic::Location` 的 `(file, line, col)` 在两个
架构上是同一个常量，所以"引用了同一个 Location 结构体"= 同一个源位置。这条桥很硬，
但它只覆盖**留下了 Location 常量的函数**（panic 点 / 索引越界点 / `unwrap` 点）。
2.0.60 实测：arm64 `.symtab` 里 1,300 个含 `graphwar` 的 FUNC 符号，只有 326 个落到了
唯一的 x86 地址上（`fixtures/name_to_addr.tsv` 的"唯一锚"）。

剩下的约 770 个**两边都有函数体、但没有任何 Location 锚**。它们只能靠**代码结构**对：

* **字面量**（`&'static str` 的 (ptr,len) 槽给的是**精确**文本，不按地址按内容比）；
* **f64 / 稀有整数常量**（0.001、200、15 这类游戏常数的指纹）；
* **函数顺序**：Rust 按 CGU/模块排函数，同一模块内两侧索引**同步平移**，这是本工具
  真正的主力（实测 269 对真值里 79% 的相邻对索引差完全相同）；
* **直接调用图**（谁调谁）—— 理论上最强，**实测贡献 0**，原因见 §"为什么调用图几乎没用"；
* **体量比 / 指令条数比**（只当闸门，不当证据）。

## 独立性（这是 `--verify` 有意义的前提）

`build` **完全不读** `panic::Location`、**完全不读** `fixtures/name_to_addr.tsv`。
名字只来自 arm64 的 `.symtab`，结构特征只来自两份 `.so` 的字节。
所以 `--verify` 拿 326 条唯一锚来比，是两条**互不相干**的证据链在同一个结论上对账，
不是自己证明自己。

## 为什么调用图几乎没用（实测，不是设计取舍）

本节值得单独写，因为"沿调用图做 WL 指纹细化"看起来是最强的办法，实测贡献 **0** 条。

原因是拓扑在这里**不是一对一**的：一个被几十个函数调用的工具函数（`drop_in_place`、
`RawGd::check_rtti`、`memcpy` 包装……）只对应一个 x86 目标，一旦它自己被定下来，
所有**还没定下来**的调用者对它的投票全部指向同一个已被占用的槽——按规则必须丢掉。
于是"从已定邻居投票"在收敛到一半时就自动失效了，剩下能投的只有冷门被调者。

改成**互选**（arm 侧选 x86、x86 侧也选 arm）也没能救回：被这层过滤掉的量太大。
真正的信息在**顺序**里，不在拓扑里。这条要写下来，免得下一个 agent 再走一遍。

## 为什么不能只做 1:1

内联决策在两个架构上**不一样**，所以本工具允许并显式标注两类非 1:1 结论：

    split   1 个 arm64 函数 ↔ 1 个 x86 函数 + 若干「在 arm64 被内联掉」的 x86 函数
    merge   1 个 x86 函数   ↔ 1 个 arm64 函数 + 若干「在 x86 被内联掉」的 arm64 函数

已知实例与**实测结论**（docs/02 §35）：`GraphwarPathCalculator::calculate_paths` 在 arm64
是 `0x3dca1c`（4,812 字节），本工具给出的唯一候选是 x86 `0x467530`（5,408 字节，
体量比 0.89），与 `name_to_addr.tsv` 的 9 个 Location 锚一致 —— 它是**干净的一对一**，
不是"一拆多"。采样器 `0x3c2530` 在 arm64 另有**独立的**一份
`GraphwarShotCalculator::calculate_function`（`0x3b5374`，1,020 字节），本工具也把它对上了。
所以"arm64 把采样器内联进 calculate_paths"是**读反了**：两侧都把采样器留成了独立函数。

## 绝不允许"猜"

本工具唯一的硬规则：**没有候选就写没有；多候选就把全部候选列出来**，不挑一个。
每条结论都带 `evidence` 字段，写清是靠哪几个特征命中的，可以逐条复核。
宁可 300 个唯一 + 400 个无候选，也不要 700 个"看起来像"。

## 输出

`fixtures/sym_match.tsv`：每行一个 arm64 函数（`graphwar` 名字全集，按 arm64 地址排序）

    arm64_name  arm64_addr  arm64_size  x86_candidates  confidence  evidence

* `x86_candidates` 为空 ⇒ **无候选**；单个 ⇒ 唯一匹配；逗号分隔多个 ⇒ **多候选**（全列）
* `confidence`：`unique`（≥2 类独立特征）/ `weak`（只有 1 类）/ `multi` / `none`，
  另有 `split` / `merge` 两种非 1:1 结论（见上）
* `evidence`：`size=1.05;lit=3;f64=1;call=5;via=lit+call` 形式，可逐条复核

## 用法

    ../.venv/bin/python tools/sym_match.py build       # 生成 fixtures/sym_match.tsv（首次 1–2 分钟，缓存命中 1 秒）
    ../.venv/bin/python tools/sym_match.py report      # 统计
    ../.venv/bin/python tools/sym_match.py --verify    # 拿 326 条唯一锚当 ground truth 自校验

`build` 需要 capstone（走仓库 venv）；`report` / `--verify` 只读 TSV，标准库即可。
默认目标：arm64 = `.reference/steam/android-2.0.60/libgraphwarrust.so`（**未 strip 的 2.0.60**），
x86-64 = `.reference/steam/2026-08-28/libgraphwarrust.so`。可用 `--arm64/--x86` 覆盖。

## 与 `name_bridge.py` 的关系

本工具**不复用** `name_bridge.py` 的任何代码，也不读它的中间产物。故意重复实现
ELF 读取 / 符号解析 / 反汇编，是为了让 `--verify` 成为**独立裁判**：两边共用一份实现，
同一个 bug 会在两处同时成立而互相印证，那就不叫验证了。
"""

import argparse
import bisect
import collections
import hashlib
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
import rust_sym  # noqa: E402  「哪条符号属于 graphwarrust」的唯一判据（结构解析，非子串）

# ---------------------------------------------------------------- 常量

DEFAULT_TRUTH = os.path.join(ROOT, "fixtures/name_to_addr.tsv")
DEFAULT_OUT = os.path.join(ROOT, "fixtures/sym_match.tsv")

# 同源函数 arm64/x86 的体量比实测区间；`--verify` 会把实测分布打出来。
SIZE_LO, SIZE_HI = 0.61, 1.19
# 指令条数比。这是比字节体量更紧的不变量：两条流水线编出来的**指令条数**几乎相同
# （实测 269 对真值的 p10/p90 = 0.83/1.17，89% 落在 0.80–1.25），而字节数会被指令宽度
# 差异放大（arm64 定长 4 字节，x86 平均 ~3.5）。
NINSN_LO, NINSN_HI = 0.70, 1.45
# 判定"候选"时放宽到的弱闸；超出这个范围一个候选都不给。
SIZE_LOOSE_LO, SIZE_LOOSE_HI = 0.40, 2.20

MIN_LIT = 16        # 字符串字面量**前缀**键的最短长度：短于它，两版的合并串表邻接差异会污染键
MAX_LIT = 128

# ---- 裁决（§F）与无候选归因（§G）用的常量
ARB_MODBLOCK = "modblock"
ARB_SCORE = "score"
MODBLOCK_MAX = 15       # 模块块整体指派：位掩码 DP 的上限（2^15·15）
SCORE_MARGIN = 1.0      # 候选集内裁决：最高分必须领先次高这么多
CONTEST_MARGIN = 3.0    # 两行抢同一条 x86 槽时，第一名要领先这么多才算"分得开"
RARE_N = 24             # "罕见"字面量/前缀：两侧各 ≤24 个函数引用才算证据
TINY_NINSN = 12         # 指令数 ≤12 视为"太小"（drop_in_place / 转发 shim）
# 控制流指令：一条都没有的函数不可能是函数（`.text` 里的数据/跳转表）
_CFLOW_INSN = re.compile(r"^(b|bl|br|ret|cbz|cbnz|tbz|tbnz|b\.[a-z]+|blr|eret)$")

CONF_UNIQUE = "unique"
CONF_WEAK = "weak"
CONF_MULTI = "multi"
CONF_NONE = "none"
CONF_SPLIT = "split"
CONF_MERGE = "merge"

STATUS = (CONF_UNIQUE, CONF_WEAK, CONF_MULTI, CONF_NONE, CONF_SPLIT, CONF_MERGE)


def _seq_contains(big, small):
    """`small` 是不是 `big` 的一段连续子序列（判"这段代码被内联进了那个函数"）。"""
    n, k = len(big), len(small)
    if k == 0 or k > n:
        return False
    first = small[0]
    for i in range(n - k + 1):
        if big[i] == first and big[i:i + k] == small:
            return True
    return False


def die(msg):
    raise SystemExit("sym_match: " + msg)


def rel(path):
    """报告里只写相对仓库根的路径（绝对路径泄露本机布局，也没法复核）。"""
    try:
        r = os.path.relpath(path, ROOT)
    except ValueError:
        return path
    return r if not r.startswith("..") else path


def _reference_root(start):
    """往上找装着 `.reference/steam/2026-08-28/` 的目录（= 上游 `gw2/`）。

    不能写死 `dirname(ROOT)`：仓库可能被 clone 到任何地方（子 agent 工作区、CI）。
    找不到就地回退，调用者再用 `--arm64/--x86` 显式给路径。
    """
    env = os.environ.get("GW2_ANALYSIS_ROOT")
    if env:
        return env
    cur = start
    for _ in range(6):
        if os.path.isdir(os.path.join(cur, ".reference", "steam", "2026-08-28")):
            return cur
        nxt = os.path.dirname(cur)
        if nxt == cur:
            break
        cur = nxt
    return os.path.dirname(start)


GW2 = _reference_root(ROOT)


def _find_funcstarts():
    """定位上游分析仓库的 Ghidra 函数起点表 `analysis/funcstarts_ghidra_va.pkl`。

    正常布局是 `<上游>/gw2-rebuild/`（兄弟目录），子 agent 的工作区却在
    `.agent-work/<topic>/wt-NN/` 里、层级更深，上游分析仓库在 `.reference/upstream/`。
    以前这里写死 `GW2/analysis/...`，在本仓库（而非上游）里跑 `build` 会直接
    `FileNotFoundError`。搜法：`GW2_FUNCSTARTS` 环境变量 → 逐级上溯 `analysis/`
    → `.reference/upstream/analysis/` → 回退到旧路径（让报错信息指向真实位置）。

    ⚠ 绝不用 `analysis/funcstarts.pkl`（`.eh_frame` 解析出来的表是错的，见 AGENTS.md）。
    """
    env = os.environ.get("GW2_FUNCSTARTS")
    if env:
        return env
    d = ROOT
    for _ in range(6):
        cand = os.path.join(d, "analysis", "funcstarts_ghidra_va.pkl")
        if os.path.exists(cand):
            return cand
        d = os.path.dirname(d)
    cand = os.path.join(GW2, ".reference/upstream/analysis/funcstarts_ghidra_va.pkl")
    if os.path.exists(cand):
        return cand
    return os.path.join(GW2, "analysis/funcstarts_ghidra_va.pkl")


DEFAULT_ARM64 = os.path.join(GW2, ".reference/steam/android-2.0.60/libgraphwarrust.so")
DEFAULT_X86 = os.path.join(GW2, ".reference/steam/2026-08-28/libgraphwarrust.so")
DEFAULT_FUNCSTARTS = _find_funcstarts()


# ---------------------------------------------------------------------------
# 1. ELF64 极简读取（两个架构都吃；只取本工具要的东西）
# ---------------------------------------------------------------------------
class Elf:
    """ELF64 little-endian：节表 + 程序头 + 按虚地址读字节。

    刻意不做完整 ELF 支持。`e_machine` 只用来断言"这不是个奇怪的东西"：
    183 = AArch64，62 = x86-64，其余直接报错，不猜。
    """

    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            self.data = f.read()
        d = self.data
        if d[:4] != b"\x7fELF" or d[4] != 2 or d[5] != 1:
            die("%s: 不是 ELF64 little-endian" % path)
        self.em = struct.unpack_from("<H", d, 18)[0]
        if self.em not in (62, 183):
            die("%s: e_machine=%d 既不是 x86-64(62) 也不是 AArch64(183)" % (path, self.em))
        shoff = struct.unpack_from("<Q", d, 0x28)[0]
        shentsize, shnum, shstrndx = struct.unpack_from("<HHH", d, 0x3A)
        o = shoff + shstrndx * shentsize
        _, _, _, _, sof, ssz, _, _, _, _ = struct.unpack_from("<IIQQQQIIQQ", d, o)
        strtab = d[sof:sof + ssz]
        self.secs = []
        for i in range(shnum):
            o = shoff + i * shentsize
            (nameoff, typ, flags, addr, off, size,
             link, info, align, entsize) = struct.unpack_from("<IIQQQQIIQQ", d, o)
            e = strtab.find(b"\0", nameoff)
            self.secs.append(dict(name=strtab[nameoff:e].decode("utf-8", "replace"),
                                  type=typ, flags=flags, addr=addr, off=off,
                                  size=size, entsize=entsize, link=link))
        self.sec = {s["name"]: s for s in self.secs}
        self.loads = []
        phoff = struct.unpack_from("<Q", d, 0x20)[0]
        phentsize, phnum = struct.unpack_from("<HH", d, 0x36)
        for i in range(phnum):
            o = phoff + i * phentsize
            p_type, _p_flags = struct.unpack_from("<II", d, o)
            p_off, p_va, _pa, p_fsz, _msz = struct.unpack_from("<QQQQQ", d, o + 8)
            if p_type == 1:                                   # PT_LOAD
                self.loads.append((p_va, p_off, p_fsz))
        self.loads.sort()
        # `.rela.dyn` 的 R_*_RELATIVE：文件里指针槽的字节是 0，真值只在 addend 里。
        # 不解析它就只能读到 0，`&'static str` 的 (ptr, len) 槽永远解不出来。
        self.rel = {}
        for s in self.secs:
            if s["type"] != 4 or not s["entsize"]:            # SHT_RELA
                continue
            for i in range(s["size"] // s["entsize"]):
                off, info, add = struct.unpack_from("<QQq", self.data, s["off"] + i * s["entsize"])
                if (info & 0xffffffff) in (8, 1027):          # R_X86_64_RELATIVE / R_AARCH64_RELATIVE
                    self.rel[off] = add & 0xffffffffffffffff

    def ptr_at(self, va):
        """读一个指针槽：优先用重定位的 addend（ELF 里那一格是 0）。"""
        v = self.rel.get(va)
        if v is not None:
            return v
        raw = self.read(va, 8)
        if not raw or len(raw) < 8:
            return None
        return struct.unpack("<Q", raw)[0]

    def va2off(self, va):
        for v, o, sz in self.loads:
            if v <= va < v + sz:
                return o + (va - v)
        return None

    def read(self, va, n):
        o = self.va2off(va)
        if o is None:
            return None
        return self.data[o:o + n]

    def sec_at(self, va):
        for s in self.secs:
            if s["flags"] & 0x2 and s["addr"] <= va < s["addr"] + s["size"]:
                return s
        return None

    def func_symbols(self):
        """[(mangled, addr, size)]：`.symtab` 里已定义的 FUNC。stripped ⇒ []。"""
        if ".symtab" not in self.sec:
            return []
        rows = []
        for line in subprocess.run(["readelf", "-sW", self.path],
                                   capture_output=True, text=True).stdout.splitlines():
            p = line.split()
            if len(p) < 8 or p[3] != "FUNC" or p[1] == "UND":
                continue
            try:
                rows.append((p[7], int(p[1], 16), int(p[2])))
            except ValueError:
                continue
        return rows


# ---------------------------------------------------------------------------
# 2. 函数宇宙（两侧的"函数清单"口径必须写死，否则一切统计都对不上）
# ---------------------------------------------------------------------------
def x86_universe(elf, funcstarts_path):
    """x86-64：Ghidra 派生的函数起点表；大小 = 下一个起点 - 本起点。

    **不用 `.eh_frame`**：`analysis/funcstarts.pkl`（eh_frame 解析）与 Ghidra 只交集 2 条，
    见仓库 `AGENTS.md`「Hard facts」与 docs/02 §12。`fixtures/name_to_addr.tsv` 的 326 条
    唯一锚就是用这张表归属的，本工具必须同口径，否则两边比的不是同一批函数。
    """
    tex = elf.sec[".text"]
    lo, hi = tex["addr"], tex["addr"] + tex["size"]
    raw = pickle.load(open(funcstarts_path, "rb"))
    starts = sorted({int(x, 16) if isinstance(x, str) else int(x) for x in raw})
    starts = [a for a in starts if lo <= a < hi]
    out = {}
    for i, a in enumerate(starts):
        nxt = starts[i + 1] if i + 1 < len(starts) else hi
        out[a] = dict(va=a, size=nxt - a, name=None)
    return out


def arm64_universe(elf):
    """arm64：`.symtab` 的 FUNC 就是权威边界；`size==0` 用下一个符号起点兜底。"""
    tex = elf.sec[".text"]
    lo, hi = tex["addr"], tex["addr"] + tex["size"]
    syms = [(n, a, s) for n, a, s in elf.func_symbols() if lo <= a < hi]
    syms.sort(key=lambda r: r[1])
    starts = [a for _n, a, _s in syms]
    out = {}
    for n, a, s in syms:
        i = bisect.bisect_right(starts, a)
        nxt = starts[i] if i < len(starts) else hi
        out[a] = dict(va=a, size=s if s else nxt - a, name=n)
    return out


# ---------------------------------------------------------------------------
# 3. 名字解析（legacy Rust mangling → module / type / method）
# ---------------------------------------------------------------------------
HASH_SUFFIX = re.compile(r"::h[0-9a-f]{16}$")
CLOSURE = "{{closure}}"
TRAILING_NOISE = {"shim", "{{vtable.shim}}", "{{reify.shim}}"}


def demangle(syms):
    if not syms:
        return []
    out = subprocess.run(["c++filt", "--format", "rust"], input="\n".join(syms),
                         capture_output=True, text=True).stdout
    lines = out.splitlines()
    if len(lines) != len(syms):
        die("c++filt 输出行数 %d != %d" % (len(lines), len(syms)))
    return lines


def split_top(s, sep="::"):
    parts, depth, cur, i = [], 0, "", 0
    while i < len(s):
        if s.startswith(sep, i) and depth == 0:
            parts.append(cur)
            cur = ""
            i += 2
            continue
        c = s[i]
        if c in "<([":
            depth += 1
        elif c in ">)]":
            depth -= 1
        cur += c
        i += 1
    parts.append(cur)
    return parts


def clean_method(segs):
    segs = list(segs)
    while segs and segs[-1] in TRAILING_NOISE:
        segs.pop()
    return "::".join(segs) if segs else None


def parse_name(d):
    """`graphwarrust::<module>::<Type>::<method>` → (module, type, method, is_closure)。

    与 `tools/name_bridge.py::parse_name` **行为一致**是刻意的：本工具的名字键必须和
    ground truth 的 `(module, type, method)` 三列同口径，否则 `--verify` 比的是两套不同的
    名字，结论没有意义。规则本身来自 legacy Rust mangling 的形状：

    * direct       `graphwarrust::mod::Type::method`
    * impl-head    `<graphwarrust::mod::Type as Trait>::method`（crate 名无长度前缀）
    * 嵌套 impl    `graphwarrust::mod::_::<impl Trait for graphwarrust::Type>::method`

    调用方须先用 `rust_sym.is_game_name(mangled)` 把符号筛过一遍（见 `graphwar_symbols`）。
    """
    d = HASH_SUFFIX.sub("", d)
    is_closure = CLOSURE in d
    # 形态 B：`<graphwarrust::mod::Type as Trait>::method`（GDExtension 的 trait impl）。
    # crate 名在 mangled 形态里**没有长度前缀**，所以必须按 `<SELF as TRAIT>` 结构解，
    # 不能靠 `graphwarrust::` 子串——旧版走的是下面的正则兜底，能对是巧合。
    head = rust_sym.impl_head_tail(d)
    if head and "<impl " not in head[0]:
        # 注意 `...::_::<impl Trait for graphwarrust::T>` 这种**嵌套 impl** 不在这里解：
        # 它的外层 `<...>` 是一条路径、末段才是匿名 impl 块，按老规则（下面第三条分支）
        # 得到 `type='_'`，与 `fixtures/name_to_addr.tsv` 的既有列一致。
        self_ty, tail = head
        segs = [x for x in rust_sym.split_top(rust_sym.split_top(self_ty, " as ")[0], "::")
                if x]
        if len(segs) < 2 or segs[0] != rust_sym.DEFAULT_CRATE:
            return None
        module = segs[1]
        owner = segs[2].split("<")[0] if len(segs) > 2 and segs[2][:1].isupper() else None
        rest = [x for x in rust_sym.split_top(tail, "::") if x]
        if CLOSURE in rest:
            rest = rest[:rest.index(CLOSURE)]
        method = clean_method(rest) or owner
        return module, owner, method, is_closure
    if d.startswith("graphwarrust::") and "<impl " not in d:
        segs = split_top(d)
        if len(segs) < 3:
            return None
        module = segs[1]
        rest = segs[2:]
        if CLOSURE in rest:
            rest = rest[:rest.index(CLOSURE)]
        owner = rest[0] if rest and rest[0][:1].isupper() else None
        if owner:
            rest = rest[1:]
        method = clean_method(rest) or owner
        return module, owner, method, is_closure
    m = re.search(r"graphwarrust::([A-Za-z0-9_]+)::([A-Za-z0-9_]+)", d)
    if not m:
        return None
    module, owner = m.group(1), m.group(2)
    if owner.startswith("__"):
        m2 = re.search(r"graphwarrust::([A-Za-z0-9_]+)::([A-Z][A-Za-z0-9_]*)", d)
        if m2:
            module, owner = m2.group(1), m2.group(2)
    method = None
    if ">::" in d:
        method = clean_method(split_top(d[d.rindex(">::") + 3:]))
    if method and CLOSURE in method:
        method = method.split("::")[0]
    return module, owner, method, is_closure


def name_key(demangled):
    """→ (module, Owner, method)，与 `name_to_addr.tsv` 的三列同口径。"""
    p = parse_name(demangled)
    if not p:
        return None
    module, owner, method, is_closure = p
    if not method:
        return None
    if is_closure:
        method = method + "::{{closure}}"
    return (module, owner, method)


# ---------------------------------------------------------------------------
# 4. 数据引用 → 跨架构可比的键
# ---------------------------------------------------------------------------
def literal_text(elf, va):
    """把 `va` 处的一串可打印 ASCII 读出来（最多 MAX_LIT 字节），不够 4 字节返回 None。"""
    s = elf.sec_at(va)
    if s is None or s["name"] != ".rodata":
        return None
    b = elf.read(va, MAX_LIT)
    if not b:
        return None
    out = bytearray()
    for c in b:
        if 32 <= c < 127:
            out.append(c)
        else:
            break
    return out.decode() if len(out) >= 4 else None


def str_slot_text(elf, va):
    """`&'static str` 的 16 字节 (ptr, len) 槽（x86 在 `.data.rel.ro`，arm64 布局相同）。

    返回它指向的真实文本；不是槽就 None。两版用**同一套**规则，否则字面量集合不可比。
    """
    s = elf.sec_at(va)
    if s is None or s["name"] not in (".data.rel.ro", ".data", ".got"):
        return None
    raw = elf.read(va, 16)
    if not raw or len(raw) < 16:
        return None
    ptr = elf.ptr_at(va)
    if ptr is None:
        return None
    ln = struct.unpack("<Q", raw[8:16])[0]
    if not (0 < ln <= MAX_LIT):
        return None
    tgt = elf.sec_at(ptr)
    if tgt is None or tgt["name"] != ".rodata":
        return None
    b = elf.read(ptr, ln)
    if not b or len(b) != ln:
        return None
    if any(c < 32 or c >= 127 for c in b):
        return None
    return b.decode()


def f64_at(elf, va):
    """`va` 处 8 字节是否是一个"人类写的" f64 常数（跨架构可比）。"""
    raw = elf.read(va, 8)
    if not raw or len(raw) < 8:
        return None
    v = struct.unpack("<d", raw)[0]
    if v != v or v in (0.0, 1.0, -1.0):
        return None
    a = abs(v)
    if not (1e-9 < a < 1e9):
        return None
    return round(v, 9)


def classify_target(elf, va):
    """一个被引用的数据地址 → (kind, value)。

    kind 三档，精度递减、用途不同：

      `slot`  `.rodata` 里某个 `&'static str` 的 (ptr, len) 槽 ⇒ **精确**的字面量文本，
              长度由 len 给出，不受合并串表的邻接影响。最强的一档。
      `pfx`   直接指向 `.rodata` 的一段可打印字节 ⇒ 只能当**前缀**用：两版合并串表的
              邻接关系不同，长于 MIN_LIT 的部分才可比。
      `f64`   8 字节恰好是一个"人类写的"双精度常数。
    """
    s = elf.sec_at(va)
    if s is None:
        return None, None
    if s["name"] in (".data.rel.ro", ".data", ".got"):
        t = str_slot_text(elf, va)
        if t is not None:
            return "slot", t
        v = f64_at(elf, va)
        if v is not None:
            return "f64", v
        return None, None
    if s["name"] == ".rodata":
        t = literal_text(elf, va)
        if t is not None:
            return "pfx", t
        v = f64_at(elf, va)
        if v is not None:
            return "f64", v
    return None, None


# ---------------------------------------------------------------------------
# 5. 逐函数特征提取（capstone）
# ---------------------------------------------------------------------------
def extract_x86(elf, universe):
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64
    from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_REG_RIP

    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    out = {}
    for va, f in universe.items():
        body = elf.read(va, f["size"])
        if not body:
            continue
        calls, refs, consts, n = [], [], set(), 0
        mns = []
        for i in cs.disasm(body, va):
            n += 1
            mns.append(i.mnemonic)
            if i.mnemonic == "call" and i.operands and i.operands[0].type == X86_OP_IMM:
                calls.append(i.operands[0].imm & 0xffffffffffffffff)
                continue
            for op in i.operands:
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    refs.append((i.address + i.size + op.mem.disp) & 0xffffffffffffffff)
                elif op.type == X86_OP_IMM:
                    v = op.imm
                    if -0x80000000 <= v <= 0xffffffff:
                        if abs(v) >= 0x100 and elf.sec_at(v) is None:
                            consts.add(v)
                    else:
                        d = struct.unpack("<d", struct.pack("<Q", v & 0xffffffffffffffff))[0]
                        if d == d and 1e-9 < abs(d) < 1e9:
                            refs.append(("f64imm", round(d, 9)))
        out[va] = dict(n=n, calls=calls, refs=refs, consts=consts, mn=mns)
    return out


def extract_arm64(elf, universe):
    from capstone import Cs, CS_ARCH_ARM64
    from capstone.arm64 import ARM64_OP_IMM, ARM64_OP_MEM, ARM64_OP_REG

    cs = Cs(CS_ARCH_ARM64, 0)
    cs.detail = True
    WRITE = 2                       # CS_AC_WRITE
    out = {}
    for va, f in universe.items():
        body = elf.read(va, f["size"])
        if not body:
            continue
        calls, refs, consts, n = [], [], set(), 0
        mns = []
        addr = {}                   # reg -> 已知的绝对地址（adrp / adrp+add 的结果）
        for i in cs.disasm(body, va):
            n += 1
            mns.append(i.mnemonic)
            ops = i.operands
            mn = i.mnemonic
            used = False
            if mn == "bl" and ops and ops[0].type == ARM64_OP_IMM:
                calls.append(ops[0].imm)
                used = True
            elif mn == "adrp" and len(ops) >= 2 and ops[1].type == ARM64_OP_IMM:
                addr[ops[0].reg] = ops[1].imm
                used = True
            elif mn in ("add", "sub") and len(ops) >= 3 and ops[1].type == ARM64_OP_REG \
                    and ops[2].type == ARM64_OP_IMM and ops[1].reg in addr:
                # 注意：`add x9, x9, #off` 的源寄存器和目的寄存器是同一个，所以**先取后写**
                tgt = addr[ops[1].reg] + (ops[2].imm if mn == "add" else -ops[2].imm)
                refs.append(tgt)
                addr[ops[0].reg] = tgt
                used = True
            elif len(ops) >= 2 and ops[1].type == ARM64_OP_MEM:
                mem = ops[1].mem
                if mem.base in addr:
                    refs.append(addr[mem.base] + mem.disp)
                elif i.reg_name(mem.base) == "pc":              # 字面量池
                    refs.append(i.address + mem.disp)
            elif mn in ("movz", "movn", "mov") and len(ops) >= 2 \
                    and ops[1].type == ARM64_OP_IMM:
                v = ops[1].imm
                if mn == "movn":
                    v = ~v
                if len(ops) >= 3 and ops[2].type == ARM64_OP_IMM and ops[2].imm in (16, 32, 48):
                    v <<= ops[2].imm
                if abs(v) >= 0x100:
                    consts.add(v)
            if not used:                # 任何写到寄存器的指令都让该寄存器上的地址失效
                for op in ops:
                    if op.type == ARM64_OP_REG and op.access & WRITE:
                        addr.pop(op.reg, None)
        out[va] = dict(n=n, calls=calls, refs=refs, consts=consts, mn=mns)
    return out


def body_sig(mns):
    """函数体的**形状签名**：助记符序列的 sha1 前 16 位。

    用途只有一个：判定"两个函数编出来是不是同一串指令"（ICF / 同构孪生）。
    故意只取助记符、不取操作数——寄存器分配和立即数在两个架构间不同，取了反而不可比；
    而同一个源函数在同一架构上被 ICF 合并时，助记符序列是**完全相同**的。
    """
    if not mns:
        return None
    h = hashlib.sha1()
    for m in mns:
        h.update(m.encode("utf-8"))
        h.update(b"|")
    return h.hexdigest()[:16]


def build_features(elf, universe, raw, starts, keep_seq=False):
    """把原始反汇编结果压成可比的键：lits / f64s / calls（归一到函数起点）。

    `keep_seq=True` 时额外保留助记符序列 `mseq`（只有 arm64 侧要——判"内联进来的一段"
    用的是同一架构内的序列包含关系，跨架构不可比）。
    """
    lit_cache = {}

    def lit_of(va):
        if va not in lit_cache:
            lit_cache[va] = classify_target(elf, va)
        return lit_cache[va]

    for va, f in universe.items():
        r = raw.get(va)
        if r is None:
            f["lits"], f["pfxs"], f["f64s"], f["calls"], f["consts"], f["ninsn"] = \
                frozenset(), frozenset(), frozenset(), (), frozenset(), 0
            f["sig"] = None
            f["bsig"] = None
            if keep_seq:
                f["mseq"] = ()
            continue
        lits, pfxs, f64s = set(), set(), set()
        for t in r["refs"]:
            if isinstance(t, tuple):                       # x86 movabs 里的 f64
                f64s.add(t[1])
                continue
            kind, val = lit_of(t)
            if kind == "slot":
                if len(val) >= 4:
                    lits.add(val)
            elif kind == "pfx":
                if len(val) >= MIN_LIT:
                    pfxs.add(val[:MIN_LIT])
            elif kind == "f64":
                f64s.add(val)
        calls = []
        for c in r["calls"]:
            i = bisect.bisect_right(starts, c) - 1
            if i >= 0 and c - starts[i] < 0x4000:
                calls.append(starts[i])                    # 归一到函数起点
        f["lits"] = frozenset(lits)
        f["pfxs"] = frozenset(pfxs)
        f["f64s"] = frozenset(f64s)
        f["calls"] = tuple(sorted(calls))
        f["consts"] = frozenset(r["consts"])
        f["ninsn"] = r["n"]
        f["sig"] = body_sig(r["mn"])
        # 逐字节签名：`sig` 只比助记符形状，`bsig` 比**指令编码本身**。
        # 判定 ICF（"两份同构函数被合并成一份"）必须用 `bsig`——形状相同、立即数/位移
        # 不同的函数到处都是（drop_in_place<T> 就是），用形状判 ICF 会大面积假阳性。
        b = elf.read(va, f["size"])
        f["bsig"] = hashlib.sha1(b).hexdigest()[:16] if b else None
        if keep_seq:
            f["mseq"] = tuple(r["mn"])

    callers = collections.defaultdict(list)
    for va, f in universe.items():
        for c in set(f["calls"]):
            callers[c].append(va)
    for va, f in universe.items():
        f["callers"] = tuple(sorted(callers.get(va, ())))
    return universe


# ---------------------------------------------------------------------------
# 6. 缓存（提取一次约 2 分钟，迭代时不该反复付这个代价）
# ---------------------------------------------------------------------------
# 缓存里存的是**特征提取的完整结果**，所以提取代码一改就必须让旧缓存失效。
# 以前 key 只有 `tag + 二进制大小`，改了提取逻辑却还吃旧缓存（实测踩过：同一份 `.so`
# 用新旧代码各跑一次，unique 差 2 条、multi 差 3 条，而且不报错）。
CACHE_VERSION = "v4-bsig"


def cache_dir():
    d = os.environ.get("SYM_MATCH_CACHE") or os.path.join(ROOT, ".agent-work/symmatch/cache")
    os.makedirs(d, exist_ok=True)
    return d


def load_side(arch, path, universe_path, tag):
    """→ {va: feature dict}。缓存命中就不重新反汇编。"""
    st = os.stat(path)
    key = "%s-%s-%s" % (CACHE_VERSION, tag, st.st_size)
    cf = os.path.join(cache_dir(), key + ".pkl")
    if os.path.exists(cf):
        try:
            with open(cf, "rb") as fh:
                saved = pickle.load(fh)
            if saved.get("mtime") == int(st.st_mtime) and saved.get("size") == st.st_size:
                return saved["universe"]
        except Exception:
            pass
    elf = Elf(path)
    if arch == "x86":
        uni = x86_universe(elf, universe_path)
        raw = extract_x86(elf, uni)
        build_features(elf, uni, raw, sorted(uni))
    else:
        uni = arm64_universe(elf)
        raw = extract_arm64(elf, uni)
        build_features(elf, uni, raw, sorted(uni), keep_seq=True)
    try:
        with open(cf, "wb") as fh:
            pickle.dump(dict(mtime=int(st.st_mtime), size=st.st_size, universe=uni), fh)
    except Exception:
        pass
    return uni


# ---------------------------------------------------------------------------
# 7. 匹配
# ---------------------------------------------------------------------------
STRICT_LO, STRICT_HI = SIZE_LO, SIZE_HI          # 0.61–1.19：实测区间
LOOSE_LO, LOOSE_HI = SIZE_LOOSE_LO, SIZE_LOOSE_HI   # 0.45–1.80：只用来"筛候选"


def _lis(pairs):
    """按第一坐标排好序的 (i, j) 取 j 的最长上升子序列（保留 i 单调的那一批）。"""
    tails, idx, prev = [], [], [-1] * len(pairs)
    for k, (_i, j) in enumerate(pairs):
        import bisect as _b
        t = _b.bisect_left(tails, j)
        if t == len(tails):
            tails.append(j)
            idx.append(k)
        else:
            tails[t] = j
            idx[t] = k
        prev[k] = idx[t - 1] if t > 0 else -1
    out, cur = [], (idx[-1] if idx else -1)
    while cur >= 0:
        out.append(pairs[cur])
        cur = prev[cur]
    return out[::-1]


class Matcher:
    """把 arm64 的图对到 x86 的图上。

    五条证据，按可靠度排序，全部记录到 `why[(a, x)]` 里，结论要能逐条复核：

      `lit`   字面量：`.rodata` 的 `&'static str` 槽给的是**精确**文本（不受合并串表
              邻接影响）；直接指向 `.rodata` 的只能当 >= 24 字节前缀用。两侧都只被一个
              函数引用 ⇒ 配对。
      `f64`   浮点常数集合（0.001 / 15 / 25 / 200 这类游戏常数的指纹）。
      `call`  调用图：a 的被调者/调用者已配上 y，且 x86 侧**互选**（reciprocal best match）。
      `order` Rust 按 CGU/模块顺序排函数：同一模块内**两侧索引同步平移**。实测 286 对
              ground truth 里 79% 的相邻对索引差完全相同，这是本项目里最强的"批量"信号。
      `size`  体量比落在 0.61–1.19（只当闸门，不当证据）。

    绝不做的事：给没有候选的函数编一个候选；在多候选里挑一个"看起来最像"的。
    """

    def __init__(self, arm, x86):
        self.arm, self.x86 = arm, x86
        self.A = sorted(arm)
        self.X = sorted(x86)
        self.ai = {a: i for i, a in enumerate(self.A)}
        self.xi = {x: i for i, x in enumerate(self.X)}
        self.mp = {}
        self.rev = {}
        self.why = collections.defaultdict(set)
        self.votes = collections.defaultdict(collections.Counter)
        self.split_extra = collections.defaultdict(list)    # arm a -> [x86 u]（arm64 内联）
        self.merge_extra = collections.defaultdict(list)    # x86 x -> [arm b]（x86 内联）
        self.arbi = {}          # (a, x) -> 裁决证据串（见 §F）
        self.log = []

    # ---------------------------------------------------------------- 基础
    def ratio(self, a, x):
        sz = self.x86[x]["size"]
        return self.arm[a]["size"] / sz if sz else None

    def nratio(self, a, x):
        n = self.x86[x].get("ninsn") or 0
        if not n:
            return None
        return (self.arm[a].get("ninsn") or 0) / n

    def gate(self, a, x, lo=LOOSE_LO, hi=LOOSE_HI, nlo=None, nhi=None):
        r = self.ratio(a, x)
        if r is None or not (lo <= r <= hi):
            return False
        if nlo is None:
            return True
        q = self.nratio(a, x)
        return q is None or nlo <= q <= nhi

    def strict(self, a, x):
        return self.gate(a, x, STRICT_LO, STRICT_HI)

    def take(self, a, x, kind, strict=False):
        if a in self.mp or x in self.rev:
            return False
        if not (self.strict(a, x) if strict else self.gate(a, x)):
            return False
        self.mp[a] = x
        self.rev[x] = a
        self.why[(a, x)].add(kind)
        return True

    # ---------------------------------------------------------------- 索引
    def _lit_index(self, key):
        idx = collections.defaultdict(list)
        for va, f in self.arm.items():
            for s in f[key]:
                idx[s].append(va)
        return idx

    def _x_lit_index(self, key):
        idx = collections.defaultdict(list)
        for va, f in self.x86.items():
            for s in f[key]:
                idx[s].append(va)
        return idx

    # ---------------------------------------------------------------- A. 字面量
    def seed_literals(self):
        """两轮：先"两侧都只被一个函数引用"的独占字面量，再"多数字面量指向同一个"的聚合票。"""
        n = 0
        for key in ("lits", "pfxs"):                 # 先精确槽，再前缀
            ia = self._lit_index(key)
            ix = self._x_lit_index(key)
            for s, al in ia.items():
                xl = ix.get(s)
                if not xl or len(al) != 1 or len(xl) != 1:
                    continue
                if self.take(al[0], xl[0], "lit"):
                    n += 1
        n += self._seed_by_votes()
        self.log.append(("A:lit", n, len(self.mp)))
        return n

    def _seed_by_votes(self):
        """一个函数引用了一组字面量，其中若干在 x86 侧也只被同一个函数引用 ⇒ 累加成票。"""
        ix_e = self._x_lit_index("lits")
        ix_p = self._x_lit_index("pfxs")
        got = 0
        for a, fa in self.arm.items():
            if a in self.mp:
                continue
            c = collections.Counter()
            for s in fa["lits"]:
                for x in ix_e.get(s, ()):
                    c[x] += 3
            for s in fa["pfxs"]:
                for x in ix_p.get(s, ()):
                    c[x] += 2
            cand = [(x, v) for x, v in c.items() if x not in self.rev and self.gate(a, x)]
            if not cand:
                continue
            cand.sort(key=lambda t: (-t[1], t[0]))
            if cand[0][1] < 3:
                continue
            if len(cand) > 1 and cand[1][1] * 2 > cand[0][1]:
                continue
            if self.take(a, cand[0][0], "lit"):
                got += 1
        return got

    # ---------------------------------------------------------------- B2. 常量
    def seed_consts(self):
        """稀有整数常量：`movabs` / `movz` 搬来的魔数（端口、位宽、掩码、阈值）。

        只要一个值在**两侧都罕见**（各不超过 RARE 个函数引用），共享它就不是巧合。
        """
        RARE = 6
        ia = collections.defaultdict(list)
        ix = collections.defaultdict(list)
        for a, f in self.arm.items():
            for v in f["consts"]:
                ia[v].append(a)
        for x, f in self.x86.items():
            for v in f["consts"]:
                ix[v].append(x)
        keys = [v for v, l in ia.items()
                if len(l) <= RARE and v in ix and len(ix[v]) <= RARE]
        got = 0
        for a, fa in self.arm.items():
            if a in self.mp:
                continue
            c = collections.Counter()
            for v in fa["consts"]:
                if v in keys:
                    for x in ix[v]:
                        c[x] += 1
            cand = [(x, k) for x, k in c.items() if x not in self.rev and self.gate(a, x)]
            if len(cand) != 1 or cand[0][1] < 2:
                continue
            if self.take(a, cand[0][0], "f64"):
                got += 1
        self.log.append(("B2:const", got, len(self.mp)))
        return got

    # ---------------------------------------------------------------- B. f64
    def seed_f64(self):
        """f64 常数指纹：|S|>=2，且 x86 侧只有**一个**函数覆盖其中 80% 以上。"""
        n = 0
        f64_x = collections.defaultdict(set)
        for x, f in self.x86.items():
            for v in f["f64s"]:
                f64_x[v].add(x)
        rare = {v: s for v, s in f64_x.items() if len(s) <= 8}
        for a, fa in self.arm.items():
            if a in self.mp or not fa["f64s"]:
                continue
            c = collections.Counter()
            for v in fa["f64s"]:
                for x in f64_x.get(v, ()):
                    c[x] += 1
            need = max(1, (len(fa["f64s"]) * 4 + 4) // 5) if len(fa["f64s"]) > 1 else 1
            hits = [x for x, k in c.items()
                    if k >= need and self.gate(a, x) and any(v in rare for v in fa["f64s"])]
            if len(hits) == 1 and self.take(a, hits[0], "f64"):
                n += 1
        self.log.append(("B:f64", n, len(self.mp)))
        return n

    # ---------------------------------------------------------------- C. 调用图
    def propagate(self, rounds=6, minv=2, margin=2.0):
        """互选式传播：arm 侧给 a 投票，x86 侧也给 x 投票，两边互相选中才算数。

        单向投票会把"一对多"的枢纽（那些被几十个函数调用的工具函数）变成磁铁，
        互选能压掉绝大部分这种噪声。
        """
        total = 0
        for _ in range(rounds):
            va = collections.defaultdict(collections.Counter)
            vx = collections.defaultdict(collections.Counter)
            for a, f in self.arm.items():
                if a in self.mp:
                    continue
                for cal in set(f["calls"]):
                    y = self.mp.get(cal)
                    if y is not None:
                        va[a][y] += 2
                for ca in f["callers"]:
                    y = self.mp.get(ca)
                    if y is not None:
                        va[a][y] += 1
            for x, f in self.x86.items():
                if x in self.rev:
                    continue
                for cal in set(f["calls"]):
                    b = self.rev.get(cal)
                    if b is not None:
                        vx[x][b] += 2
                for ca in f["callers"]:
                    b = self.rev.get(ca)
                    if b is not None:
                        vx[x][b] += 1
            got = 0
            for a, c in va.items():
                cand = [(y, v) for y, v in c.items() if y not in self.rev and self.gate(a, y)]
                if not cand:
                    continue
                cand.sort(key=lambda t: (-t[1], t[0]))
                y, v = cand[0]
                if v < minv or (len(cand) > 1 and cand[1][1] * margin > v):
                    continue
                back = [(b, w) for b, w in vx.get(y, {}).items()
                        if b not in self.mp and self.gate(b, y)]
                if not back:
                    continue
                back.sort(key=lambda t: (-t[1], t[0]))
                if back[0][0] != a:
                    continue                                  # 不是互选
                if len(back) > 1 and back[1][1] * margin > back[0][1]:
                    continue
                if self.take(a, y, "call"):
                    got += 1
            total += got
            if not got:
                break
        self.log.append(("C:call", total, len(self.mp)))
        return total

    # ---------------------------------------------------------------- D. 顺序
    def _anchor_diagonals(self, max_cluster=40):
        """把已配对的 (a,x) 按 `x86索引 - arm索引` 分组 = 同一条"对角线"= 同一个模块块。"""
        groups = collections.defaultdict(list)
        for a, x in self.mp.items():
            groups[self.xi[x] - self.ai[a]].append(self.ai[a])
        out = []
        for d, idxs in groups.items():
            idxs.sort()
            cur = [idxs[0]]
            for v in idxs[1:]:
                if v - cur[-1] <= max_cluster:
                    cur.append(v)
                else:
                    out.append((d, cur))
                    cur = [v]
            out.append((d, cur))
        return out

    def _pair_at(self, i, d):
        j = i + d
        if 0 <= i < len(self.A) and 0 <= j < len(self.X):
            return self.A[i], self.X[j]
        return None, None

    def diag_extend(self):
        """沿对角线把两侧索引同步平移的整段一次性配出来。

        只在"簇内"和"簇两端"延伸，遇到体量比出闸就停 —— 停下来的地方就是内联决策
        不同的地方，那正是 `order` 证据失效、必须靠别的证据的位置。
        """
        n = 0
        for d, cl in self._anchor_diagonals():
            if len(cl) < 2:
                continue
            for i in range(cl[0], cl[-1] + 1):
                a, x = self._pair_at(i, d)
                if a is not None and self.take(a, x, "order"):
                    n += 1
            i = cl[0] - 1
            while True:
                a, x = self._pair_at(i, d)
                if a is None or a in self.mp or x in self.rev or not self.gate(a, x):
                    break
                if self.take(a, x, "order"):
                    n += 1
                i -= 1
            i = cl[-1] + 1
            while True:
                a, x = self._pair_at(i, d)
                if a is None or a in self.mp or x in self.rev or not self.gate(a, x):
                    break
                if self.take(a, x, "order"):
                    n += 1
                i += 1
        self.log.append(("D:order", n, len(self.mp)))
        return n

    # ---------------------------------------------------------------- E. 空档对齐
    def dp_fill(self, max_gap=400, max_cells=90000, use_lis=False):
        """对角线之间的空档用 DP 对齐：允许两侧各自跳过若干函数（= 被内联掉的那些）。

        匹配得分只认"体量/指令数接近度 + 特征重合 + 入度相同"，gap 罚分固定。跳过的函数
        不丢掉，而是记成 `split` / `merge` 的候选证据——DP 走过的 gap 就是内联的位置。
        """
        import math
        # 按 (arm 索引, x86 索引) 走一遍，遇到 x86 索引**倒退**就断成一段。
        #
        # 这里**不能**用全局 LIS：两个架构的模块（CGU）顺序并不一致，LIS 会把"顺序相反"
        # 的那整批锚点当异常丢掉，于是那些模块块之间成了没有锚点的长空档，DP 一次也补不上
        # （实测因此整块漏掉过 40 个函数一段）。
        pmap = {(self.ai[a], self.xi[x]): (a, x) for a, x in self.mp.items()}
        pairs = sorted(pmap)
        if use_lis:
            # 另一条口径：全局长上升子序列。它会把"模块顺序相反"的那批锚点丢掉，
            # 但在顺序大体一致的区段里给出的空档更干净。两种都跑，先到先得，互不覆盖。
            runs = [_lis(pairs)]
        else:
            runs, cur = [], []
            for ij in pairs:
                if cur and ij[1] <= cur[-1][1]:
                    runs.append(cur)
                    cur = []
                cur.append(ij)
            if cur:
                runs.append(cur)

        n = 0
        GAP = -0.9
        for run in runs:
            for (i1, j1), (i2, j2) in zip(run, run[1:]):
                da, dx = i2 - i1, j2 - j1
                if da < 2 or dx < 2 or da > max_gap or dx > max_gap:
                    continue
                if da * dx > max_cells:
                    continue
                a1, x1 = pmap[(i1, j1)]
                B = self.A[i1 + 1:i2]
                Y = self.X[j1 + 1:j2]
                n1, n2 = len(B), len(Y)
                NEG = float("-inf")
                OPEN, EXT = -1.8, -0.25        # 仿射空档：开一次贵、延长便宜
                # 三个状态：M=配对，GX=x86 多出一个（arm64 侧内联掉了），GY=arm64 多出一个
                M = [[NEG] * (n2 + 1) for _ in range(n1 + 1)]
                GX = [[NEG] * (n2 + 1) for _ in range(n1 + 1)]
                GY = [[NEG] * (n2 + 1) for _ in range(n1 + 1)]
                BM = [[None] * (n2 + 1) for _ in range(n1 + 1)]
                BX = [[None] * (n2 + 1) for _ in range(n1 + 1)]
                BY = [[None] * (n2 + 1) for _ in range(n1 + 1)]
                M[0][0] = 0.0
                for j in range(1, n2 + 1):
                    GX[0][j] = (M[0][j - 1] + OPEN + EXT
                                if GX[0][j - 1] == NEG else GX[0][j - 1] + EXT)
                    if M[0][j - 1] + OPEN + EXT > GX[0][j]:
                        GX[0][j] = M[0][j - 1] + OPEN + EXT
                    BX[0][j] = "M" if GX[0][j] == M[0][j - 1] + OPEN + EXT else "GX"
                for i in range(1, n1 + 1):
                    GY[i][0] = (M[i - 1][0] + OPEN + EXT
                                if GY[i - 1][0] == NEG else GY[i - 1][0] + EXT)
                    if M[i - 1][0] + OPEN + EXT > GY[i][0]:
                        GY[i][0] = M[i - 1][0] + OPEN + EXT
                    BY[i][0] = "M" if GY[i][0] == M[i - 1][0] + OPEN + EXT else "GY"
                for i in range(1, n1 + 1):
                    fa = self.arm[B[i - 1]]
                    for j in range(1, n2 + 1):
                        fx = self.x86[Y[j - 1]]
                        r = fa["size"] / fx["size"] if fx["size"] else 0
                        if not (LOOSE_LO <= r <= LOOSE_HI) or r <= 0:
                            s = -9.0
                        else:
                            s = -abs(math.log(r))
                            q = self.nratio(B[i - 1], Y[j - 1])
                            if q:
                                s -= 1.2 * abs(math.log(q))
                            s += 1.0 * len(fa["lits"] & fx["lits"])
                            s += 0.7 * len(fa["pfxs"] & fx["pfxs"])
                            s += 0.5 * len(fa["f64s"] & fx["f64s"])
                            if len(fa["callers"]) == len(fx["callers"]):
                                s += 0.6
                        best, bk = NEG, None
                        for prev, name in ((M[i - 1][j - 1], "M"),
                                           (GX[i - 1][j - 1], "GX"),
                                           (GY[i - 1][j - 1], "GY")):
                            if prev != NEG and prev + s > best:
                                best, bk = prev + s, name
                        M[i][j], BM[i][j] = best, bk
                        best, bk = GX[i][j - 1] + EXT, "GX"
                        if M[i][j - 1] + OPEN + EXT > best:
                            best, bk = M[i][j - 1] + OPEN + EXT, "M"
                        GX[i][j], BX[i][j] = best, bk
                        best, bk = GY[i - 1][j] + EXT, "GY"
                        if M[i - 1][j] + OPEN + EXT > best:
                            best, bk = M[i - 1][j] + OPEN + EXT, "M"
                        GY[i][j], BY[i][j] = best, bk
                i, j = n1, n2
                state = max((("M", M[i][j]), ("GX", GX[i][j]), ("GY", GY[i][j])),
                            key=lambda t: t[1])[0]
                al, arm_gap, x86_gap = [], [], []
                while i > 0 or j > 0:
                    if state == "M":
                        al.append((B[i - 1], Y[j - 1]))
                        state = BM[i][j]
                        i -= 1
                        j -= 1
                    elif state == "GX":
                        x86_gap.append(Y[j - 1])
                        nxt = BX[i][j]
                        j -= 1
                        state = nxt or "M"
                    else:
                        arm_gap.append(B[i - 1])
                        nxt = BY[i][j]
                        i -= 1
                        state = nxt or "M"
                for a, x in al:
                    if self.take(a, x, "order"):
                        n += 1
                # 空档太长就只当"对齐在这里不确定"，不硬说成内联：一个 20 长的空档
                # 说明的是这一段对不上，不是"20 个函数都被内联进了同一个母函数"。
                if len(x86_gap) <= 3:
                    for u in x86_gap:
                        if self._contained(self.x86[u], self.arm[a1]):
                            self.split_extra[a1].append(u)
                if len(arm_gap) <= 3:
                    for b in arm_gap:
                        if self._contained(self.arm[b], self.x86[x1]):
                            self.merge_extra[x1].append(b)
        self.log.append(("E:dp", n, len(self.mp)))
        return n

    @staticmethod
    def _contained(small, big):
        """`small` 的代码是不是被 `big` 这一个函数**包含**了（= 内联进来的）。

        两个判据，任一成立即算：特征集合是子集（且非空），或者小到只能是内联进来的一段。
        DP 的空档本身只说明"这一侧多出一个函数"，不说明它就是被内联的那个——
        所以必须再过这一关，否则一个长空档里的每个函数都会被算成同一个母函数的内联证据。
        """
        if small["lits"] or small["pfxs"] or small["f64s"]:
            if small["lits"] and not small["lits"] <= big["lits"]:
                return False
            if small["pfxs"] and not small["pfxs"] <= big["pfxs"]:
                return False
            if small["f64s"] and not small["f64s"] <= big["f64s"]:
                return False
            return True
        return bool(big["size"]) and small["size"] * 5 <= big["size"]

    def adjacency_inlining(self, span=2):
        """再补一轮"内联"证据：**地址相邻**的那个函数是不是被对方吞掉了。

        DP 的空档只在"该段对得上"时才可信；地址相邻是更直接的观察：
        Rust 把同一个模块的函数排在一起，所以 `f` 和它内联进来的 `f::{{closure}}`
        在两侧各自是相邻的。判据 = 相邻 + 特征被包含 + 体量不超过对方。
        """
        for i, a in enumerate(self.A):
            if a in self.mp:
                continue
            fa = self.arm[a]
            for k in (1, -1, 2, -2):
                if abs(k) > span:
                    continue
                j = i + k
                if not (0 <= j < len(self.A)):
                    continue
                a2 = self.A[j]
                x2 = self.mp.get(a2)
                if x2 is None:
                    continue
                fx = self.x86[x2]
                if fa["ninsn"] > fx["ninsn"]:
                    continue
                if self._contained(fa, fx):
                    self.merge_extra[x2].append(a)
                    break
        for i, x in enumerate(self.X):
            if x in self.rev:
                continue
            fx = self.x86[x]
            for k in (1, -1, 2, -2):
                if abs(k) > span:
                    continue
                j = i + k
                if not (0 <= j < len(self.X)):
                    continue
                x2 = self.X[j]
                a2 = self.rev.get(x2)
                if a2 is None:
                    continue
                fa = self.arm[a2]
                if fx["ninsn"] > fa["ninsn"]:
                    continue
                if self._contained(fx, fa):
                    self.split_extra[a2].append(x)
                    break

    # ---------------------------------------------------------------- F. 裁决
    # 背景：本工具原来是"多候选就把全部候选列出来、绝不挑一个"。§35（docs/02）实测
    # 63 个多候选里 29 个其实含真值，是"拒绝猜"的代价。本轮加了一条**受限的**裁决通道：
    # 只接受**唯一且可逐条复核**的结构事实，仍然不猜。
    #
    #   R1 `modblock`  模块块整体指派
    #       一个 graphwar 模块的 `panic::Location` 文件名是一个**精确**的 `&'static str`
    #       槽（如 `src/graphwar_life_manager.rs`）。"引用这个文件名的函数"在两侧都可枚举；
    #       两侧**数量相等** ⇒ 该模块没有内联/ICF 差异，整块可以一起指派。用
    #       体量/指令数/字面量/邻居一致度做得分，位掩码 DP 求最优指派并**数最优解个数**；
    #       只有"恰好 1 个最优解"且**与模块内地址序号对齐给出同一答案**时才采纳。
    #   R2 `score`     候选集内唯一最高分
    #       只在**已经存在的候选集**里比（不新增候选）：唯一最高分、与次高差 ≥1.0、
    #       并且至少有一条特征类证据（罕见字面量/f64）或 ≥2 条已配对邻居一致。
    #
    # 两条规则的证据都写进 `evidence` 字段的 `arbi=` 段。两条都**只填空**（不动已有配对），
    # 且严格保持单射（一个 x86 地址只能被一个 arm64 函数认领）——`tests/sym_match.rs`
    # 的 `unique_answers_are_injective` 会立刻抓住违反。

    def _arbi_prep(self):
        """建裁决用的索引：字面量/前缀 → 引用它的函数；逐字节孪生。"""
        self._a_lit, self._x_lit = collections.defaultdict(list), collections.defaultdict(list)
        self._a_pfx, self._x_pfx = collections.defaultdict(list), collections.defaultdict(list)
        for va, f in self.arm.items():
            for t in f["lits"]:
                self._a_lit[t].append(va)
            for t in f["pfxs"]:
                self._a_pfx[t].append(va)
        for va, f in self.x86.items():
            for t in f["lits"]:
                self._x_lit[t].append(va)
            for t in f["pfxs"]:
                self._x_pfx[t].append(va)
        for d in (self._a_lit, self._x_lit, self._a_pfx, self._x_pfx):
            for t in d:
                d[t].sort()
        # 逐字节孪生（ICF 的判据，见 `bsig`）
        by_bsig = collections.defaultdict(list)
        for va, f in self.arm.items():
            if f.get("bsig"):
                by_bsig[f["bsig"]].append(va)
        self._twin = {}
        for lst in by_bsig.values():
            if len(lst) > 1:
                for v in lst:
                    self._twin[v] = [u for u in lst if u != v]
        self._name2addr = {}

    def _module_lit(self, a, name):
        """`a` 自己所在模块的 `panic::Location` 文件名 lit（没有就 None）。

        必须与 demangled 名字里的模块名一致：一个函数可能引用了别的模块的 Location
        （内联进来的被调者留下的），那是别人的文件名，不能拿它当"我属于哪个模块"。
        """
        k = name_key(name)
        if not k or not k[0]:
            return None
        want = "src/%s.rs" % k[0]
        if want in self.arm[a]["lits"]:
            return want
        alt = [t for t in self.arm[a]["lits"]
               if t.endswith("/%s.rs" % k[0]) or t.endswith("%s.rs" % k[0])]
        return alt[0] if len(alt) == 1 else None

    def _arbi_score(self, a, x):
        """→ (得分, 特征命中数, 已配对邻居一致数)；出闸返回 None。

        得分只由**二进制里能读到的东西**构成：罕见字面量、f64、罕见前缀、已配对邻居一致、
        体量比/指令数比的对数偏差。刻意不给"顺序"加权——顺序证据已经被 D/E 两轮用尽了。
        """
        fa, fx = self.arm[a], self.x86[x]
        if not fx["size"] or not fx["ninsn"]:
            return None
        r = fa["size"] / fx["size"]
        if not (LOOSE_LO <= r <= LOOSE_HI):
            return None
        s = -abs(math.log(r))
        q = fa["ninsn"] / fx["ninsn"]
        s -= 0.8 * abs(math.log(q))
        strong = 0        # 精确 `&'static str` 槽 / f64：内容级指纹
        weak = 0          # `.rodata` 前缀：只能当弱证据（合并串表的邻接会造假，实测见过
                          #   `GraphwarButton` 的 trampoline 与 `PlayerShowcase` 的共享
                          #   `readyPlayerShowc`——那是两条相邻字符串被拼在一起）
        for t in fa["lits"] & fx["lits"]:
            if len(self._a_lit.get(t, ())) <= RARE_N and len(self._x_lit.get(t, ())) <= RARE_N:
                s += 3.0
                strong += 1
        for t in fa["f64s"] & fx["f64s"]:
            s += 2.0
            strong += 1
        for t in fa["pfxs"] & fx["pfxs"]:
            if len(self._a_pfx.get(t, ())) <= RARE_N and len(self._x_pfx.get(t, ())) <= RARE_N:
                s += 1.0
                weak += 1
        ca = {self.mp[c] for c in fa["calls"] if c in self.mp}
        xa = {self.mp[c] for c in fa["callers"] if c in self.mp}
        nag = len(ca & set(fx["calls"])) + len(xa & set(fx["callers"]))
        s += 1.5 * nag
        return s, strong, weak, nag

    @staticmethod
    def _strict_pair(fa, fx):
        """一对函数是否落在**实测同源区间**：体量比 0.61–1.19 且指令数比 0.70–1.45。"""
        if not fx["size"] or not fx["ninsn"]:
            return False
        r = fa["size"] / fx["size"]
        if not (SIZE_LO <= r <= SIZE_HI):
            return False
        q = fa["ninsn"] / fx["ninsn"]
        return NINSN_LO <= q <= NINSN_HI

    def _best_assign(self, pend, avail):
        """位掩码 DP：→ (最优解个数, 一个最优指派 dict)。用 `_arbi_score` 当权重。

        `fwd[i][mask]` = 前 i 行、占用的 x86 槽集合为 mask 时的最优得分；最优解个数用
        同一个 `fwd` 反向数——**只有得分恰好等于最优值的走法才算**，这样"唯一"才有意义。
        """
        n = len(pend)
        if n == 0 or n > MODBLOCK_MAX:
            return None, None
        w = [[(lambda t: None if t is None else t[0])(self._arbi_score(a, x)) for x in avail]
             for a in pend]
        fwd = [{0: 0.0}]
        for i in range(n):
            cur = {}
            for mask, val in fwd[-1].items():
                for j in range(n):
                    if mask >> j & 1 or w[i][j] is None:
                        continue
                    nm = mask | (1 << j)
                    v = val + w[i][j]
                    if nm not in cur or v > cur[nm] + 1e-9:
                        cur[nm] = v
            fwd.append(cur)
        full = (1 << n) - 1
        if full not in fwd[n]:
            return None, None
        memo = {}

        def best_count(i, mask):
            if i == n:
                return 1 if mask == full else 0
            key = (i, mask)
            if key in memo:
                return memo[key]
            base = fwd[i][mask]
            tot = 0
            for j in range(n):
                if mask >> j & 1 or w[i][j] is None:
                    continue
                nm = mask | (1 << j)
                if nm not in fwd[i + 1]:
                    continue
                if abs(base + w[i][j] - fwd[i + 1][nm]) > 1e-9:
                    continue
                tot += best_count(i + 1, nm)
            memo[key] = tot
            return tot

        nopt = best_count(0, 0)
        out, mask = {}, 0
        for i in range(n):
            pick = None
            for j in range(n):
                if mask >> j & 1 or w[i][j] is None:
                    continue
                nm = mask | (1 << j)
                if nm in fwd[i + 1] and abs(fwd[i][mask] + w[i][j] - fwd[i + 1][nm]) <= 1e-9:
                    pick = j
                    break
            if pick is None:
                return None, None
            out[pend[i]] = avail[pick]
            mask |= 1 << pick
        return nopt, out

    def arbitrate(self, symbols):
        """裁决多候选。返回 (modblock 条数, score 条数)。**只填空，不改已有配对。**"""
        self._arbi_prep()
        self._symbols = symbols
        self._name2addr = {}
        for a, nm in symbols:
            self._name2addr.setdefault(nm, a)
        n_mod = self._arbi_modblocks(symbols)
        n_score = 0
        # 分数裁决要**迭代**：一行认领了某个槽以后，别的行的候选集里就少了一个竞争项，
        # 原来"差一点"的裕度可能变得清楚（实测有这种连锁）。和主匹配一样收敛到不动。
        for _ in range(4):
            got = self._arbi_scores(symbols)
            got += self._arbi_groups()
            n_score += got
            if not got:
                break
        self.log.append(("F:arbi", n_mod + n_score, len(self.mp)))
        return n_mod, n_score

    def _arbi_modblocks(self, symbols):
        got = 0
        blocks = collections.defaultdict(list)
        for a, nm in symbols:
            L = self._module_lit(a, nm)
            if L:
                blocks[L].append(a)
        for L in sorted(blocks):
            A = self._a_lit.get(L, [])
            X = self._x_lit.get(L, [])
            # 允许模块级数量差 ±1：`panic::Location` 集合是"有 panic 点的函数"，两侧
            # 差一个成员本身是允许的（多一个内联进来的引用 / 少一个被 ICF 掉的）。
            if not A or abs(len(A) - len(X)) > 1:
                continue
            # 块外占用保护：本模块的 x86 成员被"别的模块的 arm64 函数"认领了 ≥2 个时，
            # 说明"引用同一个 Location 文件名"的两侧集合**不再一一对应**（实测
            # `graphwar_team_manager` 就是这样：7 对 7，但真值落在已被别人占用的两个槽上，
            # 于是整块被挤着平移一位）。这种情况整块放弃，一条都不定。
            if sum(1 for x in X if x in self.rev and self.rev[x] not in A) > 1:
                if os.environ.get("SYM_MATCH_DEBUG"):
                    print("      [modblock] %s 块外占用>1，放弃" % L, file=sys.stderr)
                continue
            pend = sorted(a for a in A if a not in self.mp)
            avail = sorted(x for x in X if x not in self.rev)
            if not pend or len(avail) > len(pend) or len(pend) - len(avail) > 1:
                continue
            if len(pend) == 1 and len(avail) == 1:
                # 只有一行时"最优解唯一"是废话；这一条要求严格同源区间
                a, x = pend[0], avail[0]
                if not self._strict_pair(self.arm[a], self.x86[x]):
                    continue
                cnt, plan = 1, {a: x}
                rule = "n=1,strict"
            elif len(pend) == len(avail) + 1:
                # 模块里 arm64 比 x86 多一个函数 ⇒ 恰好一行的 x86 副本不存在（被内联/合并）。
                # 枚举"跳过哪一行"，要求：① 其余各行按序号对齐后**全部**落在实测同源区间；
                # ② 能这样做的跳过位置**只有一种**。两条都成立才采纳。
                plan = None
                hits = []
                for k in range(len(pend)):
                    mp2 = {pend[j]: avail[j if j < k else j - 1]
                           for j in range(len(pend)) if j != k}
                    if all(self._strict_pair(self.arm[a], self.x86[x]) for a, x in mp2.items()) \
                            and all(self._arbi_score(a, x) is not None for a, x in mp2.items()):
                        hits.append((k, mp2))
                if len(hits) != 1:
                    # 多个"跳过位置"都能让整块落在实测同源区间时，用结构得分把它们拉开：
                    # 只有第一名领先第二名 ≥ SCORE_MARGIN 才采纳。
                    scored = []
                    for k2, mp2 in hits:
                        tot = sum(self._arbi_score(a2, x2)[0] for a2, x2 in mp2.items())
                        scored.append((tot, k2, mp2))
                    if os.environ.get("SYM_MATCH_DEBUG"):
                        print("      [modblock] %s n=%d avail=%d hits=%d scores=%s"
                              % (L, len(pend), len(avail), len(hits),
                                 ["%.2f@%d" % (t, k2) for t, k2, _ in sorted(scored, reverse=True)]),
                              file=sys.stderr)
                    if len(scored) < 2:
                        continue
                    scored.sort(key=lambda t: (-t[0], t[1]))
                    if scored[0][0] - scored[1][0] < SCORE_MARGIN:
                        continue
                    _tot, k, plan = scored[0]
                    rule = "n=%d,skip@%d(strict+score)" % (len(pend), k)
                    cnt = 1
                else:
                    k, plan = hits[0]
                    rule = "n=%d,skip@%d(strict)" % (len(pend), k)
                    cnt = 1
            else:
                cnt, plan = self._best_assign(pend, avail)
                if cnt != 1 or not plan:
                    if os.environ.get("SYM_MATCH_DEBUG"):
                        print("      [modblock] %s n=%d opt=%s rank_same=%s"
                              % (L, len(pend), cnt, plan == dict(zip(pend, avail)) if plan else "-"),
                              file=sys.stderr)
                    continue
                # 与"模块内地址序号对齐"必须给同一答案（两条独立规则互证）
                if plan != dict(zip(pend, avail)):
                    continue
                if not all(self._strict_pair(self.arm[a], self.x86[plan[a]]) for a in pend):
                    if os.environ.get("SYM_MATCH_DEBUG"):
                        bad = [("0x%x->0x%x" % (a, plan[a])) for a in pend
                               if not self._strict_pair(self.arm[a], self.x86[plan[a]])]
                        print("      [modblock] %s 体量带外: %s" % (L, ",".join(bad)),
                              file=sys.stderr)
                    continue
                rule = "n=%d,opt=1,rank=same" % len(pend)
            for a in sorted(plan):
                x = plan[a]
                if a in self.mp or x in self.rev:
                    continue
                self.mp[a] = x
                self.rev[x] = a
                self.why[(a, x)].add("modblock")
                self.arbi[(a, x)] = "%s(%s)" % (ARB_MODBLOCK, rule)
                got += 1
        return got

    def _arbi_scores(self, symbols):
        """候选集内唯一最高分。

        采纳条件（三条都要）：
          ① 最高分与次高分的差 ≥ `SCORE_MARGIN`；
          ② 有**强特征**（罕见精确字面量 / f64）撑着，或有 ≥2 条已配对邻居一致撑
             （纯弱前缀不算——实测过合并串表把两个无关类的前缀拼在一起）；
          ③ 这个候选不是别的行同样"理直气壮"的首选（两个函数抢同一个 x86 槽、
             得分又拉不开时，谁上都是赌，**两条都留 multi**）。
        按裕度从大到小处理，先到先得，保持单射。
        """
        want = []
        for a, nm in symbols:
            if a in self.mp:
                continue
            cand = self._candidates(a)
            if len(cand) < 2:
                continue
            scored = []
            for x in cand:
                r = self._arbi_score(a, x)
                if r is not None:
                    scored.append((x, r[0], r[1], r[2], r[3]))
            if len(scored) < 2:
                continue
            scored.sort(key=lambda t: (-t[1], t[0]))
            top = scored[0]
            margin = top[1] - scored[1][1]
            if margin < SCORE_MARGIN:
                continue
            strong, weak, nag = top[2], top[3], top[4]
            if not (strong >= 1 or nag >= 2 or (nag >= 1 and margin >= 2.0)):
                continue                       # 只有体量/弱前缀在支撑，不算证据
            want.append(dict(margin=margin, a=a, x=top[0], score=top[1],
                             strong=strong, weak=weak, nag=nag))
        # 争用保护：同一条 x86 槽是多个行的首选时，看第一名能不能把第二名甩开
        by_x = collections.defaultdict(list)
        for w in want:
            by_x[w["x"]].append(w)
        blocked = set()
        for x, lst in by_x.items():
            if len(lst) < 2:
                continue
            lst.sort(key=lambda w: (-w["score"], w["a"]))
            if lst[0]["score"] - lst[1]["score"] < CONTEST_MARGIN:
                for w in lst:
                    blocked.add(w["a"])
        want.sort(key=lambda w: (-w["margin"], w["a"]))
        got = 0
        for w in want:
            a, x = w["a"], w["x"]
            if a in blocked or a in self.mp or x in self.rev:
                continue
            self.mp[a] = x
            self.rev[x] = a
            self.why[(a, x)].add("score")
            self.arbi[(a, x)] = "%s(margin=%.2f,strong=%d,weak=%d,neighbours=%d)" % (
                ARB_SCORE, w["margin"], w["strong"], w["weak"], w["nag"])
            got += 1
        return got

    def _arbi_groups(self):
        """互相抢同一条 x86 槽的几行，**整组一起指派**（和 modblock 同一套唯一性判据）。

        单行裁决遇到"两行的首选是同一个槽"时必须两边都留 multi（谁上都是赌）。但把这几行
        放在一起看，问题是"这个槽该给谁、另一个槽给谁"——一个**指派**问题：枚举所有
        "行→槽"的单射（允许有行没槽），若最优指派**唯一**、且与次优的得分差 ≥ `SCORE_MARGIN`，
        就整组采纳。
        """
        rows = []
        for a, _nm in self._symbols:
            if a in self.mp:
                continue
            cand = self._candidates(a)
            if len(cand) < 2:
                continue
            sc = []
            for x in cand:
                r = self._arbi_score(a, x)
                if r is not None:
                    sc.append((x, r[0], r[1], r[3]))       # x, 分, strong, nag
            if len(sc) >= 2:
                rows.append((a, sc))
        if not rows:
            return 0
        # 连成组：行与行之间共享候选槽就连在一起
        parent = list(range(len(rows)))

        def find(i):
            while parent[i] != i:
                parent[i] = parent[parent[i]]
                i = parent[i]
            return i

        def union(i, j):
            a, b = find(i), find(j)
            if a != b:
                parent[max(a, b)] = min(a, b)

        owner = collections.defaultdict(list)
        for i, (_a, sc) in enumerate(rows):
            for x, _s, _st, _nag in sc:
                owner[x].append(i)
        for x, lst in owner.items():
            for j in lst[1:]:
                union(lst[0], j)
        groups = collections.defaultdict(list)
        for i in range(len(rows)):
            groups[find(i)].append(i)
        got = 0
        for _root in sorted(groups):
            idx = groups[_root]
            if len(idx) < 2 or len(idx) > 4:
                continue
            slots = sorted({x for i in idx for x, _s, _st, _nag in rows[i][1]})
            if len(slots) > 8:
                continue
            # 暴力枚举单射（行数 ≤4、槽数 ≤8 ⇒ P(8,4)=1680）
            best = []

            def rec(k, used, plan, total):
                if k == len(idx):
                    best.append((total, tuple(sorted(plan))))
                    return
                a, sc = rows[idx[k]]
                for x, v, strong, nag in sc:
                    if x in used:
                        continue
                    if not (strong >= 1 or nag >= 2):
                        continue
                    rec(k + 1, used | {x}, plan + [(a, x)], total + v)
                # 这一行也可以不指派（可能它的 x86 副本根本不存在）
                rec(k + 1, used, plan, total)

            rec(0, set(), [], 0.0)
            if not best:
                continue
            best.sort(key=lambda t: (-t[0], t[1]))
            if len(best) < 2:
                continue
            if best[0][0] - best[1][0] < SCORE_MARGIN:
                continue                                   # 最优解不唯一/拉不开
            plan = dict(best[0][1])
            if not plan:
                continue
            for a, x in sorted(plan.items()):
                if a in self.mp or x in self.rev:
                    continue
                self.mp[a] = x
                self.rev[x] = a
                self.why[(a, x)].add("score")
                self.arbi[(a, x)] = "%s(group=%d,opt=1,lead=%.2f)" % (
                    ARB_SCORE, len(idx), best[0][0] - best[1][0])
                got += 1
        return got

    # ---------------------------------------------------------------- G. 无候选归因
    def classify_none(self, a, name):
        """为什么这条 arm64 符号在 x86 侧一个候选都没有？→ (类别, 证据串)。

        五类，按"先能证死的、后靠猜的"排：

          `icf`     逐字节相同（`bsig`）的孪生已经在 x86 侧被认领 ⇒ x86 只留了一份代码
          `inlined` 闭包/母函数已配对且地址相邻，或函数体助记符序列被相邻已配对兄弟包含，
                    或特征集合被某个已配对 x86 函数包含
          `data`    体内**没有任何控制流指令**（不是函数，是 `.text` 里的数据/跳转表）
          `tiny`    指令数 ≤ 12（`drop_in_place` / 转发 shim，本来就没有可比特征）
          `other`   以上都不成立 —— **诚实标未知**，但会把"哑火原因"写进证据串，便于分类统计：
                      `no_feature`    没有字符串字面量、没有 f64、没有罕见整数常量
                      `pfx_only`      只有"两侧几百个函数都引用"的公共前缀（不构成证据）
                      `feature_no_fit` 有特征，但共享它的 x86 函数体量比全部出闸（0.40–2.20）

        ⚠ 为什么不用"助记符序列相同"判 ICF：实测这 444 条里有 150 条存在**形状孪生**，
        但它们是**不同的源函数**（例：`drop_in_place<Result<TickEvent,_>>` 与
        `drop_in_place<http::extensions::Extensions>` 助记符序列逐条相同、字节不同）。
        形状相同 ≠ ICF。只有逐字节相同才是"同一个函数体"。
        """
        fa = self.arm[a]
        mseq = fa.get("mseq") or ()
        for b in sorted(self._twin.get(a, ())):
            if b in self.mp:
                return "icf", "twin=0x%x;twin_x86=0x%x" % (b, self.mp[b])
        # 被内联 ①：闭包 + 母函数已配对 + 两者地址相邻（Rust 把闭包紧挨着母函数排）
        if "{{closure}}" in name:
            base = name.split("::{{closure}}")[0]
            pb = self._find_parent(base)
            if pb is not None and pb in self.mp:
                if abs(self.ai.get(pb, 1 << 30) - self.ai[a]) <= 3:
                    return "inlined", "parent=0x%x;parent_x86=0x%x" % (pb, self.mp[pb])
        # 被内联 ②：函数体助记符序列是相邻已配对兄弟的一段
        i = self.ai[a]
        for k in (-2, -1, 1, 2):
            j = i + k
            if not (0 <= j < len(self.A)):
                continue
            b = self.A[j]
            if b not in self.mp:
                continue
            if _seq_contains(self.arm[b].get("mseq") or (), mseq):
                return "inlined", "body_in=0x%x;x86=0x%x" % (b, self.mp[b])
        # 被内联 ③：特征集合被某个已配对的 x86 函数包含
        if fa["lits"] or fa["f64s"]:
            for b in sorted(self.mp):
                fx = self.x86[self.mp[b]]
                if fa["lits"] and not fa["lits"] <= fx["lits"]:
                    continue
                if fa["f64s"] and not fa["f64s"] <= fx["f64s"]:
                    continue
                return "inlined", "features_in=0x%x" % self.mp[b]
        if not mseq:
            return "data", "no_insn"
        if not any(_CFLOW_INSN.match(m) for m in mseq):
            return "data", "no_control_flow;ninsn=%d" % fa["ninsn"]
        if fa["ninsn"] <= TINY_NINSN:
            return "tiny", "ninsn=%d" % fa["ninsn"]
        # ---- 其它：至少把"为什么哑火"记下来
        if not fa["lits"] and not fa["f64s"] and not fa["consts"]:
            reason = "no_feature"
        elif fa["lits"] or fa["f64s"]:
            reason = "feature_no_fit"
        else:
            reason = "pfx_only"
        return "other", "%s;ninsn=%d" % (reason, fa["ninsn"])

    def _find_parent(self, base):
        """在夹具的 1,300 个符号里找闭包的母函数（名字前缀匹配，容忍 trait 写法差异）。"""
        pb = self._name2addr.get(base)
        if pb is not None:
            return pb
        tail = "::".join(base.split("::")[-2:])
        hits = [a for nm, a in self._name2addr.items() if nm.endswith("::" + tail)]
        return hits[0] if len(hits) == 1 else None

    # ---------------------------------------------------------------- 收敛
    def run(self, rounds=8):
        self.seed_literals()
        self.seed_f64()
        self.seed_consts()
        for _ in range(rounds):
            got = (self.propagate() + self.diag_extend()
                   + self.dp_fill(use_lis=True) + self.dp_fill(use_lis=False))
            if not got:
                break
        self.adjacency_inlining()
        return self.mp

    # ---------------------------------------------------------------- 结论
    def corroborate(self, a, x):
        """统计一对 (a,x) 有几类**独立**证据成立。少于 2 类只配标 `weak`。"""
        fa, fx = self.arm[a], self.x86[x]
        kinds = set()
        if (fa["lits"] & fx["lits"]) or (fa["pfxs"] & fx["pfxs"]):
            kinds.add("lit")
        if fa["f64s"] & fx["f64s"]:
            kinds.add("f64")
        ca = {c for c in fa["calls"]}
        cx = {c for c in fx["calls"]}
        agree = 0
        for c in ca:
            y = self.mp.get(c)
            if y is not None and y in cx:
                agree += 1
        for p in fa["callers"]:
            y = self.mp.get(p)
            if y is not None and y in fx["callers"]:
                agree += 1
        if agree:
            kinds.add("call")
        if "order" in self.why[(a, x)]:
            kinds.add("order")
        if "modblock" in self.why[(a, x)]:
            # 模块块裁决是**独立证据类**：它靠的是"Location 文件名划出的集合在两侧等量"
            # + 体量/指令数兼容 + 最优解唯一，与 lit/f64/call/order 都不是同一条链。
            kinds.add("modblock")
        # `score` 裁决刻意**不**加成独立类：它是既有特征/邻居的再组合，不是新证据。
        return kinds, agree

    def evidence(self, a, x, kinds, agree):
        fa, fx = self.arm[a], self.x86[x]
        parts = ["size=%.2f" % (fa["size"] / fx["size"]) if fx["size"] else "size=?"]
        if fa["lits"] or fx["lits"]:
            parts.append("lit=%d/%d" % (len(fa["lits"] & fx["lits"]), len(fa["lits"])))
        if fa["f64s"]:
            parts.append("f64=%d/%d" % (len(fa["f64s"] & fx["f64s"]), len(fa["f64s"])))
        ca, cx = set(fa["calls"]), set(fx["calls"])
        if ca or cx:
            parts.append("call=%d/%d" % (len(ca & cx), len(ca | cx)))
        parts.append("via=" + "+".join(sorted(kinds)))
        return ";".join(parts)

    def conclude(self, symbols):
        """`symbols` = [(arm_va, demangled_name)]，输出每个符号一行。"""
        rows = []
        for a, name in symbols:
            fa = self.arm.get(a)
            if fa is None:
                continue
            if a in self.mp:
                x = self.mp[a]
                kinds, agree = self.corroborate(a, x)
                conf = CONF_UNIQUE if len(kinds) >= 2 else CONF_WEAK
                ev = self.evidence(a, x, kinds, agree)
                if (a, x) in self.arbi:
                    ev += ";arbi=" + self.arbi[(a, x)]
                if self.split_extra.get(a):
                    conf = CONF_SPLIT
                    ev += ";inlined_x86=" + ",".join(
                        "0x%x" % u for u in sorted(set(self.split_extra[a])))
                rows.append(dict(a=a, name=name, size=fa["size"], cands=[x], conf=conf, ev=ev))
                continue
            home = None
            for x, lst in self.merge_extra.items():
                if a in lst:
                    home = x
                    break
            if home is not None:
                rows.append(dict(a=a, name=name, size=fa["size"], cands=[home],
                                 conf=CONF_MERGE,
                                 ev="size=%.2f;inlined_into=0x%x"
                                    % (self.ratio(a, home) or 0, home)))
                continue
            cand = self._candidates(a)
            if not cand:
                cat, det = self.classify_none(a, name)
                rows.append(dict(a=a, name=name, size=fa["size"], cands=[],
                                 conf=CONF_NONE, ev="no_candidate;why=%s;%s" % (cat, det)))
            elif len(cand) == 1:
                x = cand[0]
                kinds, agree = self.corroborate(a, x)
                rows.append(dict(a=a, name=name, size=fa["size"], cands=[x],
                                 conf=CONF_WEAK, ev=self.evidence(a, x, kinds, agree)))
            else:
                rows.append(dict(a=a, name=name, size=fa["size"], cands=cand,
                                 conf=CONF_MULTI,
                                 ev="candidates=%d" % len(cand)))
        # ---- 单射收尾：两条不同的 arm64 函数抢同一个 x86 槽时，工具**无法裁决**
        # （两者的候选集里都只剩它一个）。按"多合一"记账：第一个保留原结论，其余记成
        # `merge` 并写出对手地址。`tests/sym_match.rs::unique_answers_are_injective` 会
        # 立刻抓住漏网的单射冲突，所以这一步是必须的，不是记账技巧。
        def _strength(r):
            """抢槽时谁"更该"保留结论：证据类数多者优先，其次体量比更接近 1。"""
            if not r["cands"]:
                return (99, 9.0, r["a"])
            kinds, _agree = self.corroborate(r["a"], r["cands"][0])
            fx = self.x86[r["cands"][0]]
            ratio = abs(math.log(r["size"] / fx["size"])) if fx["size"] else 9.0
            return (-len(kinds), ratio, r["a"])

        seen = {}
        for r in sorted(rows, key=_strength):
            if r["conf"] not in (CONF_UNIQUE, CONF_WEAK, CONF_SPLIT):
                continue
            x = r["cands"][0]
            if x in seen and seen[x][0] != r["a"]:
                r["conf"] = CONF_MERGE
                r["ev"] = ("size=%.2f;dup_claim;inlined_into=0x%x;rival=0x%x(%s)"
                           % (r["size"] / self.x86[x]["size"] if self.x86[x]["size"] else 0,
                              x, seen[x][0], seen[x][1]))
            else:
                seen.setdefault(x, (r["a"], r["name"]))
        rows.sort(key=lambda r: (r["a"], r["name"]))
        return rows

    def _candidates(self, a):
        """还没配上的 arm64 函数：把所有**有证据支持**的 x86 候选列全，不挑。"""
        fa = self.arm[a]
        c = collections.Counter()
        for key in ("lits", "pfxs"):
            idx = getattr(self, "_xidx_" + key, None)
            if idx is None:
                idx = self._x_lit_index(key)
                setattr(self, "_xidx_" + key, idx)
            for s in fa[key]:
                for x in idx.get(s, ()):
                    c[x] += 1
        if fa["f64s"]:
            idx = getattr(self, "_xidx_f64", None)
            if idx is None:
                idx = collections.defaultdict(set)
                for x, f in self.x86.items():
                    for v in f["f64s"]:
                        idx[v].add(x)
                self._xidx_f64 = idx
            for v in fa["f64s"]:
                for x in idx.get(v, ()):
                    c[x] += 1
        out = [x for x, k in c.items() if x not in self.rev and k >= 1 and self.gate(a, x)]
        return sorted(out)


# ---------------------------------------------------------------------------
# 8. 子命令
# ---------------------------------------------------------------------------
COLS = ("arm64_name", "arm64_addr", "arm64_size", "x86_candidates", "confidence", "evidence")
BANNER = (
    "# 由 tools/sym_match.py build 生成，不要手改（重跑会覆盖）。\n"
    "# 每行一个 arm64 符号（名字来自 Android 2.0.60 未 strip 的 .symtab，共 1,300 个）。\n"
    "# 宇宙口径：**提到 `graphwarrust` 的符号**（`rust_sym.mentions_crate`），"
    "故意比 `rust_sym.is_game_name` 宽 ——\n"
    "# 本表是**配对表**不是名字表：编译器胶水（`drop_in_place<…graphwarrust::X…>`）"
    "在两侧也是真函数，配对有价值。\n"
    "# 名字**键**只从游戏自有符号取（`parse_name` 里的 crate 闸），"
    "所以胶水符号不会产出 (module,type,method)。见 docs/02 §39.5。\n"
    "# 地址一律是**本架构的 ELF 虚地址**；Ghidra/REA 地址 = 该值 + 0x100000。\n"
    "# x86_candidates 为空 = 无候选；逗号分隔多个 = 多候选（全部列出，不挑一个）。\n"
    "# confidence: unique=≥2 类独立证据 / weak=只有 1 类 / multi / none /\n"
    "#             split=arm64 侧内联了额外的 x86 函数 / merge=x86 侧内联了额外的 arm64 函数。\n"
    "# evidence: size=体量比; lit/f64/call=集合重合数; via=靠哪几类证据; inlined_*=内联证据。\n"
)


def has_capstone():
    try:
        import capstone  # noqa: F401
        return True
    except ImportError:
        return False


def graphwar_symbols(arm64_path):
    """`.symtab` 里**提到本 crate** 的已定义 FUNC → [(addr, 去 hash 的 demangle)]。

    ## 这个宇宙为什么**故意**比 `rust_sym.is_game_name` 宽

    本工具不是「名字表」，是**配对表**：它要把 arm64 与 x86 两侧**没有 Location 锚**的函数
    对上（§35）。`core::ptr::drop_in_place<…graphwarrust::X…>` 这类编译器胶水在这个意义上
    仍是**真函数**（两侧都有函数体），把它们踢出去会让配对表少 330 行、并且让
    `tests/sym_match.rs` 的宇宙断言（1,300）与 §35 的统计全部失效。
    ⇒ 本工具的口径是「提到 `graphwarrust` 的符号」，由 `rust_sym.mentions_crate` 显式命名，
    **不是** `"graphwar" in n` 这种随手写的子串。名字**是否可信**另由 `parse_name` +
    `rust_sym` 决定（见下），两者不要混为一谈。

    真正用「游戏自有」判据的是 `name_bridge.py` / `win_names.py` / `three_way_match.py`
    （它们的产物是**名字**，不是配对）。
    """
    elf = Elf(arm64_path)
    tex = elf.sec[".text"]
    lo, hi = tex["addr"], tex["addr"] + tex["size"]
    syms = [(n, a) for n, a, _s in elf.func_symbols()
            if lo <= a < hi and rust_sym.mentions_crate(n)]
    syms.sort(key=lambda r: (r[1], r[0]))
    dem = demangle([n for n, _a in syms])
    return [(a, HASH_SUFFIX.sub("", d)) for (_n, a), d in zip(syms, dem)]


def cmd_build(args):
    if not has_capstone():
        die("build 需要 capstone：用仓库 venv 跑 `../.venv/bin/python tools/sym_match.py build`\n"
            "     （report / --verify 只读 TSV，标准库即可）")
    for p in (args.arm64, args.x86):
        if not os.path.exists(p):
            die("找不到 %s（用 --arm64/--x86 指定，或设 GW2_ANALYSIS_ROOT）" % p)

    print("[1/4] 反汇编 arm64（未 strip，`.symtab` 即函数边界）…", file=sys.stderr)
    arm = load_side("arm64", args.arm64, None, "a")
    print("[2/4] 反汇编 x86-64（Ghidra 函数起点表）…", file=sys.stderr)
    x86 = load_side("x86", args.x86, args.funcstarts, "x")
    print("      arm64 函数 %d / x86 函数 %d" % (len(arm), len(x86)), file=sys.stderr)

    symbols = graphwar_symbols(args.arm64)
    n_addr = len({a for a, _n in symbols})
    print("      arm64 名字含 graphwar 的符号 %d 个（%d 个不同地址）"
          % (len(symbols), n_addr), file=sys.stderr)

    print("[3/4] 匹配（字面量 → f64 → 调用图 → 顺序 → 空档对齐，迭代到收敛）…", file=sys.stderr)
    m = Matcher(arm, x86)
    m.run()
    n_mod, n_score = m.arbitrate(symbols)
    print("      裁决：modblock +%d / score +%d" % (n_mod, n_score), file=sys.stderr)
    for tag, n, tot in m.log:
        print("      %-10s +%-6d 累计 %d" % (tag, n, tot), file=sys.stderr)
    rows = m.conclude(symbols)
    print("[4/4] 写出 %s（%d 行）" % (rel(args.out), len(rows)), file=sys.stderr)

    tmp = args.out + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        fh.write(BANNER)
        fh.write("\t".join(COLS) + "\n")
        for r in rows:
            fh.write("%s\t0x%x\t%d\t%s\t%s\t%s\n" % (
                r["name"], r["a"], r["size"],
                ",".join("0x%x" % c for c in r["cands"]), r["conf"], r["ev"]))
    os.replace(tmp, args.out)
    st = collections.Counter(r["conf"] for r in rows)
    print("      " + " / ".join("%s=%d" % (k, st.get(k, 0)) for k in STATUS), file=sys.stderr)
    if args.eval:
        return cmd_verify(argparse.Namespace(tsv=args.out, truth=args.truth))
    return 0


def read_tsv(path):
    if not os.path.exists(path):
        die("没有 %s —— 先跑 `build`" % rel(path))
    rows, header = [], None
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("#") or not line.strip():
                continue
            p = line.rstrip("\n").split("\t")
            if header is None:
                header = p
                continue
            rows.append(dict(zip(header, p)))
    if not rows:
        die("%s 是空的" % rel(path))
    return rows


def load_truth(path):
    """`fixtures/name_to_addr.tsv` → {(module, Owner, method): [x86_addr, ...]}（只取唯一锚）。

    值用**列表**：同一个 (module, Type, method) 在 arm64 上可能对应多个单态化，文件里
    也就可能给出多个地址。这种 key 的 ground truth 本身不唯一，`--verify` 会把它排除在
    分母外 —— 拿它判对错是拿一个含糊的真值去判一个必须唯一的结果。
    """
    truth, n_rows = {}, 0
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("#") or not line.strip():
                continue
            p = line.rstrip("\n").split("\t")
            if len(p) < 7 or p[0] == "module":
                continue
            n_rows += 1
            if not p[3].startswith("0x") or p[6] != "唯一锚":
                continue
            truth.setdefault((p[0], p[1] if p[1] != "_" else None, p[2]), []).append(int(p[3], 16))
    return truth, n_rows


def _report_name(rows, args):
    """`report --name <子串>`：把命中行连同**逐条依据**打出来，供人工复核。"""
    hits = [r for r in rows if args.name.lower() in r["arm64_name"].lower()]
    if not hits:
        print("没有名字含 %r 的行" % args.name)
        return 1
    print("命中 %d 行（%s）" % (len(hits), rel(args.tsv)))
    for r in hits:
        print()
        print("%s" % r["arm64_name"])
        print("  arm64 0x%x（%d 字节）  confidence=%s"
              % (int(r["arm64_addr"], 16), int(r["arm64_size"]), r["confidence"]))
        cands = [c for c in r["x86_candidates"].split(",") if c]
        if not cands:
            print("  x86 候选：无")
        else:
            print("  x86 候选 %d 个：%s" % (len(cands), " ".join(cands)))
        print("  依据：%s" % r["evidence"])
        if len(cands) > 1:
            print("  （多候选的含义：这几条 x86 函数都**共享至少一个**可比特征，"
                  "按规则不挑；见 docs/02 §36）")
    return 0


def cmd_report(args):
    rows = read_tsv(args.tsv)
    if args.name:
        return _report_name(rows, args)
    st = collections.Counter(r["confidence"] for r in rows)
    print("sym_match 报告（%s，%d 行）" % (rel(args.tsv), len(rows)))
    print()
    print("  唯一匹配   unique %4d  weak %4d   合计 %4d"
          % (st[CONF_UNIQUE], st[CONF_WEAK], st[CONF_UNIQUE] + st[CONF_WEAK]))
    print("  多候选     multi  %4d" % st[CONF_MULTI])
    print("  无候选     none   %4d" % st[CONF_NONE])
    print("  一拆多     split  %4d   （1 个 arm64 函数 ↔ N 个 x86 函数）" % st[CONF_SPLIT])
    print("  多合一     merge  %4d   （1 个 x86 函数 ↔ N 个 arm64 函数）" % st[CONF_MERGE])
    arbi = collections.Counter()
    for r in rows:
        m2 = re.search(r"arbi=([a-z]+)\(", r["evidence"])
        if m2:
            arbi[m2.group(1)] += 1
    if arbi:
        print()
        print("  裁决（本轮新增的多候选 → 唯一）：" +
              " / ".join("%s %d" % kv for kv in sorted(arbi.items())))
        rule = collections.Counter(re.search(r"arbi=[a-z]+\(([a-z]+)", r["evidence"]).group(1)
                                   for r in rows if re.search(r"arbi=[a-z]+\(([a-z]+)", r["evidence"]))
        print("      其中规则：" + " / ".join("%s %d" % kv for kv in sorted(rule.items())))
    none_cat = collections.Counter()
    for r in rows:
        if r["confidence"] != CONF_NONE:
            continue
        m2 = re.search(r"why=([a-z]+)", r["evidence"])
        none_cat[m2.group(1) if m2 else "?"] += 1
    if none_cat:
        print()
        print("  无候选归因（%d 行）：" % sum(none_cat.values()) +
              " / ".join("%s %d" % kv for kv in sorted(none_cat.items())))
        sub = collections.Counter()
        for r in rows:
            if r["confidence"] != CONF_NONE:
                continue
            m2 = re.search(r"why=other;([a-z_]+)", r["evidence"])
            if m2:
                sub[m2.group(1)] += 1
        if sub:
            print("      other 的哑火原因：" + " / ".join("%s %d" % kv for kv in sorted(sub.items())))
    multi = [r for r in rows if r["confidence"] == CONF_MULTI]
    if multi:
        hist = collections.Counter(len(r["x86_candidates"].split(",")) for r in multi)
        print()
        print("  多候选的候选数分布：" +
              " ".join("%d个×%d" % (k, v) for k, v in sorted(hist.items())[:12]))
    for c in (CONF_SPLIT, CONF_MERGE):
        sel = [r for r in rows if r["confidence"] == c]
        if sel:
            print()
            print("  %s 明细（前 8）：" % c)
            for r in sel[:8]:
                print("    0x%-8x %-52s %s" % (int(r["arm64_addr"], 16),
                                               r["arm64_name"][:52], r["evidence"]))
    return 0


def cmd_verify(args):
    rows = read_tsv(args.tsv)
    truth, n_truth_all = load_truth(args.truth)
    by_key = collections.defaultdict(list)
    for r in rows:
        k = name_key(r["arm64_name"])
        if k is not None:
            by_key[k].append(r)

    agree = 0
    in_cands = 0
    n_unique = 0
    wrong, ratios = [], []
    amb_key = amb_rows = 0
    for k, addrs in sorted(truth.items(), key=lambda t: min(t[1])):
        xaddr = addrs[0]
        rs = by_key.get(k)
        if not rs:
            wrong.append((k, xaddr, "名字键没对上（fixture 里没有这个符号）"))
            continue
        if len(set(addrs)) > 1 or len(rs) > 1:
            # 同一个 (module, Type, method) 在 arm64 上有多个单态化/别名时，ground truth
            # 本身没说是哪一个。拿它判对错是拿一个含糊的真值判一个必须唯一的结果，
            # 所以这类 key 整条排除在分母外，单独计数。
            amb_key += 1
            amb_rows += len(addrs)
            continue
        cands = [c for c in rs[0]["x86_candidates"].split(",") if c]
        if int(xaddr) in {int(c, 16) for c in cands}:
            in_cands += 1
        if len(cands) == 1:
            n_unique += 1
        if len(cands) == 1 and int(cands[0], 16) == xaddr:
            agree += 1
            m = re.search(r"size=([0-9.]+)", rs[0]["evidence"])
            if m:
                ratios.append(float(m.group(1)))
        elif not cands:
            wrong.append((k, xaddr, "无候选"))
        elif xaddr in {int(c, 16) for c in cands}:
            wrong.append((k, xaddr, "候选含真值但不唯一（%d 个候选）" % len(cands)))
        else:
            wrong.append((k, xaddr, "候选不含真值：" +
                          ",".join(cands[:6]) + ("…" if len(cands) > 6 else "")))

    total = len(truth) - amb_key
    n_truth_keys = len(truth)
    rate = 100.0 * agree / total if total else 0.0
    print("sym_match --verify")
    print("  ground truth：%s 的「唯一锚」%d 行（%d 个不同的 (module, Type, method)）"
          % (rel(args.truth), n_truth_all, n_truth_keys))
    print("    其中 %d 个 key（%d 行）(module, Type, method) 在 arm64 上有多个符号/地址，"
          "真值不唯一 ⇒ 不计入分母" % (amb_key, amb_rows))
    print("  本工具 fixture：%s，%d 行" % (rel(args.tsv), len(rows)))
    print()
    print("  独立复现为**唯一匹配且地址相同**：%d / %d = %.1f%%" % (agree, total, rate))
    cat = collections.Counter()
    for _k, _a, why in wrong:
        if why.startswith("无候选"):
            cat["无候选"] += 1
        elif why.startswith("候选含真值"):
            cat["候选含真值但没定成唯一"] += 1
        elif why.startswith("候选不含真值"):
            cat["唯一候选是错的"] += 1
        else:
            cat["名字键没对上"] += 1
    print("  没复现：%d（%s）" % (len(wrong),
          " / ".join("%s %d" % (k, v) for k, v in cat.most_common())))
    if ratios:
        inband = sum(1 for r in ratios if SIZE_LO <= r <= SIZE_HI)
        print("  另外两个口径（同一个 fixture，不换判据）：")
    print("    真值**出现在候选集里**（含多候选）：%d / %d = %.1f%%  —— 召回"
          % (in_cands, total, 100.0 * in_cands / total if total else 0.0))
    print("    给出唯一候选的 %d 条里，正确的 %d 条 = %.1f%%  —— 精确"
          % (n_unique, agree, 100.0 * agree / n_unique if n_unique else 0.0))
    print("  复现上的体量比 arm64/x86：min %.2f / max %.2f，落在 %.2f–%.2f 的 %d/%d"
              % (min(ratios), max(ratios), SIZE_LO, SIZE_HI, inband, len(ratios)))
    print()
    if wrong:
        print("  没复现的逐条原因（最多 40 条）：")
        for k, xaddr, why in wrong[:40]:
            print("    0x%-8x %-46s %s"
                  % (xaddr, "::".join(x or "_" for x in k)[:46], why))
        if len(wrong) > 40:
            print("    …还有 %d 条" % (len(wrong) - 40))
    ok = rate >= 90.0
    print()
    print("  结论：一致率 %.1f%% %s 90%% 门槛 ⇒ %s"
          % (rate, ">=" if ok else "<", "PASS" if ok else "FAIL"))
    return 0 if ok else 1


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="跨架构函数匹配器（arm64 名字 → stripped x86-64 地址）")
    ap.add_argument("--verify", action="store_true", help="拿唯一锚当 ground truth 自校验")
    ap.add_argument("--tsv", default=DEFAULT_OUT)
    ap.add_argument("--truth", default=DEFAULT_TRUTH)
    sub = ap.add_subparsers(dest="cmd")

    b = sub.add_parser("build", help="生成 fixtures/sym_match.tsv")
    b.add_argument("--arm64", default=DEFAULT_ARM64)
    b.add_argument("--x86", default=DEFAULT_X86)
    b.add_argument("--funcstarts", default=DEFAULT_FUNCSTARTS)
    b.add_argument("--out", default=DEFAULT_OUT)
    b.add_argument("--truth", default=DEFAULT_TRUTH)
    b.add_argument("--eval", action="store_true", help="写完立刻跑一遍 --verify")
    b.set_defaults(func=cmd_build)

    r = sub.add_parser("report", help="统计")
    r.add_argument("--tsv", default=DEFAULT_OUT)
    r.add_argument("--name", default=None,
                   help="只打印名字含该子串的行（逐条依据，供人工复核）")
    r.set_defaults(func=cmd_report)

    args = ap.parse_args(argv)
    if args.verify:
        return cmd_verify(args)
    if not getattr(args, "func", None):
        ap.print_help()
        return 2
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
