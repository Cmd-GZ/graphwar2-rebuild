/** 记录游戏自身 getrandom 调用与首次 next_u64 的先后顺序。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const events = [];
let ready = false;
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(libc.getExportByName('getrandom'), {
      onEnter(args) { this.len = args[1].toUInt32(); },
      onLeave(ret) {
        const n = ret.toInt32();
        if (n <= 0) return;
        let where = '?';
        try {
          const frames = Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 3);
          const mm = Process.findModuleByAddress(frames[0]);
          where = mm ? (/graphwarrust/i.test(mm.name) ? 'GW+' + frames[0].sub(mm.base).toString() : mm.name) : '?';
        } catch (e) {}
        if (events.length < 400) events.push({ t: 'getrandom', len: this.len, where });
      }
    });
  } catch (e) {}
  try {
    Interceptor.attach(m.base.add(ptr('0x47f700')), {
      onEnter(args) {
        if (events.length < 400) events.push({ t: 'next_u64', ptr: args[0].toString() });
      }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ events, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.events) last = m.payload.events; });
  await script.load(); await dev.resume(pid); await sleep(35000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last || !last.length) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/seed_order.json'), JSON.stringify(last));
  const firstNext = last.findIndex(e => e.t === 'next_u64');
  console.log('事件总数:', last.length, ' 首次 next_u64 在第', firstNext, '个事件');
  console.log('前 12 个事件:');
  last.slice(0, 12).forEach((e, i) => console.log('  ', i, e.t, e.len !== undefined ? ('len=' + e.len) : '', e.where || e.ptr || ''));
  const gwCalls = last.filter(e => e.t === 'getrandom' && /GW\+/.test(e.where));
  console.log('游戏自身的 getrandom 调用:', gwCalls.length, gwCalls.slice(0,5));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
