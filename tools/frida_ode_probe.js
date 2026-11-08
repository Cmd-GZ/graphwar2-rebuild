/**
 * tools/frida_ode_probe.js —— 抓 **DiffEq（`Mode::Ode`）局**里一发开火的
 * 「采样器入参 → 点数组 → 报文读的那个点」全链（`dispatch/40`，P0）。
 *
 * 为什么单独一支：`tools/frida_point_array.js`（34 号）只抓 `(ptr,n)` 与两次调用的比对，
 * 不看采样器的**入参**，所以答不出「这一发到底是不是 Ode」「报文那个点是不是数组最后一点」。
 * 并行任务 39 正在改那两支共享脚本，本文件按任务书 §3 独立，**不改共享文件**。
 *
 * ── 地址对应的构建（必须随版本看）─────────────────────────────────────────────
 *   构建   Graphwar II Linux 2.0.60（Steam）
 *   二进制 .reference/game/libgraphwarrust.so
 *   sha256 9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187  ← 与逆向目标逐字节相同
 *   口径   RVA = ELF 虚地址 = 模块内偏移；下断 = base + RVA；Ghidra 地址 = RVA + 0x100000
 *
 *   RVA         是什么                                             证据
 *   0x3c2530    采样器（旧文档叫 `calculate_function`，"点数组的生产者"）  FDE [0x3c2530,0x3c2bb9)
 *   0x4eb9d0    求值器 evaluate_recursive(tokens,&mut i,x,y,dy)          FDE [0x4eb9d0,0x4ebbe0)
 *   0x3c2bc0    开火结算（内含 `call *0x9bac98` ⇒ 采样器；返回地址 gw+3c30c7）
 *   0x3e2830    另一处调用（返回地址 gw+3e2d3b，34 号观测的"第二块数组"）
 *   0x472470    GraphwarGameStateManager::execute_results
 *   0x4724cc    同上：开火队列预扫（rbp=self；队列 = {ptr@+0x38, len@+0x40, cap@+0x48}，元素 0x20）
 *   0x472865    同上：`movdqu -0x10(%rdx,%rcx,1),%xmm0` = 读 `pos = pts[n-1]`
 *   0x9bac98    采样器函数指针槽（运行时 = base+0x3c2530）
 *
 * ── 采样器的签名（本轮**新读出**，逐指令见 docs/06 §26）─────────────────────
 *   `0x3c2530` 的取参（objdump 原文）：
 *     3c2541  mov %ecx,%r15d              ; 4.  ecx  = ★ mode（0=Normal / 1=Ode）
 *     3c2544  movsd %xmm2,0x60(%rsp)      ; 7.  xmm2 = step（后面 200.0/step 就是它）
 *     3c254a  mov %edx,%ebp               ; 3.  edx  = flip（0x3c25a4 `test %bpl,%bpl` ⇒ xorpd 翻号）
 *     3c254c  mov %rsi,%r14               ; 2.  rsi  = &TokenBuf{cap@0,ptr@8,len@0x10}（token 16 B）
 *     3c254f  movapd %xmm1,0x20(%rsp)     ; 6.  xmm1 = y0
 *     3c2555  movapd %xmm0,(%rsp)         ; 5.  xmm0 = x0
 *     3c255a  mov %rdi,%rbx               ; 1.  rdi  = &Vec<(f64,f64)>（sret，点数组就写这里）
 *   然后 3c25a4 `test %bpl,%bpl`（flip）、3c25f5 `test %r15b,%r15b`（★ mode 分两套二分循环）。
 *   ⇒ **mode 在入口就能直接读到**，不用靠"看起来像不像积分"去猜。
 *
 * ── 它抓什么 ────────────────────────────────────────────────────────────────
 *   ① `calc`    每次采样器调用：`mode/flip/x0/y0/step` + **喂进去的 token 流**（tag,value）
 *               + 返回的 `(ptr, n)` + 摘要 + 首末点（`--cfg fullArray` 时给整条 hex）
 *   ② `pos_read` 报文读 `pos = pts[n-1]` 那一刻：`rdx/n/r13/r15`、那 16 字节、
 *               数组首末点、以及"这个指针是哪一次 `calc` 的返回"（按指针点名，不按 ret+len 猜）
 *   ③ `exec_queue` / `exec_enter`：入队那一刻的队列项（与 34 号同一口径）
 *   ④ `eval_sample` 求值器前 N 次：`x / y / dy` 三个入参 —— Ode 下 y 会取很多**不同**值
 *   ⑤ `selftest` 正对照：合成数组 相同 / 翻 1 bit / 翻 2 bit 三档
 *
 * 只**观测**：不改内存、不打补丁、不调用游戏内函数。
 */
'use strict';

const CFG = {
  rva: {
    sampler: 0x3c2530,       // 点数组的生产者（采样器）
    evaluate: 0x4eb9d0,      // 求值器
    execute_results: 0x472470,
    exec_queue: 0x4724cc,
    pos_read: 0x472865,
    slot_sampler: 0x9bac98,
  },
  arrayCap: 20000,      // 单条数组最多记多少点（16 B/点）
  fullArray: 0,         // 1 = 把整条数组的 hex 也发回宿主（离线复算用；入库样本不要这个）
  evalSampleMax: 12,    // 每发最多抽样多少次求值器的入参
  calcKeep: 8,          // 记住最近几次采样器调用的数组（用于 pos_read 按指针点名）
};

const T0 = Date.now();
const now = () => Date.now() - T0;
let BASE = null;

function hx(v) { try { return v === null || v === undefined ? null : v.toString(16); } catch (e) { return null; } }
function abHex(v) {
  try { const b = new Uint8Array(v); let s = ''; for (let i = 0; i < b.length; i++) s += ('0' + b[i].toString(16)).slice(-2); return s; } catch (e) { return null; }
}
/** xmm 寄存器（frida 给 16 字节 ArrayBuffer）→ 低 8 字节的十六进制（= 一个 f64 的位串，内存序） */
function xmmLow(v) { const h = abHex(v); return h ? h.slice(0, 16) : null; }
function bytesHex(p, n) { try { const b = new Uint8Array(p.readByteArray(n)); let s = ''; for (let i = 0; i < b.length; i++) s += ('0' + b[i].toString(16)).slice(-2); return s; } catch (e) { return null; } }
function rvaOf(a) {
  try {
    const m = Process.findModuleByAddress(a);
    if (!m) return null;
    if (BASE && m.base.equals(BASE)) return 'gw+' + hx(a.sub(BASE));
    return m.name + '+' + hx(a.sub(m.base));
  } catch (e) { return null; }
}
function btOf(ctx, d) { try { return Thread.backtrace(ctx, Backtracer.FUZZY).slice(0, d || 12).map(rvaOf); } catch (e) { return ['ERR:' + e]; } }
/** 内存序 8 字节十六进制 → f64（只用于人看的十进制；判等一律用 hex 位串） */
function f64of(h) {
  if (!h || h.length < 16) return null;
  const b = new Uint8Array(8);
  for (let i = 0; i < 8; i++) b[i] = parseInt(h.substr(i * 2, 2), 16);
  try { return new Float64Array(b.buffer)[0]; } catch (e) { return null; }
}
function readable(p) {
  try {
    if (!p || p.isNull()) return false;
    const r = Process.findRangeByAddress(p);
    return !!(r && r.protection && r.protection.charAt(0) === 'r');
  } catch (e) { return false; }
}

// ── 摘要 / 逐点比较（与 34 号同一份实现；阳性对照见 selfTestCompare）────────────
const FNV_OFF = 0xcbf29ce484222325n, FNV_PRM = 0x100000001b3n, M64 = 0xffffffffffffffffn;
function digestOf(p, len) {
  let hxy = FNV_OFF, hy = FNV_OFF;
  const CH = 2048;
  for (let i = 0; i < len; i += CH) {
    const cnt = Math.min(CH, len - i);
    const b = new Uint8Array(p.add(i * 16).readByteArray(cnt * 16));
    for (let k = 0; k < cnt; k++) {
      for (let j = 0; j < 16; j++) { hxy ^= BigInt(b[k * 16 + j]); hxy = (hxy * FNV_PRM) & M64; }
      for (let j = 8; j < 16; j++) { hy ^= BigInt(b[k * 16 + j]); hy = (hy * FNV_PRM) & M64; }
    }
  }
  return { xy: hxy.toString(16).padStart(16, '0'), y: hy.toString(16).padStart(16, '0') };
}
function arrBytes(p, len) {
  try {
    if (!p || p.isNull() || !(len > 0) || len > CFG.arrayCap) return null;
    return new Uint8Array(p.readByteArray(len * 16));
  } catch (e) { return null; }
}
function hexPoint(b, i) { let s = ''; for (let j = 0; j < 16; j++) s += ('0' + b[i * 16 + j].toString(16)).slice(-2); return s; }
function cmpArrayBytes(a, b) {
  if (!a || !b) return { same: null, reason: a ? 'B null' : 'A null' };
  if (a.length !== b.length) return { same: false, reason: 'byte-length differs', lenA: a.length, lenB: b.length };
  let diffPoints = 0; const firstDiff = [];
  for (let i = 0; i < a.length; i += 16) {
    let eq = true;
    for (let j = 0; j < 16; j++) if (a[i + j] !== b[i + j]) { eq = false; break; }
    if (!eq) { diffPoints++; if (firstDiff.length < 8) firstDiff.push([i / 16, hexPoint(a, i / 16), hexPoint(b, i / 16)]); }
  }
  return { same: diffPoints === 0, diffPoints: diffPoints, total: a.length / 16, firstDiff: firstDiff };
}
function selfTestCompare() {
  const n = 100, A = new Uint8Array(n * 16), B = new Uint8Array(n * 16);
  for (let i = 0; i < A.length; i++) { A[i] = (i * 7 + 3) & 0xff; B[i] = A[i]; }
  const same = cmpArrayBytes(A, B);
  B[37 * 16 + 5] ^= 0x01;
  const one = cmpArrayBytes(A, B);
  B[90 * 16 + 15] ^= 0x80;
  const two = cmpArrayBytes(A, B);
  return {
    identical_reports_same: same.same === true && same.diffPoints === 0,
    onebit_reports_diff: one.same === false && one.diffPoints === 1 && one.firstDiff[0][0] === 37,
    twobit_reports_diff: two.diffPoints === 2 && two.firstDiff.map((d) => d[0]).join(',') === '37,90',
    detail: { same: same.same, one: one.diffPoints, two: two.diffPoints },
  };
}

const ST = {
  ins: false, seqCalc: 0, seqPos: 0, seqExec: 0, seqEval: 0,
  calcs: [],            // 最近几次采样器调用
  lastQueue: null,
  evalShot: [],         // 本发抽样到的求值器入参
};

/** 读 `&TokenBuf{cap@0,ptr@8,len@0x10}` 里的 token 流（每 16 B：{tag: u64, value: f64}） */
function readTokens(tokBuf, maxN) {
  const o = { cap: null, ptr: null, len: null, toks: [] };
  try {
    const cap = tokBuf.readU64().toString();
    const p = tokBuf.add(8).readPointer();
    const len = tokBuf.add(0x10).readU64().toString();
    o.cap = cap; o.ptr = hx(p); o.len = len;
    const n = Math.min(Number(len), maxN || 64);
    if (p && !p.isNull() && readable(p)) {
      for (let i = 0; i < n; i++) {
        o.toks.push({ t: Number(p.add(i * 16).readU64()), v: bytesHex(p.add(i * 16 + 8), 8) });
      }
      if (Number(len) > n) o.truncated = Number(len) - n;
    }
  } catch (e) { o.error = String(e); }
  return o;
}

function install(base) {
  BASE = base;
  const mod = Process.findModuleByAddress(base);
  const res = { type: 'hooks', base: hx(base), size: mod.size, hooks: [] };
  try {
    const slot = mod.base.add(CFG.rva.slot_sampler);
    const raw = bytesHex(slot, 16);
    // ⚠ 只反**第一个** qword（前 16 个十六进制字符）；把整条 32 字符一起反会把两个 qword 混在一起
    const le2be = (h) => h.match(/../g).reverse().join('');
    const tgt = raw ? ptr('0x' + le2be(raw.substr(0, 16))) : null;
    res.slot_sampler = { addr: hx(slot), raw: raw, target: tgt ? hx(tgt) : null, target_rva: tgt ? rvaOf(tgt) : null };
  } catch (e) { res.slot_sampler = { error: String(e) }; }

  // ── ① 采样器：入口读 mode/flip/x0/y0/step/token 流，出口读 (ptr, n) 与整条数组 ──────
  try {
    Interceptor.attach(base.add(CFG.rva.sampler), {
      onEnter() {
        const c = this.context;
        this.ret = rvaOf(c.rsp.readPointer());
        this.out = c.rdi;                       // &Vec<(f64,f64)>（sret）
        // ⚠ frida 的 CpuContext 只有 64 位寄存器名（`rcx`/`rdx`），没有 `ecx`/`edx`
        this.mode = c.rcx.toUInt32() & 0xff;    // ★ 0 = Normal，1 = Ode
        this.flip = c.rdx.toUInt32() & 0xff;
        this.x0 = xmmLow(c.xmm0);
        this.y0 = xmmLow(c.xmm1);
        this.step = xmmLow(c.xmm2);
        this.tokens = readTokens(c.rsi, 64);
        this.t = now();
      },
      onLeave(retval) {
        const o = {
          type: 'calc', t: this.t, seq: ++ST.seqCalc, ret: this.ret,
          mode: this.mode, flip: this.flip,
          x0: this.x0, x0f: f64of(this.x0), y0: this.y0, y0f: f64of(this.y0),
          step: this.step, stepf: f64of(this.step),
          tokens: this.tokens, arr: null,
        };
        try {
          const cap = this.out.readU64().toString();
          const p = this.out.add(8).readPointer();
          const n = Number(this.out.add(0x10).readU64());
          o.arr = { cap: cap, ptr: hx(p), n: n };
          if (p && !p.isNull() && n > 0 && readable(p) && n <= CFG.arrayCap) {
            const dg = digestOf(p, n);
            o.arr.digest_xy = dg.xy; o.arr.digest_y = dg.y;
            try {
              o.arr.first = bytesHex(p, 16);
              o.arr.last = bytesHex(p.add((n - 1) * 16), 16);
              if (n >= 2) o.arr.second_last = bytesHex(p.add((n - 2) * 16), 16);
              o.arr.firstf = f64of(o.arr.first.slice(0, 16));
              o.arr.lastf = f64of(o.arr.last.slice(0, 16));
            } catch (e) { }
            if (CFG.fullArray) {
              const b = arrBytes(p, n);
              if (b) { let s = ''; for (let i = 0; i < b.length; i++) s += ('0' + b[i].toString(16)).slice(-2); o.arr.hex = s; }
            }
            ST.calcs.push({ seq: o.seq, ret: this.ret, ptr: p, n: n, mode: this.mode, bytes: arrBytes(p, n) });
            while (ST.calcs.length > CFG.calcKeep) ST.calcs.shift();
          } else if (n > 0) {
            o.arr.skipped = readable(p) ? 'n>arrayCap' : 'unreadable';
          }
        } catch (e) { o.arr = { error: String(e) }; }
        send(o);
      },
    });
    res.hooks.push({ name: 'sampler', rva: '0x' + CFG.rva.sampler.toString(16) });
  } catch (e) { res.hooks.push({ name: 'sampler', error: String(e) }); }

  // ── ② 求值器：只抽前 N 次的 (x, y, dy) 入参 —— Ode 下 y 取多个不同值，Normal 下恒 0 ──
  try {
    Interceptor.attach(base.add(CFG.rva.evaluate), {
      onEnter() {
        const c = this.context;
        // evaluate_recursive(tokens:&TokenBuf{cap,ptr,len}, idx:&mut usize, x, y, dy) -> xmm0
        // 只有**顶层**调用值得看：递归调用的返回地址落在 0x4eb9d0 体内（同一张表）
        const ret = rvaOf(c.rsp.readPointer());
        ST.seqEval++;
        if (ST.evalShot.length >= CFG.evalSampleMax) return;
        ST.evalShot.push({ n: ST.seqEval, ret: ret, x: xmmLow(c.xmm0), y: xmmLow(c.xmm1), dy: xmmLow(c.xmm2) });
      },
    });
    res.hooks.push({ name: 'evaluate', rva: '0x' + CFG.rva.evaluate.toString(16) });
  } catch (e) { res.hooks.push({ name: 'evaluate', error: String(e) }); }

  // ── ③ execute_results 入口 ───────────────────────────────────────────────────
  try {
    Interceptor.attach(base.add(CFG.rva.execute_results), {
      onEnter(args) {
        send({ type: 'exec_enter', seq: ++ST.seqExec, t: now(), self: hx(args[0]), bt: btOf(this.context, 8) });
      },
    });
    res.hooks.push({ name: 'execute_results', rva: '0x' + CFG.rva.execute_results.toString(16) });
  } catch (e) { res.hooks.push({ name: 'execute_results', error: String(e) }); }

  // ── ④ 开火队列预扫：入队那一刻的 (ptr,len,摘要) ───────────────────────────────
  try {
    Interceptor.attach(base.add(CFG.rva.exec_queue), {
      onEnter() {
        const self = this.context.rbp;
        const out = { type: 'exec_queue', t: now(), self: hx(self), items: [] };
        let qptr = null, qlen = null, qcap = null;
        try { qptr = self.add(0x38).readPointer(); qlen = self.add(0x40).readU32(); qcap = self.add(0x48).readU32(); } catch (e) { }
        out.qptr = hx(qptr); out.qlen = qlen; out.qcap = qcap;
        const snap = [];
        if (qptr && !qptr.isNull() && qlen >= 0 && qlen < 4096) {
          for (let i = 0; i < qlen; i++) {
            // 队列项 0x20 字节：{tag@0x00, ptr@0x08, len@0x10, u32@0x18}（34 号观测口径）
            const it = qptr.add(i * 0x20);
            let tag = null, p8 = null;
            try { tag = it.readU64().toString(16); } catch (e) { }
            try { p8 = it.add(0x8).readPointer(); } catch (e) { }
            for (const off of [0x10, 0x18]) {
              let lenv = null;
              try { lenv = it.add(off).readU32(); } catch (e) { }
              const item = { item: i, tag: tag, lenOff: '0x' + off.toString(16), ptr: hx(p8), len: lenv };
              if (lenv > 0 && p8 && !p8.isNull() && readable(p8)) {
                try { const dg = digestOf(p8, lenv); item.digest_xy = dg.xy; item.digest_y = dg.y; } catch (e) { item.digestErr = String(e); }
                try { item.first = bytesHex(p8, 16); item.last = bytesHex(p8.add((lenv - 1) * 16), 16); } catch (e) { }
                snap.push({ item: i, off: '0x' + off.toString(16), ptr: hx(p8), len: lenv, digest_xy: item.digest_xy });
              } else if (lenv > 0) { item.skipped = 'unreadable'; }
              out.items.push(item);
            }
          }
        }
        ST.lastQueue = { t: now(), qptr: hx(qptr), qlen: qlen, snap: snap };
        // 一发结束：把这一发抽样到的求值器入参一起发走（下一发重新抽）
        out.eval_sample = ST.evalShot;
        ST.evalShot = [];
        send(out);
      },
    });
    res.hooks.push({ name: 'exec_queue', rva: '0x' + CFG.rva.exec_queue.toString(16) });
  } catch (e) { res.hooks.push({ name: 'exec_queue', error: String(e) }); }

  // ── ⑤ ★ 主抓点：报文读 `pos = pts[n-1]` 的那条 `movdqu` ─────────────────────────
  try {
    Interceptor.attach(base.add(CFG.rva.pos_read), {
      onEnter() {
        const c = this.context;
        const o = { type: 'pos_read', seq: ++ST.seqPos, t: now(), rdx: hx(c.rdx), rcx: hx(c.rcx), r13: hx(c.r13), r15: hx(c.r15) };
        try {
          const n16 = c.rcx.toUInt32() >>> 0;
          o.rcx_mod16 = n16 % 16;
          o.n = n16 / 16;
          o.n_from_r13 = c.r13.toUInt32() >>> 0;
          const p = c.rdx;
          o.ptr_readable = readable(p);
          if (n16 > 0 && n16 <= 32 * 1024 * 1024 && readable(p)) {
            o.pos_bytes = bytesHex(p.add(n16 - 16), 16);        // ← movdqu 要装进 xmm0 的 16 B
            o.pos_x = o.pos_bytes ? o.pos_bytes.slice(0, 16) : null;
            o.pos_y = o.pos_bytes ? o.pos_bytes.slice(16, 32) : null;
            o.pos_xf = f64of(o.pos_x); o.pos_yf = f64of(o.pos_y);
            try { o.first = bytesHex(p, 16); } catch (e) { }
            if (o.n >= 2) { try { o.second_last = bytesHex(p.add((o.n - 2) * 16), 16); } catch (e) { } }
            try { const dg = digestOf(p, o.n); o.digest_xy = dg.xy; o.digest_y = dg.y; } catch (e) { }
            // 这个指针是**哪一次**采样器调用返回的？按指针点名（不按 ret+len 猜，见 docs/06 §43.4）
            o.which_calc = [];
            for (const ca of ST.calcs) {
              o.which_calc.push({ seq: ca.seq, ret: ca.ret, same_ptr: hx(ca.ptr) === o.rdx, same_len: ca.n === o.n, mode: ca.mode });
            }
            o.vs_queue = (ST.lastQueue ? ST.lastQueue.snap : []).map((s) => ({
              item: s.item, off: s.off, ptr: s.ptr, len: s.len,
              same_ptr: s.ptr === o.rdx, same_len: s.len === o.n, same_digest: s.digest_xy === o.digest_xy,
            }));
          }
        } catch (e) { o.error = String(e); }
        send(o);
      },
    });
    res.hooks.push({ name: 'pos_read', rva: '0x' + CFG.rva.pos_read.toString(16) });
  } catch (e) { res.hooks.push({ name: 'pos_read', error: String(e) }); }

  ST.ins = true;
  try { send({ type: 'selftest', cmp: selfTestCompare() }); } catch (e) { send({ type: 'selftest', error: String(e) }); }
  send(res);
}

recv('cfg', (m) => {
  try {
    if (m && typeof m.fullArray === 'number') CFG.fullArray = m.fullArray;
    if (m && typeof m.arrayCap === 'number') CFG.arrayCap = m.arrayCap;
    if (m && typeof m.evalSampleMax === 'number') CFG.evalSampleMax = m.evalSampleMax;
    send({ type: 'cfg_ack', cfg: CFG });
  } catch (e) { send({ type: 'cfg_ack', error: String(e) }); }
});

setInterval(() => {
  if (ST.ins) return;
  const m = Process.enumerateModules().find((x) => /graphwarrust/i.test(x.name));
  if (!m) return;
  try { install(m.base); } catch (e) { send({ type: 'err', msg: String(e) }); ST.ins = true; }
}, 50);

setInterval(() => {
  send({ type: 'beat', t: now(), ins: ST.ins, calcs: ST.seqCalc, poses: ST.seqPos, evals: ST.seqEval, exes: ST.seqExec });
}, 3000);
