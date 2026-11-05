/** 抓完整的 token 序列，检查是不是合法的前缀式。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const seqs = []; let depth = 0; let cur = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x4ef020')), {
      onEnter() { if (depth++ === 0) cur = []; },
      onLeave() { if (--depth === 0 && cur.length && seqs.length < 25) seqs.push(cur.slice()); }
    });
  } catch (e) {}
  try {
    Interceptor.attach(m.base.add(ptr('0x4ef2b0')), {
      onEnter() { if (depth > 0 && cur.length < 40) cur.push(this.context.r13.toUInt32() & 0xff); }
    });
  } catch (e) {}
}, 300);
setInterval(() => send({ seqs, ready }), 3000);
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
  script.message.connect((msg) => { if (msg.payload && msg.payload.seqs) last = msg.payload.seqs; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '35', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  const ARITY = {0:0,1:0,2:0,17:0,3:2,4:2,6:2,7:2,8:2,5:1,9:1,10:1,11:1,12:1,13:1,14:1,15:1,16:1};
  // 规则：从左往右走，need 从 1 开始；叶子 -1、算子 +arity。
  // 首次 need == 0 的位置即"表达式完成点"（GW1 adjustFunction 的截断处）。
  const cuts = [];
  let never = 0;
  for (const s of last) {
    let need = 1, cut = -1;
    for (let i = 0; i < s.length; i++) {
      need -= 1;
      need += (s[i] in ARITY ? ARITY[s[i]] : 1);
      if (need === 0) { cut = i + 1; break; }
    }
    if (cut > 0) cuts.push(cut); else never++;
  }
  console.log('抓到序列:', last.length, ' 能截出合法式:', cuts.length, ' 截不出:', never);
  console.log('截断位置分布:', JSON.stringify(cuts.sort((a,b)=>a-b)));
  for (const s of last.slice(0, 4)) {
    let need = 1, cut = -1;
    for (let i = 0; i < s.length; i++) {
      need -= 1; need += (s[i] in ARITY ? ARITY[s[i]] : 1);
      if (need === 0) { cut = i + 1; break; }
    }
    console.log(`  序列=${JSON.stringify(s)}`);
    console.log(`    → 截断于 ${cut} 个 token: ${JSON.stringify(s.slice(0, cut))}`);
  }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
