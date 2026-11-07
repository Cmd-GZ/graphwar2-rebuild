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
 *   0x472865   ★ 报文读的那一条：`movdqu -0x10(%rdx,%rcx,1),%xmm0`（`pos = pts[n-1]`）。
 *              **执行前**停下时：`rdx` = 点数组基址、`rcx` = n*16、`r13` = n、`r15d` = 队列项 +0x18 的 u32。
 *              上一句 `0x472860 mov 0x8(%rsp),%rdx` 刚从队列项 `+0x8/+0xc` 拼出指针；
 *              再上一句 `0x472859 mov %r13,%rcx / shl $0x4` 从队列项 `+0x10` 取 n。
 *              ⇒ 这里读到的 (rdx, rcx) 就是 §40.5 说的"pos 与 end_x 读的同一对"。
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
  rva: {
    execute_results: 0x472470, exec_queue_probe: 0x4724cc, calculate_function: 0x3c2530, evaluate: 0x4eb9d0,
    // 2026-11-08 `dispatch/34` 追加：`execute_results` 里读 `pos = pts[n-1]` 的那条 `movdqu`
    pos_read: 0x472865,
  },
  reallocMin: 16384,
  ringN: 3000,
  btDepth: 12,
  // ── 2026-11-08 `dispatch/34` 追加（见文件尾 §「ptr-pair probe」注释与 docs/06 §42）──
  arrayCap: 20000,      // 单个点数组最多拷多少点（16 B/点）来逐位对比；超过就只留摘要
  calcKeep: 8,          // 记住最近多少个 `calculate_function` 返回的数组（用于两两比对）
  evalRowsMax: 4000,    // `evals` 抽样行上限（驱动可用 cfg 消息改成 0，见 recv('cfg')）
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
  // 2026-11-08 `dispatch/34`：最近几次 `calculate_function` 的返回值与**字节副本**
  // `seq` 是全局自增序号 —— 有了它，"这一次的搭档是上一次"不用靠指针猜
  //（第一版就是靠 ret+len 猜，把"上一个**发**的数组"当成了"这一发的另一次调用"，
  //  于是把 20 发相同的假象翻成了 3 发"不同"；见 docs/06 §42.4 的更正）
  calcArrays: [], calcSeq: 0,
  // 2026-11-08 `dispatch/34`：最近一次 `execute_results` 入口扫到的队列项（含摘要），
  // 用来与后面 `movdqu` 那次读到的 (ptr,n) 对撞 ⇒ 判"队列项有没有在函数内被就地改写"
  lastQueue: null,
  sPos: 0,
  posReads: [],
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

// ── 2026-11-08 `dispatch/34`：点数组摘要 / 逐点比较 / 正对照 ─────────────────────
// 为什么要自己算摘要：这一轮要回答"两处读到的**是不是同一块缓冲**、若两块**内容是否逐位相同**"，
// 而原始数组一次 80 KB，搬回宿主不现实。摘要**只用来做检索/记录**；"是否相同"的判定一律用
// 下面的 `cmpArrayBytes()` **逐字节直接比**（不是拿摘要比），所以摘要碰撞不会造成假"相同"。
const FNV_OFF = 0xcbf29ce484222325n, FNV_PRM = 0x100000001b3n, M64 = 0xffffffffffffffffn;
/**
 * 一条点数组的两个 FNV-1a 64 摘要，**分块读、不整块拷贝**：
 *   `xy` 覆盖全部 16·len 字节（x 与 y 都算，本轮用）；`y` 只算每点后 8 B（口径同 §35 的 `yhash`）。
 */
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
/** 拷一条点数组的原始字节（len 点 × 16 B）；不可读/过大返回 null */
function arrBytes(p, len) {
  try {
    if (!p || p.isNull() || !(len > 0) || len > CFG.arrayCap) return null;
    return new Uint8Array(p.readByteArray(len * 16));
  } catch (e) { return null; }
}
function hexPoint(b, i) {
  let s = '';
  for (let j = 0; j < 16; j++) s += ('0' + b[i * 16 + j].toString(16)).slice(-2);
  return s;
}
/**
 * **逐点逐字节**比较两条点数组（不是比摘要）。
 * 返回 {same, lenA, lenB, diffPoints, firstDiff:[[i, A的16B, B的16B], …最多 8 个]}
 * ⚠ 这是本轮的**核心对比函数**：正对照见 `selfTestCompare()`（装机时跑，结果发 `selftest`）。
 */
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
/** 正对照：故意造两条已知不同的数组，确认对比函数**报得出来**（负结论的前提） */
function selfTestCompare() {
  const n = 100, A = new Uint8Array(n * 16), B = new Uint8Array(n * 16);
  for (let i = 0; i < A.length; i++) { A[i] = (i * 7 + 3) & 0xff; B[i] = A[i]; }
  const same = cmpArrayBytes(A, B);
  B[37 * 16 + 5] ^= 0x01;                       // 第 37 点、第 5 字节翻 1 个 bit
  const one = cmpArrayBytes(A, B);
  B[90 * 16 + 15] ^= 0x80;                      // 再加第 90 点
  const two = cmpArrayBytes(A, B);
  return {
    identical_reports_same: same.same === true && same.diffPoints === 0,
    onebit_reports_diff: one.same === false && one.diffPoints === 1 && one.firstDiff[0][0] === 37,
    twobit_reports_diff: two.diffPoints === 2 && two.firstDiff.map((d) => d[0]).join(',') === '37,90',
    detail: { same: same.same, one: one.diffPoints, oneIdx: one.firstDiff[0] && one.firstDiff[0][0], two: two.diffPoints },
  };
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
          let sret = null, xmmAll = null, arrInfo = null;
          if (name === 'calculate_function') {
            try {
              if (retval && !retval.isNull()) sret = safeRead(retval, 0x20);
            } catch (e) { }
            try {
              xmmAll = { xmm0: abHex(c.xmm0), xmm1: abHex(c.xmm1) };
            } catch (e) { }
            // ── 2026-11-08 `dispatch/34` 追加（**只读，不改内存**）─────────────────────
            // 为什么：§40.6 发现每发有**两次** `calculate_function`（`gw+3c30c7` 与 `gw+3e2d3b`），
            // 两块**不同缓冲**、点数相同；"这两条数组是否逐位相同"必须**在进程里**判（一条 80 KB）。
            // 这里：从 sret 记录 `{kind@0, ptr@8, len@0x10}` 取出数组指针与长度，
            // 算摘要、**留一份字节副本**，并与最近几次的数组**逐点逐字节**比。
            try {
              if (sret && sret.length === 64) {
                const ptrBe = leToBe(sret.substr(16, 16));     // +0x08
                const lenBe = leToBe(sret.substr(32, 16));     // +0x10
                const ap = (ptrBe && ptrBe !== '0000000000000000') ? ptr('0x' + ptrBe) : null;
                const alen = lenBe ? Number(BigInt('0x' + lenBe)) : 0;
                arrInfo = { ptr: hx(ap), len: alen, seq: ++ST.calcSeq,
                            kind: leToBe(sret.substr(0, 16)), tick18: leToBe(sret.substr(48, 16)) };
                if (ap && alen > 0 && readable(ap) && alen <= CFG.arrayCap) {
                  const bytes = arrBytes(ap, alen);
                  const dg = digestOf(ap, alen);
                  arrInfo.digest_xy = dg.xy; arrInfo.digest_y = dg.y;
                  arrInfo.pts = bytes ? [hexPoint(bytes, 0), hexPoint(bytes, alen - 1)] : null;
                  // 与记得的数组逐位比（同长度才比；不同长度直接记 lenDiff）
                  arrInfo.vs = [];
                  for (const old of ST.calcArrays) {
                    if (!old.bytes || !bytes) continue;
                    const r = cmpArrayBytes(bytes, old.bytes);
                    arrInfo.vs.push({ ret: old.ret, ptr: hx(old.ptr), len: old.len, seq: old.seq,
                                      same: r.same, diffPoints: r.diffPoints, firstDiff: r.firstDiff });
                  }
                  ST.calcArrays.push({ ret: this.ret, ptr: ap, len: alen, bytes: bytes, t: this.t, seq: arrInfo.seq });
                  while (ST.calcArrays.length > CFG.calcKeep) ST.calcArrays.shift();
                } else {
                  arrInfo.skipped = (ap && alen > CFG.arrayCap) ? 'len>arrayCap' : 'unreadable-or-empty';
                }
              }
            } catch (e) { arrInfo = { error: String(e) }; }
            // 每发两次调用，两次之间还要**跨发**比 ⇒ 直接把每次的摘要+比对结果发出去，
            // 不押到 2 s 的 beat 里（`samplers` 行也可能被抽样挤掉）
            try { send({ type: 'calc_array', t: now(), ret: this.ret, rax: hx(retval), info: arrInfo }); } catch (e) { }
          }
          // 只抽样回传（全量 20 万行会把 JSONL 撑到几十 MB；统计已在进程内做完）
          const recursive = this.ret && /^gw\+4eb(9|a|b)/.test(this.ret) && this.ret !== 'gw+4eb' ;
          if (!recursive && (name !== 'evaluate' || CFG.evalRowsMax > 0)) {
            const row = [this.s, this.t, this.ret, hx(retval), yout, this.xin, this.r.rdi, this.r.rsi, this.r.rdx, this.r.rcx, this.bt,
                         sret, xmmAll, arrInfo];
            (name === 'evaluate' ? ST.evalRows : ST.samplerRows).push(row);
            if (name === 'evaluate' && ST.evalRows.length > CFG.evalRowsMax) ST.evalRows.shift();
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
        const snap = [];
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
                const it = dumpPoints(pv, lenv, tag, '0x' + off.toString(16));
                // 2026-11-08 `dispatch/34`：给入队这一刻的数组存摘要，供 `movdqu` 那次读对撞
                try { const dg = digestOf(pv, lenv); it.digest_xy = dg.xy; it.digest_y = dg.y; } catch (e) { it.digestErr = String(e); }
                out.items.push(it);
                snap.push({ item: i, off: '0x' + off.toString(16), ptr: hx(pv), len: lenv,
                            digest_xy: it.digest_xy, digest_y: it.digest_y });
              } else if (lenv > 0) {
                out.items.push({ tag: hx(tag), ptr: hx(pv), len: lenv, lenOff: '0x' + off.toString(16),
                                 skipped: 'not a readable mapping' });
              }
            }
          }
        }
        // 2026-11-08 `dispatch/34`：把队列表在这一刻的样子记住（ptr/len/摘要），
        // 后面 `0x472865` 的 `movdqu` 会拿自己读到的 (rdx,n) 与它比 ⇒ 回答"队列项有没有被就地改写"
        ST.lastQueue = { t: now(), qptr: hx(qptr), qlen: qlen, snap: snap };
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

  // ── 2026-11-08 `dispatch/34` ★ 本轮的主抓点 ─────────────────────────────────
  // `execute_results` 里读 `pos = pts[n-1]` 的那条指令 `0x472865 movdqu -0x10(%rdx,%rcx,1),%xmm0`。
  // **执行前**停下时（前两句刚从队列项取完数）：
  //   `rdx` = 点数组基址（来自队列项 `+0x8/+0xc` 拼出的指针）
  //   `rcx` = n*16       `r13` = n（来自队列项 `+0x10`）
  //   `r15d` = 队列项 `+0x18` 的低 32 位（§35.8.4 那个 230/231 的 u32）
  // 这里只读：把这一刻的 (ptr, n)、要装进 xmm0 的 16 B、整条数组的摘要，
  // 分别与「队列入口扫到的样子」和「两次 `calculate_function` 的返回值」对撞。
  try {
    Interceptor.attach(base.add(CFG.rva.pos_read), {
      onEnter() {
        const c = this.context;
        const o = { type: 'pos_read', seq: ++ST.sPos, t: now(), rdx: hx(c.rdx), rcx: hx(c.rcx), r13: hx(c.r13), r15: hx(c.r15) };
        try {
          const n16 = c.rcx.toUInt32() >>> 0;
          o.rcx_mod16 = n16 % 16;
          o.n_from_rcx = n16 / 16;
          const p = c.rdx;
          o.ptr_readable = readable(p);
          if (n16 > 0 && n16 <= 32 * 1024 * 1024 && readable(p)) {
            o.n = n16 / 16;
            o.n_from_r13 = c.r13.toUInt32() >>> 0;
            o.pos_bytes = bytesHex(p.add(n16 - 16), 16);          // ← movdqu 要装进 xmm0 的那 16 B
            try { const dg = digestOf(p, o.n); o.digest_xy = dg.xy; o.digest_y = dg.y; } catch (e) { o.digestErr = String(e); }
            // ① 与「同一次 execute_results 入口扫到的队列项」比 ⇒ 函数体内有没有被就地改写
            o.vs_queue = [];
            if (ST.lastQueue) {
              for (const s of ST.lastQueue.snap) {
                o.vs_queue.push({ item: s.item, off: s.off, ptr: s.ptr, len: s.len,
                                  same_ptr: s.ptr === o.rdx, same_len: s.len === o.n,
                                  same_digest: s.digest_xy === o.digest_xy });
              }
            }
            // ② 与「两次 calculate_function 各自返回的数组」逐字节比 ⇒ 报文读的是哪一块
            o.vs_calc = [];
            const bytes = arrBytes(p, o.n);
            if (bytes) {
              for (const ca of ST.calcArrays) {
                if (!ca.bytes) continue;
                const r = cmpArrayBytes(bytes, ca.bytes);
                o.vs_calc.push({ ret: ca.ret, ptr: hx(ca.ptr), len: ca.len, same: r.same,
                                 diffPoints: r.diffPoints, firstDiff: r.firstDiff });
              }
            } else { o.vs_calc_skipped = 'len>arrayCap or unreadable'; }
          }
        } catch (e) { o.error = String(e); }
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'pos_read', rva: '0x' + CFG.rva.pos_read.toString(16) });
  } catch (e) { res.hooks.push({ name: 'pos_read', error: String(e) }); }

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
  // 正对照：先证明"逐点对比函数"对**已知不同**的两条数组会报不同（负结论的前提，见 docs/06 §42）
  try { send({ type: 'selftest', cmp: selfTestCompare() }); } catch (e) { send({ type: 'selftest', error: String(e) }); }
  send(res);
}

// 驱动可以在 `script.load()` 之后 post 一条 {type:'cfg', …} 来覆盖 CFG（例如把 evals 抽样关掉）
recv('cfg', (m) => {
  try {
    if (m && typeof m.evalRowsMax === 'number') CFG.evalRowsMax = m.evalRowsMax;
    if (m && typeof m.arrayCap === 'number') CFG.arrayCap = m.arrayCap;
    send({ type: 'cfg_ack', evalRowsMax: CFG.evalRowsMax, arrayCap: CFG.arrayCap });
  } catch (e) { send({ type: 'cfg_ack', error: String(e) }); }
});

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
