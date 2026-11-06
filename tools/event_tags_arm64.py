#!/usr/bin/env python3
"""`enum GraphwarEvent` 的**全部 74 个判别值**（arm64 未 strip 版，符号锚定）。

## 为什么需要它

x86 目标版是 **stripped** 的，而 `docs/07` §7 里那张 72 条「名字 → 判别值」全表
是从 **名字分派函数** `0x3d38f0` 反推的 —— 那条路径拿不到 `VersionInfo` /
`GameHeartbeat`，因为它们在 `0x3d38f0` 里**没有分支**（缺口 A）。

Android 版 `libgraphwarrust.so` **没 strip**，`.symtab` 里有 Rust 的完整符号名，
其中就有：

```
_ZN12graphwarrust14graphwar_event1_95_$LT$impl$u20$serde..ser..Serialize$u20$for$u20$
   graphwarrust..graphwar_event..GraphwarEvent$GT$9serialize17h...E.llvm...
```

`#[derive(Serialize)]` 对**外部标签枚举**生成的实现就是一张**按判别值索引的跳转表**：

```
ldr   x10, [x0]              ; Event 的 tag word
mov   w9,  #0x3e             ; tag>=0 时的默认下标
eor   x11, x10, #0x8000000000000000
cmp   x10, #0
csel  x9,  x11, x9, mi       ; tag<0 ⇒ x9 = tag ^ 0x8000… = 判别值
adrp  x10, <表页>
add   x10, x10, #<偏移>       ; 跳转表（u16，单位 = 4 字节）
adr   x11, <臂区基址>
ldrh  w12, [x10, x9, lsl #1]
add   x11, x11, x12, lsl #2
br    x11
```

⇒ **表的下标就是判别值**，每个臂就是「序列化该变体」的调用。把每个臂（或其直接被调的
outlined 函数）里写出的 `(名字指针, 名字长度)` 读出来，就是 `判别值 → 变体名`。

这条路径**完全不经过名字分派表**，因此与 `docs/07` §7 的 72 条是**独立证据**；
两者在交集上必须一致 —— `--verify` 把这一点当硬断言。

## 只需要四个 AArch64 编码

本工具**不依赖 capstone**（stdlib only），只手解 ADR / ADRP / ADD-imm / ADD-reg /
LDRH-reg / BR / MOVZ / BL / B 九种指令；遇到不认识的指令就跳过。解码形状不符合预期时
直接报错退出，不会"猜一个结果"。

## 用法

    ../.venv/bin/python tools/event_tags_arm64.py            # 74 条全表
    ../.venv/bin/python tools/event_tags_arm64.py --verify   # 自校验；失败退出码 1
    ../.venv/bin/python tools/event_tags_arm64.py --json
    ../.venv/bin/python tools/event_tags_arm64.py --so PATH  # 指定 so

arm64 so 的默认位置：`.rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so`
（从 `Graphwar II_1.0.apks` 里解 `split_config.arm64_v8a.apk` 得到）。
不在就设 `GW2_ARM64_SO`，或用 `--so`。
"""

import argparse
import json
import os
import struct
import sys

EVENT = "GraphwarEvent"


# --------------------------------------------------------------------- ELF 极简解析


class Elf:
    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            self.data = f.read()
        d = self.data
        if d[:4] != b"\x7fELF" or d[4] != 2 or d[5] != 1:
            raise SystemExit(f"{path}: 不是 ELF64 little-endian")
        if struct.unpack_from("<H", d, 0x12)[0] != 183:
            raise SystemExit(f"{path}: 不是 AArch64（e_machine != 183）")
        (self.shoff,) = struct.unpack_from("<Q", d, 0x28)
        self.shentsize, self.shnum, self.shstrndx = struct.unpack_from("<HHH", d, 0x3A)
        hdrs = []
        for i in range(self.shnum):
            o = self.shoff + i * self.shentsize
            name, typ, _flags, addr, off, size, link = struct.unpack_from("<IIQQQQI", d, o)
            hdrs.append((name, typ, addr, off, size, link))
        shstr = hdrs[self.shstrndx][3]
        self.sections = {}
        for name, typ, addr, off, size, link in hdrs:
            end = d.index(b"\0", shstr + name)
            self.sections[d[shstr + name:end].decode()] = {
                "addr": addr, "off": off, "size": size, "link": link}

    def section_bytes(self, name):
        s = self.sections[name]
        return self.data[s["off"]:s["off"] + s["size"]]

    def read(self, va, n):
        for s in self.sections.values():
            if s["addr"] and s["addr"] <= va < s["addr"] + s["size"]:
                o = s["off"] + (va - s["addr"])
                return self.data[o:o + n]
        return None

    def symbols(self):
        """.symtab 的 (name, value, size)，只读这一个符号表即可。"""
        sym = self.sections[".symtab"]
        strtab = list(self.sections.values())[0]  # 占位，下面按 link 取
        # .symtab 的 sh_link 指向它的字符串表
        for name, s in self.sections.items():
            if name == ".symtab":
                strtab = self.sections[".dynstr" if s["link"] == 0 else ".strtab"]
                if s["link"]:
                    # 直接用 link 索引不可靠（dict 无序），回退到按名字找
                    strtab = self.sections.get(".strtab", strtab)
        ent = sym["size"] // 24
        out = []
        for i in range(ent):
            o = sym["off"] + i * 24
            st_name, st_info, _other, st_shndx, st_value, st_size = \
                struct.unpack_from("<IBBHQQ", self.data, o)
            if not st_name:
                continue
            base = strtab["off"]
            end = self.data.index(b"\0", base + st_name)
            out.append((self.data[base + st_name:end].decode("utf-8", "replace"),
                        st_value, st_size))
        return out


# ------------------------------------------------------- 极窄的 AArch64 解码（stdlib only）


def _u32(buf, i):
    return struct.unpack_from("<I", buf, i)[0]


def _rd(w):
    return w & 0x1F


def _rn(w):
    return (w >> 5) & 0x1F


def _sext(v, bits):
    return v - (1 << bits) if v & (1 << (bits - 1)) else v


def dec_adr(w, pc):
    """ADR：0x10000000 / 掩码 0x9F000000。"""
    if (w & 0x9F000000) != 0x10000000:
        return None
    imm = ((w >> 5) & 0x7FFFF) << 2 | ((w >> 29) & 3)
    return _rd(w), pc + _sext(imm, 21)


def dec_adrp(w, pc):
    """ADRP：0x90000000 / 掩码 0x9F000000。"""
    if (w & 0x9F000000) != 0x90000000:
        return None
    imm = ((w >> 5) & 0x7FFFF) << 2 | ((w >> 29) & 3)
    return _rd(w), (pc & ~0xFFF) + (_sext(imm, 21) << 12)


def dec_add_imm(w):
    """ADD Xd, Xn, #imm{, lsl #12}：0x91000000 / 掩码 0xFF800000。"""
    if (w & 0xFF800000) != 0x91000000:
        return None
    imm = (w >> 10) & 0xFFF
    if (w >> 22) & 1:
        imm <<= 12
    return _rd(w), _rn(w), imm


def dec_add_reg(w):
    """ADD Xd, Xn, Xm{, lsl #N}：0x8B000000 / 掩码 0xFF200000。返回 (Rd, Rn, Rm, shift)。"""
    if (w & 0xFF200000) != 0x8B000000:
        return None
    return _rd(w), _rn(w), (w >> 16) & 0x1F, (w >> 10) & 0x3F


def dec_ldrh_reg(w):
    """LDRH Wt, [Xn, Xm, lsl #1]：掩码 0xFFE00C00 == 0x78600800。"""
    if (w & 0xFFE00C00) != 0x78600800:
        return None
    return _rd(w), _rn(w), (w >> 16) & 0x1F, (w >> 12) & 1


def dec_br(w):
    return _rn(w) if (w & 0xFFFFFC1F) == 0xD61F0000 else None


def dec_movz(w):
    """MOVZ Wd/Xd, #imm16。返回 (Rd, imm16, is64)。"""
    if (w & 0x7F800000) == 0x52800000:
        return _rd(w), (w >> 5) & 0xFFFF, False
    if (w & 0xFF800000) == 0xD2800000:
        return _rd(w), (w >> 5) & 0xFFFF, True
    return None


def dec_branch(w, pc):
    """B / BL。返回 (is_link, target)。"""
    op = w & 0xFC000000
    if op == 0x94000000 or op == 0x14000000:
        return op == 0x94000000, pc + _sext(w & 0x3FFFFFF, 26) * 4
    return None


def printable_name(elf, va, ln):
    if not (3 <= ln <= 40):
        return None
    b = elf.read(va, ln)
    if b is None or len(b) != ln or not all(32 <= c < 127 for c in b):
        return None
    s = b.decode()
    return s if s[0].isupper() and all(c.isalnum() for c in s) else None


def find_name_literal(elf, text_addr, start, budget=0x60, depth=2):
    """在一个 `序列化该变体` 的臂里找它写出的 (名字指针, 长度)。

    形状：`adrp Xd,页` + `add Xd,Xd,#imm` 给出指针，前后 ±4 条内 `mov w3,#len` 给长度。
    臂常常只是一句 `bl <outlined>`，名字在 outlined 函数里 —— 所以跟一层 `bl`/`b`。
    """
    seen = set()
    todo = [(start, budget, depth)]
    while todo:
        pc, n, d = todo.pop(0)
        if pc in seen or d < 0:
            continue
        seen.add(pc)
        off = start - text_addr  # 只用于越界保护；read 会自己判界
        body = elf.read(pc, n)
        if body is None:
            continue
        for i in range(0, len(body) - 3, 4):
            w = _u32(body, i)
            a = dec_adrp(w, pc + i)
            if not a:
                continue
            # 只接受「紧跟着 add 同一个寄存器」的形态
            if i + 8 > len(body) - 3:
                continue
            add = dec_add_imm(_u32(body, i + 4))
            if not add or add[0] != a[0] or add[1] != a[0]:
                continue
            ptr = a[1] + add[2]
            for j in range(max(0, i - 16), min(len(body) - 3, i + 24), 4):
                mv = dec_movz(_u32(body, j))
                if mv and mv[0] == 3:  # `mov w3, #len`
                    s = printable_name(elf, ptr, mv[1])
                    if s:
                        return s
        # 没有再往被调函数里看一层
        for i in range(0, len(body) - 3, 4):
            b = dec_branch(_u32(body, i), pc + i)
            if b:
                todo.append((b[1], 0x60, d - 1))
                break
    return None


def decode_event_enum(path):
    elf = Elf(path)
    taddr = elf.sections[".text"]["addr"]

    # ① 找 `impl serde::ser::Serialize for GraphwarEvent`。
    #    注意 `serde..de..Deserialize` 里也含子串 "Serialize"，所以必须按整条路径过滤，
    #    否则会挑到同名的 Deserialize 实现上去（踩过）。
    ser = [(v, s, nm) for nm, v, s in elf.symbols()
           if "serde..ser..Serialize" in nm
           and "graphwar_event" in nm and EVENT in nm]
    if not ser:
        raise SystemExit("符号表里找不到 impl Serialize for GraphwarEvent（so 是不是 strip 过？）")
    va, size, sym = max(ser, key=lambda c: c[1])
    body = elf.read(va, size)
    if body is None:
        raise SystemExit(f"读不到 0x{va:x} 的函数体")

    # ② 解跳转表的三件套：表基址（adrp+add）、臂区基址（adr）、以及 ldrh/br 的存在。
    table = arms = None
    for i in range(0, len(body) - 3, 4):
        w = _u32(body, i)
        pc = va + i
        if dec_ldrh_reg(w) and table is None:
            for j in range(i - 4, max(-1, i - 40), -4):
                a = dec_adrp(_u32(body, j), va + j)
                if a:
                    add = dec_add_imm(_u32(body, j + 4))
                    if add and add[0] == a[0] and add[1] == a[0]:
                        table = a[1] + add[2]
                        break
        if dec_ldrh_reg(w):
            for j in range(i - 4, max(-1, i - 40), -4):
                d = dec_adr(_u32(body, j), va + j)
                if d:
                    arms = d[1]
                    break
            break
    if table is None or arms is None:
        raise SystemExit(f"0x{va:x} 里没找到『跳转表 + br』的形状，拒绝猜")
    if dec_br(_u32(body, len(body) - 4)) is None and not any(
            dec_br(_u32(body, i)) for i in range(0, len(body) - 3, 4)):
        raise SystemExit("跳转表后面没有 `br`，形状不对")

    # ③ 逐格解：下标 = 判别值；遇到重名即终止（表尾填充项会绕回第 0 臂）。
    out, seen = [], set()
    raw = elf.read(table, 256 * 2)
    for k in range(128):
        (off,) = struct.unpack_from("<H", raw, k * 2)
        arm = arms + off * 4
        name = find_name_literal(elf, taddr, arm)
        if name is None:
            raise SystemExit(f"判别值 {k} 的臂 0x{arm:x} 解不出变体名")
        if name in seen:
            break
        seen.add(name)
        out.append({"tag": k, "name": name, "arm": arm, "table_off": off})
    import hashlib
    return {"so": path, "sha256": hashlib.sha256(elf.data).hexdigest(),
            "size": len(elf.data), "serialize": va, "table": table, "arms_base": arms,
            "count": len(out), "variants": out}


# ------------------------------------------------------------------ 已知 72 条（校准）


def camel(snake):
    """`name_info_event` -> `NameInfo`。"""
    if snake.endswith("_event"):
        snake = snake[:-6]
    return "".join(p.capitalize() for p in snake.split("_"))


# docs/07 §7.2 的 72 条（snake_case → 判别值）。
KNOWN_TAGS_SNAKE = {
    "tick_event": 0, "tick_request_event": 1, "tick_reply_event": 2, "subscribe_event": 3,
    "new_connection_request_event": 4, "new_connection_event": 5,
    "connection_removed_event": 6, "connected_to_server_event": 7,
    "new_player_request_event": 8, "new_bot_request_event": 9, "bot_added_event": 10,
    "change_team_request_event": 11, "remove_player_request_event": 12,
    "player_info_event": 13, "entity_removed_event": 14, "soldier_info_event": 15,
    "add_soldier_request_event": 16, "remove_soldier_request_event": 17,
    "axis_mode_change_request_event": 18, "function_mode_change_request_event": 19,
    "turn_mode_change_request_event": 20, "time_mode_change_request_event": 21,
    "game_name_request_event": 22, "target_info_event": 23, "team_info_event": 24,
    "remove_team_event": 25, "pos_info_event": 26, "life_info_event": 27,
    "life_queued_death_event": 28, "game_name_info_event": 29,
    "game_state_info_event": 30, "game_start_request_event": 31,
    "function_fire_request_event": 32, "function_fire_event": 33,
    "function_update_request_event": 34, "function_info_event": 35,
    "function_active_event": 36, "function_points_event": 37,
    "functions_calculated_event": 38, "clear_obstacles_event": 39,
    "add_obstacle_event": 40, "destroy_obstacle_event": 41,
    "queue_destroy_obstacle_event": 42, "turn_info_event": 43, "effect_info_event": 44,
    "axis_mode_info_event": 45, "function_mode_info_event": 46,
    "turn_mode_info_event": 47, "time_mode_info_event": 48, "end_of_turn_info_event": 49,
    "skin_graphics_request_event": 50, "skin_graphics_info_event": 51,
    "face_graphics_request_event": 52, "face_graphics_info_event": 53,
    "hat_graphics_request_event": 54, "hat_graphics_info_event": 55,
    "entity_effect_info_event": 56, "turn_limit_info_event": 57,
    "turn_count_info_event": 58, "name_request_event": 59, "name_info_event": 60,
    "color_info_event": 61, "game_info_event": 62, "game_removed_event": 64,
    "rank_request_event": 66, "rank_info_event": 67, "lock_request_event": 68,
    "lock_info_event": 69, "chat_message_request_event": 70, "chat_message_event": 71,
    "ban_connection_request_event": 72, "clear_bans_request_event": 73,
}

# 缺口 A 的答案（本工具要独立地再推一遍）。
EXPECT = {63: "GameHeartbeat", 65: "VersionInfo"}


def find_so(explicit):
    if explicit:
        return explicit
    env = os.environ.get("GW2_ARM64_SO")
    if env and os.path.isfile(env):
        return env
    here = os.path.dirname(os.path.abspath(__file__))
    p = here
    for _ in range(8):
        for rel in (".rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so",):
            cand = os.path.join(p, rel)
            if os.path.isfile(cand):
                return cand
        p = os.path.dirname(p)
    return None


def verify(res):
    fails, notes = [], []
    var = res["variants"]
    names = [v["name"] for v in var]

    # ① 74 个、且互不重复（重复就是解错了或读过了表尾）。
    if res["count"] != 74:
        fails.append(f"①期望 74 个变体，实际 {res['count']}")
    else:
        notes.append("①判别值 0..=73 逐格解出，共 74 个变体（表尾填充项绕回第 0 臂，据此判定边界）")
    if len(set(names)) != len(names):
        fails.append("①变体名有重复")

    # ② 缺口 A 的两格。
    for k, want in EXPECT.items():
        got = var[k]["name"] if k < len(var) else None
        if got != want:
            fails.append(f"②判别值 {k} = {got!r} != {want!r}")
    if all(var[k]["name"] == w for k, w in EXPECT.items() if k < len(var)):
        notes.append(f"②缺口 A：63 = {var[63]['name']}，65 = {var[65]['name']}（"
                     f"臂 0x{var[63]['arm']:x} / 0x{var[65]['arm']:x}）")

    # ③ 与 docs/07 §7 的 72 条独立证据对账（snake_case → CamelCase）。
    bad = []
    for snake, tag in KNOWN_TAGS_SNAKE.items():
        want = camel(snake)
        got = var[tag]["name"] if tag < len(var) else None
        if got != want:
            bad.append(f"{snake}(={want}) 期望 {tag} 实际 {got}@{tag}")
    if bad:
        fails.append("③与名字分派表的 72 条不符：" + "; ".join(bad[:6]))
    else:
        notes.append(f"③与 `0x3d38f0` 名字分派表的 {len(KNOWN_TAGS_SNAKE)} 条**逐条相同**"
                     "（两条独立证据）")

    # ④ 缺号补全：0..=73 里此前缺的就是 63/65，没有别的洞。
    holes = [k for k in range(74) if not var[k]["name"]] if len(var) >= 74 else [73]
    if holes and len(var) >= 74:
        fails.append(f"④仍有空洞：{holes}")
    elif len(var) >= 74:
        notes.append("④0..=73 全部具名，`src/events.rs` 的 63/65 缺号自此闭合")
    return fails, notes


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--so", help="arm64 未 strip 版 libgraphwarrust.so")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--verify", action="store_true")
    args = ap.parse_args()

    so = find_so(args.so)
    if so is None:
        raise SystemExit("找不到 arm64 版 libgraphwarrust.so；用 --so 指定，或设 GW2_ARM64_SO")
    res = decode_event_enum(so)

    if args.json:
        print(json.dumps(res, ensure_ascii=False, indent=2))
        return 0

    fails, notes = verify(res)
    if not args.verify:
        print(f"so: {res['so']}  (sha256 {res['sha256'][:16]}..., {res['size']} B)")
        print(f"GraphwarEvent::serialize @0x{res['serialize']:x}  "
              f"跳转表 @0x{res['table']:x}  臂区基址 @0x{res['arms_base']:x}")
        for v in res["variants"]:
            print(f"  {v['tag']:2d}  {v['name']}")
        print()
    for n in notes:
        print("  OK   " + n)
    for f in fails:
        print("  FAIL " + f)
    if args.verify:
        print()
        if fails:
            print(f"自校验失败：{len(fails)} 项")
            return 1
        print(f"自校验通过：{len(notes)} 项")
    return 0


if __name__ == "__main__":
    sys.exit(main())
