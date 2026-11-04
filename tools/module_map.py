#!/usr/bin/env python3
"""把 13430 个函数按 Rust 源文件归堆。

原理：Rust 在 panic 路径里保留 `src/<file>.rs:<line>` 字符串。找到这些字符串在
二进制里的地址，取其引用点（capstone 建的 rip-relative / imm 交叉引用），
再用 `.eh_frame` 恢复的函数边界（`funcstarts.pkl`）判定引用点属于哪个函数。

输出：
  fixtures/module_map.tsv   每个模块的函数数、引用数、地址范围
  （stdout）按函数数排序的摘要

依赖分析仓库的产物：analysis/insns.pkl、xrefs.pkl、funcstarts.pkl
（由 analysis/build_index.py 生成，约 2.5 分钟）。

用法：
    ../.venv/bin/python tools/module_map.py
"""

import bisect
import os
import pickle
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)

sys.path.insert(0, os.path.join(ROOT, "analysis"))
from elf import Elf                     # noqa: E402

SO = os.path.join(ROOT, "libgraphwarrust.so")
PATTERN = re.compile(rb"src/((?:graphwar_)?[a-z0-9_]+\.rs)(?::(\d+))?")


def section_of(secs, pos):
    """文件偏移 -> 虚拟地址；返回 (addr, section_name) 或 None。"""
    for name, s in secs.items():
        if s["offset"] <= pos < s["offset"] + s["size"]:
            return s["addr"] + (pos - s["offset"]), name
    return None


def main():
    E = Elf(SO)
    funcstarts = sorted(pickle.load(open(os.path.join(ROOT, "analysis", "funcstarts_ghidra_va.pkl"), "rb")))
    xrefs = pickle.load(open(os.path.join(ROOT, "analysis", "xrefs.pkl"), "rb"))
    relocs = pickle.load(open(os.path.join(ROOT, "analysis", "relocs.pkl"), "rb"))

    # Rust 的 panic Location 是 {ptr, len} 结构体：字符串地址存在 .data.rel.ro 的
    # 重定位 addend 里，指令引用的是结构体本身。所以先把 目标地址 -> 槽位 反过来。
    slots_of = {}
    for slot, entries in relocs.items():
        for (typ, addend) in entries:
            if typ == 8:                # R_X86_64_RELATIVE
                slots_of.setdefault(addend, []).append(slot)

    # .text/.rodata 等 alloc 段
    secs = {}
    for s in E.sections:              # Elf 的这个属性是 list[dict]，不是方法
        if s["size"] and s["addr"]:
            secs[s["name"]] = s

    blob = E.data
    occurrences = []                    # (addr, module, line)
    for m in PATTERN.finditer(blob):
        loc = section_of(secs, m.start())
        if loc is None:
            continue
        addr, _ = loc
        module = m.group(1).decode()
        line = int(m.group(2)) if m.group(2) else None
        occurrences.append((addr, module, line))

    # 字符串地址 -> 引用它的函数
    per_module = {}
    unresolved = 0
    for addr, module, line in occurrences:
        # 1) 指令直接引用字符串；2) 通过 .data.rel.ro 里的 Location 结构体间接引用
        refs = list(xrefs.get(addr, []))
        for slot in slots_of.get(addr, ()):
            refs.extend(xrefs.get(slot, []))
        if not refs:
            for probe in range(addr, addr + 24, 4):
                if probe in xrefs:
                    refs = xrefs[probe]
                    break
        if not refs:
            unresolved += 1
            continue
        rec = per_module.setdefault(module, {"funcs": set(), "refs": 0, "lines": set()})
        if line:
            rec["lines"].add(line)
        for insn_addr, _kind, _mnem, _ops in refs:
            i = bisect.bisect_right(funcstarts, insn_addr) - 1
            if i >= 0:
                rec["funcs"].add(funcstarts[i])
                rec["refs"] += 1

    rows = []
    for module, rec in per_module.items():
        funcs = sorted(rec["funcs"])
        rows.append((module, len(funcs), rec["refs"],
                     funcs[0] if funcs else 0,
                     funcs[-1] if funcs else 0,
                     max(rec["lines"]) if rec["lines"] else 0))
    rows.sort(key=lambda r: (-r[1], r[0]))

    # 每个模块的函数地址清单（供逐模块反编译使用）
    funcs_out = os.path.join(SUB, "fixtures", "module_funcs.tsv")
    with open(funcs_out, "w", encoding="utf-8") as fh:
        fh.write("module\tfunc_addr\n")
        for module in sorted(per_module):
            if not module.startswith("graphwar_"):
                continue
            for f in sorted(per_module[module]["funcs"]):
                fh.write("%s\t0x%x\n" % (module, f))

    out = os.path.join(SUB, "fixtures", "module_map.tsv")
    with open(out, "w", encoding="utf-8") as fh:
        fh.write("module\tfunctions\trefs\tfirst_func\tlast_func\tmax_line\n")
        for r in rows:
            fh.write("%s\t%d\t%d\t0x%x\t0x%x\t%d\n" % r)

    total_funcs = len(set().union(*[per_module[m]["funcs"] for m in per_module])) if per_module else 0
    print("字符串出现 %d 次，未能定位引用 %d 次" % (len(occurrences), unresolved))
    print("覆盖模块 %d 个，涉及函数 %d / 13430" % (len(rows), total_funcs))
    print()
    print("%-46s %6s %6s  %s" % ("module", "funcs", "refs", "max_line"))
    for r in rows[:25]:
        print("%-46s %6d %6d  %d" % (r[0], r[1], r[2], r[5]))


if __name__ == "__main__":
    main()
