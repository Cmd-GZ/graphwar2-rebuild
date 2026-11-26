#!/usr/bin/env python3
"""T46：生成"期望值换成真值"要用的**真值表**（`fixtures/test_expectation_truth.tsv`）。

三种真值来源，逐行列清楚，不要混为一谈：

  * `unicorn` —— **真二进制**直调（`analysis/gwemu.py` 把 `libgraphwarrust.so` 映射进内存）：
      - `0x3f11e0` 光栅化：`void f(double x, double y, double size, long mask_ptr, ulong mask_len, byte fill)`
      - `0x3f15b0` 单点碰撞：`bool is_obstacle(&self, &[x, y])`（`self+0x18 → ArcInner`，
        锁字在 `+0x10`、`{cap,ptr,len}` 在 `+0x18/+0x20/+0x28`；坐标**走指针** `rsi`）
      - `0x3c2530` 弹道采样器：`Vec<Vec2> calculate_path(&tokens, x0, y0, step, flip(edx), mode(ecx))`
      - `0x4eb9d0` 求值器 / `0x4ed670` 解析器
  * `corpus` —— 冻结语料（`.reference/replays/gw2-replays-snapshot/replays`，403 份，
    逐份 sha256 登记在 `fixtures/guardrails.json` 的 `corpus`）里**真实报文**读出来的值。
  * `frame`  —— 真帧读数（`docs/06` 记录的量测值；本脚本只做转录，重拍命令写在行内）。

用法：
    .reference/venv/bin/python tools/gen_test_expectations.py            # 写 fixtures/test_expectation_truth.tsv
    .reference/venv/bin/python tools/gen_test_expectations.py --check    # 只比对，不写

产物只含"数 + 出处"，不含截图；`fixtures/` 下的既有夹具一个都不改。
"""

import argparse
import gzip
import hashlib
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)                     # gw2-rebuild/
REF = os.path.join(SUB, ".reference")
OUT = os.path.join(SUB, "fixtures", "test_expectation_truth.tsv")
SNAPSHOT = os.path.join(REF, "replays", "gw2-replays-snapshot", "replays")

# ---------------------------------------------------------------- 真二进制
sys.path.insert(0, os.path.join(REF, "analysis-repo", "analysis"))
from gwemu import Gw                                    # noqa: E402
from unicorn.x86_const import UC_X86_REG_RAX, UC_X86_REG_XMM0   # noqa: E402

ADD, CONST, SIN, VARX = 3, 17, 14, 0
MASK_W, MASK_H = 1000, 600
MASK_N = MASK_W * MASK_H


def fnv1a64(b):
    """FNV-1a 64：测试里也要用同一个指纹（Rust 侧不必引 sha256 依赖）。"""
    h = 0xCBF29CE484222325
    for x in b:
        h ^= x
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def bits(x):
    return "%016x" % struct.unpack("<Q", struct.pack("<d", x))[0]


class Real:
    """真二进制 oracle。"""

    def __init__(self):
        self.g = Gw()

    # ---- 0x3f11e0：光栅化 -------------------------------------------------
    def rasterize(self, x, y, size, fill):
        g = self.g
        ptr = g.alloc(MASK_N)
        g.wr(ptr, b"\xff" * MASK_N)
        try:
            g.call(0x3f11e0, [ptr, MASK_N, fill], [x, y, size])
        except Exception:
            # 返回 void，模拟器把 RAX 当指针读；结果已经在内存里
            pass
        return bytes(g.rd(ptr, MASK_N)), ptr

    # ---- 0x3f15b0：单点碰撞 ----------------------------------------------
    def is_obstacle(self, mask_ptr, x, y):
        g = self.g
        a = g.alloc(0x40)
        g.wq(a + 0x00, 1)
        g.wq(a + 0x08, 1)
        g.wr(a + 0x10, b"\x00" * 8)          # 锁字 + 中毒标志
        g.wq(a + 0x18, MASK_N)               # Mrc<Mutex<Vec<u8>>> 的 cap
        g.wq(a + 0x20, mask_ptr)             # ptr
        g.wq(a + 0x28, MASK_N)               # len
        mgr = g.alloc(0x40)
        g.wq(mgr + 0x18, a)                  # manager +0x18 = ArcInner
        cb = g.alloc(16)
        g.wr(cb, struct.pack("<dd", x, y))   # rsi 指向 {x, y}
        g.call(0x3f15b0, [mgr, cb])
        return g.uc.reg_read(UC_X86_REG_RAX) & 0xFF

    # ---- 0x3c2530：弹道采样 ----------------------------------------------
    def path(self, toks, x0, y0, step, flip=0, mode=0):
        return self.g.call_path(toks, x0, y0, step, flip, mode)

    # ---- 0x4ed670 / 0x4eb9d0 ---------------------------------------------
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
        for k in (0, 8, 16):
            g.wq(out + k, 0)
        g.call(0x4ED670, [out, st])
        ptr, ln = g.rq(out + 8), g.rq(out + 16)
        toks = []
        for i in range(ln):
            tag = struct.unpack("<Q", g.rd(ptr + i * 16, 8))[0]
            val = struct.unpack("<d", g.rd(ptr + i * 16 + 8, 8))[0]
            toks.append((tag, val))
        return toks

    def eval(self, toks, x, y=0.0, dy=0.0):
        g = self.g
        v = g.make_vec(toks)
        idx = g.alloc(8)
        g.wq(idx, 0)
        g.call(0x4EB9D0, [v, idx, None, None], [x, y, dy])
        raw = g.uc.reg_read(UC_X86_REG_XMM0) & 0xFFFFFFFFFFFFFFFF
        return struct.unpack("<d", struct.pack("<Q", raw))[0]


# ---------------------------------------------------------------- 冻结语料
def load_replay(name):
    p = os.path.join(SNAPSHOT, name)
    raw = open(p, "rb").read()
    opener = gzip.open if raw[:2] == b"\x1f\x8b" else open
    evs = [json.loads(l) for l in opener(p, "rt", encoding="utf-8", errors="replace") if l.strip()]
    return evs, hashlib.sha256(raw).hexdigest()


def replay_sha(name):
    return hashlib.sha256(open(os.path.join(SNAPSHOT, name), "rb").read()).hexdigest()


# ---------------------------------------------------------------- 真值计算
def trunc(v):
    """C 的 `(int)double`：向零截断。"""
    return int(v) if v >= 0 else -int(-v)


def mask_row(R, x, y, size, fill):
    """一张掩膜的完整真值：整张 sha256 + 圆心的 43×43 邻域 sha256 + 非空计数。

    邻域以**该圆自己的像素圆心**为中心（`px = trunc(x*1000/50+500)`，
    与 `0x3f11e0` 开头那三次换算同式），不是固定的 (500,300)。
    """
    m, _ = R.rasterize(x, y, size, fill)
    cx = trunc(x * 1000.0 / 50.0 + 500.0)
    cy = trunc(y * 1000.0 / 50.0 + 300.0)
    region = bytes(m[(cy + dy) * MASK_W + (cx + dx)]
                   for dy in range(-21, 22) for dx in range(-21, 22)
                   if 0 <= cy + dy < MASK_H and 0 <= cx + dx < MASK_W)
    return {
        "mask_sha256": hashlib.sha256(m).hexdigest(),
        "mask_fnv": "%016x" % fnv1a64(m),
        "region43_fnv": "%016x" % fnv1a64(region),
        "region43_sha256": hashlib.sha256(region).hexdigest(),
        "nonempty": sum(1 for b in m if b != 0xFF),
        "px_center": m[300 * MASK_W + 500],
        "px_m25": m[300 * MASK_W + 475],
        "px_p25": m[300 * MASK_W + 525],
    }


def trajectory_row(R, toks, x0, y0, step, flip=0, mode=0):
    pts = R.path(toks, x0, y0, step, flip, mode)
    blob = b"".join(struct.pack("<dd", px, py) for (px, py) in pts)
    return {
        "n": len(pts),
        "last_x": bits(pts[-1][0]),
        "last_y": bits(pts[-1][1]),
        "pts_sha256": hashlib.sha256(blob).hexdigest(),
        "pts": pts,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()

    R = Real()
    rows = []

    def add(section, key, value, source, note):
        rows.append((section, key, value, source, note))

    # ---- A：掩膜（0x3f11e0 / 0x3f15b0）-----------------------------------
    plane_cases = [
        ("A01", "raster(0,0,1.0,0x00)"),
        ("A04", "raster(0,0,1.0,0x00)"),
        ("A05", "raster(0,0,1.0,0x10)"),
        ("A06", "raster(24.5,14.0,1.0,0x00)"),
        ("A02", "raster(-7.5,3.25,0.6,0x00)"),
        ("A03", "raster(10.0,-5.0,1.0,0x00)"),
    ]
    args = {
        "A01": (0.0, 0.0, 1.0, 0x00),
        "A02": (-7.5, 3.25, 0.6, 0x00),
        "A03": (10.0, -5.0, 1.0, 0x00),
        "A04": (0.0, 0.0, 1.0, 0x00),
        "A05": (0.0, 0.0, 1.0, 0x10),
        "A06": (24.5, 14.0, 1.0, 0x00),
    }
    for cid, label in plane_cases:
        x, y, size, fill = args[cid]
        t = mask_row(R, x, y, size, fill)
        add("plane/" + cid, "argv", "%r,%r,%r,0x%02x" % (x, y, size, fill),
            "unicorn 0x3f11e0", label)
        add("plane/" + cid, "mask_sha256", t["mask_sha256"], "unicorn 0x3f11e0", label)
        add("plane/" + cid, "region43_sha256", t["region43_sha256"], "unicorn 0x3f11e0", label)
        add("plane/" + cid, "nonempty_px", str(t["nonempty"]), "unicorn 0x3f11e0", label)
        add("plane/" + cid, "mask_fnv1a64", t["mask_fnv"], "unicorn 0x3f11e0", label)
        add("plane/" + cid, "region43_fnv1a64", t["region43_fnv"], "unicorn 0x3f11e0", label)

        # 0x3f15b0 在**真掩膜**上的判决
        m, ptr = R.rasterize(x, y, size, fill)
        probes = [(0.0, 0.0), (24.0, 14.0), (x, y), (-0.05, 0.0), (0.05, 0.0), (24.99, 14.99)]
        if cid == "A01":
            for (px, py) in probes:
                add("plane/A03", "is_obstacle(%r,%r)" % (px, py),
                    str(R.is_obstacle(ptr, px, py)), "unicorn 0x3f15b0",
                    "掩膜 = raster(0,0,1.0,0x00) 的真输出")

    # ---- B：弹道（0x3c2530）+ 命中 --------------------------------------
    # 与 src/graphwar_shot_calculator.rs::shot_points("sin(x)", 5, 2, 0.01, false, Normal, true) 同输入
    c = 2.0 - R.eval(R.parse("sin(x)"), 5.0)
    toks = [(ADD, 0.0), (CONST, c), (SIN, 0.0), (VARX, 0.0)]
    t = trajectory_row(R, toks, 5.0, 2.0, 0.01)
    add("shot/B01", "c_bits", bits(c), "unicorn 0x4eb9d0", "c = y0 - f(x0)，x0=5 y0=2")
    add("shot/B01", "n_points", str(t["n"]), "unicorn 0x3c2530", "sin(x)+c 从 (5,2) step=0.01")
    add("shot/B01", "pts_sha256", t["pts_sha256"], "unicorn 0x3c2530", "整条点数组逐位")
    add("shot/B01", "last_x", t["last_x"], "unicorn 0x3c2530", "")
    add("shot/B01", "last_y", t["last_y"], "unicorn 0x3c2530", "")
    for i in (0, 1, 2, 100, 300, t["n"] - 1):
        add("shot/B01", "pt[%d]" % i,
            "%s,%s" % (bits(t["pts"][i][0]), bits(t["pts"][i][1])), "unicorn 0x3c2530", "")

    # B02：曲线上放障碍 —— 真掩膜 + 真单点碰撞 + 真点数组 ⇒ 首个命中点
    ob_y = R.eval(R.parse("sin(x)"), 8.0) + c
    mask, ptr = R.rasterize(8.0, ob_y, 0.3, 0x00)
    hit = None
    for i, (px, py) in enumerate(t["pts"]):
        if R.is_obstacle(ptr, px, py):
            hit = i
            break
    add("shot/B02", "obstacle_y", bits(ob_y), "unicorn 0x4eb9d0", "ob.y = sin(8)+c")
    add("shot/B02", "hit_index", str(hit), "unicorn 0x3f11e0+0x3f15b0",
        "真采样器点数组上逐点跑真单点碰撞（**组合方式是我方推断**）")
    if hit is not None:
        add("shot/B02", "hit_pt", "%s,%s" % (bits(t["pts"][hit][0]), bits(t["pts"][hit][1])),
            "unicorn 0x3c2530", "命中点坐标（逐位）")
    # B03：障碍远离曲线
    mask3, ptr3 = R.rasterize(8.0, -12.0, 0.3, 0x00)
    hit3 = None
    for i, (px, py) in enumerate(t["pts"]):
        if R.is_obstacle(ptr3, px, py):
            hit3 = i
            break
    add("shot/B03", "hit_index", str(hit3), "unicorn 0x3f11e0+0x3f15b0", "障碍在 (8,-12)")

    # ---- H：半步复检的最小用例 sin(60*x) --------------------------------
    toks60 = R.parse("sin(60*x)")
    t60 = trajectory_row(R, toks60, 0.0, 0.0, 0.01)
    add("half/H01", "n_points", str(t60["n"]), "unicorn 0x3c2530", "sin(60*x) (0,0) step=0.01")
    add("half/H01", "pts_sha256", t60["pts_sha256"], "unicorn 0x3c2530", "")
    add("half/H01", "last_x", t60["last_x"], "unicorn 0x3c2530", "")
    add("half/H01", "last_y", t60["last_y"], "unicorn 0x3c2530", "")
    for i in (0, 1, 2, 50, 100, t60["n"] - 1):
        add("half/H01", "pt[%d]" % i,
            "%s,%s" % (bits(t60["pts"][i][0]), bits(t60["pts"][i][1])), "unicorn 0x3c2530", "")

    # ---- C/D/F：冻结语料真值（真实报文 / 轮转序列）-------------------------
    # 三个回放都在冻结快照里；sha256 同时登记在 fixtures/guardrails.json 的 corpus。
    def replay_batch_window(name, idx):
        evs, sha = load_replay(name)
        # 切批：GameStateInfo CalculatingResults … GameStateInfo ExecutingResults
        wins, i = [], 0
        while i < len(evs):
            ev = evs[i].get("event", {})
            if "GameStateInfo" in ev and ev["GameStateInfo"]["game_state"] == "CalculatingResults":
                j = i
                while j < len(evs):
                    e2 = evs[j].get("event", {})
                    if "GameStateInfo" in e2 and e2["GameStateInfo"]["game_state"] == "ExecutingResults":
                        wins.append((i, j))
                        i = j
                        break
                    j += 1
            i += 1
        s, e = wins[idx]
        lines = []
        for k in range(s, e + 1):
            lines.append(json.dumps(evs[k]["event"], ensure_ascii=False, separators=(",", ":")))
        return sha, evs[s].get("tick"), lines

    for name, idx, tag in (("1782642582", 0, "C01"), ("1782642582", 1, "C02"),
                           ("1782640359", 0, "C03")):
        sha, tick, lines = replay_batch_window(name, idx)
        add("corpus/" + tag, "replay", name, "corpus", "冻结快照 .reference/replays/gw2-replays-snapshot/replays/")
        add("corpus/" + tag, "sha256", sha, "corpus", "与 guardrails.json 的 corpus 同一份")
        add("corpus/" + tag, "batch_index", str(idx), "corpus", "")
        add("corpus/" + tag, "batch_start_tick", str(tick), "corpus", "")
        add("corpus/" + tag, "wire_lines", str(len(lines)), "corpus", "逐条抄进 src/_scaffold/diffconv.rs 的 REAL_* 常量")
        for ln in lines:
            if "FunctionPoints" in ln:
                d = json.loads(ln)["FunctionPoints"]
                add("corpus/" + tag, "end_x", bits(d["end_x"]), "corpus",
                    "owner=%d start=(%.17g, %.17g)" % (d["owner_id"], d["start_x"], d["start_y"]))
            if "FunctionInfo" in ln and json.loads(ln)["FunctionInfo"].get("corrected_function"):
                add("corpus/" + tag, "corrected", json.loads(ln)["FunctionInfo"]["corrected_function"], "corpus", "")

    # 回合轮转：SoldierInfo 首见顺序 + TurnInfo 序列
    for name, tag, player in (("1782764906", "F01a", 5272), ("1782673653", "F01b", 401164),
                              ("1782673653", "F02", 401060)):
        evs, sha = load_replay(name)
        roster, turns = [], []
        for e in evs:
            ev = e.get("event", {})
            if "SoldierInfo" in ev:
                si = ev["SoldierInfo"]
                if si["player_id"] == player and si["soldier_id"] not in roster:
                    roster.append(si["soldier_id"])
            elif "TurnInfo" in ev:
                ti = ev["TurnInfo"]
                if ti["player_id"] == player:
                    turns.append((ti["soldier_id"], e.get("tick")))
        add("corpus/" + tag, "replay", name, "corpus", "player=%d" % player)
        add("corpus/" + tag, "sha256", sha, "corpus", "")
        add("corpus/" + tag, "roster", ",".join(str(x) for x in roster), "corpus",
            "SoldierInfo 首见顺序")
        add("corpus/" + tag, "turninfo", ";".join("%d@%d" % (s, t) for s, t in turns), "corpus",
            "TurnInfo.soldier_id@tick")

    # ---- D：真实抓包（死亡时序）------------------------------------------
    cap = open(os.path.join(SUB, "fixtures", "protocol_death.jsonl"), "rb").read()
    add("capture/D01", "file", "fixtures/protocol_death.jsonl", "capture", "真实抓包")
    add("capture/D01", "sha256", hashlib.sha256(cap).hexdigest(), "capture", "")
    for ln in cap.decode().splitlines():
        if "LifeQueuedDeath" in ln:
            add("capture/D01", "queued_death", ln.strip(), "capture", "")

    # ---- E：真帧读数（转录，重拍见行内命令）-----------------------------
    add("frame/E01", "sprite_screen_px", "54.6", "frame",
        "docs/06 §59.8 第 1 条（三层 sprite 各 54.6 屏幕 px）；"
        "重拍：tools/frame_capture.py --side original --flow replay（私有 Xvfb 1920x1160x24）")
    add("frame/E01", "canvas_px", "42.67", "frame",
        "docs/06 §86：scale=(±0.083333, −0.083333)、512×512 贴图 ⇒ 42.67 画布 px")
    add("frame/E01", "board_px", "1273", "frame", "docs/06 §59.1 白底矩形 x 303–1575")

    with open(OUT, "w", encoding="utf-8") as f:
        f.write("section\tkey\tvalue\tsource\tnote\n")
        for r in rows:
            f.write("\t".join(r) + "\n")
    print("wrote", OUT, len(rows), "rows")
    return 0


if __name__ == "__main__":
    sys.exit(main())
