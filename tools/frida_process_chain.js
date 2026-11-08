// tools/frida_process_chain.js —— 「谁每帧驱动事件泵」的运行时计数探针（**只读**，`dispatch/43`）
//
// 口径：地址一律 **ELF 虚地址（RVA）**，下断 = base + RVA（`tools/frida_point_array.README.md`；
// Ghidra 地址 = RVA + 0x100000）。构建 = Graphwar II Linux 2.0.60，
// sha256 9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187。
//
// 每个钩子记三样东西（每秒发一条 `tally`）：
//   ① 每秒调用数（counts[hook].n）——「每帧/每秒几次」
//   ② **线程 id**（counts[hook].threads）——分清引擎主线程与 `game_server_thread`
//   ③ **返回地址**（counts[hook].sites，RVA）——运行时直接认出「谁调它」，
//      与静态的 `call *GOT` 站点（如 `0x515df8`）对账
//
// 钩的地址（身份见 docs/02 §21 / docs/16 §48）：
//
//   0x3a5760   GraphwarRootNode::process   （`INode::process` 的虚函数体 = 引擎 `_process` 回调）
//   0x515dc0   GraphwarBackend::process
//   0x518400   GraphwarGameClient::process  ← 本轮目标
//   0x519450   GraphwarTickHandler::get_time_to_tick
//   0x518240   GraphwarGameClient::tick
//   0x3d3630   GraphwarEventHandler::process_events
//   0x4500c0   game_server_thread（线程入口；只用来给线程 id 命名）
'use strict';

const HOOKS = [
  { rva: 0x3a5760, name: 'RootNode::process' },
  { rva: 0x515dc0, name: 'Backend::process' },
  { rva: 0x518400, name: 'GameClient::process' },
  { rva: 0x519450, name: 'TickHandler::get_time_to_tick' },
  { rva: 0x518240, name: 'GameClient::tick' },
  { rva: 0x3d3630, name: 'EventHandler::process_events' },
  { rva: 0x4500c0, name: 'game_server_thread' },
];

const cfg = { tallyMs: 1000 };
let mod = null;
let counts = {};
let threads = {};          // tid -> 名字（已知的线程入口）
let lastT = Date.now();
let seq = 0;
let iv = null;
const NB = 61;             // 0..60 ms，最后一个是溢出桶

function resetIv() {
  iv = { n: 0, sum: 0, min: 1e9, max: 0, hist: new Array(NB).fill(0), last: null };
}

function snapshot() {
  const now = Date.now();
  const dt = (now - lastT) / 1000.0;
  const out = { type: 'tally', seq: seq++, t: new Date().toISOString(), dt: dt, counts: {} };
  for (const k of Object.keys(counts)) {
    const h = counts[k];
    out.counts[k] = {
      n: h.n,
      per_s: dt > 0 ? h.n / dt : 0,
      threads: Object.assign({}, h.threads),
      sites: Object.assign({}, h.sites),
    };
    counts[k] = { n: 0, threads: {}, sites: {} };
  }
  if (iv) {
    out.root_interval_ms = {
      n: iv.n, mean: iv.n ? iv.sum / iv.n : 0, min: iv.n ? iv.min : 0, max: iv.max, hist: iv.hist,
    };
  }
  out.known_threads = Object.assign({}, threads);
  lastT = now;
  resetIv();
  send(out);
}

function rvaOf(a) {
  if (mod) {
    const r = a.sub(mod.base);
    if (r.compare(0) >= 0 && r.compare(mod.size) < 0) return '0x' + r.toString(16);
  }
  const list = Process.enumerateModules();
  for (const m of list) {
    if (a.compare(m.base) >= 0 && a.compare(m.base.add(m.size)) < 0) {
      return m.name + '+0x' + a.sub(m.base).toString(16);
    }
  }
  return a.toString();
}

function install() {
  for (const h of HOOKS) {
    const addr = mod.base.add(h.rva);
    try {
      Interceptor.attach(addr, {
        onEnter() {
          let c = counts[h.name];
          if (!c) c = counts[h.name] = { n: 0, threads: {}, sites: {} };
          c.n++;
          const tid = Process.getCurrentThreadId();
          const tkey = '' + tid;
          c.threads[tkey] = (c.threads[tkey] || 0) + 1;
          if (h.rva === 0x4500c0 && !threads[tkey]) {
            threads[tkey] = 'game_server_thread';
            send({ type: 'thread_name', tid: tid, name: 'game_server_thread' });
          }
          const ra = rvaOf(this.returnAddress);
          c.sites[ra] = (c.sites[ra] || 0) + 1;
          if (h.rva === 0x3a5760 && iv) {
            const t = Date.now();
            if (iv.last !== null) {
              const d = t - iv.last;
              iv.n++; iv.sum += d;
              if (d < iv.min) iv.min = d;
              if (d > iv.max) iv.max = d;
              iv.hist[d < NB - 1 ? d : NB - 1]++;
            }
            iv.last = t;
          }
        },
      });
    } catch (e) {
      send({ type: 'hook_error', name: h.name, error: '' + e });
    }
  }
  send({ type: 'hooks', hooks: HOOKS.map((h) => ({ name: h.name, rva: '0x' + h.rva.toString(16) })) });
}

function start() {
  const m = Process.findModuleByName('libgraphwarrust.so');
  if (!m) { send({ type: 'error', msg: 'libgraphwarrust.so not loaded' }); return; }
  mod = m;
  counts = {}; threads = {};
  resetIv();
  lastT = Date.now();
  install();
  send({ type: 'ready', base: m.base.toString(), size: m.size, path: m.path,
         main_tid: Process.getCurrentThreadId() });
  setInterval(snapshot, cfg.tallyMs);
}

rpc.exports = {
  cfg(o) { Object.assign(cfg, o || {}); return cfg; },
  mark(label) { send({ type: 'mark', label: label, t: new Date().toISOString() }); },
  // 取 n 条精确栈后**立刻 detach**（FUZZY backtrace 很贵：不摘掉会把 JSONL 撑到几十 MB）
  bt(rva, n) {
    const want = n || 2;
    const out = [];
    const listener = Interceptor.attach(mod.base.add(rva), {
      onEnter() {
        out.push(Thread.backtrace(this.context, Backtracer.FUZZY).map(rvaOf));
        if (out.length >= want) {
          send({ type: 'backtrace', rva: '0x' + rva.toString(16), frames: out });
          listener.detach();
        }
      },
    });
    return true;
  },
};

setTimeout(start, 0);
