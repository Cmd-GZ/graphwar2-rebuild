/** 同时抓随机流、士兵创建顺序与回合出场顺序（都带随机流位置）。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
const libc = Process.getModuleByName('libc.so.6');
let ready = false; const draws = []; const events = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x47f700')), {
      onLeave(ret) { if (draws.length < 3000) { try { draws.push(ret.toString()); } catch (e) {} } }
    });
  } catch (e) {}
  try {
    Interceptor.attach(libc.getExportByName('send'), {
      onEnter(args) {
        try {
          const len = args[2].toUInt32();
          if (len < 16 || len > 8192) return;
          const b = new Uint8Array(args[1].readByteArray(Math.min(len, 512)));
          let s = ''; for (let i = 0; i < b.length; i++) s += String.fromCharCode(b[i]);
          const mm = s.match(/\\{"(SoldierInfo|TurnInfo)":\\{[^}]*\\}\\}/g);
          if (!mm) return;
          for (const one of mm) {
            if (events.length < 60) events.push({ at: draws.length, msg: one });
          }
        } catch (e) {}
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ draws, events, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.draws) last = m.payload; });
  await script.load(); await dev.resume(pid); await sleep(50000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/turnorder_rng.json'), JSON.stringify({ draws: last.draws, events: last.events }));
  console.log('随机数:', last.draws.length, ' 事件:', last.events.length);
  last.events.slice(0, 12).forEach(e => console.log('  @draw', e.at, e.msg.slice(0, 70)));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
