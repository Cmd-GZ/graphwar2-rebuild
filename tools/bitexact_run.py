#!/usr/bin/env python3
"""bitexact_run.py —— T13「证据升档 sweep」的**真二进制侧**对拍驱动（unicorn 直调 `.so`）。

任务书 `.agent-work/specs/T13-bit-exact-sweep.md` ②。分工：

    真侧 = 本脚本（unicorn 2.1.4 隔离执行原版 libgraphwarrust.so 的真函数，地址是 ELF vaddr）
    我侧 = examples/bitexact_harness.rs（喂同一组输入给我方 src/，打印 f64 位模式）
    比较 = 逐行文本 diff（就是逐位比较，没有 eps）

真侧复用 `analysis/gwemu.py` 的 `Gw`（`tools/posy_unicorn.py` 用的同一个）：
它已经把 `malloc/free/realloc/__rust_alloc/__rust_dealloc`、Vec 扩容（`0x460c20`）、
panic（`0x94b340`）都 hook 掉了，所以隔离执行不会掉进引擎接口。

四个 mode（与 harness 同协议，见 examples/bitexact_harness.rs 头注释）：

    parse  0x4ed670  MathFunction::new（解析器）→ 前缀 token 流
    eval   0x4eb9d0  求值器
    path   0x3c2530  采样器
    pnc    0x4f5450  PolishNotationConverter::reorder_recursive   ← 本轮 sweep 主体（439 之一）

用法：

    # 1) 真侧
    ../.venv/bin/python tools/bitexact_run.py real --mode pnc \\
        --in fixtures/bit_exact_inputs.txt --out /tmp/real_pnc.txt
    # 2) 我侧（自动跑 cargo example）
    ../.venv/bin/python tools/bitexact_run.py ours --mode pnc \\
        --in fixtures/bit_exact_inputs.txt --out /tmp/ours_pnc.txt
    # 3) 逐位比较
    ../.venv/bin/python tools/bitexact_run.py diff --real /tmp/real_pnc.txt \\
        --ours /tmp/ours_pnc.txt
    # 一条龙（正对照 + sweep）
    ../.venv/bin/python tools/bitexact_run.py sweep
"""

import argparse
import os
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
MAIN = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(HERE))))
if not os.path.exists(os.path.join(MAIN, ".reference")):
    MAIN = os.path.dirname(ROOT)
ANALYSIS = os.path.join(MAIN, ".reference", "analysis-repo", "analysis")
sys.path.insert(0, ANALYSIS)

from gwemu import Gw  # noqa: E402

# ---------------------------------------------------------------------------
# ⚠️ 实证出来的 harness 校正：`emu.py` 的 libm hook 用的是 **Python 的 math**
# （`MATH = {'sin': _safe(math.sin), ...}`，`_safe` 把异常吞成 `float('nan')`）。
# 对 ±inf / NaN 输入它与 glibc 不是逐位一致 —— 实测 `sin(inf)`：Python 侧报 ValueError
# 被吞成 **+qNaN(0x7ff8…)**，而真 libm 给的是 **−qNaN(0xfff8…)**，于是把一条假的
# 「差异」算在求值器头上。这里把 MATH 换成 **ctypes 直调 libm.so.6**，让真侧真的是真侧。
# （`pow` / `atan2` 仍走 `_do_import` 里的 Python 分支，在本轮输入里没用到；已记进遗留。）
# ---------------------------------------------------------------------------
import ctypes  # noqa: E402
import emu as _emu_mod  # noqa: E402

_LIBM = ctypes.CDLL("libm.so.6")
_F1 = ("sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh",
       "exp", "log", "log10", "log2", "sqrt", "fabs", "floor", "ceil", "round", "trunc")
for _n in _F1:
    _f = getattr(_LIBM, _n)
    _f.restype = ctypes.c_double
    _f.argtypes = [ctypes.c_double]
    _emu_mod.MATH[_n] = (lambda f: (lambda x: f(x)))(_f)

# ---- 真函数地址（ELF vaddr；Ghidra = +0x100000）----
ADDR_PARSE = 0x4ED670
ADDR_EVAL = 0x4EB9D0
ADDR_PATH = 0x3C2530
ADDR_PNC = 0x4F5450

# 函数名 / 归模块 / 我方源位置（`fixtures/bit_exact_plan.txt` 里同一行）
INFO = {
    "parse": ("MathFunction::new（解析器）", "-", "src/math_function.rs:459 (parse)", "正对照"),
    "eval": ("求值器", "-", "src/math_function.rs:523 (eval)", "正对照"),
    "path": ("采样器", "graphwar_path_calculator.rs",
             "src/graphwar_path_calculator.rs:47 (calculate)", "正对照"),
    "pnc": ("PolishNotationConverter::reorder_recursive", "polish_notation_converter.rs",
            "src/polish_notation_converter.rs:146 (convert/reorder)", "观测→目标逐位"),
}


def hx(v):
    return "0x%016x" % struct.unpack("<Q", struct.pack("<d", v))[0]


def unhex(s):
    s = s.strip()
    if s.startswith("0x"):
        s = s[2:]
    return struct.unpack("<d", struct.pack("<Q", int(s, 16)))[0]


def read_sections(path):
    """fixtures/bit_exact_inputs.txt → {mode: [line, ...]}（去掉注释与空行）。"""
    out, cur = {}, None
    for line in open(path):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        if s.startswith("["):
            cur = s[1:-1].strip()
            out.setdefault(cur, [])
            continue
        if cur is not None:
            out[cur].append(s)
    return out


class Real:
    """真二进制侧。一个实例 = 一台干净的模拟器（每个 mode 复用，状态会残留但只影响堆地址）。"""

    def __init__(self):
        self.g = Gw()

    # ---- 0x4ed670 解析器 ----
    def parse(self, text):
        g = self.g
        b = text.encode()
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
        g.call(ADDR_PARSE, [out, st])
        ptr, ln = g.rq(out + 8), g.rq(out + 16)
        toks = []
        for i in range(ln):
            tag = struct.unpack("<Q", g.rd(ptr + i * 16, 8))[0]
            val = struct.unpack("<d", g.rd(ptr + i * 16 + 8, 8))[0]
            toks.append((tag, val))
        return toks

    # ---- 0x4eb9d0 求值器 ----
    def eval(self, toks, x, y, dy):
        g = self.g
        v = g.make_vec(toks)
        idx = g.alloc(8)
        g.wq(idx, 0)
        g.call(ADDR_EVAL, [v, idx, None, None], [x, y, dy])
        raw = g.uc.reg_read(0x2C) if False else None  # noqa: F841 (置空，下面用 xmm_get)
        return g.xmm_get(0)

    # ---- 0x3c2530 采样器 ----
    def path(self, toks, x0, y0, step, flip, mode):
        return self.g.call_path(toks, x0, y0, step, flip, mode)

    # ---- 0x4f5450 reorder_recursive ----
    def pnc(self, toks):
        """自造 `PolishNotationConverter` 的**纯数据**状态（布局逐指令读出，见 plan §D-A1）：
            +0x00 input.cap  +0x08 input.ptr  +0x10 input.len
            +0x18 output.cap +0x20 output.ptr +0x28 output.len
        输出 Vec 初值 cap=0 / ptr=8(dangling 16B 对齐) / len=0 —— 与调用点 0x4edce9..0x4edcf9
        三句 movq 一模一样。"""
        g = self.g
        n = len(toks)
        inptr = g.alloc(max(16 * n, 16))
        buf = b""
        for tag, val in toks:
            buf += struct.pack("<Qd", tag, val)
        g.wr(inptr, buf)
        self_p = g.alloc(0x30)
        g.wq(self_p + 0x00, n)          # input.cap
        g.wq(self_p + 0x08, inptr)      # input.ptr
        g.wq(self_p + 0x10, n)          # input.len
        g.wq(self_p + 0x18, 0)          # output.cap
        g.wq(self_p + 0x20, 8)          # output.ptr = dangling（16 对齐）
        g.wq(self_p + 0x28, 0)          # output.len
        g.call(ADDR_PNC, [self_p, 0, n - 1])
        cap = g.rq(self_p + 0x18)
        ptr = g.rq(self_p + 0x20)
        ln = g.rq(self_p + 0x28)
        out = []
        if ln and ptr > 0x1000:
            for i in range(ln):
                tag = struct.unpack("<Q", g.rd(ptr + i * 16, 8))[0]
                val = struct.unpack("<d", g.rd(ptr + i * 16 + 8, 8))[0]
                out.append((tag, val))
        return out, dict(cap=cap, ptr=ptr, len=ln)


def fmt_tokens(toks):
    return "%d|%s" % (len(toks), "|".join("%d:%s" % (t, hx(v)) for t, v in toks))


def mode_real(mode, lines):
    R = Real()
    res = []
    for line in lines:
        c = line.split("|")
        cid = c[0]
        try:
            if mode == "parse":
                res.append("%s|%s" % (cid, fmt_tokens(R.parse(c[1]))))
            elif mode == "eval":
                toks = R.parse(c[1])
                v = R.eval(toks, unhex(c[2]), unhex(c[3]), unhex(c[4]))
                res.append("%s|%s" % (cid, hx(v)))
            elif mode == "path":
                toks = R.parse(c[1])
                flip, md = int(c[5]), int(c[6])
                pts = R.path(toks, unhex(c[2]), unhex(c[3]), unhex(c[4]), flip, md)
                res.append("%s|%d|%s" % (cid, len(pts),
                                         "|".join("%s:%s" % (hx(x), hx(y)) for x, y in pts)))
            elif mode == "pnc":
                toks = [(int(a.split(":")[0]), unhex(a.split(":")[1])) for a in c[1].split(",")]
                out, info = R.pnc(toks)
                res.append("%s|%s" % (cid, fmt_tokens(out)))
        except Exception as e:                              # noqa: BLE001
            res.append("%s|__ERROR__ %s: %s" % (cid, type(e).__name__, e))
    return res


def mode_ours(mode, src_file, in_file, out_file):
    exe = os.path.join(os.environ.get("CARGO_TARGET_DIR",
                                      os.path.join(ROOT, "target")),
                       "release", "examples", "bitexact_harness")
    if not os.path.exists(exe):
        raise SystemExit("找不到 %s；先跑 cargo build --release --offline --example bitexact_harness" % exe)
    # 把 out 写成标准输出再落盘，便于人肉复核
    with open(out_file, "w") as f:
        subprocess.check_call([exe, "--mode", mode, "--in", in_file], stdout=f)
    return out_file


def diff(real_lines, ours_lines, verbose=True):
    rmap = {l.split("|")[0]: l for l in real_lines}
    omap = {l.split("|")[0]: l for l in ours_lines}
    ids = sorted(set(rmap) | set(omap))
    ok, bad = [], []
    for i in ids:
        a, b = rmap.get(i, "<missing>"), omap.get(i, "<missing>")
        (ok if a == b else bad).append(i if a == b else (i, a, b))
    if verbose:
        for x in bad:
            i, a, b = x
            print("  ✗ %s" % i)
            print("      real: %s" % a[:220])
            print("      ours: %s" % b[:220])
    return ok, bad


MODES = ["parse", "eval", "path", "pnc"]


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p1 = sub.add_parser("real")
    p1.add_argument("--mode", required=True, choices=MODES)
    p1.add_argument("--in", dest="inp", required=True)
    p1.add_argument("--out", required=True)
    p2 = sub.add_parser("ours")
    p2.add_argument("--mode", required=True, choices=MODES)
    p2.add_argument("--in", dest="inp", required=True)
    p2.add_argument("--out", required=True)
    p3 = sub.add_parser("diff")
    p3.add_argument("--real", required=True)
    p3.add_argument("--ours", required=True)
    p4 = sub.add_parser("sweep")
    p4.add_argument("--in", dest="inp", default=os.path.join(ROOT, "fixtures", "bit_exact_inputs.txt"))
    p4.add_argument("--work", default="")
    a = ap.parse_args()

    if a.cmd == "real":
        secs = read_sections(a.inp)
        lines = mode_real(a.mode, secs[a.mode])
        with open(a.out, "w") as f:
            f.write("\n".join(lines) + "\n")
        print("real %-5s %d cases → %s" % (a.mode, len(lines), a.out))
        return 0
    if a.cmd == "ours":
        secs = read_sections(a.inp)
        tmp = a.out + ".in"
        with open(tmp, "w") as f:
            f.write("\n".join(secs[a.mode]) + "\n")
        mode_ours(a.mode, tmp, tmp, a.out)
        print("ours %-5s → %s" % (a.mode, a.out))
        return 0
    if a.cmd == "diff":
        ok, bad = diff(open(a.real).read().splitlines(), open(a.ours).read().splitlines())
        print("逐位相同 %d / 不同 %d" % (len(ok), len(bad)))
        return 0 if not bad else 1
    if a.cmd == "sweep":
        work = a.work or os.path.join(ROOT, ".bitexact_work")
        os.makedirs(work, exist_ok=True)
        rc = 0
        for m in MODES:
            rf = os.path.join(work, "real_%s.txt" % m)
            of = os.path.join(work, "ours_%s.txt" % m)
            subprocess.check_call([sys.executable, os.path.abspath(__file__), "real",
                                   "--mode", m, "--in", a.inp, "--out", rf])
            subprocess.check_call([sys.executable, os.path.abspath(__file__), "ours",
                                   "--mode", m, "--in", a.inp, "--out", of])
            print("== %s  (%s)" % (m, INFO[m][0]))
            ok, bad = diff(open(rf).read().splitlines(), open(of).read().splitlines())
            print("   逐位相同 %d / 不同 %d" % (len(ok), len(bad)))
            if bad:
                rc = 1
        return rc
    return 2


if __name__ == "__main__":
    sys.exit(main())
