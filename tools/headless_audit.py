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
TOOLS = os.environ.get("GW2_AUDIT_TOOLS") or os.path.join(REPO, "client", "tools")
#: 想审计**历史版本**（例如修复前的 `238cc1c`）就把这 14 份脚本 checkout 到别的目录，再
#: `GW2_AUDIT_TOOLS=/path/to/those/gd python3 tools/headless_audit.py`。
#: 本轮修复前的读数就是这么量的：`git show 238cc1c:client/tools/<f>.gd` 写进临时目录。

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
    # 注意：`_point_at()` **不**在内 —— 它是 `_make_points()`（**测试自己喂进去的输入**）的
    # 生成式副本，断言 `line.point[k] == 输入[k]` 是"回声测试"，不是镜像（见 tools 头部口径）。
    "curve_on_screen_check": ["_expected_revealed(", "_expected_active("],
    "plane_on_screen_check": ["_expected_revealed(", "_expected_final_tick("],
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
ASSIGN_RE = re.compile(r"^\s*(?:var\s+)?([A-Za-z_]\w*)\s*(?::\s*[A-Za-z_]\w*(?:\[[^\]]*\])?)?\s*:?=(?!=)\s*(.*)$")


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


def _mask(src: str) -> str:
    """把**注释**与字符串内容换成空白（保留换行与长度），用于定位 `ok(...)` 调用点。

    ⚠️ 必须同时屏蔽注释：本轮的"取证注释"里会写出 `ok(_expected_active(73,10,100))`
    这种**历史写法**，不屏蔽就会把它们当成现存断言（2026-11-26 实测：会虚增 20 余条）。
    """
    out, i, n = [], 0, len(src)
    while i < n:
        ch = src[i]
        if ch == "#":
            while i < n and src[i] != "\n":
                out.append(" ")
                i += 1
            continue
        if ch in "\"'":
            q = ch
            out.append(" ")
            i += 1
            while i < n:
                if src[i] == "\\":
                    out.append(" " if i + 1 < n else " ")
                    out.append(" ")
                    i += 2
                    continue
                if src[i] == q:
                    out.append(" ")
                    i += 1
                    break
                out.append("\n" if src[i] == "\n" else " ")
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

    scan = _mask(src)
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
    if re.search(r"\bby_prims\b", cl) and re.search(r"\bby_formula\b", cl) and "==" in cl:
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
    if "T50" not in blob and "不是镜像" not in blob:
        for mk in MIRROR_TEXT:
            if mk in blob:
                return "M", "R6-mirror-text", f"断言或紧邻注释自己写明是镜像（`{mk}`）"

    # R3（整条级）：枚举与公式**同时**出现在一条断言里 ⇒ 互为 oracle
    if re.search(r"\bby_prims\b", flat) and re.search(r"\bby_formula\b", flat) and "==" in flat:
        return "M", "R3-two-ours", "两个我方实现互相当 oracle（枚举 vs 公式在同一断言里）"

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


#: 人工裁定表。键 = `(脚本名, 条件里的唯一子串)` —— **不用行号**，改了上方注释也不会失配。
#: 值 = (tier, sub, 理由)。理由必须写清「为什么机械判据不对、人判成什么」。
#: 全部 22 条都是本轮 T50 改完之后仍然落到 M 的，逐条有理由（见 `docs/06` §100 的表）。
OVERRIDES: dict[tuple[str, str], tuple[str, str, str]] = {
    ("base_classes_check", 'root_node.get("ads_slot")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("base_classes_check", 'vbox.get("show_global")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("base_classes_check", 'ad.get("owned")) == false'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("draw_classes_check", 'drawer.get("flip_y")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("entity_drawer_check", 'drawer.get("team_color")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("entity_drawer_check", 'drawer.get("texture_size")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("gdext_check", 'node.get("axis_mode")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("plane_on_screen_check", 'plane.get("board_scale")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("plane_on_screen_check", 'plane.get("flip_y")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("signal_classes_check", 'btn.get("input_text")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("signal_classes_check", 'btn.get("long_press_input_text")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("signal_classes_check", 'inp.get("caret_condition")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("spike_class_check", 'inst.get("axis_mode")'): ("C", "C1-shape", '`@export` 属性 set/get 接线（原判 R5 往返）：源码里已加「契约断言，不含语义」注释，同组的语义断言在相邻行/本节后段'),
    ("curve_on_screen_check", 'int(plane.call("feed_functions")) == 60'): ("C", "C3-frozen", '冻结真值 60（30·(12−10)，`0x4b67a0`）；R6 是被上方 T50 注释里的「镜像函数」字样误触发的'),
    ("curve_on_screen_check", 'int(drawer.call("point_count", 3)) == 100'): ("C", "C3-frozen", '冻结真值 100/60（`0x4b67a0` 的两条分支）；R6 误触发同上'),
    ("entity_drawer_check", 'sp0.position.is_equal_approx(Vector2(250.0, 110.0))'): ("C", "C3-frozen", '冻结真值 (250,110)；R6 被注释里的「镜像」误触发'),
    ("plane_on_screen_check", 'int(drawer.call("written", player_id)) == 5314'): ("C", "C3-frozen", '冻结真值 5314；R6 误触发同上'),
    ("play_flow_check", 'src.contains("SOLDIER_HIT_RADIUS, ring_v")'): ("C", "C1-shape", '源码文本接线检查（不是运行期数值）；R6 是被上方**原有**注释里的「镜像」误触发的'),
}

#: 允许**保留**的 M：`(脚本名, 条件子串)` → 保留理由。只用于「纯接线/差分对照、
#: 无外部真值可换」的少数几条；源码注释里必须写明它保什么、不保什么。
KEEP_M: dict[tuple[str, str], str] = {
    ("entity_drawer_check", 'label.position.is_equal_approx(want_label)'): '契约断言，不含语义：只验「布局被写下」；语义断言是紧接着的两条几何不变量（标签水平居中在精灵上 / 垂直在其上方）',
    ("play_flow_check", '_batch_bounds(got_batches) == _batch_bounds(want_batches)'): '**差分对照**：Rust 驱动 vs 本脚本独立扫描（两份实现、两份语言）。抓单边错误有效，但没有外部真值可换；真正的冻结真值断言就在它上一行 `== [[10,20,60],…]`',
    ("play_flow_check", '_batch_bounds(rb) == _batch_bounds(wb)'): '**差分对照**（同上，跑在**冻结回放语料**上）。边界无外部真值表可对，故保留差分并注明；语料本身登记在 `fixtures/guardrails.json` 的 corpus sha256',
}

def audit() -> list[Site]:
    out: list[Site] = []
    for name in SCRIPTS:
        src = open(os.path.join(TOOLS, name + ".gd"), encoding="utf-8").read()
        taint = _taint_map(src, name)
        for s in parse_script(name):
            s.tier, s.sub, s.why = classify(s, taint)
            flat = " ".join(s.cond.split())
            for (sc, sub), val in OVERRIDES.items():
                if sc == s.script and sub in flat:
                    s.tier, s.sub = val[0], val[1]
                    s.why = "人工裁定：" + val[2]
                    break
            for (sc, sub), why in KEEP_M.items():
                if sc == s.script and sub in flat:
                    s.keep = "y"
                    s.why = "人工裁定：**保留**（" + why + "）"
                    break
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

    # 正对照①（现网）：`want_label` 那条镜像在 T50 之后被**降级为契约断言**（仍判 M，
    # 但 `kept=y`，源码注释写明「契约断言，不含语义」）—— 它证明"工具能认出仍在使用的镜像"。
    expect('want_label', "M", "正对照① 现网仍存在的镜像契约断言（降级保留）",
           "entity_drawer_check")
    expect("ClassDB.class_exists(OD)", "C", "正对照② 纯契约：类存在", "obstacle_drawer_check")
    two = Site("spike_class_check", 76,
               "by_prims == expected[mode] and by_formula == expected[mode]",
               'ok(by_prims == expected[mode] and by_formula == expected[mode], "x")')
    t2, sub2, _ = classify(two, {})
    okk = t2 == "M"
    bad += 0 if okk else 1
    print(f"[self-test] {'ok  ' if okk else 'FAIL'} 正对照③ 两个我方实现互证（历史形状）"
          f"→ 判 {t2}/{sub2}（应 M）")
    # 现网已拆成两条各自对冻结真值表 ⇒ 必须判 C（不再互为 oracle）
    expect("by_prims == expected[mode]", "C", "正对照③b 拆分后只对冻结真值表 ⇒ 判 C",
           "spike_class_check")

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


PREFIX_NOTES = '# ─────────────────────────────────────────────────────────────────────────────\n# §A 修复前的 M 档逐条（27 条）：镜像是谁 / 改成什么 / 新真值从哪来 / 结果\n#    口径：拿**本判据**跑修复前那 14 份脚本（commit 238cc1c）量出来的。\n#    ⚠️ commit 238cc1c 自带的表报的是 32 条 —— 那一版判据有两处解析 bug（注释没屏蔽、\n#    `:=` 认不出），本节 27 条是修好 bug 后同一份代码的复算值（见 docs/06 §100.3）。\n# 文件:行(修复前) | 镜像的是谁的算术 | 改成什么 | 真值从哪里来 | 结果\nentity_drawer_check:343 | src/entity_drawer.rs::sprite_transform 的 position 分量（expect_sprite 逐字副本） | 字面量 Vector2(300,100) | 0x4b9530：size×0.5 ± k·x（k=10,x=5,y=5） | 绿\nentity_drawer_check:345 | 同上 的 scale 分量（实例 A 第 3 处，漏 k 的就是它） | 3 条屏幕像素真值：tex×|scale|=21.333px±0.05 / 宽==高 / scale.x==−scale.y | 闭式 128·k/60（0x4b9530 尾 mulss）+真帧 54.6px（docs/06 §59.8）+fixtures/board_geometry_equiv.tsv R_w=1279 | 绿；反面对照A 变3红\nentity_drawer_check:366 | name_label_layout（expect_label_position 逐字副本） | 降级为契约断言（注释标明）+2 条几何不变量（水平居中于精灵/垂直在其上方） | 布局意图（与具体算式无关） | 绿 +2 条\nentity_drawer_check:385 | sprite_transform position | 字面量 Vector2(200,115)+屏幕高度 21.333px | 0x4b9530（k=10,x=−5,y=3.5） | 绿 +1 条\nentity_drawer_check:551 | sprite_transform position | 字面量 Vector2(600,200)+k 翻倍⇒屏幕尺寸翻倍 42.667px | 0x4b9530（k=20）+线性性 | 绿 +1 条\nentity_drawer_check:593 | sprite_transform scale（tex_w=512 那层） | 字面量 |scale|=0.0416667 | (10/60)·(128/512) | 绿\nentity_drawer_check:595 | sprite_transform scale（tex_w=128 那层） | 字面量 |scale|=0.1666667 | (10/60)·(128/128) | 绿\nentity_drawer_check:598 | 两层 scale 差异（两我方观测互比） | 两层在屏幕上一样大=21.333px（tex×|scale| 与 tex_w 无关） | 0x4b9745「unit 各读自己的 tex_w」 | 绿 +1 条\nentity_drawer_check:808 | sprite_transform position（位置表那条路） | 字面量 Vector2(250,110) | 0x4b9530（表坐标 (0,4), k=10） | 绿\ncurve_on_screen_check:182 | 0x4b67a0 的 min(len,trunc(30·(frac−active)))（_expected_revealed） | 实现侧冻结真值：feed_functions==60 + function_point_total==60 | 0x4b67a0（30·(12−10)） | 绿；反面对照B 变红\ncurve_on_screen_check:204 | 同上 | 实现侧「tick setter 自己不给点」=90 | 0x4b67a0 + 调用时序 | 绿\ncurve_on_screen_check:215 | 同上 | 同上（14.0 时仍是 90） | 同上 | 绿\ncurve_on_screen_check:235 | 0x3be5f6 的 active_tick<tick && 30d<len+1800（_expected_active） | 问实现：function_count==1（d=63 ⇒ 1890<1900） | 0x3be5f6（严格小于） | 绿 +1 条\ncurve_on_screen_check:241 | 同上 | 真语义：active_ids 空 但 function_count==1（判出界≠真删） | 0x3be5f6 + 0x4b6418（删除推迟到喂点） | 绿 +1 条\ncurve_on_screen_check:286 | _expected_revealed 两处 | 冻结真值 point_count(3)==100 / point_count(8)==60 | 0x4b67a0（封顶 100 / 60） | 绿\ncurve_on_screen_check:337 | _expected_revealed(11.0,10,100) | 冻结真值 30（顺带验证幂等） | 0x4b67a0（30·(11−10)） | 绿\nplane_on_screen_check:224 | last_transform.x/y/s ≈ 自己读回来的 plane_size（镜像） | 与 BOARD_PIXELS（.tscn 契约常量）与字面量 20.0 比 | 500/50 + 0x1b9fc8 的 50.0 | 绿\nplane_on_screen_check:243 | _expected_revealed | 实现侧冻结真值 30 | 0x4b67a0（30·(1−0)） | 绿\nplane_on_screen_check:244 | 同上 | 同上 | 同上 | 绿\nplane_on_screen_check:246 | 同上 | 同上 | 同上 | 绿\nplane_on_screen_check:253 | 同上 | function_point_total==30 | 同上 | 绿\nplane_on_screen_check:265 | 同上（逐 tick 循环） | 冻结真值表 {2:60, 3:90, 10:300, 60:1800, 177:5310} | 0x4b67a0 + total=5314 | 绿\nplane_on_screen_check:270 | _expected_revealed(177.0,…) | 冻结真值 5310 | 0x4b67a0（30·177） | 绿\nplane_on_screen_check:309 | ceil(total/30)+active（_expected_final_tick） | 冻结真值 178.0 | ceil(5314/30) | 绿\nplane_on_screen_check:316 | _expected_revealed(final_tick,…) | 实现侧：written==5314 且 function_point_total==5314 | 冻结真值（场景 total_points） | 绿\nspike_class_check:66 | AxisDrawer.axis_mode（属性往返） | 契约断言注释 + 语义在相邻行（102 条线） | — | 绿\nspike_class_check:76 | 两个我方实现互证（by_prims vs by_formula） | 拆成两条，各自只对冻结真值表 {0:0,1:2,2:20,3:102} | docs/16 §21 的分支结构（0x4e0775…） | 绿 +4 条；反面对照C 变4红\n#    另有 13 条**属性接线**（R5 往返）降级为契约断言并在源码里加「契约断言，不含语义」注释：\n#      base_classes_check:159/161/164/170、draw_classes_check:119、entity_drawer_check:239/241、\n#      gdext_check:136、plane_on_screen_check:163/165、signal_classes_check:129/131/251\n#    其中 entity_drawer_check:239（team_color）另补了一条语义断言：\n#      「改 team_color ⇒ 画出来的图元颜色跟着变」（last_arc() 的 [8] 分量）\n#\n# §B 保留的 3 条 M（kept=y，源码注释里写清保什么、不保什么）\n# 文件:行(修复后) | 为什么保留\nentity_drawer_check:449 | 只验「布局被写下」；语义在紧接的两条几何不变量上\nplay_flow_check:753   | 差分对照（Rust 驱动 vs 本脚本独立扫描）：无外部真值可换；冻结真值断言在它上一行 == [[10,20,60],[60,70,100],[100,110,120]]\nplay_flow_check:887   | 差分对照（同上，跑在冻结回放语料上）；边界的真值表本轮没做（遗留）\n'

FIX_NOTE: dict[str, str] = {}      # `文件名:行` → 这一条本轮的修法
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
        fh.write(PREFIX_NOTES)


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
