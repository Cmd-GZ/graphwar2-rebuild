#!/usr/bin/env python3
"""把模块归属从"有 panic 锚的函数"扩散到它们的调用邻居。

种子来自 `module_map.py`（观测：函数里有 `src/graphwar_x.rs` 的 panic 位置）。
扩散规则刻意保守，宁可少标也不乱标：

  * 只走**直接调用**边（capstone 的 `call 0x...` 立即数目标）；
  * 一个未标注函数要被标注，必须至少 2 条已标注邻居边，且其中同一模块占
    >= 2/3 —— 只有单一邻居的函数保持未标注；
  * 种子本身永远是"观测"，扩散来的一律标"推断"。

输出 `fixtures/module_map_propagated.tsv`：
    module  observed  inferred  total

用法：
    ../.venv/bin/python tools/propagate.py
"""

import bisect
import collections
import os
import pickle
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)

sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, "analysis"))

import module_map                      # noqa: E402  复用字符串/段定位逻辑
from elf import Elf                    # noqa: E402

ROUNDS = 3
MIN_EDGES = 2
MAJORITY = 2.0 / 3.0


def load_funcs():
    starts = sorted(pickle.load(open(os.path.join(ROOT, "analysis", "funcstarts.pkl"), "rb")))
    return starts, set(starts)


def owner_of(starts, addr):
    i = bisect.bisect_right(starts, addr) - 1
    return starts[i] if i >= 0 else None


def seeds_from_panics(starts, xrefs, relocs):
    """返回 {module: set(func_start)}，只含有真实 panic 锚的函数。"""
    E = Elf(module_map.SO)
    secs = {s["name"]: s for s in E.sections if s["size"] and s["addr"]}

    slots_of = {}
    for slot, entries in relocs.items():
        for (typ, addend) in entries:
            if typ == 8:
                slots_of.setdefault(addend, []).append(slot)

    seeds = collections.defaultdict(set)
    for m in module_map.PATTERN.finditer(E.data):
        loc = module_map.section_of(secs, m.start())
        if loc is None:
            continue
        addr = loc[0]
        module = m.group(1).decode()
        if not module.startswith("graphwar_"):
            continue                    # 只收游戏自身模块
        refs = list(xrefs.get(addr, []))
        for slot in slots_of.get(addr, ()):
            refs.extend(xrefs.get(slot, []))
        for insn_addr, _k, _mn, _op in refs:
            f = owner_of(starts, insn_addr)
            if f is not None:
                seeds[module].add(f)
    return seeds


def build_call_edges(starts, funcset):
    """callee -> set(callers)，只保留落在已知函数边界内的直接调用。"""
    insns = pickle.load(open(os.path.join(ROOT, "analysis", "insns.pkl"), "rb"))
    callees_of = collections.defaultdict(set)   # caller -> {callee}
    total = 0
    for addr, (mnem, op_str, _size, _bytes) in insns.items():
        if not mnem.startswith("call"):
            continue
        if not op_str.startswith("0x"):
            continue
        try:
            tgt = int(op_str, 16)
        except ValueError:
            continue
        # 注意：线性反汇编与 .eh_frame 边界存在字节级漂移，调用目标不会精确等于
        # 某个函数起点，所以不能用"目标在集合里"来过滤，必须按边界归属。
        callee = owner_of(starts, tgt)
        caller = owner_of(starts, addr)
        if callee is None or caller is None or caller == callee:
            continue
        callees_of[caller].add(callee)
        total += 1
    return callees_of, total


def main():
    starts, funcset = load_funcs()
    xrefs = pickle.load(open(os.path.join(ROOT, "analysis", "xrefs.pkl"), "rb"))
    relocs = pickle.load(open(os.path.join(ROOT, "analysis", "relocs.pkl"), "rb"))

    seeds = seeds_from_panics(starts, xrefs, relocs)
    label = {}
    origin = {}
    for module, funcs in seeds.items():
        for f in funcs:
            if f not in label:          # 一个函数可能被多个模块引用，先到先得
                label[f] = module
                origin[f] = "observed"

    callees_of, n_edges = build_call_edges(starts, funcset)
    callers_of = collections.defaultdict(set)
    for caller, callees in callees_of.items():
        for c in callees:
            callers_of[c].add(caller)

    print("种子函数 %d 个，调用边 %d 条" % (len(label), n_edges))

    for rnd in range(1, ROUNDS + 1):
        newly = 0
        for func in callees_of.keys() | callers_of.keys():
            if func in label:
                continue
            nbrs = [(n, label[n]) for n in (callees_of.get(func, set()) | callers_of.get(func, set()))
                    if n in label]
            if len(nbrs) < MIN_EDGES:
                continue
            counter = collections.Counter(m for _n, m in nbrs)
            module, votes = counter.most_common(1)[0]
            if votes / len(nbrs) >= MAJORITY:
                label[func] = module
                origin[func] = "inferred"
                newly += 1
        print("  第 %d 轮：新增 %d（累计 %d）" % (rnd, newly, len(label)))
        if newly == 0:
            break

    observed = sum(1 for f in label if origin[f] == "observed")
    inferred = len(label) - observed
    print("\n覆盖 %d / %d 个函数（%.1f%%），其中观测 %d、推断 %d"
          % (len(label), len(starts), 100.0 * len(label) / len(starts), observed, inferred))

    rows = []
    mods = sorted({m for m in label.values()})
    for module in mods:
        funcs = [f for f, m in label.items() if m == module]
        obs = sum(1 for f in funcs if origin[f] == "observed")
        rows.append((module, obs, len(funcs) - obs, len(funcs)))
    rows.sort(key=lambda r: -r[3])

    out = os.path.join(SUB, "fixtures", "module_map_propagated.tsv")
    with open(out, "w", encoding="utf-8") as fh:
        fh.write("module\tobserved\tinferred\ttotal\n")
        for r in rows:
            fh.write("%s\t%d\t%d\t%d\n" % r)

    print("\n%-46s %6s %6s %6s" % ("module", "obs", "infer", "total"))
    for r in rows[:20]:
        print("%-46s %6d %6d %6d" % r)


if __name__ == "__main__":
    main()
