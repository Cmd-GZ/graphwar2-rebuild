#!/usr/bin/env python3
"""measure.py —— 给 docs 里要写的数字找一个**定义明确**的口径。

口径（写进文档时逐条对应）：
  U_B = 归一化指令形态在 08-27 里**完全找不到**的 08-28 函数个数（集合成员，不管重复计数）
  U_A = 反过来
  这两个数才是"结构真的不一样"的函数数；它们**不是**"这次补丁改了几个函数"，
  因为 08-19→08-27 这种普通版本推进也有同一量级（见 docs 里的对照实验）。
"""
import collections
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import version_diff as vd  # noqa: E402

from capstone import Cs, CS_ARCH_X86, CS_MODE_64  # noqa: E402

REPO = os.path.dirname(HERE)
# 默认值相对**仓库根**解析（不依赖当前目录）：<repo>/../.reference/steam
REF = os.environ.get("GW2_REF") or os.path.join(REPO, os.pardir, ".reference", "steam")


def load(v):
    p = os.path.join(REF, v, "libgraphwarrust.so")
    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    return vd.build_functions(cs, p, vd.fde_ranges(p))


def main():
    old = os.environ.get("GW2_OLD", "2026-08-27")
    new = os.environ.get("GW2_NEW", "2026-08-28")
    A = load(old)
    B = load(new)
    print("对照：%s -> %s" % (old, new))
    sa = {x["shape"] for x in A}
    sb = {x["shape"] for x in B}
    ub = [x for x in B if x["shape"] not in sa]
    ua = [x for x in A if x["shape"] not in sb]
    print("函数数 08-27=%d  08-28=%d  net=%+d" % (len(A), len(B), len(B) - len(A)))
    print("U_B（08-28 独有形态）=%d   U_A（08-27 独有形态）=%d" % (len(ub), len(ua)))
    print("U_B 大小分布：")
    hist = collections.Counter()
    for x in ub:
        hist[min(x["size"] // 100 * 100, 2000)] += 1
    for k in sorted(hist):
        print("   size %5d-%5d : %d" % (k, k + 99, hist[k]))
    print("U_B 里最大的 15 个：")
    for x in sorted(ub, key=lambda t: -t["size"])[:15]:
        print("   0x%06x size=%d" % (x["va"], x["size"]))
    # module attribution
    mp = os.path.join(HERE, "..", "fixtures", "module_funcs.tsv")
    attributed = {}
    if os.path.exists(mp):
        m = {}
        for line in open(mp):
            line = line.strip()
            if not line or line.startswith("module\t"):
                continue
            mod, addr = line.split("\t")
            m[int(addr, 16)] = mod
        for x in ub:
            if x["va"] in m:
                attributed.setdefault(m[x["va"]], []).append(x["va"])
        print("U_B 中能指到模块的：%d 个函数，涉及 %d 个模块" % (
            sum(len(v) for v in attributed.values()), len(attributed)))
        for mod in sorted(attributed):
            print("   %-46s %d" % (mod, len(attributed[mod])))
    for name, va in (("_exit 调用者", 0x3B5290), ("isatty 调用者1", 0x8624D0),
                     ("isatty 调用者2", 0x8624F0), ("isatty 调用者3", 0x862690),
                     ("isatty 调用者4", 0x862730)):
        x = next((y for y in B if y["va"] == va), None)
        if x is None:
            print("%-14s 0x%06x 不在 FDE 表" % (name, va))
            continue
        print("%-14s 0x%06x size=%-5d 形态独有=%s"
              % (name, va, x["size"], x["shape"] not in sa))


if __name__ == "__main__":
    main()
