#!/usr/bin/env python3
"""「原版函数 → 真实源文件」的**逐函数**审计：527 个可归属函数 vs 我们的文件布局。

## 为什么要有这个脚本

`AGENTS.md` §0 说源码级还原的终点是「**文件 / 模块 / 函数边界**能对上」。
在那之前，`docs/coverage-matrix.tsv` 只到**模块**粒度：它说「`graphwar_play_screen_control.rs`
这一行有 10 个可归属函数、ours_file 是 `src/graphwar_play_screen_control.rs`」——
但它**不能**回答「这 10 个函数各自的原版 `panic::Location` 落在哪个文件，
我们是不是把它们放进了同名文件」。已知的反例：`LevelRules` 那三个判据函数
（`0x473520` / `0x4759f0` / `0x4780c0`）的 Location **全落 `graphwar_game_state_manager.rs`**，
可它们的我方实现住在 `src/graphwar_play_screen_control.rs`（`docs/02` §17.4 登记了很久）。

本脚本把这件事做成机器可判的 527 行台账（`fixtures/boundary_audit.tsv`）。

## 判据（三档；`AGENTS.md` §4「边界无证据时不猜、不搬」）

| 判定 | 条件 | 处置 |
|---|---|---|
| `multi` | 该函数的 `panic::Location` **锚点跨 ≥2 个原版模块文件**（内联把别的文件的行号带进来） | **一律不搬**，只登记 |
| `mismatch` | 单模块，但**我们的布局里没有这个模块名的文件**（`src/<module>` ∉ 矩阵的 `ours_file`）⇒ 我们的实现在另一个名字的文件里 | 登记；有强证据才搬 |
| `match` | 单模块，且 `src/<module>` ∈ `ours_file` ⇒ 我们的布局有一份同名文件 | — |

`match` 再分两级（写在 `basis` 里，不改判定）：

* `clean` —— 该模块**只**对应一个我方文件（`ours_file` 就那一个）⇒ 逐函数边界干净；
* `split-N` —— 该模块被我方拆进 **N** 个文件（`docs/02` §17.4 的「混装（待拆）」那一类）
  ⇒ 文件名对得上，但**具体某个函数落在哪个文件**没有证据，仍不搬。

## 两个必须说清的口径

1. **函数边界用 Ghidra 的 13,263 个 funcstarts**（`analysis/funcstarts_ghidra_va.pkl`），
   和 `tools/coverage_matrix.py` **同口径**，所以总数必须都是 **527 / 86**。
   ⚠️ 但 `.eh_frame` 的 FDE 和 Ghidra 边界**不是一回事**：求值器 `0x4eb9d0` 的 FDE 是
   `0x4eb9d0..0x4ebbe0`，而 Ghidra 在这段里切出了 `0x4eb9d0 / 0x4ebb01 / 0x4ebb59 / 0x4ebb68`
   四个入口；`0x4ebbce` 那条 `lea`（→ `math_function.rs:184:24`）按 Ghidra 边界归 **`0x4ebb68`**，
   不归 `0x4eb9d0`。⇒ `addr_probe.py locs 0x4eb9d0`（FDE 口径）会报 `math_function.rs`，
   而本脚本（Ghidra 口径）里 `0x4eb9d0` **不在** 527 里 —— 两个都对，别当成矛盾。
2. **反汇编按函数起点逐个重新同步**（range = `[funcstart_i, funcstart_{i+1})`），不是从
   `.text` 头一路线扫。两者在本二进制上给出**完全相同**的 527 个函数与 86 个跨模块函数
   （自校验里断言），但按函数重同步不会因为前面的数据块而失步，判据更稳。

## 名字从哪来

* `fixtures/name_to_addr_v2.tsv` —— **三向符号匹配**（Windows COFF + Android arm64 `.symtab`
  × 本 Linux 二进制，落地键是 `(file,line,col)` 双向唯一）。**这是真名字**，`name_src=v2*`。
* `fixtures/hybrid_names.tsv` —— 结构指纹/顺序轴**预测**的候选名，`name_src=hyb:*`
  （高置信/推断/歧义/未知 四档）。候选名可用于阅读定位，**不能当结论**。
* 都没有 ⇒ `name_src=—`。

## 名字与 Location 的交叉核对

名字里的模块路径（`graphwar_xxx::Type::method`）来自**符号表**，`loc_main_file` 来自
**`panic::Location`** —— 两个独立来源。8 个函数两边不一致（内联/外联边界），
在 `basis` 里标 `名字模块≠主文件(名字=…)`；这是**线索**不是判据（已知一例 `0x4500c0`：
符号说 `graphwar_server_connection_handler::new`，Location 说 `graphwar_game_server.rs`，
`docs/06` §65 已按 Location 结案）。

## 用法

    .reference/venv/bin/python tools/boundary_audit.py            # 生成 + 打印总账
    .reference/venv/bin/python tools/boundary_audit.py --check    # 只校验已入库台账是否还一致
    .reference/venv/bin/python tools/boundary_audit.py --control  # 只跑正对照（锚点自检）

产物：`fixtures/boundary_audit.tsv`（527 行，唯一真源；本脚本生成，不要手改）。
需要 `.reference/upstream/` 里的 `.so` 与 funcstarts pkl（都不入库，见 README）。
"""

import collections
import hashlib
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)                 # gw2-rebuild/
sys.path.insert(0, HERE)

# ---------------------------------------------------------------------------
# 依赖 `tools/coverage_matrix.py` 做三件事：找二进制/pkl、解析 Location、
# 读它硬编码的 78 个模块记录（module -> ours_file）。**不复制那套数据**，
# 否则两份模块表会漂。
# ---------------------------------------------------------------------------
import coverage_matrix as CM  # noqa: E402

from capstone import Cs, CS_ARCH_X86, CS_MODE_64               # noqa: E402
from capstone.x86 import X86_OP_MEM, X86_OP_IMM, X86_REG_RIP    # noqa: E402

TSV_PATH = os.path.join(ROOT, "fixtures", "boundary_audit.tsv")
TOOL_VERSION = "1"

# 正对照：解析器必须落 math_function.rs（`AGENTS.md`「间接调用与函数归属」第 2 条）
CONTROL_MUST_LAND = [(0x4ED670, "math_function.rs")]

# 已知的、必须能复现的三条 LevelRules 证据（`docs/02` §17.4 / `docs/06` §59.2）
LEVELRULES_ANCHORS = [
    (0x473520, "graphwar_game_state_manager.rs", 5,  "get_alive_teams"),
    (0x4759F0, "graphwar_game_state_manager.rs", 12, "queue_end_turn_trigger::{{closure}}"),
    (0x4780C0, "graphwar_game_state_manager.rs", 5,  "set_state"),
]

EXPECT_ATTRIBUTED = 527
EXPECT_CONTESTED = 86


# ---------------------------------------------------------------------------
# 名字表
# ---------------------------------------------------------------------------
def read_tsv(path):
    rows = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("#") or not line.strip():
                continue
            rows.append(line.rstrip("\n").split("\t"))
    return rows


def load_names():
    """返回 {addr: (name, name_src)}。

    优先级：name_to_addr_v2（三向真名）> hybrid_names（候选）。
    """
    names = {}
    v2 = os.path.join(ROOT, "fixtures", "name_to_addr_v2.tsv")
    if os.path.exists(v2):
        rows = read_tsv(v2)
        hdr = rows[0]
        imod, ityp, imet, iadr = hdr.index("module"), hdr.index("type"), hdr.index("method"), hdr.index("x86_addr")
        ijoin = hdr.index("join_kind")
        for r in rows[1:]:
            try:
                a = int(r[iadr], 16)
            except ValueError:      # 「歧义」行没有地址
                continue
            mod = r[imod] + ".rs" if r[imod] and not r[imod].endswith(".rs") else r[imod]
            name = "::".join(x for x in (mod, r[ityp], r[imet]) if x)
            join = r[ijoin]
            src = "v2唯一锚" if join == "唯一锚" else "v2跨版本"
            names[a] = (name, src)

    hyb = os.path.join(ROOT, "fixtures", "hybrid_names.tsv")
    if os.path.exists(hyb):
        rows = read_tsv(hyb)
        hdr = rows[0]
        iadr, iname, iconf = hdr.index("x86_addr"), hdr.index("name"), hdr.index("confidence")
        for r in rows[1:]:
            try:
                a = int(r[iadr], 16)
            except ValueError:
                continue
            if a in names or not r[iname]:
                continue
            conf = r[iconf]
            if conf.startswith("order_tier"):
                tier = "顺序" + conf.split("=", 1)[-1]
            elif "高置信" in conf:
                tier = "高置信"
            elif "推断" in conf:
                tier = "推断"
            elif "歧义" in conf:
                tier = "歧义"
            else:
                tier = "未知"
            names[a] = (r[iname], "hyb:" + tier)
    return names


# ---------------------------------------------------------------------------
# 原版这一半：按 Ghidra 函数边界重新同步地扫 `.text`，收 Location 投票
# ---------------------------------------------------------------------------
def scan_votes(E, locs, funcstarts):
    """返回 {funcstart: {(module, line): 引用指令数}}。

    区间 = `[funcstart_i, funcstart_{i+1})`，逐函数重新起反汇编器 —— 见文件头口径 2。
    """
    text = E.sec(".text")
    base, size = text["addr"], text["size"]
    blob = E.data[text["offset"]:text["offset"] + size]
    end = base + size
    starts = [a for a in funcstarts if base <= a < end]

    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    votes = {}
    for i, st in enumerate(starts):
        hi = starts[i + 1] if i + 1 < len(starts) else end
        off = st - base
        for ins in md.disasm(blob[off:off + (hi - st)], st):
            for op in ins.operands:
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    tgt = (ins.address + ins.size + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
                elif op.type == X86_OP_IMM:
                    tgt = op.imm & 0xFFFFFFFFFFFFFFFF
                else:
                    continue
                hit = locs.get(tgt)
                if hit is None:
                    continue
                module, line, _col = hit
                votes.setdefault(st, {}).setdefault((module, line), 0)
                votes[st][(module, line)] += 1
    return votes


def module_votes(func_votes):
    """{(module,line): n} -> {module: {line, ...}}（与 coverage_matrix 的「按行去重」同口径）。"""
    out = {}
    for (module, line) in func_votes:
        out.setdefault(module, set()).add(line)
    return out


def pick_main(bymod):
    """锚点行数最多者胜，并列取模块名字典序 —— 与 coverage_matrix.attribute 完全同规则。"""
    return min(bymod, key=lambda m: (-len(bymod[m]), m))


# ---------------------------------------------------------------------------
# 汇编文本：每行的判定
# ---------------------------------------------------------------------------
def build_rows(votes, names):
    rows = []
    for addr in sorted(votes):
        bymod = module_votes(votes[addr])
        main = pick_main(bymod)
        main_lines = sorted(bymod[main])
        total = sum(len(v) for v in bymod.values())
        others = [(m, len(v)) for m, v in bymod.items() if m != main]
        others.sort(key=lambda kv: (-kv[1], kv[0]))

        rec = CM.M.get(main)
        ours = list(rec["ours"]) if rec else []
        ours_file = "+".join(ours) if ours else "—"
        same_named = ("src/" + main) in ours

        if len(bymod) > 1:
            verdict = "multi"
            counts = "+".join(str(n) for n in [len(bymod[main])] + [n for _m, n in others])
            basis = "Location 跨 %d 个文件（主:次 锚点行数 = %s）⇒ 边界无证据，不搬" % (len(bymod), counts)
        elif same_named:
            verdict = "match"
            if len(ours) == 1:
                basis = "clean：该模块只对应 1 个我方文件"
            else:
                basis = "split-%d：该模块被我方拆进 %d 个文件（逐函数落点无证据）" % (len(ours), len(ours))
        else:
            verdict = "mismatch"
            basis = "无同名文件：原版模块 %s 的我方实现在 %s" % (main, ours_file)

        name, name_src = names.get(addr, ("", "—"))
        if name:
            nm_mod = name.split("::", 1)[0]
            if nm_mod.endswith(".rs") and nm_mod != main:
                basis += "；名字模块≠主文件(名字=%s)" % nm_mod
        hit = 100.0 * len(bymod[main]) / total if total else 0.0
        rows.append(dict(
            addr="0x%06x" % addr,
            name=name or "—",
            name_src=name_src,
            loc_main_file="src/" + main,
            loc_main_lines=",".join(str(x) for x in main_lines),
            loc_main_anchors=len(bymod[main]),
            loc_other_files=";".join("src/%s(%d)" % (m, n) for m, n in others) or "—",
            ours_file=ours_file,
            ours_hit="%d%%" % round(hit),
            verdict=verdict,
            basis=basis,
        ))
    return rows


HEADER_COLS = ["addr", "name", "name_src", "loc_main_file", "loc_main_lines",
               "loc_main_anchors", "loc_other_files", "ours_file", "ours_hit",
               "verdict", "basis"]


def render(rows, meta, control):
    out = []
    out.append("# boundary_audit.tsv —— 由 tools/boundary_audit.py 生成，**不要手改**")
    out.append("# 重算：python3 tools/boundary_audit.py          只校验：--check          正对照：--control")
    out.append("# 一行 = 一个**可归属函数**（按 panic::Location 硬归属，Ghidra 13,263 个 funcstarts 口径）")
    out.append("# 判定（AGENTS.md §4「边界无证据时不猜、不搬」）：")
    out.append("#   match    单模块，且我方布局有一份同名文件 src/<模块>（basis 里再分 clean / split-N）")
    out.append("#   mismatch 单模块，但我方**没有**这个模块名的文件 ⇒ 我们的实现在另一个名字的文件里")
    out.append("#   multi    Location 锚点跨 ≥2 个原版模块文件 ⇒ **一律不搬**，只登记")
    out.append("# loc_main_file：锚点行数最多的模块（并列取字典序，与 coverage_matrix.attribute 同规则）")
    out.append("# loc_main_lines：主文件里被引用的行号（升序）—— 可当证据直接去 diff 原版行号")
    out.append("# loc_other_files：其余模块 + 各自锚点行数（multi 的来源）")
    out.append("# ours_file：coverage-matrix 给该模块的我方文件；ours_hit：主文件锚点占全部锚点的比例")
    out.append("# name_src：v2* = 三向符号匹配的真名；hyb:* = 结构指纹预测的候选名（不当结论）；— = 无名")
    out.append("# 正对照：%s" % control)
    out.append("# 总数：%d 行（match %d / mismatch %d / multi %d）；binary_sha256=%s；funcstarts_sha256=%s"
               % (meta["rows"], meta["match"], meta["mismatch"], meta["multi"],
                  meta["binary_sha256"], meta["funcstarts_sha256"]))
    out.append("\t".join(HEADER_COLS))
    for r in rows:
        out.append("\t".join(str(r[c]) for c in HEADER_COLS))
    return "\n".join(out) + "\n"


# ---------------------------------------------------------------------------
# 生成 + 自校验
# ---------------------------------------------------------------------------
def build():
    if not os.path.exists(CM.SO):
        raise SystemExit("boundary_audit: 找不到原版二进制 %s（本仓库不入库，见 README）" % CM.SO)
    if not os.path.exists(CM.FUNCSTARTS):
        raise SystemExit("boundary_audit: 找不到 %s（由 analysis/build_index.py 生成）" % CM.FUNCSTARTS)

    E = CM.Elf(CM.SO)
    locs = CM.resolve_locations(E, CM.find_literals(E))
    funcstarts = CM.load_funcstarts(CM.FUNCSTARTS)
    votes = scan_votes(E, locs, funcstarts)
    names = load_names()
    rows = build_rows(votes, names)

    # ---- 自校验 1：总数与跨模块数与 coverage_matrix 一致（同口径的硬证据） ----
    assert len(rows) == EXPECT_ATTRIBUTED, \
        "boundary_audit: 可归属函数 %d != %d（口径与 coverage_matrix 不一致）" % (len(rows), EXPECT_ATTRIBUTED)
    contested = sum(1 for r in rows if r["verdict"] == "multi")
    assert contested == EXPECT_CONTESTED, \
        "boundary_audit: multi %d != %d（口径与 coverage_matrix 不一致）" % (contested, EXPECT_CONTESTED)

    # ---- 自校验 2：正对照（AGENTS.md 的取数链自检） ----
    per_func = {}
    for r in rows:
        per_func[int(r["addr"], 16)] = r["loc_main_file"]
    control_bits = []
    for addr, want in CONTROL_MUST_LAND:
        got = per_func.get(addr)
        assert got == "src/" + want, \
            "boundary_audit 正对照失败：0x%x 落 %r，期望 src/%s" % (addr, got, want)
        control_bits.append("0x%x -> %s ✓" % (addr, want))
    # 求值器 0x4eb9d0：Ghidra 口径下**不在** 527 里（见文件头口径 1），这里显式钉住
    assert 0x4EB9D0 not in per_func, \
        "boundary_audit: 0x4eb9d0 出现在 527 里了 —— 文件头口径 1 的前提已变，需要复核"
    control_bits.append("0x4eb9d0 不在 527（Ghidra 切分，见文件头口径 1）✓")

    # ---- 自校验 3：LevelRules 三条证据必须复现（docs/02 §17.4 / docs/06 §59.2） ----
    for addr, want, want_anchors, label in LEVELRULES_ANCHORS:
        got = per_func.get(addr)
        assert got == "src/" + want, \
            "boundary_audit: LevelRules 证据 %s(0x%x) 落 %r，期望 src/%s" % (label, addr, got, want)
        row = next(r for r in rows if r["addr"] == "0x%06x" % addr)
        assert row["loc_main_anchors"] == want_anchors, \
            "boundary_audit: %s(0x%x) 锚点数 %s != %d" % (
                label, addr, row["loc_main_anchors"], want_anchors)

    meta = dict(
        rows=len(rows),
        match=sum(1 for r in rows if r["verdict"] == "match"),
        mismatch=sum(1 for r in rows if r["verdict"] == "mismatch"),
        multi=contested,
        binary_sha256=E.sha256,
        funcstarts_sha256=hashlib.sha256(open(CM.FUNCSTARTS, "rb").read()).hexdigest(),
    )
    control = "；".join(control_bits + ["LevelRules 0x473520/0x4759f0/0x4780c0 -> "
                                      "graphwar_game_state_manager.rs ✓"])
    return rows, meta, control, votes, locs, funcstarts


def summary(rows, meta):
    print()
    print("%d 个可归属函数：match %d / mismatch %d / multi %d"
          % (meta["rows"], meta["match"], meta["mismatch"], meta["multi"]))
    by_mod = collections.Counter(r["loc_main_file"] for r in rows)
    print()
    print("multi 的来源（主文件:次文件 锚点行数分布）：")
    dist = collections.Counter(r["loc_other_files"].count(";") + 1 for r in rows if r["verdict"] == "multi")
    print("  跨 %s 个文件" % " / ".join("%d:%d" % (k, dist[k]) for k in sorted(dist)))
    print()
    print("mismatch 清单：")
    bad = [r for r in rows if r["verdict"] == "mismatch"]
    for r in sorted(bad, key=lambda x: (x["loc_main_file"], x["addr"])):
        print("  %s  %-14s %-38s ours=%s" % (r["addr"], r["verdict"], r["loc_main_file"], r["ours_file"]))
    if not bad:
        print("  （无）")
    print()
    print("match 里被我方拆成多文件的模块（split-N，逐函数落点仍无证据）：")
    split = collections.Counter()
    for r in rows:
        if r["verdict"] == "match" and r["basis"].startswith("split"):
            split[(r["loc_main_file"], r["ours_file"])] += 1
    for (loc, ours), n in sorted(split.items(), key=lambda kv: (-kv[1], kv[0])):
        print("  %-42s %2d 个函数  ->  %s" % (loc, n, ours))
    if not split:
        print("  （无）")
    print()


# ---------------------------------------------------------------------------
# 补充轴：**我方文件自己点名的原版地址**归哪个模块（线索档，不是判据）
# ---------------------------------------------------------------------------
# 527 行台账判的是「原版函数的 Location vs 我们的文件布局」。它判不出下面这类事：
# 我方文件里**自己重写**了一段属于别的模块的行为（`LevelRules` 就是这种 ——
# 它没有对应的原版"函数"，所以不会出现在 527 行里）。可机器判的替代口径是：
# 我方文件在注释/断言里点名的原版地址，其 Location 主文件是不是本文件名。
# 这是**线索**（注释可能过期），但它正好把"哪些文件里混着别的模块的行为"排出来。
ADDR_RE = re.compile(r"0x([0-9a-f]{6,7})\b")


def citations(rows):
    """返回 [(我方文件, 引用数, 本文件命中, {别的模块: (引用数, {addr...})})]。"""
    per = {int(r["addr"], 16): r for r in rows}
    out = []
    for dirpath, _dirs, files in os.walk(os.path.join(ROOT, "src")):
        for fn in sorted(files):
            if not fn.endswith(".rs"):
                continue
            path = os.path.join(dirpath, fn)
            rel = os.path.relpath(path, ROOT)
            src = open(path, encoding="utf-8", errors="replace").read()
            cnt = collections.Counter()
            addrs = collections.defaultdict(set)
            for m in ADDR_RE.finditer(src):
                a = int(m.group(1), 16)
                rec = per.get(a)
                if rec is None:
                    continue
                cnt[rec["loc_main_file"]] += 1
                addrs[rec["loc_main_file"]].add(a)
            if not cnt:
                continue
            own = "src/" + fn
            other = {k: (cnt[k], addrs[k]) for k in cnt if k != own}
            out.append((rel, sum(cnt.values()), cnt.get(own, 0), other))
    out.sort(key=lambda r: (-sum(v[0] for v in r[3].values()), r[0]))
    return out


def print_citations(rows):
    print("我方文件点名的原版地址（按「点名了别的模块多少次」降序；线索档）")
    print("%-52s %5s %6s  %s" % ("我方文件", "引用", "本模块", "点名到的别的模块（次数）"))
    for rel, tot, own, other in citations(rows):
        if not other:
            continue
        top = ", ".join("%s×%d" % (k.replace("src/", ""), v[0])
                        for k, v in sorted(other.items(), key=lambda kv: (-kv[1][0], kv[0]))[:5])
        print("%-52s %5d %6d  %s" % (rel, tot, own, top))
    print()


def main(argv):
    check = "--check" in argv
    control_only = "--control" in argv
    want_citations = "--citations" in argv

    rows, meta, control, votes, locs, funcstarts = build()
    if control_only:
        print("正对照：")
        print("  " + control.replace("；", "\n  "))
        return 0
    if want_citations:
        print_citations(rows)
        return 0

    text = render(rows, meta, control)
    if check:
        if not os.path.exists(TSV_PATH):
            print("boundary_audit --check: 缺 %s（先跑一次不带 --check）"
                  % os.path.relpath(TSV_PATH, ROOT))
            return 1
        with open(TSV_PATH, encoding="utf-8") as fh:
            if fh.read() != text:
                print("boundary_audit --check: %s 与重算结果不一致（重跑一次生成）"
                      % os.path.relpath(TSV_PATH, ROOT))
                return 1
        print("boundary_audit --check: OK")
        summary(rows, meta)
        return 0

    with open(TSV_PATH, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
    print("wrote %s（%d 行）" % (os.path.relpath(TSV_PATH, ROOT), len(rows)))
    print("正对照：" + control)
    summary(rows, meta)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
