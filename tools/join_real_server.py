#!/usr/bin/env python3
"""连上真游戏的服务器：握手 + 加入，打印收到的报文。"""
import json, socket, struct, sys, time

port = int(sys.argv[1]) if len(sys.argv) > 1 else 46868
s = socket.create_connection(("127.0.0.1", port), timeout=10)
s.settimeout(2.0)
buf = b""

def send(o):
    b = json.dumps(o, separators=(",", ":")).encode()
    s.sendall(struct.pack(">H", len(b)) + b)
    print("  >>", json.dumps(o, separators=(",", ":"))[:100])

def pump(limit=300, quiet=1.5):
    global buf
    out = []
    s.settimeout(quiet)
    while len(out) < limit:
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
            out.append(buf[2:2+n].decode())
            buf = buf[2+n:]
    return out

send({"NewConnectionRequest": {"major_version": "2.0", "minor_version": "60"}})
hello = pump()
conn = None
for m in hello:
    o = json.loads(m)
    if "NewConnection" in o:
        conn = o["NewConnection"]["connection_id"]
print(f"握手收到 {len(hello)} 条, connection_id={conn}")
for m in hello[:8]:
    print("   ", m[:90])

if conn is not None:
    send({"NewPlayerRequest": {"connection_id": conn}})
    more = pump()
    print(f"加入后收到 {len(more)} 条:")
    for m in more[:14]:
        print("   ", m[:100])
