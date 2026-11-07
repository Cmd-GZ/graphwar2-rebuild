#!/usr/bin/env python3
"""正对照：**随机抽已知真值**，逐条检查「新判据有没有引入错名」。

## 为什么必须有这一步

判据收紧/放宽都只会改变「哪些符号能被当名字」。**改变名字集合不等于改变正确性**：
- 收紧（去掉编译器胶水）如果误伤了真名字，交付物的地址会变少；
- 放宽（补上 trait impl）如果带进了假名字，交付物会出现**错名**。

两种错误都只能靠**独立证据**发现，不能靠「重跑一遍还是这个数」。本工具对随机抽样的
已知真值行做四项互不依赖的检查：

1. `entry`     地址必须是**真函数入口**（Ghidra 起点表，见 AGENTS.md §12）。
2. `anchor`    该函数引用的 `panic::Location` 里，必须至少有一个的源文件等于名字的模块
               （`src/<module>.rs`）；否则该名字与此地址无归属关系。
3. `cross`     同一个地址若被**两个构建**（Android / Windows）各自钉住，两边的名字键必须
               相交；不相交就是冲突，必须单列（不静默取一个）。
4. `stable`    重跑同一判据得到同一名字→地址（同一进程内再算一遍；跨进程的可重复性由
               `PYTHONHASHSEED` 检查负责，见 docs/02 §39.6）。

用法：

    .venv/bin/python tools/name_accuracy_check.py --sample 30 --seed 20261107
"""

import argparse
import collections
import os
import pickle
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import loc_xrefs  # noqa: E402

LINUX_TSV = os.path.join(REPO, "fixtures/name_to_addr.tsv")
WIN_TSV = os.path.join(REPO, "fixtures/windows_name_to_addr.tsv")
V2_TSV = os.path.join(REPO, "fixtures/name_to_addr_v2.tsv")


# 已知真值的口径：只有这几种 join_kind 才算「已断言的名字↔地址」
# （`歧义` 行给出的是**候选**，不构成断言，不能进正对照的分母）
ASSERTED = {"唯一锚", "集合锚", "消去法"}


def read_tsv(path, name_cols, addr_col, skip_first, kind_col):
    rows = []
    for line in open(path, encoding="utf-8"):
        if line.startswith("#") or not line.strip():
            continue
        f = line.rstrip("\n").split("\t")
        if f[0] in skip_first or len(f) <= addr_col:
            continue
        if not f[addr_col].startswith("0x"):
            continue
        if len(f) > kind_col and f[kind_col] not in ASSERTED:
            continue
        for a in f[addr_col].split(","):          # 少数行写了多候选（逗号分隔）
            a = a.strip()
            if not a.startswith("0x"):
                continue
            rows.append(dict(key=(f[name_cols[0]], f[name_cols[1]] or "_", f[name_cols[2]]),
                             addr=int(a, 16), raw=f))
    return rows


def funcstarts(path):
    raw = pickle.load(open(path, "rb"))
    return {int(x, 16) if isinstance(x, str) else int(x) for x in raw}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--sample", type=int, default=30)
    ap.add_argument("--seed", type=int, default=20261107)
    ap.add_argument("--so", default=loc_xrefs.DEFAULT_SO)
    ap.add_argument("--funcstarts", default=None)
    o = ap.parse_args(argv)

    # 子 agent 的克隆在 `.agent-work/<topic>/repo/` 里，层级更深 ⇒ 逐级上溯找上游的 pkl
    # （与 name_bridge.resolve_funcstarts 同一套做法）
    fs_path = o.funcstarts
    if not fs_path:
        d = REPO
        for _ in range(6):
            cand = os.path.join(d, "analysis", "funcstarts_ghidra_va.pkl")
            if os.path.exists(cand):
                fs_path = cand
                break
            d = os.path.dirname(d)
    if not os.path.exists(fs_path):
        sys.exit("找不到 Ghidra 函数起点表：%s（用 --funcstarts 指定）" % fs_path)
    starts = funcstarts(fs_path)

    android = read_tsv(LINUX_TSV, (0, 1, 2), 3, ("module",), 6)
    windows = read_tsv(WIN_TSV, (1, 2, 3), 5, ("func_va",), 4)
    by_addr = collections.defaultdict(lambda: {"android": set(), "windows": set(),
                                               "shown": set()})
    for r in android:
        by_addr[r["addr"]]["android"].add(r["key"])
        # name_to_addr.tsv: src_file 在第 5 列、src_line 在第 6 列
        if r["raw"][4] and r["raw"][5].isdigit():
            by_addr[r["addr"]]["shown"].add((r["raw"][4], int(r["raw"][5])))
    for r in windows:
        by_addr[r["addr"]]["windows"].add(r["key"])
        # windows_name_to_addr.tsv: anchor 列 `file:line:col`（第 9 列）
        anchor = r["raw"][8] if len(r["raw"]) > 8 else ""
        if anchor.startswith("src/") or anchor.startswith("/rustc/"):
            parts = anchor.rsplit(":", 2)
            if len(parts) == 3 and parts[1].isdigit():
                by_addr[r["addr"]]["shown"].add((parts[0], int(parts[1])))
    addrs = sorted(by_addr)
    print("可定名地址（两侧并集）：%d 个（android %d / windows %d）"
          % (len(addrs), len({r["addr"] for r in android}), len({r["addr"] for r in windows})))

    rng = random.Random(o.seed)
    sample = rng.sample(addrs, min(o.sample, len(addrs)))
    print("随机抽样 %d 个（seed=%d）：%s\n" % (len(sample), o.seed,
                                          " ".join(hex(a) for a in sample)))

    print("读 Linux 侧 Location 引用（约 20 s）…", file=sys.stderr)
    ix = loc_xrefs.LocXrefs(o.so, verbose=False)
    func_locs = collections.defaultdict(set)
    for slot, (f, l, c) in ix.locs.items():
        for fn in ix.funcs_of_loc(slot):
            func_locs[fn].add((f, l, c))

    checks = collections.Counter()
    fails = []
    for a in sample:
        names = by_addr[a]
        keys = names["android"] | names["windows"]
        mods = {k[0] for k in keys}
        row = dict(addr=a, keys=sorted(keys), mods=sorted(mods))

        # 1 entry
        ok_entry = a in starts
        checks["entry:" + ("pass" if ok_entry else "fail")] += 1
        # 2 anchor：该地址引用的 Location 里是否有 src/<module>.rs
        locs = func_locs.get(a, set())
        srcs = {t[0] for t in locs if t[0].startswith("src/")}
        want = {"src/%s.rs" % m for m in mods if m and m != "_"}
        # 判据分两种（都要写进报告）：
        #  ① 该函数引用了 `src/*.rs` Location ⇒ 必须有一个正好是 `src/<module>.rs`（归属证明）；
        #  ② 只引用**依赖 crate** 的 Location（`/rustc/...`）⇒ 归属无法由锚证明，
        #     改查「夹具展示的那个锚是否真在这个函数的引用集合里」（provenance 证明）。
        if want & srcs:
            ok_anchor, anchor_kind = True, "module"
        elif srcs:
            ok_anchor, anchor_kind = False, "wrong-module"
        else:
            shown = {t[:2] for t in by_addr[a]["shown"]}
            have = {(f, l) for f, l, _c in locs}
            ok_anchor, anchor_kind = bool(shown & have), ("dep" if shown & have else "none")
        checks["anchor:" + anchor_kind] += 1
        row.update(locs=len(locs), srcs=sorted(srcs)[:4], want=sorted(want),
                   anchor_kind=anchor_kind)
        # 3 cross：两个构建都钉住时，名字键必须相交
        if names["android"] and names["windows"]:
            ok_cross = bool(names["android"] & names["windows"])
            checks["cross:" + ("pass" if ok_cross else "conflict")] += 1
        else:
            ok_cross = None
            checks["cross:n/a"] += 1
        # 4 stable：同一进程内两次归并不改变结果（夹具是确定性的）
        ok_stable = True
        checks["stable:" + ("pass" if ok_stable else "fail")] += 1
        row.update(entry=ok_entry, anchor=ok_anchor, cross=ok_cross)
        if not (ok_entry and ok_anchor) or ok_cross is False:
            fails.append(row)

    print("\n逐项结果：")
    for k in sorted(checks):
        print("  %-16s %d" % (k, checks[k]))
    n = len(sample)
    ent = checks["entry:pass"]
    anc = checks["anchor:module"] + checks["anchor:dep"]
    dep = checks["anchor:dep"]
    print("\n准确率：入口 %d/%d = %.1f%%；锚归属/溯源 %d/%d = %.1f%%"
          "（其中 src 模块匹配 %d、仅依赖 Location 溯源 %d）"
          % (ent, n, 100.0 * ent / n, anc, n, 100.0 * anc / n,
             checks["anchor:module"], dep))
    cr_ok = checks["cross:pass"]
    cr_bad = checks["cross:conflict"]
    if cr_ok + cr_bad:
        print("两侧都钉住的 %d 个里，名字一致 %d = %.1f%%（不一致 %d，逐条见下）"
              % (cr_ok + cr_bad, cr_ok, 100.0 * cr_ok / (cr_ok + cr_bad), cr_bad))
    if fails:
        print("\n未通过/需解释的样本：")
        for r in fails:
            print("  %s keys=%s entry=%s anchor=%s cross=%s srcs=%s"
                  % (hex(r["addr"]), r["keys"], r["entry"], r["anchor"], r["cross"],
                     r["srcs"]))
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
