#!/usr/bin/env python3
"""tools/frida_ode_edge.py —— `tools/frida_ode_edge.js` 的驱动（`dispatch/45`）。

从 `tools/frida_ode_probe.py`（40 号）复制出来的独立一支：**不改 40 号那两支**（44 号在用）。

它做四件事（必须同一个 PID 命名空间里跑完，frida 的 local device 才能看见进程）：

1. **造一局 `Mode::Ode`**：往工作区的 `XDG_DATA_HOME` 里预写 `user://campaign.save = level_20`，
   让战役 21 关全部解锁；然后点 **Campaign → 第 N 关节点 → Start**
   （Level 16/17/18/19 = 战役第 17/18/19/20 关，`docs/10-campaign.md` §4 的模式四字节
   `func=1`，逐位验证；其中 17 及以后**有障碍**）；
2. 拉起真游戏（**本机必须 `--attach`**：frida `spawn` 会把这游戏挂在 `wait4`、永不建窗口）；
3. 载入 `tools/frida_ode_edge.js` 下断（只读），用 `xdotool` 打表达式 + **长按** Fire!；
   ★ 打字**之前**先 `post` 一条 `marker{shot,text,level}` ⇒ 样本里带得出"这一发是哪个式子"；
4. 把消息写成 JSONL；`--extract` 再把原始 JSONL 摘成入库用的纯文本样本
   （含**运行时判定**的停因 `stop.kind`）。

用法：

    # ★ 本机唯一跑得通的姿势
    .reference/venv/bin/python tools/frida_ode_probe.py --attach \\
        --out <绝对路径>/ode.jsonl --display 127.0.0.1:92 \\
        --campaign-level 20 --level-node 691 751 \\
        --shots 12 --gap 9 --exprs "y,-y,sin(x)-y/2,x/2-y" --tail 25

    # 只摘样本（不给游戏）
    .reference/venv/bin/python tools/frida_ode_probe.py \\
        --extract ode.jsonl --fixture-out fixtures/frida_ode_sample.jsonl

坐标（1920×1160 窗口，本轮实测，见 docs/06 §26）：
    (534, 629)   主菜单 Campaign
    (691, 751)   关卡列表第 **16** 关的节点（三行七列，第三行第三个）
    (1072,1009)  Level N 弹窗上的 Start
    (1000,1090)  `dy/dx =` 输入框（**先 ctrl+a**，再 type）
    (1855, 55)   Fire! ← **长按** 2 s
"""
import argparse
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))


def _find_repo():
    """往上找主仓库根（含 `.reference/game`）；本脚本在 `.agent-work/**` 里跑也找得到。"""
    d = HERE
    for _ in range(12):
        if os.path.isdir(os.path.join(d, '.reference', 'game')):
            return d
        nd = os.path.dirname(d)
        if nd == d:
            break
        d = nd
    return os.path.dirname(HERE)


REPO = _find_repo()
GAME = os.path.join(REPO, '.reference', 'game')
JS = os.path.join(HERE, 'frida_ode_edge.js')

# 进对局的三步（与 34 号同一套菜单；只有第二步的坐标按关卡号变）
NAV_MAIN = (0.0, 'mousemove 534 629', 0.3, 'click 1')      # Campaign
CLICK_INPUT = ('mousemove 1000 1090', 'click 1')           # `dy/dx =` 输入框
FIRE_MOVE = 'mousemove 1855 55'
FIRE_DOWN = 'mousedown 1'

# 战役列表 21 个按钮的实测坐标（1920×1160；三行七列、**蛇形编号**）
#   行1 y=224: 0 1 2 3 4 5 6     行2 y=486: 13 12 11 10 9 8 7    行3 y=751: 14 15 16 17 18 19 20
LEVEL_NODE = {
    0: (243, 224), 1: (486, 224), 2: (700, 224), 3: (963, 224), 4: (1187, 224), 5: (1429, 224), 6: (1672, 224),
    13: (212, 486), 12: (467, 486), 11: (698, 486), 10: (963, 486), 9: (1187, 486), 8: (1429, 486), 7: (1657, 486),
    14: (218, 751), 15: (467, 751), 16: (691, 751), 17: (963, 751), 18: (1187, 751), 19: (1429, 751), 20: (1672, 751),
}

FIXTURE_SCHEMA = 'gw2-frida-ode-edge-sample/1'


def xdo(display, action):
    subprocess.run('xdotool ' + action, shell=True, env=dict(os.environ, DISPLAY=display),
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def _win_id(display):
    env = dict(os.environ, DISPLAY=display)
    for pat in ('^Graphwar$', 'Graphwar'):
        try:
            out = subprocess.run(['xdotool', 'search', '--name', pat], env=env,
                                 capture_output=True, text=True).stdout.split()
        except Exception:  # noqa: BLE001
            out = []
        if out:
            return out[-1]
    return 'root'


def _frame_mean(display):
    """抓一帧算平均亮度（ImageMagick，不依赖 PIL）。失败返回 None。"""
    try:
        w = _win_id(display)
        p = subprocess.run('import -window %s png:- | identify -format "%%[fx:mean]" -' % w,
                           shell=True, env=dict(os.environ, DISPLAY=display),
                           capture_output=True, text=True, timeout=10)
        return float(p.stdout.strip())
    except Exception:  # noqa: BLE001
        return None


def wait_drawn(display, min_mean=0.05, timeout=60.0):
    """★ 等**画面真的画出来**再点菜单。

    为什么不能只睡固定秒数：本轮实测（`docs/06` §26.1 坑 2）——`--settle 8` 时主菜单**已经画出来**，
    但第一次 `Campaign` 点击**没生效**（后面那一下点到了主菜单的 `Player`），整轮白跑。
    这里改成"平均亮度够亮（不是全黑）"再等 1 s ⇒ 不靠猜时间。
    """
    t0 = time.time()
    while time.time() - t0 < timeout:
        m = _frame_mean(display)
        if m is not None and m > min_mean:
            time.sleep(1.0)
            print('[wait] frame drawn (mean=%.3f) after %.1fs' % (m, time.time() - t0), file=sys.stderr)
            return True
        time.sleep(0.5)
    print('[wait] WARN frame still dark after %.1fs' % (time.time() - t0), file=sys.stderr)
    return False


def snap(display, d, label):
    if not d:
        return
    try:
        os.makedirs(d, exist_ok=True)
        p = os.path.join(d, '%s.png' % label)
        subprocess.run(['import', '-window', _win_id(display), p],
                       env=dict(os.environ, DISPLAY=display),
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print('[snap] %s' % p, file=sys.stderr)
    except Exception as e:  # noqa: BLE001
        print('[snap-warn]', e, file=sys.stderr)


def write_campaign_save(data_dir, level):
    """`user://campaign.save` = `level_<n>`（纯文本，`docs/10` §2 逐位验证过的格式）。

    游戏侧 `0x418170` 读全文 → `rsplit('_')` → `u32`；`level_20` = 解锁到 20（全部 21 关）。
    """
    p = os.path.join(data_dir, 'data', 'godot', 'app_userdata', 'Graphwar', 'campaign.save')
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, 'w', encoding='utf-8') as f:
        f.write('level_%d' % level)
    print('[save] %s = level_%d' % (p, level), file=sys.stderr)
    return p


def shot_actions(expr):
    """一发：点输入框 → ctrl+a → 打字；然后移到 Fire! → 长按 2 s。

    ⚠ 三条实测坑（34 号踩过，见 docs/06 §43.1）：`mousemove` 与 `click`/`mousedown`
      必须拆成两条 xdotool 命令；不 `ctrl+a` 打字会接在旧文本后面；Fire! 是长按。
    """
    pre = [(0.0, CLICK_INPUT[0]), (0.35, CLICK_INPUT[1]),
           (0.4, 'key --clearmodifiers ctrl+a'), (0.3, 'type --delay 120 "%s"' % expr)]
    fire = [(1.2, FIRE_MOVE), (0.4, FIRE_DOWN), (2.0, 'mouseup 1')]
    return pre, fire


# ═══════════════════════════════════════════════════════════════════════════════
# `--extract`：原始 JSONL → 入库用的**纯文本**样本
#   一条 `calc` 可能 60 KB（带整条数组 hex），入库只要：(mode, flip, x0/y0/step, tokens)、
#   每次调用的 (ret, ptr, n, 摘要, 首末点)、`pos_read` 的 16 字节与"哪一次调用"、队列项。
#   路径/用户名/整条数组 hex 一律不进。
# ═══════════════════════════════════════════════════════════════════════════════
def _tok_min(toks):
    return [{'t': t.get('t'), 'v': t.get('v')} for t in (toks or [])]


def _stop_min(st):
    """停因（**运行时**判定）：五条出口各自在判决指令上下断得到的那一条。"""
    if not st:
        return None
    keys = ('kind', 'at', 'how', 'count', 'budget', 'count_eq_budget', 'len', 'cap',
            'mid', 'mid_f', 'cand_y', 'cand_y_f', 'xmm0', 'xmm1',
            'arr_n', 'count_plus1_eq_n', 'from_epilogue', 'err')
    o = {k: st.get(k) for k in keys if st.get(k) is not None}
    # 派生（驱动的判定在收尾处做，探针老版本没写这一格）：预算用尽 = 接受点数 == 预算
    if o.get('kind') == 'Budget' and 'count_eq_budget' not in o:
        o['count_eq_budget'] = (o.get('count') == o.get('budget'))
    return o


def _calc_min(c, keep_tokens):
    a = c.get('arr') or {}
    o = {'seq': c.get('seq'), 'ret': c.get('ret'), 'mode': c.get('mode'), 'flip': c.get('flip'),
         'x0': c.get('x0'), 'x0_f': c.get('x0f'), 'y0': c.get('y0'), 'y0_f': c.get('y0f'),
         'step': c.get('step'), 'step_f': c.get('stepf'),
         'arr': {k: a.get(k) for k in ('ptr', 'n', 'digest_xy', 'digest_y', 'first', 'last',
                                       'second_last', 'skipped') if a.get(k) is not None}}
    if c.get('stop') is not None or c.get('stop_epilogue') is not None:
        o['stop'] = _stop_min(c.get('stop'))
        epi = c.get('stop_epilogue') or {}
        o['stop_check'] = {k: epi.get(k) for k in ('count', 'budget', 'len', 'mid_f', 'cand_y_f', 'suggests')
                           if epi.get(k) is not None}
        if c.get('accept_hits') is not None:
            o['accept_hits'] = c.get('accept_hits')
        if c.get('dur_ms') is not None:
            o['dur_ms'] = c.get('dur_ms')
        if c.get('stop_conflict'):
            o['stop_conflict'] = c.get('stop_conflict')
    if keep_tokens:
        o['tokens'] = _tok_min((c.get('tokens') or {}).get('toks'))
    return o


def _pick_calc(pos, pend):
    """从这一发所有的 `calc` 里点名"报文读的那一块"。

    ⚠ 必须**按 seq 点名**，而且 `same_ptr` 单独不够：实测同一条堆缓冲会被后一发复用
      （本轮第 3、4 发的 `rdx` 完全相同、长度不同），所以判据是
      **same_ptr 且 same_len**，多条命中取 seq 最大的；再用摘要复核一次。
    """
    which = pos.get('which_calc') or []
    cands = [w for w in which if w.get('same_ptr') and w.get('same_len')]
    cands.sort(key=lambda w: (w.get('seq') or 0))
    for want_digest in (True, False):
        for w in reversed(cands):
            c = next((x for x in pend if x.get('seq') == w.get('seq')), None)
            if c is None:
                continue
            a = c.get('arr') or {}
            if want_digest and a.get('digest_xy') != pos.get('digest_xy'):
                continue
            return c, ('seq+ptr+len+digest' if (want_digest and a.get('digest_xy') == pos.get('digest_xy'))
                       else 'seq+ptr+len')
    # 退路：指针相同的最后一条（记录用，样本里会带 report_array_pick 说明）
    if cands:
        c = next((x for x in pend if x.get('seq') == cands[-1].get('seq')), None)
        if c is not None:
            return c, 'ptr+len(digest differs)'
    return (pend[-1] if pend else None), 'fallback:last_calc'


def _pos_min(p):
    return {k: p.get(k) for k in ('seq', 'rdx', 'rcx', 'n', 'r13', 'r15', 'pos_bytes', 'pos_x', 'pos_y',
                                  'pos_xf', 'pos_yf', 'first', 'second_last', 'digest_xy', 'which_calc',
                                  'vs_queue') if p.get(k) is not None}


def _queue_min(q):
    if not q:
        return None
    return {'t': q.get('t'), 'qptr': q.get('qptr'), 'qlen': q.get('qlen'),
            'items': [{k: it.get(k) for k in ('item', 'tag', 'lenOff', 'ptr', 'len', 'digest_xy', 'first', 'last')
                       if it.get(k) is not None} for it in (q.get('items') or [])],
            'eval_sample': q.get('eval_sample')}


def parse_raw(path):
    rows = []
    with open(path, encoding='utf-8') as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def extract(specs, out_path, notes=None, repro=None):
    """`specs` = [(jsonl, label), …]；一"发"= 一个 `pos_read` + 它前面最近的那批 `calc`。

    配对规则（写死在这里 = 别人拿原始 JSONL 能一条命令重放出同一个文件）：
      * `pos_read` 出现 ⇒ 收一发；这一发的 `calc` 是**自上一个 `pos_read` 以来**的所有采样器调用；
      * `pos_read.which_calc` 里 `same_ptr=true` 的那条 = **报文实际读的那一块**（按指针点名）。
    """
    all_shots, sources = [], []
    rep = {}
    if repro:
        # `--repro` 是 `examples/ode_array_check --json` 的输出：每行一条 `calc`
        # （一发两次调用 → 两行）。这里只留 `ret == gw+3c30c7` 那一条 = 报文实际读的那块。
        with open(repro, encoding='utf-8') as f:
            for r in json.load(f):
                if str(r.get('ret', '')).endswith('3c30c7') or r.get('idx_in_shot') == 0:
                    rep[r.get('shot')] = r
    hooks = selftest = None
    for path, label in specs:
        rows = parse_raw(path)
        import hashlib
        sha = hashlib.sha256(open(path, 'rb').read()).hexdigest()
        by_type = {}
        for r in rows:
            by_type[r.get('type', '?')] = by_type.get(r.get('type', '?'), 0) + 1
        hooks = hooks or next((r for r in rows if r.get('type') == 'hooks'), None)
        selftest = selftest or next((r for r in rows if r.get('type') == 'selftest'), None)
        pend, queue, shots = [], None, []
        marker, markers = None, []
        for r in rows:
            t = r.get('type')
            if t == 'calc':
                pend.append(r)
            elif t == 'marker':
                # 宿主在**打字之前**写进同一条流的那一条：这一发打的式子 / 关卡
                marker = {'shot': r.get('shot'), 'text': r.get('text'), 'level': r.get('level')}
                markers.append(marker)
            elif t == 'exec_queue':
                queue = r
            elif t == 'pos_read':
                pos = r
                arr_calc, picked = _pick_calc(pos, pend)
                # ⚠ 配对按**序号**：采样器一卡就是几十秒，驱动会在游戏冻住时把后面几发的
                #   marker 先写进来（本轮实测 4 发只出了 2 个 pos_read）⇒ "最近一条 marker"
                #   会把第 1 发配成第 3 发的式子。第 i 个 `pos_read` ↔ 第 i 条 marker。
                mk = markers[len(shots)] if len(shots) < len(markers) else marker
                shots.append({
                    'type': 'shot', 'series': label, 'shot': len(shots),
                    'expression': (mk or {}).get('text'),
                    'level': (mk or {}).get('level'),
                    'marker_pairing': 'index' if len(shots) < len(markers) else 'last_seen',
                    'pos_read': _pos_min(pos),
                    'report_array_pick': picked,
                    'report_array_calc': _calc_min(arr_calc, True) if arr_calc else None,
                    'other_calcs': [_calc_min(c, False) for c in pend if c is not arr_calc],
                    'queue': _queue_min(queue),
                    # 报文侧的两个"必须逐位相等"（docs/06 §25.1）：`end_x` 与 `pos_x`
                    # 按 §27/§40.5/§43 是**同一次读**（`pts[n-1]`），这里显式记一份
                    'end_x': pos.get('pos_x'),
                    'end_x_f': pos.get('pos_xf'),
                })
                pend, queue, marker = [], None, None
        for s in shots:
            rr = rep.get(s['shot'])
            if rr:
                s['repro'] = {k: rr.get(k) for k in
                              ('mode_used', 'n_ours', 'same_len', 'same_bytes', 'diff_points',
                               'first_diff_index', 'verdict') if rr.get(k) is not None}
        all_shots += shots
        sources.append({'label': label, 'raw_jsonl_sha256': sha, 'raw_jsonl_lines': len(rows),
                        'records_by_type': by_type, 'shots': len(shots)})

    modes = sorted({(s.get('report_array_calc') or {}).get('mode') for s in all_shots} - {None})
    reps = [s for s in all_shots if s.get('repro')]
    repro_same = [s for s in reps if s['repro'].get('same_bytes') is True]
    n_ok = [s for s in all_shots if (s.get('pos_read') or {}).get('n') is not None]
    # 两条与实现无关的护栏（docs/06 §25.1）：报文读的点 = 数组最后一点；pos_x = 最后一点的 x
    pos_is_last = [s for s in all_shots
                   if (s.get('pos_read') or {}).get('pos_bytes') ==
                   ((s.get('report_array_calc') or {}).get('arr') or {}).get('last')]
    ptr_named = [s for s in all_shots
                 if any(w.get('same_ptr') and w.get('same_len')
                        for w in ((s.get('pos_read') or {}).get('which_calc') or []))]

    stops = [(s.get('report_array_calc') or {}).get('stop') or {} for s in all_shots]
    kinds = {}
    for st in stops:
        k = st.get('kind') or 'unknown'
        kinds[k] = kinds.get(k, 0) + 1
    budget_ok = [st for st in stops if st.get('kind') == 'Budget' and st.get('count_eq_budget') is True]
    count_ok = [st for st in stops if st.get('count_plus1_eq_n') is True]
    conflicts = [s for s in all_shots if (s.get('report_array_calc') or {}).get('stop_conflict')]
    prov = {
        'type': 'provenance', 'schema': FIXTURE_SCHEMA,
        'what': ('runtime probe of a **DiffEq (Mode::Ode)** match **with the five stop exits judged at run '
                 'time**: per shot, the sampler arguments (mode/flip/x0/y0/step + the token stream actually '
                 'fed in), the returned point array (ptr, n, digest, first/last point), **which of the five '
                 'stop exits fired** (breakpoint on the deciding instruction of each), and the point the '
                 'report reads (`pos = pts[n-1]` at execute_results 0x472865).'),
        'stop_judgement': {
            'why': ('dispatch/40 could only infer the stop reason from OUR OWN trace; 2 of the 5 exits '
                    '(budget exhausted / 100 bisections without convergence) were never observed. This probe '
                    'judges them from two **non-branch** breakpoints plus the sampler frame, so `stop.kind` '
                    'is a runtime fact.'),
            'judged_at': {
                'OverX_OverY_NaN': ('gw+0x3c2840 = accept-path entry (entered only when 0.001 > |chord-step|, '
                                    'once per accepted point). Inside, the binary checks in order '
                                    '`ucomisd px,cand_y`+jp (NaN) -> `|mid|>25` -> `|cand_y|>15`; the handler '
                                    're-evaluates the same three predicates from the frame slots the binary '
                                    'loads into xmm0/xmm1 (mid@rsp+0x80, cand_y@rsp+0x50).'),
                'Budget_NoConverge': ('gw+0x3c2b5d = shared epilogue (every exit and the normal end jump here, '
                                      'exactly once per call). count == budget (r12d == r13d, both read at the '
                                      'epilogue) => Budget; count < budget and no accept-path exit was recorded '
                                      '=> NoConverge (the bisection used up all 100 iterations).'),
            },
            'not_hooked': [
                {'rva': '0x3c2835', 'instr': 'jmp 0x3c2b5d (hi=mid branch exhausted ebx=100)',
                 'why': 'hooking an unconditional jmp makes it fire on EVERY call (even n=1), see docs/06 29.7'},
                {'rva': '0x3c2679', 'instr': 'je 0x3c2b5d (lo=mid branch exhausted)',
                 'why': 'same class of instruction; also too hot (per iteration)'},
                {'rva': '0x3c291e', 'instr': 'jmp 0x3c2b5d (count == budget)',
                 'why': 'same class of instruction; the epilogue rule covers it'},
            ],
            'exits': [
                {'kind': 'OverX', 'rva': '0x3c28a0', 'instr': 'ja 0x3c2b5d (ucomisd 25.0,|mid|)',
                 'hooked': 're-evaluated at 0x3c2840'},
                {'kind': 'OverY', 'rva': '0x3c28bc', 'instr': 'ja 0x3c2b5d (ucomisd 15.0,|cand_y|)',
                 'hooked': 're-evaluated at 0x3c2840'},
                {'kind': 'NaN', 'rva': '0x3c2881', 'instr': 'jp 0x3c2b5d (ucomisd xmm1,xmm0)',
                 'hooked': 're-evaluated at 0x3c2840', 'note': 'UNREACHABLE in Mode::Ode: entering the '
                          'accept path already requires a finite |chord-step|, and hypot(h, cand_y-y) is NaN '
                          'whenever cand_y is NaN (docs/06 29.6). 0 hits in every run.'},
                {'kind': 'NoConverge', 'rva': '0x3c2835 / 0x3c2679', 'instr': 'jmp/je 0x3c2b5d',
                 'hooked': 'judged at 0x3c2b5d (count < budget, no accept-path exit)'},
                {'kind': 'Budget', 'rva': '0x3c291e', 'instr': 'jmp 0x3c2b5d after cmp r12d,r13d',
                 'hooked': 'judged at 0x3c2b5d (count == budget)'},
            ],
            'cross_checks': [
                'stop.count + 1 == n (points accepted, first point excluded)',
                'Budget: stop.count == stop.budget (read from r12d/r13d at the epilogue)',
                'probe self-test (stopSelfTest at install) feeds 14 synthetic frame/register sets through the '
                'SAME classifier: each exit is recognised and the near misses (exactly 25.0 / exactly 15.0 / '
                'NaN vs `ja` / count != budget) are rejected',
                'arrays are re-integrated by examples/ode_array_check: bitwise identical, same length',
            ],
        },
        'how_to_make_an_ode_match': {
            'path': ('campaign: user://campaign.save = "level_20" (unlock all 21) -> Menu "Campaign" -> '
                     'level node #16 (691,751) -> popup "Start" (1072,1009). Campaign level 16 has '
                     'func=1 (DiffEq) per docs/10-campaign.md 4 (mode word 0x05030001, bit-exact).'),
            'ui_check': ('the JoinLevelPopUp shows the `dy/dx =` icon and the play screen prompt reads '
                         '`dy/dx =` instead of `f(x) =` (screenshots, not committed)'),
        },
        'build': {
            'game': 'Graphwar II Linux 2.0.60 (Steam)',
            'libgraphwarrust.so_sha256': '9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187',
            'engine': 'Godot 4.7.stable.official + godot-rust (API v4.6, runtime v4.7)',
            'address_space': 'RVA = ELF virtual address = module offset; Ghidra address = RVA + 0x100000',
            'hooked_rva': {'sampler_calculate': '0x3c2530', 'evaluate': '0x4eb9d0',
                           'execute_results': '0x472470', 'exec_queue_probe': '0x4724cc',
                           'pos_read_movdqu': '0x472865', 'slot_9bac98': '0x9bac98',
                           'ode_exits': {'no_converge_hi': '0x3c2835', 'no_converge_lo': '0x3c2679',
                                         'nan': '0x3c2881', 'over_x': '0x3c28a0', 'over_y': '0x3c28bc',
                                         'budget': '0x3c291e', 'epilogue': '0x3c2b5d'}},
        },
        'how': {
            'harness': 'Xvfb :92 -screen 0 1920x1160x24 -listen tcp -ac (+ mesa-only EGL/GLX env)',
            'driver': 'tools/frida_ode_edge.py --attach (NOT frida spawn: spawn hangs the game in wait4)',
            'probe': 'tools/frida_ode_edge.js (read-only Interceptor hooks; no memory writes)',
            'sampler_abi': ('0x3c2530 (out: Vec<(f64,f64)> = rdi, tokens: &TokenBuf{cap,ptr,len} = rsi, '
                            'flip = edx, MODE = ecx [0=Normal, 1=Ode], x0 = xmm0, y0 = xmm1, step = xmm2). '
                            'Read off the prologue (3c2541/3c2544/3c254a/3c254c/3c254f/3c2555/3c255a) and '
                            'the 0x3c25f5 `test %r15b,%r15b` split into two bisection loops.'),
            'digest': 'FNV-1a 64 over ALL 16*n bytes (`digest_xy`) and over the y halves only (`digest_y`).',
            'positive_control': ('probe self-test at install: two 100-point synthetic arrays -> same; flip 1 bit '
                                 'at point 37 -> diffPoints=1; flip one more at point 90 -> 2. Also runtime: '
                                 'different (text, constant) gives different arrays.'),
            'mode_evidence': ('(1) `ecx` at the sampler entry reads 1 for every shot; (2) the evaluator receives '
                              'many DISTINCT y arguments (Normal passes y0 once, then 0); (3) the UI shows '
                              '`dy/dx =`.'),
        },
        'cut': ('per shot: the expression typed (host marker), mode/flip/x0/y0/step, the token stream fed to '
                'the sampler ({tag, value-bits}), the array the report reads (ptr, n, digest, first/last/'
                'second-last point) **with the stop exit that fired**, the other sampler calls of the same '
                'shot, the execute_results queue snapshot, and the 16 bytes read by the `movdqu` '
                '(pos_x/pos_y).'),
        'dropped': 'beat rows, whole-array hex, and every evaluator sample except the first few.',
        'no_local_paths': 'no filesystem path and no user name; pointers are ASLR-random per run.',
        'sources': sources,
        'probe_selftest_at_install': {'array_compare': (selftest or {}).get('cmp'),
                                      'stop_classifier': (selftest or {}).get('stop')},
        'hooks': hooks,
        'counts': {'shots': len(all_shots), 'distinct_modes': modes,
                   'shots_with_repro': len(reps), 'repro_bitwise_identical': len(repro_same),
                   'pos_read_with_n': len(n_ok),
                   'pos_bytes_equal_array_last_point': len(pos_is_last),
                   'pos_ptr_is_one_of_our_calc_calls': len(ptr_named),
                   'stop_kinds': kinds,
                   'budget_exits_with_count_eq_budget': len(budget_ok),
                   'stop_count_plus1_eq_n': len(count_ok),
                   'stop_conflicts': len(conflicts)},
        'notes': notes or [],
    }
    with open(out_path, 'w', encoding='utf-8') as f:
        f.write(json.dumps(prov, ensure_ascii=False) + '\n')
        for s in all_shots:
            f.write(json.dumps(s, ensure_ascii=False) + '\n')
    return prov, all_shots


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', default=None, help='JSONL 输出路径（`--extract` 模式下不需要）')
    ap.add_argument('--display', default=os.environ.get('DISPLAY', '127.0.0.1:92'))
    ap.add_argument('--data-dir', default=None, help='XDG 根（默认 .agent-work/cg/wt-45/.run/gamedata）')
    ap.add_argument('--game', default=GAME)
    ap.add_argument('--js', default=JS)
    ap.add_argument('--attach', action='store_true', help='★ 本机必须：Popen 后 attach（frida spawn 挂死）')
    ap.add_argument('--campaign-level', type=int, default=20,
                    help='开跑前往 user://campaign.save 写 `level_<n>`（20 = 全部解锁）')
    ap.add_argument('--no-save', action='store_true', help='不写 campaign.save（存档已就绪时用）')
    ap.add_argument('--level-node', default='16', help='要点的关卡号（1..20；按内置坐标表）或 "x,y"')
    ap.add_argument('--settle', type=float, default=20.0,
                    help='启动后等多久再点菜单（之后还会等**画面画出来**，见 wait_drawn）')
    ap.add_argument('--shots', type=int, default=6)
    ap.add_argument('--exprs', default='y', help='逗号分隔的 dy/dx 表达式，逐发轮换')
    ap.add_argument('--gap', type=float, default=9.0, help='两发之间等多久（让弹道走完）')
    ap.add_argument('--tail', type=float, default=22.0, help='最后一发之后再录多久')
    ap.add_argument('--stop-exact', action='store_true',
                    help='停因断点用**精确模式**（挂在三条条件码分支自己身上；慢 3 倍，交叉验证用）')
    ap.add_argument('--full-array', action='store_true',
                    help='把整条点数组 hex 也发回来（离线逐位复算用；**不要**写进入库样本）')
    ap.add_argument('--snap-dir', default=None, help='（调试）每步按窗口 id 截图；PNG 不入库')
    ap.add_argument('--no-clicks', action='store_true', help='只挂探针，自己点')
    ap.add_argument('--extract', default=None, action='append', help='只摘样本（可给多次）')
    ap.add_argument('--fixture-out', default=None)
    ap.add_argument('--label', default=None, action='append')
    ap.add_argument('--note', default=None, action='append')
    ap.add_argument('--repro', default=None, help='`examples/ode_array_check` 的 --json 结果（并进入库样本）')
    args = ap.parse_args()

    if args.extract:
        if not args.fixture_out:
            ap.error('--extract 需要 --fixture-out')
        labels = args.label or []
        specs = [(p, labels[i] if i < len(labels) else 's%d' % (i + 1)) for i, p in enumerate(args.extract)]
        prov, shots = extract(specs, args.fixture_out, notes=args.note, repro=args.repro)
        print('[extract] shots=%d modes=%s repro=%d/%d -> %s' % (
            prov['counts']['shots'], prov['counts']['distinct_modes'],
            prov['counts']['repro_bitwise_identical'], prov['counts']['shots_with_repro'],
            args.fixture_out), file=sys.stderr)
        return
    if not args.out:
        ap.error('需要 --out（或用 --extract 模式）')

    import frida

    data = args.data_dir or os.path.join(REPO, '.agent-work', 'cg', 'wt-45', '.run', 'gamedata')
    for k in ('data', 'cache', 'config'):
        os.makedirs(os.path.join(data, k), exist_ok=True)
    if not args.no_save:
        write_campaign_save(data, args.campaign_level)

    env = {
        'DISPLAY': args.display,
        'XDG_DATA_HOME': os.path.join(data, 'data'),
        'XDG_CACHE_HOME': os.path.join(data, 'cache'),
        'XDG_CONFIG_HOME': os.path.join(data, 'config'),
        '__EGL_VENDOR_LIBRARY_FILENAMES': '/usr/share/glvnd/egl_vendor.d/50_mesa.json',
        'LIBGLX_VENDOR_LIBRARY_NAME': 'mesa',
        'LIBGL_ALWAYS_SOFTWARE': '1',
        'LC_CTYPE': 'C.UTF-8',
    }

    src = open(args.js, encoding='utf-8').read()
    out = open(args.out, 'w', encoding='utf-8')
    tally = {}

    def on_message(msg, _data):
        if msg.get('type') == 'error':
            print('[js-error]', msg.get('description'), file=sys.stderr)
            out.write(json.dumps({'type': 'js_error', 'msg': msg}) + '\n')
            return
        p = msg.get('payload') or {}
        t = p.get('type', '?')
        tally[t] = tally.get(t, 0) + 1
        out.write(json.dumps(p) + '\n')
        out.flush()
        if t == 'hooks':
            print('[hooks]', json.dumps(p, ensure_ascii=False)[:400], file=sys.stderr)
        elif t == 'selftest':
            print('[selftest]', json.dumps(p.get('cmp'), ensure_ascii=False), file=sys.stderr)
        elif t == 'calc':
            a = p.get('arr') or {}
            print('[calc] seq=%s ret=%s mode=%s flip=%s x0=%s y0=%s step=%s tok=%s -> ptr=%s n=%s' % (
                p.get('seq'), p.get('ret'), p.get('mode'), p.get('flip'), p.get('x0f'), p.get('y0f'),
                p.get('stepf'), ((p.get('tokens') or {}).get('len')), a.get('ptr'), a.get('n')), file=sys.stderr)
        elif t == 'pos_read':
            print('[pos_read] #%s rdx=%s n=%s pos_x=%s pos_y=%s which=%s' % (
                p.get('seq'), p.get('rdx'), p.get('n'), p.get('pos_xf'), p.get('pos_yf'),
                [(w.get('seq'), w.get('ret'), w.get('same_ptr'), w.get('same_len')) for w in (p.get('which_calc') or [])]),
                file=sys.stderr)
        elif t == 'exec_queue':
            print('[exec_queue] qlen=%s items=%s evals=%s' % (
                p.get('qlen'), [(i.get('lenOff'), i.get('len'), i.get('ptr')) for i in (p.get('items') or [])],
                len(p.get('eval_sample') or [])), file=sys.stderr)
        elif t == 'cfg_ack':
            print('[cfg_ack]', json.dumps(p.get('cfg'), ensure_ascii=False), file=sys.stderr)

    dev = frida.get_local_device()
    # ★ 本机宿主的 `WAYLAND_DISPLAY` 会让 Godot 连**真桌面**（niri）而不是我们的 Xvfb：
    #   现象是 `window=root`（xdotool 找不到窗口）+ 日志里出现 `DisplayServerWayland`，
    #   于是后面每一次点击都点在空处、0 发。⇒ 显式清掉 wayland，并钉死 `--display-driver x11`。
    argv = [os.path.join(args.game, 'Graphwar.x86_64'), '--resolution', '1920x1160',
            '--position', '0,0', '--display-driver', 'x11']
    env['XDG_SESSION_TYPE'] = 'x11'
    env['GDK_BACKEND'] = 'x11'
    full_env = {k: v for k, v in dict(os.environ, **env).items()
                if k not in ('LC_ALL', 'WAYLAND_DISPLAY', 'WAYLAND_SOCKET')}
    if args.attach:
        print('[*] popen', argv, file=sys.stderr)
        proc = subprocess.Popen(argv, env=full_env, cwd=args.game)
        pid = proc.pid
        t_wait = time.time()
        while time.time() - t_wait < 40:
            if _win_id(args.display) != 'root':
                break
            time.sleep(0.5)
        print('[*] pid=%s window=%s after %.1fs' % (pid, _win_id(args.display), time.time() - t_wait),
              file=sys.stderr)
    else:
        pid = dev.spawn(argv, env=full_env, cwd=args.game)
        print('[*] spawn pid', pid, file=sys.stderr)
    session = dev.attach(pid)
    script = session.create_script(src, runtime='v8')
    script.on('message', on_message)
    script.load()
    try:
        script.post({'type': 'cfg', 'fullArray': 1 if args.full_array else 0,
                     'exactExits': 1 if args.stop_exact else 0})
    except Exception as e:  # noqa: BLE001
        print('[warn] post cfg:', e, file=sys.stderr)
    if not args.attach:
        dev.resume(pid)

    t0 = time.time()
    try:
        if not args.no_clicks:
            time.sleep(args.settle)
            wait_drawn(args.display)
            snap(args.display, args.snap_dir, '00_boot')
            # ① Campaign
            xdo(args.display, NAV_MAIN[1]); time.sleep(NAV_MAIN[2]); xdo(args.display, NAV_MAIN[3])
            time.sleep(2.5)
            snap(args.display, args.snap_dir, '01_campaign')
            # ② 关卡节点
            if ',' in args.level_node:
                xy = args.level_node
            else:
                n = int(args.level_node)
                xy = '%d %d' % LEVEL_NODE[n]
            print('[nav] level node = %s' % xy, file=sys.stderr)
            xdo(args.display, 'mousemove ' + xy); time.sleep(0.4); xdo(args.display, 'click 1')
            time.sleep(2.5)
            snap(args.display, args.snap_dir, '02_popup')
            # ③ Start
            xdo(args.display, 'mousemove 1072 1009'); time.sleep(0.4); xdo(args.display, 'click 1')
            time.sleep(4.0)
            snap(args.display, args.snap_dir, '03_play')

            exprs = [e.strip() for e in args.exprs.split(',') if e.strip()] or ['y']
            for s in range(max(1, args.shots)):
                expr = exprs[s % len(exprs)]
                print('[shot %d/%d] dy/dx = %s' % (s + 1, args.shots, expr), file=sys.stderr)
                # ★ 打字**之前**把"这一发是哪个式子"写进同一条 JSONL 流 ⇒ 样本带得出表达式。
                #   （不走 `script.post`：frida 的 post→recv 在这台机器上只送到第一条，
                #    原因未查；驱动自己写则与探针记录**同序**，摘样本时按顺序配对。）
                out.write(json.dumps({'type': 'marker', 'shot': s + 1, 'text': expr,
                                      'level': args.level_node}) + '\n')
                out.flush()
                pre, fire = shot_actions(expr)
                for delay, action in pre:
                    time.sleep(delay)
                    xdo(args.display, action)
                snap(args.display, args.snap_dir, 'typed%02d' % (s + 1))
                for delay, action in fire:
                    time.sleep(delay)
                    xdo(args.display, action)
                snap(args.display, args.snap_dir, 'shot%02d' % (s + 1))
                if s + 1 < args.shots:
                    time.sleep(args.gap)
            time.sleep(args.tail)
        else:
            time.sleep(args.tail)
    finally:
        try:
            time.sleep(1.0)
            dev.kill(pid)
        except Exception as e:  # noqa: BLE001
            print('[warn] kill:', e, file=sys.stderr)
        if args.attach:
            try:
                proc.terminate()
            except Exception:  # noqa: BLE001
                pass
        out.close()
    print('[done] messages by type:', json.dumps(tally), file=sys.stderr)


if __name__ == '__main__':
    main()
