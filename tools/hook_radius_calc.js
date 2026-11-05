/** hook 半径计算点(0x3f1f96 的 maxsd 前)，读 xmm0；同时抓真实 AddObstacle。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
const libc = Process.getModuleByName('libc.so.6');
let ready = false; const radii = []; const obstacles = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x3f1f96')), {
      onEnter() {
        if (radii.length >= 80) return;
        try { radii.push(this.context.xmm0.readDouble()); } catch (e) { radii.push(null); }
      }
    });
  } catch (e) {}
  try {
    Interceptor.attach(libc.getExportByName('send'), {
      onEnter(args) {
        try {
          const len = args[2].toUInt32();
          if (len < 20 || len > 8192) return;
          const b = new Uint8Array(args[1].readByteArray(Math.min(len, 256)));
          const needle = [65,100,100,79,98,115,116,97,99,108,101];
          let hit = false;
          for (let i = 0; i + needle.length <= b.length; i++) {
            let ok = true;
            for (let j = 0; j < needle.length; j++) if (b[i+j] !== needle[j]) { ok = false; break; }
            if (ok) { hit = true; break; }
          }
          if (!hit || obstacles.length >= 60) return;
          let s = ''; for (let i = 0; i < b.length; i++) s += String.fromCharCode(b[i]);
          const mm = s.match(/\\{"AddObstacle":\\{[^}]*\\}\\}/);
          if (mm) obstacles.push(mm[0]);
        } catch (e) {}
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ radii, obstacles, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.radii) last = m.payload; });
  await script.load(); await dev.resume(pid); await sleep(45000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/radius_calc.json'), JSON.stringify(last));
  console.log('半径计算点命中:', last.radii.length, ' 真实障碍:', last.obstacles.length);
  console.log('算出的半径前 8:', last.radii.slice(0, 8).map(v => v === null ? 'null' : v.toFixed(4)).join(' '));
  console.log('真实半径前 8:', last.obstacles.slice(0, 8).map(o => JSON.parse(o).AddObstacle.radius.toFixed(4)).join(' '));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
