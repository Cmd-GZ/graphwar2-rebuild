// tools/frida_gate5_wire.js —— `dispatch/92`：真房源的**线上原文字** + 门⑤ 五道门（一份脚本，两个视角）。
//
// 目标：libgraphwarrust.so sha256 9a5381…（steam/2026-08-28）。地址一律 **vaddr**（Ghidra = vaddr+0x100000）。
// 只观测，不改游戏内存、不改任何报文。
//
// A. **线上视角**（本文件新增）：挂 libc 的 `socket/bind/listen/accept/connect/send/recv/sendto/recvfrom/write/read`，
//    按 fd 记录 ① TCP 连接的方向字节流（用 `u16` 大端长度**重组成帧**，`docs/09` §1.1）
//    ② UDP 数据报原文（LAN 发现：明文 JSON + 换行分帧，`docs/09` §1.1.1 / §11.5）。
//    ⇒ 不改协议栈也能拿到"服务器先发了什么"，比在客户端内部看事件**更早一层**（事件之前）。
// B. **门⑤ 视角**（与 [`tools/frida_cpm_gates.js`](frida_cpm_gates.js) **同一套钩子**，这里内联一份，
//    好让"线上顺序"和"门⑤ 现场取值"落在**同一份 JSONL、同一条时间轴**上）。
//    ⚠ 若 `frida_cpm_gates.js` 改了，这份内联要跟着改（本轮两者逐行同源，只多了 `seq/t` 前缀）。
//
// 输出消息类型：
//   hooks / sock / bind / listen / accept / connect          —— 套接字生命周期（带 sockaddr 解析）
//   frame   {fd,dir,name,text}                               —— TCP 帧（dir=in/out，name 是 JSON 第一个键）
//   udp_out {fd,dst,text} / udp_in {fd,src,text}             —— UDP 数据报原文
//   player_info / gate5_enter / gate5_found / gate5_miss / send / cpm_add / connected_to_server / …
'use strict';

const BASE = Process.getModuleByName('libgraphwarrust.so').base;
const A = {
  LPM_PLAYER_INFO: 0x519f40,
  LPM_CONNECTED: 0x51a570,
  GATE5_UPGRADE: 0x519fb5,
  GATE5_FOUND: 0x51a0e0,
  GATE5_MISS: 0x51a084,
  SEND_59: 0x51a170, SEND_50: 0x51a247, SEND_52: 0x51a31e, SEND_54: 0x51a3f5, SEND_66: 0x51a451,
  CPM_ADD: 0x478fc0,
  CPM_ERASE: 0x47a520,
  CPM_TICK: 0x4785f0,
  CPM_TICK_RESET: 0x47864f,
  CPM_TURNINFO: 0x479520,
  CPM_TI_STATE_READ: 0x479610,
  CPM_BOTDATA_WRITE: 0x479dfa,
};

const mod = (off) => BASE.add(off);
let seq = 0;
const t0 = Date.now();
const stamp = () => Date.now() - t0;
function emit(obj) { obj.t = stamp(); obj.seq = ++seq; send(obj); }
function hex(p, n) {
  try {
    return Array.from(new Uint8Array(p.readByteArray(n)))
      .map((b) => b.toString(16).padStart(2, '0')).join('');
  } catch (e) { return null; }
}
function rd(fn, dflt) { try { return fn(); } catch (e) { return dflt === undefined ? null : dflt; } }

// ═══════════════════════════════════════════════════════════════════════════
// A. 线上视角
// ═══════════════════════════════════════════════════════════════════════════
const libc = Process.getModuleByName('libc.so.6');
const SOCK = {};            // fd -> 'tcp' | 'udp'
const SERVER_FD = {};       // 监听 fd
const CONN = {};            // 已建立的 TCP 连接 fd（accept 或 connect 成功）
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
    emit({ type: 'frame', fd: fd, dir: dir, name: name, n: f.n, text: txt.slice(0, 1200) });
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
      emit({ type: 'accept', lfd: this.lfd, fd: fd, peer: sockaddrStr(this.p, null) });
    },
  });
}

hook('connect', {
  onEnter(args) { this.fd = args[0].toInt32(); this.addr = sockaddrStr(args[1], args[2]); },
  onLeave(ret) {
    // 异步 connect 会返回 EINPROGRESS(-1)；那条连接照样要读，所以**一律**登记
    if (SOCK[this.fd] === 'tcp') CONN[this.fd] = true;
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

// ═══════════════════════════════════════════════════════════════════════════
// B. 门⑤ 视角（内联自 tools/frida_cpm_gates.js，逐行同源）
// ═══════════════════════════════════════════════════════════════════════════
let lpmSelf = null, cpmSelf = null;
const botKeys = {};
const lastState = {};

function walkTable() {
  if (!cpmSelf) return [];
  const ctrl = rd(() => cpmSelf.add(0x18).readPointer());
  const mask = rd(() => cpmSelf.add(0x20).readU64().toNumber());
  const items = rd(() => cpmSelf.add(0x30).readU64().toNumber());
  if (!ctrl || !items || items > 4096 || mask === null || mask > 4096) return [];
  const out = [];
  for (let i = 0; i <= mask; i++) {
    const c = rd(() => ctrl.add(i).readU8());
    if (c === null || (c & 0x80) !== 0) continue;
    const entry = ctrl.sub((i + 1) * 0x78);
    const key = rd(() => entry.readU32());
    if (key === null) continue;
    out.push({ i: i, key: key, entry: entry });
  }
  return out;
}
function snapshotStates(tag) {
  const rows = walkTable();
  const s = rows.map((r) => r.key + '=' + rd(() => r.entry.add(8).readU32()));
  emit({ type: 'table', src: tag, items: rd(() => cpmSelf.add(0x30).readU64().toNumber()), states: s.join(',') });
  return rows;
}

Interceptor.attach(mod(A.LPM_PLAYER_INFO), {
  onEnter(args) {
    lpmSelf = args[0];
    const self = args[0], ev = args[1];
    const pid = rd(() => ev.add(8).readU32());
    const rows = walkTable();
    const hit = rows.find((r) => r.key === pid);
    emit({
      type: 'player_info',
      tag: rd(() => ev.readU64().toString(16)),
      pid: pid,
      conn: rd(() => ev.add(0xc).readU32()),
      connected: rd(() => self.add(0x18).readU32()),
      my_conn: rd(() => self.add(0x1c).readU32()),
      vec_len: rd(() => self.add(0x10).readU64().toNumber()),
      weak_container: rd(() => self.add(0x30).readPointer().toString()),
      cpm_box: cpmSelf ? cpmSelf.sub(0x18).toString() : null,
      weak_eq_cpm: cpmSelf ? rd(() => self.add(0x30).readPointer().equals(cpmSelf.sub(0x18))) : null,
      pid_in_bot_table: !!hit,
      bot_table_keys: rows.map((r) => r.key).join(','),
      hit_state: hit ? rd(() => hit.entry.add(8).readU32()) : null,
    });
  },
});

Interceptor.attach(mod(A.LPM_CONNECTED), {
  onEnter(args) {
    const self = args[0], ev = args[1];
    emit({ type: 'connected_to_server', self: self.toString(),
           tag: rd(() => ev.readU64().toString(16)),
           ev08: rd(() => ev.add(8).readU32()), ev0c: rd(() => ev.add(0xc).readU32()) });
  },
  onLeave() {
    if (!lpmSelf) return;
    emit({ type: 'connected_after', my_conn: rd(() => lpmSelf.add(0x1c).readU32()),
           connected: rd(() => lpmSelf.add(0x18).readU32()) });
  },
});

Interceptor.attach(mod(A.GATE5_UPGRADE), {
  onEnter(args) {
    emit({ type: 'gate5_enter',
           weak_at: args[0].toString(),
           weak_target: rd(() => args[0].readPointer().toString()),
           cpm_box: cpmSelf ? cpmSelf.sub(0x18).toString() : null,
           match: cpmSelf ? rd(() => args[0].readPointer().equals(cpmSelf.sub(0x18))) : null });
  },
});
Interceptor.attach(mod(0x46c5a0), {
  onEnter(args) { this.weakAt = args[0]; },
  onLeave(ret) {
    emit({ type: 'get_module_wk', weak_at: this.weakAt.toString(),
           weak_target: rd(() => this.weakAt.readPointer().toString()),
           ret: ret.toString(),
           cpm_box: cpmSelf ? cpmSelf.sub(0x18).toString() : null,
           match_cpm: cpmSelf ? rd(() => ret.equals(cpmSelf.sub(0x18))) : null });
  },
});
Interceptor.attach(mod(A.GATE5_FOUND), { onEnter() { emit({ type: 'gate5_found' }); } });
Interceptor.attach(mod(A.GATE5_MISS), { onEnter() { emit({ type: 'gate5_miss' }); } });

function sendHook(off, tagNo, pidOff) {
  Interceptor.attach(mod(off), {
    onEnter(args) {
      const ev = args[1];
      emit({ type: 'send', tag: tagNo, ev: ev.toString(),
             pid: rd(() => ev.add(pidOff).readU32()),
             slen: rd(() => ev.add(0x18).readU64().toNumber()),
             raw: hex(ev, 0x28) });
    },
  });
}
sendHook(A.SEND_59, 59, 0x20);
sendHook(A.SEND_50, 50, 0x20);
sendHook(A.SEND_52, 52, 0x20);
sendHook(A.SEND_54, 54, 0x20);
sendHook(A.SEND_66, 66, 0x10);

Interceptor.attach(mod(A.CPM_ADD), {
  onEnter(args) {
    cpmSelf = args[0];
    this.key = args[1].toUInt32();
    emit({ type: 'cpm_add', self: args[0].toString(), key: this.key,
           level: args[2].toInt32(), s: args[3].toInt32(), n: args[4].toInt32(),
           limit: args[5].toInt32() });
  },
  onLeave() {
    const rows = walkTable();
    const hit = rows.find((r) => r.key === this.key);
    if (hit) {
      botKeys[this.key] = hit.entry;
      emit({ type: 'bot_key', src: 'insert', key: this.key, entry: hit.entry.toString(),
             state: rd(() => hit.entry.add(8).readU32()), raw: hex(hit.entry, 0x78) });
    } else {
      emit({ type: 'bot_key_missing', key: this.key, keys: rows.map((r) => r.key).join(',') });
    }
  },
});

Interceptor.attach(mod(A.CPM_ERASE), {
  onEnter(args) {
    cpmSelf = args[0];
    const ev = args[1];
    const key = rd(() => ev.add(8).readU32());
    emit({ type: 'cpm_erase', key: key, tag: rd(() => ev.readU64().toString(16)) });
    if (key !== null) delete botKeys[key];
  },
  onLeave() { snapshotStates('after_erase'); },
});

let tickCount = 0;
Interceptor.attach(mod(A.CPM_TICK), {
  onEnter(args) {
    cpmSelf = args[0];
    tickCount++;
    if (tickCount % 60 === 1) {
      const rows = walkTable();
      emit({ type: 'cpm_tick', n: tickCount, self: args[0].toString(),
             items: rd(() => args[0].add(0x30).readU64().toNumber()),
             states: rows.map((r) => r.key + '=' + rd(() => r.entry.add(8).readU32())).join(',') });
    }
  },
});

Interceptor.attach(mod(A.CPM_TICK_RESET), {
  onEnter() {
    const p = this.context.r15;
    emit({ type: 'state_write', site: '0x47864f', ptr: p.toString(),
           before: rd(() => p.readU32()), raw: hex(p, 0x30) });
  },
});

Interceptor.attach(mod(A.CPM_TURNINFO), {
  onEnter(args) {
    cpmSelf = args[0];
    this.key = rd(() => args[1].add(8).readU32());
    const rows = walkTable();
    emit({ type: 'turn_info', self: args[0].toString(), key: this.key,
           states: rows.map((r) => r.key + '=' + rd(() => r.entry.add(8).readU32())).join(','),
           items: rd(() => args[0].add(0x30).readU64().toNumber()) });
  },
  onLeave() { emit({ type: 'turn_info_leave', key: this.key }); },
});

Interceptor.attach(mod(A.CPM_TI_STATE_READ), {
  onEnter() {
    const entry = this.context.r14.sub(0x70);
    emit({ type: 'gate_read', site: '0x479610', entry: entry.toString(),
           state: rd(() => entry.readU32()), raw: hex(entry, 0x30) });
  },
});

Interceptor.attach(mod(A.CPM_BOTDATA_WRITE), {
  onEnter() {
    const entry = this.context.r14.sub(0x70);
    emit({ type: 'botdata_write', site: '0x479dfa', entry: entry.toString(),
           new_qword: this.context.rbx.toString(), before: hex(entry, 0x30) });
  },
});

setInterval(() => {
  if (!cpmSelf) return;
  for (const r of walkTable()) {
    const st = rd(() => r.entry.add(8).readU32());
    if (st === null) continue;
    if (!(r.key in lastState)) { lastState[r.key] = st; continue; }
    if (lastState[r.key] !== st) {
      lastState[r.key] = st;
      emit({ type: 'state_change', key: r.key, state: st, raw: hex(r.entry, 0x78), src: 'timer' });
    }
  }
}, 250);

emit({ type: 'hooks', base: BASE.toString(),
       bytes_519fb1: hex(mod(0x519fb1), 6),
       bytes_478fc0: hex(mod(0x478fc0), 4),
       bytes_47864f: hex(mod(0x47864f), 4),
       bytes_4786ab: hex(mod(0x4786ab), 7),
       n_hooks: Object.keys(A).length });
emit({ type: 'ready' });
