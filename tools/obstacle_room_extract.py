#!/usr/bin/env python3
"""tools/obstacle_room_extract.py —— 把 `tools/obstacle_room.js` 抓到的运行记录
整理成**可入库的文本样本**（`fixtures/obstacle_destroy_pairs.jsonl`）。

输入有两路（都在 `.agent-work/`，**可丢弃、不入库**）：

  ① frida JSONL（`tools/gw2_drive.py --js tools/obstacle_room.js --out ...`）：进程内部一侧
     —— 挖洞指令记录、两次 `queue_destroy_obstacle` 的实参、两/一次 `destroy_obstacle`、
        每个到期 tick 桶里的条目数；
  ② 真游戏自己写的回放（`user://replays/<ts>`，gzip 的 JSONL）：**线上一侧**
     —— `FunctionPoints` / `EffectInfo` / `DestroyObstacle` 的报文原文。

它做的事：把同一发的两路记录对齐成一条 JSONL 记录，并把进程地址换成**稳定的角色标签**
（`server_side` / `client_side`），去掉所有路径与指针，只留可复算的数值与位模式。

`--check` 会逐条复算 5 条恒等式（任何一条不成立就返回 1）：
  ① 挖洞指令的 (x,y) == 点数组**末点**（位模式逐位）；
  ② 点序号 == `len-1`（数组在撞点处被截断）；
  ③ 两次排队的角色恰好是 `{server_side, client_side}`，且 tick 相同；
  ④ 服务端侧记录的 `(field, idx)` == (射手 soldier_id, 点序号)；两侧 radius 位模式相同；
  ⑤ 有回放的那几条：`tick == FunctionPoints.tick + 3 + idx//30 + 60`。

用法：

    ../.reference/venv/bin/python tools/obstacle_room_extract.py \
        --frida <run.jsonl> --replay <user://replays/xxx> \
        --level "Level 2" --fired '["x","0.030708*x","0.030708*x"]' \
        --out fixtures/obstacle_destroy_pairs.jsonl [--append]

观测口径（全部是**位模式**比较，不是"差多少"）：

  * `queue_calls[].x_bits/y_bits` = 进程内 `queue_destroy_obstacle` 的第 2 实参所指记录；
  * `destroy_wire_calls[].x_bits/y_bits` = `destroy_obstacle` 的第 2 实参（= 报文里的坐标）；
  * `wire_replay` = 回放里那一发对应的 `DestroyObstacle` 报文原文。
"""
import argparse
import gzip
import json
import struct
import sys

# `self` 的角色**由调用点定**（不看地址高低）：
#   gw+472d1e = execute_results 的挖洞块（`call 0x3f20f0` 的返回地址）⇒ 权威/服务端侧那个 ObstacleManager
#   gw+3e36bf = graphwar_remote_event_handler(0x3e2830) 里 tag-42 的消费点 ⇒ 客户端侧那个 ObstacleManager
# 两个 self 是**不同的对象**（实测一个是 .so 堆、一个是进程静态低地址），这里只报角色不报指针。
CALL_ROLE = {
    'gw+472d1e': 'server_side',
    'gw+3e36bf': 'client_side',
}


def d(h):
    return None if h is None else struct.unpack('<d', bytes.fromhex(h))[0]


def bits(h):
    """小端位串 → 大端 16 位十六进制（便于人和 `f64::to_bits()` 对照）"""
    if h is None:
        return None
    return '%016x' % struct.unpack('<Q', bytes.fromhex(h))[0]


def ulp_diff(a, b):
    """两个位模式（大端 hex）之间差几个 ulp（按无符号位模式差；同号时即 ulp 数）"""
    if a is None or b is None:
        return None
    ua, ub = int(a, 16), int(b, 16)
    return abs(ua - ub)


def map_roles(queue_calls):
    """self 十六进制串 → 角色（由 queue_destroy 的调用点定）"""
    m = {}
    for q in queue_calls:
        site = (q.get('bt') or [None])[0]
        m[q.get('self')] = CALL_ROLE.get(site, 'unknown@%s' % site)
    return m


def self_role(m, self_hex):
    return m.get(self_hex, 'unknown')


def load_frida(path):
    rows = []
    for line in open(path, encoding='utf-8'):
        try:
            rows.append(json.loads(line))
        except Exception:  # noqa: BLE001
            pass
    return rows


def load_replay(path):
    if not path:
        return []
    raw = gzip.open(path, 'rb').read().decode('utf-8', 'replace')
    out = []
    for line in raw.splitlines():
        try:
            out.append(json.loads(line))
        except Exception:  # noqa: BLE001
            pass
    return out


def shots_from_frida(rows):
    """把一次运行的 frida 记录切成"发"，返回 [{pts, destroy_rec, queue_calls, wire_calls, buckets}]"""
    shots = []
    cur = None
    for r in rows:
        t = r.get('type')
        if t == 'exec_queue':
            cur = {'exec_queue': r, 'pts': None, 'destroy_rec': None,
                   'queue_calls': [], 'wire_calls': [], 'buckets': []}
            shots.append(cur)
            for it in r.get('items') or []:
                if 'pts' in it and cur['pts'] is None:
                    cur['pts'] = it['pts']
                if it.get('kind') == '9223372036854775808' and cur['destroy_rec'] is None:
                    cur['destroy_rec'] = it
        elif cur is None:
            continue
        elif t == 'queue_destroy':
            cur['queue_calls'].append(r)
        elif t == 'destroy_wire':
            cur['wire_calls'].append(r)
        elif t == 'tick_bucket':
            cur['buckets'].append(r)
    return shots


def check(path):
    """自洽性闸门：样本读回来后逐条复算三条恒等式（不命中就返回 1）。"""
    bad = 0
    rows = [json.loads(l) for l in open(path, encoding='utf-8')]
    for r in rows:
        pa, di = r['point_array'], r['destroy_instruction']
        q = r['queue_destroy_calls']
        # ① 挖洞指令的 (x,y) == 点数组**最后一点**（位模式逐位）
        tl = pa['tail_last'] or {}
        if not (di['x_bits'] == tl.get('x_bits') and di['y_bits'] == tl.get('y_bits')):
            print('[FAIL] %s 挖洞指令 != 点数组末点' % r['sample']); bad += 1
        # ② 点序号 == len-1（数组在撞点处被截断）
        if di['point_index'] != pa['last_index']:
            print('[FAIL] %s 序号 %s != len-1 %s' % (r['sample'], di['point_index'], pa['last_index'])); bad += 1
        # ③ 两次排队来自两个不同角色，且 tick 相同（客户端用的是报文里那个 tick）
        roles = sorted(c['role'] for c in q)
        if roles != ['client_side', 'server_side']:
            print('[FAIL] %s 两次排队的角色 = %s' % (r['sample'], roles)); bad += 1
        if len({c['tick'] for c in q}) != 1:
            print('[FAIL] %s 两次排队的 tick 不同 = %s' % (r['sample'], [c['tick'] for c in q])); bad += 1
        # ④ 服务端侧的记录字段 == 射手 id，客户端侧第 3 个字段是 radius（不是 u64 记录）
        srv = [c for c in q if c['role'] == 'server_side'][0]
        cli = [c for c in q if c['role'] == 'client_side'][0]
        if srv['rec_field'] != di['soldier_id'] or srv['rec_index'] != di['point_index']:
            print('[FAIL] %s 服务端记录 (field,idx) != (soldier_id,point_index)' % r['sample']); bad += 1
        if cli['radius_bits'] != srv['radius_bits']:
            print('[FAIL] %s 客户端侧 radius 与生产侧不一致' % r['sample']); bad += 1
        # ⑤ 有回放的那几条：tick == FunctionPoints.tick + 3 + idx//30 + 60（§12.4 那条式子）
        w = r.get('wire_replay')
        if w:
            want = w['tick_function_points'] + 3 + di['point_index'] // 30 + 60
            if w['tick_destroy'] != want or srv['tick'] != want:
                print('[FAIL] %s tick=%s 期望 %s（= fp %s +3 +%d/30 +60）'
                      % (r['sample'], srv['tick'], want, w['tick_function_points'], di['point_index']))
                bad += 1
    print('[check] %d 条样本，%d 条不满足恒等式' % (len(rows), bad))
    return 1 if bad else 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--frida', nargs='+', default=[], help='一个或多个 frida JSONL（--check 时不需要）')
    ap.add_argument('--replay', default='', help='同一次运行的回放（gzip JSONL）')
    ap.add_argument('--level', default='Level 2')
    ap.add_argument('--fired', default='[]', help='JSON 列表：这一局"打中障碍"的每发的函数原文，按序')
    ap.add_argument('--out', required=True)
    ap.add_argument('--append', action='store_true')
    ap.add_argument('--check', action='store_true', help='只做自洽性校验（读 --out 指向的样本）')
    ap.add_argument('--run-base', type=int, default=0, help='样本名里的 run 起始号（多次调用时避免重名）')
    args = ap.parse_args()

    if args.check:
        sys.exit(check(args.out))
    if not args.frida:
        ap.error('--frida 必填（除非 --check）')

    fired = json.loads(args.fired)
    replay = load_replay(args.replay)

    # 回放里按序取每一发的 (FunctionPoints, EffectInfo, DestroyObstacle)
    wire_shots = []
    pend = {}
    for rec in replay:
        ev = rec.get('event') or {}
        for k, v in ev.items():
            if k == 'FunctionPoints':
                pend = {'FunctionPoints': v, 'tick_fp': rec.get('tick')}
            elif k == 'EffectInfo' and v.get('effect_type') == 'ExplosionEffect':
                pend['EffectInfo'] = v
                pend['tick_eff'] = rec.get('tick')
            elif k == 'DestroyObstacle' and pend:
                pend['DestroyObstacle'] = v
                pend['tick_destroy'] = rec.get('tick')
                wire_shots.append(pend)
                pend = {}

    out = []
    fi = args.run_base
    for path in args.frida:
        fi += 1
        shot_no = 0
        rows = load_frida(path)
        obs = [(d(r['x']), d(r['y']), d(r['r'])) for r in rows
               if r.get('type') == 'draw_circle' and r.get('color') == 0]
        for s in shots_from_frida(rows):
            rec = s['destroy_rec']
            if not rec:
                continue
            shot_no += 1
            pts = s['pts'] or {}
            qc = s['queue_calls']
            wc = s['wire_calls']
            roles = map_roles(qc)
            o = {
                'sample': 'obstacle_room/run%d#shot%d' % (fi, shot_no),
                'level': args.level,
                'obstacle': ({'pos_x': obs[0][0], 'pos_y': obs[0][1], 'radius': obs[0][2]}
                             if obs else None),
                'function_text': fired[len(out)] if len(out) < len(fired) else None,
                'point_array': {
                    'len': pts.get('len'),
                    'last_index': (pts.get('len') - 1) if pts.get('len') else None,
                    'head_first': ({'x': d(pts['head'][0][0]), 'y': d(pts['head'][0][1]),
                                    'x_bits': bits(pts['head'][0][0]), 'y_bits': bits(pts['head'][0][1])}
                                   if pts.get('head') else None),
                    'tail_last': ({'x': d(pts['tail'][-1][1]), 'y': d(pts['tail'][-1][2]),
                                   'x_bits': bits(pts['tail'][-1][1]), 'y_bits': bits(pts['tail'][-1][2])}
                                  if pts.get('tail') else None),
                    'bit_identical_point_groups': (pts.get('dup') or {}).get('dupCount'),
                },
                'destroy_instruction': {
                    'tag': hex(int(rec['kind'])),
                    'x': d(rec['x']), 'y': d(rec['y']),
                    'x_bits': bits(rec['x']), 'y_bits': bits(rec['y']),
                    'soldier_id': rec.get('field'), 'point_index': rec.get('idx'),
                },
                'queue_destroy_calls': [{
                    'role': self_role(roles, q.get('self')),
                    'call_site': (q.get('bt') or [None])[0],
                    'x_bits': bits(q.get('x')), 'y_bits': bits(q.get('y')),
                    'x': d(q.get('x')), 'y': d(q.get('y')),
                    'radius_bits': bits(q.get('radius')), 'tick': q.get('tick'),
                    'rec_field': q.get('field'), 'rec_index': q.get('idx'),
                } for q in qc],
                'destroy_wire_calls': [{
                    'role': self_role(roles, w.get('self')),
                    'call_site': (w.get('bt') or [None])[0],
                    'x_bits': bits(w.get('x')), 'y_bits': bits(w.get('y')),
                    'x': d(w.get('x')), 'y': d(w.get('y')),
                    'radius_bits': bits(w.get('radius')),
                } for w in wc],
                'tick_buckets': [{'role': self_role(roles, b.get('self')), 'n_entries': b.get('n'),
                                  'tick': b.get('tickKey')} for b in s['buckets']],
            }
            # 判定：两条报文的坐标是否**逐位**相同
            if len(wc) >= 2:
                o['verdict'] = {
                    'n_wire_messages': len(wc),
                    'x_bits_equal': bits(wc[0]['x']) == bits(wc[1]['x']),
                    'y_bits_equal': bits(wc[0]['y']) == bits(wc[1]['y']),
                    'x_ulp_diff': ulp_diff(bits(wc[0]['x']), bits(wc[1]['x'])),
                    'y_ulp_diff': ulp_diff(bits(wc[0]['y']), bits(wc[1]['y'])),
                    'radius_equal': bits(wc[0]['radius']) == bits(wc[1]['radius']),
                }
            # 线上一侧（回放）：按序对齐
            if len(out) < len(wire_shots):
                w = wire_shots[len(out)]
                o['wire_replay'] = {
                    'FunctionPoints': w.get('FunctionPoints'),
                    'EffectInfo': w.get('EffectInfo'),
                    'DestroyObstacle': w.get('DestroyObstacle'),
                    'tick_function_points': w.get('tick_fp'),
                    'tick_effect': w.get('tick_eff'),
                    'tick_destroy': w.get('tick_destroy'),
                }
                do = w.get('DestroyObstacle') or {}
                tl = o['point_array']['tail_last'] or {}
                wb_x = ('%016x' % struct.unpack('<Q', struct.pack('<d', do['pos_x']))[0]) if 'pos_x' in do else None
                wb_y = ('%016x' % struct.unpack('<Q', struct.pack('<d', do['pos_y']))[0]) if 'pos_y' in do else None
                o.setdefault('verdict', {})['wire_vs_point_array_last'] = {
                    'wire_x_bits': wb_x, 'wire_y_bits': wb_y,
                    'x_bits_equal': wb_x == tl.get('x_bits'),
                    'y_bits_equal': wb_y == tl.get('y_bits'),
                    'x_ulp_diff': ulp_diff(wb_x, tl.get('x_bits')),
                    'y_ulp_diff': ulp_diff(wb_y, tl.get('y_bits')),
                }
            out.append(o)

    mode = 'a' if args.append else 'w'
    with open(args.out, mode, encoding='utf-8') as f:
        for o in out:
            f.write(json.dumps(o, ensure_ascii=False, sort_keys=False) + '\n')
    print('[ok] %d 条 → %s' % (len(out), args.out), file=sys.stderr)
    for o in out:
        v = o.get('verdict') or {}
        print('  %s  x_eq=%s y_eq=%s y_ulp=%s  idx=%s tick=%s' % (
            o['sample'], v.get('x_bits_equal'), v.get('y_bits_equal'), v.get('y_ulp_diff'),
            o['destroy_instruction']['point_index'],
            (o['queue_destroy_calls'] or [{}])[0].get('tick')), file=sys.stderr)


if __name__ == '__main__':
    main()
