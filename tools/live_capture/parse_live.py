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

# ── 目标：任务书 §0 的 8 类「403 份冻结回放里零真实帧」的协议事件 ──
#
# ⚠️ 两处**任务书口径需要修正**（本轮实测得出，报告里点名）：
#  ① `EndingGame` **不是线上事件**：`WIRE_SPEC`（74 条）与 `docs/07` §3.3 全表里都没有这个事件名；
#     它是 `GraphwarGameState` 枚举的第 6 个值（`docs/07` §5.2），线上表现为
#     `{"GameStateInfo":{"game_state":"EndingGame"}}`。所以本表按"状态值"判它。
#  ② `ClearBans` / `BanConnection` 的线上**事件名**带 `Request` 后缀
#     （`ClearBansRequest` tag 73 / `BanConnectionRequest` tag 72，见 WIRE_SPEC）。
TARGETS = [
    # (任务书里的名字, 线上事件名, 额外判据)
    ('LockInfo', 'LockInfo', None),
    ('ChatMessage', 'ChatMessage', None),
    ('GameHeartbeat', 'GameHeartbeat', None),
    ('GameRemoved', 'GameRemoved', None),
    ('TurnLimitInfo', 'TurnLimitInfo', None),
    ('ClearBans', 'ClearBansRequest', None),
    ('BanConnection', 'BanConnectionRequest', None),
    ('EndingGame', 'GameStateInfo', ('game_state', 'EndingGame')),
    # 下面两条不在任务书的"8 类"里，但属 `docs/07` §7.7 那 9 条"无抓包"断言（与上面同源），
    # 中文注释：它们是上面两条的**请求方向**对偶（client→server），本轮一起抓到，一并入表。
    ('LockRequest（补：9 条断言之一）', 'LockRequest', None),
    ('ChatMessageRequest（补：9 条断言之一）', 'ChatMessageRequest', None),
]

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
        recs = []
        for line in open(path, encoding='utf-8', errors='replace'):
            line = line.strip()
            if not line:
                continue
            try:
                recs.append(json.loads(line))
            except json.JSONDecodeError:
                continue
        # 老探针（`frida_gate5_wire.js`）的帧记录**没有 `role` 字段** ⇒ 从同一份文件里的
        # `connect` / `accept` 记录把 fd→role 反推出来（房主局两端同进程，方向必须靠 role 判）
        legacy_role = {}
        for rec in recs:
            if rec.get('type') == 'connect':
                legacy_role[rec.get('fd')] = 'client'
            elif rec.get('type') == 'accept':
                legacy_role[rec.get('fd')] = 'server'
        for rec in recs:
            t = rec.get('type')
            if t == 'frame' and not rec.get('role'):
                rec['role'] = legacy_role.get(rec.get('fd'))
            if t == 'frame':
                d = direction_of(rec)
                if not d:
                    continue
                hexs = rec.get('hex')
                src_note = 'hex'
                if not hexs:
                    # 老探针（`tools/frida_gate5_wire.js`）只记 text，不记 hex。
                    # 只要 `n == len(utf8(text))` 就说明负载是纯 ASCII/UTF-8 且未被截断 ⇒ 可无歧义复原。
                    txt = rec.get('text') or ''
                    if rec.get('n') is not None and len(txt.encode('utf-8')) == rec.get('n'):
                        hexs = txt.encode('utf-8').hex()
                        src_note = 'hex_reconstructed_from_text'
                frames.append({'dir': d, 'name': rec.get('name'), 'hex': hexs, 'hex_src': src_note,
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
    # 隐私（口径 3）：聊天正文**连 hex 一起打码** —— 帧里只留 `<len=N>`，
    # 原始负载长度另存 `n`，这样"字段形状 + 长度分布"仍在，正文不在。
    hexout, masked, masked_lens = f['hex'], [], {}
    if ev in MASK_EVENTS and body:
        for k, v in body.items():
            if k in MASK_FIELDS:
                masked.append('<len=%d>' % (len(v) if isinstance(v, str) else -1))
                masked_lens[k] = len(v) if isinstance(v, str) else -1
        mhex = bytes.fromhex(f['hex']).decode('utf-8', 'replace')
        for k, ln in masked_lens.items():
            mhex = re.sub(r'("%s"\s*:\s*)"(?:[^"\\]|\\.)*"' % k,
                          lambda mm, _ln=ln: mm.group(1) + '"<len=%d>"' % _ln, mhex)
        hexout = mhex.encode('utf-8').hex()
    return {
        'dir': f['dir'],
        'event': ev,
        'tag': (sp or {}).get('tag'),
        'sha256': sha,                                   # 负载字节的 sha256（不是整帧）
        'frame_sha256': hashlib.sha256(bytes([f['n'] >> 8, f['n'] & 0xff]) + payload).hexdigest(),
        'n': len(payload),
        'hex': hexout,
        'hex_src': f.get('hex_src', 'hex'),
        'hex_masked': bool(masked),
        'masked_lens': masked_lens,
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



def scan_corpus(snapshot, spec):
    """扫冻结回放快照（gzip 压缩的 JSON 行）里出现过哪些目标事件。

    为什么必须解压后再扫：T46 当初判"9 类零真实帧"时是在**压缩字节**上 grep 的，
    于是 `TurnLimitInfo`（21 份回放里各有 1 帧 `{"turn_limit":5|10|6}`）被漏掉，
    这条假阴性本轮实测纠正（见 docs/06 §103.5）。
    """
    import gzip
    counts, samples = {}, {}
    if not snapshot or not os.path.isdir(snapshot):
        return counts, samples
    names = [t[1] for t in TARGETS] + ['GameStateInfo']
    for fn in sorted(os.listdir(snapshot)):
        path = os.path.join(snapshot, fn)
        try:
            raw = open(path, 'rb').read()
            try:
                data = gzip.decompress(raw)
            except OSError:
                data = raw
            txt = data.decode('utf-8', 'replace')
        except OSError:
            continue
        for nm in names:
            for m in re.finditer(r'\{"%s":\{[^}]*\}\}' % re.escape(nm), txt):
                counts[nm] = counts.get(nm, 0) + 1
                samples.setdefault(nm, []).append((fn, m.group(0)))
    return counts, samples

# ══════════════════════════════════════════════════════════════════════════
# 主流程
# ══════════════════════════════════════════════════════════════════════════
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--raw', action='append', required=True, help='live_wire.js 原始 JSONL（可多次）')
    ap.add_argument('--out-frames', default=os.path.join(REPO, 'fixtures', 'protocol_live_frames.jsonl'))
    ap.add_argument('--out-enums', default=os.path.join(REPO, 'fixtures', 'protocol_enum_domains.tsv'))
    ap.add_argument('--out-zero', default=os.path.join(REPO, 'fixtures', 'protocol_zero_frame_events.tsv'))
    ap.add_argument('--out-expected', default=os.path.join(REPO, 'fixtures', 'protocol_live_expected.tsv'),
                    help='给 src/_scaffold/protocol.rs 的单测编译进去的真帧字节表（9 条断言换真值用）')
    ap.add_argument('--spec', default=None, help='WIRE_SPEC 抽取源（默认 src/_scaffold/protocol.rs）')
    ap.add_argument('--session-note', default='', help='写进 frames.jsonl 头行的会话说明')
    ap.add_argument('--corpus-snapshot', default=None,
                    help='冻结回放快照目录（403 份）；给定时扫里面的目标事件帧，作为"回放里其实有"的证据源')
    args = ap.parse_args()

    spec = load_wire_spec(args.spec)
    corpus_counts, corpus_samples = scan_corpus(args.corpus_snapshot, spec)
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

    # ── 表 3：8 类零帧事件逐类判定 ────────────────────────────────────────
    os.makedirs(os.path.dirname(args.out_zero), exist_ok=True)
    counts, examples = {}, {}
    for p in parsed:
        counts[p['event']] = counts.get(p['event'], 0) + 1
        examples.setdefault(p['event'], []).append(p)

    def task_hits(name, wire, extra):
        """某类目标事件的命中帧（`extra` 用来把 EndingGame 按状态值筛出来）。"""
        rows = examples.get(wire, [])
        if extra:
            k, v = extra
            rows = [r for r in rows if dict(r['fields']).get(k) == v]
        return rows

    with open(args.out_zero, 'w', encoding='utf-8') as fh:
        fh.write('任务书名字\t线上事件名\ttag\t抓到次数\t方向\t'
                 '字段顺序（实测 vs WIRE_SPEC）\t与 WIRE_SPEC 差异\t升档后该改什么\tsha256（首帧负载）\n')
        for name, wire, extra in TARGETS:
            rows = task_hits(name, wire, extra)
            n = len(rows)
            tag = (spec.get(wire) or {}).get('tag')
            if rows:
                ex = rows[0]
                verdict, detail = wire_spec_compare(ex, spec)
                order = '%s（实测） vs %s（WIRE_SPEC）' % (
                    [k for k, _ in ex['fields']],
                    [k for k, _ in spec.get(wire, {'fields': []})['fields']])
                dirs = sorted({r['dir'] for r in rows})
                up = ('观测（真帧 %d 条）' % n if verdict == '一致'
                      else '观测 + 需按实测修正：%s' % detail)
                fh.write('%s\t%s\t%s\t%d\t%s\t%s\t%s\t%s\t%s\n' % (
                    name, wire, tag, n, '/'.join(dirs), order, detail, up, ex['sha256']))
            elif corpus_counts.get(wire):
                # 回放语料里其实有 ⇒ 不是"零真实帧"，是"回放里能观测到、本轮没在线上抓到"
                exs = corpus_samples[wire]
                vals = sorted({re.sub(r'^.*?\{', '{', e[1]) for e in exs})
                fh.write('%s\t%s\t%s\t0（线上）/ %d（冻结语料）\t回放\t%s\t'
                         '线上未抓到；语料里的取值=%s\t观测（语料真帧，%d 份回放）\t—\n'
                         % (name, wire, tag, len(exs), '—',
                            ' | '.join(vals[:4]), len({e[0] for e in exs})))
            else:
                note = '线上 0 帧、冻结语料 0 帧（本轮三会话共 %d 帧里没有）' % len(parsed)
                fh.write('%s\t%s\t%s\t0\t—\t—\t—\t%s，保持 推断（缺的触发条件见 docs/06 §103.6）\t—\n'
                         % (name, wire, tag, note))

    # ── 表 4（小，给单测编译进来）：真帧负载的字节原文 ─────────────────────
    # 目的：`src/_scaffold/protocol.rs` 的 `SAMPLES` 期望值从此**来自抓包**，
    # 不再来自"我方实现自己跑出来的串"。测试逻辑：`payload_hex` 解码后必须与
    # `SAMPLES` 里那条样本的 UTF-8 字节**逐字节相同** ⇒ 样本就是线上真字节。
    exp_rows = []
    for name, wire, extra in TARGETS:
        rows = task_hits(name, wire, extra)
        if not rows:
            # 线上没抓到、但冻结语料里有（`TurnLimitInfo`）⇒ 用**回放里的真帧**当期望值，
            # 这样那条断言一样是从抓包换来的真值，不是自洽。
            if corpus_samples.get(wire):
                txt = corpus_samples[wire][0][1]
                exp_rows.append((wire, name, '回放', txt.encode('utf-8').hex(), txt,
                                 len(txt.encode('utf-8'))))
            continue
        ex = rows[0]
        # 打码过的（聊天）用打码后的 hex；其余用原始 hex
        hexs = ex['hex']
        txt = bytes.fromhex(hexs).decode('utf-8', 'replace')
        exp_rows.append((wire, name, ex['dir'], hexs, txt, ex['n']))
    with open(args.out_expected, 'w', encoding='utf-8') as fh:
        fh.write('# T53 真帧字节表（给 src/_scaffold/protocol.rs 的单测编译进来）\n')
        fh.write('# 产出：tools/live_capture/parse_live.py；采集：tools/live_capture/live_wire.js\n')
        fh.write('# 口径：payload_hex 是线上负载字节（不含 u16 长度前缀）；'
                 '打码帧（ChatMessage）的 message 值已替换为 <len=N>，原始长见 wire_n\n')
        fh.write('event\ttask_name\tdirection\tpayload_hex\tpayload_text\twire_n\n')
        for wire, name, dirn, hexs, txt, n in exp_rows:
            fh.write('%s\t%s\t%s\t%s\t%s\t%d\n' % (wire, name, dirn, hexs, txt, n))

    # ── 摘要（打到 stderr，报告直接抄）────────────────────────────────────
    print('[*] 原始文件: %s' % ', '.join(meta['raw_files']), file=sys.stderr)
    print('[*] 探针帧 %d 条 → 去重后 %d 条（配对合并 %d）' % (len(frames), len(uniq), merged),
          file=sys.stderr)
    print('[*] 事件种类 %d' % len(counts), file=sys.stderr)
    print('[*] 目标事件命中:', file=sys.stderr)
    for name, wire, extra in TARGETS:
        rows = examples.get(wire, [])
        if extra:
            k, v = extra
            rows = [r for r in rows if dict(r['fields']).get(k) == v]
        print('    %-14s %-22s %d' % (name, wire, len(rows)), file=sys.stderr)
    print('[*] peers: %s' % json.dumps(meta['peers'], ensure_ascii=False), file=sys.stderr)


if __name__ == '__main__':
    main()
