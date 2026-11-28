#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""T110 审计脚本 —— 分母外台账（fixtures/denom_outside.tsv）+ 四口径的**独立复算**。

只读，不改任何台账/源码。四个模式（可全跑）：

    $PY fixtures/denom_outside_audit_t110.py --calibers        # 四口径从零复算 vs tools/function_coverage.py --stats
    $PY fixtures/denom_outside_audit_t110.py --contradictions  # 台账内部矛盾扫描 S1–S7（全 1244 行）
    $PY fixtures/denom_outside_audit_t110.py --serde           # T107 的 13 条 serde derive 判据独立复核（D1∧D2 + 负对照）
    $PY fixtures/denom_outside_audit_t110.py --impl            # 每条 denom:no-loc 行到 src/ 找同名 fn / vaddr 注释

口径与判据的出处（**先提交后跑**）：`fixtures/denom_outside_refresh_plan.txt` §5/§6。
地址口径：ELF vaddr（RVA）。$PY = `.reference/venv/bin/python`（需要 capstone）。
"""
import argparse
import collections
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))

ANDROID_SO = os.path.join(ROOT, ".reference/steam/android-2.0.60/libgraphwarrust.so")
DENOM_TSV = os.path.join(ROOT, "fixtures/denom_outside.tsv")
BA_TSV = os.path.join(ROOT, "fixtures/boundary_audit.tsv")
FC_TSV = os.path.join(ROOT, "fixtures/function_coverage.tsv")
NA_TSV = os.path.join(ROOT, "fixtures/f3_triage.tsv")
MATRIX = os.path.join(ROOT, "docs/coverage-matrix.tsv")
TOOL = os.path.join(ROOT, "tools/function_coverage.py")

NA_PREFIX = "na:"
NA_STRONG = ("na:std-mono", "na:gdext-glue")      # = tools/function_coverage.py::NA_STRONG
GRADES_WITH_EVIDENCE = ("逐位", "观测")            # = ...::GRADES_WITH_EVIDENCE
PIN_F1 = ("name", "name-other", "addr", "name-soft", "macro")
GENERIC_METHODS = ("new", "default", "serialize", "deserialize", "clone", "drop", "tick",
                   "process", "ready", "clear", "init")


# ---------------------------------------------------------------------------
# 读表（一律跳过 `#` 注释；`stop_banner` 用于 f3_triage 的「主表 vs F2-scan 分节」边界）
# ---------------------------------------------------------------------------
def read_tsv(path, first_cols=("addr", "android_addr", "module", "vaddr"), stop_banner=False):
    out = []
    with open(path, encoding="utf-8") as fh:
        for ln in fh:
            if stop_banner and ln.startswith("# ----"):
                break
            if ln.startswith("#") or not ln.strip():
                continue
            f = ln.rstrip("\n").split("\t")
            if f[0] in first_cols or not f[0].startswith("0x"):
                continue
            out.append(f)
    return out


def load_denom():
    """denom_outside.tsv → [{addr, size, module, owner, method, form, verdict, subclass, group, ours, note}]"""
    out = []
    for f in read_tsv(DENOM_TSV):
        if len(f) < 19:
            continue
        out.append(dict(addr=int(f[0], 16), addr_s=f[0], size=int(f[1]), module=f[3], owner=f[4],
                        method=f[5], form=f[6], verdict=f[13], subclass=f[14], group=f[15],
                        ours=f[16], note=f[18] if len(f) > 18 else ""))
    return out


def load_matrix_grades():
    g = {}
    for ln in open(MATRIX, encoding="utf-8"):
        if ln.startswith("#"):
            continue
        f = ln.rstrip("\n").split("\t")
        if f[0] == "module":
            continue
        g[f[0]] = f[7]
    return g


def load_addr_manual():
    """从生成器源码里原样抽出人工复核闸 `ADDR_MANUAL`（它是**数据表**，不是判据逻辑）。"""
    src = open(TOOL, encoding="utf-8").read()
    blk = src[src.index("ADDR_MANUAL = {"):]
    blk = blk[:blk.index("\n}\n") + 3]
    return {m.group(1): m.group(2) for m in re.finditer(r'"(0x[0-9a-f]+)":\s*\("(F\d)"', blk)}


# ---------------------------------------------------------------------------
# 模式 1：四口径独立复算
# ---------------------------------------------------------------------------
def mode_calibers(verbose=True):
    R = {}
    ba = read_tsv(BA_TSV)
    fc = read_tsv(FC_TSV)
    R["n"] = len(ba)
    R["n_fc"] = len(fc)
    grade = load_matrix_grades()
    am = load_addr_manual()
    mine, tool, dis = collections.Counter(), collections.Counter(), []
    for f in fc:
        addr, name, mod, ours_loc, bucket, sub, pin = f[0], f[1], f[3], f[5], f[6], f[7], f[8]
        tool[bucket] += 1
        g = grade.get(mod, "?")
        has_body = ours_loc.strip() not in ("", "—")
        if pin in PIN_F1 and g in GRADES_WITH_EVIDENCE and has_body:
            b = "F1"
        elif pin in ("name-free", "hyb", "scaffold") or (pin in PIN_F1 and has_body):
            b = "F2"
        elif pin == "none":
            b = "F3"
        else:
            b = "F2"
        if pin == "addr" and addr in am:          # 人工复核闸：**只作用于 pin==addr**（工具原闸门）
            b = am[addr]
        mine[b] += 1
        if b != bucket:
            dis.append((addr, name, mod, g, pin, bucket, b, sub))
    R["tool"], R["mine"], R["dis"], R["addr_manual"] = tool, mine, dis, am
    # na 台账：**主表**（横幅之后是机判候选，不得计入）
    tri = read_tsv(NA_TSV, stop_banner=True)
    cats = collections.Counter(f[4] for f in tri)
    R["n_na"] = sum(v for k, v in cats.items() if k.startswith(NA_PREFIX))
    R["n_strong"] = sum(v for k, v in cats.items() if k in NA_STRONG)
    R["n_ledger"], R["cats"] = len(tri), cats
    # X：同 src_fn_group 的 business 行，verdict 一致且以 /denom:no-loc 结尾
    de = load_denom()
    grp = collections.defaultdict(list)
    for r in de:
        grp[r["group"]].append(r)
    buckets, X, Xmc = collections.Counter(), 0, 0
    for k, v in grp.items():
        bs = [x for x in v if x["form"] == "business"]
        if not bs:
            continue
        vs = {x["verdict"] for x in bs}
        if len(vs) != 1:
            buckets["<mixed>"] += 1
            continue
        vd = vs.pop()
        if not vd.endswith("/denom:no-loc"):
            continue
        buckets[vd.split("/")[0]] += 1
        X += 1
        Xmc += len(bs)
    R["X"], R["Xmc"], R["Xbuckets"] = X, Xmc, buckets
    n, m1 = R["n"], mine["F1"]
    R["cal"] = [("A", m1, n), ("B", m1, n - R["n_na"]), ("B'", m1, n - R["n_strong"]),
                ("D_A", m1, n + X), ("D_B", m1, n - R["n_na"] + X),
                ("D_B'", m1, n - R["n_strong"] + X),
                ("D_A+", m1 + (X - buckets["real-missing"]), n + X)]
    if verbose:
        print("=== T110 四口径**独立复算**（不复用 function_coverage.py 的 stats()/classify()）===")
        print("n（boundary_audit 行数）        = %d    （生成器台账行数 %d）" % (R["n"], R["n_fc"]))
        print("F 档  工具判档 = %s" % dict(R["tool"]))
        print("      **独立重判** = %s ；逐行不一致 **%d** 行" % (dict(R["mine"]), len(dis)))
        for d in dis[:20]:
            print("         %s %-56s grade=%s pin=%s 工具=%s 我=%s (%s)" % d)
        print("na 台账（主表）n_ledger=%d  na:*=%d  强 na=%d ；类别=%s"
              % (R["n_ledger"], R["n_na"], R["n_strong"], dict(R["cats"])))
        print("X（源码组）= **%d 组 / %d 条机器码**；分类 = %s" % (X, Xmc, dict(buckets)))
        print("\n  口径   F1 / 分母        = 百分比")
        for tag, num, den in R["cal"]:
            print("  %-6s %4d / %4d = %5.1f%%" % (tag, num, den, num * 100.0 / den))
        print("\n恒等式：D_A 分母 %d == 527 + X(%d) ? %s ；F1+F2+F3+F4 == 527 ? %s"
              % (R["cal"][3][2], X, R["cal"][3][2] == 527 + X,
                 sum(R["mine"][k] for k in ("F1", "F2", "F3", "F4")) == 527))
        print("⚠️ 已知陷阱：`fixtures/f3_triage.tsv` 若不按 `# ----` 横幅截断（整文件读），")
        print("   na:* 会数成 %d / 强 na %d ⇒ B 分母 %d（假）。生成器与本源都用**主表**。"
              % (0, 0, 0) if False else "")
    return R


# ---------------------------------------------------------------------------
# 模式 2：台账内部矛盾扫描 S1–S7（全 1244 行）
# ---------------------------------------------------------------------------
def mode_contradictions(verbose=True):
    de = load_denom()
    found = collections.OrderedDict()

    # S1 real-missing 却填了 ours_src（T107 抓 13 条用的判据）
    s1 = [r for r in de if r["verdict"].startswith("real-missing") and r["ours"].strip() not in ("", "—")]
    found["S1 real-missing 但 ours_src≠—"] = s1
    # S2 同组 business 行 verdict 不一致
    grp = collections.defaultdict(list)
    for r in de:
        if r["form"] == "business":
            grp[r["group"]].append(r)
    s2 = [(k, sorted({x["verdict"] for x in v})) for k, v in grp.items() if len({x["verdict"] for x in v}) > 1]
    found["S2 同组 business verdict 不一致"] = s2
    # S3 business+no-loc 且 method ∈ {serialize,deserialize} 且 owner=='_'（= serde trait impl 形态）
    s3 = [r for r in de if r["form"] == "business" and r["verdict"].endswith("/denom:no-loc")
          and r["owner"] == "_" and r["method"] in ("serialize", "deserialize")]
    found["S3 business 且是 serde trait impl 形态（C-NA 复扫候选）"] = s3
    # S4 note/ours 自称 own-module 但 verdict 不是 implemented
    s4 = [r for r in de if "own-module" in r["note"] and not r["verdict"].startswith("implemented/")]
    found["S4 note 称 own-module 但 verdict≠implemented"] = s4
    # S5 头部计数块 vs 实际逐行统计
    hdr = {}
    with open(DENOM_TSV, encoding="utf-8") as fh:
        for ln in fh:
            if not ln.startswith("#"):
                break
            m = re.match(r"#\s{2}(\S+:\S+|na:\S+|unknown|real-missing/denom:no-loc|"
                         r"implemented/denom:no-loc|implemented-alt/denom:no-loc)\s+(\d+)\s*$", ln.rstrip())
            if m:
                hdr[m.group(1)] = int(m.group(2))
    actual = collections.Counter(r["verdict"] for r in de)
    s5 = [(k, v, actual.get(k)) for k, v in hdr.items() if actual.get(k) != v]
    found["S5 头部计数 vs 逐行统计"] = s5
    # S6 implemented 但 method 是通用名（同名 ≠ 同一函数，弱证据）
    s6 = [r for r in de if r["verdict"].startswith("implemented/") and r["method"] in GENERIC_METHODS]
    found["S6 implemented 但 method 是通用名（弱证据）"] = s6
    # S7 implemented 但 src/ 里扫不到同名 fn（工具假阳性）
    idx = _src_fn_index()
    s7 = [r for r in de if r["verdict"].startswith("implemented/")
          and r["method"] not in ("_",) and r["method"] not in idx]
    found["S7 implemented 但 src/ 里没有同名 fn（假阳性嫌疑）"] = s7

    if verbose:
        print("=== 台账内部矛盾扫描（全 %d 行；判据见预注册 plan §6）===" % len(de))
        for k, v in found.items():
            print("\n%s ：%d 条" % (k, len(v)))
            for x in v[:12]:
                if isinstance(x, dict):
                    print("   0x%06x %-30s %-42s %s" % (x["addr"], x["module"], x["method"], x["verdict"]))
                else:
                    print("   %s" % (x,))
            if len(v) > 12:
                print("   …（还有 %d 条）" % (len(v) - 12))
    return found


def _src_fn_index():
    names = set()
    pat = re.compile(r"\bfn\s+([A-Za-z0-9_]+)\s*(?:<|\()")
    for dp, _dn, fns in os.walk(os.path.join(ROOT, "src")):
        for fn in fns:
            if not fn.endswith(".rs"):
                continue
            for ln in open(os.path.join(dp, fn), encoding="utf-8", errors="replace"):
                s = ln.lstrip()
                if s.startswith("//"):
                    continue
                m = pat.search(ln)
                if m:
                    names.add(m.group(1))
    return names


# ---------------------------------------------------------------------------
# 模式 3：T107 的 13 条 serde derive 判据**独立复核**（D1∧D2 + 负对照）
# ---------------------------------------------------------------------------
def mode_serde(verbose=True):
    import naming_android as NA
    side = NA.Side(ANDROID_SO, "android")
    t = side.text
    lo, hi = t["addr"], t["addr"] + t["size"]
    syms = [s for s in side.elf.symtab()
            if s["type"] == NA.STT_FUNC and s["size"] > 0 and lo <= s["value"] < hi]
    dem = [re.sub(r"::h[0-9a-f]{16}", "", d) for d in NA.demangle_all([s["name"] for s in syms])]
    byaddr = collections.defaultdict(list)
    for s, d in zip(syms, dem):
        byaddr[s["value"]].append(d)
    IMPL = re.compile(r"<impl\s+serde::(ser|de)::\w+\s+for\s+([A-Za-z0-9_:]+)>")
    SIB = re.compile(r"<impl\s+serde::de::\w+\s+for\s+([A-Za-z0-9_:]+)>\s*::\s*(?:deserialize|serialize)"
                     r"\s*::\s*(__Visitor|__FieldVisitor|__Field|__SerdeRenamed)\b")
    priv = collections.defaultdict(set)
    for d in dem:
        m = SIB.search(d)
        if m:
            priv[m.group(1)].add(m.group(2))

    rows = load_denom()
    byaddr_row = {r["addr"]: r for r in rows}
    # 全台账：每条是 serde trait impl 的机器码行
    impl_rows = []
    for a, ds in byaddr.items():
        for d in ds:
            m = IMPL.search(d)
            if m and a in byaddr_row:
                impl_rows.append((a, m.group(2), sorted(priv.get(m.group(2), [])), byaddr_row[a]["form"],
                                  byaddr_row[a]["verdict"]))
    # 正对照 = 现在的 13 条 na:other/serde-derive；负对照 = 手写固有方法 3 条
    pos = [r for r in impl_rows if byaddr_row[r[0]]["subclass"] == "serde-derive"]
    neg = [(a, byaddr_row[a]["verdict"]) for a in (0x4691a4, 0x46947c, 0x4693c0)]
    ok_pos = all(r[2] for r in pos)
    neg_clean = [a for a, _v in neg if a not in {r[0] for r in impl_rows}]
    if verbose:
        print("=== serde derive 判据独立复核（D1=`<impl serde::… for M::T>`；D2=同类型有 derive 私有名兄弟）===")
        print("本构建带 serde_derive 私有名的类型数：%d" % len(priv))
        print("\n台账里是 serde trait impl 的机器码行：**%d** 条" % len(impl_rows))
        for k, v in sorted(collections.Counter((bool(r[2]), r[3], r[4]) for r in impl_rows).items()):
            print("   D2=%-5s form=%-13s verdict=%-26s %3d" % (k[0], k[1], k[2], v))
        print("\n正对照（T107 A 组，应 13 条且 D2 全成立）：%d 条，D2 全中=%s" % (len(pos), ok_pos))
        bad = [r for r in pos if not r[2]]
        for r in bad:
            print("   ✗ 0x%06x %s" % (r[0], r[1]))
        print("负对照（手写固有方法 3 条，D1 应不成立 ⇒ 不出现在 impl_rows 里）：")
        for a, v in neg:
            print("   0x%06x 在 impl_rows 里？%s（verdict=%s）" % (a, "**是（判据漏了）**" if a in {r[0] for r in impl_rows} else "否 ✓", v))
        print("\n⇒ 结论：漏判只可能是「D1∧D2 成立但 form=business」的行；台账里这类行 = %d 条"
              % len([r for r in impl_rows if r[2] and r[3] == "business"]))
    return dict(impl_rows=impl_rows, pos=pos, neg=neg)


# ---------------------------------------------------------------------------
# 模式 4：逐条落码核对（同名 fn / vaddr 注释 + 函数体）
# ---------------------------------------------------------------------------
def mode_impl(verbose=True):
    files = []
    for dp, _dn, fns in os.walk(os.path.join(ROOT, "src")):
        for fn in sorted(fns):
            if fn.endswith(".rs"):
                p = os.path.join(dp, fn)
                files.append((os.path.relpath(p, ROOT),
                              open(p, encoding="utf-8", errors="replace").read().splitlines()))
    fnre = re.compile(r"\bfn\s+([A-Za-z0-9_]+)\s*(?:<|\()")
    def find_fn(name):
        pat = re.compile(r"\bfn\s+" + re.escape(name) + r"\s*(?:<|\()")
        for p, lines in files:
            for i, t in enumerate(lines, 1):
                if not t.lstrip().startswith("//") and pat.search(t):
                    return "%s:%d" % (p, i)
        return None
    def find_addr(a):
        h = "%06x" % a
        pat = re.compile(r"(?:0x)?" + h + r"\b", re.I)
        out = []
        for p, lines in files:
            for i, t in enumerate(lines, 1):
                if pat.search(t):
                    nxt = None
                    for j in range(i, min(i + 9, len(lines) + 1)):
                        m = fnre.match(lines[j - 1].lstrip())
                        if m:
                            nxt = "%s:%d" % (m.group(1), j)
                            break
                    out.append((p, i, t.strip()[:100], nxt))
        return out
    rows = [r for r in load_denom() if r["verdict"].startswith("real-missing")]
    if verbose:
        print("=== 逐条落码核对：real-missing 的 %d 行到 src/ 找证据（预注册 §1 C-IMPL-1/2/3）===" % len(rows))
    res = []
    for r in rows:
        fn = find_fn(r["method"])
        va = find_addr(r["addr"])
        res.append((r, fn, va))
        if verbose:
            print("\n0x%06x %s::%s" % (r["addr"], r["module"], r["method"]))
            print("   ① 同名 fn：%s" % (fn or "**无**"))
            if va:
                for h in va[:3]:
                    print("   ② vaddr@%s:%d fn=%s  %s" % (h[0], h[1], h[3] or "—", h[2]))
            else:
                print("   ② vaddr 注释：**无**")
    return res


# ---------------------------------------------------------------------------
# 模式 5：刷新前后**逐行**差异（改判留痕表；输出 fixtures/denom_outside_refresh_diff.tsv）
# ---------------------------------------------------------------------------
def load_denom_from(path):
    out = {}
    for i, ln in enumerate(open(path, encoding="utf-8"), 1):
        if ln.startswith("#"):
            continue
        f = ln.rstrip("\n").split("\t")
        if f[0] == "android_addr" or len(f) < 19 or not f[0].startswith("0x"):
            continue
        out[int(f[0], 16)] = dict(line=i, row=f)
    return out


def mode_refresh_diff(old_path, out_path=None, verbose=True):
    old = load_denom_from(old_path)
    new = load_denom_from(DENOM_TSV)
    if set(old) != set(new):
        raise SystemExit("两份台账的地址集合不同：多 %d / 少 %d"
                         % (len(set(new) - set(old)), len(set(old) - set(new))))
    rows = []
    for a in sorted(old):
        o, n = old[a]["row"], new[a]["row"]
        if o[13] == n[13] and o[16] == n[16] and o[14] == n[14]:
            continue
        ov, nv, nsub, nours = o[13], n[13], n[14], n[16]
        if ov == nv and o[16] != nours:
            kind = "ours_src 更新"
        elif nsub == "serde-derive":
            kind = "MANUAL-NA（T107/T110）"
        elif nsub.startswith("no-loc:module-file-split") or nsub == "no-loc:layered":
            kind = "MANUAL-IMPL（T110）"
        elif ov.startswith("real-missing") and nv.startswith("implemented"):
            kind = "AUTO（工具重跑检测到落码）"
        else:
            kind = "其它"
        rows.append((new[a]["line"], a, n, ov, nv, nsub, nours, kind))
    if out_path:
        with open(out_path, "w", encoding="utf-8") as fh:
            fh.write("# denom_outside_refresh_diff.tsv —— T110：分母外台账**刷新前后**的逐行差异（改判留痕）\n")
            fh.write("# 生成：$PY fixtures/denom_outside_audit_t110.py --refresh-diff <旧台账> --out <本文件>\n")
            fh.write("#   旧 = 刷新前入库版（commit 696e513，T107 后）；新 = 本次刷新（T107_MANUAL_NA + T110_MANUAL_IMPL 生效）\n")
            fh.write("# 列：行号(新台账) | android vaddr | module | method | 原值 verdict | 新值 verdict | 新 subclass | 新 ours_src | 类别\n")
            fh.write("# 类别说明：AUTO = 工具自身按「同名 fn / 组级」判出来的（本轮没人工介入，是 T106/T108/T109 落码后重跑的结果）；\n")
            fh.write("#           MANUAL-* = 人工表命中（依据见台账尾部 §MANUAL-T110 + 预注册 plan §4）\n")
            for line, a, n, ov, nv, nsub, nours, kind in rows:
                fh.write("\t".join(["L%d" % line, "0x%06x" % a, n[3], n[5], ov, nv, nsub, nours, kind]) + "\n")
    if verbose:
        print("=== 刷新前后逐行差异（%d 行；旧=%s）===" % (len(rows), old_path))
        cnt = collections.Counter(r[7] for r in rows)
        for k, v in cnt.most_common():
            print("   %-32s %d" % (k, v))
        for r in rows:
            print("  %-6s 0x%06x %-30s %-42s %-30s → %-28s %s"
                  % ("L%d" % r[0], r[1], r[2][3], r[2][5], r[3], r[4], r[7]))
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--calibers", action="store_true")
    ap.add_argument("--contradictions", action="store_true")
    ap.add_argument("--serde", action="store_true")
    ap.add_argument("--impl", action="store_true")
    ap.add_argument("--refresh-diff", metavar="OLD_TSV")
    ap.add_argument("--out", metavar="PATH")
    a = ap.parse_args()
    if a.refresh_diff:
        mode_refresh_diff(a.refresh_diff, a.out)
        return
    if not any(vars(a).values()):
        a.calibers = a.contradictions = a.impl = True
    if a.calibers:
        mode_calibers()
        print()
    if a.contradictions:
        mode_contradictions()
        print()
    if a.serde:
        mode_serde()
        print()
    if a.impl:
        mode_impl()


if __name__ == "__main__":
    main()
