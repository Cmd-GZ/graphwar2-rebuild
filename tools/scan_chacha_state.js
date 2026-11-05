/** 在生成障碍的那一刻扫内存，找活着的 ChaCha 状态（以 "expand 32-byte k" 开头）。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const gw = () => Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
let done = false;
const report = [];
Interceptor.attach(libc.getExportByName('send'), {
  onEnter(args) {
    if (done) return;
    try {
      const len = args[2].toUInt32();
      if (len < 20 || len > 8192) return;
      const s = args[1].readUtf8String(Math.min(len, 200));
      if (!s || s.indexOf('AddObstacle') < 0) return;
      done = true;
      const pat = '65 78 70 61 6e 64 20 33 32 2d 62 79 74 65 20 6b';
      const ranges = Process.enumerateRanges('rw-');
      const hits = [];
      for (const r of ranges) {
        if (hits.length >= 12) break;
        if (!r.base || !r.size) continue;
        try {
          Memory.scan(r.base, Math.min(r.size, 8 * 1024 * 1024), pat, {
            onMatch(addr) {
              if (hits.length >= 12) return 'stop';
              try {
                const words = [];
                for (let i = 0; i < 16; i++) words.push(addr.add(i * 4).readU32());
                const isConst = words[0] === 0x61707865 && words[1] === 0x3320646e &&
                                words[2] === 0x79622d32 && words[3] === 0x6b206574;
                if (!isConst) return;
                const tail = [];
                for (let i = 4; i < 16; i++) tail.push(words[i].toString(16).padStart(8, '0'));
                hits.push({ where: addr.toString(), words: words.map(w => w.toString(16).padStart(8,'0')) });
              } catch (e) {}
            }
          });
        } catch (e) {}
      }
      report.push({ ranges: ranges.length, hits });
    } catch (e) { report.push({ err: String(e) }); }
  }
});
setInterval(() => send({ report, done }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.report) last = m.payload; });
  await script.load();
  await dev.resume(pid);
  await sleep(50000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last || !last.report.length) { console.log('NO_HIT'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/chacha_state.json'), JSON.stringify(last.report));
  const r = last.report[last.report.length - 1];
  console.log('扫描区间数:', r.ranges, ' 命中:', r.hits ? r.hits.length : 'err', r.err || '');
  (r.hits || []).slice(0, 5).forEach(h => console.log('  ', h.where, h.words.join(' ')));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
