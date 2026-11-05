/** 试探 V(适应度函数第三个入参) 与"当前被评分函数的 token 数"的关系。 */
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
    Interceptor.attach(m.base.add(ptr('0x51b0f8')), {
      onEnter(args) {
        if (n >= 60) return;
        n++;
        const rec = { V: this.context.rdx.toUInt32() };
        try { rec.limit = this.context.rcx.add(0x58).readU32(); } catch (e) { rec.limit = 'ERR'; }
        // 扫描栈：找到"值等于 V 且前面一格是指向合法 token 数组的指针"
        rec.hits = 0; rec.layout = null;
        const sp = this.context.rsp;
        for (let off = 0; off < 0x600; off += 8) {
          try {
            // 布局 A: [ptr][cap][len]   len 在 off+16
            if (sp.add(off + 16).readU64().toNumber() === rec.V) {
              const base = sp.add(off).readPointer();
              if (base.compare(ptr('0x1000')) > 0) {
                let ok = true;
                for (let k = 0; k < Math.min(rec.V, 6); k++) {
                  if (base.add(k * 16).readU64().toNumber() > 19) { ok = false; break; }
                }
                if (ok) { rec.hits++; if (!rec.layout) rec.layout = 'A@' + off; }
              }
            }
            // 布局 B: [ptr][len]
            if (sp.add(off + 8).readU64().toNumber() === rec.V) {
              const base = sp.add(off).readPointer();
              if (base.compare(ptr('0x1000')) > 0) {
                let ok = true;
                for (let k = 0; k < Math.min(rec.V, 6); k++) {
                  if (base.add(k * 16).readU64().toNumber() > 19) { ok = false; break; }
                }
                if (ok) {
                  rec.hits++;
                  if (!rec.layout) {
                    rec.layout = 'B@' + off;
                    const tg = [];
                    for (let k = 0; k < Math.min(rec.V, 10); k++) tg.push(base.add(k * 16).readU64().toNumber());
                    rec.tags = tg;
                  }
                }
              }
            }
          } catch (e) {}
        }
        samples.push(rec);
      }
    });
  } catch (e) { samples.push({ attach: String(e).slice(0, 60) }); }
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
  await sleep(parseInt(process.argv[3] || '45', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  console.log('样本数:', last.length);
  console.log(JSON.stringify(last.slice(0, 12), null, 0).slice(0, 1800));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
