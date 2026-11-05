/** 统计 next_u64 的返回地址（= 调用方），找出 AI 从哪里抽随机数。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const sites = {}; let total = 0;
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x4625b0')), {
      onEnter() {
        total++;
        if (total > 20000) return;
        try {
          const ra = this.returnAddress;
          const off = ra.sub(m.base).toString();
          sites[off] = (sites[off] || 0) + 1;
        } catch (e) {}
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ sites, total, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.sites) last = m.payload; });
  await script.load(); await dev.resume(pid);
  const dur = parseInt(process.argv[3] || '60', 10) * 1000;
  await sleep(dur);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/rng_callsites.json'), JSON.stringify(last.sites));
  const rows = Object.entries(last.sites).sort((a, b) => b[1] - a[1]);
  console.log('next_u64 总调用:', last.total, ' 不同调用点:', rows.length);
  rows.slice(0, 12).forEach(([k, v]) => console.log(`  ${String(v).padStart(6)}  ${k}`));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
