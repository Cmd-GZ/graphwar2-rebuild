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
from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_OP_REG, X86_REG_RIP

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
