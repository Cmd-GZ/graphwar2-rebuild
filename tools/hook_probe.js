/**
 * 在真游戏进程里挂一批 hook，统计启动期间哪些关键函数被调用。
 * 输出每 1 秒统计一次，共 DURATION 秒。
 */
const path = require('path');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// RVA（虚地址）与名字
const TARGETS = {
  calculate_path: 0x3c2530,
  parse_function: 0x4ed670,
  eval:          0x4eb9d0,
  rasterize:     0x3f11e0,
  point_test:    0x3f15b0,
  mask_ctor:     0x523020,
  turn_a:        0x4628d0,
  turn_b:        0x4627c0,
  life_entry:    0x454b40,
  shot_calc:     0x3c2bc0,
  levelgen_a:    0x466c40,
  levelgen_b:    0x466d20,
};

const SRC = `
const targets = ${JSON.stringify(TARGETS)};
const counts = {};
function findModule() {
  return Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
}
let installed = false;
const timer = setInterval(() => {
  if (installed) return;
  const gw = findModule();
  if (!gw) return;
  installed = true;
  for (const [name, rva] of Object.entries(targets)) {
    try {
      Interceptor.attach(gw.base.add(rva), { onEnter() { counts[name] = (counts[name] || 0) + 1; } });
      counts[name] = 0;
    } catch (e) { counts[name] = 'ERR'; }
  }
  send({ installed: true, base: gw.base.toString() });
}, 5);
setInterval(() => { if (installed) send({ counts: Object.assign({}, counts) }); }, 1000);
`;

(async () => {
  const dev = await frida.getLocalDevice();
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: ROOT + '/.godot-data', XDG_CACHE_HOME: ROOT + '/.godot-cache',
    XDG_CONFIG_HOME: ROOT + '/.godot-config',
  });
  const pid = await dev.spawn([ROOT + '/Graphwar.x86_64', '--headless'], { env, cwd: ROOT });
  const session = await dev.attach(pid);
  const script = await session.createScript(SRC);
  let last = null;
  script.message.connect((m) => { last = m.payload; });
  await script.load();      // 进程还挂起时装好 hook
  await dev.resume(pid);    // 然后再放行，确保能抓到初始化调用
  await sleep(8000);
  console.log(JSON.stringify(last));
  await session.detach();
  await dev.kill(pid);
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
