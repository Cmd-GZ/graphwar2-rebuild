/** 在发出 AddObstacle 的瞬间读栈内存，找 ChaCha σ 常量与疑似密钥。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
let done = false;
const out = [];
Interceptor.attach(libc.getExportByName('send'), {
  onEnter(args) {
    if (done) return;
    try {
      const len = args[2].toUInt32();
      if (len < 20 || len > 8192) return;
      const head = new Uint8Array(args[1].readByteArray(Math.min(len, 256)));
      const needle = [65,100,100,79,98,115,116,97,99,108,101]; // AddObstacle
      let found = false;
      for (let i = 0; i + needle.length <= head.length; i++) {
        let ok = true;
        for (let j = 0; j < needle.length; j++) if (head[i+j] !== needle[j]) { ok = false; break; }
        if (ok) { found = true; break; }
      }
      if (!found) return;
      done = true;
      const sp = this.context.rsp;
      let u8 = null;
      for (const sz of [8192, 4096, 2048, 1024]) {
        try { u8 = new Uint8Array(sp.readByteArray(sz)); break; } catch (e) {}
      }
      if (!u8) { out.push({ sp: sp.toString(), err: 'unreadable stack' }); return; }
      // 找 "expand 32-byte k"
      const pat = [0x65,0x78,0x70,0x61,0x6e,0x64,0x20,0x33,0x32,0x2d,0x62,0x79,0x74,0x65,0x20,0x6b];
      const hits = [];
      for (let i = 0; i + 64 <= u8.length; i++) {
        let ok = true;
        for (let j = 0; j < 16; j++) if (u8[i+j] !== pat[j]) { ok = false; break; }
        if (!ok) continue;
        const words = [];
        for (let w = 0; w < 16; w++) {
          const o = i + w*4;
          words.push((u8[o] | (u8[o+1]<<8) | (u8[o+2]<<16) | (u8[o+3]<<24)) >>> 0);
        }
        let deref = null;
        try {
          const p0 = (u8[i+16] | (u8[i+17]<<8) | (u8[i+18]<<16) | (u8[i+19]<<24)) >>> 0;
          const p1 = (u8[i+20] | (u8[i+21]<<8) | (u8[i+22]<<16) | (u8[i+23]<<24)) >>> 0;
          const candidate = ptr('0x' + p1.toString(16) + p0.toString(16).padStart(8,'0'));
          const b = new Uint8Array(candidate.readByteArray(48));
          let hex = ''; for (let k = 0; k < b.length; k++) hex += b[k].toString(16).padStart(2,'0');
          deref = hex;
        } catch (e) { deref = 'unreadable'; }
        hits.push({ off: i, words: words.map(w => w.toString(16).padStart(8,'0')), deref });
      }
      out.push({ sp: sp.toString(), hits });
    } catch (e) { out.push({ err: String(e) }); }
  }
});
setInterval(() => send({ out }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.out) last = m.payload.out; });
  await script.load();
  await dev.resume(pid);
  await sleep(45000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last || !last.length) { console.log('NO_HIT'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/stack_at_obstacle.json'), JSON.stringify(last));
  const r = last[0];
  console.log('sp=', r.sp, ' err=', r.err || '');
  console.log('栈内 σ 常量命中:', r.hits ? r.hits.length : 0);
  (r.hits || []).slice(0, 4).forEach(h => console.log('  +0x' + h.off.toString(16), h.words.join(' ')));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
