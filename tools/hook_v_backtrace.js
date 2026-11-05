/** 在 V 的使用点抓调用栈，找出真正的调用者函数。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const frames = {}; let n = 0;
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x51b0f8')), {
      onEnter() {
        if (n >= 300) return;
        n++;
        let bt = [];
        try { bt = Thread.backtrace(this.context, Backtracer.FUZZY); } catch (e) {}
        for (let i = 1; i < Math.min(bt.length, 5); i++) {
          try {
            const off = bt[i].sub(m.base).toString();
            frames[off] = (frames[off] || 0) + 1;
          } catch (e) {}
        }
      }
    });
  } catch (e) { frames['ATTACH_FAIL'] = 1; }
}, 300);
setInterval(() => send({ frames, n, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.frames) last = m.payload; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '40', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  console.log('样本:', last.n, ' 不同回溯帧:', Object.keys(last.frames).length);
  Object.entries(last.frames).sort((a, b) => b[1] - a[1]).slice(0, 10)
    .forEach(([k, v]) => console.log(`  ${String(v).padStart(4)}  ${k}`));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
