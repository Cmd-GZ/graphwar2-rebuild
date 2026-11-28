#!/usr/bin/env python3
"""F3 缺口逐条分类（**度量口径审计**，只审计不改码）。

## 为什么有这个工具

`fixtures/function_coverage.tsv` 把 527 个原版可归属函数分成 F1/F2/F3/F4。分母 527 是诚实的，
但 **F3 的 153 条不是同一种东西**：T71 在三个重模块的 35 条 F3 里逐指令取证后发现，
33 条**根本不该由我们手写**（12 条 = `core::slice::sort` 的单态化、19 条 = gdext 宏产物）。
把它们算进"缺口"会让函数级复原率**虚低**；反过来，若把 std 内核的地址写进我们函数的注释，
`tools/function_coverage.py` 的 P2（地址指认）会**虚高**（T71 实测假抬 5 条 F1，docs/06 §123.5）。

本工具按 **`fixtures/f3_triage_plan.txt` 预先写死**的判据，把 F3 逐条判成
`real-missing / na:std-mono / na:gdext-glue / na:other / unknown` 之一，并写出
`fixtures/f3_triage.tsv`（`vaddr | 名字 | FDE尺寸 | 判据命中 | 类别 | 依据`）。

## 口径

* 地址一律 **ELF vaddr（RVA）**；Ghidra/REA = 本工具地址 + 0x100000（只加一次）。
* 函数范围 = `.eh_frame` FDE（自解，不读 `analysis/*.pkl`）；引用/表项 = `.rela.dyn`（`R_X86_64_RELATIVE`）。
* 归属 = `panic::Location`（`.data.rel.ro` 的 {file,len,line,col}）。
* 反汇编 = capstone x86-64；ELF/FDE 复用 `tools/addr_probe.py` 的 `Ctx`（同一套解析，避免两套口径）。

## 用法

    PY=.reference/venv/bin/python
    $PY tools/f3_triage.py --features <vaddr> [<vaddr> ...]   # 打印特征（人工复核用）
    $PY tools/f3_triage.py --control                          # 正/负对照（先跑这个）
    $PY tools/f3_triage.py                                    # 全量：写 fixtures/f3_triage.tsv + 打印统计
    $PY tools/f3_triage.py --stats                            # 只打印统计（不写文件）

**本工具不改任何计分工具**：`tools/function_coverage.py` 的 F 档是它的输入，不是它的输出。
"""

from __future__ import annotations

import argparse
import collections
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import addr_probe as ap  # noqa: E402
from capstone import Cs, CS_ARCH_X86, CS_MODE_64  # noqa: E402
from capstone.x86 import (  # noqa: E402
    X86_OP_IMM, X86_OP_MEM, X86_OP_REG, X86_REG_RIP,
)

FAKE = "—"
OUT_TSV = os.path.join(ROOT, "fixtures", "f3_triage.tsv")

# 预注册正对照（fixtures/f3_triage_plan.txt §4）—— 台账 fixtures/f3_heavy_anchors.tsv 逐行
CTRL_STD = """0x41a9d0 0x41ab40 0x41b500 0x41b720 0x41b880 0x41bdf0
0x4600e0 0x4602f0 0x481140 0x481870 0x4825b0 0x482c20""".split()
CTRL_GDEXT = """0x3aa6a0 0x3aa820 0x3b56c0 0x3b5910 0x3b5ce0
0x3b6e00 0x3b6ed0 0x3b7020 0x426430 0x433c00 0x4dec00 0x4e3b00 0x4e3c80 0x4e8f80
0x4e91d0 0x4e9380 0x4e9590 0x500070 0x5005b0""".split()
# G4（源码闭包）按预注册判 unknown —— 这三条是 T71 记为"源码闭包、父函数无真名"的
CTRL_GDEXT_CLOSURE = {"0x4dec00", "0x500070", "0x5005b0"}
CTRL_REAL = ["0x3afcf0", "0x3b4ec0"]              # T71 落码的两条业务函数
CTRL_REAL_EXTRA = ["0x519630", "0x519840", "0x4ee720"]  # 已知 F1 真业务（防"全判 na"）
# §10 E9-2：3 条旧 G4 闭包样本在 T87 定名后升 F1、已不在 F3 集合 ⇒ 补一条**活着的** G4 正对照：
# `0x4eb94b` 的 Location `src/math_function.rs:184:24` 被 ≥2 条**无名** F3 行共享（G4 的机判形式），
# 且它不在 F3 的 machine 路径上被任何更强规则抢走 ⇒ 必须仍判 `unknown`。
CTRL_G4_LIVE = ["0x4eb94b"]
CTRL_STD_SIZES = [0x164, 0x158, 0x564, 0x100, 0x721, 0x66f]  # 台账：每对逐字节相同

PREFIX = {
    "lock cmpxchg": "ATOMIC", "ucomisd": "CMP", "self-call": "SELF",
    "stride": "STRIDE", "bulk": "BULK", "cmov": "CMOV", "borrow": "BORROW",
    "slot": "SLOT", "pair": "PAIR", "loc": "LOC", "caller-loc": "CALLER_LOC",
}


# ---------------------------------------------------------------------------
# 特征提取
# ---------------------------------------------------------------------------
class Feat:
    __slots__ = (
        "va", "start", "end", "size", "n_insn", "self_calls", "strides",
        "ucomisd", "jp", "bulk", "cmov", "atomics", "borrow", "locs",
        "callers", "slots", "lea_refs", "caller_loc_n", "callees",
        "n_direct_call", "n_plt", "strings", "cmpsd", "ret_like",
        "first", "last_ops",
        # §10 勘误 E6/E7：调用形态（本轮新增）
        "indirect", "slot_addrs", "slot_ref_sites",
    )

    # 调用形态四档（预注册 §10 E8）
    @property
    def callform(self):
        if self.callers >= 1:
            return "direct"
        if self.indirect >= 1:
            return "indirect"
        if self.slots >= 1:
            return "handed-out"
        return "dead"


def secname(ctx, va):
    for s in ctx.elf.secs:
        if s["size"] and s["addr"] <= va < s["addr"] + s["size"]:
            return s["name"]
    return None


def is_writable_global(ctx, va):
    n = secname(ctx, va)
    return n in (".data", ".bss", ".got", ".data.rel.ro")


def locs_of_range(ctx, lo, hi):
    """该范围引用的 panic::Location（按 file 聚合，保留出现顺序）。"""
    votes = collections.Counter()
    seen = {}
    for ins in ap.dis_list(ctx, lo, hi):
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                t = ins.address + ins.size + op.mem.disp
                loc = ap.maybe_location(ctx, t)
                if loc:
                    votes[loc] += 1
                    seen.setdefault(loc, ins.address)
    return votes, seen


def strings_of_range(ctx, lo, hi, cap=6):
    out = []
    for ins in ap.dis_list(ctx, lo, hi):
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                t = ins.address + ins.size + op.mem.disp
                if t in ctx.rel or ap.maybe_location(ctx, t):
                    continue
                s = ctx.cstr(t)
                if len(s) >= 6 and all(0x20 <= c < 0x7F for c in s):
                    txt = s.decode("ascii")
                    if txt not in out:
                        out.append(txt)
    return out[:cap]


def features(ctx, va, index):
    f = Feat()
    f.va = va
    ex = ctx.fde_extent(va)
    if not ex:
        f.start = f.end = va
        f.size = 0
        f.indirect = 0
        f.slot_addrs = []
        f.slot_ref_sites = []
        return f
    f.start, f.end = ex
    f.size = ex[1] - ex[0]
    insns = ap.dis_list(ctx, f.start, f.end)
    f.n_insn = len(insns)
    f.self_calls = 0
    f.strides = []
    f.ucomisd = 0
    f.jp = 0
    f.bulk = 0
    f.cmov = 0
    f.cmpsd = 0
    f.atomics = []
    f.borrow = []
    f.callers = 0
    f.slots = 0
    f.lea_refs = 0
    f.n_direct_call = 0
    f.n_plt = 0
    f.callees = []
    f.ret_like = False
    f.first = [(i.mnemonic, i.op_str) for i in insns[:3]]
    f.last_ops = [(i.mnemonic, i.op_str) for i in insns[-2:]]

    call_imm, rip, imm = index

    for k, ins in enumerate(insns):
        m = ins.mnemonic
        ops = ins.operands
        # 自递归 / 直接调用
        if m == "call":
            if ops and ops[0].type == X86_OP_IMM:
                tgt = ops[0].imm & 0xFFFFFFFFFFFFFFFF
                f.n_direct_call += 1
                if tgt == f.start:
                    f.self_calls += 1
                else:
                    f.callees.append(tgt)
                    sn = secname(ctx, tgt)
                    if sn in (".plt", ".plt.sec", ".plt.got"):
                        f.n_plt += 1
        # 元素步长
        if m in ("imul", "mul") and len(ops) == 3 and ops[2].type == X86_OP_IMM:
            v = ops[2].imm
            if 0x20 <= v <= 0x10000:      # 除法魔数（0xaaaaaaab 之类）不算元素步长
                f.strides.append(v)
        if m.startswith("ucomis") or m in ("comisd", "comiss"):
            f.ucomisd += 1
            f.cmpsd += 1
        if m in ("jp", "jpe", "jnp", "jpo"):
            f.jp += 1
        if m in ("movups", "movaps", "movdqu", "movdqa", "vmovups", "vmovaps"):
            f.bulk += 1
        if m.startswith("cmov"):
            f.cmov += 1
        # ATOMIC 懒初始化单例
        if m.endswith("cmpxchg") and 240 in (getattr(ins, "prefix", None) or ()):  # 0xf0 = LOCK
            for op in ops:
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    t = ins.address + ins.size + op.mem.disp
                    if is_writable_global(ctx, t) and t not in f.atomics:
                        f.atomics.append(t)
        # BORROW：借用标记 0 -> -1 写在 [reg+0x10]
        if m == "mov" and len(ops) == 2 and ops[0].type == X86_OP_MEM and ops[1].type == X86_OP_IMM:
            if ops[1].imm & 0xFFFFFFFFFFFFFFFF == 0xFFFFFFFFFFFFFFFF and ops[0].mem.disp == 0x10:
                f.borrow.append(ins.address)

    if f.n_insn <= 3 and all(x[0] in ("ret", "xor", "nop", "push", "pop", "mov") for x in f.first):
        f.ret_like = f.n_insn <= 3 and insns and insns[-1].mnemonic in ("ret", "jmp")

    # Location / 串
    votes, seen = locs_of_range(ctx, f.start, f.end)
    f.locs = [(loc, n) for loc, n in votes.most_common()]
    f.strings = strings_of_range(ctx, f.start, f.end)

    # 调用点 / 数据表槽
    f.callers = len(call_imm.get(f.start, ()))
    f.slots = len([k for k, v in ctx.rel.items() if v == f.start])
    f.lea_refs = len(rip.get(f.start, ())) + len(imm.get(f.start, ()))

    # §10 勘误 E6：**间接调用**（经 GOT/数据槽的 `call|jmp qword ptr [rip+槽]`）。
    # 为什么需要它：预注册正的 12 条 std 正对照里 `0x4825b0/0x482c20` 的 `callers(rel32)==0`、
    # `slots==1`，但它们的槽（`0x9bbf40`/`0x9bbf38`）被 5 条 `call qword ptr [rip+…]` **间接调用**
    # （调用点在 `graphwar_function_evolutionator` 的排序实例里）⇒ 只看 rel32 会误判成"没人调用"。
    f.slot_addrs = sorted(k for k, v in ctx.rel.items() if v == f.start)
    f.slot_ref_sites = []
    f.indirect = 0
    for s in f.slot_addrs:
        for site in sorted(rip.get(s, ())):
            one = ap.dis_list(ctx, site, site + 8)
            if not one:
                continue
            ins0 = one[0]
            if ins0.mnemonic not in ("call", "jmp"):
                continue
            if not ins0.operands or ins0.operands[0].type != X86_OP_MEM:
                continue
            if ins0.operands[0].mem.base != X86_REG_RIP:
                continue
            f.indirect += 1
            f.slot_ref_sites.append(site)

    # 唯一/主要调用者自己引用 Location 吗（=0 ⇒ 调用者是 std/gdext/引擎侧）
    sites = sorted(call_imm.get(f.start, ()))
    f.caller_loc_n = -1
    if len(sites) == 1:
        cex = ctx.fde_extent(sites[0])
        if cex:
            cv, _ = locs_of_range(ctx, cex[0], cex[1])
            f.caller_loc_n = len(cv)
    return f



# ---------------------------------------------------------------------------
# 人工裁决表（预注册 §3 纪律 3：机器规则只是**候选**，类别是人工裁决结果）
# 键 = ELF vaddr；值 = (最终类别, 改判理由)。未列的条目用机器候选。
# 每条改判都能否掉一条机器规则，理由里写否定它的**具体指令/尺寸/名字来源**。
# ---------------------------------------------------------------------------
MANUAL = {
    # --- R-STD 误判：0x308d 字节的业务函数恰好有 ucomisd/jp/movups，中了 B 组 ---
    "0x3c2bc0": ("real-missing",
                 "R-STD-B 误判：size 0x308d/2624 insn（本批最大），STRIDE=0x3e8=1000=画布宽（业务常量），"
                 "v2 真名 GraphwarShotCalculator::calculate_shot_results，Location 跨 obstacle_manager/shot_calculator"
                 " ⇒ 业务函数，不是排序单态化"),
    # --- G3 误判：唯一调用者没有 Location 也可能是线程闭包/虚调用，不是 gdext 回调 ---
    "0x44e8f0": ("real-missing",
                 "G3 误判：v2 真名 GameServerData::subscribe_to_room_info_updates::{{closure}}，size 0x8f9、"
                 "有分支与多处业务调用 ⇒ 业务闭包，不是 #[func] 分发壳"),
    "0x44f990": ("real-missing",
                 "G3 误判：v2 真名 GraphwarGameServer::start（boundary 锚行 130），size 0x72c ⇒ 业务函数"),
    "0x4661a0": ("real-missing",
                 "G3 误判：v2 真名 GraphwarLevelGenerator::generate_level::{{closure}}（锚行 94,95,96,102,111），"
                 "size 0xa75 ⇒ 业务闭包（关卡生成），不是分发壳"),
    "0x51d080": ("real-missing",
                 "G3 误判：v2 真名 GraphwarFunctionEvolutionator::calculate_functions_old（T71 §123.8 已定性："
                 "变体 B 的 (1+λ) 爬山，6918 字节）⇒ 业务函数，只是没读完"),
    # --- G2 误判：RefCell 借用 + 字段写/读 之外还有业务动作 ---
    "0x477fa0": ("real-missing",
                 "G2 误判：写 [+0x11e] 布尔字段后又 emit 信号（call 0x3d3530，打包 tag 0x8000000000000045）"
                 " ⇒ 手写 setter（带副作用），不是 gdext 生成的纯属性包装"),
    "0x478010": ("real-missing",
                 "G2 误判：把 24 字节结构体拷进 [+0]/[+0x10] 后借 [+0xd8] 并 emit 信号（tag …1d）"
                 " ⇒ 手写 setter，不是纯属性包装"),
    "0x4764b0": ("real-missing",
                 "G2 误判：先判 [+0x120]==1，再 get_module(0x46e140)，再做 0x2010003>>shift 的位运算 ⇒ 业务 getter"),
    "0x4765f0": ("real-missing",
                 "G2 误判：同 0x4764b0 形态（判 [+0x120]、get_module、位运算）⇒ 业务 getter"),
    "0x477410": ("real-missing",
                 "G2 误判：同 0x4764b0 形态 ⇒ 业务 getter"),
    "0x4737a0": ("real-missing",
                 "G2 误判：size 0x214、call 0x46d8c0 + 0x3bea40 多次、多分支 ⇒ 业务方法"),
    "0x473e50": ("real-missing",
                 "G2 误判：size 0x343、call 0x46e7a0/0x3c2120/0x3c17d0/0x46dd00/0x464d10/0x46e580 ⇒ 业务方法"),
    "0x475590": ("real-missing",
                 "G2 误判：size 0x316、先判 [+0x120] 再 call 0x46d8c0/0x47dec0 ⇒ 业务方法"),
    "0x3d4fa0": ("real-missing",
                 "G2 误判：v2 真名 GraphwarGameRecorder::save_event，size 0x2b8、165 条指令、6 个业务调用 ⇒ 业务方法"),
    # --- G2 命中但判不了：信号 connect/emit 闭包体，父函数不可定 ---
    "0x4c5c00": ("unknown",
                 "G2 命中但判不了：整函数只有「借 self → 取 [+0x1f8] 模块 → 借它 → emit 信号(tag …10) → 释放」，"
                 "Location 在 graphwar_setup_control.rs:270:72（另一模块的行列）⇒ 信号闭包体；"
                 "缺：父函数是谁（.gd/.tscn 的 connect 站点）"),
    "0x4c5cb0": ("unknown", "同 0x4c5c00（Location setup_control.rs:277:72、size/形态逐条相同）"),
    "0x4c5d60": ("unknown", "同 0x4c5c00（Location setup_control.rs:284:72、size/形态逐条相同）"),
    "0x5153f0": ("unknown",
                 "G2 命中但判不了：借 [+0x30] 后 call 0x47dec0 + 一串位运算；Location graphwar_backend.rs:359:49；"
                 "缺：它是 backend 的闭包体还是独立方法（符号表无名字）"),
    "0x520100": ("unknown", "同 0x5153f0（与它同处 .data.rel.ro 表 0x961fc0/0x961fc8 相邻两项）"),
    # --- 泛型单态化：7 条同尺寸同 Location=1 个源码 fn 的实例 ---
    **{a: ("na:other",
           "我方源码层是**一个泛型 fn**（hyb 名 GraphwarNodes::get_node_by_name）：7 条 FDE 尺寸全 0xf9、"
           "**字节级同体**（与 0x3aaea0 逐字节比只有 26 字节不同 = 13 处 rip 相对位移，指向每类型各自的槽）、"
           "引用同一批 `.rodata` 字面量（0x1bd8f2 `upcast_ref` / 0x1bd8fc `Upcast failed…`）、"
           "Location 全是 graphwar_nodes.rs:8:62、调用点数各自不同（9/4/7/5/1/2/1）⇒ 同一泛型 fn 的 7 份单态化实例，"
           "不是 7 个缺口（写 1 个泛型即覆盖）")
       for a in ("0x3aaea0", "0x3ab290", "0x3ab390", "0x3ab490", "0x3ab690", "0x3ab790", "0x3ab890")},
    # --- math_function：地址不是 .eh_frame 函数起点 ---
    "0x4eb86a": ("unknown",
                 "地址 0x4eb86a **不是** .eh_frame 函数起点（落在 FDE 0x4eb830..0x4eb920 内）⇒ 原版边界本身不可判；"
                 "缺：Ghidra funcstart 与 .eh_frame 的边界裁决（v2 名 remove_offset 是对的，但边界是切出来的）"),
    "0x4eb94b": ("unknown",
                 "地址 0x4eb94b **不是** .eh_frame 函数起点（落在 0x4eb920..0x4eb9cb 内），从起点反汇编即错位"
                 "（jno/add r10b/subps）⇒ 边界不可判"),
    "0x4ebb68": ("unknown",
                 "地址 0x4ebb68 **不是** .eh_frame 函数起点（落在 0x4eb9d0..0x4ebbe0 内），反汇编错位 ⇒ 边界不可判"),
    "0x4eccb3": ("unknown",
                 "地址 0x4eccb3 **不是** .eh_frame 函数起点（落在 0x4ec160..0x4ed66e 内，Ghidra 把大函数切了）"
                 "⇒ 边界不可判；缺：Ghidra/.eh_frame 边界裁决"),
    "0x4ebbe0": ("real-missing",
                 "size 0x574 真实函数体（正规 prologue + 分支），Location math_function.rs:184:24；"
                 "机器 G4 是因为它与上面两条无名地址共享同一 Location 才判 unknown，但它自己有名字（hyb:高置信 "
                 "simplify_recursive）⇒ 语义档位仍是推断（名字只是结构指纹），缺口是实的"),
    # === 2026-11-27 T92（`docs/06` §139 ② 硬前置）：把 T89 在 §138.5 的 6 条裁决**补进生成器** ===
    # 为什么必须补：`fixtures/f3_triage.tsv` 是**历史裁决记录**，但重跑本工具会整表重写 ⇒ 这 6 条
    # 之前只活在 tsv 里，一重跑就全被冲掉（§138.9 第 1 条实测：5 条回 real-missing、
    # `0x523410` 整行消失）。下面 6 条的 `依据` 与 §138.5 的表逐条一致，**判据没有放宽**：
    # 每一条都有 Android `.symtab` 的**正向**指名（标准库泛型本体名 + size）+ 调用形态 `call rel32=1/slot=0`。
    # 机器规则判不了它们的原因：内层循环被 LTO 摊平后 A/B/C 三组结构特征不命中（R-STD 是**结构**判据，
    # 这 6 条的结构已经被优化掉，只剩 Android 符号这条**正向**证据）。
    "0x3b6c70": ("na:std-mono",
                 "**人工改判**(T89 §138.5 采纳，2026-11-27；T92 补进 MANUAL)：Android 侧该 Location 键集"
                 "唯一具名命中 `std::thread::local::LocalKey<T>::with`（std，size 0x10c / Linux 0xd0）；"
                 "调用形态 **call rel32=1 / slot=0**（被直接调用、地址不被取走）= 泛型单态化的形态 "
                 "⇒ std 实例，不是游戏缺口；机器 R-STD 未命中是因为内层循环被 LTO 摊平"),
    "0x3bfdd0": ("na:std-mono",
                 "**人工改判**(T89 §138.5 采纳)：Android 命中 `<Vec<T> as SpecFromIterNested<T,I>>::from_iter`"
                 "（alloc，size 0x23c / Linux 0x25b）；call rel32=1 / slot=0 ⇒ alloc 泛型实例"),
    "0x3caa20": ("na:std-mono",
                 "**人工改判**(T89 §138.5 采纳)：Android 命中 `Vec<T,A>::retain`（alloc，0x388 / 0x426）；"
                 "call rel32=1 / slot=0 ⇒ alloc 泛型实例（其 hyb 预测名 request_update_function 是**假预测**）"),
    "0x522070": ("na:std-mono",
                 "**人工改判**(T89 §138.5 采纳)：Android 命中 `Rc<T,A>::new_cyclic_in`（alloc，0x3ec / 0x477）；"
                 "call rel32=1 / slot=0 ⇒ alloc 泛型实例"),
    "0x5238c0": ("na:std-mono",
                 "**人工改判**(T89 §138.5 采纳)：Android 命中 `Rc<T,A>::new_cyclic_in`（alloc，0x2c4 / 0x358）；"
                 "call rel32=1 / slot=0 ⇒ alloc 泛型实例"),
    "0x523410": ("na:std-mono",
                 "**人工改判**(T89 §138.5 **新增行**)：Android 命中 `Rc<T,A>::new_cyclic_in`（alloc，0x208 / 0x2c1）；"
                 "call rel32=1（调用点 0x4c6c50，落在 0x4c6b40 内）/ slot=0 ⇒ alloc 泛型实例。"
                 "机器规则判 na:gdext-glue 只因为**唯一调用者不引用 Location**（CALLER_LOC=0），"
                 "但那不等于 gdext 产物（§138.3 第 1 条：这条陪判据精度极低）。"
                 "⚠️ 顺带纠正 `src/graphwar_replay_control.rs:142-145` 把它注释成 update_replay_list 的后台半边 —— "
                 "注释错，`src/` 不在本工具写范围，只登记"),
    # ---- 下面 5 条 T89 §138.3 的裁决（`na:gdext-glue` → `real-missing`）**不进 MANUAL**，原因可复跑： ----
    #   `0x3aae60 0x3b70b0` 已被 T87 的 Android 定名（`ButtonManager::new::{{closure}}`）抬进 **F2**；
    #   `0x3b6e00 0x3b6ed0 0x3b7020` 被抬进 **F1**（`GraphwarRootNode::ready::{{closure}}`）。
    #   ⇒ 它们**已不在 F3 集合**里，重跑时 `final_class()` 根本查不到它们，写进 MANUAL 也不会有任何效果；
    #     它们的类别仍以 tsv 的历史行为准（重跑时由 `write()` 的「归档」块登记，见 §139 ②）。
    #   复跑核对（5 条都必须是 F1/F2 而非 F3）：
    #     .reference/venv/bin/python tools/f3_triage.py --features 0x3aae60 0x3b6ed0  # 只看特征，不改台账
}

# ---------------------------------------------------------------------------
# 判决（规则见 fixtures/f3_triage_plan.txt §3）
# ---------------------------------------------------------------------------
def g4_closure(row, f, shared_locs):
    """预注册 G4（**勘误 §9** 细化）：同一 `panic::Location` 被 ≥2 条无名 F3 行引用 ⇒
    同一个源码构件（闭包/宏展开）的多个编译产物，**父函数在符号表里没有真名** ⇒ 判不了。

    依据：T71 §123.2 第 4 条对 lan_vbox 的三条（`0x4dec00/0x500070/0x5005b0`）就是这么判的
    —— 它们共享 `src/graphwar_lan_vbox.rs:59:40`，而本模块唯一 v2 锚 `process` 在原版行 90 > 59
    ⇒ 闭包不在任何已命名函数范围内。本函数把"共享同一 Location + 无名"做成机判。
    """
    if not f.locs or not row or not shared_locs:
        return False, ""
    if str(row.get("name_src") or "").startswith("v2"):
        return False, ""
    for loc, _ in f.locs:
        if loc in shared_locs:
            return True, ("Location %s 被 ≥2 条无名 F3 行共享 ⇒ 同一源码构件的多个编译产物，"
                          "父函数无真名" % loc)
    return False, ""


def rule_hits(f, size_freq, row=None, shared_locs=None):
    """返回 (命中列表, 说明 dict)。命中项字符串会被写进 TSV 的「判据命中」列。"""
    hits = []
    cmp_hit = f.ucomisd >= 2 and f.jp >= 2 and bool(f.locs)
    stride_hit = bool(f.strides)
    pair = size_freq.get(f.size, 0) >= 2
    A = f.self_calls >= 2 and stride_hit
    B = cmp_hit and (f.bulk >= 2 or f.cmov >= 4)
    C = cmp_hit and pair
    if f.self_calls >= 2:
        hits.append("SELF=%d" % f.self_calls)
    if stride_hit:
        hits.append("STRIDE=" + ",".join("%#x" % v for v in f.strides[:3]))
    if f.ucomisd >= 2:
        hits.append("CMP=%d/%d" % (f.ucomisd, f.jp))
    if f.bulk >= 2:
        hits.append("BULK=%d" % f.bulk)
    if f.cmov >= 4:
        hits.append("CMOV=%d" % f.cmov)
    if pair:
        hits.append("PAIR=%#x" % f.size)
    if f.atomics:
        hits.append("ATOMIC=%s" % ",".join("%#x" % a for a in f.atomics[:2]))
    if f.borrow:
        hits.append("BORROW=%d" % len(f.borrow))
    if f.slots:
        hits.append("SLOT=%d" % f.slots)
    if f.caller_loc_n == 0:
        hits.append("CALLER_LOC=0")
    g4, g4why = g4_closure(row, f, shared_locs)
    if g4:
        hits.append("G4-CLOSURE")
    # 机器候选
    std = A or B or C
    gdext = []
    if f.atomics and f.callers == 0:
        gdext.append("G1")
    if f.callers == 0 and f.slots >= 1 and 0x78 in f.strides:
        gdext.append("G1b")     # gdext 的 0x78 字节方法/属性记录，只经数据表被引用
    if f.borrow and f.callers <= 1 and f.slots >= 1:
        gdext.append("G2")      # 属性访问器：RefCell 借用 + 本函数地址在访问器表里
    if f.callers == 1 and f.caller_loc_n == 0 and f.size >= 0x180:
        gdext.append("G3")
    other = []
    if f.ret_like:
        other.append("O-ret")
    # §10 勘误 E6：把**调用形态**记进「判据命中」列（新增第四档轴，不改任何既有规则）
    hits.append("CALLFORM=%s" % f.callform)
    if f.indirect and f.callers == 0:
        hits.append("INDIRECT=%d@%#x" % (f.indirect, f.slot_ref_sites[0]))
    return dict(hits=hits, A=A, B=B, C=C, cmp_hit=cmp_hit, pair=pair,
                std=std, gdext=gdext, other=other, g4=g4, g4why=g4why,
                callform_ok=callform_ok(f, A, B, C))


def final_class(addr, f, hits):
    """最终类别 = 人工裁决表优先，否则机器候选。返回 (类别, 机器候选)。"""
    m = machine_class(f, hits)
    if addr in MANUAL:
        return MANUAL[addr][0], m
    return m, m


def machine_class(f, hits, manual=None):
    """机器候选类别；manual 覆盖优先。"""
    if manual:
        return manual
    # 顺序 = 证据强度（预注册 §3 + 勘误 §9）：结构性的 na 证据 > G4 的"判不了"
    if hits["gdext"]:
        return "na:gdext-glue"
    if hits["std"]:
        return "na:std-mono"
    if hits.get("g4"):
        return "unknown"
    if hits["other"]:
        return "na:other"
    return "real-missing"


# ---------------------------------------------------------------------------
# §10 勘误 E6 / E7：「调用形态」闸（2026-11-27 T92；判据在跑数前写死在 plan §10）
# ---------------------------------------------------------------------------
# 起因（T89 `docs/06` §138.5）：`0x438880` 被 T87 的 R3 负向推断判成 `na:std-mono`
# （Android 命中 `std::sys::backtrace::__rust_begin_short_backtrace`），但逐位证据表明
# `0x438896 mov edx,0x1e`(=30) + `0x4388a0 call [slot]`(=TcpStream::connect_timeout)
# ⇒ 它是**游戏 `connect` 的线程闭包体**，只是被整体内联进 std 的 spawn wrapper。
WRAP_FAMILY = ("__rust_begin_short_backtrace", "catch_unwind",
               "thread::spawn", "LangStart")


def callform_ok(f, A, B, C):
    """§10 E6：调用形态是否支持「这一条是泛型单态化实例」。

    预注册判据 = ① `CALLERS(rel32) >= 1`（被直接调用）**或** ② R-STD 结构组 A/B/C 命中。
    * **`indirect` 不算"被调用"**（第一版实现把它算进去了，被 F 组正对照当场抓出：`0x438880` 也是
      `indirect` —— 它的槽 `0x9bb940` 被 std 线程入口 `0x43b070` 用 `call qword ptr [rip+…]` 调用。
      ⇒ 「被间接调用」对 0x438880 与合法单态化**同样成立**，没有区分力，不许当腿）。
    * ② 是"体本身就是算法"的结构证据（`0x438880` 的 self/strides/ucomisd/bulk/cmov **全 0** ⇒ 不中）；
      两条 `CALLERS=0` 的 std 正对照（`0x4825b0/0x482c20`）正是靠 ② 过闸（B+C 命中）。
    """
    return (f.callers >= 1) or A or B or C


def callform_verdict(f, A, B, C):
    """→ (ok, 人可读理由)。判据同 `callform_ok()`。"""
    note = ""
    if f.indirect and f.callers == 0:
        note = ("（附：槽被 `call|jmp qword ptr [rip+槽]` 间接调用 %d 处，首址 %#x —— "
                "**不算**被直接调用，plan §10 E6）" % (f.indirect, f.slot_ref_sites[0]))
    if f.callers >= 1:
        return True, "① CALLERS(rel32)=%d（被直接调用）" % f.callers
    if A or B or C:
        grp = "/".join(g for g, v in (("A", A), ("B", B), ("C", C)) if v)
        return True, "② R-STD 结构组 %s 命中（体本身就是泛型算法）%s" % (grp, note)
    return False, ("① CALLERS=0；② R-STD 结构组 A/B/C **全不中**%s ⇒ "
                   "「标准库单态化实例」这个解释站不住（与 T89 驳回的 `0x438880` 同形）" % note)


def wrap_family_of(android_hits):
    """§10 E7：Android 侧命中的具名函数是否属于 wrapper 家族。

    返回命中的家族名（或空串）。**只认预注册名单里的 4 个**，不按"像不像"自由裁量。
    """
    for h in android_hits:
        for w in WRAP_FAMILY:
            if w in h:
                return w
    return ""


def gate_na_callform(rows):
    """§10 E6 闸：对**分类结果里每一条 `na:std-mono`**（含 MANUAL 裁决）复核调用形态。

    为什么闸放在分类之后而不是 `rule_hits()` 里：机器候选 `std = A∨B∨C`，判据 ② 天然满足
    ⇒ 机判行永远过闸；**真正需要闸的是人工/外部轴（T87 R3）给的 `na:std-mono`**。
    `0x438880` 正是外部轴给的（它从来不在本台账里，在 `function_coverage.py --android-na-report`
    的 21 条候选里），它 `CALLERS=0` 且 A/B/C 全不中 ⇒ 本闸拒。
    → (rows, violations)。violations 非空 ⇒ 调用方**不许出数**（同 `function_coverage.py` 的 M 闸）。
    """
    out, viol = [], []
    for o in rows:
        if o["cat"] != "na:std-mono":
            continue
        f, h = o["f"], o["h"]
        ok, why = callform_verdict(f, h["A"], h["B"], h["C"])
        out.append((o["row"]["addr"], f.callform, ok, why))
        if not ok:
            viol.append((o["row"]["addr"], why))
    return out, viol


# ---------------------------------------------------------------------------
# 输入
# ---------------------------------------------------------------------------
def read_cov(path=os.path.join(ROOT, "fixtures", "function_coverage.tsv")):
    rows = []
    with open(path, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("#") or not ln.strip():
                continue
            p = ln.rstrip("\n").split("\t")
            if len(p) < 13 or not p[0].startswith("0x"):
                continue
            rows.append(dict(addr=p[0], name=p[1], name_src=p[2], module=p[3],
                             ours=p[4], bucket=p[6], sub=p[7]))
    return rows


def read_boundary(path=os.path.join(ROOT, "fixtures", "boundary_audit.tsv")):
    out = {}
    with open(path, encoding="utf-8") as fh:
        hdr = None
        for ln in fh:
            if ln.startswith("#"):
                continue
            p = ln.rstrip("\n").split("\t")
            if hdr is None:
                hdr = p
                continue
            if not p or not p[0].startswith("0x"):
                continue
            d = dict(zip(hdr, p))
            out[d["addr"]] = d
    return out


# ---------------------------------------------------------------------------
# 输出
# ---------------------------------------------------------------------------
def fmt_feat(f, h):
    bits = []
    bits.append("size=%#x/%d insn" % (f.size, f.n_insn))
    if h["hits"]:
        bits.append("|".join(h["hits"]))
    if f.locs:
        bits.append("LOC=" + ";".join("%s x%d" % (l, n) for l, n in f.locs[:2]))
    bits.append("callers=%d" % f.callers)
    if f.caller_loc_n >= 0:
        bits.append("caller_loc=%d" % f.caller_loc_n)
    if f.strings:
        bits.append("str=" + " / ".join(f.strings[:2]))
    return " ".join(bits)


def evidence(f, h, row, bnd):
    """人可读的「依据」：机器特征 + Location + 归属。"""
    bits = []
    if h["A"]:
        bits.append("三段自递归 %d 次 + 元素步长 %s" %
                    (f.self_calls, ",".join("%#x" % v for v in f.strides[:2])))
    if h["B"]:
        bits.append("比较器 ucomisd×%d/jp×%d + %s" %
                    (f.ucomisd, f.jp, ("整块搬运×%d" % f.bulk) if f.bulk >= 2 else ("选择链 cmov×%d" % f.cmov)))
    if h["C"]:
        bits.append("比较器 + FDE 尺寸 %#x 与本批另一条逐字节相同（成对单态化）" % f.size)
    if h["gdext"]:
        if "G1" in h["gdext"]:
            bits.append("lock cmpxchg 写入可写全局 %s + 全 .text 零 call 调用点（只经数据表）"
                        % ",".join("%#x" % a for a in f.atomics[:2]))
        if "G1b" in h["gdext"]:
            bits.append("零 call 调用点 + 地址在数据表 (%d 槽) + 0x78 字节记录追加（gdext 方法/属性表）"
                        % f.slots)
        if "G2" in h["gdext"]:
            bits.append("借用标记 [reg+0x10] 0→-1 共 %d 处（属性访问器形态）" % len(f.borrow))
        if "G3" in h["gdext"]:
            bits.append("唯一调用者不引用任何 Location（gdext ptrcall/call_func 注册回调）")
    if f.locs:
        bits.append("Location=" + ";".join("%s×%d" % (l, n) for l, n in f.locs[:2]))
    if f.callees:
        bits.append("被调者 %d 个（%s…）" % (len(f.callees), ",".join("%#x" % c for c in f.callees[:3])))
    biz = [t for t in f.strings if "res://" in t]
    if biz:
        bits.append("业务串=" + biz[0][:70])
    elif f.strings:
        bits.append("串=" + f.strings[0][:40])
    if row and row.get("name_src"):
        bits.append("名字来源=%s" % row["name_src"])
    if bnd and bnd.get("loc_main_lines"):
        bits.append("边界锚行=%s" % bnd["loc_main_lines"][:40])
    return "；".join(bits)


def mk_ctx():
    return ap.Ctx(ap.SO_DEFAULT)


def shared_locs_of(f3, feats):
    """被 ≥2 条**无名**（非 v2 真名）F3 行引用的 Location（判 G4 用）。"""
    owners = collections.defaultdict(set)
    for r in f3:
        if str(r.get("name_src") or "").startswith("v2"):
            continue
        for loc, _ in feats[r["addr"]].locs:
            owners[loc].add(r["addr"])
    return {loc for loc, s in owners.items() if len(s) >= 2}


def mod_v2_min_lines(cov, bnd):
    """每个模块里 **v2 真名**函数的最小 Location 行号（判 G4 的"父函数有没有真名"用）。"""
    out = {}
    for r in cov:
        if not str(r.get("name_src") or "").startswith("v2"):
            continue
        b = bnd.get(r["addr"])
        if not b or not b.get("loc_main_lines"):
            continue
        mod = os.path.basename(str(r.get("module") or ""))
        for x in str(b["loc_main_lines"]).split(","):
            x = x.strip()
            if not x.isdigit():
                continue
            n = int(x)
            if mod not in out or n < out[mod]:
                out[mod] = n
    return out


def build(verbose=True):
    ctx = mk_ctx()
    index = ctx.index()
    cov = read_cov()
    bnd = read_boundary()
    f3 = [r for r in cov if r["bucket"] == "F3"]
    f2 = [r for r in cov if r["bucket"] == "F2"]
    if verbose:
        print("F3=%d F2=%d（分母 %d）" % (len(f3), len(f2), len(cov)))
    feats = {}
    for r in cov:
        feats[r["addr"]] = features(ctx, int(r["addr"], 16), index)
    size_freq = collections.Counter(feats[r["addr"]].size for r in cov)
    return (ctx, cov, bnd, f3, f2, feats, size_freq,
            shared_locs_of(f3, feats), mod_v2_min_lines(cov, bnd))


def main(argv=None):
    ap_ = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap_.add_argument("--features", nargs="+", metavar="VADDR")
    ap_.add_argument("--control", action="store_true")
    ap_.add_argument("--stats", action="store_true")
    ap_.add_argument("--na-rescan", dest="na_rescan", action="store_true",
                     help="§10 E8：全量重扫 fixtures/f3_triage.tsv 的 na:* 行（调用形态 + Android 命中家族）")
    ap_.add_argument("--json", metavar="PATH")
    args = ap_.parse_args(argv)

    if args.features:
        ctx = mk_ctx()
        index = ctx.index()
        for a in args.features:
            va = int(a, 16)
            f = features(ctx, va, index)
            print("== %s  FDE %#x..%#x size %#x  insn %d" % (a, f.start, f.end, f.size, f.n_insn))
            print("   first: " + " ; ".join("%s %s" % (m, o) for m, o in f.first))
            print("   self_calls=%d strides=%s ucomisd=%d jp=%d bulk=%d cmov=%d" %
                  (f.self_calls, [hex(v) for v in f.strides], f.ucomisd, f.jp, f.bulk, f.cmov))
            print("   atomics=%s borrow=%d slots=%d callers=%d lea_refs=%d caller_loc=%d"
                  % ([hex(x) for x in f.atomics], len(f.borrow), f.slots, f.callers,
                     f.lea_refs, f.caller_loc_n))
            print("   locs=%s" % (["%s x%d" % (l, n) for l, n in f.locs[:4]]))
            print("   strings=%s" % f.strings)
            print("   callees=%s" % (["%#x" % c for c in f.callees[:10]]))
        return 0

    ctx, cov, bnd, f3, f2, feats, size_freq, shared_locs, _mv2 = build()
    rows = []
    for r in f3:
        f = feats[r["addr"]]
        h = rule_hits(f, size_freq, r, shared_locs)
        rows.append((r, f, h))

    if args.control:
        return control(rows, feats, size_freq, shared_locs)

    # ---- 全量：落 fixtures/f3_triage.tsv + 统计 -------------------------------
    out = []
    for r, f, h in rows:
        cat, mach = final_class(r["addr"], f, h)
        out.append(dict(row=r, f=f, h=h, cat=cat, mach=mach))

    # §10 E6 闸：na:std-mono 的调用形态必须先过闸，否则**不许出数**（同 function_coverage 的 M 闸）
    gate_rows, viol = gate_na_callform(out)
    print("\n=== §10 E6 闸：na:std-mono 的调用形态复核（%d 条）" % len(gate_rows))
    for a, cf, ok, why in gate_rows:
        print("  %-4s %-10s CALLFORM=%-11s %s" % ("✓" if ok else "✗", a, cf, why))
    if viol:
        print("✗ R-CALLFORM 闸不通过（%d 条）⇒ 按预注册 §10 E6 **拒绝出数**：" % len(viol))
        for a, why in viol:
            print("   %s %s" % (a, why))
        return 2

    if args.na_rescan:
        return cmd_na_rescan(cov, feats)
    if args.stats:
        return stats(out)
    return write(out, args.json)


# ---------------------------------------------------------------------------
# §10 勘误 E8：全量重扫 na 台账（本轮交付物）
# ---------------------------------------------------------------------------
RESCAN_OUT = os.path.join(ROOT, "fixtures", "f3_triage_na_rescan.tsv")


def read_ledger_main(path=OUT_TSV):
    """读台账**主表**（读到 `# ----` 分节横幅即停，F2-scan 不进）。→ [(addr, name, size, cat, ev)]"""
    out = []
    if not os.path.exists(path):
        return out
    with open(path, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("# ----"):
                break
            if ln.startswith("#") or not ln.strip():
                continue
            c = ln.rstrip("\n").split("\t")
            if c[0] == "vaddr" or not c[0].startswith("0x"):
                continue
            out.append(c)
    return out


def android_hit_names():
    """→ {linux_va: [Android 侧命中的具名函数 demangled…]}（用 T87 的特征缓存，按 Location 键集）。

    依赖 `tools/naming_android.py` 的 `load_cache()`；缓存不可用就返回 `{}` 并打印说明
    （**不让 Android 轴缺失变成静默的空值**：调用方会把该列写成「Android 轴不可用」）。
    """
    try:
        import naming_android as NA
        data = NA.load_cache()
        ax = NA.Axes(data)
    except Exception as e:                                  # noqa: BLE001
        print("  ⚠ Android 轴不可用（%s: %s）⇒ 该列留空，E7 只能靠 Location 判" %
              (type(e).__name__, e))
        return None
    out = {}
    for va, L in ax.lin.items():
        top, _ = ax.l_candidates(L)
        names = []
        for a in top[:2]:
            for r in ax.agn.get(a, ()):
                names.append(r["demangled"])
        out[va] = names
    return out


def cmd_na_rescan(cov, feats):
    """§10 E8：把台账主表里**全部 `na:*` 行**逐条按 E6/E7 重扫，写
    `fixtures/f3_triage_na_rescan.tsv`。

    这是本轮的交付物：回答「还有多少条是**被内联进 std wrapper 的游戏闭包**被误判成 na:*」。
    """
    bucket = {r["addr"]: r["bucket"] for r in cov}
    name = {r["addr"]: r["name"] for r in cov}
    led = read_ledger_main()
    nas = [c for c in led if c[4].startswith("na:")]
    print("\n=== §10 E8 全量重扫 na 台账：主表 %d 行，其中 na:* **%d 条**"
          % (len(led), len(nas)))
    andn = android_hit_names()
    lines = []
    lines.append("# f3_triage_na_rescan.tsv —— §10 E8（T92）：`fixtures/f3_triage.tsv` 主表 **全部 na:* 行**"
                 "按预注册 §10 的 E6/E7 重扫")
    lines.append("# 判据 = fixtures/f3_triage_plan.txt §10（跑数前写死）。地址 = ELF vaddr。")
    lines.append("# 形态四档：direct=CALLERS(rel32)>=1 / indirect=CALLERS=0 但槽被 call|jmp [rip+槽] 调用 / "
                 "handed-out=CALLERS=0 且槽只被取走 / dead=谁都不引用")
    lines.append("# E6 = na:std-mono 必须「CALLERS(rel32)>=1 ∨ R-STD 结构组命中」（indirect **不算**，见 plan §10 E11）；"
                 "E7 = wrapper 家族命中 + "
                 "Linux 自身 Location 含 src/graphwar_*.rs ⇒ 游戏闭包被内联进 foreign wrapper ⇒ 不许 na")
    lines.append("vaddr\t台账类别\tFDE尺寸\tCALLERS\tINDIRECT\tSLOTS\t形态\tR-STD\tG命中\t"
                 "Linux自身Location\tAndroid命中符号\t复核触发\t裁决\t依据")
    n_keep = n_flip = n_review = 0
    shape_of = []          # [(addr, 台账类别, 形态)]
    for c in nas:
        addr, cat, sz = c[0], c[4], c[2]
        f = feats.get(addr)
        if f is None:
            lines.append("\t".join([addr, cat, sz, "—", "—", "—", "—", "—", "—", "—",
                                    "不在 527 全集", "—", "unknown", "无特征"]))
            continue
        r = {"addr": addr, "name_src": "", "module": ""}
        h = rule_hits(f, collections.Counter(), r, None)
        grp = "/".join(g for g, v in (("A", h["A"]), ("B", h["B"]), ("C", h["C"])) if v) or "—"
        locs = ";".join("%s×%d" % (l, n) for l, n in f.locs[:2]) or "—"
        ah = andn.get(int(addr, 16), []) if andn is not None else []
        ahs = " | ".join(sorted({a.split("::h")[0] for a in ah})[:2]) or \
            ("—" if andn is not None else "（Android 轴不可用）")
        game_locs = [l for l, _ in f.locs if l.startswith("src/")]
        triggers, verdict, why = [], "维持 na", []
        if cat == "na:std-mono":
            ok, w = callform_verdict(f, h["A"], h["B"], h["C"])
            why.append(w)
            if not ok:
                triggers.append("E6")
                verdict = "**不许 na**（E6 拒）"
        wfam = wrap_family_of(ah)
        if wfam:
            triggers.append("E7:%s" % wfam)
            if game_locs:
                verdict = "**不许 na**（E7 拒：游戏闭包被内联进 foreign wrapper）"
                why.append("wrapper 家族 `%s` 命中 + Linux 自身 Location 有游戏文件 %s ⇒ 体是游戏源码"
                           % (wfam, game_locs[:2]))
            else:
                why.append("wrapper 家族 `%s` 命中但 Linux 自身 Location 全在非游戏文件 ⇒ wrapper 本体实例，na 成立"
                           % wfam)
        elif andn is not None and not ah:
            triggers.append("E7:无Android证据")
            why.append("Android 侧无具名命中 ⇒ E7 不适用（本行的 na 来自机器结构规则，不是 R3 负向推断）")
        if not why:
            why.append("E6 不适用（本行不是 na:std-mono）；E7 未触发（Android 命中不在 wrapper 家族 "
                       "⇒ 不是「被内联进 foreign wrapper 的游戏闭包」）；本行的 na 来自机器结构规则，维持")
        if verdict.startswith("维持"):
            n_keep += 1
        elif "不许 na" in verdict:
            n_flip += 1
        if triggers:
            n_review += 1
        shape_of.append((addr, cat, f.callform))
        lines.append("\t".join([addr, cat, sz, str(f.callers), str(f.indirect), str(f.slots),
                                f.callform, grp, ("|".join(h["gdext"]) or "—"), locs, ahs,
                                (",".join(triggers) or "—"), verdict,
                                ("；".join(why)).replace("\t", " ")]))
    # ---- 正对照：T89 驳回的那一条（不在台账里，但闸必须拒它）----
    ctl = "0x438880"
    f = feats.get(ctl)
    if f is not None:
        h = rule_hits(f, collections.Counter(), {"addr": ctl, "name_src": "naming_android", "module": ""}, None)
        ok, w = callform_verdict(f, h["A"], h["B"], h["C"])
        ah = (andn or {}).get(int(ctl, 16), [])
        wfam = wrap_family_of(ah)
        game_locs = [l for l, _ in f.locs if l.startswith("src/")]
        print("  正对照 %s：CALLFORM=%s INDIRECT=%d R-STD=%s ⇒ E6 %s；Android 家族=%s + 游戏 Location=%s ⇒ E7 %s"
              % (ctl, f.callform, f.indirect,
                 "/".join(g for g, v in (("A", h["A"]), ("B", h["B"]), ("C", h["C"])) if v) or "全不中",
                 "✓ 拒（na 不成立）" if not ok else "✗ 竟然通过",
                 wfam or "—", bool(game_locs),
                 "✓ 拒（na 不成立）" if (wfam and game_locs) else "✗"))
        if ok:
            print("  ✗ 正对照失败 ⇒ 闸没挡住 T89 已驳回的那条，先修工具")
            return 2
    open(RESCAN_OUT, "w", encoding="utf-8").write("\n".join(lines) + "\n")
    print("  写出 %s（%d 行）" % (os.path.relpath(RESCAN_OUT, ROOT), len(nas)))
    print("  裁决：维持 na **%d** 条 / **不许 na（改判）%d** 条 / 触发复核 %d 条"
          % (n_keep, n_flip, n_review))
    shp = collections.Counter(x[2] for x in shape_of)
    print("  调用形态分布（%d 条）：%s" % (len(shape_of), dict(shp)))
    for k in ("direct", "indirect", "handed-out", "dead"):
        if k in shp:
            bycat = collections.Counter(cat for _a, cat, cf in shape_of if cf == k)
            print("    %-11s %3d 条（%s）"
                  % (k, shp[k], ", ".join("%s=%d" % (c, n) for c, n in sorted(bycat.items()))))
    return 0


def _cat_key(o):
    order = {"real-missing": 0, "na:std-mono": 1, "na:gdext-glue": 2,
             "na:other": 3, "unknown": 4}
    return (order.get(o["cat"], 9), int(o["row"]["addr"], 16))


def write(out, json_path=None):
    # §10 E12（落地勘误）：台账 `name` 列必须取 **boundary_audit.tsv 的名字**，不能取
    # `function_coverage.tsv` 的「符号名」。原因：`tools/function_coverage.py` 的 M7 闸要求
    # 「台账 name == boundary name」，而这两列的来源口径不同 —— boundary 用 v2 `role=outer` 身份
    # （T89 裁决后一个地址一个名字），function_coverage 的符号名列在 5 个地址上还带 `::{{closure}}`
    # （例 `0x4c5c00` boundary `connect_entry_buttons` vs cov `connect_entry_buttons::{{closure}}`）。
    # 旧台账（T82 手工写的）用的是 boundary 名，所以 M7 一直是绿的；本轮改为重跑生成，
    # 若照抄 cov 的名字就会一次造成 5 条 M7 假违例 ⇒ 这里显式取 boundary 名。
    bnames = {a: d.get("name") for a, d in read_boundary().items()}
    lines = []
    lines.append("# f3_triage.tsv —— F3 缺口**逐条**分类（度量口径审计，只审计不改码）")
    lines.append("# 判据 = **预注册** fixtures/f3_triage_plan.txt（先提交后跑）+ 同文件 §9/§10 勘误（追加式）")
    lines.append("# 复跑：PY=.reference/venv/bin/python ; $PY tools/f3_triage.py --control   # 先验判法（rc=0 才许信）")
    lines.append("#       $PY tools/f3_triage.py                                             # 重写本文件 + 打印统计")
    lines.append("# 口径：地址 = ELF vaddr（RVA）；FDE 尺寸 = .eh_frame 自解；归属 = panic::Location；")
    lines.append("#       **机器规则只是候选，「类别」列是人工裁决结果**（改判的条目在「依据」里写明否定机器候选的理由）")
    lines.append("# 类别：real-missing=真的缺（进诚实缺口）/ na:std-mono=std 单态化 / na:gdext-glue=gdext 宏产物")
    lines.append("#       na:other=其它不该手写（含泛型单态化实例）/ unknown=判不了（计入缺口，保守）")
    lines.append("# §10 新增：`判据命中` 列带 CALLFORM=<direct|indirect|handed-out|dead>（调用形态）")
    lines.append("#   定义：direct=CALLERS(rel32)>=1；indirect=CALLERS=0 但槽被 call|jmp [rip+槽] 调用；")
    lines.append("#         handed-out=CALLERS=0 且槽只被取走；dead=谁都不引用。判据见 plan §10 E6/E8。")
    lines.append("# 列：vaddr | 名字（function_coverage.tsv 的符号名） | FDE尺寸 | 判据命中 | 类别 | 依据")
    lines.append("vaddr\tname\tsize\thits\tcategory\tevidence")
    for o in sorted(out, key=_cat_key):
        r, f, h = o["row"], o["f"], o["h"]
        hits = "|".join(h["hits"]) if h["hits"] else "—"
        if o["cat"] != o["mach"]:
            hits += " ⇒机器候选=%s" % o["mach"]
        ev = evidence(f, h, r, None)
        if r["addr"] in MANUAL:
            ev = (ev + "；" if ev else "") + "**人工改判**：" + MANUAL[r["addr"]][1]
        nm = bnames.get(r["addr"]) or r["name"]
        lines.append("\t".join([r["addr"], nm or FAKE, "%#x" % f.size,
                                hits, o["cat"], ev.replace("\t", " ")]))
    # ---- F2 顺带扫描（不进 153 的算术，预注册 §5） ----
    lines.append("")
    lines.append("# ---- F2-scan：F2（有实现无物证）97 条的同一套规则扫描 —— **单独计数，不进上面 153 的算术** ----")
    lines.append("# 意义：F2 若也大量是 na，则 M2=F1+F2 同样虚高；命中不等于改判 F2 的档位（那是 function_coverage 的事）")
    lines.append("vaddr\tname\tsize\thits\tcategory_machine\tevidence")
    ctx, cov, bnd, f3, f2, feats, size_freq, shared, mv2 = build(verbose=False)
    n_f2_na = collections.Counter()
    for r in f2:
        f = feats[r["addr"]]
        h = rule_hits(f, size_freq, r, shared)
        cat = machine_class(f, h)
        n_f2_na[cat] += 1
        if cat.startswith("na:") or cat == "unknown":
            lines.append("\t".join([r["addr"], r["name"] or FAKE, "%#x" % f.size,
                                    "|".join(h["hits"]) or "—", cat,
                                    evidence(f, h, r, None).replace("\t", " ")]))
    # ---- §10 E10：历史行归档（台账不是 F3 的派生视图，是历史裁决记录）----
    # 把「上一版台账主表里有、本轮重跑后已不属于 F3」的行写成**注释行**（不参与任何机读计数）。
    # 归档行自己也读回来再去重 ⇒ 反复重跑不丢历史。
    now = {o["row"]["addr"] for o in out}
    covb = {r["addr"]: r for r in cov}
    archived = {}
    for c in read_ledger_main():
        if c[0] not in now and c[0] not in archived:
            archived[c[0]] = c[4]
    old_lines = list(open(OUT_TSV, encoding="utf-8")) if os.path.exists(OUT_TSV) else []
    for ln in old_lines:
        if not ln.startswith("# [归档] "):
            continue
        parts = ln[len("# [归档] "):].split()
        if parts and parts[0].startswith("0x"):
            archived.setdefault(parts[0], parts[1] if len(parts) > 1 else "?")
    if archived:
        lines.append("")
        lines.append("# ---- 归档：曾在台账主表、本轮重跑后**已不属于 F3 集合**的历史裁决行"
                     "（§10 E10；注释行，不参与任何机读计数）----")
        for a in sorted(archived, key=lambda x: int(x, 16)):
            r = covb.get(a)
            if r:
                lines.append("# [归档] %s %s ⇒ 已离开 F3（现 %s：%s %s）"
                             % (a, archived[a], r["bucket"], r["name_src"] or "—", r["name"] or FAKE))
            else:
                lines.append("# [归档] %s %s ⇒ 已不在 527 全集" % (a, archived[a]))
    open(OUT_TSV, "w", encoding="utf-8").write("\n".join(lines) + "\n")
    print("写出 %s（F3 %d 行 + F2-scan %d 行 + 归档 %d 行）" %
          (OUT_TSV, len(out), sum(1 for x in lines if x.startswith("0x")) - len(out), len(archived)))
    return stats(out)


def stats(out):
    c = collections.Counter(o["cat"] for o in out)
    n = len(out)
    cov = read_cov()
    total = len(cov)
    f1 = sum(1 for r in cov if r["bucket"] == "F1")
    f2n = sum(1 for r in cov if r["bucket"] == "F2")
    na = c["na:std-mono"] + c["na:gdext-glue"] + c["na:other"]
    na_strong = c["na:std-mono"] + c["na:gdext-glue"]
    rm = c["real-missing"]
    unk = c["unknown"]
    print("\n=== F3 %d 条分类（人工裁决后）" % n)
    for k in ("real-missing", "na:std-mono", "na:gdext-glue", "na:other", "unknown"):
        print("  %-16s %3d  (%5.1f%%)" % (k, c[k], c[k] * 100.0 / n))
    print("  合计 %d（= 当前 F3 %d 条 / 分母 527）%s"
          % (sum(c.values()), n, "✓" if sum(c.values()) == n else "✗"))
    # 台账一致性（§10 E10 之后才有意义）：盘上台账主表行数 vs 本轮 F3 条数
    led = read_ledger_main()
    if led:
        n_na_led = sum(1 for c in led if c[4].startswith("na:"))
        print("  盘上台账：主表 %d 行 / na:* %d 条 ⇒ 与本轮 F3 %d 条相差 %+d（差数应 = 归档行数）"
              % (len(led), n_na_led, n, len(led) - n))
    print("\n=== 三个数字")
    print("  ① 诚实缺口：F3 里 real-missing = **%d** 条（unknown %d 条另计；na:* %d 条不该手写）"
          % (rm, unk, na))
    print("  ② 口径 A（现行，含 na）：F1/527 = %d/%d = %.1f%%" % (f1, total, f1 * 100.0 / total))
    print("     口径 B（不含 na）：F1/(527-%d) = %d/%d = **%.1f%%**"
          % (na, f1, total - na, f1 * 100.0 / (total - na)))
    print("     口径 B'（只剔强 na：std-mono+gdext-glue %d 条）：F1/(527-%d) = %d/%d = **%.1f%%**"
          % (na_strong, na_strong, f1, total - na_strong, f1 * 100.0 / (total - na_strong)))
    print("     口径 C（不含 na，分子含 F2）：(F1+F2)/(527-%d) = %d/%d = **%.1f%%**"
          % (na, f1 + f2n, total - na, (f1 + f2n) * 100.0 / (total - na)))
    print("     参考：口径 A 的 M2=F1+F2 = %d/%d = %.1f%%" % (f1 + f2n, total,
                                                       (f1 + f2n) * 100.0 / total))
    bymod = collections.defaultdict(collections.Counter)
    for o in out:
        bymod[o["row"]["module"]][o["cat"]] += 1
    print("\n=== 按模块：na 占比最高的 8 个（F3 行数 >= 3）")
    rank = sorted([(m, cc) for m, cc in bymod.items() if sum(cc.values()) >= 3],
                  key=lambda x: -(x[1]["na:std-mono"] + x[1]["na:gdext-glue"] + x[1]["na:other"])
                  * 1.0 / sum(x[1].values()))
    for m, cc in rank[:8]:
        t = sum(cc.values())
        na_ = cc["na:std-mono"] + cc["na:gdext-glue"] + cc["na:other"]
        print("  %-40s F3=%2d na=%2d (%5.1f%%) real=%2d unk=%2d" %
              (m, t, na_, na_ * 100.0 / t, cc["real-missing"], cc["unknown"]))
    return 0


def control(rows, feats, size_freq, shared_locs=None):
    """正/负对照：12 std / 19 gdext / 2 真业务 / 3 附加负对照（见预注册 §4）。"""
    print("=" * 100)
    print("正/负对照（判据见 fixtures/f3_triage_plan.txt §4）")
    print("=" * 100)
    ok = True

    # 机判自校验：12 条 std 的 FDE 尺寸必须与台账逐对相同
    print("\n-- 机判自校验：12 条 std 的 FDE 尺寸（台账期望两两相同：%s）"
          % ", ".join("%#x" % s for s in CTRL_STD_SIZES))
    sizes = []
    for a in CTRL_STD:
        f = feats.get(a)
        if f is None:
            ctx = mk_ctx()
            f = features(ctx, int(a, 16), ctx.index())
        sizes.append((a, f.size))
    got = collections.Counter(s for _, s in sizes)
    print("   实测成对: " + "  ".join("%#x×%d" % (s, n) for s, n in sorted(got.items())))
    expect = collections.Counter({s: 2 for s in CTRL_STD_SIZES})
    if got == expect:
        print("   ✓ 与台账逐对相同")
    else:
        print("   ✗ 与台账不符（期望 %s）" % dict(expect))
        ok = False

    def judge(a, want, note=""):
        nonlocal ok
        f = feats.get(a)
        if f is None:
            ctx = ap.Ctx(None)
            f = features(ctx, int(a, 16), ctx.index())
        h = rule_hits(f, size_freq, {"addr": a, "name_src": "", "module": ""}, shared_locs)
        got = machine_class(f, h)
        good = (got == want)
        if not good:
            ok = False
        print("  %-4s %-9s want=%-14s got=%-14s %s %s" %
              ("✓" if good else "✗", a, want, got, fmt_feat(f, h)[:80], note))
        return good

    # §10 E9：控制组里 3 条 G4 源码闭包的期望值**已过期**（它们在 T87 定名后升 F1、不在 F3 集合了）。
    covrows = {r["addr"]: r for r in read_cov()}
    f3addrs = {r["addr"] for r, _, _ in rows}

    def judge_closure_left_f3(a):
        """§10 E9-1：断言「**已不在 F3**（T87 定名 ⇒ F1/F2）」+「机器候选不是 na:*」。

        旧期望是 `unknown`（G4 判不了），前提是"父函数在符号表里没有真名"。T87 用 Android
        `.symtab` 把它们定名成 `GraphwarLanVBox::update_game_list::{{closure}}` 后，这个前提**不成立了**；
        而 `machine_class()` 只对 F3 行生效 —— 对已离场的地址它当然退回 `real-missing`。
        ⇒ 继续要求 `unknown` 等于要求工具维持一个已失效的结论。
        """
        nonlocal ok
        f = feats.get(a)
        if f is None:
            f = features(mk_ctx(), int(a, 16), None)
        h = rule_hits(f, size_freq, {"addr": a, "name_src": "", "module": ""}, shared_locs)
        got = machine_class(f, h)
        left = a not in f3addrs
        not_na = not got.startswith("na:")
        good = left and not_na
        if not good:
            ok = False
        r = covrows.get(a, {})
        print("  %-4s %-9s 期望「已不在 F3 ∧ 机器候选非 na」  在F3=%s  机器候选=%-13s 现=%s: %s"
              % ("✓" if good else "✗", a, ("否" if left else "**是**"), got,
                 r.get("bucket", "?"), (r.get("name") or "—")[:44]))
        return good

    print("\n-- A 组：12 条 std 单态化 ⇒ 应判 na:std-mono")
    for a in CTRL_STD:
        judge(a, "na:std-mono")

    print("\n-- B 组：19 条 gdext 宏产物 ⇒ 应判 na:gdext-glue")
    print("   （其中 3 条源码闭包 `%s`：§10 E9 —— T87 已用 Android 符号表把它们定名成"
          % ",".join(sorted(CTRL_GDEXT_CLOSURE)))
    print("    `GraphwarLanVBox::update_game_list::{{closure}}` ⇒ 已被判 F1、**不在 F3 集合**，"
          "故本组改为负向断言）")
    for a in CTRL_GDEXT:
        if a in CTRL_GDEXT_CLOSURE:
            judge_closure_left_f3(a)
        else:
            judge(a, "na:gdext-glue")

    print("\n-- C 组：2 条 T71 落码的真业务函数 ⇒ 不许判 na:*")
    for a in CTRL_REAL:
        f = feats.get(a) or features(mk_ctx(), int(a, 16), (None, None, None))
        h = rule_hits(f, size_freq, None, shared_locs)
        got = machine_class(f, h)
        good = not got.startswith("na:")
        ok = ok and good
        print("  %-4s %-9s want=real（非 na）  got=%-14s %s" %
              ("✓" if good else "✗", a, got, fmt_feat(f, h)[:70]))

    print("\n-- D 组：3 条已知 F1 真业务 ⇒ 不许判 na:*（防「把一切判 na」）")
    for a in CTRL_REAL_EXTRA:
        f = feats.get(a) or features(mk_ctx(), int(a, 16), (None, None, None))
        h = rule_hits(f, size_freq, None, shared_locs)
        got = machine_class(f, h)
        good = not got.startswith("na:")
        ok = ok and good
        print("  %-4s %-9s want=real（非 na）  got=%-14s %s" %
              ("✓" if good else "✗", a, got, fmt_feat(f, h)[:70]))

    print("\n-- E 组（§10 E9-2）：**活着的** G4 正对照 —— 3 条旧样本已离场，必须补一条")
    print("   （判据：G4 命中 ⇒ machine_class 必须给 unknown；这些地址**不在** MANUAL 里被用来兜这一条）")
    for a in CTRL_G4_LIVE:
        f = feats.get(a) or features(mk_ctx(), int(a, 16), (None, None, None))
        h = rule_hits(f, size_freq, {"addr": a, "name_src": "", "module": ""}, shared_locs)
        got = machine_class(f, h)
        good = (got == "unknown" and h["g4"])
        ok = ok and good
        print("  %-4s %-9s want=unknown(G4)  got=%-14s g4=%-5s %s"
              % ("✓" if good else "✗", a, got, h["g4"], h["g4why"][:60]))

    print("\n-- F 组（§10 E6）：R-CALLFORM 正/负对照（调用形态闸）")
    print("   负对照：12 条 std 正对照必须全部**过闸**（体是泛型算法 ⇒ 允许 na:std-mono）")
    for a in CTRL_STD:
        f = feats.get(a) or features(mk_ctx(), int(a, 16), (None, None, None))
        h = rule_hits(f, size_freq, None, shared_locs)
        good, why = callform_verdict(f, h["A"], h["B"], h["C"])
        ok = ok and good
        print("  %-4s %-9s CALLFORM=%-11s %s" % ("✓" if good else "✗", a, f.callform, why[:88]))
    print("   正对照：T89 驳回的那一条 `0x438880` **若被当成 na:std-mono 候选，闸必须拒绝**")
    a = "0x438880"
    f = feats.get(a) or features(mk_ctx(), int(a, 16), (None, None, None))
    h = rule_hits(f, size_freq, {"addr": a, "name_src": "naming_android", "module": ""}, shared_locs)
    rej, why = callform_verdict(f, h["A"], h["B"], h["C"])
    good = (not rej) and h["A"] is False and h["B"] is False and h["C"] is False and f.callers == 0
    ok = ok and good
    print("  %-4s %-9s want=闸拒绝  CALLFORM=%-11s CALLERS=%d R-STD=%s ⇒ %s"
          % ("✓" if good else "✗", a, f.callform, f.callers,
             "/".join(g for g, v in (("A", h["A"]), ("B", h["B"]), ("C", h["C"])) if v) or "全不中",
             why[:70]))

    print("\n对照结论：%s" % ("全部通过 ⇒ 可以进全量" if ok else "有失败 ⇒ 必须先修工具/规则（预注册 §4）"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
