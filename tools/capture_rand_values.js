/** 抓每次 next_u64 的返回值与返回后的状态，同时抓真实 AddObstacle 报文。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
let ready = false;
const draws = [];      // {i, ret, key0, ctr0}
const obstacles = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return;
  ready = true;
  clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x47f700')), {
      onEnter(args) {
        this.p = args[0];
        try { this.k0 = this.p.readU32(); this.c0 = this.p.add(32).readU32(); } catch (e) {}
      },
      onLeave(ret) {
        if (draws.length >= 400) return;
        try {
          const hex = ret.toString();
          draws.push({ i: draws.length, v: hex, ctr0: this.c0 >>> 0 });
        } catch (e) {}
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
          let s = '';
          for (let i = 0; i < b.length; i++) s += String.fromCharCode(b[i]);
          const mm = s.match(/\\{"AddObstacle":\\{[^}]*\\}\\}/);
          if (mm) obstacles.push({ at: draws.length, msg: mm[0] });
        } catch (e) {}
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ draws, obstacles, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.draws) last = m.payload; });
  await script.load();
  await dev.resume(pid);
  await sleep(50000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/rand_values.json'), JSON.stringify({ draws: last.draws, obstacles: last.obstacles }));
  console.log('随机值样本:', last.draws.length, ' 障碍报文:', last.obstacles.length);
  console.log('前 6 个返回值:', last.draws.slice(0, 6).map(d => d.v).join(' '));
  last.obstacles.slice(0, 2).forEach(o => console.log('  @draw', o.at, o.msg));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
