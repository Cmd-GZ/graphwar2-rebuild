#!/usr/bin/env python3
"""tools/lan_ai_triple.py —— LAN+AI 局的三样判据（`dispatch/69`，判据同 `docs/06` §40.4）。

输入：`tools/gw2_step_drive.py --triple` 抓下来的**原始 JSONL**。
输出：① 一份可入库的纯文本样本（`--fixture-out`）② 逐发表（12 条判据）打到 stderr。

为什么另写一个而不是直接用 `tools/frida_point_array.py --triple-report`：
那边的"一发"是按**调用点**切组的（`gw+3c30c7` 开新组），这套切法在 §40 的三局里成立
（每发两次调用、两个调用点）。LAN+AI 局里 AI 的**进化**会把同一个采样器打上百次
（每次评分一次 `calculate_shot_results`→`calculate_function`），切组就散成了几百"发"。
本工具反过来：**以 `pos_read`（`0x472865` 真的执行了）为锚**，一发一条，再用
（起点位模式 + 时间序）把 ①token 列表 ②`0x3c5f70` 印出来的串 ③`EffectInfo` 认回去。

判据（**只用同一次开火内的数据**，不许跨发借）：
  const_eq_printed   token 末位 `tag=17` 的 f64 位模式 == `float(印出来的串)` 的位模式（0 ulp 判据）
  printed_round3     `{:.3}`(c_full) == 印出来的串，`c_full = y0 − f(∓x0)`（真求值器 0x4eb9d0）
  array_untouched    `pos_read.vs_calc` 里有 `same=true & diffPoints==0` 的条目
  n_eq_wire          `pos_read.n` == 那条 calc 数组的 `len`（没被夹小）
  copy_exact         ③载荷的 pos 16 字节 == ②那 16 字节
  json_pos_bits_match ③JSON 文本里两个小数 parse 回来 == ②那 16 字节
  on_curve           真求值器 f(pos_x) == pos_y（逐位）
  pos_x_eq_end_x     ②的 pos_x == `FunctionPoints.end_x`（逐位）
  ai_or_human        开火方：`flip`/起点与抓到的那一发的对应关系 + 是否有人打的那一发做对照

用法：
    ../.venv/bin/python tools/lan_ai_triple.py --jsonl <原始.jsonl> \
        --fixture-out fixtures/lan_ai_triple_sample.jsonl --oracle 1
"""
import argparse
import json
import struct
import sys

TAG_ADD = 3         # `+`（`docs/03-function-language.md` §12.1）
TAG_SUB = 4         # `-`
TAG_CONST = 17      # token 语言里的常数（`docs/03-function-language.md` §12）
FIRE_SUFFIX_RET = 'gw+3c5da0'   # `0x3c5f70` 经 `0x3c5c50`（`fire_functions` 那条路）的返回地址


def _d(hexs):
    return struct.unpack('<d', bytes.fromhex(hexs))[0]


def _bits4(d):
    """f64 → **大端**十六进制位模式（人读的形式，与 `docs/06` 里的 `0x4023…` 一致）。"""
    return '%016x' % struct.unpack('<Q', struct.pack('<d', d))[0]


def _be(le_hex):
    """把 **内存字节序**（frida 的 `bytesHex`，小端）的十六进制转成上面 `_bits4` 的形式。"""
    b = bytes.fromhex(le_hex)
    return b[::-1].hex()


def _ulp_diff(a, b):
    """两个 f64 相差几个 ulp（按位模式的有符号差；符号不同返回 None）。"""
    if a is None or b is None:
        return None
    ia = struct.unpack('<q', struct.pack('<d', a))[0]
    ib = struct.unpack('<q', struct.pack('<d', b))[0]
    if (ia < 0) != (ib < 0):
        return None
    return abs(ia - ib)


def decode_tokens(hexs):
    """16 B/token → [(tag, value_bits_hex, value_as_f64_or_None)]"""
    out = []
    for i in range(len(hexs) // 32):
        tag = struct.unpack('<Q', bytes.fromhex(hexs[i * 32:i * 32 + 16]))[0]
        vb = hexs[i * 32 + 16:i * 32 + 32]
        out.append((tag, vb, struct.unpack('<d', bytes.fromhex(vb))[0]))
    return out


def printed_c(text):
    """`" - 2.201"` / `" + 9.961"` → (符号, f64)。空/不可解析返回 (None, None)。"""
    if not text:
        return None, None
    t = text.strip().replace(' ', '')
    if not t:
        return None, None
    sign = t[0] if t[0] in '+-' else '+'
    body = t[1:] if t[0] in '+-' else t
    try:
        v = float(body)
    except ValueError:
        return None, None
    return sign, (v if sign == '+' else -v)


def group_shots(rows):
    """以 `pos_read` 为锚切"发"，并用**独立**的三条链把 ①②③ 认回来（不许用点相等当键）。

    对齐链（每一条都能单独复查）：
      ② ← `pos_read`（`0x472865` 真的执行了）
      ② 的那条数组 ← `calc_array.info.ptr == pos_read.rdx` 且 `len == n`（时间最近的一次）
      ① ← 那条 `calc_array` 的 `rax`（= 隐藏 sret）→ 同 `sret` 且时间在它之前的 `tok_fire`
      ①' ← `0x3c5f70` 的火路径印字：`ret == gw+3c5da0` 且时间在 `fnpoints` 之前最近的一次
      ③ ← `pos_read` 之后最近的 `eff_payload` / `eff_json`
    点是否相等**只作为判据**（`pt_eq_start_ulp`），不作为对齐键 —— 否则判据自证。
    """
    fires = [r for r in rows if r.get('type') == 'tok_fire']
    calcs = [r for r in rows if r.get('type') == 'calc_array']
    sufs = [r for r in rows if r.get('type') == 'tok_suffix' and r.get('ret') == FIRE_SUFFIX_RET]
    poss = [r for r in rows if r.get('type') == 'pos_read']
    effs = [r for r in rows if r.get('type') in ('eff_payload', 'eff_json')]
    fnps = [r for r in rows if r.get('type') == 'fnpoints']
    shots = []
    for pos in poss:
        t = pos['t']
        fn = max((f for f in fnps if f['t'] <= t), key=lambda f: f['t'], default=None)
        if fn is None:
            continue
        # ② 的那条数组：先按 (ptr, len) 认（= 没被截断）；认不到再按"首点 == 这一发的射手点"认
        #    （被障碍/边界截断时队列里的是**截断后的副本**，指针与长度都与采样器的返回不同，
        #      `docs/06` §40.6 第 2 条讲的就是这一族）
        calc = None
        for c in calcs:
            i = c.get('info') or {}
            if c['t'] <= t and i.get('ptr') == pos.get('rdx') and i.get('len') == pos.get('n'):
                if calc is None or c['t'] > calc['t']:
                    calc = c
        if calc is None:
            for c in calcs:
                i = c.get('info') or {}
                pts = i.get('pts') or []
                if c['t'] <= t and pts and pts[0] == (fn['start_x'] + fn['start_y']):
                    if calc is None or c['t'] > calc['t']:
                        calc = c
        fire = None
        if calc is not None:
            rax = calc.get('rax')
            cand = [f for f in fires if f.get('sret') == rax and f['t'] <= calc['t']]
            fire = max(cand, key=lambda f: f['t'], default=None)
        suf = None
        cand = [x for x in sufs if x['t'] <= fn['t']]
        suf = max(cand, key=lambda x: x['t'], default=None)
        eff = sorted([e for e in effs if e['t'] >= t], key=lambda e: e['t'])[:2]
        shots.append(dict(pos=pos, fnp=fn, calc=calc, fire=fire, suf=suf, eff=eff,
                          x0=fn['start_x'], y0=fn['start_y'],
                          flip=(fire.get('flip') if fire else (suf or {}).get('flip'))))
    return shots


def judge_shot(s, oracle=None):
    pos, fn, suf = s['pos'], s['fnp'], s['suf']
    j = {}
    pos_bytes = pos.get('pos_bytes') or ''
    j['pos_hex'] = pos_bytes
    j['pos_x'] = _d(pos_bytes[:16]) if len(pos_bytes) >= 32 else None
    j['pos_y'] = _d(pos_bytes[16:32]) if len(pos_bytes) >= 32 else None
    j['n'] = pos.get('n')
    j['start_x_bits'] = fn.get('start_x')
    j['start_y_bits'] = fn.get('start_y')
    j['end_x_bits'] = fn.get('end_x')
    j['flip'] = s.get('flip')
    j['calc_ret'] = (s.get('calc') or {}).get('ret')
    j['fire_seq'] = (s.get('fire') or {}).get('seq')
    # ── ② 数组没被改过 / n 没被夹小 ───────────────────────────────────────────
    vs = pos.get('vs_calc') or []
    hit = [v for v in vs if v.get('same') and v.get('diffPoints') == 0]
    j['array_untouched'] = bool(hit)
    j['array_hit_len'] = (hit[0].get('len') if hit else None)
    j['n_eq_wire'] = (j['array_hit_len'] == j['n']) if hit else None
    j['vs_calc_n'] = len(vs)
    # ── ① 常数 token vs 印出来的串 ────────────────────────────────────────────
    j['printed_text'] = (suf or {}).get('text')
    sign, printed = printed_c(j['printed_text'])
    j['printed_value'] = printed
    j['printed_bits'] = _bits4(printed) if printed is not None else None
    toks = None
    f = s.get('fire')
    if f:
        tk = decode_tokens((f.get('tok') or {}).get('hex') or '')
        if tk:
            toks = tk
            j['token_ret'] = f.get('ret')
            j['token_seq'] = f.get('seq')
            j['n_tokens'] = len(tk)
    if toks:
        consts = [(i, t) for i, t in enumerate(toks) if t[0] == TAG_CONST]
        if consts:
            i, (tag, vb, v) = consts[-1]
            j['const_index'] = i
            j['const_bits'] = vb
            j['const_value'] = v
            sign_op = toks[0][0] if toks else None
            c_signed = v if sign_op == TAG_ADD else (-v if sign_op == TAG_SUB else None)
            j['const_sign_op'] = sign_op
            j['const_signed'] = c_signed
            j['const_eq_printed'] = ((_bits4(c_signed) == j['printed_bits'])
                                     if (c_signed is not None and j['printed_bits']) else None)
            j['const_ulp_vs_printed'] = _ulp_diff(c_signed, printed) if (c_signed is not None and printed is not None) else None
        j['tokens'] = [{'tag': t[0], 'bits': t[1]} for t in toks]
        j['tag_seq'] = [t[0] for t in toks]
    # ── pos_x == end_x ───────────────────────────────────────────────────────
    j['pos_x_eq_end_x'] = (_be(pos_bytes[:16]) == _be(fn.get('end_x') or '00' * 8)) if pos_bytes else None
    if suf:
        j['pt_x_bits'] = suf.get('x0')
        j['pt_y_bits'] = suf.get('y0')
        j['pt_eq_start_x_ulp'] = _ulp_diff(_d(suf['x0']), _d(fn['start_x']))
        j['pt_eq_start_y_ulp'] = _ulp_diff(_d(suf['y0']), _d(fn['start_y']))
        j['pt_eq_start_exact'] = (suf.get('x0') == fn.get('start_x') and suf.get('y0') == fn.get('start_y'))
    # ── ③ 载荷 / JSON ────────────────────────────────────────────────────────
    pl = [e for e in s['eff'] if e.get('type') == 'eff_payload']
    js = [e for e in s['eff'] if e.get('type') == 'eff_json']
    if pl:
        p = pl[0]
        j['eff_payload_hex'] = (p.get('pos_x_bits') or '') + (p.get('pos_y_bits') or '')
        j['copy_exact'] = (j['eff_payload_hex'] == pos_bytes)
        j['effect_id'] = p.get('effect_id')
        j['start_tick'] = p.get('start_tick')
        j['effect_type'] = p.get('effect_type')
        j['eff_site'] = p.get('site')
    if js:
        j['eff_json_text'] = js[0].get('text')
        try:
            o = json.loads(js[0]['text'])['EffectInfo']
            j['json_pos_bits_match'] = (_bits4(o['pos_x']) + _bits4(o['pos_y'])
                                        == _be(pos_bytes[:16]) + _be(pos_bytes[16:32]))
        except Exception as e:  # noqa: BLE001
            j['json_pos_bits_match'] = None
            j['json_error'] = str(e)
    # ── 真求值器：f(pos_x) == pos_y ──────────────────────────────────────────
    if oracle and toks is not None and j['pos_x'] is not None:
        ev = [(t[0], t[2]) for t in toks]
        try:
            y_direct = oracle.eval(ev, j['pos_x'])
            y_mirror = oracle.eval(ev, -j['pos_x'])
            j['f_pos_x'] = y_direct
            j['f_neg_pos_x'] = y_mirror
            j['on_curve_direct'] = (_bits4(y_direct) == _be(pos_bytes[16:32]))
            j['on_curve_mirror'] = (_bits4(y_mirror) == _be(pos_bytes[16:32]))
            j['on_curve'] = j['on_curve_mirror'] if s.get('flip') else j['on_curve_direct']
        except Exception as e:  # noqa: BLE001
            j['on_curve'] = None
            j['oracle_error'] = str(e)
        # ①=采样器? 用**同一份 token** 在真采样器上重跑，末点必须逐位等于 ②
        try:
            f0 = s.get('fire') or {}
            step = _d(f0['step']) if f0.get('step') else 0.01
            j['step'] = step
            path = oracle.path([(t[0], t[2]) for t in toks], _d(s['x0']), _d(s['y0']),
                               step, f0.get('flip') or 0, f0.get('mode') or 0)
            j['sampler_n'] = len(path)
            if path:
                lb = struct.pack('<d', path[-1][0]).hex() + struct.pack('<d', path[-1][1]).hex()
                j['sampler_last_bits'] = lb
                j['sampler_replay_exact'] = (len(path) == j['n'] and lb == pos_bytes[:32])
        except Exception as e:  # noqa: BLE001
            j['sampler_replay_exact'] = None
            j['sampler_error'] = str(e)
        # `c_full = y0 − f(∓x0)`（原始函数 = 去掉顶层算子与尾部常数后的 token 串）
        try:
            if toks and toks[0][0] in (TAG_ADD, TAG_SUB) and toks[-1][0] == TAG_CONST:
                raw = [(t[0], t[2]) for t in toks[1:-1]]
                x0 = _d(s['x0'])
                y0 = _d(s['y0'])
                fx = oracle.eval(raw, -x0 if s.get('flip') else x0)
                c_full = y0 - fx
                j['c_full'] = c_full
                j['printed_is_round3_of_c_full'] = ('%s%.3f' % ('-' if c_full < 0 else '+', abs(c_full))
                                                   == (s['suf'] or {}).get('text', '').strip().replace(' ', ''))
        except Exception as e:  # noqa: BLE001
            j['c_full_error'] = str(e)
    return j


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--jsonl', required=True)
    ap.add_argument('--label', default='lan-ai')
    ap.add_argument('--fixture-out', default=None)
    ap.add_argument('--oracle', type=int, default=0,
                    help='1 = 用真二进制求值器（tools/posy_unicorn.py 的 Real）判 on_curve / c_full')
    ap.add_argument('--note', default=None)
    a = ap.parse_args()

    rows = [json.loads(l) for l in open(a.jsonl, encoding='utf-8') if l.strip()]
    shots = group_shots(rows)
    print('[group] pos_read 锚点 %d 发' % len(shots), file=sys.stderr)

    oracle = None
    if a.oracle:
        sys.path.insert(0, __import__('os').path.dirname(__import__('os').path.abspath(__file__)))
        import posy_unicorn as pu
        oracle = pu.Real()

    out = []
    prov = {'type': 'provenance', 'schema': 'gw2-lan-ai-triple/1', 'label': a.label,
            'what': '`dispatch/69`：LAN 局（LanLobby→Create Game→GameSetup 加 Bot）里让 AI 开火，'
                    '同一次开火抓 ①0x3c2530 的 token 列表 ②0x472865 的 pts[n-1] ③EffectInfo 载荷+JSON；'
                    '另记 0x3c5f70 吃到的 &Point 与它印出来的串（判"常数末位差"）。',
            'how': 'tools/gw2_step_drive.py 驱动 + tools/frida_point_array.js（--triple）；'
                   '本文件由 tools/lan_ai_triple.py 从原始 JSONL 摘要得到（原始 JSONL 不入库）',
            'counts': {}}
    if a.note:
        prov['notes'] = a.note
    out.append(prov)
    cnt = dict(shots=0, const_eq_printed=0, array_untouched=0, n_eq_wire=0, copy_exact=0,
               json_pos_bits_match=0, on_curve=0, pos_x_eq_end_x=0, fire_path_suffix=0,
               pt_eq_start_exact=0, printed_is_round3_of_c_full=0, sampler_replay_exact=0)
    for i, s in enumerate(shots):
        j = judge_shot(s, oracle)
        row = {'type': 'shot', 'series': a.label, 'shot': i + 1,
               'start_x': s['x0'], 'start_y': s['y0'], 'start_x_value': _d(s['x0']),
               'start_y_value': _d(s['y0']), 'flip': s.get('flip'),
               'judge': j}
        out.append(row)
        cnt['shots'] += 1
        for k in ('const_eq_printed', 'array_untouched', 'n_eq_wire', 'copy_exact',
                  'json_pos_bits_match', 'on_curve', 'pos_x_eq_end_x', 'pt_eq_start_exact',
                  'printed_is_round3_of_c_full', 'sampler_replay_exact'):
            if j.get(k):
                cnt[k] += 1
        if s['suf']:
            cnt['fire_path_suffix'] += 1
        print('  #%d flip=%s n=%s pos=(%r,%r) printed=%r const_ulp=%s const_eq=%s pts_eq=%s '
              'c_full=%r round3=%s sampler=%s array=%s n_eq=%s copy=%s json=%s on_curve=%s end_x_eq=%s tags=%s' % (
                  i + 1, j.get('flip'), j.get('n'), j.get('pos_x'), j.get('pos_y'),
                  j.get('printed_text'), j.get('const_ulp_vs_printed'), j.get('const_eq_printed'),
                  j.get('pt_eq_start_exact'), j.get('c_full'), j.get('printed_is_round3_of_c_full'),
                  j.get('sampler_replay_exact'), j.get('array_untouched'), j.get('n_eq_wire'), j.get('copy_exact'),
                  j.get('json_pos_bits_match'), j.get('on_curve'), j.get('pos_x_eq_end_x'),
                  j.get('tag_seq')), file=sys.stderr)
    prov['counts'] = cnt
    if a.fixture_out:
        with open(a.fixture_out, 'w', encoding='utf-8') as f:
            for r in out:
                f.write(json.dumps(r, ensure_ascii=False) + '\n')
        print('[out] %s' % a.fixture_out, file=sys.stderr)
    print('[counts] %s' % json.dumps(cnt), file=sys.stderr)


if __name__ == '__main__':
    main()
