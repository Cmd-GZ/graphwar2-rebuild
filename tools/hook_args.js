/**
 * 在真实对局里挂 hook 并抓参数：每个目标函数记录前 N 次调用的寄存器值。
 * 用途：为回合/生命/开火结算取得真值样本。
 */
const path = require('path');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const TARGETS = {
  shot_calc: 0x3c2bc0, calculate_path: 0x3c2530, parse_function: 0x4ed670,
  turn_a: 0x4628d0, turn_b: 0x4627c0, life_entry: 0x454b40,
  levelgen_a: 0x466c40, levelgen_b: 0x466d20,
};
const MAXKEEP = 3;

const SRC = `
const targets = ${JSON.stringify(TARGETS)};
const MAXKEEP = ${MAXKEEP};
const counts = {}; const samples = {};
let installed = false;
setInterval(() => {
  if (installed) return;
  const gw = Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
  if (!gw) return;
  installed = true;
  for (const [name, rva] of Object.entries(targets)) {
    counts[name] = 0; samples[name] = [];
    try {
      Interceptor.attach(gw.base.add(rva), {
        onEnter(args) {
          counts[name]++;
          if (samples[name].length < MAXKEEP) {
            samples[name].push({
              rdi: args[0].toString(), rsi: args[1].toString(),
              rdx: args[2].toString(), rcx: args[3].toString(),
              xmm0: this.context.xmm0.toString(), xmm1: this.context.xmm1.toString(),
            });
          }
        }
      });
    } catch (e) { samples[name] = 'ERR:' + e.message; }
  }
  send({ installed: true });
}, 5);
setInterval(() => { if (installed) send({ counts: counts, samples: samples }); }, 5000);
`;

(async () => {
  const dev = await frida.getLocalDevice();
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: ROOT + '/.godot-data', XDG_CACHE_HOME: ROOT + '/.godot-cache',
    XDG_CONFIG_HOME: ROOT + '/.godot-config',
  });
  const seconds = Number(process.argv[2] || 45);
  const pid = await dev.spawn([ROOT + '/tools/Godot_v4.7.2-stable_linux.x86_64', '--headless', '--path', ROOT + '/godot-live'], { env, cwd: ROOT });
  const session = await dev.attach(pid);
  const script = await session.createScript(SRC);
  let last = null;
  script.message.connect((m) => { if (m.payload && m.payload.counts) last = m.payload; });
  await script.load();
  await dev.resume(pid);
  await sleep(seconds * 1000);
  console.log(JSON.stringify(last));
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
