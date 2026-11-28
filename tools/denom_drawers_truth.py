#!/usr/bin/env python3
"""T108：`fixtures/denom_drawers_math_truth.tsv` 真值生成器（**值级对拍**用）。

判据/输入集来自**预注册件** `fixtures/denom_drawers_math_plan.txt` §3（commit e86d035），
本脚本只按那份计划跑，不新增/不删减用例。

## 真值来源（**逐位**档）
* 执行体 = `tools/crossarch_emu.py::X86` → `.reference/analysis-repo/analysis/gwemu.py`
  （把 Linux x86-64 `libgraphwarrust.so` 映射进 unicorn，跑**真机器码**；libm = 真 glibc）。
  这和 docs/06 那 88 条「逐位」结论是同一个执行体，不是 cargo test。
* S1 `GraphwarShotCalculator::calculate_function` = linux `0x3c2530`
  ABI：`rdi=sret(Vec)`, `rsi=tokens_vec`, `edx=flip`, `ecx=mode`, xmm0/1/2 = x0,y0,step
  （= `analysis/gwemu.py::Gw.call_path`，与 `fixtures/test_expectation_truth.tsv` 的 shot/* 同源）。
* S2 `calculate_corrected_function` = `0x3c5c50`、S3 `calculate_function_offset_str` = `0x3c5f70`
  ABI：`rdi=sret(64B)`, `rsi=text_ptr`, `rdx=len`, `rcx=&Point{x,y}`, `r8b=flip`
  （= `tools/crossarch_emu.py::X86.textpt`；第 4 参是 **bool**，见 docs/06 §153 的 flag 探针）。

## 产物列
    section  func  case  tokens|text   x_bits  y_bits  step_bits  flip  mode  ...
  * 字符串字段一律 `_hex` 存 UTF-8 十六进制（TSV 里不放空格/制表符），另给 `_utf8` 供人读；
  * 点数组只存 `n` + 整条 sha256 + 首/末点（整条塞不进 TSV，比较时按 sha256 判）。

用法：
    .reference/venv/bin/python tools/denom_drawers_truth.py            # 写夹具
    .reference/venv/bin/python tools/denom_drawers_truth.py --check     # 只比对已入库的
    .reference/venv/bin/python tools/denom_drawers_truth.py --controls  # 只跑 §3.4 正/负对照
"""

import argparse
import hashlib
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
MAIN = os.path.abspath(os.path.join(HERE, ".."))
REF = os.path.join(MAIN, ".reference")
OUT = os.path.join(MAIN, "fixtures", "denom_drawers_math_truth.tsv")
sys.path.insert(0, HERE)
import crossarch_emu as ce  # noqa: E402

VARX, ADD, SIN, WHERE_CONST = 0, 3, 14, 17


def bits(v):
    return struct.pack(">d", v).hex()


def unbits(h):
    return struct.unpack(">d", bytes.fromhex(h))[0]


def hexs(s):
    return s.encode("utf-8").hex()


def unhexs(h):
    return bytes.fromhex(h).decode("utf-8")


def tokens_field(toks):
    """`tag:value_bits;...`（空 token 流 = 空串）。"""
    return ";".join("%d:%016x" % (t, struct.unpack(">Q", struct.pack(">d", v))[0])
                    for t, v in toks)


def fnv1a64(b):
    """FNV-1a 64（与 tools/gen_test_expectations*.py / src/_scaffold/protocol.rs 同口径）。"""
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def points_bytes(pts):
    return b"".join(struct.pack("<dd", x, y) for (x, y) in pts)


def points_sha(pts):
    return hashlib.sha256(points_bytes(pts)).hexdigest()


# --------------------------------------------------------------------------- 输入集（预注册）
# §3.1 V1：S1 采样器 —— ≥16 组；ODE（mode=1）+ 空 token 流必覆盖
def v1_cases():
    show = lambda t: [(t, 0.0)]
    cases = [
        # (case, tokens, x0, y0, step, flip, mode)
        ("V1-01", show(WHERE_CONST), 0.0, 0.0, 0.01, 0, 0),
        ("V1-02", [(ADD, 0.0), (WHERE_CONST, 0.0), (VARX, 0.0)], 5.0, 2.0, 0.01, 0, 0),
        ("V1-03", [(ADD, 0.0), (WHERE_CONST, 0.0), (SIN, 0.0), (VARX, 0.0)], 5.0, 2.0, 0.01, 0, 0),
        ("V1-04", [(ADD, 0.0), (WHERE_CONST, 0.0), (SIN, 0.0), (VARX, 0.0)], -24.5, 0.0, 0.02, 1, 0),
        ("V1-05", [(VARX, 0.0), (WHERE_CONST, 3.0), (8, 0.0)], 1.0, 0.0, 0.01, 0, 0),  # x^3
        ("V1-06", [], 0.0, 0.0, 0.01, 0, 0),                       # 空 token 流 / Normal
        ("V1-07", [], 0.0, 0.0, 0.01, 0, 1),                       # 空 token 流 / ODE（"导数恒 0"）
        ("V1-08", [(WHERE_CONST, 0.0)], 0.0, 0.0, 0.01, 0, 1),     # ODE：dy/dx = 0
        ("V1-09", [(VARX, 0.0)], 0.0, 0.0, 0.01, 0, 1),            # ODE：dy/dx = x ⇒ y = x²/2
        ("V1-10", [(VARX, 0.0)], 1.0, 1.0, 0.01, 0, 1),
        ("V1-11", [(ADD, 0.0), (WHERE_CONST, 0.0), (SIN, 0.0), (VARX, 0.0)], 0.0, 0.0, 0.01, 1, 1),
        ("V1-12", [(ADD, 0.0), (WHERE_CONST, 0.0), (SIN, 0.0), (VARX, 0.0)], 0.0, 0.0, 0.25, 0, 0),
        ("V1-13", [(ADD, 0.0), (WHERE_CONST, 0.0), (VARX, 0.0)], 1e308, 0.0, 0.01, 0, 0),
        ("V1-14", [(ADD, 0.0), (WHERE_CONST, 0.0), (VARX, 0.0)], float("nan"), 0.0, 0.01, 0, 0),
        ("V1-15", [(ADD, 0.0), (WHERE_CONST, 0.0), (VARX, 0.0)], 0.0, 0.0, 0.01, 1, 0),
        ("V1-16", [(ADD, 0.0), (WHERE_CONST, 1e300), (VARX, 0.0)], 0.0, 0.0, 0.01, 0, 0),
        ("V1-17", [(ADD, 0.0), (WHERE_CONST, -0.0), (VARX, 0.0)], -0.0, 0.0, 0.01, 0, 0),
        ("V1-18", [(ADD, 0.0), (WHERE_CONST, 0.0), (VARX, 0.0)], 2.0, 3.0, 0.5, 0, 0),
        ("V1-19", [(ADD, 0.0), (WHERE_CONST, 0.0), (VARX, 0.0)], 2.0, 3.0, 1.0, 0, 0),
        ("V1-20", [(ADD, 0.0), (WHERE_CONST, 0.0), (VARX, 0.0)], 2.0, 3.0, 0.0, 0, 0),
    ]
    return cases


# §3.2 V2：S2/S3 修正量外壳 —— ≥24 组；y≠0 至少 6 组；flag=0x100 一条
def v2_cases():
    return [
        # (case, text, x, y, flag)
        ("V2-01", "", 0.0, 0.0, 0),
        ("V2-02", "0", 0.0, 0.0, 0),
        ("V2-03", "x", 5.0, 0.0, 0),          # c = -5 ⇒ " - 5.000"
        ("V2-04", "x/2", 24.5, 0.0, 0),       # c = -12.25
        ("V2-05", "x/2", -24.5, 0.0, 0),      # c = +12.25
        ("V2-06", "x^3", 1.0, 0.0, 0),        # c = -1
        ("V2-07", "x ^ 3", 1.0, 0.0, 0),      # 原文逐字保留（空格）
        ("V2-08", "X^3", 1.0, 0.0, 0),        # 大写不被规范化（原文逐字）
        ("V2-09", "x^3   ", 1.0, 0.0, 0),     # 尾随空格逐字保留
        ("V2-10", "  x^3", 1.0, 0.0, 0),      # 前导空格逐字保留
        ("V2-11", "sin x", 1.0, 0.0, 0),      # 省括号
        ("V2-12", "sin(x)", 2.5, 3.0, 0),     # y≠0：c = 3 - sin(2.5) = +2.402
        ("V2-13", "sin(x)", 2.5, 3.0, 1),     # flip：c = 3 - sin(-2.5) = +3.598
        ("V2-14", "sin(x)", 2.5, 3.0, 0x100), # **低字节 0** ⇒ 不 flip（第 4 参是 bool 不是 mode）
        ("V2-15", "sin(x)", 2.5, 3.0, 2),     # 低字节非 0 ⇒ flip（同上）
        ("V2-16", "tan(x)", 0.0, 0.0, 0),
        ("V2-17", "sqrt(x)", 4.0, 0.0, 0),
        ("V2-18", "exp(x)+ln(x)", 1.5, 0.0, 0),
        ("V2-19", "abs((abs((-0.57+x))+-4.85))", 1.0, 0.0, 0),
        ("V2-20", "1/0", 1.0, 0.0, 0),        # c = -inf ⇒ " - inf"
        ("V2-21", "sqrt(-1)", 1.0, 0.0, 0),   # c = NaN ⇒ " + NaN"
        ("V2-22", "2+3", 1.0, 0.0, 0),
        ("V2-23", "x/100000", 1.0, 0.0, 0),   # c = -1e-5 ⇒ " - 0.000"
        ("V2-24", "x/100000", -1.0, 0.0, 0),  # c = +1e-5 ⇒ " + 0.000"
        ("V2-25", "x^2+1", 1.0, 2.0, 0),      # y≠0，c = 0
        ("V2-26", "x", float("nan"), 0.0, 0), # x=NaN ⇒ c=NaN
        ("V2-27", "x", float("inf"), 0.0, 0), # x=inf ⇒ c=-inf
        ("V2-28", "x", -0.0, 0.0, 0),         # x=-0 ⇒ c=+0
        ("V2-29", "x", 1.0, 1e308, 0),        # y 极大
        ("V2-30", "x", 1.0, float("nan"), 0), # y=NaN
        ("V2-31", "x+1", -3.5, 0.0, 1),       # flip + y=0（= crossarch F9 c_flip）
        ("V2-32", "cos((0.19*(6.70+x)))", 11.962551145879328, 3.7443842094294837, 1),  # 抓包帧 161
        ("V2-33", "abs((abs((-0.57+x))+-4.85))", 14.305645069934501, -1.3905063945238432, 1),  # 抓包帧 203
    ]


# 本轮工作中（读 0x3c5c50 反汇编）发现的**新行为**：`calculate_corrected_function` 会把
# 原文里**未闭合的 `(`** 用 `)` 补齐（0x3c5c50 的 0x3c5c80..0x3c5d32 计数 + 0x3c5e90 的补括号循环）。
# 预注册的 V2 没有这条 ⇒ **单独 section V3-extra**，明写"不在预注册输入集内、为钉住新发现而追加"，
# 不混进 V2 的主判据（预注册 §3.2 的判据仍然只用在 V2 上）。
def v3_cases():
    return [
        ("V3-01", "(x", 1.0, 2.0, 0),
        ("V3-02", "((x", 1.0, 2.0, 0),
        ("V3-03", "x)", 1.0, 2.0, 0),        # 多余的 ')' 不删（depth 不为负）
        ("V3-04", "sin(x", 1.0, 2.0, 0),
        ("V3-05", "(()", 1.0, 2.0, 0),
        ("V3-06", "x(", 1.0, 2.0, 0),
        ("V3-07", "))x((", 1.0, 2.0, 0),     # 前导 ')' 不抵，尾随 '((' 补两个
        ("V3-08", "0", 1.0, -0.0, 0),        # c = -0.0 ⇒ " + -0.000"（+ 分支逐字格式 -0.0）
        ("V3-09", "x", 1e308, -1e308, 0),    # c = -inf ⇒ " - inf"
        ("V3-10", "x", -1e308, 1e308, 0),    # c = +inf ⇒ " + inf"
        ("V3-11", "(x", 1.0, 2.0, 1),        # 括号补齐 + flip
    ]


# --------------------------------------------------------------------------- 真值
def truth():
    x = ce.X86()
    rows = []
    for (cid, toks, x0, y0, step, flip, mode) in v1_cases():
        pts = x.path(toks, x0, y0, step, flip, mode)
        rows.append(dict(section="S1", func="calculate_function", case=cid,
                         tokens=tokens_field(toks), x=bits(x0), y=bits(y0), step=bits(step),
                         flip=str(flip), mode=str(mode), n=str(len(pts)),
                         sha=points_sha(pts), fnv="%016x" % fnv1a64(points_bytes(pts)),
                         first=("%016x,%016x" % (struct.unpack(">Q", struct.pack(">d", pts[0][0]))[0],
                                                 struct.unpack(">Q", struct.pack(">d", pts[0][1]))[0])) if pts else "",
                         last=("%016x,%016x" % (struct.unpack(">Q", struct.pack(">d", pts[-1][0]))[0],
                                                struct.unpack(">Q", struct.pack(">d", pts[-1][1]))[0])) if pts else ""))
    for (cid, text, xx, yy, flag) in v2_cases():
        r = x.textpt(text, xx, yy, flag)
        if not r.get("plausible"):
            raise SystemExit("V2 %s: sret 不像 String —— ABI 口径变了，停下" % cid)
        corr, off = r["out"].split("||")
        corr_s, off_s = _sret_string(corr), _sret_string(off)
        rows.append(dict(section="S2S3", func="calculate_corrected_function+calculate_function_offset_str",
                         case=cid, texts=hexs(text), x=bits(xx), y=bits(yy), flag=str(flag),
                         corr_len=str(len(corr_s.encode("utf-8"))), corr_hex=hexs(corr_s),
                         off_len=str(len(off_s.encode("utf-8"))), off_hex=hexs(off_s),
                         corr_utf8=corr_s, off_utf8=off_s))
    for (cid, text, xx, yy, flag) in v3_cases():
        r = x.textpt(text, xx, yy, flag)
        if not r.get("plausible"):
            raise SystemExit("V3 %s: sret 不像 String —— ABI 口径变了，停下" % cid)
        corr, off = r["out"].split("||")
        corr_s, off_s = _sret_string(corr), _sret_string(off)
        rows.append(dict(section="S2S3-extra", func="calculate_corrected_function+calculate_function_offset_str",
                         case=cid, texts=hexs(text), x=bits(xx), y=bits(yy), flag=str(flag),
                         corr_len=str(len(corr_s.encode("utf-8"))), corr_hex=hexs(corr_s),
                         off_len=str(len(off_s.encode("utf-8"))), off_hex=hexs(off_s),
                         corr_utf8=corr_s, off_utf8=off_s))
    return rows


def _sret_string(rep):
    """从 `_norm_sret` 的文本表示里把 `HEAPPTR[...]` 中的串取出来。

    格式：`<cap> HEAPPTR[<str>] <len> <0>*7`。cap/len 是 8 字节字，`[..]` 里就是串内容
    （`tools/crossarch_emu.py::_norm_sret` 的口径）；串里不会有 `]`（函数名/数字/运算符）。
    """
    i = rep.index("HEAPPTR[")
    j = rep.index("]", i)
    return rep[i + len("HEAPPTR["):j]


# --------------------------------------------------------------------------- §3.4 对照
def controls():
    ok = True
    x = ce.X86()
    # C1 正对照：shot/B01（fixtures/test_expectation_truth.tsv，T46 用同一个执行体生成）
    c = 2.0 - unbits(x.eval_tokens(x.parse("sin(x)"), 5.0)[0])
    toks = [(ADD, 0.0), (WHERE_CONST, c), (SIN, 0.0), (VARX, 0.0)]
    pts = x.path(toks, 5.0, 2.0, 0.01, 0, 0)
    want_n, want_sha = 2471, "9640807e4cde2de51caf9935bdc03099f2abeb585e8855eb6e715fb415effe1e"
    got_sha = points_sha(pts)
    c1 = (len(pts) == want_n and got_sha == want_sha)
    print("C1 正对照 shot/B01：n=%d sha=%s ⇒ %s" % (len(pts), got_sha[:16], "PASS" if c1 else "FAIL"))
    ok &= c1
    # C2 负对照（预注册原文）：step 改 1 ulp ⇒ 点数或末点必须变。
    #   ⚠️ 实测**不变**：采样器用弦长二分追，步长 1 ulp 被 `LENGTH_TOL` 吸收。
    #   照预注册**如实记 FAIL**（不改判据），另加 C2'/C2'' 两条 1-ulp 扰动当补充负对照。
    sw = lambda v: struct.unpack(">d", struct.pack(">Q", struct.unpack(">Q", struct.pack(">d", v))[0] + 1))[0]
    pts2 = x.path(toks, 5.0, 2.0, sw(0.01), 0, 0)
    c2 = (len(pts2) != len(pts)) or (pts2[-1] != pts[-1])
    print("C2  负对照 step+1ulp（预注册原文）：n=%d vs %d ⇒ %s" % (len(pts2), len(pts), "PASS" if c2 else "FAIL"))
    print("      —— 不变是**预期外但可解释**的：1 ulp 步长差被弦长二分容差吸收；判据不改")
    # C2' x0 +1ulp ⇒ 首点必须变
    p_x = x.path(toks, sw(5.0), 2.0, 0.01, 0, 0)
    c2p = (p_x[0] != pts[0])
    print("C2' 负对照 x0+1ulp：首点 %s vs %s ⇒ %s" % (p_x[0], pts[0], "PASS" if c2p else "FAIL"))
    ok &= c2p
    # C2'' 常数 +1ulp ⇒ 末点必须变
    p_c = x.path([(ADD, 0.0), (WHERE_CONST, sw(c)), (SIN, 0.0), (VARX, 0.0)], 5.0, 2.0, 0.01, 0, 0)
    c2pp = (p_c[-1] != pts[-1])
    print("C2''负对照 c+1ulp：末点 %s vs %s ⇒ %s" % (p_c[-1], pts[-1], "PASS" if c2pp else "FAIL"))
    ok &= c2pp
    # C3 负对照：V2 的 text 改一个字符 ⇒ 输出必须变
    a = _sret_string(x.textpt("x", 5.0, 0.0, 0)["out"].split("||")[0])
    b = _sret_string(x.textpt("y", 5.0, 0.0, 0)["out"].split("||")[0])
    c3 = (a != b)
    print("C3 负对照 text x→y：%r vs %r ⇒ %s" % (a, b, "PASS" if c3 else "FAIL"))
    ok &= c3
    # C5（本任务追加）正对照：空 token 流 + ODE 必须返回**非空**点数组（否则"空=导数 0"分支没法测）
    p = x.path([], 0.0, 0.0, 0.01, 0, 1)
    print("C5 空 token 流 ODE：n=%d ⇒ %s" % (len(p), "PASS" if len(p) > 0 else "FAIL"))
    ok &= (len(p) > 0)
    print("\n对照汇总：%s" % ("全过" if ok else "**有 FAIL**"))
    return 0 if ok else 1


# --------------------------------------------------------------------------- 写/查
COLS = ["section", "func", "case", "tokens", "texts", "x", "y", "step", "flip", "mode",
        "n", "sha", "fnv", "first", "last", "flag", "corr_len", "corr_hex", "off_len", "off_hex",
        "corr_utf8", "off_utf8"]


def render(rows):
    lines = [
        "# denom_drawers_math_truth.tsv —— T108 **值级对拍**真值（生成器 tools/denom_drawers_truth.py）",
        "# 判据/输入集 = fixtures/denom_drawers_math_plan.txt §3（预注册，commit e86d035）",
        "# 真值来源 = **逐位**：Linux x86-64 libgraphwarrust.so 的**真机器码**在 unicorn 下直跑",
        "#   执行体 tools/crossarch_emu.py::X86 → .reference/analysis-repo/analysis/gwemu.py（真 glibc libm）",
        "#   S1 = 0x3c2530 calculate_function（采样器）：rdi=sret, rsi=tokens, edx=flip, ecx=mode, xmm= x0,y0,step",
        "#   S2 = 0x3c5c50 calculate_corrected_function / S3 = 0x3c5f70 calculate_function_offset_str：",
        "#        rdi=sret(64B), rsi=text, rdx=len, rcx=&Point{x,y}, r8b=flip（第 4 参是 bool，见 V2-14/V2-15）",
        "# 复算：.reference/venv/bin/python tools/denom_drawers_truth.py（--check 只比对；--controls 只跑对照）",
        "# 列：tokens=`tag:valuebits;...`（空=空 token 流）｜x/y/step = f64 大端十六进制位模式",
        "#     texts / corr_hex / off_hex = UTF-8 十六进制（TSV 里不放空格）｜corr_utf8/off_utf8 仅供人读",
        "#     S1 的 sha/fnv = 整条点数组（小端 <dd 拼）的 sha256 / FNV-1a64（Rust 侧零依赖用 fnv）；first/last = 首/末点 f64 位模式",
        "# section S2S3 = 预注册 §3.2 的 V2 输入集（主判据）；S2S3-extra = V3，**不在预注册输入集内**，",
        "#   是本轮读 0x3c5c50 反汇编后**发现新行为**（未闭合 '(' 会被补 ')'）才追加的，不与 V2 混算。",
        "\t".join(COLS),
    ]
    for r in rows:
        lines.append("\t".join(r.get(c, "") for c in COLS))
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--controls", action="store_true")
    a = ap.parse_args()
    if a.controls:
        return controls()
    rows = truth()
    text = render(rows)
    if a.check:
        old = open(OUT, encoding="utf-8").read()
        if old == text:
            print("✓ %s 与现算一致（%d 行）" % (OUT, len(rows)))
            return 0
        print("✗ %s 与现算不一致 —— 打印前 40 行差异" % OUT)
        import difflib
        for ln in list(difflib.unified_diff(old.splitlines(), text.splitlines(), "disk", "now"))[:40]:
            print(ln)
        return 1
    with open(OUT, "w", encoding="utf-8") as fh:
        fh.write(text)
    print("写出 %s（%d 条：S1 %d / S2S3 %d）"
          % (OUT, len(rows), sum(1 for r in rows if r["section"] == "S1"),
             sum(1 for r in rows if r["section"] == "S2S3")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
