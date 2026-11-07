#!/usr/bin/env python3
"""把 **Windows 2.0.60 `graphwarrust.dll`（x86-64，带 COFF 符号表）** 的函数名
落到 **Linux 2.0.60 `libgraphwarrust.so`（x86-64，stripped）** 的地址上。

## 为什么这份参考比 Android 那份强

`docs/02` §20 用 Android arm64 的 `.symtab` 提供名字，两个弱点：

1. **跨架构**：名字→地址的桥只有 `panic::Location` 的 `(file,line,col)`，
   arm64 与 x86-64 的**内联决策不同**，同一个 Location 在两侧被引用它的函数集合不一样
   （§20 里 171 行「歧义」大部分就是这么来的）。
2. **名字少**：arm64 那份只有 1,300 个含 `graphwar` 的 `FUNC`，去重后 524 个唯一名。

Windows 这份是 **同版本（2.0.60）、同指令集（x86-64）、带符号**：

    .reference/steam/windows-2.0.60/graphwarrust.dll   sha256 ab490af6…
    1,082 个 `_ZN12graphwarrust…` 符号；哈希归一后 807 个唯一名

同指令集带来的直接收益：**内联形态接近**，同一个 Location 在 PE 与 ELF 两侧
「被哪些函数引用」的集合**不会因为 ISA 差异而分叉**，于是「两侧各只有一个函数引用」的
一对一锚（下文 `唯一锚`）比 §20 多得多。

## 桥仍然是 `(file,line,col)`，但 PE 上的形态不一样（这里是必须自己验的部分）

ELF 侧（`tools/loc_xrefs.py`）：Location 在 `.data.rel.ro`，`file_ptr` 是
`R_X86_64_RELATIVE`，**文件里存的是 addend（= 目标虚地址）**。

PE 侧：Rust 生成的是**基于 ImageBase 的绝对地址**，落盘就是
`ImageBase + RVA` 的 8 字节常量，改基址靠 `.reloc` 的 `IMAGE_REL_BASED_DIR64`
（本 DLL：38,355 条 DIR64，type 0 的 189 条是 ABSOLUTE 填充）。
⇒ **不能照搬 ELF 的「读 addend」**，要读这个 8 字节常量本身。

正因如此，`.reloc` 反过来是**最好用的索引**：每一个 Location 的首字段必然带一条 DIR64
重定位。本工具**优先**用 `.reloc` 枚举（穷尽、不会漏），`.reloc` 缺失时才退回
「扫 `.rdata`/`.data` 的 8 字节对齐槽 + 自校验」。

实测（本版）：盲扫只找到 1,046 个 `src/` 结构体，`.reloc` 枚举找到 **2,056** 个 ——
差一倍，原因是盲扫要求槽首 8 字节对齐且紧邻字面量，而 `.rdata` 里大量 Location
是 16 字节交错排布的。**所以 `.reloc` 是主路径，不是优化。**

## 布局（两侧同构，已双向验证）

    struct Location { file: &'static str, line: u32, col: u32 }   // 24 字节
    +0  file_ptr : *const u8     ← PE: 绝对地址，带 DIR64；ELF: RELATIVE addend
    +8  file_len : usize
    +16 line     : u32
    +20 col      : u32

自校验四连（缺一不可，否则普通 `(&str,u32,u32)` 会混进来）：
① `+0` 是一条 DIR64（或盲扫时落在 `.rdata`/`.data`）；
② `+8` 的 len 与 `+0` 处走到字面量末尾的字节数**完全相等**；
③ 该字面量可打印且以 `.rs` 结尾；
④ `line/col` 落在 `loc_xrefs.py` **同一套窗口**（`1<=line<100000`、`1<=col<4096`）——
   两侧必须同一套规则，否则 join 的一对一会被人为破坏（`name_bridge.py` 的
    `LocIndex` docstring 里那条老教训）。

## 函数边界从哪来（PE 侧不靠 Ghidra）

`.pdata`（x64 unwind 的 `RUNTIME_FUNCTION` 数组，12 字节/条：Begin/End/UnwindInfo）
给出**穷尽且精确**的函数区间 —— 比 ELF 侧靠 `.eh_frame` FDE 还干净。
本 DLL 48,853 条。符号表只提供**名字**，不提供边界
（`scl 3` 的静态符号没有 aux size 记录，别指望它）。

## 一条命令复现

    ../.venv/bin/python tools/win_names.py stats                  # 覆盖统计 + 自检
    ../.venv/bin/python tools/win_names.py locs                    # .reloc → Location 表
    ../.venv/bin/python tools/win_names.py symbols                 # 符号表 → 归一化名字
    ../.venv/bin/python tools/win_names.py xrefs                   # Location → 引用它的函数
    ../.venv/bin/python tools/win_names.py bridge                  # 落盘 fixtures/windows_name_to_addr.tsv
    ../.venv/bin/python tools/win_names.py conflicts               # 同 Location 两侧不同名

`--dll` / `GW2_WIN_DLL` 指定 DLL，缺省 `.reference/steam/windows-2.0.60/graphwarrust.dll`。
Linux 侧默认**就地**调用 `tools/loc_xrefs.py` 的 `LocXrefs`（约 40 s）；
用 `--linux-locs FILE` 复用 `loc_xrefs.py dump locs` 的旧输出可跳过这一步。

依赖：Python 3.9+ 标准库 + **capstone**。不需要 Ghidra、不需要 pkl、不联网。
全量扫描（21.7 MB `.text`，约 530 万条指令）约 60 s。
"""

import argparse
import bisect
import collections
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
sys.path.insert(0, HERE)

DEFAULT_DLL_REL = os.path.join(
    ".reference", "steam", "windows-2.0.60", "graphwarrust.dll")
DEFAULT_DLL = os.path.join(REPO, DEFAULT_DLL_REL)

# 匹配 crate 的**裸**名字。不要用 `_ZN12graphwarrust` 前缀：GDExtension 的 trait impl
# 名字形如 `_ZN133_$LT$graphwarrust..axis_drawer..AxisDrawer$u20$as$u20$godot_core..X$GT$…`，
# 里面 crate 名**没有长度前缀**。只用前缀过滤会静默丢掉 59 个函数（本版实测），
# 其中 25 个是本工具唯一能钉住的 —— 全被丢掉就等于收益凭空消失 25 个地址。
CRATE = "graphwarrust"

# 「属于游戏自己的函数」的判据 —— 两侧必须**完全相同**（见 android_symbol_names）。
#
#   A  `_ZN12graphwarrust…`                    crate 内的直接路径
#   B  `_ZN<len>_$LT$graphwarrust..Type…$GT$…` 显式 `impl` 在游戏类型上的 trait 方法
#                                              （GDExtension 的 `__register_methods` 等）
#
# **刻意排除** C 类：`core::ptr::drop_in_place<…graphwarrust::…>` 这类
# 「库函数恰好以游戏类型为泛型实参」的实例化。它们是编译器生成的胶水，不是原版源码里的
# 函数；`parse_name` 对它们只能给出 `(module, type, None)`，写进夹具就是虚报。
# 本版实测：C 类有 327 个名，加进来不会多钉住任何地址（A+B 与「包含 graphwarrust」
# 两种口径的 tier-1 结果同为 334 个函数 / 332 个地址），只会污染名字集合。
_IMPL_ON_GAME = re.compile(r"^_ZN\d+_\$LT\$graphwarrust\.\.")


def is_game_name(name):
    """该符号是不是「游戏自己的函数」（见上面 A/B 判据）。"""
    if name.startswith("_ZN12graphwarrust"):
        return True
    return bool(_IMPL_ON_GAME.match(name))

# Rust 的 legacy mangling 尾巴 `17h<16 hex>E`：同一逻辑函数在不同单态化下哈希不同。
# 归一化掉它，两侧（arm64 / x86-64 / PE / ELF）才能按「同一个名字」对齐。
_HASH = re.compile(r"17h[0-9a-f]{16}E$")

# loc_xrefs.py `_find_locations()` 的验收窗口，两侧必须一致（见模块 docstring ④）。
LINE_MAX, COL_MAX, LEN_MIN, LEN_MAX = 100000, 4096, 4, 300  # `4 < ln < 300`


def norm_name(name):
    """哈希归一：`…17h0123456789abcdefE` → `…E`。"""
    return _HASH.sub("E", name)


# --------------------------------------------------------------------------
# 最小 PE 解析（不引入 pefile：本仓库的依赖面要小）
# --------------------------------------------------------------------------
class PE:
    def __init__(self, path):
        with open(path, "rb") as fh:
            self.data = fh.read()
        d = self.data
        if d[:2] != b"MZ":
            raise SystemExit("不是 MZ：%s" % path)
        e_lfanew = struct.unpack_from("<I", d, 0x3C)[0]
        if d[e_lfanew:e_lfanew + 4] != b"PE\0\0":
            raise SystemExit("不是 PE 签名：%s" % path)
        coff = e_lfanew + 4
        (self.machine, self.nsec, _ts, self.ptrsym, self.nsym,
         self.opt_size, self.chars) = struct.unpack_from("<HHIIIHH", d, coff)
        opt = coff + 20
        magic = struct.unpack_from("<H", d, opt)[0]
        if magic == 0x20B:      # PE32+
            self.image_base = struct.unpack_from("<Q", d, opt + 24)[0]
        elif magic == 0x10B:    # PE32
            self.image_base = struct.unpack_from("<I", d, opt + 28)[0]
        else:
            raise SystemExit("未知 optional header magic：%#x" % magic)
        self.sections = []
        sec = opt + self.opt_size
        for i in range(self.nsec):
            o = sec + i * 40
            name = d[o:o + 8].rstrip(b"\0").decode("ascii", "replace")
            vsize, vaddr, rawsize, rawptr = struct.unpack_from("<IIII", d, o + 8)
            self.sections.append(dict(name=name, vsize=vsize, vaddr=vaddr,
                                      rawsize=rawsize, rawptr=rawptr, idx=i + 1))

    def sec(self, name):
        for s in self.sections:
            if s["name"] == name:
                return s
        return None

    def sec_by_idx(self, idx):
        for s in self.sections:
            if s["idx"] == idx:
                return s
        return None

    def sec_of_rva(self, rva):
        for s in self.sections:
            if s["vaddr"] <= rva < s["vaddr"] + s["vsize"]:
                return s
        return None

    def va2off(self, va):
        s = self.sec_of_rva(va - self.image_base)
        if s is None or not s["rawsize"]:
            return None
        return s["rawptr"] + (va - self.image_base - s["vaddr"])

    def off2va(self, off):
        for s in self.sections:
            if s["rawsize"] and s["rawptr"] <= off < s["rawptr"] + s["rawsize"]:
                return self.image_base + s["vaddr"] + (off - s["rawptr"])
        return None

    def u64(self, va):
        o = self.va2off(va)
        if o is None or o + 8 > len(self.data):
            return None
        return struct.unpack_from("<Q", self.data, o)[0]

    def read(self, va, n):
        o = self.va2off(va)
        if o is None:
            return None
        return self.data[o:o + n]

    def cstring(self, off):
        e = self.data.index(b"\0", off)
        return self.data[off:e].decode("utf-8", "replace")

    # -- COFF 符号表 ------------------------------------------------------
    def symbols(self):
        """逐条产出 `(name, value, secnum, type, storage_class)`。

        `value` 是**节内偏移**（COFF 语义），不是 RVA —— 这一点必须验而不是猜：
        本 DLL 第一个符号 `_ZN12graphwarrust11axis_drawer1_6__init…` 的 value 是
        `0x2b120`，`.text` 的 RVA 是 `0x1000`，两者相加 `0x2c120` 处正是
        `41 57 41 56 57 55 53 48 81 ec 98 00 00 00`（`push r15…sub rsp,0x98`）；
        而 `0x2b120` 处是 `48 85 d2 …`（一条 `test rdx,rdx`，不是函数头）。
        另有一个独立佐证：`value + 0x1000` 落在 `.pdata` 的 `BeginAddress` 上的有
        910/1018，而 `value` 本身只有 34/1018。
        """
        d, out = self.data, []
        strtab = self.ptrsym + self.nsym * 18
        i = 0
        while i < self.nsym:
            o = self.ptrsym + i * 18
            name8 = d[o:o + 8]
            if name8[:4] == b"\0\0\0\0":
                name = self.cstring(strtab + int.from_bytes(name8[4:], "little"))
            else:
                name = name8.split(b"\0", 1)[0].decode("utf-8", "replace")
            val = struct.unpack_from("<I", d, o + 8)[0]
            sec = struct.unpack_from("<h", d, o + 12)[0]
            typ = struct.unpack_from("<H", d, o + 14)[0]
            scl, naux = d[o + 16], d[o + 17]
            out.append((name, val, sec, typ, scl))
            i += 1 + naux
        return out

    # -- .pdata：x64 unwind RUNTIME_FUNCTION -------------------------------
    def pdata_funcs(self):
        """`[(begin_rva, end_rva)]`，按 begin 升序。函数边界的**权威**来源。"""
        s = self.sec(".pdata")
        if s is None:
            return []
        d = self.data
        out = []
        for k in range(s["vsize"] // 12):
            o = s["rawptr"] + k * 12
            begin, end, _u = struct.unpack_from("<III", d, o)
            if begin and end > begin:
                out.append((begin, end))
        out.sort()
        return out

    # -- .reloc -----------------------------------------------------------
    def relocs(self):
        """`[(type, va)]`。type 10 = IMAGE_REL_BASED_DIR64（PE32+ 的真重定位）。"""
        s = self.sec(".reloc")
        if s is None:
            return []
        d = self.data
        out = []
        off, end = s["rawptr"], s["rawptr"] + s["rawsize"]
        while off + 8 <= end and off + 8 <= len(d):
            page, size = struct.unpack_from("<II", d, off)
            if size < 8:
                break
            for k in range((size - 8) // 2):
                if off + 10 + k * 2 > len(d):
                    break
                e = struct.unpack_from("<H", d, off + 8 + k * 2)[0]
                out.append((e >> 12, self.image_base + page + (e & 0xFFF)))
            off += size
        return out


# --------------------------------------------------------------------------
# Location 提取
# --------------------------------------------------------------------------
def _rs_literal_at(pe, va, maxlen=400):
    """若 `va` 起是一段 `…/*.rs` 字面量，返回其精确文本（走到第一个非路径字符为止）。

    Rust 的 `file!()` 字面量**不 NUL 结尾**且相邻字面量紧挨着，所以长度只能靠
    「字符集 + 以 `.rs` 结尾」界定，再用结构体里的 `len` 交叉验证。
    """
    if va < pe.image_base:
        return None
    o = pe.va2off(va)
    if o is None:
        return None
    d = pe.data
    k = o
    limit = min(len(d), o + maxlen)
    while k < limit:
        c = d[k]
        if c < 0x20 or c > 0x7E:
            break
        k += 1
    raw = d[o:k]
    if not raw.endswith(b".rs"):
        return None
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        return None
    if " " in text or "/" not in text:
        return None
    return text


def _in_sections(pe, va, names):
    s = pe.sec_of_rva(va - pe.image_base)
    return s is not None and s["name"] in names


def find_locations(pe, verbose=True):
    """`{slot_va: (file, line, col)}`。

    主路径：`.reloc` 的 DIR64 枚举（穷尽）。退回路径：`.rdata`/`.data` 8 字节对齐盲扫。
    """
    data_names = {".rdata", ".data"}
    locs = {}

    def accept(slot, text, ln, line, col):
        if not (LEN_MIN < ln <= LEN_MAX):
            return
        if len(text) != ln or not text.endswith(".rs"):
            return
        if not (1 <= line < LINE_MAX and 1 <= col < COL_MAX):
            return
        locs[slot] = (text, line, col)

    relocs = pe.relocs()
    dir64 = [va for t, va in relocs if t == 10]
    n_from_reloc = 0
    for slot in dir64:
        if not _in_sections(pe, slot, data_names):
            continue
        tgt = pe.u64(slot)
        if tgt is None or not _in_sections(pe, tgt, data_names):
            continue
        text = _rs_literal_at(pe, tgt)
        if text is None:
            continue
        o = pe.va2off(slot)
        ln, line, col = struct.unpack_from("<QII", pe.data, o + 8)
        before = len(locs)
        accept(slot, text, ln, line, col)
        if len(locs) > before:
            n_from_reloc += 1

    n_from_scan = 0
    if not dir64:
        # 没有 .reloc（或者被剥掉）时只能盲扫
        for sname in (".rdata", ".data"):
            s = pe.sec(sname)
            if s is None:
                continue
            for rva in range(s["vaddr"], s["vaddr"] + s["vsize"] - 24, 8):
                slot = pe.image_base + rva
                o = s["rawptr"] + (rva - s["vaddr"])
                tgt, ln = struct.unpack_from("<QQ", pe.data, o)
                if not _in_sections(pe, tgt, data_names):
                    continue
                text = _rs_literal_at(pe, tgt)
                if text is None:
                    continue
                line, col = struct.unpack_from("<II", pe.data, o + 16)
                before = len(locs)
                accept(slot, text, ln, line, col)
                if len(locs) > before:
                    n_from_scan += 1

    if verbose:
        print("Location 结构体：%d（DIR64 路径 %d，盲扫路径 %d）"
              % (len(locs), n_from_reloc, n_from_scan))
        tri = set(locs.values())
        src = set(v for v in tri if v[0].startswith("src/"))
        print("  唯一三元组：%d（其中 src/ %d）" % (len(tri), len(src)))
    return locs


# --------------------------------------------------------------------------
# 名字 vs 边界
# --------------------------------------------------------------------------
def function_starts(pe, verbose=True):
    """`[func_va]` 升序 —— Windows 侧函数边界。

    ⚠️ **`.pdata` 不穷尽**：x64 的 leaf 函数（无栈分配、不保存非易失寄存器）可以没有
    `RUNTIME_FUNCTION` 条目。本 DLL 实测 `.pdata` 有 48,853 条，只覆盖 `.text` 的 96.5%；
    另有 **6,429** 个 `.text` 函数符号的地址不在任何 `.pdata` 起点上
    （其中 106 个是 `graphwarrust` 的具名函数，多数是 `{{closure}}`）。

    拿 `.pdata` 单独当边界，这 106 个函数引用的 Location 会被算到**前一个**函数头上，
    于是「Windows 侧唯一引用」的判定被污染。所以边界取
    **`.pdata` 起点 ∪ `.text` 函数符号（`ty == 0x20`）地址** 的并集。
    这是为了与 Linux 侧对齐：`loc_xrefs.py` 用 `.eh_frame` FDE（Rust 在 ELF 上给
    所有函数都发 FDE，含 leaf），本来就是穷尽的。
    """
    starts = set(pe.image_base + b for b, _e in pe.pdata_funcs())
    text = pe.sec(".text")
    n_sym = 0
    if text is not None:
        for name, val, sec, typ, scl in pe.symbols():
            if sec == 1 and typ == 0x20:
                starts.add(pe.image_base + text["vaddr"] + val)
                n_sym += 1
    if verbose:
        pdata = len(set(pe.image_base + b for b, _e in pe.pdata_funcs()))
        print("函数起点：.pdata %d + 函数符号 %d → 并集 %d（符号补进 %d 个）"
              % (pdata, n_sym, len(starts), len(starts) - pdata))
    return sorted(starts)


def named_functions(pe, verbose=True):
    """`{func_va: [raw_name, ...]}`——只收落在 `.text` 里的**游戏自有**函数符号。

    "游戏自有"的判据见 `is_game_name`。⚠️ **不要用 `_ZN12graphwarrust` 前缀过滤**：
    GDExtension 的 trait impl 名字形如
    `_ZN133_$LT$graphwarrust..axis_drawer..AxisDrawer$u20$as$u20$godot_core..X$GT$…`，
    其中 crate 名**没有长度前缀**。只按前缀过滤会静默丢掉 59 个函数（本版实测），
    其中 25 个是本工具**唯一**能钉住的地址 —— 等于收益凭空消失 25 个。


    ⚠️ COFF 符号的 `Value` 是**节内偏移**，不是 RVA。所以
    `func_va = ImageBase + 节.VirtualAddress + Value`。
    本 DLL 里 `.text` 的 `VirtualAddress = 0x1000`，漏掉它整体平移 4 KB —— 这个错
    不会报任何异常，只会让 `.pdata` 起点命中率从 910/1018 掉到 34/1018（本工具的
    `stats` 就是在盯这两个数，见 `cmd_stats` 的自检）。
    """
    text = pe.sec(".text")
    if text is None:
        return {}
    out = collections.defaultdict(list)
    for name, val, sec, typ, scl in pe.symbols():
        if sec != 1:
            continue
        if not is_game_name(name):
            continue
        if typ != 0x20:      # IMAGE_SYM_TYPE_FUNCTION
            continue
        out[pe.image_base + text["vaddr"] + val].append(name)
    if verbose:
        rn = [n for v in out.values() for n in v]
        direct = sum(1 for n in rn if n.startswith("_ZN12graphwarrust"))
        print("函数符号：%d 个名 / %d 个唯一地址（直接路径 %d、`<impl …>` 形态 %d）；"
              "哈希归一后 %d 个逻辑名"
              % (len(rn), len(out), direct, len(rn) - direct,
                 len(set(norm_name(n) for n in rn))))
    return dict(out)


# --------------------------------------------------------------------------
# 全 .text 反汇编：Location → 引用它的函数
# --------------------------------------------------------------------------
def loc_xrefs(pe, locs, starts, verbose=True):
    """`{slot_va: set(func_begin_va)}`。

    引用形态按 `loc_xrefs.py` 同一套口径：被引用的可以是结构体首址、`+8`（len）、
    `+16`（line/col）—— 三种都算「引用了该 Location」。
    """
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64, CS_OP_MEM, CS_OP_IMM
    from capstone.x86 import X86_REG_RIP

    index = {}
    for slot in locs:
        index[slot] = slot
        index[slot + 8] = slot
        index[slot + 16] = slot

    refs = collections.defaultdict(set)

    text = pe.sec(".text")
    if text is None:
        return refs
    base = pe.image_base + text["vaddr"]
    blob = pe.data[text["rawptr"]:text["rawptr"] + text["rawsize"]]

    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    n = 0
    for ins in md.disasm(blob, base):
        n += 1
        tgt = None
        for op in ins.operands:
            if op.type == CS_OP_MEM and op.mem.base == X86_REG_RIP:
                tgt = ins.address + ins.size + op.mem.disp
            elif op.type == CS_OP_IMM and op.imm in index:
                tgt = op.imm
            if tgt is not None and tgt in index:
                k = bisect.bisect_right(starts, ins.address) - 1
                if k >= 0:
                    refs[index[tgt]].add(starts[k])
                break
    if verbose:
        print("反汇编 %d 条指令；被引用的 Location %d / %d"
              % (n, len(refs), len(locs)))
    return refs


# --------------------------------------------------------------------------
# Linux 侧
# --------------------------------------------------------------------------
def linux_locs_from_tsv(path):
    """读 `loc_xrefs.py dump locs` 的输出 → `{triple: set(func_addr_int)}`。

    行格式：`<slot>\t<file>:<line>:<col>\t<f1,f2,…>`（无函数时第三列是 `-`）。
    """
    out = collections.defaultdict(set)
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            parts = line.rstrip("\n").split("\t")
            if len(parts) < 3:
                continue
            f, l, c = parts[1].rsplit(":", 2)
            tri = (f, int(l), int(c))
            if parts[2] != "-":
                for a in parts[2].split(","):
                    if a:
                        out[tri].add(int(a, 16))
            else:
                out.setdefault(tri, set())
    return out


def linux_locs_live(so_path, verbose=True):
    """就地跑 `loc_xrefs.LocXrefs`，省掉一次 TSV 中转。"""
    from loc_xrefs import LocXrefs
    ix = LocXrefs(so_path, verbose=verbose)
    out = collections.defaultdict(set)
    for slot, (f, l, c) in ix.locs.items():
        tri = (f, l, c)
        out[tri]  # 确保键存在
        for fn in ix.funcs_of_loc(slot):
            out[tri].add(fn)
    return dict(out)


# --------------------------------------------------------------------------
# join
# --------------------------------------------------------------------------
_LOGICAL_CACHE = {}


def logical_names(raw_names):
    """legacy mangled 名 → `[(module, type, method)]`。

    刻意复用 `name_bridge.py` 的 `demangle`/`parse_name`，而不是自己写一套：
    **两侧必须同一套规则**，否则「Windows 独有名字」会变成口径差异的产物
    （`name_bridge.parse_name` 的 `HASH_SUFFIX`、`CLOSURE`、`TRAILING_NOISE` 都在
    那里调过参，抄一份必然漂移）。
    """
    import name_bridge as nb
    need = [n for n in raw_names if n not in _LOGICAL_CACHE]
    if need:
        for raw, d in zip(need, nb.demangle(need)):
            p = nb.parse_name(d)
            _LOGICAL_CACHE[raw] = (p[0], p[1] or "_", p[2]) if p else None
    out = []
    for n in raw_names:
        v = _LOGICAL_CACHE.get(n)
        if v is not None:
            out.append(v)
    return out


def build_join(pe, locs, named, refs, linux, verbose=True):
    """Windows `name → Location → Linux 地址`。两级证据，逐条标在 `join_kind` 里。

    **`唯一锚`（硬配对）** —— 某个 `(file,line,col)` 三元组在 Windows 侧只被一个函数引用、
    在 Linux 侧也只被一个函数引用。判据与 `name_bridge.py` 同一套。这一级不看名字，
    所以它同时也是下面 `集合锚` 的**独立裁判**。

    **`集合锚`（集合互最佳）** —— 单个三元组经常被内联共享，但**一组**三元组的重叠可以
    唯一。判据要四条全中，缺一不取：

    1. Windows 函数 W 的三元组集合 `S(W)` 与 Linux 函数 L 的 `T(L)` 的重叠数
       `c(L)=|S(W)∩T(L)|` 在 Linux 侧**严格唯一最大**；
    2. 反向也成立 —— L 在 Windows 侧的最佳也是 W（互为最佳，剔掉「多对一」）；
    3. `c >= 2`（单个共享三元组不算证据，留给 `唯一锚`）；
    4. 与 `唯一锚` 冲突的一律降级为 `歧义`，**不静默取一个**。

    这一级与 `唯一锚` 的重叠部分是自检：本版 306 个共同配对里 **0 个不一致**。
    """
    # 名字：Windows 函数 VA → 归一化 raw 名（只收有符号的）
    fn_raw = {va: sorted(set(ns)) for va, ns in named.items()}

    # Windows 侧：函数 ↔ 三元组（只算 src/*.rs，依赖 crate 的 Location 不参与）
    fn_tris = collections.defaultdict(set)
    tri_fns = collections.defaultdict(set)
    for slot, fns in refs.items():
        tri = locs[slot]
        if not tri[0].startswith("src/"):
            continue
        for fva in fns:
            fn_tris[fva].add(tri)
            tri_fns[tri].add(fva)

    # Linux 侧：三元组 ↔ 函数、函数 ↔ 三元组
    l_tris = collections.defaultdict(set)
    lt_fns = collections.defaultdict(set)
    for tri, fns in linux.items():
        if not tri[0].startswith("src/"):
            continue
        for L in fns:
            l_tris[L].add(tri)
            lt_fns[tri].add(L)

    # ---- 第一级：唯一锚 ----
    tier1 = {}
    for tri, w in tri_fns.items():
        l = lt_fns.get(tri, set())
        if len(w) == 1 and len(l) == 1:
            fva, L = next(iter(w)), next(iter(l))
            tier1.setdefault(fva, collections.defaultdict(list))[L].append(tri)

    # ---- 第二级：集合互最佳 ----
    inv = collections.defaultdict(list)
    for L, ts in l_tris.items():
        for t in ts:
            inv[t].append(L)
    w2L = collections.defaultdict(collections.Counter)
    for fva, ts in fn_tris.items():
        if fva not in fn_raw:
            continue          # 无名函数只参与「唯一性」判定，不产生名字
        for t in ts:
            for L in inv.get(t, ()):
                w2L[fva][L] += 1
    L2w = collections.defaultdict(collections.Counter)
    for fva, c in w2L.items():
        for L, n in c.items():
            L2w[L][fva] += n

    tier2 = {}
    for fva, c in w2L.items():
        if not c:
            continue
        best = max(c.values())
        tops = [L for L, n in c.items() if n == best]
        if len(tops) != 1 or best < 2:
            continue
        L = tops[0]
        rc = L2w[L]
        rbest = max(rc.values())
        rtops = [g for g, n in rc.items() if n == rbest]
        if len(rtops) != 1 or rtops[0] != fva:
            continue
        tier2[fva] = (L, best)

    # ---- 合成：唯一锚优先；集合锚若与唯一锚冲突则降级 ----
    t1_addr = {}
    for fva, byL in tier1.items():
        if len(byL) == 1:
            t1_addr[fva] = next(iter(byL))

    rows = []
    for fva in sorted(set(fn_tris) & set(fn_raw)):
        raws = fn_raw[fva]
        names = logical_names(raws)
        base = dict(func_va=fva, raw_names=raws,
                    norm=norm_name(raws[0]), names=names,
                    win_tris=fn_tris[fva])
        if fva in t1_addr:
            L = t1_addr[fva]
            anchors = sorted(tier1[fva][L])
            rows.append(dict(base, kind="唯一锚", targets=[L], anchor=anchors[0],
                             score=(len(anchors), len(fn_tris[fva]))))
            continue
        if fva in tier1:
            rows.append(dict(base, kind="歧义", targets=sorted(tier1[fva]),
                             anchor=None, score=(0, len(fn_tris[fva]))))
            continue
        if fva in tier2:
            L, c = tier2[fva]
            shared = sorted(t for t in fn_tris[fva] if L in lt_fns.get(t, ()))
            rows.append(dict(base, kind="集合锚", targets=[L],
                             anchor=shared[0] if shared else None,
                             score=(c, len(fn_tris[fva]))))
            continue
        rows.append(dict(base, kind="无一对一锚", targets=[], anchor=None,
                         score=(0, len(fn_tris[fva]))))

    if verbose:
        k = collections.Counter(r["kind"] for r in rows)
        print("Windows 命名函数的 join 结果：%s" % dict(k))
        for kind in ("唯一锚", "集合锚"):
            a = {r["targets"][0] for r in rows if r["kind"] == kind}
            print("  %s 钉住的 Linux 地址：%d" % (kind, len(a)))
        a = {r["targets"][0] for r in rows if r["kind"] in ("唯一锚", "集合锚")}
        print("  合计不同的 Linux 地址：%d" % len(a))
        # 自检：集合锚 vs 唯一锚 的一致性
        agree = dis = 0
        for r in rows:
            if r["kind"] != "集合锚":
                continue
            if r["func_va"] in t1_addr:
                if t1_addr[r["func_va"]] == r["targets"][0]:
                    agree += 1
                else:
                    dis += 1
        print("  自检：集合锚与唯一锚重叠配对 %d 个，不一致 %d 个" % (agree, dis))
    return rows


def write_bridge_tsv(path, rows, header_notes):
    """`fixtures/windows_name_to_addr.tsv`：一行一个引用了 `src/*.rs` Location 的
    Windows 命名函数。`module/type/method` 与 `name_to_addr.tsv` 同一套口径，可直接对拍。"""
    with open(path, "w", encoding="utf-8") as fh:
        for note in header_notes:
            fh.write("# %s\n" % note)
        fh.write("func_va\tmodule\ttype\tmethod\tjoin_kind\tlinux_addr"
                 "\tmatch\tn_locs\tanchor\tnorm_name\traw_names\n")
        for r in rows:
            nm = r["names"][0] if r["names"] else ("_", "_", "_")
            addrs = ",".join("0x%x" % a for a in r["targets"])
            sc = "%d/%d" % r["score"]
            a = r["anchor"]
            anchor = "%s:%d:%d" % a if a else "-"
            fh.write("%s\t%s\t%s\t%s\t%s\t%s\t%s\t%d\t%s\t%s\t%s\n" % (
                "0x%x" % r["func_va"], nm[0], nm[1], nm[2], r["kind"],
                addrs or "-", sc, len(r["win_tris"]), anchor,
                r["norm"], ",".join(r["raw_names"])))


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------
def _default_dll():
    """向上找 `.reference/steam/windows-2.0.60/graphwarrust.dll`。

    本仓库在 `.agent-work/<topic>/repo` 这类克隆位置时 `..` 不等于工作区根，
    所以从 REPO 逐级向上探，找到第一个存在的。
    """
    cur = REPO
    for _ in range(8):
        cand = os.path.join(cur, DEFAULT_DLL_REL)
        if os.path.exists(cand):
            return cand
        parent = os.path.dirname(cur)
        if parent == cur:
            break
        cur = parent
    return DEFAULT_DLL


def _open(args):
    path = args.dll or os.environ.get("GW2_WIN_DLL") or _default_dll()
    if not os.path.exists(path):
        raise SystemExit("DLL 不存在：%s（用 --dll 或 GW2_WIN_DLL 指定）" % path)
    pe = PE(path)
    print("DLL：%s\n  ImageBase %#x，%d 节，%d 个 COFF 符号记录"
          % (path, pe.image_base, len(pe.sections), pe.nsym))
    return pe


def cmd_locs(pe, args):
    locs = find_locations(pe)
    src = sorted(v for v in locs.values() if v[0].startswith("src/"))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("slot\tfile\tline\tcol\n")
            for slot, (f, l, c) in sorted(locs.items()):
                fh.write("%#x\t%s\t%d\t%d\n" % (slot, f, l, c))
        print("→ %s" % args.out)
    else:
        for slot, (f, l, c) in sorted(locs.items())[:20]:
            print("%#x\t%s:%d:%d" % (slot, f, l, c))
    print("唯一 src/ 三元组：%d" % len(set(src)))
    return 0


def cmd_symbols(pe, args):
    named = named_functions(pe)
    norm = collections.Counter()
    for va, names in named.items():
        for n in names:
            norm[norm_name(n)] += 1
    print("逻辑名（哈希归一）：%d；出现多于一次：%d"
          % (len(norm), sum(1 for v in norm.values() if v > 1)))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("func_va\traw_name\tnorm_name\n")
            for va in sorted(named):
                for n in named[va]:
                    fh.write("%#x\t%s\t%s\n" % (va, n, norm_name(n)))
        print("→ %s" % args.out)
    else:
        for va in sorted(named)[:15]:
            print("%#x\t%s" % (va, named[va][0]))
    return 0


def cmd_funcs(pe, args):
    funcs = pe.pdata_funcs()
    print(".pdata 函数区间：%d" % len(funcs))
    text = pe.sec(".text")
    if text:
        covered = sum(min(e, text["vaddr"] + text["vsize"]) - max(b, text["vaddr"])
                      for b, e in funcs
                      if b < text["vaddr"] + text["vsize"] and e > text["vaddr"])
        print("  覆盖 .text %.2f%%" % (100.0 * covered / text["vsize"]))
    return 0


def _linux_map(pe, args):
    if args.linux_locs:
        print("Linux Location 表：%s" % args.linux_locs)
        return linux_locs_from_tsv(args.linux_locs)
    from loc_xrefs import DEFAULT_SO
    so = args.so or os.environ.get("GW2_TARGET_SO") or DEFAULT_SO
    print("Linux 目标：%s" % so)
    return linux_locs_live(so, verbose=True)


def cmd_xrefs(pe, args):
    locs, funcs, named, refs = _scan_cached(pe, args)
    tri_fns = collections.defaultdict(set)
    for slot, fns in refs.items():
        for f in fns:
            tri_fns[locs[slot]].add(f)
    one = sum(1 for v in tri_fns.values() if len(v) == 1)
    print("被引用的 src/ 三元组：%d；其中 Windows 侧唯一引用：%d"
          % (sum(1 for t in tri_fns if t[0].startswith("src/")),
             sum(1 for t, v in tri_fns.items() if t[0].startswith("src/") and len(v) == 1)))
    print("命名函数命中：%d" % len(named))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("slot\tfile\tline\tcol\tfuncs\n")
            for slot, fns in sorted(refs.items()):
                f, l, c = locs[slot]
                fh.write("%#x\t%s\t%d\t%d\t%s\n" % (
                    slot, f, l, c, ",".join("%#x" % f for f in sorted(fns))))
        print("→ %s" % args.out)
    del one
    return 0


def _scan_cached(pe, args, verbose=True):
    """`(locs, funcs, named, refs)`，可落盘复用。

    全量 `.text` 反汇编约 60 s，调试 join 规则时不该每次都付这个代价。
    缓存里存的是**已经解析好的事实**（不是中间态），所以复用不会改变结论；
    换了 DLL 会因 `--cache` 文件里记了 identity 而被拒绝。
    """
    import json
    path = args.cache
    # schema 版本进 identity：改了事实口径（例如函数边界从 .pdata 换成并集）之后
    # 旧缓存必须失效，否则会拿旧结论冒充新结论。
    ident = {"schema": 2, "image_base": pe.image_base, "nsym": pe.nsym,
             "text_size": pe.sec(".text")["vsize"]}
    if path and os.path.exists(path):
        with open(path, encoding="utf-8") as fh:
            blob = json.load(fh)
        if blob.get("ident") == ident:
            locs = {int(k): tuple(v) for k, v in blob["locs"].items()}
            funcs = [int(x) for x in blob["funcs"]]
            refs = {int(k): set(v) for k, v in blob["refs"].items()}
            # `named` 不进缓存：算它只要 0.4 s，而过滤口径一改（本工具改过一次：
            # `_ZN12graphwarrust` 前缀 → 裸 `graphwarrust`）旧缓存就会拿旧名字冒充新结论。
            named = named_functions(pe, verbose=False)
            if verbose:
                print("复用缓存：%s（Location %d，命名函数 %d，被引用 Location %d）"
                      % (path, len(locs), len(named), len(refs)))
            return locs, funcs, named, refs
        if verbose:
            print("缓存 identity 不匹配，重算：%s" % path)
    locs = find_locations(pe)
    funcs = function_starts(pe)
    named = named_functions(pe)
    refs = loc_xrefs(pe, locs, funcs)
    if path:
        with open(path, "w", encoding="utf-8") as fh:
            json.dump({"ident": ident,
                       "locs": {str(k): list(v) for k, v in locs.items()},
                       "funcs": list(funcs),
                       "refs": {str(k): sorted(v) for k, v in refs.items()}}, fh)
        if verbose:
            print("→ 缓存 %s" % path)
    return locs, funcs, named, refs


AN_DEFAULT = os.path.join("android-2.0.60", "libgraphwarrust.so")


def android_symbol_names(so_path, verbose=True):
    """Android 2.0.60 `.symtab` 里含 `graphwarrust` 的 FUNC 名（去重）。

    ⚠️ 过滤条件必须与 Windows 侧**完全相同**（见 `CRATE`），否则「Windows 独有名字」
    里会混进一堆两边都有的 `<impl …>` 名字，纯增量被高估。
    

    用 `readelf -sW`：Android 那份**没有 strip**，`.symtab` 是档案里最硬的一份名字来源。
    """
    import subprocess
    if not os.path.exists(so_path):
        if verbose:
            print("⚠️ Android 参考不存在，跳过名字集合比较：%s" % so_path)
        return set()
    out = subprocess.run(["readelf", "-sW", so_path], capture_output=True,
                         text=True, check=True).stdout
    names = set()
    for line in out.splitlines():
        f = line.split()
        if len(f) < 8 or f[3] != "FUNC" or f[6] == "UND":
            continue
        n = f[7]
        if is_game_name(n):
            names.add(n)
    if verbose:
        print("Android 名字：%d 个 raw" % len(names))
    return names


def android_addr_names(tsv_path):
    """`fixtures/name_to_addr.tsv` → `{linux_addr: {(module,type,method)}}`。"""
    out = collections.defaultdict(set)
    if not os.path.exists(tsv_path):
        return out
    with open(tsv_path, encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("#"):
                continue
            p = line.rstrip("\n").split("\t")
            if len(p) < 8 or p[0] == "module":
                continue
            if p[3].startswith("0x"):
                out[int(p[3], 16)].add((p[0], p[1], p[2]))
    return out


def _ll(rows):
    """rows → `{linux_addr: {logical names}}`。"""
    out = collections.defaultdict(set)
    for r in rows:
        if r["kind"] not in ("唯一锚", "集合锚"):
            continue
        for a in r["targets"]:
            out[a] |= set(r["names"])
    return out


_TRI_FNS_CACHE = {}


def _win_tri_fns(A):
    """`{三元组: set(Windows 函数 VA)}`（只算 src/*.rs）。"""
    if "v" not in _TRI_FNS_CACHE:
        out = collections.defaultdict(set)
        for slot, fns in A["refs"].items():
            tri = A["locs"][slot]
            if tri[0].startswith("src/"):
                out[tri] |= fns
        _TRI_FNS_CACHE["v"] = out
    return _TRI_FNS_CACHE["v"]


def _analysis(pe, args, verbose=True):
    locs, starts, named, refs = _scan_cached(pe, args, verbose=verbose)
    linux = _linux_map(pe, args)

    win_src = set(v for v in locs.values() if v[0].startswith("src/"))
    lin_src = set(t for t in linux if t[0].startswith("src/"))
    if verbose:
        print("\n① Location 三元组（src/*.rs）：Windows %d，Linux %d，交集 %d"
              % (len(win_src), len(lin_src), len(win_src & lin_src)))
        print("   仅 Windows %d，仅 Linux %d"
              % (len(win_src - lin_src), len(lin_src - win_src)))
        for t in sorted(lin_src - win_src)[:20]:
            print("     仅 Linux：%s:%d:%d" % t)

    rows = build_join(pe, locs, named, refs, linux, verbose=verbose)
    return dict(locs=locs, starts=starts, named=named, refs=refs,
                linux=linux, rows=rows, win_src=win_src, lin_src=lin_src)


def cmd_bridge(pe, args):
    A = _analysis(pe, args)
    out = args.out or os.path.join(REPO, "fixtures", "windows_name_to_addr.tsv")
    write_bridge_tsv(out, A["rows"], [
        "由 tools/win_names.py bridge 生成，不要手改。",
        "func_va 是 Windows 2.0.60 graphwarrust.dll 的函数地址（ImageBase 0x180000000，"
        "COFF Value 是节内偏移，已加 .text 的 VirtualAddress 0x1000）。",
        "linux_addr 是 Linux 2.0.60 libgraphwarrust.so 的 ELF 虚地址（Ghidra/REA = +0x100000）。",
        "join 键是 panic::Location 的 (file,line,col) 三元组。",
        "join_kind=唯一锚：该三元组两侧各只有一个函数引用（与 name_bridge.py 同一套判据）。",
        "join_kind=集合锚：三元组集合互为最佳且重叠>=2；与唯一锚重叠部分 0 个不一致。",
        "match = 决定这次配对的三元组数 / 该函数引用的 src/*.rs 三元组数；"
        "n_locs = 该函数引用的 src 三元组总数。",
        "anchor = 起决定作用的那个 (file,line,col)，可直接用 loc_xrefs.py loc 复查。",
        "module/type/method 由 name_bridge.demangle+parse_name 得出，与 name_to_addr.tsv 同口径。",
    ])
    print("→ %s（%d 行）" % (out, len(A["rows"])))
    return 0


def cmd_report(pe, args):
    """一次打出全部验收数字（文档里引用的每个数都能用这一条命令复跑）。"""
    A = _analysis(pe, args)
    rows = A["rows"]
    landed = _ll(rows)
    win_addr = set(landed)

    # Windows 名字集合
    raw_all = sorted({n for va, ns in A["named"].items() for n in ns})
    win_keys = set()
    for va, ns in A["named"].items():
        win_keys |= set(logical_names(ns))
    landed_keys = set()
    for a, ks in landed.items():
        landed_keys |= ks

    print("\n② Windows 名字 / 落地")
    print("   符号表：%d 个 raw 名 → %d 个逻辑名（module/type/method）"
          % (len(raw_all), len(win_keys)))
    print("   其中引用了 src/*.rs Location 的命名函数：%d 个（其余 %d 个无锚，任何符号表都救不了）"
          % (len(rows), len(A["named"]) - len(rows)))
    print("   落地逻辑名：%d（%.1f%%）；落地 Linux 地址：%d"
          % (len(landed_keys), 100.0 * len(landed_keys) / max(1, len(win_keys)),
             len(win_addr)))
    denom = {L for tri, fns in A["linux"].items()
             if tri[0].startswith("src/") for L in fns}
    print("   Linux 侧分母（引用了 src/*.rs Location 的函数）：%d → 覆盖 %.1f%%"
          % (len(denom), 100.0 * len(win_addr) / max(1, len(denom))))

    # 与 Android 名字比较
    so = args.android_so or os.path.join(os.path.dirname(os.path.dirname(
        os.path.abspath(args.dll or _default_dll()))), "android-2.0.60",
        "libgraphwarrust.so")
    an_raw = android_symbol_names(so)
    an_keys = set(logical_names(sorted(an_raw))) if an_raw else set()
    print("\n③ 与 Android 2.0.60 名字集合比较")
    if an_keys:
        only_win = win_keys - an_keys
        print("   Android 逻辑名 %d，Windows 逻辑名 %d，交集 %d"
              % (len(an_keys), len(win_keys), len(win_keys & an_keys)))
        print("   Windows 独有 %d 个，其中落地 %d 个"
              % (len(only_win), len(only_win & landed_keys)))
    else:
        only_win = set()

    # Android 已有锚 vs Windows 锚
    and_addr = android_addr_names(args.android_tsv or os.path.join(
        REPO, "fixtures", "name_to_addr.tsv"))
    if and_addr:
        oa = set(and_addr)
        print("\n④ 可定名的 Linux 地址")
        print("   现有（Android 锚）：%d" % len(oa))
        print("   本工具（Windows 锚）：%d" % len(win_addr))
        print("   交集 %d，并集 %d（+%d），Android 独有 %d"
              % (len(oa & win_addr), len(oa | win_addr),
                 len(oa | win_addr) - len(oa), len(oa - win_addr)))
        # 纯增量：Windows 独有名字且落地
        pure = {a for a, ks in landed.items() if ks and ks <= only_win}
        print("   Windows 独有名字单独钉住的地址：%d" % len(pure))

        print("\n⑤ 冲突（同一 Linux 地址，两侧名字不一致）—— 不静默取一个，逐条给证据")
        n_dis = 0
        for a in sorted(oa & win_addr):
            an, wn = and_addr[a], landed[a]
            if an == wn:
                continue
            n_dis += 1
            print("   %#x" % a)
            print("     Android 名：%s" % sorted("/".join(x) for x in an))
            print("     Windows 名：%s" % sorted("/".join(x) for x in wn))
            # 证据：谁的三元组集合把 Linux 侧这个函数覆盖得更全
            l_tri = {t for t, fns in A["linux"].items()
                     if t[0].startswith("src/") and a in fns}
            owner = collections.defaultdict(set)
            for tri, fns in _win_tri_fns(A).items():
                if tri in l_tri and len(fns) == 1:
                    owner[next(iter(fns))] |= {tri}
            print("     Linux 侧该函数引用 %d 个 src 三元组；Windows 侧各名字覆盖："
                  % len(l_tri))
            for f, ts in sorted(owner.items(), key=lambda kv: -len(kv[1])):
                cover = len(ts)
                flag = "  ← 完全覆盖（集合相等）" if cover == len(l_tri) else ""
                names = A["named"].get(f)
                label = ("/".join(logical_names(names)[0])
                         if names and logical_names(names) else "<无名>")
                print("       %-56s %2d/%d%s" % (label, cover, len(l_tri), flag))
        print("   一致 %d / 不一致 %d" % (len(oa & win_addr) - n_dis, n_dis))
    return 0


def cmd_conflicts(pe, args):
    """Windows 侧内部：同一 Location 被多个**不同名**的函数引用。

    只看 Windows 自己，所以**不需要 Linux 侧**（不走 40 s 的 `loc_xrefs` 扫描）。
    """
    locs, _starts, named, refs = _scan_cached(pe, args)
    A = dict(locs=locs, named=named, refs=refs)
    tri_fns = collections.defaultdict(set)
    for slot, fns in A["refs"].items():
        tri = A["locs"][slot]
        if tri[0].startswith("src/"):
            for f in fns:
                tri_fns[tri].add(f)
    n = 0
    for tri, fns in sorted(tri_fns.items()):
        names = set()
        for f in fns:
            if f in A["named"]:
                names |= set(logical_names(A["named"][f]))
        if len(names) > 1:
            n += 1
            if n <= args.limit:
                print("%s:%d:%d  被 %d 个函数引用，名字 %d 种："
                      % (tri + (len(fns), len(names))))
                for nm in sorted(names):
                    print("    %s" % "/".join(nm))
    print("Windows 侧「同一 src Location 多个不同名」的三元组：%d" % n)
    return 0


def cmd_stats(pe, args):
    locs = find_locations(pe)
    named = named_functions(pe)
    starts = set(function_starts(pe))
    text = pe.sec(".text")
    print("函数起点：%d（.text %#x 字节）" % (len(starts), text["vsize"]))
    hit = sum(1 for va in named if va in starts)
    print("命名函数落在函数起点上：%d / %d" % (hit, len(named)))
    tri = set(locs.values())
    print("Location 结构体 %d；唯一三元组 %d；src/ %d"
          % (len(locs), len(tri), sum(1 for t in tri if t[0].startswith("src/"))))
    return 0


def main(argv):
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", choices=["stats", "locs", "symbols", "funcs",
                                    "xrefs", "bridge", "conflicts", "report"])
    ap.add_argument("--dll", default=None)
    ap.add_argument("--so", default=None, help="Linux 目标 .so")
    ap.add_argument("--linux-locs", default=None,
                    help="复用 loc_xrefs.py dump locs 的 TSV，跳过现场扫描")
    ap.add_argument("--out", default=None)
    ap.add_argument("--android-so", default=None, help="Android 2.0.60 .so（名字集合比较用）")
    ap.add_argument("--android-tsv", default=None,
                    help="现有 fixtures/name_to_addr.tsv（Android 已有锚）")
    ap.add_argument("--limit", type=int, default=20)
    ap.add_argument("--cache", default=None,
                    help="把全量扫描结果落盘复用（换 DLL 会被 identity 拒绝）")
    o = ap.parse_args(argv[1:])
    pe = _open(o)
    return {"stats": cmd_stats, "locs": cmd_locs, "symbols": cmd_symbols,
            "funcs": cmd_funcs, "xrefs": cmd_xrefs, "bridge": cmd_bridge,
            "conflicts": cmd_conflicts, "report": cmd_report}[o.cmd](pe, o)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
