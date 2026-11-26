#!/usr/bin/env python3
"""bitexact_plan.py —— 生成 T13 的**预注册清单** `fixtures/bit_exact_plan.txt`。

任务书 `.agent-work/specs/T13-bit-exact-sweep.md` ①：
  * 439 个 `观测` 级函数的清单（函数名 / Ghidra 地址 / 我方文件:行）；
  * 静态可执行性分类器（`tools/bitexact_classify.py`）的三档结果 + 可复核理由；
  * `executable` + `needs_state` 里**要跑哪些**、每个函数**输入集怎么生成**（≥16 组，含边界）；
  * **先写进 fixture 再跑**，跑完不许改判据。

本脚本只读：`docs/coverage-matrix.tsv`（模块→档位）、`docs/02-module-map.md` §18 的
"方法 | 类型 | x86 地址 | 源位置"表（327 行，用来把地址接回函数名与我方 `src/…:行`）、
`tools/bitexact_classify.py` 的分类结果。

用法：
    ../.venv/bin/python tools/bitexact_plan.py            # 打印到 stdout
    ../.venv/bin/python tools/bitexact_plan.py --write    # 写 fixtures/bit_exact_plan.txt
"""

import argparse
import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
MAIN = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(HERE))))
if not os.path.exists(os.path.join(MAIN, ".reference")):
    MAIN = os.path.dirname(ROOT)

ROW = re.compile(r"^\|\s*`([^`]+)`\s*\|\s*`([^`]+)`\s*\|\s*`(0x[0-9a-fA-F]+)`\s*\|\s*`?([^`|]+?)`?\s*\|")


def load_addr2src():
    """docs/02 §18 的「方法 | 类型 | x86 地址 | 源位置」表 → {addr: (name, ty, src)}。"""
    out = {}
    with open(os.path.join(ROOT, "docs", "02-module-map.md")) as f:
        for line in f:
            m = ROW.match(line)
            if m:
                a = int(m.group(3), 16)
                out.setdefault(a, (m.group(1), m.group(2), m.group(4).strip()))
    return out


# ---------------------------------------------------------------- 输入集规格
# 预注册：每个函数族的输入集生成方式，跑之前定死。**≥16 组**，含 NaN/±0/±inf/1ulp/负零。
INPUT_SPEC = {
    "f64_scalar": [
        ("+0.0", 0x0000000000000000), ("-0.0", 0x8000000000000000),
        ("+1.0", 0x3FF0000000000000), ("-1.0", 0xBFF0000000000000),
        ("min_denormal", 0x0000000000000001), ("min_normal", 0x0010000000000000),
        ("max_finite", 0x7FEFFFFFFFFFFFFF), ("+inf", 0x7FF0000000000000),
        ("-inf", 0xFFF0000000000000), ("qNaN", 0x7FF8000000000000),
        ("-qNaN", 0xFFF8000000000000), ("1.0+1ulp", 0x3FF0000000000001),
        ("1.0-1ulp", 0x3FEFFFFFFFFFFFFF), ("pi", 0x400921FB54442D18),
        ("0.5", 0x3FE0000000000000), ("1e308", 0x7FE1CCF385EBC8A0),
        ("1e-308", 0x000FFFFFFFFFFFFF), ("1234567890123456789.0", 0x43E123456789ABCD),
    ],
    "tokens": [
        "空序列（调用方不调，见 convert）", "单个原子 CONST", "单个原子 VAR_X",
        "bal:+ - * / ^ 各一条二元表达式", "嵌套括号 (x+(y*z))", "括号不配对：多 )",
        "括号不配对：多 (", "一元 sqrt/ln/exp/abs/sin/cos/tan 各一条",
        "一元 neg(-)", "平级左结合 a-b+c", "平级右结合 a-(b+c)",
        "+ 缺左操作数", "+ 缺右操作数", "- 缺左操作数",
        "未知 tag（20/21/0xdead）", "深嵌套 32 层括号",
        "CONST 值取 NaN/±0/±inf/1ulp（值不影响根的选择，但必须逐位保留）",
        "长序列 64 token 混合表达式",
    ],
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true")
    a = ap.parse_args()

    # 分类结果（先跑分类器拿 JSON；这里不发它，避免把判据耦合进本脚本）
    cj = os.path.join(ROOT, ".bitexact_classify.json")
    if not os.path.exists(cj):
        subprocess.check_call([sys.executable, os.path.join(HERE, "bitexact_classify.py"),
                               "--json", cj], stderr=subprocess.DEVNULL)
    rows = json.load(open(cj))
    a2s = load_addr2src()

    from collections import Counter
    cnt = Counter(r["tier"] for r in rows)
    L = []
    W = L.append
    W("# fixtures/bit_exact_plan.txt —— T13「证据升档 sweep」**预注册清单**")
    W("#")
    W("# 这张表在**跑任何东西之前**写定并单独 commit（任务书 ① ）。跑完不改判据；")
    W("# 要加要改必须另起 commit 并写明为什么。生成它的是 tools/bitexact_plan.py，")
    W("# 分类判据在 tools/bitexact_classify.py 的 docstring 里（判据 1-8，每条都要能指到指令/符号）。")
    W("#")
    W("# 口径：`观测` 级函数 = docs/coverage-matrix.tsv 里 status=观测 的 67 个模块的")
    W("# `orig_funcs` 之和 = 439（tools/coverage_matrix.py 的 panic::Location 归属口径，")
    W("# 地址是 **ELF vaddr**；Ghidra/REA 地址 = 本地址 + 0x100000，只加一次）。")
    W("# ⚠️ 档位是**按模块**判的（matrix 的 status 列），不是按函数判的 —— 所以")
    W("#   「439 个观测级函数」严格说是「落在观测级模块里的 439 个可归属函数」。")
    W("#")
    W("# 三档定义：")
    W("#   executable           指针参数只有「只读输入」与「只写输出」两种，不需要任何既有状态")
    W("#   needs_state          有既被读又被写的指针参数（`&mut self` 一类）或写全局 —— 状态是纯数据、可造")
    W("#   isolated_impossible  本体或热路径传递调用里出现引擎派发 / TLS / sys_io / thread / unwind")
    W("#")
    W("# 统计：%s（合计 %d）" % (dict(cnt), len(rows)))
    W("")
    W("=" * 100)
    W("A. 全部 439 个函数的分类（地址 | 模块 | 函数名 | 我方源位置 | 档位 | 理由 | 指令数）")
    W("=" * 100)
    W("# 函数名 / 我方源位置 来自 docs/02-module-map.md §18 的「方法|类型|x86 地址|源位置」表；")
    W("# 表里没有的（`-`）就是 199 个**匿名单态化/闭包**（weak_caller 39、module_container 20 等），")
    W("# 只能按地址 + 模块 + 锚点数定位。")
    W("%-10s %-42s %-34s %-40s %-20s %-30s %s" %
      ("addr", "module", "fn", "our_src", "tier", "reason", "ninsn"))
    for r in sorted(rows, key=lambda x: (x["module"], x["addr"])):
        nm, ty, src = a2s.get(r["addr"], ("-", "-", "-"))
        W("%-10s %-42s %-34s %-40s %-20s %-30s %d" %
          (hex(r["addr"]), r["module"], nm, src, r["tier"], r["reason"], r["n_insn"]))
    W("")
    W("=" * 100)
    W("B. 分类判据（可复核；每条都能指到指令/符号）")
    W("=" * 100)
    W("""
1 engine_dispatch  `call *off(%reg)` 而 %reg 从「运行时才填的全局槽」读出。实测就是 gdext 的引擎接口表：
                   0x8abca0（draw_arc 包装）`mov 0x10e9a0(%rip),%r15  # 9ba6b0` → `movzbl 0x14f0(%r15)`。
                   隔离环境里 [0x9ba6b0] 恒 0 ⇒ 必崩。判据落在 call 指令地址 + 全局槽地址上（分类器 --json 可复现）。
2 runtime_ptr      `call *0x…(%rip)` 的槽在 .rela.dyn 里**连重定位都没有** ⇒ 只可能由 GDExtension 入口运行时写。
3 tls              任何 `fs:`/`gs:` 段内存操作数，或直接调用 `__tls_get_addr`（thread_rng / std::io 在这里）。
4 sys_io           直接调用 read/write/socket/send/recv/open/statx/mmap64/clock_gettime/syscall/…（名单在脚本里）。
5 thread           pthread_create/join/detach/key_*/setspecific/self。
6 unwind           _Unwind_* / __cxa_thread_atexit_impl。
7 冷路径不传播     `call *slot` 的下一条指令是 int3/ud2 的是 panic/print_error 收尾（实测 0x925660 =
                   __rust_alloc_error_handler：`call *0x9ba698 ; int3`）。这类边**不参与**致命性传递。
7b 非返回错误路径  函数体里**没有 ret** 且自身致命（0x94b340 = panic 入口）的节点，致命性**不向上传播**。
                   不做这条会把整个二进制连坐：439 个里 264 个是这么被 panic 连坐的。
8 传递            直接 call / 可解析的间接 call 边向上传播最坏档；**只走非冷边**。
                  间接 call 解析：.rela.dyn 的 R_X86_64_RELATIVE 有 addend（实测 8231 条指向 .text）⇒ 直接得目标；
                  GLOB_DAT/JUMP_SLOT ⇒ libc 导入名；`call *%reg` 回溯 ≤15 条找 `mov %reg,[%rip+G]`。
                  回溯不出 ⇒ indirect_unknown，**按 needs_state 处理，不许假装能跑**。
9 needs_state 判据 某指针参数「既被读又被写」（读=作基址访问或 lea 取地址；写=作目的基址）⇒ `&mut self` 一类；
                  或有写全局（`mov [%rip+全局], …`）。
10 executable 判据 指针参数只有「只读输入」与「只写输出」；无全局写；无 indirect_unknown。
""".strip())
    W("")
    W("=" * 100)
    W("C. 输入集生成方式（预注册；**≥16 组**，跑之前定死）")
    W("=" * 100)
    W("C1. f64 标量（18 组，全部按 u64 位模式给，两侧都按位喂，避免十进制往返）：")
    for nm, bits in INPUT_SPEC["f64_scalar"]:
        W("    %-24s 0x%016x" % (nm, bits))
    W("")
    W("C2. Vec<FunctionToken>（16B/token = {tag: u64, value: f64}）输入序列（18 组）：")
    for i, s in enumerate(INPUT_SPEC["tokens"], 1):
        W("    %2d  %s" % (i, s))
    W("")
    W("比较口径：**逐位**。f64 用 `to_bits()` 比 u64；整数/字节直接比；")
    W("Vec 输出比 (len, 每个 token 的 tag u64 与 value 的 u64 位模式)。**不用 abs diff < eps**。")
    W("")
    W("=" * 100)
    W("D. 预注册的**执行清单**（跑哪些 + 每组怎么喂）")
    W("=" * 100)
    W("""
D-A 确定能跑（ABI 已由反汇编逐指令读出，输入状态是可造纯数据）
  A1  0x4f5450  PolishNotationConverter::reorder_recursive
      我方 src/polish_notation_converter.rs（同名 recusive；docs/02 §18 记 :36）
      真侧 ABI：rdi=&mut self{ input Vec @+0x00/+0x08/+0x10, output Vec @+0x18/+0x20/+0x28 }，
               rsi=start(usize), rdx=end(usize 闭区间)。insn `0x4f545e cmp %rdx,%rsi; jbe`
               与 `0x4f54ca mov 0x10(%rdi),%rsi` / `0x4f54a2 movsd %xmm0,0x8(%rax,%rcx,1)`
               把两个 Vec 的 (cap,ptr,len) 与 16B/token 落点全部钉死。
      输入：input Vec 用 C2 的 18 组序列（gwemu 的 make_vec 造真 Vec 布局）；
            output Vec = {cap=0, ptr=8(dangling), len=0}，与 `0x4edce9..0x4edcf9` 三句 movq 一致。
      输出：读回 output 的 (ptr@+0x20, len@+0x28) 前 len 个 16B token。
      我方：`PolishNotationConverter::reorder(input)`（内部就是
            `if !input.is_empty() { reorder_recursive(0, len-1) }`，与原版调用点同口径）。
  A2  0x41a9d0 / 0x41ab40  graphwar_function_evolutionator
      ABI 待**反汇编逐指令确认**（argv 形态初步是 in:rcx,rdi,rdx / out:rsi）。
      规则：装不上就记「没跑成」，不许猜。
  A3  0x4600e0 / 0x4602f0  graphwar_function_evolutionator（in:rdi,rdx）同上。
D-B 尝试（状态可造但需要真 HashMap/Weak，代价高；跑不成如实记）
  B1  0x465870 graphwar_target_manager / 0x4783f0 graphwar_entity_effect_manager /
      0x4556d0 graphwar_soldier_manager / 0x518530 graphwar_mode_manager
      —— 事件分派型（tag 比较 + 写 1~2 字节），self 是纯字节表。
  B2  0x3d3250（weak_caller 的一个单态化）—— Weak<RefCell<T>> 语义，strong=0/1 两态。
D-C **不跑**（如实记账，绝不假装跑过）
  * 304 个 isolated_impossible：要 Godot 引擎 / TLS / sys_io，隔离执行不可能；
  * 102 个 needs_state 中状态造不出来（真 HashMap<TypeId,Rc<dyn Any>>、真 Rc 图、真 mpsc 通道）的；
  * 任何 ABI 定不下来的（记「ABI 未知」而不是猜着喂）。
""".strip())
    W("")
    W("=" * 100)
    W("E. 正对照（跑之前就要过）")
    W("=" * 100)
    W("""
挑 3 个 docs/06 已标「逐位」的**真函数**（都不在 439 里，是外部基准）：
  P1 0x4eb9d0 求值器（docs/06 §35 自校验点，gwemu.call_eval 已在用）
  P2 0x4ed670 解析器（math_function.rs 的 panic 锚落点）
  P3 0x3c2530 采样器（docs/91：24/24 用例整条点序列逐位相等）
要求：① 我方/真侧在同一组输入上 **100% 报相同**；
     ② 把某一个输入**人为改 1 ulp**，harness 必须报**差异**（证明不是"永远报相同"）。
""".strip())
    txt = "\n".join(L) + "\n"
    if a.write:
        out = os.path.join(ROOT, "fixtures", "bit_exact_plan.txt")
        with open(out, "w") as f:
            f.write(txt)
        print("wrote %s (%d lines)" % (out, len(L)))
    else:
        sys.stdout.write(txt)
    return 0


if __name__ == "__main__":
    sys.exit(main())
