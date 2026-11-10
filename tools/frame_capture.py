#!/usr/bin/env python3
"""tools/frame_capture.py —— **一侧**的视觉帧采集器（`dispatch/94`，原版 / 我方同一套点击脚本）。

为什么写成"一个脚本跑完一整侧"：

* 本机的 bash 每条命令是一个**独立 PID namespace** ⇒ 后台 `nohup` 起的游戏在下一条命令里
  **不存在**（实测：窗口 id 还在、进程没了；frida 也看不到）。所以"起 Xvfb → 起游戏 → 点 → 截"
  必须在**同一个进程树**里完成；
* `/tmp` 是逐命令 tmpfs ⇒ **不能用 unix socket**，Xvfb 必须 `-listen tcp -ac`，`DISPLAY=127.0.0.1:<n>`；
* 本机是 Wayland 会话（`DISPLAY=:0` / `WAYLAND_DISPLAY=wayland-1` 都在环境里）⇒ 不显式盖掉
  `DISPLAY`、不 `unset WAYLAND_DISPLAY`，Godot 会直接跑在 Wayland 上（窗口不在 Xvfb 里）；
* Vulkan 不可用 ⇒ 两路都回落 OpenGL3（日志里留证）；
* `import -window root` 在无合成器的 Xvfb 上是**全黑**的，必须按**窗口 id** 抓。

固定的东西（写死在 `fixtures/frame_diff_thresholds.txt` §1，本脚本只是执行者）：

    私有 Xvfb 1920x1160x24 (TCP) ｜ 游戏 `--resolution 1920x1160 --position 0,0`
    mesa llvmpipe（GLX/EGL 都只给 mesa）｜ 取帧 = ImageMagick `import -window <id>`
    三屏的统一点击脚本见 §STEPS；每屏 3 帧、帧间隔 1.2 s

用法（两路各跑一次；产物 = PNG + `manifest.json` + 两侧日志）：

    python3 tools/frame_capture.py --side original --out <可写目录>/orig --display-num 81
    python3 tools/frame_capture.py --side ours     --out <可写目录>/mine --display-num 82

`--side ours` 需要 `client/libgw2_rebuild.so` 已就位（`cargo build --features gdext-bindings`
之后拷进去），否则 gdext 类全空、界面起不来。

**只观测**：不改游戏、不改客户端代码，只发 X 事件与截图。
"""
import argparse
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))


def _find_repo():
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
GAME_DIR = os.path.join(REPO, '.reference', 'game')
GODOT = os.path.join(REPO, '.reference', 'godot', 'Godot_v4.7.2-stable_linux.x86_64')
CLIENT = os.path.join(REPO, 'client')

# ── 1920x1160 下的点击坐标（两路**同一串**；来自原版实测帧，见 docs/06 §33 / §52）──────
CAMPAIGN_BTN = (534, 629)     # 主菜单 Campaign
LEVEL_NODE_0 = (241, 228)     # 关卡网格第 0 关（蛇形编号从左上起）
START_BTN = (1074, 1009)      # 关卡弹窗 Start
KEY_X = (1625, 730)           # 表达式键盘上的 `x`
FIRE_BTN = (1855, 55)         # Fire!（**long_press**：必须 mousedown→sleep→mouseup）

FRAMES_PER_SCREEN = 3
FRAME_GAP_S = 1.2

# ── 增补 1（`fixtures/frame_diff_thresholds_amend1.txt` §3）：用冻结回放当"固定局面" ──
REPLAYS_BTN = (1343, 749)          # 主菜单 Replays
# 回放列表第 1 行（局面 `1793897298`，Level 0）的播放键：**两侧坐标不同**
# —— 因为两边的列表行本身长得不一样（原版 3 列图标 + ▶ 在最右；我方多出文件名/大小列）。
REPLAY_ROW1_PLAY = {'original': (1805, 132), 'ours': (1619, 124)}
REPLAY_END_WAIT = 45.0             # 2276 tick ÷ 60 = 37.9 s ⇒ 45 s 一定播到文件末尾

# 冻结快照里的两份回放（**只拷、不改**；快照本身是全仓库三轴数字的分母，禁止往里写）
SNAPSHOT_REPLAYS = ['1793897298', '1783255495']


def snapshot_dir():
    return os.path.join(REPO, '.reference', 'replays', 'gw2-replays-snapshot', 'replays')


def user_replays_dir(side, home):
    """两侧各自的 `user://replays`。

    ⚠️ **本脚本给两侧都设了 `XDG_DATA_HOME=<home>/data`，原版也是 Godot 4.7**
    （不是 3.x）⇒ 它的 `user://` 落在 `<home>/data/godot/app_userdata/Graphwar/`，
    **不是** `~/.local/share/...`。第一跑就踩了：把回放拷进 `~/.local/share/...`，
    原版回放列表**空**（列表屏 0 行），于是 S2/S3/S4 全成了"跟空列表比"。
    """
    if side == 'original':
        return os.path.join(home, 'data', 'godot', 'app_userdata',
                            'Graphwar', 'replays')
    return os.path.join(home, 'data', 'godot', 'app_userdata',
                        'Graphwar II — Client Rebuild', 'replays')


def legacy_replays_dir(side, home):
    """另一种可能的落点（原版不设 XDG 时在 `~/.local/share`）——两边都拷一份，稳妥。"""
    if side != 'original':
        return None
    return os.path.join(home, '.local', 'share', 'godot', 'app_userdata',
                        'Graphwar', 'replays')


def seed_replays(side, home):
    """把快照里的两份回放拷进该侧的 user://replays；返回拷进去的文件名。"""
    src = snapshot_dir()
    dirs = [user_replays_dir(side, home)]
    legacy = legacy_replays_dir(side, home)
    if legacy:
        dirs.append(legacy)
    got = []
    for name in SNAPSHOT_REPLAYS:
        p = os.path.join(src, name)
        if not os.path.exists(p):
            raise RuntimeError('冻结快照里找不到回放 %s' % p)
        for d in dirs:
            os.makedirs(d, exist_ok=True)
            shutil.copy2(p, os.path.join(d, name))
        got.append(name)
    return got


def steps_for(flow, side):
    """返回 [(屏名, [ ('click', xy, settle) | ('hold', xy, hold, settle) | ('wait', s) ])]。"""
    if flow == 'campaign':
        return [
            ('menu', []),
            ('prep', [
                ('click', CAMPAIGN_BTN, 1.5),
                ('click', LEVEL_NODE_0, 1.5),
                ('click', START_BTN, 3.0),
            ]),
            ('shot', [
                ('click', KEY_X, 0.6),
                ('hold', FIRE_BTN, 2.0, 6.0),
            ]),
        ]
    if flow == 'replay':
        return [
            ('menu', []),
            ('replay_lobby', [('click', REPLAYS_BTN, 2.0)]),
            ('replay_play', [('click', REPLAY_ROW1_PLAY[side], 2.0)]),
            ('replay_end', [('wait', REPLAY_END_WAIT)]),
        ]
    raise SystemExit('未知 flow: %s' % flow)

MESA_ENV = {
    '__EGL_VENDOR_LIBRARY_FILENAMES': '/usr/share/glvnd/egl_vendor.d/50_mesa.json',
    'LIBGLX_VENDOR_LIBRARY_NAME': 'mesa',
    'LIBGL_ALWAYS_SOFTWARE': '1',
    'GALLIUM_DRIVER': 'llvmpipe',
}


def log(msg):
    print('[frame_capture] %s' % msg, file=sys.stderr, flush=True)


def run(cmd, env, timeout=30, check=False):
    p = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=timeout)
    if check and p.returncode != 0:
        raise RuntimeError('%s -> rc=%d %s' % (cmd, p.returncode, p.stderr[-400:]))
    return p


def wait_display(display, env, seconds=20.0):
    """等 Xvfb 能连上（`xdotool getdisplaygeometry` 成功即算起来）。"""
    t0 = time.time()
    while time.time() - t0 < seconds:
        p = run(['xdotool', 'getdisplaygeometry'], env)
        if p.returncode == 0:
            return True
        time.sleep(0.2)
    return False


def wait_window(display, env, pattern, seconds=40.0):
    """等游戏窗口出现；返回窗口 id（`xdotool search --name`）。"""
    t0 = time.time()
    while time.time() - t0 < seconds:
        p = run(['xdotool', 'search', '--name', pattern], env)
        ids = p.stdout.split()
        if ids:
            return ids[-1]
        time.sleep(0.25)
    return None


def window_geometry(win, env):
    p = run(['xdotool', 'getwindowgeometry', '--shell', win], env)
    geo = {}
    for line in p.stdout.splitlines():
        if '=' in line:
            k, v = line.split('=', 1)
            geo[k] = v
    return geo


def xdo(env, *args, timeout=20):
    return run(['xdotool'] + list(args), env, timeout=timeout)


def click(env, xy, settle):
    x, y = xy
    xdo(env, 'mousemove', str(x), str(y))
    time.sleep(0.15)
    xdo(env, 'click', '1')
    time.sleep(settle)


def hold(env, xy, hold_s, settle):
    x, y = xy
    xdo(env, 'mousemove', str(x), str(y))
    time.sleep(0.15)
    xdo(env, 'mousedown', '1')
    time.sleep(hold_s)
    xdo(env, 'mouseup', '1')
    time.sleep(settle)


def snap(env, win, path):
    p = run(['import', '-window', win, path], env, timeout=60)
    if p.returncode != 0 or not os.path.exists(path):
        raise RuntimeError('import 失败: %s' % p.stderr[-400:])
    with open(path, 'rb') as f:
        data = f.read()
    return {'file': os.path.basename(path), 'sha256': hashlib.sha256(data).hexdigest(),
            'bytes': len(data)}


def start_xvfb(num, log_path):
    env = dict(os.environ, **MESA_ENV)
    env.pop('WAYLAND_DISPLAY', None)
    display = '127.0.0.1:%d' % num
    # TCP 端口（6000+n）已被占 ⇒ 说明这个 display 有人用，直接报错（**不换号**，避免截到别人的窗口）
    f = open(log_path, 'wb')
    proc = subprocess.Popen(
        ['Xvfb', ':%d' % num, '-screen', '0', '1920x1160x24',
         '-listen', 'tcp', '-ac', '-nolisten', 'unix'],
        stdout=f, stderr=subprocess.STDOUT, env=env)
    return proc, display, env, f


def start_game(side, display, home, log_path, extra_args):
    env = dict(os.environ, **MESA_ENV)
    env.pop('WAYLAND_DISPLAY', None)
    env['DISPLAY'] = display
    env['HOME'] = home
    if side == 'original':
        env['XDG_DATA_HOME'] = os.path.join(home, 'data')
        env['XDG_CACHE_HOME'] = os.path.join(home, 'cache')
        env['XDG_CONFIG_HOME'] = os.path.join(home, 'config')
        argv = [os.path.join(GAME_DIR, 'Graphwar.x86_64'),
                '--resolution', '1920x1160', '--position', '0,0'] + extra_args
        cwd = GAME_DIR
        title = 'Graphwar'
    else:
        env['XDG_DATA_HOME'] = os.path.join(home, 'data')
        env['XDG_CACHE_HOME'] = os.path.join(home, 'cache')
        env['XDG_CONFIG_HOME'] = os.path.join(home, 'config')
        argv = [GODOT, '--path', CLIENT, '--rendering-driver', 'opengl3',
                '--resolution', '1920x1160', '--position', '0,0'] + extra_args
        cwd = CLIENT
        title = 'Graphwar'
    for d in ('data', 'cache', 'config'):
        os.makedirs(os.path.join(home, d), exist_ok=True)
    f = open(log_path, 'wb')
    proc = subprocess.Popen(argv, cwd=cwd, env=env, stdout=f, stderr=subprocess.STDOUT)
    return proc, env, title, f


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--side', choices=['original', 'ours'], required=True)
    ap.add_argument('--out', required=True, help='产物目录（PNG + manifest.json + 日志）')
    ap.add_argument('--display-num', type=int, default=81)
    ap.add_argument('--settle-boot', type=float, default=4.0,
                    help='窗口出现到第一次截图之间等多久（默认 4 s，写死在预注册 §2）')
    ap.add_argument('--home', default=None, help='HOME（默认 <out>/home）')
    ap.add_argument('--game-arg', action='append', default=[], help='透传给游戏的额外参数')
    ap.add_argument('--shots', default=None, help='截图输出子目录（默认 <out>/frames）')
    ap.add_argument('--flow', choices=['campaign', 'replay'], default='replay',
                    help='点法：replay = 增补 1 的四屏（回放局面，默认）；campaign = 原登记的三屏')
    ap.add_argument('--seed-replays', action='store_true',
                    help='把冻结快照里的两份回放拷进该侧 user://replays（不写快照）')
    args = ap.parse_args(argv)

    out = os.path.abspath(args.out)
    frames_dir = os.path.abspath(args.shots or os.path.join(out, 'frames'))
    home = os.path.abspath(args.home or os.path.join(out, 'home'))
    for d in (out, frames_dir, home):
        os.makedirs(d, exist_ok=True)

    manifest = {'side': args.side, 'flow': args.flow,
                'display': '127.0.0.1:%d' % args.display_num,
                'out': out, 'frames_dir': frames_dir, 'home': home,
                'started_at': time.strftime('%Y-%m-%dT%H:%M:%S'),
                'frames_per_screen': FRAMES_PER_SCREEN, 'frame_gap_s': FRAME_GAP_S,
                'steps': [], 'frames': [], 'notes': []}

    if args.seed_replays:
        manifest['replays_seeded'] = seed_replays(args.side, home)
        log('回放已就位: %s' % manifest['replays_seeded'])

    xvfb = game = None
    xlog = flog = None
    ok = False
    try:
        xvfb, display, env, xlog = start_xvfb(args.display_num, os.path.join(out, 'xvfb.log'))
        if not wait_display(display, env):
            raise RuntimeError('Xvfb %s 起不来（见 xvfb.log）' % display)
        log('Xvfb %s 就绪' % display)

        game, genv, title, flog = start_game(args.side, display, home,
                                             os.path.join(out, 'game.log'), args.game_arg)
        win = wait_window(display, genv, title)
        if not win:
            raise RuntimeError('等不到游戏窗口（见 game.log）')
        geo = window_geometry(win, genv)
        manifest['window'] = {'id': win, 'geometry': geo, 'title_pattern': title}
        log('窗口 %s 几何 %sx%s+%s+%s' % (win, geo.get('WIDTH'), geo.get('HEIGHT'),
                                          geo.get('X'), geo.get('Y')))
        time.sleep(args.settle_boot)

        for screen, steps in steps_for(args.flow, args.side):
            for st in steps:
                if st[0] == 'click':
                    click(genv, st[1], st[2])
                    manifest['steps'].append({'screen': screen, 'action': 'click',
                                              'xy': list(st[1]), 'settle_s': st[2]})
                elif st[0] == 'wait':
                    time.sleep(st[1])
                    manifest['steps'].append({'screen': screen, 'action': 'wait',
                                              'seconds': st[1]})
                    log('%s: wait %.1fs' % (screen, st[1]))
                elif st[0] == 'hold':
                    hold(genv, st[1], st[2], st[3])
                    manifest['steps'].append({'screen': screen, 'action': 'hold',
                                              'xy': list(st[1]), 'hold_s': st[2],
                                              'settle_s': st[3]})
                log('%s: %s' % (screen, st[0]))
            for i in range(FRAMES_PER_SCREEN):
                path = os.path.join(frames_dir, '%s_%d.png' % (screen, i + 1))
                rec = snap(genv, win, path)
                rec.update({'screen': screen, 'index': i + 1,
                            't_rel_s': round(time.time() - manifest.get('_t0', time.time()), 2)})
                manifest['frames'].append(rec)
                log('%s #%d -> %s' % (screen, i + 1, rec['file']))
                if i + 1 < FRAMES_PER_SCREEN:
                    time.sleep(FRAME_GAP_S)
        ok = True
    except Exception as e:  # noqa: BLE001
        manifest['error'] = str(e)
        log('失败：%s' % e)
    finally:
        for p in (game, xvfb):
            if p is not None and p.poll() is None:
                try:
                    p.send_signal(signal.SIGTERM)
                    p.wait(timeout=10)
                except Exception:  # noqa: BLE001
                    try:
                        p.kill()
                    except Exception:  # noqa: BLE001
                        pass
        for f in (flog, xlog):
            if f is not None:
                try:
                    f.close()
                except Exception:  # noqa: BLE001
                    pass
        manifest.pop('_t0', None)
        manifest['ok'] = ok
        manifest['finished_at'] = time.strftime('%Y-%m-%dT%H:%M:%S')
        with open(os.path.join(out, 'manifest.json'), 'w') as f:
            json.dump(manifest, f, ensure_ascii=False, indent=1)
        log('manifest -> %s (ok=%s)' % (os.path.join(out, 'manifest.json'), ok))
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
