/**
 * 抓指定函数前 N 次调用的寄存器，并把 rdi/rsi 指向的前 4 个 8 字节一并读出。
 * 用途：为 shot_calc / life_entry 这类"参数是结构体指针"的函数取证。
 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const TARGETS = JSON.parse(process.argv[2] || '{"shot_calc":15734336,"life_entry":4541760}');
const MAXKEEP = Number(process.argv[3] || 4);

const SRC = `
function dbl(buf) { return new DataView(buf).getFloat64(0, true); }
function peek(p, n) {
  try { const out = []; for (let i = 0; i < n; i++) out.push(p.add(i*8).readU64().toNumber()); return out; }
  catch (e) { return 'ERR'; }
}
const targets = ${JSON.stringify(TARGETS)};
const MAXKEEP = ${MAXKEEP};
let installed = false; const samples = {}; const counts = {};
setInterval(() => {
  if (installed) return;
  const gw = Process.enumerateModules().find(m => /graphwarrust/i.test(m.name));
  if (!gw) return;
  installed = true;
  for (const [name, rva] of Object.entries(targets)) {
    samples[name] = []; counts[name] = 0;
    try {
      Interceptor.attach(gw.base.add(rva), {
        onEnter(args) {
          counts[name]++;
          if (samples[name].length >= MAXKEEP) return;
          const rec = {
            rdi: args[0].toString(), rsi: args[1].toString(),
            rdx: args[2].toString(), rcx: args[3].toString(),
            x0: dbl(this.context.xmm0), x1: dbl(this.context.xmm1), x2: dbl(this.context.xmm2),
            mem_rdi: peek(args[0], 4), mem_rsi: peek(args[1], 4),
          };
          samples[name].push(rec);
          this.rec = rec;
        },
        onLeave() {
          if (this.rec && this.rec.after === undefined) {
            this.rec.after_rdi = peek(ptr(this.rec.rdi), 4);
            this.rec.after_rsi = peek(ptr(this.rec.rsi), 4);
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
  const pid = await dev.spawn([ROOT + '/tools/Godot_v4.7.2-stable_linux.x86_64', '--headless', '--path', ROOT + '/godot-live'], { env, cwd: ROOT });
  const session = await dev.attach(pid);
  const script = await session.createScript(SRC);
  let last = null;
  script.message.connect((m) => { if (m.payload && m.payload.counts) last = m.payload; });
  await script.load();
  await dev.resume(pid);
  await sleep(60000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  fs.writeFileSync(path.join(ROOT, '.rea-work/captured_args.json'), JSON.stringify(last));
  console.log(JSON.stringify(last && last.counts));
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
