#!/usr/bin/env python3
"""把真实障碍坐标反推成随机值，在抓到的随机流里查找（docs/54）。"""
import json
import os
import sys

# 数据由 node tools/capture_rand_values.js 写到工作区根目录的 .rea-work/ 下
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, '..', '.rea-work', 'rand_values.json')
if not os.path.exists(PATH):
    sys.exit(f"缺少 {PATH}：先跑 node tools/capture_rand_values.js")
d = json.load(open(PATH))
draws = [int(x['v'], 16) for x in d['draws']]
vals = [(draws[k] >> 11) * 2.0 ** -53 for k in range(len(draws))]

hits = 0
for o in d['obstacles']:
    p = json.loads(o['msg'])['AddObstacle']
    x, y, r = p['pos_x'], p['pos_y'], p['radius']
    u = x / (2 * r + 25) / 2 + 0.5
    v = y / (2 * r + 15) / 2 + 0.5
    iu = next((k for k, val in enumerate(vals) if abs(val - u) < 2e-6), None)
    iv = next((k for k, val in enumerate(vals) if abs(val - v) < 2e-6), None)
    ok = iu is not None and iv is not None and iv - iu == 1
    hits += ok
    print(f"x={x:9.3f} y={y:9.3f} r={r:7.4f}  u@{iu} v@{iv}  {'✓' if ok else '✗'}")
print(f"\n命中 {hits}/{len(d['obstacles'])}")
