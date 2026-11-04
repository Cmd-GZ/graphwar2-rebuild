/**
 * 抓真实对局里 calculate_path 的入参与输出轨迹，供与复刻对拍。
 *
 * ABI（见 analysis/gwemu.py call_path）：
 *   rdi = out Vec (cap,ptr,len 各 8 字节)
 *   rsi = tokens Vec
 *   rdx = flagA(flip), rcx = flagB(ode)
 *   xmm0 = x0, xmm1 = y0, xmm2 = step
 *   Token = { u64 tag, f64 value } 共 16 字节
 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const CAPTURE = 2;

const SRC = `
function dbl(buf) { return new DataView(buf).getFloat64(0, true); }
function readVec(p) {
  const cap = p.readU64().toNumber();
  const ptr = p.add(8).readPointer();
  const len = p.add(16).readU64().toNumber();
  return { cap, ptr, len };
}
let installed = false;
const results = [];
setInterval(() => {
  if (installed) return;
  const gw = Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
  if (!gw) return;
  installed = true;
  Interceptor.attach(gw.base.add(0x3c2530), {
    onEnter(args) {
      this.saved = {
        out: args[0], tok: args[1],
        a: args[2].toInt32(), b: args[3].toInt32(),
        x0: dbl(this.context.xmm0), y0: dbl(this.context.xmm1), step: dbl(this.context.xmm2),
      };
    },
    onLeave() {
      if (results.length >= ${CAPTURE}) return;
      const s = this.saved;
      try {
        const tv = readVec(s.tok);
        const toks = [];
        for (let i = 0; i < tv.len && i < 20000; i++) {
          const base = tv.ptr.add(i * 16);
          toks.push([base.readU64().toNumber(), base.add(8).readDouble()]);
        }
        const ov = readVec(s.out);
        const pts = [];
        for (let i = 0; i < ov.len && i < 20000; i++) {
          const base = ov.ptr.add(i * 16);
          pts.push([base.readDouble(), base.add(8).readDouble()]);
        }
        results.push({ x0: s.x0, y0: s.y0, step: s.step, a: s.a, b: s.b, toks: toks, pts: pts });
        send({ done: results.length });
      } catch (e) { send({ error: e.message }); }
    }
  });
}, 5);
setInterval(() => { if (results.length) send({ results: results }); }, 5000);
`;

(async () => {
  const dev = await frida.getLocalDevice();
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: ROOT + '/.godot-data', XDG_CACHE_HOME: ROOT + '/.godot-cache',
    XDG_CONFIG_HOME: ROOT + '/.godot-config',
  });
  const pid = await dev.spawn([ROOT + '/tools/Godot_v4.7.2-stable_linux.x86_64', '--headless', '--path', ROOT + '/godot-live'], { env, cwd: ROOT });
  const session = await dev.attach(pid);
  const script = await session.createScript(SRC);
  let last = null;
  script.message.connect((m) => { if (m.payload && m.payload.results) last = m.payload.results; });
  await script.load();
  await dev.resume(pid);
  await sleep(60000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.error('ERR no capture'); process.exit(1); }
  const out = path.join(ROOT, '.rea-work/captured_path.json');
  fs.writeFileSync(out, JSON.stringify(last));
  console.log('CAPTURED', last.length, 'calls ->', out,
              'points:', last.map(c => c.pts.length).join(','));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
