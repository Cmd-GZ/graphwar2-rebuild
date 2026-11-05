/** 统计 getrandom 的调用点（返回地址）与请求长度，找出游戏自己的 RNG 播种。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const gw = Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
const libc = Process.getModuleByName('libc.so.6');
const sites = {};
const p = libc.getExportByName('getrandom');
let calls = 0, errors = 0, lastErr = '';
const mods = Process.enumerateModules().map(m => ({ name: m.name, base: m.base, end: m.base.add(m.size) }));
Interceptor.attach(p, {
  onEnter(args) { this.len = args[1].toUInt32(); this.ret = this.returnAddress; },
  onLeave(ret) {
    try {
      calls++;
      const n = ret.toInt32();
      if (n <= 0) return;
      let where = 'other';
      if (this.ret) {
        for (const m of mods) {
          if (this.ret.compare(m.base) >= 0 && this.ret.compare(m.end) < 0) {
            where = m.name + '+' + this.ret.sub(m.base).toString();
            break;
          }
        }
      }
      const key = where + ' len=' + this.len;
      sites[key] = (sites[key] || 0) + 1;
    } catch (e) { errors++; lastErr = String(e); }
  }
});
setInterval(() => send({ sites, calls, errors, lastErr }), 4000);
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
  script.message.connect((m) => { if (m.payload && m.payload.sites) { last = m.payload.sites; global.__diag = m.payload; } });
  await script.load();
  await dev.resume(pid);
  await sleep(35000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  console.log('calls=', global.__diag.calls, 'errors=', global.__diag.errors, 'lastErr=', global.__diag.lastErr);
  fs.writeFileSync(path.join(ROOT, '.rea-work/rng_sites.json'), JSON.stringify(last));
  const rows = Object.entries(last).sort((a, b) => b[1] - a[1]);
  console.log('不同调用点/长度组合:', rows.length);
  rows.slice(0, 12).forEach(([k, v]) => console.log(String(v).padStart(5), k));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
