#!/usr/bin/env python3
"""模块级覆盖审计：把「一共 54 个 Rust 模块、每个模块有多少函数」跟
「我们到底在文档里引用过其中多少个函数」对起来。

回答的是那句反复被问的话：「所以到底漏了什么」。

输入（都已存在，不重新反汇编）：
    fixtures/module_map_propagated.tsv   模块 → 函数总数（observed + inferred）
    fixtures/module_funcs.tsv            模块 → 具体函数地址（observed）
    ../analysis/funcstarts_ghidra_va.pkl  全二进制 13263 个函数起点（虚地址）
    docs/**/*.md                         我们在文档里引用过的地址
    src/*.rs                            重建实现里提到过的模块名

输出（stdout + fixtures/module_audit.tsv）：
    module / total / observed / cited / cited_ratio / in_src / verdict

## 这个指标能说明什么、不能说明什么

`cited` 的口径是**函数起始地址在文档里被写下来过**。它测的是「我有没有把地址
落在纸面上」这个**记录习惯**，不是「我懂不懂」。

两个方向都有噪声，而且都实测过：

* **假阴性**（懂但没写地址）：`graphwar_function_input.rs` 的 13 个映射函数一个
  都没被引用，可解析器 `0x4ed670` 是**逐位验证**过的——它根本不在
  `module_funcs.tsv` 里（该表只收「有 panic 字符串被引用」的函数）。
  `graphwar_plane_pos_manager.rs` 同理：`px = 20x + 500` 是从常量反查得到的，
  文档里没记任何 `0x3c62xx`。
* **假阳性**（写了地址但没读懂）：只被 xref 表连带提到的地址也会计数。

⇒ 所以本表**只能**用来找「大面积、连地址都没出现过」的模块，**不能**反过来当
完成度打分，也不能把 `NO-CITE` 读成「没看过」。要下结论必须逐条人工复核。
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)          # gw2-rebuild/
ROOT = os.path.dirname(SUB)          # 上游分析仓库（只读用它的函数表）

# 十六进制地址；排除明显不是代码地址的长串（md5、时间戳等）
ADDR_RE = re.compile(r"0x([0-9a-fA-F]{6})\b")


def load_totals():
    path = os.path.join(SUB, "fixtures", "module_map_propagated.tsv")
    out = {}
    with open(path, encoding="utf-8") as f:
        next(f)
        for line in f:
            mod, observed, inferred, total = line.rstrip("\n").split("\t")
            out[mod] = (int(observed), int(inferred), int(total))
    return out


def load_funcs():
    path = os.path.join(SUB, "fixtures", "module_funcs.tsv")
    out = {}
    with open(path, encoding="utf-8") as f:
        next(f)
        for line in f:
            mod, addr = line.rstrip("\n").split("\t")
            out.setdefault(mod, set()).add(int(addr, 16))
    return out


def collect_doc_addrs():
    """文档里出现过的全部地址（虚地址口径）。"""
    seen = set()
    for root, _dirs, files in os.walk(os.path.join(SUB, "docs")):
        for name in files:
            if not name.endswith(".md"):
                continue
            with open(os.path.join(root, name), encoding="utf-8", errors="replace") as f:
                for m in ADDR_RE.finditer(f.read()):
                    seen.add(int(m.group(1), 16))
    return seen


def collect_src_modules():
    """src/*.rs 里用注释引用过的模块名 —— 说明这块被实现时参考过。"""
    names = set()
    mod_re = re.compile(r"graphwar_[a-z0-9_]+\.rs")
    srcdir = os.path.join(SUB, "src")
    for name in os.listdir(srcdir):
        if not name.endswith(".rs"):
            continue
        with open(os.path.join(srcdir, name), encoding="utf-8", errors="replace") as f:
            names.update(mod_re.findall(f.read()))
    return names


def global_stats(doc_addrs):
    """整个 .so 的 13263 个函数里，文档写下过地址的有多少。

    这个数是**下界**，而且是全二进制口径：分母里混着 Rust std、hashbrown、
    Godot 绑定等与游戏逻辑无关的函数，所以它注定很小，不能当完成度百分比读。
    """
    path = os.path.join(ROOT, "analysis", "funcstarts_ghidra_va.pkl")
    if not os.path.exists(path):
        return None
    import pickle
    starts = set(pickle.load(open(path, "rb")))
    # 文档里 Ghidra 口径（+0x100000）和虚地址口径混用，两种都算命中
    hit = starts & (doc_addrs | {a - 0x100000 for a in doc_addrs})
    return len(starts), len(hit), len(doc_addrs)


def main():
    totals = load_totals()
    funcs = load_funcs()
    doc_addrs = collect_doc_addrs()
    src_mods = collect_src_modules()

    rows = []
    for mod, (observed, inferred, total) in totals.items():
        addrs = funcs.get(mod, set())
        # 文档可能写成虚拟地址，也可能写成 Ghidra 地址（+0x100000），两种都算
        cited = sum(1 for a in addrs if a in doc_addrs or (a + 0x100000) in doc_addrs)
        ratio = cited / observed if observed else 0.0
        in_src = mod in src_mods
        if total == 0:
            verdict = "n/a"
        elif cited == 0:
            verdict = "NO-CITE"
        elif ratio < 0.15:
            verdict = "thin"
        elif ratio < 0.4:
            verdict = "partial"
        else:
            verdict = "core"
        rows.append((mod, total, observed, inferred, cited, ratio, in_src, verdict))

    rows.sort(key=lambda r: (r[7] != "NO-CITE", -r[1]))

    out = os.path.join(SUB, "fixtures", "module_audit.tsv")
    with open(out, "w", encoding="utf-8") as f:
        f.write("module\ttotal\tobserved\tinferred\tcited\tcited_ratio\tin_src\tverdict\n")
        for mod, total, observed, inferred, cited, ratio, in_src, verdict in rows:
            f.write(f"{mod}\t{total}\t{observed}\t{inferred}\t{cited}\t"
                    f"{ratio:.3f}\t{int(in_src)}\t{verdict}\n")

    print(f"{'module':40s} {'tot':>4s} {'obs':>4s} {'inf':>4s} "
          f"{'cit':>4s} {'ratio':>6s} {'src':>3s}  verdict")
    print("-" * 88)
    for mod, total, observed, inferred, cited, ratio, in_src, verdict in rows:
        print(f"{mod:40s} {total:4d} {observed:4d} {inferred:4d} "
              f"{cited:4d} {ratio:6.2f} {int(in_src):3d}  {verdict}")

    print()
    n = {v: sum(1 for r in rows if r[7] == v)
         for v in ("NO-CITE", "thin", "partial", "core")}
    fn = {v: sum(r[1] for r in rows if r[7] == v) for v in n}
    tot = sum(fn.values())
    for v in ("core", "partial", "thin", "NO-CITE"):
        print(f"{v:10s} {n[v]:3d} 模块  {fn[v]:5d} 函数  {100.0 * fn[v] / tot:5.1f}%")
    print(f"{'合计':10s} {sum(n.values()):3d} 模块  {tot:5d} 函数")
    g = global_stats(doc_addrs)
    if g:
        total, hit, naddr = g
        print(f"\n全二进制：文档写下过地址的函数 {hit} / {total} "
              f"= {100.0 * hit / total:.2f}%（文档里不同地址共 {naddr} 个）")

    print(f"\n→ {out}")


if __name__ == "__main__":
    sys.exit(main())
