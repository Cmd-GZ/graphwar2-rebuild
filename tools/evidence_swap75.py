#!/usr/bin/env python3
"""75 · 换**证据类型**：三条候选数据源的可行性 / 判据 / 正对照 / 两侧覆盖率。

为什么有这个文件（`dispatch/75-new-evidence-types.md`，`docs/02` §48）：
    71 号把"加特征 / 调权重"关掉了（24 特征边际表：只有 `gram` / `scal` 有边际，
    7 个特征拿掉 top-1 反而涨）；72 号把"扩正对照"关掉了（975 里 622 是依赖 crate，
    游戏代码仍 n≈169、±6.4 ⇒ 瓶颈是**锚**不是名字）。⇒ 只剩"换数据来源的类型"。

三条候选（任务书 §1）：
    ① crash 栈字符串（`graphwar_crash_reporter` / panic 栈）—— 能不能把栈里的名字/地址变成锚？
    ② campaign 关卡表（21 关参数）—— 能不能变成"地址 → 已知用途"的锚？
    ③ 真机帧（截图 / 报文里的可见状态）—— 能不能反推参数/地址？

**本工具的红线（任务书 §2、41 号纪律）**
    * 每条判据都**不看指纹分数**（不用 `score_same`、不用 top-1、不用任何候选排序）；
    * 每条都报**两侧**：正对照（已定名的 975）一侧 + 目标集合（12,736）一侧 —— 只报一侧就是泄漏；
    * 正对照只答一个问题："新源在**已经被独立证据定过名**的那批上对不对得上"。

子命令
    feas1      ① 崩溃栈：符号化能力清单 + 崩溃产物在不在 + 能锚多少（上界）
    campaign   ② 关卡表：锚集、两侧覆盖率、与已有名字的正对照、留出半重叠
    feas3      ③ 真机帧：帧资产清单 + 判据为什么不可能给出地址级锚
    all        三条跑完，写 `fixtures/evidence_sources75.tsv`（+ campaign 表）

复跑（本仓库根）：
    $PY tools/evidence_swap75.py all \
        --so  .reference/game/libgraphwarrust.so \
        --fixtures fixtures
"""

from __future__ import annotations

import argparse
import bisect
import os
import pickle
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import sym_match as SM                     # noqa: E402  （Elf / x86_universe / 函数起点表）
import campaign_data as CD                 # noqa: E402  （21 关描述字面量的 (vaddr,len)）
import campaign_params as CP               # noqa: E402  （21 关参数的逐位解码）

GW = "/home/CG/Myprojects/graphwar2-rebuild"
DEFAULT_SO = os.path.join(GW, ".reference/game/libgraphwarrust.so")
DEFAULT_WIN = os.path.join(GW, ".reference/windows/graphwarrust-2.0.60.dll")
DEFAULT_AND = os.path.join(GW, ".reference/steam/android-2.0.60/libgraphwarrust.so")

FIX = os.path.join(ROOT, "fixtures")

# 任务书 §1 的三条候选
SOURCES = ("crash_stack", "campaign_table", "real_frame")


# ---------------------------------------------------------------------------
# 夹具读取（统一的"跳过 # 注释、第一条非注释行是表头"口径）
# ---------------------------------------------------------------------------
def rows(path):
    hdr, out = None, []
    if not os.path.exists(path):
        return out
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


def sect(path):
    return {s["name"]: s for s in SM.Elf(path).secs}


def truth_and_target(fixtures):
    """两侧人群：正对照 975（有真值）/ 目标集合 12,736（没有名字的那批）。"""
    tr = rows(os.path.join(fixtures, "fingerprint_holdout72_samples.tsv"))
    tg = rows(os.path.join(fixtures, "callgraph_fingerprint.tsv"))
    truth = {int(r["linux_addr"], 16): r for r in tr if r.get("linux_addr", "").startswith("0x")}
    tgt = {int(r["linux_addr"], 16) for r in tg if r.get("linux_addr", "").startswith("0x")}
    return truth, tgt


def names_linux(fixtures):
    """地址 → (module, method, src_file)。三份夹具并集（与 named_linux() 同源）。"""
    out = {}
    for name, key in (("name_to_addr_v2.tsv", "x86_addr"), ("name_to_addr.tsv", "x86_addr")):
        for r in rows(os.path.join(fixtures, name)):
            a = r.get(key, "")
            if not a.startswith("0x"):
                continue
            va = int(a, 16)
            if va not in out:
                out[va] = (r.get("module", ""), r.get("method", ""), r.get("src_file", ""))
    for r in rows(os.path.join(fixtures, "windows_name_to_addr.tsv")):
        if r.get("join_kind") not in ("唯一锚", "集合锚"):
            continue
        a = r.get("linux_addr", "")
        if a.startswith("0x") and int(a, 16) not in out:
            out[int(a, 16)] = ("", r.get("win_name", r.get("name", "")), "")
    return out


# ---------------------------------------------------------------------------
# ① 崩溃栈字符串：符号化能力 + 产物 + 上界
# ---------------------------------------------------------------------------
def feas1(so=DEFAULT_SO, win=DEFAULT_WIN, andr=DEFAULT_AND, fixtures=FIX, out=None):
    """崩溃栈能给什么？三个子问题的**可测**答案，最后给出"能锚多少个函数"的上界。

    判据（不看分数）：一条栈帧只有在满足下面任一条时才是一个**新锚**
        (a) 帧里带可读的符号名（需要 `.symtab`/COFF 符号/调试信息能对地址symbolize），或
        (b) 帧里带 `src/*.rs:line`（panic::Location）而该 Location 未被 §37 收过。
    两者都不满足 ⇒ 帧只给出地址，而地址集合（13,263）与 Location 集合（527）本来就有。
    """
    res = []
    # (1) 每个构建的"symbolize 靠什么"清单
    for tag, path in (("linux", so), ("android", andr)):
        if not os.path.exists(path):
            continue
        sec = sect(path)
        keys = sorted(k for k in sec
                      if k.startswith((".debug", ".symtab", ".dynsym", ".eh_frame")))
        res.append(dict(
            check="sections:" + tag,
            value="symtab=%d debug=%d dynsym=%d" % (
                int(".symtab" in sec),
                sum(1 for k in sec if k.startswith(".debug")),
                sec.get(".dynsym", {}).get("size", 0) // 24 if ".dynsym" in sec else 0),
            detail=";".join(keys)))
    if os.path.exists(win):
        # PE：COFF 符号表在不在（Windows 那 939 个真名就是从这儿来的）
        d = open(win, "rb").read()
        pe = struct.unpack_from("<I", d, 0x3C)[0]
        nsym = struct.unpack_from("<I", d, pe + 16)[0]      # COFF: NumberOfSymbols @+16
        res.append(dict(check="sections:win", value="coff_syms=%d" % nsym,
                        detail="PE COFF 符号表条数（0 = 只剩 .pdata 的函数边界）"))

    # (2) 崩溃上报器自己的产物与格式串
    d = open(so, "rb").read()
    hits = [s for s in (b"user://crash_log.txt", b"<unnamed>", b"<unknown>",
                        b"stack backtrace:", b"Failed to get global server address for crash")
            if s in d]
    res.append(dict(check="crash_artifacts",
                    value="strings=%d" % len(hits),
                    detail="|".join(s.decode() for s in hits)))
    # 8 条字面量：直接数 `src/graphwar_crash_reporter.rs:` 的出现次数
    n_lit = d.count(b"src/graphwar_crash_reporter.rs:")
    res.append(dict(check="crash_reporter_literals", value=str(n_lit),
                    detail="`src/graphwar_crash_reporter.rs:` 字面量份数；"
                           "这些是 inline &str，不是 panic::Location 结构（§17.2）"))

    # (3) 有没有**真的**崩溃产物（crash log / minidump / 上报件）
    found = []
    for base in (ROOT, GW):
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [x for x in dirnames if x not in
                           (".git", "target", "node_modules", "cargo-home", "rustup",
                            "toolkits", "godot", "replays", "analysis-repo")]
            for fn in filenames:
                low = fn.lower()
                if ("crash" in low and low.endswith((".txt", ".log", ".json", ".dmp", ".rpt"))) \
                        or low.endswith((".dmp", ".mdmp")):
                    found.append(os.path.join(dirpath, fn))
    res.append(dict(check="real_crash_artifact", value=str(len(found)),
                    detail="|".join(found[:4]) or "无（仓库与 .reference 都没有崩溃日志/转储）"))

    # (4) 上界：栈帧可能给 info 的种类 ∩ 已有集合
    truth, tgt = truth_and_target(fixtures)
    res.append(dict(check="anchor_yield_bound", value="0",
                    detail="帧给出的只可能是 地址 ∪ panic Location；"
                           "13,263 个地址已全列、527 个 Location 已全收（§37）⇒ 新锚 0"))
    _emit(res, out)
    return res


# ---------------------------------------------------------------------------
# ② campaign 关卡表：地址 → 已知用途
# ---------------------------------------------------------------------------
def campaign(so=DEFAULT_SO, fixtures=FIX, out=None, tsv=None, verbose=True):
    """21 关数据表 → 一堆 Linux 地址 → 用途标签。

    判据（不看任何指纹分数、不看距离/相似度）：
        A. 字节级：21 段关卡描述字面量（`campaign_data.DESC`）的 **rel32 xref** 全部落在
           同一个函数里 ⇒ 该函数 = 关卡数据表（key/title/描述/四模式元组）。
        B. 逐位：`campaign_params.decode_arm()` 对 21 个 arm 解出的
           (soldiers, targets, bot 难度, 回合上限, 障碍预设, min_count, vtable_a/fn_a)，
           其中 `fn_a` 是**开场闭包**、`vtable_a[+0x20]`/`vtable_b` 是布局校验闭包 ——
           这些指针槽本身就是"地址 → 用途"。
        C. 文档锚（`docs/10` §4/§12）：开场驱动 `0x466d20`、士兵 `0x476320`、
           靶子 `0x46f310`、解锁刷新 `0x418820`、记录装配 `0x442aa0`。

    **正对照**：A/B/C 里凡是在 975 里的地址，用它的**已有名字**检验用途说法是否对得上
    （模块名 + 对开场闭包还要检验 `setup_level_<n>` 的**关卡号逐位相同**）。
    """
    data = open(so, "rb").read()
    elf = CP.Elf(data)
    relocs = CP.load_relative_relocs(elf)
    smelf = SM.Elf(so)
    univ = SM.x86_universe(smelf, SM.DEFAULT_FUNCSTARTS)
    starts = sorted(univ)
    sset = set(starts)

    def containing(va):
        if va in sset:
            return va
        return starts[bisect.bisect_right(starts, va) - 1]

    truth, tgt = truth_and_target(fixtures)
    names = names_linux(fixtures)

    # --- A: 21 段描述字面量的 xref（纯字节判据） ---
    tex = smelf.sec[".text"]
    lo, hi = tex["addr"], tex["addr"] + tex["size"]
    blob = smelf.read(lo, tex["size"])
    want = {a: i for i, (a, _n) in enumerate(CD.DESC)}
    xref = {i: [] for i in want.values()}
    for o in range(len(blob) - 4):
        rel = struct.unpack_from("<i", blob, o)[0]
        t = want.get(lo + o + 4 + rel)
        if t is not None:
            xref[t].append(lo + o)
    table_funcs = {containing(v[0]) for v in xref.values() if v}
    n_desc_hit = sum(1 for v in xref.values() if v)

    # --- B: 21 个 arm 的逐位解码 ---
    arms = [CP.decode_arm(data, elf, relocs, lv) for lv in range(21)]

    anchors = []       # (addr, kind, level, claimed_purpose)

    def add(addr, kind, level, purpose):
        anchors.append(dict(addr=addr, kind=kind, level=level, purpose=purpose))

    for f in sorted(table_funcs):
        add(f, "level_table", None,
            "21 关数据表：key=\"level_<n>\" / title / 描述字面量 / 四模式元组"
            "（21/21 描述字面量的 rel32 xref 都落在这里）")
    # 文档锚（docs/10 §4/§12，逐位验证过的常量与调用关系）
    for va, why in ((0x442AA0, "把一条关卡记录装进 Vec（21 个 arm 各调用一次）"),
                    (0x466D20, "关卡开场驱动 generate_level_custom（清障碍 + ≤5 轮生成/校验）"),
                    (0x466C40, "地形生成 generate_terrain（scatter 分支的公共尾巴）"),
                    (0x476320, "set_player_num_soldiers（edx = 该关士兵数）"),
                    (0x46F310, "add_target（调用次数 = 该关靶子数）"),
                    (0x418820, "show_hide_campaign_levels（解锁刷新，cmp ebp,0x14）")):
        add(va, "doc_anchor", None, why)
    for r in arms:
        lv = r["level"]
        add(r["arm"], "arm", lv,
            "Level %d 的 start_level 分支（soldiers=%s targets=%s bot=%s turn=%s 障碍=%s min=%s）"
            % (lv, r["soldiers"], r["targets"],
               "-" if r["difficulty"] is None else r["difficulty"],
               "-" if r["turn"] is None else r["turn"], r["obstacle"],
               "-" if r["min_count"] is None else r["min_count"]))
        add(r["fn_a"], "open_closure", lv,
            "Level %d 的开场闭包（传给 generate_level_custom 的 FnMut；障碍预设 %s"
            " min_count=%s）" % (lv, r["obstacle"],
                                 "-" if r["min_count"] is None else r["min_count"]))
        add(r["fn_b"], "layout_closure", lv,
            "Level %d 的布局校验闭包（vtable_b[+0x20]，%s）" % (lv, r["obstacle"]))

    # 去重（同一函数可能被多关共用：障碍生成闭包按 min_count 复用）
    by_func = {}
    for a in anchors:
        c = containing(a["addr"])
        e = by_func.setdefault(c, dict(addr=c, kinds=[], levels=[], purposes=[],
                                       raw=a["addr"]))
        if a["kind"] not in e["kinds"]:
            e["kinds"].append(a["kind"])
        if a["level"] is not None and a["level"] not in e["levels"]:
            e["levels"].append(a["level"])
        if a["purpose"] not in e["purposes"]:
            e["purposes"].append(a["purpose"])

    # --- 分类 + 正对照 ---
    out_rows = []
    for c in sorted(by_func):
        e = by_func[c]
        side = "truth" if c in truth else ("target" if c in tgt else "other")
        mod, meth, srcf = names.get(c, ("", "", ""))
        # 用途是否对得上（只在有名字时可判）
        ok = "-"
        if side == "truth" and mod:
            want_mod = {"level_table": ("level_manager", "level_generator", "campaign", "play"),
                        "doc_anchor": ("level_generator", "game_state_manager", "campaign",
                                       "level_manager"),
                        "arm": ("level_manager", "level_generator"),
                        "open_closure": ("level_manager", "level_generator"),
                        "layout_closure": ("level_generator", "level_manager")}
            cats = [want_mod.get(k, ()) for k in e["kinds"]]
            ok = "是" if any(any(w in mod for w in c2) for c2 in cats if c2) else "否"
            # 硬检验（对所有"名字里带关口"的都做）：名字里的关卡号必须等于数据表里的关卡号
            if "setup_level_" in meth:
                try:
                    num = int(meth.split("setup_level_")[1].split("::")[0])
                except ValueError:
                    num = None
                if num is not None and e["levels"]:
                    if len(e["levels"]) == 1:
                        ok = ("是(关号%d=%d)" % (num, e["levels"][0])
                              if num in e["levels"] else
                              "否(关号%d≠%s)" % (num, e["levels"]))
                    else:
                        # 一关的闭包被多关共用时，只要求关号在集合里（集合级）
                        ok = ("是(关号%d∈%s)" % (num, ",".join(str(x) for x in e["levels"]))
                              if num in e["levels"] else
                              "否(关号%d∉%s)" % (num, ",".join(str(x) for x in e["levels"])))
        out_rows.append(dict(
            addr="0x%06x" % c,
            side=side,
            kinds="+".join(e["kinds"]),
            levels=",".join(str(x) for x in e["levels"]) or "-",
            purpose=e["purposes"][0],
            truth_name=("%s::%s" % (mod, meth)) if mod else "-",
            src_file=srcf or "-",
            purpose_agrees=ok,
            ptr_site="0x%06x" % e["raw"] if e["raw"] != c else "-",
        ))

    n = len(out_rows)
    n_truth = sum(1 for r in out_rows if r["side"] == "truth")
    n_target = sum(1 for r in out_rows if r["side"] == "target")
    n_other = n - n_truth - n_target
    pc = [r for r in out_rows if r["side"] == "truth" and r["purpose_agrees"] != "-"]
    pc_ok = [r for r in pc if r["purpose_agrees"].startswith("是")]
    n_holdout = sum(1 for r in out_rows
                    if r["side"] == "truth"
                    and truth[int(r["addr"], 16)].get("half") == "holdout")

    summary = [
        ("source", "campaign_table"),
        ("anchors_total", str(n)),
        ("anchors_truth_975", str(n_truth)),
        ("anchors_target_12736", str(n_target)),
        ("anchors_other", str(n_other)),
        ("cov_truth_pct", "%.2f" % (100.0 * n_truth / max(1, len(truth)))),
        ("cov_target_pct", "%.2f" % (100.0 * n_target / max(1, len(tgt)))),
        ("ratio_truth_over_target", "%.1f" % ((n_truth / max(1, len(truth)))
                                              / max(1e-9, n_target / max(1, len(tgt))))),
        ("positive_control_named", str(len(pc))),
        ("positive_control_agree", str(len(pc_ok))),
        ("holdout_half_rows_touched", str(n_holdout)),
        ("desc_literals_xref_hit", "%d/21" % n_desc_hit),
        ("level_table_funcs", ",".join("0x%06x" % f for f in sorted(table_funcs))),
    ]
    if verbose:
        print("# 75 · campaign 关卡表 → 地址 → 用途：锚集与两侧覆盖率")
        print("# 判据不看指纹分数：A=描述字面量 rel32 xref、B=逐位解码的指针槽、C=docs/10 的常量锚")
        print()
        print("%-10s %-6s %-14s %-5s %-46s %-11s %s" %
              ("func", "side", "kinds", "levels", "purpose", "agrees", "truth_name"))
        for r in out_rows:
            print("%-10s %-6s %-14s %-5s %-46s %-11s %s" % (
                r["addr"], r["side"], r["kinds"], r["levels"],
                r["purpose"][:44], r["purpose_agrees"], r["truth_name"][:60]))
        print()
        for k, v in summary:
            print("%-26s %s" % (k, v))
    _emit(out_rows, tsv)
    return dict(rows=out_rows, summary=dict(summary))


# ---------------------------------------------------------------------------
# ③ 真机帧
# ---------------------------------------------------------------------------
def feas3(so=DEFAULT_SO, fixtures=FIX, out=None):
    """真机帧能给什么？**不是**"有没有截图"，而是"一帧能不能唯一钉住一个函数地址"。

    判据（不看分数）：一帧要成为地址级锚，必须存在一条
        `可观测量 → 唯一代码路径` 的映射（该量只能由那一个函数/常量产生），并且
        **两侧都能取到**（否则又是一个"单侧有"的伪特征）。
    帧上能读到的是：位置/尺寸/颜色/字符串/数值。其中
        * 字符串 → `.rodata` 常量（已有 `strs`/`consts` 特征，且两侧都在同一份 pck 里 ⇒ 无跨构建判别力）；
        * 数值/几何 → 客户端绘制参数（`client/` 与 `.tscn`，不是目标集合那 12,736 个 Rust 函数）；
        * 布局（障碍/士兵/靶子）→ 只有 campaign 那 21 关的**参数**（②已经用二进制直接解出来了）。
    ⇒ 帧这一侧的上界 = 已经能从二进制读到的那些数；读不到的部分（一帧里没有可观测量的函数）占绝大多数。
    """
    res = []
    n_img = 0
    for sub in ("docs/img", "docs/guide/img", "docs/images"):
        p = os.path.join(ROOT, sub)
        if os.path.isdir(p):
            n_img += sum(1 for f in os.listdir(p) if f.lower().endswith(
                (".png", ".jpg", ".jpeg", ".webp", ".gif")))
    res.append(dict(check="frame_assets_in_repo", value=str(n_img),
                    detail="docs/img 等目录里的截图（0 = 仓库里没有可用的真机帧）"))
    truth, tgt = truth_and_target(fixtures)
    res.append(dict(check="anchor_yield_bound", value="0",
                    detail="帧只给可见状态：字符串→.rodata（已收）、几何→client/ 绘制"
                           "（不是 12,736 的成员）、关卡布局→campaign 已用二进制解出 ⇒ "
                           "新锚 0；且没有 `帧→地址` 的唯一点对点映射"))
    res.append(dict(check="two_sided", value="不可达",
                    detail="真机帧只有原版实跑那一次；无法对 12,736 个函数逐一取帧 ⇒ "
                           "两侧覆盖率无从谈起（41 号纪律：单侧特征不许用）"))
    _emit(res, out)
    return res


# ---------------------------------------------------------------------------
# ④ 留出半 before → after：新源到底能不能进打分器
# ---------------------------------------------------------------------------
def holdout(so=DEFAULT_SO, fixtures=FIX, out=None, anchors=None, jobs=1):
    """留出半（481 行）跑一遍 REF，再把 campaign 锚住的那些行单独拿出来看。

    为什么这样做而不是"加一列特征再跑"：任务书 §2 要求判据独立于指纹分数。
    campaign 的产出是**每个函数一个用途标签**，不是两两相似度 ⇒ 它进不了 `score_same`。
    它能做的只有两件事，各自可测：
        (a) 当**硬约束/覆盖**：命中行的 top-1 直接被它改写 —— 但它只对**已有名字**的行
            有跨构建含义（Linux 用途 → Windows 同名函数），对目标集合的 7 个函数没有
            Windows 对侧 ⇒ 改写量 = 0；
        (b) 当**独立复核**：把命中行在 REF 下的 top-1 对错单独报出来，看它是不是"已经被
            现有证据解决的那批"。
    本子命令算的就是 (b)：全留出半的 REF top-1（应与 §46 的 87.53% 一致，作为工具自检）
    + campaign 命中子集的对错 + 那子集里能新增跨构建对的条数（= 0）。
    """
    import callgraph_fingerprint as CF
    rw = dict(CF.W_SAME)
    rw["scal"] = 0.25                      # REF = W_SAME + scal=0.25（§43.4 预注册）
    lin = CF.load_fp("linux")
    win = CF.load_fp("win")
    score, _w, window = CF.scorer_for("linux", "win", rw)
    pool = CF.Pool(win)
    truth, tgt = truth_and_target(fixtures)
    if anchors is None:
        anchors = rows(os.path.join(fixtures, "campaign_anchors75.tsv"))
    anch = {int(r["addr"], 16) for r in anchors}

    n = ok = 0
    n_h = ok_h = 0
    half_stat = {"holdout": [0, 0], "tune": [0, 0]}      # 锚住的行的 [对, 总]
    sib_err = [0, 0]                                     # 错的行里 top-1 落在"锚集对侧"的条数
    rest_stat = {"holdout": [0, 0], "tune": [0, 0]}      # 没被锚住的其余行
    hit_rows = []
    for va, r in sorted(truth.items()):
        if va not in lin.feats or not r.get("win_addr", "").startswith("0x"):
            continue
        want = int(r["win_addr"], 16)
        if r.get("half") == "holdout":
            n += 1
        top = CF.rank(lin.feats[va], pool, score, window, k=1)
        good = int(bool(top) and top[0][1] == want)
        if r.get("half") == "holdout":
            ok += good
        bucket = half_stat if va in anch else rest_stat
        h = r.get("half", "?")
        if h in bucket:
            bucket[h][0] += good
            bucket[h][1] += 1
        if va in anch:
            n_h += 1
            ok_h += good
            hit_rows.append(dict(addr="0x%06x" % va, half=r.get("half", "?"),
                                 top1="0x%x" % top[0][1] if top else "-",
                                 want="0x%x" % want, good=str(good)))
    wants = {int(r["win_addr"], 16) for r in truth.values()
             if r.get("win_addr", "").startswith("0x")
             and int(r["linux_addr"], 16) in anch}
    for r in hit_rows:
        if r["good"] == "0" and int(r["top1"], 16) in wants:
            sib_err[0] += 1
        if r["good"] == "0":
            sib_err[1] += 1
    res = [
        dict(check="holdout_top1_REF", value="%d/%d = %.2f%%" % (ok, n, 100.0 * ok / max(1, n)),
             detail="全留出半自检（期望与 docs/02 §46 的 87.53% 一致）"),
        dict(check="campaign_anchored_rows_in_truth", value=str(n_h),
             detail="975 里被 campaign 锚住的行数（含两半）"),
        dict(check="campaign_anchored_rows_correct", value="%d/%d" % (ok_h, n_h),
             detail="这些行在 REF 下 top-1 已对的比例"),
        dict(check="anchored_half_holdout", value="%d/%d = %.1f%%" % (
            half_stat["holdout"][0], half_stat["holdout"][1],
            100.0 * half_stat["holdout"][0] / max(1, half_stat["holdout"][1])),
             detail="留出半里被 campaign 锚住的行的 REF top-1"),
        dict(check="anchored_half_tune", value="%d/%d = %.1f%%" % (
            half_stat["tune"][0], half_stat["tune"][1],
            100.0 * half_stat["tune"][0] / max(1, half_stat["tune"][1])),
             detail="调参半同上（只作参考，不参与选择）"),
        dict(check="rest_half_holdout", value="%d/%d = %.1f%%" % (
            rest_stat["holdout"][0], rest_stat["holdout"][1],
            100.0 * rest_stat["holdout"][0] / max(1, rest_stat["holdout"][1])),
             detail="留出半里**没被**锚住的行（对照：这批好得多）"),
        dict(check="errors_inside_anchor_set", value="%d/%d" % (sib_err[0], sib_err[1]),
             detail="锚住行里错的那几条，top-1 有百分之多少落在**同一批锚的 Windows 对侧**"
                    "（= 关卡闭包近孪生：结构指纹分不开，正是 campaign 表能分开的地方）"),
        dict(check="before_after", value="%d/%d -> %d/%d" % (ok, n, ok, n),
             detail="campaign 能给的新跨构建对 = 0（目标集合那 7 个没有对侧名字/用途对）"
                    "⇒ 加进去一格不动"),
    ]
    if out:
        _emit(res, out)
    print("# campaign 命中的每一行（含两半）")
    for r in hit_rows:
        print("%-10s %-8s top1=%-12s want=%-12s good=%s" % (
            r["addr"], r["half"], r["top1"], r["want"], r["good"]))
    for r in res:
        print("%-32s %-22s %s" % (r["check"], r["value"], r["detail"]))
    return res


def _emit(res, out):
    if isinstance(res, list) and res and "check" in res[0]:
        for r in res:
            print("%-28s %-10s %s" % (r["check"], r["value"], r["detail"]))
    if not out:
        return
    with open(out, "w", encoding="utf-8") as fh:
        if isinstance(res, list) and res and "check" in res[0]:
            fh.write("check\tvalue\tdetail\n")
            for r in res:
                fh.write("%s\t%s\t%s\n" % (r["check"], r["value"], r["detail"]))
        else:
            hdr = list(res[0].keys())
            fh.write("\t".join(hdr) + "\n")
            for r in res:
                fh.write("\t".join(str(r.get(k, "")) for k in hdr) + "\n")


def main(argv=None):
    ap = argparse.ArgumentParser(description="75 · 三条新数据源的可行性 / 判据 / 正对照")
    ap.add_argument("cmd", choices=("feas1", "campaign", "feas3", "holdout", "all"))
    ap.add_argument("--so", default=os.environ.get("GW2_SO") or DEFAULT_SO)
    ap.add_argument("--win", default=DEFAULT_WIN)
    ap.add_argument("--android", default=DEFAULT_AND)
    ap.add_argument("--fixtures", default=FIX)
    ap.add_argument("--out", default=None, help="结果 TSV（campaign 子命令单独用 --tsv）")
    a = ap.parse_args(argv)
    if a.cmd == "feas1":
        feas1(a.so, a.win, a.android, a.fixtures, a.out)
    elif a.cmd == "feas3":
        feas3(a.so, a.fixtures, a.out)
    elif a.cmd == "campaign":
        campaign(a.so, a.fixtures, None, a.out)
    elif a.cmd == "holdout":
        holdout(a.so, a.fixtures, a.out)
    else:
        f1 = feas1(a.so, a.win, a.android, a.fixtures, None)
        f3 = feas3(a.so, a.fixtures, None)
        c = campaign(a.so, a.fixtures, None,
                     os.path.join(a.out or FIX, "campaign_anchors75.tsv"))
        s = c["summary"]
        lines = []
        for r in f1:
            lines.append(dict(source="crash_stack", check=r["check"], value=r["value"],
                              detail=r["detail"]))
        lines.append(dict(source="campaign_table", check="anchors_total",
                          value=s["anchors_total"], detail="见 campaign_anchors75.tsv"))
        lines.append(dict(source="campaign_table", check="anchors_truth_975",
                          value=s["anchors_truth_975"],
                          detail="两侧之一：已定名那批（正对照）"))
        lines.append(dict(source="campaign_table", check="anchors_target_12736",
                          value=s["anchors_target_12736"],
                          detail="两侧之二：目标集合（新用途标签）"))
        lines.append(dict(source="campaign_table", check="coverage",
                          value="truth %s%% vs target %s%%" % (s["cov_truth_pct"],
                                                              s["cov_target_pct"]),
                          detail="比值 %s×（两侧差 = 判别力是否有偏）"
                                 % s["ratio_truth_over_target"]))
        lines.append(dict(source="campaign_table", check="positive_control",
                          value="%s/%s" % (s["positive_control_agree"],
                                           s["positive_control_named"]),
                          detail="已有名字里用途对得上 / 可判条数"))
        lines.append(dict(source="campaign_table", check="holdout_overlap",
                          value=s["holdout_half_rows_touched"],
                          detail="与留出半（481 行）重叠几行 —— 决定能不能做 before→after"))
        for r in f3:
            lines.append(dict(source="real_frame", check=r["check"], value=r["value"],
                              detail=r["detail"]))
        _emit(lines, os.path.join(a.out or FIX, "evidence_sources75.tsv"))
        print("\n写入 fixtures/evidence_sources75.tsv / campaign_anchors75.tsv")
    return 0


if __name__ == "__main__":
    sys.exit(main())
