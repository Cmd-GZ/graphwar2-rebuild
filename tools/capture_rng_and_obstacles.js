/** 同一次运行里同时抓 ChaCha 状态与真实障碍坐标，用于验证重放。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
let ready = false;
const states = [];      // 每次取随机数时的 (counter, key)
const obstacles = [];   // 真实发出的 AddObstacle 报文
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return;
  ready = true;
  clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x47f700')), {
      onEnter(args) {
        if (states.length >= 200) return;
        try {
          const p = args[0];
          const w = [];
          for (let i = 0; i < 12; i++) w.push(p.add(i * 4).readU32() >>> 0);
          if (w[0] === 0 && w[1] === 0) return;   // 未初始化
          states.push(w);
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
          const m2 = s.match(/\\{"AddObstacle":\\{[^}]*\\}\\}/);
          if (m2) obstacles.push(m2[0]);
        } catch (e) {}
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ states, obstacles, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.states) last = m.payload; });
  await script.load();
  await dev.resume(pid);
  await sleep(50000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/rng_and_obstacles.json'), JSON.stringify({ states: last.states, obstacles: last.obstacles }));
  console.log('状态样本:', last.states.length, ' 障碍报文:', last.obstacles.length);
  if (last.states.length) {
    const w = last.states[0];
    console.log('首个状态 key:', w.slice(0, 8).map(x => x.toString(16).padStart(8,'0')).join(' '));
    console.log('           ctr:', w.slice(8).map(x => x.toString(16).padStart(8,'0')).join(' '));
  }
  last.obstacles.slice(0, 3).forEach(o => console.log('  ', o));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
