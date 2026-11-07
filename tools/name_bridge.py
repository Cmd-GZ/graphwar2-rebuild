#!/usr/bin/env python3
"""把**原版源码里的真函数名**绑到 **2.0.60 x86-64 的地址**上。

## 为什么这件事能做成

两份 2.0.60 构建来自同一份源码、不同架构：

| 文件 | 架构 | 符号表 |
|---|---|---|
| Android `libgraphwarrust.so`（`project.binary` 里 version = 2.0.60） | arm64 | **有** `.symtab` |
| Linux `libgraphwarrust.so`（Steam 2026-08-28） | x86-64 | 被 strip |

桥是 `core::panic::Location`：

    struct Location { file: &'static str, line: u32, col: u32 }   // 24 字节

编译器把 `src/<file>.rs:<line>:<col>` 写进 `.data.rel.ro`，`.rela.dyn` 里有一条
`R_*_RELATIVE` 重定位指向 `.rodata` 的字符串。**这个常量与指令集无关**，所以：

    Android:  符号名  ↔ 它引用的 (file,line)
    Linux  :  地址    ↔ 它引用的 (file,line)
                              ↓ join
              符号名  ↔ Linux 地址

## 三级证据（输出里逐条标）

* `唯一锚`   —— 某个 `(file,line)` 在两侧**各自只被一个函数引用** ⇒ 双向唯一的硬配对。
* `歧义`     —— 该锚一对一不成立（内联/macro 让多个函数共享同一行），或一侧多函数。
* `次级证据` —— 没有唯一锚，但**独有的字符串引用 + 函数大小 + 调用邻居**三项都自洽。
* `未知`     —— 什么都对不上。

## 口径警告

* 本脚本输出的 **`x86_addr` 是 Linux 2.0.60 的 ELF 虚地址**（`objdump` 口径）。
  Ghidra/REA 地址 = 该值 + `0x100000`（只加一次）。**arm64 地址一律不进任何表**。
* 一个函数可能引用多个 Location（内联），也可能一个都没有；锚点表只写「引用了」，不写「属于」。
* `src_file` 只保留 `src/*.rs`（游戏自己的源文件）；依赖 crate 的 Location（`library/…`）
  单独计入 `dep_anchors`，不参与文案输出但参与唯一性判定——因为它们同样是编译期常量。

## 用法

    # 一条命令复现核心交付物
    .venv/bin/python tools/name_bridge.py build

    # 只看统计 / 只看某个模块 / 查某两个地址有没有锚
    .venv/bin/python tools/name_bridge.py stats
    .venv/bin/python tools/name_bridge.py module graphwar_shot_calculator
    .venv/bin/python tools/name_bridge.py probe 0x3c2530 0x3c2bc0

只依赖 binutils（`readelf` / `c++filt`）+ capstone + Python 标准库。
"""

import argparse
import bisect
import collections
import os
import pickle
import re
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import rust_sym  # noqa: E402  「哪条符号属于 graphwarrust」的唯一判据（结构解析，非子串）

# ---------------------------------------------------------------- 常量

X86_RELATIVE = 8        # R_X86_64_RELATIVE
AARCH64_RELATIVE = 1027  # R_AARCH64_RELATIVE
RELATIVE_BY_EM = {62: X86_RELATIVE, 183: AARCH64_RELATIVE}

HASH_SUFFIX = re.compile(r"::h[0-9a-f]{16}$")
CLOSURE = "{{closure}}"

# gdext / rustc 样板，不是原版源码里的函数名（与 tools/symtab_dump.py 同一份口径）。
BOILERPLATE = {
    "__register_methods", "__register_exports", "__virtual_call",
    "__inner_init", "__init", "class_name", "expecting",
    "panic_cold_explicit", "panic_cold_dispatch", "ptrcall_fn", "varcall_fn", "virtual_fn",
}
TRAILING_NOISE = {"ptrcall_fn", "varcall_fn", "virtual_fn",
                  "panic_cold_explicit", "panic_cold_dispatch", "__inner_init", "__init"}

DEFAULT_ARM64 = "/home/user/MyProject/gw2/.rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so"
DEFAULT_X86 = "/home/user/MyProject/gw2/.reference/steam/2026-08-28/libgraphwarrust.so"


def warn(m):
    print(f"[name_bridge] {m}", file=sys.stderr)


def run(cmd, **kw):
    env = dict(os.environ, LC_ALL="C")
    try:
        return subprocess.run(cmd, capture_output=True, text=True, check=True,
                              env=env, **kw).stdout
    except FileNotFoundError:
        sys.exit(f"找不到 {cmd[0]}：本脚本需要 binutils（readelf / c++filt）")
    except subprocess.CalledProcessError as e:
        sys.exit(f"{' '.join(cmd)} 失败：{e.stderr.strip()[:400]}")


# ---------------------------------------------------------------- ELF

class Elf:
    """只读本脚本要的四段：`.text` / `.rodata` / `.data.rel.ro` / `.rela.dyn`（+ `.symtab`）。"""

    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            self.data = f.read()
        # e_machine 直接从 ELF 头取：readelf 在 zh_CN locale 下把 "Machine:" 印成 "系统架构:"，
        # 按英文关键字匹配会静默失败（本脚本第一版踩过，x86 侧 0 个锚就是这么来的）。
        self.em = struct.unpack_from("<H", self.data, 18)[0]
        self.sec = {}
        cur = None
        for line in run(["readelf", "-SW", path]).splitlines():
            p = line.replace("[", " ").replace("]", " ").split()
            if len(p) >= 6 and p[0].isdigit():
                cur = p[1]
                try:
                    self.sec[cur] = [int(p[3], 16), int(p[4], 16), int(p[5], 16)]
                except ValueError:
                    pass
            elif cur and len(p) >= 2 and re.fullmatch(r"[0-9a-f]+", p[0]):
                self.sec[cur][2] = int(p[0], 16)  # NOBITS 的 Size 换行到下一行
        for need in (".text", ".rodata", ".data.rel.ro", ".rela.dyn"):
            if need not in self.sec:
                sys.exit(f"{path} 缺 {need}")

    def ro_bytes(self, va, n):
        base, off, sz = self.sec[".rodata"]
        if not (base <= va < base + sz) or va + n > base + sz:
            return None
        return self.data[off + (va - base): off + (va - base) + n]

    def relocs(self):
        base, off, sz = self.sec[".rela.dyn"]
        out = {}
        for i in range(sz // 24):
            r_off, r_info, r_add = struct.unpack_from("<QQq", self.data, off + i * 24)
            out[r_off] = (r_info & 0xFFFFFFFF, r_add)
        return out

    # -- panic::Location ------------------------------------------------

    def locations(self):
        """`.data.rel.ro` 里 24 字节的 `Location` 结构体 → {struct_va: (file, line, col)}。

        自校验：第 0 个 8 字节槽是 `R_*_RELATIVE`，addend = 文件名字面量地址；
        槽位 +8 是 len。按 len 取出的字节必须全 ASCII 且以 `.rs` 结尾。
        这一条同时绕开「`.rodata` 字面量非 NUL 结尾、strings 会把相邻字面量粘成一条」的坑。
        """
        want = RELATIVE_BY_EM[self.em]
        dr_va, dr_off, dr_sz = self.sec[".data.rel.ro"]
        out = {}
        for r_off, (r_type, add) in self.relocs().items():
            if r_type != want or not (dr_va <= r_off < dr_va + dr_sz):
                continue
            i = dr_off + (r_off - dr_va)
            if i + 24 > len(self.data):
                continue
            _p, ln, line, col = struct.unpack_from("<QQII", self.data, i)
            if not (4 < ln < 300):
                continue
            raw = self.ro_bytes(add, ln)
            if raw is None:
                continue
            try:
                s = raw.decode("ascii")
            except UnicodeDecodeError:
                continue
            if not s.endswith(".rs") or not (1 <= line < 100000 and 1 <= col < 2000):
                continue
            out[r_off] = (s, line, col)
        return out

    # -- 符号表 --------------------------------------------------------

    def func_symbols(self):
        "[(mangled, addr, size)]：`.symtab` 里已定义的 `FUNC`。stripped 时返回 []。"
        if ".symtab" not in self.sec:
            return []
        rows = []
        for line in run(["readelf", "-sW", self.path]).splitlines():
            p = line.split()
            if len(p) < 8 or p[3] != "FUNC" or p[1] == "UND":
                continue
            try:
                rows.append((p[7], int(p[1], 16), int(p[2])))
            except ValueError:
                continue
        return rows


# ---------------------------------------------------------------- 指令扫描

class LocIndex:
    """`Location` 结构体的 24 字节布局 → 「这个操作数目标算不算引用某个 Location」。

        +0  file: &'static str 的**指针槽**（`R_*_RELATIVE` 重定位）
        +8  file.len: usize
        +16 line: u32, col: u32

    所以 `mov rax, [loc]`（x86）/ `ldr x0, [x8, #off]`（arm64）取的是 file 指针，
    目标是 `loc` 本身；而取 `len`/`line` 的目标是 `loc+8`/`loc+16`。三者都算「引用了该
    Location」。**两侧必须用同一套规则**，否则 join 的一对一会被人为破坏。
    """

    __slots__ = ("addrs",)

    def __init__(self, addrs):
        self.addrs = set(addrs)

    def __contains__(self, a):
        return a in self.addrs

    def lookup(self, target):
        "target 落在某个 Location 结构体的 24 字节里 ⇒ 返回结构体起始地址，否则 None。"
        for back in (0, 8, 16):
            if target - back in self.addrs:
                return target - back
        return None


def refs_x86(elf, locs):
    """x86-64：全 `.text` 线性反汇编，收集所有 RIP 相对寻址/立即数的目标落在 Location 上的点。

    为什么要「所有 RIP 相对操作数」而不只 `lea`：`panic::Location` 有时被 `mov`/`cmp`
    直接寻址；只认 `lea` 会漏（2024 版的 tools/symtab_dump.py 只认 lea，本脚本放宽）。
    返回 {insn_va: [loc_va, ...]}。
    """
    import capstone
    X = capstone.x86
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.detail = True
    base, off, sz = elf.sec[".text"]
    code = elf.data[off:off + sz]
    out = collections.defaultdict(list)
    for insn in md.disasm(code, base):
        end = insn.address + insn.size
        for op in insn.operands:
            tgt = None
            if op.type == X.X86_OP_MEM and op.mem.base == X.X86_REG_RIP:
                tgt = end + op.mem.disp
            elif op.type == X.X86_OP_IMM:
                tgt = op.imm
            if tgt is None:
                continue
            lv = locs.lookup(tgt)
            if lv is not None:
                out[insn.address].append(lv)
    return out


# arm64 上会打断 adrp→add 生命周期的指令：调用会冲掉 caller-saved 寄存器，
# 分支让线性扫描的「已知值」不再可信 ⇒ 一律清空重来。不清会让陈旧值在后续基本块里
# 被当成 Location 地址，既产生假引用也掩盖真引用（第一版实测 arm64 只解出 2783 个键，
# 而 x86 侧 4666 个，差的就是这一类）。
ARM64_BARRIER = {"bl", "blr", "br", "ret", "cbz", "cbnz", "tbz", "tbnz", "b"}


def refs_arm64(elf, locs):
    """aarch64：`adrp xN, page` + `add xN, xN, #lo12`（取地址）/ `ldr`（读字段）。

    顺序扫一遍 `.text`，维护 reg → 已知绝对值的表；任何操作数（寄存器 / 内存基址+偏置 /
    立即数）若带已知值，就把目标交给 `LocIndex.lookup` 判定。
    返回 {insn_va: [loc_va, ...]}。
    """
    import capstone
    A = capstone.arm64
    md = capstone.Cs(capstone.CS_ARCH_ARM64, capstone.CS_MODE_ARM)
    md.detail = True
    base, off, sz = elf.sec[".text"]
    code = elf.data[off:off + sz]
    known = {}
    out = collections.defaultdict(list)

    def hit(target, insn_va):
        lv = locs.lookup(target)
        if lv is not None:
            out[insn_va].append(lv)

    for insn in md.disasm(code, base):
        m, ops = insn.mnemonic, insn.operands
        if m == "adrp" and len(ops) == 2 and ops[0].type == A.ARM64_OP_REG \
                and ops[1].type == A.ARM64_OP_IMM:
            known[ops[0].reg] = ops[1].imm
            continue
        if m == "adr" and len(ops) == 2 and ops[0].type == A.ARM64_OP_REG \
                and ops[1].type == A.ARM64_OP_IMM:
            known[ops[0].reg] = ops[1].imm
            hit(ops[1].imm, insn.address)
            continue
        if m in ("add", "sub") and len(ops) >= 3 \
                and ops[0].type == A.ARM64_OP_REG and ops[1].type == A.ARM64_OP_REG \
                and ops[1].reg in known and ops[2].type == A.ARM64_OP_IMM:
            v = known[ops[1].reg] + (ops[2].imm if m == "add" else -ops[2].imm)
            known[ops[0].reg] = v
            hit(v, insn.address)
            continue
        for op in ops:  # 一般指令：只看读操作数
            if op.type == A.ARM64_OP_MEM:
                if op.mem.base in known:
                    hit(known[op.mem.base] + op.mem.disp, insn.address)
            elif op.type == A.ARM64_OP_REG:
                if op.reg in known:
                    hit(known[op.reg], insn.address)
            elif op.type == A.ARM64_OP_IMM:
                hit(op.imm, insn.address)
        if ops and ops[0].type == A.ARM64_OP_REG:
            known.pop(ops[0].reg, None)   # 目标寄存器被覆盖 ⇒ 旧值作废
        if m in ARM64_BARRIER or m.startswith("b."):
            known.clear()
    return out


# ---------------------------------------------------------------- 函数边界

def ranges_from_symtab(elf):
    """arm64：`.symtab` 的 (addr, size) 就是权威边界；size==0 的用「下一个符号起点」兜底。"""
    tex_va, _, tex_sz = elf.sec[".text"]
    lo, hi = tex_va, tex_va + tex_sz
    syms = [(a, s, n) for n, a, s in elf.func_symbols() if lo <= a < hi]
    syms.sort()
    starts = [a for a, _s, _n in syms]
    out = {}
    for a, s, n in syms:
        i = bisect.bisect_right(starts, a)
        nxt = starts[i] if i < len(starts) else hi
        out[a] = (n, max(s, 0) or (nxt - a))
    return out


def ranges_from_funcstarts(path, elf):
    """x86-64：用仓库的 Ghidra 派生函数起点表（**不是** `.eh_frame`，那份是错的）。

    `analysis/funcstarts_ghidra_va.pkl` 已经是 ELF 虚地址空间（值域 `0x37b940…0x94eb70`），
    与 `analysis/funcstarts.pkl`（`.eh_frame` 解析、13430 条、与 Ghidra 只交集 2 个）无关。
    """
    tex_va, _, tex_sz = elf.sec[".text"]
    lo, hi = tex_va, tex_va + tex_sz
    raw = pickle.load(open(path, "rb"))
    starts = sorted({int(x, 16) if isinstance(x, str) else int(x) for x in raw})
    starts = [a for a in starts if lo <= a < hi]
    out = {}
    for i, a in enumerate(starts):
        nxt = starts[i + 1] if i + 1 < len(starts) else hi
        out[a] = (None, nxt - a)
    return out


def owner_of(ranges, starts, va):
    "含 va 的函数起点（最近的前驱起点）；不在任何区间里返回 None。"
    i = bisect.bisect_right(starts, va) - 1
    if i < 0:
        return None
    a = starts[i]
    _n, sz = ranges[a]
    return a if va < a + sz else None


# ---------------------------------------------------------------- 名字解析

def demangle(syms):
    if not syms:
        return []
    out = run(["c++filt", "--format", "rust"], input="\n".join(syms))
    lines = out.splitlines()
    if len(lines) != len(syms):
        warn(f"c++filt 行数 {len(lines)} != 输入 {len(syms)}")
    return lines


def split_top(s, sep="::"):
    parts, depth, cur, i = [], 0, "", 0
    while i < len(s):
        if s.startswith("::", i) and depth == 0:
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

    三种形态都要吃（c++filt 的三种输出，见 `tools/rust_sym.py` 的判据表）：

    * **direct**  `graphwarrust::mod::Type::method`
    * **impl-head** `<graphwarrust::mod::Type as Trait>::method`
      —— GDExtension 的 trait impl。crate 名在这条符号的 mangled 形态里**没有长度前缀**，
      所以「前缀过滤」会把它整类丢掉；这里按 `<SELF as TRAIT>` 的 SELF 取路径。
    * **嵌套 impl**  `graphwarrust::mod::_::<impl Trait for graphwarrust::Type>::method`
      —— `#[derive(Serialize)]` 那一类，crate 首段仍是 `graphwarrust`，走进第三条分支。

    调用方必须先按 `rust_sym.is_game_name(mangled)` 把**符号**筛过一遍；
    `parse_name` 只负责把一条已知属于本 crate 的名字切成三列，不再兼职做 crate 判定
    （旧版靠「demangle 后文本里有没有 `graphwarrust::`」隐式筛选，会把
    `core::ptr::drop_in_place<graphwarrust::X>` 这类胶水也算进来）。
    """
    d = HASH_SUFFIX.sub("", d)
    is_closure = CLOSURE in d
    # 形态 B：`<SELF as TRAIT>::tail`
    head = rust_sym.impl_head_tail(d)
    if head and "<impl " not in head[0]:
        # 注意 `...::_::<impl Trait for graphwarrust::T>` 这种**嵌套 impl** 不在这里解：
        # 它的外层 `<...>` 是一条路径、末段才是匿名 impl 块，按老规则（下面第三条分支）
        # 得到 `type='_'`，与 `fixtures/name_to_addr.tsv` 的既有列一致。
        self_ty, tail = head
        segs = [s for s in rust_sym.split_top(rust_sym.split_top(self_ty, " as ")[0], "::")
                if s]
        if len(segs) < 2 or segs[0] != rust_sym.DEFAULT_CRATE:
            return None
        module = segs[1]
        owner = segs[2].split("<")[0] if len(segs) > 2 and segs[2][:1].isupper() else None
        rest = [s for s in rust_sym.split_top(tail, "::") if s]
        if CLOSURE in rest:
            rest = rest[:rest.index(CLOSURE)]
        method = clean_method(rest) or owner
        return module, owner, method, is_closure
    if d.startswith(rust_sym.DEFAULT_CRATE + "::") and "<impl " not in d:
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
    # 形态 C（嵌套 impl）：路径首段已是本 crate，`<impl …>` 出现在中段。
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


def is_boilerplate(method):
    if not method:
        return True
    return method in BOILERPLATE or method.split("::")[0] in BOILERPLATE


# ---------------------------------------------------------------- 构建

def build(arm64_path, x86_path, funcstarts_path):
    a, b = Elf(arm64_path), Elf(x86_path)

    # 两侧的 Location 集合（按结构体地址查；join 的键是 (file, line)）
    loc_a, loc_b = a.locations(), b.locations()
    if not loc_a or not loc_b:
        sys.exit("Location 解码失败（一侧为 0）：检查 .rela.dyn 的重定位类型")
    warn(f"arm64 Locations {len(loc_a)} / x86-64 Locations {len(loc_b)}")

    # ① 指令 → 引用了哪个 Location
    idx_a, idx_b = LocIndex(loc_a), LocIndex(loc_b)
    ra = refs_arm64(a, idx_a)
    rb = refs_x86(b, idx_b)
    touched_a = {lv for ls in ra.values() for lv in ls}
    touched_b = {lv for ls in rb.values() for lv in ls}
    src_touched_a = {lv for lv in touched_a if loc_a[lv][0].startswith("src/")}
    src_touched_b = {lv for lv in touched_b if loc_b[lv][0].startswith("src/")}
    src_all_a = {lv for lv, v in loc_a.items() if v[0].startswith("src/")}
    src_all_b = {lv for lv, v in loc_b.items() if v[0].startswith("src/")}
    warn(f"引用点 arm64 {len(ra)} / x86-64 {len(rb)}")
    # 对称性检查：同一份源码在两侧被引用到的 src Location 结构体应当数量相近
    warn(f"被代码引用的 Location 结构体：arm64 {len(touched_a)} (src {len(src_touched_a)}"
         f"/{len(src_all_a)})，x86-64 {len(touched_b)} (src {len(src_touched_b)}/{len(src_all_b)})")

    # ② 函数边界
    #
    # ⚠️ 边界用**全部** `.symtab` 符号（`ranges_from_symtab` 直接读 elf），名字只用
    # **本 crate** 的符号。两件事必须分开：边界是「这段字节属于哪个函数」的事实，
    # 与「这个函数叫什么名字」无关；混在一起会让一个非游戏符号把相邻函数切错。
    syms_all = a.func_symbols()
    syms_a = [(n, addr, sz) for (n, addr, sz) in syms_all if rust_sym.is_game_name(n)]
    rng_a = ranges_from_symtab(a)
    rng_b = ranges_from_funcstarts(funcstarts_path, b)
    starts_a = sorted(rng_a)
    starts_b = sorted(rng_b)
    warn(f"函数 arm64(symtab) {len(starts_a)} / x86-64(ghidra) {len(starts_b)}")
    warn(f"符号 arm64 全部 {len(syms_all)} → graphwarrust 自己的 {len(syms_a)}"
         f"（判据见 tools/rust_sym.py；不是子串匹配）")

    # ③ 函数 → {(file,line,col)}
    #
    # join 的键用**三元组**而不是 (file,line)：`Location` 常量本身带 col，
    # 两侧必然逐字相同；而同一行常有多个 `.expect()`（本二进制里 281 个 (file,line)
    # 带 >1 个 col），只用 (file,line) 会把本来唯一的锚人为变成歧义。
    # 歧义率仍按 (file,line) 单独统计并报告（见 stats）。
    anch_a = collections.defaultdict(set)   # sym_addr -> {(file,line,col)}
    anch_b = collections.defaultdict(set)   # func_va  -> {(file,line,col)}
    for insn, locs in ra.items():
        f = owner_of(rng_a, starts_a, insn)
        if f is None:
            continue
        for lv in locs:
            anch_a[f].add(loc_a[lv])
    for insn, locs in rb.items():
        f = owner_of(rng_b, starts_b, insn)
        if f is None:
            continue
        for lv in locs:
            anch_b[f].add(loc_b[lv])

    # ④ join：只接受双向唯一
    #
    # ⚠️ **必须排序**：`anch_a[f]` 是 set，元素是 `(file,line,col)` 字符串元组，
    # 其迭代顺序受 Python 的 per-process hash 随机化影响。而 ⑤ 的消去法是
    # **顺序敏感**的贪心（「只剩一个候选」是在当时已占用的集合上判定的），
    # 于是同一份输入换个进程就能给出不同的地址。2026-11 实测：`PYTHONHASHSEED`
    # 0/1/2/3/4 下 `save_game` 分别落到 0x50b130 / 0x508aba / 0x508aba /
    # 0x3d5740 / 0x3d5740 —— 同一份二进制、同一个工具，三种答案。
    # 这里把所有「顺序会泄漏到结果」的迭代都钉死成字典序/数值序。
    owner_a = collections.defaultdict(set)  # key -> {arm64 func}
    owner_b = collections.defaultdict(set)
    for f, s in sorted(anch_a.items()):
        for k in sorted(s):
            owner_a[k].add(f)
    for f, s in sorted(anch_b.items()):
        for k in sorted(s):
            owner_b[k].add(f)

    unique = {}
    for k, fa in owner_a.items():
        fb = owner_b.get(k)
        if fb and len(fa) == 1 and len(fb) == 1:
            unique[k] = (next(iter(fa)), next(iter(fb)))

    # 一个 arm64 函数被多个唯一锚指向 x86 的两个不同地址 ⇒ 该名字在 x86 侧被拆开，必须报
    pair_votes = collections.defaultdict(collections.Counter)
    for k, (fa, fb) in unique.items():
        pair_votes[fa][fb] += 1
    conflicts = {fa: c for fa, c in pair_votes.items() if len(c) > 1}

    # ⑤ 约束传播：把「直接唯一」的配对当种子，用**一一对应**（一个函数只可能是一个函数）
    #    消去已占用的 x86 地址，使一部分原本一对多的锚塌缩成唯一。
    #
    #    这一步是**消去法**，不是猜：只有当某个函数的候选地址集合被消到只剩 1 个时才落定。
    #    仍然坚持「不强行选一个」——`pair_votes` 里有多候选的函数（x86 侧被拆开的那种）
    #    不参与种子，也不参与传播。
    direct = {fa: next(iter(c)) for fa, c in pair_votes.items() if len(c) == 1}
    claimed = collections.Counter(direct.values())
    direct = {fa: fb for fa, fb in direct.items() if claimed[fb] == 1}   # x86 地址被两个名字抢 ⇒ 不当种子
    assign = dict(direct)
    used_b = set(assign.values())

    keys_a = {f: {k for k in s if k in owner_b} for f, s in anch_a.items()}
    keys_b = {f: {k for k in s if k in owner_a} for f, s in anch_b.items()}
    shared = [k for k in owner_a if k in owner_b]

    propagated = {}
    for _round in range(20):
        changed = False
        # 规则 1：某个锚的两侧未定候选都只剩 1 个
        for k in sorted(shared):
            A = [f for f in owner_a[k] if f not in assign]
            B = {f for f in owner_b[k] if f not in used_b}
            if len(A) == 1 and len(B) == 1:
                fa, fb = A[0], next(iter(B))
                assign[fa] = fb
                used_b.add(fb)
                propagated[fa] = fb
                changed = True
        # 规则 2：某个 arm64 函数的所有未定候选地址只剩 1 个
        for fa in sorted(anch_a):
            if fa in assign or not keys_a[fa]:
                continue
            B = {b for k in keys_a[fa] for b in owner_b.get(k, ()) if b not in used_b}
            if len(B) == 1:
                fb = next(iter(B))
                assign[fa] = fb
                used_b.add(fb)
                propagated[fa] = fb
                changed = True
        # 规则 3：某个 x86 函数的所有未定候选名字只剩 1 个（反向，同样只求唯一）
        for fb in sorted(anch_b):
            if fb in used_b or not keys_b[fb]:
                continue
            A = {a for k in keys_b[fb] for a in owner_a.get(k, ()) if a not in assign}
            if len(A) == 1:
                fa = next(iter(A))
                assign[fa] = fb
                used_b.add(fb)
                propagated[fa] = fb
                changed = True
        if not changed:
            break

    # ⑤ 名字
    #
    # `syms_a` 已经过 `rust_sym.is_game_name` 筛选（②），所以这里每一条都是本 crate 的
    # 符号；`sym_form` 只记录**它是哪种 mangling 形态**（provenance，不参与配对）：
    #   direct      = `_ZN12graphwarrust…` 直接路径
    #   impl-head   = `_ZN<len>_$LT$graphwarrust..T$u20$as$u20$Trait$GT$…`（没有长度前缀）
    #   nested-impl = `_ZN12graphwarrust…<impl Trait for graphwarrust::T>…`
    names = demangle([n for n, _a, _s in syms_a])
    sym_info = {}
    for (mangled, addr, size), d in zip(syms_a, names):
        sym_info[addr] = (d, size, rust_sym.form_label(mangled), mangled)

    return dict(a=a, b=b, loc_a=loc_a, loc_b=loc_b, ra=ra, rb=rb,
                rng_a=rng_a, rng_b=rng_b, starts_a=starts_a, starts_b=starts_b,
                anch_a=anch_a, anch_b=anch_b, owner_a=owner_a, owner_b=owner_b,
                unique=unique, conflicts=conflicts, sym_info=sym_info, pair_votes=pair_votes,
                assign=assign, direct=direct, propagated=propagated,
                touched_a=touched_a, touched_b=touched_b)


# ---------------------------------------------------------------- 次级证据

# 关于「次级证据」：本工具**不产出**任何 `次级证据` 行，这是有意的。
#
# 任务允许在无锚时用「独有字符串引用 / 调用邻居 / 函数大小相近」兜底，但 `docs/02` §19
# 已经证否了最后一条：08-27 → 08-28 这一版 `.text` 净差只有 +272 字节，函数却被整体重排，
# 采样器 `0x3c2530` 在 08-27 侧根本没有同尺寸对应物（最接近的 `0x4e18e0` 相似度 0.859）。
# ⇒ 在这个版本对上，函数大小/形态的分辨率不足以定名，用了就是猜。
# 字符串引用同理：`.rodata` 串只证明「被这片代码引用」，不证明归属（§16.4 第 3 条）。
# 所以无锚一律落 `未知` 或 `歧义`，宁可少给名字，也不给错名字。


# ---------------------------------------------------------------- 输出

def resolve_funcstarts(p):
    """向上逐级找上游仓库的 `analysis/funcstarts_ghidra_va.pkl`。

    本仓库在 `<上游>/gw2-rebuild/` 下 ⇒ 正常情况是 `tools/../../analysis/`；但子 agent 的
    克隆在 `.agent-work/<topic>/repo/` 里，层级更深，所以逐级上溯到工作区根为止。
    """
    if p:
        if os.path.exists(p):
            return p
        sys.exit(f"--funcstarts 指定的文件不存在：{p}")
    here = os.path.dirname(os.path.abspath(__file__))
    d = here
    for _ in range(6):
        cand = os.path.join(d, "analysis", "funcstarts_ghidra_va.pkl")
        if os.path.exists(cand):
            return cand
        d = os.path.dirname(d)
    sys.exit("找不到 analysis/funcstarts_ghidra_va.pkl：用 --funcstarts 指定。\n"
             "⚠ 不要用 analysis/funcstarts.pkl（.eh_frame 解析，表是错的，见 AGENTS.md）。")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", nargs="?", default="build",
                    choices=["build", "stats", "module", "probe", "conflicts"])
    ap.add_argument("arg", nargs="*")
    ap.add_argument("--arm64", default=os.environ.get("GW2_ARM64_SO", DEFAULT_ARM64))
    ap.add_argument("--x86", default=os.environ.get("GW2_X86_SO", DEFAULT_X86))
    ap.add_argument("--funcstarts", default=os.environ.get("GW2_FUNCSTARTS"))
    ap.add_argument("--out", default=None, help="TSV 输出路径（缺省 fixtures/name_to_addr.tsv）")
    o = ap.parse_args()

    for p in (o.arm64, o.x86):
        if not os.path.exists(p):
            sys.exit(f"缺输入：{p}")
    st = build(o.arm64, o.x86, resolve_funcstarts(o.funcstarts))
    a, b = st["a"], st["b"]

    if o.cmd == "probe":
        for s in o.arg:
            va = int(s, 0)
            print(f"=== x86 {s} ===")
            hits = st["anch_b"].get(va)
            print("  is func entry:", va in st["rng_b"])
            if hits:
                own = sorted(x for x in hits if x[0].startswith("src/"))
                print(f"  anchors: {len(hits)} (src/*.rs: {len(own)})")
                for k in own:
                    f, l, c = k
                    na = st["owner_a"].get(k, set())
                    nb = st["owner_b"].get(k, set())
                    print(f"    {f}:{l}:{c}  arm64_owners={len(na)} x86_owners={len(nb)}"
                          f"  unique={len(na) == 1 and len(nb) == 1}")
                    for x in sorted(na):
                        d = st["sym_info"].get(x, ("?", 0, "?", ""))[0]
                        print(f"        arm64 {hex(x)} {d}")
            else:
                print("  anchors: 0")
        return

    if o.cmd == "stats":
        print_stats(st)
        return

    if o.cmd == "conflicts":
        for fa, c in st["conflicts"].items():
            d = st["sym_info"].get(fa, ("?", 0, "?", ""))[0]
            print(f"{hex(fa)} {d}")
            for fb, n in c.most_common():
                print(f"    {hex(fb)}  votes={n}")
        return

    if o.cmd == "module":
        want = o.arg[0] if o.arg else None
        rows = rows_for_tsv(st)
        for r in rows:
            if want and want not in (r["module"] or ""):
                continue
            print("\t".join(str(r[k]) for k in
                            ("module", "type", "method", "x86_addr", "src_file",
                             "src_line", "join_kind", "sym_form", "evidence")))
        return

    # build
    rows = rows_for_tsv(st)
    out_path = o.out or os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                     "..", "fixtures", "name_to_addr.tsv")
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as f:
        f.write("# 由 tools/name_bridge.py build 生成，不要手改。\n")
        f.write("# x86_addr 是 Linux 2.0.60 的 ELF 虚地址；Ghidra/REA 地址 = 该值 + 0x100000。\n")
        f.write("# 名字来自 Android 2.0.60 arm64 的 .symtab，经 panic::Location 的 (file,line) join 到 x86。\n")
        f.write("# 手工裁决（%d 条）已内建在 name_bridge.py 的 MANUAL_FIXES 里，`build` 直接产出本文件：\n"
                % len(MANUAL_FIXES))
        for k in sorted(MANUAL_FIXES):
            f.write("#   %s::%s::%s @ %s:%s\n" % k)
        f.write("# sym_form = 名字来自哪种 mangling 形态（direct / impl-head / nested-impl；判据见 tools/rust_sym.py）。\n")
        f.write("# 只统计 `graphwarrust` 自己的符号：`_ZN12graphwarrust…`（direct）、"
                "`_ZN<len>_$LT$graphwarrust..T$u20$as$u20$Trait$GT$…`（impl-head，crate 名没有长度前缀）、"
                "以及 `graphwarrust::…<impl Trait for graphwarrust::T>…`（nested-impl）。\n")
        f.write("\t".join(("module", "type", "method", "x86_addr", "src_file",
                           "src_line", "join_kind", "sym_form", "evidence")) + "\n")
        for r in rows:
            f.write("\t".join(str(r[k]) for k in
                              ("module", "type", "method", "x86_addr", "src_file",
                               "src_line", "join_kind", "sym_form", "evidence")) + "\n")
    warn(f"写出 {out_path}：{len(rows)} 行")
    tally = collections.Counter(r["join_kind"] for r in rows)
    warn("  " + ", ".join(f"{k}={v}" for k, v in tally.most_common()))


def _spearman(pairs):
    """两列秩的 Spearman 相关系数。

    用途：**独立于 join 本身的正确性检查**。rustc 按固定顺序给 CGU 排函数，
    两侧 `.text` 里同一函数的相对次序应当高度一致。若 join 全是错的，
    这个值会掉到 0 附近；正确时应当 >0.9。
    """
    n = len(pairs)
    if n < 3:
        return None
    rank = lambda xs: {v: i for i, v in enumerate(sorted(xs))}
    rx, ry = rank([p[0] for p in pairs]), rank([p[1] for p in pairs])
    d2 = sum((rx[a] - ry[b]) ** 2 for a, b in pairs)
    return 1 - 6 * d2 / (n * (n * n - 1))


def print_stats(st):
    src = lambda s: any(k[0].startswith("src/") for k in s)
    fa = {f for f, s in st["anch_a"].items() if src(s)}
    fb = {f for f, s in st["anch_b"].items() if src(s)}
    print(f"arm64 函数（.symtab，限 .text）{len(st['starts_a'])}；带任意锚 {len(st['anch_a'])}；"
          f"带 src 锚 {len(fa)}")
    print(f"x86-64 函数（Ghidra 起点表）  {len(st['starts_b'])}；带任意锚 {len(st['anch_b'])}；"
          f"带 src 锚 {len(fb)}")
    print(f"被引用的 Location 键数（三元组）：arm64 {len(st['owner_a'])} / x86 {len(st['owner_b'])}")

    nuniq = len(st["unique"])
    ca = len({x[0] for x in st["unique"].values()})
    cb = len({x[1] for x in st["unique"].values()})
    print(f"\n双向唯一锚 {nuniq} 个；覆盖 arm64 函数 {ca}、x86 函数 {cb}")

    # 歧义率（质量指标）：按 key 统计「两侧都出现、但不是一对一」的比例
    both = [k for k in st["owner_a"] if k in st["owner_b"]]
    src_both = [k for k in both if k[0].startswith("src/")]
    for label, keys in (("全部", both), ("仅 src/*.rs", src_both)):
        amb = [k for k in keys if not (len(st["owner_a"][k]) == 1 and len(st["owner_b"][k]) == 1)]
        if keys:
            print(f"歧义率（{label}）：{len(amb)}/{len(keys)} 个键非一对一"
                  f" = {100.0 * len(amb) / len(keys):.1f}%")

    # 只用 (file,line) 的歧义率，作为对照（任务口径要求报告）
    fl_a = collections.defaultdict(set)
    for f, s in st["anch_a"].items():
        for k in s:
            fl_a[k[:2]].add(f)
    fl_b = collections.defaultdict(set)
    for f, s in st["anch_b"].items():
        for k in s:
            fl_b[k[:2]].add(f)
    common_fl = [k for k in fl_a if k in fl_b and k[0].startswith("src/")]
    bad = [k for k in common_fl if not (len(fl_a[k]) == 1 and len(fl_b[k]) == 1)]
    if common_fl:
        print(f"歧义率（仅 src，退化到 (file,line) 口径）：{len(bad)}/{len(common_fl)}"
              f" = {100.0 * len(bad) / len(common_fl):.1f}%"
              f"  ← 这就是「同一行被多个函数共享」的量")

    # 消去法（约束传播）
    direct, prop = st.get("direct", {}), st.get("propagated", {})
    print(f"\n直接唯一配对 {len(direct)} 对；消去法额外确定 {len(prop)} 对"
          f"（合计 {len(direct) + len(prop)}）")
    print(f"硬矛盾（一个名字在 x86 侧被拆成多个函数，保留为歧义）{len(st['conflicts'])}")

    # 正确性旁证 1：次序相关
    rank_a = {a: i for i, a in enumerate(st["starts_a"])}
    rank_b = {a: i for i, a in enumerate(st["starts_b"])}
    pairs = [(rank_a[x], rank_b[y]) for x, y in st["unique"].values()
             if x in rank_a and y in rank_b]
    rho = _spearman(pairs)
    if rho is not None:
        print(f"次序相关（Spearman，{len(pairs)} 对）：{rho:.4f}"
              f"  ← 两侧同一函数的相对编译次序；接近 1 说明配对整体可信")

    # 正确性旁证 2：函数大小比例（x86 一般略大于 arm64）
    ratios = []
    for x, y in st["unique"].values():
        sa = st["rng_a"].get(x, (None, 0))[1]
        sb = st["rng_b"].get(y, (None, 0))[1]
        if sa > 16 and sb > 16:
            ratios.append(sb / sa)
    if ratios:
        ratios.sort()
        med = ratios[len(ratios) // 2]
        within = sum(1 for r in ratios if 0.33 <= r <= 3.0)
        print(f"函数大小比 x86/arm64：中位数 {med:.2f}；落在 [1/3, 3] 内的 {within}/{len(ratios)}"
              f" = {100.0 * within / len(ratios):.1f}%  ← 明显离群的配对是可疑的")


def best_anchor(st, fa, module):
    """展示用锚：优先取**本模块自己的** `src/<module>.rs`，否则取任意 src 锚，再否则取任意锚。

    为什么：一个函数体里常有内联进来的别的模块的 `Location`（如 `calculate_shot_results`
    里就有 `graphwar_obstacle_manager.rs` 的锚）。展示列若随便取一个，会让人误以为
    该函数属于那个模块。
    """
    src = sorted(st["anch_a"].get(fa, ()), key=lambda t: (t[0], t[1], t[2]))
    want = f"src/{module}.rs"
    for k in src:
        if k[0] == want:
            return k, True
    for k in src:
        if k[0].startswith("src/"):
            return k, False
    return (src[0] if src else ("", "", "")), False


# ---------------------------------------------------------------- 手工裁决
#
# `build` 的输出**曾经**要靠人手改：这两条地址是工具算错、又被独立证据推翻的，
# 手工改正后只留在夹具的注释里 —— 于是「重跑 build」必然把裁决覆盖掉（谁重跑谁背锅）。
# 2026-11 把它们搬进来：**生成即可复现**，且每条都写清为什么推翻工具值。
#
# 键 = (module, type, method, src_file, src_line)；值 = 覆盖项。
MANUAL_FIXES = {
    # 0x4eb86a **不是指令边界**：它落在 `0x4eb868 add eax,-3` 中间。
    # 该函数（`MathFunction::remove_offset`）的入口是 0x4eb830（objdump 可验）。
    ("math_function", "MathFunction", "remove_offset",
     "src/math_function.rs", "52"): dict(
        x86_addr="0x4eb830",
        evidence="2 个双向唯一 (file,line,col) 锚；展示锚 src/math_function.rs:52:43"
                 "（工具原值 0x4eb86a 不是指令边界 —— 落在 0x4eb868 `add eax,-3` 中间）"),
    # 已被内联进 0x4ec160（该 .text 区间只有一个函数体）。
    ("math_function", "MathFunction", "to_string_recursive",
     "src/math_function.rs", "285"): dict(
        x86_addr="0x4ec160",
        evidence="1 个双向唯一（该函数已被内联进 0x4ec160） (file,line,col) 锚；"
                 "展示锚 src/math_function.rs:285:28"),
    # ⚠️ 工具值随 PYTHONHASHSEED 漂移（0x508aba / 0x50b130 / 0x3d5740 三种都出现过，
    #    见 build() ④ 的排序注释）。**只有 0x3d5740 有三份独立证据**：
    #    Windows 侧唯一锚 2/2（fixtures/windows_name_to_addr.tsv）、
    #    sym_match 的 unique 候选、name_to_addr_v2 的三方一致裁决。
    ("graphwar_game_recorder", "GraphwarGameRecorder", "save_game",
     "src/graphwar_game_recorder.rs", "150"): dict(
        x86_addr="0x3d5740",
        evidence="消去法唯一：无直接一对一锚，按「一个名字只对应一个地址」消去后只剩 "
                 "0x3d5740；展示锚 src/graphwar_game_recorder.rs:150:81"
                 "（工具消去法给的是 0x508aba，与 Windows 侧唯一锚 2/2、sym_match、v2 三方冲突，"
                 "按独立证据取 0x3d5740）"),
    # 这一行本来就写着「消去法唯一」，`join_kind` 列却标成 `唯一锚` —— 分档写错了。
    ("math_function", "FunctionRandomizer", "mutate_function",
     "/rustc/ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96/library/core/src/slice/index.rs",
     "1020"): dict(join_kind="消去法"),
}


def apply_manual_fixes(rows):
    """把 `MANUAL_FIXES` 应用到行上；返回实际命中的键（供写盘时记账）。"""
    hit = []
    for r in rows:
        # `src_line` 在行里是 int（来自 Location 元组），而 MANUAL_FIXES 的键写成字符串
        # （夹具里那一列就是文本）。这里统一成 str，否则裁决**永远命不中**且不报错
        # ——2026-11 就因为这个类型不一致，第一次跑出来的文件看起来「裁决没生效」。
        k = (r["module"], r["type"], r["method"], r["src_file"], str(r["src_line"]))
        fix = MANUAL_FIXES.get(k)
        if not fix:
            continue
        r.update(fix)
        hit.append(k)
    return hit


def rows_for_tsv(st):
    """一行 = 一个 arm64 函数（名字）在最优证据下的 x86 地址。"""
    sym_info = st["sym_info"]
    votes = st["pair_votes"]
    assign = st.get("assign", {})
    direct = st.get("direct", {})
    propagated = st.get("propagated", {})
    rows = []
    # 先按 arm64 函数聚合：它拿到哪些唯一锚、指向哪个 x86 地址
    for fa, info in sorted(sym_info.items()):
        d, size, form, _raw = info
        parsed = parse_name(d)
        if not parsed:
            continue
        module, owner, method, is_closure = parsed
        if owner is None and method is None:
            continue
        if is_boilerplate(method) or method == "_" or method == owner:
            continue
        c = votes.get(fa)
        # (a) 直接双向唯一
        if fa in direct:
            fb = direct[fa]
            pick, pick_own = best_anchor(st, fa, module)
            rows.append(dict(module=module, type=owner or "_", method=method,
                             x86_addr=hex(fb), src_file=pick[0], src_line=pick[1],
                             join_kind="唯一锚",
                             evidence=(f"{c[fb]} 个双向唯一 (file,line,col) 锚；"
                                       f"展示锚 {pick[0]}:{pick[1]}:{pick[2]}"
                                       f"{'' if pick_own else '（非本模块文件，属内联）'}"),
                             _closure=is_closure, _size=size, _a=fa))
            continue
        # (b) 直接一对多，但用「一一对应」消去已定配对后只剩一个候选
        if fa in propagated:
            fb = propagated[fa]
            pick, pick_own = best_anchor(st, fa, module)
            rows.append(dict(module=module, type=owner or "_", method=method,
                             x86_addr=hex(fb), src_file=pick[0], src_line=pick[1],
                             join_kind="唯一锚",
                             evidence=("消去法唯一：无直接一对一锚，按「一个名字只对应一个地址」"
                                       f"消去后只剩 {hex(fb)}；展示锚 {pick[0]}:{pick[1]}:{pick[2]}"),
                             _closure=is_closure, _size=size, _a=fa))
            continue
        # (c) 一个 arm64 函数被唯一锚指向**多个** x86 地址 ⇒ 该函数在两个构建里被拆开了
        # （arm64 内联进本体的代码，在 x86 上被单独甩成一个函数，反之亦然）。
        # 任务口径：**不强行选一个**，标「歧义」并把候选与判据写进 evidence。
        if c and len(c) > 1:
            cands = []
            for fb, n in c.most_common():
                ks = [k for k, v in st["unique"].items() if v == (fa, fb)]
                ownmod = sum(1 for k in ks if k[0] == f"src/{module}.rs")
                cands.append(f"{hex(fb)}={n}票"
                             f"（本模块键 {ownmod}/{len(ks)}）")
            rows.append(dict(module=module, type=owner or "_", method=method,
                             x86_addr="", src_file="", src_line="",
                             join_kind="歧义",
                             evidence="该名字在 x86 侧被拆成多个函数，候选："
                                      + "；".join(cands) + "。按模块一致性的推荐值见 docs §19.5",
                             _closure=is_closure, _size=size, _a=fa))
            continue
        if c:
            fb, n = c.most_common(1)[0]
            kind = "唯一锚"
            src = sorted(st["anch_a"].get(fa, ()), key=lambda t: (t[0], t[1], t[2]))
            own = [x for x in src if x[0].startswith("src/")]
            pick = own[0] if own else (src[0] if src else ("", "", ""))
            ev = f"{n} 个双向唯一 (file,line,col) 锚，首个 src/{pick[0].split('/')[-1]}:{pick[1]}:{pick[2]}"
            rows.append(dict(module=module, type=owner or "_", method=method,
                             x86_addr=hex(fb), src_file=pick[0], src_line=pick[1],
                             join_kind=kind, evidence=ev,
                             _closure=is_closure, _size=size, _a=fa))
            continue
        # 没有唯一锚：判断是「歧义」还是「未知」
        amb = [k for k in st["anch_a"].get(fa, ()) if k in st["owner_a"]]
        src_amb = [k for k in amb if k[0].startswith("src/")]
        if amb:
            why = (f"引用了 {len(amb)} 个 Location 键"
                   f"（src {len(src_amb)} 个），但没有一个是两侧一对一")
            # 尽量给出候选，方便后人接手；不选、不猜
            cand = sorted({hex(y) for k in amb for y in st["owner_b"].get(k, ())})
            if cand:
                why += f"；候选 x86（未定）: {', '.join(cand[:4])}"
        else:
            why = "该函数不引用任何 panic::Location（无锚，名字无法落地）"
        rows.append(dict(module=module, type=owner or "_", method=method,
                         x86_addr="", src_file="", src_line="",
                         join_kind="歧义" if amb else "未知", evidence=why,
                         _closure=is_closure, _size=size, _a=fa))
    # provenance：这一行的名字来自哪种 mangling 形态（不参与配对，只用于复盘）
    for r in rows:
        info = sym_info.get(r["_a"])
        r["sym_form"] = info[2] if info else "-"
    hit = apply_manual_fixes(rows)
    st["manual_fix_hits"] = hit
    return rows


if __name__ == "__main__":
    main()
