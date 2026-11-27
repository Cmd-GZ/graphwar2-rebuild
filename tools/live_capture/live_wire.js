/**
 * tools/live_capture/live_wire.js —— T53「连官服抓包」的线上抓帧探针（只读观测）。
 *
 * 与 [`tools/frida_gate5_wire.js`](../../frida_gate5_wire.js) 的关系：那一支是 §51 为「门⑤」写的，
 * 钩子分成 A（线上）/ B（进程内五道门）两半，帧只留 `text[:1200]` 且**不分方向**。
 * 本支是它 A 半的**收紧版**，为的是让抓到的字节能进 `fixtures/` 当证据：
 *
 * | 项 | frida_gate5_wire.js | 本支 |
 * |---|---|---|
 * | 钩子范围 | A 半 + B 半（门⑤ 17 个） | **只 A 半**（socket/bind/listen/accept/connect/send/write/writev/recv/read/sendto/recvfrom） |
 * | 帧原文 | `text` 截到 1200 字节 | **完整 `hex` + 完整 `text`**（hex 用来算帧 sha256） |
 * | 方向 | 只有 `dir=in/out`（`out` 实际一条都没有，见下） | `dir` + **`role`**（`client`=connect 出来的 / `server`=accept 出来的） |
 * | 分帧 | u16 大端 + 嗅 `'{'`/`'}'` 重同步 | 同法（+ 记录重同步次数，异常可见） |
 * | 时间 | `t` = 相对 start 的 ms | 同法 + `wall` 绝对 epoch 秒 |
 *
 * ⚠️ **为什么必须记 `role`**：房主局里 TCP 两端**都在本进程**（客户端 `connect` 到自己的
 * `[::1]:port` 游戏服务器），所以同一条帧会被两个 fd 各读一次 ——
 * `server→client` 的帧是**客户端 fd 的 `recv`**（`dir=in, role=client`），
 * `client→server` 的帧是**服务端 fd 的 `recv`**（`dir=in, role=server`）。
 * 只看 `dir` 会把两个方向混在一起（§51 的 JSONL 就是这样：全 `in`、0 个 `out`）。
 *
 * ⚠️ **写方向走 `write`/`writev`，不走 `send`**（实测：`send` 钩子在本二进制上 0 命中，
 * 而两端 `recv` 都拿到了完整字节）。所以本支把 `send`/`write`/`writev` 全挂上，
 * 每条 chunk 带 `via` 字段，跑完可以核对"到底哪个 syscall 承载字节"。
 * `recv` 必须跳过 `MSG_PEEK`（0x2，这游戏的读循环先 peek 再 recv，不跳会双喂）。
 *
 * 帧格式（`docs/09` §1.1 / `docs/07` §3.1）：`u16 大端长度` + `长度字节的 JSON`，长度只算负载。
 */
'use strict';

const LIB = Process.getModuleByName('libc.so.6');
const MAXPAY = 0xffff;                // graphwar_stream_buffer::MAX_PAYLOAD
const T0 = Date.now();
let seq = 0;

const ROLE = {};        // fd -> 'client' | 'server'
const KIND = {};        // fd -> 'tcp' | 'udp' | 'other'
const PEER = {};        // fd -> 'ip:port'
const BUF = {};         // (fd|dir) -> bytes[]
const RSYNC = {};       // (fd|dir) -> 重同步次数

function emit(o) {
  o.t = Date.now() - T0;
  o.wall = Math.round(Date.now() / 1000);
  o.seq = ++seq;
  send(o);
}

function hexOf(bytes) {
  let s = '';
  for (let i = 0; i < bytes.length; i++) s += (bytes[i] < 16 ? '0' : '') + bytes[i].toString(16);
  return s;
}

// Frida 的 V8 里没有 TextDecoder（`tools/frida_gate5_wire.js` 已踩过）⇒ 自己解 UTF-8。
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

function sockaddr(p, addrlen) {
  try {
    const fam = p.readU16();
    if (fam === 2) {                                        // AF_INET
      const port = (p.add(2).readU8() << 8) | p.add(3).readU8();
      const b = new Uint8Array(p.add(4).readByteArray(4));
      return b.join('.') + ':' + port;
    }
    if (fam === 10) {                                       // AF_INET6
      const port = (p.add(2).readU8() << 8) | p.add(3).readU8();
      const b = new Uint8Array(p.add(8).readByteArray(16));
      const parts = [];
      for (let i = 0; i < 16; i += 2) parts.push(((b[i] << 8) | b[i + 1]).toString(16));
      if (parts.slice(0, 5).join(':') === '0:0:0:0:0' && parts[5] === 'ffff') {
        return '::ffff:' + b.slice(12).join('.') + ':' + port;
      }
      if (parts.slice(0, 7).join(':') === '0:0:0:0:0:0:0') return '[::1]:' + port;
      return '[' + parts.join(':') + ']:' + port;
    }
    return 'af' + fam + ':' + (addrlen ? addrlen.toInt32() : '?');
  } catch (e) { return null; }
}

/** 把累积字节切帧：`u16 大端长度` + 负载；负载必须像单键 JSON 对象，否则逐字节重同步。
 *
 * ⚠️ **数据不够时必须 `break` 等下一批，绝不许重同步**（实测 F1 跑踩到的坑）：
 * 这游戏的读循环**先单独读 2 字节长度前缀**再读正文，于是第一次 `feed` 的缓冲只有 2 字节 ——
 * 旧写法在"看不清负载首字节"时也 `i += 1`，把一个长度前缀的**低字节**当成垃圾丢掉，
 * 整条流从此错位（解出 n=8762 的假帧、后面的真帧全被吃掉）。
 * 判据：只有"长度不合法"或"长度合法且字节够、但形态不对"才重同步。
 */
function decode(fd, dir) {
  const key = fd + '|' + dir;
  let b = BUF[key];
  if (!b || b.length < 2) return;
  let i = 0;
  const out = [];
  while (b.length - i >= 2) {
    const n = (b[i] << 8) | b[i + 1];
    if (!(n > 0 && n <= MAXPAY)) { i += 1; RSYNC[key] = (RSYNC[key] || 0) + 1; continue; }
    if (b.length - i - 2 < n) break;                        // 帧未收全 ⇒ 等下一批
    if (b[i + 2] !== 0x7b || b[i + 2 + n] !== 0x7d) {        // 形态不对 ⇒ 重同步
      i += 1; RSYNC[key] = (RSYNC[key] || 0) + 1; continue;
    }
    out.push(b.slice(i + 2, i + 2 + n));
    i += 2 + n;
  }
  BUF[key] = b.slice(i);
  for (const payload of out) {
    const txt = utf8(payload);
    let name = null;
    try { name = Object.keys(JSON.parse(txt))[0]; } catch (e) { name = '<unparsed>'; }
    emit({ type: 'frame', fd: fd, role: ROLE[fd] || null, peer: PEER[fd] || null,
           dir: dir, name: name, n: payload.length, hex: hexOf(payload), text: txt });
  }
}

function feed(fd, dir, bytes) {
  if (!bytes || !bytes.length) return;
  if (!(fd in ROLE)) return;     // 只收登记过的 TCP 连接（connect 出来的 / accept 出来的）
  const key = fd + '|' + dir;
  BUF[key] = (BUF[key] || []).concat(bytes);
  decode(fd, dir);
  // 保险丝：某条流万一不成帧（长度前缀永远凑不齐），别让缓冲无界长下去把脚本线程拖死
  if (BUF[key] && BUF[key].length > (2 << 20)) {
    emit({ type: 'bufdrop', fd: fd, dir: dir, role: ROLE[fd] || null, len: BUF[key].length,
           rsync: RSYNC[key] || 0 });
    BUF[key] = [];
  }
}

function readBytes(ptr, len) {
  // 单次 syscall 可以把**多条**帧一起读回来，所以上限远大于 MAXPAY（1500 条事件的一批约 128 KB）
  if (!(len > 0) || len > (1 << 21)) return null;
  try { return Array.from(new Uint8Array(ptr.readByteArray(len))); } catch (e) { return null; }
}

function hook(name, cb) {
  try {
    const addr = LIB.getExportByName(name);
    if (!addr) return emit({ type: 'hookfail', name: name, why: 'no-such-export' });
    Interceptor.attach(addr, cb);
  } catch (e) { emit({ type: 'hookfail', name: name, err: String(e) }); }
}

hook('socket', {
  onEnter(args) { this.fam = args[0].toInt32(); this.ty = args[1].toInt32(); },
  onLeave(ret) {
    const fd = ret.toInt32();
    if (fd < 0) return;
    const base = this.ty & 0xf;
    KIND[fd] = base === 1 ? 'tcp' : (base === 2 ? 'udp' : 'other');
    emit({ type: 'sock', fd: fd, fam: this.fam, ty: this.ty, kind: KIND[fd] });
  },
});

hook('bind', {
  onEnter(args) {
    emit({ type: 'bind', fd: args[0].toInt32(), kind: KIND[args[0].toInt32()] || null,
           addr: sockaddr(args[1], args[2]) });
  },
});

hook('listen', {
  onEnter(args) { emit({ type: 'listen', fd: args[0].toInt32(), backlog: args[1].toInt32() }); },
});

for (const nm of ['accept', 'accept4']) {
  hook(nm, {
    onEnter(args) { this.lfd = args[0].toInt32(); this.p = args[1]; },
    onLeave(ret) {
      const fd = ret.toInt32();
      if (fd < 0) return;
      const peer = sockaddr(this.p, null);
      ROLE[fd] = 'server'; PEER[fd] = peer;
      emit({ type: 'accept', lfd: this.lfd, fd: fd, peer: peer });
    },
  });
}

hook('connect', {
  onEnter(args) { this.fd = args[0].toInt32(); this.addr = sockaddr(args[1], args[2]); },
  onLeave(ret) {
    // 异步 connect 返回 EINPROGRESS(-1)，那条连接照样要读 ⇒ 一律登记为 client 侧
    if (KIND[this.fd] === 'tcp') { ROLE[this.fd] = 'client'; PEER[this.fd] = this.addr; }
    emit({ type: 'connect', fd: this.fd, addr: this.addr, ret: ret.toInt32() });
  },
});

// ── 写方向 ────────────────────────────────────────────────────────────────
// ⚠️⚠️ **只挂 `send`，绝对不许挂 `write` / `writev`**（实测踩过，两次）：
//   Frida 自己的消息通道（JS 里的 `send()`）在**目标进程**里也是走 libc 的 `write` 落盘/落管道，
//   于是 `emit() → send() → write() → 我们的 onEnter → feed() → emit() → …` 无限递归自噬：
//   ① F1 跑只出 2 帧就再没有任何消息（连每秒一次的 LAN 广播 `udp_out` 都停了）；
//   ② F2 跑整整 160 s **一条消息都没有**（`frame` / `udp_out` 全 0），脚本线程已经死在里面。
//   判据：`{"type":"ready"}` 到了、开头几十条 `sock/connect/bind` 也到了 ⇒ 脚本加载没问题，
//   是**运行期**被递归拖死的。§51 的 `frida_gate5_wire.js` 也只挂 `send`（0 命中）没踩过。
//
// 代价与本任务的匹配：写方向在本二进制上**本来就 0 命中**（§51 跑 5 局、本轮 A1/B1/C1 全 0），
// 所以丢掉它不损失证据 —— 房主局里 `client→server` 的帧由**服务端 fd 的 `recv`** 拿到
// （两端同进程，见文件头 `role` 说明）；远端官服房间里只拿 `server→client`，
// 而本任务要的 8 个事件里 6 个是服务端广播（见 docs/06 §新增节的方向列）。
hook('send', {
  onEnter(args) { feed(args[0].toInt32(), 'out', readBytes(args[1], args[2].toInt32())); },
});

// ── 读方向 ────────────────────────────────────────────────────────────────
// ⚠️ **`recv` 与 `read` 必须都挂，且都喂同一个缓冲** —— 这是本任务排查出来的现场事实
//   （`tools/live_capture/diag_syscalls.js` 实测，见 docs/06 §103.2 的表）：
//   * **长度前缀**走 `recv`：样本每次恰好 2 字节 `00 ff`（= 255，正是 GameInfo 帧的负载长）；
//   * **正文**走 `read`：`recv` 那一路永远只拿到 2 字节。
//   ⇒ 只挂 `recv` 时缓冲永远停在"收到 2 字节前缀、等正文"，**一帧都出不来**（F3 跑实测 0 帧）；
//     只挂 `read` 则拿不到前缀。两者喂同一 (fd,'in') 缓冲既不重复也不丢（每条字节只被读一次）。
//   * 远端 TLS 那条路（`198.18.0.38:443`，取 `graphwar_II_ip.txt`）用 `recv`+`read` 混合，
//     但它是 HTTPS 不是本协议，样本以 `\x16\x03\x03` 开头，成帧失败会被重同步吃掉，不影响 TCP 流。
hook('recv', {
  onEnter(args) { this.fd = args[0].toInt32(); this.p = args[1]; this.flags = args[3].toInt32(); },
  onLeave(ret) { if (!(this.flags & 0x2)) feed(this.fd, 'in', readBytes(this.p, ret.toInt32())); },
});

hook('read', {
  onEnter(args) { this.fd = args[0].toInt32(); this.p = args[1]; },
  onLeave(ret) { feed(this.fd, 'in', readBytes(this.p, ret.toInt32())); },
});

// ── UDP（LAN 发现：`[ff02::1]:42968`，只留原文，不进本任务的三张表）────────
// 注意：TCP fd 上的 `recvfrom` 也要按"读方向"喂帧（同一 fd 可能换 syscall）；
// 只有**不是**已登记 TCP 连接时，才当 UDP 数据报处理。
hook('recvfrom', {
  onEnter(args) {
    this.fd = args[0].toInt32(); this.p = args[1]; this.sp = args[4]; this.al = args[5];
    this.peek = args[3].toInt32() & 0x2;
  },
  onLeave(ret) {
    if (this.peek) return;
    const bytes = readBytes(this.p, ret.toInt32());
    if (!bytes) return;
    if (this.fd in ROLE) { feed(this.fd, 'in', bytes); return; }
    if (bytes.length > 4096) return;
    emit({ type: 'udp_in', fd: this.fd, src: sockaddr(this.sp, this.al),
           n: bytes.length, text: utf8(bytes) });
  },
});

// UDP 发送（LAN 发现广播）：房主每秒把 9 字段 GameInfo 发到 `[ff02::1]:42968`，
// `address` 字段就是本机房间地址 —— 两实例做 Ban/ClearBans 局时靠它拿 `Join IP` 的目标。
hook('sendto', {
  onEnter(args) {
    const fd = args[0].toInt32();
    if (fd in ROLE) return;
    const bytes = readBytes(args[1], args[2].toInt32());
    if (!bytes || bytes.length > 4096) return;
    emit({ type: 'udp_out', fd: fd, dst: sockaddr(args[4], args[5]),
           n: bytes.length, text: utf8(bytes) });
  },
});

emit({ type: 'ready', n_hooks: 15 });
