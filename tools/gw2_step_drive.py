#!/usr/bin/env python3
"""tools/gw2_step_drive.py —— 按**脚本化的点击计划**驱动真游戏（`dispatch/69` LAN+AI 局用）。

和 [`tools/frida_point_array.py`](frida_point_array.py) 的分工：

* 那边把"进对局 + 打一发"写死成 `NAV`（主菜单 → 战役 → 关卡 → Start → 输入框 → Fire）；
* 这边**不预设路线**：路线是一条**步骤串**（`--steps`），一步一条命令，
  这样"LAN 大厅 → 建房 → 加 Bot → Start → 等 AI 开火"这种新流程不用改任何既有文件
  （`frida_point_array.py` 一行没动 —— 只 import 它的 `xdo` / `snap` / `_win_id`）。

探针仍是 `tools/frida_point_array.js` 那一套（`--triple` = 66 号的三样：token 列表 /
`pts[n-1]` / `EffectInfo`）。**不需要加新钩子**：`tok_suffix`（`0x3c5f70`）本来就同时记了
① 它**吃到的那 16 字节 `&Point`**（`x0`/`y0` 原文）② 它印出来的串（`text`/`hex`）
③ `ret` = 返回地址（⇒ 是**哪一个**调用点：`0x3c5da0` 还是 `0x4f28ac`，见 docs/06 §42）。

步骤语法（逗号或换行分隔；`#` 到行尾是注释）：

```
w2.5            等 2.5 s
c 1380 629      移动 + 点击（`mousemove` 与 `click` 分成两条命令 —— 合成一条会点错位置）
m 1000 1090     只移动指针
d / u           mousedown / mouseup（长按 = `d, w2, u`）
t "sin(x)+3"    打字（`type --delay 120`）
k Delete        按键（xdotool key）
s 名字          截游戏窗口到 `<snap-dir>/NN_名字.png`
r pos_read AI#1 打点：把"上一次打点以来各计数器涨了多少"打到 stderr
```

`r` 打的是 `tok_fire / tok_suffix / pos_read / eff_payload / eff_json / calc_array / fnpoints`。
判"AI 到底开火没有"就看 `r` 这一行。

用法（X 显示用私有 Xvfb，见 tools/frida_point_array.README.md）：

    .reference/venv/bin/python tools/gw2_step_drive.py --display 127.0.0.1:95 \
        --data-dir <可写目录>/gamedata --out <可写目录>/run.jsonl --snap-dir <可写目录>/snaps \
        --triple --settle 20 --steps 's boot, w1, c 1380 629, w2, s lobby'

只观测，不改游戏内存。
"""
import argparse
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import frida_point_array as fpa  # noqa: E402  复用启动/点击/截图

COUNTERS = ('tok_fire', 'tok_suffix', 'pos_read', 'eff_payload', 'eff_json',
            'calc_array', 'fnpoints', 'fnpoints_ctor', 'exec_queue', 'exec_enter')


def parse_steps(text):
    """把步骤串拆成 `[(op, args)]`；`#` 注释、逗号/换行分隔，`t "…"` 里的逗号不算分隔。"""
    out, buf, q = [], '', False
    for ch in text:
        if ch == '"':
            q = not q
            buf += ch
        elif ch in ',\n' and not q:
            if buf.strip():
                out.append(buf.strip())
            buf = ''
        else:
            buf += ch
    if buf.strip():
        out.append(buf.strip())
    steps = []
    for raw in out:
        if raw.startswith('#'):
            continue
        op, _sp, rest = raw.partition(' ')
        rest = rest.strip()
        # `w1.5` / `r pos_read` 两种写法都收：单字母 op 后面直接跟参数时拆开
        if len(op) > 1 and op[0] in 'wkrd':
            rest = (op[1:] + ' ' + rest).strip()
            op = op[0]
        if op in ('c', 'm'):
            xy = rest.split()
            steps.append((op, (int(xy[0]), int(xy[1]))))
        elif op == 't':
            steps.append((op, rest.strip('"')))
        elif op == 's':
            steps.append((op, rest or 'snap'))
        elif op in ('w', 'k', 'd', 'u', 'r'):
            steps.append((op, rest))
        else:
            raise SystemExit('未知步骤: %r' % raw)
    return steps


def print_msg(p):
    """每个 type 一行摘要（只打关心的那几个；原始 payload 已整条写进 JSONL）。"""
    t = p.get('type', '?')
    if t == 'hooks':
        print('[hooks]', json.dumps(p, ensure_ascii=False)[:300], file=sys.stderr)
    elif t == 'cfg_ack':
        print('[cfg_ack]', json.dumps(p, ensure_ascii=False), file=sys.stderr)
    elif t == 'tok_fire':
        print('[tok_fire] #%s caller=%s flip=%s mode=%s x0=%s y0=%s len=%s text=%r' % (
            p.get('seq'), p.get('caller'), p.get('flip'), p.get('mode'),
            p.get('x0'), p.get('y0'), (p.get('tokens') or {}).get('len'),
            (p.get('parse_text') or '')[:70]), file=sys.stderr)
    elif t == 'tok_suffix':
        print('[tok_suffix] #%s ret=%s flip=%s pt=(%s,%s) text=%r' % (
            p.get('seq'), p.get('ret'), p.get('flip'), p.get('x0'), p.get('y0'),
            p.get('text')), file=sys.stderr)
    elif t == 'pos_read':
        print('[pos_read] #%s n=%s pos=%s' % (p.get('seq'), p.get('n'), p.get('pos_bytes')),
              file=sys.stderr)
    elif t == 'eff_payload':
        print('[eff_payload] %s #%s id=%s tick=%s pos=(%s,%s)' % (
            p.get('site'), p.get('seq'), p.get('effect_id'), p.get('start_tick'),
            p.get('pos_x'), p.get('pos_y')), file=sys.stderr)
    elif t == 'eff_json':
        print('[eff_json] #%s text=%s' % (p.get('seq'), (p.get('text') or '')[:130]),
              file=sys.stderr)
    elif t == 'fnpoints':
        print('[fnpoints] #%s start=(%s,%s) end_x=%s owner=%s ret=%s' % (
            p.get('seq'), p.get('start_x'), p.get('start_y'), p.get('end_x'),
            p.get('owner_id'), p.get('ret')), file=sys.stderr)


def win_ids(display):
    """列**所有**名字像 Graphwar 的窗口 id（`docs/06` §42 的教训：`tail -1` 可能拿到陈旧窗口）。"""
    try:
        out = subprocess.run(['xdotool', 'search', '--name', 'Graphwar'],
                             env=dict(os.environ, DISPLAY=display),
                             capture_output=True, text=True).stdout.split()
    except Exception:  # noqa: BLE001
        out = []
    return out


def snap_all(display, label):
    """把**每一个** Graphwar 窗口各截一张（`<label>__w<id>.png`）；只有一个时就是普通截图。"""
    d = fpa._SNAP['dir']
    ids = win_ids(display)
    print('[windows] %s' % ids, file=sys.stderr)
    if not d or not ids:
        return
    os.makedirs(d, exist_ok=True)
    fpa._SNAP['n'] += 1
    for wid in ids:
        p = os.path.join(d, '%02d_%s__w%s.png' % (fpa._SNAP['n'], label, wid))
        subprocess.run(['import', '-window', wid, p],
                       env=dict(os.environ, DISPLAY=display),
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print('[snap] %s' % p, file=sys.stderr)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', required=True)
    ap.add_argument('--steps', required=True, help='步骤串，见文件头')
    ap.add_argument('--display', default='127.0.0.1:95')
    ap.add_argument('--data-dir', required=True)
    ap.add_argument('--snap-dir', default=None)
    ap.add_argument('--js', default=fpa.JS)
    ap.add_argument('--game', default=fpa.GAME)
    ap.add_argument('--settle', type=float, default=20.0, help='窗口出现后再等多久执行步骤')
    ap.add_argument('--tail', type=float, default=5.0, help='步骤跑完后再录多久')
    ap.add_argument('--triple', action='store_true', help='开 66 号的三样探针（token+fnpoints+eff）')
    ap.add_argument('--keep-alive', type=float, default=0.0,
                    help='步骤跑完后再等这么久才杀进程（让 AI 自己打完）')
    args = ap.parse_args()

    steps = parse_steps(args.steps)
    print('[steps] %d: %s' % (len(steps), steps), file=sys.stderr)

    import frida

    data = args.data_dir
    genv = {
        'DISPLAY': args.display,
        'XDG_DATA_HOME': os.path.join(data, 'data'),
        'XDG_CACHE_HOME': os.path.join(data, 'cache'),
        'XDG_CONFIG_HOME': os.path.join(data, 'config'),
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
    ST = {k: 0 for k in COUNTERS}
    events = []          # 逐个关键事件（判"谁开的火"）

    def on_message(msg, _data):
        if msg.get('type') == 'error':
            print('[js-error]', msg.get('description'), file=sys.stderr)
            out.write(json.dumps({'type': 'js_error', 'msg': msg}) + '\n')
            return
        p = msg.get('payload') or {}
        t = p.get('type', '?')
        tally[t] = tally.get(t, 0) + 1
        if t in ST:
            ST[t] += 1
        if t in ('tok_fire', 'tok_suffix', 'pos_read', 'eff_json', 'fnpoints'):
            events.append({'t': t, 'wall': round(time.time(), 3), 'seq': p.get('seq'),
                           'caller': p.get('caller'), 'ret': p.get('ret'),
                           'x0': p.get('x0'), 'y0': p.get('y0'), 'flip': p.get('flip'),
                           'n': p.get('n'), 'text': p.get('text'),
                           'start_x': p.get('start_x'), 'start_y': p.get('start_y'),
                           'end_x': p.get('end_x'), 'owner_id': p.get('owner_id')})
        out.write(json.dumps(p) + '\n')
        out.flush()
        print_msg(p)

    dev = frida.get_local_device()
    game = args.game
    argv = [os.path.join(game, 'Graphwar.x86_64'), '--resolution', '1920x1160', '--position', '0,0']
    env = {k: v for k, v in dict(os.environ, **genv).items()
           if k not in ('LC_ALL', 'WAYLAND_DISPLAY')}
    print('[*] popen', argv, file=sys.stderr)
    proc = subprocess.Popen(argv, env=env, cwd=game)
    pid = proc.pid
    print('[*] pid', pid, file=sys.stderr)
    t_wait = time.time()
    while time.time() - t_wait < 60:
        if fpa._win_id(args.display) != 'root':
            break
        time.sleep(0.5)
    print('[*] window=%s after %.1fs' % (fpa._win_id(args.display), time.time() - t_wait),
          file=sys.stderr)

    session = dev.attach(pid)
    script = session.create_script(src, runtime='v8')
    script.on('message', on_message)
    script.load()
    try:
        script.post({'type': 'cfg', 'evalRowsMax': 0,
                     'tokenProbe': bool(args.triple),
                     'fnpoints': bool(args.triple),
                     'diverge': False, 'dvMam': False,
                     'effProbe': bool(args.triple)})
    except Exception as e:  # noqa: BLE001
        print('[warn] post cfg:', e, file=sys.stderr)

    fpa._SNAP['dir'] = args.snap_dir
    t0 = time.time()
    mark = dict(ST)

    def since(m):
        return {k: ST[k] - m[k] for k in COUNTERS if ST[k] != m[k]}

    try:
        time.sleep(args.settle)
        snap_all(args.display, 'settle')
        mark = dict(ST)
        for i, (op, arg) in enumerate(steps):
            print('[step %d/%d] +%.1fs %s %s' % (i + 1, len(steps), time.time() - t0, op, arg),
                  file=sys.stderr)
            if op == 'w':
                time.sleep(float(arg))
            elif op == 'c':
                fpa.xdo(args.display, 'mousemove %d %d' % arg)
                time.sleep(0.35)
                fpa.xdo(args.display, 'click 1')
                time.sleep(0.25)
            elif op == 'm':
                fpa.xdo(args.display, 'mousemove %d %d' % arg)
            elif op == 'd':
                fpa.xdo(args.display, 'mousedown 1')
            elif op == 'u':
                fpa.xdo(args.display, 'mouseup 1')
            elif op == 't':
                fpa.xdo(args.display, 'type --delay 120 "%s"' % arg)
            elif op == 'k':
                fpa.xdo(args.display, 'key --clearmodifiers %s' % arg)
            elif op == 's':
                snap_all(args.display, arg)
            elif op == 'r':
                print('[mark %s] +%.1fs %s' % (arg, time.time() - t0, json.dumps(since(mark))),
                      file=sys.stderr)
                mark = dict(ST)
        if args.keep_alive > 0:
            print('[keep-alive] %.1fs' % args.keep_alive, file=sys.stderr)
            time.sleep(args.keep_alive)
        time.sleep(args.tail)
        print('[final] since last mark:', json.dumps(since(mark)), file=sys.stderr)
        for e in events:
            print('[event] %s' % json.dumps(e, ensure_ascii=False), file=sys.stderr)
    finally:
        time.sleep(0.5)
        try:
            dev.kill(pid)
        except Exception as e:  # noqa: BLE001
            print('[warn] kill:', e, file=sys.stderr)
        try:
            proc.terminate()
        except Exception:  # noqa: BLE001
            pass
        out.close()
    print('[done] messages by type:', json.dumps(tally), file=sys.stderr)


if __name__ == '__main__':
    main()
