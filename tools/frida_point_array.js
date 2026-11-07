/**
 * tools/frida_point_array.js —— 在**真游戏**里抓"点数组"的填充者（`dispatch/25`，P0）。
 *
 * 目标（任务书 §2.3）：
 *   ① 谁往开火队列的点数组（`Vec<(f64,f64)>`）里写点；
 *   ② 写进去的 y 与**求值器**返回值是否逐位相同（1 ulp 在不在这一步）；
 *   ③ 同批两条 `DestroyObstacle` 坐标相同、序号不同 —— 序号从哪来。
 *
 * ── 下断地址对应的构建（地址必须带版本，见 AGENTS.md §0）──────────────────────
 *   构建：**Graphwar II Linux 2.0.60（Steam）**
 *         `libgraphwarrust.so` sha256 = 9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187
 *         （= 逆向目标那份，逐字节相同；x86-64 ELF，stripped，`.eh_frame` 完整）
 *   地址：**ELF 虚地址（RVA = 模块内偏移）**，下断一律 `base + rva`；Ghidra 地址 = RVA + 0x100000。
 *
 *   RVA        函数（`.eh_frame` FDE 实测范围；`tools/addr_probe.py fde <rva>`）
 *   0x472470   GraphwarGameStateManager::execute_results   [0x472470, 0x473513)
 *   0x4724cc   同上函数体内"开火队列预扫"第一条指令（`mov 0x40(%rbp),%rsi`）：
 *              rbp = self；队列 Vec = {ptr @ self+0x38, len @ self+0x40, cap @ self+0x48}，
 *              元素步长 0x20（函数体里 `shl $0x5`）。
 *   0x3c2530   calculate_function（点数组的**生产者**）   [0x3c2530, 0x3c2bb9)
 *   0x4eb9d0   求值器（`src/math_function.rs`）           [0x4eb9d0, 0x4ebbe0)
 *   0x9bac98   旧文档说的"采样器唯一入口"函数指针槽 —— 运行时读出 = `gw+0x3c2530`（已核对）
 *
 * ── 它怎么判定 1 ulp ───────────────────────────────────────────────────────
 *   在**进程内**建两张表：求值器每次返回的 y（xmm0 低 8 字节，位模式）与 (x,y) 对；
 *   等 `execute_results` 里的点数组出现时逐点回查：
 *       exact  = 这个点的 y 位模式出现过（**逐位相同**）
 *       ulp1   = 只差 ±1 ulp
 *       miss   = 都没有
 *       pairWho= (x,y) 双位模式都命中的那些点，命中的求值调用**来自哪个返回地址**
 *   这样不必把几十万行原始数据搬回宿主。
 *
 * ── 用法 ───────────────────────────────────────────────────────────────────
 *   A) 配套驱动（推荐；自动开局 + 长按开火）：tools/frida_point_array.py
 *   B) 自己挂（自己点菜单到对局里再开火）：
 *        frida -f ./Graphwar.x86_64 -l tools/frida_point_array.js --runtime=v8
 *
 * 只**观测**：不改内存、不改逻辑、不打补丁。
 *
 * ── 2026-11-07 独立复现轮的三处修正（全部只读，见 docs/06 §35.8.7）──────────────
 *  ① `hooks` 补上**真的**去读函数指针槽 `0x9bac98`（README 一直这么写，代码里原来是空的），
 *     并换算成 `gw+<RVA>`；这是"两个调用点都走同一个槽、槽 = 0x3c2530"的自证材料。
 *  ② `calculate_function` 的 `onLeave` 补记**返回值本身** `[rax, rax+0x18)` 与 `xmm0/xmm1` 全 16 字节。
 *     原脚本只记 `xmm0` 低 8 字节，于是"返回值就是这个队列元素（ptr 同一个）"**没法用本脚本重放**。
 *  ③ 队列元素的 dump 判据由 `pv.compare(base) > 0`（假设点数组一定在模块基址之上）改成
 *     "该地址落在**可读映射**里"。原判据在点数组落到 glibc brk 堆（低位地址，实测 `0x41679010`）时
 *     会**静默跳过整条点数组**、一条 `classify` 都不出。跳过的元素现在显式记 `skipped`。
 */
'use strict';

const CFG = {
  rva: { execute_results: 0x472470, exec_queue_probe: 0x4724cc, calculate_function: 0x3c2530, evaluate: 0x4eb9d0 },
  reallocMin: 16384,
  ringN: 3000,
  btDepth: 12,
};

let BASE = null;
const T0 = Date.now();
const now = () => Date.now() - T0;

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
function btOf(ctx, d) { try { return Thread.backtrace(ctx, Backtracer.FUZZY).slice(0, d || CFG.btDepth).map(rvaOf); } catch (e) { return ['ERR:' + e]; } }

/** 16 位十六进制位串（内存小端序）→ BigInt 位模式 */
function bitsOf(hexLE) {
  if (!hexLE || hexLE.length !== 16) return null;
  let v = 0n;
  for (let i = 14; i >= 0; i -= 2) v = (v << 8n) | BigInt(parseInt(hexLE.substr(i, 2), 16));
  return v;
}
function hexOfBits(b) {
  const s = (b & 0xffffffffffffffffn).toString(16).padStart(16, '0');
  // 反成小端字节串（与 readByteArray 的字节序一致）
  let out = '';
  for (let i = 14; i >= 0; i -= 2) out += s.substr(i, 2);
  return out;
}

const ST = {
  ins: false, sEval: 0, sSampler: 0, sExec: 0,
  evalRows: [], samplerRows: [], reallocRows: [],
  evalHist: {}, samplerHist: {},
  yMap: new Map(),      // y(LE hex) -> 次数
  pairMap: new Map(),   // "x|y" -> 次数
  ring: [],
  stats: { yHits: 0, y1ulp: 0, yMiss: 0, pairHits: 0, pairMiss: 0, arrays: 0 },
};

function flushAll() {
  if (ST.evalRows.length) { send({ type: 'evals', t: now(), rows: ST.evalRows }); ST.evalRows = []; }
  if (ST.samplerRows.length) { send({ type: 'samplers', t: now(), rows: ST.samplerRows }); ST.samplerRows = []; }
  if (ST.reallocRows.length) { send({ type: 'reallocs', t: now(), rows: ST.reallocRows }); ST.reallocRows = []; }
}

/** 逐点回查：完全命中 / ±1 ulp / 不命中 */
function classify(p, len) {
  const res = { n: len, exact: 0, ulp1: 0, miss: 0, pairExact: 0, examples: [], x: [], y: [] };
  const yB = new Map();
  for (const [k, v] of ST.yMap) yB.set(bitsOf(k), v);
  for (let i = 0; i < len; i++) {
    const xb = bytesHex(p.add(i * 16), 8), yb = bytesHex(p.add(i * 16 + 8), 8);
    if (i < 4) { res.x.push(xb); res.y.push(yb); }
    if (ST.yMap.has(yb)) res.exact++;
    else {
      const b = bitsOf(yb);
      let hit = false;
      for (const d of [-1n, 1n]) { if (yB.has(b + d)) { hit = true; break; } }
      if (hit) res.ulp1++; else res.miss++;
    }
    const who = ST.pairMap.get(xb + '|' + yb);
    if (who !== undefined) { res.pairExact++; res.pairWho = res.pairWho || {}; res.pairWho[who] = (res.pairWho[who] || 0) + 1; }
  }
  if (res.miss && res.examples.length < 6) {
    for (let i = 0; i < len && res.examples.length < 6; i++) {
      const yb = bytesHex(p.add(i * 16 + 8), 8);
      if (!ST.yMap.has(yb)) res.examples.push([i, bytesHex(p.add(i * 16), 8), yb]);
    }
  }
  return res;
}

function dumpPoints(p, len, tag, lenOff) {
  const o = { tag: hx(tag), ptr: hx(p), len: len, lenOff: lenOff, head: [], tail: [], yhash: null };
  if (!p || p.isNull() || !(len > 0) || len > (1 << 22)) return o;
  try {
    for (let i = 0; i < Math.min(8, len); i++) o.head.push([bytesHex(p.add(i * 16), 8), bytesHex(p.add(i * 16 + 8), 8)]);
    for (let i = Math.max(0, len - 8); i < len; i++) o.tail.push([bytesHex(p.add(i * 16), 8), bytesHex(p.add(i * 16 + 8), 8)]);
    let h = 0xcbf29ce484222325n;
    for (let i = 0; i < len; i += 1024) {
      const cnt = Math.min(1024, len - i);
      const b = new Uint8Array(p.add(i * 16 + 8).readByteArray(cnt * 16));
      for (let k = 0; k < b.length; k += 16) for (let j = 0; j < 8; j++) { h ^= BigInt(b[k + j]); h = (h * 0x100000001b3n) & 0xffffffffffffffffn; }
    }
    o.yhash = h.toString(16);
    o.all = bytesHex(p, len * 16);
    o.classify = classify(p, len);
  } catch (e) { o.error = String(e); }
  return o;
}

/** 这个地址是不是落在**可读**的映射里（判"能不能当指针用来 dump"，不看堆布局） */
function readable(p) {
  try {
    if (!p || p.isNull()) return false;
    const r = Process.findRangeByAddress(p);
    return !!(r && r.protection && r.protection.charAt(0) === 'r');
  } catch (e) { return false; }
}

/** 安全读：地址不在可读映射里就返回 null（宁可缺一条证据，也不要把整个钩子打挂） */
function safeRead(p, n) {
  try {
    if (!p || p.isNull()) return null;
    const r = Process.findRangeByAddress(p);
    if (!r || !r.protection || r.protection.charAt(0) !== 'r') return null;
    return bytesHex(p, n);
  } catch (e) { return null; }
}

/** 小端 8 字节十六进制 → 大端十六进制（为了和 rawitems 里的字段直接对照） */
function leToBe(h) {
  if (!h || h.length !== 16) return null;
  return h.match(/../g).reverse().join('');
}

function install(base) {
  BASE = base;
  const mod = Process.findModuleByAddress(base);
  const res = { type: 'hooks', base: hx(base), size: mod.size, hooks: [] };
  // ── 2026-11-07 独立复现轮追加（**只读，不改内存**）────────────────────────────
  // README 原本就写着 "`hooks` = 下断结果 + 指针槽 `0x9bac98` 前后 16 个字的运行时取值"，
  // 但代码里没有这一项；而 §35.4 的"唯一入口"和两个 `call *0x9bac98(%rip)` 调用点
  // 都要靠它才能自证。补上之后这条证据就能从仓库里一条命令重放。
  try {
    const slot = mod.base.add(0x9bac98);
    const raw = safeRead(slot, 16);
    const tgt = raw ? ptr('0x' + leToBe(raw.substr(0, 16))) : null;
    res.slot_9bac98 = {
      addr: hx(slot), raw: raw,
      target: tgt ? hx(tgt) : null,
      target_rva: tgt ? rvaOf(tgt) : null,
    };
  } catch (e) { res.slot_9bac98 = { error: String(e) }; }

  function hookFn(name, rva) {
    try {
      Interceptor.attach(base.add(rva), {
        onEnter() {
          this.s = (name === 'evaluate') ? ++ST.sEval : ++ST.sSampler;
          this.t = now();
          this.ret = rvaOf(this.context.rsp.readPointer());            // ★ 精确返回地址
          const hist = (name === 'evaluate') ? ST.evalHist : ST.samplerHist;
          hist[this.ret] = (hist[this.ret] || 0) + 1;
          this.r = { rdi: hx(this.context.rdi), rsi: hx(this.context.rsi), rdx: hx(this.context.rdx), rcx: hx(this.context.rcx) };
          this.xin = xmmLow(this.context.xmm0);
          this.bt = (this.s <= 3 || this.s % 20000 === 0) ? btOf(this.context) : null;
        },
        onLeave(retval) {
          const c = this.context;
          const yout = xmmLow(c.xmm0);
          if (name === 'evaluate') {
            if (yout) ST.yMap.set(yout, (ST.yMap.get(yout) || 0) + 1);
            if (yout && this.xin) ST.pairMap.set(this.xin + '|' + yout, this.ret);
          }
          // ── 2026-11-07 独立复现轮追加（**只读，不改内存**）──────────────────────
          // 为什么要加：§35.4 第 1 条证据是"`calculate_function` 的**返回值**就是队列元素
          // （ptr 同一个）"。可是原脚本只记 `xmm0` 的**低 8 字节**，JSONL 里根本没有返回值
          // 的指针 —— 也就是说那条证据无法用仓库里的脚本重放出来。
          // 这里把**完整返回值**记下来：本函数 0x20 字节的记录是**返回值在内存里**的
          // （onLeave 时 `rax == rdi` = 调用方给的隐藏 sret 指针），所以直接读 `[rax]`。
          // 同时记 xmm0/xmm1 全 16 字节，用来复核 §35.4 里"xmm0 低 16 字节 = {kind, ptr}"。
          let sret = null, xmmAll = null;
          if (name === 'calculate_function') {
            try {
              if (retval && !retval.isNull()) sret = safeRead(retval, 0x20);
            } catch (e) { }
            try {
              xmmAll = { xmm0: abHex(c.xmm0), xmm1: abHex(c.xmm1) };
            } catch (e) { }
          }
          // 只抽样回传（全量 20 万行会把 JSONL 撑到几十 MB；统计已在进程内做完）
          const recursive = this.ret && /^gw\+4eb(9|a|b)/.test(this.ret) && this.ret !== 'gw+4eb' ;
          if (!recursive) {
            const row = [this.s, this.t, this.ret, hx(retval), yout, this.xin, this.r.rdi, this.r.rsi, this.r.rdx, this.r.rcx, this.bt,
                         sret, xmmAll];
            (name === 'evaluate' ? ST.evalRows : ST.samplerRows).push(row);
            if (ST.evalRows.length > 4000) ST.evalRows.shift();
          }
          ST.ring.push([this.s, this.t, name[0], this.ret, this.xin, yout]);
          while (ST.ring.length > CFG.ringN) ST.ring.shift();
        },
      });
      res.hooks.push({ name: name, rva: '0x' + rva.toString(16) });
    } catch (e) { res.hooks.push({ name: name, error: String(e) }); }
  }

  hookFn('evaluate', CFG.rva.evaluate);
  hookFn('calculate_function', CFG.rva.calculate_function);

  try {
    Interceptor.attach(base.add(CFG.rva.execute_results), {
      onEnter(args) {
        this.self = args[0];
        send({ type: 'exec_enter', seq: ++ST.sExec, t: now(), self: hx(this.self), bt: btOf(this.context, 20), evals: ST.sEval, samplers: ST.sSampler });
      },
    });
    res.hooks.push({ name: 'execute_results' });
  } catch (e) { res.hooks.push({ name: 'execute_results', error: String(e) }); }

  try {
    Interceptor.attach(base.add(CFG.rva.exec_queue_probe), {
      onEnter() {
        flushAll();
        const self = this.context.rbp;
        const out = { type: 'exec_queue', t: now(), self: hx(self), qraw: bytesHex(self.add(0x28), 0x28), items: [], rawitems: [] };
        let qptr = null, qlen = null, qcap = null;
        try { qptr = self.add(0x38).readPointer(); qlen = self.add(0x40).readU32(); qcap = self.add(0x48).readU32(); } catch (e) { }
        out.qptr = hx(qptr); out.qlen = qlen; out.qcap = qcap;
        if (qptr && !qptr.isNull() && qlen >= 0 && qlen < 4096) {
          for (let i = 0; i < qlen; i++) {
            const it = qptr.add(i * 0x20);
            out.rawitems.push(bytesHex(it, 0x20));
            let tag = null, pv = null;
            try { tag = it.readU64(); pv = it.add(8).readPointer(); } catch (e) { }
            for (const off of [0x10, 0x18]) {
              let lenv = null;
              try { lenv = it.add(off).readU32(); } catch (e) { }
              // 2026-11-07 独立复现轮修正：原来的判据是 `pv.compare(base) > 0`
              // （假设点数组一定落在模块基址**之上**）。这个假设不成立 ——
              // 一发的 Vec 缓冲在 glibc 的 brk 堆上时指针会是 0x416xxxxx 这种低位地址，
              // 于是**整条点数组被静默跳过、classify 一条都不出**（实测 run3 就是这样丢的）。
              // 改成"这个地址落在可读映射里"，语义正确且不再依赖堆的布局。
              if (lenv > 0 && pv && !pv.isNull() && readable(pv)) {
                out.items.push(dumpPoints(pv, lenv, tag, '0x' + off.toString(16)));
              } else if (lenv > 0) {
                out.items.push({ tag: hx(tag), ptr: hx(pv), len: lenv, lenOff: '0x' + off.toString(16),
                                 skipped: 'not a readable mapping' });
              }
            }
          }
        }
        out.evalHist = ST.evalHist; out.samplerHist = ST.samplerHist;
        out.yMapSize = ST.yMap.size; out.pairMapSize = ST.pairMap.size;
        out.recentEvalBts = ST.evalRows.slice(-40).map((r) => [r[0], r[2]]);
        out.ring = ST.ring.slice(-CFG.ringN);
        send(out);
        ST.ring = [];
      },
    });
    res.hooks.push({ name: 'exec_queue_probe' });
  } catch (e) { res.hooks.push({ name: 'exec_queue_probe', error: String(e) }); }

  try {
    const rp = (() => { try { return Process.getModuleByName('libc.so.6').findExportByName('realloc'); } catch (e) { try { return Module.getGlobalExportByName('realloc'); } catch (e2) { return null; } } })();
    if (rp) {
      Interceptor.attach(rp, {
        onEnter(args) { this.sz = args[1].toUInt32() >>> 0; this.ret = rvaOf(this.context.rsp.readPointer()); },
        onLeave(retval) {
          if (this.sz < CFG.reallocMin) return;
          if (!this.ret || this.ret.indexOf('gw+') !== 0) return;
          ST.reallocRows.push([now(), this.sz, this.ret, hx(retval), btOf(this.context)]);
        },
      });
      res.hooks.push({ name: 'realloc', sizeMin: CFG.reallocMin });
    }
  } catch (e) { res.hooks.push({ name: 'realloc', error: String(e) }); }

  ST.ins = true;
  send(res);
}

setInterval(() => {
  if (ST.ins) return;
  const m = Process.enumerateModules().find((x) => /graphwarrust/i.test(x.name));
  if (!m) return;
  try { install(m.base); } catch (e) { send({ type: 'err', msg: String(e) }); ST.ins = true; }
}, 50);

setInterval(() => {
  flushAll();
  send({ type: 'beat', t: now(), ins: ST.ins, evals: ST.sEval, samplers: ST.sSampler, execs: ST.sExec, yMap: ST.yMap.size });
}, 2000);
