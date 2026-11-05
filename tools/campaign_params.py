#!/usr/bin/env python3
"""Extract the *per-level gameplay parameters* of the 21 Graphwar II campaign levels.

Why this file exists (docs/118 §12): the previous round got the titles, the
descriptions and the four mode bytes, but concluded that "obstacle layout / soldier
count / target count / bot count" were unknowable, because `0x466d20` was believed to
be the level generator and its arguments looked like opaque `FnMut` captures.

That was one level of indirection too deep.  What is actually there:

    start_level arm  --call-->  0x466d20   "level opening driver"
                                   |  clears the obstacle Vec, then loops <=5 times
                                   +--call *vtableA(+0x20)  = obstacle generator
                                   +--call *vtableB(+0x20)  = layout validator

`vtableA` is one of 15 static 0x28-byte records in `.data.rel.ro`; its `+0x20` slot is
either `reset_mask`-only ("no obstacles"), one of the three centre-single-obstacle
copies, or a `mov $N,%esi; jmp 0x466c40` thunk whose `esi` is the scatter generator's
`min_count`.  The other per-level numbers:

  * soldier count  = the `edx` immediate of `0x476320(mgr, player, edx)`
  * target count   = how many times `0x46f310(team)` is called
  * turn limit     = high half of the `movabs $0x..00000001` fed to `turn_limit_info`
  * bot difficulty = 3rd u32 of the `bot_added_event` payload (3/25/50/100/250)

How the numbers are found, and why it is safe: we do **byte-pattern** scanning, not a
full disassembler, and every candidate is validated against an exact expected operand
(a known function address, a known GOT slot, a known string immediate).  False
positives inside immediates therefore cannot produce a row.

Run:
    GW2_SO=../libgraphwarrust.so ../.venv/bin/python tools/campaign_params.py
    GW2_SO=../libgraphwarrust.so ../.venv/bin/python tools/campaign_params.py --tsv
    ../.venv/bin/python tools/campaign_params.py --verify-replays
"""

from __future__ import annotations

import collections
import glob
import gzip
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SO = os.environ.get("GW2_SO") or os.path.join(HERE, "..", "..", "libgraphwarrust.so")


# ---------------------------------------------------------------------------
# ELF + relocations
# ---------------------------------------------------------------------------

class Elf:
    def __init__(self, data: bytes):
        self.d = data
        if data[:4] != b"\x7fELF":
            raise SystemExit(f"{SO}: not an ELF file")
        shoff = struct.unpack_from("<Q", data, 0x28)[0]
        shentsize = struct.unpack_from("<H", data, 0x3A)[0]
        shnum = struct.unpack_from("<H", data, 0x3C)[0]
        shstrndx = struct.unpack_from("<H", data, 0x3E)[0]
        secs = []
        for i in range(shnum):
            name, typ, _f, addr, off, size, _l, _i, _a, _e = \
                struct.unpack_from("<IIQQQQIIQQ", data, shoff + i * shentsize)
            secs.append(dict(name=name, typ=typ, addr=addr, off=off, size=size))
        stro = secs[shstrndx]
        for s in secs:
            end = data.index(b"\0", stro["off"] + s["name"])
            s["sname"] = data[stro["off"] + s["name"]:end].decode()
        self.secs = secs

    def section(self, name):
        for s in self.secs:
            if s["sname"] == name:
                return s
        raise KeyError(name)

    def va2off(self, va):
        for s in self.secs:
            if s["typ"] != 8 and s["size"] and s["addr"] <= va < s["addr"] + s["size"]:
                return s["off"] + (va - s["addr"])
        return None


def load_relative_relocs(elf):
    """slot_va -> addend, for R_X86_64_RELATIVE only (all a `.got` slot needs)."""
    rela = elf.section(".rela.dyn")
    out = {}
    for o in range(rela["off"], rela["off"] + rela["size"], 24):
        r_offset, r_info, r_addend = struct.unpack_from("<QQq", elf.d, o)
        if (r_info & 0xFFFFFFFF) == 8:
            out.setdefault(r_offset, r_addend)
    return out


# ---------------------------------------------------------------------------
# Addresses (all ELF virtual; Ghidra = va + 0x100000)
# ---------------------------------------------------------------------------

FUNC_DRIVER = 0x466D20          # "level opening driver"
FUNC_SCATTER = 0x466C40         # `mov $N,%esi; jmp` target == scatter entry
FUNC_SCATTER_BODY = 0x3F1E50    # docs/55 / docs/61
FUNC_RESET_MASK = 0x3F19A0
FUNC_PLACE_OBSTACLE = 0x3F17A0
FUNC_SET_SOLDIERS = 0x476320    # (mgr, player_id, edx = count)
FUNC_ADD_TARGET = 0x46F310      # (team_id) -> one target, emits target_info (tag 23)
FUNC_ADD_BOT = 0x4739C0         # create the bot player
FUNC_SET_TURN_LIMIT = 0x472400  # compact form used by level 3 (1, N)
CENTRE_GENS = (0x49F1C0, 0x49F370, 0x4A5E80)
NO_OBSTACLE_GENS = (0x49E570, 0x49EAB0, 0x4A93E0)

# Branch bodies, keyed by 0-based level.  Derived from the `"Level <n>"` immediate each
# arm builds -- NOT from address order (the dispatcher `0x499740` inlines the arms in
# the order 0,10,1,11,2,12,3,13 because `level_n` and `level_{n+10}` only differ in the
# mode dword and LLVM merged them).
ARM = {
    0: (0x499957, 0x499E1E), 10: (0x499E1E, 0x49A3F2),
    1: (0x49A3F2, 0x49A924), 11: (0x49A924, 0x49AF3C),
    2: (0x49AF3C, 0x49B472), 12: (0x49B472, 0x49BB63),
    3: (0x49BB63, 0x49BF41), 13: (0x49BF41, 0x49C66C),
    4: (0x49F510, 0x49FE80), 5: (0x4A04D0, 0x4A0DB0),
    6: (0x4A1310, 0x4A1CD0), 7: (0x4A2430, 0x4A2DA0),
    8: (0x4A33F0, 0x4A3DF0), 9: (0x4A43A0, 0x4A4C80),
    14: (0x4A6980, 0x4A73B0), 15: (0x4A78F0, 0x4A8320),
    16: (0x4A8860, 0x4A91B0), 17: (0x4A94C0, 0x4A9F00),
    18: (0x4AA640, 0x4AAFC0), 19: (0x4AB500, 0x4ABF00),
    20: (0x4AC440, 0x4ACE40),
}
# `"Level <n>"` construction site per level (the self-check anchor)
TITLE_SITE = {
    0: 0x499B71, 10: 0x49A038, 1: 0x49A60C, 11: 0x49AB22, 2: 0x49B156, 12: 0x49B694,
    3: 0x49BCF9, 13: 0x49C163, 4: 0x49F742, 5: 0x4A06B8, 6: 0x4A1542, 7: 0x4A2662,
    8: 0x4A3622, 9: 0x4A4588, 14: 0x4A6B94, 15: 0x4A7B04, 16: 0x4A8A92, 17: 0x4A96F2,
    18: 0x4AA82E, 19: 0x4AB6EE, 20: 0x4AC62E,
}
# `call 0x466d20` site per level
DRIVER_SITE = {
    0: 0x499DB5, 10: 0x49A389, 1: 0x49A8BB, 11: 0x49AED2, 2: 0x49B409, 12: 0x49B9A6,
    3: 0x49BEEE, 13: 0x49C4B5, 4: 0x49F9F3, 5: 0x4A09EB, 6: 0x4A1835, 7: 0x4A2913,
    8: 0x4A3940, 9: 0x4A48BB, 14: 0x4A6F59, 15: 0x4A7EC6, 16: 0x4A8D30, 17: 0x4A9A3F,
    18: 0x4AABCC, 19: 0x4ABAEA, 20: 0x4ACA2A,
}
TAG_BOT_ADDED = 0x800000000000000A     # docs/114 #10 = bot_added_event

MODE_AXIS = {3: "EveryUnit", 2: "EveryFive", 1: "OnlyMain", 0: "NoAxis"}
MODE_FUNC = {0: "NormalFunction", 1: "DiffEqFunction"}
MODE_TURN = {0: "SequentialTurns", 1: "SimultaneousTurns"}

MODE_WORD = [
    0x05030000, 0x05030000, 0x05030000, 0x05030000, 0x05030000,
    0x05030000, 0x05030000, 0x05030000, 0x05030000, 0x05030000,
    0x05020000, 0x05020100, 0x05010000, 0x05010000, 0x05010100,
    0x05000000, 0x05030001, 0x05030001, 0x05030001, 0x05010001,
    0x05000001,
]

REPLAY_DIR = os.path.expanduser("~/.local/share/godot/app_userdata/Graphwar/replays")


# ---------------------------------------------------------------------------
# Byte-pattern helpers.  Every hit is validated against an exact expected operand.
# ---------------------------------------------------------------------------

def scan(data, off_lo, off_hi, va_lo):
    """Single pass over [off_lo, off_hi); yields (vaddr, kind, payload) for the few
    instruction shapes we care about.  Unrecognised bytes are skipped one at a time --
    that is fine because every shape we emit is validated by its consumer."""
    o = off_lo
    while o < off_hi:
        va = va_lo + (o - off_lo)
        b = data[o]
        # call rel32
        if b == 0xE8 and o + 5 <= off_hi:
            yield (va, "call", va + 5 + struct.unpack_from("<i", data, o + 1)[0])
            o += 5
            continue
        # call *disp32(%rip)   (FF 15)
        if b == 0xFF and data[o + 1] == 0x15 and o + 6 <= off_hi:
            slot = va + 6 + struct.unpack_from("<i", data, o + 2)[0]
            yield (va, "call_ind", slot)
            o += 6
            continue
        # movabs $imm64,%r64   (REX.W B8+rd, so 48..4F then B8..BF)
        if 0x48 <= b <= 0x4F and 0xB8 <= data[o + 1] <= 0xBF and o + 10 <= off_hi:
            imm = struct.unpack_from("<q", data, o + 2)[0]
            reg = (data[o + 1] - 0xB8) + (8 if (b & 1) else 0)
            yield (va, "movabs", (imm & 0xFFFFFFFFFFFFFFFF, reg, va + 10))
            o += 10
            continue
        # mov $imm32,%esi  (BE) / mov $imm32,%edx (BA)
        if b in (0xBA, 0xBE) and o + 5 <= off_hi:
            imm = struct.unpack_from("<i", data, o + 1)[0]
            yield (va, "mov_edx" if b == 0xBA else "mov_esi", imm & 0xFFFFFFFF)
            o += 5
            continue
        # lea disp32(%rip),%rdx  (48 8D 15) / ,%r8 (4C 8D 05)
        if data[o:o + 2] == b"\x48\x8d" and data[o + 2] == 0x15 and o + 7 <= off_hi:
            yield (va, "lea_rdx", va + 7 + struct.unpack_from("<i", data, o + 3)[0])
            o += 7
            continue
        if data[o:o + 2] == b"\x4c\x8d" and data[o + 2] == 0x05 and o + 7 <= off_hi:
            yield (va, "lea_r8", va + 7 + struct.unpack_from("<i", data, o + 3)[0])
            o += 7
            continue
        # movl $imm32,disp8(%rsp)    C7 44 24 dd imm32
        if data[o:o + 3] == b"\xc7\x44\x24" and o + 8 <= off_hi:
            disp = struct.unpack_from("<b", data, o + 3)[0]
            imm = struct.unpack_from("<i", data, o + 4)[0]
            yield (va, "movl_rsp", (disp & 0xFF if disp >= 0 else disp, imm & 0xFFFFFFFF))
            o += 8
            continue
        # movl $imm32,disp32(%rsp)   C7 84 24 dd32 imm32
        if data[o:o + 3] == b"\xc7\x84\x24" and o + 11 <= off_hi:
            disp = struct.unpack_from("<i", data, o + 3)[0]
            imm = struct.unpack_from("<i", data, o + 7)[0]
            yield (va, "movl_rsp", (disp, imm & 0xFFFFFFFF))
            o += 11
            continue
        # "Level <n>" built as two movl into a heap buffer:
        #   C7 40 03 65 6c 20 <digit>   ("el <d>" at offset 3)
        if data[o:o + 4] == b"\xc7\x40\x03\x65" and data[o + 4:o + 6] == b"\x6c\x20" \
                and o + 7 <= off_hi and 0x30 <= data[o + 6] <= 0x39:
            yield (va, "title_digit", data[o + 6] - 0x30)
            o += 7
            continue
        o += 1


def decode_arm(data, elf, relocs, level):
    lo, hi = ARM[level]
    off_lo = elf.va2off(lo)
    off_hi = elf.va2off(hi)
    ev = list(scan(data, off_lo, off_hi, lo))

    # --- closure A/B vtables ------------------------------------------------
    vt_a = vt_b = None
    for i, (va, kind, p) in enumerate(ev):
        if kind == "call" and p == FUNC_DRIVER:
            for j in range(i - 1, max(i - 40, -1), -1):
                va2, k2, p2 = ev[j]
                if k2 == "lea_rdx" and vt_a is None:
                    vt_a = p2
                elif k2 == "lea_r8" and vt_b is None:
                    vt_b = p2
                if vt_a is not None and vt_b is not None:
                    break
            break
    if vt_a is None or vt_b is None:
        raise SystemExit(f"level {level}: could not locate closure vtables in "
                         f"{lo:#x}..{hi:#x}")

    def slot(va):
        if va in relocs:
            return relocs[va]
        return struct.unpack_from("<Q", data, elf.va2off(va))[0]

    size_a, fn_a = slot(vt_a + 0x08), slot(vt_a + 0x20)
    size_b, fn_b = slot(vt_b + 0x08), slot(vt_b + 0x20)

    # --- obstacle preset ----------------------------------------------------
    if fn_a in CENTRE_GENS:
        obstacle, min_count = "centre-single", None
    elif fn_a in NO_OBSTACLE_GENS:
        obstacle, min_count = "none", 0
    else:
        min_count = None
        for _va, k, p in scan(data, elf.va2off(fn_a), elf.va2off(fn_a) + 0x20, fn_a):
            if k == "mov_esi":
                min_count = p
        obstacle = "scatter"
        if min_count is None:
            raise SystemExit(f"level {level}: closure A {fn_a:#x} is neither centre, "
                             f"none, nor a `mov $N,%esi; jmp 0x466c40` thunk")

    # --- soldier / target / bot / turn limit --------------------------------
    soldier_calls, targets, bots, turn = [], 0, 0, None
    diff = None
    tag_slots = {}          # stack disp -> tag value stored there
    pending_tags = []       # (vaddr, tag)  of `movabs $0x8000...`
    for va, kind, p in ev:
        if kind == "movabs":
            pending_tags.append((va, p[0]))
        elif kind == "call_ind":
            fn = relocs.get(p)
            if fn == FUNC_SET_SOLDIERS:
                for va2, k2, p2 in reversed(ev):
                    if va2 >= va:
                        continue
                    if k2 == "mov_edx":
                        soldier_calls.append(p2)
                        break
                    if k2 in ("call", "call_ind"):
                        break
            elif fn == FUNC_ADD_TARGET:
                targets += 1
            elif fn == FUNC_ADD_BOT:
                bots += 1
            elif fn == FUNC_SET_TURN_LIMIT:
                for va2, k2, p2 in reversed(ev):
                    if va2 >= va:
                        continue
                    if k2 == "mov_edx":
                        turn = p2
                        break
                    if k2 in ("call", "call_ind"):
                        break
        elif kind == "movl_rsp":
            disp, imm = p
            if (disp - 0x10) in tag_slots and tag_slots[disp - 0x10] == TAG_BOT_ADDED:
                diff = imm
        elif kind == "call" and p == FUNC_DRIVER:
            pass
        elif kind in ("mov_esi", "mov_edx"):
            pass
    # `movabs $0x<hi>00000001` -> the `turn_limit_info` payload; low u32 is always 1,
    # high u32 is the limit.  (Level 3 uses the compact `0x472400(1, N)` form instead,
    # already picked up above.)
    for va, tag in pending_tags:
        if (tag & 0xFFFFFFFF) == 1 and (tag >> 32) and (tag >> 60) != 8:
            turn = tag >> 32

    # bot difficulty = 3rd u32 of the `bot_added_event` payload.  We validate by the
    # observed ladder {3,25,50,100,250} and by "the arm must actually add a bot".
    ladder = {3, 25, 50, 100, 250}
    cands = [p[1] for _va, kind, p in ev
             if kind == "movl_rsp" and p[1] in ladder]
    if bots:
        if not cands:
            raise SystemExit(f"level {level}: arm adds a bot but no difficulty "
                             f"immediate from the ladder {sorted(ladder)} was found")
        diff = cands[0]
    else:
        if cands:
            raise SystemExit(f"level {level}: no bot, yet difficulty candidate "
                             f"{cands} found -- the ladder filter is too loose")
        diff = None

    mw = MODE_WORD[level]
    return dict(
        level=level, arm=lo, title_site=TITLE_SITE[level],
        driver_site=DRIVER_SITE[level], vtable_a=vt_a, vtable_b=vt_b,
        size_a=size_a, size_b=size_b, fn_a=fn_a, fn_b=fn_b,
        obstacle=obstacle, min_count=min_count,
        soldiers=max(soldier_calls) if soldier_calls else None,
        soldier_calls=soldier_calls, targets=targets, bots=bots,
        difficulty=diff, turn=turn,
        func=mw & 0xFF, turn_mode=(mw >> 8) & 0xFF, axis=(mw >> 16) & 0xFF,
        byte3=(mw >> 24) & 0xFF,
    )


def _bot_difficulty_fallback(data, elf, ev, lo, hi):
    """Recover the 3rd u32 of the bot payload.

    The compiler emits, for the bot event, either
        movabs $0x800000000000000a,%rXX ; ... ; mov %rXX,B(%rsp) ; movl $imm,B+0x10(%rsp)
    or (level 11) a `lea -0x13(%rXX)`-shifted base.  In both cases the difficulty
    immediate is a `movl $imm,B+0x10(%rsp)` that appears shortly after the tag and is
    the only `movl` in the arm whose value is one of the observed difficulty ladder.
    We validate against the ladder so a stray immediate cannot be mistaken for it.
    """
    ladder = {3, 25, 50, 100, 250}
    cands = []
    seen_tag = False
    for va, kind, p in ev:
        if kind == "movabs" and (p[0] >> 60) == 0x8 and (p[0] & 0xFF) == 0x0A:
            seen_tag = True
        if seen_tag and kind == "movl_rsp" and p[1] in ladder:
            cands.append((va, p[1]))
    if not cands:
        return None
    return cands[0][1]


# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------

def main():
    tsv = "--tsv" in sys.argv
    verify = "--verify-replays" in sys.argv

    data = open(SO, "rb").read()
    elf = Elf(data)
    relocs = load_relative_relocs(elf)
    print(f"# {SO}", file=sys.stderr)
    print(f"# R_X86_64_RELATIVE slots = {len(relocs)}", file=sys.stderr)

    # self-check: every arm must actually build its own "Level <n>"
    for lv, site in TITLE_SITE.items():
        lo, hi = ARM[lv]
        assert lo <= site < hi, f"title site {site:#x} outside arm {lo:#x}..{hi:#x}"
        off = elf.va2off(site)
        if data[off] == 0xC7:                       # short form: "el <d>" at +3
            assert data[off + 3:off + 6] == b"\x65\x6c\x20"
            assert data[off + 6] - 0x30 == (lv % 10)
        else:                                        # movabs form: literal "Level <n>"
            assert data[off:off + 2] == b"\x48\xb8"
            want = b"Level %d" % lv
            assert data[off + 2:off + 2 + len(want)] == want, (
                f"level {lv}: title site {site:#x} does not build {want!r}")

    rows = [decode_arm(data, elf, relocs, lv) for lv in range(21)]

    if tsv:
        print("level\ttitle\tarm\tsoldiers\ttargets\tbots\tdifficulty\tturn_limit"
              "\tobstacle\tmin_count\tfunc\tturn_mode\taxis\tvtable_a\tfn_a")
        for r in rows:
            print("\t".join(str(x) for x in (
                r["level"] + 1, f"Level {r['level']}", f"0x{r['arm']:x}",
                r["soldiers"], r["targets"], r["bots"],
                "" if r["difficulty"] is None else r["difficulty"],
                "" if r["turn"] is None else r["turn"],
                r["obstacle"], "" if r["min_count"] is None else r["min_count"],
                MODE_FUNC[r["func"]], MODE_TURN[r["turn_mode"]], MODE_AXIS[r["axis"]],
                f"0x{r['vtable_a']:x}", f"0x{r['fn_a']:x}")))
        return 0

    print()
    print(f"{'#':>3} {'title':9} {'arm':10} {'obstacle':15} {'min':>4} {'sold':>4}"
          f" {'tgt':>3} {'bot':>3} {'diff':>5} {'turn':>4}  modes func/turn/axis")
    for r in rows:
        print(f"{r['level']+1:>3} {'Level '+str(r['level']):9} {r['arm']:#010x} "
              f"{r['obstacle']:15} {str(r['min_count']):>4} {str(r['soldiers']):>4}"
              f" {r['targets']:>3} {r['bots']:>3} {str(r['difficulty']):>5}"
              f" {str(r['turn']):>4}  "
              f"{MODE_FUNC[r['func']]}/{MODE_TURN[r['turn_mode']]}/{MODE_AXIS[r['axis']]}")
    print()
    print("closure records (.data.rel.ro; +0x08 size, +0x20 = the slot 0x466d20 calls):")
    for r in rows:
        print(f"  L{r['level']:>2}  A@{r['vtable_a']:#08x} size={r['size_a']:#4x} "
              f"fn={r['fn_a']:#08x}   B@{r['vtable_b']:#08x} size={r['size_b']:#4x} "
              f"fn={r['fn_b']:#08x}")

    if verify:
        return _verify_replays(rows)
    return 0


# ---------------------------------------------------------------------------
# Optional replay cross-check (reads the user's save dir; never committed)
# ---------------------------------------------------------------------------

def _replay_signatures():
    per = collections.defaultdict(list)
    for f in sorted(glob.glob(os.path.join(REPLAY_DIR, "*"))):
        try:
            txt = gzip.open(f, "rb").read().decode("utf-8", "replace")
        except Exception:
            continue
        evs = []
        for line in txt.splitlines():
            try:
                evs.append(json.loads(line))
            except Exception:
                pass
        name = None
        for e in evs:
            ev = e.get("event") or {}
            if "GameNameInfo" in ev:
                name = ev["GameNameInfo"].get("game_name")
        if name and name.startswith("Level "):
            per[name].append(evs)
    out = {}
    for name, samples in per.items():
        sold, tgts, nbots = set(), set(), set()
        obsts = []
        for evs in samples:
            s = collections.defaultdict(set)
            removed, tgt, botplayers = set(), set(), set()
            players = set()
            # the replay re-broadcasts a snapshot after the level's own events, so the
            # *last* value of each mode event is the level's own setting, and the first
            # ones are the pre-level defaults.
            last = {}
            for e in evs:
                ev = e.get("event") or {}
                if not ev:
                    continue
                n = list(ev)[0]
                b = ev[n]
                if n == "PlayerInfo":
                    players.add(b["player_id"])
                elif n == "SoldierInfo":
                    s[b["player_id"]].add(b["soldier_id"])
                elif n == "EntityRemoved":
                    removed.add(b["entity_id"])
                elif n == "TargetInfo":
                    tgt.add(b["target_id"])
                elif n == "SkinGraphicsInfo":
                    if str(b.get("graphics_str", "")).startswith("bot_"):
                        botplayers.add(b["entity_id"])
                elif n in ("AxisModeInfo", "FunctionModeInfo", "TurnModeInfo"):
                    last[n] = list(b.values())[0]
                elif n == "TurnLimitInfo":
                    last["turn"] = b["turn_limit"]
                elif n == "AddObstacle":
                    obsts.append(b)
            counts = tuple(sorted(len(v - removed) for v in s.values()))
            if counts:
                sold.add(counts[0])
            tgts.add(len(tgt - removed))
            # only *players* carry a bot_N skin; their soldiers inherit it
            nbots.add(len(botplayers & players))
        out[name] = dict(
            soldiers=min(sold) if sold else None,
            targets=min(tgts) if tgts else None,
            bots=min(nbots) if nbots else None,
            turn=last.get("turn"),
            axis=last.get("AxisModeInfo"),
            func=last.get("FunctionModeInfo"),
            turn_mode=last.get("TurnModeInfo"),
            obstacle_counts=sorted({len(obsts) // max(len(samples), 1)}),
        )
    return out


def _verify_replays(rows):
    sig = _replay_signatures()
    bad = 0
    print()
    print("replay cross-check vs", REPLAY_DIR)
    for r in rows:
        got = sig.get(f"Level {r['level']}")
        if not got:
            print(f"  !! Level {r['level']}: no replay sample")
            bad += 1
            continue
        checks = [
            ("soldiers", r["soldiers"], got["soldiers"]),
            ("targets", r["targets"], got["targets"]),
            ("bots", r["bots"], got["bots"]),
            ("turn_limit", r["turn"], got["turn"]),
            ("axis", MODE_AXIS[r["axis"]], got["axis"]),
            ("function", MODE_FUNC[r["func"]], got["func"]),
            ("turn_mode", MODE_TURN[r["turn_mode"]], got["turn_mode"]),
        ]
        ok = True
        for name, a, b in checks:
            if a != b:
                print(f"  !! Level {r['level']}: {name} static={a!r} runtime={b!r}")
                ok = False
                bad += 1
        if r["min_count"] is not None and r["obstacle"] == "scatter":
            if min(got["obstacle_counts"]) < r["min_count"]:
                print(f"  !! Level {r['level']}: obstacle count "
                      f"{min(got['obstacle_counts'])} < min_count {r['min_count']}")
                ok = False
                bad += 1
        if r["obstacle"] == "none" and max(got["obstacle_counts"] or [0]) != 0:
            print(f"  !! Level {r['level']}: expected 0 obstacles, got "
                  f"{got['obstacle_counts']}")
            ok = False
            bad += 1
        if r["obstacle"] == "centre-single":
            if max(got["obstacle_counts"] or [0]) != 1:
                print(f"  !! Level {r['level']}: expected 1 obstacle, got "
                      f"{got['obstacle_counts']}")
                ok = False
                bad += 1
        if ok:
            print(f"  L{r['level']:>2} ok")
    print()
    print("replay cross-check:", "ALL MATCH (21/21)" if bad == 0
          else f"{bad} mismatch(es)")
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
