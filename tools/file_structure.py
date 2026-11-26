#!/usr/bin/env python3
"""T34：用 `panic::Location` 的**行号**反推原版每个文件**内部**的函数顺序与"疑似缺口"。

判据全部预注册在 `fixtures/file_structure_plan.txt`（**先写死再跑**）。本脚本只实现那份判据。

## 这条证据为什么是新的

`panic::Location` 一直只被当**文件级**证据用（"这个函数属于哪个模块"，见
`tools/boundary_audit.py`）。但 Location 的结构体是 `{file: &'static str, line: u32, col: u32}`
—— **line 是源码行号**。同一文件里两个函数的行号一比，就多出一条信息：
**它们在原版文件里的先后**；相邻锚点之间行号跨度异常大，就说明**那一段里还有我们没归属的函数**。

## 用法

    .reference/venv/bin/python tools/file_structure.py control-a   # 正对照 A：两个已知函数 + LevelRules
    .reference/venv/bin/python tools/file_structure.py control-b   # 正对照 B：拿我方自己的二进制做闭环（决定性）
    .reference/venv/bin/python tools/file_structure.py build       # 先跑两级正对照，再生成两张 tsv（正对照失败则**不写**）
    .reference/venv/bin/python tools/file_structure.py check       # 重算并与已入库 tsv 逐字节比对
    .reference/venv/bin/python tools/file_structure.py both        # 只跑两级正对照

原版侧只读 `.so`（`.rela.dyn` + `.data.rel.ro` + `.eh_frame`，经 `tools/loc_xrefs.py`），
**不读** `analysis/*.pkl`；我方侧只读 `src/**/*.rs` 文本 + `docs/coverage-matrix.tsv`。

## 口径（与 `fixtures/file_structure_plan.txt` 一一对应）

* 锚 = 一条 `(Location 槽 → 引用指令)`；三元组 `(文件,行,列)` + `ix.func_of(指令)`。
* 只算 `原版文件 == src/<name>.rs`（原版自己的 crate）；第三方 crate / 子目录路径排除（plan §1.3）。
* `main_file(F)` = F 引用到的 `src/*.rs` 里行数最多者（并列取字典序），与
  `tools/coverage_matrix.py::attribute` 同规则（plan §1.4）。
* `lo=min L`、`hi=max L`、`n=|L|`；`位置序` 按 `lo` 升序（plan §2）。
* `τ_eff = max(200, ceil(p99(我方真实相邻 blank)))`（plan §4.1，由 `control-b` 标定）。
* **行的单位 = 同锚组，不是 funcstart**（plan §9.3，这条是跑完才发现要改的）：
  同一 `(文件, 行集合)` 的多个 funcstart **可能**是①同一泛型函数的多个单态化、②`macro_rules!`
  展开出的**多个不同**函数、③多个函数共享同一个内联 panic 点（`lock().unwrap()` 一类）。
  三者 `Location` 完全一样、**行号分辨不了** ⇒ 一律归一组、`定序=否`、**不合并计数**，
  组内 ≥3 个 funcstart 的登记成 G4。

## 匹配档（plan §3.1）

| 档 | 判据 |
|---|---|
| **S** | v2 三向真名 `Type::method` 在我方该模块的文件里**唯一**命中，且类型名也对上 |
| **W** | v2 真名，但只按 `method` 名在该模块文件里唯一命中（我方类型名与原版不同，如 `TeamTable` vs `GraphwarTeamManager`），或名字带 `::{{closure}}` 后缀 |
| **A** | 我方 `src/*.rs` 注释里**自己点名**的原版地址（A1 紧邻 doc 块恰好一个 `0x…`；A2 表行）。**线索档**：与 Location 独立，可当匹配器但只进"扩展口径"（plan §9.2） |
| **X** | 只能在我方**别的**文件里唯一命中（函数被搬到别的文件）——出行但不进顺序统计 |
| **hyb** | 只有 `fixtures/hybrid_names.tsv` 的候选名 —— **线索**，不进主统计（它的成分含顺序轴对齐，拿来算顺序率是循环论证） |
| **—** | 无 |
"""

import argparse
import bisect
import collections
import glob
import hashlib
import math
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import loc_xrefs as LX                                   # noqa: E402

TSV_PATH = os.path.join(ROOT, "fixtures", "file_structure.tsv")
GAPS_PATH = os.path.join(ROOT, "fixtures", "file_structure_gaps.tsv")
PLAN_PATH = os.path.join(ROOT, "fixtures", "file_structure_plan.txt")
MATRIX = os.path.join(ROOT, "docs", "coverage-matrix.tsv")
V2 = os.path.join(ROOT, "fixtures", "name_to_addr_v2.tsv")
HYB = os.path.join(ROOT, "fixtures", "hybrid_names.tsv")
OURS_SO = os.path.join(ROOT, ".cargo-target", "debug", "libgw2_rebuild.so")

TOOL_VERSION = "1"

# plan §4.1 预注册阈值（先写死；control-b 给出 τ_eff 的标定）
TAU_BASE = 200
TOP_N = 20
GAME_SRC = re.compile(r"src/[A-Za-z0-9_]+\.rs")


# ---------------------------------------------------------------------------
# 0. 小工具
# ---------------------------------------------------------------------------
def read_rows(path):
    rows = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("#") or not line.strip():
                continue
            rows.append(line.rstrip("\n").split("\t"))
    return rows


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def pick_main(bymod):
    """锚点行数最多者胜，并列取模块名字典序 —— 与 coverage_matrix.attribute 完全同规则。"""
    return min(bymod, key=lambda m: (-len(bymod[m]), m))


# ---------------------------------------------------------------------------
# 1. 原版侧：锚点 → {函数: {(文件,行)}}
# ---------------------------------------------------------------------------
def collect_anchors(ix, verbose=True):
    """返回 (per_func, line_funcs, dropped, slots_by_file)。

    per_func   = {func_start: {(file, line)}}
    line_funcs = {(file, line): {func_start, ...}}   —— G3「共享锚」用
    dropped    = 被 §1.3 口径排除掉的 Location 槽数（第三方 crate）
    slots_by_file = {原版文件: Location 槽数} —— plan §6「按锚点数取前 20」用的就是这个
    """
    per_func = collections.defaultdict(set)
    line_funcs = collections.defaultdict(set)
    slots = collections.Counter()
    dropped = 0
    for slot, (f, line, _col) in ix.locs.items():
        if not GAME_SRC.fullmatch(f):
            dropped += 1
            continue
        slots[f] += 1
        for fn in ix.funcs_of_loc(slot):
            per_func[fn].add((f, line))
            line_funcs[(f, line)].add(fn)
    if verbose:
        print("[file_structure] Location 槽 %d，其中非原版 crate 路径 %d 被排除（plan §1.3）⇒ 保留 %d"
              % (len(ix.locs), dropped, len(ix.locs) - dropped))
    return dict(per_func), dict(line_funcs), dropped, dict(slots)


def group_by_module(per_func):
    """{原版文件: {func: {line,...}}}（按 main_file 规则；每个函数只进一个文件）。"""
    out = collections.defaultdict(dict)
    for fn, flc in per_func.items():
        bymod = collections.defaultdict(set)
        for f, line in flc:
            bymod[f].add(line)
        main = pick_main(bymod)
        out[main][fn] = bymod[main]
    return dict(out)


def module_clusters(funcs):
    """按**锚点行集合**把 funcstart 聚类 ⇒ 一行 = 一个「原版源码函数位点」。

    判据（plan §9 事后修正）：同一 `(文件, 行集合)` 的多个 funcstart 是同一个源码函数的
    多个实例化（泛型单态化 / 宏展开 / 同一函数的 `.llvm.` 副本），它们共享 Location 行号。
    聚类后：`insts` = 实例数，`addr` = 代表地址（最小），`lo/hi/n` 来自行集合。
    """
    groups = collections.defaultdict(list)
    for fn, lines in funcs.items():
        groups[frozenset(lines)].append(fn)
    rows = []
    for lineset, addrs in groups.items():
        addrs.sort()
        rows.append(dict(addrs=addrs, addr=addrs[0], insts=len(addrs),
                         lines=sorted(lineset), lo=min(lineset), hi=max(lineset), n=len(lineset)))
    rows.sort(key=lambda r: (r["lo"], -r["n"], r["addr"]))
    for i, r in enumerate(rows, 1):
        r["pos"] = i
    return rows


# ---------------------------------------------------------------------------
# 2. 我方侧：src/**/*.rs 的函数（限定名 + 起始行）
# ---------------------------------------------------------------------------
FN_RE = re.compile(r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:default[ \t]+)?(?:const[ \t]+)?"
                   r"(?:async[ \t]+)?(?:unsafe[ \t]+)?(?:extern[ \t]+\"[^\"]*\"[ \t]+)?"
                   r"fn[ \t]+([A-Za-z0-9_]+)")
IMPL_RE = re.compile(r"^[ \t]*(?:unsafe[ \t]+)?impl(?:<[^>]*>)?[ \t]+(.*)$")
MOD_RE = re.compile(r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?mod[ \t]+([A-Za-z0-9_]+)")
TRAIT_RE = re.compile(r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:unsafe[ \t]+)?trait[ \t]+([A-Za-z0-9_]+)")


def _impl_target(rest):
    """`Foo for Bar where ...` / `Foo<T> {` / `crate::x::Foo` -> 类型名。"""
    rest = rest.split(" where ")[0]
    if " for " in rest:
        rest = rest.split(" for ", 1)[1]
    rest = rest.strip().rstrip("{").strip()
    rest = re.sub(r"<.*", "", rest).strip()
    return rest


def parse_rust_fns(path):
    """返回 [(起始行, method, 限定名)]；跳过 `mod tests` 等内部模块里的 fn。"""
    out = []
    try:
        lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
    except OSError:
        return out
    depth = 0
    stack = []            # [(入口 depth, kind, label)]
    pending = None
    for i, raw in enumerate(lines, 1):
        s = raw
        if "//" in s and '"' not in s.split("//", 1)[0]:      # 去行尾注释（保守）
            s = s.split("//", 1)[0]
        if pending is not None and "{" in s:
            stack.append((depth, pending[0], pending[1]))
            pending = None
        m_mod = MOD_RE.match(s)
        m_impl = IMPL_RE.match(s)
        m_trait = TRAIT_RE.match(s)
        if m_mod:
            if "{" in s:
                stack.append((depth, "mod", m_mod.group(1)))
            else:
                pending = ("mod", m_mod.group(1))
        elif m_impl and not s.lstrip().startswith("impl<"):
            lab = _impl_target(m_impl.group(1))
            if "{" in s:
                stack.append((depth, "impl", lab))
            else:
                pending = ("impl", lab)
        elif m_trait:
            if "{" in s:
                stack.append((depth, "trait", m_trait.group(1)))
            else:
                pending = ("trait", m_trait.group(1))
        mf = FN_RE.match(s)
        if mf:
            if "mod" not in [k for _d, k, _l in stack]:
                impls = [l for _d, k, l in stack if k == "impl"]
                qual = "%s::%s" % (impls[-1], mf.group(1)) if impls else mf.group(1)
                out.append((i, mf.group(1), qual))
        depth += s.count("{") - s.count("}")
        while stack and depth <= stack[-1][0]:
            stack.pop()
    return out


def load_ours_fns():
    """({限定名: [(file,line)]}, {method: [(file,line,qual)]}, {file: [(line,method,qual)]})"""
    by_qual = collections.defaultdict(list)
    by_method = collections.defaultdict(list)
    files = {}
    for path in sorted(glob.glob(os.path.join(ROOT, "src", "**", "*.rs"), recursive=True)):
        rel = os.path.relpath(path, ROOT)
        fns = parse_rust_fns(path)
        files[rel] = fns
        for line, meth, qual in fns:
            by_qual[qual].append((rel, line))
            by_method[meth].append((rel, line, qual))
    return dict(by_qual), dict(by_method), files


# ---------------------------------------------------------------------------
# 3. 名字证据
# ---------------------------------------------------------------------------
def load_v2():
    """{addr: (module, type, method, join_kind)}"""
    out = {}
    rows = read_rows(V2)
    hdr = rows[0]
    iadr, imod, ityp, imet, ijoin = (hdr.index(c) for c in
                                     ("x86_addr", "module", "type", "method", "join_kind"))
    for r in rows[1:]:
        try:
            addr = int(r[iadr], 16)
        except ValueError:
            continue
        if r[ijoin] == "歧义":
            continue
        out[addr] = (r[imod], r[ityp], r[imet], r[ijoin])
    return out


def load_hyb():
    """{addr: (name, confidence)}"""
    out = {}
    if not os.path.exists(HYB):
        return out
    rows = read_rows(HYB)
    hdr = rows[0]
    iadr, iname, iconf = (hdr.index(c) for c in ("x86_addr", "name", "confidence"))
    for r in rows[1:]:
        try:
            addr = int(r[iadr], 16)
        except ValueError:
            continue
        if r[iname]:
            out[addr] = (r[iname], r[iconf])
    return out


def load_matrix():
    """{module: [ours_file...]}"""
    out = {}
    if not os.path.exists(MATRIX):
        return out
    rows = read_rows(MATRIX)
    hdr = rows[0]
    imod, iours = hdr.index("module"), hdr.index("ours_file")
    for r in rows[1:]:
        out[r[imod]] = [x for x in r[iours].split("+") if x and x != "—"]
    return out


def resolve_closure(meth):
    """`parse::{{closure}}` -> ('parse', True)；`_::__init::__inner_init` -> ('_', True)"""
    if "::{{closure}}" in meth or meth.endswith("::__inner_init"):
        return meth.split("::", 1)[0], True
    return meth, False


ADDR_RE = re.compile(r"0x([0-9a-f]{6,7})\b")
# 我方文档里的表行：`| \`add_to_team\` | \`0x3c17d0\` | …`
TABLE_ROW_RE = re.compile(r"\|\s*`?([A-Za-z0-9_]+)`?\s*\|[^|]*?`0x([0-9a-f]{6,7})`")
COMMENT_RE = re.compile(r"^\s*(///|//!|//)")


def load_annotations(our_fns_by_file):
    """我方源码注释里**自己点名**的原版地址 → 我方函数（plan §9 追加档 A）。

    两形态（都要求注释里给出的地址**恰好一个**，避免拿一段散文里的地址硬配）：
      A1 紧邻 doc 块：`fn` 定义行之前连续注释块（≤30 行）里恰好一个 `0x…`；
      A2 表行：`| \\`name\\` | \\`0xADDR\\` |`，`name` 在**同一文件**里唯一命中我方 fn。
    ⚠️ 这是**线索档**（我方文档可能过期），与 Location 行号证据相互独立 ⇒ 可以当匹配器，
       但不能当"顺序"的证据；报告里与 S/W 分开统计。
    """
    out = collections.defaultdict(list)
    for path, fns in our_fns_by_file.items():
        if not path.startswith("src/") or "/" in path[len("src/"):]:
            continue                                    # 只看顶层 src/*.rs
        lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
        for ln in lines:
            if not COMMENT_RE.match(ln):
                continue
            for m in TABLE_ROW_RE.finditer(ln):
                name, addr = m.group(1), int(m.group(2), 16)
                cands = [(l, q) for (l, me, q) in fns if me == name]
                if len(cands) == 1:
                    out[addr].append((path, cands[0][0], cands[0][1], "A2"))
        for (l, _me, q) in fns:
            j = l - 2
            block = []
            while j >= 0 and COMMENT_RE.match(lines[j]) and len(block) < 30:
                block.append(lines[j])
                j -= 1
            if not block:
                continue
            addrs = {int(x, 16) for x in ADDR_RE.findall("\n".join(block))}
            if len(addrs) == 1:
                out[addrs.pop()].append((path, l, q, "A1"))
    # 去歧义：同一个地址若指向多个 (file,line)，整条丢弃
    return {a: v[0] for a, v in out.items() if len({(x[0], x[1]) for x in v}) == 1}


def match_ours(addr, v2, ours_by_qual, ours_by_method, module_files, all_files_set, annot):
    """原版函数 → 我方函数。返回 (档, 我方限定名, 我方文件, 我方行)。

    档：S（v2 真名，类型+方法唯一）/ W（v2 真名，只按方法名唯一）/ X（v2 真名，在别的文件）/
        A（我方源码注解）/ — 。
    """
    if addr in v2:
        _mod, typ, meth, _join = v2[addr]
        base, is_closure = resolve_closure(meth)
        # ① 限定名唯一 + 落在该模块的我方文件里 ⇒ S
        if typ:
            cands = [c for c in ours_by_qual.get("%s::%s" % (typ, base), []) if c[0] in module_files]
            if len(cands) == 1:
                return ("W" if is_closure else "S", "%s::%s" % (typ, base), cands[0][0], cands[0][1])
        # ② 只按方法名（限该模块文件）⇒ W
        cands = [c for c in ours_by_method.get(base, []) if c[0] in module_files]
        if len(cands) == 1:
            return ("W", cands[0][2], cands[0][0], cands[0][1])
        # ③ 全仓唯一 ⇒ X（函数被我方搬到了别的文件）
        cands = ours_by_method.get(base, [])
        if len(cands) == 1 and cands[0][0] in all_files_set:
            return ("X", cands[0][2], cands[0][0], cands[0][1])
    # ④ A 档：我方源码注释里自己点名的地址
    if annot and addr in annot:
        f, line, qual, _kind = annot[addr]
        return ("A", qual, f, line)
    return ("—", "", "", 0)


def match_hyb(addr, hyb, ours_by_qual, ours_by_method, module_files):
    if addr not in hyb:
        return None
    name, conf = hyb[addr]
    parts = name.split("::")
    typ = parts[-2] if len(parts) >= 2 else ""
    base, _ = resolve_closure(parts[-1])
    if typ:
        cands = [c for c in ours_by_qual.get("%s::%s" % (typ, base), []) if c[0] in module_files]
        if len(cands) == 1:
            return ("hyb", "%s::%s" % (typ, base), cands[0][0], cands[0][1], conf)
    cands = [c for c in ours_by_method.get(base, []) if c[0] in module_files]
    if len(cands) == 1:
        return ("hyb", cands[0][2], cands[0][0], cands[0][1], conf)
    return None


# ---------------------------------------------------------------------------
# 4. 骨架 / 缺口 / 一致率（plan §2–§4 的实现）
# ---------------------------------------------------------------------------
def hyb_module_index(hyb, per_func):
    """{原版文件: [未锚定的 hyb 候选 addr,...]} —— 「可能缺的函数」线索（不当结论）。"""
    out = collections.defaultdict(list)
    for addr, (name, _conf) in hyb.items():
        mod = name.split("::", 1)[0]
        if not mod.endswith(".rs"):
            mod += ".rs"
        key = "src/" + mod if not mod.startswith("src/") else mod
        if addr in per_func:
            continue
        out[key].append(addr)
    for k in out:
        out[k].sort()
    return dict(out)


def build_skeleton(per_func, line_funcs, slots_by_file, v2, hyb, matrix, ours_by_qual,
                   ours_by_method, all_files_set, tau_eff, annot=None,
                   tau_sens=(100, 200, 400)):
    """产出 (rows, gaps, stats, by_module, hyb_idx, sens)。

    ⚠️ 行单位 = **同锚组**（plan §9 事后修正）：一组 = 同一 `(文件, 行集合)` 的 funcstart 集合。
       组内 funcstart 数 > 1 时**不能**当成"同一个源码函数的多个实例化"——宏展开、泛型单态化、
       共享内联位点三种都会给出相同行集合，Location 无法区分 ⇒ 该组标「同锚组 N」，
       `lo` 仍可当"这一组锚点所在的行"，但**不用于顺序统计**（`定序=否`）。
    """
    by_module = group_by_module(per_func)
    hyb_idx = hyb_module_index(hyb, per_func)
    sens = {t: 0 for t in tau_sens}

    def anchor_count(mod):
        """该文件里**被引用的不同行号数**（同一行被多个 funcstart 引用只算一行）。"""
        return len({ln for lines in by_module[mod].values() for ln in lines})

    # plan §6：按**该文件的 Location 槽数**降序取前 TOP_N（并列取字典序）
    ranked = sorted(by_module, key=lambda m: (-slots_by_file.get(m, 0), m))
    chosen = ranked[:TOP_N]
    if "src/math_function.rs" not in chosen:
        chosen.append("src/math_function.rs")

    rows, gaps, stats = [], [], []
    rank_tier = {"S": 0, "W": 1, "A": 2, "X": 3, "—": 9}
    for mod in chosen:
        base_mod = mod[len("src/"):]
        module_files = set(matrix.get(base_mod, []))
        groups = module_clusters(by_module[mod])
        for g in groups:
            best = None
            for a in g["addrs"]:                      # 组内多个 funcstart 都试，取最强匹配
                t = match_ours(a, v2, ours_by_qual, ours_by_method, module_files,
                               all_files_set, annot)
                if best is None or rank_tier[t[0]] < rank_tier[best[0]]:
                    best = t
                if best[0] in ("S", "W"):
                    break
            g["tier"], g["our_fn"], g["our_file"], g["our_line"] = best
            g["orderable"] = (g["insts"] == 1)
            if g["tier"] not in ("S", "W", "A"):
                for a in g["addrs"]:
                    h = match_hyb(a, hyb, ours_by_qual, ours_by_method, module_files)
                    if h:
                        g["hyb_fn"], g["hyb_file"], g["hyb_line"], g["hyb_conf"] = \
                            h[1], h[2], h[3], h[4]
                        break
        # ---- G1（无锚空白）/ G2（区间交叠）：按 lo 序的相邻两组 ----
        for i in range(1, len(groups)):
            a, b = groups[i - 1], groups[i]
            if b["lo"] <= a["hi"]:
                gaps.append(dict(mod=mod, kind="G2", a=a, b=b, blank=0))
            else:
                blank = b["lo"] - a["hi"] - 1
                qual = [t for t in sorted(tau_sens) if blank >= t]
                for t in qual:
                    sens[t] += 1
                if blank >= tau_eff:
                    gaps.append(dict(mod=mod, kind="G1", a=a, b=b, blank=blank))
                elif qual:            # plan §4 预注册的灵敏度档：标出来，不进主口径
                    gaps.append(dict(mod=mod, kind="G1(灵敏度 τ=%d)" % max(qual),
                                     a=a, b=b, blank=blank))
        # ---- G3（同一行被 ≥2 个**可定序**位点引用 ⇒ 真内联/共享 panic 点） ----
        orderable_lines = collections.defaultdict(set)
        for i, g in enumerate(groups):
            if g["orderable"]:
                for ln in g["lines"]:
                    orderable_lines[ln].add(i)
        for (f, line), fns in sorted(line_funcs.items()):
            if f != mod or len(fns) < 2:
                continue
            owners = orderable_lines.get(line, set())
            if len(owners) < 2:
                continue
            gaps.append(dict(mod=mod, kind="G3", a=None, b=None, blank=0, line=line,
                             fns=sorted(fns), owners=len(owners)))
        # ---- G4（同锚组 ≥3 个 funcstart：宏展开/泛型/共享位点 ⇒ 我们**分辨不出**这些函数） ----
        for g in groups:
            if g["insts"] >= 3:
                gaps.append(dict(mod=mod, kind="G4", a=g, b=None, blank=g["insts"] - 1))
        # ---- 顺序一致率（plan §3.2）：只用**可定序**组；主口径 S/W，扩展 +A ----
        def order_stats(sel):
            adj = adj_tot = inv = inv_tot = 0
            for i in range(len(sel) - 1):
                a, b = sel[i], sel[i + 1]
                if a["our_line"] == b["our_line"]:
                    continue
                adj_tot += 1
                adj += 1 if a["our_line"] < b["our_line"] else 0
            for i in range(len(sel)):
                for j in range(i + 1, len(sel)):
                    inv_tot += 1
                    inv += 1 if sel[i]["our_line"] > sel[j]["our_line"] else 0
            return adj, adj_tot, inv, inv_tot

        strict = [g for g in groups if g["orderable"] and g["tier"] in ("S", "W")]
        ext = [g for g in groups if g["orderable"] and
               (g["tier"] in ("S", "W") or (g["tier"] == "A" and g["our_file"] in module_files))]
        matched = ext or strict
        adj, adj_tot, inv, inv_tot = order_stats(strict)
        eadj, eadj_tot, einv, einv_tot = order_stats(ext)
        # ---- 每行判定 ----
        g1_starts = {id(g["b"]) for g in gaps if g["kind"] == "G1" and g["mod"] == mod}
        g2_starts = {id(g["b"]) for g in gaps if g["kind"] == "G2" and g["mod"] == mod}
        prev_m = None
        for r in groups:
            if id(r) in g1_starts:
                verdict = "疑似缺口"
            elif id(r) in g2_starts:
                verdict = "交叠(G2)"
            elif not r["orderable"]:
                verdict = "同锚组不可定序"
            elif not module_files:
                verdict = "无我方文件"
            elif r["tier"] == "—":
                verdict = "未匹配"
            elif len(matched) < 2:
                verdict = "孤立锚"
            elif r["tier"] == "X":
                verdict = "他在别文件"
            elif prev_m is None:
                verdict = "顺序一致"
            else:
                verdict = "顺序一致" if prev_m["our_line"] < r["our_line"] else "顺序错"
            if r in matched:
                prev_m = r
            prev = groups[r["pos"] - 2] if r["pos"] > 1 else None
            rows.append(dict(
                file=mod, line=r["lo"], span="%d..%d" % (r["lo"], r["hi"]), n=r["n"],
                insts=r["insts"],
                pair=",".join("0x%06x" % x for x in r["addrs"][:4])
                     + ("…" if len(r["addrs"]) > 4 else ""),
                addr="0x%06x" % r["addr"], pos=r["pos"],
                delta=(r["lo"] - prev["lo"]) if prev else 0,
                blank=("—" if prev is None else max(0, r["lo"] - prev["hi"] - 1)),
                our_fn=r["our_fn"] or (r.get("hyb_fn") or "—"),
                our_loc=(("%s:%d" % (r["our_file"], r["our_line"])) if r["our_file"] else "—"),
                tier=r["tier"], verdict=verdict,
                orderable="是" if r["orderable"] else "否",
                hyb=("hyb:%s %s:%d" % (r.get("hyb_conf", ""), r.get("hyb_file", ""),
                                       r.get("hyb_line", 0))) if r.get("hyb_fn") else "—",
            ))
        stats.append(dict(module=mod, slots=slots_by_file.get(mod, 0),
                          anchors=anchor_count(mod), funcs=len(groups),
                          funcstarts=sum(r["insts"] for r in groups),
                          orderable=sum(1 for r in groups if r["orderable"]),
                          lo=groups[0]["lo"], hi=groups[-1]["hi"], matched=len(matched),
                          tier_s=sum(1 for r in groups if r["tier"] == "S"),
                          tier_w=sum(1 for r in groups if r["tier"] == "W"),
                          tier_x=sum(1 for r in groups if r["tier"] == "X"),
                          tier_a=sum(1 for r in groups if r["tier"] == "A"),
                          adj=adj, adj_tot=adj_tot, inv=inv, inv_tot=inv_tot,
                          eadj=eadj, eadj_tot=eadj_tot, einv=einv, einv_tot=einv_tot,
                          g1=sum(1 for g in gaps if g["mod"] == mod and g["kind"] == "G1"),
                          g1s=sum(1 for g in gaps if g["mod"] == mod
                                  and g["kind"].startswith("G1(灵敏度")),
                          g2=sum(1 for g in gaps if g["mod"] == mod and g["kind"] == "G2"),
                          g3=sum(1 for g in gaps if g["mod"] == mod and g["kind"] == "G3"),
                          g4=sum(1 for g in gaps if g["mod"] == mod and g["kind"] == "G4"),
                          hyb_unmatched=len(hyb_idx.get(mod, []))))
    return rows, gaps, stats, by_module, hyb_idx, sens


# ---------------------------------------------------------------------------
# 5. 正对照 A：两个已知函数 + LevelRules（plan §5.1）
# ---------------------------------------------------------------------------
CONTROL_A = [(0x4ED670, "src/math_function.rs", "解析器"),
             (0x4EB9D0, "src/math_function.rs", "求值器")]
LEVELRULES = [(0x473520, "src/graphwar_game_state_manager.rs", 5),
              (0x4759F0, "src/graphwar_game_state_manager.rs", 12),
              (0x4780C0, "src/graphwar_game_state_manager.rs", 5)]


def cmd_control_a(ix):
    per_func, _lf, _d, _sl = collect_anchors(ix, verbose=False)
    problems = []
    print("\n=== 正对照 A（plan §5.1）：两个已知函数 + LevelRules ===")
    for addr, want, label in CONTROL_A:
        flc = per_func.get(addr)
        got = sorted({f for f, _l in flc}) if flc else []
        ok = got == [want]
        print("  %s %s %-6s -> %s（期望 %s）"
              % ("✓" if ok else "✗", "0x%06x" % addr, label, ",".join(got) or "无锚点", want))
        if not ok:
            problems.append("0x%x %s 落 %r，期望 %s" % (addr, label, got, want))
    for addr, want, want_anchor in LEVELRULES:
        flc = per_func.get(addr) or set()
        bymod = collections.defaultdict(set)
        for f, line in flc:
            bymod[f].add(line)
        got = pick_main(bymod) if bymod else ""
        n = len(bymod.get(got, ()))
        ok = got == want and n == want_anchor
        print("  %s 0x%06x LevelRules -> %-38s 锚点行 %d（期望 %s / %d）"
              % ("✓" if ok else "✗", addr, got or "无", n, want, want_anchor))
        if not ok:
            problems.append("LevelRules 0x%x 落 %r/%d" % (addr, got, n))
    # 顺序轴：原版 lo 的先后 vs 我方文件里的先后（两个已知函数只有 1 个锚点，只能比先后）
    lo = {}
    for addr, want, _label in CONTROL_A:
        lines = sorted(l for f, l in (per_func.get(addr) or set()) if f == want)
        lo[addr] = lines[0] if lines else None
    parser_lo, eval_lo = lo.get(0x4ED670), lo.get(0x4EB9D0)
    _by_qual, _by_method, files = load_ours_fns()
    firsts = {}
    for line, meth, _q in files.get("src/math_function.rs", []):
        firsts.setdefault(meth, line)
    parser_side = [firsts[m] for m in ("parse", "lex", "expr") if m in firsts]
    eval_side = [firsts[m] for m in ("eval", "ev") if m in firsts]
    print("  原版 lo：解析器=%s 求值器=%s；我方 src/math_function.rs：解析器侧=%s 求值器侧=%s"
          % (parser_lo, eval_lo, parser_side, eval_side))
    if parser_lo is not None and eval_lo is not None and parser_side and eval_side:
        orig_first = "解析器" if parser_lo < eval_lo else "求值器"
        ours_first = "解析器" if min(parser_side) < min(eval_side) else "求值器"
        ok = orig_first == ours_first
        print("  %s 原版先后（%s）== 我方先后（%s）" % ("✓" if ok else "✗", orig_first, ours_first))
        if not ok:
            problems.append("原版先后 %s != 我方 %s" % (orig_first, ours_first))
    return problems


# ---------------------------------------------------------------------------
# 6. 正对照 B：我方自己的二进制做闭环（plan §5.2，决定性）
# ---------------------------------------------------------------------------
def demangle_all(names):
    """Rust legacy 名（`_ZN...E`）用 c++filt 批量解。"""
    if not names:
        return {}
    try:
        out = subprocess.run(["c++filt"], input="\n".join(names), capture_output=True,
                             text=True, timeout=300)
        dem = out.stdout.splitlines()
    except Exception:                                       # noqa: BLE001
        dem = []
    if len(dem) != len(names):
        dem = list(names)
    return dict(zip(names, dem))


def our_symbols(so):
    """{addr: demangled} —— `.symtab` 的函数符号（我方 debug 构建未 strip）。"""
    out = subprocess.run(["nm", "--defined-only", "--no-demangle", so],
                         capture_output=True, text=True, timeout=900)
    addrs, syms = [], []
    for line in out.stdout.splitlines():
        p = line.split()
        if len(p) < 3 or p[1].lower() not in ("t", "w"):
            continue
        try:
            a = int(p[0], 16)
        except ValueError:
            continue
        addrs.append(a)
        syms.append(p[2])
    dem = demangle_all(syms)
    return {a: dem.get(s, s) for a, s in zip(addrs, syms)}


def rust_path_to_src(dem):
    """`gw2_rebuild::graphwar_team_manager::TeamTable::add_to_team` -> (file, Type, method)

    要处理的三种 demangle 形态（都是实测见过的）：
      * `gw2_rebuild[a1b2c3]::m::f`            —— crate disambiguator 方括号
      * `<gw2_rebuild[a1b2c3]::m::T>::new`     —— impl 块里的方法
      * `<gw2_rebuild[a1b2c3]::m::T as Trait>::method`
    """
    d = re.sub(r"\[[0-9a-f]+\]", "", dem)                 # crate disambiguator
    if d.startswith("<"):
        try:
            inner, rest = d[1:].split(">::", 1)
        except ValueError:
            return None
        inner = inner.split(" as ", 1)[0]                 # trait impl 形式
        d = inner + "::" + rest
    p = [x for x in d.split("::") if not re.fullmatch(r"h[0-9a-f]{16}", x)]
    if not p or p[0] != "gw2_rebuild" or len(p) < 3:
        return None
    file = "src/%s.rs" % p[1]
    rest = p[2:]
    if len(rest) >= 2:
        return file, rest[-2], rest[-1]
    return file, "", rest[-1]


def cmd_control_b(so):
    """同一套管线跑我方自己的 .so，与真实源码顺序比（ground truth）。"""
    if not os.path.exists(so):
        return None, ["我方二进制不存在：%s（先 cargo build --offline）" % so]
    problems = []
    print("\n=== 正对照 B（plan §5.2）：用我方自己的二进制做闭环 ===")
    print("  目标：%s" % os.path.relpath(so, ROOT))
    print("  sha256=%s" % sha256(so))
    ix = LX.LocXrefs(so, verbose=False)
    per_func, _lf, _d, _sl = collect_anchors(ix)
    syms = our_symbols(so)
    sym_addrs = sorted(syms)
    by_qual, by_method, files = load_ours_fns()

    deltas = []
    name_mismatch = 0
    per_file = collections.defaultdict(list)
    for fn, flc in per_func.items():
        i = bisect.bisect_right(sym_addrs, fn) - 1
        if i < 0:
            continue
        rp = rust_path_to_src(syms[sym_addrs[i]])
        if rp is None:
            continue
        file, typ, meth = rp
        bymod = collections.defaultdict(set)
        for f, line in flc:
            bymod[f].add(line)
        main = pick_main(bymod)
        if main != file:
            name_mismatch += 1
            continue
        lines = bymod[main]
        lo, hi = min(lines), max(lines)
        qual = "%s::%s" % (typ, meth) if typ else meth
        cand = [ln for (f, ln) in by_qual.get(qual, []) if f == file]
        if not cand:
            cand = [ln for (f, ln, _q) in by_method.get(meth, []) if f == file]
        if not cand:
            continue
        src_line = min(cand)
        deltas.append((lo - src_line, file, meth, src_line, lo, hi))
        per_file[file].append((lo, hi, src_line, meth))
    if not deltas:
        return None, ["正对照 B：没有任何函数能把 Location ↔ 我方源码行对上"]
    ds = sorted(d[0] for d in deltas)
    pick = lambda q: ds[min(len(ds) - 1, int(q * len(ds)))]        # noqa: E731
    print("  能对上源码的锚定函数：%d（主文件≠符号模块而丢弃：%d）" % (len(deltas), name_mismatch))
    print("  Δ = lo(二进制) − 源码起始行：中位数 %d / p90 %d / p99 %d / 最大 %d"
          % (pick(0.5), pick(0.90), pick(0.99), ds[-1]))
    # 相邻对顺序一致率
    adj = tot = 0
    for file, items in per_file.items():
        s = sorted(items, key=lambda t: t[0])
        for i in range(len(s) - 1):
            if s[i][2] == s[i + 1][2]:
                continue
            tot += 1
            adj += 1 if s[i][2] < s[i + 1][2] else 0
    rate = 100.0 * adj / tot if tot else 0.0
    print("  相邻对顺序一致率 adj_rate_self = %d/%d = **%.2f%%**" % (adj, tot, rate))
    # τ 标定：我方**真实相邻**（源码里也相邻）函数之间的 blank = lo(B) − hi(A) − 1
    blanks = []
    for file, items in per_file.items():
        s = sorted(items, key=lambda t: t[2])
        for i in range(len(s) - 1):
            blanks.append(s[i + 1][0] - s[i][1] - 1)
    blanks = [b for b in blanks if b >= 0]
    if blanks:
        bs = sorted(blanks)
        p99 = bs[min(len(bs) - 1, int(0.99 * len(bs)))]
        tau_eff = max(TAU_BASE, int(math.ceil(p99)))
        print("  我方真实相邻函数的 blank：中位数 %d / p90 %d / p99 %d / 最大 %d"
              % (bs[len(bs) // 2], bs[int(0.90 * len(bs))], p99, bs[-1]))
        print("  ⇒ τ_eff = max(%d, ceil(p99)) = **%d**（plan §4.1）" % (TAU_BASE, tau_eff))
    else:
        tau_eff = TAU_BASE
    worst = []
    for file, items in per_file.items():
        if len(items) < 2:
            continue
        s = sorted(items, key=lambda t: t[0])
        inv = sum(1 for i in range(len(s)) for j in range(i + 1, len(s)) if s[i][2] > s[j][2])
        n = len(s) * (len(s) - 1) // 2
        worst.append((inv / n if n else 0.0, file, len(s), inv, n))
    worst.sort(reverse=True)
    print("  逆序率最高的 5 个我方文件（k≥2）：")
    for r, file, k, inv, n in worst[:5]:
        print("    %-42s k=%-3d 逆序 %d/%d = %.0f%%" % (file, k, inv, n, 100 * r))
    return dict(rate=rate, adj=adj, tot=tot, med=pick(0.5), p99=pick(0.99), max=ds[-1],
                tau_eff=tau_eff, n=len(deltas)), problems


# ---------------------------------------------------------------------------
# 7. 生成 tsv
# ---------------------------------------------------------------------------
TSV_COLS = ["原版文件", "行号", "我方函数", "我方文件:行", "位置序", "与前一锚行号差", "判定",
            "与前锚无锚空白", "锚点数", "行号跨度", "同锚组funcstart数", "定序", "函数地址(代表+组内)",
            "匹配档", "hyb 候选（线索）"]
GAP_COLS = ["原版文件", "行号区间", "已知两侧函数", "缺口行数", "可能是什么（若能指到地址）",
            "缺什么证据", "形态"]


def render_tsv(rows, stats, ix, tau_eff, ctl_b, dropped, sens=None):
    sens_line = ("—" if not sens else
                 " / ".join("%d:%d" % (t, sens[t]) for t in sorted(sens)))
    head = [
        "# file_structure.tsv —— 由 tools/file_structure.py build 生成，**不要手改**",
        "# 重算：$MAIN/.reference/venv/bin/python tools/file_structure.py build",
        "# 判据预注册：fixtures/file_structure_plan.txt（先写死再跑）；工具版本 " + TOOL_VERSION,
        "# 一行 = 一个被 panic::Location 锚定的原版函数，在**它的主文件**里的位置",
        "# 列：原版文件 | 行号(=lo) | 我方函数 | 我方文件:行 | 位置序 | 与前一锚行号差 | 判定",
        "#     | 与前锚无锚空白 | 锚点数 | 行号跨度(lo..hi) | 同锚组funcstart数 | 定序 | 函数地址(代表+组内)",
        "#     | 匹配档 | hyb 候选（线索）",
        "# ⚠️ 一行 = 一个**同锚组**（plan §9 事后修正）：同一 (文件, 行集合) 的 funcstart 归一组。",
        "#    组内 funcstart 数 >1 时**无法判定**它们是「同一泛型函数的多个单态化」还是「宏展开出的多个函数」",
        "#    还是「多个函数共享同一个内联 panic 点」—— 三者 Location 完全一样 ⇒ 该组 `定序=否`，不进顺序统计。",
        "# 判定 ∈ {顺序一致, 顺序错, 疑似缺口, 疑似缺口, 交叠(G2), 孤立锚, 未匹配, 无我方文件,",
        "#        他在别文件, 同锚组不可定序}（plan §3/§4）",
        "#   疑似缺口 = 与前一个锚之间的无锚行数 ≥ τ_eff；孤立锚 = 该文件 S/W 档 <2，判不出顺序",
        "# 匹配档 ∈ {S,W,A,X,hyb,—}（plan §3.1 + §9 追加档 A）：",
        "#   S/W = v2 三向真名的强/弱匹配（**主统计口径**）；A = 我方源码注释里自己点名的原版地址（线索档，",
        "#   扩展口径）；X = 我方把函数搬到了别的文件（出行、不进顺序统计）；hyb = 只有 hybrid 候选名（不进任何口径）",
        "# 地址口径：ELF vaddr（RVA），不是 Ghidra 的 +0x100000",
        "# 目标 .so sha256=%s" % getattr(ix, "sha256", "?"),
        "# Location 槽 %d / 其中非原版 crate 路径 %d 被排除（plan §1.3）" % (len(ix.locs), dropped),
        "# τ_eff=%d 行（plan §4.1 由 control-b 标定）；G1 灵敏度（τ=100/200/400 下的缺口数）：%s"
        % (tau_eff, sens_line),
        "# 自对照 control-b（我方自己的二进制，plan §5.2）：相邻对一致率 %.2f%%（%d/%d）；"
        "Δ=lo−源码行 中位数 %s / p99 %s / 最大 %s"
        % (ctl_b["rate"], ctl_b["adj"], ctl_b["tot"], ctl_b["med"], ctl_b["p99"], ctl_b["max"])
        if ctl_b else "# 自对照 control-b：未跑（顺序结论降级为线索）",
        "#",
        "# ── 每模块骨架摘要 ──",
    ]
    for s in sorted(stats, key=lambda x: (-x["anchors"], x["module"])):
        adj = "%.0f%%" % (100.0 * s["adj"] / s["adj_tot"]) if s["adj_tot"] else "n/a"
        inv = "%.0f%%" % (100.0 * (1 - s["inv"] / s["inv_tot"])) if s["inv_tot"] else "n/a"
        eadj = "%.0f%%" % (100.0 * s["eadj"] / s["eadj_tot"]) if s["eadj_tot"] else "n/a"
        head.append("#   %-46s 槽=%-4d 锚点行=%-4d 同锚组=%-3d 可定序=%-3d funcstart=%-3d lines=%-9s S/W/A/X=%d/%d/%d/%d "
                    "相邻对一致=%s(%d/%d) 全局一致=%s [扩展口径 %s(%d/%d)] "
                    "缺口G1/G2/G3/G4=%d/%d/%d/%d(+G1灵敏度%d) hyb未锚定=%d"
                    % (s["module"], s["slots"], s["anchors"], s["funcs"], s["orderable"],
                       s["funcstarts"],
                       "%d..%d" % (s["lo"], s["hi"]),
                       s["tier_s"], s["tier_w"], s["tier_a"], s["tier_x"], adj, s["adj"],
                       s["adj_tot"], inv, eadj, s["eadj"], s["eadj_tot"],
                       s["g1"], s["g2"], s["g3"], s["g4"], s["g1s"], s["hyb_unmatched"]))
    out = head + ["\t".join(TSV_COLS)]
    for r in rows:
        out.append("\t".join(str(x) for x in
                             [r["file"], r["line"], r["our_fn"], r["our_loc"], r["pos"],
                              r["delta"], r["verdict"], r["blank"], r["n"], r["span"],
                              r["insts"], r["orderable"], r["pair"], r["tier"], r["hyb"]]))
    return "\n".join(out) + "\n"


def render_gaps(gaps, by_module, hyb_idx, tau_eff, ctl_b):
    head = [
        "# file_structure_gaps.tsv —— 由 tools/file_structure.py build 生成，**不要手改**",
        "# 重算：$MAIN/.reference/venv/bin/python tools/file_structure.py build",
        "# 用途：给后续「找回未归属函数」任务的输入 —— 这些行号区间里**疑似还有原版函数**",
        "# 形态：G1        = 相邻两锚之间无锚空白 ≥ τ_eff（τ_eff=%d，plan §4）——**主口径**" % tau_eff,
        "#       G1(灵敏度 τ=N) = 未达 τ_eff，但按 plan §4 预注册的灵敏度档 τ∈{100,200,400} 会命中；",
        "#                      **只作参考，不进主数字**（本轮 τ_eff=%d 下一行都没有 ⇒ 主口径 G1 = 0）" % tau_eff,
        "#       G2        = 两个不同位点的锚点区间交叠（内联 ⇒ 行号交错，不能仅按行号定序；plan §4 G2）",
        "#       G3        = 同一 (文件,行) 被 ≥2 个**可定序**位点引用（真内联/共享 panic 点；plan §4 G3）",
        "#       G4        = 同锚组里有 ≥3 个 funcstart（宏展开/泛型单态化/共享位点 ⇒ 我们分辨不出这些函数；",
        "#                   plan §9 事后追加）—— 对「找回未归属函数」是**最有用的输入**",
        "# ⚠️ G1 只有**行号空白**这一条证据，**不含地址证据**：别读成「这里确定有函数」",
        "# 「可能是什么」列里的 hyb 是 `fixtures/hybrid_names.tsv` 的**候选名**（线索档，含顺序轴",
        "#   对齐成分，不能当结论）；`—` = 连线索都没有",
    ]
    out = head + ["\t".join(GAP_COLS)]
    for g in sorted(gaps, key=lambda x: (x["mod"], x["kind"], (x["a"] or {}).get("lo", 0))):
        mod = g["mod"]
        cands = hyb_idx.get(mod, [])
        if g["kind"] == "G2":
            a, b = g["a"], g["b"]
            rng = "交叠：A.hi=%d / B.lo=%d" % (a["hi"], b["lo"])
            sides = "0x%06x(lo=%d,hi=%d,锚点=%d) ↔ 0x%06x(lo=%d,hi=%d,锚点=%d)" % (
                a["addr"], a["lo"], a["hi"], a["n"], b["addr"], b["lo"], b["hi"], b["n"])
            guess = "内联：B 的锚点行落在 A 的行区间内（或反之）⇒ 两者在源码里交错，不能仅按行号定序"
            want = ("逐指令确认 A/B 的内联关系（`fixtures/boundary_audit.tsv` 的 multi 那一类）；"
                    "必要时用 `tools/loc_xrefs.py func <addr>` 看证据链")
            out.append("\t".join([mod, rng, sides, "0", guess, want, g["kind"]]))
            continue
        if g["kind"] == "G4":
            g4 = g["a"]
            rng = "%d..%d" % (g4["lo"], g4["hi"])
            sides = "同锚组 %d 个 funcstart：%s" % (
                g4["insts"], ",".join("0x%06x" % x for x in g4["addrs"][:8])
                + ("…" if len(g4["addrs"]) > 8 else ""))
            out.append("\t".join([mod, rng, sides, str(g4["insts"] - 1),
                                  "宏展开 / 泛型单态化 / 多个函数共享同一套 Location 行："
                                  "行号证据**无法**把它们分开",
                                  "每个 funcstart 各自的符号名或调用者（arm64 `.symtab` / 调用图）；"
                                  "`fixtures/hybrid_names_pending` 里的候选名只能当线索", "G4"]))
            continue
        if g["kind"].startswith("G1"):
            a, b = g["a"], g["b"]
            rng = "%d..%d" % (a["hi"] + 1, b["lo"] - 1)
            sides = "0x%06x(lo=%d,hi=%d,锚点=%d,实例=%d) ↔ 0x%06x(lo=%d,hi=%d,锚点=%d,实例=%d)" % (
                a["addr"], a["lo"], a["hi"], a["n"], a["insts"],
                b["addr"], b["lo"], b["hi"], b["n"], b["insts"])
            guess = "—（本文件只有这两个锚点，无地址证据）"
            if cands:
                guess = "线索（hyb 名，**不当结论**）：本文件有 %d 个 funcstart 被预测属于它但无锚点，如 %s" % (
                    len(cands), ", ".join("0x%06x" % x for x in cands[:5]))
            want = ("第三个独立来源：arm64 `.symtab` 按文件的符号条数/顺序，或另一版本二进制的 Location 行号；"
                    "本轮只有行号空白")
            out.append("\t".join([mod, rng, sides, str(g["blank"]), guess, want, g["kind"]]))
        else:
            sides = "行 %d 被 %d 个 funcstart 引用（去重后仍 ≥2 个位点 ⇒ 真内联/共享 Location）：%s" % (
                g["line"], len(g["fns"]), ",".join("0x%06x" % x for x in g["fns"][:8]))
            out.append("\t".join([mod, "%d..%d" % (g["line"], g["line"]), sides, "0",
                                  "内联：同一 (文件,行) 的 Location 被 ≥2 个**不同**位点引用",
                                  "逐指令确认内联边界；与 `boundary_audit.tsv` 的 multi 交叉核对", "G3"]))
    return "\n".join(out) + "\n"


# ---------------------------------------------------------------------------
# 8. 主流程
# ---------------------------------------------------------------------------
def run_build(so, ctl_b, verbose=True):
    ix = LX.LocXrefs(so, verbose=False)
    per_func, line_funcs, dropped, slots_by_file = collect_anchors(ix, verbose=verbose)
    v2 = load_v2()
    hyb = load_hyb()
    matrix = load_matrix()
    ours_by_qual, ours_by_method, our_files = load_ours_fns()
    annot = load_annotations(our_files)
    all_files = set(our_files)
    tau_eff = (ctl_b or {}).get("tau_eff", TAU_BASE)
    rows, gaps, stats, by_module, hyb_idx, sens = build_skeleton(
        per_func, line_funcs, slots_by_file, v2, hyb, matrix, ours_by_qual, ours_by_method,
        all_files, tau_eff, annot=annot)
    tsv = render_tsv(rows, stats, ix, tau_eff, ctl_b, dropped, sens)
    gtsv = render_gaps(gaps, by_module, hyb_idx, tau_eff, ctl_b)
    return dict(rows=rows, gaps=gaps, stats=stats, by_module=by_module, hyb_idx=hyb_idx,
                tsv=tsv, gtsv=gtsv, tau_eff=tau_eff, per_func=per_func, sens=sens, annot=annot,
                line_funcs=line_funcs, ix=ix, dropped=dropped, ctl_b=ctl_b,
                our_files=our_files)


def print_report(res):
    rows, stats, gaps = res["rows"], res["stats"], res["gaps"]
    print("\n=== 每模块骨架（按锚点数降序）===")
    print("%-46s %6s %7s %6s %5s %13s %8s %8s %8s %s"
          % ("原版文件", "槽", "锚点行", "同锚组", "可定序", "lines", "相邻S/W", "全局S/W",
             "扩展+S/W+A", "缺口 G1/G2/G3/G4"))
    for s in sorted(stats, key=lambda x: (-x["anchors"], x["module"])):
        adj = ("%.0f%%" % (100.0 * s["adj"] / s["adj_tot"])) if s["adj_tot"] else "n/a"
        inv = ("%.0f%%" % (100.0 * (1 - s["inv"] / s["inv_tot"]))) if s["inv_tot"] else "n/a"
        eadj = ("%.0f%%" % (100.0 * s["eadj"] / s["eadj_tot"])) if s["eadj_tot"] else "n/a"
        print("%-46s %6d %7d %6d %5d %13s %8s %8s %8s %d/%d/%d/%d"
              % (s["module"], s["slots"], s["anchors"], s["funcs"], s["orderable"],
                 "%d..%d" % (s["lo"], s["hi"]), adj, inv, eadj,
                 s["g1"], s["g2"], s["g3"], s["g4"]))
    adj = sum(s["adj"] for s in stats)
    tot = sum(s["adj_tot"] for s in stats)
    inv = sum(s["inv"] for s in stats)
    itot = sum(s["inv_tot"] for s in stats)
    eadj = sum(s["eadj"] for s in stats)
    etot = sum(s["eadj_tot"] for s in stats)
    einv = sum(s["einv"] for s in stats)
    eitot = sum(s["einv_tot"] for s in stats)
    print("\n扩展口径（+A 档）：相邻对一致 %d/%d = %.1f%%；全局 %s"
          % (eadj, etot, 100.0 * eadj / etot if etot else 0.0,
             ("%.1f%%" % (100.0 * (1 - einv / eitot)) if eitot else "n/a")))
    print("τ 灵敏度（G1 缺口数）：%s（主口径 τ_eff=%d ⇒ G1=%d）"
          % (" / ".join("τ=%d:%d" % (t, res["sens"][t]) for t in sorted(res["sens"])),
             res["tau_eff"], sum(s["g1"] for s in stats)))
    print("\n合池：相邻对一致 %d/%d = %.1f%%；全局（1−逆序率）%s；模块 %d；同锚组 %d（可定序 %d /"
          " funcstart %d）；S/W/A/X 档 = %d/%d/%d/%d；G1/G2/G3/G4 = %d/%d/%d/%d；τ_eff=%d"
          % (adj, tot, 100.0 * adj / tot if tot else 0.0,
             ("%.1f%%" % (100.0 * (1 - inv / itot)) if itot else "n/a"),
             len(stats), sum(s["funcs"] for s in stats),
             sum(s["orderable"] for s in stats), sum(s["funcstarts"] for s in stats),
             sum(s["tier_s"] for s in stats), sum(s["tier_w"] for s in stats),
             sum(s["tier_a"] for s in stats), sum(s["tier_x"] for s in stats),
             sum(s["g1"] for s in stats), sum(s["g2"] for s in stats),
             sum(s["g3"] for s in stats), sum(s["g4"] for s in stats),
             res["tau_eff"]))
    ver = collections.Counter(r["verdict"] for r in rows)
    print("判定分布：%s" % ", ".join("%s=%d" % kv for kv in ver.most_common()))


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", nargs="?", default="build",
                    choices=["build", "control-a", "control-b", "both", "check"])
    ap.add_argument("--so", default=LX.DEFAULT_SO)
    ap.add_argument("--ours-so", default=OURS_SO)
    o = ap.parse_args(argv[1:])
    if not os.path.exists(o.so):
        raise SystemExit("目标 .so 不存在：%s" % o.so)

    problems = []
    ctl_b = None
    if o.cmd in ("control-b", "both", "build", "check"):
        ctl_b, probs = cmd_control_b(o.ours_so)
        problems += probs
    if o.cmd in ("control-a", "both", "build", "check"):
        problems += cmd_control_a(LX.LocXrefs(o.so, verbose=False))
    if o.cmd in ("build", "check"):
        res = run_build(o.so, ctl_b)
        print_report(res)
        if res["ctl_b"]:
            print("（自对照 adj_rate_self=%.2f%%，决策规则见 plan §5.2 ⇒ %s）"
                  % (res["ctl_b"]["rate"],
                     "前提成立" if res["ctl_b"]["rate"] >= 95 else
                     ("前提成立但只报锚点≥3" if res["ctl_b"]["rate"] >= 80 else "**前提被证伪**")))
        if o.cmd == "check":
            bad = 0
            for path, text in ((TSV_PATH, res["tsv"]), (GAPS_PATH, res["gtsv"])):
                old = open(path, encoding="utf-8").read() if os.path.exists(path) else ""
                if old != text:
                    print("✗ %s 与重算结果不一致（%d vs %d 字符）"
                          % (os.path.relpath(path, ROOT), len(old), len(text)))
                    bad += 1
                else:
                    print("✓ %s 与重算结果逐字符一致（%d 字符）"
                          % (os.path.relpath(path, ROOT), len(text)))
            if bad or problems:
                return 1
            return 0
        if problems:
            print("\n✗ 正对照失败 %d 条 ⇒ **不写** fixtures（fail-closed）：" % len(problems))
            for p in problems:
                print("  -", p)
            return 1
        open(TSV_PATH, "w", encoding="utf-8").write(res["tsv"])
        open(GAPS_PATH, "w", encoding="utf-8").write(res["gtsv"])
        print("\n[file_structure] 写出 %s（%d 行数据）与 %s（%d 行数据）"
              % (os.path.relpath(TSV_PATH, ROOT), len(res["rows"]),
                 os.path.relpath(GAPS_PATH, ROOT), len(res["gaps"])))
    if problems:
        print("\n✗ 正对照失败 %d 条：" % len(problems))
        for p in problems:
            print("  -", p)
        return 1
    print("\n✓ 全部正对照通过")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
