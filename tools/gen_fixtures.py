#!/usr/bin/env python3
"""生成 tests 用的真值夹具。

真值来源分两种，不要混为一谈：
  * path.tsv / eval.tsv —— **真二进制**。分析仓库的 analysis/gwemu.py 用 unicorn
    把 libgraphwarrust.so 映射进内存，直接调用 calculate_path / eval 读回结果。
  * parse.tsv          —— 已验证的 Python 参照实现 graphwar_curve.py::parse_function
    （它本身由 analysis/difftest_parser.py 对二进制做过 90 例对拍）。

用法（需要分析仓库的 .venv）：
    ../.venv/bin/python tools/gen_fixtures.py
"""

import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)                 # gw2-rebuild/
ROOT = os.path.dirname(SUB)                 # 分析仓库根

sys.path.insert(0, os.path.join(ROOT, "analysis"))
sys.path.insert(0, ROOT)

import graphwar_curve as pc          # noqa: E402
from gwemu import Gw                 # noqa: E402
from unicorn.x86_const import UC_X86_REG_XMM0   # noqa: E402


def call_eval(g, toks, x, y, dy):
    """gwemu.call_eval 把 XMM0 当无符号 64 位解包，高位置 1 时会抛 struct.error。
    这里按 128 位读回后截低 64 位，语义与'读回 double'等价。"""
    v = g.make_vec(toks)
    idx = g.alloc(8)
    g.wq(idx, 0)
    g.call(0x4EB9D0, [v, idx, None, None], [x, y, dy])
    raw = g.uc.reg_read(UC_X86_REG_XMM0) & ((1 << 128) - 1)
    return struct.unpack('<d', struct.pack('<Q', raw & ((1 << 64) - 1)))[0]

OUT = os.path.join(SUB, "fixtures")


def bits(v: float) -> str:
    return "%016x" % struct.unpack("<Q", struct.pack("<d", v))[0]


def toks_str(tokens) -> str:
    parts = []
    for t in tokens:
        if isinstance(t, int):
            parts.append("%d:%s" % (t, bits(0.0)))
        else:
            parts.append("%d:%s" % (t[0], bits(t[1])))
    return ";".join(parts) if parts else "-"


def write(name: str, lines):
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, name)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines))
        fh.write("\n")
    print("wrote %-12s %4d cases" % (name, len(lines)))


# ---------------------------------------------------------------------------
# 1. 解析器：文本 -> 前缀 token 列表
# ---------------------------------------------------------------------------
PARSE_CASES = [
    "2*x+1", "sin(x)", "x^2", "2^3^2", "-x^2", "sqrt x", "2sin(x)",
    "(x+1)(x-1)", "xy", "log(x)", "ln(x)", "exp(x)", "abs(x)", "tan(x)",
    "PI", "pi", "x/2", ".5x", "1.-x", "y", "y'", "e", "n",
    "sin", "sin^2(x)", "+", "+x", "x+", "-", "--x", "(((x)))",
    "2.5e", "X", "SIN(X)", "3.", "0", "1+2+3",
]


def gen_parse():
    lines = []
    for text in PARSE_CASES:
        toks = pc.parse_function(text)
        lines.append("%s\t%d\t%s" % (text, len(toks), toks_str(toks)))
    write("parse.tsv", lines)


# ---------------------------------------------------------------------------
# 2. 求值：token 列表 -> f64（真二进制）
# ---------------------------------------------------------------------------
EVAL_TOKS = {
    "x": [pc.VAR_X],
    "sin": [pc.SIN, pc.VAR_X],
    "x^2": [pc.POW, pc.VAR_X, (pc.CONST, 2.0)],
    "sqrt": [pc.SQRT, pc.VAR_X],
    "log": [pc.LOG, pc.VAR_X],
    "ln": [pc.LN, pc.VAR_X],
    "exp": [pc.EXP, pc.VAR_X],
    "abs": [(pc.UNARY_MINUS, 0), pc.VAR_X, ],
    "div": [pc.DIVIDE, (pc.CONST, 1.0), pc.VAR_X],
    "tan": [pc.TAN, pc.VAR_X],
    "y": [pc.VAR_Y],
    "dy": [pc.VAR_DY],
    "short": [pc.ADD],
    "empty": [],
}

EVAL_POINTS = [(0.0, 0.0, 0.0), (0.5, 1.0, 2.0), (-2.0, 3.0, 0.0),
               (1e-9, 0.0, 0.0), (25.0, -15.0, 0.0)]


def gen_eval(g):
    lines = []
    for name, toks in EVAL_TOKS.items():
        for (x, y, dy) in EVAL_POINTS:
            r = call_eval(g, toks, x, y, dy)
            lines.append("%s\t%s\t%s\t%s\t%s\t%s"
                         % (name, toks_str(toks), bits(x), bits(y), bits(dy), bits(r)))
    write("eval.tsv", lines)


# ---------------------------------------------------------------------------
# 3. 采点：token 列表 -> 点序列（真二进制）
# ---------------------------------------------------------------------------
PATH_TOKS = {
    "const0": [(pc.CONST, 0.0)],
    "const3": [(pc.CONST, 3.0)],
    "x": [pc.VAR_X],
    "negx": [(pc.UNARY_MINUS, 0), pc.VAR_X],
    "sin": [pc.SIN, pc.VAR_X],
    "x^2": [pc.POW, pc.VAR_X, (pc.CONST, 2.0)],
    "sqrt": [pc.SQRT, pc.VAR_X],
    "log": [pc.LOG, pc.VAR_X],
    "ln": [pc.LN, pc.VAR_X],
    "exp": [pc.EXP, pc.VAR_X],
    "abs": [pc.ABS, pc.VAR_X],
    "div2": [pc.DIVIDE, pc.VAR_X, (pc.CONST, 2.0)],
    "divx": [pc.DIVIDE, (pc.CONST, 1.0), pc.VAR_X],
    "tan": [pc.TAN, pc.VAR_X],
}

PATH_PARAMS = [((0.0, 0.0), 0.25), ((-3.0, 1.0), 0.1), ((4.0, -2.0), 0.5),
               ((10.0, 5.0), 0.25)]


def gen_path(g):
    lines = []
    for name, toks in PATH_TOKS.items():
        for (x0, y0), step in PATH_PARAMS:
            for flip in (0, 1):
                for ode in (0, 1):
                    pts = g.call_path(toks, x0, y0, step, flip, ode)
                    encoded = ";".join("%s,%s" % (bits(px), bits(py)) for px, py in pts)
                    lines.append("%s\t%d\t%d\t%s\t%s\t%s\t%s\t%d\t%s"
                                 % (name, flip, ode, bits(x0), bits(y0), bits(step),
                                    toks_str(toks), len(pts), encoded if encoded else "-"))
    write("path.tsv", lines)


def main():
    print("映射真二进制…")
    g = Gw()
    gen_parse()
    gen_eval(g)
    gen_path(g)


if __name__ == "__main__":
    main()
