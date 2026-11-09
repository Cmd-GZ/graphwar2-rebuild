#!/usr/bin/env python3
"""tools/gw2_lan_capture.py —— 抓**真游戏**自己发出的 LAN 发现广播 + 列它的监听端口（`dispatch/92`）。

和 [`tools/frida_cpm_gates.js`](frida_cpm_gates.js) 的分工：那边看**进程内**的五道门，
这边看**进程外**的两件事，用来定位"真房源"：

1. **LAN 发现广播**（UDP）：真游戏当房主时，把 9 字段 `GameInfo`（`docs/09` §1.1.1）发给
   `BROADCAST_PORT`。绑住那个端口就能读到**原文**，里面 `address` 字段就是真房源服务器地址；
2. **监听端口**：每 `--ss-period` 秒跑一次 `ss -tlnp`，把**本游戏 PID** 的 TCP 监听口记下来
   （`docs/09` §5.5 的三条坑：`/tmp` 逐命令 tmpfs、`pkill -f Godot` 会误杀、`ss -tln` 不带进程名）。

用法（只读，不改游戏内存、不改游戏文件）：

    $PY tools/gw2_lan_capture.py --out $R/lan.jsonl --duration 120 --pid-file $R/game.pid

输出每行一个 JSON：`{"type":"udp"|"listen"|"udp_error","t":…,…}`；`t` 是本工具自己的单调钟 ms。
"""
import argparse
import json
import socket
import struct
import subprocess
import sys
import time

T0 = time.time()


def log(out, obj):
    obj['t'] = round((time.time() - T0) * 1000, 1)
    line = json.dumps(obj, ensure_ascii=False)
    print(line, file=sys.stderr, flush=True)
    if out:
        out.write(line + '\n')
        out.flush()


def bind_udp(port, ipv6=False):
    """尽量把 `port` 占住；REUSEADDR/REUSEPORT 是必须的（真游戏自己也 bind 这个口）。"""
    fam = socket.AF_INET6 if ipv6 else socket.AF_INET
    host = '::' if ipv6 else '0.0.0.0'
    s = socket.socket(fam, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    if hasattr(socket, 'SO_REUSEPORT'):
        try:
            s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
        except OSError:
            pass
    try:
        s.bind((host, port))
    except OSError as e:  # noqa: BLE001
        s.close()
        return None, str(e)
    s.settimeout(0.5)
    # 收 IPv4 广播 / IPv6 全部节点组播（ff02::1）——真游戏走哪条不确定，两条都挂（`docs/09` §11.5）
    if ipv6:
        try:
            group = socket.inet_pton(socket.AF_INET6, 'ff02::1')
            for idx, name in socket.if_nameindex():
                try:
                    s.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_JOIN_GROUP,
                                 group + struct.pack('@I', idx))
                    log(None, {'type': 'joined', 'family': 6, 'if': name, 'idx': idx})
                except OSError:
                    pass
        except OSError:
            pass
    return s, None


def listen_ports(pid):
    """`ss -tlnp` 里属于这个 PID 的 TCP 监听口（不带 `-p` 匹配不到进程名，见 `docs/09` §5.2）。"""
    try:
        out = subprocess.run(['ss', '-tlnp'], capture_output=True, text=True, timeout=5).stdout
    except Exception:  # noqa: BLE001
        return []
    ports = []
    for line in out.splitlines():
        if 'pid=%d,' % pid not in line:
            continue
        parts = line.split()
        if len(parts) >= 4:
            ports.append(parts[3])
    return ports


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', required=True)
    ap.add_argument('--duration', type=float, default=120.0)
    ap.add_argument('--udp-port', type=int, default=55463,
                    help='LAN 发现广播端口（`docs/09` §1.1 抓到的 55463）')
    ap.add_argument('--ss-period', type=float, default=2.0)
    ap.add_argument('--pid', type=int, default=0, help='游戏 PID（0 = 每轮从 --pid-file 读）')
    ap.add_argument('--pid-file', default=None)
    ap.add_argument('--pid-name', default='Graphwar.x86_64',
                    help='PID 文件还没有时按进程名兜底（`pgrep -f`）')
    args = ap.parse_args()

    out = open(args.out, 'w', encoding='utf-8')
    socks = []
    for v6 in (False, True):
        s, err = bind_udp(args.udp_port, v6)
        if s:
            socks.append((v6, s))
        else:
            log(out, {'type': 'bind_error', 'family': 6 if v6 else 4, 'err': err})
    if not socks:
        log(out, {'type': 'fatal', 'msg': 'no udp socket bound'})
        return 2

    seen_ports = None
    end = time.time() + args.duration
    next_ss = 0.0
    while time.time() < end:
        for v6, s in socks:
            try:
                data, src = s.recvfrom(65535)
            except socket.timeout:
                continue
            except OSError as e:  # noqa: BLE001
                log(out, {'type': 'udp_error', 'family': 6 if v6 else 4, 'err': str(e)})
                continue
            txt = data.decode('utf-8', 'replace')
            try:
                obj = json.loads(txt.strip().split('\n')[0])
            except ValueError:
                obj = None
            log(out, {'type': 'udp', 'family': 6 if v6 else 4, 'src': str(src),
                      'nbytes': len(data), 'json': obj, 'text': txt[:400]})
        now = time.time()
        if now >= next_ss:
            next_ss = now + args.ss_period
            pid = args.pid
            if not pid and args.pid_file:
                try:
                    pid = int(open(args.pid_file).read().strip() or 0)
                except (OSError, ValueError):
                    pid = 0
            if not pid and args.pid_name:
                try:
                    r = subprocess.run(['pgrep', '-f', args.pid_name], capture_output=True,
                                       text=True, timeout=5).stdout.split()
                    pid = int(r[0]) if r else 0
                except Exception:  # noqa: BLE001
                    pid = 0
            if pid:
                ports = listen_ports(pid)
                if ports != seen_ports:
                    seen_ports = ports
                    log(out, {'type': 'listen', 'pid': pid, 'ports': ports})
    out.close()
    return 0


if __name__ == '__main__':
    sys.exit(main())
