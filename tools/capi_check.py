#!/usr/bin/env python3
"""用 ctypes 直接调用构建出的 .so，验证它算的曲线与游戏一致。

这是"能构建出二进制"的**功能证据**：不只是符号存在，而是产物确实复现了算法。

真值来自 `graphwar_curve.py`（上游已验证与真二进制逐位一致）。
"""
import ctypes, glob, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)
sys.path.insert(0, ROOT)
import graphwar_curve as pc

def find_so():
    pats = [os.path.join(ROOT, ".cargo-home/target/release/libgw2_rebuild.so"),
            os.path.join(SUB, "target/release/libgw2_rebuild.so")]
    for p in pats:
        if os.path.exists(p): return p
    g = glob.glob(os.path.join(ROOT, "**/libgw2_rebuild.so"), recursive=True)
    return g[0] if g else None

def main():
    so_path = find_so()
    if not so_path:
        print("找不到 libgw2_rebuild.so，先 cargo build --release"); return 1
    lib = ctypes.CDLL(so_path)
    lib.gw2_shot_points.restype = ctypes.c_int64
    lib.gw2_shot_points.argtypes = [ctypes.c_char_p, ctypes.c_double, ctypes.c_double,
                                    ctypes.c_double, ctypes.c_uint8, ctypes.c_uint8,
                                    ctypes.POINTER(ctypes.c_double), ctypes.c_size_t]
    lib.gw2_correction.restype = ctypes.c_double
    lib.gw2_correction.argtypes = [ctypes.c_char_p, ctypes.c_double, ctypes.c_double, ctypes.c_uint8]

    cap = 40000
    buf = (ctypes.c_double * (cap * 2))()
    cases = [("sin(x)", 5.0, 2.0, 0.01, 0, 0), ("x^2/10 - 3", -7.0, 4.0, 0.01, 0, 0),
             ("sqrt(abs(x))", 0.0, -5.0, 0.01, 1, 0), ("2*x+1", 3.0, 1.0, 0.05, 0, 0),
             ("log(abs(x)+1)", -12.0, 6.0, 0.02, 0, 0)]
    bad = 0
    for text, x0, y0, step, flip, ode in cases:
        n = lib.gw2_shot_points(text.encode(), x0, y0, step, flip, ode, buf, cap)
        if n < 0:
            print(f"FAIL {text!r}: 返回 {n}"); bad += 1; continue
        got = [(buf[2*i], buf[2*i+1]) for i in range(n)]
        exp = pc.shot_points(text, x0, y0, step=step, flip=bool(flip), ode=bool(ode))
        if len(got) != len(exp):
            print(f"FAIL {text!r}: 点数 {len(got)} != {len(exp)}"); bad += 1; continue
        for i, (g, e) in enumerate(zip(got, exp)):
            if g != e:
                print(f"FAIL {text!r}: 第 {i} 点 {g} != {e}"); bad += 1; break
        else:
            c_so = lib.gw2_correction(text.encode(), x0, y0, flip)
            c_py = pc.correction(pc.parse_function(text), x0, y0, bool(flip))
            okc = (c_so == c_py) or (c_so != c_so and c_py != c_py)
            print(f"{'ok  ' if okc else 'FAIL'} {text!r:22} {n:5d} 点  修正 {c_so:+.6f}")
            bad += 0 if okc else 1
    print("\nRESULT:", "PASS" if bad == 0 else f"FAIL ({bad} 项)")
    return 0 if bad == 0 else 2

if __name__ == "__main__":
    sys.exit(main())
