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
    Interceptor.attach(m.base.add(ptr('0x4ef2b0')), {
      onEnter() {
        if (total >= 400000) return;
        try {
          const tag = this.context.r13.toUInt32() & 0xff;
          hist[tag] = (hist[tag] || 0) + 1;
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
  const names = {0:'x',1:'y',2:"y'",3:'+',4:'-',5:'neg',6:'*',7:'/',8:'^',9:'sqrt',10:'log',11:'ln',12:'exp',13:'abs',14:'sin',15:'cos',16:'tan',17:'const'};
  const expect = {17:11,0:6,1:4,3:10,4:5,6:11,7:7,8:2,9:1,10:1,11:1,12:6,13:7,14:7,15:5,16:1};
  console.log('总样本:', last.total);
  console.log('tag  名称      实测     实测%    期望%');
  for (const [tag, n] of Object.entries(last.hist).sort((a,b)=>b[1]-a[1])) {
    const e = expect[tag];
    const pct = (100*n/last.total).toFixed(2);
    const epct = e ? (100*e/85).toFixed(2) : '-';
    console.log(`${String(tag).padStart(3)}  ${(names[tag]||'?').padEnd(8)} ${String(n).padStart(7)}  ${pct.padStart(6)}  ${String(epct).padStart(6)}`);
  }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
