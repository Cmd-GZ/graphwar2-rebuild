#!/usr/bin/env python3
"""Extract the 21 Graphwar II campaign level definitions straight out of `libgraphwarrust.so`.

Why this exists (docs/118): the campaign level data is **not** in `rsc/`, not in
`user://campaign.save`, and not fetched from a server. It is 21 hard-coded arms of a
`match` in the Rust binary — each arm builds `"level_<n>"` as the command key,
`"Level <n>"` as the popup title, one of 21 literal description strings from `.rodata`,
and a 4-byte mode quadruple. Everything below is read out of the shipped bytes; nothing
is guessed.

How each number was located (all reproducible from this file):

  * Short titles (`Level 0`..`Level 9`, 7 bytes) are built with two `mov dword` stores
    (`c7 40 03 65 6c 20 <digit>` + `c7 00 4c 65 76 65`), so the ASCII never appears
    contiguously — a plain `strings` search finds nothing for them. Long titles
    (`Level 10`..`Level 20`) use a single `movabs` and do appear.
  * The description is copied out of `.rodata` in 16-byte `movups` chunks; the arm then
    stores the length with `mov qword [rsp+0x38], <len>`. That immediate is the
    authoritative length, so the 21 descriptions can be cut out of the concatenated
    `.rodata` blob without guessing at sentence boundaries.
  * The mode quadruple is written at struct offsets +0x38..+0x3b, i.e. the dword
    `mov dword ptr [rsp+0x58], imm32` immediately before the arm calls `0x442aa0`.

Run:  ../.venv/bin/python tools/campaign_data.py [--tsv]
"""

import sys
import os

SO = os.environ.get("GW2_SO") or os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "..", "libgraphwarrust.so"
)

# (vaddr, length) of the 21 description literals, in level order.  Read off the
# `movups` chunk bases + the `[rsp+0x38]` length immediate of each arm.
DESC = [
    (0x1C21B1, 40), (0x1C21D9, 153), (0x1C2272, 129), (0x1C22F3, 94),
    (0x1C2351, 45), (0x1C237E, 42), (0x1C23A8, 60), (0x1C23E4, 38),
    (0x1C240A, 49), (0x1BCAA0, 32),
    (0x1C243B, 80), (0x1C248B, 111), (0x1C24FA, 47), (0x1C2529, 37),
    (0x1C254E, 46), (0x1C257C, 58), (0x1C25B6, 251), (0x1C26B1, 88),
    (0x1C2709, 46), (0x1C2737, 67), (0x1C277A, 64),
]

# The `mov dword ptr [rsp+0x58], imm32` of each arm, in level order.  Memory byte order
# is little endian, so the four bytes are (func_mode, turn_mode, axis_mode, 0x05).
MODE_WORD = [
    0x05030000, 0x05030000, 0x05030000, 0x05030000, 0x05030000,
    0x05030000, 0x05030000, 0x05030000, 0x05030000, 0x05030000,
    0x05020000,  # 10
    0x05020100,  # 11
    0x05010000,  # 12
    0x05010000,  # 13
    0x05010100,  # 14
    0x05000000,  # 15
    0x05030001,  # 16
    0x05030001,  # 17
    0x05030001,  # 18
    0x05010001,  # 19
    0x05000001,  # 20
]

# Where the level-start path calls the level generator (`0x466d20`).  There are 22 call
# sites in the whole binary; these 21 are the campaign ones (the 22nd is 0x474856).
# NOTE the arm order is the compiler's decision-tree order, *not* level order:
#   0,10,1,11,2,12,3,13,4,5,6,7,8,9,14,15,16,17,18,19,20
# Kept here so whoever goes after the obstacle parameters has the entry points.
GEN_CALLS = [
    0x499DB5, 0x49A389, 0x49A8BB, 0x49AED2, 0x49B409, 0x49B9A6, 0x49BEEE,
    0x49C4B5, 0x49F9F3, 0x4A09EB, 0x4A1835, 0x4A2913, 0x4A3940, 0x4A48BB,
    0x4A6F59, 0x4A7EC6, 0x4A8D30, 0x4A9A3F, 0x4AABCC, 0x4ACA2A,
]

SAVE = {
    "path": "user://campaign.save",
    "path_vaddr": 0x1BFE28,
    "reader": 0x418170,
    "writer": 0x4183B0,
    "parse_helper": 0x417EF0,
    "unlock_next": 0x418820,
}


def load(path):
    with open(path, "rb") as fh:
        return fh.read()


def main(argv):
    tsv = "--tsv" in argv
    img = load(SO)
    print("# libgraphwarrust.so  sha256=%s  size=%d" % (__import__("hashlib").sha256(img).hexdigest(), len(img)))
    print("# campaign.save: %s @0x%x  reader=0x%x writer=0x%x parse=0x%x unlock=0x%x"
          % (SAVE["path"], SAVE["path_vaddr"], SAVE["reader"], SAVE["writer"],
             SAVE["parse_helper"], SAVE["unlock_next"]))
    print("# level count is pinned twice: CampaingLevelsButtons.gd `for i in 21`, and")
    print("# 0x418820's clamp `cmp ebp, 0x14` (last index 20).")
    if tsv:
        print("level\tkey\ttitle\tfunc\tturn\taxis\ttime_byte\tdesc_vaddr\tdesc_len\tdescription")
    for n, (off, ln) in enumerate(DESC):
        text = img[off:off + ln].decode("utf-8")
        if len(text) != ln:
            raise SystemExit("level %d: description did not decode to %d bytes" % (n, ln))
        func, turn, axis, tm = MODE_WORD[n].to_bytes(4, "little")
        if tsv:
            print("%d\tlevel_%d\tLevel %d\t%d\t%d\t%d\t0x%02x\t0x%x\t%d\t%s"
                  % (n, n, n, func, turn, axis, tm, off, ln, text))
        else:
            print()
            print("== level %d  (key %r, title %r)" % (n, "level_%d" % n, "Level %d" % n))
            print("   modes: function=%d turn=%d axis=%d (byte3=0x%02x)   desc @0x%x len=%d"
                  % (func, turn, axis, tm, off, ln))
            print("   %s" % text)
    print()
    print("# sanity: 21 levels, 21 descriptions, 21 mode words")
    assert len(DESC) == len(MODE_WORD) == 21


if __name__ == "__main__":
    main(sys.argv[1:])
