#!/usr/bin/env python3
"""func_diff.py — 给 08-28 的某个函数找 08-27 的最佳对应函数，并打印归一化指令级差异。

用法：
    ../.venv/bin/python func_diff.py <old.so> <old_fde> <new.so> <new_fde> 0x3c2530 [0x4ed670 ...]
"""
import difflib
import importlib.util
import os
import sys

from capstone import Cs, CS_ARCH_X86, CS_MODE_64

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("vd", os.path.join(HERE, "version_diff.py"))
vd = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vd)


def shape_lines(cs, path, ranges):
    d, secs, loads, _ = vd.elf_sections(path)
    out = []
    for va, end in ranges:
        off, eoff = vd.va2off(loads, va), vd.va2off(loads, end - 1)
        if off is None or eoff is None:
            continue
        body = d[off:eoff + 1]
        shape, smalls, n = vd.normalize(cs, body, va)
        out.append(dict(va=va, size=end - va, lines=shape.split("\n"), body=body, smalls=smalls))
    return out


def main(old, old_fde, new, new_fde, *vas):
    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    A = shape_lines(cs, old, vd.fde_ranges(old_fde))
    B = shape_lines(cs, new, vd.fde_ranges(new_fde))
    idxB = {x["va"]: x for x in B}
    idxA = {x["va"]: x for x in A}
    for vs in vas:
        va = int(vs, 16)
        y = idxB.get(va)
        if y is None:
            print("0x%06x 不在 new 的 FDE 表里" % va)
            continue
        best = None
        for x in A:
            if x["size"] != y["size"]:
                continue
            r = difflib.SequenceMatcher(None, x["lines"], y["lines"]).ratio()
            if best is None or r > best[0]:
                best = (r, x)
        if best is None:
            best = (0.0, None)
            for x in A:
                r = difflib.SequenceMatcher(None, x["lines"], y["lines"]).ratio()
                if r > best[0]:
                    best = (r, x)
        r, x = best
        print("=" * 78)
        print("new va=0x%06x size=%d insns=%d" % (va, y["size"], len(y["lines"])))
        if x is None:
            print("  old 侧无候选")
            continue
        print("best old va=0x%06x size=%d insns=%d  shape-ratio=%.4f"
              % (x["va"], x["size"], len(x["lines"]), r))
        d = list(difflib.unified_diff(x["lines"], y["lines"], "old", "new", n=3, lineterm=""))
        print("  normalized-instruction diff: %d lines" % len(d))
        for l in d[:120]:
            print("   " + l)
        if x["size"] == y["size"]:
            diffs = [i for i in range(len(x["body"])) if x["body"][i] != y["body"][i]]
            print("  raw-byte differences: %d bytes at %s" % (len(diffs), diffs[:40]))
        if x["smalls"] != y["smalls"]:
            print("  small-immediate change: old=%s" % (x["smalls"][:20],))
            print("                          new=%s" % (y["smalls"][:20],))


if __name__ == "__main__":
    main(*sys.argv[1:])
