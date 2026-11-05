/**
 * hook_ai_phases.js — runtime observation of the GW2 AI (evolutionary search) phases.
 *
 * It answers three static-analysis claims with runtime data (docs/100):
 *   1. is there a 25 s wall-clock budget, checked once at the top of every generation
 *      (`cmp $0x19` at vaddr 0x51b7cf)?
 *   2. does every score call use step 0.5 (coarse, everyone) or 0.01 (top-3 fine pass)?
 *   3. how many score calls does one generation contain?
 *
 * Address space: **ELF vaddr** (frida). Ghidra address = vaddr + 0x100000.
 *
 * Implementation notes (hard-won, do not "simplify"):
 *  * The script must run on the **V8** runtime: on frida 17.23.2 the default QuickJS
 *    runtime throws `RangeError: invalid array index` for *any* CpuContext access, so
 *    every context-using hook silently produces nothing.
 *  * The callback signature is `onEnter(args)`: `args` is the integer-argument array,
 *    and the register context is `this.context` (NOT the first callback parameter).
 *    On x64 `this.context` does expose `xmm0`..`xmm15` as 16-byte ArrayBuffers, so the
 *    `double` first argument (the step) is `new Float64Array(this.context.xmm0)[0]`.
 *    `this.returnAddress` identifies the call site.
 *  * The score core also spills the step: `51a761 movq %xmm0,0x78(%rsp)`, so
 *    GW2_SCORE_READ=spill reads [rsp+0x78] at 0x51a767 as an independent cross-check
 *    of the xmm0 read.
 *  * Every Interceptor.attach is individually try/catch'ed: one bad address must not
 *    stop the rest (an earlier experiment lost two hooks exactly that way).
 *
 * One command reproduces a run:
 *   GW2_BOT_OPT=5 GW2_RUN_SECS=60 GW2_OUT=/abs/out.json node tools/hook_ai_phases.js
 *
 * Env:
 *   GW2_BOT_OPT      1..6 difficulty (read by godot-live/probe.gd)
 *   GW2_RUN_SECS     wall-clock seconds for the whole run (default 60)
 *   GW2_OUT          output JSON path (default <gw2>/.rea-work/ai_phases.json)
 *   GW2_LIVE         godot-live project dir (default <gw2>/godot-live)
 *   GW2_GODOT        godot binary (default <gw2>/tools/Godot_v4.7.2-stable_linux.x86_64)
 *   GW2_WORK         XDG base dir + cwd (default <gw2>)
 *   GW2_HEAVY        1 = per-candidate hooks (score/factory/clone); 0 = light run
 *                    (a light run measures the budget without instrumentation bias)
 *   GW2_SCORE_READ   reg  = read step from xmm0 at 0x51a750 (default)
 *                    spill= read the spilled copy at 0x51a767 (cross-check)
 *   GW2_SLEEP_AT_GEN / GW2_SLEEP_SECS
 *                    clock-identity control: block the AI thread for N s at generation K
 *                    (Thread.sleep inside the loop-head callback) and watch `elapsed`
 *   GW2_CLOCK        1 = sample Timespec::now (0x8f5bf0) 200 times, then detach.
 *                    Never leave it on: that wrapper runs ~3e5 times/s and the game stalls
 *   GW2_START_FRAME  forwarded to probe.gd
 */
'use strict';
const path = require('path');
const fs = require('fs');
const { spawn } = require('child_process');

const GW2 = process.env.GW2_ROOT || '/home/user/MyProject/gw2';
const frida = require(path.join(GW2, '.rea-runtime/node_modules/frida'));

const LIVE = process.env.GW2_LIVE || path.join(GW2, 'godot-live');
const GODOT = process.env.GW2_GODOT || path.join(GW2, 'tools/Godot_v4.7.2-stable_linux.x86_64');
const WORK = process.env.GW2_WORK || GW2;
const OUT = process.env.GW2_OUT || path.join(GW2, '.rea-work/ai_phases.json');
const DIFF = process.env.GW2_BOT_OPT || '3';
const RUN_SECS = parseInt(process.env.GW2_RUN_SECS || '60', 10);
const HEAVY = (process.env.GW2_HEAVY || '1') !== '0';
const SCORE_READ = process.env.GW2_SCORE_READ || 'reg';
// The Rust Timespec::now wrapper is called ~3e5 times/s by the engine itself.  Hooking it
// is only useful as a separate, tiny experiment (GW2_CLOCK=1), never in a probe run.
const CLOCK = (process.env.GW2_CLOCK || '0') === '1';
// Clock-identity experiment: block the AI thread for N seconds at generation K and see
// whether the game's own `elapsed()` (compared against 25 s) jumps by N.  Wall clock ->
// it jumps; CPU/thread clock -> it does not.
const SLEEP_AT_GEN = parseInt(process.env.GW2_SLEEP_AT_GEN || '0', 10);
const SLEEP_SECS = parseFloat(process.env.GW2_SLEEP_SECS || '2');

const CFG = { heavy: HEAVY, diff: DIFF, scoreRead: SCORE_READ, clock: CLOCK,
              sleepAtGen: SLEEP_AT_GEN, sleepSecs: SLEEP_SECS };

const SRC = `
'use strict';
const CFG = ${JSON.stringify(CFG)};
const T = {
  ai_entry:      0x51b440,   // evolution driver, variant A (carries the 25 s budget)
  ai_entry_b:    0x51d080,   // evolution driver, variant B (same 25 s)
  loop_head:     0x51b7c6,   // per-generation head (variant A): mov %ebx,0x14c(%rsp)
  cmp25:         0x51b7cf,   // cmp $0x19,%rax  -> rax = elapsed secs, rdx = nanos
  exit_timeout:  0x51cc4d,   // timeout exit (jmp to the common exit)
  exit_complete: 0x51cb6d,   // generation-count exit (jmp to the common exit)
  exit_common:   0x51cbaf,   // both exits converge here
  ai_entry_b2:   0x51d080,
  loop_head_b:   0x51d396,   // per-generation head (variant B)
  cmp25_b:       0x51d398,   // cmp $0x19,%rax (variant B)
  exit_b:        0x51e543,   // variant B single exit
  score:         0x51a750,   // score core entry (step in xmm0)
  score_spill:   0x51a767,   // right after "movq %xmm0,0x78(%rsp)"
  factory:       0x4ee3a0,   // child factory
  clone_site:    0x51b8c5,   // the memcpy call = clone of one elite
  randfunc:      0x4ef020,   // makeRandomFunction
  ctor:          0x4ee0e0,   // build / repair
  mutate:        0x4ee720,   // mutation
  now_wrap:      0x8f5bf0,   // Timespec::now entry, edi = clockid
  now_ts:        0x8f5c03,   // after clock_gettime: rbp-0x20 = tv_sec, rbp-0x18 = tv_nsec
};

let m = null, ready = false;
const errors = [];
const state = {
  started: Date.now(),
  evolutions: [], cur: null,
  scoreTotal: 0,
  factoryTotal: 0, cloneTotal: 0, randfuncTotal: 0, ctorTotal: 0, mutateTotal: 0,
  stepHist: {},      // exact step double -> count
  retHist: {},       // return address (vaddr) -> count
  stepBySite: {},    // return address -> {stepValue: count}
  clockIds: {},      // clockid -> count
  clockSamples: [],  // CLOCK_MONOTONIC readings seen by the game
  exits: { timeout_a: 0, complete_a: 0, common_a: 0, exit_b: 0 },
  variantB: 0,
  hookErrors: [], bodyErrors: [],
  sendErr: null,
};

function bodyErr(where, e) {
  if (state.bodyErrors.length < 12) state.bodyErrors.push(where + ': ' + String((e && e.message) || e));
}

function newEvo() {
  return { i: state.evolutions.length + 1, t0: Date.now(), t1: null, gen: 0, gens: [],
           curGen: null, elapsed: [], score: 0, factory: 0, clone: 0, ended: null };
}

function pushGen(c) { if (c && c.curGen) { c.gens.push(c.curGen); c.curGen = null; } }

function att(off, name, cbs) {
  if (!m) return 'no-module';
  try { return Interceptor.attach(m.base.add(off), cbs); }
  catch (e) { state.hookErrors.push(name + '@' + off.toString(16) + ': ' + e.message); return false; }
}

function hookAll() {
  const ok = {};

  // ---------- driver + budget (always on: ~1 event per generation) ----------
  ok.ai_entry = att(T.ai_entry, 'ai_entry', {
    onEnter() {
      try {
        const c = this.context;
        const e = newEvo();
        e.args = { gens: c.r8.toUInt32(), s: c.r9.toUInt32(),
                   param7: c.rsp.add(8).readU32(), rcx: c.rcx.toUInt32() };
        state.cur = e;
      } catch (err) { bodyErr('ai_entry', err); }
    },
    onLeave() {
      try {
        const e = state.cur; if (!e) return;
        pushGen(e);
        e.t1 = Date.now(); e.dur_ms = e.t1 - e.t0;
        state.evolutions.push(e); state.cur = null;
      } catch (err) { bodyErr('ai_entry.leave', err); }
    },
  });

  ok.loop_head = att(T.loop_head, 'loop_head', {
    onEnter() {
      try {
        const e = state.cur; if (!e) return;
        pushGen(e);
        e.gen++;
        e.curGen = { g: e.gen, t: Date.now(), score: 0, c: 0, f: 0, fine: [], fac: 0, cl: 0 };
        if (CFG.sleepAtGen && e.gen === CFG.sleepAtGen) {
          // block THIS thread (the AI thread) for a known wall interval
          try { Thread.sleep(CFG.sleepSecs); } catch (e2) {}
          e.sleptAt = { gen: e.gen, secs: CFG.sleepSecs, t: Date.now() };
        }
      } catch (err) { bodyErr('loop_head', err); }
    },
  });

  ok.cmp25 = att(T.cmp25, 'cmp25', {
    onEnter() {
      try {
        const e = state.cur; if (!e) return;
        const c = this.context;
        // 0x51b724 stores the Instant start into 0x160(%rsp)/0x168(%rsp); rsp is the
        // function frame and does not move inside the loop, so the same slots are
        // readable here.  This gives the game clock's ABSOLUTE value (clock base check).
        let start = null;
        try {
          const sp = c.rsp;
          start = { secs: parseInt(sp.add(0x160).readU64().toString(), 10),
                    nanos: sp.add(0x168).readU32() };
        } catch (e2) {}
        // NB: NativePointer#toString() defaults to HEX; parseInt(x,10) silently yields 0
        // for "0x19".  Always pass an explicit radix (this bug cost one round of runs).
        e.elapsed.push({ g: e.gen, secs: parseInt(c.rax.toString(10), 10),
                         nanos: c.rdx.toUInt32(), t: Date.now(), start: start });
      } catch (err) { bodyErr('cmp25', err); }
    },
  });

  ok.exit_timeout = att(T.exit_timeout, 'exit_timeout', {
    onEnter() { try { state.exits.timeout_a++; if (state.cur) state.cur.ended = 'timeout'; }
                catch (err) { bodyErr('exit_timeout', err); } } });
  ok.exit_complete = att(T.exit_complete, 'exit_complete', {
    onEnter() { try { state.exits.complete_a++; if (state.cur) state.cur.ended = 'complete'; }
                catch (err) { bodyErr('exit_complete', err); } } });
  ok.exit_common = att(T.exit_common, 'exit_common', {
    onEnter() { try { state.exits.common_a++; } catch (err) { bodyErr('exit_common', err); } } });

  // ---------- variant B (watch only) ----------
  ok.ai_entry_b = att(T.ai_entry_b, 'ai_entry_b', {
    onEnter() { try { state.variantB++; } catch (err) { bodyErr('ai_entry_b', err); } } });
  ok.exit_b = att(T.exit_b, 'exit_b', {
    onEnter() { try { state.exits.exit_b++; } catch (err) { bodyErr('exit_b', err); } } });

  // ---------- clock: SAMPLE ONLY, then detach ----------
  // Timespec::now (0x8f5bf0) is called ~3e5 times/s by the engine's own loops; hooking
  // it unconditionally saturates the frida JS thread and the game stalls. 200 samples
  // are enough to prove the clockid and to align the game clock with Date.now().
  let nNowWrap = 0, nNowTs = 0;
  const wrapL = !CFG.clock ? null : att(T.now_wrap, 'now_wrap', {
    onEnter() {
      try { const k = this.context.rdi.toUInt32();
            state.clockIds[k] = (state.clockIds[k] || 0) + 1; }
      catch (err) { bodyErr('now_wrap', err); }
      if (++nNowWrap >= 200 && wrapL) { try { wrapL.detach(); } catch (e) {} }
    } });
  const tsL = !CFG.clock ? null : att(T.now_ts, 'now_ts', {
    onEnter() {
      try {
        const b = this.context.rbp;
        state.clockSamples.push({ t: Date.now(),
                                  secs: parseInt(b.sub(0x20).readU64().toString(), 10),
                                  nanos: parseInt(b.sub(0x18).readU64().toString(), 10) });
      } catch (err) { bodyErr('now_ts', err); }
      if (++nNowTs >= 200 && tsL) { try { tsL.detach(); } catch (e) {} }
    } });
  ok.now_wrap = !!wrapL; ok.now_ts = !!tsL;

  // ---------- per candidate (heavy only: distorts wall clock) ----------
  if (CFG.heavy) {
    const scoreOff = CFG.scoreRead === 'spill' ? T.score_spill : T.score;
    ok.score = att(scoreOff, 'score', {
      onEnter() {
        try {
          state.scoreTotal++;
          const c = this.context;
          const step = CFG.scoreRead === 'spill'
            ? c.rsp.add(0x78).readDouble()          // spilled by movq %xmm0,0x78(%rsp)
            : new Float64Array(c.xmm0)[0];          // the double argument itself
          const key = step === 0.5 ? '0.5'
                    : (Math.abs(step - 0.01) < 1e-12 ? '0.01' : String(step));
          state.stepHist[key] = (state.stepHist[key] || 0) + 1;
          const site = this.returnAddress.sub(m.base).toString(16);
          state.retHist[site] = (state.retHist[site] || 0) + 1;
          const bs = state.stepBySite[site] || (state.stepBySite[site] = {});
          bs[key] = (bs[key] || 0) + 1;
          const e = state.cur;
          if (!e || !e.curGen) return;
          const g = e.curGen;
          e.score++; g.score++;
          if (key === '0.5') g.c++;
          else { g.f++; g.fine.push(g.score); }   // 1-based position inside the generation
        } catch (err) { bodyErr('score', err); }
      },
    });
    ok.factory = att(T.factory, 'factory', {
      onEnter() { try { state.factoryTotal++; const e = state.cur;
                        if (e && e.curGen) { e.factory++; e.curGen.fac++; } }
                  catch (err) { bodyErr('factory', err); } } });
    ok.clone_site = att(T.clone_site, 'clone_site', {
      onEnter() { try { state.cloneTotal++; const e = state.cur;
                        if (e && e.curGen) { e.clone++; e.curGen.cl++; } }
                  catch (err) { bodyErr('clone_site', err); } } });
    ok.randfunc = att(T.randfunc, 'randfunc', {
      onEnter() { try { state.randfuncTotal++; } catch (err) { bodyErr('randfunc', err); } } });
    ok.ctor = att(T.ctor, 'ctor', {
      onEnter() { try { state.ctorTotal++; } catch (err) { bodyErr('ctor', err); } } });
    ok.mutate = att(T.mutate, 'mutate', {
      onEnter() { try { state.mutateTotal++; } catch (err) { bodyErr('mutate', err); } } });
  }

  ready = true;
  send({ kind: 'ready', heavy: CFG.heavy, scoreRead: CFG.scoreRead,
         hooks: ok, hookErrors: state.hookErrors });
}

const finder = setInterval(() => {
  if (ready) return;
  try {
    const mod = Process.enumerateModules().find((x) => /graphwarrust/i.test(x.name));
    if (!mod) return;
    m = mod;
    clearInterval(finder);
    hookAll();
  } catch (e) { /* module-list race: retry */ }
}, 150);

function snapshot(full) {
  const evos = state.evolutions.map((e) => ({
    i: e.i, args: e.args, dur_ms: e.dur_ms, gen: e.gen, ended: e.ended, sleptAt: e.sleptAt,
    score: e.score, factory: e.factory, clone: e.clone, elapsed: e.elapsed,
    gens: e.gens.map((g) => ({ g: g.g, score: g.score, c: g.c, f: g.f, fine: g.fine,
                               fac: g.fac, cl: g.cl })),
  }));
  const out = {
    kind: 'snap', t: Date.now(), heavy: CFG.heavy, scoreRead: CFG.scoreRead, diff: CFG.diff,
    mbase: m ? m.base.toString() : null, ready: ready,
    now: { evo: state.evolutions.length, inProgress: !!state.cur },
    genHeadHits: state.evolutions.reduce((a, e) => a + e.gen, 0) + (state.cur ? state.cur.gen : 0),
    scoreTotal: state.scoreTotal, factoryTotal: state.factoryTotal,
    cloneTotal: state.cloneTotal, randfuncTotal: state.randfuncTotal,
    ctorTotal: state.ctorTotal, mutateTotal: state.mutateTotal,
    stepHist: state.stepHist, retHist: state.retHist, stepBySite: state.stepBySite,
    clockIds: state.clockIds, clockSamples: state.clockSamples,
    exits: state.exits, variantB: state.variantB,
    hookErrors: state.hookErrors, bodyErrors: state.bodyErrors, sendErr: state.sendErr,
    cur: state.cur ? { i: state.cur.i, gen: state.cur.gen, t0: state.cur.t0,
                       score: state.cur.score,
                       lastElapsed: state.cur.elapsed.length
                         ? state.cur.elapsed[state.cur.elapsed.length - 1] : null }
                   : null,
  };
  if (full) out.evolutions = evos;
  else out.evolutionsTail = evos.slice(-2);
  return out;
}

function safeSend(full) {
  try { send(snapshot(full)); }
  catch (e) { state.sendErr = String((e && e.message) || e);
              try { send({ kind: 'senderr', msg: state.sendErr }); } catch (e2) {} }
}
setInterval(() => safeSend(false), 2000);
setInterval(() => safeSend(true), 10000);
`;

(async () => {
  const env = Object.assign({}, process.env, {
    XDG_DATA_HOME: path.join(WORK, '.godot-data'),
    XDG_CACHE_HOME: path.join(WORK, '.godot-cache'),
    XDG_CONFIG_HOME: path.join(WORK, '.godot-config'),
    GW2_BOT_OPT: DIFF,
  });
  const logPath = OUT.replace(/\.json$/, '') + '.godot.log';
  let child = null, session = null, pid = null;
  let last = null, full = null, readyMsg = null, gameOut = '', nSnap = 0;
  const log = (...a) => console.error('[drv]', ...a);
  try {
    // frida-owned spawn: the game starts PAUSED, so the hooks are in place for every call.
    // (attaching to an already-running godot process fails here: attach times out)
    const dev = await frida.getLocalDevice();
    pid = await dev.spawn([GODOT, '--headless', '--path', LIVE],
                          { env, cwd: WORK, stdio: 'inherit' });
    log('game pid', pid, 'diff', DIFF, 'heavy', HEAVY, 'scoreRead', SCORE_READ,
        'secs', RUN_SECS);
    session = await dev.attach(pid);
    const script = await session.createScript(SRC, { runtime: 'v8' });
    script.message.connect((msg) => {
      if (msg.type === 'error') { log('SCRIPT ERROR', msg.description); return; }
      const p = msg.payload || {};
      if (p.kind === 'ready') { readyMsg = p; log('hooks', JSON.stringify(p.hooks)); }
      else if (p.kind === 'snap') {
        last = p; if (p.evolutions) full = p;
        if (++nSnap % 5 === 1)
          log('snap#', nSnap, 'evo', p.now.evo, 'gen', p.genHeadHits, 'score', p.scoreTotal,
              'inProg', p.now.inProgress, p.cur ? 'curGen=' + p.cur.gen : '');
      }
      else if (p.kind === 'senderr') log('SEND ERROR', p.msg);
    });
    await script.load();
    await dev.resume(pid);
    await new Promise((r) => setTimeout(r, RUN_SECS * 1000));
    try { await session.detach(); } catch (e) {}
    session = null;
    try { await dev.kill(pid); } catch (e) {}
    pid = null;
  } catch (e) {
    log('FATAL', (e && e.stack) || e);
  } finally {
    try { if (pid) child = null; } catch (e) {}
    const data = full || last || { kind: 'none' };
    if (readyMsg) data.readyMsg = readyMsg;
    data.cfg = { diff: DIFF, heavy: HEAVY, scoreRead: SCORE_READ, runSecs: RUN_SECS,
                 live: LIVE, godot: GODOT, out: OUT };
    try {
      fs.mkdirSync(path.dirname(OUT), { recursive: true });
      fs.writeFileSync(OUT, JSON.stringify(data, null, 1));
      fs.writeFileSync(logPath, gameOut.slice(-400000));
      console.log('WROTE', OUT, '(game log:', logPath + ')');
    } catch (e) { console.error('WRITE FAILED', e.message); }

    if (data.now) {
      console.log('module base      :', data.mbase);
      console.log('evolutions       :', data.now.evo, 'inProgress', data.now.inProgress);
      console.log('generation heads :', data.genHeadHits);
      console.log('score            :', data.scoreTotal, ' factory', data.factoryTotal,
                  ' clone', data.cloneTotal);
      console.log('step histogram   :', JSON.stringify(data.stepHist));
      console.log('step by site     :', JSON.stringify(data.stepBySite));
      console.log('exits            :', JSON.stringify(data.exits), 'variantB', data.variantB);
      console.log('clock ids        :', JSON.stringify(data.clockIds));
      console.log('hook/body errors :', JSON.stringify(data.hookErrors), JSON.stringify(data.bodyErrors));
      if (data.evolutions) {
        data.evolutions.forEach((e) => {
          const el = e.elapsed.length ? e.elapsed[e.elapsed.length - 1] : null;
          console.log(`  evo#${e.i} args=${JSON.stringify(e.args)} gens=${e.gen} dur=${e.dur_ms}ms ` +
                      `score=${e.score} fac=${e.factory} clone=${e.clone} ended=${e.ended} ` +
                      `lastElapsed=${el ? el.secs + 's+' + el.nanos + 'ns' : '-'}`);
        });
      }
    } else {
      console.log('NO SNAPSHOT RECEIVED');
      if (readyMsg) console.log('ready:', JSON.stringify(readyMsg));
    }
    const tail = gameOut.split('\n')
      .filter((l) => /SET_FUNC|FI=|FI_MISSING|error|ERROR/i.test(l)).slice(-8);
    if (tail.length) console.log('game log tail:', tail.join(' | ').slice(0, 800));
  }
  process.exit(0);
})().catch((e) => { console.error('ERR', (e && e.stack) || e); process.exit(1); });
