#!/usr/bin/env python3
"""callgraph_gpu_sim.py — 结构指纹的**大规模相似度检索**（GPU 版，任务 61）。

## 它做什么

把 `callgraph_fingerprint.py` 的 `build` 产物**物化成特征矩阵**，在 GPU 上对
**Linux 13,263 × Windows 51,985** 的相似度取 top-5，并给出一条**与 CPU 版逐条一致**的
正对照。特征口径**完全**沿用 51 号重训过的权重（`CF.W_SAME`），一个字都不改：

    size   0.25   sim_log(size)                    连续
    consts 0.25   jaccard(常量集合)                  multi-hot 内积
    caller 0.50   multiset_sim(调用者尺寸多重集)      按 rank 配对的 log 核
    gram   2.00   jaccard(指令三元组集合)            multi-hot 内积
    strs / strs_s / hist / callee / cbsig / rbsig / ncal / ncall = **0**（不读）

⚠ `strs` 是泄漏（`panic::Location` 的 file 字段投影），本工具**从不读它**
（`docs/159` §3.2、`docs/02` §35.3.1；任务书也把它列为硬约束）。

## 特征矩阵（F 的构成）

    F = F_c(8) + V_gram(41,512) + V_consts(5,860) = 47,380 列

* 连续段 `F_c` = size / ninsn / ncallees / ncallers / nconsts / nstrs_s / ngrams /
  bsig 高 32 位，float32 稠密（13,263 × 8 与 51,985 × 8，合计 2.0 MB）；
* 集合段（gram / consts）是 **multi-hot 稀疏**：Linux 7.3e5 nnz、Windows 2.8e6 nnz。
  **不物化稠密矩阵**：稠密要 5.6e9 个 float32（22 GB），而 `A @ Bᵀ` 的 5.7e13 flop
  几乎全是零。改成只走非零的**等价算式** `I = A_multi-hot @ B_multi-hotᵀ`
  （`searchsorted + gather + index_add`），结果与稠密 GEMM **逐位相同**
  （计数是小整数，float64 累加与累加顺序无关）。

## 三个子命令

    cpu   —— **同口径基线**：逐行调 `callgraph_fingerprint.rank()`（发布版代码本体，不另写）
    gpu   —— 切块 + 稀疏内积 + rank 配对核，全部在 torch(CUDA)
    check —— 比对两份 top-K，并与 `fixtures/callgraph_fingerprint.tsv` 对 top-1

## 用法

    export CFGFP_CACHE=<指纹缓存目录>            # 366 MB、cfgfp-v5，别重扫
    ../.reference/venv/bin/python tools/callgraph_gpu_sim.py cpu   --out cpu.tsv
    <gpu-python>        tools/callgraph_gpu_sim.py gpu   --out gpu.tsv --report gpu.json
    <any-python>        tools/callgraph_gpu_sim.py check --cpu cpu.tsv --gpu gpu.tsv

⚠ **不依赖 numpy**（GPU 侧只用 torch：宿主 gpu-venv 里就没有 numpy）。
`--limit N` 只做前 N 行（量墙钟用）；`--max-cells` 是分块的格子上限（默认 3e6）。
"""

import argparse
import bisect
import collections
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))


def _preboot(argv):
    """`--tools` / `--cache` / `--reference-root` 必须在 import `callgraph_fingerprint`
    **之前**生效：那个模块在 import 时就把"仓库根 / 四份构建的路径"算成了模块级常量
    （`_first_existing` + `sym_match.GW2`），之后再设环境变量已经晚了。

    正常从仓库里直接跑时什么都不用给：`sym_match._reference_root` 会从仓库根**往上找
    6 级** `.reference/steam/2026-08-28/`，所以在 `.agent-work/cg/wt-XX` 里跑也能找到。
    只有"脚本被复制到别处执行"（`cuda_run` 就是这么干的）时才必须给 `--tools`。
    """
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
OUT_FIXTURE = os.path.join(FIXTURES, "callgraph_fingerprint.tsv")

# 权重：**与 51 号发布版逐位相同**（不覆盖、不重训）。显式列出来只是为了让
# "GPU 侧到底算了哪几个特征"一目了然；`strs`/`hist` 在 W_SAME 里本来就是 0。
W = CF.W_SAME
WIN = CF.WIN_SAME
EPS = CF.SIZE_EPS
ACTIVE = tuple(k for k in ("size", "consts", "caller", "gram") if W.get(k))

TOPK = 5
MAXCELLS = 3_000_000  # 一个块最多 R×C 个格子（float64 下约 24 MB/张量）
_CH = 2_000_000       # 稀疏展开每段最多的命中条数（控制瞬时显存）

CONT_NAMES = ("size", "ninsn", "ncallees", "ncallers", "nconsts", "nstrs_s",
              "ngrams", "bsig_hi32")


# ===========================================================================
# 1. 物化：FP.feats → 平铺数组（**不用 numpy**，直接造 torch CPU 张量）
# ===========================================================================
class Side:
    """一份构建的特征矩阵；**行序 = (size, 地址) 升序**，与 `CF.Pool` 完全同序。

    集合段一律 CSR（`x_off` 前缀和 + `x_flat` 拼接），绝不物化稠密多热矩阵。
    """

    __slots__ = ("tag", "addrs", "sizes", "cont", "nq", "nd",
                 "cal_off", "cal_flat", "g_off", "g_flat", "c_off", "c_flat")


def _cont_row(fe):
    """连续段 8 列。`nstrs_s` 只作为计数进矩阵，不参与打分（`strs*` 权重 0）。"""
    return (fe["size"], fe["ninsn"], len(fe["callees"]), len(fe["callers"]),
            len(fe["consts"]), len(fe["strs_s"]), len(fe["grams"]),
            int(fe["bsig"][:8], 16) if fe.get("bsig") else 0)


def build_side(fp, addrs, gvocab, cvocab, torch):
    s = Side()
    s.tag = fp.tag
    n = len(addrs)
    s.addrs = torch.tensor(addrs, dtype=torch.int64)
    sizes = [0] * n
    cont = [None] * n
    nq = [0] * n
    nd = [0] * n
    cal_off = [0] * (n + 1)
    g_off = [0] * (n + 1)
    c_off = [0] * (n + 1)
    cal, gram, con = [], [], []
    for i, va in enumerate(addrs):
        fe = fp.feats[va]
        sizes[i] = fe["size"]
        cont[i] = _cont_row(fe)
        cs = fe["consts"]
        cs = cs if isinstance(cs, (set, frozenset)) else set(cs)
        nd[i] = len(cs)
        nq[i] = len(fe["callers"])
        cal.extend(fe["caller_sizes"])
        gram.extend(gvocab[g] for g in fe["grams"])
        con.extend(cvocab[c] for c in cs)
        cal_off[i + 1] = len(cal)
        g_off[i + 1] = len(gram)
        c_off[i + 1] = len(con)
    s.sizes = torch.tensor(sizes, dtype=torch.int64)
    s.cont = torch.tensor(cont, dtype=torch.float32)
    s.nq = torch.tensor(nq, dtype=torch.int32)
    s.nd = torch.tensor(nd, dtype=torch.int32)
    s.cal_off, s.cal_flat = torch.tensor(cal_off, dtype=torch.int64), \
        torch.tensor(cal, dtype=torch.int64)
    s.g_off, s.g_flat = torch.tensor(g_off, dtype=torch.int64), \
        torch.tensor(gram, dtype=torch.int64)
    s.c_off, s.c_flat = torch.tensor(c_off, dtype=torch.int64), \
        torch.tensor(con, dtype=torch.int64)
    return s


def build_matrices(torch, fps, queries=None):
    """两侧 `Side` + 词表。`queries` 为 None ⇒ 整份 Linux 宇宙（13,263 行）。"""
    lin, win = fps["linux"], fps["win"]
    qaddrs = list(queries or sorted(lin.universe))
    qaddrs.sort(key=lambda va: (lin.feats[va]["size"], va))
    paddrs = sorted(win.universe, key=lambda va: (win.feats[va]["size"], va))
    gvocab, cvocab = {}, {}
    for fp, addrs in ((lin, qaddrs), (win, paddrs)):
        for va in addrs:
            for g in fp.feats[va]["grams"]:
                if g not in gvocab:
                    gvocab[g] = len(gvocab)
            for c in fp.feats[va]["consts"]:
                if c not in cvocab:
                    cvocab[c] = len(cvocab)
    return (build_side(lin, qaddrs, gvocab, cvocab, torch),
            build_side(win, paddrs, gvocab, cvocab, torch), gvocab, cvocab)


def window_bounds(qsizes, psizes, torch):
    """逐行候选窗口 `[lo, hi)`：`CF.Pool.window` 的逐位复刻（含 max(8,…) / max(lo+1,…)）。

    池按 (size, 地址) 排序 ⇒ 候选是**一段连续下标**，这正是"按 size 切块"的依据。
    """
    ps = psizes.tolist()
    qs = qsizes.tolist()
    lo = [0] * len(qs)
    hi = [0] * len(qs)
    for i, s in enumerate(qs):
        a = max(8, int(s * WIN[0]))
        b = max(a + 1, int(s * WIN[1]))
        lo[i] = bisect.bisect_left(ps, a)
        hi[i] = bisect.bisect_right(ps, b)
    return torch.tensor(lo, dtype=torch.int64), torch.tensor(hi, dtype=torch.int64)


# ===========================================================================
# 2. CPU 基线：**调发布版自己的 rank()**，不另写一套
# ===========================================================================
def cmd_cpu(args):
    fps = CF.load_all(tags=["win", "linux"])
    lin = fps["linux"]
    pool = CF.Pool(fps["win"])
    score, _w, win = CF.scorer_for("win", "linux")
    qs = sorted(lin.universe, key=lambda va: (lin.feats[va]["size"], va))
    if args.limit:
        qs = qs[:args.limit]
    if args.shard:
        i, n = [int(x) for x in args.shard.split("/")]
        qs = qs[i::n]                    # 与 `cf run --shard` 同一种按序跨步切法
    cand = sum(len(pool.window(lin.feats[va]["size"], win)) for va in qs)
    sys.stderr.write("[cpu] %d 行 × 候选池 %d（候选对合计 %d）\n"
                     % (len(qs), len(pool.vas), cand))
    t0 = time.perf_counter()
    rows = []
    for i, va in enumerate(qs):
        fe = lin.feats[va]
        rows.append((va, fe["size"], len(fe["callers"]),
                     CF.rank(fe, pool, score, win, args.topk)))
        if (i + 1) % 250 == 0:
            el = time.perf_counter() - t0
            sys.stderr.write("[cpu] %d/%d  %.1f s（预计总 %.0f s）\n"
                             % (i + 1, len(qs), el, el * len(qs) / (i + 1)))
    dt = time.perf_counter() - t0
    write_rows(args.out, rows, args.topk)
    st = dict(backend="cpu", rows=len(qs), candidate_pairs=cand, wall_s=round(dt, 4),
              shard=args.shard or "-",
              us_per_pair=round(dt / max(1, cand) * 1e6, 3),
              pairs_per_s=round(cand / dt), topk=args.topk)
    sys.stderr.write("[cpu] 墙钟 %.3f s（%d 个候选对，%.2f µs/对）\n"
                     % (dt, cand, dt / max(1, cand) * 1e6))
    print(json.dumps(st))
    if args.report:
        with open(args.report, "w", encoding="utf-8") as fh:
            json.dump(st, fh, indent=2, sort_keys=True)
    return 0


def write_rows(path, rows, topk):
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("# tools/callgraph_gpu_sim.py 的 top-K；"
                 "score 为 IEEE-754 双精度全精度（%.17g 写出）\n")
        hdr = ["linux_addr", "size", "ncallers"]
        for r in range(1, topk + 1):
            hdr += ["top%d_addr" % r, "top%d_score" % r]
        fh.write("\t".join(hdr) + "\n")
        for va, size, ncal, r in rows:
            f = ["0x%x" % va, str(size), str(ncal)]
            for j in range(topk):
                if j < len(r):
                    f += ["0x%x" % r[j][1], "%.17g" % r[j][0]]
                else:
                    f += ["-", "-"]
            fh.write("\t".join(f) + "\n")


def read_rows(path):
    out = {}
    with open(path, encoding="utf-8") as fh:
        hdr = None
        for line in fh:
            if line.startswith("#") or not line.strip():
                continue
            p = line.rstrip("\n").split("\t")
            if hdr is None:
                hdr = p
                continue
            out[int(p[0], 16)] = p
    return out


# ===========================================================================
# 3. GPU：切块 + 稀疏内积 + rank 配对核
# ===========================================================================
def _cum_excl(cnt, torch):
    return torch.cat([torch.zeros(1, dtype=torch.int64, device=cnt.device),
                      torch.cumsum(cnt, 0)[:-1]])


def _expand(start, cnt, torch):
    """`repeat_interleave(start, cnt)` + 行内偏移 ⇒ 每个 posting 元素在全局数组里的下标。"""
    tot = int(cnt.sum())
    if tot == 0:
        return None
    return torch.repeat_interleave(start, cnt, output_size=tot) + \
        (torch.arange(tot, device=cnt.device, dtype=torch.int64) -
         torch.repeat_interleave(_cum_excl(cnt, torch), cnt, output_size=tot))


def _gather_csr(flat, off, b0, b1, qn, torch):
    idx = _expand(off[b0:b1], qn, torch)
    return None if idx is None else flat[idx]


def _sparse_intersect(q_flat, q_off, b0, b1, pk, pk_col, clo, C, wl, wh, torch):
    """`I = A_multi-hot @ B_multi-hotᵀ` 的等价稀疏算式（只走非零 ⇒ 与稠密 GEMM 逐位相同）。

    `pk` 是池侧 (vid,col) 的**全局有序键**，所以 `(vid, col∈[lo,hi))` 的 posting 命中
    是一段连续下标：两个 searchsorted 拿到 `[start,end)`，repeat_interleave 展开成
    (行,列) 对，最后 index_add 计数。计数是小整数，float64 累加**与累加顺序无关**。
    """
    dev = q_flat.device
    qn = q_off[b0 + 1:b1 + 1] - q_off[b0:b1]
    SHIFT = 20
    vid = _gather_csr(q_flat, q_off, b0, b1, qn, torch)
    if vid is None:
        return None
    tot_q = int(vid.numel())
    row = torch.repeat_interleave(torch.arange(b0, b1, device=dev, dtype=torch.int64),
                                  qn, output_size=tot_q)
    base = vid << SHIFT
    # ⚠ `wl`/`wh` 是**块内**列号，而 `pk` 的列字段是**池全局**列号 ⇒ 查键前要加回 clo。
    # （漏了这一步时，只有 clo==0 的块恰好对，别的块一律查空——2026-11 实测的坑。）
    s = torch.searchsorted(pk, base | (wl[row - b0] + clo), right=False)
    e = torch.searchsorted(pk, base | (wh[row - b0] + clo), right=False)
    cnt = e - s
    del base, e
    I = torch.zeros((b1 - b0) * C, dtype=torch.float64, device=dev)
    cum = torch.cumsum(cnt, 0)
    tot = int(cum[-1]) if cum.numel() else 0
    # 展开是这一路里唯一会爆显存的中间量（尾部的大函数 posting 段很长）⇒ **按条数切成
    # ≤`_CH` 条的若干段**再累加，峰值显存从 GB 级降到几十 MB，结果不变（计数是精确整数）。
    i0 = 0
    nlit = int(cnt.numel())
    while i0 < nlit:
        base_cum = 0 if i0 == 0 else int(cum[i0 - 1])
        tgt = base_cum + _CH
        i1 = int(torch.searchsorted(
            cum, torch.tensor(tgt, device=dev, dtype=cum.dtype), right=False)) + 1
        i1 = min(max(i1, i0 + 1), nlit)
        seg_s, seg_cnt = s[i0:i1], cnt[i0:i1]
        idx = _expand(seg_s, seg_cnt, torch)
        if idx is not None:
            # 展开后的每一条命中都归它出发的那一行（`row` 按 posting 段长度再次展开）
            row_exp = torch.repeat_interleave(row[i0:i1], seg_cnt, output_size=idx.numel())
            I.index_add_(0, (row_exp - b0) * C + (pk_col[idx] - clo),
                         torch.ones(idx.numel(), dtype=torch.float64, device=dev))
            del row_exp
        i0 = i1
        if tot and i0 >= nlit:
            break
    return I.view(b1 - b0, C)


def log_ulp_probe(torch, q, p, n=200000, seed=0):
    """CUDA 的 `log()` 与 glibc 的 `math.log()` 在 float64 上**是否逐位相同**。

    为什么值得单独测：GPU 全量 top-1 与 CPU **13,263/13,263 完全一致**，只有 2 行的
    第 2–4 名互换了位置。那 2 行里同一个候选的分数差 **1 ulp**（1.1e-16）——两个候选在
    精确算术下**完全并列**，CPU 侧因 1 ulp 差异按分数排，GPU 侧因并列按地址排 ⇒ 顺序不同。
    所以这不是实现 bug。这里把"log 差 1 ulp"这件事直接测出来当证据。
    """
    import math
    import random
    rnd = random.Random(seed)
    pool_sizes = [int(x) for x in p.sizes.tolist()]
    q_sizes = [int(x) for x in q.sizes.tolist()]
    rs = []
    for _ in range(n):
        a = rnd.choice(q_sizes)
        b = rnd.choice(pool_sizes)
        rs.append((b + 1.0) / (a + 1.0))
    g = torch.log(torch.tensor(rs, dtype=torch.float64, device="cuda")).cpu().tolist()
    exact = ulp1 = big = 0
    maxu = 0.0
    for r, gv in zip(rs, g):
        cv = math.log(r)
        if gv == cv:
            exact += 1
            continue
        d = abs(gv - cv)
        u = d / math.ulp(cv) if math.ulp(cv) else 0.0
        maxu = max(maxu, u)
        if u <= 1.0001:
            ulp1 += 1
        else:
            big += 1
    return dict(n=n, exact_same=exact, differ_1ulp=ulp1, differ_more=big,
                max_ulp_diff=maxu, pct_exact=round(100.0 * exact / n, 3))


def dense_vs_sparse(torch, q, p, wlo, whi, V, b0=-1, rows=64, ncol=4096):
    """用**真·稠密** `A_multi-hot @ B_multi-hotᵀ`（一次 GEMM）对拍稀疏算式。

    任务书 §1.2 的原话就是"A @ Bᵀ 用同一套切块"。稠密版在这里**不能用于全量**：
    池侧稠密矩阵是 41,512 × 51,985（8.6 GB fp32）+ 13,263 × 41,512 的查询侧，
    2013 = 5.7e13 flop，4060 上约 3800 s，而且 99.9% 的元素是 0。所以生产路径走稀疏，
    这里抽一块（默认 64 行 × 4096 列）**证明两者逐位相同**，否则"等价"就是空话。
    """
    n = len(q.sizes)
    if b0 < 0:                                     # 自动挑"候选窗口最宽"的那 64 行，覆盖面最大
        best, pick = -1, 0
        for i in range(0, max(1, n - rows + 1), 64):
            c = int(whi[i:i + rows].max() - wlo[i:i + rows].min())
            if c > best:
                best, pick = c, i
        b0 = pick
    b0 = max(0, min(b0, n - 1))
    b1 = min(n, b0 + rows)
    rows = b1 - b0
    clo = int(wlo[b0:b1].min())
    chi = int(whi[b0:b1].max())
    C = chi - clo
    nc = min(C, ncol)
    dev = "cpu"
    # 稠密 A（rows × V）
    qn = q.g_off[b0 + 1:b1 + 1] - q.g_off[b0:b1]
    tot = int(qn.sum())
    vids = q.g_flat[q.g_off[b0]:q.g_off[b1]]
    row_of = torch.repeat_interleave(torch.arange(rows, dtype=torch.int64), qn,
                                     output_size=tot)
    A = torch.zeros((rows, V), dtype=torch.float32)
    A[row_of, vids] = 1.0
    # 稠密 B（V × nc）：先挑出落在 [clo, clo+nc) 的非零
    pcols = torch.repeat_interleave(torch.arange(len(p.sizes), dtype=torch.int64),
                                    p.g_off[1:] - p.g_off[:-1], output_size=int(p.g_off[-1]))
    m = (pcols >= clo) & (pcols < clo + nc)
    B = torch.zeros((V, nc), dtype=torch.float32)
    B[p.g_flat[m], pcols[m] - clo] = 1.0
    dense = (A @ B).double()                       # ← 任务书要的那一次大 GEMM
    # 稀疏算式（生产路径）
    SHIFT = 20
    col_all = torch.arange(len(p.sizes), dtype=torch.int64)
    key = (p.g_flat << SHIFT) | pcols
    order = torch.argsort(key)
    pk, pk_col = key[order], pcols[order]
    sp = _sparse_intersect(q.g_flat, q.g_off, b0, b1, pk, pk_col, clo, C,
                           wlo[b0:b1] - clo, whi[b0:b1] - clo, torch)
    # ⚠ 只比**行窗口内**的格子：`_sparse_intersect` 会按每行的 [lo,hi) 掩掉窗口外的列，
    # 而稠密 GEMM 不掩 —— 不掩就会把"窗口外的命中"也算进去，得到假的不一致。
    cols_i = torch.arange(nc)
    inwin = (cols_i[None, :] >= (wlo[b0:b1] - clo)[:, None]) & \
            (cols_i[None, :] < (whi[b0:b1] - clo)[:, None])
    diff = torch.where(inwin, (dense - sp[:, :nc]).abs(), torch.zeros(1))
    return dict(rows=rows, cols=int(nc), vocab=int(V), block=(b0, b1),
                compared_cells=int(inwin.sum()), max_abs_diff=float(diff.max()),
                mismatches=int((diff > 0).sum()),
                dense_shape=[rows, int(nc)], nonzero_A=int(A.sum()), nonzero_B=int(B.sum()))


def gpu_topk(torch, q, p, wlo, whi, topk=TOPK, max_cells=MAXCELLS, args_profile=False):
    """返回 `[(linux_addr, size, ncallers, [(score, win_addr), ...]), ...]`（与 CPU 同序）。

    块内 `(R, C)` = 行 × 候选列，四个特征各算一遍再融合：

        S  = 1 - min(|log((sd+1)/(sq+1))|/eps, 1)           连续
        Ig = A_gram @ B_gramᵀ, Ic = A_consts @ B_constsᵀ    集合（multi-hot 内积）
        St = Σ_{t<min(nq,nd)} 核(uq[t], ud[t])               rank 配对

    融合的加减顺序与 `CF.score_same` 里 `add()` 的**调用顺序逐字对应**：浮点加法不满足
    结合律，换个顺序就会在 1e-16 量级分叉，最终可能翻转 top-5。
    """
    dev = "cuda"
    props = torch.cuda.get_device_properties(0)
    F64 = torch.float64

    def f64(x):
        return x.to(F64)

    prof = collections.OrderedDict((k, 0.0) for k in
                                   ("size", "sparse", "caller", "fuse", "topk", "other"))
    _last = [time.perf_counter()]

    def _mark(k):
        nonlocal _last
        if args_profile:
            torch.cuda.synchronize()
            now = time.perf_counter()
            prof[k] += now - _last[0]
            _last[0] = now
        else:
            _last[0] = time.perf_counter()

    stats = dict(device=props.name, capability="%d.%d" % (props.major, props.minor),
                 total_MB=round(props.total_memory / 2**20, 1),
                 torch=torch.__version__,
                 tf32=bool(torch.backends.cuda.matmul.allow_tf32))

    # ---- 池侧（Windows）常量驻显存；multi-hot 压成**全局有序键** ----
    psizes = p.sizes.to(dev)
    pnq = p.nq.to(dev)
    pg_off, pg_flat = p.g_off.to(dev), p.g_flat.to(dev)
    pc_off, pc_flat = p.c_off.to(dev), p.c_flat.to(dev)
    pcal_off, pcal_flat = p.cal_off.to(dev), p.cal_flat.to(dev)
    cols_all = torch.arange(len(psizes), device=dev, dtype=torch.int64)
    SHIFT = 20                                   # 列号 < 2^20（51,985 ✓），vid < 2^20
    assert len(psizes) < (1 << SHIFT)

    def _sorted_keys(flat, off):
        """(vid, col) → 全局有序键 + **排序后**的列号。⚠ `torch.sort` 返回的是
        (值, 下标)，下标不是列号——必须用它去 gather 原列号，否则计数会散布到错列。"""
        col = torch.repeat_interleave(cols_all, off[1:] - off[:-1],
                                      output_size=int(off[-1]))
        key = (flat << SHIFT) | col
        order = torch.argsort(key)
        return key[order], col[order]

    pk, pk_col = _sorted_keys(pg_flat, pg_off)
    ck, ck_col = _sorted_keys(pc_flat, pc_off)
    del cols_all

    # caller 多重集只在 nd>0 的列上非零（`multiset_sim` 对空集直接返回 0.0）⇒ 列压到
    # "有调用者的那一批"（51,985 → 15,170）；仍是 size 序 ⇒ 块内是连续区间，可二分。
    act = torch.nonzero(pnq > 0).flatten()
    act_list = act.tolist()

    qg_off, qg_flat = q.g_off.to(dev), q.g_flat.to(dev)
    qc_off, qc_flat = q.c_off.to(dev), q.c_flat.to(dev)
    qcal_off, qcal_flat = q.cal_off.to(dev), q.cal_flat.to(dev)
    qnq_all = q.nq.to(dev)
    qaddrs = q.addrs
    qsizes = q.sizes

    nq_rows = len(qsizes)
    # 自适应分块：保证每块 R×C ≤ MAXCELLS。尾部（大函数）的窗口极宽，死用 512 行会爆显存。
    blocks = []
    i = 0
    while i < nq_rows:
        lo, hi, j = int(wlo[i]), int(whi[i]), i + 1
        while j < nq_rows:
            nlo, nhi = min(lo, int(wlo[j])), max(hi, int(whi[j]))
            if (j + 1 - i) * (nhi - nlo) > max_cells and j > i:
                break
            lo, hi = nlo, nhi
            j += 1
        blocks.append((i, j))
        i = j
    sys.stderr.write("[gpu] 分块 %d 个（每块 R×C ≤ %d）\n" % (len(blocks), max_cells))
    out = []
    t0 = time.perf_counter()
    peak = 0.0
    ambig = 0
    maxC = 0
    for bi, (b0, b1) in enumerate(blocks):
        R = b1 - b0
        clo = int(wlo[b0:b1].min())
        chi = int(whi[b0:b1].max())
        C = chi - clo
        maxC = max(maxC, C)
        if C <= 0:
            for i in range(b0, b1):
                out.append((int(qaddrs[i]), int(qsizes[i]), int(qnq_all[i]), []))
            continue
        cols = torch.arange(C, device=dev)
        addr_blk = p.addrs[clo:chi]
        wl = wlo[b0:b1].to(dev) - clo
        wh = whi[b0:b1].to(dev) - clo
        in_win = (cols[None, :] >= wl[:, None]) & (cols[None, :] < wh[:, None])

        sq = f64(qsizes[b0:b1].to(dev))
        sd = f64(psizes[clo:chi])

        _mark("other")
        # ① size —— 与 CPU 同一条公式（先算比值再取 log）
        ratio = (sd[None, :] + 1.0) / (sq[:, None] + 1.0)
        S = 1.0 - torch.minimum(torch.abs(torch.log(ratio)) / EPS,
                                torch.ones_like(ratio))
        del ratio

        _mark("size")
        # ② 集合段 —— I = A @ Bᵀ
        Ig = _sparse_intersect(qg_flat, qg_off, b0, b1, pk, pk_col, clo, C, wl, wh, torch)
        Ic = _sparse_intersect(qc_flat, qc_off, b0, b1, ck, ck_col, clo, C, wl, wh, torch)

        _mark("sparse")
        # ③ caller —— rank 配对核（t 升序累加，与 CPU 的 Σ 顺序一致）
        St = torch.zeros((R, C), dtype=F64, device=dev)
        alo = bisect.bisect_left(act_list, clo)
        ahi = bisect.bisect_left(act_list, chi)   # chi 是**开区间**端点，不能用 bisect_right
        nq_blk = qnq_all[b0:b1]
        if ahi > alo and int(nq_blk.max()) > 0:
            acol_g = act[alo:ahi]                          # **池全局**列号（查 p.* 用这个）
            acol = acol_g - clo                            # 块内列号（写 St 用这个）
            Sc = torch.zeros((R, ahi - alo), dtype=F64, device=dev)
            # 行按 nq 降序 ⇒ 第 t 轮参与的行是前缀；总工作量 = Σ nq（实测 3.4e4 行·rank）
            order = torch.argsort(nq_blk, descending=True)
            nq_sorted = nq_blk[order]
            nd_acol = pnq[acol_g]
            for t in range(int(nq_sorted[0])):
                cnt = int((nq_sorted > t).sum())
                if cnt == 0:
                    break
                ok = nd_acol > t
                if not bool(ok.any()):
                    continue
                ridx = order[:cnt]
                # idx 可能越过后缀边界（该列调用者数 ≤ t），但那些位置随后被 `ok` 掩掉；
                # 不 clamp 的话 CUDA 会直接 device-side assert（2026-11 实测）。
                uq = f64(qcal_flat[torch.clamp(qcal_off[b0 + ridx] + t, max=qcal_flat.numel() - 1)])
                ud = f64(pcal_flat[torch.clamp(pcal_off[acol_g] + t, max=pcal_flat.numel() - 1)])
                r2 = (ud[None, :] + 1.0) / (uq[:, None] + 1.0)
                tent = 1.0 - torch.minimum(torch.abs(torch.log(r2)) / EPS,
                                           torch.ones_like(r2))
                Sc[ridx] += torch.where(ok[None, :], tent, torch.zeros_like(tent))
            St[:, acol] = Sc
            del Sc, order, nq_sorted, nd_acol

        _mark("caller")
        # ④ 融合 —— 加减顺序与 score_same 的 add() 顺序逐字一致
        tot = W["size"] * S
        wt = torch.full((R, C), W["size"], dtype=F64, device=dev)
        if Ic is not None:
            qc = f64(qc_off[b0 + 1:b1 + 1] - qc_off[b0:b1])
            pc = f64(pc_off[clo + 1:chi + 1] - pc_off[clo:chi])
            hasC = (qc[:, None] > 0) & (pc[None, :] > 0)
            den = qc[:, None] + pc[None, :] - Ic
            Cv = torch.where(hasC, Ic / torch.clamp(den, min=1.0), torch.zeros_like(Ic))
            tot = tot + W["consts"] * Cv
            wt = wt + W["consts"] * f64(hasC)
            del hasC, den, Cv
        del Ic
        qn = f64(nq_blk)
        pn = f64(pnq[clo:chi])
        nmin = torch.minimum(qn[:, None], pn[None, :])
        pr = St / torch.clamp(nmin, min=1.0)
        cr = (pn[None, :] + 1.0) / (qn[:, None] + 1.0)
        cn = 1.0 - torch.minimum(torch.abs(torch.log(cr)) / EPS, torch.ones_like(cr))
        Ms = 0.5 * pr + 0.5 * cn
        Mv = torch.where((qn[:, None] > 0) & (pn[None, :] > 0), Ms,
                         torch.zeros_like(Ms))
        tot = tot + W["caller"] * Mv
        wt = wt + W["caller"]
        del St, pr, Ms, nmin, cr, cn, Mv
        if Ig is not None:
            qg = f64(qg_off[b0 + 1:b1 + 1] - qg_off[b0:b1])
            pg = f64(pg_off[clo + 1:chi + 1] - pg_off[clo:chi])
            den = qg[:, None] + pg[None, :] - Ig
            Gv = torch.where(den > 0, Ig / torch.clamp(den, min=1.0),
                             torch.zeros_like(Ig))
            tot = tot + W["gram"] * Gv
            wt = wt + W["gram"]
            del den, Gv
        del Ig, S
        score = tot / wt
        del tot, wt
        score = torch.where(in_win, score, torch.full_like(score, -1e300))

        _mark("fuse")
        # ⑤ top-K：精确的 (-score, 地址) 序，全程向量化。
        #    `kth` = 第 K 大的**值** ⇒ 严格大于它的候选最多 K-1 个（一定进 top-K），
        #    等于它的可能一大片（并列）⇒ 取其中**地址最小**的 K 个。
        #    两者并起来按 (-score, addr) 排，就是 CPU `sc.sort(key=(-score, addr))` 的前 K 名，
        #    与并列规模无关。全量 13,263 行里 5,165 行有 5 名并列 —— 逐行 Python 会吃掉 10 s。
        kk = min(topk, C)
        kth = torch.topk(score, k=kk, dim=1)[0][:, kk - 1]
        eq = in_win & (score == kth[:, None])
        gt = in_win & (score > kth[:, None])
        ambig += int(((eq.sum(dim=1)) > 1).sum())
        NEG = torch.full_like(score, -1e300)
        BIG = torch.iinfo(torch.int64).max
        addr_c = addr_blk.to(dev)
        eqa = torch.where(eq, addr_c[None, :], torch.full_like(addr_c[None, :], BIG))
        sm, eqi = torch.topk(eqa, k=kk, dim=1, largest=False)
        gtv, gti = torch.topk(torch.where(gt, score, NEG), k=kk, dim=1)
        sc_eq = torch.gather(score, 1, eqi)
        sm_l, eqi_l = sm.cpu().tolist(), eqi.cpu().tolist()
        gtv_l, gti_l = gtv.cpu().tolist(), gti.cpu().tolist()
        sc_eq_l = sc_eq.cpu().tolist()
        addrs_blk = addr_blk.tolist()
        NEGv = -1e299
        for r in range(R):
            cand = [(gtv_l[r][j], addrs_blk[gti_l[r][j]])
                    for j in range(kk) if gtv_l[r][j] > NEGv]
            for j in range(kk):
                if sm_l[r][j] != BIG:
                    cand.append((sc_eq_l[r][j], addrs_blk[eqi_l[r][j]]))
            cand.sort(key=lambda u: (-u[0], u[1]))
            i = b0 + r
            out.append((int(qaddrs[i]), int(qsizes[i]), int(qnq_all[i]), cand[:topk]))
        _mark("topk")
        del score, eq, gt, eqa, sm, eqi, gtv, gti, sc_eq, NEG, kth, in_win
        peak = max(peak, torch.cuda.max_memory_allocated() / 2**20)
        if bi % 8 == 0:
            torch.cuda.synchronize()
            sys.stderr.write("[gpu] %d/%d  %.1f s  peak %.0f MB  maxC %d\n"
                             % (b1, nq_rows, time.perf_counter() - t0, peak, maxC))
    torch.cuda.synchronize()
    dt = time.perf_counter() - t0
    if args_profile:
        prof["other"] += dt - sum(prof.values())
        stats["profile_s"] = {k: round(v, 4) for k, v in prof.items()}
    stats.update(wall_s=round(dt, 4),
                 peak_alloc_MB=round(max(peak, torch.cuda.max_memory_allocated() / 2**20), 1),
                 rows=nq_rows, max_columns=maxC, ambiguous_rows=ambig,
                 candidate_pairs=int(sum(int(whi[i] - wlo[i]) for i in range(nq_rows))))
    return out, stats, dt


def cmd_gpu(args):
    import torch
    if not torch.cuda.is_available():
        sys.stderr.write("[gpu] CUDA 不可用（torch %s）\n" % torch.__version__)
        return 2
    torch.manual_seed(0)
    torch.backends.cuda.matmul.allow_tf32 = False     # 显式关掉：内积要**精确整数**
    fps = CF.load_all(tags=["win", "linux"])
    qs = sorted(fps["linux"].universe,
                key=lambda va: (fps["linux"].feats[va]["size"], va))
    if args.limit:
        qs = qs[:args.limit]
    t_m = time.perf_counter()
    q, p, gv, cv = build_matrices(torch, fps, qs)
    wlo, whi = window_bounds(q.sizes, p.sizes, torch)
    t_m = time.perf_counter() - t_m
    F = len(CONT_NAMES) + len(gv) + len(cv)
    sys.stderr.write("[gpu] 物化 %.1f s：Linux %d × F=%d（连续 %d + gram %d + consts %d），"
                     "Windows %d × F=%d\n"
                     % (t_m, len(q.sizes), F, len(CONT_NAMES), len(gv), len(cv),
                        len(p.sizes), F))
    if args.selftest:
        st = dense_vs_sparse(torch, q, p, wlo, whi, len(gv))
        st["log_ulp"] = log_ulp_probe(torch, q, p)
        sys.stderr.write("[selftest] 稠密 A@Bᵀ vs 稀疏：%s\n" % json.dumps(st))
        if args.report:
            with open(args.report + ".selftest.json", "w", encoding="utf-8") as fh:
                json.dump(st, fh, indent=2, sort_keys=True)
    rows, stats, dt = gpu_topk(torch, q, p, wlo, whi, args.topk, args.max_cells,
                               args_profile=args.profile)
    stats.update(materialize_s=round(t_m, 4), total_s=round(t_m + dt, 4),
                 feature_cols=F, vocab_gram=len(gv), vocab_consts=len(cv),
                 nnz_gram_linux=int(q.g_off[-1]), nnz_gram_win=int(p.g_off[-1]),
                 nnz_consts_linux=int(q.c_off[-1]), nnz_consts_win=int(p.c_off[-1]),
                 nnz_caller_linux=int(q.cal_off[-1]), nnz_caller_win=int(p.cal_off[-1]),
                 cont_cols=list(CONT_NAMES), active_features=list(ACTIVE))
    write_rows(args.out, rows, args.topk)
    if args.report:
        with open(args.report, "w", encoding="utf-8") as fh:
            json.dump(stats, fh, indent=2, sort_keys=True)
    print(json.dumps(stats, indent=2, sort_keys=True))
    return 0


# ===========================================================================
# 4. 对拍：GPU vs CPU vs 夹具
# ===========================================================================
def cmd_merge(args):
    """把分片跑出来的 top-K TSV 合并成一份（多进程 CPU 基线用）。

    分片是按行跨步切的，每片自带表头 ⇒ 不能用 `cat` 拼（表头会被当数据行）。
    """
    out = {}
    for f in args.files:
        out.update(read_rows(f))
    rows = [(a, int(p[1]), int(p[2]),
             [(float(p[4 + 2 * j]), int(p[3 + 2 * j], 16))
              for j in range((len(p) - 3) // 2) if p[3 + 2 * j] != "-"])
            for a, p in sorted(out.items())]
    write_rows(args.out, rows, args.topk)
    print(json.dumps(dict(merged_rows=len(rows), shards=len(args.files)), sort_keys=True))
    return 0


def cmd_check(args):
    cpu = read_rows(args.cpu)
    gpu = read_rows(args.gpu)
    topk = (len(next(iter(cpu.values()))) - 3) // 2
    fps = CF.load_all(tags=["linux"])
    ctrl = CF.named_linux_348()
    tgt = set(CF.target_addrs(fps))
    rep = dict(rows_cpu=len(cpu), rows_gpu=len(gpu), topk=topk,
               rows_ctrl348=len([a for a in cpu if a in ctrl]),
               rows_target=len([a for a in cpu if a in tgt]))

    def cmp(addrset, label):
        n = t1 = t5 = 0
        bad = []
        dmax = 0.0
        for a in sorted(addrset):
            if a not in cpu or a not in gpu:
                continue
            n += 1
            c, g = cpu[a], gpu[a]
            if c[3] == g[3]:
                t1 += 1
            if [c[3 + 2 * j] for j in range(topk)] == [g[3 + 2 * j] for j in range(topk)]:
                t5 += 1
            else:
                bad.append(a)
            for j in range(topk):
                if c[4 + 2 * j] != "-" and g[4 + 2 * j] != "-":
                    dmax = max(dmax, abs(float(c[4 + 2 * j]) - float(g[4 + 2 * j])))
        rep[label] = dict(n=n, top1_same=t1, top5_same=t5, top5_mismatch=n - t5,
                          max_abs_dscore=dmax,
                          mismatch_addrs=["0x%x" % a for a in bad[:20]])

    if ctrl:
        cmp(ctrl, "ctrl348")
    if tgt:
        cmp(tgt, "target12736")
    cmp(set(cpu) & set(gpu), "all")

    if args.controls_out:
        # 348 个已定名样本的**逐条**对照表（入库文本：这是"GPU 与 CPU 同口径"的证据本体）
        emit = sorted(ctrl) if ctrl else sorted(set(cpu) & set(gpu))
        with open(args.controls_out, "w", encoding="utf-8") as fh:
            fh.write("# GPU vs CPU 的 top-K 逐条对照（%d 行）。\n" % len(emit))
            fh.write("# cpu_* = callgraph_fingerprint.rank()（发布版 CPU 代码本体）；"
                     "gpu_* = callgraph_gpu_sim.py gpu（torch/CUDA）。\n")
            fh.write("# 348 = 三份夹具的名字并集 ∩ 引用 src/*.rs Location 的 527 个函数"
                     "（真值来自编译期常量，与打分器无关）。\n")
            fh.write("# same_top1/same_top5 逐行判等（地址序列完全一致才算 same_top5）；"
                     "max_abs_dscore 是两边同名列分数的最大绝对差。\n")
            hdr = ["linux_addr", "size", "ncallers"]
            hdr += ["cpu_top%d_addr" % r for r in range(1, topk + 1)]
            hdr += ["cpu_top%d_score" % r for r in range(1, topk + 1)]
            hdr += ["gpu_top%d_addr" % r for r in range(1, topk + 1)]
            hdr += ["gpu_top%d_score" % r for r in range(1, topk + 1)]
            hdr += ["same_top1", "same_top5", "max_abs_dscore"]
            fh.write("\t".join(hdr) + "\n")
            for a in emit:
                c, g = cpu.get(a), gpu.get(a)
                if not c or not g:
                    continue
                ca = [c[3 + 2 * j] for j in range(topk)]
                ga = [g[3 + 2 * j] for j in range(topk)]
                cs = [c[4 + 2 * j] for j in range(topk)]
                gs = [g[4 + 2 * j] for j in range(topk)]
                d = max([abs(float(x) - float(y)) for x, y in zip(cs, gs)
                         if x != "-" and y != "-"] or [0.0])
                fh.write("\t".join([c[0], c[1], c[2]] + ca + cs + ga + gs +
                                   ["1" if ca[0] == ga[0] else "0",
                                    "1" if ca == ga else "0", "%.3g" % d]) + "\n")
    if os.path.exists(OUT_FIXTURE):
        fx = {}
        with open(OUT_FIXTURE, encoding="utf-8") as fh:
            hdr = None
            for line in fh:
                if line.startswith("#") or not line.strip():
                    continue
                p2 = line.rstrip("\n").split("\t")
                if hdr is None:
                    hdr = p2
                    continue
                d = dict(zip(hdr, p2))
                fx[int(d["linux_addr"], 16)] = d
        n = ag = agq = 0
        for a, d in fx.items():
            if a not in gpu:
                continue
            n += 1
            if d["match_addr"] == gpu[a][3]:
                ag += 1
                if d["score"] != "-" and gpu[a][4] != "-" and \
                        abs(float(d["score"]) - float(gpu[a][4])) <= 0.00005:
                    agq += 1
        rep["fixture_top1"] = dict(n=n, addr_same=ag, score4dp_same=agq,
                                   score4dp_diff=ag - agq)
    print(json.dumps(rep, indent=2, sort_keys=True))
    if args.report:
        with open(args.report, "w", encoding="utf-8") as fh:
            json.dump(rep, fh, indent=2, sort_keys=True)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n")[0])
    ap.add_argument("cmd", choices=["cpu", "gpu", "check", "merge"])
    ap.add_argument("--out", default="")
    ap.add_argument("--report", default="")
    ap.add_argument("--cpu", default="")
    ap.add_argument("--gpu", default="")
    ap.add_argument("--controls-out", default="",
                    help="把 348 个正对照的 CPU vs GPU 逐条对照写成 TSV（入库文本）")
    ap.add_argument("--topk", type=int, default=TOPK)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--shard", default="", help="CPU 基线分片 i/N（多进程并行用）")
    ap.add_argument("files", nargs="*", help="merge 的输入分片")
    ap.add_argument("--max-cells", type=int, default=MAXCELLS, dest="max_cells",
                    help="分块的 R×C 上限（显存旋钮；默认 3e6）")
    ap.add_argument("--profile", action="store_true", help="分阶段计时（size/sparse/caller/fuse/topk）")
    ap.add_argument("--selftest", action="store_true",
                    help="用真·稠密 A@Bᵀ 抽一块对拍稀疏内积（证明两者逐位相同）")
    ap.add_argument("--tools", default="", help="tools/ 目录（脚本被复制到别处执行时用）")
    ap.add_argument("--cache", default="", help="指纹缓存目录（= CFGFP_CACHE）")
    ap.add_argument("--reference-root", default="",
                    help="仓库根（定位 .reference/ 下四份构建；只读文件大小算缓存键，不重扫）")
    args = ap.parse_args(argv)
    if not os.environ.get("CFGFP_CACHE") and not args.cache:
        sys.stderr.write("[warn] 没设 CFGFP_CACHE / --cache ⇒ 会重扫（20+ min）\n")
    if args.cmd == "cpu":
        return cmd_cpu(args)
    if args.cmd == "gpu":
        return cmd_gpu(args)
    if args.cmd == "merge":
        return cmd_merge(args)
    return cmd_check(args)


if __name__ == "__main__":
    sys.exit(main())
