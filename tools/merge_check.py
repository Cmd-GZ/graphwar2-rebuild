#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""merge_check.py —— 合并前的一条命令（AGENTS.md「合并纪律 A」的机器版）。

起因：3b 要求合并前**先读报告的 §遗留 / §建议**，可这条纪律靠人记，已经栽过两次。
第二次的代价是第七轮 §2 的标题「画面上什么都没变」，合完才发现。
⇒ 把「读遗留清单 / 核范围 / 查污染 / 四件套」压成一条命令：
缺节、碰禁区、二进制、dispatch 带进 master 都直接非 0 —— 不跑完就不许合。

    python3 tools/merge_check.py --cg origin/CG               # 默认：< 5 s，不跑测试
    python3 tools/merge_check.py --cg origin/CG --run-tests   # 再跑四件套
    python3 tools/merge_check.py --branch CG                  # 没有远端时用本地分支

退出码：0 = 通过；1 = 有阻断项；2 = 用法 / 环境错（不是 git 仓库、分支解析不到）。

判断依据（全部来自分支上的真实数据，不采信报告自述）：

* A 报告体检：分支上**最新**的 ``dispatch/reports/*.md``（文件名日期 + 提交时间排序），
  全文打印「遗留 / 建议」两节；**缺节 = 阻断**（AGENTS 3b）。顺带把作者自曝的
  「退化了 / 没有了 / 画面上没变」这类句子单独列出来。
* B 范围核对：``merge-base..CG`` 的真实 diff，按目录分组，对照禁区
  （``src/lib.rs``、``src/game.rs``、``client/src/**``）。禁区文件要么在报告里被
  「声明 / 允许 / 放行」过，要么 ``--allow <path>`` 显式放行 —— 否则阻断。
  （不从任务书的允许清单反推：那样会把「`src/**` 注释里的…」也算成放行，禁区就废了。）
* C 污染检查：新增文件 > 100 KB、二进制扩展名、git 自己判定的二进制、``dispatch/``
  会不会被带进 master（会 = 阻断，除非 ``--allow-dispatch`` 承认会用路径剔除）。
* D 四件套：``cargo test --offline`` / ``coverage_matrix --check`` / ``check_doc_refs`` /
  三条轴护栏 —— 只有 ``--run-tests`` 才跑，默认只打印命令；红 = 阻断。

只用标准库（与 tools/*.py 其它脚本一致），不写任何文件、不联网。
"""

from __future__ import annotations

import argparse
import fnmatch
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

# ---------------------------------------------------------------- 颜色 / 打印

RED = "\033[1;31m"
YEL = "\033[1;33m"
GRN = "\033[1;32m"
DIM = "\033[2m"
BOLD = "\033[1m"
OFF = "\033[0m"

_COLOR = False


def paint(text, *codes):
    """给一段文字上色；不支持颜色时原样返回（CI / 重定向到文件时不该有转义符）。"""
    if not _COLOR:
        return text
    return "".join(codes) + text + OFF


def head(title, subtitle=""):
    print()
    line = "【%s】" % title
    print(paint(line + (" " + subtitle if subtitle else ""), BOLD))


def bullet(text, mark=""):
    print("  " + (mark + " " if mark else "") + text)


# ---------------------------------------------------------------- git 封装

class EnvError(Exception):
    """环境/用法错误 —— 退出码 2，不当作"合并阻断"。"""


def git(repo, *args, check=True):
    p = subprocess.run(
        ["git", "-C", str(repo)] + list(args),
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if check and p.returncode != 0:
        raise EnvError("git %s 失败：%s" % (" ".join(args), (p.stderr or "").strip()))
    return p


def find_repo(start):
    p = subprocess.run(
        ["git", "-C", str(start), "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if p.returncode != 0:
        return None
    return Path(p.stdout.strip())


def resolve_ref(repo, name):
    """解析成一个能用的 ref；``--cg origin/CG`` 在无远端的仓库里退化成 ``CG``。"""
    cands = [name]
    if name.startswith("origin/"):
        cands.append(name[len("origin/"):])
    for c in cands:
        if git(repo, "rev-parse", "--verify", "--quiet", c + "^{commit}", check=False).returncode == 0:
            return c
    return None


def pick_base(repo, wanted):
    if wanted:
        ref = resolve_ref(repo, wanted)
        if not ref:
            raise EnvError("基线分支解析不到：%s（用 --base 指定）" % wanted)
        return ref
    for cand in ("origin/master", "master", "main"):
        ref = resolve_ref(repo, cand)
        if ref:
            return ref
    raise EnvError("找不到基线分支（origin/master / master / main）；用 --base 指定")


# ---------------------------------------------------------------- A. 报告体检

HEAD_RE = re.compile(r"^(#{1,6})\s+(\S.*?)\s*$")
# 「遗留 / 未做 / 诚实清单」都算遗留节；「建议 / 下一步」都算建议节。容忍两种写法并存。
LEFT_RE = re.compile(
    r"(遗留|未做|诚实清单|待办|未完成|leftover|remaining|open\s+issues?"
    r"|known\s+(issues?|gaps?)|todo)", re.I)
NEXT_RE = re.compile(
    r"(建议|下一步|后续|next\s+steps?|recommend|follow[-\s]?ups?)", re.I)

# 作者自曝：退化 / 丢失 / 画面上没变。第二组是刚栽过的那类（第七轮 §2）。
SELF_RE = re.compile(
    r"(退化|回退|丢失|丢了|没有了|不复现"           # 退化 / 丢失
    r"|什么都没变|没变化|没有任何变化|看不出|看不到"  # 画面上没变
    r"|失效|没生效|不生效|未生效|无效)")


def latest_report(repo, cg, reports_dir):
    """分支上最新的一份报告：先看文件名日期，再看最后一次提交时间。"""
    out = git(repo, "ls-tree", "-r", "--name-only", cg, "--", reports_dir).stdout
    files = [p for p in out.splitlines() if p.endswith(".md")]
    if not files:
        return None, []
    rows = []
    for p in files:
        r = git(repo, "log", "-1", "--format=%ct", cg, "--", p, check=False)
        ctime = int(r.stdout.strip() or 0)
        m = re.search(r"(\d{4})-(\d{2})-(\d{2})", p)
        fdate = (int(m.group(1)), int(m.group(2)), int(m.group(3))) if m else (0, 0, 0)
        rows.append((fdate, ctime, p))
    rows.sort()
    return rows[-1][2], rows


def find_sections(lines):
    """把 markdown 切成 (标题, 起, 止)，只留标题命中「遗留 / 建议」的。"""
    heads = []
    for i, ln in enumerate(lines):
        m = HEAD_RE.match(ln)
        if m:
            heads.append((len(m.group(1)), m.group(2).strip(), i))
    secs = []
    for k, (lvl, title, i) in enumerate(heads):
        end = len(lines)
        for lvl2, _t2, j in heads[k + 1:]:
            if lvl2 <= lvl:
                end = j
                break
        groups = []
        if LEFT_RE.search(title):
            groups.append("leftover")
        if NEXT_RE.search(title):
            groups.append("next")
        if groups:
            secs.append({"groups": groups, "title": title, "start": i, "end": end})
    return secs


def print_section(lines, sec):
    body = lines[sec["start"]:sec["end"]]
    while body and not body[-1].strip():
        body.pop()
    print(paint("  ──── %s " % sec["title"], DIM))
    for ln in body:
        print(ln)
    print()


def scan_self_disclosure(lines):
    hits = []
    for i, ln in enumerate(lines, 1):
        if SELF_RE.search(ln):
            hits.append((i, ln.rstrip()))
    return hits


def section_a(repo, cg, args, report_lines_out):
    """返回 (报告路径, 缺节列表, 自曝句列表)。report_lines_out 供 B 判「报告里声明过」。"""
    path, rows = latest_report(repo, cg, args.reports_dir)
    if not path:
        head("A", "报告体检")
        print(paint("  ❗ 分支上没有 %s/*.md —— 按 AGENTS 3b 不许合" % args.reports_dir, RED))
        return None, ["报告"], []
    text = git(repo, "show", "%s:%s" % (cg, path)).stdout
    lines = text.splitlines()
    report_lines_out.extend(lines)

    secs = find_sections(lines)
    found = {g: [s for s in secs if g in s["groups"]] for g in ("leftover", "next")}

    head("A", "报告体检 —— %s（%d 行，分支上共 %d 份）" % (path, len(lines), len(rows)))

    for key in ("leftover", "next"):
        for sec in found[key]:
            print_section(lines, sec)

    missing = [label for key, label in (("leftover", "§遗留"), ("next", "§建议"))
               if not found[key]]
    if missing:
        print(paint("  ❗ 报告缺 §遗留/§建议 —— 按 AGENTS 3b 不许合", RED))
        bullet(paint("缺：%s（标题里要出现「遗留/未做/诚实清单」与「建议/下一步」）"
                     % "、".join(missing), RED))

    hits = scan_self_disclosure(lines)
    if hits:
        bullet(paint("自曝句 %d 行（作者自己写的退化/没变）—— 这些必须先读：" % len(hits), YEL))
        for i, ln in hits[:12]:
            print("    L%-4d %s" % (i, ln if len(ln) <= 150 else ln[:147] + "..."))
        if len(hits) > 12:
            print("    …… 还有 %d 行（自己 grep 一下）" % (len(hits) - 12))
    else:
        bullet("自曝句：无（不代表没问题，只是没自曝）")
    return path, missing, hits


# ---------------------------------------------------------------- B. 范围核对

FORBIDDEN = [
    (re.compile(r"^src/lib\.rs$"), "装配入口，只有协调者能改"),
    (re.compile(r"^src/game\.rs$"), "核心拟合逻辑"),
    (re.compile(r"^client/src/"), "客户端逻辑"),
]
DISPATCH = "dispatch/"

# 禁区放行只有两条路，**都要留痕**：
#   ① ``--allow <path>``：本次运行的显式承认；
#   ② 报告里写「声明 / 允许 / 放行 + 该路径」：作者自己登记过（本仓库的既有做法）。
# 故意**不从**任务书的「可以改的文件」反推：实测那样会把 `src/**` 这种写法也算成
# 放行（07 号的清单写的是「`src/**` 注释里的 docs/16 §NN」），禁区就形同虚设。
DECL_RE = re.compile(r"(声明|允许|放行|已批准)")


def is_allowed(path, allow, report_lines):
    for a in allow:
        if path == a or fnmatch.fnmatch(path, a) or path.startswith(a.rstrip("/") + "/"):
            return "--allow %s" % a
    for ln in report_lines:
        if path in ln and DECL_RE.search(ln):
            return "报告里声明过"
    return None


def changed_files(repo, mb, cg):
    p = git(repo, "diff", "--name-status", "-M", "--no-color", "%s..%s" % (mb, cg))
    rows = []
    for ln in p.stdout.splitlines():
        if not ln.strip():
            continue
        parts = ln.split("\t")
        st = parts[0]
        if st[:1] in ("R", "C"):
            rows.append((st, parts[2], parts[1]))
        else:
            rows.append((st, parts[1], None))
    return rows


GROUP_ORDER = ["src", "client", "docs", "fixtures", "tools", "tests", "examples",
               "dispatch"]


def group_of(path):
    parts = path.split("/")
    return parts[0] if len(parts) > 1 else "(根)"


def section_b(repo, mb, cg, args, report_lines, state):
    rows = changed_files(repo, mb, cg)
    head("B", "范围核对 —— %d 个文件（merge-base %s）" % (len(rows), mb[:8]))
    if not rows:
        bullet(paint("diff 为空：这个分支没有要合的东西", DIM))
        return []

    # 按目录分组，一行一组
    order = GROUP_ORDER + sorted({group_of(p) for _s, p, _o in rows} - set(GROUP_ORDER))
    counts = {}
    for st, p, _o in rows:
        g = group_of(p)
        c = counts.setdefault(g, {"A": 0, "M": 0, "D": 0, "R": 0, "other": 0})
        key = st[:1]
        c[key if key in ("A", "M", "D") else ("R" if key == "R" else "other")] += 1
    for g in order:
        if g not in counts:
            continue
        c = counts[g]
        bits = ["%s %d" % ({"A": "新增", "M": "改", "D": "删", "R": "改名",
                            "other": "其它"}[k], v)
                for k, v in c.items() if v]
        print("  %-10s %3d 个（%s）" % (g + "/", sum(c.values()), " / ".join(bits)))
        if args.verbose:
            for st, p, old in rows:
                if group_of(p) == g:
                    print(paint("      %-3s %s%s" % (st, p, "  ← " + old if old else ""), DIM))

    # 禁区
    blockers, hits = [], []
    for st, p, _old in rows:
        for rx, why in FORBIDDEN:
            if not rx.search(p):
                continue
            # 目录级禁区（client/src/**）按文件逐个判，方便精确声明
            hits.append((p, why, is_allowed(p, args.allow, report_lines), st))
            break
    if hits:
        print()
        for p, why, how, st in hits:
            if how:
                bullet(paint("已声明，放行：%s（%s；%s）" % (p, why, how), GRN))
            else:
                bullet(paint("❌ 禁区：%s（%s；%s）—— 没声明就退回" % (p, why, st), RED))
                bullet(paint("   要放行：报告里写一句「声明：%s」或 --allow %s" % (p, p), DIM))
                blockers.append("禁区 %s" % p)
    else:
        bullet("禁区（src/lib.rs / src/game.rs / client/src/**）：未碰")
    state["blockers"] += blockers
    return rows


# ---------------------------------------------------------------- C. 污染检查

BIN_EXT = {".so", ".pck", ".png", ".ttf", ".dll", ".dylib", ".exe", ".o", ".a",
           ".zip", ".gz", ".jpg", ".jpeg", ".ogg", ".wav", ".pkl", ".glb", ".bin"}
MAX_KB = 100


def blob_sizes(repo, ref):
    sizes = {}
    for ln in git(repo, "ls-tree", "-r", "-l", ref).stdout.splitlines():
        meta, _, path = ln.partition("\t")
        f = meta.split()
        if len(f) >= 4 and f[3].isdigit():
            sizes[path] = int(f[3])
    return sizes


def git_binary_paths(repo, mb, cg):
    out = set()
    for ln in git(repo, "diff", "--numstat", "-M", "%s..%s" % (mb, cg)).stdout.splitlines():
        parts = ln.split("\t")
        if len(parts) >= 3 and parts[0] == "-" and parts[1] == "-":
            out.add(parts[2])
    return out


def section_c(repo, mb, cg, rows, args, state):
    head("C", "污染检查")
    added = [p for st, p, _o in rows if st.startswith("A")]
    sizes = blob_sizes(repo, cg)
    big = sorted(((p, sizes.get(p, 0)) for p in added if sizes.get(p, 0) > MAX_KB * 1024),
                 key=lambda x: -x[1])
    bin_git = git_binary_paths(repo, mb, cg)
    bin_ext = [p for _st, p, _o in rows
               if os.path.splitext(p)[1].lower() in BIN_EXT]
    bin_all = sorted(set(bin_git) | set(bin_ext))

    bullet("新增 %d 个，> %d KB：%s" % (
        len(added), MAX_KB,
        paint("❌ %d 个（%s）" % (len(big), "，".join(
            "%s %.0f KB" % (p, s / 1024.0) for p, s in big[:5])), RED) if big else "无"))
    if big:
        state["blockers"] += ["大文件 %s" % p for p, _s in big[:5]]

    bullet("二进制（扩展名或 git 判定）：%s" % (
        paint("❌ " + "，".join(bin_all[:8]), RED) if bin_all else "无"))
    if bin_all:
        state["blockers"] += ["二进制 %s" % p for p in bin_all[:8]]

    disp = [p for st, p, _o in rows if p.startswith(DISPATCH)]
    if disp:
        if args.allow_dispatch:
            bullet(paint("dispatch/ %d 个会随合并进 master —— 已用 --allow-dispatch 承认，合并时"
                         "必须剔除" % len(disp), YEL))
        else:
            bullet(paint("❌ dispatch/ %d 个会随合并进 master —— 现在合就污染（AGENTS A5）"
                         % len(disp), RED))
            state["blockers"].append("dispatch 进 master")
        print(paint("      剔除办法：git merge --no-commit %s && git rm -r -q --cached dispatch"
                    " && rm -rf dispatch && git commit" % cg, DIM))
    else:
        bullet("dispatch/ 不会进 master")


# ---------------------------------------------------------------- D. 四件套

def run(cmd, cwd, label):
    t0 = time.time()
    p = subprocess.run(cmd, cwd=str(cwd), capture_output=True, text=True,
                       encoding="utf-8", errors="replace")
    return p, time.time() - t0, label


def summarize_cargo(p):
    passed = sum(int(x) for x in re.findall(r"test result: ok\. (\d+) passed", p.stdout))
    failed = sum(int(x) for x in re.findall(r"test result: FAILED\. (\d+) passed", p.stdout))
    failed += sum(int(x) for x in re.findall(r"(\d+) failed", p.stdout))
    return passed, failed


def _disp(cmd, py, outdir):
    """只打印时用：解释器写成 python3，临时目录写成 $OUT（别把随机 tmp 路径当文档）。"""
    out = []
    for c in cmd:
        if py and c == py:
            c = "python3"
        if outdir and c.startswith(outdir):
            c = "$OUT" + c[len(outdir):]
        out.append(c)
    return " ".join(out)


def section_d(repo, mb, cg, args, state):
    py = sys.executable or "python3"
    axes = [a.strip() for a in args.axes.split(",") if a.strip()]
    drive = {"fire": "fire", "batch": "batch", "batch_conv": "conv"}
    outdir = args.workdir or tempfile.mkdtemp(prefix="mergecheck-")
    if args.workdir:
        os.makedirs(outdir, exist_ok=True)

    guard_cmds = []
    for ax in axes:
        if ax not in drive:
            continue
        json_p = os.path.join(outdir, "%s.jsonl" % ax)
        policy = "default" if ax in ("fire", "batch") else "strict"
        guard_cmds.append((
            "③ 护栏 %s" % ax,
            ["cargo", "run", "--release", "--offline", "-q", "--example", "replay_attrib",
             "--", "--dir", args.snapshot or "<快照>", "--drive", drive[ax],
             "--policy", policy, "--json", json_p],
            [py, "tools/bit_exact_attrib.py", "--json", json_p,
             "--check-baseline", "--axis", ax],
        ))

    cmds = [
        ("① cargo test", ["cargo", "test", "--offline"], None),
        ("② coverage", [py, "tools/coverage_matrix.py", "--check"], None),
        ("② docrefs", [py, "tools/check_doc_refs.py"], None),
    ]

    head("D", "四件套" + ("" if args.run_tests else "（未跑；加 --run-tests 才跑）"))
    if not os.environ.get("CARGO_TARGET_DIR"):
        bullet(paint("CARGO_TARGET_DIR 未设 —— 多 worktree 共用 target 会串味（AGENTS 踩坑 2）", YEL))

    for label, cmd, _ in cmds:
        if not args.run_tests:
            print("  " + _disp(cmd, py, outdir))
            continue
        p, dt, _l = run(cmd, repo, label)
        ok = p.returncode == 0
        extra = ""
        if "cargo test" in " ".join(cmd):
            passed, failed = summarize_cargo(p)
            extra = " — %d passed / %d failed" % (passed, failed)
        print("  %s %s%s（%.1f s）" % (
            paint("✅" if ok else "❌", GRN if ok else RED), label, extra, dt))
        if not ok:
            state["blockers"].append("%s 红" % label)
            for ln in (p.stdout or p.stderr).splitlines()[-12:]:
                print(paint("      " + ln, DIM))

    for label, replay, check in guard_cmds:
        if not args.run_tests or not args.snapshot:
            print("  " + _disp(replay, py, outdir))
            print("  " + _disp(check, py, outdir))
            if args.run_tests and not args.snapshot:
                bullet(paint("护栏跳过：缺 --snapshot <快照目录>", YEL))
            continue
        pr, dtr, _l = run(replay, repo, label)
        if pr.returncode != 0:
            print("  %s %s 回放失败（%.1f s）" % (paint("❌", RED), label, dtr))
            state["blockers"].append("%s 回放失败" % label)
            continue
        pc, dtc, _l = run(check, repo, label)
        ok = pc.returncode == 0
        print("  %s %s 护栏（%.1f s）" % (paint("✅" if ok else "❌", GRN if ok else RED),
                                       label, dtr + dtc))
        if not ok:
            state["blockers"].append("%s 护栏不一致" % label)
            for ln in (pc.stdout or pc.stderr).splitlines()[-10:]:
                print(paint("      " + ln, DIM))

    if len(axes) == 2:
        bullet(paint("第三条轴 batch_conv 默认没跑；要跑就 --axes fire,batch,batch_conv", DIM))


# ---------------------------------------------------------------- main

def parse_args(argv):
    ap = argparse.ArgumentParser(
        prog="merge_check.py",
        description="合并前的一条命令：报告体检 / 范围核对 / 污染检查 / 四件套",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="例：python3 tools/merge_check.py --cg origin/CG\n"
               "    python3 tools/merge_check.py --branch CG --allow client/src/x.gd")
    ap.add_argument("--cg", default="origin/CG", help="待合入的分支（默认 origin/CG）")
    ap.add_argument("--branch", help="本地分支名；没有远端时用它（等价于 --cg）")
    ap.add_argument("--base", help="基线分支（默认 origin/master → master → main）")
    ap.add_argument("--repo", help="仓库目录（默认当前目录的 git 顶层）")
    ap.add_argument("--reports-dir", default="dispatch/reports", help="报告目录")
    ap.add_argument("--allow", action="append", default=[],
                    help="放行某个禁区路径（可重复；只在本次运行生效）")
    ap.add_argument("--allow-dispatch", action="store_true",
                    help="承认合并时用路径剔除 dispatch/（否则 dispatch 进 master 算阻断）")
    ap.add_argument("--run-tests", action="store_true", help="跑四件套（默认只打印命令）")
    ap.add_argument("--snapshot", help="三条轴护栏需要的回放快照目录")
    ap.add_argument("--workdir", help="护栏产物目录（默认临时目录；要留证就指定）")
    ap.add_argument("--axes", default="fire,batch",
                    help="护栏轴（默认 fire,batch；可 fire,batch,batch_conv）")
    ap.add_argument("--color", choices=["auto", "always", "never"], default="auto")
    ap.add_argument("-v", "--verbose", action="store_true", help="B 节逐文件列出")
    return ap.parse_args(argv)


def main(argv):
    global _COLOR
    args = parse_args(argv)
    if args.color == "always":
        _COLOR = True
    elif args.color == "never":
        _COLOR = False
    else:
        _COLOR = bool(os.environ.get("FORCE_COLOR")) or (
            not os.environ.get("NO_COLOR") and sys.stdout.isatty())

    t0 = time.time()
    repo = find_repo(args.repo or os.getcwd()) or find_repo(Path(__file__).resolve().parent.parent)
    if repo is None:
        print(paint("不是 git 仓库：%s（用 --repo 指定）" % (args.repo or os.getcwd()), RED))
        return 2

    wanted = args.branch or args.cg
    cg = resolve_ref(repo, wanted)
    if not cg:
        print(paint("分支解析不到：%s" % wanted, RED))
        if wanted.startswith("origin/"):
            print("  没有远端时用本地分支：--branch %s" % wanted[len("origin/"):])
        return 2
    base = pick_base(repo, args.base)
    mb = git(repo, "merge-base", base, cg, check=False).stdout.strip()
    if not mb:
        print(paint("没有共同祖先：%s 与 %s" % (base, cg), RED))
        return 2

    print(paint("=== 合并前检查 %s → %s（merge-base %s）===" % (cg, base, mb[:8]), BOLD))

    state = {"blockers": []}
    report_lines = []
    _path, missing, hits = section_a(repo, cg, args, report_lines)
    if missing:
        state["blockers"] += ["报告缺 %s" % m for m in missing]

    rows = section_b(repo, mb, cg, args, report_lines, state)
    section_c(repo, mb, cg, rows, args, state)
    section_d(repo, mb, cg, args, state)

    dt = time.time() - t0
    print()
    print(paint("──── 结论 ────", BOLD))
    if state["blockers"]:
        seen, uniq = set(), []
        for b in state["blockers"]:
            if b not in seen:
                seen.add(b)
                uniq.append(b)
        print(paint("❌ 不许合：%d 个阻断项 —— %s" % (len(uniq), "；".join(uniq)), RED))
    else:
        print(paint("✅ 可以合（A 节已打印，自曝句 %d 行请先读完）" % len(hits), GRN))
    print(paint("耗时 %.2f s（%s）" % (dt, "含四件套" if args.run_tests else "未跑测试"),
                DIM))
    return 1 if state["blockers"] else 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except EnvError as exc:
        print(paint("环境/用法错误：%s" % exc, RED))
        sys.exit(2)
    except KeyboardInterrupt:
        sys.exit(130)
