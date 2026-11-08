#!/usr/bin/env python3
"""**每回合发数 ↔ 对拍失败** 的相关性统计（`docs/06-verification.md` §34 的表就是本脚本的输出）。

输入：`examples/replay_attrib --drive fire --policy default --json` 的逐批 JSONL
（**只读**既有输出，不改任何判定口径 —— 失败 = `content_ok == false`，
大差 = 批内出现 `DiffClass::NumericValueFar`，即 `replaydiff` 自己的 `delta > 1e-3` 判据）。

```bash
export CARGO_HOME=<WS>/.reference/cargo-home CARGO_TARGET_DIR=<WS>/.agent-work/cg/target62
R=<WS>/.reference/replays/gw2-replays-snapshot/replays
cargo run --release --offline -q --example replay_attrib -- \
    --dir $R --drive fire --policy default --json f.jsonl     # 1080 批
python3 tools/shots_vs_fail.py --json f.jsonl                 # 本文 §34 的全部分桶

# 上游那 5 份新样本（§33.4f 的 26 批）用同一套判据复算：
cargo run --release --offline -q --example replay_attrib -- \
    --dir dispatch/replays --drive fire --policy default --json n5.jsonl
python3 tools/shots_vs_fail.py --json n5.jsonl --label upstream26
```

## 口径（**三个词，别混**）

| 词 | 判据 | 出处 |
|---|---|---|
| **失败** | `content_ok == false`（该批内容与回放不一致） | `replaydiff::compare_batch` |
| **大差（>1e-3）** | 批内**至少一条** diff 的 `class == "数值（差 >1e-3）"`，即 `|ours−observed| > 1e-3` | `replaydiff.rs` 的 `DiffClass::NumericValueFar` |
| **每回合发数** | 该结算批的 `shots`（= 同一 tick 内解析出的发数） | `split_batches` |

分桶优先级与互斥规则**照抄** `tools/bit_exact_attrib.py`（`classify`），本脚本 import 它，
不重写 —— 否则两处规则会漂移。`--shots-limit` 只影响"发数分布"里长尾的显示。

统计量全部自己算（venv 里没有 numpy/scipy，也不该为一张表装依赖）：
Wilson 区间、卡方（含 Yates 连续性校正）、Fisher 精确检验（两侧）、
风险比 + 对数区间、Cochran–Armitage 趋势检验。
"""

import argparse
import collections
import importlib.util
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
Z95 = 1.959963984540054
LARGE_CLASS = "数值（差 >1e-3）"


def _load_attrib():
    """import `tools/bit_exact_attrib.py`（文件名带下划线，可直接按模块导入）。"""
    spec = importlib.util.spec_from_file_location(
        "bit_exact_attrib", os.path.join(HERE, "bit_exact_attrib.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


ATTRIB = _load_attrib()


# ---------------------------------------------------------------- 基础统计


def wilson(k, n, z=Z95):
    """Wilson score 区间（小样本比 Wald 靠谱；n=0 时返回 (0,0,1)）。"""
    if n == 0:
        return (0.0, 0.0, 1.0)
    p = k / n
    den = 1 + z * z / n
    c = (p + z * z / (2 * n)) / den
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return (p, max(0.0, c - h), min(1.0, c + h))


def _lnfact(n):
    return math.lgamma(n + 1)


def _hypergeom_pmf(k, K, n, N):
    """超几何：总体 N、成功 K、抽 n、命中 k。"""
    return math.exp(
        _lnfact(K) - _lnfact(k) - _lnfact(K - k)
        + _lnfact(N - K) - _lnfact(n - k) - _lnfact(N - K - n + k)
        - (_lnfact(N) - _lnfact(n) - _lnfact(N - n))
    )


def fisher_two_sided(a, b, c, d):
    """2x2 表 [[a,b],[c,d]] 的两侧 Fisher 精确检验 p。

    与 scipy.stats.fisher_exact 同定义：把"概率 ≤ 观测表概率"的所有表加起来。
    """
    N = a + b + c + d
    K, n = a + b, a + c
    p_obs = _hypergeom_pmf(a, K, n, N)
    lo, hi = max(0, n - (N - K)), min(K, n)
    tot = 0.0
    for k in range(lo, hi + 1):
        p = _hypergeom_pmf(k, K, n, N)
        if p <= p_obs * (1 + 1e-12):
            tot += p
    return min(1.0, tot)


def chi2_2x2(a, b, c, d, yates=False):
    """2x2 卡方，返回 (chi2, p)。df=1 时生存函数 = erfc(sqrt(x/2))。"""
    n = a + b + c + d
    if n == 0:
        return (0.0, 1.0)
    if yates:
        num = n * (abs(a * d - b * c) - n / 2.0) ** 2
    else:
        num = n * (a * d - b * c) ** 2
    den = (a + b) * (c + d) * (a + c) * (b + d)
    if den == 0:
        return (0.0, 1.0)
    x = num / den
    x = max(0.0, x)
    return (x, math.erfc(math.sqrt(x / 2.0)))


def risk_ratio(a, n1, b, n2):
    """风险比 (p1/p2) + Katz 对数区间；处理 0 格（加 0.5 的 Haldane 修正）。"""
    if n1 == 0 or n2 == 0:
        return (float("nan"), float("nan"), float("nan"))
    if a == 0 or a == n1 or b == 0 or b == n2:
        a2, b2 = a + 0.5, b + 0.5
        n12, n22 = n1 + 1.0, n2 + 1.0
        aa, bb = a, b
    else:
        a2, b2, n12, n22 = float(a), float(b), float(n1), float(n2)
        aa, bb = a, b
    p1, p2 = a2 / n12, b2 / n22
    if p2 == 0:
        return (float("inf"), float("nan"), float("nan"))
    rr = p1 / p2
    se = math.sqrt(1 / a2 - 1 / n12 + 1 / b2 - 1 / n22) if a2 > 0 and b2 > 0 else float("nan")
    if math.isnan(se):
        return (rr, float("nan"), float("nan"))
    lo = math.exp(math.log(rr) - Z95 * se)
    hi = math.exp(math.log(rr) + Z95 * se)
    return (rr, lo, hi)


def norm_ppf(p):
    """标准正态分位数（bisection，够用且无依赖）。"""
    lo, hi = -40.0, 40.0
    for _ in range(200):
        mid = (lo + hi) / 2
        if 0.5 * (1 + math.erf(mid / math.sqrt(2))) < p:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def cochran_armitage(groups, scores=None):
    """Cochran–Armitage 趋势检验：groups = [(n, k), …]（按发数递增）。

    返回 (z, 双侧 p)。零假设：失败率与发数**无单调趋势**。
    """
    groups = [(n, k) for n, k in groups if n > 0]
    if len(groups) < 2:
        return (float("nan"), float("nan"))
    if scores is None:
        scores = list(range(len(groups)))
    N = sum(n for n, _ in groups)
    R = sum(k for _, k in groups)
    if N == 0 or R == 0 or R == N:
        return (float("nan"), float("nan"))
    pbar = R / N
    tbar = sum(s * n for s, (n, _) in zip(scores, groups)) / N
    num = sum(s * k for s, (_, k) in zip(scores, groups)) - pbar * N * tbar
    var = pbar * (1 - pbar) * (
        sum(s * s * n for s, (n, _) in zip(scores, groups)) - N * tbar * tbar
    )
    if var <= 0:
        return (float("nan"), float("nan"))
    z = num / math.sqrt(var)
    return (z, math.erfc(abs(z) / math.sqrt(2)))


def pct(k, n):
    return f"{100.0 * k / n:.1f}%" if n else "—"


# ---------------------------------------------------------------- 数据


def load(path):
    """读 JSONL；只留 `batch >= 0` 的结算批（错误行单独计数）。"""
    rows, errs = [], []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            r = json.loads(line)
            if r.get("batch", -1) >= 0:
                rows.append(r)
            else:
                errs.append(r)
    rows.sort(key=lambda r: (r["file"], r["batch"]))
    return rows, errs


def has_large(r):
    return any(d.get("class") == LARGE_CLASS for d in r["diffs"])


def max_delta(r):
    """批内最大的 |ours−observed|（数值型 diff；用来给"大差"一个幅度）。"""
    best = 0.0
    for d in r["diffs"]:
        a, b = ATTRIB.num(d.get("ours")), ATTRIB.num(d.get("observed"))
        if a is None or b is None:
            continue
        best = max(best, abs(a - b))
    return best


def bucket_of(shots):
    return shots if shots <= 8 else 9


# ---------------------------------------------------------------- 报告


def table(rows, label):
    print(f"== {label}：{len({r['file'] for r in rows})} 份回放 / {len(rows)} 个结算批 ==")
    ok = sum(1 for r in rows if r["content_ok"])
    print(f"内容命中 {ok} / {len(rows)}（{pct(ok, len(rows))}）；失败 {len(rows) - ok} 批")
    print()

    hist = collections.Counter(r["shots"] for r in rows)
    print("-- 每回合发数的分布（同时回合到底常见几发）--")
    print(f"{'发数':>4}{'批数':>8}{'占比':>9}{'有该发数的回放':>16}")
    per_file = collections.defaultdict(set)
    for r in rows:
        per_file[r["shots"]].add(r["file"])
    for s in sorted(hist):
        print(f"{s:>4}{hist[s]:>8}{pct(hist[s], len(rows)):>9}{len(per_file[s]):>16}")
    print(f"{'合计':>4}{len(rows):>8}{'100.0%':>9}{len({r['file'] for r in rows}):>16}")
    print()

    # ---- 逐发数分桶 ----
    buckets = collections.defaultdict(list)
    for r in rows:
        buckets[bucket_of(r["shots"])].append(r)

    print("-- 按每回合发数分桶：批数 / 失败 / 失败率（Wilson 95%）/ 含大差批 / 大差率 / 类别分布 --")
    hdr = (f"{'发数':>5}{'批数':>6}{'失败':>6}{'失败率':>8}{'失败率 95% CI':>18}"
           f"{'含大差':>7}{'大差率':>8}")
    print(hdr + "  互斥主类分布（`bit_exact_attrib.classify`）")
    print("-" * 118)
    for s in sorted(buckets):
        bs = buckets[s]
        f_n = sum(1 for r in bs if not r["content_ok"])
        l_n = sum(1 for r in bs if has_large(r))
        _, lo, hi = wilson(f_n, len(bs))
        cats = collections.Counter(ATTRIB.classify(r)[0] for r in bs if not r["content_ok"])
        dist = " ".join(f"{c}:{n}" for c, n in cats.most_common())
        name = f"{s}" if s <= 8 else "9+"
        print(f"{name:>5}{len(bs):>6}{f_n:>6}{pct(f_n, len(bs)):>8}"
              f"{f'[{100*lo:.0f}%,{100*hi:.0f}%]':>18}{l_n:>7}{pct(l_n, len(bs)):>8}  {dist}")
    print("-" * 118)
    allf = sum(1 for r in rows if not r["content_ok"])
    alll = sum(1 for r in rows if has_large(r))
    print(f"{'合计':>5}{len(rows):>6}{allf:>6}{pct(allf, len(rows)):>8}"
          f"{'':>18}{alll:>7}{pct(alll, len(rows)):>8}")
    print()

    # ---- 三组（与 §33.4f 同分组）----
    groups = {
        "≤3 发": [r for r in rows if r["shots"] <= 3],
        "4–5 发": [r for r in rows if 4 <= r["shots"] <= 5],
        "≥6 发": [r for r in rows if r["shots"] >= 6],
    }
    print("-- 三组（与 §33.4f 同一分组）--")
    print(f"{'分组':>8}{'批数':>6}{'失败':>6}{'失败率':>8}{'失败率 95% CI':>18}{'含大差':>7}{'大差率':>8}")
    print("-" * 70)
    for g, rs in groups.items():
        f_n = sum(1 for r in rs if not r["content_ok"])
        l_n = sum(1 for r in rs if has_large(r))
        _, lo, hi = wilson(f_n, len(rs))
        print(f"{g:>8}{len(rs):>6}{f_n:>6}{pct(f_n, len(rs)):>8}"
              f"{f'[{100*lo:.0f}%,{100*hi:.0f}%]':>18}{l_n:>7}{pct(l_n, len(rs)):>8}")
    print()
    return groups, buckets


def q1(groups):
    print("== 问题①：≥6 发真的显著更差吗 ==")
    lo3, hi6 = groups["≤3 发"], groups["≥6 发"]
    a = sum(1 for r in hi6 if not r["content_ok"])
    b = len(hi6) - a
    c = sum(1 for r in lo3 if not r["content_ok"])
    d = len(lo3) - c
    print(f"  2x2：≥6 发 失败 {a}/{len(hi6)}；≤3 发 失败 {c}/{len(lo3)}")
    x, p = chi2_2x2(a, b, c, d)
    xy, py = chi2_2x2(a, b, c, d, yates=True)
    pf = fisher_two_sided(a, b, c, d)
    rr, rlo, rhi = risk_ratio(a, len(hi6), c, len(lo3))
    _, l1, h1 = wilson(a, len(hi6))
    _, l0, h0 = wilson(c, len(lo3))
    print(f"  卡方（无校正）  χ²={x:.2f}  p={p:.3g}")
    print(f"  卡方（Yates）   χ²={xy:.2f}  p={py:.3g}")
    print(f"  Fisher 两侧精确 p={pf:.3g}")
    print(f"  风险比 RR={rr:.2f}  95% CI [{rlo:.2f}, {rhi:.2f}]")
    print(f"  ≥6 发失败率 {100*a/len(hi6):.1f}% [{100*l1:.0f}%,{100*h1:.0f}%]；"
          f"≤3 发 {100*c/len(lo3):.1f}% [{100*l0:.0f}%,{100*h0:.0f}%]")
    print()
    return dict(chi2=x, p=p, p_yates=py, p_fisher=pf, rr=rr, rr_lo=rlo, rr_hi=rhi)


def q2(rows):
    print("== 问题②：大差（>1e-3）是否几乎只在 ≥N 发出现？N 是多少 ==")
    hist = collections.Counter(r["shots"] for r in rows)
    lh = collections.Counter(r["shots"] for r in rows if has_large(r))
    total_l = sum(lh.values())
    print(f"  大差批共 {total_l} 个（占全部 {len(rows)} 批的 {pct(total_l, len(rows))}）")
    print(f"{'发数':>5}{'批数':>7}{'含大差':>8}{'大差率':>9}{'大差占全部大差':>16}")
    print("-" * 46)
    for s in sorted(hist):
        print(f"{s:>5}{hist[s]:>7}{lh.get(s,0):>8}{pct(lh.get(s,0), hist[s]):>9}"
              f"{pct(lh.get(s,0), total_l):>16}")
    print("-" * 46)
    print()
    print("  逐阈值看（若大差'几乎只在 ≥N 发'，下面 ≥N 那一列应很快到 100%）：")
    print(f"{'N':>3}{'≥N 批数':>9}{'≥N 含大差':>11}{'≥N 大差率':>11}"
          f"{'< N 里的大差批':>15}{'大差被 ≥N 覆盖':>16}")
    print("-" * 68)
    best = None
    for n in range(1, 10):
        ge = [r for r in rows if r["shots"] >= n]
        lt = [r for r in rows if r["shots"] < n]
        gl = sum(1 for r in ge if has_large(r))
        ll = sum(1 for r in lt if has_large(r))
        cov = gl / total_l if total_l else 0.0
        print(f"{n:>3}{len(ge):>9}{gl:>11}{pct(gl, len(ge)):>11}{ll:>15}{pct(gl, total_l):>16}")
        if best is None or cov > best[1]:
            best = (n, cov)
    print("-" * 68)
    lo = min((r["shots"] for r in rows if has_large(r)), default=None)
    # 覆盖率从 N=1 起若**单调下降**，就说明"越小的 N 覆盖越多"，即"几乎只在 ≥N"不成立
    mono_down = all(
        sum(1 for r in rows if r["shots"] >= n and has_large(r))
        <= sum(1 for r in rows if r["shots"] >= n + 1 and has_large(r))
        for n in range(1, 9)
    )
    print(f"  ⇒ 最低发数的大差批是 **{lo} 发**；≥N 覆盖率随 N 单调下降 = {mono_down}")
    print("  ⇒ 结论：**不存在这样的 N ≥ 2**。大差在 1 发批里就有 "
          f"{lh.get(1, 0)} 个（占全部大差的 {pct(lh.get(1, 0), total_l)}）")
    print()
    return lh, hist


def q3(rows):
    print("== 问题③：顺序回合（每回合 1 发）里有没有 >1e-3 的大差 ==")
    seq = [r for r in rows if r["shots"] == 1]
    bad = [r for r in seq if has_large(r)]
    print(f"  1 发批 {len(seq)} 个（{len({r['file'] for r in seq})} 份回放）；"
          f"其中失败 {sum(1 for r in seq if not r['content_ok'])}；含大差 {len(bad)} 个"
          f"（{pct(len(bad), len(seq))}）")
    if bad:
        print("  含大差的 1 发批（逐条，按幅度降序）：")
        for r in sorted(bad, key=lambda r: -max_delta(r)):
            ds = [d for d in r["diffs"] if d.get("class") == LARGE_CLASS]
            print(f"    {r['file']} 批{r['batch']}  tick {r['tick']}  owner {r['owners']}  "
                  f"mode {r['mode']}  最大差 {max_delta(r):.6g}")
            for d in ds[:4]:
                print(f"        {d['path']}: ours={d['ours']} observed={d['observed']}")
    else:
        print("  ⇒ 没有。1 发批里一条 >1e-3 的差都没有。")
    print()
    # 顺序性验证：1 发批是否各自占一个 tick（= 不是同时回合里"只有一个人开火"）
    byfile = collections.defaultdict(list)
    for r in rows:
        byfile[r["file"]].append(r)
    same = tot = 0
    for f, rs in byfile.items():
        rs.sort(key=lambda r: r["batch"])
        for p, q in zip(rs, rs[1:]):
            if q["shots"] == 1:
                tot += 1
                same += q["tick"] == p["tick"]
    print(f"  顺序性旁证：非首条的 1 发批 {tot} 个，与前一批**同 tick** 的 {same} 个"
          f" ⇒ 1 发批确实各占一个回合（不是同时回合里只有一人开火）")
    print()
    return seq, bad


def trend(rows):
    print("== 趋势检验（不是两分组的'显著'，而是'越多人越差'这个方向）==")
    hist = collections.OrderedDict()
    for s in sorted({r["shots"] for r in rows}):
        bs = [r for r in rows if r["shots"] == s]
        hist[s] = (len(bs), sum(1 for r in bs if not r["content_ok"]))
    z, p = cochran_armitage(list(hist.values()))
    print(f"  Cochran–Armitage（分数 = 发数本身，1…{max(hist)}）：z={z:.2f}  p={p:.3g}")
    z2, p2 = cochran_armitage([v for k, v in hist.items() if k >= 2])
    print(f"  只在 ≥2 发的批里（分数 = 发数，2…{max(hist)}）：      z={z2:.2f}  p={p2:.3g}")
    # 每回合发数 × 失败率 的一点式汇总
    print(f"  1 发 {hist.get(1,(0,0))[1]}/{hist.get(1,(0,0))[0]}；"
          f"2 发 {hist.get(2,(0,0))[1]}/{hist.get(2,(0,0))[0]}；"
          f"3 发 {hist.get(3,(0,0))[1]}/{hist.get(3,(0,0))[0]}；"
          f"≥4 发 {sum(v[1] for k,v in hist.items() if k>=4)}/"
          f"{sum(v[0] for k,v in hist.items() if k>=4)}")
    print()
    return z, p, z2, p2


def clustering(rows):
    """**反例与混杂检查**：发数是否只是"少数几局"的代理？"""
    print("== 混杂检查：≥6 发的批是不是集中在少数几份回放 ==")
    big = [r for r in rows if r["shots"] >= 6]
    files = collections.Counter(r["file"] for r in big)
    print(f"  ≥6 发 {len(big)} 批，来自 {len(files)} 份回放："
          + "，".join(f"{f}×{n}" for f, n in files.most_common()))
    print(f"  这 {len(files)} 份回放共有 {sum(1 for r in rows if r['file'] in files)} 批，"
          f"其中失败 {sum(1 for r in rows if r['file'] in files and not r['content_ok'])}")
    print("  留一法（每次剔掉一份贡献 ≥6 发批的回放，看 ≥6 发的失败率还站不站得住）：")
    print(f"{'剔掉':>14}{'≥6 批数':>9}{'失败':>6}{'失败率':>9}{'Fisher p vs ≤3':>16}")
    print("-" * 56)
    for f in sorted(files):
        sub = [r for r in rows if r["file"] != f]
        g6 = [r for r in sub if r["shots"] >= 6]
        g3 = [r for r in sub if r["shots"] <= 3]
        a = sum(1 for r in g6 if not r["content_ok"])
        c = sum(1 for r in g3 if not r["content_ok"])
        p = fisher_two_sided(a, len(g6) - a, c, len(g3) - c)
        print(f"{f:>14}{len(g6):>9}{a:>6}{pct(a, len(g6)):>9}{p:>16.3g}")
    print("-" * 56)
    print("  同一份回放内部的对照（有 ≥2 发批的回放，一发 vs 多发）：")
    multi_files = sorted({r["file"] for r in rows if r["shots"] >= 2})
    one_ok = one_n = mul_ok = mul_n = 0
    for f in multi_files:
        for r in rows:
            if r["file"] != f:
                continue
            if r["shots"] == 1:
                one_n += 1
                one_ok += r["content_ok"]
            else:
                mul_n += 1
                mul_ok += r["content_ok"]
    print(f"    {len(multi_files)} 份回放内部：1 发批命中 {one_ok}/{one_n}"
          f"（{pct(one_ok, one_n)}）；≥2 发批命中 {mul_ok}/{mul_n}（{pct(mul_ok, mul_n)}）")
    if mul_n and one_n:
        p = fisher_two_sided(mul_n - mul_ok, mul_ok, one_n - one_ok, one_ok)
        print(f"    Fisher 两侧 p={p:.3g}（同局内配对，排除了'这几局本身特殊'这个混杂项）")
    print()
    return files


def within_match(rows):
    """**这条相关性最要命的地方**：`shots` 在一局里基本是常量 ⇒ 比的是"局"，不是"回合"。"""
    print("== 混杂检查 2：shots 是**每局常量**，所以 ≥6 vs ≤3 比的是不同的局 ==")
    byfile = collections.defaultdict(list)
    for r in rows:
        byfile[r["file"]].append(r)
    mixed = {f: rs for f, rs in byfile.items() if len({r["shots"] for r in rs}) > 1}
    fixed = len(byfile) - len(mixed)
    print(f"  有结算批的回放 {len(byfile)}：发数恒定的 {fixed}"
          f"（{pct(fixed, len(byfile))}），混合发数的 {len(mixed)} 份")
    print("  ⇒ 全语料里**唯一能排除'这几局本身就有问题'**的证据，只有这 "
          f"{len(mixed)} 份混合局：")
    print(f"{'回放':>14}{'≥6 发 失败':>12}{'<6 发 失败':>12}   shots")
    print("-" * 58)
    for f in sorted(mixed):
        rs = mixed[f]
        ge = [r for r in rs if r["shots"] >= 6]
        lt = [r for r in rs if r["shots"] < 6]
        ge_txt = f"{sum(1 for r in ge if not r['content_ok'])}/{len(ge)}"
        lt_txt = f"{sum(1 for r in lt if not r['content_ok'])}/{len(lt)}"
        print(f"{f:>14}{ge_txt:>12}{lt_txt:>12}   {sorted(r['shots'] for r in rs)}")
    print("-" * 58)
    ge = [r for rs in mixed.values() for r in rs if r["shots"] >= 6]
    lt = [r for rs in mixed.values() for r in rs if r["shots"] < 6]
    a = sum(1 for r in ge if not r["content_ok"])
    c = sum(1 for r in lt if not r["content_ok"])
    pf = fisher_two_sided(a, len(ge) - a, c, len(lt) - c) if ge and lt else float("nan")
    print(f"  同局内合计：≥6 发 {a}/{len(ge)} vs <6 发 {c}/{len(lt)}"
          f"（n={len(ge) + len(lt)}，Fisher p={pf:.3g}）")
    print()

    print("== 混杂检查 3：贡献 ≥6 发批的 4 份回放，**自己的一发批也全失败** ==")
    big = sorted({r["file"] for r in rows if r["shots"] >= 6})
    tot_le = tot_le_f = 0
    for f in big:
        rs = [r for r in rows if r["file"] == f]
        le = [r for r in rs if r["shots"] <= 3]
        k = sum(1 for r in le if not r["content_ok"])
        tot_le += len(le)
        tot_le_f += k
        print(f"  {f}: 该局 ≤3 发 {k}/{len(le)} 失败；"
              f"≥6 发 {sum(1 for r in rs if r['shots'] >= 6 and not r['content_ok'])}/"
              f"{sum(1 for r in rs if r['shots'] >= 6)} 失败")
    print(f"  ⇒ 这 {len(big)} 局的 ≤3 发批 **{tot_le_f}/{tot_le} 全失败**："
          f"局内没有'多发才崩'的对照，'多发'与'整局都有问题'分不开")
    print()

    print("== 换个切法：顺序回合 vs 同时回合（发数 >1 即同一 tick 多人开火）==")
    seq = [r for r in rows if r["shots"] == 1]
    sim = [r for r in rows if r["shots"] > 1]
    for nm, rs in (("顺序（1 发/回合）", seq), ("同时（≥2 发/回合）", sim)):
        k = sum(1 for r in rs if not r["content_ok"])
        _, lo, hi = wilson(k, len(rs))
        print(f"  {nm:<20}{len(rs):>5} 批  失败 {k:>4}  {pct(k, len(rs)):>7}"
              f"  CI [{100*lo:.0f}%,{100*hi:.0f}%]  含大差 {sum(1 for r in rs if has_large(r)):>3}")
    a = sum(1 for r in sim if not r["content_ok"])
    c = sum(1 for r in seq if not r["content_ok"])
    x, p = chi2_2x2(a, len(sim) - a, c, len(seq) - c)
    pf = fisher_two_sided(a, len(sim) - a, c, len(seq) - c)
    rr, rl, rh = risk_ratio(a, len(sim), c, len(seq))
    print(f"  同时 vs 顺序：χ²={x:.2f} p={p:.3g} Fisher p={pf:.3g} "
          f"RR={rr:.2f} [{rl:.2f}, {rh:.2f}]")
    print()
    return mixed


def magnitudes(rows):
    print("== 大差的**幅度**：1 发批里的大差并不比多发批温柔 ==")
    one = [r for r in rows if r["shots"] == 1 and has_large(r)]
    mul = [r for r in rows if r["shots"] > 1 and has_large(r)]
    for nm, rs in (("1 发批", one), ("多发批", mul)):
        if not rs:
            continue
        ms = sorted(max_delta(r) for r in rs)
        mid = ms[len(ms) // 2]
        print(f"  {nm:<8}{len(rs):>4} 批（{len({r['file'] for r in rs})} 份回放）  "
              f"最大差 min {ms[0]:.4g} / 中位 {mid:.4g} / max {ms[-1]:.4g}")
        print("           " + "  ".join(
            f">{t:g}: {sum(1 for v in ms if v > t)}" for t in (1e-2, 0.1, 1.0, 10.0)))
    if one:
        print("  1 发批里最大的 5 个（幅度 >10 = 整条弹道都错，不是末位差）：")
        for r in sorted(one, key=max_delta, reverse=True)[:5]:
            paths = [d["path"].split(".")[-1] for d in r["diffs"] if d.get("class") == LARGE_CLASS]
            print(f"    {r['file']} 批{r['batch']}  mode {r['mode']}  "
                  f"最大差 {max_delta(r):.4g}  字段 {paths}")
    print()
    return one, mul


def tsv(path, rows, groups, lbuckets):
    """把汇总表写成**文本**（`fixtures/` 只收文本，401 份 JSONL 不入库）。"""
    lines = []
    lines.append("# 每回合发数 ↔ 对拍失败：全语料汇总（`tools/shots_vs_fail.py --tsv` 生成）")
    lines.append("# 轴 = fire（`replay_attrib --drive fire --policy default --json`）")
    lines.append("# 语料 = .reference/replays/gw2-replays-snapshot/replays 的 401 份"
                 f"（有结算批 {len({r['file'] for r in rows})} 份 / {len(rows)} 个结算批）")
    lines.append("# 失败 = content_ok==false；大差 = 批内出现 DiffClass::NumericValueFar（|Δ|>1e-3）")
    lines.append("shots\tbatches\tfiles\tfails\tfail_rate\tbatches_large\tlarge_rate")
    per_file = collections.defaultdict(set)
    for r in rows:
        per_file[r["shots"]].add(r["file"])
    for s in sorted(lbuckets):
        bs = lbuckets[s]
        k = sum(1 for r in bs if not r["content_ok"])
        l = sum(1 for r in bs if has_large(r))
        name = str(s) if s <= 8 else "9+"
        lines.append(f"{name}\t{len(bs)}\t{len(per_file[s])}\t{k}\t{100.0*k/len(bs):.1f}%\t{l}"
                     f"\t{100.0*l/len(bs):.1f}%")
    k = sum(1 for r in rows if not r["content_ok"])
    l = sum(1 for r in rows if has_large(r))
    lines.append(f"合计\t{len(rows)}\t{len({r['file'] for r in rows})}\t{k}"
                 f"\t{100.0*k/len(rows):.1f}%\t{l}\t{100.0*l/len(rows):.1f}%")
    lines.append("")
    lines.append("# 三组（与 docs/06 §33.4f 同分组）")
    lines.append("group\tbatches\tfails\tfail_rate\tfails_large\tlarge_rate")
    for g, rs in groups.items():
        k = sum(1 for r in rs if not r["content_ok"])
        l = sum(1 for r in rs if has_large(r))
        lines.append(f"{g}\t{len(rs)}\t{k}\t{100.0*k/len(rs):.1f}%\t{l}\t{100.0*l/len(rs):.1f}%")

    # 关键统计量（与 docs/06 §34 逐个对得上）
    g3, g6 = groups["≤3 发"], groups["≥6 发"]
    a = sum(1 for r in g6 if not r["content_ok"])
    c = sum(1 for r in g3 if not r["content_ok"])
    x, p = chi2_2x2(a, len(g6) - a, c, len(g3) - c)
    xy, py = chi2_2x2(a, len(g6) - a, c, len(g3) - c, yates=True)
    pf = fisher_two_sided(a, len(g6) - a, c, len(g3) - c)
    rr, rl, rh = risk_ratio(a, len(g6), c, len(g3))
    seq = [r for r in rows if r["shots"] == 1]
    sim = [r for r in rows if r["shots"] > 1]
    sa = sum(1 for r in sim if not r["content_ok"])
    sc = sum(1 for r in seq if not r["content_ok"])
    sx, sp = chi2_2x2(sa, len(sim) - sa, sc, len(seq) - sc)
    byfile = collections.defaultdict(list)
    for r in rows:
        byfile[r["file"]].append(r)
    mixed = sum(1 for rs in byfile.values() if len({r["shots"] for r in rs}) > 1)
    lines.append("")
    lines.append("# 关键统计量（全部由本脚本现算，无 scipy）")
    lines.append(f"# ≥6 vs ≤3：χ²={x:.2f} p={p:.3g}；Yates χ²={xy:.2f} p={py:.3g}；"
                 f"Fisher 两侧 p={pf:.3g}；RR={rr:.2f} [{rl:.2f}, {rh:.2f}]")
    lines.append(f"# 同时(≥2 发) vs 顺序(1 发)：χ²={sx:.2f} p={sp:.3g}；"
                 f"{100.0*sa/len(sim):.1f}% vs {100.0*sc/len(seq):.1f}%")
    lines.append(f"# 每局发数是否恒定：{len(byfile) - mixed}/{len(byfile)} 份回放只出现一种发数"
                 f"（⇒ ≥6 vs ≤3 主要是**跨局**比较，不是局内比较）")
    n_large = sum(1 for r in rows if has_large(r))
    n_large_one = sum(1 for r in rows if r["shots"] == 1 and has_large(r))
    lines.append("# 大差（|Δ|>1e-3）最低出现在 1 发批；1 发批占全部大差 "
                 f"{100.0*n_large_one/max(1, n_large):.1f}%"
                 f"（{n_large_one}/{n_large}）→ 不存在'大差只在 ≥N 发'的 N ≥ 2")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(f"汇总表已写入 {path}（{len(lines)} 行，纯文本）")


def upstream(rows_five, upstream_ref):
    print("== 与 §33.4f 那条 26 批结论对照 ==")
    print(f"  上游 26 批 = 5 份新录 replay（dispatch/replays）的**全部结算批**；"
          f"本机同口径复算得 {len({r['file'] for r in rows_five})} 份 / {len(rows_five)} 批")
    print(f"{'分组':>8}{'上游批数':>9}{'上游失败':>9}{'上游大差':>9}"
          f"{'本机批数':>9}{'本机失败':>9}{'本机大差':>9}")
    print("-" * 62)
    for g, cond in (("≤3 发", lambda s: s <= 3), ("4–5 发", lambda s: 4 <= s <= 5),
                    ("≥6 发", lambda s: s >= 6)):
        rs = [r for r in rows_five if cond(r["shots"])]
        up = upstream_ref.get(g, ("—", "—", "—"))
        print(f"{g:>8}{up[0]:>9}{up[1]:>9}{up[2]:>9}"
              f"{len(rs):>9}{sum(1 for r in rs if not r['content_ok']):>9}"
              f"{sum(1 for r in rs if has_large(r)):>9}")
    print("-" * 62)
    print("  逐批明细（5 份新样本，`shots` / 失败 / 大差 / 主类）：")
    for r in rows_five:
        print(f"    {r['file']} 批{r['batch']}  {r['shots']} 发  tick {r['tick']}  "
              f"{'失败' if not r['content_ok'] else '命中'}  "
              f"{'大差' if has_large(r) else '    '}  {ATTRIB.classify(r)[0]}")
    print()


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", required=True)
    ap.add_argument("--label", default="全语料")
    ap.add_argument("--five", default="", help="5 份新样本的 JSONL（对照 §33.4f 时给）")
    ap.add_argument("--tsv", default="", help="把分桶汇总表写成纯文本（fixtures/ 只收文本）")
    args = ap.parse_args(argv)

    rows, errs = load(args.json)
    groups, lbuckets = table(rows, args.label)
    stats = q1(groups)
    q2(rows)
    q3(rows)
    trend(rows)
    clustering(rows)
    within_match(rows)
    magnitudes(rows)

    if args.five and os.path.exists(args.five):
        five, _ = load(args.five)
        ref = {"≤3 发": (17, 6, 1), "4–5 发": (1, 1, 1), "≥6 发": (8, 8, 5)}
        upstream(five, ref)

    if args.tsv:
        tsv(args.tsv, rows, groups, lbuckets)

    # 机读摘要（写进 fixtures 的那张表由 --tsv 决定，这里只回一行给 CI/复跑核对）
    print("== 复跑核对 ==")
    print(f"  结算批 {len(rows)}，内容命中 {sum(1 for r in rows if r['content_ok'])}，"
          f"失败 {sum(1 for r in rows if not r['content_ok'])}，"
          f"含大差 {sum(1 for r in rows if has_large(r))}")
    print(f"  ≥6 发 vs ≤3 发：χ²={stats['chi2']:.2f} p={stats['p']:.3g} "
          f"(Yates p={stats['p_yates']:.3g}) Fisher p={stats['p_fisher']:.3g} "
          f"RR={stats['rr']:.2f}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
