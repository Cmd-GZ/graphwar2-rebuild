/** 对比"抽到的长度"(0x4ef0b6 处的 eax) 与"实际吐出的 token 数"(分派点计数)。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const pairs = {}; let depth = 0, emitted = 0, drawn = null, total = 0;
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x4ef020')), {
      onEnter() { if (depth++ === 0) { emitted = 0; drawn = null; } },
      onLeave() {
        if (--depth === 0 && drawn !== null) {
          const key = drawn + '->' + emitted;
          pairs[key] = (pairs[key] || 0) + 1;
          total++;
        }
      }
    });
  } catch (e) { pairs['entry_fail'] = 1; }
  try {
    // 0x4ef0b6 是 cvttsd2si 之后，eax = 抽到的长度（有符号 32 位）
    Interceptor.attach(m.base.add(ptr('0x4ef0b6')), {
      onEnter() { if (depth === 1) drawn = this.context.rax.toInt32(); }
    });
  } catch (e) { pairs['len_fail'] = 1; }
  try {
    Interceptor.attach(m.base.add(ptr('0x4ef2b0')), { onEnter() { if (depth > 0) emitted++; } });
  } catch (e) { pairs['disp_fail'] = 1; }
}, 300);
setInterval(() => send({ pairs, total, ready }), 3000);
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
  script.message.connect((msg) => { if (msg.payload && msg.payload.pairs) last = msg.payload; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '40', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  console.log('完整对样本:', last.total);
  const rows = Object.entries(last.pairs).filter(([k]) => k.includes('->'))
    .map(([k, v]) => [k, v]).sort((a, b) => b[1] - a[1]);
  console.log('抽到->吐出  次数');
  rows.slice(0, 15).forEach(([k, v]) => console.log(`  ${k.padEnd(12)} ${v}`));
  const fails = Object.keys(last.pairs).filter((k) => !k.includes('->'));
  if (fails.length) console.log('异常键:', fails.join(','));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
