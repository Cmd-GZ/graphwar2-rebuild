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
# 见 tools/event_tags_arm64.py / docs/06 §24.6(c)）。**它几乎不是 `movabs` 出来的**：
# LLVM 对 i64::MIN+n 有两种写法，两种都踩过坑 ——
#
#   ① `movabs r, 0x7fffffffffffffff` + `add r, n+1`（n+1 ≤ 63 时常用）
#   ② `movabs r, 0x7fffffffffffffff` + `inc r`         ← tag 恰好 0x8000…0 时
#   ③ `movabs r, 0x8000…|n` 直写                          ← 少数（如 tag 42）
#
# 只扫 ③ 会得出「EffectInfo 的生产者查不动」的错结论（上一轮就是这样：全 `.text`
# 只有 2 处 `movabs 0x8000…2c`，且都是消费者）。本命令把三种形态一起扫，
# 并给出每个生产点落在哪个函数里。
#
# 正对照（自带，扫不到就说明扫法坏了）：
#   tag 42 = `0x800000000000002a` 必须命中 0x3f20f0（`GraphwarObstacleManager::queue_destroy_obstacle`）
#   tag 41 = `0x8000000000000029` 必须命中 0x3f1b30（`GraphwarObstacleManager::destroy_obstacle`）
#   tag 44 = `0x800000000000002c` 必须同时命中生产点（execute_results）与 2 个消费者
TAG_SENTINEL = 0x7fffffffffffffff


def _tag_of_reg(ins_list, i, reg, span=60):
    """从 `movabs reg, i64::MAX` 出发，往后找第一次改 reg 的 `add/inc/lea`，还原 tag。"""
    for j in range(i + 1, min(i + span, len(ins_list))):
        y = ins_list[j]
        if y.regs_write and reg in y.regs_write:
            return None
        if y.operands and y.operands[0].type == X86_OP_REG and y.operands[0].reg == reg:
            if y.mnemonic == "add" and len(y.operands) == 2 and y.operands[1].type == X86_OP_IMM:
                imm = y.operands[1].imm
                if 0 < imm < 74:
                    return (0x8000000000000000 + imm - 1, y.address)
            if y.mnemonic == "inc":
                return (0x8000000000000000, y.address)
    return None


def cmd_tags(ctx, only=None, show_uses=False):
    """扫全 `.text`：`0x8000000000000000|n` 的三种 materialize 形态 + 直写 movabs。"""
    found = {}          # tag -> [(site, tag_site, kind)]
    for lo, hi in ctx.exec_ranges:
        ins_list = dis_list(ctx, lo, hi)
        for i, ins in enumerate(ins_list):
            if ins.mnemonic != "movabs" or len(ins.operands) != 2:
                continue
            d, s = ins.operands
            if d.type != X86_OP_REG or s.type != X86_OP_IMM:
                continue
            v = s.imm & 0xFFFFFFFFFFFFFFFF
            if v == TAG_SENTINEL:
                got = _tag_of_reg(ins_list, i, d.reg)
                if got:
                    found.setdefault(got[0], []).append((ins.address, got[1], "MAX+n"))
            elif v >> 32 == 0x80000000:
                found.setdefault(v, []).append((ins.address, ins.address, "movabs"))
    print("== 全 .text 的 `0x8000…|n` materialize 点（三种形态）：%d 个 tag ==" % len(found))
    for tag in sorted(found):
        n = tag & 0x7FFFFFFF
        sites = found[tag]
        if only is not None and n != only:
            continue
        if only is None and n > 73:      # `GraphwarEvent` 只有 0..=73；更大的值是 Vec 容量哨兵之类的噪声
            continue
        print("\n tag %-3d %#018x   %d 处" % (n, tag, len(sites)))
        owners = Counter(ctx.owner(a) for a, _, _ in sites)
        for o, k in sorted(owners.items(), key=lambda kv: -kv[1]):
            addrs = " ".join("%#x" % a for a, _, _ in sites if ctx.owner(a) == o)
            loc = ""
            if o:
                ex = ctx.fde_extent(o)
                locs = _owner_loc(ctx, o, ex[1] if ex else o + 0x400)
                loc = ("  " + locs) if locs else ""
            print("   in %#x  x%-2d %s%s" % (o or 0, k, addrs, loc))
    # 正对照
    ok42 = any(ctx.owner(a) == 0x3f20f0 for a, _, _ in found.get(0x800000000000002a, []))
    ok41 = any(ctx.owner(a) == 0x3f1b30 for a, _, _ in found.get(0x8000000000000029, []))
    print("\n== 正对照 ==")
    print("  tag 42 @0x3f20f0 (queue_destroy_obstacle): %s" % ("OK" if ok42 else "**失败**"))
    print("  tag 41 @0x3f1b30 (destroy_obstacle)      : %s" % ("OK" if ok41 else "**失败**"))
    return 0 if (ok42 and ok41) else 1


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
    p = sub.add_parser("tags", help="扫全 .text 的事件 tag 生产点（三种 materialize 形态）")
    p.add_argument("--only", type=lambda s: int(s, 0), default=None, help="只看某个判别值 n")
    p.set_defaults(fn=lambda c, x: cmd_tags(c, x.only))
    args = ap.parse_args(argv)
    return args.fn(Ctx(args.so), args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
