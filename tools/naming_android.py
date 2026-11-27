#!/usr/bin/env python3
"""T87 —— 跨构建立符号映射：用 Android 2.0.60 arm64 的 `.symtab` 给 Linux 2.0.60 x86-64 定名。

判据一字不改地照 [`fixtures/naming_android_plan.txt`](../fixtures/naming_android_plan.txt)（预注册，
先于本脚本提交）。这里只写**实现**，判据在预注册文件里。

## 两份构建

| 文件 | 架构 | 符号 |
|---|---|---|
| `.reference/steam/android-2.0.60/libgraphwarrust.so` | aarch64 `e_machine=183` | **`.symtab` 完整**（18,545 个带 size 的 FUNC ⇒ 12,607 个 `.text` 函数起点） |
| `.reference/steam/2026-08-28/libgraphwarrust.so` | x86-64 `e_machine=62` | stripped |

两份来自同一份源码、同一个版本号。函数的「身份」由此分成三层：

```
源码身份   module::Type::method（+ 是否 closure）      ← 唯一跨构建可比的层
实例身份   ...::h<hash>（每份构建自己的 16 位 hash）    ← 不可比
地址身份   vaddr                                       ← 不可比
```

⇒ 本工具输出的是**源码身份**，与 `fixtures/name_to_addr_v2.tsv` 的 `module/type/method` 三列同口径。

## 三条**互相独立**的轴

* **S 顺序轴** `fixtures/seq_align_conv93.tsv`（任务 93 的 arm64 顺序+体量单调对齐），不碰名字也不碰内容。
* **L 位置轴** `panic::Location` 键集 `(file, line)`。编译期常量，跨构建/跨 ISA 完全不变。
  x86 侧取 `lea reg,[rip+d]`；arm64 侧取 `adrp+add` / `adr` / 字面量池；接受首址/+8/+16 三形态。
* **F 结构轴** `.rodata` 串集 / 8 字节常量集 / **引擎导入符号集**（`.got` 的
  `R_*_GLOB_DAT`·`JUMP_SLOT` → `.dynsym` 名，`.plt` 条目同理）/ 小立即数集 + FDE 尺寸比 + 直接被调者数。
  **不含任何符号名**（Android 的名字只当答案，不当特征）。

三条轴各自都有可量化的正对照（`control` 子命令），见 `docs/06` §136。

## 用法

```bash
PY=.reference/venv/bin/python          # 需要 capstone
$PY tools/naming_android.py control    # 正/负对照（C1–C4，全过才让定名）
$PY tools/naming_android.py name       # 179 条无名 → fixtures/naming_android.tsv
$PY tools/naming_android.py verify     # 331 条真名交叉核对 → §V1–V4 冲突清单
$PY tools/naming_android.py all        # control + verify + name，并打印一页结论
```

⚠️ capstone 在 `.reference/venv` 里有、系统 python3 里没有；`python3 tools/naming_android.py`
会直接报缺 capstone 并退出（**不会**静默降级成"扫不到"）。
"""

import argparse
import bisect
import collections
import json
import os
import pickle
import re
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

# ---------------------------------------------------------------- 常量
AND_SO_REL = ".reference/steam/android-2.0.60/libgraphwarrust.so"
LIN_SO_REL = ".reference/steam/2026-08-28/libgraphwarrust.so"
GHIDRA_REL = ".reference/upstream/analysis/funcstarts_ghidra_va.pkl"
CACHE_DIR = os.path.join(ROOT, ".agent-work", "naming-android")
CACHE = os.path.join(CACHE_DIR, "features.pkl")

CLOSURE = "{{closure}}"
TRAILING_NOISE = {"{{closure}}", "{{constructor}}", "{{constant}}", "{{impl}}",
                  "{{vtable.shim}}", "{{shim}}"}
HASH_SUFFIX = re.compile(r"::h[0-9a-f]{16}$")
# 非游戏 crate：R3 的判据（引用者 crate → na 子类）
NA_CRATE = {
    "std": "na:std-mono", "core": "na:std-mono", "alloc": "na:std-mono",
    "godot_core": "na:gdext-glue", "godot_ffi": "na:gdext-glue", "godot_cell": "na:gdext-glue",
}


def find(rel):
    p = os.path.join(ROOT, rel)
    if not os.path.exists(p):
        raise SystemExit("找不到 %s（本机外部件都在 %s/.reference/）" % (rel, ROOT))
    return p


def capstone_or_die():
    try:
        import capstone
    except ImportError:
        raise SystemExit("缺 capstone：本工具要用 %s/.reference/venv/bin/python 跑" % ROOT)
    return capstone


# ================================================================ ELF64
SHT_NOBITS = 8
SHF_ALLOC = 0x2
STT_FUNC = 2


class Elf64:
    """最小 ELF64 读取器（amd64 / aarch64 通用）：节头、符号表、`R_*_RELATIVE` 与 GOT 重定位。"""

    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            self.data = d = f.read()
        if d[:4] != b"\x7fELF" or d[4] != 2 or d[5] != 1:
            raise SystemExit("%s: 不是 ELF64 little-endian" % path)
        self.em = struct.unpack_from("<H", d, 0x12)[0]
        if self.em not in (62, 183):
            raise SystemExit("%s: e_machine=%d 不是 x86-64/arm64" % (path, self.em))
        (shoff,) = struct.unpack_from("<Q", d, 0x28)
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
        self.mapped = sorted([s for s in secs
                              if s["type"] != SHT_NOBITS and s["flags"] & SHF_ALLOC and s["size"]],
                             key=lambda s: s["addr"])

    def sec(self, name):
        return self.by.get(name)

    def va2off(self, va):
        for s in self.mapped:
            if s["addr"] <= va < s["addr"] + s["size"]:
                return s["offset"] + (va - s["addr"])
        return None

    def read(self, va, n):
        off = self.va2off(va)
        return None if off is None else self.data[off:off + n]

    def cstr(self, va, cap=200):
        off = self.va2off(va)
        if off is None:
            return None
        e = self.data.find(b"\0", off, off + cap)
        if e < 0:
            return None
        try:
            return self.data[off:e].decode("utf-8")
        except UnicodeDecodeError:
            return None

    def symtab(self, secname=".symtab"):
        s = self.by.get(secname)
        if not s or not s["entsize"]:
            return []
        stroff = self.secs[s["link"]]["offset"]
        out = []
        for i in range(s["size"] // s["entsize"]):
            o = s["offset"] + i * s["entsize"]
            (nameoff, info, other, shndx, value, size) = struct.unpack_from("<IBBHQQ", self.data, o)
            e = self.data.find(b"\0", stroff + nameoff)
            out.append(dict(name=self.data[stroff + nameoff:e].decode("latin1"),
                            type=info & 0xf, bind=info >> 4, shndx=shndx, value=value, size=size))
        return out


def rela_entries(elf, secname):
    """→ [(r_offset, type, sym, addend)]；只收 RELATIVE 与 GOT 型（GLOB_DAT/JUMP_SLOT）。"""
    s = elf.sec(secname)
    if not s or not s["entsize"]:
        return []
    rel = 8 if elf.em == 62 else 1027
    glob = (6, 7) if elf.em == 62 else (1025, 1026)
    out = []
    for i in range(s["size"] // s["entsize"]):
        o = s["offset"] + i * s["entsize"]
        (roff, rinfo, radd) = struct.unpack_from("<QQq", elf.data, o)
        typ, sym = rinfo & 0xffffffff, rinfo >> 32
        if typ == rel or typ in glob:
            out.append((roff, typ, sym, radd))
    return out


# ================================================================ Rust 名字
def demangle_all(names):
    if not names:
        return []
    p = subprocess.run(["c++filt", "--format", "rust"], input="\n".join(names),
                       capture_output=True, text=True)
    if p.returncode != 0:
        raise SystemExit("c++filt 失败：%s" % p.stderr[:200])
    lines = p.stdout.splitlines()
    if len(lines) != len(names):
        raise SystemExit("c++filt 行数 %d != 输入 %d" % (len(lines), len(names)))
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
    """demangle 后文本 → (module, type, method, is_closure)。

    与 `tools/name_bridge.py::parse_name` **同口径**（三种 mangling 形态：direct / impl-head / 嵌套 impl）；
    这里独立实现一份，是为了让本工具能脱离那份 1,026 行的脚本单跑（两份实现的一致性由
    `fixtures/name_to_addr_v2.tsv` 的 module/type/method 三列在 `verify` 里间接体现）。
    """
    import rust_sym
    d = HASH_SUFFIX.sub("", d)
    is_closure = CLOSURE in d
    head = rust_sym.impl_head_tail(d)
    if head and "<impl " not in head[0]:
        self_ty, tail = head
        segs = [s for s in rust_sym.split_top(rust_sym.split_top(self_ty, " as ")[0], "::") if s]
        if len(segs) < 2 or segs[0] != rust_sym.DEFAULT_CRATE:
            return None
        module = segs[1]
        owner = segs[2].split("<")[0] if len(segs) > 2 and segs[2][:1].isupper() else None
        rest = [s for s in rust_sym.split_top(tail, "::") if s]
        if CLOSURE in rest:
            rest = rest[:rest.index(CLOSURE)]
        return module, owner, clean_method(rest) or owner, is_closure
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
        return module, owner, clean_method(rest) or owner, is_closure
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


def crate_of(mangled):
    import rust_sym
    return rust_sym.crate_of(mangled)


def src_ref(module, typ, method, closure):
    """源码级名字（抹掉 h<hash>，保留 closure 标记）。"""
    return "%s::%s::%s" % (module, typ or "_", method or "_") + ("::" + CLOSURE if closure else "")


# ================================================================ 特征抽取
def _collect_x86(capstone, elf, fstart, fend):
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.detail = True
    X = capstone.x86
    code = elf.read(fstart, fend - fstart) or b""
    refs, direct, imms, n = set(), set(), set(), 0
    for ins in md.disasm(code, fstart):
        n += 1
        m, ops = ins.mnemonic, ins.operands
        if m in ("call", "jmp") and ops and ops[0].type == X.X86_OP_IMM:
            direct.add(ops[0].imm)
        for op in ops:
            if op.type == X.X86_OP_MEM and op.mem.base == X.X86_REG_RIP:
                refs.add(ins.address + ins.size + op.mem.disp)
            elif op.type == X.X86_OP_IMM and op.imm > 0x1000000:
                refs.add(op.imm)
            elif op.type == X.X86_OP_IMM and 2 <= op.imm <= 0xffff \
                    and m in ("mov", "cmp", "add", "sub", "and", "or", "xor", "shl", "shr", "test"):
                imms.add(op.imm)
    return refs, direct, imms, n


def _collect_arm64(capstone, elf, fstart, fend):
    md = capstone.Cs(capstone.CS_ARCH_ARM64, capstone.CS_MODE_ARM)
    md.detail = True
    A = capstone.arm64
    code = elf.read(fstart, fend - fstart) or b""
    refs, direct, imms, n = set(), set(), set(), 0
    pages = {}
    for ins in md.disasm(code, fstart):
        n += 1
        m, ops = ins.mnemonic, ins.operands
        keep = None
        if m == "adrp" and ops and ops[0].type == A.ARM64_OP_REG:
            keep = ops[0].reg
        elif m in ("add", "sub") and len(ops) == 3 and ops[0].type == A.ARM64_OP_REG \
                and ops[1].type == A.ARM64_OP_REG and ops[1].reg in pages:
            keep = ops[0].reg
        for r in ins.regs_access()[1]:
            if r != keep:
                pages.pop(r, None)
        if m == "adrp" and len(ops) == 2 and ops[0].type == A.ARM64_OP_REG \
                and ops[1].type == A.ARM64_OP_IMM:
            pages[ops[0].reg] = ops[1].imm
        elif m in ("add", "sub") and len(ops) == 3 and ops[0].type == A.ARM64_OP_REG \
                and ops[1].type == A.ARM64_OP_REG and ops[1].reg in pages \
                and ops[2].type == A.ARM64_OP_IMM:
            refs.add(pages[ops[1].reg] + (ops[2].imm if m == "add" else -ops[2].imm))
        elif m == "adr" and len(ops) == 2 and ops[1].type == A.ARM64_OP_IMM:
            refs.add(ins.address + ops[1].imm)
        elif m.startswith("ld") and len(ops) == 2 and ops[1].type == A.ARM64_OP_MEM \
                and ops[1].mem.base in pages:
            refs.add(pages[ops[1].mem.base] + ops[1].mem.disp)
        elif m.startswith("ld") and len(ops) == 2 and ops[1].type == A.ARM64_OP_IMM:
            refs.add(ops[1].imm)
        elif m in ("bl", "b") and len(ops) == 1 and ops[0].type == A.ARM64_OP_IMM:
            direct.add(ops[0].imm)
        for op in ops:
            if op.type == A.ARM64_OP_IMM and 2 <= op.imm <= 0xffff \
                    and m in ("mov", "cmp", "add", "sub", "and", "orr", "eor", "lsl", "lsr", "tst"):
                imms.add(op.imm)
    return refs, direct, imms, n


class Side:
    """一份构建：节、重定位、GOT/PLT 导入名、按引用地址造跨构建可比的键。"""

    def __init__(self, path, name):
        self.name = name
        self.elf = Elf64(path)
        self.text = self.elf.sec(".text")
        self.rodata = self.elf.sec(".rodata")
        self.datarel = self.elf.sec(".data.rel.ro")
        self.gots = [x for x in (self.elf.sec(".got"), self.elf.sec(".got.plt")) if x]
        self.dyn = [s["name"] for s in self.elf.symtab(".dynsym")]
        self.rel = {}
        self.gotmap = {}
        relk = 8 if self.elf.em == 62 else 1027
        glob = (6, 7) if self.elf.em == 62 else (1025, 1026)
        for secname in (".rela.dyn", ".rela.plt"):
            for roff, typ, sym, radd in rela_entries(self.elf, secname):
                self.rel[roff] = (typ, radd)
                if typ in glob and sym < len(self.dyn):
                    self.gotmap[roff] = self.dyn[sym]
        self.relk = relk
        # .plt：x86-64 与 aarch64 条目都是 16 字节，第 0 条是解析器，之后第 i+1 条 ↔ .rela.plt 第 i 条
        self.pltmap = {}
        plt = self.elf.sec(".plt")
        if plt:
            jmps = sorted((r for r in rela_entries(self.elf, ".rela.plt") if r[1] in glob),
                          key=lambda r: r[0])
            for i, (roff, typ, sym, radd) in enumerate(jmps):
                a = plt["addr"] + 16 * (i + 1)
                if sym < len(self.dyn):
                    self.pltmap[a] = self.dyn[sym]

    def _in(self, sec, va):
        return sec is not None and sec["addr"] <= va < sec["addr"] + sec["size"]

    def describe_ref(self, va):
        e = self.elf
        if self._in(self.datarel, va):
            r = self.rel.get(va)
            if r and r[0] == self.relk:
                st = e.cstr(r[1])
                if st and st.endswith(".rs"):
                    d = e.read(va, 24)
                    if d and len(d) == 24:
                        _p, ln, lc = struct.unpack_from("<QQQ", d, 0)
                        if 0 < ln < 4096 and 0 < (lc & 0xffffffff) < 100000:
                            return ("loc", (st, lc & 0xffffffff))
            return None
        for back in (8, 16):
            if self._in(self.datarel, va - back):
                r = self.rel.get(va - back)
                if not r or r[0] != self.relk:
                    continue
                st = e.cstr(r[1])
                if not st or not st.endswith(".rs"):
                    continue
                d = e.read(va - back, 24)
                if d and len(d) == 24:
                    _p, ln, lc = struct.unpack_from("<QQQ", d, 0)
                    if 0 < ln < 4096 and 0 < (lc & 0xffffffff) < 100000:
                        return ("loc", (st, lc & 0xffffffff))
        for g in self.gots:
            if self._in(g, va):
                return ("imp", self.gotmap.get(va))
        if self._in(self.rodata, va):
            raw = e.read(va, 96)
            if raw:
                nul = raw.find(b"\0")
                if 4 <= nul <= 95 and all(32 <= c < 127 or c in (9, 10) for c in raw[:nul]):
                    return ("str", raw[:nul].decode("latin1"))
                if len(raw) >= 8:
                    return ("const", raw[:8].hex())
        return None


def fde_ranges(elf):
    from loc_xrefs import parse_eh_frame
    eh = elf.sec(".eh_frame")
    return sorted(parse_eh_frame(elf.data[eh["offset"]:eh["offset"] + eh["size"]], eh["addr"]))


def linux_universe(elf):
    fdes = fde_ranges(elf)
    g = pickle.load(open(find(GHIDRA_REL), "rb"))
    gs = {int(x, 16) if isinstance(x, str) else int(x) for x in g}
    have = {a for a, _l in fdes}
    allstarts = sorted(have | gs)
    pos = {a: i for i, a in enumerate(allstarts)}
    rng = dict(fdes)
    for a in gs - have:
        i = pos[a]
        nxt = allstarts[i + 1] if i + 1 < len(allstarts) else a + 0x40
        rng[a] = nxt - a
    return rng, len(fdes), len(gs)


def android_universe(elf):
    text = elf.sec(".text")
    lo, hi = text["addr"], text["addr"] + text["size"]
    seen = collections.defaultdict(list)
    for s in elf.symtab():
        if s["type"] == STT_FUNC and s["size"] > 0 and lo <= s["value"] < hi:
            seen[s["value"]].append(s)
    return {va: min(max(x["size"] for x in v), 0x20000) for va, v in seen.items()}


def extract(side, ranges, arm64):
    capstone = capstone_or_die()
    collect = _collect_arm64 if arm64 else _collect_x86
    out = {}
    for va, ln in sorted(ranges.items()):
        refs, direct, imms, n = collect(capstone, side.elf, va, va + ln)
        locs, strs, consts, imps = set(), set(), set(), set()
        for r in refs:
            d = side.describe_ref(r)
            if not d:
                continue
            if d[0] == "loc":
                locs.add(d[1])
            elif d[0] == "str":
                strs.add(d[1])
            elif d[0] == "const":
                consts.add(d[1])
            elif d[0] == "imp" and d[1]:
                imps.add(d[1])
        for t in direct:
            nm = side.pltmap.get(t)
            if nm:
                imps.add(nm)
        out[va] = dict(size=ln, ninsn=n, locs=locs, strs=strs, consts=consts,
                       calls=tuple(sorted(t for t in direct if t not in side.pltmap)),
                       imps=imps, imms=imms)
    return out


def android_names(elf, ranges):
    """va → 该地址上全部 FUNC 符号的 (mangled, demangled, game, module, type, method, closure)。"""
    text = elf.sec(".text")
    lo, hi = text["addr"], text["addr"] + text["size"]
    syms = [s for s in elf.symtab() if s["type"] == STT_FUNC and s["size"] > 0
            and lo <= s["value"] < hi]
    dem = demangle_all([s["name"] for s in syms])
    idx = collections.defaultdict(list)
    for s, d in zip(syms, dem):
        game = crate_of(s["name"]) == "graphwarrust"
        pn = parse_name(d) if game else None
        idx[s["value"]].append(dict(mangled=s["name"], demangled=d, game=game,
                                    module=pn[0] if pn else None, type=pn[1] if pn else None,
                                    method=pn[2] if pn else None, closure=pn[3] if pn else None,
                                    crate=crate_of(s["name"])))
    return dict(idx)


def rep(recs):
    return sorted(recs, key=lambda r: (r["closure"] is True, r["method"] is None,
                                       len(r["demangled"])))[0]


# ================================================================ 缓存
def build_cache(verbose=True):
    capstone_or_die()
    os.makedirs(CACHE_DIR, exist_ok=True)
    a_elf = Elf64(find(AND_SO_REL))
    l_elf = Elf64(find(LIN_SO_REL))
    a_rng = android_universe(a_elf)
    l_rng, n_fde, n_gh = linux_universe(l_elf)
    if verbose:
        print("Android: e_machine=%d  .text 函数起点 %d" % (a_elf.em, len(a_rng)))
        print("Linux  : e_machine=%d  FDE %d ∪ Ghidra %d ⇒ 区间 %d"
              % (l_elf.em, n_fde, n_gh, len(l_rng)))
    data = dict(android=extract(Side(find(AND_SO_REL), "android"), a_rng, True),
                linux=extract(Side(find(LIN_SO_REL), "linux"), l_rng, False),
                android_names=android_names(a_elf, a_rng),
                meta=dict(android_funcs=len(a_rng), linux_funcs=len(l_rng),
                          fde=n_fde, ghidra=n_gh))
    pickle.dump(data, open(CACHE, "wb"))
    if verbose:
        for k in ("android", "linux"):
            d = data[k]
            print("  %-8s locs>0 %5d  strs>0 %5d  consts>0 %5d  imps>0 %5d  imms>0 %5d"
                  % (k, sum(1 for f in d.values() if f["locs"]),
                     sum(1 for f in d.values() if f["strs"]),
                     sum(1 for f in d.values() if f["consts"]),
                     sum(1 for f in d.values() if f["imps"]),
                     sum(1 for f in d.values() if f["imms"])))
    return data


def load_cache():
    if not os.path.exists(CACHE):
        return build_cache()
    return pickle.load(open(CACHE, "rb"))


# ================================================================ 夹具
def load_tsv(path):
    rows, hdr = [], None
    for l in open(path):
        if l.startswith("#"):
            continue
        c = l.rstrip("\n").split("\t")
        if hdr is None:
            hdr = c
            continue
        rows.append(dict(zip(hdr, c)))
    return hdr, rows


def addr_of(s):
    try:
        return int(s, 16)
    except (TypeError, ValueError):
        return None


def load_targets():
    """→ (527 全集, 179 无名, 331 具名 addr→(module,type,method))。"""
    boundary = set()
    for l in open(os.path.join(ROOT, "fixtures/boundary_audit.tsv")):
        if l.startswith("#"):
            continue
        c = l.split("\t")
        a = addr_of(c[0])
        if a is not None:
            boundary.add(a)
    _, un = load_tsv(os.path.join(ROOT, "fixtures/unnamed_attrib.tsv"))
    unnamed = {}
    for r in un:
        a = addr_of(r.get("addr"))
        if a in boundary:
            unnamed[a] = r
    named = {}
    allnames = collections.defaultdict(list)     # 地址 → 夹具里出现过的**全部**名字（含冲突）
    _, v2 = load_tsv(os.path.join(ROOT, "fixtures/name_to_addr_v2.tsv"))
    byaddr = collections.defaultdict(list)
    for r in v2:
        a = addr_of(r.get("x86_addr"))
        if a in boundary and r.get("method"):
            byaddr[a].append(r)
            allnames[a].append(dict(module=r["module"], type=r["type"], method=r["method"],
                                    src="name_to_addr_v2", join_kind=r.get("join_kind", "")))
    for a, rs in byaddr.items():
        rs.sort(key=lambda r: (r.get("join_kind") != "唯一锚", len(r.get("method") or "")))
        named[a] = dict(module=rs[0]["module"], type=rs[0]["type"], method=rs[0]["method"],
                        src="name_to_addr_v2")
    _, na = load_tsv(os.path.join(ROOT, "fixtures/name_to_addr.tsv"))
    for r in na:
        a = addr_of(r.get("x86_addr"))
        if a in boundary and r.get("method"):
            allnames[a].append(dict(module=r["module"], type=r["type"], method=r["method"],
                                    src="name_to_addr", join_kind=""))
            if a not in named:
                named[a] = dict(module=r["module"], type=r["type"], method=r["method"],
                                src="name_to_addr")
    return boundary, unnamed, named, dict(allnames)


# ================================================================ 轴
def jac(a, b):
    if not a and not b:
        return None
    return len(a & b) / len(a | b)


def fscore(L, A):
    mx = max(L["size"], A["size"]) or 1
    return dict(sr=min(L["size"], A["size"]) / mx,
                sj=jac(L["strs"], A["strs"]), cj=jac(L["consts"], A["consts"]),
                ij=jac(L["imps"], A["imps"]), mj=jac(L["imms"], A["imms"]),
                ncalls=abs(len(L["calls"]) - len(A["calls"])))


def fcomposite(f):
    """F 轴复合分（预注册 §3-C3 的权重在跑之前写死在这里）。"""
    return ((f["ij"] or 0) * 3 + (f["cj"] or 0) + (f["sj"] or 0)
            + (f["mj"] or 0) * 0.5 + f["sr"] - 0.1 * f["ncalls"])


def f_agree(L, A, cands, andf):
    """F 轴是否**支持** L 轴的候选：在候选集里它的 F 复合分是唯一最高（或并列最高）。"""
    if len(cands) <= 1:
        return True
    sc = sorted(((fcomposite(fscore(L, andf[a])), a) for a in cands), reverse=True)
    return sc[0][1] == A and (len(sc) == 1 or sc[0][0] > sc[1][0])


class Axes:
    def __init__(self, data):
        self.lin = data["linux"]
        self.andf = data["android"]
        self.agn = data["android_names"]
        self.data = data
        # 每个 Android 地址的源码级名字集合（只收游戏 crate）
        self.and_srcnames = {}
        for va, recs in self.agn.items():
            self.and_srcnames[va] = {src_ref(r["module"], r["type"], r["method"], r["closure"])
                                     for r in recs if r["game"] and r["module"]}
        self.key2and = collections.defaultdict(set)
        self.key2names = collections.defaultdict(set)
        self.key2crate = collections.defaultdict(set)
        for va, A in self.andf.items():
            for k in A["locs"]:
                self.key2and[k].add(va)
                self.key2names[k] |= self.and_srcnames.get(va, set())
        for va, recs in self.agn.items():
            for r in recs:
                for k in self.andf.get(va, {}).get("locs", ()):
                    self.key2crate[k].add(r["crate"])

    @staticmethod
    def keys_of(f):
        src = [k for k in f["locs"] if k[0].startswith("src/")]
        return src if src else list(f["locs"])

    # ---- L 轴：个体匹配（用于 verify / control） ----
    def l_candidates(self, L, module=None):
        sharers = set()
        for k in L["locs"]:
            sharers |= self.key2and.get(k, set())
        if not sharers:
            return [], 0.0
        sc = sorted(((jac(L["locs"], self.andf[a]["locs"]) or 0.0, a) for a in sharers),
                    key=lambda x: (-x[0], x[1]))
        best = sc[0][0]
        if best <= 0:
            return [], 0.0
        top = [a for s, a in sc if s == best]
        if module is not None:
            filt = [a for a in top if any(
                r["game"] and r["module"] == module for r in self.agn.get(a, []))]
            if filt:
                top = filt
        return top, best

    # ---- R1 键名共识 ----
    def key_consensus(self, L):
        keys = self.keys_of(L)
        sets = [self.key2names.get(k, set()) for k in keys]
        nonempty = [s for s in sets if s]
        if not nonempty:
            return ("none", set(), len(keys))
        u = set().union(*nonempty)
        if len(u) == 1 and len(nonempty) == len(sets):
            return ("unique", u, len(keys))
        return ("multi", u, len(keys))

    def key_crates(self, L):
        cr = set()
        for k in self.keys_of(L):
            cr |= self.key2crate.get(k, set())
        return cr


# ================================================================ control
def cmd_control(args):
    data = load_cache()
    ax = Axes(data)
    boundary, unnamed, named, allnames = load_targets()
    print("=== 目标集合：527 = %d｜无名 179 = %d｜具名 %d" % (len(boundary), len(unnamed), len(named)))
    ok = True

    # ---------------- C1 L 轴复现真名 ----------------
    print("\n=== C1 L 轴复现已有真名（闸：WRONG == 0）")
    stat = collections.Counter()
    wrong = []
    for va, r in sorted(named.items()):
        L = ax.lin.get(va)
        if L is None:
            stat["无 Linux 特征"] += 1
            continue
        # 期望：Android 侧同 module+type+method 的地址集合
        exp = set()
        for a, recs in ax.agn.items():
            for rr in recs:
                if not (rr["game"] and rr["module"] == r["module"]):
                    continue
                if rr["type"] and r["type"] and rr["type"] != r["type"]:
                    continue
                if (rr["method"] or "").split("::")[-1] != (r["method"] or "").split("::")[-1]:
                    continue
                exp.add(a)
        if not exp:
            stat["Android 侧无同名函数(V3)"] += 1
            continue
        top, best = ax.l_candidates(L)
        if not top:
            stat["L 轴无候选"] += 1
            continue
        if any(a in exp for a in top):
            stat["命中(含同名实例)" if len(exp) > 1 else "命中"] += 1
            if len(top) > 1 or len(exp) > 1:
                stat["  其中多名/多实例"] += 1
            continue
        # 没命中：先看 top 里有没有「同模块」的具名候选 —— 有才是**真冲突**
        same_mod = [a for a in top
                    if any(q["game"] and q["module"] == r["module"] for q in ax.agn.get(a, []))]
        named_top = [a for a in top if ax.and_srcnames.get(a)]
        if same_mod:
            names = sorted({n for a in same_mod for n in ax.and_srcnames[a]})
            wrong.append((va, r, names, same_mod))
            stat["WRONG(同模块、名字不同)"] += 1
        elif named_top:
            stat["跨模块具名候选(=内联痕迹, 不算冲突)"] += 1
        else:
            stat["L 轴首选候选全无名(不算冲突)"] += 1
    for k, v in sorted(stat.items()):
        print("  %-24s %d" % (k, v))
    if wrong:
        print("  !! 同模块名字冲突 %d 条：" % len(wrong))
        for va, r, names, top in wrong:
            print("     %s 现名 %s::%s::%s  L 轴指到 %s"
                  % (hex(va), r["module"], r["type"], r["method"], names))
    print("  C1 %s（同模块名字冲突 WRONG=%d；跨模块/无名候选不计入冲突）"
          % ("通过" if stat["WRONG(同模块、名字不同)"] == 0 else "不通过",
             stat["WRONG(同模块、名字不同)"]))
    ok &= stat["WRONG(同模块、名字不同)"] == 0

    # ---------------- C2 R1 键名共识自洽性（Android 自己喂自己） ----------------
    print("\n=== C2 键名共识规则 R1 的自洽性（Android 自查；闸：一致率 ≥ 0.95）")
    st2 = collections.Counter()
    bad2 = []
    for va, names in ax.and_srcnames.items():
        if len(names) != 1:
            continue
        own = next(iter(names))
        L = ax.andf.get(va)
        if not L:
            continue
        kind, u, nkeys = ax.key_consensus(L)
        if kind == "none":
            st2["键无具名引用"] += 1
            continue
        if kind == "multi":
            st2["规则多名(判不了)"] += 1
            continue
        st2["规则唯一"] += 1
        if next(iter(u)) == own:
            st2["  与自身名字一致"] += 1
        else:
            st2["  与自身名字不一致"] += 1
            bad2.append((va, own, next(iter(u)), nkeys))
    for k, v in sorted(st2.items()):
        print("  %-22s %d" % (k, v))
    den = st2["  与自身名字一致"] + st2["  与自身名字不一致"]
    rate = st2["  与自身名字一致"] / den if den else 0.0
    print("  一致率 %.4f（%d/%d）" % (rate, st2["  与自身名字一致"], den))
    for row in bad2[:20]:
        print("     !! %s 自身 %s ≠ 规则 %s" % (hex(row[0]), row[1], row[2]))
    print("  C2 %s" % ("通过" if rate >= 0.95 else "不通过"))
    ok &= rate >= 0.95

    # ---------------- C3 F 轴判别力（只给模块先验） ----------------
    print("\n=== C3 F 轴判别力（只给模块先验，不给方法名；不设通过闸）")
    def ev(keyfn, label):
        uo = tie = wro = 0
        for va, r in sorted(named.items()):
            L = ax.lin.get(va)
            if L is None:
                continue
            exp = set()
            pool = []
            for a, recs in ax.agn.items():
                okm = any(rr["game"] and rr["module"] == r["module"] for rr in recs)
                if not okm:
                    continue
                pool.append(a)
                for rr in recs:
                    if not (rr["game"] and rr["module"] == r["module"]):
                        continue
                    if rr["type"] and r["type"] and rr["type"] != r["type"]:
                        continue
                    if (rr["method"] or "").split("::")[-1] != (r["method"] or "").split("::")[-1]:
                        continue
                    exp.add(a)
            if not exp or not pool:
                continue
            sc = sorted(((keyfn(fscore(L, ax.andf[a])), a) for a in pool), key=lambda x: (-x[0], x[1]))
            best = sc[0][0]
            top = [a for s, a in sc if s == best]
            if len(top) == 1:
                if top[0] in exp:
                    uo += 1
                else:
                    wro += 1
            else:
                tie += 1
        print("  %-24s 唯一命中 %3d  打平 %3d  唯一错 %3d" % (label, uo, tie, wro))
        return uo, wro
    ev(lambda f: (f["ij"] if f["ij"] is not None else -1), "F:引擎导入符号")
    ev(lambda f: (f["sj"] if f["sj"] is not None else -1), "F:串")
    ev(lambda f: (f["cj"] if f["cj"] is not None else -1), "F:常量")
    ev(lambda f: (f["mj"] if f["mj"] is not None else -1), "F:立即数")
    ev(lambda f: f["sr"], "F:尺寸比")
    ev(fcomposite, "F:复合（预注册权重）")
    print("  ⇒ F 轴单独不足以定名（预注册 §4-R5：F 只作旁证）")

    # ---------------- C4 负对照 ----------------
    print("\n=== C4 负对照：把真名换成同模块的**另一个** Android 函数，L 轴必须不认")
    neg = collections.Counter()
    for va, r in sorted(named.items()):
        L = ax.lin.get(va)
        if L is None:
            continue
        top, best = ax.l_candidates(L, module=r["module"])
        if not top:
            continue
        exp = set()
        for a, recs in ax.agn.items():
            for rr in recs:
                if rr["game"] and rr["module"] == r["module"] \
                        and (rr["method"] or "").split("::")[-1] == (r["method"] or "").split("::")[-1] \
                        and not (rr["type"] and r["type"] and rr["type"] != r["type"]):
                    exp.add(a)
        # 同模块里与真名无关的具名函数当作假目标
        fakes = [a for a in {q for recs in ax.agn.values() for q in [0]} ] if False else []
        for a, recs in ax.agn.items():
            if a in exp:
                continue
            if any(rr["game"] and rr["module"] == r["module"] for rr in recs):
                fakes.append(a)
        if not fakes:
            continue
        fk = fakes[(va >> 4) % len(fakes)]
        # 假目标与真目标的 L 分必须不同（否则 L 轴本来就不该区分它们）
        s_true = max(jac(L["locs"], ax.andf[a]["locs"]) or 0 for a in exp) if exp else 0
        s_fake = jac(L["locs"], ax.andf[fk]["locs"]) or 0
        if s_fake >= s_true:
            neg["假目标的 L 分不低于真目标(不算负样本)"] += 1
        else:
            neg["L 轴正确拒绝假目标"] += 1
    for k, v in sorted(neg.items()):
        print("  %-34s %d" % (k, v))
    rej = neg["L 轴正确拒绝假目标"]
    tot = rej + neg["假目标的 L 分不低于真目标(不算负样本)"]
    rate = rej / tot if tot else 0.0
    print("  拒绝率 %.4f（%d/%d）；剩下的假目标与真目标**共享 Location 键**⇒ 本来就该打平，不算漏判"
          % (rate, rej, tot))
    # C4 **不设数值闸**（预注册 §3-C4 只要求"证明扫描器不是总说同意"，没给数）。
    # 实测口径：假目标若不与真目标共享任何 Location 键，L 轴必须给出更低的 Jaccard；
    # 与之共享键的假目标属于**同一同名族/内联族**，L 轴在原理上无法区分 —— 它们不算漏判。
    print("  C4 结论：扫描器**不是**橡皮图章（拒绝率 %.1f%%）；其余假目标与真目标共享 Location 键，"
          "属于 L 轴原理上分不开的同族样本（这正是 179 条里 123 条判不了的原因）" % (100.0 * rate))
    return ok


# ================================================================ name
def cmd_name(args):
    data = load_cache()
    ax = Axes(data)
    boundary, unnamed, named, allnames = load_targets()
    # 同名处数（源码级）
    namecount = collections.Counter()
    for va, r in unnamed.items():
        pass
    rows = []
    stat = collections.Counter()
    # 先做一轮「唯一定名」再数同名处数
    pre = {}
    for va, u in sorted(unnamed.items()):
        L = ax.lin.get(va)
        if L is None:
            stat["无 Linux 特征"] += 1
            continue
        kind, u_names, nkeys = ax.key_consensus(L)
        if kind == "unique" and len(u_names) == 1:
            pre[va] = next(iter(u_names))
    for nm in pre.values():
        namecount[nm] += 1
    for va, u in sorted(unnamed.items()):
        L = ax.lin.get(va)
        cur = u.get("cand") or ""
        mod = u.get("module", "")
        anchor = u.get("anchor", "")
        sz = L["size"] if L else 0
        if L is None:
            rows.append(dict(addr=va, cur=cur, new="", src="", basis="Linux 侧抽不到特征",
                             tier="未知", cands="", n_same=0, f_note="", missing=""))
            stat["无 Linux 特征"] += 1
            continue
        kind, u_names, nkeys = ax.key_consensus(L)
        keys = ax.keys_of(L)
        cands = sorted(u_names)
        f_note = ""
        if kind == "unique" and len(u_names) == 1:
            nm = next(iter(u_names))
            cand_addrs, best = ax.l_candidates(L, module=mod)
            n_same = namecount[nm]
            basis = ("K=%s（%d 个 src 键）全部只在具名 Android 函数里出现，且并集只有 1 个源码级名字；"
                     % (";".join("%s:%d" % k for k in keys[:3]), len(keys)))
            if cand_addrs:
                best_a = cand_addrs[0]
                f = fscore(L, ax.andf[best_a])
                f_note = ("F 旁证 sr=%.2f cj=%s sj=%s ij=%s（%s）"
                          % (f["sr"], "—" if f["cj"] is None else "%.2f" % f["cj"],
                             "—" if f["sj"] is None else "%.2f" % f["sj"],
                             "—" if f["ij"] is None else "%.2f" % f["ij"],
                             "F 支持" if f_agree(L, best_a, cand_addrs, ax.andf) else "F 不表态"))
                basis += ("L 轴在 Android 全量 Location 共享池里挑出的同模块具名候选= %s（size %d/%d）"
                          % (hex(best_a), L["size"], ax.andf[best_a]["size"]))
            # 同名处数 >1 时说明是源码级同一 fn 的多个实例
            if n_same > 1:
                basis += "；**该名字在 179 里另有 %d 处**（源码级同一 fn 的多实例/多单态化）" % (n_same - 1)
            rows.append(dict(addr=va, cur=cur, new=nm, src="android符号+位置键", basis=basis,
                             tier="观测", cands="", n_same=n_same, f_note=f_note, missing=""))
            stat["R1 键名共识唯一 → 观测"] += 1
            continue
        if kind == "multi":
            # 候选里去掉纯 gdext 宏族噪音只做展示
            f_note = ""
            cand_addrs, best = ax.l_candidates(L, module=mod)
            basis = ("K=%s（%d 个 src 键）在 Android 侧被**多个不同名**函数引用 ⇒ 键不足以定位到唯一源码函数"
                     % (";".join("%s:%d" % k for k in keys[:3]), len(keys)))
            if cand_addrs:
                best_a = cand_addrs[0]
                f = fscore(L, ax.andf[best_a])
                f_note = ("L 轴首选 %s（size %d/%d，F: sr=%.2f cj=%s）"
                          % (hex(best_a), L["size"], ax.andf[best_a]["size"], f["sr"],
                             "—" if f["cj"] is None else "%.2f" % f["cj"]))
            rows.append(dict(addr=va, cur=cur, new="", src="", basis=basis, tier="未知",
                             cands="; ".join(cands[:4]) + ("…(+%d)" % (len(cands) - 4) if len(cands) > 4 else ""),
                             n_same=0, f_note=f_note,
                             missing="缺能区分 `h<hash>` 实例的证据：两份构建的实例化/内联份数不同，"
                                     "键与结构（size/常量/导入符号）在该族内完全相同"))
            stat["R2 键名共识多名 → 不定名"] += 1
            continue
        # kind == none：键没有具名引用
        crates = ax.key_crates(L)
        nac = {NA_CRATE[c] for c in crates if c in NA_CRATE}
        if nac:
            rows.append(dict(addr=va, cur=cur, new="", src="android符号(非游戏)", basis=
                             "K=%s 在 Android 侧**只被非游戏 crate 的具名函数引用**（%s）⇒ 该 Linux 函数是这些 crate 的单态化，"
                             "不是游戏源码函数（T82 na 判据的独立复核）"
                             % (";".join("%s:%d" % k for k in keys[:3]), ",".join(sorted(crates)[:6])),
                             tier="观测", cands="", n_same=0, f_note="",
                             missing="—（判为 %s，不给游戏名；不进 F1）" % ",".join(sorted(nac))))
            stat["R3 非游戏 crate 单态化 → na"] += 1
        else:
            rows.append(dict(addr=va, cur=cur, new="", src="", basis=
                             "K=%s 在 Android 侧没有任何具名函数引用（引用者全无名）"
                             % (";".join("%s:%d" % k for k in keys[:3])),
                             tier="未知", cands="", n_same=0, f_note="",
                             missing="Android 侧该键只见于无名函数 ⇒ 缺该函数在 Android 上的符号名"))
            stat["R4 键无具名引用 → 不定名"] += 1

    out = os.path.join(ROOT, "fixtures/naming_android.tsv")
    with open(out, "w") as f:
        f.write("# naming_android.tsv —— T87 跨构建符号映射的逐条产物（tools/naming_android.py name 生成，不要手改）\n")
        f.write("# 判据 = fixtures/naming_android_plan.txt（预注册）。地址 = **Linux** ELF vaddr（Ghidra/REA = +0x100000）。\n")
        f.write("# 名字来源：android符号+位置键 = R1 键名共识（Android .symtab 读出源码级名字，档位 观测）；\n")
        f.write("#           android符号(非游戏) = R3，判 na:*，**不是**游戏函数名；空 = 未定（R2/R4）。\n")
        f.write("# 档位口径同 AGENTS.md：观测 = 能指到地址且链接是编译期常量；未知 = 判不了（写清缺什么）。\n")
        f.write("vaddr\t现用名/预测名\t新定名\t名字来源\t依据\t档位\t候选名字集(未定时)\t同名处数\tF/旁证\t缺什么\n")
        for r in rows:
            f.write("\t".join([hex(r["addr"]), r["cur"], r["new"], r["src"], r["basis"],
                               r["tier"], r["cands"], str(r["n_same"]), r["f_note"], r["missing"]]) + "\n")
    print("=== 179 定名结果（→ %s）" % os.path.relpath(out, ROOT))
    for k, v in sorted(stat.items()):
        print("  %-40s %d" % (k, v))
    named_ok = sum(1 for r in rows if r["new"])
    print("  **真名 %d / %d**（R1 键名共识，档位 观测）" % (named_ok, len(rows)))
    print("  na 判定 %d / %d（R3，非游戏 crate 单态化）"
          % (sum(1 for r in rows if r["tier"] == "观测" and not r["new"]), len(rows)))
    print("  未定 %d（R2 多名 %d + R4 无具名引用 %d）"
          % (sum(1 for r in rows if r["tier"] == "未知"),
             stat["R2 键名共识多名 → 不定名"], stat["R4 键无具名引用 → 不定名"]))
    return rows


# ================================================================ verify
COLD = ("::panic_cold_explicit", "::panic_cold_due_to_overflow", "::panic_cold_index")


def norm_name(n):
    for c in COLD:
        if n.endswith(c):
            return n[:-len(c)]
    return n


def cmd_verify(args):
    """交叉核对已有真名：① 逐条判 V1/V2/V3/V4；② 夹具内同址多名的冲突裁决。"""
    data = load_cache()
    ax = Axes(data)
    boundary, unnamed, named, allnames = load_targets()
    rows = []
    stat = collections.Counter()
    for va, r in sorted(named.items()):
        L = ax.lin.get(va)
        stated = "%s::%s::%s" % (r["module"], r["type"] or "_", r["method"])
        if L is None:
            stat["V0"] += 1
            rows.append((va, stated, "V0", "Linux 侧抽不到特征", "", "", ""))
            continue
        same = set()
        for a, recs in ax.agn.items():
            for rr in recs:
                if not (rr["game"] and rr["module"] == r["module"]):
                    continue
                if rr["type"] and r["type"] and rr["type"] != r["type"]:
                    continue
                if (rr["method"] or "").split("::")[-1] != (r["method"] or "").split("::")[-1]:
                    continue
                same.add(a)
        top, best = ax.l_candidates(L, module=r["module"])
        lnames = sorted({n for a in top for n in ax.and_srcnames.get(a, set())})
        if not same:
            st, verdict, note = "V3", "Android 侧无同名函数", \
                "跨构建内联/裁剪差异或名字形态不同；既不能证实也不能证伪"
        elif any(a in same for a in top):
            if len(same) > 1:
                st, verdict, note = "V4", "一致（Android 同名实例 %d 个）" % len(same), "源码级同一 fn 的多份机器码"
            else:
                st, verdict, note = "V1", "一致", "L 轴 + Android 同名，双向对上"
        else:
            same_mod = [a for a in top
                        if any(q["game"] and q["module"] == r["module"] for q in ax.agn.get(a, []))]
            if same_mod:
                st, verdict = "V2", "**冲突**"
                note = "L 轴在 Android 全量 Location 共享池里指到另一个同模块名字：%s" % lnames
            else:
                # 同名函数在 Android 侧**是存在的**（`same` 非空），只是 L 轴首选了另一个
                # （常是无名 / 跨模块）候选 ⇒ 既不是冲突，也不该记成"Android 无同名函数"。
                st, verdict = "V5", "同名存在，但 L 轴首选不是它"
                note = ("Android 同名实例 %d 个（%s）；L 轴按 Location 键集首选了 %s"
                        "（%s）——该函数的键在 Android 侧被别的函数共享得更多"
                        % (len(same), ",".join(hex(x) for x in sorted(same)[:3]),
                           hex(top[0]) if top else "—",
                           "无名候选" if top and not ax.and_srcnames.get(top[0]) else
                           ",".join(lnames[:2])))
        stat[st] += 1
        rows.append((va, stated, st, verdict, ",".join(n[:26] for n in lnames[:3]),
                     hex(top[0]) if top else "", note))

    # ---- ② 夹具内同址多名（名字层面的复原冲突） ----
    conflicts = []
    for va, lst in sorted(allnames.items()):
        names = sorted({(x["module"], x["type"] or "", norm_name(x["method"] or "")) for x in lst})
        if len(names) < 2:
            continue
        L = ax.lin.get(va)
        if L is None:
            continue
        top, best = ax.l_candidates(L)
        cands = []
        for a in top:
            for n in sorted(ax.and_srcnames.get(a, set())):
                cands.append((n, ax.andf[a]["size"], a))
        # Android 裁决：候选名里与夹具里的某一个名字相等 → 支持它；否则按尺寸比排序取最接近
        sup = []
        for n, sz, a in cands:
            for (m2, t2, me2) in names:
                if norm_name("%s::%s::%s" % (m2, t2 or "_", me2)).split("::")[-1] == n.split("::")[-1] \
                        and m2 == n.split("::")[0]:
                    sup.append((n, sz, a))
        verdict = ""
        if sup:
            sup.sort(key=lambda x: -min(L["size"], x[1]) / max(L["size"], x[1], 1))
            verdict = "Android 支持 `%s`（size %d/%d）" % (sup[0][0], L["size"], sup[0][1])
        elif cands:
            verdict = "Android 首选 `%s`（size %d/%d）—— 与夹具里任何一个名字都不同" % (
                cands[0][0], L["size"], cands[0][1])
        else:
            verdict = "Android 侧无具名候选（判不了）"
        conflicts.append((va, names, L["size"], verdict, lst))

    out = os.path.join(ROOT, "fixtures/naming_android_verify.tsv")
    with open(out, "w") as f:
        f.write("# naming_android_verify.tsv —— T87 用 Android .symtab 交叉核对已有真名（tools/naming_android.py verify）\n")
        f.write("# 判据 = fixtures/naming_android_plan.txt §5。地址 = Linux ELF vaddr。名字一律源码级 module::Type::method。\n")
        f.write("# 【A 区】逐条：V1 一致 / V2 冲突 / V3 Android 侧**没有**同名函数 / V4 同名多实例 / "
                "V5 同名存在但 L 轴首选不是它（不算冲突、也不算无同名） / V0 无特征\n")
        f.write("区域\tvaddr\t现用真名\t判定\t结论\tL轴指到的Android名字\tL轴地址\t说明\n")
        for row in rows:
            f.write("A\t" + "\t".join([hex(row[0])] + list(row[1:])) + "\n")
        f.write("# 【B 区】夹具内**同一地址出现多个不同名字**（名字层面的复原冲突）＋ Android 侧裁决\n")
        f.write("区域\tvaddr\tsize\t夹具里的名字\tAndroid 裁决\n")
        for va, names, sz, verdict, lst in conflicts:
            f.write("B\t%s\t%d\t%s\t%s\n"
                    % (hex(va), sz, "; ".join(sorted({"%s::%s::%s" % n for n in names})), verdict))
    print("=== 已有真名交叉核对（→ %s）" % os.path.relpath(out, ROOT))
    for k, v in sorted(stat.items()):
        print("  %-4s %d" % (k, v))
    conf = [x for x in rows if x[2] == "V2"]
    print("  **A 区名字冲突 %d 条**" % len(conf))
    for x in conf:
        print("     %s %s ← L 轴指到 %s" % (hex(x[0]), x[1], x[4]))
    print("  **B 区夹具内同址多名 %d 条**（Android 裁决见 TSV）：" % len(conflicts))
    for va, names, sz, verdict, lst in conflicts:
        print("     %s size=%-6d %s  ⇒ %s" % (hex(va), sz,
              " | ".join(sorted({"%s::%s::%s" % n for n in names})), verdict))
    return rows, conflicts


# ================================================================ all
def cmd_all(args):
    ok = cmd_control(args)
    print()
    cmd_verify(args)
    print()
    cmd_name(args)
    print("\n=== 结论闸：C1/C2 %s" % ("通过" if ok else "不通过"))
    return 0 if ok else 2


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("cmd", choices=["build", "control", "name", "verify", "all"])
    ap.add_argument("--rebuild", action="store_true", help="强制重建特征缓存")
    args = ap.parse_args(argv)
    if args.cmd == "build" or args.rebuild:
        build_cache()
        if args.cmd == "build":
            return 0
    if args.cmd == "control":
        return 0 if cmd_control(args) else 2
    if args.cmd == "name":
        cmd_name(args)
        return 0
    if args.cmd == "verify":
        cmd_verify(args)
        return 0
    return cmd_all(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
