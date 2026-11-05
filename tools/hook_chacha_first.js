/** hook ChaCha 块函数(0x850f30)，记录前几次调用栈，找播种路径。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const recs = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x750f30')), {
      onEnter(args) {
        if (recs.length >= 5) return;
        try {
          const p = args[0];
          const w = [];
          for (let i = 0; i < 12; i++) w.push(p.add(i * 4).readU32() >>> 0);
          const frames = Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 10).map(a => {
            try { const mm = Process.findModuleByAddress(a);
              return mm ? (/graphwarrust/i.test(mm.name) ? 'GW+' + a.sub(mm.base).toString() : mm.name) : '?';
            } catch (e) { return '?'; }
          });
          recs.push({ key0: w[0] >>> 0, words: w.slice(0,4).map(x=>x.toString(16)), frames });
        } catch (e) { recs.push({ err: String(e) }); }
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ recs, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.recs) last = m.payload.recs; });
  await script.load(); await dev.resume(pid); await sleep(30000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last || !last.length) { console.log('NO_CALL'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/chacha_calls.json'), JSON.stringify(last));
  for (const r of last.slice(0, 3)) {
    console.log('--- key0=', r.key0, r.words ? r.words.join(' ') : r.err);
    (r.frames || []).forEach(f => console.log('    ', f));
  }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
