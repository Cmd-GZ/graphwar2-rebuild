/** 直接统计长度公式的输出（0x4ef0b6 处的 eax），不配对调用。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const hist = {}; let total = 0;
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x4ef0b6')), {
      onEnter() {
        if (total >= 200000) return;
        const n = this.context.rax.toInt32();
        const k = n < 0 ? '<0' : String(n);
        hist[k] = (hist[k] || 0) + 1;
        total++;
      }
    });
  } catch (e) { hist.FAIL = 1; }
}, 300);
setInterval(() => send({ hist, total, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.hist) last = m.payload; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '40', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  const rows = Object.entries(last.hist).filter(([k]) => k !== 'FAIL').sort((a,b)=>Number(a[0])-Number(b[0]));
  console.log('样本:', last.total);
  console.log('抽出长度:', rows.map(([k,v]) => `${k}:${v}`).join(' '));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
