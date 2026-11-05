/** 对比 simplify(0x4ebbe0) 的输入 token 流与输出表达式。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const pairs = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  const readTags = (base, n) => {
    const out = [];
    for (let k = 0; k < n; k++) {
      try { out.push(base.add(k * 16).readU64().toNumber()); } catch (e) { break; }
    }
    return out;
  };
  try {
    Interceptor.attach(m.base.add(ptr('0x4ebbe0')), {
      onEnter(args) {
        this.in = null;
        try {
          const reader = args[1];
          const tokPtr = reader.add(8).readPointer();
          const idx = args[2].readU64().toNumber();
          this.inIdx = idx;
          this.inTags = readTags(tokPtr.add(idx * 16), 14);
          this.inPtr = tokPtr.toString();
        } catch (e) { this.inTags = null; }
      },
      onLeave(ret) {
        if (pairs.length >= 20 || !this.inTags) return;
        let outTags = null;
        try {
          // sret: args[0] 指向返回的 Vec{ptr,cap,len}
          const r = this.context; // 不可靠，改用 onEnter 存的 args[0]
        } catch (e) {}
        pairs.push({ inIdx: this.inIdx, inTags: this.inTags });
      }
    });
  } catch (e) { pairs.push({ fail: String(e).slice(0, 60) }); }
}, 300);
setInterval(() => send({ pairs, ready }), 3000);
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
  script.message.connect((msg) => { if (msg.payload && msg.payload.pairs) last = msg.payload.pairs; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '35', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  console.log('样本:', last.length);
  for (const p of last.slice(0, 6)) console.log('  输入@' + p.inIdx, JSON.stringify(p.inTags));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
