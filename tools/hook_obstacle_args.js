/** hook 关卡生成相关函数，打印参数（含 xmm 里的 double）。 */
const path = require('path');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const gw = Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
const targets = %TARGETS%;   // {name: offset}
const recs = {};
let ready = false;
let ticks = 0;
setInterval(() => { ticks++; }, 1000);
function d(ctx, i) {
  try { return ctx.xmm[i].readDouble(); } catch (e) { return null; }
}
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return;
  ready = true;
  clearInterval(timer);
  for (const [name, off] of Object.entries(targets)) {
    recs[name] = [];
    try {
      Interceptor.attach(m.base.add(ptr(off)), {
        onEnter(args) {
          if (recs[name].length >= 8) return;
          recs[name].push({
            rdi: args[0].toString(), rsi: args[1].toString(), rdx: args[2].toString(),
            xmm0: d(this.context, 0), xmm1: d(this.context, 1), xmm2: d(this.context, 2)
          });
        }
      });
    } catch (e) { recs[name].push({ err: String(e) }); }
  }
}, 300);
setInterval(() => send({ recs, ready, ticks }), 3000);
`;

(async () => {
  const targets = { gen_loop: '0x3f1a50', add_obstacle: '0x45a574' };
  const dev = await frida.getLocalDevice();
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: ROOT + '/.godot-data', XDG_CACHE_HOME: ROOT + '/.godot-cache',
    XDG_CONFIG_HOME: ROOT + '/.godot-config',
  });
  const pid = await dev.spawn([ROOT + '/tools/Godot_v4.7.2-stable_linux.x86_64', '--headless', '--path', ROOT + '/godot-live'], { env, cwd: ROOT });
  const session = await dev.attach(pid);
  const script = await session.createScript(SRC.replace('%TARGETS%', JSON.stringify(targets)));
  let last = null;
  let diag = null;
  script.message.connect((m) => {
    if (m.type === 'error') { console.error('SCRIPT_ERROR', m.description); return; }
    if (m.payload && m.payload.recs) { last = m.payload.recs; diag = m.payload; }
  });
  await script.load();
  console.error('script loaded ok');
  await dev.resume(pid);
  await sleep(45000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA', JSON.stringify(diag)); return; }
  console.log('ready=', diag.ready, 'ticks=', diag.ticks);
  for (const [name, rows] of Object.entries(last)) {
    console.log('===', name, rows.length, '次 ===');
    rows.slice(0, 6).forEach(r => console.log('   ', JSON.stringify(r)));
  }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
