/*
 * tools/frida_lean_triple.js —— **精简**三样探针（`dispatch/82`，AI-vs-AI 的收敛解那一发）。
 *
 * 为什么另写一个：`docs/06` §42.8 第 3 条实测——`tools/frida_point_array.js` 的**求值器钩子**
 * （`0x4eb9d0`）在 `onLeave` 里对**每一次**求值做 `Map` 记账，而一次 AI 评分要调它约 **1.4 万次**
 * （实测 `evals` 3.6e6 / `samplers` 253）⇒ 单次评分 ≈ **0.3 s**，一回合（AI 墙钟预算 25 s）
 * 只够跑完**一代**，AI 交出来的是随机个体（69 号那一发是单 token 的 `x`）。
 * 本文件把那些**每次求值都要付钱**的东西全部去掉，只留"每发 O(1) 次"的钩子：
 *
 *   0x3c2530  `calculate_function`（采样器）入口 + 返回值记录（**不再拷整条数组的十六进制**，
 *             只算一个 32 位×2 的滚动摘要；最近 `keepCalcs` 条才留原始字节）
 *   0x472865  `execute_results` 里 `movdqu pts[n-1]`（pos 读点）——在这一刻**回吐**在进程内
 *             攒下的 `tok_fire` / `calc_array` 环（见下），所以**不需要**给每次采样发一条消息
 *   0x4729d6  `EffectInfo` 装配点 A（`rsi` = &载荷）         ┐ 每发各一次
 *   0x472e4d  同上的站点 B（Death/Spark/Ash）                │
 *   0x50f990  `EffectInfo` → JSON 写入器                     ┘
 *   0x3c5f70  `calculate_function_offset_str`（印 `" + 9.961"`；`rcx` = &Point）——每发一次
 *   0x50eb20  `FunctionPointsEvent` → JSON（`start_x/start_y/end_x/owner_id`）——每发一次
 *
 * **去掉的**（与 `frida_point_array.js` 的差集，逐条对得上）：
 *   ✗ `0x4eb9d0` 求值器（**主因**：3.6e6 次/局，每次 Map + 字符串拼接）
 *   ✗ `0x4ed670` 解析器（只在输入框打字时才走，与"AI 交卷那一发"无关；要文本就从 0x3c5f70 的实参取）
 *   ✗ `0x4724cc` 开火队列预扫（每次把**整条**点数组 dump 成十六进制）
 *   ✗ `0x456810` `FunctionPoints` 装配（每次把整条数组 hexPoint 化并与 8 条数组逐点比）
 *   ✗ `libc realloc` 钩子、`evals/samplers/reallocs` 抽样行、`ring`/`yMap`/`pairMap` 记账
 *   ✗ 每次 `calculate_function` 都 `send` 一条 `tok_fire` + 一条 `calc_array`（AI 搜索里是几万条）
 *
 * 消息形状与 `tools/frida_point_array.js` 的 `tok_fire` / `tok_suffix` / `pos_read` /
 * `eff_payload` / `eff_json` / `fnpoints` / `calc_array` **同名同字段**（这样 `tools/lan_ai_triple.py`
 * 那套判据一行不改就能用；环里回吐的那些由 `tools/lean_triple.py` 摊平成同样的行）。
 *
 * 只**观测**：不改内存、不改逻辑、不打补丁。
 *
 * 地址全部是 `libgraphwarrust.so` 的 vaddr（RVA = 模块内偏移），
 * 构建 sha256 `9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187`（steam/2026-08-28）。
 */
'use strict';

const RVA = {
  sampler: 0x3c2530,
  pos_read: 0x472865,
  eff_A: 0x4729d6,
  eff_B: 0x472e4d,
  eff_json: 0x50f990,
  suffix: 0x3c5f70,
  fnpoints_ser: 0x50eb20,
};

const CFG = {
  maxTokens: 256,      // 单个 token 列表最多拷多少 token（16 B/token）
  arrayCap: 40960,     // 单条点数组最多纳入（点）；超过只记 ptr/len，不算摘要
  keepCalcs: 8,        // 环里最近多少条 `calculate_function` 返回**留原始字节**
  ringCalcs: 4096,     // 环里最近多少条 calc 记录（都带摘要；`dispatch/82` 第二轮从 24 提到 4096 ——
                       // 24 条时环在 2 ms 内就被 AI 搜索冲掉，`0x472865` 那一刻已经找不到生产者了）
  ringFires: 24,       // 环里最近多少条 `tok_fire` 记录（都带 token 十六进制）
  sendFirst: 400,      // 前多少条 `tok_fire` / `calc_array` 也**直接**发给宿主机（看搜索轨迹）
  sendEvery: 3000,     // 之后每 N 条发一条
  suffixOther: 60,     // 非开火路径（输入框 `0x4f28a6`）的 `tok_suffix` 最多发多少条
  jsonMax: 4096,
};

let BASE = null;
const T0 = Date.now();
const now = () => Date.now() - T0;

function hx(v) { try { return v === null || v === undefined ? null : v.toString(16); } catch (e) { return null; } }
function abHex(v) {
  try { const b = new Uint8Array(v); let s = ''; for (let i = 0; i < b.length; i++) s += ('0' + b[i].toString(16)).slice(-2); return s; } catch (e) { return null; }
}
function xmmLow(v) { const h = abHex(v); return h ? h.slice(0, 16) : null; }
function bytesHex(p, n) {
  try { const b = new Uint8Array(p.readByteArray(n)); let s = ''; for (let i = 0; i < b.length; i++) s += ('0' + b[i].toString(16)).slice(-2); return s; } catch (e) { return null; }
}
function rvaOf(a) {
  try {
    const m = Process.findModuleByAddress(a);
    if (!m) return null;
    if (BASE && m.base.equals(BASE)) return 'gw+' + hx(a.sub(BASE));
    return m.name + '+' + hx(a.sub(m.base));
  } catch (e) { return null; }
}
function readable(p) {
  try {
    if (!p || p.isNull()) return false;
    const r = Process.findRangeByAddress(p);
    return !!(r && r.protection && r.protection.charAt(0) === 'r');
  } catch (e) { return false; }
}
/** Rust `Vec<T>`/`String` 三元组：`{cap@+0, ptr@+8, len@+0x10}` */
function vecTriple(p) {
  try {
    return { cap: Number(p.readU64().toString()), ptr: p.add(8).readPointer(), len: Number(p.add(0x10).readU64().toString()) };
  } catch (e) { return { error: String(e) }; }
}
/** 两个独立的 32 位 FNV-1a（**不分配**、不比 BigInt）：只用来"同一份内容"的快速对撞，
 *  真正的"相同"判定仍在 `cmpArray()` 里**逐字节**比（碰撞不会造成假"相同"） */
function hash2(b) {
  let h1 = 0x811c9dc5 | 0, h2 = 0x01000193 | 0;
  for (let i = 0; i < b.length; i++) {
    const v = b[i];
    h1 = Math.imul(h1 ^ v, 16777619) | 0;
    h2 = Math.imul(h2 + v, 2246822519) | 0;
    h2 ^= h2 >>> 13;
  }
  return ((h1 >>> 0).toString(16).padStart(8, '0') + (h2 >>> 0).toString(16).padStart(8, '0'));
}
function hexPoint(b, i) {
  let s = '';
  for (let j = 0; j < 16; j++) s += ('0' + b[i * 16 + j].toString(16)).slice(-2);
  return s;
}
/** 逐点逐字节比（返回 diffPoints / firstDiff），与 `frida_point_array.js` 的同名函数同口径 */
function cmpArray(a, b) {
  if (!a || !b) return { same: null, reason: a ? 'B null' : 'A null' };
  if (a.length !== b.length) return { same: false, reason: 'byte-length differs', lenA: a.length / 16, lenB: b.length / 16 };
  let diffPoints = 0; const firstDiff = [];
  for (let i = 0; i < a.length; i += 16) {
    let eq = true;
    for (let j = 0; j < 16; j++) if (a[i + j] !== b[i + j]) { eq = false; break; }
    if (!eq) { diffPoints++; if (firstDiff.length < 8) firstDiff.push([i / 16, hexPoint(a, i / 16), hexPoint(b, i / 16)]); }
  }
  return { same: diffPoints === 0, diffPoints: diffPoints, total: a.length / 16, firstDiff: firstDiff };
}
/** 正对照：故意造两条已知不同的数组，确认比较器**报得出来**（负结论的前提，照 AGENTS.md 合并纪律 A4） */
function selfTestCompare() {
  const n = 64, A = new Uint8Array(n * 16), B = new Uint8Array(n * 16);
  for (let i = 0; i < A.length; i++) { A[i] = (i * 7 + 3) & 0xff; B[i] = A[i]; }
  const same = cmpArray(A, B), sameH = (hash2(A) === hash2(B));
  B[37 * 16 + 5] ^= 0x01;
  const one = cmpArray(A, B), oneH = (hash2(A) === hash2(B));
  B[60 * 16 + 15] ^= 0x80;
  const two = cmpArray(A, B);
  return { identical_same: same.same === true && same.diffPoints === 0,
           identical_hash_eq: sameH === true,
           onebit_diff1: one.same === false && one.diffPoints === 1 && one.firstDiff[0][0] === 37,
           onebit_hash_differs: oneH === false,
           twobit_diff2: two.diffPoints === 2 && two.firstDiff.map((d) => d[0]).join(',') === '37,60' };
}

const ST = {
  ins: false,
  samplers: 0, suffixes: 0, suffixesFire: 0, posReads: 0, effPays: 0, effJsons: 0, fnpoints: 0,
  fireRing: [], calcRing: [], calcSeq: 0,
  lastFn: null,
};

function sendTokFire(o) {
  try { send(o); } catch (e) { }
}
function sendCalc(o) {
  try { send(o); } catch (e) { }
}

/** 前缀比较：`pos`（n 点，16 B/点）是不是 `arr`（m≥n 点）的**逐字节前缀**。
 *  用来判 §40.6/§42.5 那一族"打到障碍被截断"的发——那时队列项是采样器数组的**前 n 点**，
 *  整条比必然不相等（长度不同），只能比前缀。 */
function cmpPrefix(pos, arr, nPts) {
  if (!pos || !arr) return { same: null, reason: 'null side' };
  const nb = nPts * 16;
  if (arr.length < nb) return { same: false, reason: 'arr shorter than pos' };
  let diffPoints = 0; const firstDiff = [];
  for (let i = 0; i < nb; i += 16) {
    let eq = true;
    for (let j = 0; j < 16; j++) if (arr[i + j] !== pos[i + j]) { eq = false; break; }
    if (!eq) { diffPoints++; if (firstDiff.length < 8) firstDiff.push([i / 16, hexPoint(pos, i / 16), hexPoint(arr, i / 16)]); }
  }
  return { same: diffPoints === 0, diffPoints: diffPoints, comparedPoints: nPts, firstDiff: firstDiff };
}

function install(base) {
  BASE = base;
  const mod = Process.findModuleByAddress(base);
  const res = { type: 'hooks', lean: true, base: hx(base), size: mod.size, hooks: [], dropped: [] };

  // ── ① 采样器 `0x3c2530`：入口实参 + 退出时 sret 记录（ptr/len/摘要；最近 N 条留字节）──────
  try {
    Interceptor.attach(base.add(RVA.sampler), {
      onEnter() {
        const c = this.context;
        this.t = now();
        this.ret = rvaOf(c.rsp.readPointer());
        const o = { type: 'tok_fire', lean: true, seq: ++ST.samplers, t: this.t, ret: this.ret,
                    sret: hx(c.rdi),
                    flip: (Number(c.rdx.toString()) & 0xff),
                    mode: (Number(c.rcx.toString()) & 0xff),
                    x0: xmmLow(c.xmm0), y0: xmmLow(c.xmm1), step: xmmLow(c.xmm2) };
        try {
          const vec = vecTriple(c.rsi);
          o.vec = { cap: vec.cap, ptr: hx(vec.ptr), len: vec.len };
          if (vec.ptr && !vec.ptr.isNull() && vec.len > 0 && vec.len <= CFG.maxTokens) {
            const b = new Uint8Array(vec.ptr.readByteArray(vec.len * 16));
            let s = '';
            for (let i = 0; i < b.length; i++) s += ('0' + b[i].toString(16)).slice(-2);
            o.tok = { len: vec.len, ptr: hx(vec.ptr), hex: s };
          } else { o.tok = { len: vec.len, ptr: hx(vec.ptr), note: 'empty-or-len>maxTokens' }; }
        } catch (e) { o.error = String(e); }
        this.o = o;
        ST.fireRing.push(o);
        while (ST.fireRing.length > CFG.ringFires) ST.fireRing.shift();
        if (o.seq <= CFG.sendFirst || o.seq % CFG.sendEvery === 0) sendTokFire(o);
      },
      onLeave(retval) {
        const o = this.o; if (!o) return;
        const rec = { seq: ++ST.calcSeq, t: this.t, ret: this.ret, rax: hx(retval), sret: hx(retval),
                      ptr: null, len: 0, kind: null, digest_xy: null, pts: null, bytes: null,
                      fire_seq: o.seq };
        try {
          if (retval && !retval.isNull()) {
            const sret = new Uint8Array(retval.readByteArray(0x20));
            rec.kind = Array.from(sret.slice(0, 8)).reverse().map((x) => ('0' + x.toString(16)).slice(-2)).join('');
            const ap = retval.add(8).readPointer();
            const alen = Number(retval.add(0x10).readU64().toString());
            rec.ptr = hx(ap); rec.len = alen;
            if (ap && !ap.isNull() && alen > 0 && alen <= CFG.arrayCap && readable(ap)) {
              const bytes = new Uint8Array(ap.readByteArray(alen * 16));
              rec.digest_xy = hash2(bytes);
              rec.pts = [hexPoint(bytes, 0), hexPoint(bytes, alen - 1)];
              rec.bytes = bytes;                     // 只保留最近 keepCalcs 条的字节（下面 shift 时清掉）
            } else { rec.skipped = (alen > CFG.arrayCap) ? 'len>arrayCap' : 'unreadable-or-empty'; }
          }
        } catch (e) { rec.error = String(e); }
        ST.calcRing.push(rec);
        while (ST.calcRing.length > CFG.ringCalcs) {
          const drop = ST.calcRing.shift();
          if (drop) drop.bytes = null;               // 环外的不再留字节（省内存）
        }
        // 最近 keepCalcs 条之外的把字节腾掉（保留摘要，仍能出 vs_calc 的"摘要不同"）
        let seen = 0;
        for (let i = ST.calcRing.length - 1; i >= 0; i--) {
          seen++;
          if (seen > CFG.keepCalcs && ST.calcRing[i].bytes) ST.calcRing[i].bytes = null;
        }
        if (rec.seq <= CFG.sendFirst || rec.seq % CFG.sendEvery === 0) {
          sendCalc({ type: 'calc_array', lean: true, t: rec.t, ret: rec.ret, rax: rec.rax,
                     info: { ptr: rec.ptr, len: rec.len, kind: rec.kind, seq: rec.seq,
                             digest_xy: rec.digest_xy, pts: rec.pts, bytes_kept: !!rec.bytes } });
        }
      },
    });
    res.hooks.push({ name: 'lean_sampler_0x3c2530' });
  } catch (e) { res.hooks.push({ name: 'lean_sampler_0x3c2530', error: String(e) }); }

  // ── ② `0x472865`：`pos = pts[n-1]` 那一条 `movdqu`（**每发一次**）──────────────────
  //     在这里把 ①② 的环**一起回吐**：对宿主来说"这一发的三样"是同时到的，
  //     不需要为 AI 搜索里的每一次采样各发一条消息。
  try {
    Interceptor.attach(base.add(RVA.pos_read), {
      onEnter() {
        const c = this.context;
        const o = { type: 'pos_read', lean: true, seq: ++ST.posReads, t: now(),
                    rdx: hx(c.rdx), rcx: hx(c.rcx), r13: hx(c.r13), r15: hx(c.r15) };
        try {
          const n16 = c.rcx.toUInt32() >>> 0;
          o.n = n16 / 16;
          const p = c.rdx;
          if (n16 > 0 && o.n <= CFG.arrayCap && readable(p)) {
            const bytes = new Uint8Array(p.readByteArray(n16));
            o.pos_bytes = hexPoint(bytes, o.n - 1);            // 最后 16 字节 = movdqu 要装进 xmm0 的
            o.digest_xy = hash2(bytes);
            // ② 与环里每一次 `calculate_function` 返回的数组对齐。**三条对齐键分开记**：
            //    · same_ptr   —— `rdx` 就是那个数组的基址（最硬：同一条缓冲）
            //    · same_len   —— 长度相同（可能被截断成同一个长度，仍需字节/摘要判）
            //    · same_digest—— 不同缓冲、内容相同（摘要相等且长度相同）
            //    只有**interesting** 的才回传（环 4096 条，全回传没有意义），另给计数。
            o.vs_calc = [];
            o.ring = { n: ST.calcRing.length, newest_seq: (ST.calcRing[ST.calcRing.length - 1] || {}).seq,
                       oldest_seq: (ST.calcRing[0] || {}).seq };
            const cnt = { same_ptr: 0, same_len: 0, same_digest: 0 };
            for (const ca of ST.calcRing) {
              const sp = (ca.ptr === o.rdx), sl = (ca.len === o.n), sd = (ca.digest_xy === o.digest_xy);
              if (sp) cnt.same_ptr++;
              if (sl) cnt.same_len++;
              if (sd && sl) cnt.same_digest++;
              if (!sp && !sd && !(sl && ca.bytes === null)) continue;   // 只要"有线索"的条目
              const r = { ret: ca.ret, seq: ca.seq, t: ca.t, ptr: ca.ptr, len: ca.len,
                          same_ptr: sp, same_len: sl, same_digest: sd, bytes_kept: !!ca.bytes,
                          d_seq: ca.seq };
              if (ca.bytes && sl) {                       // 长度相同 ⇒ 整条逐字节比
                const cm = cmpArray(bytes, ca.bytes);
                r.same = cm.same; r.diffPoints = cm.diffPoints; r.firstDiff = cm.firstDiff;
              } else if (ca.bytes && ca.len > o.n) {      // 采样器数组更长 ⇒ 队列项可能是它的**前 n 点**
                const cm = cmpPrefix(bytes, ca.bytes, o.n);
                r.prefix_same = cm.same; r.prefix_diffPoints = cm.diffPoints;
                r.prefix_firstDiff = cm.firstDiff;
                r.calc_last = hexPoint(ca.bytes, ca.len - 1);
                r.pos_last = o.pos_bytes;
                r.truncated_from_len = ca.len;
                r.same = false;                           // 整条不相等（长度本来就不同）
                r.via = 'prefix';
              } else if (sd && !ca.bytes) {
                r.same = sl; r.via = 'digest(distinct buffer, bytes evicted)';
              }
              o.vs_calc.push(r);
            }
            o.vs_counts = cnt;
          } else { o.skipped = 'unreadable-or-too-long'; }
        } catch (e) { o.error = String(e); }
        // ★ 环回吐（含每个 calc 的 token 列表），让宿主能在"这一发"上做三样对齐
        try {
          o.recent = { fires: ST.fireRing.slice(-8), calcs: ST.calcRing.map((x) => ({
            seq: x.seq, t: x.t, ret: x.ret, rax: x.rax, ptr: x.ptr, len: x.len,
            digest_xy: x.digest_xy, pts: x.pts, bytes_kept: !!x.bytes, fire_seq: x.fire_seq })) };
          if (ST.lastFn) o.last_fn = { seq: ST.lastFn.seq, t: ST.lastFn.t,
                                      start_x: ST.lastFn.start_x, end_x: ST.lastFn.end_x,
                                      equal_x: ST.lastFn.end_x === (o.pos_bytes || '').substr(0, 16) };
        } catch (e) { o.recentErr = String(e); }
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'lean_pos_read_0x472865' });
  } catch (e) { res.hooks.push({ name: 'lean_pos_read_0x472865', error: String(e) }); }

  // ── ③ `EffectInfo` 装配点 A/B（每发一次）──────────────────────────────────────────
  for (const [rva, tag] of [[RVA.eff_A, 'A_0x4729d6'], [RVA.eff_B, 'B_0x472e4d']]) {
    try {
      Interceptor.attach(base.add(rva), {
        onEnter() {
          const c = this.context;
          const o = { type: 'eff_payload', lean: true, site: tag, seq: ++ST.effPays, t: now(),
                      rsi: hx(c.rsi), ret: rvaOf(c.rsp.readPointer()) };
          try {
            const p = c.rsi;
            o.payload_hex = bytesHex(p, 0x28);
            o.pos_x_bits = bytesHex(p.add(8), 8);
            o.pos_y_bits = bytesHex(p.add(0x10), 8);
            o.pos_x = p.add(8).readDouble();
            o.pos_y = p.add(0x10).readDouble();
            o.effect_id = p.add(0x18).readU32();
            o.start_tick = p.add(0x1c).readU32();
            o.effect_type = p.add(0x20).readU8();
          } catch (e) { o.error = String(e); }
          try { send(o); } catch (e) { }
        },
      });
      res.hooks.push({ name: 'lean_eff_payload_' + tag });
    } catch (e) { res.hooks.push({ name: 'lean_eff_payload_' + tag, error: String(e) }); }
  }

  // ── ④ `EffectInfo` → JSON（每发一次）──────────────────────────────────────────────
  try {
    Interceptor.attach(base.add(RVA.eff_json), {
      onEnter() {
        // rdi/rsi 都是调用者保存：必须在 onEnter 抄下来（`frida_point_array.js` §66 的第一版就错在这）
        this.w = this.context.rdi;
        this.pay = this.context.rsi;
        this.len0 = -1;
        try {
          const s = this.w.readPointer();
          this.sPtr = s.add(8).readPointer();
          this.len0 = Number(s.add(0x10).readU64().toString());
        } catch (e) { this.errIn = String(e); }
      },
      onLeave() {
        const o = { type: 'eff_json', lean: true, seq: ++ST.effJsons, t: now() };
        try {
          const p = this.pay;
          o.effect_id = p.add(0x10).readU32();
          o.start_tick = p.add(0x14).readU32();
          o.pos_x_bits = bytesHex(p, 8);
          o.pos_y_bits = bytesHex(p.add(8), 8);
          o.payload_hex = bytesHex(p, 0x20);
        } catch (e) { o.payErr = String(e); }
        try {
          const s = this.w.readPointer();
          const len = Number(s.add(0x10).readU64().toString());
          const ptr = s.add(8).readPointer();
          o.len_before = this.len0; o.len_after = len;
          const n = len - this.len0;
          if (this.len0 >= 0 && n > 0 && n <= CFG.jsonMax) o.text = ptr.add(this.len0).readUtf8String(n);
          else o.note = 'len delta out of range';
        } catch (e) { o.error = String(e); }
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'lean_eff_json_0x50f990' });
  } catch (e) { res.hooks.push({ name: 'lean_eff_json_0x50f990', error: String(e) }); }

  // ── ⑤ `0x3c5f70`：`c = y0 - f(∓x0)` 印出来的串 + 它吃到的 `&Point`（每发一次）──────────
  try {
    Interceptor.attach(base.add(RVA.suffix), {
      onEnter() {
        this.o = { type: 'tok_suffix', lean: true, seq: ++ST.suffixes, t: now(),
                   ret: rvaOf(this.context.rsp.readPointer()), sret: hx(this.context.rdi),
                   flip: (Number(this.context.r8.toString()) & 0xff) };
        try {
          const p = this.context.rcx;                     // &(x0,y0)
          if (p && !p.isNull()) { this.o.x0 = bytesHex(p, 8); this.o.y0 = bytesHex(p.add(8), 8); }
        } catch (e) { }
      },
      onLeave(retval) {
        const o = this.o; if (!o) return;
        try {
          const srt = vecTriple(retval || ptr(o.sret));
          o.len = srt.len;
          if (srt.ptr && !srt.ptr.isNull() && srt.len >= 0 && srt.len <= 64) {
            o.text = srt.ptr.readUtf8String(srt.len);
            o.hex = bytesHex(srt.ptr, srt.len);
          }
        } catch (e) { o.error = String(e); }
        // 输入框那条路（`ret=gw+4f28ac`）每敲一个键就一条；只留前 `suffixOther` 条，
        // 开火那条路（`ret=gw+3c5da0`）一条不漏。
        const isFire = o.ret === 'gw+3c5da0';
        if (isFire) ST.suffixesFire++;
        if (isFire || ST.suffixes <= CFG.suffixOther) { try { send(o); } catch (e) { } }
      },
    });
    res.hooks.push({ name: 'lean_suffix_0x3c5f70' });
  } catch (e) { res.hooks.push({ name: 'lean_suffix_0x3c5f70', error: String(e) }); }

  // ── ⑥ `FunctionPointsEvent` → JSON：报文里的 `start_x/start_y/end_x/owner_id`（每发一次）──
  try {
    Interceptor.attach(base.add(RVA.fnpoints_ser), {
      onEnter() {
        const o = { type: 'fnpoints', lean: true, seq: ++ST.fnpoints, t: now(),
                    ret: rvaOf(this.returnAddress) };
        try {
          const ev = this.context.rsi;
          o.ev_ptr = hx(ev);
          o.start_x = bytesHex(ev, 8);
          o.start_y = bytesHex(ev.add(8), 8);
          o.end_x = bytesHex(ev.add(0x10), 8);
          try { o.owner_id = ev.add(0x18).readS64().toString(); } catch (e) { o.owner_id = null; }
        } catch (e) { o.error = String(e); }
        ST.lastFn = o;
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'lean_fnpoints_ser_0x50eb20' });
  } catch (e) { res.hooks.push({ name: 'lean_fnpoints_ser_0x50eb20', error: String(e) }); }

  ST.ins = true;
  res.dropped = ['evaluate_0x4eb9d0', 'parser_0x4ed670', 'exec_queue_0x4724cc', 'fnpoints_ctor_0x456810',
                 'realloc', 'evals_rows', 'ring_yMap_pairMap'];
  try { res.selftest = selfTestCompare(); } catch (e) { res.selftest = { error: String(e) }; }
  try { send(res); } catch (e) { }
}

// 驱动（`gw2_step_drive.py --triple`）仍会 post 一条 cfg；本探针**永远开着**，只回 ack 便于日志对齐
recv('cfg', (m) => {
  try {
    if (m && typeof m.jsonMax === 'number') CFG.jsonMax = m.jsonMax;
    if (m && typeof m.sendFirst === 'number') CFG.sendFirst = m.sendFirst;
    send({ type: 'cfg_ack', lean: true, ignored: m || {}, sendFirst: CFG.sendFirst,
           keepCalcs: CFG.keepCalcs, ringCalcs: CFG.ringCalcs });
  } catch (e) { send({ type: 'cfg_ack', lean: true, error: String(e) }); }
});

setInterval(() => {
  if (ST.ins) return;
  const m = Process.enumerateModules().find((x) => /graphwarrust/i.test(x.name));
  if (!m) return;
  try { install(m.base); } catch (e) { send({ type: 'err', msg: String(e) }); ST.ins = true; }
}, 50);

setInterval(() => {
  try {
    send({ type: 'beat', lean: true, t: now(), ins: ST.ins, samplers: ST.samplers,
           suffixes: ST.suffixes, suffixesFire: ST.suffixesFire, posReads: ST.posReads,
           effPays: ST.effPays, effJsons: ST.effJsons, fnpoints: ST.fnpoints,
           ringFires: ST.fireRing.length, ringCalcs: ST.calcRing.length });
  } catch (e) { }
}, 2000);
