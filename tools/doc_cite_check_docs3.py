#!/usr/bin/env python3
"""审计面④第三轮用：`docs/144`–`docs/162`（19 篇）里所有「地址 + 指令」引文的**全量机核**。

## 它回答什么

T40（`docs/06` §88）/ T65（§112）那两轮只做到"抽样判档 + 部分引文机核"。本工具把
**引文核对这一步做成可复算的一条命令**：从 19 篇后期文档里抽出每一处「地址 + 助记符 + 操作数」，
拿**真 `.so`** 反汇编首条（或 ±0x40 内最近的同助记符）逐条比对，输出四类：

| 判定 | 含义 |
|---|---|
| `MATCH` / `MNEM-OK` | 干净：地址上确实是这条指令（操作数只差写法：AT&T 顺序、`%reg`、`byte ptr`、rip 解引用成绝对地址、符号名代替寄存器） |
| `ADDR-OFF(±N)` | 该地址上不是这条指令，但**同助记符**在 ±0x40 内 ⇒ 文档地址错位 N 字节（引文级错） |
| `MISMATCH` | 该地址附近找不到这条指令（地址错得更多 / 引文抄错） |
| `SKIP-*` | 一行多指令、伪码、`⇒` 推导、不在可执行段 —— 人工看 |

x86-64 引文对 `.reference/game/libgraphwarrust.so`（= Linux 2.0.60，md5 `b156c315…`）；
arm64 引文（`bl <名字>` / `stp q0,q1` 这类）对 `.reference/steam/android-2.0.60/libgraphwarrust.so`，
符号名走它自己的 `.symtab`（`_ZN…17h<hash>E` 去 hash 后取最后两级）。

## 用法

```bash
.reference/venv/bin/python tools/doc_cite_check_docs3.py                # 统计 + 逐条（有问题的才打印）
.reference/venv/bin/python tools/doc_cite_check_docs3.py --tsv /tmp/x.tsv
.reference/venv/bin/python tools/doc_cite_check_docs3.py --only docs/162 --all
```

## 口径与已知盲区

* 地址一律 **ELF vaddr**（= 模块内偏移 = RVA），与 `AGENTS.md`「反复踩过的坑」第 1 条一致。
* 只认**地址在前**的引文（`0xADDR <mnem> <ops>` / 代码块里 `ADDR <mnem> <ops>` / 表格
  `| 0xADDR | mnem ops |`）。`mnem … @0xADDR` 这种"指令在前"的写法**故意不抽**——
  第一版试过，会把 `0x51a162 lea … @0x51a170 call` 里的 `lea` 错配到后一个地址上（假错）。
* 一行里只有地址、没有指令的句子（本批有 1,300+ 条）不参与机核，只统计条数。
* 操作数只做"数值集合 + 写法"比较，**不判语义**：`mov rax,[rbx]` 与 `mov rax,[rbx+0]`
  会判成干净。语义要看文档结论，靠人读。
"""

import argparse
import json
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

X86_SO = os.environ.get("GW2_SO", os.path.join(ROOT, ".reference/game/libgraphwarrust.so"))
A64_SO = os.environ.get(
    "GW2_A64_SO", os.path.join(ROOT, ".reference/steam/android-2.0.60/libgraphwarrust.so"))

DOCS = [
    "docs/144-lobby-fixes.md",
    "docs/147-ai-behaviour.md",
    "docs/149-coverage-matrix.md",
    "docs/150-win-rule.md",
    "docs/151-addr-open-items.md",
    "docs/152-audit-of-merged-claims.md",
    "docs/153-audit-downstream.md",
    "docs/154-docs-audit.md",
    "docs/158-audit-407.md",
    "docs/159-audit-fingerprint.md",
    "docs/160-verify-782.md",
    "docs/161-manager-gaps.md",
    "docs/162-audit-tiers.md",
    "docs/archive/145-tab-fix.md",
    "docs/archive/146-source-recovery.md",
    "docs/archive/148-guide-manual.md",
    "docs/archive/155-round-log.md",
    "docs/archive/156-client-rounds.md",
    "docs/archive/157-name-matching.md",
]

MNEMS = set("""
mov add sub lea cmp test xor and or not shl shr sar sal imul idiv div mul inc dec neg
jmp je jne jz jnz ja jae jb jbe jg jge jl jle js jns jo jno jp call ret leave push pop
movsx movzx movsxd movsd movss movq movd movdqu movaps movapd movups movupd movhlps
movdqa movlhps punpcklqdq pmuludq cvtsi2sd cvttsd2si cvtsd2ss addsd subsd mulsd divsd
addss subss mulss divss ucomisd comisd ucomiss cmpsd xorpd xorps andpd sqrt sqrtsd
cmpss lock cmpxchg xchg nop bt bts btr setne sete setb movabs shld shrd ror rol
cdqe cltq stos stosd rep movsq cmove cmovb cmovne cmovae cmovg cmovl cqo cbw cwd
adc sbb rcl rcr fld fst fadd fsub fmul fdiv movbe bextr crc32 bswap bsf bsr tzcnt lzcnt
cont popcnt prefetcht0 vmovsd vmovups vmovq vmovd vmovdqu vpxor vaddsd vmulsd vcvtsi2sd
bl ldr str adr cbz cbnz tbz tbnz fmov sdiv udiv fmadd scvtf fcvtzs movz movk stp ldp
blr br cmeq fcvt ldur stur ldrsb ldrb ldrh strb
""".split())

ARM_ONLY = {"bl", "b", "ldr", "str", "adr", "cbz", "cbnz", "tbz", "tbnz", "fmov", "sdiv",
            "udiv", "fmadd", "scvtf", "fcvtzs", "movz", "movk", "stp", "ldp", "blr",
            "ldur", "stur", "ldrsb", "ldrb", "ldrh", "strb", "cmeq", "fcvt"}
ARM_OPS = re.compile(r"\b[dsqvh]\d+\b|\b[wx]\d+\b|\bsp\b|\b#[-\d]")

CITE_RE = re.compile(
    r"(?:\b0x([0-9a-f]{5,7})\b|\|?\s*(?:0x)?([0-9a-f]{5,7})\s+)\s*[`|]?\s*"
    r"([a-z][a-z0-9]{1,9})\b\s*([^|`\n]{0,90})")

FAM = {"movzbl": "movzx", "movzwl": "movzx", "movsbl": "movsx", "movswl": "movsx",
       "movslq": "movsxd", "movl": "mov", "movw": "mov", "movb": "mov", "movq": "mov",
       "movabs": "mov", "movabsq": "mov", "testl": "test", "testb": "test", "testq": "test",
       "cmpl": "cmp", "cmpb": "cmp", "cmpq": "cmp", "addl": "add", "addq": "add",
       "subl": "sub", "subq": "sub", "leal": "lea", "leaq": "lea", "callq": "call",
       "retq": "ret", "pushq": "push", "popq": "pop", "xorl": "xor", "orq": "or",
       "shlq": "shl", "shrq": "shr", "andl": "and"}


# --------------------------------------------------------------------------- 抽取
def clean_ops(ops):
    ops = (ops or "").strip()
    for sep in ("⇒", "->", "→", "；", ";", "  #", "←", "═", " = "):
        if sep in ops:
            ops = ops.split(sep)[0]
    return ops.strip().rstrip(".,:）")


def harvest(path):
    """返回 (引文列表, 只有地址的行数)。"""
    cites, addrlines = [], 0
    lines = open(path, encoding="utf-8", errors="replace").read().split("\n")
    for i, ln in enumerate(lines, 1):
        if not re.search(r"[0-9a-f]{5,7}", ln):
            continue
        has_addr = bool(re.search(r"\b0x[0-9a-f]{5,7}\b", ln))
        if has_addr:
            addrlines += 1
        seen = set()
        for m in CITE_RE.finditer(ln):
            a1, a2, mnem, ops = m.group(1), m.group(2), m.group(3), m.group(4)
            if mnem not in MNEMS:
                continue
            addr = a1 or a2
            if not a1:  # 代码块里没有 0x 前缀：必须真是行首数字列
                if ln[:m.start(2)].strip(" |`") != "":
                    continue
                if not (0x1C0000 <= int(addr, 16) <= 0x9FFFFF):
                    continue
            if (addr, mnem) in seen:
                continue
            seen.add((addr, mnem))
            o = clean_ops(ops)
            if mnem == "lock" and re.match(r"^[a-z]{2,9}\b", o):
                mnem, o = o.split(None, 1)[0], o.split(None, 1)[1] + "" if " " in o else ""
            cites.append({"line": i, "addr": addr, "mnem": mnem,
                          "ops": o, "text": ln.strip()[:300]})
    return cites, addrlines


# --------------------------------------------------------------------------- ELF / symtab
class AnyElf:
    """极简 ELF64：vaddr→file offset + 读字节 + .symtab 名字表（x86-64 / arm64 通用）。"""

    def __init__(self, path):
        self.data = d = open(path, "rb").read()
        assert d[:4] == b"\x7fELF", "not ELF"
        self.e_machine = struct.unpack_from("<H", d, 0x12)[0]
        e_shoff = struct.unpack_from("<Q", d, 0x28)[0]
        e_shentsize = struct.unpack_from("<H", d, 0x3A)[0]
        e_shnum = struct.unpack_from("<H", d, 0x3C)[0]
        e_shstrndx = struct.unpack_from("<H", d, 0x3E)[0]
        secs = []
        for i in range(e_shnum):
            o = e_shoff + i * e_shentsize
            (name, typ, flags, addr, off, size, link, info, align, ent) = \
                struct.unpack_from("<IIQQQQIIQQ", d, o)
            secs.append({"name": name, "typ": typ, "addr": addr, "off": off,
                         "size": size, "link": link, "entsize": ent})
        self.secs = secs
        shstr = secs[e_shstrndx]

        def sname(ofs):
            s = shstr["off"] + ofs
            return d[s:d.index(b"\0", s)].decode()
        self.seg = []       # (vaddr_lo, vaddr_hi, file_off)
        self.names = {}     # 短名 → [addr...]
        for s in secs:
            nm = sname(s["name"])
            if nm == ".symtab" and s["size"]:
                self._load_syms(s, secs)
        e_phoff = struct.unpack_from("<Q", d, 0x20)[0]
        e_phnum = struct.unpack_from("<H", d, 0x38)[0]
        for i in range(e_phnum):
            o = e_phoff + i * 0x38
            p_type = struct.unpack_from("<I", d, o)[0]
            p_off, p_va, _, p_filesz, _, _ = struct.unpack_from("<QQQQQQ", d, o + 8)
            if p_type == 1:
                self.seg.append((p_va, p_va + p_filesz, p_off - p_va))

    def _load_syms(self, s, secs):
        d = self.data
        strtab = secs[s["link"]]
        for k in range(s["size"] // 24):
            o = s["off"] + k * 24
            st_name, _st_info, _o, _shndx, st_value, _sz = struct.unpack_from("<IBBHQQ", d, o)
            if not st_name or not st_value:
                continue
            b = strtab["off"] + st_name
            nm = d[b:d.index(b"\0", b)].decode("utf-8", "replace")
            parts = _demangle(nm)
            if not parts:
                continue
            short = parts[-1]
            self.names.setdefault(short, []).append(st_value)
            if len(parts) >= 2:
                self.names.setdefault(parts[-2] + "::" + short, []).append(st_value)

    def read(self, va, n):
        for lo, hi, delta in self.seg:
            if lo <= va < hi:
                off = va + delta
                return self.data[off:off + min(n, hi - va)]
        raise ValueError("vaddr 0x%x 不在任何 PT_LOAD" % va)


def a64_sym(elk, name):
    return elk.names.get(name)


def _demangle(mangled):
    """Rust legacy `_ZN<len><ident>...E` → 组件列表（跳过 `17h<hash>` 段）。够用即可。"""
    if not mangled.startswith("_ZN"):
        return []
    body = mangled[3:]
    out, i = [], 0
    while i < len(body):
        if body[i] == "E":
            break
        m = re.match(r"(\d+)", body[i:])
        if not m:
            return out or []
        n = int(m.group(1))
        i += len(m.group(1))
        ident = body[i:i + n]
        i += n
        if re.fullmatch(r"h[0-9a-f]{16}", ident):
            continue
        out.append(ident)
    return out


# --------------------------------------------------------------------------- 判定
def fam(m):
    m = m.replace("lock ", "").replace("rep ", "")
    return FAM.get(m, m)


def _att_mem(s):
    """`0x8(%r14,%r13,1)` / `(%rdx,%r12,1)` / `-0x4(%rcx)` → `[r14+r13*1+0x8]` 这类规范形。"""
    def repl(m):
        disp, base, idx, scale = m.group(1), m.group(2), m.group(3), m.group(4)
        parts = []
        if base:
            parts.append(base)
        if idx:
            parts.append(idx + ("*" + scale if scale else ""))
        if disp:
            parts.append(disp.lstrip("+"))
        return "[" + "+".join(parts) + "]" if parts else "[0]"
    s = re.sub(r"(-?0x[0-9a-f]+)?\((%[a-z0-9]+)?(?:,(%[a-z0-9]+)(?:,(\d))?)?\)", repl, s)
    return s.replace("%", "")


def norm(s):
    s = (s or "").lower()
    s = s.replace("\u2026", "").replace("...", "")
    s = re.sub(r"\b(byte|word|dword|qword|xmmword)\s*ptr\b", " ", s)
    s = re.sub(r"\bptr\b", " ", s)
    s = _att_mem(s) if "%" in s else s
    s = re.sub(r"\*1(?![0-9])", "", s)          # AT&T 的 scale=1 是默认值
    s = re.sub(r"(?<![0-9])1\*", "", s)
    s = re.sub(r"0x([0-9a-f]+)", lambda m: str(int(m.group(1), 16)), s)   # 0x8 == 8
    s = s.replace("[", "(").replace("]", ")")
    s = re.sub(r"\(([^()]*)\)", lambda m: "(" + m.group(1).replace(",", "+") + ")", s)
    s = re.sub(r"\s+", "", s)
    s = s.rstrip(".,;:）】]")
    return s


def nums(s):
    out = []
    s = re.sub(r"(?<=[0-9a-f])_(?=[0-9a-f])", "", (s or "").lower())
    for tok in re.findall(r"(?<![0-9a-z])(0x[0-9a-f]+|\d+)(?![0-9a-z])", s):
        try:
            out.append(int(tok, 16 if tok.startswith("0x") else 10))
        except ValueError:
            pass
    return sorted(out)


def _split_top(ops):
    """按顶层逗号切（括号里的逗号不算）。"""
    out, depth, cur = [], 0, ""
    for ch in ops or "":
        if ch in "([":
            depth += 1
        elif ch in ")]":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur)
            cur = ""
            continue
        cur += ch
    out.append(cur)
    return [p.strip() for p in out if p.strip()]


def _att_swap(ops):
    """AT&T → Intel 顺序（含 `%` 的写法，任意操作数个数）。"""
    if "%" not in (ops or ""):
        return None
    parts = _split_top(ops)
    return ",".join(reversed(parts)).replace("%", "")


def _rip_target(ins, ops):
    """`[rip ± 0x..]` → 目标绝对地址（给"文档直接写解析后地址"那种写法）。"""
    m = re.search(r"\[rip ([+-]) 0x([0-9a-f]+)\]", (ops or "").lower())
    if not m:
        return None
    disp = int(m.group(2), 16) * (1 if m.group(1) == "+" else -1)
    return ins.address + ins.size + disp


def ops_ok(doc_ops, ins):
    raw = (doc_ops or "").strip()
    if "/" in raw:                       # 一行里塞了多条指令（伪码）
        return True
    if re.match(r"^[A-Za-z_][\w:]*$", raw):   # `jbe skip` / `bl get_position`：符号目标
        return True
    if re.fullmatch(r"[0-9a-f]{5,7}", raw):    # `call 46ee00` / `jne 475b20`：裸十六进制目标
        return int(raw, 16) in nums(ins.op_str) or \
            (_rip_target(ins, ins.op_str) == int(raw, 16))
    if re.search(r"0x\w*…", raw):        # `0x8000…0033` 这类缩写：比头尾
        head = re.search(r"(0x[0-9a-f]{1,8})…([0-9a-f]{1,8})", raw)
        rn2 = nums(ins.op_str)
        if head and rn2:
            v = rn2[0]
            hx = "%x" % v
            if hx.startswith(head.group(1)[2:]) and hx.endswith(head.group(2)):
                return True
    d, r = norm(doc_ops), norm(ins.op_str)
    if re.match(r"^[a-z]+[0-9]{1,2}b$", d) and r.startswith(d + ","):   # `test %r14b` 省了第二操作数
        return True
    if not d or d == r or d.lstrip("%") == r.lstrip("%"):
        return True
    att = _att_swap(doc_ops)
    if att and norm(att) == r:
        return True
    dn, rn = nums(doc_ops), nums(ins.op_str)
    if dn and dn == rn:
        return True
    if dn and set(dn) <= set(rn):
        # 符号基址（[payload+0x70] / [ctx+0x58] / [isize::MAX+0x24]）：文档省略了真实基址寄存器
        return True
    tgt = _rip_target(ins, ins.op_str)
    if tgt is not None and nums(doc_ops) and tgt in nums(doc_ops):
        return True
    if d in norm(ins.mnemonic + ins.op_str) or r in d:
        return True
    reg = r"\b[er]?([abcd]x|si|di|bp|sp)\b"
    if re.sub(reg, "R", d) == re.sub(reg, "R", r):
        return True
    # 最后一招：寄存器多重集 + 数字多重集都相同（AT&T 顺序、*1、0x8 vs 8、括号写法差异）
    if _regset(doc_ops) and _regset(doc_ops) == _regset(ins.op_str):
        a = sorted(set(nums(norm(doc_ops))))
        b = sorted(set(nums(norm(ins.op_str))))
        if a == b:
            return True
        if a and set(a) <= set(b) and not (set(b) - set(a)):
            return True
    return False


REGS = re.compile(r"\b(r[a-z0-9]{1,3}|e[a-z]{2}|[abcd][xlh]|[sd]il|[sb]pl|xmm\d+|ymm\d+)\b")


def _regset(ops):
    s = _att_mem((ops or "").lower())
    return sorted(REGS.findall(s.replace("%", "")))


def judge_x86(ctx, va, mnem, ops):
    ins = list(ctx._md.disasm(ctx.read(va, min(24, 0x1000)), va))
    if not ins:
        return "SKIP-NOTEXT"
    first = ins[0]
    if fam(mnem) == fam(first.mnemonic):
        return "MATCH" if ops_ok(ops, first) else "OPS-DIFF"
    owner = ctx.owner(va) or va
    lo = max(owner, va - 0x40)
    hits = [i.address for i in ctx._md.disasm(ctx.read(lo, va - lo + 0x80), lo)
            if fam(i.mnemonic) == fam(mnem)]
    if hits:
        best = min(hits, key=lambda a: abs(a - va))
        return "ADDR-OFF(%+#x)" % (best - va)
    return "MISMATCH"


def judge_arm64(elk, va, mnem, ops):
    try:
        ins = list(MDA.disasm(elk.read(va, 32), va))
    except ValueError:
        return "SKIP-NOTEXT"
    if not ins:
        return "SKIP-NOTEXT"
    first = ins[0]
    if fam(mnem) == fam(first.mnemonic):
        if ops_ok(ops, first):
            return "MATCH"
        # `bl <名字>`：花名换成地址再比
        m = re.match(r"^([A-Za-z_][\w:]{2,})\s*$", (ops or "").strip())
        if m and mnem in ("bl", "b", "blr"):
            cands = a64_sym(elk, m.group(1)) or a64_sym(elk, m.group(1).split("::")[-1])
            t = re.search(r"#?(0x[0-9a-f]+)", first.op_str)
            if cands and t and int(t.group(1), 16) in cands:
                return "MATCH"
        return "OPS-DIFF"
    for i in MDA.disasm(elk.read(va, 0x60), va):
        if fam(i.mnemonic) == fam(mnem):
            return "A64-ADDR-OFF(%+#x)" % (i.address - va)
    return "MISMATCH"


# --------------------------------------------------------------------------- main
def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--tsv", help="把逐条结果写成 TSV")
    ap.add_argument("--only", help="只跑某篇（子串匹配）")
    ap.add_argument("--all", action="store_true", help="打印每一条，不只打印有问题的")
    args = ap.parse_args(argv)

    try:
        import capstone  # noqa: F401
    except ImportError:
        sys.exit("需要 capstone：用 .reference/venv/bin/python 跑本脚本")
    global MDA
    from capstone import Cs, CS_ARCH_ARM64, CS_MODE_ARM
    MDA = Cs(CS_ARCH_ARM64, CS_MODE_ARM)

    import addr_probe as AP
    ctx = AP.Ctx(X86_SO)
    from capstone import Cs as _Cs, CS_ARCH_X86, CS_MODE_64
    ctx._md = _Cs(CS_ARCH_X86, CS_MODE_64)
    elk = AnyElf(A64_SO)

    docs = [d for d in DOCS if not args.only or args.only in d]
    rows, stat, per_doc = [], {}, {}
    addr_only = 0
    for doc in docs:
        cites, addrlines = harvest(doc)
        addr_only += addrlines - len({c["line"] for c in cites})
        for c in cites:
            va = int(c["addr"], 16)
            is_arm = c["mnem"] in ARM_ONLY or (
                bool(ARM_OPS.search(c["ops"])) and
                not re.search(r"\[|rip|xmm|r\d+[dwb]", (c["ops"] or "").lower()))
            v = (judge_arm64 if is_arm else judge_x86)(elk if is_arm else ctx, va,
                                                       c["mnem"], c["ops"])
            k = v.split("(")[0]
            stat[k] = stat.get(k, 0) + 1
            per_doc.setdefault(doc, {}).setdefault(k, 0)
            per_doc[doc][k] += 1
            rows.append(dict(doc=doc, line=c["line"], addr="0x" + c["addr"],
                             doc_ops="%s %s" % (c["mnem"], c["ops"]),
                             verdict=v, arch="a64" if is_arm else "x86", text=c["text"]))

    bad = [r for r in rows if r["verdict"].split("(")[0] in ("ADDR-OFF", "MISMATCH", "OPS-DIFF")]
    print("== 引文机核（%d 篇）==" % len(docs))
    for k in sorted(stat):
        print("  %-16s %d" % (k, stat[k]))
    print("  合计 %d 条「地址+指令」引文；另有 %d 行只有地址没有指令（不参与机核）"
          % (len(rows), addr_only))
    print("\n== 按篇 ==")
    for doc in docs:
        d = per_doc.get(doc, {})
        print("  %-42s %3d 条  %s" % (doc, sum(d.values()),
                                      " ".join("%s=%d" % kv for kv in sorted(d.items()))))
    print("\n== 需人工看的 %d 条 ==" % len(bad))
    for r in bad if not args.all else rows:
        print("  %s:%d %s [%s] doc[%s] → %s" %
              (r["doc"], r["line"], r["addr"], r["arch"], r["doc_ops"][:60], r["verdict"]))
    if args.tsv:
        with open(args.tsv, "w", encoding="utf-8") as f:
            f.write("doc\tline\taddr\tarch\tdoc_ops\tverdict\ttext\n")
            for r in rows:
                f.write("\t".join(str(r[k]) for k in
                                  ("doc", "line", "addr", "arch", "doc_ops", "verdict", "text")) + "\n")
        print("\nTSV →", args.tsv)


if __name__ == "__main__":
    main()
