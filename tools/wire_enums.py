#!/usr/bin/env python3
"""线上字符串枚举的**完整取值集合**：serde `VARIANTS` 表 + 判别值顺序。

## 为什么这个工具能"证明全集"

Rust 的 `#[derive(Deserialize)]` 对**枚举**会生成：

```rust
deserializer.deserialize_enum("AxisMode", VARIANTS, __Visitor { .. })
```

其中 `VARIANTS: &'static [&'static str]` 是**声明顺序**的**全部**变体名。
`EnumAccess` 认不出来时走 `Error::unknown_variant(variant, VARIANTS)`，这条错误路径把
`VARIANTS` 的**长度**作为立即数写死在代码里：

```
lea  r8, [rip + VARIANTS]      ; 4c 8d 05 <disp32>
mov  ecx, N                    ; b9 <imm32>       <- N = 变体个数
...
call <serde::de::Error::unknown_variant>
```

所以「枚举名 → (数组地址, 个数 N)」是**代码自己说出来的**，不用猜；
而 `VARIANTS` 在 `.data.rel.ro` 里是 `N` 组 `(&'static str.ptr, len)`，
指针由 `.rela.dyn` 的 `R_X86_64_RELATIVE` 填（**文件里是 0**，所以直接读文件读出来是空）。

⇒ **全 `.text` 扫这个指令形状，找到的（数组, N）就是本二进制里 serde 枚举的闭集。**
本二进制恰好 8 个：`GraphwarEvent` + 7 个线上值枚举。找漏/找多都会在 `--verify` 里炸。

## 为什么不能用 `strings`

Rust 字面量**不以 NUL 结尾**，`strings` 会把相邻字面量粘成一条（`Timer1mTimer2mTimer3m…`）。
本工具全部按 `.rela.dyn` 给出的 `(ptr, len)` 精确读，边界由重定位自己给出。

## 校准点（方法有效性的判据）

`GraphwarGameState` 的真值在 `docs/07` §3.4 / `docs/14` §11 / `src/turnstate.rs` 已记为
`.data.rel.ro 0x9586d0` 那张 `[&str; 8]`，**字节长 10/5/19/18/20/16/10/12**。
本工具必须原样复现这 8 个名字与这 8 个长度，否则方法就是错的 —— `--verify` 把它当硬断言。

## 用法

    ../.venv/bin/python tools/wire_enums.py            # 人读
    ../.venv/bin/python tools/wire_enums.py --json     # 机器读
    ../.venv/bin/python tools/wire_enums.py --verify   # 自校验；不自洽退出码 1

需要上游目录里的 `libgraphwarrust.so`（x86-64 目标版）。用 `GW2_UPSTREAM=/path/to/gw2`
硬指，或从本脚本位置逐级上溯。可选 `--so PATH` 直接指定。

## 地址口径

本工具全部用 **ELF 虚地址**（= Ghidra/REA 地址 − `0x100000`）。
"""

import argparse
import json
import os
import re
import struct
import sys

# ---------------------------------------------------------------- ELF 极简解析


class Elf:
    """只读本工具要的三样东西：节头、`.rela.dyn` 的 RELATIVE 表、`.text` 原始字节。"""

    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            self.data = f.read()
        d = self.data
        if d[:4] != b"\x7fELF" or d[4] != 2 or d[5] != 1:
            raise SystemExit(f"{path}: 不是 ELF64 little-endian")
        (self.shoff,) = struct.unpack_from("<Q", d, 0x28)
        self.shentsize, self.shnum, self.shstrndx = struct.unpack_from("<HHH", d, 0x3A)
        self.sections = {}
        raw = []
        for i in range(self.shnum):
            o = self.shoff + i * self.shentsize
            name, typ, flags, addr, off, size = struct.unpack_from("<IIQQQQ", d, o)
            raw.append((name, typ, addr, off, size))
        strtab_off = raw[self.shstrndx][3]
        for name, typ, addr, off, size in raw:
            end = d.index(b"\0", strtab_off + name)
            self.sections[d[strtab_off + name:end].decode()] = (addr, off, size)
        self.rel = self._load_relative()

    def _load_relative(self):
        """r_offset -> r_addend，只要 R_X86_64_RELATIVE（= 8）。"""
        addr, off, size = self.sections[".rela.dyn"]
        out = {}
        for i in range(size // 24):
            r_off, r_info, r_add = struct.unpack_from("<QQq", self.data, off + i * 24)
            if (r_info & 0xFFFFFFFF) == 8:
                out[r_off] = r_add
        return out

    def va2off(self, va):
        for addr, off, size in self.sections.values():
            if addr and addr <= va < addr + size:
                return off + (va - addr)
        return None

    def read(self, va, n):
        o = self.va2off(va)
        return None if o is None else self.data[o:o + n]

    def in_section(self, name, va):
        addr, _, size = self.sections[name]
        return addr <= va < addr + size

    def str_at(self, va):
        """把一个 `&'static str` 槽读成 bytes：需要 RELATIVE 重定位 + 后随长度。"""
        if va not in self.rel:
            return None
        ptr = self.rel[va]
        o = self.va2off(va + 8)
        if o is None:
            return None
        (ln,) = struct.unpack_from("<Q", self.data, o)
        if not (0 < ln <= 120) or not self.in_section(".rodata", ptr):
            return None
        b = self.read(ptr, ln)
        if b is None or len(b) != ln or not all(32 <= c < 127 for c in b):
            return None
        return b


# ------------------------------------------------- 全 .text 扫 `unknown_variant` 形状

# lea r64, [rip + disp32] 的机器码：REX.W（0x48）或 REX.W|R（0x4c）+ 0x8d + modrm。
# modrm 的 mod=00、rm=101 就是 RIP 相对；低 3 位与 reg 无关，所以 (b & 0xc7) == 0x05。
_LEA_RE = re.compile(rb"[\x48\x4c]\x8d[\x05\x0d\x15\x1d\x25\x2d\x35\x3d]")


def _mov_imm32(buf, i):
    """`mov r32, imm32` → 立即数；否则 None。覆盖 B8..BF 与 REX.R 前缀形态。"""
    if i + 5 <= len(buf) and 0xB8 <= buf[i] <= 0xBF:
        return struct.unpack_from("<I", buf, i + 1)[0], 5
    if i + 6 <= len(buf) and buf[i] == 0x41 and 0xB8 <= buf[i + 1] <= 0xBF:
        return struct.unpack_from("<I", buf, i + 2)[0], 6
    return None


def find_variants_arrays(elf):
    """返回 [(数组虚地址, N, [名字...], 命中点虚地址)]。"""
    taddr, toff, tsize = elf.sections[".text"]
    text = elf.data[toff:toff + tsize]
    found = {}
    for m in _LEA_RE.finditer(text):
        o = m.start()
        if o + 7 > len(text):
            continue
        va = taddr + o
        tgt = va + 7 + struct.unpack_from("<i", text, o + 3)[0]
        if not elf.in_section(".data.rel.ro", tgt):
            continue
        mv = _mov_imm32(text, o + 7)
        if mv is None:
            continue
        n = mv[0]
        if not (2 <= n <= 200):
            continue
        names = []
        for k in range(n):
            s = elf.str_at(tgt + 16 * k)
            if s is None:
                break
            names.append(s.decode())
        if len(names) == n:
            found.setdefault((tgt, n), []).append(va)
    return [(t, n, [elf.str_at(t + 16 * k).decode() for k in range(n)], sites)
            for (t, n), sites in sorted(found.items())]


# --------------------------------------------------------------- 已知判别值（docs/07 §7.2）

# 「snake_case 名字 -> 判别值」72 条，来自 `0x3d38f0` 的分派表（docs/07 §7.2 全表）。
# 这里只用来**校准** VARIANTS 的位置：如果 VARIANTS 真是声明顺序且等于判别值，
# 这 72 条必须逐个落在同一格上。
KNOWN_TAGS = {
    "Tick": 0, "TickRequest": 1, "TickReply": 2, "Subscribe": 3,
    "NewConnectionRequest": 4, "NewConnection": 5, "ConnectionRemoved": 6,
    "ConnectedToServer": 7, "NewPlayerRequest": 8, "NewBotRequest": 9,
    "BotAdded": 10, "ChangeTeamRequest": 11, "RemovePlayerRequest": 12,
    "PlayerInfo": 13, "EntityRemoved": 14, "SoldierInfo": 15,
    "AddSoldierRequest": 16, "RemoveSoldierRequest": 17,
    "AxisModeChangeRequest": 18, "FunctionModeChangeRequest": 19,
    "TurnModeChangeRequest": 20, "TimeModeChangeRequest": 21, "GameNameRequest": 22,
    "TargetInfo": 23, "TeamInfo": 24, "RemoveTeam": 25, "PosInfo": 26, "LifeInfo": 27,
    "LifeQueuedDeath": 28, "GameNameInfo": 29, "GameStateInfo": 30,
    "GameStartRequest": 31, "FunctionFireRequest": 32, "FunctionFire": 33,
    "FunctionUpdateRequest": 34, "FunctionInfo": 35, "FunctionActive": 36,
    "FunctionPoints": 37, "FunctionsCalculated": 38, "ClearObstacles": 39,
    "AddObstacle": 40, "DestroyObstacle": 41, "QueueDestroyObstacle": 42, "TurnInfo": 43,
    "EffectInfo": 44, "AxisModeInfo": 45, "FunctionModeInfo": 46, "TurnModeInfo": 47,
    "TimeModeInfo": 48, "EndOfTurnInfo": 49, "SkinGraphicsRequest": 50,
    "SkinGraphicsInfo": 51, "FaceGraphicsRequest": 52, "FaceGraphicsInfo": 53,
    "HatGraphicsRequest": 54, "HatGraphicsInfo": 55, "EntityEffectInfo": 56,
    "TurnLimitInfo": 57, "TurnCountInfo": 58, "NameRequest": 59, "NameInfo": 60,
    "ColorInfo": 61, "GameInfo": 62, "GameRemoved": 64, "RankRequest": 66,
    "RankInfo": 67, "LockRequest": 68, "LockInfo": 69, "ChatMessageRequest": 70,
    "ChatMessage": 71, "BanConnectionRequest": 72, "ClearBansRequest": 73,
}

# 缺口 A：VARIANTS 里 63 / 65 这两格就是这两个事件的**声明位置**。
GAP_A_EXPECT = {63: "GameHeartbeat", 65: "VersionInfo"}

# 校准点：GraphwarGameState（docs/07 §3.4、docs/14 §11、src/turnstate.rs）。
GAMESTATE = ["Connecting", "Setup", "WaitingForFunctions", "CalculatingResults",
             "SynchronizingResults", "ExecutingResults", "EndingGame", "Disconnected"]
GAMESTATE_LENS = [10, 5, 19, 18, 20, 16, 10, 12]

# 缺口 B：7 个线上值枚举的**完整**取值集合（声明顺序 = 判别值）。
# 这些名字不是猜的：其一是二进制里的 VARIANTS 表逐字读出（本工具），
# 其二是 arm64 未 strip 版按符号锚定解出的 判别值→名字（见 docs/07 §5.2）。
VALUE_ENUMS = {
    "AxisMode": ["NoAxis", "OnlyMain", "EveryFive", "EveryUnit"],
    "FunctionMode": ["NormalFunction", "DiffEqFunction"],
    "TurnMode": ["SequentialTurns", "SimultaneousTurns"],
    "TimeMode": ["Timer30s", "Timer1m", "Timer2m", "Timer3m", "Timer5m", "TimerInf"],
    "GraphwarGameState": GAMESTATE,
    "GraphwarTeam": ["Team1", "Team2"],
    "EffectType": ["ExplosionEffect", "DeathEffect", "SparkEffect", "AshEffect"],
}

# 线上字段 → 枚举（`docs/07` §3.3/§3.4 的 `enum(str)` 列，逐个有着落）。
WIRE_FIELDS = {
    "axis_mode": "AxisMode",
    "function_mode": "FunctionMode",
    "turn_mode": "TurnMode",
    "time_mode": "TimeMode",
    "game_state": "GraphwarGameState",
    "team": "GraphwarTeam",
    "effect_type": "EffectType",
}

# 抓包里观测到的取值（观测档），用来交叉：**必须是**集合的子集。
OBSERVED = {
    "axis_mode": ["EveryUnit"],
    "function_mode": ["NormalFunction"],
    "turn_mode": ["SequentialTurns", "SimultaneousTurns"],
    "time_mode": ["Timer1m", "Timer3m"],
    "game_state": ["Setup", "ExecutingResults"],
    "effect_type": ["ExplosionEffect", "DeathEffect"],
    "team": [],
}


# ------------------------------------------------------------------------- 定位 so


def find_so(explicit, ghidra=False):
    """x86-64 目标版 libgraphwarrust.so。

    优先 `.reference/steam/*/libgraphwarrust.so`（本项目逆向目标），
    再退回上游根目录那份（`.venv` 的 difftest 用的就是它）。
    不按 Ghidra 地址口径做任何换算：本文件全用 ELF 虚地址。
    """
    if explicit:
        return explicit
    roots = []
    env = os.environ.get("GW2_UPSTREAM")
    if env:
        roots.append(env)
    here = os.path.dirname(os.path.abspath(__file__))
    p = here
    for _ in range(8):
        roots.append(p)
        p = os.path.dirname(p)
    for r in roots:
        # `.reference/steam/<date>/libgraphwarrust.so`（本项目钉死的逆向目标，日期倒序取最新）
        steams = os.path.join(r, ".reference", "steam")
        if os.path.isdir(steams):
            for name in sorted(os.listdir(steams), reverse=True):
                cand = os.path.join(steams, name, "libgraphwarrust.so")
                if os.path.isfile(cand):
                    return cand
        cand = os.path.join(r, "libgraphwarrust.so")
        if os.path.isfile(cand):
            return cand
    return None


# ------------------------------------------------------------------------- 输出


def collect(path):
    elf = Elf(path)
    arrays = find_variants_arrays(elf)
    import hashlib
    out = {"so": path, "sha256": hashlib.sha256(elf.data).hexdigest(),
           "size": len(elf.data), "arrays": []}
    for addr, n, names, sites in arrays:
        out["arrays"].append({"addr": addr, "n": n, "names": names,
                              "site": sites[0], "refs": len(sites)})
    # 把每个 array 认到枚举头上：7 个值枚举 + GraphwarEvent（唯一 74 元的）。
    for a in out["arrays"]:
        a["enum"] = None
        for name, want in VALUE_ENUMS.items():
            if a["names"] == want:
                a["enum"] = name
        if a["n"] == len(KNOWN_TAGS) + 2 and a["names"][0] == "Tick":
            a["enum"] = "GraphwarEvent"
        a["lens"] = [len(x.encode()) for x in a["names"]]
    return out


def report(res):
    print(f"so: {res['so']}  (sha256 {res['sha256'][:16]}..., {res['size']} B)")
    print(f"serde 枚举 VARIANTS 表：{len(res['arrays'])} 个\n")
    w = max(len(a["enum"] or "?") for a in res["arrays"])
    for a in sorted(res["arrays"], key=lambda a: -a["n"]):
        print(f"  {a['enum'] or '?':<{w}}  N={a['n']:<3} @0x{a['addr']:08x} "
              f"(unknown_variant 在 0x{a['site']:08x}, {a['refs']} 处引用)")
        if a["n"] <= 8:
            print("      " + "  ".join(f"{i}={x}" for i, x in enumerate(a["names"])))


# ------------------------------------------------------------------------- 自校验


def verify(res):
    fails, notes = [], []
    arrs = res["arrays"]

    # ① 枚举闭集：形状扫描找到的必须恰好 8 个（7 值枚举 + GraphwarEvent）。
    if len(arrs) != 8:
        fails.append(f"①期望 8 个 VARIANTS 表，实际 {len(arrs)}")
    else:
        notes.append("① unknown_variant 形状全 .text 扫描 → 恰好 8 个 VARIANTS 表（闭集）")

    by_enum = {a["enum"]: a for a in arrs if a["enum"]}
    # ② 认不出来 / 认重了都要炸。
    for name in list(VALUE_ENUMS) + ["GraphwarEvent"]:
        got = [a for a in arrs if a["enum"] == name]
        if len(got) != 1:
            fails.append(f"②{name}: 期望恰好 1 张表，实际 {len(got)}")

    # ③ 校准点：GraphwarGameState 的 8 个名字 + 8 个长度。
    gs = by_enum.get("GraphwarGameState")
    if gs is None:
        fails.append("③找不到 GraphwarGameState 的 VARIANTS 表（校准点无从核对）")
    else:
        if gs["names"] != GAMESTATE:
            fails.append(f"③GameState 名字不符：{gs['names']}")
        if gs["lens"] != GAMESTATE_LENS:
            fails.append(f"③GameState 长度不符：{gs['lens']} != {GAMESTATE_LENS}")
        if gs["addr"] != 0x9586D0:
            notes.append(f"③GameState 表在 0x{gs['addr']:08x}（docs 记的是 0x9586d0）")
        if gs["names"] == GAMESTATE and gs["lens"] == GAMESTATE_LENS:
            notes.append("③校准点通过：GameState 8 值 = Connecting/Setup/WaitingForFunctions/"
                         "CalculatingResults/SynchronizingResults/ExecutingResults/EndingGame/"
                         "Disconnected，长度 10/5/19/18/20/16/10/12")

    # ④ 缺口 A：74 元表里 63/65 两格；且 72 条已知判别值逐个落在同名格上。
    ev = by_enum.get("GraphwarEvent")
    if ev is None:
        fails.append("④找不到 74 元的 GraphwarEvent VARIANTS 表")
    else:
        if ev["n"] != 74:
            fails.append(f"④GraphwarEvent N={ev['n']} != 74")
        for idx, want in GAP_A_EXPECT.items():
            got = ev["names"][idx] if idx < ev["n"] else None
            if got != want:
                fails.append(f"④VARIANTS[{idx}] = {got!r} != {want!r}")
        bad = [f"{nm}: 期望 {t} 实际 {ev['names'].index(nm) if nm in ev['names'] else '缺'}"
               for nm, t in KNOWN_TAGS.items()
               if nm not in ev["names"] or ev["names"].index(nm) != t]
        if bad:
            fails.append("④已知判别值与 VARIANTS 位置不符：" + "; ".join(bad))
        else:
            notes.append(f"④GraphwarEvent：VARIANTS 位置 == 判别值 {len(KNOWN_TAGS)}/{len(KNOWN_TAGS)} 全中；"
                         f"[63]={ev['names'][63]} [65]={ev['names'][65]}")
        dup = [n for n in VALUE_ENUMS if n in ev["names"]]
        if dup:
            fails.append(f"④事件名与枚举名冲突：{dup}")

    # ⑤ 缺口 B：7 个值枚举的集合大小 + 全集唯一性（同名集合不得在别的表里重复出现）。
    sizes = {k: len(v) for k, v in VALUE_ENUMS.items()}
    for name, want in VALUE_ENUMS.items():
        a = by_enum.get(name)
        if a is None:
            fails.append(f"⑤缺 {name}")
            continue
        if a["names"] != want:
            fails.append(f"⑤{name} 取值不符：{a['names']} != {want}")
        # 顺序=判别值：名字不得重复
        if len(set(a["names"])) != len(a["names"]):
            fails.append(f"⑤{name} 有重复变体名")
        # 不能同时是别的枚举
        others = [o for o, w in VALUE_ENUMS.items() if o != name and w == a["names"]]
        if others:
            fails.append(f"⑤{name} 的表与 {others} 同形，无法区分")
    notes.append("⑤7 个值枚举全集：" + ", ".join(f"{k}({v})" for k, v in sizes.items()))

    # ⑥ 线上字段 → 枚举，全部有着落；抓包观测值必须是成员。
    for field, enm in WIRE_FIELDS.items():
        if enm not in VALUE_ENUMS:
            fails.append(f"⑥字段 {field} 指到未知枚举 {enm}")
        for v in OBSERVED.get(field, []):
            if v not in VALUE_ENUMS[enm]:
                fails.append(f"⑥抓包观测值 {field}={v} 不在 {enm} 的集合里")
    notes.append("⑥线上 enum(str) 字段 → 枚举 全部有着落；抓包观测值全部落在集合内")

    return fails, notes


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--so", help="x86-64 目标版 libgraphwarrust.so")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--verify", action="store_true",
                    help="只跑自校验；不自洽则退出码 1")
    args = ap.parse_args()

    so = find_so(args.so)
    if so is None:
        raise SystemExit("找不到 libgraphwarrust.so；用 --so 指定，或设 GW2_UPSTREAM")
    res = collect(so)

    if args.json:
        print(json.dumps(res, ensure_ascii=False, indent=2))
        return 0

    if not args.verify:
        report(res)
        print("\n（--verify 跑自校验）")
        return 0

    fails, notes = verify(res)
    for n in notes:
        print("  OK   " + n)
    for f in fails:
        print("  FAIL " + f)
    print()
    if fails:
        print(f"自校验失败：{len(fails)} 项")
        return 1
    print(f"自校验通过：{len(notes)} 项")
    return 0


if __name__ == "__main__":
    sys.exit(main())
