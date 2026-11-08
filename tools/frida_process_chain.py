#!/usr/bin/env python3
"""tools/frida_process_chain.py —— `frida_process_chain.js` 的驱动（`dispatch/43`，**只读**）。

回答一个问题：**「谁每帧驱动事件泵」** —— 从引擎的 `_process` 下到
`GraphwarEventHandler::process_events`，每一环每秒被调几次。

## 本机唯一能跑通的姿势（沿用 `tools/frida_point_array.py` 的结论）

* `frida spawn` 会把这份游戏挂在 `wait4`、永不建窗口 ⇒ **自己 `Popen` + `dev.attach(pid)`**；
* `/tmp` 是逐命令 tmpfs ⇒ Xvfb 用 **TCP** 监听（`-listen tcp`），`DISPLAY=127.0.0.1:<n>`；
* Xvfb 必须只看见 mesa（`__EGL_VENDOR_LIBRARY_FILENAMES` / `LIBGLX_VENDOR_LIBRARY_NAME`），
  否则 `libEGL_nvidia` 串到 `swrast_dri` 直接 core dump。

## 用法

    .reference/venv/bin/python tools/frida_process_chain.py \
        --out .agent-work/run/process_chain.jsonl --menu 6 --match 12

    # 只挂探针、不点菜单（自己手动玩）：
    ... --no-clicks

    # 只把已有 JSONL 摘成入库用的纯文本（**入库的是这一份**）：
    ... --extract a.jsonl --fixture-out fixtures/frida_process_chain_sample.jsonl

产物（JSONL）：`hook` / `ready` / `tally`（每秒一次，含各钩子计数与 `RootNode::process`
的调用间隔直方图）/ `backtrace`（`GameClient::process` 前 3 次的精确栈）/ `mark`（阶段边界）。
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
GAME = os.path.join(REPO, '.reference', 'game')
JS = os.path.join(HERE, 'frida_process_chain.js')

# 1920x1160 下的点击坐标（与 tools/frida_point_array.README.md 同一份实测值）
NAV = [
    (0.0, 'mousemove 534 629'),
    (0.4, 'click 1'),          # 主菜单 Campaign
    (4.0, 'mousemove 243 224'),
    (0.4, 'click 1'),          # 关卡列表里唯一的节点
    (4.0, 'mousemove 1072 1009'),
    (0.4, 'click 1'),          # Level 0 弹窗 Start
]


def xdo(display, action):
    subprocess.run('xdotool ' + action, shell=True,
                   env=dict(os.environ, DISPLAY=display),
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def win_id(display):
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


def start_xvfb(display_num, w, h, logpath):
    env = dict(os.environ)
    env.update({
        '__EGL_VENDOR_LIBRARY_FILENAMES': '/usr/share/glvnd/egl_vendor.d/50_mesa.json',
        'LIBGLX_VENDOR_LIBRARY_NAME': 'mesa',
        'LIBGL_ALWAYS_SOFTWARE': '1',
    })
    log = open(logpath, 'wb')
    p = subprocess.Popen(['Xvfb', ':%d' % display_num, '-screen', '0', '%dx%dx24' % (w, h),
                          '-listen', 'tcp', '-ac'], env=env, stdout=log, stderr=log)
    time.sleep(1.5)
    return p


# ── JSONL → 入库纯文本 ────────────────────────────────────────────────────────
FIXTURE_SCHEMA = 'gw2-frida-process-chain-sample/1'


def extract(paths, out_path):
    """把 tally 行摘成一份小表：每秒各钩子的计数 + 线程 + 返回地址。"""
    import hashlib

    tallies, marks, hooks = [], [], []
    for p in paths:
        for line in open(p, encoding='utf-8'):
            line = line.strip()
            if not line:
                continue
            r = json.loads(line)
            t = r.get('type')
            if t == 'tally':
                tallies.append(r)
            elif t == 'mark':
                marks.append({'t': r.get('t'), 'label': r.get('label')})
            elif t == 'hooks':
                hooks.append(r.get('hooks'))

    rows = []
    for r in tallies:
        c = r.get('counts') or {}
        iv = r.get('root_interval_ms') or {}
        rows.append({
            'dt': round(r.get('dt', 0), 3),
            'n': {k: v.get('n') for k, v in c.items()},
            'threads': {k: v.get('threads') for k, v in c.items()},
            'sites': {k: v.get('sites') for k, v in c.items()},
            'root_iv_ms': {'n': iv.get('n'), 'mean': round(iv.get('mean', 0), 3)}
            if iv.get('n') else None,
        })

    prov = {
        'schema': FIXTURE_SCHEMA,
        'hook_rvas': hooks[0] if hooks else None,
        'source_sha256': [hashlib.sha256(open(p, 'rb').read()).hexdigest() for p in paths],
        'rows': rows,
        'marks': marks,
    }
    with open(out_path, 'w', encoding='utf-8') as f:
        f.write('# %s —— 由 tools/frida_process_chain.py --extract 生成，不要手改。\n'
                % FIXTURE_SCHEMA)
        f.write('# 口径：RVA = ELF 虚地址；构建 = Graphwar II Linux 2.0.60\n')
        json.dump(prov, f, ensure_ascii=False, indent=1)
        f.write('\n')
    print('[extract] rows=%d marks=%d -> %s' % (len(rows), len(marks), out_path), file=sys.stderr)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', default=os.path.join('.agent-work', 'frida_process_chain.jsonl'))
    ap.add_argument('--display', default=None, help='复用已有 X 显示（默认自己起 Xvfb :93）')
    ap.add_argument('--menu', type=float, default=6.0, help='主菜单阶段录多久（秒）')
    ap.add_argument('--match', type=float, default=12.0, help='进对局后录多久（秒）')
    ap.add_argument('--settle', type=float, default=6.0, help='进程起来后等多久再挂钩')
    ap.add_argument('--no-clicks', action='store_true')
    ap.add_argument('--bt-rva', default='0x518400',
                    help='进对局后对这个 RVA 抓一次 FUZZY 精确栈（默认 GameClient::process）')
    ap.add_argument('--bt-n', type=int, default=2)
    ap.add_argument('--data-dir', default=os.path.join('.agent-work', 'frida_pc_data'))
    ap.add_argument('--game', default=GAME)
    ap.add_argument('--js', default=JS)
    ap.add_argument('--extract', action='append', default=None)
    ap.add_argument('--fixture-out', default=None)
    args = ap.parse_args()

    if args.extract:
        extract(args.extract, args.fixture_out or 'fixtures/frida_process_chain_sample.jsonl')
        return

    import frida

    out_path = os.path.abspath(args.out)
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    data = os.path.abspath(args.data_dir)
    for k in ('data', 'cache', 'config'):
        os.makedirs(os.path.join(data, k), exist_ok=True)

    xvfb = None
    display = args.display
    if display is None:
        start_xvfb(93, 1920, 1160, os.path.join(os.path.dirname(out_path), 'xvfb.log'))
        display = '127.0.0.1:93'
        xvfb = True
    print('[*] DISPLAY', display, file=sys.stderr)

    env = {
        'DISPLAY': display,
        'XDG_DATA_HOME': os.path.join(data, 'data'),
        'XDG_CACHE_HOME': os.path.join(data, 'cache'),
        'XDG_CONFIG_HOME': os.path.join(data, 'config'),
        '__EGL_VENDOR_LIBRARY_FILENAMES': '/usr/share/glvnd/egl_vendor.d/50_mesa.json',
        'LIBGLX_VENDOR_LIBRARY_NAME': 'mesa',
        'LIBGL_ALWAYS_SOFTWARE': '1',
        'LC_CTYPE': 'C.UTF-8',
    }
    env = {k: v for k, v in dict(os.environ, **env).items() if k not in ('LC_ALL',)}

    argv = [os.path.join(args.game, 'Graphwar.x86_64'),
            '--resolution', '1920x1160', '--position', '0,0']
    print('[*] popen', argv, file=sys.stderr)
    proc = subprocess.Popen(argv, env=env, cwd=args.game,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    pid = proc.pid

    t_wait = time.time()
    while time.time() - t_wait < 60:
        if win_id(display) != 'root':
            break
        time.sleep(0.5)
    print('[*] window=%s after %.1fs pid=%d' % (win_id(display), time.time() - t_wait, pid),
          file=sys.stderr)
    time.sleep(args.settle)

    tallies = []
    hooks = []
    marks = []
    bts = []
    src = open(args.js, encoding='utf-8').read()
    out = open(out_path, 'w', encoding='utf-8')

    def on_message(msg, _data):
        if msg.get('type') == 'error':
            print('[js-error]', msg.get('description'), file=sys.stderr)
            out.write(json.dumps({'type': 'js_error', 'msg': msg}) + '\n')
            return
        p = msg.get('payload') or {}
        t = p.get('type')
        if t == 'tally':
            tallies.append(p)
        elif t == 'mark':
            marks.append(p)
        elif t == 'backtrace':
            bts.append(p)
        elif t in ('hook', 'hook_error'):
            hooks.append(p)
        out.write(json.dumps(p, ensure_ascii=False) + '\n')
        out.flush()

        if t == 'hooks':
            for h in p.get('hooks') or []:
                print('[hook] %-32s %s' % (h.get('name'), h.get('rva')), file=sys.stderr)
        elif t == 'hook_error':
            print('[hook-error]', p, file=sys.stderr)
        elif t == 'ready':
            print('[ready] base=%s size=%s main_tid=%s' % (
                p.get('base'), p.get('size'), p.get('main_tid')), file=sys.stderr)
        elif t == 'mark':
            print('[mark] %s' % p.get('label'), file=sys.stderr)
        elif t == 'thread_name':
            print('[thread] tid=%s = %s' % (p.get('tid'), p.get('name')), file=sys.stderr)
        elif t == 'backtrace':
            print('[bt] %s' % p.get('rva'), file=sys.stderr)
            for fr in p.get('frames') or []:
                print('   ' + ' <- '.join(fr), file=sys.stderr)
        elif t == 'tally':
            c = p.get('counts') or {}
            iv = p.get('root_interval_ms') or {}
            parts = []
            for k, v in c.items():
                th = ','.join(sorted(v.get('threads') or {}))
                st = ' '.join('%s(%d)' % (a, b) for a, b in sorted((v.get('sites') or {}).items()))
                parts.append('%s n=%d thr=[%s] from=[%s]' % (k, v.get('n'), th, st))
            print('[tally %02d] dt=%.2fs %s%s' % (
                p.get('seq'), p.get('dt', 0), ' | '.join(parts),
                ('  root_iv n=%s mean=%.2fms min=%s max=%s' % (
                    iv.get('n'), iv.get('mean', 0), iv.get('min'), iv.get('max')))
                if iv.get('n') else ''), file=sys.stderr)

    dev = frida.get_local_device()
    session = dev.attach(pid)
    script = session.create_script(src, runtime='v8')
    script.on('message', on_message)
    script.load()
    try:
        script.exports_sync.cfg({'tallyMs': 1000})
    except Exception as e:  # noqa: BLE001
        print('[warn] cfg:', e, file=sys.stderr)

    try:
        script.exports_sync.mark('menu')
        time.sleep(args.menu)
        if not args.no_clicks:
            for wait, act in NAV:
                time.sleep(wait)
                xdo(display, act)
            time.sleep(3.0)
            script.exports_sync.mark('in_match')
            if args.bt_rva and args.bt_n:
                try:
                    script.exports_sync.bt(int(args.bt_rva, 16), args.bt_n)
                except Exception as e:  # noqa: BLE001
                    print('[warn] bt:', e, file=sys.stderr)
            time.sleep(args.match)
        script.exports_sync.mark('end')
        time.sleep(1.5)
    finally:
        try:
            session.detach()
        except Exception:  # noqa: BLE001
            pass
        out.close()
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except Exception:  # noqa: BLE001
            proc.kill()
        if xvfb:
            subprocess.run(['pkill', '-f', 'Xvfb :93'], stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
        print('[*] jsonl -> %s (lines=%d)' % (out_path, len(tallies)), file=sys.stderr)


if __name__ == '__main__':
    main()
