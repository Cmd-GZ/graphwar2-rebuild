/** hook getrandom 并抓调用栈，按"调用方模块+偏移"聚类。 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const gw = Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
const records = [];
function where(addr) {
  try {
    const m = Process.findModuleByAddress(addr);
    if (!m) return 'unknown';
    return m.name + '+' + addr.sub(m.base).toString();
  } catch (e) { return 'err'; }
}
const p = libc.getExportByName('getrandom');
Interceptor.attach(p, {
  onEnter(args) { this.len = args[1].toInt32(); },
  onLeave(ret) {
    if (records.length >= 40) return;
    const n = ret.toInt32();
    if (n <= 0) return;
    const frames = Thread.backtrace(this.context, Backtracer.ACCURATE).slice(0, 6).map(where);
    records.push({ len: this.len, frames });
  }
});
setInterval(() => send({ records }), 4000);
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
  script.message.connect((m) => { if (m.payload && m.payload.records) last = m.payload.records; });
  await script.load();
  await dev.resume(pid);
  await sleep(35000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_RECORDS'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/rng_backtrace.json'), JSON.stringify(last));
  const cluster = {};
  for (const r of last) {
    const key = r.frames.slice(0, 4).join(' <- ');
    cluster[key] = (cluster[key] || 0) + 1;
  }
  console.log('RECORDS', last.length, 'CLUSTERS', Object.keys(cluster).length);
  Object.entries(cluster).slice(0, 8).forEach(([k, v]) => console.log(v, 'x', k));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
