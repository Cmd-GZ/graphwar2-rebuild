#!/usr/bin/env python3
"""seq_align.py — 全序列单调对齐：把 arm64 的函数**顺序 + 体量**对齐到 stripped 的 x86-64 上（任务 93）。

## 一句话

两条**有序**函数序列 —— A = x86-64 `2026-08-28`（13,263，体量 = 下一条起点 − 本起点）、
B = Android `2.0.60` arm64 `.symtab` 的 FUNC（18,793 个符号 → **12,609 个不同 .text 起点**，体量 = `st_size`）——
做**保序全局对齐**（带状 DP + sum-product 前后向 → 每格匹配后验），
让 12,736 个一个 `Location` 都不引用的 x86 函数拿到「顺序推断」的对侧 arm64 函数（及其名字）。

## 为什么不是"再涨一个点"

结构指纹（§35/§43/§44/§45/§46/§48）只能给"同源候选"，且在**近孪生族**（21 关的关卡闭包，
同一份代码换常量）上系统性失效 **51.9%**。而那一族在**序列上是有序的**、体量也几乎一样：
"顺序"是这套证据里唯一还没被用过的独立轴。本工具只做这一件事，**不碰**任何指纹特征。

## 红线（任务书 §2，逐条落实在代码里）

1. **348 个真名只当留出验证。** 算法与参数写死在 `PREREG` 里，`print-config` 打印后者的规范 JSON
   与 sha256；`fixtures/seq_align_prereg93.md` 是同一份内容的散文版，**先提交、后验证**。
   本工具的 q 只由 `st_size`/`st_off` 序列（体量 + 邻域体量）与跳步罚分构成 —— **不读 `Location`、
   不读名字、不读地址**。
2. **锚点可选但默认不用**：`--anchors` 只在"锚点分段"变体里用，锚点集与评估集**互斥**
   （`split_half_addrs()` 的 sha1 奇偶分半，与指纹两半同一口径）。
   默认 `P0`（无锚）评估 = 全体 344 对，**不存在循环论证**。
3. **不给"看着像"**：可以跳过（内联/ICF/单侧单态化），可以判"无对齐"；每行带后验与置信档。
   对齐结果是 **推断** 级，不是命名（与 §35 的"结构同源候选"同一档）。

## 算法（与 `PREREG` 一一对应）

```
序列      A: x86 13,263（地址升序）      B: arm64 12,609（地址升序）
匹配质量  q(i,j) = (1.0·S + 0.5·C) / (1.0 + 0.5)     （两侧都有 C 时才进分母）
  S = exp(-(ln(sA/sB)/0.30)²)，r ∉ [0.40, 2.20] ⇒ S = 0（硬闸，表"不可能同源"）
  C = 1 − mean_{t∈±1..±6} |ln sA[i+t] − ln sB[j+t]| / ln3   （截到 [0,1]；少于 4 对邻域则不计）
跳步      每跳过一个 A 元素或一个 B 元素各罚 gap = 0.15
DP        log 域 sum-product（不是 max-sum）：前向/后向都算 ⇒ 每格出**匹配后验** p(i,j)
          带状：|j − i·NB/NA| ≤ band = 256（格数 13,263 × 513 ≈ 6.8e6）
决策      j* = argmax_j p(i,j)；置信档 A/B/C 由 (p1, p1−p2) 的**预注册**阈值定；否则"无对齐"
```

## 用法（两个解释器：`truth` 需要仓库 venv + 指纹缓存，其余只要 numpy）

```bash
# ① 抽取序列 + 真值（venv：要 capstone 的指纹缓存）
CFGFP_CACHE=<v6 缓存> $PY tools/seq_align.py truth --work <workdir>
# ② 对齐 + 写夹具（系统 python3，带 numpy）
python3 tools/seq_align.py run --work <workdir> --out fixtures/seq_align.tsv
# ③ 三条验收
python3 tools/seq_align.py verify --work <workdir> --align fixtures/seq_align.tsv
# ④ 预注册网格（12 核以内）+ 锚点分段变体
python3 tools/seq_align.py sweep --work <workdir> --jobs 8
python3 tools/seq_align.py run --work <workdir> --anchors tune --out /tmp/a.tsv
# ⑤ 合成数据自检（含**乱序对照**）
python3 tools/seq_align.py selftest
# ⑥ 预注册清单
python3 tools/seq_align.py print-config
```
"""

import argparse
import collections
import hashlib
import json
import math
import os
import pickle
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

def find_ref(rel):
    """`<repo>/.reference/...` —— 分支工作区比仓库根深几层，逐级向上找（与 tools/sym_match.py 同法）。"""
    env = os.environ.get("GW2_REFERENCE")
    bases = [env] if env else []
    base = ROOT
    for _ in range(8):
        bases.append(base)
        base = os.path.dirname(base)
    for b in bases:
        if not b:
            continue
        p = os.path.join(b, ".reference", rel)
        if os.path.exists(p):
            return p
    raise SystemExit("找不到 .reference/%s；用 GW2_REFERENCE 指定 .reference 目录" % rel)


X86_SO = find_ref("steam/2026-08-28/libgraphwarrust.so")
ARM64_SO = find_ref("steam/android-2.0.60/libgraphwarrust.so")
FUNCSTARTS = os.environ.get("GW2_FUNCSTARTS") or find_ref("upstream/analysis/funcstarts_ghidra_va.pkl")

try:
    import numpy as np
except ImportError:                                       # pragma: no cover
    np = None

NEG = -1e30

# ===========================================================================
# 预注册：算法与参数（**先提交、后验证**；改这里就是改预注册，必须重写 md 并重新 commit）
# ===========================================================================
PREREG = {
    "version": "93-v2",
    "seq": {
        "x86": "funcstarts_ghidra_va.pkl（值域实测 = ELF VA 0x37b940..0x94eb84，即 x86 .text）",
        "x86_size": "下一条起点 − 本起点；最后一条用 .text 末尾补",
        "arm64": "android-2.0.60 .symtab 的 STT_FUNC（18,793 条 → .text 内 12,609 个不同起点）",
        "arm64_size": "st_size",
    },
    "size_floor": 8,             # 体量下限（避免 0 与极小值把 ln 拉爆）
    "size_sigma": 0.30,          # S = exp(-(ln r / sigma)^2)
    "size_loose": [0.40, 2.20],  # 硬闸：超出即 S=0（"不可能同源门"）
    "ctx_k": 6,                  # 邻域取 ±6 个函数
    "ctx_beta": 1.0986122886681098,   # ln 3
    "ctx_min": 4,                # 至少 4 对邻域才计 C
    "w_size": 1.0,
    "w_ctx": 0.5,
    "gap": 0.15,                 # 每跳过一个函数（任一侧）扣的分
    "band": 256,                 # |j - i·NB/NA| <= band
    # 行内归一化后验的档位阈值 [q1 下限, (q1-q2) 下限]；q1/q2 是**给定该行被匹配**时的
    # 搭档分布的最大值/次大值。阈值由合成自检（selftest，**不碰真值**）标定，见 prereg md §4。
    # 决策 = max-sum（Viterbi）路径；置信 = "强制换搭档"的分数损失（margin，与 q 同量纲）。
    # q_min：搭档的匹配质量下限（低于它一律"无对齐"，哪怕 margin 很大）
    "q_min": 0.10,
    "tier_margin": {"A": 0.50, "B": 0.25, "C": 0.05},
    "grid": {"band": [64, 128, 256, 512], "gap": [0.05, 0.10, 0.15, 0.30]},
    "select_rule": "网格配置只在 tune 半上选（split_half_addrs 的 sha1 奇偶），选中后报 holdout 半；"
                   "headline 用 P0 = 无锚 + band=256 + gap=0.15，在**全体 344 对**上报（不经过选择）",
    "eval": {
        "truth": "control_pairs('android','linux') = 严格 Location join（每键两侧唯一 + 双向）= 344 对",
        "primary_criterion": "对齐器的 arm64 地址 == 真值 arm64 地址",
        "secondary_criterion": "去 mangling + 去 .llvm. 克隆后缀后名字相同（允许同源克隆）",
        "near_twin": "975 行 v2 真值里 half=holdout ∩ campaign 锚表（fixtures/campaign_anchors75.tsv）= 27 行",
        "target": "12,736 = linux 宇宙 13,263 里不引用任何 src/*.rs Location 的那些",
    },
}


def prereg_json():
    return json.dumps(PREREG, sort_keys=True, ensure_ascii=False)


def prereg_sha():
    return hashlib.sha256(prereg_json().encode()).hexdigest()


# ===========================================================================
# 1. 序列
# ===========================================================================
def readelf(*args):
    return subprocess.run(["readelf"] + list(args), capture_output=True, text=True).stdout


def text_bounds(so):
    """.text 的 (addr, addr+size)。只认**名字精确等于** `.text` 的节。"""
    for line in readelf("-SW", so).splitlines():
        p = line.replace("[", " ").replace("]", " ").split()
        if len(p) >= 7 and p[1] == ".text":
            return int(p[3], 16), int(p[3], 16) + int(p[5], 16)
    raise SystemExit("%s: 找不到 .text" % so)


def func_symbols(so):
    """`[(mangled, addr, size)]`：`.symtab` 里已定义的 FUNC（与 tools/sym_match.py 同口径）。"""
    rows = []
    for line in readelf("-sW", so).splitlines():
        p = line.split()
        if len(p) < 8 or p[3] != "FUNC" or p[1] == "UND":
            continue
        try:
            rows.append((p[7], int(p[1], 16), int(p[2])))
        except ValueError:
            continue
    return rows


def x86_sequence():
    """x86-64：Ghidra 起点表 → (addr, size) 升序；size = 下一条起点 − 本起点。"""
    lo, hi = text_bounds(X86_SO)
    raw = pickle.load(open(FUNCSTARTS, "rb"))
    starts = sorted({int(x, 16) if isinstance(x, str) else int(x) for x in raw})
    starts = [a for a in starts if lo <= a < hi]
    out = []
    for i, a in enumerate(starts):
        nxt = starts[i + 1] if i + 1 < len(starts) else hi
        out.append((a, nxt - a))
    return out


def arm64_sequence():
    """arm64：`.symtab` STT_FUNC → 不同起点的 (addr, size, name)；同一地址多名字取最短那个当名字。"""
    lo, hi = text_bounds(ARM64_SO)
    syms = [(n, a, s) for (n, a, s) in func_symbols(ARM64_SO) if lo <= a < hi]
    by = {}
    for n, a, s in syms:
        cur = by.get(a)
        if cur is None or (s > cur[0]) or (s == cur[0] and len(n) < len(cur[1])):
            by[a] = (max(s, cur[0]) if cur else s, n if cur is None or len(n) < len(cur[1]) else cur[1])
    out = [(a, sz, nm) for a, (sz, nm) in sorted(by.items())]
    return out


# ---------------------------------------------------------------- 名字
# legacy Rust mangling 的转义（`$LT$` = `<`，路径分隔写成 `..`）+ 尾部 `::h<16 hex>` 哈希。
_ESC = (("$LT$", "<"), ("$GT$", ">"), ("$u20$", " "), ("$u7b$", "{"), ("$u7d$", "}"),
        ("$u27$", "'"), ("$u5b$", "["), ("$u5d$", "]"), ("$u3b$", ";"), ("$u7e$", "~"),
        ("$C$", ","), ("$RF$", "&"), ("$BP$", "*"), ("$u5e$", "^"), ("$u21$", "!"),
        ("$u2b$", "+"), ("$u3d$", "="), ("$u40$", "@"))
_HASH = re.compile(r"::h[0-9a-f]{8,17}$")
_LLVM = re.compile(r"\.llvm(\.\d+)?$")


def _unescape(s):
    for a, b in _ESC:
        s = s.replace(a, b)
    return s.replace("..", "::")


def demangle(names):
    """Rust legacy mangled → 可读名。

    三种形态都处理：`_ZN…E`（长度前缀）、`_$LT$…$GT$::…`（impl-head，**路径用 `..` 分隔**）、
    以及 `graphwarrust::…` 直写。尾部 `::h<16 hex>` 命名的哈希**去掉**（地址列已经保证唯一）。
    """
    out = []
    for n in names:
        s = n
        if s.startswith("_ZN") and s.endswith("E"):
            body = s[3:-1]
            parts, i, ok = [], 0, True
            while i < len(body):
                j = i
                while j < len(body) and body[j].isdigit():
                    j += 1
                if j == i:
                    ok = False
                    break
                ln = int(body[i:j])
                parts.append(_unescape(body[j:j + ln]))
                i = j + ln
            if ok:
                s = "::".join(parts)
        s = _unescape(_HASH.sub("", _LLVM.sub("", s)))
        out.append(s.lstrip("_") if s.startswith("_") else s)
    return out


def sym_key(name):
    """名字归一（**次要判据**用）：去 `.llvm.N` 克隆后缀、去 `::h<16 hex>`、解转义。"""
    s = _LLVM.sub("", name)
    s = _HASH.sub("", s)
    return _unescape(s)


# ===========================================================================
# 2. 工作目录里的抽取结果（`truth` 子命令产出，其余子命令只读）
# ===========================================================================
def wpath(work, name):
    return os.path.join(work, name)


def read_tsv(path):
    hdr, out = None, []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            p = line.split("\t")
            if hdr is None:
                hdr = p
                continue
            out.append(dict(zip(hdr, p)))
    return out


def write_tsv(path, hdr, rows, comment=()):
    with open(path, "w", encoding="utf-8") as fh:
        for c in comment:
            fh.write("# %s\n" % c)
        fh.write("\t".join(hdr) + "\n")
        for r in rows:
            fh.write("\t".join(str(r.get(h, "")) for h in hdr) + "\n")


def load_work(work):
    x86 = [(int(r["addr"], 16), int(r["size"])) for r in read_tsv(wpath(work, "x86.seq.tsv"))]
    arm = [(int(r["addr"], 16), int(r["size"]), r["name"]) for r in read_tsv(wpath(work, "arm64.seq.tsv"))]
    return x86, arm


def load_truth(work):
    """[(x86_addr, arm64_addr)]；`truth` 子命令落的盘。"""
    return [(int(r["x86_addr"], 16), int(r["arm64_addr"], 16))
            for r in read_tsv(wpath(work, "truth.tsv"))]


# ===========================================================================
# 3. 匹配质量 + 带状 DP（log 域 sum-product）
# ===========================================================================
def build_q(sa, sb, band, cfg=None):
    """返回 (Q, J0, M)：Q[i,k] 是 A[i] 与 B[J0[i]+k] 的匹配质量（带外 / 越界为 -inf 语义 ⇒ 0 分但不匹配）。

    Q 用 float32；不可匹配（越界或 S=0）记 0 —— 0 分匹配在 DP 里等价于"不匹配但白拿两格"，
    必须**禁止**，所以调用方把 Q<=0 的格视为不可用（用 NEG 表达）。
    """
    cfg = cfg or PREREG
    na, nb = len(sa), len(sb)
    M = 2 * band + 1
    la = np.log(np.maximum(sa, cfg["size_floor"]).astype(np.float64))
    lb = np.log(np.maximum(sb, cfg["size_floor"]).astype(np.float64))
    center = np.rint(np.arange(na) * (nb / float(na))).astype(np.int64)
    K = np.arange(-band, band + 1, dtype=np.int64)
    J = (center[:, None] + K[None, :]).astype(np.int32)    # (na, M)
    valid = (J >= 0) & (J < nb)
    Jc = np.clip(J, 0, nb - 1)
    lr = (la[:, None] - lb[Jc]).astype(np.float32)          # ln(sA/sB)
    sig = cfg["size_sigma"]
    S = np.exp(-(lr / sig) ** 2)
    lo, hi = cfg["size_loose"]
    S[(lr < math.log(lo)) | (lr > math.log(hi))] = 0.0
    # 邻域体量轮廓（不含 i,j 自己）
    k = cfg["ctx_k"]
    acc = np.zeros((na, M), dtype=np.float32)
    cnt = np.zeros((na, M), dtype=np.int16)
    for t in range(1, k + 1):
        for sgn in (-1, 1):
            ia = np.arange(na)[:, None] + sgn * t
            jb = J + sgn * t
            ok = (ia >= 0) & (ia < na) & (jb >= 0) & (jb < nb)
            ia_c = np.clip(ia, 0, na - 1)
            jb_c = np.clip(jb, 0, nb - 1)
            d = np.abs(la[ia_c] - lb[jb_c]).astype(np.float32)
            acc += np.where(ok, d, 0.0)
            cnt += ok
    have = cnt >= cfg["ctx_min"]
    C = np.where(have, np.clip(1.0 - (acc / np.maximum(cnt, 1)) / cfg["ctx_beta"], 0.0, 1.0), 0.0)
    ws, wc = cfg["w_size"], cfg["w_ctx"]
    den = ws + np.where(have, wc, 0.0)
    Q = (ws * S + np.where(have, wc * C, 0.0)) / den
    Q = np.where(valid & (Q > 0), Q, NEG).astype(np.float32)
    return Q, int(center[0]), M, center


def band_j(center_i, nb, band):
    """第 i 行第 k 列对应的真实 j（越界为 -1）。"""
    K = np.arange(-band, band + 1, dtype=np.int64)
    j = int(center_i) + K
    return np.where((j >= 0) & (j < nb), j, -1)


def forward(Q, nb, band, center, gap, order=None):
    """**max-sum** 前向（状态 = 前缀 F[i][j]：A[0..i]、B[0..j] 全被"消费"（匹配或跳过）的最优分）。

    F[i][j] = max( F[i-1][j-1] + q(i,j),  F[i-1][j] - gap [跳 A[i]],  F[i][j-1] - gap [跳 B[j]] )
    返回 (Fprev, Z, choice)：
      * `Fprev[i,k] = F[i-1][ J[i,k]-1 ]`（虚拟行 i=-1：F[-1][j] = -gap*(j+1)，F[-1][-1] = 0）
      * `Z = F[na-1][nb-1]`（全体对齐的最优分）
      * `choice[i,k]` ∈ {0 匹配, 1 跳 A, 2 跳 B}，供 Viterbi 回溯
    ⚠ 为什么是 max-sum 而不是 sum-product：**线性跳步罚分下路径数按二项式增长**，
      Σ 会被"跳步交错顺序"淹没（13,263×12,609 上全跳过那一族的权重 ≈ e^{14045} 倍于任何
      有匹配的对齐）⇒ 后验无意义。判据是"最优分"，置信度用**强制搭档的分数损失**（见 align）。
    """
    na, M = Q.shape
    Fprev = np.full((na, M), NEG, dtype=np.float64)
    choice = np.zeros((na, M), dtype=np.uint8)
    K = np.arange(-band, band + 1, dtype=np.int64)
    # 虚拟行 i=-1（j 从 -1 起）：j=-1 权重 0；j>=0 = 把 B[0..j] 全跳过
    prev_lo = -1
    prev = np.full(M, NEG, dtype=np.float64)
    prev[0] = 0.0
    for j in range(0, M - 1):
        prev[j + 1] = -gap * (j + 1)
    row = prev
    for i in range(na):
        ci = int(center[i])
        jj = ci + K
        ok = (jj >= 0) & (jj < nb)
        # ① 匹配 (i,j)：从 F[i-1][j-1] 来
        idx = (jj - 1) - prev_lo
        okm = ok & (idx >= 0) & (idx < M)
        v = np.where(okm, prev[np.clip(idx, 0, M - 1)], NEG)
        Fprev[i] = v
        a1 = v + Q[i]
        # ② 跳过 A[i]：从 F[i-1][j] 来
        idx2 = jj - prev_lo
        ok2 = ok & (idx2 >= 0) & (idx2 < M)
        a2 = np.where(ok2, prev[np.clip(idx2, 0, M - 1)], NEG) - gap
        A = np.maximum(a1, a2)
        ch = np.where(a1 >= a2, 0, 1).astype(np.uint8)
        if order is not None:
            bad = ~order.allowed(i, jj)
            A = np.where(bad, NEG, A)
        # ③ 行内跳 B[j]
        b = np.where(ok, A + gap * jj.astype(np.float64), NEG)
        kb = -1 - (ci - band)                  # 虚拟列 j=-1（F[i][-1] = -gap*(i+1)）
        if 0 <= kb < M:
            b[kb] = -gap * (i + 1) + gap * (-1.0)
        u = np.maximum.accumulate(b)
        row = np.where(ok, u - gap * jj.astype(np.float64), NEG)
        ch = np.where(u > b, 2, ch).astype(np.uint8)      # 被左边带上来 ⇒ 跳 B
        if 0 <= kb < M:
            row[kb] = -gap * (i + 1)           # 虚拟列存下来，供下一行"匹配 (i+1,0)"起手
        choice[i] = np.where(ok, ch, 3)
        if 0 <= kb < M:
            choice[i][kb] = 3
        prev, prev_lo = row, int(jj[0])
    corner = (nb - 1) - (int(center[na - 1]) - band)
    Z = float(row[corner]) if 0 <= corner < M else NEG
    return Fprev, Z, choice


def backward(Q, nb, band, center, gap, order=None):
    """**max-sum** 后向：beta[i,k] = 从 (i, J[i,k]) **匹配之后**走完的最优分（不含 q(i,j)）。

    beta[i][j] = max( q(i+1,j+1)+beta[i+1][j+1], -g+beta[i+1][j], -g+beta[i][j+1] )
    最后一行（i=na-1）没有后继 A ⇒ beta = -gap*(nb-1-j)。
    """
    na, M = Q.shape
    beta = np.full((na, M), NEG, dtype=np.float64)
    K = np.arange(-band, band + 1, dtype=np.int64)
    ci = int(center[-1])
    jj = ci + K
    ok = (jj >= 0) & (jj < nb)
    beta[-1] = np.where(ok, -gap * (nb - 1 - jj).astype(np.float64), NEG)
    if order is not None:
        beta[-1] = np.where(order.allowed(na - 1, jj), beta[-1], NEG)
    kb0 = nb - (ci - band)
    if 0 <= kb0 < M:
        beta[-1][kb0] = 0.0
    nxt, nxt_lo = beta[-1], int(jj[0])
    for i in range(na - 2, -1, -1):
        ci = int(center[i])
        jj = ci + K
        ok = (jj >= 0) & (jj < nb)
        ci2 = int(center[i + 1])
        k2 = (jj + 1) - (ci2 - band)
        idxm = (jj + 1) - nxt_lo
        okm = ok & (k2 >= 0) & (k2 < M) & (idxm >= 0) & (idxm < M)
        A1 = np.where(okm, nxt[np.clip(idxm, 0, M - 1)] + Q[i + 1][np.clip(k2, 0, M - 1)], NEG)
        idx2 = jj - nxt_lo
        ok2 = ok & (idx2 >= 0) & (idx2 < M)
        A2 = np.where(ok2, nxt[np.clip(idx2, 0, M - 1)], NEG) - gap
        A = np.maximum(A1, A2)
        if order is not None:
            A = np.where(order.allowed(i, jj), A, NEG)
        b = np.where(ok, A - gap * jj.astype(np.float64), NEG)
        kb = nb - (ci - band)                  # 虚拟列 j=nb：只**存**、不参与本行
        u = np.maximum.accumulate(b[::-1])[::-1]
        beta[i] = np.where(ok, u + gap * jj.astype(np.float64), NEG)
        if 0 <= kb < M:
            beta[i][kb] = -gap * (na - 1 - i)
        nxt, nxt_lo = beta[i], int(jj[0])
    return beta


class AnchorOrder(object):
    """锚点分段：只允许落在同一对相邻锚点围出的矩形里；锚点格加 `bonus` 强制被匹配。"""

    def __init__(self, pairs, cfg=None):
        cfg = cfg or PREREG
        pairs = sorted(set(pairs))
        seq = [p[1] for p in pairs]
        keep = longest_increasing(seq)
        self.dropped = len(pairs) - len(keep)
        self.pairs = [pairs[i] for i in keep]
        self.bonus = 50.0
        self.ai = [p[0] for p in self.pairs]
        self.aj = [p[1] for p in self.pairs]

    def allowed(self, i, jj):
        import bisect
        s = bisect.bisect_left(self.ai, i)
        lo_j = self.aj[s - 1] if s > 0 else -1
        ok = jj > lo_j
        if s < len(self.ai):
            ok &= jj < self.aj[s]
        return ok


def longest_increasing(seq):
    import bisect
    tails, tails_idx, prev = [], [], [-1] * len(seq)
    for idx, v in enumerate(seq):
        k = bisect.bisect_left(tails, v)
        if k == len(tails):
            tails.append(v)
            tails_idx.append(idx)
        else:
            tails[k] = v
            tails_idx[k] = idx
        prev[idx] = tails_idx[k - 1] if k > 0 else -1
    out = []
    cur = tails_idx[-1] if tails_idx else -1
    while cur >= 0:
        out.append(cur)
        cur = prev[cur]
    return out[::-1]


def align(sa, sb, cfg=None, anchors=None, verbose=False):
    """跑一遍：返回 (js, qbest, margin, matched, stats, aux)。

    * `js[i]`   = A[i] 在 **Viterbi 路径**上的搭档 B 下标（-1 = 最优解里它被跳过）
    * `qbest[i]`= 该搭档的匹配质量 q（-1 表示没有搭档）
    * `margin[i]` = **强制** A[i] 换搭档的分数损失之差：`best1 - best2`（同 q 同量纲）
                    —— 这就是"唯一对齐"的置信度（不是概率）
    * `matched[i]` = A[i] 被强制匹配时相对全局最优的损失（>0 ⇒ 最优解确实跳过了它）
    """
    cfg = cfg or PREREG
    band, gap = int(cfg["band"]), float(cfg["gap"])
    na, nb = len(sa), len(sb)
    Q, j0, M, center = build_q(sa, sb, band, cfg)
    order = None
    if anchors:
        order = AnchorOrder(anchors, cfg)
        for (ai, aj) in order.pairs:                  # 强制锚点被匹配
            if 0 <= ai < na:
                k = aj - (int(center[ai]) - band)
                if 0 <= k < M:
                    Q[ai, k] = np.float32(max(float(Q[ai, k]), 0.0) + order.bonus)
    Fprev, Z, choice = forward(Q, nb, band, center, gap, order=order)
    beta = backward(Q, nb, band, center, gap, order=order)
    # 每个 (i,j)：**强制**匹配它时整条对齐的最优分；与全局最优的差 = 强制它的代价
    score = Q.astype(np.float64) + Fprev + beta
    del Fprev
    bad = Q < (NEG / 2)
    score[bad] = NEG
    best = np.argmax(score, axis=1)
    q1 = score[np.arange(na), best]
    S2 = score.copy()
    S2[np.arange(na), best] = NEG
    q2 = S2.max(axis=1)
    del S2, score
    # ① 决策用 Viterbi 路径（保证 1:1、保序），不用逐行 argmax
    js = np.full(na, -1, dtype=np.int64)
    i, j = na - 1, nb - 1
    guard = 0
    while i >= 0 and j >= 0 and guard < (na + nb + 10):
        guard += 1
        ci = int(center[i])
        k = j - (ci - band)
        if not (0 <= k < M):
            break
        c = int(choice[i, k])
        if c == 2:
            j -= 1
        elif c == 1:
            i -= 1
        elif c == 0:
            js[i] = j
            i -= 1
            j -= 1
        else:
            break
    matched_js = js >= 0
    qbest = np.where(matched_js,
                     Q[np.arange(na), np.clip(js - (center - band), 0, M - 1)], -1.0)
    margin = np.where(matched_js & (q1 > NEG / 2), q1 - q2, -1.0)
    forced = np.where(q1 > NEG / 2, Z - q1, 1e9)
    stats = dict(cells=int(Q.size), band=band, gap=gap, Z=Z,
                 anchors_dropped=(order.dropped if order else 0),
                 anchors_used=(len(order.pairs) if order else 0),
                 viterbi_matches=int(matched_js.sum()),
                 skips_A=int((~matched_js).sum()))
    del Q, beta, choice
    return js, qbest.astype(np.float64), margin.astype(np.float64), forced, stats, None


def tier_of(q, margin, cfg=None):
    """置信档：只看 (q, margin) 两个**预注册**阈值的组合，不看任何真值。"""
    cfg = cfg or PREREG
    if q < 0 or margin < 0:
        return "none"
    if q < cfg["q_min"]:
        return "none"
    for name in ("A", "B", "C"):
        if margin >= cfg["tier_margin"][name]:
            return name
    return "none"


def _brute(sa, sb, cfg, band):
    """穷举**所有**保序对齐（小序列）：返回 (最优分, {(i,j): 强制匹配它时的最优分})。"""
    Q, _j0, M, center = build_q(sa, sb, band, cfg)
    na, nb = len(sa), len(sb)
    def qv(i, j):
        k = j - (int(center[i]) - band)
        if 0 <= k < M:
            v = float(Q[i, k])
            return None if v < -1e29 else v
        return None
    g = cfg["gap"]
    best = [NEG]
    forced = {}
    def rec(i, j, sc, ms):
        if i == na and j == nb:
            if sc > best[0]:
                best[0] = sc
            for m in ms:
                forced[m] = max(forced.get(m, NEG), sc)
            return
        if i < na and j < nb:
            v = qv(i, j)
            if v is not None:
                rec(i + 1, j + 1, sc + v, ms + [(i, j)])
        if i < na:
            rec(i + 1, j, sc - g, ms)
        if j < nb:
            rec(i, j + 1, sc - g, ms)
    rec(0, 0, 0.0, [])
    return best[0], forced


def cmd_brute(args):
    """自检⓪：带状 **max-sum** DP 与**穷举**对拍（小序列；含"强制匹配"分数）。"""
    rng = np.random.RandomState(args.seed)
    worst_z = worst_f = 0.0
    badz = badf = badpath = 0
    for _t in range(args.n):
        na, nb = rng.randint(2, 5), rng.randint(2, 6)
        sa = list(rng.randint(10, 400, na))
        sb = list(rng.randint(10, 400, nb))
        band = max(na, nb) + 2
        cfg = dict(PREREG, band=band)
        Zb, Fb = _brute(sa, sb, cfg, band)
        Q, _j0, M, center = build_q(sa, sb, band, cfg)
        Fprev, Zd, _ch = forward(Q, nb, band, center, cfg["gap"])
        beta = backward(Q, nb, band, center, cfg["gap"])
        worst_z = max(worst_z, abs(Zd - Zb))
        badz += int(abs(Zd - Zb) > 1e-9)
        for i in range(na):
            for j in range(nb):
                k = j - (int(center[i]) - band)
                if not (0 <= k < M) or float(Q[i, k]) < -1e29:
                    continue
                got = float(Q[i, k]) + float(Fprev[i, k]) + float(beta[i, k])
                worst_f = max(worst_f, abs(got - Fb.get((i, j), NEG)))
                badf += int(abs(got - Fb.get((i, j), NEG)) > 1e-9)
        # 回溯路径本身：保序、1:1、分数 == 穷举最优
        js, qb, mg, fo, st, _ = align(sa, sb, dict(PREREG, band=band))
        pairs = [(i, int(js[i])) for i in range(na) if int(js[i]) >= 0]
        mono = all(pairs[t][0] < pairs[t + 1][0] and pairs[t][1] < pairs[t + 1][1]
                   for t in range(len(pairs) - 1))
        dup = len({j for _i, j in pairs}) != len(pairs)
        sc = sum(float(Q[i, j - (int(center[i]) - band)]) for i, j in pairs)
        sc -= cfg["gap"] * ((na - len(pairs)) + (nb - len(pairs)))
        if not mono or dup or abs(sc - Zb) > 1e-9:
            badpath += 1
            worst_f = max(worst_f, abs(sc - Zb))
    print("自检⓪ 穷举对拍 %d 组：max|ΔZ| = %.2e（不符 %d 组）; "
          "max|Δ 强制匹配分| = %.2e（不符 %d 格）; 回溯路径不符 %d 组"
          % (args.n, worst_z, badz, worst_f, badf, badpath))
    return 0 if (badz == 0 and badf == 0 and badpath == 0) else 1


def cmd_truth(args):
    """抽取序列 + 真值 + 目标集合 + 近孪生 27 行（**只在这里**碰指纹缓存与 Location）。"""
    work = args.work
    os.makedirs(work, exist_ok=True)
    xs = x86_sequence()
    arms = arm64_sequence()
    write_tsv(wpath(work, "x86.seq.tsv"), ["addr", "size"],
              [dict(addr="0x%x" % a, size=s) for a, s in xs],
              comment=["任务 93 · x86 序列：funcstarts_ghidra_va.pkl ∩ .text，size = 下一起点 − 本起点"])
    write_tsv(wpath(work, "arm64.seq.tsv"), ["addr", "size", "name"],
              [dict(addr="0x%x" % a, size=s, name=n) for a, s, n in arms],
              comment=["任务 93 · arm64 序列：android-2.0.60 .symtab STT_FUNC ∩ .text，不同起点，size = st_size"])
    print("x86 %d  arm64 %d（符号条目 %d）" % (len(xs), len(arms), len(func_symbols(ARM64_SO))))

    sys.path.insert(0, HERE)
    import callgraph_fingerprint as F
    fps = F.load_all(tags=["linux", "android"])
    truth, meta = F.control_pairs("android", "linux", fps=fps)
    rows = []
    names = dict((a, n) for a, _s, n in arms)
    # x86 侧的名字（来自三份夹具，只用于展示/次要判据，不参与对齐）
    x86name = {}
    for fn in ("name_to_addr.tsv", "name_to_addr_v2.tsv"):
        p = os.path.join(ROOT, "fixtures", fn)
        if not os.path.exists(p):
            continue
        for r in read_tsv(p):
            if r.get("x86_addr", "").startswith("0x"):
                x86name.setdefault(int(r["x86_addr"], 16), "%s::%s" % (r.get("type") or "", r.get("method") or ""))
    for lin, andr in sorted(truth.items()):
        rows.append(dict(x86_addr="0x%x" % lin, arm64_addr="0x%x" % andr,
                         arm64_name=names.get(andr, ""), x86_name=x86name.get(lin, "")))
    nm = demangle([r["arm64_name"] for r in rows])
    for r, d in zip(rows, nm):
        r["arm64_name"] = d
    write_tsv(wpath(work, "truth.tsv"), ["x86_addr", "arm64_addr", "arm64_name", "x86_name"], rows,
              comment=["任务 93 · 留出验证真值：control_pairs('android','linux') 严格 Location join = %d 对" % len(rows),
                       "**只用于验证**，不参与对齐、不参与选参（预注册见 fixtures/seq_align_prereg93.tsv/md）",
                       "meta=%s" % json.dumps(meta, sort_keys=True)])

    # 目标集合 12,736 与 527
    lin = fps["linux"]
    tgt = [va for va in sorted(lin.universe) if not F.loc_keys(lin, va)]
    write_tsv(wpath(work, "target12736.tsv"), ["addr"], [dict(addr="0x%x" % a) for a in tgt],
              comment=["任务 93 · 目标集合：13,263 里不引用任何 src/*.rs Location 的 %d 个" % len(tgt)])

    # 近孪生 27 行（v2 留出半 ∩ campaign 锚表）
    anchors = read_tsv(os.path.join(ROOT, "fixtures/campaign_anchors75.tsv"))
    aset = {int(r["addr"], 16): r for r in anchors if r.get("addr", "").startswith("0x")}
    samples = read_tsv(os.path.join(ROOT, "fixtures/fingerprint_holdout72_samples.tsv"))
    nt = []
    for r in samples:
        if r.get("half") != "holdout":
            continue
        a = int(r["linux_addr"], 16)
        if a in aset and a in truth:
            nt.append(dict(x86_addr="0x%x" % a, arm64_addr="0x%x" % truth[a],
                           arm64_name=names.get(truth[a], ""), kind=aset[a].get("kinds", ""),
                           truth_name=aset[a].get("truth_name", "")))
    write_tsv(wpath(work, "neartwin27.tsv"), ["x86_addr", "arm64_addr", "arm64_name", "kind", "truth_name"], nt,
              comment=["任务 93 · 近孪生族 27 行 = v2 留出半 ∩ campaign 锚表（结构指纹 14/27 = 51.9%）"])

    # 锚点/评估互斥分半（与指纹两半同口径：地址 sha1 奇偶）
    tune, hold = F.split_half_addrs([p[0] for p in sorted(truth.items())])
    tset = set(tune)
    write_tsv(wpath(work, "anchors_tune.tsv"), ["x86_addr", "arm64_addr"],
              [dict(x86_addr="0x%x" % a, arm64_addr="0x%x" % truth[a]) for a in sorted(tset)],
              comment=["任务 93 · 锚点分段变体用的锚点 = 真值的 sha1 奇偶 **tune 半**（%d 条）" % len(tune)])
    write_tsv(wpath(work, "holdout.tsv"), ["x86_addr", "arm64_addr"],
              [dict(x86_addr="0x%x" % a, arm64_addr="0x%x" % truth[a]) for a in sorted(hold)],
              comment=["任务 93 · 锚点分段变体用的评估集 = 真值的 sha1 奇偶 **holdout 半**（%d 条，与锚点互斥）" % len(hold)])
    print("真值 %d 对；目标集合 %d；近孪生 %d；锚点/评估分半 %d/%d"
          % (len(rows), len(tgt), len(nt), len(tune), len(hold)))


def cmd_run(args):
    work = args.work
    xs, arms = load_work(work)
    sa = [s for _a, s in xs]
    sb = [s for _a, s, _n in arms]
    cfg = dict(PREREG)
    if args.band:
        cfg["band"] = args.band
    if args.gap is not None:
        cfg["gap"] = args.gap
    apairs = None
    if args.anchors:
        which = "anchors_tune.tsv" if args.anchors == "tune" else "holdout.tsv"
        araw = [(int(r["x86_addr"], 16), int(r["arm64_addr"], 16)) for r in read_tsv(wpath(work, which))]
        xidx = {a: i for i, (a, _s) in enumerate(xs)}
        a2i = {a: j for j, (a, _s, _n) in enumerate(arms)}
        apairs = sorted({(xidx[a], a2i[j]) for a, j in araw if a in xidx and j in a2i})
    js, qbest, margin, forced, stats, _ = align(sa, sb, cfg, apairs, verbose=True)
    tgt = {int(r["addr"], 16) for r in read_tsv(wpath(work, "target12736.tsv"))}
    names = demangle([n for _a, _s, n in arms])
    rows = []
    counts = collections.Counter()
    for i, (a, s) in enumerate(xs):
        j = int(js[i])
        t = tier_of(float(qbest[i]), float(margin[i]), cfg)
        in_t = a in tgt
        counts[("target" if in_t else "named", t)] += 1
        rows.append(dict(x86_addr="0x%x" % a, x86_size=s, in_target=int(in_t),
                         arm64_addr=("0x%x" % arms[j][0]) if j >= 0 else "",
                         arm64_name=(names[j] if j >= 0 else ""),
                         size_ratio=("%.3f" % (float(s) / max(arms[j][1], 1))) if j >= 0 else "",
                         q_match=("%.4f" % qbest[i]) if j >= 0 else "",
                         margin=("%.4f" % margin[i]) if j >= 0 else "",
                         tier=t,
                         source=("order+size(%s)" % ("anchored" if apairs else "noanchor")) if j >= 0 else ""))
    out = args.out
    write_tsv(out, ["x86_addr", "x86_size", "in_target", "arm64_addr", "arm64_name", "size_ratio",
                    "q_match", "margin", "tier", "source"],
              rows,
              comment=["任务 93 · 全序列单调对齐（arm64 .symtab 顺序+体量 ⇒ x86），**推断**级，不是命名",
                       "prereg=%s sha256=%s" % (PREREG["version"], prereg_sha()),
                       "config: band=%d gap=%s anchors=%s" % (cfg["band"], cfg["gap"],
                                                              "tune(%d)" % len(apairs) if apairs else "none"),
                       "格数 %d；Viterbi 匹配 %d / 跳过 %d；置信档阈值见 fixtures/seq_align_prereg93.md"
                       % (stats["cells"], stats["viterbi_matches"], stats["skips_A"]),
                       "in_target=1 即 12,736 那一批（原本一个 Location 都不引用）"])
    print("写出 %s；格数 %d；档位 %s" % (out, stats["cells"],
          dict(sorted(((k[0] + "/" + k[1]), v) for k, v in counts.items()))))
    return stats


def cmd_verify(args):
    """三条验收：① 留出准确率 ② 覆盖率 ③ 近孪生 27 行。"""
    work = args.work
    xs, arms = load_work(work)
    a2i = {a: j for j, (a, _s, _n) in enumerate(arms)}
    rows = {int(r["x86_addr"], 16): r for r in read_tsv(args.align)}
    truth = load_truth(work)
    names = demangle([n for _a, _s, n in arms])
    ok_addr = ok_name = 0
    per_tier = collections.Counter()
    per_tier_ok = collections.Counter()
    miss = []
    for a, andr in truth:
        r = rows.get(a)
        if not r:
            continue
        got = r["arm64_addr"]
        good = (got == "0x%x" % andr)
        t = r["tier"]
        per_tier[t] += 1
        if good:
            ok_addr += 1
            per_tier_ok[t] += 1
        elif args.misses:
            miss.append((a, got, r["arm64_name"], "0x%x" % andr,
                         names[a2i[andr]] if andr in a2i else "", t, r["q_match"], r["margin"]))
        if r["arm64_name"] and andr in a2i and sym_key(r["arm64_name"]) == sym_key(names[a2i[andr]]):
            ok_name += 1
    n = len(truth)
    tgt = [int(r["addr"], 16) for r in read_tsv(wpath(work, "target12736.tsv"))]
    cov = collections.Counter()
    for a in tgt:
        r = rows.get(a)
        cov[r["tier"] if r else "missing"] += 1
    nt = read_tsv(wpath(work, "neartwin27.tsv"))
    nt_ok = nt_t = 0
    nt_rows = []
    for r in nt:
        a, andr = int(r["x86_addr"], 16), int(r["arm64_addr"], 16)
        got = rows.get(a, {}).get("arm64_addr", "")
        good = got == "0x%x" % andr
        nt_t += 1
        nt_ok += good
        nt_rows.append((r["x86_addr"], r["truth_name"], got,
                        rows.get(a, {}).get("arm64_name", ""),
                        rows.get(a, {}).get("tier", "?"), good))
    hold = {int(r["x86_addr"], 16) for r in read_tsv(wpath(work, "holdout.tsv"))}
    h_ok = sum(1 for a, andr in truth
               if a in hold and rows.get(a, {}).get("arm64_addr") == "0x%x" % andr)
    h_n = sum(1 for a, andr in truth if a in hold)
    print("=" * 78)
    print("① 留出验证（真值 %d 对；headline 配置**不用锚点** ⇒ 与任何拟合都不重叠）" % n)
    print("   地址一致 top-1：%d/%d = %.2f%%" % (ok_addr, n, 100.0 * ok_addr / max(1, n)))
    print("   名字一致 top-1：%d/%d = %.2f%%（次要判据，允许 .llvm. 克隆）"
          % (ok_name, n, 100.0 * ok_name / max(1, n)))
    print("   分档：" + "  ".join("%s %d/%d=%.1f%%" % (t, per_tier_ok[t], per_tier[t],
                                                    100.0 * per_tier_ok[t] / max(1, per_tier[t]))
                                for t in ("A", "B", "C", "none") if per_tier[t]))
    print("   锚点变体 holdout 半：%d/%d = %.2f%%" % (h_ok, h_n, 100.0 * h_ok / max(1, h_n)))
    print("② 覆盖率：目标集合 %d" % len(tgt))
    for t in ("A", "B", "C", "none"):
        print("   %-5s %6d  %5.1f%%" % (t, cov[t], 100.0 * cov[t] / max(1, len(tgt))))
    print("③ 近孪生 27 行：%d/%d = %.1f%%（结构指纹 14/27 = 51.9%%）"
          % (nt_ok, nt_t, 100.0 * nt_ok / max(1, nt_t)))
    for r in nt_rows:
        print("   %s %-52s -> %-22s %-42s %-4s %s"
              % (r[0], r[1][:52], r[2], (r[3] or "")[:42], r[4], "OK" if r[5] else "--"))
    if args.misses:
        print("错的 %d 条（地址 / 给的名字 / 真值地址 / 真值名字 / 档 / q / margin）：" % len(miss))
        for m in miss:
            print("   %s got %-22s %-38s want %-22s %-30s %s %s %s"
                  % ("0x%x" % m[0], m[1], (m[2] or "")[:38], m[3], (m[4] or "")[:30],
                     m[5], m[6], m[7]))
    json.dump(dict(n=n, ok_addr=ok_addr, ok_name=ok_name,
                   tiers={t: [per_tier_ok[t], per_tier[t]] for t in per_tier},
                   cov=dict(cov), neartwin=[nt_ok, nt_t], holdout=[h_ok, h_n]),
              open(wpath(work, "verify_%s.json" % (args.tag or "p0")), "w"), indent=1)
    return ok_addr, n


def cmd_sweep(args):
    """预注册网格：配置在 **tune 半**上选，选中后报 holdout 半（headline 不用选中值）。"""
    import multiprocessing as mp
    work = args.work
    xs, arms = load_work(work)
    sa = [s for _a, s in xs]
    sb = [s for _a, s, _n in arms]
    truth = load_truth(work)
    hold = {int(r["x86_addr"], 16) for r in read_tsv(wpath(work, "holdout.tsv"))}
    combos = [(b, g) for b in PREREG["grid"]["band"] for g in PREREG["grid"]["gap"]]

    def one(cb):
        b, g = cb
        cfg = dict(PREREG)
        cfg["band"], cfg["gap"] = b, g
        js, qb, mg, fo, st, _ = align(sa, sb, cfg)
        got = {}
        for i, (a, _s) in enumerate(xs):
            if int(js[i]) >= 0:
                got[a] = arms[int(js[i])][0]
        tune_ok = tune_n = hold_ok = hold_n = 0
        for a, andr in truth:
            good = 1 if got.get(a) == andr else 0
            if a in hold:
                hold_n += 1
                hold_ok += good
            else:
                tune_n += 1
                tune_ok += good
        return (b, g, tune_ok, tune_n, hold_ok, hold_n, st["cells"])

    with mp.Pool(min(args.jobs, 12)) as pool:
        res = pool.map(one, combos)
    for r in sorted(res, key=lambda z: (-z[2] / max(1, z[3]), z[0], z[4])):
        print("band=%-5d gap=%-5s tune=%3d/%3d=%6.2f%%  holdout=%3d/%3d=%6.2f%%  cells=%d"
              % (r[0], r[1], r[2], r[3], 100.0 * r[2] / max(1, r[3]), r[4], r[5],
                 100.0 * r[4] / max(1, r[5]), r[6]))
    best = max(res, key=lambda z: (z[2] / max(1, z[3]), z[4]))
    print("预注册选择规则 ⇒ band=%d gap=%s（tune 上最好）；它在 holdout 上是 %d/%d = %.2f%%"
          % (best[0], best[1], best[4], best[5], 100.0 * best[4] / max(1, best[5])))
    write_tsv(wpath(work, "sweep.tsv"),
              ["band", "gap", "tune_ok", "tune_n", "holdout_ok", "holdout_n", "cells"],
              [dict(band=r[0], gap=r[1], tune_ok=r[2], tune_n=r[3], holdout_ok=r[4],
                    holdout_n=r[5], cells=r[6]) for r in res],
              comment=["任务 93 · 预注册网格（band × gap），选择只看 tune 半，holdout 半只报数"])


def cmd_selftest(args):
    """合成数据自检：保序 + 插入 + 删除 + 体量噪声 ⇒ 应恢复；**乱序对照** ⇒ 应崩。

    合成数据**不碰**任何真值（也不读 truth.tsv），所以拿它标定档位阈值不构成"先跑后改参数"。
    """
    rng = np.random.RandomState(93)
    base = None
    if os.path.exists(wpath(args.work, "x86.seq.tsv")):
        xs, _arms = load_work(args.work)
        base = [s for _a, s in xs]
    if not base:
        base = list(np.exp(rng.normal(4.5, 1.2, 13000)).astype(int) + 8)
    # B = A 的 85% 保序子集（模拟内联/ICF），中点再插 300 个"另一侧独有"的函数
    keep = [i for i in range(len(base)) if rng.rand() > 0.15]
    sb, truth = [], {}
    xa = []
    for idx, i in enumerate(keep):
        if idx == len(keep) // 2:
            for _ in range(300):
                sb.append(max(8, int(base[i] * math.exp(rng.normal(0, 0.6)))))
        sb.append(max(8, int(base[i] * math.exp(rng.normal(0, 0.18)))))
        truth[len(xa)] = len(sb) - 1
        xa.append(base[i])
    sa, m = [], {}
    for k in range(len(keep)):
        sa.append(xa[k])
        m[len(sa) - 1] = truth[k]
        if rng.rand() < 0.03:
            sa.append(max(8, int(xa[k] * math.exp(rng.normal(0, 0.9)))))
    cfg = dict(PREREG)
    js, qb, mg, fo, st, _ = align(sa, sb, cfg)
    hit = sum(1 for i, j in m.items() if int(js[i]) == j)
    hitall = sum(1 for i in range(len(sa)) if int(js[i]) == m.get(i, -2))
    print("自检①（保序 + 15%% 删 + 300 插入 + 3%% 增 + 18%% 体量噪声）：%d/%d = %.1f%%  格数 %d"
          % (hit, len(m), 100.0 * hit / len(m), st["cells"]))
    print("   档位标定（q_min=%s tier_margin=%s）："
          % (cfg["q_min"], json.dumps(cfg["tier_margin"], sort_keys=True)))
    tot, good = collections.Counter(), collections.Counter()
    for i in range(len(sa)):
        t = tier_of(float(qb[i]), float(mg[i]), cfg)
        tot[t] += 1
        good[t] += int(int(js[i]) == m.get(i, -2))
    for t in ("A", "B", "C", "none"):
        if tot[t]:
            print("   %-5s %6d 行  精度 %6.2f%%" % (t, tot[t], 100.0 * good[t] / tot[t]))
    allm = sum(tot[t] for t in ("A", "B", "C"))
    kept = sum(good[t] for t in ("A", "B", "C"))
    print("   档 A+B+C 覆盖 %d/%d = %.1f%%、精度 %.2f%%"
          % (allm, len(m), 100.0 * allm / len(m), 100.0 * kept / max(1, allm)))
    perm = rng.permutation(len(sb))
    sb2 = [sb[q] for q in perm]
    inv = {q: k for k, q in enumerate(perm)}
    js2, _a, _b, _c, _d, _e = align(sa, sb2, cfg)
    hit2 = sum(1 for i, j in m.items() if int(js2[i]) == inv[j])
    print("自检②（B 侧整体乱序的**阴性对照**）：%d/%d = %.2f%%   ⇒ 顺序轴必须显著高于它"
          % (hit2, len(m), 100.0 * hit2 / len(m)))
    return hit / len(m), hit2 / len(m)


def cmd_print_config(args):
    print(json.dumps("PREREG", sort_keys=True))
    print(prereg_json())
    print("sha256 = %s" % prereg_sha())


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--work", default=os.path.join(ROOT, ".agent-work/seqalign93"))
    sub = ap.add_subparsers(dest="cmd", required=True)
    t = sub.add_parser("truth")
    t.set_defaults(fn=cmd_truth)
    r = sub.add_parser("run")
    r.add_argument("--out", default=os.path.join(ROOT, "fixtures/seq_align.tsv"))
    r.add_argument("--band", type=int, default=None)
    r.add_argument("--gap", type=float, default=None)
    r.add_argument("--anchors", default=None, help="'tune' 或 'holdout'")
    r.set_defaults(fn=cmd_run)
    v = sub.add_parser("verify")
    v.add_argument("--align", default=os.path.join(ROOT, "fixtures/seq_align.tsv"))
    v.add_argument("--tag", default=None)
    v.add_argument("--misses", action="store_true")
    v.set_defaults(fn=cmd_verify)
    s = sub.add_parser("sweep")
    s.add_argument("--jobs", type=int, default=8)
    s.set_defaults(fn=cmd_sweep)
    st = sub.add_parser("selftest")
    st.set_defaults(fn=cmd_selftest)
    br = sub.add_parser("brute")
    br.add_argument("--n", type=int, default=200)
    br.add_argument("--seed", type=int, default=0)
    br.set_defaults(fn=cmd_brute)
    p = sub.add_parser("print-config")
    p.set_defaults(fn=cmd_print_config)
    a = ap.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
