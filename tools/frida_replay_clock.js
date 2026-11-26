// tools/frida_replay_clock.js —— 回放**播放时钟**的真机读数探针（T12 / `task-9`，**只读**）
//
// 目标：把 `GraphwarReplayScreenControl::process`(`0x4c6f40`) 那条"播放时钟"从静态指令
// 推进到**运行时真值**。静态已经读出（见 `docs/06` §63 与 `tools/replay_clock_disasm.py`）：
//
//   `GraphwarTickHandler`（`graphwar_tick_handler.rs`）是**全游戏唯一的时基**：
//     +0x10 u64 / +0x18 u32 = 已排程时间（每次 `tick()` 加 **16_666_666 ns**）
//     +0x40 u32             = **tick 计数器**（`tick()` 里 +1）
//     +0x20 u64 / +0x28 u32 = **暂停标记**：`nanos == 1_000_000_000`(1e9) ⇒ **没暂停**
//                             （`is_paused()` = `nanos != 1e9`，`0x3d7685 cmp` + `0x3d768b setne`）
//   回放播放器 `GraphwarReplayPlayer`（`graphwar_replay_player.rs`）：
//     +0x58 u32 = **播放头 tick**（`process` 每帧从时基镜像过来）
//     +0x08 usize = 事件游标；+0x18 ptr / +0x20 len = 事件表（步长 0x50，`event.tick` = u32 @ +0x48）
//     +0x30 ptr / +0x38 len = 检查点 tick 表（`go_to_*_checkpoint` 二分查找）
//
// 本探针只**读内存 + 数调用**，不写目标进程一个字节（`Interceptor.attach` 且 onLeave 里不 setRetval）。
//
// 口径：地址一律 **ELF 虚地址（RVA）**，下断 = base + RVA（Ghidra 地址 = RVA + 0x100000）。
// 时间源：`clock_gettime(CLOCK_MONOTONIC)`，µs。
'use strict';

// ── 下钩的地址（身份见 docs/06 §63 的静态表；模块归属用 `panic::Location` 反查）──
const HOOKS = {
  tick:      { rva: 0x519840, name: 'TickHandler::tick' },              // 时基推进（rdi = &handler）
  gtt:       { rva: 0x519450, name: 'TickHandler::get_time_to_tick' },  // 判决点（rdi = &handler）
  playerproc:{ rva: 0x3d7380, name: 'ReplayPlayer::process' },          // 每帧（rdi = 播放器, rsi = host）
  inject:    { rva: 0x3d61a0, name: 'ReplayPlayer::inject_events' },    // 批量注入（跳转/载入）
  toggle:    { rva: 0x3d6080, name: 'ReplayPlayer::toggle_pause' },     // ▶/⏸
  pause:     { rva: 0x3d7270, name: 'ReplayPlayer::pause' },            // 只 pause
  ispaused:  { rva: 0x3d7630, name: 'ReplayPlayer::is_paused' },        // 读暂停标记
  isp_load:  { rva: 0x3d7674, name: 'is_paused:load_nanos' },           // `mov ebp,[rbx+0x40]` 那一刻
  next_ckpt: { rva: 0x3d68b0, name: 'ReplayPlayer::go_to_next_checkpoint' },    // ⏭
  first_ckpt:{ rva: 0x3d6a40, name: 'ReplayPlayer::go_to_first_checkpoint' },   // 载入 / ⏮到底
  prev_ckpt: { rva: 0x3d6bb0, name: 'ReplayPlayer::go_to_previous_checkpoint' },// ⏮
  screen:    { rva: 0x4c6f40, name: 'ReplayScreenControl::process' },   // 回放屏每帧
  gameproc:  { rva: 0x518400, name: 'GameClient::process' },            // 对局时基循环（§44 的正对照）
};

const CFG = { sampleMs: 100, hookLogMax: 400 };
let mod = null, clock_gettime = null, tsbuf = null, startedUs = 0;
let clockPtr = null;                    // `GraphwarTickHandler*`（从 tick/gtt 的 rdi 抓）
let samples = [];                       // [{t_ms, tick, p1, p2, p3, p40}]
let counts = {};
let events = [];                        // 结构化事件（调用/暂停切换/跳转）

function bump(k, n) { counts[k] = (counts[k] || 0) + (n || 1); }
function push(ev) { if (events.length < 4000) events.push(ev); }

function findExport(name) {
  try { if (typeof Module.getGlobalExportByName === 'function') return Module.getGlobalExportByName(name); } catch (e) {}
  try { if (typeof Module.getExportByName === 'function') return Module.getExportByName(null, name); } catch (e) {}
  try { return Process.getModuleByName('libc.so.6').getExportByName(name); } catch (e) {}
  return null;
}
function nowUs() {
  if (!clock_gettime) return Date.now() * 1000;
  clock_gettime(1 /* CLOCK_MONOTONIC */, tsbuf);
  return tsbuf.readU64(0).toNumber() * 1000000 + Math.floor(tsbuf.readU64(8).toNumber() / 1000);
}
function tMs() { return Math.round((nowUs() - startedUs) / 1000); }
function hex(a) { return a ? a.toString() : null; }
function rvaAddr(a) {
  if (!a || !mod) return null;
  const r = a.sub(mod.base);
  if (r.compare(0) >= 0 && r.compare(mod.size) < 0) return '0x' + r.toString(16);
  return a.toString();
}

// ── 时基（`GraphwarTickHandler`）读数：全走 rdi = &handler 的裸偏移 ─────────────────
function readClock(p) {
  if (!p || p.isNull()) return null;
  try {
    return {
      p1s: p.add(0x00).readU64().toNumber(),      // pair#1 secs
      p1n: p.add(0x08).readU32(),                 // pair#1 nanos
      p2s: p.add(0x10).readU64().toNumber(),      // pair#2 secs（排程累加）
      p2n: p.add(0x18).readU32(),                 // pair#2 nanos（每次 tick +16_666_666）
      p3s: p.add(0x20).readU64().toNumber(),      // pair#3 secs（暂停标记）
      p3n: p.add(0x28).readU32(),                 // pair#3 nanos（==1e9 ⇒ 没暂停）
      tick: p.add(0x40).readU32(),                // ★ tick 计数器
    };
  } catch (e) { return { err: '' + e }; }
}

function sample(reason) {
  const c = readClock(clockPtr);
  if (!c) return null;
  const s = { t_ms: tMs(), reason: reason || '', tick: c.tick, p2s: c.p2s, p2n: c.p2n,
              p3s: c.p3s, p3n: c.p3n, p1s: c.p1s, p1n: c.p1n };
  samples.push(s);
  return s;
}

// ── 装钩子 ────────────────────────────────────────────────────────────────────
function install() {
  const addr = {};
  for (const k in HOOKS) {
    addr[k] = mod.base.add(HOOKS[k].rva);
    send({ type: 'hook', key: k, name: HOOKS[k].name, rva: '0x' + HOOKS[k].rva.toString(16) });
  }

  // ① 时基推进：每次 tick 一次（60/s）—— 抓 clock 指针 + 计数 + 首次若干条明细
  Interceptor.attach(addr.tick, {
    onEnter() {
      const p = this.context.rdi;
      if (!clockPtr || clockPtr.isNull()) { clockPtr = p; send({ type: 'clock_ptr', ptr: hex(p), at: 'tick' }); }
      bump('tick');
      if (counts.tick <= CFG.hookLogMax) {
        const c = readClock(p);
        push({ ev: 'tick', t_ms: tMs(), tick: c && c.tick, p2n: c && c.p2n, p3n: c && c.p3n });
      }
    },
  });

  // ② 判决点：get_time_to_tick（一帧 + 每个 tick 各一次；只抓指针/计数，别刷爆）
  Interceptor.attach(addr.gtt, {
    onEnter() {
      const p = this.context.rdi;
      if (!clockPtr || clockPtr.isNull()) { clockPtr = p; send({ type: 'clock_ptr', ptr: hex(p), at: 'gtt' }); }
      bump('gtt');
    },
  });

  // ③ 回放播放器每帧：self+0x58 = 播放头（镜像），self+0x00 tag / +0x08 游标 / +0x20 事件数
  Interceptor.attach(addr.playerproc, {
    onEnter() {
      bump('playerproc');
      if (counts.playerproc <= CFG.hookLogMax) {
        const self = this.context.rdi;
        let o = { ev: 'playerproc', t_ms: tMs() };
        try {
          o.tag = self.readU8();
          o.cursor = self.add(0x08).readU64().toNumber();
          o.ntick = self.add(0x58).readU32();
          o.nevents = self.add(0x20).readU64().toNumber();
        } catch (e) { o.err = '' + e; }
        push(o);
      }
    },
  });

  // ④ 批量注入（跳转 / 载入）：rsi = 起始事件下标，edx = until_tick
  Interceptor.attach(addr.inject, {
    onEnter() {
      bump('inject');
      const self = this.context.rdi;
      let o = { ev: 'inject', t_ms: tMs() };
      try {
        o.caller = rvaAddr(this.returnAddress);
        o.start = this.context.rsi.toUInt32();
        o.until_tick = this.context.rdx.toUInt32();
        o.ntick_before = self.add(0x58).readU32();
        o.nevents = self.add(0x20).readU64().toNumber();
      } catch (e) { o.err = '' + e; }
      push(o);
    },
    onLeave() { push({ ev: 'inject_ret', t_ms: tMs() }); },
  });

  // ⑤ ▶/⏸ 与 pause：前后各采一次时基（暂停标记就是在这里翻的）
  for (const k of ['toggle', 'pause']) {
    Interceptor.attach(addr[k], {
      onEnter() {
        bump(k);
        push({ ev: k + '_enter', t_ms: tMs(), caller: rvaAddr(this.returnAddress),
               clock: readClock(clockPtr) });
        sample(k + '_enter');
      },
      onLeave() {
        push({ ev: k + '_leave', t_ms: tMs(), clock: readClock(clockPtr) });
        sample(k + '_leave');
      },
    });
  }

  // ⑥ is_paused：入口采一次；关键在于 `mov ebp,[rbx+0x40]` 那条指令（rbx = 借到的时基）
  Interceptor.attach(addr.ispaused, {
    onEnter() { bump('is_paused'); push({ ev: 'is_paused_enter', t_ms: tMs(), clock: readClock(clockPtr) }); },
    onLeave(ret) {
      push({ ev: 'is_paused_ret', t_ms: tMs(), ret: ret.toInt32() & 0xff });
    },
  });
  Interceptor.attach(addr.isp_load, {
    onEnter() {
      const rbx = this.context.rbx;
      let o = { ev: 'is_paused_load', t_ms: tMs() };
      try {
        o.nanos_p3 = rbx.add(0x40).readU32();       // 时基 +0x28
        o.paused_by_static_rule = (o.nanos_p3 !== 1000000000);
      } catch (e) { o.err = '' + e; }
      push(o);
    },
  });

  // ⑥b ⏮/⏭/首检查点：入口读播放头/检查点表，出口读播放头（跳一次多少 tick）
  for (const k of ['next_ckpt', 'first_ckpt', 'prev_ckpt']) {
    Interceptor.attach(addr[k], {
      onEnter() {
        bump(k);
        const self = this.context.rdi;
        let o = { ev: k + '_enter', t_ms: tMs(), caller: rvaAddr(this.returnAddress) };
        try {
          o.ntick = self.add(0x58).readU32();
          o.cursor = self.add(0x08).readU64().toNumber();
          const cp = self.add(0x30).readPointer();
          const nc = self.add(0x38).readU64().toNumber();
          o.n_checkpoints = nc;
          if (nc > 0 && !cp.isNull()) {
            const arr = [];
            for (let i = 0; i < Math.min(nc, 8); i++) arr.push(cp.add(i * 4).readU32());
            o.checkpoints = arr;
          }
        } catch (e) { o.err = '' + e; }
        push(o);
      },
      onLeave() {
        const self = this.context.rdi;
        let o = { ev: k + '_leave', t_ms: tMs() };
        try { o.ntick = self.add(0x58).readU32(); o.clock = readClock(clockPtr); } catch (e) { o.err = '' + e; }
        push(o);
        sample(k + '_leave');
      },
    });
  }

  // ⑦ 回放屏 / 对局循环：只计数（判别"现在是不是在回放屏"）
  Interceptor.attach(addr.screen, { onEnter() { bump('screen'); } });
  Interceptor.attach(addr.gameproc, { onEnter() { bump('gameproc'); } });
}

let iv = null;
function armTimer() {
  if (iv) { clearInterval(iv); iv = null; }
  iv = setInterval(() => { try { sample('timer'); } catch (e) {} }, CFG.sampleMs);
}

function start() {
  const m = Process.findModuleByName('libgraphwarrust.so');
  if (!m) { send({ type: 'error', msg: 'libgraphwarrust.so not loaded' }); return; }
  mod = m;
  const cg = findExport('clock_gettime');
  if (cg) {
    clock_gettime = new NativeFunction(cg, 'int', ['int', 'pointer'], 'default');
    tsbuf = Memory.alloc(16);
  }
  startedUs = nowUs();
  install();
  send({ type: 'ready', base: m.base.toString(), size: m.size, path: m.path,
         time_src: clock_gettime ? 'clock_gettime(CLOCK_MONOTONIC)' : 'Date.now()(ms 降级)' });
  armTimer();
}

rpc.exports = {
  cfg(o) { Object.assign(CFG, o || {}); armTimer(); return CFG; },
  mark(label) {
    const s = sample('mark:' + label);
    push({ ev: 'mark', label: label, t_ms: tMs(), clock: readClock(clockPtr), sample: s });
    send({ type: 'mark', label: label, t_ms: tMs() });
    return sample ? sample.tick : null;
  },
  snap(reason) { return sample(reason || 'rpc'); },
  dump() {
    return { counts: counts, clock_ptr: hex(clockPtr), samples: samples, events: events, t_ms: tMs() };
  },
  now() { return tMs(); },
};

setTimeout(start, 0);
