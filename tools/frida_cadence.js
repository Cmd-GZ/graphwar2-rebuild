// tools/frida_cadence.js —— `GraphwarGameClient::process`(`0x518400`) 的**节奏**探针（只读，`dispatch/80`）
//
// 与 `tools/frida_process_chain.js`（`dispatch/43`，只数「每秒几次」）的分工：
// 那个只给 n/s；这一个给**每次 process 内部的形状**：
//
//   ① 每帧几个 `tick`、几个泵（`pump`）—— **追赶**直接落在这两张直方图上
//   ② `tick` 之间的墙钟间隔（µs）—— 定步是不是由墙钟决定、追赶时是不是背靠背
//   ③ `0x51848e` 那个判决点（`cmp edx,0x3b9aca00`）上 `edx` 的实际取值 ——
//      「欠一个 tick」哨兵（1e9）vs 还差多少纳秒（0..1e9-1），每个循环一圈一条
//   ④ 一帧内的**事件顺序**（`T`=tick / `P`=泵）与**调用者**（返回地址 RVA）
//
// 口径：地址一律 **ELF 虚地址（RVA）**，下断 = base + RVA（Ghidra 地址 = RVA + 0x100000）。
// 构建 = Graphwar II Linux 2.0.60，sha256 9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187。
//
// 时间源：`clock_gettime(CLOCK_MONOTONIC)`（`Date.now()` 只有 ms，量不了 16.7 ms 上的抖动）。
// ⚠ `CLOCK_MONOTONIC` 在进程被 `SIGSTOP` 时**继续走** —— 追赶实验（`--stall`）就靠这一点。
'use strict';

// ── 下钩的地址（身份见 docs/16 §48.1 / §48.3；`tools/addr_probe.py locs/dis`）──
const HOOKS = [
  { key: 'root',    rva: 0x3a5760, name: 'RootNode::process' },              // 引擎 _process（帧节拍的正对照）
  { key: 'backend', rva: 0x515dc0, name: 'Backend::process' },               // 中间那一跳
  { key: 'proc',    rva: 0x518400, name: 'GameClient::process' },            // ★ 目标：一帧一次
  { key: 'tick',    rva: 0x518240, name: 'GameClient::tick' },               // ★ 循环体 ①
  { key: 'pump',    rva: 0x3d3630, name: 'EventHandler::process_events' },   // ★ 循环体 ②（事件泵）
  { key: 'gtt',     rva: 0x519450, name: 'TickHandler::get_time_to_tick' },  // 时基（含服务端线程的调用）
  { key: 'due',     rva: 0x51848e, name: 'process:cmp_due' },                // ★ 判决点（edx = 剩余 ns / 1e9 哨兵）
  { key: 'neg',     rva: 0x463da0, name: 'TurnManager::next_turn' },         // 负对照（只在换手时调）
  { key: 'delta',   rva: 0x42a8bf, name: 'vcall:load_delta' },               // `movsd xmm0,[rax]`：引擎这一步交给 _process 的 f64
];

const CFG = { tallyMs: 1000, firstFramesMax: 40, longFrameMin: 2, longFramesMax: 80 };

let mod = null;
let clock_gettime = null;
let tsbuf = null;
let startedUs = 0;
let snapshotSeq = 0;

// frida 17 删了 `Module.getExportByName`；三档回退，找不到就退回 ms 时钟（并如实报 `time_src`）
function findExport(name) {
  try {
    if (typeof Module.getGlobalExportByName === 'function') return Module.getGlobalExportByName(name);
  } catch (e) { /* 继续回退 */ }
  try {
    if (typeof Module.getExportByName === 'function') return Module.getExportByName(null, name);
  } catch (e) { /* 继续回退 */ }
  try { return Process.getModuleByName('libc.so.6').getExportByName(name); } catch (e) { /* 放弃 */ }
  return null;
}
let timeSrc = 'none';

// ── 微秒时钟 ─────────────────────────────────────────────────────────────────
function nowUs() {
  if (!clock_gettime) return Date.now() * 1000;
  clock_gettime(1 /* CLOCK_MONOTONIC */, tsbuf);
  return tsbuf.readU64(0).toNumber() * 1000000 + Math.floor(tsbuf.readU64(8).toNumber() / 1000);
}

function rvaOf(a) {
  if (mod) {
    const r = a.sub(mod.base);
    if (r.compare(0) >= 0 && r.compare(mod.size) < 0) return '0x' + r.toString(16);
  }
  return a.toString();
}

// ── µs 分桶（间隔分布一律走这套；桶标签随 `buckets` 消息发一次）────────────────
const BUCKETS = [
  [0, 1], [1, 2], [2, 4], [4, 8], [8, 16], [16, 33], [33, 66], [66, 100],
  [100, 500], [500, 1000], [1000, 5000], [5000, 10000], [10000, 15000],
  [15000, 16500], [16500, 17000], [17000, 20000], [20000, 30000], [30000, 50000],
  [50000, 100000], [100000, 500000], [500000, 1e12],
];
const BUCKET_LABELS = BUCKETS.map((b, i) =>
  (i === BUCKETS.length - 1 ? '>=' + b[0] : '[' + b[0] + ',' + b[1] + ')'));
function bk(d) {
  for (let i = 0; i < BUCKETS.length; i++) if (d < BUCKETS[i][1]) return i;
  return BUCKETS.length - 1;
}
function bump(o, k) { o[k] = (o[k] || 0) + 1; }
// frida 的 CpuContext 寄存器在不同版本里可能是 number / UInt64 / Int64（没有 `toUint32`）
function toU32(v) {
  if (typeof v === 'number') return v >>> 0;
  if (v && typeof v.toNumber === 'function') return Math.abs(v.toNumber()) % 4294967296;
  const n = Number(v);
  return isFinite(n) ? (Math.abs(n) % 4294967296) : 0;
}
function stat(o, v) {
  o.n++; o.sum += v;
  if (v < o.min) o.min = v;
  if (v > o.max) o.max = v;
}
function newStat(withHist) {
  const o = { n: 0, sum: 0, min: 1e18, max: 0 };
  if (withHist) o.hist = {};
  return o;
}

// ── 聚合状态（`reset` 时整体重建）─────────────────────────────────────────────
let A = null;
function resetAgg() {
  A = {
    t0: nowUs(),
    counts: {},        // name -> {n, threads:{}, sites:{}}
    bySite: {},        // 返回地址 -> 每帧聚合
    ticks: 0, pumps: 0, probes: 0, due: 0,
    probeSlack: {},    // 判决点上 edx（µs 桶），**不含**哨兵
    probeSlackNs: newStat(false),
    tickIv: {},        // 相邻两个 client tick 的墙钟间隔（µs 桶）
    tickIvStat: newStat(false),
    procIv: {},        // 相邻两帧的间隔（µs 桶，只算"最多帧的那个返回地址"）
    longFrames: [],
    firstFrames: [],
    neg: 0,
    // 引擎 `_process(delta)` 的 delta 原文（0x42a8bf 上从 Variant 里读出来的 f64）
    delta_us: {}, deltaStat: newStat(false), delta_bad: 0, deltaSamples: [],
  };
  lastTickUs = null;
  for (const k of Object.keys(lastFrameUs)) delete lastFrameUs[k];
  for (const k of Object.keys(FRAME)) delete FRAME[k];
}
let lastTickUs = null;
let diagRegSent = false;
const lastFrameUs = {};   // site -> us
const FRAME = {};         // tid -> 进行中的 frame

function cnt(key, tid, site) {
  let c = A.counts[key];
  if (!c) c = A.counts[key] = { n: 0, threads: {}, sites: {} };
  c.n++;
  const tk = '' + tid;
  c.threads[tk] = (c.threads[tk] || 0) + 1;
  if (site) c.sites[site] = (c.sites[site] || 0) + 1;
}

// "每帧调用者" = 帧数最多的那个返回地址（主菜单里是 `0x515dfe`；准备阶段还会多出 `0x51718b` 忙等）
function dominantSite() {
  let best = null, bn = -1;
  for (const k of Object.keys(A.bySite)) {
    if (A.bySite[k].n > bn) { bn = A.bySite[k].n; best = k; }
  }
  return best;
}

function finalize(f) {
  let s = A.bySite[f.site];
  if (!s) {
    s = A.bySite[f.site] = {
      n: 0, ticks_total: 0, pumps_total: 0, probes_total: 0,
      ticks_hist: {}, pumps_hist: {}, probes_hist: {},
      dwell_us: newStat(false), gap_us: newStat(false), gap_hist: {},
      zero_tick_n: 0, first_pump_n: 0, order_ok_n: 0, order_bad_n: 0,
    };
  }
  s.n++;
  s.ticks_total += f.ticks; s.pumps_total += f.pumps; s.probes_total += f.probes;
  bump(s.ticks_hist, f.ticks);
  bump(s.pumps_hist, f.pumps);
  bump(s.probes_hist, f.probes);
  stat(s.dwell_us, f.dwell);
  if (f.gap >= 0) { stat(s.gap_us, f.gap); bump(s.gap_hist, bk(f.gap)); }
  if (f.ticks === 0) s.zero_tick_n++;
  if (f.order.length && f.order[0] === 'P') s.first_pump_n++;
  const exact = !f.trunc && f.ticks <= 20 && f.pumps === f.ticks && f.order === 'TP'.repeat(f.ticks);
  if (exact) s.order_ok_n++; else s.order_bad_n++;

  if (A.firstFrames.length < CFG.firstFramesMax) {
    A.firstFrames.push({
      t_ms: Math.round((f.t0 - A.t0) / 1000), site: f.site,
      ticks: f.ticks, pumps: f.pumps, probes: f.probes,
      dwell_us: f.dwell, gap_us: f.gap, order: f.order + (f.trunc ? '…' : ''),
    });
  }
  if (f.ticks >= CFG.longFrameMin && A.longFrames.length < CFG.longFramesMax) {
    A.longFrames.push({
      t_ms: Math.round((f.t0 - A.t0) / 1000), site: f.site,
      ticks: f.ticks, pumps: f.pumps, probes: f.probes,
      dwell_us: f.dwell, gap_us: f.gap, order: f.order + (f.trunc ? '…' : ''),
    });
  }
}

function snapshot(reason) {
  const dom = dominantSite();
  const out = {
    type: 'tally', seq: snapshotSeq++, reason: reason || 'timer',
    wall: new Date().toISOString(),
    since_reset_s: Math.round((nowUs() - A.t0) / 10000) / 100,
    counts: A.counts,
    bySite: A.bySite,
    totals: { ticks: A.ticks, pumps: A.pumps, probes: A.probes, due: A.due, neg: A.neg },
    probe_slack_us_hist: A.probeSlack,
    probe_slack_ns: A.probeSlackNs,
    tick_iv_us_hist: A.tickIv,
    tick_iv_us: A.tickIvStat,
    proc_iv_us_hist: A.procIv,
    long_frames: A.longFrames,
    first_frames: A.firstFrames,
    dominant_site: dom,
    proc_iv: dom && A.bySite[dom] ? A.bySite[dom].gap_us : null,
    delta_us_hist: A.delta_us,
    delta_us: A.deltaStat,
    delta_bad: A.delta_bad,
    delta_samples: A.deltaSamples,
  };
  send(out);
  // 每秒的清零（直方图 / first_frames / long_frames 是累计的，只在 reset() 时清）
  A.counts = {};
  A.probeSlack = {};
  A.tickIv = {};
  A.procIv = {};
  A.delta_us = {};
  A.probeSlackNs = newStat(false);
  A.tickIvStat = newStat(false);
  return true;
}

// ── 下钩 ────────────────────────────────────────────────────────────────────
function install() {
  const addr = {};
  for (const h of HOOKS) addr[h.key] = mod.base.add(h.rva);
  const fail = (name, e) => send({ type: 'hook_error', name: name, error: '' + e });

  try {
    Interceptor.attach(addr.proc, {
      onEnter() {
        const t = nowUs();
        const tid = Process.getCurrentThreadId();
        const site = rvaOf(this.returnAddress);
        const prev = lastFrameUs[site];
        FRAME[tid] = {
          t0: t, site: site, ticks: 0, pumps: 0, probes: 0,
          order: '', trunc: false, gap: prev === undefined ? -1 : t - prev,
        };
        lastFrameUs[site] = t;
        cnt('proc', tid, site);
      },
      onLeave() {
        const tid = Process.getCurrentThreadId();
        const f = FRAME[tid];
        if (!f) return;
        delete FRAME[tid];
        f.dwell = nowUs() - f.t0;
        finalize(f);
      },
    });
  } catch (e) { fail('GameClient::process', e); }

  try {
    Interceptor.attach(addr.tick, {
      onEnter() {
        const tid = Process.getCurrentThreadId();
        cnt('tick', tid, rvaOf(this.returnAddress));
        const f = FRAME[tid];
        if (!f) return;               // 不在 process 里的 tick 不计入帧（和服务端线程的区分）
        f.ticks++;
        A.ticks++;
        if (f.order.length < 40) f.order += 'T'; else f.trunc = true;
        const t = nowUs();
        if (lastTickUs !== null) {
          const d = t - lastTickUs;
          bump(A.tickIv, bk(d));
          stat(A.tickIvStat, d);
        }
        lastTickUs = t;
      },
    });
  } catch (e) { fail('GameClient::tick', e); }

  try {
    Interceptor.attach(addr.pump, {
      onEnter() {
        const tid = Process.getCurrentThreadId();
        cnt('pump', tid, rvaOf(this.returnAddress));
        const f = FRAME[tid];
        if (!f) return;
        f.pumps++;
        A.pumps++;
        if (f.order.length < 40) f.order += 'P'; else f.trunc = true;
      },
    });
  } catch (e) { fail('EventHandler::process_events', e); }

  // ★ 判决点：0x51848e `cmp edx,0x3b9aca00`。此刻 edx = `get_time_to_tick` 返回的 nanos。
  //   == 1e9 ⇒ "欠一个 tick"（哨兵）；否则 = 距离下一个 tick 还有多少 ns。
  try {
    Interceptor.attach(addr.due, {
      onEnter() {
        const tid = Process.getCurrentThreadId();
        cnt('due', tid, null);
        const f = FRAME[tid];
        if (f) f.probes++;
        A.probes++;
        if (!diagRegSent) {
          diagRegSent = true;
          send({ type: 'diag_reg', rdx_type: typeof this.context.rdx,
                 rdx_str: '' + this.context.rdx, rax_str: '' + this.context.rax });
        }
        const nanos = toU32(this.context.rdx);
        if (nanos === 1000000000) A.due++;
        else {
          bump(A.probeSlack, bk(Math.floor(nanos / 1000)));
          stat(A.probeSlackNs, nanos);
        }
      },
    });
  } catch (e) { fail('process:cmp_due', e); }

  try {
    Interceptor.attach(addr.gtt, {
      onEnter() { cnt('gtt', Process.getCurrentThreadId(), rvaOf(this.returnAddress)); },
    });
  } catch (e) { fail('get_time_to_tick', e); }

  // ★ delta 原文：0x42a8bf `movsd xmm0,[rax]`（typed vcall 里把 Variant 的 f64 装进 xmm0）。
  //   这是**引擎交给 `_process` 的那个 delta**；`frida` 不暴露 xmm，但 rax 就是那个 f64 的地址。
  try {
    Interceptor.attach(addr.delta, {
      onEnter() {
        cnt('delta', Process.getCurrentThreadId(), null);
        let v;
        try {
          const r = this.context.rax;
          const p = (typeof r === 'number') ? ptr(r) : r;
          v = p.readDouble();
        } catch (e) { A.delta_bad++; return; }
        if (!isFinite(v) || v < 0 || v > 10) { A.delta_bad++; return; }
        const us = Math.round(v * 1e6);
        bump(A.delta_us, bk(us));
        stat(A.deltaStat, us);
        if (A.deltaSamples.length < 24) A.deltaSamples.push({ us: us, v: v });
      },
    });
  } catch (e) { fail('vcall:load_delta', e); }

  for (const k of ['root', 'backend', 'neg']) {
    try {
      Interceptor.attach(addr[k], {
        onEnter() {
          cnt(k, Process.getCurrentThreadId(), rvaOf(this.returnAddress));
          if (k === 'neg') A.neg++;
        },
      });
    } catch (e) { fail(k, e); }
  }

  send({ type: 'hooks', hooks: HOOKS.map((h) => ({ name: h.name, rva: '0x' + h.rva.toString(16) })) });
  send({ type: 'buckets', labels: BUCKET_LABELS });
}

let iv = null;
function armTimer() {
  if (iv) { clearInterval(iv); iv = null; }
  iv = setInterval(() => {
    try { snapshot('timer'); } catch (e) { send({ type: 'snap_error', e: '' + e }); }
  }, CFG.tallyMs);
}

function start() {
  const m = Process.findModuleByName('libgraphwarrust.so');
  if (!m) { send({ type: 'error', msg: 'libgraphwarrust.so not loaded' }); return; }
  mod = m;
  const cg = findExport('clock_gettime');
  if (cg) {
    clock_gettime = new NativeFunction(cg, 'int', ['int', 'pointer'], 'default');
    tsbuf = Memory.alloc(16);
    timeSrc = 'clock_gettime(CLOCK_MONOTONIC)';
  } else {
    timeSrc = 'Date.now()(ms，降级)';
  }
  startedUs = nowUs();
  resetAgg();
  install();
  send({
    type: 'ready', base: m.base.toString(), size: m.size, path: m.path,
    main_tid: Process.getCurrentThreadId(), time_src: timeSrc,
  });
  armTimer();
}

rpc.exports = {
  cfg(o) { Object.assign(CFG, o || {}); armTimer(); return CFG; },
  mark(label) {
    send({ type: 'mark', label: label, wall: new Date().toISOString(),
           t_ms: Math.round((nowUs() - startedUs) / 1000) });
    return true;
  },
  reset(label) {
    resetAgg();
    send({ type: 'reset', label: label || '', wall: new Date().toISOString() });
    return true;
  },
  snap(reason) { return snapshot(reason || 'rpc'); },
  now() { return { t_ms: Math.round((nowUs() - startedUs) / 1000), wall: new Date().toISOString() }; },
};

setTimeout(start, 0);
