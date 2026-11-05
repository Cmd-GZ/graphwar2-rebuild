/** 试探评估器 FUN_004eb9d0 的参数布局：哪个寄存器指向 token 向量？ */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const samples = []; let n = 0;
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x4eb9d0')), {
      onEnter(args) {
        if (n >= 6) return;
        n++;
        const rec = {};
        for (let i = 0; i < 4; i++) {
          try {
            const p = args[i];
            if (p.compare(ptr('0x1000')) < 0) { rec['a' + i] = 'small:' + p.toUInt32(); continue; }
            const vec = p.readPointer();            // 可能是 Vec {ptr,cap,len}
            const cap = p.add(8).readU64().toNumber();
            const len = p.add(16).readU64().toNumber();
            let first = null;
            try { first = vec.readU64().toNumber(); } catch (e) {}
            rec['a' + i] = { raw: p.toString(), vec: vec.toString(), cap, len, first };
          } catch (e) { rec['a' + i] = 'ERR ' + e.message.slice(0, 40); }
        }
        samples.push(rec);
      }
    });
  } catch (e) { samples.push({ attach: String(e).slice(0, 80) }); }
}, 300);
setInterval(() => send({ samples, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.samples) last = m.payload.samples; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '40', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  console.log(JSON.stringify(last, null, 1).slice(0, 2600));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
