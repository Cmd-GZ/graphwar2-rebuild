// tools/live_capture/diag_syscalls.js —— 带 fd 角色映射的 syscall 采样诊断（T53 排查用）。
//
// 目的：回答「TCP 的字节到底由哪个 libc 函数、在哪个 fd 上、以什么长度交付」。
// 结论直接决定 `live_wire.js` 该挂哪些函数（见该文件"读方向"那段注释）。
//
// 两条铁律（都踩过）：
// ① **绝不在钩子里 `send()`** —— 会自噬（`emit→send→write→hook→emit`），实测两次把脚本线程拖死；
//    所以钩子体内只写内存计数器/样本环，`send()` 全部放到 `setInterval` 里。
// ② `write`/`writev` 要挂**只能计数不能发**，否则同样自噬。
'use strict';

const LIB = Process.getModuleByName('libc.so.6');
const ROLE = {};
const KIND = {};
const CNT = {};
const BYTES = {};
const SAMPLE = {};
const MAXS = 6;
const T0 = Date.now();

function note(fn, fd, bytes) {
  const k = fn + '|' + fd;
  CNT[k] = (CNT[k] || 0) + 1;
  BYTES[k] = (BYTES[k] || 0) + (bytes ? bytes.length : 0);
  if (!bytes || !bytes.length) return;
  const arr = SAMPLE[k] || (SAMPLE[k] = []);
  if (arr.length < MAXS) {
    let h = '';
    const m = Math.min(48, bytes.length);
    for (let i = 0; i < m; i++) h += (bytes[i] < 16 ? '0' : '') + bytes[i].toString(16);
    arr.push({ n: bytes.length, hex: h });
  }
}

function grab(ptr, len) {
  if (!(len > 0) || len > (1 << 21)) return null;
  try { return Array.from(new Uint8Array(ptr.readByteArray(len))); } catch (e) { return null; }
}

function hookIO(name, bufArg, lenArg, isOut) {
  try {
    const p = LIB.getExportByName(name);
    if (!p) return;
    Interceptor.attach(p, {
      onEnter(args) {
        this.fd = args[0].toInt32();
        this.buf = args[bufArg];
        this.len = args[lenArg].toInt32();
        this.flags = (name === 'recv' || name === 'recvfrom') ? args[3].toInt32() : 0;
      },
      onLeave(ret) {
        const n = isOut ? this.len : ret.toInt32();
        if (n > 0 && this.flags & 0x2) return;              // MSG_PEEK：同一批字节会再来一次
        note(name, this.fd, n > 0 ? grab(this.buf, n) : null);
      },
    });
  } catch (e) { CNT['hookfail|' + name] = 1; }
}

hookIO('recv', 1, 2, false);
hookIO('read', 1, 2, false);
hookIO('recvfrom', 1, 2, false);
hookIO('send', 1, 2, true);
hookIO('write', 1, 2, true);
hookIO('writev', 1, 2, true);

function hookSock() {
  try {
    Interceptor.attach(LIB.getExportByName('socket'), {
      onEnter(args) { this.ty = args[1].toInt32(); },
      onLeave(ret) {
        const fd = ret.toInt32();
        if (fd < 0) return;
        const b = this.ty & 0xf;
        KIND[fd] = b === 1 ? 'tcp' : (b === 2 ? 'udp' : 'other');
      },
    });
    Interceptor.attach(LIB.getExportByName('connect'), {
      onEnter(args) { this.fd = args[0].toInt32(); },
      onLeave(ret) { if (KIND[this.fd] === 'tcp') ROLE[this.fd] = 'client'; },
    });
    for (const nm of ['accept', 'accept4']) {
      const p = LIB.getExportByName(nm);
      if (!p) continue;
      Interceptor.attach(p, {
        onLeave(ret) { const fd = ret.toInt32(); if (fd >= 0) ROLE[fd] = 'server'; },
      });
    }
  } catch (e) { CNT['hookfail|sock'] = 1; }
}
hookSock();

setInterval(() => {
  // ⚠️ 只在这里 send（不在系统调用上下文里）
  send({ type: 'counts', t: Date.now() - T0, role: ROLE, kind: KIND,
         cnt: CNT, bytes: BYTES, sample: SAMPLE });
}, 3000);
send({ type: 'ready' });
