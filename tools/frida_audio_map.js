// tools/frida_audio_map.js —— 音效 ↔ 事件映射的**真机读数**探针（T19 / `task-16`，**只读**）
//
// 目标：把 `docs/17` §5.1.1 挂着的那条「音效 ↔ 事件映射 = 未知」变成一张有运行时证据的表。
// 预注册（跑之前写死）：`fixtures/audio_event_map_plan.txt`。
//
// ## 这个探针只做三件事
//   ① **资源身份**：`ResourceLoader::load`(`0x4b1da0`) 抓「路径字面量 → 资源对象句柄」；
//      `SoundHandler` 构造收尾(`0x5238c0`) 抓「载荷结构体 → 最终 handler」，两者接起来
//      才知道 `+0x80/+0x90/+0xa0/+0xb0/+0xc0` 各装哪个 mp3。**只认指针相等，不认名字像**。
//   ② **事件身份**：4 条订阅的 `dyn FnMut` 胶合 shim(`0x4bf400/0x4bf420/0x4bf440/0x4bf460`)
//      与 4 个处理方法(`0x4bf090/0x4bf340/0x4bf480/0x4bef20`) 的入口，读 payload 的
//      tag / `+0x1c` u32 / `+0x20` u8，并抓**调用者的返回地址**（= 事件派发方）。
//   ③ **播放时刻**：`AudioStreamPlayer::play`(`0x8ae4c0`) 每次调用 + 当前 tick。
//      tick 读 `GraphwarTickHandler[+0x40]`（`0x519840` 的 `rdi` 缓存下来），
//      不读墙钟 —— 软渲染下帧率会抖，墙钟不能当对齐轴。
//
// ## 纪律
//   * `Interceptor.attach` + onEnter/onLeave **只读**：不 setRetval、不写目标进程任何字节；
//   * 地址口径 = **ELF vaddr（RVA）**，下断 = base + RVA（Ghidra = RVA + 0x100000）；
//   * 高频函数（`process` `0x4bfe10` 每帧、`tick` 60/s）**只计数/只更新 tick**，不逐次入库。
'use strict';

// ── 下钩表（身份与 RVA 的出处见 fixtures/audio_event_map_plan.txt §4）────────────────
const HOOKS = {
  // ① 资源
  load:  { rva: 0x4b1da0, name: 'ResourceLoader::load(binding)',  kind: 'pair' },
  ctor:  { rva: 0x5238c0, name: 'SoundHandler ctor/finish',       kind: 'ctor' },
  hummrd:{ rva: 0x3eaa00, name: 'FileAccess::get_file_as_bytes',  kind: 'read' },
  // ② 装 stream / 播
  eaud:  { rva: 0x4bed70, name: 'EffectAudioData::new',           kind: 'eaud' },
  eaudv: { rva: 0x4bed81, name: 'EffectAudioData::new(+0x11) vol',kind: 'eaudv' },
  sstr:  { rva: 0x3ebb80, name: 'AudioStreamPlayer::set_stream A',kind: 'two' },
  sstr2: { rva: 0x3eb8b0, name: 'AudioStreamPlayer::set_stream B',kind: 'two' },
  play:  { rva: 0x8ae4c0, name: 'AudioStreamPlayer::play',        kind: 'play' },
  qslot: { rva: 0x41d060, name: 'get stream handle from self+slot',kind: 'qslot' },
  // ③ 事件侧：4 个处理方法
  eff:   { rva: 0x4bf090, name: 'effect_info_event',              kind: 'handler', slot: 0 },
  sold:  { rva: 0x4bf340, name: 'soldier_info_event',             kind: 'handler', slot: 1 },
  gst:   { rva: 0x4bef20, name: 'game_state_event',               kind: 'handler', slot: 2 },
  entrm: { rva: 0x4bf480, name: 'entity_removed_event',           kind: 'handler', slot: 3 },
  // ③b 4 条订阅的 `dyn FnMut` 胶合 shim（`lea` 出上面 4 个真函数）
  w_eff:  { rva: 0x4bf400, name: 'closure_shim(effect_info_event)',   kind: 'wrap' },
  w_sold: { rva: 0x4bf420, name: 'closure_shim(soldier_info_event)',  kind: 'wrap' },
  w_gst:  { rva: 0x4bf440, name: 'closure_shim(game_state_info_event)', kind: 'wrap' },
  w_entrm:{ rva: 0x4bf460, name: 'closure_shim(entity_removed_event)', kind: 'wrap' },
  // ④ 时钟 / 每帧
  tick:  { rva: 0x519840, name: 'TickHandler::tick',               kind: 'tick' },
  proc:  { rva: 0x4bfe10, name: 'SoundHandler::process',           kind: 'proc' },
};

const CFG = { logMax: 4000, btDepth: 8, procSampleMax: 3 };
let mod = null, clockPtr = null, curTick = -1, lastSelf = null;
let startedUs = 0, counts = {}, events = [], nSeq = 0;
let btCache = new Map();

function bump(k) { counts[k] = (counts[k] || 0) + 1; }
function push(o) { if (events.length < CFG.logMax * 4) { o.seq = ++nSeq; events.push(o); } }
function nowUs() { return Date.now() * 1000; }
function tMs() { return Math.round((nowUs() - startedUs) / 1000); }
function hx(a) { try { return a ? '0x' + a.toString(16) : null; } catch (e) { return null; } }

function rva(a) {
  try {
    if (!a || !mod) return null;
    const r = a.sub(mod.base);
    if (r.compare(0) >= 0 && r.compare(mod.size) < 0) return '0x' + r.toString(16);
    return '0x' + a.toString(16);
  } catch (e) { return null; }
}
function rvaNum(a) {
  try {
    if (!a || !mod) return null;
    const r = a.sub(mod.base);
    if (r.compare(0) >= 0 && r.compare(mod.size) < 0) return parseInt('0x' + r.toString(16), 16);
    return null;
  } catch (e) { return null; }
}

// 读一个「可能是句柄」的 8 字节；顺便把解引用后的值也读一下（不猜，原样记录）
function w64(p) { try { return hx(p.readU64()); } catch (e) { return null; } }
function wu32(p, off) { try { return p.add(off).readU32(); } catch (e) { return null; } }
function wu8(p, off) { try { return p.add(off).readU8(); } catch (e) { return null; } }
function rdstr(p, len) {
  try {
    const n = Math.min(len | 0, 256);
    if (!n || n <= 0) return null;
    const bytes = new Uint8Array(p.readByteArray(n));
    let s = '';
    for (let i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i]);
    return s;
  } catch (e) { return null; }
}
function win(p, n) {          // 读 n 字节的**原始十六进制**（读不到就 null）
  try { const b = new Uint8Array(p.readByteArray(n)); let s = ''; for (let i = 0; i < b.length; i++) s += (b[i] < 16 ? '0' : '') + b[i].toString(16); return s; }
  catch (e) { return null; }
}
function backtrace(ctx) {
  try {
    const key = rva(ctx.rip) || '';
    if (btCache.has(key)) return btCache.get(key);
    const bt = Thread.backtrace(ctx, Backtracer.ACCURATE).slice(0, CFG.btDepth).map(rva);
    btCache.set(key, bt);
    return bt;
  } catch (e) { return null; }
}
function clock() { return { tick: curTick, clock_ptr: hx(clockPtr) }; }

// ── 装钩子 ────────────────────────────────────────────────────────────────────
function install() {
  const A = {};
  for (const k in HOOKS) {
    A[k] = mod.base.add(HOOKS[k].rva);
    send({ type: 'hook', key: k, name: HOOKS[k].name, rva: '0x' + HOOKS[k].rva.toString(16) });
  }

  // ① load：路径字面量（无 NUL，必须按长度截断）+ 返回句柄
  Interceptor.attach(A.load, {
    onEnter() {
      bump('load');
      this.path = rdstr(this.context.rdi, this.context.rsi.toInt32());
      this.plen = this.context.rsi.toInt32();
    },
    onLeave(ret) {
      push(Object.assign({ ev: 'load', path: this.path, len: this.plen,
                           han_lo: hx(ret), han_hi: hx(this.context.rdx) }, clock()));
    },
  });

  // ①b humm.wav 走 FileAccess（不是 ResourceLoader）
  Interceptor.attach(A.hummrd, {
    onEnter() {
      bump('humm_read');
      push(Object.assign({ ev: 'humm_read', path: rdstr(this.context.rsi, this.context.rdx.toInt32()) },
                         clock()));
    },
  });

  // ①c 构造收尾：入参 = 载荷结构体（5 个 16 字节资源对，载入序）；返回 = 最终 handler
  Interceptor.attach(A.ctor, {
    onEnter() { bump('ctor'); this.pay = win(this.context.rdi, 0x70); this.self = this.context.rdi; },
    onLeave(ret) {
      push(Object.assign({ ev: 'ctor', payload_hex: this.pay,
                           handler: hx(ret), handler_hex: win(ret, 0xd8) }, clock()));
    },
  });

  // ② EffectAudioData::new：6 个调用点唯一的收窄口
  Interceptor.attach(A.eaud, {
    onEnter() {
      bump('eaud');
      const c = this.context;
      push(Object.assign({
        ev: 'eaud', ret: rva(this.returnAddress), bt: backtrace(c),
        a: hx(c.rsi), b: hx(c.rdx), c: hx(c.rcx), d: hx(c.r8), val: c.r9.toInt32(),
        self_pair: lastSelf ? win(lastSelf.add(0x60), 0x10) : null,
      }, clock()));
    },
  });
  // ②b 音量：`sub rsp,0x78` 之后 `movss [rsp+0x1c],xmm0` 已把 xmm0 落地；在下一句读栈
  Interceptor.attach(A.eaudv, {
    onEnter() {
      bump('eaud_vol');
      const sp = this.context.rsp;
      const vol = win(sp.add(0x1c), 4);
      if (events.length && events[events.length - 1].ev === 'eaud') {
        events[events.length - 1].vol_u32 = vol;
      }
    },
  });

  // ②c set_stream 的两份绑定：把两个实参各自的 16 字节窗口原样记下
  for (const k of ['sstr', 'sstr2']) {
    Interceptor.attach(A[k], {
      onEnter() {
        bump(k);
        const c = this.context;
        push(Object.assign({ ev: k, ret: rva(this.returnAddress),
                             a: hx(c.rdi), b: hx(c.rsi),
                             a_hex: win(c.rdi, 0x10), b_hex: win(c.rsi, 0x10) }, clock()));
      },
    });
  }

  // ②d play()：★ 播放的**正面证据**
  //   队列那一路 rdi = 队列条目指针（条目 = {+0x00 Option<Gd> (16), +0x10 u32 触发 tick,
  //   +0x14 u8 已启动}，步长 0x18）；humm 那一路 rdi = 栈上 Ref 的地址。两种都原样记，
  //   读到什么算什么（不猜）。
  Interceptor.attach(A.play, {
    onEnter() {
      bump('play');
      const c = this.context;
      let o = { ev: 'play', arg: hx(c.rdi), ret: rva(this.returnAddress), bt: backtrace(c) };
      try { o.arg_hex = win(c.rdi, 0x18); } catch (e) {}
      try { o.arg_v10 = c.rdi.add(0x10).readU32(); } catch (e) {}
      try { o.arg_f14 = c.rdi.add(0x14).readU8(); } catch (e) {}
      push(Object.assign(o, clock()));
    },
  });

  // ②e 从 self+slot 取资源句柄的辅助：rdi - lastSelf 就是槽偏移
  Interceptor.attach(A.qslot, {
    onEnter() { bump('qslot'); this.rdi = this.context.rdi; this.ret = rva(this.returnAddress); },
    onLeave(ret) {
      let off = null;
      try { if (lastSelf) off = '0x' + this.rdi.sub(lastSelf).toString(16); } catch (e) {}
      push(Object.assign({ ev: 'qslot', ret: this.ret, addr: hx(this.rdi), slot: off,
                           lo: hx(ret), hi: hx(this.context.rdx) }, clock()));
    },
  });

  // ③ 4 个事件处理方法：payload 的 tag / +0x1c u32 / +0x20 u8
  for (const k of ['eff', 'sold', 'gst', 'entrm']) {
    Interceptor.attach(A[k], {
      onEnter() {
        bump(k);
        const c = this.context;
        const ev = c.rsi;
        lastSelf = c.rdi;
        let o = { ev: k, self: hx(c.rdi), payload: hx(ev), ret: rva(this.returnAddress),
                  bt: backtrace(c) };
        if (ev && !ev.isNull()) {
          o.tag = w64(ev);              // 预期 `0x800000000000002c`(44) / `...1e`(30) / 无
          o.p08 = wu8(ev, 0x8);
          o.p1c = wu32(ev, 0x1c);
          o.p20 = wu8(ev, 0x20);
          o.pay_hex = win(ev, 0x30);
        }
        // handler 自己的 5 个资源槽 + `+0x60` 那个 player 对（首次抓一次就够，见 dump）
        push(Object.assign(o, clock()));
      },
    });
    // ③b 对应的胶合 shim：调用者返回地址 = 事件派发方
    const w = 'w_' + k;
    Interceptor.attach(A[w], {
      onEnter() {
        bump(w);
        const c = this.context;
        push(Object.assign({ ev: w, env: hx(c.rdi), self: (function () { try { return hx(c.rdi.readPointer()); } catch (e) { return null; } })(),
                             payload: hx(c.rsi), ret: rva(this.returnAddress),
                             bt: backtrace(c), pay_hex: win(c.rsi, 0x30) }, clock()));
      },
    });
  }

  // ④ tick：只更新 curTick（60/s，逐次入库会淹掉真数据）
  Interceptor.attach(A.tick, {
    onEnter() {
      bump('tick');
      const p = this.context.rdi;
      if (!clockPtr || clockPtr.isNull()) clockPtr = p;
      try { curTick = p.add(0x40).readU32(); } catch (e) {}
    },
  });

  // ④b process：每帧，只计数 + 记前几次的指针
  Interceptor.attach(A.proc, {
    onEnter() {
      bump('proc');
      if (counts.proc <= CFG.procSampleMax) {
        push(Object.assign({ ev: 'proc', self: hx(this.context.rdi), evptr: hx(this.context.rsi) }, clock()));
      }
    },
  });
}

let startTries = 0;
function start() {
  const m = Process.findModuleByName('libgraphwarrust.so');
  if (!m) {
    // 起进程后立刻 attach 时 .so 可能还没加载完 ⇒ 重试，别把"挂晚了"当"没调用"
    if (++startTries <= 240) { setTimeout(start, 250); return; }
    send({ type: 'error', msg: 'libgraphwarrust.so not loaded after 60s' });
    return;
  }
  mod = m;
  startedUs = nowUs();
  install();
  send({ type: 'ready', base: m.base.toString(), size: m.size, path: m.path });
  armQueue();
}

// ── 队列采样：`Vec<EffectAudioData>` 在 `self+0x00`（cap/ptr/len = +0x00/+0x08/+0x10，
//    元素 0x18 字节）。采样只为回答一件事：载入期压进去的条目**有没有被清掉**，
//    以及每条的「触发 tick」字段到底是多少。只读。
let qLast = null;
function queueSample(reason) {
  if (!lastSelf) return;
  let cap, ptr, len;
  try {
    cap = lastSelf.add(0x00).readU64();
    ptr = lastSelf.add(0x08).readPointer();
    len = lastSelf.add(0x10).readU64().toNumber();
  } catch (e) { return; }
  const ents = [];
  if (len > 0 && len < 256 && !ptr.isNull()) {
    for (let i = 0; i < Math.min(len, 12); i++) {
      const e = ptr.add(i * 0x18);
      let v = null, st = null, g = null;
      try { v = e.add(0x10).readU32(); } catch (e2) {}
      try { st = e.add(0x14).readU8(); } catch (e2) {}
      try { g = hx(e.readPointer()); } catch (e2) {}
      ents.push([v, st, g]);
    }
  }
  const sig = JSON.stringify([len, ents]);
  if (sig === qLast && reason !== 'rpc') return;       // 只在变化时入库，避免刷屏
  qLast = sig;
  push(Object.assign({ ev: 'queue', reason: reason || '', cap: hx(cap), ptr: hx(ptr), len: len,
                       ents: ents }, clock()));
}
let qiv = null;
function armQueue() { if (qiv) clearInterval(qiv); qiv = setInterval(() => { try { queueSample('timer'); } catch (e) {} }, 250); }

rpc.exports = {
  mark(label) {
    push({ ev: 'mark', label: label, t_ms: tMs(), tick: curTick });
    send({ type: 'mark', label: label, t_ms: tMs(), tick: curTick });
    return curTick;
  },
  tick() { return curTick; },
  queue() { queueSample('rpc'); return qLast; },
  dump() {
    return { counts: counts, events: events, clock_ptr: hx(clockPtr), t_ms: tMs(), tick_end: curTick };
  },
};

setTimeout(start, 0);
