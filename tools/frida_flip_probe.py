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
JS = os.path.join(HERE, 'frida_flip_probe.js')

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

FIXTURE_SCHEMA = 'gw2-frida-flip-sample/1'


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

    判据（写在这里 = 别人拿原始 JSONL 能重放出同一个文件）：

      1. **`same_ptr` 必须成立** —— 报文那 16 字节就是从这块缓冲里读的（探针按指针点名）；
      2. 再优先 `same_len` 且摘要相同（没被截断的那一发）；
      3. 退而求其次 `same_len`；**最后**才是"指针相同、长度不同" ——
         那是 `docs/06` §29.5 观测到的**障碍截断**：同一条缓冲、`Vec.len` 被原地改小。
         这种发在样本里带 `arr_truncated`，别把它当成"我方复算对不上"。

    ⚠ `same_ptr` 单独不够还有另一个原因：同一条堆缓冲会被后一发复用（实测同一局里
      第 3、4 发的 `rdx` 完全相同、长度不同），所以 `same_len` / 摘要要做二次确认。
    """
    which = pos.get('which_calc') or []
    cands = [w for w in which if w.get('same_ptr')]
    cands.sort(key=lambda w: (w.get('seq') or 0))

    def calc_of(w):
        return next((x for x in pend if x.get('seq') == w.get('seq')), None)

    for w in reversed(cands):
        c = calc_of(w)
        if c is not None and w.get('same_len') and (c.get('arr') or {}).get('digest_xy') == pos.get('digest_xy'):
            return c, 'seq+ptr+len+digest'
    for w in reversed(cands):
        c = calc_of(w)
        if c is not None and w.get('same_len'):
            return c, 'seq+ptr+len(digest differs)'
    for w in reversed(cands):
        c = calc_of(w)
        if c is not None:
            return c, 'seq+ptr(len truncated, see arr_truncated)'
    # 完全没有 same_ptr：只可能是探针漏了那条 calc，记录用
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


# ═══════════════════════════════════════════════════════════════════════════════
# 回合感知用的运行时状态（**从探针自己的消息**里数，不靠固定延时）
#   `pos`  = 已收到的 `pos_read` 条数（= 已结算的发数，我方与 Bot 各算一发）
#   `flip0`/`flip1` = 已收到的采样器调用里 `flip=0` / `flip=1` 的条数
#   ⇒ "我方这一发有没有真的进去"看 `flip0` 涨没涨；"对手那一发打完了吗"看 `pos` 涨没涨。
# ═══════════════════════════════════════════════════════════════════════════════
STT = {'pos': 0, 'flip0': 0, 'flip1': 0, 'last_calc': 0.0, 't0': 0.0}


def wait_until(pred, timeout, step=0.25):
    """等到 `pred()` 为真；超时返回 False。只轮询**同一条消息流**里数出来的计数器。"""
    t = time.time()
    while time.time() - t < timeout:
        try:
            if pred():
                return True
        except Exception:  # noqa: BLE001
            pass
        time.sleep(step)
    return False


def parse_raw(path):
    rows = []
    with open(path, encoding='utf-8') as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows

def _cand_min(cs):
    """进化器评的候选（`step = 0.5`）：只留条数 / 序号区间 / flip 集合 / 最长点数。"""
    if not cs:
        return None
    return {'n': len(cs),
            'seq_first': min(c.get('seq') or 0 for c in cs),
            'seq_last': max(c.get('seq') or 0 for c in cs),
            'flips': sorted({c.get('flip') for c in cs} - {None}),
            'step': sorted({c.get('stepf') for c in cs} - {None}),
            'max_points': max(((c.get('arr') or {}).get('n') or 0) for c in cs)}


def _trunc_min(calc, pos):
    """"采样器给的 n" 与 "报文那一刻的 n" 不同 ⇒ 中间有人原地改了 `Vec.len`（§29.5）。"""
    if not calc:
        return None
    an = (calc.get('arr') or {}).get('n')
    pn = pos.get('n')
    if an is None or pn is None or an == pn:
        return None
    return {'calc_n': an, 'report_n': pn, 'same_ptr': True,
            'note': 'truncated after the sampler returned (docs/06 29.5)'}


def _ctor_min(c):
    if not c:
        return None
    o = {k: c.get(k) for k in ('seq', 't', 'caller', 'owner_id', 'vec', 'start_pt', 'end_pt',
                               'end_x', 'end_xf', 'digest_xy', 'vs_calc', 'skipped', 'error')
         if c.get(k) is not None}
    return o


def _ser_min(s):
    if not s:
        return None
    return {k: s.get(k) for k in ('seq', 't', 'ret', 'start_x', 'start_y', 'start_xf', 'start_yf',
                                  'end_x', 'end_xf', 'owner_id', 'error') if s.get(k) is not None}


def _dir_of(calc):
    """点数组的**走向**：首点 x 与末点 x 的位串 → 递增 / 递减 / 未定。

    这是本轮的"可指认差异"之一：`flip=0` 递增、`flip=1` 递减。用 hex 位串比较
    （`f64of` 只给人看，判等一律位串），并且首点就是采样器入参里的 `x0`。
    """
    a = (calc or {}).get('arr') or {}
    fx, lx = a.get('first'), a.get('last')
    if not fx or not lx:
        return None
    return _dir_of_hex(fx[:16], lx[:16])


def _dir_of_hex(fx, lx):
    if not fx or not lx:
        return None
    import struct
    try:
        f = struct.unpack('<d', bytes.fromhex(fx))[0]
        l = struct.unpack('<d', bytes.fromhex(lx))[0]
    except Exception:  # noqa: BLE001
        return None
    if f == l:
        return 'flat'
    return 'x_increasing' if l > f else 'x_decreasing'


def _xsign(hexx):
    """x0 的符号（只看最高位那一个 bit：位串第 15 个字节 ≥ 0x80 就是负）。"""
    if not hexx or len(hexx) < 16:
        return None
    try:
        return -1 if int(hexx[14:16], 16) >= 0x80 else 1
    except Exception:  # noqa: BLE001
        return None


def extract(specs, out_path, notes=None, repro=None):
    """`specs` = [(jsonl, label), …]；一"发"= 一个 `pos_read` + 它前面最近的那批 `calc`。

    配对规则（写死在这里 = 别人拿原始 JSONL 能一条命令重放出同一个文件）：

      * `pos_read`（`execute_results` 的 `0x472865`）出现 ⇒ 收一发；这一发的 `calc` 是
        **自上一个 `pos_read` 以来**的所有采样器调用（我方一发 / Bot 一发都会各来一次）；
      * `pos_read.which_calc` 里 `same_ptr=true` 的那条 = **报文实际读的那一块**（按指针点名）；
      * `fnpoints_ctor`（`0x456810`）与 `fnpoints`（`0x50eb20`）同样收在这一发的窗口里
        ⇒ 报文里的 `owner_id` / `start_x` / `end_x` 与"哪一次 `calc` 的数组"能对上；
      * `expression` 用**文件顺序 + 消费一次**配：一条 `marker` 只配给**它后面的第一个** `pos_read`；
        之后到下一个 `marker` 之间的 `pos_read` 没有 `expression`（= Bot 那一发，宿主没打字）。
        ⚠ 不能按"最近一条 marker"配：Bot 的 `pos_read` 就在我方那条 marker 后面几十秒，
        那样会把 Bot 那一发也记成我方打的式子（2026-11-08 实测，见 docs/06 §31.5）。
    """
    all_shots, sources = [], []
    rep = {}
    if repro:
        # `--repro` 是 `examples/ode_array_check --json` 的输出：每行一条 `calc`
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
        ctors, sers, markers = [], [], []
        mk_pending = None
        for r in rows:
            t = r.get('type')
            if t == 'calc':
                pend.append(r)
            elif t == 'fnpoints_ctor':
                ctors.append(r)
            elif t == 'fnpoints':
                sers.append(r)
            elif t == 'marker':
                # ★ marker 由**驱动自己**写进同一条流（探针那条 `post` 在这台机器上不送达），
                #   所以它没有探针时基的 `t`。配对规则改成"**文件顺序 + 消费一次**"：
                #   一条 marker 只配给**它后面的第一个** `pos_read`；之后的 `pos_read`
                #   （= Bot 那一发，宿主没有打字）`expression` 为 null。
                mk_pending = {'shot': r.get('shot'), 'text': r.get('text'), 'level': r.get('level'),
                              't': r.get('t')}
                markers.append(mk_pending)
            elif t == 'exec_queue':
                queue = r
            elif t == 'pos_read':
                pos = r
                arr_calc, picked = _pick_calc(pos, pend)
                mk = mk_pending
                mk_pending = None
                # 这一发窗口里的 `FunctionPoints`：**两个**装配点各取最近一条
                #   A = `0x471a88`（喂开火那一刻的副本，报文 `end_x` 出自它，44 号 §28.2）
                #   B = `0x3e44d4`（target_manager 自己那条数组）
                ctor_a = ctor_b = None
                for c in ctors:
                    if (c.get('t') or 0) <= (pos.get('t') or 0):
                        if '471a8' in (c.get('caller') or ''):
                            ctor_a = c
                        else:
                            ctor_b = c
                ctor = ctor_a or ctor_b
                ser = None
                for s2 in sers:
                    if (s2.get('t') or 0) <= (pos.get('t') or 0):
                        ser = s2
                shots.append({
                    'type': 'shot', 'series': label, 'shot': len(shots),
                    'expression': (mk or {}).get('text'),
                    'level': (mk or {}).get('level'),
                    'marker_shot': (mk or {}).get('shot'),
                    'pos_read': _pos_min(pos),
                    'report_array_pick': picked,
                    'report_array_calc': _calc_min(arr_calc, True) if arr_calc else None,
                    # 同一发里别的**真发**调用（`step = 0.01`，通常只有 `gw+3e2d3b` 那一次）；
                    # 进化器评候选（`step = 0.5`，Bot 关里一发能有几百条）只留计数与序号区间，
                    # 否则入库样本要涨到几百 KB 而信息量为零。
                    'other_calcs': [_calc_min(c, False) for c in pend
                                    if c is not arr_calc and (c.get('stepf') or 0) <= 0.009],
                    'candidate_calls': _cand_min([c for c in pend if (c.get('stepf') or 0) > 0.009]),
                    'queue': _queue_min(queue),
                    'fnpoints_ctor': _ctor_min(ctor),
                    'fnpoints_ctor_b': _ctor_min(ctor_b) if ctor_b is not ctor else None,
                    'fnpoints_wire': _ser_min(ser),
                    # 报文侧的两个字段：`end_x` 取**写入器**那一刻的原文（没有就退回装配处），
                    # `pos_x` 取 `0x472865` 那次 `movdqu` 的 16 字节
                    'end_x': (ser or {}).get('end_x') or (ctor or {}).get('end_x'),
                    'end_x_f': (ser or {}).get('end_xf'),
                    'pos_x': pos.get('pos_x'), 'pos_x_f': pos.get('pos_xf'),
                    'flip': (arr_calc or {}).get('flip'),
                    'mode': (arr_calc or {}).get('mode'),
                    'x0': (arr_calc or {}).get('x0'), 'x0_f': (arr_calc or {}).get('x0f'),
                    'y0': (arr_calc or {}).get('y0'), 'y0_f': (arr_calc or {}).get('y0f'),
                    'x0_sign': _xsign((arr_calc or {}).get('x0')),
                    'arr_dir': _dir_of(arr_calc),
                    'arr_truncated': _trunc_min(arr_calc, pos),
                })
                pend, queue = [], None
        for s in shots:
            rr = rep.get(s['shot'])
            if rr:
                s['repro'] = {k: rr.get(k) for k in
                              ('mode_used', 'n_ours', 'same_len', 'same_bytes', 'diff_points',
                               'first_diff_index', 'verdict') if rr.get(k) is not None}
        all_shots += shots
        sources.append({'label': label, 'raw_jsonl_sha256': sha, 'raw_jsonl_lines': len(rows),
                        'records_by_type': by_type, 'shots': len(shots)})

    modes = sorted({s.get('mode') for s in all_shots} - {None})
    flips = sorted({s.get('flip') for s in all_shots} - {None})
    by_flip = {}
    for s in all_shots:
        k = str(s.get('flip'))
        by_flip[k] = by_flip.get(k, 0) + 1
    by_dir = {}
    for s in all_shots:
        k = '%s|flip=%s' % (s.get('arr_dir'), s.get('flip'))
        by_dir[k] = by_dir.get(k, 0) + 1
    n_ok = [s for s in all_shots if (s.get('pos_read') or {}).get('n') is not None]
    pos_is_last = [s for s in all_shots
                   if (s.get('pos_read') or {}).get('pos_bytes') ==
                   ((s.get('report_array_calc') or {}).get('arr') or {}).get('last')]
    ptr_named = [s for s in all_shots
                 if any(w.get('same_ptr') and w.get('same_len')
                        for w in ((s.get('pos_read') or {}).get('which_calc') or []))]
    # ★ 本轮的三个判据（写在这里 = 判据不写在报告里）
    flip1 = [s for s in all_shots if s.get('flip') == 1]
    flip0 = [s for s in all_shots if s.get('flip') == 0]
    dir_ok = [s for s in flip1 if s.get('arr_dir') == 'x_decreasing']
    dir_ok0 = [s for s in flip0 if s.get('arr_dir') == 'x_increasing']
    side_ok = [s for s in flip1 if s.get('x0_sign') == 1]
    side_ok0 = [s for s in flip0 if s.get('x0_sign') == -1]
    endx_eq_posx = [s for s in all_shots if s.get('end_x') and s.get('pos_x')
                    and s.get('end_x') == s.get('pos_x')]
    start_eq_x0 = [s for s in all_shots
                   if ((s.get('fnpoints_ctor') or {}).get('start_pt') or '')[:16] == (s.get('x0') or '')
                   or ((s.get('fnpoints_wire') or {}).get('start_x') or '') == (s.get('x0') or '')]
    # owner_id 只取**装配处**（`0x456810` 的 `esi`，u32）；写入器那 8 字节的高 4 位是脏的
    # （44 号 §28.5 第 3 条的同一件事），别拿它当 id。
    owners = sorted({(s.get('fnpoints_ctor') or {}).get('owner_id') for s in all_shots} - {None})

    prov = {
        'type': 'provenance', 'schema': FIXTURE_SCHEMA,
        'what': ('runtime probe of **flip=1 shots and multi-soldier / Bot campaign levels (18-20)**: per shot, '
                 'the sampler arguments (**flip**, mode, x0/y0/step + the token stream fed in), the returned '
                 'point array (ptr, n, digest, **first/last point -> sampling direction**), the '
                 '`FunctionPointsEvent` assembly at 0x456810 (owner_id / start = pts[0] / end_x = pts[len-1].x) '
                 'and the JSON writer 0x50eb20, and the 16 bytes the report reads (pos = pts[n-1] at '
                 'execute_results 0x472865).'),
        'why_this_gap': ('every runtime sample before this round was Level 16/17 (single soldier, our side, '
                         'flip=0). flip=1 (Team2 shooting towards -x) had ZERO samples, and Levels 18-20 '
                         '(3/4/5 soldiers + a Bot) had none either.'),
        'how_to_make_flip1': {
            'summary': ('flip comes from the **team** (graphwar_turn_manager::Team::flip: Team1 x<=0 -> flip=0, '
                        'Team2 x>=0 -> flip=1; docs/archive/105-team-sides.md). In campaign the player is '
                        'always Team1, so our own shots are flip=0. **The Bot is Team2** => the shot the Bot '
                        'fires right after ours is the flip=1 sample: enter a campaign level that HAS an '
                        'opponent and just wait for its turn.'),
            'levels_with_a_bot': ('campaign level index -> docs/10-campaign.md 4: L5(bot_0), L9(bot_2), '
                                  'L11(bot_3), L14(bot_3), L15(bot_4), L18(bot_3), L19(bot_3), L20(bot_4)'),
            'steps': [
                'user://campaign.save = "level_20" (unlock all 21 levels)',
                'main menu "Campaign" (534,629)',
                'level node #18 / #19 / #20 (1187,751) / (1429,751) / (1672,751)  [3 rows x 7 cols, snake numbering]',
                'popup "Start" (1072,1009)',
                'click the input box (1000,1090) -> ctrl+a -> type the expression -> long-press Fire! (1855,55) 2 s',
                'do NOT type the next expression immediately: the Bot takes its turn now, and ITS shot is '
                'the flip=1 sample (x0 > 0, points run towards -x, owner_id = the Bot soldier)',
            ],
            'ui_check': 'in Levels 18-20 the prompt reads `dy/dx =` (func=1, DiffEq); the Bot fires by itself.',
        },
        'build': {
            'game': 'Graphwar II Linux 2.0.60 (Steam)',
            'libgraphwarrust.so_sha256': '9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187',
            'engine': 'Godot 4.7.stable.official + godot-rust (API v4.6, runtime v4.7)',
            'address_space': 'RVA = ELF virtual address = module offset; Ghidra address = RVA + 0x100000',
            'hooked_rva': {'sampler_calculate': '0x3c2530', 'evaluate': '0x4eb9d0',
                           'execute_results': '0x472470', 'exec_queue_probe': '0x4724cc',
                           'pos_read_movdqu': '0x472865', 'slot_9bac98': '0x9bac98',
                           'fnpoints_ctor': '0x456810', 'fnpoints_ser_json': '0x50eb20'},
        },
        'how': {
            'harness': 'Xvfb :93 -screen 0 1920x1160x24 -listen tcp -ac (+ mesa-only EGL/GLX env)',
            'driver': 'tools/frida_flip_probe.py --attach (NOT frida spawn: spawn hangs the game in wait4)',
            'probe': 'tools/frida_flip_probe.js (read-only Interceptor hooks; no memory writes)',
            'sampler_abi': ('0x3c2530 (out: Vec<(f64,f64)> = rdi, tokens: &TokenBuf{cap,ptr,len} = rsi, '
                            'FLIP = edx, MODE = ecx [0=Normal, 1=Ode], x0 = xmm0, y0 = xmm1, step = xmm2). '
                            'Read off the prologue; 0x3c25a4 `test %bpl,%bpl` is the flip branch.'),
            'fnpoints_abi': ('0x456810: rdi=self, esi=owner_id (u32), rdx=&Vec<(f64,f64)>; the body reads '
                             'pts[len-1].x (0x4569b8) -> end_x and pts[0] (0x4569be) -> start_x/start_y. '
                             '0x50eb20 writes them into the JSON: rsi=&event{start_x@0,start_y@8,end_x@0x10,'
                             'owner_id@0x18}.'),
            'digest': 'FNV-1a 64 over ALL 16*n bytes (`digest_xy`) and over the y halves only (`digest_y`).',
            'positive_control': ('probe self-test at install: two 100-point synthetic arrays -> same; flip 1 bit '
                                 'at point 37 -> diffPoints=1; flip one more at point 90 -> 2. Runtime: flip=0 and '
                                 'flip=1 shots of the same match produce opposite array directions and different '
                                 'digests, so the comparison code does report differences on real data.'),
        },
        'cut': ('per shot: expression typed (host marker + marker_dt_ms), flip/mode/x0/y0/step, the token '
                'stream fed to the sampler ({tag, value-bits}), the array the report reads (ptr, n, digest, '
                'first/last/second-last point), the derived sampling direction (`arr_dir`) and x0 sign, the '
                'other sampler calls of the same shot, the `FunctionPoints` assembly + JSON writer, the '
                'execute_results queue snapshot, and the 16 bytes read by the `movdqu` (pos_x/pos_y).'),
        'dropped': 'beat rows, whole-array hex, and every evaluator sample except the first few.',
        'no_local_paths': 'no filesystem path and no user name; pointers are ASLR-random per run.',
        'sources': sources,
        'probe_selftest_at_install': (selftest or {}).get('cmp'),
        'hooks': hooks,
        'counts': {'shots': len(all_shots), 'distinct_modes': modes, 'distinct_flips': flips,
                   'shots_by_flip': by_flip, 'array_direction_by_flip': by_dir,
                   'pos_read_with_n': len(n_ok),
                   'pos_bytes_equal_array_last_point': len(pos_is_last),
                   'arr_truncated_shots': len([s for s in all_shots if s.get('arr_truncated')]),
                   'pos_ptr_is_one_of_our_calc_calls': len(ptr_named),
                   'flip1_x_decreasing': len(dir_ok), 'flip1_shots': len(flip1),
                   'flip0_x_increasing': len(dir_ok0), 'flip0_shots': len(flip0),
                   'flip1_x0_positive': len(side_ok), 'flip0_x0_negative': len(side_ok0),
                   'wire_end_x_eq_pos_x': len(endx_eq_posx),
                   'wire_start_eq_shooter_x0': len(start_eq_x0),
                   'owner_ids_seen': owners},
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
    ap.add_argument('--gap', type=float, default=3.0, help='两发之间先睡多久（再等回合切换）')
    ap.add_argument('--start-with-opponent', type=int, default=1,
                    help='1 = Start 之后先等**对手**那一发（有 Bot 的关卡它先走；默认开）')
    ap.add_argument('--between-opponent', type=int, default=1,
                    help='1 = 我方每发之后等对手那一发打完（Bot 关默认开）')
    ap.add_argument('--opp-timeout', type=float, default=260.0, help='等对手那一发的上限（秒）')
    ap.add_argument('--turn-detect', type=float, default=25.0,
                    help='Start 之后先看这么久谁先走（对手动了看 `flip=1` 计数）')
    ap.add_argument('--shot-timeout', type=float, default=150.0, help='等我方那一发结算的上限（秒）')
    ap.add_argument('--retry-timeout', type=float, default=18.0, help='等"我方那一发进了采样器"的上限（秒）')
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
        c = prov['counts']
        print('[extract] shots=%d modes=%s flips=%s dir=%s end_x==pos_x %d -> %s' % (
            c['shots'], c['distinct_modes'], c['shots_by_flip'], c['array_direction_by_flip'],
            c['wire_end_x_eq_pos_x'], args.fixture_out), file=sys.stderr)
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
            if p.get('flip') == 0:
                STT['flip0'] += 1
            elif p.get('flip') == 1:
                STT['flip1'] += 1
            STT['last_calc'] = time.time()
            a = p.get('arr') or {}
            print('[calc] seq=%s ret=%s mode=%s flip=%s x0=%s y0=%s step=%s tok=%s -> ptr=%s n=%s' % (
                p.get('seq'), p.get('ret'), p.get('mode'), p.get('flip'), p.get('x0f'), p.get('y0f'),
                p.get('stepf'), ((p.get('tokens') or {}).get('len')), a.get('ptr'), a.get('n')), file=sys.stderr)
        elif t == 'pos_read':
            STT['pos'] += 1
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

            # ★★ 回合感知（本轮新增，2026-11-08 实测的必要条件）
            #   对有对手的关卡（Level 5/9/11/14/15/18/19/20）**Bot 先走一步**：
            #   Start 之后立刻就是它的回合（一局里它先在 `step=0.5` 上评了 204 个候选式子、
            #   30 s 后才用 `step=0.01` 打出真发）。此时我方输入框**是禁用的**：
            #   实测 22 s 后打字 ⇒ 输入框仍空、`Fire!` 点了没反应（截图 `.run/snap/shot01.png`），
            #   整轮只录到 Bot 那一发。⇒ **必须先等对手那一发的 `pos_read`**，再轮到我方。
            #   判据用探针自己发回来的 `pos_read`（= execute_results 读点那一刻），不靠固定延时。
            if args.start_with_opponent:
                # ★ 先**判断这一局谁先走**（不能一律等对手）：
                #   有 Bot 的关卡并不都是 Bot 先走 —— Level 18 实测 Bot 先（Start 后 1 s 内就开始
                #   评式子了），而 Level 19 实测**我方先**（3.5 分钟一条 `calc` 都没有）。
                #   判据是探针自己的 `flip=1` 计数（Bot 属于 Team2 ⇒ 它的每一次采样器调用都是
                #   flip=1）；没有对手活动就说明轮到我方，别再干等。
                print('[turn] detecting whose turn it is (%.0fs window) ...' % args.turn_detect,
                      file=sys.stderr)
                busy = wait_until(lambda: STT['flip1'] > 0 or STT['pos'] >= 1, args.turn_detect)
                if busy:
                    print('[turn] opponent is moving (flip1=%d) -> wait for its shot' % STT['flip1'],
                          file=sys.stderr)
                    got = wait_until(lambda: STT['pos'] >= 1, args.opp_timeout)
                    print('[turn] opponent opening shot: %s (pos_read=%d, %.1fs)' % (
                        'seen' if got else 'TIMEOUT', STT['pos'], time.time() - t0), file=sys.stderr)
                    if got:
                        time.sleep(4.0)          # 弹道动画 + 回合切换
                        snap(args.display, args.snap_dir, '04_our_turn')
                else:
                    print('[turn] no opponent activity in %.0fs -> it is OUR turn first' % args.turn_detect,
                          file=sys.stderr)

            exprs = [e.strip() for e in args.exprs.split(',') if e.strip()] or ['y']
            for s in range(max(1, args.shots)):
                expr = exprs[s % len(exprs)]
                base_pos = STT['pos']
                base_flip0 = STT['flip0']
                print('[shot %d/%d] dy/dx = %s (pos_read=%d)' % (s + 1, args.shots, expr, base_pos),
                      file=sys.stderr)
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
                # 我方那一发有没有真的打进采样器（`flip=0` 的那条 `calc`）？没有就重打一次。
                ok = wait_until(lambda: STT['flip0'] > base_flip0, args.retry_timeout)
                if not ok:
                    print('[shot %d] WARN no flip=0 sampler call within %.0fs -> retype' % (
                        s + 1, args.retry_timeout), file=sys.stderr)
                    snap(args.display, args.snap_dir, 'nofire%02d' % (s + 1))
                    pre, fire = shot_actions(expr)
                    for delay, action in pre:
                        time.sleep(delay)
                        xdo(args.display, action)
                    for delay, action in fire:
                        time.sleep(delay)
                        xdo(args.display, action)
                    wait_until(lambda: STT['flip0'] > base_flip0, args.retry_timeout)
                # 等这一发结算（`pos_read` 出现）
                wait_until(lambda: STT['pos'] > base_pos, args.shot_timeout)
                snap(args.display, args.snap_dir, 'resolved%02d' % (s + 1))
                if s + 1 < args.shots:
                    time.sleep(args.gap)
                    # 对手的回合（我们只等它把那一发打完；它的 `calc` 全在 flip=1 上）
                    if args.between_opponent:
                        got = wait_until(lambda: STT['pos'] > base_pos + 1, args.opp_timeout)
                        print('[shot %d] opponent follow-up: %s (pos_read=%d)' % (
                            s + 1, 'seen' if got else 'none', STT['pos']), file=sys.stderr)
                        time.sleep(3.0)
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
