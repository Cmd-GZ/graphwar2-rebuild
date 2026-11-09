#!/usr/bin/env python3
"""tools/gate5_real_join.py —— 当**加入方**连**真游戏自己开的房源**，把收到的报文逐条记下来（`dispatch/92`）。

和 [`tools/gate5_serve.py`](gate5_serve.py) 正好相反：那边**我们当服务器**（控帧序 ⇒ 只能证明"命中分支可达"），
这边**我们当客户端**（真服务器控帧序 ⇒ 才能回答"真房源里 `BotAdded` 与 `PlayerInfo` 谁先"）。

* 端口：真房源的 TCP 口是**每局随机**的（`docs/09` §1.1 抓到的 `[::1]:35245`；按 `--pid` 的 `ss -tlnp` 找）；
* 帧：`u16` 大端长度 + JSON（`docs/09` §1.1）；
* 握手：先发 `NewConnectionRequest`（`{major_version:"2.0", minor_version:"60"}`），收到 `NewConnection` 后
  发 `NewPlayerRequest{connection_id}`（`docs/09` §3.1 / §48.4 的同一套），随后**只收不发**；
* 输出：每行一个 JSON（`t` = 本工具单调钟 ms，`dir` = in/out，`name` = JSON 第一个键，`text` = 原文）。

只读：不改游戏内存、不改游戏文件；连的是真游戏进程自己 listen 的那个 socket。

用法：

    $PY tools/gate5_real_join.py --pid <游戏PID> --out <日志> --listen-timeout 180 --hold 60
    $PY tools/gate5_real_join.py --addr 198.18.0.1:46809 --out <日志> --hold 30   # 端口已知时

退出码：0 = 至少收到一帧；3 = 端口等不到；4 = 连上了但服务器一个字都没回（= `docs/09` §5.5 那种局面）。
"""
import argparse
import json
import os
import socket
import struct
import subprocess
import sys
import time

T0 = time.time()


def now_ms():
    return round((time.time() - T0) * 1000, 1)


class Log:
    def __init__(self, path):
        self.f = open(path, 'w', encoding='utf-8') if path else None

    def __call__(self, obj):
        obj['t'] = now_ms()
        line = json.dumps(obj, ensure_ascii=False)
        print(line, file=sys.stderr, flush=True)
        if self.f:
            self.f.write(line + '\n')
            self.f.flush()

    def close(self):
        if self.f:
            self.f.close()


def listen_ports(pid):
    """`ss -tlnp` 里属于这个 PID 的监听地址（不带 `-p` 匹配不到进程名，见 `docs/09` §5.2）。"""
    try:
        out = subprocess.run(['ss', '-tlnp'], capture_output=True, text=True, timeout=5).stdout
    except Exception:  # noqa: BLE001
        return []
    rows = []
    for line in out.splitlines():
        if 'pid=%d,' % pid not in line:
            continue
        parts = line.split()
        if len(parts) >= 4:
            rows.append(parts[3])
    return rows


def listen_by_host(host):
    """`ss -tln`（**不带** `-p`）里绑定在 `host` 上的监听口。

    为什么要这条：跑游戏的那条命令在**自己的 PID 命名空间**里（`bwrap --unshare-pid`，见 AGENTS 的坑），
    所以另一条命令 `ss -tlnp` / `pgrep` 都**看不到**游戏进程 —— 但**网络命名空间是共享的**，
    `ss -tln` 能看到它的 socket。真房源固定绑在登录用的那块非环回地址上（实测 `198.18.0.1`，
    与 GameSetup 左下角 `GameIpLabel` 逐字一致），所以按 host 找比按 pid 找更稳。
    """
    try:
        out = subprocess.run(['ss', '-tln'], capture_output=True, text=True, timeout=5).stdout
    except Exception:  # noqa: BLE001
        return []
    rows = []
    for line in out.splitlines():
        parts = line.split()
        if len(parts) >= 4 and parts[0] == 'LISTEN' and parts[3].startswith(host + ':'):
            rows.append(parts[3])
    return rows


def wait_for_addr(args, log):
    deadline = time.time() + args.listen_timeout
    while time.time() < deadline:
        if args.ss_host:
            rows = listen_by_host(args.ss_host)
            if rows:
                log({'type': 'listen_seen', 'host': args.ss_host, 'addrs': rows})
                a = rows[0]
                host, _, port = a.rpartition(':')
                return host, int(port), a
            time.sleep(0.5)
            continue
        rows = listen_ports(args.pid)
        if rows:
            log({'type': 'listen_seen', 'pid': args.pid, 'addrs': rows})
            # 优先 198.18.0.1 / 本机可路由地址；`0.0.0.0:p` / `[::]:p` 就走 127.0.0.1
            for a in rows:
                host, _, port = a.rpartition(':')
                host = host.strip('[]')
                if host in ('0.0.0.0', '*', '::', ''):
                    return '127.0.0.1', int(port), a
                return host, int(port), a
        time.sleep(0.5)
    return None, None, None


def enc(name, body):
    payload = json.dumps({name: body}, separators=(',', ':')).encode()
    return struct.pack('>H', len(payload)) + payload


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', required=True)
    ap.add_argument('--pid', type=int, default=0, help='游戏 PID（同一 PID 命名空间里才看得见）')
    ap.add_argument('--ss-host', default=None,
                    help='按 `ss -tln` 里绑定的 IP 找端口（跨 PID 命名空间唯一可行的办法，见 wait_for_addr）')
    ap.add_argument('--addr', default=None, help='host:port；给了就不去 ss 里找')
    ap.add_argument('--listen-timeout', type=float, default=180.0)
    ap.add_argument('--hold', type=float, default=60.0, help='握手后继续收多少秒')
    ap.add_argument('--no-join', action='store_true', help='只握手、不发 NewPlayerRequest')
    ap.add_argument('--quiet-after', type=float, default=2.0, help='多久没帧就认为这轮读完了')
    args = ap.parse_args()

    log = Log(args.out)
    if args.addr:
        host, _, port = args.addr.rpartition(':')
        host = host.strip('[]') or '127.0.0.1'
        port = int(port)
        shown = args.addr
    else:
        host, port, shown = wait_for_addr(args, log)
        if not host:
            log({'type': 'fatal', 'why': 'no-listen-port', 'pid': args.pid})
            return 3
    log({'type': 'target', 'host': host, 'port': port, 'ss_addr': shown})

    try:
        s = socket.create_connection((host, port), timeout=5)
    except OSError as e:  # noqa: BLE001
        log({'type': 'fatal', 'why': 'connect-failed', 'err': str(e)})
        return 3
    s.setblocking(False)
    log({'type': 'connected', 'host': host, 'port': port})
    s.sendall(enc('NewConnectionRequest', {'major_version': '2.0', 'minor_version': '60'}))
    log({'type': 'sent', 'name': 'NewConnectionRequest'})

    buf = b''
    frames = 0
    conn = None
    joined = False
    end = time.time() + args.hold
    last_frame = time.time()
    while time.time() < end:
        try:
            chunk = s.recv(65536)
        except BlockingIOError:
            chunk = None
        except OSError as e:  # noqa: BLE001
            log({'type': 'recv_error', 'err': str(e)})
            break
        if chunk == b'':
            log({'type': 'peer_closed'})
            break
        if chunk:
            buf += chunk
            while len(buf) >= 2:
                n = struct.unpack('>H', buf[:2])[0]
                if len(buf) < 2 + n:
                    break
                text = buf[2:2 + n].decode('utf-8', 'replace')
                buf = buf[2 + n:]
                frames += 1
                last_frame = time.time()
                try:
                    name = next(iter(json.loads(text)))
                    body = json.loads(text)[name]
                except Exception:  # noqa: BLE001
                    name, body = '<unparsed>', None
                log({'type': 'frame', 'dir': 'in', 'name': name, 'n': n, 'body': body, 'text': text})
                if name == 'NewConnection' and not joined and not args.no_join:
                    conn = body.get('connection_id')
                    s.sendall(enc('NewPlayerRequest', {'connection_id': conn}))
                    log({'type': 'sent', 'name': 'NewPlayerRequest', 'connection_id': conn})
                    joined = True
                    last_frame = time.time()
        else:
            time.sleep(0.02)
            if frames and time.time() - last_frame > args.quiet_after:
                log({'type': 'quiet', 'frames': frames})
                break
    s.close()
    log({'type': 'done', 'frames': frames})
    log.close()
    return 0 if frames else 4


if __name__ == '__main__':
    sys.exit(main())
