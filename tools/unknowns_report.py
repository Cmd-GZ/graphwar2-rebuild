#!/usr/bin/env python3
"""T14 · `docs/00` §3「明确未知」四条 —— 一次性复算并生成 `fixtures/unknowns_*.tsv`。

四条任务：
  ① 弹道用什么绘图原语（`docs/16` §12 ⑭）
  ② `LevelRules` 的文件归属（`docs/02` §17.4）
  ③ 掩膜的持有者与生命周期（`docs/08` §7）
  ④ `rank_manager` 段位算法 + `RankInfo` 字段数 2 vs 3（`docs/02` §15）

用法（仓库约定 venv；目标 `.so` 只读）：

    export GW2_TARGET_SO=$PWD/.reference/game/libgraphwarrust.so
    ../.reference/venv/bin/python tools/unknowns_report.py                # 只打印
    ../.reference/venv/bin/python tools/unknowns_report.py --emit         # 顺手写 fixtures/

地址口径：**全部是 ELF vaddr**（= 模块内偏移 = RVA）。本机 2026-11-26 复核：
`analysis/funcstarts_ghidra_va.pkl` 装的就是 ELF vaddr，**不要 ±0x100000**；
只有 Ghidra/REA 反编译文本里的 `_DAT_xxxx` / `FUN_xxxx` 才是 `+0x100000` 口径。

判据与自校验
------------
* ① **有界否定**：候选绘图原语名先做**整文件字节扫描**（不是只看 `.rodata`、不是 `strings`），
  不存在就出"0 命中"；存在的走 字面量 → 包装 → GOT 槽 → 调用点 全链。
  **正对照**：`draw_arc` 必须复现 `0x19f9d0 → 0x8abca0 → 槽 0x9bc020 → 3 处调用在 0x4b2890`。
* ② 归属用 `panic::Location`（`.data.rel.ro` 的 24 字节 `{ptr(R_RELATIVE), len, line, col}`，
  由 `tools/loc_xrefs.py` 从 `.so` 自己还原，不读上游 pkl）。
  **正对照**：`0x4eb9d0` / `0x4ed670` 必须落 `src/math_function.rs`。
* ③ 掩膜 = `Mutex` 里的 1000×600 字节；引用点用 `lock cmpxchg [r14+0x10],1` 认。
* ④ 线上字段数用 serde 的 `struct RankInfoEvent with 2 elements` 字面量 + 真实抓包夹具。
"""

import argparse
import collections
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
sys.path.insert(0, HERE)


# ---------------------------------------------------------------------------
# 小工具
# ---------------------------------------------------------------------------
def _ix():
    import loc_xrefs
    return loc_xrefs.LocXrefs(so=os.environ.get("GW2_TARGET_SO"), verbose=False)


def sec_of(ix, va):
    for lo, hi, name in ix.data_ranges:
        if lo <= va < hi:
            return name
    return "?"


def read_ro(ix, va, n):
    ro = ix.elf.sec(".rodata")
    if not (ro["addr"] <= va < ro["addr"] + ro["size"]):
        return None
    return ix.elf.read(va, n)


def f64_at(ix, va):
    b = read_ro(ix, va, 8)
    return None if b is None else struct.unpack("<d", b)[0]


def f32_at(ix, va):
    b = read_ro(ix, va, 4)
    return None if b is None else struct.unpack("<f", b)[0]


def whole_file_hits(ix, needle):
    """**整文件**字节扫描（比 `strings` 强：Rust 字面量不以 NUL 结尾）。"""
    b = needle.encode()
    out, p = [], ix.data.find(b)
    while p >= 0:
        out.append(p)
        p = ix.data.find(b, p + 1)
    return out


def file_off_to_va(ix, off):
    for s in ix.elf.secs:
        if s["type"] == 8:      # SHT_NOBITS：文件里没有字节
            continue
        if s["offset"] <= off < s["offset"] + s["size"]:
            return s["addr"] + (off - s["offset"]), s["name"]
    return None, None


def str_lit(ix, va, ln):
    b = ix.elf.read(va, ln)
    return None if b is None else b.decode("latin-1")


def find_strlits(ix, text, limit=64):
    ro = ix.elf.sec(".rodata")
    blob = ix.data[ro["offset"]:ro["offset"] + ro["size"]]
    needle = text.encode()
    out, p = [], blob.find(needle)
    while p >= 0 and len(out) < limit:
        out.append(ro["addr"] + p)
        p = blob.find(needle, p + 1)
    return out


def pairs_in_func(ix, fn, cap=0x2600):
    """gdext 包装里的 `(类名, 方法名)` 常量对。

        0x8abd39 lea rax,[rip-X] ; -> 0x29ef2d 'CanvasItem'
        0x8abd40 mov [rsp+0x48],rax
        0x8abd45 mov qword [rsp+0x50],0xa      ; len=10
        0x8abd4e lea rax,[rip-Y] ; -> 0x19f9d0 'draw_arc'
        0x8abd55 mov [rsp+0x58],rax
        0x8abd5a mov qword [rsp+0x60],8        ; len=8
    """
    import capstone
    from capstone import x86 as cx
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.detail = True
    md.syntax = capstone.CS_OPT_SYNTAX_INTEL
    end = ix.func_end(fn) or (fn + cap)
    n = max(0, min(end - fn, cap)) or cap
    blob = ix.elf.read(fn, n)
    if blob is None:
        return []
    ro = ix.elf.sec(".rodata")
    reg2addr, ptrs, lens, out = {}, {}, {}, []
    for ins in md.disasm(blob, fn):
        ops = ins.operands
        if ins.mnemonic == "lea" and len(ops) == 2 and ops[0].type == cx.X86_OP_REG \
                and ops[1].type == cx.X86_OP_MEM and ops[1].mem.base == cx.X86_REG_RIP:
            tgt = ins.address + ins.size + ops[1].mem.disp
            if ro["addr"] <= tgt < ro["addr"] + ro["size"]:
                reg2addr[ops[0].reg] = tgt
            else:
                reg2addr.pop(ops[0].reg, None)
        elif ins.mnemonic == "mov" and len(ops) == 2 and ops[0].type == cx.X86_OP_MEM \
                and ops[0].mem.base == cx.X86_REG_RSP:
            disp = ops[0].mem.disp
            if ops[1].type == cx.X86_OP_REG and ops[1].reg in reg2addr:
                ptrs[disp] = (reg2addr[ops[1].reg], ins.address)
            elif ops[1].type == cx.X86_OP_IMM and 0 < ops[1].imm < 4096:
                lens[disp] = (ops[1].imm, ins.address)
        if ops and ops[0].type == cx.X86_OP_REG and ins.mnemonic not in (
                "lea", "mov", "movzx", "movsx", "movsxd"):
            reg2addr.pop(ops[0].reg, None)
    for disp, (addr, pins) in sorted(ptrs.items(), key=lambda kv: kv[1][1]):
        ln = lens.get(disp + 8)
        if not ln:
            continue
        txt = str_lit(ix, addr, ln[0])
        if txt is None:
            continue
        out.append((addr, ln[0], txt, pins, ln[1]))
    return out


def slots_of_addend(ix, addend):
    return sorted(s for s, a in ix.relocs.items() if a == addend)


def slot_refs(ix, slot):
    return sorted(ix.refs_to(slot))


def callers_of(ix, target):
    direct = sorted(ix.calls.get(target, ()))
    indirect = []
    for slot in slots_of_addend(ix, target):
        for insn, form, mn, op in slot_refs(ix, slot):
            indirect.append((slot, insn, mn, op, ix.func_of(insn)))
    return direct, indirect


def locs_of(ix, fn):
    out = []
    for (f, line, col), hits in sorted(ix.locs_of_func(fn).items()):
        out.append((f, line, col, min(h[0] for h in hits)))
    return out


def src_locs(ix, fn):
    return [t for t in locs_of(ix, fn) if t[0].startswith("src/")]


# ---------------------------------------------------------------------------
# ① 弹道绘图原语
# ---------------------------------------------------------------------------
CANVAS_METHODS = [
    # 画线/曲线一族的全部候选（Godot 4 CanvasItem API）
    "draw_line", "draw_polyline", "draw_polyline_colors", "draw_multiline",
    "draw_multiline_colors", "draw_dashed_line", "draw_primitive", "draw_mesh",
    "draw_colored_polygon", "draw_polygon", "draw_arc", "draw_rect", "draw_circle",
    "draw_texture_rect", "draw_set_transform", "draw_string", "draw_char",
    # Line2D 节点一族
    "add_point", "set_point_position", "get_point_position", "get_point_count",
    "clear_points", "set_width", "set_default_color", "set_joint_mode",
    "set_begin_cap_mode", "set_end_cap_mode", "set_points",
]


def task_plot(ix, rows):
    print("=" * 78)
    print("① 弹道用什么绘图原语")
    print("=" * 78)
    ok = True
    # --- 整文件字节扫描：候选名在不在 ---
    print("\n[A] 整文件字节扫描（`.so` 全文件，不只 `.rodata`）")
    present, absent = {}, []
    for name in CANVAS_METHODS:
        hits = whole_file_hits(ix, name)
        if not hits:
            absent.append(name)
            continue
        vas = []
        for off in hits[:4]:
            va, sec = file_off_to_va(ix, off)
            vas.append((va, sec))
            rows.append(("prim-scan", name, "", hex(va or 0), sec or "", "present", ""))
        present[name] = vas
        print("  %-22s %d 命中 %s" % (name, len(hits), [hex(v) for v, _ in vas[:3]]))
    print("  ---- 不存在（0 命中）----")
    print("  " + ", ".join(absent))
    for name in absent:
        rows.append(("prim-scan", name, "", "", "", "absent", "整文件 0 命中"))
    return ok


def chain(ix, name, rows=None, tag="prim-chain"):
    """字面量 → 包装 → GOT 槽 → 调用点。返回 [(lit, wrapper, slot, [callsites])]。"""
    out = []
    for lit in find_strlits(ix, name, limit=4):
        wrappers = collections.defaultdict(list)
        for insn, form, mn, op in ix.refs_to(lit):
            wrappers[ix.func_of(insn)].append(insn)
        for fn in sorted(x for x in wrappers if x is not None):
            # 只认"字面量后面紧跟 len = 名字长度"的那个函数（真正的 gdext 包装）
            is_wrap = any(addr == lit and ln == len(name)
                          for addr, ln, _t, _p, _l in pairs_in_func(ix, fn))
            if not is_wrap:
                continue
            slots = slots_of_addend(ix, fn)
            callsites = []
            for s in slots:
                for insn, form, mn, op in slot_refs(ix, s):
                    callsites.append((s, insn, mn, op, ix.func_of(insn)))
            out.append((lit, fn, slots, callsites))
            if rows is not None:
                if not callsites:
                    rows.append((tag, name, "%#x" % lit, "%#x" % fn, "", "no-slot-use",
                                 "字面量→包装 OK，槽无调用点"))
                for s, insn, mn, op, owner in callsites:
                    rows.append((tag, name, "%#x" % lit, "%#x" % fn, "%#x" % s,
                                 "%#x" % insn, "in %#x" % (owner or 0)))
    return out


def task_plot_chain(ix, rows):
    print("\n[B] 字面量 → 包装 → GOT 槽 → 调用点")
    # 正对照：draw_arc 必须复现文档值
    ctl = chain(ix, "draw_arc", rows)
    got = [(w, tuple(s), tuple(sorted({cs[1] for cs in cs_list})),
            tuple(sorted({cs[4] for cs in cs_list})))
           for _l, w, s, cs_list in ctl]
    expect = (0x8abca0, (0x9bc020,), (0x4b366c, 0x4b36fd, 0x4b378a, 0x4d8485, 0x4d84ed),
              (0x4b2890, 0x4d8100))
    print("  正对照 draw_arc: %s" % ("OK" if expect in got else "**FAIL** %s" % got))
    if expect not in got:
        rows.append(("prim-control", "draw_arc", "", "", "", "FAIL", repr(got)))
    for name in ("draw_line", "draw_rect", "draw_circle", "draw_polygon",
                 "draw_texture_rect", "draw_set_transform"):
        for lit, fn, slots, callsites in chain(ix, name, rows):
            funcs = collections.Counter(cs[4] for cs in callsites)
            print("  %-20s lit %#x wrap %#x slot %s  callsites=%d  funcs=%s"
                  % (name, lit, fn, [hex(s) for s in slots], len(callsites),
                     {hex(k or 0): v for k, v in funcs.items()}))
    print("\n[C] Line2D 节点一族（弹道的实际载体）")
    for name in ("add_point", "set_width", "set_default_color", "set_joint_mode"):
        for lit, fn, slots, callsites in chain(ix, name, rows):
            funcs = collections.Counter(cs[4] for cs in callsites)
            print("  %-18s lit %#x wrap %#x slot %s  refs=%d  funcs=%s"
                  % (name, lit, fn, [hex(s) for s in slots], len(callsites),
                     {hex(k or 0): v for k, v in funcs.items()}))


def task_plot_consts(ix, rows):
    print("\n[D] 弹道的几何/揭示公式（立即数逐位）")
    facts = [
        ("Line2D::set_width 的常量", 0x1baff8, "f32", 0.125),
        ("揭示点数乘数 (tick→point)", 0x19ef90, "f64", 30.0),
        ("f64→usize 饱和上界", 0x19f008, "f64", 18446744073709551616.0),
        ("f64→usize 饱和中点", 0x19f618, "f64", 9223372036854775808.0),
    ]
    for label, va, kind, want in facts:
        got = f32_at(ix, va) if kind == "f32" else f64_at(ix, va)
        ok = got is not None and abs(got - want) <= abs(want) * 1e-12
        print("  %-28s %#x = %-22r %s" % (label, va, got, "OK" if ok else "MISMATCH"))
        rows.append(("plot-const", label, "%#x" % va, kind, repr(got),
                     "ok" if ok else "mismatch", "期望 %r" % want))
    # 揭示公式的指令序列
    print("  揭示公式（0x4b67a0–0x4b6815，在 GraphwarPlane::draw 0x4b5df0 内）：")
    for line in ("0x4b67a0  mov  eax, dword ptr [r12 - 4]      ; active_tick (u32)",
                 "0x4b67ad  movsd xmm1, [rsp + 0x130]        ; get_fractional_tick() 0x519530",
                 "0x4b67b6  subsd xmm1, xmm0                 ; - (double)active_tick",
                 "0x4b67ba  mulsd xmm1, [0x19ef90]           ; * 30.0",
                 "0x4b67c2  cvttsd2si rax, xmm1              ; trunc → usize（饱和见下）",
                 "0x4b67f3  cmovae r13, rdx                  ; <0 ⇒ 0",
                 "0x4b6806  cmova  r13, -1                   ; >2^64 ⇒ usize::MAX",
                 "0x4b6812  cmp    r12, r13                 ; r12 = points.len",
                 "0x4b6815  cmovb  r13, r12                 ; reveal = min(len, …)"):
        print("    " + line)
        rows.append(("plot-reveal", "reveal", "", "", "", "observed", line))


# ---------------------------------------------------------------------------
# ② LevelRules 归属
# ---------------------------------------------------------------------------
LEVELRULES_FUNCS = [
    ("get_alive_teams", 0x473520),
    ("evaluate（含上限/两队入口 0x475a64/0x475a80/0x475a88）", 0x4759f0),
    ("winner_byte 表（0x478133–0x478236）", 0x4780c0),
    ("handle_tick（对照，已结案）", 0x470840),
    ("execute_results（对照，已结案）", 0x472470),
    ("SELF-CHECK 求值器", 0x4eb9d0),
    ("SELF-CHECK 解析器", 0x4ed670),
]


def task_levelrules(ix, rows):
    print("\n" + "=" * 78)
    print("② LevelRules 的文件归属（panic::Location）")
    print("=" * 78)
    for label, va in LEVELRULES_FUNCS:
        fn = ix.func_of(va) or va
        locs = src_locs(ix, fn)
        locs_all = locs_of(ix, fn)
        print("  %#x  span %#x  %-46s" % (fn, (ix.func_end(fn) or 0) - fn, label))
        if locs_all:
            files = collections.Counter(t[0] for t in locs_all)
            for f, n in files.most_common():
                print("      %-40s %d 条 Location" % (f, n))
            for f, line, col, insn in locs[:6]:
                print("        %s:%d (引用指令 %#x)" % (f, line, insn))
            rows.append(("levelrules", label, "%#x" % fn, locs[0][0] if locs else "",
                         str(sorted({t[0] for t in locs_all})), "observed", ""))
        else:
            print("      （无 Location）")
            rows.append(("levelrules", label, "%#x" % fn, "", "", "no-location", ""))
    # 自校验
    chk = {}
    for va in (0x4eb9d0, 0x4ed670):
        fn = ix.func_of(va) or va
        chk[va] = sorted({t[0] for t in locs_of(ix, fn)})
    ok = all(any("math_function.rs" in f for f in v) for v in chk.values())
    print("  正对照（0x4eb9d0 / 0x4ed670 → math_function.rs）: %s" % ("OK" if ok else "**FAIL**"))
    rows.append(("levelrules-control", "math_function.rs", "0x4eb9d0,0x4ed670", "",
                 repr(chk), "ok" if ok else "FAIL", ""))
    # 该模块 Location 函数总表（用于看地址簇）
    gsm = sorted({fn for slot, (f, _l, _c) in ix.locs.items()
                  if f.endswith("graphwar_game_state_manager.rs")
                  for fn in ix.funcs_of_loc(slot)})
    print("  `graphwar_game_state_manager.rs` 有 Location 的函数：%d 个"
          "（地址范围 %#x–%#x）" % (len(gsm), gsm[0], gsm[-1]))
    for fn in gsm:
        rows.append(("gsm-funcs", "%#x" % fn, "%#x" % ((ix.func_end(fn) or 0) - fn), "", "", "location", ""))
    return gsm


# ---------------------------------------------------------------------------
# ③ 掩膜：持有者与生命周期
# ---------------------------------------------------------------------------
MASK_FUNCS = [
    ("分配/构造：alloc(600000)+memset(0xFF)+Arc<Mutex<Vec<u8>>>", 0x523020),
    ("obstacle_manager::new（ctor，:208）→ 调 0x523020 + 订阅 tick_event", 0x3f2360),
    ("光栅化（写，:89/:100）", 0x3f11e0),
    ("is_obstacle（读，单点查表）", 0x3f15b0),
    ("加一个障碍（写，:258/:259，锁 = lock cmpxchg [self[0x18]+0x10]）", 0x3f17a0),
    ("挖洞/删一个（写，:263/:264）", 0x3f1b30),
    ("按列表重建（无 Location，遍历→0x3f17a0→0x3f11e0）", 0x3f1e50),
    ("按 tick 应用 QueueDestroyObstacle（:295）→ 0x3f1b30", 0x3f1d30),
]


def task_mask(ix, rows):
    print("\n" + "=" * 78)
    print("③ 掩膜的持有者与生命周期")
    print("=" * 78)
    for label, va in MASK_FUNCS:
        fn = ix.func_of(va) or va
        end = ix.func_end(fn) or 0
        d, ind = callers_of(ix, fn)
        print("  %#x span %#x  %s" % (fn, end - fn, label))
        for f, line, col, insn in src_locs(ix, fn)[:4]:
            print("      LOC %s:%d" % (f, line))
        print("      callers direct=%d %s" % (len(d), [hex(x) for x in d[:6]]))
        for slot, insn, mn, op, owner in ind[:4]:
            print("      callers slot %#x @%#x in FUNC %#x" % (slot, insn, owner or 0))
        rows.append(("mask-fn", label, "%#x" % fn, "%#x" % (end - fn),
                     ";".join("%s:%d" % (f, l) for f, l, _c, _i in src_locs(ix, fn)[:3]),
                     "direct=%d indirect=%d" % (len(d), len(ind)),
                     ",".join("%#x" % x for x in d[:6])))
        for x in d:
            owner = ix.func_of(x) or 0
            locs = ";".join("%s:%d" % (f, l) for f, l, _c, _i in src_locs(ix, owner)[:3])
            rows.append(("mask-caller", label, "%#x" % fn, "%#x" % x, locs, "direct", ""))
    # 锁与字段偏移（从 0x3f15b0 / 0x3f17a0 的指令序读出）
    print("\n  锁与字段偏移（0x3f15b0 / 0x3f17a0 / 0x3f1b30 共同的取锁序，观测）：")
    for line in ("mov  r14, qword ptr [rdi + 0x18]   ; self+0x18 = 掩膜的 Arc 指针",
                 "lock cmpxchg dword ptr [r14 + 0x10], 1  ; +0x10 = Mutex 锁字",
                 "movzx eax, byte ptr [r14 + 0x14]   ; +0x14 = 中毒标志",
                 "mov  rdi, qword ptr [r14 + 0x20]   ; +0x20 = 数据指针（600000 字节）",
                 "mov  rsi, qword ptr [r14 + 0x28]   ; +0x28 = 长度 = 600000"):
        print("    " + line)
        rows.append(("mask-lock", "layout", "", "", "", "observed", line))
    print("\n  数据流小结：")
    for line in ("创建：0x523020 alloc(0x927c0=600000,1) + memset(0xFF) → 包进 0x30 头"
                 "（strong@+0, weak@+8, 锁 u32@+0x10, 毒@+0x14, cap@+0x18, ptr@+0x20, len@+0x28）",
                 "持有：`GraphwarGameClient::new` 0x517690(graphwar_game_client.rs:53–60) → "
                 "0x3f2360(obstacle_manager.rs:208) → 0x523020；"
                 "`graphwar_game_server.rs:219–222` 的 0x4500c0 也调它",
                 "增量写①：事件分派 0x3e2830 @0x3e414c → 0x3f17a0(加) / @0x3e43b1 → 0x3f1b30(挖)",
                 "增量写②：tick 路径 0x3f1d30(obstacle_manager.rs:295) → 0x3f1b30（QueueDestroyObstacle 到期挖洞）",
                 "整表重建：0x466c40(graphwar_level_generator.rs:179–180) → 0x3f1e50 → 每条 0x3f17a0 → 0x3f11e0",
                 "读：0x3f15b0（:89 光栅化 / :100 之后单点查表）——对局链不直接调它，"
                 "`0x3c2bc0` 内联同一套 1000/50/500 换算（docs/08 §11）"):
        print("    · " + line)
        rows.append(("mask-flow", "", "", "", "", "observed", line))


# ---------------------------------------------------------------------------
# ④ rank_manager / RankInfo
# ---------------------------------------------------------------------------
RANK_FUNCS = [
    ("broadcast all（遍历 map 发 tag 67）", 0x454d80),
    ("entity_removed 处理（删 map 条目）", 0x454e80),
    ("init/订阅 new_connection + entity_removed", 0x454f30),
    ("set/clear（写 map + 发 tag 67）", 0x455090),
    ("远端事件处理器（调 0x455090 的一处）", 0x3dee00),
    ("事件分派器（调 0x455090 的另一处）", 0x3e2830),
    ("22 张段位图的加载者", 0x4c9190),
]


# 只看"算数"：浮点四则 + 整数除法/取模。**不含位运算**（hashmap 的 `shl/shr/and` 是索引，
# 不是段位算术；把它们算进来会让正对照失真）。
ARITH_MNEM = ("divsd", "divss", "div", "idiv", "mulsd", "mulss", "imul",
              "addsd", "subsd", "cvtsi2sd", "cvtsi2ss")


def _mnem_counter(ix, fn):
    import capstone
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.syntax = capstone.CS_OPT_SYNTAX_INTEL
    end = ix.func_end(fn) or fn
    blob = ix.elf.read(fn, end - fn)
    c = collections.Counter()
    for ins in md.disasm(blob, fn):
        c[ins.mnemonic] += 1
    return c


def task_rank_arith(ix, rows):
    """「rank_manager 里算不出来」的**机器检查**：数指令 + 正对照。"""
    print("\n  rank_manager「没有算术」的机器检查（指令计数；同一次反汇编里带正对照）：")
    for label, fn, expect_zero in (("rank broadcast", 0x454d80, True),
                                   ("rank remove", 0x454e80, True),
                                   ("rank init", 0x454f30, True),
                                   ("rank set", 0x455090, True),
                                   ("CONTROL GraphwarPlane::draw", 0x4b5df0, False),
                                   ("CONTROL EntityDrawer::draw", 0x4b2890, False)):
        c = _mnem_counter(ix, fn)
        n = sum(c.values())
        arith = {k: c[k] for k in ARITH_MNEM if c.get(k)}
        got_zero = not arith
        verdict = "ok" if got_zero == expect_zero else "FAIL"
        print("    %-30s %#x %5d insns  arith=%s  %s"
              % (label, fn, n, arith or "{}", verdict))
        rows.append(("rank-arith", label, "%#x" % fn, str(n), repr(arith), verdict,
                     "期望 arithmetic %s" % ("= 0" if expect_zero else "> 0")))


def task_rank(ix, rows):
    print("\n" + "=" * 78)
    print("④ rank_manager 段位算法 + RankInfo 字段数")
    print("=" * 78)
    for label, va in RANK_FUNCS:
        fn = ix.func_of(va) or va
        d, ind = callers_of(ix, fn)
        print("  %#x span %#x  %s" % (fn, (ix.func_end(fn) or 0) - fn, label))
        for f, line, col, insn in src_locs(ix, fn)[:4]:
            print("      LOC %s:%d" % (f, line))
        rows.append(("rank-fn", label, "%#x" % fn, "%#x" % ((ix.func_end(fn) or 0) - fn),
                     ";".join("%s:%d" % (f, l) for f, l, _c, _i in src_locs(ix, fn)[:4]),
                     "observed", ""))
    # serde 的 expecting 串 = 字段数的**二进制内**答案
    print("\n  线上字段数：")
    for text, want in (("struct RankInfoEvent with 2 elements", 2),
                       ("struct RankRequestEvent with 2 elements", 2)):
        lits = find_strlits(ix, text, limit=2)
        print("    %-44r 字面量 %s ⇒ N=%d" % (text, [hex(x) for x in lits] or "未找到", want))
        for lit in lits:
            rows.append(("rank-wire", text, "%#x" % lit, "", "", "observed", "N=%d" % want))
    # 3 个 dword 的 payload：两个生产点
    print("\n  内存 payload 是 3 个 dword，逐址：")
    for line in ("0x454e14  pshufd xmm0,xmm0,0xe1  ; (rank, entity_id) 交换成 (value,key)",
                 "0x454e19  movq [rsp+0xc],xmm0     ; +0xc = rank(u32), +0x10 = entity_id(u32)",
                 "0x454e0c  mov  dword [rsp+8],1    ; +8 = Option 的判别值（1 = Some）",
                 "0x454e1f  movabs rax,0x8000000000000043  ; +0 = Event 判别值 67",
                 "0x455103  mov [rsp+0x1c],ebx      ; set 路径：+0x1c = value",
                 "0x4550fe  mov [rsp+0x18],r14d     ;          +0x18 = 判别值（edx；==1 ⇒ 存进 map）",
                 "0x455107  mov [rsp+0x20],ebp      ;          +0x20 = entity_id",
                 "0x4550b0  cmp edx,1 / jne 0x4550c3 ; 判别值 != 1 ⇒ 走删条目分支 ⇒ 线上 rank:null"):
        print("    " + line)
        rows.append(("rank-payload", "", "", "", "", "observed", line))
    # 22 张段位图
    print("\n  段位图资源（`.rodata` 逐条）：")
    paths = []
    ro = ix.elf.sec(".rodata")
    blob = ix.data[ro["offset"]:ro["offset"] + ro["size"]]
    import re
    for m in re.finditer(rb"res://ranks/rank_(\d+)\.svg", blob):
        paths.append((int(m.group(1)), ro["addr"] + m.start(), len(m.group())))
    print("    共 %d 个：%s" % (len(paths),
                                ", ".join("rank_%d@%#x" % (i, va) for i, va, _l in paths[:3])
                                + " … rank_%d@%#x" % (paths[-1][0], paths[-1][1])))
    for i, va, ln in paths:
        rows.append(("rank-asset", "rank_%02d" % i, "%#x" % va, str(ln), "res://ranks/rank_%d.svg" % i, "observed", ""))
    # 有界否定：段位名/阈值表
    print("\n  有界否定（`.rodata` 定向扫描）：")
    for kw in ("Bronze", "Silver", "Gold", "Platinum", "Diamond", "Master", "Grandmaster",
               "bronze", "silver", "elite", "leaderboard", "elo", "rating", "mmr"):
        n = len(whole_file_hits(ix, kw))
        print("    %-14s %d 命中" % (kw, n))
        rows.append(("rank-names", kw, str(n), "", "", "absent" if n == 0 else "present", ""))
    task_rank_arith(ix, rows)
    # 真实抓包（夹具）里的字段
    print("\n  真实抓包夹具（`fixtures/protocol*.jsonl`）：")
    for p in ("fixtures/protocol.jsonl", "fixtures/protocol_match.jsonl"):
        fp = os.path.join(REPO, p)
        if not os.path.exists(fp):
            continue
        for line in open(fp, encoding="utf-8"):
            line = line.strip()
            if '"RankInfo"' in line or '"RankRequest"' in line:
                print("    %s: %s" % (p, line))
                rows.append(("rank-wire-capture", p, "", "", "", "observed", line))


# ---------------------------------------------------------------------------
def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--emit", action="store_true", help="写 fixtures/unknowns_*.tsv")
    ap.add_argument("--only", default="", help="逗号分隔：plot,levelrules,mask,rank")
    a = ap.parse_args(argv)
    only = set(x.strip() for x in a.only.split(",") if x.strip())
    ix = _ix()
    print("[unknowns_report] so=%s sha 需自行核对（expect 9a538115…）"
          % (os.environ.get("GW2_TARGET_SO") or "DEFAULT"))

    tables = {"plot": [], "levelrules": [], "mask": [], "rank": []}
    if not only or "plot" in only:
        task_plot(ix, tables["plot"])
        task_plot_chain(ix, tables["plot"])
        task_plot_consts(ix, tables["plot"])
    if not only or "levelrules" in only:
        task_levelrules(ix, tables["levelrules"])
    if not only or "mask" in only:
        task_mask(ix, tables["mask"])
    if not only or "rank" in only:
        task_rank(ix, tables["rank"])

    if a.emit:
        for name, rows in tables.items():
            if not rows:                 # `--only` 时不要用空表覆盖别人的夹具
                continue
            path = os.path.join(REPO, "fixtures", "unknowns_%s.tsv" % name)
            with open(path, "w", encoding="utf-8") as f:
                f.write("\t".join(["kind", "subject", "addr", "detail", "extra", "evidence", "note"]) + "\n")
                for r in rows:
                    f.write("\t".join(str(x).replace("\t", " ") for x in r) + "\n")
            print("\n[emit] %s  %d 行" % (path, len(rows)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
