#!/usr/bin/env python3
"""tools/gate5_serve.py —— 造「本连接的第一个 player_info 是 Bot」的局（`dispatch/86` 门⑤命中分支）。

背景（`docs/06` §46.5）：`GraphwarLocalPlayerManager::handle_player_info`（`0x519f40`）的
第五道门（`0x519fb1` → `get_module_wk::<ComputerPlayerManager>` → Bot 表按 `player_id` 查）
只在**本连接的第一个 `player_info`** 上求值一次（门④ `0x519f9d` 旧 len==0 先拦）。
原版 LAN 局里那个"第一发"永远是本机玩家自己（pid 229 / conn 228）⇒ 3 局都走未命中。
要打中它，必须让**第一发**是一个已经在 CPM Bot 表里的 `player_id`。

做法：**自建一个说同一套协议的服务器**，让真游戏客户端当**加入方**连进来，
由服务器控制报文顺序（这就是"纯 Bot 客户端 / 连接重建"那条路的可控版本）。

* LAN 发现：往客户端绑定的 UDP 端口灌一条 9 字段 `GameInfo`（`docs/09` §1.1.1 的格式，
  变体 C，见 `docs/09` §6.6.3 —— 实测这是唯一被 UI 接受的形态），`address` 指向本工具；
* 对局通信：TCP，`u16` 大端长度 + JSON（`docs/09` §1.1）；
* 顺序（`--mode hit`）：
  `NewConnectionRequest` → [`NewConnection` + 6 条 Info + `ConnectedToServer`]
  → 客户端 `NewPlayerRequest` → `BotAdded{232}`（⇒ CPM 插表 + 客户端自己发 59/50/52/54/66）
  → **第一发** `PlayerInfo{player_id=232, connection_id=<我的>}` ⇒ 门⑤ 命中
* 判据（两条独立）：
  ① **线上**：命中 ⇒ 服务器**收不到**该 pid 的 `NameRequest/SkinGraphicsRequest/
     FaceGraphicsRequest/HatGraphicsRequest/RankRequest`（59/50/52/54/66）；
  ② **frida**（`tools/frida_cpm_gates.js`）：`gate5_enter` → `gate5_found`（`0x51a0e0`）且零 `send`。
* 对照（`--mode nobot`）：同样的顺序但**不发 `BotAdded`** ⇒ 第一发 player_info 是个"不是 Bot"
  的 id ⇒ 门⑤ 求值但未命中（`0x51a084`）⇒ 5 条请求照发。

只写日志、不改游戏内存。用法见文件尾 `main()` 里的 `--help`。
"""
import argparse
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


def enc(name, body):
    """单键对象 + `u16` 大端长度（`docs/09` §1.1）。"""
    payload = json.dumps({name: body}, separators=(',', ':')).encode()
    return struct.pack('>H', len(payload)) + payload


def game_info(room, address, count=2, state='Setup'):
    """9 字段 `GameInfo`（`docs/09` §1.1.1）；结尾带一个换行（变体 C）。"""
    return (json.dumps({
        'room_name': room,
        'game_state': state,
        'current_player_count': count,
        'address': address,
        'axis_mode': 'EveryUnit',
        'function_mode': 'NormalFunction',
        'turn_mode': 'SequentialTurns',
        'time_mode': 'Timer1m',
        'locked': False,
    }, separators=(',', ':')) + '\n').encode()


class Broadcaster(threading.Thread):
    """周期性把一条房间记录灌进客户端的 LAN 发现端口。"""

    def __init__(self, targets, payload, period=0.5, stop=None):
        super().__init__(daemon=True)
        self.targets = targets
        self.payload = payload
        self.period = period
        self.stop = stop or threading.Event()
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

    def read_frame(self, timeout):
        """返回 (name, body) 或 None（超时 / 对端关闭）。"""
        deadline = time.time() + timeout
        while True:
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
                    return ('<unparsed>', {'raw': txt})
                name = next(iter(obj))
                return (name, obj[name])
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
                log({'type': 'peer_closed'})
                return None
            self.buf += chunk


REQ_TAGS = ('NameRequest', 'SkinGraphicsRequest', 'FaceGraphicsRequest',
            'HatGraphicsRequest', 'RankRequest')


def pump_for(reader, seconds, stats):
    """在 `seconds` 秒内**持续读**，逐帧打收到时刻（别用 `sleep` —— 那会把到达时刻压到一起）。"""
    deadline = time.time() + seconds
    while True:
        left = deadline - time.time()
        if left <= 0:
            return
        f = reader.read_frame(left)
        if f is None:
            return
        name, body = f
        log({'type': 'recv', 'name': name, 'body': body})
        if name in REQ_TAGS:
            stats.setdefault('requests', []).append({'tag_name': name, 'body': body})


def serve(conn, addr, args, stats):
    reader = FrameReader(conn)
    log({'type': 'accepted', 'peer': '%s:%d' % addr})
    conn_id = args.connection_id

    # ① 客户端连上后第一条报文（真实客户端发 NewConnectionRequest）
    first = reader.read_frame(args.handshake_wait)
    if first:
        log({'type': 'recv', 'name': first[0], 'body': first[1]})
    else:
        log({'type': 'no_client_frame', 'note': '客户端没说第一句，主动发 NewConnection'})

    # ② 握手 7 条（顺序照 docs/archive/33 §1）+ ConnectedToServer
    hs = [
        ('NewConnection', {'connection_id': conn_id}),
        ('TurnModeInfo', {'turn_mode': 'SequentialTurns'}),
        ('GameStateInfo', {'game_state': args.game_state}),
        ('GameNameInfo', {'game_name': 'Graphwar Game'}),
        ('AxisModeInfo', {'axis_mode': 'EveryUnit'}),
        ('FunctionModeInfo', {'function_mode': 'NormalFunction'}),
        ('TimeModeInfo', {'time_mode': 'Timer1m'}),
    ]
    for name, body in hs:
        conn.sendall(enc(name, body))
        log({'type': 'send', 'name': name, 'body': body})
    # `ConnectedToServer` 在真实抓包里没出现（`docs/07` §3.3 记"逐位（无抓包）"），
    # 这里显式发一条；**先看客户端自己会不会发 `NewPlayerRequest`**（= 它内部产生了该事件）
    req = reader.read_frame(1.5)
    if req:
        log({'type': 'recv', 'name': req[0], 'body': req[1]})
        stats['client_emitted_connected'] = req[0] == 'NewPlayerRequest'
    if not stats.get('client_emitted_connected'):
        conn.sendall(enc('ConnectedToServer', {'connection_id': conn_id}))
        log({'type': 'send', 'name': 'ConnectedToServer', 'body': {'connection_id': conn_id}})
        req = reader.read_frame(3.0)
        if req:
            log({'type': 'recv', 'name': req[0], 'body': req[1]})

    # ②b `rebuild`：先按"原版局面"来一发 self 的 player_info（门⑤求值一次、未命中），
    #     再发**第二条 `NewConnection`**（= 连接重建）⇒ 客户端 `handle_connected_to_server`
    #     把门④的旧 len 清 0 ⇒ 下一个 player_info 又能走到门⑤
    if args.mode == 'rebuild':
        body = {'player_id': args.self_pid, 'connection_id': conn_id}
        conn.sendall(enc('PlayerInfo', body))
        log({'type': 'send', 'name': 'PlayerInfo', 'body': body, 'role': 'self(first)'})
        pump_for(reader, args.gap, stats)

    # ③ Bot：先申报（`BotAdded` ⇒ 客户端 `add_computer_player` 插表 + 自己发 5 条）
    if args.mode in ('hit', 'selffirst', 'rebuild'):
        body = {'connection_id': conn_id, 'player_id': args.bot_pid, 'level': args.bot_level}
        conn.sendall(enc('BotAdded', body))
        log({'type': 'send', 'name': 'BotAdded', 'body': body})
        # 持续读（别 sleep）：客户端插表后会**自己**发 59/50/52/54/66（CPM 申报身份），
        # 这几帧的到达时刻必须与下面那条 `PlayerInfo` 分开记，否则无法判"是谁发的"
        pump_for(reader, args.gap, stats)

    # ④ 第一发 player_info（`--mode selffirst` 先发"本机玩家自己"，模拟原版局面）
    if args.mode == 'selffirst':
        body = {'player_id': args.self_pid, 'connection_id': conn_id}
        conn.sendall(enc('PlayerInfo', body))
        log({'type': 'send', 'name': 'PlayerInfo', 'body': body, 'role': 'self(first)'})
        pump_for(reader, args.gap, stats)

    if args.mode == 'rebuild':
        # 连接重建：第二条 `NewConnection`（新 connection_id）⇒ 客户端重新握手
        new_id = conn_id + 1
        conn.sendall(enc('NewConnection', {'connection_id': new_id}))
        log({'type': 'send', 'name': 'NewConnection', 'body': {'connection_id': new_id},
             'role': 'rebuild'})
        pump_for(reader, 1.5, stats)
        conn_id = new_id

    pid = args.bot_pid   # selffirst 的第一发是 self（上面那段），这一发永远是 Bot
    body = {'player_id': pid, 'connection_id': conn_id}
    conn.sendall(enc('PlayerInfo', body))
    log({'type': 'send', 'name': 'PlayerInfo', 'body': body,
         'role': ('first-after-connect' if args.mode == 'hit'
                  else ('bot(second)' if args.mode == 'selffirst' else 'bot(after-rebuild)'))})


    # ⑤ 收尾：把客户端发出来的请求全记下来（判据①的原始材料）
    deadline = time.time() + args.collect
    while time.time() < deadline:
        f = reader.read_frame(deadline - time.time())
        if f is None:
            break
        name, body = f
        log({'type': 'recv', 'name': name, 'body': body})
        if name in REQ_TAGS:
            stats.setdefault('requests', []).append({'tag_name': name, 'body': body})
    log({'type': 'session_done', 'requests': stats.get('requests', [])})


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--mode', choices=('hit', 'nobot', 'selffirst', 'rebuild'), default='hit')
    ap.add_argument('--tcp-port', type=int, default=46701)
    ap.add_argument('--connection-id', type=int, default=230)
    ap.add_argument('--bot-pid', type=int, default=232)
    ap.add_argument('--self-pid', type=int, default=229)
    ap.add_argument('--bot-level', type=int, default=1000)
    ap.add_argument('--game-state', default='Setup')
    ap.add_argument('--room', default='gate5')
    ap.add_argument('--gap', type=float, default=2.0, help='BotAdded 与 PlayerInfo 之间等多久')
    ap.add_argument('--collect', type=float, default=30.0, help='发完第一发 player_info 后收多久')
    ap.add_argument('--handshake-wait', type=float, default=8.0)
    ap.add_argument('--udp-port', type=int, default=42968)
    ap.add_argument('--udp-targets', default='127.0.0.1,255.255.255.255')
    ap.add_argument('--out', required=True)
    ap.add_argument('--broadcast-for', type=float, default=0.0,
                    help='>0 时到点自动退出（否则一直发，Ctrl-C 结束）')
    args = ap.parse_args()

    global LOG
    LOG = open(args.out, 'w', encoding='utf-8')

    address = '127.0.0.1:%d' % args.tcp_port
    payload = game_info(args.room, address)
    targets = [(h, args.udp_port) for h in args.udp_targets.split(',') if h]
    bc = Broadcaster(targets, payload)
    bc.start()
    log({'type': 'config', 'mode': args.mode, 'tcp_port': args.tcp_port,
         'advertised': address, 'udp_targets': targets, 'room': args.room,
         'game_info': payload.decode().strip()})

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(('127.0.0.1', args.tcp_port))
    srv.listen(4)
    srv.settimeout(1.0)
    log({'type': 'listening', 'on': address})

    stats = {}
    t_end = time.time() + args.broadcast_for if args.broadcast_for > 0 else None
    try:
        while t_end is None or time.time() < t_end:
            try:
                conn, addr = srv.accept()
            except socket.timeout:
                continue
            try:
                serve(conn, addr, args, stats)
            finally:
                conn.close()
            if args.mode:
                break  # 只服务一条连接（判据只需要一次）
    except KeyboardInterrupt:
        pass
    finally:
        bc.stop.set()
        log({'type': 'summary', 'udp_sent': bc.sent,
             'client_emitted_connected': stats.get('client_emitted_connected'),
             'requests': stats.get('requests', [])})
        LOG.close()


if __name__ == '__main__':
    main()
