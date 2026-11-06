#!/usr/bin/env python3
"""`panic::Location` → **引用它的代码地址** → **这些代码属于哪个函数**：只靠这一份 `.so`。

## 这个工具替代了什么，为什么必须替代

旧链路（`tools/class_probe.py` 2026-11-07 及以前）是三级上游 pkl：

    ①  `.data.rel.ro` 里 24 字节的 Location 结构体        ← `analysis/relocs.pkl`
    ②  「谁引用了这个结构体」                             ← `analysis/xrefs.pkl`
    ③  引用指令落在哪个函数                               ← `analysis/funcstarts_ghidra_va.pkl`

第 ② 跳曾被判为「pkl 与目标 `.so` 不是同一次扫描」。**2026-11-07 复查证明这个判断是错的**：
`analysis/xrefs.pkl` 与 `.reference/steam/2026-08-28/libgraphwarrust.so`
（sha256 `9a538115…`，与本工具 `stats` 打出的一致）**逐条相符** ——
`xrefs.pkl[0x95e4f8] == [(0x4b6dc5, 'rip', 'lea', 'rdi, [rip + 0x4a772c]')]`，
而 `0x4b6dc5` 正是 `src/graphwar_plane.rs:274:129` 在整份 `.text` 里**唯一**的引用点。

真正的错误在 `class_probe.py` 自己身上：它把 pkl 里**已经是虚地址**的引用点又过了一遍
`off2va()`（当文件偏移），于是每条引用被平移 `.text` 的 VA↔文件偏移差 `+0x1000`，
归属整体向右漂一个函数（`0x4b6dc5 → 0x4b7dc5 → 0x4b7a70`）。见 `docs/16-client.md` §27。

所以本工具的意义不是「换一份更新的 pkl」，而是**让这一跳可以被独立复算**：

* 只读目标 `.so`（ELF 头 + `.rela.dyn` + `.eh_frame`），**不读任何 `analysis/*.pkl`**；
* 函数边界也从 `.so` 自己解（`.eh_frame` FDE，覆盖 `.text` 的 98.35%），不需要 Ghidra；
* 既能修 `class_probe.py` 的地址空间错误，也能当**独立裁判**复核任何旧结论。

## 覆盖判据（本工具的规格，不是实现细节）

一个 Location（`{file: &'static str, line: u32, col: u32}`，24 字节）被引用时，被引用的
可能是结构体首址、`+8`（`len`）或 `+16`（`line/col`）——**三种都算「引用了该 Location」**。
命中按下表分四类形态，`stats` 逐形态报数，可复核：

| 代号 | 形态 | 判据 |
|---|---|---|
| **R** | rip-relative 直引 | 任意指令的内存操作数 `[rip+disp]`，`insn.end+disp == loc{,+8,+16}` |
| **I** | 立即数直引 | 任意指令的立即数操作数 `== loc{,+8,+16}`（`mov r64, imm64` 等） |
| **G** | 寄存器携带 | 寄存器被 `lea r,[rip+disp]` / `mov r,[rip+slot]` / `mov r,imm` 赋成该地址，随后用作 `call *r` / `jmp *r` / 内存基址 + 位移。**这就是「只匹配 `call *(slot)` 会漏」的那个形态** |
| **D** | 数据槽携带 | 某个**被重定位的数据槽**的值（`R_X86_64_RELATIVE` 的 addend）本身指向该 Location，代码再经该槽间接取到 |

本版 `.so` 实测：`R` 覆盖绝大多数，`G` 额外找到 **4 条**（`R` 看不到），`I`/`D` 为 0。
`G`/`D` 是**实现了才敢报 0**：`G` 靠寄存器污点传播（`_scan` 里的 `val` 表，遇分支/调用/写回即清），
`D` 靠「全表扫 addend 是否落在 Location 集合里」。**这两个形态对普通函数指针槽
（gdext 方法表 / GOT）不是 0**，`refs <addr>` 子命令就是用它们查「谁调用了这个槽」的。

`.text` 之外的执行段（`__lcxx_override`、`.plt`）也扫；`.plt` 的 `jmp *[rip+got]` 就是 `R` 形态。

引用**目标**只记落在**可寻址数据节**的地址（`.rodata`/`.data.rel.ro`/`.data`/`.got`/`.bss` …），
另有 `calls[目标] = [调用点]` 记代码间直接调用；两者合起来覆盖 `class_probe` 原有的全部查询。

## 函数边界（不依赖 Ghidra）

1. `.eh_frame` 的 FDE `initial_location + range`（本版 13430 条，`.text` 内 13426 条，
   覆盖 `.text` 的 **98.35%**；13426 个起点**全部**通过「线性反汇编的指令边界」校验）。
2. `.eh_frame` 覆盖不到的零散函数（无 unwind 信息）：补 `call/jmp rel32` 的**段内目标**，
   且该目标不落在任何 FDE 区间内（本版补 64 个：`.text` 内 6 个 + `.text` 尾 2 个 + `.plt` 56 个桩）。
3. 每个执行段的首地址。

`func_of(va)` = 最近的前驱起点；`in_gap(va)` 判「既不在任何 FDE 区间、也不是补出来的函数头」。
本版 5301 个 Location 的**全部引用点无一落在空隙**，所以下面的归属没有「最近前驱兜底」的成分。

## 一条命令复现

    ../.venv/bin/python tools/loc_xrefs.py stats                 # 覆盖统计 + 自检
    ../.venv/bin/python tools/loc_xrefs.py check-tsv             # 327 条唯一锚交叉验证
    ../.venv/bin/python tools/loc_xrefs.py loc src/graphwar_plane.rs:274:129
    ../.venv/bin/python tools/loc_xrefs.py slot 0x95e4f8
    ../.venv/bin/python tools/loc_xrefs.py func 0x4b5df0
    ../.venv/bin/python tools/loc_xrefs.py dump refs             # 全量 xref（TSV）

目标 `.so` 用 `--so` 或 `GW2_TARGET_SO` 指定；缺省是
`/home/user/MyProject/gw2/.reference/steam/2026-08-28/libgraphwarrust.so`。
依赖：Python 3.9+ 标准库 + **capstone**。不需要 Ghidra、不需要上游 pkl。
全量扫描约 40 s（1.52 M 条指令），同进程内复用同一个 `LocXrefs` 实例即可。

## 给 `tools/class_probe.py` 用的 API

    ix = LocXrefs()                        # 一次扫描，全部缓存
    ix.locs                # {struct_va: (file, line, col)}
    ix.string_slots        # {slot_va: (addend, len, text)}  .data.rel.ro 的 (&str,len) 槽
    ix.refs_to(addr)       # [(insn_va, form, mnemonic, op_str)]  谁引用了 addr
    ix.refs_from(insn)     # [(target_va, form, mnemonic, op_str)] 该指令引用了谁
    ix.funcs_of_loc(slot)  # [func_start...]  该 Location 被哪些函数引用
    ix.func_hits_of_loc(s) # {func_start: [(insn, form, mn, op, +off)]}  证据链
    ix.locs_of_func(f)     # {(file,line,col): [(insn, ...)]}
    ix.func_of(va) / ix.func_end(f) / ix.in_gap(va) / ix.func_source(f)
    ix.calls               # {target_va: [call_site_va...]}
    ix.read(va, n)         # 按虚地址读字节
"""

import argparse
import bisect
import collections
import os
import re
import struct
import sys

DEFAULT_SO = os.environ.get(
    "GW2_TARGET_SO",
    "/home/user/MyProject/gw2/.reference/steam/2026-08-28/libgraphwarrust.so")

# 已知良好目标的指纹：换 `.so` 时 `stats` 会明着提醒（不阻断，因为要能扫其它构建）。
KNOWN_SHA256 = "9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187"

R_X86_64_RELATIVE = 8
SHT_NOBITS = 8
SHF_ALLOC = 0x2
SHF_EXECINSTR = 0x4
SHF_WRITE = 0x1


# ---------------------------------------------------------------------------
# 1. 极简 ELF64 读取（只取本工具要的东西，不引第三方）
# ---------------------------------------------------------------------------
class Elf:
    """ELF64 小端：节头 + 按虚地址读字节 + `.rela.dyn`。

    刻意不做完整 ELF 支持：只要节名、`R_X86_64_RELATIVE` 项、以及虚地址↔文件偏移换算。
    目标是一份 amd64 共享库；不是就明确报错，不猜。
    """

    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            self.data = f.read()
        d = self.data
        if d[:4] != b"\x7fELF" or d[4] != 2:
            raise SystemExit("%s 不是 ELF64：magic=%r class=%d" % (path, d[:4], d[4]))
        self.em = struct.unpack_from("<H", d, 18)[0]
        if self.em != 62:
            raise SystemExit("%s 不是 x86-64（e_machine=%d）" % (path, self.em))
        shoff = struct.unpack_from("<Q", d, 0x28)[0]
        shentsize, shnum, shstrndx = struct.unpack_from("<HHH", d, 0x3A)
        secs = []
        for i in range(shnum):
            o = shoff + i * shentsize
            (nameoff, typ, flags, addr, off, size,
             link, info, align, entsize) = struct.unpack_from("<IIQQQQIIQQ", d, o)
            secs.append(dict(i=i, nameoff=nameoff, type=typ, flags=flags, addr=addr,
                             offset=off, size=size, link=link, info=info,
                             align=align, entsize=entsize))
        stro = secs[shstrndx]
        st = d[stro["offset"]:stro["offset"] + stro["size"]]
        for s in secs:
            e = st.find(b"\0", s["nameoff"])
            s["name"] = st[s["nameoff"]:e].decode("latin1")
        self.secs = secs
        self.by = {s["name"]: s for s in secs}
        # 有文件内容的可寻址节，用于虚地址 ↔ 文件偏移
        self.mapped = [s for s in secs
                       if s["type"] != SHT_NOBITS and s["flags"] & SHF_ALLOC and s["size"]]

    def sec(self, name):
        s = self.by.get(name)
        if s is None:
            raise SystemExit("%s 缺节 %s（本工具只认 amd64 共享库）" % (self.path, name))
        return s

    def va2off(self, va):
        for s in self.mapped:
            if s["addr"] <= va < s["addr"] + s["size"]:
                return s["offset"] + (va - s["addr"])
        return None

    def off2va(self, off):
        for s in self.mapped:
            if s["offset"] <= off < s["offset"] + s["size"]:
                return s["addr"] + (off - s["offset"])
        return None

    def read(self, va, n):
        o = self.va2off(va)
        if o is None or o < 0 or o + n > len(self.data):
            return None
        return self.data[o:o + n]

    def u64(self, va):
        b = self.read(va, 8)
        return None if b is None else struct.unpack("<Q", b)[0]

    def u32(self, va):
        b = self.read(va, 4)
        return None if b is None else struct.unpack("<I", b)[0]

    def exec_sections(self):
        return [s for s in self.secs
                if s["type"] != SHT_NOBITS and s["flags"] & SHF_EXECINSTR and s["size"]]

    def data_sections(self):
        """可寻址、无执行权限的节（引用目标只记这些，否则每个小立即数都会成键）。"""
        return [s for s in self.secs
                if s["type"] != SHT_NOBITS and s["flags"] & SHF_ALLOC
                and not s["flags"] & SHF_EXECINSTR and s["size"]]

    def relative_relocs(self):
        """`{slot_va: addend}`：只收 `R_X86_64_RELATIVE`（写入值 = addend = 目标虚地址）。"""
        s = self.sec(".rela.dyn")
        out = {}
        for i in range(s["size"] // 24):
            off, info, add = struct.unpack_from("<QQq", self.data, s["offset"] + i * 24)
            if (info & 0xFFFFFFFF) == R_X86_64_RELATIVE:
                out[off] = add
        return out


# ---------------------------------------------------------------------------
# 2. `.eh_frame`：函数边界（起点 + 长度），纯解析
# ---------------------------------------------------------------------------
# DW_EH_PE 低 4 位 → (struct 格式, 字节数)
_PE = {0x00: ("<Q", 8), 0x01: ("<H", 2), 0x02: ("<I", 4), 0x03: ("<I", 4),
       0x04: ("<Q", 8), 0x09: ("<b", 1), 0x0A: ("<h", 2), 0x0B: ("<i", 4),
       0x0C: ("<i", 4), 0x0D: ("<q", 8), 0x0F: ("<Q", 8)}


def _pe_decode(buf, p, enc, field_va, base_va):
    """按 DW_EH_PE 解一个指针；`field_va` 是该字段自身的虚地址（pcrel 用）。"""
    if (enc & 0x0F) not in _PE:
        raise ValueError("unsupported DW_EH_PE 0x%02x" % enc)
    fmt, size = _PE[enc & 0x0F]
    v = struct.unpack_from(fmt, buf, p)[0]
    app = enc & 0x70
    if app == 0x10:      # pcrel
        v += field_va
    elif app == 0x30:    # datarel
        v += base_va
    return v, p + size


def _uleb(buf, p):
    r = s = 0
    while True:
        b = buf[p]
        p += 1
        r |= (b & 0x7F) << s
        s += 7
        if not b & 0x80:
            return r, p


def parse_eh_frame(data, base_va):
    """返回 `[(start_va, length)]`（FDE 表），按起点排序。

    CIE 只用来取 FDE 指针编码（`zR` 的 R 字节，或非 z 形式的 `R`）；augmentation 里
    `P`/`L`/`S` 按各自编码长度跳过 —— 跳错会让后面整体错位，所以长度表写显式的
    （本项目旧 `analysis/funcstarts.pkl` 就是在这类细节上错位 12 字节而全表报废）。
    """
    p, cies, out = 0, {}, []
    n = len(data)
    while p + 4 <= n:
        rec = p
        ln = struct.unpack_from("<I", data, p)[0]
        p += 4
        if ln == 0:
            break                                   # 终止记录
        hdr = 4
        if ln == 0xFFFFFFFF:                        # 64 位长度形式
            ln = struct.unpack_from("<Q", data, p)[0]
            p += 8
            hdr = 12
        end = p + ln
        if end > n:
            break
        cie_id = struct.unpack_from("<I", data, p)[0]
        p += 4
        if cie_id == 0:                             # CIE
            p += 1                                  # version
            q = data.find(b"\0", p)
            aug = data[p:q].decode("latin1")
            p = q + 1
            if "eh" in aug:
                p += 8
            _code_align, p = _uleb(data, p)
            p += 1                                  # data_align（sleb）
            _ra, p = _uleb(data, p)
            fde_enc = None
            if aug.startswith("z"):
                alen, p = _uleb(data, p)
                if alen:
                    a, p = data[p:p + alen], p + alen
                    q = 0
                    for c in aug[1:]:               # 'z' 之后才对应长度前缀里的字节
                        if c == "R":
                            fde_enc = a[q]
                            q += 1
                        elif c == "L":
                            q += 1
                        elif c == "P":
                            q += 1 + _PE[a[q] & 0x0F][1]
                        elif c == "S":
                            pass
            elif "R" in aug:
                fde_enc = data[p]
                p += 1
            cies[rec] = fde_enc
            p = end
            continue
        cie_off = rec + hdr - cie_id
        if cie_off not in cies or cies[cie_off] is None:
            p = end
            continue
        enc = cies[cie_off]
        start, p = _pe_decode(data, p, enc, base_va + p, base_va)
        rng = struct.unpack_from("<I", data, p)[0]
        p += 4
        out.append((start, rng))
        p = end
    out.sort()
    return out


def parse_eh_frame_hdr(elf):
    """`.eh_frame_hdr` 查找表 → `[start_va]`（`.eh_frame` 解析失败时的备用来源）。"""
    s = elf.sec(".eh_frame_hdr")
    d = elf.data[s["offset"]:s["offset"] + s["size"]]
    base = s["addr"]
    if d[0] != 1:
        raise SystemExit(".eh_frame_hdr 版本 %d，只认 1" % d[0])
    p = 4
    _ehp, p = _pe_decode(d, p, d[1], base + p, base)
    cnt = struct.unpack_from("<I", d, p)[0]
    p += 4
    enc = d[3]
    starts = []
    for _ in range(cnt):
        v, p = _pe_decode(d, p, enc, base + p, base)
        _fde, p = _pe_decode(d, p, enc, base + p, base)
        starts.append(v)
    return starts


# ---------------------------------------------------------------------------
# 3. 引用扫描
# ---------------------------------------------------------------------------
_HEX_TAIL = re.compile(r"0x([0-9a-f]+)\s*$")


def _writes_first(mnem, ops):
    """第一个操作数是不是「写」目标（决定要不要作废这个寄存器的污点）。"""
    if mnem in ("cmp", "test", "nop", "push", "jmp", "call", "bt", "ucomisd",
                "comisd", "ucomiss", "comiss"):
        return False
    return bool(ops)


class LocXrefs:
    """一次扫描，得到：Location 表、代码→数据引用、函数边界、直接调用表。"""

    def __init__(self, so=None, verbose=True):
        try:
            import capstone
            from capstone import x86 as cx
        except ImportError:
            raise SystemExit(
                "缺 capstone。本工具用 capstone 反汇编，仓库约定：\n"
                "    ../.venv/bin/python tools/loc_xrefs.py ...\n"
                "（系统 python3 没装 capstone）")
        self.cs = capstone
        self.cx = cx
        self.elf = Elf(so or DEFAULT_SO)
        self.data = self.elf.data
        self.relocs = self.elf.relative_relocs()
        self._find_locations()
        self._find_string_slots()
        self.exec_ranges = [(s["addr"], s["addr"] + s["size"])
                            for s in self.elf.exec_sections()]
        self.data_ranges = [(s["addr"], s["addr"] + s["size"], s["name"])
                            for s in self.elf.data_sections()]
        self._scan(verbose=verbose)
        self._build_functions()

    # -- Location ------------------------------------------------------
    @staticmethod
    def _printable(b):
        return all(32 <= c < 127 or c in (9, 10) for c in b)

    def _find_locations(self):
        """`.data.rel.ro` 的 24 字节 `{ptr(R_RELATIVE), len, line, col}` → Location。

        自校验四连（缺一不可，否则普通 `(&str, u32, u32)` 结构体会混进来）：
        ① 槽首有 `R_X86_64_RELATIVE`，addend 落在 `.rodata` 里；
        ② `+8` 的 `len` 与 addend 处读出的字节数一致（这一条同时躲开
           「`.rodata` 字面量不 NUL 结尾、strings 会把相邻字面量粘成一条」的坑）；
        ③ 字节全可打印且以 `.rs` 结尾；
        ④ `line/col` 在合理区间（`col < 4096`；`name_bridge.py` 用 2000，
           会掉掉 godot-core `central.rs:91:3013` 这一个真 Location —— 见 §27 附注）。
        """
        dr = self.elf.sec(".data.rel.ro")
        ro = self.elf.sec(".rodata")
        lo, hi = dr["addr"], dr["addr"] + dr["size"]
        rlo, rhi = ro["addr"], ro["addr"] + ro["size"]
        self.locs = {}
        for slot, addend in self.relocs.items():
            if not (lo <= slot < hi):
                continue
            ln = self.elf.u64(slot + 8)
            if ln is None or not (4 < ln < 300):
                continue
            if not (rlo <= addend < rhi) or addend + ln > rhi:
                continue
            raw = self.elf.read(addend, ln)
            if raw is None or len(raw) != ln or not self._printable(raw):
                continue
            try:
                text = raw.decode("ascii")
            except UnicodeDecodeError:
                continue
            if not text.endswith(".rs"):
                continue
            line, col = self.elf.u32(slot + 16), self.elf.u32(slot + 20)
            if line is None or col is None:
                continue
            if not (1 <= line < 100000 and 1 <= col < 4096):
                continue
            self.locs[slot] = (text, line, col)
        # 引用判定用：首址 / +8 / +16 三个阶段都算「引用了该 Location」
        self.loc_hit = {}
        for slot in self.locs:
            self.loc_hit[slot] = slot
            self.loc_hit[slot + 8] = slot
            self.loc_hit[slot + 16] = slot
        self.by_file_line = collections.defaultdict(list)
        for slot, (f, l, c) in self.locs.items():
            self.by_file_line[(f, l)].append((slot, c))

    def _find_string_slots(self):
        """`.data.rel.ro` 里所有 `(&str, len)` 形态的槽 → 供 `class_probe` 的线索 A 用。"""
        self.string_slots = {}
        for slot, addend in self.relocs.items():
            ln = self.elf.u64(slot + 8)
            if ln is None or not (1 <= ln <= 4096):
                continue
            raw = self.elf.read(addend, ln)
            if raw is None or len(raw) != ln or not self._printable(raw):
                continue
            try:
                text = raw.decode("utf-8")
            except UnicodeDecodeError:
                continue
            self.string_slots[slot] = (addend, ln, text)

    # -- 扫描 ----------------------------------------------------------
    def _scan(self, verbose=True):
        """一遍线性反汇编，同时收集 R/I/G/D 四种形态、直接调用、分支目标、指令边界。"""
        cx = self.cx
        md = self.cs.Cs(self.cs.CS_ARCH_X86, self.cs.CS_MODE_64)
        md.detail = True
        md.syntax = self.cs.CS_OPT_SYNTAX_INTEL
        self.refs = collections.defaultdict(list)       # target -> [(insn, form, mn, op)]
        self.refs_from = collections.defaultdict(list)  # insn -> [(target, form, mn, op)]
        self.calls = collections.defaultdict(list)      # target -> [insn]
        self.insn_at = set()
        self.branch_targets = set()
        self._insns = {}                                # section name -> (blob, base)
        # 形态 D 的载体：被重定位的数据槽，且其值指向某个 Location（本版应为 0 个）
        self.loc_ptr_slots = {slot: add for slot, add in self.relocs.items()
                              if add in self.locs}
        nops = 0
        for s in self.elf.exec_sections():
            blob = self.data[s["offset"]:s["offset"] + s["size"]]
            self._insns[s["name"]] = (blob, s["addr"])
            val = {}                                    # reg -> 已知绝对地址（污点）
            for ins in md.disasm(blob, s["addr"]):
                self.insn_at.add(ins.address)
                end = ins.address + ins.size
                mnem, ops = ins.mnemonic, ins.operands

                # --- 读操作数：R / I / G ---
                # `nop dword ptr [rax + rax]` 这类多字节填充也会带内存操作数，且寄存器可能
                # 恰好被污点命中 ⇒ 必须排除，否则会凭空造出引用（本工具第一版就这样多报了 2 条）。
                for op in (() if mnem == "nop" else ops):
                    if op.type == cx.X86_OP_MEM:
                        if op.mem.base == cx.X86_REG_RIP:
                            self._record(end + op.mem.disp, ins, "R")
                        elif op.mem.base in val:
                            self._record(val[op.mem.base] + op.mem.disp, ins, "G")
                        elif op.mem.index in val:
                            self._record(val[op.mem.index] * max(op.mem.scale, 1)
                                         + op.mem.disp, ins, "G")
                    elif op.type == cx.X86_OP_IMM and op.imm >= 0x1000:
                        self._record(op.imm, ins, "I")

                # --- 间接 call/jmp：`call *%reg`（形态 G 的落点）---
                if mnem in ("call", "jmp") and ops:
                    o0 = ops[0]
                    if o0.type == cx.X86_OP_REG and o0.reg in val:
                        self._record(val[o0.reg], ins, "G")
                    elif o0.type == cx.X86_OP_IMM:
                        if self._in_exec(o0.imm):
                            self.calls[o0.imm].append(ins.address)
                            self.branch_targets.add(o0.imm)

                # --- 写操作数：更新/作废污点；`mov r,[rip+loc_ptr]` 记形态 D ---
                if ops and _writes_first(mnem, ops) and ops[0].type == cx.X86_OP_REG:
                    r = ops[0].reg
                    nv = None
                    if mnem == "lea" and len(ops) == 2 and ops[1].type == cx.X86_OP_MEM:
                        m = ops[1].mem
                        if m.base == cx.X86_REG_RIP:
                            nv = end + m.disp
                        elif m.base in val and not m.index:
                            nv = val[m.base] + m.disp
                    elif mnem == "mov" and len(ops) == 2:
                        src = ops[1]
                        if src.type == cx.X86_OP_IMM:
                            nv = src.imm
                        elif src.type == cx.X86_OP_REG and src.reg in val:
                            nv = val[src.reg]
                        elif src.type == cx.X86_OP_MEM and src.mem.base == cx.X86_REG_RIP:
                            slot = end + src.mem.disp
                            if slot in self.loc_ptr_slots:
                                self._record_loc(self.loc_ptr_slots[slot], ins, "D")
                            nv = self.relocs.get(slot)      # 形态 D：槽的值
                    elif mnem in ("add", "sub") and len(ops) == 2 \
                            and ops[1].type == cx.X86_OP_IMM and r in val:
                        nv = val[r] + (ops[1].imm if mnem == "add" else -ops[1].imm)
                    if nv is not None and nv >= 0x1000:
                        val[r] = nv
                    else:
                        val.pop(r, None)
                # 跨基本块时清空：陈旧值会伪造引用
                if mnem.startswith(("ret", "call", "jmp", "j", "loop")):
                    val.clear()
                nops += 1
        self.n_insns = nops
        if verbose:
            print("[loc_xrefs] %d Location / %d 字符串槽 / %d 指令 / %d 引用目标"
                  % (len(self.locs), len(self.string_slots), nops, len(self.refs)),
                  file=sys.stderr)

    def _record(self, tgt, ins, form):
        if not self._in_data(tgt):
            return
        rec = (ins.address, form, ins.mnemonic, ins.op_str)
        self.refs[tgt].append(rec)
        self.refs_from[ins.address].append((tgt, form, ins.mnemonic, ins.op_str))

    def _record_loc(self, loc, ins, form):
        rec = (ins.address, form, ins.mnemonic, ins.op_str)
        self.refs[loc].append(rec)
        self.refs_from[ins.address].append((loc, form, ins.mnemonic, ins.op_str))

    def _in_data(self, va):
        return any(lo <= va < hi for lo, hi, _n in self.data_ranges)

    def _in_exec(self, va):
        return any(lo <= va < hi for lo, hi in self.exec_ranges)

    # -- 函数边界（扫描之后建，因为要用 branch_targets / insn_at）--------
    def _build_functions(self):
        eh = self.elf.sec(".eh_frame")
        try:
            fdes = parse_eh_frame(
                self.data[eh["offset"]:eh["offset"] + eh["size"]], eh["addr"])
        except (ValueError, struct.error, IndexError) as e:
            if os.environ.get("GW2_XREF_DEBUG"):
                print("[loc_xrefs] .eh_frame 解析失败（%r），退回 .eh_frame_hdr" % (e,),
                      file=sys.stderr)
            fdes = []
        if not fdes:
            fdes = [(a, 0) for a in parse_eh_frame_hdr(self.elf)]
        self.fde_ranges = [r for r in fdes if r[1]]
        self.fde_starts = sorted({a for a, _l in fdes})
        self._fde_lo = [a for a, _l in self.fde_ranges]
        self._fde_rng = self.fde_ranges

        src = {a: "fde" for a in self.fde_starts}
        for lo, _hi in self.exec_ranges:
            src.setdefault(lo, "section-start")
        for t in self.branch_targets:
            if t in src or t not in self.insn_at:
                continue
            if not self.in_gap(t):
                continue
            src[t] = "call-target"
        self.start_source = src
        self.starts = sorted(src)
        self.start_set = set(self.starts)
        self._extent = {}
        for i, a in enumerate(self.starts):
            nxt = self.starts[i + 1] if i + 1 < len(self.starts) else (1 << 64)
            self._extent[a] = (a, nxt)
        self.extra = sorted(a for a, k in src.items() if k == "call-target")

    def in_gap(self, va):
        """不在任何 FDE 区间里 ⇒ 归属只能是「最近前驱」，可信度低一档。"""
        if not self._fde_lo:
            return True
        i = bisect.bisect_right(self._fde_lo, va) - 1
        if i < 0:
            return True
        st, ln = self._fde_rng[i]
        return not (st <= va < st + ln)

    def func_of(self, va):
        i = bisect.bisect_right(self.starts, va) - 1
        return self.starts[i] if i >= 0 else None

    def func_end(self, f):
        return self._extent[f][1] if f in self._extent else None

    def func_source(self, f):
        """这个起点是哪来的：`fde` / `call-target` / `section-start`。"""
        return self.start_source.get(f, "?")

    # -- 查询 ----------------------------------------------------------
    def refs_to(self, addr):
        return sorted(self.refs.get(addr, ()))

    def funcs_of_loc(self, slot):
        """引用该 Location 的函数集合（首址 / +8 / +16 三阶段合并）。"""
        return sorted(self.func_hits_of_loc(slot))

    def func_hits_of_loc(self, slot):
        """`{func_start: [(insn, form, mn, op, +off)]}` —— 带证据链的归属。"""
        out = collections.defaultdict(list)
        for off in (0, 8, 16):
            for insn, form, mnem, op in sorted(self.refs.get(slot + off, ())):
                fn = self.func_of(insn)
                if fn is not None:
                    out[fn].append((insn, form, mnem, op, off))
        return dict(out)

    def locs_of_func(self, fn):
        """该函数引用到的全部 Location：`{(file, line, col): [(insn, form, mn, op)]}`。"""
        out = collections.defaultdict(list)
        lo, hi = fn, self.func_end(fn)
        for tgt, lst in self.refs.items():
            slot = self.loc_hit.get(tgt)
            if slot is None:
                continue
            for insn, form, mnem, op in lst:
                if lo <= insn < hi:
                    out[self.locs[slot]].append((insn, form, mnem, op))
        return dict(out)

    def read(self, va, n):
        return self.elf.read(va, n)


# ---------------------------------------------------------------------------
# 4. 打印 / 校验
# ---------------------------------------------------------------------------
def fmt(v):
    return "0x%x" % v if v is not None else "?"


def gh(v):
    return "0x%x" % (v + 0x100000) if v is not None else "?"


def load_tsv(path):
    rows, header = [], None
    with open(path, encoding="utf-8") as f:
        for line in f:
            if line.startswith("#"):
                continue
            parts = line.rstrip("\n").split("\t")
            if header is None:
                header = parts
                continue
            rows.append(dict(zip(header, parts)))
    return rows


def check_tsv(ix, path):
    """用 `fixtures/name_to_addr.tsv` 的**唯一锚**做交叉验证。

    判据：TSV 每一行 `(x86_addr, src_file, src_line)` 断言「这个函数引用了这一行」。
    本工具独立算 `(file,line) → 函数`，若该行落不进 `x86_addr` 的 Location 集合，
    要么归属错、要么函数边界不同。逐条打印，最后给一致/不一致计数。
    """
    rows = [r for r in load_tsv(path) if r.get("join_kind") == "唯一锚"]
    per_func = collections.defaultdict(set)
    for slot, (f, l, _c) in ix.locs.items():
        for fn in ix.funcs_of_loc(slot):
            per_func[fn].add((f, l))
    ok, bad, missing = 0, [], []
    for r in rows:
        addr = int(r["x86_addr"], 0)
        want = (r["src_file"], int(r["src_line"]))
        if addr not in ix.start_set:
            missing.append((r, "不是本工具的函数起点（边界口径不同）"))
            continue
        got = per_func.get(addr, set())
        if want in got:
            ok += 1
        else:
            bad.append((r, sorted(got)))
    n = len(rows)
    print("唯一锚 %d 条：一致 %d / 不一致 %d / 起点缺失 %d"
          % (n, ok, len(bad), len(missing)))
    if bad:
        print("\n不一致逐条（TSV 声称 vs 本工具算出的该函数 src:行）：")
        for r, got in bad:
            print("  %s %s::%s  TSV=%s:%s  本工具=%s"
                  % (r["x86_addr"], r["module"], r["method"], r["src_file"],
                     r["src_line"],
                     ", ".join("%s:%d" % g for g in got)
                     or "（该函数没引用任何 Location）"))
            alt = sorted({fn for fn, s in per_func.items()
                          if (r["src_file"], int(r["src_line"])) in s})
            if alt:
                print("      ↳ 该 src:行 被本工具归到：%s" % ", ".join(fmt(a) for a in alt))
    if missing:
        print("\n起点缺失 %d 条：" % len(missing))
        for r, why in missing:
            print("  %s %s::%s — %s" % (r["x86_addr"], r["module"], r["method"], why))
    return ok, bad, missing


def loc_forms(ix):
    """Location 命中按 (形态, 结构体偏移) 计数 —— 覆盖判据的实测值。"""
    forms = collections.Counter()
    insns = set()
    gap = 0
    for slot in ix.locs:
        for off in (0, 8, 16):
            for insn, form, _m, _o in ix.refs.get(slot + off, ()):
                forms[(form, off)] += 1
                insns.add(insn)
                if ix.in_gap(insn):
                    gap += 1
    return forms, insns, gap


def cmd_stats(ix, args):
    import hashlib
    h = hashlib.sha256(ix.data).hexdigest()
    text = ix.elf.sec(".text")
    print("目标      : %s" % ix.elf.path)
    print("sha256    : %s %s" % (h, "(已知良好目标，与 docs/16 §26/§27 同一份)"
                                 if h == KNOWN_SHA256
                                 else "⚠️ 与 KNOWN_SHA256 不同：旧结论不要跨构建套用"))
    print(".text     : %s .. %s (%d B)" % (fmt(text["addr"]),
                                           fmt(text["addr"] + text["size"]), text["size"]))
    print("Location  : %d" % len(ix.locs))
    print("字符串槽  : %d (.data.rel.ro 里 (&str,len) 形态)" % len(ix.string_slots))
    n_fde = sum(1 for a, k in ix.start_source.items() if k == "fde")
    n_tgt = sum(1 for a, k in ix.start_source.items() if k == "call-target")
    n_sec = sum(1 for a, k in ix.start_source.items() if k == "section-start")
    print("函数起点  : %d = FDE %d + call-target %d + 段首 %d"
          % (len(ix.starts), n_fde, n_tgt, n_sec))
    cov = sum(l for _a, l in ix.fde_ranges)
    print("FDE 覆盖  : %d B = %.2f%% of .text；起点全部是指令边界：%s"
          % (cov, 100.0 * cov / text["size"],
             all(s in ix.insn_at for s in ix.fde_starts)))
    print("引用目标  : %d 个地址 / %d 条引用（只记可寻址数据节）"
          % (len(ix.refs), sum(len(v) for v in ix.refs.values())))
    print("直接调用  : %d 个目标 / %d 个调用点"
          % (len(ix.calls), sum(len(v) for v in ix.calls.values())))
    print("指令      : %d 条（%d 段）" % (ix.n_insns, len(ix._insns)))

    forms, insns, gap = loc_forms(ix)
    print("\nLocation 命中（覆盖判据实测，逐形态）:")
    for (form, off), n in sorted(forms.items()):
        print("  形态 %-2s +%-2d : %d 条" % (form, off, n))
    print("  合计          : %d 条引用，落在 %d 条不同指令上"
          % (sum(forms.values()), len(insns)))
    print("  形态 D 载体槽 : %d 个（值本身指向某个 Location 的 .data.rel.ro 槽）"
          % len(ix.loc_ptr_slots))
    print("  落在空隙里    : %d 条（0 = 归属无歧义，不靠「最近前驱」兜底）" % gap)
    n_noref = sum(1 for s in ix.locs if not ix.funcs_of_loc(s))
    print("  无引用的 Location: %d" % n_noref)
    anchored = {f for s in ix.locs for f in ix.funcs_of_loc(s)}
    print("\n被 Location 锚定的函数: %d / %d" % (len(anchored), len(ix.starts)))
    print("\n复算：../.venv/bin/python tools/loc_xrefs.py check-tsv")


def _locs_from_spec(ix, spec):
    """`src/foo.rs:12` / `src/foo.rs:12:3` / `0x95e4f8` → [(slot, (file,line,col))]"""
    if spec.startswith("0x"):
        slot = int(spec, 16)
        return [(slot, ix.locs[slot])] if slot in ix.locs else []
    parts = spec.rsplit(":", 2)
    if len(parts) < 2:
        raise SystemExit("用法：loc <文件>:<行>[:<列>]，例如 loc src/graphwar_plane.rs:274:129")
    f, line = parts[0], int(parts[1])
    col = int(parts[2]) if len(parts) > 2 else None
    out = []
    for slot, c in ix.by_file_line.get((f, line), ()):
        if col is None or c == col:
            out.append((slot, ix.locs[slot]))
    return sorted(out)


def cmd_loc(ix, args):
    for spec in args.specs:
        hits = _locs_from_spec(ix, spec)
        if not hits:
            print("%s：本工具没找到这个 Location" % spec)
            continue
        for slot, (f, l, c) in hits:
            print("%s  slot=%s  %s:%d:%d" % (spec, fmt(slot), f, l, c))
            for fn, recs in sorted(ix.func_hits_of_loc(slot).items()):
                for insn, form, mnem, op, off in recs:
                    print("   ref %-10s %-2s +%-2d  %-7s %-40s func=%s%s"
                          % (fmt(insn), form, off, mnem, op[:40], fmt(fn),
                             "  [空隙]" if ix.in_gap(insn) else ""))
                print("   func %s .. %s  (%s)" % (fmt(fn), fmt(ix.func_end(fn)),
                                                  ix.func_source(fn)))
            if not ix.funcs_of_loc(slot):
                print("   （没有引用点）")


def cmd_slot(ix, args):
    for spec in args.specs:
        a = int(spec, 0)
        recs = ix.refs_to(a)
        print("%s：%d 条引用" % (fmt(a), len(recs)))
        for insn, form, mnem, op in recs:
            print("   %-10s %-2s %-7s %-40s func=%s"
                  % (fmt(insn), form, mnem, op[:40], fmt(ix.func_of(insn))))


def cmd_func(ix, args):
    for spec in args.specs:
        f = int(spec, 0)
        print("%s .. %s  (%s, %d B)" % (fmt(f), fmt(ix.func_end(f)),
                                        ix.func_source(f), ix.func_end(f) - f))
        locs = ix.locs_of_func(f)
        for (file, line, col), insns in sorted(locs.items()):
            print("   Location %-52s :%-5d:%-4d  %d 条引用 %s"
                  % (file, line, col, len(insns),
                     ", ".join(fmt(i[0]) for i in sorted(insns)[:6])))
        if not locs:
            print("   （该函数不引用任何 panic::Location —— 无锚）")
        cs = sorted({t for t, sites in ix.calls.items()
                     for s in sites if ix.func_of(s) == f})
        print("   calls: %s" % (", ".join(fmt(t) for t in cs[:24]) or "-"))


def cmd_check_tsv(ix, args):
    path = args.tsv
    if not path:
        here = os.path.dirname(os.path.abspath(__file__))
        path = os.path.join(os.path.dirname(here), "fixtures", "name_to_addr.tsv")
    ok, bad, missing = check_tsv(ix, path)
    return 0 if not bad and not missing else 1


def cmd_dump(ix, args):
    what = args.specs[0] if args.specs else "refs"
    if what == "refs":
        for tgt in sorted(ix.refs):
            for insn, form, mnem, op in sorted(ix.refs[tgt]):
                print("%s\t%s\t%s\t%s\t%s" % (fmt(tgt), fmt(insn), form, mnem, op))
    elif what == "locs":
        for slot, (f, l, c) in sorted(ix.locs.items()):
            print("%s\t%s:%d:%d\t%s" % (fmt(slot), f, l, c,
                                        ",".join(fmt(x) for x in ix.funcs_of_loc(slot))
                                        or "-"))
    elif what == "funcs":
        for f in ix.starts:
            print("%s\t%s\t%d\t%s" % (fmt(f), fmt(ix.func_end(f)),
                                      ix.func_end(f) - f, ix.func_source(f)))
    else:
        raise SystemExit("dump 只认 refs / locs / funcs")
    return 0


def main(argv):
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", choices=["stats", "loc", "slot", "func", "check-tsv", "dump"])
    ap.add_argument("specs", nargs="*", help="地址或 文件:行[:列]；dump 用 refs/locs/funcs")
    ap.add_argument("--so", default=DEFAULT_SO)
    ap.add_argument("--tsv", default=None)
    ap.add_argument("--quiet", action="store_true")
    o = ap.parse_args(argv[1:])
    if not os.path.exists(o.so):
        raise SystemExit("目标 .so 不存在：%s（用 --so 或 GW2_TARGET_SO 指定）" % o.so)
    ix = LocXrefs(o.so, verbose=not o.quiet)
    if o.cmd == "stats":
        cmd_stats(ix, o)
        return 0
    if o.cmd == "loc":
        cmd_loc(ix, o)
        return 0
    if o.cmd == "slot":
        cmd_slot(ix, o)
        return 0
    if o.cmd == "func":
        cmd_func(ix, o)
        return 0
    if o.cmd == "check-tsv":
        return cmd_check_tsv(ix, o)
    if o.cmd == "dump":
        return cmd_dump(ix, o)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
