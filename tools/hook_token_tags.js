/** 在 token 分派的汇合点(0x4ef2b0)读 r13d = tag，统计实际分布，验证 docs/71 的权重表。 */
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
    Interceptor.attach(m.base.add(ptr('0x4ef020')), {
      onEnter() {
        if (total >= 400000) return;
        try {
          const ra = this.returnAddress.sub(m.base).toString();
          hist[ra] = (hist[ra] || 0) + 1;
          total++;
        } catch (e) {}
      }
    });
  } catch (e) {}
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
  const dur = parseInt(process.argv[3] || '70', 10) * 1000;
  await sleep(dur);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/token_tags.json'), JSON.stringify(last.hist));
  console.log('token 生成总次数:', last.total, ' 不同调用方:', Object.keys(last.hist).length);
  for (const [ra, n] of Object.entries(last.hist).sort((a,b)=>b[1]-a[1]).slice(0, 10)) {
    console.log(`  ${String(n).padStart(7)}  ${ra}`);
  }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
