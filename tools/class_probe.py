#!/usr/bin/env python3
"""按 `panic::Location` 把 `.rodata` 字面量 / 类型名 / 模块名归属到**函数**。

    ../.venv/bin/python tools/class_probe.py classes                          # 类型名 → 引用它的函数
    ../.venv/bin/python tools/class_probe.py module graphwar_plane            # 模块名 → Location → 函数
    ../.venv/bin/python tools/class_probe.py func 0x4b5df0                   # 单个函数的字面量 + 调用表
    ../.venv/bin/python tools/class_probe.py literals 'subscribe_to_event'   # 反查字面量

## ⚠️ 2026-11-07：这里的归属列**曾经**整体偏 `+0x1000`，现在已修复（v2）

**旧版本（v1）错在哪**：`Index.funcs_ref_slot()` 与 `classes` 子命令把上游
`analysis/xrefs.pkl` 里的引用点当成**文件偏移**又过了一遍 `off2va()`，而那份 pkl 里存的
**已经是虚地址**（`analysis/build_index.py` 用 `md.disasm(blob, t['addr'])` 生成）。
`.text` 的「虚地址 − 文件偏移」= `0x1000`，于是每条引用被平移 `+0x1000`，
归属**整体向右漂一个函数**：

    :274:129  本工具 v1 给 0x4b7a70   正确 0x4b5df0   （引用指令 0x4b6dc5，v1 当成 0x4b7dc5）
    :218:129  本工具 v1 给 0x4b8bd0   正确 0x4b7a70
    :68:1     本工具 v1 给 0x4b2890   正确 四个 gdext 入口（4 个塌成 3 个）

**上一版 docstring 把这个错误归因成「pkl 与目标 `.so` 不是同一次扫描」——那是错的。**
2026-11-07 逐条复核：`analysis/xrefs.pkl` 与
`.reference/steam/2026-08-28/libgraphwarrust.so`（sha256 `9a538115…`）**完全一致** ——
5301 个 Location 的「引用指令集合」与本工具 v2 独立扫出来的**逐条相同**。
（同一个错误结论也曾写进 `docs/16-client.md` §26.1；同文件 §16.1.1 当时的判断才是对的。）

**v2 怎么改的**：归属不再读任何 `analysis/*.pkl`，改由同仓库的
`tools/loc_xrefs.py` 从**目标 `.so` 自己**算（`.rela.dyn` + `.eh_frame` + capstone）；
本文件只剩“把 Location 的 `(file,line)` 拼成可读输出”这一层。

**新判据**（`tools/loc_xrefs.py` 的规格，`loc_xrefs.py stats` 逐形态报数）：

* Location = `.data.rel.ro` 里 `{ptr(R_X86_64_RELATIVE), len, line, col}`，且
  ① addend 落在 `.rodata`、② `+8` 的 len 与字面量字节数一致、③ 全可打印且以 `.rs` 结尾、
  ④ `line/col` 合理；**引用判定把首址 / `+8` / `+16` 三阶段都算**；
* 引用形态四类：**R** `[rip+disp]` 直引、**I** 立即数直引、**G** 寄存器携带
  （`lea/mov` 先装地址、再 `call *%reg` 或作内存基址 —— 只匹配 `call *(slot)` 会漏的那一类）、
  **D** 被重定位数据槽的值指向该 Location。本版实测 R=9739、G/I/D=0
  （**实测 0，不是没实现**：G 走寄存器污点传播、D 走全表 addend 反查）；
* 函数边界 = `.eh_frame` FDE 的 `start+range`（覆盖 `.text` 的 98.35%，起点全部通过
  指令边界校验）＋ FDE 覆盖不到的 `call/jmp` 段内目标（63 个）＋ 段首。**不需要 Ghidra。**

**一条命令复算 + 交叉验证**：

    ../.venv/bin/python tools/loc_xrefs.py stats        # 判据逐形态计数 + 目标 sha256
    ../.venv/bin/python tools/loc_xrefs.py check-tsv    # 拿 fixtures/name_to_addr.tsv 的 327 个唯一锚对打

`check-tsv` 现状：**324/327 一致**；2 条是函数边界口径问题（一条地址根本不在指令边界上，
一条是 Ghidra 把 `to_string_recursive` 拆出独立函数而 x86 实际合并在调用者体内），
1 条是 TSV 自己标 `唯一锚` 但证据栏写着“消去法”。详见 `docs/16-client.md` §27。

## 运行前提

需要 **capstone**；仓库约定用 venv：

    ../.venv/bin/python tools/class_probe.py module <模块>

目标 `.so` 用 `GW2_TARGET_SO` 或 `--so` 指定，缺省
`/home/user/MyProject/gw2/.reference/steam/2026-08-28/libgraphwarrust.so`。
**不再需要上游 `analysis/*.pkl`**（那些现在只能当对照，见 `loc_xrefs.py` 顶部说明）。
"""

import bisect
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import loc_xrefs                                            # noqa: E402

GHIDRA_DELTA = 0x100000

# SysV 传参顺序下 (ptr, len) 的寄存器配对：只有配对的 mov 才当成长度
LEN_PAIR = {"rsi": "edx", "rdi": "esi", "rdx": "ecx", "rcx": "r8d",
            "rax": "edx", "r8": "r9d", "esi": "edx", "edi": "esi"}
NOISE = ("/home/", "/rustc/", "library/", "/cargo/")


def printable(b):
    return all(32 <= c < 127 or c in (9, 10) for c in b)


class Index:
    """薄适配层：扫描与归属全部交给 `loc_xrefs.LocXrefs`，这里只保留旧接口名。"""

    def __init__(self, so=None):
        self.ix = loc_xrefs.LocXrefs(so, verbose=False)
        self.elf = self.ix.elf
        self.data = self.elf.data
        self.md = self.ix.cs.Cs(self.ix.cs.CS_ARCH_X86, self.ix.cs.CS_MODE_64)
        self.md.detail = True
        self.md.syntax = self.ix.cs.CS_OPT_SYNTAX_INTEL
        # ---- 兼容旧属性名（历史上被 import 过的那些）----
        self.locs = self.ix.locs                       # slot -> (file, line, col)
        self.funcstarts = self.ix.starts
        self.lit_of_slot = dict(self.ix.string_slots)  # slot -> (addend, len, text)
        self.lits = {}                                 # addend -> [(len, text, slot)]
        for slot, (addend, ln, text) in self.ix.string_slots.items():
            self.lits.setdefault(addend, []).append((ln, text, slot))
        ro = self.elf.sec(".rodata")
        self.ro_lo, self.ro_hi = ro["addr"], ro["addr"] + ro["size"]
        self.ro_targets = sorted(t for t in self.ix.refs
                                 if self.ro_lo <= t < self.ro_hi)
        self.secs = [dict(name=s["name"], addr=s["addr"], offset=s["offset"],
                          size=s["size"]) for s in self.elf.mapped]

    # ---------- 地址
    def off2va(self, off):
        """**文件偏移** → 虚地址。

        ⚠️ 只对真正的文件偏移用；引用点（`xrefs` / `refs_to`）已经是虚地址，
        v1 的 bug 就是在虚地址上又套了一次这个方法。
        """
        return self.elf.off2va(off)

    def va2off(self, va):
        return self.elf.va2off(va)

    def read(self, va, n):
        return self.elf.read(va, n)

    def func_of(self, va):
        return self.ix.func_of(va)

    def func_end(self, f):
        return self.ix.func_end(f)

    def funcs_ref_slot(self, slot):
        """引用该 Location 结构体的函数集合（首址 / +8 / +16 三阶段，含 G/D 形态）。"""
        return set(self.ix.funcs_of_loc(slot))

    def funcs_ref_addr(self, va):
        """引用任意数据地址 `va` 的函数集合（v1 的 `classes` 在这里也套过错 off2va）。"""
        return {self.ix.func_of(i[0]) for i in self.ix.refs_to(va)} - {None}

    def text_at(self, va, length=None, maxlen=400):
        if length:
            raw = self.read(va, length)
            if raw and printable(raw):
                return raw.decode("utf-8", "replace")
        raw = self.read(va, maxlen)
        if not raw:
            return None
        n = 0
        for c in raw:
            if not (32 <= c < 127 or c in (9, 10)):
                break
            n += 1
        i = bisect.bisect_right(self.ro_targets, va)
        if i < len(self.ro_targets):
            n = min(n, self.ro_targets[i] - va)     # 上界：下一个被引用地址
        return raw[:n].decode("utf-8", "replace") if n else None

    # ---------- 线索 A：这条指令引用到的 (ptr,len) 槽
    def func_slot_literals(self, insn_addr):
        out = []
        for tgt, _form, _mn, _ops in self.ix.refs_from.get(insn_addr, ()):
            if tgt in self.lit_of_slot:
                lva, ln, text = self.lit_of_slot[tgt]
                out.append((tgt, lva, ln, text))
        return out

    # ---------- 线索 B：函数体里的 lea
    def func_literals(self, f):
        from capstone.x86 import X86_OP_MEM, X86_REG_RIP
        out, seen = [], set()
        end = self.func_end(f)
        code = self.read(f, min(end - f, 40000))
        if code is None:
            return out
        insns = list(self.md.disasm(code, f))
        for i, ins in enumerate(insns):
            # 线索 A：这条指令引用了某个 (&str,len) 槽 ⇒ 打出槽里的字符串
            for _slot, lva, ln, text in self.func_slot_literals(ins.address):
                if (lva, ln) not in seen:
                    seen.add((lva, ln))
                    out.append((ins.address, ins.mnemonic, ins.op_str, (lva, ln, text)))
            if ins.mnemonic != "lea":
                continue
            tva = None
            for op in ins.operands:
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    tva = ins.address + ins.size + op.mem.disp
                    break
            if tva is None or not (self.ro_lo <= tva < self.ro_hi):
                continue
            lenreg = LEN_PAIR.get(ins.op_str.split(",")[0].strip())
            ln = None
            if lenreg:
                for nxt in insns[i + 1:i + 4]:
                    m = re.match(re.escape(lenreg) + r", (0x[0-9a-f]+|\d+)$", nxt.op_str) \
                        if nxt.mnemonic == "mov" else None
                    if m and 1 <= int(m.group(1), 0) <= 4096:
                        ln = int(m.group(1), 0)
                        break
            text = self.text_at(tva, ln)
            if text and (tva, text) not in seen:
                seen.add((tva, text))
                out.append((ins.address, ins.mnemonic, ins.op_str, (tva, len(text), text)))
        return out

    def direct_calls(self, f):
        end = self.func_end(f)
        out = []
        for tgt, sites in self.ix.calls.items():
            for s in sites:
                if f <= s < end:
                    out.append((s, tgt))
        return sorted(out)


def fmt(v):
    return "0x%x" % v if v is not None else "?"


def both(v):
    return "%s(gh+%x)" % (fmt(v), GHIDRA_DELTA) if v else "?"


def show(f, ix, label=""):
    print("%sFUNC %s -> %s" % (label, both(f), fmt(ix.func_end(f))))
    for iva, mn, ops, (lva, ln, text) in ix.func_literals(f):
        if any(text.startswith(p) for p in NOISE):
            continue
        print("   %-6s %-34s %-13s %r" % (fmt(iva), ops[:34], fmt(lva), text[:110]))
    calls = ix.direct_calls(f)
    if calls:
        print("   calls: %s" % ", ".join(both(t) for _, t in calls[:24]))


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 1
    so = None
    args = list(argv[1:])
    if "--so" in args:
        i = args.index("--so")
        so = args[i + 1]
        del args[i:i + 2]
    ix = Index(so)
    cmd = args[0]
    if cmd == "classes":
        rx = re.compile(r"graphwarrust::((?:[a-z0-9_]+::)*)([A-Za-z0-9_]+)")
        hits = {}
        for va, rows in ix.lits.items():
            for ln, text, slot in rows:
                for m in rx.finditer(text):
                    hits.setdefault(m.group(0), set()).add((va, slot))
        for tva in ix.ro_targets:
            t = ix.text_at(tva)
            if not t:
                continue
            for m in rx.finditer(t):
                if m.start() == 0:
                    hits.setdefault(m.group(0), set()).add((tva, None))
        for text in sorted(hits):
            refs = set()
            for va, slot in hits[text]:
                if slot is not None:
                    refs |= ix.funcs_ref_slot(slot)
                # 直接引用该字面量的指令（v1 这里也套了 off2va，同样偏 +0x1000）
                refs |= ix.funcs_ref_addr(va)
            print("%-56s %s" % (text, ", ".join(both(f) for f in sorted(refs)) or "-"))
        print("# %d 个类型名" % len(hits))
    elif cmd == "module":
        mod = args[1]
        name = "src/%s.rs" % mod
        print("=== %s" % name)
        for va, rs in sorted(ix.lits.items()):
            for ln, text, slot in rs:
                if text != name:
                    continue
                w = ix.read(slot + 16, 8)
                if not w:
                    continue
                line, col = struct.unpack("<II", w)
                fs = sorted(ix.funcs_ref_slot(slot))
                print("  Location :%d:%-3d struct=%s  funcs: %s" % (
                    line, col, fmt(slot), ", ".join(both(f) for f in fs) or "-"))
                for f in fs:
                    show(f, ix, "   ")
    elif cmd == "func":
        show(int(args[1], 16), ix)
    elif cmd == "literals":
        rx, n = re.compile(args[1]), 0
        for va, rs in sorted(ix.lits.items()):
            for ln, text, slot in rs:
                if rx.search(text):
                    n += 1
                    print("%-13s len=%-4d %s" % (fmt(va), ln, text[:130]))
        print("# %d literal(s)" % n)
    else:
        print(__doc__)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
