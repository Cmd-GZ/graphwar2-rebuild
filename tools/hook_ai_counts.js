/** 统计 AI 相关函数的调用次数：入口 0x51b440、评分 0x51a750、采样 0x3c2530。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
const targets = { ai_entry: 0x51b440, score: 0x51a750, path: 0x3c2530, evolve: 0x51b000 };
const counts = {}; const marks = [];
let ready = false;
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  for (const [name, off] of Object.entries(targets)) {
    counts[name] = 0;
    try {
      Interceptor.attach(m.base.add(ptr(off)), { onEnter() {
        counts[name]++;
        if (name === 'ai_entry' && marks.length < 40) marks.push({ t: Date.now(), counts: JSON.parse(JSON.stringify(counts)) });
      } });
    } catch (e) { counts[name] = -1; }
  }
}, 300);
setInterval(() => send({ counts, marks }), 3000);
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
  await script.load(); await dev.resume(pid); await sleep(70000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/ai_counts.json'), JSON.stringify(last));
  console.log('总计:', JSON.stringify(last.counts));
  console.log('AI 入口次数:', last.marks.length);
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
