#!/usr/bin/env python3
"""把「谁调用了某个函数」逐个归到 `src/*.rs` 模块——一次扫描，不重复起进程。

为什么需要它：`loc_xrefs.py func <起点> --quiet` 每次都要重扫整个 `.so`（~16 s），
十几个调用点就是几分钟，而且**手工敲地址容易把中间地址当起点**（`func` 只认函数起点）。
本脚本只建**一个** `LocXrefs` 实例，然后：

1. 自己用 capstone 扫所有可执行段，找 `call rel32` 目标 == 指定地址的调用点；
2. 用 Ghidra 的 `funcstarts_ghidra_va.pkl`（虚地址空间）把调用点归到**函数起点**；
3. 用 `LocXrefs.locs_of_func()` 取该函数引用的全部 `panic::Location`
   （`{file, line, col}` 的 `file` 字段），按文件计数，**众数**即模块；
4. 同时打出 `fixtures/module_funcs.tsv` 的既有归属做交叉核对——两者不一致会明着报。

用法（仓库约定用带 capstone 的 venv）：

    ../.venv/bin/python tools/los_callers.py                  # 默认查 0x467530
    ../.venv/bin/python tools/los_callers.py 0x467530 0x3f15b0
    ../.venv/bin/python tools/los_callers.py --callees 0x467530
"""

import argparse
import bisect
import collections
import os
import pickle
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)
sys.path.insert(0, HERE)

from loc_xrefs import DEFAULT_SO, LocXrefs, fmt, gh  # noqa: E402

MODMAP_TSV = os.path.join(SUB, "fixtures", "module_funcs.tsv")
NAMES_TSV = os.path.join(SUB, "fixtures", "name_to_addr.tsv")


def starts_pkl():
    """函数起点表在上游分析仓库（`../analysis/`），克隆到别处时要能找得到。"""
    rel = os.path.join("analysis", "funcstarts_ghidra_va.pkl")
    cands = [os.environ.get("GW2_FUNCSTARTS")]
    base = ROOT
    for _ in range(4):                     # 克隆可能比 gw2-rebuild/ 深几层，逐级向上找
        cands.append(os.path.join(base, rel))
        base = os.path.dirname(base)
    for c in cands:
        if c and os.path.exists(c):
            return c
    raise SystemExit("找不到 funcstarts_ghidra_va.pkl（用 GW2_FUNCSTARTS 指定）")


def load_starts():
    """Ghidra 函数起点（**已经是 ELF 虚地址空间**）。pkl 里可能是 int 也可能是 hex 串。"""
    raw = pickle.load(open(starts_pkl(), "rb"))
    return sorted(int(x, 16) if isinstance(x, str) else int(x) for x in raw)


def load_modmap():
    out = {}
    if not os.path.exists(MODMAP_TSV):
        return out
    for line in open(MODMAP_TSV, encoding="utf-8").read().splitlines()[1:]:
        if not line.strip():
            continue
        mod, addr = line.split("\t")
        out[int(addr, 16)] = mod
    return out


def load_names():
    """`name_to_addr.tsv` → {虚地址: "模块::名字"}。同名多址时优先取「唯一锚」那一行。"""
    out = {}
    if not os.path.exists(NAMES_TSV):
        return out
    for line in open(NAMES_TSV, encoding="utf-8").read().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        cols = line.split("\t")
        if len(cols) < 7 or not cols[3].startswith("0x"):
            continue
        addr = int(cols[3], 16)
        name = "%s::%s" % (cols[1], cols[2])
        if addr not in out or cols[6] == "唯一锚":
            out[addr] = name
    return out


def scan_call_sites(ix, target):
    """`.text` 等可执行段里所有 `call rel32`（立即数目标）== target 的指令地址。"""
    md = ix.cs.Cs(ix.cs.CS_ARCH_X86, ix.cs.CS_MODE_64)
    md.detail = True
    sites = []
    for lo, hi in ix.exec_ranges:
        code = ix.read(lo, hi - lo)
        for ins in md.disasm(code, lo):
            if ins.mnemonic != "call" or not ins.operands:
                continue
            op = ins.operands[0]
            if op.type == ix.cs.x86.X86_OP_IMM and op.imm == target:
                sites.append(ins.address)
    return sorted(sites)


def scan_callees(ix, func):
    """该函数体内所有 `call rel32` 的立即数目标（去重、排序）。"""
    md = ix.cs.Cs(ix.cs.CS_ARCH_X86, ix.cs.CS_MODE_64)
    md.detail = True
    lo, hi = func, ix.func_end(func)
    out = collections.Counter()
    for ins in md.disasm(ix.read(lo, hi - lo), lo):
        if ins.mnemonic != "call" or not ins.operands:
            continue
        op = ins.operands[0]
        if op.type == ix.cs.x86.X86_OP_IMM:
            out[op.imm] += 1
    return out


def attribute(ix, func):
    """-> (众数模块, 文件计数, 代表 file:line, Location 条数)。"""
    locs = ix.locs_of_func(func)
    if not locs:
        return None, collections.Counter(), None, 0
    files = collections.Counter()
    for (f, line, col) in locs:
        files[f] += 1
    mode = files.most_common(1)[0][0]
    rep = min(((l, c) for (f, l, c) in locs if f == mode))
    return mode, files, "%s:%d" % (mode, rep[0]), len(locs)


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("targets", nargs="*", type=lambda s: int(s, 0),
                    help="被查的函数虚地址（默认 0x467530）")
    ap.add_argument("--so", default=DEFAULT_SO)
    ap.add_argument("--callees", action="store_true", help="反向：列该函数的被调者")
    ap.add_argument("--quiet", action="store_true")
    o = ap.parse_args(argv[1:])
    if not o.targets:
        o.targets = [0x467530]

    ix = LocXrefs(o.so, verbose=not o.quiet)
    starts = load_starts()
    modmap = load_modmap()
    names = load_names()

    for target in o.targets:
        if target not in starts:
            print("# %s 不是 Ghidra 函数起点——拒绝归因（会归到错的函数）" % fmt(target))
            continue
        print("# target %s（Ghidra %s）%d B" %
              (fmt(target), gh(target), ix.func_end(target) - target))

        if o.callees:
            print("callee\tx\tghidra\tsize\tname\tmodule\trepr")
            for tgt, n in sorted(scan_callees(ix, target).items()):
                is_start = tgt in starts
                mod, files, rep, nl = attribute(ix, tgt) if is_start else (
                    None, collections.Counter(), None, 0)
                size = ix.func_end(tgt) - tgt if is_start else 0
                print("%s\tx%d\t%s\t%d\t%s\t%s\t%s" %
                      (fmt(tgt), n, gh(tgt), size, names.get(tgt, "-"),
                       mod or "-", rep or "-"))
            continue

        sites = scan_call_sites(ix, target)
        print("# %d 个直接调用点" % len(sites))
        print("callsite\tfunc_start\tghidra\tsize\tsource\tname\tmodule\tmodule_funcs.tsv\tmode_vote\trepr\tlocs")
        for s in sites:
            fn = ix.func_of(s)  # .eh_frame 口径，作交叉核对
            gi = bisect.bisect_right(starts, s) - 1
            gfn = starts[gi] if gi >= 0 else None
            if gfn is None:
                print("%s\t?\t?\t?\t?\t?\t?\t?\t?\t?" % fmt(s))
                continue
            mod, files, rep, nl = attribute(ix, gfn)
            vote = " ".join("%s=%d" % (os.path.basename(f), n)
                            for f, n in files.most_common())
            top = files.most_common(1)[0][1] if files else 0   # 众数文件拿到几票
            print("%s\t%s\t%s\t%d\t%s\t%s\t%s\t%s\t%d/%d\t%s\t%d" %
                  (fmt(s), fmt(gfn), gh(gfn), ix.func_end(gfn) - gfn,
                   ix.func_source(gfn), names.get(gfn, "-"), mod or "-",
                   modmap.get(gfn, "-"), top, nl, rep or "-", nl))
            if not o.quiet:
                print("#     vote: %s" % (vote or "-"))
            if fn != gfn:
                print("#     ⚠ .eh_frame 归属 = %s ≠ Ghidra 口径 %s" % (fmt(fn), fmt(gfn)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
