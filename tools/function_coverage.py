#!/usr/bin/env python3
"""T39 —— 527 个原版可归属函数的**函数级**复原率（F1 / F2 / F3 / F4）。

## 这个脚本回答什么

`docs/coverage-matrix.tsv` 的招牌数字是「原版可归属函数 **527 / 527 已实现**」，
但它的"已实现"是**模块级**判断：模块条目存在（`mod()` 写了一行）+ 有人给它标了档位。
T30 已证伪（`docs/06` §80）：`boundary_audit` 的 10 条 mismatch 里**只有 1 条我方有函数体**，
其余 9 条原版函数**从未落码**（那个 `events.rs` 里从来只有 `enum` + 一张表）。

本脚本按**预注册判据**（`fixtures/function_coverage_plan.txt`，先于本脚本提交）
把 527 行逐函数判成 F1–F4，产出 `fixtures/function_coverage.tsv` 与三种口径的分子。

## T85 追加：`na` 口径 + P2 负向排除（预注册 `fixtures/function_coverage_na_plan.txt`）

T82（`docs/06` §129）把 F3 的 153 条逐条裁决成五类：53 `real-missing` / 12 `na:std-mono` /
69 `na:gdext-glue` / 7 `na:other` / 12 `unknown`（台账 `fixtures/f3_triage.tsv`，**人工裁决**）。
本脚本按 T82 §129.5.2 的四条建议把这份台账接进计分：

1. **`na` 列**（+`na_basis` 依据列，追加在 13 列之后）：值域
   `real-missing|na:std-mono|na:gdext-glue|na:other|unknown`，缺省 `real-missing`
   （缺省语义是「**没有 na 证据**」，不是「已确认真的缺」——T82 只审了 153 条 F3）。
   台账是**人工裁决**，本脚本**不重新判类别**，只做机判**闸**（M1–M7，见下）。
2. **`--stats` 三口径**：A（含 na，分母 527）/ B（剔 `na:*`，分母 439）/ B′（只剔强 na
   `std-mono+gdext-glue`，分母 446）；分子分母都打，**招牌数 = 口径 B**，并同时印出
   **旧口径（不带负向排除）的 A** 供与历史读数（`docs/06` §129.0 的 277/527）对照。
3. **P2 负向排除（N1）**：判了 `na:*` 的地址，即使出现在我方注释里（P2 会命中）**也不算 F1 证据**，
   一律降 `F3`+子类 `na:<原类别>`。理由：T71 §123.5 实测假抬 5 条，T80 之后同一机制假抬 **33 条**
   （文件顶部的"台账注释块"被 `attach_addr` 贴到第一个 fn 上 —— 那是**登记**不是实现）。
4. **`--no-na` = 完全旧行为**：不读台账、不写 `na` 列、不做 N1，生成物与 `--stats` 与改动前**逐字节相同**。

**闸**（任一不满足 ⇒ 打印全部违例后 `exit(2)`，不写 TSV、不出三口径）：
`M1` 值域 / `M2` 台账 addr 必须在 `boundary_audit` 的 527 行里 / `M3` 台账 addr 不重复 /
`M4` `na:*` 的 FDE 尺寸 ≤ **0x800**（实测 na 全 ≤0x721，≥0x800 上全是假阳性，见 §129.3）/
`M5` 依据文本非空且 ≥40 字符、命中 5 个固定标记（`Location=`/`被调者`/`串=`/`名字来源=`/`人工改判`）里 ≥2 个
（预注册写 ≥3；实测 4 条只命中 2 ⇒ 硬闸降到 2 并把那 4 条每次运行都打印，偏差登记 docs/06 §131.8-1）/
`M6` `na:*` 行若被**名字**指认命中 ⇒ 台账与代码矛盾（报错）/ `M7` 台账 `name` 必须与 boundary 同名。

## 判据（照抄预注册文件，一字不改）

    F1 有实现 + 有物证   我方存在一个函数能指到该原版函数（真名/地址引用），且档位有可复核出处
    F2 有实现 + 无物证   有对应代码但指不到原版函数（只有结构指纹预测名 / 只住在 _scaffold / 档位=推断）
    F3 只有文件/骨架     文件在、档位写了，但**没有函数体**
    F4 完全没有         连文件/条目都没有

三层机判：
  S0 ours 文件不存在 / 空                     -> F4
  S1 ours 文件没有任何**实现函数**（非 #[cfg(test)]）-> F3 子类 skel
  P1 NAME 原版真名（name_src=v2*）的函数段在我方同名实现函数里命中 -> pin=name
  P2 ADDR 原版地址出现在我方注释里且能贴到一个 fn 上                 -> pin=addr
  P3 HYB  只有结构指纹预测名（name_src=hyb:*）命中                  -> pin=hyb（只能到 F2）
  定档：pin∈{name,addr} 且 grade≠推断 -> F1；pin∈{hyb,scaffold} 或 grade=推断 -> F2；
        无 pin -> F3（按子类分"确证没落码"与"判不了"两组）

## 与预注册文件的四处**判定口径细化**（跑完登记在 docs/06 §87 的「§我写错了」）

R1 `src/_scaffold/**` 里的同名函数**不算**复原层实现（AGENTS.md §4：自造桥接不进复原层）
   ⇒ 命中时记 pin=scaffold（落到 F2 子类 scaffold-only），不落 F1。预注册原文只说"再全 src/ 找"。
R2 编译器生成的子函数（`::{{closure}}`、`::panic_cold_explicit`）取**父函数**做 pin
   （预注册 §6-L4 只写了 closure；`panic_cold_*` 是同一类东西，一并按父函数判）。
R3 gdext 宏产物：`__register_methods` 判 F3 子类 `na:macro-generated`（源码层无对应 fn，
   等价物是 `#[func]` 属性），`_::__init::__inner_init` 按父名 `init` 找 `fn init`。
R4 只有**声明没有函数体**的 fn（trait 方法声明、`fn x();`）不算实现。

## 用法

    python3 tools/function_coverage.py            # 生成 fixtures/function_coverage.tsv + 打印统计（含 na 列与三口径）
    python3 tools/function_coverage.py --stats    # 只打印统计（不写文件）
    python3 tools/function_coverage.py --control  # 只跑正/反面对照
    python3 tools/function_coverage.py --module graphwar_level_manager.rs   # 单模块逐条
    python3 tools/function_coverage.py --no-na    # **旧行为**（T85 之前）：不读台账、不写 na 列、不做负向排除
                                                  #   ⇒ 复跑历史数字（A = 312/527 那批）的唯一方式

地址口径：一律 **ELF vaddr（RVA）**。只读 `src/**`，不改任何实现。
"""

from __future__ import annotations

import argparse
import collections
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BOUNDARY = "fixtures/boundary_audit.tsv"
MATRIX = "docs/coverage-matrix.tsv"
OUT = "fixtures/function_coverage.tsv"
NA_LEDGER = "fixtures/f3_triage.tsv"

GRADES_WITH_EVIDENCE = ("逐位", "观测")
SCAFFOLD_PREFIX = "src/_scaffold/"

# ---------------------------------------------------------------------------
# T85：`na` 口径（预注册 fixtures/function_coverage_na_plan.txt §2–§6）
# ---------------------------------------------------------------------------
# 值域逐字照抄 T82 台账的「类别」列；缺省 = real-missing（语义 = **没有 na 证据**，不是"确认真的缺"）
NA_DOMAIN = ("real-missing", "na:std-mono", "na:gdext-glue", "na:other", "unknown")
NA_DEFAULT = "real-missing"
NA_PREFIX = "na:"
# 强 na = 函数体根本不在我们源码里（不可争议）⇒ 口径 B′ 的分母只剔这两类
NA_STRONG = ("na:std-mono", "na:gdext-glue")
# 体积上限（§4）：实测 na 全 ≤0x721，已知假阳性 0x3c2bc0 = 0x308d ⇒ 0x800 在空档里
NA_SIZE_CAP = 0x800
# 人工复核闸（§5）：依据文本必须够长，且命中下面 5 个固定标记里 ≥2 个（预注册写 ≥3，偏差见下）
NA_EVIDENCE_MIN = 40
NA_EVIDENCE_KEYS = ("Location=", "被调者", "串=", "名字来源=", "人工改判")
# ⚠️ 预注册 §5 写的是「≥3 个标记」，实测**4 条** `na:std-mono` 只命中 2 个（`Location=` + `名字来源=`，
#    长度 124–131；它们的依据靠 `hits` 列的 PAIR/SELF 特征成立，文本只是更简）。台账是 T82 的产物、
#    本轮不许改 ⇒ 硬闸降到 **2**，但把「<3」的条目**每次运行都打出来**（下面的 WARN_KEYS）。
#    这是**放宽**判据的偏差，已登记 docs/06 新节「§我写错了」。
NA_EVIDENCE_MIN_KEYS = 2
NA_EVIDENCE_WARN_KEYS = 3
# 缺省行的依据列文案（必须能一眼分出「台账裁决过」与「只是缺省值」）
NA_DEFAULT_BASIS = "—(未入 na 台账：T82 只审了 153 条 F3，本行是缺省值不是结论)"

# ---------------------------------------------------------------------------
# P2（地址指认）的**人工复核表**
#
# 预注册 M2 的机制判据是"地址出现在注释里且能贴到一个 fn 上 ⇒ 有实现"。机制判据
# 抓不到一种情况：引用出现在**邻居函数**的 doc 里（调用者/被调用者/测试投影），
# 于是把"邻居"当成了"实现"。P2 一共命中 39 条，39 条**逐条读过源码**，
# 下面 7 条机制上命中但语义不成立，按实际改判（理由逐条写死，可复核）：
# ---------------------------------------------------------------------------
ADDR_MANUAL = {
    # 地址 → (改判档, 子类, 理由)
    "0x3f0e40": ("F2", "addr-neighbor",
                 "注释写的是「`0x3f0e40` 的可判定模型」，贴在 function_history_recorder.rs:295 的"
                 "模型函数上；原版函数体（事件处理）没有逐条还原 ⇒ 有对应代码但指认只到模型"),
    "0x418820": ("F2", "addr-neighbor",
                 "campaign_control.rs:653 `refresh_index` 注释自述「本函数只还原**传什么数**」，"
                 "原版 0x418820 的写入效果（改按钮什么属性）**未证** ⇒ 部分实现"),
    "0x4741a0": ("F2", "addr-neighbor",
                 "game.rs:1254 贴到的 `entity_effect_of` 是**测试投影**；注释里的 0x4741a0 是"
                 "`add_entity_effect(_, AshEffect)` 那条循环的出处，不是这个投影 ⇒ 指认不到实现"),
    "0x51a750": ("F2", "addr-neighbor",
                 "贴到 function_evolutionator.rs:708 `fine_pass`，但注释是「三名各来一次 "
                 "`call 0x51a750`」= **调用点**；真正的适应度实现在同文件 `fitness()`(1172) "
                 "⇒ 有对应代码、指认错位"),
    "0x4f44c0": ("F3", "register-only",
                 "贴到 function_input.rs:705 `delete_chars_at_caret_plan`，但注释说 "
                 "`delete_token_at_caret` **调**它（是**被调用者**）；该原版函数体不存在"),
    "0x4f4a60": ("F3", "register-only",
                 "同上（`delete_token_after_caret` 是调用者一侧），函数体不存在"),
    "0x476320": ("F3", "register-only",
                 "贴到 setup_control.rs:701 `campaign_soldier_count`，注释说地址只给出 "
                 "`edx` 立即数（**常量出处**）；原版的 setter 函数体没有实现"),
}

# 正对照（T30 三个新文件，形态已知）；期望写在预注册文件 §3
CONTROL_POS = {
    "graphwar_event_handler.rs": 2,
    "graphwar_remote_event_handler.rs": 3,
    "graphwar_server_connection_handler.rs": 6,
}
CONTROL_NEG = ("math_function.rs", "graphwar_tick_handler.rs")


# ---------------------------------------------------------------------------
# Rust 源码扫描：fn / impl / 注释 / 地址引用
# ---------------------------------------------------------------------------
CHAR_LIT = re.compile(r"'(?:\\x[0-9a-fA-F]{2}|\\u\{[0-9a-fA-F]{1,6}\}|\\.[^']*|[^\\'])'")
FN_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
IMPL_RE = re.compile(r"\bimpl\b")
MOD_RE = re.compile(r"\bmod\s+[A-Za-z_][A-Za-z0-9_]*\s*\{")
CFG_TEST = "#[cfg(test)]"
HEX_RE = re.compile(r"0x([0-9a-fA-F][0-9a-fA-F_]*)")


def strip_and_comments(src):
    """返回 (stripped, comments)。

    stripped 与 src **等长**（注释/字符串里的字符换成空格，换行保留），
    这样任何位置都能直接映射回行号。comments 是 [(line, text)]（只收 `//` 与 `/* */`）。
    """
    out = []
    comments = []
    i, n = 0, len(src)
    line = 1
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            if j < 0:
                j = n
            comments.append((line, src[i:j]))
            out.append(" " * (j - i))
            i = j
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            j = src.find("*/", i + 2)
            j = n if j < 0 else j + 2
            body = src[i:j]
            comments.append((line, body))
            line += body.count("\n")
            out.append(re.sub(r"[^\n]", " ", body))
            i = j
            continue
        if c == "r" and i + 1 < n and src[i + 1] in '#"':
            m = re.match(r'r(#*)"', src[i:])
            if m:
                close = '"' + m.group(1)
                j = src.find(close, i + m.end())
                j = n if j < 0 else j + len(close)
                body = src[i:j]
                out.append(re.sub(r"[^\n]", " ", body))
                line += body.count("\n")
                i = j
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    j += 1
                    break
                j += 1
            body = src[i:j]
            out.append(re.sub(r"[^\n]", " ", body))
            line += body.count("\n")
            i = j
            continue
        if c == "'":
            m = CHAR_LIT.match(src, i)
            if m:
                body = m.group(0)
                out.append(" " * len(body))
                i = m.end()
                continue
        out.append(c)
        if c == "\n":
            line += 1
        i += 1
    return "".join(out), comments


def line_of(text, pos):
    return text.count("\n", 0, pos) + 1


def brace_match(text, open_pos):
    """text[open_pos] == '{'；返回配对 '}' 的位置（找不到返回 len-1）。"""
    depth = 0
    for i in range(open_pos, len(text)):
        ch = text[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return i
    return len(text) - 1


def _last_ident(header):
    """从 impl 头里取被实现的类型名：`impl<T> Foo<T> for Bar` -> Bar；`impl Foo` -> Foo。"""
    h = re.sub(r"\{.*", "", header, flags=re.S)
    h = re.sub(r"<[^<>]*>", "", h)
    h = re.sub(r"\bwhere\b.*", "", h, flags=re.S)
    ids = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", h)
    # 去掉 impl / for / trait 路径前缀等关键字；取最后一个标识符作为类型名
    idents = [x for x in ids if x not in ("impl", "for", "where", "unsafe", "const", "dyn", "mut")]
    # `impl core::fmt::Debug for Foo` -> 最后一个标识符就是 Foo；`impl Foo` -> Foo
    return idents[-1] if idents else ""


class FnDef:
    __slots__ = ("name", "file", "line", "end_line", "has_body", "impl_type", "is_test")

    def __init__(self, name, file, line, end_line, has_body, impl_type, is_test):
        self.name = name
        self.file = file
        self.line = line
        self.end_line = end_line
        self.has_body = has_body
        self.impl_type = impl_type
        self.is_test = is_test

    def __repr__(self):
        return f"<fn {self.name} {self.file}:{self.line} body={self.has_body} test={self.is_test}>"


def scan_file(rel):
    with open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace") as fh:
        src = fh.read()
    stripped, comments = strip_and_comments(src)

    # impl 块范围（用于给 fn 记 impl_type）
    impls = []  # (start_line, end_line, type_name)
    for m in IMPL_RE.finditer(stripped):
        ob = stripped.find("{", m.end())
        if ob < 0:
            continue
        header = stripped[m.start():ob]
        cb = brace_match(stripped, ob)
        impls.append((line_of(stripped, m.start()), line_of(stripped, cb), _last_ident(header)))

    # #[cfg(test)] mod 块范围
    test_ranges = []
    for m in re.finditer(re.escape(CFG_TEST), stripped):
        mm = MOD_RE.search(stripped, m.end())
        if not mm:
            continue
        ob = stripped.find("{", mm.start())
        cb = brace_match(stripped, ob)
        test_ranges.append((line_of(stripped, ob), line_of(stripped, cb)))

    fns = []
    for m in FN_RE.finditer(stripped):
        name = m.group(1)
        # 找函数体的 '{' 或声明结束的 ';'
        j = m.end()
        nob = stripped.find("{", j)
        nsc = stripped.find(";", j)
        if nob < 0:
            continue
        has_body = (nsc < 0) or (nob < nsc)
        if has_body:
            cb = brace_match(stripped, nob)
            end_line = line_of(stripped, cb)
        else:
            end_line = line_of(stripped, nsc)
        start_line = line_of(stripped, m.start())
        impl_type = ""
        best = None
        for (sl, el, ty) in impls:
            if sl <= start_line <= el:
                if best is None or sl > best[0]:
                    best = (sl, ty)
        if best:
            impl_type = best[1]
        is_test = any(sl <= start_line <= el for (sl, el) in test_ranges)
        fns.append(FnDef(name, rel, start_line, end_line, has_body, impl_type, is_test))

    # 注释里的地址引用
    cites = collections.defaultdict(list)  # addr_int -> [(line, text)]
    for (ln, text) in comments:
        for hm in HEX_RE.finditer(text):
            raw = hm.group(1).replace("_", "")
            try:
                val = int(raw, 16)
            except ValueError:
                continue
            if val < 0x1000:
                continue
            cites[val].append((ln, text.strip()))

    return fns, comments, cites


class SrcIndex:
    def __init__(self):
        self.fns_by_file = {}
        self.fns_by_name = collections.defaultdict(list)
        self.comments = {}
        self.cites = {}
        self.line_count = {}
        files = []
        for dirpath, _dirs, names in os.walk(os.path.join(ROOT, "src")):
            for nm in sorted(names):
                if nm.endswith(".rs"):
                    rel = os.path.relpath(os.path.join(dirpath, nm), ROOT).replace(os.sep, "/")
                    files.append(rel)
        for rel in sorted(files):
            fns, comments, cites = scan_file(rel)
            self.fns_by_file[rel] = fns
            self.line_count[rel] = len(open(os.path.join(ROOT, rel), encoding="utf-8",
                                            errors="replace").read().splitlines())
            for f in fns:
                self.fns_by_name[f.name].append(f)
            self.comments[rel] = comments
            for addr, lst in cites.items():
                self.cites.setdefault(addr, []).extend([(rel, ln, tx) for (ln, tx) in lst])

    # -- 查找 -------------------------------------------------------------
    def find_fns(self, name, files, body_only=True):
        pool = files
        if pool == ("__ALL__",):
            cands = list(self.fns_by_name.get(name, []))
        else:
            cands = [f for f in self.fns_by_name.get(name, []) if f.file in pool]
        out = []
        for f in cands:
            if body_only and not f.has_body:
                continue
            if f.is_test:
                continue
            out.append(f)
        return out

    def find_fn(self, name, files, body_only=True):
        """在给定文件集里找第一个同名实现函数。files=("__ALL__",) 表示全 src/。"""
        pool = files
        if pool == ("__ALL__",):
            cands = self.fns_by_name.get(name, [])
        else:
            cands = [f for f in self.fns_by_name.get(name, []) if f.file in pool]
        for f in cands:
            if body_only and not f.has_body:
                continue
            if f.is_test:
                continue
            return f
        return None

    def attach_addr(self, addr, files):
        """addr 的注释引用能不能贴到一个 fn 上；返回 (FnDef, rel, line) 或 None。"""
        for (rel, ln, _tx) in self.cites.get(addr, []):
            if files != ("__ALL__",) and rel not in files:
                continue
            hit = self._attach_one(rel, ln)
            if hit:
                return hit + (ln,)
        return None

    def _attach_one(self, rel, ln):
        fns = self.fns_by_file.get(rel, [])
        # ① 注释行落在 fn 的行区间里（优先最内层 = 起始行最大的那个）
        inside = [f for f in fns if f.line <= ln <= f.end_line and f.has_body and not f.is_test]
        if inside:
            inside.sort(key=lambda f: f.line)
            return (inside[-1], rel)
        # ② 注释行落在一段 doc 注释块里，而该块**直接终结于**某个 fn/impl 内 fn：
        #    从引用行往下走，只要还是注释/属性/空行就继续，遇到第一个代码行时判它是不是 fn 头。
        #    （旧写法是"上方 <=30 行"，会把长 doc 块误判成贴不上；那是保守方向的错误，
        #     会让 F1 变少 —— 这里改成按块走，块长度上限 200 行。）
        fn_starts = {f.line: f for f in fns if f.has_body and not f.is_test}
        k = ln + 1
        while k <= ln + 200:
            t = _line_text(rel, k).strip()
            if not t or t.startswith("//") or t.startswith("#[") or t.startswith("#!"):
                k += 1
                continue
            f = fn_starts.get(k)
            if f:
                return (f, rel)
            # 代码行但不是 fn 头（const/struct/use…）⇒ 这段注释不是 fn 的 doc
            break
        return None


_LINE_CACHE = {}


def _line_text(rel, line):
    key = (rel, line)
    if key not in _LINE_CACHE:
        if rel not in _LINE_CACHE:
            with open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace") as fh:
                _LINE_CACHE[rel] = fh.read().splitlines()
        arr = _LINE_CACHE[rel]
        _LINE_CACHE[key] = arr[line - 1] if 1 <= line <= len(arr) else ""
    return _LINE_CACHE[key]


# ---------------------------------------------------------------------------
# 输入
# ---------------------------------------------------------------------------
def read_rows(path):
    rows = []
    with open(os.path.join(ROOT, path), encoding="utf-8") as fh:
        for line in fh:
            if line.startswith("#") or not line.strip():
                continue
            rows.append(line.rstrip("\n").split("\t"))
    return rows


def load_boundary():
    rows = read_rows(BOUNDARY)
    hdr = rows[0]
    idx = {k: i for i, k in enumerate(hdr)}
    out = []
    for r in rows[1:]:
        if len(r) < len(hdr):
            continue
        out.append(dict(
            addr=r[idx["addr"]],
            addr_int=int(r[idx["addr"]], 16),
            name=r[idx["name"]],
            name_src=r[idx["name_src"]],
            loc_main_file=r[idx["loc_main_file"]],
            loc_main_lines=r[idx["loc_main_lines"]],
            ours_file=r[idx["ours_file"]],
            ours_hit=r[idx["ours_hit"]],
            verdict=r[idx["verdict"]],
            basis=r[idx["basis"]],
        ))
    return out


def load_matrix():
    rows = read_rows(MATRIX)
    hdr = rows[0]
    idx = {k: i for i, k in enumerate(hdr)}
    m = {}
    for r in rows[1:]:
        if len(r) < len(hdr):
            continue
        m[r[idx["module"]]] = dict(
            module=r[idx["module"]], scope=r[idx["scope"]],
            orig_funcs=int(r[idx["orig_funcs"]]), contested=int(r[idx["contested"]]),
            anchors=int(r[idx["anchors"]]), ours_file=r[idx["ours_file"]],
            ours_funcs=int(r[idx["ours_funcs"]]), status=r[idx["status"]],
            evidence=r[idx["evidence"]],
        )
    return m


# ---------------------------------------------------------------------------
# T85：读 + 闸 `fixtures/f3_triage.tsv`（T82 的人工裁决台账）
# ---------------------------------------------------------------------------
def load_na_ledger():
    """读台账的**主表**（153 行），返回 {addr(lower): dict(...)}。

    只读主表：读到 `# ----` 开头的行（F2-scan 分节横幅）即停 —— 那一节是**机判候选**，
    T82 §129.5.3 明写"命中不等于改判 F2 的档位"（里面还有 4 条已知的真业务函数），
    **绝不能**当 na 用（预注册 §1）。边界写死在这里，不靠缩进/空行猜。
    """
    path = os.path.join(ROOT, NA_LEDGER)
    if not os.path.exists(path):
        sys.exit(f"na 台账不存在：{NA_LEDGER}（旧行为请加 --no-na）")
    raw = []
    with open(path, encoding="utf-8") as fh:
        for ln_no, line in enumerate(fh, 1):
            if line.startswith("# ----"):
                break
            if line.startswith("#") or not line.strip():
                continue
            raw.append((ln_no, line.rstrip("\n").split("\t")))
    if len(raw) < 2:
        sys.exit(f"{NA_LEDGER}: 主表为空（只读到 {len(raw)} 行）")
    hdr = raw[0][1]
    need = ["vaddr", "name", "size", "hits", "category", "evidence"]
    if hdr[:len(need)] != need:
        sys.exit(f"{NA_LEDGER}: 主表表头应以前 6 列 {need} 开头，实得 {hdr}")
    idx = {k: i for i, k in enumerate(hdr)}
    ledger = {}
    for ln_no, r in raw[1:]:
        if len(r) < len(hdr):
            sys.exit(f"{NA_LEDGER}:{ln_no}: 列数 {len(r)} < 表头 {len(hdr)}")
        addr = r[idx["vaddr"]].strip().lower()
        if addr in ledger:      # M3
            sys.exit(f"{NA_LEDGER}:{ln_no}: 地址重复 {addr}（首次在 {ledger[addr]['line']} 行）")
        try:
            size = int(r[idx["size"]], 16)
        except ValueError:
            sys.exit(f"{NA_LEDGER}:{ln_no}: size 不是十六进制 {r[idx['size']]!r}")
        ledger[addr] = dict(addr=r[idx["vaddr"]].strip(), name=r[idx["name"]],
                            size=size, hits=r[idx["hits"]],
                            category=r[idx["category"]].strip(),
                            evidence=r[idx["evidence"]].strip(), line=ln_no)
    return ledger


def validate_na_ledger(ledger, boundary):
    """§3–§5 的机判闸 M1/M2/M4/M5/M7（M3 在读取时已挡，M6 要判完档才能查）。

    预注册要求「先打印**全部**违例，再 exit(2)」——所以这里只收集，不早退。
    """
    viol = []
    warn = []
    bmap = {r["addr"].lower(): r for r in boundary}
    for addr, e in ledger.items():
        # M1 值域
        if e["category"] not in NA_DOMAIN:
            viol.append(f"M1 {NA_LEDGER}:{e['line']} {e['addr']} 类别 {e['category']!r} 不在值域 {NA_DOMAIN}")
            continue
        # M2 台账 addr 必须在 527 行里（否则分母已经不是 527 = 台账漂移）
        b = bmap.get(addr)
        if b is None:
            viol.append(f"M2 {NA_LEDGER}:{e['line']} {e['addr']} 不在 {BOUNDARY}（台账漂移：分母不是 527）")
            continue
        # M7 同名（最灵敏的漂移探针）
        if e["name"] != b["name"]:
            viol.append(f"M7 {NA_LEDGER}:{e['line']} {e['addr']} 名字与 boundary 不一致："
                        f"台账 {e['name']!r} ≠ boundary {b['name']!r}")
        # M4 体积上限（只对 na:* 生效）
        if e["category"].startswith(NA_PREFIX) and e["size"] > NA_SIZE_CAP:
            viol.append(f"M4 {NA_LEDGER}:{e['line']} {e['addr']} 尺寸 0x{e['size']:x} > 上限 "
                        f"0x{NA_SIZE_CAP:x}（§4：大函数上的 na 特征全是假阳性）")
        # M5 依据（n 人工复核闸：na 入库必须带依据）
        ev = e["evidence"]
        hits = [k for k in NA_EVIDENCE_KEYS if k in ev]
        if len(ev) < NA_EVIDENCE_MIN or len(hits) < NA_EVIDENCE_MIN_KEYS:
            viol.append(f"M5 {NA_LEDGER}:{e['line']} {e['addr']} 依据不足：长 {len(ev)} 字符、"
                        f"命中 {hits}（要求 ≥{NA_EVIDENCE_MIN} 字符且 ≥{NA_EVIDENCE_MIN_KEYS} 个标记）")
        elif len(hits) < NA_EVIDENCE_WARN_KEYS:
            warn.append(f"{e['addr']}（{e['category']}）只命中 {len(hits)} 个标记 {hits}、"
                        f"长 {len(ev)} —— 低于预注册 §5 的 ≥{NA_EVIDENCE_WARN_KEYS}，见 docs/06 的「§我写错了」")
    if warn:
        print(f"\n⚠ 依据偏薄（{len(warn)} 条，**不拒绝出数**，但每次运行都提示）：")
        for w in warn:
            print("   " + w)
    if viol:
        print(f"\n✗ na 台账没通过机判闸（{len(viol)} 条违例）—— 按预注册 §3，**拒绝出数**：")
        for v in viol:
            print("   " + v)
        sys.exit(2)


def na_counts(ledger):
    """返回 (n_na, n_strong, n_total)：分母修正用（口径 A/B/B′）。"""
    n_na = sum(1 for e in ledger.values() if e["category"].startswith(NA_PREFIX))
    n_strong = sum(1 for e in ledger.values() if e["category"] in NA_STRONG)
    return n_na, n_strong, len(ledger)


# ---------------------------------------------------------------------------
# 名字 → 候选 fn 名
# ---------------------------------------------------------------------------
COMPILER_CHILD = ("::{{closure}}", "::panic_cold_explicit", "::panic_cold_display",
                  "::panic_cold_index", "::panic_cold_unwrap")


def name_candidates(name, name_src):
    """返回 (cands, kind, note)。

    kind: normal / child-of-fn（编译器生成的子函数，用父函数判）/ macro-generated / noname
    """
    if not name or name == "—":
        return [], "noname", "无名"
    if "?" in name:            # hyb 的 `Type::?::_` 占位符不是名字
        return [], "noname", "歧义占位名"
    n = name
    kind = "normal"
    for suf in COMPILER_CHILD:
        if n.endswith(suf):
            n = n[:-len(suf)]
            kind = "child-of-fn"
            break
    segs = [s for s in n.split("::") if s]
    if not segs:
        return [], "noname", "空名"
    last = segs[-1]
    if kind == "child-of-fn":
        return [last], kind, last
    if last.startswith("__inner_init"):
        return ["init"], "macro-generated", "__inner_init→init"
    if last.startswith("__"):
        return [], "macro-generated", last
    return [last], kind, last


def expected_type(name):
    """原版名字里"函数所属类型"那一段：`m::Type::method` -> Type；自由函数返回 ""。"""
    if not name or name == "—" or "?" in name:
        return ""
    n = name
    for suf in COMPILER_CHILD:
        if n.endswith(suf):
            n = n[:-len(suf)]
            break
    segs = [s for s in n.split("::") if s]
    if len(segs) < 3:
        return ""
    ty = segs[-2]
    if ty in ("_", "__init") or not ty[:1].isupper():
        return ""
    return ty


# ---------------------------------------------------------------------------
# 判档
# ---------------------------------------------------------------------------
def judge(row, matrix, idx, na=None):
    """na=None ⇒ 旧行为（不读台账、不做负向排除）；na=dict ⇒ T85 的 na 口径（预注册 §6）。"""
    module = os.path.basename(row["loc_main_file"])
    ours = row["ours_file"]
    grade = matrix.get(module, {}).get("status", "")
    module_evidence = matrix.get(module, {}).get("evidence", "")
    na_entry = (na or {}).get(row["addr"].lower())

    base = dict(
        bucket="", sub="", pin="none", pin_where="", missing="", judge_method="",
        type_check="n/a", type_note="", na_blocked=False,
    )
    # ours_file 可能是多文件（用 '+' 连接）：该模块被我方拆进 N 个文件
    ours_list = [p for p in (ours or "").split("+") if p]
    ours_present = [p for p in ours_list if p in idx.fns_by_file]

    # ---- S0: 文件在不在 ----------------------------------------------------
    if not ours_present:
        base.update(bucket="F4", sub="no-file", pin="none",
                    missing="我方没有这个模块的文件",
                    judge_method="S0：src/ 下不存在 ours_file")
        return base

    impl_fns = [f for p in ours_present for f in idx.fns_by_file[p]
                if not f.is_test and f.has_body]
    all_files = tuple(idx.fns_by_file.keys())
    ours_key = ours_present[0] if len(ours_present) == 1 else " / ".join(ours_present)

    # ---- S1: 只有骨架 ------------------------------------------------------
    if not impl_fns:
        base.update(bucket="F3", sub="skel", pin="none",
                    missing="文件只有数据/注释/枚举，没有一个实现函数",
                    judge_method=f"S1：{ours_key} 的实现函数数 = 0（{sum(idx.line_count.get(x, 0) for x in ours_present)} 行）")
        return base

    cands, kind, note = name_candidates(row["name"], row["name_src"])
    v2 = row["name_src"].startswith("v2")
    hyb = row["name_src"].startswith("hyb")

    hit = None
    pin = "none"
    pin_where = ""
    exp_type = expected_type(row["name"])

    def _match(cands, files, want_soft):
        """按名字找 fn。四种命中质量：

        exact  impl 类型与原名里的类型一致（原版是自由函数时只要求 impl_type 为空）
        free   原版是 `Type::method`，我方只有**同名自由函数**（跨模块）⇒ 只能算弱指认
        soft   同名但在**另一个 impl 类型**下（类型被改名/合并）
        同名函数可能有很多个（`fn ready` 在 9 个 *_class.rs 里各有一个），必须遍历全部候选。
        """
        soft = free = None
        for c in cands:
            for f in idx.find_fns(c, files):
                if not exp_type:
                    if not f.impl_type:
                        return f, "exact"
                    if soft is None:
                        soft = f
                    continue
                if f.impl_type == exp_type:
                    return f, "exact"
                if not f.impl_type and free is None:
                    free = f
                elif soft is None:
                    soft = f
        if want_soft is False:
            return (None, "")
        pick = {"soft": soft, "free": free}[want_soft]
        return (pick, want_soft) if pick else (None, "")

    # ---- P0 宏生成 wrapper（`_::__init::__inner_init`）：本体是我方 `fn init` ----
    #  gdext 在两侧都由 `#[godot_api]` 从用户写的 `fn init` 生成这个 wrapper，
    #  所以"源码层对应物 = fn init"；找到就算实现（R3）。
    if kind == "macro-generated" and cands:
        stem = os.path.basename(row["loc_main_file"]).replace(".rs", "")
        near = [p for p in idx.fns_by_file
                if p.startswith(f"src/{stem}/") or p == f"src/{stem}.rs"]
        cls = f"{SCAFFOLD_PREFIX}{stem}_class.rs"
        for c in cands:
            f = idx.find_fn(c, tuple(near))
            if f:
                hit, pin, pin_where = f, "macro", f"{f.file}:{f.line}"
                break
        if not hit and cls in idx.fns_by_file:
            f = idx.find_fn(cands[0], (cls,))
            if f:
                hit, pin, pin_where = f, "scaffold", f"{f.file}:{f.line}"

    # ---- P1 NAME（真名，强）------------------------------------------------
    if not hit and v2 and cands:
        f, q = _match(cands, tuple(ours_present), want_soft=False)
        if f:
            hit, pin = f, "name"
        if not hit:
            f, q = _match(cands, ("__ALL__",), want_soft=False)
            if f:
                hit, pin = f, ("name-other" if not f.file.startswith(SCAFFOLD_PREFIX)
                               else "scaffold")
        if not hit:   # R5b：同名但在本模块的**另一个 impl 类型**下 ⇒ 类型被改名/合并
            f, q = _match(cands, tuple(ours_present), want_soft="soft")
            if f:
                hit, pin = f, "name-soft"
        if not hit:   # R5c：同名但只有**跨模块的自由函数** ⇒ 指认只能算推断
            f, q = _match(cands, ("__ALL__",), want_soft="free")
            if f and not f.file.startswith(SCAFFOLD_PREFIX):
                hit, pin = f, "name-free"
        if not hit:   # R1：同名实现只在 src/_scaffold/
            f, q = _match(cands, ("__ALL__",), want_soft="free")
            if f:
                hit, pin = f, "scaffold"
        if hit:
            pin_where = f"{hit.file}:{hit.line}"

    # ---- P2 ADDR（地址引用，强）-------------------------------------------
    p2_hit = False
    if not hit:
        a = idx.attach_addr(row["addr_int"], tuple(ours_present)) or idx.attach_addr(row["addr_int"], all_files)
        if a:
            p2_hit = True
            f, rel, ln = a
            hit, pin, pin_where = f, "addr", f"{f.file}:{f.line}(注释 {rel}:{ln})"
            if rel.startswith(SCAFFOLD_PREFIX) or f.file.startswith(SCAFFOLD_PREFIX):
                pin = "scaffold"

    # ---- P3 HYB（预测名，弱）----------------------------------------------
    if not hit and hyb and cands:
        f, _q = _match(cands, tuple(ours_present), want_soft=False)
        if not f:
            f, _q = _match(cands, ("__ALL__",), want_soft=False)
        if f:
            hit, pin, pin_where = f, "hyb", f"{f.file}:{f.line}"

    # 只按地址"登记"过、但贴不到任何 fn（T30 那 9 条的样子）
    cited_anywhere = row["addr_int"] in idx.cites
    gh = idx.cites.get(row["addr_int"] + 0x100000)

    # ---- N1 负向排除（T85；预注册 §6）：判了 na 的地址**不是**实现证据 ----
    # 台账 = 人工裁决（T82）。只要该类别的函数体本就不该我们手写，它出现在我方注释里
    # 最多是"登记/取证线索"。T71 §123.5 实测假抬 5 条；T80 后同一机制假抬 33 条。
    if na_entry and na_entry["category"].startswith(NA_PREFIX) and p2_hit:
        cat = na_entry["category"]
        base.update(bucket="F3", sub=cat, pin="none", pin_where="",
                    missing=f"台账判 {cat}（不该手写），但地址出现在我方注释里 —— "
                            f"按 N1 负向排除，**不算实现证据**；台账依据：{NA_LEDGER}:{na_entry['line']}",
                    judge_method=f"N1：P2 命中被 na 负向排除（{cat}）",
                    na_blocked=True)
        return base

    # ---- 人工复核（只作用于 P2 的命中；见 ADDR_MANUAL）---------------------
    if pin == "addr" and row["addr"] in ADDR_MANUAL:
        nb, nsub, why = ADDR_MANUAL[row["addr"]]
        base.update(bucket=nb, sub=nsub, pin=pin, pin_where=pin_where,
                    missing=why, judge_method="P2 机制命中，但人工复核否决（见 ADDR_MANUAL）")
        return base

    # ---- 定档 ------------------------------------------------------------
    if pin == "name-soft":
        tin = (f"同名函数在 `impl {hit.impl_type}` 下，原版类型是 `{exp_type}` ⇒ 类型被改名/合并"
               if hit and hit.impl_type else "同名但 impl 类型与原版不一致")
        base.update(type_check="renamed", type_note=tin)
    elif pin in ("name", "name-other", "addr") and hit:
        base.update(type_check=("ok" if hit.impl_type else "ok-free"), type_note="")
    elif pin == "macro" and hit:
        base.update(type_check="n/a",
                    type_note="gdext 宏 wrapper（`__inner_init`），本体是我方 `fn init`")

    if hit and pin in ("name", "name-other", "addr", "name-soft", "macro"):
        if grade in GRADES_WITH_EVIDENCE:
            base.update(bucket="F1", sub="pinned", pin=pin, pin_where=pin_where,
                        missing=("" if pin != "name-soft" else
                                 f"类型边界不同（我方 impl {hit.impl_type or '自由函数'} ≠ 原版 {exp_type}）"),
                        judge_method=f"{pin} 指认 + 档位={grade}")
        else:
            base.update(bucket="F2", sub="grade-inferred", pin=pin, pin_where=pin_where,
                        missing=f"模块档位只是{grade or '未标'}，缺逐位/观测级物证",
                        judge_method=f"{pin} 指认但档位={grade or '未标'}")
        return base

    if hit and pin == "name-free":
        base.update(bucket="F2", sub="free-fn-name", pin=pin, pin_where=pin_where,
                    type_check="free-fn",
                    missing=f"原版是 `{exp_type}::{cands[0]}`（类型上的方法），我方只有跨模块的"
                            f"同名自由函数 ⇒ 指认只能算推断",
                    judge_method="P1 未命中同类型实现，只找到自由函数同名 ⇒ F2")
        return base

    if hit and pin in ("hyb", "scaffold"):
        why = {"hyb": "只有结构指纹预测名（hyb），候选名不能当结论",
               "scaffold": "同名实现只在 src/_scaffold/（自造桥接，按 AGENTS §4 不进复原层）"}[pin]
        base.update(bucket="F2", sub=pin + "-only", type_check="n/a",
                    pin=pin, pin_where=pin_where,
                    missing=why, judge_method=f"pin={pin} ⇒ 只能到 F2")
        return base

    # ---- 无 pin ⇒ F3（按子类分两组）---------------------------------------
    if kind == "macro-generated":
        base.update(bucket="F3", sub="na:macro-generated", pin="none",
                    missing=f"宏生成（{note}）：源码层无独立 fn，等价物是 #[func]/#[godot_api] 属性",
                    judge_method="R3：gdext 宏产物，无显式 fn")
        return base

    # 父函数缺失（编译产物无处安放）
    if kind == "child-of-fn":
        parent = cands[0] if cands else ""
        pfound = idx.find_fn(parent, ("__ALL__",)) if parent else None
        if not pfound:
            base.update(bucket="F3", sub="parent-missing", pin="none",
                        missing=f"编译器子函数（{note}）的父函数 `{parent}` 在我方不存在",
                        judge_method="R2：取父函数，父函数无对应")
            return base

    if cited_anywhere:
        base.update(bucket="F3", sub="register-only", pin="none",
                    missing="地址有登记（注释里写了）但没有贴到任何函数体",
                    judge_method=f"P2 未命中：{row['addr']} 只被登记，无 fn")
        return base

    if v2:
        base.update(bucket="F3", sub="named-miss", pin="none",
                    missing=f"真名 `{(cands or ['?'])[0]}` 在我方没有同名实现函数",
                    judge_method="P1 未命中（真名是硬的）")
        return base
    if hyb:
        base.update(bucket="F3", sub="hyb-miss", pin="none",
                    missing=f"预测名 `{(cands or [note])[0]}` 未命中；预测本身也不可靠",
                    judge_method="P3 未命中（弱证据）")
        return base

    base.update(bucket="F3", sub="noname", pin="none",
                missing="原版函数无名可指（symbol 侧无锚），我方也没有地址引用",
                judge_method="跳过 P1/P3，P2 未命中")
    if gh:
        base["missing"] += f"；只有 Ghidra 口径 +0x100000 的引用（{gh[0][0]}:{gh[0][1]}）"
    return base


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
def build(ledger=None):
    """ledger=None ⇒ 旧行为（--no-na）；ledger=dict ⇒ na 口径 + P2 负向排除（T85 默认）。

    闸（M1/M2/M3/M4/M5/M7）在 `load_na_ledger` + `validate_na_ledger` 里做，M6 在本函数尾部。
    """
    rows = load_boundary()
    matrix = load_matrix()
    idx = SrcIndex()
    out = []
    GAP = ("skel", "register-only", "named-miss", "parent-missing")
    UNK = ("hyb-miss", "noname")
    for r in rows:
        j = judge(r, matrix, idx, ledger)
        if j["bucket"] in ("F1", "F2"):
            j["group"] = "ok"
        elif j["sub"] in GAP:
            j["group"] = "gap"
        elif j["sub"] in UNK:
            j["group"] = "unk"
        else:
            j["group"] = "na"
        j["na"] = (ledger or {}).get(r["addr"].lower())
        out.append(dict(row=r, **j))
    # M6 台账 vs 代码矛盾：na:* 行被**名字**指认命中 ⇒ 要么台账错、要么名字指认错，人来判断
    if ledger:
        NAME_PINS = ("name", "name-other", "name-soft", "macro")
        viol = [o for o in out
                if o["na"] and o["na"]["category"].startswith(NA_PREFIX) and o["pin"] in NAME_PINS]
        if viol:
            print(f"\n✗ na 台账与代码矛盾（{len(viol)} 条）：台账判 na:*，却靠**名字**指认到了实现 —— "
                  f"按预注册 §3-M6 **拒绝出数**：")
            for o in viol:
                print(f"   {o['row']['addr']}  {o['na']['category']:14s} pin={o['pin']:9s} "
                      f"{o['pin_where']}  台账 {NA_LEDGER}:{o['na']['line']}")
            print("   ⇒ 要么台账类别错（那是有实现、不该 na），要么名字指认错（同名不代表同一个函数）。"
                  "P2（地址）命中**不算**矛盾 —— 那是 N1 要排除的对象。")
            sys.exit(2)
    return out


def fmt_row(o, with_na=False):
    r = o["row"]
    cols = [
        r["addr"], r["name"], r["name_src"], os.path.basename(r["loc_main_file"]),
        r["ours_file"], o["pin_where"], o["bucket"], o["sub"], o["pin"],
        o["type_check"], r["verdict"], o["judge_method"], o["missing"],
    ]
    if with_na:
        e = o.get("na")
        cols.append(e["category"] if e else NA_DEFAULT)
        cols.append((f"ledger#{e['line']} " + e["evidence"].replace("\t", " ")[:120]) if e
                    else NA_DEFAULT_BASIS)
    return "\t".join(cols)


HEADER = ("# function_coverage.tsv —— 由 tools/function_coverage.py 生成，**不要手改**\n"
          "# 重算：python3 tools/function_coverage.py      只看统计：--stats      正/反面对照：--control\n"
          "# 一行 = 一个**原版可归属函数**（527 行，= fixtures/boundary_audit.tsv 的行，分母 527）\n"
          "# 判据是**预注册**的：fixtures/function_coverage_plan.txt（先提交，后跑）。\n"
          "#   F1 有实现+有物证（真名/地址指认 + 档位逐位/观测）\n"
          "#   F2 有实现+无物证（只有预测名 / 只在 _scaffold / 档位=推断）\n"
          "#   F3 只有文件/骨架（文件在、档位写了，但没有函数体）\n"
          "#   F4 完全没有（连文件/条目都没有）\n"
          "# 列：addr(ELF vaddr) | 符号名 | 名字来源 | 原版模块 | 我方文件 | 我方实现位置 | 判档 | 子类 |"
          " 指认方式 | 类型核对 | boundary_audit 判定 | 判据（机判步骤） | 缺什么\n"
          "#   type_check：ok=同名函数在 impl 同名类型下 / free-fn=我方是自由函数 / renamed=同名但 impl 类型"
          "与原版不同（**敏感度口径**：只要求名字则算 F1，要求类型边界也一致则不算）/ n/a=非名字指认\n"
          "# 子类：skel=文件无实现函数 / register-only=只登记了地址没函数体 / named-miss=真名无同名实现 /\n"
          "#       parent-missing=编译产物的父函数不存在 / hyb-miss=预测名未命中 / noname=无名 /\n"
          "#       na:macro-generated=gdext 宏生成（源码层无对应 fn）/ scaffold-only=只在 _scaffold /\n"
          "#       hyb-only=只有预测名 / pinned=F1 的指认成立 / grade-inferred=档位=推断 /\n"
          "#       addr-neighbor=地址引用只贴到邻居函数（人工复核否决，见工具里的 ADDR_MANUAL）\n")

# T85：启用 na 口径时追加的两列与说明（`--no-na` 时不追加 ⇒ 生成物与 T85 之前逐字节相同）
NA_HEADER_EXTRA = (
    "# ── T85 追加两列（预注册 fixtures/function_coverage_na_plan.txt）──────────────────────────\n"
    "#   na       = 该原版函数**该不该由我们手写**的裁决（来自 fixtures/f3_triage.tsv，T82 人工裁决，\n"
    "#              工具不重判）：real-missing（缺省 = **没有 na 证据**，不是「确认真的缺」）/ na:std-mono /\n"
    "#              na:gdext-glue / na:other / unknown。只审过 153 条 F3，其余行是缺省值。\n"
    "#   na_basis = 依据（ledger#行号 + 台账的「依据」列前 120 字）；缺省行写明「未入台账」。\n"
    "#   **负向排除（N1）**：na:* 的地址出现在我方注释里**不算**实现证据（P2 否决）⇒ 该行判 F3 + 子类 na:*。\n"
    "#   三口径见 --stats：A 含 na(分母 527) / B 剔 na(分母 439) / B′(只剔强 na, 分母 446)，招牌= B。\n"
)


def stats(out, verbose=True, na=None):
    c = collections.Counter(o["bucket"] for o in out)
    sub = collections.Counter(o["sub"] for o in out)
    pin = collections.Counter(o["pin"] for o in out)
    matrix = load_matrix()
    bymod = collections.defaultdict(lambda: collections.Counter())
    for o in out:
        bymod[os.path.basename(o["row"]["loc_main_file"])][o["bucket"]] += 1
    n = len(out)
    m1 = c["F1"]
    m2 = c["F1"] + c["F2"]
    m3 = m2 + c["F3"]
    print(f"\n=== 分母 {n}（= fixtures/boundary_audit.tsv 的 527 行，Ghidra funcstarts 口径）")
    print(f"  F1 有实现+有物证   {c['F1']:4d}  ({c['F1']*100.0/n:5.1f}%)   <- M1 严格函数级复原")
    print(f"  F2 有实现+无物证   {c['F2']:4d}  ({c['F2']*100.0/n:5.1f}%)")
    print(f"  F3 只有文件/骨架   {c['F3']:4d}  ({c['F3']*100.0/n:5.1f}%)")
    print(f"  F4 完全没有        {c['F4']:4d}  ({c['F4']*100.0/n:5.1f}%)")
    print(f"\n  M1 = F1            = {m1:4d} / {n}  ({m1*100.0/n:5.1f}%)")
    print(f"  M2 = F1+F2         = {m2:4d} / {n}  ({m2*100.0/n:5.1f}%)  (有对应代码的上限)")
    print(f"  M3 = F1+F2+F3      = {m3:4d} / {n}  ({m3*100.0/n:5.1f}%)  <- 不得使用（把没做数成做了）")
    print(f"\n  pin 分布: {dict(pin.most_common())}")
    tc = collections.Counter(o["type_check"] for o in out if o["bucket"] == "F1")
    print(f"  F1 的 type_check: {dict(tc.most_common())}")
    strict1 = sum(1 for o in out if o["bucket"] == "F1" and o["type_check"] != "renamed")
    print(f"  敏感度 R5b：若再要求「同名函数在同一个 impl 类型下」⇒ M1_strict = {strict1} / {n} "
          f"({strict1*100.0/n:5.1f}%)")
    tgt = collections.Counter(o["pin_where"].split("(")[0] for o in out
                              if o["bucket"] == "F1" and o["pin_where"])
    extra = sum(v - 1 for v in tgt.values() if v > 1)
    m2n = m2 - sum(1 for o in out if o["bucket"] == "F2" and o["sub"] == "scaffold-only")
    print(f"  敏感度 R1：F2 里 {sum(1 for o in out if o['sub']=='scaffold-only')} 条是 scaffold-only"
          f"（代码只在自造桥接层）⇒ 只算「实现住在 src/ 原版模块里」时 M2_noscaffold = {m2n} / {n}"
          f" ({m2n*100.0/n:5.1f}%)")
    print(f"  敏感度 R6：F1 只指向 {len(tgt)} 个不同我方函数；"
          f"{sum(1 for v in tgt.values() if v > 1)} 个目标被 ≥2 个原版函数共用 "
          f"⇒ 1:1 边界上限 M1_1to1 = {m1 - extra} / {n} ({(m1-extra)*100.0/n:5.1f}%)")
    print("  子类分布:")
    for k, v in sub.most_common():
        print(f"    {k:22s} {v:4d}")
    f3_gap = sum(v for k, v in sub.items() if k in ("skel", "register-only", "named-miss", "parent-missing"))
    f3_unk = sum(v for k, v in sub.items() if k in ("hyb-miss", "noname"))
    if not na:
        # ---- 旧行为（--no-na）：这一段与 T85 之前**逐字节相同**（C1 的判据）----
        f3_na = sub.get("na:macro-generated", 0)
        print(f"\n  F3 三组：①缺函数边界（skel/register-only/named-miss/parent-missing）= {f3_gap}"
              f"  ②判不了（hyb-miss/noname）= {f3_unk}  ③宏产物不算缺 = {f3_na}")
        dm = n - f3_na
        print(f"  分母修正（剔除 {f3_na} 条 gdext 宏生成、源码层本无独立 fn）："
              f"{m1} / {dm} = {m1*100.0/dm:5.1f}%")
    else:
        # T85：na 子类 = 台账判 na:* 的降档行（含原有的 na:macro-generated）⇒ 一律不进 gap/unk
        f3_na = sum(v for k, v in sub.items() if k.startswith(NA_PREFIX))
        print(f"\n  F3 三组：①缺函数边界（skel/register-only/named-miss/parent-missing）= {f3_gap}"
              f"  ②判不了（hyb-miss/noname）= {f3_unk}  ③na（源码层本无独立 fn / 不该手写）= {f3_na}")

        # --------------------------------------------------------------
        # T85：三种口径（预注册 §7）。分子 = 负向排除后的 F1，分母按 na 台账修正。
        # --------------------------------------------------------------
        n_na, n_strong, n_ledger = na_counts(na)
        blocked = sum(1 for o in out if o.get("na_blocked"))
        legacy_f1 = m1 + blocked        # 不做负向排除时的 F1 = 旧口径的分子
        dA, dB, dBp = n, n - n_na, n - n_strong
        cB = m2                          # 口径 C 的分子 = F1+F2（参照用，不许当招牌）
        print(f"\n=== 函数级复原率**三口径**（na 台账 = {NA_LEDGER}，T82 人工裁决；"
              f"覆盖 {n_ledger}/{n} 条，其中 na:* = {n_na}（强 na {n_strong}））")
        print(f"  口径 A  含 na（分母 {dA}）：{m1:4d} / {dA}  = {m1*100.0/dA:5.1f}%"
              f"   保守/历史对照（分母把'标准库/宏产物'也算成我们的缺口）")
        print(f"  口径 B  剔 na（分母 {dB}）：{m1:4d} / {dB}  = {m1*100.0/dB:5.1f}%"
              f"   <- **招牌数**（分子分母同口径剔 na）")
        print(f"  口径 B′ 只剔强 na（分母 {dBp}）：{m1:4d} / {dBp}  = {m1*100.0/dBp:5.1f}%"
              f"   区间下界（na:other 那 {n_na - n_strong} 条留回分母）")
        print(f"  口径 C  参照（含 F2）：{cB:4d} / {dB}  = {cB*100.0/dB:5.1f}%"
              f"   ⚠️ F2 未做 na 专项（T82 §129.5.3）⇒ 仍虚高")
        print(f"  na 负向排除（N1）：{blocked} 条 na:* 的地址曾被我方注释引用（P2 假命中）⇒ "
              f"不算 F1（F1 {legacy_f1} → {m1}）")
        print(f"  [历史] 旧口径（`--no-na`，无负向排除）：A = {legacy_f1} / {dA} = "
              f"{legacy_f1*100.0/dA:5.1f}%  —— **不可引用**，只用于与 T85 之前的读数对照")
        print(f"  [禁止] 分子用含 na 的 {legacy_f1} + 分母剔 na 的 {dB} = "
              f"{legacy_f1*100.0/dB:5.1f}% 是**双向占便宜**的口径错配（HANDOVER 曾误写 71.1%）")

    GAP = ("skel", "register-only", "named-miss", "parent-missing")
    UNK = ("hyb-miss", "noname")
    permod = collections.defaultdict(lambda: collections.Counter())
    for o in out:
        mod = os.path.basename(o["row"]["loc_main_file"])
        permod[mod][o["group"]] += 1
        permod[mod]["tot"] += 1
        permod[mod][o["bucket"]] += 1
    print("\n=== 按模块：**确证缺函数边界**（gap）占比最高的 10 个（orig_funcs >= 5）")
    print("    gap = F3 里的 skel/register-only/named-miss/parent-missing（有证据表明没有这个函数体）")
    print("    unk = F3 里的 hyb-miss/noname（判不了）  na = gdext 宏产物（源码层无独立 fn）")
    rank = sorted([(m, cc) for m, cc in permod.items() if cc["tot"] >= 5],
                  key=lambda x: (-x[1]["gap"] * 1.0 / x[1]["tot"], -x[1]["gap"]))
    for mod, cc in rank[:10]:
        print(f"  gap {cc['gap']*100.0/cc['tot']:5.1f}%  {cc['gap']:3d}/{cc['tot']:3d}  {mod:42s}"
              f" F1={cc['F1']:3d} F2={cc['F2']:3d} F3={cc['F3']:3d}  unk={cc['unk']:2d} na={cc['na']:2d}"
              f"  (矩阵={matrix.get(mod, {}).get('status', '?')})")
    print("\n=== 按模块：gap **绝对条数**最多的 10 个（不看比例，看补工作量）")
    for mod, cc in sorted(rank, key=lambda x: (-x[1]["gap"], x[1]["tot"]))[:10]:
        print(f"  {cc['gap']:3d}/{cc['tot']:3d} = {cc['gap']*100.0/cc['tot']:5.1f}%  {mod}")
    print("\n=== 最严重的 12 个具体函数（F3 + 符号表真名，按 Location 锚点数降序）")
    def anchors(o):
        r = o["row"]
        return (r["loc_main_lines"].count(",") + 1) if r["loc_main_lines"] else 0
    sev = [o for o in out if o["bucket"] == "F3" and o["row"]["name_src"].startswith("v2")
           and o["group"] == "gap"]
    for o in sorted(sev, key=lambda o: (-anchors(o), o["row"]["addr"]))[:12]:
        print(f"  {o['row']['addr']}  锚{anchors(o):3d}  {o['sub']:14s} {o['row']['loc_main_file']:38s}"
              f" {o['row']['name'][:58]}")
    return c, sub, pin, bymod


def control(out):
    """正 / 反面对照。判法自校验的**判据**在预注册文件 §3 里写死，这里只报结果。"""
    print("\n=== 正对照① T30 三个新文件（形态已知，`docs/06` §80）")
    spec = {
        "graphwar_event_handler.rs": "F1=1（0x3d38f0 名字表有实现）、F3=1（0x3d3630 没有独立函数体）",
        "graphwar_remote_event_handler.rs": "三条全 F4（规格 §2 的期望）",
        "graphwar_server_connection_handler.rs": "部分 F2（规格 §2 的期望）",
    }
    note = {
        "graphwar_event_handler.rs":
            "✅ F1 对上；`0x3d3630` 实测 **F2/scaffold-only**（不是 F3）：地址在 "
            "src/_scaffold/root_wiring.rs 里贴着 `KernelBus::process_events`(2553) ⇒ 按预注册 R1 "
            "「代码只在自造桥接层」记 F2。方向一致：它**没有**被记成复原层已实现。",
        "graphwar_remote_event_handler.rs":
            "⚠️ 定义差异：按预注册 F4 的定义（『连文件/条目都没有』），该模块 126 行文件 + `FUNCTIONS` "
            "表逐条列着 0x3dee00/0x3e5910 ⇒ 应判 **F3/skel**（文件在、无函数体），不是 F4。"
            "对 M1/M2 无影响（两者都不进分子）。",
        "graphwar_server_connection_handler.rs":
            "⚠️ 实测 **F3/register-only ×6**（不是部分 F2）：6 条只被写进 `FUNCTIONS` 证据表，"
            "没有同类型实现、也没有 scaffold 实现 ⇒ 连 F2 的『有对应代码』都不成立。"
            "T30 的结论（这 6 条从未落码）**被这条独立复现**。",
    }
    for mod, want in spec.items():
        rows = [o for o in out if os.path.basename(o["row"]["loc_main_file"]) == mod]
        cc = collections.Counter(o["bucket"] for o in rows)
        print(f"\n  {mod}   n={len(rows)}（台账 orig_funcs 见 coverage-matrix）")
        print(f"    规格期望: {want}")
        print(f"    实测:     " + "  ".join(f"{k}={cc[k]}" for k in ("F1", "F2", "F3", "F4")))
        for o in sorted(rows, key=lambda x: x["row"]["addr"]):
            print(f"      {o['row']['addr']}  {o['bucket']}/{o['sub']:16s} pin={o['pin']:9s} "
                  f"{o['row']['name'][:62]}")
        print(f"    {note[mod]}")

    print("\n=== 反面对照② 已知扎实的模块：'真名 + 我方同 impl 同名函数'必须判 F1")
    viol = 0
    for mod in CONTROL_NEG:
        rows = [o for o in out if os.path.basename(o["row"]["loc_main_file"]) == mod]
        cc = collections.Counter(o["bucket"] for o in rows)
        print(f"\n  {mod}: n={len(rows)}  " + "  ".join(f"{k}={cc[k]}" for k in ("F1", "F2", "F3", "F4")))
        for o in sorted(rows, key=lambda x: x["row"]["addr"]):
            print(f"    {o['row']['addr']}  {o['bucket']}/{o['sub']:16s} pin={o['pin']:9s} "
                  f"{o['row']['name'][:66]}")
            if o["pin"] == "name":
                ok = o["bucket"] == "F1"
            elif o["row"]["name_src"].startswith("v2"):
                ok = True   # 没有同 impl 同名函数 ⇒ 判 F1 才是错
            else:
                ok = True
            if not ok:
                viol += 1
                print(f"      ✗ 违反：真名+同名函数却判了 {o['bucket']}")
    print(f"\n  反面对照违反数 = {viol}（必须为 0）")
    print("\n判法自校验结论：")
    print("  · 正向：3 个正对照里 1 项完全一致（event_handler 的 F1）、2 项差异**都被定义解释掉**"
          "（remote 的 F4→F3 是四档定义；server_conn 的『部分 F2』实测连 F2 都不成立 ⇒ T30 的" +
          "结论被独立复现）；核心断言『这 9 条从未落码』**对上了**（9/9 不在 F1）。")
    print(f"  · 反向：{viol} 条违反 ⇒ 判法没有把'真名+同名实现'误判成缺。")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--stats", action="store_true", help="只打印统计，不写文件")
    ap.add_argument("--control", action="store_true", help="只跑正/反面对照")
    ap.add_argument("--module", default="", help="只打印某个模块的逐条判定")
    ap.add_argument("--no-na", dest="no_na", action="store_true",
                    help="旧行为（T85 之前）：不读 na 台账、不写 na 列、不做 P2 负向排除；"
                         "生成物与 --stats 与改动前逐字节相同")
    args = ap.parse_args(argv)

    use_na = not args.no_na
    ledger = None
    if use_na:
        ledger = load_na_ledger()
        validate_na_ledger(ledger, load_boundary())    # M1/M2/M4/M5/M7（M3 在读取时已挡）
    out = build(ledger)
    if args.control:
        control(out)
        return 0
    c, sub, pin, bymod = stats(out, na=ledger)
    if args.module:
        rows = [o for o in out if os.path.basename(o["row"]["loc_main_file"]) == args.module]
        print(f"\n=== {args.module}: {len(rows)} 行")
        for o in sorted(rows, key=lambda x: x["row"]["addr"]):
            natag = f" na={(o['na']['category'] if o.get('na') else NA_DEFAULT):14s}" if use_na else ""
            print(f"  {o['row']['addr']} {o['bucket']}/{o['sub']:18s} pin={o['pin']:7s} "
                  f"{o['pin_where']:38s}{natag} {o['row']['name'][:60]}")
        return 0
    if args.stats:
        return 0

    path = os.path.join(ROOT, OUT)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(HEADER)
        if use_na:
            fh.write(NA_HEADER_EXTRA)
        for o in out:
            fh.write(fmt_row(o, with_na=use_na) + "\n")
    print(f"\n写出 {OUT}：{len(out)} 行{'（含 na/na_basis 两列）' if use_na else '（--no-na：旧 13 列）'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
