#!/usr/bin/env python3
"""F3 缺口逐条分类（**度量口径审计**，只审计不改码）。

## 为什么有这个工具

`fixtures/function_coverage.tsv` 把 527 个原版可归属函数分成 F1/F2/F3/F4。分母 527 是诚实的，
但 **F3 的 153 条不是同一种东西**：T71 在三个重模块的 35 条 F3 里逐指令取证后发现，
33 条**根本不该由我们手写**（12 条 = `core::slice::sort` 的单态化、19 条 = gdext 宏产物）。
把它们算进"缺口"会让函数级复原率**虚低**；反过来，若把 std 内核的地址写进我们函数的注释，
`tools/function_coverage.py` 的 P2（地址指认）会**虚高**（T71 实测假抬 5 条 F1，docs/06 §123.5）。

本工具按 **`fixtures/f3_triage_plan.txt` 预先写死**的判据，把 F3 逐条判成
`real-missing / na:std-mono / na:gdext-glue / na:other / unknown` 之一，并写出
`fixtures/f3_triage.tsv`（`vaddr | 名字 | FDE尺寸 | 判据命中 | 类别 | 依据`）。

## 口径

* 地址一律 **ELF vaddr（RVA）**；Ghidra/REA = 本工具地址 + 0x100000（只加一次）。
* 函数范围 = `.eh_frame` FDE（自解，不读 `analysis/*.pkl`）；引用/表项 = `.rela.dyn`（`R_X86_64_RELATIVE`）。
* 归属 = `panic::Location`（`.data.rel.ro` 的 {file,len,line,col}）。
* 反汇编 = capstone x86-64；ELF/FDE 复用 `tools/addr_probe.py` 的 `Ctx`（同一套解析，避免两套口径）。

## 用法

    PY=.reference/venv/bin/python
    $PY tools/f3_triage.py --features <vaddr> [<vaddr> ...]   # 打印特征（人工复核用）
    $PY tools/f3_triage.py --control                          # 正/负对照（先跑这个）
    $PY tools/f3_triage.py                                    # 全量：写 fixtures/f3_triage.tsv + 打印统计
    $PY tools/f3_triage.py --stats                            # 只打印统计（不写文件）

**本工具不改任何计分工具**：`tools/function_coverage.py` 的 F 档是它的输入，不是它的输出。
"""

from __future__ import annotations

import argparse
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import addr_probe as ap  # noqa: E402
from capstone import Cs, CS_ARCH_X86, CS_MODE_64  # noqa: E402
from capstone.x86 import (  # noqa: E402
    X86_OP_IMM, X86_OP_MEM, X86_OP_REG, X86_REG_RIP,
)

FAKE = "—"
OUT_TSV = os.path.join(ROOT, "fixtures", "f3_triage.tsv")

# 预注册正对照（fixtures/f3_triage_plan.txt §4）—— 台账 fixtures/f3_heavy_anchors.tsv 逐行
CTRL_STD = """0x41a9d0 0x41ab40 0x41b500 0x41b720 0x41b880 0x41bdf0
0x4600e0 0x4602f0 0x481140 0x481870 0x4825b0 0x482c20""".split()
CTRL_GDEXT = """0x3aa6a0 0x3aa820 0x3b56c0 0x3b5910 0x3b5ce0
0x3b6e00 0x3b6ed0 0x3b7020 0x426430 0x433c00 0x4dec00 0x4e3b00 0x4e3c80 0x4e8f80
0x4e91d0 0x4e9380 0x4e9590 0x500070 0x5005b0""".split()
# G4（源码闭包）按预注册判 unknown —— 这三条是 T71 记为"源码闭包、父函数无真名"的
CTRL_GDEXT_CLOSURE = {"0x4dec00", "0x500070", "0x5005b0"}
CTRL_REAL = ["0x3afcf0", "0x3b4ec0"]              # T71 落码的两条业务函数
CTRL_REAL_EXTRA = ["0x519630", "0x519840", "0x4ee720"]  # 已知 F1 真业务（防"全判 na"）
CTRL_STD_SIZES = [0x164, 0x158, 0x564, 0x100, 0x721, 0x66f]  # 台账：每对逐字节相同

PREFIX = {
    "lock cmpxchg": "ATOMIC", "ucomisd": "CMP", "self-call": "SELF",
    "stride": "STRIDE", "bulk": "BULK", "cmov": "CMOV", "borrow": "BORROW",
    "slot": "SLOT", "pair": "PAIR", "loc": "LOC", "caller-loc": "CALLER_LOC",
}


# ---------------------------------------------------------------------------
# 特征提取
# ---------------------------------------------------------------------------
class Feat:
    __slots__ = (
        "va", "start", "end", "size", "n_insn", "self_calls", "strides",
        "ucomisd", "jp", "bulk", "cmov", "atomics", "borrow", "locs",
        "callers", "slots", "lea_refs", "caller_loc_n", "callees",
        "n_direct_call", "n_plt", "strings", "cmpsd", "ret_like",
        "first", "last_ops",
    )


def secname(ctx, va):
    for s in ctx.elf.secs:
        if s["size"] and s["addr"] <= va < s["addr"] + s["size"]:
            return s["name"]
    return None


def is_writable_global(ctx, va):
    n = secname(ctx, va)
    return n in (".data", ".bss", ".got", ".data.rel.ro")


def locs_of_range(ctx, lo, hi):
    """该范围引用的 panic::Location（按 file 聚合，保留出现顺序）。"""
    votes = collections.Counter()
    seen = {}
    for ins in ap.dis_list(ctx, lo, hi):
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                t = ins.address + ins.size + op.mem.disp
                loc = ap.maybe_location(ctx, t)
                if loc:
                    votes[loc] += 1
                    seen.setdefault(loc, ins.address)
    return votes, seen


def strings_of_range(ctx, lo, hi, cap=6):
    out = []
    for ins in ap.dis_list(ctx, lo, hi):
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                t = ins.address + ins.size + op.mem.disp
                if t in ctx.rel or ap.maybe_location(ctx, t):
                    continue
                s = ctx.cstr(t)
                if len(s) >= 6 and all(0x20 <= c < 0x7F for c in s):
                    txt = s.decode("ascii")
                    if txt not in out:
                        out.append(txt)
    return out[:cap]


def features(ctx, va, index):
    f = Feat()
    f.va = va
    ex = ctx.fde_extent(va)
    if not ex:
        f.start = f.end = va
        f.size = 0
        return f
    f.start, f.end = ex
    f.size = ex[1] - ex[0]
    insns = ap.dis_list(ctx, f.start, f.end)
    f.n_insn = len(insns)
    f.self_calls = 0
    f.strides = []
    f.ucomisd = 0
    f.jp = 0
    f.bulk = 0
    f.cmov = 0
    f.cmpsd = 0
    f.atomics = []
    f.borrow = []
    f.callers = 0
    f.slots = 0
    f.lea_refs = 0
    f.n_direct_call = 0
    f.n_plt = 0
    f.callees = []
    f.ret_like = False
    f.first = [(i.mnemonic, i.op_str) for i in insns[:3]]
    f.last_ops = [(i.mnemonic, i.op_str) for i in insns[-2:]]

    call_imm, rip, imm = index

    for k, ins in enumerate(insns):
        m = ins.mnemonic
        ops = ins.operands
        # 自递归 / 直接调用
        if m == "call":
            if ops and ops[0].type == X86_OP_IMM:
                tgt = ops[0].imm & 0xFFFFFFFFFFFFFFFF
                f.n_direct_call += 1
                if tgt == f.start:
                    f.self_calls += 1
                else:
                    f.callees.append(tgt)
                    sn = secname(ctx, tgt)
                    if sn in (".plt", ".plt.sec", ".plt.got"):
                        f.n_plt += 1
        # 元素步长
        if m in ("imul", "mul") and len(ops) == 3 and ops[2].type == X86_OP_IMM:
            v = ops[2].imm
            if 0x20 <= v <= 0x10000:      # 除法魔数（0xaaaaaaab 之类）不算元素步长
                f.strides.append(v)
        if m.startswith("ucomis") or m in ("comisd", "comiss"):
            f.ucomisd += 1
            f.cmpsd += 1
        if m in ("jp", "jpe", "jnp", "jpo"):
            f.jp += 1
        if m in ("movups", "movaps", "movdqu", "movdqa", "vmovups", "vmovaps"):
            f.bulk += 1
        if m.startswith("cmov"):
            f.cmov += 1
        # ATOMIC 懒初始化单例
        if m.endswith("cmpxchg") and 240 in (getattr(ins, "prefix", None) or ()):  # 0xf0 = LOCK
            for op in ops:
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    t = ins.address + ins.size + op.mem.disp
                    if is_writable_global(ctx, t) and t not in f.atomics:
                        f.atomics.append(t)
        # BORROW：借用标记 0 -> -1 写在 [reg+0x10]
        if m == "mov" and len(ops) == 2 and ops[0].type == X86_OP_MEM and ops[1].type == X86_OP_IMM:
            if ops[1].imm & 0xFFFFFFFFFFFFFFFF == 0xFFFFFFFFFFFFFFFF and ops[0].mem.disp == 0x10:
                f.borrow.append(ins.address)

    if f.n_insn <= 3 and all(x[0] in ("ret", "xor", "nop", "push", "pop", "mov") for x in f.first):
        f.ret_like = f.n_insn <= 3 and insns and insns[-1].mnemonic in ("ret", "jmp")

    # Location / 串
    votes, seen = locs_of_range(ctx, f.start, f.end)
    f.locs = [(loc, n) for loc, n in votes.most_common()]
    f.strings = strings_of_range(ctx, f.start, f.end)

    # 调用点 / 数据表槽
    f.callers = len(call_imm.get(f.start, ()))
    f.slots = len([k for k, v in ctx.rel.items() if v == f.start])
    f.lea_refs = len(rip.get(f.start, ())) + len(imm.get(f.start, ()))

    # 唯一/主要调用者自己引用 Location 吗（=0 ⇒ 调用者是 std/gdext/引擎侧）
    sites = sorted(call_imm.get(f.start, ()))
    f.caller_loc_n = -1
    if len(sites) == 1:
        cex = ctx.fde_extent(sites[0])
        if cex:
            cv, _ = locs_of_range(ctx, cex[0], cex[1])
            f.caller_loc_n = len(cv)
    return f


# ---------------------------------------------------------------------------
# 判决（规则见 fixtures/f3_triage_plan.txt §3）
# ---------------------------------------------------------------------------
def g4_closure(row, f, shared_locs):
    """预注册 G4（**勘误 §9** 细化）：同一 `panic::Location` 被 ≥2 条无名 F3 行引用 ⇒
    同一个源码构件（闭包/宏展开）的多个编译产物，**父函数在符号表里没有真名** ⇒ 判不了。

    依据：T71 §123.2 第 4 条对 lan_vbox 的三条（`0x4dec00/0x500070/0x5005b0`）就是这么判的
    —— 它们共享 `src/graphwar_lan_vbox.rs:59:40`，而本模块唯一 v2 锚 `process` 在原版行 90 > 59
    ⇒ 闭包不在任何已命名函数范围内。本函数把"共享同一 Location + 无名"做成机判。
    """
    if not f.locs or not row or not shared_locs:
        return False, ""
    if str(row.get("name_src") or "").startswith("v2"):
        return False, ""
    for loc, _ in f.locs:
        if loc in shared_locs:
            return True, ("Location %s 被 ≥2 条无名 F3 行共享 ⇒ 同一源码构件的多个编译产物，"
                          "父函数无真名" % loc)
    return False, ""


def rule_hits(f, size_freq, row=None, shared_locs=None):
    """返回 (命中列表, 说明 dict)。命中项字符串会被写进 TSV 的「判据命中」列。"""
    hits = []
    cmp_hit = f.ucomisd >= 2 and f.jp >= 2 and bool(f.locs)
    stride_hit = bool(f.strides)
    pair = size_freq.get(f.size, 0) >= 2
    A = f.self_calls >= 2 and stride_hit
    B = cmp_hit and (f.bulk >= 2 or f.cmov >= 4)
    C = cmp_hit and pair
    if f.self_calls >= 2:
        hits.append("SELF=%d" % f.self_calls)
    if stride_hit:
        hits.append("STRIDE=" + ",".join("%#x" % v for v in f.strides[:3]))
    if f.ucomisd >= 2:
        hits.append("CMP=%d/%d" % (f.ucomisd, f.jp))
    if f.bulk >= 2:
        hits.append("BULK=%d" % f.bulk)
    if f.cmov >= 4:
        hits.append("CMOV=%d" % f.cmov)
    if pair:
        hits.append("PAIR=%#x" % f.size)
    if f.atomics:
        hits.append("ATOMIC=%s" % ",".join("%#x" % a for a in f.atomics[:2]))
    if f.borrow:
        hits.append("BORROW=%d" % len(f.borrow))
    if f.slots:
        hits.append("SLOT=%d" % f.slots)
    if f.caller_loc_n == 0:
        hits.append("CALLER_LOC=0")
    g4, g4why = g4_closure(row, f, shared_locs)
    if g4:
        hits.append("G4-CLOSURE")
    # 机器候选
    std = A or B or C
    gdext = []
    if f.atomics and f.callers == 0:
        gdext.append("G1")
    if f.callers == 0 and f.slots >= 1 and 0x78 in f.strides:
        gdext.append("G1b")     # gdext 的 0x78 字节方法/属性记录，只经数据表被引用
    if f.borrow and f.callers <= 1 and f.slots >= 1:
        gdext.append("G2")      # 属性访问器：RefCell 借用 + 本函数地址在访问器表里
    if f.callers == 1 and f.caller_loc_n == 0 and f.size >= 0x180:
        gdext.append("G3")
    other = []
    if f.ret_like:
        other.append("O-ret")
    return dict(hits=hits, A=A, B=B, C=C, cmp_hit=cmp_hit, pair=pair,
                std=std, gdext=gdext, other=other, g4=g4, g4why=g4why)


def machine_class(f, hits, manual=None):
    """机器候选类别；manual 覆盖优先。"""
    if manual:
        return manual
    # 顺序 = 证据强度（预注册 §3 + 勘误 §9）：结构性的 na 证据 > G4 的"判不了"
    if hits["gdext"]:
        return "na:gdext-glue"
    if hits["std"]:
        return "na:std-mono"
    if hits.get("g4"):
        return "unknown"
    if hits["other"]:
        return "na:other"
    return "real-missing"


# ---------------------------------------------------------------------------
# 输入
# ---------------------------------------------------------------------------
def read_cov(path=os.path.join(ROOT, "fixtures", "function_coverage.tsv")):
    rows = []
    with open(path, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("#") or not ln.strip():
                continue
            p = ln.rstrip("\n").split("\t")
            if len(p) < 13 or not p[0].startswith("0x"):
                continue
            rows.append(dict(addr=p[0], name=p[1], name_src=p[2], module=p[3],
                             ours=p[4], bucket=p[6], sub=p[7]))
    return rows


def read_boundary(path=os.path.join(ROOT, "fixtures", "boundary_audit.tsv")):
    out = {}
    with open(path, encoding="utf-8") as fh:
        hdr = None
        for ln in fh:
            if ln.startswith("#"):
                continue
            p = ln.rstrip("\n").split("\t")
            if hdr is None:
                hdr = p
                continue
            if not p or not p[0].startswith("0x"):
                continue
            d = dict(zip(hdr, p))
            out[d["addr"]] = d
    return out


# ---------------------------------------------------------------------------
# 输出
# ---------------------------------------------------------------------------
def fmt_feat(f, h):
    bits = []
    bits.append("size=%#x/%d insn" % (f.size, f.n_insn))
    if h["hits"]:
        bits.append("|".join(h["hits"]))
    if f.locs:
        bits.append("LOC=" + ";".join("%s x%d" % (l, n) for l, n in f.locs[:2]))
    bits.append("callers=%d" % f.callers)
    if f.caller_loc_n >= 0:
        bits.append("caller_loc=%d" % f.caller_loc_n)
    if f.strings:
        bits.append("str=" + " / ".join(f.strings[:2]))
    return " ".join(bits)


def evidence(f, h, row, bnd):
    """人可读的「依据」：机器特征 + Location + 归属。"""
    bits = []
    if h["A"]:
        bits.append("三段自递归 %d 次 + 元素步长 %s" %
                    (f.self_calls, ",".join("%#x" % v for v in f.strides[:2])))
    if h["B"]:
        bits.append("比较器 ucomisd×%d/jp×%d + %s" %
                    (f.ucomisd, f.jp, ("整块搬运×%d" % f.bulk) if f.bulk >= 2 else ("选择链 cmov×%d" % f.cmov)))
    if h["C"]:
        bits.append("比较器 + FDE 尺寸 %#x 与本批另一条逐字节相同（成对单态化）" % f.size)
    if h["gdext"]:
        if "G1" in h["gdext"]:
            bits.append("lock cmpxchg 写入可写全局 %s + 全 .text 零 call 调用点（只经数据表）"
                        % ",".join("%#x" % a for a in f.atomics[:2]))
        if "G1b" in h["gdext"]:
            bits.append("零 call 调用点 + 地址在数据表 (%d 槽) + 0x78 字节记录追加（gdext 方法/属性表）"
                        % f.slots)
        if "G2" in h["gdext"]:
            bits.append("借用标记 [reg+0x10] 0→-1 共 %d 处（属性访问器形态）" % len(f.borrow))
        if "G3" in h["gdext"]:
            bits.append("唯一调用者不引用任何 Location（gdext ptrcall/call_func 注册回调）")
    if f.locs:
        bits.append("Location=" + ";".join("%s×%d" % (l, n) for l, n in f.locs[:2]))
    if row and row.get("name_src"):
        bits.append("名字来源=%s" % row["name_src"])
    if bnd and bnd.get("loc_main_lines"):
        bits.append("边界锚行=%s" % bnd["loc_main_lines"][:40])
    return "；".join(bits)


def mk_ctx():
    return ap.Ctx(ap.SO_DEFAULT)


def shared_locs_of(f3, feats):
    """被 ≥2 条**无名**（非 v2 真名）F3 行引用的 Location（判 G4 用）。"""
    owners = collections.defaultdict(set)
    for r in f3:
        if str(r.get("name_src") or "").startswith("v2"):
            continue
        for loc, _ in feats[r["addr"]].locs:
            owners[loc].add(r["addr"])
    return {loc for loc, s in owners.items() if len(s) >= 2}


def mod_v2_min_lines(cov, bnd):
    """每个模块里 **v2 真名**函数的最小 Location 行号（判 G4 的"父函数有没有真名"用）。"""
    out = {}
    for r in cov:
        if not str(r.get("name_src") or "").startswith("v2"):
            continue
        b = bnd.get(r["addr"])
        if not b or not b.get("loc_main_lines"):
            continue
        mod = os.path.basename(str(r.get("module") or ""))
        for x in str(b["loc_main_lines"]).split(","):
            x = x.strip()
            if not x.isdigit():
                continue
            n = int(x)
            if mod not in out or n < out[mod]:
                out[mod] = n
    return out


def build(verbose=True):
    ctx = mk_ctx()
    index = ctx.index()
    cov = read_cov()
    bnd = read_boundary()
    f3 = [r for r in cov if r["bucket"] == "F3"]
    f2 = [r for r in cov if r["bucket"] == "F2"]
    if verbose:
        print("F3=%d F2=%d（分母 %d）" % (len(f3), len(f2), len(cov)))
    feats = {}
    for r in cov:
        feats[r["addr"]] = features(ctx, int(r["addr"], 16), index)
    size_freq = collections.Counter(feats[r["addr"]].size for r in cov)
    return (ctx, cov, bnd, f3, f2, feats, size_freq,
            shared_locs_of(f3, feats), mod_v2_min_lines(cov, bnd))


def main(argv=None):
    ap_ = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap_.add_argument("--features", nargs="+", metavar="VADDR")
    ap_.add_argument("--control", action="store_true")
    ap_.add_argument("--stats", action="store_true")
    ap_.add_argument("--json", metavar="PATH")
    args = ap_.parse_args(argv)

    if args.features:
        ctx = mk_ctx()
        index = ctx.index()
        for a in args.features:
            va = int(a, 16)
            f = features(ctx, va, index)
            print("== %s  FDE %#x..%#x size %#x  insn %d" % (a, f.start, f.end, f.size, f.n_insn))
            print("   first: " + " ; ".join("%s %s" % (m, o) for m, o in f.first))
            print("   self_calls=%d strides=%s ucomisd=%d jp=%d bulk=%d cmov=%d" %
                  (f.self_calls, [hex(v) for v in f.strides], f.ucomisd, f.jp, f.bulk, f.cmov))
            print("   atomics=%s borrow=%d slots=%d callers=%d lea_refs=%d caller_loc=%d"
                  % ([hex(x) for x in f.atomics], len(f.borrow), f.slots, f.callers,
                     f.lea_refs, f.caller_loc_n))
            print("   locs=%s" % (["%s x%d" % (l, n) for l, n in f.locs[:4]]))
            print("   strings=%s" % f.strings)
            print("   callees=%s" % (["%#x" % c for c in f.callees[:10]]))
        return 0

    ctx, cov, bnd, f3, f2, feats, size_freq, shared_locs, _mv2 = build()
    rows = []
    for r in f3:
        f = feats[r["addr"]]
        h = rule_hits(f, size_freq, r, shared_locs)
        rows.append((r, f, h))

    if args.control:
        return control(rows, feats, size_freq, shared_locs)

    # ---- 人工复核产物（fixtures/f3_triage.tsv）的自动部分 -------------------
    print("\n=== 机器候选分布（**人工裁决会覆盖它**）")
    c = collections.Counter(machine_class(f, h) for _, f, h in rows)
    for k, v in c.most_common():
        print("  %-16s %3d" % (k, v))
    print("\n=== 全量明细（机器特征，供人工复核）")
    for r, f, h in rows:
        print("%s\t%-58s\t%#6x\t%s" % (r["addr"], (r["name"] or FAKE)[:58], f.size, fmt_feat(f, h)))
    return 0


def control(rows, feats, size_freq, shared_locs=None):
    """正/负对照：12 std / 19 gdext / 2 真业务 / 3 附加负对照（见预注册 §4）。"""
    print("=" * 100)
    print("正/负对照（判据见 fixtures/f3_triage_plan.txt §4）")
    print("=" * 100)
    ok = True

    # 机判自校验：12 条 std 的 FDE 尺寸必须与台账逐对相同
    print("\n-- 机判自校验：12 条 std 的 FDE 尺寸（台账期望两两相同：%s）"
          % ", ".join("%#x" % s for s in CTRL_STD_SIZES))
    sizes = []
    for a in CTRL_STD:
        f = feats.get(a)
        if f is None:
            ctx = mk_ctx()
            f = features(ctx, int(a, 16), ctx.index())
        sizes.append((a, f.size))
    got = collections.Counter(s for _, s in sizes)
    print("   实测成对: " + "  ".join("%#x×%d" % (s, n) for s, n in sorted(got.items())))
    expect = collections.Counter({s: 2 for s in CTRL_STD_SIZES})
    if got == expect:
        print("   ✓ 与台账逐对相同")
    else:
        print("   ✗ 与台账不符（期望 %s）" % dict(expect))
        ok = False

    def judge(a, want, note=""):
        nonlocal ok
        f = feats.get(a)
        if f is None:
            ctx = ap.Ctx(None)
            f = features(ctx, int(a, 16), ctx.index())
        h = rule_hits(f, size_freq, {"addr": a, "name_src": "", "module": ""}, shared_locs)
        got = machine_class(f, h)
        good = (got == want)
        if not good:
            ok = False
        print("  %-4s %-9s want=%-14s got=%-14s %s %s" %
              ("✓" if good else "✗", a, want, got, fmt_feat(f, h)[:80], note))
        return good

    print("\n-- A 组：12 条 std 单态化 ⇒ 应判 na:std-mono")
    for a in CTRL_STD:
        judge(a, "na:std-mono")

    print("\n-- B 组：19 条 gdext 宏产物 ⇒ 应判 na:gdext-glue（其中 3 条源码闭包按 G4 判 unknown）")
    for a in CTRL_GDEXT:
        want = "unknown" if a in CTRL_GDEXT_CLOSURE else "na:gdext-glue"
        judge(a, want, "(G4 源码闭包)" if a in CTRL_GDEXT_CLOSURE else "")

    print("\n-- C 组：2 条 T71 落码的真业务函数 ⇒ 不许判 na:*")
    for a in CTRL_REAL:
        f = feats.get(a) or features(mk_ctx(), int(a, 16), (None, None, None))
        h = rule_hits(f, size_freq, None, shared_locs)
        got = machine_class(f, h)
        good = not got.startswith("na:")
        ok = ok and good
        print("  %-4s %-9s want=real（非 na）  got=%-14s %s" %
              ("✓" if good else "✗", a, got, fmt_feat(f, h)[:70]))

    print("\n-- D 组：3 条已知 F1 真业务 ⇒ 不许判 na:*（防「把一切判 na」）")
    for a in CTRL_REAL_EXTRA:
        f = feats.get(a) or features(mk_ctx(), int(a, 16), (None, None, None))
        h = rule_hits(f, size_freq, None, shared_locs)
        got = machine_class(f, h)
        good = not got.startswith("na:")
        ok = ok and good
        print("  %-4s %-9s want=real（非 na）  got=%-14s %s" %
              ("✓" if good else "✗", a, got, fmt_feat(f, h)[:70]))

    print("\n对照结论：%s" % ("全部通过 ⇒ 可以进全量" if ok else "有失败 ⇒ 必须先修工具/规则（预注册 §4）"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
