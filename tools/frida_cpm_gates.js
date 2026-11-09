// tools/frida_cpm_gates.js —— G7（LocalPlayerManager 的第五道门）+ `Computer.state` 探针。
//
// 目标：libgraphwarrust.so sha256 9a5381…（steam/2026-08-28）。地址一律 **vaddr**（Ghidra = vaddr+0x100000）。
// 只观测，不改游戏内存。
//
// ① `handle_player_info`（`0x519f40`）进门时的五道门取值：
//      ① tag == 13        `0x519f58`  ② connected == 1 `0x519f68`（self+0x18）
//      ③ connection 相等   `0x519f71`（self+0x1c vs event+0x0c）
//      ④ 旧 len == 0      `0x519f9d`（self+0x10）
//      ⑤ bot 表里没有该 id `0x519fb1` → `0x46c5a0`（get_module_wk）→ `0x51a058`
//    门⑤两个出口直接挂钩：`0x51a084`（未命中 ⇒ 继续发 5 连发）/ `0x51a0e0`（命中 ⇒ 收尾不发）
// ② 5 连发：`0x51a170`(59) / `0x51a247`(50) / `0x51a31e`(52) / `0x51a3f5`(54) / `0x51a451`(66)
// ③ 第五道门那张表 = `GraphwarComputerPlayerManager` 的 Bot 表：
//      插入 `add_computer_player` `0x478fc0`（key 在 esi）；删除 `handle_entity_removed_event` `0x47a520`
//      表 = self+0x18（ctrl）/ +0x20（mask）/ +0x30（items），桶步长 0x78，key@entry+0x00
// ④ `Computer.state`：门里读的那一格 = entry+0x08
//      初始化 3（`0x478fe3`+`0x479036` 搬进 BotData+0x00）；读 `0x4786ab` / `0x479610`；
//      `handle_tick` 写回 3 `0x47864f`；整条 BotData 重写 `0x479dfa`
//      + 100 ms 定时**直读内存**（不依赖写入点）：state 一变就报，附整条 entry 的十六进制
'use strict';

const BASE = Process.getModuleByName('libgraphwarrust.so').base;
const A = {
  LPM_PLAYER_INFO: 0x519f40,
  LPM_CONNECTED: 0x51a570,
  GATE5_UPGRADE: 0x519fb5,
  GATE5_FOUND: 0x51a0e0,
  GATE5_MISS: 0x51a084,
  SEND_59: 0x51a170, SEND_50: 0x51a247, SEND_52: 0x51a31e, SEND_54: 0x51a3f5, SEND_66: 0x51a451,
  CPM_ADD: 0x478fc0,
  CPM_ERASE: 0x47a520,
  CPM_TICK: 0x4785f0,
  CPM_TICK_RESET: 0x47864f,
  CPM_TURNINFO: 0x479520,
  CPM_TI_STATE_READ: 0x479610,
  CPM_BOTDATA_WRITE: 0x479dfa,
};

const mod = (off) => BASE.add(off);
let seq = 0;
const t0 = Date.now();
const stamp = () => Date.now() - t0;
function emit(obj) { obj.t = stamp(); obj.seq = ++seq; send(obj); }
function hex(p, n) {
  try {
    return Array.from(new Uint8Array(p.readByteArray(n)))
      .map((b) => b.toString(16).padStart(2, '0')).join('');
  } catch (e) { return null; }
}
function rd(fn, dflt) { try { return fn(); } catch (e) { return dflt === undefined ? null : dflt; } }

let lpmSelf = null, cpmSelf = null;
const botKeys = {};
const lastState = {};

function walkTable() {
  if (!cpmSelf) return [];
  const ctrl = rd(() => cpmSelf.add(0x18).readPointer());
  const mask = rd(() => cpmSelf.add(0x20).readU64().toNumber());
  const items = rd(() => cpmSelf.add(0x30).readU64().toNumber());
  if (!ctrl || !items || items > 4096 || mask === null || mask > 4096) return [];
  const out = [];
  for (let i = 0; i <= mask; i++) {
    // hashbrown：ctrl[i] 的最高位为 0 才是占用槽（0xFF/0x80 = EMPTY/DELETED）
    const c = rd(() => ctrl.add(i).readU8());
    if (c === null || (c & 0x80) !== 0) continue;
    const entry = ctrl.sub((i + 1) * 0x78);
    const key = rd(() => entry.readU32());
    if (key === null) continue;
    out.push({ i: i, key: key, entry: entry });
  }
  return out;
}
function snapshotStates(tag) {
  const rows = walkTable();
  const s = rows.map((r) => r.key + '=' + rd(() => r.entry.add(8).readU32()));
  emit({ type: 'table', src: tag, items: rd(() => cpmSelf.add(0x30).readU64().toNumber()), states: s.join(',') });
  return rows;
}

// ── ① 玩家信息事件 + 五道门 ────────────────────────────────────────────────
Interceptor.attach(mod(A.LPM_PLAYER_INFO), {
  onEnter(args) {
    lpmSelf = args[0];
    const self = args[0], ev = args[1];
    const pid = rd(() => ev.add(8).readU32());
    const rows = walkTable();
    const hit = rows.find((r) => r.key === pid);
    emit({
      type: 'player_info',
      tag: rd(() => ev.readU64().toString(16)),
      pid: pid,
      conn: rd(() => ev.add(0xc).readU32()),
      connected: rd(() => self.add(0x18).readU32()),
      my_conn: rd(() => self.add(0x1c).readU32()),
      vec_len: rd(() => self.add(0x10).readU64().toNumber()),
      weak_container: rd(() => self.add(0x30).readPointer().toString()),
      cpm_box: cpmSelf ? cpmSelf.sub(0x18).toString() : null,
      weak_eq_cpm: cpmSelf ? rd(() => self.add(0x30).readPointer().equals(cpmSelf.sub(0x18))) : null,
      pid_in_bot_table: !!hit,
      bot_table_keys: rows.map((r) => r.key).join(','),
      hit_state: hit ? rd(() => hit.entry.add(8).readU32()) : null,
    });
  },
});

Interceptor.attach(mod(A.LPM_CONNECTED), {
  onEnter(args) {
    const self = args[0], ev = args[1];
    emit({ type: 'connected_to_server', self: self.toString(),
           tag: rd(() => ev.readU64().toString(16)),
           ev08: rd(() => ev.add(8).readU32()), ev0c: rd(() => ev.add(0xc).readU32()) });
  },
  onLeave() {
    if (!lpmSelf) return;
    emit({ type: 'connected_after', my_conn: rd(() => lpmSelf.add(0x1c).readU32()),
           connected: rd(() => lpmSelf.add(0x18).readU32()) });
  },
});

Interceptor.attach(mod(A.GATE5_UPGRADE), {
  onEnter(args) {
    emit({ type: 'gate5_enter',
           weak_at: args[0].toString(),
           weak_target: rd(() => args[0].readPointer().toString()),
           cpm_box: cpmSelf ? cpmSelf.sub(0x18).toString() : null,
           match: cpmSelf ? rd(() => args[0].readPointer().equals(cpmSelf.sub(0x18))) : null });
  },
});
Interceptor.attach(mod(0x46c5a0), {
  // get_module_wk::<T>（20 个单态化里的第一个，docs/02 §22.3 记作 ComputerPlayerManager 的 wk）
  // 返回 rax = RcInner 基（= M 基 - 0x18）；与 CPM 的 self（M 基）比，判定"取的是不是 CPM"
  onEnter(args) {
    this.weakAt = args[0];
  },
  onLeave(ret) {
    const r = ret.toUInt32 ? ret : null;
    emit({ type: 'get_module_wk', weak_at: this.weakAt.toString(),
           weak_target: rd(() => this.weakAt.readPointer().toString()),
           ret: ret.toString(),
           cpm_box: cpmSelf ? cpmSelf.sub(0x18).toString() : null,
           match_cpm: cpmSelf ? rd(() => ret.equals(cpmSelf.sub(0x18))) : null });
  },
});
Interceptor.attach(mod(A.GATE5_FOUND), { onEnter() { emit({ type: 'gate5_found' }); } });
Interceptor.attach(mod(A.GATE5_MISS), { onEnter() { emit({ type: 'gate5_miss' }); } });

function sendHook(off, tagNo, pidOff) {
  Interceptor.attach(mod(off), {
    onEnter(args) {
      const ev = args[1];
      emit({ type: 'send', tag: tagNo, ev: ev.toString(),
             pid: rd(() => ev.add(pidOff).readU32()),
             slen: rd(() => ev.add(0x18).readU64().toNumber()),
             raw: hex(ev, 0x28) });
    },
  });
}
sendHook(A.SEND_59, 59, 0x20);
sendHook(A.SEND_50, 50, 0x20);
sendHook(A.SEND_52, 52, 0x20);
sendHook(A.SEND_54, 54, 0x20);
sendHook(A.SEND_66, 66, 0x10);

// ── ③ bot 表的插入 / 删除 ─────────────────────────────────────────────────
Interceptor.attach(mod(A.CPM_ADD), {
  onEnter(args) {
    cpmSelf = args[0];
    this.key = args[1].toUInt32();
    emit({ type: 'cpm_add', self: args[0].toString(), key: this.key,
           level: args[2].toInt32(), s: args[3].toInt32(), n: args[4].toInt32(),
           limit: args[5].toInt32() });
  },
  onLeave() {
    const rows = walkTable();
    const hit = rows.find((r) => r.key === this.key);
    if (hit) {
      botKeys[this.key] = hit.entry;
      emit({ type: 'bot_key', src: 'insert', key: this.key, entry: hit.entry.toString(),
             state: rd(() => hit.entry.add(8).readU32()), raw: hex(hit.entry, 0x78) });
    } else {
      emit({ type: 'bot_key_missing', key: this.key,
             keys: rows.map((r) => r.key).join(',') });
    }
  },
});

Interceptor.attach(mod(A.CPM_ERASE), {
  onEnter(args) {
    cpmSelf = args[0];
    const ev = args[1];
    const key = rd(() => ev.add(8).readU32());
    emit({ type: 'cpm_erase', key: key, tag: rd(() => ev.readU64().toString(16)) });
    if (key !== null) delete botKeys[key];
  },
  onLeave() { snapshotStates('after_erase'); },
});

// ── ④ Computer.state ─────────────────────────────────────────────────────
let tickCount = 0;
Interceptor.attach(mod(A.CPM_TICK), {
  onEnter(args) {
    cpmSelf = args[0];
    tickCount++;
    if (tickCount % 60 === 1) {
      const rows = walkTable();
      emit({ type: 'cpm_tick', n: tickCount, self: args[0].toString(),
             items: rd(() => args[0].add(0x30).readU64().toNumber()),
             states: rows.map((r) => r.key + '=' + rd(() => r.entry.add(8).readU32())).join(',') });
    }
  },
});

Interceptor.attach(mod(A.CPM_TICK_RESET), {
  onEnter() {
    const p = this.context.r15;
    emit({ type: 'state_write', site: '0x47864f', ptr: p.toString(),
           before: rd(() => p.readU32()), raw: hex(p, 0x30) });
  },
});

Interceptor.attach(mod(A.CPM_TURNINFO), {
  onEnter(args) {
    cpmSelf = args[0];
    this.key = rd(() => args[1].add(8).readU32());
    const rows = walkTable();
    emit({ type: 'turn_info', self: args[0].toString(), key: this.key,
           states: rows.map((r) => r.key + '=' + rd(() => r.entry.add(8).readU32())).join(','),
           items: rd(() => args[0].add(0x30).readU64().toNumber()) });
  },
  onLeave() {
    emit({ type: 'turn_info_leave', key: this.key });
  },
});

Interceptor.attach(mod(A.CPM_TI_STATE_READ), {
  onEnter() {
    const entry = this.context.r14.sub(0x70);
    emit({ type: 'gate_read', site: '0x479610', entry: entry.toString(),
           state: rd(() => entry.readU32()), raw: hex(entry, 0x30) });
  },
});

Interceptor.attach(mod(A.CPM_BOTDATA_WRITE), {
  onEnter() {
    const entry = this.context.r14.sub(0x70);
    emit({ type: 'botdata_write', site: '0x479dfa', entry: entry.toString(),
           new_qword: this.context.rbx.toString(), before: hex(entry, 0x30) });
  },
});

// ── 定时直读 state（0/1/2/3 任何迁移都抓） ────────────────────────────────
setInterval(() => {
  if (!cpmSelf) return;
  for (const r of walkTable()) {
    const st = rd(() => r.entry.add(8).readU32());
    if (st === null) continue;
    if (!(r.key in lastState)) { lastState[r.key] = st; continue; }
    if (lastState[r.key] !== st) {
      lastState[r.key] = st;
      emit({ type: 'state_change', key: r.key, state: st, raw: hex(r.entry, 0x78), src: 'timer' });
    }
  }
}, 100);

// ── 正对照：探针自己验一遍（字节读得到，才说明 base/内存访问都对） ──────────
emit({ type: 'hooks', base: BASE.toString(),
       bytes_519fb1: hex(mod(0x519fb1), 6),   // 期望 49 8d 7e 30 e8 …
       bytes_478fc0: hex(mod(0x478fc0), 4),
       bytes_47864f: hex(mod(0x47864f), 4),   // 期望 49 c7 07 03 00 00 00
       bytes_4786ab: hex(mod(0x4786ab), 7),   // 期望 83 7c 2b 90 03
       n_hooks: Object.keys(A).length });
emit({ type: 'ready' });
