#!/usr/bin/env python3
"""tools/frida_cadence.py —— `frida_cadence.js` 的驱动（`dispatch/80`，**只读** + 可控 `SIGSTOP` 追赶实验）。

回答的问题：**`GraphwarGameClient::process`(`0x518400`) 到底按什么节奏跑** ——
每帧几个 tick、tick 的墙钟间隔、有没有追赶、引擎给的 `delta` 读不读。

## 本机唯一能跑通的姿势（沿用 `tools/frida_process_chain.py` / `frida_point_array.py`）

* `frida spawn` 会把游戏挂在 `wait4` ⇒ **自己 `Popen` + `dev.attach(pid)`**；
* `/tmp` 是逐命令 tmpfs ⇒ Xvfb **TCP** 监听（`-listen tcp -ac`），`DISPLAY=127.0.0.1:<n>`；
* Xvfb 只许看见 mesa（`__EGL_VENDOR_LIBRARY_FILENAMES` / `LIBGLX_VENDOR_LIBRARY_NAME`），否则 core dump；
* `user://` 重定向 = `XDG_DATA/CACHE/CONFIG_HOME` 三个**绝对路径**（相对路径 Godot 静默回落 `$HOME`）；
* 进局用战役那条路（`--levels` 预写 `campaign.save`），坐标见 `frida_point_array.LEVEL_NODE`。

## 两件事分得很开（别混）

* `--steps`：**驱动**（点进对局）。语法复用 `tools/gw2_step_drive.py` 的步骤串。
* `--record`：**测量**（对局里的稳态窗口）。窗口开头会 `rpc reset()` —— 之前的一切（菜单/开局忙等）
  一律不算，所以"每秒几次"这类数字都从 reset 之后算。

## 追赶实验（`--stall`）

真游戏里拿不到"长帧"？不需要真帧：把**进程**冻住 `D` 毫秒（`SIGSTOP`/`SIGCONT`），
墙钟（`CLOCK_MONOTONIC`）照走，于是**恢复后的那一帧**必须一次补齐 `D/16.67ms` 个 tick。
这正好是"一帧欠 N 个 tick"的直接观测 —— 也就是 `0x5184a1 jmp 0x518470` 那条回边。

## 用法

    export PY=/home/CG/Myprojects/graphwar2-rebuild/.reference/venv/bin/python
    $PY tools/frida_cadence.py --out <工作区>/cad_norm.jsonl \
        --data-dir <工作区>/gd_norm --record 70 --stall 200@20 --stall 600@40 --stall 1500@55

    # 帧率与 delta 的**判别实验**（引擎给的 delta 被 `--fixed-fps` 钉死，墙钟不钉）：
    $PY tools/frida_cadence.py ... --game-arg=--fixed-fps --game-arg=10 --record 40

产物（JSONL）：`hooks` / `ready` / `buckets` / `mark` / `reset` / `tally`（每秒一条，累计直方图）。
"""
import argparse
import json
import os
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import frida_point_array as fpa            # noqa: E402  xdo / _win_id / LEVEL_NODE / write_campaign_save
import gw2_step_drive as gsd               # noqa: E402  parse_steps（步骤串语法与 69 号同一份）

JS = os.path.join(HERE, 'frida_cadence.js')
REPO = fpa.REPO
GAME = fpa.GAME


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


def parse_stalls(items):
    """`DUR_MS@AT_S` 列表 → [(at_s, dur_ms)]。`AT_S` 是**测量窗口**内的偏移秒。"""
    out = []
    for it in items or []:
        dur, _, at = it.partition('@')
        out.append((float(at or 0), float(dur)))
    return sorted(out)


def summarize_tally(p):
    c = p.get('counts') or {}
    by = p.get('bySite') or {}
    dom = p.get('dominant_site')
    s = by.get(dom) or {}
    th = s.get('ticks_hist') or {}
    ths = ' '.join('%s:%s' % (k, th[k]) for k in sorted(th, key=lambda x: float(x)))
    tot = p.get('totals') or {}
    cnt = lambda k: (c.get(k) or {}).get('n', 0)
    iv = (p.get('tick_iv_us') or {})
    return ('[t %6.1fs] proc=%d tick=%d pump=%d due=%d gtt=%d | cum tick=%s | '
            'ticks/frame {%s} zero=%s gap_mean=%sµs tick_iv_mean=%sµs') % (
        p.get('since_reset_s', 0), cnt('proc'), cnt('tick'), cnt('pump'), cnt('due'), cnt('gtt'),
        tot.get('ticks'), ths, s.get('zero_tick_n'),
        round((s.get('gap_us') or {}).get('sum', 0) / max(1, (s.get('gap_us') or {}).get('n', 1))),
        round(iv.get('sum', 0) / max(1, iv.get('n', 1))))


LABELS = ['[0,1)', '[1,2)', '[2,4)', '[4,8)', '[8,16)', '[16,33)', '[33,66)', '[66,100)',
          '[100,500)', '[500,1000)', '[1k,5k)', '[5k,10k)', '[10k,15k)', '[15k,16.5k)',
          '[16.5k,17k)', '[17k,20k)', '[20k,30k)', '[30k,50k)', '[50k,100k)',
          '[100k,500k)', '>=500k']


def _count_hist_str(h):
    """tick 数/tick 次数那种**小整数**直方图（不能套 µs 桶标签）。"""
    if not h:
        return '(empty)'
    return ' '.join('%s:%d' % (k, h[k]) for k in sorted(h, key=lambda x: float(x)))


def _hist_str(h, top=None):
    if not h:
        return '(empty)'
    items = sorted(((int(k), v) for k, v in h.items()), key=lambda kv: -kv[1])
    if top:
        items = items[:top]
    return ' '.join('%s:%d' % (LABELS[k] if k < len(LABELS) else str(k), v) for k, v in items)


def summarize(path, only_after_reset=True):
    """把一份 cadence JSONL 打成**真值表**（可原地复跑：`--summary <jsonl>`）。

    口径：直方图是**每秒一份**写进 JSONL 的 ⇒ 这里跨 tally 求和 = 窗口累计；
    解析以最后一次 `reset` 为界（reset 之前的菜单/开局忙等一律不算）。
    """
    import collections
    rows = []
    for line in open(path, encoding='utf-8'):
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    start = 0
    if only_after_reset:
        for i, r in enumerate(rows):
            if r.get('type') == 'reset':
                start = i
    tal = [r for r in rows[start:] if r.get('type') == 'tally']
    if not tal:
        print('[summary] 没有 tally（%s）' % path, file=sys.stderr)
        return 1
    # ⚠ 取**文件里最后一条** tally：totals / 直方图都是累计的，最后一条 = 整个窗口；
    #   按 `reason=='final'` 取会漏掉 RPC final 之后、进程退出之前那 1–2 秒。
    p = tal[-1]
    W = float(p.get('since_reset_s') or 0)
    dom = p.get('dominant_site')
    s = (p.get('bySite') or {}).get(dom) or {}
    tot = p.get('totals') or {}
    frames = s.get('n', 0)
    ticks = tot.get('ticks', 0)
    pumps = tot.get('pumps', 0)
    probes = tot.get('probes', 0)
    print('== %s' % os.path.basename(path))
    print('window        : %.2f s（最后一次 reset 之后）' % W)
    print('dominant site : %s（= 每帧调用者）' % dom)
    print('frames        : %d  ⇒ **%.2f fps**' % (frames, frames / W if W else 0))
    print('ticks         : %d  ⇒ **%.2f tick/s**' % (ticks, ticks / W if W else 0))
    print('pumps         : %d（第 2 个循环体调用）' % pumps)
    print('probes        : %d  ⇒ frames+ticks = %d（%s）' % (
        probes, frames + ticks, '✓' if probes == frames + ticks else '✗'))
    print('ratio         : tick/frame = %.3f ; frame/tick = %.4f' % (
        ticks / frames if frames else 0, frames / ticks if ticks else 0))
    print('ticks/frame   : %s' % _count_hist_str(s.get('ticks_hist')))
    print('pumps/frame   : %s' % _count_hist_str(s.get('pumps_hist')))
    print('zero-tick frames: %d（%.1f%%，这些帧**泵一次都没调**）' % (
        s.get('zero_tick_n', 0), 100.0 * s.get('zero_tick_n', 0) / frames if frames else 0))
    print('order         : TP-exact=%d bad=%d first-event-is-pump=%d' % (
        s.get('order_ok_n', 0), s.get('order_bad_n', 0), s.get('first_pump_n', 0)))
    g = s.get('gap_us') or {}
    if g.get('n'):
        print('frame gap µs  : n=%d mean=%.1f min=%d max=%d' % (
            g['n'], g['sum'] / g['n'], g['min'], g['max']))
        print('   hist       : %s' % _hist_str(s.get('gap_hist')))
    d = s.get('dwell_us') or {}
    if d.get('n'):
        print('dwell µs      : mean=%.1f min=%d max=%d（process 自身耗时，不阻塞）' % (
            d['sum'] / d['n'], d['min'], d['max']))
    # 跨 tally 求和（直方图/统计都是"每秒一份"）
    tiv, slack, dlt = collections.Counter(), collections.Counter(), collections.Counter()
    tn = ts = sn = ss = dn = ds = 0
    tmin, tmax, smin, smax, dmin, dmax = 1e18, 0, 1e18, 0, 1e18, 0
    for r in tal:
        for k, v in (r.get('tick_iv_us_hist') or {}).items():
            tiv[int(k)] += v
        for k, v in (r.get('probe_slack_us_hist') or {}).items():
            slack[int(k)] += v
        for k, v in (r.get('delta_us_hist') or {}).items():
            dlt[int(k)] += v
        st = r.get('tick_iv_us') or {}
        if st.get('n'):
            tn += st['n']; ts += st['sum']; tmin = min(tmin, st['min']); tmax = max(tmax, st['max'])
        st = r.get('probe_slack_ns') or {}
        if st.get('n'):
            sn += st['n']; ss += st['sum']; smin = min(smin, st['min']); smax = max(smax, st['max'])
        st = r.get('delta_us') or {}
        if st.get('n'):
            dn += st['n']; ds += st['sum']; dmin = min(dmin, st['min']); dmax = max(dmax, st['max'])
    if tiv:
        tn = sum(tiv.values())
        print('tick interval µs: n=%d mean=%.1f min=%d max=%d' % (tn, ts / max(1, tn), tmin, tmax))
        print('   hist       : %s' % _hist_str(tiv, top=8))
    if sn:
        print('probe slack µs: n=%d mean=%.0f min=%d max=%d（判决点上"距下一个 tick 还有多少"）' % (
            sn, ss / sn / 1000.0, smin // 1000, smax // 1000))
        print('   hist       : %s' % _hist_str(slack, top=6))
    if dlt:
        # ⚠ `delta_us` 那对 n/sum 在早期版本里是**累计**的（跨 tally 求和会重复计数）
        #   ⇒ 样本数一律取直方图的和（直方图每秒清零，求和 = 窗口累计）。
        n_hist = sum(dlt.values())
        print('engine delta µs: n=%d min=%d max=%d（0x42a8bf 上 `movsd xmm0,[rax]` 的原文）' % (
            n_hist, dmin, dmax))
        print('   hist       : %s' % _hist_str(dlt, top=6))
    lf = p.get('long_frames') or []
    if lf:
        print('long frames   : %d 条（ticks>=2）' % len(lf))
        for f in lf[:14]:
            print('   t=%6dms ticks=%-3d pumps=%-3d dwell=%5dµs gap=%8dµs order=%s' % (
                f.get('t_ms', 0), f.get('ticks', 0), f.get('pumps', 0), f.get('dwell_us', 0),
                f.get('gap_us', 0), f.get('order', '')))
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', default=None)
    ap.add_argument('--summary', action='append', default=None, help='只分析已有 JSONL（可给多份）')
    ap.add_argument('--display', default=None, help='复用已有 X display（默认自己起一个 Xvfb）')
    ap.add_argument('--display-num', type=int, default=97)
    ap.add_argument('--data-dir', default=os.path.join(os.path.dirname(os.path.abspath('.')), '.agent-work', 'cad_data'))
    ap.add_argument('--game', default=GAME)
    ap.add_argument('--js', default=JS)
    ap.add_argument('--steps', default=None,
                    help='驱动步骤串（默认：主菜单 Campaign → 关卡 → Start）')
    ap.add_argument('--levels', type=int, default=20, help='预写 campaign.save = level_<n>')
    ap.add_argument('--settle', type=float, default=18.0, help='窗口出现后等多久再开始点')
    ap.add_argument('--pre-record', type=float, default=2.0, help='步骤跑完后等多久开始测量')
    ap.add_argument('--record', type=float, default=60.0, help='测量窗口长度（秒）')
    ap.add_argument('--stall', action='append', default=None, help='SIGSTOP 实验：DUR_MS@AT_S')
    ap.add_argument('--stall-at-start', action='store_true', help='把 stall 的 AT 从窗口起点算（默认）')
    ap.add_argument('--game-arg', action='append', default=None, help='透传给游戏的额外参数')
    ap.add_argument('--snap-dir', default=None)
    args = ap.parse_args()

    if args.summary:
        rc = 0
        for pth in args.summary:
            rc |= summarize(pth)
        return rc
    if not args.out:
        ap.error('--out 或 --summary 二者必给其一')

    if args.steps is None:
        lv = fpa.LEVEL_NODE.get(0, (243, 224))
        args.steps = 'w1, c 534 629, w4, c %d %d, w4, c 1072 1009' % lv
    steps = gsd.parse_steps(args.steps)
    stalls = parse_stalls(args.stall)
    print('[steps] %d: %s' % (len(steps), steps), file=sys.stderr)
    print('[stalls] %s' % stalls, file=sys.stderr)

    import frida

    out_path = os.path.abspath(args.out)
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    data = os.path.abspath(args.data_dir)
    for k in ('data', 'cache', 'config'):
        os.makedirs(os.path.join(data, k), exist_ok=True)
    fpa.write_campaign_save(data, args.levels)

    xvfb = None
    display = args.display
    if display is None:
        start_xvfb(args.display_num, 1920, 1160,
                   os.path.join(os.path.dirname(out_path), 'xvfb.log'))
        display = '127.0.0.1:%d' % args.display_num
        xvfb = True
    print('[*] DISPLAY', display, file=sys.stderr)
    if args.snap_dir:
        fpa._SNAP['dir'] = args.snap_dir

    genv = {
        'DISPLAY': display,
        'XDG_DATA_HOME': os.path.join(data, 'data'),
        'XDG_CACHE_HOME': os.path.join(data, 'cache'),
        'XDG_CONFIG_HOME': os.path.join(data, 'config'),
        '__EGL_VENDOR_LIBRARY_FILENAMES': '/usr/share/glvnd/egl_vendor.d/50_mesa.json',
        'LIBGLX_VENDOR_LIBRARY_NAME': 'mesa',
        'LIBGL_ALWAYS_SOFTWARE': '1',
        'LC_CTYPE': 'C.UTF-8',
    }
    env = {k: v for k, v in dict(os.environ, **genv).items()
           if k not in ('LC_ALL', 'WAYLAND_DISPLAY')}

    argv = [os.path.join(args.game, 'Graphwar.x86_64'),
            '--resolution', '1920x1160', '--position', '0,0'] + (args.game_arg or [])
    print('[*] popen', argv, file=sys.stderr)
    proc = subprocess.Popen(argv, env=env, cwd=args.game,
                            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    pid = proc.pid
    print('[*] pid', pid, file=sys.stderr)

    t_wait = time.time()
    while time.time() - t_wait < 90:
        if fpa._win_id(display) != 'root':
            break
        if proc.poll() is not None:
            print('[fatal] 游戏进程已退出（rc=%s）' % proc.returncode, file=sys.stderr)
            out = proc.stderr.read().decode('utf-8', 'replace')[-4000:]
            print(out, file=sys.stderr)
            return 2
        time.sleep(0.5)
    print('[*] window=%s after %.1fs' % (fpa._win_id(display), time.time() - t_wait), file=sys.stderr)

    err = open(out_path.replace('.jsonl', '.stderr.log'), 'w', encoding='utf-8')
    out = open(out_path, 'w', encoding='utf-8')
    ready = {'ev': None}
    last = {}

    def on_message(msg, _data):
        if msg.get('type') == 'error':
            print('[js-error]', msg.get('description'), file=sys.stderr)
            out.write(json.dumps({'type': 'js_error', 'msg': msg}) + '\n')
            return
        p = msg.get('payload') or {}
        t = p.get('type', '?')
        if t == 'tally':
            last['tally'] = p
            print(summarize_tally(p), file=sys.stderr)
        elif t == 'ready':
            ready['ev'] = p
            print('[ready] base=%s size=%s main_tid=%s time_src=%s' % (
                p.get('base'), p.get('size'), p.get('main_tid'), p.get('time_src')), file=sys.stderr)
        elif t == 'hooks':
            for h in p.get('hooks') or []:
                print('[hook] %-34s %s' % (h.get('name'), h.get('rva')), file=sys.stderr)
        elif t == 'buckets':
            print('[buckets] %d: %s' % (len(p.get('labels') or []), p.get('labels')), file=sys.stderr)
        elif t in ('mark', 'reset'):
            print('[%s] %s' % (t, p.get('label')), file=sys.stderr)
        elif t == 'hook_error':
            print('[hook-error]', p, file=sys.stderr)
        out.write(json.dumps(p, ensure_ascii=False) + '\n')
        out.flush()

    dev = frida.get_local_device()
    session = dev.attach(pid)
    script = session.create_script(open(args.js, encoding='utf-8').read(), runtime='v8')
    script.on('message', on_message)
    script.load()
    t0 = time.time()

    try:
        if not ready['ev']:
            print('[warn] 没有收到 ready（钩子没挂上？）', file=sys.stderr)
        time.sleep(args.settle)
        if args.snap_dir:
            fpa.snap(display, 'settle')
        for i, (op, arg) in enumerate(steps):
            print('[step %d/%d] +%.1fs %s %s' % (i + 1, len(steps), time.time() - t0, op, arg),
                  file=sys.stderr)
            if op == 'w':
                time.sleep(float(arg))
            elif op == 'c':
                fpa.xdo(display, 'mousemove %d %d' % arg)
                time.sleep(0.35)
                fpa.xdo(display, 'click 1')
                time.sleep(0.25)
            elif op == 'm':
                fpa.xdo(display, 'mousemove %d %d' % arg)
            elif op == 'd':
                fpa.xdo(display, 'mousedown 1')
            elif op == 'u':
                fpa.xdo(display, 'mouseup 1')
            elif op == 't':
                fpa.xdo(display, 'type --delay 120 "%s"' % arg)
            elif op == 'k':
                fpa.xdo(display, 'key --clearmodifiers %s' % arg)
            elif op == 's':
                fpa.snap(display, arg)
            elif op == 'r':
                pass

        time.sleep(args.pre_record)
        if args.snap_dir:
            fpa.snap(display, 'pretrecord')
        try:
            script.exports_sync.reset('record-start')
        except Exception as e:  # noqa: BLE001
            print('[fatal] reset RPC 失败（探针没跑起来）：%s' % e, file=sys.stderr)
        print('[*] 测量窗口开始（%.0f s，stalls=%s）' % (args.record, stalls), file=sys.stderr)

        rec0 = time.time()
        si = 0
        while True:
            now = time.time() - rec0
            if now >= args.record:
                break
            if si < len(stalls) and now >= stalls[si][0]:
                at, dur = stalls[si]
                si += 1
                try:
                    script.exports_sync.mark('stall_begin %.0fms' % dur)
                except Exception:  # noqa: BLE001
                    pass
                print('[stall] SIGSTOP %.0fms at +%.1fs' % (dur, now), file=sys.stderr)
                os.kill(pid, signal.SIGSTOP)
                time.sleep(dur / 1000.0)
                os.kill(pid, signal.SIGCONT)
                try:
                    script.exports_sync.mark('stall_end %.0fms' % dur)
                except Exception:  # noqa: BLE001
                    pass
                continue
            time.sleep(0.05)
        try:
            script.exports_sync.snap('final')
        except Exception as e:  # noqa: BLE001
            print('[warn] final snap:', e, file=sys.stderr)
        if args.snap_dir:
            fpa.snap(display, 'end')
        time.sleep(1.5)
    finally:
        out.close()
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
            subprocess.run(['pkill', '-f', 'Xvfb :%d' % args.display_num],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print('[*] jsonl -> %s' % out_path, file=sys.stderr)

    t = last.get('tally')
    if t:
        print('[last tally] since_reset=%ss totals=%s' % (t.get('since_reset_s'), t.get('totals')),
              file=sys.stderr)


if __name__ == '__main__':
    sys.exit(main())
