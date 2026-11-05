/** 统计"生成一个随机函数"需要抽多少 token：hook 组装函数(0x5ee300)进出 + token 生成器(0x4ef020)。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const lens = {}; let depth = 0, cur = 0, builds = 0;
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  Interceptor.attach(m.base.add(ptr('0x4ee300')), {
    onEnter() { depth++; if (depth === 1) cur = 0; },
    onLeave() { if (depth === 1) { lens[cur] = (lens[cur] || 0) + 1; builds++; } depth--; }
  });
  Interceptor.attach(m.base.add(ptr('0x4ef020')), {
    onEnter() { if (depth > 0) cur++; }
  });
}, 300);
setInterval(() => send({ lens, builds, ready }), 3000);
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
  await sleep(parseInt(process.argv[3] || '50', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/build_lengths.json'), JSON.stringify(last.lens));
  const rows = Object.entries(last.lens).map(([k, v]) => [Number(k), v]).sort((a, b) => a[0] - b[0]);
  const total = rows.reduce((s, [, v]) => s + v, 0);
  console.log('组装次数:', total, ' 不同长度:', rows.length);
  console.log('长度 次数  占比');
  rows.slice(0, 25).forEach(([k, v]) => console.log(`  ${String(k).padStart(3)} ${String(v).padStart(6)}  ${(100*v/total).toFixed(1)}%`));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
