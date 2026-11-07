#!/usr/bin/env python3
"""tools/frida_point_array.py —— `tools/frida_point_array.js` 的驱动。

它做三件事（必须**同一个进程**里跑完，因为 frida 的 local device 只能看见本 PID 命名空间的进程）：

1. 用 frida `spawn` 拉起**真游戏**（`Graphwar II` Linux 2.0.60）；
2. 载入 `tools/frida_point_array.js` 下断（地址 = x86-64 ELF 虚地址，见脚本头）；
3. 用 `xdotool` 把对局点进去并**开一炮**，收集消息写成 JSONL。

用法（X 显示可以用 `Xvfb`，见 tools/README-frida.md）：

    .reference/venv/bin/python tools/frida_point_array.py --out /abs/run.jsonl

    # 只挂探针、不点菜单（自己手动玩）：
    .reference/venv/bin/python tools/frida_point_array.py --out /abs/run.jsonl --no-clicks

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
CLICKS = [
    # (delay_since_prev_s, action)
    (0.0, 'mousemove 534 629 click 1'),    # 主菜单 Campaign
    (4.0, 'mousemove 243 224 click 1'),    # campaign 列表里唯一的关卡节点
    (4.0, 'mousemove 1072 1009 click 1'),  # Level 0 弹窗 Start
    (7.0, 'mousemove 1000 1048 click 1'),  # f(x)= 输入框
    # ★ 必须先全选再打字：单击只是把**光标放进去**，直接 type 会**接在旧文本后面**。
    #   2026-11-07 独立复现轮实测：`--extra-shot` 想打 `x/2`，实际打进去的是
    #   `sin(x)+3x/2`（真机曲线与 `sin(x)+3x/2` 逐位吻合 929/929，见 docs/06 §35.8）。
    (0.3, 'key --clearmodifiers ctrl+a'),
    (0.3, 'type --delay 120 "sin(x)+3"'),
    (1.2, 'mousemove 1855 50 mousedown 1'),  # Fire!（长按才开火）
    (2.0, 'mouseup 1'),
]


def xdo(display, action):
    cmd = 'xdotool ' + action
    subprocess.run(cmd, shell=True, env=dict(os.environ, DISPLAY=display),
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', required=True, help='JSONL 输出路径')
    ap.add_argument('--display', default=os.environ.get('DISPLAY', ':92'))
    ap.add_argument('--data-dir', default=os.path.join(REPO, '.agent-work', 'cg', 'gamedata25'))
    ap.add_argument('--js', default=JS, help='要载入的 frida 脚本')
    ap.add_argument('--game', default=GAME, help='游戏目录（含 Graphwar.x86_64 与 .pck）')
    ap.add_argument('--no-clicks', action='store_true')
    ap.add_argument('--settle', type=float, default=10.0, help='启动后等多久再点菜单')
    ap.add_argument('--tail', type=float, default=20.0, help='开火后再录多久')
    ap.add_argument('--extra-shot', action='store_true', help='开火后再打一炮')
    args = ap.parse_args()

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
            print('[exec_queue] t=%s nitems=%s lens=%s' % (
                p.get('t'), p.get('nitems'), [i.get('len') for i in p.get('items') or []]), file=sys.stderr)
        elif t == 'exec_enter':
            print('[exec_enter] t=%s self=%s bt=%s' % (p.get('t'), p.get('self'), (p.get('bt') or [])[:4]),
                  file=sys.stderr)

    dev = frida.get_local_device()
    game = args.game
    argv = [os.path.join(game, 'Graphwar.x86_64'), '--resolution', '1920x1160', '--position', '0,0']
    print('[*] spawn', argv, file=sys.stderr)
    env = {k: v for k, v in dict(os.environ, **env).items() if k not in ('LC_ALL',)}
    pid = dev.spawn(argv, env=env, cwd=game)
    print('[*] pid', pid, file=sys.stderr)
    session = dev.attach(pid)
    script = session.create_script(src, runtime='v8')
    script.on('message', on_message)
    script.load()
    dev.resume(pid)
    print('[*] resumed', file=sys.stderr)

    t0 = time.time()
    try:
        if not args.no_clicks:
            time.sleep(args.settle)
            for delay, action in CLICKS:
                time.sleep(delay)
                print('[click] +%.1fs %s' % (time.time() - t0, action), file=sys.stderr)
                xdo(args.display, action)
            if args.extra_shot:
                time.sleep(10)
                xdo(args.display, 'mousemove 1000 1048 click 1')
                time.sleep(0.4)
                xdo(args.display, 'key --clearmodifiers ctrl+a')   # 先清空，否则会接在旧文本后面
                time.sleep(0.3)
                xdo(args.display, 'type --delay 120 "x/2"')
                time.sleep(1.2)
                xdo(args.display, 'mousemove 1855 50 mousedown 1')
                time.sleep(2.0)
                xdo(args.display, 'mouseup 1')
            time.sleep(args.tail)
        else:
            time.sleep(args.tail)
    finally:
        try:
            time.sleep(1.0)
            dev.kill(pid)
        except Exception as e:  # noqa: BLE001
            print('[warn] kill:', e, file=sys.stderr)
        out.close()
    print('[done] messages by type:', json.dumps(tally), file=sys.stderr)


if __name__ == '__main__':
    main()
