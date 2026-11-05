/** 打印 AI 入口 FUN_0061b440(vaddr 0x51b440) 的实参 —— 直接读参数表。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const calls = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x51b440')), {
      onEnter(args) {
        if (calls.length >= 12) return;
        const rec = {};
        for (let i = 0; i < 6; i++) { try { rec['a' + i] = args[i].toUInt32(); } catch (e) { rec['a' + i] = null; } }
        try { rec.s0 = this.context.rsp.add(8).readU32(); } catch (e) {}
        try { rec.s1 = this.context.rsp.add(16).readU32(); } catch (e) {}
        calls.push(rec);
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ calls, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.calls) last = m.payload.calls; });
  await script.load(); await dev.resume(pid);
  const dur = parseInt(process.argv[3] || '70', 10) * 1000;
  await sleep(dur);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last || !last.length) { console.log('NO_CALL'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/ai_args.json'), JSON.stringify(last));
  console.log('前几次调用的实参:', JSON.stringify(last.slice(0, 3)));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
