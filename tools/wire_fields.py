#!/usr/bin/env python3
"""从 `libgraphwarrust.so` 里读出每个网络事件的**字段名与字段顺序**（线上键序）。

## 为什么需要它

协议是唯一对外的接口，而 `#[derive(Serialize)]` 生成的代码**就是**线格式的定义：

```
serialize_struct("XxxEvent", N)      -> 名字只进错误串，JSON 里不出现
serialize_field("字段名", 值)         -> 字段名 = .rodata 里的 &'static str
```

`serde_json` 把 `serialize_struct` 的名字丢掉，但 `serialize_field` 的键**逐个写进报文**，
而且**调用顺序就是线上顺序**。所以「事件名 + 字段名序列」不是猜的，是从代码里读的。

在这份二进制的代码生成形状里（LLVM 把 serde 的 `Compound` 全部内联了），一次
`serialize_field` 编译成三条连续指令：

```
lea  rsi, [rip + 字段名]      ; &'static str.ptr
mov  edx, LEN                 ; &'static str.len   （必须 == 从文件里读到的字节数，这就是自校验）
call <serialize_field 单态>    ; 值在 rcx
```

事件的**名字**走同一个"写带引号字符串"的辅助函数，但用 rdx/ecx 这对寄存器：

```
lea  rdx, [rip + "LockInfo"]
mov  ecx, 8
call <写键名>
```

⇒ 反汇编一个事件的 `Serialize` 实现，按地址顺序收 `lea rsi` 那一串，就是线上键序。

⚠️ 判据要点：**枚举值**（如 `GameState::EndingGame`、"ExplosionEffect"）也走同一个
辅助函数，但它只有一个裸字符串、没有 `{…}`。所以本脚本用**花括号深度**区分：
只有深度 ≥ 2（外层 `{"事件名":` 之后的那个内层 `{`）里出现的 `lea rsi` 才算字段。
这一条同时把 `{"X":标量}` 形状排除在外。

## 为什么不能直接用 `strings`

Rust 字面量**不以 NUL 结尾**，`strings` 会把相邻两个字面量粘成一条（实测：单独的
`locked` / `game_address` / `message` 在 `strings` 里搜不到，不是不存在）。
本脚本直接从 `lea` 的 RIP 相对目标取地址、从随后那条 `mov <lenreg>, IMM` 取**精确长度**，
再按这个长度从文件里读字节 —— 边界由代码自己给出，不靠猜。
（`.rela.dyn` 的 `R_X86_64_RELATIVE` 只用来读 `struct <Name> with N elements` 这类**数据**字面量。）

## 地址口径

内部全用 **ELF 虚地址**。`analysis/xrefs.pkl` 与 `analysis/funcstarts_ghidra_va.pkl`
都是虚地址（已用 `objdump` 对拍：`end_of_turn_tick` 的 `lea` 在 `0x512b1f`）。
输出同时给 Ghidra/REA 地址（= 虚地址 + `0x100000`，**只加一次**）。

## 自校验

1. **长度自校验**：字段名后面必须紧跟 `mov <lenreg>, len`，且 `len` 个字节必须全可打印；
2. **N 自校验**：字段个数必须等于 `.rodata` 里 `struct <Name>Event with N elements` 的 N；
3. **反序列化交叉**：`Deserialize` 侧引用的字段名集合与 `Serialize` 侧相同（SIMD 合并
   常量会让个别长键比不出来，所以这一项按"子集"判，缺口单独报，不算失败）；
4. **抓包交叉**（离线，见 `tests/wire_format.rs`）：真实报文里的键序必须相同。

## 用法

    ../.venv/bin/python tools/wire_fields.py             # 人读的表
    ../.venv/bin/python tools/wire_fields.py --json      # 机器读
    ../.venv/bin/python tools/wire_fields.py --verify    # 只跑自校验，不自洽则退出码非 0
    ../.venv/bin/python tools/wire_fields.py --struct 0x5060d0   # 任意 struct 的键序（GameInfo 示例）

需要上游目录里的 `libgraphwarrust.so` + `analysis/funcstarts_ghidra_va.pkl`（都不入库）。
用 `GW2_UPSTREAM=/path/to/gw2` 硬指，或从本脚本位置逐级上溯。

⚠️ **2026-11-26（T16）修的两处**（此前 `--verify` 在本机直接跑不起来）：
1. `find_upstream()` 原来要求 `xrefs.pkl` 与 `.so` 同处 `analysis/` 下 —— 本机布局里
   `.so` 在 `.reference/upstream/`、`analysis/elf.py` 在 `.reference/analysis-repo/analysis/`、
   而 `xrefs.pkl` **根本没有**（它是上游 `analysis/build_index.py` 的产物，不入库）。
   现在：`.so` 与 `funcstarts_ghidra_va.pkl` 分别找；`xrefs.pkl` 缺失时**现场反汇编 `.text`
   重建同样的倒排索引**（约 150 万条指令 / 3 s），有 pkl 时优先用 pkl（行为完全不变）。
2. 补一条**空体事件**的说明：`.rodata` 里 `struct <Name>Event with N elements` 共 **67** 条
   = **64 个有字段事件** + **3 个负载结构体**（`GameInfo` 9 / `GraphwarColor` 3 / `EventInfo` 2）；
   **10 个空体事件没有这条串**（0 字段结构体走 `expecting` 短形式 `struct <Name>Event`）。
   所以"事件字段数"与"N 串条数"本来就不是一一对应，`N 自校验` 的**分母是有字段事件 64**。
   逐事件复核见 `tools/protocol_recount.py` + `fixtures/protocol_field_recount.tsv`（`docs/06` §66）。
"""

import argparse
import bisect
import json
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
GHIDRA_DELTA = 0x100000

# SysV 里 (ptr, len) 的合法寄存器配对：lea 用的寄存器 -> 长度用的立即数寄存器
LEN_REG_OF = {"rsi": "edx", "rdi": "esi", "rdx": "ecx", "rcx": "r8d",
              "rax": "edx", "r8": "r9d", "esi": "edx", "edi": "esi"}
NAME_REGS = {"rdx", "edx"}          # 事件名：rdx / ecx
FIELD_REGS = {"rsi", "esi"}         # 字段名：rsi / edx

C_OBC, C_OBC_CLOSE, C_COLON = 0x7B, 0x7D, 0x3A
IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def find_upstream():
    """返回 (含 libgraphwarrust.so 的目录, 含 funcstarts_ghidra_va.pkl 的 analysis 目录)。

    2026-11-26（T16）：原来要求 `xrefs.pkl` 与 `.so` 同处一个 `analysis/`，
    本机布局里那条根本不成立（so 在 `.reference/upstream/`，elf.py 在
    `.reference/analysis-repo/analysis/`，`xrefs.pkl` 不存在）。现在分开找。
    """
    cands = [os.environ.get("GW2_UPSTREAM")]
    d = REPO
    for _ in range(4):
        cands.append(d)
        d = os.path.dirname(d)
    for base in (os.path.join(REPO, ".reference"), REPO, os.path.dirname(REPO)):
        cands += [os.path.join(base, "upstream"), os.path.join(base, "game"),
                  os.path.join(base, "analysis-repo"), base]
    sodir = next((c for c in cands if c and os.path.exists(os.path.join(c, "libgraphwarrust.so"))), None)
    ana = next((os.path.join(c, "analysis") for c in cands
                if c and os.path.exists(os.path.join(c, "analysis", "funcstarts_ghidra_va.pkl"))), None)
    # elf.py 是上游 analysis 仓库里的模块，不一定与 .so / pkl 同处一个目录
    elfdir = next((os.path.join(c, "analysis") for c in cands
                   if c and os.path.exists(os.path.join(c, "analysis", "elf.py"))), ana)
    if sodir is None or ana is None:
        raise SystemExit("找不到上游目录（libgraphwarrust.so + analysis/funcstarts_ghidra_va.pkl）；"
                         "用 GW2_UPSTREAM 指定")
    return sodir, ana, elfdir


def build_xrefs(E):
    """`analysis/xrefs.pkl` 缺失时现场重建同一个倒排索引（`analysis/build_index.py` 的同款逻辑）。

    pkl 不入库（上游 `.gitignore` 挡 `*.pkl`），所以"复跑命令"必须能在没有 pkl 的机器上跑。
    """
    import pickle
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64, CS_OPT_SYNTAX_INTEL
    from capstone.x86 import X86_OP_MEM, X86_OP_IMM, X86_REG_RIP
    t = E.sec('.text')
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    md.syntax = CS_OPT_SYNTAX_INTEL
    x = {}
    for ins in md.disasm(E.data[t['offset']:t['offset'] + t['size']], t['addr']):
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                x.setdefault(ins.address + ins.size + op.mem.disp, []).append(
                    (ins.address, 'rip', ins.mnemonic, ins.op_str))
            elif op.type == X86_OP_IMM:
                if op.imm > 0x1000:
                    x.setdefault(op.imm & 0xffffffffffffffff, []).append(
                        (ins.address, 'imm', ins.mnemonic, ins.op_str))
    sys.stderr.write("# xrefs.pkl 缺失 ⇒ 现场重建：%d 个引用目标\n" % len(x))
    return x



def printable(b):
    return all(32 <= c < 127 or c in (9, 10) for c in b)


class Wire:
    def __init__(self):
        import pickle
        sodir, ana, elfdir = find_upstream()
        sys.path.insert(0, elfdir)                               # elf.py 可能不在 so 旁边
        from elf import Elf                                     # noqa: E402
        from capstone import Cs, CS_ARCH_X86, CS_MODE_64, CS_OPT_SYNTAX_INTEL  # noqa: E402
        self.E = Elf(os.path.join(sodir, "libgraphwarrust.so"))
        self.data = self.E.data
        xrefs_pkl = os.path.join(ana, "xrefs.pkl")
        self.xrefs = (pickle.load(open(xrefs_pkl, "rb")) if os.path.exists(xrefs_pkl)
                      else build_xrefs(self.E))
        self.funcstarts = sorted(pickle.load(
            open(os.path.join(ana, "funcstarts_ghidra_va.pkl"), "rb")))
        self.md = Cs(CS_ARCH_X86, CS_MODE_64)
        self.md.detail = True
        self.md.syntax = CS_OPT_SYNTAX_INTEL
        self._reloc_data_strings()
        self._struct_counts()

    # ---------- 只用于读「数据」字面量（`struct X with N elements`）
    def _reloc_data_strings(self):
        rel = self.E.sec(".rela.dyn")
        self.relmap = {}
        for i in range(rel["size"] // 24):
            off, info, add = struct.unpack_from("<QQq", self.data, rel["offset"] + i * 24)
            if (info & 0xffffffff) == 8:                        # R_X86_64_RELATIVE
                self.relmap[off] = add
        self.txt_at = {}                                        # slot -> text
        for slot in self.relmap:
            t = self._str_slot(slot)
            if t is not None:
                self.txt_at[slot] = t

    def _str_slot(self, slot):
        """把一个 (ptr, len) 数据槽位解成精确字面量。"""
        if slot not in self.relmap:
            return None
        w = self.E.read(slot + 8, 8)
        if not w:
            return None
        ln = struct.unpack("<Q", w)[0]
        if not (1 <= ln <= 4096):
            return None
        raw = self.E.read(self.relmap[slot], ln)
        if not raw or len(raw) != ln or not printable(raw):
            return None
        try:
            return raw.decode()
        except UnicodeDecodeError:
            return None

    def _struct_counts(self):
        pat = re.compile(r"^struct (\w+) with (\d+) elements?$")
        self.counts, self.count_slot = {}, {}
        for slot, t in self.txt_at.items():
            m = pat.match(t)
            if m:
                self.counts[m.group(1)] = int(m.group(2))
                self.count_slot[m.group(1)] = slot

    # ---------- 函数边界
    def func_of(self, va):
        i = bisect.bisect_right(self.funcstarts, va) - 1
        return self.funcstarts[i] if i >= 0 else None

    def func_end(self, f):
        i = bisect.bisect_right(self.funcstarts, f)
        return self.funcstarts[i] if i < len(self.funcstarts) else None

    def dis(self, va, n):
        code = self.E.read(va, n)
        return list(self.md.disasm(code, va)) if code else []

    def dis_func(self, f, cap=40000):
        end = self.func_end(f) or (f + 4096)
        return self.dis(f, min(end - f, cap))

    # ---------- 核心：`lea reg,[rip+字面量]` + 紧随其后的精确长度
    def keyed_lea(self, site):
        """site 必须是 `lea reg,[rip+X]`；后面 3 条内要有 `mov <len_reg>, IMM`，
        IMM 必须是**该处真实的 &str 长度**。返回 (reg, text, IMM) 或 None。

        自校验：IMM 个字节必须全可打印。`lea` 的地址由 xrefs 给出，是真实指令边界。
        """
        from capstone.x86 import X86_OP_MEM, X86_REG_RIP
        ins = self.dis(site, 64)
        if not ins or ins[0].address != site or ins[0].mnemonic != "lea":
            return None
        tva = None
        for op in ins[0].operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                tva = ins[0].address + ins[0].size + op.mem.disp
        if tva is None:
            return None
        reg = ins[0].op_str.split(",")[0].strip()
        want = LEN_REG_OF.get(reg)
        if want is None:
            return None
        for nxt in ins[1:4]:
            m = re.match(r"^%s, (0x[0-9a-f]+|\d+)$" % re.escape(want), nxt.op_str)
            if nxt.mnemonic != "mov" or not m:
                continue
            ln = int(m.group(1), 0)
            if not (1 <= ln <= 4096):
                return None
            raw = self.E.read(tva, ln)
            if not raw or len(raw) != ln or not printable(raw):
                return None
            try:
                return reg, raw.decode(), ln, tva
            except UnicodeDecodeError:
                return None
        return None

    # ---------- 「写带引号字符串」的辅助函数：靠多数票自动认，不硬编码
    def find_str_writer(self):
        # 代码里的 `lea rdx,[rip+X]` 有两种指向：直接指字符串字节，或指它的 (ptr,len) 静态槽。
        # 两者都要试，否则事件名（多为前者）会被漏掉。
        cands = set(self.relmap.values()) | set(self.txt_at)
        votes = {}
        for slot in cands:
            for row in self.xrefs.get(slot, ()):
                site, kind, mn, _ = row
                if kind != "rip" or mn != "lea":
                    continue
                got = self.keyed_lea(site)
                if not got or got[0] not in NAME_REGS:
                    continue
                ins = self.dis(site, 32)
                for i in range(1, min(4, len(ins))):
                    if ins[i].mnemonic == "call":
                        votes[ins[i].op_str] = votes.get(ins[i].op_str, 0) + 1
                        break
        if not votes:
            raise SystemExit("没找到写字符串的辅助函数")
        best = max(votes, key=lambda k: votes[k])
        return int(best, 16), votes[best]

    # ---------- 抽一个函数的 (名字, [字段…])；花括号深度决定哪些 lea 算字段
    def extract(self, f, str_writer):
        ins = self.dis_func(f)
        if not ins:
            return None
        name, fields, depth = None, [], 0
        for x in ins:
            if x.mnemonic == "mov" and "byte ptr" in x.op_str:
                if re.search(r", 0x%x$" % C_OBC, x.op_str):
                    depth += 1
                    continue
                if re.search(r", 0x%x$" % C_OBC_CLOSE, x.op_str):
                    depth -= 1
                    continue
            if x.mnemonic != "lea":
                continue
            got = self.keyed_lea(x.address)
            if not got:
                continue
            reg, text, _, tva = got
            if reg in NAME_REGS and name is None and depth >= 1:
                nxt = self.dis(x.address, 40)
                for i in range(1, min(5, len(nxt))):
                    if nxt[i].mnemonic == "call":
                        if int(nxt[i].op_str, 16) == str_writer:
                            name = text
                        break
            elif reg in FIELD_REGS and name is not None and depth >= 2:
                # serde 字段名一定是 Rust 标识符。浮点字段的序列化路径会额外
                # `write_str("}")`（1 字节字面量），它会伪装成第 N+1 个"字段"——
                # 标识符过滤把它挡掉，这是判据，不是凑数。
                if not IDENT.match(text):
                    continue
                # 紧随其后的 `call` 是这一字段 `serialize_field` 的单态化版本；
                # 同一地址 ⇒ 同一个值类型，可用来给没抓包过的字段定类型。
                mono = None
                for y in self.dis(x.address, 64)[1:6]:
                    if y.mnemonic == "call" and y.op_str.startswith("0x"):
                        mono = int(y.op_str, 16)
                        break
                fields.append((text, tva, mono))
        return (name, fields) if name else None

    def event_serializers(self):
        self.str_writer, self.str_writer_votes = self.find_str_writer()
        sites = sorted({row[0] for row in self.xrefs.get(self.str_writer, ())})
        funcs = sorted({self.func_of(s) for s in sites} - {None})
        out = []
        for f in funcs:
            r = self.extract(f, self.str_writer)
            if r:
                out.append((f, r[0], r[1]))
        return out

    def fields_at(self, f):
        """不要求有事件名：直接抽一个 struct 的键序（用于 GameInfo / GraphwarColor）。"""
        if not hasattr(self, "str_writer"):
            self.str_writer, self.str_writer_votes = self.find_str_writer()
        ins = self.dis_func(f)
        fields, depth = [], 0
        for x in ins:
            if x.mnemonic == "mov" and "byte ptr" in x.op_str:
                if re.search(r", 0x%x$" % C_OBC, x.op_str):
                    depth += 1
                    continue
                if re.search(r", 0x%x$" % C_OBC_CLOSE, x.op_str):
                    depth -= 1
                    continue
            if x.mnemonic != "lea" or depth < 1:
                continue
            got = self.keyed_lea(x.address)
            if got and got[0] in FIELD_REGS and IDENT.match(got[1]):
                mono = None
                for y in self.dis(x.address, 64)[1:6]:
                    if y.mnemonic == "call" and y.op_str.startswith("0x"):
                        mono = int(y.op_str, 16)
                        break
                fields.append((got[1], got[3], mono))
        return fields

    # ---------- 反序列化侧交叉核对
    #
    # `Deserialize` 的 `visit_map` 这份二进制里被 LLVM 内联成了一串 **SIMD 字节比较**，
    # 比较常量还常常是**相邻字面量的合并窗口**，所以「从 lea 读 (ptr,len)」这条办法在这里
    # 不成立。改用**包含测试**：某字段名 `x` 的字面量在 `X` 起、长 `len(x)`，
    # 只要反序列化函数体内有**任何一个** RIP 相对引用落在 `[X, X+len(x))`，就算它比了 `x`。
    # 这既覆盖整串比较，也覆盖「从中间起 16 字节」的合并窗口。
    def invert_xrefs(self):
        if hasattr(self, "_inv"):
            return self._inv
        inv = {}
        for tgt, rows in self.xrefs.items():
            for row in rows:
                inv.setdefault(row[0], []).append(tgt)
        self._inv = inv
        return inv

    def deser_refs(self, f):
        inv = self.invert_xrefs()
        end = self.func_end(f) or (f + 4096)
        out = set()
        for site, tgts in inv.items():
            if f <= site < end:
                out.update(tgts)
        return out

    def deser_check(self, struct_name, fields):
        """返回 (deser 函数, 被比出的字段名列表)。"""
        slot = self.count_slot.get(struct_name)
        if slot is None:
            return None, None
        funcs = sorted({self.func_of(r[0]) for r in self.xrefs.get(slot, ())} - {None})
        if not funcs:
            return None, None
        best = (None, [])
        for f in funcs:
            runs = []
            for t in self.deser_refs(f):
                raw = self.E.read(t, 32)
                if not raw:
                    continue
                n = 0
                for c in raw:
                    if not (32 <= c < 127):
                        break
                    n += 1
                if n:
                    runs.append(raw[:n].decode())
            hit = []
            for text, _tva, _mono in fields:
                # 判据：这段比较要覆盖 `text` 的全部字节。要么比较窗口就从这个字面量起点开始，
                # 要么它从 `text` 内部某个偏移开始（LLVM 合并相邻字面量后的 16 字节窗口）。
                for r in runs:
                    o = r.find(text)
                    if o == 0 or (0 <= o < 16 and len(r) - o >= len(text)):
                        hit.append(text)
                        break
            if len(hit) > len(best[1]):
                best = (f, hit)
        return best


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--verify", action="store_true")
    ap.add_argument("--struct", help="抽任意函数地址（虚地址 0x…）的键序，用于 payload 结构体")
    a = ap.parse_args(argv[1:])

    w = Wire()

    if a.struct:
        f = int(a.struct, 16)
        fs_ = w.fields_at(f)
        print("# 0x%x (Ghidra 0x%x) -> %d 个键：%s"
              % (f, f + GHIDRA_DELTA, len(fs_), ", ".join(t for t, _v, _m in fs_)))
        return 0

    rows = []
    for f, name, fields in w.event_serializers():
        struct = name + "Event"
        n = w.counts.get(struct)
        d_f, d_names = w.deser_check(struct, fields)
        flags = []
        if n is not None and n != len(fields):
            flags.append("N=%d≠%d" % (n, len(fields)))
        if n is None and fields:
            flags.append("N串缺失但有字段")
        names = [t for t, _v, _m in fields]
        missing = [x for x in names if d_names is not None and x not in d_names]
        rows.append(dict(func=f, ghidra=f + GHIDRA_DELTA, event=name, struct=struct,
                         n=len(fields), n_string=n, fields=names,
                         field_vas=[v for _, v, _m in fields],
                         field_monos=[("0x%x" % m) if m else None for _, _, m in fields],
                         deser_func=d_f, deser_names=d_names,
                         deser_missing=missing, flags=flags))
    rows.sort(key=lambda r: r["event"])

    if a.json:
        print(json.dumps(dict(str_writer=w.str_writer,
                              str_writer_votes=w.str_writer_votes,
                              struct_counts=w.counts, events=rows),
                         indent=1, ensure_ascii=False))
        return 0

    print("# 写字符串的辅助函数 = 0x%x（多数票 %d）  Ghidra 0x%x"
          % (w.str_writer, w.str_writer_votes, w.str_writer + GHIDRA_DELTA))
    print("# 事件 serializer = %d 个；`.rodata` 里 `struct <Name>Event with N elements` = %d 条"
          % (len(rows), len(w.counts)))
    print()
    print("%-28s %-10s %-10s %-4s %s" % ("事件", "Serialize", "Ghidra", "N", "字段（按线序）"))
    bad = []
    for r in rows:
        print("%-28s 0x%-8x 0x%-8x %-4s %s%s" % (
            r["event"], r["func"], r["ghidra"], r["n"],
            ", ".join("%s%s" % (f, "" if not m else "«%s»" % m)
                      for f, m in zip(r["fields"], r["field_monos"])) or "(空体 / 无字段)",
            ("   <<< " + "; ".join(r["flags"])) if r["flags"] else ""))
        if r["flags"]:
            bad.append(r)
    print()
    print("# N 自校验失败：%d 条" % len(bad))
    for r in bad:
        print("#   0x%x %s -> %s" % (r["func"], r["event"], r["flags"]))
    miss = [(r["event"], r["deser_missing"]) for r in rows if r["deser_missing"]]
    print("# 反序列化侧未比出的键（SIMD 合并常量所致，非矛盾）：%d 条" % len(miss))
    for e, m in miss:
        print("#   %-26s %s" % (e, m))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
