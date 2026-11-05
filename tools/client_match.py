#!/usr/bin/env python3
"""最小客户端：连上服务端、握手、开局、打几发，打印收到的报文种类。"""
import json
import socket
import struct
import sys

port = int(sys.argv[1]) if len(sys.argv) > 1 else 39997
s = socket.create_connection(("127.0.0.1", port), timeout=15)
buf = b""

def send(obj):
    body = json.dumps(obj, separators=(",", ":")).encode()
    s.sendall(struct.pack(">H", len(body)) + body)

def pump(limit=200, stop_on=None):
    """读帧直到安静下来；返回收到的消息列表。"""
    global buf
    got = []
    s.settimeout(1.0)
    while len(got) < limit:
        try:
            chunk = s.recv(65536)
        except socket.timeout:
            break
        if not chunk:
            break
        buf += chunk
        while len(buf) >= 2:
            n = struct.unpack(">H", buf[:2])[0]
            if len(buf) < 2 + n:
                break
            txt = buf[2:2 + n].decode()
            buf = buf[2 + n:]
            got.append(txt)
            if stop_on and stop_on in txt:
                return got
    return got

send({"NewConnectionRequest": {"major_version": "2.0", "minor_version": "60"}})
hello = pump()
print("握手:", [json.loads(m).popitem()[0] for m in hello])

send({"GameStartRequest": {}})
start = pump()
kinds = {}
for m in start:
    k = json.loads(m).popitem()[0]
    kinds[k] = kinds.get(k, 0) + 1
print("开局:", kinds)

def who_plays(msgs):
    for m in msgs:
        obj = json.loads(m)
        if "TurnInfo" in obj:
            return obj["TurnInfo"]["player_id"]
    return None

turn = 0
current = who_plays(start)
for i in range(12):
    if current is None:
        break
    send({"FunctionUpdateRequest": {"player_id": current, "function": "0.00"}})
    pump(limit=5)
    send({"FunctionFireRequest": {"player_id": current}})
    got = pump()
    ks = [json.loads(m).popitem()[0] for m in got]
    if not ks:
        print("没有回应，结束")
        break
    turn += 1
    print(f"第 {turn} 发 (player {current}): 共 {len(ks)} 条 -> {ks}")
    if any("EndingGame" in m for m in got):
        print("对局结束（EndingGame）")
        break
    current = who_plays(got)
print("共收到回合数:", turn)
