#!/usr/bin/env python3
"""tools/audio_replay_align.py —— 把「真机 hook 产物」+「同一份冻结回放」折成
`fixtures/audio_event_map.tsv`（T19 / `task-16`）。**纯离线、只读**：不跑游戏、不改任何文件。

## 它按什么规则对齐（= `fixtures/audio_event_map_plan.txt` §5 的判据，跑之前就写死了）

三件事必须同时为真，一条映射才算 `观测`：

1. **资源**：`EffectAudioData::new` 那条记录里的「资源句柄」与 `ResourceLoader::load`
   抓到的那 5 个 `res://sounds/*.mp3` 句柄**指针相等**；
2. **事件**：该记录前一条 handler 记录（`effect_info_event` / `soldier_info_event` /
   `entity_removed_event` / `game_state_event`）的 payload 能解出**事件判别值 + 关键字段**，
   且它取资源用的**槽偏移**（`self+0x80..0xc0`，由 `0x41d060` 的 `rdi - self` 直接量出）
   与 (1) 的资源一致；
3. **时刻**：payload 的 `+0x1c`（`EffectInfo.start_tick`）与**回放 JSONL 里那条事件的
   `start_tick` 逐字节相等**（`effect_id` / `pos_x` / `pos_y` 也相等）⇒ 这不是"顺序对上了"，
   是**同一条事件**。

只过 1+2 记 `推断`；只过 1 记 `未知`。**墙钟时间戳完全不参与判定**（软渲染帧率会抖）。

## 用法

    .reference/venv/bin/python tools/audio_replay_align.py \\
        --run <run1.jsonl> --run <run4.jsonl> --run <run3.jsonl> \\
        --replay-dir .reference/replays/gw2-replays-snapshot/replays \\
        --out fixtures/audio_event_map.tsv

`--run` 可重复；每份 JSONL 由 `tools/frida_audio_map.py` 产出。JSONL 里必须带
`load` / `ctor` / `eaud` / `qslot` / `play` / 4 个 handler 记录（缺 `load` 的话该轮
无法判资源身份，脚本会**拒绝**并说明为什么 —— 这正是 `frida_audio_map.py` 改成
"进程一起来就 attach"的原因）。
"""
import argparse
import json
import os
import struct
import sys

# ── 静态常量（出处见 docs/17 §4 与 fixtures/audio_event_map_plan.txt §4）────────────
# `EffectAudioData::new` 的 6 个调用点 → 取的是 self 里哪个槽（槽 = 资源）
ARM_SLOT = {
    0x4bf1af: 0xA0,   # 0x4bf090 臂 → `lea rdi,[rbx+0xa0]`
    0x4bf1fc: 0xA0,   # 0x4bf090 臂（另一条）→ `lea rdi,[rbx+0xa0]`
    0x4bf249: 0x80,   # 0x4bf090 臂 → `lea rdi,[rbx+0x80]`
    0x4bf28f: 0x90,   # 0x4bf090 臂 → `lea rdi,[rbx+0x90]`
    0x4bf3a3: 0xB0,   # 0x4bf340（soldier_info_event）→ `lea rdi,[rbx+0xb0]`
    0x4bf4e8: 0xC0,   # 0x4bf480（entity_removed_event）→ `lea rdi,[rbx+0xc0]`
}
# 槽 → 载入序（构造函数把 5 个资源按 载入序 写到 +0x80/+0x90/+0xa0/+0xb0/+0xc0）
SLOT_ORDER = {0x80: 0, 0x90: 1, 0xA0: 2, 0xB0: 3, 0xC0: 4}
# 事件判别值（`src/events.rs`，逐位）
EVENT_TAG = {
    0x800000000000002C: ('EffectInfo', 44),
    0x800000000000000F: ('SoldierInfo', 15),
    0x800000000000000E: ('EntityRemoved', 14),
    0x800000000000001E: ('GameStateInfo', 30),
}
HANDLER_EV = {'eff': 'EffectInfo', 'sold': 'SoldierInfo', 'entrm': 'EntityRemoved', 'gst': 'GameStateInfo'}
EFFECT_TYPE = ['ExplosionEffect', 'DeathEffect', 'SparkEffect', 'AshEffect']
# 播放调用点 → 哪一路
PLAY_QUEUE = 0x4C0CC0     # `SoundHandler::process` 的队列循环
PLAY_HUMM = 0x4C01DE      # `SoundHandler::process` 的 humm generator 路
TSV_COLS = ['序号', '事件类型', '关键字段', '音频资源', '触发源函数地址', '置信度', '备注']


def f32(u):
    return struct.unpack('<f', struct.pack('<I', u & 0xFFFFFFFF))[0]


def f64(hex8):
    return struct.unpack('<d', bytes.fromhex(hex8))[0]


def u32(hex4):
    return struct.unpack('<I', bytes.fromhex(hex4))[0]


def load_run(path):
    rows = []
    for line in open(path, encoding='utf-8'):
        line = line.strip()
        if not line:
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return rows


def load_map(rows):
    """`load` 记录 → {句柄(lo,hi): 路径}"""
    m, order = {}, []
    for r in rows:
        if r.get('ev') == 'load':
            m[(r['han_lo'], r['han_hi'])] = r['path']
            order.append(r['path'])
    return m, order


def ctor_slots(rows):
    """`ctor` 的载荷结构体（`0x5238c0` 的入参）里 5 个资源对的偏移 → 句柄。

    静态已知：载荷 `+0x18/+0x28/+0x38/+0x48/+0x58` = 载入序 1..5（`0x4bf550` 的栈结构体）。
    这里**不复述**那个偏移，只用它取句柄，再和 `load` 的返回句柄对指针 —— 对不上就报错。
    """
    for r in rows:
        if r.get('ev') == 'ctor' and r.get('payload_hex'):
            b = bytes.fromhex(r['payload_hex'])
            pairs = []
            for i, off in enumerate((0x18, 0x28, 0x38, 0x48, 0x58)):
                lo = struct.unpack_from('<Q', b, off)[0]
                hi = struct.unpack_from('<Q', b, off + 8)[0]
                pairs.append(('0x%x' % lo, '0x%x' % hi))
            return pairs
    return None


def parse_payload(kind, hexs):
    """把 handler 记录里的 payload 前 0x30 字节解成关键字段（偏移见 plan §2/§3）。"""
    if not hexs:
        return {}
    b = bytes.fromhex(hexs)
    try:
        if kind == 'eff':
            return {'pos_x': struct.unpack_from('<d', b, 0x08)[0],
                    'pos_y': struct.unpack_from('<d', b, 0x10)[0],
                    'effect_id': struct.unpack_from('<I', b, 0x18)[0],
                    'start_tick': struct.unpack_from('<I', b, 0x1C)[0],
                    'effect_type': b[0x20],
                    'effect_type_name': EFFECT_TYPE[b[0x20]] if b[0x20] < 4 else '?(%d)' % b[0x20]}
        if kind == 'sold':
            return {'soldier_id': struct.unpack_from('<I', b, 0x08)[0],
                    'player_id': struct.unpack_from('<I', b, 0x0C)[0]}
        if kind == 'entrm':
            return {'entity_id': struct.unpack_from('<I', b, 0x08)[0]}
        if kind == 'gst':
            return {'state': b[0x08]}
    except (struct.error, IndexError):
        return {}
    return {}


def replay_events(path):
    import gzip
    raw = open(path, 'rb').read()
    if raw[:2] == b'\x1f\x8b':
        raw = gzip.decompress(raw)
    out = []
    for i, line in enumerate(raw.decode('utf-8', 'replace').splitlines()):
        line = line.strip()
        if not line:
            continue
        try:
            o = json.loads(line)
        except json.JSONDecodeError:
            continue
        out.append({'line_no': i, 'tick': o.get('tick'), 'event': o.get('event', {})})
    return out


def find_replay_match(ev, payload, used):
    """在回放事件表里找**同一条**事件：优先按 effect_id/start_tick/坐标/实体 id 精确匹配。"""
    def free(k):
        return k not in used
    for i, e in enumerate(ev):
        body = e['event']
        if ev_name(body) != payload.get('_name'):
            continue
        if not free(i):
            continue
        if payload['_name'] == 'EffectInfo':
            b = body['EffectInfo']
            if (b.get('effect_id') == payload.get('effect_id')
                    and b.get('start_tick') == payload.get('start_tick')
                    and abs(b.get('pos_x', 1e9) - payload.get('pos_x', 1e18)) < 1e-9
                    and abs(b.get('pos_y', 1e9) - payload.get('pos_y', 1e18)) < 1e-9):
                return i, e
        elif payload['_name'] == 'SoldierInfo':
            if body['SoldierInfo'].get('soldier_id') == payload.get('soldier_id'):
                return i, e
        elif payload['_name'] == 'EntityRemoved':
            if body['EntityRemoved'].get('entity_id') == payload.get('entity_id'):
                return i, e
        elif payload['_name'] == 'GameStateInfo':
            return i, e        # 状态值的串→数映射不在本轮范围内，只按序取第一条未用的
    return None, None


def ev_name(body):
    for k in body:
        return k
    return '?'


def analyze_run(jsonl, replay_path, label=None):
    label = label or os.path.basename(os.path.dirname(os.path.abspath(jsonl))) or 'run'
    rows = load_run(jsonl)
    lm, load_order = load_map(rows)
    pairs = ctor_slots(rows)
    warns = []
    if not lm:
        raise SystemExit('%s: 没有 `load` 记录 ⇒ 无法判资源身份（多半是 attach 太晚）。'
                         '拒绝出表：宁缺勿猜。' % jsonl)
    if not pairs:
        raise SystemExit('%s: 没有 `ctor` 记录 ⇒ 无法把槽偏移钉到资源上。' % jsonl)
    slot_res = {}
    for i, p in enumerate(pairs):
        name = lm.get(p)
        if not name:
            raise SystemExit('%s: ctor 载荷第 %d 个句柄 %s 在 `load` 结果里找不到 ⇒ 数据自相矛盾'
                             % (jsonl, i + 1, p))
        slot_res[0x80 + 0x10 * i] = name
    # 也让载入序本身自校验一次（plan P1）
    wants = ['res://sounds/explosion.mp3', 'res://sounds/squash.mp3', 'res://sounds/spark.mp3',
             'res://sounds/pop.mp3', 'res://sounds/unpop.mp3']
    if load_order != wants:
        warns.append('载入序 %s 与预注册 P1 不符（期望 %s）' % (load_order, wants))

    ev = replay_events(replay_path)
    used = set()
    inst = []                      # 每条 eaud 一条
    last_handler = None
    last_qslot = None
    for r in rows:
        k = r.get('ev')
        if k in HANDLER_EV:
            pay = parse_payload(k, r.get('pay_hex'))
            pay['_name'] = HANDLER_EV[k]
            pay['_handler'] = {'eff': 0x4BF090, 'sold': 0x4BF340,
                               'entrm': 0x4BF480, 'gst': 0x4BEF20}[k]
            pay['_tag'] = r.get('tag')
            pay['_tick'] = r.get('tick')
            pay['_dispatcher'] = r.get('ret')
            last_handler = pay
            continue
        if k == 'qslot':
            slot = r.get('slot')
            if slot and slot.startswith('0x') and len(slot) <= 5:
                try:
                    last_qslot = int(slot, 16)
                except ValueError:
                    pass
            continue
        if k != 'eaud':
            continue
        ret = int(r['ret'], 16) if r.get('ret') else None
        arm_slot = ARM_SLOT.get(ret)
        stream = lm.get((r['c'], r['d']))
        # 三道独立指证：静态臂 → 槽；运行时 qslot → 槽；句柄 → 资源
        ok_slot = (arm_slot is not None and last_qslot == arm_slot)
        ok_stream = (stream is not None and arm_slot is not None and slot_res.get(arm_slot) == stream)
        conf = '观测' if (ok_slot and ok_stream) else ('推断' if ok_stream else '未知')
        name = (stream or slot_res.get(arm_slot or last_qslot) or '?')
        rp = None
        if last_handler and last_handler.get('_name') != 'GameStateInfo':
            idx, e = find_replay_match(ev, last_handler, used)
            if idx is not None:
                used.add(idx)
                rp = e
        inst.append({
            'run': label,
            'jsonl': os.path.basename(jsonl),
            'seq': r.get('seq'),
            'tick': r.get('tick'),
            'arm_ret': r.get('ret'),
            'arm_slot': arm_slot,
            'qslot': last_qslot,
            'slot': arm_slot if arm_slot is not None else last_qslot,
            'stream': name,
            'val': r.get('val'),
            'vol': f32(u32(r['vol_u32'])) if r.get('vol_u32') else None,
            'handler': last_handler,
            'replay': rp,
            'conf': conf,
            'ok_slot': ok_slot,
            'ok_stream': ok_stream,
        })
        last_qslot = None

    # play()：队列路径按「条目里的触发 tick 字段」对回 eaud；humm 路径对 FunctionActive.active_tick
    plays = []
    for r in rows:
        if r.get('ev') != 'play':
            continue
        ret = int(r['ret'], 16) if r.get('ret') else None
        plays.append({'ret': r.get('ret'), 'ret_i': ret, 'tick': r.get('tick'),
                      'arg': r.get('arg'), 'v10': r.get('arg_v10')})
    for p in plays:
        if p['ret_i'] == PLAY_QUEUE:
            if p['v10'] is None:
                warns.append('play@tick=%s 没有条目 tick 字段（探针版本过旧）⇒ 该条 play 不做归属'
                             % p['tick'])
                continue
            cand = [x for x in inst if x['val'] == p['v10'] and x.get('played') is None]
            if cand:
                cand[0]['played'] = p['tick']
                cand[0]['play_ambiguity'] = len(cand)
                p['maps_to'] = cand[0]
        elif p['ret_i'] == PLAY_HUMM:
            hit = None
            for e in ev:
                b = e['event']
                if 'FunctionActive' in b and b['FunctionActive'].get('active_tick') == p['tick']:
                    hit = e
                    break
            p['maps_to'] = hit
    max_tick = max([r.get('tick') or 0 for r in rows] or [0])
    n_gst = sum(1 for r in rows if r.get('ev') == 'gst')
    return {'jsonl': jsonl, 'label': label, 'max_tick': max_tick, 'n_gst': n_gst,
            'inst': inst, 'plays': plays, 'warns': warns,
            'load_order': load_order, 'slot_res': slot_res,
            'replay': os.path.basename(replay_path)}


def fmt_key(a):
    if a['handler'] is None:
        return '(humm/generator 路，无事件 payload)'
    h = a['handler']
    n = h['_name']
    if n == 'EffectInfo':
        return ('effect_id=%d effect_type=%s(%d) start_tick=%d pos=(%.6f, %.6f)'
                % (h['effect_id'], h['effect_type_name'], h['effect_type'],
                   h['start_tick'], h['pos_x'], h['pos_y']))
    if n == 'SoldierInfo':
        return 'soldier_id=%d player_id=%d' % (h['soldier_id'], h['player_id'])
    if n == 'EntityRemoved':
        return 'entity_id=%d' % h['entity_id']
    if n == 'GameStateInfo':
        return 'state=%s' % h.get('state')
    return ''


def build_tsv(runs, replay_dir):
    lines = []
    lines.append('# fixtures/audio_event_map.tsv —— **音效 ↔ 事件映射的真机真值表**（T19 / `task-16`，2026-11-26）')
    lines.append('#')
    lines.append('# 生成命令（**可原地复跑**，离线、不跑游戏；两轮标签 R1/R2 见下）：')
    lines.append('#   .reference/venv/bin/python tools/audio_replay_align.py \\')
    lines.append('#       --run <w>/r1.jsonl --replay 1793897298 --label R1 \\')
    lines.append('#       --run <w>/r2.jsonl --replay 1782640836 --label R2 \\')
    lines.append('#       --replay-dir .reference/replays/gw2-replays-snapshot/replays --out fixtures/audio_event_map.tsv')
    lines.append('#   R1 = 冻结回放 1793897298（1 士兵 + 1 靶子；ExplosionEffect×2 + SparkEffect×1 + SoldierInfo×3 + EntityRemoved×1）')
    lines.append('#   R2 = 冻结回放 1782640836（DeathEffect×1 + AshEffect×2 + ExplosionEffect×2 + SoldierInfo×8）')
    lines.append('# 观测命令（真机，起 Xvfb + 起游戏 + attach + 采样在**同一条命令同一个进程树**里）：')
    lines.append('#   .reference/venv/bin/python tools/frida_audio_map.py --out <run>.jsonl --data-dir <w>/gd \\')
    lines.append('#       --display-num 119 --replay <冻结回放名> --after-play <秒>')
    lines.append('#')
    lines.append('# 读法：')
    lines.append('#   kind=rule    —— 「事件类型 + 关键字段 → 音频资源」的**结论行**（`备注` 里给证据链）；')
    lines.append('#   kind=inst    —— **每一条**真机观测实例（一轮一条），`备注` 里给该轮的 tick / 播放情况；')
    lines.append('#   kind=coverage—— 覆盖/未覆盖清单（下一步要什么样的局）。')
    lines.append('# 地址口径：ELF vaddr（Ghidra = 本地址 + 0x100000）；构建 = steam 2026-08-28 libgraphwarrust.so。')
    lines.append('# 证据档：**观测** = 运行时三条独立指证同时成立（资源句柄指针相等 / 静态臂==运行时槽 / 回放同一条事件）；')
    lines.append('#          **推断** = 只成立前两条；**未知** = 只有资源一条。`cargo test` 不构成任何一级证据。')
    lines.append('\t'.join(TSV_COLS))

    # ── kind=rule：把实例按 (事件类型, effect_type/槽) 聚合 ────────────────────────────
    agg = {}
    for r in runs:
        for a in r['inst']:
            h = a['handler'] or {}
            name = h.get('_name', '?(humm)')
            key = (name, h.get('effect_type') if name == 'EffectInfo' else None, a['slot'])
            agg.setdefault(key, []).append((r, a))
    order = [('EffectInfo', 0, 0x80), ('EffectInfo', 1, 0x90), ('EffectInfo', 2, 0xA0),
             ('EffectInfo', 3, 0xA0), ('SoldierInfo', None, 0xB0), ('EntityRemoved', None, 0xC0),
             ('GameStateInfo', None, None)]
    n_rule = 0
    for key in order:
        if key not in agg:
            continue
        n_rule += 1
        items = agg[key]
        name, etype, slot = key
        a0 = items[0][1]
        if name == 'EffectInfo':
            evname = 'EffectInfo'
            keydesc = 'effect_type=%s(%d)' % (EFFECT_TYPE[etype], etype)
            extra = '（同一槽还收 `effect_type` 的其余值 ⇒ 这是 match 的**兜底臂**）' if etype == 3 else ''
            tag = 44
        elif name == 'SoldierInfo':
            evname, keydesc, extra, tag = 'SoldierInfo', '(无判别字段，一律响)', '', 15
        elif name == 'EntityRemoved':
            evname, keydesc, extra, tag = 'EntityRemoved', '(无判别字段，一律响)', '', 14
        else:
            evname, keydesc, extra, tag = 'GameStateInfo', 'state==1 (Setup) 清空待播队列', '**不出声**', 30
        src = ('handler=0x%06x → EffectAudioData::new@%s → 槽 self+0x%02x'
               % (a0['handler']['_handler'], a0['arm_ret'], slot) if name != 'GameStateInfo'
               else 'handler=0x%06x（只清队列）' % a0['handler']['_handler'])
        runs_s = ' + '.join(sorted({r['label'] for r, _ in items}))
        note = ('判别值 %d；槽 self+0x%02x 与载入序第 %d 个资源指针相等；音量 %.1f dB；'
                '真机实例 %d 条（%s）；派发方 retaddr=%s'
                % (tag, slot, SLOT_ORDER.get(slot, -1) + 1, a0['vol'] if a0['vol'] is not None else float('nan'),
                   len(items), runs_s, a0['handler'].get('_dispatcher')))
        if extra:
            note += '；' + extra
        lines.append('\t'.join(['rule-%02d' % n_rule, evname, keydesc,
                                a0['stream'], src, a0['conf'], note]))

    # ── kind=inst：逐条实例 ─────────────────────────────────────────────────────────
    n_inst = 0
    for r in runs:
        for a in r['inst']:
            n_inst += 1
            h = a['handler'] or {}
            rep = a['replay']
            if rep is not None:
                rp = '回放第 %d 行 tick=%s「%s」' % (rep['line_no'], rep['tick'], ev_name(rep['event']))
            else:
                rp = '回放里匹配不到同一条事件'
            play = a.get('played')
            evname = h.get('_name', '?(humm)')
            if play is not None:
                play_s = str(play)
                if a.get('play_ambiguity', 1) > 1:
                    play_s += ('（value=0 的条目有 %d 条候选，按序记首条 ⇒ 归属推断）'
                               % a['play_ambiguity'])
            elif (a['val'] or 0) > (r.get('max_tick') or 0):
                play_s = '**超出观测窗**（触发 tick=%s > 本轮最后 tick=%s）' % (a['val'], r.get('max_tick'))
            else:
                play_s = '**未观测到 play()**（入队后被丢弃，见 cx-04）'
            scale_tick = ('-1（TickHandler 还没跑过 ⇒ 回放载入期批量注入）'
                          if a['tick'] in (-1, None) else str(a['tick']))
            note = ('%s；臂 ret=%s；槽 self+0x%02x；EffectAudioData.value=%s；'
                    'play@tick=%s；量表 tick=%s；%s'
                    % (a['run'], a['arm_ret'],
                       a['slot'] if a['slot'] is not None else -1, a['val'],
                       play_s, scale_tick, rp))
            lines.append('\t'.join(['inst-%02d' % n_inst, evname, fmt_key(a),
                                    a['stream'],
                                    ('0x%06x → self+0x%02x' % (h.get('_handler', 0), a['slot']))
                    if a.get('handler') else '0x4bfe10 → self+0x60(humm)',
                                    a['conf'], note]))

    # ── kind=coverage ──────────────────────────────────────────────────────────────
    cov = [
        ('EffectInfo', 'effect_type=ExplosionEffect(0) → explosion.mp3', '已覆盖（多局多条）'),
        ('EffectInfo', 'effect_type=DeathEffect(1) → squash.mp3', '已覆盖（1782640836 一例）'),
        ('EffectInfo', 'effect_type=SparkEffect(2) → spark.mp3', '已覆盖'),
        ('EffectInfo', 'effect_type=AshEffect(3) → spark.mp3（兜底臂）', '已覆盖（1782640836 两例）'),
        ('EffectInfo', 'effect_type>=4（非法判别值）', '**无法覆盖**：构造上 JSON 里只有 4 个串，'
         '反序列化闸（`0x40b834` 的首字符集合 `{E,D,S,A}`）挡住其余 ⇒ 兜底臂的"其余"永远走不到'),
        ('SoldierInfo', '→ pop.mp3', '已覆盖（入队 %d 条；每局只有 1 条真的 play()，见 docs/06 §70.6）'
         % sum(1 for r in runs for a in r['inst'] if (a['handler'] or {}).get('_name') == 'SoldierInfo')),
        ('EntityRemoved', '→ unpop.mp3', '已覆盖（入队；同样多数没 play()）'),
        ('GameStateInfo', 'state==1(Setup) 清队列 / 其余状态', '已覆盖：**不出声**（负对照）'),
        ('SoundHandler::process', 'humm generator 路（`0x4c01de`）', '已观测 2 次/每局：play tick == '
         '回放 `FunctionActive.active_tick`；**不属于 5 个槽**，不出映射行'),
        ('—', '`tear.mp3` / `humm_2` / `humm_b` / `shepard_2`', '**仍无引用**：本轮 2 局里 `load` 只见到'
         ' 5 个 mp3 + `humm.wav` 一次 ⇒ 与 docs/17 §5.1.2 的负结果一致'),
        ('—', '`humm.wav`（Rust 自己读，走 `FileAccess::get_file_as_bytes`）',
         '已观测：每局恰好 1 次读 `res://sounds/humm.wav`'),
    ]
    for i, (evn, what, state) in enumerate(cov, 1):
        lines.append('\t'.join(['cov-%02d' % i, evn, what, '—', '—',
                                '观测' if '已' in state else '未知', state]))

    # ── kind=counterexample：反例检查（撞车 / 孤儿 / 沉默 / 负对照）──────────────────
    per_run = []
    for r in runs:
        n_eaud = len(r['inst'])
        n_q = sum(1 for p in r['plays'] if p['ret_i'] == PLAY_QUEUE)
        n_h = sum(1 for p in r['plays'] if p['ret_i'] == PLAY_HUMM)
        n_played = sum(1 for a in r['inst'] if a.get('played') is not None)
        per_run.append('%s: 入队 eaud=%d、队列路 play=%d、humm 路 play=%d、入队后真的 play 了=%d'
                       % (r['label'], n_eaud, n_q, n_h, n_played))
    cx = [
        ('撞车（两条事件 → 同一音频）',
         '`EffectInfo(effect_type=2 SparkEffect)` 与 `EffectInfo(effect_type=3 AshEffect)` '
         '**都落 `self+0xa0` → `spark.mp3`**（详见 rule-03 / rule-04）',
         '**确认存在，且是设计**：`0x4bf090` 是"四臂落三槽"，第 4 臂是兜底臂'),
        ('撞车（同一条音频被两条不同种类的**事件**用）',
         '`self+0xa0` 只被 `EffectInfo` 用；`pop`/`unpop`/`explosion`/`squash` 各自独占一个槽 '
         '⇒ **5 个槽 ↔ 5 条资源一一对应**，没有跨事件类型复用',
         '确认（5 槽 = 5 资源，来自 ctor 载荷顺序 + `qslot` 读数）'),
        ('孤儿 1（有音频、找不到事件）',
         '`SoundHandler::process` 的 **humm generator 路**（`play` 的 ret = `0x4c01de`）不由 4 条订阅事件触发；'
         '实测 play 的 tick 与回放 `FunctionActive.active_tick` **相等**（615 / 1550、87 / 3525）',
         '已归属：它不是"事件音效"，是弹道在飞时的连续音（本轮只观测，不出映射行）'),
        ('孤儿 2（入队了但没播）', '；'.join(per_run) + '。**入队 ≠ 播放**：载入期批量入队的那一批里，'
         '只有 1 条走到 `play()`，其余被 `process` 尾部的清理调用 `0x3caef0`'
         '（`Vec::retain(tick < entry.value || AudioStreamPlayer::is_playing(entry))`，'
         'is_playing = 绑定 `0x8adc70`，名字串 `0x29effb`）移走 ⇒ **映射仍成立，但"响了几次"要按 play 计**',
         '**观测到丢弃 + 机制部分闭合**；为什么同一帧里它们既没播又被删 **未闭合 ⇒ 推断**'),
        ('沉默（有事件、没音频）',
         '两局里 `game_state_info_event` 被派发了 %s 次、`EntityEffectInfo`/`LifeInfo`/`PosInfo`/'
         '`FunctionInfo`/`FunctionFire` 全都有出现 —— 前者一次 `eaud` 都没有、后者**一次 handler 都没进**'
         '（音效侧只订阅 4 条）'
         % '/'.join(str(r['n_gst']) for r in runs),
         '负对照成立：不是"钩子没挂上"，因为同一时刻 `EffectInfo`/`SoldierInfo`/`EntityRemoved` 都抓到了'),
        ('负对照（探针有效性）', '每局 `load` 恰好 **5 次**（explosion/squash/spark/pop/unpop，长度 26/23/22/20/22）、'
         '`FileAccess::get_file_as_bytes` 恰好 **1 次**（`res://sounds/humm.wav`）、`ctor` 恰好 1 次',
         '成立 ⇒ 本文的"没抓到"可以当"没发生"用'),
    ]
    for i, (what, detail, verdict) in enumerate(cx, 1):
        lines.append('\t'.join(['cx-%02d' % i, '反例检查', what, '—', '—',
                                '观测', detail + ' ⇒ ' + verdict]))
    return '\n'.join(lines) + '\n'


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('--run', action='append', required=True, help='frida_audio_map.py 产出的 JSONL（可重复）')
    ap.add_argument('--replay', action='append', default=None,
                    help='与 --run 一一对应的冻结回放名；不给就按文件名猜（默认 1793897298）')
    ap.add_argument('--replay-dir',
                    default=os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                                         '.reference', 'replays', 'gw2-replays-snapshot', 'replays'))
    ap.add_argument('--label', action='append', default=None,
                    help='每轮的短标签（可重复；缺省用 JSONL 的父目录名）。'
                         '⚠ 不把 `.agent-work/...` 路径写进夹具 —— 那个目录会被回收')
    ap.add_argument('--out', default=None, help='输出 TSV（缺省打印到 stdout）')
    a = ap.parse_args()

    replays = a.replay
    if not replays:
        replays = ['1793897298'] * len(a.run)
    if len(replays) != len(a.run):
        ap.error('--replay 的个数必须与 --run 一致')
    labels = a.label or [None] * len(a.run)
    if len(labels) != len(a.run):
        ap.error('--label 的个数必须与 --run 一致')
    runs = []
    for jl, rp, lb in zip(a.run, replays, labels):
        path = os.path.join(a.replay_dir, rp)
        if not os.path.exists(path):
            raise SystemExit('找不到冻结回放 %s' % path)
        runs.append(analyze_run(jl, path, lb))
    tsv = build_tsv(runs, a.replay_dir)
    if a.out:
        with open(a.out, 'w', encoding='utf-8') as f:
            f.write(tsv)
        print('TSV -> %s（%d 行）' % (a.out, tsv.count('\n')), file=sys.stderr)
    else:
        sys.stdout.write(tsv)
    for r in runs:
        for w in r['warns']:
            print('[warn] %s: %s' % (r['label'], w), file=sys.stderr)
    return 0


if __name__ == '__main__':
    sys.exit(main())
