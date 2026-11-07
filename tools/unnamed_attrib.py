#!/usr/bin/env python3
"""给「有 `panic::Location` 但三份符号表都没有名字」的函数找**归属 / 角色**，并给正对照准确率。

## 这个工具解决的问题

Linux 目标里有 **527** 个函数引用了 `src/*.rs` 的 `panic::Location`（这是「函数→源文件」的
绝对天花板，见 `docs/02` §37.0），其中 **348** 个已经由三份夹具钉上名字。剩下的 **179** 个
**没有任何一份符号表**给出名字，但它们的 `Location` 已经告诉了我们**文件**（⇒ 模块）。
本工具回答三件事：

1. **模块**：来自 `Location` 的 `file`（多文件时按锚点数取多数，与 `coverage_matrix` 同规则）；
2. **候选角色**：把**共享同一个 `Location` 键**的有名 Windows 函数当候选池，
   用**结构**（尺寸 / 指令直方图 / 整数常量 / f64 常量 / 字符串前缀 / 被调者数）排序，
   再做「同名键消去」，给出 top 候选 + 分数 + 与次名的差；
3. **正对照**：拿已知名字的 348 个函数**遮住名字**跑同一套规则，按地址 hash 分半
   （调参半 / 留出半），报 top-1 / top-3 准确率。真值优先用 **Windows 地址**（`win_addr`），
   因为名字会撞上两套 `parse_name` 的口径差；名字级口径也打一份，用来展示那个差。
   阈值（size 比窗口 / 结构分下限 / 打平）**全部来自调参半的分布**，不是拍脑袋。

## 决策规则（`decide`，先写死再看结果）

| 桶 | 判据 | 档位 | 给出什么 |
|---|---|---|---|
| **A 个体** | 结构最优有名，且该 Windows 函数只被本函数认领（1:1） | 推断 | 具体函数名 |
| **B 族** | 结构最优有名，但同键下 Linux 侧有 k 个同构函数 | 推断 | 源函数名可信、类型参数未知 |
| **C 族歧义** | 两个**不同名字**同分（差 < `TIE_EPS`） | 推断 | 「这两个之一」 |
| **D 无名字** | 不筛名字时最优的 Windows 函数**本身没有名字** | 未知 | 对应 Windows 无名函数 + 调用邻域 |
| **低分** | 结构相似度 < `MIN_SCORE` | 未知 | — |

## 判据（**不许**是"名字像"）

`AGENTS.md` §0.3 与第五轮 `prefix-fix` 教训都指向同一条：名字相似度不是证据。本工具的
全部判据都是**结构可证**的：

| 代号 | 判据 | 为什么它是结构而不是名字 |
|---|---|---|
| **K** | 两个函数引用**同一个** `panic::Location` 三元组 | 三元组是编译期常量，跨构建/ISA 不变（§20.2） |
| **M** | 候选名属于 `Location.file` 对应的模块 | 模块名由同一份三元组的 `file` 字段给出 |
| **S** | 结构相似度（尺寸比、指令直方图余弦、常量集 Jaccard、字符串前缀集 Jaccard） | 只用字节形态，不读名字 |
| **E** | 消去：某名字已唯一落在**别的** Linux 地址上 ⇒ 在本函数上降权 | 「一对一」约束，与 v1 夹具的 `消去法` 同源 |
| **N** | 调用邻域：调用者/被调者里有已定名的函数 | 调用图是结构 |

候选池只放 **Windows 2.0.60**：它是同版本、同 ISA（x86-64）、带 COFF 名字的构建。
Android 是 arm64，助记符直方图与 x86 不可比，因此**不参与**结构排序（它的名字经
`fixtures/name_to_addr.tsv` 早已并入名字集合，不存在漏用）。

## 用法

    ../.reference/venv/bin/python tools/unnamed_attrib.py build      # 扫两份构建（首次约 55 s）
    ../.reference/venv/bin/python tools/unnamed_attrib.py list       # 179 个 + 模块分布
    ../.reference/venv/bin/python tools/unnamed_attrib.py control    # 正对照（分半 + 分桶 + 消融）
    ../.reference/venv/bin/python tools/unnamed_attrib.py attrib     # 写 fixtures/unnamed_attrib.tsv
    ../.reference/venv/bin/python tools/unnamed_attrib.py probe 0x4b2890

`build` 只要 capstone（走仓库 `.reference/venv`）；其余子命令只读缓存与夹具。
缓存键带 `SCHEMA` 版本：改了特征口径就必须抬版本号，否则会静默吃旧缓存（`sym_match` §36.0 踩过）。
"""

import argparse
import collections
import hashlib
import math
import os
import pickle
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import sym_match as SM                       # noqa: E402  Elf / demangle / name_key
import three_way_match as TWM               # noqa: E402  Pe / universe / Location 桥 / 余弦
import rust_sym                             # noqa: E402  「哪条符号属于 graphwarrust」的唯一判据

SCHEMA = "ua-v2"
SRC_PREFIX = "src/"

DEFAULT_LINUX = TWM.DEFAULT_LINUX
DEFAULT_WIN = TWM.DEFAULT_WIN

FIXTURES = os.path.join(ROOT, "fixtures")
OUT_TSV = os.path.join(FIXTURES, "unnamed_attrib.tsv")

# 结构特征的权重：**在调参半上定**（`control --ablate` 打出消融表），留出半只报准确率。
# 不把「被调者多重集」当主项：TWM §37.6 已量到 Linux/Windows 被调者数之比中位数 0.38
# （LTO/ICF 把子调用内联掉了），不归一化会系统性把正确答案打低分；这里只用条数当弱项，
# 且默认权重 0（消融里可以打开看它到底帮不帮忙）。
WEIGHTS = dict(size=0.5, insn=0.5, hist=1.0, ints=1.0, f64=1.0, f32=0.5, strs=0.5, ncallee=0.0)
SIZE_EPS = 0.4          # log 尺寸比的饱和点（与 TWM 同口径）
MIN_STR = 5             # 字符串前缀最短长度（短于它噪声太大）
STR_PREFIX = 24         # 只取前缀：两版的合并串表邻接不同，长串会污染（sym_match MIN_LIT 同因）
MIN_INT = 0x100         # 与 TWM 的 consts 口径一致（小整数两侧都常见，等于噪声）
ELIM_PENALTY = 0.15     # 消去规则的降权（在调参半上定，见 control --ablate）


def die(msg):
    sys.stderr.write("unnamed_attrib: %s\n" % msg)
    raise SystemExit(2)


def cache_dir():
    d = os.environ.get("UNNAMED_CACHE") or os.path.join(ROOT, ".agent-work/unnamed_attrib")
    os.makedirs(d, exist_ok=True)
    return d


# ===========================================================================
# 1. 特征抽取：函数体里**与名字无关**的东西
# ===========================================================================
def _printable_prefix(raw, limit=STR_PREFIX):
    """从 raw 里取一段可打印 ASCII 前缀（Rust `&str` 不是 NUL 结尾，只能按可打印性截）。"""
    out = []
    for c in raw[:96]:
        if 32 <= c < 127:
            out.append(chr(c))
            if len(out) >= limit:
                break
        else:
            break
    return "".join(out)


def _f64(raw):
    if len(raw) < 8:
        return None
    v = struct.unpack("<d", raw[:8])[0]
    if v != v or v in (float("inf"), float("-inf")) or v == 0.0:
        return None
    if not (1e-9 <= abs(v) <= 1e9):
        return None
    if all(32 <= c < 127 for c in raw[:8]):        # 文本被误读成 double
        return None
    return float("%.6g" % v)


def _f32(raw):
    if len(raw) < 4:
        return None
    v = struct.unpack("<f", raw[:4])[0]
    if v != v or v in (float("inf"), float("-inf")) or v == 0.0:
        return None
    if not (1e-6 <= abs(v) <= 1e6):
        return None
    if all(32 <= c < 127 for c in raw[:4]):
        return None
    return float("%.6g" % v)


def scan_x86(obj, universe, locs, verbose=True):
    """一次线性反汇编，按函数起点归属，抽 9 类特征。

    与 `three_way_match.scan_x86` 的差别只有三处（其余口径**刻意保持一致**，否则两边比的
    不是同一批函数）：多了 **f64/f32 常量池**、**字符串前缀**、**被调者条数**。
    """
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64
    from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_REG_RIP

    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    tex = obj.sec[".text"]
    code = obj.read(tex["addr"], tex["size"])
    starts = sorted(universe)
    hit = TWM.LocHit(locs)
    feats = {}
    for va in universe:
        feats[va] = dict(ninsn=0, calls=[], icalls=0, ints=set(), f64=set(), f32=set(),
                         strs=collections.Counter(), hist=collections.Counter(),
                         locrefs=set(), strrefs=0)
    for insn in cs.disasm(code, tex["addr"]):
        owner = TWM.owner_of(starts, universe, insn.address)
        if owner is None:
            continue
        fe = feats[owner]
        fe["ninsn"] += 1
        fe["hist"][insn.mnemonic] += 1
        end = insn.address + insn.size
        if insn.mnemonic == "call":
            if insn.operands and insn.operands[0].type == X86_OP_IMM:
                fe["calls"].append(insn.operands[0].imm & 0xFFFFFFFFFFFFFFFF)
                continue
            fe["icalls"] += 1
        for op in insn.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                tgt = (end + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
            elif op.type == X86_OP_IMM:
                tgt = op.imm & 0xFFFFFFFFFFFFFFFF
                if -0x80000000 <= op.imm <= 0xFFFFFFFF and abs(op.imm) >= MIN_INT \
                        and obj.sec_at(op.imm) is None:
                    fe["ints"].add(op.imm)
            else:
                continue
            lv = hit.lookup(tgt)
            if lv is not None:
                fe["locrefs"].add(lv)
            sec = obj.sec_at(tgt)
            if sec is None or sec["name"] == ".text":
                continue
            raw = obj.read(tgt, 16)
            if not raw:
                continue
            s = _printable_prefix(raw)
            if len(s) >= MIN_STR:
                fe["strs"][s] += 1
                fe["strrefs"] += 1
            v = _f64(raw)
            if v is not None:
                fe["f64"].add(v)
            v = _f32(raw)
            if v is not None:
                fe["f32"].add(v)
    if verbose:
        sys.stderr.write("[ua] %-24s %6d 函数, %6d 个引用到 Location\n"
                         % (os.path.basename(obj.path), len(universe),
                            sum(1 for f in feats.values() if f["locrefs"])))
    return feats


class Side:
    def __init__(self, tag, kind, path, universe, feats, locs, names):
        self.tag, self.kind, self.path = tag, kind, path
        self.universe, self.feats, self.locs, self.names = universe, feats, locs, names
        self.starts = sorted(universe)
        self.func_locs = {}
        for va in universe:
            self.func_locs[va] = {locs[l] for l in feats[va]["locrefs"] if l in locs}
            f = feats[va]
            f["size"] = universe[va]["end"] - va
            calls = []
            for c in f["calls"]:
                a = TWM.owner_of(self.starts, universe, c)
                if a is not None:
                    calls.append(a)
            f["callees"] = tuple(sorted(calls))

    def state(self):
        return dict(tag=self.tag, kind=self.kind, path=self.path, universe=self.universe,
                    feats=self.feats, locs=self.locs, names=self.names)

    @classmethod
    def from_state(cls, st):
        return cls(st["tag"], st["kind"], st["path"], st["universe"], st["feats"],
                   st["locs"], st["names"])


def side_names(universe):
    """有名函数 → [(name_key, demangled)]。判据只有一份：`rust_sym.is_game_name`。"""
    raw = []
    for va, f in universe.items():
        for n in (f.get("name") or ()):
            if rust_sym.is_game_name(n):
                raw.append((va, n))
    if not raw:
        return {}
    dem = SM.demangle([n for _v, n in raw])
    out = collections.defaultdict(list)
    for (va, _m), d in zip(raw, dem):
        k = SM.name_key(d)
        if k:
            out[va].append((k, d))
    return dict(out)


def load_side(tag, kind, path, funcstarts=None, force=False):
    try:
        size = os.path.getsize(path)
    except OSError as e:
        die("读不到 %s：%s" % (path, e))
    key = "%s-%s-%d-%s.pkl" % (tag, os.path.basename(path).replace(".", "_"), size, SCHEMA)
    cp = os.path.join(cache_dir(), key)
    if os.path.exists(cp) and not force:
        with open(cp, "rb") as f:
            return Side.from_state(pickle.load(f))
    if kind == "pe":
        obj = TWM.Pe(path)
        universe = TWM.pe_universe(obj)
        locs = obj.locations(True)
        names = side_names(universe)
    else:
        obj = SM.Elf(path)
        universe = TWM.elf_universe(obj, funcstarts)
        locs = TWM.elf_locations(obj, True)
        names = side_names(universe) if obj.em == 183 else {}
    feats = scan_x86(obj, universe, locs)
    side = Side(tag, kind, path, universe, feats, locs, names)
    with open(cp, "wb") as f:
        pickle.dump(side.state(), f, protocol=4)
    return side


def load_sides(args, force=False):
    return {
        "linux": load_side("linux", "elf", args.linux, args.funcstarts, force),
        "win": load_side("win", "pe", args.win, None, force),
    }


# ===========================================================================
# 2. 夹具 / 名字集合
# ===========================================================================
def tsv_rows(path):
    hdr, rows = None, []
    if not os.path.exists(path):
        return [], []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            p = line.split("\t")
            if hdr is None:
                hdr = p
                continue
            rows.append(dict(zip(hdr, p)))
    return hdr, rows


def named_map():
    """Linux 地址 → 名字键集合。三份夹具**全取**（口径最宽 = 对"还没名字"最保守）。

    `join_kind` 不筛：`唯一锚` / `集合锚` / `消去法` / `跨版本锚` 都算"已有一个名字"。
    这样算出来的无名集合一定不会把"其实已经有名字"的函数算进去（宁少不多）。
    """
    out = collections.defaultdict(set)
    _h, v1 = tsv_rows(os.path.join(FIXTURES, "name_to_addr.tsv"))
    for r in v1:
        a = r.get("x86_addr", "").strip()
        if a.startswith("0x"):
            out[int(a, 16)].add((r["module"], r["type"], r["method"]))
    _h, v2 = tsv_rows(os.path.join(FIXTURES, "name_to_addr_v2.tsv"))
    for r in v2:
        a = r.get("x86_addr", "").strip()
        if a.startswith("0x"):
            out[int(a, 16)].add((r["module"], r["type"], r["method"]))
    _h, w = tsv_rows(os.path.join(FIXTURES, "windows_name_to_addr.tsv"))
    for r in w:
        if r.get("join_kind") not in ("唯一锚", "集合锚"):
            continue
        a = r.get("linux_addr", "").strip()
        if a.startswith("0x"):
            out[int(a, 16)].add((r["module"], r["type"], r["method"]))
    return dict(out)


def src_locs(side, va):
    return {k for k in side.func_locs.get(va, ()) if k[0].startswith(SRC_PREFIX)}


def module_of(side, va):
    """函数 → 模块（= `Location.file` 的 stem）。

    一个函数可能引用两个模块的 Location（内联），按锚点数取多数、并列取字典序最小 ——
    与 `coverage_matrix.attribute` 的压平规则**同一条**，这样两边的模块分布能对账。
    """
    c = collections.Counter(k[0] for k in src_locs(side, va))
    if not c:
        return None
    top = max(c.values())
    return sorted(k for k, v in c.items() if v == top)[0]


def module_name(path):
    return os.path.splitext(os.path.basename(path or ""))[0]


# ===========================================================================
# 3. 候选生成 + 结构打分
# ===========================================================================
def jaccard(a, b):
    if not a or not b:
        return 0.0
    return len(a & b) / float(len(a | b))


def score_pair(qf, df, w=None):
    """结构相似度（越大越像）。**不读名字、不读地址、不读 Location 键。**"""
    w = w or WEIGHTS
    s = 0.0
    s += w["size"] * (1.0 - min(abs(math.log((df["size"] + 1.0) / (qf["size"] + 1.0))) / SIZE_EPS, 1.0))
    s += w["insn"] * (1.0 - min(abs(math.log((df["ninsn"] + 1.0) / (qf["ninsn"] + 1.0))) / SIZE_EPS, 1.0))
    s += w["hist"] * TWM.cos_hist(qf["hist"], df["hist"])
    if qf["ints"] and df["ints"]:
        s += w["ints"] * jaccard(qf["ints"], df["ints"])
    if qf["f64"] and df["f64"]:
        s += w["f64"] * jaccard(qf["f64"], df["f64"])
    if qf["f32"] and df["f32"]:
        s += w["f32"] * jaccard(qf["f32"], df["f32"])
    if qf["strs"] and df["strs"]:
        s += w["strs"] * jaccard(set(qf["strs"]), set(df["strs"]))
    if w.get("ncallee"):
        s += w["ncallee"] * (1.0 - min(abs(math.log((len(df["callees"]) + 1.0) /
                                                   (len(qf["callees"]) + 1.0))) / SIZE_EPS, 1.0))
    return s


class Matcher:
    def __init__(self, sides, named):
        self.s = sides
        self.lin, self.win = sides["linux"], sides["win"]
        self.named = named
        self.idx = collections.defaultdict(set)
        for va in self.win.universe:
            if va not in self.win.names:
                continue
            for k in src_locs(self.win, va):
                self.idx[k].add(va)
        # 同一张表但**不筛名字**：判断「真对应体是不是无名函数」用（rank_all）
        self.all_idx = collections.defaultdict(set)
        for va in self.win.universe:
            for k in src_locs(self.win, va):
                self.all_idx[k].add(va)
        # 消去用：某名字键已经落在哪些 Linux 地址上
        self.landed_at = collections.defaultdict(set)
        for a, keys in named.items():
            for k in keys:
                self.landed_at[k].add(a)
        self.module_cache = {}

    def mod(self, side, va):
        key = (side.tag, va)
        if key not in self.module_cache:
            self.module_cache[key] = module_name(module_of(side, va))
        return self.module_cache[key]

    def candidates(self, lin_va, module_filter=True):
        S = src_locs(self.lin, lin_va)
        mod = self.mod(self.lin, lin_va)
        out = []
        for k in S:
            for c in self.idx.get(k, ()):
                if module_filter and self.mod(self.win, c) != mod:
                    continue
                out.append(c)
        return S, mod, sorted(set(out))

    def rank(self, lin_va, w=None, module_filter=True):
        S, mod, cands = self.candidates(lin_va, module_filter)
        rows = []
        for c in cands:
            r = self._row(lin_va, c, w)
            if 1.0 / GATE_RATIO <= r["ratio"] <= GATE_RATIO:
                rows.append(r)
        rows.sort(key=lambda r: (-(r["score"] - ELIM_PENALTY * r["elim"]), r["addr"]))
        return S, mod, rows

    def _row(self, lin_va, c, w=None):
        sc = score_pair(self.lin.feats[lin_va], self.win.feats[c], w)
        elim = 0.0
        for k, _d in self.win.names.get(c, ()):
            dest = self.landed_at.get(k, set())
            if dest and lin_va not in dest:
                elim = 1.0
                break
        return dict(addr=c, score=sc, elim=elim, names=self.win.names.get(c, []),
                    size=self.win.feats[c]["size"], locs=src_locs(self.win, c),
                    ratio=(self.win.feats[c]["size"] + 1.0) /
                          (self.lin.feats[lin_va]["size"] + 1.0),
                    hist=TWM.cos_hist(self.lin.feats[lin_va]["hist"],
                                      self.win.feats[c]["hist"]))

    def rank_all(self, lin_va, w=None):
        """**有名 + 无名**的 Windows 函数一起排：用来判断「真对应体是不是压根没名字」。

        这一步是本工具最重要的负结论闸门：若结构上更像的那个 Windows 函数**没有**名字，
        那么"最有名候选"就不是它的真身，只能写 `未知`（§40.5 给了这条闸门的对照）。
        """
        S = src_locs(self.lin, lin_va)
        mod = self.mod(self.lin, lin_va)
        out = []
        for k in S:
            for c in self.all_idx.get(k, ()):
                if self.mod(self.win, c) != mod:
                    continue
                out.append(c)
        rows = [self._row(lin_va, c, w) for c in sorted(set(out))]
        rows = [r for r in rows if 1.0 / GATE_RATIO <= r["ratio"] <= GATE_RATIO]
        rows.sort(key=lambda r: (-(r["score"] - ELIM_PENALTY * r["elim"]), r["addr"]))
        return S, mod, rows


# ===========================================================================
# 4. 正对照
# ===========================================================================
def split_halves(pool):
    """按地址 hash 分半（稳定、可复跑）；调参半定权重/阈值，留出半只报数。"""
    A, B = [], []
    for va in sorted(pool):
        h = int(hashlib.sha1(("%d" % va).encode()).hexdigest()[:8], 16)
        (A if h % 2 == 0 else B).append(va)
    return A, B


def margin_bucket(m):
    for t in ("0.3", "0.2", "0.1", "0.05"):
        if m >= float(t):
            return t
    return "0"


def run_control(m, targets, w=None, module_filter=True):
    stats = collections.Counter()
    rows = []
    for va in targets:
        S, mod, ranked = m.rank(va, w, module_filter)
        if not ranked:
            stats["nocand"] += 1
            rows.append(dict(va=va, n=0, hit=False, margin=0.0, top=None, mod=mod,
                             truth=set(), truth_win=set()))
            continue
        stats["n"] += 1
        truth = set(m.named.get(va, ()))
        top = ranked[0]
        topkeys = {k for k, _d in top["names"]}
        hit = bool(topkeys & truth)
        eff = [r["score"] - ELIM_PENALTY * r["elim"] for r in ranked]
        margin = eff[0] - eff[1] if len(eff) > 1 else 1.0
        k3 = set()
        for r in ranked[:3]:
            k3 |= {k for k, _d in r["names"]}
        stats["top1"] += hit
        stats["top3"] += bool(k3 & truth)
        tw = set(win_truth().get(va, ()))
        addr_hit = top["addr"] in tw
        stats["addr_hit"] += addr_hit
        if not hit:
            stats["fail"] += 1
            stats["fail_but_addr_hit"] += addr_hit
        mb = margin_bucket(margin)
        stats["m_%s" % mb] += 1
        if hit:
            stats["mhit_%s" % mb] += 1
        cb = min(len(ranked), 4)
        stats["c%d" % cb] += 1
        if hit:
            stats["c%dh" % cb] += 1
        rows.append(dict(va=va, n=len(ranked), hit=hit, margin=margin, top=top,
                         truth=truth, mod=mod, truth_win=set(win_truth().get(va, ()))))
    return stats, rows


_WIN_TRUTH = None


def win_truth():
    """Linux 地址 → **已知的 Windows 对应函数地址**集合（正对照的真值，不来自名字）。

    两个来源，都是既有的跨构建结论：
      * `fixtures/windows_name_to_addr.tsv` 的 `func_va`（一行 = 一个 Windows 有名函数）；
      * `fixtures/name_to_addr_v2.tsv` 的 `evidence` 列里 `win_addr=`（三向匹配器写下的地址）。
    用**地址**而不是名字当正对照真值，是为了把「两套 pipeline 的 name_key 解析差异」从准确率里
    摘出去 —— 那属于名字口径问题，不属于匹配器问题（§40.4 把它单列一行）。
    """
    global _WIN_TRUTH
    if _WIN_TRUTH is not None:
        return _WIN_TRUTH
    import re as _re
    out = collections.defaultdict(set)
    _h, wrows = tsv_rows(os.path.join(FIXTURES, "windows_name_to_addr.tsv"))
    for r in wrows:
        for a in (r.get("linux_addr") or "").split(","):
            a = a.strip()
            if not a.startswith("0x"):
                continue
            for fa in (r.get("func_va") or "").split(","):
                fa = fa.strip()
                if fa.startswith("0x"):
                    out[int(a, 16)].add(int(fa, 16))
    _h, v2 = tsv_rows(os.path.join(FIXTURES, "name_to_addr_v2.tsv"))
    for r in v2:
        a = (r.get("x86_addr") or "").strip()
        m = _re.search(r"win_addr=(0x[0-9a-f]+)", r.get("evidence") or "")
        if a.startswith("0x") and m:
            out[int(a, 16)].add(int(m.group(1), 16))
    _WIN_TRUTH = dict(out)
    return _WIN_TRUTH


def run_control_addr(m, targets, w=None, module_filter=True):
    """正对照（地址级）：top-1 的 Windows 函数是不是**已知的那个对应体**。

    这个口径比名字级更严也更干净：它问的是「同一份源码里的同一个函数」，
    完全绕开 v1（`name_bridge`）与 `sym_match.name_key` 两套解析的差异。
    """
    truth = win_truth()
    stats = collections.Counter()
    rows = []
    for va in targets:
        S, mod, ranked = m.rank(va, w, module_filter)
        tru = set(truth.get(va, ()))
        if not ranked:
            stats["nocand"] += 1
            continue
        stats["n"] += 1
        stats["inpool"] += bool(tru & set(m.win.names))
        hit = ranked[0]["addr"] in tru
        stats["top1"] += hit
        stats["top3"] += any(r["addr"] in tru for r in ranked[:3])
        cb = min(len(ranked), 4)
        stats["c%d" % cb] += 1
        if hit:
            stats["c%dh" % cb] += 1
        # 闸门校准：若「不筛名字」时最优的是无名函数，本规则会说"真身无名"
        _S, _m, allrows = m.rank_all(va, w)
        gated = bool(allrows) and not allrows[0]["names"]
        stats["gate_says_unnamed"] += gated
        if gated and hit:
            stats["gate_false_alarm"] += 1
        rows.append(dict(va=va, hit=hit, truth=tru, top=ranked[0], gated=gated))
    return stats, rows


def fmt_control_addr(stats, label):
    n = max(1, stats["n"])
    out = ["=== %s（地址级真值）：目标 %d，有候选 %d，真对应体在候选池内 %d"
           % (label, stats["n"] + stats["nocand"], stats["n"], stats["inpool"])]
    out.append("  top-1 = %d/%d = %.1f%%   top-3 = %d/%d = %.1f%%"
               % (stats["top1"], n, 100.0 * stats["top1"] / n,
                  stats["top3"], n, 100.0 * stats["top3"] / n))
    for c in (1, 2, 3, 4):
        k, h = stats["c%d" % c], stats["c%dh" % c]
        out.append("  候选%-4s n=%3d 命中 %3d 精度 %s"
                   % ("=%d" % c if c < 4 else ">=4", k, h,
                      "%.1f%%" % (100.0 * h / k) if k else "—"))
    out.append("  「真身无名」闸门误报（本该有名却说无名）：%d / %d"
               % (stats["gate_false_alarm"], stats["n"]))
    return "\n".join(out)


def fmt_control(stats, label):
    n = max(1, stats["n"])
    out = []
    out.append("=== %s：目标 %d，有候选 %d ===" % (label, stats["n"] + stats["nocand"], stats["n"]))
    out.append("  top-1 = %d/%d = %.1f%%   top-3 = %d/%d = %.1f%%"
               % (stats["top1"], n, 100.0 * stats["top1"] / n,
                  stats["top3"], n, 100.0 * stats["top3"] / n))
    for t in ("0.05", "0.1", "0.2", "0.3"):
        k, h = stats["m_%s" % t], stats["mhit_%s" % t]
        out.append("  margin>=%-5s n=%3d 命中 %3d 精度 %s"
                   % (t, k, h, "%.1f%%" % (100.0 * h / k) if k else "—"))
    for c in (1, 2, 3, 4):
        k, h = stats["c%d" % c], stats["c%dh" % c]
        lbl = "候选=%d" % c if c < 4 else "候选>=4"
        out.append("  %-8s n=%3d 命中 %3d 精度 %s"
                   % (lbl, k, h, "%.1f%%" % (100.0 * h / k) if k else "—"))
    out.append("  名字级 top-1 的 Windows **地址**对上的：%d/%d（差集 = 两套 name_key parse 不同）"
               % (stats["addr_hit"], n))
    out.append("  名字级失败 %d 个里，%d 个其实命中了正确的 Windows 函数（地址相同、名字键不同）"
               % (stats["fail"], stats["fail_but_addr_hit"]))
    return "\n".join(out)


# ===========================================================================
# 5. 子命令
# ===========================================================================
def base_data(args, force=False):
    sides = load_sides(args, force)
    lin = sides["linux"]
    named = named_map()
    attributed = {va for va in lin.universe if src_locs(lin, va)}
    named_in = {a: ks for a, ks in named.items() if a in attributed}
    unnamed = sorted(attributed - set(named_in))
    return sides, attributed, named_in, unnamed


def cmd_build(args):
    sides = load_sides(args, force=args.force)
    for tag, s in sides.items():
        print("%-6s 函数 %6d  有 src Location %5d  有名函数 %5d"
              % (tag, len(s.universe), sum(1 for v in s.universe if src_locs(s, v)),
                 len(s.names)))
    return 0


def cmd_list(args):
    sides, attributed, named_in, unnamed = base_data(args, args.force)
    lin = sides["linux"]
    print("可归属（引用了 src/*.rs Location）: %d" % len(attributed))
    print("已有名字: %d   无名: %d" % (len(named_in), len(unnamed)))
    dist = collections.Counter(module_name(module_of(lin, v)) for v in unnamed)
    print("\n模块分布（%d 个模块）:" % len(dist))
    for m, c in dist.most_common():
        print("  %4d  %s" % (c, m))
    if args.verbose:
        print("\n地址清单:")
        for v in unnamed:
            print("  %#010x  %-38s size=%-6d locs=%d"
                  % (v, module_name(module_of(lin, v)), lin.feats[v]["size"],
                     len(src_locs(lin, v))))
    return 0


def cmd_control(args):
    sides, attributed, named_in, unnamed = base_data(args, args.force)
    m = Matcher(sides, named_in)
    A, B = split_halves(named_in)
    truth = win_truth()
    addr_targets = [a for a in sorted(named_in) if a in truth]
    AA, BB = split_halves(addr_targets)
    for label, targets in (("调参半", AA), ("留出半", BB)):
        stats, _rows = run_control_addr(m, targets, module_filter=args.module_filter)
        print(fmt_control_addr(stats, label))
        print()
    print("--- 名字级正对照（真值 = 夹具里的 name_key 三元组；含两套 parse 的差异）---")
    for label, targets in (("调参半", A), ("留出半", B)):
        stats, _rows = run_control(m, targets, module_filter=args.module_filter)
        print(fmt_control(stats, label))
        print()
    if args.ablate:
        print("=== 消融（调参半，改一个权重看 top-1）===")
        base = None
        variants = [("基线", dict(WEIGHTS)),
                    ("无 hist", dict(WEIGHTS, hist=0.0)),
                    ("无 f64", dict(WEIGHTS, f64=0.0)),
                    ("无 f32", dict(WEIGHTS, f32=0.0)),
                    ("无 strs", dict(WEIGHTS, strs=0.0)),
                    ("无 ints", dict(WEIGHTS, ints=0.0)),
                    ("只用 size", dict(size=1.0, insn=0.0, hist=0.0, ints=0.0,
                                       f64=0.0, f32=0.0, strs=0.0)),
                    ("加 被调者数", dict(WEIGHTS, ncallee=0.3)),
                    ("无模块过滤", None)]
        for lbl, w in variants:
            if w is None:
                stats, _r = run_control(m, A, module_filter=False)
            else:
                stats, _r = run_control(m, A, w)
            pct = 100.0 * stats["top1"] / max(1, stats["n"])
            if base is None:
                base = pct
            print("  %-12s top-1 = %3d/%3d = %.1f%%  (Δ %+.1f)"
                  % (lbl, stats["top1"], stats["n"], pct, pct - base))
    return 0


def cmd_probe(args):
    sides, attributed, named_in, unnamed = base_data(args, args.force)
    m = Matcher(sides, named_in)
    lin = sides["linux"]
    for a in args.addr:
        va = int(a, 16)
        S, mod, ranked = m.rank(va)
        print("%#x  module=%s  size=%d  locs=%s"
              % (va, mod, lin.feats[va]["size"], [TWM.fmt_key(k) for k in sorted(S)]))
        f = lin.feats[va]
        print("  ints=%s" % sorted(f["ints"]))
        print("  f64=%s" % sorted(f["f64"]))
        print("  f32=%s" % sorted(f["f32"])[:10])
        print("  strs=%s" % [s for s in f["strs"]][:8])
        print("  named=%s" % sorted(named_in.get(va, [])))
        for r in ranked[:5]:
            print("  %#x score=%.3f elim=%.0f size=%d ratio=%.2f names=%s"
                  % (r["addr"], r["score"], r["elim"], r["size"], r["ratio"],
                     ["/".join(x for x in k if x) for k, _d in r["names"]][:3]))
    return 0


# ===========================================================================
# 6. 归属（决策规则 + 证据串）
# ===========================================================================
# 决策规则**先写死再看结果**（防止事后挑阈值）：
#   1. 候选 = 共享 ≥1 个 src Location 键、**同模块**的 Windows 函数；
#   2. 「真身无名闸门」：不筛名字时最优的那个若不是有名函数 ⇒ 真对应体无名字 ⇒ `未知`；
#   3. 闸门通过后，size 比必须落在 [1/6, 6]（结构不能离谱），否则 `未知`；
#   4. 通过 2+3 ⇒ 档位 `推断`，候选角色 = 该 Windows 函数的名字。
GATE_RATIO = 3.0        # 候选的 size 比窗口：正对照里「正确配对」的 p1/p99 是 0.71 / 2.14，
                        # 取 [1/3, 3] 会放过 2/335 个真配对（0.6%），但挡掉尺寸离谱的假配对
TIE_EPS = 0.05          # 两个候选分数差小于它就当「打平」（正对照里这个窗口内没有出现误判）
MIN_SCORE = 1.0         # 结构相似度下限：正对照里正确配对的 score 中位数 2.36、p5 1.41；
                        # 低于 1.0 的有 9/335（2.7%），用来挡「同键但形状不像」的假候选


def short(k):
    """(module, Type, method) → 人读的短名。空段与 `_` 段跳过。"""
    return "::".join(x for x in k if x and x != "_")


def short_names(keys, limit=2):
    """一组 name_key → 人读短名（多名字时用 `+` 连，最多 limit 个）。"""
    return "+".join(short(k) for k in sorted(set(keys))[:limit])


def fnum(va):
    return "%#x" % va


def decide(m, va, named_in, claims=None):
    """一个无名 Linux 函数 → 一行归属结论。返回 dict（字段与夹具列一一对应）。

    决策规则（**先写死，再看结果**；阈值全部来自 `control` 的两个正对照）：

    | 桶 | 判据 | 档位 | 能给出的东西 |
    |---|---|---|---|
    | **A 个体** | 结构最优**有名**，且该 Windows 函数只被本函数认领（1:1） | 推断 | 具体函数名 |
    | **B 族** | 结构最优有名，但同键下 Linux 侧有 k 个同构函数（单态化族） | 推断 | 源函数名；缺的是类型参数 |
    | **C 族歧义** | 结构最优的**不同名字**打平（差 < TIE_EPS） | 推断 | 「这些候选之一」，名字不唯一 |
    | **D 无名字** | 不筛名字时最优的那个 Windows 函数**没有名字** | 未知 | 只能给「对应 Windows 无名函数 + 调用邻域」 |
    """
    lin, win = m.lin, m.win
    S = src_locs(lin, va)
    mod = m.mod(lin, va)
    f = lin.feats[va]
    called_by = sorted({c for c in m.callers.get(va, ()) if c in named_in})
    calls = sorted({c for c in f["callees"] if c in named_in})
    strs = [s for s in f["strs"] if not s.endswith(".rs")][:3]
    row = dict(addr=va, module=mod, size=f["size"], ninsn=f["ninsn"], nloc=len(S),
               anchor=sorted(TWM.fmt_key(k) for k in S)[0] if S else "",
               tier="未知", bucket="", cand="", cand_addr=0, cand_module="",
               evidence="", gated="",
               callers=";".join(short_names(named_in[c]) for c in called_by[:2]),
               callees=";".join(short_names(named_in[c]) for c in calls[:2]),
               strs="|".join(strs))
    if not S:
        row["bucket"] = "无候选"
        row["evidence"] = "无 src Location"
        return row
    _S, _mod, allrows = m.rank_all(va)
    if not allrows:
        row["bucket"] = "无候选"
        row["evidence"] = "同模块内没有共享 Location 键的 Windows 函数"
        return row
    eff = sorted(((r["score"] - ELIM_PENALTY * r["elim"], r) for r in allrows),
                 key=lambda t: (-t[0], t[1]["addr"]))
    best_s, best = eff[0]
    tied = [r for s, r in eff if best_s - s < TIE_EPS]
    tied_names = {k for r in tied for k, _d in r["names"]}
    margin = (eff[0][0] - eff[1][0]) if len(eff) > 1 else 1.0
    row["gated"] = "是" if not best["names"] else "否"
    # ---- 证据串（结构 + 键 + 邻域）----
    parts = ["score=%.2f" % best_s,
             "size %d/%d=%.2f" % (f["size"], best["size"], best["ratio"]),
             "hist=%.2f" % best["hist"],
             "共享键 %d/%d%s" % (len(S & best["locs"]), len(S),
                                "（集合相等）" if S == best["locs"] else "")]
    if len(eff) > 1:
        parts.append("次名差 %.2f" % margin)
    if row["callers"]:
        parts.append("被 %s 调用" % row["callers"])
    if row["callees"]:
        parts.append("调用 %s" % row["callees"])
    if strs:
        parts.append("字符串 %s" % ",".join(strs))
    named_rows = [r for s, r in eff if r["names"]]

    # ---- D：真身无名（闸门）----
    if not best["names"]:
        row["tier"] = "未知"
        row["bucket"] = "D 无名字"
        row["evidence"] = ("结构最优的 Windows 对应体**没有名字**（`%s`，size 比 %.2f，hist=%.2f，"
                           "共享键 %d/%d）；%s"
                           % (fnum(best["addr"]), best["ratio"], best["hist"],
                              len(S & best["locs"]), len(S),
                              "；".join(parts[4:]) or "—"))
        if named_rows:
            nr = named_rows[0]
            row["cand"] = short(nr["names"][0][0])
            row["cand_addr"] = nr["addr"]
            row["cand_module"] = m.mod(win, nr["addr"])
            row["evidence"] += "；最接近的**有名**候选 `%s`（score=%.2f，比无名那个低 %.2f）" % (
                row["cand"], nr["score"] - ELIM_PENALTY * nr["elim"],
                best_s - (nr["score"] - ELIM_PENALTY * nr["elim"]))
        return row
    if not (1.0 / GATE_RATIO <= best["ratio"] <= GATE_RATIO):
        row["tier"] = "未知"
        row["bucket"] = "结构不符"
        row["evidence"] = "结构不符（size 比 %.2f 超出 [1/6,6]）；%s" % (
            best["ratio"], "；".join(parts))
        return row

    # ---- C：不同名字打平 ----
    if len(tied_names) > 1:
        row["tier"] = "推断"
        row["bucket"] = "C 族歧义"
        names = sorted(short(k) for k in tied_names)
        row["cand"] = " 或 ".join(names[:4])
        row["cand_module"] = row["module"]
        row["evidence"] = ("**%d 个候选名同分**（差 < %.2f）：%s；%s"
                           % (len(tied_names), TIE_EPS, "、".join(names[:4]),
                              "；".join(parts)))
        return row

    # ---- 低分：同键但形状不像（正对照里正确配对 score 中位数 2.36）----
    if best_s < MIN_SCORE:
        row["tier"] = "未知"
        row["bucket"] = "低分"
        row["evidence"] = ("结构相似度只有 %.2f（低于下限 %.2f，正对照正确配对中位数 2.36）；"
                           "最接近的有名候选 `%s`（size 比 %.2f）；%s"
                           % (best_s, MIN_SCORE, short(best["names"][0][0]),
                              best["ratio"], "；".join(parts)))
        return row

    # ---- A/B：名字唯一 ----
    k = claims.get(best["addr"], 1) if claims else 1
    row["bucket"] = "A 个体" if k == 1 else "B 族"
    row["tier"] = "推断"
    row["cand"] = short(best["names"][0][0])
    row["cand_addr"] = best["addr"]
    row["cand_module"] = m.mod(win, best["addr"])
    ev = list(parts)
    if k > 1:
        key0 = sorted(S & best["locs"])[0]
        nlin = sum(1 for v in m.lin.universe if key0 in m.lin.func_locs.get(v, ()))
        nwin = len(m.all_idx.get(key0, ()))
        ev.insert(4, "该键下 Linux 侧 %d 个函数（其中 %d 个无名）↔ Windows 侧 %d 个函数 —— "
                     "单态化族，源函数名可信、**类型参数未知**" % (nlin, k, nwin))
    row["evidence"] = "；".join(ev)
    return row


def cmd_attrib(args):
    sides, attributed, named_in, unnamed = base_data(args, args.force)
    m = Matcher(sides, named_in)
    # 调用者反查（Linux 侧直接 call 边）
    callers = collections.defaultdict(set)
    for va in m.lin.universe:
        for c in m.lin.feats[va]["callees"]:
            callers[c].add(va)
    m.callers = callers
    # 第一遍：谁是各无名函数的结构最优（用于 A/B 的 1:1 判定）
    claims = collections.Counter()
    for va in unnamed:
        _S, _m, allrows = m.rank_all(va)
        if allrows:
            claims[allrows[0]["addr"]] += 1
    rows = [decide(m, va, named_in, claims) for va in unnamed]
    dist = collections.Counter(r["tier"] for r in rows)
    buckets = collections.Counter(r["bucket"] for r in rows)
    mods = collections.Counter(r["module"] for r in rows)
    if args.write:
        cols = ["addr", "module", "size", "ninsn", "nloc", "anchor", "tier", "bucket",
                "cand", "cand_addr", "cand_module", "gated", "callers",
                "callees", "strs", "evidence"]
        with open(OUT_TSV, "w", encoding="utf-8") as fh:
            fh.write("# 由 tools/unnamed_attrib.py attrib --write 生成，不要手改（重跑会覆盖）。\n")
            fh.write("# 每个「有 src Location 但三份符号表都没有名字」的函数一行（本版 179 行）。\n")
            fh.write("# linux_addr 是 Linux 2.0.60 的 ELF 虚地址（Ghidra/REA = +0x100000）。\n")
            fh.write("# module 来自 panic::Location 的 file（多文件按锚点数取多数，规则同 coverage_matrix）。\n")
            fh.write("# bucket：A 个体 / B 族 / C 族歧义 / D 无名字 / 结构不符 / 无候选。\n")
            fh.write("# tier：推断 = 同模块共享 Location 键 + 结构最优**有名字**且未打平；未知 = 真身无名/结构不符/无候选。\n")
            fh.write("# cand 是候选名字（module::Type::method）；cand_addr 是它在 Windows 2.0.60 的地址。\n")
            fh.write("# callers/callees 只列**已定名**的调用者/被调者（Linux 侧直接 call 边）。\n")
            fh.write("\t".join(cols) + "\n")
            for r in rows:
                vals = []
                for k in cols:
                    v = r[k]
                    vals.append("%#x" % v if k in ("addr", "cand_addr") and v else str(v))
                fh.write("\t".join(vals) + "\n")
        sys.stderr.write("[ua] 写 %s：%d 行（推断 %d / 未知 %d）\n"
                         % (os.path.relpath(OUT_TSV, ROOT), len(rows),
                            dist["推断"], dist["未知"]))
    print("桶分布：")
    for b, c in buckets.most_common():
        print("  %4d %s" % (c, b))
    print("\n档位：推断 %d / 未知 %d（合计 %d）" % (dist["推断"], dist["未知"], len(rows)))
    print("\n模块分布：")
    for mm, c in mods.most_common():
        print("  %4d %s" % (c, mm))
    if not args.table:
        return 0
    print("\n| linux_addr | 模块（from Location） | 候选角色 | 证据 | 档位 |")
    print("|---|---|---|---|---|")
    for r in rows:
        print("| `%s` | `%s` | %s | %s | %s |"
              % (fnum(r["addr"]), r["module"],
                 ("`%s`" % r["cand"]) if r["cand"] else "—",
                 r["evidence"].replace("|", "/"), r["tier"]))
    return 0



def main(argv=None):
    ap = argparse.ArgumentParser(description="无名函数归属（结构匹配 + 正对照）")
    ap.add_argument("--linux", default=os.environ.get("GW2_LINUX_SO") or DEFAULT_LINUX)
    ap.add_argument("--win", default=os.environ.get("GW2_WIN_DLL") or DEFAULT_WIN)
    ap.add_argument("--funcstarts", default=SM.DEFAULT_FUNCSTARTS)
    ap.add_argument("--force", action="store_true", help="忽略缓存重扫")
    ap.add_argument("--module-filter", action="store_true", default=True)
    ap.add_argument("--no-module-filter", dest="module_filter", action="store_false")
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("build")
    p = sub.add_parser("list")
    p.add_argument("-v", "--verbose", action="store_true")
    p = sub.add_parser("control")
    p.add_argument("--ablate", action="store_true")
    p = sub.add_parser("attrib")
    p.add_argument("--write", action="store_true", help="写 fixtures/unnamed_attrib.tsv")
    p.add_argument("--table", action="store_true", help="打印 docs/02 §40 的 markdown 表")
    p = sub.add_parser("probe")
    p.add_argument("addr", nargs="+")
    args = ap.parse_args(argv)
    if not args.funcstarts or not os.path.exists(args.funcstarts):
        die("找不到 funcstarts（用 --funcstarts 指定 funcstarts_ghidra_va.pkl）")
    cmd = args.cmd or "list"
    return {"build": cmd_build, "list": cmd_list, "control": cmd_control,
            "attrib": cmd_attrib, "probe": cmd_probe}.get(cmd, cmd_list)(args)


if __name__ == "__main__":
    sys.exit(main())
