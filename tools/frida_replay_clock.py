#!/usr/bin/env python3
"""tools/frida_replay_clock.py —— `frida_replay_clock.js` 的驱动（T12 / `task-9`，**只读**）。

回答的问题（`docs/00` §0.1b.2 的 **D 档**、`docs/16` §56.6 #7）：
**原版回放播放屏的播放时钟到底怎么走？进屏那一刻 `playing` 初值是什么？** 以及
⏮/⏭ 一次跳多少、推进速率是不是 60.00 tick/s（`docs/06` §44 的真值）。

## 本机唯一能跑通的姿势（沿用 `tools/frida_cadence.py` / `frame_capture.py`）

* `frida spawn` 会把游戏挂在 `wait4` ⇒ **自己 `Popen` + `dev.attach(pid)`**；
* `/tmp` 是逐命令 tmpfs ⇒ 后台进程跨命令不存在 ⇒ **起 Xvfb → 起游戏 → attach → 点击 → 收数据**
  必须在**同一条命令的同一个进程树**里做完（所以整个流程写在一个脚本里）；
* Xvfb 必须 **TCP**（`-listen tcp -ac`）、`DISPLAY=127.0.0.1:<n>`，并 `unset WAYLAND_DISPLAY`，
  否则 Godot 跑在 Wayland 上、窗口不在 Xvfb 里；
* Xvfb 只许看见 mesa（`__EGL_VENDOR_LIBRARY_FILENAMES` / `LIBGLX_VENDOR_LIBRARY_NAME`）；
* `user://` 重定向 = `XDG_DATA/CACHE/CONFIG_HOME` 三个**绝对路径**；
* 回放要在 `user://replays` 里（原版 = `<home>/data/godot/app_userdata/Graphwar/replays`），
  否则回放列表 0 行。

## 点法（坐标来自 `frame_capture.py` / `docs/06` §55.2 的真帧测量）

| 屏 | 动作 | 坐标 |
|---|---|---|
| 主菜单 | Replays | (1343, 749) |
| 回放列表 | 第 1 行的 ▶ | (1805, 132) |
| 回放播放屏 | ⏮ / ▶ / ⏭ | (48, 629) / (148, 629) / (248, 629)（96×96 @y581） |

## 用法

    export PY=/home/CG/Myprojects/graphwar2-rebuild/.reference/venv/bin/python
    $PY tools/frida_replay_clock.py --out <可写目录>/clock.jsonl \\
        --data-dir <可写目录>/gd --display-num 91 --seed-replays

产物：一份 JSONL（`ready` / `hook` / `clock_ptr` / `mark` / `samples` / `events` / `counts`），
`fixtures/replay_clock_observations.tsv` 由 `--tsv` 从这份 JSONL 折出来（可原地复跑）。
"""
import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
sys.path.insert(0, HERE)

JS = os.path.join(HERE, 'frida_replay_clock.js')
REPLAYS_BTN = (1343, 749)          # 主菜单 Replays
REPLAY_ROW1_PLAY = (1805, 132)     # 回放列表第 1 行 ▶（original 侧）
PLAY_SCREEN = {'prev': (48, 629), 'play': (148, 629), 'next': (248, 629)}
SNAPSHOT_REPLAYS = ['1793897298', '1783255495']   # 冻结快照里那两份（**只读**）

MESA_ENV = {
    '__EGL_VENDOR_LIBRARY_FILENAMES': '/usr/share/glvnd/egl_vendor.d/50_mesa.json',
    'LIBGLX_VENDOR_LIBRARY_NAME': 'mesa',
    'LIBGL_ALWAYS_SOFTWARE': '1',
    'GALLIUM_DRIVER': 'llvmpipe',
}


def log(msg):
    print('[replay_clock] %s' % msg, file=sys.stderr, flush=True)


def find_external():
    d = HERE
    for _ in range(10):
        if os.path.isdir(os.path.join(d, '.reference')):
            return d
        d = os.path.dirname(d)
    return SUB


EXT = find_external()
GAME_DIR = os.path.join(EXT, '.reference', 'game')
SNAPSHOT_DIR = os.path.join(EXT, '.reference', 'replays', 'gw2-replays-snapshot', 'replays')


def start_xvfb(num, w, h, logpath):
    env = dict(os.environ, **MESA_ENV)
    f = open(logpath, 'wb')
    p = subprocess.Popen(['Xvfb', ':%d' % num, '-screen', '0', '%dx%dx24' % (w, h),
                          '-listen', 'tcp', '-ac'], env=env, stdout=f, stderr=f)
    time.sleep(1.5)
    return p


def xenv(display):
    env = {k: v for k, v in os.environ.items() if k not in ('WAYLAND_DISPLAY', 'LC_ALL')}
    env['DISPLAY'] = display
    env.setdefault('LC_CTYPE', 'C.UTF-8')
    return env


def xdo(display, action):
    subprocess.run('xdotool ' + action, shell=True, env=xenv(display),
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def win_id(display):
    env = xenv(display)
    for pat in ('^Graphwar$', 'Graphwar'):
        try:
            out = subprocess.run(['xdotool', 'search', '--name', pat], env=env,
                                 capture_output=True, text=True).stdout.split()
        except Exception:  # noqa: BLE001
            out = []
        if out:
            return out[-1]
    return 'root'


def snap(display, path):
    subprocess.run(['import', '-window', win_id(display), path], env=xenv(display),
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def click(display, xy, settle):
    x, y = xy
    xdo(display, 'mousemove %d %d' % (x, y))
    time.sleep(0.20)
    xdo(display, 'click 1')
    time.sleep(settle)


def seed_replays(home):
    """把冻结快照的两份回放拷进**这个 HOME 的** `user://replays`（快照本身只读）。"""
    dst = os.path.join(home, 'data', 'godot', 'app_userdata', 'Graphwar', 'replays')
    os.makedirs(dst, exist_ok=True)
    legacy = os.path.join(home, '.local', 'share', 'godot', 'app_userdata', 'Graphwar', 'replays')
    got = []
    for name in SNAPSHOT_REPLAYS:
        src = os.path.join(SNAPSHOT_DIR, name)
        if not os.path.exists(src):
            log('⚠ 快照里没有 %s（%s）' % (name, SNAPSHOT_DIR))
            continue
        for d in (dst, legacy):
            os.makedirs(d, exist_ok=True)
            shutil.copy2(src, os.path.join(d, name))
        got.append(name)
    return got


def fold_tsv(paths):
    """把一份或多份 JSONL 折成 `fixtures/replay_clock_observations.tsv`（**可原地复跑**）。

    列：`kind / run / phase / t_ms / action / field / value / unit / note`
    * `kind=summary`：一个测量事实一行（速率、跳转量、常量校验……）
    * `kind=sample` ：每 100 ms 一条时基快照（`field` 留空，各列填 `tick`/`p2n`/`p3n`…）
    * `kind=event`  ：钩子事件（inject / toggle_pause / 检查点跳转 / is_paused 返回值）
    """
    NS = 1_000_000_000
    rows = []
    def add(**kw):
        rows.append(kw)
    for path in paths:
        run = os.path.basename(path).replace('.jsonl', '')
        js = [json.loads(l) for l in open(path, encoding='utf-8') if l.strip()]
        marks = {r['label']: r['t_ms'] for r in js if r.get('type') == 'mark'}
        samples = [r for r in js if r.get('type') == 'sample']
        events = [r for r in js if r.get('type') == 'event']
        counts = next((r.get('counts') for r in js if r.get('type') == 'counts'), {}) or {}

        def phase(t):
            ph = ''
            for lab in ('menu', 'replay_lobby', 'enter_play_screen', 'hold_end', 'press_play',
                        'play_end', 'press_next_1', 'next_1_end', 'press_next_2', 'next_2_end',
                        'press_prev_1', 'prev_1_end', 'press_play_2', 'play_2_end'):
                if lab in marks and marks[lab] <= t:
                    ph = lab
            return ph

        def rate(t0, t1):
            ws = [s for s in samples if t0 <= s['t_ms'] <= t1]
            if len(ws) < 2:
                return None
            d = ws[-1]['tick'] - ws[0]['tick']
            dt = (ws[-1]['t_ms'] - ws[0]['t_ms']) / 1000.0
            paused = sum(1 for s in ws if s['p3n'] != NS)
            return d, dt, paused, len(ws)

        # ── summary：载入态 / 播放态 / 跳转 / 常量
        if 'enter_play_screen' in marks and 'hold_end' in marks:
            r = rate(marks['enter_play_screen'], marks['hold_end'])
            if r:
                add(kind='summary', run=run, phase='载入态(不点任何键)', t_ms='',
                    action='clock.tick 变化', field='tick_rate',
                    value='%.2f' % (r[0] / r[1]), unit='tick/s',
                    note='dt=%.2fs dtick=%d 暂停标记已置 %d/%d 采样' % (r[1], r[0], r[2], r[3]))
        if 'press_play' in marks and 'play_end' in marks:
            r = rate(marks['press_play'], marks['play_end'])
            if r:
                ws = [s for s in samples if marks['press_play'] <= s['t_ms'] <= marks['play_end']]
                dp2 = (ws[-1]['p2s'] * NS + ws[-1]['p2n']) - (ws[0]['p2s'] * NS + ws[0]['p2n'])
                add(kind='summary', run=run, phase='按 ▶ 之后', t_ms='', action='clock.tick 变化',
                    field='tick_rate', value='%.2f' % (r[0] / r[1]), unit='tick/s',
                    note='dt=%.2fs dtick=%d 暂停标记已清 %d/%d' % (r[1], r[0],
                                                                  r[3] - r[2], r[3]))
                if r[0]:
                    add(kind='summary', run=run, phase='按 ▶ 之后', t_ms='',
                        action='pair#2 排程时间', field='tick_period',
                        value='%.3f' % (dp2 / r[0]), unit='ns/tick',
                        note='静态常量 16_666_666（0xfe502a @0x51986c）')
        if 'play_2_end' in marks:
            r = rate(marks['play_2_end'] - 2200, marks['play_2_end'])
            if r:
                add(kind='summary', run=run, phase='跳转后再 ▶', t_ms='', action='clock.tick 变化',
                    field='tick_rate', value='%.2f' % (r[0] / r[1]), unit='tick/s',
                    note='dt=%.2fs dtick=%d' % (r[1], r[0]))

        # ── event：钩子事件（跳转量、暂停切换、is_paused 返回值）
        for e in events:
            k = e.get('ev')
            if k in ('tick', 'playerproc'):
                continue
            if k == 'inject':
                add(kind='event', run=run, phase=phase(e['t_ms']), t_ms=e['t_ms'],
                    action='inject_events(caller=%s)' % e.get('caller'), field='until_tick',
                    value=e.get('until_tick'), unit='tick',
                    note='start=%s 跳前 tick=%s 事件数=%s' % (e.get('start'),
                                                             e.get('ntick_before'),
                                                             e.get('nevents')))
            elif k in ('toggle_enter', 'toggle_leave', 'pause_enter', 'pause_leave'):
                c = e.get('clock') or {}
                add(kind='event', run=run, phase=phase(e['t_ms']), t_ms=e['t_ms'],
                    action='%s(caller=%s)' % (k, e.get('caller')), field='p3_nanos',
                    value=c.get('p3n'), unit='ns',
                    note='tick=%s；p3n==1e9 ⇒ 未暂停' % c.get('tick'))
            elif k in ('next_ckpt_enter', 'first_ckpt_enter', 'prev_ckpt_enter'):
                add(kind='event', run=run, phase=phase(e['t_ms']), t_ms=e['t_ms'],
                    action='%s(caller=%s)' % (k, e.get('caller')), field='checkpoints',
                    value=','.join(str(x) for x in (e.get('checkpoints') or [])), unit='tick',
                    note='跳前 tick=%s 检查点数=%s' % (e.get('ntick'), e.get('n_checkpoints')))
            elif k == 'is_paused_ret':
                add(kind='event', run=run, phase=phase(e['t_ms']), t_ms=e['t_ms'],
                    action='is_paused 返回', field='ret', value=e.get('ret'), unit='bool',
                    note='静态规则：ret = (p3n != 1e9)')
            elif k == 'is_paused_load':
                add(kind='event', run=run, phase=phase(e['t_ms']), t_ms=e['t_ms'],
                    action='is_paused 内 0x3d7674 读 [rbx+0x40]', field='p3_nanos',
                    value=e.get('nanos_p3'), unit='ns',
                    note='paused=%s' % e.get('paused_by_static_rule'))

        # ── sample：每 100 ms 一条（audit 用）
        for s in samples:
            add(kind='sample', run=run, phase=phase(s['t_ms']), t_ms=s['t_ms'],
                action=s.get('reason') or 'timer', field='', value='', unit='',
                note='tick=%s p1=%s.%09d p2=%s.%09d p3=%s.%09d' % (
                    s.get('tick'), s.get('p1s'), s.get('p1n') or 0,
                    s.get('p2s'), s.get('p2n') or 0, s.get('p3s'), s.get('p3n') or 0))
        add(kind='summary', run=run, phase='整轮', t_ms='', action='钩子计数', field='counts',
            value=json.dumps(counts, sort_keys=True), unit='次', note='')

    cols = ['kind', 'run', 'phase', 't_ms', 'action', 'field', 'value', 'unit', 'note']
    out = ['# fixtures/replay_clock_observations.tsv —— 原版回放播放时钟的**真机读数**'
           '（T12 / task-9，2026-11-26）',
           '# 生成命令： `.reference/venv/bin/python tools/frida_replay_clock.py --tsv '
           '<run>.jsonl > fixtures/replay_clock_observations.tsv`',
           '# 读法：`kind=summary` 是结论行（tick 率 / 每 tick 纳秒 / 整轮钩子计数）；'
           '`kind=event` 是钩子事件；',
           '#       `kind=sample` 是每 100 ms 一条时基快照（`p3_nanos == 1000000000` ⇒ **没暂停**）。',
           '# 地址口径：ELF 虚地址（Ghidra = 本地址 + 0x100000）。构建 = '
           'steam/2026-08-28 libgraphwarrust.so。',
           '\t'.join(cols)]
    for r in rows:
        out.append('\t'.join(str(r.get(c, '')).replace('\t', ' ') for c in cols))
    return '\n'.join(out) + '\n'


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('--out', default=None, help='JSONL 产物（真机模式必给）')
    ap.add_argument('--data-dir', default=None, help='这个 HOME（XDG_* 全指向它下面）')
    ap.add_argument('--tsv', nargs='*', default=None,
                    help='只折叠：给 JSONL，打印 TSV（不跑真机）')
    ap.add_argument('--snap-dir', default=None, help='截图目录（可选，仅留现场，不入库）')
    ap.add_argument('--display-num', type=int, default=91)
    ap.add_argument('--game-dir', default=GAME_DIR)
    ap.add_argument('--game-arg', action='append', default=[])
    ap.add_argument('--settle', type=float, default=6.0, help='进主菜单后等多久（llvmpipe 慢）')
    ap.add_argument('--hold', type=float, default=5.0, help='进播放屏后**不点任何键**观测多久')
    ap.add_argument('--after-play', type=float, default=6.0, help='按 ▶ 之后观测多久')
    ap.add_argument('--after-jump', type=float, default=3.0, help='每次 ⏮/⏭ 之后观测多久')
    ap.add_argument('--no-xvfb', action='store_true')
    a = ap.parse_args()

    if a.tsv is not None:
        sys.stdout.write(fold_tsv(a.tsv))
        return 0
    if not a.data_dir:
        ap.error('真机模式需要 --data-dir')
    out_path = os.path.abspath(a.out)
    os.makedirs(os.path.dirname(out_path) or '.', exist_ok=True)
    data = os.path.abspath(a.data_dir)
    home = os.path.join(data, 'home')
    # ⚠ `user://` 的落点 = `$XDG_DATA_HOME/godot/app_userdata/<项目名>/`；HOME 与 XDG_* 必须
    #   指向**同一棵树**（`frame_capture.py` 的 `XDG_DATA_HOME = <home>/data` 就是这个意思）。
    #   第一跑把回放拷到了 `<data>/home/data/...` 而 XDG_DATA_HOME 是 `<data>/data/...`
    #   ⇒ 回放列表 0 行（截图 `02_replay_lobby.png` 是空列表），点击全打在空气上。
    for sub in ('data', 'cache', 'config'):
        os.makedirs(os.path.join(home, sub), exist_ok=True)
    os.makedirs(home, exist_ok=True)
    seeded = seed_replays(home)
    log('回放已就位: %s -> %s' % (seeded, os.path.join(home, 'data', 'godot')))

    import frida

    xvfb = None
    if not a.no_xvfb:
        start_xvfb(a.display_num, 1920, 1160, os.path.join(os.path.dirname(out_path), 'xvfb.log'))
        display = '127.0.0.1:%d' % a.display_num
        xvfb = True
    else:
        display = os.environ['DISPLAY']
    log('DISPLAY=%s HOME=%s' % (display, home))

    genv = dict(MESA_ENV)
    genv.update({
        'DISPLAY': display,
        'HOME': home,
        'XDG_DATA_HOME': os.path.join(home, 'data'),
        'XDG_CACHE_HOME': os.path.join(home, 'cache'),
        'XDG_CONFIG_HOME': os.path.join(home, 'config'),
        'LC_CTYPE': 'C.UTF-8',
    })
    env = {k: v for k, v in dict(os.environ, **genv).items()
           if k not in ('LC_ALL', 'WAYLAND_DISPLAY')}
    argv = [os.path.join(a.game_dir, 'Graphwar.x86_64'),
            '--resolution', '1920x1160', '--position', '0,0'] + (a.game_arg or [])
    log('popen %s' % argv)
    proc = subprocess.Popen(argv, env=env, cwd=a.game_dir,
                            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    pid = proc.pid
    log('pid=%d' % pid)

    t_wait = time.time()
    while time.time() - t_wait < 90:
        if win_id(display) != 'root':
            break
        if proc.poll() is not None:
            log('[fatal] 游戏退出 rc=%s' % proc.returncode)
            log(proc.stderr.read().decode('utf-8', 'replace')[-4000:])
            return 2
        time.sleep(0.5)
    log('window=%s after %.1fs' % (win_id(display), time.time() - t_wait))

    fout = open(out_path, 'w', encoding='utf-8')
    ready = {'base': None}

    def emit(obj):
        fout.write(json.dumps(obj, ensure_ascii=False) + '\n')
        fout.flush()

    def on_message(msg, _data):
        if msg.get('type') == 'error':
            log('[js-error] %s' % msg.get('description'))
            emit({'type': 'js_error', 'msg': msg.get('description')})
            return
        p = msg.get('payload') or {}
        t = p.get('type', '?')
        if t == 'ready':
            ready['base'] = p.get('base')
            log('[ready] base=%s size=%s time_src=%s' % (p.get('base'), p.get('size'), p.get('time_src')))
        elif t == 'hook':
            log('[hook] %-34s %s' % (p.get('name'), p.get('rva')))
        elif t == 'clock_ptr':
            log('[clock] ptr=%s at=%s' % (p.get('ptr'), p.get('at')))
        elif t == 'mark':
            log('[mark] %-16s t=%dms' % (p.get('label'), p.get('t_ms')))
        if t in ('ready', 'hook', 'clock_ptr', 'mark', 'error', 'js_error'):
            emit(dict(p, type=t))

    dev = frida.get_local_device()
    session = dev.attach(pid)
    script = session.create_script(open(JS, encoding='utf-8').read(), runtime='v8')
    script.on('message', on_message)
    script.load()
    t0 = time.time()

    def mark(label):
        try:
            return script.exports_sync.mark(label)
        except Exception as e:  # noqa: BLE001
            log('[mark-warn] %s' % e)
            return None

    def shot(label):
        if not a.snap_dir:
            return
        os.makedirs(a.snap_dir, exist_ok=True)
        p = os.path.join(a.snap_dir, label + '.png')
        snap(display, p)
        log('[snap] %s' % p)

    try:
        if not ready['base']:
            log('[warn] 没有收到 ready：钩子可能没挂上')
        time.sleep(a.settle)
        shot('01_menu')
        mark('menu')

        # 主菜单 → 回放列表
        click(display, REPLAYS_BTN, 2.5)
        shot('02_replay_lobby')
        mark('replay_lobby')

        # 回放列表第 1 行 ▶ → 回放播放屏（**进屏那一刻 = 本节的核心读数**）
        click(display, REPLAY_ROW1_PLAY, 1.2)
        mark('enter_play_screen')
        shot('03_enter_play_screen')
        log('[*] 进屏后**不点任何键**观测 %.1fs' % a.hold)
        time.sleep(a.hold)
        mark('hold_end')
        shot('04_after_hold')

        # ▶（第一颗键）：播放
        click(display, PLAY_SCREEN['play'], 0.4)
        mark('press_play')
        shot('05_press_play')
        log('[*] 按 ▶ 后观测 %.1fs' % a.after_play)
        time.sleep(a.after_play)
        mark('play_end')
        shot('06_after_play')

        # ⏭ 一次
        click(display, PLAY_SCREEN['next'], 0.4)
        mark('press_next_1')
        time.sleep(a.after_jump)
        mark('next_1_end')
        shot('07_after_next1')

        # ⏭ 再一次
        click(display, PLAY_SCREEN['next'], 0.4)
        mark('press_next_2')
        time.sleep(a.after_jump)
        mark('next_2_end')

        # ⏮ 一次
        click(display, PLAY_SCREEN['prev'], 0.4)
        mark('press_prev_1')
        time.sleep(a.after_jump)
        mark('prev_1_end')
        shot('08_after_prev1')

        # ▶/⏸ 再切一次（看它是不是把时钟冻住）
        click(display, PLAY_SCREEN['play'], 0.4)
        mark('press_play_2')
        time.sleep(a.after_jump)
        mark('play_2_end')
        shot('09_after_play2')

        dump = script.exports_sync.dump()
        emit({'type': 'counts', 'counts': dump.get('counts'), 'clock_ptr': dump.get('clock_ptr'),
              't_ms': dump.get('t_ms')})
        for s in dump.get('samples') or []:
            emit(dict(s, type='sample'))
        for e in dump.get('events') or []:
            emit(dict(e, type='event'))
        log('[*] samples=%d events=%d counts=%s' % (
            len(dump.get('samples') or []), len(dump.get('events') or []), dump.get('counts')))
    finally:
        fout.close()
        try:
            session.detach()
        except Exception:  # noqa: BLE001
            pass
        try:
            proc.send_signal(signal.SIGTERM)
            proc.wait(timeout=10)
        except Exception:  # noqa: BLE001
            try:
                proc.kill()
            except Exception:  # noqa: BLE001
                pass
        if xvfb:
            subprocess.run(['pkill', '-f', 'Xvfb :%d' % a.display_num],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        log('JSONL -> %s' % out_path)
    return 0


if __name__ == '__main__':
    sys.exit(main())
