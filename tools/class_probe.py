#!/usr/bin/env python3
"""⚠️ 2026-11-07 警告：**本工具的"函数归属"列已被证不可靠，不要直接引用。**

它把「Location 结构体 → 引用它的代码地址」这一跳交给上游 `analysis/xrefs.pkl`，
而那份 pkl 与 `.reference/steam/2026-08-28/libgraphwarrust.so` 不是同一次扫描，
错误形态是**归属漂移到相邻/无关函数**。已观测到的三例（`docs/16` §26.1）：

    :274:129  本工具给 0x4b7a70   重扫给 0x4b5df0
    :218:129  本工具给 0x4b8bd0   重扫给 0x4b7a70
    :68:1     本工具给 0x4b2890   重扫给四个 gdext 入口

**现在应当优先用：**
* `fixtures/name_to_addr.tsv` —— 327 个函数的真名 ↔ x86 地址（跨架构 `panic::Location` 双向唯一 join，`docs/02` §20）
* `tools/symtab_dump.py` —— 78 个模块 / 165 类型 / 290 方法名的权威清单
* arm64 2.0.60 的 `.symtab` + 跨架构指纹（前几轮 agent 的通行做法）

本工具仍可用于**粗筛**（"这个模块大概在哪片地址"），但凡要写进文档的地址，
必须用 objdump / arm64 指纹 / 调用多重集复核过。详见 `docs/16-client.md` §26.1。
"""
# ⚠️ 运行前提：它 import 的是上游 `analysis/elf.py`，那个模块需要 **capstone**。
# 系统 python 没装，必须用仓库的 venv 跑：
#     ../.venv/bin/python tools/class_probe.py classes
#     ../.venv/bin/python tools/class_probe.py module graphwar_texture_loader
# （否则报 ModuleNotFoundError: No module named 'capstone'）


import bisect
import os
import pickle
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
GHIDRA_DELTA = 0x100000

# SysV 传参顺序下 (ptr, len) 的寄存器配对：只有配对的 mov 才当成长度
LEN_PAIR = {"rsi": "edx", "rdi": "esi", "rdx": "ecx", "rcx": "r8d",
            "rax": "edx", "r8": "r9d", "esi": "edx", "edi": "esi"}
NOISE = ("/home/", "/rustc/", "library/", "/cargo/")


def find_upstream():
    cands = [os.environ.get("GW2_UPSTREAM")]
    d = REPO
    for _ in range(4):
        cands.append(d)
        d = os.path.dirname(d)
    for c in cands:
        if c and os.path.exists(os.path.join(c, "libgraphwarrust.so")) \
                and os.path.exists(os.path.join(c, "analysis", "xrefs.pkl")):
            return c
    raise SystemExit("找不到上游目录（libgraphwarrust.so + analysis/*.pkl）；用 GW2_UPSTREAM 指定")


def printable(b):
    return all(32 <= c < 127 or c in (9, 10) for c in b)


class Index:
    def __init__(self):
        up = find_upstream()
        sys.path.insert(0, os.path.join(up, "analysis"))
        from elf import Elf                                    # noqa: E402
        from capstone import Cs, CS_ARCH_X86, CS_MODE_64, CS_OPT_SYNTAX_INTEL   # noqa: E402
        self.E = Elf(os.path.join(up, "libgraphwarrust.so"))
        self.data = self.E.data
        self.relocs = pickle.load(open(os.path.join(up, "analysis", "relocs.pkl"), "rb"))
        self.xrefs = pickle.load(open(os.path.join(up, "analysis", "xrefs.pkl"), "rb"))
        self.funcstarts = sorted(pickle.load(
            open(os.path.join(up, "analysis", "funcstarts_ghidra_va.pkl"), "rb")))
        self.md = Cs(CS_ARCH_X86, CS_MODE_64)
        self.md.detail = True
        self.md.syntax = CS_OPT_SYNTAX_INTEL
        self.secs = [dict(name=s["sname"], addr=s["addr"], offset=s["offset"], size=s["size"])
                     for s in self.E.sections if s["addr"] and s["offset"]]
        ro = next(s for s in self.secs if s["name"] == ".rodata")
        self.ro_lo, self.ro_hi = ro["addr"], ro["addr"] + ro["size"]
        self._build()

    # ---------- 地址
    def off2va(self, off):
        for s in self.secs:
            if s["offset"] <= off < s["offset"] + s["size"]:
                return s["addr"] + (off - s["offset"])
        return None

    def va2off(self, va):
        for s in self.secs:
            if s["addr"] <= va < s["addr"] + s["size"]:
                return s["offset"] + (va - s["addr"])
        return None

    def read(self, va, n):
        o = self.va2off(va)
        return None if o is None or o + n > len(self.data) else self.data[o:o + n]

    # ---------- 线索 A：(ptr,len)
    def _build(self):
        slot_target = {}
        for slot, entries in self.relocs.items():
            for typ, addend in entries:
                if typ == 8:
                    slot_target[slot] = addend
        self.lit_of_slot = {}
        self.lits = {}                    # lit_va -> [(len, text, slot)]
        for slot, addend in slot_target.items():
            w = self.read(slot + 8, 8)
            if not w:
                continue
            ln = struct.unpack("<Q", w)[0]
            if not (1 <= ln <= 4096):
                continue
            raw = self.read(addend, ln)
            if not raw or len(raw) != ln or not printable(raw):
                continue
            try:
                text = raw.decode("utf-8")
            except UnicodeDecodeError:
                continue
            self.lit_of_slot[slot] = (addend, ln, text)
            self.lits.setdefault(addend, []).append((ln, text, slot))
        self.ro_targets = sorted({t for t in self.xrefs
                                  if self.ro_lo <= t < self.ro_hi})

    def func_of(self, va):
        i = bisect.bisect_right(self.funcstarts, va) - 1
        return self.funcstarts[i] if i >= 0 else None

    def func_end(self, f):
        i = bisect.bisect_right(self.funcstarts, f)
        return self.funcstarts[i] if i < len(self.funcstarts) else self.ro_lo

    def funcs_ref_slot(self, slot):
        out = set()
        for t in self.xrefs.get(slot, ()):
            va = self.off2va(t[0])
            if va is not None:
                f = self.func_of(va)
                if f is not None:
                    out.add(f)
        return out

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
            for tgt, kind, mn, ops in self.xrefs.get(ins.address, ()):
                if tgt in self.lit_of_slot:
                    lva, ln, text = self.lit_of_slot[tgt]
                    if (lva, ln) not in seen:
                        seen.add((lva, ln))
                        out.append((ins.address, mn, ops, (lva, ln, text)))
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
        for tgt, lst in self.xrefs.items():
            for off, kind, mn, _ in lst:
                if kind == "imm" and mn == "call":
                    va = self.off2va(off)
                    if va is not None and f <= va < end:
                        out.append((va, tgt))
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
    ix = Index()
    cmd = argv[1]
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
                for off in ix.xrefs.get(va, ()):
                    f = ix.func_of(ix.off2va(off[0]) or 0)
                    if f:
                        refs.add(f)
            print("%-56s %s" % (text, ", ".join(both(f) for f in sorted(refs)) or "-"))
        print("# %d 个类型名" % len(hits))
    elif cmd == "module":
        mod = argv[2]
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
        show(int(argv[2], 16), ix)
    elif cmd == "literals":
        rx, n = re.compile(argv[2]), 0
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
