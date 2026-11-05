/** 统计 token 分派结果的 tag 直方图（用于对比 NormalFunction / DiffEqFunction 模式）。 */
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
    Interceptor.attach(m.base.add(ptr('0x4ef2b0')), {
      onEnter() {
        if (total >= 200000) return;
        try {
          const tag = this.context.r13.toUInt32() & 0xff;
          hist[tag] = (hist[tag] || 0) + 1;
          total++;
        } catch (e) {}
      }
    });
  } catch (e) { hist['FAIL'] = 1; }
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
  await sleep(parseInt(process.argv[3] || '60', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  const names = {0:'x',1:'y',2:"y'",3:'+',4:'-',5:'neg',6:'*',7:'/',8:'^',9:'sqrt',10:'log',11:'ln',12:'exp',13:'abs',14:'sin',15:'cos',16:'tan',17:'const'};
  console.log('总样本:', last.total);
  for (const [tag, n] of Object.entries(last.hist).sort((a,b)=>b[1]-a[1])) {
    console.log(`  ${String(tag).padStart(3)} ${(names[tag]||'?').padEnd(7)} ${String(n).padStart(7)}  ${(100*n/last.total).toFixed(2)}%`);
  }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
