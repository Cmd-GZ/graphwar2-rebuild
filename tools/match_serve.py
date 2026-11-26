#!/usr/bin/env python3
"""tools/match_serve.py —— **脚本化对局服务器**（任务 T8）：把局面钉死，两侧各连一次。

## 为什么需要它

视觉真值此前只覆盖主菜单 + 回放流四屏 + 静态两屏（`fixtures/frame_diff_results*.tsv`）。
**对局屏（准备圈 / 棋盘 / 回合 HUD）没有真值**，因为原版战役关卡的士兵/靶子落点不可复现
（`docs/06` §53.5 第 1 条：`GW2_MATCH_SEED` 只对我方服务端有效）。

`docs/06` §48 / §51 已经把「**自建协议服务器 + 真客户端连上来**」这条路走通过：
真客户端能当**加入方**连进一台说同一套协议的服务器（LAN 发现变体 C + TCP `u16` 大端长度 +
JSON）。本工具就是那条路的"按固定剧本发帧"版本：

    UDP 灌 9 字段裸 `GameInfo`（变体 C，结尾带 "\\n"）
      ⇒ 客户端 LAN 列表出现一行 ⇒ 点 Join ⇒ TCP 连上
      ⇒ 我们按 `fixtures/frame_match_events.txt` 的 `[phase1:match_ready]` 原样发
      ⇒ 采集脚本拍完 S7 后 touch 触发文件
      ⇒ 我们发 `[phase2:match_play]`
      ⇒ 采集脚本拍完 S8

## 纪律

* **不发明编码**：帧 = `u16` 大端长度 + JSON 对象（`docs/09` §1.1）；事件名/字段名/字段顺序
  来自 `src/_scaffold/protocol.rs` 与 `docs/07` §3.3；**取值与顺序**来自
  `fixtures/frame_match_events.txt`（该文件逐条抄真回放 `1793897298`）。
  本脚本**读什么发什么**，不重排、不改值。
* **只写日志、不改游戏内存**。发出去的每帧都进 `--out`（JSONL：时刻 / 相位 / 事件名 / 字节数 /
  完整载荷），这也是"两侧吃到的字节一模一样"的证据。
* 两侧用**同一份 fixture**：唯一的差别是 `--side` 标签与采集脚本的点击坐标。

## 用法（一般由 `tools/frame_capture.py --flow match|endgame` 起；也可单独跑）

    python3 tools/match_serve.py --fixture fixtures/frame_match_events.txt \\
        --tcp-port 46702 --udp-port 42968 --out <工作区>/serve.jsonl \\
        --trigger-file <工作区>/phase2.trigger --side original --duration 240

触发：`--trigger-file` 指定的文件一出现 ⇒ 等 `--phase-gap` 秒 ⇒ 发 phase2。
没给 `--trigger-file` 时用 `--auto-phase2 <秒>`（从 phase1 发完算起），默认 12 s。

## T18：多相位（`--flow endgame` 用）

fixture 里可以有**任意个** `[phaseN:屏名]` 小节：第 1 个连上就发，第 2..N 个各等一个触发文件。
多相位用 `--trigger-files a,b,...`（逗号分隔，第 i 项 = 相位 i+2 的触发），
每个相位的 tick 基准取 `[meta] tick_phase<k>`。**两相位 + `--trigger-file` 的老路
（T8 的 `--flow match`）行为与日志逐行保持不变** —— 只是把 `if len(phases) > 1` 那一段
写成了 `for index in range(1, len(phases))`，两相位时循环体恰好跑一次。
"""
import argparse
import hashlib
import json
import os
import socket
import struct
import sys
import threading
import time

T0 = time.time()
LOG = None
LOCK = threading.Lock()


def log(obj):
    obj['t'] = round(time.time() - T0, 3)
    with LOCK:
        line = json.dumps(obj, ensure_ascii=False)
        print(line, file=sys.stderr, flush=True)
        if LOG:
            LOG.write(line + '\n')
            LOG.flush()


# ---------------------------------------------------------------- 帧编解码

def enc(payload: bytes) -> bytes:
    """`u16` 大端长度 + JSON（`docs/09` §1.1）。payload 是 fixture 里的原样字节。"""
    return struct.pack('>H', len(payload)) + payload


def game_info_broadcast(meta: dict, tcp_port: int) -> bytes:
    """LAN 发现报文 = **裸的 9 字段对象 + 结尾换行**（变体 C，`docs/24` §1.1 / `docs/115` §5.3）。

    只有这一种形态被真客户端接受（§48.2 ①：加事件外壳的 `{"GameInfo":…}` 被丢弃）。
    meta 里那行 `room=` 就是它的模板，`{tcp_port}` 会被替换。
    """
    raw = meta['room'].replace('{tcp_port}', str(tcp_port))
    obj = json.loads(raw)
    return (json.dumps(obj, separators=(',', ':')) + '\n').encode()


class Broadcaster(threading.Thread):
    """周期性把房间记录灌进客户端的 LAN 发现端口（客户端在 LanLobby 绑 UDP 42968）。"""

    def __init__(self, targets, payload, period=0.5):
        super().__init__(daemon=True)
        self.targets = targets
        self.payload = payload
        self.period = period
        self.stop = threading.Event()
        self.sent = 0

    def run(self):
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        while not self.stop.is_set():
            for host, port in self.targets:
                try:
                    s.sendto(self.payload, (host, port))
                    self.sent += 1
                except OSError as e:  # noqa: BLE001
                    log({'type': 'udp_error', 'target': '%s:%d' % (host, port), 'err': str(e)})
            self.stop.wait(self.period)


class FrameReader:
    def __init__(self, sock):
        self.sock = sock
        self.buf = b''
        self.closed = False          # 对端关掉之后**不再**反复喊 peer_closed（日志会被刷爆）

    def read_frame(self, timeout):
        """返回 (name, body, raw_bytes) 或 None（超时 / 对端关闭）。"""
        deadline = time.time() + timeout
        while True:
            if self.closed:
                return None
            while len(self.buf) >= 2:
                n = struct.unpack('>H', self.buf[:2])[0]
                if len(self.buf) < 2 + n:
                    break
                raw = self.buf[2:2 + n]
                self.buf = self.buf[2 + n:]
                txt = raw.decode('utf-8', 'replace')
                try:
                    obj = json.loads(txt)
                except ValueError:
                    return ('<unparsed>', {'raw': txt}, raw)
                return (next(iter(obj)), obj[next(iter(obj))], raw)
            left = deadline - time.time()
            if left <= 0:
                return None
            self.sock.settimeout(left)
            try:
                chunk = self.sock.recv(65536)
            except socket.timeout:
                return None
            except OSError as e:  # noqa: BLE001
                log({'type': 'tcp_error', 'err': str(e)})
                return None
            if not chunk:
                if not self.closed:
                    self.closed = True
                    log({'type': 'peer_closed'})
                return None
            self.buf += chunk


# ---------------------------------------------------------------- fixture 解析

def load_fixture(path):
    """解析 `fixtures/frame_match_events.txt`：`[meta]` 的 key=value + 每个 `[phase:<屏名>]`
    的 `[seq]` 行。返回 (meta, [(phase_name, [payload_bytes, …]), …], sha256)。

    行格式：`<json>   # 注释`（注释里的 tick/事件序是可追溯信息，不参与发送）。
    """
    with open(path, 'rb') as f:
        raw = f.read()
    sha = hashlib.sha256(raw).hexdigest()
    meta, phases = {}, []
    section = None
    cur = None
    for ln in raw.decode('utf-8').splitlines():
        s = ln.strip()
        if not s or s.startswith('#'):
            continue
        if s.startswith('[') and s.endswith(']'):
            name = s[1:-1]
            if name == 'meta':
                section, cur = 'meta', None
            elif name.startswith('phase') and ':' in name:
                # 两种写法都收：`[phase1:match_ready]`（本仓库 fixture 用的）与 `[phase:match_ready]`
                section = 'phase'
                cur = [name.split(':', 1)[1], []]
                phases.append(cur)
            elif name == 'seq':
                section = 'seq'
            elif name == 'endseq':
                section = None
            else:
                raise SystemExit('未知小节: %s' % s)
            continue
        if section == 'meta':
            k, _, v = s.partition('=')
            meta[k.strip()] = v.strip()
        elif section == 'seq' and cur is not None:
            if '#' in s:
                s = s.split('#', 1)[0].strip()
            if s:
                json.loads(s)          # 早失败：不合法的 JSON 不许发出去
                cur[1].append(s.encode())
    if not phases:
        raise SystemExit('%s 里没有 [phase:…] 小节' % path)
    return meta, phases, sha


# ---------------------------------------------------------------- 服务一条连接

def tick_now(ctx):
    """服务器"当前 tick"：phase1 期间 = 快照那一刻的 tick；phase2 之后 = 开火那一刻的 tick
    + 墙钟 × `tick_hz`。

    两个基准都来自真回放 `1793897298`：快照那段是 tick **6**（事件 17..62 的 `tick`），
    第一发是 tick **550**（事件 76..80 的 `tick`）、`FunctionActive.active_tick = 615`、
    爆炸 `EffectInfo.start_tick = 1281`。所以 `TickReply` 回的是**这局真实的 tick 轴**，
    不是随手编一个数。
    """
    base, t0 = ctx['tick_base']
    return int(base + (time.time() - t0) * ctx['tick_hz'])


def send_tick_reply(conn, ctx, stats, why):
    body = {'current_tick': tick_now(ctx)}
    payload = json.dumps({'TickReply': body}, separators=(',', ':')).encode()
    conn.sendall(enc(payload))
    stats['ticks'] += 1
    log({'type': 'send', 'phase': ctx.get('phase', '?'), 'name': 'TickReply',
         'bytes': len(payload) + 2, 'payload': payload.decode(), 'why': why})


def pump_for(reader, seconds, stats, ctx=None):
    """在一段时间里**持续读**并记录客户端发来的帧（别用 sleep —— 会把到达时刻压平）。

    `ctx` 给了就顺带**回答 `TickRequest`**（原版客户端 1 Hz 发一条；真服务器回 `TickReply`，
    `docs/07` 判别值 2「逐位 + 观测（4 行）」）。这一条决定客户端动画时钟走到哪个 tick。
    """
    deadline = time.time() + seconds
    while True:
        left = deadline - time.time()
        if left <= 0:
            return
        f = reader.read_frame(left)
        if f is None:
            return
        name, body, raw = f
        log({'type': 'recv', 'name': name, 'body': body, 'bytes': len(raw)})
        stats['recv'].append({'name': name, 'body': body})
        if ctx is not None and ctx.get('reply_tick') and name == 'TickRequest':
            try:
                send_tick_reply(reader.sock, ctx, stats, 'answer TickRequest')
            except OSError as e:  # noqa: BLE001
                log({'type': 'tick_reply_failed', 'err': str(e)})
                return


def phase_tick_base(meta, index):
    """第 `index`（0 起）个相位期间服务器 tick 的**基准值**。

    依次取 `[meta] tick_phase<index+1>`，没有就退化到 `tick_phase2`（2 相位的旧剧本只有
    `tick_phase1`/`tick_phase2` 两个键，行为与 T8 时**逐字一致**）。
    """
    key = 'tick_phase%d' % (index + 1)
    if key in meta:
        return float(meta[key])
    if index == 0:
        return float(meta.get('tick_phase1', 6))
    return float(meta.get('tick_phase2', 550))


def send_phase(conn, phase_name, payloads, args, stats):
    """把一个相位的帧**原样**按顺序发出（夹 `--frame-gap`）；每帧进日志。"""
    log({'type': 'phase_begin', 'phase': phase_name, 'frames': len(payloads)})
    for i, payload in enumerate(payloads, 1):
        conn.sendall(enc(payload))
        obj = json.loads(payload.decode())
        log({'type': 'send', 'phase': phase_name, 'index': i, 'name': next(iter(obj)),
             'bytes': len(payload) + 2, 'payload': payload.decode()})
        time.sleep(args.frame_gap)
    stats['sent'].append({'phase': phase_name, 'frames': len(payloads)})
    log({'type': 'phase_done', 'phase': phase_name})


def wait_trigger(reader, args, stats, ctx, trigger, index):
    """等第 `index` 个相位（0 起，1 = phase2）的触发文件；没给就退回定时。

    `--trigger-files a,b`（T18 新增，多相位用）或旧的单数 `--trigger-file`（T8 的两相位剧本）
    都收。两相位 + 单数那条路 **逐行保持 T8 的行为**（`trigger_seen` / `auto_phase2` / 超时返回）。
    """
    if trigger:
        t_wait = 0.0
        while not os.path.exists(trigger):
            pump_for(reader, 0.1, stats, ctx)
            t_wait += 0.1
            if args.duration and (time.time() - T0) > args.duration:
                rec = {'type': 'trigger_timeout', 'waited': round(t_wait, 2)}
                if index > 1:
                    rec.update({'phase_index': index + 1, 'trigger': trigger})
                log(rec)
                return False
        rec = {'type': 'trigger_seen', 'waited': round(t_wait, 2)}
        if index > 1:
            rec.update({'phase_index': index + 1, 'trigger': trigger})
        log(rec)
        return True
    deadline = time.time() + args.auto_phase2
    while time.time() < deadline:
        pump_for(reader, 0.1, stats, ctx)
    if index == 1 and not args.trigger_files:
        log({'type': 'auto_phase2', 'after': args.auto_phase2})   # T8 的旧日志行，别改
    else:
        log({'type': 'auto_phase', 'phase_index': index + 1, 'after': args.auto_phase2})
    return True


def serve(conn, addr, args, phases, stats, meta):
    reader = FrameReader(conn)
    log({'type': 'accepted', 'peer': '%s:%d' % addr})
    # tick 轴（见 tick_now）：phase1 = 快照 tick，phase2 = 开火 tick（两者都来自真回放）
    ctx = {'tick_base': (phase_tick_base(meta, 0), time.time()),
           'tick_hz': float(meta.get('tick_hz', 60)), 'reply_tick': args.reply_tick,
           'phase': phases[0][0], 'meta': meta}
    stats['ticks'] = 0
    # 每个相位 2..N 各一个触发文件：`--trigger-files`（多相位）优先，否则沿用 `--trigger-file`
    triggers = args.trigger_files or ([args.trigger_file] if args.trigger_file else [])

    # ① 客户端连上后第一帧（真客户端发 NewConnectionRequest；`docs/07` 判别值 4）
    first = reader.read_frame(args.handshake_wait)
    if first:
        log({'type': 'recv', 'name': first[0], 'body': first[1], 'bytes': len(first[2])})
        stats['recv'].append({'name': first[0], 'body': first[1]})
    else:
        log({'type': 'no_client_frame', 'note': '客户端没说第一句，仍按剧本发 phase1'})

    # ② phase1 逐帧发出（**原样字节**，来自 fixture）
    send_phase(conn, phases[0][0], phases[0][1], args, stats)

    # ③ 相位 2..N：每个都「等触发（采集脚本拍完上一屏后 touch）→ 等 phase_gap → 原样发」
    for index in range(1, len(phases)):
        ctx['tick_base'] = (phase_tick_base(meta, index), time.time())
        ctx['phase'] = phases[index][0]
        if index == 1:
            why = 'phase2 = 真回放第一发的 tick'          # T8 的旧日志行，别改
        else:
            why = 'phase%d = fixture [meta] tick_phase%d' % (index + 1, index + 1)
        rec = {'type': 'tick_base', 'value': ctx['tick_base'][0], 'why': why}
        if index > 1:            # T18：多相位才多这一列（两相位时日志与 T8 逐字相同）
            rec['phase_index'] = index + 1
        log(rec)
        trigger = triggers[index - 1] if index - 1 < len(triggers) else None
        if not wait_trigger(reader, args, stats, ctx, trigger, index):
            return
        if args.phase_gap > 0:
            time.sleep(args.phase_gap)
        send_phase(conn, phases[index][0], phases[index][1], args, stats)

    # ④ 收尾：把客户端后续发来的帧记下来（判"它有没有回话"/"它以为自己在哪一屏"）
    deadline = time.time() + args.collect
    while time.time() < deadline:
        pump_for(reader, 0.2, stats, ctx)
    log({'type': 'session_done', 'recv_names': [r['name'] for r in stats['recv']],
         'tick_replies': stats['ticks'], 'tick_final': tick_now(ctx)})


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--fixture', default=os.path.join(os.path.dirname(here), 'fixtures',
                                                      'frame_match_events.txt'))
    ap.add_argument('--tcp-port', type=int, default=None, help='默认取 fixture [meta] tcp_port')
    ap.add_argument('--udp-port', type=int, default=None, help='默认取 fixture [meta] udp_port')
    ap.add_argument('--udp-targets', default='127.0.0.1',
                    help='LAN 发现的目标；逗号分隔（同一房间灌多个 target ⇒ 客户端列表会出现多行）')
    ap.add_argument('--side', default='?', help='只写进日志（original / ours）')
    ap.add_argument('--out', required=True, help='逐帧发送日志（JSONL）')
    ap.add_argument('--trigger-file', default=None,
                    help='这个文件一出现就发 phase2（采集脚本在两个屏之间 touch）；两相位剧本用')
    ap.add_argument('--trigger-files', default=None,
                    help='T18：多相位用 —— 逗号分隔，第 i 项 = 相位 i+2 的触发文件'
                         '（`--flow endgame` 给两个：phase2 / phase3）。给了就优先于 --trigger-file')
    ap.add_argument('--auto-phase2', type=float, default=12.0,
                    help='没给 --trigger-file 时：phase1 发完等这么久就发 phase2')
    ap.add_argument('--phase-gap', type=float, default=None,
                    help='触发到 phase2 之间再等多久（默认取 fixture [meta] phase_gap_s）')
    ap.add_argument('--frame-gap', type=float, default=0.02, help='两帧之间间隔（秒）')
    ap.add_argument('--reply-tick', action='store_true', default=True,
                    help='收到 TickRequest 就回 TickReply（默认开；真服务器会回，docs/07 tag 2）')
    ap.add_argument('--no-reply-tick', dest='reply_tick', action='store_false',
                    help='关掉 TickReply（诊断用：验证"动画时钟是不是靠它推的"）')
    ap.add_argument('--handshake-wait', type=float, default=8.0)
    ap.add_argument('--collect', type=float, default=8.0, help='phase2 之后继续收多久')
    ap.add_argument('--duration', type=float, default=300.0, help='整条命令的上限（秒）')
    ap.add_argument('--broadcast-for', type=float, default=0.0,
                    help='>0 时到点自动收摊（0 = 一直广播到进程被杀）')
    ap.add_argument('--connections', type=int, default=1,
                    help='服务几条连接（客户端重连时 >1；每条都从头喂同一份剧本）')
    args = ap.parse_args()

    # T18：`--trigger-files a,b` ⇒ 每个相位 2..N 一个触发文件；不给就沿用 T8 的单数形式
    args.trigger_files = ([t for t in args.trigger_files.split(',') if t]
                          if args.trigger_files else [])

    meta, phases, sha = load_fixture(args.fixture)
    tcp_port = args.tcp_port if args.tcp_port is not None else int(meta.get('tcp_port', 46702))
    udp_port = args.udp_port if args.udp_port is not None else int(meta.get('udp_port', 42968))
    if args.phase_gap is None:
        args.phase_gap = float(meta.get('phase_gap_s', 1.0))
    if args.trigger_files and len(args.trigger_files) < len(phases) - 1:
        raise SystemExit('--trigger-files 给了 %d 个，但 fixture 有 %d 个相位（需要 %d 个触发）'
                         % (len(args.trigger_files), len(phases), len(phases) - 1))

    global LOG
    LOG = open(args.out, 'w', encoding='utf-8')

    address = '127.0.0.1:%d' % tcp_port
    payload = game_info_broadcast(meta, tcp_port)
    targets = [(h, udp_port) for h in args.udp_targets.split(',') if h]
    bc = Broadcaster(targets, payload)
    bc.start()
    log({'type': 'config', 'side': args.side, 'tcp_port': tcp_port,
         'advertised': address, 'udp_targets': targets,
         'fixture': os.path.abspath(args.fixture), 'fixture_sha256': sha,
         'phases': [{'name': n, 'frames': len(p)} for n, p in phases],
         'broadcast': payload.decode().strip()})

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(('127.0.0.1', tcp_port))
    srv.listen(4)
    srv.settimeout(1.0)
    log({'type': 'listening', 'on': address})

    stats = {'recv': [], 'sent': []}
    n = 0
    try:
        while n < args.connections:
            try:
                conn, addr = srv.accept()
            except socket.timeout:
                if args.duration and (time.time() - T0) > args.duration:
                    log({'type': 'accept_timeout'})
                    break
                continue
            n += 1
            try:
                serve(conn, addr, args, phases, stats, meta)
            except (BrokenPipeError, ConnectionResetError) as e:
                log({'type': 'client_gone', 'err': str(e)})
            finally:
                conn.close()
    except KeyboardInterrupt:
        pass
    finally:
        bc.stop.set()
        log({'type': 'summary', 'udp_sent': bc.sent, 'connections': n,
             'sent': stats['sent'], 'recv_names': [r['name'] for r in stats['recv']]})
        LOG.close()


if __name__ == '__main__':
    main()
