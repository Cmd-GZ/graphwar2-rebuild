#!/usr/bin/env python3
"""从抓包里找两类样本：弹道经过队友、一发杀多人（docs/63）。"""
import json, math, sys

def scan(txt):
    out, i = [], 0
    while True:
        j = txt.find('{"', i)
        if j < 0: break
        d2, ins, k = 0, False, j
        while k < len(txt):
            ch = txt[k]
            if ins:
                if ch == '\\': k += 2; continue
                if ch == '"': ins = False
            else:
                if ch == '"': ins = True
                elif ch == '{': d2 += 1
                elif ch == '}':
                    d2 -= 1
                    if d2 == 0: break
            k += 1
        if d2 != 0: break
        out.append(txt[j:k+1]); i = k + 1
    return out

def load(path):
    from collections import defaultdict
    d = json.load(open(path))
    streams = defaultdict(bytearray); prev = {}
    for c in d:
        k = (c['dir'], c['fd'])
        if prev.get(k) == c['hex']:
            prev[k] = None; continue
        prev[k] = c['hex']; streams[k].extend(bytes.fromhex(c['hex']))
    best = None; bestn = 0
    for k, b in streams.items():
        seq = scan(bytes(b).decode('latin-1'))
        n = len([f for f in seq if f.startswith('{"FunctionPoints"')])
        if n > bestn: best, bestn = seq, n
    return best or []

def analyse(seq):
    team, pos, shots, cur = {}, {}, [], None
    kills_total = 0
    for f in seq:
        kind = f.split('"')[1]
        p = json.loads(f)[kind]
        if kind == 'TeamInfo': team[p['entity_id']] = p['team']
        elif kind == 'PosInfo': pos[p['entity_id']] = (p['pos_x'], p['pos_y'])
        elif kind == 'FunctionPoints':
            cur = {'owner': p['owner_id'], 'x0': p['start_x'], 'y0': p['start_y'],
                   'end_x': p['end_x'], 'kills': [], 'pos': dict(pos)}
            shots.append(cur)
        elif kind == 'LifeQueuedDeath' and cur is not None:
            cur['kills'].append(p['entity_id'])
            kills_total += 1
    return team, shots, kills_total

if __name__ == '__main__':
    seq = load(sys.argv[1] if len(sys.argv) > 1 else '.rea-work/net_raw.json')
    team, shots, total = analyse(seq)
    print(f"队伍 {team}")
    print(f"射击 {len(shots)}，死亡 {total}")
    multi = [s for s in shots if len(s['kills']) >= 2]
    print(f"一发多杀: {len(multi)}")
    for s in multi[:5]:
        print(f"   {s['owner']} 起点({s['x0']:.2f},{s['y0']:.2f}) 杀 {s['kills']}")
    # 队友位置记录（供下一步判定弹道是否经过队友）
    with open('.rea-work/life_cases.json', 'w') as f:
        json.dump({'team': team, 'shots': shots}, f)
    print("已写出 .rea-work/life_cases.json")
