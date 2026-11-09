#!/usr/bin/env python3
"""tools/lean_triple.py —— **精简探针**（`tools/frida_lean_triple.js`）的判据（`dispatch/82`）。

为什么要多一层：精简探针为了不拖慢 AI 的进化，**不再给每一次 `calculate_function` 各发一条消息**
（AI 搜索里那是几万条），而是把它们攒在**进程内的环**里，等 `0x472865`（`pos = pts[n-1]` 那条
`movdqu`，每发只执行一次）那一刻连同 `pos` 一起回吐。所以宿主拿到的原始 JSONL 里：

  · `tok_fire` / `calc_array` —— 只有**前 `sendFirst` 条 + 之后每 `sendEvery` 条**（看搜索轨迹用）；
  · 每一发的 ①② 藏在对应 `pos_read` 行的 `recent.fires` / `recent.calcs` 里。

本文件做两件事：

1. **摊平**：把每条 `pos_read.recent` 还原成与 `tools/frida_point_array.js` **同形状**的
   `tok_fire` / `calc_array` 行（字段名、语义都一样），于是
   `tools/lan_ai_triple.py` 里那套过了 §40/§42 的 `group_shots()` / `judge_shot()` **一行不改**就能用；
2. **追加本轮真正要判的那两条**（§42.6 点名的"缺什么"）：
   * `c` 离 3 位小数的**四舍五入边界**（`.xxx5`）多远 —— 绝对量、相对量、**折成 ulp**
     ⇒ 回答"打印级判据在这一发上有没有判别力"（69 号那发离边界 1.3e-4，等于没判别力）；
   * 喂进 `0x3c5f70` 的那个 `&Point` 与报文 `start_x/start_y` 的**逐位**比较（69 号的硬判据）。

用法：
    ../.reference/venv/bin/python tools/lean_triple.py --jsonl <原始.jsonl> --oracle 1
"""
import argparse
import json
import math
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import lan_ai_triple as lat  # noqa: E402  （判据本体：group_shots / judge_shot / _d / _bits4 / _ulp_diff）

FIRE_SUFFIX_RET = lat.FIRE_SUFFIX_RET      # 'gw+3c5da0'


def unfold(rows):
    """把 `pos_read.recent` 摊平成独立行（形状与 `frida_point_array.js` 的消息一致）。

    ⚠ 摊平出来的行**带原来的 `t`**（进程内时钟），所以 `lan_ai_triple.group_shots()` 的
    "时间上最近"那几条对齐链照旧成立；**不改任何字段语义**。
    """
    out = []
    n_fire_syn = n_calc_syn = 0
    for r in rows:
        t = r.get('type')
        if t == 'pos_read' and isinstance(r.get('recent'), dict):
            for f in r['recent'].get('fires') or []:
                if not isinstance(f, dict):
                    continue
                o = dict(f)
                o['type'] = 'tok_fire'
                o['from_ring'] = True
                out.append(o)
                n_fire_syn += 1
            for c in r['recent'].get('calcs') or []:
                if not isinstance(c, dict):
                    continue
                out.append({'type': 'calc_array', 't': c.get('t'), 'ret': c.get('ret'),
                            'rax': c.get('rax'), 'from_ring': True,
                            'info': {'ptr': c.get('ptr'), 'len': c.get('len'),
                                     'seq': c.get('seq'), 'digest_xy': c.get('digest_xy'),
                                     'pts': c.get('pts'), 'bytes_kept': c.get('bytes_kept')}})
                n_calc_syn += 1
        out.append(r)
    return out, n_fire_syn, n_calc_syn


def boundary_metrics(c_full):
    """`c` 离 3 位小数四舍五入边界（`.xxx5`）多远。

    `"{:.3}".format(c)` 只在 `c·1000` 的小数部分穿过 **0.5** 时才会换一位 ⇒ 边界距离
    `d = |frac(c·1000) − 0.5| / 1000`（绝对值），再折成 `c` 在这个量级上的 ulp 数：
    `d / ulp(c)`。**d_ulp 越大，打印级判据越没判别力**（1 ulp 的差根本打不动末位）。
    """
    if c_full is None or not isinstance(c_full, (int, float)) or math.isnan(c_full) or math.isinf(c_full):
        return {}
    x = c_full * 1000.0
    frac = x - math.floor(x)
    d = abs(frac - 0.5) / 1000.0
    # c 所在 binade 的 ulp（IEEE-754 双精度）
    m, e = math.frexp(abs(c_full)) if c_full != 0 else (0.5, 0)
    ulp = math.ldexp(1.0, e - 53)
    return {'c_x1000_frac': frac,
            'boundary_abs': d,
            'ulp_c': ulp,
            'boundary_ulps': (d / ulp) if ulp else None,
            'boundary_rel': (d / abs(c_full)) if c_full else None,
            'printed_3': '%s%.3f' % ('-' if c_full < 0 else '+', abs(c_full))}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--jsonl', required=True, nargs='+')
    ap.add_argument('--label', default='lean-AIvAI')
    ap.add_argument('--oracle', type=int, default=0)
    ap.add_argument('--show-search', type=int, default=1,
                    help='1 = 打印 AI 搜索轨迹的统计（种子个体的 token 数 / 采样次数 / 估算代数）')
    a = ap.parse_args()

    all_rows = []
    for p in a.jsonl:
        all_rows += [json.loads(l) for l in open(p, encoding='utf-8') if l.strip()]
    rows, n_fire_syn, n_calc_syn = unfold(all_rows)
    print('[unfold] 原始 %d 行 → 摊平后 %d 行（环里补出 tok_fire %d / calc_array %d）'
          % (len(all_rows), len(rows), n_fire_syn, n_calc_syn), file=sys.stderr)

    kinds = {}
    for r in rows:
        kinds[r.get('type')] = kinds.get(r.get('type'), 0) + 1
    print('[kinds] %s' % json.dumps(kinds, ensure_ascii=False), file=sys.stderr)
    beats = [r for r in rows if r.get('type') == 'beat']
    if beats:
        print('[beat-last] %s' % json.dumps(beats[-1], ensure_ascii=False), file=sys.stderr)

    # ── 搜索轨迹：AI 交卷前评了多少个体（= 进化跑了多少代）────────────────────────────
    if a.show_search:
        fires = [r for r in rows if r.get('type') == 'tok_fire']
        ntoks = {}
        for f in fires:
            n = ((f.get('tok') or {}).get('len'))
            if n is not None:
                ntoks[n] = ntoks.get(n, 0) + 1
        last_beat = beats[-1] if beats else {}
        samplers = last_beat.get('samplers')
        print('[search] tok_fire 观测 %d 条；token 个数直方图 %s' % (len(fires), ntoks), file=sys.stderr)
        if samplers:
            for pop in (250, 200, 150, 100, 50):
                print('[search] samplers=%s ⇒ 评分≈%.0f ⇒ 代数(种群 %d)≈%.1f'
                      % (samplers, samplers / 2.0, pop, samplers / 2.0 / (pop + 3)), file=sys.stderr)

    shots = lat.group_shots(rows)
    print('[group] pos_read 锚点 %d 发' % len(shots), file=sys.stderr)

    # ── ③ 的配对修正（**不是**放宽判据，而是按"哪一个 EffectInfo 才是这一发的落点"选）──────
    # `lan_ai_triple.group_shots()` 取"`pos_read` 之后最近的 eff_payload/eff_json"；
    # 60/69 那两次一枪只有一个效果，所以没暴露问题。AI 收敛后一枪常常**同时**打掉士兵
    # 又炸到墙 ⇒ 同一发里有 AshEffect（站点 B，士兵死亡处）与 ExplosionEffect（站点 A，
    # 这一发的落点）。按"时间最近"会配到 Ash 上，于是 `copy_exact` 变成 False —— 那是**配错**，
    # 不是数组被改。这里改成：**站点 A（Explosion）优先**，取它之后最近的 JSON。
    for s in shots:
        t = s['pos']['t']
        cands = [e for e in rows if e.get('type') in ('eff_payload', 'eff_json') and e.get('t', 0) >= t]
        siteA = [e for e in cands if e.get('type') == 'eff_payload'
                 and str(e.get('site', '')).startswith('A')]
        if not siteA:
            continue
        first = min(siteA, key=lambda e: e['t'])
        # JSON 与载荷按 **effect_id** 配（同一发里常常先有 Ash 后有声 Explosion，
        # 按时间"最近"会配到另一条的 JSON 上 —— 那是配错，不是数值不同）
        js = [e for e in cands if e.get('type') == 'eff_json'
              and e.get('effect_id') == first.get('effect_id') and e['t'] >= first['t']]
        s['eff'] = [first] + ([min(js, key=lambda e: e['t'])] if js else [])
        if not js:
            print('[eff-pick] ⚠ no eff_json with effect_id=%s' % first.get('effect_id'), file=sys.stderr)
        print('[eff-pick] shot@t=%s → site=%s type=%s pos=(%r,%r)'
              % (t, first.get('site'), first.get('effect_type'),
                 first.get('pos_x'), first.get('pos_y')), file=sys.stderr)

    oracle = None
    if a.oracle:
        import posy_unicorn as pu
        oracle = pu.Real()

    summary = []
    for i, s in enumerate(shots):
        j = lat.judge_shot(s, oracle)
        bm = boundary_metrics(j.get('c_full'))
        j.update(bm)
        # ── ② 的第二口径：截断族（队列项 = 采样器数组的**前 n 点**）──────────────────────
        vs = s['pos'].get('vs_calc') or []
        prod = [v for v in vs if v.get('same_ptr')]
        pre = [v for v in vs if v.get('prefix_same') is True and v.get('prefix_diffPoints') == 0]
        j['producer_found'] = bool(prod)
        j['producer_len'] = prod[0].get('len') if prod else None
        j['prefix_untouched'] = bool(pre)
        j['truncated_from_len'] = pre[0].get('truncated_from_len') if pre else None
        j['vs_counts'] = s['pos'].get('vs_counts')
        sent = (s.get('suf') or {})
        rec = {'shot': i + 1,
               'flip': s.get('flip'), 'n': j.get('n'),
               'n_tokens': j.get('n_tokens'), 'tag_seq': j.get('tag_seq'),
               'printed': j.get('printed_text'),
               'c_full': j.get('c_full'),
               'const_eq_printed': j.get('const_eq_printed'),
               'const_ulp_vs_printed': j.get('const_ulp_vs_printed'),
               'printed_is_round3_of_c_full': j.get('printed_is_round3_of_c_full'),
               'array_untouched': j.get('array_untouched'),
               'prefix_untouched': j.get('prefix_untouched'),
               'producer_found': j.get('producer_found'),
               'producer_len': j.get('producer_len'),
               'truncated_from_len': j.get('truncated_from_len'),
               'n_eq_wire': j.get('n_eq_wire'),
               'on_curve': j.get('on_curve'),
               'pos_x_eq_end_x': j.get('pos_x_eq_end_x'),
               'pt_eq_start_exact': j.get('pt_eq_start_exact'),
               'pt_eq_start_x_ulp': j.get('pt_eq_start_x_ulp'),
               'sampler_replay_exact': j.get('sampler_replay_exact'),
               'copy_exact': j.get('copy_exact'),
               'json_pos_bits_match': j.get('json_pos_bits_match'),
               'boundary_abs': j.get('boundary_abs'),
               'boundary_ulps': j.get('boundary_ulps'),
               'boundary_rel': j.get('boundary_rel'),
               'c_x1000_frac': j.get('c_x1000_frac'),
               'suffix_ret': sent.get('ret'),
               'fire_seq': j.get('fire_seq')}
        summary.append(rec)
        print('  #%d flip=%s n=%s tokens=%s(%s) printed=%r c_full=%r 边界: abs=%.3e ulp=%.3g | '
              'const_eq=%s round3=%s array=%s prefix=%s(prod=%s,len=%s,trunc=%s) n_eq=%s '
              'on_curve=%s end_x=%s pt_eq_start=%s replay=%s'
              % (i + 1, rec['flip'], rec['n'], rec['n_tokens'], rec['tag_seq'], rec['printed'],
                 rec['c_full'], (rec['boundary_abs'] or float('nan')),
                 (rec['boundary_ulps'] or float('nan')),
                 rec['const_eq_printed'], rec['printed_is_round3_of_c_full'],
                 rec['array_untouched'], rec['prefix_untouched'], rec['producer_found'],
                 rec['producer_len'], rec['truncated_from_len'],
                 rec['n_eq_wire'], rec['on_curve'],
                 rec['pos_x_eq_end_x'], rec['pt_eq_start_exact'], rec['sampler_replay_exact']),
              file=sys.stderr)
    print('[summary] %s' % json.dumps(summary, ensure_ascii=False, default=str), file=sys.stderr)


if __name__ == '__main__':
    main()
