/**
 * 抓真实对局的原始 socket 字节流（按 fd/方向分块），供事后按帧重组。
 * TCP 会拆包，所以不能按调用边界当帧。
 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const MAXCHUNK = Number(process.argv[2] || 4000);
const SECONDS = Number(process.argv[3] || 60);

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const chunks = [];
function hex(buf) { const u8 = new Uint8Array(buf); let s=''; for (let i=0;i<u8.length;i++) s += u8[i].toString(16).padStart(2,'0'); return s; }
function push(dir, fd, buf) {
  if (chunks.length >= ${MAXCHUNK}) return;
  chunks.push({ dir, fd, hex: hex(buf), len: buf.byteLength });
}
function hookSend(name) {
  try { Interceptor.attach(libc.getExportByName(name), { onEnter(args) {
    const len = args[2].toInt32();
    if (len <= 0 || len > 8192) return;
    const b = args[1].readByteArray(len); if (b) push('out', args[0].toInt32(), b);
  }}); } catch (e) {}
}
function hookRecv(name) {
  try { Interceptor.attach(libc.getExportByName(name), {
    onEnter(args) { this.buf = args[1]; this.fd = args[0].toInt32(); },
    onLeave(ret) { const n = ret.toInt32(); if (n <= 0 || n > 8192) return;
      try { const b = this.buf.readByteArray(n); if (b) push('in', this.fd, b); } catch (e) {} }
  }); } catch (e) {}
}
hookSend('send'); hookSend('sendto');
hookRecv('recv'); hookRecv('recvfrom');
setInterval(() => { if (chunks.length) send({ chunks }); }, 3000);
`;

(async () => {
  const dev = await frida.getLocalDevice();
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: ROOT + '/.godot-data', XDG_CACHE_HOME: ROOT + '/.godot-cache',
    XDG_CONFIG_HOME: ROOT + '/.godot-config',
  });
  const pid = await dev.spawn([ROOT + '/tools/Godot_v4.7.2-stable_linux.x86_64', '--headless', '--path', ROOT + '/godot-live'], { env, cwd: ROOT });
  const session = await dev.attach(pid);
  const script = await session.createScript(SRC);
  let last = null;
  script.message.connect((m) => { if (m.payload && m.payload.chunks) last = m.payload.chunks; });
  await script.load();
  await dev.resume(pid);
  await sleep(SECONDS * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_CHUNKS'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/net_raw.json'), JSON.stringify(last));
  console.log('CHUNKS', last.length);
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
