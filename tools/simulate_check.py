#!/usr/bin/env python3
"""开火模拟的装配校验：Rust 一次调用 vs 用真二进制积木拼出的同一流水线。

oracle 侧（三段全部调真二进制）：
  1. 掩膜：alloc + memset(0xFF)，每个障碍调 `0x3f11e0` 光栅化
  2. 采点：`graphwar_curve` 算修正后调 `0x3c2530`
  3. 判定：逐点调 `0x3f15b0`

被测：构建出的 .so 里的 `gw2_simulate`（一次调用完成同样的事）。

两边用的积木各自都已单独对拍过；这一步校验的是**装配**（循环边界、顺序）。
"""
import ctypes, os, struct, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE); ROOT = os.path.dirname(SUB)
sys.path.insert(0, os.path.join(ROOT, "analysis"))
sys.path.insert(0, ROOT)
from gwemu import Gw                                  # noqa: E402
from unicorn.x86_const import UC_X86_REG_RAX          # noqa: E402
import graphwar_curve as pc                           # noqa: E402

N = 1000 * 600
SO = os.path.join(ROOT, ".cargo-home/target/release/libgw2_rebuild.so")


class Oracle:
    def __init__(self):
        self.g = Gw()
        self.mask = self.g.alloc(N)
        inner = self.g.alloc(0x40)
        self.g.wq(inner + 0x10, 0); self.g.wr(inner + 0x14, b"\x00")
        self.g.wq(inner + 0x20, self.mask); self.g.wq(inner + 0x28, N)
        self.ctx = self.g.alloc(0x20); self.g.wq(self.ctx + 0x18, inner)
        self.pt = self.g.alloc(16)

    def build_mask(self, obstacles):
        self.g.wr(self.mask, b"\xff" * N)
        for (ox, oy, osz) in obstacles:
            try:
                self.g.call(0x3f11e0, [self.mask, N, 0x00], [ox, oy, osz])
            except Exception:
                pass

    def hit(self, x, y):
        self.g.wr(self.pt, struct.pack("<2d", x, y))
        try:
            self.g.call(0x3f15b0, [self.ctx, self.pt], [])
        except Exception:
            pass
        return self.g.uc.reg_read(UC_X86_REG_RAX) & 0xFF

    def run(self, text, x0, y0, step, flip, ode, obstacles):
        self.build_mask(obstacles)
        toks = pc.parse_function(text)
        if not toks:
            return (-1, 0)
        if not ode:
            toks = pc.shifted_tokens(toks, pc.correction(toks, x0, y0, flip))
        pts = self.g.call_path(toks, x0, y0, step, int(flip), int(ode))
        for i, (x, y) in enumerate(pts):
            if self.hit(x, y):
                return (i, len(pts))
        return (-1, len(pts))


CASES = [
    ("sin(x)", 5.0, 2.0, 0.01, 0, 0, []),
    ("sin(x)", 5.0, 2.0, 0.01, 0, 0, [(8.0, 2.0, 0.3)]),
    ("sin(x)", 5.0, 2.0, 0.01, 0, 0, [(8.0, -12.0, 0.3)]),
    ("x/5", -10.0, -2.0, 0.01, 0, 0, [(0.0, 0.0, 0.5), (10.0, 2.0, 0.4)]),
    ("y", 0.0, 1.0, 0.05, 0, 1, [(5.0, 5.0, 0.5)]),
]


def main():
    o = Oracle()
    lib = ctypes.CDLL(SO)
    lib.gw2_simulate.restype = ctypes.c_int64
    lib.gw2_simulate.argtypes = [ctypes.c_char_p, ctypes.c_double, ctypes.c_double,
                                 ctypes.c_double, ctypes.c_uint8, ctypes.c_uint8,
                                 ctypes.POINTER(ctypes.c_double), ctypes.c_size_t,
                                 ctypes.POINTER(ctypes.c_int64)]
    bad = 0
    for (text, x0, y0, step, flip, ode, obstacles) in CASES:
        exp_hit, exp_n = o.run(text, x0, y0, step, bool(flip), bool(ode), obstacles)
        flat = [v for obs in obstacles for v in obs] or [0.0]
        arr = (ctypes.c_double * len(flat))(*flat)
        cnt = ctypes.c_int64(-1)
        got_hit = lib.gw2_simulate(text.encode(), x0, y0, step, flip, ode,
                                   arr, len(obstacles), ctypes.byref(cnt))
        ok = (got_hit == exp_hit) and (cnt.value == exp_n)
        bad += 0 if ok else 1
        print(f"{'ok  ' if ok else 'FAIL'} {text!r:10} 障碍{len(obstacles)} "
              f"命中 oracle={exp_hit} ours={got_hit}  点数 oracle={exp_n} ours={cnt.value}")
    print("\nRESULT:", "PASS" if bad == 0 else f"FAIL ({bad})")
    return 0 if bad == 0 else 2


if __name__ == "__main__":
    sys.exit(main())
