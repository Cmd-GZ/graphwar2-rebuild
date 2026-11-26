#!/usr/bin/env python3
"""tools/frida_audio_map.py —— `frida_audio_map.js` 的驱动（T19 / `task-16`）。

回答的问题：`docs/17` §5.1.1 那条「音效 ↔ 事件的映射 = 未知」。
预注册（跑之前写死）：`fixtures/audio_event_map_plan.txt`。

## 本机唯一能跑通的姿势（沿用 `tools/frida_replay_clock.py` / `frame_capture.py`）

* `frida spawn` 会把游戏挂在 `wait4` ⇒ **自己 `Popen` + `dev.attach(pid)`**；
* `/tmp` 是逐命令 tmpfs、后台进程跨命令不存在 ⇒ **起 Xvfb → 起游戏 → attach → 点击 → 收数据**
  必须在**同一条命令的同一个进程树**里做完（所以整个流程写在一个脚本里）；
* Xvfb 必须 **TCP**（`-listen tcp -ac`）、`DISPLAY=127.0.0.1:<n>`，并 `unset WAYLAND_DISPLAY`，
  否则 Godot 跑在 Wayland 上、窗口不在 Xvfb 里；
* `user://` 重定向 = `XDG_DATA/CACHE/CONFIG_HOME` 三个**绝对路径**；
  回放要落在 `$XDG_DATA_HOME/godot/app_userdata/Graphwar/replays`，否则回放列表 0 行；
* **只往这个 HOME 里放一份回放**（冻结快照只读，`shutil.copy2` 出来），
  这样回放列表第 1 行一定是它 —— 点击坐标才是确定的。

## 点法（坐标来自 `tools/frame_capture.py` / `docs/06` §55.2 的真帧测量）

| 屏 | 动作 | 坐标 |
|---|---|---|
| 主菜单 | Replays | (1343, 749) |
| 回放列表 | 第 1 行的 ▶ | (1805, 132) |
| 回放播放屏 | ⏮ / ▶ / ⏭ | (48, 629) / (148, 629) / (248, 629) |

## 用法

    .reference/venv/bin/python tools/frida_audio_map.py \\
        --out <可写目录>/run1.jsonl --data-dir <可写目录>/gd \\
        --display-num 119 --replay 1793897298 --after-play 46

产物：一份 JSONL，每行一条观测（`ready`/`hook`/`mark`/`load`/`humm_read`/`ctor`/`eaud`/
`eaud_vol`/`sstr`/`sstr2`/`play`/`qslot`/`eff`/`sold`/`gst`/`entrm`/`w_*`/`proc`/`counts`）。
**不入库**（含运行时指针）；入库的是由它折出来的 `fixtures/audio_event_map.tsv`。
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
JS = os.path.join(HERE, 'frida_audio_map.js')

REPLAYS_BTN = (1343, 749)          # 主菜单 Replays
REPLAY_ROW1_PLAY = (1805, 132)     # 回放列表第 1 行 ▶
PLAY_BTN = (148, 629)              # 回放播放屏 ▶
SNAPSHOT_REL = os.path.join('.reference', 'replays', 'gw2-replays-snapshot', 'replays')

MESA_ENV = {
    '__EGL_VENDOR_LIBRARY_FILENAMES': '/usr/share/glvnd/egl_vendor.d/50_mesa.json',
    'LIBGLX_VENDOR_LIBRARY_NAME': 'mesa',
    'LIBGL_ALWAYS_SOFTWARE': '1',
    'GALLIUM_DRIVER': 'llvmpipe',
}


def log(msg):
    print('[audio_map] %s' % msg, file=sys.stderr, flush=True)


def find_external():
    d = HERE
    for _ in range(10):
        if os.path.isdir(os.path.join(d, '.reference')):
            return d
        d = os.path.dirname(d)
    return os.path.dirname(HERE)


EXT = find_external()
GAME_DIR = os.path.join(EXT, '.reference', 'game')
SNAPSHOT_DIR = os.path.join(EXT, SNAPSHOT_REL)


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


def seed_replay(home, name):
    """把**一份**冻结回放拷进这个 HOME 的 `user://replays`（快照本身只读）。"""
    dst = os.path.join(home, 'data', 'godot', 'app_userdata', 'Graphwar', 'replays')
    legacy = os.path.join(home, '.local', 'share', 'godot', 'app_userdata', 'Graphwar', 'replays')
    src = os.path.join(SNAPSHOT_DIR, name)
    if not os.path.exists(src):
        raise SystemExit('快照里没有 %s（%s）' % (name, SNAPSHOT_DIR))
    for d in (dst, legacy):
        os.makedirs(d, exist_ok=True)
        for old in os.listdir(d):        # 只留这一份，保证「第 1 行」确定
            try:
                os.unlink(os.path.join(d, old))
            except OSError:
                pass
        shutil.copy2(src, os.path.join(d, name))
    return name


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('--out', required=True, help='JSONL 产物')
    ap.add_argument('--data-dir', required=True, help='这个 HOME（XDG_* 全指向它下面）')
    ap.add_argument('--replay', default='1793897298', help='冻结快照里的回放名（预注册选定）')
    ap.add_argument('--snap-dir', default=None, help='截图目录（可选，仅留现场，不入库）')
    ap.add_argument('--display-num', type=int, default=119)
    ap.add_argument('--game-dir', default=GAME_DIR)
    ap.add_argument('--settle', type=float, default=6.0, help='进主菜单后等多久（llvmpipe 慢）')
    ap.add_argument('--hold', type=float, default=4.0, help='进播放屏后**不按键**观测多久（载入期音效）')
    ap.add_argument('--after-play', type=float, default=46.0, help='按 ▶ 之后观测多久（整场 2276 tick≈38 s）')
    a = ap.parse_args()

    out_path = os.path.abspath(a.out)
    os.makedirs(os.path.dirname(out_path) or '.', exist_ok=True)
    data = os.path.abspath(a.data_dir)
    home = os.path.join(data, 'home')
    for sub in ('data', 'cache', 'config'):
        os.makedirs(os.path.join(home, sub), exist_ok=True)
    seeded = seed_replay(home, a.replay)
    log('回放已就位: %s -> %s' % (seeded, os.path.join(home, 'data', 'godot')))

    import frida

    start_xvfb(a.display_num, 1920, 1160, os.path.join(os.path.dirname(out_path), 'xvfb.log'))
    display = '127.0.0.1:%d' % a.display_num
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
            '--resolution', '1920x1160', '--position', '0,0']
    log('popen %s' % argv)
    proc = subprocess.Popen(argv, env=env, cwd=a.game_dir,
                            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    pid = proc.pid
    log('pid=%d' % pid)


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
            log('[ready] base=%s size=%s' % (p.get('base'), p.get('size')))
        elif t == 'hook':
            log('[hook] %-38s %s' % (p.get('name'), p.get('rva')))
        elif t == 'mark':
            log('[mark] %-18s t=%dms tick=%s' % (p.get('label'), p.get('t_ms'), p.get('tick')))

    dev = frida.get_local_device()
    session = dev.attach(pid)      # ← 进程一起来就 attach（不等窗口），否则会漏掉启动期的 load/ctor
    script = session.create_script(open(JS, encoding='utf-8').read(), runtime='v8')
    script.on('message', on_message)
    script.load()

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

        click(display, REPLAYS_BTN, 2.5)
        shot('02_replay_lobby')
        mark('replay_lobby')

        # 回放列表第 1 行 ▶ → 回放播放屏（**载入**：事件被批量注入，pop/unpop 应该在这一段）
        click(display, REPLAY_ROW1_PLAY, 1.2)
        mark('enter_play_screen')
        shot('03_enter_play_screen')
        log('[*] 进屏后**不点任何键**观测 %.1fs（载入期音效）' % a.hold)
        time.sleep(a.hold)
        mark('hold_end')
        shot('04_after_hold')

        # ▶：开始整场播放（tick 1 → 2276 ≈ 38 s）
        click(display, PLAY_BTN, 0.4)
        mark('press_play')
        shot('05_press_play')
        log('[*] 按 ▶ 后观测 %.1fs' % a.after_play)
        time.sleep(a.after_play)
        mark('play_end')
        shot('06_after_play')

        dump = script.exports_sync.dump()
        emit({'type': 'counts', 'counts': dump.get('counts'),
              'clock_ptr': dump.get('clock_ptr'), 'tick_end': dump.get('tick_end'),
              't_ms': dump.get('t_ms')})
        for e in dump.get('events') or []:
            emit(e)
        log('[*] events=%d counts=%s' % (len(dump.get('events') or []), dump.get('counts')))
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
        subprocess.run(['pkill', '-f', 'Xvfb :%d' % a.display_num],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        log('JSONL -> %s' % out_path)
    return 0


if __name__ == '__main__':
    sys.exit(main())
