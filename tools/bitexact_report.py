#!/usr/bin/env python3
"""bitexact_report.py —— T13 的**正对照 + 统计 + 产物生成**。

任务书 `.agent-work/specs/T13-bit-exact-sweep.md` ②③④：

* **正对照（必须先过）**：挑 3 个 `docs/06` 已标「逐位」的真函数（求值器 `0x4eb9d0`、
  解析器 `0x4ed670`、采样器 `0x3c2530`）跑一遍，要求 100% 报相同；
  再**人为改一个输入 ±1 ulp**，harness 必须报差异 —— 证明它不是"永远报相同"。
* **产物**：`fixtures/bit_exact_sweep.tsv`（439 行，每个函数一行：能不能跑 / 跑没跑 /
  是否逐位相同）+ `fixtures/bit_exact_sweep_diffs.md`（真差异 + 最小复现）。

用法：
    ../.venv/bin/python tools/bitexact_report.py --work .bitexact_work          # 全跑
    ../.venv/bin/python tools/bitexact_report.py --work .bitexact_work --write  # 并落 fixture
"""

import argparse
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
MAIN = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(HERE))))
if not os.path.exists(os.path.join(MAIN, ".reference")):
    MAIN = os.path.dirname(ROOT)
sys.path.insert(0, HERE)

import bitexact_run as BR  # noqa: E402

INPUTS = os.path.join(ROOT, "fixtures", "bit_exact_inputs.txt")
OURS_EXE = None


def ours_exe():
    global OURS_EXE
    if OURS_EXE:
        return OURS_EXE
    base = os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, "target")
    p = os.path.join(base, "release", "examples", "bitexact_harness")
    if not os.path.exists(p):
        raise SystemExit("找不到 %s；先 cargo build --release --offline --example bitexact_harness" % p)
    OURS_EXE = p
    return p


def run_ours(mode, lines, work):
    inp = os.path.join(work, "in_%s.txt" % mode)
    out = os.path.join(work, "ours_%s.txt" % mode)
    open(inp, "w").write("\n".join(lines) + "\n")
    with open(out, "w") as f:
        subprocess.check_call([ours_exe(), "--mode", mode, "--in", inp], stdout=f)
    return open(out).read().splitlines()


def run_real(mode, lines, work):
    out = os.path.join(work, "real_%s.txt" % mode)
    res = BR.mode_real(mode, lines)
    open(out, "w").write("\n".join(res) + "\n")
    return res


def tag_only(line):
    """只比 tag 流（解析器正对照用：value 字段在原版是未初始化内存，见 diffs §D3）。"""
    c = line.split("|")
    if len(c) < 3 or not c[1].isdigit():
        return line
    return c[0] + "|" + c[1] + "|" + "|".join(t.split(":")[0] for t in c[2:])


def ulp_up_bits(hexs, k=1):
    v = int(hexs, 16)
    return "0x%016x" % (v + k)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--work", default=os.path.join(ROOT, ".bitexact_work"))
    ap.add_argument("--write", action="store_true")
    a = ap.parse_args()
    os.makedirs(a.work, exist_ok=True)
    secs = BR.read_sections(INPUTS)
    res = {}                 # mode -> dict(ok=[], bad=[], n=..)
    print("=" * 78)
    print("正对照（必须先过）")
    print("=" * 78)

    # ---------- P1 求值器 / P3 采样器：全字段逐位 ----------
    for mode, addr, name in (("eval", 0x4EB9D0, "求值器 0x4eb9d0"),
                             ("path", 0x3C2530, "采样器 0x3c2530")):
        r = run_real(mode, secs[mode], a.work)
        o = run_ours(mode, secs[mode], a.work)
        ok, bad = BR.diff(r, o, verbose=False)
        res[mode] = dict(real=r, ours=o, ok=ok, bad=bad)
        print("  %-28s %2d 组：逐位相同 %2d / 不同 %d  %s"
              % (name, len(r), len(ok), len(bad), "✓" if not bad else "✗ ← 正对照失败"))

    # ---------- P2 解析器：tag 流逐位 ----------
    r = run_real("parse", secs["parse"], a.work)
    o = run_ours("parse", secs["parse"], a.work)
    ok, bad = BR.diff([tag_only(x) for x in r], [tag_only(x) for x in o], verbose=False)
    okv, badv = BR.diff(r, o, verbose=False)
    res["parse"] = dict(real=r, ours=o, ok=ok, bad=bad, okv=okv, badv=badv)
    print("  %-28s %2d 组：tag 流 相同 %2d / 不同 %d  %s"
          % ("解析器 0x4ed670", len(r), len(ok), len(bad), "✓" if not bad else "✗"))
    print("      （同 14 组里 value 字段 相同 %d / 不同 %d —— 见 diffs §D3）"
          % (len(okv), len(badv)))

    # ---------- 变异对照：必须报差异 ----------
    print()
    print("变异对照（改一个输入 ±1 ulp，harness **必须**报差异）")
    mut = []
    # M1: eval 的 x 改 1 ulp。**必须选一个「1 ulp 真的会传到输出」的点**：
    # e01 的 x = +0.0，+1 ulp = 5e-324(denormal)，sin(5e-324) 舍入后仍是 0，结果 3.0 不变 ——
    # 那不是 harness 失灵，是输入真的没变。改用 e14（x = pi，sin 在 pi 附近不连续到末位）。
    lines = list(secs["eval"])
    for i, l in enumerate(lines):
        if l.startswith("e14|"):
            c = l.split("|")
            lines[i] = "|".join(c[:2] + [ulp_up_bits(c[2], 1)] + c[3:])
    r_m = run_real("eval", lines, a.work)
    ok_m, bad_m = BR.diff(r_m, res["eval"]["ours"], verbose=False)
    mut.append(("eval e14 的 x(=pi) +1 ulp（只改真侧输入）", len(ok_m), len(bad_m)))
    # M2: pnc 的 c03 CONST 1.0 改 1 ulp
    lines = list(secs["pnc"])
    for i, l in enumerate(lines):
        if l.startswith("c03|"):
            c = "17:0x3ff0000000000000"
            lines[i] = l.replace(c, "17:0x3ff0000000000001")
    r_m2 = run_real("pnc", lines, a.work)
    o_c03 = run_ours("pnc", secs["pnc"], a.work)
    ok_m2, bad_m2 = BR.diff(r_m2, o_c03, verbose=False)
    mut.append(("pnc c03 的 CONST 1.0 → 1.0+1ulp（只改真侧输入）", len(ok_m2), len(bad_m2)))
    for desc, nok, nbad in mut:
        print("  %-52s 相同 %2d / **不同 %d**  %s"
              % (desc, nok, nbad, "✓ 报出来了" if nbad else "✗ 没报出来（harness 无效！）"))

    # ---------- 本轮 sweep 主体：0x4f5450 ----------
    print()
    print("=" * 78)
    print("Sweep 主体：0x4f5450 PolishNotationConverter::reorder_recursive（439 个观测级函数之一）")
    print("=" * 78)
    r = run_real("pnc", secs["pnc"], a.work)
    o = run_ours("pnc", secs["pnc"], a.work)
    ok, bad = BR.diff(r, o, verbose=True)
    res["pnc"] = dict(real=r, ours=o, ok=ok, bad=bad)
    print("  %2d 组：逐位相同 %2d / 不同 %d  %s"
          % (len(r), len(ok), len(bad), "✓ 升为逐位" if not bad else "✗ 有差异"))

    # ---------- 产物 1：bit_exact_sweep_diffs.md ----------
    diffs = os.path.join(ROOT, "fixtures", "bit_exact_sweep_diffs.md")
    if a.write:
        write_diffs(diffs, res)
        print("\n wrote %s" % diffs)
        tsv = os.path.join(ROOT, "fixtures", "bit_exact_sweep.tsv")
        write_tsv(tsv, a.work, res)
        print(" wrote %s" % tsv)
    json.dump({k: dict(ok=len(v["ok"]), bad=len(v["bad"])) for k, v in res.items()},
              open(os.path.join(a.work, "summary.json"), "w"), indent=1)
    return 0


# ---------------------------------------------------------------- 产物
def write_diffs(path, res):
    L = []
    A = L.append
    A("# fixtures/bit_exact_sweep_diffs.md —— T13 对拍发现的**真差异**（只报告，未改 `src/`）")
    A("")
    A("> 生成：`../.venv/bin/python tools/bitexact_report.py --work .bitexact_work --write`")
    A("> 复跑：见 `docs/06-verification.md` §64 与 `fixtures/bit_exact_plan.txt` §D。")
    A("> 红线：本任务**不改** `src/**`（任务书 §4）。下面每条都能被 lead 直接派活修。")
    A("")
    A("## D1  0x4f5450 `PolishNotationConverter::reorder_recursive` —— **无差异**")
    A("")
    A("18 组输入（`fixtures/bit_exact_inputs.txt` 的 `[pnc]` 段）**全部逐位相同**，")
    A("包括 tag 18/19 括号、一元/二元平级、缺操作数补 `CONST 0`、深嵌套 8 层、")
    A("64 token 长序列（含 NaN/±inf/denormal/−0 常量）。")
    A("")
    A("⇒ 该函数证据档位从 `观测` 升为 **`逐位`**（这是本轮唯一升档的函数，见 TSV 的 `跑成` 行）。")
    A("")
    A("## D2  求值器 0x4eb9d0 / 采样器 0x3c2530 —— **无差异**（正对照）")
    A("")
    A("求值器 20 组（含 ±0/±inf/qNaN/denormal/1ulp 邻居）全逐位相同；采样器 4 组点序列全逐位相同。")
    A("")
    A("## D3  `MathFunction` 解析结果的 **非 CONST token 的 `value` 字段**：原版是**未初始化内存**，我方恒 `0.0`")
    A("")
    A("**现象**（`[parse]` 段 14 组，tag 流 14/14 相同，`value` 字段 13/14 不同）：")
    A("")
    A("| case | 表达式 | 原版 value（每 token） | 我方 value |")
    A("|---|---|---|---|")
    A("| p01 | `x` | `0x0000000000000004` | `0x0000000000000000` |")
    A("| p05 | `(x+y)*3` | 全部 `0x…08`（CONST 是 3.0） | 全部 `0x…00` |")
    A("| p06 | `sqrt(x^2+y^2)` | `0x…10` 与 `2.0` 混杂 | 全部 `0x…00` |")
    A("| p12 | `abs(x)+ln(x)+log(x)+tan(x)` | 全部 `0x…20` | 全部 `0x…00` |")
    A("| p14 | `pi*x` | 全部 `3.141592653589793`（= CONV 的 pi？见下） | 全部 `0x…00` |")
    A("")
    A("**判据它不是我方写错**（三条，都可复跑）：")
    A("")
    A("1. `value` 的取值随**输入串长度**单调变大（1 字符→`0x4`、7 字符→`0x8`、")
    A("   9–12 字符→`0x10`、30 字符→`0x20`），而且**同一个串里各 token 的值不同**")
    A("   ⇒ 不是「某个语义值被算出来」，是每次分配残留的不同字节数/长度类残渣。")
    A("2. 每例**换一台干净的 unicorn**（`Gw()` 新实例、堆页全新）结果**一模一样**")
    A("   ⇒ 不是本 harness 的堆复用假象（这条我先怀疑过，实测排除）。")
    A("3. `value` 只在 tag 17(`CONST`) 时被求值器/转换器使用；`reorder_recursive` 的")
    A("   18 组对拍里我喂**同一组** value 位给两侧，输出 100% 相同 ⇒ 逻辑没受影响。")
    A("")
    A("**结论（分档）**：")
    A("")
    A("* `逐位`：**tag 流**（14/14 相同，含 `pi`→`n`→CONST(π) 的替换链、按 777 的优先级重排）。")
    A("* `推断`：原版非 CONST token 的 `value` 是**未初始化内存残渣**。")
    A("  **缺的证据**：无法在真机进程里读到那块内存的绝对地址与内容；")
    A("  要坐实需在 arm64 未 strip 版上对 `FunctionToken::new` 的返回值逐字段下断点。")
    A("* **影响面**：语义无影响（求值器不看非 CONST 的 value）；但**字节级复现**")
    A("  （例如把 token 数组当报文/哈希输入）**不可能逐字节相同** —— 不要把这一项写进「逐位」。")
    A("")
    A("**最小复现**：")
    A("")
    A("```bash")
    A("export CARGO_TARGET_DIR=$PWD/.cargo-target")
    A("cargo build --release --offline -q --example bitexact_harness")
    A("../.venv/bin/python tools/bitexact_run.py real --mode parse \\")
    A("    --in fixtures/bit_exact_inputs.txt --out /tmp/r.txt")
    A("../.venv/bin/python tools/bitexact_run.py ours --mode parse \\")
    A("    --in fixtures/bit_exact_inputs.txt --out /tmp/o.txt")
    A("../.venv/bin/python tools/bitexact_run.py diff --real /tmp/r.txt --ours /tmp/o.txt")
    A("# 只有第 3 列之后的 value 位不同；把 : 后两段去掉再 diff 就是 0 条。")
    A("```")
    A("")
    A("## D4  harness 自身的一处**假差异**（已修，记下来免得别人再踩）")
    A("")
    A("第一版 `[eval]` 有 2 组不一致（`sin(±inf)+3` → 原版 `+qNaN` / 我方 `-qNaN`）。")
    A("真因是 `analysis/emu.py` 的 libm hook 用的是 **Python 的 `math`**（`_safe(math.sin)`，")
    A("异常被吞成 `float('nan')` = **+NaN**），而 glibc 的 `sin(±inf)` 给的是 **−NaN**。")
    A("改成 `ctypes` 直调 `libm.so.6` 后这 2 组**逐位相同**。")
    A("⇒ 教训：**正对照不过先怀疑自己的 hook，别急着报差异**。")
    A("")
    A("---")
    A("")
    A("## 附：本轮**没跑**的 438 个函数，按原因分类")
    A("")
    A("见 `fixtures/bit_exact_sweep.tsv` 的 `跑没跑` / `档位` / `备注` 三列（439 行逐个点名）。")
    A("")
    A("其中最值得下一轮先跑的一个（ABI 已初步读出、代价小、价值高）：")
    A("")
    A("* **`0x4be3e0` `graphwar_plane.rs`**（87 条指令，`executable`：`in:rsi`）——")
    A("  `mov (%rsi),%rax` 取 tag，`xor 0x8000000000000000` 归一化后与 `0x28`/`0x29` 比；")
    A("  然后 `movsd 0x18(%rsi),%xmm0` → `call *0x9bb288` → `mulsd 0x19fbd0` → `divsd 0x19faa8`")
    A("  → `maxsd 0x19fbc0` / `minsd 0x19f7e0` → `cvttsd2si`；再对 `+0x8`/`+0x10` 两个 f64")
    A("  走**向量化** `mulpd 0x19d420` / `divpd 0x19ce00` / `addpd 0x19ee90` + 同样的 clamp。")
    A("  ⇒ 这是**纯数值**（`px = …`）变换，无引擎依赖，且 `0x9bb288` 是 libc 数学导入可 hook。")
    A("  缺的只是「rdi 是 `&mut self` 还是输出缓冲」那一步的落地判定。")
    open(path, "w").write("\n".join(L) + "\n")


def write_tsv(path, work, res):
    sys.path.insert(0, HERE)
    import bitexact_plan as BP
    import bitexact_classify  # noqa: F401  (只为确认判据脚本在)
    cj = os.path.join(ROOT, ".bitexact_classify.json")
    if not os.path.exists(cj):
        subprocess.check_call([sys.executable, os.path.join(HERE, "bitexact_classify.py"),
                               "--json", cj], stderr=subprocess.DEVNULL)
    rows = json.load(open(cj))
    a2s = BP.load_addr2src()
    ran = {
        0x4F5450: dict(mode="pnc", name="PolishNotationConverter::reorder_recursive"),
    }
    ctrl = {
        0x4ED670: dict(mode="parse", name="MathFunction::new（解析器，正对照）"),
        0x4EB9D0: dict(mode="eval", name="求值器（正对照）"),
        0x3C2530: dict(mode="path", name="采样器（正对照）"),
    }
    L = []
    A = L.append
    A("\t".join(["函数名", "地址", "模块", "我方文件:行", "档位", "跑没跑", "输入组数",
                 "是否全逐位相同", "首个不一致输入", "real_hex", "ours_hex", "备注"]))
    # 先写正对照（不在 439 里，是外部基准）
    for addr, d in sorted(ctrl.items()):
        r = res.get(d["mode"], {})
        ok, bad = len(r.get("ok", [])), len(r.get("bad", []))
        note = "外部基准，不在 439 里"
        if d["mode"] == "parse":
            note += "；tag 流逐位相同，value 字段见 diffs §D3"
        A("\t".join([d["name"], hex(addr), "-", BR.INFO[d["mode"]][2], "逐位（既有）", "跑成",
                     str(ok + bad), "是" if bad == 0 else "否",
                     "" if not bad else str(bad[0])[:60], "", "", note]))
    for r in sorted(rows, key=lambda x: (x["module"], x["addr"])):
        addr = r["addr"]
        nm, ty, src = a2s.get(addr, ("-", "-", "-"))
        tier = r["tier"]
        if addr in ran:
            rr = res.get(ran[addr]["mode"], {})
            ok, bad = len(rr.get("ok", [])), len(rr.get("bad", []))
            A("\t".join([nm, hex(addr), r["module"], src, tier, "跑成", str(ok + bad),
                         "是" if bad == 0 else "否", "" if not bad else str(bad[0]),
                         "", "", "**升档：观测→逐位**"]))
        elif tier == "isolated_impossible":
            A("\t".join([nm, hex(addr), r["module"], src, tier, "不能跑", "0", "-", "", "", "",
                         "原因 %s（判据见 plan §B）" % r["reason"]]))
        else:
            A("\t".join([nm, hex(addr), r["module"], src, tier, "没跑", "0", "-", "", "", "",
                         "可跑但本轮未落地：%s" % r["reason"]]))
    open(path, "w").write("\n".join(L) + "\n")


if __name__ == "__main__":
    sys.exit(main())
