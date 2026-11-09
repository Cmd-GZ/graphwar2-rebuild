#!/usr/bin/env python3
"""`y0` 的来路：`GraphwarPlanePosManager` 那张表 → 采样器 —— **逐条指令断言**。

## 为什么需要它

`docs/06-verification.md` §47 的结论是"**采样器吃的 `x0/y0` 在 `0x3c2bc0` 里不是算出来的，
是从一张表里抄的**；那张表只有两个写入者，两个都是逐字节拷贝"。这条结论如果只写在文档里，
下一个人只能重新反汇编一遍才知道对不对。本工具把**这条链上的每一条指令**变成断言：

```text
计算器入口            0x3c2bc0  只读 [arg2+0x10] 这一张表
表项布局              stride 0x20，key=u32@0, x@+8, y@+0x10, z@+0x18
采样器调用点          0x3c30c1（全函数唯一），y0 = xmm1 ← [rsp+0x108] ← 表项 +0x10
表的 get              0x3c62b0  Option<(f64,f64,f64)>，命中时 +8/+0x10/+0x18 三拷贝
表的 insert           0x3c64c0  只有 2 个调用者：0x465f85（关卡生成器）/ 0x3e3347（事件处理器）
事件处理器写什么       0x3e2830 把 FunctionPoints 事件 +8/+0x10/+0x18 原样抄进去，key=u32@+0x20
calculate_shot_results 的调用者   槽 0x9bacb0，只有 2 处：0x471825（fire_functions）/ 0x51a9a1（AI 评分）
```

任一条断言失败 ⇒ 那条文档结论**已经过期**，不许再引用。所以本工具只在"`.so` 没变"时才是绿的：
目标 `.so` 的 sha256 写在 `EXPECT_SHA` 里，不匹配就**先报错退出**（地址会整体漂移）。

## 用法

```bash
export PY=<WS>/.reference/venv/bin/python
$PY tools/plane_pos_chain.py            # 断言 + 打印这条链
$PY tools/plane_pos_chain.py --verbose  # 每条断言附上实际指令
```

依赖：`capstone`（`.reference/venv` 里有）。`.so` 默认取 `.reference/steam/2026-08-28/`，
用 `GW2_SO=` 覆盖。地址一律 **ELF 虚地址**（Ghidra 地址 = 本工具地址 + 0x100000）。
"""

import os
import sys
import hashlib

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)

EXPECT_SHA = "9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187"


def find_so():
    env = os.environ.get("GW2_SO")
    if env:
        return env
    for depth in range(4):
        base = os.path.abspath(os.path.join(HERE, *[".."] * (depth + 1)))
        for tag in ("2026-08-28", "android-2.0.60"):
            p = os.path.join(base, ".reference", "steam", tag, "libgraphwarrust.so")
            if os.path.exists(p):
                return p
    sys.exit("找不到 libgraphwarrust.so；用 GW2_SO= 指定")


# ---------------------------------------------------------------------------
# 这里开始是"链"。每条 = (说明, 地址, 断言列表)
# 断言写法：(capstone 助记符, 操作数子串) —— 操作数按 capstone 的 Intel 语法渲染。
# ---------------------------------------------------------------------------

# 1) 计算器入口只从 [arg2+0x10] 那张表取表指针
CHAIN_TABLE_ARG = [
    ("计算器取了 [arg2+0x10] 当「位置表」指针", 0x3C2CEE, ("mov", "rbx, qword ptr [rax + 0x10]")),
    ("同一句还把 rsi 存起来（arg2 就是 rsi）", 0x3C2BF4, ("mov", "qword ptr [rsp + 0x150], rsi")),
]

# 2) 采样器调用点：全 0x3c2bc0 唯一；x0/y0 来自栈槽
CHAIN_SAMPLER = [
    ("采样器 sret = rsp+0xb0", 0x3C3099, ("lea", "rdi, [rsp + 0xb0]")),
    ("x0 ← [rsp+0x90]", 0x3C30A1, ("movq", "xmm0, qword ptr [rsp + 0x90]")),
    ("y0 ← [rsp+0x108]", 0x3C30AA, ("movq", "xmm1, qword ptr [rsp + 0x108]")),
    ("tokens ← rsp+0x10", 0x3C30B3, ("lea", "rsi, [rsp + 0x10]")),
    ("xmm2 ← [rsp+0x110]（= arg2+0 的 step）", 0x3C30B8, ("movq", "xmm2, qword ptr [rsp + 0x110]")),
    ("call *0x9bac98（calculate_function）", 0x3C30C1, ("call", "qword ptr [rip + 0x5f7bd1]")),
]

# 3) 两个栈槽是"从表项抄过来的"，抄的位置 = 表项 +0x18 / +0x10
CHAIN_COPY = [
    ("x0 ← 表项[r8-0x18]", 0x3C2F20, ("movsd", "xmm0, qword ptr [r8 - 0x18]")),
    ("→ [rsp+0x90]", 0x3C2F26, ("movsd", "qword ptr [rsp + 0x90], xmm0")),
    ("y0 ← 表项[r8-0x10]", 0x3C2F2F, ("movq", "xmm0, qword ptr [r8 - 0x10]")),
    ("→ [rsp+0x108]", 0x3C2F35, ("movq", "qword ptr [rsp + 0x108], xmm0")),
]

# 4) 表的 get：0x3c62b0 —— stride 0x20、key u32、值 24 字节三拷贝
CHAIN_GET = [
    ("stride = idx<<5（0x20）", 0x3C6377, ("shl", "r10, 5")),
    ("key 在表项 -0x20，和 u32 id 比", 0x3C6381, ("cmp", "ebp, dword ptr [r9 - 0x20]")),
    ("值的第三个 f64 ← 表项 -0x8", 0x3C63BB, ("mov", "rax, qword ptr [r9 - 8]")),
    ("→ out+0x18", 0x3C63BF, ("mov", "qword ptr [rbx + 0x18], rax")),
    ("值的前 16 字节 ← 表项 -0x18", 0x3C63C3, ("movdqu", "xmm0, xmmword ptr [r9 - 0x18]")),
    ("→ out+0x8（x）+0x10（y）", 0x3C63C9, ("movdqu", "xmmword ptr [rbx + 8], xmm0")),
    ("命中 ⇒ Option = 1", 0x3C63CE, ("mov", "ecx, 1")),
]

# 5) 表的 insert：0x3c64c0 —— 值就是 (f64,f64,f64)
CHAIN_INSERT = [
    ("insert 取出表指针 [rdi+0x10]", 0x3C64E8, ("mov", "r13, qword ptr [rdi + 0x10]")),
    ("值 .0 ← [rdx]", 0x3C64D6, ("movups", "xmm0, xmmword ptr [rdx]")),
    ("值 .2 ← [rdx+0x10]", 0x3C64DE, ("movsd", "xmm0, qword ptr [rdx + 0x10]")),
    ("u32 id 从 esi 进 ebx", 0x3C64D1, ("mov", "ebx, esi")),
]

# 6) 事件处理器 0x3e2830：把 FunctionPoints 事件原样抄进这张表
CHAIN_EVENT_WRITER = [
    ("抄事件 +8（x,y 16 字节）", 0x3E32D4, ("movups", "xmm0, xmmword ptr [rbx + 8]")),
    ("抄事件 +0x18（end_x）", 0x3E32E0, ("movq", "xmm0, qword ptr [rbx + 0x18]")),
    ("key = u32 @事件 +0x20（owner_id 低 32 位）", 0x3E32EE, ("mov", "ebp, dword ptr [rbx + 0x20]")),
    ("要的是 PlanePosManager 模块（wk = 0x46c9e0）", 0x3E32F4, ("call", "46c9e0")),
    ("→ insert(0x3c64c0)", 0x3E3347, ("call", "3c64c0")),
]

# 7) 关卡生成器 0x465a70：初始值也写进同一张表
CHAIN_LEVELGEN = [
    ("要的是 PlanePosManager 模块", 0x466E20, ("call", "46c9e0")),
    ("初始三元组：x ← [rsp+0x70]", 0x465F54, ("movaps", "xmm0, xmmword ptr [rsp + 0x70]")),
    ("第三位 = 0.5", 0x465F6D, ("movabs", "rax, 0x3fe0000000000000")),
    ("→ insert(0x3c64c0)", 0x465F85, ("call", "3c64c0")),
]


def load_text(so):
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64
    from loc_xrefs import Elf
    e = Elf(so)
    sec = e.sec(".text")
    va, size = sec["addr"], sec["size"]
    data = e.read(va, size)          # Elf.read() 吃的是**虚地址**，不是文件偏移
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = False
    ins = {}
    for i in md.disasm(data, va):
        ins[i.address] = (i.mnemonic, i.op_str)
    return ins


def check(ins, chain, verbose):
    bad = []
    for why, addr, (mn, op) in chain:
        got = ins.get(addr)
        ok = got is not None and got[0] == mn and op in got[1]
        tail = "" if got is None else "%s %s" % got
        mark = "ok  " if ok else "FAIL"
        if verbose or not ok:
            print("  [%s] 0x%x  %-46s  %s" % (mark, addr, why, tail))
        if not ok:
            bad.append((why, addr, tail))
    return bad


def main():
    verbose = "--verbose" in sys.argv or "-v" in sys.argv
    if "--self-test" in sys.argv:
        return self_test()
    so = find_so()
    sha = hashlib.sha256(open(so, "rb").read()).hexdigest()
    print("so   %s" % so)
    print("sha  %s" % sha)
    if sha != EXPECT_SHA:
        sys.exit(
            "\n[FAIL] sha256 与 EXPECT_SHA 不符 —— 地址会整体漂移，先重跑 docs/06 §47 的复跑步骤。\n"
            "       期望 %s\n" % EXPECT_SHA
        )
    sys.path.insert(0, HERE)
    ins = load_text(so)

    groups = [
        ("① 计算器只认 [arg2+0x10] 这一张表", CHAIN_TABLE_ARG),
        ("② 采样器调用点（唯一）与它的 x0/y0 栈槽", CHAIN_SAMPLER),
        ("③ 两个栈槽是表项的逐字节拷贝", CHAIN_COPY),
        ("④ 表的 get（0x3c62b0）= Option<(f64,f64,f64)>", CHAIN_GET),
        ("⑤ 表的 insert（0x3c64c0）= 24 字节值 + u32 key", CHAIN_INSERT),
        ("⑥ 事件处理器 0x3e2830 把 FunctionPoints 原样抄进表", CHAIN_EVENT_WRITER),
        ("⑦ 关卡生成器 0x465a70 写初始三元组（第三位 0.5）", CHAIN_LEVELGEN),
    ]
    bad = []
    for title, chain in groups:
        print("\n" + title)
        bad += check(ins, chain, verbose)

    # insert 的调用者必须是 2 个（多一个 ⇒ "只有两个写入者"这条过期）
    print("\n⑧ insert(0x3c64c0) 的调用者")
    callers = sorted(a for a, (mn, op) in ins.items()
                     if mn == "call" and op.replace("0x", "").startswith("3c64c0"))
    print("   call rel32 = %d  %s" % (len(callers), [hex(x) for x in callers]))
    if [hex(x) for x in callers] != ["0x3e3347", "0x465f85"]:
        bad.append(("insert 调用者集合", 0x3C64C0, str([hex(x) for x in callers])))

    # 采样器槽 `0x9bac98` 在 0x3c2bc0 体内只能被引用 1 次（多一处 ⇒ "唯一采样点"过期）
    print("\n②b `0x3c2bc0` 体内引用槽 `0x9bac98`（= `rip + 0x5f7bd1`）的次数")
    n = [a for a, (mn, op) in ins.items() if 0x3C2BC0 <= a < 0x3C5C4D and "0x5f7bd1" in op]
    print("   = %d  %s" % (len(n), [hex(x) for x in n]))
    if n != [0x3C30C1]:
        bad.append(("槽 0x9bac98 在 0x3c2bc0 内的引用", 0x3C2BC0, str([hex(x) for x in n])))

    print("\n" + ("全部断言通过。" if not bad else "%d 条断言失败 —— 文档 §47 需要重做。" % len(bad)))
    return 1 if bad else 0


def self_test():
    """② 正对照：把一条断言改错，`check()` 必须报 FAIL —— 证明它不是"永远绿"。"""
    so = find_so()
    sys.path.insert(0, HERE)
    ins = load_text(so)
    broken = [("故意写错的期望", 0x3C30C1, ("call", "qword ptr [rip + 0xdead]"))]
    bad = check(ins, broken, verbose=False)
    ok = len(bad) == 1
    print("[self-test] 假断言被抓到：%s" % ("是（工具不是永远绿）" if ok else "否 —— 工具坏了"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
