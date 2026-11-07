#!/usr/bin/env python3
"""`C` 桶（`corrected_function` 文本不符）14 批的**逐位对拍台** —— `docs/06-verification.md` §41 的证据生成器。

跑法（与 `tools/eval_oracle.py` 同一套 unicorn 环境）：

```bash
WS=<仓库根>                     # 含 analysis/emu.py 与 libgraphwarrust.so 软链的上游仓库
$WS/.venv/bin/python tools/nan_c_probe.py
```

它直接调**真二进制的三个函数**，把「真值 / 我方 / 回放」摆在一起：

| 地址（ELF 虚地址） | 是什么 |
|---|---|
| `0x4ed670` | `MathFunction` 的解析器（文本 → `Vec<Token>`，16 B/token `{tag:u64, value:f64}`） |
| `0x4eb9d0` | `evaluate_recursive`（求值器） |
| `0x3c5f70` | `GraphwarShotCalculator::calculate_function_offset_str`（**产生 `" ± c.ddd"` 的那一步**） |

`0x3c5f70` 的签名（`objdump` 实读，见 §41.3）：`(sret String, text.ptr, text.len, *const Point{x,y}, flip)`；
内部就是 `c = y0 − f(flip ? −x0 : x0)`，`f` 走同一个求值器，最后 `{:.3}` 印成 `" - x.xxx"` / `" + x.xxx"`。

**这个脚本要回答的问题**：`C` 桶那 14 批「回放印 `NaN`、我们印数值」到底是
「解析器/求值器/偏移函数」哪里不一样，还是**喂进去的 `Point` 不一样**。
结论写死在下面的 `CASES` 注释里，脚本只负责复现。
"""

import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)


def find_analysis_repo(start):
    """往上找含 `analysis/emu.py` 的仓库根（本地 clone 可能嵌在 `.agent-work/` 里）。"""
    d = start
    for _ in range(8):
        if os.path.exists(os.path.join(d, "analysis", "emu.py")):
            return d
        nxt = os.path.dirname(d)
        if nxt == d:
            break
        d = nxt
    return start


WS = find_analysis_repo(os.path.dirname(REPO))
os.environ.setdefault("EMU", WS)
os.environ.setdefault("SO", os.path.join(WS, "libgraphwarrust.so"))
sys.path.insert(0, HERE)

import eval_oracle as EO  # noqa: E402  （它会 import 上游 emu/gwemu）

ADDR_PARSE = 0x4ED670
ADDR_EVAL = 0x4EB9D0
ADDR_OFFSET = 0x3C5F70


def fb(hexbits):
    """hex bits（大端写法，与 `f64::to_bits` 的十六进制）→ f64。"""
    return struct.unpack("<d", struct.pack("<Q", int(hexbits, 16)))[0]


def fbits(v):
    return "%016x" % struct.unpack("<Q", struct.pack("<d", v))[0]


def offset_str(Gw, text, x0, y0, flip=0):
    """调真 `calculate_function_offset_str`：返回 `" + 1.197"` / `" + NaN"` 这种后缀串。"""
    g = EO.G(Gw)
    EO.reset_heap(Gw)
    b = text.encode()
    tp = g.alloc(max(len(b), 1))
    g.wr(tp, b)
    pt = g.alloc(16)
    g.wq(pt, struct.unpack("<Q", struct.pack("<d", x0))[0])
    g.wq(pt + 8, struct.unpack("<Q", struct.pack("<d", y0))[0])
    out = g.alloc(24)
    for off in (0, 8, 16):
        g.wq(out + off, 0)
    # rdi=out(String) rsi=text.ptr rdx=text.len rcx=&Point r8=flip
    g.call(ADDR_OFFSET, [out, tp, len(b), pt, flip], [0.0, 0.0, 0.0])
    _cap, ptr, ln = g.rq(out), g.rq(out + 8), g.rq(out + 16)
    if ln == 0 or ptr == 0 or ln > 10000:
        return "<empty>"
    return g.rd(ptr, ln).decode("utf-8", "replace")


def eval_at(Gw, tokens, x):
    EO.reset_heap(Gw)
    return EO.call_eval(Gw, tokens, x)


# 每一行 = (标签, 函数原文, 报文 start_x 的 bits, 报文 start_y, 回放印出来的后缀)
# 前 4 条是 `C` 桶（回放与我们不符）；后 2 条是**同一批语料里我们本来就对**的
# 巨幅 `c` 批（`|c| ≥ 1e13`），它们是「x 只能等于 start_x」的定标点。
CASES = [
    ("1783267628#15", "sin(x+14.29047676041342374)/(x+14.29047676041342374)",
     "c02c94b95eb47287", 2.196978210960218, " + NaN"),
    ("1783267628#24", "sin(x+14.290476760413424)/(x+14.290476760413424)",
     "c02c94b95eb47287", 2.196978210960218, " + 1.197"),
    ("1783267628#28", "sin(x+14.290476760413421)/(x+14.29047676041341)",
     "c02c94b95eb47287", 2.196978210960218, " + 2.054"),
    ("1783268357#0", "sin(x+23.72311439353351)/(x+23.72311439353351)",
     "c037b91e065f7e49", 0.0324569185983119, " - 0.968"),
    ("1793873040#1", "9999999999999999x",
     "c032bac76b1b8d9d", 9.5360443368328, " + 187296053831356512.000"),
    ("1793029143#0", "-9999999999999999999999999x",
     "c002e88db1d82d82", 7.065419066086733, " - 23635515112015936427655168.000"),
]


def main():
    Gw = EO.load_oracle()
    print("== 真 `calculate_function_offset_str`（0x3c5f70）按**报文 start_x** 的产出 ==")
    print(f"{'批':14s} {'start_x':>24s} {'真函数产出':>44s} {'回放':>44s}")
    bad = 0
    for label, text, xb, y0, obs in CASES:
        x0 = fb(xb)
        got = offset_str(Gw, text, x0, y0)
        ok = "OK " if got == obs else "DIFF"
        if got != obs:
            bad += 1
        print(f"{label:14s} {repr(x0):>24s} {got:>44s} {obs:>44s}  {ok}")

    print()
    print("== 真求值器在 start_x / start_x∓1ulp 上的 `f`（± 只影响 `sin(u)/u` 的 u 是否为 0）==")
    for label, text, xb, _y0, obs in CASES[:4]:
        toks = EO.call_parse(Gw, text)
        x0 = fb(xb)
        cols = []
        for name, dx in (("x0", 0), ("toward0", 1), ("away", -1)):
            x = f64_shift(x0, dx)
            v = eval_at(Gw, toks, x)
            cols.append(f"{name}={v!r}")
        print(f"{label:14s} " + "  ".join(cols))
    print()
    print("判定：前 4 条（C 桶）只有把 point.x 挪 1 ulp 向零才复现回放；")
    print("      后 2 条（巨幅 c）只有用报文 start_x 本值才复现回放 ⇒ 差的是**调用方的 Point**。")
    print()
    print(f"汇总：{bad}/{len(CASES)} 条与回放不符（应当只有前 4 条；不符在后 2 条上说明模型反了）")
    return 0


def f64_shift(x, k):
    """按幅值挪 k 个 ulp（k>0 = 向零）。"""
    if x == 0.0 or x != x:
        return x
    return struct.unpack("<d", struct.pack("<Q", struct.unpack("<Q", struct.pack("<d", x))[0] - k))[0]


if __name__ == "__main__":
    sys.exit(main())
