#!/usr/bin/env python3
"""B 层：AI 进化器的 **25 s 预算到底是墙钟还是迭代计数** + 我方实现的耗时实测。

三件事：

* `--static`  静态链：预算比较点 → 取时间函数 → `clock_gettime(CLOCK_MONOTONIC)`
  （GOT 槽用 `.rela.dyn` 解，不看文档抄的地址）；
* `--emu`     动态判死：把「取时间函数」的 GOT 槽换成**可控桩**（返回指定
  Duration），再让真二进制的预算块自己跑一遍 —— 换时钟值就换出口 ⇒ 不是迭代计数；
* `--runs`    我方实现（`examples/ai_explain`，release）在**本机**跑难度 6 / 难度 5
  等档位的墙钟耗时与评分数 ⇒ 裁决 `docs/13` §8 那条「难度 6 跑不完」。

产出：`fixtures/ai_behaviour_runs.tsv`（`--runs`）。

```bash
.reference/venv/bin/python tools/ai_budget_probe.py --static
.reference/venv/bin/python tools/ai_budget_probe.py --emu
.reference/venv/bin/python tools/ai_budget_probe.py --runs
```

⚠️ 口径：unicorn 比真机慢 1~2 个数量级，**不拿 emu 的耗时去裁决 25 s**，
它只用来判「终止条件读的是什么」。
"""

import argparse
import os
import platform
import struct
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SO = os.path.join(ROOT, ".reference/game/libgraphwarrust.so")
EMU_REPO = os.environ.get("EMU", os.path.join(ROOT, ".reference/analysis-repo"))
RUNS_TSV = os.path.join(ROOT, "fixtures/ai_behaviour_runs.tsv")

#: 预算块（起点是循环头，终点两条出口）
BUDGET_HEAD = 0x51B7B6
BUDGET_CALL = 0x51B7CD
BUDGET_CONTINUE = 0x51B7EA     # 没超时 → 继续本代
BUDGET_EXIT = 0x51CC4D         # 超时 → 跳出（与"代数到顶"共用出口）
GOT_ELAPSED = 0x9BAFE8         # GOT 槽：Instant::elapsed
GOT_RNG_STATE = 0x9BB290       # GOT 槽：thread-local RNG 状态访问器


def load_emu():
    sys.path.insert(0, os.path.join(EMU_REPO, "analysis"))
    sys.path.insert(0, EMU_REPO)
    try:
        import emu  # noqa: F401
        from gwemu import Gw
    except Exception as exc:  # pragma: no cover
        raise SystemExit("导入上游 emu/gwemu 失败（EMU=%s）：%s" % (EMU_REPO, exc))
    return Gw


# ---------------------------------------------------------------------------
# --static
# ---------------------------------------------------------------------------
def do_static():
    sys.path.insert(0, HERE)
    from ai_params_crosscheck import Elf, Code, x_budget, x_clock_chain, IS_IMM

    elf = Elf(SO)
    evo = Code(elf, 0x51B440, 0x2CC0)
    bud = x_budget(elf, evo)
    clk = x_clock_chain(elf)
    print("== B1 静态链（全部 vaddr，二进制 sha256 见 ai_params_crosscheck.py）==")
    ins = evo.at(BUDGET_HEAD)
    print("  0x%x: %-6s %s        ← 取「取时间函数」指针（GOT 0x%x → 0x%x）"
          % (ins.address, ins.mnemonic, ins.op_str, clk["elapsed_got"], clk["elapsed_fn"]))
    ins = evo.at(BUDGET_CALL)
    print("  0x%x: %-6s %s  (rax)              ← 每代一次，就在代循环顶部"
          % (ins.address, ins.mnemonic, ins.op_str))
    for a in bud["cmp25"]:
        ins = evo.at(a)
        print("  0x%x: %-6s %s" % (ins.address, ins.mnemonic, ins.op_str))
    ins = evo.at(0x51B7D3)
    print("  0x%x: %-6s %s" % (ins.address, ins.mnemonic, ins.op_str))
    ins = evo.at(0x51B7D5)
    print("  0x%x: %-6s %s" % (ins.address, ins.mnemonic, ins.op_str))
    ins = evo.at(0x51B7E4)
    print("  0x%x: %-6s %s        ← 超时出口 0x%x"
          % (ins.address, ins.mnemonic, ins.op_str, BUDGET_EXIT))
    print("  代循环回边：0x%x → 0x%x（回到预算检查前的循环头）"
          % (bud["back_edges"][0][0], bud["back_edges"][0][1]))
    print("  秒表链：0x%x --call--> 0x%x --call--> 0x%x --call--> GOT 0x%x = %s"
          % (BUDGET_CALL, clk["elapsed_fn"], clk["clock_fn"],
             clk["gettime_got"], clk["gettime_import"]))
    print("  clockid 立即数 = %d @0x%x（1 = CLOCK_MONOTONIC）⇒ **墙钟**，不是迭代计数"
          % (clk["clockid"], clk["clockid_at"]))
    print("  变体 B 的同构检查点(0x51d398)是否也在：",
          "有" if 0x51D398 in [i.address for i in evo.insns] else "窗口外/未解")
    return 0


# ---------------------------------------------------------------------------
# --emu
# ---------------------------------------------------------------------------
def _stub_duration(em, secs, nanos):
    """写一个 `movabs rax,secs; movabs rdx,nanos; ret` 的桩，返回其地址。"""
    code = (b"\x48\xb8" + struct.pack("<Q", secs)
            + b"\x48\xba" + struct.pack("<Q", nanos) + b"\xc3")
    p = em.alloc(len(code) + 16)
    em.wr(p, code)
    return p


def do_emu():
    Gw = load_emu()
    cases = [
        (0, 1, "elapsed = 1 ns", "continue"),
        (24, 0, "elapsed = 24 s", "continue"),
        (25, 0, "elapsed = 25.000000000 s（边界）", "continue"),
        (25, 1, "elapsed = 25 s + 1 ns", "exit"),
        (26, 0, "elapsed = 26 s", "exit"),
        (600, 0, "elapsed = 600 s", "exit"),
    ]
    print("== B1 动态判死：把 GOT 0x%x（elapsed）换成可控桩，跑真二进制的预算块 0x%x =="
          % (GOT_ELAPSED, BUDGET_HEAD))
    print("   块内只有 `call rax`（墙钟）与 `cmp rax,0x19`（25 秒）—— 换桩值必须换出口")
    rows = []
    for secs, nanos, label, want in cases:
        em = Gw()
        stub = _stub_duration(em, secs, nanos)
        em.wq(GOT_ELAPSED, stub)          # 把 elapsed 函数指针换成桩
        hit = {}

        def mark(name):
            def f(uc, addr):
                hit.setdefault("pc", name)
                raise StopIteration
            return f

        em.trace(BUDGET_CONTINUE, mark("continue"))
        em.trace(BUDGET_EXIT, mark("exit"))
        try:
            em.call(BUDGET_HEAD, [], [], max_steps=20000)
        except Exception as exc:  # 走到别的路径（例如解引用垃圾栈）⇒ 记为 unknown
            hit.setdefault("pc", "unknown:%s" % str(exc)[:60])
        got = hit.get("pc", "unknown:未命中任何出口")
        ok = got == want
        rows.append((label, want, got, ok))
        print("   %-28s 期望 %-8s 实到 %-8s %s"
              % (label, want, got, "✓" if ok else "✗"))
    good = sum(1 for r in rows if r[3])
    print("⇒ %d/%d 判据命中" % (good, len(rows)))
    if good != len(rows):
        print("  （动态结论只按命中项算；未命中项如实列在上面）")
    return 0 if good == len(rows) else 1


# ---------------------------------------------------------------------------
# --runs
# ---------------------------------------------------------------------------
def _bin_path():
    tgt = os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, ".cargo-target")
    p = os.path.join(tgt, "release/examples/ai_explain")
    if not os.path.exists(p):
        raise SystemExit("先构建：cargo build --release --offline --example ai_explain（缺 %s）" % p)
    return p


def _machine():
    try:
        model = ""
        with open("/proc/cpuinfo", encoding="utf-8", errors="replace") as fh:
            for line in fh:
                if line.startswith("model name"):
                    model = line.split(":", 1)[1].strip()
                    break
        load = os.getloadavg()
    except Exception:
        model, load = "", (0.0, 0.0, 0.0)
    return {
        "cpu": model,
        "nproc": str(os.cpu_count() or 0),
        "kernel": platform.release(),
        "load1": "%.2f" % load[0],
    }


def _run_once(binp, args):
    t0 = time.time()
    out = subprocess.run([binp] + args, capture_output=True, text=True)
    dt = time.time() - t0
    text = out.stdout + out.stderr
    evals = truncated = None
    for line in text.splitlines():
        if "评分次数" in line:
            part = line.split("评分次数", 1)[1]
            num = "".join(c for c in part.split("（")[0] if c.isdigit())
            evals = int(num) if num else None
            truncated = "true" in line
    gens = pop = None
    for line in text.splitlines():
        if line.startswith("代数 "):
            body = line.replace("代数 ", "")
            gens = int(body.split(" ")[0])
            seg = body.split("种群 ")[1]
            pop = int(seg.split("（")[0])
    return {"rc": out.returncode, "secs": dt, "evals": evals,
            "truncated": truncated, "gens": gens, "population": pop}


def do_runs(reps, budget_override):
    binp = _bin_path()
    mach = _machine()
    cases = [
        ("difficulty=250", ["--difficulty", "250", "--summary"], "250 代 × 250 种群"),
        ("difficulty=1000", ["--difficulty", "1000", "--summary"], "1000 代 × 250 种群（难度 6）"),
    ]
    rows = []
    print("== B2 我方实现实测（release / 本机 %s / nproc=%s / load1=%s）=="
          % (mach["cpu"], mach["nproc"], mach["load1"]))
    for name, args, note in cases:
        for i in range(reps):
            r = _run_once(binp, args)
            rows.append(dict(case=name, rep=i + 1, note=note, **r, **mach))
            print("  %-16s #%d  %7.2f s  评分 %-7s 撞墙钟=%s" %
                  (name, i + 1, r["secs"], r["evals"], r["truncated"]))
    for secs in budget_override:
        for i in range(reps):
            r = _run_once(binp, ["--difficulty", "1000", "--budget", str(secs), "--summary"])
            rows.append(dict(case="difficulty=1000&budget=%s" % secs, rep=i + 1,
                             note="与真游戏同口径（带 25 s 墙钟）", **r, **mach))
            print("  %-16s #%d  %7.2f s  评分 %-7s 撞墙钟=%s" %
                  ("budget=%s" % secs, i + 1, r["secs"], r["evals"], r["truncated"]))
    # 汇总
    print("\n== 汇总 ==")
    agg = {}
    for r in rows:
        agg.setdefault(r["case"], []).append(r)
    for case, rs in agg.items():
        secs = [x["secs"] for x in rs]
        ev = [x["evals"] for x in rs if x["evals"]]
        print("  %-22s 耗时 %s  评分 %s  撞墙钟=%s"
              % (case, " / ".join("%.2f" % s for s in secs),
                 " / ".join(str(e) for e in ev),
                 rs[0]["truncated"]))
    hdr = ["case", "rep", "gens", "population", "evaluations", "budget_secs",
           "elapsed_s", "truncated", "rc", "cpu", "nproc", "kernel", "load1", "note"]
    lines = ["\t".join(hdr)]
    for r in rows:
        lines.append("\t".join(str(x) for x in (
            r["case"], r["rep"], r["gens"], r["population"], r["evals"],
            r["secs"] if r["case"] == "x" else
            (25 if "budget=25" in r["case"] else ""),
            "%.3f" % r["secs"], r["truncated"], r["rc"], r["cpu"], r["nproc"],
            r["kernel"], r["load1"], r["note"])))
    with open(RUNS_TSV, "w", encoding="utf-8") as fh:
        fh.write("# B2：我方实现（examples/ai_explain, release）在本机的墙钟实测\n")
        fh.write("# 生成：.reference/venv/bin/python tools/ai_budget_probe.py --runs\n")
        fh.write("# 口径：只有本机有效；load1 是跑之前的 1 分钟负载（本机是多 agent 共享的）\n")
        fh.write("\n".join(lines) + "\n")
    print("\n写入 %s（%d 行）" % (os.path.relpath(RUNS_TSV, ROOT), len(rows)))
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--static", action="store_true")
    ap.add_argument("--emu", action="store_true")
    ap.add_argument("--runs", action="store_true")
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--budget", type=float, nargs="*", default=[25.0])
    ap.add_argument("--so", default=SO)
    args = ap.parse_args()
    if args.so != SO:
        globals()["SO"] = args.so
    rc = 0
    if args.static:
        rc |= do_static()
    if args.emu:
        rc |= do_emu()
    if args.runs:
        rc |= do_runs(args.reps, args.budget)
    if not (args.static or args.emu or args.runs):
        ap.error("至少给一个：--static / --emu / --runs")
    return rc


if __name__ == "__main__":
    sys.exit(main())
