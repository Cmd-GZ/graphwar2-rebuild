#!/usr/bin/env python3
"""T50：审计 `client/tools/*_check.gd` 里每一条 headless 断言的**期望值来源**。

**为什么要有这个工具**

`AGENTS.md` §0.5：「写测试的人不许同时是"判定测试通过标准的人"——自写断言只能证明自洽。」
T37（`docs/06` §85）点名 `client/tools/*_check.gd` 这批断言从来没审过自洽性，
而实例 A（`src/entity_drawer.rs::update_sprite` 的 `scale.x` 漏 `k`，活了 19 天 / 557 commit）
的**第 3 处镜像**正是 `entity_drawer_check.gd::expect_sprite()`。本工具把那批断言逐条判档。

## 唯一的核心判据（"能不能被真实错误打破"）

一条断言值不值钱，只问一句：**把被测实现的算式改错，它会不会红？**

* **不会红** ⇒ `M`（镜像）。它的期望值与实现**同一套算术**算出，两边一起错。
* **会红** ⇒ 至少是回归保护。再按期望值来源分：
  * `C`——期望值是**我方插桩计数/接口形状/冻结字面常量**（能防回归，但不是复原证据）；
  * `G`——期望值有**外部出处**（反汇编地址/冻结语料/真帧像素/已入库 `fixtures/`）。

## 三档 + 子码（`fixtures/headless_assertion_audit.tsv` 的 `tier` / `sub` 两列）

| tier | sub | 判据（`classify()` 实现，可复算） |
|---|---|---|
| `M` | `R1-local-helper` | 条件里（或经局部变量间接）调用**本地重算函数**：`LOCAL_REIMPL` 里那 8 个 |
| `M` | `R2-taint` | 条件里引用的局部变量被 R1 污染（`var want := expect_sprite(...)` → `ok(kids[0].position.is_equal_approx(want["position"]))`） |
| `M` | `R3-two-ours` | 条件两边**都是我方的观测值**互相比（`a.is_equal_approx(b)`、`by_prims == by_formula`）——只能证"两个镜像一致" |
| `M` | `R4-ratio` | 期望是**我方计数的比例不变式**（`arcs == 3 * draws`、`per_frame == updates * 2`） |
| `M` | `R5-roundtrip` | `set(X, v)` 之后 `ok(get(X) == v)`；message 带「往返/读回来/设进去」 |
| `M` | `R6-mirror-text` | 断言语句或紧邻注释自己写着「同一条算式 / 独立重算 / 镜像 / 手算期望」 |
| `C` | `C1-shape` | 接口形状：`class_exists` / `is_class` / `is Sprite2D` / `!= null` / `get_method_list` / `get_property_list` / `has_node` / `is_connected` / `src.contains(...)` / `FileAccess.file_exists` … |
| `C` | `C2-probe` | **我方插桩观测**：`int(x.call("...count")) == N`、`bool(x.call("gate_..."))`、`is_instance_valid(...)`、`arr.size() == N` —— 证"逻辑跑了/接上了"，**不含算法数值语义** |
| `C` | `C3-frozen` | 期望是**冻结字面常量**（`Vector2(250.0,150.0)`、`== 0.9`、`== 4`）且没有外部出处 —— **能防回归，不是复原证据**（`AGENTS.md` §0.2） |
| `G` | `G1-addr` | 断言（或其上下文）带反汇编 vaddr（`0x[0-9a-f]{5,}`）且期望是字面量 |
| `G` | `G2-corpus` | 带「冻结语料 / 真帧 / 真机 / 逐位 / unicorn / `fixtures/` / `bit_exact_*` / `path.tsv` / `eval.tsv` / `protocol.jsonl`」出处 |
| `?` | — | 机械判据都没命中 ⇒ **人工裁定**，结果写进 `OVERRIDES`（本文件内，逐条带理由） |

判据**顺序**：`M` → `G` → `C`（严的一侧优先）。理由：一条既像镜像又带地址注释的断言，
仍然"改错不会红"，所以还是 M。

## 允许保留的 M（`KEEP_M`）

契约类断言本来就该自洽（它测的是接线）。凡是 `M` 里判定为**纯接线/往返转发**、
且**没有语义可断言**的，在 TSV 的 `keep` 列标 `y` 并写明"契约断言，不含语义"——
它们留在代码里，但代码里也加了同一句注释。除 `KEEP_M` 之外的 M 都必须改造。

## 复跑

```bash
python3 tools/headless_audit.py                  # 统计 + 写 fixtures/headless_assertion_audit.tsv
python3 tools/headless_audit.py --self-test      # 正对照 3 条 + 反面对照 3 条，必须全过
python3 tools/headless_audit.py --list M         # 只列 M 档（逐条 文件:行 + 子码 + 理由）
python3 tools/headless_audit.py --list G --file entity_drawer_check
```

## 正对照（`--self-test`，不过就不许用这个工具）

1. `entity_drawer_check.gd` 的 `expect_sprite()` 那三条（`position` / `scale` / `flip_h`）必须判 **M**
   —— 实例 A 第 3 处；
2. `obstacle_drawer_check.gd` 的 `ClassDB.class_exists(OD)` 必须判 **C**；
3. `spike_class_check.gd` 的「枚举 vs 公式」必须判 **M**。

反面对照：把三条已知档位的断言形状各篡改一次，判据必须跟着变（证明规则不是恒真）。

## 我这条判法可能漏什么（诚实交代）

* **静态**判据：只看断言文本与局部数据流，**不看跑起来会不会红**。所以 `C3-frozen` 里
  仍然可能混着"抄错的常量"（恒真），本工具判的是"期望值来源"，不是"这条断言有没有用"。
* `R3-two-ours` 用正则识别"两边都不像字面量"，可能把"观测值 vs 一个有出处的字面量"
  误判成 M（偏严，宁可多报 M 不漏 M）。
* **不含语义的接线断言**（`C1`/`C2`）里也有真会红的（例如"方法被引擎真的调到了"），
  它们不是镜像但也不是复原证据 —— 本工具按 `C` 记，理由列里写清了是哪一种。
"""

from __future__ import annotations

import argparse
import csv
import os
import re
import sys
from dataclasses import dataclass

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
TOOLS = os.path.join(REPO, "client", "tools")

#: 14 个 headless 验收脚本（`docs/06` §93.8 的 for 循环就是这张表）
SCRIPTS = [
    "assemble_check", "base_classes_check", "curve_on_screen_check", "draw_classes_check",
    "entity_drawer_check", "function_drawer_check", "gdext_check", "obstacle_drawer_check",
    "plane_on_screen_check", "play_flow_check", "process_loop_check", "recovered_view_check",
    "signal_classes_check", "spike_class_check",
]

#: **本地把被测算法再写一遍**的 helper（可复算：`grep -nE '^func (_)?(expect|_expected|_point_at|_board_to_canvas|_team_mirror)' client/tools/*_check.gd`）
LOCAL_REIMPL: dict[str, list[str]] = {
    "entity_drawer_check": ["expect_sprite(", "expect_label_position(", "_team_mirror("],
    "curve_on_screen_check": ["_expected_revealed(", "_expected_active(", "_point_at("],
    "plane_on_screen_check": ["_expected_revealed(", "_expected_final_tick(", "_board_to_canvas("],
    "play_flow_check": ["_expected_batches(", "_expected_batches_raw("],
}

MIRROR_TEXT = ["同一条算式", "独立重算", "镜像", "两边算出来不一样", "GDScript 版（", "手算期望"]
ROUNDTRIP_TEXT = ["往返", "读回来", "设进去", "set/get"]

G_MARKERS = [
    "fixtures/", "bit_exact", "path.tsv", "eval.tsv", "protocol.jsonl", "真帧", "真机",
    "逐位", "冻结语料", "unicorn", "反汇编", "objdump",
]
ADDR_RE = re.compile(r"0x[0-9a-fA-F]{5,}")

C_SHAPE = [
    r"ClassDB\.class_exists", r"\.is_class\(", r"\.get_class\(", r"\bis Control\b",
    r"\bis Node\b", r"\bis Object\b", r"\bis Resource\b", r"\bis CanvasItem\b",
    r"!=\s*null\b", r"==\s*null\b", r"FileAccess\.file_exists", r"ResourceLoader\.exists",
    r"DirAccess", r"get_method_list\(", r"has_method\(", r"_has_method\(", r"names\.has\(",
    r"methods\.has\(", r"_missing\(", r"get_property_list\(", r"_has_property\(",
    r"get_child_count\(", r"has_node\(", r"get_node\(", r"find_child\(",
    r"is_inside_tree\(", r"get_signal_list\(", r"is_connected\(", r"\.has_signal\(",
    r"_signal_names\(", r"src\.contains\(", r"\.contains\(", r"\bPackedScene\b",
    r"\.instantiate\(", r"queue_free\(", r"\bfreed\b", r"get_children\(\)",
    r"\.resource_path\b", r"is_instance_valid\(", r"begins_with\(",
    r"\bis Texture2D\b", r"\bis Sprite2D\b", r"\bis RichTextLabel\b", r"\bis Line2D\b",
    r"\bis Label\b", r"\bis Button\b", r"\bis ColorRect\b", r"\bis SceneTree\b",
    r"\.get_class\(\)", r"property_names\(", r"_has_member\(", r"\.is_empty\(\)",
    r"_gd_files\(", r"_scan_client_sources\(", r"_scan_src_symbols\(",
    r"_arg_types\(", r"load\(", r"\bVariant\.", r"get_minimum_size\(\)",
    r"Engine\.has_singleton\(", r"is_visible_in_tree\(", r"\.has_point\(",
    r"_same_ids\(", r"_same_id_set\(", r"_ids_ascending\(", r"_has_kind",
]

OK_RE = re.compile(r"(?<![\w.])ok\s*\(")
STRING_RE = re.compile(r'"((?:[^"\\]|\\.)*)"')
NUM_LIT_RE = re.compile(r"^[-+]?\d+(?:\.\d+)?(?:e[-+]?\d+)?$")
ASSIGN_RE = re.compile(r"^\s*(?:var\s+)?([A-Za-z_]\w*)\s*(?::\s*[A-Za-z_]\w*(?:\[[^\]]*\])?)?\s*=(?!=)\s*(.*)$")


def _strip_strings(text: str) -> str:
    out, i, n = [], 0, len(text)
    while i < n:
        ch = text[i]
        if ch in "\"'":
            q = ch
            out.append(" ")
            i += 1
            while i < n:
                if text[i] == "\\":
                    out.append("  " if i + 1 < n else " ")
                    i += 2
                    continue
                if text[i] == q:
                    out.append(" ")
                    i += 1
                    break
                out.append("\n" if text[i] == "\n" else " ")
                i += 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def _balanced(text: str, start: int) -> int:
    depth, i, n = 0, start, len(text)
    while i < n:
        ch = text[i]
        if ch in "\"'":
            q = ch
            i += 1
            while i < n:
                if text[i] == "\\":
                    i += 2
                    continue
                if text[i] == q:
                    i += 1
                    break
                i += 1
            continue
        if ch == "#":
            while i < n and text[i] != "\n":
                i += 1
            continue
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def _first_arg(inner: str) -> str:
    depth, i, n = 0, 0, len(inner)
    while i < n:
        ch = inner[i]
        if ch in "\"'":
            q = ch
            i += 1
            while i < n:
                if inner[i] == "\\":
                    i += 2
                    continue
                if inner[i] == q:
                    i += 1
                    break
                i += 1
            continue
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        elif ch == "," and depth == 0:
            return inner[:i]
        i += 1
    return inner


@dataclass
class Site:
    script: str
    line: int
    cond: str
    raw: str
    context: str = ""
    tier: str = "?"
    sub: str = ""
    why: str = ""
    keep: str = ""          # "y" = 保留（契约断言，不含语义）

    @property
    def rel(self) -> str:
        return f"client/tools/{self.script}.gd:{self.line}"

    @property
    def msg(self) -> str:
        """`ok(cond, "消息")` 里**第二个实参**的字符串内容（消息，不是条件里的串）。"""
        inner = self.raw[self.raw.index("(") + 1:-1]
        first = _first_arg(inner)
        rest = inner[len(first) + 1:]
        m = STRING_RE.search(rest)
        return m.group(1) if m else ""


def _taint_map(src: str, script: str) -> dict[str, str]:
    """局部变量污染表：变量名 → 'live'（由本地重算 helper 赋值）。"""
    helpers = LOCAL_REIMPL.get(script, [])
    if not helpers:
        return {}
    taint: dict[str, str] = {}
    lines = _strip_strings(src).split("\n")
    for ln in lines:
        m = ASSIGN_RE.match(ln)
        if not m:
            continue
        name, rhs = m.group(1), m.group(2)
        if any(h in rhs for h in helpers):
            taint[name] = "live"
            continue
        # 传递：rhs 里出现**已经污染**的变量名（按词边界，防止 `s` 匹配一切）
        for v in taint:
            if re.search(rf"(?<![\w.]){re.escape(v)}(?![\w])", rhs):
                taint[name] = "live"
                break
    return taint


def parse_script(name: str) -> list[Site]:
    path = os.path.join(TOOLS, name + ".gd")
    with open(path, encoding="utf-8") as fh:
        src = fh.read()
    lines = src.split("\n")
    starts = [0]
    for ln in lines:
        starts.append(starts[-1] + len(ln) + 1)

    def line_of(off: int) -> int:
        lo, hi = 0, len(starts) - 1
        while lo < hi:
            mid = (lo + hi + 1) // 2
            if starts[mid] <= off:
                lo = mid
            else:
                hi = mid - 1
        return lo + 1

    scan = _strip_strings(src)
    sites: list[Site] = []
    for m in OK_RE.finditer(scan):
        if re.search(r"func\s+$", scan[max(0, m.start() - 12):m.start()]):
            continue
        end = _balanced(src, m.end() - 1)
        inner = src[m.end():end - 1]
        cond = _first_arg(inner).strip()
        ln = line_of(m.start())
        ctx = []
        j = ln - 2
        while j >= 0 and len(ctx) < 3:
            t = lines[j].strip()
            if t.startswith("#"):
                ctx.append(t)
                j -= 1
            else:
                break
        # 断言自身可能跨行，把语句尾部的行内注释也带上
        tail = lines[line_of(end - 1) - 1] if end - 1 < len(src) else ""
        sites.append(Site(name, ln, cond, src[m.start():end],
                          " ".join(reversed(ctx)) + " " + tail.strip()))
    return sites


DECL_NAME_RE = re.compile(r"^(want|expected|exp_)")
IDENT_RE = re.compile(r"(?<![\w.])([A-Za-z_]\w*)")
NOT_CONST_WORDS = {"true", "false", "null", "Vector2", "Vector2i", "Color", "Rect2",
                   "PackedStringArray", "PackedFloat64Array", "PackedInt32Array",
                   "PackedInt64Array", "Vector2Array", "Array", "int", "float", "bool",
                   "String", "size", "x", "y", "z", "is_equal_approx", "absf", "maxf",
                   "minf", "roundf", "floorf", "ceilf", "clampf", "PI", "TAU", "NAN",
                   "INF", "not", "and", "or", "in", "self", "get", "call", "is",
                   "ZERO", "ONE", "INF_NEG"}


def _is_declared_truth(side: str) -> bool:
    """期望侧是不是**已声明的真值**：字面量，或只用脚本 `const` / 字面量拼出来的表达式。

    全大写标识符 = 脚本里的 `const`（冻结的声明值）。只要一侧全是这种，它就不是
    "我方的运行期输出"，因此不构成镜像。
    """
    t = " ".join(side.strip().split())
    if not t:
        return False
    if t in ("true", "false"):
        return True
    if NUM_LIT_RE.match(t) or re.fullmatch(r"[-+0-9.eE]+", t):
        return True
    if DECL_NAME_RE.match(t):
        return True
    # 去掉字符串字面量后，剩下的每个标识符都必须是 const / 白名单词
    bare = re.sub(r'"[^"]*"', " ", t)
    for m in IDENT_RE.finditer(bare):
        w = m.group(1)
        if w in NOT_CONST_WORDS:
            continue
        if re.fullmatch(r"[A-Z][A-Z0-9_]*", w):
            continue
        if w.startswith(("Packed", "Vector", "Color", "Rect")):
            continue
        return False
    # 还要能"冻结"：不含函数调用（除 .size() 这种纯属性）与下标运算以外的运行期读
    if re.search(r"\.(call|get)\(|get_point|\.position|\.scale|\.texture", bare):
        return False
    return True


def _split_cmp(cond: str) -> tuple[str, str, str]:
    """把**单个比较子句**按比较运算符切开，返回 (lhs, op, rhs)。"""
    c = cond.strip()
    m = re.match(r"^(.*?)\.is_equal_approx\((.*)\)\s*$", c, re.S)
    if m:
        return m.group(1), "is_equal_approx", m.group(2)
    m = re.match(r"^\(?\s*(?:not\s+)?is_equal_approx\((.*)\)\s*$", c, re.S)
    if m:
        args = _split_args(m.group(1))
        if len(args) >= 2:
            return args[0], "is_equal_approx", args[1]
    m = re.match(r"^\(?\s*(?:not\s+)?_near\((.*)\)\s*$", c, re.S)
    if m:
        args = _split_args(m.group(1))
        if len(args) >= 2:
            return args[0], "is_equal_approx", args[1]
    depth = 0
    for i, ch in enumerate(c):
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        elif depth == 0:
            if c.startswith("==", i) and c[i - 1:i] not in "=!<>":
                return c[:i], "==", c[i + 2:]
            if c.startswith("!=", i) and c[i - 1:i] not in "=!<>":
                return c[:i], "!=", c[i + 2:]
            for op in ("<=", ">=", "<", ">"):
                if c.startswith(op, i) and c[i - 1:i] not in "=!<>":
                    return c[:i], op, c[i + len(op):]
    return "", "", c


def _split_args(inner: str) -> list[str]:
    out, buf, depth = [], [], 0
    i, n = 0, len(inner)
    while i < n:
        ch = inner[i]
        if ch in "\"'":
            q = ch
            buf.append(ch)
            i += 1
            while i < n:
                buf.append(inner[i])
                if inner[i] == "\\":
                    if i + 1 < n:
                        buf.append(inner[i + 1])
                    i += 2
                    continue
                if inner[i] == q:
                    i += 1
                    break
                i += 1
            continue
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            out.append("".join(buf).strip())
            buf = []
            i += 1
            continue
        buf.append(ch)
        i += 1
    out.append("".join(buf).strip())
    return out


def _clauses(flat: str) -> list[str]:
    """把断言条件按**顶层** and/or 切成分句（`not` 不切）。"""
    parts, buf, depth = [], [], 0
    i, n = 0, len(flat)
    while i < n:
        ch = flat[i]
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if depth == 0:
            m = re.match(r"\s+(and|or)\s+", flat[i:])
            if m:
                parts.append("".join(buf))
                buf = []
                i += m.end()
                continue
        buf.append(ch)
        i += 1
    parts.append("".join(buf))
    return [p.strip() for p in parts if p.strip()]


def _classify_clause(cl: str, site: Site, taint: dict[str, str],
                     blob: str) -> tuple[str, str, str] | None:
    helpers = LOCAL_REIMPL.get(site.script, [])
    for h in helpers:
        if h in cl:
            return "M", "R1-local-helper", f"直接调本地重算函数 `{h.rstrip('(')}`（与实现同一套算术）"
    for v in taint:
        if re.search(rf"(?<![\w.]){re.escape(v)}(?![\w])", cl):
            return "M", "R2-taint", (
                f"比较对象 `{v}` 是本地重算函数的返回值（`{v} := expect_*/_expected_*`）")
    if re.search(r"\b(by_prims|by_formula|by_enum|_by_formula)\b", cl) and "==" in cl:
        return "M", "R3-two-ours", "两个我方实现互相当 oracle（枚举 vs 公式）"

    lhs, op, rhs = _split_cmp(cl)
    if op in ("==", "is_equal_approx") and lhs.strip() and rhs.strip():
        lt, rt = _is_declared_truth(lhs), _is_declared_truth(rhs)
        if not lt and not rt:
            return "C", "C6-obs-consistency", (
                f"两个运行期观测值互相比（增量/一致性，`{cl[:48]}`）——可被真实错误打破，但不是外部真值")
        return "C", "C3-frozen", "期望侧是已声明的真值/冻结常量（能防回归，不是复原证据）"
    if re.search(r"==\s*\d+\s*\*", cl) and not _is_declared_truth(cl.split("==")[-1]):
        return "M", "R4-ratio", "期望是「我方计数的倍数」不变式（改错不会红）"
    if op in ("!=", "<", ">", "<=", ">="):
        return "C", "C2-probe", "单调性/不等观测，不含精确数值语义"
    return None


def classify(site: Site, taint: dict[str, str]) -> tuple[str, str, str]:
    cond = site.cond
    blob = site.raw + " " + site.context
    flat = " ".join(cond.split())

    # R5 往返 / R6 文本镜像：整条断言级别
    msg = site.msg
    if any(k in msg for k in ROUNDTRIP_TEXT) and re.search(r"\.(get|call)\(", flat):
        return "M", "R5-roundtrip", "往返断言：只证明「我设进去的值读回来相等」"
    for mk in MIRROR_TEXT:
        if mk in blob:
            return "M", "R6-mirror-text", f"断言或紧邻注释自己写明是镜像（`{mk}`）"

    verdicts: list[tuple[str, str, str]] = []
    for cl in _clauses(flat):
        r = _classify_clause(cl, site, taint, blob)
        if r is not None:
            verdicts.append(r)
    if any(v[0] == "M" for v in verdicts):
        return next(v for v in verdicts if v[0] == "M")

    # ---------------- G ----------------
    if ADDR_RE.search(blob):
        return "G", "G1-addr", "带反汇编 vaddr 出处（且期望是已声明真值）"
    if any(k in blob for k in G_MARKERS):
        return "G", "G2-corpus", "带冻结语料/真帧/真机/逐位出处"

    # ---------------- C ----------------
    for pat in C_SHAPE:
        if re.search(pat, cond):
            return "C", "C1-shape", f"接口形状/存在性/接线（`{pat[:26]}`），不含数值语义"
    if re.search(r"\.(call|get)\(", cond):
        return "C", "C2-probe", "我方插桩观测（计数/布尔/字符串），不含算法数值语义"
    if verdicts:
        return verdicts[0]
    if re.fullmatch(r"[A-Za-z_]\w*", flat) or re.fullmatch(r"not\s+[A-Za-z_]\w*", flat) \
            or re.fullmatch(r"bool\(.*\)", flat) or re.fullmatch(r"not\s+bool\(.*\)", flat):
        return "C", "C4-agg-bool", "聚合布尔变量/布尔观测（多处观测的与运算结果）"
    if re.search(r"\bis\s+[A-Z]\w+", flat):
        return "C", "C1-shape", "类型检查（`is <原生类>`），接口形状"
    if re.search(r"\.has\(", flat):
        return "C", "C1-shape", "集合/信号表成员性，接口形状"
    if re.search(r"\.size\(\)\s*(>=|>|<=|<)", flat):
        return "C", "C2-probe", "计数单调性观测，不含精确数值语义"
    if re.search(r"(>=|>|<=|<)\s*[-+0-9.]", flat):
        return "C", "C2-probe", "单调性/下界观测，不含精确数值语义"
    return "?", "", "机械判据都没命中 ⇒ 需人工裁定"


#: 人工裁定表：`文件名:行` → (tier, sub, 理由)
OVERRIDES: dict[str, tuple[str, str, str]] = {}

#: 允许保留的 M（纯接线/往返转发，无语义可断言）：`文件名:行` → 保留理由
KEEP_M: dict[str, str] = {}


def audit() -> list[Site]:
    out: list[Site] = []
    for name in SCRIPTS:
        src = open(os.path.join(TOOLS, name + ".gd"), encoding="utf-8").read()
        taint = _taint_map(src, name)
        for s in parse_script(name):
            key = f"{s.script}:{s.line}"
            if key in OVERRIDES:
                s.tier, s.sub, s.why = OVERRIDES[key]
            else:
                s.tier, s.sub, s.why = classify(s, taint)
            if s.tier == "M" and key in KEEP_M:
                s.keep = "y"
                s.why += "①保留：" + KEEP_M[key]
            out.append(s)
    return out


def self_test() -> int:
    bad = 0

    def find(sub, script=None):
        for name in SCRIPTS:
            if script and name != script:
                continue
            src = open(os.path.join(TOOLS, name + ".gd"), encoding="utf-8").read()
            taint = _taint_map(src, name)
            for s in parse_script(name):
                if sub in s.cond or sub in s.raw:
                    if f"{s.script}:{s.line}" in OVERRIDES:
                        t, _, _ = OVERRIDES[f"{s.script}:{s.line}"]
                        return s, t
                    t, _, _ = classify(s, taint)
                    return s, t
        return None, None

    def expect(sub, want, what, script=None):
        nonlocal bad
        s, got = find(sub, script)
        if s is None:
            print(f"[self-test] FAIL 找不到断言：{what}")
            bad += 1
            return
        okk = got == want
        bad += 0 if okk else 1
        print(f"[self-test] {'ok  ' if okk else 'FAIL'} {what} → 判 {got}（应 {want}）  [{s.rel}]")

    expect('want["position"]', "M", "正对照① 实例 A 第 3 处：expect_sprite 的 position 镜像",
           "entity_drawer_check")
    expect('want["scale"]', "M", "正对照①b 实例 A 第 3 处：scale 分量镜像",
           "entity_drawer_check")
    expect("ClassDB.class_exists(OD)", "C", "正对照② 纯契约：类存在", "obstacle_drawer_check")
    expect("by_prims == expected[mode]", "M", "正对照③ 两个我方实现互证", "spike_class_check")

    # 正对照④：**历史形状复现**（与现网行解耦）—— 把 `2f64674^` 里那两行的形状写死在这里，
    # 任何时候跑 `--self-test` 都必须判 M。它证明"这个工具当年会把实例 A 判成镜像"。
    for hist_cond, hist_msg in [
        ('kids[0].scale.is_equal_approx(want["scale"])', "sprite.scale"),
        ('kids[0].position.is_equal_approx(want["position"])', "sprite.position"),
        ('label.position.is_equal_approx(want_label)', "name label position"),
    ]:
        hist = Site("entity_drawer_check", 155, hist_cond,
                    f'ok({hist_cond}, "{hist_msg}")')
        th, subh, _ = classify(hist, {"want": "live", "want_label": "live"})
        okk = th == "M"
        bad += 0 if okk else 1
        print(f"[self-test] {'ok  ' if okk else 'FAIL'} 正对照④ 实例 A 历史形状"
              f"`{hist_cond[:40]}` → 判 {th}/{subh}（应 M）")

    # 反面对照：篡改形状，判据必须跟着变
    for i, (site, want, why) in enumerate([
        (Site("x", 1, 'kids[0].position.is_equal_approx(want["position"])',
              'ok(kids[0].position.is_equal_approx(want["position"]), "x")'),
         "M", "反面对照① 镜像形状"),
        (Site("x", 1, "int(drawer.get(\"team_color\")) == TEAM_COLOR",
              'ok(int(drawer.get("team_color")) == TEAM_COLOR, "属性 team_color 往返一致")'),
         "M", "反面对照② 往返形状"),
        (Site("x", 1, 'ClassDB.class_exists("Foo")', 'ok(ClassDB.class_exists("Foo"), "类")'),
         "C", "反面对照③ 契约形状"),
    ], 1):
        t, _, _ = classify(site, {"want": "live"})
        okk = t == want
        bad += 0 if okk else 1
        print(f"[self-test] {'ok  ' if okk else 'FAIL'} {why} → 判 {t}（应 {want}）")

    n = sum(1 for _ in [1])
    print(f"[self-test] {'PASS' if bad == 0 else f'{bad} 条不过'}")
    return 0 if bad == 0 else 1


FIX_NOTE: dict[str, str] = {}      # `文件名:行` → 这一条本轮的修法（commit 2 填）
FIX_RESULT: dict[str, str] = {}    # `文件名:行` → 本轮结果（after 档位 / 反面对照变红）


def write_tsv(sites: list[Site], out: str) -> None:
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    n = {t: sum(1 for x in sites if x.tier == t) for t in ("M", "C", "G", "?")}
    with open(out, "w", newline="", encoding="utf-8") as fh:
        w = csv.writer(fh, delimiter="\t", lineterminator="\n")
        fh.write(
            "# fixtures/headless_assertion_audit.tsv —— `client/tools/*_check.gd` 逐条断言的**期望值来源**审计\n"
            "# 生成：`python3 tools/headless_audit.py --out fixtures/headless_assertion_audit.tsv`\n"
            "# 判据写在 `tools/headless_audit.py` 头部（可复算）；正/反面对照：`--self-test`\n"
            f"# 本表 {len(sites)} 行 = 14 个脚本里 `ok(...)` 的**静态调用点**（运行期 passed 数更多，因为有循环）。\n"
            f"# 分布：M-镜像 {n['M']} / C-契约 {n['C']} / G-真值 {n['G']} / ?-未定 {n['?']}\n"
            "# tier  M=期望值由被测代码同一套算术算出（改错不会红）\n"
            "#       C=契约/接线/插桩计数/冻结字面常量（能防回归，不是复原证据）\n"
            "#       G=期望值有外部出处（反汇编 vaddr / 冻结语料 / 真帧像素 / 已入库 fixtures）\n"
            "# sub   见工具头部 R1..R6 / C1..C6 / G1..G2 的判据表\n"
            "# kept  这条 M 是否**允许保留**（纯接线，注释里要写'契约断言，不含语义'）\n"
            "# fix   本轮对这一条的修法（只对 M 档填）\n"
            "# result 本轮结果（after 档位 / 反面对照是否变红）\n"
        )
        w.writerow(["script", "line", "tier", "sub", "kept", "reason", "condition",
                    "message", "fix", "result"])
        for s in sites:
            key = f"{s.script}:{s.line}"
            w.writerow([s.script, s.line, s.tier, s.sub, s.keep, s.why,
                        " ".join(s.cond.split()), s.msg.replace("\n", " "),
                        FIX_NOTE.get(key, "") if s.tier == "M" else "",
                        FIX_RESULT.get(key, "")])


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--list", metavar="TIER")
    ap.add_argument("--sub", metavar="SUB")
    ap.add_argument("--file")
    ap.add_argument("--out", default=os.path.join(REPO, "fixtures", "headless_assertion_audit.tsv"))
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    sites = audit()
    if args.file:
        sites = [s for s in sites if s.script == args.file]

    if args.list:
        for s in sites:
            if s.tier == args.list and (not args.sub or args.sub in s.sub):
                print(f"{s.rel}\t{s.sub}\t{s.why}\t{s.cond[:100]}")
        return 0

    per: dict[str, dict[str, int]] = {}
    for s in sites:
        d = per.setdefault(s.script, {"M": 0, "C": 0, "G": 0, "?": 0})
        d[s.tier] = d.get(s.tier, 0) + 1
    print(f"{'script':26s} {'all':>5s} {'M':>4s} {'C':>4s} {'G':>4s} {'?':>4s}")
    for name in SCRIPTS:
        if name in per:
            d = per[name]
            print(f"{name:26s} {sum(d.values()):5d} {d['M']:4d} {d['C']:4d} {d['G']:4d} {d['?']:4d}")
    tot = {t: sum(d.get(t, 0) for d in per.values()) for t in ("M", "C", "G", "?")}
    print(f"{'TOTAL':26s} {sum(tot.values()):5d} {tot['M']:4d} {tot['C']:4d} {tot['G']:4d} {tot['?']:4d}")
    subs: dict[str, int] = {}
    for s in sites:
        subs[s.sub or "(none)"] = subs.get(s.sub or "(none)", 0) + 1
    print("\n子码分布：")
    for k in sorted(subs, key=lambda x: -subs[x]):
        print(f"  {k:20s} {subs[k]:5d}")
    if args.out:
        write_tsv(sites, args.out)
        print(f"\n写出 {args.out}（{len(sites)} 行）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
