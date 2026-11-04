#!/usr/bin/env python3
"""把 Rust 光栅化结果与**真二进制**的输出逐字节对比。

oracle：analysis/gwemu.py 用 unicorn 直接调用 `0x3f11e0`
（签名 `f(x, y, size, mask_ptr, mask_len, fill)`，不依赖上下文对象）。
被测：构建出的 `.so` 里的 `gw2_rasterize`，用 ctypes 调用。

用法：../.venv/bin/python tools/mask_check.py
"""
import ctypes, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)
sys.path.insert(0, os.path.join(ROOT, "analysis"))
from gwemu import Gw  # noqa: E402

N = 1000 * 600
SO = os.path.join(ROOT, ".cargo-home/target/release/libgw2_rebuild.so")

CASES = [
    (0.0, 0.0, 1.0, 0x00),
    (10.0, -5.0, 1.0, 0x00),
    (-7.5, 3.25, 0.6, 0x00),
    (0.0, 0.0, 2.0, 0x10),
    (24.5, 14.0, 1.0, 0x00),   # 贴边：检查裁剪
]

def oracle(g, x, y, size, fill):
    ptr = g.alloc(N)
    g.wr(ptr, b"\xff" * N)
    try:
        g.call(0x3f11e0, [ptr, N, fill], [x, y, size])
    except Exception:
        # 该函数返回 void，模拟器却把 RAX 当指针读；结果已在内存里，忽略即可
        pass
    return bytes(g.rd(ptr, N))

def main():
    g = Gw()
    lib = ctypes.CDLL(SO)
    lib.gw2_rasterize.restype = ctypes.c_int64
    lib.gw2_rasterize.argtypes = [ctypes.c_double, ctypes.c_double, ctypes.c_double,
                                  ctypes.POINTER(ctypes.c_ubyte), ctypes.c_size_t, ctypes.c_ubyte]
    buf = (ctypes.c_ubyte * N)()
    total_diff = 0
    for (x, y, size, fill) in CASES:
        exp = oracle(g, x, y, size, fill)
        ctypes.memset(buf, 0xFF, N)
        rc = lib.gw2_rasterize(x, y, size, buf, N, fill)
        got = bytes(buf)
        diff = sum(1 for a, b in zip(exp, got) if a != b)
        total_diff += diff
        tag = "ok  " if diff == 0 else "DIFF"
        print(f"{tag} x={x:6} y={y:6} size={size} fill=0x{fill:02x}  差异像素 {diff}")
        if diff:
            shown = 0
            for i, (a, b) in enumerate(zip(exp, got)):
                if a != b and shown < 4:
                    print(f"      idx={i} ({i%1000},{i//1000}) oracle={a:#04x} ours={b:#04x}")
                    shown += 1
    print("\nRESULT:", "PASS" if total_diff == 0 else f"FAIL（累计 {total_diff} 像素不同）")
    return 0 if total_diff == 0 else 2

if __name__ == "__main__":
    sys.exit(main())
