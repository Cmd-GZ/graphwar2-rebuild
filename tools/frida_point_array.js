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

// ═══════════════════════════════════════════════════════════════════════════════
// 2026-11-08 `dispatch/39`：开火那一刻**真正喂进** `calculate_function` 的 token 列表
//
// 为什么单独一块：并行任务 40 也在改本文件。本轮新增的东西**全部**收在这里
// （`TOKCFG` + `installTokenProbe()` + 三个 `tok_*` 消息），主探针一行没动，
// 开关是 `TOKCFG.on`（默认 **关**；驱动 `--tokens` 用 `post({type:'cfg',tokenProbe:true})` 打开）。
//
// 地址（构建同文件头：`libgraphwarrust.so` sha256 9a538115…）：
//   0x3c2530  calculate_function（采样器）  [0x3c2530, 0x3c2bb9)
//             **签名（序言逐条读出，见 docs/06 §45.2）**
//               rdi = sret（0x20 字节的 `{kind, ptr, len, +0x18}` 记录）
//               rsi = `&Vec<Token>`（`{cap@+0, ptr@+8, len@+0x10}`，token 步长 16 B）
//               edx = flip(u8)          ecx = mode(u8)
//               xmm0 = x0  xmm1 = y0  xmm2 = step
//             体内 6 处 `call 0x4eb9d0`（求值器）都用 `rdi = 这个 rsi`：`(cap, ptr, len)` 布局成立。
//   0x4ed670  `MathFunction::new`（解析器）= 槽 `0x9bacf8` 的目标
//             rdi = sret `&mut Vec<Token>`；rsi = `&String` 输入文本（`{cap@+0, ptr@+8, len@+0x10}`）
//             ⇒ 这是**唯一**能读到"喂给采样器的那串文本"的位置（`docs/119` §1 同布局）。
//             全 `.text` 只有 5 个 `call *0x9bacf8(%rip)` 调用点：0x3c5ffd（offset_str 内）、
//             0x455ed8 / 0x455f4e（设函数）、0x51ba93 / 0x51beff；**采样器那条链上没有直接 parse**，
//             所以"token 是不是由某一次 parse 产出"只能靠**运行时**（ptr 恒等 + 逐字节）判。
//   0x3c5f70  `calculate_function_offset_str`（`c = y0 - f(∓x0)` → `" + 1.916"`）
//             rdi = sret `&mut String`（0x3c6189 `mov %rbx,0x10(%r13)` 写回）；rsi/rdx = 文本+buf
//             rcx = `&(x0,y0)`（`0x3c6003 movsd (%r14)` = x0、`0x3c602f movsd 0x8(%r14)` = y0）
//             r8d = flip。**onLeave 读 sret** 就拿到真机自己印的那个后缀串。
// ═══════════════════════════════════════════════════════════════════════════════
const TOKCFG = {
  on: false,          // ★ 总开关（默认关：不加 --tokens 的跑法行为与之前**逐字节相同**）
  maxTokens: 256,     // 单个 token 列表最多拷多少 token（16 B/token）
  parseKeep: 256,     // 记住最近多少次 parse（token 列表可能是很久以前 parse 出来的）
  textMax: 4096,      // 单个文本最多记多少字节
  suffixRows: 400,    // `tok_suffix` 最多留多少行
};
const TOKST = {
  seq: 0, fires: 0, parses: 0, suffixes: 0, pSeq: 0,
  ring: [],           // [{seq, t, ret, sret, text, out:{cap,ptr,len,hex,tags}}]
  rows: [],
};

// ═══════════════════════════════════════════════════════════════════════════════
// 2026-11-08 `dispatch/44`：`FunctionPoints.end_x` 的**装配/序列化处**
//
// 为什么单独一块：并行任务 45 也在动本文件；本轮新增的东西**全部**收在这里
// （`FPCFG` / `FPST` / `installFunctionPointsProbe()` / `fnpoints` / `pos_x_locate` 两条消息），
// 开关是 `FPCFG.on`（默认 **关**；驱动 `--fnpoints` 用 `post({type:'cfg',fnpoints:true})` 打开），
// 主探针与 token 探针一行没动。
//
// 地址（构建同文件头：`libgraphwarrust.so` sha256 9a538115…）：
//   0x50eb20  `FunctionPointsEvent` → JSON 的**写入器**（手写 serde，逐条指令读出）
//               rdi = 写入器（`{cap@+0, ptr@+8, len@+0x10}` 的 String 包装）
//               rsi = ★ `&FunctionPointsEvent`
//                     `start_x` f64 @+0x00、`start_y` f64 @+0x08、`end_x` f64 @+0x10、
//                     `owner_id` i64 @+0x18      （依据：0x50ebaf/0x50ebf0/0x50ec0f/0x50ec13
//                                                依次 `lea 0x18(%r14)` / `%r14` / `0x8(%r14)` / `0x10(%r14)`）
//               键名来自 rodata：`FunctionPoints`@0x1c55e9(0xe)、`start_x`@0x1c506b(7)、
//                     `start_y`@0x1c5072(7)、`end_x`@0x1c5079(5)、`owner_id`@0x19f6c8(8)
//               调用者：唯一直接调用点 `0x5046d3`（事件枚举 → JSON 的派发器 `0x504640`，
//                     判据 37 = FunctionPoints，读 `0x1c48d0` 的 62 项跳表第 37 项 = `0x5046c9`）
//   ⇒ 在这一处停下时，`end_x` 这 8 个字节就是**报文里那个值**（已定型、后面不再改）。
//
// 要回答的三件事只能靠"同时"看两处：
//   ① 报文 `end_x` 的 8 字节；② `0x472865` 那次读的 `pos` 16 字节；③ 每次 `calculate_function`
//   返回的整条点数组（主探针已经在 `calc_array` 里留了字节副本）。所以这里对 `end_x`
//   逐条数组做"**在第几个点上**"的定位（x 分量 8 字节逐字节比，不是比摘要）。
// ═══════════════════════════════════════════════════════════════════════════════
const FPCFG = {
  on: false,          // ★ 总开关（默认关：不加 --fnpoints 的跑法行为与之前**逐字节相同**）
  rva: {
    fnpoints_ctor: 0x456810,  // ★ FunctionPointsEvent 的装配（读 `pts[len-1].x` → end_x）
    fnpoints_ser: 0x50eb20,   // FunctionPointsEvent → JSON（rsi = &event）
    pos_read: 0x472865,       // 与主探针同一处 `movdqu`（这里再挂一个**只读**监听，用于"点定位"）
  },
  idxMax: 12,         // 每条数组最多报几个"命中的下标"
  arrMax: 8,          // 最多对多少条数组做定位（= 主探针的 calcKeep）
  rowsMax: 400,       // `fnpoints` 最多留多少行
  btDepth: 20,
};
const FPST = {
  seq: 0, posSeq: 0, ctorSeq: 0, rows: [], lastFn: null, lastCtor: null,
};

/** 点数组第 i 点的 x（**前 8 字节原始字节序**的十六进制，32→16 字符）；越界返回 null */
function pointXHex(bytes, i) {
  try {
    if (!bytes || i < 0 || i * 16 + 16 > bytes.length) return null;
    let s = '';
    for (let j = 0; j < 8; j++) s += ('0' + bytes[i * 16 + j].toString(16)).slice(-2);
    return s;
  } catch (e) { return null; }
}
/** 在一条数组里找"x 位模式 == xhex"的所有下标（最多取 max 个）——**逐字节**比，不比摘要 */
function findXIdx(bytes, len, xhex, max) {
  const out = [];
  if (!bytes || !xhex || xhex.length !== 16) return out;
  const want = [];
  for (let j = 0; j < 8; j++) want.push(parseInt(xhex.substr(j * 2, 2), 16));
  for (let i = 0; i < len; i++) {
    let hit = true;
    for (let j = 0; j < 8; j++) if (bytes[i * 16 + j] !== want[j]) { hit = false; break; }
    if (hit) { out.push(i); if (out.length >= max) break; }
  }
  return out;
}
/** 把一条数组与一个 16 字节的 `pos` 对比：x 命中哪些下标、y 命中哪些下标、x&y 同下标命中 */
function locatePoint(bytes, len, posHex16, max) {
  const o = { x_idx: [], y_idx: [], xy_idx: [] };
  if (!bytes || !posHex16 || posHex16.length !== 32) return o;
  const xh = posHex16.substr(0, 16), yh = posHex16.substr(16, 16);
  o.x_idx = findXIdx(bytes, len, xh, max);
  // y 分量：每点后 8 字节
  const wantY = [];
  for (let j = 0; j < 8; j++) wantY.push(parseInt(yh.substr(j * 2, 2), 16));
  for (let i = 0; i < len; i++) {
    let hit = true;
    for (let j = 0; j < 8; j++) if (bytes[i * 16 + 8 + j] !== wantY[j]) { hit = false; break; }
    if (hit) { o.y_idx.push(i); if (o.y_idx.length >= max) break; }
  }
  for (const i of o.x_idx) if (o.y_idx.indexOf(i) >= 0) o.xy_idx.push(i);
  return o;
}

function installFunctionPointsProbe(base, res) {
  // ⓪ ★ `FunctionPointsEvent` 的**装配处**（`end_x` 真正被读出来的那一条指令在这里面）
  //    fn [0x456810, 0x456b75)：`rdi` = self、`esi` = owner/soldier id、`rdx` = `&Vec<(f64,f64)>`
  //    体内 0x4569b8 `movq -0x10(%rcx,%rax,1),%xmm0`（rcx = vec.ptr、rax = len·16）
  //    = **`pts[len-1].x` ⇒ 写进 `FunctionPointsEvent.end_x`**；
  //    同处 0x4569be `movdqu (%rcx),%xmm1` = `pts[0]` ⇒ `start_x/start_y`；
  //    0x4569ce `mov %r15d,0x30(%rsp)` = owner（**u32**）；
  //    0x4569d3 `lea 0x26(%r8),%rax`（r8=0x7fff…ff）+ 0x4569d7 = 判别式 `0x8000000000000025`
  //    （= 派发器的 37 号 FunctionPoints，见本轮 0x1c48d0 跳表第 37 项 = 0x5046c9）。
  //    两个直接调用点：`0x3e44d4`（采样器 `gw+3e2d3b` 那次返回的那条数组）与 `0x471a88`。
  try {
    Interceptor.attach(base.add(FPCFG.rva.fnpoints_ctor), {
      onEnter() {
        if (!FPCFG.on) return;
        const o = { type: 'fnpoints_ctor', seq: ++FPST.ctorSeq, t: now(),
                    caller: rvaOf(this.returnAddress), bt: btOf(this.context, 8) };
        try {
          o.self = hx(this.context.rdi);
          o.owner_id = this.context.rsi.toUInt32() >>> 0;
          const v = this.context.rdx;
          o.vec_ptr_hdr = hx(v);
          const cap = v.readU64().toString();
          const ap = v.add(8).readPointer();
          const alen = Number(v.add(0x10).readU64().toString());
          o.vec = { cap: cap, ptr: hx(ap), len: alen };
          if (ap && !ap.isNull() && alen > 0 && alen <= CFG.arrayCap && readable(ap)) {
            o.start_pt = bytesHex(ap, 16);                       // = pts[0]（报文 start_x/start_y）
            o.end_pt = bytesHex(ap.add((alen - 1) * 16), 16);    // = pts[len-1]（报文 end_x 就是它的前 8 B）
            o.end_x = o.end_pt.substr(0, 16);
            // 与本次运行里 `calculate_function` 返回过的数组对撞：是哪一条（指针 + 逐字节）
            o.vs_calc = [];
            const bytes = arrBytes(ap, alen);
            for (const ca of ST.calcArrays.slice(-FPCFG.arrMax)) {
              const r = { ret: ca.ret, ptr: hx(ca.ptr), len: ca.len, seq: ca.seq, t: ca.t,
                          same_ptr: hx(ca.ptr) === o.vec.ptr };
              if (ca.bytes && bytes) {
                const c = cmpArrayBytes(bytes, ca.bytes);
                r.same_bytes = c.same; r.diffPoints = c.diffPoints;
              }
              o.vs_calc.push(r);
            }
          } else { o.skipped = 'unreadable-or-empty-or-too-long'; }
        } catch (e) { o.error = String(e); }
        FPST.lastCtor = o;
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'fnpoints_ctor', rva: '0x' + FPCFG.rva.fnpoints_ctor.toString(16) });
  } catch (e) { res.hooks.push({ name: 'fnpoints_ctor', error: String(e) }); }

  // ① `FunctionPointsEvent` 的 JSON 写入器：`end_x` 在这里**定型**
  try {
    Interceptor.attach(base.add(FPCFG.rva.fnpoints_ser), {
      onEnter() {
        if (!FPCFG.on) return;
        const o = { type: 'fnpoints', seq: ++FPST.seq, t: now(),
                    ret: rvaOf(this.returnAddress), bt: btOf(this.context, FPCFG.btDepth) };
        try {
          const ev = this.context.rsi;
          o.ev_ptr = hx(ev);
          o.start_x = bytesHex(ev, 8);
          o.start_y = bytesHex(ev.add(8), 8);
          o.end_x = bytesHex(ev.add(0x10), 8);          // ★ 报文里的 `end_x` 原文
          try { o.owner_id = ev.add(0x18).readS64().toString(); } catch (e) { o.owner_id = null; }
          // 与"本次运行里 `calculate_function` 返回过的数组"逐点定位（x 这 8 字节）
          o.vs_calc = [];
          for (const ca of ST.calcArrays.slice(-FPCFG.arrMax)) {
            const r = { ret: ca.ret, ptr: hx(ca.ptr), len: ca.len, seq: ca.seq, t: ca.t };
            if (ca.bytes) {
              r.last_x = pointXHex(ca.bytes, ca.len - 1);
              r.last_x_eq = r.last_x === o.end_x;
              r.prev_x = pointXHex(ca.bytes, ca.len - 2);
              r.prev_x_eq = r.prev_x === o.end_x;
              r.x_eq_idx = findXIdx(ca.bytes, ca.len, o.end_x, FPCFG.idxMax);
            } else { r.bytes = 'not-kept'; }
            o.vs_calc.push(r);
          }
          FPST.lastFn = o;
        } catch (e) { o.error = String(e); }
        if (FPST.rows.length > FPCFG.rowsMax) FPST.rows.shift();
        FPST.rows.push([o.seq, o.t, o.end_x]);
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'fnpoints_ser', rva: '0x' + FPCFG.rva.fnpoints_ser.toString(16) });
  } catch (e) { res.hooks.push({ name: 'fnpoints_ser', error: String(e) }); }

  // ② 与主探针**同一处** `0x472865`：把这一刻的 `pos` 在每条数组里**定位到下标**
  //    （主探针的 `pos_read` 只报"整条数组是否逐字节相同"，分不出"差一个点"；这里补上）
  try {
    Interceptor.attach(base.add(FPCFG.rva.pos_read), {
      onEnter() {
        if (!FPCFG.on) return;
        const c = this.context;
        const o = { type: 'pos_x_locate', seq: ++FPST.posSeq, t: now(), rdx: hx(c.rdx), rcx: hx(c.rcx) };
        try {
          const n16 = c.rcx.toUInt32() >>> 0;
          o.n = n16 / 16;
          const p = c.rdx;
          if (n16 > 0 && o.n <= CFG.arrayCap && readable(p)) {
            const bytes = arrBytes(p, o.n);
            o.pos_bytes = bytesHex(p.add(n16 - 16), 16);
            o.pos_x = o.pos_bytes ? o.pos_bytes.substr(0, 16) : null;
            if (bytes) {
              o.vs_calc = [];
              for (const ca of ST.calcArrays.slice(-FPCFG.arrMax)) {
                const r = { ret: ca.ret, ptr: hx(ca.ptr), len: ca.len, seq: ca.seq, same_ptr: hx(ca.ptr) === o.rdx };
                if (ca.bytes) {
                  const loc = locatePoint(ca.bytes, ca.len, o.pos_bytes, FPCFG.idxMax);
                  r.x_idx = loc.x_idx; r.y_idx = loc.y_idx; r.xy_idx = loc.xy_idx;
                  r.same_bytes = cmpArrayBytes(bytes, ca.bytes).same;
                } else { r.bytes = 'not-kept'; }
                o.vs_calc.push(r);
              }
            }
            // 与"最近一条 `fnpoints`"直接对撞：报文里 `end_x` vs 这里读到的 `pos.x`
            if (FPST.lastFn) {
              o.last_fn = { seq: FPST.lastFn.seq, t: FPST.lastFn.t, end_x: FPST.lastFn.end_x,
                            equal_x: FPST.lastFn.end_x === o.pos_x };
            }
            // 与"最近一次装配"对撞：装配读的 (ptr,len) 与这里读的 (ptr,len) 是同一次吗
            if (FPST.lastCtor) {
              const lc = FPST.lastCtor;
              o.last_ctor = { seq: lc.seq, t: lc.t, caller: lc.caller,
                              ptr: (lc.vec || {}).ptr, len: (lc.vec || {}).len,
                              same_ptr: (lc.vec || {}).ptr === o.rdx,
                              same_len: (lc.vec || {}).len === o.n,
                              end_x: lc.end_x,
                              equal_x: lc.end_x === o.pos_x,
                              end_eq_pos_point: lc.end_pt === o.pos_bytes };
            }
          }
        } catch (e) { o.error = String(e); }
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'pos_x_locate', rva: '0x' + FPCFG.rva.pos_read.toString(16) });
  } catch (e) { res.hooks.push({ name: 'pos_x_locate', error: String(e) }); }
}

/** Rust 的 `Vec<T>` / `String` 三元组：`{cap@+0, ptr@+8, len@+0x10}`（与 `docs/119` §1 同布局） */
function vecTriple(p) {
  try {
    const cap = Number(p.readU64().toString());
    const vp = p.add(8).readPointer();
    const len = Number(p.add(0x10).readU64().toString());
    return { cap: cap, ptr: vp, len: len };
  } catch (e) { return { error: String(e) }; }
}
/** token 列表的原始字节（16 B/token）+ 判别值序列（tag 是 u64 低 8 字节） */
function tokBytes(v) {
  const o = { len: v.len, ptr: hx(v.ptr), hex: null, tags: null };
  if (!v.ptr || v.ptr.isNull() || !(v.len > 0)) { o.note = 'empty'; return o; }
  if (v.len > TOKCFG.maxTokens) { o.note = 'len>maxTokens'; return o; }
  try {
    const b = new Uint8Array(v.ptr.readByteArray(v.len * 16));
    let s = '';
    for (let i = 0; i < b.length; i++) s += ('0' + b[i].toString(16)).slice(-2);
    o.hex = s;
    o.tags = [];
    for (let i = 0; i < v.len; i++) {
      let t = 0n;
      for (let j = 7; j >= 0; j--) t = (t << 8n) | BigInt(b[i * 16 + j]);
      o.tags.push(Number(t));
    }
  } catch (e) { o.error = String(e); }
  return o;
}
/** 两条 token 列表**逐字节**比（hex 是规范化的，`===` 就是 memcmp） */
function tokCmp(a, b) {
  if (!a || !b) return { same: null, reason: 'null side' };
  if (!a.hex || !b.hex) return { same: null, reason: 'no bytes' };
  if (a.len !== b.len) return { same: false, reason: 'len differs', nA: a.len, nB: b.len };
  if (a.hex === b.hex) return { same: true, nA: a.len, nB: b.len };
  const diff = [];
  for (let i = 0; i < a.len && diff.length < 6; i++) {
    const x = a.hex.substr(i * 32, 32), y = b.hex.substr(i * 32, 32);
    if (x !== y) diff.push([i, x, y]);
  }
  return { same: false, reason: 'bytes differ', nA: a.len, nB: b.len, firstDiff: diff };
}
/** 采样器入口那一刻的 16 字节 xmm → 低 8 字节 = 一个 f64 的位串（内存序） */
function _xmm(p) { return xmmLow(p); }

function installTokenProbe(base, res) {
  // ── ① 解析器 `0x4ed670`：文本 → token 列表（**token 的唯一来源**）──────────────
  try {
    Interceptor.attach(base.add(0x4ed670), {
      onEnter() {
        if (!TOKCFG.on) return;
        const c = this.context;
        const str = vecTriple(c.rsi);                      // &String 输入文本
        const rec = { seq: ++TOKST.pSeq, t: now(),
                      ret: rvaOf(c.rsp.readPointer()), sret: hx(c.rdi),
                      txt: null, txtLen: null, out: null };
        try {
          if (str.ptr && !str.ptr.isNull() && str.len >= 0 && str.len <= TOKCFG.textMax) {
            rec.txtLen = str.len;
            rec.txt = str.ptr.readUtf8String(str.len);
            rec.txtHex = bytesHex(str.ptr, str.len);       // ★ 逐字节（含大小写/空格）
          } else { rec.txtSkip = 'len>textMax or unreadable'; rec.txtLen = str.len; }
        } catch (e) { rec.txtErr = String(e); }
        this.rec = rec;
      },
      onLeave(retval) {
        if (!TOKCFG.on) return;
        const rec = this.rec; if (!rec) return;
        try {
          const out = vecTriple(retval || ptr(rec.sret));
          rec.out = tokBytes(out);
          rec.out.cap = out.cap;
          TOKST.parses++;
          TOKST.ring.push(rec);
          while (TOKST.ring.length > TOKCFG.parseKeep) TOKST.ring.shift();
          send({ type: 'tok_parse', t: now(), rec: rec });
        } catch (e) { send({ type: 'tok_parse', t: now(), rec: rec, error: String(e) }); }
      },
    });
    res.hooks.push({ name: 'tok_parse_0x4ed670' });
  } catch (e) { res.hooks.push({ name: 'tok_parse_0x4ed670', error: String(e) }); }

  // ── ② 采样器 `0x3c2530`：**开火那一刻**的实参（token 列表 + x0/y0/step/flip/mode）──
  try {
    Interceptor.attach(base.add(0x3c2530), {
      onEnter() {
        if (!TOKCFG.on) return;
        const c = this.context;
        const o = { type: 'tok_fire', seq: ++TOKST.fires, t: now(),
                    ret: rvaOf(c.rsp.readPointer()), sret: hx(c.rdi),
                    flip: (Number(c.rdx.toString()) & 0xff),
                    mode: (Number(c.rcx.toString()) & 0xff),
                    x0: xmmLow(c.xmm0), y0: xmmLow(c.xmm1), step: xmmLow(c.xmm2) };
        try {
          const vec = vecTriple(c.rsi);
          o.vec = { cap: vec.cap, ptr: hx(vec.ptr), len: vec.len };
          o.tok = tokBytes(vec);
          // 与环里**每一次** parse 比：ptr 恒等 + 逐字节（这是"token 从哪来"的唯一运行时证据）
          o.vs_parse = [];
          for (const r of TOKST.ring) {
            if (!r.out) continue;
            const ptrEq = r.out.ptr && vec.ptr && !vec.ptr.isNull() &&
                          ((r.out.ptr === o.vec.ptr) || hx(r.out.ptr) === o.vec.ptr);
            const cmp = tokCmp(r.out, o.tok);
            if (ptrEq || (cmp.same === true)) {
              o.vs_parse.push({ seq: r.seq, ret: r.ret, sret: r.sret, t: r.t,
                                ptr_eq: !!ptrEq, same_bytes: cmp.same,
                                text: r.txt, txtHex: r.txtHex, txtLen: r.txtLen,
                                parse_out: r.out, firstDiff: cmp.firstDiff });
            }
          }
          if (!o.vs_parse.length) o.vs_parse_note = 'no parse in ring matches this token list';
        } catch (e) { o.error = String(e); }
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'tok_fire_0x3c2530' });
  } catch (e) { res.hooks.push({ name: 'tok_fire_0x3c2530', error: String(e) }); }

  // ── ③ 修正常数的**文本**：`calculate_function_offset_str` 0x3c5f70 的返回值 ─────
  // 这是"回放里 corrected_function 的后缀"唯一的真机出处（`{:.3}` 印出来的那 3 位小数）。
  try {
    Interceptor.attach(base.add(0x3c5f70), {
      onEnter() {
        if (!TOKCFG.on) return;
        this.o = { type: 'tok_suffix', seq: ++TOKST.suffixes, t: now(),
                   ret: rvaOf(this.context.rsp.readPointer()), sret: hx(this.context.rdi),
                   flip: (Number(this.context.r8.toString()) & 0xff) };
        try {
          const p = this.context.rcx;                         // &(x0,y0)
          if (p && !p.isNull()) { this.o.x0 = bytesHex(p, 8); this.o.y0 = bytesHex(p.add(8), 8); }
        } catch (e) { }
      },
      onLeave(retval) {
        if (!TOKCFG.on) return;
        const o = this.o; if (!o) return;
        try {
          const srt = vecTriple(retval || ptr(o.sret));
          o.len = srt.len;
          if (srt.ptr && !srt.ptr.isNull() && srt.len >= 0 && srt.len <= 64) {
            o.text = srt.ptr.readUtf8String(srt.len);
            o.hex = bytesHex(srt.ptr, srt.len);
          }
        } catch (e) { o.error = String(e); }
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'tok_suffix_0x3c5f70' });
  } catch (e) { res.hooks.push({ name: 'tok_suffix_0x3c5f70', error: String(e) }); }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2026-11-08 `dispatch/48`：**两装配点同时抓** + 抓"谁把长度改小"
//
// 为什么单独一块：并行任务 49 也在动本文件。本轮新增的东西**全部**收在这里
// （`DVCFG` / `DVST` / `installDivergeProbe()` / `dv_*` 五条消息），开关是 `DVCFG.on`
// （默认 **关**；驱动 `--diverge` 用 `post({type:'cfg',diverge:true})` 打开），主探针 /
// token 探针 / FunctionPoints 探针一行没动。
//
// 这一块要回答的两件事（任务书 `dispatch/48`）：
//   ① `0x456810`（`end_x` 的装配点，两个调用者 `gw+471a8d` / `gw+3e44d9`）喂进去的那条数组，
//      与 `0x472865`（`pos` 读点）读的那条数组，**在同一发里**是不是同一份内容？
//      → 在 ctor 那一刻**整条拷字节**，在 pos 那一刻**逐点逐字节比**（差 1 bit 也报）。
//   ② `0x472865` 读到的 `n` 是被谁改小的？
//      → 在 `calculate_shot_results`（`0x3c2bc0`）返回时把它的**返回结构**（0x20 字节）与它
//        指向的那条 `Vec<Item>`（0x20 字节/项：kind@+0、ptr@+8/+0xc 两个 32 位半、len@+0x10、
//        owner@+0x18）整条 dump 出来；配合 `calculate_function`（0x3c2530）返回时的 `n`，
//        就能把"5098 → 1528"夹在 [采样器返回, calculate_shot_results 返回] 这个区间里。
//
// 地址（构建同文件头：`libgraphwarrust.so` sha256 9a538115…）：
//   0x456810  FunctionPointsEvent 装配（fn [0x456810,0x456b75)）：rdx = &Vec<(f64,f64)>
//   0x472865  execute_results 读 `pos = pts[n-1]`（rdx = 点数组基址、rcx = n*16）
//   0x3c2bc0  `GraphwarShotCalculator::calculate_shot_results`（fn [0x3c2bc0,0x3c5c4d)）
//             rdi = sret（0x20 字节）、rsi = &params（+0x20 是第一步指针）
//             唯一直接调用点是槽 `0x9bacb0` 的两处：`0x471825`（game_state_manager）与 `0x51a9a1`
//             返回后 `0x4718a1` 起把 sret 当 `Vec`（cap@+0、ptr@+8、len@+0x10）拷进 self+0x30，
//             再逐项（0x20 B）`malloc(len*16)+memcpy` 然后把**副本**喂给 `0x456810` ⇒
//             `end_x` 读的就是这个 item 的长度。
//   0x3c2530  采样器 `calculate_function`（fn [0x3c2530,0x3c2bb9)）；`0x3c30c7` 是它在本函数里的返回点
// ═══════════════════════════════════════════════════════════════════════════════
const DVCFG = {
  on: false,           // ★ 总开关（默认关：不加 --diverge 的跑法行为与之前**逐字节相同**）
  rva: {
    fnpoints_ctor: 0x456810,
    pos_read: 0x472865,
    csr: 0x3c2bc0,     // calculate_shot_results
  },
  ctorKeep: 6,         // 记住最近几次 ctor 喂进去的完整数组副本（每份 ≤ arrayCap 点）
  itemsMax: 24,        // `dv_csr` 最多 dump 结果 Vec 的多少项
  arrMax: 20000,       // 单条数组最多拷多少点（16 B/点）——与 CFG.arrayCap 独立
  mam: false,          // 页级写监视（`--dv-mam`）：item push 之后武装一次，抓"紧接着的那次访问"
  rowsMax: 200,
};
const DVST = { ctorSeq: 0, posSeq: 0, csrSeq: 0, ctors: [], rows: [] };

/** 一条点数组的完整快照：摘要 + 首末点 + **原始字节**（判"逐字节相同"只认字节，不认摘要） */
function dvSnap(p, len) {
  const o = { ptr: hx(p), len: len };
  if (!p || p.isNull() || !(len > 0) || len > DVCFG.arrMax || !readable(p)) {
    o.note = 'unreadable-or-empty-or-too-long'; return o;
  }
  try {
    const dg = digestOf(p, len);
    o.digest_xy = dg.xy; o.digest_y = dg.y;
    o.first = bytesHex(p, 16);
    o.last = bytesHex(p.add((len - 1) * 16), 16);
    o.bytes = arrBytes(p, len);
  } catch (e) { o.error = String(e); }
  return o;
}

/** 两条数组的**前 np 点**逐点逐字节比（长度不同也能比；`cmpArrayBytes` 要求等长，这里补上） */
function dvCmpPrefix(a, b, np) {
  const o = { points: np, diffPoints: 0, firstDiff: [] };
  if (!a || !b) { o.reason = 'null side'; return o; }
  const lim = Math.min(np, Math.floor(a.length / 16), Math.floor(b.length / 16));
  o.points = lim;
  for (let i = 0; i < lim; i++) {
    let eq = true;
    for (let j = 0; j < 16; j++) if (a[i * 16 + j] !== b[i * 16 + j]) { eq = false; break; }
    if (!eq) {
      o.diffPoints++;
      if (o.firstDiff.length < 6) o.firstDiff.push([i, hexPoint(a, i), hexPoint(b, i)]);
    }
  }
  return o;
}

/** 0x20 字节的队列/结果项 → {kind, ptr, len, owner, extra}（ptr 存成 +0x8/+0xc 两个 32 位半） */
function dvItem(raw) {
  const o = { raw: raw };
  if (!raw || raw.length !== 64) return o;
  try {
    const be = (h) => h.match(/../g).reverse().join('');            // 小端 hex → 大端 hex
    const u = (h) => BigInt('0x' + be(h));
    // 布局（`execute_results` 0x4727e1 逐条读出：`mov 0x8(%rax),%r15d` / `mov 0xc(%rax),%r12d`
    //        / `shl $0x20,%r12` / `or %r15,%r12` ⇒ 指针 = (+0xc<<32)|(+0x8)，两个 32 位半）
    o.kind = '0x' + be(raw.substr(0, 16));                          // +0x00 u64
    const lo = u(raw.substr(16, 8)), hi = u(raw.substr(24, 8));     // +0x08 / +0x0c 两个 u32
    o.ptr = '0x' + (((hi << 32n) | lo).toString(16));               // 拼出来的指针
    o.len = u(raw.substr(32, 16)).toString();                       // +0x10 u64
    o.owner = Number(u(raw.substr(48, 8)));                         // +0x18 u32
    o.extra = '0x' + be(raw.substr(56, 8));                         // +0x1c u32
    o.alt = { f64_at_8: null };                                     // 另一种变体：+0x8 起是 f64
    try {
      const dv = new DataView(new Uint8Array(raw.match(/../g).map((x) => parseInt(x, 16))).buffer);
      o.alt.f64_at_8 = dv.getFloat64(8, true);
      o.alt.f64_at_16 = dv.getFloat64(16, true);
    } catch (e2) { }
  } catch (e) { o.error = String(e); }
  return o;
}

function installDivergeProbe(base, res) {
  // ── ① ctor 那一刻：把喂进去的**整条数组**留一份字节副本 ─────────────────────
  try {
    Interceptor.attach(base.add(DVCFG.rva.fnpoints_ctor), {
      onEnter() {
        if (!DVCFG.on) return;
        const o = { type: 'dv_ctor', seq: ++DVST.ctorSeq, t: now(), caller: rvaOf(this.returnAddress) };
        try {
          const v = this.context.rdx;
          const cap = Number(v.readU64().toString());
          const p = v.add(8).readPointer();
          const len = Number(v.add(0x10).readU64().toString());
          o.vec = { cap: cap, ptr: hx(p), len: len };
          o.owner_id = this.context.rsi.toUInt32() >>> 0;
          const snap = dvSnap(p, len);
          o.arr = { ptr: snap.ptr, len: snap.len, digest_xy: snap.digest_xy, digest_y: snap.digest_y,
                    first: snap.first, last: snap.last, note: snap.note, error: snap.error };
          DVST.ctors.push({ t: o.t, caller: o.caller, ptr: snap.ptr, len: snap.len,
                            bytes: snap.bytes, digest_xy: snap.digest_xy });
          while (DVST.ctors.length > DVCFG.ctorKeep) DVST.ctors.shift();
          o.kept = DVST.ctors.length;
        } catch (e) { o.error = String(e); }
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'dv_fnpoints_ctor', rva: '0x' + DVCFG.rva.fnpoints_ctor.toString(16) });
  } catch (e) { res.hooks.push({ name: 'dv_fnpoints_ctor', error: String(e) }); }

  // ── ② pos 那一刻：与**每个** ctor 快照逐点比（差 1 bit 也报）──────────────────
  try {
    Interceptor.attach(base.add(DVCFG.rva.pos_read), {
      onEnter() {
        if (!DVCFG.on) return;
        const c = this.context;
        const o = { type: 'dv_pos', seq: ++DVST.posSeq, t: now(), rdx: hx(c.rdx), rcx: hx(c.rcx) };
        try {
          const n16 = c.rcx.toUInt32() >>> 0;
          const n = n16 / 16;
          o.n = n; o.rcx_mod16 = n16 % 16;
          const p = c.rdx;
          if (n16 > 0 && n <= DVCFG.arrMax && readable(p)) {
            const pos = bytesHex(p.add(n16 - 16), 16);
            o.pos_bytes = pos;
            o.pos_x = pos ? pos.substr(0, 16) : null;
            o.pos_y = pos ? pos.substr(16, 16) : null;
            const bytes = arrBytes(p, n);
            o.vs_ctor = [];
            for (const k of DVST.ctors) {
              const r = { t: k.t, caller: k.caller, ptr: k.ptr, len: k.len,
                          same_ptr: k.ptr === o.rdx, same_len: k.len === n };
              if (k.bytes && bytes) {
                const cp = dvCmpPrefix(bytes, k.bytes, Math.min(k.len, n));
                r.prefix_points = cp.points;
                r.prefix_diff = cp.diffPoints;
                r.firstDiff = cp.firstDiff;
                r.last_point_eq = (hexPoint(bytes, n - 1) === hexPoint(k.bytes, k.len - 1));
                const loc = locatePoint(k.bytes, k.len, pos, 6);
                r.pos_x_idx_in_ctor = loc.x_idx;
                r.pos_xy_idx_in_ctor = loc.xy_idx;
              } else { r.bytes = 'not-kept'; }
              o.vs_ctor.push(r);
            }
            if (FPST.lastFn) {
              o.wire = { seq: FPST.lastFn.seq, t: FPST.lastFn.t, end_x: FPST.lastFn.end_x,
                         equal_x: FPST.lastFn.end_x === o.pos_x };
            }
            o.digest_xy = null;
            try { const dg = digestOf(p, n); o.digest_xy = dg.xy; } catch (e) { }
          } else { o.note = 'unreadable-empty-or-too-long'; }
        } catch (e) { o.error = String(e); }
        if (DVST.rows.length > DVCFG.rowsMax) DVST.rows.shift();
        DVST.rows.push([o.seq, o.t, o.n, o.pos_x]);
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'dv_pos', rva: '0x' + DVCFG.rva.pos_read.toString(16) });
  } catch (e) { res.hooks.push({ name: 'dv_pos', error: String(e) }); }

  // ── ③ `calculate_shot_results` 返回：把返回结构 + 结果 Vec 的每一项 dump 出来 ──
  //    这一处是回答"长度是被谁改小的"的关键：如果返回的 item 里已经有 `{ptr=P, len=1528}`，
  //    那么"改小"**不是** `execute_results` 里改的，而是这个函数里算出来的（区间就此收紧）。
  try {
    Interceptor.attach(base.add(DVCFG.rva.csr), {
      onEnter() {
        if (!DVCFG.on) return;
        this.t = now();
        this.sret = this.context.rdi;
        try {
          send({ type: 'dv_csr_enter', t: this.t, sret: hx(this.context.rdi), params: hx(this.context.rsi),
                 step: bytesHex(this.context.rsi.add(0x00), 8), mode: bytesHex(this.context.rsi.add(0x08), 8) });
        } catch (e) { }
      },
      onLeave(retval) {
        if (!DVCFG.on) return;
        const o = { type: 'dv_csr', t: now(), t_enter: this.t, rax: hx(retval), sret: hx(this.sret) };
        try {
          const s = (retval && !retval.isNull()) ? retval : this.sret;
          const w = [];
          for (let i = 0; i < 4; i++) w.push(bytesHex(s.add(i * 8), 8));
          o.sret_words = w;
          const cap = Number(s.readU64().toString());
          const vp = s.add(8).readPointer();
          const len = Number(s.add(0x10).readU64().toString());
          o.vec_guess = { cap: cap, ptr: hx(vp), len: len };
          if (vp && !vp.isNull() && readable(vp) && len > 0 && len <= 8192) {
            const items = [];
            const nmax = Math.min(len, DVCFG.itemsMax);
            for (let i = 0; i < nmax; i++) {
              const it = vp.add(i * 0x20);
              const rec = dvItem(bytesHex(it, 0x20));
              rec.i = i;
              try {
                const pv = rec.ptr ? ptr(rec.ptr) : null;
                const il = rec.len ? Number(rec.len) : 0;
                if (pv && !pv.isNull() && readable(pv) && il > 0 && il <= DVCFG.arrMax) {
                  rec.arr_first = bytesHex(pv, 16);
                  rec.arr_last = bytesHex(pv.add((il - 1) * 16), 16);
                  const dg = digestOf(pv, il);
                  rec.arr_digest_xy = dg.xy;
                }
              } catch (e) { rec.arrErr = String(e); }
              items.push(rec);
            }
            o.items = items;
            o.vec_len = len;
          }
        } catch (e) { o.error = String(e); }
        try { send(o); } catch (e) { }
      },
    });
    res.hooks.push({ name: 'dv_csr', rva: '0x' + DVCFG.rva.csr.toString(16) });
  } catch (e) { res.hooks.push({ name: 'dv_csr', error: String(e) }); }

  // ── ④ "长度是在哪条指令被写进 item 的"：在几个候选**存储点**上下断 ────────────
  //    静态读出（`objdump -d` 0x3c2bc0..0x3c5c4d，见 `dispatch/48` 报告）：
  //      `0x3c2d4f..0x3c2d89`  把一项 0x20 字节的记录 push 进"源 Vec"（ptr@+0x8 取 [rsp+0x100]、
  //                            len@+0x10 取 [rsp+0x1a0]、owner@+0x18 = r12d、flag@+0x1c）
  //      `0x3c30d7/0x3c30e7`   采样器（0x3c30c1 `call *0x9bac98`）返回后立刻把 sret 的
  //                            [1] 存进 [rsp+0x100]、[2] 存进 [rsp+0x1a0] —— 所以"源 item 的 len"
  //                            在**静态上**看起来就是采样器的 n；是否被改小只能靠**运行时**看。
  //      `0x3c50e0..0x3c50eb`  "源 Vec" → "结果 Vec" 的过滤拷贝（`execute_results` 读的就是它）
  //    这几个钩子只读寄存器/栈槽，不改内存。
  const DV_SITES = [
    [0x3c2d5e, 'src_push_kind'], [0x3c2d6a, 'src_push_ptr'], [0x3c2d77, 'src_push_len'],
    [0x3c30d7, 'post_calc_slot100'], [0x3c30e7, 'post_calc_slot1a0'],
    [0x3c50d5, 'copy_idx'], [0x3c50eb, 'copy_len_owner'],
    // ★ 静态定位到的"改 len"那一条：`0x3c3bf7 mov %r10,0x10(%r14)`（配对的下一条项在 `0x3c3c2c`）。
    //   守卫：`cmp %r10,0x10(%r14)` / `jb` ⇒ 只在"新长度更小"时才写 ⇒ 命中即是一次**截断**。
    //   `r10 = [point_event+0x1c] + 1`、`[point_event+0x18]` = owner（与 item 的 +0x18 比）。
    [0x3c3bf7, 'clamp_len_0'], [0x3c3c2c, 'clamp_len_1'],
  ];
  res.dv_sites = [];
  for (const [rva, nm] of DV_SITES) {
    try {
      Interceptor.attach(base.add(rva), {
        onEnter() {
          if (!DVCFG.on) return;
          const c = this.context;
          const o = { type: 'dv_site', site: nm, rva: '0x' + rva.toString(16), t: now() };
          try {
            o.rax = hx(c.rax); o.rbx = hx(c.rbx); o.rcx = hx(c.rcx); o.rdx = hx(c.rdx);
            o.r8 = hx(c.r8); o.r9 = hx(c.r9); o.r10 = hx(c.r10);
            o.r12 = hx(c.r12); o.r13 = hx(c.r13); o.r14 = hx(c.r14); o.r15 = hx(c.r15);
            o.slot100 = bytesHex(c.rsp.add(0x100), 8);
            o.slot1a0 = bytesHex(c.rsp.add(0x1a0), 8);
            o.xmm0 = abHex(c.xmm0);
          } catch (e) { }
          try { send(o); } catch (e) { }
        },
      });
      res.dv_sites.push({ name: nm, rva: '0x' + rva.toString(16) });
    } catch (e) { res.dv_sites.push({ name: nm, error: String(e) }); }
  }

  // ── ⑤ 页级访问监视（"慢动作"）：item 刚 push 进 Vec 之后，把它所在那一页设成陷阱，
  //    每抓到一次访问就**重新武装**一次 ⇒ 得到"这一页上按顺序发生的访问/写入"清单。
  //    为什么这么做：`Vec.len` 的写入者是唯一没有静态答案的一步（见 `dispatch/48` 报告），
  //    `MemoryAccessMonitor` 又是**每页一次**的（本机实测：一页写 5 次只报第 1 次）。
  //    开关：`DVCFG.mam`（驱动 `--dv-mam`），默认关。
  DVST.mamHits = 0; DVST.mamPage = null; DVST.mamItem = null;
  function dvMamArm(page, why) {
    try { MemoryAccessMonitor.disable(); } catch (e) { }
    try {
      send({ type: 'dv_mam_armed', t: now(), page: hx(page), why: why, item: hx(DVST.mamItem) });
      MemoryAccessMonitor.enable({ base: page, size: 0x1000 }, {
        onAccess(d) {
          DVST.mamHits++;
          let off = null;
          try { off = DVST.mamItem ? d.address.sub(DVST.mamItem).toString() : null; } catch (e) { }
          try {
            send({ type: 'dv_mam', t: now(), n: DVST.mamHits, op: d.operation, from: rvaOf(d.from),
                   from_abs: hx(d.from), addr: hx(d.address), off_item: off,
                   bt: btOf(d.context, 8) });
          } catch (e) { }
          if (DVST.mamHits < 40) {
            try { dvMamArm(page, 'rearm' + DVST.mamHits); }
            catch (e) { try { send({ type: 'dv_mam_err', msg: 'rearm:' + e }); } catch (e2) { } }
          }
        },
        onComplete() { try { send({ type: 'dv_mam_done', t: now() }); } catch (e) { } },
      });
    } catch (e) { try { send({ type: 'dv_mam_err', msg: String(e) }); } catch (e2) { } }
  }
  try {
    Interceptor.attach(base.add(0x3c2d90), {     // push 之后：`mov %rbx,0x128(%rsp)`（源 Vec.len++）
      onEnter() {
        if (!DVCFG.on || !DVCFG.mam) return;
        const c = this.context;
        try {
          const idx = Number(c.rbx.toString()) - 1;      // 刚 push 的那一项
          DVST.mamItem = c.rax.add(idx * 0x20);
          DVST.mamPage = DVST.mamItem.and(ptr('0xfffffffffffff000'));
          DVST.mamHits = 0;
          dvMamArm(DVST.mamPage, 'after_push');
        } catch (e) { try { send({ type: 'dv_mam_err', msg: String(e) }); } catch (e2) { } }
      },
    });
    res.hooks.push({ name: 'dv_mam_arm', rva: '0x3c2d90' });
    // 0x3c3685：`cmp %rdi,%rsi` —— 紧跟 0x3c3680 那次 `mov 0x10(%rcx,%rdx,1),%rsi`（遍历各项取 max len）。
    // 如果刚才读的正好是我们那一项的 +0x10，就在**读之后**重新武装（避免把这次读本身当成"下一次访问"）。
    Interceptor.attach(base.add(0x3c3685), {
      onEnter() {
        if (!DVCFG.on || !DVCFG.mam || !DVST.mamItem) return;
        try {
          const c = this.context;
          const a = c.rcx.add(c.rdx).add(0x10);
          if (a.equals(DVST.mamItem.add(0x10))) dvMamArm(DVST.mamPage, 'after_maxlen_read');
        } catch (e) { }
      },
    });
    res.hooks.push({ name: 'dv_mam_rearm_after_read', rva: '0x3c3685' });
  } catch (e) { res.hooks.push({ name: 'dv_mam_arm', error: String(e) }); }

  // 正对照：一条"逐点前缀比较"的自测，证明它**报得出** 1 bit 的差
  try {
    const n = 50, A = new Uint8Array(n * 16), B = new Uint8Array(n * 16);
    for (let i = 0; i < A.length; i++) { A[i] = (i * 13 + 7) & 0xff; B[i] = A[i]; }
    const same = dvCmpPrefix(A, B, n);
    B[41 * 16 + 3] ^= 0x01;
    const one = dvCmpPrefix(A, B, n);
    res.dv_selftest = { identical: same.diffPoints === 0, onebit: one.diffPoints === 1,
                        onebitIdx: one.firstDiff[0] && one.firstDiff[0][0], points: n };
  } catch (e) { res.dv_selftest = { error: String(e) }; }
}

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

  // ── 2026-11-08 `dispatch/39`：token 探针（**默认关**，`TOKCFG.on=false` 时三个钩子立刻 return；
  //    开关可以装机后再 post，所以这里**无条件装**，不赌 install 与 cfg 的先后）─────────
  try { installTokenProbe(base, res); } catch (e) { res.hooks.push({ name: 'token_probe', error: String(e) }); }

  // ── 2026-11-08 `dispatch/44`：`FunctionPoints.end_x` 的装配/序列化处（**默认关**，
  //    `FPCFG.on=false` 时两个钩子立刻 return；开关可以装机后再 post）──────────────
  try { installFunctionPointsProbe(base, res); } catch (e) { res.hooks.push({ name: 'fnpoints_probe', error: String(e) }); }

  // ── 2026-11-08 `dispatch/48`：两装配点**同时**抓 + `calculate_shot_results` 返回结构
  //    （**默认关**，`DVCFG.on=false` 时三个钩子立刻 return；开关可以装机后再 post）──────
  try { installDivergeProbe(base, res); } catch (e) { res.hooks.push({ name: 'diverge_probe', error: String(e) }); }

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
    // 2026-11-08 `dispatch/39`：token 探针开关（驱动 `--tokens` 打开）
    if (m && typeof m.tokenProbe === 'boolean') TOKCFG.on = m.tokenProbe;
    if (m && typeof m.maxTokens === 'number') TOKCFG.maxTokens = m.maxTokens;
    // 2026-11-08 `dispatch/44`：FunctionPoints 探针开关（驱动 `--fnpoints` 打开）
    if (m && typeof m.fnpoints === 'boolean') FPCFG.on = m.fnpoints;
    // 2026-11-08 `dispatch/48`：分叉探针开关（驱动 `--diverge` 打开）
    if (m && typeof m.diverge === 'boolean') DVCFG.on = m.diverge;
    if (m && typeof m.dvArrMax === 'number') DVCFG.arrMax = m.dvArrMax;
    if (m && typeof m.dvMam === 'boolean') DVCFG.mam = m.dvMam;
    send({ type: 'cfg_ack', evalRowsMax: CFG.evalRowsMax, arrayCap: CFG.arrayCap,
           tokenProbe: TOKCFG.on, maxTokens: TOKCFG.maxTokens, fnpoints: FPCFG.on, diverge: DVCFG.on });
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
