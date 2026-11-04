#!/usr/bin/env node
/**
 * 活体 oracle：用 Frida 拉起真实游戏，在运行中的进程里调用 libgraphwarrust.so 的内部函数。
 *
 * 为什么需要它：静态反编译只能读代码，unicorn 只能模拟纯函数。
 * 真游戏进程里调函数，才是"真值"的最强形式。
 *
 * 用法：
 *   node tools/frida_live.js modules
 *   node tools/frida_live.js call <rva-hex> <ret-type> [arg-type:value,...]
 *
 * 地址：RVA = 模块内偏移 = ELF 虚地址（.text 的文件偏移与虚地址差 0x1000，
 * 但这里用的是虚地址，直接 base + rva）。
 *
 * 例（取整函数，Ghidra 0xa302b0 -> 虚地址 0x9302b0）：
 *   node tools/frida_live.js call 0x9302b0 double double:2.7
 */
const path = require('path');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// 模块加载完成后要执行的脚本由 command 决定
function buildSource(cmd, params) {
  const boot = `
    const gw = Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
    if (!gw) { send({error: 'libgraphwarrust.so 未加载'}); } else {
      const base = gw.base;
  `;
  const tail = `}\n`;
  if (cmd === 'modules') {
    return boot + `
      send({ modules: Process.enumerateModules().map(m => ({name: m.name, base: m.base, size: m.size})),
             gw: { name: gw.name, base: base, size: gw.size } });
    ` + tail;
  }
  if (cmd === 'call') {
    const rva = params[0];
    const retType = params[1] || 'void';
    const argSpecs = (params[2] || '').split(',').filter(Boolean);
    const types = argSpecs.map((a) => a.split(':')[0]);
    const values = argSpecs.map((a) => a.split(':')[1]);
    const literal = (t, v) => {
      if (t === 'double' || t === 'float') return String(v);
      if (t === 'pointer' || t === 'int' || t === 'uint' || t === 'long') return String(v);
      return String(v);
    };
    return boot + `
      const fnPtr = base.add(${Number(rva)});
      const args = [${types.map((t, i) => literal(t, values[i])).join(', ')}];
      const f = new NativeFunction(fnPtr, '${retType}', [${types.map((t) => `'${t}'`).join(', ')}]);
      try {
        const r = f(${types.map(() => 'args').length ? '...args' : ''});
        send({ ok: true, rva: ${Number(rva)}, ptr: fnPtr.toString(), ret: String(r) });
      } catch (e) { send({ ok: false, error: e.message }); }
    ` + tail;
  }
  throw new Error('unknown command: ' + cmd);
}

(async () => {
  const cmd = process.argv[2];
  const params = process.argv.slice(3);
  const dev = await frida.getLocalDevice();
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: ROOT + '/.godot-data',
    XDG_CACHE_HOME: ROOT + '/.godot-cache',
    XDG_CONFIG_HOME: ROOT + '/.godot-config',
  });
  const pid = await dev.spawn([ROOT + '/Graphwar.x86_64', '--headless'], { env, cwd: ROOT });
  const session = await dev.attach(pid);
  await dev.resume(pid);
  await sleep(8000); // 等扩展加载
  const script = await session.createScript(buildSource(cmd, params));
  const done = new Promise((res) => script.message.connect((m) => res(m.payload)));
  await script.load();
  const msg = await Promise.race([done, new Promise((_, rej) => setTimeout(() => rej(new Error('timeout')), 15000))]);
  console.log(JSON.stringify(msg));
  await session.detach();
  await dev.kill(pid);
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
