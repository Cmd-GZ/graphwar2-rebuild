/** 在发出 AddObstacle 报文的那一刻抓调用栈，定位"生成障碍"的代码。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const out = [];
function hookSend(name) {
  let p = null;
  try { p = libc.getExportByName(name); } catch (e) { return; }
  Interceptor.attach(p, {
    onEnter(args) {
      try {
        const len = args[2].toUInt32();
        if (len < 20 || len > 8192) return;
        const s = args[1].readUtf8String(Math.min(len, 200));
        if (!s || s.indexOf('AddObstacle') < 0) return;
        if (out.length >= 3) return;
        const frames = Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 14).map(a => {
          try {
            const m = Process.findModuleByAddress(a);
            if (!m) return '?';
            return /graphwarrust/i.test(m.name) ? 'GW+' + a.sub(m.base).toString() : m.name;
          } catch (e) { return '?'; }
        });
        out.push({ api: name, len, msg: s.slice(0, 120), frames });
      } catch (e) {}
    }
  });
}
['send', 'sendto', 'write'].forEach(hookSend);
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
  fs.writeFileSync(path.join(ROOT, '.rea-work/addobstacle_bt.json'), JSON.stringify(last));
  for (const r of last) {
    console.log('---', r.api, 'len', r.len, r.msg.slice(0, 60));
    console.log('   ', r.frames.join('\n    '));
  }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
