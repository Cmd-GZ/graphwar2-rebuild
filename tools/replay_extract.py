#!/usr/bin/env python3
"""从抓包里按**回合**提取"开火 + 当时的函数原文 + 结果"。

关键点：`FunctionInfo` 会随回合更新，所以必须记录**时间线**——
每次开火时取该实体当时最新的函数，而不是整局最后一条。
"""
import json
import sys


def scan(txt):
    out, i = [], 0
    while True:
        j = txt.find('{"', i)
        if j < 0:
            break
        depth, ins, k = 0, False, j
        while k < len(txt):
            ch = txt[k]
            if ins:
                if ch == '\\':
                    k += 2
                    continue
                if ch == '"':
                    ins = False
            else:
                if ch == '"':
                    ins = True
                elif ch == '{':
                    depth += 1
                elif ch == '}':
                    depth -= 1
                    if depth == 0:
                        break
            k += 1
        if depth != 0:
            break
        out.append(txt[j:k + 1])
        i = k + 1
    return out


def main(path):
    from collections import defaultdict
    d = json.load(open(path))
    streams = defaultdict(bytearray)
    prev = {}
    for c in d:
        k = (c['dir'], c['fd'])
        if prev.get(k) == c['hex']:
            prev[k] = None
            continue
        prev[k] = c['hex']
        streams[k].extend(bytes.fromhex(c['hex']))

    best = None
    for k, b in streams.items():
        seq = scan(bytes(b).decode('latin-1'))
        shots = [f for f in seq if f.startswith('{"FunctionPoints"')]
        if shots and (best is None or len(shots) > len(best[1])):
            best = (k, seq)
    if not best:
        print("NO_SHOTS")
        return 1
    seq = best[1]

    obstacles, positions, others = [], {}, {}
    funcs = {}          # owner -> 当前函数原文（随时间更新）
    shots = []
    cur = None
    for f in seq:
        kind = f.split('"')[1]
        p = json.loads(f)[kind]
        if kind == 'AddObstacle':
            obstacles.append((p['pos_x'], p['pos_y'], p['radius']))
        elif kind == 'PosInfo':
            positions[p['entity_id']] = (p['pos_x'], p['pos_y'])
        elif kind == 'FunctionInfo':
            if p['function']:
                funcs[p['owner_id']] = p['function']       # 只记**原始**函数
        elif kind == 'FunctionPoints':
            cur = {
                'owner': p['owner_id'],
                'start_x': p['start_x'], 'start_y': p['start_y'],
                'end_x': p['end_x'],
                'function': funcs.get(p['owner_id'], '0.00'),
                'kills': [],
            }
            shots.append(cur)
        elif kind == 'LifeQueuedDeath' and cur is not None:
            cur['kills'].append(p['entity_id'])
    print(json.dumps({'obstacles': obstacles, 'positions': positions, 'shots': shots}))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else '.rea-work/net_raw.json'))
