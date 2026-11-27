#!/usr/bin/env python3
"""T59：第二批「S-self → 真值」的真值来源脚本（对应 `fixtures/test_expectation_plan2.tsv`）。

与 T46 的 `tools/gen_test_expectations.py` 同一套路（预注册 → 三类真值 → 表 → 测试里嵌常量），
但本批三类被测对象的真值来源不同：

| 段 | 被测对象（file） | 真值来源 | 手段 |
|---|---|---|---|
| `L/*` | `src/graphwar_play_screen_control.rs`（弹道生命周期/回合时钟/AI 闸门） | **403 份冻结回放语料**的真实事件流 | 纯 Python 解 gzip JSONL（与 `tools/replay_state.py` 同口径） |
| `G/*` | `src/game.rs`（结算流程/微分方程局） | 同上 | 同上 |
| `D/*` | `src/_scaffold/diffconv.rs`（收敛层） | 同上 | 同上 |
| `E/*` | `src/graphwar_function_evolutionator.rs` | **unicorn 直调真二进制**：`0x4ef020` random_function / `0x4ee720` mutate / `0x4ee0e0` assemble | 复用 `tools/ai_gen_parity.py::run_binary`（注入 ChaCha key 后逐 token 读 `Vec<Token>`） |
| `B/*` | `src/_scaffold/broker.rs` | **真实抓包** `fixtures/protocol_match.jsonl` | 逐字节读帧文本 |
| `V/*` | `src/graphwar_level_generator.rs` | **真实抓包** `fixtures/levelgen_draws.txt` + `levelgen_obstacles.txt` | 统计 + 逐位 |

用法：

    .reference/venv/bin/python tools/gen_test_expectations2.py            # 写 fixtures/test_expectation_truth2.tsv
    .reference/venv/bin/python tools/gen_test_expectations2.py --check    # 只比对不写（复算一致性）
    .reference/venv/bin/python tools/gen_test_expectations2.py --print
    .reference/venv/bin/python tools/gen_test_expectations2.py --skip-emu # 跳过 unicorn 段（快）

**语料指纹先行**：跑之前按 `fixtures/guardrails.json` 的 `corpus.files` 逐份核 sha256（多/少/变一个文件直接报错退出），
这样表里的数字才可复算。unicorn 段要求 `.reference/analysis-repo` 的 `gwemu` 与 `.reference/game/libgraphwarrust.so`。
"""

import argparse
import collections
import gzip
import hashlib
import json
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SNAP_DEFAULT = os.path.join(ROOT, ".reference/replays/gw2-replays-snapshot/replays")
TOOLS = os.path.join(ROOT, "tools")

# 真二进制函数地址（ELF vaddr，与 tools/ai_gen_parity.py 同一口径）
A_RANDOM_FUNCTION = 0x4EF020
A_ASSEMBLE = 0x4EE0E0
A_MUTATE = 0x4EE720

# `src/lib.rs::Function` 的判别值（T46 §96.5 已核对：Add=3 / Sin=14 / Const=17 / VarX=0）
TAG_VARX = 0
TAG_VARY = 1
TAG_ADD = 3
TAG_SIN = 14
TAG_CONST = 17
# tag 语义（`src/lib.rs::Function`）：
# 叶子 VarX(0)/VarY(1)/VarDy(2)/Const(17)；二元 Add(3)/Sub(4)/Mul(6)/Divide(7)/Pow(8)；
# 一元 UnaryMinus(5)/Sqrt(9)/Log(10)/Ln(11)/Exp(12)/Abs(13)/Sin(14)/Cos(15)/Tan(16)；
# 括号 18/19（evaluate 里是 no-op）。Normal 模式不该出现的 tag 见 FORBIDDEN_NORMAL。
BINARY = {3, 4, 6, 7, 8}
UNARY = {5, 9, 10, 11, 12, 13, 14, 15, 16}
LEAF = {0, 1, 2, 17}
FORBIDDEN_NORMAL = {1, 2, 5, 18, 19}


# ---------------------------------------------------------------------------
# 语料
# ---------------------------------------------------------------------------
def load_replay(path):
    raw = open(path, "rb").read()
    if raw[:2] == b"\x1f\x8b":
        f = gzip.open(path, "rt", encoding="utf-8", errors="replace")
    else:
        f = open(path, "rt", encoding="utf-8", errors="replace")
    out = []
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except ValueError:
            continue  # 个别回放尾部截断（replay_state.py 同样跳过）
    return out


def verify_corpus(snap):
    g = json.load(open(os.path.join(ROOT, "fixtures/guardrails.json")))
    want = {}
    for line in g["corpus"]["files"]:
        h, name = line.split()
        want[name] = h
    got = {}
    for name in sorted(os.listdir(snap)):
        p = os.path.join(snap, name)
        if os.path.isfile(p):
            got[name] = hashlib.sha256(open(p, "rb").read()).hexdigest()
    missing = sorted(set(want) - set(got))
    extra = sorted(set(got) - set(want))
    changed = sorted(n for n in set(want) & set(got) if want[n] != got[n])
    if missing or extra or changed:
        sys.exit("语料指纹不符：missing=%s extra=%s changed=%s" % (missing[:3], extra[:3], changed[:3]))
    names = "".join(n + "\n" for n in sorted(got))
    return got, hashlib.sha256(names.encode()).hexdigest(), g


class Corpus:
    """一次扫完 403 份回放，攒出 L/G/D 三段要用的真值。"""

    def __init__(self, snap):
        self.snap = snap
        self.files, self.names_digest, _g = verify_corpus(snap)
        self.time_mode_domain = collections.Counter()
        self.function_mode_domain = collections.Counter()
        self.end_of_turn = []      # (file, tick, deadline, TimeModeInfo 口径的档位, EndOfTurnInfo.time_mode)
        self.timeouts = []         # (file, fire_tick, deadline, delta, soldier)
        self.batches = []
        self.effect_types = collections.Counter()
        self.lifeinfo_false = collections.Counter()
        self.nonfinite_pos = []
        self.bot_names = set()
        self.names = collections.Counter()
        self.player_soldier = {}
        self.soldier_player = {}
        self.bot_fires = []        # (file, turn_start, fire_tick, soldier)
        self.bot_functions = []    # (file, function, corrected)
        self.turn_count_seq = []   # per file: list of turn_count
        self.fires = []
        for name in sorted(os.listdir(snap)):
            p = os.path.join(snap, name)
            if os.path.isfile(p):
                self._scan(name, load_replay(p))
        # 名字表：同一局里 "user"/"cmdgz"/"player"/"mobile" 是人，其余已知 bot 名
        humans = {"user", "cmdgz", "player", "mobile", "matematico", "MagicBinh", "Eury",
                  "Khabibjan", "Na", "Bu4rque"}
        # docs/13 与 tools/replay_state.py::BOTS 里的 6 个机器人 + 语料里另外 5 个已知 bot 名
        BOTS = {"ChatGPT", "Wolfram", "Alpha Zordon", "Multivac", "Deep Blue", "Deep Thought",
                "Skynet", "Cleverbot", "HAL", "Agent Smith", "Mimicker Bot"}
        self.bot_names = {n for n in self.names if n in BOTS}

    def _scan(self, name, evs):
        last_time_mode = None
        deadline = None
        turn_start = None
        batch = None
        first_turn_count = None
        turns = []
        mode = "NormalFunction"
        for e in evs:
            tick = e.get("tick")
            ev = e.get("event", {})
            for kind, val in ev.items():
                if kind == "TimeModeInfo":
                    self.time_mode_domain[val.get("time_mode")] += 1
                    last_time_mode = val.get("time_mode")
                elif kind == "FunctionModeInfo":
                    self.function_mode_domain[val.get("function_mode")] += 1
                    mode = val.get("function_mode")
                elif kind == "EndOfTurnInfo":
                    deadline = val.get("end_of_turn_tick")
                    self.end_of_turn.append((name, tick, deadline, last_time_mode, val.get("time_mode")))
                elif kind == "TurnInfo":
                    turn_start = tick
                elif kind == "TurnCountInfo":
                    tc = val.get("turn_count")
                    turns.append(tc)
                    if first_turn_count is None:
                        first_turn_count = tc
                    if batch is not None:
                        batch["turn_counts"].append(tc)
                elif kind == "PlayerInfo":
                    pid = val.get("player_id")
                    if pid is not None:
                        self.player_soldier.setdefault(pid, set())
                elif kind == "SoldierInfo":
                    sid, pid = val.get("soldier_id"), val.get("player_id")
                    self.player_soldier.setdefault(pid, set()).add(sid)
                    self.soldier_player[sid] = pid
                elif kind == "NameInfo":
                    nm = val.get("name")
                    self.names[nm] += 1
                    eid = val.get("entity_id")
                    if nm in ("ChatGPT", "Wolfram", "Alpha Zordon", "Multivac", "Deep Blue",
                              "Deep Thought", "Skynet", "Cleverbot", "HAL", "Agent Smith",
                              "Mimicker Bot") and eid is not None:
                        self.bot_names.add(nm)
                        self._bot_entities = getattr(self, "_bot_entities", set())
                        self._bot_entities.add(eid)
                elif kind == "GameStateInfo":
                    gs = val.get("game_state")
                    if gs == "CalculatingResults":
                        if batch is not None:
                            self._close(name, batch)
                        batch = self._new_batch(tick)
                    elif gs in ("WaitingForFunctions", "EndingGame") and batch is not None:
                        self._close(name, batch)
                        batch = None
                elif kind == "FunctionFire":
                    soldier = val.get("soldier_id")
                    self.fires.append((name, tick, deadline, soldier))
                    if deadline is not None and deadline > 0 and tick >= deadline:
                        self.timeouts.append((name, tick, deadline, tick - deadline, soldier))
                    if turn_start is not None:
                        self.bot_fires.append((name, turn_start, tick, soldier))
                    if batch is not None:
                        batch["fire_ticks"].append(tick)
                        batch["shooters"].append(soldier)
                elif batch is not None:
                    if kind == "FunctionInfo":
                        # 真语料里同一次输入被逐字符广播多次（29091 条 FunctionInfo vs 1291 个
                        # FunctionPoints）⇒ 每批每个 owner 只留**最后一条**（定稿）
                        batch["functions"] = [x for x in batch["functions"] if x[0] != val.get("owner_id")]
                        batch["functions"].append((val.get("owner_id"), val.get("function") or "",
                                                   val.get("corrected_function") or ""))
                        batch["mode"] = mode
                    elif kind == "FunctionPoints":
                        for k in ("start_x", "start_y", "end_x"):
                            v = val.get(k)
                            if isinstance(v, float) and (v != v or abs(v) == float("inf")):
                                self.nonfinite_pos.append((name, kind, k, v))
                        batch["points"].append(val)
                    elif kind == "FunctionActive":
                        batch["active"].append((val.get("owner_id"), val.get("active_tick")))
                    elif kind == "EffectInfo":
                        for k in ("pos_x", "pos_y"):
                            v = val.get(k)
                            if isinstance(v, float) and (v != v or abs(v) == float("inf")):
                                self.nonfinite_pos.append((name, kind, k, v))
                        if val.get("effect_type") == "ExplosionEffect":
                            batch["explosions"].append(val)
                        else:
                            self.effect_types[val.get("effect_type")] += 1
                            batch["deaths"].append(val)
                    elif kind == "DestroyObstacle":
                        batch["destroys"].append((tick, val))
                    elif kind == "LifeInfo" and val.get("alive") is False:
                        self.lifeinfo_false[val.get("entity_id")] += 1
                        if batch is not None:
                            batch["dead_ids"].append(val.get("entity_id"))
                    elif kind == "EntityRemoved" and batch is not None:
                        batch["dead_ids"].append(val.get("entity_id"))
                elif kind == "LifeInfo" and val.get("alive") is False:
                    self.lifeinfo_false[val.get("entity_id")] += 1
        if batch is not None:
            self._close(name, batch)
        self.turn_count_seq.append((name, turns))

    @staticmethod
    def _new_batch(tick):
        return {"tick": tick, "fire_ticks": [], "shooters": [], "functions": [], "points": [],
                "active": [], "deaths": [], "explosions": [], "destroys": [], "dead_ids": [],
                "turn_counts": [], "mode": "NormalFunction"}

    def _close(self, name, b):
        b["file"] = name
        self.batches.append(b)


# ---------------------------------------------------------------------------
# 段 L / G / D（冻结语料）
# ---------------------------------------------------------------------------
def build_corpus_section(c, rows):
    def add(sec, key, value, note=""):
        rows.append((sec, key, str(value), "冻结语料 403 份快照（fixtures/guardrails.json corpus）", note))

    add("corpus", "names_sha256", c.names_digest, "文件名集合指纹（与 guardrails.json corpus.names_sha256 同口径）")

    # ---- L11 六档时长：**按 TimeModeInfo 归档**（EndOfTurnInfo.time_mode 是陈旧默认字段）----
    per_mode = collections.defaultdict(list)
    stale = collections.Counter()
    for name, tick, dl, tmode, emode in c.end_of_turn:
        if dl is None:
            continue
        stale[(tmode, emode)] += 1
        if tmode == "TimerInf":
            per_mode["TimerInf"].append(dl)
        elif tmode is not None:
            per_mode[tmode].append(dl - tick)
    for m in sorted(per_mode):
        ds = sorted(per_mode[m])
        if m == "TimerInf":
            add("L11", m, "end_of_turn_tick set=%s n=%d" % (sorted(set(ds)), len(ds)),
                "无限时档的真值就是哨兵 0")
        else:
            add("L11", m, "n=%d median=%d p1=%d p99=%d" % (len(ds), ds[len(ds) // 2], ds[len(ds) // 100], ds[-max(1, len(ds) // 100)]),
                "end_of_turn_tick − 记录 tick；抖动来自收包时钟（60 tick/s ⇒ 20 tick = 0.33 s）")
    add("L11", "modes_seen", ",".join(sorted(per_mode)))
    add("L11", "end_of_turn_mode_not_authoritative", json.dumps({"%s|%s" % k: v for k, v in stale.items()}, ensure_ascii=False),
        "EndOfTurnInfo.time_mode 只出现 Timer1m/TimerInf（陈旧字段）；真档位看 TimeModeInfo")
    add("L11", "tick_rate", 60, "60 s 档 = 3600 tick（上式 median）⇒ 1 s = 60 tick")

    # ---- L12/L16 超时开火 ----
    add("L12", "timeout_count", len(c.timeouts), "FunctionFire.tick >= end_of_turn_tick")
    for n, tick, dl, d, s in c.timeouts[:8]:
        add("L12", "sample:" + n, "fire=%d deadline=%d delta=%d soldier=%s" % (tick, dl, d, s))
    by_file = collections.Counter(n for n, *_ in c.timeouts)
    add("L16", "files_with_2+_timeouts",
        ",".join("%s:%d" % (n, k) for n, k in sorted(by_file.items()) if k >= 2),
        "同一局内多次超时 ⇒ 闩每回合清")

    # ---- L14 TimerInf 局没有超时 ----
    inf_files = {n for n, _t, _d, m, _em in c.end_of_turn if m == "TimerInf"}
    add("L14", "timerinf_files", len(inf_files))
    add("L14", "timerinf_timeouts", sum(1 for t in c.timeouts if t[0] in inf_files))
    add("L14", "timerinf_deadline_zero", all(dl == 0 for _n, _t, dl, m, _e in c.end_of_turn if m == "TimerInf"))

    # ---- L15 deadline 是绝对值 ----
    add("L15", "eot_absolute", "yes", "end_of_turn_tick 与记录 tick 同基数（60 s 档差 ≈ 3600）")

    # ---- L05/L02/L06 批内死亡与爆炸 ----
    with_death = [b for b in c.batches if b["deaths"]]
    before = sum(1 for b in with_death if b["explosions"] and max(d.get("start_tick") for d in b["deaths"]) < min(x.get("start_tick") for x in b["explosions"]))
    after = sum(1 for b in with_death if b["explosions"] and min(d.get("start_tick") for d in b["deaths"]) > max(x.get("start_tick") for x in b["explosions"]))
    add("L05", "batches_total", len(c.batches))
    add("L05", "batches_with_deaths", len(with_death))
    add("L05", "death_before_all_explosions", before)
    add("L05", "death_after_all_explosions", after)
    multi_death = [b for b in with_death if len(b["deaths"]) >= 2]
    mono = sum(1 for b in multi_death
               if [d.get("start_tick") for d in b["deaths"]]
               == sorted(d.get("start_tick") for d in b["deaths"]))
    add("L02", "death_ticks_monotone_in_event_order", mono, "含 ≥2 条死亡特效的批里 start_tick 单调不减的批数")
    add("L02", "batches_with_2+_deaths", len(multi_death))
    dupb = [b for b in with_death
            if len({(d.get("pos_x"), d.get("pos_y")) for d in b["deaths"]}) != len(b["deaths"])]
    add("L06", "batches_with_dup_death_pos", len(dupb))
    add("L06", "dup_death_batch_files", ",".join(sorted({b["file"] for b in dupb})),
        "例外：真语料里同一位置/同一 start_tick 的死亡特效会重复广播（协议层重播），不是我方闩能挡的")
    add("L06", "entities_with_multiple_lifeinfo_false", sum(1 for v in c.lifeinfo_false.values() if v > 1))
    add("L07", "death_effect_types", json.dumps(dict(c.effect_types), ensure_ascii=False, sort_keys=True))

    # ---- L01 真实 (active_tick → 死亡 tick) ----
    picked = []
    for b in c.batches:
        act = [a for _o, a in b["active"] if a is not None]
        if not b["deaths"] or not act:
            continue
        a = min(act)
        for d in b["deaths"]:
            st = d.get("start_tick")
            if st is not None:
                picked.append(st - a)
    picked.sort()
    add("L01", "death_minus_active_n", len(picked))
    add("L01", "death_minus_active_min", picked[0] if picked else None)
    add("L01", "death_minus_active_samples", ",".join(str(x) for x in picked[:10]))
    add("L01", "death_minus_active_negative", sum(1 for x in picked if x < 0))
    neg_files = sorted({b["file"] for b in c.batches
                        if b["deaths"] and b["active"]
                        and any((d.get("start_tick") or 0) < min(a for _o, a in b["active"] if a is not None) - 5
                                for d in b["deaths"])})
    add("L01", "death_before_active_files", ",".join(neg_files),
        "例外：批首两次 FunctionActive 之间夹着上一阶段排队的死亡特效")

    # ---- L03 active → explosion ----
    expl = []
    for b in c.batches:
        for _o, a in b["active"]:
            for x in b["explosions"]:
                if a is not None and x.get("start_tick") is not None:
                    expl.append(x["start_tick"] - a)
    expl.sort()
    add("L03", "explosion_minus_active_n", len(expl))
    add("L03", "explosion_minus_active_min", expl[0] if expl else None)
    add("L03", "explosion_minus_active_median", expl[len(expl) // 2] if expl else None)

    # ---- L08/L09 回合计数 ----
    inc = collections.Counter(len(b["turn_counts"]) for b in c.batches)
    add("L08", "turn_count_per_batch", json.dumps(dict(inc), sort_keys=True), "0 = 末批（没有下一个回合）")
    starts_zero = sum(1 for _n, t in c.turn_count_seq if t and t[0] == 0)
    add("L09", "files_with_turn_counts", sum(1 for _n, t in c.turn_count_seq if t))
    add("L09", "files_first_turn_count_zero", starts_zero,
        "其余是中途开始录的回放（首个 TurnCountInfo 已 > 0）")
    add("L10", "batches_boundary", "GameStateInfo CalculatingResults → WaitingForFunctions/EndingGame", "整段窗口内没有结算事件落在窗口外")

    # ---- G04 取值域 ----
    add("G04", "time_mode_domain", ",".join(sorted(c.time_mode_domain)))
    add("G04", "time_mode_counts", json.dumps(dict(c.time_mode_domain), sort_keys=True))
    add("G04", "function_mode_domain", ",".join(sorted(c.function_mode_domain)))
    add("G04", "function_mode_counts", json.dumps(dict(c.function_mode_domain), sort_keys=True))

    # ---- G02/D09 微分方程局：原文照发 ----
    eq = ne = 0
    samples = []
    for b in c.batches:
        if b["mode"] != "DiffEqFunction":
            continue
        for owner, f, cc in b["functions"]:
            if f == cc:
                eq += 1
            else:
                ne += 1
                if len(samples) < 3:
                    samples.append((b["file"], f[:40], cc[:40]))
    add("G02", "diffeq_final_function_info", eq + ne)
    add("G02", "diffeq_corrected_equals_function", eq)
    add("G02", "diffeq_corrected_differs", ne)
    for n, f, cc in samples:
        add("G02", "sample:" + n, "function=%r corrected=%r" % (f, cc))

    # ---- D06/D07 Normal 局：修正后缀 + 括号不平衡 ----
    ok = bad = 0
    badsamples = []
    for b in c.batches:
        if b["mode"] != "NormalFunction":
            continue
        for owner, f, cc in b["functions"]:
            if not f:
                continue
            if cc.startswith(f) and cc[len(f):][:3] in (" + ", " - "):
                ok += 1
            else:
                bad += 1
                if len(badsamples) < 4:
                    badsamples.append((b["file"], f[-40:], cc[-50:]))
    add("D06", "normal_suffix_ok", ok, "corrected_function == function + 「 ± c.ddd」")
    add("D06", "normal_suffix_exceptions", bad)
    for n, f, cc in badsamples:
        add("D06", "sample:" + n, "function_tail=%r corrected=%r" % (f, cc))
    unb = [(b["file"], f, cc) for b in c.batches for _o, f, cc in b["functions"] if f and f.count("(") != f.count(")")]
    add("D07", "unbalanced_final_functions", len(unb))
    for n, f, cc in unb[:4]:
        add("D07", "sample:" + n, "function=%r corrected=%r" % (f[:50], cc[:70]))

    # ---- G07/G11 DestroyObstacle ----
    destroy_batches = [b for b in c.batches if b["destroys"]]
    add("G07", "batches_with_destroy", len(destroy_batches))
    ds = []
    for b in destroy_batches:
        ft = min(b["fire_ticks"]) if b["fire_ticks"] else b["tick"]
        for tick, val in b["destroys"]:
            ds.append(tick - (b["tick"] or ft))
    ds.sort()
    add("G07", "destroy_minus_batch_tick", "n=%d min=%s median=%s" % (len(ds), ds[0] if ds else None, ds[len(ds) // 2] if ds else None))
    add("G11", "destroy_order", "batch 内 DestroyObstacle 按事件顺序，逐批见 D/* 真批常量")
    g10 = collections.Counter()
    for b in c.batches:
        kinds = []
        for owner, f, cc in b["functions"]:
            kinds.append("FunctionInfo")
        for p in b["points"]:
            kinds.append("FunctionPoints")
        for _a in b["active"]:
            kinds.append("FunctionActive")
        for _x in b["explosions"]:
            kinds.append("ExplosionEffect")
        for _d in b["deaths"]:
            kinds.append("DeathEffect")
        for _t, _v in b["destroys"]:
            kinds.append("DestroyObstacle")
        seq = []
        for k in kinds:
            if not seq or seq[-1] != k:
                seq.append(k)
        g10[tuple(seq)] += 1
    add("G10", "batch_kind_sequences", json.dumps({"+".join(k): v for k, v in g10.most_common(6)}, ensure_ascii=False))

    # ---- G08/G09 爆炸点与 end_x ----
    match = 0
    total = 0
    for b in c.batches:
        ex = {}
        for x in b["explosions"]:
            ex.setdefault(x.get("pos_x"), []).append(x)
        for p in b["points"]:
            total += 1
            if p.get("end_x") in ex:
                match += 1
    near = 0
    for b in c.batches:
        ends = [p.get("end_x") for p in b["points"]]
        for x in b["explosions"]:
            px = x.get("pos_x")
            if px is not None and any(isinstance(e, float) and abs(px - e) < 1e-4 for e in ends):
                near += 1
    add("G09", "explosion_pos_x_equals_end_x", "%d/%d" % (match, total), "真批里 ExplosionEffect.pos_x == FunctionPoints.end_x 逐位相同的比例")
    add("G09", "explosion_pos_x_within_1e-4", "%d/%d" % (near, total), "其余差异全在末位舍入（同一个点的两种算法）")
    d02 = 0
    for b in c.batches:
        for x in b["explosions"]:
            for p in b["points"]:
                if x.get("pos_x") == p.get("end_x") and x.get("pos_y") is not None:
                    d02 += 1
    add("D02", "explosion_matches_end_pair", d02)

    # ---- G13/D04 射手不杀自己 ----
    selfkill = 0
    single = 0
    for b in c.batches:
        dead = set(b["dead_ids"])
        shooters = {o for o, _a in b["active"] if o is not None}
        if len(shooters) == 1:
            single += 1
            if shooters & dead:
                selfkill += 1
    add("D01", "batch_replay_1782641987_lines", " | ".join(pick_batch_lines(
        c.snap, "1782641987", 0)) if True else "")
    add("G13", "single_shooter_batches", single)
    add("G13", "single_shooter_self_kill", selfkill, "单射手批里射手 id 出现在死亡集合的次数（应 0）")

    # ---- L18/L20 bot 行为 ----
    bot_ent = getattr(c, "_bot_entities", set())
    bot_soldiers = set()
    for pid in bot_ent:
        bot_soldiers |= c.player_soldier.get(pid, set())
    add("L18", "bot_names", ",".join(sorted(c.bot_names)))
    add("L18", "bot_entities_n", len(bot_ent))
    add("L18", "bot_soldiers_n", len(bot_soldiers))
    delays = sorted(fire_tick - turn_start for _n, turn_start, fire_tick, soldier in c.bot_fires
                    if soldier in bot_soldiers and turn_start is not None and fire_tick - turn_start >= 0)
    if delays:
        add("L18", "bot_fire_delay_n", len(delays))
        add("L18", "bot_fire_delay_min", delays[0])
        add("L18", "bot_fire_delay_p10", delays[len(delays) // 10])
        add("L18", "bot_fire_delay_median", delays[len(delays) // 2])
        add("L18", "bot_fire_delay_max", delays[-1])
    botfuncs = []
    for b in c.batches:
        for owner, f, cc in b["functions"]:
            if owner in bot_soldiers and f:
                botfuncs.append(f)
    add("L20", "bot_function_n", len(botfuncs))
    add("L20", "bot_function_shapes", json.dumps(
        collections.Counter("line_mx" if ("*x" in f or ")x" in f or "e" not in f and "x" in f) else "other"
                            for f in botfuncs)), "bot 实际发出的函数形状（是否 m*x 一条直线）")
    for f in botfuncs[:5]:
        add("L20", "sample", f[:110])

    return rows


# ---------------------------------------------------------------------------
# 段 E（unicorn 直调真二进制）
# ---------------------------------------------------------------------------
def build_emu_section(rows):
    sys.path.insert(0, TOOLS)
    from ai_gen_parity import run_binary  # noqa: E402

    def fmt_tokens(toks):
        return ",".join("%d:%016x" % (t[0], t[1]) for t in toks)
    from ai_rng_inject import load_emu  # noqa: E402

    Gw = load_emu()

    def add(sec, key, value, note=""):
        rows.append((sec, key, str(value),
                     "unicorn 直调真二进制 %s" % note if note else "unicorn 直调真二进制", ""))

    def key_of(i):
        return bytes(((i * 37 + j * 11) & 0xFF) for j in range(32))

    def run(which, key, mode=0, tokens=()):
        toks, _cap, ln = run_binary(Gw, which, key, mode, tokens)
        return toks, ln

    K1 = key_of(1)
    add("E", "key1_hex", K1.hex(), "mutate/gen 用例的统一 key（32 B ChaCha key）")

    # ---- E06 mutate(无值 token) ----
    toks, ln = run("mutate", K1, 0, [(TAG_ADD, 0), (TAG_SIN, 0)])
    add("E06", "mutate_op_only", fmt_tokens(toks), "0x4ee720 输入 [Add,Sin]")

    # ---- E07 mutate(VarY) / E10 mutate(VarX) ----
    toks, ln = run("mutate", K1, 0, [(TAG_VARY, 0)])
    add("E07", "mutate_vary", fmt_tokens(toks), "0x4ee720 输入 [VarY]")
    toks, _ = run("mutate", K1, 0, [(TAG_VARX, 0)])
    add("E10", "mutate_varx", fmt_tokens(toks), "0x4ee720 输入 [VarX]")

    # ---- E08 mutate(Const) 200 个不同 key：是否每次都改 ----
    changed = 0
    samples = []
    for i in range(200):
        toks, _ = run("mutate", key_of(i), 0, [(TAG_CONST, struct.unpack("<Q", struct.pack("<d", 3.0))[0])])
        if len(toks) == 1 and toks[0][0] == TAG_CONST and toks[0][1] != struct.unpack("<Q", struct.pack("<d", 3.0))[0]:
            changed += 1
        if i < 3:
            samples.append(fmt_tokens(toks))
    add("E08", "mutate_const_changed_of_200", changed, "0x4ee720 输入 [Const(3.0)]，key=key_of(i)")
    add("E08", "mutate_const_samples", " | ".join(samples))

    # ---- E09/E11 mutate 多轮（同 key 连续喂回） ----
    cur = [(TAG_CONST, struct.unpack("<Q", struct.pack("<d", 10.0))[0])]
    seq = []
    lens = []
    for r in range(40):
        toks, _ = run("mutate", K1, 0, cur)
        cur = [(t, b) for t, b, _c, _l in toks]
        lens.append(len(cur))
        if cur and cur[0][0] == TAG_CONST:
            seq.append("%016x" % cur[0][1])
    add("E09", "mutate_rounds_const_bits", ",".join(seq[:12]),
        "0x4ee720 连喂 40 轮（每轮换 key：run_binary 每次新建 emulator，同 key 会重放同一条流）")
    add("E11", "mutate_rounds_lens", ",".join(str(x) for x in lens), "0x4ee720 连喂 40 轮的长度序列")

    # ---- E12/E14/E15 生成与合法性（N=1200 次调用 + 120 次 assemble 复核）----
    def not_final(tokens):
        """真二进制自己的判据：把 gen 的输出再喂 `assemble 0x4ee0e0`，输出变了 ⇒ 未定稿。

        ⚠️ 不要用"前缀式计数"当判据：真 `assemble` 会把**悬空二元算子丢掉**
        （`[Sin,VarX,Add] → [Sin,VarX]`，见 E13），说明原版的合法性判据不是前缀计数
        （T59 §106.6 已登记；我第一版就这里判错，改前先想清楚）。
        """
        inp = [(t, b) for t, b, _c, _l in tokens]
        out, _ln = run("assemble", K1, 0, inp)
        return [(t[0], t[1]) for t in out] != inp

    n_calls = 1200
    tags_hist = collections.Counter()
    lens_gen = []
    bad = 0
    alltok = 0
    for i in range(n_calls):
        toks, ln = run("gen", key_of(i), 0)
        tags = [t for t, _b, _c, _l in toks]
        lens_gen.append(ln)
        alltok += len(tags)
        tags_hist.update(tags)
        if not tags:
            bad += 1
    add("E02", "gen_calls", n_calls, "0x4ef020 mode=0，key=key_of(i)")
    add("E02", "gen_tokens_total", alltok)
    add("E02", "gen_tag_histogram", json.dumps({str(k): v for k, v in sorted(tags_hist.items())}))
    add("E03", "gen_tag_domain", ",".join(str(k) for k in sorted(tags_hist)))
    add("E03", "gen_forbidden_tags",
        json.dumps({str(t): tags_hist[t] for t in sorted(FORBIDDEN_NORMAL) if t in tags_hist}),
        "真二进制 mode=0 里不该出现的 tag（VarY/VarDy/UnaryMinus/括号）—— 实测为空")
    add("E04", "gen_len_mean", "%.4f" % (sum(lens_gen) / len(lens_gen)), "含 0 长度的样本（n<=0 直接空 Vec）")
    pos = [x for x in lens_gen if x > 0]
    add("E04", "gen_len_nonzero_mean", "%.4f" % (sum(pos) / len(pos)))
    add("E04", "gen_len_min", min(lens_gen))
    add("E04", "gen_len_max", max(lens_gen))
    add("E05", "gen_const_count", tags_hist[TAG_CONST])
    nf = 0
    for i in range(120):
        toks, _ln = run("gen", key_of(i), 0)
        if toks and not_final(toks):
            nf += 1
    add("E14", "gen_empty_of_120", bad, "裸生成器在 n<=0 时直接返回空 Vec（1200 次里的空样本数）")
    add("E12", "gen_not_final_of_120", nf,
        "gen 的输出再喂 assemble 会变（长度或 token）的例数 ⇒ 裸生成器不补齐，分层可观测")
    add("E12", "gen_final_of_120", 120 - nf)

    # ---- E13 assemble ----
    cases = [
        [(TAG_VARX, 0)],
        [(TAG_ADD, 0), (TAG_VARX, 0)],
        [(TAG_ADD, 0), (TAG_CONST, 0x4008000000000000)],
        [(TAG_SIN, 0), (TAG_VARX, 0), (TAG_ADD, 0)],
    ]
    for k, inp in enumerate(cases):
        toks, ln = run("assemble", K1, 0, inp)
        add("E13", "assemble_case%d" % k, "in=[%s] out=[%s]" % (fmt_tokens([(t, b) for t, b in inp]), fmt_tokens(toks)))

    # ---- E15 裸生成 vs assemble（长度三分：补长 / 同长 / 变短） ----
    grew = same = shrank = 0
    for i in range(120):
        toks, ln = run("gen", key_of(i), 0)
        inp = [(t, b) for t, b, _c, _l in toks]
        out, _ln2 = run("assemble", K1, 0, inp)
        ln2 = len(out)
        if ln2 > ln:
            grew += 1
        elif ln2 == ln:
            same += 1
        else:
            shrank += 1
    add("E15", "gen_vs_assemble_120", "grew=%d same=%d shrank=%d" % (grew, same, shrank),
        "0x4ef020 的输出再喂 0x4ee0e0：变长 = 补齐；变短 = 丢掉悬空算子")

    # ---- E01 权重直方图 vs 表（真二进制样本） ----
    add("E01", "histogram_source", "E02/gen_tag_histogram（真二进制 %d 次调用 / %d token）" % (n_calls, alltok))
    implied = sorted(((v, k) for k, v in tags_hist.items()), reverse=True)
    add("E01", "implied_weights", json.dumps({str(k): round(v * 85.0 / alltok, 3) for v, k in implied}),
        "真二进制直方图折成 85 分母的隐含权重（与 table() 比）")
    add("E01", "implied_weight_sum", "%.3f" % sum(v * 85.0 / alltok for v in tags_hist.values()))
    return rows


def pick_batch_lines(snap, replay, want_index=0):
    """从回放原文里**逐行抄**第 want_index 个结算批（含尾部到下一回合开始前的行）。

    返回原始 JSONL 行（不带外层 `\\n`），供测试里逐条嵌成常量（T46 `REAL_BATCH*` 的口径）。
    """
    path = os.path.join(snap, replay)
    raw = open(path, "rb").read()
    lines = (gzip.open(path, "rt", encoding="utf-8", errors="replace").readlines()
             if raw[:2] == b"\x1f\x8b" else open(path, "rt", encoding="utf-8", errors="replace").readlines())
    lines = [l.rstrip("\n") for l in lines if l.strip()]
    out = []
    idx = -1
    inside = False
    for l in lines:
        try:
            d = json.loads(l)
        except ValueError:
            continue
        gs = None
        if "GameStateInfo" in d.get("event", {}):
            gs = d["event"]["GameStateInfo"].get("game_state")
        if gs == "CalculatingResults":
            idx += 1
            if idx == want_index:
                inside = True
                out = [l]
            elif inside:
                break
            continue
        if inside:
            if gs in ("WaitingForFunctions",):
                break
            out.append(l)
    return out


# ---------------------------------------------------------------------------
# 段 B（真抓包帧）
# ---------------------------------------------------------------------------
def build_broker_section(rows):
    path = os.path.join(ROOT, "fixtures/protocol_match.jsonl")
    raw = open(path, "rb").read()
    sha = hashlib.sha256(raw).hexdigest()
    lines = [l for l in raw.decode("utf-8", "replace").splitlines() if l.strip()]

    def add(sec, key, value, note=""):
        rows.append((sec, key, str(value), "真实抓包 fixtures/protocol_match.jsonl sha256=%s" % sha, note))

    add("B", "fixture_sha256", sha)
    kinds = []
    for l in lines:
        try:
            d = json.loads(l)
        except ValueError:
            continue
        kinds.append(list(d.keys()))
    flat = ["+".join(k) for k in kinds]
    add("B04", "session_kinds", ",".join(flat[:16]), "抓包会话前 16 帧的事件名（顺序即真值）")
    add("B04", "frame_count", len(lines))

    def first_of(name):
        for l in lines:
            d = json.loads(l)
            if name in d:
                return l, d[name]
        return None, None

    for name, sec in (("GameInfo", "B01"), ("VersionInfo", "B03"), ("GameNameInfo", "B02"),
                      ("GameNameRequest", "B02"), ("NewConnectionRequest", "B02")):
        line, val = first_of(name)
        if line is None:
            add(sec, name, "MISSING", "抓包里没有这一帧")
            continue
        add(sec, name + "_frame", line)
        add(sec, name + "_keys", ",".join(val.keys()) if isinstance(val, dict) else "non-object")
    # room_name 形态（GameInfo 的房间表项）
    for l in lines:
        d = json.loads(l)
        if "room_name" in d:
            add("B06", "room_record", l)
            add("B06", "room_record_keys", ",".join(d.keys()))
            break
    # 帧长前缀：抓包 fixtures 存的是**解析后的文本**（tools/capture_net.js 的口径），
    # 不含 2 字节长度前缀 ⇒ B05 只能如实记"无原始字节样本"
    add("B05", "raw_frame_available", "no",
        "fixtures 里只有解析后文本（capture_net.js 口径）；u16 大端长度前缀无原始字节样本 ⇒ 该条降为推断")
    return rows


# ---------------------------------------------------------------------------
# 段 V（关卡生成：真实抓包）
# ---------------------------------------------------------------------------
def build_levelgen_section(rows):
    draws_path = os.path.join(ROOT, "fixtures/levelgen_draws.txt")
    obs_path = os.path.join(ROOT, "fixtures/levelgen_obstacles.txt")
    draws = [int(l.strip(), 16) for l in open(draws_path) if l.strip() and not l.startswith("#")]
    obs = []
    for l in open(obs_path):
        l = l.strip()
        if not l or l.startswith("#"):
            continue
        x, y, r = l.split("\t")
        obs.append((float(x), float(y), float(r)))
    sha_d = hashlib.sha256(open(draws_path, "rb").read()).hexdigest()
    sha_o = hashlib.sha256(open(obs_path, "rb").read()).hexdigest()

    def add(sec, key, value, note=""):
        rows.append((sec, key, str(value),
                     "真实抓包 levelgen_draws.txt sha256=%s / levelgen_obstacles.txt sha256=%s" % (sha_d, sha_o), note))

    add("V", "draws_count", len(draws))
    add("V", "obstacles_count", len(obs))
    rs = sorted(r for _x, _y, r in obs)
    xs = sorted(x for x, _y, _r in obs)
    ys = sorted(y for _x, y, _r in obs)
    add("V03", "radius_min", rs[0])
    add("V03", "radius_max", rs[-1])
    add("V03", "radius_mean", "%.6f" % (sum(rs) / len(rs)))
    add("V03", "radius_at_floor_0.8", sum(1 for r in rs if r == 0.8), "真实数据里半径正好 0.8 的个数（半径下限夹紧）")
    add("V03", "x_range", "[%.6f, %.6f]" % (xs[0], xs[-1]))
    add("V03", "y_range", "[%.6f, %.6f]" % (ys[0], ys[-1]))
    add("V04", "obstacle_at_origin", sum(1 for x, y, _r in obs if x == 0.0 and y == 0.0))
    add("V08", "radius_sorted_head", ",".join("%.6f" % r for r in rs[:6]))
    add("V08", "radius_sorted_tail", ",".join("%.6f" % r for r in rs[-4:]))
    add("V09", "obstacle_count", len(obs), "一次真实运行的障碍数（散布式）")
    add("V01", "first_draw", "%016x" % draws[0])
    add("V01", "draws_head", ",".join("%016x" % d for d in draws[:4]))
    add("V06", "draw_consumption", "3 + 2*13 = 29", "关卡生成从第 3 个随机数开始（docs/56），散布式每个障碍 2 个随机数")
    # 真实流 → 真实障碍的逐值对照（replays_captured_level_exactly 已用 include_str! 核过，这里登记指纹）
    add("V02", "obstacle_bits_sha256", hashlib.sha256(
        "".join(struct.pack("<ddd", *o).hex() for o in obs).encode()).hexdigest(),
        "13 个障碍 × (x,y,r) 的 IEEE754 位型串 sha256（测试里逐值比，这里只登记指纹）")
    return rows


# ---------------------------------------------------------------------------
def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", default=SNAP_DEFAULT)
    ap.add_argument("--out", default=os.path.join(ROOT, "fixtures/test_expectation_truth2.tsv"))
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--print", dest="do_print", action="store_true")
    ap.add_argument("--skip-emu", action="store_true")
    args = ap.parse_args(argv)

    rows = []
    c = Corpus(args.snapshot)
    build_corpus_section(c, rows)
    build_broker_section(rows)
    build_levelgen_section(rows)
    if not args.skip_emu:
        build_emu_section(rows)

    out = ["section\tkey\tvalue\tsource\tnote"]
    out += ["\t".join(str(x) for x in r) for r in rows]
    text = "\n".join(out) + "\n"
    if args.do_print:
        print(text)
        return 0
    if args.check:
        cur = open(args.out, encoding="utf-8").read()
        if cur == text:
            print("OK: %s 与复算一致（%d 行）" % (args.out, len(rows)))
            return 0
        print("MISMATCH: %s 与复算不一致（复算 %d 行）" % (args.out, len(rows)))
        return 1
    with open(args.out, "w", encoding="utf-8") as f:
        f.write(text)
    print("写 %s（%d 行）" % (args.out, len(rows)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
