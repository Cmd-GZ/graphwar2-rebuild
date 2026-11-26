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

§T2（2026-11-24，`dispatch/T2` 消引擎版本混淆项）新增的三组开关
------------------------------------------------------------------
门槛一个数没改（仍是 `fixtures/frame_diff_thresholds.txt`）；本脚本只是多了一种"用哪个引擎跑"：

    --engine <bin>          我方侧引擎（默认 4.7.2 编辑器）
    --game-dir <dir>        原版游戏目录（默认自动探测工作区根）
    --snapshot <dir>        冻结回放快照（默认自动探测 `dist/gw2-replays-snapshot/replays`）
    --ours-mode {project,pck}
    --ours-pck-dir <dir>    pck 模式的部署目录（`<引擎同名>.pck` + `libgw2_rebuild.so`）

`pck` 模式为什么必须存在：**原版那份 4.7.0 二进制是 release export template**，编译时带了
`disable_path_overrides=yes` ⇒ `--path` / `--main-pack` / 位置参数 `project.godot` 全部被拒
（实测报错原文见 docs/06 §54.2）。它只认"二进制旁边同名 .pck"这一种输入，也就是它自己
出货时的形态。所以用它跑我方工程的唯一办法是：4.7.2 编辑器 `--export-pack` 出 `<stem>.pck`，
把 4.7.0 模板拷成 `<stem>.x86_64`、`libgw2_rebuild.so` 一起放同目录。

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
    """**被测量的那棵树** = 本脚本所在的 checkout（`<repo>/tools/frame_capture.py` ⇒ `<repo>`）。

    2026-11-26 修（t6 发现的真陷阱）：原来的第一判据是"从脚本位置向上第一个含
    `.reference/game` 的目录"。在 `git worktree` 里跑时，工作树本身**没有** `.reference/`
    （外部件是 gitignore 的，只在主 checkout 里）⇒ 会一路走到**主 checkout** ⇒
    `CLIENT` 指向**主 checkout 的 client/**，于是 `--side ours` **静默拍"没改过的那份客户端"**，
    而报告里看不出任何异常（数字"稳定不变"反而像是改动没生效）。

    现在：**被测树 = 脚本自己所在的 checkout**；外部件另用 `_find_external()` 向上找。
    "测哪棵树"与"外部件在哪"是两个概念，别混。
    """
    d = HERE
    for _ in range(12):
        if os.path.isfile(os.path.join(d, 'client', 'project.godot')):
            return d
        nd = os.path.dirname(d)
        if nd == d:
            break
        d = nd
    return os.path.dirname(HERE)


def _find_external():
    """外部件（原版三件套 / 冻结回放 / 引擎）所在的树：从脚本位置向上找 `.reference/`。

    主 checkout 与 `git worktree` 共享同一份 `.reference/`（工作树里通常没有，就向上走到主
    checkout）。找不到就退回被测树本身。
    """
    d = HERE
    for _ in range(12):
        if os.path.isdir(os.path.join(d, '.reference')):
            return d
        nd = os.path.dirname(d)
        if nd == d:
            break
        d = nd
    return _find_repo()


def _find_ws():
    """工作区根 = 含原版 `Graphwar.x86_64` 的目录（向上最多 12 层）。

    **T2 起才需要**：94 号那一轮原版三件套在 `<ws>/.reference/game/`，现在它们直接躺在
    工作区根（`Graphwar.x86_64` / `Graphwar.pck` / `libgraphwarrust.so`），4.7.2 编辑器在
    `<ws>/tools/`，冻结回放在 `<ws>/dist/gw2-replays-snapshot/replays/`。
    """
    d = HERE
    for _ in range(12):
        if os.path.exists(os.path.join(d, 'Graphwar.x86_64')):
            return d
        nd = os.path.dirname(d)
        if nd == d:
            break
        d = nd
    return os.path.dirname(HERE)


def _first_existing(*cands):
    for c in cands:
        if c and os.path.exists(c):
            return c
    return cands[0] if cands else ''


REPO = _find_repo()          # ← 被测树（`CLIENT = <REPO>/client`）
_EXT = _find_external()      # ← 外部件所在的树（通常 = 主 checkout）
_WS = _find_ws()

# 三件路径都可以被「环境变量」或「命令行参数」覆盖（`--engine` / `--game-dir` / `--snapshot`）：
#   GW2_GAME_DIR  原版游戏目录（内有 Graphwar.x86_64 + Graphwar.pck + libgraphwarrust.so）
#   GW2_ENGINE    「我方」侧默认用的引擎二进制（4.7.2 编辑器；4.7.0 由 --engine 显式指定）
#   GW2_SNAPSHOT  冻结回放快照目录（回放屏的"固定局面"，见 fixtures/frame_diff_thresholds_amend1.txt §2）
GAME_DIR = _first_existing(os.environ.get('GW2_GAME_DIR', ''),
                           os.path.join(REPO, '.reference', 'game'),
                           os.path.join(_EXT, '.reference', 'game'),
                           _WS)
GODOT = _first_existing(os.environ.get('GW2_ENGINE', ''),
                        os.path.join(REPO, '.reference', 'godot',
                                     'Godot_v4.7.2-stable_linux.x86_64'),
                        os.path.join(_EXT, '.reference', 'godot',
                                     'Godot_v4.7.2-stable_linux.x86_64'),
                        os.path.join(_WS, 'tools', 'Godot_v4.7.2-stable_linux.x86_64'))
# 冻结快照的落点：94 号在 `.reference/replays/gw2-replays-snapshot/replays`，现在在 `dist/` 下同名目录
SNAPSHOT = _first_existing(os.environ.get('GW2_SNAPSHOT', ''),
                           os.path.join(REPO, '.reference', 'replays',
                                        'gw2-replays-snapshot', 'replays'),
                           os.path.join(_EXT, '.reference', 'replays',
                                        'gw2-replays-snapshot', 'replays'),
                           os.path.join(_WS, 'dist', 'gw2-replays-snapshot', 'replays'))
CLIENT = os.path.join(REPO, 'client')

# ── 1920x1160 下的点击坐标（两路**同一串**；来自原版实测帧，见 docs/06 §33 / §52）──────
CAMPAIGN_BTN = (534, 629)     # 主菜单 Campaign
LEVEL_NODE_0 = (241, 228)     # 关卡网格第 0 关（蛇形编号从左上起）
START_BTN = (1074, 1009)      # 关卡弹窗 Start
KEY_X = (1625, 730)           # 表达式键盘上的 `x`
FIRE_BTN = (1855, 55)         # Fire!（**long_press**：必须 mousedown→sleep→mouseup）

FRAMES_PER_SCREEN = 3
FRAME_GAP_S = 1.2

# ── T8（`--flow match`）：**对局屏**的两屏。局面由 `tools/match_serve.py` 钉死，两侧同一份剧本 ──
# 主菜单 "Play"（进 LAN 大厅）：`docs/06` §48.8 的点击串里那一发 `c 1344 622`（原版真机实测）。
# 我方主菜单与原版**整屏 0 像素差**（`docs/06` §53.0 ①）⇒ 同一个坐标。
MENU_PLAY = (1344, 622)
# LAN 列表第 1 行的 `Join`：`docs/06` §48.2 ① 量出来的原版坐标（1920×1160 的 (1842, 209)）。
# 我方这一个流**实测也用同一格**（T8：`--side ours` 一发点中，服务器 t=7.548 s 收到
# `NewConnectionRequest`；见 `docs/06` §60.3）⇒ 两侧同一串坐标，不分开配。
LAN_JOIN_ROW1 = {'original': (1842, 209), 'ours': (1842, 209)}
# S8 的等待：phase2 触发后服务器还要等 `[meta] phase_gap_s`(1.0 s) 才发那 8 帧，
# 再等 5.0 s 让服务端 tick 轴走过 `FunctionActive.active_tick=615`、弹道长出来
# （**这条等待是必要的**：1.6 s 时 client tick 还没到 615，S8 里看不到弹道，见 docs/06 §60.2.1）。
# 两侧同一条时间线 ⇒ 相位差只来自各自动画实现，噪声地板会量它。
MATCH_PLAY_WAIT = 5.0

# ── 增补 1（`fixtures/frame_diff_thresholds_amend1.txt` §3）：用冻结回放当"固定局面" ──
REPLAYS_BTN = (1343, 749)          # 主菜单 Replays
# 回放列表第 1 行（局面 `1793897298`，Level 0）的播放键。
# 原版那个点是 94 号那次量的；我方这个点是 T5 改版后量的 —— 行版式本轮已与原版对齐
# （`docs/06` §56：S2 整屏 0 像素差），所以两者现在其实是同一个位置
# （真帧 ▶ 矩形 1780..1846 × 100..166，中心 (1813,133)）；保留两个键只是为了不改旧记录。
REPLAY_ROW1_PLAY = {'original': (1805, 132), 'ours': (1813, 133)}
REPLAY_END_WAIT = 45.0             # 2276 tick ÷ 60 = 37.9 s ⇒ 45 s 一定播到文件末尾

# 冻结快照里的两份回放（**只拷、不改**；快照本身是全仓库三轴数字的分母，禁止往里写）
SNAPSHOT_REPLAYS = ['1793897298', '1783255495']

# ── T20（`--flow lobby/player/setup/connect/matchpop/level`，2026-11-26）──────────────
# **剩余 13 屏**的点击坐标，全部是 1920×1160 窗口内绝对像素。预注册与"每屏期望"在
# `fixtures/frame_rest_targets.txt`（§1 的表），判据仍是 `fixtures/frame_diff_thresholds.txt`
# （sha256 9cc9fcda…）§3 —— **门槛一个数没改**。
# 每条坐标后面注明来源：既有常量 / 既有文档里的真机点击串 / **本轮从原版真帧量出来的**。
LAN_CREATE_GAME = (765, 1107)    # LAN 大厅底排 " Create Game "（边框 535..995 × 1063..1152，本轮量）
LAN_JOIN_IP = (1192, 1107)       # LAN 大厅底排 " Join IP "（边框 1000..1383，本轮量）
LAN_TAB_GLOBAL = (1480, 50)      # 顶部 "Global" 页签（右半 1048..1907，本轮量）
GAMESETUP_ADD_BOT = (1646, 989)  # GameSetup 底排第五格 = 加 Bot（1574..1718 × 923..1055，本轮量）
ADDBOT_CANCEL = (960, 1006)      # AddBotPopUp 面板下方 " Cancel "（870..1051 × 975..1038，本轮量）
JOINIP_EDIT = (900, 530)         # JoinIpPopUp 的 Ip 输入框（347..1665 × 495..564，本轮量）
JOINIP_JOIN = (1065, 728)        # JoinIpPopUp 的 " Join "（971..1158 × 697..760，本轮量）
CONNECTING_CANCEL = (851, 728)   # ConnectingPopUp 的 " Cancel "（同上一行口径，本轮量）
PLAY_CHAT = (1013, 48)           # 对局屏顶栏第 3 颗开关 = 聊天（941..1086 × 0..95，本轮量）
PLAY_BACK = (73, 48)             # 对局屏左上返回键（0..145 × 0..95，本轮量）
CONFIRM_NO = (764, 703)          # 退出确认框的 " No "（面板 482..1437 × 386..768 内，本轮量）
PLAYER_BTN = (536, 749)          # 主菜单 CharacterButton → PlayerScreen（docs/16 §7.5.1 真机点击串）
PLAYER_TAB_SKINS = (150, 150)    # 玩家页 Skins 页签（进屏默认就是它 ⇒ 本 flow 不需要点）
PLAYER_TAB_FACES = (470, 150)    # 玩家页 Faces 页签（docs/16 §7.5.1 真机点击串）
PLAYER_TAB_HATS = (790, 150)     # 玩家页 Hats 页签（第三个页签，本轮量）
# `--flow connect` 的两个地址（预注册 §1.4）：有静默监听 ⇒ 停在 Connecting；没人听 ⇒ 立刻 Disconnected
CONNECT_SILENT_PORT = 46799
CONNECT_SILENT_ADDRESS = '127.0.0.1:%d' % CONNECT_SILENT_PORT
CONNECT_DEAD_ADDRESS = '127.0.0.1:1'


def silent_listener_source():
    """`--flow connect` 专用子进程源码：**只 accept、不发任何字节**的 TCP 监听。

    为什么需要它：`ConnectingPopUp` 只有在"连上了但还没握手完"时才停在屏幕上；
    没有监听就立刻被拒（直接跳 Disconnected），有真服务器就又太快握手完成。
    它只是**环境**，两侧同一份；不碰被测代码。
    """
    return (
        'import socket, sys, time\n'
        'srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)\n'
        "srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)\n"
        "srv.bind(('127.0.0.1', int(sys.argv[1])))\n"
        'srv.listen(8)\n'
        "print('listening', flush=True)\n"
        'conns = []\n'
        'while True:\n'
        '    try:\n'
        '        c, _ = srv.accept()\n'
        '        conns.append(c)\n'      # 收下就攥着，**不读也不写**
        '    except OSError:\n'
        '        time.sleep(0.1)\n')


def start_silent_listener(port, out, log_path=None):
    """起静默 TCP 监听子进程（同一个进程树：本机逐命令 PID namespace，见文件头）。"""
    log_path = log_path or os.path.join(out, 'silent_listener.log')
    f = open(log_path, 'wb')
    proc = subprocess.Popen([sys.executable, '-u', '-c', silent_listener_source(), str(port)],
                            stdout=f, stderr=subprocess.STDOUT)
    return proc, (log_path, f)


def wait_listening(log_path, seconds=10.0):
    """等静默监听把 `listening` 写进日志（= 端口已经在听）。"""
    t0 = time.time()
    while time.time() - t0 < seconds:
        if os.path.exists(log_path):
            try:
                with open(log_path, encoding='utf-8', errors='replace') as f:
                    if 'listening' in f.read():
                        return True
            except OSError:
                pass
        time.sleep(0.1)
    return False


def wait_tcp_port_free(port, seconds=30.0):
    """等 `127.0.0.1:<port>` 不再有人 listen（**T20 新 flow 专用**）。

    为什么需要：连续跑两条都要起 `tools/match_serve.py` 的 flow 时，上一条收摊的 SIGTERM
    到内核真正释放监听之间有一个很短的窗口 ⇒ 下一条会 `OSError: [Errno 98] Address already
    in use` 而**静默**变成"服务器起不来"（本轮实测踩过：`setup`/`connect` 两条连跑时）。
    既有 `--flow match` 的行为一位不动，这个等待只加在 T20 新增的 flow 上。
    """
    import socket as _socket
    t0 = time.time()
    while time.time() - t0 < seconds:
        s = _socket.socket(_socket.AF_INET, _socket.SOCK_STREAM)
        s.settimeout(0.3)
        try:
            if s.connect_ex(('127.0.0.1', port)) != 0:
                return True          # 连不上 ⇒ 没人在听 ⇒ 端口空着
        finally:
            s.close()
        time.sleep(0.2)
    return False


def kill_proc(proc, name='子进程'):
    """SIGTERM → 等 10 s → SIGKILL；返回它有没有被收掉。"""
    if proc is None:
        return True
    try:
        if proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=10)
            except Exception:  # noqa: BLE001
                proc.kill()
                try:
                    proc.wait(timeout=5)
                except Exception:  # noqa: BLE001
                    pass
    except Exception as e:  # noqa: BLE001
        log('收%s失败：%s' % (name, e))
        return False
    return proc.poll() is not None


def snapshot_dir(override=None):
    """冻结快照回放目录（**只读**；显式 `--snapshot` > 环境变量 > 两个已知落点）。"""
    if override:
        return os.path.abspath(override)
    return SNAPSHOT


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


def seed_replays(side, home, snapshot=None):
    """把快照里的两份回放拷进该侧的 user://replays；返回拷进去的文件名。"""
    src = snapshot_dir(snapshot)
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
    if flow == 'static':
        # §T6（2026-11-26）：**纯静态两屏** —— 主菜单点 Campaign 之后的战役选择屏，再点关卡 0
        # 之后的关卡说明弹窗。不含棋盘局面/骰子/tick ⇒ 不需要固定局面、不需要 `--seed-replays`；
        # 坐标沿用既有常量（`CAMPAIGN_BTN` / `LEVEL_NODE_0`，来自原版实测帧 docs/06 §33/§52）。
        # 点击后各等 2.0 s（`fixtures/static_screens_targets.txt` §2 预注册）；每屏仍 3 帧。
        # ⚠️ 不点 `START_BTN`：那一步才进对局。
        return [
            ('campaign', [('click', CAMPAIGN_BTN, 2.0)]),
            ('level_popup', [('click', LEVEL_NODE_0, 2.0)]),
        ]
    if flow == 'replay':
        return [
            ('menu', []),
            ('replay_lobby', [('click', REPLAYS_BTN, 2.0)]),
            ('replay_play', [('click', REPLAY_ROW1_PLAY[side], 2.0)]),
            ('replay_end', [('wait', REPLAY_END_WAIT)]),
        ]
    if flow == 'match':
        # §T8（2026-11-26）：**对局屏两屏**（`fixtures/frame_match_events.txt` 预注册）。
        # 服务器由本脚本起（同一个进程树：本机逐命令 PID namespace，见文件头 §53.7 第 2 条）。
        # 点法：主菜单 Play → LAN 大厅（服务器在灌 UDP 房间记录）→ 点第 1 行 Join →
        # 服务器按剧本发 phase1 ⇒ 进对局屏（S7）⇒ 拍 3 帧 ⇒ `trigger` 让服务器发 phase2
        # ⇒ 进对局中（S8）⇒ 拍 3 帧。
        return [
            ('match_ready', [
                ('click', MENU_PLAY, 2.5),
                ('click', LAN_JOIN_ROW1[side], 3.0),
            ]),
            ('match_play', [
                ('trigger', None),
                ('wait', MATCH_PLAY_WAIT),
            ]),
        ]
    # ── T20（2026-11-26）：**剩余 13 屏**，预注册 = `fixtures/frame_rest_targets.txt` §1 ──
    # 既有四支（campaign/replay/static/match）以上/以下**一行未改**；下面全是新增分支。
    if flow == 'lobby':
        # §1.1：LAN 大厅两页。房间行是服务端数据 ⇒ 由本脚本起一条**固定**的 UDP 房间广播
        # （`tools/match_serve.py` + `fixtures/frame_match_events.txt` 的 `[meta] room=`，只读）。
        return [
            ('lan_lobby_local', [('click', MENU_PLAY, 3.0)]),
            # ⚠️ settle 1.5 s → 3.0 s：预注册 §1.1 写的是 1.5 s，实测原版切到 Global 页之后
            # 四行要**过 1.5 s 才画全**（第 1 帧与第 2/3 帧差 39 681 px，第 2/3 帧互相逐字节相同）
            # ⇒ 拍早了会拿"还没画完"的帧去比。判据一个数没改，只是等到屏稳定。
            ('lan_lobby_global', [('click', LAN_TAB_GLOBAL, 3.0)]),
        ]
    if flow == 'player':
        # §1.2：玩家选择/商店三页（Skins 是进屏默认页 ⇒ 不需要点）
        return [
            ('player_skins', [('click', PLAYER_BTN, 2.0)]),
            ('player_faces', [('click', PLAYER_TAB_FACES, 1.5)]),
            ('player_hats', [('click', PLAYER_TAB_HATS, 1.5)]),
        ]
    if flow == 'setup':
        # §1.3：建房 → 对局设置屏；再点底排"加 Bot"格 → AddBotPopUp
        return [
            ('game_setup', [('click', MENU_PLAY, 2.5), ('click', LAN_CREATE_GAME, 2.5)]),
            ('add_bot', [('click', GAMESETUP_ADD_BOT, 2.0)]),
        ]
    if flow == 'connect':
        # §1.4：三个连接弹窗。`ConnectingPopUp` 靠"静默 TCP 监听"停在屏幕上；
        # `DisconnectedPopUp` 靠"没人听的端口"立刻被拒。点回来之前先 `ctrl+a` 清空输入框
        # （原版**不会**自己清 —— 探路实测：重开 JoinIpPopUp 时上一次的地址还在）。
        return [
            ('join_ip', [('click', MENU_PLAY, 2.5), ('click', LAN_JOIN_IP, 2.0)]),
            ('connecting', [
                ('click', JOINIP_EDIT, 0.6),
                ('type', CONNECT_SILENT_ADDRESS),
                ('click', JOINIP_JOIN, 2.5),
            ]),
            ('disconnected', [
                ('click', CONNECTING_CANCEL, 2.0),
                ('click', LAN_JOIN_IP, 2.0),
                ('click', JOINIP_EDIT, 0.6),
                ('key', 'ctrl+a'),
                ('type', CONNECT_DEAD_ADDRESS),
                ('click', JOINIP_JOIN, 3.0),
            ]),
        ]
    if flow == 'matchpop':
        # §1.5：对局中才会出现的那两个弹窗。服务器由本脚本起（同 `--flow match`），
        # 但**不 touch 触发文件** ⇒ 一直停在 S7 准备态，弹窗期间底图不会突然开火。
        # ⚠️ 次序与预注册 §1.5 相反（**先退出框、再聊天框**）：预注册写的是"先开聊天再点返回"，
        # 去采集时发现**我方 ChatPopUp 是整屏浮层、盖住了顶栏那颗开关** ⇒ 同一串点击在原版
        # 能把聊天关掉、在我方关不掉（这一条本身是发现，写进 docs/06 §69）。改成"退出框先拍，
        # 关掉它（No）之后再开聊天" ⇒ 两侧同一串点击都能走通。判据与期望值一个没改。
        return [
            ('confirm_exit', [
                ('click', MENU_PLAY, 2.5),
                ('click', LAN_JOIN_ROW1[side], 4.0),
                ('click', PLAY_BACK, 2.0),
            ]),
            ('chat_popup', [
                ('click', CONFIRM_NO, 1.5),      # 关掉退出确认框（" No "）
                ('click', PLAY_CHAT, 2.5),
            ]),
        ]
    if flow == 'level':
        # §1.6：关卡弹窗（= 既有 `--flow static` 的 `level_popup`，同一个 .tscn）。
        # 两个坐标都是既有常量；本轮重复采一次只为 13 屏口径一致（static 分支不改）。
        return [
            ('join_level_popup', [('click', CAMPAIGN_BTN, 2.0), ('click', LEVEL_NODE_0, 2.0)]),
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


def engine_version(binary):
    """跑 `<引擎> --version` 拿版本串（**留证用**：本轮的重点就是引擎版本）。"""
    try:
        p = subprocess.run([binary, '--version'], capture_output=True, text=True, timeout=30)
        out = (p.stdout or '').strip().splitlines()
        return out[0].strip() if out else ''
    except Exception:  # noqa: BLE001
        return ''


def start_game(side, display, home, log_path, extra_args, engine=None, game_dir=None,
               ours_mode='project', ours_pck_dir=None):
    """起游戏。`ours` 侧有两种部署形态（**本轮新增第二种，见文件头 §T2**）：

    * `project`（94 号那种）= `godot --path client`，需要带编辑器功能的引擎（4.7.2 编辑器）；
    * `pck` = 导出模板 + 同名 `.pck`。**原版那份 4.7.0 二进制是 release export template，
      编进了 `disable_path_overrides=yes`**（`--path` / `--main-pack` / 位置参数全被拒），
      所以想让它跑我方工程，只有这一条合法路子：把工程导出成 `<basename>.pck` 放在
      二进制旁边（`<basename>.x86_64` + `<basename>.pck` + `libgw2_rebuild.so` 三件同目录）。
    """
    env = dict(os.environ, **MESA_ENV)
    env.pop('WAYLAND_DISPLAY', None)
    env['DISPLAY'] = display
    env['HOME'] = home
    if side == 'original':
        gdir = os.path.abspath(game_dir or GAME_DIR)
        env['XDG_DATA_HOME'] = os.path.join(home, 'data')
        env['XDG_CACHE_HOME'] = os.path.join(home, 'cache')
        env['XDG_CONFIG_HOME'] = os.path.join(home, 'config')
        argv = [os.path.join(gdir, 'Graphwar.x86_64'),
                '--resolution', '1920x1160', '--position', '0,0'] + extra_args
        cwd = gdir
        title = 'Graphwar'
    else:
        eng = os.path.abspath(engine or GODOT)
        env['XDG_DATA_HOME'] = os.path.join(home, 'data')
        env['XDG_CACHE_HOME'] = os.path.join(home, 'cache')
        env['XDG_CONFIG_HOME'] = os.path.join(home, 'config')
        if ours_mode == 'pck':
            cwd = os.path.abspath(ours_pck_dir or '')
            if not os.path.isdir(cwd):
                raise RuntimeError('--ours-mode pck 需要 --ours-pck-dir（放 <basename>.pck 的目录）')
            base = os.path.basename(eng)
            stem = base[:-len('.x86_64')] if base.endswith('.x86_64') else base
            if not os.path.exists(os.path.join(cwd, stem + '.pck')):
                raise RuntimeError('%s/%s.pck 不存在（导出模板只认"二进制同名 .pck"）' % (cwd, stem))
            argv = [eng, '--rendering-driver', 'opengl3',
                    '--resolution', '1920x1160', '--position', '0,0'] + extra_args
        else:
            cwd = CLIENT
            argv = [eng, '--path', CLIENT, '--rendering-driver', 'opengl3',
                    '--resolution', '1920x1160', '--position', '0,0'] + extra_args
        title = 'Graphwar'
    for d in ('data', 'cache', 'config'):
        os.makedirs(os.path.join(home, d), exist_ok=True)
    f = open(log_path, 'wb')
    proc = subprocess.Popen(argv, cwd=cwd, env=env, stdout=f, stderr=subprocess.STDOUT)
    return proc, env, title, f


def start_match_server(side, out, fixture, tcp_port, udp_port, trigger_file,
                       server_duration=300.0, log_path=None):
    """起**脚本化对局服务器**（§T8）：`tools/match_serve.py`，把局面钉死在 fixture 那份剧本上。

    为什么必须由本脚本起（而不是另开一条命令）：本机每条 bash 命令是独立 PID namespace，
    上一条命令里 `nohup` 起的进程在下一条里就不存在（文件头 §53.7 第 2 条）；
    "起服务器 → 起游戏 → 点 → 截"必须在**同一个进程树**里完成。

    服务器自己写逐帧发送日志（`<out>/serve.jsonl`）；stdout/stderr 另存
    `<out>/serve.stderr.log`（里面也有同内容，便于 `tail` 看进度）。
    """
    script = os.path.join(REPO, 'tools', 'match_serve.py')
    if not os.path.exists(script):
        raise RuntimeError('找不到 %s' % script)
    srv_log = os.path.join(out, 'serve.jsonl')
    err_log = log_path or os.path.join(out, 'serve.stderr.log')
    cmd = [sys.executable, script,
           '--fixture', fixture,
           '--out', srv_log,
           '--side', side,
           '--trigger-file', trigger_file,
           '--tcp-port', str(tcp_port),
           '--udp-port', str(udp_port),
           '--duration', str(server_duration)]
    f = open(err_log, 'wb')
    proc = subprocess.Popen(cmd, stdout=f, stderr=subprocess.STDOUT)
    return proc, srv_log, err_log, f


def wait_server_listening(srv_log, seconds=15.0):
    """等服务器把 `listening` 写进日志（= 它的 TCP 已经在听、UDP 已经在灌）。"""
    t0 = time.time()
    while time.time() - t0 < seconds:
        if os.path.exists(srv_log):
            try:
                with open(srv_log, encoding='utf-8', errors='replace') as f:
                    if '"listening"' in f.read():
                        return True
            except OSError:
                pass
        time.sleep(0.1)
    return False


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
    ap.add_argument('--flow', default='replay',
                    choices=['campaign', 'replay', 'static', 'match',
                             'lobby', 'player', 'setup', 'connect', 'matchpop', 'level'],
                    help='点法：replay = 增补 1 的四屏（回放局面，默认）；campaign = 原登记的三屏；'
                         'static = T6 的静态两屏（campaign / level_popup，见 fixtures/static_screens_targets.txt）；'
                         'match = T8 的对局屏两屏（match_ready / match_play，见 fixtures/frame_match_events.txt）；'
                         'lobby/player/setup/connect/matchpop/level = T20 的剩余 13 屏'
                         '（见 fixtures/frame_rest_targets.txt §1）')
    # ── T8（`--flow match` 专用；不给 ⇒ 行为与既有三支完全一致）────────────────────────
    ap.add_argument('--match-fixture', default=None,
                    help='对局屏的事件序列 fixture（默认 <repo>/fixtures/frame_match_events.txt）')
    ap.add_argument('--match-tcp-port', type=int, default=46702,
                    help='脚本化服务器的 TCP 端口（必须与 fixture [meta] tcp_port 一致，'
                         '否则 LAN 广播里的 address 指向错的地方）')
    ap.add_argument('--match-udp-port', type=int, default=42968,
                    help='LAN 发现端口（原版 LAN 大厅绑 42968，观测值）')
    ap.add_argument('--match-server-duration', type=float, default=300.0,
                    help='服务器进程活多久（秒）；采集全程通常 < 120 s')
    ap.add_argument('--seed-replays', action='store_true',
                    help='把冻结快照里的两份回放拷进该侧 user://replays（不写快照）')
    # ── T2（消引擎版本混淆项）新增：都不给 ⇒ 行为与 94 号那一轮完全一致 ──────────────
    ap.add_argument('--engine', default=None,
                    help='「我方」侧引擎二进制（默认 <ws>/tools/Godot_v4.7.2-stable_linux.x86_64）')
    ap.add_argument('--game-dir', default=None,
                    help='原版游戏目录（内有 Graphwar.x86_64 + Graphwar.pck；默认自动探测）')
    ap.add_argument('--snapshot', default=None, help='冻结回放快照目录（默认自动探测）')
    ap.add_argument('--ours-mode', choices=['project', 'pck'], default='project',
                    help='我方侧部署形态：project = `godot --path client`（需要编辑器版引擎）；'
                         'pck = 导出模板 + 同名 .pck（4.7.0 release export template 只认这种）')
    ap.add_argument('--ours-pck-dir', default=None,
                    help='--ours-mode pck 时：放 <引擎同名词>.pck 与 libgw2_rebuild.so 的目录')
    args = ap.parse_args(argv)

    out = os.path.abspath(args.out)
    frames_dir = os.path.abspath(args.shots or os.path.join(out, 'frames'))
    home = os.path.abspath(args.home or os.path.join(out, 'home'))
    for d in (out, frames_dir, home):
        os.makedirs(d, exist_ok=True)

    engine_bin = os.path.abspath(args.engine or GODOT) if args.side == 'ours' else None
    manifest = {'side': args.side, 'flow': args.flow,
                'display': '127.0.0.1:%d' % args.display_num,
                'out': out, 'frames_dir': frames_dir, 'home': home,
                'started_at': time.strftime('%Y-%m-%dT%H:%M:%S'),
                'frames_per_screen': FRAMES_PER_SCREEN, 'frame_gap_s': FRAME_GAP_S,
                'steps': [], 'frames': [], 'notes': [],
                # 引擎身份（**T2 的对照量**：原版 4.7.0 / 我方 4.7.2 的混淆项就消在这一行）
                'ours_mode': args.ours_mode,
                'engine': engine_bin,
                'engine_version': engine_version(engine_bin) if engine_bin else
                                  engine_version(os.path.join(os.path.abspath(args.game_dir or GAME_DIR),
                                                              'Graphwar.x86_64')),
                'game_dir': os.path.abspath(args.game_dir or GAME_DIR),
                # 2026-11-26：**把"测的是哪棵树"写进 manifest** —— 工作树里跑时原来的
                # `_find_repo()` 会走到主 checkout（见 `_find_repo` 的 docstring），
                # 拍出来的是旧客户端却不报错；现在事后可以只查这一行。
                'repo': REPO, 'client': CLIENT, 'external': _EXT}

    if args.seed_replays:
        manifest['replays_seeded'] = seed_replays(args.side, home, args.snapshot)
        log('回放已就位: %s' % manifest['replays_seeded'])

    # ── T8：`--flow match` 要一台脚本化服务器把局面钉死（只在这条分支上做，既有三支不受影响）──
    # ── T20：`lobby`/`setup`/`connect`/`matchpop` 也要它 —— 前三条只用它的 **UDP 房间广播**
    #    把 LAN 大厅的行数钉死（预注册 §2.2），`matchpop` 用它把局面钉死后进对局屏（§2.1）。
    #    加进来的都是**新增 flow**，`match` 这一支的行为一位没变。
    needs_server = args.flow in ('match', 'lobby', 'setup', 'connect', 'matchpop')
    match_srv = None
    match_srv_files = None
    trigger_file = os.path.join(out, 'phase2.trigger')
    if needs_server:
        # §T20：T20 新增的 flow 可能连着跑（都起同一个 TCP 端口）⇒ 先等上一条把端口放掉。
        # `--flow match` 的行为一位不改（不放这个等待进去）。
        if args.flow != 'match' and not wait_tcp_port_free(args.match_tcp_port):
            raise SystemExit('TCP %d 一直被人占着（上一条 flow 的服务器没收干净）'
                             % args.match_tcp_port)
        fixture = os.path.abspath(args.match_fixture or os.path.join(
            REPO, 'fixtures', 'frame_match_events.txt'))
        if not os.path.exists(fixture):
            raise SystemExit('找不到对局屏 fixture: %s' % fixture)
        # 上一轮留下的触发文件必须删掉，否则服务器会以为"已经可以发 phase2 了"
        if os.path.exists(trigger_file):
            os.remove(trigger_file)
        match_srv, srv_log, err_log, srv_handle = start_match_server(
            args.side, out, fixture, args.match_tcp_port, args.match_udp_port, trigger_file,
            server_duration=args.match_server_duration)
        match_srv_files = (srv_log, err_log, srv_handle)
        attempt = 0
        while not wait_server_listening(srv_log) and attempt < 3:
            # 起不来就**当场收掉**再重试（否则它会一直占着 46702，下一条 flow 起不来 —— 本轮实测踩过）。
            # T20 新增的 flow 连着跑时，上一条的服务器端口释放与本条的 bind 之间会有很短的窗口
            # ⇒ `OSError: [Errno 98] Address already in use`。既有 `--flow match` 不重试。
            kill_proc(match_srv, '脚本化服务器')
            if args.flow == 'match':
                break
            attempt += 1
            log('脚本化服务器起不来，%.1f s 后重试（第 %d 次）' % (5.0 * attempt, attempt))
            time.sleep(5.0 * attempt)
            if not wait_tcp_port_free(args.match_tcp_port):
                continue
            match_srv, srv_log, err_log, srv_handle = start_match_server(
                args.side, out, fixture, args.match_tcp_port, args.match_udp_port, trigger_file,
                server_duration=args.match_server_duration)
            match_srv_files = (srv_log, err_log, srv_handle)
        if not wait_server_listening(srv_log):
            kill_proc(match_srv, '脚本化服务器')
            raise SystemExit('脚本化服务器起不来（见 %s）' % err_log)
        log('脚本化服务器就绪：tcp=%d udp=%d fixture=%s' %
            (args.match_tcp_port, args.match_udp_port, os.path.basename(fixture)))
        manifest['match'] = {'fixture': fixture, 'server_log': srv_log,
                             'server_stderr': err_log, 'trigger_file': trigger_file,
                             'tcp_port': args.match_tcp_port, 'udp_port': args.match_udp_port}

    # ── T20：`--flow connect` 需要一条"只 accept、不发字节"的 TCP 监听（预注册 §2.3）──
    silent_srv = None
    silent_files = None
    if args.flow == 'connect':
        silent_srv, silent_files = start_silent_listener(CONNECT_SILENT_PORT, out)
        if not wait_listening(silent_files[0]):
            kill_proc(silent_srv, '静默 TCP 监听')
            raise SystemExit('静默 TCP 监听起不来（见 %s）' % silent_files[0])
        log('静默 TCP 监听就绪：127.0.0.1:%d（只收不发）' % CONNECT_SILENT_PORT)
        manifest['silent_listener'] = {'port': CONNECT_SILENT_PORT, 'log': silent_files[0]}

    xvfb = game = None
    xlog = flog = None
    ok = False
    try:
        xvfb, display, env, xlog = start_xvfb(args.display_num, os.path.join(out, 'xvfb.log'))
        if not wait_display(display, env):
            raise RuntimeError('Xvfb %s 起不来（见 xvfb.log）' % display)
        log('Xvfb %s 就绪' % display)

        game, genv, title, flog = start_game(args.side, display, home,
                                             os.path.join(out, 'game.log'), args.game_arg,
                                             engine=args.engine, game_dir=args.game_dir,
                                             ours_mode=args.ours_mode,
                                             ours_pck_dir=args.ours_pck_dir)
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
                elif st[0] == 'trigger':
                    # §T8：`--flow match` 专用 —— 拍完 S7 之后 touch 触发文件，
                    # 服务器看到它就发 phase2（比 sleep 稳：不会与截图抢时间）
                    with open(trigger_file, 'w') as tf:
                        tf.write('%s %s\n' % (screen, time.strftime('%Y-%m-%dT%H:%M:%S')))
                    manifest['steps'].append({'screen': screen, 'action': 'trigger',
                                              'file': trigger_file})
                elif st[0] == 'type':
                    # §T20：`--flow connect` 往 JoinIpPopUp 的输入框里打字
                    # （`--delay 120` 与既有工具的口径一致；打太快 Godot 会丢字符）
                    xdo(genv, 'type', '--delay', '120', st[1])
                    time.sleep(0.4)
                    manifest['steps'].append({'screen': screen, 'action': 'type',
                                              'text': st[1]})
                elif st[0] == 'key':
                    # §T20：`key ctrl+a` = 清空输入框（原版**不会**自己清）
                    xdo(genv, 'key', st[1])
                    time.sleep(0.3)
                    manifest['steps'].append({'screen': screen, 'action': 'key',
                                              'key': st[1]})
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
        # §T8：脚本化服务器收摊（它自己会把 summary 写进 serve.jsonl）
        if match_srv is not None:
            try:
                if match_srv.poll() is None:
                    match_srv.send_signal(signal.SIGTERM)
                    try:
                        match_srv.wait(timeout=10)
                    except Exception:  # noqa: BLE001
                        match_srv.kill()
            except Exception as e:  # noqa: BLE001
                log('收服务器失败：%s' % e)
        if match_srv_files is not None:
            try:
                match_srv_files[2].close()
            except Exception:  # noqa: BLE001
                pass
            manifest.setdefault('match', {})['server_rc'] = (
                match_srv.returncode if match_srv is not None else None)
        # §T20：静默 TCP 监听收摊（`--flow connect` 专用；不收拾会占着 46799 让下一轮起不来）
        if silent_srv is not None:
            try:
                if silent_srv.poll() is None:
                    silent_srv.send_signal(signal.SIGTERM)
                    try:
                        silent_srv.wait(timeout=10)
                    except Exception:  # noqa: BLE001
                        silent_srv.kill()
            except Exception as e:  # noqa: BLE001
                log('收静默监听失败：%s' % e)
        if silent_files is not None:
            try:
                silent_files[1].close()
            except Exception:  # noqa: BLE001
                pass
            manifest.setdefault('silent_listener', {})['rc'] = (
                silent_srv.returncode if silent_srv is not None else None)
        manifest.pop('_t0', None)
        manifest['ok'] = ok
        manifest['finished_at'] = time.strftime('%Y-%m-%dT%H:%M:%S')
        with open(os.path.join(out, 'manifest.json'), 'w') as f:
            json.dump(manifest, f, ensure_ascii=False, indent=1)
        log('manifest -> %s (ok=%s)' % (os.path.join(out, 'manifest.json'), ok))
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
