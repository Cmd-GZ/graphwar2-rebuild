#!/usr/bin/env python3
"""生成 `fixtures/to_text.tsv`：token -> 文本（`0x4ec160`）的真值夹具。

真值来源：**真二进制**。用分析仓库的 `analysis/gwemu.py`（unicorn 映射
`libgraphwarrust.so`）直接调用 `0x4ec160`，读回它写出的 `String`。

夹具格式（与 `fixtures/parse.tsv` 的 token 列一致，见 `tests/golden.rs`）：

    tag:hexbits;tag:hexbits;... <TAB> 输出文本

`-` 表示空 token 列表。文本里不会出现 TAB / 换行（输出字母表只有
`x y + - * / ^ ( ) . 数字 sin cos tan sqrt log ln exp abs NaN inf`）。

用法（需要分析仓库的 .venv）：
    ../.venv/bin/python tools/gen_to_text_fixture.py
"""

import os
import random
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)                # gw2-rebuild/


def _find_root():
    """分析仓库根：有 `analysis/gwemu.py` 的那一级。可用 GW2_ROOT 覆盖。"""
    env = os.environ.get("GW2_ROOT")
    if env:
        return env
    d = REPO
    while True:
        if os.path.exists(os.path.join(d, "analysis", "gwemu.py")):
            return d
        parent = os.path.dirname(d)
        if parent == d:
            raise SystemExit("找不到含 analysis/gwemu.py 的分析仓库根（用 GW2_ROOT=... 指定）")
        d = parent


ROOT = _find_root()

sys.path.insert(0, os.path.join(ROOT, "analysis"))
sys.path.insert(0, ROOT)

from gwemu import (Gw, HEAP, VAR_X, VAR_Y, VAR_DY, ADD, SUB,  # noqa: E402
                   UNARY_MINUS, MUL, DIVIDE, POW, SQRT, LOG, LN, EXP, ABS,
                   SIN, COS, TAN, CONST)

TO_TEXT = 0x4EC160
OUT = os.path.join(REPO, "fixtures", "to_text.tsv")
ROUNDTRIP = os.path.join(REPO, "fixtures", "to_text_roundtrip.tsv")


def bits_hex(v):
    """f64 -> 16 位十六进制（**数值位串**，与 parse.tsv 一致；不是小端字节序）。"""
    return "%016x" % struct.unpack("<Q", struct.pack("<d", float(v)))[0]


def reset_heap(g):
    """每次调用前把模拟堆指针拨回头：几百例累积起来会吃光 64 MiB。"""
    g.heap_ptr = HEAP


def call_to_text(g, toks):
    """调用 0x4ec160，返回 (text, pos_after)。"""
    v = g.make_vec(toks)
    out = g.alloc(24)
    g.wq(out, 0)
    g.wq(out + 8, 0)
    g.wq(out + 16, 0)
    pos = g.alloc(8)
    g.wq(pos, 0)
    g.call(TO_TEXT, [out, v, pos])
    ptr = g.rq(out + 8)
    ln = g.rq(out + 16)
    txt = bytes(g.rd(ptr, ln)).decode("utf-8") if ln else ""
    return txt, g.rq(pos)


# ---------------------------------------------------------------------------
# 语料
# ---------------------------------------------------------------------------

C = lambda v: (CONST, float(v))          # noqa: E731
X, Y, DY = (VAR_X, 0.0), (VAR_Y, 0.0), (VAR_DY, 0.0)
OPS = [ADD, SUB, MUL, DIVIDE, POW]
FNS = [SQRT, LOG, LN, EXP, ABS, SIN, COS, TAN]


def fixture_tokens():
    """`fixtures/parse.tsv` 里已有的真值 token 流（真解析器产出）。"""
    p = os.path.join(REPO, "fixtures", "parse.tsv")
    cases = []
    for line in open(p, encoding="utf-8"):
        line = line.rstrip("\n")
        if not line:
            continue
        col = line.split("\t")[2]
        toks = []
        for item in col.split(";"):
            tag, bits = item.split(":")
            toks.append((int(tag), struct.unpack("<d", bytes.fromhex(bits)[::-1])[0]))
        cases.append(toks)
    return cases


def hand_cases():
    return [
        [],
        [X],
        [Y],
        [DY],
        # 每个函数一个
        *[[(f, 0.0), X] for f in FNS],
        # 每个二元运算一个
        *[[(op, 0.0), X, C(2)] for op in OPS],
        # 一元负号：不加括号
        [(UNARY_MINUS, 0.0), X],
        [(UNARY_MINUS, 0.0), (UNARY_MINUS, 0.0), X],
        [(UNARY_MINUS, 0.0), (POW, 0.0), X, C(2)],
        [(MUL, 0.0), (UNARY_MINUS, 0.0), C(1.06), X],
        [(ADD, 0.0), (UNARY_MINUS, 0.0), C(3.49), X],
        [(TAN, 0.0), (SIN, 0.0), (MUL, 0.0), (UNARY_MINUS, 0.0), C(0.12), X],
        # ^ 左结合
        [(POW, 0.0), (POW, 0.0), X, C(2), C(3)],
        [(POW, 0.0), X, (POW, 0.0), C(2), C(3)],
        # 深层嵌套
        [(SIN, 0.0), (COS, 0.0), (TAN, 0.0), (SQRT, 0.0), (ABS, 0.0), X],
        [(DIVIDE, 0.0), (ADD, 0.0), (MUL, 0.0), X, C(2), C(1), (SUB, 0.0), X, C(3)],
        # 缺操作数 / 截断
        [(SIN, 0.0)],
        [(ADD, 0.0)],
        [(SUB, 0.0)],
        [(UNARY_MINUS, 0.0)],
        [(SQRT, 0.0), (SQRT, 0.0)],
        [(ADD, 0.0), X],
        [(ADD, 0.0), (ADD, 0.0), X, X],
        [(MUL, 0.0), (ADD, 0.0), X],
        # 多余 token：只消费一棵子树
        [X, Y],
        [C(1), (ADD, 0.0), X, X],
        # 负常量做 `^` 的左底座：文本 `-c^y` 会被重新解析成 `neg(c^y)`，
        # 与原来的 `(-c)^y` 数值不同（真行为，往返夹具里钉住）
        [(POW, 0.0), C(-2.5), Y],
        [(POW, 0.0), C(-2.5), C(0.5)],
        [(EXP, 0.0), (POW, 0.0), C(-1.4959242104336722e114), Y],
        [(POW, 0.0), (POW, 0.0), C(-2.0), C(2.0), C(3.0)],
        [(POW, 0.0), (MUL, 0.0), C(-2.0), X, Y],
    ]


def const_cases():
    vals = [0.0, -0.0, 1.0, -1.0, 0.5, 0.25, 0.125, 0.135, 0.005, 0.015, 0.0025,
            1.005, 2.675, 12.3456, 123.456, 1e-9, 1e-7, 1e21, 1e22, 1e300, 1e-300,
            5e-324, 1.7976931348623157e308, 9007199254740993.0, 1.0 / 3.0,
            float("nan"), float("inf"), float("-inf"), 3.141592653589793,
            2.718281828459045, 999999.999, 0.07, -0.07, -1.06, -23.81, -0.12]
    return [[C(v)] for v in vals]


def random_trees(rng, n):
    """随机生成**形状合法**的前缀树（操作数齐全）。"""
    out = []
    for _ in range(n):
        depth = rng.randint(1, 4)

        def gen(d):
            r = rng.random()
            if d <= 0 or r < 0.3:
                return [rng.choice([X, Y, DY, C(round(rng.uniform(-20, 20), 2))])]
            if r < 0.55:
                return [(rng.choice(FNS), 0.0)] + gen(d - 1)
            if r < 0.65:
                return [(UNARY_MINUS, 0.0)] + gen(d - 1)
            return ([(rng.choice(OPS), 0.0)] + gen(d - 1) + gen(d - 1))

        out.append(gen(depth))
    return out


def random_streams(rng, n):
    """随机 tag 序列（可能截断，覆盖"缺操作数"的路径）。"""
    out = []
    for _ in range(n):
        k = rng.randint(0, 9)
        toks = []
        for _ in range(k):
            t = rng.choice([VAR_X, VAR_Y, VAR_DY, ADD, SUB, UNARY_MINUS, MUL,
                            DIVIDE, POW, SQRT, LOG, LN, EXP, ABS, SIN, COS, TAN, CONST])
            toks.append((t, round(rng.uniform(-5, 5), 2) if t == CONST else 0.0))
        out.append(toks)
    return out


CAPTURED = [
    "sin(x)",
    "0.00",
    "(-1.06*x)",
    "(-23.81*x)",
    "tan(sin((-0.12*x)))",
    "abs((-3.49+x))",
    "((2.00*x)+1.00)",
    "((x^2.00)^3.00)",
]


def real_parse(g, s):
    """调用真解析器 `0x4ed670`，返回 token 序列。"""
    b = s.encode()
    mem = g.alloc(len(b) + 1)
    g.wr(mem, b)
    st = g.alloc(24)
    g.wq(st, len(b))
    g.wq(st + 8, mem)
    g.wq(st + 16, len(b))
    out = g.alloc(24)
    g.wq(out, 0)
    g.wq(out + 8, 0)
    g.wq(out + 16, 0)
    g.call(0x4ED670, [out, st])
    ptr, n = g.rq(out + 8), g.rq(out + 16)
    return [(struct.unpack("<Q", g.rd(ptr + i * 16, 8))[0],
             struct.unpack("<d", g.rd(ptr + i * 16 + 8, 8))[0]) for i in range(n)]


def main():
    g = Gw()
    cases = []
    cases += fixture_tokens()
    cases += hand_cases()
    cases += const_cases()
    # 想扩大语料做一次性复核时：GW2_TO_TEXT_TREES=800 GW2_TO_TEXT_STREAMS=600 ...
    n_trees = int(os.environ.get("GW2_TO_TEXT_TREES", "150"))
    n_streams = int(os.environ.get("GW2_TO_TEXT_STREAMS", "120"))
    rng = random.Random(20261106)
    cases += random_trees(rng, n_trees)
    cases += random_streams(rng, n_streams)

    lines = []
    seen = set()
    leftover = 0
    for toks in cases:
        reset_heap(g)
        key = tuple((t, struct.pack("<d", v)) for t, v in toks)
        if key in seen:
            continue
        seen.add(key)
        txt, pos = call_to_text(g, toks)
        # 只消费一棵子树：`[x, y]` 这种"多余 token"流会剩下尾巴（与二进制一致）
        assert pos <= len(toks), (toks, pos)
        leftover += pos != len(toks)
        enc = "-" if not toks else ";".join(
            "%d:%s" % (t, bits_hex(v)) for t, v in toks)
        lines.append("%s\t%s" % (enc, txt))

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print("wrote %d cases -> %s（%d 例有多余 token 未消费）" % (len(lines), OUT, leftover))

    # --- 第二份夹具：文本级两代往返，直接钉住"二进制自己也不是总能回到原文本" ---
    # 每行 `t1<TAB>t2`：t1 = to_text(parse(t1 的来源))，t2 = to_text(parse(t1))。
    # 我们用**自己的** parse+to_text 去复算 t2，因此这一条同时压住解析与反解析。
    texts = [txt for _, txt in (l.split("\t", 1) for l in lines)]
    texts += CAPTURED
    rt_lines = []
    n_idem = 0
    for s in texts:
        reset_heap(g)
        t1, _ = call_to_text(g, real_parse(g, s))
        t2, _ = call_to_text(g, real_parse(g, t1))
        n_idem += t1 == t2
        rt_lines.append("%s\t%s" % (t1, t2))

    with open(ROUNDTRIP, "w", encoding="utf-8") as f:
        f.write("\n".join(rt_lines) + "\n")
    print("wrote %d roundtrip cases -> %s（其中 %d 例文本是不动点）"
          % (len(rt_lines), ROUNDTRIP, n_idem))

    # 交叉校验：抓包里的 AI 文本必须是不动点（text -> parse -> text）
    print("\n--- captured AI text fixed points ---")
    for s in CAPTURED:
        reset_heap(g)
        txt, _ = call_to_text(g, real_parse(g, s))
        print("  %-22r -> %-22r %s" % (s, txt, "OK" if txt == s else "MISMATCH"))


if __name__ == "__main__":
    main()
