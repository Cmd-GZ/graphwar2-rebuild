/** hook 取随机 u64 的函数（vaddr 0x47f700），读出 ChaCha 状态（key + counter）。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
let ready = false, recs = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return;
  ready = true;
  clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x47f700')), {
      onEnter(args) {
        if (recs.length >= 12) return;
        try {
          const p = args[0];
          const words = [];
          for (let i = 0; i < 16; i++) words.push(p.add(i * 4).readU32().toString(16).padStart(8, '0'));
          recs.push({ ptr: p.toString(), words });
        } catch (e) { recs.push({ err: String(e) }); }
      }
    });
  } catch (e) { recs.push({ attachErr: String(e) }); }
}, 300);
setInterval(() => send({ recs, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.recs) last = m.payload; });
  await script.load();
  await dev.resume(pid);
  await sleep(45000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/rand_state.json'), JSON.stringify(last.recs));
  console.log('记录数:', last.recs.length);
  last.recs.slice(0, 6).forEach(r => console.log('  ', r.ptr || r.err || r.attachErr, (r.words || []).join(' ')));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
