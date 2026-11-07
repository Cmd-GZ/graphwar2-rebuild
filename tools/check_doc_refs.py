#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""docs/16-client.md 的小节编号与全仓交叉引用校验。

    python3 tools/check_doc_refs.py        # 校验；退出码 0 = 通过
    python3 tools/check_doc_refs.py -v     # 连白名单条目（含理由）逐条打印

两项检查（对应 dispatch/07-docs-renumber.md 的验收口径）：

① docs/16-client.md 的 `## NN.` 标题：从 1 开始、连续、无重复、无空洞。
   顺带校验 `### A.B` / `#### A.B.C` 的首段 = 所属顶层小节号 —— 否则"重排"只做了一半
   （2026-11 重排前，§17 的子标题写成 16.x、§40 的写成 39.3.x、§43 的写成 42.x，
   就是这个检查缺失造成的）。

② 全仓每一条"指向 docs/16"的 `§NN` 引用都能落到一个真实标题上（无死链）。
   归属规则：**同一行里 § 之前最近的文档标记**决定它指谁 ——
     `docs/16` / `docs/16-client.md` / `16-client.md` / `` `16` ``  →  docs/16
     `docs/02`、`AGENTS.md`、`archive/…`、`` `137` ``（archive 分篇号）等  →  别的文档
   在 docs/16 自身里，没有标记的 `§` 视为自引用；在别的文件里，"本文件/本文/本节 §…"
   视为该文件自身，不计入。

白名单（EXEMPT）分三类，**每条都打印理由**，不是静默放过：

* `PRE_EXISTING_DEAD` —— 改号**之前**就落不到任何小节的引用。成因已逐条查明：
  docs/16 是从 19 篇 archive 分篇合并出来的，合并时节号前缀（`137` §5.2 那种）
  丢了一大批，于是 `§5.4` 这种号在 docs/16 里找不到对应小节。本任务规定
  "只改编号、不改指向"，所以**保留并登记**，不顺手改成别的节。
* `OUT_OF_SCOPE` —— 文件不在本任务的允许改动清单内（任务书 §5：只许改
  `docs/16-client.md`、其它 `docs/*.md`、`src/**`、本脚本、coverage 生成物）。
  这些文件里的 `§` 引用没有同步；底下 `KNOWN_FIX` 把其中已成死链的那几条的
  正确新号写出来了，便于下一步一次性补。
* `RUNNING_TEXT` —— 正文里当字符串用的 `§`（例如「六条硬键里的 §」），不是引用。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOC16 = ROOT / "docs/16-client.md"

SCAN_SUFFIXES = {".md", ".rs", ".gd", ".py", ".tsv"}
SKIP_DIR_NAMES = {
    ".git", "target", "rsc", "node_modules", ".agent-work",
    ".godot-data", ".godot-cache", ".godot-config", "recovered-src",
}

REF_RE = re.compile(r"§(\d+(?:\.\d+)*)")
HEAD_RE = re.compile(r"^(#{2,4})\s+(\d+(?:\.\d+)*)([.．])?(?=\s|$)")

# --- 文档标记：同一行里决定 §NN 归属 -------------------------------------------------
# (正则, 归属)。'16' = docs/16；'SELF' = 该文件自身；'OTHER' = 别的文档。
MARKERS: list[tuple[re.Pattern, str]] = [
    (re.compile(r"docs/16-client\.md"), "16"),
    (re.compile(r"docs/16(?![-\w])"), "16"),
    (re.compile(r"(?<![\w/-])16-client\.md"), "16"),
    (re.compile(r"`16`"), "16"),
    (re.compile(r"本文件|本文|本节"), "SELF"),
    (re.compile(r"docs/[0-9A-Za-z_\-]+\.md"), "OTHER"),
    (re.compile(r"docs/\d+(?![0-9A-Za-z\-])"), "OTHER"),
    (re.compile(r"AGENTS\.md"), "OTHER"),
    (re.compile(r"archive/[0-9A-Za-z_\-./]+"), "OTHER"),
    (re.compile(r"(?<![\w/])\d{3}-[a-z0-9\-]+\.md"), "OTHER"),
    (re.compile(r"(?<![\w/])\d{2}-[a-z0-9\-]+\.md"), "OTHER"),
    (re.compile(r"`\d{2,3}`"), "OTHER"),            # archive 分篇号：`137` §5.2
    (re.compile(r"(?<![\w`])\d{2,3}(?=\s*§)"), "OTHER"),
]

# --- 白名单 1：改号前就落不到任何小节的引用（只登记，不修） --------------------------
PRE_EXISTING_DEAD: list[tuple[str, str, str]] = [
    ("docs/16-client.md", "§2.3",
     "合并前 `archive/108-client-rebuild.md` §2.3（core_bridge 接缝）/ `archive/140` 的 §2.3；"
     "合并时丢了 `NNN` 前缀。docs/16 的 §2 只有 2.1 一节"),
    ("docs/16-client.md", "§3.1",
     "= `archive/121-visual-fidelity.md` §3.1；丢了 `121` 前缀"),
    ("docs/16-client.md", "§3.2",
     "= `archive/121-visual-fidelity.md` §3.2（「返回键把同一个箭头画了两遍」）；丢了前缀"),
    ("docs/16-client.md", "§3.3",
     "= `archive/121` §3.3 / `archive/137-pixel-fidelity.md` §3.3 / `archive/143-play-flow.md` §3.3；丢了前缀"),
    ("docs/16-client.md", "§3.4",
     "= `archive/135-fire-fx.md` §3.4 / `archive/140-plane-visual.md` §3.4；丢了前缀"),
    ("docs/16-client.md", "§3.6",
     "= `archive/140-plane-visual.md` §3.6（正文自己写着「原 §3.6」）；丢了前缀"),
    ("docs/16-client.md", "§3.7",
     "= `archive/116-kernel-api.md` §3.7（错误码 `bad_arguments`）；丢了前缀"),
    ("docs/16-client.md", "§4.2",
     "= `archive/134-net-client.md` §4.2（断线三种都落 DisconnectedPopUp）；丢了前缀"),
    ("docs/16-client.md", "§4.3",
     "= `archive/130-popups.md` §4.3（同批 124/129/145 也有 §4.3）；丢了前缀"),
    ("docs/16-client.md", "§4.4",
     "= `archive/140-plane-visual.md` §4.4 / `docs/111-audio.md` §4.4（前缀 `docs/111` 也丢了）"),
    ("docs/16-client.md", "§4.6",
     "= `archive/92-ui-inventory.md` §4.6（AddBotPopUp，名字↔难度那条）；丢了 `92` 前缀"),
    ("docs/16-client.md", "§4.5",
     "= `archive/140-plane-visual.md` §4.5 / `docs/111-audio.md` §4.5（前缀 `docs/111` 也丢了）"),
    ("docs/16-client.md", "§4.11",
     "= `archive/92-ui-inventory.md` §4.11（ConfirmPlayExitPopUp）；丢了 `92` 前缀"),
    ("docs/16-client.md", "§5.3",
     "= `archive/126-level-obstacles.md` §5.3（内核注册 dump）/ `archive/135` §5.3；丢了前缀"),
    ("docs/16-client.md", "§5.4",
     "= `archive/126-level-obstacles.md` §5.4（端到端 `4480+1` 点）；丢了前缀"),
    ("docs/16-client.md", "§6.1",
     "= `archive/126-level-obstacles.md` §6.1（客户端自生成 vs 服务端广播）；丢了前缀"),
    ("docs/16-client.md", "§6.2",
     "= `archive/120-hit-test.md` §6.2（状态行原文）；丢了前缀"),
    ("docs/16-client.md", "§6.3",
     "= `archive/120-hit-test.md` §6.3（注册日志）/ `archive/126-level-obstacles.md` §6.3；丢了前缀"),
    ("docs/16-client.md", "§8.4",
     "= `archive/116-kernel-api.md` §8.4；丢了前缀"),
    ("docs/16-client.md", "§1.3",
     "= `archive/135-fire-fx.md` §1.3（屏幕级接管/升级路径）；丢了前缀"),
    ("docs/16-client.md", "§2.2",
     "= `archive/140-plane-visual.md` §2.2（三次 `draw_arc` 的参数）；丢了前缀"),
    ("docs/16-client.md", "§2.6",
     "= `archive/140-plane-visual.md` §2.6（正文自己写着「原 §2.6」）；丢了前缀"),
    ("docs/16-client.md", "§0",
     "指的是 `src/graphwar_long_press_animation.rs` 文件头的 `§0`（源码注释里的节号），"
     "不是文档节号 —— 归属规则把它算进了 docs/16 自引用"),
    ("docs/05-shot-pipeline.md", "§4.10",
     "docs/16 §4 只有 4.1 一个子节，没有 4.10；改号前后都落不到（疑为合并前 archive 分篇节号）"),
    ("docs/05-shot-pipeline.md", "§3.7",
     "docs/16 §3 只有 3.1/3.1.1/3.1.2，没有 3.7；改号前后都落不到"),
    ("docs/09-network.md", "§160",
     "docs/16 从来没有 §160（重排前最大 45、重排后 40）；更早编号体系的遗留"),
    ("docs/10-campaign.md", "§543",
     "docs/16 从来没有 §543；更早编号体系的遗留"),
    ("docs/20-source-recovery.md", "§2.3",
     "docs/16 §2 只有 2.1 一节；`§2.3` 是合并前 `archive/92` 的节号（丢了前缀）"),
    ("src/graphwar_lan_vbox.rs", "§160",
     "docs/16 从来没有 §160（真机 dump 实际在 §2.1.4 LanLobby 那一节）"),
    ("src/_scaffold/graphwar_lan_vbox_class.rs", "§160",
     "docs/16 从来没有 §160（真机 dump 实际在 §2.1.4 LanLobby 那一节）"),
    ("tests/graphwar_lan_vbox.rs", "§160",
     "docs/16 从来没有 §160（真机 dump 实际在 §2.1.4 LanLobby 那一节）"),
]

# --- 白名单 2：文件不在本任务允许改动的清单内 --------------------------------------
OUT_OF_SCOPE: list[tuple[str, str]] = [
    ("docs/HANDOVER.md",
     "任务书 §5 明令不许改（「不许改 … docs/HANDOVER.md」）；里面的 §引用未同步"),
    ("docs/archive/",
     "归档文档（冻结，只读）：保留当时写的编号"),
    ("docs/coverage-matrix.tsv",
     "生成物；这些 §号的源头是 `tools/coverage_matrix.py` 的说明字符串，而该 .py 不在允许清单内"
     "（改它即可让生成物自动跟着变）"),
    ("tests/",
     "不在允许清单内（只许 `docs/*.md` 与 `src/**`）；tests/ 里是验收断言，本任务不动"),
    ("client/",
     "不在允许清单内（只许 `docs/*.md` 与 `src/**`）"),
    ("tools/",
     "不在允许清单内（只许 `docs/*.md` 与 `src/**`）"),
    ("dispatch/",
     "任务书（派发用，不是产品文档），不改"),
    ("README.md",
     "不在允许清单内（任务书写的是「其它 `docs/*.md`」）"),
]

# 上表文件里，重排后已成死链的那些 —— 给出正确新号，便于下一步一次性补。
KNOWN_FIX: dict[tuple[str, str], str] = {
    ("client/tools/assemble_check.gd", "§42"): "§38",
    ("client/tools/function_drawer_check.gd", "§42"): "§38",
    ("tests/graphwar_player_screen_control.rs", "§25.9"): "§22.9",
    ("docs/coverage-matrix.tsv", "§25.9"): "§22.9",
    ("tools/coverage_matrix.py", "§25.9"): "§22.9",
    ("docs/HANDOVER.md", "§2.3"): "§22.3？(无从判定：§2 无 2.3；见 PRE_EXISTING_DEAD 同类)",
    ("docs/archive/141-sim-turns.md", "§37"): "§33",
    ("docs/archive/141-sim-turns.md", "§37.5"): "§33.5",
    ("docs/archive/81-replay-validation.md", "§37"): "§33",
}


def rel(p: Path) -> str:
    return str(p.relative_to(ROOT))


def markers(line: str) -> list[tuple[int, int, str]]:
    """一行里所有文档标记：(起点, 终点, 归属)。重叠时 '16'/'SELF' 优先于 'OTHER'。"""
    ents: dict[int, tuple[int, str]] = {}
    for rx, kind in MARKERS:
        for m in rx.finditer(line):
            cur = ents.get(m.start())
            if cur is None:
                ents[m.start()] = (m.end(), kind)
            elif cur[1] == "OTHER" and kind in ("16", "SELF"):
                ents[m.start()] = (m.end(), kind)
    return sorted((s, e, k) for s, (e, k) in ents.items())


def iter_files() -> list[Path]:
    out = []
    for p in sorted(ROOT.rglob("*")):
        if not p.is_file() or p.suffix not in SCAN_SUFFIXES:
            continue
        if any(part in SKIP_DIR_NAMES for part in p.relative_to(ROOT).parts):
            continue
        if p.resolve() == Path(__file__).resolve():
            continue  # 本脚本自己的说明文字里全是 § 举例
        out.append(p)
    return out


def check_headings() -> list[str]:
    """① 顶层编号连续 + 子标题首段与所属小节一致。"""
    errs: list[str] = []
    lines = DOC16.read_text(encoding="utf-8").splitlines()
    sec_nums: list[int] = []
    sec_line: dict[int, int] = {}
    cur_sec: int | None = None
    cur_sub: str | None = None
    for ln, line in enumerate(lines, 1):
        m = HEAD_RE.match(line)
        if not m:
            continue
        level, num = len(m.group(1)), m.group(2)
        parts = num.split(".")
        if level == 2:
            if len(parts) != 1:
                errs.append(f"docs/16-client.md:{ln}: 顶层标题带子号 `{num}`")
                continue
            if cur_sec is not None and int(num) != cur_sec + 1:
                errs.append(
                    f"docs/16-client.md:{ln}: `## {num}.` 前面是 `## {cur_sec}.` —— 不连续"
                )
            if int(num) in sec_line:
                errs.append(
                    f"docs/16-client.md:{ln}: `## {num}.` 重复（首次在第 {sec_line[int(num)]} 行）"
                )
            sec_line.setdefault(int(num), ln)
            sec_nums.append(int(num))
            cur_sec, cur_sub = int(num), None
        else:
            if cur_sec is None:
                errs.append(f"docs/16-client.md:{ln}: `{num}` 出现在第一个 `## ` 之前")
                continue
            if int(parts[0]) != cur_sec:
                errs.append(
                    f"docs/16-client.md:{ln}: `{'#' * level} {num}` 的首段 {parts[0]} "
                    f"≠ 所属小节 {cur_sec}（上层的标题号没跟着改）"
                )
            if level == 3:
                cur_sub = num
            elif cur_sub and ".".join(parts[:2]) != cur_sub:
                errs.append(
                    f"docs/16-client.md:{ln}: `#### {num}` 的前两段 ≠ 所属子节 `{cur_sub}`"
                )
    if not errs and sec_nums != list(range(1, len(sec_nums) + 1)):
        errs.append(f"docs/16-client.md: 顶层编号不是 1..{len(sec_nums)}：{sec_nums}")
    return errs


def load_headings() -> set[str]:
    heads = set()
    for line in DOC16.read_text(encoding="utf-8").splitlines():
        m = HEAD_RE.match(line)
        if m:
            heads.add(m.group(2))
    return heads


def out_of_scope_reason(path: str) -> str | None:
    for prefix, why in OUT_OF_SCOPE:
        if path == prefix or path.startswith(prefix):
            return why
    return None


def check_refs(verbose: bool) -> tuple[list[str], list[str]]:
    """② 全仓 docs/16 §引用无死链（白名单逐条给理由）。"""
    heads = load_headings()
    errs: list[str] = []
    notes: list[str] = []
    dead_by_file: dict[str, list[tuple[int, str]]] = {}
    exempt_dead: list[tuple[str, int, str, str]] = []
    exempt_scope: list[tuple[str, int, str, str]] = []
    total = resolved = 0

    for p in iter_files():
        path = rel(p)
        try:
            lines = p.read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError as exc:  # pragma: no cover
            errs.append(f"{path}: 读不了（{exc}）")
            continue
        scope_why = out_of_scope_reason(path)
        for ln, line in enumerate(lines, 1):
            if "§" not in line:
                continue
            ms = markers(line)
            for m in REF_RE.finditer(line):
                prev = [t for t in ms if t[0] < m.start()]
                if path == "docs/16-client.md":
                    # 本篇里「本节 §X」也是自引用
                    owned = (not prev) or prev[-1][2] in ("16", "SELF")
                else:
                    owned = bool(prev) and prev[-1][2] == "16"
                if not owned:
                    continue
                total += 1
                ref = "§" + m.group(1)
                if ref[1:] in heads:
                    resolved += 1
                    continue
                # 死链：先看是不是改号前就死的，再看文件在不在允许清单内
                why = next(
                    (w for f, r, w in PRE_EXISTING_DEAD if f == path and r == ref), None
                )
                if why:
                    exempt_dead.append((path, ln, ref, why))
                    continue
                if scope_why:
                    fix = KNOWN_FIX.get((path, ref))
                    note = scope_why + (f"；正确的新号是 {fix}" if fix else "")
                    exempt_scope.append((path, ln, ref, note))
                    continue
                dead_by_file.setdefault(path, []).append((ln, ref))

    for path, items in dead_by_file.items():
        for ln, ref in items:
            errs.append(f"{path}:{ln}: `{ref}` 找不到对应小节（死链，且不在白名单里）")

    notes.append(f"② 指向 docs/16 的 §引用：共 {total} 条，解析成功 {resolved} 条，"
                 f"白名单豁免 {len(exempt_dead) + len(exempt_scope)} 条，死链 {sum(len(v) for v in dead_by_file.values())} 条")
    if verbose:
        for path, ln, ref, why in exempt_dead:
            notes.append(f"   [既有死链] {path}:{ln} `{ref}` —— {why}")
        for path, ln, ref, why in exempt_scope:
            notes.append(f"   [不在允许清单] {path}:{ln} `{ref}` —— {why}")
    else:
        agg: dict[str, int] = {}
        for path, ln, ref, why in exempt_dead + exempt_scope:
            agg[path] = agg.get(path, 0) + 1
        for path, n in sorted(agg.items()):
            notes.append(f"   豁免 {n} 条：{path}（-v 看每条理由）")
    return errs, notes


def main(argv: list[str]) -> int:
    verbose = "-v" in argv or "--verbose" in argv
    print(f"仓库根：{ROOT}")
    print("① docs/16-client.md 的小节编号")
    errs1 = check_headings()
    if errs1:
        for e in errs1:
            print("   ✗", e)
    else:
        nums = [int(m.group(2)) for m in
                (HEAD_RE.match(l) for l in DOC16.read_text(encoding="utf-8").splitlines()) if m and len(m.group(1)) == 2]
        print(f"   ✓ `## NN.` 连续无重复无空洞：1..{len(nums)}（{len(nums)} 节）")
        subs = sum(1 for l in DOC16.read_text(encoding="utf-8").splitlines()
                   if (m := HEAD_RE.match(l)) and len(m.group(1)) > 2)
        print(f"   ✓ 子标题首段与所属小节一致（{subs} 个 `###`/`####`）")

    print("② 全仓指向 docs/16 的 §引用")
    errs2, notes = check_refs(verbose)
    for n in notes:
        print("   " + n)
    for e in errs2:
        print("   ✗", e)
    if not errs2:
        print("   ✓ 无死链（白名单条目逐条有理由）")

    bad = len(errs1) + len(errs2)
    print()
    print("结论：" + ("通过（退出码 0）" if bad == 0 else f"失败（{bad} 项）"))
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
