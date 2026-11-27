#!/usr/bin/env python3
"""T73：第四批「S-self → 真值」的真值来源脚本（对应 `fixtures/test_expectation_plan4.tsv`）。

与 T46 / T59 / T64 同一套路（预注册 → 真值 → 表 → 测试里嵌常量），五段被测对象：

| 段 | 被测对象（file） | 真值来源 | 手段 |
|---|---|---|---|
| `F/*` | `src/_scaffold/protocol.rs` 与 `src/_scaffold/broker.rs` 的**帧格式** | **线上真 wire 字节**（`fixtures/protocol_live_frames.jsonl` 1717 帧；每帧带 `sha256`(负载) / `frame_sha256`(u16 前缀+负载) / `n`） | 先逐帧核 `sha256`/`frame_sha256`/`n`，再把「完整 frame 字节」按抓包顺序拼起来算 FNV-1a 64 |
| `L/*` | `src/_scaffold/protocol.rs` 的**字段数 / 键名键序** | 同上（线上真帧）+ `GameInfo` 52 帧 | 从负载**字节**解 JSON（保序）→ 顶层字段数直方图 + 键序 |
| `T/*` | `src/graphwar_turn_manager.rs` 的**回合广播** | **冻结回放语料 401 份**（`fixtures/guardrails.json` corpus，逐份 sha256） | 解 gzip JSONL，重建每个玩家的士兵表与生命表，逐条 `TurnInfo` 判状态 |
| `S/*` | `tests/sim_turns.rs` 的**同时回合形状** | 同上 | 91 份 `SimultaneousTurns` 回放的回合结构（每人一帧 TurnInfo、同 tick、EndOfTurnInfo 在后） |
| `R/*` | `tests/misc_modules.rs` 的 `CRASH_REPORT_PORT` | **真二进制指令**（Android 2.0.60 arm64，未 strip，`.symtab` 有真名） | `llvm-objdump` 读出 `mov w8/w10,#0xa7dc` 的 32 位字，测试里解码 MOVZ 立即数 |

⚠️ 口径（写在这里，免得下一个人再踩）：

1. **1717 帧里有 2 帧是打码帧**（`ChatMessage` / `ChatMessageRequest`，T53 隐私口径：
   正文换成 `<len=N>`）⇒ 这 2 帧的 `hex` 长度 ≠ `n`，**不进** `F/*` 的 frame 字节聚合。
   `F/frames_verified = 1715` 就是能逐字节重建的帧数。
2. **`T/*` 的「全灭」判据是「该玩家的士兵在发帧那一刻全部不在生命表里」**：生命表用
   `LifeInfo{alive}` + `EntityRemoved` + `RemoveTeam` 维护；玩家实体**不算**生命表实体
   （T64 §115.5 踩过：把玩家算进去会把对战局全判成有人）。
3. **`R/*` 是 arm64 Android 版的指令**（x86-64 目标 stripped、只能靠 Location 归属；
   arm64 未 strip，符号名直接指到 `GraphwarCrashReporter::resolve_default_address`）。

用法：

    .reference/venv/bin/python tools/gen_test_expectations4.py            # 写 fixtures/test_expectation_truth4.tsv
    .reference/venv/bin/python tools/gen_test_expectations4.py --check    # 只比对不写（复算一致性）
    .reference/venv/bin/python tools/gen_test_expectations4.py --print
"""

import argparse
import collections
import gzip
import hashlib
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SNAP_DEFAULT = os.path.join(ROOT, ".reference/replays/gw2-replays-snapshot/replays")
LIVE = os.path.join(ROOT, "fixtures/protocol_live_frames.jsonl")
OUT_DEFAULT = os.path.join(ROOT, "fixtures/test_expectation_truth4.tsv")

# x86-64 目标（2026-08-28，stripped；`.text` 的 vaddr - fileoff = 0x1000）
X86 = os.path.join(ROOT, ".reference/game/libgraphwarrust.so")
# GraphwarGameStateManager::get_alive_teams（`fixtures/boundary_audit.tsv` v2 唯一锚，640 字节）
A_ALIVE_TEAMS = 0x473520
# arm64 Android 2.0.60（未 strip）
ARM64 = os.path.join(ROOT, ".reference/steam/android-2.0.60/libgraphwarrust.so")
# GraphwarCrashReporter::resolve_default_address（.symtab 真名；980 字节）
A_RESOLVE = 0x4CA8B4
# 函数体里两条 `mov w8/w10,#0xa7dc`（llvm-objdump 读出）
PORT_WORDS = [(0x4CAB30, 0x5294FB88), (0x4CAB64, 0x5294FB8A)]

SUPPLEMENTED = [
    "NameRequest", "NameInfo", "RankRequest", "RankInfo", "LockInfo", "LockRequest",
    "ChatMessage", "ChatMessageRequest", "SkinGraphicsRequest", "SkinGraphicsInfo",
    "FaceGraphicsRequest", "FaceGraphicsInfo", "HatGraphicsRequest", "HatGraphicsInfo",
    "GameHeartbeat", "GameRemoved", "VersionInfo", "BotAdded", "TurnLimitInfo",
    "EntityEffectInfo", "ClearObstacles", "ClearBansRequest", "BanConnectionRequest",
]


def fnv1a64(buf):
    h = 0xCBF29CE484222325
    for b in buf:
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def hex_bytes(h):
    return bytes(int(h[i:i + 2], 16) for i in range(0, len(h), 2))


def ordered_pairs(text):
    """按原文顺序解 JSON（`object_pairs_hook=list` 保序）。"""
    return json.loads(text, object_pairs_hook=lambda p: p)


def top_fields(text):
    """事件顶层字段名（保序）：`{"Event":{...}}` → 内层对象的键列表。"""
    pairs = ordered_pairs(text)
    if len(pairs) != 1:
        raise SystemExit("不是单键帧：%r" % text[:80])
    body = pairs[0][1]
    if isinstance(body, list):
        return [k for k, _ in body]
    return []  # 空体 `{}`


# ---------------------------------------------------------------------------
# 抓包：逐帧核指纹
# ---------------------------------------------------------------------------
def load_live():
    rows = []
    meta = None
    for line in open(LIVE, encoding="utf-8"):
        line = line.strip()
        if not line:
            continue
        d = json.loads(line)
        if d.get("event") is None:
            meta = d
            continue
        rows.append(d)
    if meta is None:
        raise SystemExit("抓包缺 meta 行")
    return meta, rows


def verify_live(rows):
    """逐帧核 `sha256`(负载) / `frame_sha256`(u16 前缀 + 负载) / `n`。"""
    ok = masked = 0
    for r in rows:
        if r.get("hex_masked"):
            masked += 1
            continue
        payload = hex_bytes(r["hex"])
        n = r["n"]
        if len(payload) != n:
            raise SystemExit("帧长不符：%s n=%d len=%d" % (r["event"], n, len(payload)))
        if hashlib.sha256(payload).hexdigest() != r["sha256"]:
            raise SystemExit("负载 sha256 不符：%s" % r["event"])
        frame = n.to_bytes(2, "big") + payload
        if hashlib.sha256(frame).hexdigest() != r["frame_sha256"]:
            raise SystemExit("frame sha256 不符（前缀口径变了？）：%s" % r["event"])
        ok += 1
    return ok, masked


def build_frame_section(rows, rows_ok, rows_masked, out):
    def add(key, value, note=""):
        out.append(("F", key, str(value),
                    "线上真 wire 字节 fixtures/protocol_live_frames.jsonl（T53 抓包）", note))

    add("frames_total", len(rows), "抓包去重后帧数（meta.frames_deduped）")
    add("frames_verified", rows_ok, "能逐字节重建的帧（hex + sha256 + frame_sha256 + n 四项自校验通过）")
    add("frames_masked", rows_masked, "打码帧（ChatMessage / ChatMessageRequest，正文换 <len=N>）")
    add("prefix_rule", "u16be(n)||payload", "frame_sha256 = sha256(u16 大端长度 ‖ 负载)")

    wire = bytearray()
    verified = [r for r in rows if not r.get("hex_masked")]
    for r in verified:
        wire += r["n"].to_bytes(2, "big") + hex_bytes(r["hex"])
    add("wire_fnv", "0x%016x" % fnv1a64(bytes(wire)),
        "**全部 %d 帧的完整 wire 字节**（u16 前缀 + 负载）按抓包顺序拼接的 FNV-1a 64"
        % len(verified))
    add("wire_len", len(wire), "上式的字节总数")

    per = collections.Counter(r["event"] for r in rows)
    add("kinds", len(per), "抓包里出现过的事件种类数")

    # LAN 广播（另一个通道）：`fixtures/protocol_match.jsonl` 的 `{"room_name" …}` 行
    lan = [l.strip() for l in open(os.path.join(ROOT, "fixtures/protocol_match.jsonl"),
                                   encoding="utf-8")
           if l.strip().startswith('{"room_name"')]
    add("lan_broadcast_lines", len(lan), "旧抓包里 LAN 广播文本行数（无 u16 前缀）")
    add("lan_broadcast_first_byte", "0x%02x" % (lan[0].encode()[0] if lan else 0),
        "LAN 广播原文首字节（`{` = 0x7b；带前缀时首字节会是长度高位 0x00）")


# ---------------------------------------------------------------------------
# 补齐事件的字段数 / 键序
# ---------------------------------------------------------------------------
def build_live_section(rows, out):
    by = collections.defaultdict(list)
    for r in rows:
        if r.get("event") in SUPPLEMENTED or r.get("event") == "GameInfo":
            by[r["event"]].append(r)
    kinds = SUPPLEMENTED + ["GameInfo"]
    for kind in kinds:
        frames = by.get(kind, [])
        hist = collections.Counter()
        orders = set()
        joined = bytearray()
        for r in frames:
            payload = hex_bytes(r["hex"])
            joined += payload
            try:
                text = payload.decode("utf-8")
                fields = top_fields(text)
            except (UnicodeDecodeError, ValueError):
                fields = None
            if fields is None:
                hist["unparsed"] += 1
                continue
            hist[len(fields)] += 1
            orders.add(tuple(fields))
        note = "线上真帧" if frames else "线上 0 帧（换不了）"
        if kind == "TurnLimitInfo":
            note = "线上 0 帧；真值见 T/ 与 test_expectation_truth3.tsv 的语料段"
        out.append(("L", "%s/frames" % kind, str(len(frames)),
                    "线上真帧 fixtures/protocol_live_frames.jsonl", note))
        out.append(("L", "%s/fieldcounts" % kind,
                    json.dumps({str(k): v for k, v in sorted(hist.items(), key=lambda kv: str(kv[0]))},
                               sort_keys=True),
                    "同上：从**负载字节**解 JSON 后数顶层字段", "每种事件逐帧字段数恒定 ⇒ 期望值可直接钉"))
        out.append(("L", "%s/keyorders" % kind, str(len(orders)),
                    "同上", "真帧里出现过的不同键序个数（1 = 现场顺序唯一）"))
        if len(orders) == 1:
            out.append(("L", "%s/keyorder" % kind, "|".join(next(iter(orders))),
                        "同上", "**真帧现场键序**（我方序列化必须同序）"))
        if frames:
            out.append(("L", "%s/payload_fnv" % kind, "0x%016x" % fnv1a64(bytes(joined)),
                        "同上", "该事件全部帧负载按抓包顺序拼接的 FNV-1a 64（打码帧用打码后的字节）"))
        # `GameInfo` 的 9 个字段在**内层**对象里：`{"GameInfo":{"game_info":{…}}}`
        if kind == "GameInfo" and frames:
            inner = set()
            for r in frames:
                pairs = ordered_pairs(hex_bytes(r["hex"]).decode("utf-8"))
                body = pairs[0][1]
                gi = dict(body)["game_info"]
                inner.add(tuple(k for k, _ in gi))
            out.append(("L", "GameInfo/inner_keyorders", str(len(inner)), "同上",
                        "内层 `game_info` 对象的键序个数"))
            if len(inner) == 1:
                out.append(("L", "GameInfo/inner_keyorder", "|".join(next(iter(inner))), "同上",
                            "**真帧内层键序**（9 个字段；我方 `to_json()` 必须同序）"))
            else:
                raise SystemExit("GameInfo 内层键序不唯一：%s" % inner)

    # `VersionInfo` 线上 0 帧，但旧抓包 `fixtures/protocol_match.jsonl` 里有 1 帧（T59 已用）
    vi = [l.strip() for l in open(os.path.join(ROOT, "fixtures/protocol_match.jsonl"),
                                  encoding="utf-8") if l.strip().startswith('{"VersionInfo"')]
    out.append(("L", "VersionInfo/fixture_frames", str(len(vi)),
                "旧抓包 fixtures/protocol_match.jsonl", "线上 1717 帧里没有它；夹具里这一条是解析后文本"))
    if vi:
        out.append(("L", "VersionInfo/fixture_keyorder", "|".join(top_fields(vi[0])),
                    "同上", "旧抓包真帧的键序"))
        out.append(("L", "VersionInfo/fixture_fieldcount", str(len(top_fields(vi[0]))),
                    "同上", "旧抓包真帧的顶层字段数"))


# ---------------------------------------------------------------------------
# 冻结语料：指纹 + 回合
# ---------------------------------------------------------------------------
def verify_corpus(snap):
    g = json.load(open(os.path.join(ROOT, "fixtures/guardrails.json"), encoding="utf-8"))
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
    return got


def load_replay(path):
    raw = open(path, "rb").read()
    if raw[:2] == b"\x1f\x8b":
        text = gzip.decompress(raw).decode("utf-8", "replace")
    else:
        text = raw.decode("utf-8", "replace")
    out = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except ValueError:
            continue
    return out


def scan_turns(snap, files):
    """逐份回放重建「玩家 → 士兵表」「士兵 → 是否还活着」，逐条 TurnInfo 判状态。"""
    turninfo = 0
    turn_limits = []        # (file, value, 字段名元组)
    zero_all_dead = []      # (file, tick, player_id, nsoldiers)
    zero_empty_roster = []  # (file, tick, player_id)
    single = []             # (file, player_id, soldier_id, n_turns)
    empty_players = []      # (file, player_id, n_turns)  —— 从没有 SoldierInfo 的玩家
    sim_rounds = []         # (file, tick, n_turninfo, n_ffr, n_ff, n_gsi, order_ok)
    sim_files = 0
    files_scanned = 0
    for name in sorted(os.listdir(snap)):
        p = os.path.join(snap, name)
        if not os.path.isfile(p) or not name.isdigit():
            continue
        files_scanned += 1
        recs = load_replay(p)
        soldiers = collections.defaultdict(list)
        turns = collections.defaultdict(list)
        mode = None
        for r in recs:
            ev = r.get("event")
            if not isinstance(ev, dict) or len(ev) != 1:
                continue
            k, b = next(iter(ev.items()))
            if k == "TurnModeInfo":
                mode = b.get("turn_mode")
            if k == "TurnLimitInfo":
                keys = tuple(sorted(b.keys()))
                turn_limits.append((name, int(b.get("turn_limit")), keys))
            if k == "SoldierInfo":
                sid = b.get("soldier_id")
                pid = b.get("player_id")
                if sid is not None and pid is not None:
                    # ⚠️ `SoldierInfo` 会对同一个 soldier 重复发（实测 1793884550 玩家 1047 的
                    # 1082 出现两次）⇒ 士兵表按**首见顺序去重**（同一人的重复注册不算两个兵）
                    if sid not in soldiers[pid]:
                        soldiers[pid].append(sid)
        # 第二遍：维护生命表，逐条 TurnInfo 判状态
        alive = collections.defaultdict(lambda: True)
        created = set()
        for r in recs:
            ev = r.get("event")
            if not isinstance(ev, dict) or len(ev) != 1:
                continue
            k, b = next(iter(ev.items()))
            tick = r.get("tick")
            if k == "SoldierInfo":
                sid = b.get("soldier_id")
                pid = b.get("player_id")
                if sid is not None:
                    created.add(sid)
                    if pid is not None:
                        # 只保留**发帧时刻之前**已经存在的士兵
                        pass
            elif k == "LifeInfo":
                eid = b.get("entity_id")
                if eid is not None and "alive" in b:
                    alive[eid] = bool(b["alive"])
            elif k in ("EntityRemoved", "RemoveTeam"):
                eid = b.get("entity_id")
                if eid is not None:
                    alive[eid] = False
            elif k == "TurnInfo":
                turninfo += 1
                sid = b.get("soldier_id")
                pid = b.get("player_id")
                if sid is None:
                    continue
                turns[pid].append((tick, sid))
                if sid != 0:
                    continue
                ss = soldiers.get(pid, [])
                if not ss:
                    zero_empty_roster.append((name, tick, pid))
                else:
                    nalive = sum(1 for s in ss if alive[s])
                    if nalive == 0:
                        zero_all_dead.append((name, tick, pid, len(ss), tuple(ss)))
                    else:
                        raise SystemExit(
                            "%s tick=%s 玩家 %s 的士兵还有 %d 个活着却广播了 soldier_id=0"
                            % (name, tick, pid, nalive))
        for pid, ss in soldiers.items():
            if len(ss) == 1:
                ts = [s for _, s in turns.get(pid, [])]
                if len(ts) >= 2:
                    if len(set(ts)) != 1:
                        raise SystemExit("%s 玩家 %s 单士兵却发了多个 id %s" % (name, pid, set(ts)))
                    single.append((name, pid, ss[0], len(ts)))
        for pid, ts in turns.items():
            if not soldiers.get(pid):
                empty_players.append((name, pid, len(ts)))
        # 同时回合形状
        if mode == "SimultaneousTurns":
            sim_files += 1
            bytick = collections.defaultdict(list)
            for r in recs:
                ev = r.get("event")
                if not isinstance(ev, dict) or len(ev) != 1:
                    continue
                bytick[r.get("tick")].append(next(iter(ev)))
            for tick, ks in sorted(bytick.items(), key=lambda kv: (kv[0] is None, kv[0])):
                ti = ks.count("TurnInfo")
                if ti == 0:
                    continue
                try:
                    first = ks.index("TurnInfo")
                    last = len(ks) - 1 - ks[::-1].index("TurnInfo")
                    eot = ks.index("EndOfTurnInfo") if "EndOfTurnInfo" in ks else None
                except ValueError:
                    eot = None
                    first = last = -1
                ok = eot is None or (first < eot and last < eot)
                sim_rounds.append((name, tick, ti, ks.count("FunctionFireRequest"),
                                   ks.count("FunctionFire"), ks.count("GameStateInfo"), ok))
    return dict(files_scanned=files_scanned, turninfo=turninfo, turn_limits=turn_limits,
                zero_all_dead=zero_all_dead, zero_empty_roster=zero_empty_roster,
                single=single, empty_players=empty_players,
                sim_files=sim_files, sim_rounds=sim_rounds)


def build_turn_section(c, corpus, out):
    def add(key, value, note=""):
        out.append(("T", key, str(value),
                    "冻结语料 401 份回放快照（fixtures/guardrails.json corpus，逐份 sha256 已核）", note))

    add("files_scanned", c["files_scanned"], "扫过的回放份数")
    add("turninfo_frames", c["turninfo"], "`TurnInfo` 帧总数")
    add("soldier_zero_frames", len(c["zero_all_dead"]) + len(c["zero_empty_roster"]),
        "`TurnInfo.soldier_id == 0` 的帧数")
    add("soldier_zero_all_dead", len(c["zero_all_dead"]),
        "其中「该玩家有士兵、发帧时全死」的帧数")
    add("soldier_zero_all_dead_list",
        ";".join("%s:%s:%s:%s:%s" % (t[0], t[1], t[2], t[3], ",".join(str(x) for x in t[4]))
                 for t in sorted(c["zero_all_dead"])),
        "明细 file:tick:player_id:士兵数:士兵 id 列表（`PlayerTurns` 的输入就是这个顺序 = SoldierInfo 首见顺序）")
    add("soldier_zero_empty_roster", len(c["zero_empty_roster"]),
        "其中「该玩家从来没有 SoldierInfo（空队）」的帧数")
    add("soldier_zero_empty_roster_list",
        ";".join("%s:%s:%s" % t for t in sorted(c["zero_empty_roster"])),
        "明细 file:tick:player_id")
    add("files_with_all_dead_zero",
        ",".join(sorted({t[0] for t in c["zero_all_dead"]})),
        "出现「全灭广播 0」的回放文件")
    add("single_soldier_players", len(c["single"]),
        "「玩家只有一个士兵且连发 ≥2 次 TurnInfo」的人数")
    add("single_soldier_list",
        ";".join("%s:%s:%s:%s" % t for t in c["single"]),
        "明细 file:player_id:soldier_id:连发次数（全部同一个 id）")
    add("empty_roster_players", len(c["empty_players"]),
        "「有 TurnInfo 但从来没有 SoldierInfo」的玩家数")
    add("empty_roster_players_list",
        ";".join("%s:%s:%s" % t for t in c["empty_players"]), "明细 file:player_id:TurnInfo 次数")
    add("corpus_names_sha256", corpus["names_digest"], "文件名集合指纹")

    tl = c["turn_limits"]
    add("turn_limit_frames", len(tl), "`TurnLimitInfo` 帧数（线上 1717 帧抓不到它 ⇒ 语料是唯一真值来源）")
    add("turn_limit_values", json.dumps({str(k): v for k, v in
                                         sorted(collections.Counter(t[1] for t in tl).items())}),
        "取值直方图")
    add("turn_limit_keyorder", "|".join(sorted({k for t in tl for k in t[2]})),
        "`TurnLimitInfo` 载荷的字段名（语料重建文本的键）")
    add("turn_limit_files", ",".join(sorted(t[0] for t in tl)), "出现的回放文件")

    rounds = c["sim_rounds"]

    def adds(key, value, note=""):
        out.append(("S", key, str(value),
                    "冻结语料 401 份回放快照（fixtures/guardrails.json corpus，逐份 sha256 已核）", note))

    adds("sim_files", c["sim_files"], "`TurnModeInfo == SimultaneousTurns` 的回放份数")
    adds("sim_rounds", len(rounds), "这些回放里出现 TurnInfo 的回合数")
    hist = collections.Counter(r[2] for r in rounds)
    adds("turninfo_per_round", json.dumps({str(k): v for k, v in sorted(hist.items())}),
        "每回合 `TurnInfo` 帧数直方图（= 该回合玩家数）")
    adds("end_of_turn_after_all_turninfo", sum(1 for r in rounds if r[6]),
        "`EndOfTurnInfo` 出现在该回合**全部** TurnInfo 之后的回合数")
    adds("function_fire_request_frames", sum(r[3] for r in rounds),
        "这些回合里 `FunctionFireRequest` 帧数（client→server 事件，回放不录 ⇒ 0）")


def elf_sections(path):
    """极简 ELF64 段表解析（只要 `.text` 的 sh_addr/sh_offset）。"""
    data = open(path, "rb").read()
    e_shoff = int.from_bytes(data[0x28:0x30], "little")
    e_shentsize = int.from_bytes(data[0x3A:0x3C], "little")
    e_shnum = int.from_bytes(data[0x3C:0x3E], "little")
    e_shstrndx = int.from_bytes(data[0x3E:0x40], "little")
    def sh(i):
        b = e_shoff + i * e_shentsize
        return dict(name=int.from_bytes(data[b:b + 4], "little"),
                    addr=int.from_bytes(data[b + 16:b + 24], "little"),
                    off=int.from_bytes(data[b + 24:b + 32], "little"),
                    size=int.from_bytes(data[b + 32:b + 40], "little"))
    strtab = sh(e_shstrndx)
    def name_of(i):
        b = strtab["off"] + sh(i)["name"]
        end = data.index(b"\x00", b)
        return data[b:end].decode()
    return {name_of(i): sh(i) for i in range(e_shnum)}, data


def text_bytes(vaddr, n):
    secs, data = elf_sections(X86)
    t = secs[".text"]
    off = t["off"] + (vaddr - t["addr"])
    return data[off:off + n]


def build_rodata_section(out):
    def add(key, value, note=""):
        out.append(("R", key, str(value),
                    "真二进制 arm64 Android 2.0.60（未 strip，.symtab 有真名）", note))

    def addx(key, value, note=""):
        out.append(("R", key, str(value),
                    "真二进制 x86-64 2026-08-28（stripped；指令字节按 `.text` 的 vaddr/offset 映射读出）",
                    note))

    digest = hashlib.sha256(open(ARM64, "rb").read()).hexdigest()
    add("binary", "android-2.0.60/libgraphwarrust.so", "来源路径（.reference/，gitignore）")
    add("binary_sha256", digest, "整份二进制 sha256")
    add("port_function", "GraphwarCrashReporter::resolve_default_address",
        "`.symtab` 真名（arm64 未 strip）")
    add("port_function_addr", "0x%x" % A_RESOLVE, "ELF vaddr（= 模块内偏移 = RVA）")
    add("port_function_len", 980, "FDE 字节数")
    add("port_words",
        ",".join("0x%x:0x%08x" % (a, w) for a, w in PORT_WORDS),
        "函数体里两条 `mov w8/w10,#0xa7dc` 的（地址, 4 字节小端字）")
    value = 42972
    for _, w in PORT_WORDS:
        imm16 = (w >> 5) & 0xFFFF
        hw = (w >> 21) & 0x3
        if imm16 != value or hw != 0:
            raise SystemExit("MOVZ 解码不符：0x%08x -> imm16=0x%x hw=%d" % (w, imm16, hw))
    add("port_value", value, "两条指令解码出的立即数（MOVZ Wd,#0x%x）" % value)

    # ---- x86-64：get_alive_teams（0x473520，640 字节）「空生命表」分支 ----
    # 这一条给 `tests/win_rule.rs::empty_match_has_no_verdict` 用：原版对这个函数
    # 的**空表**输入走哪条分支、写什么返回值，是**指令级**可读的。
    addx("alive_teams_function", "GraphwarGameStateManager::get_alive_teams",
        "boundary_audit.tsv 的 v2 唯一锚（Location 行 867/868/869/875/876）")
    addx("alive_teams_addr", "0x%x" % A_ALIVE_TEAMS, "ELF vaddr（= 模块内偏移 = RVA）")
    addx("alive_teams_len", 640, "FDE 字节数")
    secs, data = elf_sections(X86)
    t = secs[".text"]
    addx("alive_teams_text_delta", "0x%x" % (t["addr"] - t["off"]),
        "`.text` 的 vaddr − fileoff（复算指令字节时用：fileoff = vaddr − 这个值）")
    addx("x86_binary_sha256", hashlib.sha256(data).hexdigest(), "整份二进制 sha256")

    guard = text_bytes(0x4735AE, 9)          # test %rbx,%rbx ; je rel32（3+6 字节）
    if guard[:3] != b"\x48\x85\xdb" or guard[3:5] != b"\x0f\x84":
        raise SystemExit("0x4735ae 处不是 `test %rbx,%rbx` / `je rel32`：%s" % guard.hex())
    je_addr = 0x4735B1
    rel = int.from_bytes(guard[5:9], "little", signed=True)  # je 的后 4 字节 = rel32
    target = je_addr + 6 + rel
    addx("alive_teams_guard", "0x%x:%s" % (0x4735AE, guard.hex()),
        "`48 85 db` = test %rbx,%rbx（rbx = 生命表长度）；`0f 84 <rel32>` = je（非零才进循环）")
    addx("alive_teams_guard_target", "0x%x" % target,
        "上一条 je 的跳转目标（= 空表分支的入口；测试里用 rel32 自己算一遍）")
    zero_t2 = text_bytes(target, 8)          # movl $0x0, 0x4(%rsp)
    zero_t1 = text_bytes(target + 8, 7)      # movl $0x0, (%rsp)（7 字节编码）
    if zero_t2.hex() != "c744240400000000" or zero_t1.hex() != "c7042400000000":
        raise SystemExit("空表分支不是两条清零：%s / %s" % (zero_t2.hex(), zero_t1.hex()))
    addx("alive_teams_zero_t2", "0x%x:%s" % (target, zero_t2.hex()),
        "`movl $0x0,0x4(%rsp)`（第二个返回值 = Team2 有活体 = false）")
    addx("alive_teams_zero_t1", "0x%x:%s" % (target + 8, zero_t1.hex()),
        "`movl $0x0,(%rsp)`（第一个返回值 = Team1 有活体 = false）")
    addx("alive_teams_loop_dead_exit", "0x%x" % 0x47367D,
        "循环跑完没找到活人时也落到这里（两条返回值仍是 0x4735be/0x4735c5 的初值 0）")


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", default=SNAP_DEFAULT)
    ap.add_argument("--out", default=OUT_DEFAULT)
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--print", dest="do_print", action="store_true")
    ap.add_argument("--skip-corpus", action="store_true")
    args = ap.parse_args(argv)

    rows = []
    meta, live = load_live()
    ok, masked = verify_live(live)
    build_frame_section(live, ok, masked, rows)
    build_live_section(live, rows)
    if not args.skip_corpus:
        files = verify_corpus(args.snapshot)
        names = "".join(n + "\n" for n in sorted(files))
        corpus = dict(names_digest=hashlib.sha256(names.encode()).hexdigest())
        c = scan_turns(args.snapshot, files)
        c.update(corpus)
        build_turn_section(c, corpus, rows)
    build_rodata_section(rows)

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
