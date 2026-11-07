#!/usr/bin/env python3
"""tools/gw2_drive.py —— 通用「真游戏驱动器」（Xvfb + frida spawn + xdotool 脚本化点击）。

为什么又写一个（`tools/frida_point_array.py` 已经能跑）：
  `frida_point_array.py` 的点击脚本是**写死**的（Campaign → 唯一关卡 → Level 0 → sin(x)+3），
  而本任务要进 **Level 2（中心单障碍）** 并对**任意函数**开火，还要能在半路截图确认
  UI 到底长什么样。所以这里把「下断脚本」与「操作计划」拆开：

    --js    frida 脚本（只观测；可以不给 = 空脚本，只驱动 UI）
    --plan  操作计划 JSON（见下），也可用 --no-clicks 只起游戏

计划 JSON（动作按序执行，`settle` 是 spawn→第一次点击之间等多久）：

    {"settle": 12, "steps": [
       ["click", 534, 629, "Campaign"],          # 单击
       ["wait", 3],                              # 等 s
       ["shot", "campaign"],                     # 截图到 <--shots>/<name>.png
       ["type", "sin(x)+3"],                     # 键入（xdotool type）
       ["key", "Return"],                        # 敲键
       ["hold", 1855, 50, 2000, "Fire!"],        # 长按（Fire! 是 long_press，见 docs/06 §35.1）
       ["scroll", 5, 3, "down"]                  # 在 (x,y) 上滚轮 3 格
    ]}

坑（都实测过，别再踩）：
  * `/tmp` 是**逐命令** tmpfs ⇒ Xvfb 一律 `-listen tcp`，`DISPLAY=127.0.0.1:92`；
  * frida 的 local device 只看得到**本命令**里的进程 ⇒ 起游戏、下断、点击必须在**同一条命令**里；
  * `Fire!` 是 long_press 按钮：单击只按下不动 ⇒ 必须 `mousedown; sleep; mouseup`；
  * 显卡走 mesa llvmpipe（`__EGL_VENDOR_LIBRARY_FILENAMES` + `LIBGL_ALWAYS_SOFTWARE=1`），
    否则 `libEGL_nvidia` → `swrast_dri` 段错误（core dump）。

依赖（照抄即可）：
  * frida **17.26.1**：`<ws>/.reference/venv/bin/python -c "import frida; print(frida.__version__)"`；
  * `Xvfb` + `xdotool` + ImageMagick `import`（截图）：本机 `/usr/bin/{Xvfb,xdotool,import}`；
  * 游戏三件套：`<ws>/.reference/game/{Graphwar.x86_64, Graphwar.pck, libgraphwarrust.so}`，
    `libgraphwarrust.so` sha256 **`9a538115…`**（Graphwar II Linux **2.0.60**，x86-64 ELF）
    —— 下断用的 RVA 全在 `tools/obstacle_room.js` 头里，**换构建必须重核**。

只**观测**：不改游戏内存、不打补丁。
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


class Driver:
    def __init__(self, args):
        self.args = args
        self.display = args.display
        self.shots = args.shots
        self.vars = {}
        self.env = dict(os.environ)
        if self.shots:
            os.makedirs(self.shots, exist_ok=True)

    # ── X / 输入 ────────────────────────────────────────────────────────────
    def xenv(self):
        e = dict(os.environ)
        e['DISPLAY'] = self.display
        return e

    def xdo(self, action):
        subprocess.run('xdotool ' + action, shell=True, env=self.xenv(),
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def shot(self, name):
        if not self.shots:
            return
        p = os.path.join(self.shots, name + '.png')
        subprocess.run(['import', '-display', self.display, '-window', 'root', p],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print('[shot] %s' % p, file=sys.stderr)

    def start_xvfb(self):
        self.xvfb = None
        # 已经有一个可用的 X 就直接用
        r = subprocess.run(['xdpyinfo', '-display', self.display],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if r.returncode == 0:
            print('[*] reuse X %s' % self.display, file=sys.stderr)
            return
        num = self.display.split(':')[-1]
        cmd = ['Xvfb', ':' + num, '-screen', '0', '1920x1160x24', '-listen', 'tcp', '-ac']
        print('[*] Xvfb', ' '.join(cmd), file=sys.stderr)
        # Xvfb 自己也会走 GLX/EGL：不指 mesa 就 `libEGL_nvidia → swrast_dri` 段错误
        # （docs/06 §35.1 第 2 条，本条实测同样适用于 Xvfb 本进程）
        env = dict(os.environ,
                   __EGL_VENDOR_LIBRARY_FILENAMES='/usr/share/glvnd/egl_vendor.d/50_mesa.json',
                   LIBGLX_VENDOR_LIBRARY_NAME='mesa',
                   LIBGL_ALWAYS_SOFTWARE='1')
        self.xvfb = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                     env=env)
        for _ in range(50):
            time.sleep(0.2)
            r = subprocess.run(['xdpyinfo', '-display', self.display],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if r.returncode == 0:
                return
        raise SystemExit('Xvfb 起不来：%s' % self.display)

    # ── 计划 ────────────────────────────────────────────────────────────────
    def muzzle_slope(self, jsonl):
        """从 frida JSONL 里取**上一发**的点数组首点（= 炮口）算斜率。

        为什么需要它：原版的弹道是 `y = f(x) + c`，`c` 由"曲线必须过炮口"定
        （炮口 `(x_m, y_m)` 是随关卡随机摆放的，UI 上显示的那条 `+ 2.221` 就是 `c`）。
        要让弹道**保证穿过原点**的障碍物，取 `f(x) = (y_m / x_m) * x` ⇒ `c = 0`。
        """
        last = None
        for line in open(jsonl, encoding='utf-8'):
            try:
                r = json.loads(line)
            except Exception:  # noqa: BLE001
                continue
            if r.get('type') != 'exec_queue':
                continue
            for it in r.get('items') or []:
                pts = it.get('pts') or {}
                if pts.get('head'):
                    last = pts['head'][0]
        if not last:
            raise SystemExit('muzzle_slope: JSONL 里还没有点数组（%s）' % jsonl)
        import struct
        xm = struct.unpack('<d', bytes.fromhex(last[0]))[0]
        ym = struct.unpack('<d', bytes.fromhex(last[1]))[0]
        m = ym / xm
        print('[muzzle] (x_m,y_m)=(%.17g, %.17g) -> f(x)=%.9f*x  (c=0 过原点)' % (xm, ym, m),
              file=sys.stderr)
        self.vars['m'] = m
        self.vars['xm'] = xm
        self.vars['ym'] = ym

    def run_plan(self, plan):
        t0 = time.time()
        for step in plan.get('steps', []):
            act = step[0]
            if act == 'wait':
                time.sleep(float(step[1]))
            elif act == 'click':
                self.xdo('mousemove %d %d click 1' % (step[1], step[2]))
                print('[click] +%.1fs %d,%d %s' % (time.time() - t0, step[1], step[2],
                                                   step[3] if len(step) > 3 else ''), file=sys.stderr)
            elif act == 'dclick':
                self.xdo('mousemove %d %d click --repeat 2 1' % (step[1], step[2]))
                print('[dclick] +%.1fs %d,%d' % (time.time() - t0, step[1], step[2]), file=sys.stderr)
            elif act == 'mousemove':
                self.xdo('mousemove %d %d' % (step[1], step[2]))
            elif act == 'scroll':
                self.xdo('mousemove %d %d' % (step[1], step[2]))
                self.xdo('click --repeat %d %d' % (abs(int(step[3])), 4 if step[3] < 0 else 5))
            elif act == 'type':
                self.xdo('type --delay 120 "%s"' % step[1])
                print('[type] +%.1fs %s' % (time.time() - t0, step[1]), file=sys.stderr)
            elif act == 'key':
                self.xdo('key ' + step[1])
                print('[key] +%.1fs %s' % (time.time() - t0, step[1]), file=sys.stderr)
            elif act == 'hold':
                self.xdo('mousemove %d %d' % (step[1], step[2]))
                time.sleep(0.2)
                self.xdo('mousedown 1')
                time.sleep(float(step[3]) / 1000.0)
                self.xdo('mouseup 1')
                print('[hold] +%.1fs %d,%d %dms %s' % (time.time() - t0, step[1], step[2], step[3],
                                                       step[4] if len(step) > 4 else ''), file=sys.stderr)
            elif act == 'shot':
                self.shot(step[1])
            elif act == 'calc_slope':
                self.muzzle_slope(step[1])
            elif act == 'type_tpl':
                txt = step[1].format(m=('%.6f' % self.vars.get('m', 0.0)))
                self.xdo('type --delay 120 "%s"' % txt)
                print('[type] +%.1fs %s' % (time.time() - t0, txt), file=sys.stderr)
            elif act == 'mark':
                print('[mark] +%.1fs %s' % (time.time() - t0, step[1]), file=sys.stderr)
            else:
                raise SystemExit('未知动作 %r' % (act,))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', default='', help='frida 消息 JSONL 输出（不给就丢）')
    ap.add_argument('--js', default='', help='frida 脚本路径（不给 = 不下断）')
    ap.add_argument('--plan', default='', help='操作计划 JSON')
    ap.add_argument('--display', default='127.0.0.1:92')
    ap.add_argument('--shots', default='', help='截图目录')
    ap.add_argument('--data-dir', default=os.path.join(REPO, '.agent-work', 'cg', 'wt-27', 'gamedata27'))
    ap.add_argument('--game', default=GAME)
    ap.add_argument('--no-clicks', action='store_true')
    ap.add_argument('--settle', type=float, default=12.0)
    ap.add_argument('--tail', type=float, default=3.0, help='计划跑完后再录多久')
    ap.add_argument('--keep', action='store_true', help='不杀游戏（调试用，别在批量里用）')
    args = ap.parse_args()

    plan = {'settle': args.settle, 'steps': []}
    if args.plan:
        plan = json.load(open(args.plan, encoding='utf-8'))
    if args.no_clicks:
        plan = {'settle': 0, 'steps': []}

    d = Driver(args)
    d.start_xvfb()

    import frida

    env = {
        'DISPLAY': args.display,
        'XDG_DATA_HOME': os.path.join(args.data_dir, 'data'),
        'XDG_CACHE_HOME': os.path.join(args.data_dir, 'cache'),
        'XDG_CONFIG_HOME': os.path.join(args.data_dir, 'config'),
        '__EGL_VENDOR_LIBRARY_FILENAMES': '/usr/share/glvnd/egl_vendor.d/50_mesa.json',
        'LIBGLX_VENDOR_LIBRARY_NAME': 'mesa',
        'LIBGL_ALWAYS_SOFTWARE': '1',
        'LC_CTYPE': 'C.UTF-8',
    }
    for k in ('data', 'cache', 'config'):
        os.makedirs(os.path.join(args.data_dir, k), exist_ok=True)

    src = open(args.js, encoding='utf-8').read() if args.js else "'use strict';"
    out = open(args.out, 'w', encoding='utf-8') if args.out else None
    tally = {}

    def on_message(msg, _data):
        if msg.get('type') == 'error':
            print('[js-error]', str(msg.get('description'))[:1500], file=sys.stderr)
            if out:
                out.write(json.dumps({'type': 'js_error', 'msg': msg}) + '\n')
                out.flush()
            return
        p = msg.get('payload') or {}
        t = p.get('type', '?')
        tally[t] = tally.get(t, 0) + 1
        if out:
            out.write(json.dumps(p) + '\n')
            out.flush()
        if t in ('hooks', 'summary', 'hit', 'destroy_wire'):
            print('[%s] %s' % (t, json.dumps(p, ensure_ascii=False)[:1200]), file=sys.stderr)

    argv = [os.path.join(args.game, 'Graphwar.x86_64'),
            '--resolution', '1920x1160', '--position', '0,0']
    dev = frida.get_local_device()
    print('[*] spawn', argv, file=sys.stderr)
    fenv = {k: v for k, v in dict(os.environ, **env).items() if k not in ('LC_ALL',)}
    pid = dev.spawn(argv, env=fenv, cwd=args.game)
    print('[*] pid', pid, file=sys.stderr)
    session = dev.attach(pid)
    script = session.create_script(src, runtime='v8')
    script.on('message', on_message)
    script.load()
    dev.resume(pid)
    print('[*] resumed', file=sys.stderr)

    try:
        time.sleep(float(plan.get('settle', args.settle)))
        d.run_plan(plan)
        time.sleep(args.tail)
    finally:
        if not args.keep:
            try:
                dev.kill(pid)
            except Exception as e:  # noqa: BLE001
                print('[warn] kill:', e, file=sys.stderr)
        if out:
            out.close()
        if d.xvfb:
            d.xvfb.send_signal(signal.SIGTERM)
    print('[done] messages by type:', json.dumps(tally), file=sys.stderr)


if __name__ == '__main__':
    main()
