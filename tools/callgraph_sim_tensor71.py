#!/usr/bin/env python3
"""callgraph_sim_tensor71.py — **物化一次相似度张量 + N 组权重批加权**（任务 71）。

## 为什么有这个东西（63 号自己写的诊断，不是我的推测）

`tools/callgraph_gpu_sim.py`（61 号）是一个 **top-K 检索器**：整份候选池物化一次、
逐行吐排名，**但它不吐"每个特征的相似度"**。而"多组权重"要的恰恰是
**同一批相似度的加权和** —— 相似度与权重无关，所以只该算一次。
63 号（`docs/02` §44.6）的做法是"每换一组权重就把打分器从头跑一遍"，接口形状决定了
GPU 帮不上；它明确写「这个设计我没做，也未实测」。

本工具就是那个设计：

    ① 逐特征相似度 S[f] 物化一次（float64，按块，CSR/CSC 稀疏，不落盘）
    ② score[c] = Σ_f w[c,f]·S[f] / Σ_f w[c,f]·A[f]   —— A 是"该特征在这个格子上有值"的掩码
    ③ 于是 318 组 / 3,000 组权重 = **同一份张量的 N 次广播乘加**，不是 N 次重算

## 逐位复刻的前提（这是能不能用的关键，不是装饰）

`CF.score_same` 里的 `add()` 是**按固定顺序**做的 `tot += w*v; wt += w`。浮点加法不满足
结合律，换顺序就会在 1e-16 量级分叉、可能翻转 top-1。所以：

* 累加**严格按** `score_same` 里 `add()` 的调用顺序：size→hist→consts→callee→caller→
  strs_s→strs→ncal→ncall→cbsig→rbsig→gram→shape→scal→scal_c→wconst→stok→cls；
* 权重 0 的特征在 CPU 侧被 `if weight` 跳过，本工具**照样参与累加**（`tot + 0.0*V == tot`，
  对有限值是精确恒等），所以**同一份张量能同时给出任意权重子集的结果且逐位等于 CPU**；
* 分母 `wt` 也按同一顺序累加，且**只加"有值"的那一项**（= 掩码 A[f]）；
* `multiset_sim` 的 Σ 按 rank 升序逐项累加、`cos_hist` 的模长在 **CPU 侧用 Python 精确算好**
  再送进去，避免 GPU 的规约顺序改动结果。

## 特征（24 个，超出任务书要求的 16–18）

`size` 连续；`consts/gram/shape/scal/scal_c/wconst/stok/strs_s/strs/cbsig/rbsig/cgram_a`
多重集 one-hot 内积（Jaccard）；`caller/callee/caller_a/callee_a` 按 rank 配对的多重集相似度；
`ncal/ncall/ncal_a/ncall_a/icall` 计数的 log 比；`hist/cls` 加权 one-hot 余弦。

`cgram_a` 是**二阶编码**（任务 70 §45.9 第 3 条指的真空白）：把「被调者的**尺寸**多重集」
换成「被调者（直接边 ∪ 指针边）的**助记符三元组并集**」，Jaccard 比较。

`strs` 是泄漏列（`panic::Location` 的 file 投影），永远权重 0，只在 LOO 表里当对照。

## 子命令

    selfcheck  张量与 `CF.score_same` 逐位对拍（含 ULP 直方图）+ top-K 与 `CF.rank` 逐条对拍
    sweep      N 组配置 × 人群 × 半：一次物化 + 批加权；`--mode old` 是同口径的"每组重算"
    loo        边际贡献表（从某个基线里逐个拿掉一个特征）
    pop        把正对照人群（975 / 348 / 目标集合）落成文本夹具

## 用法（GPU 必须走 `cuda_run`，bash 沙箱里看不到 /dev/nvidia*）

    export CFGFP_CACHE=<v6 缓存>
    $PY tools/callgraph_sim_tensor71.py selfcheck --backend cpu
    <gpu-python> tools/callgraph_sim_tensor71.py sweep --backend gpu --configs-file cfgs.tsv \
        --pop v2 --out hits.tsv --report r.json
"""

import argparse
import bisect
import collections
import itertools
import json
import math
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))


def _preboot(argv):
    """`--tools` / `--cache` / `--reference-root` 必须在 import CF **之前**生效（同 61 号）。"""
    p = argparse.ArgumentParser(add_help=False)
    p.add_argument("--tools", default="")
    p.add_argument("--reference-root", default="")
    p.add_argument("--cache", default="")
    a, _ = p.parse_known_args(argv)
    tools = a.tools or HERE
    if tools not in sys.path:
        sys.path.insert(0, tools)
    if a.cache and not os.environ.get("CFGFP_CACHE"):
        os.environ["CFGFP_CACHE"] = os.path.abspath(a.cache)
    if a.reference_root:
        R = os.path.abspath(a.reference_root)
        for k, rel in (("GW2_LINUX_SO", ".reference/steam/2026-08-28/libgraphwarrust.so"),
                       ("GW2_ANDROID_SO", ".reference/steam/android-2.0.60/libgraphwarrust.so"),
                       ("GW2_WIN_DLL", ".reference/windows/graphwarrust-2.0.60.dll"),
                       ("GW2_WIN_OLD_DLL", ".reference/windows/graphwarrust-early2026.dll")):
            if not os.environ.get(k) and os.path.exists(os.path.join(R, rel)):
                os.environ[k] = os.path.join(R, rel)


_preboot(sys.argv[1:])

import callgraph_fingerprint as CF              # noqa: E402

FIXTURES = os.path.join(CF.ROOT, "fixtures")
POP_FIXTURE = os.path.join(FIXTURES, "fingerprint_holdout72_samples.tsv")
SIZE_EPS = CF.SIZE_EPS
WIN = CF.WIN_SAME

# ===========================================================================
# 1. 特征登记表
# ===========================================================================
# `order` 是**累加顺序**：前 18 个必须与 `CF.score_same` 里 add() 的调用顺序逐字对应
# （逐位复刻的依据）；后面 6 个是本轮新加的（`_a` 二阶边 + `icall`），CPU 侧没有对应物，
# 排在哪不影响与 CPU 的一致性（它们只在 CPU 权重为 0 时才与 CPU 可比）。
ORDER = ("size", "hist", "consts", "callee", "caller", "strs_s", "strs", "ncal", "ncall",
         "cbsig", "rbsig", "gram", "shape", "scal", "scal_c", "wconst", "stok", "cls",
         # --- 以下是本轮新增（`score_same` 里没有；顺序是我们自己定的）---
         "callee_a", "caller_a", "ncal_a", "ncall_a", "cgram_a", "icall")

# 集合段：key → 取哪个字段（集合语义，Jaccard = |A∩B| / |A∪B|）
SETS = dict(consts="consts", gram="grams", shape="shgrams", scal="scal", scal_c="scal_c",
            wconst="wconst", stok="stok", strs_s="strs_s_set", strs="strs_set",
            cbsig="callee_bsig", rbsig="caller_bsig", cgram_a="@cgram_a")
# 多重集段：按 rank 配对
MSETS = dict(callee="callee_sizes", caller="caller_sizes",
             callee_a="callee_sizes_a", caller_a="caller_sizes_a")
# 计数段：两个整数之间的 `sim_log`
COUNTS = dict(ncal="callees", ncall="callers", ncal_a="callees_a", ncall_a="callers_a",
              icall="icalls")
# 直方图段：加权 one-hot 的余弦（模长在 CPU 侧算）
HISTS = dict(hist="hist", cls="cls")

# 每个特征进不进**分母**（= 这一格有没有值）。这是 `score_same` 里 `add()` 的 `None` 规则：
#   "always"  = 打不出值也返回 0.0（cos_hist / multiset_sim / jaccard_sets）⇒ 分母恒加
#   "both"    = 两侧都非空才算（jaccard 系列）⇒ 分母按格掩码
#   "either"  = 只要有一侧非空就算（ncal/ncall 的 `if (qf or df)`）
COND = dict(size="always", hist="always", consts="both", callee="always", caller="always",
            strs_s="both", strs="both", ncal="either", ncall="either",
            cbsig="always", rbsig="always", gram="both", shape="both", scal="both",
            scal_c="both", wconst="both", stok="both", cls="always",
            callee_a="always", caller_a="always", ncal_a="either", ncall_a="either",
            cgram_a="both", icall="either")

CORE = ("size", "gram", "consts", "caller", "scal")     # REF 的 5 个特征（§43/§44）
ALL_FEATS = tuple(ORDER)


def parse_feats(spec):
    if not spec or spec == "all":
        return list(ALL_FEATS)
    if spec == "core":
        return list(CORE)
    out = [x.strip() for x in spec.split(",") if x.strip()]
    for k in out:
        if k not in COND:
            raise SystemExit("未知特征 %r（可用：%s）" % (k, ",".join(ALL_FEATS)))
    return out


# ===========================================================================
# 2. 人群：正对照 975 / 旧 348 / 目标集合
# ===========================================================================
def read_tsv(path):
    rows, hdr = [], None
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("#") or not line.strip():
                continue
            p = line.rstrip("\n").split("\t")
            if hdr is None:
                hdr = p
                continue
            rows.append(dict(zip(hdr, p)))
    return rows


def population(which):
    """返回 (name, {linux_addr: win_addr|None}, 排序后的行列表)。

    * `v2`     = 任务 72 的正对照 975（读夹具，975 行逐条；判据只看 Location/符号名）
    * `old348` = 旧口径：v2 里 `class==S0` 且落在 `named_linux_348()` 里的那批
                 （与 `CF.cmd_pc` 的 `old` 同一算法 ⇒ 应为 344 条可测，留出半 175）
    * 分半一律用 `CF.split_half_addrs`（按地址 sha1 奇偶），与 §43/§44/§46 同一规则。

    ⚠ 为什么 `old348` 是 344 而不是 331：§44 的 331 是**旧真值**（`control_pairs`）∩348；
    本轮读的是 v2 夹具，S0 里含 344 条，多出的 13 条正是 §46 说的"原本没有真值的
    已定名函数被 S1/S2 补进来"的那批。为保持"与 §44 可比"，本工具另报 `--pop old331`
    （= 用 `CF.control_pairs` 重算旧真值 ∩ 348），见 `population()` 的分支。
    """
    rows = read_tsv(POP_FIXTURE)
    truth, cls, half = {}, {}, {}
    for r in rows:
        a = int(r["linux_addr"], 16)
        truth[a] = int(r["win_addr"], 16)
        cls[a] = r["class"]
        half[a] = r["half"]
    named = CF.named_linux_348()
    if which == "v2":
        addrs = sorted(truth)
        A, B = CF.split_half_addrs(addrs)
        return "v2（975）", truth, sorted(addrs), set(A), set(B)
    if which == "old348":
        addrs = sorted(a for a in truth if a in named and cls[a] == "S0")
        A, B = CF.split_half_addrs(addrs)
        return "old348（v2 的 S0 ∩ named）", truth, addrs, set(A), set(B)
    if which == "old331":
        fps = CF.load_all(tags=["win", "linux"])
        tr0, _prov = CF.control_pairs("win", "linux", fps)
        addrs = sorted(a for a in tr0 if a in named and a in truth)
        A, B = CF.split_half_addrs(addrs)
        return "old331（旧真值 ∩ named）", truth, addrs, set(A), set(B)
    raise SystemExit("未知人群 %r" % which)


# ===========================================================================
# 3. 物化：FP.feats → 稀疏段（不落盘，只常驻显存/内存）
# ===========================================================================
class Side:
    """一份构建的**逐特征**物化结果（CSR，行序 = (size, 地址) 升序，与 `CF.Pool` 同序）。"""

    __slots__ = ("tag", "addrs", "sizes", "segs", "msets", "counts", "hnorm",
                 "hvals", "nvec")

    def __init__(self):
        self.segs = {}      # key -> (off, flat)  或 (off, flat, val) 加权
        self.msets = {}     # key -> (off, flat)
        self.counts = {}    # key -> int64 一维
        self.hnorm = {}     # key -> float64 一维（每行的模长）
        self.hvals = {}     # key -> float64 一维（每个 posting 的权重）


def cgram_of(fe, feats):
    """二阶编码：`∪_{c ∈ callees_a} grams(c)`（被调者的助记符三元组并集）。

    任务 70 §45.9 第 3 条指的那条真空白：现在的 `callee` 是「被调者的**尺寸**多重集」，
    尺寸只有 ~200 个取值，信息量本来就低；换成被调者的助记符三元组才吃得到那 52,576 条边的信息。
    """
    out = set()
    for c in fe["callees_a"]:
        d = feats.get(c)
        if d is not None:
            out |= d["grams"]
    return out


def build_side(torch, fp, addrs, feats_wanted, vocabs, need_a=False):
    s = Side()
    s.tag = fp.tag
    n = len(addrs)
    s.addrs = torch.tensor(addrs, dtype=torch.int64)
    sizes = [0] * n
    s.nvec = {}
    seg_off = {k: [0] * (n + 1) for k in feats_wanted if k in SETS}
    seg_flat = {k: [] for k in seg_off}
    seg_val = {k: [] for k in seg_off}
    ms_off = {k: [0] * (n + 1) for k in feats_wanted if k in MSETS}
    ms_flat = {k: [] for k in ms_off}
    cnt = {k: [0] * n for k in feats_wanted if k in COUNTS}
    hnorm = {k: [0.0] * n for k in feats_wanted if k in HISTS}
    h_off = {k: [0] * (n + 1) for k in hnorm}
    h_flat, h_val = {k: [] for k in hnorm}, {k: [] for k in hnorm}
    cgram_cache = {}
    for i, va in enumerate(addrs):
        fe = fp.feats[va]
        sizes[i] = fe["size"]
        for k in feats_wanted:
            if k in SETS:
                attr = SETS[k]
                if attr == "@cgram_a":
                    g = cgram_cache.get(va)
                    if g is None:
                        g = cgram_of(fe, fp.feats)
                        cgram_cache[va] = g
                    items = g
                else:
                    items = fe[attr]
                    if k in ("cbsig", "rbsig"):
                        # `jaccard_sets` 对 tuple 先 `set()`：ICF 合并过的被调者会有重复 sha1，
                        # 不去重就会把 |A∩B| 数成"出现次数"而 CPU 数的是"种类数"。
                        items = set(items)
                v = vocabs[k]
                for x in items:
                    j = v.get(x)
                    if j is None:
                        j = len(v)
                        v[x] = j
                    seg_flat[k].append(j)
                    seg_val[k].append(1.0)
                seg_off[k][i + 1] = len(seg_flat[k])
            elif k in MSETS:
                fl = ms_flat[k]
                fl.extend(fe[MSETS[k]])
                ms_off[k][i + 1] = len(fl)
            elif k in COUNTS:
                v = fe[COUNTS[k]]
                cnt[k][i] = v if isinstance(v, int) else len(v)
            elif k in HISTS:
                h = fe[HISTS[k]]
                v = vocabs[k]
                acc = 0.0
                for mn, c in h.items():           # dict 插入序 = 扫描序，与 CPU 的 sum() 同序
                    j = v.get(mn)
                    if j is None:
                        j = len(v)
                        v[mn] = j
                    h_flat[k].append(j)
                    h_val[k].append(float(c))
                    acc += c * c
                h_off[k][i + 1] = len(h_flat[k])
                hnorm[k][i] = math.sqrt(acc)
    s.sizes = torch.tensor(sizes, dtype=torch.int64)
    for k in seg_off:
        s.segs[k] = (torch.tensor(seg_off[k], dtype=torch.int64),
                     torch.tensor(seg_flat[k], dtype=torch.int64),
                     torch.tensor(seg_val[k], dtype=torch.float64))
    for k in ms_off:
        s.msets[k] = (torch.tensor(ms_off[k], dtype=torch.int64),
                      torch.tensor(ms_flat[k], dtype=torch.int64))
    for k, v in cnt.items():
        s.counts[k] = torch.tensor(v, dtype=torch.int64)
    for k in hnorm:
        s.hnorm[k] = torch.tensor(hnorm[k], dtype=torch.float64)
        s.hvals[k] = (torch.tensor(h_off[k], dtype=torch.int64),
                      torch.tensor(h_flat[k], dtype=torch.int64),
                      torch.tensor(h_val[k], dtype=torch.float64))
    return s


def build(torch, fps, rows, feats):
    """行侧（Linux 正对照）与池侧（Windows 全宇宙）都物化；返回 (q, p, 词表大小)。"""
    lin, win = fps["linux"], fps["win"]
    qaddrs = sorted(rows, key=lambda va: (lin.feats[va]["size"], va))
    paddrs = sorted(win.universe, key=lambda va: (win.feats[va]["size"], va))
    vocabs = {k: {} for k in feats if k in SETS or k in HISTS}
    q = build_side(torch, lin, qaddrs, feats, vocabs)
    p = build_side(torch, win, paddrs, feats, vocabs)
    return q, p, {k: len(v) for k, v in vocabs.items()}


def window_bounds(qsizes, psizes):
    """逐行候选窗口 `[lo, hi)`：`CF.Pool.window` 的逐位复刻（含 max(8,…) / max(lo+1,…)）。"""
    ps = psizes.tolist() if hasattr(psizes, "tolist") else list(psizes)
    qs = qsizes.tolist() if hasattr(qsizes, "tolist") else list(qsizes)
    lo, hi = [0] * len(qs), [0] * len(qs)
    for i, s in enumerate(qs):
        a = max(8, int(s * WIN[0]))
        b = max(a + 1, int(s * WIN[1]))
        lo[i] = bisect.bisect_left(ps, a)
        hi[i] = bisect.bisect_right(ps, b)
    return lo, hi


# ===========================================================================
# 4. 逐特征相似度（块内 R×C）
# ===========================================================================
def _cum_excl(cnt, torch):
    return torch.cat([torch.zeros(1, dtype=torch.int64, device=cnt.device),
                      torch.cumsum(cnt, 0)[:-1]])


def _expand(start, cnt, torch):
    tot = int(cnt.sum())
    if tot == 0:
        return None
    return torch.repeat_interleave(start, cnt, output_size=tot) + \
        (torch.arange(tot, device=cnt.device, dtype=torch.int64) -
         torch.repeat_interleave(_cum_excl(cnt, torch), cnt, output_size=tot))


def _gather_csr(flat, off, b0, b1, qn, torch):
    idx = _expand(off[b0:b1], qn, torch)
    return None if idx is None else flat[idx]


SHIFT = 32          # 键 = (vid << 32) | col；col < 51,985 ✓，vid < 2^31 ✓
CHUNK = 2_000_000   # 稀疏展开每段最多多少条命中（控制瞬时显存）


def _sorted_keys(flat, off, torch, val=None):
    """(vid, col) → 全局有序键 + 排序后的列号（同 61 号；`sort` 返回的下标不是列号）。

    `val` 给了就**按同一置换**排好一起返回 —— 加权段（`hist`/`cls`）的 posting 值必须
    和键一起重排，否则 `index_add` 会把 A 的权重乘到 B 的 posting 上（静默错值）。
    """
    ncol = int(off.numel()) - 1
    col = torch.repeat_interleave(torch.arange(ncol, device=flat.device, dtype=torch.int64),
                                  off[1:] - off[:-1], output_size=int(off[-1]))
    key = (flat << SHIFT) | col
    order = torch.argsort(key)
    return key[order], col[order], (None if val is None else val[order])


def _intersect(seg, b0, b1, clo, C, wl, wh, pk, pk_col, torch, weighted=False, pkval=None):
    """`I = A_w @ B_wᵀ` 的等价稀疏算式（只走非零；结果是 float64 的**加权**和）。

    `pk` 是池侧 (vid,col) 的全局有序键 ⇒ `(vid, col∈[lo,hi))` 的 posting 是一段连续下标。
    """
    off, flat, val = seg
    qn = off[b0 + 1:b1 + 1] - off[b0:b1]
    vid = _gather_csr(flat, off, b0, b1, qn, torch)
    if vid is None:
        return None
    dev = vid.device
    tot_q = int(vid.numel())
    row = torch.repeat_interleave(torch.arange(b0, b1, device=dev, dtype=torch.int64),
                                  qn, output_size=tot_q)
    base = vid << SHIFT
    s = torch.searchsorted(pk, base | (wl[row - b0] + clo), right=False)
    e = torch.searchsorted(pk, base | (wh[row - b0] + clo), right=False)
    del base
    cnt = e - s
    del e
    I = torch.zeros((b1 - b0) * C, dtype=torch.float64, device=dev)
    cum = torch.cumsum(cnt, 0)
    nlit = int(cnt.numel())
    qval = val[off[b0]:off[b1]] if weighted else None
    allval = pkval if weighted else None        # **已按键排序**的值，和 `idx` 同一坐标系
    i0 = 0
    while i0 < nlit:
        base_cum = 0 if i0 == 0 else int(cum[i0 - 1])
        tgt = base_cum + CHUNK
        i1 = int(torch.searchsorted(
            cum, torch.tensor(tgt, device=dev, dtype=cum.dtype), right=False)) + 1
        i1 = min(max(i1, i0 + 1), nlit)
        seg_s, seg_cnt = s[i0:i1], cnt[i0:i1]
        idx = _expand(seg_s, seg_cnt, torch)
        if idx is not None:
            row_exp = torch.repeat_interleave(row[i0:i1], seg_cnt, output_size=idx.numel())
            w = (torch.repeat_interleave(qval[i0:i1], seg_cnt, output_size=idx.numel())
                 * allval[idx]) if weighted else \
                torch.ones(idx.numel(), dtype=torch.float64, device=dev)
            I.index_add_(0, (row_exp - b0) * C + (pk_col[idx] - clo), w)
            del row_exp, w
        i0 = i1
    return I.view(b1 - b0, C)


def mat_block(torch, q, p, feats, b0, b1, clo, chi, wl, wh, dbg=None):
    """**物化**一个块：返回 {key: (V, A)}，V 是逐特征相似度 (R,C) float64，A 是可行性掩码。

    这就是"一次物化"那一步；`--mode old` 会在每组权重里重跑它（= 63 号的做法）。
    """
    dev = wl.device
    F64 = torch.float64
    R, C = b1 - b0, chi - clo
    out = {}
    pk_cache = {}

    def keys(k):
        if k not in pk_cache:
            off, flat, val = p.segs[k] if k in p.segs else p.hvals[k]
            pk_cache[k] = _sorted_keys(flat, off, torch, val if k in HISTS else None)
        return pk_cache[k]

    def f64(x):
        return x.to(F64)

    ones = torch.ones((R, C), dtype=F64, device=dev)
    if "size" in feats:
        sq = f64(q.sizes[b0:b1].to(dev))
        sd = f64(p.sizes[clo:chi].to(dev))
        with torch.no_grad():
            ratio = (sd[None, :] + 1.0) / (sq[:, None] + 1.0)
            V = 1.0 - torch.minimum(torch.abs(torch.log(ratio)) / SIZE_EPS,
                                    torch.ones_like(ratio))
        out["size"] = (V, ones)
        del ratio, V

    for k in feats:
        if k in SETS:
            # ① 分子 = 交集条数（|A∩B|）；分母的可用性 = 两侧都非空
            pk, pkc, _pv = keys(k)
            I = _intersect(q.segs[k], b0, b1, clo, C, wl, wh, pk, pkc, torch)
            if I is None:
                out[k] = (torch.zeros((R, C), dtype=F64, device=dev),
                          torch.zeros((R, C), dtype=F64, device=dev))
                continue
            qo = q.segs[k][0]
            po = p.segs[k][0]
            qn = f64(qo[b0 + 1:b1 + 1] - qo[b0:b1]).to(dev)
            pn = f64(po[clo + 1:chi + 1] - po[clo:chi]).to(dev)
            has = (qn[:, None] > 0) & (pn[None, :] > 0)
            den = qn[:, None] + pn[None, :] - I
            # `jaccard` 在两侧非空时 den ≥ max(qn,pn) ≥ 1 > 0，clamp 不改任何值
            V = torch.where(has, I / torch.clamp(den, min=1.0),
                            torch.zeros_like(I))
            cond = COND[k]
            if cond == "both":
                A = f64(has)
            elif cond == "either":
                A = f64((qn[:, None] > 0) | (pn[None, :] > 0))
            else:
                A = ones
            out[k] = (V, A)
            del I, den, has, V, A
        elif k in MSETS:
            off, flat = q.msets[k]
            poff, pflat = p.msets[k]
            qn = off[b0 + 1:b1 + 1] - off[b0:b1]
            pn = poff[clo + 1:chi + 1] - poff[clo:chi]
            St = torch.zeros((R, C), dtype=F64, device=dev)
            nmax = int(qn.max()) if qn.numel() else 0
            if nmax > 0 and int(pn.max()) > 0:
                order = torch.argsort(qn, descending=True)
                nq_sorted = qn[order]
                for t in range(nmax):
                    cnt = int((nq_sorted > t).sum())
                    if cnt == 0:
                        break
                    ok = pn > t
                    if not bool(ok.any()):
                        continue
                    ridx = order[:cnt]
                    uq = f64(flat[torch.clamp(off[b0 + ridx] + t,
                                              max=int(flat.numel()) - 1)]).to(dev)
                    ud = f64(pflat[torch.clamp(poff[clo:chi] + t,
                                               max=int(pflat.numel()) - 1)]).to(dev)
                    r2 = (ud[None, :] + 1.0) / (uq[:, None] + 1.0)
                    tent = 1.0 - torch.minimum(torch.abs(torch.log(r2)) / SIZE_EPS,
                                               torch.ones_like(r2))
                    St[ridx] += torch.where(ok[None, :], tent, torch.zeros_like(tent))
                    del uq, ud, r2, tent
            qnf = f64(qn).to(dev)
            pnf = f64(pn).to(dev)
            nmin = torch.minimum(qnf[:, None], pnf[None, :])
            pr = St / torch.clamp(nmin, min=1.0)
            cr = (pnf[None, :] + 1.0) / (qnf[:, None] + 1.0)
            cn = 1.0 - torch.minimum(torch.abs(torch.log(cr)) / SIZE_EPS,
                                     torch.ones_like(cr))
            val = torch.where((qnf[:, None] > 0) & (pnf[None, :] > 0),
                              0.5 * pr + 0.5 * cn, torch.zeros_like(pr))
            out[k] = (val, ones)
            del St, pr, cr, cn, val, nmin
        elif k in COUNTS:
            qv = f64(q.counts[k][b0:b1].to(dev))
            pv = f64(p.counts[k][clo:chi].to(dev))
            r2 = (pv[None, :] + 1.0) / (qv[:, None] + 1.0)
            V = 1.0 - torch.minimum(torch.abs(torch.log(r2)) / SIZE_EPS,
                                    torch.ones_like(r2))
            cond = COND[k]
            has = (qv[:, None] > 0) | (pv[None, :] > 0)
            if cond == "either":
                V = torch.where(has, V, torch.zeros_like(V))
                A = f64(has)
            else:
                A = ones
            out[k] = (V, A)
            del V, A, r2, has
        elif k in HISTS:
            off, flat, val = q.hvals[k]
            poff, pflat, pval = p.hvals[k]
            kk3 = keys(k)
            num = _intersect((off, flat, val), b0, b1, clo, C, wl, wh,
                             kk3[0], kk3[1], torch, weighted=True, pkval=kk3[2])
            dq = q.hnorm[k][b0:b1].to(dev)
            dp = p.hnorm[k][clo:chi].to(dev)
            if num is None:
                out[k] = (torch.zeros((R, C), dtype=F64, device=dev), ones)
                continue
            den = dq[:, None] * dp[None, :]
            V = torch.where((num > 0) & (den > 0), num / torch.clamp(den, min=1e-300),
                            torch.zeros_like(num))
            out[k] = (V, ones)
            del num, V, den
    return out


# ===========================================================================
# 5. 融合（严格按 score_same 的 add() 顺序）+ 精确 top-K
# ===========================================================================
NEG = -1e300
BIGA = (1 << 62) - 1


def fuse(pairs, W, torch):
    """`score[c] = Σ_f w[c,f]·V[f] / Σ_f w[c,f]·A[f]`，逐位按 CPU 的加法顺序。

    `pairs` = [(V_f, A_f)]，顺序必须是 `feats` 的累加序；`W` 是 (N, F) float64（已在设备上）。
    权重 0 的项照样加（`tot + 0.0*V == tot` 对有限值精确成立），所以同一份张量
    能同时给出任意权重子集的结果，且**逐位等于** CPU 逐项 add() 的结果。
    """
    N = W.shape[0]
    tot = W[:, 0].view(N, 1, 1) * pairs[0][0]
    wt = W[:, 0].view(N, 1, 1) * pairs[0][1]
    for i in range(1, len(pairs)):
        tot = tot + W[:, i].view(N, 1, 1) * pairs[i][0]
        wt = wt + W[:, i].view(N, 1, 1) * pairs[i][1]
    # CPU 的 `score_same` 以 `tot / wt if wt else 0.0` 收尾 ⇒ wt==0 的格子是**0.0**不是 NaN。
    # 真实配置里 wt 恒 > 0（size/caller 无条件进分母），但单特征自检会走到这一支。
    sc = torch.where(wt > 0, tot / torch.clamp(wt, min=1e-300), torch.zeros_like(tot))
    del tot, wt
    return sc


def topk_lex(sc, in_win, addr_c, kk, torch):
    """精确的 `(-score, addr)` 序前 kk 名（并列按**地址最小**，与 CPU 的 sort 键一致）。

    两步：① 严格大于第 kk 名的一律进（它们的下标唯一，topk 拿到就是它）；
    ② 等于第 kk 名的按地址取最小的 kk 个；③ 两组并起来按 `(-score, addr)` 两级稳定排序。
    """
    M, C = sc.shape
    S = torch.where(in_win, sc, torch.full_like(sc, NEG))
    kk = min(kk, C)
    v, i = torch.topk(S, k=kk, dim=1)
    kth = v[:, kk - 1:kk]
    a_gt = torch.gather(addr_c.expand(M, C), 1, i)
    a_gt = torch.where(v > kth, a_gt, torch.full_like(a_gt, BIGA))
    eqa = torch.where(S == kth, addr_c.expand(M, C), torch.full_like(S, BIGA, dtype=torch.int64))
    tie_a, _ = torch.topk(eqa, k=kk, dim=1, largest=False)
    cs = torch.cat([torch.where(a_gt != BIGA, v, torch.full_like(v, NEG)),
                    torch.where(tie_a != BIGA, kth.expand(M, kk),
                                torch.full_like(kth.expand(M, kk), NEG))], 1)
    ca = torch.cat([a_gt, tie_a], 1)
    o1 = torch.sort(ca, dim=1, stable=True)[1]                 # 先按地址升序
    o2 = torch.sort(torch.gather(cs, 1, o1), dim=1, stable=True, descending=True)[1]
    idx = torch.gather(o1, 1, o2)
    fs = torch.gather(cs, 1, idx)[:, :kk]
    fa = torch.gather(ca, 1, idx)[:, :kk]
    return fs, fa


# ===========================================================================
# 6. 配置表
# ===========================================================================
def label_of(w, feats):
    """配置标签：只列**非 0** 的键（与 `CF.wlabel` 同风格，含本轮新键）。"""
    parts = []
    for k in feats:
        v = w.get(k, 0.0)
        if v:
            parts.append("%s%g" % (k, v))
    return " ".join(parts) if parts else "(empty)"


BASE = dict(CF.W_SAME)          # 51 号基线（size.25 gram2 consts.25 caller.5）
REF = dict(BASE, scal=0.5)      # §43 headline（并列桶最高，旧口径 78.2）
PREREG = dict(BASE, scal=0.25)  # §43.4 预注册选中（旧口径 76.4）


def builtin_configs(kind, feats):
    """内置配置集。返回 [(label, {key: weight})]。"""
    out = []

    def add(note, w):
        out.append((note, {k: float(w.get(k, 0.0)) for k in feats}))

    if kind == "anchor":
        add("BASE", BASE)
        add("PREREG scal.25", PREREG)
        add("REF scal.5", REF)
        add("scal_c.25（泄漏探针）", dict(BASE, scal=0.0, scal_c=0.25))
        add("scal_c.5（泄漏探针）", dict(BASE, scal=0.0, scal_c=0.5))
        add("shape1.0", dict(BASE, shape=1.0))
    elif kind == "loo-ref":
        # REF 的 5 个特征逐个拿掉（§44.3 那张"留一法"表的张量版）
        add("REF", REF)
        for k in CORE:
            w = dict(REF)
            w[k] = 0.0
            add("REF − %s" % k, w)
    elif kind == "loo-all":
        # **全特征等权**基线 + 逐个拿掉（这是张量才做得出来的表：§44.3 只能在 REF 上做）
        full = {k: 0.0 for k in feats}
        for k in feats:
            if k != "strs":                     # `strs` 是泄漏列，永不进基线
                full[k] = 0.5
        add("ALL18 等权 0.5", full)
        for k in feats:
            if k == "strs" or not full.get(k):
                continue
            w = dict(full)
            w[k] = 0.0
            add("ALL − %s" % k, w)
    elif kind == "second":
        # 二阶编码：REF ± `cgram_a` / `callee_a` / `ncal_a`（任务 70 §45.9 第 3 条）
        add("REF", REF)
        for k in ("cgram_a", "callee_a", "ncal_a", "caller_a", "ncall_a"):
            for wt in (0.25, 0.5, 1.0):
                add("REF + %s=%g" % (k, wt), dict(REF, **{k: wt}))
        add("REF − caller + cgram_a=.5", dict(REF, caller=0.0, cgram_a=0.5))
        add("REF + cgram_a=.5 + callee_a=.5", dict(REF, cgram_a=0.5, callee_a=0.5))
        add("只用二阶 cgram_a=1", {k: (1.0 if k == "cgram_a" else 0.0) for k in feats})
    elif kind == "grid":
        # 补图+二阶的加性网格（预注册用：只在**调参半**上选，见 §7）
        axes = collections.OrderedDict([("scal", (0.0, 0.25, 0.5)),
                                        ("cgram_a", (0.0, 0.25, 0.5)),
                                        ("callee_a", (0.0, 0.25, 0.5)),
                                        ("ncal_a", (0.0, 0.25, 0.5))])
        for combo in itertools.product(*axes.values()):
            w = dict(BASE)
            for k, v in zip(axes, combo):
                w[k] = v
            add(label_of(w, feats) or "(empty)", w)
    elif kind == "ab":
        # A/B（旧做法 vs 新做法）用的配置集：**只碰 CORE 里的键**，这样两种做法
        # 物化的是同一套特征，加速比才是单一变量。3^4 = 81 组。
        axes = collections.OrderedDict([("scal", (0.0, 0.25, 0.5)),
                                        ("gram", (1.0, 2.0, 3.0)),
                                        ("caller", (0.0, 0.5, 1.0)),
                                        ("size", (0.0, 0.25, 0.5))])
        for combo in itertools.product(*axes.values()):
            w = dict(BASE)
            for k, v in zip(axes, combo):
                w[k] = v
            add(label_of(w, feats) or "(empty)", w)
    elif kind.startswith("big"):
        # 天花板实验：`big<N>` = N 组权重（轴网格 × 随机补足），全在**调参半**上选。
        # 有张量之后这件事是一次批处理，所以"3,000 组"是真跑出来的，不是外推。
        import random
        n = int(kind[3:] or 3000)
        rnd = random.Random(71)
        pool = [k for k in feats if k != "strs"]
        levels = (0.0, 0.125, 0.25, 0.375, 0.5, 0.75, 1.0, 1.5, 2.0)
        seen = set()
        # ① BASE 的 5 个特征先用全笛卡尔积（9^5 太大 ⇒ 用受限轴）
        fixed = [k for k in CORE if k in pool]
        for combo in itertools.product((0.0, 0.25, 0.5), (0.0, 0.25, 0.5, 1.0, 2.0),
                                       (0.0, 0.25, 0.5), (0.0, 0.25, 0.5, 1.0),
                                       (0.0, 0.25, 0.5, 1.0)):
            w = {k: 0.0 for k in feats}
            for k, v in zip(fixed, combo):
                w[k] = v
            key = tuple(round(w[k], 6) for k in feats)
            if key in seen or not any(w.values()):
                continue
            seen.add(key)
            add(label_of(w, feats), w)
            if len(out) >= n // 2:
                break
        # ② 其余随机（每个特征从 9 档里独立取；固定随机种子 ⇒ 可复跑）
        guard = 0
        while len(out) < n and guard < n * 40:
            guard += 1
            w = {k: (rnd.choice(levels) if rnd.random() < 0.6 else 0.0) for k in pool}
            if not any(w.values()):
                continue
            key = tuple(round(w.get(k, 0.0), 6) for k in feats)
            if key in seen:
                continue
            seen.add(key)
            add(label_of(w, feats), w)
    else:
        raise SystemExit("未知 --configs %r" % kind)
    if kind.startswith("big"):
        out = out[:int(kind[3:] or 3000)]
    return out


# ===========================================================================
# 7. 主循环：一次物化 + N 组批加权
# ===========================================================================
def to_dev(side, dev):
    """把一份 `Side` 的全部 CSR/连续数组搬到设备上（**就地改**，只搬一次）。

    61 号的 `cmd_gpu` 是逐段 `.to(dev)`；这里统一搬，避免"池侧忘了搬"这类静默错
    （第一版就死在 `_intersect` 里 `cuda:0 and cpu` 混用）。
    """
    if dev == "cpu":
        return side
    side.addrs = side.addrs.to(dev)
    side.sizes = side.sizes.to(dev)
    for k, t in list(side.segs.items()):
        side.segs[k] = tuple(x.to(dev) for x in t)
    for k, t in list(side.msets.items()):
        side.msets[k] = tuple(x.to(dev) for x in t)
    for k, t in list(side.counts.items()):
        side.counts[k] = t.to(dev)
    for k, t in list(side.hnorm.items()):
        side.hnorm[k] = t.to(dev)
    for k, t in list(side.hvals.items()):
        side.hvals[k] = tuple(x.to(dev) for x in t)
    return side


def run(torch, q, p, feats, cfgs, rowsel, dev, cells=600_000, cfg_cells=4_000_000,
        mode="new", topk=5, truth=None, rowaddrs=None, verbose=True, prof=None):
    """返回 (hits, wall_s, stats)。

    `hits[c][ri] = (top1, top5)`：第 c 组配置在第 ri 行（`rowsel` 序）上的命中。
    """
    F64 = torch.float64
    to_dev(q, dev)
    to_dev(p, dev)
    n = len(rowsel)
    qsizes = [int(q.sizes[i]) for i in rowsel]
    raddr = [rowaddrs[i] for i in rowsel] if rowaddrs is not None else None
    wlo, whi = window_bounds(qsizes, p.sizes)
    psz = p.sizes.tolist()
    addrs_all = p.addrs.to(dev)
    qrow = torch.tensor(rowsel, dtype=torch.int64, device=dev)
    Wt = torch.tensor([[w.get(k, 0.0) for k in feats] for _l, w in cfgs],
                      dtype=F64, device=dev)
    N = len(cfgs)
    hits = [[None] * n for _ in range(N)]

    # 分块：R×C ≤ cells（尾部大函数窗口极宽，死用固定行数会爆显存）
    blocks, i = [], 0
    while i < n:
        lo, hi, j = wlo[i], whi[i], i + 1
        while j < n:
            nlo, nhi = min(lo, wlo[j]), max(hi, whi[j])
            if (j + 1 - i) * (nhi - nlo) > cells and j > i:
                break
            lo, hi = nlo, nhi
            j += 1
        blocks.append((i, j))
        i = j
    peak = 0.0
    t_mat = t_fuse = t_top = 0.0
    t0 = time.perf_counter()
    for bi, (b0, b1) in enumerate(blocks):
        R = b1 - b0
        clo, chi = int(min(wlo[b0:b1])), int(max(whi[b0:b1]))
        C = chi - clo
        if C <= 0:
            for r in range(b0, b1):
                for c in range(N):
                    hits[c][r] = (0, 0)
            continue
        assert C <= (1 << SHIFT)
        cols = torch.arange(C, device=dev)
        addr_blk = addrs_all[clo:chi]
        wl = torch.tensor([wlo[b0 + k] - clo for k in range(R)], dtype=torch.int64, device=dev)
        wh = torch.tensor([whi[b0 + k] - clo for k in range(R)], dtype=torch.int64, device=dev)
        in_win = (cols[None, :] >= wl[:, None]) & (cols[None, :] < wh[:, None])
        ncols = max(1, min(N, max(1, cfg_cells // max(1, R * C))))
        for c0 in range(0, N, ncols):
            c1 = min(N, c0 + ncols)
            if mode == "old":
                # 63 号的做法：每组权重都把逐特征相似度**重算一遍**。为了 A/B 是单一变量，
                # 这里重算**同一套特征集合**（不是"只算这组权重非 0 的那几个"）——
                # 那会让 old 侧偏快、把加速比压低，是保守方向。
                for ci in range(c0, c1):
                    tm = time.perf_counter()
                    V = mat_block(torch, q, p, feats, b0, b1, clo, chi, wl, wh)
                    t_mat += time.perf_counter() - tm
                    tf = time.perf_counter()
                    sc = fuse([V[k] for k in feats], Wt[ci:ci + 1], torch)[0]   # (R,C)
                    t_fuse += time.perf_counter() - tf
                    tt = time.perf_counter()
                    _s, fa = topk_lex(sc, in_win, addr_blk, topk, torch)
                    t_top += time.perf_counter() - tt
                    if truth is not None:
                        trow = torch.tensor([truth[raddr[b0 + r]] for r in range(R)],
                                            dtype=torch.int64, device=dev)
                        h1 = (fa[:, 0] == trow)
                        h5 = (fa == trow[:, None]).any(dim=1)
                        hits[ci][b0:b1] = list(zip(h1.tolist(), h5.tolist()))
                    del V, sc, fa
            else:
                # 本工具的做法：**一次物化**，N 组权重只是同一个张量的广播乘加
                tm = time.perf_counter()
                V = mat_block(torch, q, p, feats, b0, b1, clo, chi, wl, wh)
                t_mat += time.perf_counter() - tm
                tf = time.perf_counter()
                sc = fuse([V[k] for k in feats], Wt[c0:c1], torch)
                t_fuse += time.perf_counter() - tf
                tt = time.perf_counter()
                fs, fa = topk_lex(sc.view((c1 - c0) * R, C), in_win.expand(c1 - c0, R, C)
                                  .reshape((c1 - c0) * R, C), addr_blk, topk, torch)
                t_top += time.perf_counter() - tt
                if truth is not None:
                    trow = torch.tensor([[truth[raddr[b0 + r]]] for r in range(R)],
                                        dtype=torch.int64, device=dev)
                    h1 = (fa.view(c1 - c0, R, topk)[:, :, 0] == trow[None, :, 0])
                    h5 = (fa.view(c1 - c0, R, topk) == trow[None, :, 0][:, :, None]).any(dim=2)
                    h1l, h5l = h1.tolist(), h5.tolist()
                    for ci in range(c1 - c0):
                        hits[c0 + ci][b0:b1] = list(zip(h1l[ci], h5l[ci]))
                del sc, fa, fs, V
            if dev == "cuda":
                peak = max(peak, torch.cuda.max_memory_allocated() / 2**20)
        if verbose and bi % 4 == 0:
            if dev == "cuda":
                torch.cuda.synchronize()
            sys.stderr.write("[tensor] 块 %d/%d（行 %d–%d，C=%d）%.1f s peak %.0f MB\n"
                             % (bi + 1, len(blocks), b0, b1, C,
                                time.perf_counter() - t0, peak))
    if dev == "cuda":
        torch.cuda.synchronize()
    wall = time.perf_counter() - t0
    cells_total = int(sum(whi[i] - wlo[i] for i in range(n)))
    st = dict(backend=dev, mode=mode, rows=n, configs=N, blocks=len(blocks),
              candidate_cells=cells_total, features=list(feats),
              cells_x_feats=cells_total * len(feats),
              cells_x_feats_x_cfg=cells_total * len(feats) * N,
              materialize_s=round(t_mat, 4), fuse_s=round(t_fuse, 4),
              topk_s=round(t_top, 4), wall_s=round(wall, 4),
              peak_alloc_MB=round(peak, 1))
    if prof is not None:
        prof.update(st)
    return hits, wall, st


def acc(hits, N, n):
    """每组配置的 (top1, top5) 命中数。"""
    out = []
    for c in range(N):
        h = hits[c]
        out.append((sum(1 for x in h if x and x[0]), sum(1 for x in h if x and x[1]), n))
    return out


def write_hits(path, cfgs, hits, rowaddrs):
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("# tools/callgraph_sim_tensor71.py 逐样本命中（1=top-1 对 / top-5 对）\n")
        fh.write("cfg\tlabel\tn\ttop1\ttop5\thits\n")
        for ci, (lab, _w) in enumerate(cfgs):
            h = hits[ci]
            s = "".join("%d%d" % (1 if x[0] else 0, 1 if x[1] else 0) if x else "--"
                        for x in h)
            fh.write("%d\t%s\t%d\t%d\t%d\t%s\n"
                     % (ci, lab, len(h), sum(1 for x in h if x[0]),
                        sum(1 for x in h if x[1]), s))


# ===========================================================================
# 8. 子命令
# ===========================================================================
def _torch(backend):
    import torch
    # CPU 侧**显式压线程**：这台机器 32 线程，还有别人在跑合并验证；
    # torch 默认吃满 16 线程，两个进程就能把机器占满并互相抢核（2026-11 实测）。
    n = int(os.environ.get("SIMT_THREADS", "8"))
    torch.set_num_threads(max(1, n))
    if backend == "gpu":
        if not torch.cuda.is_available():
            raise SystemExit("CUDA 不可用（torch %s）" % torch.__version__)
        torch.manual_seed(0)
        torch.backends.cuda.matmul.allow_tf32 = False
        return torch, "cuda"
    return torch, "cpu"


def cmd_selfcheck(args):
    """张量 vs `CF.score_same`：① 随机对逐位对拍 ② 一个块的 top-K 与 `CF.rank` 逐条对拍。"""
    torch, dev = _torch(args.backend)
    fps = CF.load_all(tags=["win", "linux"])
    feats = parse_feats(args.feats)
    name, truth, addrs, A, B = population(args.pop)
    pool = CF.Pool(fps["win"])
    # ---- ① 随机对：取一个块，把张量的每一格与 score_same 逐格比 ----
    import random
    rnd = random.Random(71)
    # 只给**物化的那几个**特征权重；其余一律 0（否则比的是"半个配置 vs 整个配置"）
    w = {k: 0.0 for k in ALL_FEATS}
    for k in feats:
        w[k] = args.probe_weight
    rows = sorted(addrs)[:args.rows]
    q, p, vocab = build(torch, fps, rows, feats)
    print(json.dumps(dict(population=name, rows=len(rows), feats=feats, dev=dev,
                          vocab=vocab), ensure_ascii=False, sort_keys=True))
    wlo, whi = window_bounds([int(q.sizes[i]) for i in range(len(rows))], p.sizes)
    psz = p.sizes.tolist()
    worst = None
    total = 0
    hist = collections.Counter()
    for bi in range(args.blocks):
        b0 = rnd.randrange(0, max(1, len(rows) - 1))
        b1 = min(len(rows), b0 + args.block_rows)
        clo, chi = int(min(wlo[b0:b1])), int(max(whi[b0:b1]))
        R, C = b1 - b0, chi - clo
        if C <= 0:
            continue
        wl = torch.tensor([wlo[b0 + k] - clo for k in range(R)], dtype=torch.int64, device=dev)
        wh = torch.tensor([whi[b0 + k] - clo for k in range(R)], dtype=torch.int64, device=dev)
        V = mat_block(torch, q, p, feats, b0, b1, clo, chi, wl, wh)
        Wt = torch.tensor([[w.get(k, 0.0) for k in feats]], dtype=torch.float64, device=dev)
        sc = fuse([V[k] for k in feats], Wt, torch)[0]
        scl = sc.cpu().tolist()
        # CPU 真值：CF.score_same 逐格
        qaddrs = q.addrs.tolist()
        paddrs = p.addrs.tolist()
        ref = []
        maxd = 0.0
        nbad = 0
        for r in range(R):
            qf = fps["linux"].feats[qaddrs[b0 + r]]
            line = []
            for c in range(wlo[b0 + r] - clo, whi[b0 + r] - clo):
                df = fps["win"].feats[paddrs[clo + c]]
                v = CF.score_same(qf, df, w)
                line.append(v)
                d = abs(v - scl[r][c]) if c >= 0 else 0.0
                if d > 0:
                    nbad += 1
                    u = d / math.ulp(v) if v and math.ulp(v) else 0.0
                    hist["1ulp" if u <= 1.0001 else ("<=3ulp" if u <= 3.0001 else ">3ulp")] += 1
                    if u > maxd:
                        maxd = u
                        worst = (qaddrs[b0 + r], paddrs[clo + c], v, scl[r][c], u)
            total += len(line)
            ref.append(line)
        print("[selfcheck] 块 %d：%d 行 × C=%d，逐格比 %d，不一致 %d"
              % (bi, R, C, int(sum(len(x) for x in ref)), nbad))
    print(json.dumps(dict(cells_compared=total, mismatch_by_class=dict(hist),
                          worst_rel=worst), ensure_ascii=False, sort_keys=True))

    # ---- ② top-K：整块与 CF.rank 逐条比 ----
    mism = 0
    cmp_rows = 0
    for ri in range(min(args.topk_rows, len(rows))):
        r = ri
        cl, ch = wlo[r], whi[r]
        if ch - cl <= 0:
            continue
        b0, b1 = r, r + 1
        clo, chi = cl, ch
        C = chi - clo
        wl = torch.zeros(1, dtype=torch.int64, device=dev)
        wh = torch.tensor([C], dtype=torch.int64, device=dev)
        V = mat_block(torch, q, p, feats, b0, b1, clo, chi, wl, wh)
        Wt = torch.tensor([[w.get(k, 0.0) for k in feats]], dtype=torch.float64, device=dev)
        sc = fuse([V[k] for k in feats], Wt, torch)[0]
        inw = torch.ones((1, C), dtype=torch.bool, device=dev)
        _fs, fa = topk_lex(sc, inw, p.addrs[clo:chi].to(dev), args.topk, torch)
        got1 = int(fa[0, 0])
        ww = w                              # `CF.rank` 只把 (qf, df) 传给打分器 ⇒ 权重闭包进去
        ref = CF.rank(fps["linux"].feats[q.addrs[r].item()], pool,
                      lambda a, b: CF.score_same(a, b, ww), CF.WIN_SAME, args.topk)
        cmp_rows += 1
        if ref and ref[0][1] != got1:
            mism += 1
            print("[selfcheck] top-1 不一致 %#x：张量 %#x / CPU %#x"
                  % (q.addrs[r].item(), got1, ref[0][1]))
    print("[selfcheck] top-1 逐条：%d 行，不一致 %d" % (cmp_rows, mism))
    return 0 if mism == 0 else 1


def _load_cfg_file(path):
    out = []
    for r in read_tsv(path):
        w = {k: float(v) for k, v in r.items() if k != "label" and v not in ("", "-")}
        out.append((r.get("label", label_of(w, list(w))), w))
    return out


def cmd_sweep(args):
    torch, dev = _torch(args.backend)
    t_proc = time.perf_counter()
    fps = CF.load_all(tags=["win", "linux"])
    if any(k.endswith("_a") for k in parse_feats(args.feats)) or \
            any(k.endswith("_a") for k in parse_feats(args.config_feats)):
        import callgraph_indirect as CGIC
        CGIC.attach(fps["linux"])
        CGIC.attach(fps["win"])
    name, truth, addrs, A, B = population(args.pop)
    half = sorted(A) if args.half == "tune" else sorted(B)
    cfgs = (_load_cfg_file(args.configs_file) if args.configs_file
            else builtin_configs(args.configs, parse_feats(args.config_feats)))
    feats = parse_feats(args.feats) if args.feats else \
        sorted({k for _l, w in cfgs for k, v in w.items() if v},
               key=lambda k: ORDER.index(k))
    for _l, w in cfgs:
        for k in w:
            if k not in feats and w[k]:
                raise SystemExit("配置用了没物化的特征 %r（--feats 里加上它）" % k)
    t_build = time.perf_counter()
    q, p, vocab = build(torch, fps, half, feats)
    t_build = time.perf_counter() - t_build
    nnz = {k: (int(p.segs[k][0][-1]) if k in p.segs else 0) for k in feats}
    sys.stderr.write("[tensor] %s / %s 半：行 %d，池 %d，特征 %d %s\n"
                     % (name, args.half, len(half), len(p.sizes), len(feats), feats))
    rows_sel = list(range(len(half)))
    if args.limit:
        rows_sel = rows_sel[:args.limit]
    hits, wall, st = run(torch, q, p, feats, cfgs, rows_sel, dev,
                         cells=args.cells, cfg_cells=args.cfg_cells, mode=args.mode,
                         topk=args.topk, truth=truth, rowaddrs=q.addrs.tolist())
    an = acc(hits, len(cfgs), len(rows_sel))
    print("%-46s %8s %8s" % ("配置", "top-1", "top-5"))
    for ci, (lab, _w) in enumerate(cfgs):
        k1, k5, n = an[ci]
        print("%-46s %7.2f%% %7.2f%%" % (lab[:46], 100.0 * k1 / n, 100.0 * k5 / n))
    st.update(population=name, half=args.half, build_s=round(t_build, 4),
              nnz_pool=nnz, vocab=vocab, proc_wall_s=round(time.perf_counter() - t_proc, 4),
              topk=args.topk, results=[dict(label=lab, top1=k1, top5=k5, n=n)
                                       for (lab, _w), (k1, k5, n) in zip(cfgs, an)])
    if args.out:
        write_hits(args.out, cfgs, hits, [q.addrs[i].item() for i in rows_sel])
    if args.report:
        with open(args.report, "w", encoding="utf-8") as fh:
            json.dump(st, fh, indent=2, sort_keys=True)
    print(json.dumps(st, ensure_ascii=False, sort_keys=True))
    return 0


def cmd_pop(args):
    fps = CF.load_all()
    pc = CF.positive_control_v2(fps)
    named = CF.named_linux_348()
    out = []
    for a, w in sorted(pc["truth"].items()):
        out.append((a, w, pc["cls"][a], a in named))
    path = os.path.join(args.out or os.path.join(FIXTURES, "fingerprint_pop71.tsv"))
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("# 任务 71 · 正对照人群（v2 975 / 旧口径 348）逐条；判据只用 Location/符号名\n")
        fh.write("linux_addr\twin_addr\tclass\tin_named348\thalf\n")
        for a, w, c, nm in out:
            h = "tune" if a in set(CF.split_half_addrs(sorted(pc["truth"]))[0]) else "holdout"
            fh.write("0x%x\t0x%x\t%s\t%s\t%s\n"
                     % (a, w, c, "是" if nm else "否", h))
    print("写出 %s（%d 行）" % (path, len(out)))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("cmd", choices=["selfcheck", "sweep", "pop"])
    ap.add_argument("--tools", default="")
    ap.add_argument("--reference-root", default="")
    ap.add_argument("--cache", default="")
    ap.add_argument("--backend", default="cpu", choices=["cpu", "gpu"])
    ap.add_argument("--pop", default="v2", choices=["v2", "old348", "old331"])
    ap.add_argument("--half", default="holdout", choices=["tune", "holdout"],
                    help="留出半只报数；预注册选择必须在 tune 半上做")
    ap.add_argument("--feats", default="", help="物化哪些特征（默认：配置里非 0 的并集）")
    ap.add_argument("--config-feats", default="all", help="内置配置按哪些键展开")
    ap.add_argument("--configs", default="anchor")
    ap.add_argument("--configs-file", default="")
    ap.add_argument("--mode", default="new", choices=["new", "old"])
    ap.add_argument("--cells", type=int, default=600_000)
    ap.add_argument("--cfg-cells", type=int, default=4_000_000)
    ap.add_argument("--topk", type=int, default=5)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--rows", type=int, default=48)
    ap.add_argument("--block-rows", type=int, default=8)
    ap.add_argument("--blocks", type=int, default=4)
    ap.add_argument("--topk-rows", type=int, default=200)
    ap.add_argument("--probe-weight", type=float, default=0.5)
    ap.add_argument("--out", default="")
    ap.add_argument("--report", default="")
    args = ap.parse_args(argv)
    if args.cmd == "selfcheck":
        return cmd_selfcheck(args)
    if args.cmd == "sweep":
        return cmd_sweep(args)
    return cmd_pop(args)


if __name__ == "__main__":
    sys.exit(main())
