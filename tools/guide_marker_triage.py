#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""guide_marker_triage.py —— `docs/guide/**` 的 `[逐位]` 标记分档台账（任务 T68）

## 这个工具解决什么问题

`docs/guide/README.md` §2 在 2026-11-27（T61）把 `[逐位]` 的口径收紧到 `AGENTS.md` §0.3 的原义：
**`逐位` = 与真二进制逐字节 / 逐位对拍（值级）**；"能指到反汇编地址 + 读懂那几条指令的结构"
只是 `[观测]`。T61 只改了定义、**没有回改历史条目**（那需要逐条重新对拍）。
本工具把每条 `[逐位]` 判成四档，产出 `fixtures/guide_marker_triage.tsv`，作为回改的可复算依据。

## 判据（唯一口径，先读完再用）

只问一个问题：**这条 `[逐位]` 的出处里，有没有"能从真二进制里读出字节/数值、或与真实记录逐字节相同"
的东西？** 有 ⇒ `T`；只有地址/指令结构/样本统计 ⇒ `O`；什么都没有 ⇒ `I`。

* `T-真逐位`（值级对拍结果，读者可复算）。命中任一：
  - `T1` 8 字节 f64 位型：`0x` + 16 位十六进制（如 `0x3f847ae147ae147b`）；
  - `T2` FDE 长度/边界字节读数：`size 0x..`、`.eh_frame`、`FDE = 0x..–0x..` 且带长度；
  - `T3` 明确写"与真二进制 / 真实记录**逐字节或逐位相同**"，并给出结果：
    `逐字节相同/一致`、`逐位相等`、`逐字符相同`、`N/N 个点一致`、`f64::to_bits`、
    `unicorn 直调`、`bit_exact`；
  - `T4` 指向 `fixtures/` 里已入库的**值级**真值夹具（`eval.tsv`/`path.tsv`/`bit_exact_*` 等）。
  - ⚠️ **反例（不算 T）**：只写"逐条指令"/"逐条汇编"（那是读指令，不是对拍）；
    只写"两路证据对上"（S/C 与 R 交叉）；只写 `N/N` 份回放（那是样本统计 ⇒ `O`）；
    只写"cargo test / selftest 过了"（AGENTS §0.2 不算证据）。
* `O-应降为观测`：有可复核指针，但没有值级对拍结果。命中任一：
  - `O1` 反汇编地址 / 指令助记符 / 函数名 / 函数指针槽 / 签名（"读懂结构"）；
  - `O2` 真实报文 / 真机 / 真帧 / frida / 冻结语料 / 回放的**样本统计**（`N/N`，分母是样本）；
  - `O3` 只命名 `.rodata`/字面量/名字表的位置，没把字节读成值；
  - `O4` 指到我们自己的 `src/…:行` 或其它文档的 §（转引，本身就是二手的）。
* `I-应降为推断`：块里**没有任何**可复核指针（无地址、无 `docs/NN §`、无 `src/…`、无 `fixtures/`、
  无样本数），只有结论；或证据是"我算了一下"这类人手推算。
* `U-判不了`：块里同时出现地址和一句"一致/相同"，但**没说和谁比**，且没有样本数
  ⇒ 无法判断它是值级对拍还是结构比对（保守起见不猜）。
* `M-元引用`（**不计入四档分母**）：讲**标记体系本身**或**章节头/统计口径**的 `[逐位]`
  —— README §2 的定义表与统计表、各章开头的"证据标记含义见…"、
  `docs/guide/05` 章的"本章几乎每一条都是 [推断]"、`bot` 篇的标记说明。
  它们不是"结论"，回改会破坏标记体系的自述。

### 分块规则（决定"出处"落在哪）

* 表格行：整行（手册的写法是"结论在左、出处+标记在右"）；
* 围栏代码块（```）里的行：**只取该行**（每行是独立一条，不能把上一行的字节混进来）；
* 列表项/段落：向上合并到空行/标题（最多 3 行）+ 向下 1 行（续行）。
* 块里没有指针时，看**最近的章节导语**（向上 40 行内的非表格非空行）是否声明了证据链
  （`证据|来自|出处|对上|一致|回放|语料|反汇编|字面量`）：有 ⇒ 该条按 `O4`（章节级转引），
  仍然没有 ⇒ `I`。

## 已知口径边界（不要偷偷放宽）

1. 任务书正文把 `O` 说成"只有地址/指令/`.rodata` 常量"。本工具更细：
   "把 `.rodata` 的 8 字节读出来、写成位型/值" = `T`（README §2 的第一个示例就是这么算的）；
   "只说常量在 `0x…`、或只写十进制值不给字节读法" = `O3`。差别在 TSV 的 `why` 里逐条写明。
2. 语料统计（`N/N`）判 `O`，不判 `T` —— 分母是样本不是二进制输出（README §2 第三个示例）。
3. `N/N 与真二进制逐字节/随机点一致` 判 `T` —— 分母是**真二进制的输出**，这是对拍（README §2）。
4. 分档**只判证据强度，不判结论对错**（对错是 T60/T61 那条线：`docs/06` §107/§108）。

## 用法（可复算）

```bash
python3 tools/guide_marker_triage.py --self-test                 # 正/负对照，必须全 PASS
python3 tools/guide_marker_triage.py --emit /tmp/triage.tsv      # 出当前工作树的台账
python3 tools/guide_marker_triage.py --rev <pre-change-commit> \
        --emit /tmp/triage-before.tsv                            # 回改之后复算（取旧版本）
python3 tools/guide_marker_triage.py --apply --rev <c> --dry-run # 只打印将要改的行
```

`--rev` 让台账**在回改之后仍可复算**：改完标记再跑，分母就变了，台账无法核对。
`--apply` 只做两件事：`O` → 把该处 `[逐位]` 换成 `[观测]`；`I` → 换成 `[推断]` 并补"（缺：…）"。
`T`/`U`/`M` 一个字符都不动。

## 输出列

`id | file | line | class | rule | evidence_kind | excerpt | why | how_to_upgrade`
"""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys

GUIDE_DIR = "docs/guide"
CONTENT_FILES = [
    "01-对局规则.md",
    "02-函数与弹道.md",
    "03-关卡与战役.md",
    "04-AI与难度.md",
    "05-武器与技巧.md",
    "06-时钟与回合推进.md",
    "07-联机协议.md",
    "08-回放格式.md",
    "bot-行为说明.md",
]
README_FILE = "README.md"
MARKER = "[逐位]"

# ---------------------------------------------------------------- T 规则（值级对拍）
T_RULES = [
    ("T1", r"0x[0-9a-fA-F]{16}\b|fff0000000000000",
     "给了 8 字节 f64 位型（真 .so 的 .rodata/常量读出来的字节）"),
    ("T2", r"\bsize 0x[0-9a-f]+\b|\.eh_frame|FDE\s*=.*0x[0-9a-f]+.*0x[0-9a-f]+",
     "给了 FDE 长度/边界字节读数（从真 .so 读出）"),
    ("T3b", r"逐字节(相同|一致|比对上|全等|写出)|逐位(相同|一致|相等)|逐字符(相同|一致)|"
            r"点序列逐位相等|f64::to_bits|to_bits\(\)|unicorn\s*直调|bit_exact",
     "写了与真二进制/真实记录逐字节·逐位·逐字符相同的结果"),
    ("T3c", r"\d+\s*/\s*\d+\s*(个)?(随机点|点)(一致|相同)|(随机点|点)\s*\d+\s*个?(一致|相同)",
     "写了与真二进制输出在同输入 N 个点上的值级一致"),
    ("T3d", r"(与真二进制|与真实记录|与真机|和真二进制)[^\n]{0,16}(逐字节|逐位|逐字符)|"
            r"(逐字节|逐位|逐字符)[^\n]{0,8}(解出|写出)",
     "写了'与真二进制/真实记录逐字节·逐位·逐字符'的对拍结果"),
    ("T4", r"fixtures/(bit_exact|eval|path|parse)[\w.]*|fixtures/[\w./-]*bit_exact",
     "指向 fixtures/ 里已入库的值级真值夹具"),
    ("T4b", r"(eval|path|parse)\.tsv",
     "指向 unicorn 直调真函数生成的值级夹具（eval.tsv/path.tsv）"),
]

# ---------------------------------------------------------------- O 规则（有指针，无值级结果）
O_RULES = [
    ("O1", r"0x[0-9a-fA-F]{5,}|函数指针|槽|签名|`(mov|lea|cmp|test|je|jne|jbe|jmp|call|add|sub|dec|inc|"
           r"mulsd|addsd|subsd|sqrtsd|xorps|ucomisd|cvtsi|imul|ror|rev16|rol|setne|cmove|shl|shr|and|or|xor|ret)\b",
     "只有地址/指令结构/函数签名（读懂了代码路径，没有值级对拍结果）"),
    ("O2", r"\d+\s*/\s*\d+\s*(份|条|关|个|帧|次|发|局|例|行|种)|(份回放|条真帧|冻结语料|语料\s*\d+)",
     "只有真实报文/真机/冻结语料的样本统计（分母是样本，不是二进制输出）"),
    ("O2b", r"frida|真机读数|真帧|抓包|帧差|像素差|运行时(读内存|抽样)",
     "证据是真机/真帧/抓包/运行时观测"),
    ("O3", r"\.rodata|\.data\.rel\.ro|名字表|字面量|字符串表|立即数",
     "只命名了 .rodata/字面量/立即数的位置，没把字节读成值"),
    ("O4", r"docs/(archive/)?\d|src/[\w./-]+\.rs|src/_scaffold/[\w./-]+\.rs|tools/[\w./-]+\.py|"
           r"`?docs/`?\s*§|§\d",
     "只转引到我们自己的 src/文档某节（二手出处）"),
]

# ---------------------------------------------------------------- M 规则（元引用）
M_PATTERNS = [
    r"证据标记含义见",
    r"本章的 `\[逐位\]` 条目全部来自",
    r"本章几乎每一条都是",
    r"本篇原用粗体",
    r"证据标记（2026-11-27 补齐",
    r"^\| `\[逐位\]` \|",           # README 定义表
    r"本手册里 `\[逐位\]` 的两种情况",
    r"\* \"常量 `1e6` 在",
    r"^\| 章节 \| `\[逐位\]`",
    r"现逐条补齐；它的",
    r"^\*\*⚠️ 口径说明",
    r"⚠️ \*\*读表注意\*\*",
    r"^\| 章节 \|",
]

U_RULE = ("U1", r"(一致|相同|符合)",
          "同时出现地址和一句'一致/相同'，但没说比较对象、也没有样本数 ⇒ 判不了")


def _git_show(rev: str, path: str) -> str | None:
    try:
        out = subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, check=True)
        return out.stdout.decode("utf-8")
    except subprocess.CalledProcessError:
        return None


def _read(path: str, rev: str | None) -> str | None:
    if rev:
        return _git_show(rev, path)
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as fh:
        return fh.read()


SECTION_RE = re.compile(r"^(#{1,6})\s+(.*)$")
LIST_RE = re.compile(r"^\s*(?:[-*+]|\d+\.)\s+")
POINTER_RE = re.compile(
    r"0x[0-9a-fA-F]{4,}|docs[/ ]|src/|fixtures/|tools/|§\d|`?[A-Za-z_]+\.(rs|gd|tscn|py)`?|"
    r"\d+\s*/\s*\d+|回放|语料|真帧|抓包|frida|反汇编|指令|\.rodata|\.data\.rel\.ro|"
    r"字面量|名字表|立即数|✔")
CHAIN_RE = re.compile(r"证据|来自|出处|对上|一致|回放|语料|反汇编|字面量|立即数|真值|对拍")


def paragraphs(lines: list[str]) -> list[tuple[int, int]]:
    """返回 [(start, end)) 的段落区间（连续非空行）。"""
    out, start = [], None
    for i, line in enumerate(lines):
        if line.strip():
            if start is None:
                start = i
        else:
            if start is not None:
                out.append((start, i))
                start = None
    if start is not None:
        out.append((start, len(lines)))
    return out


def fence_mask(lines: list[str]) -> list[bool]:
    """标记哪些行在围栏代码块内（``` 或 ~~~）。"""
    out, inside, fence = [], False, None
    for line in lines:
        s = line.strip()
        m = re.match(r"^(```+|~~~+)", s)
        if m:
            if not inside:
                inside, fence = True, m.group(1)[:3]
                out.append(True)          # 围栏本身算块内，避免被当段落
                continue
            elif s.startswith(fence):
                inside, fence = False, None
                out.append(True)
                continue
        out.append(inside)
    return out


def block_for(lines: list[str], idx: int, in_fence: list[bool]) -> str:
    line = lines[idx]
    if in_fence[idx] or line.lstrip().startswith("|"):
        return line
    start, steps = idx, 0
    while start - 1 >= 0 and steps < 3:
        prev = lines[start - 1]
        if not prev.strip() or SECTION_RE.match(prev) or prev.lstrip().startswith("|") \
                or in_fence[start - 1]:
            break
        start -= 1
        steps += 1
    end, steps = idx, 0
    while end + 1 < len(lines) and steps < 1:
        nxt = lines[end + 1]
        if not nxt.strip() or SECTION_RE.match(nxt) or nxt.lstrip().startswith("|") or in_fence[end + 1]:
            break
        if LIST_RE.match(nxt):
            break
        end += 1
        steps += 1
    return " ".join(l.strip() for l in lines[start:end + 1])


def section_intro(lines: list[str], idx: int, in_fence: list[bool]) -> str:
    """向上 40 行内的最近一条非表格、非空、非代码行（章节导语）。"""
    steps = 0
    for j in range(idx - 1, -1, -1):
        if steps > 40:
            break
        steps += 1
        s = lines[j].strip()
        if not s or in_fence[j] or s.startswith("|"):
            continue
        return s
    return ""


def classify(block: str, intro: str) -> tuple[str, str, str]:
    # 1) 值级对拍结果
    for rid, pat, why in T_RULES:
        if re.search(pat, block):
            return "T", rid, why
    has_ptr = bool(POINTER_RE.search(block))
    # 2) 有指针、无值级结果
    if has_ptr:
        for rid, pat, why in O_RULES:
            if re.search(pat, block):
                # U：有地址 + 含糊'一致'，既无样本数也无值级表述
                if rid == "O1" and re.search(U_RULE[1], block) \
                        and not re.search(r"\d+\s*/\s*\d+", block) \
                        and not re.search(r"(逐字节|逐位|逐字符|位型|size 0x)", block):
                    return "U", U_RULE[0], U_RULE[2]
                return "O", rid, why
        return "O", "O4", "有指针但没有值级对拍结果"
    # 3) 块里没指针：看章节导语是否声明了证据链
    if intro and CHAIN_RE.search(intro):
        return "O", "O4", "块内无指针，靠章节导语声明的证据链（二手/结构级）"
    return "I", "I1", "块内没有任何可复核指针（无地址/夹具/样本数），只有结论"


HOW_TO_UPGRADE = {
    "O": "① 做一次值级对拍：unicorn 直调该函数（或从真 .so 读该字节）并与我方输出比 `f64::to_bits()`；"
         "② 或指到 `fixtures/` 里已有的值级夹具行（`eval.tsv`/`path.tsv`/`bit_exact_*.tsv`）。样本统计不算",
    "I": "先补一个可复核指针（反汇编地址 / `.rodata` 偏移+长度 / 一份真语料样本），再按 O 的升级路径做值级对拍",
    "U": "说清那句'一致/相同'是和谁比的：对真二进制 ⇒ 写位型升 T；对语料/真机 ⇒ 落 O（观测）",
    "T": "—",
    "M": "—（元引用，不是结论）",
}

# 人工改判表（自动判据在这几处过严或过松；每行都给出理由，评审可逐条核）
# key = "文件名:行号"（行号取自 --rev 的那一版；回改只换标记不改行数 ⇒ 行号稳定）
OVERRIDES: dict[str, tuple[str, str, str]] = {
    # —— 过松：自动判据把"字面量/逐字节"算成 T，但该条的结论本身是结构/顺序级
    # —— 过严：结论本身就是"字节事实"（偏移 + 长度 + 内容都给了）⇒ 补回 T
    "01-对局规则.md:286": ("T", "T2b",
        "结论 = 载荷恰好 1 个字段；证据 = `mov r9d,1` 立即数 + `.rodata` 的名字串内容"
        "（`struct TurnLimitInfoEvent with 1 element`）+ 语料 21/21 键集合 —— 前两项是真二进制里的字节/立即数"),
    "03-关卡与战役.md:219": ("T", "T2b",
        "结论 = 存档路径常量；证据给了 `.rodata` 偏移 `0x1bfe28` + 长度 `0x14` = 20 + **内容**"
        "`user://campaign.save`（三段齐了，读者可从 .so 截 20 字节逐字节比）"),
    "08-回放格式.md:20": ("T", "T2b",
        "结论 = 每行两个字段名；证据给了 x86/arm64 **两套** `.rodata` 偏移 + 字段名长度（5 / 4 字节）"
        "—— 偏移 + 长度 + 内容都在，可逐字节比"),
    "08-回放格式.md:36": ("T", "T2b",
        "结论 = 路径字面量内容与长度；证据给了 `.rodata 0x19bdef` + `mov w1,#0xf`（15）+ 内容"
        "`user://replays/` —— 偏移 + 长度 + 内容，可逐字节比"),
    "07-联机协议.md:171": ("O", "O1",
        "结论是『版本串是字面量、不是拼接』（结构级）；`docs/09 §11.6` 给的是指令地址，"
        "不是 `.rodata` 字节读数 ⇒ 观测"),
    "02-函数与弹道.md:85": ("T", "T3c",
        "§5.1 表是『把值喂进真函数 `0x4eb9d0` 的输出』（§5 导语 + `docs/03` §11）；"
        "边界值的输出就是值级结果 ⇒ T（行内缺『来自喂值』的注解，已补出处）"),
    "02-函数与弹道.md:86": ("T", "T3c", "同上：`log(1e-320)` 的输出值来自真函数喂值 ⇒ T"),
    "02-函数与弹道.md:87": ("T", "T3c", "同上：`log/ln` 的四个输出值来自真函数喂值 ⇒ T"),
    "07-联机协议.md:25": ("O", "O2b",
        "『逐字节写出』说的是版本串字面量，而本条结论是**握手 7 条的顺序**（抓包观测），"
        "顺序不是值级对拍对象 ⇒ 观测"),
    "README.md:175": ("O", "O4",
        "本条是转引（`src/…` 行号 + 文档 + `fixtures/ticks.tsv` 113 样本）；"
        "样本表中位数不是值级夹具 ⇒ 观测"),
    # —— 过严/块边界：档位声明单独成行，证据在上一张表/上一段
    "02-函数与弹道.md:69": ("O", "O3",
        "给了 `e`/`n`(π) 的 f64 全精度值（等价于 8 字节读数），但没给 `.rodata` 地址 ⇒ 观测；补地址即可升 T"),
    "02-函数与弹道.md:183": ("O", "O3",
        "小节标题声明；正文表只给 `.rodata` 地址、没把 8 字节读成值 ⇒ 观测"),
    "02-函数与弹道.md:202": ("O", "O4",
        "两个常量在 §6.2 表里只有 `.rodata` 地址（观测级），本条是乘法等价说法 ⇒ 观测"),
    "03-关卡与战役.md:99": ("O", "O3",
        "参数表只给函数入口地址 + 立即数位置，是结构级 ⇒ 观测"),
    "03-关卡与战役.md:278": ("O", "O1",
        "小节标题声明；正文是逐条指令（`docs/150` §4）⇒ 观测"),
    "04-AI与难度.md:51": ("O", "O1",
        "参数表是反汇编读出的字段偏移 ⇒ 观测（无值级对拍结果）"),
    "04-AI与难度.md:36": ("O", "O2b",
        "`[逐位]` 指的是上方参数表（反汇编），`[观测]` 是 replay 超限比例 ⇒ 观测"),
    "07-联机协议.md:138": ("O", "O1", "枚举取值集合来自 `VARIANTS` 表 + `mov ecx,N`（指令级）⇒ 观测"),
    "07-联机协议.md:139": ("O", "O1", "同上：枚举取值集合来自指令级证据 ⇒ 观测"),
    "07-联机协议.md:140": ("O", "O1", "同上 ⇒ 观测"),
    "07-联机协议.md:141": ("O", "O1", "同上 ⇒ 观测"),
    "07-联机协议.md:142": ("O", "O1", "同上 ⇒ 观测"),
    "07-联机协议.md:143": ("O", "O1", "同上 ⇒ 观测"),
    "07-联机协议.md:144": ("O", "O1", "同上 ⇒ 观测"),
    "05-武器与技巧.md:22": ("O", "O4", "转引 02 §7 的机制（`c = y0 − f(±x0)`；本条无指针）⇒ 观测"),
    "05-武器与技巧.md:23": ("O", "O4", "转引 02 §6 的采样机制 ⇒ 观测"),
    "05-武器与技巧.md:24": ("O", "O4", "转引 01 §5/02 §11 的掩膜机制 ⇒ 观测"),
    "05-武器与技巧.md:31": ("O", "O4", "转引 02 §6.2 的棋盘常量（该处只有 `.rodata` 地址）⇒ 观测"),
    "05-武器与技巧.md:48": ("O", "O4", "转引机制 A（02 §7）⇒ 观测"),
    "05-武器与技巧.md:49": ("O", "O4", "转引 02 §6.3 的点预算 ⇒ 观测"),
    "05-武器与技巧.md:50": ("O", "O4", "转引 02 §7 的平坦直线机制 ⇒ 观测"),
    "05-武器与技巧.md:81": ("O", "O4", "转引 02 §6.3 的点预算 ⇒ 观测"),
    "05-武器与技巧.md:99": ("O", "O4", "转引 02 §6.1 的二分机制（`docs/archive/91`）⇒ 观测"),
    "05-武器与技巧.md:100": ("O", "O4", "转引 02 §6.1 的不收敛收线 ⇒ 观测"),
    "05-武器与技巧.md:130": ("O", "O4", "转引 02 §5 的 NaN 清单 ⇒ 观测"),
    "05-武器与技巧.md:164": ("O", "O4", "转引 02 §11 的掩膜机制 ⇒ 观测"),
    "05-武器与技巧.md:185": ("O", "O4", "转引 04 的进化搜索机制 ⇒ 观测"),
    "05-武器与技巧.md:221": ("O", "O4", "转引 02 §8 / 03 §2 的 ODE 模式 ⇒ 观测"),
    "05-武器与技巧.md:229": ("O", "O4", "转引 02 §8 的 RK4 积分 ⇒ 观测"),
    "05-武器与技巧.md:230": ("O", "O4", "转引 02 §8 的初值口径 ⇒ 观测"),
    "05-武器与技巧.md:231": ("O", "O4", "转引 02 §4 怪癖 #7（第三个变量恒 0）⇒ 观测"),
    "05-武器与技巧.md:232": ("O", "O4", "转引 02 §4 怪癖 #6（普通模式 `y` 恒 0）⇒ 观测"),
    "05-武器与技巧.md:233": ("O", "O4", "转引 02 §8（ODE 与普通模式结果不同）⇒ 观测"),
    "05-武器与技巧.md:30": ("O", "O4", "转引 02 §6.3 的两个常量（等价说法）⇒ 观测"),
    "05-武器与技巧.md:193": ("O", "O4", "转引 04 §6 的 25 s 墙钟 ⇒ 观测"),
}

# 正对照：6 条手工确认过的（3 T + 3 O）。判据/分块改了就先把这 6 条跑绿。
POSITIVE_CONTROLS = [
    ("01-对局规则.md", "② 取步长                   服务端常量 = 0.01 (0x3f847ae147ae147b)", "T",
     "给了 0.01 的 f64 位型（真 .so 的 8 字节读出来就是这个）"),
    ("01-对局规则.md", "300 随机点一致", "T",
     "写明了'我方 vs 真二进制'在 300 个随机点上的逐字节相同"),
    ("01-对局规则.md", "size 0x14", "T",
     "FDE 长度是 .so 的字节读数（addr_probe.py fde），原写 16 字节被它更正"),
    ("01-对局规则.md", "`0x46f310` 里发 `TeamInfo{Team2}`", "O",
     "只有指令地址 + 读懂了它发什么事件，无值级比较结果"),
    ("03-关卡与战役.md", "全部 21 行：`S`/`C` 与 385 份回放的 `R`", "O",
     "两路证据交叉 + 回放 21/21 = 观测（README §2 第三个示例）"),
    ("07-联机协议.md", "arm64 `0x45c590 cmp w24,#1,lsl #12`", "O",
     "只有指令地址 + 编码方式（`1<<12`），没有与真二进制输出的值级对拍结果"),
]


def entries(rev: str | None) -> list[dict]:
    out = []
    for name in CONTENT_FILES + [README_FILE]:
        path = f"{GUIDE_DIR}/{name}"
        text = _read(path, rev)
        if text is None:
            continue
        lines = text.split("\n")
        in_fence = fence_mask(lines)
        paras = paragraphs(lines)
        para_of: dict[int, tuple[int, int]] = {}
        for (a, b) in paras:
            for k in range(a, b):
                para_of[k] = (a, b)
        first_h2 = next((i for i, l in enumerate(lines) if re.match(r"^##\s", l)), len(lines))
        for i, line in enumerate(lines):
            if MARKER not in line:
                continue
            block = block_for(lines, i, in_fence)
            a, b = para_of.get(i, (i, i + 1))
            para = "\n".join(lines[a:b])
            is_meta = any(re.search(p, para, re.M) for p in M_PATTERNS)
            # 内容文件的章首导语（第一个 ## 之前）里讲标记体系的段落 = 元引用
            if not is_meta and name in CONTENT_FILES and i < first_h2 and re.search(r"标记|档位|证据", para):
                is_meta = True
            if is_meta:
                cls, rule, why = "M", "M1", "标记体系自述/章节头/统计口径，不是一条结论"
            else:
                cls, rule, why = classify(block, section_intro(lines, i, in_fence))
                key = f"{name}:{i + 1}"
                if key in OVERRIDES:
                    cls, rule, why = OVERRIDES[key]
            out.append({
                "file": path,
                "name": name,
                "line": i + 1,
                "n_occ": line.count(MARKER),
                "class": cls,
                "rule": rule,
                "why": why,
                "excerpt": re.sub(r"\s+", " ", line.strip())[:400],
                "text": line,
            })
    return out


def self_test(rev: str | None) -> int:
    bad = 0
    print("== 正对照（3 T + 3 O，手工确认过）==")
    for fname, needle, want, reason in POSITIVE_CONTROLS:
        path = f"{GUIDE_DIR}/{fname}"
        text = _read(path, rev)
        if text is None:
            print(f"  FAIL {fname}: 读不到文件")
            bad += 1
            continue
        lines = text.split("\n")
        in_fence = fence_mask(lines)
        hits = 0
        for i, line in enumerate(lines):
            if MARKER not in line or needle not in line:
                continue
            cls, rule, why = classify(block_for(lines, i, in_fence), "")
            hits += 1
            ok = cls == want
            print(f"  {'PASS' if ok else 'FAIL'} {fname}:{i+1} [{rule}]->{cls} 期望{want} :: {reason}")
            if not ok:
                print(f"       block={block_for(lines, i, in_fence)[:200]}")
                bad += 1
        if hits == 0:
            print(f"  FAIL {fname}: 找不到正对照串 {needle!r}")
            bad += 1
    print("\n== 负对照（同一套规则必须能判出不同档）==")
    fake = {
        "只有地址": ("- 开火入口在 `0x3c2f20`，里面有 `cmp` 和 `je` [逐位]", "", "O"),
        "只有语料": ("- 冻结语料 45/45 份回放里都是这样 [逐位]", "", "O"),
        "有位型": ("- 步长常量 `0.01` 在 `0x19fc78`，8 字节是 `0x3f847ae147ae147b` [逐位]", "", "T"),
        "逐字节": ("- 我方输出与真二进制逐字节相同 188/188 [逐位]", "", "T"),
        "什么都没": ("- 这一发必定命中 [逐位]", "", "I"),
    }
    for k, (blk, intro, want) in fake.items():
        cls, rule, why = classify(blk, intro)
        ok = cls == want
        print(f"  {'PASS' if ok else 'FAIL'} {k}: [{rule}]->{cls} 期望{want}")
        if not ok:
            bad += 1
    print(f"\n结论：{'全部通过' if bad == 0 else f'{bad} 条不通过'}")
    return 0 if bad == 0 else 1


COLS = ["id", "file", "line", "class", "rule", "n_occ", "excerpt", "why", "how_to_upgrade"]
CHANGE = {"O": "[观测]", "I": "[推断]", "T": "[逐位]", "U": "[逐位]", "M": "[逐位]"}


def emit(rev: str | None, out_path: str | None) -> int:
    rows = entries(rev)
    if not rows:
        print("没有扫到任何 [逐位] —— 路径/分支不对？", file=sys.stderr)
        return 1
    lines = ["\t".join(COLS)]
    stat: dict[str, int] = {}
    for n, r in enumerate(rows, 1):
        stat[r["class"]] = stat.get(r["class"], 0) + 1
        lines.append("\t".join([
            f"G{n:03d}", r["file"], str(r["line"]), r["class"], r["rule"], str(r["n_occ"]),
            r["excerpt"].replace("\t", " "), r["why"].replace("\t", " "),
            HOW_TO_UPGRADE[r["class"]].replace("\t", " "),
        ]))
    body = "\n".join(lines) + "\n"
    if out_path:
        header = (
            "# T68 台账：`docs/guide/**` 里**每一条** `[逐位]` 的档位判定\n"
            "# 判据（可复算）：`tools/guide_marker_triage.py` 文件头「判据」一节；正/负对照用 `--self-test`\n"
            "# 列：" + " | ".join(COLS) + "\n"
            "# class：T-真逐位 / O-应降为观测 / I-应降为推断 / U-判不了 / M-元引用（不计入四档）\n"
            "# 回改规则：O → `[观测]`；I → `[推断]` + 补「缺什么」；T/U/M 不动\n"
        )
        with open(out_path, "w", encoding="utf-8") as fh:
            fh.write(header + body)
        print(f"写入 {out_path}（{len(rows)} 行）", file=sys.stderr)
    else:
        sys.stdout.write(body)
    print("统计：" + " ".join(f"{k}={v}" for k, v in sorted(stat.items())), file=sys.stderr)
    return 0


MISSING_NOTE = "（缺：与真二进制对拍的值级证据）"
DUP_RE = re.compile(r"`\[(观测|推断)\]`(\s*[+/,、]\s*)`\[\1\]`")


def apply(rev: str | None, dry_run: bool) -> int:
    rows = entries(rev)
    changed = {"O": 0, "I": 0}
    per_file: dict[str, list[dict]] = {}
    for r in rows:
        if r["class"] in ("O", "I"):
            per_file.setdefault(r["name"], []).append(r)
    for name, rs in per_file.items():
        path = f"{GUIDE_DIR}/{name}"
        text = _read(path, None if not rev else None)  # 回改作用在当前工作树
        if text is None:
            print(f"跳过 {path}（读不到）", file=sys.stderr)
            continue
        lines = text.split("\n")
        for r in rs:
            i = r["line"] - 1
            line = lines[i]
            if MARKER not in line:
                print(f"  ！{name}:{r['line']} 已不含 [逐位]，跳过", file=sys.stderr)
                continue
            if r["class"] == "O":
                new = line.replace(MARKER, "[观测]")
                changed["O"] += 1
            else:
                new = line.replace(MARKER, "[推断]" + MISSING_NOTE, 1)
                changed["I"] += 1
            new = DUP_RE.sub(lambda m: f"`[{m.group(1)}]`", new)
            lines[i] = new
            print(f"  {name}:{r['line']}  {r['class']}→{CHANGE[r['class']]}")
        if not dry_run:
            with open(path, "w", encoding="utf-8") as fh:
                fh.write("\n".join(lines))
    print(f"改判：O→[观测] {changed['O']} 处；I→[推断] {changed['I']} 处"
          f"{'（dry-run，未写盘）' if dry_run else ''}", file=sys.stderr)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--rev", default=None, help="从某个 commit 读文件（回改后复算台账用）")
    ap.add_argument("--emit", default=None, help="写出 TSV 路径")
    ap.add_argument("--self-test", action="store_true", help="跑正/负对照")
    ap.add_argument("--apply", action="store_true", help="按台账回改标记")
    ap.add_argument("--dry-run", action="store_true", help="配合 --apply：只打印不写盘")
    a = ap.parse_args()
    if a.self_test:
        return self_test(a.rev)
    if a.apply:
        return apply(a.rev, a.dry_run)
    return emit(a.rev, a.emit)


if __name__ == "__main__":
    sys.exit(main())
