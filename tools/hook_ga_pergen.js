/** 按"每代 = 种群+3 次评分"对齐，统计每代各构造路径的调用次数。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false;
const GEN = 253;                       // 难度 5：种群 250 + 3 次精评
const c = { crossover: 0, mutate: 0, randfunc: 0, ctor: 0 };
let score = 0; const gens = []; let prev = { ...c };
const targets = { crossover: '0x4ee3a0', mutate: '0x4ee720', randfunc: '0x4ef020', ctor: '0x4ee0e0' };
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  for (const [name, off] of Object.entries(targets)) {
    try { Interceptor.attach(m.base.add(ptr(off)), { onEnter() { c[name]++; } }); } catch (e) {}
  }
  try {
    Interceptor.attach(m.base.add(ptr('0x51a750')), {
      onEnter() {
        score++;
        if (score % GEN === 0 && gens.length < 8) {
          const d = {};
          for (const k of Object.keys(c)) { d[k] = c[k] - prev[k]; }
          gens.push(d); prev = { ...c };
        }
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ gens, score, c, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.gens) last = m.payload; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '60', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  console.log('评分总次数:', last.score, ' 完整代数:', last.gens.length);
  console.log('每代：crossover  mutate  randfunc  ctor');
  last.gens.forEach((g, i) => console.log(`  #${i+1}: ${String(g.crossover).padStart(4)}  ${String(g.mutate).padStart(4)}  ${String(g.randfunc).padStart(4)}  ${String(g.ctor).padStart(4)}`));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
