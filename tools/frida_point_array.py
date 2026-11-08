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


# ═══════════════════════════════════════════════════════════════════════════════
# 2026-11-08 `dispatch/44`：把战役关卡解锁 / 点进某个关卡（造 `DiffEq` 局要用）
#   ① `user://campaign.save` = `level_<n>`（纯文本；`docs/10-campaign.md` §2 逐位验证过）
#   ② 关卡网格坐标（1920×1160，三行七列、**蛇形编号**；`docs/06` §27.1 ③ 实测）
#   全部**追加**在原结构之外：不给这两个开关时 NAV 与老行为逐字节相同。
# ═══════════════════════════════════════════════════════════════════════════════
LEVEL_NODE = {
    0: (243, 224), 1: (486, 224), 2: (700, 224), 3: (963, 224), 4: (1187, 224), 5: (1429, 224), 6: (1672, 224),
    13: (212, 486), 12: (467, 486), 11: (698, 486), 10: (963, 486), 9: (1187, 486), 8: (1429, 486), 7: (1657, 486),
    14: (218, 751), 15: (467, 751), 16: (691, 751), 17: (963, 751), 18: (1187, 751), 19: (1429, 751), 20: (1672, 751),
}


def write_campaign_save(data_dir, level):
    """往工作区自己的 `XDG_DATA_HOME` 里预写 `user://campaign.save = level_<n>`。

    游戏侧 `0x418170` 读全文 → `rsplit('_')` → `u32`；`level_20` = 解锁到 20（全部 21 关）。
    ⚠ 只写**工作区**的 data 目录（`--data-dir`），不碰用户真正的 `user://`。
    """
    p = os.path.join(data_dir, 'data', 'godot', 'app_userdata', 'Graphwar', 'campaign.save')
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, 'w', encoding='utf-8') as f:
        f.write('level_%d' % level)
    print('[save] campaign.save = level_%d' % level, file=sys.stderr)
    return p


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
        elif t == 'tok_fire':
            v = p.get('vec') or {}
            tk = p.get('tok') or {}
            print('[tok_fire] #%s ret=%s flip=%s mode=%s x0=%s y0=%s step=%s vec=(cap=%s,len=%s) tags=%s vs_parse=%s' % (
                p.get('seq'), p.get('ret'), p.get('flip'), p.get('mode'),
                p.get('x0'), p.get('y0'), p.get('step'), v.get('cap'), v.get('len'), tk.get('tags'),
                [(q.get('seq'), q.get('ret'), q.get('ptr_eq'), q.get('same_bytes'), (q.get('text') or '')[:60])
                 for q in (p.get('vs_parse') or [])]), file=sys.stderr)
        elif t == 'tok_parse':
            r = p.get('rec') or {}
            o = r.get('out') or {}
            print('[tok_parse] #%s ret=%s txtlen=%s outlen=%s tags=%s txt=%r' % (
                r.get('seq'), r.get('ret'), r.get('txtLen'), o.get('len'), o.get('tags'),
                (r.get('txt') or '')[:120]), file=sys.stderr)
        elif t == 'tok_suffix':
            print('[tok_suffix] #%s ret=%s flip=%s x0=%s y0=%s -> %r' % (
                p.get('seq'), p.get('ret'), p.get('flip'), p.get('x0'), p.get('y0'), p.get('text')),
                file=sys.stderr)
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


# ═══════════════════════════════════════════════════════════════════════════════
# 2026-11-08 `dispatch/39`：把 token 探针的原始 JSONL 摘成入库样本 + 做**逐字节**比较
#
# 输入：`--tokens` 跑出来的原始 JSONL（可能多份，按 `--label` 起名）+（可选）**同一局**的
#   回放文件（`user://replays/<unix 秒>`，gzip JSONL）——有它才能做"真机抓到的 token 列表
#   vs 回放里记的两种表示"的**同局**比较。
#
# 三种比较（全部逐字节，不看"像不像"）：
#   A. 探针在 `calculate_function` 入口抓到的 token 列表
#      vs **同一次运行里解析器产出的那个 token 列表**（ptr 恒等 + 逐字节）→ "token 从哪来"
#   B. 探针抓到的"喂进解析器的那串文本"（`txtHex`）
#      vs 回放里 `FunctionInfo.corrected_function` 的 UTF-8 字节 → 回放记的是不是喂进去的那串
#   C. 真二进制（unicorn 直调 `0x4ed670`）重新解析 `corrected_function` 得到的 token 列表
#      vs 探针抓到的 token 列表（16 B/token 逐字节，含常量的 f64 位模式）→ 端到端闭合（`--oracle`）
# ═══════════════════════════════════════════════════════════════════════════════

TOKEN_TAG = {0: 'x', 1: 'y', 2: "y'", 3: '+', 4: '-', 5: 'neg', 6: '*', 7: '/', 8: '^',
             9: 'sqrt', 10: 'log', 11: 'ln', 12: 'exp', 13: 'abs', 14: 'sin', 15: 'cos',
             16: 'tan', 17: 'const', 18: '(', 19: ')'}
TOKENS_FIXTURE_SCHEMA = 'gw2-frida-token-at-fire-sample/1'


def decode_tokens(hexs):
    """16 B/token 的原始十六进制 → [{tag, name, bits, value}]。`tag` 是 u64，`value` 是 f64。"""
    import struct as _s
    out = []
    if not hexs:
        return out
    for i in range(0, len(hexs), 32):
        ch = hexs[i:i + 32]
        if len(ch) != 32:
            break
        raw = bytes.fromhex(ch)
        tag = _s.unpack('<Q', raw[:8])[0]
        val = _s.unpack('<d', raw[8:])[0]
        out.append({'tag': tag, 'name': TOKEN_TAG.get(tag, '?%d' % tag),
                    'value_bits': raw[8:].hex(), 'value': val})
    return out


def token_sig(hexs):
    """逐字节比较用的规范化形式（就是 hex 本身；空列表 → 空串）。"""
    return hexs or ''


def prefix_text(tokens):
    """token 列表 → **前缀写法**的短语（只是给人看的忠实渲染，不是 `0x4ec160` 的 `to_text`）。"""
    parts = []
    for t in tokens:
        if t['name'] == 'const':
            # 用 repr（最短往返表示）：`10.938` 而不是 `10.938000000000001`；
            # 精确定义仍然只有 `value_bits` 那 16 个十六进制字符算数。
            parts.append(repr(t['value']))
        else:
            parts.append(t['name'])
    return '(' + ' '.join(parts) + ')'


def read_replay_functions(path):
    """读一局回放（gzip JSONL），返回 (按出现顺序的 FunctionInfo 行, 其余关键行摘要, sha256)。"""
    import gzip
    import hashlib
    raw = open(path, 'rb').read()
    sha = hashlib.sha256(raw).hexdigest()
    try:
        text = gzip.decompress(raw)
    except OSError:
        text = raw
    rows = [json.loads(l) for l in text.split(b'\n') if l.strip()]
    fis, keep = [], []
    for r in rows:
        ev = r.get('event') or {}
        k = next(iter(ev), None)
        if k == 'FunctionInfo':
            fis.append({'tick': r.get('tick'), **ev[k]})
        elif k in ('FunctionPoints', 'FunctionFire', 'EffectInfo', 'GameStateInfo', 'TargetInfo', 'LifeInfo'):
            keep.append({'tick': r.get('tick'), 'event': {k: ev[k]}})
    return fis, keep, sha


def _function_for_fire(fis, corrected):
    """在回放里找 `corrected_function` 与抓到的文本**逐字节相等**的那一条 FunctionInfo。

    找不到就返回 None（不许猜"大概是这条"）。
    """
    for i, f in enumerate(fis):
        cf = f.get('corrected_function')
        if cf is not None and cf == corrected:
            return i, f
    return None, None


def tokens_report(specs, out_path, replay_path=None, notes=None, oracle=0):
    """`--tokens-report`：原始 JSONL（可多份）→ 入库样本 + 三种逐字节比较。"""
    import hashlib
    import struct as _s

    fires, sources = [], []
    hooks = selftest = None
    by_type_all = {}
    for path, label in specs:
        raw = open(path, 'rb').read()
        sha = hashlib.sha256(raw).hexdigest()
        rows = [json.loads(l) for l in raw.decode('utf-8').split('\n') if l.strip()]
        by_type = {}
        for r in rows:
            t = r.get('type', '?')
            by_type[t] = by_type.get(t, 0) + 1
            by_type_all[t] = by_type_all.get(t, 0) + 1
        hooks = hooks or next((r for r in rows if r.get('type') == 'hooks'), None)
        selftest = selftest or next((r for r in rows if r.get('type') == 'selftest'), None)
        suffixes = [r for r in rows if r.get('type') == 'tok_suffix' and r.get('text')]
        suffix_texts = sorted({r['text'] for r in suffixes})
        for r in rows:
            if r.get('type') != 'tok_fire':
                continue
            tok = r.get('tok') or {}
            matched = (r.get('vs_parse') or [])
            # 只认"逐字节相同"的那一条；按 seq 取最小的（同一次运行里同一段文本可能 parse 两次）
            same = [q for q in matched if q.get('same_bytes') is True]
            best = sorted(same, key=lambda q: (q.get('seq') or 0))[0] if same else None
            fires.append({
                'type': 'fire', 'series': label, 'seq': r.get('seq'), 't': r.get('t'),
                'caller': r.get('ret'),
                'flip': r.get('flip'), 'mode': r.get('mode'),
                'x0_bits': r.get('x0'), 'y0_bits': r.get('y0'), 'step_bits': r.get('step'),
                'vec': r.get('vec'),
                'tokens': {'len': tok.get('len'), 'hex': tok.get('hex'), 'tags': tok.get('tags')},
                'matched_parse': None if not best else {
                    'seq': best.get('seq'), 'caller': best.get('ret'),
                    'ptr_eq': best.get('ptr_eq'), 'same_bytes': best.get('same_bytes'),
                    'text': best.get('text'), 'txt_hex': best.get('txtHex'), 'txt_len': best.get('txtLen'),
                    'out_ptr': (best.get('parse_out') or {}).get('ptr'),
                },
                'vs_parse_all_same': len(same),
                # 这一发抓到的那串文本**以哪些**修正常数后缀结尾（就 `0x3c5f70` 印出来的那些串，
                # 逐个与文本做字节后缀比较；不做"取最后一条"这种猜法）
                'suffix_matches': sorted({t for t in suffix_texts
                                          if (best or {}).get('text') and best['text'].endswith(t)}),
            })
        sources.append({'label': label, 'raw_jsonl_sha256': sha, 'raw_jsonl_lines': len(rows),
                        'records_by_type': by_type, 'fires': sum(1 for r in rows if r.get('type') == 'tok_fire')})

    # ── 同一局的回放：只用来做 B/C 两条比较（没有就不做，不许拿别的局凑）────────
    fis, replay_keep, replay_sha = ([], [], None)
    if replay_path:
        fis, replay_keep, replay_sha = read_replay_functions(replay_path)

    # ── 可选：真二进制（unicorn）再解析一遍（C）────────────────────────────────
    oracle_parse = None
    if oracle:
        sys.path.insert(0, HERE)
        import eval_oracle as EO  # 复用既有的真二进制对拍台，不另造
        Gw = EO.load_oracle()
        EO.G(Gw)                      # 常驻一个模拟器

        def oracle_parse(text):       # noqa: F811
            EO.reset_heap(Gw)
            toks = EO.call_parse(Gw, text)
            return [{'tag': t, 'bits': '%016x' % _s.unpack('<Q', _s.pack('<d', v))[0]} for t, v in toks]

    # ── 逐发：解码 + 三种比较 ──────────────────────────────────────────────────
    rows_out = []
    cmp_A = cmp_B = cmp_C = 0
    n_fires = 0
    for f in fires:
        n_fires += 1
        dec = decode_tokens((f['tokens'] or {}).get('hex'))
        f['tokens_decoded'] = dec
        f['prefix_text'] = prefix_text(dec)
        mp = f.get('matched_parse') or {}
        text = mp.get('text')
        f['A_tokens_eq_parse'] = bool(mp.get('same_bytes') is True)
        cmp_A += 1 if f['A_tokens_eq_parse'] else 0
        # B：抓到的文本字节 vs 回放里那条 corrected_function 的字节
        if text is not None and fis:
            idx, rec = _function_for_fire(fis, text)
            if rec is not None:
                fn = rec.get('function')
                cf = rec.get('corrected_function')
                prefix_ok = cf is not None and fn is not None and cf.startswith(fn)
                f['replay'] = {'function_index': idx, 'tick': rec.get('tick'),
                               'function': fn, 'corrected_function': cf,
                               'corrected_utf8_hex': (cf or '').encode('utf-8').hex(),
                               'function_is_prefix_of_corrected': bool(prefix_ok)}
                f['B_text_eq_replay_corrected'] = (text == cf)
                cmp_B += 1 if f['B_text_eq_replay_corrected'] else 0
            else:
                f['replay'] = None
                f['B_text_eq_replay_corrected'] = None
        # C：真二进制解析 corrected_function → token 列表，与抓到的逐字节比
        if oracle_parse is not None and f.get('replay') and f['replay'].get('corrected_function'):
            want = f['tokens']
            got = oracle_parse(f['replay']['corrected_function'])
            # ⚠ 位串的两种写法：抓的是内存里那 8 字节的**小端字节序**十六进制，
            #   oracle 给的是 u64 的**大端**十六进制；统一折成整数再比，别拿字符串比。
            same = (len(got) == (want.get('len') or 0) and all(
                (d['tag'] == g['tag'] and
                 int.from_bytes(bytes.fromhex(d['value_bits']), 'little') == int(g['bits'], 16))
                for d, g in zip(dec, got)))
            f['C_oracle_tokens_match'] = bool(same)
            f['C_oracle_tokens'] = got
            cmp_C += 1 if same else 0
        rows_out.append(f)

    # 全精度修正常数 vs 印出来的 3 位小数：**这一条是 407 批的前提**
    const_notes = []
    for f in rows_out:
        for t in f['tokens_decoded']:
            if t['name'] == 'const':
                const_notes.append({'series': f['series'], 'seq': f['seq'], 'value': t['value'],
                                    'value_bits': t['value_bits']})

    prov = {
        'type': 'provenance', 'schema': TOKENS_FIXTURE_SCHEMA,
        'what': ('what the sampler really eats: at `calculate_function` (x86 RVA 0x3c2530) entry we dump the '
                 'token list `&Vec<Token>` (16 B/token) it is handed, the caller, x0/y0/step/flip/mode, and we '
                 'dump every `MathFunction::new` (0x4ed670) call (input text bytes -> token list) so the token '
                 'list can be traced to the exact text it was parsed from. Compared byte-for-byte with the two '
                 'text representations a replay records (`FunctionInfo.function` / `FunctionInfo.corrected_function`).'),
        'build': {
            'game': 'Graphwar II Linux 2.0.60 (Steam)',
            'libgraphwarrust.so_sha256': '9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187',
            'engine': 'Godot 4.7.stable.official.5b4e0cb0f + godot-rust (API v4.6, runtime v4.7)',
            'address_space': 'RVA = ELF virtual address = module offset; Ghidra address = RVA + 0x100000',
            'hooked_rva': {'calculate_function': '0x3c2530', 'math_function_new_0x4ed670': '0x4ed670',
                           'calculate_function_offset_str': '0x3c5f70'},
            'token_layout': 'Vec<Token>{cap@+0, ptr@+8, len@+0x10}; Token = {tag: u64, value: f64} = 16 B',
            'abi': ('calculate_function(sret:rdi, tokens:rsi, flip:edx, mode:ecx, x0:xmm0, y0:xmm1, step:xmm2); '
                    'MathFunction::new(sret:rdi, text:&String rsi)'),
        },
        'how': {
            'harness': 'Xvfb :97 -screen 0 1920x1160x24 -listen tcp -ac  (+ mesa-only EGL/GLX env)',
            'driver': ('tools/frida_point_array.py --attach --tokens  (NOT frida spawn: spawn hangs this build in '
                       'wait4 before it creates a window; see docs/06 42.1)'),
            'flow': 'Popen game -> attach -> Campaign / level node / Start -> type f(x) -> LONG-PRESS Fire!',
            'correlation': ('the sampler hook compares its token list against every parse in a 256-entry ring '
                            'by (a) pointer identity of the token buffer and (b) 16 B/token byte equality'),
        },
        'cut': ('per calculate_function call: caller, flip/mode, x0/y0/step bit patterns, the token list '
                '(hex of 16*len bytes + tags), and the parse record it matches (caller + input text + text hex). '
                'Replay side: only the FunctionInfo row whose corrected_function is byte-identical to the '
                'captured text (no guessing), plus the shot-shaping rows (FunctionPoints / EffectInfo).'),
        'dropped': ('beat heartbeats, evals/samplers bulk rows, the raw gzip replay bytes, screenshots.'),
        'no_local_paths': 'this file contains no filesystem path and no user name; pointers are ASLR-random per run.',
        'sources': sources,
        'replay': ({'sha256': replay_sha, 'function_info_rows': len(fis),
                    'excerpt': replay_keep} if replay_path else None),
        'probe_selftest_at_install': (selftest or {}).get('cmp'),
        'hooks': hooks,
        'const_tokens': const_notes,
        'counts': {'fires': n_fires,
                   'A_tokens_equal_parse': cmp_A,
                   'B_captured_text_equals_replay_corrected': cmp_B,
                   'C_oracle_tokens_equal_captured': (cmp_C if oracle_parse is not None else None),
                   'replay_function_info_rows': len(fis)},
        'notes': notes or [],
    }
    with open(out_path, 'w', encoding='utf-8') as fh:
        fh.write(json.dumps(prov, ensure_ascii=False) + '\n')
        for r in rows_out:
            fh.write(json.dumps(r, ensure_ascii=False) + '\n')
    return prov, rows_out


# ═══════════════════════════════════════════════════════════════════════════════
# 2026-11-08 `dispatch/44`：带 `--fnpoints` 的原始 JSONL → **入库用纯文本样本**
#   一条发看的四件事（`docs/06` §28）：
#     · ctor_A  `0x456810` 在 `gw+471a8d` 那一支的读数（`end_x` = `pts[len-1].x`）
#     · ctor_B  同一个函数在 `gw+3e44d9`（target_manager）那一支的读数
#     · wire    `FunctionPointsEvent → JSON`（`0x50eb20`）那一刻的 `end_x/start_*` 位模式
#     · pos     `0x472865` 那一刻的 `pos`（16 B）与"在每条数组里命中的下标"
#   不发出去的：整条数组 hex、beat、reallocs；不含任何路径/用户名。
# ═══════════════════════════════════════════════════════════════════════════════

def endx_fixture(specs, out_path, exprs_per_series=None, modes=None, levels=None, notes=None):
    """specs = [(raw_jsonl, label)]；exprs_per_series[i] = 该系列按**执行顺序**打进去的表达式表。"""
    import struct as _struct

    def _f64(h):
        try:
            return _struct.unpack('<d', bytes.fromhex(h))[0]
        except Exception:  # noqa: BLE001
            return None

    prov = {
        'schema': 'gw2-frida-endx-sample/1',
        'question': 'dispatch/44：FunctionPoints.end_x 的装配处 vs EffectInfo.pos_x 的读点，同一发里同不同',
        'build': {
            'game': 'Graphwar II Linux 2.0.60 (Steam)',
            'so': 'libgraphwarrust.so',
            'sha256': '9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187',
            'addr_space': 'RVA = ELF 虚地址 = 模块内偏移；下断 = base + RVA；Ghidra = RVA + 0x100000',
        },
        'probe': 'tools/frida_point_array.js（--fnpoints：fnpoints_ctor / fnpoints / pos_x_locate 三条消息）'
                 ' + tools/frida_point_array.py（--fnpoints / --campaign-level / --level-node / --endx-extract）',
        'hooks': [
            {'rva': '0x456810', 'what': 'FunctionPointsEvent 装配（graphwar_function_manager.rs）',
             'reads': 'rdx=&Vec<(f64,f64)>；0x4569b8 movq -0x10(%rcx,%rax,1) = pts[len-1].x → end_x；'
                      '0x4569be movdqu (%rcx) = pts[0] → start_x/start_y；0x4569ce = owner_id(u32)'},
            {'rva': '0x471a88', 'what': '装配点 A（graphwar_game_state_manager.rs 侧调用，喂的是副本）'},
            {'rva': '0x3e44d4', 'what': '装配点 B（graphwar_target_manager.rs 侧调用，喂的是 gw+3e2d3b 那条数组）'},
            {'rva': '0x50eb20', 'what': 'FunctionPointsEvent → JSON 写入器（报文里 end_x 在这里定型）'},
            {'rva': '0x472865', 'what': 'execute_results 里 movdqu -0x10(%rdx,%rcx,1) = pos'},
        ],
        'positive_control': '探针装机时跑 selfTestCompare()：合成数组 相同/翻 1 bit/翻 2 bit 三档均正确；'
                            '运行期也报得出不同（同一发两次 calculate_function 的数组 diffPoints=129/3872/445/103 等）',
        'pointer_note': '样本里的堆/栈指针是**该次运行**的 ASLR 值，只用于判"同一个对象"（same_ptr），跨运行不可比',
        'not_in_file': '整条点数组的十六进制、beat 心跳、reallocs、任何文件系统路径与用户名',
        'series': [],
        'counts': {},
    }
    stats = dict(shots=0, ser_eq_ctorA=0, ser_eq_ctorB=0, pos_eq_ctorA=0, pos_eq_ctorB=0,
                 A_ne_B=0, queue_array_unchanged=0, queue_array_changed=0)
    rows = [prov]
    for i, (path, label) in enumerate(specs):
        raw = [json.loads(l) for l in open(path, encoding='utf-8', errors='replace') if l.strip()]
        mode = (modes or [])[i] if modes and i < len(modes) else None
        level = (levels or [])[i] if levels and i < len(levels) else None
        exprs = []
        if exprs_per_series and i < len(exprs_per_series) and exprs_per_series[i]:
            exprs = [e.strip() for e in exprs_per_series[i].split(',') if e.strip()]
        # 一发的形状：calcA → ctorA(0x471a8d) → SER → calcB → ctorB(0x3e44d9) → pos
        # ⇒ 以 **ctorA** 为发界（SER 之后的 ctorB / pos 属于同一发）
        groups = []
        for r in raw:
            t = r.get('type')
            if t == 'fnpoints_ctor' and r.get('caller') == 'gw+471a8d':
                groups.append({'ctorA': r, 'ctorB': None, 'pos': None, 'ser': None})
                continue
            if not groups:
                continue
            g = groups[-1]
            if t == 'fnpoints_ctor':
                g['ctorB'] = r
            elif t == 'fnpoints':
                g['ser'] = r
            elif t == 'pos_x_locate':
                g['pos'] = r
        groups = [g for g in groups if g.get('ser')]
        prov['series'].append({'label': label, 'raw_series': '(原始 JSONL 未入库)', 'mode': mode,
                               'level': level, 'shots': len(groups)})
        for k, g in enumerate(groups):
            a, b, ser, pos = g['ctorA'], g['ctorB'], g['ser'], g['pos']
            stats['shots'] += 1
            o = {'schema': 'gw2-frida-endx-shot/1', 'series': label, 'mode': mode, 'level': level,
                 'shot': k + 1, 'expr_typed': exprs[k] if k < len(exprs) else None}
            av, bv = (a.get('vec') or {}), ((b or {}).get('vec') or {})
            o['ctor_A'] = {'caller': a.get('caller'), 't_ms': a.get('t'),
                           'vec': {'ptr': av.get('ptr'), 'len': av.get('len')},
                           'end_x_bits': a.get('end_x'), 'end_x': _f64(a.get('end_x')),
                           'start_pt': a.get('start_pt'), 'end_pt': a.get('end_pt'),
                           'owner_id': a.get('owner_id'),
                           'vs_calc': [{'ret': x.get('ret'), 'ptr': x.get('ptr'), 'len': x.get('len'),
                                        'same_ptr': x.get('same_ptr'), 'same_bytes': x.get('same_bytes')}
                                       for x in (a.get('vs_calc') or [])[-2:]]}
            if b:
                o['ctor_B'] = {'caller': b.get('caller'), 't_ms': b.get('t'),
                               'vec': {'ptr': bv.get('ptr'), 'len': bv.get('len')},
                               'end_x_bits': b.get('end_x'), 'end_x': _f64(b.get('end_x')),
                               'start_pt': b.get('start_pt')}
            o['wire'] = {'end_x_bits': ser.get('end_x'), 'end_x': _f64(ser.get('end_x')),
                         'start_x_bits': ser.get('start_x'), 'start_y_bits': ser.get('start_y'),
                         'ret': ser.get('ret')}
            if pos:
                o['pos'] = {'t_ms': pos.get('t'), 'rdx': pos.get('rdx'), 'n': pos.get('n'),
                            'pos_bytes': pos.get('pos_bytes'), 'pos_x_bits': pos.get('pos_x'),
                            'pos_x': _f64(pos.get('pos_x')), 'last_ctor': pos.get('last_ctor'),
                            'vs_calc': [{'ret': x.get('ret'), 'ptr': x.get('ptr'), 'len': x.get('len'),
                                         'same_ptr': x.get('same_ptr'), 'x_idx': x.get('x_idx'),
                                         'xy_idx': x.get('xy_idx'), 'same_bytes': x.get('same_bytes')}
                                        for x in (pos.get('vs_calc') or [])[-2:]]}
            # ── 三条判决（全部**位模式**比较）──
            o['judge_ser_eq_ctorA'] = (a.get('end_x') == ser.get('end_x'))
            if b:
                o['judge_ser_eq_ctorB'] = (b.get('end_x') == ser.get('end_x'))
            if pos:
                o['judge_pos_eq_ctorA'] = (a.get('end_x') == pos.get('pos_x'))
                if b:
                    o['judge_pos_eq_ctorB'] = (b.get('end_x') == pos.get('pos_x'))
            if b:
                o['A_minus_B_end_x'] = (_f64(a.get('end_x')) or 0.0) - (_f64(b.get('end_x')) or 0.0)
                o['A_minus_B_len'] = (av.get('len') or 0) - (bv.get('len') or 0)
            if pos:
                cur = None
                for x in (pos.get('vs_calc') or []):
                    if x.get('same_ptr'):
                        cur = x
                if cur is not None:
                    o['queue_array_bytes_unchanged_at_pos_read'] = cur.get('same_bytes')
            # 统计
            stats['ser_eq_ctorA'] += 1 if o.get('judge_ser_eq_ctorA') else 0
            stats['ser_eq_ctorB'] += 1 if o.get('judge_ser_eq_ctorB') else 0
            stats['pos_eq_ctorA'] += 1 if o.get('judge_pos_eq_ctorA') else 0
            stats['pos_eq_ctorB'] += 1 if o.get('judge_pos_eq_ctorB') else 0
            stats['A_ne_B'] += 1 if (b and a.get('end_x') != b.get('end_x')) else 0
            if o.get('queue_array_bytes_unchanged_at_pos_read') is True:
                stats['queue_array_unchanged'] += 1
            elif o.get('queue_array_bytes_unchanged_at_pos_read') is False:
                stats['queue_array_changed'] += 1
            rows.append(o)
    if notes:
        prov['notes'] = list(notes)
    prov['counts'] = stats
    with open(out_path, 'w', encoding='utf-8') as f:
        for o in rows:
            f.write(json.dumps(o, ensure_ascii=False) + '\n')
    return prov, rows


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
    ap.add_argument('--tokens', action='store_true',
                    help='（2026-11-08 `dispatch/39`）打开 **token 探针**：在 `calculate_function`(0x3c2530) 入口'
                         '抓它真正吃到的 token 列表，并在解析器 0x4ed670 上抓「文本 → token」的每一次调用，'
                         '两者用 ptr 恒等 + 逐字节对撞。不加这个开关时行为与之前**逐字节相同**')
    ap.add_argument('--fnpoints', action='store_true',
                    help='（2026-11-08 `dispatch/44`）打开 **FunctionPoints 探针**：在 '
                         '`FunctionPointsEvent → JSON` 写入器 0x50eb20 入口抓报文里 `end_x` 的 8 字节原文'
                         '（结构体 `start_x@+0/start_y@+8/end_x@+0x10/owner_id@+0x18`），'
                         '并在 0x472865 那次读上把 `pos` 在每条 `calculate_function` 数组里**定位到下标**。'
                         '不加这个开关时行为与之前**逐字节相同**')
    ap.add_argument('--campaign-level', type=int, default=None,
                    help='（`dispatch/44`）开跑前往**工作区** `XDG_DATA_HOME` 写 `user://campaign.save = level_<n>`'
                         '（20 = 解锁全部；给这个开关才会写）')
    ap.add_argument('--level-node', default=None,
                    help='（`dispatch/44`）要点的关卡号（0..20，按内置坐标表）或 "x,y"；不给 = 老行为 `243 224`')
    ap.add_argument('--snap-dir', default=None, help='（调试用）每一步截一张 1920x1160 的 PNG 到这个目录；PNG 不入库')
    # ── 2026-11-08 `dispatch/44`：把 `--fnpoints` 的原始 JSONL 摘成入库样本（不给游戏）──
    ap.add_argument('--endx-extract', default=None, action='append',
                    help='（`dispatch/44`）原始 JSONL（要带 `--fnpoints` 的记录），可给多次')
    ap.add_argument('--endx-fixture-out', default=None,
                    help='（`dispatch/44`）`--endx-extract` 的输出文件（纯文本 JSONL）')
    ap.add_argument('--series-exprs', default=None, action='append',
                    help='（`dispatch/44`）与 `--endx-extract` 逐个对应的"这一系列按**执行顺序**打进去的表达式"')
    ap.add_argument('--series-mode', default=None, help='（`dispatch/44`）Normal,DiffEq,… 与 `--endx-extract` 逐个对应')
    ap.add_argument('--series-level', default=None, help='（`dispatch/44`）关卡号列表，与 `--endx-extract` 逐个对应')
    ap.add_argument('--extract', default=None, action='append',
                    help='不给游戏、只把已有 JSONL 摘成入库样本（可给多次；配合 --fixture-out）')
    ap.add_argument('--fixture-out', default=None, help='`--extract` 的输出文件（纯文本 JSONL）')
    ap.add_argument('--note', default=None, action='append',
                    help='`--extract` 时写进 provenance 的说明（可给多次）')
    ap.add_argument('--label', default=None, action='append',
                    help='`--extract` / `--tokens-report` 时给每批起的标签（顺序对应；缺省用 seriesN）')
    # ── 2026-11-08 `dispatch/39`：token 抓取的**离线**摘要模式（不给游戏）──────────────
    ap.add_argument('--tokens-report', default=None, action='append',
                    help='（`dispatch/39`）不给游戏：把 `--tokens` 跑出来的原始 JSONL 摘成入库样本，'
                         '并做 A/B/C 三种**逐字节**比较（可给多次；配合 --tokens-fixture-out）')
    ap.add_argument('--tokens-fixture-out', default=None, help='`--tokens-report` 的输出文件（纯文本 JSONL）')
    ap.add_argument('--replay', default=None,
                    help='（`dispatch/39`）**同一局**的回放文件（`user://replays/<unix 秒>`，gzip JSONL）；'
                         '给了它才能做"抓到的文本 vs 回放记的 corrected_function"这条同局比较')
    ap.add_argument('--oracle', type=int, default=0,
                    help='（`dispatch/39`）>0 时再用**真二进制**（unicorn 直调 0x4ed670）解析一遍回放里的 '
                         'corrected_function，与抓到的 token 列表逐字节比（需要 EMU 环境，见 tools/eval_oracle.py）')
    args = ap.parse_args()

    if args.endx_extract:
        if not args.endx_fixture_out:
            ap.error('--endx-extract 需要 --endx-fixture-out')
        labels = args.label or []
        specs = [(p, labels[i] if i < len(labels) else 'series%d' % (i + 1))
                 for i, p in enumerate(args.endx_extract)]
        modes = args.series_mode.split(',') if args.series_mode else None
        levels = args.series_level.split(',') if args.series_level else None
        prov, rows = endx_fixture(specs, args.endx_fixture_out, exprs_per_series=args.series_exprs,
                                  modes=modes, levels=levels, notes=args.note)
        c = prov['counts']
        print('[endx-extract] shots=%d wire==ctorA %d | pos==ctorA %d | A!=B %d | queue_bytes_unchanged %d -> %s' % (
            c['shots'], c['ser_eq_ctorA'], c['pos_eq_ctorA'], c['A_ne_B'],
            c['queue_array_unchanged'], args.endx_fixture_out), file=sys.stderr)
        for r in rows[1:]:
            print('  %-10s #%d %-14s A=%s wire=%s pos=%s A-B=%s' % (
                r['series'], r['shot'], r.get('expr_typed'), r['ctor_A'].get('end_x'),
                (r.get('wire') or {}).get('end_x'), (r.get('pos') or {}).get('pos_x'),
                r.get('A_minus_B_end_x')), file=sys.stderr)
        return
    if args.tokens_report:
        if not args.tokens_fixture_out:
            ap.error('--tokens-report 需要 --tokens-fixture-out')
        labels = args.label or []
        specs = [(p, labels[i] if i < len(labels) else 'series%d' % (i + 1))
                 for i, p in enumerate(args.tokens_report)]
        prov, rows = tokens_report(specs, args.tokens_fixture_out, replay_path=args.replay,
                                   notes=args.note, oracle=args.oracle)
        c = prov['counts']
        print('[tokens-report] fires=%d A=%d B=%s C=%s -> %s' % (
            c['fires'], c['A_tokens_equal_parse'], c['B_captured_text_equals_replay_corrected'],
            c['C_oracle_tokens_equal_captured'], args.tokens_fixture_out), file=sys.stderr)
        for r in rows:
            print('[fire %s/%s] caller=%s flip=%s mode=%s len=%s  %s\n           text=%r  A=%s B=%s' % (
                r['series'], r['seq'], r['caller'], r['flip'], r['mode'], r['tokens']['len'],
                r['prefix_text'], (r.get('matched_parse') or {}).get('text'),
                r['A_tokens_eq_parse'], r.get('B_text_eq_replay_corrected')), file=sys.stderr)
        return
    if not args.extract and not args.out:
        ap.error('需要 --out（或者用 --extract / --tokens-report 模式）')
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

    # 2026-11-08 `dispatch/44`：造 DiffEq 局要用 `--campaign-level`（只写工作区 data 目录）
    if args.campaign_level is not None:
        write_campaign_save(data, args.campaign_level)
    # 关卡节点那一步的坐标：`--level-node` 给了就换掉 NAV 的第 2 步（不给 = 老行为）
    nav = list(NAV)
    if args.level_node:
        if ',' in args.level_node:
            xy = args.level_node.replace(',', ' ')
        else:
            n = int(args.level_node)
            if n not in LEVEL_NODE:
                ap.error('--level-node %d 不在内置坐标表里' % n)
            xy = '%d %d' % LEVEL_NODE[n]
        nav[1] = (nav[1][0], 'mousemove %s click 1' % xy)
        print('[nav] level node = %s' % xy, file=sys.stderr)

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
        elif t == 'fnpoints':
            print('[fnpoints] #%s end_x=%s start=(%s,%s) owner=%s ret=%s bt=%s' % (
                p.get('seq'), p.get('end_x'), p.get('start_x'), p.get('start_y'), p.get('owner_id'),
                p.get('ret'), (p.get('bt') or [])[:5]), file=sys.stderr)
            for v in (p.get('vs_calc') or []):
                print('    vs calc ret=%s len=%s last_x_eq=%s prev_x_eq=%s x_idx=%s' % (
                    v.get('ret'), v.get('len'), v.get('last_x_eq'), v.get('prev_x_eq'), v.get('x_eq_idx')),
                    file=sys.stderr)
        elif t == 'fnpoints_ctor':
            v = p.get('vec') or {}
            print('[fnpoints_ctor] #%s caller=%s owner=%s vec=(ptr=%s,len=%s) end_x=%s start_pt=%s end_pt=%s' % (
                p.get('seq'), p.get('caller'), p.get('owner_id'), v.get('ptr'), v.get('len'),
                p.get('end_x'), p.get('start_pt'), p.get('end_pt')), file=sys.stderr)
            for x in (p.get('vs_calc') or []):
                print('    vs calc ret=%s ptr=%s len=%s same_ptr=%s same_bytes=%s' % (
                    x.get('ret'), x.get('ptr'), x.get('len'), x.get('same_ptr'), x.get('same_bytes')),
                    file=sys.stderr)
        elif t == 'pos_x_locate':
            print('[pos_x_locate] #%s n=%s pos_x=%s last_fn=%s' % (
                p.get('seq'), p.get('n'), p.get('pos_x'), p.get('last_fn')), file=sys.stderr)
            for v in (p.get('vs_calc') or []):
                print('    vs calc ret=%s len=%s same_ptr=%s x_idx=%s xy_idx=%s same_bytes=%s' % (
                    v.get('ret'), v.get('len'), v.get('same_ptr'), v.get('x_idx'), v.get('xy_idx'),
                    v.get('same_bytes')), file=sys.stderr)
        elif t == 'pos_read':
            print('[pos_read] #%s rdx=%s rcx=%s n=%s pos=%s vs_queue=%s vs_calc=%s' % (
                p.get('seq'), p.get('rdx'), p.get('rcx'), p.get('n'), p.get('pos_bytes'),
                [(v.get('ptr'), v.get('same_ptr'), v.get('same_len'), v.get('same_digest')) for v in (p.get('vs_queue') or [])],
                [(v.get('ret'), v.get('same'), v.get('diffPoints')) for v in (p.get('vs_calc') or [])]),
                file=sys.stderr)

    dev = frida.get_local_device()
    game = args.game
    argv = [os.path.join(game, 'Graphwar.x86_64'), '--resolution', '1920x1160', '--position', '0,0']
    # 2026-11-08 `dispatch/44` 新坑：本机是 **niri/Wayland** 会话，`WAYLAND_DISPLAY` 一存在
    # Godot 就走 `DisplayServerWayland`，**根本不在 Xvfb 上建 X11 窗口** ⇒ `xdotool search` 一直
    # 返回 `root`、点击全落在根窗口、整轮 0 发（实测：`window=root after 40.4s`）。
    # 这里强制走 X11：把 `WAYLAND_DISPLAY` 摘掉（`--display` 给你的那个 X display 才是目标）。
    env = {k: v for k, v in dict(os.environ, **env).items()
           if k not in ('LC_ALL', 'WAYLAND_DISPLAY')}
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
        # 2026-11-08 `dispatch/39`：`--tokens` 同时打开 token 探针（关着的时候探针三个钩子立即 return）
        script.post({'type': 'cfg', 'evalRowsMax': args.eval_rows,
                     'tokenProbe': bool(args.tokens), 'fnpoints': bool(args.fnpoints)})
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
            for i, (delay, action) in enumerate(nav):
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
