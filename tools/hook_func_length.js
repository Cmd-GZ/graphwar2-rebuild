/** 统计"一次随机函数生成"包含多少个 token。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const lens = {}; let depth = 0, cur = 0, total = 0; const tagsPerFunc = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x4ef020')), {
      onEnter() { if (depth++ === 0) cur = 0; },
      onLeave() { if (--depth === 0) { lens[cur] = (lens[cur] || 0) + 1; total++; } }
    });
  } catch (e) { lens['attach_entry_fail'] = 1; }
  try {
    Interceptor.attach(m.base.add(ptr('0x4ef2b0')), { onEnter() { cur++; } });
  } catch (e) { lens['attach_disp_fail'] = 1; }
}, 300);
setInterval(() => send({ lens, total, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.lens) last = m.payload; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '45', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  const rows = Object.entries(last.lens).filter(([k]) => /^\d+$/.test(k)).map(([k, v]) => [Number(k), v]).sort((a, b) => a[0] - b[0]);
  const n = rows.reduce((s, [, v]) => s + v, 0);
  console.log('生成次数:', n);
  console.log('token 数  次数   占比');
  rows.slice(0, 20).forEach(([k, v]) => console.log(`  ${String(k).padStart(3)}  ${String(v).padStart(5)}  ${(100*v/n).toFixed(1)}%`));
  const fails = Object.keys(last.lens).filter((k) => !/^\d+$/.test(k));
  if (fails.length) console.log('异常键:', fails.join(','));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
