/** 抓 getrandom 的返回值（前 N 次）与总次数，判断能否作为可重放种子。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
let total = 0;
const samples = [];
let p = null;
try { p = libc.getExportByName('getrandom'); } catch (e) {}
if (p) {
  Interceptor.attach(p, {
    onEnter(args) { this.buf = args[0]; this.len = args[1].toInt32(); },
    onLeave(ret) {
      const n = ret.toInt32();
      total++;
      if (n > 0 && samples.length < 6 && n <= 64) {
        try {
          const b = this.buf.readByteArray(n);
          const u8 = new Uint8Array(b);
          let hex = ''; for (let i = 0; i < u8.length; i++) hex += u8[i].toString(16).padStart(2, '0');
          samples.push({ len: n, hex });
        } catch (e) {}
      }
    }
  });
  send({ hooked: true });
} else { send({ hooked: false }); }
setInterval(() => send({ total, samples }), 4000);
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
  script.message.connect((m) => { if (m.payload && m.payload.total !== undefined) last = m.payload; });
  await script.load();
  await dev.resume(pid);
  await sleep(40000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  fs.writeFileSync(path.join(ROOT, '.rea-work/getrandom.json'), JSON.stringify(last));
  console.log(JSON.stringify(last));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
