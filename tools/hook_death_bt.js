/** 在发出 LifeQueuedDeath 的瞬间抓调用栈，定位致死判定代码。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const hits = [];
Interceptor.attach(libc.getExportByName('send'), {
  onEnter(args) {
    try {
      const len = args[2].toUInt32();
      if (len < 16 || len > 8192) return;
      const b = new Uint8Array(args[1].readByteArray(Math.min(len, 512)));
      let s = ''; for (let i = 0; i < b.length; i++) s += String.fromCharCode(b[i]);
      if (s.indexOf('LifeQueuedDeath') < 0 || hits.length >= 3) return;
      const frames = Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 10).map(a => {
        try { const m = Process.findModuleByAddress(a);
          return m ? (/graphwarrust/i.test(m.name) ? 'GW+' + a.sub(m.base).toString() : m.name) : '?';
        } catch (e) { return '?'; }
      });
      hits.push({ msg: s.slice(0, 60), frames });
    } catch (e) {}
  }
});
setInterval(() => send({ hits }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.hits) last = m.payload.hits; });
  await script.load(); await dev.resume(pid); await sleep(60000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last || !last.length) { console.log('NO_HIT'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/death_bt.json'), JSON.stringify(last));
  for (const h of last) { console.log('---', h.msg); console.log('   ', h.frames.join('\n    ')); }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
