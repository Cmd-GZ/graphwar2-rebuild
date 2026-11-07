/**
 * tools/obstacle_room.js —— 抓「挖洞」（`QueueDestroyObstacle` 42 / `DestroyObstacle` 41）的**生产侧**
 * 原始记录（`dispatch/27`，P1：造一个带障碍的对局，收「同坐标不同序号」样本）。
 *
 * ── 地址口径（**必须带版本**，见 AGENTS.md §0）─────────────────────────────
 *   构建：Graphwar II Linux 2.0.60（Steam）
 *         libgraphwarrust.so sha256 9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187
 *         （= 逆向目标那份，逐字节相同）
 *   地址：**ELF 虚地址 / RVA**，下断一律 `base + rva`；Ghidra 地址 = RVA + 0x100000。
 *
 *   RVA        函数（来源）
 *   0x3c2bc0   `GraphwarShotCalculator::calculate_shot_results`
 *              （`tools/addr_probe.py fde 0x3c2bc0` → [0x3c2bc0,0x3c5c4d)；sret：`mov [rsp+0x1b8],rdi`
 *               ⇒ 第 1 实参 = 返回 `Vec<Record0x20>` 的**隐式返回指针**，第 2 实参 = 那发的结构体）
 *   0x3f20f0   `GraphwarObstacleManager::queue_destroy_obstacle`
 *              签名（逐指令核过，`tools/addr_probe.py dis 0x3f20f0`）：
 *                rdi = &mut self, rsi = &(f64 x, f64 y, u64 (field:u32, idx:u32)), edx = tick:u32,
 *                xmm0 = radius:f64
 *   0x3f1b30   `GraphwarObstacleManager::destroy_obstacle`
 *              签名：rdi = &mut self, rsi = &(f64 x, f64 y), xmm0 = radius:f64
 *              （`0x3f1b93 movups xmm0,[rsi]` / `0x3f1bc4…` / `0x3f1c09 add r13,0x2a` = push tag 41）
 *   0x4724cc   `execute_results` 体内"开火队列预扫"第一条指令（`mov 0x40(%rbp),%rsi`）
 *              rbp = GSM；队列 = {ptr @ +0x38, ? @ +0x40, ? @ +0x48}，元素步长 **0x20**
 *              （见 docs/06 §35.3；这里三个 u64 全 dump，长度/容量按合理性在宿主侧判）
 *   0x3c38e4   `calculate_shot_results` 体内**写挖洞指令**的第一条指令
 *              （0x3c38d7 MAX→inc→tag 0x8000…0 / +0x08 = (x,y) / +0x18 = field / +0x1c = idx）
 *
 * ── 抓什么 ────────────────────────────────────────────────────────────────
 *   `queue_destroy`   : 每一次"排一次挖洞"的**全部实参位模式**（x,y,radius,tick + u64 两个半字）
 *   `destroy_wire`    : 每一次 `destroy_obstacle`（= 发一条 tag 41）的 (x,y,radius) 位模式
 *   `shot_calc_ret`   : `calculate_shot_results` 的**返回值 Vec** 逐条 0x20 字节记录（落盘前不解释）
 *   `exec_queue`      : 事件队列原文（kind + 两个 u64 + +0x08 指针），
 *                       并把"像点数组"的项做**重复点扫描**（同一 (x,y) 位模式出现在哪几个下标）
 *
 * 只**观测**：不改内存、不改逻辑、不打补丁。
 */
'use strict';

const CFG = {
  rva: {
    shot_calc: 0x3c2bc0,
    queue_destroy: 0x3f20f0,
    destroy_obstacle: 0x3f1b30,
    exec_queue_probe: 0x4724cc,
    destroy_write: 0x3c38e4,
    draw_circle: 0x3f11e0,      // GraphwarObstacleData::draw_circle(data,?,color, x=xmm0, y=xmm1, r=xmm2)
    tick_bucket: 0x3f1d99,      // handle_tick_event 体内：rax = 这个 tick 桶里的条目数（rdi 已存入 rbx=self）
  },
  maxPointsScan: 40000,     // 点数组重复扫描的上限（超过就只报 len）
  maxQueueItems: 4096,
};

let BASE = null;
const T0 = Date.now();
const now = () => Date.now() - T0;

function hx(v) { try { return v === null || v === undefined ? null : v.toString(16); } catch (e) { return null; } }
function bytesHex(p, n) {
  try {
    const b = new Uint8Array(p.readByteArray(n));
    let s = '';
    for (let i = 0; i < b.length; i++) s += ('0' + b[i].toString(16)).slice(-2);
    return s;
  } catch (e) { return null; }
}
/** f64 的小端位串（16 位十六进制 = 一个 f64） */
function f64b(p, off) { return bytesHex(p.add(off), 8); }
/** xmm 寄存器（16 字节）→ 低 8 字节位串 */
function xmmBits(v) {
  try {
    const b = new Uint8Array(v);
    let s = '';
    for (let i = 0; i < 8; i++) s += ('0' + b[i].toString(16)).slice(-2);
    return s;
  } catch (e) { return null; }
}
function modOf(a) { try { const m = Process.findModuleByAddress(a); return m ? m.name : null; } catch (e) { return null; } }
function rvaOf(a) {
  try {
    const m = Process.findModuleByAddress(a);
    if (!m) return null;
    if (BASE && m.base.equals(BASE)) return 'gw+' + hx(a.sub(BASE));
    return m.name + '+' + hx(a.sub(m.base));
  } catch (e) { return null; }
}
function btOf(ctx, d) {
  try { return Thread.backtrace(ctx, Backtracer.FUZZY).slice(0, d || 8).map(rvaOf); } catch (e) { return ['ERR:' + e]; }
}
/** 逐位等价：位串 → 可比较的十进制（只用于去重/聚合，不改数值） */
function lebToBig(hexLE) {
  if (!hexLE || hexLE.length !== 16) return null;
  let v = 0n;
  for (let i = 14; i >= 0; i -= 2) v = (v << 8n) | BigInt(parseInt(hexLE.substr(i, 2), 16));
  return v;
}

const ST = { ins: false, nShotCalc: 0, nQueue: 0, nWire: 0, nQ: 0 };

/**
 * 点数组的**重复点扫描**（在进程内做，避免把几十万点搬回宿主）：
 * 用 (x 位串 | y 位串) 做键，报出所有"同一坐标出现在 ≥2 个下标"的分组。
 * 这是 §31.5 第 1 条那条未知（"两个不同点序号上坐标为何逐位相同"）的正面探针。
 */
function dupScan(p, len) {
  const res = { n: len, groups: [], dupCount: 0, stepX: null };
  if (!p || p.isNull() || !(len > 0) || len > CFG.maxPointsScan) return { n: len, skipped: true };
  try {
    const first = new Map();
    const groups = new Map();
    let steps = 0, hist = {};
    let prevx = null;
    for (let i = 0; i < len; i++) {
      const xb = bytesHex(p.add(i * 16), 8), yb = bytesHex(p.add(i * 16 + 8), 8);
      const k = xb + '|' + yb;
      if (first.has(k)) {
        const g = groups.get(k) || [first.get(k)];
        g.push(i);
        groups.set(k, g);
      } else first.set(k, i);
      if (prevx !== null && i < 6) {
        // x 步长（位模式差，用 BigInt 只做展示）
        const a = lebToBig(prevx), b = lebToBig(xb);
        if (a !== null && b !== null) {
          const dBits = (b > a) ? b - a : a - b;
          hist[dBits.toString(16)] = (hist[dBits.toString(16)] || 0) + 1;
        }
        steps++;
      }
      prevx = xb;
    }
    for (const [k, g] of groups) { res.groups.push({ key: k, idx: g }); res.dupCount += g.length; }
    if (res.groups.length > 64) res.groups = res.groups.slice(0, 64);
    res.stepHint = hist;
  } catch (e) { res.error = String(e); }
  return res;
}

function flushNothing() { }

function install(base) {
  BASE = base;
  const mod = Process.findModuleByAddress(base);
  const res = { type: 'hooks', base: hx(base), size: mod.size, hooks: [], sha: '9a538115' };

  function hook(rva, h) {
    try {
      Interceptor.attach(base.add(rva), h);
      res.hooks.push({ rva: '0x' + rva.toString(16), ok: true });
    } catch (e) { res.hooks.push({ rva: '0x' + rva.toString(16), error: String(e) }); }
  }

  // ── ① 排一次挖洞（tag 42 的生产者实参） ────────────────────────────────
  hook(CFG.rva.queue_destroy, {
    onEnter(args) {
      const o = { type: 'queue_destroy', seq: ++ST.nQueue, t: now(), self: hx(args[0]), rec: hx(args[1]),
                  selfMod: modOf(args[0]), selfOff: rvaOf(args[0]) };
      try {
        o.x = f64b(args[1], 0); o.y = f64b(args[1], 8);
        o.u64 = bytesHex(args[1].add(16), 8);
        o.field = args[1].add(16).readU32();
        o.idx = args[1].add(20).readU32();
      } catch (e) { o.err = String(e); }
      o.tick = this.context.rdx.toUInt32();
      o.tickPtr = hx(args[2]);
      o.radius = xmmBits(this.context.xmm0);
      o.bt = btOf(this.context, 8);
      send(o);
    },
  });

  // ── ② 发一条 tag 41（DestroyObstacle） ─────────────────────────────────
  hook(CFG.rva.destroy_obstacle, {
    onEnter(args) {
      const o = { type: 'destroy_wire', seq: ++ST.nWire, t: now(), self: hx(args[0]), rec: hx(args[1]),
                  selfMod: modOf(args[0]), selfOff: rvaOf(args[0]) };
      try { o.x = f64b(args[1], 0); o.y = f64b(args[1], 8); } catch (e) { o.err = String(e); }
      o.radius = xmmBits(this.context.xmm0);
      o.bt = btOf(this.context, 8);
      send(o);
    },
  });

  // ── ③ 写挖洞指令的那条指令（拿到写入 **之前** 的现场：x,y 与 (field,idx)） ──
  hook(CFG.rva.destroy_write, {
    onEnter() {
      const o = { type: 'destroy_write', t: now() };
      const c = this.context;
      const dst = c.rax.add(c.rcx);
      try {
        o.dst = hx(dst);
        o.x = bytesHex(c.rsp.add(0x70), 8);
        o.y = bytesHex(c.rsp.add(0x78), 8);
        o.field = c.r14.toString();
        o.idxDword = c.rsp.add(0x38).readU32();
        o.cursor = bytesHex(c.rsp.add(0x38), 8);
        o.r13 = hx(c.r13);
        o.r15 = hx(c.r15);
      } catch (e) { o.err = String(e); }
      o.bt = btOf(c, 8);
      send(o);
    },
  });

  // ── ③b 掩膜画圆：建障碍 / 挖洞都走这里（区分靠 edx 的颜色） ──────────────
  hook(CFG.rva.draw_circle, {
    onEnter(args) {
      const o = { type: 'draw_circle', t: now(), data: hx(args[0]), rsi: hx(args[1]),
                  color: this.context.rdx.toUInt32(), ret: rvaOf(this.context.rsp.readPointer()) };
      o.x = xmmBits(this.context.xmm0);
      o.y = xmmBits(this.context.xmm1);
      o.r = xmmBits(this.context.xmm2);
      send(o);
    },
  });

  // ── ③c 到期 tick 的桶里**有几条**（一条 = 一条 tag 41 报文） ─────────────
  hook(CFG.rva.tick_bucket, {
    onEnter() {
      const c = this.context;
      let n = null;
      try { n = Number(c.rax); } catch (e) { }
      send({ type: 'tick_bucket', t: now(), n: n, self: hx(c.rbx),
             tickKey: c.rsp.add(0xc).readU32(),
             bt: btOf(c, 6) });
    },
  });

  // ── ④ calculate_shot_results 的返回值（整条结果 Vec） ─────────────────
  hook(CFG.rva.shot_calc, {
    onEnter(args) {
      this.seq = ++ST.nShotCalc;
      this.sret = args[0];
      this.arg1 = args[1];
      this.ret = rvaOf(this.context.rsp.readPointer());
      send({ type: 'shot_calc_enter', seq: this.seq, t: now(), sret: hx(this.sret), arg1: hx(this.arg1), ret: this.ret });
    },
    onLeave(retval) {
      const o = { type: 'shot_calc_ret', seq: this.seq, t: now(), sret: hx(this.sret), ret: this.ret, recs: [] };
      try {
        // sret = &Vec<Record0x20>；Rust Vec=(ptr, cap, len) —— 两种解释都读，宿主侧按合理性判
        const v = this.sret;
        o.vec = { a: hx(v.readPointer()), b: v.add(8).readU64().toString(), c: v.add(16).readU64().toString() };
        // rustc 会重排 Vec 的字段：实测这里 (cap, ptr, len)，而队列那边是 (ptr, len, cap)。
        // 所以**不假设顺序**：三个 u64 里"像堆指针"的那个当 ptr，剩两个取 min=len / max=cap。
        const f0 = v.readU64(), f1 = v.add(8).readU64(), f2 = v.add(16).readU64();
        const cand = [[0, f0], [1, f1], [2, f2]];
        let ptrOff = -1;
        for (const [i, val] of cand) {
          const n = Number(val);
          if (val > 0x100000000n && val < 0x7fffffffffff) { ptrOff = i; break; }
        }
        let ptr0 = ptr.zero, len = null, cap = null;
        if (ptrOff >= 0) {
          ptr0 = v.add(ptrOff * 8).readPointer();
          const rest = cand.filter((x) => x[0] !== ptrOff).map((x) => Number(x[1]));
          len = Math.min(rest[0], rest[1]); cap = Math.max(rest[0], rest[1]);
          if (!(len >= 0 && len <= 4096)) { len = null; }
        }
        o.ptrOff = ptrOff; o.len = len; o.cap = cap;
        if (len !== null && !ptr0.isNull() && len > 0) {
          for (let i = 0; i < len && i < 512; i++) {
            const r = ptr0.add(i * 0x20);
            o.recs.push(bytesHex(r, 0x20));
          }
        }
      } catch (e) { o.err = String(e); }
      send(o);
    },
  });

  // ── ⑤ execute_results 里的开火队列（含挖洞记录 + 点数组） ──────────────
  hook(CFG.rva.exec_queue_probe, {
    onEnter() {
      const self = this.context.rbp;
      const o = { type: 'exec_queue', seq: ++ST.nQ, t: now(), self: hx(self), items: [] };
      let qptr = null;
      try {
        qptr = self.add(0x38).readPointer();
        o.qraw = bytesHex(self.add(0x30), 0x28);
        o.u40 = self.add(0x40).readU64().toString();
        o.u48 = self.add(0x48).readU64().toString();
      } catch (e) { o.err = String(e); }
      // 长度 = 两个 u64 里"元素个数"那个：优先 +0x40（§35.3 记的 len 位置），另一个当 cap
      let len = null;
      try {
        const a = Number(self.add(0x40).readU64()), b = Number(self.add(0x48).readU64());
        if (a >= 0 && a <= CFG.maxQueueItems && (a <= b || b === 0)) len = a;
        else if (b >= 0 && b <= CFG.maxQueueItems) len = b;
      } catch (e) { }
      o.qlen = len; o.qptr = hx(qptr);
      if (qptr && !qptr.isNull() && len) {
        for (let i = 0; i < len; i++) {
          const it = qptr.add(i * 0x20);
          const rec = { i: i, raw: bytesHex(it, 0x20) };
          try {
            rec.kind = it.readU64().toString();
            rec.q8 = it.add(8).readU64().toString();
            rec.q10 = it.add(0x10).readU64().toString();
            rec.q18 = it.add(0x18).readU64().toString();
            // 非 tag 记录（kind 小整数，如 0x1000/0x2000）= 点数组项：ptr@+8, len@+0x10
            if (rec.kind === '4096' || rec.kind === '8192' || (Number(rec.kind) > 0 && Number(rec.kind) < 0x10000)) {
              const p = it.add(8).readPointer();
              const n = Number(it.add(0x10).readU64());
              const head = [], tail = [];
              try {
                for (let k = 0; k < Math.min(6, n); k++) head.push([bytesHex(p.add(k * 16), 8), bytesHex(p.add(k * 16 + 8), 8)]);
                for (let k = Math.max(0, n - 6); k < n; k++) tail.push([k, bytesHex(p.add(k * 16), 8), bytesHex(p.add(k * 16 + 8), 8)]);
              } catch (e) { }
              rec.pts = { ptr: hx(p), len: n, head: head, tail: tail, dup: dupScan(p, n) };
            }
            if (rec.kind === '9223372036854775808') {           // 0x8000…0：挖洞指令
              rec.x = bytesHex(it.add(8), 8);
              rec.y = bytesHex(it.add(0x10), 8);
              rec.field = it.add(0x18).readU32();
              rec.idx = it.add(0x1c).readU32();
            }
          } catch (e) { rec.err = String(e); }
          o.items.push(rec);
        }
      }
      send(o);
    },
  });

  ST.ins = true;
  send(res);
}

setInterval(() => {
  if (ST.ins) return;
  let m = null;
  try { m = Process.enumerateModules().find((x) => /graphwarrust/i.test(x.name)); } catch (e) { }
  if (!m) return;
  try { install(m.base); } catch (e) { ST.ins = true; send({ type: 'err', msg: String(e) }); }
}, 50);

setInterval(() => {
  send({ type: 'beat', t: now(), ins: ST.ins, shotCalc: ST.nShotCalc, queueDestroy: ST.nQueue, wire: ST.nWire, execQueues: ST.nQ });
}, 3000);
