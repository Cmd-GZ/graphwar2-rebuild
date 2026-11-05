/** 验证几个可疑函数是不是关卡生成器：看它们在什么时候被调用、调用几次。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const hits = {}; const t0 = Date.now();
const targets = {
  // 注意：这些是**虚地址**（= Ghidra 地址 - 0x100000）
  scatter: '0x3f1e50',
  disk:    '0x4ea6dc',
  ring:    '0x4ea8a0',
  s3:      '0x4ea9bc',
  s4:      '0x4eab32',
  s5:      '0x4eaca6',
  center:  '0x49f370',
  q564d10: '0x464d10',   // Ghidra 0x564d10
  q565a70: '0x465a70',   // Ghidra 0x565a70
  q56f690: '0x46f690',   // Ghidra 0x56f690
  q578fc0: '0x478fc0',   // Ghidra 0x578fc0
};
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  for (const [name, off] of Object.entries(targets)) {
    try {
      Interceptor.attach(m.base.add(ptr(off)), {
        onEnter() { (hits[name] = hits[name] || []).push(Date.now() - t0); }
      });
    } catch (e) { hits[name] = ['ATTACH_FAIL: ' + e.message]; }
  }
}, 300);
setInterval(() => send({ hits, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.hits) last = m.payload; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '45', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  for (const [name, arr] of Object.entries(last.hits)) {
    if (typeof arr[0] === 'string') { console.log(`  ${name.padEnd(8)} ${arr[0]}`); continue; }
    console.log(`  ${name.padEnd(8)} 调用 ${String(arr.length).padStart(3)} 次  时间点(ms): ${arr.slice(0, 6).join(', ')}`);
  }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
