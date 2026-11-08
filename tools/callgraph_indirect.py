#!/usr/bin/env python3
"""callgraph_indirect.py — 把**间接调用**落成边：四份构建 × (直接 / 指针 / 取地址) 三列。

## 为什么需要这个工具（任务 70 / `docs/02` §45；靶子是 §44 第 3 条）

63 号的范围审计（`docs/02` §44.1）留了一句没修的话：

    调用图这条边是「只建直接调用」，而两份构建对这件事的口径根本不同：
    Linux `.so` 有 56,263 条 `call *disp(%rip)`，Windows DLL 只有 119 条
    ⇒ `callee` / `caller` / `ncal` / `ncall` / `cbsig` / `rbsig` 这 6 个特征**两侧本来就不可比**。

本工具把那段补上：把 `call *disp(%rip)` / `call *%reg` / `call *(%reg)` 落到**具体目标函数**，
于是两侧的「被调者集合 / 调用者集合」都能从**同一套口径**（直接边 ∪ 已落定的指针边）算出来。

## 三个量（任务书 §1.2）

对每个函数给：

* `dcal` —— 直接被调者集合（`call rel32`，来自指纹缓存的 `callees`，口径不变）；
* `pcal` —— 指针被调者集合（本工具新解出来的）；
* `naa`  —— 被取地址次数（`lea reg,[fn]` / `mov reg,imm=fn` / 数据槽里的函数地址）。

## 指针边怎么落定（四档，**逐档报数，不混着算**）

| 档 | 形态 | 落定方式 |
|---|---|---|
| A | `call *disp(%rip)` | Linux：`.rela.dyn` 的 `R_X86_64_RELATIVE` addend（`sym_match.Elf.rel`）；Windows：`.reloc` 的 `DIR64` 槽 + 磁盘上的真值 |
| B | `call *%reg` | 函数内**有界**寄存器回溯：`lea reg,[rip+X]` / `mov reg,imm` / `mov reg,[rip+slot]` ⇒ 目标 |
| C | `call *(%reg+disp)` | 同上回溯基址寄存器；基址是静态地址 ⇒ 有效槽 = base+disp，再按 A 落定 |
| D | 槽被**代码写入**（不是重定位） | `lea reg,[fn]` + `mov [rip+slot],reg` 这种「注册回调/闭包」写；**一个槽多处写入 ⇒ 保留全部目标** |

`B`/`C` 是**推断**档（回溯有界、遇屏障即弃、目标必须落在函数宇宙里）；`A`/`D` 是**观测**档。
落不定的（运行时填表、`(%reg)` 指向堆上对象的虚表）当**未知报**，不猜。

## 四份构建的实测规模（本机，`--jobs 16`）

见 `docs/02` §45 的表；一句话：**Linux 侧 52,577 条指针边、Windows 侧只有 1 条**，
因为 PE 没有符号抢占（intra-DLL 调用一律 `call rel32`），而 ELF 的 `-fno-plt` + 默认可见性
把「调到静态链接进本 .so 的 libstd 函数」全变成了 `call *GOT(%rip)`。

## 用法

    $PY tools/callgraph_indirect.py build  --jobs 16      # 扫四份构建，写缓存（不入库）
    $PY tools/callgraph_indirect.py four                  # 四份构建 × 三列 + 多目标槽 + 无法解析
    $PY tools/callgraph_indirect.py graph linux           # 单份构建的图规模 + 正对照
    $PY tools/callgraph_indirect.py cover                 # 6 个特征「两侧可比」改造前后的覆盖率
    $PY tools/callgraph_indirect.py bench --jobs 16       # 留出半 before→after（预注册选择规则）
    $PY tools/callgraph_indirect.py emit                  # 落 fixtures/*.tsv（只文本）
"""

import argparse
import collections
import itertools
import math
import multiprocessing
import os
import pickle
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import callgraph_fingerprint as C            # noqa: E402  BUILDS / load_fp / 打分器 / Pair
import sym_match as SM                       # noqa: E402  Elf
import three_way_match as T                  # noqa: E402  Pe / universe / owner_of

FIXTURES = os.path.join(ROOT, "fixtures")
OUT_TSV = os.path.join(FIXTURES, "callgraph_indirect.tsv")

# 缓存版本：扫描口径/回溯规则一改就抬（吃旧缓存会让数字静默漂）
#   v1 → v2：把 `lea reg,[fn]` 与 `mov reg,imm=fn` 分开计数（`naa` 的两种形态）；
#            并把 thunk 折叠（`--collapse`）结果单列，不再覆盖原始边。
CACHE_VERSION = "cgic-v2"

# 回溯窗口：从 `call` 往回最多看这么多条指令（含跳转但不越过 call/jmp/ret 屏障）
TRACE_WINDOW = 12

# x86-64 寄存器别名：写任一别名都视作写了这个 64 位寄存器
_ALIAS = {}
for _c, _names in {
    "rax": "rax eax ax al ah", "rbx": "rbx ebx bx bl bh", "rcx": "rcx ecx cx cl ch",
    "rdx": "rdx edx dx dl dh", "rsi": "rsi esi si sil", "rdi": "rdi edi di dil",
    "rbp": "rbp ebp bp bpl", "rsp": "rsp esp sp spl",
}.items():
    for _n in _names.split():
        _ALIAS[_n] = _c
for _i in range(8, 16):
    for _suffix in ("", "d", "w", "b"):
        _ALIAS["r%d%s" % (_i, _suffix)] = "r%d" % _i


def cache_dir():
    d = os.environ.get("CGIC_CACHE")
    if not d:
        d = os.path.join(SM.GW2, ".agent-work/cg/cgic70")
    os.makedirs(d, exist_ok=True)
    return d


def _size(path):
    try:
        return os.path.getsize(path)
    except OSError:
        return 0


def cache_path(tag):
    return os.path.join(cache_dir(), "%s-%d-%s.pkl" % (tag, _size(C.BUILDS[tag]["path"]),
                                                       CACHE_VERSION))


# ===========================================================================
# 1. 扫描：分片 + 进程池（任务书 §2「必须吃满 16 核」）
#
# 分片按**函数起点**切（不是按字节硬切）：`.text` 里的函数是连续的，所以
# `[starts[i0], starts[i1])` 一定从一条指令边界开始，不会切出半条指令来。
# ===========================================================================
_CTX = {}          # fork 之后子进程直接继承（只读，不写回）


def _open_build(tag):
    cfg = C.BUILDS[tag]
    if cfg["kind"] == "pe":
        obj = T.Pe(cfg["path"])
        U = T.pe_universe(obj)
        rel = obj.relocs()
    else:
        obj = SM.Elf(cfg["path"])
        U = T.elf_universe(obj, C.FUNCSTARTS if cfg["funcstarts"] else None)
        rel = obj.rel
    return obj, U, rel


_READONLY_REG0 = {"cmp", "test", "push", "call", "jmp", "bt", "ptest", "comiss", "comisd",
                  "ucomiss", "ucomisd", "cmps", "prefetch", "nop", "prefetcht0"}


def _reg(insn_name):
    return _ALIAS.get(insn_name, insn_name)


def _scan_x86_shard(spec):
    """扫一段 `.text`（以函数起点为界）。返回**只依赖本段**的东西，合并留给父进程。

    返回 `dict(kind=Counter, sites=[...], stores=[...], naa=[...])`：

    * `sites`  —— `[(owner, site, kind, refkind, ref)]`：`kind ∈ {p_rip,p_reg,p_mem}`；
                  `refkind ∈ {"slot","reg","none"}`，`slot` 已经是**静态槽地址**（A/C 档）；
    * `stores` —— `[(owner, slot, value)]`：代码往静态槽里写的**函数地址**（D 档）；
    * `naa`    —— `[(owner, site, target, how)]`：`how ∈ {"lea","imm","data"}`。
    """
    tag, i, n = spec
    ctx = _CTX[tag]
    obj, U, rel, starts = ctx["obj"], ctx["U"], ctx["rel"], ctx["starts"]
    lo = starts[(i * len(starts)) // n]
    hi = starts[((i + 1) * len(starts)) // n] if i + 1 < n else ctx["tex_end"]

    from capstone import Cs, CS_ARCH_X86, CS_MODE_64
    from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_OP_REG, X86_REG_RIP

    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    code = obj.read(lo, hi - lo)
    kind = collections.Counter()
    sites, stores, naa = [], [], []
    if not code:
        return dict(kind=kind, sites=sites, stores=stores, naa=naa)

    # 每函数一份寄存器状态；`owner` 一变就清（分片一定从函数边界开始）
    regval = {}         # reg -> ("val", addr) | ("ptr", slot)
    when = {}           # reg -> 指令序号
    cur = None
    idx = 0
    for insn in C.disasm_all(cs, code, lo, hi, 1):
        owner = T.owner_of(starts, U, insn.address)
        if owner is None:
            continue
        if owner != cur:
            cur, regval, when, idx = owner, {}, {}, 0
        idx += 1
        mn = insn.mnemonic
        end = insn.address + insn.size
        ops = insn.operands

        # ---- 屏障：call/jmp/ret 之后寄存器状态一律作废（跨基本块不许猜）----
        # 屏障（清寄存器状态）：call 在下面单独清；jmp/ret/ud2 的 fall-through 不可信。
        # **jcc 不清**：`mov rax,[rip+slot]; test rax,rax; jne .; call *rax` 是全库最常见的形态，
        # 清掉就等于放弃这一段；代价是可能把 fall-through 上的旧值当真理，所以下面还有
        # 「窗口 ≤ TRACE_WINDOW 条 + 目标必须落在函数宇宙里」两道闸。
        is_branch = mn == "jmp" or mn in ("ret", "retf", "ud2", "int3")
        if mn == "call":
            # 先按当前状态解析这条 call，再清状态
            if ops and ops[0].type == X86_OP_IMM:
                kind["d"] += 1
            elif ops and ops[0].type == X86_OP_MEM and ops[0].mem.base == X86_REG_RIP:
                kind["p_rip"] += 1
                slot = (end + ops[0].mem.disp) & 0xFFFFFFFFFFFFFFFF
                sites.append((owner, insn.address, "p_rip", "slot", slot))
            elif ops and ops[0].type in (X86_OP_REG, X86_OP_MEM):
                op = ops[0]
                if op.type == X86_OP_REG:
                    kind["p_reg"] += 1
                    r = _reg(insn.reg_name(op.reg))
                    st = regval.get(r)
                    if st and idx - when.get(r, -99) <= TRACE_WINDOW:
                        sites.append((owner, insn.address, "p_reg", st[0], st[1]))
                    else:
                        sites.append((owner, insn.address, "p_reg", "reg", r))
                else:
                    kind["p_mem"] += 1
                    base = _reg(insn.reg_name(op.mem.base)) if op.mem.base else None
                    st = regval.get(base) if base else None
                    if st and st[0] == "val" and idx - when.get(base, -99) <= TRACE_WINDOW:
                        # 基址是**静态地址** ⇒ 有效槽 = base+disp（虚表/回调表，C 档）
                        slot = (st[1] + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
                        sites.append((owner, insn.address, "p_mem", "slot", slot))
                    else:
                        sites.append((owner, insn.address, "p_mem", "reg", base or "?"))
            # 跨过 call 之后寄存器被改写（caller-saved）⇒ 状态作废
            regval, when = {}, {}
            continue

        # ---- `jmp fn`（尾调用）：**不是取地址**，但 §37 的 1,472 把它算进去了 ----#
        # 这里单列 `how="jmp"`，`naa`（真·取地址）不含它，`naa_all` 含它 ⇒ 两个口径都能报。
        if mn == "jmp" and ops and ops[0].type == X86_OP_IMM:
            o = T.owner_of(starts, U, ops[0].imm & 0xFFFFFFFFFFFFFFFF)
            if o is not None and o != owner:
                naa.append((owner, insn.address, o, "jmp"))

        # ---- 取地址 / 槽写入（D 档）：先看读、再看写 ----
        tgt_mem = tgt_imm = None
        if ops:
            for op in ops:
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    tgt_mem = (end + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
                    break
                if op.type == X86_OP_IMM:
                    tgt_imm = op.imm & 0xFFFFFFFFFFFFFFFF
                    break
        if mn == "lea" and len(ops) == 2 and ops[0].type == X86_OP_REG \
                and ops[1].type == X86_OP_MEM and ops[1].mem.base == X86_REG_RIP:
            a = (end + ops[1].mem.disp) & 0xFFFFFFFFFFFFFFFF
            r = _reg(insn.reg_name(ops[0].reg))
            regval[r], when[r] = ("val", a), idx
            o = T.owner_of(starts, U, a)
            if o is not None and o != owner:
                naa.append((owner, insn.address, o, "lea"))
        elif mn.startswith("mov") and len(ops) == 2 and ops[0].type == X86_OP_REG:
            r = _reg(insn.reg_name(ops[0].reg))
            if ops[1].type == X86_OP_MEM and ops[1].mem.base == X86_REG_RIP:
                slot = (end + ops[1].mem.disp) & 0xFFFFFFFFFFFFFFFF
                regval[r], when[r] = ("ptr", slot), idx       # 槽里的值（可能多目标）
            elif ops[1].type == X86_OP_IMM:
                a = ops[1].imm & 0xFFFFFFFFFFFFFFFF
                regval[r], when[r] = ("val", a), idx
                o = T.owner_of(starts, U, a)
                if o is not None and o != owner and ops[1].size == 8:
                    naa.append((owner, insn.address, o, "imm"))
            elif insn.operands[1].type == X86_OP_MEM and ops[1].mem.base \
                    and regval.get(_reg(insn.reg_name(ops[1].mem.base)), (None,))[0] == "val":
                base = _reg(insn.reg_name(ops[1].mem.base))
                st = regval.get(base)
                if st and st[0] == "val" and idx - when.get(base, -99) <= TRACE_WINDOW:
                    regval[r], when[r] = ("ptr", (st[1] + ops[1].mem.disp) & 0xFFFFFFFFFFFFFFFF), idx
                else:
                    regval.pop(r, None)
            else:
                regval.pop(r, None)
        else:
            # 其它写内存的形态：`mov [rip+slot], reg/imm` —— 存的是函数地址就是 D 档
            if len(ops) >= 2 and ops[0].type == X86_OP_MEM:
                slot = None
                if ops[0].mem.base == X86_REG_RIP:
                    slot = (end + ops[0].mem.disp) & 0xFFFFFFFFFFFFFFFF
                else:
                    b = _reg(insn.reg_name(ops[0].mem.base)) if ops[0].mem.base else None
                    st = regval.get(b) if b else None
                    if st and st[0] == "val" and idx - when.get(b, -99) <= TRACE_WINDOW:
                        slot = (st[1] + ops[0].mem.disp) & 0xFFFFFFFFFFFFFFFF
                if slot is not None and mn.startswith("mov"):
                    val = None
                    if ops[1].type == X86_OP_IMM:
                        val = ops[1].imm & 0xFFFFFFFFFFFFFFFF
                    elif ops[1].type == X86_OP_REG:
                        st = regval.get(_reg(insn.reg_name(ops[1].reg)))
                        if st and st[0] == "val":
                            val = st[1]
                        elif st and st[0] == "ptr":
                            val = ("copy", st[1])
                    if isinstance(val, tuple) and val[0] == "copy":
                        stores.append((owner, slot, val))
                    elif val is not None:
                        o = T.owner_of(starts, U, val)
                        if o is not None:
                            stores.append((owner, slot, o))

        # ---- 数据槽里的函数地址（取地址第 3 种形态：`.data.rel.ro` 的表项）----
        if tgt_mem is not None:
            o = T.owner_of(starts, U, tgt_mem)
            if o is not None and o != owner:
                naa.append((owner, insn.address, o,
                            "data" if mn not in ("call", "jmp") else "jmp"))

        # ---- 写寄存器：本轮没被上面显式建模的，一律当作「值未知」----
        try:
            _rw = cs.regs_access(insn)
            written = _rw[1] if isinstance(_rw, tuple) else ()
        except Exception:
            written = ()
        modeled = None
        if ops and ops[0].type == X86_OP_REG:
            modeled = _reg(insn.reg_name(ops[0].reg))
            if mn not in _READONLY_REG0:
                regval.pop(modeled, None)
        for rid in written:                       # 隐式写（mul/div/cpuid…）
            nm = _reg(cs.reg_name(rid))
            if nm != modeled:
                regval.pop(nm, None)
        if is_branch:                             # 无条件跳转/ret：状态作废
            regval, when = {}, {}
    return dict(kind=kind, sites=sites, stores=stores, naa=naa)


def _scan_arm64_shard(spec):
    """arm64（Android）：`bl` 直接、`blr` 间接、`br` 尾调用。

    间接目标的落定靠 `obj.rel` 的 `R_AARCH64_RELATIVE`：`ldr xN,[pc,#lit]` 的槽
    存的就是函数地址（addend）；`adrp+add` 造出的地址同样可查。
    """
    tag, i, n = spec
    ctx = _CTX[tag]
    obj, U, rel, starts = ctx["obj"], ctx["U"], ctx["rel"], ctx["starts"]
    lo = starts[(i * len(starts)) // n]
    hi = starts[((i + 1) * len(starts)) // n] if i + 1 < n else ctx["tex_end"]

    from capstone import Cs, CS_ARCH_ARM64
    from capstone.arm64 import ARM64_OP_IMM, ARM64_OP_MEM, ARM64_OP_REG

    cs = Cs(CS_ARCH_ARM64, 0)
    cs.detail = True
    code = obj.read(lo, hi - lo)
    kind = collections.Counter()
    sites, stores, naa = [], [], []
    if not code:
        return dict(kind=kind, sites=sites, stores=stores, naa=naa)
    regval, when = {}, {}
    cur, idx = None, 0
    for insn in C.disasm_all(cs, code, lo, hi, 4):
        owner = T.owner_of(starts, U, insn.address)
        if owner is None:
            continue
        if owner != cur:
            cur, regval, when, idx = owner, {}, {}, 0
        idx += 1
        mn = insn.mnemonic
        ops = insn.operands
        if mn in ("bl", "b"):
            if ops and ops[0].type == ARM64_OP_IMM:
                kind["d"] += 1
            elif mn == "b":
                pass
            continue
        if mn in ("br", "blr"):
            kind["p_reg" if mn == "blr" else "p_br"] += 1
            if ops and ops[0].type == ARM64_OP_REG:
                r = insn.reg_name(ops[0].reg)
                st = regval.get(r)
                if st and idx - when.get(r, -99) <= TRACE_WINDOW:
                    sites.append((owner, insn.address, "p_reg", st[0], st[1]))
                else:
                    sites.append((owner, insn.address, "p_reg", "reg", r))
            continue
        # adrp xN, #page
        if mn == "adrp" and len(ops) == 2 and ops[0].type == ARM64_OP_REG:
            r = insn.reg_name(ops[0].reg)
            if ops[1].type == ARM64_OP_IMM:
                regval[r], when[r] = ("val", ops[1].imm & 0xFFFFFFFFFFFFFFFF), idx
            else:
                regval.pop(r, None)
        elif mn in ("add", "ldr") and len(ops) >= 3 and ops[0].type == ARM64_OP_REG \
                and ops[1].type == ARM64_OP_REG:
            r = insn.reg_name(ops[0].reg)
            if mn == "add":
                # `add xN, xM, #imm`（3 操作数）与 `add xN, xM, xK, lsl #n`（4 操作数）都要认；
                # 第一版只写了 4 操作数那支，于是 `adrp+add+blr` 这条最常见的形态一条都没落定。
                base = insn.reg_name(ops[1].reg)
                st = regval.get(base)
                imm = ops[2].imm if ops[2].type == ARM64_OP_IMM else None
                if st and st[0] == "val" and imm is not None and \
                        idx - when.get(base, -99) <= TRACE_WINDOW:
                    a = (st[1] + imm) & 0xFFFFFFFFFFFFFFFF
                    regval[r], when[r] = ("val", a), idx
                    o = T.owner_of(starts, U, a)
                    if o is not None and o != owner:
                        naa.append((owner, insn.address, o, "lea"))
                else:
                    regval.pop(r, None)
            elif mn == "ldr":
                mem = insn.operands[1]
                if ops[1].type == ARM64_OP_MEM and ops[1].mem.base == 0:
                    slot = ops[1].mem.disp & 0xFFFFFFFFFFFFFFFF      # 字面量池
                elif ops[1].type == ARM64_OP_REG:
                    base = insn.reg_name(ops[1].reg)
                    st = regval.get(base)
                    slot = (st[1] + (ops[2].mem.disp if ops[2].type == ARM64_OP_MEM else 0)) \
                        & 0xFFFFFFFFFFFFFFFF if st and st[0] == "val" else None
                else:
                    slot = None
                if slot is not None:
                    regval[r], when[r] = ("ptr", slot), idx
                else:
                    regval.pop(r, None)
            else:
                regval.pop(r, None)
        else:
            if ops and ops[0].type == ARM64_OP_REG:
                regval.pop(insn.reg_name(ops[0].reg), None)
            if ops and ops[0].type == ARM64_OP_MEM:
                if ops[0].mem.base == 0:
                    o = T.owner_of(starts, U, ops[0].mem.disp & 0xFFFFFFFFFFFFFFFF)
                    if o is not None and o != owner:
                        naa.append((owner, insn.address, o, "data"))
            for op in ops:
                if op.type == ARM64_OP_IMM:
                    o = T.owner_of(starts, U, op.imm & 0xFFFFFFFFFFFFFFFF)
                    if o is not None and o != owner:
                        naa.append((owner, insn.address, o, "imm"))
    return dict(kind=kind, sites=sites, stores=stores, naa=naa)


# ===========================================================================
# 2. 槽 → 目标（**多目标必须全保留**）+ 图装配
# ===========================================================================
def _slot_targets(obj, U, starts, partials, verbose=True):
    """把「静态槽 → 目标函数集合」建起来，四个来源合并（一个槽多处写入 = 多目标）。

    来源：① `R_*_RELATIVE`（ELF）/ `.reloc` DIR64 + 磁盘真值（PE）；
          ② 代码里的 `mov [slot], <函数地址>`（回调注册表）；
          ③ `mov [slot], [slot2]` 这种**拷贝**（表对表拷贝，追一层）。
    """
    out = collections.defaultdict(set)
    src = collections.defaultdict(set)
    # ① 重定位写
    for slot, addend in obj.rel.items() if not isinstance(obj, T.Pe) else ():
        out[slot].add(addend)
        src[slot].add("reloc")
    if isinstance(obj, T.Pe):
        for slot in obj.relocs():
            b = obj.read(slot, 8)
            if b and len(b) == 8:
                out[slot].add(int.from_bytes(b, "little"))
                src[slot].add("reloc")
    # ② / ③ 代码写（可能多处写同一个槽 ⇒ 目标集合自然多元素）
    copies = []
    for p in partials:
        for owner, slot, val in p["stores"]:
            if isinstance(val, tuple) and val[0] == "copy":
                copies.append((slot, val[1], owner))
                continue
            out[slot].add(val)
            src[slot].add("store")
    for slot, src_slot, owner in copies:
        for v in out.get(src_slot, ()):
            out[slot].add(v)
            src[slot].add("copy")
    # 只保留真的落在函数宇宙里的目标
    keep = {}
    for slot, vals in out.items():
        tg = {T.owner_of(starts, U, v) for v in vals}
        tg = {t for t in tg if t is not None}
        if tg:
            keep[slot] = frozenset(tg)
    multi_slots = {s: v for s, v in keep.items() if len(v) > 1}
    if verbose:
        sys.stderr.write("[cgic] 槽 %d 个（重定位/代码写），落在函数上的 %d 个；"
                         "**多目标槽 %d 个**（目标数 2..%d）\n"
                         % (len(out), len(keep), len(multi_slots),
                            max((len(v) for v in multi_slots.values()), default=0)))
    return keep, src, multi_slots


def scan_build(tag, jobs=1, verbose=True):
    """扫一份构建 → 图状态（**只依赖本构建**，可缓存）。"""
    cfg = C.BUILDS[tag]
    t0 = time.time()
    fp = C.load_fp(tag, verbose=verbose)
    obj, U, rel = _open_build(tag)
    starts = sorted(U)
    tex = obj.sec[".text"]
    _CTX[tag] = dict(obj=obj, U=U, rel=rel, starts=starts,
                     tex_end=tex["addr"] + tex["size"])
    fn = _scan_arm64_shard if cfg["isa"] == "arm64" else _scan_x86_shard
    nsh = max(1, int(jobs)) * 4 if jobs and jobs > 1 else 1
    nsh = min(nsh, len(starts))
    specs = [(tag, i, nsh) for i in range(nsh)]
    if jobs and jobs > 1 and nsh > 1:
        ctx = multiprocessing.get_context("fork")
        with ctx.Pool(processes=min(jobs, nsh)) as pool:
            partials = pool.map(fn, specs, chunksize=1)
    else:
        partials = [fn(s) for s in specs]
    dt_scan = time.time() - t0

    kind = collections.Counter()
    for p in partials:
        kind.update(p["kind"])
    slots, slot_src, multi = _slot_targets(obj, U, starts, partials, verbose)

    # ---- 装配：每函数指针被调者 / 未落定计数 / 取地址 ----
    ptr_out = collections.defaultdict(set)
    unres = collections.defaultdict(collections.Counter)
    nres = 0
    for p in partials:
        for owner, site, k, rk, ref in p["sites"]:
            if rk == "slot":
                tg = slots.get(ref)
                if tg:
                    ptr_out[owner] |= set(tg)
                    nres += 1
                    if len(tg) > 1:
                        kind["site_multi"] += 1
                    continue
                unres[owner]["slot-no-target"] += 1
            elif rk == "reg":
                unres[owner]["reg"] += 1
            else:
                unres[owner]["none"] += 1
    naa = collections.Counter()          # 目标 → 被取地址次数（真·取地址：lea/imm/数据槽）
    naa_all = collections.Counter()      # 同上，但**含 `jmp fn` 尾调用**（与 §37 的 1,472 对账用）
    naa_sites = collections.Counter()    # 函数 → 它取了多少次别人的地址
    for p in partials:
        for owner, site, tgt, how in p["naa"]:
            naa_all[tgt] += 1
            if how != "jmp":
                naa[tgt] += 1
                naa_sites[owner] += 1
    # 指针槽里的函数地址也算「地址外泄」（§37.1 的第三条边）
    slot_taken = collections.Counter()
    for slot, tg in slots.items():
        for t in tg:
            slot_taken[t] += 1

    # ---- thunk 折叠（可选）：`call *GOT` 落到一个 16 B 的 `jmp real` 垫片时 ----
    thunk_to = {}
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64
    from capstone.x86 import X86_OP_IMM, X86_OP_MEM
    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    for a in set(ptr_out) | {t for ts in ptr_out.values() for t in ts}:
        f = fp.feats.get(a)
        if not f or f["size"] > 32:
            continue
        body = obj.read(a, min(f["size"], 16))
        if not body:
            continue
        ins = list(cs.disasm(body, a))
        if ins and ins[0].mnemonic == "jmp" and ins[0].operands and \
                ins[0].operands[0].type == X86_OP_IMM:
            o = T.owner_of(starts, U, ins[0].operands[0].imm & 0xFFFFFFFFFFFFFFFF)
            if o is not None and o != a:
                thunk_to[a] = o
    ptr_col = collections.defaultdict(set)
    for a, ts in ptr_out.items():
        ptr_col[a] = {thunk_to.get(t, t) for t in ts}

    st = dict(v=CACHE_VERSION, tag=tag, path=cfg["path"], size=_size(cfg["path"]),
              kind=cfg["kind"], isa=cfg["isa"], kindsites=dict(kind),
              nfunc=len(U), dt_scan=dt_scan, jobs=jobs, nsh=nsh,
              ptr_out={a: tuple(sorted(v)) for a, v in ptr_out.items()},
              ptr_col={a: tuple(sorted(v)) for a, v in ptr_col.items()},
              unres={a: dict(c) for a, c in unres.items()},
              naa=dict(naa), naa_all=dict(naa_all), naa_sites=dict(naa_sites),
              slots={s: tuple(sorted(v)) for s, v in slots.items()},
              slot_src={s: tuple(sorted(v)) for s, v in slot_src.items()},
              multi=len(multi), thunk_to=thunk_to, nres_sites=nres,
              slot_taken=dict(slot_taken))
    if verbose:
        sys.stderr.write("[cgic] %-8s %d 函数；直接 %d / p_rip %d / p_reg %d / p_mem %d；"
                         "指针边 %d 条（去重）/%d 个调用点；取地址 %d 个函数；未落定 %d 个调用点；"
                         "%.1f s（jobs=%d）\n"
                         % (tag, len(U), kind.get("d", 0), kind.get("p_rip", 0),
                            kind.get("p_reg", 0), kind.get("p_mem", 0),
                            sum(len(v) for v in ptr_out.values()), nres, len(naa),
                            sum(sum(c.values()) for c in unres.values()), dt_scan, jobs))
    return st


def build(tags=None, force=False, jobs=1, verbose=True):
    out = {}
    for tag in (tags or C.BUILDS):
        cp = cache_path(tag)
        if os.path.exists(cp) and not force:
            with open(cp, "rb") as fh:
                st = pickle.load(fh)
            if st.get("v") == CACHE_VERSION:
                if verbose:
                    sys.stderr.write("[cgic] 吃缓存 %s\n" % os.path.basename(cp))
                out[tag] = st
                continue
        st = scan_build(tag, jobs=jobs, verbose=verbose)
        with open(cp, "wb") as fh:
            pickle.dump(st, fh, protocol=4)
        if verbose:
            sys.stderr.write("[cgic] 缓存写出 %s（%.1f MB）\n"
                             % (os.path.basename(cp), os.path.getsize(cp) / 1e6))
        out[tag] = st
    return out


_STATE = {}


def load_state(tag, verbose=False):
    if tag in _STATE:
        return _STATE[tag]
    cp = cache_path(tag)
    if not os.path.exists(cp):
        T.die("没有 %s 的图缓存：先跑 `tools/callgraph_indirect.py build --jobs 16`" % tag)
    with open(cp, "rb") as fh:
        st = pickle.load(fh)
    if st.get("v") != CACHE_VERSION:
        T.die("%s 的图缓存版本 %r ≠ %r，重跑 build" % (cp, st.get("v"), CACHE_VERSION))
    _STATE[tag] = st
    return st


# ===========================================================================
# 3. 把图挂到指纹库上 → 6 个特征变成「两侧可比」
# ===========================================================================
ALL_KEYS = ("callee_a", "caller_a", "ncal_a", "ncall_a", "cbsig_a", "rbsig_a")


def attach(fp, st=None, collapse=False):
    """给 `fp.feats[*]` 挂上「直接边 ∪ 指针边」的派生字段（**不改既有字段**）。

    为什么要另开一组键而不是改 `callees`：§43/§44 的 72.7% / 76.4% / 78.2% 都是
    在**直接边**口径上量的，改掉就再也复现不了。所以新旧并存，A/B 对照才是单一变量。
    """
    st = st or load_state(fp.tag)
    ptr = st["ptr_col"] if collapse else st["ptr_out"]
    feats = fp.feats
    for va, fe in feats.items():
        ts = ptr.get(va, ())
        fe["ptr_callees"] = tuple(ts)
        allc = tuple(sorted(set(fe["callees"]) | set(ts)))
        fe["callees_a"] = allc
        fe["callee_sizes_a"] = tuple(sorted(feats[a]["size"] for a in allc))
        fe["callee_bsig_a"] = tuple(sorted(feats[a]["bsig"] or "-" for a in allc))
        fe["naa"] = int(st["naa"].get(va, 0))
    # 反向边（**从 `callees_a` 反推**；别拿 `fe["callers"]` 去反推 —— 那是「谁调用我」，
    # 反推出来的是「我调用谁」，方向会静默反过来）
    allcallers = collections.defaultdict(list)
    for va, fe in feats.items():
        for c in fe["callees_a"]:
            allcallers[c].append(va)
    for va, fe in feats.items():
        fe["caller_sizes_a"] = tuple(sorted(feats[a]["size"] for a in sorted(allcallers.get(va, ()))))
        fe["caller_bsig_a"] = tuple(sorted(feats[a]["bsig"] or "-"
                                           for a in sorted(allcallers.get(va, ()))))
        fe["callers_a"] = tuple(sorted(allcallers.get(va, ())))
        fe["ncaller_a"] = len(fe["callers_a"])
        fe["ncallee_a"] = len(fe["callees_a"])
    return fp


# ===========================================================================
# 4. 报告
# ===========================================================================
def cmd_four(args):
    states = build(tags=args.tag, force=False, jobs=1, verbose=False)
    W = sys.stdout.write
    W("%-9s %-28s %8s %9s %9s %9s %9s %9s %9s\n"
      % ("构建", "文件", "函数", "直接边", "指针边", "取地址", "未落定", "槽", "多目标槽"))
    for tag, st in states.items():
        fp = C.load_fp(tag, verbose=False)
        d = sum(len(f["callees"]) for f in fp.feats.values())
        p = sum(len(v) for v in st["ptr_out"].values())
        un = sum(v for a, c in st["unres"].items() for v in c.values())
        W("%-9s %-28s %8d %9d %9d %9d %9d %9d %9d\n"
          % (tag, os.path.basename(st["path"]), st["nfunc"], d, p, len(st["naa"]), un,
             len(st["slots"]), st["multi"]))
    W("\n调用点分类（每条 `call` 一条；`p_*` 是间接调用的三种形态）：\n")
    W("%-9s %10s %9s %9s %9s %9s\n" % ("构建", "直接", "p_rip", "p_reg", "p_mem", "p_br"))
    for tag, st in states.items():
        k = st["kindsites"]
        W("%-9s %10d %9d %9d %9d %9d\n" % (tag, k.get("d", 0), k.get("p_rip", 0),
                                           k.get("p_reg", 0), k.get("p_mem", 0),
                                           k.get("p_br", 0)))
    W("\n槽的写者来源（重定位 / 代码写 / 表对表拷贝；**一个槽可以有多个写者**）：\n")
    for tag, st in states.items():
        srcs = collections.Counter(tuple(v) for v in st["slot_src"].values())
        multi = [s for s, v in st["slots"].items() if len(v) > 1]
        W("  %-9s 槽来源 %-46s 多目标槽 %d 个（例：%s）\n"
          % (tag, dict(srcs), len(multi),
             ", ".join("%#x→%s" % (s, [hex(x) for x in st["slots"][s]]) for s in multi[:2])))
    return 0


def cmd_graph(args):
    tag = args.tag
    st = load_state(tag)
    fp = C.load_fp(tag, verbose=False)
    W = sys.stdout.write
    k = st["kindsites"]
    d = sum(len(f["callees"]) for f in fp.feats.values())
    p_raw = sum(len(v) for v in st["ptr_out"].values())
    p_col = sum(len(v) for v in st["ptr_col"].values())
    W("构建 %s（%s，%d 函数）\n" % (tag, os.path.basename(st["path"]), st["nfunc"]))
    W("边：直接 `call rel32` %d 条（去重后）；指针边 %d 条（去重）／%d 个落定调用点；"
      "折叠 thunk 后 %d 条\n" % (d, p_raw, st["nres_sites"], p_col))
    W("      §37 报的 52,577 是**调用点**数（同一函数反复调同一个槽会重复计），"
      "去重后的边数是 %d\n" % p_raw)
    W("调用点：直接 %d / `call *disp(%%rip)` %d / `call *%%reg` %d / `call *(%%reg)` %d"
      " / `br`(arm64) %d\n"
      % (k.get("d", 0), k.get("p_rip", 0), k.get("p_reg", 0), k.get("p_mem", 0),
         k.get("p_br", 0)))
    un = collections.Counter()
    for c in st["unres"].values():
        un.update(c)
    W("未落定的调用点 %d（%s）；其中 `slot-no-target` = 槽本身没有静态写者（运行时填表 / 外部导入）\n"
      % (sum(un.values()), dict(un)))
    # 反向：指针边让多少「零直接调用者」的函数拿到调用者
    rev = collections.defaultdict(set)
    for va, ts in st["ptr_out"].items():
        for t in ts:
            rev[t].add(va)
    gained = [va for va in fp.universe if not fp.feats[va]["callers"] and rev.get(va)]
    W("指针边让**原本零直接调用者**的函数获得调用者：%d（§37 的 2,045 同一口径）\n" % len(gained))
    W("地址被**代码取**（`lea` / `mov imm` / 数据槽）：%d 个函数，%d 个调用点；"
      "若把 `jmp fn`（尾调用）也算进来是 %d 个\n"
      % (len(st["naa"]), sum(st["naa_sites"].values()), len(st["naa_all"])))
    W("地址进了函数指针槽（多目标全保留）：%d 个函数\n" % len(st["slot_taken"]))
    W("静态槽 %d 个：重定位写 %d / 代码写 %d；**多目标槽 %d 个**；thunk 折叠识别 %d 个\n"
      % (len(st["slots"]), sum(1 for v in st["slot_src"].values() if "reloc" in v),
         sum(1 for v in st["slot_src"].values() if "store" in v), st["multi"],
         len(st["thunk_to"])))
    if args.control:
        W("\n正对照（已知槽，`AGENTS.md`「间接调用」第 1 条）：\n")
        for slot, want in ((0x9bac98, 0x3c2530), (0x9bc020, 0x8abca0)):
            tg = st["slots"].get(slot)
            W("  槽 %#x → %s（应为 %#x）%s\n"
              % (slot, [hex(t) for t in tg] if tg else None, want,
                 "✔" if tg == (want,) else "✘"))
    return 0


def _cov_row(feats, addrs, getter):
    vals = [len(getter(feats[a])) for a in addrs]
    hit = sum(1 for v in vals if v)
    return hit, len(addrs), hit / float(len(addrs)) if addrs else 0.0


def cmd_cover(args):
    """6 个特征改造前后的**两侧**覆盖率（构建侧：Linux / Windows；集合侧：正对照 / 目标）。"""
    fps = C.load_all(tags=["linux", "win"])
    attach(fps["linux"])
    attach(fps["win"])
    W = sys.stdout.write
    named = sorted(C.named_linux_348())
    tgt = C.target_addrs(fps)
    W("=== (1) 6 个特征的覆盖率：直接边口径（before）vs 直接∪指针（after）===\n")
    W("两侧 = 正对照 %d（已定名）/ 目标集合 %d（无 src/ Location）；都在 Linux 侧算\n"
      % (len(named), len(tgt)))
    W("%-10s %-22s %-22s %s\n" % ("特征", "正对照 命中/总数", "目标 命中/总数", "比值"))
    spec = [("callee", "callee_sizes", "callee_sizes_a"),
            ("caller", "caller_sizes", "caller_sizes_a"),
            ("ncal", "callees", "callees_a"),
            ("ncall", "callers", "callers_a"),
            ("cbsig", "callee_bsig", "callee_bsig_a"),
            ("rbsig", "caller_bsig", "caller_bsig_a")]
    rows = []
    for label, old, new in spec:
        for which, key in (("before", old), ("after", new)):
            g = (lambda k: (lambda fe: fe[k]))(key)
            hp, np_, pr = _cov_row(fps["linux"].feats, named, g)
            ht, nt, tr = _cov_row(fps["linux"].feats, tgt, g)
            W("%-10s %-22s %-22s %.2f   [%s]\n"
              % (label if which == "before" else label + "*",
                 "%d/%d = %.1f%%" % (hp, np_, 100 * pr),
                 "%d/%d = %.1f%%" % (ht, nt, 100 * tr), (pr / tr) if tr else float("inf"),
                 which))
            rows.append((label, which, hp, np_, ht, nt))
    W("\n=== (2) **构建之间**的可比性（这才是不可以前的那一半）===\n")
    W("%-8s %10s %12s %14s %16s %12s\n"
      % ("构建", "函数", "有指针边的", "占全部函数", "指针边数/函数", "空被调者"))
    for tag in ("linux", "win", "win_old", "android"):
        st = load_state(tag)
        fp = fps.get(tag) or C.load_fp(tag, verbose=False)
        np_ = sum(len(v) for v in st["ptr_out"].values())
        withptr = sum(1 for va in fp.universe if st["ptr_out"].get(va))
        empty = sum(1 for va in fp.universe if not fp.feats[va]["callees"])
        W("%-8s %10d %10d %11.1f%% %12.2f %9.1f%%\n"
          % (tag, len(fp.universe), withptr, 100.0 * withptr / max(1, len(fp.universe)),
             np_ / max(1, len(fp.universe)), 100.0 * empty / max(1, len(fp.universe))))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("# 任务 70 · 6 特征「两侧可比」改造前后的覆盖率\n")
            fh.write("# 正对照 = named_linux_348()；目标 = 无 src/ Location 的函数\n")
            fh.write("feature\twhich\tpos_hit\tpos_n\ttgt_hit\ttgt_n\n")
            for r in rows:
                fh.write("%s\t%s\t%d\t%d\t%d\t%d\n" % r)
        W("落盘：%s\n" % args.out)
    return 0


# ===========================================================================
# 5. 打分（6 个特征换成「两侧可比」的版本）+ 留出半
#
# ⚠ 这一节**不改** `callgraph_fingerprint.py` 的打分器：它在 `score_same` 里按
# `w.get(key)` 取权重，而 `w` 就是传进来的字典 ⇒ 只要本模块把新键的**值**挂到
# `feats` 上（§3 的 `attach`），再把权重写进配置，就能用**同一个** `Pair.ev` /
# `Pool` / `rank` / 真值 / 分半跑。**一处评分逻辑都没有第二份实现。**
# ===========================================================================
NEW_TERMS = (
    ("callee_a", lambda q, d: C.multiset_sim(q["callee_sizes_a"], d["callee_sizes_a"])),
    ("caller_a", lambda q, d: C.multiset_sim(q["caller_sizes_a"], d["caller_sizes_a"])),
    ("ncal_a", lambda q, d: C.sim_log(q["ncallee_a"], d["ncallee_a"])
        if (q["ncallee_a"] or d["ncallee_a"]) else None),
    ("ncall_a", lambda q, d: C.sim_log(q["ncaller_a"], d["ncaller_a"])
        if (q["ncaller_a"] or d["ncaller_a"]) else None),
    ("cbsig_a", lambda q, d: C.jaccard_sets(q["callee_bsig_a"], d["callee_bsig_a"])),
    ("rbsig_a", lambda q, d: C.jaccard_sets(q["caller_bsig_a"], d["caller_bsig_a"])),
)


def score_same_icg(qf, df, w=None):
    """`C.score_same` + 6 个「两侧可比」版本的特征（权重 0 时与 `C.score_same` **逐位相同**）。

    为什么能这样叠：`score_same` 是 `Σ w_k v_k / Σ w_k`（只累加 `w_k≠0 且 v_k≠None` 的项）。
    本函数把新增项按同一个归一化并进去 —— 分母加新增权重，分子加新增项。
    `w_k = 0` 时一个字节都不影响，所以「before」的每一个数都仍然是 §43/§44 的数。
    """
    w = w or C.W_SAME
    base_w = {k: v for k, v in w.items() if k not in dict(NEW_TERMS)}
    tot = wt = 0.0
    # 复用既有打分器：先把基线项算出来，再按同一套口径并新项
    # （`C.score_same` 内部已归一化，这里还原成未归一的 tot/wt 需要权重和 ⇒ 自己算一遍）
    for k, fn in (("size", lambda: C.sim_log(qf["size"], df["size"])),
                  ("hist", lambda: C.cos_hist(qf["hist"], df["hist"])),
                  ("consts", lambda: C.jaccard(qf["consts"], df["consts"])
                   if (qf["consts"] and df["consts"]) else None),
                  ("callee", lambda: C.multiset_sim(qf["callee_sizes"], df["callee_sizes"])),
                  ("caller", lambda: C.multiset_sim(qf["caller_sizes"], df["caller_sizes"])),
                  ("strs_s", lambda: C.jaccard(qf["strs_s_set"], df["strs_s_set"])
                   if (qf["strs_s"] and df["strs_s"]) else None),
                  ("strs", lambda: C.jaccard(qf["strs_set"], df["strs_set"])
                   if (qf["strs"] and df["strs"]) else None),
                  ("ncal", lambda: C.sim_log(len(qf["callees"]), len(df["callees"]))
                   if (qf["callees"] or df["callees"]) else None),
                  ("ncall", lambda: C.sim_log(len(qf["callers"]), len(df["callers"]))
                   if (qf["callers"] or df["callers"]) else None),
                  ("cbsig", lambda: C.jaccard_sets(qf["callee_bsig"], df["callee_bsig"])),
                  ("rbsig", lambda: C.jaccard_sets(qf["caller_bsig"], df["caller_bsig"])),
                  ("gram", lambda: C.jaccard(qf["grams"], df["grams"])
                   if (qf["grams"] and df["grams"]) else None),
                  ("shape", lambda: C.jaccard(qf["shgrams"], df["shgrams"])
                   if (qf["shgrams"] and df["shgrams"]) else None),
                  ("scal", lambda: C.jaccard(qf["scal"], df["scal"])
                   if (qf["scal"] and df["scal"]) else None),
                  ("scal_c", lambda: C.jaccard(qf["scal_c"], df["scal_c"])
                   if (qf["scal_c"] and df["scal_c"]) else None),
                  ("wconst", lambda: C.jaccard(qf["wconst"], df["wconst"])
                   if (qf["wconst"] and df["wconst"]) else None),
                  ("stok", lambda: C.jaccard(qf["stok"], df["stok"])
                   if (qf["stok"] and df["stok"]) else None),
                  ("cls", lambda: C.cos_hist(qf["cls"], df["cls"]))):
        weight = base_w.get(k)
        if not weight:
            continue
        val = fn()
        if val is None:
            continue
        tot += weight * val
        wt += weight
    for k, fn in NEW_TERMS:
        weight = w.get(k)
        if not weight:
            continue
        val = fn(qf, df)
        if val is None:
            continue
        tot += weight * val
        wt += weight
    return tot / wt if wt else 0.0


SCAL_KEYS_ICG = ("scal", "callee_a", "ncal_a")


def wlabel_icg(w):
    return C.wlabel({k: v for k, v in w.items() if v}) or "(empty)"


def _icg_eval(job):
    label, w, which = job
    pair = _G["pair"]
    addrs = pair.A if which == "A" else pair.B
    ok1 = okk = 0
    # `C.rank` 只把 (qf, df) 传给打分器 ⇒ 权重必须**闭包**进去（第一版忘了这一步，
    # 结果是 18 组配置全都被当成 W_SAME 跑，全部打印 72.7% —— 自检也没发现，因为
    # 自检比的是 score_same_icg(..., w) 与 C.score_same(..., w)，那条路是显式传 w 的）。
    ww = w
    sc = lambda q, d: score_same_icg(q, d, ww)          # noqa: E731
    for a in addrs:
        r = C.rank(pair.dst.feats[a], pair.pool, sc, C.WIN_SAME, _G["topk"])
        t = pair.truth[a]
        ok1 += bool(r) and r[0][1] == t
        okk += any(c == t for _s, c in r)
    return dict(label=label, w=w, which=which, top1=ok1, top5=okk, n=len(addrs))


_G = {}


def _run(fn, jobs, items, label):
    t0 = time.time()
    if jobs <= 1 or len(items) <= 1:
        res = [fn(it) for it in items]
    else:
        ctx = multiprocessing.get_context("fork")
        with ctx.Pool(processes=min(jobs, len(items))) as pool:
            res = pool.map(fn, items, chunksize=1)
    dt = time.time() - t0
    sys.stderr.write("[cgic] %s：%d 项，jobs=%d，墙钟 %.1f s\n" % (label, len(items), jobs, dt))
    return res, dt


def cmd_bench(args):
    fps = C.load_all(tags=["linux", "win"])
    attach(fps["linux"], collapse=args.collapse)
    attach(fps["win"], collapse=args.collapse)
    pair = C.Pair("win", "linux", fps=fps)
    _G["pair"], _G["topk"] = pair, args.topk
    W = sys.stdout.write
    base = dict(C.W_SAME)
    ref = dict(base, scal=0.5)          # 57 号 headline（§43 headline 配置）

    # ---- (0) 自检：新键权重 0 时必须与发布口径**逐位相同**，否则 before/after 不是单一变量 ----
    W("=== (0) 自检：score_same_icg（新键权重 0）与 C.score_same 逐位相同？ ===\n")
    import random
    rnd = random.Random(70)
    cands = sorted(pair.dst.universe)
    bad = 0
    for _ in range(2000):
        a = rnd.choice(cands)
        b = rnd.choice(pair.pool.vas)
        for w in (base, ref, dict(base, gram=1.0, size=0.5),
                  dict(base, callee=0.5, caller=0.5, ncal=0.5, ncall=0.5, cbsig=0.25, rbsig=0.25)):
            if abs(score_same_icg(pair.dst.feats[a], fps["win"].feats[b], w)
                   - C.score_same(pair.dst.feats[a], fps["win"].feats[b], w)) > 1e-12:
                bad += 1
    W("2,000 随机对 × 4 组权重：不一致 %d 处 %s\n"
      % (bad, "✔" if bad == 0 else "✘ 下面所有数作废"))

    # ---- (1) 单一变量 A/B：同一组权重，只换「图」----
    six_b = dict(base, callee=0.5, caller=0.5, ncal=0.5, ncall=0.5, cbsig=0.25, rbsig=0.25)
    six_a = dict(base, callee_a=0.5, caller_a=0.5, ncal_a=0.5, ncall_a=0.5,
                 cbsig_a=0.25, rbsig_a=0.25)
    cfgs = [
        ("(0) BEFORE 基线（直接边，§43 (0)）", base),
        ("(1) BEFORE 基线+scal=0.5（§43 REF，锚点）", ref),
        ("(2) BEFORE 只用 6 个特征（callee/caller/counts/bsig）", six_b),
        ("(3) AFTER  同上，6 个全换成直接∪指针", six_a),
        ("(4) AFTER  REF + callee_a=0.5", dict(ref, callee_a=0.5)),
        ("(5) AFTER  REF + ncal_a=0.5", dict(ref, ncal_a=0.5)),
        ("(6) AFTER  REF + callee_a=ncal_a=0.5", dict(ref, callee_a=0.5, ncal_a=0.5)),
        ("(7) AFTER  REF + caller_a=0.5（替 caller）", dict(ref, callee_a=0.0, caller=0.0,
                                                          caller_a=0.5)),
        ("(8) AFTER  REF + 六特征全开 0.25",
         dict(ref, callee_a=0.25, caller_a=0.25, ncal_a=0.25, ncall_a=0.25,
              cbsig_a=0.25, rbsig_a=0.25)),
        ("(9) AFTER  REF + callee_a=ncal_a=1", dict(ref, callee_a=1.0, ncal_a=1.0)),
    ]
    W("\n=== (1) 消融：before（直接边）/ after（直接∪指针），**两半都报** ===\n")
    resA, dtA = _run(_icg_eval, args.jobs, [(l, w, "A") for l, w in cfgs], "调参半")
    resB, dtB = _run(_icg_eval, args.jobs, [(l, w, "B") for l, w in cfgs], "留出半（只报数）")
    W("墙钟 调参半 %.1f s / 留出半 %.1f s（jobs=%d）；候选窗口 %s；n=%d/%d\n"
      % (dtA, dtB, args.jobs, C.WIN_SAME, len(pair.A), len(pair.B)))
    W("%-52s | %-15s | %-15s\n" % ("配置", "调参半 top-1/top-5", "留出半 top-1/top-5"))
    byA = {r["label"]: r for r in resA}
    byB = {r["label"]: r for r in resB}
    for label, _w in cfgs:
        a, b = byA[label], byB[label]
        W("%-52s | %5.1f%% %5.1f%%   | %5.1f%% %5.1f%%\n"
          % (label, 100.0 * a["top1"] / a["n"], 100.0 * a["top5"] / a["n"],
             100.0 * b["top1"] / b["n"], 100.0 * b["top5"] / b["n"]))

    # ---- (2) 预注册选择：只在调参半上选（规则**先定后用**，与 §43.4 同一条）----
    axes = collections.OrderedDict([("scal", (0.0, 0.25, 0.5)),
                                   ("callee_a", (0.0, 0.25, 0.5)),
                                   ("ncal_a", (0.0, 0.25, 0.5))])
    grid = []
    for combo in itertools.product(*axes.values()):
        g = dict(base)
        for k, v in zip(axes, combo):
            g[k] = v
        grid.append((wlabel_icg(g), g))
    W("\n=== (2) 加性网格 %d 组（轴 %s；**只在调参半上选**）===\n"
      % (len(grid), " ; ".join("%s=%s" % (k, ",".join("%.3g" % v for v in vs))
                               for k, vs in axes.items())))
    gres, gdt = _run(_icg_eval, args.jobs, [(l, w, "A") for l, w in grid], "加性网格（调参半）")
    W("墙钟 %.1f s（jobs=%d）\n" % (gdt, args.jobs))
    gres.sort(key=lambda r: (-r["top1"], -r["top5"], r["label"]))
    W("%-44s %-8s %s\n" % ("权重（非 0 项）", "top-1", "top-5"))
    for r in gres[:8]:
        W("%-44s %5.1f%%   %5.1f%%\n"
          % (r["label"], 100.0 * r["top1"] / r["n"], 100.0 * r["top5"] / r["n"]))
    gbest = gres[0]["top1"]
    bucket = [r for r in gres if r["top1"] >= gbest - 1]
    # tiebreak（预注册）：新增特征（callee_a / ncal_a，不算 scal 这个 §43 已有的）非 0 的个数最少
    bucket.sort(key=lambda r: (sum(1 for k in ("callee_a", "ncal_a") if r["w"].get(k)),
                               sum(1 for k in SCAL_KEYS_ICG if r["w"].get(k)),
                               -r["top5"], r["label"]))
    pick = bucket[0]
    W("调参半最高 %d/%d = %.1f%%，并列桶（≥最高−1 个样本）%d 组\n"
      % (gbest, gres[0]["n"], 100.0 * gbest / gres[0]["n"], len(bucket)))
    hres, _ = _run(_icg_eval, args.jobs, [(pick["label"], pick["w"], "B")],
                   "预注册选中的留出半（只报一次）")
    hr = hres[0]
    W("预注册选中：%s\n" % pick["label"])
    W("  -> 调参半 %.1f%% / %.1f%%；**留出半 %.1f%% / %.1f%%**（%d/%d）\n"
      % (100.0 * pick["top1"] / pick["n"], 100.0 * pick["top5"] / pick["n"],
         100.0 * hr["top1"] / hr["n"], 100.0 * hr["top5"] / hr["n"], hr["top1"], hr["n"]))
    bres, _ = _run(_icg_eval, args.jobs, [(r["label"], r["w"], "B") for r in bucket],
                   "并列桶留出半（只报数）")
    vals = sorted(100.0 * r["top1"] / r["n"] for r in bres)
    refb = byB[cfgs[1][0]]
    W("并列桶 %d 组的留出半 top-1：最低 %.1f%% / 中位 %.1f%% / 最高 %.1f%%"
      "（REF 锚点 %.1f%% 落在桶里第 %d 位）\n"
      % (len(bres), vals[0], vals[len(vals) // 2], vals[-1],
         100.0 * refb["top1"] / refb["n"],
         sum(1 for v in vals if v <= 100.0 * refb["top1"] / refb["n"])))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("# 任务 70 · 间接调用图让 6 个特征「两侧可比」前后的留出半"
                     "（tools/callgraph_indirect.py bench）\n")
            fh.write("# 真值 %s；调参半 n=%d / 留出半 n=%d；窗口 %s；collapse=%s\n"
                     % (", ".join("%s=%d" % kv for kv in pair.prov.items()),
                        len(pair.A), len(pair.B), C.WIN_SAME, args.collapse))
            fh.write("# 留出半**只报数**（AGENTS.md 合并纪律 A4）；预注册选择只在调参半上做\n")
            fh.write("label\tweights\ttop1_tune\ttop5_tune\tn_tune\ttop1_hold\ttop5_hold\tn_hold\n")
            for label, _w in cfgs:
                a, b = byA[label], byB[label]
                fh.write("%s\t%s\t%d\t%d\t%d\t%d\t%d\t%d\n"
                         % (label, wlabel_icg(_w), a["top1"], a["top5"], a["n"],
                            b["top1"], b["top5"], b["n"]))
            fh.write("# --- 加性网格（预注册选择）：调参半 + 选中/并列桶的留出半 ---\n")
            for r in gres:
                fh.write("GRID %s\t%s\t%d\t%d\t%d\t-\t-\t-\n"
                         % (r["label"], wlabel_icg(r["w"]), r["top1"], r["top5"], r["n"]))
            fh.write("PICK %s\t%s\t%d\t%d\t%d\t%d\t%d\t%d\n"
                     % (pick["label"], wlabel_icg(pick["w"]), pick["top1"], pick["top5"],
                        pick["n"], hr["top1"], hr["top5"], hr["n"]))
            for r in sorted(bres, key=lambda r: r["label"]):
                fh.write("TIE %s\t%s\t-\t-\t-\t%d\t%d\t%d\n"
                         % (r["label"], wlabel_icg(r["w"]), r["top1"], r["top5"], r["n"]))
        W("落盘：%s\n" % args.out)
    return 0


def cmd_emit(args):
    """落文本夹具：四份构建的规模 + Linux 每函数三列 + 348 正对照。"""
    states = build(tags=None, force=False, jobs=1, verbose=False)
    W = sys.stdout.write
    rows = []
    for tag, st in states.items():
        fp = C.load_fp(tag, verbose=False)
        for va in sorted(fp.universe):
            fe = fp.feats[va]
            rows.append((tag, va, fe["size"], len(fe["callees"]),
                         len(st["ptr_out"].get(va, ())), st["naa"].get(va, 0),
                         st["naa_sites"].get(va, 0)))
    with open(OUT_TSV, "w", encoding="utf-8") as fh:
        fh.write("# 任务 70 · 每个函数的三个量（直接被调者 / 指针被调者 / 被取地址次数）\n")
        fh.write("# 复算：$PY tools/callgraph_indirect.py build --jobs 16 && $PY tools/callgraph_indirect.py emit\n")
        fh.write("# build\tva\tsize\tncal_direct\tncal_ptr\tnaa(被别人取地址)\tnaa_sites(它取别人)\n")
        for r in rows:
            fh.write("%s\t0x%x\t%d\t%d\t%d\t%d\t%d\n" % r)
    W("落盘 %s（%d 行）\n" % (OUT_TSV, len(rows)))
    return 0



def cmd_discrim(args):
    """判别力（§44.1 的口径）：真配对上的相似度均值 − 同 size 窗口随机候选的均值。

    这个量比"留出半 top-1"敏感得多（top-1 是**排第一**的硬判决，n=165 上 1 个样本 = 0.6 点），
    所以 6 个特征"补上间接边之后到底有没有信息"要先看这里，再看准确率。
    """
    import random
    fps = C.load_all(tags=["linux", "win"])
    attach(fps["linux"], collapse=args.collapse)
    attach(fps["win"], collapse=args.collapse)
    pair = C.Pair("win", "linux", fps=fps)
    lin, win = pair.dst, pair.src
    pool = C.Pool(win)          # 候选池 = Windows 全量（与 C.Pair 同一口径）
    rnd = random.Random(70)
    spec = [("callee", "callee_sizes", "callee_sizes_a"),
            ("caller", "caller_sizes", "caller_sizes_a"),
            ("ncal", "callees", "callees_a"),
            ("ncall", "callers", "callers_a"),
            ("cbsig", "callee_bsig", "callee_bsig_a"),
            ("rbsig", "caller_bsig", "caller_bsig_a")]
    W = sys.stdout.write
    W("判别力 = 真配对 sim 均值 − 同窗口随机候选 sim 均值（n=%d 个可测真配对，每个 %d 个随机候选）\n"
      % (len(pair.testable), args.nrand))
    W("%-8s %-9s %-22s %-22s %s\n" % ("特征", "口径", "真配对 均值/中位/非零", "随机 均值/中位/非零", "判别力"))
    rows = []
    for label, old, new in spec:
        for which, key in (("before", old), ("after", new)):
            if key == "ncallee_a":
                fn = lambda q, d: C.sim_log(q["ncallee_a"], d["ncallee_a"]) \
                    if (q["ncallee_a"] or d["ncallee_a"]) else 0.0
            elif key == "ncaller_a":
                fn = lambda q, d: C.sim_log(q["ncaller_a"], d["ncaller_a"]) \
                    if (q["ncaller_a"] or d["ncaller_a"]) else 0.0
            elif key in ("callees", "callers"):
                fn = lambda q, d, k=key: C.sim_log(len(q[k]), len(d[k])) if (q[k] or d[k]) else 0.0
            elif key in ("callee_sizes", "caller_sizes", "callee_sizes_a", "caller_sizes_a"):
                fn = lambda q, d, k=key: C.multiset_sim(q[k], d[k])
            else:
                fn = lambda q, d, k=key: C.jaccard_sets(q[k], d[k])
            tv, rv = [], []
            for a in pair.testable:
                q = lin.feats[a]
                d = win.feats[pair.truth[a]]
                tv.append(fn(q, d))
                cands = pool.window(q["size"], C.WIN_SAME)
                for c in rnd.sample(cands, min(args.nrand, len(cands))):
                    rv.append(fn(q, win.feats[c]))
            def stats(v):
                v2 = sorted(v)
                return (sum(v) / len(v), v2[len(v2) // 2], sum(1 for x in v if x) / len(v))
            tm, tmed, tnz = stats(tv)
            rm, rmed, rnz = stats(rv)
            W("%-8s %-9s %-22s %-22s %+.3f\n"
              % (label if which == "before" else label + "*", which,
                 "%.3f/%.3f/%.1f%%" % (tm, tmed, 100 * tnz),
                 "%.3f/%.3f/%.1f%%" % (rm, rmed, 100 * rnz), tm - rm))
            rows.append((label, which, tm, tmed, tnz, rm, rmed, rnz))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("# 任务 70 · 6 个特征改造前后的判别力（真配对 vs 同窗口随机候选）\n")
            fh.write("feature\twhich\ttrue_mean\ttrue_median\ttrue_nonzero\trand_mean\t"
                     "rand_median\trand_nonzero\n")
            for r in rows:
                fh.write("%s\t%s\t%.4f\t%.4f\t%.4f\t%.4f\t%.4f\t%.4f\n"
                         % (r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7]))
        W("落盘：%s\n" % args.out)
    return 0



def _pair_eval(job):
    """同一批 query 上跑两个配置，返回逐样本命中位图（配对检验用）。"""
    w1, w2, which = job
    pair = _G["pair"]
    addrs = pair.A if which == "A" else pair.B
    b1, b2 = [], []
    for a in addrs:
        q = pair.dst.feats[a]
        t = pair.truth[a]
        r1 = C.rank(q, pair.pool, lambda x, y: score_same_icg(x, y, w1), C.WIN_SAME, 1)
        r2 = C.rank(q, pair.pool, lambda x, y: score_same_icg(x, y, w2), C.WIN_SAME, 1)
        b1.append(bool(r1) and r1[0][1] == t)
        b2.append(bool(r2) and r2[0][1] == t)
    return dict(which=which, b1=b1, b2=b2)


def mcnemar_exact(b, c):
    """McNemar 精确二项检验（b/c 是**不一致**的两格）；返回双尾 p。"""
    n = b + c
    if n == 0:
        return 1.0
    k = min(b, c)
    tail = sum(math.comb(n, i) for i in range(k + 1)) / float(2 ** n)
    return min(1.0, 2 * tail)


def cmd_pairs(args):
    """配对比较：同一批 query，配置 A vs B 的翻转数与 McNemar 精确 p。

    为什么必须配对：n=165 上 1 个样本 = 0.61 点，而留出半的 95% CI 宽 ~12 点
    （§44.4）。**只有配对**（同一批样本、同一套真值、同一个窗口）才分得出 5 点级的差别。
    """
    fps = C.load_all(tags=["linux", "win"])
    attach(fps["linux"], collapse=args.collapse)
    attach(fps["win"], collapse=args.collapse)
    pair = C.Pair("win", "linux", fps=fps)
    _G["pair"] = pair
    base = dict(C.W_SAME)
    six_b = dict(base, callee=0.5, caller=0.5, ncal=0.5, ncall=0.5, cbsig=0.25, rbsig=0.25)
    six_a = dict(base, callee_a=0.5, caller_a=0.5, ncal_a=0.5, ncall_a=0.5,
                 cbsig_a=0.25, rbsig_a=0.25)
    combos = [("6 特征 only：改前 → 改后", six_b, six_a),
              ("基线 → REF(scal=.5)", base, dict(base, scal=0.5)),
              ("REF → REF+callee_a=.5", dict(base, scal=0.5), dict(base, scal=0.5, callee_a=0.5)),
              ("REF → REF+caller_a 替 caller", dict(base, scal=0.5),
               dict(base, scal=0.5, caller=0.0, caller_a=0.5))]
    W = sys.stdout.write
    W("%-30s %-8s %-14s %-8s %-8s %s\n"
      % ("比较", "半", "A→B 翻转", "b/c", "Δ", "McNemar 精确 p"))
    rows = []
    jobs = list(itertools.chain([(w1, w2, "A") for _l, w1, w2 in combos],
                                [(w1, w2, "B") for _l, w1, w2 in combos]))
    res, _ = _run(_pair_eval, max(1, args.jobs), jobs, "配对比较")
    for i, (label, w1, w2) in enumerate(combos):
        for tag, d in (("调参半", res[i]), ("留出半", res[i + len(combos)])):
            b = sum(1 for x, y in zip(d["b1"], d["b2"]) if y and not x)
            c = sum(1 for x, y in zip(d["b1"], d["b2"]) if x and not y)
            a1 = sum(d["b1"])
            a2 = sum(d["b2"])
            n = len(d["b1"])
            W("%-30s %-8s %-14s %-8s %-8s p=%.4f\n"
              % (label, tag, "%d→%d (%d/%d)" % (a1, a2, a2 - a1, n), "%d/%d" % (b, c),
                 "%+.1f pt" % (100.0 * (a2 - a1) / n), mcnemar_exact(b, c)))
            rows.append((label, tag, a1, a2, b, c, n, mcnemar_exact(b, c)))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write("# 任务 70 · 配对比较（同一批 query / 同一真值 / 同一窗口）\n")
            fh.write("compare\thalf\tA_hit\tB_hit\tb(wrong->right)\tc(right->wrong)\tn\tmcnemar_p\n")
            for r in rows:
                fh.write("%s\t%s\t%d\t%d\t%d\t%d\t%d\t%.4f\n" % r)
        W("落盘：%s\n" % args.out)
    return 0


def cmd_selftest(args):
    """正对照：**扫描器/解析器本身**先验一遍（任务书 §3「负结论要有正对照」）。

    四件事，每件都必须 PASS，否则后面那些「0 个」「扫不到」一个都不能信：
    1. 已知槽：`0x9bac98 → 0x3c2530`、`0x9bc020 → 0x8abca0`（`AGENTS.md`「间接调用」第 1 条）；
    2. GOT 槽被调用：`call *0x9bc020(%rip)` 的调用点里必须包含 `EntityDrawer::draw`（`0x4b2890`）；
    3. 多目标槽的**保留逻辑**：人工给同一个槽两个写者 ⇒ 必须保留两个目标（本二进制里
       多目标槽是 0 个，所以只能用合成输入验这条代码路径 —— 这正是「0 要能自证不是 bug」）；
    4. 分片不变性：同一段 `.text` 用 1 片与 7 片扫，边的集合必须**逐条相同**。
    """
    W = sys.stdout.write
    ok = True

    def check(name, cond, extra=""):
        nonlocal ok
        ok = ok and bool(cond)
        W("  [%s] %-46s %s\n" % ("PASS" if cond else "**FAIL**", name, extra))

    W("=== (1) 已知槽（AGENTS.md「间接调用」第 1 条）===\n")
    st = load_state("linux")
    for slot, want in ((0x9bac98, 0x3c2530), (0x9bc020, 0x8abca0)):
        got = st["slots"].get(slot)
        check("槽 %#x → %#x" % (slot, want), got == (want,), "实测 %s"
              % ([hex(t) for t in got] if got else None))

    W("=== (2) `call *0x9bc020(%rip)` 的调用点里要有 EntityDrawer::draw ===\n")
    fp = C.load_fp("linux", verbose=False)
    hits = [va for va in fp.universe
            if 0x8abca0 in st["ptr_out"].get(va, ()) or 0x8abca0 in st["ptr_col"].get(va, ())]
    check("0x4b2890 有指向 wrapper 0x8abca0 的指针边", 0x4b2890 in hits,
          "（共 %d 个函数，前 6: %s）" % (len(hits), [hex(x) for x in hits[:6]]))

    W("=== (3) 多目标槽：同一个槽两个写者 ⇒ 两个目标都要在 ===\n")
    U = {0x100: dict(va=0x100, end=0x200), 0x200: dict(va=0x200, end=0x300),
         0x300: dict(va=0x300, end=0x400)}
    starts = sorted(U)

    class _FakeObj:
        rel = {}

        def read(self, va, n):
            return None

    partials = [dict(stores=[(0x100, 0x9000, 0x200), (0x300, 0x9000, 0x300)], sites=[],
                     naa=[], kind=collections.Counter())]
    slots, src, multi = _slot_targets(_FakeObj(), U, starts, partials, verbose=False)
    check("槽 0x9000 保留 2 个目标", slots.get(0x9000) == (0x200, 0x300) or
          set(slots.get(0x9000, ())) == {0x200, 0x300}, "实测 %s" % (slots.get(0x9000),))
    check("它被记为多目标槽", 0x9000 in multi, "multi=%d" % len(multi))

    W("=== (4) 分片不变性（同一段 .text，1 片 vs 7 片）===\n")
    tag = "linux"
    obj, U2, rel = _open_build(tag)
    tex = obj.sec[".text"]
    _CTX[tag] = dict(obj=obj, U=U2, rel=rel, starts=sorted(U2),
                     tex_end=tex["addr"] + tex["size"])
    one = _scan_x86_shard((tag, 0, 1))
    many = [_scan_x86_shard((tag, i, 7)) for i in range(7)]
    agg = dict(kind=collections.Counter(), sites=[], stores=[], naa=[])
    for p in many:
        agg["kind"].update(p["kind"])
        for k in ("sites", "stores", "naa"):
            agg[k].extend(p[k])
    check("sites 集合逐条相同", sorted(one["sites"]) == sorted(agg["sites"]),
          "%d vs %d" % (len(one["sites"]), len(agg["sites"])))
    check("stores 集合逐条相同", sorted(one["stores"]) == sorted(agg["stores"]))
    check("naa 集合逐条相同", sorted(one["naa"]) == sorted(agg["naa"]))
    check("调用点计数相同", one["kind"] == agg["kind"], str(dict(one["kind"])))
    W("\n%s\n" % ("全部 PASS ✔" if ok else "**有 FAIL** ✘"))
    return 0 if ok else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd")

    def add(name, fn, **kw):
        p = sub.add_parser(name)
        p.set_defaults(fn=fn)
        for k, v in kw.items():
            p.add_argument("--" + k.replace("_", "-"), default=v, **(
                dict(action="store_true") if v is True else {}))
        return p

    p = sub.add_parser("build")
    p.add_argument("--jobs", type=int, default=16)
    p.add_argument("--force", action="store_true")
    p.add_argument("--tag", action="append", default=None)
    p.set_defaults(fn=lambda a: (build(tags=a.tag, force=a.force, jobs=a.jobs), 0)[1])

    p = sub.add_parser("four")
    p.add_argument("--tag", action="append", default=None)
    p.set_defaults(fn=cmd_four)

    p = sub.add_parser("graph")
    p.add_argument("--tag", default="linux")
    p.add_argument("--control", action="store_true", default=True)
    p.set_defaults(fn=cmd_graph)

    p = sub.add_parser("cover")
    p.add_argument("--out", default="")
    p.set_defaults(fn=cmd_cover)

    p = sub.add_parser("bench")
    p.add_argument("--jobs", type=int, default=16)
    p.add_argument("--topk", type=int, default=5)
    p.add_argument("--collapse", action="store_true")
    p.add_argument("--on-ref", action="store_true")
    p.add_argument("--base-label", default="REF(scal.5)")
    p.add_argument("--out", default="")
    p.set_defaults(fn=cmd_bench)

    p = sub.add_parser("discrim")
    p.add_argument("--nrand", type=int, default=200)
    p.add_argument("--collapse", action="store_true")
    p.add_argument("--out", default="")
    p.set_defaults(fn=cmd_discrim)

    p = sub.add_parser("pairs")
    p.add_argument("--jobs", type=int, default=8)
    p.add_argument("--collapse", action="store_true")
    p.add_argument("--out", default="")
    p.set_defaults(fn=cmd_pairs)

    p = sub.add_parser("selftest")
    p.set_defaults(fn=cmd_selftest)

    p = sub.add_parser("emit")
    p.set_defaults(fn=cmd_emit)

    args = ap.parse_args(argv)
    if not getattr(args, "fn", None):
        ap.print_help()
        return 2
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
