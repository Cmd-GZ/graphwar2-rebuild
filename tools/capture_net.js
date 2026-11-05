/**
 * 在真实对局进程里 hook libc 的 socket 收发，抓真实网络报文。
 * 用途：还原 LAN 协议（帧格式、消息语义）。
 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const MAXPKT = Number(process.argv[2] || 40);

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const out = [];
function hex(buf) {
  const u8 = new Uint8Array(buf);
  let s = '';
  for (let i = 0; i < u8.length; i++) s += u8[i].toString(16).padStart(2, '0');
  return s;
}
function push(kind, bytes, extra) {
  if (out.length >= ${MAXPKT}) return;
  out.push(Object.assign({ kind, len: bytes.byteLength, hex: hex(bytes) }, extra || {}));
}
function hook(name, onEnter) {
  try {
    const p = libc.getExportByName(name);
    if (!p) return;
    Interceptor.attach(p, { onEnter });
  } catch (e) {}
}
hook('sendto', function (args) {
  const len = args[2].toInt32();
  const buf = args[1].readByteArray(len);
  let port = -1;
  try {
    const sa = args[4];
    if (!sa.isNull()) port = sa.add(2).readU16();
  } catch (e) {}
  if (buf && len > 0 && len < 2048) push('sendto', buf, { fd: args[0].toInt32(), port: port });
});
hook('recvfrom', function (args) {
  this.len = args[2].toInt32();
  this.buf = args[1];
  this.fd = args[0].toInt32();
});
hook('recvfrom', null);
setInterval(() => {
  // recvfrom 需要返回值才知道实际长度，用另一个 hook 方式
}, 1000);
setInterval(() => { if (out.length) send({ packets: out }); }, 3000);
`;

// recvfrom 要用 onLeave 读返回值长度，单独写一套
const SRC2 = `
const libc = Process.getModuleByName('libc.so.6');
const out = [];
function hex(buf) { const u8 = new Uint8Array(buf); let s = ''; for (let i = 0; i < u8.length; i++) s += u8[i].toString(16).padStart(2,'0'); return s; }
function push(kind, bytes, extra) { if (out.length >= ${MAXPKT}) return; out.push(Object.assign({kind, len: bytes.byteLength, hex: hex(bytes)}, extra||{})); }
function hookSend(name, bufArg, lenArg, kindName) {
  try {
    const p = libc.getExportByName(name);
    Interceptor.attach(p, { onEnter(args) {
      const len = args[lenArg].toInt32();
      if (len <= 0 || len > 4096) return;
      const buf = args[bufArg].readByteArray(len);
      if (buf) push(kindName, buf, { fd: args[0].toInt32(), fn: name });
    }});
  } catch (e) {}
}
function hookRecv(name, kindName) {
  try {
    const p = libc.getExportByName(name);
    Interceptor.attach(p, {
      onEnter(args) { this.buf = args[1]; this.fd = args[0].toInt32(); this.cap = args[2].toInt32(); },
      onLeave(ret) {
        const n = ret.toInt32();
        if (n <= 0 || n > 4096) return;
        try { const b = this.buf.readByteArray(n); if (b) push(kindName, b, { fd: this.fd, fn: name }); } catch (e) {}
      }
    });
  } catch (e) {}
}
hookSend('sendto', 1, 2, 'udp_send');
hookRecv('recvfrom', 'udp_recv');
hookSend('send', 1, 2, 'tcp_send');
hookRecv('recv', 'tcp_recv');
hookSend('sendmsg', 1, 2, 'msg_send');
hookRecv('recvmsg', 'msg_recv');
setInterval(() => { if (out.length) send({ packets: out }); }, 3000);
`;

(async () => {
  const dev = await frida.getLocalDevice();
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: ROOT + '/.godot-data', XDG_CACHE_HOME: ROOT + '/.godot-cache',
    XDG_CONFIG_HOME: ROOT + '/.godot-config',
  });
  const pid = await dev.spawn([ROOT + '/tools/Godot_v4.7.2-stable_linux.x86_64', '--headless', '--path', ROOT + '/godot-live'], { env, cwd: ROOT });
  const session = await dev.attach(pid);
  const script = await session.createScript(SRC2);
  let last = null;
  script.message.connect((m) => { if (m.payload && m.payload.packets) last = m.payload.packets; });
  await script.load();
  await dev.resume(pid);
  await sleep(25000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_PACKETS'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/packets.json'), JSON.stringify(last));
  console.log('PACKETS', last.length, 'first:', JSON.stringify(last.slice(0, 3)));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
