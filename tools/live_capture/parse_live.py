#!/usr/bin/env python3
"""tools/live_capture/parse_live.py —— 把 `live_wire.js` 的原始 JSONL 解析成三张入库表（T53）。

输入是探针吐的**原始行**（`.agent-work/...` 里，一次会话一份）；输出是 `fixtures/` 里的
**纯文本**证据表（`AGENTS.md` 规矩：pcap/二进制不进仓库，只提交解析后的文本）：

| 产物 | 内容 |
|---|---|
| `fixtures/protocol_live_frames.jsonl` | 每条真实帧一行：方向 / 事件名 / tag / 负载 hex / sha256 / 解析出的字段与取值 / 时间戳 |
| `fixtures/protocol_enum_domains.tsv` | 线上枚举字段的**实测取值全集**（与 `docs/07` §5.2 的已知集合逐值对照） |
| `fixtures/protocol_zero_frame_events.tsv` | 8 个「零真实帧」事件逐类判定：抓到几次 / 字段顺序与 `WIRE_SPEC` 逐项对比 / 差异 / 升档后该改什么 |

## 三条口径（写死在这里，跑完不许为了对上而改）

1. **方向**由**接收端**定：`role=client, dir=in` ⇒ `server→client`；`role=server, dir=in` ⇒ `client→server`。
   房主局里 TCP 两端**都在本进程**（客户端 `connect` 到自己的 `[::1]:port`），所以同一条帧
   会被两个 fd 各读一次 —— 两个视角的 (方向, 事件名, hex) 相同 ⇒ 按「相邻 + 同 (方向,hex) + 不同 fd」配对去重。
   远端连接（官服 broker / 官服房间）只有一个端点在本进程，`client→server` 只能靠 `role=client, dir=out`
   的 `write/writev` 钩子拿 —— 那一侧**没有接收端视角**，所以允许"只有 out"的帧。
2. **字段顺序** = JSON 里键出现的顺序（serde 的 `Serialize` 按声明顺序写），与 `WIRE_SPEC` 的
   `fields` 列表逐项比。**类型**按 JSON 值的实际形态判（`bool` / `i64` / `f64` / `str` / `null` / `object`），
   与 `WIRE_SPEC` 的 `ValType` 映射后比。
3. **隐私**：`ChatMessage` / `ChatMessageRequest` 的 `message` 正文**一律打码**（只留长度），
   人名字段（`name` / `player_name`）只留长度与是否 ASCII。理由见 `docs/plan-official-capture.md` §1.3。

## 用法

    $PY tools/live_capture/parse_live.py \
        --raw <捕获目录>/F2.jsonl --raw <捕获目录>/G1.jsonl \
        --out-frames fixtures/protocol_live_frames.jsonl \
        --out-enums  fixtures/protocol_enum_domains.tsv \
        --out-zero   fixtures/protocol_zero_frame_events.tsv
"""
import argparse
import hashlib
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

# ── 目标：8 个「403 份冻结回放里零真实帧」的事件（docs/plan-official-capture.md §0）──
TARGETS = ['LockInfo', 'ChatMessage', 'GameHeartbeat', 'GameRemoved',
           'TurnLimitInfo', 'ClearBans', 'BanConnection', 'EndingGame']

# 打码规则：值替换成 `<len=N>`（ChatMessage 正文与人名，见文件头口径 3）
MASK_FIELDS = {'message'}
MASK_NAME_OF = {'name', 'player_name', 'room_name'}
MASK_EVENTS = {'ChatMessage', 'ChatMessageRequest'}

VALTYPE_OF_JSON = {
    'bool': 'Bool', 'int': 'I64', 'float': 'F64', 'str': 'Str',
    'null': 'Null', 'object': 'Object', 'list': 'Array',
}

# WIRE_SPEC 里 ValType ↔ 实测 JSON 形态的相容关系（Option<...> 允许 null）
COMPAT = {
    'Bool': {'bool'},
    'I64': {'int'},
    'U8': {'int'},
    'F64': {'float', 'int'},
    'Str': {'str'},
    'OptI64': {'int', 'null'},
    'OptStr': {'str', 'null'},
    'Color': {'object'},
    'GameInfo': {'object'},
}

# docs/07 §5.2 已定死的 7 个线上枚举（实测值 → 序号，用于对照；序号本身来自文档）
KNOWN_ENUMS = {
    'axis_mode': ('AxisMode', ['NoAxis', 'OnlyMain', 'EveryFive', 'EveryUnit']),
    'function_mode': ('FunctionMode', ['NormalFunction', 'DiffEqFunction']),
    'turn_mode': ('TurnMode', ['SequentialTurns', 'SimultaneousTurns']),
    'time_mode': ('TimeMode', ['Timer30s', 'Timer1m', 'Timer2m', 'Timer3m', 'Timer5m', 'TimerInf']),
    'game_state': ('GraphwarGameState', ['Connecting', 'Setup', 'WaitingForFunctions',
                                         'CalculatingResults', 'SynchronizingResults',
                                         'ExecutingResults', 'EndingGame', 'Disconnected']),
    'team': ('GraphwarTeam', ['Team1', 'Team2']),
    'effect_type': ('EffectType', ['ExplosionEffect', 'DeathEffect', 'SparkEffect', 'AshEffect']),
}


# ══════════════════════════════════════════════════════════════════════════
# WIRE_SPEC 抽取（唯一权威：src/_scaffold/protocol.rs 的 const WIRE_SPEC）
# ══════════════════════════════════════════════════════════════════════════
def load_wire_spec(path=None):
    """从源码抽 `WireEvent { name, tag, fields }` → {name: {'tag':int|None,'fields':[(n,t)]}}。

    不手抄表：直接抽源码，避免"文档抄一份、代码另一份"的老毛病。
    """
    path = path or os.path.join(REPO, 'src', '_scaffold', 'protocol.rs')
    src = open(path, encoding='utf-8', errors='replace').read()
    # const WIRE_SPEC 块
    m = re.search(r'pub const WIRE_SPEC[^=]*=\s*&\[(.*?)\n\];', src, re.S)
    if not m:
        raise SystemExit('抽不到 WIRE_SPEC（%s）' % path)
    block = m.group(1)
    out = {}
    for line in block.splitlines():
        mm = re.search(r'WireEvent\s*\{\s*name:\s*"([^"]+)"', line)
        if not mm:
            continue
        name = mm.group(1)
        t = re.search(r'tag:\s*(Some\((\d+)\)|None)', line)
        tag = int(t.group(2)) if (t and t.group(2)) else None
        f = re.search(r'fields:\s*&\[(.*?)\]\s*\}', line)
        fields = []
        if f:
            for fm in re.finditer(r'\("([^"]+)",\s*ValType::(\w+)\)', f.group(1)):
                fields.append((fm.group(1), fm.group(2)))
        out[name] = {'tag': tag, 'fields': fields}
    if len(out) < 60:
        raise SystemExit('WIRE_SPEC 只抽到 %d 条，明显不对（期望 74）' % len(out))
    return out


# ══════════════════════════════════════════════════════════════════════════
# 原始 JSONL → 帧序列
# ══════════════════════════════════════════════════════════════════════════
def direction_of(rec):
    """接收端定方向（口径 1）。"""
    role, d = rec.get('role'), rec.get('dir')
    if role == 'client' and d == 'in':
        return 'server→client'
    if role == 'server' and d == 'in':
        return 'client→server'
    if role == 'client' and d == 'out':
        return 'client→server'
    if role == 'server' and d == 'out':
        return 'server→client'
    return None


def load_raw(paths):
    """读原始 JSONL → (帧列表, 会话元信息)。帧已由探针切好（u16 大端 + JSON）。"""
    frames, meta = [], {'raw_files': [], 'peers': {}, 'connects': [], 'listens': [],
                        'hookfail': [], 'udp': 0, 'socks': 0}
    for path in paths:
        name = os.path.basename(path)
        meta['raw_files'].append(name)
        for line in open(path, encoding='utf-8', errors='replace'):
            line = line.strip()
            if not line:
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            t = rec.get('type')
            if t == 'frame':
                d = direction_of(rec)
                if not d:
                    continue
                frames.append({'dir': d, 'name': rec.get('name'), 'hex': rec.get('hex'),
                               'n': rec.get('n'), 'text': rec.get('text'), 'fd': rec.get('fd'),
                               'role': rec.get('role'), 'peer': rec.get('peer'),
                               't': rec.get('t'), 'wall': rec.get('wall'),
                               'srcrec': name})
                if rec.get('peer'):
                    meta['peers'][rec['peer']] = meta['peers'].get(rec['peer'], 0) + 1
            elif t == 'connect':
                meta['connects'].append({'addr': rec.get('addr'), 'ret': rec.get('ret')})
            elif t == 'listen':
                meta['listens'].append(rec.get('fd'))
            elif t == 'hookfail':
                meta['hookfail'].append(rec)
            elif t in ('udp_in', 'udp_out'):
                meta['udp'] += 1
            elif t == 'sock':
                meta['socks'] += 1
    return frames, meta


def dedupe(frames):
    """同一帧的两个视角（发送端写 + 接收端读）配对去重。

    判据（口径 1）：**相邻**、同 (方向, 事件名, hex)、**不同 fd**、时间差 ≤ 300 ms。
    远端连接只有一侧视角，天然不会配对。去重条数照实返回，便于复核。
    """
    out, merged = [], 0
    for f in frames:
        if out:
            p = out[-1]
            if (p['dir'] == f['dir'] and p['name'] == f['name'] and p['hex'] == f['hex']
                    and p['fd'] != f['fd'] and abs((f.get('t') or 0) - (p.get('t') or 0)) <= 300):
                p.setdefault('fds', [p['fd']]).append(f['fd'])
                merged += 1
                continue
        f = dict(f)
        f['fds'] = [f['fd']]
        out.append(f)
    return out, merged


# ══════════════════════════════════════════════════════════════════════════
# 字段级解析
# ══════════════════════════════════════════════════════════════════════════
def json_kind(v):
    if v is None:
        return 'null'
    if isinstance(v, bool):
        return 'bool'
    if isinstance(v, int):
        return 'int'
    if isinstance(v, float):
        return 'float'
    if isinstance(v, str):
        return 'str'
    if isinstance(v, list):
        return 'list'
    if isinstance(v, dict):
        return 'object'
    return '?'


def mask_value(ev, key, v):
    """打码（口径 3）：ChatMessage 正文 / 人名类字段 ⇒ 只留长度。"""
    if ev in MASK_EVENTS and key in MASK_FIELDS:
        n = len(v) if isinstance(v, str) else -1
        return '<len=%d>' % n, 'str'
    if key in MASK_NAME_OF and isinstance(v, str):
        return '<str len=%d>' % len(v), 'str'
    return v, json_kind(v)


def parse_frame(f, spec):
    """一帧 → {name, tag, body, dir, fields:[(key, masked_value, kind)], sha256 ...}。"""
    payload = bytes.fromhex(f['hex'])
    sha = hashlib.sha256(payload).hexdigest()
    ev, body, ok = f['name'], None, True
    try:
        obj = json.loads(f['text'])
        keys = list(obj.keys())
        if len(keys) != 1:
            ok = False
        else:
            ev = keys[0]
            body = obj[ev]
    except Exception:                                    # noqa: BLE001
        ok = False
    fields = []
    if ok and isinstance(body, dict):
        for k, v in body.items():
            mv, kind = mask_value(ev, k, v)
            fields.append((k, mv, kind))
    sp = spec.get(ev)
    return {
        'dir': f['dir'],
        'event': ev,
        'tag': (sp or {}).get('tag'),
        'sha256': sha,                                   # 负载字节的 sha256（不是整帧）
        'frame_sha256': hashlib.sha256(bytes([f['n'] >> 8, f['n'] & 0xff]) + payload).hexdigest(),
        'n': len(payload),
        'hex': f['hex'],
        'fields': [(k, v) for (k, v, _) in fields],
        'field_kinds': {k: kd for (k, _, kd) in fields},
        'empty_body': body == {} if ok else None,
        'parse_ok': ok,
        't_ms': f.get('t'),
        'wall': f.get('wall'),
        'peer': f.get('peer'),
        'fds': f.get('fds'),
        'raw_file': f.get('srcrec'),
    }


def wire_spec_compare(parsed, spec):
    """与 WIRE_SPEC 逐项比：字段顺序 / 字段名 / 类型。返回 (verdict, detail)。"""
    ev = parsed['event']
    sp = spec.get(ev)
    if not sp:
        return 'WIRE_SPEC 里没有这个事件', '未知事件名'
    got = [k for (k, _) in parsed['fields']]
    kinds = parsed['field_kinds']
    want = [n for (n, _) in sp['fields']]
    want_t = dict(sp['fields'])
    problems = []
    if got != want:
        problems.append('字段顺序/集合 %s ≠ %s' % (got, want))
    for k in got:
        if k not in want_t:
            problems.append('多出字段 %r' % k)
            continue
        actual = kinds.get(k)
        allowed = COMPAT.get(want_t[k], set())
        if actual not in allowed:
            problems.append('字段 %r 类型实测 %s，WIRE_SPEC 说 %s' % (k, actual, want_t[k]))
    return ('一致' if not problems else '不一致', '; '.join(problems) or '字段顺序与类型逐项相同')


# ══════════════════════════════════════════════════════════════════════════
# 主流程
# ══════════════════════════════════════════════════════════════════════════
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--raw', action='append', required=True, help='live_wire.js 原始 JSONL（可多次）')
    ap.add_argument('--out-frames', default=os.path.join(REPO, 'fixtures', 'protocol_live_frames.jsonl'))
    ap.add_argument('--out-enums', default=os.path.join(REPO, 'fixtures', 'protocol_enum_domains.tsv'))
    ap.add_argument('--out-zero', default=os.path.join(REPO, 'fixtures', 'protocol_zero_frame_events.tsv'))
    ap.add_argument('--spec', default=None, help='WIRE_SPEC 抽取源（默认 src/_scaffold/protocol.rs）')
    ap.add_argument('--session-note', default='', help='写进 frames.jsonl 头行的会话说明')
    args = ap.parse_args()

    spec = load_wire_spec(args.spec)
    frames, meta = load_raw(args.raw)
    uniq, merged = dedupe(frames)
    parsed = [parse_frame(f, spec) for f in uniq]

    # ── 表 1：frames.jsonl（头行 = 会话元信息，便于复核来源）─────────────
    os.makedirs(os.path.dirname(args.out_frames), exist_ok=True)
    with open(args.out_frames, 'w', encoding='utf-8') as fh:
        head = {
            '_meta': 'T53 官服真帧。产出工具 tools/live_capture/parse_live.py；'
                     '采集探针 tools/live_capture/live_wire.js；口径见该脚本文件头。',
            'probe_raw_files': meta['raw_files'],
            'frames_deduped': len(uniq),
            'frames_merged_from_both_endpoints': merged,
            'hookfail': meta['hookfail'],
            'peers': meta['peers'],
            'note': args.session_note,
        }
        fh.write(json.dumps(head, ensure_ascii=False) + '\n')
        for p in parsed:
            fh.write(json.dumps(p, ensure_ascii=False) + '\n')

    # ── 表 2：枚举取值全集（实测 vs docs/07 §5.2 已知集合）────────────────
    seen = {}
    for p in parsed:
        for k, v in p['fields']:
            if k in KNOWN_ENUMS and isinstance(v, str) and not str(v).startswith('<'):
                seen.setdefault(k, set()).add(v)
    os.makedirs(os.path.dirname(args.out_enums), exist_ok=True)
    with open(args.out_enums, 'w', encoding='utf-8') as fh:
        fh.write('field\tenum\tobserved_values（实测，本次会话全集）\tknown_values（docs/07 §5.2 已定死）\n'
                 '\t\tmissing_in_live（已知但本次没出现）\textra_in_live（实测多出来的 = 文档漏项）\n')
        for field, (enum, known) in KNOWN_ENUMS.items():
            obs = sorted(seen.get(field, ()))
            missing = [k for k in known if k not in obs]
            extra = [v for v in obs if v not in known]
            fh.write('%s\t%s\t%s\t%s\n\t\t%s\t%s\n' % (
                field, enum, ','.join(obs) or '(本次未出现)',
                ','.join(known), ','.join(missing) or '-', ','.join(extra) or '-'))

    # ── 表 3：8 个零帧事件逐类判定 ────────────────────────────────────────
    os.makedirs(os.path.dirname(args.out_zero), exist_ok=True)
    counts = {}
    examples = {}
    for p in parsed:
        counts[p['event']] = counts.get(p['event'], 0) + 1
        examples.setdefault(p['event'], []).append(p)
    with open(args.out_zero, 'w', encoding='utf-8') as fh:
        fh.write('事件\ttag\t抓到次数\t字段顺序（实测 vs WIRE_SPEC）\t与 WIRE_SPEC 差异\t'
                 '升档后该改什么\tsha256（首帧负载）\n')
        for name in TARGETS:
            n = 0
            if name == 'ClearBans':                          # Request/Info 两条都算
                n = counts.get('ClearBans', 0) + counts.get('ClearBansRequest', 0)
            elif name == 'BanConnection':
                n = counts.get('BanConnection', 0) + counts.get('BanConnectionRequest', 0)
            else:
                n = counts.get(name, 0)
            ex = None
            for cand in (name, name + 'Request'):
                if examples.get(cand):
                    ex = examples[cand][0]
                    break
            if ex:
                verdict, detail = wire_spec_compare(ex, spec)
                order = '%s（实测） vs %s（WIRE_SPEC）' % (
                    [k for k, _ in ex['fields']],
                    [k for k, _ in spec.get(ex['event'], {'fields': []})['fields']])
                up = ('观测（真帧 %d 条，见 protocol_live_frames.jsonl）' % n
                      if verdict == '一致' else '观测 + 需按实测修正：%s' % detail)
                fh.write('%s\t%s\t%d\t%s\t%s\t%s\t%s\n' % (
                    name, ex['tag'], n, order, detail, up, ex['sha256']))
            else:
                fh.write('%s\t%s\t0\t—\t未能触发（见报告 §遗留：缺什么触发条件）\t'
                         '保持 推断，不许用推断填\t—\n'
                         % (name, (spec.get(name) or {}).get('tag', '—')))

    # ── 摘要（打到 stderr，报告直接抄）────────────────────────────────────
    print('[*] 原始文件: %s' % ', '.join(meta['raw_files']), file=sys.stderr)
    print('[*] 探针帧 %d 条 → 去重后 %d 条（配对合并 %d）' % (len(frames), len(uniq), merged),
          file=sys.stderr)
    print('[*] 事件种类 %d' % len(counts), file=sys.stderr)
    print('[*] 目标事件命中:', file=sys.stderr)
    for name in TARGETS:
        hit = {k: v for k, v in counts.items() if k.startswith(name)}
        print('    %-14s %s' % (name, hit or '0'), file=sys.stderr)
    print('[*] peers: %s' % json.dumps(meta['peers'], ensure_ascii=False), file=sys.stderr)


if __name__ == '__main__':
    main()
