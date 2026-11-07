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

function install(base) {
  BASE = base;
  const mod = Process.findModuleByAddress(base);
  const res = { type: 'hooks', base: hx(base), size: mod.size, hooks: [] };

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
          // 只抽样回传（全量 20 万行会把 JSONL 撑到几十 MB；统计已在进程内做完）
          const recursive = this.ret && /^gw\+4eb(9|a|b)/.test(this.ret) && this.ret !== 'gw+4eb' ;
          if (!recursive) {
            const row = [this.s, this.t, this.ret, hx(retval), yout, this.xin, this.r.rdi, this.r.rsi, this.r.rdx, this.r.rcx, this.bt];
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
              if (lenv > 0 && pv && !pv.isNull() && pv.compare(base) > 0) out.items.push(dumpPoints(pv, lenv, tag, '0x' + off.toString(16)));
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
