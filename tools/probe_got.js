/** 在真进程里读指定 GOT 槽，解析实际目标（哪个模块 + 偏移）。 */
const path = require('path');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const slots = %SLOTS%;
let done = false;
const timer = setInterval(() => {
  if (done) return;
  const gw = Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
  if (!gw) return;
  const out = {};
  for (const s of slots) {
    try {
      const target = gw.base.add(ptr(s)).readPointer();
      const m = Process.findModuleByAddress(target);
      out[s] = m ? (m.name + '+' + target.sub(m.base).toString()) : 'unmapped';
    } catch (e) { out[s] = 'err:' + e; }
  }
  done = true;
  clearInterval(timer);
  send({ got: out, base: gw.base.toString(), size: gw.size });
}, 500);
`;

(async () => {
  const slots = ['0x9baba8', '0x9ba5a0', '0x9ba870', '0x9ba6d0', '0x9ba5b8', '0x9ba798'];
  const dev = await frida.getLocalDevice();
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: ROOT + '/.godot-data', XDG_CACHE_HOME: ROOT + '/.godot-cache',
    XDG_CONFIG_HOME: ROOT + '/.godot-config',
  });
  const pid = await dev.spawn([ROOT + '/tools/Godot_v4.7.2-stable_linux.x86_64', '--headless', '--path', ROOT + '/godot-live'], { env, cwd: ROOT });
  const session = await dev.attach(pid);
  const script = await session.createScript(SRC.replace('%SLOTS%', JSON.stringify(slots)));
  let res = null;
  script.message.connect((m) => { if (m.payload && m.payload.got) res = m.payload.got; });
  await script.load();
  await dev.resume(pid);
  await sleep(30000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  console.log(JSON.stringify(res, null, 1));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
