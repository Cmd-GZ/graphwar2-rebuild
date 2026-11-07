#!/usr/bin/env python3
"""点数组（`Vec<(f64,f64)>`）**填充链**的可复跑取证器（`dispatch/20-point-array-filler`）。

## 它回答什么

`docs/06-verification.md` §32.5 留了一个空白：「`[x19+0x30..0x40]` 那个点数组的**填充者**是谁」。
这个脚本把从「采样器算出一个点」到「`execute_results` 把 `points[len-1]` 当爆炸点」的
**每一跳**打出来，并逐跳判定这一跳**有没有做浮点运算**（也就是：写进数组的 `y` 是不是
逐位等于采样器的返回值）。

```bash
# 1) 名字 + 地址（arm64 2.0.60 未 strip：名字是**符号表原文**，不是猜的）
../.venv/bin/python tools/point_array_chain.py sym

# 2) 逐跳指令（arm64 与 x86-64 各一遍）
../.venv/bin/python tools/point_array_chain.py hop --arch arm
../.venv/bin/python tools/point_array_chain.py hop --arch x86

# 3) 自检（**正对照**）：所有锚点必须命中，否则本工具不可信
../.venv/bin/python tools/point_array_chain.py check
```

## 口径

* **arm64** 地址 = `.symtab` 的 `st_value`（`android-2.0.60/libgraphwarrust.so`，未 strip）。
  名字全部来自符号表，不做任何跨架构推断。
* **x86-64** 地址 = `2026-08-28/libgraphwarrust.so` 的 ELF 虚地址（Ghidra = +0x100000）。
  这份被 strip，落地靠 `fixtures/name_to_addr.tsv` 的 `panic::Location` 唯一锚 +
  **指令形态**双向核对（本脚本就是那份核对）。
* `hop` 只打印**判定要用的那几条**指令，不是整个函数（整函数请用 `objdump -d`）。

## 正对照（为什么相信"没找到算术"是真的）

`check` 子命令要求下面每一条都成立，任一条不成立就退出码 1：

1. arm64 `0x3b5374` 的符号名必须以 `…18calculate_function…` 结尾（符号表读法有效）；
2. arm64 `0x4a8558` = `…15execute_results…`、`0x4a7160` = `…14fire_functions…`
   （`docs/06` §25.4 钉过的两个地址，能对上才说明同一份二进制）；
3. x86 `0x3c2530` 内必须出现 `4eb9d0`（**求值器**，§32 已逐位对拍过的那个），
   且出现 `movsd %xmm0,(%rax,%rcx,1)` + `movsd %xmm0,0x8(%rax,%rcx,1)` 这一对
   「写 x、写 y」；
4. x86 `0x470eb0` 内必须出现 `3f847ae147ae147b`（= `0.01`，与 arm64 `movz/movk` 造出来的
   同一个字面量）以及 `call 456810`（`set_function_points`）与 `call 472470`
   （`execute_results`）；
5. x86 `0x456810` 内必须出现 `movups (%rbx),%xmm0` + `movups %xmm0,(%r15)`
   （Vec 三段头整体搬进 `HashMap` 桶）——这就是「填充」的那一刻。

第 3/4/5 条互为交叉：**同一条链在两份不同架构的二进制上都成立**，才敢说"这一跳没有算术"。
"""

import argparse
import os
import re
import bisect
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def _find_ref():
    """`参考二进制` 在 `<ws>/.reference/steam/<tag>/`；隔离 worktree 比 `<ws>` 深几层时逐级向上找。"""
    d = HERE
    for _ in range(8):
        cand = os.path.join(d, ".reference/steam")
        if os.path.isdir(cand):
            return os.path.join(d, ".reference")
        nd = os.path.dirname(d)
        if nd == d:
            break
        d = nd
    return os.path.join(os.path.dirname(HERE), ".reference")


REF = _find_ref()
ARM = os.environ.get(
    "GW2_ARM_SO",
    os.path.join(REF, "steam/android-2.0.60/libgraphwarrust.so"))
X86 = os.environ.get(
    "GW2_SO", os.path.join(REF, "steam/2026-08-28/libgraphwarrust.so"))

# ---------------------------------------------------------------------------
# 链（每一跳都要能被 objdump 复现；`arith` = 这一跳有没有做浮点运算）
# ---------------------------------------------------------------------------
CHAIN = [
    dict(n=1, what="采样器算点并 push 进 Vec", arith="是（这一步就是采样器本身）",
         arm=("GraphwarShotCalculator::calculate_function", 0x3B5374),
         x86=("GraphwarShotCalculator::calculate_function", 0x3C2530)),
    dict(n=2, what="结果被打包成 GraphwarShotResult{points,id,flag}", arith="否（只写指针/长度）",
         arm=("GraphwarShotCalculator::calculate_shot_results", 0x3B5770),
         x86=("GraphwarShotCalculator::calculate_shot_results", 0x3C2BC0)),
    dict(n=3, what="整条 Vec 头搬进 self.results（+0x30..+0x40）", arith="否（搬 24 字节头）",
         arm=("GraphwarGameStateManager::fire_functions", 0x4A7160),
         x86=("GraphwarGameStateManager::fire_functions", 0x470EB0)),
    dict(n=4, what="逐元素深拷贝点表（16 字节块）→ set_function_points", arith="否（16/32 字节块拷贝）",
         arm=("GraphwarGameStateManager::fire_functions", 0x4A7160),
         x86=("GraphwarGameStateManager::fire_functions", 0x470EB0)),
    dict(n=5, what="存进 HashMap<u32, Vec<(f64,f64)>> 桶", arith="否（搬 24 字节头）",
         arm=("GraphwarFunctionManager::set_function_points", 0x489278),
         x86=("GraphwarFunctionManager::set_function_points", 0x456810)),
    dict(n=6, what="execute_results 取 points[len-1] 当爆炸点 pos", arith="否（只 ldp 两个 f64）",
         arm=("GraphwarGameStateManager::execute_results", 0x4A8558),
         x86=("GraphwarGameStateManager::execute_results", 0x472470)),
]

# 每条断言：(二进制, 地址, 必须在反汇编里出现的串, 说明)
ASSERTIONS = [
    ("arm", 0x3B5374, "18calculate_function", "arm64 符号表读法有效（名字来自 .symtab）"),
    ("arm", 0x4A8558, "15execute_results", "与 docs/06 §25.4 钉的地址一致"),
    ("arm", 0x4A7160, "14fire_functions", "与 name_to_addr 的 x86 唯一锚互证"),
    ("arm", 0x489278, "19set_function_points", "填充者的名字"),
    ("arm", 0x3B5770, "22calculate_shot_results", "结果打包者"),
    ("x86", 0x3C2530, "4eb9d0", "★ 采样器里调用的是**求值器**（§32 已逐位排除的那个）"),
    ("x86", 0x3C2530, "movsd  %xmm0,(%rax,%rcx,1)", "★ 写 point.x"),
    ("x86", 0x3C2530, "movsd  %xmm0,0x8(%rax,%rcx,1)", "★ 写 point.y"),
    ("x86", 0x3C2530, "9baca0", "push 走 GOT 槽（所以「零 call rel32」会漏掉它）"),
    ("x86", 0x470EB0, "3f847ae147ae147b", "0.01 = 采样弧长步长（与 arm64 同一字面量）"),
    ("x86", 0x470EB0, "call   456810", "→ set_function_points"),
    ("x86", 0x470EB0, "call   472470", "→ execute_results"),
    ("x86", 0x456810, "movups (%rbx),%xmm0", "★ Vec 头（cap,ptr）整体搬走"),
    ("x86", 0x456810, "movups %xmm0,(%r15)", "★ Vec 头整体写进 HashMap 桶"),
    ("x86", 0x472470, "0x40(%rbp)", "★ self.results.len"),
    ("x86", 0x472470, "0x38(%rbp)", "★ self.results.ptr"),
    ("x86", 0x3C2BC0, "0x18(%r14,%r13,1)", "★ 队列条目取 GraphwarShotResult.id"),
]

# hop 模式只打印这些串所在的行（判据行）
HOP_KEYS = {
    0x3B5374: ["bl\t0x4452d4", "stp\td9, d12", "stp\td8, d13", "mov\tx8, #0x4069",
               "fdiv\td1, d12, d1", "fadd\td13, d9, d0", "tbz\tw22, #0x0"],
    0x3B5770: ["bl\t0x3b5374", "stp\tx27, x10", "str\tw28", "strb\tw10"],
    0x4A7160: ["fmov\td0, x8", "bl\t0x3b5770", "stur\tq0, [x22, #0x30]",
               "stur\tx8, [x22, #0x40]", "ldp\tq0, q1", "stp\tq0, q1", "bl\t0x489278"],
    0x489278: ["str\tq0, [x22]", "str\tx8, [x22, #0x10]", "bl\t0x3d7a40"],
    0x4A8558: ["ldr\tx8, [x19, #0x40]", "ldr\tx10, [x19, #0x38]",
               "ldp\td9, d8, [x8, #-0x10]"],
    0x3C2530: ["4eb9d0", "9baca0", "movsd  %xmm0,(%rax,%rcx,1)",
               "movsd  %xmm0,0x8(%rax,%rcx,1)", "19fc78"],
    0x3C2BC0: ["0x18(%r14,%r13,1)", "movdqu %xmm0,0x8(%rax,%rcx,1)"],
    0x470EB0: ["3f847ae147ae147b", "call   456810", "call   472470",
               "movups (%rcx),%xmm0", "movups %xmm0,(%rsi)"],
    0x456810: ["movups (%rbx),%xmm0", "movups %xmm0,(%r15)"],
    0x472470: ["0x40(%rbp)", "0x38(%rbp)", "shl    $0x5"],
    0x3C37A0: ["movb   $0x1,(%rax,%r15,1)", "movupd (%rdx,%r12,1)",
               "cmpb   $0xff,(%rax,%rdi,1)"],
}


def disasm(arch, start, size):
    """返回 (行列表, 函数体文本)。arm64 走 llvm-objdump，x86 走 objdump。"""
    if arch == "arm":
        so, tool = ARM, ["llvm-objdump", "-d", "--no-show-raw-insn",
                         "--start-address=%#x" % start,
                         "--stop-address=%#x" % (start + size), ARM]
    else:
        so, tool = X86, ["objdump", "-d", "--no-show-raw-insn",
                         "--start-address=%#x" % start,
                         "--stop-address=%#x" % (start + size), X86]
    if not os.path.exists(so):
        sys.exit("找不到目标二进制：%s（用 GW2_ARM_SO / GW2_SO 指定）" % so)
    out = subprocess.run(tool, capture_output=True, text=True).stdout
    lines = [l.rstrip() for l in out.splitlines() if "\t" in l]
    return lines


def arm_symtab():
    """arm64 .symtab：地址 -> 名字（demangle 用 nm -C 的等价物：直接读 nm 原文）。"""
    out = subprocess.run(["nm", "-S", "--defined-only", ARM],
                         capture_output=True, text=True).stdout
    tab = []
    for line in out.splitlines():
        m = re.match(r"([0-9a-f]{16})\s+([0-9a-f]{16})\s+\S\s+(\S+)$", line)
        if m:
            tab.append((int(m.group(1), 16), int(m.group(2), 16), m.group(3)))
    tab.sort()
    return tab


def demangle(name):
    out = subprocess.run(["c++filt", name], capture_output=True, text=True).stdout.strip()
    return out or name


def cmd_sym(_a):
    tab = arm_symtab()
    print("arm64 %s：%d 个带尺寸的符号（未 strip）" % (os.path.basename(ARM), len(tab)))
    by_addr = {a: (s, n) for a, s, n in tab}
    print("\n链路里每个函数在 arm64 上的**符号表原文**：")
    for h in CHAIN:
        a = h["arm"][1]
        size, name = by_addr.get(a, (0, "<该地址不是符号起点>"))
        print("  0x%08x  size=0x%-6x  %s" % (a, size, demangle(name)))
    print("\n名字都带 `17h…E` 后缀（Rust legacy mangling 的哈希）——这是符号表原文，不是猜的。")
    print("注：`…E.llvm.NNN` 是 LLVM 把函数摘出去（outline）后的克隆，与本体同体不同名。")


def cmd_hop(a):
    arch = a.arch
    print("== %s：点数组填充链，逐跳判据指令 ==" % ("arm64 2.0.60" if arch == "arm" else "x86-64 2026-08-28"))
    for h in CHAIN:
        name, addr = h["arm"] if arch == "arm" else h["x86"]
        print("\n[%d] %s" % (h["n"], h["what"]))
        print("    %s @ %#x   这一跳做了浮点运算吗？→ %s" % (name, addr, h["arith"]))
        keys = HOP_KEYS.get(addr)
        if not keys:
            print("    （没有登记判据行）")
            continue
        lines = disasm(arch, addr, 0x400 if addr not in (0x470EB0, 0x3B5770, 0x4A7160,
                                                        0x4A8558, 0x456810, 0x472470,
                                                        0x3C2BC0, 0x3C2530) else 0x1400)
        hit = [l for l in lines if any(k in l for k in keys)]
        for l in hit[:12]:
            print("      " + l.strip())
        if not hit:
            print("      ?? 一条判据行都没命中——本跳的结论作废，先查地址")


def cmd_check(_a):
    bad = 0
    print("== 正对照：%d 条断言，全部必须成立 ==" % len(ASSERTIONS))
    cache = {}
    for arch, addr, needle, why in ASSERTIONS:
        if addr not in cache:
            cache[addr] = disasm(arch, addr, 0x1400)
        ok = any(needle in l for l in cache[addr])
        print("  [%s] %-6s %#-9x %-32s %s" % ("OK" if ok else "FAIL", arch, addr, needle, why))
        bad += 0 if ok else 1
    print("\n%d 条失败" % bad)
    if bad:
        print("⇒ **扫描法无效**，上面所有「没找到算术」的结论都不许写进文档。")
        return 1
    print("⇒ 扫描法有效：同一跳在 arm64 与 x86-64 上都指到同一条指令形态。")
    return 0


def cmd_fields(_a):
    """`GraphwarShotResult` 与队列条目的字段偏移（从写指令反推，不猜类型）。"""
    print("GraphwarShotResult（32 B）：")
    for off, what, ev in [
        ("+0x00", "points.cap", "arm 0x3b5870 `stp x27, x10, [x8]`"),
        ("+0x08", "points.ptr", "同上（第二个寄存器）"),
        ("+0x10", "points.len", "arm 0x3b5884 `str x9, [x8, #0x10]`"),
        ("+0x18", "id: u32", "arm 0x3b5890 `str w28, [x8, #0x18]`；x86 0x3c38a3 读它"),
        ("+0x1c", "flag: u8", "arm 0x3b5894 `strb w10, [x8, #0x1c]`"),
    ]:
        print("  %s %-12s %s" % (off, what, ev))
    print("\n挖洞队列条目（32 B，x86 calculate_shot_results 0x3c38e4..0x3c38fe）：")
    for off, what, ev in [
        ("+0x00", "tag 0x8000000000000000", "`movabs $0x7fff…; inc %rdx; mov %rdx,(%rax,%rcx,1)`"),
        ("+0x08", "(x, y) 16 B", "`movdqu %xmm0,0x8(%rax,%rcx,1)` ← 直接来自 points[X]"),
        ("+0x18", "id: u32", "`mov 0x18(%r14,%r13,1),%r14d` = GraphwarShotResult.id"),
        ("+0x1c", "X: u32（采样序号）", "`mov 0x38(%rsp),%edx` = 外层循环变量"),
    ]:
        print("  %s %-22s %s" % (off, what, ev))
    print("\n循环嵌套（x86 0x3c3755..0x3c37a0）：外层 over X（0x3c3770 `inc`、0x3c378d 存 [rsp+0x38]），")
    print("内层 over 结果记录（0x3c37aa `add $0x20,%r13` / `inc %r15`），`flags[r15]` 按**结果**索引。")
    print("⇒ 每个结果记录最多排一次队；X 在内层是**共享**的。")


def _arm_text():
    out = subprocess.run(["readelf", "-S", "-W", ARM], capture_output=True, text=True).stdout
    for line in out.splitlines():
        m = re.match(r"\s*\[\s*\d+\]\s+(\.text)\s+\S+\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)", line)
        if m:
            return int(m.group(2), 16), int(m.group(3), 16), int(m.group(4), 16)
    sys.exit("读不出 .text 段表")


def arm_callers(target):
    """arm64 全 `.text` 扫 `bl` 立即数，返回调用点地址（`bl` 的 imm26<<2 相对 PC）。"""
    data = open(ARM, "rb").read()
    addr, off, size = _arm_text()
    code = data[off:off + size]
    tab = arm_symtab()
    starts = [a for a, _s, _n in tab]
    owners = {a: n for a, _s, n in tab}

    def owner(pc):
        i = bisect.bisect_right(starts, pc) - 1
        if i >= 0 and pc - starts[i] < 0x2000:
            return owners[starts[i]]
        return None

    hits = []
    for cur in range(addr, addr + size, 4):
        w = struct.unpack_from("<I", code, cur - addr)[0]
        if (w & 0xFC000000) != 0x94000000:
            continue
        imm = w & 0x3FFFFFF
        if imm & 0x2000000:
            imm -= 0x4000000
        if cur + imm * 4 == target:
            hits.append(cur)
    return [(h, owner(h)) for h in hits]


def cmd_pos(_a):
    """`PlanePosManager` 的读写两侧（§33.4 的正面结论靠这条）。"""
    want = [("get_position", 0x3B7F58), ("set_position", 0x3B8160)]
    bad = 0
    for nm, addr in want:
        hits = arm_callers(addr)
        print("== arm64 %#x `GraphwarPlanePosManager::%s`：%d 个 `bl` 调用点" % (addr, nm, len(hits)))
        for pc, own in hits:
            print("   %#010x  in %s" % (pc, demangle(own) if own else "<未归属>"))
        if not hits:
            bad += 1
    # 正对照：get_position 的两个已知调用点在 fire_functions / execute_results 里
    got = [pc for pc, _ in arm_callers(0x3B7F58)]
    ctrl = [pc for pc in got if pc in (0x4A72DC, 0x4A8B40)]
    print("\n正对照：`get_position` 必须命中 0x4a72dc（fire_functions）与 0x4a8b40（execute_results）"
          " → %s" % ("OK" if len(ctrl) == 2 else "FAIL（扫法无效）"))
    if len(ctrl) != 2:
        bad += 1
    print("⇒ `set_position` 的调用点若**都不在**弹道计算里，则那张位置表不是本机采样的产物。")
    return 1 if bad else 0


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("sym", help="arm64 符号表里的链路地址/尺寸")
    p = sub.add_parser("hop", help="逐跳判据指令")
    p.add_argument("--arch", choices=("arm", "x86"), default="arm")
    sub.add_parser("check", help="正对照（全部断言必须成立）")
    sub.add_parser("fields", help="字段偏移（从写指令反推）")
    sub.add_parser("pos", help="PlanePosManager 读写两侧（arm64 bl 扫描）")
    a = ap.parse_args(argv)
    return {"sym": cmd_sym, "hop": cmd_hop, "check": cmd_check,
            "fields": cmd_fields, "pos": cmd_pos}[a.cmd](a) or 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
