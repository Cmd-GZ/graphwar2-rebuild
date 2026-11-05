/** 稳态下 AI 到底调用哪些"构造子代"的函数：交叉 / 变异 / 随机生成。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const counts = {}; const series = {}; const t0 = Date.now();
const targets = {
  crossover: '0x4ee3a0',   // 交叉
  mutate:    '0x4ee720',   // 变异
  randfunc:  '0x4ef020',   // 随机新函数
  ctor:      '0x4ee0e0',   // 构造辅助
};
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  for (const [name, off] of Object.entries(targets)) {
    counts[name] = 0; series[name] = [];
    try {
      Interceptor.attach(m.base.add(ptr(off)), {
        onEnter() {
          counts[name]++;
          if (series[name].length < 4000) {
            const t = Date.now() - t0;
            if (!series[name].length || t - series[name][series[name].length-1] > 400) series[name].push(t);
          }
        }
      });
    } catch (e) { counts[name] = 'FAIL'; }
  }
}, 300);
setInterval(() => send({ counts, series, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.counts) last = m.payload; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '60', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/ga_paths.json'), JSON.stringify(last));
  for (const [k, v] of Object.entries(last.counts)) {
    const s = last.series[k] || [];
    console.log(`  ${k.padEnd(10)} ${String(v).padStart(6)} 次   首次出现(ms): ${s.slice(0, 5).join(', ') || '从未'}`);
  }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
