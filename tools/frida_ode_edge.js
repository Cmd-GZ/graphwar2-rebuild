/**
 * tools/frida_ode_edge.js —— 抓 **DiffEq（`Mode::Ode`）局**里一发的
 * 「采样器入参 → 点数组 → **停在哪条出口** → 报文读的那个点」全链（`dispatch/45`，P1）。
 *
 * 与 `tools/frida_ode_probe.js`（40 号）的关系：**从这里复制出来的独立一支**。
 * 40 号那支只抓"入参 + 数组 + 报文读点"，**停因只能靠我方的 trace 反推**；
 * 本支在采样器 `0x3c2530` **五条出口各自的判决指令上**下断，于是
 * 「这一发为什么停」变成**运行时判定**，而不是"看最后一个点像什么"。
 * 并行任务 44 在改 `frida_point_array.*` / 40 号那两支，本文件不碰它们。
 *
 * ── 新增：五条出口的**运行时判据**（RVA 全部来自 `0x3c2530` 的反汇编，见 §29.1）──
 *   RVA        入口条件（在这条指令上就是"已经决定要停"）        停因
 *   0x3c2840   **接受路径入口**（`0x3c27f2 ja` 成立才进来，一个点一次）：
 *              进来后二进制按 `ucomisd px,cand_y`+jp → `|mid|>25` → `|cand_y|>15` 的顺序查
 *              ⇒ 这一处同时判定 NaN / OverX / OverY（本探针的主判据之一）
 *   0x3c2b5d   **公共收尾**（所有出口 + 正常结束都跳这里，1 次调用只进 1 次）：
 *              `count == budget` ⇒ 预算用尽；`count < budget` 且接受路径没记过停因 ⇒ 二分 100 次不收敛
 *   0x3c2835   `jmp 0x3c2b5d`（hi=mid 分支用完 100 次）—— **不挂**（见下）
 *   0x3c2679   `je  0x3c2b5d`（lo=mid 分支用完）—— 逐轮命中太密，**不挂**
 *   0x3c291e   `jmp 0x3c2b5d`（`cmp %r13d,%r12d` 相等）—— **不挂**（见下）
 *
 *   ⚠ **无条件 `jmp` 不能挂**：挂上 0x3c2835 / 0x3c291e 之后，**每一次**采样器调用都会
 *   两条都命中（连 `n=1` 的那发也是 `NoConverge@3c2835 → Budget@3c291e → epi`），
 *   而返回的点数组仍与真值**逐位相同** ⇒ 那是 frida 重定位这两条 `jmp` 的观测假象。
 *   证据与推断见 `docs/06-verification.md` §29.5。
 *
 *   条件码分支（0x3c2881 / 0x3c28a0 / 0x3c28bc）在**每接受一个点**都会执行一次，
 *   所以 handler 里**自己把条件再算一遍**（与 `jp`/`ja` 同义；值取自栈帧
 *   `mid@rsp+0x80` / `cand_y@rsp+0x50`，正是那两条 `movapd` 装进 xmm 的东西）再记账；
 *   计数与收尾那两条只在**出口**执行，直接记账。三条判据互相独立：
 *   出口记账（精确）/ 收尾帧里的 `mid`、`cand_y`（复核）/ `r12d==r13d`（预算）。
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
 *               + ★ `stop`：**这一发停在哪条出口**（runtime 判定，见上表）
 *   ② `pos_read` 报文读 `pos = pts[n-1]` 那一刻：`rdx/n/r13/r15`、那 16 字节、
 *               数组首末点、以及"这个指针是哪一次 `calc` 的返回"（按指针点名，不按 ret+len 猜）
 *   ③ `exec_queue` / `exec_enter`：入队那一刻的队列项（与 34 号同一口径）
 *   ④ `eval_sample` 求值器前 N 次：`x / y / dy` 三个入参 —— Ode 下 y 会取很多**不同**值
 *   ⑤ `marker`  宿主在打字前 post 的 `{text, shot, level}` ⇒ 样本里能写清"这一发是哪个式子"
 *   ⑥ `selftest` 正对照：合成数组 相同 / 翻 1 bit / 翻 2 bit 三档
 *               + ★ 出口判据的正对照（见 `stopSelfTest`）：把每条出口的寄存器组合
 *                 喂给**同一套** `classifyFrame()`，看它是否给出对应的停因
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
    // ★ 五条出口 + 公共收尾（`0x3c2530` 的 Ode 分支，逐条来自 objdump）
    ode_no_conv_hi: 0x3c2835,   // hi=mid 分支用完 100 次二分
    ode_no_conv_lo: 0x3c2679,   // lo=mid 分支用完（`je` 只有 ebx==0 时成立）
    ode_nan: 0x3c2881,          // `jp`：cand_y 或 px 是 NaN
    ode_over_x: 0x3c28a0,       // `ja`：|mid| > 25
    ode_over_y: 0x3c28bc,       // `ja`：|cand_y| > 15
    ode_budget: 0x3c291e,       // 点数预算用尽（cmp r13d,r12d 相等后 jmp）
    ode_epilogue: 0x3c2b5d,     // 公共收尾（所有出口都跳这里）
    ode_accept_entry: 0x3c2840, // 接受路径入口（`ja 3c2840` 之后；一个被接受的点进一次）
  },
  // ★ 采样器栈帧（`sub $0xc8,%rsp` 之后，全部是 `rsp+偏移`）
  frame: { vec_cap: 0x38, vec_ptr: 0x40, vec_len: 0x48, cand_y: 0x50, mid: 0x80 },
  arrayCap: 200000,     // 单条数组最多记多少点（16 B/点）；★ 必须 > 20001，否则"预算用尽"那发记不下来
  fullArray: 0,         // 1 = 把整条数组的 hex 也发回宿主（离线复算用；入库样本不要这个）
  evalSampleMax: 12,    // 每发最多抽样多少次求值器的入参
  calcKeep: 8,          // 记住最近几次采样器调用的数组（用于 pos_read 按指针点名）
  stopHook: 1,          // 1 = 装五条出口的断点（0 只在排查开销时用）
  // 0 = **入口模式**（默认）：只挂 `0x3c2840`，一个点一次，把 NaN→OverX→OverY 按二进制顺序复算
  // 1 = **精确模式**：挂在三条条件码分支自己的地址上（每个点 3 次，慢 ~3 倍，用来做交叉验证）
  exactExits: 0,
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

// ══════════════════════════════════════════════════════════════════════════════
// ★ 停因：Ode 五条出口的**运行时**判定（`dispatch/45` 的核心新增）
//   判据不写在报告里，写在下面这三个函数里；`stopSelfTest()` 是它们的正对照。
// ══════════════════════════════════════════════════════════════════════════════
const LIMIT_X = 25.0, LIMIT_Y = 15.0;

/**
 * 与**条件码分支同义**的本地复算：只有条件真的成立，才算"这一次是从这条出口走的"。
 * `mid` / `cand_y` 取自采样器栈帧（`mid@rsp+0x80`、`cand_y@rsp+0x50`）——
 * 这正是那三条分支**在寄存器里装的东西**（`0x3c2871 movapd 0x50(%rsp),%xmm0`、
 * `0x3c2860 movapd 0x80(%rsp),%xmm1` 之后再 `ucomisd`），所以两者等价，
 * 但读内存比每次造 `Float64Array` 视图快一个量级（这个 handler 每个点都要跑）。
 * 同一套函数也被 `stopSelfTest()` 正对照调用。
 */
function classifyFrame(kind, mid, cand_y, extra) {
  const x = extra || {};
  switch (kind) {
    // 0x3c2881 `jp`：`ucomisd %xmm1,%xmm0`（xmm0 = cand_y、xmm1 = px=±mid）无序 ⇒ 任一 NaN
    case 'NaN': return (mid === null || cand_y === null) ? null : (mid !== mid || cand_y !== cand_y);
    // 0x3c28a0 `ja`：`ucomisd 25.0,%xmm0`（xmm0 = |mid|）；NaN 时 CF=1 ⇒ ja 不成立
    case 'OverX': return (mid === null) ? null : (mid === mid) && Math.abs(mid) > LIMIT_X;
    // 0x3c28bc `ja`：`ucomisd 15.0,%xmm0`（xmm0 = |cand_y|）
    case 'OverY': return (cand_y === null) ? null : (cand_y === cand_y) && Math.abs(cand_y) > LIMIT_Y;
    // 0x3c2679 `je`：`dec %ebx` 之后 ebx == 0（lo=mid 分支把 100 次用完）
    case 'NoConverge': return (x.ebx === null || x.ebx === undefined) ? null : (x.ebx >>> 0) === 0;
    // 0x3c291e：`cmp %r13d,%r12d` 相等（接受的点数 == 预算）
    case 'Budget': return (x.count === null || x.budget === null) ? null : (x.count >>> 0) === (x.budget >>> 0);
    default: return null;
  }
}

/** 收尾帧 `mid`/`cand_y` → 该走哪条出口（复核用；**不**是主判据） */
function kindFromFrame(mid, cand_y) {
  if (mid === null || cand_y === null) return null;
  if (mid !== mid || cand_y !== cand_y) return 'NaN';
  if (Math.abs(mid) > LIMIT_X) return 'OverX';
  if (Math.abs(cand_y) > LIMIT_Y) return 'OverY';
  return 'NoConverge';
}

/** 正对照：每条出口的帧值组合都必须被**它自己的**判据认出来，且近邻不认。 */
function stopSelfTest() {
  const cases = [
    ['NaN', { mid: NaN, cand_y: 0 }, true],
    ['NaN', { mid: 1.0, cand_y: NaN }, true],
    ['NaN', { mid: 1.0, cand_y: 2.0 }, false],
    ['OverX', { mid: 25.0, cand_y: 0 }, false],          // 严格 `>`：恰好 25 不停
    ['OverX', { mid: 25.0000000001, cand_y: 0 }, true],
    ['OverX', { mid: -25.5, cand_y: 0 }, true],          // 取绝对值后与符号无关
    ['OverX', { mid: NaN, cand_y: 0 }, false],           // NaN 不走 `ja`
    ['OverY', { mid: 0, cand_y: 15.0 }, false],
    ['OverY', { mid: 0, cand_y: 15.5 }, true],
    ['OverY', { mid: 0, cand_y: -14.9 }, false],
    ['NoConverge', { mid: 0, cand_y: 0, ebx: 0 }, true],
    ['NoConverge', { mid: 0, cand_y: 0, ebx: 100 }, false],
    ['Budget', { mid: 0, cand_y: 0, count: 20000, budget: 20000 }, true],
    ['Budget', { mid: 0, cand_y: 0, count: 19999, budget: 20000 }, false],
  ];
  const bad = [];
  for (const c of cases) {
    const got = classifyFrame(c[0], c[1].mid, c[1].cand_y, { ebx: c[1].ebx, count: c[1].count, budget: c[1].budget });
    if (got !== c[2]) bad.push({ kind: c[0], frame: c[1], want: c[2], got: got });
  }
  // 帧判据的单调性也验一下（复核用）：NaN 优先于越界、越界优先于 NoConverge
  const order = [
    ['NaN', 30.0, NaN], ['OverX', 30.0, 1.0], ['OverY', 1.0, 20.0], ['NoConverge', 1.0, 1.0],
  ].map((t) => t[0] === kindFromFrame(t[1], t[2])).every(Boolean);
  if (!order) bad.push({ kindFromFrame: 'order' });
  return { cases: cases.length, bad: bad, ok: bad.length === 0 };
}

/** 控制流环形缓冲：查"这一次调用到底走了哪几条指令"（出口 vs 收尾 vs 接受路径） */
function flowPush(tag) {
  const c = curOfThread().cur;
  if (!c) return;
  c.flow.push(tag);
  if (c.flow.length > 24) c.flow.shift();
}

/** 本线程当前的采样器调用（同一进程里可能有两个线程各跑一次，实测见过 ⇒ 必须按 tid 分开） */
function curOfThread() {
  const tid = Process.getCurrentThreadId();
  return { tid: tid, cur: ST.byTid[tid] || null };
}

/** 把"这一发停在哪条出口"记到当前采样器调用的记录上（一条调用只有一个出口 ⇒ 只记第一次） */
function noteStop(kind, at, ctx) {
  const t = curOfThread();
  const cur = t.cur;
  if (cur) {
    cur.exits = cur.exits || [];
    cur.exits.push({ kind: kind, at: at, tid: t.tid, callId: cur.callId });
    if (cur.exits.length <= 8) ST.events.push({ kind: kind, at: at, tid: t.tid, callId: cur.callId });
  }
  if (!cur || cur.stop) return;
  const o = { kind: kind, at: at, mode: cur.mode };
  try {
    o.count = ctx.r12.toUInt32();          // 已接受的点数（不含首点）
    o.budget = ctx.r13.toUInt32();         // `(200/step) as i32`（饱和、下限 1）
    const rsp = ctx.rsp;
    o.len = Number(rsp.add(CFG.frame.vec_len).readU64());   // 数组当前长度（可能还没 push 最后一个）
    o.cap = Number(rsp.add(CFG.frame.vec_cap).readU64());
    o.mid = bytesHex(rsp.add(CFG.frame.mid), 8);            // 触发这一次的那个候选 x
    o.cand_y = bytesHex(rsp.add(CFG.frame.cand_y), 8);      // 候选 y（RK4 的结果）
    o.mid_f = f64of(o.mid); o.cand_y_f = f64of(o.cand_y);
    o.xmm0 = xmmLow(ctx.xmm0); o.xmm1 = xmmLow(ctx.xmm1);
    if (kind === 'Budget') o.count_eq_budget = o.count === o.budget;
  } catch (e) { o.err = String(e); }
  cur.stop = o;
}

const ST = {
  ins: false, seqCalc: 0, seqPos: 0, seqExec: 0, seqEval: 0,
  calcs: [],            // 最近几次采样器调用
  lastQueue: null,
  evalShot: [],         // 本发抽样到的求值器入参
  cur: null,            // ★ 当前正在跑的采样器调用（单线程视角，仅调试用）
  byTid: {},            // ★ 每线程各自的"当前调用"——采样器可能被**不同线程**同时调用
  stack: [],            // 万一嵌套（不该发生）也不串
  seqCall: 0,           // 采样器调用序号（onEnter 就发，给出口事件点名）
  events: [],           // ★ 出口事件日志（含 tid / callId），用来查"一次调用命中两条出口"
  hits: {},             // 断点命中计数（诊断用）
  marker: null,         // 宿主 post 的 `{text, shot, level}`（这一发打的是什么式子）
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
        // ★ 开一条"这次调用"的记录：出口断点往它上面记停因
        this.tid = Process.getCurrentThreadId();
        this.cur = { callId: ++ST.seqCall, tid: this.tid, mode: this.mode, flip: this.flip,
                     startedAt: Date.now(), acceptHits: 0, stop: null, epi: null, exits: [], flow: [] };
        ST.prevByTid = ST.prevByTid || {};
        ST.prevByTid[this.tid] = ST.byTid[this.tid] || null;
        ST.byTid[this.tid] = this.cur;
        ST.stack.push(this.cur); ST.cur = this.cur;
      },
      onLeave(retval) {
        const cur = this.cur;
        // 按 tid 还原（同一线程可能先后多次调用；不同线程可能并行）
        if (ST.byTid[this.tid] === cur) {
          const prev = (ST.prevByTid || {})[this.tid] || null;
          if (prev) ST.byTid[this.tid] = prev; else delete ST.byTid[this.tid];
        }
        if (ST.stack.length && ST.stack[ST.stack.length - 1] === cur) ST.stack.pop();
        ST.cur = ST.stack.length ? ST.stack[ST.stack.length - 1] : null;
        const o = {
          type: 'calc', t: this.t, seq: ++ST.seqCalc, ret: this.ret,
          mode: this.mode, flip: this.flip,
          x0: this.x0, x0f: f64of(this.x0), y0: this.y0, y0f: f64of(this.y0),
          step: this.step, stepf: f64of(this.step),
          tokens: this.tokens, arr: null,
        };
        if (cur) {
          o.call_id = cur.callId; o.tid = cur.tid;
          o.exits = (cur.exits || []).slice(0, 8);  // ★ 这一条调用命中的**所有**出口事件（正常应正好 1 条）
          o.flow_tail = (cur.flow || []).slice(-16);  // ★ 这次调用最后 16 个控制流事件（排查用）
          o.stop = cur.stop;                       // ★ 停因（null = 没抓到出口，样本里会显式写）
          o.stop_epilogue = cur.epi;               // 收尾帧复核（mid / cand_y / count / budget）
          if (cur.stop_conflict) o.stop_conflict = cur.stop_conflict;
          o.accept_hits = cur.acceptHits;          // 命中"接受路径"的次数（应 = n-1 或 n）
          o.dur_ms = Date.now() - cur.startedAt;   // 采样器内部耗时（含探针开销）
        }
        this.marker = ST.marker;                   // 这一发打的是哪个式子（宿主 post 的）
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
          // ★ 交叉核对：出口记账的 `count`（已接受点数）与返回数组长度必须是 `n == count + 1`
          if (cur && cur.stop) {
            cur.stop.arr_n = n;
            cur.stop.count_plus1_eq_n = (cur.stop.count + 1 === n);
          }
          if (cur) o.arr_n_minus_accept_hits = (n - 1) - cur.acceptHits;
        } catch (e) { o.arr = { error: String(e) }; }
        o.marker = this.marker;
        send(o);
      },
    });
    res.hooks.push({ name: 'sampler', rva: '0x' + CFG.rva.sampler.toString(16) });
  } catch (e) { res.hooks.push({ name: 'sampler', error: String(e) }); }

  // ── ①b ★ 五条出口：在**判决指令**上下断，把"为什么停"变成运行时事实 ────────────
  //   `cond=true` 的三条是条件码分支（每个被接受的点都会执行一次）⇒ handler 里复算条件；
  //   另两条只在出口执行（hi 分支用完 / 预算相等）⇒ 直接记账。
  if (CFG.stopHook) {
    // ⚠ **不要挂无条件 `jmp`**：0x3c2835 / 0x3c291e 两条都是 `jmp 0x3c2b5d`，
    //   挂上去以后每一次调用都会**两条都命中**（连 n=1 那发都是 NoConverge→Budget），
    //   而数组仍与真值逐位相同 ⇒ 是 frida 重定位这条 jmp 造成的**观测假象**，不是游戏行为。
    //   （控制流证据：`flow_tail = ['NoConverge@3c2835','Budget@3c291e','epi']`，见 §29.5）
    //   ⇒ 只挂**非分支**指令：接受路径入口 0x3c2840 + 公共收尾 0x3c2b5d。
    const specs = [];
    if (CFG.exactExits) {
      specs.push({ kind: 'NaN', rva: CFG.rva.ode_nan, cond: true, why: 'jp 收尾：cand_y 或 px 是 NaN' });
      specs.push({ kind: 'OverX', rva: CFG.rva.ode_over_x, cond: true, why: 'ja 收尾：|mid| > 25' });
      specs.push({ kind: 'OverY', rva: CFG.rva.ode_over_y, cond: true, why: 'ja 收尾：|cand_y| > 15' });
    }
    for (const s of specs) {
      const at = 'gw+' + s.rva.toString(16);
      try {
        Interceptor.attach(base.add(s.rva), {
          onEnter() {
            const cur = curOfThread().cur;
            if (!cur) return;
            ST.hits[s.kind] = (ST.hits[s.kind] || 0) + 1;
            flowPush(s.kind + '@' + s.rva.toString(16));
            if (s.kind === 'NaN') cur.acceptHits++;      // 接受路径的入口，一条一个点
            if (s.cond) {
              const c = this.context, rsp = c.rsp;
              let mid = null, cy = null;
              try {
                mid = rsp.add(CFG.frame.mid).readDouble();
                cy = rsp.add(CFG.frame.cand_y).readDouble();
              } catch (e) { }
              if (classifyFrame(s.kind, mid, cy, {}) === true) noteStop(s.kind, at, c);
            } else {
              noteStop(s.kind, at, this.context);
            }
          },
        });
        res.hooks.push({ name: 'ode_exit_' + s.kind, rva: at, why: s.why });
      } catch (e) { res.hooks.push({ name: 'ode_exit_' + s.kind, rva: at, error: String(e) }); }
    }

    // ── ①b' 入口模式：在**接受路径入口** `0x3c2840` 上按二进制的顺序复算三条检查 ────────
    //   `0x3c2840` 只有 `0x3c27f2 ja`（`0.001 > |chord-step|`）成立才进得来 ⇒ 一个被接受的点一次。
    //   进了这里之后二进制的顺序是：`ucomisd px,cand_y`+jp（0x3c2881）→ `|mid|>25`（0x3c28a0）
    //   → `|cand_y|>15`（0x3c28bc），任一成立就停；都不成立才把点 push 进去。
    if (!CFG.exactExits) {
      const at = 'gw+' + CFG.rva.ode_accept_entry.toString(16) + '(accept-entry)';
      try {
        Interceptor.attach(base.add(CFG.rva.ode_accept_entry), {
          onEnter() {
            const cur = curOfThread().cur;
            if (!cur) return;
            cur.acceptHits++;
            flowPush('accept' + CFG.rva.ode_accept_entry.toString(16));
            const c = this.context, rsp = c.rsp;
            let mid = null, cy = null;
            try {
              mid = rsp.add(CFG.frame.mid).readDouble();
              cy = rsp.add(CFG.frame.cand_y).readDouble();
            } catch (e) { }
            const k = kindFromFrame(mid, cy);
            if (k && k !== 'NoConverge') noteStop(k, at, c);   // NoConverge = 三条都没中 = 这个点被接受
          },
        });
        res.hooks.push({ name: 'ode_accept_entry', rva: 'gw+' + CFG.rva.ode_accept_entry.toString(16),
                         why: '接受路径入口：按 0x3c2881/0x3c28a0/0x3c28bc 的顺序复算 NaN / |mid|>25 / |cand_y|>15' });
      } catch (e) { res.hooks.push({ name: 'ode_accept_entry', error: String(e) }); }
    }

    // ── ①c 公共收尾：所有出口都跳到这里（1 次调用 1 次）⇒ 兜底 + 复核 ──────────────
    try {
      Interceptor.attach(base.add(CFG.rva.ode_epilogue), {
        onEnter() {
          const cur = curOfThread().cur;
          if (!cur) return;
          flowPush('epi');
          const epi = {};
          try {
            const c = this.context;
            epi.count = c.r12.toUInt32();            // 已接受点数
            epi.budget = c.r13.toUInt32();
            epi.rsp = hx(c.rsp);
            const rsp = c.rsp;
            epi.len = Number(rsp.add(CFG.frame.vec_len).readU64());
            epi.cap = Number(rsp.add(CFG.frame.vec_cap).readU64());
            epi.mid = bytesHex(rsp.add(CFG.frame.mid), 8);
            epi.cand_y = bytesHex(rsp.add(CFG.frame.cand_y), 8);
            epi.mid_f = f64of(epi.mid); epi.cand_y_f = f64of(epi.cand_y);
            if (cur.mode === 1 && !cur.stop) {
              // ★ 主判据（收尾处三分）：能走到收尾而没在**接受路径**上记过停因的只有两种可能 ——
              //   ① `count == budget`：接受的点数正好等于 `(200/step) as i32` ⇒ 预算用尽；
              //   ② `count <  budget`：二分把 100 次用完了（hi=mid 分支 0x3c282f→0x3c2835，
              //      或 lo=mid 分支 0x3c2679），这一轮**没有**进入接受路径 ⇒ 不收敛。
              //   两条 `jmp` 不挂断（见上），但它们指向的收尾 `0x3c2b5d` 每次都必经。
              const isBudget = epi.count === epi.budget;
              cur.stop = {
                kind: isBudget ? 'Budget' : 'NoConverge',
                at: 'gw+3c2b5d(epilogue)',
                how: isBudget ? 'count == budget' : 'count < budget & no accept-path exit',
                mode: cur.mode,
                count: epi.count, budget: epi.budget,
                mid: epi.mid, cand_y: epi.cand_y, mid_f: epi.mid_f, cand_y_f: epi.cand_y_f,
                from_epilogue: true,
              };
            } else if (cur.stop && cur.mode === 1) {
              // 复核：收尾帧里最后那个候选，与出口断点记的停因是否**相容**。
              // ⚠ 这是**启发式**（帧里剩的是"最后一次二分算了什么"，不一定是判决依据）：
              //    真实反例 —— 全 NaN 候选走的是 hi=mid 分支把 100 次用完 ⇒ 停因是 NoConverge，
              //    而帧里 cand_y 是 NaN ⇒ 这里会 suggest NaN。冲突要**照实记**，不许吞。
              epi.suggests = kindFromFrame(epi.mid_f, epi.cand_y_f);
              if (cur.stop.kind !== 'Budget' && epi.suggests !== cur.stop.kind) {
                cur.stop_conflict = [cur.stop.kind, epi.suggests];
              }
            }
          } catch (e) { epi.err = String(e); }
          cur.epi = epi;
        },
      });
      res.hooks.push({ name: 'ode_epilogue', rva: 'gw+' + CFG.rva.ode_epilogue.toString(16) });
    } catch (e) { res.hooks.push({ name: 'ode_epilogue', error: String(e) }); }
  }

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
  try { send({ type: 'selftest', cmp: selfTestCompare(), stop: stopSelfTest() }); } catch (e) { send({ type: 'selftest', error: String(e) }); }
  send(res);
}

// ★ 宿主在**打字之前** post 一条 `{type:'marker', shot, text, level}` ⇒ 样本里带得出"这一发是哪个式子"
recv('marker', (m) => {
  try {
    ST.marker = { shot: m && m.shot, text: m && m.text ? String(m.text) : null, level: m && m.level };
    send({ type: 'marker', t: now(), shot: ST.marker.shot, text: ST.marker.text, level: ST.marker.level });
  } catch (e) { send({ type: 'marker', error: String(e) }); }
});

recv('cfg', (m) => {
  try {
    if (m && typeof m.fullArray === 'number') CFG.fullArray = m.fullArray;
    if (m && typeof m.arrayCap === 'number') CFG.arrayCap = m.arrayCap;
    if (m && typeof m.evalSampleMax === 'number') CFG.evalSampleMax = m.evalSampleMax;
    if (m && typeof m.stopHook === 'number') CFG.stopHook = m.stopHook;
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
  send({ type: 'beat', t: now(), ins: ST.ins, calcs: ST.seqCalc, poses: ST.seqPos, evals: ST.seqEval,
         exes: ST.seqExec, hits: ST.hits, threads: Object.keys(ST.byTid).map(Number), evlog: ST.events.slice(-8) });
}, 3000);
