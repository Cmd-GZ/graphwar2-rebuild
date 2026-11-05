/** hook libc syscall()，只关心 SYS_getrandom(318)，抓返回的字节。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const p = libc.getExportByName('syscall');
const recs = [];
let total = 0;
Interceptor.attach(p, {
  onEnter(args) {
    this.isGr = (args[0].toUInt32() === 318);
    if (this.isGr) { this.buf = args[1]; this.len = args[2].toUInt32(); }
  },
  onLeave(ret) {
    if (!this.isGr) return;
    total++;
    try {
      const n = ret.toInt32();
      if (n > 0 && recs.length < 40) {
        const u8 = new Uint8Array(this.buf.readByteArray(n));
        let hex = ''; for (let i = 0; i < u8.length; i++) hex += u8[i].toString(16).padStart(2, '0');
        const frames = Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 5).map(a => {
          try { const m = Process.findModuleByAddress(a); return m ? (/graphwarrust/i.test(m.name) ? 'GW+' + a.sub(m.base).toString() : m.name) : '?'; } catch (e) { return '?'; }
        });
        recs.push({ req: this.len, got: n, hex, frames });
      }
    } catch (e) {}
  }
});
setInterval(() => send({ total, recs, alive: true }), 2000);
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
  script.message.connect((m) => { if (m.payload && m.payload.recs) { last = m.payload; } });
  await script.load();
  await dev.resume(pid);
  await sleep(15000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/syscall_rng.json'), JSON.stringify(last));
  console.log('SYS_getrandom 总数 =', last.total, ' 记录 =', last.recs.length);
  last.recs.slice(0, 6).forEach(r => console.log(' req', r.req, 'got', r.got, r.hex.slice(0, 64), '|', r.frames.slice(0,3).join(' < ')));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
