/** 抓种子字节（游戏自身 getrandom）与首次 next_u64 时的 ChaCha key，直接比对。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const seeds = []; let key = null;
let ready = false;
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(libc.getExportByName('getrandom'), {
      onEnter(args) { this.buf = args[0]; this.len = args[1].toUInt32(); },
      onLeave(ret) {
        const n = ret.toInt32();
        if (n <= 0 || seeds.length >= 8) return;
        let where = '?';
        try {
          const f = Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 1);
          const mm = Process.findModuleByAddress(f[0]);
          where = mm ? (/graphwarrust/i.test(mm.name) ? 'GW+' + f[0].sub(mm.base).toString() : mm.name) : '?';
        } catch (e) {}
        try {
          const b = new Uint8Array(this.buf.readByteArray(n));
          let hex = ''; for (let i = 0; i < b.length; i++) hex += b[i].toString(16).padStart(2,'0');
          seeds.push({ len: n, hex, where });
        } catch (e) {}
      }
    });
  } catch (e) {}
  try {
    Interceptor.attach(m.base.add(ptr('0x47f700')), {
      onEnter(args) {
        if (key !== null) return;
        try {
          const p = args[0];
          const w = [];
          for (let i = 0; i < 8; i++) w.push(p.add(i*4).readU32() >>> 0);
          if (w.every(x => x === 0)) return;      // 跳过未初始化的
          key = w;
        } catch (e) {}
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ seeds, key, ready }), 3000);
`;
(async () => {
  const dev = await frida.getLocalDevice();
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: ROOT + '/.godot-data', XDG_CACHE_HOME: ROOT + '/.godot-cache',
    XDG_CONFIG_HOME: ROOT + '/.godot-config' });
  const pid = await dev.spawn([ROOT + '/tools/Godot_v4.7.2-stable_linux.x86_64', '--headless', '--path', ROOT + '/godot-live'], { env, cwd: ROOT });
  const session = await dev.attach(pid);
  const script = await session.createScript(SRC);
  let last = null;
  script.message.connect((m) => { if (m.payload && m.payload.seeds) last = m.payload; });
  await script.load(); await dev.resume(pid); await sleep(30000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/seed_bytes.json'), JSON.stringify(last));
  console.log('种子候选:', last.seeds.map(s => s.hex + '@' + s.where));
  console.log('ChaCha key:', last.key ? last.key.map(w=>w.toString(16).padStart(8,'0')).join(' ') : null);
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
