#!/usr/bin/env python3
"""真二进制对拍台：`math_function` 的**求值器**（`0x4eb9d0`）与**解析器**（`0x4ed670`）。

用 unicorn 把 `libgraphwarrust.so` 映射进内存，直接调这两个函数，
把我们自己的 `parse` / `eval` 与它**逐位**（`f64::to_bits`）比较。

## 环境

```bash
WS=<仓库根>
export EMU=$WS/.reference/analysis-repo          # 上游分析仓库（含 analysis/emu.py）
export SO=$WS/.reference/steam/2026-08-28/libgraphwarrust.so   # sha256 9a538115…
$WS/.reference/venv/bin/python tools/eval_oracle.py --sweep
```

`$EMU/libgraphwarrust.so` 必须软链到上面那份（`analysis/emu.py` 从自己目录找 so）。
本脚本**不依赖网络、不写仓库**。

## 三种跑法

```bash
# ① 边角语义表（log/ln/exp/div/sqrt/pow 的定义域与 NaN 符号）—— 修 m_log10 那一批用的就是它
tools/eval_oracle.py --sweep

# ② 最小复现（回放残差里挑的 8 条，print 三方：真二进制 / 我方 / 回放观测）
tools/eval_oracle.py --repro

# ③ 全量逐发比较：先由 Rust 侧导出（每发一条）
cargo run --release --offline --example eval_oracle -- --dir <快照> --json /tmp/evalcases.jsonl
tools/eval_oracle.py --cases /tmp/evalcases.jsonl
```

## 为什么要把数学导入换成 libm（**不是** python 的 `math`）

`analysis/emu.py` 把二进制的动态导入 `log`/`log10`/`exp`/`sin`… 映射到 python 的 `math`，
而 python 对**定义域外**入参是**抛异常**（再被 `_safe` 兜成 `NaN`），真 libm 是**返回值**：
`log(0) = -inf`、`exp(1000) = inf`。旧夹具因此把 `log(0)` 记成 NaN —— 那是 oracle 的假象，
不是二进制的行为（`docs/06` §30.3）。本脚本用 `ctypes` 直接调 `libm.so.6`。

## 已知限制（写在这里，免得下次又踩）

* 引擎侧只映射到 `0x4eb9d0` / `0x4ed670` 两个函数，其余导入（malloc/memmove/…）走
  `analysis/emu.py` 的桩；参数必须是**纯 token 列表 + x**，不能带真实 `Match` 状态。
* 一次 `Gw()` 常驻，`--cases` 模式逐行复用；遇到未实现的导入会抛 `unhandled import`。
"""

import argparse
import collections
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)

ENV_EMU = os.environ.get("EMU", "/home/CG/Myprojects/graphwar2-rebuild/.reference/analysis-repo")
ENV_SO = os.environ.get(
    "SO", "/home/CG/Myprojects/graphwar2-rebuild/.reference/steam/2026-08-28/libgraphwarrust.so"
)


def load_oracle():
    """导入上游 `analysis/emu.py` + `gwemu.py`，并把数学导入换成真 libm。"""
    sys.path.insert(0, os.path.join(ENV_EMU, "analysis"))
    sys.path.insert(0, ENV_EMU)
    try:
        import emu
        from gwemu import Gw
    except Exception as exc:  # pragma: no cover
        raise SystemExit(
            "导入上游 emu 失败（%s）。把 EMU 指到分析仓库根（含 analysis/emu.py、"
            "libgraphwarrust.so 软链）：%s" % (exc, ENV_EMU)
        )
    import ctypes

    libm = ctypes.CDLL("libm.so.6", use_errno=True)
    for name in list(emu.MATH):
        fn = getattr(libm, name, None)
        if fn is None:
            continue
        fn.restype = ctypes.c_double
        fn.argtypes = [ctypes.c_double]
        emu.MATH[name] = (lambda f: (lambda v: f(ctypes.c_double(v))))(fn)

    # ---- `pow` 也要换成真 libm（2026-11-08，`dispatch/52` 差分矩阵）----
    #
    # 上游 `analysis/emu.py::_do_import` 对 `pow` 是**特判**的：它算 python 的 `a ** b`。
    # 定义域外 python 返回 **complex**，被那段代码兜成 `float('nan')` —— 也就是**正** NaN
    # （`7ff8000000000000`）；而二进制真的从 glibc 拿 `pow`（`UND pow@GLIBC_2.2.5`），
    # glibc 的 `pow(-2.5, 0.5)` 给的是**负** NaN（`fff8000000000000`）。
    # ⇒ 与 §30.3 那条 `log(0)` 是同一类**oracle 假象**，不换掉就会在矩阵里刷出
    # 一堆假的"求值不一致"（本仓库旧矩阵里 205 个 eval 槽位就是这么来的）。
    #
    # 换法：拦 `_do_import`，只劫 `pow` 这一条导入，其余原样交回上游实现。
    # ⚠️ 劫持分支必须**自己做上游那段收尾**（弹返回地址进 RSP/RIP），
    # 否则 emulator 会继续在导入桩上执行 —— 表现为 `UncError: Unhandled CPU exception`。
    libm_pow = libm.pow
    libm_pow.restype = ctypes.c_double
    libm_pow.argtypes = [ctypes.c_double, ctypes.c_double]
    _orig_do_import = emu.Emu._do_import

    def _do_import(self, address):
        if self.imports[address].split("@")[0] == "pow":
            from unicorn.x86_const import UC_X86_REG_RIP, UC_X86_REG_RSP

            a, b = self.xmm_get(0), self.xmm_get(1)
            self.xmm_set(0, libm_pow(ctypes.c_double(a), ctypes.c_double(b)))
            uc = self.uc
            sp = uc.reg_read(UC_X86_REG_RSP)
            raddr = self.rq(sp)
            uc.reg_write(UC_X86_REG_RSP, sp + 8)
            uc.reg_write(UC_X86_REG_RIP, raddr)
            return
        return _orig_do_import(self, address)

    emu.Emu._do_import = _do_import
    return Gw


_G = None
_CALLS = 0


def G(Gw):
    global _G
    if _G is None:
        _G = Gw()
    return _G


def reset_heap(Gw):
    """每次比较前把 emu 的分配指针放回堆底。

    为什么必须有：`analysis/emu.py` 的 `alloc()` 是**只进不退**的 bump allocator，
    `free` 是空操作。逐发跑 1290 行会累积到 64 MB 堆顶（实测第 ~470 行起
    `RuntimeError('emulated heap exhausted')`）。每次调用只用当次的数据，
    下一次调用前把指针放回去是安全的（真二进制不跨调用持有这些指针）。
    """
    g = G(Gw)
    import emu

    g.heap_ptr = emu.HEAP
    g.sizes.clear()


def bits(v):
    return struct.unpack("<Q", struct.pack("<d", v))[0]


def from_bits(h):
    return struct.unpack("<d", struct.pack("<Q", int(h, 16) & 0xFFFFFFFFFFFFFFFF))[0]


def hx(v):
    return "%016x" % bits(v)


# ---------------------------------------------------------------------------
# 两个真函数
# ---------------------------------------------------------------------------
def call_parse(Gw, text):
    """`0x4ed670`：文本 -> `Vec<Token>`（16 字节/token，`{tag: u64, value: f64}`）。

    ⚠️ 这个函数**就地**把输入串转小写，所以每次都造一份新的 `String`。
    Vec 字段顺序 `{cap@0, ptr@8, len@0x10}`（与 `evaluate_recursive` 读的一致）。
    """
    g = G(Gw)
    b = text.encode()
    p = g.alloc(max(len(b), 1))
    g.wr(p, b)
    s = g.alloc(24)
    g.wq(s, max(len(b), 1))
    g.wq(s + 8, p)
    g.wq(s + 16, len(b))
    out = g.alloc(24)
    g.wq(out, 0)
    g.wq(out + 8, 0)
    g.wq(out + 16, 0)
    g.call(0x4ED670, [out, s], [0.0, 0.0, 0.0])
    cap, ptr, ln = g.rq(out), g.rq(out + 8), g.rq(out + 16)
    if ln > 100000 or ptr == 0:
        raise RuntimeError("解析结果不可信 cap=%d ptr=%#x len=%d" % (cap, ptr, ln))
    return [(g.rq(ptr + i * 16), g.rd_f64(ptr + i * 16 + 8)) for i in range(ln)]


def call_eval(Gw, tokens, x, y=0.0, dy=0.0, start=0):
    """`0x4eb9d0`：`evaluate_recursive(tokens, &mut i, x, y, dy) -> xmm0`。

    `start` 就是 `&mut i` 的初值 —— 传子树起点即可**单独求值任意子式**（定位用）。
    """
    from unicorn.x86_const import UC_X86_REG_XMM0

    g = G(Gw)
    n = len(tokens)
    ptr = g.alloc(max(16 * n, 16))
    blob = b"".join(struct.pack("<Qd", int(t), float(v)) for t, v in tokens)
    g.wr(ptr, blob)
    vec = g.alloc(24)
    g.wq(vec, n)
    g.wq(vec + 8, ptr)
    g.wq(vec + 16, n)
    idx = g.alloc(8)
    g.wq(idx, start)
    g.call(0x4EB9D0, [vec, idx, None, None], [x, y, dy])
    raw = g.uc.reg_read(UC_X86_REG_XMM0) & 0xFFFFFFFFFFFFFFFF
    return struct.unpack("<d", struct.pack("<Q", raw))[0]


# ---------------------------------------------------------------------------
# ① 边角语义表
# ---------------------------------------------------------------------------
SWEEP = [
    ("log(0)", [(10, 0.0), (17, 0.0)], 0.0),
    ("log(-0.0)", [(10, 0.0), (17, -0.0)], 0.0),
    ("log(-1)", [(10, 0.0), (5, 0.0), (17, 1.0)], 0.0),
    ("log(1e-320)", [(10, 0.0), (17, 1e-320)], 0.0),
    ("ln(0)", [(11, 0.0), (17, 0.0)], 0.0),
    ("ln(-2)", [(11, 0.0), (5, 0.0), (17, 2.0)], 0.0),
    ("exp(1000)", [(12, 0.0), (17, 1000.0)], 0.0),
    ("exp(-1000)", [(12, 0.0), (17, -1000.0)], 0.0),
    ("sqrt(-1)", [(9, 0.0), (5, 0.0), (17, 1.0)], 0.0),
    ("sqrt(-0.0)", [(9, 0.0), (17, -0.0)], 0.0),
    ("1/0", [(7, 0.0), (17, 1.0), (17, 0.0)], 0.0),
    ("-1/0", [(7, 0.0), (5, 0.0), (17, 1.0), (17, 0.0)], 0.0),
    ("1/-0.0", [(7, 0.0), (17, 1.0), (17, -0.0)], 0.0),
    ("0/0", [(7, 0.0), (17, 0.0), (17, 0.0)], 0.0),
    ("pow(-1,0.5)", [(8, 0.0), (5, 0.0), (17, 1.0), (17, 0.5)], 0.0),
    ("pow(0,-1)", [(8, 0.0), (17, 0.0), (5, 0.0), (17, 1.0)], 0.0),
    ("abs(-inf)", [(13, 0.0), (7, 0.0), (5, 0.0), (17, 1.0), (17, 0.0)], 0.0),
]


def run_sweep(Gw):
    print("%-14s %-22s %s" % ("表达式", "真二进制 0x4eb9d0", "说明"))
    for name, toks, x in SWEEP:
        v = call_eval(Gw, toks, x)
        note = ""
        if name in ("log(0)", "log(-0.0)", "ln(0)"):
            note = "← libm 给 -inf（python math 会给 NaN，旧夹具就是错的）"
        print("%-14s %-22r %016x %s" % (name, v, bits(v), note))


# ---------------------------------------------------------------------------
# ② 最小复现（回放残差里的 8 条）
# ---------------------------------------------------------------------------
# (回放文件, corrected_function 文本, 求值用的 x, 回放 ExplosionEffect.pos_y 的位)
REPRO = [
    ("1782672301", "x + 35.910", -20.911460745133482, 0x402DFF408986D909),
    ("1782673792", "x + 11.649", 3.3506886740401742, 0x402DFFD731A25E4A),
    ("1782646337", "-0.29(x)^2 + 10.691", 6.322417849086799, 0xBFECD64EAD4A355F),
    ("1783342620", "sqrt(abs(175-x^2)-1) - 0.112", 13.19090509020422, 0xBFBB7268730D6E4D),
    ("1783342620", "-x + 2.457", 17.455988098017656, 0xC02DFF7B5E34287D),
    ("1782642880", "sin(x6cos(5)) - 10.615", -1.0476229930439582, 0xC0272F6480BFC54E),
    ("1782646897", "1/(100(x-10)) + 6.282", 9.99952991831176, 0xC02DFB57096FA9B3),
    ("1783342620", "sqrt(4-4sin(x)^2) + 0.221", 24.992373421336396, 0x40019C527CF1AD20),
]


def run_repro(Gw):
    print("%-30s %-22s %-18s %-18s %s" % ("corrected_function", "x", "真二进制", "回放 pos_y", "差"))
    bad = 0
    for name, text, x, obs in REPRO:
        toks = call_parse(Gw, text)
        got = bits(call_eval(Gw, toks, x))
        d = obs - got
        bad += 0 if abs(d) == 1 else 1
        print("%-30s %-22r %016x %016x %+d ulp" % (text, x, got, obs, d))
    print("\n%d/%d 条真二进制与回放**恰好差 1 ulp**（这就是 §25.2 那 154 批的形态）"
          % (len(REPRO) - bad, len(REPRO)))


# ---------------------------------------------------------------------------
# ③ 全量逐发比较
# ---------------------------------------------------------------------------
def run_cases(Gw, path, limit):
    rows = [json.loads(l) for l in open(path, encoding="utf-8")]
    rows = rows[:limit]
    stat = collections.Counter()
    bad = []
    for r in rows:
        reset_heap(Gw)
        text = r["fn"]
        if not text:
            stat["empty"] += 1
            continue
        try:
            rtok = call_parse(Gw, text)
        except Exception as exc:
            stat["parse_err"] += 1
            bad.append(("parse_err", r["file"], r["shot"], text, repr(exc)))
            continue
        ours = [(t, from_bits(b)) for t, b in r["tokens"]]
        if [t for t, _ in rtok] != [t for t, _ in ours]:
            stat["tokens_differ"] += 1
            bad.append(("tags", r["file"], r["shot"], text,
                        [t for t, _ in rtok], [t for t, _ in ours]))
            continue
        if any(a[0] == 17 and a[1] != b[1] for a, b in zip(rtok, ours)):
            stat["const_differ"] += 1
            bad.append(("const", r["file"], r["shot"], text, rtok, ours))
            continue
        stat["tokens_ok"] += 1
        x = from_bits(r["x_bits"])
        try:
            got = call_eval(Gw, rtok, x)
        except Exception as exc:
            stat["eval_err"] += 1
            bad.append(("evalerr", r["file"], r["shot"], text, repr(exc), x))
            continue
        want = from_bits(r["ours_bits"])
        if bits(got) == bits(want) or (got != got and want != want):
            stat["eval_ok"] += 1
        else:
            stat["eval_differ"] += 1
            bad.append(("eval", r["file"], r["shot"], text, hx(got), hx(want), x))
    print("行数 %d" % len(rows))
    for k in ("tokens_ok", "tokens_differ", "const_differ", "eval_ok", "eval_differ",
              "parse_err", "eval_err", "empty"):
        if stat[k]:
            print("  %-14s %d" % (k, stat[k]))
    for m in bad[:10]:
        print("  !", str(m)[:220])
    return stat


def main():
    ap = argparse.ArgumentParser(description="math_function 求值器/解析器的真二进制对拍台")
    ap.add_argument("--sweep", action="store_true", help="边角语义表")
    ap.add_argument("--repro", action="store_true", help="最小复现 8 条")
    ap.add_argument("--cases", help="examples/eval_oracle.rs 导出的逐发 JSONL")
    ap.add_argument("--limit", type=int, default=10 ** 9)
    ap.add_argument("--so", default=ENV_SO, help="真二进制路径（覆盖 SO 环境变量）")
    args = ap.parse_args()
    if not os.path.exists(args.so):
        raise SystemExit("找不到真二进制：%s" % args.so)
    link = os.path.join(ENV_EMU, "libgraphwarrust.so")
    if not os.path.exists(link):
        raise SystemExit("缺 %s —— 软链到 %s" % (link, args.so))
    print("oracle: %s\n" % os.path.realpath(link))
    Gw = load_oracle()
    if args.sweep:
        run_sweep(Gw)
    if args.repro:
        run_repro(Gw)
    if args.cases:
        run_cases(Gw, args.cases, args.limit)
    if not (args.sweep or args.repro or args.cases):
        ap.print_help()


if __name__ == "__main__":
    main()
