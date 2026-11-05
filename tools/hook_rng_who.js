/** 对每次 getrandom 取调用栈，统计栈里出现的游戏函数（是否落在关卡生成/AI 附近）。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const gw = Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
const libc = Process.getModuleByName('libc.so.6');
const stacks = {};
let calls = 0;
const p = libc.getExportByName('getrandom');
Interceptor.attach(p, {
  onEnter(args) { this.len = args[1].toUInt32(); },
  onLeave(ret) {
    try {
      const n = ret.toInt32();
      if (n <= 0) return;
      calls++;
      let frames = [];
      try { frames = Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 8); } catch (e2) {}
      let desc = [];
      for (const a of frames) {
        try {
          const m = Process.findModuleByAddress(a);
          if (!m) { desc.push('?'); continue; }
          desc.push(/graphwarrust/i.test(m.name) ? 'GW+' + a.sub(m.base).toString() : m.name);
        } catch (e3) { desc.push('?'); }
      }
      const key = desc.length ? desc.join(' < ') : 'EMPTY';
      stacks[key] = (stacks[key] || 0) + 1;
    } catch (e) {}
  }
});
setInterval(() => send({ stacks, calls }), 4000);
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
  script.message.connect((m) => { if (m.payload && m.payload.stacks) last = m.payload; });
  await script.load();
  await dev.resume(pid);
  await sleep(35000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/rng_who.json'), JSON.stringify(last.stacks));
  console.log('calls=', last.calls, 'distinct stacks=', Object.keys(last.stacks).length);
  Object.entries(last.stacks).sort((a,b)=>b[1]-a[1]).slice(0,10)
    .forEach(([k,v]) => console.log(String(v).padStart(4), k.slice(0,150)));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
