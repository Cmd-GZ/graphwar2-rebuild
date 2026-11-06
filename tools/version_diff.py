#!/usr/bin/env python3
"""version_diff.py — 逐节 + 逐函数对比两版 libgraphwarrust.so。

存在的理由（为什么不能直接 `cmp`）：
  这两个 build 之间 **函数布局被整体重排**（同一段代码在 08-27/08-28 里位于完全不同的
  偏移，见 §"为什么不能逐字节 diff"）。所以：
    * 文件尾可能残留**没有任何 section 覆盖的死数据**（08-27 就有 801,936 字节）；
    * `.text` 的 86% 字节仍是同一批内容，但 rel32/disp32 全变了 ⇒ 逐字节 diff 全是噪声；
    * `.rodata` 的合并串表被重新排序 ⇒ 逐字节 diff 全是边界噪声。
  本脚本的做法：
    1. 解析 section 表，报告每节大小差 + 文件尾死数据（< 最后一个 section 之后、不属于任何
       section、也不在任何 PT_LOAD 里的字节）。
    2. 用 `readelf --debug-dump=frames-interp` 的 FDE 表当**函数边界**（两版同源，可比）。
    3. 每个函数做**操作数级归一化**：立即数 → `<imm>`，RIP 相对 → `[rip]`，其余内存 disp → `<d>`。
       归一化后逐条指令比较 ⇒ 只有"真正改了指令形态"的函数才会不匹配。
    4. 另开一条**小立即数通道**：把 < 0x10000 的立即数和非 RIP 的 disp 按顺序留下。
       形态相同、小立即数却不同的函数 = "同结构、换了常数"，这类是真改动的高价值线索。
    5. `.dynsym` / `.dynstr` 集合对比 = 新增/消失的导入符号（新功能的强证据）。
    6. `.rela.dyn` 的 R_X86_64_RELATIVE + 槽位 +8 的 len ⇒ 精确边界的 Rust `&str` 字面量，
       对比**字面量集合**（不受串表重排影响）。

地址口径：输出一律 **ELF 虚地址**（08-28 的地址只对 08-28 有效）。Ghidra/REA 地址 = +0x100000。

依赖：capstone（用仓库 venv：`../.venv/bin/python tools/version_diff.py ...`）、`readelf`。

用法：
    ../.venv/bin/python tools/version_diff.py \\
        ../../.reference/steam/2026-08-27/libgraphwarrust.so \\
        ../../.reference/steam/2026-08-28/libgraphwarrust.so
    ... --max-list 40        # 每个清单最多打多少行
"""
import argparse
import collections
import hashlib
import re
import struct
import subprocess
import sys

from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86 import X86_OP_REG, X86_OP_IMM, X86_OP_MEM, X86_REG_RIP

FDE_RE = re.compile(r"FDE cie=[0-9a-f]+ pc=([0-9a-f]+)\.\.([0-9a-f]+)")
R_X86_64_RELATIVE = 8
SMALL = 0x10000          # 立即数"小"的阈值：小于它的值当作可能变了的常数留下


# ---------------------------------------------------------------- ELF plumbing
def elf_sections(path):
    d = open(path, "rb").read()
    shoff = struct.unpack_from("<Q", d, 0x28)[0]
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", d, 0x3A)
    o = shoff + shstrndx * shentsize
    _, _, _, _, sof, ssz, _, _, _, _ = struct.unpack_from("<IIQQQQIIQQ", d, o)
    strtab = d[sof:sof + ssz]
    secs = []
    for i in range(shnum):
        o = shoff + i * shentsize
        nm, ty, fl, ad, off, sz, lk, inf, al, en = struct.unpack_from("<IIQQQQIIQQ", d, o)
        e = strtab.find(b"\0", nm)
        secs.append(dict(name=strtab[nm:e].decode(), type=ty, flags=fl, addr=ad,
                         off=off, size=sz, entsize=en))
    phoff = struct.unpack_from("<Q", d, 0x20)[0]
    phentsize, phnum = struct.unpack_from("<HH", d, 0x36)
    loads = []
    for i in range(phnum):
        o = phoff + i * phentsize
        t, fl, off, va, pa, fsz, msz, al = struct.unpack_from("<IIQQQQQQ", d, o)
        if t == 1:
            loads.append((va, off, fsz))
    return d, secs, loads, shoff + shentsize * shnum


def sec_by_name(secs, name):
    for s in secs:
        if s["name"] == name:
            return s
    raise KeyError(name)


def va2off(loads, va):
    for v, o, sz in loads:
        if v <= va < v + sz:
            return o + (va - v)
    return None


def fde_ranges(path):
    try:
        out = subprocess.run(["readelf", "--debug-dump=frames-interp", path],
                             capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError) as e:
        raise SystemExit("readelf 不可用：%s" % e)
    return [(int(m.group(1), 16), int(m.group(2), 16))
            for m in (FDE_RE.search(l) for l in out.splitlines()) if m]


# ------------------------------------------------------- instruction normalize
def normalize(cs, body, va):
    """-> (shape_text, small_imms) 。shape 丢掉一切地址/偏移/立即数值。"""
    shapes, smalls, n = [], [], 0
    off = 0
    while off < len(body):
        i = next(cs.disasm(body[off:], va + off, count=1), None)
        if i is None:
            shapes.append("<db %02x>" % body[off])
            off += 1
            continue
        off += i.size
        n += 1
        ops = []
        for op in i.operands:
            if op.type == X86_OP_REG:
                ops.append(i.reg_name(op.reg))
            elif op.type == X86_OP_IMM:
                ops.append("<imm>")
                if abs(op.imm) < SMALL:
                    smalls.append(op.imm)
            elif op.type == X86_OP_MEM:
                m = op.mem
                if m.base == X86_REG_RIP:
                    ops.append("[rip]")
                else:
                    ops.append("[%s+%s*%d]" % (
                        i.reg_name(m.base) if m.base else "",
                        i.reg_name(m.index) if m.index else "", m.scale))
                    if m.disp and abs(m.disp) < SMALL:
                        smalls.append(m.disp)
        shapes.append("%s %s" % (i.mnemonic, ",".join(ops)))
    return "\n".join(shapes), tuple(smalls), n


def build_functions(cs, path, ranges):
    d, secs, loads, _ = elf_sections(path)
    out = []
    for va, end in ranges:
        off, eoff = va2off(loads, va), va2off(loads, end - 1)
        if off is None or eoff is None:
            continue
        body = d[off:eoff + 1]
        shape, smalls, n = normalize(cs, body, va)
        out.append(dict(va=va, size=end - va, ninstr=n, smalls=smalls,
                        shape=hashlib.sha1(shape.encode()).digest(), body_sha=hashlib.sha1(body).digest()))
    return out


# ------------------------------------------------------------------- analyses
def sec_report(da, sa, ea, db, sb, eb, out):
    out("## 1. 节表对比")
    out("%-20s %12s %12s %10s" % ("section", "old size", "new size", "delta"))
    na = {s["name"]: s for s in sa}
    nb = {s["name"]: s for s in sb}
    tot = 0
    for k in [s["name"] for s in sa]:
        a = na[k]["size"]
        b = nb.get(k, {}).get("size", 0)
        tot += b - a
        out("%-20s %12d %12d %+10d%s" % (k or "(null)", a, b, b - a, "  <<<" if a != b else ""))
    out("%-20s %12d %12d %+10d" % ("SUM", sum(s["size"] for s in sa),
                                   sum(s["size"] for s in sb), tot))
    out("")
    out("文件长度：old=%d  new=%d  delta=%+d" % (len(da), len(db), len(db) - len(da)))
    out("section 表结束后（= 无任何 section 覆盖的文件尾）：")
    for tag, d, e in (("old", da, ea), ("new", db, eb)):
        tail = d[e:]
        mapped = "mapped" if tail and any(va2off([]) is not None for _ in ()) else ""
        out("  %s: shtab_end=%d  file_size=%d  死数据=%d 字节" % (tag, e, len(d), len(tail)))
    if len(db) != eb:
        out("  ⚠️ new 版文件尾有未覆盖字节")
    out("")


def dead_tail_provenance(old_path, new_path, out):
    """证明 old 的文件尾不是它自己的数据：和同组更老的版本逐字节比。"""
    out("## 2. 文件尾死数据的来源")
    out("（把 old 的尾部与同目录下更早版本的同偏移字节比较；相同 ⇒ 不是本版构建产物）")
    import os
    base = os.path.dirname(os.path.abspath(old_path))
    parent = os.path.dirname(base)
    A = open(old_path, "rb").read()
    out("  %s 总长 %d" % (os.path.basename(old_path), len(A)))
    sibs = []
    for name in sorted(os.listdir(parent)):
        p = os.path.join(parent, name, "libgraphwarrust.so")
        if os.path.isfile(p) and os.path.abspath(p) != os.path.abspath(old_path):
            sibs.append((name, p))
    if not sibs:
        out("  （同目录下没有其他版本可比较；把 MANIFEST 里各版本放在同级子目录即可自动比较）")
    for name, p in sibs:
        B = open(p, "rb").read()
        n = 0
        for i in range(1, min(len(A), len(B)) + 1):
            if A[-i] != B[-i]:
                break
            n += 1
        out("    与 %-12s (len %d) 相同后缀 %d 字节%s"
            % (name, len(B), n, "   <-- 整段尾部逐字节相同" if n >= len(A) - 10218976 else ""))
    out("")


def function_report(old_path, new_path, out, limit):
    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    A = build_functions(cs, old_path, fde_ranges(old_path))
    B = build_functions(cs, new_path, fde_ranges(new_path))
    out("## 3. 函数级对比（FDE 边界 / 操作数归一化）")
    out("  函数数：old=%d (%s)  new=%d (%s)" % (len(A), "08-27", len(B), "08-28"))
    ca = collections.Counter(x["shape"] for x in A)
    cb = collections.Counter(x["shape"] for x in B)
    shared = set(ca) & set(cb)
    out("  归一化形态完全一致（只差地址/偏移）：old %d 函数 / new %d 函数"
        % (sum(ca[h] for h in shared), sum(cb[h] for h in shared)))
    # surplus instances
    cnt = collections.Counter()
    surplusB = []
    for x in B:
        cnt[x["shape"]] += 1
        if cnt[x["shape"]] > ca.get(x["shape"], 0):
            surplusB.append(x)
    cnt = collections.Counter()
    surplusA = []
    for x in A:
        cnt[x["shape"]] += 1
        if cnt[x["shape"]] > cb.get(x["shape"], 0):
            surplusA.append(x)
    # same shape is already handled; look for shape-equal + smalls-differ
    bysmall = collections.defaultdict(list)
    for x in surplusA:
        bysmall[(x["shape"], x["smalls"])].append(x)
    const_changed, newB = [], []
    for y in surplusB:
        same_shape = [x for x in surplusA if x["shape"] == y["shape"]]
        if same_shape:
            const_changed.append((same_shape[0], y))
        else:
            newB.append(y)
    out("  surplus：new 侧 %d / old 侧 %d" % (len(surplusB), len(surplusA)))
    out("  形态相同但小立即数不同（真改动候选）：%d" % len(const_changed))
    for x, y in const_changed[:limit]:
        out("     old=0x%06x  new=0x%06x size=%-5d old_smalls=%s new_smalls=%s"
            % (x["va"], y["va"], y["size"], x["smalls"][:8], y["smalls"][:8]))
    out("  形态也不相同（新增/结构改变）：%d" % len(newB))
    for y in sorted(newB, key=lambda t: t["va"])[:limit]:
        out("     new=0x%06x size=%-6d insns=%d" % (y["va"], y["size"], y["ninstr"]))
    used = {id(x) for x, _ in const_changed}
    gone = [x for x in surplusA if x["shape"] not in {y["shape"] for y in newB} and id(x) not in used]
    out("  只在 old 侧出现（消失/结构改变）：%d" % len(gone))
    for x in sorted(gone, key=lambda t: t["va"])[:limit]:
        out("     old=0x%06x size=%-6d insns=%d" % (x["va"], x["size"], x["ninstr"]))
    return A, B


def dynsym_report(old_path, new_path, out):
    out("## 4. 动态符号（导入/导出）差异")

    def syms(path):
        d, secs, loads, _ = elf_sections(path)
        s = sec_by_name(secs, ".dynsym")
        st = sec_by_name(secs, ".dynstr")
        names = []
        for i in range(s["size"] // 24):
            nm = struct.unpack_from("<I", d, s["off"] + i * 24)[0]
            e = d.find(b"\0", st["off"] + nm)
            names.append(d[st["off"] + nm:e].decode("utf-8", "replace"))
        return names

    a, b = syms(old_path), syms(new_path)
    out("  条目数：old=%d new=%d" % (len(a), len(b)))
    added = sorted(set(b) - set(a))
    removed = sorted(set(a) - set(b))
    out("  只在 new：%s" % (added or "（无）"))
    out("  只在 old：%s" % (removed or "（无）"))
    out("")


def literal_report(old_path, new_path, out, limit):
    out("## 5. 精确边界的 Rust 字面量（.rela.dyn R_X86_64_RELATIVE + 槽位+8 的 len）")

    def lits(path):
        d, secs, loads, _ = elf_sections(path)
        ro = sec_by_name(secs, ".rodata")
        rel = sec_by_name(secs, ".rela.dyn")
        got = set()
        for i in range(rel["size"] // 24):
            off, info, add = struct.unpack_from("<QQq", d, rel["off"] + i * 24)
            if (info & 0xFFFFFFFF) != R_X86_64_RELATIVE:
                continue
            if not (ro["addr"] <= add < ro["addr"] + ro["size"]):
                continue
            so = va2off(loads, off + 8)
            if so is None:
                continue
            ln = struct.unpack_from("<Q", d, so)[0]
            p = va2off(loads, add)
            if not (1 <= ln <= 4096) or p is None or p + ln > len(d):
                continue
            b = d[p:p + ln]
            if all(32 <= c < 127 or c in (9, 10) for c in b):
                got.add(b)
        return got

    a, b = lits(old_path), lits(new_path)
    out("  可打印字面量：old 去重 %d 条 / new 去重 %d 条" % (len(a), len(b)))
    out("  新增 %d 条：" % len(b - a))
    for s in sorted(b - a)[:limit]:
        out("     %r" % s.decode("utf-8", "replace"))
    out("  消失 %d 条：" % len(a - b))
    for s in sorted(a - b)[:limit]:
        out("     %r" % s.decode("utf-8", "replace"))
    out("")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("old")
    ap.add_argument("new")
    ap.add_argument("--max-list", type=int, default=25)
    args = ap.parse_args()
    buf = []
    out = buf.append
    da, sa, la, ea = elf_sections(args.old)
    db, sb, lb, eb = elf_sections(args.new)
    sec_report(da, sa, ea, db, sb, eb, out)
    dead_tail_provenance(args.old, args.new, out)
    function_report(args.old, args.new, out, args.max_list)
    dynsym_report(args.old, args.new, out)
    literal_report(args.old, args.new, out, args.max_list)
    sys.stdout.write("\n".join(buf) + "\n")


if __name__ == "__main__":
    main()
