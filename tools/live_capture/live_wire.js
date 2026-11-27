/**
 * tools/live_capture/live_wire.js —— T53「连官服抓包」的线上抓帧探针（只读观测，不改游戏内存）。
 *
 * **来源**：[`tools/frida_gate5_wire.js`](../../frida_gate5_wire.js) 的 **A 半（线上视角）逐行拷贝**，
 * 只做了三条**加法**（不改任何一条钩子语义，理由见下）：
 *  ① 帧记录多两个字段 `role`（`client`=connect 出来的 / `server`=accept 出来的）与 `peer`（对端地址）
 *     —— 房主局两端同进程，光看 `dir` 分不出方向（§51 的 JSONL 就是全 `in`、0 个 `out`）；
 *  ② 帧记录带**完整 `hex`**（不再 `text[:1200]` 截断），用于算帧负载的 sha256 当证据；
 *  ③ 去掉 B 半（门⑤ 的 17 个进程内钩子）—— 与本任务无关，且少一半噪声。
 *
 * ## ⚠️ 三条实测结论（都是本任务踩出来的，别再改回去）
 *
 * 1. **只挂 `recv`，不许挂 `read`/`write`/`writev`**：
 *    - 读侧 `graphwar_stream_buffer::read_event` 是 `peek(MSG_PEEK)` 拿长度前缀 + `read_exact` 取正文
 *      （`docs/02` §17 已逐位复原），`MSG_PEEK` 必须跳过（`flags & 0x2`），否则同一批字节喂两遍；
 *    - 再挂 `read` 会把 `read_exact` 的正文再喂一遍 ⇒ 前缀/正文错位（实测 D1 跑解出 n=8762 的假帧）；
 *    - **挂 `write`/`writev` 会自噬**：Frida 自己的消息通道在目标进程里也走 libc 写，
 *      `emit() → send() → write() → onEnter → feed() → emit() → …`，实测 F1 只出 2 帧、
 *      F2 整整 160 s 一条消息都没有（连每秒一次的 LAN 广播都停了）。
 * 2. **分帧只能靠"嗅 `{`/`}` 重同步"**（本文件 `decodeFrames` 沿用 §51 的实现）：因为
 *    `read_exact` 只交正文、前缀来自被跳过的 peek，缓冲里**可能没有前缀**。
 *    改成"等够 n 字节再切"的严格写法会永远等下去（实测 F3/F4：0 帧）。代价是极端情况下丢首帧，
 *    但 `n`（探针记的负载长）与 `hex` 长度可以交叉自证。
 * 3. 帧格式：`u16 大端长度` + 该长度的 JSON（长度只算负载，`docs/07` §3.1 / `docs/09` §1.1）。
 */
'use strict';

let seq = 0;
const t0 = Date.now();
const stamp = () => Date.now() - t0;
const wall = () => Math.round(Date.now() / 1000);
function emit(obj) { obj.t = stamp(); obj.wall = wall(); obj.seq = ++seq; send(obj); }

const libc = Process.getModuleByName('libc.so.6');
const SOCK = {};            // fd -> 'tcp' | 'udp'
const SERVER_FD = {};       // 监听 fd
const CONN = {};            // 已建立的 TCP 连接 fd（accept 或 connect 成功）
const ROLE = {};            // fd -> 'client'（connect 出来）/ 'server'（accept 出来）
const PEER = {};            // fd -> 对端 'ip:port'
const BUF = {};             // (fd|dir) -> 未成帧的字节
// Frida 的 V8 里没有 TextDecoder（实测：顶层 `new TextDecoder` 直接 ReferenceError ⇒ script.load() 失败、
// 一条消息都不会到 driver，看起来像"游戏没连网"）。自己解：JSON 是 UTF-8，非 ASCII 只在玩家名里。
function utf8(bytes) {
  let out = '', i = 0;
  const n = bytes.length;
  while (i < n) {
    const b = bytes[i++];
    if (b < 0x80) { out += String.fromCharCode(b); continue; }
    if (b < 0xc0) { continue; }
    if (b < 0xe0) { out += String.fromCharCode(((b & 0x1f) << 6) | (bytes[i++] & 0x3f)); continue; }
    if (b < 0xf0) {
      out += String.fromCharCode(((b & 0x0f) << 12) | ((bytes[i++] & 0x3f) << 6) | (bytes[i++] & 0x3f));
      continue;
    }
    const cp = ((b & 0x07) << 18) | ((bytes[i++] & 0x3f) << 12) |
               ((bytes[i++] & 0x3f) << 6) | (bytes[i++] & 0x3f);
    out += String.fromCodePoint(cp);
  }
  return out;
}

function sockaddrStr(p, addrlen) {
  try {
    const fam = p.readU16();
    if (fam === 2) {                                   // AF_INET
      const port = (p.add(2).readU8() << 8) | p.add(3).readU8();
      const b = new Uint8Array(p.add(4).readByteArray(4));
      return b.join('.') + ':' + port;
    }
    if (fam === 10) {                                  // AF_INET6
      const port = (p.add(2).readU8() << 8) | p.add(3).readU8();
      const b = new Uint8Array(p.add(8).readByteArray(16));
      const parts = [];
      for (let i = 0; i < 16; i += 2) parts.push(((b[i] << 8) | b[i + 1]).toString(16));
      // ::ffff:a.b.c.d 形式（IPv4 映射）压平，读起来更像文档里的样本
      if (parts.slice(0, 5).join(':') === '0:0:0:0:0' && parts[5] === 'ffff') {
        return '::ffff:' + b.slice(12).join('.') + ':' + port;
      }
      return '[' + parts.join(':') + ']:' + port;
    }
    return 'af' + fam + ':' + (addrlen ? addrlen.toInt32() : '?');
  } catch (e) { return null; }
}

function decodeFrames(fd, dir) {
  const key = fd + '|' + dir;
  let b = BUF[key];
  if (!b || b.length < 2) return;
  const out = [];
  let i = 0;
  while (b.length - i >= 2) {
    const n = (b[i] << 8) | b[i + 1];
    // 合法帧 = 长度前缀 + 以 '{' 开头、'}' 结尾的 JSON（`docs/09` §1.2 全部是单键对象）
    if (n === 0 || n > 8192 || i + 2 >= b.length || b[i + 2] !== 0x7b) { i += 1; continue; }
    if (b.length - i - 2 < n) break;
    if (b[i + 1 + n] !== 0x7d) { i += 1; continue; }
    out.push({ n: n, bytes: b.slice(i + 2, i + 2 + n) });
    i += 2 + n;
  }
  BUF[key] = b.slice(i);
  for (const f of out) {
    const txt = utf8(f.bytes);
    let name = null;
    try { name = Object.keys(JSON.parse(txt))[0]; } catch (e) { name = '<unparsed>'; }
    emit({ type: 'frame', fd: fd, dir: dir, role: ROLE[fd] || null,
           peer: PEER[fd] || null, name: name, n: f.n,
           hex: f.bytes.map(function (b) { return (b < 16 ? '0' : '') + b.toString(16); }).join(''),
           text: txt });
  }
}

function feed(fd, dir, ptr, len) {
  if (!(fd in CONN)) return;
  if (!(len > 0) || len > 65536) return;
  let bytes;
  try { bytes = Array.from(new Uint8Array(ptr.readByteArray(len))); } catch (e) { return; }
  const key = fd + '|' + dir;
  const prev = BUF[key] || [];
  BUF[key] = prev.concat(bytes);
  decodeFrames(fd, dir);
}

function hook(name, cb) {
  try {
    const addr = libc.getExportByName(name);
    if (!addr) return emit({ type: 'hookfail', name: name, why: 'no-such-export' });
    Interceptor.attach(addr, cb);
  } catch (e) { emit({ type: 'hookfail', name: name, err: String(e) }); }
}

hook('socket', {
  onEnter(args) { this.fam = args[0].toInt32(); this.ty = args[1].toInt32(); },
  onLeave(ret) {
    const fd = ret.toInt32();
    if (fd < 0) return;
    const kind = (this.ty & 0xf) === 1 ? 'tcp' : ((this.ty & 0xf) === 2 ? 'udp' : 'other');
    SOCK[fd] = kind;
    emit({ type: 'sock', fd: fd, fam: this.fam, ty: this.ty, kind: kind });
  },
});

hook('bind', {
  onEnter(args) {
    emit({ type: 'bind', fd: args[0].toInt32(), kind: SOCK[args[0].toInt32()] || null,
           addr: sockaddrStr(args[1], args[2]) });
  },
});

hook('listen', {
  onEnter(args) {
    const fd = args[0].toInt32();
    SERVER_FD[fd] = true;
    emit({ type: 'listen', fd: fd, backlog: args[1].toInt32() });
  },
});

for (const nm of ['accept', 'accept4']) {
  hook(nm, {
    onEnter(args) { this.lfd = args[0].toInt32(); this.p = args[1]; },
    onLeave(ret) {
      const fd = ret.toInt32();
      if (fd < 0) return;
      CONN[fd] = true;
      ROLE[fd] = 'server';
      PEER[fd] = sockaddrStr(this.p, null);
      emit({ type: 'accept', lfd: this.lfd, fd: fd, peer: PEER[fd] });
    },
  });
}

hook('connect', {
  onEnter(args) { this.fd = args[0].toInt32(); this.addr = sockaddrStr(args[1], args[2]); },
  onLeave(ret) {
    // 异步 connect 会返回 EINPROGRESS(-1)；那条连接照样要读，所以**一律**登记
    if (SOCK[this.fd] === 'tcp') { CONN[this.fd] = true; ROLE[this.fd] = 'client'; PEER[this.fd] = this.addr; }
    emit({ type: 'connect', fd: this.fd, addr: this.addr, ret: ret.toInt32(),
           prev_errno: null });
  },
});

hook('send', {
  onEnter(args) { feed(args[0].toInt32(), 'out', args[1], args[2].toInt32()); },
});
// ⚠ 不要同时挂 `write`/`read`：这游戏读写 socket 走 libc `send`/`recv`，两条都挂会把同一段字节
//   喂两遍，帧长前缀就错位（实测 57 处连续重复 + 86 处 `<unparsed>`）。只挂 send/recv。
// ⚠ 必须跳过 `MSG_PEEK`（0x2）：这游戏的读循环**先 peek 再 recv**（判帧头/等放大包），
//   不跳会把同一段字节喂两遍 ⇒ 每个帧都出现两次、后面全部错位（实测 474/500 重复）。
hook('recv', {
  onEnter(args) { this.fd = args[0].toInt32(); this.p = args[1]; this.flags = args[3].toInt32(); },
  onLeave(ret) { if (!(this.flags & 0x2)) feed(this.fd, 'in', this.p, ret.toInt32()); },
});
hook('sendto', {
  onEnter(args) {
    const fd = args[0].toInt32(), len = args[2].toInt32();
    if (!(len > 0) || len > 4096) return;
    let txt = null;
    try { txt = utf8(Array.from(new Uint8Array(args[1].readByteArray(len)))); } catch (e) { return; }
    emit({ type: 'udp_out', fd: fd, dst: sockaddrStr(args[4], args[5]), n: len, text: txt.slice(0, 800) });
  },
});
hook('recvfrom', {
  onEnter(args) {
    this.fd = args[0].toInt32(); this.p = args[1]; this.sp = args[4]; this.al = args[5];
    this.peek = args[3].toInt32() & 0x2;
  },
  onLeave(ret) {
    if (this.peek) return;
    const n = ret.toInt32();
    if (!(n > 0) || n > 4096) return;
    let txt = null;
    try { txt = utf8(Array.from(new Uint8Array(this.p.readByteArray(n)))); } catch (e) { return; }
    emit({ type: 'udp_in', fd: this.fd, src: sockaddrStr(this.sp, this.al), n: n, text: txt.slice(0, 800) });
  },
});


emit({ type: 'ready', n_hooks: 11 });
