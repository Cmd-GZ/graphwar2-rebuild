#!/usr/bin/env python3
"""仅凭抓到的 ChaCha 随机流 + 二进制里的 ziggurat 表，完整预测障碍列表（docs/56）。"""
import json, math, os, struct, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SO = os.path.join(ROOT, '..', 'libgraphwarrust.so')
DATA = os.path.join(ROOT, '..', '.rea-work', 'rand_values.json')
if not os.path.exists(DATA):
    sys.exit(f"缺少 {DATA}：先跑 node tools/capture_rand_values.js")

data = open(SO, 'rb').read()
X = list(struct.unpack('<257d', data[0x25b428:0x25b428 + 257 * 8]))       # ZIG_NORM_X
F = [0.0] + list(struct.unpack('<257d', data[0x25ac20:0x25ac20 + 257 * 8]))  # ZIG_NORM_F

d = json.load(open(DATA))
draws = [int(x['v'], 16) for x in d['draws']]
real = [json.loads(o['msg'])['AddObstacle'] for o in d['obstacles']]

pos = 3  # 关卡生成从第 3 个随机数开始（前 3 个被别处消费）
def nxt():
    global pos
    v = draws[pos]; pos += 1
    return v
def bits(b):
    return struct.unpack('<d', struct.pack('<Q', b))[0]

def normal():
    while True:
        u = nxt(); i = u & 0xff
        x = X[i] * (bits((u >> 12) | 0x4000000000000000) - 3.0)
        if abs(x) < X[i + 1]:
            return x
        if i == 0:
            while True:
                a, b = nxt(), nxt()
                la = math.log(bits((a >> 12) | 0x3FF0000000000000) - 1.0 + 1e-300)
                lb = math.log(bits((b >> 12) | 0x3FF0000000000000) - 1.0 + 1e-300)
                if lb * -2.0 >= (la / 3.654152885361009) ** 2:
                    return -la / 3.654152885361009
        u2 = nxt()
        if F[i + 2] + (u2 >> 11) * 2.0 ** -53 * (F[i + 1] - F[i + 2]) < math.exp(-0.5 * x * x):
            return x

n = int(normal() * 7.0 + 15.0)
print(f"预测个数 {n}，真实 {len(real)}")
bad = 0
for i in range(min(n, len(real))):
    r = max(normal() * 1.6 + 2.6, 0.8)
    u = (nxt() >> 11) * 2.0 ** -53
    x = (2 * r + 25.0) * (2 * u - 1.0)
    v = (nxt() >> 11) * 2.0 ** -53
    y = (2 * r + 15.0) * (2 * v - 1.0)
    o = real[i]
    ok = abs(o['pos_x'] - x) < 1e-9 and abs(o['pos_y'] - y) < 1e-9 and abs(o['radius'] - r) < 1e-9
    if not ok:
        bad += 1
    print(f"  #{i:2d} 预测 ({x:9.3f},{y:9.3f}) r={r:6.3f} | "
          f"真实 ({o['pos_x']:9.3f},{o['pos_y']:9.3f}) r={o['radius']:6.3f}  {'✓' if ok else '✗'}")
print(f"\n完全一致 {min(n,len(real)) - bad}/{min(n,len(real))}")
