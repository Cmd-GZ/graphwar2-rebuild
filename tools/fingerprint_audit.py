#!/usr/bin/env python3
"""fingerprint_audit.py — 任务 63：**逐特征取值范围审计** ＋ 消融 ＋ bootstrap 置信区间。

## 这个工具补的是哪一段（`docs/02` §35 与 §43 都不回答的那一问）

57 号（`docs/02` §43）发现 `consts` 的取值域写成了 `0x100 <= |imm| <= 0xFFFFFFFF`，
于是 **<0x100 的关卡号 / 标志位 / 小计数整段看不见**；把这一段补成新特征 `scal` 之后，
留出半 **72.7% → 78.2%（+5.5 点）**。⇒ **一个特征的取值域假设错了，就藏掉了 5.5 个点。**

其余 15 个特征**没人系统审过**。本工具把这件事做成机械的，三件事：

1. `scan`   —— 把每个特征**被扔掉的那一段**重新扫出来（`scal` 的 0/±1、`consts` 的 >32 位、
   `callee` 的**间接调用**与落不进 universe 的调用点、`stok` 的 2 字符 token、被
   `string_at` 的 `len>=4` 挡掉的短字符串），落到一份**只含计数与集合**的缓存。
2. `audit`  —— 逐个特征打印「取值域 / 丢弃比例（正对照 vs 目标集合）/ 丢弃部分是否携带信息」。
   "携带信息"用**两个互相独立的量**判，不靠权重：
     ① **判别力**：真配对（Location 真值）与**同窗口随机候选**的该特征相似度中位，差得开 = 有信号；
     ② **加回去 Δtop-1**：把丢弃段当成新特征加进打分（`sweep` 里那一组 `hole:*` 配置）。
3. `sweep`  —— 16 特征**留一法 / 加入法** ＋ 权重网格 ＋ 2^4 子集枚举 ＋ 漏洞补丁组，
   **两半都报，权重只在调参半上选**（`AGENTS.md` 合并纪律 A4）。
4. `boot`   —— bootstrap 95% 置信区间（百分位法，B=2000）＋ 与基线的**配对** bootstrap
   ＋ McNemar 精确检验（同一批判出样本上的两个配置才可比）。

## 口径（与 `callgraph_fingerprint.py` 逐位一致的部分）

* 打分器 `score_audit()` 是 `CF.score_same()` 的**逐行复刻**，只在最后**追加**本任务的
  漏洞特征项；追加项的权重为 0 时 `add()` 直接返回 ⇒ **基线逐位不变**
  （实测与 `CF.score_same` 的 top-1/top-5 完全相同：调参半 81.9%/91.6%、留出半 72.7%/88.5%）。
* 分半 / 真值 / 候选窗口全部复用 `callgraph_fingerprint.py` 的实现，不另起一套：
  `CF.Pair` / `CF.split_half_addrs` / `CF.Pool.window`。

## 用法

    ../.reference/venv/bin/python tools/fingerprint_audit.py scan --tags linux,win,win_old
    ../.reference/venv/bin/python tools/fingerprint_audit.py audit
    ../.reference/venv/bin/python tools/fingerprint_audit.py sweep --jobs 32
    ../.reference/venv/bin/python tools/fingerprint_audit.py boot --jobs 32 \
        --configs "BASE,REF,hole:scal0=.5,hole:icall=.3"

`--out` 只落**文本**夹具（`AGENTS.md` 硬规矩 3：不入库缓存/二进制）。
"""

import argparse
import bisect
import collections
import hashlib
import itertools
import math
import multiprocessing
import os
import pickle
import random
import re
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import callgraph_fingerprint as CF              # noqa: E402
import sym_match as SM                          # noqa: E402
import three_way_match as T                     # noqa: E402

AUDIT_VERSION = "auditv1"

# ===========================================================================
# 1. 漏洞字段：每个特征**被扔掉的那一段**
#
# 命名口径：`hole:<名字>` 在 `sweep` 里当特征名用（`--weights` 也认）。
# 每条都对应 `callgraph_fingerprint.py` 里一个**具体的**阈值/白名单/区间截断（写在 AUDIT_SPEC）。
# ===========================================================================
HOLE_KEYS = ("imm01", "small", "scal0", "c64", "scal64", "stok2", "stokx", "icall",
             "scal_sgn")

# 16 个特征（12 个 W_SAME 键 + 57 号的 4 个新键）。`cls` / `scal_c` 是旁支，不在 16 之内。
FEAT16 = ("size", "hist", "gram", "consts", "callee", "caller", "strs", "strs_s",
          "ncal", "ncall", "cbsig", "rbsig", "shape", "scal", "wconst", "stok")

AUDIT_SPEC = collections.OrderedDict([
    # key: (取值域/口径, 代码依据, 被排除的合法段)
    ("size", ("函数字节数 = end - va；打分用 sim_log(eps=0.4) 饱和",
              "FP.__init__ `fe['size']`；`sim_log`；`Pool.window` [0.70,1.40]",
              "窗口外的候选**根本不参与排序**（真配对 3.0% 落在窗外）；比值超出 e^±0.4 ⇒ 相似度夹到 0")),
    ("hist", ("原始助记符计数（Counter），cos 相似度", "`fe['hist']`；`cos_hist`",
              "无（但权重默认 0；`mn_id` 的 16 位哈希有 0~1 对碰撞）")),
    ("gram", ("助记符 id 序列的三元组集合（Jaccard）",
              "`_trigrams(fe['seq'])`；`mn_id` 取 sha1 前 16 位",
              "指令数 <3 的函数只剩 1 个伪三元组（`_trigrams` 的特例分支）")),
    ("consts", ("整数立即数，**0x100 <= |v| <= 0xFFFFFFFF**，且不在任何节、不是函数地址；**带符号**存",
                "`scan_x86` 里 `-0x80000000 <= op.imm <= 0xFFFFFFFF and abs(...) >= 0x100`",
                "① 0x2..0xFF（**57 号已用 `scal` 补**）；② 0/±1（**至今没人补**）；"
                "③ |v|>0xFFFFFFFF 或 v<-2^31（只有 `wconst` 收，而 `wconst` 默认权重 0）")),
    ("callee", ("被调者**尺寸**多重集，只收落在 universe 里的**直接**调用目标",
                "`FP.__init__` `fe['callee_sizes']`；`T.owner_of`",
                "**间接调用**（`fe['icalls']` 算了但**从来没进打分**）；目标落不进 universe 的调用点"
                "（PLT/垫片/函数中部）；被调者身份只剩尺寸（身份在 `cbsig`，权重 0）")),
    ("caller", ("调用者尺寸多重集，只由**直接**调用反推", "同上 `fe['caller_sizes']`",
                "同一批：间接调用边不存在于调用图里 ⇒ 只被函数指针调用的函数「没有调用者」")),
    ("strs", ("引用到的字符串内容 sha1，**5 个只读节**、可打印 ASCII、NUL 结尾、长度 4..96",
              "`STR_SECTIONS`；`_cstr(min 4)`；`string_at(4 < len <= 96)`",
              "**这是泄漏列**（`panic::Location` 的 file 字段），默认权重 0，不进任何结论")),
    ("strs_s", ("= strs − Rust 源文件路径", "`RE_LOC_FILE` / `is_loc_file`",
              "同上；另：非 ASCII（含中文）字符串整段不可见（`decode('ascii','ignore')`）")),
    ("ncal", ("被调者个数（log 比）", "`len(fe['callees'])`", "同 `callee`：间接调用不计入")),
    ("ncall", ("调用者个数（log 比）", "`len(fe['callers'])`", "同 `caller`")),
    ("cbsig", ("被调者**机器码 sha1** 的集合 Jaccard（ICF 归一）", "`fe['callee_bsig']`；`jaccard_sets`",
               "`bsig` 是**未归一**的整函数 sha1：跨构建只要差一个立即数/偏移就完全不同；"
               "ICF 只合并**逐字节相同**的函数 ⇒ 跨 LLVM 版本几乎对不上")),
    ("rbsig", ("调用者机器码 sha1 集合", "`fe['caller_bsig']`", "同 `cbsig`")),
    ("shape", ("操作数形状三元组：寄存器只记宽度类、内存只记基址类、立即数一律记 `i`",
               "`scan_x86` 的 `_parts`；`shape_id`；`_trigrams`",
               "**立即数的符号/宽度被抹掉**（`mov eax,1` 与 `mov eax,-1` 同 token）；指令数 <3 退化")),
    ("scal", ("全部 IMM 立即数，**|v| >= 2**，不在任何节、不是函数地址；**取 abs（丢符号）**",
              "`scan_x86` 里 `abs(op.imm) >= 2 and obj.sec_at(...) is None and tgt not in universe`",
              "**0 / +1 / -1 整段被门槛挡掉**（`-1` 还是 x86-64 最常见的全 1 掩码）；正负号合并")),
    ("wconst", ("|v| >= 0x10000 的立即数，外加 f32/f64 位型解码后的规范化数值（`%.6g`）",
                "`float_tag`；`|x| in [1e-6, 1e15]` 且非 0/NaN/inf；**只返回第一个能解的宽度**",
                "|x|<1e-6 或 |x|>1e15 的浮点字面量；同一立即数只保留一种宽度解释")),
    ("stok", ("非 `.rs` 字符串拆成 **token**：`[A-Za-z][A-Za-z0-9]{2,}`（**长度 >= 3**）",
              "`_TOK_RE`；`str_tokens`", "**2 字符 token**（`id`/`hp`/`xy`/`ui`/`ai`）；"
              "含数字开头的 token（`1p`/`2d`）；长度 <4 的整条字符串（`_cstr` 的 `n<4` 直接丢）")),
])


# ===========================================================================
# 2. 扫描：把**丢弃段**补出来（一次线性反汇编，x86-64）
#
# 与 `CF.scan_x86` 的过滤条件**逐条对齐**：本工具不重算 `scal` 本身（那在 v6 缓存里），
# 只算「被同一个条件挡掉的那一段」，以及对每个条件计数（审计表要用比例）。
# ===========================================================================
_TOK2_RE = re.compile(r"[A-Za-z][A-Za-z0-9]{1,}")
_CAMEL_RE = re.compile(r"(?<=[a-z0-9])(?=[A-Z])")


def tokens_relaxed(s, minlen=2):
    """放宽版 `str_tokens`：`minlen=2`（原版是 3），并保留 camelCase 拆分。"""
    txt = s.decode("ascii", "ignore") if isinstance(s, (bytes, bytearray)) else s
    out = set()
    for w in _TOK2_RE.findall(txt):
        for part in _CAMEL_RE.split(w):
            t = part.lower()
            if len(t) >= minlen and not t.isdigit():
                out.add(t)
    return out


def _cstr_short(obj, va, limit=CF.MAX_STR, minlen=1, maxlen=3):
    """`va` 处是长度 1..maxlen 的可打印 C 字符串吗？——`CF._cstr` 的 `n<4` 挡掉的那一族。"""
    o = obj.va2off(va)
    if o is None:
        return None
    d = obj.data[o:o + limit]
    n = 0
    while n < len(d) and 32 <= d[n] < 127:
        n += 1
    if n < minlen or n > maxlen or n >= len(d):
        return None
    return d[:n]


def scan_x86_holes(obj, universe, verbose=True):
    """扫一份 x86-64 构建，返回 {va: 漏洞字段 dict}（**与 v6 缓存同一套 universe**）。"""
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64
    from capstone.x86 import (X86_OP_IMM, X86_OP_MEM, X86_OP_REG,
                              X86_REG_RIP, X86_REG_RSP, X86_REG_RBP)

    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    tex = obj.sec[".text"]
    code = obj.read(tex["addr"], tex["size"])
    starts = sorted(universe)
    ro = {n: obj.sec[n] for n in CF.STR_SECTIONS if n in obj.sec}
    ro_lo = min((s["addr"] for s in ro.values()), default=0)
    ro_hi = max((s["addr"] + s["size"] for s in ro.values()), default=0)

    def sec_name(va):
        for n, s in ro.items():
            if s["addr"] <= va < s["addr"] + s["size"]:
                return n
        return None

    def new():
        return dict(imm_n=0, imm_lt2=0, imm_0=0, imm_1=0, imm_m1=0, imm_sec=0, imm_func=0,
                    consts_lo=0, consts_hi=0, c64_sgn=0, scal_n=0,
                    icalls=0, call_direct=0, call_resolved=0, callres=0,
                    str_ref=0, str_none=0, str_short=0, str_ok=0,
                    tok_drop=0, tok_short_str=0,
                    small=set(), scal_sgn=set(), c64=set(),
                    stok2=set(), tok=set(), ninsn=0)

    feats = {va: new() for va in universe}
    for insn in CF.disasm_all(cs, code, tex["addr"], tex["addr"] + tex["size"], 1):
        owner = T.owner_of(starts, universe, insn.address)
        if owner is None:
            continue
        fe = feats[owner]
        fe["ninsn"] += 1
        end = insn.address + insn.size
        if insn.mnemonic == "call":
            if insn.operands and insn.operands[0].type == X86_OP_IMM:
                fe["call_direct"] += 1
                if T.owner_of(starts, universe, insn.operands[0].imm) is None:
                    fe["callres"] += 1        # 落不进 universe 的直接调用点（垫片 / 函数中部）
                else:
                    fe["call_resolved"] += 1
                continue
            fe["icalls"] += 1                 # 间接调用：**算了但从来没进打分**
        for op in insn.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                tgt = (end + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
            elif op.type == X86_OP_IMM:
                tgt = op.imm & 0xFFFFFFFFFFFFFFFF
                v = op.imm
                fe["imm_n"] += 1
                inseg = obj.sec_at(v) is not None
                infunc = tgt in universe
                if inseg:
                    fe["imm_sec"] += 1
                elif infunc:
                    fe["imm_func"] += 1
                else:
                    av = abs(v)
                    if av >= 2:                                  # CF 的 scal 门槛
                        fe["scal_n"] += 1
                        fe["scal_sgn"].add(v)                    # 带符号版（测「丢符号」）
                        if av >= 0x10000:
                            pass
                    else:
                        fe["imm_lt2"] += 1
                        if v == 0:
                            fe["imm_0"] += 1
                        elif v == 1:
                            fe["imm_1"] += 1
                        elif v == -1:
                            fe["imm_m1"] += 1
                        fe["small"].add(av)
                    # CF 的 consts 门槛：0x100..0xFFFFFFFF
                    if av < 0x100:
                        fe["consts_lo"] += 1
                    if not (-0x80000000 <= v <= 0xFFFFFFFF):
                        fe["consts_hi"] += 1
                        fe["c64"].add(av)                        # 32 位装不下的常量
            else:
                continue
            lv = None
            if sec_name(tgt):
                fe["str_ref"] += 1
                s = CF.string_at(obj, tgt, ro_lo, ro_hi)
                if s is not None:
                    fe["str_ok"] += 1
                    if not CF.is_loc_file(s):
                        t2 = tokens_relaxed(s, 2)
                        fe["stok2"] = fe["stok2"] | t2
                        t3 = CF.str_tokens(s)
                        fe["tok"] = fe["tok"] | set(t3)
                        fe["tok_drop"] += len(t2 - set(t3))
                else:
                    fe["str_none"] += 1
                    s2 = _cstr_short(obj, tgt)
                    if s2 is not None:
                        fe["str_short"] += 1
                        t2 = tokens_relaxed(s2, 2)
                        fe["stok2"] = fe["stok2"] | t2
                        fe["tok_short_str"] += len(t2)
    for f in feats.values():
        for k in ("small", "scal_sgn", "c64", "stok2", "tok"):
            f[k] = frozenset(f[k])
    if verbose:
        n = len(feats)
        sys.stderr.write(
            "[audit] %-26s %6d 函数；IMM %d（|v|<2 %d / 在节 %d / 是函数地址 %d）；"
            "间接调用 %d；调用点落不进 universe %d；短字符串 %d\n"
            % (os.path.basename(obj.path), n,
               sum(f["imm_n"] for f in feats.values()),
               sum(f["imm_lt2"] for f in feats.values()),
               sum(f["imm_sec"] for f in feats.values()),
               sum(f["imm_func"] for f in feats.values()),
               sum(f["icalls"] for f in feats.values()),
               sum(f["callres"] for f in feats.values()),
               sum(f["str_short"] for f in feats.values())))
    return feats


def audit_cache_dir():
    d = os.environ.get("CFGFP_AUDIT_CACHE") or os.path.join(CF.cache_dir(), "audit")
    os.makedirs(d, exist_ok=True)
    return d


def scan_one(tag, force=False):
    cfg = CF.BUILDS[tag]
    size = os.path.getsize(cfg["path"])
    cp = os.path.join(audit_cache_dir(), "%s-%d-%s.pkl" % (tag, size, AUDIT_VERSION))
    if os.path.exists(cp) and not force:
        with open(cp, "rb") as fh:
            return pickle.load(fh)
    if cfg["kind"] == "pe":
        obj = T.Pe(cfg["path"])
        universe = T.pe_universe(obj)
    else:
        obj = SM.Elf(cfg["path"])
        universe = T.elf_universe(obj, CF.FUNCSTARTS if cfg["funcstarts"] else None)
    if getattr(obj, "em", 62) != 62:
        T.die("%s 不是 x86-64（本工具只审同 ISA 那一半；跨 ISA 是 §35.6 的负结论）" % tag)
    feats = scan_x86_holes(obj, universe)
    st = dict(tag=tag, feats=feats, nfunc=len(universe))
    with open(cp, "wb") as fh:
        pickle.dump(st, fh, protocol=4)
    return st


def cmd_scan(args):
    tags = [t.strip() for t in (args.tags or "linux,win,win_old").split(",") if t.strip()]
    for tag in tags:
        st = scan_one(tag, force=args.force)
        print("%-9s %6d 函数（漏洞字段已缓存）" % (tag, st["nfunc"]))
    return 0


# ===========================================================================
# 3. 把漏洞字段注入 v6 指纹（同 VA），并给出派生集合
# ===========================================================================
def inject(fps, tags):
    for tag in tags:
        st = scan_one(tag)
        fp = fps[tag]
        hf = st["feats"]
        miss = sum(1 for va in fp.universe if va not in hf)
        if miss:
            T.die("%s：%d 个函数在漏洞扫描里找不到（universe 不一致，别混用）" % (tag, miss))
        for va in fp.universe:
            fe = fp.feats[va]
            h = hf[va]
            scal = fe["scal"]
            fe["hole_small"] = h["small"]                 # {0,1}：scal 门槛挡掉的那一段
            fe["hole_scal0"] = scal | h["small"]          # 天真补丁：把门槛去掉
            fe["hole_c64"] = h["c64"]                     # consts 的 >32 位限制挡掉的那一段
            fe["hole_scal64"] = scal | h["c64"]
            fe["hole_scal_sgn"] = h["scal_sgn"]           # scal 的带符号版（测「丢符号」）
            fe["hole_stok2"] = h["stok2"]                 # 长度 >=2 的 token
            fe["hole_stokx"] = h["stok2"] - fe["stok"]    # 只算被丢掉的那一段
            fe["hole_imm01"] = (h["imm_0"], h["imm_1"], h["imm_m1"])
            fe["hole_icall"] = h["icalls"]
            fe["hole_cnt"] = h                              # 审计表用的计数


def _cos3(a, b):
    """3 维计数向量的余弦（0/1/-1 立即数直方图）。"""
    num = a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    if not num:
        return None
    da = math.sqrt(a[0] ** 2 + a[1] ** 2 + a[2] ** 2)
    db = math.sqrt(b[0] ** 2 + b[1] ** 2 + b[2] ** 2)
    return num / (da * db) if da and db else None


# ===========================================================================
# 4. 打分器：`CF.score_same` 的逐行复刻 + 追加漏洞项
#
# ⚠ 追加项**必须排在基础项之后**，权重 0 时 `add()` 直接跳过 ⇒ 基础项的加法顺序与
# `CF.score_same` 完全一致（浮点加法不满足结合律，顺序就是口径）。
# ===========================================================================
def score_audit(qf, df, w):
    tot = wt = 0.0

    def add(weight, val):
        nonlocal tot, wt
        if weight and val is not None:
            tot += weight * val
            wt += weight

    add(w.get("size"), CF.sim_log(qf["size"], df["size"]))
    add(w.get("hist"), CF.cos_hist(qf["hist"], df["hist"]))
    add(w.get("consts"), CF.jaccard(qf["consts"], df["consts"])
        if (qf["consts"] and df["consts"]) else None)
    add(w.get("callee"), CF.multiset_sim(qf["callee_sizes"], df["callee_sizes"]))
    add(w.get("caller"), CF.multiset_sim(qf["caller_sizes"], df["caller_sizes"]))
    add(w.get("strs_s"), CF.jaccard(qf["strs_s_set"], df["strs_s_set"])
        if (qf["strs_s"] and df["strs_s"]) else None)
    add(w.get("strs"), CF.jaccard(qf["strs_set"], df["strs_set"])
        if (qf["strs"] and df["strs"]) else None)
    add(w.get("ncal"), CF.sim_log(len(qf["callees"]), len(df["callees"]))
        if (qf["callees"] or df["callees"]) else None)
    add(w.get("ncall"), CF.sim_log(len(qf["callers"]), len(df["callers"]))
        if (qf["callers"] or df["callers"]) else None)
    add(w.get("cbsig"), CF.jaccard_sets(qf["callee_bsig"], df["callee_bsig"]))
    add(w.get("rbsig"), CF.jaccard_sets(qf["caller_bsig"], df["caller_bsig"]))
    add(w.get("gram"), CF.jaccard(qf["grams"], df["grams"])
        if (qf["grams"] and df["grams"]) else None)
    add(w.get("shape"), CF.jaccard(qf["shgrams"], df["shgrams"])
        if (qf["shgrams"] and df["shgrams"]) else None)
    add(w.get("scal"), CF.jaccard(qf["scal"], df["scal"])
        if (qf["scal"] and df["scal"]) else None)
    add(w.get("scal_c"), CF.jaccard(qf["scal_c"], df["scal_c"])
        if (qf["scal_c"] and df["scal_c"]) else None)
    add(w.get("wconst"), CF.jaccard(qf["wconst"], df["wconst"])
        if (qf["wconst"] and df["wconst"]) else None)
    add(w.get("stok"), CF.jaccard(qf["stok"], df["stok"])
        if (qf["stok"] and df["stok"]) else None)
    add(w.get("cls"), CF.cos_hist(qf["cls"], df["cls"]))
    # ---- 本任务追加：**被丢弃段**（任务 63）----
    add(w.get("small"), CF.jaccard(qf["hole_small"], df["hole_small"])
        if (qf["hole_small"] and df["hole_small"]) else None)
    add(w.get("scal0"), CF.jaccard(qf["hole_scal0"], df["hole_scal0"])
        if (qf["hole_scal0"] and df["hole_scal0"]) else None)
    add(w.get("c64"), CF.jaccard(qf["hole_c64"], df["hole_c64"])
        if (qf["hole_c64"] and df["hole_c64"]) else None)
    add(w.get("scal64"), CF.jaccard(qf["hole_scal64"], df["hole_scal64"])
        if (qf["hole_scal64"] and df["hole_scal64"]) else None)
    add(w.get("scal_sgn"), CF.jaccard(qf["hole_scal_sgn"], df["hole_scal_sgn"])
        if (qf["hole_scal_sgn"] and df["hole_scal_sgn"]) else None)
    add(w.get("stok2"), CF.jaccard(qf["hole_stok2"], df["hole_stok2"])
        if (qf["hole_stok2"] and df["hole_stok2"]) else None)
    add(w.get("stokx"), CF.jaccard(qf["hole_stokx"], df["hole_stokx"])
        if (qf["hole_stokx"] and df["hole_stokx"]) else None)
    add(w.get("icall"), CF.sim_log(qf["hole_icall"], df["hole_icall"]))
    add(w.get("imm01"), _cos3(qf["hole_imm01"], df["hole_imm01"]))
    return tot / wt if wt else 0.0


KEY_ORDER = ("size", "hist", "gram", "consts", "callee", "caller", "strs", "strs_s",
             "ncal", "ncall", "cbsig", "rbsig", "cls", "shape", "scal", "scal_c",
             "wconst", "stok",
             "small", "scal0", "c64", "scal64", "scal_sgn", "stok2", "stokx",
             "icall", "imm01")
ALL_KEYS = KEY_ORDER


def wlab(w):
    return " ".join("%s%.3g" % (k, w[k]) for k in KEY_ORDER if w.get(k)) or "(empty)"


class APair(CF.Pair):
    """`CF.Pair` ＋ 本工具的打分器与漏洞特征。真值/分半/候选池完全复用父类。"""

    def __init__(self, src_tag="win", dst_tag="linux", tags=("linux", "win")):
        fps = CF.load_all(tags=list(tags))
        inject(fps, [t for t in tags])
        super().__init__(src_tag, dst_tag, fps)

    def ev(self, w, addrs, topk=5):
        ok1 = okk = 0
        hit1 = []
        hit5 = []
        pool = self.pool
        win = CF.WIN_SAME
        for a in addrs:
            fe = self.dst.feats[a]
            cands = pool.window(fe["size"], win)
            sc = [(score_audit(fe, pool.fp.feats[c], w), c) for c in cands]
            sc.sort(key=lambda t: (-t[0], t[1]))
            r = sc[:topk]
            t = self.truth[a]
            h1 = bool(r) and r[0][1] == t
            h5 = any(c == t for _s, c in r)
            ok1 += h1
            okk += h5
            hit1.append(1 if h1 else 0)
            hit5.append(1 if h5 else 0)
        return ok1, okk, len(addrs), hit1, hit5


_G = {}


def _eval(job):
    label, w = job
    p = _G["pair"]
    a1, a5, an, ha1, _ha5 = p.ev(w, p.A, _G["topk"])
    b1, b5, bn, hb1, _hb5 = p.ev(w, p.B, _G["topk"])
    return dict(label=label, w=w, a1=a1, a5=a5, an=an, a_hit=ha1,
                b1=b1, b5=b5, bn=bn, b_hit=hb1)


def _run_jobs(fn, jobs, items, label):
    t0 = time.time()
    jobs = max(1, int(jobs or 1))
    if jobs == 1 or len(items) <= 1:
        res = [fn(it) for it in items]
    else:
        ctx = multiprocessing.get_context("fork")
        with ctx.Pool(processes=min(jobs, len(items))) as pool:
            res = list(pool.imap_unordered(fn, items, chunksize=1))
    dt = time.time() - t0
    sys.stderr.write("[audit] %s：%d 项，jobs=%d，墙钟 %.1f s（%.2f s/项）\n"
                     % (label, len(items), min(jobs, max(1, len(items))), dt, dt / max(1, len(items))))
    return res, dt


# ===========================================================================
# 5. 置信区间：bootstrap（百分位）+ Wilson + 配对 bootstrap + McNemar 精确检验
# ===========================================================================
def boot_ci(hits, B=2000, seed=63, alpha=0.05):
    """百分位 bootstrap（对**样本**重采样，不是对模型重采样）。"""
    n = len(hits)
    rng = random.Random(seed)
    vals = []
    for _ in range(B):
        s = 0
        for _i in range(n):
            s += hits[rng.randrange(n)]
        vals.append(s / n)
    vals.sort()
    return vals[int(alpha / 2 * B)], vals[int((1 - alpha / 2) * B) - 1]


def boot_delta_ci(ha, hb, B=2000, seed=63, alpha=0.05):
    """**配对** bootstrap：同一个重采样下标同时作用在 A/B 两套命中向量上 ⇒ 差值的 CI。

    为什么必须配对：两个配置跑的是**同一批**留出样本，样本难度是共同噪声；
    独立重采样会把共同噪声算进差值的方差里，区间被高估。
    """
    n = len(ha)
    assert n == len(hb)
    rng = random.Random(seed)
    vals = []
    for _ in range(B):
        sa = sb = 0
        for _i in range(n):
            j = rng.randrange(n)
            sa += ha[j]
            sb += hb[j]
        vals.append((sb - sa) / n)
    vals.sort()
    return vals[int(alpha / 2 * B)], vals[int((1 - alpha / 2) * B) - 1]


def wilson(k, n, z=1.959963985):
    """Wilson 区间（对**一个**比例的解析区间，不依赖重采样）。"""
    if not n:
        return 0.0, 0.0
    p = k / n
    d = 1 + z * z / n
    c = p + z * z / (2 * n)
    r = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))
    return (c - r) / d, (c + r) / d


def mcnemar_exact(ha, hb):
    """McNemar **精确**检验（二项式版）——同一批样本上两个分类器的成对比较。

    返回 (b, c, p)：b = 只有 A 对、c = 只有 B 对（A = 基线，B = 新配置）。
    """
    b = sum(1 for x, y in zip(ha, hb) if x and not y)
    c = sum(1 for x, y in zip(ha, hb) if y and not x)
    n = b + c
    if n == 0:
        return b, c, 1.0
    # 双尾精确二项检验，p=0.5
    k = min(b, c)
    tail = sum(math.comb(n, i) for i in range(0, k + 1)) / (2.0 ** n)
    return b, c, min(1.0, 2 * tail)


# ===========================================================================
# 6. 配置表
# ===========================================================================
BASE = dict(CF.W_SAME)                       # 51 号基线（去泄漏重训后的默认）
REF = dict(BASE, scal=0.5)                   # 57 号预注册选中值（留出半 78.2%）
FIXED_2P4 = dict(gram=2.0, consts=0.25, shape=0.5, scal=0.5)


def configs(which="all"):
    out = [("00 BASE（51 号基线 W_SAME）", dict(BASE)),
           ("01 REF（57 号：BASE+scal=0.5）", dict(REF))]
    # (a) 留一法：把 REF 里**有权重**的那 5 个特征逐个清零
    for k in ("size", "gram", "consts", "caller", "scal"):
        w = dict(REF)
        w[k] = 0.0
        out.append(("LOO-%-6s（REF − %s）" % (k, k), w))
    # (b) 加入法：把权重为 0 的 11 个特征逐个加进 REF（权重 0.5，与 57 号的扫描同档）
    for k in ("hist", "strs_s", "callee", "cbsig", "rbsig", "ncal", "ncall",
              "wconst", "stok", "shape", "cls"):
        out.append(("ADD-%-6s（REF + %s=0.5）" % (k, k), dict(REF, **{k: 0.5})))
    # (c) 2^4 子集枚举：{gram, consts, shape, scal}
    for bits in itertools.product((0, 1), repeat=4):
        w = {"size": 0.25, "caller": 0.5}
        names = ("gram", "consts", "shape", "scal")
        for b, k in zip(bits, names):
            w[k] = FIXED_2P4[k] if b else 0.0
        lab = "SUB4 " + "".join(k[0].upper() if b else "-" for b, k in zip(bits, names))
        out.append((lab, w))
    if which == "quick":
        return out
    # (d) 权重网格：scal × consts × shape × gram（5×4×3×3 = 180）
    for sc, co, sh, gr in itertools.product((0.0, 0.25, 0.5, 1.0, 2.0),
                                            (0.0, 0.25, 0.5, 1.0),
                                            (0.0, 0.5, 1.0),
                                            (1.0, 2.0, 3.0)):
        w = dict(size=0.25, caller=0.5, scal=sc, consts=co, shape=sh, gram=gr)
        out.append(("GRID scal%.3g consts%.3g shape%.3g gram%.3g" % (sc, co, sh, gr), w))
    # (e) 漏洞补丁网格：imm01 × stok2 × c64 × icall × wconst（3×2×2×2×2 × stokx 2 = 96）
    for im, st, c6, ic, wc, sx in itertools.product((0.0, 0.5, 1.0), (0.0, 0.5), (0.0, 0.5),
                                                    (0.0, 0.3), (0.0, 0.5), (0.0, 0.5)):
        if not (im or st or c6 or ic or wc or sx):
            continue
        w = dict(REF, imm01=im, stok2=st, c64=c6, icall=ic, wconst=wc, stokx=sx)
        out.append(("HOLE imm01=%.3g stok2=%.3g c64=%.3g icall=%.3g wconst=%.3g stokx=%.3g"
                    % (im, st, c6, ic, wc, sx), w))
    # (f) 单点：把每个"洞"单独加回 REF（这是任务书要的那一问）
    for k, v in (("scal0", 0.5), ("small", 0.5), ("stokx", 0.5), ("stok2", 0.5),
                 ("imm01", 1.0), ("c64", 0.5), ("scal64", 0.5), ("icall", 0.3),
                 ("scal_sgn", 0.5)):
        out.append(("FILL-%s=%.3g" % (k, v), dict(REF, **{k: v})))
    return out


def cmd_sweep(args):
    print("[audit] 加载指纹 + 漏洞字段 …", flush=True)
    pair = APair()
    _G["pair"], _G["topk"] = pair, args.topk
    print("[audit] 真值 %s；调参半 %d / 留出半 %d；窗口 %s"
          % (", ".join("%s=%d" % kv for kv in pair.prov.items()),
             len(pair.A), len(pair.B), CF.WIN_SAME), flush=True)
    cfgs = configs("quick" if args.quick else "all")
    if args.only:
        pat = args.only
        cfgs = [c for c in cfgs if pat in c[0]]
    res, dt = _run_jobs(_eval, args.jobs, cfgs, "配置扫描")
    order = {lab: i for i, (lab, _w) in enumerate(cfgs)}
    res.sort(key=lambda r: order.get(r["label"], 1 << 30))
    W = sys.stdout.write
    W("配置 %d 组；墙钟 %.1f s（jobs=%d）；调参半 n=%d / 留出半 n=%d（留出半**不参与选择**）\n\n"
      % (len(res), dt, args.jobs, len(pair.A), len(pair.B)))
    W("%-58s | %-13s | %-13s\n" % ("配置", "调参半 top-1/top-5", "留出半 top-1/top-5"))
    W("%-58s | %-13s | %-13s\n" % ("", "（只这里允许选）", "（只报数）"))
    for r in res:
        W("%-58s | %5.1f%% %5.1f%%  | %5.1f%% %5.1f%%\n"
          % (r["label"][:58], 100.0 * r["a1"] / r["an"], 100.0 * r["a5"] / r["an"],
             100.0 * r["b1"] / r["bn"], 100.0 * r["b5"] / r["bn"]))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("# 任务 63 · 特征消融 / 漏洞补丁（tools/fingerprint_audit.py sweep）\n")
            fh.write("# 真值：%s；分半：调参半 %d / 留出半 %d；窗口 %s；类别=win→linux（同 ISA）\n"
                     % (", ".join("%s=%d" % kv for kv in pair.prov.items()),
                        len(pair.A), len(pair.B), CF.WIN_SAME))
            fh.write("# 留出半只报数；权重只在调参半上选（AGENTS.md 合并纪律 A4）\n")
            fh.write("label\tweights\tn_tune\ttop1_tune\ttop5_tune\tn_hold\ttop1_hold\ttop5_hold\n")
            for r in res:
                fh.write("%s\t%s\t%d\t%.4f\t%.4f\t%d\t%.4f\t%.4f\n"
                         % (r["label"], wlab(r["w"]), r["an"], r["a1"] / float(r["an"]),
                            r["a5"] / float(r["an"]), r["bn"],
                            r["b1"] / float(r["bn"]), r["b5"] / float(r["bn"])))
        W("\n落盘：%s\n" % args.out)
    return res


# ===========================================================================
# 7. 取值范围审计表
# ===========================================================================
def _jset(a, b):
    """集合相似度，**完全按 `score_same` 的守卫**：任一为空 ⇒ None（不进分母）。"""
    return CF.jaccard(a, b) if (a and b) else None


def _jmulti(a, b):
    """`jaccard_sets`：CPU 侧**没有**空集守卫（一边空就是 0.0）。"""
    return CF.jaccard_sets(a, b)


def _msim(a, b):
    """`multiset_sim`：CPU 侧同样没有空集守卫（一边空就是 0.0）。"""
    return CF.multiset_sim(a, b)


def _ncount(a, b):
    """`ncal`/`ncall`：只有**两侧都空**才是 None。"""
    return CF.sim_log(len(a), len(b)) if (a or b) else None


SIM_FEATURES = collections.OrderedDict([
    ("size", lambda q, d: CF.sim_log(q["size"], d["size"])),
    ("hist", lambda q, d: CF.cos_hist(q["hist"], d["hist"])),
    ("gram", lambda q, d: _jset(q["grams"], d["grams"])),
    ("consts", lambda q, d: _jset(q["consts"], d["consts"])),
    ("callee", lambda q, d: _msim(q["callee_sizes"], d["callee_sizes"])),
    ("caller", lambda q, d: _msim(q["caller_sizes"], d["caller_sizes"])),
    ("strs", lambda q, d: _jset(q["strs_set"], d["strs_set"])),
    ("strs_s", lambda q, d: _jset(q["strs_s_set"], d["strs_s_set"])),
    ("ncal", lambda q, d: _ncount(q["callees"], d["callees"])),
    ("ncall", lambda q, d: _ncount(q["callers"], d["callers"])),
    ("cbsig", lambda q, d: _jmulti(q["callee_bsig"], d["callee_bsig"])),
    ("rbsig", lambda q, d: _jmulti(q["caller_bsig"], d["caller_bsig"])),
    ("shape", lambda q, d: _jset(q["shgrams"], d["shgrams"])),
    ("scal", lambda q, d: _jset(q["scal"], d["scal"])),
    ("wconst", lambda q, d: _jset(q["wconst"], d["wconst"])),
    ("stok", lambda q, d: _jset(q["stok"], d["stok"])),
    # 丢弃段（本任务新增）：全部按"集合/计数特征"的同一套守卫
    ("hole:small", lambda q, d: _jset(q["hole_small"], d["hole_small"])),
    ("hole:scal0", lambda q, d: _jset(q["hole_scal0"], d["hole_scal0"])),
    ("hole:c64", lambda q, d: _jset(q["hole_c64"], d["hole_c64"])),
    ("hole:scal_sgn", lambda q, d: _jset(q["hole_scal_sgn"], d["hole_scal_sgn"])),
    ("hole:stok2", lambda q, d: _jset(q["hole_stok2"], d["hole_stok2"])),
    ("hole:stokx", lambda q, d: _jset(q["hole_stokx"], d["hole_stokx"])),
    ("hole:icall", lambda q, d: CF.sim_log(q["hole_icall"], d["hole_icall"])),
    ("hole:imm01", lambda q, d: _cos3(q["hole_imm01"], d["hole_imm01"])),
])


# 特征名 → v6 缓存里的字段名（`ncal`/`ncall` 没有字段，用 len()）
FIELD_OF = {
    "size": "size", "hist": "hist", "gram": "grams", "consts": "consts",
    "callee": "callee_sizes", "caller": "caller_sizes", "strs": "strs",
    "strs_s": "strs_s", "ncal": ("len", "callees"), "ncall": ("len", "callers"),
    "cbsig": "callee_bsig", "rbsig": "caller_bsig", "shape": "shgrams",
    "scal": "scal", "wconst": "wconst", "stok": "stok",
    "small": "hole_small", "scal0": "hole_scal0", "c64": "hole_c64",
    "scal_sgn": "hole_scal_sgn", "stok2": "hole_stok2", "stokx": "hole_stokx",
    "icall": "hole_icall", "imm01": "hole_imm01",
}


def _q(vals, p):
    """分位点（最近秩）。"""
    if not vals:
        return 0
    vals = sorted(vals)
    return vals[min(len(vals) - 1, int(len(vals) * p))]


def cmd_audit(args):
    pair = APair()
    lin, win = pair.fps["linux"], pair.fps["win"]
    named = sorted(CF.named_linux_348())
    tgt = CF.target_addrs(pair.fps)
    W = sys.stdout.write
    rng = random.Random(63)

    # 真配对（331 个可测）与「同窗口随机候选」
    testable = pair.testable
    pairs = []
    for a in testable:
        pairs.append((a, pair.truth[a]))
    rand_pairs = []
    for a in testable:
        cands = pair.pool.window(lin.feats[a]["size"], CF.WIN_SAME)
        if len(cands) > 1:
            for c in rng.sample(cands, min(8, len(cands))):
                rand_pairs.append((a, c))

    W("== (A) 每个特征的取值域 / 丢弃比例（正对照 %d vs 目标集合 %d）==\n\n"
      % (len(named), len(tgt)))
    W("%-15s %-13s %-16s %-13s %-13s %-7s %s\n"
      % ("特征", "空/零率 正/目", "大小/取值中位 正/目", "sim 中位 真/随",
         "sim 均值 真/随", "判别力", "非零率 真/随"))
    rows = []
    for k, fn in SIM_FEATURES.items():
        ckey = FIELD_OF.get(k.split(":")[-1], k.split(":")[-1])

        # 数值型特征（size / 计数 / imm01 三元组）没有「集合大小」，用**值本身**；
        # 其余（集合/多重集）用基数。两类都在同一张表里报，列名写清是"大小/取值"。
        def _sz(fe, ckey=ckey, key=k.split(":")[-1]):
            if isinstance(ckey, tuple):
                return len(fe[ckey[1]])
            v = fe[ckey]
            if isinstance(v, (int, float)):
                return v
            if key == "imm01":
                return sum(v)
            return len(v)
        empty_p = sum(1 for a in named if not _sz(lin.feats[a]))
        empty_t = sum(1 for a in tgt if not _sz(lin.feats[a]))
        sz_p = _q([_sz(lin.feats[a]) for a in named], 0.5)
        sz_t = _q([_sz(lin.feats[a]) for a in tgt], 0.5)
        st = [fn(lin.feats[a], win.feats[b]) for a, b in pairs]
        sr = [fn(lin.feats[a], win.feats[b]) for a, b in rand_pairs]
        st = [v for v in st if v is not None]
        sr = [v for v in sr if v is not None]
        # 中位 + **均值**都报：很多特征是"零膨胀"的（真配对里大多数是 0，少数是 1），
        # 只看中位会得出"没信号"的错误结论；反之只看均值会被覆盖率不对称带偏。
        mst = _q(st, 0.5) if st else 0
        msr = _q(sr, 0.5) if sr else 0
        ast = sum(st) / len(st) if st else 0.0
        asr = sum(sr) / len(sr) if sr else 0.0
        nzt = sum(1 for v in st if v > 0) / len(st) if st else 0.0
        nzr = sum(1 for v in sr if v > 0) / len(sr) if sr else 0.0
        W("%-15s %5.1f%%/%5.1f%%  %8d/%-8d %6.3f/%6.3f %6.3f/%6.3f %+6.3f  %4.0f%%/%4.0f%%\n"
          % (k, 100.0 * empty_p / len(named), 100.0 * empty_t / len(tgt), sz_p, sz_t,
             mst, msr, ast, asr, ast - asr, 100 * nzt, 100 * nzr))
        rows.append((k, empty_p, len(named), empty_t, len(tgt), sz_p, sz_t,
                     round(mst, 4), round(msr, 4), round(ast, 4), round(asr, 4),
                     round(ast - asr, 4), round(100 * nzt, 2), round(100 * nzr, 2)))

    # 窗口召回（size 的"丢弃段"）
    ratios = [(win.feats[pair.truth[a]]["size"] + 1.0) / (lin.feats[a]["size"] + 1.0)
              for a in testable]
    keep = sum(1 for r in ratios if CF.WIN_SAME[0] <= r <= CF.WIN_SAME[1])
    W("\n-- size 的区间截断（候选窗口 [%.2f,%.2f]，`Pool.window`）--\n" % CF.WIN_SAME)
    W("   真配对 size 比落在窗外的 %d/%d = %.1f%%（**这些样本 top-1 必错**）\n"
      % (len(ratios) - keep, len(ratios), 100.0 * (len(ratios) - keep) / len(ratios)))

    # 丢弃量（IMM / 调用 / 字符串）——正对照 vs 目标
    W("\n== (B) 每个阈值**实际扔掉了多少**（只统计 Linux 一侧的指令/串）==\n\n")
    W("%-34s %-14s %-14s %s\n" % ("阈值（代码里的那一行）", "正对照(348)", "目标(12736)", "评价"))
    def tot(addrs, field):
        return sum(lin.feats[a]["hole_cnt"][field] for a in addrs)
    def per(addrs, field):
        return tot(addrs, field) / float(sum(lin.feats[a]["hole_cnt"]["imm_n"] for a in addrs) or 1)
    lines = [
        ("IMM 操作数总数", "imm_n", None),
        ("|v| < 2（scal 门槛挡掉）", "imm_lt2", None),
        ("  其中 v == 0", "imm_0", None),
        ("  其中 v == +1", "imm_1", None),
        ("  其中 v == -1（全 1 掩码）", "imm_m1", None),
        ("落进某个节（当地址扔）", "imm_sec", None),
        ("是函数地址（当地址扔）", "imm_func", None),
        ("< 0x100（consts 门槛挡掉）", "consts_lo", None),
        ("32 位装不下（consts 挡掉）", "consts_hi", None),
        ("间接调用（icalls，未进打分）", "icalls", None),
        ("直接调用点落不进 universe", "callres", None),
        ("字符串引用（只读节内）", "str_ref", None),
        ("  其中 string_at 解不出", "str_none", None),
        ("  长度 1..3（_cstr 的 n<4 挡掉）", "str_short", None),
        ("token 被长度>=3 门槛挡掉（条）", "tok_drop", None),
    ]
    for lab, f, _ in lines:
        W("%-34s %-14d %-14d\n" % (lab, tot(named, f), tot(tgt, f)))
    W("\n   按 IMM 操作数归一：|v|<2 = 正对照 %.1f%% / 目标 %.1f%%；"
      "32 位装不下 = %.2f%% / %.2f%%；间接调用 /直接调用 = %.2f%% / %.2f%%\n"
      % (100.0 * per(named, "imm_lt2"), 100.0 * per(tgt, "imm_lt2"),
         100.0 * per(named, "consts_hi"), 100.0 * per(tgt, "consts_hi"),
         100.0 * tot(named, "icalls") / (tot(named, "call_direct") or 1),
         100.0 * tot(tgt, "icalls") / (tot(tgt, "call_direct") or 1)))

    # 判定：丢弃段有没有携带信息
    W("\n== (C) 每个「洞」的判定（判别力 = 真配对 sim 中位 − 同窗口随机 sim 中位）==\n\n")
    W("%-16s %-10s %-12s %-12s %s\n" % ("洞", "空集率 正/目", "真配对 sim", "随机 sim", "判"))
    verdicts = []
    for r in rows:
        if not r[0].startswith("hole:"):
            continue
        d = r[11]
        if r[1] == len(named) or r[3] == len(tgt):
            v = "有一侧几乎全空 ⇒ 参与打分的格子极少"
        elif d >= 0.15:
            v = "有判别力"
        elif d >= 0.05:
            v = "弱"
        else:
            v = "几乎无判别力"
        W("%-16s %5.1f%%/%5.1f%%  %6.3f      %6.3f      %s\n"
          % (r[0], 100.0 * r[1] / r[2], 100.0 * r[3] / r[4], r[9], r[10], v))
        verdicts.append((r[0], v))

    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("# 任务 63 · 逐特征取值范围审计（tools/fingerprint_audit.py audit）\n")
            fh.write("# 正对照 = 已定名 %d；目标集合 = 无 src Location 的 %d；"
                     "真配对 = %d 个可测的 Location 真值\n"
                     % (len(named), len(tgt), len(testable)))
            fh.write("# sim 中位 = 该特征在真配对上的相似度中位；随机 = 同 size 窗口内随机候选\n")
            fh.write("feature\tempty_pos\tpos_n\tempty_tgt\ttgt_n\t"
                     "median_size_pos\tmedian_size_tgt\t"
                     "sim_med_true\tsim_med_rand\tsim_mean_true\tsim_mean_rand\t"
                     "discrim_mean\tnz_true_pct\tnz_rand_pct\n")
            for r in rows:
                fh.write("%s\t%d\t%d\t%d\t%d\t%d\t%d\t%.4f\t%.4f\t%.4f\t%.4f\t"
                         "%.4f\t%.2f\t%.2f\n" % r)
            fh.write("# ---- 丢弃量（Linux 一侧）----\n")
            fh.write("counter\tpos_total\ttgt_total\n")
            for lab, f, _ in lines:
                fh.write("%s\t%d\t%d\n" % (lab, tot(named, f), tot(tgt, f)))
            fh.write("# 窗口召回：真配对落在 [%.2f,%.2f] 内 %d/%d\n"
                     % (CF.WIN_SAME[0], CF.WIN_SAME[1], keep, len(ratios)))
        W("\n落盘：%s\n" % args.out)
    return rows


# ===========================================================================
# 8. bootstrap 置信区间 + 与 51 号基线的显著性
# ===========================================================================
def cmd_boot(args):
    pair = APair()
    _G["pair"], _G["topk"] = pair, args.topk
    allc = configs("all")
    byname = {}
    for lab, w in allc:
        byname[lab] = w
    picks = []
    for name in (args.configs or "BASE,REF").split(","):
        name = name.strip()
        if not name:
            continue
        if name in ("BASE", "REF"):
            picks.append((name, byname["00 BASE（51 号基线 W_SAME）"] if name == "BASE"
                          else byname["01 REF（57 号：BASE+scal=0.5）"]))
        elif name in byname:
            picks.append((name, byname[name]))
        elif "=" in name:                       # 例如 hole:scal0=.5,hole:icall=.3
            w = dict(REF)
            for kv in name.split("+"):
                kv = kv.strip()
                if not kv:
                    continue
                if kv.startswith("hole:"):
                    k, _, v = kv[5:].partition("=")
                    w[k] = float(v)
                else:
                    k, _, v = kv.partition("=")
                    w[k] = float(v)
            picks.append((name, w))
        else:
            T.die("认不出的配置名 %r（可用：BASE / REF / 配置表里的整名 / `hole:x=.5+...`）" % name)
    res, dt = _run_jobs(_eval, args.jobs, picks, "bootstrap 配置")
    W = sys.stdout.write
    W("bootstrap：B=%d，百分位法，对**留出半样本**重采样；留出半 n=%d\n\n"
      % (args.B, len(pair.B)))
    W("%-46s %-8s %-20s %-20s %s\n"
      % ("配置", "点估计", "bootstrap 95% CI", "Wilson 95% CI", "判"))
    table = {}
    for r in res:
        k, n = r["b1"], r["bn"]
        lo, hi = boot_ci(r["b_hit"], B=args.B, seed=args.seed)
        wl, wh = wilson(k, n)
        W("%-46s %5.2f%%   [%5.2f%%, %5.2f%%]    [%5.2f%%, %5.2f%%]  n=%d\n"
          % (r["label"][:46], 100.0 * k / n, 100.0 * lo, 100.0 * hi,
             100.0 * wl, 100.0 * wh, n))
        table[r["label"]] = r
    # 与 BASE 的配对比较
    base = table.get("BASE")
    if base is None:
        base = res[0]
    W("\n-- 与基线（51 号 72.7%%）的**配对**比较（同一批留出样本）--\n")
    W("%-46s %-10s %-22s %-22s %s\n"
      % ("配置", "Δtop-1", "配对 bootstrap 95% CI", "McNemar b/c", "p"))
    for r in res:
        if r is base:
            continue
        d = (r["b1"] - base["b1"]) / float(base["bn"])
        lo, hi = boot_delta_ci(base["b_hit"], r["b_hit"], B=args.B, seed=args.seed)
        b, c, p = mcnemar_exact(base["b_hit"], r["b_hit"])
        W("%-46s %+5.2f pt   [%+6.2f, %+6.2f] pt   b=%d c=%d            p=%.4g%s\n"
          % (r["label"][:46], 100 * d, 100 * lo, 100 * hi, b, c, p,
             "  **显著**" if p < 0.05 else "  不显著"))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("# 任务 63 · 78.18%% 的 bootstrap 95%% 置信区间（B=%d，百分位法）\n" % args.B)
            fh.write("# 留出半 n=%d（按 Linux 地址 sha1 奇偶分半，留出半不参与选择）\n" % len(pair.B))
            fh.write("label\tweights\tpoint\tn\tboot_lo\tboot_hi\twilson_lo\twilson_hi\t"
                     "delta_vs_base\tdlo\tdhi\tmcnemar_b\tmcnemar_c\tp\n")
            for r in res:
                k, n = r["b1"], r["bn"]
                lo, hi = boot_ci(r["b_hit"], B=args.B, seed=args.seed)
                wl, wh = wilson(k, n)
                d = (k - base["b1"]) / float(n)
                dlo, dhi = boot_delta_ci(base["b_hit"], r["b_hit"], B=args.B, seed=args.seed)
                b, c, p = mcnemar_exact(base["b_hit"], r["b_hit"])
                fh.write("%s\t%s\t%.4f\t%d\t%.4f\t%.4f\t%.4f\t%.4f\t%.4f\t%.4f\t%.4f\t"
                         "%d\t%d\t%.6g\n"
                         % (r["label"], wlab(r["w"]), k / float(n), n, lo, hi, wl, wh,
                            d, dlo, dhi, b, c, p))
        W("\n落盘：%s\n" % args.out)
    return res


# ===========================================================================
def main(argv=None):
    ap = argparse.ArgumentParser(description="任务 63：特征取值范围审计 / 消融 / bootstrap CI")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("scan", help="补扫每个特征被丢弃的那一段（x86-64，缓存到 .agent-work）")
    p.add_argument("--tags", default="linux,win,win_old")
    p.add_argument("--force", action="store_true")
    p.set_defaults(fn=cmd_scan)

    p = sub.add_parser("audit", help="逐特征取值范围审计表（不调参）")
    p.add_argument("--out", default="")
    p.set_defaults(fn=cmd_audit)

    p = sub.add_parser("sweep", help="消融：留一法 / 加入法 / 权重网格 / 2^4 子集 / 漏洞补丁")
    p.add_argument("--jobs", type=int, default=1)
    p.add_argument("--topk", type=int, default=5)
    p.add_argument("--quick", action="store_true", help="只跑留一/加入/子集（不跑大网格）")
    p.add_argument("--only", default="", help="只跑标签里含这个子串的配置")
    p.add_argument("--out", default="")
    p.set_defaults(fn=cmd_sweep)

    p = sub.add_parser("boot", help="bootstrap 95%% CI + 与基线的配对比较")
    p.add_argument("--configs", default="BASE,REF")
    p.add_argument("--jobs", type=int, default=1)
    p.add_argument("--B", type=int, default=2000)
    p.add_argument("--seed", type=int, default=63)
    p.add_argument("--topk", type=int, default=5)
    p.add_argument("--out", default="")
    p.set_defaults(fn=cmd_boot)

    args = ap.parse_args(argv)
    args.fn(args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
