#!/usr/bin/env python3
"""比较 capture_hashmap_order.js 产出的报文顺序（docs/113）。

用法：
    python3 tools/compare_message_order.py run/p1.json run/p2.json [...]
    python3 tools/compare_message_order.py run/*.json --brief

每个文件先按"一次进程跑了两局"切成两个 match（以 `GameNameRequest` 为界、
以 `NewConnection` 所在 fd 作为服务端方向），再把批内顺序**归一化到创建序**
（创建序 = `PlayerInfo`/`SoldierInfo` 首次公告的顺序，跨运行稳定），这样
实体 id 不同的两局也能直接比"第几个"。

输出三块：
  A. 每个 match 的"开局全量重发批"里各类消息的顺序（创建序下标）
  B. 每个 match 的 TurnInfo 序列
  C. 多文件时：按消息类型逐一比较（相同 / 不同 / 首个差异位置）

只用标准库。
"""
import json
import sys
import collections

WANT = ('ClearObstacles', 'FunctionUpdateRequest')   # 批的起止标记
ID_KEYS = ('soldier_id', 'entity_id', 'owner_id', 'player_id', 'connection_id')
SKIP_ORDER = ('AddObstacle', 'ClearObstacles')       # 向量发射，不是遍历


def load(path):
    d = json.load(open(path))
    out = []
    for f in d.get('frames', []):
        try:
            m = json.loads(f['msg'])
        except Exception:
            continue
        if not isinstance(m, dict) or len(m) != 1:
            continue
        (t, body), = m.items()
        out.append({'t': t, 'b': body, 'fd': f['fd'], 'tid': f['tid']})
    return out, d


def split_matches(frames):
    """一局 = 一条 TCP 连接：以 GameNameRequest 起，服务端 fd 由 NewConnection 决定。"""
    starts = [i for i, f in enumerate(frames) if f['t'] == 'GameNameRequest']
    out = []
    for k, a in enumerate(starts):
        b = starts[k + 1] if k + 1 < len(starts) else len(frames)
        seg = frames[a:b]
        fd = next((f['fd'] for f in seg if f['t'] == 'NewConnection'), None)
        out.append({'frames': seg, 'server_fd': fd, 'tid': next((f['tid'] for f in seg if f['t'] == 'NewConnection'), None)})
    return out


def creation_index(m):
    idx, players, soldiers = {}, [], []
    for f in m['frames']:
        if f['t'] == 'PlayerInfo':
            pid = f['b'].get('player_id')
            if pid not in idx:
                idx[pid] = ('p', len(players)); players.append(pid)
        elif f['t'] == 'SoldierInfo':
            sid = f['b'].get('soldier_id')
            if sid not in idx:
                idx[sid] = ('s', len(soldiers)); soldiers.append(sid)
    return idx, players, soldiers


def start_block(m):
    fr = m['frames']
    try:
        a = next(i for i, f in enumerate(fr) if f['t'] == WANT[0])
    except StopIteration:
        return []
    b = len(fr)
    for i in range(a, len(fr)):
        if fr[i]['t'] == WANT[1]:
            b = i
            break
    return fr[a:b]


def id_key(rows):
    for k in ID_KEYS:
        if isinstance(rows[0], dict) and k in rows[0]:
            return k
    return None


def show(path, brief=False):
    frames, raw = load(path)
    ms = split_matches(frames)
    print('=' * 74)
    print('%s : %d 帧, %d 局' % (path, len(frames), len(ms)))
    for i, m in enumerate(ms):
        if not m['frames']:
            continue
        idx, players, soldiers = creation_index(m)
        block = start_block(m)
        print('-- 第 %d 局  server_fd=%s  server_tid=%s  玩家=%s 士兵(创建序)=%s'
              % (i + 1, m['server_fd'], m['tid'], players, soldiers))
        print('   批大小 %d 帧 / 全匹配 %d 帧 (%.0f%%)'
              % (len(block), len(m['frames']), 100.0 * len(block) / max(1, len(m['frames']))))
        per_type = collections.OrderedDict()
        for f in block:
            per_type.setdefault(f['t'], []).append(f['b'])
        for t, rows in per_type.items():
            if t in SKIP_ORDER:
                print('   %-20s n=%d' % (t, len(rows)))
                continue
            k = id_key(rows)
            if k is None:
                print('   %-20s n=%d' % (t, len(rows)))
                continue
            vals = [r[k] for r in rows]
            print('   %-20s %s' % (t, [idx.get(v, v) for v in vals]))
        turns = [(f['b'].get('player_id'), f['b'].get('soldier_id')) for f in m['frames'] if f['t'] == 'TurnInfo']
        print('   TurnInfo             %s' % [tuple(idx.get(x, x) for x in t) for t in turns])


def per_type(path):
    frames, _ = load(path)
    out = collections.OrderedDict()
    for f in frames:
        out.setdefault(f['t'], []).append(json.dumps(f['b'], sort_keys=True))
    return out


def compare(a, b):
    A, B = per_type(a), per_type(b)
    types = sorted(set(A) | set(B))
    same = [t for t in types if A.get(t) == B.get(t)]
    print('=' * 74)
    print('%s  vs  %s : %d 类相同 / %d 类不同（共 %d 类）'
          % (a, b, len(same), len(types) - len(same), len(types)))
    for t in types:
        if A.get(t) == B.get(t):
            continue
        xa, xb = A.get(t, []), B.get(t, [])
        n = min(len(xa), len(xb))
        k = next((i for i in range(n) if xa[i] != xb[i]), None)
        print('   %-22s 条数 %d/%d  首个差异 #%s' % (t, len(xa), len(xb), k))
        if k is not None:
            print('        %s' % xa[k][:110])
            print('        %s' % xb[k][:110])


def main(argv):
    brief = '--brief' in argv
    paths = [a for a in argv if not a.startswith('--')]
    if not paths:
        print(__doc__)
        return 2
    for p in paths:
        show(p, brief)
    for p in paths[1:]:
        compare(paths[0], p)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
