#!/usr/bin/env python3
"""posy_trace.py —— 在 unicorn 里把**真采样器写成的那 8 个字节**逐指令录下来。

问题（docs/06 §33 / dispatch/60）：`EffectInfo.pos_y` 装配时是**逐字节拷贝**数组末点
（`0x4729a0 movdqu [rsp+0x78], xmm0` ← `0x472865 movdqu`），所以回放里那个 `…844`
必须是**真机数组末点本身**。那就得问：真采样器里，末点的 `y` 是哪条指令算出来的、
用的是哪个 `x`？

本脚本：跑真采样器（`0x3c2530`），同时
  ① 打开 unicorn 的写内存 hook，只保留落在"返回数组最后两个点"区间里的写，记 `(rip, addr, bits)`；
  ② 记下每一次求值器（`0x4eb9d0`）调用的 `x`（xmm0）—— 只留最后 6 次；
  ③ 打印末点、末点前一点，以及"末点 y 是不是 f(末点 x)"。

用法：
    ../.venv/bin/python tools/posy_trace.py --replay R.json --batch 0
"""

import argparse
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REF = os.path.abspath(os.path.join(HERE, "..", "..", "..", "..", ".reference"))
sys.path.insert(0, os.path.abspath(os.path.join(REF, "analysis-repo", "analysis")))
import struct as _s  # noqa: E402
from unicorn import Uc, UC_HOOK_MEM_WRITE, UC_PROT_ALL  # noqa: E402
from unicorn.x86_const import UC_X86_REG_RIP, UC_X86_REG_XMM0  # noqa: E402
from gwemu import Gw  # noqa: E402

from posy_unicorn import Real, load_batches, bits, STEP  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--replay", required=True)
    ap.add_argument("--batch", type=int, default=0)
    ap.add_argument("--flip", type=int, default=0)
    a = ap.parse_args()
    b = load_batches(a.replay)[a.batch]
    toks = Real().parse(b["corr"])
    m = 0 if b["mode"] == "NormalFunction" else 1

    g = Gw()
    # ---- 第一趟：拿到数组指针与长度
    pts = g.call_path(toks, b["start_x"], b["start_y"], STEP, a.flip, m)
    n = len(pts)
    print("第一趟：%d 点，末点 = (%.17g, %.17g)" % (n, pts[-1][0], pts[-1][1]))

    # 数组指针：call_path 里 out 是 24 字节 {cap,ptr,len}；这里重跑一遍并记录
    hits = []
    evals = []

    def on_write(uc, access, address, size, value, user):
        hits.append((uc.reg_read(UC_X86_REG_RIP), address, size, uc.mem_read(address, size)))

    def h_eval(uc, address):
        v = uc.reg_read(UC_X86_REG_XMM0)
        evals.append(struct.unpack("<d", struct.pack("<Q", v))[0])

    g2 = Gw()
    # 先跑一遍拿 ptr（同参、同分配序列 ⇒ 指针可复现）
    v = g2.make_vec(toks)
    out = g2.alloc(24)
    for off in (0, 8, 16):
        g2.wq(out + off, 0)
    g2.call(0x3C2530, [out, v, a.flip, m], [b["start_x"], b["start_y"], STEP])
    ptr = g2.rq(out + 8)
    ln = g2.rq(out + 16)
    print("数组 ptr=%#x len=%d" % (ptr, ln))

    # ---- 第二趟：同参，打开写 hook（过滤最后 4 个点）
    g3 = Gw()
    g3.trace(0x4EB9D0, h_eval)
    v3 = g3.make_vec(toks)
    out3 = g3.alloc(24)
    for off in (0, 8, 16):
        g3.wq(out3 + off, 0)
    lo = ptr + max(0, (ln - 4)) * 16
    hi = ptr + ln * 16
    evals.clear()

    def filt(uc, access, address, size, value, user):
        if lo <= address < hi + 8:
            on_write(uc, access, address, size, value, user)

    h = g3.uc.hook_add(UC_HOOK_MEM_WRITE, filt)
    g3.call(0x3C2530, [out3, v3, a.flip, m], [b["start_x"], b["start_y"], STEP])
    g3.uc.hook_del(h)
    ptr3, ln3 = g3.rq(out3 + 8), g3.rq(out3 + 16)
    print("第二趟 ptr=%#x len=%d（%s）" % (ptr3, ln3, "同指针" if ptr3 == ptr else "指针变了！"))
    print()
    print("落在最后 4 个点区间 [%#x, %#x) 的写：%d 条" % (lo, hi, len(hits)))
    for rip, addr, size, raw in hits:
        idx = (addr - ptr3) // 16
        off = (addr - ptr3) % 16
        val = ""
        if size == 8:
            val = "%.17g [%s]" % (struct.unpack("<d", raw)[0], struct.unpack("<Q", raw)[0].to_bytes(8, "big").hex())
        elif size == 16:
            x, y = struct.unpack("<dd", raw)
            val = "(%.17g, %.17g)" % (x, y)
        print("  rip=%#x  addr=%#x  pts[%d]+%d  size=%d  %s" % (rip, addr, idx, off, size, val))
    print()
    print("求值器 0x4eb9d0 调用共 %d 次；最后 6 次的 x：" % len(evals))
    for x in evals[-6:]:
        print("   x = %.17g  [%s]" % (x, bits(x)))
    if evals:
        last_x = evals[-1]
        print("   末次求值 x = %.17g ；数组末点 x = %.17g（%s）"
              % (last_x, pts[-1][0], "同" if bits(last_x) == bits(pts[-1][0]) else "不同"))


if __name__ == "__main__":
    main()
