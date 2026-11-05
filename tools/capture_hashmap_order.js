/**
 * 顺序取证采集器：一次进程内跑两局，抓①完整报文流（按 fd 重组成帧）②两份熵的取值。
 *
 *   node tools/capture_hashmap_order.js <out.json> [seconds] [pinHash16B] [pinChaCha32B] [--keys]
 *
 *   pinHash16B : 16 字节十六进制。给了就把**所有** len=16 的 getrandom 返回改成它
 *                （Rust std 的 RandomState base key 从这里来）。
 *   pinChaCha32B: 32 字节十六进制。给了就改 syscall(SYS_getrandom=318) 的返回
 *                （rand 的 ThreadRng/ChaCha 种子从这里来）。
 *   --keys     : 额外 hook 0x47dec0（内联 SipHash-1-3，键 = [rdi] 的 16 字节）记录实际哈希键。
 *
 * 为什么要钉：见 docs/113。不钉时顺序每次进程都不同；钉住后跨进程逐字节一致。
 *
 * 两个坑（都真踩过）：
 *   1. 协议帧头是 **u16 大端 2 字节** + JSON（docs/24 §1），按 3 字节解析会静默 0 帧；
 *   2. 游戏把一帧拆成两次 send（先 2 字节长度、再 JSON 体），必须按 fd 累积。
 *
 * harness：本脚本假定 .agent-work/hashdet/godot-live 存在且 probe.gd 换成了
 * tools/probe_two_matches.gd（一进程两局）。路径与仓库其它 tools/*.js 一致，写死根目录。
 */
const path = require('path');
const fs = require('fs');
const ROOT = '/home/user/MyProject/gw2';
const WORK = path.join(ROOT, '.agent-work/hashdet');
const frida = require(path.join(ROOT, '.rea-runtime/node_modules/frida'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const OUT = process.argv[2] || path.join(WORK, 'run/cap.json');
const SECONDS = Number(process.argv[3] || 40);
const PIN_HASH = process.argv[4] || '';
const PIN_CHACHA = process.argv[5] || '';
const WITH_KEYS = process.argv.indexOf('--keys') >= 0;

const SRC = `
const libc = Process.getModuleByName('libc.so.6');
const PIN = ${JSON.stringify(PIN_HASH)};
const PINC = ${JSON.stringify(PIN_CHACHA)};
const WITH_KEYS = ${WITH_KEYS};
const gen = [];            // 熵观测
let framelog = [];         // 重组出来的协议帧
const fds = {};            // fd -> {buf, proto, bad}
let rawFrames = 0, pinnedH = 0, pinnedC = 0;
const keys = {};           // SipHash 键（--keys 时）
let sipCalls = 0;

function hexOf(u8) { let s=''; for (let i=0;i<u8.length;i++) s += u8[i].toString(16).padStart(2,'0'); return s; }

/* ---------- ① 哈希种子：libc getrandom ---------- */
function siteOf(ctx) {
  try {
    const bt = Thread.backtrace(ctx, Backtracer.FUZZY);
    for (const a of bt) {
      const m = Process.findModuleByAddress(a);
      if (m && /graphwarrust/i.test(m.name)) return 'GW+' + a.sub(m.base).toString();
    }
  } catch (e) {}
  return '?';
}
try {
  Interceptor.attach(libc.getExportByName('getrandom'), {
    onEnter(a) { this.buf = a[0]; this.len = a[1].toUInt32(); this.tid = Process.getCurrentThreadId(); this.site = siteOf(this.context); },
    onLeave(r) {
      const n = r.toInt32();
      if (n <= 0) return;
      let before = null, after = null;
      try { before = hexOf(new Uint8Array(this.buf.readByteArray(n))); } catch (e) {}
      if (PIN && n === 16) {
        try {
          const b = [];
          for (let i = 0; i < 16; i++) b.push(parseInt(PIN.substr(i*2,2), 16));
          this.buf.writeByteArray(b); pinnedH++;
        } catch (e) {}
      }
      try { after = hexOf(new Uint8Array(this.buf.readByteArray(n))); } catch (e) {}
      if (gen.length < 4000) {
        const rec = { t: Date.now(), tid: this.tid, len: n, site: this.site };
        if (this.site.indexOf('GW+') === 0) { rec.before = before; rec.after = after; }
        gen.push(rec);
      }
    }
  });
} catch (e) { gen.push({ err: 'attach getrandom: ' + e }); }

/* ---------- ② ChaCha 种子：libc syscall(SYS_getrandom=318) ---------- */
try {
  Interceptor.attach(libc.getExportByName('syscall'), {
    onEnter(a) { this.n = a[0].toInt32(); if (this.n !== 318) return; this.buf = a[1]; this.len = a[2].toUInt32(); },
    onLeave(r) {
      if (this.n !== 318) return;
      const got = r.toInt32();
      if (got <= 0) return;
      if (PINC && this.len * 2 <= PINC.length) {
        try {
          const b = [];
          for (let i = 0; i < this.len; i++) b.push(parseInt(PINC.substr(i*2, 2), 16));
          this.buf.writeByteArray(b); pinnedC++;
        } catch (e) {}
      }
      if (gen.length < 4000) {
        let after = null;
        try { after = hexOf(new Uint8Array(this.buf.readByteArray(Math.min(got, 32)))); } catch (e) {}
        gen.push({ t: Date.now(), tid: Process.getCurrentThreadId(), len: this.len, site: 'syscall318', after });
      }
    }
  });
} catch (e) { gen.push({ err: 'attach syscall: ' + e }); }

/* ---------- ③ 报文帧：按 fd 累积 + u16 大端拆帧 ---------- */
const HDR = 2;
function feed(fd, u8, tid) {
  if (fd <= 2) return;
  let st = fds[fd];
  if (!st) { st = fds[fd] = { buf: new Uint8Array(0), proto: false, bad: 0 }; }
  if (st.bad > 3) return;
  const nb = new Uint8Array(st.buf.length + u8.length);
  nb.set(st.buf, 0); nb.set(u8, st.buf.length);
  st.buf = nb;
  let off = 0;
  while (st.buf.length - off >= HDR) {
    const len = (st.buf[off] << 8) | st.buf[off+1];
    if (len === 0 || len > 60000) { st.bad++; st.buf = new Uint8Array(0); return; }
    if (st.buf.length - off < HDR + len) break;
    let txt = '';
    const body = st.buf.subarray(off+HDR, off+HDR+len);
    try { txt = String.fromCharCode.apply(null, Array.from(body)); } catch (e) { txt = ''; }
    if (txt.charCodeAt(0) !== 0x7b /* { */) { st.bad++; st.buf = new Uint8Array(0); return; }
    st.proto = true; rawFrames++;
    if (framelog.length < 200000) framelog.push({ fd, tid, t: Date.now(), len, msg: txt.slice(0, 400) });
    off += HDR + len;
  }
  st.buf = st.buf.subarray(off);
  if (st.buf.length > 60000) st.buf = new Uint8Array(0);
}
function hookRw(name, kind) {
  try {
    const p = libc.getExportByName(name);
    if (!p) return;
    Interceptor.attach(p, { onEnter(args) {
      const fd = args[0].toInt32();
      if (fd <= 2) return;
      const tid = Process.getCurrentThreadId();
      try {
        if (kind === 'send' || kind === 'write') {
          const len = args[2].toUInt32();
          if (len === 0 || len > 65536) return;
          feed(fd, new Uint8Array(args[1].readByteArray(len)), tid);
        } else if (kind === 'writev' || kind === 'sendmsg') {
          const iov = kind === 'writev' ? args[1] : args[1].add(16).readPointer();
          const cnt = kind === 'writev' ? args[2].toInt32() : args[1].add(24).toInt32();
          if (cnt <= 0 || cnt > 64) return;
          for (let i = 0; i < cnt; i++) {
            const base = iov.add(i*16).readPointer();
            const l = iov.add(i*16+8).toUInt32();
            if (l === 0 || l > 65536) continue;
            feed(fd, new Uint8Array(base.readByteArray(l)), tid);
          }
        }
      } catch (e) {}
    }});
  } catch (e) {}
}
hookRw('send', 'send'); hookRw('write', 'write');
hookRw('writev', 'writev'); hookRw('sendmsg', 'sendmsg');

/* ---------- ④ 可选：SipHash 键（0x47dec0，键 = [rdi] 的 16 字节） ---------- */
if (WITH_KEYS) {
  let hooked = false;
  setInterval(() => {
    if (hooked) return;
    const m = Process.findModuleByName('libgraphwarrust.so');
    if (!m) return;
    hooked = true;
    try {
      Interceptor.attach(m.base.add(0x47dec0), { onEnter(args) {
        sipCalls++;
        try {
          const k = hexOf(new Uint8Array(args[0].readByteArray(16)));
          let e = keys[k];
          if (!e) { if (Object.keys(keys).length < 400) keys[k] = e = { n: 0, tid: Process.getCurrentThreadId(), t: Date.now() }; }
          e.n++;
        } catch (e) {}
      }});
    } catch (e) { gen.push({ err: 'attach 0x47dec0: ' + e }); }
  }, 30);
}

let genSent = 0, frameSent = 0;
setInterval(() => {
  try {
    const g = gen.slice(genSent); genSent = gen.length;
    const f = framelog.slice(frameSent); frameSent = framelog.length;
    send({ kind: 'snap', gen: g, frames: f, nf: rawFrames, pinnedH, pinnedC, sipCalls, keys,
           fds: Object.keys(fds).map(k => ({ fd: +k, proto: fds[k].proto, bad: fds[k].bad })) });
  } catch (e) { send({ kind: 'err', msg: String(e) }); }
}, 1000);
`;

(async () => {
  const dev = await frida.getLocalDevice();
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: path.join(WORK, '.godot-data'),
    XDG_CACHE_HOME: path.join(WORK, '.godot-cache'),
    XDG_CONFIG_HOME: path.join(WORK, '.godot-config'),
  });
  const argv = [path.join(ROOT, 'tools/Godot_v4.7.2-stable_linux.x86_64'),
                '--headless', '--path', path.join(WORK, 'godot-live')];
  const pid = await dev.spawn(argv, { env, cwd: WORK });
  const session = await dev.attach(pid);
  const script = await session.createScript(SRC, { runtime: 'v8' });   // 必须 v8，见 docs/100
  const allGen = [], allFrames = [];
  let keys = {}, fds = null, pinnedH = 0, pinnedC = 0, sipCalls = 0, exited = false;
  try { session.detached.connect(() => { exited = true; }); } catch (e) {}
  script.message.connect((m) => {
    if (m.type === 'error') { console.error('SCRIPT_ERROR', m.description || JSON.stringify(m)); return; }
    const p = m.payload;
    if (!p || p.kind !== 'snap') return;
    for (const g of p.gen) allGen.push(g);
    for (const f of p.frames) allFrames.push(f);
    keys = p.keys || keys; fds = p.fds; pinnedH = p.pinnedH; pinnedC = p.pinnedC; sipCalls = p.sipCalls;
  });
  await script.load();
  await dev.resume(pid);
  const deadline = Date.now() + SECONDS * 1000;
  while (Date.now() < deadline && !exited) await sleep(500);
  try { await session.detach(); } catch (e) {}
  try { await dev.kill(pid); } catch (e) {}

  const seen = new Set(); const gen = [];
  for (const g of allGen) { const k = JSON.stringify(g); if (!seen.has(k)) { seen.add(k); gen.push(g); } }
  fs.writeFileSync(OUT, JSON.stringify({ pinHash: PIN_HASH, pinChaCha: PIN_CHACHA,
    pinnedHash: pinnedH, pinnedChaCha: pinnedC, sipCalls, keys, exited, fds, gen, frames: allFrames }, null, 1));
  console.log('WROTE', OUT, 'frames', allFrames.length, 'gen', gen.length,
              'pinnedH', pinnedH, 'pinnedC', pinnedC, 'keys', Object.keys(keys).length);
  process.exit(0);
})().catch((e) => { console.error('ERR', e && e.stack || e); process.exit(1); });
