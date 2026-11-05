/** 统计 libc 随机源调用次数（不含字节内容，避免 hook 自身出错）。 */
const path = require('path');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const counts = {};
function hook(name) {
  counts[name] = 0;
  let p = null;
  try { p = libc.getExportByName(name); } catch (e) { counts[name] = -1; return; }
  try { Interceptor.attach(p, { onEnter() { counts[name]++; } }); }
  catch (e) { counts[name] = -2; }
}
['getrandom','rand','random','srand','getentropy'].forEach(hook);
setInterval(() => send({ counts: JSON.parse(JSON.stringify(counts)) }), 5000);
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
  script.message.connect((m) => { if (m.payload && m.payload.counts) last = m.payload; });
  await script.load();
  await dev.resume(pid);
  await sleep(40000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  console.log(JSON.stringify(last));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
