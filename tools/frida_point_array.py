#!/usr/bin/env python3
"""tools/frida_point_array.py —— `tools/frida_point_array.js` 的驱动。

它做三件事（必须**同一个进程**里跑完，因为 frida 的 local device 只能看见本 PID 命名空间的进程）：

1. 拉起**真游戏**（`Graphwar II` Linux 2.0.60）—— 本机必须用 `--attach`（见下）；
2. 载入 `tools/frida_point_array.js` 下断（地址 = x86-64 ELF 虚地址，见脚本头）；
3. 用 `xdotool` 把对局点进去并**开炮**（可连发），收集消息写成 JSONL。

用法（X 显示可以用 `Xvfb`，见 tools/frida_point_array.README.md）：

    # ★ 本机唯一能跑通的姿势：自己 Popen 再 attach（frida spawn 会把这游戏挂在 wait4、永不建窗口）
    .reference/venv/bin/python tools/frida_point_array.py --attach --out /abs/run.jsonl

    # 按老姿势 spawn（在别的机器上可能正常；本机实测会把游戏挂死，见 docs/06 §42.1）
    .reference/venv/bin/python tools/frida_point_array.py --out /abs/run.jsonl

    # 只挂探针、不点菜单（自己手动玩）：
    .reference/venv/bin/python tools/frida_point_array.py --attach --out /abs/run.jsonl --no-clicks

    # 连打 N 发、逐发换表达式：
    ... --attach --out /abs/run.jsonl --shots 24 --gap 9 --exprs "sin(x)+3,x/2,0"

    # 不给游戏，只把已有 JSONL 摘成入库用的纯文本样本（`--extract` 可给多次）：
    .reference/venv/bin/python tools/frida_point_array.py \
        --extract a.jsonl --extract b.jsonl --label series1 --label series2 \
        --fixture-out fixtures/frida_ptr_pair_sample.jsonl

注意：脚本**只观测**，不改游戏内存。
"""
import argparse
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))


def _find_repo():
    """往上找主仓库根（含 `.reference/game`）；本脚本可能在 `.agent-work/**` 里跑。"""
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
JS = os.path.join(HERE, 'frida_point_array.js')

# ── 1920x1160 下的点击坐标（来自本轮的实测截图，见 docs/06 §33）──────────────
# 前三步：进对局（主菜单 → 关卡节点 → Level 0 的 Start）；之后每一发都重复"输入框 → 打字 → 长按"
NAV = [
    (0.0, 'mousemove 534 629 click 1'),    # 主菜单 Campaign
    (4.0, 'mousemove 243 224 click 1'),    # campaign 列表里唯一的关卡节点
    (4.0, 'mousemove 1072 1009 click 1'),  # Level 0 弹窗 Start
]
# ⚠ 2026-11-08（本轮实测）：输入框真正的可点区域在 y≈1061–1134（1920×1160 窗口截图量的），
#   旧的 y=1048 落在框**上面**，点了不进焦点。改成 y=1090（见 docs/06 §42.1）。
CLICK_INPUT = 'mousemove 1000 1090 click 1'   # f(x)= 输入框
# ⚠ `mousemove` 与 `mousedown` **必须分成两条 xdotool 命令**（中间留 0.4 s）：
#   合成一条 `mousemove X Y mousedown 1` 时会话把"warp 指针"和"按下"挤在同一个请求批里，
#   按下事件按**旧位置**派发（实测：Fire 按钮只被描边、没有开火，曲线不出现）。
FIRE_MOVE = 'mousemove 1855 55'
FIRE_DOWN = 'mousedown 1'

# 单发：输入框 → ctrl+a 全选 → 打字 → 长按 2 s
# ★ 必须先全选再打字：单击只是把**光标放进去**，直接 type 会**接在旧文本后面**。
#   2026-11-07 独立复现轮实测：`--extra-shot` 想打 `x/2`，实际打进去的是
#   `sin(x)+3x/2`（真机曲线与 `sin(x)+3x/2` 逐位吻合 929/929，见 docs/06 §35.8）。
def shot_actions(expr):
    """返回 (打字前的动作, 开火动作)。

    ⚠ 2026-11-08 本机实测：`mousemove` 与 `click 1` **也必须分开**（和 Fire 那条一样）。
      合成一条 `mousemove X Y click 1` 时，点击按**旧指针位置**派发 —— 第 1 发之后
      输入框再也没拿到焦点，连打 24 发里实际生效的 f(x) 一直是第一条（快照里看得见）。
    """
    return ([
        (0.0, 'mousemove 1000 1090'),
        (0.3, 'click 1'),
        (0.4, 'key --clearmodifiers ctrl+a'),
        (0.3, 'type --delay 120 "%s"' % expr),
    ], [
        (1.2, FIRE_MOVE),
        (0.4, FIRE_DOWN),
        (2.0, 'mouseup 1'),
    ])

_pre, _fire = shot_actions('sin(x)+3')
CLICKS = NAV + _pre + _fire            # 兼容旧用法（= 进对局 + 打一发 sin(x)+3）


def xdo(display, action):
    cmd = 'xdotool ' + action
    subprocess.run(cmd, shell=True, env=dict(os.environ, DISPLAY=display),
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


# ── 2026-11-08 `dispatch/34`：可选的逐步截图（**只用于调坐标/留现场**，PNG 不入库）──
_SNAP = {'dir': None, 'n': 0}


def _win_id(display):
    """找游戏窗口 id（`xdotool search --name`）；找不到回 'root'。"""
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


def snap(display, label):
    """截**游戏窗口**（不是 root）成 PNG；失败不影响主流程。"""
    d = _SNAP['dir']
    if not d:
        return
    try:
        os.makedirs(d, exist_ok=True)
        _SNAP['n'] += 1
        p = os.path.join(d, '%02d_%s.png' % (_SNAP['n'], label))
        # ⚠ `import -window root` 在**没有合成器**的 Xvfb 上只抓到根窗口（实测全黑，mean=0）：
        #   游戏的 1920×1160 子窗口不会合成到 root 上。必须按**窗口 id** 抓。
        subprocess.run(['import', '-window', _win_id(display), p],
                       env=dict(os.environ, DISPLAY=display),
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print('[snap] %s' % p, file=sys.stderr)
    except Exception as e:  # noqa: BLE001
        print('[snap-warn]', e, file=sys.stderr)


# ═══════════════════════════════════════════════════════════════════════════════
# 2026-11-08 `dispatch/34`：把原始 JSONL 摘成**入库用**的纯文本样本（`--extract`）
#   原始 JSONL 里每个 `exec_queue` 都带整条点数组的十六进制（一条 3968 点 = 127 KB 文本），
#   而且有 `beat`、`reallocs` 这类噪声。入库只要：每次 `calculate_function` 的 (ptr,len,摘要)、
#   两次调用之间的**逐点对比结果**、`movdqu` 那一刻的 (rdx,rcx) 与它和队列项/两块数组的对比。
#   规则写在这里 = 别人拿原始 JSONL 能一条命令重放出同一个入库文件。
# ═══════════════════════════════════════════════════════════════════════════════

FIXTURE_SCHEMA = 'gw2-frida-ptr-pair-sample/1'


def _classify_min(c):
    if not isinstance(c, dict):
        return None
    return {k: c.get(k) for k in ('n', 'exact', 'ulp1', 'miss', 'pairExact', 'pairWho')
            if c.get(k) is not None}


def _queue_item_min(it):
    o = {k: it.get(k) for k in ('lenOff', 'tag', 'ptr', 'len', 'digest_xy', 'digest_y', 'yhash', 'skipped')
         if it.get(k) is not None}
    if it.get('head'):
        o['head'] = it['head']
    if it.get('tail'):
        o['tail'] = it['tail']
    cl = _classify_min(it.get('classify'))
    if cl:
        o['classify'] = cl
    return o


def _parse_one(src_path, label, notes=None):
    """摘一份原始 JSONL → (shots, meta)。规则见上面那段注释。"""
    rows = []
    with open(src_path, encoding='utf-8') as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))

    import hashlib
    raw_sha = hashlib.sha256(open(src_path, 'rb').read()).hexdigest()
    by_type = {}
    for r in rows:
        t = r.get('type', '?')
        by_type[t] = by_type.get(t, 0) + 1

    shots, pend = [], []
    cur = None
    for r in rows:
        t = r.get('type')
        if t == 'calc_array':
            pend.append(r)
        elif t == 'exec_queue':
            q = {k: r.get(k) for k in ('t', 'self', 'qptr', 'qlen', 'qcap')}
            q['items'] = [_queue_item_min(i) for i in (r.get('items') or []) if i.get('len')]
            q['rawitems'] = r.get('rawitems')          # 每项 0x20 字节原文（短，留）
            cur = {'queue': q, 'calc_calls': [], 'pos_read': None, 'exec_t': r.get('t')}
        elif t == 'pos_read':
            if cur is None:
                cur = {'calc_calls': [], 'queue': None}
            cur['pos_read'] = {k: r.get(k) for k in
                               ('seq', 't', 'rdx', 'rcx', 'rcx_mod16', 'n', 'n_from_rcx', 'n_from_r13',
                                'r13', 'r15', 'pos_bytes', 'digest_xy', 'digest_y', 'vs_queue', 'vs_calc')
                               if r.get(k) is not None}
            for c in pend:
                cur['calc_calls'].append({
                    'ret': c.get('ret'), 'rax': c.get('rax'),
                    'info': {k: (c.get('info') or {}).get(k) for k in
                             ('seq', 'kind', 'ptr', 'len', 'digest_xy', 'digest_y', 'pts', 'vs', 'skipped', 'error')
                             if (c.get('info') or {}).get(k) is not None},
                })
            # 两次调用之间是否逐位相同 —— 直接从第二次的 `vs` 里取（cmpArrayBytes 的原始结论）
            pair = None
            if len(cur['calc_calls']) >= 2:
                b_right = cur['calc_calls'][1]
                a_right = cur['calc_calls'][0]
                b, a = b_right['info'], a_right['info']
                # ⚠ 找"这一发的另一次调用"必须**点名**，不能只按 ret+len：
                # `ST.calcArrays` 里留着**前几发**的数组，ret+len 会命中最早那一条（跨发的），
                # 第一版就是这样把 20 发的"相同"误判成 3 发的"不同"（docs/06 §42.4）。
                # 现在优先用探针给的全局 `seq`（a.seq 的那条），没有 seq 时退回"最近一条 ret+ptr 相同的"。
                vs = b.get('vs') or []
                hit = None
                if a.get('seq') is not None:
                    hit = next((v for v in vs if v.get('seq') == a.get('seq')), None)
                if hit is None:
                    cand = [v for v in vs if v.get('ret') == a_right.get('ret') and v.get('len') == a.get('len')]
                    hit = cand[-1] if cand else None
                if hit is not None:
                    pair = {'a_ret': a_right.get('ret'), 'b_ret': b_right.get('ret'),
                            'a_len': a.get('len'), 'b_len': b.get('len'),
                            'same_bytes': bool(hit.get('same')), 'diff_points': hit.get('diffPoints'),
                            'first_diff': hit.get('firstDiff')}
                else:
                    pair = {'a_ret': a_right.get('ret'), 'b_ret': b_right.get('ret'),
                            'a_len': a.get('len'), 'b_len': b.get('len'), 'same_bytes': None,
                            'note': 'no comparison row (different len? bytes not copied?)'}
            cur['pair'] = pair
            shots.append(cur)
            pend = []
    # 把每发的序号补上
    for i, s in enumerate(shots):
        s['type'] = 'shot'
        s['series'] = label
        s['shot'] = i
    meta = {'label': label, 'sha256': raw_sha, 'lines': len(rows), 'records_by_type': by_type,
            'hooks': next((r for r in rows if r.get('type') == 'hooks'), None),
            'selftest': next((r for r in rows if r.get('type') == 'selftest'), None)}
    return shots, meta


def extract_fixture(specs, out_path, notes=None):
    """把一份或多份原始 JSONL 摘成**一个**入库样本；返回 (provenance, shots)。

    `specs` = [(jsonl 路径, 标签), …]，标签会写进每一行的 `series`。
    """
    all_shots, sources = [], []
    hooks = selftest = None
    for path, label in specs:
        sh, meta = _parse_one(path, label)
        all_shots += sh
        hooks = hooks or meta['hooks']
        selftest = selftest or meta['selftest']
        sources.append({'label': meta['label'], 'raw_jsonl_sha256': meta['sha256'],
                        'raw_jsonl_lines': meta['lines'], 'records_by_type': meta['records_by_type'],
                        'shots': len(sh)})

    same = [s for s in all_shots if (s.get('pair') or {}).get('same_bytes') is True]
    diff = [s for s in all_shots if (s.get('pair') or {}).get('same_bytes') is False]
    unk = [s for s in all_shots if (s.get('pair') or {}).get('same_bytes') is None]
    q_len_ok = [s for s in all_shots if any(v.get('same_len') and v.get('same_ptr') for v in
                                            ((s.get('pos_read') or {}).get('vs_queue') or []))]
    q_dg_ok = [s for s in all_shots if any(v.get('same_len') and v.get('same_digest') for v in
                                           ((s.get('pos_read') or {}).get('vs_queue') or []))]
    pr_eq_both = [s for s in all_shots if len([v for v in ((s.get('pos_read') or {}).get('vs_calc') or [])
                                               if v.get('same') is True]) >= 2]
    lens = sorted({(s.get('pos_read') or {}).get('n') for s in all_shots if (s.get('pos_read') or {}).get('n')})

    prov = {
        'type': 'provenance', 'schema': FIXTURE_SCHEMA,
        'what': ('runtime probe of the two (ptr, n) readings behind docs/06 42: '
                 'the report layer reads `pos = pts[n-1]` at execute_results `0x472865`; '
                 'each shot calls calculate_function TWICE (gw+3c30c7 / gw+3e2d3b) '
                 'into two different buffers; is the array the report reads the same buffer, '
                 'and are the two arrays bitwise identical?'),
        'build': {
            'game': 'Graphwar II Linux 2.0.60 (Steam)',
            'libgraphwarrust.so_sha256': '9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187',
            'engine': 'Godot 4.7.stable.official.5b4e0cb0f + godot-rust (API v4.6, runtime v4.7)',
            'address_space': 'RVA = ELF virtual address = module offset; Ghidra address = RVA + 0x100000',
            'hooked_rva': {'execute_results': '0x472470', 'exec_queue_probe': '0x4724cc',
                           'pos_read_movdqu': '0x472865', 'calculate_function': '0x3c2530',
                           'evaluate': '0x4eb9d0', 'slot_9bac98': '0x9bac98'},
        },
        'how': {
            'harness': 'Xvfb :92 -screen 0 1920x1160x24 -listen tcp -ac  (+ mesa-only EGL/GLX env)',
            'driver': ('tools/frida_point_array.py --attach   (NOT frida spawn: on this build spawn '
                       'hangs the game in wait4 before it creates a window; see docs/06 42.1)'),
            'probe': 'tools/frida_point_array.js (read-only Interceptor hooks; no memory writes)',
            'flow': 'Popen game -> attach -> click Campaign / level node / Start -> type f(x) -> LONG-PRESS Fire! -> execute_results',
            'digest': ('FNV-1a 64 over ALL 16*len bytes of the point array (`digest_xy`) and over the y halves '
                       'only (`digest_y`). Digests are only for indexing/recording: every same/different verdict '
                       'is decided by an in-process byte-by-byte memcmp (cmpArrayBytes), not by a digest compare.'),
            'positive_control': ('the probe runs `selfTestCompare()` at install: two 100-point synthetic arrays, '
                                 'identical -> same=true; flip one bit at point 37 -> diffPoints=1 at index 37; '
                                 'flip another at point 90 -> diffPoints=2 at 37,90. Recorded in `probe_selftest`. '
                                 'A second, *runtime* positive control: shots with different f(x) do compare '
                                 'different (see `pair`/`vs` rows of shots across series).'),
        },
        'cut': ('per shot: every calculate_function return record (ret addr, kind, ptr, len, both digests, '
                'first/last point), the byte-compare of call#2 against call#1, the execute_results queue '
                'snapshot (0x20-byte raw items + per-item len/ptr/digest/head8/tail8/classify), and the '
                '`movdqu` reading (rdx, rcx, n, the 16 bytes = pts[n-1], both digests, and its comparison '
                'against the queue snapshot and against both calculate_function arrays).'),
        'dropped': ('`beat` heartbeats, `reallocs` rows, `evals`/`samplers` bulk rows, and the full hex dump of '
                    'each array (the raw JSONL has it; here only digests + head/tail points survive).'),
        'no_local_paths': 'this file contains no filesystem path and no user name; all pointers are ASLR-random per run.',
        'sources': sources,
        'probe_selftest_at_install': (selftest or {}).get('cmp'),
        'hooks': hooks,
        'counts': {'shots': len(all_shots), 'pair_same_bytes': len(same), 'pair_different': len(diff),
                   'pair_unknown': len(unk),
                   'pos_read_ptr_and_len_match_queue': len(q_len_ok),
                   'pos_read_digest_matches_queue': len(q_dg_ok),
                   'pos_read_equals_both_calc_arrays': len(pr_eq_both),
                   'distinct_array_lengths': lens},
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
    ap.add_argument('--display', default=os.environ.get('DISPLAY', ':92'))
    ap.add_argument('--data-dir', default=os.path.join(REPO, '.agent-work', 'cg', 'gamedata25'))
    ap.add_argument('--js', default=JS, help='要载入的 frida 脚本')
    ap.add_argument('--game', default=GAME, help='游戏目录（含 Graphwar.x86_64 与 .pck）')
    ap.add_argument('--no-clicks', action='store_true')
    ap.add_argument('--settle', type=float, default=10.0, help='启动后等多久再点菜单')
    ap.add_argument('--tail', type=float, default=20.0, help='开火后再录多久')
    ap.add_argument('--extra-shot', action='store_true', help='开火后再打一炮')
    # ── 2026-11-08 `dispatch/34`：连打多发的模式（每发之间等它把弹道走完）──────────
    ap.add_argument('--shots', type=int, default=1, help='连打几发（默认 1 = 旧行为）')
    ap.add_argument('--exprs', default='sin(x)+3', help='逗号分隔的表达式表，逐发轮换（对局内生效的是打完 ctrl+a 后打进去的那条）')
    ap.add_argument('--gap', type=float, default=9.0, help='两发之间等多久（让上一发结算完）')
    ap.add_argument('--eval-rows', type=int, default=0, help='`evals` 抽样行上限；0（默认）= 不收，只留 classify 用的 yMap')
    ap.add_argument('--attach', action='store_true',
                    help='自己 Popen 游戏再 attach（**本机必须**：frida spawn 会把这游戏挂死在 wait4，见 docs/06 §42.1）')
    ap.add_argument('--snap-dir', default=None, help='（调试用）每一步截一张 1920x1160 的 PNG 到这个目录；PNG 不入库')
    ap.add_argument('--extract', default=None, action='append',
                    help='不给游戏、只把已有 JSONL 摘成入库样本（可给多次；配合 --fixture-out）')
    ap.add_argument('--fixture-out', default=None, help='`--extract` 的输出文件（纯文本 JSONL）')
    ap.add_argument('--note', default=None, action='append',
                    help='`--extract` 时写进 provenance 的说明（可给多次）')
    ap.add_argument('--label', default=None, action='append',
                    help='`--extract` 时给每批起的标签（顺序对应；缺省用 seriesN）')
    args = ap.parse_args()

    if not args.extract and not args.out:
        ap.error('需要 --out（或者用 --extract 模式）')
    if args.extract:
        if not args.fixture_out:
            ap.error('--extract 需要 --fixture-out')
        labels = args.label or []
        specs = [(p, labels[i] if i < len(labels) else 'series%d' % (i + 1))
                 for i, p in enumerate(args.extract)]
        prov, shots = extract_fixture(specs, args.fixture_out, notes=args.note)
        print('[extract] shots=%d pair_same=%d pair_diff=%d pair_unknown=%d -> %s' % (
            prov['counts']['shots'], prov['counts']['pair_same_bytes'], prov['counts']['pair_different'],
            prov['counts']['pair_unknown'], args.fixture_out), file=sys.stderr)
        return

    import frida

    data = args.data_dir
    env = {
        'DISPLAY': args.display,
        'XDG_DATA_HOME': os.path.join(data, 'data'),
        'XDG_CACHE_HOME': os.path.join(data, 'cache'),
        'XDG_CONFIG_HOME': os.path.join(data, 'config'),
        # Xvfb(:92) 上只有 mesa llvmpipe —— 不让 glvnd 去碰 nvidia
        '__EGL_VENDOR_LIBRARY_FILENAMES': '/usr/share/glvnd/egl_vendor.d/50_mesa.json',
        'LIBGLX_VENDOR_LIBRARY_NAME': 'mesa',
        'LIBGL_ALWAYS_SOFTWARE': '1',
        'LC_CTYPE': 'C.UTF-8',
    }
    for k in ('data', 'cache', 'config'):
        os.makedirs(os.path.join(data, k), exist_ok=True)

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
        elif t == 'exec_queue':
            print('[exec_queue] t=%s qlen=%s items=%s' % (
                p.get('t'), p.get('qlen'), [(i.get('lenOff'), i.get('len'), i.get('digest_xy')) for i in p.get('items') or []]),
                file=sys.stderr)
        elif t == 'exec_enter':
            print('[exec_enter] t=%s self=%s bt=%s' % (p.get('t'), p.get('self'), (p.get('bt') or [])[:4]),
                  file=sys.stderr)
        elif t == 'selftest':
            print('[selftest] cmp', json.dumps(p.get('cmp'), ensure_ascii=False), file=sys.stderr)
        elif t == 'cfg_ack':
            print('[cfg_ack]', json.dumps(p, ensure_ascii=False), file=sys.stderr)
        elif t == 'calc_array':
            i = p.get('info') or {}
            print('[calc_array] ret=%s ptr=%s len=%s vs=%s' % (
                p.get('ret'), i.get('ptr'), i.get('len'),
                [(v.get('ret'), v.get('same'), v.get('diffPoints')) for v in (i.get('vs') or [])]),
                file=sys.stderr)
        elif t == 'pos_read':
            print('[pos_read] #%s rdx=%s rcx=%s n=%s pos=%s vs_queue=%s vs_calc=%s' % (
                p.get('seq'), p.get('rdx'), p.get('rcx'), p.get('n'), p.get('pos_bytes'),
                [(v.get('ptr'), v.get('same_ptr'), v.get('same_len'), v.get('same_digest')) for v in (p.get('vs_queue') or [])],
                [(v.get('ret'), v.get('same'), v.get('diffPoints')) for v in (p.get('vs_calc') or [])]),
                file=sys.stderr)

    dev = frida.get_local_device()
    game = args.game
    argv = [os.path.join(game, 'Graphwar.x86_64'), '--resolution', '1920x1160', '--position', '0,0']
    env = {k: v for k, v in dict(os.environ, **env).items() if k not in ('LC_ALL',)}
    if args.attach:
        # ── 2026-11-08 `dispatch/34`：★ 本机实测 **frida `spawn` 会把这份游戏挂死** ──────────
        # 现象：`dev.spawn()` + `dev.resume()` 之后进程树变成
        #   Graphwar(父, `x64_sys_wait4` 里等子进程) → Graphwar(子, `futex_wait`)，
        # 引擎只打到 "switching to OpenGL 3" 就**永远不建窗口**（`xwininfo -root -children` 0 个，窗口截图全黑 mean=0），
        # 于是后面的点击全落在根窗口上 —— 五次抓取一次都没开成炮（不是"打得不准"，是**没窗口**）。
        # 改成"自己 Popen + `dev.attach(pid)`"后一切正常（同一台机、同一个 Xvfb、同一份脚本）。
        print('[*] popen', argv, file=sys.stderr)
        proc = subprocess.Popen(argv, env=env, cwd=game)
        pid = proc.pid
        print('[*] pid', pid, file=sys.stderr)
        # 等它真的把窗口建出来（没有窗口就别急着点）
        t_wait = time.time()
        while time.time() - t_wait < 40:
            if _win_id(args.display) != 'root':
                break
            time.sleep(0.5)
        print('[*] window=%s after %.1fs' % (_win_id(args.display), time.time() - t_wait), file=sys.stderr)
    else:
        print('[*] spawn', argv, file=sys.stderr)
        pid = dev.spawn(argv, env=env, cwd=game)
        print('[*] pid', pid, file=sys.stderr)
    session = dev.attach(pid)
    script = session.create_script(src, runtime='v8')
    script.on('message', on_message)
    script.load()
    # 2026-11-08 `dispatch/34`：连打多发的模式把 `evals` 抽样关掉（默认 0），
    # 否则 20 发会把 JSONL 撑到几十 MB；`classify` 用的 yMap/pairMap 不受影响。
    try:
        script.post({'type': 'cfg', 'evalRowsMax': args.eval_rows})
    except Exception as e:  # noqa: BLE001
        print('[warn] post cfg:', e, file=sys.stderr)
    if not args.attach:
        dev.resume(pid)
        print('[*] resumed', file=sys.stderr)

    _SNAP['dir'] = args.snap_dir
    t0 = time.time()
    try:
        if not args.no_clicks:
            time.sleep(args.settle)
            snap(args.display, 'boot')
            for i, (delay, action) in enumerate(NAV):
                time.sleep(delay)
                print('[click] +%.1fs %s' % (time.time() - t0, action), file=sys.stderr)
                xdo(args.display, action)
                snap(args.display, 'nav%d' % i)
            exprs = [e.strip() for e in args.exprs.split(',') if e.strip()] or ['sin(x)+3']
            nshots = max(1, args.shots)
            for s in range(nshots):
                expr = exprs[s % len(exprs)]
                print('[shot %d/%d] f(x) = %s' % (s + 1, nshots, expr), file=sys.stderr)
                pre, fire = shot_actions(expr)
                for delay, action in pre:
                    time.sleep(delay)
                    xdo(args.display, action)
                snap(args.display, 'typed%d' % (s + 1))
                for delay, action in fire:
                    time.sleep(delay)
                    xdo(args.display, action)
                snap(args.display, 'shot%d' % (s + 1))
                if s + 1 < nshots:
                    time.sleep(args.gap)          # 让这一发的弹道走完、结算完
            if args.extra_shot:
                time.sleep(10)
                pre, fire = shot_actions('x/2')
                for delay, action in pre + fire:
                    time.sleep(delay)
                    xdo(args.display, action)
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
