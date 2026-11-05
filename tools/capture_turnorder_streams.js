/** 按 RNG 状态指针分流记录，并记录 SoldierInfo/TurnInfo 的全局调用序号。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
const libc = Process.getModuleByName('libc.so.6');
let ready = false; let n = 0; const calls = []; const events = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x47f700')), {
      onEnter(args) { this.p = args[0].toString(); },
      onLeave(ret) {
        if (n >= 4000) return;
        try { calls.push({ i: n++, ptr: this.p, v: ret.toString() }); } catch (e) {}
      }
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
          for (const one of mm) if (events.length < 80) events.push({ at: n, msg: one });
        } catch (e) {}
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ calls, events, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.calls) last = m.payload; });
  await script.load(); await dev.resume(pid); await sleep(50000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  const out = process.argv[2] || 'streams.json';
  fs.writeFileSync(path.join(ROOT, '.rea-work', out), JSON.stringify({ calls: last.calls, events: last.events }));
  const ptrs = {};
  for (const c of last.calls) ptrs[c.ptr] = (ptrs[c.ptr] || 0) + 1;
  console.log('saved', out, '调用数', last.calls.length, '不同状态指针', Object.keys(ptrs).length);
  Object.entries(ptrs).sort((a,b)=>b[1]-a[1]).slice(0,4).forEach(([p,c]) => console.log('  ', p, c));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
