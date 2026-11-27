#!/usr/bin/env python3
"""T64：第三批「S-self → 真值」的真值来源脚本（对应 `fixtures/test_expectation_plan3.tsv`）。

与 T46 / T59 同一套路（预注册 → 真值 → 表 → 测试里嵌常量），三段被测对象、三种真值来源：

| 段 | 被测对象（file） | 真值来源 | 手段 |
|---|---|---|---|
| `P/*` | `src/_scaffold/protocol.rs`（23 个补齐事件的往返/字段） | **线上真帧**（`fixtures/protocol_live_frames.jsonl`，T53 抓包 1717 帧）+ **冻结回放语料**（TurnLimitInfo 21 帧）+ **旧抓包**（VersionInfo） | 逐帧核 sha256/n 后再用；每帧负载字节就是期望值 |
| `W/*` | `tests/win_rule.rs`（胜负判据） | **403 份冻结回放的终局状态**（217 份含 `GameStateInfo{"game_state":"EndingGame"}`） | 纯 Python 解 gzip JSONL，重建 TeamInfo/SoldierInfo/SkinGraphicsInfo/LifeInfo 终态 |
| `M/*` | `tests/misc_modules.rs`（`polish_notation_converter` 的 reorder） | **unicorn 直调真二进制**：解析器 `0x4ed670`（含 reorder）+ `PolishNotationConverter::reorder_recursive 0x4f5450` | 复用 `tools/bitexact_run.py::Real` |

⚠️ 两条实测口径（写在这里，免得下一个人再踩）：

1. **真解析器的非 CONST token 的 `value` 槽是堆上残留**（原版只给 CONST 写 value）。
   所以`P/*` 与 `M/*` 的期望值只钉「tag 序列 + CONST 的 value 位型」，不钉算子的 value
   （与 `fixtures/bit_exact_inputs.txt` 里算子 value 恒 0 的写法一致）。
2. **`TurnLimitInfo` 的真帧文本是「回放记录的 event 对象 → 紧凑 JSON」重建**（线上负载抓不到它，
   见 `docs/06` §110.5：T53 是在解压后的回放里扫到的 21 帧）。这一行的档位是
   `观测（回放记录）`，不是「线上负载字节」。

用法：

    .reference/venv/bin/python tools/gen_test_expectations3.py            # 写 fixtures/test_expectation_truth3.tsv
    .reference/venv/bin/python tools/gen_test_expectations3.py --check    # 只比对不写（复算一致性）
    .reference/venv/bin/python tools/gen_test_expectations3.py --print
    .reference/venv/bin/python tools/gen_test_expectations3.py --skip-emu # 跳过 unicorn 段（快）

**语料/抓包指纹先行**：403 份回放按 `fixtures/guardrails.json` 的 `corpus.files` 逐份核 sha256；
1717 条真帧按帧自带的 `sha256`/`frame_sha256`/`n` 逐帧核。任何一处不符直接退出。
"""

import argparse
import collections
import gzip
import hashlib
import json
import os
import re
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SNAP_DEFAULT = os.path.join(ROOT, ".reference/replays/gw2-replays-snapshot/replays")
LIVE = os.path.join(ROOT, "fixtures/protocol_live_frames.jsonl")
PROTOCOL_RS = os.path.join(ROOT, "src/_scaffold/protocol.rs")
TOOLS = os.path.join(ROOT, "tools")
OUT_DEFAULT = os.path.join(ROOT, "fixtures/test_expectation_truth3.tsv")

# 真二进制函数地址（ELF vaddr）
A_PARSE = 0x4ED670          # MathFunction::new（解析器；后半段就是 reorder）
A_REORDER = 0x4F5450        # PolishNotationConverter::reorder_recursive

TAG_CONST = 17


# ---------------------------------------------------------------------------
# 小工具
# ---------------------------------------------------------------------------
def fnv1a64(buf):
    h = 0xCBF29CE484222325
    for b in buf:
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def bits(v):
    return struct.unpack("<Q", struct.pack("<d", v))[0]


def fmt_tokens(toks):
    """tag 序列 + CONST 的 value 位型（口径见文件头第 1 条）。"""
    out = []
    for t, v in toks:
        if t == TAG_CONST:
            out.append("%d:%016x" % (t, bits(v)))
        else:
            out.append("%d" % t)
    return ",".join(out)


def supplemented_names():
    """从 `src/_scaffold/protocol.rs` 的 `SUPPLEMENTED_EVENT_NAMES` 抽出 23 个名字（不手抄）。"""
    src = open(PROTOCOL_RS, encoding="utf-8").read()
    m = re.search(r"pub const SUPPLEMENTED_EVENT_NAMES: \[&str; 23\] = \[(.*?)\];", src, re.S)
    if not m:
        sys.exit("找不到 SUPPLEMENTED_EVENT_NAMES")
    return re.findall(r'"([A-Za-z0-9_]+)"', m.group(1))


# ---------------------------------------------------------------------------
# 段 P/：线上真帧
# ---------------------------------------------------------------------------
def build_live_section(rows):
    def add(sec, key, value, src, note=""):
        rows.append((sec, key, str(value), src, note))

    SRC = "线上真帧 fixtures/protocol_live_frames.jsonl（T53 抓包；探针 tools/live_capture/live_wire.js）"
    if not os.path.exists(LIVE):
        sys.exit("缺 %s" % LIVE)
    lines = [l for l in open(LIVE, encoding="utf-8").read().splitlines() if l.strip()]
    meta = json.loads(lines[0])
    frames = [json.loads(l) for l in lines[1:]]
    if meta.get("frames_deduped") != len(frames):
        sys.exit("会话头说 %s 帧，实际 %d" % (meta.get("frames_deduped"), len(frames)))

    # 逐帧核：payload 长度 / payload sha256 / 带 u16 长度前缀的帧 sha256
    #
    # ⚠️ 打码帧（`hex_masked=true`，只有 `ChatMessage*` 的正文）不适用：它的 `hex` 是
    # **打码后**的文本（`<len=N>` 比原文短/长），而 `n`/`sha256` 是**原始**线上负载的口径。
    # 对打码帧只核「能解成 UTF-8 且 `masked_lens` 里的键都在」——打码规则本身由
    # T53 的 `tools/live_capture/parse_live.py` 保证，这里不重复实现。
    bad = []
    masked = 0
    for i, f in enumerate(frames):
        payload = bytes.fromhex(f["hex"])
        if f.get("hex_masked"):
            masked += 1
            text = payload.decode("utf-8", "replace")
            for key in (f.get("masked_lens") or {}):
                if '"%s":"<len=' % key not in text:
                    bad.append((i, "masked_key"))
            continue
        if len(payload) != f["n"]:
            bad.append((i, "n"))
            continue
        if hashlib.sha256(payload).hexdigest() != f["sha256"]:
            bad.append((i, "sha256"))
            continue
        frame = struct.pack(">H", len(payload)) + payload
        if hashlib.sha256(frame).hexdigest() != f["frame_sha256"]:
            bad.append((i, "frame_sha256"))
    if bad:
        sys.exit("真帧自校验失败（前 5 条）：%s" % bad[:5])

    kinds = supplemented_names()
    per = collections.OrderedDict((k, []) for k in kinds)
    for f in frames:
        if f["event"] in per:
            per[f["event"]].append(f)

    add("P/live", "frames", len(frames), SRC, "会话头 frames_deduped=%s；未打码的 %d 帧逐帧 sha256/frame_sha256/n 自校验通过，"
        "打码帧 %d 条（只核打码键）" % (meta.get("frames_deduped"), len(frames) - masked, masked))
    add("P/live", "kinds", len({f["event"] for f in frames}), SRC, "56 种事件")
    covered = [(k, v) for k, v in per.items() if v]
    total = sum(len(v) for _, v in covered)
    add("P/live", "supplemented_kinds", len(covered), SRC, "23 个补齐事件里有 %d 种抓到真帧" % len(covered))
    add("P/live", "supplemented_frames", total, SRC, "这 %d 种事件的帧总数（新增硬闸要逐帧回放）" % len(covered))

    for k in kinds:
        fs = per[k]
        if not fs:
            add("P/%s" % k, "frames", 0, SRC,
                "换不了：1717 帧 0 帧（docs/06 §110.6 的缺触发条件）")
            continue
        payloads = [bytes.fromhex(f["hex"]) for f in fs]
        blob = b"".join(payloads)
        dirs = collections.Counter(f["dir"] for f in fs)
        masked = sum(1 for f in fs if f.get("hex_masked"))
        add("P/%s" % k, "frames", len(fs), SRC,
            "方向 %s%s" % (dict(dirs), "；打码帧 %d 条" % masked if masked else ""))
        add("P/%s" % k, "fnv1a64", "%016x" % fnv1a64(blob), SRC,
            "%d 帧负载按抓包顺序拼接的 FNV-1a 64（测试里用同一个 FNV 复算）" % len(fs))
        add("P/%s" % k, "sha256", hashlib.sha256(blob).hexdigest(), SRC, "同上，sha256 口径")
        add("P/%s" % k, "first", payloads[0].decode("utf-8", "replace"), SRC, "首帧原文")


# ---------------------------------------------------------------------------
# 段 P/（回放）+ 段 W/：冻结语料
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
    names = "".join(n + "\n" for n in sorted(got))
    return got, hashlib.sha256(names.encode()).hexdigest()


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


class Corpus:
    def __init__(self, snap):
        self.snap = snap
        self.files, self.names_digest = verify_corpus(snap)
        self.turn_limits = []        # (file, value, payload_text)
        self.endings = []            # dict per ending replay
        for name in sorted(os.listdir(snap)):
            p = os.path.join(snap, name)
            if not os.path.isfile(p) or not name.isdigit():
                continue
            self._scan(name, load_replay(p))

    def _scan(self, name, recs):
        team = {}
        kind = {}
        alive = {}
        pos = {}
        turn_limit = None
        turn_count = 0
        end_tick = None
        for r in recs:
            ev = r.get("event")
            if not isinstance(ev, dict) or len(ev) != 1:
                continue
            (k, body) = next(iter(ev.items()))
            tick = r.get("tick")
            if k == "TeamInfo":
                eid = body.get("entity_id")
                if eid is not None and body.get("team") in ("Team1", "Team2"):
                    team[eid] = 1 if body["team"] == "Team2" else 0
                    alive.setdefault(eid, True)
            elif k == "SoldierInfo":
                # ⚠️ `SoldierInfo` 的键是 `soldier_id`（不是 `entity_id`），另有 `player_id`。
                eid = body.get("soldier_id", body.get("entity_id"))
                if eid is not None:
                    kind[eid] = "soldier"
                    alive.setdefault(eid, True)
            elif k == "PlayerInfo":
                # 玩家**不是**生命表里的实体（`0x473520` 数的是生命表）⇒ 只登记 kind、
                # 不进存活统计：否则「该队士兵全灭」永远判不出来（实测：把玩家算进去后
                # 217 个终局只剩 45 个「一队零活体」，172 个对战局全被判成双方都有活人）。
                eid = body.get("entity_id")
                if eid is not None:
                    kind.setdefault(eid, "player")
            elif k in ("PosInfo", "PosInfoList"):
                eid = body.get("entity_id")
                if eid is not None and body.get("pos_x") is not None:
                    pos[eid] = (body.get("pos_x"), body.get("pos_y"))
                    alive.setdefault(eid, True)
            elif k == "SkinGraphicsInfo":
                # 只有 `graphics_str == "target"` 才是靶子（`docs/150` §Q1：127/127 个靶子
                # 都是 `SkinGraphicsInfo="target"`）。⚠️ **非 target 的 skin 不代表是士兵** ——
                # 玩家实体也发 `SkinGraphicsInfo`，把它当士兵会把玩家算进生命表 ⇒ 对战局的
                # 「该队士兵全灭」永远判不出来（实测踩过：217 个终局只剩 45 个判成一队全灭）。
                eid = body.get("entity_id")
                if eid is not None:
                    alive.setdefault(eid, True)
                    if body.get("graphics_str") == "target":
                        kind[eid] = "target"
            elif k == "LifeInfo":
                eid = body.get("entity_id")
                if eid is not None and "alive" in body:
                    alive[eid] = bool(body["alive"])
            elif k in ("EntityRemoved", "RemoveTeam"):
                # 实体**离开生命表**：实战里对战局的收场大量走这条路（不是 `LifeInfo{alive:false}`）。
                # 实测踩过：只认 LifeInfo 时 217 个终局里有 33 个「双方都还有活人」，
                # 补上 EntityRemoved/RemoveTeam 后降到 0（见 docs/06 §111）。
                eid = body.get("entity_id")
                if eid is not None:
                    alive[eid] = False
            elif k == "TurnLimitInfo":
                v = body.get("turn_limit")
                if v is not None:
                    turn_limit = v
                    payload = json.dumps({"TurnLimitInfo": {"turn_limit": int(v)}},
                                         separators=(",", ":"))
                    self.turn_limits.append((name, int(v), payload))
            elif k == "TurnCountInfo":
                if body.get("turn_count") is not None:
                    turn_count = int(body["turn_count"])
            elif k == "GameStateInfo" and body.get("game_state") == "EndingGame":
                end_tick = tick
        if end_tick is None:
            return
        # 生命表里的实体 = 士兵 + 靶子（玩家不算；见上面 `PlayerInfo` 那段）
        per_team = {0: [], 1: []}
        for eid, tm in team.items():
            if kind.get(eid) in ("soldier", "target"):
                per_team[tm].append(eid)
        comp = []
        for eid in sorted(team):
            comp.append("%d:%d:%s:%s" % (eid, team[eid], kind.get(eid, "unknown"),
                                         "a" if alive.get(eid, True) else "d"))
        alive_cnt = {tm: sum(1 for eid in per_team[tm] if alive.get(eid, True)) for tm in (0, 1)}
        targets = sum(1 for eid in team if kind.get(eid) == "target")
        self.endings.append(dict(
            file=name, tick=end_tick, turn_limit=turn_limit, turn_count=turn_count,
            comp=comp, alive=alive_cnt, targets=targets,
            soldiers_only=(targets == 0),
            pos={str(e): pos.get(e) for e in team},
        ))


def build_corpus_section(c, rows):
    def add(sec, key, value, src, note=""):
        rows.append((sec, key, str(value), src, note))

    SRC = "冻结语料 403 份快照（fixtures/guardrails.json corpus，逐份 sha256 已核）"
    add("corpus", "names_sha256", c.names_digest, SRC,
        "文件名集合指纹（与 guardrails.json corpus.names_sha256 同口径）")
    add("corpus", "files", len(c.files), SRC, "401 份回放 + MANIFEST.tsv + SHA256SUMS")

    # ---- TurnLimitInfo（T53 §110.5：解压后 21 帧；T46 的「零帧」是 gzip-grep 假阴性）----
    vals = collections.Counter(v for _f, v, _p in c.turn_limits)
    files = {f for f, _v, _p in c.turn_limits}
    add("P/TurnLimitInfo", "corpus_frames", len(c.turn_limits), SRC,
        "%d 份回放各 1 帧；T53 §110.5 用解压重扫纠正了 T46 的 gzip-grep 假阴性" % len(files))
    add("P/TurnLimitInfo", "corpus_values", json.dumps(dict(sorted(vals.items())), ensure_ascii=False), SRC,
        "取值直方图（T53 记的是 1/5/6/10）")
    add("P/TurnLimitInfo", "corpus_files", len(files), SRC, "含该事件的回放份数")
    add("P/TurnLimitInfo", "fnv1a64", "%016x" % fnv1a64(b"".join(p.encode() for _f, _v, p in c.turn_limits)),
        SRC, "21 条重建文本按文件名顺序拼接的 FNV-1a 64")
    add("P/TurnLimitInfo", "first", c.turn_limits[0][2], SRC,
        "首帧：回放记录的 event 对象 → 紧凑 JSON（口径见文件头第 2 条）")
    for _f, v, p in c.turn_limits:
        add("P/TurnLimitInfo/%d" % v, "payload", p, SRC, "值 %d 的重建文本" % v)

    # ---- 终局（W 段）----
    n_files = sum(1 for f in c.files if f.isdigit())
    n_end = len(c.endings)
    add("W/corpus", "replays", n_files, SRC, "401 份回放")
    add("W/corpus", "ending_frames", n_end, SRC,
        "含 GameStateInfo{game_state:EndingGame} 的回放数（**T46「403 份 0 次」是 gzip-grep 假阴性**）")
    both_wiped = [e for e in c.endings if e["alive"][0] == 0 and e["alive"][1] == 0]
    one_wiped = [e for e in c.endings if (e["alive"][0] == 0) != (e["alive"][1] == 0)]
    add("W/corpus", "one_team_wiped", len(one_wiped), SRC, "恰好一队零活实体")
    add("W/corpus", "both_teams_wiped", len(both_wiped), SRC,
        "双方同时零活体（W02 的真值：%d 例）" % len(both_wiped))
    lim = [e for e in c.endings if e["turn_limit"] is not None and e["turn_count"] >= e["turn_limit"]]
    add("W/corpus", "turn_limit_reached", len(lim), SRC,
        "终局时 turn_count 已到上限（真上限用尽的收场；W04 相关）")
    add("W/corpus", "ending_with_target", sum(1 for e in c.endings if e["targets"] > 0), SRC,
        "终局里含靶子的（打靶关）")
    add("W/corpus", "ending_soldiers_only", sum(1 for e in c.endings if e["targets"] == 0), SRC,
        "终局里全是士兵的（对战/联机局）")

    # ---- 两个真实终局样本（W01 用）----
    def pick(pred, label):
        cand = [e for e in c.endings if pred(e) and e in one_wiped]
        if not cand:
            add("W01/%s" % label, "case", "NONE", SRC, "语料里没有符合条件的终局")
            return
        e = cand[0]
        add("W01/%s" % label, "file", e["file"], SRC, "回放 id（快照里的文件名）")
        add("W01/%s" % label, "ending_tick", e["tick"], SRC, "GameStateInfo{EndingGame} 的 tick")
        add("W01/%s" % label, "alive", json.dumps(e["alive"], sort_keys=True), SRC,
            "终局两队活实体数（Team1/Team2）")
        wiped = [t for t in (0, 1) if e["alive"][t] == 0]
        add("W01/%s" % label, "wiped_team", wiped[0], SRC, "零活体的一队 = 输的一队")
        add("W01/%s" % label, "winner", 1 - wiped[0], SRC, "另一队 = 真值里的胜方")
        add("W01/%s" % label, "composition", "|".join(e["comp"]), SRC,
            "终局队伍构成 id:team:soldier|target:alive(a)/dead(d)；测试按这个重建 Match")
        add("W01/%s" % label, "targets", e["targets"], SRC, "其中的靶子个数")
        add("W01/%s" % label, "turn_limit", e["turn_limit"], SRC, "该局有没有 TurnLimitInfo")

    pick(lambda e: e["targets"] > 0, "target_level")
    pick(lambda e: e["targets"] == 0, "versus_level")

    # ---- 双方同时清空（W02）：语料里**有**这种收场，而不是 0 例 ----
    add("W02/corpus", "both_wiped", len(both_wiped), SRC,
        "`tests/win_rule.rs::mutual_wipe_has_no_winner` 的注释写着「语料里没有这种收场」——**本轮证伪**：%d 例真终局双方零活体"
        % len(both_wiped))
    if both_wiped:
        e = both_wiped[0]
        add("W02/corpus", "file", e["file"], SRC, "真终局样本")
        add("W02/corpus", "composition", "|".join(e["comp"]), SRC,
            "终局构成 id:team:kind:a/d；两队士兵全 d、只剩 player 实体 a")
        add("W02/corpus", "alive", json.dumps(e["alive"], sort_keys=True), SRC, "两队活实体数")
        add("W02/corpus", "targets", e["targets"], SRC, "0 = 对战局")

    # ---- 回合上限用尽的收场（W04 相关）----
    if lim:
        e = lim[0]
        add("W04/corpus", "file", e["file"], SRC, "终局时 turn_count 已到上限的真实回放")
        add("W04/corpus", "turn_limit", e["turn_limit"], SRC, "该局的 TurnLimitInfo 值")
        add("W04/corpus", "turn_count", e["turn_count"], SRC, "终局时的 turn_count")
        add("W04/corpus", "alive", json.dumps(e["alive"], sort_keys=True), SRC, "两队活实体数")
        add("W04/corpus", "composition", "|".join(e["comp"]), SRC, "终局构成")


# ---------------------------------------------------------------------------
# 段 M/：unicorn 直调（解析器 + reorder）
# ---------------------------------------------------------------------------
# polish_tests 的 16 条：输入就是测试里 `to_prefix(&[...])` 那份**中缀 token 序列**
# （`function_token::new` 的产物；tag 见 `src/lib.rs::Function`：
#  VarX=0 VarY=1 VarDy=2 Add=3 Sub=4 UnaryMinus=5 Mul=6 Divide=7 Pow=8
#  Sqrt=9 Log=10 Ln=11 Exp=12 Abs=13 Sin=14 Cos=15 Tan=16 Const=17 LeftBracket=18 RightBracket=19）。
#
# ⇒ 真值一律 = 把**这份 token 序列**直接喂 `PolishNotationConverter::reorder_recursive 0x4f5450`。
# ⚠️ 不要改成喂 `0x4ed670` 解析器：那是「文本 → 前缀」，中间还夹着一趟分词，
#    而测试的输入是**已经分好词的中缀 token**（口径不同，见 `docs/06` §111.4）。
REORDER_CASES = [
    ("M01", "empty_input_stays_empty", []),
    ("M02", "single_atom_passes_through", [(0, 0.0)]),
    ("M03", "multiplication_binds_tighter_than_addition",
     [(17, 1.0), (3, 0.0), (17, 2.0), (6, 0.0), (17, 3.0)]),
    ("M04", "brackets_change_the_root",
     [(18, 0.0), (17, 1.0), (3, 0.0), (17, 2.0), (19, 0.0), (6, 0.0), (17, 3.0)]),
    ("M05", "brackets_never_reach_the_output",
     [(18, 0.0), (17, 1.0), (3, 0.0), (17, 2.0), (19, 0.0), (6, 0.0), (17, 3.0)]),
    ("M06", "subtraction_is_left_associative",
     [(17, 1.0), (4, 0.0), (17, 2.0), (4, 0.0), (17, 3.0)]),
    ("M07", "pow_is_left_associative",
     [(17, 2.0), (8, 0.0), (17, 3.0), (8, 0.0), (17, 2.0)]),
    ("M08", "mixed_same_precedence_is_left_associative",
     [(17, 1.0), (7, 0.0), (17, 2.0), (6, 0.0), (17, 3.0)]),
    ("M09", "unary_function_wraps_everything_to_its_right",
     [(14, 0.0), (18, 0.0), (0, 0.0), (19, 0.0)]),
    ("M10", "unary_functions_are_right_associative", [(14, 0.0), (15, 0.0), (0, 0.0)]),
    ("M11", "tokenizer_never_produces_unary_minus", [(4, 0.0), (0, 0.0)]),
    ("M12", "unary_minus_token_if_present_binds_looser_than_pow",
     [(5, 0.0), (0, 0.0), (8, 0.0), (17, 2.0)]),
    ("M13", "missing_right_operand_of_plus_becomes_const_zero", [(17, 1.0), (3, 0.0)]),
    ("M14", "missing_left_operand_of_minus_becomes_const_zero", [(3, 0.0), (17, 1.0)]),
    ("M15", "only_plus_and_minus_fill_holes", [(6, 0.0), (17, 2.0)]),
    ("M16", "deep_nesting_still_reaches_atoms",
     [(18, 0.0), (18, 0.0), (0, 0.0), (19, 0.0), (19, 0.0)]),
]


def build_reorder_section(rows):
    def add(sec, key, value, src, note=""):
        rows.append((sec, key, str(value), src, note))

    sys.path.insert(0, TOOLS)
    sys.path.insert(0, os.path.join(ROOT, ".reference/analysis-repo/analysis"))
    from bitexact_run import Real  # noqa: E402

    SRC = "unicorn 直调真二进制：解析器 0x4ed670 / PolishNotationConverter::reorder_recursive 0x4f5450"
    R = Real()
    # ⚠️ 口径（本轮踩过两次，写在文件头也写在这里）：
    #  * 测试的输入是 `function_token::new` 出来的**中缀 token 序列** ⇒ 真值只能是
    #    「同一份 token 序列喂 `0x4f5450`」。
    #  * **不能**拿 `0x4ed670`（解析器）的输出当真值：它做的是「文本 → 分词 → reorder」，
    #    多了一趟分词（`-x` 在解析器路径上会被分词成 tag 5 `UnaryMinus`，而测试的输入是
    #    `ft::new("-")` = tag 4 `Sub`）——两份口径不同，混用会把「分词器的行为」当成
    #    「转换器的行为」记进表里。
    for cid, name, inp in REORDER_CASES:
        toks = list(inp)
        add("M/%s" % cid, "input",
            json.dumps([[t, "%016x" % bits(v)] for t, v in toks]), SRC,
            "中缀 token 序列（%s）" % name)
        out, info = R.pnc(list(toks))
        add("M/%s" % cid, "expected", fmt_tokens(out), SRC,
            "真 reorder 0x4f5450 给出的前缀 token（len=%d）" % info["len"])


# ---------------------------------------------------------------------------
# 段 P/（旧抓包）：VersionInfo
# ---------------------------------------------------------------------------
def build_fixture_section(rows):
    def add(sec, key, value, src, note=""):
        rows.append((sec, key, str(value), src, note))

    SRC = "真实抓包 fixtures/protocol_match.jsonl（tools/capture_net*.js）"
    p = os.path.join(ROOT, "fixtures/protocol_match.jsonl")
    for line in open(p, encoding="utf-8"):
        line = line.strip()
        if not line:
            continue
        try:
            m = json.loads(line)
        except ValueError:
            continue
        if "VersionInfo" in m:
            text = json.dumps(m, separators=(",", ":"), ensure_ascii=False)
            add("P/VersionInfo", "frames", 1, SRC, "旧抓包里的真帧（线上 1717 帧里没有它）")
            add("P/VersionInfo", "payload", text, SRC, "原文")
            add("P/VersionInfo", "sha256", hashlib.sha256(text.encode()).hexdigest(), SRC, "payload sha256")
            return
    sys.exit("fixtures/protocol_match.jsonl 里没有 VersionInfo")


# ---------------------------------------------------------------------------
def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", default=SNAP_DEFAULT)
    ap.add_argument("--out", default=OUT_DEFAULT)
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--print", dest="do_print", action="store_true")
    ap.add_argument("--skip-emu", action="store_true")
    args = ap.parse_args(argv)

    rows = []
    build_live_section(rows)
    c = Corpus(args.snapshot)
    build_corpus_section(c, rows)
    build_fixture_section(rows)
    if not args.skip_emu:
        build_reorder_section(rows)

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
