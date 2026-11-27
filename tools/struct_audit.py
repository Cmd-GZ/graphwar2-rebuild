#!/usr/bin/env python3
"""数据结构取证：从真二进制里反推 **serde 结构体的字段名 / 字段顺序 / 字段数 / 字段偏移**。

## 为什么需要它

`AGENTS.md` §0 的终点是**源码级还原**，其中「数据结构能对上」是四条里最没被审计过的一条：
527 个可归属函数已按文件/边界对齐，但**结构体字段集合/顺序/偏移/大小**从来没逐项对过表。
这类偏差**不会让 `cargo test` 变红**（我方多一个/少一个字段，行为可以完全一致），
所以必须单独取证。

## 证据从哪来（四个来源，强度递减）

| 档 | 来源 | 给出什么 | 为什么可信 |
|---|---|---|---|
| **E1 串** | `.rodata` 的 `struct <Name> with N element(s)` | **字段数 N**（serde 在 `invalid_length` 里写死的描述串） | 由 `serde_derive` 从字段个数直接生成，改一个字段数这个串就变 |
| **E2 表** | `.data.rel.ro` 里的 `&'static [&'static str]` 字段名数组 | **字段名 + 声明顺序** | `Deserialize::deserialize` 把 `FIELDS` 表**地址与长度都写成指令立即数**，表项是 `(ptr,len)`，可逐项校验 |
| **E3 偏移** | `Serialize::serialize` 里 `mov/movsd 0xNN(%rXX),…`（`%rXX` = `&self`） | **字段偏移**，并借调用顺序给出**序列化顺序** | 每个字段的读取是 `self+offset`，偏移由编译器按定义顺序算 |
| **E4 尺寸** | sret 调用点的 `sub rsp,N` / `lea [rsp+k]` / `movaps` 宽度 | 没法 serde 的结构体的**大小**下限 | 被调方写 `[rcx+0x14]` ⇒ 结构至少 0x18 字节 |

`E1`/`E2` 是**逐位**档（串与表是编译期常量，能逐字节复核）；`E3` 是**观测**档
（指令级，但类型宽度要靠寄存器宽度判断）；`E4` 是**推断**档（只给下限）。

## 口径

一律 **ELF vaddr（RVA）**。Ghidra/REA 地址 = 本工具地址 **+ 0x100000**（只加一次）。

## 用法

    $PY tools/struct_audit.py tsv                     # 生成 fixtures/struct_audit.tsv（80 行审计表）
    $PY tools/struct_audit.py tsv --check             # 只校验（与磁盘上的表逐字节比）
    $PY tools/struct_audit.py serde                   # 全部 67 条 serde 名串 + 引用函数 + 归属
    $PY tools/struct_audit.py fields GraphwarColor --offsets   # 单个结构体的字段名表 / 偏移明细
    $PY tools/struct_audit.py tables                  # 全 `.data.rel.ro` 的 `&[&str]` 表扫描（见下）
    $PY tools/struct_audit.py rust                    # 我方 src 的 `pub struct` 清单
    $PY tools/struct_audit.py check                   # 按预注册清单打一张对表

## 一条反直觉的实测结论：**serde 的 `FIELDS` 表在二进制里根本不存在**

`#[derive(Deserialize)]` 生成的 `deserialize_struct(name, FIELDS, visitor)` 里，`FIELDS` 是
`&'static [&'static str]`；但 **`serde_json` 的 `deserialize_struct` 实现直接忽略 `fields`**
（它只调 `deserialize_map`）。于是链接期优化把这层调用内联后，**`FIELDS` 数组成了死数据、没有发出来**。
实测：全 `.data.rel.ro` 只有 **3** 张可校验的 `&[&str]` 表（`tables` 子命令），没有一张是事件字段表；
字段名只剩两条活路 —— ①`Serialize` 侧的 `lea rsi / mov edx,LEN` 序列（E2）；
②`Deserialize` visitor 里 `missing field` / `duplicate field` 分支对字段名字面量的 `lea`（只给集合、不给顺序）。

工具只做**取证与对表**，判定（match/mismatch/unknown）与修法写进
`fixtures/struct_audit.tsv` 与 `docs/06` §82 —— 判定是人的事，工具不给结论。
"""

import argparse
import json
import os
import re
import struct
import sys

from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_OP_REG, X86_REG_RIP, X86_REG_RSP

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import loc_xrefs  # noqa: E402

SO_DEFAULT = os.path.join(REPO, ".reference", "steam", "2026-08-28", "libgraphwarrust.so")
RE_SERDE = re.compile(rb"^struct ([A-Za-z_][A-Za-z0-9_]*) with (\d+) elements?$")
IDENT = re.compile(rb"^[a-z_][A-Za-z0-9_]*$")


def fmt(v):
    return "0x%x" % v if isinstance(v, int) else str(v)


# ---------------------------------------------------------------------------
# 索引：串 / 字段名表 / 引用
# ---------------------------------------------------------------------------
class Probe:
    def __init__(self, so=None, quiet=True):
        self.ix = loc_xrefs.LocXrefs(so=so or SO_DEFAULT, verbose=False)
        if quiet:
            self.ix._scan(verbose=False)
        self.elf = self.ix.elf
        self.rel = self.elf.relative_relocs()
        self.cs = Cs(CS_ARCH_X86, CS_MODE_64)
        self.cs.detail = True
        self.data_ranges = [(s["addr"], s["addr"] + s["size"])
                            for s in self.elf.data_sections()]

    # -- 读指针：`.data.rel.ro` 里的指针是 R_X86_64_RELATIVE 的 addend ----
    def ptr(self, slot):
        return self.rel.get(slot, self.elf.u64(slot))

    def in_data(self, va):
        return any(lo <= va < hi for lo, hi in self.data_ranges)

    # -- serde 结构体名串：以 `.data.rel.ro` 的 `&'static str` 槽为准 ------
    def serde_names(self):
        """[(name, n, str_va, slot)]：`struct <Name> with N element(s)`。

        **判据不是"在 `.rodata` 里搜串"，而是"`.data.rel.ro` 的 `&'static str` 槽"**：
        槽是 `{ptr, len}` 两个字，`ptr` 的 addend 是串首址、`len` 是**编译期长度**。
        必须 `read(addend, len)` 得到的字节**整体**匹配 `struct <N> with <K> element(s)`。

        ⚠️ 为什么不能只在 `.rodata` 里正搜：Rust 字面量**没有 NUL 终止**、相邻串背靠背，
        `struct LockInfoEvent with 1 element` 后面紧跟 `struct NameInfoEvent…`，
        贪婪的 `elements?` 会把下一个串的首字母 `s` 吃进去 ⇒ **下一个结构体整体漏掉**。
        实测这样会少 10 个（67→57），其中就包括 `NameInfoEvent`。
        """
        out, seen = [], set()
        for slot, add in self.rel.items():
            n = self.elf.u64(slot + 8)
            if n is None or not (24 <= n <= 72):
                continue
            b = self.elf.read(add, n)
            if not b or len(b) != n:
                continue
            m = RE_SERDE.match(b)
            if not m:
                continue
            if add in seen:
                continue
            seen.add(add)
            out.append((m.group(1).decode(), int(m.group(2)), add, slot))
        return out

    def refs_to(self, va):
        """引用该地址的指令地址列表。

        两级形态都收：**R** rip 直引（`lea`/`mov`/`call`），以及
        **D** 经 `.data.rel.ro` 的 `&'static str`/表槽中转
        （串地址写在槽里 ⇒ 代码只 `lea` 槽，[addr_probe.py](addr_probe.py)
        的 `callers` 子命令把它报成「reloc slot addend」）。
        漏掉 D 形态会得出「这个串没有引用者」的错结论。
        """
        out, seen = [], set()
        frontier = [va]
        for _ in range(3):
            nxt = []
            for tgt in frontier:
                for r in self.ix.refs.get(tgt, []):
                    if r[0] not in seen:
                        seen.add(r[0])
                        out.append(r[0])
                nxt.extend(self._rev_rel().get(tgt, []))
            frontier = [x for x in nxt if x not in seen]
            if not frontier:
                break
        return out

    def _rev_rel(self):
        if not hasattr(self, "_rev"):
            rev = {}
            for slot, add in self.rel.items():
                rev.setdefault(add, []).append(slot)
            self._rev = rev
        return self._rev

    def func_of(self, va):
        f = self.ix.func_of(va)
        return (f, self.ix.func_end(f)) if f is not None else (None, None)

    def func_src(self, f):
        """panic::Location 投票出的源文件（原版 `src/xxx.rs`）。"""
        try:
            locs = self.ix.locs_of_func(f)
        except Exception:
            return None
        if not locs:
            return None
        files = {}
        for slot in locs:
            info = self.ix.locs.get(slot) or {}
            name = info.get("file") if isinstance(info, dict) else None
            if name:
                files[name] = files.get(name, 0) + 1
        return max(files.items(), key=lambda kv: kv[1])[0] if files else None

    # -- 字段名表：`&'static [&'static str]` -----------------------------
    def try_str_slice(self, va, max_n=48):
        """把 `va` 当 `&[&str]`（ptr, len 两字）解释；全项可校验才算命中。"""
        if not self.in_data(va):
            return None
        p = self.ptr(va)
        n = self.elf.u64(va + 8)
        if p is None or n is None or not (1 <= n <= max_n) or not self.in_data(p):
            return None
        names = []
        for i in range(n):
            slot = p + 16 * i
            q = self.ptr(slot)
            k = self.elf.u64(slot + 8)
            if q is None or k is None or not (1 <= k <= 40) or not self.in_data(q):
                return None
            s = self.elf.read(q, k)
            if s is None or len(s) != k or not IDENT.match(s):
                return None
            names.append((s.decode(), q))
        return (p, va, names)

    def field_slice_in(self, f):
        """在一个函数体里找所有能校验通过的字段名表。"""
        lo, hi = self.func_of(f)
        if lo is None:
            return []
        blob = self.elf.read(lo, hi - lo)
        if blob is None:
            return []
        found, done = [], set()
        for ins in self.cs.disasm(blob, lo):
            for op in ins.operands:
                if op.type != X86_OP_MEM or op.mem.base != X86_REG_RIP:
                    continue
                tgt = (ins.address + ins.size + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
                if tgt in done:
                    continue
                done.add(tgt)
                r = self.try_str_slice(tgt)
                if r:
                    found.append((ins.address, tgt, r[2]))
        return found

    def all_str_tables(self, max_n=48, only=(".data.rel.ro", ".data")):
        """全表扫描：`.data.rel.ro` 里所有能校验通过的 `&'static [&'static str]`。

        这是「字段名 + 顺序」最直接的取证方式 —— 表项 `(ptr,len)` 逐项可校验，
        且 serde 的 `FIELDS` 就是 `&["a","b",…]` 这种形状。
        """
        out, done = {}, set()
        for sec in self.elf.data_sections():
            if only and sec["name"] not in only:
                continue
            base, size = sec["addr"], sec["size"]
            for i in range(0, size - 16, 8):
                a = base + i
                if a in done:
                    continue
                r = self.try_str_slice(a, max_n=max_n)
                if not r:
                    continue
                done.add(a)
                key = (r[0], len(r[2]))
                if key in out:
                    continue
                out[key] = (a, r[2])
        return out

    def name_of_deser_func(self, f):
        """在 `Deserialize::deserialize` 里找 `deserialize_struct(name, FIELDS, …)` 的 name。

        形态：`lea <槽>(%rip),%rsi`（name.ptr，槽是 `&'static str`）+ `mov $len,%edx`。
        槽里的 addend 才是串地址。校验 `strlen(串) == len` 才算命中。
        """
        lo, hi = self.func_of(f)
        if lo is None:
            return []
        blob = self.elf.read(lo, hi - lo)
        ins = list(self.cs.disasm(blob, lo))
        out = []
        for i, x in enumerate(ins):
            if x.mnemonic != "lea" or len(x.operands) != 2:
                continue
            op, dst = x.operands
            if op.type != X86_OP_MEM or op.mem.base != X86_REG_RIP:
                continue
            if x.reg_name(dst.reg) not in ("rsi", "rdx", "rcx", "r8"):
                continue
            slot = (x.address + x.size + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
            sp = self.ptr(slot)
            if sp is None or not self.in_data(sp):
                continue
            for y in ins[i + 1:i + 8]:
                if y.mnemonic != "mov" or len(y.operands) != 2:
                    continue
                if y.operands[0].type != X86_OP_IMM:
                    continue
                n = y.operands[0].imm
                if not (2 <= n <= 64):
                    continue
                s2 = self.elf.read(sp, n)
                if s2 is None or len(s2) != n:
                    continue
                if all(32 <= c < 127 for c in s2):
                    out.append((s2.decode(), sp, slot))
                break
        return out


    def ser_triples(self, f):
        """`Serialize::serialize` 的字段三元组：`(名, 名串va, 长度, 值偏移, 单态)`。

        形状（LLVM 把 serde 的 `Compound` 全内联后的实测形状）：

            lea  rsi, [rip+名]      ; &'static str.ptr
            mov  edx, LEN           ; &'static str.len
            lea  rcx, [rbx+k] / mov rcx, rbx   ; 值 = self+k
            call <serialize_field 单态>

        **代码顺序 = 声明顺序 = 线上键序**。`单态地址`是类型证据（同单态 = 同类型）。
        自校验：`LEN` 必须等于目标处可读字节数，且 `LEN` 个字节必须是纯标识符。
        """
        lo, hi = self.func_of(f)
        if lo is None:
            return []
        blob = self.elf.read(lo, hi - lo)
        ins = list(self.cs.disasm(blob, lo))
        selfreg = None
        for y in ins[:8]:
            if y.mnemonic == "mov" and len(y.operands) == 2 and y.operands[0].type == X86_OP_REG \
                    and y.operands[1].type == X86_OP_REG and y.reg_name(y.operands[1].reg) == "rdi":
                selfreg = y.reg_name(y.operands[0].reg)
                break
        out = []
        for i, x in enumerate(ins):
            if x.mnemonic != "lea" or len(x.operands) != 2:
                continue
            dst, op = x.operands              # capstone: operands[0] = 目标寄存器
            if op.type != X86_OP_MEM or op.mem.base != X86_REG_RIP:
                continue
            if x.reg_name(dst.reg) != "rsi":
                continue
            tgt = (x.address + x.size + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
            name, nlen = None, None
            for y in ins[i + 1:i + 6]:
                if y.mnemonic != "mov" or len(y.operands) != 2:
                    continue
                # 长度有两种落法：`mov LEN,%edx`（reg←imm）与 `mov [rsp+k],LEN`（mem←imm）
                if (y.operands[1].type == X86_OP_IMM and 1 <= y.operands[1].imm <= 32
                        and y.operands[0].type in (X86_OP_MEM, X86_OP_REG)):
                    nlen = int(y.operands[1].imm)
                if nlen:
                    break
            if not nlen:
                continue
            b = self.elf.read(tgt, nlen)
            if not b or not re.fullmatch(rb"[a-z_][a-z0-9_]*", b):
                continue
            name = b.decode()
            # 值偏移：`lea rcx,[self+k]` / `mov rcx,self`。
            # ⚠️ 两种落法都要收：多数结构体在**键 lea 之后**取值；`GameInfo::serialize` 在
            # **键 lea 之前**就 `lea rcx,[rbx+0x3c]`（实测 0x5061a8 → 键 0x5061ac），只看后面会漏。
            # ⚠️ 反向窗口只许取 `lea`：上一个字段的 `call <单态>` 就在键 lea 前面，
            #    若在反向里认 call 当单态，整张表的「名→单态」会整体错一位（已踩）。
            off, mono = None, None
            # `&self` 的寄存器 = 函数入口 `mov rXX, rdi` 的目标（GameInfo 是 rbx）。
            # 值指针形态：`mov <reg>, self`（偏移 0）或 `lea <reg>, [self+k]`。
            for y in ins[max(0, i - 3):i]:
                off = _self_off(y, selfreg, off)
            if off == 0:
                # `add rbx,k` 之后 `mov rcx,rbx`（GameInfo 的 locked 就是这形态）
                for y in ins[max(0, i - 6):i][::-1]:
                    if y.mnemonic == "add" and len(y.operands) == 2 \
                            and y.operands[0].type == X86_OP_REG \
                            and y.reg_name(y.operands[0].reg) == selfreg \
                            and y.operands[1].type == X86_OP_IMM:
                        off = int(y.operands[1].imm)
                        break
            if off is None:
                for y in ins[i + 1:i + 7]:
                    off = _self_off(y, selfreg, off)
                    if off is not None:
                        break
            for y in ins[i + 1:i + 10]:
                if y.mnemonic == "call" and y.operands and y.operands[0].type == X86_OP_IMM:
                    mono = y.operands[0].imm
                    break
            out.append((name, tgt, nlen, off, mono, x.address))
        return out

    def visitor_names(self, f):
        """反序列化 visitor 里能读出的字段名（`duplicate field` / `missing field` 路径）。

        ⚠️ 只给**集合**，不给顺序（这些路径的排列由分支顺序决定，不是声明顺序）。
        """
        lo, hi = self.func_of(f)
        if lo is None:
            return []
        blob = self.elf.read(lo, hi - lo)
        ins = list(self.cs.disasm(blob, lo))
        out = []
        for i, x in enumerate(ins):
            if x.mnemonic != "lea" or len(x.operands) != 2:
                continue
            dst, op = x.operands              # capstone: operands[0] = 目标寄存器
            if op.type != X86_OP_MEM or op.mem.base != X86_REG_RIP:
                continue
            tgt = (x.address + x.size + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
            for y in ins[i + 1:i + 5]:
                if y.mnemonic != "mov" or len(y.operands) != 2:
                    continue
                if y.operands[0].type == X86_OP_MEM and y.operands[1].type == X86_OP_IMM:
                    n = int(y.operands[1].imm)
                    if not (1 <= n <= 32):
                        continue
                    b = self.elf.read(tgt, n)
                    if b and re.fullmatch(rb"[a-z_][a-z0-9_]*", b):
                        out.append((b.decode(), tgt, n))
                    break
        # 同一个串地址可能被不同长度的立即数"误配"（`lea X` 后面紧跟 `mov …,1` 会把
        # `event` 读成 `e`）⇒ **同址只留最长的那个**，再按名字去重保序。
        best = {}
        for n, tgt, L in out:
            if tgt not in best or L > best[tgt][2]:
                best[tgt] = (n, tgt, L)
        seen, uniq = set(), []
        for n, tgt, L in sorted(best.values(), key=lambda r: r[2], reverse=True):
            if n not in seen:
                seen.add(n)
                uniq.append((n, tgt, L))
        return uniq

    def offset_profile(self, f, base_regs=("rdi", "rsi", "rcx", "rbx", "rax", "rdx")):
        """函数体里 `mov/movsd/movss [reg+disp]` 的偏移直方图（只按寄存器名粗分）。"""
        lo, hi = self.func_of(f)
        out = {}
        if lo is None:
            return out
        blob = self.elf.read(lo, hi - lo)
        for ins in self.cs.disasm(blob, lo):
            if len(ins.operands) == 0:
                continue
            for op in ins.operands:
                if op.type != X86_OP_MEM or op.mem.base == X86_REG_RIP:
                    continue
                reg = ins.reg_name(op.mem.base)
                disp = op.mem.disp
                key = (ins.mnemonic, reg, disp, op.size)
                out.setdefault(key, []).append(ins.address)
        return out


def _self_off(ins, selfreg, cur):
    """若该指令把 `selfreg`（`&self`）或 `[selfreg+k]` 写进某寄存器 ⇒ 返回 k（`mov` 形态返回 0）。"""
    if cur is not None or len(ins.operands) != 2:
        return cur
    dst, src = ins.operands
    if dst.type != X86_OP_REG:
        return cur
    if ins.mnemonic == "mov" and src.type == X86_OP_REG and ins.reg_name(src.reg) == selfreg:
        return 0
    if ins.mnemonic == "lea" and src.type == X86_OP_MEM and src.mem.base != X86_REG_RIP \
            and ins.reg_name(src.mem.base) == selfreg:
        return src.mem.disp
    return cur

# ---------------------------------------------------------------------------
# 我方 Rust 源码里的结构体
# ---------------------------------------------------------------------------
RE_STRUCT = re.compile(r"^\s*pub(?:\([^)]*\))?\s+struct\s+([A-Za-z_][A-Za-z0-9_]*)")
RE_FIELD = re.compile(r"^\s*pub(?:\([^)]*\))?\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.+?),\s*$")


def rust_structs(paths):
    """[(name, file:line, [(field, type)])] —— 只认 `pub struct` + `pub field: T,` 这种朴素形状。"""
    out = []
    for path in paths:
        try:
            lines = open(path, encoding="utf-8").read().splitlines()
        except OSError:
            continue
        i = 0
        while i < len(lines):
            m = RE_STRUCT.match(lines[i])
            if not m:
                i += 1
                continue
            name, start = m.group(1), i + 1
            fields = []
            j = i
            # 找 `{`
            while j < len(lines) and "{" not in lines[j]:
                j += 1
            depth = 0
            k = j
            while k < len(lines):
                depth += lines[k].count("{") - lines[k].count("}")
                fm = RE_FIELD.match(lines[k])
                if fm:
                    fields.append((fm.group(1), fm.group(2).strip()))
                if depth <= 0 and k > j:
                    break
                k += 1
            out.append((name, "%s:%d" % (os.path.relpath(path, REPO), start), fields))
            i = k + 1
    return out


# ---------------------------------------------------------------------------
# 子命令
# ---------------------------------------------------------------------------
def cmd_serde(p, args):
    rows = []
    for name, n, va in p.serde_names():
        refs = p.refs_to(va)
        funcs = []
        for r in refs:
            f, end = p.func_of(r)
            if f is not None and f not in [x[0] for x in funcs]:
                funcs.append((f, end, p.func_src(f)))
        rows.append((name, n, va, funcs))
    rows.sort(key=lambda r: r[0])
    if args.json:
        print(json.dumps([{"name": n, "n": k, "str_va": fmt(v),
                           "funcs": [{"fde": fmt(f), "end": fmt(e), "src": s}
                                     for f, e, s in fs]} for n, k, v, fs in rows],
                         ensure_ascii=False, indent=1))
        return 0
    print("# 结构体\t字段数N\t名串vaddr\t引用函数(FDE)")
    for name, n, va, funcs in rows:
        fs = "; ".join("%s..%s%s" % (fmt(f), fmt(e), ("[%s]" % s) if s else "") for f, e, s in funcs)
        print("%s\t%d\t%s\t%s" % (name, n, fmt(va), fs or "(无 rip 直引)"))
    print("# 共 %d 个 serde 结构体名串" % len(rows), file=sys.stderr)
    return 0


def cmd_fields(p, args):
    want = args.name
    hits = [t for t in p.serde_names() if t[0] == want]
    if not hits:
        print("找不到 serde 名串 %s" % want, file=sys.stderr)
        return 2
    for name, n, va in hits:
        print("== %s : 串 %s 声明 %d 个字段" % (name, fmt(va), n))
        for r in p.refs_to(va):
            f, end = p.func_of(r)
            if f is None:
                print("  引用 %s 不在任何 FDE 内" % fmt(r))
                continue
            print("  - 引用指令 %s @ %s..%s  src=%s" % (fmt(r), fmt(f), fmt(end), p.func_src(f) or "?"))
            sl = p.field_slice_in(f)
            if not sl:
                print("    字段名表：未命中（该函数可能只做长度报错，不含 FIELDS 表）")
            for ins_va, tbl_va, names in sl:
                print("    字段名表 @ %s（引用指令 %s），%d 项：" % (fmt(tbl_va), fmt(ins_va), len(names)))
                for idx, (nm, sva) in enumerate(names):
                    print("      [%2d] %-24s 串 %s" % (idx, nm, fmt(sva)))
            if args.offsets:
                prof = p.offset_profile(f)
                print("    偏移读数（mnemonic, base, disp, size → 指令数）：")
                for (mnem, reg, disp, size), addrs in sorted(prof.items(),
                                                             key=lambda kv: (kv[0][2], kv[0][1])):
                    if disp == 0 or abs(disp) > 0x400:
                        continue
                    print("      %-8s [%s%+4x] w=%d  ×%-3d  @%s"
                          % (mnem, reg, disp, size, len(addrs), fmt(addrs[0])))
        if args.dis:
            for r in p.refs_to(va):
                f, end = p.func_of(r)
                if f is None:
                    continue
                blob = p.elf.read(f, end - f)
                print("  --- disasm %s..%s ---" % (fmt(f), fmt(end)))
                for ins in p.cs.disasm(blob, f):
                    print("  %s  %-8s %s" % (fmt(ins.address), ins.mnemonic, ins.op_str))
    return 0


def cmd_tables(p, args):
    tabs = p.all_str_tables()
    rows = []
    for (tp, n), (hdr, names) in tabs.items():
        refs = p.refs_to(hdr)
        funcs = sorted({p.func_of(r)[0] for r in refs if p.func_of(r)[0] is not None})
        owners = []
        for f in funcs:
            owners.append((f, p.func_src(f), p.name_of_deser_func(f)))
        rows.append((hdr, names, owners))
    rows.sort(key=lambda r: r[0])
    for hdr, names, owners in rows:
        tag = ",".join(x for x in [",".join(nm for nm, _, _ in o[2]) for o in owners] if x)
        print("%s\t%d\t%s\t%s" % (fmt(hdr), len(names),
                                    ",".join(n for n, _ in names),
                                    ("own=%s" % tag) if tag else ""))
    print("# 共 %d 张可校验的 `&[&str]` 表（serde 的 FIELDS 数组在链接期被优化成死数据，"
          "**不在这里**——见文件头「反直觉的实测结论」）" % len(rows), file=sys.stderr)
    return 0


def cmd_rust(p, args):
    paths = args.path or sorted(
        os.path.join(REPO, "src", f) for f in os.listdir(os.path.join(REPO, "src"))
        if f.endswith(".rs"))
    if args.path and os.path.isdir(args.path[0]):
        d = args.path[0]
        paths = sorted(os.path.join(d, f) for f in os.listdir(d) if f.endswith(".rs"))
    rows = rust_structs(paths)
    rows.sort(key=lambda r: r[1])
    print("# 结构体\t位置\t字段数\t字段")
    for name, loc, fields in rows:
        print("%s\t%s\t%d\t%s" % (name, loc, len(fields),
                                  ", ".join("%s: %s" % (a, b) for a, b in fields)))
    print("# 共 %d 个 pub struct" % len(rows), file=sys.stderr)
    return 0


def cmd_check(p, args):
    """按预注册清单（fixtures/struct_audit_plan.txt 的 `#audit <Name>` 行）逐项取证。"""
    plan = args.plan or os.path.join(REPO, "fixtures", "struct_audit_plan.txt")
    want = []
    for ln in open(plan, encoding="utf-8"):
        m = re.match(r"^#audit\s+([A-Za-z_][A-Za-z0-9_]*)\s*$", ln.strip())
        if m:
            want.append(m.group(1))
    serde = {n: (k, v) for n, k, v in p.serde_names()}
    ours = {r[0]: r for r in rust_structs([os.path.join(REPO, "src", f)
                                           for f in os.listdir(os.path.join(REPO, "src"))
                                           if f.endswith(".rs")])}
    print("# 结构体\tE1字段数\tE2字段名表\t我方定义\t我方字段数")
    for name in want:
        e1 = serde.get(name)
        slices = []
        if e1:
            for r in p.refs_to(e1[1]):
                f, end = p.func_of(r)
                if f is None:
                    continue
                for ins_va, tbl_va, names in p.field_slice_in(f):
                    slices.append((f, tbl_va, names))
        mine = ours.get(name)
        print("%s\t%s\t%s\t%s\t%s" % (
            name,
            e1[0] if e1 else "-",
            ("; ".join("%s@%s(%d)" % (fmt(t), ",".join(n for n, _ in ns), len(ns))
                       for _, t, ns in slices) or "-"),
            mine[1] if mine else "-",
            len(mine[2]) if mine else "-"))
    return 0



# ---------------------------------------------------------------------------
# 第二轮（T56）：E5 单态类型类 / E6 枚举变体 / E7 非 serde 结构体窗口
# ---------------------------------------------------------------------------
# `Serialize` 单态 → **类型类**。判据（逐位，可复算）：
#   * 枚举：单态体里 `movzx r32, byte ptr [reg]`（或 `cmp byte ptr [reg], imm`）
#     + 一张 **4 字节相对跳表**（`movsxd rX, dword ptr [rY + rX*4]` + `add rX, rY` + `jmp rX`）
#     ⇒ 每个 case 一条 `lea r?, [rip+变体名]` + `mov r32, LEN` + `call write`。
#     case 下标 = **判别值**（跳表顺序 = 声明顺序 = 判别值顺序）。
#   * 整数：单态体里出现 itoa 的 u32 阈值常量 `0x2710`(10^4) 与 `0x5f5e0fff`(10^8-1)
#     且**没有**符号检查（`test`+`js` / `neg`）⇒ 无符号 32 位。8 字节整数不出现这两个常量
#     （u64 走 10^8/10^16 的另一组阈值 + `shr` 0x2d/0x3d）。
#     宽度的**独立第二证据**是结构体布局里的相邻字段偏移差（见 `layout`）。
#   * bool：单态体里同时出现 `true`/`false` 两个 4/5 字节字面量。
#   * 串：单态体里把值原样写进 writer（`write_str`/转义内联），没有数值格式化路径。
#   * 其它（含"有 tag 但不带跳表"的枚举）⇒ `unknown`，**不猜**。
SERDE_WRITE = 0x3bdf00     # serde_json 的 write_map_key / write_str（共同汇点）
SERDE_GROW = 0x461100      # serde_json 的输出缓冲扩容
ITOA_U32_LO = 0x2710       # itoa write_u32: n < 10_000
ITOA_U32_HI = 0x5F5E0FF    # itoa write_u32: n < 100_000_000


def _insn_list(p, f, limit=None):
    lo, hi = p.func_of(f)
    if lo is None:
        return []
    blob = p.elf.read(lo, hi - lo)
    if blob is None:
        return []
    out = list(p.cs.disasm(blob, lo))
    return out[:limit] if limit else out


def _imms(ins):
    return [int(o.imm) for o in ins.operands if o.type == X86_OP_IMM]


def _str_at(p, va, n):
    b = p.elf.read(va, n)
    return b.decode("ascii", "replace") if b and len(b) == n else None


def _lea_rip_targets(ins):
    """(指令, 目标va, 目标寄存器名) —— 只取 `lea r, [rip+disp]`。"""
    out = []
    for x in ins:
        if x.mnemonic != "lea" or len(x.operands) != 2:
            continue
        op = x.operands[1]
        if op.type != X86_OP_MEM or op.mem.base != X86_REG_RIP:
            continue
        tgt = (x.address + x.size + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
        out.append((x, tgt, x.reg_name(x.operands[0].reg)))
    return out


def _jump_table(p, ins):
    """识别 `movsxd rX, dword ptr [rY + rX*4]` + `add rX, rY` + `jmp rX`，返回 (base, [case_va])。"""
    idx = {x.address: i for i, x in enumerate(ins)}
    for i, x in enumerate(ins):
        if x.mnemonic != "movsxd" or "dword ptr" not in x.op_str or "*4" not in x.op_str:
            continue
        base = None
        for y in reversed(ins[max(0, i - 6):i]):
            if y.mnemonic == "lea" and len(y.operands) == 2 and y.operands[1].type == X86_OP_MEM \
                    and y.operands[1].mem.base == X86_REG_RIP:
                base = (y.address + y.size + y.operands[1].mem.disp) & 0xFFFFFFFFFFFFFFFF
                break
        if base is None:
            continue
        jt = None
        for y in ins[i + 1:i + 5]:
            if y.mnemonic == "jmp" and y.operands and y.operands[0].type == X86_OP_REG:
                jt = i
                break
        if jt is None:
            continue
        lo, hi = ins[0].address, ins[-1].address + ins[-1].size
        cases = []
        for k in range(256):
            b = p.elf.read(base + 4 * k, 4)
            if not b or len(b) != 4:
                break
            tgt = (base + int.from_bytes(b, "little", signed=True)) & 0xFFFFFFFFFFFFFFFF
            if not (lo <= tgt < hi):
                break
            cases.append((k, tgt))
        if cases:
            return base, cases
    return None, []


def _case_string(p, ins, case_va):
    """从 case 入口线性走到 `call write`，收集 (串va, 长度)。支持穿过无条件 jmp。"""
    idx = {x.address: i for i, x in enumerate(ins)}
    i, ptr, ln, seen = idx.get(case_va), None, None, set()
    while i is not None and 0 <= i < len(ins) and i not in seen:
        seen.add(i)
        x = ins[i]
        if x.mnemonic == "call":
            break
        if x.mnemonic == "jmp" and x.operands and x.operands[0].type == X86_OP_IMM:
            i = idx.get(int(x.operands[0].imm))
            continue
        if x.mnemonic == "lea" and len(x.operands) == 2 and x.operands[1].type == X86_OP_MEM \
                and x.operands[1].mem.base == X86_REG_RIP:
            reg = x.reg_name(x.operands[0].reg)
            if reg in ("rdx", "rsi", "rcx", "r8") and ptr is None:
                ptr = (x.address + x.size + x.operands[1].mem.disp) & 0xFFFFFFFFFFFFFFFF
        if x.mnemonic == "mov" and len(x.operands) == 2 and x.operands[1].type == X86_OP_IMM \
                and x.operands[0].type == X86_OP_REG and ln is None:
            if x.reg_name(x.operands[0].reg) in ("ecx", "edx", "r8d", "rcx", "rdx", "r8"):
                ln = int(x.operands[1].imm)
        i += 1
    if ptr is None or ln is None or not (1 <= ln <= 48):
        return None, None
    return _str_at(p, ptr, ln), ptr


def mono_kind(p, addr):
    """`Serialize` 单态 → 类型类。返回 dict（kind/detail/evidence）。"""
    f, _ = p.func_of(addr)
    if f is None:
        return {"kind": "unknown", "detail": "地址不在任何 FDE 内", "evidence": ""}
    ins = _insn_list(p, f)
    # 尾调用形态：单态体最后是 `jmp <外部目标>`（= 真正的 `<T as Serialize>::serialize`），
    # 本函数只是 map-key 写入 + 值转发。GameState/TimeMode/address 都是这一形态。
    lo, hi = p.func_of(f)
    if ins:
        for x in reversed(ins[-60:]):
            if x.mnemonic == "jmp" and x.operands and x.operands[0].type == X86_OP_IMM:
                t = int(x.operands[0].imm)
                if not (lo <= t < hi):
                    inner = _insn_list(p, t)
                    if inner:
                        ins = inner
                        f = p.func_of(t)[0]
                    break
    imms = set()
    for x in ins:
        imms.update(_imms(x))
    # --- 枚举（跳表形态）---
    base, cases = _jump_table(p, ins)
    if base is not None:
        names = []
        for k, va in cases:
            nm, ptr = _case_string(p, ins, va)
            names.append((k, nm, ptr))
        if all(n for _, n, _ in names):
            return {"kind": "enum", "detail": "enum{%s}" % ", ".join("%s=%d" % (n, k) for k, n, _ in names),
                    "evidence": "跳表 @%s，%d 个 case；判别式 = `movzx` 读 1 字节" % (fmt(base), len(cases)),
                    "variants": [(k, n) for k, n, _ in names], "table": base, "func": f}
        return {"kind": "enum?", "detail": "有跳表但 case 串不全", "evidence": "跳表 @%s" % fmt(base), "func": f}
    # --- 二变体枚举：两种实测形态（都必须能从控制流定出判别值 0/1 对应哪个串）---
    leas = _lea_rip_targets(ins)
    idx_by_addr = {x.address: i for i, x in enumerate(ins)}
    # ⚠️ 这些单态体里**还有一个** `cmp byte ptr [rdi + 8], 1`（serde map 的「首个字段」状态），
    # 它排在真正判别式前面 ⇒ 必须排除带位移的形态，否则永远认错。
    cmps = [x for x in ins if x.mnemonic == "cmp" and "byte ptr" in x.op_str and "+" not in x.op_str]
    if cmps:
        ci = idx_by_addr[cmps[0].address]
        # 形态 A：`cmp byte [r], 0` + `lea A` + `lea B` + `cmovne` ⇒ 0=A、1=B
        for x in ins[ci + 1:ci + 8]:
            if x.mnemonic.startswith("cmov"):
                two = [y for y in ins[max(0, ci - 8):idx_by_addr[x.address]]
                       if y.mnemonic == "lea" and len(y.operands) == 2
                       and y.operands[1].type == X86_OP_MEM]
                if len(two) >= 2:
                    lens = {int(y.operands[1].imm) for y in ins
                            if y.mnemonic == "mov" and len(y.operands) == 2
                            and y.operands[1].type == X86_OP_IMM
                            and y.operands[0].type == X86_OP_REG
                            and y.reg_name(y.operands[0].reg) in ("ecx", "edx", "r8d")
                            and 3 <= int(y.operands[1].imm) <= 32}
                    if len(lens) != 1:
                        return {"kind": "enum2", "detail": "2 变体但变体名长度不唯一（%s）⇒ 只报集合"
                                % sorted(lens), "evidence": "`%s` + `%s`" % (cmps[0].op_str, x.mnemonic),
                                "func": f}
                    L = lens.pop()
                    def _nm(insn):
                        t = (insn.address + insn.size + insn.operands[1].mem.disp) & 0xFFFFFFFFFFFFFFFF
                        return _str_at(p, t, L)
                    n0 = ( _nm(two[-1]), two[-1].address)
                    n1 = ( _nm(two[-2]), two[-2].address)
                    if n0[0] and n1[0]:
                        return {"kind": "enum2", "detail": "enum{%s=0, %s=1}" % (n0[0], n1[0]),
                                "evidence": "`%s` + `%s`（0 走 fallthrough）" % (cmps[0].op_str, x.mnemonic),
                                "variants": [(0, n0[0]), (1, n1[0])], "func": f}
                break
        # 形态 B：`cmp byte [r], 1` + `jne L` ⇒ 0 = fallthrough、1 = L
        for x in ins[ci + 1:ci + 4]:
            if x.mnemonic in ("jne", "jne ", "jne") and x.operands and x.operands[0].type == X86_OP_IMM:
                tgt = int(x.operands[0].imm)
                if tgt in idx_by_addr:
                    fall = _case_string(p, ins, x.address + x.size)
                    jump = _case_string(p, ins, tgt)
                    if fall[0] and jump[0]:
                        # `cmp byte,1` + `jne L`：**等于 1 时走 fallthrough**，不等于（=0）跳到 L
                        return {"kind": "enum2", "detail": "enum{%s=0, %s=1}" % (jump[0], fall[0]),
                                "evidence": "`%s` + `%s L`（1 = fallthrough，0 = L）"
                                % (cmps[0].op_str, x.mnemonic),
                                "variants": [(0, jump[0]), (1, fall[0])], "func": f}
    # --- u32（itoa 阈值 + 无符号分支）---
    if ITOA_U32_LO in imms and ITOA_U32_HI in imms:
        signed = any(("test" in x.mnemonic and "js" in x.op_str) or
                     (x.mnemonic == "jmp" and False) for x in ins)
        # 更硬的判据：有没有 `js`/`jl` 指令
        signed = any(x.mnemonic in ("js", "jl", "jns", "jge", "jle") for x in ins) or \
                 any(x.mnemonic == "neg" for x in ins)
        return {"kind": "int", "detail": "unsigned 32-bit（itoa write_u32 路径）",
                "evidence": "阈值 0x2710 + 0x5f5e0fff；符号检查 %s" % ("有" if signed else "无"),
                "func": f}
    # --- bool ---
    ro = p.elf.sec(".rodata")
    heads = []
    for x, tgt, reg in leas:
        s = _str_at(p, tgt, 8)
        if s:
            heads.append(s)
    if any(h.startswith("true") for h in heads) and any(h.startswith("false") for h in heads):
        return {"kind": "bool", "detail": "bool（true/false 两个字面量）", "evidence": "lea `true`/`false`", "func": f}
    # --- 串 ---
    if not imms & {ITOA_U32_LO, ITOA_U32_HI}:
        calls = {int(x.operands[0].imm) for x in ins
                 if x.mnemonic == "call" and x.operands and x.operands[0].type == X86_OP_IMM}
        ext = calls - {SERDE_WRITE, SERDE_GROW}
        if not ext:
            return {"kind": "str", "detail": "串（转义/写入全部内联，无外层调用）",
                    "evidence": "只有 write/grow 两个汇点", "func": f}
    return {"kind": "unknown", "detail": "未命中已知指纹", "evidence": "", "func": f}


def serde_layout(p, ser_f):
    """`Serialize::serialize` → 字段布局：[(名, 值偏移, 单态, 类型类)]。"""
    tri = p.ser_triples(ser_f)
    out = []
    for name, sva, nlen, off, mono, site in tri:
        k = mono_kind(p, mono) if mono else {"kind": "?", "detail": "?", "evidence": ""}
        out.append({"name": name, "off": off, "mono": mono, "kind": k, "site": site})
    return out


def _size_of_kind(k):
    """类型类 → 字节宽（只在**无歧义**时给数；enum 且已知变体数时按 1 字节 tag 给 1）。"""
    t = k.get("kind")
    if t == "enum" or t == "enum2":
        return 1
    if t == "bool":
        return 1
    if t == "int":
        d = k.get("detail", "")
        if "32-bit" in d:
            return 4
    if t == "str":
        return None            # String=24 / &str=16 / Cow=32 不可从指纹定
    return None



def cmd_mono(p, args):
    for a in args.addr:
        va = int(a, 16)
        k = mono_kind(p, va)
        print("%s\t%s\t%s\t%s" % (a, k["kind"], k["detail"],
                                     "func=%s %s" % (fmt(k["func"]), k["evidence"]) if k.get("func") else ""))
    return 0


def cmd_layout(p, args):
    for a in args.addr:
        va = int(a, 16)
        rows = serde_layout(p, va)
        print("== Serialize @%s：%d 个字段（声明顺序 = 线上键序）" % (fmt(va), len(rows)))
        prev = None
        for r in rows:
            gap = "" if r["off"] is None or prev is None or r["off"] <= prev else \
                  "   ← 与前一个字段相距 %d 字节" % (r["off"] - prev)
            print("   %-24s off=%-6s mono=%-9s %-52s%s" % (
                r["name"], fmt(r["off"]) if r["off"] is not None else "?",
                fmt(r["mono"]) if r["mono"] else "?",
                r["kind"]["kind"] + "：" + r["kind"]["detail"], gap))
            if r["off"] is not None:
                prev = r["off"]
        offs = sorted(r["off"] for r in rows if r["off"] is not None)
        if offs:
            print("   —— 偏移排序：%s" % ", ".join(fmt(o) for o in offs))
            print("   —— 偏移跨度下限 = 最后偏移 + 最后字段宽（宽由类型类给，未知时见备注）")
        if args.json:
            import json as _json
            print(_json.dumps([{k2: v2 for k2, v2 in r.items() if k2 != "kind"} |
                               {"kind": r["kind"]["kind"], "detail": r["kind"]["detail"]}
                               for r in rows], ensure_ascii=False, indent=1))
    return 0


def cmd_enums(p, args):
    """扫 .text 里所有 `Serialize` 枚举实现（跳表形态），产出「枚举名 → 变体表」。

    枚举名不在这类实现里（serde 的 `serialize_unit_variant` 只拿变体名），
    所以用**变体集合**做指纹去和我方 enum 对表；能指到的调用点用 `mono` 反查。
    """
    seen = {}
    for f in p.ix.func_starts if hasattr(p.ix, "func_starts") else []:
        pass
    # 退一步：从已知单态地址出发（args.addr），或扫全部函数里带跳表的
    for f in sorted(p.ix.func_of_cache.keys() if hasattr(p.ix, "func_of_cache") else []):
        pass
    for a in args.addr:
        va = int(a, 16)
        k = mono_kind(p, va)
        print("%s\t%s\t%s" % (a, k["kind"], k["detail"]))
    return 0


def cmd_structs(p, args):
    """非 serde 结构体扫描（E7）：在**可归属函数**里找「同一基址寄存器的固定偏移窗口」+ sret。

    判据（先写死，复算即本函数）：
      * 只统计 `mov/movsd/movss/movaps/movdqu/movups` 且基址寄存器不是 rsp/rbp、位移 ∈ [0, 0x800)；
      * 同一个基址寄存器上**≥`--min-off` 个不同位移**且跨度 ≤ 0x800 ⇒ 认作一个"结构体窗口"；
      * 窗口大小下限 = max(位移 + 访问宽度)；这是 **E4 下界**（上界要调用方夹逼，本轮不做）；
      * 函数归属用 `fixtures/function_coverage.tsv` 的 527 行（vaddr → 原版模块 + 我方文件）。
    """
    import csv
    cov = os.path.join(REPO, "fixtures", "function_coverage.tsv")
    mod = {}
    if os.path.exists(cov):
        for ln in open(cov, encoding="utf-8"):
            if ln.startswith("#") or not ln.strip():
                continue
            c = ln.rstrip("\n").split("\t")
            mod[int(c[0], 16)] = (c[3], c[5])
    out = []
    for va, (srcf, ours) in sorted(mod.items()):
        if args.limit and len(out) >= args.limit:
            break
        ins = _insn_list(p, va)
        if not ins:
            continue
        acc = {}
        for x in ins:
            if x.mnemonic not in ("mov", "movsd", "movss", "movaps", "movdqu", "movups",
                                  "vmovaps", "vmovdqu", "vmovups"):
                continue
            for op in x.operands:
                if op.type != X86_OP_MEM or op.mem.base == X86_REG_RIP:
                    continue
                reg = x.reg_name(op.mem.base)
                if reg in ("rsp", "rbp", "rip"):
                    continue
                if not (0 <= op.mem.disp < 0x800):
                    continue
                acc.setdefault(reg, set()).add((op.mem.disp, op.size))
        best = None
        for reg, s in acc.items():
            offs = sorted({d for d, w in s})
            if len(offs) < args.min_off:
                continue
            span = offs[-1] - offs[0]
            lo_ = max(d + w for d, w in s)
            cand = (len(offs), span, lo_, reg)
            if best is None or cand > best:
                best = cand
        if best is None:
            continue
        n, span, lower, reg = best
        out.append((va, srcf.split("::")[0], n, span, lower, reg, ours))
    if args.tsv:
        print("# 函数vaddr\t原版模块\t不同偏移数\t跨度\tE4下界\t基址寄存器\t我方文件")
        for va, m, n, span, lower, reg, ours in out:
            print("%s\t%s\t%d\t%d\t%d\t%s\t%s" % (fmt(va), m, n, span, lower, reg, ours))
        return 0
    for va, m, n, span, lower, reg, ours in out:
        print("%-10s %-40s offs=%-3d span=0x%-4x E4下界=0x%-4x %s" % (fmt(va), m, n, span, lower, reg))
    print("# %d 个函数命中固定偏移窗口（判据见 cmd_structs docstring）" % len(out), file=sys.stderr)
    return 0



# ---------------------------------------------------------------------------
# 第二轮审计表：fixtures/struct_audit2.tsv（T56）
# ---------------------------------------------------------------------------
# 每个锚点都写死**地址**，判定列由工具**现场重算**（`tsv2 --check` 必须逐字节相同）：
#   * serde 负载 ⇒ 用 `serde_layout`（E2 键序 + E3 值偏移 + E5 类型类）现场取；
#   * 枚举      ⇒ 用 `mono_kind`（E6 变体名 + 判别值）现场取，再用我方 `pub enum` 逐项比；
#   * 非 serde  ⇒ 用 `cmd_structs` 的同一判据现场算窗口下界（只有下界 ⇒ unknown）。
AUDIT2_SERDE = [
    # (结构体, 我方文件（**权威形态**所在）, 我方类型名, Serialize 地址, E1 名串槽说明)
    ("GameInfo", "src/events.rs", "GameInfo", 0x5060D0),
]
AUDIT2_ENUM = [
    # (枚举, 我方文件, 我方类型名, 单态地址, 枚举 Serialize 实现地址)
    ("GameState", "src/turnstate.rs", "GameState", 0x3CC260, 0x46F020),
    ("AxisMode", "src/graphwar_mode_manager.rs", "AxisMode", 0x3CB560, None),
    ("FunctionMode", "src/graphwar_mode_manager.rs", "FunctionMode", 0x3CB670, None),
    ("TurnMode", "src/graphwar_turn_manager.rs", "TurnMode", 0x3CBF70, None),
    ("TimeMode", "src/graphwar_mode_manager.rs", "TimeMode", 0x3CBCE0, 0x518A10),
]
AUDIT2_STRUCT = [
    # (我方类型名, 我方文件, 二进制锚点函数 vaddr（None ⇒ 由**模块名机械挑选**）, E3 期望偏移, 说明)
    # ⚠️ 路径已按 T57（`docs/06` §104）搬家后的现状更新：`*_class.rs` 里的 gdext 类搬回了原版模块文件。
    ("EffectAudioData", "src/graphwar_audio_control.rs", None, "0x0/0x10/0x14"),
    ("PatternSegment", "src/graphwar_level_generator.rs", None, "0x0/0x8/0x10/0x18/0x20/0x24/0x28"),
    ("GraphwarGameRecorder", "src/graphwar_game_recorder.rs", 0x3D4FA0, "0x08/0x18/0x40/0x48"),
    ("ShotCalculator", "src/graphwar_shot_calculator.rs", 0x3C2BC0, None),
    ("FunctionEvolutionator", "src/graphwar_function_evolutionator.rs", 0x41A9D0, None),
    ("GraphwarPlane", "src/graphwar_plane.rs", 0x4B5DF0, None),
    ("GraphicsManager", "src/graphwar_graphics_manager.rs", 0x468D80, None),
    ("ReplayControl", "src/graphwar_replay_control.rs", 0x4C6F40, None),
    ("LevelManager", "src/graphwar_level_manager.rs", 0x499740, None),
    ("ClientConnectionHandler", "src/graphwar_client_connection_handler.rs", 0x45A340, None),
    ("AxisDrawer", "src/_scaffold/gdext_mod.rs", 0x3E2A00, None),
    ("GraphwarLanVBox", "src/graphwar_lan_vbox.rs", 0x4DEC00, None),
]


def _ours_layout(path, name):
    """在**指定文件**里找我方同名结构体的字段（**私有字段也算**，gdext 类的字段都是私有的）。

    与 [`rust_structs`] 的区别：那个只认 `pub struct` + `pub field`（用于 serde 负载的公共口径），
    这里要的是"我方到底声明了几个字段、什么类型"，私有与否无关 ⇒ 单独一个朴素解析器。
    """
    try:
        lines = open(path, encoding="utf-8").read().splitlines()
    except OSError:
        return None
    pat = re.compile(r"^\s*pub(?:\([^)]*\))?\s+struct\s+%s\b" % re.escape(name))
    fld = re.compile(r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.+?),\s*$")
    for i, ln in enumerate(lines):
        if not pat.match(ln):
            continue
        fields, j, depth = [], i, 0
        while j < len(lines) and "{" not in lines[j]:
            j += 1
        k = j
        while k < len(lines):
            depth += lines[k].count("{") - lines[k].count("}")
            if k > j:
                m = fld.match(lines[k])
                if m and not lines[k].lstrip().startswith(("//", "#", "#[")):
                    fields.append((m.group(1), m.group(2).strip()))
            if depth <= 0 and k >= j:
                break
            k += 1
        return ("%s:%d" % (os.path.relpath(path, REPO), i + 1), fields)
    return None


RE_ENUM = re.compile(r"^\s*pub\s+enum\s+([A-Za-z_][A-Za-z0-9_]*)")
RE_VARIANT = re.compile(r"^\s{4}([A-Z][A-Za-z0-9_]*)\s*(?:=\s*(\d+))?\s*,?\s*$")


def rust_enums(paths):
    """我方 `pub enum` → [(名字, 文件:行, [(变体, 判别值)])]。判别值 = 显式或声明顺序。"""
    out = []
    for path in paths:
        try:
            lines = open(path, encoding="utf-8").read().splitlines()
        except OSError:
            continue
        i = 0
        while i < len(lines):
            m = RE_ENUM.match(lines[i])
            if not m:
                i += 1
                continue
            name, start, vs, nxt, j = m.group(1), i + 1, [], None, i
            while j < len(lines) and "{" not in lines[j]:
                j += 1
            depth, k = 0, j
            while k < len(lines):
                depth += lines[k].count("{") - lines[k].count("}")
                vm = RE_VARIANT.match(lines[k])
                if vm:
                    v = vm.group(1)
                    if vm.group(2) is not None:
                        nxt = int(vm.group(2))
                    vs.append((v, nxt if nxt is not None else len(vs)))
                    nxt = vs[-1][1] + 1 if vm.group(2) is not None else None
                if depth <= 0 and k > j:
                    break
                k += 1
            out.append((name, "%s:%d" % (os.path.relpath(path, REPO), start), vs))
            i = k + 1
    return out


def _struct_window(p, va, min_off=4):
    ins = _insn_list(p, va)
    if not ins:
        return None
    acc = {}
    for x in ins:
        if x.mnemonic not in ("mov", "movsd", "movss", "movaps", "movdqu", "movups",
                              "vmovaps", "vmovdqu", "vmovups"):
            continue
        for op in x.operands:
            if op.type != X86_OP_MEM or op.mem.base == X86_REG_RIP:
                continue
            reg = x.reg_name(op.mem.base)
            if reg in ("rsp", "rbp", "rip") or not (0 <= op.mem.disp < 0x800):
                continue
            acc.setdefault(reg, set()).add((op.mem.disp, op.size))
    best = None
    for reg, ss in acc.items():
        offs = sorted({d for d, w in ss})
        if len(offs) < min_off:
            continue
        cand = (len(offs), offs[-1] - offs[0], max(d + w for d, w in ss), reg, offs)
        if best is None or cand[:4] > best[:4]:
            best = cand
    return best


def cmd_tsv2(p, args):
    rows = []
    allsrc = [os.path.join(REPO, "src", f) for f in sorted(os.listdir(os.path.join(REPO, "src")))
              if f.endswith(".rs")]
    ours_struct = {r[0]: r for r in rust_structs(allsrc)}
    ours_enum = {}
    for nm, loc, vs in rust_enums(allsrc):
        ours_enum.setdefault(nm, []).append((loc, vs))
    e1 = {n: (k, v, s) for n, k, v, s in p.serde_names()}

    # ---- (1) serde 负载：GameInfo ----
    # T66：权威形态在 `src/events.rs`（5 个真枚举 + u32 + address String），
    # `src/_scaffold/protocol.rs` 只剩一个 **JSON 视图**（枚举字段用线上串承载，
    # `Broadcast` 是它的别名）⇒ 「我方字段集」按**权威形态**列，视图单独在备注里点明。
    for name, ours_path, ours_ty, serf in AUDIT2_SERDE:
        lay = serde_layout(p, serf)
        bnames = [r["name"] for r in lay]
        mine = ours_struct.get("GameInfo")  # 取 `src/events.rs` 那份（_scaffold 不在 allsrc 里）
        ofields = [f for f, _ in mine[2]] if mine else []
        otypes = dict(mine[2]) if mine else {}
        k, sva, slot = e1.get(name, (None, None, None))
        bad, ok = [], []
        WANT_TY = {
            "game_state": "GameState",
            "axis_mode": "AxisMode",
            "function_mode": "FunctionMode",
            "turn_mode": "TurnMode",
            "time_mode": "TimeMode",
        }
        for r in lay:
            kk = r["kind"]["kind"]
            t = otypes.get(r["name"], "")
            if r["off"] == 0x18:
                ok.append("%s@0x18 我方 `String`(24B)（原版 32 字节枚举，身份=推断，登记）" % r["name"])
            elif r["name"] in WANT_TY:
                if WANT_TY[r["name"]] in t:
                    ok.append("%s=%s ✅" % (r["name"], WANT_TY[r["name"]]))
                else:
                    bad.append("%s 我方 `%s`，原版 `%s`" % (r["name"], t or "?", r["kind"]["detail"][:40]))
            elif kk == "int":
                if "u32" in t:
                    ok.append("%s=u32 ✅" % r["name"])
                else:
                    bad.append("%s@%s：u32（4 字节），我方 `%s`" % (r["name"], fmt(r["off"]), t or "?"))
        judge = "match" if (len(bnames) == len(ofields) == (k or -1) and bnames == ofields and not bad) else "mismatch"
        note = ("**字段数/键序 match；5 个枚举字段已落地成真类型**（T66）：" + "；".join(ok)
                if not bad else "**类型 mismatch**：" + "；".join(bad))
        note += ("｜**我方 3 处 → 1 处（T66，lead 授权后达成）**：结构体定义只剩 `src/events.rs` 一处"
                 "（本行「我方字段集」列）；`src/_scaffold/protocol.rs` 里的是 `pub use crate::events::GameInfo;`"
                 "（**同一类型**，只提供 serde 两个进出口 `to_json`/`parse`），`Broadcast` 是该类型的**别名**"
                 "（原版只有一个 `struct GameInfo`）。达成条件 = lead 批准改 `tests/wire_format.rs` 的 "
                 "`\"Timers1m\"` 错字（真值 `Timer1m`）+ `_scaffold/broker.rs` 的 `i64`→`u32`")
        note += "｜**仍登记**：`address` 身份（32B/1B tag/2 变体，形状=观测、身份=推断）"
        rows.append([name,
                     "%s（`GameInfo`，**唯一结构体定义**；`_scaffold/protocol.rs` 里是同一类型的 `pub use` + serde 进出口，`Broadcast` = 别名）" % ours_path,
                     "%d 字段：%s" % (len(ofields), ",".join("%s:%s" % (f, t) for f, t in mine[2]) if mine else "?"),
                     "E1 slot=%s→%s N=%s；E2 ser=%s；E3 值偏移见括号；E5 单态：%s" % (
                         fmt(slot), fmt(sva), k, fmt(serf),
                         ", ".join("%s=%s%s" % (r["name"], fmt(r["off"]), fmt(r["mono"])) for r in lay)),
                     "%d 字段：%s" % (len(bnames), ", ".join(bnames)),
                     judge, "逐位 E1+E2+E6 + 观测 E3 + 观测 E5", note])

    # ---- (2) 枚举 ----
    for name, ours_path, ours_ty, mono, implva in AUDIT2_ENUM:
        k = mono_kind(p, mono)
        bvars = k.get("variants") or []
        cand = ours_enum.get(ours_ty) or []
        hit = [c for c in cand if c[0].startswith(ours_path)] or cand[:1]
        oline = hit[0][0] if hit else "-"
        ovars = hit[0][1] if hit else []
        others = ["%s（%d 变体）" % (l, len(v)) for l, v in cand if not l.startswith(ours_path)]
        dup = ("；⚠️ 我方另有重复定义：%s" % "、".join(others)) if others else ""
        judge = "match" if (bvars and len(bvars) == len(ovars) and
                            [(v, d) for v, d in ovars] == [(n, d) for d, n in bvars]) else "mismatch"
        if not bvars:
            judge = "unknown"
        rows.append([name, "%s（%s）" % (ours_path, oline),
                     "%d 变体：%s" % (len(ovars), ", ".join("%s=%d" % (v, d) for v, d in ovars)),
                     "E5 单态=%s ⇒ %s；E6 %s%s" % (
                         fmt(mono), k["kind"], k["evidence"],
                         ("；实现 @%s" % fmt(implva)) if implva else ""),
                     "%d 变体：%s" % (len(bvars), ", ".join("%s=%d" % (n, d) for d, n in bvars)),
                     judge, "逐位 E5+E6（变体名 LEN 自校验 + 跳表下标 = 判别值）",
                     ("变体名逐字相同、判别值逐项相同" if judge == "match" else "见左列差异") + dup])

    # ---- (3) EventInfo（serde，但只有 Deserialize；字段名/顺序走 visitor）----
    ev = [(n, kk, v, sl) for n, kk, v, sl in p.serde_names() if n == "EventInfo"]
    mine = ours_struct.get("EventInfo")
    ofields = [f for f, _ in mine[2]] if mine else []
    k1, sva, slot = ev[0][1], ev[0][2], ev[0][3]
    miss = []
    for r in p.refs_to(sva):
        f, _ = p.func_of(r)
        if f is None:
            continue
        for n, tgt, L in p.visitor_names(f):
            miss.append((r, n, tgt, L))
    names = []
    for r, n, tgt, L in miss:
        if n not in [x[0] for x in names]:
            names.append((n, tgt, L))
    rows.append(["EventInfo",
                 "%s（`EventInfo`；T32 记成「我方没有」= **错的**）" % (mine[1] if mine else "-"),
                 "%d 字段：%s" % (len(ofields), ", ".join(ofields)),
                 "E1 slot=%s→%s N=%s；字段名 = Deserialize visitor 的 missing_field 路徑（%s）；"
                 "顺序 = missing_field 调用顺序 `event`@0x397592 先于 `tick`@0x397843；"
                 "E3 内存布局：event@0x00（GraphwarEvent 0x48 字节）、tick@0x48（4 字节，"
                 "`0x3d62ac mov ecx,dword ptr [rax+rsi+0x48]` 在 save_game 0x3d5740 里）、元素步长 0x50（0x3d4fa0 闭包）" % (
                     fmt(slot), fmt(sva), k1, ", ".join("%s@%s(len=%d)" % (n, fmt(t), L) for n, t, L in names)),
                 "%d 字段：event, tick（顺序 event→tick）" % k1,
                 "match" if (k1 == len(ofields) and ofields == ["event", "tick"]) else "mismatch",
                 "逐位 E1 + 观测 E2(集合) / E3(顺序+宽度)",
                 "**审计发现 = mismatch（我方 3 字段且顺序反了 tick,event,body）、改后 = match**："
                 "本轮把 `EventInfo` 改成 `{ event: GraphwarEvent, tick: u32 }` 两字段；"
                 "`GraphwarEvent{name,body}` 承载原版 0x48 字节外部标签枚举的**表示差异仍登记**"])

    # ---- (4) 非 serde 结构体：E7 窗口 + **E8 上界夹逼**（T66）----
    # 判定按 fixtures/struct_audit_plan3.txt §1.2/§1.3 的规则**现场重算**：
    #   * U 到手且我方字段偏移集合（按标量尺寸表算）== [0,U) 内窗口偏移集合 ⇒ match（偏移+宽度档）
    #   * U 到手且对不上（或我方字段数明显少于窗口偏移数）⇒ mismatch
    #   * U 不成立、或我方布局算不出（枚举/私有复合类型无尺寸表项）⇒ unknown（写清卡点）
    cov = {}
    covp = os.path.join(REPO, "fixtures", "function_coverage.tsv")
    if os.path.exists(covp):
        for ln in open(covp, encoding="utf-8"):
            if ln.startswith("#") or not ln.strip():
                continue
            c = ln.rstrip("\n").split("\t")
            cov.setdefault(c[3], []).append(int(c[0], 16))
    for name, ours_path, anchor, offs_hint in AUDIT2_STRUCT:
        if anchor is None:
            mod = os.path.basename(ours_path)
            best = None
            want = {int(x, 16) for x in offs_hint.split("/")} if offs_hint else set()
            for va in cov.get(mod, []):
                ww = _struct_window(p, va)
                if ww is None:
                    continue
                hit = len(want & set(ww[4]))
                key = (hit, ww[0], ww[2])
                if best is None or key > best[0]:
                    best = (key, va, ww)
            if best is None:
                rows.append([name, "%s（%s）" % (ours_path, "-"), "?", "E7：该模块无可归属函数窗口",
                             "?", "unknown", "推断 E7", "登记"])
                continue
            anchor = best[1]
        w = _struct_window(p, anchor)
        mine = _ours_layout(os.path.join(REPO, ours_path), name)
        oline = mine[0] if mine else "-"
        ofields = mine[1] if mine else []
        nf = len(ofields)
        if mine is None:
            modfile = os.path.join(REPO, ours_path)
            cands = [r[0] for r in rust_structs([modfile])] if os.path.exists(modfile) else []
            oline = "（我方该文件**没有**名为 `%s` 的结构体；该文件内 pub struct：%s）" % (
                name, ", ".join(cands) or "无")
        if w is None:
            rows.append([name, "%s（%s）" % (ours_path, oline), "%d 字段" % nf,
                         "E7 @%s：未命中固定偏移窗口" % fmt(anchor), "?", "unknown", "推断 E7", "登记"])
            continue
        cnt, span, lower, reg, offs = w
        # --- E8：上界夹逼（判据见 fixtures/struct_audit_plan3.txt §1.2）---
        u, found = e8_bounds(p, anchor)
        if found:
            e8txt = "；".join("%s **U=0x%x**（%s）" % (src, uu, why) for src, uu, why in found[:3])
            if len(found) > 3:
                e8txt += "；…共 %d 条合格源，取 min" % len(found)
        else:
            callers = sorted({p.ix.func_of(x) for x in p.ix.calls.get(anchor, [])} - {None})
            slots = [sl for sl, ad in p.rel.items() if ad == anchor]
            disp = []
            for sl in slots:
                for rec in p.ix.refs.get(sl, ()):
                    f = p.ix.func_of(rec[0])
                    disp.append("%s@%s" % (fmt(f) if f else "?", fmt(rec[0])))
            e8txt = ("E8a/E8b **无合格源**（直接调用者 %d 个：%s；函数指针槽 %d 个%s；判据要求 U≥下界）" % (
                len(callers), ", ".join(fmt(c) for c in callers[:4]) or "无",
                len(slots), ("（派发点 " + ", ".join(disp[:4]) + "）") if disp else "（连静态槽都没有）")
                + "；E8c 无自夹逼")
        inside = [o for o in offs if u is not None and o < u]
        # --- 判定 ---
        ours_off = _ours_offsets(ofields) if ofields else None
        judge, note, src_tag = "unknown", "", "推断 E7/E8"
        if u is None:
            note = "只有 E7 下界（max 偏移+宽度）；E8 三条源都不成立 ⇒ 上界仍缺（卡点见「二进制证据」列）"
        elif ours_off is not None:
            want = [o for _, o, _ in ours_off]
            if want == inside:
                judge, src_tag = "match", "推断 E7/E8（偏移+宽度档，名字未证）"
                note = "E8 把上界夹到 0x%x：`[0,U)` 内窗口偏移 == 我方字段偏移（%s）" % (
                    u, ",".join(fmt(o) for o in want))
            else:
                judge, src_tag = "mismatch", "推断 E7/E8"
                note = "E8 把上界夹到 0x%x，但 `[0,U)` 内窗口偏移 %s ≠ 我方 %s" % (
                    u, ",".join(fmt(o) for o in inside), ",".join(fmt(o) for o in want))
        else:
            note = ("E8 夹到 **U=0x%x**（下界 0x%x）⇒ 尺寸 ∈ [0x%x, 0x%x]；"
                    "但我方字段里有尺寸表外的类型（%s）⇒ 逐字段偏移算不出，仍需人工比" % (
                        u, lower, lower, u,
                        ", ".join(t for _, t in ofields if t.rstrip(",") not in FIELD_SIZE) or "-"))
        if len(offs) >= 3 and offs[:3] == [0, 8, 0x10] and span >= 0x30:
            note += "；⚠️ 窗口前三个偏移 0x0/0x8/0x10 是 `Vec<T>` 的头（ptr/len/cap）⇒ 锚点很可能在操作某个 `Vec<T>` 的**父结构体**或另一个类型，不是本行这个 T 本身（窗口归属存疑，不许当结论）"
        if nf and inside and nf * 2 <= len(inside) and judge == "unknown":
            # 我方字段数远少于窗口偏移数 ⇒ 窗口几乎不可能是我方这个结构体（多字段/多对象混采）
            note += "；⚠️ 我方只 %d 个字段而窗口有 %d 个偏移 ⇒ 窗口大概率不是该结构体" % (nf, len(inside))
        rows.append([name, "%s（%s）" % (ours_path, oline), "%d 字段：%s" % (
                         nf, ", ".join("%s: %s" % (f, t) for f, t in ofields) if ofields else "?"),
                     "E7 @%s：基址 %s，%d 个不同偏移 [%s]，跨度 %d，下界 **0x%x**；%s" % (
                         fmt(anchor), reg, cnt,
                         ",".join(fmt(o) for o in offs[:16]) + ("…" if len(offs) > 16 else ""),
                         span, lower, e8txt),
                     "窗口下界 0x%x%s" % (lower, ("、上界 **0x%x**" % u) if u is not None else "（上界缺）"),
                     judge, src_tag, note])

    rows.sort(key=lambda r: (r[0].lower(), r[1]))
    head = [
        "# struct_audit2.tsv —— T56 数据结构审计表（第二轮；T66 第三轮**重算**：GameInfo 枚举落地 + 3 处形态 2 处 + E8 上界）",
        "# 重算：../.reference/venv/bin/python tools/struct_audit.py tsv2        只校验：tsv2 --check",
        "# 预注册：fixtures/struct_audit_plan2.txt（判据 E1–E7，第一轮）与 fixtures/struct_audit_plan3.txt（E8 上界 + 本轮改法，先于本表提交）",
        "#",
        "# 列：结构体 | 我方定义位置 | 我方字段集 | 二进制证据(地址→指令/串/单态/跳表) | 二进制字段集 | 判定 | 依据档 | 备注",
        "# 判定 = **当前源码** vs 二进制的状态（判据见预注册 §1，一字未改）。",
        "# 证据档：逐位 = 串/跳表/变体名/单态是编译期常量且 LEN 自校验；观测 = 指令级偏移/写回；推断 = 尺寸上下界（E7 下界 / E8 上界）。",
        "# 地址口径：ELF vaddr（RVA）；Ghidra/REA 地址 = 本表地址 + 0x100000（只加一次）。",
    ]
    text = "\n".join(head + ["\t".join(r) for r in rows]) + "\n"
    path = os.path.join(REPO, "fixtures", "struct_audit2.tsv")
    if args.check:
        old = open(path, encoding="utf-8").read() if os.path.exists(path) else ""
        if old != text:
            print("struct_audit2.tsv 与二进制不一致（重算即可更新）", file=sys.stderr)
            import difflib
            for d in list(difflib.unified_diff(old.splitlines(), text.splitlines(),
                                               "disk", "recomputed", lineterm=""))[:40]:
                print(d)
            return 1
        print("struct_audit2.tsv OK（%d 行数据）" % len(rows))
        return 0
    open(path, "w", encoding="utf-8").write(text)
    print("wrote %s：%d 行" % (os.path.relpath(path, REPO), len(rows)))
    from collections import Counter
    print("判定：", dict(Counter(r[5] for r in rows)))
    return 0



# ---------------------------------------------------------------------------
# 第四轮审计表：fixtures/struct_audit4.tsv（T78）
# ---------------------------------------------------------------------------
# 与 tsv2 的分工：tsv2 回答「**serde 负载/枚举** 对不对得上」；本表回答
# `docs/06` §119.9 留下的 **6 条数据结构遗留**（枚举收敛 / 构造点 / 可达性 /
# 字段数 / 地址字段身份 / 窗口归属）。
#
# 判定列与档位列是**人的事**（本工具不给结论）；但本表里的**每一个地址引文都由
# 工具现场复核**：`tsv4 --check` 会把每条引文的 `(地址, 期望操作数片段)` 重新反汇编
# 一遍，对不上就红。这就是这张表能当证据链用的原因。
#
# 引用格式：`"0x4b7380|A"`… 见 `_cite`；`CITES` 里每条 = (vaddr, 期望出现的操作数/助记符片段)。


def _cite(p, va):
    """反汇编**单条**指令（用于逐条复核引文）。返回 (mnemonic, op_str) 或 None。"""
    try:
        b = p.elf.read(va, 15)
    except Exception:
        return None
    if not b:
        return None
    for ins in p.cs.disasm(b, va):
        return (ins.mnemonic, ins.op_str)
    return None


def _check_cites(p, cites):
    """复核一组引文：每条形如 `"0x4b7380|mov edi, 0x140"`（地址|期望片段）。

    返回 (ok, txt)：ok=False 时 txt 里点名**哪一条**对不上、实际指令是什么。
    """
    ok, bad, shown = True, [], []
    for c in cites:
        a, want = c.split("|", 1)
        got = _cite(p, int(a, 0))
        if got is None:
            ok = False
            bad.append("%s 读不出指令" % a)
            continue
        text = ("%s %s" % got).strip()
        if want.strip() not in text:
            ok = False
            bad.append("%s 期望含 `%s`，实际 `%s`" % (a, want, text))
        shown.append("%s `%s`" % (a, text))
    return ok, ("；".join(bad) if bad else "；".join(shown))


def _width_window(p, va, base=None):
    """同 `_struct_window`，但连**每个偏移的访问宽度**一起给（E7 + 宽度）。

    返回 (基址, ["0x0/8", "0x8/8", …], 下界) 或 None。`base` 指定基址寄存器时只取那一个。
    """
    acc = {}
    for x in _insn_list(p, va):
        if x.mnemonic not in ("mov", "movsd", "movss", "movaps", "movdqu", "movups",
                              "vmovaps", "vmovdqu", "vmovups", "movzx"):
            continue
        for op in x.operands:
            if op.type != X86_OP_MEM or op.mem.base == X86_REG_RIP:
                continue
            reg = x.reg_name(op.mem.base)
            if reg in ("rsp", "rbp", "rip") or not (0 <= op.mem.disp < 0x800):
                continue
            if base and reg != base:
                continue
            acc.setdefault(reg, set()).add((op.mem.disp, op.size))
    if not acc:
        return None
    reg, s = max(acc.items(), key=lambda kv: len(kv[1]))
    offs = sorted(s)
    return (reg, ["0x%x/%d" % (d, w) for d, w in offs], max(d + w for d, w in s))


def cmd_tsv4(p, args):
    """写/校验 `fixtures/struct_audit4.tsv`（T78 第四轮：6 条遗留的结案表）。"""
    import csv  # noqa: F401  （保持与 tsv2 相同的解析口径）
    src = os.path.join(REPO, "src")
    files = [os.path.join(src, f) for f in sorted(os.listdir(src)) if f.endswith(".rs")]

    def enum_defs(name):
        """全 `src/` 里该 `pub enum` 的定义处（= 判「还有没有重复定义」的唯一依据）。"""
        return [loc for nm, loc, _vs in rust_enums(files) if nm == name]

    def struct_of(path, name):
        lay = _ours_layout(os.path.join(REPO, path), name)
        if lay is None:
            return None, "（该文件**没有**名为 `%s` 的结构体）" % name
        loc, fields = lay
        return len(fields), "%s（%s）%d 字段：%s" % (
            loc, name, len(fields), ", ".join("%s: %s" % f for f in fields))

    rows = []
    all_cites = []   # 每条 `0xADDR|期望片段`，在写表/校验前**统一逐条复核**（对不上直接红）

    # ---- ① 枚举收敛（两处）----
    for name, want_one in (("AxisMode", 1), ("TimeMode", 1)):
        defs = enum_defs(name)
        win, _ = _struct_window(p, 0x3CB560 if name == "AxisMode" else 0x3CBCE0), None
        ok = (len(defs) == want_one)
        rows.append([
            "① 枚举收敛 `%s`" % name,
            "定义处 %d 个：%s" % (len(defs), "、".join(defs)),
            "E5/E6：单态 %s + 跳表变体名（tsv2 该行 `match`）；本轮判据只查**定义处个数**"
            % ("0x3cb560" if name == "AxisMode" else "0x3cbce0"),
            "match" if ok else "mismatch",
            "逐位（变体集合/判别值，见 tsv2）" if ok else "—",
            "T78 已收敛：`pub use crate::graphwar_mode_manager::%s;` + 固有项合并（`ALL` 只留一处）"
            % name,
        ])

    # ---- ② GraphwarPlane 构造点（E8b 完整版）----
    nf, ours = struct_of("src/graphwar_plane.rs", "GraphwarPlane")
    all_cites += [
        "0x4b7380|push rbp",           # IControl>::init 入口（函数存在 = 构造点存在）
        "0x420dc7|mov rsi, rsp",
        "0x420dca|call qword ptr [rip + 0x59a960]",
        "0x420dde|mov edx, 0xf8",
        "0x4126df|mov edx, 0x110",
        "0x412704|mov edi, 0x140",
        "0x412709|mov esi, 8",
        "0x412723|add r14, 0x30",
        "0x41272f|mov edx, 0x110",
    ]
    rows.append([
        "② `GraphwarPlane` 构造点 / 尺寸",
        ours or "?",
        "构造链：`IControl>::init` **0x4b7380** ← 槽 `0x9bb730`（唯一引用）← 填实例函数 **0x420d80**"
        "（`lea rdi,[rsp+0x18]` 收 init 返回、按 8+16+`0xf8` 三段拷进 `[rbx]/[rbx+8]/[rbx+0x18]`）"
        "← 调用者 **0x412510 / 0x414ba0**（后者 = `Gd::new_gd` 链）；"
        "`0x412510` 里 `__rust_alloc(0x140, 8)` @0x412704，数据指针 = 返回值+0x30，"
        "随后两次 `memcpy(…, 0x110)` @0x4126df/0x41272f ⇒ **对象数据 0x110 字节 = 272**"
        "（E8b-1 分配立即数 + E8b-2 定长拷贝，两源一致；E7 下界 0xa0 ≤ 0x110 ✓）",
        "观测（E8b 精确尺寸）",
        "观测 E8b-1+E8b-2",
        "精确尺寸 **0x110**；我方 23 字段的**逐字段**边界仍无 E2 字段名证据 ⇒ 只给尺寸，不裁字段",
    ])

    # ---- ③ ShotCalculator ----
    w3 = _width_window(p, 0x3C2BC0)
    rows.append([
        "③ `ShotCalculator` 构造点 / 窗口归属",
        struct_of("src/graphwar_shot_calculator.rs", "ShotCalculator")[1],
        "锚点 = `GraphwarShotCalculator::calculate_shot_results` **0x3c2bc0**（符号桥 `name_to_addr_v2.tsv`）；"
        "**符号桥里它只有 `calculate_*` 与 `drop_in_place`，没有 `new`/`init`** ⇒ 无构造点可追（E8b 卡点）；"
        "派发 = 槽 `0x9bacb0` 2 处（`0x471825` in `0x470eb0`、`0x51a9a1` in `0x51a750`）；"
        "E7 窗口基址 %s 的 `%s`（含 `0x0/0x8/0x10` = `Vec` 头 + 步长 0x20 的成串偏移：`0x30,0x50,…,0xf0`）"
        "⇒ **多对象混采**（数组元素基址），不是单个结构体" % (
            w3[0] if w3 else "?", " ".join(w3[1][:12]) if w3 else "?"),
        "unknown",
        "观测 E7+槽 / 推断 窗口归属",
        "卡点：该类型**没有 `new`/`init` 符号**（arm64 `.symtab` 实证）⇒ 拿不到它的 E8b；"
        "E8c `U=0x148` 是**函数自己的栈工作区**（`rcx=[rsp+0xb0]`），不能当结构体上界。"
        "**线索（未证）**：调用者 `0x470eb0` 里有一处 `__rust_alloc(0x30, 8)` @0x471553"
        "（初始化 `[0]=1/[8]=1/[0x10]dword/[0x14]byte/[0x18]/[0x20]/[0x28]`），"
        "但派发点 @0x471825 的 `rdi=[rsp+0xb8]` 是**栈对象**、且该块存进 `[rsp+0x38]` 后未被装进 `rdi` "
        "⇒ **两者是否同一对象未证**，下一轮从这里查",
    ])

    # ---- ④ AxisDrawer / GraphicsManager 可达性（正对照）----
    all_cites += [
        "0x471553|mov edi, 0x30",      # ③ 的**线索**（未证）：调用者里的 0x30 分配
        "0x4b5df0|push rbp",           # 正对照锚点（GraphwarPlane::draw）入口
        "0x4e05b0|push rbp",           # AxisDrawer::IControl::draw 入口
        "0x4e01f0|push rbx",           # AxisDrawer 的 __virtual_call::virtual_fn thunk 入口
        "0x469390|push r15",           # GraphwarGraphicsManager::new 入口
    ]
    rows.append([
        "④ `AxisDrawer` / `GraphicsManager` 可达性",
        "`AxisDrawer`（`src/_scaffold/gdext_mod.rs:248`，gdext 类）/ "
        "`GraphicsManager`（我方**没有**该结构体；有 `GraphwarGraphicsManager`）",
        "**正对照（先做）**：`addr_probe.py callers 0x4b5df0` ⇒ 槽 `0x9bb7f0` 1 个使用点 **0x42937a**"
        "（= T66 §119.4 已知派发点）⇒ 扫描器有效。"
        "`AxisDrawer` 是 **Godot 类**：`IControl>::draw` **0x4e05b0**、`__register_methods` **0x3e9a30**、"
        "`ImplementsGodotVirtual::__virtual_call::virtual_fn` **0x4e01f0**（体内是 `AxisDrawer`/`process`/"
        "`upcast_ref` 名字派发器）⇒ 引擎经 `virtual_fn` 回调，**静态 `.data.rel.ro` 槽上本来就没有它**。"
        "`GraphwarGraphicsManager::new` **0x469390**（普通结构体）有 2 个直接调用者：`0x451303` in `0x4500c0`、"
        "`0x517d73` in `0x517690` ⇒ 「0 可直接调用」只针对 E7 锚点 `0x468d80`，不是这个类整体",
        "match（可达性已证）",
        "观测（指令/槽）",
        "T66 的「0 直接调用 + 0 槽」是**扫描边界**造成的假象（只扫了 527 个 Location 锚函数）；"
        "本轮正对照通过后再下结论",
    ])

    # ---- ⑤ GraphwarGameRecorder ----
    nf5, ours5 = struct_of("src/graphwar_game_recorder.rs", "GraphwarGameRecorder")
    w5 = _width_window(p, 0x3D5260)
    all_cites += [
        "0x3d528c|mov edi, 0x50",      # E8b-1：__rust_alloc(0x50, 8) ⇒ 对象恰好 0x50
        "0x3d5296|call qword ptr [rip + 0x5e5394]",
        "0x3d50a7|lea rcx, [r13*4]",   # EventInfo 步长 ×5
        "0x3d50af|add rcx, r13",
        "0x3d50b2|shl rcx, 4",         # ×16 ⇒ 元素 0x50
        "0x3cbac7|cmp byte ptr [rdi + 8], 1",   # address 单态的 1 字节 tag
        "0x3cbadd|mov byte ptr [rax + rsi], 0x2c",  # 追加 ','
    ]
    rows.append([
        "⑤ `GraphwarGameRecorder` 字段/偏移",
        ours5 or "?",
        "**E8b-1 精确尺寸**：`new` **0x3d5260** 开头 `mov edi,0x50 / mov esi,8 / call __rust_alloc`"
        "（@0x3d528c..0x3d5296）⇒ 对象 = **0x50 字节**；同一 `rax` 上的窗口是 "
        "`%s`（基址 %s）⇒ **8 个 qword(0x00..0x40) + 1 个 byte@0x48**（0x48+1=0x49 ≤ 0x50 ✓ 自洽）；"
        "E7 锚点 `0x3d4fa0`（`save_event`）只碰 "
        "`0x0/0x8/0x10`(Vec₁) + `0x20/0x28` + `0x48`；元素步长 **0x50**（`lea rcx,[r13*4]` + "
        "`add rcx,r13` + `shl rcx,4` @0x3d50a7..0x3d50b2 ⇒ ×5×16）⇒ `EventInfo` 0x50 字节" % (
            w5[0] if w5 else "?", " ".join(w5[1]) if w5 else "?"),
        "mismatch",
        "观测（E8b-1 精确尺寸 + E7+宽度，指令级）",
        "我方 `events: Vec`(0x0) / `last_state: u8`(**0x18**) / `subscribed: Vec`(0x20) / `saves: usize`(0x38) = **0x40**；"
        "原版是 **3 个 24 字节字段(0x0/0x18/0x30) + 1 个 byte@0x48** = **0x50** ⇒ 我方 `last_state` 错位到 0x18"
        "（我方自己的文档注释写的是「原版 `+0x48`」，与字段顺序**自相矛盾**）+ 多一个 `saves`、少一个 24 字节字段。"
        "尺寸已精确（E8b-1 = 0x50，与 10 槽/宽度自洽）；下一步是**逐字段语义**（3 个 24 字节字段各是什么、"
        "`last_state` 归位到 0x48）",
    ])

    # ---- ⑥ address 身份 ----
    rows.append([
        "⑥ `GameInfo.address` 身份",
        "`src/events.rs` 的 `GameInfo::address: String`（24B）",
        "E3 `layout 0x5060d0`：`address` off=**0x18**、类型类单态 **0x3cbab0**（`layout` 判「未命中已知指纹」）；"
        "0x18 起的下一个字段 `current_player_count` off=**0x38** ⇒ 占 **0x18..0x38 = 32 字节**（E3 逐位）；"
        "单态体 **0x3cbab0**：`cmp byte ptr [rdi+8], 1`（**1 字节判别/状态**）、"
        "`mov byte ptr [rax+rsi], 0x2c`（向 String 追加 `','` 分隔符）、`mov byte ptr [rdi+8], 2`"
        "⇒ 被序列化的是一个**带 1 字节 tag 的复合值**（不是裸 `String`）",
        "推断",
        "观测（形状）+ 推断（身份）",
        "身份 `SocketAddr` **仍未证**：缺 ① x86 侧 `core::net::socket_addr::*` 的名字映射"
        "（arm64 `.symtab` 有这些名，但现成符号桥 `sym_match.py` 只匹配提到 `graphwarrust` 的符号，"
        "扩它会变成第二个工具 ⇒ 本轮不做）；② 构造点把 32 字节当整体读写的指令（两个已知 GameInfo 调用者"
        "`0x3cbdb0`/`0x457790` 里都没有 32 字节整体写）⇒ **不许写成结论**",
    ])

    # ---- ⑦ PatternSegment 窗口归属 ----
    w7 = _width_window(p, 0x466D20, base="rax")
    rows.append([
        "⑦ `PatternSegment` 窗口归属",
        struct_of("src/graphwar_level_generator.rs", "PatternSegment")[1],
        "锚点 `0x466d20` = `GraphwarLevelGenerator::generate_level_custom`（`name_to_addr_v2.tsv`）；"
        "基址 rax 的窗口 `%s` ⇒ 前三个是 **`Vec<T>` 头(ptr/len/cap)**、第 4 个是 **8 字节** @0x48；"
        "E8a：三个调用者 `lea rdi,[rsp+0x68]`（frame `0xb8`）⇒ `U=0x50`，且 `0x68+0x50=0xb8` = **整帧尾部**"
        " ⇒ 被传进来的是**一个 0x50 字节、以 `Vec` 开头的父结构体**" % (" ".join(w7[1]) if w7 else "?"),
        "unknown",
        "观测（E7+宽度+E8a，指令级）",
        "⇒ **窗口归属证伪**：这个窗口不是 `PatternSegment`（7 个数值字段、无 `Vec`）⇒ T66 的 `mismatch` "
        "**作废**，改判 `unknown`；`PatternSegment` 自身布局仍无独立证据（元素步长没找到："
        "`generate_level_custom`/`generate_level::{{closure}}` 里没有 `×0x30` 的 stride 模式）",
    ])

    head = [
        "# struct_audit4.tsv —— T78 数据结构审计表（第四轮：`docs/06` §119.9 的 6 条遗留）",
        "# 重算/校验：`../.reference/venv/bin/python tools/struct_audit.py tsv4` / `tsv4 --check`",
        "# 预注册：fixtures/struct_audit_plan4.txt（判据 E1–E8 + 本轮 E8b 完整版 + 期望值，先于本表提交）",
        "#",
        "# 列：目标 | 我方 | 二进制证据（地址→指令/槽/单态） | 判定 | 依据档 | 卡点/下一步",
        "# 判定口径：match = 两边可复核地一致；mismatch = 有指令级证据且不一致；unknown = 只拿到部分证据；",
        "# 档位：逐位 = 编译期常量逐项校验；观测 = 指令级（地址可复跑）；推断 = 有依据未证。",
        "# 地址口径：ELF vaddr（RVA）；Ghidra/REA 地址 = 本表地址 + 0x100000（只加一次）。",
        "# `tsv4 --check` 会把「二进制证据」列里带 `0x…|片段` 的引文**逐条重新反汇编复核**（见 tools/struct_audit.py 的 _check_cites）。",
    ]
    cite_ok, cite_txt = _check_cites(p, all_cites)
    if not cite_ok:
        print("❌ 引文复核失败（本表的地址引文必须逐条能重新反汇编出期望片段）：", file=sys.stderr)
        print("   " + cite_txt, file=sys.stderr)
        return 1
    text = "\n".join(head + ["\t".join(r) for r in rows]) + "\n"
    path = os.path.join(REPO, "fixtures", "struct_audit4.tsv")
    if args.check:
        old = open(path, encoding="utf-8").read() if os.path.exists(path) else ""
        if old != text:
            print("struct_audit4.tsv 与二进制不一致（重算即可更新）", file=sys.stderr)
            import difflib
            for d in list(difflib.unified_diff(old.splitlines(), text.splitlines(),
                                               "disk", "recomputed", lineterm=""))[:40]:
                print(d)
            return 1
        print("struct_audit4.tsv OK（%d 行数据；%d 条地址引文逐条复核通过）" % (len(rows), len(all_cites)))
        return 0
    open(path, "w", encoding="utf-8").write(text)
    print("wrote %s：%d 行（%d 条地址引文逐条复核通过）" % (
        os.path.relpath(path, REPO), len(rows), len(all_cites)))
    from collections import Counter
    print("判定：", dict(Counter(r[3] for r in rows)))
    return 0


# ---------------------------------------------------------------------------
# E8 上界夹逼（T66；判据预先写死在 fixtures/struct_audit_plan3.txt §1.2，跑完不许改）
# ---------------------------------------------------------------------------
# 三条源，全部**指令级**，且都只证明"某块缓冲有多大"，不证明"那就是这个结构体"⇒ 推断：
#   E8a 调用方栈缓冲：调 f 时 rdi = lea [rsp+k] ⇒ U = 调用方 frame - k
#   E8b 分配尺寸立即数：调 f 时 rdi 来自 rax，而 rax 来自 `mov e?x,IMM; call <alloc>` ⇒ U = IMM
#   E8c 同函数栈窗口：f 里 `lea/mov <窗口基址>,[rsp+k]` ⇒ U = frame - k
# 拒绝规则：U < E7 下界 ⇒ 该源不成立（缓冲不是这个结构体）；多条取 min。
FRAME_MNEMS = ("sub",)


def _e8_dataflow(p, f, call_targets, dispatch_addrs=frozenset()):
    """在 f 里做**直线**数据流：reg → ("stack", k) / ("heap_imm", n) / ("mem", 0)。

    ⚠️ 跨 `call/jmp/j*/ret` 一律清空（陈旧值会伪造夹逼）；因此只会**漏**，不会凭空造。
    返回 (frame, [(call_addr, target, rdi_state)], {basereg: ("stack", k)})。
    """
    ins = _insn_list(p, f)
    if not ins:
        return 0, [], {}
    frame = 0
    for x in ins:
        if x.mnemonic in FRAME_MNEMS and x.op_str.startswith("rsp,"):
            im = _imms(x)
            if im:
                frame = max(frame, im[0])
    state, out, basestack, prev = {}, [], {}, []
    for x in ins:
        if x.mnemonic == "lea" and len(x.operands) == 2 and x.operands[1].type == X86_OP_MEM \
                and x.operands[1].mem.base == X86_REG_RSP:
            state[x.reg_name(x.operands[0].reg)] = ("stack", x.operands[1].mem.disp)
        elif x.mnemonic == "mov" and len(x.operands) == 2 and x.operands[0].type == X86_OP_REG:
            dst, src = x.reg_name(x.operands[0].reg), x.operands[1]
            if src.type == X86_OP_REG and x.reg_name(src.reg) in state:
                state[dst] = state[x.reg_name(src.reg)]
            elif src.type == X86_OP_IMM:
                state[dst] = ("imm", int(src.imm))
            elif src.type == X86_OP_MEM:
                state[dst] = ("mem", 0)
            else:
                state.pop(dst, None)
        elif x.mnemonic in ("add", "sub") and len(x.operands) == 2 \
                and x.operands[0].type == X86_OP_REG:
            reg = x.reg_name(x.operands[0].reg)
            im = _imms(x)
            if im and reg in state and state[reg][0] == "stack":
                k = state[reg][1] + (im[0] if x.mnemonic == "add" else -im[0])
                state[reg] = ("stack", k)
            else:
                state.pop(reg, None)
        if x.mnemonic == "call":
            # E8b：紧邻 ≤6 条里有 `mov e?x/edi/esi/edx/ecx/r8d, IMM` ⇒ 分配尺寸立即数
            size = None
            for y in reversed(prev[-6:]):
                if y.mnemonic == "mov" and len(y.operands) == 2 and y.operands[0].type == X86_OP_REG:
                    rn = y.reg_name(y.operands[0].reg)
                    if rn in ("edi", "esi", "edx", "ecx", "r8d", "r9d") and y.operands[1].type == X86_OP_IMM:
                        size = int(y.operands[1].imm)
                        break
            if size is not None:
                state["rax"] = ("heap_imm", size)
            # 目标：`call rel32` 的立即数在 call_targets 里
            im = _imms(x)
            if (im and im[0] in call_targets) or (x.address in dispatch_addrs):
                out.append((x.address, im[0] if im else 0, state.get("rdi"), state.get("rcx"),
                            state.get("rdx"), state.get("r8")))
        if x.mnemonic.startswith(("ret", "call", "jmp", "j", "loop")):
            state.clear()
            state = {}
        prev.append(x)
    return frame, out, basestack


def e8_bounds(p, anchor):
    """E8：返回 (U 或 None, [(源, U, 说明)])。判据见 fixtures/struct_audit_plan3.txt §1.2。"""
    w = _struct_window(p, anchor)
    lower = w[2] if w else 0
    found = []
    # 直接 `call rel32` 的调用者 + **函数指针槽派发**的调用者（T66 第二轮：
    # 3 个类 0 个直接调用者，走 `.data.rel.ro` 的 R_X86_64_RELATIVE 槽 ⇒ 那条链也要算进来；
    # 这就是 `tools/ptr_slots.py` 的判据，只是搬进本工具、不再另起一套）
    callers = sorted({p.ix.func_of(s) for s in p.ix.calls.get(anchor, [])} - {None})
    slots = [sl for sl, ad in p.rel.items() if ad == anchor]
    dispatch = set()
    for sl in slots:
        for rec in p.ix.refs.get(sl, ()):
            dispatch.add(rec[0])
            f = p.ix.func_of(rec[0])
            if f is not None:
                callers.append(f)
    callers = sorted(set(callers))
    targets = {anchor, 0}
    for f in callers:
        frame, calls, _ = _e8_dataflow(p, f, targets, frozenset(dispatch))
        for (addr, tgt, rdi, rcx, rdx, r8) in calls:
            if tgt != anchor and addr not in dispatch:
                continue
            if rdi is not None and rdi[0] == "stack":
                u = frame - rdi[1]
                if 0 < u >= lower:
                    found.append(("E8a", u, "caller %s @%s: frame 0x%x, rdi=[rsp+0x%x]"
                                  % (fmt(f), fmt(addr), frame, rdi[1])))
            if rdi is not None and rdi[0] == "heap_imm":
                u = rdi[1]
                if 0 < u >= lower:
                    found.append(("E8b", u, "caller %s @%s: 分配立即数 0x%x" % (fmt(f), fmt(addr), u)))
    # E8c：锚点函数自己把窗口基址指向栈
    if w is not None:
        base = w[3]
        ins = _insn_list(p, anchor)
        frame = 0
        for x in ins:
            if x.mnemonic in FRAME_MNEMS and x.op_str.startswith("rsp,"):
                im = _imms(x)
                if im:
                    frame = max(frame, im[0])
        for x in ins:
            if x.mnemonic == "lea" and len(x.operands) == 2 \
                    and x.operands[0].type == X86_OP_REG and x.reg_name(x.operands[0].reg) == base \
                    and x.operands[1].type == X86_OP_MEM and x.operands[1].mem.base == X86_REG_RSP:
                u = frame - x.operands[1].mem.disp
                if 0 < u >= lower:
                    found.append(("E8c", u, "self @%s: %s=[rsp+0x%x], frame 0x%x"
                                  % (fmt(x.address), base, x.operands[1].mem.disp, frame)))
    found.sort(key=lambda r: r[1])
    return (found[0][1] if found else None), found


# 我方 Rust 结构体字段偏移估算（只认标量 + 少数已知尺寸；未知类型 ⇒ 返回 None，不猜）
FIELD_SIZE = {
    "f64": 8, "f32": 4, "u64": 8, "i64": 8, "usize": 8, "isize": 8,
    "u32": 4, "i32": 4, "bool": 1, "u8": 1, "i8": 1, "u16": 2, "i16": 2,
    "String": 24, "Vec<EventInfo>": 24, "Vec<&'static str>": 24,
}


def _ours_offsets(fields):
    """[(field, offset, size)]；任一字段类型没有尺寸表项 ⇒ None（不猜）。"""
    off, out = 0, []
    for name, ty in fields:
        ty = ty.strip().rstrip(",")
        sz = FIELD_SIZE.get(ty)
        if sz is None:
            return None
        off = (off + sz - 1) // sz * sz if sz < 8 else (off + 7) // 8 * 8
        out.append((name, off, sz))
        off += sz
    return out


def cmd_bounds(p, args):
    """E8 上界夹逼（判据：fixtures/struct_audit_plan3.txt §1.2，一字未改）。"""
    rc = 0
    for a in args.addr:
        va = int(a, 0)
        w = _struct_window(p, va)
        if w is None:
            print("%-10s 无 E7 窗口" % fmt(va))
            continue
        cnt, span, lower, reg, offs = w
        u, found = e8_bounds(p, va)
        print("== %s  基址 %s  %d 偏移  跨度 %d  下界 0x%x" % (fmt(va), reg, cnt, span, lower))
        for src, uu, why in found:
            print("   %s  U=0x%-4x  %s" % (src, uu, why))
        if u is None:
            callers = sorted({p.ix.func_of(s) for s in p.ix.calls.get(va, [])} - {None})
            print("   ⇒ 无合格夹逼（直接调用者 %d 个：%s）" % (
                len(callers), ", ".join(fmt(c) for c in callers[:6]) or "无"))
            rc = 1
        else:
            inside = [o for o in offs if o < u]
            print("   ⇒ U=0x%x；[0,U) 内偏移 %s" % (u, ",".join(fmt(o) for o in inside)))
    return 0

def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--so", default=SO_DEFAULT)
    sub = ap.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("serde"); s.add_argument("--json", action="store_true")
    s.set_defaults(fn=cmd_serde)

    s = sub.add_parser("fields"); s.add_argument("name")
    s.add_argument("--offsets", action="store_true"); s.add_argument("--dis", action="store_true")
    s.set_defaults(fn=cmd_fields)

    s = sub.add_parser("tables"); s.set_defaults(fn=cmd_tables)

    s = sub.add_parser("tsv"); s.add_argument("--check", action="store_true")
    s.set_defaults(fn=cmd_tsv)

    s = sub.add_parser("rust"); s.add_argument("path", nargs="*")
    s.set_defaults(fn=cmd_rust)

    s = sub.add_parser("check"); s.add_argument("--plan")
    s.set_defaults(fn=cmd_check)

    s = sub.add_parser("mono"); s.add_argument("addr", nargs="+")
    s.set_defaults(fn=cmd_mono)

    s = sub.add_parser("layout"); s.add_argument("addr", nargs="+")
    s.add_argument("--json", action="store_true")
    s.set_defaults(fn=cmd_layout)

    s = sub.add_parser("enums"); s.add_argument("addr", nargs="+")
    s.set_defaults(fn=cmd_enums)

    s = sub.add_parser("tsv2"); s.add_argument("--check", action="store_true")
    s.set_defaults(fn=cmd_tsv2)
    s = sub.add_parser("tsv4"); s.add_argument("--check", action="store_true")
    s.set_defaults(fn=cmd_tsv4)

    s = sub.add_parser("bounds")
    s.add_argument("addr", nargs="+")
    s.set_defaults(fn=cmd_bounds)
    s = sub.add_parser("structs")
    s.add_argument("--min-off", type=int, default=3)
    s.add_argument("--limit", type=int, default=0)
    s.add_argument("--tsv", action="store_true")
    s.set_defaults(fn=cmd_structs)

    a = ap.parse_args(argv)
    p = Probe(so=a.so)
    return a.fn(p, a)




# ---------------------------------------------------------------------------
# 审计表：fixtures/struct_audit.tsv
# ---------------------------------------------------------------------------
# `Serialize` 单态 → 类型类。判据：**同单态地址 = 同类型**；不同地址 = 不同类型。
# 枚举各自一个单态（`AxisMode`/`FunctionMode`/`TurnMode`/`TimeMode`/`GameState`/
# `GraphwarTeam`/`EffectType` 各一个），所以"枚举字段"是可判定的、不是猜的。
MONO_CLASS = {
    0x3cc060: "整数族 serialize（i64/u64/usize 单态）",
    0x3cb250: "String",
    0x3cbab0: "另一个串类型（与 String 单态不同）",
    0x3cb760: "f64",
    0x3cbb80: "bool",
    0x3cc4b0: "Option<i64>",
    0x3cb440: "Option<String>",
    0x3cb330: "enum EffectType",
    0x3cb560: "enum AxisMode",
    0x3cb670: "enum FunctionMode",
    0x3cbf70: "enum TurnMode",
    0x3cbce0: "enum TimeMode",
    0x3cc260: "enum GameState",
    0x3cbe80: "enum GraphwarTeam",
    0x3cb9e0: "GraphwarColor",
    0x3cbdb0: "GameInfo",
}
# 我方 `ValType` → 与二进制类型类的相容集合（ValType 只描述**线上值的形状**）
VALUETYPE_OK = {
    "I64": {"整数族 serialize（i64/u64/usize 单态）"},
    "U8": {"u8"},
    "F64": {"f64"},
    "Bool": {"bool"},
    "Str": {"String", "另一个串类型（与 String 单态不同）",
            "enum AxisMode", "enum FunctionMode", "enum TurnMode", "enum TimeMode",
            "enum GameState", "enum GraphwarTeam", "enum EffectType"},
    "OptI64": {"Option<i64>"},
    "OptStr": {"Option<String>"},
    "Color": {"GraphwarColor"},
    "GameInfo": {"GameInfo"},
}


def _wire_spec():
    """我方 `WIRE_SPEC`（`src/_scaffold/protocol.rs`）：{事件名: (行号, [(字段, ValType)])}"""
    path = os.path.join(REPO, "src", "_scaffold", "protocol.rs")
    out = {}
    for i, ln in enumerate(open(path, encoding="utf-8").read().splitlines(), 1):
        m = re.match(r'\s*WireEvent \{ name: "([A-Za-z0-9_]+)", tag: ([^,]+), '
                     r'fields: &\[(.*?)\] \},', ln)
        if not m:
            continue
        fs = re.findall(r'\("([a-z0-9_]+)", ValType::([A-Za-z0-9_]+)\)', m.group(3))
        out[m.group(1)] = (i, fs)
    return out


def _wire_fields_json(p):
    """调既有工具 `tools/wire_fields.py --json`（它自带 N / 长度 / 反序列化三条自校验）。"""
    import subprocess
    import tempfile
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as tf:
        path = tf.name
    try:
        r = subprocess.run([sys.executable, os.path.join(HERE, "wire_fields.py"), "--json"],
                           capture_output=True, text=True)
        if r.returncode != 0:
            raise SystemExit("wire_fields.py --json 失败：\n" + r.stdout[-2000:] + r.stderr[-2000:])
        return json.loads(r.stdout)
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass


def _payload_rows(p, rows):
    """3 个负载结构体（不是事件）：`GameInfo` / `GraphwarColor` / `EventInfo`。

    前两个有独立的 `Serialize` 实现（地址在下面写死，**每次跑都现场复核键序**）；
    `EventInfo` 没找到 Serialize 实现，只能从 `Deserialize` visitor 的
    `missing field` / `duplicate field` 路径读出字段名 ⇒ 顺序与宽度都**未知**。
    """
    PAYLOAD_SER = {"GraphwarColor": 0x464c00, "GameInfo": 0x5060d0}
    for name, f in PAYLOAD_SER.items():
        tri = p.ser_triples(f)
        names = [t[0] for t in tri]
        ev = "E2 ser=%s: %s" % (fmt(f), ", ".join(
            "%s@%s(mono=%s)" % (t[0], fmt(t[1]), fmt(t[4]) if t[4] else "?") for t in tri))
        if name == "GameInfo":
            ours = "src/_scaffold/protocol.rs:1844（`GameInfo`）+ src/events.rs:356（同名第二处）"
            ofields = ("room_name,g ame_state,current_player_count,address,axis_mode,"
                       "function_mode,turn_mode,time_mode,locked").replace("g ame", "game")
            note = ("**类型不一致（登记不改）**：二进制 5 个枚举字段（game_state=enum GameState、"
                    "axis_mode/function_mode/turn_mode/time_mode 各一个独立单态）+ "
                    "current_player_count=整数族 + address 与 room_name **不是同一个串类型**；"
                    "我方两处定义分别用 String/u8/… ⇒ 源码级差异")
            judge = "match" if len(names) == 9 else "mismatch"
        else:
            ours = ("src/_scaffold/protocol.rs:1809 与 src/graphwar_color_manager.rs:261"
                    "（同名两处，字段一致）")
            ofields = "r,g,b（u8×3，偏移 0/1/2）"
            note = "三字段与顺序、类型（u8 单态 0x3cc330）逐一相同；我方两处定义都是 r,g,b: u8"
            judge = "match"
        rows.append([name, ours, ofields, ev, "%d 字段" % len(names), judge, "逐位 E1+E2+E3", note])
    # EventInfo：只能从 visitor 读
    k, sva, slot = [(kk, v, s) for n, kk, v, s in p.serde_names() if n == "EventInfo"][0]
    vis = []
    for r in p.refs_to(sva):
        f, _ = p.func_of(r)
        if f is not None:
            vis.append((f, p.visitor_names(f)))
    names = []
    for f, vn in vis:
        for n, va, L in vn:
            if n not in [x[0] for x in names]:
                names.append((n, va, L))
    first = vis[0][0] if vis else None
    rows.append(["EventInfo", "(我方没有这个结构体)", "—",
                 "E1 slot=%s→%s N=%d; E2 Deserialize visitor %s 读到 %s" % (
                     fmt(slot), fmt(sva), k, fmt(first) if first else "?",
                     ", ".join("%s@%s(len=%d)" % (n, fmt(v), L) for n, v, L in names) or "（无）"),
                 "%d 字段：%s（**顺序与宽度未知**）" % (k, ",".join(n for n, _, _ in names)),
                 "mismatch", "逐位 E1 + 观测 E2(仅集合)",
                 "**我方缺这个结构体**；处置=登记。缺的证据：它的 `Serialize` 实现（全 .text 里"
                 "没有函数按 `lea rsi/mov edx/.../call` 形状序列化 `event`+`tick`），"
                 "⇒ 顺序与字段宽度要 frida 在构造点取样才能定"])


def _ours_fields(path, name):
    """从我方源码里读某个 `pub struct` 的字段名（顺序）——用 `tools/struct_audit.py rust` 的同一个解析器。"""
    for nm, loc, fields in rust_structs([os.path.join(REPO, path)]):
        if nm == name:
            return loc.split(":")[-1], [f for f, _ in fields]
    return None, []


def _layout_rows(p, rows):
    """非 serde 结构体：`Evaluation` / `Individual`（sret + 固定偏移证据）+ 1 条 unknown。"""
    ev_path = "src/graphwar_function_evolutionator.rs"
    ev_line, ev_fields = _ours_fields(ev_path, "Evaluation")
    ind_line, ind_fields = _ours_fields(ev_path, "Individual")
    ev_ok = ev_fields == ["score", "dist", "hits", "esi"]
    ind_ok = ind_fields == ["tokens", "score", "dist", "hits", "esi"]
    rows.append([
        "Evaluation",
        "%s:%s" % (ev_path, ev_line),
        ",".join("%s" % f for f in ev_fields) + "（24 字节）",
        "E4 sret 写回 0x51b10d[rcx]=score / 0x51b11d[rcx+8]=dist / 0x51b111[rcx+0x10]=hits / "
        "0x51b114[rcx+0x14]=esi；上界：调用点 0x51bc93 之后只拷 24 字节（0x51bce5+0x51bcf4）",
        "24 字节：f64@0, f64@8, u32@0x10, u32@0x14",
        "match" if ev_ok else "mismatch", "逐位 E4(尺寸上下界夹逼) + 观测 E3(写回偏移)",
        "**本轮已修（审计发现=mismatch，改后=match）**：改前只有 3 字段（score/dist/hits，"
        "20 字节逻辑大小，靠 f64 的 8 字节对齐被补到 24 ⇒ **大小碰巧对、字段数不对**）。"
        "补上的第 4 字段 `esi`：语义 = 开火结果项 `payload>>32` 的累加器（`0x51acc8` 唯一累加点，"
        "T28 §78.3），本实现恒 0、行为影响 0。struct_size/offset 已加断言钉住"])
    rows.append([
        "Individual",
        "%s:%s" % (ev_path, ind_line),
        ",".join("%s" % f for f in ind_fields) + "（48 字节）",
        "E3 步长 `rcx = n*3*16`（= n*48，0x51bccf）；评分四元组拷到 +0x18/+0x20/+0x28"
        "（0x51bce5 movdqu → +0x18；0x51bcf4/0x51bcfc → +0x28）",
        "48 字节：Vec@0(24), f64@0x18, f64@0x20, u32@0x28, u32@0x2c",
        "match" if ind_ok else "mismatch", "逐位 E3(偏移) 复核 + 我方 layout 断言",
        "**本轮补齐**第 4 个评分字段 `esi`（改前 +0x28 只放 hits、+0x2c 空着；"
        "因对齐大小恰好也是 48 ⇒ 只看 sizeof 发现不了）" + ("" if ind_ok else "｜⚠️ 现有字段序列不对")])
    rows.append([
        "ScoringContext（我方）",
        "src/graphwar_function_evolutionator.rs（`ScoringContext`）",
        "step:f64, mask:&[u8], targets:&[Target], limit:u32",
        "E3 调用点 0x51bc7a-0x51bc93：评分核心 `0x51a750` 是**自由函数**，参数是 "
        "`rdi`=sret、`rsi`=[rsp+0x50]、`rdx`=[rsp+0x10]、`ecx`=[rsp+0x4c]、`xmm0`=步长常量 ⇒ "
        "**没有任何「上下文结构体」被传进去**",
        "无结构体：4 个独立参数（2 指针 + 1 u32 + 1 f64）+ sret",
        "unknown", "推断 E3（只能证明「没传结构体」）",
        "**源码级差异，只登记**：我方把这 4 个参数打包成 `ScoringContext` 结构体。"
        "缺的证据：二进制无法证明原版「没有」这个结构体（可能只是被内联/拆成参数），"
        "要 frida 在构造点看栈/寄存器才能定 ⇒ 标 unknown"])


def cmd_tsv(p, args):
    rows = []
    ours = _wire_spec()
    wf = _wire_fields_json(p)
    e1 = {n: (k, v, s) for n, k, v, s in p.serde_names()}
    for e in wf["events"]:
        wname = e["event"]
        sname = e["struct"]
        nl, ofields = ours.get(wname, (None, []))
        onames = [f for f, _ in ofields]
        bnames = list(e["fields"])
        monos = e["field_monos"]
        bad_type = []
        for (f, t), mo in zip(ofields, monos):
            cls = MONO_CLASS.get(int(mo, 16) if isinstance(mo, str) else mo, "?")
            if cls != "?" and cls not in VALUETYPE_OK.get(t, set()):
                bad_type.append("%s:我方%s≠%s" % (f, t, cls))
        k, sva, slot = e1.get(sname, (e["n"], None, None))
        e1txt = ("E1 无串（0 字段结构体走 `expecting` 短形式）" if sva is None
                 else "E1 slot=%s→%s N=%d" % (fmt(slot), fmt(sva), e["n"]))
        judge = "match" if (len(onames) == len(bnames) == e["n"] and onames == bnames
                            and not bad_type) else "mismatch"
        note = ""
        if bad_type:
            note = "类型冲突：" + "; ".join(bad_type)
        elif not bnames:
            note = "空体事件：没有 `struct … with N elements` 串（0 字段结构体走 `expecting` 短形式）"
        rows.append([sname, "src/_scaffold/protocol.rs:%s（WIRE_SPEC/%s）" % (nl, wname),
                     ",".join(onames) or "（0 字段）",
                     "%s; E2 ser=%s: %s" % (
                         e1txt, fmt(e["func"]),
                         ", ".join("%s@%s" % (f, fmt(v)) for f, v in zip(bnames, e["field_vas"]))),
                     "%d 字段: %s" % (e["n"], ", ".join(bnames) or "（0 字段）"),
                     judge, "逐位 E1+E2（wire_fields --verify 自校验通过）", note])
    _payload_rows(p, rows)
    _layout_rows(p, rows)
    rows.sort(key=lambda r: (r[0].lower(), r[1]))

    head = [
        "# struct_audit.tsv —— T32 数据结构审计表",
        "# 重算：../.reference/venv/bin/python tools/struct_audit.py tsv           只校验：tsv --check",
        "# 预注册（判据 + 清单 + 我方字段数）：fixtures/struct_audit_plan.txt（先于本表提交）",
        "#",
        "# 列：结构体 | 我方定义位置 | 我方字段集 | 二进制证据(地址→指令/串/单态) | 二进制字段集 | 判定 | 依据档 | 备注",
        "# 判定 = **当前源码** vs 二进制的状态（按预注册：E1 的 N == 我方字段数 且 E2 的字段名顺序",
        "# 与我方一致 ⇒ match）。**本轮审计发现的偏差**写在备注：已修的会注明「审计发现=mismatch，改后=match」。",
        "# 类型（E3 单态）**另设一列备注**，不参与 match/mismatch 判定（预注册里它是 E3 档）。",
        "# 证据等级：逐位 = 串/表/指令里的编译期常量逐字节复核；观测 = 指令级偏移；推断 = 只有尺寸下限。",
        "# 地址口径：ELF vaddr（RVA）。Ghidra/REA 地址 = 本表地址 + 0x100000（只加一次）。",
    ]
    text = "\n".join(head + ["\t".join(r) for r in rows]) + "\n"
    path = os.path.join(REPO, "fixtures", "struct_audit.tsv")
    if args.check:
        old = open(path, encoding="utf-8").read()
        if old != text:
            print("struct_audit.tsv 与二进制不一致（重算即可更新）", file=sys.stderr)
            import difflib
            for d in list(difflib.unified_diff(old.splitlines(), text.splitlines(),
                                               "disk", "recomputed", lineterm=""))[:60]:
                print(d)
            return 1
        print("struct_audit.tsv OK（%d 行数据）" % len(rows))
        return 0
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(text)
    print("wrote %s：%d 行" % (os.path.relpath(path, REPO), len(rows)))
    from collections import Counter
    print("判定：", dict(Counter(r[5] for r in rows)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
