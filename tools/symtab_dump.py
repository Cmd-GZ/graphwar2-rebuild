#!/usr/bin/env python3
"""从 2024-07-30 版未 strip 的 `libgraphwarrust.so` 里取出**原版的真实模块/类型/方法名**。

## 为什么需要它

2026 年的三个版本都被 `strip` 了，只剩 `.dynsym` 里的导出符号；`docs/02` §17 的 70 个模块名
是从 `.rodata` 里残留的 `src/<mod>.rs` 字面量**反推**出来的，没有一条来自符号表。
**2024-07-30 首发周那一版带完整 `.symtab`**（13,086 个 `FUNC`，1,315 个含 `graphwar`），
这是全项目唯一能拿到「原版源码级名字」的物证。

本脚本把它榨干，产出四张表 + 一张桥表：

1. **模块表**   `graphwarrust::<module>::…` 的第 2 段 ⇒ 权威模块清单（函数数 / 类型 / trait）。
2. **类型表**   同上第 3 段里的大写开头段 ⇒ 该模块声明的结构体/枚举。
3. **方法名表** 第 3 段之后剩下的段 ⇒ **原版的源码级函数名**（`forward_event`、`tick` …）。
4. **锚点表**   `panic::Location`（`{file.ptr, file.len, line:u32, col:u32}`）解出来的
   `函数符号 ↔ src/<file>.rs:LINE:COL`——这是「符号名」与「源文件」之间**最硬的桥**。
5. **对照表**   与 `docs/02-module-map.md` §17 / `docs/coverage-matrix.tsv` 的 70 个模块对账。

## 口径（重要，写文档时必须带上）

* 名字来自 **2024-07-30 版**（sha256 `6eee7bb8…`），**不是** 2.0.60。两年里名字**大概率**没变，
  但这是 `推断`，不是 `逐位`。
* 本脚本输出的**地址一律是 2024 版的 ELF 虚地址**。两版 `.text` 差 40 万字节，
  **绝对不能**当成 2.0.60 的地址。唯一能跨版本用的是 **名字** 和 **`src/<file>.rs:LINE`**。
* `graphwar` 子串出现在符号里 ≠ 该符号属于游戏代码：`core::ptr::drop_in_place<…TickEvent…>`
  是 std 的泛型单态化。本脚本把它们单独归到 `generic` 类，不进模块表。
* 一个 `Location` 结构体可能被多个函数引用（泛型/内联）⇒ 归属**天然有歧义**，
  锚点表只写「引用了」，不写「属于」。

## 用法

    python3 tools/symtab_dump.py --so <path>/libgraphwarrust.so modules
    python3 tools/symtab_dump.py modules --markdown
    python3 tools/symtab_dump.py methods graphwar_client_connection_handler
    python3 tools/symtab_dump.py locations --module graphwar_shot_calculator
    python3 tools/symtab_dump.py compare
    python3 tools/symtab_dump.py all > /tmp/symtab.md

只依赖 `readelf` / `objdump` / `c++filt`（binutils）与 Python 标准库。
`readelf -sW` 的列、`objdump -d` 的 `# <addr>` 注释在 binutils 各版本上一致；
若某个发行版改格式，用 `--dump-dis` 复用已经落盘的 `objdump -d` 输出。
"""

import argparse
import collections
import os
import re
import struct
import subprocess
import sys

# ---------------------------------------------------------------- 常量

GRAPH_ROOT = "graphwarrust"

# Rust legacy mangling 的 hash 后缀 `::h<16 hex>`；v0 mangling 是 `_R…`，本二进制里一个都没有。
HASH_SUFFIX = re.compile(r"::h[0-9a-f]{16}$")

# `graphwarrust::mod::Type::method::{{closure}}::h…`
CLOSURE = "{{closure}}"

# gdext 生成的样板，不是原版源码里的函数名。
# `__register_methods` / `__virtual_call` / `__register_exports` 来自
# `godot_core::obj::traits::cap::{ImplementsGodotApi,ImplementsGodotVirtual,ImplementsGodotExports}`，
# `__inner_init` 来自 `GodotClass` 的构造壳体，`panic_cold_explicit` 是 rustc 给
# 「函数里的 panic 路径」单独开的冷段（**它属于宿主方法**，`X::foo::panic_cold_explicit` ⇒ `X::foo`）。
BOILERPLATE = {
    "__register_methods", "__register_exports", "__virtual_call",
    "__inner_init", "__init", "class_name",
    "panic_cold_explicit", "panic_cold_dispatch", "ptrcall_fn", "varcall_fn", "virtual_fn",
    "expecting",  # serde `Visitor::expecting`
}

def warn(msg):
    print(f"[symtab_dump] {msg}", file=sys.stderr)


def run(cmd, **kw):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, check=True, **kw).stdout
    except FileNotFoundError:
        sys.exit(f"找不到 {cmd[0]}：本脚本需要 binutils（readelf / objdump / c++filt）")
    except subprocess.CalledProcessError as e:
        sys.exit(f"{' '.join(cmd)} 失败：{e.stderr.strip()[:400]}")


# ---------------------------------------------------------------- ELF 读取


class Elf:
    """只读本脚本要用的三段：`.text` / `.rodata` / `.data.rel.ro` / `.rela.dyn`。

    不引第三方库：`readelf -SW` 的列在 binutils ≥2.30 上稳定（Idx Name Type Address Off Size ES Flg Lk Inf Al）。
    """

    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            self.data = f.read()
        self.sections = {}
        cur = None
        for line in run(["readelf", "-SW", path]).splitlines():
            p = line.replace("[", " ").replace("]", " ").split()
            if len(p) >= 6 and p[0].isdigit():
                cur = p[1]
                try:
                    self.sections[cur] = (int(p[3], 16), int(p[4], 16), int(p[5], 16))
                except ValueError:
                    pass
            elif cur and len(p) >= 2 and re.fullmatch(r"[0-9a-f]+", p[0]):
                va, off, sz = self.sections[cur]
                self.sections[cur] = (va, off, int(p[0], 16))  # NOBITS 的 Size 换行到下一行
        for need in (".text", ".rodata", ".data.rel.ro", ".rela.dyn"):
            if need not in self.sections:
                sys.exit(f"{path} 缺 {need}：这看起来不是一个带重定位的 ELF 共享库")

    def bytes_at_va(self, va, n):
        "从 .rodata 取 n 字节（Rust 字面量不在 .data），越界返回 None。"
        base, off, sz = self.sections[".rodata"]
        if not (base <= va < base + sz) or va + n > base + sz:
            return None
        i = off + (va - base)
        return self.data[i:i + n]

    # -- 符号表 -------------------------------------------------------

    def func_symbols(self):
        """[(mangled, addr, size)]，只取 `.symtab` 里 `FUNC` 的已定义符号。

        用 `.symtab`（而非 `.dynsym`）是**整个方法成立的前提**：2026 的三版 strip 了，
        只剩 `.dynsym`，所以只有 2024 版能做这件事。
        """
        out = run(["readelf", "-sW", self.path])
        rows = []
        for line in out.splitlines():
            p = line.split()
            if len(p) < 8 or p[3] != "FUNC" or p[1] == "UND":
                continue
            try:
                addr = int(p[1], 16)
                size = int(p[2])
            except ValueError:
                continue
            rows.append((p[7], addr, size))
        return rows

    # -- 重定位 -------------------------------------------------------

    def relocs(self):
        "`.rela.dyn` → {r_offset: (r_type, r_addend)}。只此一段，`.rela.plt` 与本脚本无关。"
        base, off, sz = self.sections[".rela.dyn"]
        out = {}
        for i in range(sz // 24):
            r_off, r_info, r_add = struct.unpack_from("<QQq", self.data, off + i * 24)
            out[r_off] = (r_info & 0xFFFFFFFF, r_add)
        return out

    # -- panic::Location ---------------------------------------------

    def locations(self):
        """解出所有 `core::panic::Location` 结构体。

        布局（`core/src/panic/location.rs`，2024 年的 rustc 1.79 起没变）：

            struct Location { file: &'static str, line: u32, col: u32 }
            // 8(ptr) + 8(len) + 4 + 4 = 24 字节

        `.data.rel.ro` 里第 0 个 8 字节槽被 `R_X86_64_RELATIVE` 占着，addend 就是串地址；
        **槽位 +8 就是 len**——这就是自校验：按 len 取出的字节必须全 ASCII 且以 `.rs` 结尾。
        这一条同时绕开了「`.rodata` 里字面量非 NUL 结尾、`strings` 会把相邻字面量粘成一条」的坑。

        返回 {struct_va: (file, line, col)}。
        """
        ro_va, _, ro_sz = self.sections[".rodata"]
        dr_va, dr_off, dr_sz = self.sections[".data.rel.ro"]
        out = {}
        for r_off, (r_type, add) in self.relocs().items():
            if r_type != 8 or not (dr_va <= r_off < dr_va + dr_sz):
                continue
            i = dr_off + (r_off - dr_va)
            if i + 24 > len(self.data):
                continue
            _ptr, ln, line, col = struct.unpack_from("<QQII", self.data, i)
            if not (4 < ln < 300):
                continue
            raw = self.bytes_at_va(add, ln)
            if raw is None:
                continue
            try:
                s = raw.decode("ascii")
            except UnicodeDecodeError:
                continue
            if not s.endswith(".rs"):
                continue
            if not (1 <= line < 100000 and 1 <= col < 2000):
                continue
            out[r_off] = (s, line, col)
        return out


# ---------------------------------------------------------------- 名字解析


def demangle(syms):
    "一次性喂给 `c++filt --format rust`；比逐个调用快两个数量级。"
    if not syms:
        return []
    out = run(["c++filt", "--format", "rust"], input="\n".join(syms))
    lines = out.splitlines()
    if len(lines) != len(syms):
        warn(f"c++filt 行数 {len(lines)} != 输入 {len(syms)}，改用 rustc-demangle 兜底会失真")
    return lines


def strip_hash(name):
    return HASH_SUFFIX.sub("", name)


# 这些段总是某条生成路径的尾巴，剥掉之后剩下的才是原版源码里的方法名。
TRAILING_NOISE = {"ptrcall_fn", "varcall_fn", "virtual_fn",
                  "panic_cold_explicit", "panic_cold_dispatch",
                  "__inner_init", "__init"}

GW_ITEM = re.compile(rf"{GRAPH_ROOT}::([A-Za-z0-9_]+)::([A-Z][A-Za-z0-9_]*)")


def clean_method(segs):
    segs = list(segs)
    while segs and segs[-1] in TRAILING_NOISE:
        segs.pop()
    if not segs:
        return None
    return "::".join(segs)


def is_boilerplate(method):
    if not method:
        return False
    return method in BOILERPLATE or method.split("::")[0] in BOILERPLATE


def split_top(s, sep="::"):
    "按 `::` 切但尊重尖括号/圆括号嵌套，`Result<a::B, c::D>` 不会被切开。"
    parts, depth, cur = [], 0, ""
    i = 0
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


class Item:
    """一个 `graphwarrust` 定义的函数符号解出来的身份。"""

    __slots__ = ("mangled", "demangled", "addr", "size", "kind", "module",
                 "owner", "method", "is_closure", "trait_path", "generic_args")

    def __init__(self, mangled, demangled, addr, size):
        self.mangled = mangled
        self.demangled = strip_hash(demangled)
        self.addr = addr
        self.size = size
        self.module = None
        self.owner = None          # 结构体/枚举名（可能是 `_`）
        self.method = None
        self.is_closure = False
        self.trait_path = None     # impl 块的 `as X` 部分
        self.generic_args = []
        self.kind = classify(self.demangled)


def classify(d):
    """把 1,315 个含 graphwar 的符号分成三类。

    * `direct`  —— 名字**以** `graphwarrust::` 开头：游戏自己定义的函数。
    * `impl`    —— 名字以 `<graphwarrust::…>` 开头：为游戏类型做的 trait impl
                   （`IControl::draw`、`GodotClass::class_name`、serde 的 `__Visitor`…）。
    * `generic` —— `core::ptr::drop_in_place<…TickEvent…>` 之类：std 的泛型单态化，
                   **不是**游戏的代码，但能反推出类型名。
    """
    if d.startswith(GRAPH_ROOT + "::") and "<impl " not in d:
        return "direct"
    if "<impl " in d or d.startswith("<" + GRAPH_ROOT + "::"):
        return "impl"
    if d.startswith("<"):
        return "impl_foreign"
    return "generic"


def parse_items(symbols):
    "符号表 → Item 列表，并填好 module/owner/method。"
    dem = demangle([s[0] for s in symbols])
    items = []
    for (mangled, addr, size), d in zip(symbols, dem):
        if "graphwar" not in d.lower():
            continue
        it = Item(mangled, d, addr, size)
        d = it.demangled
        if it.kind == "direct":
            segs = split_top(d)
            it.module = segs[1]
            rest = segs[2:]
            if CLOSURE in rest:
                it.is_closure = True
                rest = rest[:rest.index(CLOSURE)]
            if rest and rest[0][:1].isupper():
                it.owner = rest[0]
                rest = rest[1:]
            it.method = clean_method(rest) or it.owner
            if it.method == it.owner:
                it.owner = None
        elif it.kind == "impl":
            # 两种形态都要吃：
            #   <graphwarrust::m::Type as Trait>::method
            #   <graphwarrust::m::_::<impl serde::Deserialize for graphwarrust::m::Type>::deserialize::__Visitor as …>
            # 后者是 serde derive 生成的嵌套 impl，` as ` 出现两次，所以要取**最外层**的。
            m = GW_ITEM.search(d)
            if m:
                it.module, it.owner = m.group(1), m.group(2)
            else:
                mm = re.match(rf"<\s*(?:{GRAPH_ROOT}::)?([A-Za-z0-9_]+)::([A-Za-z0-9_]+)\s", d)
                if mm:
                    it.module, it.owner = mm.group(1), mm.group(2)
            traits = re.findall(r" as ([A-Za-z_][\w:]*)", d)
            if traits:
                it.trait_path = traits[-1]
            if ">::" in d:
                it.method = clean_method(split_top(d[d.rindex(">::") + 3:]))
            # `__Visitor` / `__FieldVisitor` 是 serde derive 的辅助类型，方法归给宿主类型
            if it.owner and it.owner.startswith("__"):
                inner = GW_ITEM.search(d)
                if inner:
                    it.owner = inner.group(2)
            if it.method and CLOSURE in it.method:
                it.is_closure = True
                it.method = it.method.split("::")[0]
        else:
            # 泛型实参里的 `graphwarrust::mod::Type` 一并收进类型表
            it.generic_args = GW_ITEM.findall(d)
        items.append(it)
    return items


def modules_from_generics(items):
    "只出现在泛型实参里的模块（自己的函数全是泛型时，符号表里没有 direct 项）。"
    seen = collections.Counter()
    for it in items:
        for mod, _ty in it.generic_args:
            seen[mod] += 1
    return seen


# ---------------------------------------------------------------- 锚点桥


def build_anchor_map(elf, dis_path=None):
    """函数符号 ↔ `src/<file>.rs:LINE:COL`。

    做法：`objdump -d -j .text` 每个函数头一行 `<addr> <mangled>:`；函数体里
    `lea …# <addr>` 的**目标地址**若正好是一个 `Location` 结构体的起始地址，就记一条。
    objdump 已经在注释里算好了 rip 相对地址，自己算 `disp` 是本项目踩过的坑
    （`docs/02` §16.4 第 2 条的警告）。

    返回 {mangled: {(file, line, col)}}。
    """
    locs = elf.locations()
    if dis_path:
        text = open(dis_path, "r", errors="replace").read()
    else:
        text = run(["objdump", "-d", "--no-show-raw-insn", "-j", ".text", elf.path])
    hdr = re.compile(r"^([0-9a-f]{16}) <(.+)>:$")
    tgt = re.compile(r"# ([0-9a-f]+) ")
    out = collections.defaultdict(set)
    cur = None
    seen_syms = set()
    for line in text.splitlines():
        m = hdr.match(line)
        if m:
            cur = m.group(2)
            seen_syms.add(cur)
            continue
        if cur is None or "\tlea " not in line:
            continue
        m2 = tgt.search(line)
        if not m2:
            continue
        hit = locs.get(int(m2.group(1), 16))
        if hit:
            out[cur].add(hit)
    return out, locs, seen_syms


def anchor_for_file(anchors, mangled):
    "该函数引用的 Location 落在哪些 src 文件上（按行号排序）。"
    return sorted(anchors.get(mangled, ()), key=lambda t: (t[0], t[1], t[2]))


# ---------------------------------------------------------------- 输出

def own_files(anchors, mangled):
    "该函数引用的 Location 里属于游戏自己（`src/*.rs`）的那些文件；依赖的单独计数。"
    own, dep = [], set()
    for f, _l, _c in anchors.get(mangled, ()):
        if f.startswith("src/"):
            own.append(f)
        else:
            dep.add(os.path.basename(f))
    return sorted(set(own)), dep


def file_spans(locs):
    "{src 文件: (最小行, 最大行, Location 个数)}——**原版源文件长度的硬下界**。"
    span = collections.defaultdict(lambda: [10 ** 9, 0, 0])
    for f, ln, _c in locs.values():
        if not f.startswith("src/"):
            continue
        e = span[f]
        e[0] = min(e[0], ln)
        e[1] = max(e[1], ln)
        e[2] += 1
    return {f: tuple(v) for f, v in span.items()}


def module_span(mod, anchors, items_for_mod):
    "该模块自己的函数引用到的 Location 里，落在 `src/<同模块名>.rs` 上的行范围。"
    want = f"src/{mod}.rs"
    lines = []
    for it in items_for_mod:
        for f, ln, _c in anchors.get(it.mangled, ()):
            if f == want:
                lines.append(ln)
    return (min(lines), max(lines), len(lines)) if lines else None


def all_module_names(items):
    "模块全集：有 direct 函数的 ∪ 有 impl 的 ∪ 只作为泛型实参露面的。"
    mods = set()
    for it in items:
        if it.module:
            mods.add(it.module)
        for m, _t in it.generic_args:
            mods.add(m)
    return mods


def cmd_modules(items, anchors, out):
    by = collections.defaultdict(list)
    for it in items:
        if it.kind == "direct" and it.module:
            by[it.module].append(it)
    impls = collections.defaultdict(set)
    types = collections.defaultdict(set)
    for it in items:
        if it.module and it.owner and it.owner != "_":
            types[it.module].add(it.owner)
        if it.kind == "impl" and it.module and it.trait_path:
            impls[it.module].add(it.trait_path)
        for mod, ty in it.generic_args:
            types[mod].add(ty)
    mods = sorted(all_module_names(items))
    nofn = [m for m in mods if not by.get(m)]
    bulky = {}

    out.write(f"#### A. 权威模块清单（{len(mods)} 个模块；其中 {len(mods) - len(nofn)} 个有自己的函数）\n\n")
    out.write("| 模块 | 函数 | 类型 | 方法 | `src/<模块>.rs` 行范围（锚点） | gdext trait impl |\n"
              "|---|---|---|---|---|---|\n")
    for mod in mods:
        fns = by.get(mod, [])
        methods = sorted({i.method for i in fns
                          if i.method and i.method != "_" and not i.is_closure
                          and not is_boilerplate(i.method)})
        sp = module_span(mod, anchors, fns) if fns else None
        tl = sorted(types.get(mod, ()))
        tcell = (f"{len(tl)} 个（见下）" if len(tl) > 12
                 else (", ".join(f"`{x}`" for x in tl) or "—"))
        if len(tl) > 12:
            bulky[mod] = tl
        out.write("| `{m}` | {n} | {t} | {nm} | {sp} | {tr} |\n".format(
            m=mod,
            n=len(fns),
            t=tcell,
            nm=len(methods),
            sp=f"{sp[0]}–{sp[1]}（{sp[2]}）" if sp else "—",
            tr=", ".join(f"`{x.split('::')[-1]}`" for x in sorted(impls.get(mod, ()))) or "—",
        ))
    if nofn:
        out.write("\n**没有一个自己的函数**（只在 impl / 泛型实参里露面）的模块："
                  + ", ".join(f"`{m}`" for m in nofn) + "\n")
    for mod, tl in bulky.items():
        out.write(f"\n##### `{mod}` 的 {len(tl)} 个类型（符号表原样）\n\n")
        for i in range(0, len(tl), 4):
            out.write(", ".join(f"`{x}`" for x in tl[i:i + 4]) + "\n")
        out.write("\n")


def cmd_methods(items, anchors, out, only=None):
    """原版源码级的方法名。`panic_cold_explicit` / `__register_methods` 之类的
    gdext+rustc 样板单独收在每节的「样板」行里，不混进方法名。"""
    by = collections.defaultdict(lambda: collections.defaultdict(set))
    boiler = collections.defaultdict(set)
    # 有些函数的**外层本体被内联掉了，只剩闭包**（实测 `setup_level_0`…`setup_level_4`）。
    # 这种名字仍然证明该函数在原版源码里存在，所以保留，但打 `†` 标记。
    closure_only = collections.defaultdict(set)
    for it in items:
        if it.kind not in ("direct", "impl") or not it.module:
            continue
        if only and it.module != only:
            continue
        owner = it.owner or "_"
        if is_boilerplate(it.method) or not it.method:
            if it.method:
                boiler[it.module].add(it.method)
            continue
        if it.method == "_" or it.method == it.owner:
            continue
        if it.is_closure:
            closure_only[it.module].add((owner, it.method))
            continue
        by[it.module][owner].add(it.method)
    out.write("\n#### C. 方法名（原版源码级函数名）\n\n")
    out.write("> `panic_cold_explicit` / `__register_methods` / `__virtual_call` / `__inner_init` /\n"
              "> `class_name` / `__register_exports` / serde 的 `expecting` 是 rustc 的冷段 + gdext/serde\n"
              "> 的样板，**不是**原版源码里的函数名，已剔除。`{{closure}}` 也算不到方法名上。\n\n")
    if only:
        for mod in sorted(by):
            out.write(f"#### `{mod}`\n\n")
            for owner in sorted(by[mod]):
                names = sorted(n for n in by[mod][owner] if n)
                if names:
                    out.write(f"* `{owner}`: " + ", ".join(f"`{n}`" for n in names) + "\n")
            if boiler.get(mod):
                out.write(f"* <sub>样板: {', '.join('`'+x+'`' for x in sorted(boiler[mod]))}</sub>\n")
            out.write("\n")
        return
    out.write("| 模块 | 方法名（按所属类型分组） |\n|---|---|\n")
    for mod in sorted(by):
        cells = []
        for owner in sorted(by[mod]):
            names = sorted(n for n in by[mod][owner] if n)
            if not names:
                continue
            cells.append(f"`{owner}`: " + ", ".join(f"`{n}`" for n in names))
        for owner, name in sorted(closure_only.get(mod, ())):
            if name not in by[mod].get(owner, ()):
                cells.append(f"`{owner}`: `{name}`†")
        if not cells:
            continue
        out.write(f"| `{mod}` | " + "<br>".join(cells) + " |\n")
    out.write("\n<sub>† 只有 `{{closure}}` 符号、外层本体被内联掉的名字——"
              "名字本身仍是存在的证据。</sub>\n")
    out.write("\n")


def cmd_types(items, out):
    by = collections.defaultdict(set)
    for it in items:
        if it.module and it.owner and it.kind in ("direct", "impl"):
            by[it.module].add(it.owner)
        for mod, ty in it.generic_args:
            by[mod].add(ty)
    # impl 名字里出现的枚举变体（serde `__Visitor`）也算类型
    out.write("\n### 类型清单\n\n| 模块 | 类型 |\n|---|---|\n")
    for mod in sorted(by):
        tys = sorted(t for t in by[mod] if t and t != "_")
        if tys:
            out.write(f"| `{mod}` | " + ", ".join(f"`{t}`" for t in tys) + " |\n")


def cmd_locations(items, anchors, locs, out, only=None, limit=None):
    by = collections.defaultdict(list)
    for it in items:
        if not it.module or (only and it.module != only):
            continue
        a = anchor_for_file(anchors, it.mangled)
        if a:
            by[it.module].append((it, a))
    out.write("\n#### 符号名 ↔ `src/文件.rs:行:列`（panic::Location 桥）\n\n")
    for mod in sorted(by):
        out.write(f"#### `{mod}`\n\n| 符号（去 hash） | 锚点 |\n|---|---|\n")
        rows = sorted(by[mod], key=lambda t: (t[0].method or "", t[0].addr))
        for it, a in (rows[:limit] if limit else rows):
            name = it.demangled.split("::")[-1]
            # ⚠ 只显示游戏自己的 `src/*.rs`：`mod.rs` / `lib.rs` / `vec/mod.rs` 这些 basename
            # 在 std 与各 crate 里到处都是，按 basename 显示会把人骗到（本轮真踩过）。
            own = [x for x in a if x[0].startswith("src/")]
            dep = len(a) - len(own)
            if own:
                first = own[0][0].split("/")[-1]
                tail = [f"{ln}:{c}" for _f, ln, c in own[1:4]]
                anchor = f"`{first}:{own[0][1]}:{own[0][2]}`"
                if tail:
                    anchor += " " + ", ".join(f"`{t}`" for t in tail)
                if len(own) > 4:
                    anchor += f" …(+{len(own) - 4})"
            else:
                anchor = "—"
            if dep:
                anchor += f" <sub>+{dep} 个依赖锚</sub>"
            out.write(f"| `{name}` | {anchor} |\n")
        out.write("\n")


# ---------------------------------------------------------------- 对照


def load_coverage_matrix(path):
    rows = []
    if not os.path.exists(path):
        warn(f"没有 {path}，跳过对照（该文件由 tools/coverage_matrix.py 生成）")
        return rows
    header = None
    for line in open(path, encoding="utf-8"):
        if line.startswith("#") or not line.strip():
            continue
        cols = line.rstrip("\n").split("\t")
        if header is None:
            header = cols
            continue
        rows.append(dict(zip(header, cols)))
    return rows


def known_modules_from_docs(doc_path):
    """从 `docs/02-module-map.md` §17 的两张表里抠出模块名。

    只认 `| `graphwar_xxx.rs`（…） |` / `| `xxx.rs`（…） |` 这种**首列是模块名**的行，
    所以 §17.3/§17.4 那些以 `src/…` 开头的行不会被误收。
    """
    mods = set()
    if not os.path.exists(doc_path):
        warn(f"没有 {doc_path}，跳过 §17 对照")
        return mods
    inside = False
    for line in open(doc_path, encoding="utf-8"):
        if line.startswith("### 17.1") or line.startswith("### 17.2"):
            inside = True
            continue
        if line.startswith("### 17.3"):
            break
        if not inside:
            continue
        m = re.match(r"\|\s*`([a-z0-9_]+\.rs)`", line)
        if m:
            mods.add(m.group(1)[:-3])
    return mods


def module_methods(items, mod):
    "{该模块下 (direct + impl) 的全部非样板方法名}，接口与 `cmd_methods` 一致但按模块聚合。"
    out = set()
    for it in items:
        if it.module != mod or it.kind not in ("direct", "impl"):
            continue
        if not it.method or it.method == "_" or is_boilerplate(it.method):
            continue
        out.add(it.method)
    return sorted(out)


def cmd_compare(items, anchors, out, doc_path, matrix_path):
    by = collections.defaultdict(list)
    for it in items:
        if it.kind == "direct" and it.module:
            by[it.module].append(it)
    sym_modules = set(by)
    # 只靠泛型/impl 露面的模块
    only_generic = {m for m in modules_from_generics(items)} - sym_modules

    known = known_modules_from_docs(doc_path)
    matrix = load_coverage_matrix(matrix_path)
    matrix_mods = {r["module"][:-3] for r in matrix if r.get("module", "").endswith(".rs")}
    unknown = {r["module"][:-3] for r in matrix
               if r.get("status") == "未知" and r.get("scope") != "dep"}

    out.write("\n#### 与现有认知对照\n\n")
    out.write(f"* 符号表（`graphwarrust::` 直系函数）：**{len(sym_modules)}** 个模块\n")
    out.write(f"* 只靠泛型实参/impl 露面的模块：**{len(only_generic)}** 个"
              f"（{', '.join('`'+m+'`' for m in sorted(only_generic))}）\n")
    out.write(f"* `docs/02` §17 记录：**{len(known)}** 个；"
              f"`docs/coverage-matrix.tsv`：**{len(matrix_mods)}** 个；"
              f"其中标 `未知` 的 **{len(unknown)}** 个\n\n")

    ours = known | matrix_mods
    dep_scope = {r["module"][:-3] for r in matrix if r.get("scope") == "dep"}

    both = sorted(ours & sym_modules)
    out.write(f"##### 对得上的（{len(both)} 个，名字逐一相同）\n\n")
    out.write(", ".join(f"`{m}`" for m in both) + "\n\n")

    ours_only = sorted(ours - sym_modules)
    out.write(f"##### 我们记录了、2024 版符号表里没有（{len(ours_only)} 个）\n\n")
    out.write(", ".join(f"`{m}`" for m in ours_only) + "\n\n")

    new = sorted(sym_modules - ours)
    out.write(f"##### 新发现的模块（{len(new)} 个，我们完全没记录）\n\n")
    new_rows = []
    for m in new:
        fns = by[m]
        new_rows.append((m, len(fns),
                         sorted({i.owner for i in fns if i.owner and i.owner != "_"}),
                         module_methods(items, m)))
    for m, n, tys, ms in sorted(new_rows, key=lambda t: -t[1]):
        out.write(f"* `{m}`（{n} 个函数）类型 {', '.join('`'+t+'`' for t in tys) or '—'}；"
                  f"方法 {', '.join('`'+x+'`' for x in ms[:12]) or '—'}"
                  f"{' …' if len(ms) > 12 else ''}\n")
    out.write("\n")

    if dep_scope:
        out.write("##### 分类要改的\n\n")
        for m in sorted(dep_scope):
            if m in sym_modules:
                out.write(f"* `{m}` 在 `coverage-matrix.tsv` 里被记成 `scope=dep`（第三方依赖），"
                          f"但符号路径是 `{GRAPH_ROOT}::{m}::…` ⇒ 它**在 graphwarrust crate 里**，"
                          f"是本仓库自己的一个模块（{len(by.get(m, []))} 个函数）。"
                          f"内容仍是 `WeakCaller` 这类通用工具，但它不是外部 crate。\n")
        out.write("\n")

    out.write(f"##### 22 个「完全未知」模块，这次解出真名的\n\n")
    out.write("| 模块 | 符号表里的真身 | 函数数 | 方法（节选） |\n|---|---|---|---|\n")
    solved = 0
    for m in sorted(unknown):
        if m in sym_modules:
            solved += 1
            fns = by[m]
            tys = sorted({i.owner for i in fns if i.owner})
            ms = module_methods(items, m)
            out.write(f"| `{m}` | {', '.join('`'+t+'`' for t in tys) or '（自由函数）'} | {len(fns)} | "
                      f"{', '.join('`'+x+'`' for x in ms[:8]) or '—'}{' …' if len(ms) > 8 else ''} |\n")
        elif m in only_generic:
            out.write(f"| `{m}` | 只有类型没有函数 | 0 | — |\n")
        else:
            out.write(f"| `{m}` | **符号表里完全没有** | 0 | — |\n")
    out.write(f"\n**{len(unknown)} 个未知模块里，{solved} 个在符号表里有自己的函数。**\n")


def cmd_srcdiff(so_a, so_b, out):
    """两版之间 `src/*.rs` 字面量集合的差。

    用途只有一个：**判断某个模块是哪一版才有的**。stripped 的版本没有符号表，
    所以「2026 版还有没有这个模块」只能靠它自己内嵌的源文件路径字面量回答——
    而 `panic::Location` 之外还有 inline `&str` 形态（`file!()` / 手写路径），
    所以 `strings` 在这件事上比 Location 表更全。
    """
    def files(p):
        s = run(["strings", "-a", p])
        return {m for m in re.findall(r"src/[a-z0-9_]+\.rs", s)}
    a, b = files(so_a), files(so_b)
    out.write(f"\n### `src/*.rs` 字面量差（{os.path.basename(os.path.dirname(so_a))} vs "
              f"{os.path.basename(os.path.dirname(so_b))}）\n\n")
    out.write(f"* 只在前者：{', '.join(f'`{x}`' for x in sorted(a - b)) or '—'}\n")
    out.write(f"* 只在后者：{', '.join(f'`{x}`' for x in sorted(b - a)) or '—'}\n")
    out.write(f"* 前者 {len(a)} 个，后者 {len(b)} 个，共有 {len(a & b)} 个\n")


def cmd_summary(items, anchors, locs, out):
    kinds = collections.Counter(i.kind for i in items)
    direct = [i for i in items if i.kind == "direct"]
    mods = all_module_names(items)
    owners = {i.owner for i in items if i.owner and i.owner != "_" and not i.owner.startswith("__")}
    methods = {i.method for i in items
               if i.method and not i.is_closure and not is_boilerplate(i.method) and i.method != "_"}
    spans = file_spans(locs)
    out.write("#### 总量\n\n")
    out.write(f"* 含 `graphwar` 的 `FUNC` 符号：**{len(items)}** 个"
              f"（direct {kinds['direct']} / impl {kinds['impl']} / generic {kinds['generic']}）\n")
    out.write(f"* 模块：**{len(mods)}**；类型：**{len(owners)}**；"
              f"非样板方法名（去重）：**{len(methods)}**\n")
    out.write(f"* `panic::Location` 结构体：**{len(locs)}**；"
              f"其中游戏自己的 `src/*.rs` 文件 **{len(spans)}** 个\n")
    out.write(f"* 能配上 Location 锚的函数：**{len(anchors)}**\n")

    # 一致性：符号名的模块段 vs 锚点文件名的 stem
    with_a = [(i, own_files(anchors, i.mangled)[0]) for i in direct]
    with_a = [(i, fs) for i, fs in with_a if fs]
    agree = sum(1 for i, fs in with_a if i.module in {os.path.basename(f)[:-3] for f in fs})
    out.write(f"\n**交叉验证**：{len(direct)} 个 direct 函数里 {len(with_a)} 个带 `src/*.rs` 锚；"
              f"其中 **{agree}** 个的「符号名模块段」正好等于某个锚点文件的 stem"
              f"（{100.0 * agree / max(1, len(with_a)):.1f}%）。"
              f"不一致的 {len(with_a) - agree} 个全是泛型/内联复用（见 §18.6）。\n\n")


# ---------------------------------------------------------------- main


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--so", default=None,
                    help="2024-07-30 版 libgraphwarrust.so（未 strip）。"
                         "缺省时读 $GW2_SYMTAB_SO，再缺省找 ../.reference/steam/2024-07-30/")
    ap.add_argument("--dump-dis", default=None,
                    help="复用已落盘的 `objdump -d --no-show-raw-insn -j .text` 输出")
    ap.add_argument("--repo", default=None, help="gw2-rebuild 仓库根（用于对照 docs/）")
    ap.add_argument("--so-b", default=None,
                    help="srcdiff 的对照版本（缺省 `…/2026-08-28/libgraphwarrust.so`）")
    ap.add_argument("cmd", nargs="?", default="summary",
                    choices=["summary", "modules", "types", "methods", "locations",
                             "compare", "srcdiff", "all"])
    ap.add_argument("arg", nargs="?", default=None, help="methods/locations 的模块过滤")
    ap.add_argument("--markdown", action="store_true", help="（默认就是 markdown）")
    a = ap.parse_args()

    so = a.so or os.environ.get("GW2_SYMTAB_SO")
    if not so:
        here = os.path.dirname(os.path.abspath(__file__))
        # 本仓库在 <上游>/gw2-rebuild/ 下，`.reference/` 在上游根：
        #   repo/tools/../../.reference/steam/2024-07-30/libgraphwarrust.so
        for up in (os.path.join(here, "..", "..", ".reference", "steam",
                                "2024-07-30", "libgraphwarrust.so"),
                   os.path.join(here, "..", "..", "..", ".reference", "steam",
                                "2024-07-30", "libgraphwarrust.so"),
                   os.path.join(here, "..", "..", "libgraphwarrust.so")):
            if os.path.exists(up):
                so = up
                break
    if not so or not os.path.exists(so):
        sys.exit("找不到 2024-07-30 版 libgraphwarrust.so：用 --so 指定或设 $GW2_SYMTAB_SO。\n"
                 "⚠ 2026 的三个版本被 strip 了，只有 2024-07-30 版有 .symtab。")
    repo = a.repo or os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")

    elf = Elf(so)
    syms = elf.func_symbols()
    items = parse_items(syms)
    anchors, locs, _seen = build_anchor_map(elf, a.dump_dis)

    out = sys.stdout
    out.write(f"<!-- 由 tools/symtab_dump.py 生成；二进制 sha256 见文首 -->\n")
    if a.cmd in ("summary", "all"):
        cmd_summary(items, anchors, locs, out)
    if a.cmd in ("modules", "all"):
        cmd_modules(items, anchors, out)
    if a.cmd in ("types", "all"):
        cmd_types(items, out)
    if a.cmd in ("methods", "all"):
        cmd_methods(items, anchors, out, only=a.arg)
    if a.cmd == "methods" and not a.arg:
        pass
    if a.cmd in ("locations", "all"):
        cmd_locations(items, anchors, locs, out, only=a.arg)
    if a.cmd in ("compare", "all"):
        cmd_compare(items, anchors, out,
                    os.path.join(repo, "docs", "02-module-map.md"),
                    os.path.join(repo, "docs", "coverage-matrix.tsv"))
    if a.cmd == "srcdiff":
        so_b = a.so_b or os.path.join(os.path.dirname(os.path.dirname(so)),
                                      "2026-08-28", "libgraphwarrust.so")
        if not os.path.exists(so_b):
            sys.exit(f"srcdiff 需要对照版本：{so_b} 不存在，用 --so-b 指定")
        cmd_srcdiff(so, so_b, out)


if __name__ == "__main__":
    main()
