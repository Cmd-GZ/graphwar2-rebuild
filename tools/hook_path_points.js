/** 抓真游戏 calculate_path(0x3c2530) 的输入与输出点序列，用于与我们实现逐点对拍。 */
const path = require('path'); const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const SRC = `
let ready = false; const runs = [];
const timer = setInterval(() => {
  if (ready) return;
  const m = Process.enumerateModules().find(x => /graphwarrust/i.test(x.name));
  if (!m) return; ready = true; clearInterval(timer);
  try {
    Interceptor.attach(m.base.add(ptr('0x3c2530')), {
      onEnter(args) {
        this.sret = args[0];
        // 尝试记录几个可能的标量参数（浮点参数在 xmm0..）
        this.c = this.context;
        this.info = {
          xmm0: this.context.xmm0 ? null : null,
        };
      },
      onLeave() {
        if (runs.length >= 6) return;
        try {
          const p = this.sret;
          const ptr = p.readPointer();
          const cap = p.add(8).readU64().toNumber();
          const len = p.add(16).readU64().toNumber();
          if (len > 5000) return;
          const pts = [];
          for (let k = 0; k < Math.min(len, 40); k++) {
            pts.push([ptr.add(k * 16).readDouble(), ptr.add(k * 16 + 8).readDouble()]);
          }
          const last = len > 0 ? [ptr.add((len - 1) * 16).readDouble(),
                                  ptr.add((len - 1) * 16 + 8).readDouble()] : null;
          runs.push({ len, cap, first: pts, last });
        } catch (e) {}
      }
    });
  } catch (e) { runs.push({ fail: String(e).slice(0, 80) }); }
}, 300);
setInterval(() => send({ runs, ready }), 3000);
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
  script.message.connect((m) => { if (m.payload && m.payload.runs) last = m.payload.runs; });
  await script.load(); await dev.resume(pid);
  await sleep(parseInt(process.argv[3] || '60', 10) * 1000);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}
  if (!last) { console.log('NO_DATA'); return; }
  fs.writeFileSync(path.join(ROOT, '.rea-work/path_points.json'), JSON.stringify(last, null, 1));
  console.log('抓到', last.length, '次路径计算');
  for (const r of last.slice(0, 4)) {
    console.log(`  点数=${r.len} 终点=${JSON.stringify(r.last)} 前3点=${JSON.stringify((r.first||[]).slice(0,3))}`);
  }
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
