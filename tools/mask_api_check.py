#!/usr/bin/env python3
"""掩膜 API 的随机对拍：单点测试 + 光栅化。

- oracle：unicorn 调用真二进制的 `0x3f15b0`（点测试）与 `0x3f11e0`（光栅化）
- 被测：构建出的 .so 里的 `gw2_is_occupied` / `gw2_rasterize`（ctypes）

点测试需要伪造上下文：ctx[0x18] -> inner，
inner[+0x10]=锁字, +0x14=毒标志, +0x20=数据指针, +0x28=长度。
"""
import ctypes, os, random, struct, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE); ROOT = os.path.dirname(SUB)
sys.path.insert(0, os.path.join(ROOT, "analysis"))
from gwemu import Gw                      # noqa: E402
from unicorn.x86_const import UC_X86_REG_RAX  # noqa: E402

N = 1000 * 600
SO = os.path.join(ROOT, ".cargo-home/target/release/libgw2_rebuild.so")


class Oracle:
    def __init__(self):
        self.g = Gw()
        self.mask = self.g.alloc(N)
        inner = self.g.alloc(0x40)
        self.g.wq(inner + 0x10, 0)
        self.g.wr(inner + 0x14, b"\x00")
        self.g.wq(inner + 0x20, self.mask)
        self.g.wq(inner + 0x28, N)
        self.ctx = self.g.alloc(0x20)
        self.g.wq(self.ctx + 0x18, inner)
        self.pt = self.g.alloc(16)

    def write_mask(self, blob):
        self.g.wr(self.mask, blob)

    def read_mask(self):
        return bytes(self.g.rd(self.mask, N))

    def hit(self, x, y):
        self.g.wr(self.pt, struct.pack("<2d", x, y))
        try:
            self.g.call(0x3f15b0, [self.ctx, self.pt], [])
        except Exception:
            pass
        return self.g.uc.reg_read(UC_X86_REG_RAX) & 0xFF

    def rasterize(self, x, y, size, fill):
        self.g.wr(self.mask, b"\xff" * N)
        try:
            self.g.call(0x3f11e0, [self.mask, N, fill], [x, y, size])
        except Exception:
            pass
        return self.read_mask()


def main():
    rnd = random.Random(1234)
    o = Oracle()
    lib = ctypes.CDLL(SO)
    lib.gw2_is_occupied.restype = ctypes.c_int64
    lib.gw2_is_occupied.argtypes = [ctypes.POINTER(ctypes.c_ubyte), ctypes.c_size_t,
                                    ctypes.c_double, ctypes.c_double]
    lib.gw2_rasterize.restype = ctypes.c_int64
    lib.gw2_rasterize.argtypes = [ctypes.c_double] * 3 + [ctypes.POINTER(ctypes.c_ubyte),
                                                          ctypes.c_size_t, ctypes.c_ubyte]

    bad = 0
    # ---- 单点测试：随机掩膜 + 随机坐标（含越界）----
    blob = bytearray(b"\xff" * N)
    for _ in range(400):
        blob[rnd.randrange(N)] = rnd.randrange(0, 0xFF)
    o.write_mask(bytes(blob))
    buf = (ctypes.c_ubyte * N).from_buffer_copy(bytes(blob))
    for _ in range(300):
        x = rnd.uniform(-30, 30)
        y = rnd.uniform(-20, 20)
        if o.hit(x, y) != lib.gw2_is_occupied(buf, N, x, y):
            bad += 1
            if bad < 4:
                print(f"  DIFF hit x={x:.3f} y={y:.3f} oracle={o.hit(x,y)}")
    print(f"点测试：300 个随机点，差异 {bad}")

    # ---- 光栅化：随机位置与半径 ----
    rbad = 0
    rb = (ctypes.c_ubyte * N)()
    for i in range(6):
        x = rnd.uniform(-24, 24); y = rnd.uniform(-14, 14); size = rnd.uniform(0.2, 1.5)
        fill = 0x00
        exp = o.rasterize(x, y, size, fill)
        ctypes.memset(rb, 0xFF, N)
        lib.gw2_rasterize(x, y, size, rb, N, fill)
        got = bytes(rb)
        d = sum(1 for a, b in zip(exp, got) if a != b)
        rbad += d
        print(f"  光栅化 x={x:7.3f} y={y:7.3f} size={size:.3f}  差异像素 {d}")
    print(f"光栅化：6 组随机用例，累计差异 {rbad}")

    total = bad + rbad
    print("\nRESULT:", "PASS" if total == 0 else f"FAIL ({total})")
    return 0 if total == 0 else 2


if __name__ == "__main__":
    sys.exit(main())
