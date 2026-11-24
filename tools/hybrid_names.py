#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""混合定名器：把「顺序轴」（93 号全序列对齐）与「结构指纹」（REF 配置）**按域**组合。

## 为什么要有这个东西（一句话）

两条轴各自的盲区是**互补**的，而且互补的位置已经量出来了：

* 结构指纹在**关卡闭包族（近孪生）**上系统性失效 —— 族内 **14/27 = 51.9%**、族外 **407/454 = 89.6%**
  （`docs/02` §35.12，任务 75；REF = `W_SAME + scal=0.25`，§43.4 预注册）。
* 顺序轴反过来 —— 族内 **37/44 = 84%**（tier A **28/28 = 100%**）、族外 **31/282 = 11%**
  （`docs/02` §35.12b，任务 93 的独立复算；人群 = `fixtures/name_to_addr.tsv` 有地址的 326 条真名）。

所以本工具的规则只有一条实质内容：**在一手证据（指纹）已知失效的那一族里，换成另一手（顺序 + 体量），
其余一律仍用指纹**。族外顺序轴只有 11%，把它当默认会把全库从 76.4% 拖到 20.9%（§35.12b 的口径警告）。

## 三份输入（全部只读）

| 输入 | 文件 | 用它的哪几列 |
|---|---|---|
| 顺序轴 | `fixtures/seq_align_conv93.tsv`（13,263 行） | `arm64_name`（→ 规范名）、`tier`（A/B/C/none） |
| 结构指纹 | `CF.rank()` 的 top-2（REF 权重，见下）；缓存 `CFGFP_CACHE` 不入库 | `top1_addr / top1_score / top2_score`（→ 规范名 + 分数 + margin） |
| campaign 锚 | `fixtures/campaign_anchors75.tsv`（52 锚） | `addr`（→ **可部署**的族判据 D2） |
| 真名（**只做留出验证**） | `fixtures/name_to_addr.tsv` | `x86_addr / module / type / method` |

## 预注册规则（**先写死、先 commit，之后才跑留出验证**；改一条就等于改预注册）

规范名（`NAME_CANON`）：Rust 符号 → `module::Type::method` 三元组，口径与 `tools/name_bridge.py`
的 `demangle + parse_name` **完全相同**（判据来源：`tools/rust_sym.py` 的三形态）。两条**已知的
口径修补**（不改任何规则、只让「同一个名字」在三种写法下判等；`fixtures/name_to_addr.tsv` 自己
就是这么产出的）：

* `ZN…` 开头的 arm64 符号补回前导 `_` 再交给 `c++filt`（93 号的对齐表存的是 ELF 符号名去掉
  `_Z` 前缀之后的形态；实测 1,873 / 9,607 条被赋名的行是这种形态，不补则整批解析失败）；
* 方法名末尾的 `::_{{closure}}` 去掉（真值表的方法名就是**外层方法**口径：326 行里 0 行带 closure；
  一个外层函数与它内部的闭包在 `Location` 口径下同名，所以闭包指向外层方法算**同名**）。

| 编号 | 规则 | 精确写法（代码里的常量） |
|---|---|---|
| **D1** | **标签域**（只用于**测量**，不是可部署判据） | 真名 `method` 里含 `level` 或 `campaign`（不分大小写）⇒ 族内。它是 §35.12b 那 44 行的定义本身；**用了真名，所以不许用于产出** |
| **D2** | **可部署域**（输出表只用它） | `x86_addr ∈ fixtures/campaign_anchors75.tsv` 的 `addr` 列（52 个，truth/target 两侧都算）⇒ 族内。判据是 campaign 数据表的 rel32 xref，**不读任何名字**（§35.12.2 第 2 条：它只能当锚/标签，不能当特征） |
| **R1** | 族内 ∧ 顺序轴 `tier == A` ⇒ **用顺序轴** | `FAMILY_MODE = "order"`，`ORDER_TIER_ADOPT = "A"` |
| **R2** | 其余**一律用指纹** | `DEFAULT_MODE = "fp"`（无候选 ⇒ `mode=none`） |
| **R3** | 两种证据给出**不同**名字 ⇒ **只报不裁** | 名字不发生任何改变（模式仍由 R1/R2 定），逐条写进 `fixtures/hybrid_names_conflicts.tsv`。**没有**"冲突时取分数高的那一个"这类裁决规则 |
| **R4** | 采用源的规范名解析不出来（非 `graphwarrust` 符号 / 空）⇒ `mode=none`，**不**回退到另一个源、**不**看 top-2 | `MODE_NONE` |
| **R5** | 指纹方向的规则**冻结**为 REF（`W_SAME + scal=0.25`，`docs/02` §43.4 预注册选中），**不重新选权重、不再扫网格** | `FP_WEIGHTS_SPEC = "scal=0.25"` |
| **R6** | 留出评估口径：**规范名三元组严格相等**（不是子串）；分母 = 326 行**全体**（无名字 = 未命中）；另报"两轴都有名字"的可比子集。**326 条真名只进评估，不进任何决策** | 见 `cmd_eval` |

**结论只有两种**（`CONCLUSION_RULE`）：族内命中率**提上去了**（混合 > 纯指纹，且族外不掉）⇒
写「族内可从 51.9% 提到 X%」；否则写「不可提，因为…」。

## 输出（三张表，全部是文本夹具）

1. `fixtures/hybrid_names.tsv` —— 13,263 行：`x86_addr  mode(order/fp/none)  名字  置信  依据`
   （依据 = 命中的规则 + 是否冲突；`mode=none` 的行名字留空）
2. `fixtures/hybrid_names_eval.tsv` —— 三列对照（纯指纹 / 纯顺序 / 混合）× 族内 / 族外 / 总体，
   每格给 `n / 命中 / 比例 / Wilson95`；另附 D1（测量域）与 D2（可部署域）两套划分
3. `fixtures/hybrid_names_conflicts.tsv` —— 全部 13,263 行里两种证据给出不同名字的地址（R3 只报不裁）；
   `fixtures/hybrid_names_conflicts_holdout.tsv` —— 其中落在留出半 326 行里的那些，附可核对真值

## 复跑

```bash
export PY=<repo>/.venv/bin/python  CFGFP_CACHE=<cfgfp-v6 缓存目录>     # 缓存 366 MB，不入库
# ① 指纹 top-2（REF）。8 进程在本机会 OOM（win 池 1.12 GB RSS/进程），逐分片串行更稳：
for i in 0 1 2 3 4 5 6 7; do $PY tools/callgraph_gpu_sim.py cpu --weights "scal=0.25" \
    --topk 2 --shard $i/8 --out fp$i.tsv; done
$PY tools/hybrid_names.py build  --fp 'fp*.tsv'
$PY tools/hybrid_names.py eval   --fp 'fp*.tsv'
python3 tools/check_doc_refs.py
```

## 诚实边界（先写下来，免得事后当发现）

1. **D1 用了真名**（`method` 里有没有 `level`）。所以"族内 84%"这句话的**域**本身就是事后定义的；
   可部署的 D2 与它**不完全重合**（326 行里 D2 覆盖 45 行、含 2 行不是 level 方法、漏 1 行 level 方法，
   逐条列在 eval 表的 `D2_*` 行与 conflicts 表里）。两套数都报，不挑好看的那套。
2. **指纹那一列的"名字"是从预测搭档的 Windows 符号转来的**（`module::Type::method`），
   所以它的上限就是指纹 top-1 的上限（族内 §35.12 的 51.9% 是**地址口径**；本表用**名字口径**，
   两者在"预测搭档就是真搭档"时等价，但 ICF / 同名不同函数会让名字口径略高）。
3. **R1 的 `tier == A` 是预注册的**（93 号在 tier A 上 100%，族内非 A 只有 9/16），
   不是在本轮族内数据上挑的。族内 tier A 之外的行仍走指纹，**不许**因为"族内顺序轴也 84%"就整族切过去
   （那是另一种规则，没预注册，也没有独立验证）。
4. **13,263 行输出里的 `mode=fp` 是"候选名"不是"结论名"**：对 12,736 个没有 `Location` 的目标函数，
   指纹只到"结构同源候选、逐条待核"（§35.0 口径块）。这张表的意义是**把两条轴的分工写死并可复算**，
   不是给全库定名。
"""
import argparse
import collections
import glob
import json
import os
import re
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
if HERE not in sys.path:
    sys.path.insert(0, HERE)

FIXTURES = os.path.join(ROOT, "fixtures")
OUT_HYBRID = os.path.join(FIXTURES, "hybrid_names.tsv")
OUT_EVAL = os.path.join(FIXTURES, "hybrid_names_eval.tsv")
OUT_CONFLICTS = os.path.join(FIXTURES, "hybrid_names_conflicts.tsv")

PREREG_VERSION = "hybrid-names-1"

# ===========================================================================
# 预注册（**改这里 = 改预注册**；跑过留出验证之后一格都不许动）
# ===========================================================================
# D1 标签域：真名 method 里含这些关键字（小写比较）⇒ §35.12b 的 44 行就是它。
FAMILY_LABEL_KEYWORDS = ("level", "campaign")
# D2 可部署域：campaign 数据表锚住的 x86 地址（52 个）。名字、分数一律不看。
ANCHOR_FIXTURE = os.path.join(FIXTURES, "campaign_anchors75.tsv")
# R1：族内 + 这个 tier ⇒ 用顺序轴。**不扫阈值**（A 是 93 号预注册档位，合成自检标定的）。
ORDER_TIER_ADOPT = "A"
FAMILY_MODE = "order"
# R2：其余一律用指纹。
DEFAULT_MODE = "fp"
# R4：解析不出规范名 ⇒ none（不回退、不看 top-2）。
MODE_NONE = "none"
# R5：指纹方向冻结在 REF（§43.4 预注册选中）。**不许**在这里扫网格。
FP_WEIGHTS_SPEC = "scal=0.25"
FP_TOP_K = 2                      # top-2 只为了算 margin（§35 的档位判据）
# R6：评估口径 —— 严格规范名相等；分母 = 全体 326；另一格给"两轴都有名字"的可比子集。
EVAL_CRITERION = "strict_canon"   # (module, type, method) 三元组全等
EVAL_DENOMINATOR = "all"          # 无名字 = 未命中
CONCLUSION_RULE = ("混合族内 > 纯指纹族内 且 混合族外 >= 纯指纹族外 ⇒ 报「族内可从 51.9% 提到 X%」；"
                   "否则报「不可提，因为…」")

# 顺序轴夹具（93 号）
SEQ_ALIGN_FIXTURE = os.path.join(FIXTURES, "seq_align_conv93.tsv")
# 真名夹具（**只做留出验证**）
TRUTH_FIXTURE = os.path.join(FIXTURES, "name_to_addr.tsv")

# 域的名字（输出/表格里的列值，便于程序化引用）
DOMAINS = ("D1_family", "D1_outside", "all", "D2_family", "D2_outside")
MODES = ("pure_fp", "pure_order", "hybrid")

# 置信：顺序轴就是档位；指纹轴用 §35.5 的档位（score/margin 阈值与 `CF.tier_from_score` 同一套）
ORDER_CONF_PREFIX = "order_tier="
FP_CONF_FMT = "fp:%s:score=%.4f:margin=%.4f"


# ===========================================================================
# 输入
# ===========================================================================
def rows(path):
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("#") or not line.strip():
                continue
            yield line.rstrip("\n").split("\t")


def load_truth(path=None):
    """{x86_addr: (module, type, method)} —— 只有**有地址**的行才进留出（326 条）。"""
    out = {}
    for r in rows(path or TRUTH_FIXTURE):
        if len(r) < 4 or not r[3].startswith("0x"):
            continue
        out[int(r[3], 16)] = (r[0], r[1] or None, r[2])
    return out


def load_order(path=None):
    """{x86_addr: dict(arm64_name, tier, arm64_addr)}（表头驱动，容忍列序变化）。"""
    out, hdr = {}, None
    for r in rows(path or SEQ_ALIGN_FIXTURE):
        if hdr is None:
            hdr = [c.strip().lower() for c in r]
            if "x86_addr" not in hdr:                 # 没有表头 ⇒ 用 93 号的固定列序
                hdr = ["x86_addr", "x86_size", "in_target", "arm64_addr", "arm64_name",
                       "size_ratio", "q_match", "margin", "tier", "source"]
                r2 = r
            else:
                continue
        d = dict(zip(hdr, r))
        try:
            addr = int(d["x86_addr"], 16)
        except (KeyError, ValueError):
            continue
        a64 = (d.get("arm64_addr") or "").strip()
        out[addr] = dict(arm64_addr=int(a64, 16) if a64.startswith("0x") else None,
                         arm64_name=(d.get("arm64_name") or "").strip(),
                         tier=(d.get("tier") or "").strip())
    return out


def load_anchors(path=None):
    """D2：campaign 表的 52 个锚地址（side=truth/target 都算，判据是"被数据表 xref 到"）。"""
    out = set()
    for r in rows(path or ANCHOR_FIXTURE):
        if r and r[0].startswith("0x"):
            out.add(int(r[0], 16))
    return out


def load_fp_preds(patterns):
    """指纹 top-2 表（允许多分片/多文件；后读的覆盖先读的，同一地址只会有一份）。

    行的列序按 `tools/callgraph_gpu_sim.py` 的 `write_rows`：
        linux_addr, size, ncallers, top1_addr, top1_score, top2_addr, top2_score, ...
    也接受本仓 scratch 里那种 5 列精简表：linux_addr, top1_addr, top1_score, top2_addr, top2_score
    """
    paths = []
    for p in patterns:
        paths.extend(sorted(glob.glob(p)) if any(c in p for c in "*?[") else [p])
    if not paths:
        raise SystemExit("没给 --fp（指纹 top-K 表）；复跑见文件头部的命令块")
    out = {}
    for path in paths:
        if not os.path.exists(path):
            continue
        hdr = None
        for r in rows(path):
            if hdr is None and not r[0].startswith("0x"):
                hdr = [c.strip().lower() for c in r]
                continue
            if hdr is not None:
                d = dict(zip(hdr, r))
                try:
                    addr = int(d["linux_addr"], 16)
                except (KeyError, ValueError):
                    continue
                t1 = d.get("top1_addr", "-")
                t2 = d.get("top2_addr", "-")
                out[addr] = dict(
                    top1=int(t1, 16) if t1.startswith("0x") else None,
                    top1_score=float(d["top1_score"]) if d.get("top1_score") not in (None, "", "-") else None,
                    top2=int(t2, 16) if t2.startswith("0x") else None,
                    top2_score=float(d["top2_score"]) if d.get("top2_score") not in (None, "", "-") else None,
                )
            else:
                out[int(r[0], 16)] = dict(
                    top1=int(r[1], 16) if r[1].startswith("0x") else None,
                    top1_score=float(r[2]), top2=int(r[3], 16) if r[3].startswith("0x") else None,
                    top2_score=float(r[4]))
    for v in out.values():
        v["margin"] = (v["top1_score"] - v["top2_score"]) if v["top1_score"] is not None else 0.0
        v["tier"] = fp_tier(v["top1_score"], v["margin"])
    return out


def fp_tier(score, margin):
    """与 `callgraph_fingerprint.tier_from_score` **同一套阈值**（只读常量，不改）。"""
    import callgraph_fingerprint as CF
    return CF.tier_from_score(score, margin)


# ===========================================================================
# 规范名（`NAME_CANON`）
# ===========================================================================
_CLOSURE_SUFFIXES = ("::_{{closure}}", "::{{closure}}")
_CANON_CACHE = {}


def _parse_demangled(d):
    """demangle 之后的串 → (module, type, method)；不是本 crate 的符号（`name_bridge.parse_name`
    返回 None）或方法名为空 ⇒ None。末尾 `::_{{closure}}` 去掉（真值表就是外层方法口径）。"""
    import name_bridge as NB
    p = NB.parse_name(d)
    if p is None:
        return None
    module, owner, method, _is_clo = p
    if not method:
        return None
    for suf in _CLOSURE_SUFFIXES:
        while method.endswith(suf):
            method = method[:-len(suf)]
    return (module, owner, method) if method else None


def _norm_mangled(name):
    return ("_" + name) if name.startswith("ZN") else name


def prime_canon(names):
    """一次 `c++filt` 批处理填满缓存（13,263 个符号逐个起子进程要几分钟，批量是几秒）。"""
    import name_bridge as NB
    todo = sorted({n for n in names if n and n not in _CANON_CACHE})
    if not todo:
        return
    for n, d in zip(todo, NB.demangle([_norm_mangled(n) for n in todo])):
        _CANON_CACHE[n] = _parse_demangled(d)


def to_canon(name):
    """mangled / 已 demangle 的 Rust 符号 → `(module, type, method)` 或 None。

    两条口径修补（见文件头的 `NAME_CANON`）：补 `ZN` 的前导 `_`、去掉末尾 `::_{{closure}}`。
    """
    if not name:
        return None
    if name not in _CANON_CACHE:
        prime_canon([name])
    return _CANON_CACHE.get(name)


def fmt_canon(c):
    if not c:
        return ""
    return "%s::%s::%s" % (c[0], c[1] or "?", c[2])


def load_win_names(addrs):
    """{Windows VA: 规范名} —— 只覆盖 `addrs` 里出现的地址（一次 `c++filt` 批处理）。"""
    import callgraph_fingerprint as CF
    fp = CF.load_fp("win")
    want = [a for a in addrs if a in (fp.names or {})]
    mangled, owner = [], []
    for va in want:
        for nm in sorted(fp.names[va]):
            mangled.append(("_" + nm) if nm.startswith("ZN") else nm)
            owner.append(va)
    out = {}
    if not mangled:
        return out
    import name_bridge as NB
    for va, d in zip(owner, NB.demangle(mangled)):
        c = _parse_demangled(d)
        if c is not None:
            out.setdefault(va, c)     # 同一地址有多个符号别名时取字典序第一个可解析者（确定性）
    return out


# ===========================================================================
# 域 + 规则
# ===========================================================================
def family_label(truth_row):
    """D1（标签域，只用于测量）：真名 method 含 `FAMILY_LABEL_KEYWORDS`。"""
    method = (truth_row[2] or "").lower()
    return any(k in method for k in FAMILY_LABEL_KEYWORDS)


def family_deployable(addr, anchors):
    """D2（可部署域）：被 campaign 数据表锚住。"""
    return addr in anchors


def decide(addr, order_row, fp_pred, order_canon, fp_canon, anchors):
    """R1/R2/R3/R4：返回 (mode, name_canon, conf, basis, conflict_info)。

    * `conflict_info` 只**报告**（R3）：两种证据都有名字且不相等。
    * 采用的名字**只**由 R1/R2 决定，与冲突与否无关（"只报不裁"）。
    """
    d2 = family_deployable(addr, anchors)
    tier = (order_row or {}).get("tier", "")
    if d2 and tier == ORDER_TIER_ADOPT:
        if not order_canon:                     # R4：采用源解析不出规范名 ⇒ none，**不回退**
            return MODE_NONE, None, "", "R4:D2-family+tierA-but-no-canon-name", None
        mode, name = FAMILY_MODE, order_canon
        conf = ORDER_CONF_PREFIX + tier
        basis = "R1:D2-family+tierA"
    elif fp_canon:
        mode, name = DEFAULT_MODE, fp_canon
        conf = FP_CONF_FMT % (fp_pred["tier"], fp_pred["top1_score"], fp_pred["margin"])
        basis = "R2:D2-family-nontierA" if d2 else "R2:outside-D2"
    else:
        mode, name, conf = MODE_NONE, None, ""
        basis = "R4:no-canon-name" + ("(D2-family" if d2 else "")
    conflict = None
    if order_canon and fp_canon and order_canon != fp_canon:
        conflict = dict(order=order_canon, fp=fp_canon)
        basis += "+R3:conflict"
    return mode, name, conf, basis, conflict


# ===========================================================================
# 统计
# ===========================================================================
def wilson(hits, n, z=1.96):
    if not n:
        return (0.0, 0.0, 0.0)
    p = hits / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * ((p * (1 - p) / n + z * z / (4 * n * n)) ** 0.5) / d
    return (p, max(0.0, c - h), min(1.0, c + h))


def pct(hits, n):
    return "%d/%d = %.1f%%" % (hits, n, 100.0 * hits / n) if n else "0/0"


# ===========================================================================
# 子命令
# ===========================================================================
def _win_name_table(preds_or_addrs):
    addrs = set()
    for a in preds_or_addrs:
        if a:
            addrs.add(a)
    return load_win_names(addrs)


def cmd_build(args):
    """写出 `fixtures/hybrid_names.tsv`（13,263 行）+ `fixtures/hybrid_names_conflicts.tsv`。"""
    t0 = time.time()
    truth = load_truth(args.truth)
    order = load_order(args.align)
    anchors = load_anchors(args.anchors)
    preds = load_fp_preds(args.fp)
    addrs = sorted(order)
    # 指纹名字：只对"预测搭档"取一次名字表
    fp_name_of = load_win_names({p["top1"] for p in preds.values() if p["top1"]})
    prime_canon(order[a]["arm64_name"] for a in addrs)
    order_canon = {a: to_canon(order[a]["arm64_name"]) for a in addrs}
    fp_canon = {a: fp_name_of.get(preds[a]["top1"]) if a in preds and preds[a]["top1"] else None
                for a in addrs}
    n_conf = 0
    out_rows = []
    conflict_rows = []
    for a in addrs:
        o = order[a]
        p = preds.get(a) or dict(top1=None, top1_score=None, top2=None, top2_score=None,
                                 margin=0.0, tier="未知")
        mode, name, conf, basis, conflict = decide(a, o, p, order_canon.get(a), fp_canon.get(a), anchors)
        out_rows.append((a, mode, fmt_canon(name), conf, basis))
        if conflict:
            n_conf += 1
            conflict_rows.append((a, o["tier"], fmt_canon(conflict["order"]),
                                  ("0x%x" % p["top1"]) if p["top1"] else "-",
                                  "" if p["top1_score"] is None else "%.17g" % p["top1_score"],
                                  "" if p["margin"] is None else "%.17g" % p["margin"],
                                  fmt_canon(conflict["fp"]), mode))
    with open(args.out, "w", encoding="utf-8") as fh:
        fh.write("# 混合定名器（%s）：顺序轴（93 全序列对齐）× 结构指纹（REF=W_SAME+scal=0.25），按域组合\n"
                 % PREREG_VERSION)
        fh.write("# 规则 D2/R1/R2/R3/R4 见 tools/hybrid_names.py 文件头（预注册）；"
                 "D1 标签域只用于测量，不进本表\n")
        fh.write("# fp = 预测搭档的 Windows 符号规范名（module::Type::method），**候选名、逐条待核**\n")
        fh.write("x86_addr\tmode\tname\tconfidence\tbasis\n")
        for a, mode, name, conf, basis in out_rows:
            fh.write("0x%x\t%s\t%s\t%s\t%s\n" % (a, mode, name, conf, basis))
    with open(args.conflicts, "w", encoding="utf-8") as fh:
        fh.write("# 冲突表（R3：**只报不裁**）：两种证据都给出了规范名且不相等。采用的名字不受冲突影响。\n")
        fh.write("# order_name = 顺序轴（93 对齐）的名字；fp_name = 指纹 top-1 搭档的 Windows 名字。\n")
        fh.write("x86_addr\torder_tier\torder_name\tfp_top1_addr\tfp_score\tfp_margin\tfp_name\tadopted_mode\n")
        for r in conflict_rows:
            fh.write("0x%x\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" % r)
    cnt = collections.Counter(r[1] for r in out_rows)
    sys.stderr.write("[build] %d 行 → %s（%.1fs）\n" % (len(out_rows), args.out, time.time() - t0))
    for m in (FAMILY_MODE, DEFAULT_MODE, MODE_NONE):
        sys.stderr.write("[build]   mode=%-5s %5d\n" % (m, cnt.get(m, 0)))
    sys.stderr.write("[build] 冲突 %d 条 → %s\n" % (n_conf, args.conflicts))
    return 0


def cmd_eval(args):
    """留出验证：纯指纹 / 纯顺序 / 混合 三列 × 域（D1 测量域 + D2 可部署域）。"""
    truth = load_truth(args.truth)
    order = load_order(args.align)
    anchors = load_anchors(args.anchors)
    preds = load_fp_preds(args.fp)
    addrs = sorted(truth)
    fp_name_of = load_win_names({p["top1"] for p in preds.values() if p["top1"]})
    fp_canon = {a: fp_name_of.get(preds[a]["top1"]) if a in preds and preds[a]["top1"] else None
                for a in addrs}
    prime_canon((order.get(a) or {}).get("arm64_name", "") for a in addrs)
    order_canon = {a: to_canon((order.get(a) or {}).get("arm64_name", "")) for a in addrs}
    # 三列的"名字"（None = 无名字 = 未命中）
    pure = dict(pure_fp={}, pure_order={}, hybrid={})
    conf = {}
    d1 = {a: family_label(truth[a]) for a in addrs}
    d2 = {a: family_deployable(a, anchors) for a in addrs}
    for a in addrs:
        o = order.get(a) or dict(tier="", arm64_name="")
        p = preds.get(a) or dict(top1=None, top1_score=None, top2=None, top2_score=None,
                                margin=0.0, tier="未知")
        mode, name, c, basis, conflict = decide(a, o, p, order_canon.get(a), fp_canon.get(a), anchors)
        pure["pure_fp"][a] = fp_canon.get(a)
        pure["pure_order"][a] = order_canon.get(a)
        pure["hybrid"][a] = name
        conf[a] = (mode, c, basis, conflict, o.get("tier", ""))
    # 域
    def dom_addrs(d):
        if d == "all":
            return addrs
        if d == "D1_family":
            return [a for a in addrs if d1[a]]
        if d == "D1_outside":
            return [a for a in addrs if not d1[a]]
        if d == "D2_family":
            return [a for a in addrs if d2[a]]
        return [a for a in addrs if not d2[a]]
    # 可比子集：两轴都有名字（口径 R6 的第二格）
    comparable = [a for a in addrs if pure["pure_fp"].get(a) and pure["pure_order"].get(a)]
    out = []
    for table, pool in (("A_all", addrs), ("B_comparable", comparable)):
        for d in DOMAINS:
            sub = [a for a in dom_addrs(d) if a in (pool if table == "B_comparable" else addrs)]
            if not sub:
                continue
            for m in MODES:
                hits = sum(1 for a in sub if pure[m].get(a) == truth[a])
                p, lo, hi = wilson(hits, len(sub))
                out.append(dict(table=table, domain=d, mode=m, n=len(sub), hits=hits,
                                acc=round(p, 4), wilson_lo=round(lo, 4), wilson_hi=round(hi, 4)))
    # 口径复现（§35.12b 的"子串判据"那套，证明本表不是另起口径）
    def loose_hit(a):
        t = truth[a]
        nm = (order.get(a) or {}).get("arm64_name", "")
        return bool(nm) and bool(t[1]) and (t[1] in nm) and (t[2] in nm)
    loose = []
    for d in ("D1_family", "D1_outside", "all"):
        sub = dom_addrs(d)
        hits = sum(1 for a in sub if loose_hit(a))
        p, lo, hi = wilson(hits, len(sub))
        loose.append(dict(domain=d, n=len(sub), hits=hits, acc=round(p, 4),
                          wilson_lo=round(lo, 4), wilson_hi=round(hi, 4)))
    # 冲突
    conflicts = [a for a in addrs if conf[a][3]]
    c_rows = []
    for a in conflicts:
        oc, fc = conf[a][3]["order"], conf[a][3]["fp"]
        c_rows.append(dict(addr=a, mode=conf[a][0], order=fmt_canon(oc), fp=fmt_canon(fc),
                           order_right=(oc == truth[a]), fp_right=(fc == truth[a]),
                           truth=fmt_canon(truth[a]), tier=conf[a][4]))
    # 写表
    with open(args.out, "w", encoding="utf-8") as fh:
        fh.write("# 混合定名器（%s）留出验证：纯指纹 / 纯顺序 / 混合 三列对照\n" % PREREG_VERSION)
        fh.write("# 人群 = fixtures/name_to_addr.tsv 有地址的 %d 条真名（**只验证、不参与任何决策**）\n" % len(addrs))
        fh.write("# 口径 = 规范名严格相等（module::Type::method）；分母 = 全体；无名字 = 未命中（R6）\n")
        fh.write("# cmp=bench 是 B 表：两轴都有名字的可比子集（%d 行）\n" % len(comparable))
        fh.write("criterion\ttable\tdomain\tmode\tn\thits\tacc\twilson_lo\twilson_hi\n")
        for r in out:
            fh.write("%s\t%s\t%s\t%s\t%d\t%d\t%.4f\t%.4f\t%.4f\n"
                     % (EVAL_CRITERION, r["table"], r["domain"], r["mode"], r["n"], r["hits"],
                        r["acc"], r["wilson_lo"], r["wilson_hi"]))
        for r in loose:
            fh.write("loose_substring\tA_all\t%s\tpure_order\t%d\t%d\t%.4f\t%.4f\t%.4f\n"
                     % (r["domain"], r["n"], r["hits"], r["acc"], r["wilson_lo"], r["wilson_hi"]))
        fh.write("# 附加：冲突 %d 条（其中 顺序对 %d / 指纹对 %d / 都错 %d）\n"
                 % (len(c_rows), sum(1 for r in c_rows if r["order_right"] and not r["fp_right"]),
                    sum(1 for r in c_rows if r["fp_right"] and not r["order_right"]),
                    sum(1 for r in c_rows if not r["order_right"] and not r["fp_right"])))
        fh.write("# 结论规则：%s\n" % CONCLUSION_RULE)
    with open(args.conflicts_out, "w", encoding="utf-8") as fh:
        fh.write("# 留出半的冲突明细（两轴都有名字且不同）：**只报不裁**；truth 只用于事后核对\n")
        fh.write("x86_addr\torder_tier\torder_name\tfp_name\ttruth_name\torder_right\tfp_right\tadopted_mode\n")
        for r in c_rows:
            fh.write("0x%x\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n"
                     % (r["addr"], r["tier"], r["order"], r["fp"], r["truth"],
                        "1" if r["order_right"] else "0", "1" if r["fp_right"] else "0", r["mode"]))
    # 摘要
    def get(table, d, m):
        for r in out:
            if r["table"] == table and r["domain"] == d and r["mode"] == m:
                return r
        return None
    print("[eval] 人群 %d 行；D1 族内 %d / 族外 %d；D2 族内 %d / 族外 %d；可比子集 %d"
          % (len(addrs), sum(d1.values()), len(addrs) - sum(d1.values()),
             sum(d2.values()), len(addrs) - sum(d2.values()), len(comparable)))
    print("%-11s %-10s %-12s %s" % ("table", "domain", "mode", "命中"))
    for r in out:
        if r["table"] == "A_all":
            print("%-11s %-10s %-12s %s  [%.1f, %.1f]"
                  % (r["table"], r["domain"], r["mode"], pct(r["hits"], r["n"]),
                     100 * r["wilson_lo"], 100 * r["wilson_hi"]))
    print("[eval] 冲突 %d 条" % len(c_rows))
    return 0


def cmd_selfcheck(args):
    """**不碰指纹、不碰真名的规则自检**：域计数 + 顺序轴口径复现（§35.12b）。"""
    truth = load_truth(args.truth)
    order = load_order(args.align)
    anchors = load_anchors(args.anchors)
    d1 = {a for a in truth if family_label(truth[a])}
    d2 = anchors & set(truth)
    def loose(a):
        t = truth[a]
        nm = (order.get(a) or {}).get("arm64_name", "")
        return bool(nm) and bool(t[1]) and (t[1] in nm) and (t[2] in nm)
    prime_canon((order.get(a) or {}).get("arm64_name", "") for a in truth)

    def strict(a):
        return to_canon((order.get(a) or {}).get("arm64_name", "")) == truth[a]
    checks = []
    checks.append(("留出 326 行", len(truth) == 326))
    checks.append(("D1 族内 = 44（§35.12b 的定义）", len(d1) == 44))
    checks.append(("D2 族内 = 45（campaign 锚 ∩ 留出）", len(d2) == 45))
    checks.append(("顺序轴 D1 族内 37/44（loose 口径复现 §35.12b）",
                   sum(map(loose, d1)) == 37 and len(d1) == 44))
    checks.append(("顺序轴 D1 族内 tierA 28/28（§35.12b）",
                   sum(1 for a in d1 if (order.get(a) or {}).get("tier") == "A") == 28
                   and sum(1 for a in d1 if (order.get(a) or {}).get("tier") == "A" and loose(a)) == 28))
    checks.append(("顺序轴族外 31/282（§35.12b）",
                   sum(1 for a in truth if a not in d1 and loose(a)) == 31
                   and len([a for a in truth if a not in d1]) == 282))
    checks.append(("loose 与 strict 在 326 行上逐行一致（规范名口径不改变 §35.12b 的数）",
                   all(loose(a) == strict(a) for a in truth)))
    for name, ok in checks:
        print("%s  %s" % ("OK  " if ok else "FAIL", name))
    return 0 if all(ok for _n, ok in checks) else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n")[0])
    ap.add_argument("cmd", choices=["build", "eval", "selfcheck", "prereg"])
    ap.add_argument("--fp", nargs="*", default=[], help="指纹 top-K 表（可给多分片/通配符）")
    ap.add_argument("--align", default=SEQ_ALIGN_FIXTURE)
    ap.add_argument("--truth", default=TRUTH_FIXTURE)
    ap.add_argument("--anchors", default=ANCHOR_FIXTURE)
    ap.add_argument("--out", default="")
    ap.add_argument("--conflicts", default=OUT_CONFLICTS)
    ap.add_argument("--conflicts-out", default="")
    a = ap.parse_args(argv)
    if a.cmd == "prereg":
        print(json.dumps(dict(version=PREREG_VERSION, family_label_keywords=FAMILY_LABEL_KEYWORDS,
                              order_tier_adopt=ORDER_TIER_ADOPT, family_mode=FAMILY_MODE,
                              default_mode=DEFAULT_MODE, fp_weights=FP_WEIGHTS_SPEC,
                              criterion=EVAL_CRITERION, denominator=EVAL_DENOMINATOR,
                              conclusion_rule=CONCLUSION_RULE), ensure_ascii=False, indent=1))
        return 0
    if a.cmd == "selfcheck":
        return cmd_selfcheck(a)
    if a.cmd == "build":
        a.out = a.out or OUT_HYBRID
        return cmd_build(a)
    a.out = a.out or OUT_EVAL
    a.conflicts_out = a.conflicts_out or os.path.join(FIXTURES, "hybrid_names_conflicts_holdout.tsv")
    return cmd_eval(a)


if __name__ == "__main__":
    sys.exit(main())
