#!/usr/bin/env python3
"""把「哪条符号算 `graphwarrust` 自己的名字」这件事**数出来**，写成夹具。

## 为什么要有这个工具

`tools/rust_sym.py` 定了判据（按 legacy mangling 的长度前缀切段 → 取路径首段的 crate），
`name_bridge.py` / `sym_match.py` / `win_names.py` / `three_way_match.py` 都用它。
但「判据换了以后，每个构建到底多认/少认了多少名字」必须能**独立复算**，而不是
信某次运行的控制台输出。

    .venv/bin/python tools/name_crate_audit.py            # 写 fixtures/name_crate_audit.tsv
    .venv/bin/python tools/name_crate_audit.py --print    # 只打印，不写文件

## 三个口径（同一批符号，只换判据）

| 口径 | 规则 | 谁在用 |
|---|---|---|
| `prefix_only` | `_ZN12graphwarrust…` 字面前缀 | **旧文档/旧估计假设的口径**，也是「漏掉 trait impl」的那个 |
| `unified` | `rust_sym.is_game_name`（direct + impl-head + nested-impl） | 本轮之后所有工具 |
| `coarse` | mangled 正文里出现 `graphwarrust`（子串） | `name_bridge.py` / `sym_match.py` 改动前的隐含行为 |

`prefix_only` 与 `unified` 的差 = 「前缀过滤会静默丢掉的函数」（本表主题 #1）；
`coarse` 与 `unified` 的差 = 「旧工具其实多收了哪些编译器胶水」（主题 #2）。

输出四段：① 每构建符号普查；② 粗口径独有的名字键（含是否落地）；
③ 结构解析救回的名字（旧正则返回 None、新解析能给）；④ 口径对**已交付夹具**的影响
（若只用 prefix_only，会有多少已交付名字/地址消失）。
"""

import argparse
import collections
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import rust_sym          # noqa: E402
import name_bridge as NB  # noqa: E402
import win_names as WN    # noqa: E402
import sym_match as SM    # noqa: E402

# 参考二进制的根（与 sym_match/three_way_match 同一个解析方式；子 agent 的克隆层级更深）
GW2 = SM.GW2

BUILDS = [
    # (标签, 格式, 路径, 说明)
    ("android-2.0.60", "elf-arm64",
     os.path.join(GW2, ".reference/steam/android-2.0.60/libgraphwarrust.so"),
     "名字来源（未 strip，.symtab）"),
    ("windows-2.0.60", "pe-x86_64",
     os.path.join(GW2, ".reference/steam/windows-2.0.60/graphwarrust.dll"),
     "名字来源（COFF 符号表）"),
    ("windows-early2026", "pe-x86_64",
     os.path.join(GW2, ".reference/steam/windows-early2026/graphwarrust.dll"),
     "上一版，仅作对照"),
    ("linux-2026-08-28", "elf-x86_64",
     os.path.join(GW2, ".reference/steam/2026-08-28/libgraphwarrust.so"),
     "**目标**（被 strip，应 0 个名字）"),
]

# 已交付夹具 → (三列名字的下标, 地址列下标, 归属侧)
DELIVERED = [
    ("fixtures/name_to_addr.tsv", (0, 1, 2), 3, "android"),
    ("fixtures/windows_name_to_addr.tsv", (1, 2, 3), 5, "windows"),
    ("fixtures/name_to_addr_v2.tsv", (0, 1, 2), 3, None),
]

PREFIX = "_ZN12graphwarrust"
# 旧 `parse_name` 的兜底正则：要求 `graphwarrust::<module>::<X>` 两段
_OLD_REGEX = re.compile(r"graphwarrust::([A-Za-z0-9_]+)::([A-Za-z0-9_]+)")

COLS = ["build", "format", "symbols", "game_total", "sym_prefix_only", "sym_impl_head",
        "sym_nested_impl", "sym_other_crate", "sym_unparsable",
        "names_prefix_only", "names_unified", "names_coarse",
        "keys_missed_by_prefix", "keys_coarse_only", "keys_recovered_by_parse", "note"]


def elf_symbols(path):
    """唯一定义的 `FUNC` 符号名（`.symtab`；stripped 时为空）。"""
    out = subprocess.run(["readelf", "-sW", path], capture_output=True, text=True).stdout
    names = set()
    for line in out.splitlines():
        f = line.split()
        if len(f) < 8 or f[3] != "FUNC" or f[1] == "UND":
            continue
        names.add(f[7])
    return sorted(names)


def pe_symbols(path):
    """COFF 里落在 `.text` 的函数符号名（`typ == 0x20`）。"""
    pe = WN.PE(path)
    return sorted({n for n, _v, sec, typ, _scl in pe.symbols()
                   if sec == 1 and typ == 0x20 and n})


def keys_of(names, dem, pred):
    """→ {(module,type,method)}；`pred(mangled)` 决定这条符号算不算。"""
    ks = set()
    for n in names:
        if not pred(n):
            continue
        p = NB.parse_name(dem[n])
        if p and p[2]:
            ks.add((p[0], p[1] or "_", p[2]))
    return ks


def is_prefix_only(n):
    """旧口径：`_ZN12graphwarrust…` 字面前缀（漏掉整个 trait impl 形态）。"""
    return n.startswith(PREFIX)


def census(tag, fmt, path, note):
    if not os.path.exists(path):
        return None, None
    names = elf_symbols(path) if fmt.startswith("elf") else pe_symbols(path)
    dem = dict(zip(names, NB.demangle(names)))
    game = [n for n in names if rust_sym.is_game_name(n)]
    labels = collections.Counter(rust_sym.form_label(n) for n in game)
    k_prefix = keys_of(names, dem, is_prefix_only)
    k_unified = keys_of(names, dem, rust_sym.is_game_name)
    k_coarse = keys_of(names, dem, rust_sym.mentions_crate)
    # 结构解析救回：demangle 是 `<SELF as TRAIT>::…`、而旧正则兜底（要求 `crate::X::Y` 两段）
    # 找不到 ⇒ 旧 parse_name 返回 None ⇒ 整个符号被丢。
    recovered = collections.defaultdict(list)
    for n in game:
        d = dem[n]
        if rust_sym.impl_head_tail(d) is None or "<impl " in d:
            continue
        p = NB.parse_name(d)
        if not p or not p[2]:
            continue
        if _OLD_REGEX.search(d) is None:
            recovered[(p[0], p[1] or "_", p[2])].append(n)
    row = dict(
        build=tag, format=fmt, symbols=len(names),
        game_total=len(game),
        sym_prefix_only=sum(1 for n in game if is_prefix_only(n)),
        sym_impl_head=labels.get("impl-head", 0),
        sym_nested_impl=labels.get("nested-impl", 0),
        sym_other_crate=sum(1 for n in names
                            if rust_sym.mentions_crate(n) and not rust_sym.is_game_name(n)),
        sym_unparsable=sum(1 for n in names if rust_sym.classify(n)[0] == "unparsable"),
        names_prefix_only=len(k_prefix),
        names_unified=len(k_unified),
        names_coarse=len(k_coarse),
        keys_missed_by_prefix=len(k_unified - k_prefix),
        keys_coarse_only=len(k_coarse - k_unified),
        keys_recovered_by_parse=len(recovered),
        note=note,
    )
    return row, dict(dem=dem, game=game, names=names, k_prefix=k_prefix,
                     k_unified=k_unified, k_coarse=k_coarse, recovered=recovered)


def delivered_rows():
    """[(rel, key, addr, side)]：只保留**带地址**的行。"""
    out = []
    for rel, cols, ai, side in DELIVERED:
        p = os.path.join(REPO, rel)
        if not os.path.exists(p):
            continue
        for line in open(p, encoding="utf-8"):
            if line.startswith("#") or not line.strip():
                continue
            f = line.rstrip("\n").split("\t")
            if f[0] in ("module", "func_va") or len(f) <= ai:
                continue
            if not f[ai].startswith("0x"):
                continue
            out.append((rel, (f[cols[0]], f[cols[1]] or "_", f[cols[2]]), f[ai], side))
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--print", dest="do_print", action="store_true",
                    help="只打印，不写 fixtures/name_crate_audit.tsv")
    ap.add_argument("--out", default=os.path.join(REPO, "fixtures/name_crate_audit.tsv"))
    o = ap.parse_args(argv)

    rows, states = [], {}
    for tag, fmt, path, note in BUILDS:
        row, extra = census(tag, fmt, path, note)
        if row is None:
            sys.stderr.write("跳过（文件不存在）：%s\n" % path)
            continue
        rows.append(row)
        states[tag] = extra

    drows = delivered_rows()
    landed = {k for _rel, k, _a, _s in drows}

    L = []
    L.append("# 由 tools/name_crate_audit.py 生成，不要手改（重跑会覆盖）。")
    L.append("# 判据（唯一实现在 tools/rust_sym.py，**不是**子串匹配）：legacy mangling")
    L.append("#   `_ZN<len><elem><elem>…17h<hex>E` 逐段按**十进制长度前缀**切分，")
    L.append("# 取路径首段（或 `<SELF as TRAIT>` 的 SELF 首段）作为 crate 名。")
    L.append("#   direct      `_ZN12graphwarrust…`                    首段就是 crate 名")
    L.append("#   impl-head   `_ZN<len>_$LT$graphwarrust..T$u20$as$u20$Trait$GT$…`")
    L.append("#               crate 名**没有长度前缀**（GDExtension 的 trait impl）——")
    L.append("#               只用 `_ZN12graphwarrust` 前缀会把这一整类丢掉（本表主题 #1）")
    L.append("#   nested-impl `graphwarrust::…<impl Trait for graphwarrust::T>…`（derive）")
    L.append("#   other-crate 正文提到 graphwarrust、但首段 crate 不是它")
    L.append("#               （`core::ptr::drop_in_place<…graphwarrust::X…>` 这类编译器胶水）")
    L.append("# 三口径：prefix_only = `_ZN12graphwarrust` 字面前缀；unified = 本判据；")
    L.append("#         coarse = mangled 正文含 `graphwarrust`"
             "（name_bridge/sym_match 改动前的隐含行为）。")
    L.append("# names_* = 该口径下能切出的 (module,type,method) 唯一键数。")
    L.append("")
    L.append("# ===== ① 每构建符号普查 =====")
    L.append("\t".join(COLS))
    for r in rows:
        L.append("\t".join(str(r[c]) for c in COLS))

    L.append("")
    L.append("# ===== ② 粗口径独有的名字键（「含 graphwarrust」会凭空多出来的名字）=====")
    L.append("# 全部来自 other-crate 符号：parse_name 从泛型实参里抠出来的，不是源码里的方法名。")
    L.append("# build\tmodule\ttype\tmethod\tsymbols\texample_mangled"
             "\tlands_addr(1=已交付夹具里有地址)")
    for tag, st in states.items():
        extra = st["k_coarse"] - st["k_unified"]
        if not extra:
            continue
        by_key = collections.defaultdict(list)
        for n in st["names"]:
            if not rust_sym.mentions_crate(n) or rust_sym.is_game_name(n):
                continue
            p = NB.parse_name(st["dem"][n])
            if p and p[2]:
                by_key[(p[0], p[1] or "_", p[2])].append(n)
        for k in sorted(extra):
            syms = by_key.get(k, [])
            L.append("\t".join([tag, k[0], k[1], k[2], str(len(syms)),
                                syms[0] if syms else "-",
                                "1" if k in landed else "0"]))

    L.append("")
    L.append("# ===== ③ 结构解析救回的名字（旧正则返回 None、新解析给得出）=====")
    L.append("# 形态：`<graphwarrust::Type as Trait>::method` —— 自类型直接在 crate 根、没有")
    L.append("# `<module>` 段，旧正则兜底 `graphwarrust::X::Y` 匹配不到 ⇒ 整个符号被丢弃。")
    L.append("# build\tmodule\ttype\tmethod\tsymbols\texample_mangled\tlands_addr")
    any_recovered = False
    for tag, st in states.items():
        for k, syms in sorted(st["recovered"].items()):
            any_recovered = True
            L.append("\t".join([tag, k[0], k[1], k[2], str(len(syms)), syms[0],
                                "1" if k in landed else "0"]))
    if not any_recovered:
        L.append("#\t（本轮：无）")

    L.append("")
    L.append("# ===== ④ 若只用 prefix_only，已交付夹具会消失多少（what-if）=====")
    L.append("# 只统计「已交付且带地址」的行；k∈unified 且 k∉prefix_only 才算前缀口径的损失。")
    L.append("# side\tdelivered_named_rows\tlost_names\tlost_addrs")
    for side, tag in (("android", "android-2.0.60"), ("windows", "windows-2.0.60")):
        st = states.get(tag)
        if st is None:
            continue
        tot = sum(1 for _rel, _k, _x, sd in drows if sd == side)
        lost_keys, lost_addrs = set(), set()
        for rel, k, a, sd in drows:
            if sd != side or k in st["k_prefix"] or k not in st["k_unified"]:
                continue
            lost_keys.add(k)
            lost_addrs.add(a)
        L.append("\t".join([side, str(tot), str(len(lost_keys)), str(len(lost_addrs))]))

    text = "\n".join(L) + "\n"
    if o.do_print:
        sys.stdout.write(text)
    else:
        with open(o.out, "w", encoding="utf-8") as fh:
            fh.write(text)
        sys.stderr.write("写出 %s（%d 个构建）\n" % (o.out, len(rows)))
    for r in rows:
        sys.stderr.write("  %-20s 符号 %6d  游戏 %4d（prefix %4d / impl %3d / nested %3d）"
                         " 名字 prefix %4d → unified %4d（coarse %4d）\n"
                         % (r["build"], r["symbols"], r["game_total"],
                            r["sym_prefix_only"], r["sym_impl_head"], r["sym_nested_impl"],
                            r["names_prefix_only"], r["names_unified"], r["names_coarse"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
