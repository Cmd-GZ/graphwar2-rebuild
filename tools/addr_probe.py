#!/usr/bin/env python3
"""单地址取证：把「这条指令/这个地址到底引用或被谁引用」一次问清。

## 为什么需要它

清理「地址级悬案」时 90% 的时间花在同一件事上：拿一个虚地址 → 反汇编一小段 →
手工把 `lea 0x1234(%rip)` 的目标、`call *0x...(%rip)` 的 GOT 槽、`jmp *%rax` 的跳表、
`panic::Location` 结构体逐个换算。换算错一次（`.text` 的 VA↔文件偏移差 `+0x1000`，
或把 Ghidra 地址当虚地址用）就会**安静地**读到垃圾，并得出一个看起来很合理的错结论。

本工具把这几步做成一条命令，且每个结论都自带地址：

| 命令 | 回答 |
|---|---|
| `dis VA`     | 反汇编一个函数（边界取 `.eh_frame` FDE），就地注解：`.rodata` 串、GOT 槽→重定位 addend、`panic::Location` |
| `fde VA`     | 函数真实范围（FDE）与 Ghidra 范围并列——**Ghidra 偶尔把两个函数并成一个**，此时 FDE 是独立裁判 |
| `str VA`     | 读 `.rodata` 字节，判断是不是可打印串 |
| `callers VA` | `call rel32` 调用点 **+ 函数指针三形态**（`lea` 取址 / `movabs imm64` / 引用 addend==VA 的重定位槽）。`call rel32` 为零**不代表**没人调 |
| `locs VA`    | 该函数引用了哪些 `panic::Location`（`{file,len,line,col}`）→ 归到 `src/*.rs`，按次数投票 |
| `jt VA`      | 展开一条 `lea` 指向的 int32 相对跳表：条目数 + 目标分布（判「默认臂」用） |

## 口径

* 一律 **ELF 虚地址**。Ghidra/REA 地址 = 本工具地址 **+ 0x100000**（只加一次）。
* 函数边界不读 `analysis/*.pkl`：`.eh_frame` FDE 从 `.so` 自己解；Ghidra 表只作**对照**打印
  （`GW2_FUNCSTARTS` 指定，缺省自动向上找 `analysis/funcstarts_ghidra_va.pkl`）。
* 全 `.text` 的索引（调用点 / rip 引用 / 立即数）第一次算完就缓存到 `GW2_PROBE_CACHE`
  （缺省 `.agent-work/addr-audit/probe_index.pkl`），之后每次查询是毫秒级。

用法：
    ../.venv/bin/python tools/addr_probe.py fde     0x3e6830
    ../.venv/bin/python tools/addr_probe.py dis     0x3e6830
    ../.venv/bin/python tools/addr_probe.py callers 0x3e6830
    ../.venv/bin/python tools/addr_probe.py locs    0x418820
    ../.venv/bin/python tools/addr_probe.py jt      0x1bf148
"""

import argparse
import bisect
import os
import pickle
import struct
import sys
from collections import Counter

from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86 import X86_OP_MEM, X86_OP_IMM, X86_OP_REG, X86_REG_RIP

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)
sys.path.insert(0, HERE)

from loc_xrefs import Elf, parse_eh_frame, parse_eh_frame_hdr  # noqa: E402

def _find_so():
    """目标 `.so` 在 `gw2/.reference/steam/<tag>/`；克隆比 `gw2/` 深几层时逐级向上找。"""
    env = os.environ.get("GW2_SO")
    if env:
        return env
    base = ROOT
    for _ in range(6):
        d = os.path.join(base, ".reference", "steam")
        if os.path.isdir(d):
            tags = sorted(t for t in os.listdir(d) if t.startswith("2026-"))
            for t in reversed(tags or sorted(os.listdir(d))):
                c = os.path.join(d, t, "libgraphwarrust.so")
                if os.path.exists(c):
                    return c
        base = os.path.dirname(base)
    return os.path.join(ROOT, ".reference", "steam", "2026-08-28", "libgraphwarrust.so")


SO_DEFAULT = _find_so()
CACHE_DEFAULT = os.environ.get(
    "GW2_PROBE_CACHE",
    os.path.join(ROOT, ".agent-work", "addr-audit", "probe_index.pkl"),
)
PRINTABLE = set(range(0x20, 0x7F))


def show(b):
    if not b:
        return ""
    if all(c in PRINTABLE for c in b):
        return repr(b.decode("ascii"))
    return "<bin %s%s>" % (b[:16].hex(), "…" if len(b) > 16 else "")


class Ctx:
    """ELF + FDE + 重定位。**不扫全 .text**，所以起进程是毫秒级。"""

    def __init__(self, so):
        self.so = so
        self.elf = Elf(so)
        eh = self.elf.sec(".eh_frame")
        try:
            fdes = parse_eh_frame(
                self.elf.data[eh["offset"]: eh["offset"] + eh["size"]], eh["addr"]
            )
        except Exception:
            fdes = []
        if not fdes:
            fdes = [(a, 0) for a in parse_eh_frame_hdr(self.elf)]
        self.fde_ranges = sorted(r for r in fdes if r[1])
        self.fde_lo = [a for a, _ in self.fde_ranges]
        self.exec_ranges = [
            (s["addr"], s["addr"] + s["size"]) for s in self.elf.exec_sections() if s["size"]
        ]
        self.rel = dict(self.elf.relative_relocs())
        self._idx = None

    # -- 函数边界 ------------------------------------------------------
    def fde_extent(self, va):
        i = bisect.bisect_right(self.fde_lo, va) - 1
        for j in (i, i + 1):
            if 0 <= j < len(self.fde_ranges):
                st, ln = self.fde_ranges[j]
                if st <= va < st + ln:
                    return (st, st + ln)
        return None

    def owner(self, va):
        ex = self.fde_extent(va)
        return ex[0] if ex else None

    # -- 读 ------------------------------------------------------------
    def read(self, va, n):
        return self.elf.read(va, n)

    def cstr(self, va, cap=200):
        out = bytearray()
        for i in range(cap):
            try:
                c = self.elf.read(va + i, 1)
            except Exception:
                break
            if not c or not (0 < c[0] < 0x7F):
                break
            out += c
        return bytes(out)

    # -- 全 .text 索引（缓存）------------------------------------------
    def index(self, cache=CACHE_DEFAULT):
        if self._idx is not None:
            return self._idx
        stat = os.stat(self.so)
        key = "%s:%d:%d" % (os.path.abspath(self.so), stat.st_size, int(stat.st_mtime))
        if cache and os.path.exists(cache):
            try:
                k, idx = pickle.load(open(cache, "rb"))
                if k == key:
                    self._idx = idx
                    return idx
            except Exception:
                pass
        md = Cs(CS_ARCH_X86, CS_MODE_64)
        md.detail = True
        call_imm, rip, imm = {}, {}, {}
        for lo, hi in self.exec_ranges:
            for ins in md.disasm(self.elf.read(lo, hi - lo), lo):
                for op in ins.operands:
                    if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                        t = ins.address + ins.size + op.mem.disp
                        rip.setdefault(t, []).append(ins.address)
                    elif op.type == X86_OP_IMM:
                        v = op.imm & 0xFFFFFFFFFFFFFFFF
                        if ins.mnemonic == "call":
                            call_imm.setdefault(v, []).append(ins.address)
                        elif v > 0x10000:
                            imm.setdefault(v, []).append(ins.address)
        idx = (call_imm, rip, imm)
        if cache:
            try:
                os.makedirs(os.path.dirname(cache), exist_ok=True)
                pickle.dump((key, idx), open(cache, "wb"))
            except Exception:
                pass
        self._idx = idx
        return idx


def dis_list(ctx, lo, hi):
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    return list(md.disasm(ctx.read(lo, hi - lo), lo))


_LOC_CACHE = {}


def maybe_location(ctx, slot):
    """`{file:&'static str, len:u64, line:u32, col:u32}` 24 字节；slot 可能指向 +0/+8/+16。"""
    if slot in _LOC_CACHE:
        return _LOC_CACHE[slot]
    out = None
    for d in (0, -8, -16):
        s = slot + d
        if s not in ctx.rel:
            continue
        p = ctx.rel[s]
        try:
            ln = struct.unpack("<Q", ctx.read(s + 8, 8))[0]
            raw = ctx.read(p, min(ln, 200)) or b""

            line, col = struct.unpack("<II", ctx.read(s + 16, 8))
        except Exception:
            continue
        if raw and raw.startswith(b"src/") and all(0x20 <= c < 0x7F for c in raw):
            out = "%s:%d:%d" % (raw.decode("ascii"), line, col)
            break
    _LOC_CACHE[slot] = out
    return out


def note(ctx, ins):
    out = []
    for op in ins.operands:
        if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
            t = ins.address + ins.size + op.mem.disp
            if t in ctx.rel:
                out.append("slot %#x -> %#x" % (t, ctx.rel[t]))
                continue
            loc = maybe_location(ctx, t)
            if loc:
                out.append("Location %s" % loc)
                continue
            try:
                q = ctx.read(t, 8)
            except Exception:
                q = b""
            if q and all(0x20 <= c < 0x7F for c in q):
                out.append("@%#x %s" % (t, show(ctx.cstr(t)[:72])))
            else:
                out.append("@%#x (%s)" % (t, show(q)))
        elif op.type == X86_OP_IMM:
            v = op.imm & 0xFFFFFFFFFFFFFFFF
            if v > 0x1000:
                out.append("imm=%#x" % v)
    return out


_ST = []


def ghidra_starts():
    global _ST
    if _ST:
        return _ST
    p = os.environ.get("GW2_FUNCSTARTS")
    if not p or not os.path.exists(p):
        d = ROOT
        p = None
        for _ in range(6):
            c = os.path.join(d, "analysis", "funcstarts_ghidra_va.pkl")
            if os.path.exists(c):
                p = c
                break
            d = os.path.dirname(d)
    if p and os.path.exists(p):
        raw = pickle.load(open(p, "rb"))
        _ST = sorted(int(x, 16) if isinstance(x, str) else int(x) for x in raw)
    return _ST


def ghidra_extent(va):
    st = ghidra_starts()
    if not st:
        return None
    i = bisect.bisect_left(st, va)
    if i >= len(st) or st[i] != va:
        return (va, None)
    return (va, st[i + 1] if i + 1 < len(st) else None)


# ---------------------------------------------------------------------------
def cmd_fde(ctx, a):
    ex = ctx.fde_extent(a)
    print("requested  %#x" % a)
    print("eh_frame   %s" % (("%#x..%#x  size %#x" % (ex[0], ex[1], ex[1] - ex[0])) if ex else "(no FDE)"))
    i = bisect.bisect_left(ctx.fde_lo, a)
    if i >= len(ctx.fde_lo) or ctx.fde_lo[i] != a:
        print("note       NOT an FDE function start")
    g = ghidra_extent(a)
    print("ghidra     %s" % (("%#x..%#x  size %#x" % (g[0], g[1], g[1] - g[0])) if g and g[1] else "(no entry)"))
    if g and g[1]:
        i = bisect.bisect_right(ctx.fde_lo, g[0])
        inner = [a for a in ctx.fde_lo[i:] if a < g[1]]
        if inner:
            print("!! Ghidra 范围 [%#x,%#x) 内还含有 %d 个 FDE 函数起点 %s"
                  % (g[0], g[1], len(inner), " ".join("%#x" % a for a in inner[:6])))
            print("   ⇒ Ghidra 把相邻函数并在了一起；真函数长度以 eh_frame 为准")
    return 0


def cmd_dis(ctx, a, length=None):
    ex = ctx.fde_extent(a)
    hi = (ex[1] if ex else a + 0x120) if length is None else a + length
    for ins in dis_list(ctx, a, hi):
        print("  %#08x  %-7s %-32s %s" % (ins.address, ins.mnemonic, ins.op_str, "  ".join(note(ctx, ins))))
    return 0


def cmd_str(ctx, a, n):
    print("%#x: %s" % (a, show(ctx.read(a, n))))
    s = ctx.cstr(a)
    if s:
        print("cstr:  %r" % s.decode("ascii"))
    return 0


def cmd_callers(ctx, a):
    call_imm, rip, imm = ctx.index()
    sites = sorted(call_imm.get(a, ()))
    print("== call rel32: %d ==" % len(sites))
    for s in sites:
        print("  %#x   in %#x" % (s, ctx.owner(s) or 0))
    slots = sorted(k for k, v in ctx.rel.items() if v == a)
    print("== reloc slot addend == %#x : %d ==" % (a, len(slots)))
    for sl in slots:
        users = sorted(rip.get(sl, ()))
        print("  slot %#x  used by %d site(s)" % (sl, len(users)))
        for u in users:
            print("     %#x   in %#x" % (u, ctx.owner(u) or 0))
    hits = [(s, "rip/lea") for s in sorted(rip.get(a, ()))]
    hits += [(s, "imm") for s in sorted(imm.get(a, ()))]
    print("== 直接取址（lea / movabs imm64）: %d ==" % len(hits))
    for s, k in hits:
        print("  %#x  %-7s in %#x" % (s, k, ctx.owner(s) or 0))
    return 0


def cmd_locs(ctx, a):
    ex = ctx.fde_extent(a)
    hi = ex[1] if ex else a + 0x120
    votes, seen = {}, {}
    for ins in dis_list(ctx, a, hi):
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                t = ins.address + ins.size + op.mem.disp
                loc = maybe_location(ctx, t)
                if loc:
                    f = loc.split(":")[0]
                    votes[f] = votes.get(f, 0) + 1
                    seen.setdefault(f, loc)
    if not votes:
        print("(no panic::Location referenced)")
        return 0
    for f, n in sorted(votes.items(), key=lambda kv: -kv[1]):
        print("  %-44s x%-3d  e.g. %s" % (f, n, seen[f]))
    return 0


def cmd_jt(ctx, a, base=None, owner=None):
    lo, hi = 0x370000, 0x960000
    if owner:
        ex = ctx.fde_extent(int(owner, 16))
        if ex:
            lo, hi = ex
    entries = []
    n = 0
    while n < 8192:
        try:
            d = struct.unpack("<i", ctx.read(a + 4 * n, 4))[0]
        except Exception:
            break
        tgt = a + d
        if not (0x370000 <= tgt < 0x960000):
            break
        if hi is not None and not (lo <= tgt < hi):
            break
        entries.append(tgt)
        n += 1
    c = Counter(entries)
    print("entries: %d   distinct targets: %d" % (len(entries), len(c)))
    print("-- 按表序（先看这段：`idx` 与判别值的对应全靠顺序，聚合会把顺序丢掉）--")
    for i, t in enumerate(entries[:24]):
        print("  idx %-2d -> %#x" % (i, t))
    if len(entries) > 24:
        print("  ...（共 %d 项）" % len(entries))
    for t, k in sorted(c.items(), key=lambda kv: (-kv[1], kv[0])):
        print("  %#x  x%d" % (t, k))
    return 0


# ---------------------------------------------------------------- 事件 tag 的生产点
# `GraphwarEvent` 的线上判别值一律是 `0x8000000000000000 | n`（n = 0..73，
# 见 tools/event_tags_arm64.py / docs/07 §7.1.1）。**它几乎不是 `movabs` 出来的**：
# LLVM 对 i64::MIN+n 有**四种**写法，每一种都踩过坑 ——
#
#   ① `movabs r, 0x7fffffffffffffff` + `add r, n+1`（n+1 ≤ 63 时常用）
#   ② `movabs r, 0x7fffffffffffffff` + `inc r`         ← tag 恰好 0x8000…0 时
#   ③ `movabs r, 0x8000…|n` 直写                          ← 少数（如 tag 42）
#   ④ `movabs r, 0x8000000000000000` + `add r, n`（`n = 0` 就是基址本身，无后修饰）
#      同族的还有 `or r, n` / `inc r` / `lea r, [r + n]` —— 基址是 **2^63 本身**、
#      不是 i64::MAX，所以 n **从 0 起算**，比 ① 少减一。
#
# 只扫 ③ 会得出「EffectInfo 的生产者查不动」的错结论（archive/155-round-log.md 那轮就是这样：全
# `.text` 只有 2 处 `movabs 0x8000…2c`，且都是消费者）。
#
# ⚠ **形态④ 是 2026-11-10 补上的（dispatch/83）**：补之前这批点全被记进 **tag 0**
# （修前 2235 处），于是凡"某 tag 只有 N 处"的结论都系统性偏低。实例：
# `add_soldier_to_player`（0x474a10）里的 `0x474b8d movabs 0x8000…000` +
# `0x474b97 add rax,0xf` = tag 15，修前被记成 tag 0（docs/161 C10 就是这么登记的）。
#
# 正对照（自带，扫不到就说明扫法坏了）：
#   tag 42 = `0x800000000000002a` 必须命中 0x3f20f0（`GraphwarObstacleManager::queue_destroy_obstacle`）
#   tag 41 = `0x8000000000000029` 必须命中 0x3f1b30（`GraphwarObstacleManager::destroy_obstacle`）
#   tag 44 = `0x800000000000002c` 必须有生产点（`execute_results`，0x472470 体内）**且**
#            命中 2 个消费者（0x4bf09b / 0x4e2dd4）——「N 个生产者」这类负结论的样板
#   tag 15 形态④ = 0x474b8d 必须被认成 tag 15（修形态④之前它落在 tag 0 ⇒ 这条会红）
TAG_SENTINEL = 0x7fffffffffffffff
TAG_BASE = 0x8000000000000000
# 形态④ 判出来的判别值上界：`GraphwarEvent` 只有 0..=73，这里留到 0x7f —— 实测有一处
# `0x3d3656 add rdx, 0x4a`（= 74，graphwar_event_handler.rs 里的 SwissTable 探针，
# 是否真是 GraphwarEvent 判别值**未证**），宁可多记一个桶也不静默丢掉。
TAG_N_MAX = 0x7f


def _tag_of_reg(ins_list, i, reg, span=60):
    """形态①②：从 `movabs reg, i64::MAX` 出发，往后找第一次改 reg 的 `add/inc/lea`，还原 tag。

    返回 `(tag, tag_site, kind)`，kind ∈ {"MAX+add", "MAX+inc"}；认不出返回 None。
    `span=60` 是**历史口径**（LLVM 常把 `add` 排在参数搬运之后），刻意不改小 ——
    改小就会动到既有 tag 计数，见 dispatch/83。
    """
    for j in range(i + 1, min(i + span, len(ins_list))):
        y = ins_list[j]
        if y.regs_write and reg in y.regs_write:
            return None
        if y.operands and y.operands[0].type == X86_OP_REG and y.operands[0].reg == reg:
            if y.mnemonic == "add" and len(y.operands) == 2 and y.operands[1].type == X86_OP_IMM:
                imm = y.operands[1].imm
                if 0 < imm < 74:
                    return (TAG_BASE + imm - 1, y.address, "MAX+add")
            if y.mnemonic == "inc":
                return (TAG_BASE, y.address, "MAX+inc")
    return None


# 少数指令**隐式**写 GPR（这里只列会碰 rax/rdx/rbx/rcx 的）
_IMPLICIT_WRITERS = {
    "mul": ("rax", "rdx"), "imul": ("rax", "rdx"), "div": ("rax", "rdx"),
    "idiv": ("rax", "rdx"), "cpuid": ("rax", "rbx", "rcx", "rdx"),
}
# SysV 调用约定里会被 `call` 踩掉的 caller-saved 寄存器（rbx/rbp/r12–r15 不受影响）
_CALL_CLOBBER = {"rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"}


def _writes_reg(ins, reg):
    """这条指令是否写了 `reg`（显式 dst 在操作数 0，外加少数隐式写 / `call` 的调用约定）。

    ⚠ **不能用 capstone 的 `ins.regs_write`**：本机 5.0.7 对 `add rax, 0xf` 只回
    `[EFLAGS]`（隐式写），显式 dst 不在里面，用它判"谁写了 rax"会永远为假。
    """
    ops = ins.operands
    if ops and ops[0].type == X86_OP_REG and ops[0].reg == reg:
        return True
    name = ins.reg_name(reg)
    if ins.mnemonic == "call":
        return name in _CALL_CLOBBER
    return name in _IMPLICIT_WRITERS.get(ins.mnemonic, ())


def _tag_after_base(ins_list, i, reg, span=12):
    # span = **向后看几条指令**（不含 movabs 本身）；12 是实测够用的窗口。
    """形态④：从 `movabs reg, 0x8000…000` 出发，把紧随其后的那次"补 n"折成 tag。

    只认**第一个写 `reg` 的指令**；它必须是下面四条之一，否则返回 None（该点仍是 tag 0）：

        `add reg, n`  0 ≤ n ≤ TAG_N_MAX          （n = 0 是空操作，等于基址，仍是 tag 0）
        `or  reg, n`  1 ≤ n ≤ TAG_N_MAX          （基址低 63 位全 0 ⇒ or 与 add 等价）
        `inc reg`     ⇒ n = 1
        `lea reg, [reg + n]`  0 ≤ n ≤ TAG_N_MAX

    负立即数（`add reg, -0x11` 把值拉回 i64::MAX 一族）、`sub`/`dec`/`xor`/`mov` 一律不算
    —— 那批是 std 里 `i64::MIN + k` 的算术，不是事件判别值。

    返回 `(tag, tag_site, kind)`，kind ∈ {"BASE+add", "BASE+or", "BASE+inc", "BASE+lea"}。
    """
    for j in range(i + 1, min(i + 1 + span, len(ins_list))):
        y = ins_list[j]
        ops = y.operands
        dst_is_reg = bool(ops) and ops[0].type == X86_OP_REG and ops[0].reg == reg
        if not dst_is_reg:
            if _writes_reg(y, reg):     # 隐式写（div/imul/call…）：值已经不可追
                return None
            continue                    # store / 与 reg 无关的指令：跳过
        if y.mnemonic in ("add", "or") and len(ops) == 2 and ops[1].type == X86_OP_IMM:
            imm = ops[1].imm            # 有符号：负立即数直接判死
            if 0 <= imm <= TAG_N_MAX and (y.mnemonic == "add" or imm > 0):
                return (TAG_BASE + imm, y.address, "BASE+" + y.mnemonic)
            return None
        if y.mnemonic == "inc":
            return (TAG_BASE + 1, y.address, "BASE+inc")
        if (y.mnemonic == "lea" and len(ops) == 2 and ops[1].type == X86_OP_MEM
                and ops[1].mem.base == reg and ops[1].mem.index == 0
                and 0 <= ops[1].mem.disp <= TAG_N_MAX):
            return (TAG_BASE + ops[1].mem.disp, y.address, "BASE+lea")
        return None                     # cmp/test/mov/sub/dec/…：不是 tag 构造
    return None


def cmd_tags(ctx, only=None, show_uses=False, json_path=None, tsv_path=None,
             counts_path=None):
    """扫全 `.text`：`0x8000000000000000|n` 的**四种** materialize 形态。

    形态见本节开头注释；每个 tag 给出生产点地址 + 落在哪个函数 + 模块归属。
    一条 `movabs 0x8000…000` 若后面没有"补 n"（形态④ 判不出），仍按形态③ 记进 tag 0
    —— 与补形态④ 之前完全一致，所以**只有形态④ 那批点会换桶**。

    `--tsv` / `--counts` 写夹具时每个站点额外带一列 `tag_legacy`：**补形态④ 之前**
    这个点会落在哪个 tag（= 2^63，即 tag 0）。于是"修前/修后"对照可以从同一份扫描
    直接算出来，不需要第二遍扫描，也不需要把老版本留在仓库里。
    """
    found = {}          # tag -> [(site, tag_site, kind, tag_legacy)]
    for lo, hi in ctx.exec_ranges:
        ins_list = dis_list(ctx, lo, hi)
        for i, ins in enumerate(ins_list):
            if ins.mnemonic != "movabs" or len(ins.operands) != 2:
                continue
            d, s = ins.operands
            if d.type != X86_OP_REG or s.type != X86_OP_IMM:
                continue
            v = s.imm & 0xFFFFFFFFFFFFFFFF
            if v == TAG_SENTINEL:                       # 形态①②（基址 i64::MAX）
                got = _tag_of_reg(ins_list, i, d.reg)
                if got:
                    found.setdefault(got[0], []).append((ins.address, got[1], got[2], got[0]))
            elif v == TAG_BASE:                         # 形态④（基址 2^63）或形态③ 的 tag 0
                got = _tag_after_base(ins_list, i, d.reg)
                if got:
                    found.setdefault(got[0], []).append((ins.address, got[1], got[2], v))
                else:
                    found.setdefault(v, []).append((ins.address, ins.address, "movabs", v))
            elif v >> 32 == 0x80000000:                 # 形态③（直写 0x8000…|n，n≠0）
                found.setdefault(v, []).append((ins.address, ins.address, "movabs", v))
    print("== 全 .text 的 `0x8000…|n` materialize 点（四种形态）：%d 个 tag ==" % len(found))
    for tag in sorted(found):
        n = tag & 0x7FFFFFFF
        sites = found[tag]
        if only is not None and n != only:
            continue
        if only is None and n > 73:      # `GraphwarEvent` 只有 0..=73；更大的值是 Vec 容量哨兵之类的噪声
            continue
        print("\n tag %-3d %#018x   %d 处" % (n, tag, len(sites)))
        owners = Counter(ctx.owner(a) for a, _, _, _ in sites)
        for o, k in sorted(owners.items(), key=lambda kv: -kv[1]):
            addrs = " ".join("%#x" % a for a, _, _, _ in sites if ctx.owner(a) == o)
            loc = ""
            if o:
                ex = ctx.fde_extent(o)
                locs = _owner_loc(ctx, o, ex[1] if ex else o + 0x400)
                loc = ("  " + locs) if locs else ""
            print("   in %#x  x%-2d %s%s" % (o or 0, k, addrs, loc))
    kinds = Counter(k for v in found.values() for _, _, k, _ in v)
    print("\n== 形态计数（判据见本节开头注释）==")
    for k in ("MAX+add", "MAX+inc", "movabs", "BASE+add", "BASE+or", "BASE+inc", "BASE+lea"):
        print("  %-9s x%d" % (k, kinds.get(k, 0)))
    print("  合计 %d 处（含 tag>73 的桶）" % sum(len(v) for v in found.values()))
    # ---------------------------------------------------------------- 正对照
    # 三条老对照（形态①②③）+ 一条形态④ 的对照；任何一条红都说明扫法坏了，不是"没生产者"。
    def has(tag, addr):
        return any(a == addr for a, _, _, _ in found.get(tag, []))

    ok42 = any(ctx.owner(a) == 0x3F20F0 for a, _, _, _ in found.get(0x800000000000002A, []))
    ok41 = any(ctx.owner(a) == 0x3F1B30 for a, _, _, _ in found.get(0x8000000000000029, []))
    t44 = found.get(0x800000000000002C, [])
    ok44_prod = any(ctx.owner(a) == 0x472470 for a, _, _, _ in t44)   # execute_results
    ok44_cons = has(0x800000000000002C, 0x4BF09B) and has(0x800000000000002C, 0x4E2DD4)
    ok44 = ok44_prod and ok44_cons
    ok15 = any(a == 0x474B8D and k.startswith("BASE+")
               for a, _, k, _ in found.get(0x800000000000000F, []))
    print("\n== 正对照 ==")
    print("  tag 42 @0x3f20f0 (queue_destroy_obstacle)   : %s" % ("OK" if ok42 else "**失败**"))
    print("  tag 41 @0x3f1b30 (destroy_obstacle)          : %s" % ("OK" if ok41 else "**失败**"))
    print("  tag 44 生产点 @0x472470 + 消费者 0x4bf09b/0x4e2dd4: %s%s"
          % ("OK" if ok44 else "**失败**",
             "" if ok44 else "（生产点 %s / 消费者 %s）"
             % ("有" if ok44_prod else "缺", "有" if ok44_cons else "缺")))
    print("  tag 15 形态④ 0x474b8d (add_soldier_to_player)  : %s" % ("OK" if ok15 else "**失败**"))
    if json_path:
        import json
        json.dump({"so": ctx.so,
                   "tags": {"%d" % t: [[a, b, k, lg] for a, b, k, lg in v]
                            for t, v in sorted(found.items())},
                   "counts": {"%d" % t: len(v) for t, v in sorted(found.items())},
                   "kinds": dict(kinds)}, open(json_path, "w"), indent=1)
        print("\nJSON 已写入 %s" % json_path)
    if tsv_path:
        # 每站点一行：tsv 里 `tag_legacy` = 补形态④ 之前落进的桶（2^63 = tag 0）
        with open(tsv_path, "w") as fh:
            fh.write("# 由 tools/addr_probe.py tags --tsv 生成，不要手改（重跑会覆盖）。\n")
            fh.write("# so=%s\n" % ctx.so)
            fh.write("# 形态：MAX+add/MAX+inc = ①②；movabs = ③；BASE+add/or/inc/lea = ④\n")
            fh.write("# tag_legacy = 补形态④ 之前该点落进的 tag（只有形态④ 那批会 != tag）\n")
            fh.write("tag\tn\tkind\tsite\ttag_site\ttag_legacy\tn_legacy\towner\tmodules\n")
            for t, v in sorted(found.items()):
                for a, b, k, lg in v:
                    o = ctx.owner(a) or 0
                    ex = ctx.fde_extent(o) if o else None
                    locs = _owner_loc(ctx, o, ex[1] if ex else o + 0x400) if o else ""
                    fh.write("%d\t%d\t%s\t%#x\t%#x\t%d\t%d\t%#x\t%s\n"
                             % (t, t & 0x7FFFFFFF, k, a, b, lg, lg & 0x7FFFFFFF,
                                o, locs.replace("\t", " ")))
        print("站点 TSV 已写入 %s" % tsv_path)
    if counts_path:
        # 每 tag 一行：修前（按 tag_legacy 计）/ 修后 / 差
        before = Counter(lg for v in found.values() for _, _, _, lg in v)
        after = Counter()
        for t, v in found.items():
            after[t] += len(v)
        with open(counts_path, "w") as fh:
            fh.write("# 由 tools/addr_probe.py tags --counts 生成，不要手改（重跑会覆盖）。\n")
            fh.write("# so=%s  （口径：四种形态；before = 只认形态①②③）\n" % ctx.so)
            fh.write("n\tbefore\tafter\tdelta\n")
            for t in sorted(set(before) | set(after)):
                fh.write("%d\t%d\t%d\t%+d\n" % (t & 0x7FFFFFFF, before.get(t, 0),
                                                  after.get(t, 0), after.get(t, 0) - before.get(t, 0)))
        print("计数 TSV 已写入 %s" % counts_path)
    return 0 if (ok42 and ok41 and ok44 and ok15) else 1


def _owner_loc(ctx, start, end):
    """函数里出现次数最多的 `src/*.rs`（panic::Location 投票），用于模块归属。"""
    votes = Counter()
    for ins in dis_list(ctx, start, end):
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                t = ins.address + ins.size + op.mem.disp
                loc = maybe_location(ctx, t)
                if loc:
                    votes[loc.split(":")[0]] += 1
    return ", ".join("%s x%d" % (f, n) for f, n in votes.most_common(2))


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--so", default=SO_DEFAULT)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("fde"); p.add_argument("va")
    p.set_defaults(fn=lambda c, x: cmd_fde(c, int(x.va, 16)))
    p = sub.add_parser("dis"); p.add_argument("va")
    p.add_argument("--length", type=lambda s: int(s, 0), default=None)
    p.set_defaults(fn=lambda c, x: cmd_dis(c, int(x.va, 16), x.length))
    p = sub.add_parser("str"); p.add_argument("va")
    p.add_argument("--n", type=lambda s: int(s, 0), default=32)
    p.set_defaults(fn=lambda c, x: cmd_str(c, int(x.va, 16), x.n))
    p = sub.add_parser("callers"); p.add_argument("va")
    p.set_defaults(fn=lambda c, x: cmd_callers(c, int(x.va, 16)))
    p = sub.add_parser("locs"); p.add_argument("va")
    p.set_defaults(fn=lambda c, x: cmd_locs(c, int(x.va, 16)))
    p = sub.add_parser("jt"); p.add_argument("va")
    p.add_argument("--owner", default=None, help="只保留落在该函数 FDE 范围内的目标")
    p.set_defaults(fn=lambda c, x: cmd_jt(c, int(x.va, 16), None, x.owner))
    p = sub.add_parser("tags", help="扫全 .text 的事件 tag 生产点（四种 materialize 形态）")
    p.add_argument("--only", type=lambda s: int(s, 0), default=None, help="只看某个判别值 n")
    p.add_argument("--json", default=None, metavar="PATH",
                   help="把扫描结果（tag → 站点/形态 + 形态计数）写成 JSON，供 fixtures 复扫对拍")
    p.add_argument("--tsv", default=None, metavar="PATH", help="每站点一行写 TSV（带 tag_legacy 列）")
    p.add_argument("--counts", default=None, metavar="PATH", help="每 tag 一行写 TSV：修前/修后/差")
    p.set_defaults(fn=lambda c, x: cmd_tags(c, x.only, json_path=x.json,
                                            tsv_path=x.tsv, counts_path=x.counts))
    args = ap.parse_args(argv)
    return args.fn(Ctx(args.so), args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
