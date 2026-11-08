#!/usr/bin/env python3
"""posy_unicorn.py —— 用**真二进制**（unicorn 直调 `.so`）复算回放的弹道，回答
"回放里那个 `EffectInfo.pos_y` 到底能不能由采样器产出来"。

背景（docs/06 §33 / dispatch/60）：顺序回合那发 `1794158844` 批#0 的 `pos_y`
我方与真二进制都算 `-14.998302430429845`（末位 `6d50`），回放记 `…844`（`6d4f`）。

本脚本做三件事：
  ① **复现**：把回放自己的 `corrected_function` 文本喂给**真解析器**（`0x4ed670`）得到
     token 流，再把 token 流 + `start_x/start_y` + `step` + `flip` + `mode` 喂给**真采样器**
     （`0x3c2530`），拿回整条点数组 —— 与回放的 `FunctionPoints.end_x` / `EffectInfo.pos_*` 比**位**。
  ② **变异**：对 `x0/y0` 做 ±1..3 ulp 的扫动（§27.5 的老线索），看有没有一档能同时
     命中回放的 `end_x`（不动）与 `pos_y`（动 1 ulp）。
  ③ **回代**：用真求值器（`0x4eb9d0`）在末点附近找使 `f(x) == 回放 pos_y` 的 `x`，
     看它在不在 `x0 + k·step` 的格点上 —— 判断"回放那个 y 是不是在另一个 x 上求出来的"。

用法（二进制 sha256 `9a538115…`，见 `docs/06` §35）：
    ../.venv/bin/python tools/posy_unicorn.py --replay R.json --batch 0
    ../.venv/bin/python tools/posy_unicorn.py --replay R.json --batch 0 --variants
    ../.venv/bin/python tools/posy_unicorn.py --replay R.json --batch 0 --solve

注意：`--replay` 要**解压后**的 JSONL（`gunzip -c <回放> > R.json`）。
"""

import argparse
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REF = os.path.join(HERE, "..", "..", "..", "..", ".reference")
ANALYSIS = os.path.abspath(os.path.join(REF, "analysis-repo", "analysis"))
sys.path.insert(0, ANALYSIS)
from gwemu import Gw  # noqa: E402
from unicorn.x86_const import UC_X86_REG_XMM0  # noqa: E402

STEP = 0.01          # §27.5 / fixtures/frida_token_at_fire_sample.jsonl：step 恒 0.01
STEP_BITS = "7b14ae47e17a843f"


def bits(x):
    return struct.pack("<d", x)[::-1].hex()


def ulp_up(x, k=1):
    for _ in range(k):
        x = struct.unpack("<d", struct.pack("<Q", struct.unpack("<Q", struct.pack("<d", x))[0] + 1))[0]
    return x


def ulp_dn(x, k=1):
    for _ in range(k):
        x = struct.unpack("<d", struct.pack("<Q", struct.unpack("<Q", struct.pack("<d", x))[0] - 1))[0]
    return x


# ---------------------------------------------------------------- 真二进制调用
class Real:
    def __init__(self):
        self.g = Gw()

    def parse(self, text):
        """真解析器 0x4ed670：文本 -> token 列表（16 B/token：tag u64 + f64）"""
        g = self.g
        b = text.encode()
        mem = g.alloc(len(b) + 1)
        g.wr(mem, b)
        st = g.alloc(24)
        g.wq(st, len(b)); g.wq(st + 8, mem); g.wq(st + 16, len(b))
        out = g.alloc(24)
        g.wq(out, 0); g.wq(out + 8, 0); g.wq(out + 16, 0)
        g.call(0x4ED670, [out, st])
        ptr, ln = g.rq(out + 8), g.rq(out + 16)
        toks = []
        for i in range(ln):
            tag = struct.unpack("<Q", g.rd(ptr + i * 16, 8))[0]
            val = struct.unpack("<d", g.rd(ptr + i * 16 + 8, 8))[0]
            toks.append((tag, val))
        return toks

    def path(self, toks, x0, y0, step, flip=0, mode=0):
        """真采样器 0x3c2530：edx=flip、ecx=mode"""
        return self.g.call_path(toks, x0, y0, step, flip, mode)

    def eval(self, toks, x, y=0.0, dy=0.0):
        # 不用 gwemu.call_eval：它在 xmm0 最高位为 1（负数/NaN）时会 struct.pack('<Q') 崩
        g = self.g
        v = g.make_vec(toks)
        idx = g.alloc(8)
        g.wq(idx, 0)
        g.call(0x4EB9D0, [v, idx, None, None], [x, y, dy])
        raw = g.uc.reg_read(UC_X86_REG_XMM0) & 0xFFFFFFFFFFFFFFFF
        return struct.unpack("<d", struct.pack("<Q", raw))[0]


# ---------------------------------------------------------------- 回放读取
def load_batches(path):
    """把 JSONL 切成"发"：每个 FunctionPoints 收尾一批。返回 [{...}]"""
    fn, corr, mode = {}, {}, "NormalFunction"
    out = []
    cur = None
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            e = json.loads(line)["event"]
            if "FunctionModeInfo" in e:
                mode = e["FunctionModeInfo"]["function_mode"]
            elif "FunctionInfo" in e:
                fi = e["FunctionInfo"]
                if fi.get("corrected_function"):
                    cur = {"owner": fi["owner_id"], "func": fi["function"],
                           "corr": fi["corrected_function"], "mode": mode,
                           "points": [], "effects": []}
            elif "FunctionPoints" in e:
                fp = e["FunctionPoints"]
                if cur is None:
                    cur = {"owner": fp["owner_id"], "func": None, "corr": None,
                           "mode": mode, "points": [], "effects": []}
                cur.update(start_x=fp["start_x"], start_y=fp["start_y"], end_x=fp["end_x"])
                out.append(cur)
                cur = None
            elif "EffectInfo" in e and cur is not None:
                cur["effects"].append(e["EffectInfo"])
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--replay", required=True)
    ap.add_argument("--batch", type=int, default=0)
    ap.add_argument("--flip", type=int, default=0)
    ap.add_argument("--step", default=STEP_BITS)
    ap.add_argument("--variants", action="store_true")
    ap.add_argument("--solve", action="store_true")
    a = ap.parse_args()
    step = struct.unpack("<d", bytes.fromhex(a.step)[::-1])[0] if a.step != STEP_BITS else STEP
    mode = 1 if False else None

    B = load_batches(a.replay)
    b = B[a.batch]
    print("批 #%d  owner=%s  mode=%s" % (a.batch, b.get("owner"), b["mode"]))
    print("  function          = %s" % b["func"])
    print("  corrected_function= %s" % b["corr"])
    print("  start = (%.17g, %.17g)  [%s, %s]" % (b["start_x"], b["start_y"], bits(b["start_x"]), bits(b["start_y"])))
    print("  end_x = %.17g  [%s]" % (b["end_x"], bits(b["end_x"])))
    wire_expl = [e for e in b["effects"] if e["effect_type"] == "ExplosionEffect"]
    for e in wire_expl:
        print("  回放 EffectInfo(Explosion) pos = (%.17g, %.17g)  [%s, %s]"
              % (e["pos_x"], e["pos_y"], bits(e["pos_x"]), bits(e["pos_y"])))
    print("  其它 effect: %s" % [(e["effect_type"], round(e["pos_x"], 4), round(e["pos_y"], 4)) for e in b["effects"]])

    R = Real()
    toks = R.parse(b["corr"])
    m = 0 if b["mode"] == "NormalFunction" else 1
    pts = R.path(toks, b["start_x"], b["start_y"], step, a.flip, m)
    print()
    print("真二进制：%d 点（mode=%d flip=%d step=%.17g）" % (len(pts), m, a.flip, step))
    for k in range(max(0, len(pts) - 4), len(pts)):
        print("   pts[%d] = (%.17g, %.17g)  [%s, %s]" % (k, pts[k][0], pts[k][1], bits(pts[k][0]), bits(pts[k][1])))
    print("   末点 x vs 回放 end_x : %s" % ("逐位相同" if bits(pts[-1][0]) == bits(b["end_x"]) else "不同"))
    if wire_expl:
        w = wire_expl[0]
        print("   末点 x vs 回放 pos_x : %s" % ("逐位相同" if bits(pts[-1][0]) == bits(w["pos_x"]) else "不同"))
        print("   末点 y vs 回放 pos_y : %s" % ("逐位相同" if bits(pts[-1][1]) == bits(w["pos_y"]) else
                                          "差 %d ulp" % _ulp_diff(pts[-1][1], w["pos_y"])))
    # 末点是不是 f(x)（真求值器）
    y_at = R.eval(toks, pts[-1][0], 0.0, 0.0)
    print("   f(末点 x)（真求值器，y=dy=0）= %.17g  [%s]  %s"
          % (y_at, bits(y_at), "== 末点 y" if bits(y_at) == bits(pts[-1][1]) else "!= 末点 y"))

    if a.variants:
        print()
        print("变异扫动（x0/y0 各 ±3 ulp；判据：end_x 逐位不动 且 pos_y 变成回放那个值）")
        if not wire_expl:
            print("  回放里没有 ExplosionEffect，跳过")
            return
        w = wire_expl[0]
        hit = 0
        for which in ("x0", "y0"):
            base = b["start_x"] if which == "x0" else b["start_y"]
            for k in (-3, -2, -1, 1, 2, 3):
                v = ulp_up(base, -k) if k < 0 else ulp_up(base, k)
                x0 = v if which == "x0" else b["start_x"]
                y0 = v if which == "y0" else b["start_y"]
                p = R.path(toks, x0, y0, step, a.flip, m)
                same_n = len(p) == len(pts)
                lastx_ok = bits(p[-1][0]) == bits(b["end_x"])
                ystat = "逐位相同" if bits(p[-1][1]) == bits(w["pos_y"]) else "差 %d" % _ulp_diff(p[-1][1], w["pos_y"])
                flag = ""
                if bits(p[-1][1]) == bits(w["pos_y"]):
                    hit += 1
                    flag = " ★pos_y 命中"
                print("   %s %+d ulp: n=%d(%s) end_x=%s pos_y→%s%s"
                      % (which, k, len(p), "同" if same_n else "异", "同" if lastx_ok else "异", ystat, flag))
        print("   命中 %d 档" % hit)

    if a.solve:
        print()
        print("回代：找 x 使 f(x) 逐位等于回放的 pos_y（真求值器）")
        if not wire_expl:
            print("  回放里没有 ExplosionEffect，跳过")
            return
        w = wire_expl[0]
        x_last = pts[-1][0]
        found = []
        for k in range(-4000, 4001):
            x = ulp_up(x_last, k) if k >= 0 else ulp_up(x_last, k)
            if R.eval(toks, x, 0.0, 0.0) == w["pos_y"]:
                found.append((k, x))
        print("   末点 x 附近 ±4000 ulp 内命中 %d 个 x" % len(found))
        for k, x in found[:8]:
            print("     x = 末点 x %+d ulp = %.17g  [%s]" % (k, x, bits(x)))
        # 这个 x 在不在格点上
        for k, x in found[:8]:
            t = (x - b["start_x"]) / step
            print("     k_grid = (x - x0)/step = %.6f  最近整数 = %d  (差 %.3e)"
                  % (t, round(t), abs(t - round(t))))


def _ulp_diff(a, b):
    ia = struct.unpack("<q", struct.pack("<d", a))[0]
    ib = struct.unpack("<q", struct.pack("<d", b))[0]
    return ia - ib


def is_finite(x):
    return x == x and abs(x) != float("inf")


if __name__ == "__main__":
    main()
