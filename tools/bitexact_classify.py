#!/usr/bin/env python3
"""bitexact_classify.py —— 439 个「观测」级函数的**静态可隔离执行性分类器**（T13 预注册）。

任务书 `.agent-work/specs/T13-bit-exact-sweep.md` ① 要求：先写一个静态分类器（capstone 扫函数体），
把 439 个已实现但**没对拍过**的函数分成三档，并给出**可复核的理由**（哪条指令 / 哪个符号）。
本脚本只做静态判定，不比任何输出；产物写进 `fixtures/bit_exact_plan.txt`，**先 commit 再跑**。

## 三档

    executable            只吃标量/浮点（或只写调用方给的输出缓冲）；无全局写、无 TLS、无引擎派发
    needs_state           要一个对象/全局状态，但那状态是**纯数据**（可造字节，如 `&mut self` 里的表）
    isolated_impossible   本体或热路径传递调用里出现下面任一致命项

## 致命项（判据 1-6，`reason` 列写明是哪条 + 指令地址）

1. `engine_dispatch` —— `call *off(%reg)`，而 `%reg` 是从**运行时才填的全局槽**读出来的指针。
   实测这类就是 gdext 的引擎接口表：`0x8abca0`（`draw_arc` 包装）里
   `mov 0x10e9a0(%rip),%r15  # 9ba6b0` → `movzbl 0x14f0(%r15)` → …，隔离环境里 `[0x9ba6b0]` 恒 0。
2. `runtime_ptr` —— `call *0x…(%rip)` 的槽**连重定位都没有**（`.rela.dyn` 里查不到），
   只可能由 GDExtension 入口在运行时写入。
3. `tls` —— 任何 `fs:` 段内存操作数，或直接调用 `__tls_get_addr`（`thread_rng` / `std::io` 在这）。
4. `sys_io` —— 直接调用 `read/write/socket/send/recv/open/statx/mmap64/clock_gettime/syscall/…`。
5. `thread` —— `pthread_create/join/detach/key_*/setspecific/self`。
6. `unwind` —— `_Unwind_*` / `__cxa_thread_atexit_impl`。

**冷路径不算**：`call *slot` 的下一条指令是 `int3`/`ud2` 的，是 panic/`print_error` 收尾
（实测 `0x925660` = 分配失败处理：`call *0x9ba698; int3`）。这种边**不参与传递**，
只在 `cold_calls` 里记数——它不影响"喂对输入就能跑"。

## 静态调用图的解析（为什么可信）

* `call rel32` → 目标在 `.text` 里：内部函数；在 `.plt` 里：按桩里的 `jmp *0x…(%rip)` 反查导入名。
* `call *0x…(%rip)` → 槽地址；`.rela.dyn` 的 `R_X86_64_RELATIVE` 有 addend ⇒ 直接解析成 `.text` 目标
  （实测 **8231** 条这种重定位，`.got` 基本上就是内部函数指针表）；`GLOB_DAT/JUMP_SLOT` ⇒ libc 导入名。
* `call *%reg` → 回溯最多 15 条找 `mov %reg, [%rip+G]`，得到 G 再同上判。
* 其余 `call *%reg` ⇒ `indirect_unknown`，**不许假装能跑**：按 `needs_state` 处理并写明。

## 用法
    ../.venv/bin/python tools/bitexact_classify.py --out fixtures/bit_exact_plan.txt
    ../.venv/bin/python tools/bitexact_classify.py --json /tmp/c.json --show executable
"""

import argparse
import bisect
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
MAIN = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(HERE))))
if not os.path.exists(os.path.join(MAIN, ".reference")):
    MAIN = os.path.dirname(ROOT)
os.environ.setdefault("COVERAGE_MATRIX_UPSTREAM", os.path.join(MAIN, ".reference", "upstream"))
sys.path.insert(0, os.path.join(MAIN, "tools"))

import coverage_matrix as CM  # noqa: E402  （复用 Elf/Location/归属，不改它）
from capstone import Cs, CS_ARCH_X86, CS_MODE_64  # noqa: E402
from capstone.x86 import (  # noqa: E402
    X86_OP_IMM, X86_OP_MEM, X86_OP_REG, X86_REG_RIP, X86_REG_FS, X86_REG_GS,
    X86_REG_RDI, X86_REG_RSI, X86_REG_RDX, X86_REG_RCX, X86_REG_R8, X86_REG_R9,
    X86_REG_RAX, X86_REG_EAX, X86_REG_XMM0, X86_REG_RSP, X86_REG_RBP,
    X86_REG_RBX, X86_REG_R12, X86_REG_R13, X86_REG_R14, X86_REG_R15,
)

SYS_IO = set("""
read write writev open open64 close lseek64 statx
send recv sendto recvfrom sendmsg recvmsg
socket bind listen connect accept4 getpeername getsockname
setsockopt getsockopt poll epoll_create1 epoll_ctl epoll_wait
eventfd ioctl shutdown fcntl mmap64 munmap mprotect abort signal sigaltstack syscall
clock_gettime clock_nanosleep sched_yield pause gettid getenv getcwd realpath
readlink unlink dlsym dl_iterate_phdr getauxval sysconf
getaddrinfo freeaddrinfo __res_init gai_strerror stderr isatty fflush
gnu_get_libc_version __xpg_strerror_r pthread_atfork
""".split())
THREAD_IMP = set("""pthread_create pthread_join pthread_detach pthread_self
pthread_key_create pthread_key_delete pthread_setspecific
pthread_attr_init pthread_attr_destroy pthread_attr_setstacksize
pthread_attr_getstack pthread_attr_getguardsize pthread_getattr_np
pthread_setname_np""".split())
UNWIND_IMP = {"_Unwind_Resume", "_Unwind_RaiseException", "__cxa_thread_atexit_impl"}
DATA_SECS = (".got", ".got.plt", ".bss", ".data", ".data.rel.ro", ".tdata", ".tbss")


# ---------------------------------------------------------------- ELF
def elf_sections(data):
    e_shoff, = struct.unpack_from("<Q", data, 0x28)
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", data, 0x3A)
    secs = []
    for i in range(e_shnum):
        o = e_shoff + i * e_shentsize
        nameoff, typ, flags, addr, offset, size = struct.unpack_from("<IIQQQQ", data, o)
        secs.append(dict(nameoff=nameoff, typ=typ, addr=addr, off=offset, size=size))
    st = secs[e_shstrndx]
    for s in secs:
        end = data.index(b"\0", st["off"] + s["nameoff"])
        s["name"] = data[st["off"] + s["nameoff"]:end].decode()
    return {s["name"]: s for s in secs}


def build_maps(data, byname):
    """返回 (slot→libc 名, .plt 桩→导入名, slot→('text',addr)/('runtime',None))。"""
    dsym, dstr = byname[".dynsym"], byname[".dynstr"]
    syms = []
    for i in range(dsym["size"] // 24):
        o = dsym["off"] + i * 24
        nm, info, other, shndx, value, size = struct.unpack_from("<IBBHQQ", data, o)
        end = data.index(b"\0", dstr["off"] + nm)
        syms.append(data[dstr["off"] + nm:end].decode().split("@")[0])
    text = byname[".text"]
    tlo, thi = text["addr"], text["addr"] + text["size"]
    got2libc, slot2raw = {}, {}
    for relname in (".rela.dyn", ".rela.plt"):
        r = byname.get(relname)
        if not r:
            continue
        for i in range(r["size"] // 24):
            o = r["off"] + i * 24
            off, info, add = struct.unpack_from("<QQq", data, o)
            typ, symi = info & 0xFFFFFFFF, info >> 32
            add &= 0xFFFFFFFFFFFFFFFF
            slot2raw.setdefault(off, []).append((typ, symi, add))
            if typ in (6, 7) and symi < len(syms):
                got2libc[off] = syms[symi]
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    plt2name = {}
    plt = byname.get(".plt")
    if plt and plt["size"]:
        blob = data[plt["off"]:plt["off"] + plt["size"]]
        for ins in md.disasm(blob, plt["addr"]):
            if ins.mnemonic != "jmp":
                continue
            for op in ins.operands:
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    slot = (ins.address + ins.size + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
                    if slot in got2libc:
                        plt2name[ins.address] = got2libc[slot]

    def slot_kind(slot):
        if slot is None:
            return ("unknown", None)
        if slot in got2libc:
            return ("libc", got2libc[slot])
        for typ, symi, add in slot2raw.get(slot, ()):
            if typ == 8 and tlo <= add < thi:
                return ("text", add)
        if slot in slot2raw:
            return ("reloc_other", slot2raw[slot][0][0])
        return ("runtime", None)
    return got2libc, plt2name, slot_kind, slot2raw, md


def data_ranges(byname):
    out = []
    for n in DATA_SECS:
        s = byname.get(n)
        if s and s["size"]:
            out.append((s["addr"], s["addr"] + s["size"], n))
    return out


def in_ranges(ranges, addr):
    for lo, hi, n in ranges:
        if lo <= addr < hi:
            return n
    return None


# ---------------------------------------------------------------- 扫描
def scan(data, byname, funcstarts):
    """逐函数特征 + 静态调用边。返回 (per_func_feat, edges)。"""
    text = byname[".text"]
    tlo, thi = text["addr"], text["addr"] + text["size"]
    got2libc, plt2name, slot_kind, slot2raw, md = build_maps(data, byname)
    dr = data_ranges(byname)
    blob = data[text["off"]:text["off"] + text["size"]]
    ins = list(md.disasm(blob, tlo))
    A = [i.address for i in ins]
    fset = set(funcstarts)

    def fn_of(addr):
        i = bisect.bisect_right(funcstarts, addr) - 1
        return funcstarts[i] if i >= 0 else None

    feats = {}
    edges = {}
    cur = None
    prov = {}
    for k, insn in enumerate(ins):
        f = fn_of(insn.address)
        if f != cur:
            cur, prov = f, {}
            feats.setdefault(f, dict(
                n_insn=0, calls=[], tls=[], sysio=[], threads=[], unwind=[],
                global_writes=[], arg_ptr_read={}, arg_ptr_write={},
                touches_xmm0=False, indirect_unknown=[], has_ret=False,
            ))
        ft = feats[cur]
        ft["n_insn"] += 1
        if insn.mnemonic in ("ret", "retq"):
            ft["has_ret"] = True
        # --- 寄存器来源跟踪（只跟踪 64 位通用寄存器）---
        if insn.mnemonic in ("call", "jmp") and insn.operands:
            op = insn.operands[0]
            cold = k + 1 < len(ins) and ins[k + 1].mnemonic in ("int3", "ud2")
            if op.type == X86_OP_IMM:
                t = op.imm & 0xFFFFFFFFFFFFFFFF
                if t in plt2name:
                    nm = plt2name[t]
                    ft["calls"].append((insn.address, ("import", nm), cold))
                    if nm in SYS_IO:
                        ft["sysio"].append((insn.address, nm))
                    elif nm in THREAD_IMP:
                        ft["threads"].append((insn.address, nm))
                    elif nm in UNWIND_IMP:
                        ft["unwind"].append((insn.address, nm))
                elif tlo <= t < thi:
                    ft["calls"].append((insn.address, ("text", t), cold))
            elif op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                slot = (insn.address + insn.size + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
                ft["calls"].append((insn.address, slot_kind(slot), cold))
            elif op.type == X86_OP_REG:
                p = prov.get(op.reg)
                if p and p[0] == "global":
                    ft["calls"].append((insn.address, ("engine_dispatch", p[1]), cold))
                else:
                    ft["calls"].append((insn.address, ("indirect_unknown", p), cold))
                    ft["indirect_unknown"].append((insn.address, insn.mnemonic, insn.op_str))
            # 调用只破坏 **caller-saved** 寄存器；rbx/rbp/r12-r15 是 callee-saved，
            # 调用之后它们仍指着我方参数派生的地址 —— 早先在这里 `prov.clear()` 把这条
            # 线索一起抹了，导致「调用后写回 self」的函数被误判成 executable（实测 0x4be3e0：
            # `mov %rdi,%rbx` → call → `mov %r14,0x60(%rbx)`，本应 inout_self）。
            for r in tuple(prov):
                # 只清 caller-saved：rbx/rbp/r12-r15 跨调用仍有效
                if r not in (X86_REG_RBX, X86_REG_RBP, X86_REG_R12, X86_REG_R13,
                             X86_REG_R14, X86_REG_R15, X86_REG_RSP):
                    prov.pop(r, None)
            continue
        # 内存操作数特征
        for oi, op in enumerate(insn.operands):
            if op.type != X86_OP_MEM:
                continue
            if op.mem.segment in (X86_REG_FS, X86_REG_GS):
                ft["tls"].append((insn.address, "fs:%s" % hex(op.mem.disp)))
            base = op.mem.base
            if base == X86_REG_RIP:
                tgt = (insn.address + insn.size + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
                seg = in_ranges(dr, tgt)
                if seg and oi == 0:
                    ft["global_writes"].append((insn.address, "%s@%#x" % (seg, tgt)))
            rname = None
            for cand, creg in (("rdi", X86_REG_RDI), ("rsi", X86_REG_RSI),
                               ("rdx", X86_REG_RDX), ("rcx", X86_REG_RCX)):
                if base == creg:
                    rname = cand
            if rname is None and base:
                pv = prov.get(base)
                if pv and pv[0] == "argp":
                    rname = pv[1]
            if rname:
                d = ft["arg_ptr_write"] if oi == 0 else ft["arg_ptr_read"]
                d[rname] = d.get(rname, 0) + 1
        # 寄存器写：更新来源
        if insn.operands and insn.operands[0].type == X86_OP_REG:
            dst = insn.operands[0].reg
            if dst == X86_REG_XMM0:
                ft["touches_xmm0"] = True
            src = insn.operands[1] if len(insn.operands) > 1 else None
            p = None
            if insn.mnemonic in ("mov", "lea", "add", "sub") and src is not None and len(insn.operands) >= 2:
                b = src
                if src.type == X86_OP_MEM and src.mem.base and not (
                        src.mem.base == X86_REG_RIP):
                    bb = src.mem.base
                    pv = prov.get(bb)
                    nm = {X86_REG_RDI: "rdi", X86_REG_RSI: "rsi",
                          X86_REG_RDX: "rdx", X86_REG_RCX: "rcx"}.get(bb)
                    if nm:
                        p = ("argp", nm)
                    elif pv and pv[0] == "argp":
                        p = pv
                    else:
                        p = None
                elif src.type == X86_OP_REG and insn.mnemonic == "mov":
                    nm = {X86_REG_RDI: "rdi", X86_REG_RSI: "rsi",
                          X86_REG_RDX: "rdx", X86_REG_RCX: "rcx"}.get(src.reg)
                    pv = prov.get(src.reg)
                    if nm:
                        p = ("argp", nm)
                    elif pv and pv[0] == "argp":
                        p = pv
                    else:
                        p = None
                if p is not None:
                    prov[dst] = p
                    src = None
            if src is not None and insn.mnemonic == "mov":
                if src.type == X86_OP_MEM and src.mem.base == X86_REG_RIP:
                    slot = (insn.address + insn.size + src.mem.disp) & 0xFFFFFFFFFFFFFFFF
                    p = ("global", slot)
                elif src.type == X86_OP_MEM:
                    p = ("mem", src.mem.base)
                elif src.type == X86_OP_IMM:
                    p = ("imm", src.imm & 0xFFFFFFFFFFFFFFFF)
            elif src is not None and insn.mnemonic == "lea" and src.type == X86_OP_MEM \
                    and src.mem.base == X86_REG_RIP:
                slot = (insn.address + insn.size + src.mem.disp) & 0xFFFFFFFFFFFFFFFF
                p = ("addr", slot)
            if p:
                prov[dst] = p
            else:
                prov.pop(dst, None)
    return feats, edges


# ---------------------------------------------------------------- 分类
FATAL_KIND = {"engine_dispatch", "runtime", "reloc_other", "unknown_import"}


def self_like(ft):
    """参数指针里**既被读又被写**的那些 = `&mut self` 一类的既有状态（必须自己造）。"""
    return sorted(set(ft["arg_ptr_read"]) & set(ft["arg_ptr_write"]))


def classify(feats):
    """三档分类。致命项按调用图向上传播（迭代不动点，不用递归——13k 个函数递归会炸栈）。"""
    fatal = {}
    callers = {}
    for f, ft in feats.items():
        why = []
        if ft["tls"]:
            why.append("tls")
        if ft["sysio"]:
            why.append("sys_io")
        if ft["threads"]:
            why.append("thread")
        if ft["unwind"]:
            why.append("unwind")
        for a, kind, cold in ft["calls"]:
            if cold:
                continue
            if kind[0] in ("engine_dispatch", "runtime", "reloc_other"):
                why.append(kind[0])
            elif kind[0] == "text":
                callers.setdefault(kind[1], []).append(f)
        if why:
            fatal[f] = sorted(set(why))[0]

    # 「非返回错误路径」不传播：panic / alloc-error / print_error 这类函数体里**没有 ret**
    # （实测 0x94b340 = panic 入口、0x925660 = __rust_alloc_error_handler，都以 int3/ud2 收尾）。
    # 正常输入走不到它们；把它们当致命会把**整个二进制**连坐 —— 439 个函数里 264 个是这么被连坐的。
    nore = {f for f in fatal if not feats[f]["has_ret"]}

    reason = dict(fatal)
    work = [f for f in fatal if f not in nore]
    while work:
        f = work.pop()
        for c in callers.get(f, ()):
            if c not in fatal and c not in nore:
                fatal[c] = reason[c] = "via:" + hex(f)
                work.append(c)

    tier = {}
    for f, ft in feats.items():
        if f in fatal:
            tier[f] = "isolated_impossible"
        elif self_like(ft) or ft["global_writes"] or ft["indirect_unknown"]:
            tier[f] = "needs_state"
            bits = []
            sl = self_like(ft)
            if sl:
                bits.append("inout_self:" + ",".join(sl))
            if ft["global_writes"]:
                bits.append("global_wr:%d" % len(ft["global_writes"]))
            if ft["indirect_unknown"]:
                bits.append("indirect_unknown:%d" % len(ft["indirect_unknown"]))
            reason[f] = ";".join(bits)
        else:
            tier[f] = "executable"
            bits = []
            if ft["arg_ptr_read"]:
                bits.append("in:" + ",".join(sorted(ft["arg_ptr_read"])))
            if ft["arg_ptr_write"]:
                bits.append("out:" + ",".join(sorted(ft["arg_ptr_write"])))
            reason[f] = ";".join(bits) or "scalar_io"
    return tier, reason


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", default="")
    ap.add_argument("--out", default="")
    ap.add_argument("--show", default="")
    a = ap.parse_args()
    from collections import Counter

    E = CM.Elf(CM.SO)
    data = E.data
    byname = elf_sections(data)
    text = byname[".text"]
    tlo, thi = text["addr"], text["addr"] + text["size"]
    funcstarts = CM.load_funcstarts(CM.FUNCSTARTS)
    feats, _ = scan(data, byname, funcstarts)
    tier, reason = classify(feats)

    lits = CM.find_literals(E)
    locs = CM.resolve_locations(E, lits)
    refs = CM.sweep_references(E, locs)
    per_func, contested = CM.attribute(funcstarts, locs, refs)
    status = {}
    with open(os.path.join(ROOT, "docs", "coverage-matrix.tsv")) as f:
        for line in f:
            if line.startswith("#") or line.startswith("module\t"):
                continue
            c = line.rstrip("\n").split("\t")
            if len(c) >= 9:
                status[c[0]] = c[7]
    obs = sorted((fn, m) for fn, (m, _n) in per_func.items() if status.get(m) == "观测")
    assert len(obs) == 439, "预注册前提被证伪：观测级函数 %d != 439" % len(obs)

    rows = [(fn, mod, tier.get(fn, "?"), reason.get(fn, "?"), feats[fn]["n_insn"])
            for fn, mod in obs]
    print("439 观测级函数静态分类：", dict(Counter(r[2] for r in rows)), file=sys.stderr)
    print("全 .text 被判为 isolated_impossible 的函数总数：",
          sum(1 for v in tier.values() if v == "isolated_impossible"), file=sys.stderr)
    if a.json:
        json.dump([dict(addr=fn, module=mod, tier=t, reason=r, n_insn=n)
                   for fn, mod, t, r, n in rows], open(a.json, "w"), indent=0)
    if a.show:
        for fn, mod, t, r, n in rows:
            if t == a.show:
                print("%#x\t%s\t%s\t%d" % (fn, mod, r, n))
    return 0


if __name__ == "__main__":
    sys.exit(main())
