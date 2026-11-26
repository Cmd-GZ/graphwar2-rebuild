#!/usr/bin/env python3
"""逐事件重数 `Event` 的字段数 —— `docs/07` §3.2 第 ① 号验证的重做（T16 / task-13）。

## 这份工具要解决什么

`docs/07` §3.2 曾声称：**「字段个数 == `.rodata` 里 `struct <Name>Event with N elements` 的 N」
= 67/67**，并说「N 串只有 67 条，另外 7 个事件的 `Serialize` 一次 `serialize_field` 都不调」。
本条线（T16）实测这三个数**都对不上**，本工具用**两种互相独立的方法**逐事件重数：

* **方法 A（Deserialize 侧，指令级）**：serde derive 生成的
  `Deserializer::deserialize_struct(self, name, fields, visitor)` 参数块里，
  `fields.len`（= 结构体声明的字段数）是**编译期立即数**：

  ```
  # 布局 S —— 有字段事件（Result 大 ⇒ sret 在 rdi）
  lea  rdx, [rip+name]      ; &str.ptr    name == "<Name>Event"
  lea  r8,  [rip+FIELDS]    ; &[&str].ptr 在 .data.rel.ro
  mov  rdi, <sret>
  mov  ecx, <name_len>      ; &str.len   ← 自校验：必须等于真实字节数
  mov  r9d, <N>             ; &[&str].len ← **N 立即数**
  mov  rsi, <deserializer>
  call <该结构体自己的 Deserialize 辅助函数>

  # 布局 Z —— 空体事件（零字段结构体走寄存器返回 ⇒ 无 sret）
  lea  rsi, [rip+name]
  mov  edx, <name_len>
  mov  ecx, 8               ; 空切片的悬垂指针（align_of::<&str>() == 8）
  mov  rdi, <deserializer>
  xor  r8d, r8d             ; &[&str].len = 0
  call <…>
  ```

  ⚠️ 规格里建议的"从 `Serialize` 侧 `serialize_struct(name_ptr, N)` 取立即数"**在本 `.so` 上不成立**：
  `serde_json::Serializer::serialize_struct` 既不用 `name` 也不用 `len`，内联后 LLVM 把
  这两组立即数**当死代码删掉**，只剩一条 `mov byte ptr […], 0x7b` 直接写 `{`。
  用 `--check-shape` 可以把这条负结论复现出来（命中数 = 0）。

* **方法 B（Serialize 侧，指令级）**：数 `Serialize` impl 里 `serialize_field` 的**调用次数**，
  并顺手得到**线上键序**。判据原样沿用 `tools/wire_fields.py`（上一轮做这件事的工具）：
  写带引号字符串的辅助函数靠多数票自动认；用**花括号深度**区分"事件名"与"字段名"。

* **方法 C（数据侧，只用于解释 N 串，不用于取 N）**：`.rodata` 里
  `struct <Name>Event with N elements` 字面量。⚠️ 这三个字面量**紧邻拼接、没有 NUL**，
  所以扫法不同结论会大幅不同，本工具把三种口径**同时打印**：
  `strings(1)` 行扫描 / 朴素非重叠正则 / **重叠感知扫描**（只有后者是权威读法）。

## 证据档

* 方法 A / B = **逐位**（都是与真二进制指令逐条对拍出来的立即数 / 调用点）；
* 方法 C = **逐位**（`.rodata` 字节 + `.rela.dyn` 重定位）；
* A 与 B 是**两个独立方向**（反序列化侧 vs 序列化侧），它们逐事件相等才有意义。

## 用法

    ../.reference/venv/bin/python tools/protocol_recount.py                  # 人读表 + 统计
    ../.reference/venv/bin/python tools/protocol_recount.py --tsv fixtures/protocol_field_recount.tsv
    ../.reference/venv/bin/python tools/protocol_recount.py --check-shape     # 方法 A 的形状勘察
    ../.reference/venv/bin/python tools/protocol_recount.py --json

需要 `.reference/`（原版 `.so` + capstone venv + `analysis/elf.py` +
`analysis/funcstarts_ghidra_va.pkl`）。**不需要** `analysis/xrefs.pkl` —— 本工具自己扫
`.text` 建 rip 相对引用索引（约 150 万条指令，跑一次 ~3 s）。

地址口径：**全篇 ELF vaddr**。`analysis/funcstarts_ghidra_va.pkl` 里装的就是 vaddr
（最小值 = `.text` 起点、最大值 ≈ `.text` 末尾；减 `0x100000` 反而对不上 `.eh_frame`）。
只有 Ghidra/REA 反编译文本里的 `_DAT_/FUN_` 符号是 `+0x100000` 口径。
"""

import argparse
import bisect
import json
import os
import pickle
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)

IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
CALLD = re.compile(r"^0x([0-9a-f]+)$")
IMM = r"(0x[0-9a-f]+|\d+)"      # 立即数（十进制或 0x 十六进制），嵌进别的正则用
LEN_REG_OF = {"rsi": "edx", "rdi": "esi", "rdx": "ecx", "rcx": "r8d",
              "rax": "edx", "r8": "r9d", "esi": "edx", "edi": "esi"}
NAME_REGS = {"rdx", "edx"}          # 事件名（写带引号字符串）走 rdx/ecx 这一对
FIELD_REGS = {"rsi", "esi"}         # 字段名走 rsi/edx 这一对
EVENT_NAME_PAT = re.compile(r"^[A-Za-z0-9_]+Event$")

# ---------------------------------------------------------------- 环境发现


def _walk_refs():
    """`.reference/` 下的候选根（老布局 `$WS/.reference` 与新布局都在）。"""
    out = []
    env = os.environ.get("GW2_UPSTREAM")
    if env:
        out.append(env)
    for base in (os.path.join(REPO, ".reference"), REPO, os.path.dirname(REPO)):
        out += [os.path.join(base, "upstream"), os.path.join(base, "game"), base,
                os.path.join(base, "analysis-repo")]
    return out


def find_inputs():
    so = pkl = elfpkg = None
    for root in _walk_refs():
        if not root:
            continue
        c = os.path.join(root, "libgraphwarrust.so")
        if so is None and os.path.exists(c):
            so = c
        c = os.path.join(root, "analysis", "funcstarts_ghidra_va.pkl")
        if pkl is None and os.path.exists(c):
            pkl = c
    for root in _walk_refs():
        if not root:
            continue
        c = os.path.join(root, "analysis")
        if os.path.exists(os.path.join(c, "elf.py")):
            elfpkg = c
            break
    if not (so and pkl and elfpkg):
        raise SystemExit("找不到输入：so=%r pkl=%r elf.py:%r（用 GW2_UPSTREAM 指向上游目录）"
                         % (so, pkl, elfpkg))
    return so, pkl, elfpkg


# ---------------------------------------------------------------- 二进制索引


def printable(b):
    return all(32 <= c < 127 or c in (9, 10) for c in b)


class Bin:
    """一次反汇编 + 一个 rip 相对引用倒排索引；全部地址是 ELF vaddr。"""

    def __init__(self):
        so, pkl, elfpkg = find_inputs()
        sys.path.insert(0, elfpkg)
        from elf import Elf
        from capstone import Cs, CS_ARCH_X86, CS_MODE_64, CS_OPT_SYNTAX_INTEL
        self.E = Elf(so)
        self.data = self.E.data
        self.funcstarts = sorted(pickle.load(open(pkl, "rb")))
        t = self.E.sec(".text")
        md = Cs(CS_ARCH_X86, CS_MODE_64)
        md.syntax = CS_OPT_SYNTAX_INTEL
        RIP = re.compile(r"\[rip ([+-]) (0x[0-9a-f]+)\]")
        self.ins = []                       # (addr, size, mnem, op_str, rip_target)
        for ins in md.disasm(self.data[t["offset"]:t["offset"] + t["size"]], t["addr"]):
            tgt = None
            m = RIP.search(ins.op_str)
            if m:
                d = int(m.group(2), 16)
                tgt = ins.address + ins.size + (d if m.group(1) == "+" else -d)
            self.ins.append((ins.address, ins.size, ins.mnemonic, ins.op_str, tgt))
        self.addrs = [r[0] for r in self.ins]
        self.idx = {a: i for i, a in enumerate(self.addrs)}
        self._inv = {}
        for i, r in enumerate(self.ins):
            if r[4] is not None:
                self._inv.setdefault(r[4], []).append(i)
        self.relmap = {}                    # R_X86_64_RELATIVE: 槽位 -> 目标
        rel = self.E.sec(".rela.dyn")
        for i in range(rel["size"] // 24):
            off, info, add = struct.unpack_from("<QQq", self.data, rel["offset"] + i * 24)
            if (info & 0xffffffff) == 8:
                self.relmap[off] = add

    # ---- 段与读
    def sec_span(self, name):
        s = self.E.sec(name)
        return s["addr"], s["addr"] + s["size"]

    def in_sec(self, name, va):
        lo, hi = self.sec_span(name)
        return lo <= va < hi

    def read(self, va, n):
        o = self.E.addr_to_off(va)
        return self.data[o:o + n] if o is not None else None

    # ---- 函数边界
    def func_of(self, va):
        i = bisect.bisect_right(self.funcstarts, va) - 1
        return self.funcstarts[i] if i >= 0 else None

    def func_end(self, f):
        i = bisect.bisect_right(self.funcstarts, f)
        return self.funcstarts[i] if i < len(self.funcstarts) else f + 4096

    def func_range(self, f):
        return (bisect.bisect_left(self.addrs, f),
                bisect.bisect_left(self.addrs, self.func_end(f)))

    # ---- `lea reg,[rip+X]` + 紧随其后的精确长度 ⇒ (reg, text, len, 字面量 vaddr)
    def keyed_lea(self, i):
        a, sz, mn, ops, tgt = self.ins[i]
        if mn != "lea" or tgt is None:
            return None
        reg = ops.split(",")[0].strip()
        want = LEN_REG_OF.get(reg)
        if want is None:
            return None
        for q in self.ins[i + 1:i + 4]:
            m = re.match(r"^%s, %s$" % (re.escape(want), IMM), q[3])
            if q[2] != "mov" or not m:
                continue
            L = int(m.group(1), 0)
            if not (1 <= L <= 4096):
                return None
            raw = self.read(tgt, L)
            if not raw or len(raw) != L or not printable(raw):
                return None
            try:
                return reg, raw.decode(), L, tgt
            except UnicodeDecodeError:
                return None
        return None

    def str_slots(self):
        """`.data.rel.ro` 里的 `(ptr,len)` 静态字符串槽位（重定位后才是真指针）。"""
        out = {}
        for slot in self.relmap:
            w = self.read(slot + 8, 8)
            if not w:
                continue
            ln = struct.unpack("<Q", w)[0]
            if not (1 <= ln <= 4096):
                continue
            raw = self.read(self.relmap[slot], ln)
            if raw and len(raw) == ln and printable(raw):
                out[slot] = (self.relmap[slot], ln, raw)
        return out


# ---------------------------------------------------------------- 方法 A


def method_a(b):
    """Deserialize 侧：`deserialize_struct(name, fields, visitor)` 参数块的立即数 N。

    返回 ({事件名(去 Event 后缀) -> dict}, 重复块列表)。两种寄存器布局都收（判据见模块 docstring）：
      S（有字段事件，Result 大 ⇒ sret 在 rdi）：name=(rdx,ecx) fields=(r8,r9d)
      Z（空体事件，零字段结构体走寄存器返回 ⇒ 无 sret）：name=(rsi,rdx) fields=(rcx=8, r8=0)
    实现是**从 call 往回走**逐条认角色，不假设两者顺序相同（它们确实不同）。
    """
    inro = lambda v: b.in_sec(".rodata", v)
    indrr = lambda v: b.in_sec(".data.rel.ro", v)
    got, dup = {}, []
    for ic, r in enumerate(b.ins):
        if r[2] != "call" or not CALLD.match(r[3]):
            continue
        # ---- 1) N：call 前 3 条内找 `mov r9d,IMM`(S) 或 `xor r8d,r8d`(Z)
        N = layout = None
        for k in range(1, 4):
            j = ic - k
            if j < 0:
                break
            q = b.ins[j]
            m = re.match(r"^r9d, %s$" % IMM, q[3])
            if q[2] == "mov" and m:
                N, layout, jn = int(m.group(1), 0), "S", j
                break
            if q[2] == "xor" and q[3] == "r8d, r8d":
                N, layout, jn = 0, "Z", j
                break
        if layout is None:
            continue
        # ---- 2) 从 N 往回走，按角色收 name/len/fields（允许夹杂 `mov rdi/rsi` 这类搬运）
        name_va = namelen = fptr = None
        j = jn - 1
        for _ in range(6):
            if j < 0 or name_va is not None:
                break
            q = b.ins[j]
            if q[2] == "lea" and q[4] is not None:
                reg = q[3].split(",")[0].strip()
                # S 的 name 在 rdx；Z 的 name 在 rsi
                if layout == "S" and reg == "rdx" and inro(q[4]):
                    name_va = q[4]
                elif layout == "Z" and reg == "rsi" and inro(q[4]):
                    name_va = q[4]
                elif layout == "S" and reg == "r8" and indrr(q[4]):
                    fptr = q[4]
                j -= 1
                continue
            if q[2] == "mov":
                reg = q[3].split(",")[0].strip()
                m = re.match(r"^%s, %s$" % (re.escape(reg), IMM), q[3])
                if m:
                    v = int(m.group(1), 0)
                    if layout == "S" and reg == "ecx" and namelen is None:
                        namelen = v
                    elif layout == "Z" and reg == "edx" and namelen is None:
                        namelen = v
                    elif layout == "Z" and reg == "ecx" and fptr is None:
                        fptr = v          # 空切片的悬垂指针（= 8）
                j -= 1
                continue
            break
        if name_va is None or namelen is None:
            continue
        if layout == "Z" and fptr != 8:   # align_of::<&str>() == 8，这是 `&[]` 的悬垂 ptr
            continue
        raw = b.read(name_va, namelen)
        if not raw or len(raw) != namelen or not printable(raw):
            continue
        text = raw.decode()
        if not EVENT_NAME_PAT.match(text):
            continue
        fields = []
        if N:
            fields = fields_of(b, fptr, N)
            if len(fields) != N:
                continue
        ev = text[:-len("Event")]
        rec = dict(N=N, site=r[0], callee=r[3], name_va=name_va, namelen=namelen,
                   fields_va=fptr, fields=fields, layout=layout, func=b.func_of(r[0]))
        if ev in got:
            dup.append((ev, got[ev]["site"], rec["site"]))
            continue
        got[ev] = rec
    return got, dup


def fields_of(b, va, n):
    """按 `R_X86_64_RELATIVE` 重定位后的指针读 &[&str]（`.data.rel.ro` 里存的是 0 占位）。"""
    out = []
    for k in range(n):
        pva = va + k * 16
        raw = b.read(pva, 16)
        if not raw or len(raw) < 16:
            return out
        ptr, ln = struct.unpack("<QQ", raw)
        ptr = b.relmap.get(pva, ptr)
        if not (1 <= ln <= 64):
            return out
        s = b.read(ptr, ln)
        if not s or len(s) != ln or not printable(s):
            return out
        try:
            out.append(s.decode())
        except UnicodeDecodeError:
            return out
    return out


# ---------------------------------------------------------------- 方法 B


def find_str_writer(b, slots):
    """多数票认「写带引号字符串」的辅助函数（判据同 `tools/wire_fields.py`）。"""
    cands = set(b.relmap.values()) | set(slots)
    votes = {}
    for slot in cands:
        for i in b._inv.get(slot, ()):
            if b.ins[i][2] != "lea":
                continue
            got = b.keyed_lea(i)
            if not got or got[0] not in NAME_REGS:
                continue
            for q in b.ins[i + 1:i + 4]:
                if q[2] == "call":
                    votes[q[3]] = votes.get(q[3], 0) + 1
                    break
    if not votes:
        raise SystemExit("没找到写字符串的辅助函数")
    best = max(votes, key=lambda k: votes[k])
    return int(best, 16), votes[best], votes


def extract(b, f, str_writer):
    """抽一个 serialize 函数的 (事件名, [(字段名, 字面量 vaddr, serialize_field 单态)])。

    判据原样照 `tools/wire_fields.py`：花括号深度 ≥1 的第一个 rdx/edx 串写 = 事件名；
    深度 ≥2 的 rsi/esi 串写 = 字段；字段名必须是 Rust 标识符（挡掉浮点路径多写的 `}`）。
    """
    lo, hi = b.func_range(f)
    name, fields, depth = None, [], 0
    for i in range(lo, hi):
        x = b.ins[i]
        if x[2] == "mov" and "byte ptr" in x[3]:
            if re.search(r", 0x7b$", x[3]):
                depth += 1
                continue
            if re.search(r", 0x7d$", x[3]):
                depth -= 1
                continue
        if x[2] != "lea":
            continue
        got = b.keyed_lea(i)
        if not got:
            continue
        reg, text, L, tva = got
        if reg in NAME_REGS and name is None and depth >= 1:
            for q in b.ins[i + 1:i + 5]:
                if q[2] == "call":
                    if CALLD.match(q[3]) and int(q[3], 16) == str_writer:
                        name = text
                    break
        elif reg in FIELD_REGS and name is not None and depth >= 2:
            if not IDENT.match(text):
                continue
            mono = None
            for q in b.ins[i + 1:i + 7]:
                if q[2] == "call" and CALLD.match(q[3]):
                    mono = int(q[3], 16)
                    break
            fields.append((text, tva, mono, x[0]))
    return (name, fields) if name else None


def method_b(b):
    slots = b.str_slots()
    str_writer, votes, allvotes = find_str_writer(b, slots)
    sites = [r for r in b.ins if r[2] == "call" and r[3] == "0x%x" % str_writer]
    funcs = sorted({b.func_of(s[0]) for s in sites} - {None})
    out = {}
    for f in funcs:
        r = extract(b, f, str_writer)
        if r:
            out[r[0]] = dict(func=f, fields=r[1], count=len(r[1]))
    return dict(str_writer=str_writer, votes=votes, allvotes=allvotes,
                sites=len(sites), funcs=len(funcs), events=out)


def check_shape(b, mb):
    """预注册 §1.1 的机械判据：74 个事件 serializer 里有没有 `serialize_struct(name,N)` 立即数。"""
    hits = []
    for ev, rec in mb["events"].items():
        lo, hi = b.func_range(rec["func"])
        for i in range(lo, hi):
            if b.ins[i][2] != "lea":
                continue
            got = b.keyed_lea(i)
            if not got:
                continue
            reg, text, L, tva = got
            # (name, len) 之后再出现第二个长度立即数 => serialize_struct(name,N) 的形状
            for q in b.ins[i + 1:i + 5]:
                if q[2] == "mov" and LEN_REG_OF.get(reg) and q[3].startswith(LEN_REG_OF[reg]):
                    continue
                m = re.match(r"^(ecx|r8d), %s$" % IMM, q[3])
                if q[2] == "mov" and m:
                    hits.append((ev, q[0], text, int(m.group(1), 0)))
                    break
    return hits


def controls(res):
    """`docs/06` §66.3 的正对照，逐条打印（证明扫描器/判据有效，不是"扫不到"）。"""
    b, mb, mc = res["bin"], res["mb"], res["mc"]
    out = []
    w = out.append
    w("=== 正对照 1：已知锚点（N 串 vs docs/07 §3.4 的字段清单）")
    for name, want in (("GraphwarColor", 3), ("GameInfo", 9), ("EventInfo", 2)):
        got = mc["C3"].get(name)
        w("   %-13s N串 N=%-2s  §3.4 写的 %d 个 ⇒ %s" % (
            name, got["N"] if got else "缺", want,
            "✓" if got and got["N"] == want else "✗"))
    w("")
    w("=== 正对照 2：方法 B 独立抽出的 Serialize 地址应等于 docs/07 §3.4 登记的值")
    for ev, va in (("LockInfo", 0x50ca30), ("LockRequest", 0x50e420)):
        r = mb["events"].get(ev)
        w("   %-12s 方法B=0x%-8x §3.4=0x%-8x ⇒ %s（字段=%s）" % (
            ev, r["func"], va, "✓" if r and r["func"] == va else "✗",
            ",".join(f[0] for f in r["fields"])))
    w("")
    w("=== 正对照 3：'serialize_struct 立即数被 DCE' 不是扫描器瞎了 —— 变体名的 lea 还在")
    got = b.keyed_lea(b.idx[0x50ca64]) if 0x50ca64 in b.idx else None
    w("   0x50ca64 %s（LockInfo 的 serialize_newtype_variant 变体名，真的写进报文）" % (got,))
    w("   同一函数的 serialize_struct(结构体名, N) 形状命中：%d（见 --check-shape）"
      % len(check_shape(b, mb)))
    w("")
    w("=== 正对照 4：两条独立传输（代码段立即数 vs .data.rel.ro 重定位数据）")
    same = [ev for ev, r in res["ma"].items()
            if mb["events"].get(ev) and r["fields"] == [f[0] for f in mb["events"][ev]["fields"]]]
    w("   方法 A 的 FIELDS 数组内容 == 方法 B 的键序：%d / %d 个有字段事件" % (len(same), len(res["ma"])))
    w("")
    w("=== 正对照 5：空体事件真的写 {}（不是 unit variant 的裸字符串）")
    for ev in ("TickRequest", "GameHeartbeat"):
        r = mb["events"][ev]
        lo, hi = b.func_range(r["func"])
        br = [(b.ins[i][0], b.ins[i][3]) for i in range(lo, hi)
              if b.ins[i][2] == "mov" and "byte ptr" in b.ins[i][3]
              and re.search(r", 0x7[bd]$", b.ins[i][3])]
        w("   %-14s Serialize=0x%-8x 花括号写入 %d 条（0x7b='{' / 0x7d='}'），"
          "serialize_field 0 次 ⇒ 线上 '{\"%s\":{}}'" % (ev, r["func"], len(br), ev))
    w("")
    w("=== 正对照 6：'有字段' ⟺ '有 N 串'（在全部 struct 类型上）")
    bare = bare_struct_names(b)
    ev_types = {ev + "Event" for ev in res["ma"]}
    w("   .rodata 里短形式 'struct <T>' 字面量（被代码引用一次）：%d 个" % len(bare))
    w("   其中带 N 串的：%d；不带 N 串的：%d" % (
        len([t for t in bare if t in mc["C3"]]), len([t for t in bare if t not in mc["C3"]])))
    noN = sorted(t for t in bare if t not in mc["C3"])
    w("   不带 N 串的全部是 0 字段事件结构体：%s" % (
        all(res["ma"].get(t[:-len("Event")], {}).get("N") == 0 for t in noN) if noN else "-"))
    w("   明细：%s" % noN)
    return "\n".join(out)


def bare_struct_names(b):
    """`.rodata` 里被代码精确引用的短形式 `struct <T>` 字面量集合（77 个 = 74 事件 + 3 负载结构体）。"""
    out = {}
    for i in range(len(b.ins)):
        kl = b.keyed_lea(i)
        if not kl:
            continue
        m = re.match(r"^struct ([A-Za-z0-9_]+)$", kl[1])
        if m:
            out.setdefault(m.group(1), []).append(b.ins[i][0])
    return out


# ---------------------------------------------------------------- 方法 C


def method_c(b):
    """`.rodata` 里 `struct <T> with N elements` 的三种口径。"""
    ta, tb = b.sec_span(".rodata")
    blob = b.data[b.E.sec(".rodata")["offset"]:b.E.sec(".rodata")["offset"] + (tb - ta)]
    pat = re.compile(rb"struct ([A-Za-z0-9_]+) with (\d+) elements?")
    # C3 重叠感知（权威）
    c3, pos = {}, 0
    while True:
        i = blob.find(b"struct ", pos)
        if i < 0:
            break
        m = pat.match(blob, i)
        if m:
            e = m.group(1).decode()
            c3[e] = dict(N=int(m.group(2)), va=ta + i, text=m.group(0).decode())
        pos = i + 1
    # C2 朴素非重叠
    c2 = {}
    for m in pat.finditer(blob):
        c2[m.group(1).decode()] = int(m.group(2))
    # C1 strings(1) 行扫描（>=4 可打印、整行必须恰好是这一句）
    full = re.compile(r"^struct ([A-Za-z0-9_]+) with (\d+) elements?$")
    c1 = {}
    for line in re.split(rb"[^\x20-\x7e\t]", blob):
        if len(line) >= 4:
            m = full.match(line.decode())
            if m:
                c1[m.group(1)] = int(m.group(2))
    return dict(C3=c3, C2=c2, C1=c1)


# ---------------------------------------------------------------- 规格与文档

WIRE_RE = re.compile(
    r'WireEvent \{ name: "(\w+)", tag: (?:Some\((\d+)\)|None), fields: &\[(.*?)\] \}')


def parse_wire_spec(path):
    src = open(path, encoding="utf-8").read()
    out = {}
    for m in WIRE_RE.finditer(src):
        name, tag, body = m.group(1), m.group(2), m.group(3)
        fields = re.findall(r'\("(\w+)", ValType::(\w+)\)', body)
        out[name] = dict(tag=int(tag) if tag is not None else None,
                         fields=[f for f, _t in fields])
    return out


ROW_RE = re.compile(r"^\| `(\w+)` \| `(\d+)` \| (.+?) \| (.+?) \|$")


def parse_doc33(path):
    out = {}
    for line in open(path, encoding="utf-8"):
        m = ROW_RE.match(line.strip())
        if not m:
            continue
        name, tag, body = m.group(1), int(m.group(2)), m.group(3).strip()
        if "空体" in body:
            fields = []
        else:
            fields = [c for c in body.split(", ") if c.strip()]
        out[name] = dict(tag=tag, body=body, nfields=len(fields))
    return out


# ---------------------------------------------------------------- 组装


def build():
    b = Bin()
    ma, dup_a = method_a(b)
    mb = method_b(b)
    mc = method_c(b)
    wire = parse_wire_spec(os.path.join(REPO, "src", "_scaffold", "protocol.rs"))
    doc = parse_doc33(os.path.join(REPO, "docs", "07-protocol.md"))

    names = sorted(set(wire) | set(doc) | set(ma) | set(mb["events"]))
    rows = []
    for ev in names:
        w = wire.get(ev, {})
        d = doc.get(ev, {})
        a = ma.get(ev)
        bb = mb["events"].get(ev)
        nstr = mc["C3"].get(ev + "Event")
        wf = list(w.get("fields", []))
        bf = [f[0] for f in bb["fields"]] if bb else None
        af = a["fields"] if a else None
        checks = []
        if a and bb:
            checks.append(("A==B", a["N"] == bb["count"]))
            if af is not None and bf is not None:
                checks.append(("A键序==B键序", af == bf))
        if a:
            checks.append(("A==WIRE_SPEC", a["N"] == len(wf)))
            checks.append(("A==§3.3", a["N"] == d.get("nfields")))
        if nstr:
            checks.append(("N串==A", nstr["N"] == a["N"] if a else None))
            checks.append(("N串==WIRE_SPEC", nstr["N"] == len(wf)))
        bad = [k for k, ok in checks if ok is False]
        rows.append(dict(
            event=ev, tag=w.get("tag", d.get("tag")),
            wfields=len(wf), wf=wf,
            a_N=(a["N"] if a else None), a_site=(a["site"] if a else None),
            a_layout=(a["layout"] if a else None), a_callee=(a["callee"] if a else None),
            a_fields=af,
            b_count=(bb["count"] if bb else None), b_func=(bb["func"] if bb else None),
            b_fields=bf,
            has_nstr=bool(nstr), nstr_N=(nstr["N"] if nstr else None),
            nstr_va=(nstr["va"] if nstr else None),
            doc_n=d.get("nfields"), doc_body=d.get("body"),
            checks=checks, bad=bad,
            ok=(not bad) and a is not None and bb is not None and bool(w) and bool(d),
        ))
    return dict(bin=b, ma=ma, dup_a=dup_a, mb=mb, mc=mc, wire=wire, doc=doc, rows=rows)


def human(res):
    b, mb, mc, rows = res["bin"], res["mb"], res["mc"], res["rows"]
    out = []
    w = out.append
    w("# 事件 serializer（方法 B 入口）：call 0x%x 的站点 %d 个，落在 %d 个函数里 ⇒ 抽出 %d 个事件"
      % (mb["str_writer"], mb["sites"], mb["funcs"], len(mb["events"])))
    w("# 方法 A（Deserialize `fields.len` 立即数）：%d 个事件" % len(res["ma"]))
    w("# 方法 C 的三种口径：strings(1) 行扫描 %d 条 / 朴素非重叠正则 %d 条 / **重叠感知 %d 条**"
      % (len(mc["C1"]), len(mc["C2"]), len(mc["C3"])))
    c3ev = {k: v for k, v in mc["C3"].items() if k.endswith("Event")}
    w("#   N 串里以 Event 结尾（= 事件结构体）：%d；其余：%s"
      % (len(c3ev), sorted(set(mc["C3"]) - set(c3ev))))
    w("")
    w("%-30s %-4s %-6s %-6s %-6s %-5s %-6s %-6s %s" % (
        "事件", "tag", "A(N)", "B(#)", "WIRE", "N串", "N串N", "§3.3", "结果"))
    for r in rows:
        w("%-30s %-4s %-6s %-6s %-6s %-5s %-6s %-6s %s" % (
            r["event"], r["tag"] if r["tag"] is not None else "-",
            r["a_N"] if r["a_N"] is not None else "缺",
            r["b_count"] if r["b_count"] is not None else "缺",
            r["wfields"], "有" if r["has_nstr"] else "无",
            r["nstr_N"] if r["nstr_N"] is not None else "-",
            r["doc_n"] if r["doc_n"] is not None else "缺",
            "✓" if r["ok"] else ("✗ " + ",".join(r["bad"]))))
    n = len(rows)
    ok = [r for r in rows if r["ok"]]
    w("")
    w("# 行数 %d；全四列一致 %d 行；不一致 %d 行" % (n, len(ok), n - len(ok)))
    for k in ("A==B", "A==WIRE_SPEC", "A==§3.3", "N串==A", "A键序==B键序"):
        tot = [r for r in rows if any(c[0] == k for c in r["checks"])]
        good = [r for r in tot if all(ok2 for kk, ok2 in r["checks"] if kk == k)]
        w("#   %-12s 适用 %d 行，相等 %d 行" % (k, len(tot), len(good)))
    empty = [r["event"] for r in rows if r["a_N"] == 0]
    w("# 空体事件（A 的 N==0）：%d 个 %s" % (len(empty), empty))
    wn = [r["event"] for r in rows if r["has_nstr"]]
    w("# 有 N 串的事件：%d 个；无 N 串的：%d 个 %s"
      % (len(wn), n - len(wn), [r["event"] for r in rows if not r["has_nstr"]]))
    return "\n".join(out)


TSV_HEAD = ("事件名\ttag\tWIRE_SPEC 字段数\t实测字段数(方法A)\tserialize_field 次数(方法B)\t"
            "是否有 N 串\tN 串里的 N\t§3.3 表的值\t一致?\t方法A_证据\t方法B_Serialize")


def tsv(res):
    lines = [TSV_HEAD]
    for r in res["rows"]:
        lines.append("\t".join(str(x) for x in (
            r["event"], r["tag"] if r["tag"] is not None else "-",
            r["wfields"],
            r["a_N"] if r["a_N"] is not None else "缺",
            r["b_count"] if r["b_count"] is not None else "缺",
            "有" if r["has_nstr"] else "无",
            r["nstr_N"] if r["nstr_N"] is not None else "-",
            r["doc_n"] if r["doc_n"] is not None else "缺",
            "✓" if r["ok"] else "✗:" + "+".join(r["bad"]),
            ("0x%x %s N=%d" % (r["a_site"], r["a_layout"], r["a_N"])) if r["a_site"] else "-",
            ("0x%x" % r["b_func"]) if r["b_func"] else "-")))
    return "\n".join(lines) + "\n"


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--tsv", metavar="PATH", help="写成 TSV")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--controls", action="store_true",
                    help="把 docs/06 §66.3 的正对照逐条打印（证明扫描器有效）")
    ap.add_argument("--check-shape", action="store_true",
                    help="预注册 §1.1：Serialize 侧 serialize_struct 立即数是否存在（期望 0 命中）")
    a = ap.parse_args(argv[1:])

    res = build()
    if a.check_shape:
        hits = check_shape(res["bin"], res["mb"])
        print("# 方法 A 形状勘察（预注册 §1.1）")
        print("#   在 %d 个事件 serializer 里搜 `serialize_struct(name_ptr, N)` 的 (lea name, mov len, mov N) 形状"
              % len(res["mb"]["events"]))
        print("#   命中：%d 条 ⇒ 结论：Serialize 侧的 N 立即数在本 .so 上**不存在**（被内联后的 DCE 删掉）" % len(hits))
        for h in hits[:10]:
            print("#     %s 0x%x %r N=%d" % h)
        print("#   注意这不是\"扫描器瞎了\"：同一个函数 0x50ca30（LockInfo 的 Serialize）里")
        print("#   `lea rdx,[rip+0x19f928 \"LockInfo\"]` + `mov ecx,8` + `call 0x3bdf00` 是**在的** ——")
        print("#   那是 serialize_newtype_variant 的**变体名**（真的写进报文）；被 DCE 的是")
        print("#   serialize_struct 的 (结构体名, N)，因为 serde_json 不用它们。")
        return 0
    if a.controls:
        print(controls(res))
        return 0
    if a.tsv:
        open(a.tsv, "w", encoding="utf-8").write(tsv(res))
        print("# 已写 %s（%d 行 + 1 行表头）" % (a.tsv, len(res["rows"])))
    if a.json:
        print(json.dumps(dict(
            str_writer=res["mb"]["str_writer"], votes=res["mb"]["allvotes"],
            nstr_counts={k: len(v) for k, v in res["mc"].items()},
            rows=[{k: v for k, v in r.items() if k not in ("checks", "wf", "doc_body")}
                  for r in res["rows"]]), indent=1, ensure_ascii=False))
        return 0
    print(human(res))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
