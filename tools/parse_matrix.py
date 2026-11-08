#!/usr/bin/env python3
"""解析器 / 求值器的**差分矩阵**：生成 → 并行 unicorn 对拍 → 比对 → 入库。

## 它比什么

真二进制（`libgraphwarrust.so`，sha256 `9a538115…`）两个函数，由 unicorn 逐指令模拟：

| 地址 | 是什么 |
|---|---|
| `0x4ed670` | `MathFunction::new` —— **解析器**：文本 → `Vec<Token>`（16 B/token） |
| `0x4eb9d0` | `evaluate_recursive` —— **求值器**：`(tokens, &mut i, x, y, dy)` → `xmm0` |

我方那一半是 `src/math_function.rs::parse` / `eval`，由 `examples/parse_matrix.rs` 导出。
两边吃**同一条原文**、用**同一组 `(x,y,dy)`**，逐项比：

* token **tag 序列**逐个比；
* `CONST`（tag 17）的 `f64` **位模式**逐个比（算子 token 的 `value` 不是契约：真二进制从不
  初始化它，读出来是堆残留 —— 所以夹具里一律归一成 `0`，与 `fixtures/parse.tsv` 同口径）；
* 求值结果的 `f64` **位模式**逐个比（`NaN` 与 `NaN` 位模式不同也算差异，另记为 `nan_bits`）。

## 跑法

```bash
export EMU=<含 analysis/emu.py 的分析仓库>        # 默认 .reference/analysis-repo
$PY tools/parse_matrix.py --gen                    # 写 fixtures/parse_matrix.jsonl
$PY tools/parse_matrix.py --run --jobs 16          # 并行跑真二进制（写工作目录分片）
$PY tools/parse_matrix.py --ours                   # cargo 跑 examples/parse_matrix.rs
$PY tools/parse_matrix.py --compare                # 比对 + 写 fixtures/parse_matrix_truth.tsv
$PY tools/parse_matrix.py --all --jobs 16          # 上面四步
$PY tools/parse_matrix.py --selftest               # **正对照**：故意注入已知错误，验证能抓到
$PY tools/parse_matrix.py --speed --cases 240      # 串行 vs 并行加速比
$PY tools/parse_matrix.py --real-all --jobs 16     # 全部真语料原文（默认不进 fixtures）
```

## 并行

unicorn 是**单核**活，每条式子独立 ⇒ 按条分片、**每进程一个 `Uc` 实例**
（`Gw()` 在子进程里建，父进程不建）。`--jobs N` 就是进程数。

## 产物（文本，入库）

* `fixtures/parse_matrix.jsonl` —— 矩阵本体（`{id, cat, text}`，一行一条；`cat` 是形状分类）；
* `fixtures/parse_matrix_truth.tsv` —— **真二进制**的答案（`id / cat / text / tokens / eval 位串`），
  供以后回归：只要重跑 `--ours` 再 `--compare` 就能知道有没有退。
"""

import argparse
import collections
import glob
import gzip
import json
import os
import random
import struct
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)                       # 仓库根
MATRIX_PATH = os.path.join(ROOT, "fixtures", "parse_matrix.jsonl")
TRUTH_PATH = os.path.join(ROOT, "fixtures", "parse_matrix_truth.tsv")
WORK = os.environ.get("GW2_MX_WORK", os.path.join(ROOT, ".agent-work", "parse-matrix"))

SEED = 52

# 求值用的 (x, y, dy)。前 6 条 y = dy = 0（游戏里的真实取值），最后 1 条把 y / y' 也喂进去。
# ⚠️ 这一串必须与 examples/parse_matrix.rs 的默认表一致；实际跑时由 --xy 显式传给 Rust 侧。
XY = [
    (0.0, 0.0, 0.0),
    (1.0, 0.0, 0.0),
    (-2.5, 0.0, 0.0),
    (3.141592653589793, 0.0, 0.0),
    (19.0, 0.0, 0.0),
    (1e-8, 0.0, 0.0),
    (2.25, 1.25, -0.5),
]
XY_SPEC = ";".join("%r,%r,%r" % t for t in XY)


# ---------------------------------------------------------------------------
# 环境
# ---------------------------------------------------------------------------
def find_up(*rel):
    """从仓库根往上找含 `rel` 的目录（子 agent 的工作树嵌在 `.agent-work/` 里）。"""
    d = ROOT
    for _ in range(8):
        if all(os.path.exists(os.path.join(d, r)) for r in rel):
            return d
        d = os.path.dirname(d)
    return None


def setup_env():
    up = find_up(".reference", ".reference/analysis-repo/analysis/emu.py")
    if up:
        os.environ.setdefault("EMU", os.path.join(up, ".reference", "analysis-repo"))
        os.environ.setdefault(
            "SO", os.path.join(up, ".reference", "steam", "2026-08-28", "libgraphwarrust.so"))
    sys.path.insert(0, HERE)


def replay_dir():
    d = os.environ.get("GW2_REPLAY_DIR")
    if d:
        return d
    up = find_up(".reference/replays/gw2-replays-snapshot/replays")
    if up:
        return os.path.join(up, ".reference", "replays", "gw2-replays-snapshot", "replays")
    home = os.environ.get("HOME", "")
    return os.path.join(home, ".local/share/godot/app_userdata/Graphwar/replays")


def bits(v):
    return struct.unpack("<Q", struct.pack("<d", float(v)))[0]


def is_nan_bits(h):
    u = int(h, 16)
    return (u >> 52) & 0x7FF == 0x7FF and (u & ((1 << 52) - 1)) != 0


# ---------------------------------------------------------------------------
# 矩阵生成
# ---------------------------------------------------------------------------
ATOMS = ["x", "y", "y'", "0", "1", "2", "3", "10", "0.5", ".5", "5.", "1e3", "2.5", "e", "n", "pi"]
BINOPS = ["+", "-", "*", "/", "^"]
FUNCS = ["sin", "cos", "tan", "log", "ln", "sqrt", "exp", "abs"]


class Bag:
    def __init__(self):
        self.cases = []
        self.seen = set()
        self.stat = collections.Counter()

    def add(self, cat, text):
        if not isinstance(text, str):
            return
        # 运输口径：一条 = 一行（`\n` / `\r` 不能进矩阵；tab 可以）
        if "\n" in text or "\r" in text:
            return
        if text in self.seen:
            return
        self.seen.add(text)
        self.cases.append({"id": len(self.cases), "cat": cat, "text": text})
        self.stat[cat] += 1


def gen_enum_binary(b):
    """二元运算符 × 左右操作数的系统枚举 —— 覆盖面最广的一层。"""
    left = ["x", "y", "2", ".5", "e", "n", "sin(x)", "(x+1)", "-x", "x+y", "y'", "10"]
    right = ["x", "2", "(x+1)", "-2", "y'", "sqrt(x)", "e", "0.5"]
    for op in BINOPS:
        for a in left:
            for c in right:
                b.add("enum_binary", a + op + c)


def gen_enum_unary(b):
    """一元负号 / 函数前缀 × 一堆操作数。"""
    pref = ["-", "--", "- ", "sin", "sin ", "sin(x)*", "sqrt", "log", "ln", "exp", "abs",
            "sin^2", "sin(x)^", "-sin", "-sqrt", "log(", "sin(", "-(", "abs(-"]
    tail = ["x", "2", "(x)", "-x", "x^2", "(x+1)^2", "sin(x)", "y'", ".5"]
    for p in pref:
        for t in tail:
            b.add("enum_unary", p + t)


def gen_pow(b):
    """`^` 左结合 / 一元负号与 `^` 的优先 —— 手写全组合 + 空格变体。"""
    fixed = [
        "x^2^3", "2^3^2", "3^2^2", "x^y^2", "2^3^2^2", "x^2^3^2",
        "x^-2", "-x^-2", "x^2^-1", "2^-3^2", "x^y'^2", "(x)^2^2", "x^(2)^3",
        "x^2*3", "x^2/3", "2^x^2", "x^(2^3)", "(x^2)^3", "x ^ 2 ^ 3",
        "-2^2", "(-2)^2", "-x^2", "--x^2", "x^--2", "x^+2", "x^-+2",
        "-x^2^2", "-(x^2)^2", "2^-2^-2", "x^2^x", "sin(x)^2^2", "sqrt(x)^2",
        "x^0.5", "x^.5", "x^5.", "2^1e3", "e^x^2", "n^2^2", "-e^-x^2",
        "x*x^2", "(x+1)^2(x-1)^2", "x^(y')", "y'^2", "y'^x",
    ]
    for t in fixed:
        b.add("pow_assoc", t)
        b.add("pow_assoc", t.replace("^", " ^ "))
    for op in ["^"]:
        for a in ["x", "2", "-x", "(x+1)", "sin(x)", "y'"]:
            for c in ["2", "x", "-2", "(x)", "y'"]:
                b.add("pow_assoc", a + op + c + op + "2")


def gen_implicit(b):
    """隐式乘法：相邻"可结束/可开始"单元之间的自动 `*`。"""
    fixed = ["2x", "2 x", "2sin(x)", "xy", "x y", "(x+1)(x-1)", "2(x)(3)", "x sin(x)",
             "sin(x)cos(x)", "2e", "ex", "nx", "pie", "2pi", "x y'", "y'x", "yy", "2 3",
             "3 4 5", "x(x)", "(x)x", "2(3)", "(2)(3)(4)", "sin(x)sin(x)", "2e3", "2n",
             "2y'", "x2", "2x2", "x2x", "2(3)x", "x(2)3", ")(2)(", "2)3", "x)2", "x)(2)"]
    for t in fixed:
        b.add("implicit", t)
    unit = ["2", "x", "(x+1)", "sin(x)", "y'", "sqrt(x)", "e", "n", ".5", "(x)(x)"]
    for a in unit:
        for c in unit:
            b.add("implicit", a + c)


def gen_func_chain(b):
    """log/ln/sqrt/exp/abs（以及三角函数）的**连写**：`logln(x)`、`sqrtabs(x)` …"""
    for f in FUNCS:
        for g in FUNCS:
            b.add("func_chain", f + g + "(x)")
            b.add("func_chain", f + g + "x")
    for f in FUNCS:
        for g in FUNCS:
            for h in ["ln", "log", "sqrt"]:
                b.add("func_chain", f + g + h + "(x)")
    extra = ["log(sqrt(abs(x)))", "sqrt(abs(ln(x)))", "exp(ln(x))", "ln(exp(x))",
             "abs(abs(abs(x)))", "log(ln(sqrt(exp(abs(x)))))", "sin(cos(tan(x)))",
             "sqrt sqrt x", "log ln x", "exp abs x", "abs exp x", "sqrtabsx",
             "loglnx", "expabslnx", "sinsincos(x)", "sqrt(x)abs(x)", "log(x)ln(x)",
             "sin(sin(sin(sin(x))))"]
    for t in extra:
        b.add("func_chain", t)


def gen_number(b):
    """数字形态：`.5` / `5.` / `1e3` / `1E-3` / 长数字 / 未匹配的残渣。"""
    nums = ["0", "1", "10", ".5", "5.", "1.5", "1e3", "1E-3", "1e-3", "1.2.3", "1..2",
            "00", "007", "0.000", ".", "1.", "1e", "e1", "1E", "1e+3", "1e400", "1e-400",
            "0x10", "1_000", "1234567890123456789012345", "12345678901234567890123456789012345678901234567890",
            "9" * 40, "1" * 400, "6.02e23", "1e3e3", "3.14.15", "０１", "²", "³", "①",
            "0.1", "1.7976931348623157e308", "5e-324", "-0", "0.0"]
    for t in nums:
        for ctx in ["{}", "{}x", "x*{}", "{} +1", "sin({})", "{}{}", "({})", "x^{}", "-{}",
                    "{}^2", "2^{}", "{})", "({}", "{}e", "e{}"]:
            b.add("number", ctx.format(t) if ctx.count("{}") == 1 else ctx.format(t, t))


def gen_spaces(b):
    """空格 / tab：输入框会删空格，但**解析器本身**拿到的就是原样文本。"""
    fixed = [" x ", "x + 1", "x  +  1", "\tx+1", "x\t+\t1", "s in(x)", "sin (x)", "sq rt(x)",
             "x y", " x", "x ", "  ", "\t", "x \t y", "( x )", "sin\t(x)", "2 x 3",
             "x ^ 2", "x^ 2", "x ^2", " - x", "- x", "x - - 1", " x ^ 2 ^ 3 ",
             "l o g ( x )", "p i", "pi ", " pi", "p i x"]
    for t in fixed:
        b.add("spaces", t)


def gen_empty(b):
    for t in ["", " ", "\t", "()", ")", "(", "  ", "\t\t", "(()", "())", "()()"]:
        b.add("empty", t)


def gen_long(b):
    """超长串（>1 KB）。⚠️ 深度控制在 ~300 以内：两边都是递归下降，太深会炸栈。"""
    b.add("long", "+".join(["1"] * 600))                      # 1.2 KB 平铺
    b.add("long", "+".join(["x"] * 400))
    b.add("long", "1" * 1200)                                  # 一个字面量
    b.add("long", "x" * 1100)                                  # 1100 个隐式乘法
    b.add("long", "(" * 300 + "x" + ")" * 300)                 # 300 层深
    b.add("long", "(" * 300 + "x")                             # 300 个未闭合
    b.add("long", "x" + ")" * 300)                             # 300 个无主 )
    b.add("long", "sin(" * 200 + "x" + ")" * 200)
    b.add("long", "2*" * 600 + "x")
    b.add("long", "-" * 600 + "x")
    b.add("long", "^".join(["2"] * 300))
    b.add("long", "#" * 1200)                                  # 全非法字符
    b.add("long", "x+" + "+".join(["sin(cos(tan(x)))"] * 60))
    b.add("long", "x" * 600 + ")".join([""]) + ")" * 600)
    b.add("long", "y'" * 600)


def gen_illegal(b):
    """非法字符 / 非 ASCII：词法器一律跳过；但**大小写折叠**这一步的边界就在这。"""
    chars = ["#", "@", "%", "&", "!", "?", "_", "~", "`", "[", "]", "{", "}",
             "<", ">", ";", ":", '"', "\\", "|", "=", "$", ",", "'", "’"]
    for c in chars:
        b.add("illegal", c)
        b.add("illegal", "x" + c)
        b.add("illegal", c + "x")
        b.add("illegal", "x" + c + "y")
        b.add("illegal", "si" + c + "n(x)")
    for t in ["√x", "π", "Π", "πx", "PI", "Πx", "e^π", "∞", "α", "×", "÷", "²", "x²", "x³",
              "Ω", "–", "—", "x😀", "×x", "÷2", "x÷2", "πΠ", "３", "６x", "x·2", "°",
              "\u00a0x", "x\u00a0", "Å", "Kx", "İx", "ǅ", "Σx", "σx", "ς"]:
        b.add("illegal", t)


def gen_y(b):
    """`y` 与 `y'` 的切词：正则里 `y'` 排在 `y` 之前。"""
    for t in ["y", "y'", "yy", "y'y", "y''", "y'''", "y y'", "sin(y')", "y'^2", "Y", "Y'",
              "y’", "y`", 'y"', "y '", "y' y'", "y'y'", "'y", "q y", "yq", "y2", "2y'",
              "yy'", "y'x", "xy'", "y'*2", "y'/2"]:
        b.add("y_token", t)


def gen_random(b, rng):
    """随机语法表达式（合法）+ 随机 token 汤（不合法）—— 大头在这里。"""
    atoms = ATOMS

    def expr(depth):
        r = rng.random()
        if depth <= 0 or r < 0.34:
            return rng.choice(atoms)
        if r < 0.52:
            return expr(depth - 1) + rng.choice(BINOPS) + expr(depth - 1)
        if r < 0.66:
            return "(" + expr(depth - 1) + ")"
        if r < 0.79:
            return rng.choice(FUNCS) + expr(depth - 1)
        if r < 0.90:
            return "-" + expr(depth - 1)
        return expr(depth - 1) + expr(depth - 1)

    for _ in range(2600):
        b.add("random_expr", expr(rng.randint(1, 5)))

    toks = ["(", ")", "x", "y", "y'", "2", ".5", "^", "+", "-", "*", "/", "e", "n",
            "sin", "log", "sqrt", " ", "1e3"]
    for _ in range(1400):
        n = rng.randint(2, 11)
        b.add("token_soup", "".join(rng.choice(toks) for _ in range(n)))


def gen_mutate(b, rng, pool, n):
    """在真语料原文上做单字符/单 token 的随机破坏 —— 最容易撞出边角的形状。"""
    alphabet = "()xyz0123456789+-*/^'. en"
    for _ in range(n):
        s = rng.choice(pool)
        if not s:
            continue
        for _ in range(rng.randint(1, 3)):
            if not s:
                break
            k = rng.randrange(len(s))
            op = rng.random()
            if op < 0.34:
                s = s[:k] + rng.choice(alphabet) + s[k:]
            elif op < 0.67:
                s = s[:k] + s[k + 1:]
            else:
                s = s[:k] + rng.choice(alphabet) + s[k + 1:]
        b.add("real_mutate", s)


def load_real_texts():
    """真回放快照里出现过的 `function` / `corrected_function` 原文（去重）。"""
    d = replay_dir()
    files = sorted(glob.glob(os.path.join(d, "*")))
    seen = set()
    for f in files:
        if not os.path.isfile(f):
            continue
        try:
            raw = gzip.open(f, "rb").read()
        except Exception:
            try:
                raw = open(f, "rb").read()
            except Exception:
                continue
        try:
            txt = raw.decode("utf-8")
        except UnicodeDecodeError:
            continue
        for line in txt.splitlines():
            if '"FunctionInfo"' not in line:
                continue
            try:
                ev = json.loads(line)["event"]["FunctionInfo"]
            except Exception:
                continue
            for k in ("function", "corrected_function"):
                t = ev.get(k)
                if isinstance(t, str) and t:
                    seen.add(t)
    return sorted(seen)


def gen_matrix(real_sample=1800):
    rng = random.Random(SEED)
    b = Bag()
    gen_enum_binary(b)
    gen_enum_unary(b)
    gen_pow(b)
    gen_implicit(b)
    gen_func_chain(b)
    gen_number(b)
    gen_spaces(b)
    gen_empty(b)
    gen_long(b)
    gen_illegal(b)
    gen_y(b)
    gen_random(b, rng)

    real = load_real_texts()
    if real:
        # 分层抽样：不平衡括号（上一轮修的那类"风险形状"）全给足，再均匀抽平衡的
        risky = [t for t in real if t.count("(") != t.count(")")]
        plain = [t for t in real if t.count("(") == t.count(")")]
        rng.shuffle(risky)
        rng.shuffle(plain)
        n_risk = min(len(risky), real_sample // 2)
        n_plain = min(len(plain), real_sample - n_risk)
        for t in risky[:n_risk]:
            b.add("real_unbalanced", t)
        for t in plain[:n_plain]:
            b.add("real_balanced", t)
        gen_mutate(b, rng, plain[:400] + risky[:200], 700)
    return b


# ---------------------------------------------------------------------------
# 我方一侧（cargo）
# ---------------------------------------------------------------------------
def run_ours(matrix, out_path):
    texts = [c["text"] for c in matrix]
    cmd = ["cargo", "run", "--release", "--offline", "-q", "--example", "parse_matrix",
           "--", "--xy", XY_SPEC]
    t0 = time.time()
    p = subprocess.run(cmd, cwd=ROOT, input="\n".join(texts) + "\n",
                       capture_output=True, text=True, encoding="utf-8")
    if p.returncode != 0:
        raise SystemExit("ours 侧失败（rc=%d）：\n%s" % (p.returncode, p.stderr[-4000:]))
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(p.stdout)
    rows = [json.loads(l) for l in p.stdout.splitlines() if l.strip()]
    if len(rows) != len(texts):
        raise SystemExit("ours 侧条数不符：%d 行 / %d 条" % (len(rows), len(texts)))
    for k, r in enumerate(rows):
        if r["i"] != k:
            raise SystemExit("ours 侧序号错位：第 %d 行 i=%r" % (k, r["i"]))
    return rows, time.time() - t0


# ---------------------------------------------------------------------------
# 真二进制一侧（unicorn，多进程）
# ---------------------------------------------------------------------------
def _real_worker(task):
    k, items, out_path = task
    import eval_oracle as EO

    Gw = EO.load_oracle()
    done = []
    with open(out_path, "w", encoding="utf-8") as f:
        for idx, text in items:
            EO.reset_heap(Gw)
            rec = {"i": idx}
            try:
                toks = EO.call_parse(Gw, text)
            except Exception as exc:                      # panic 分支、堆耗尽、……
                rec["err"] = "parse:" + repr(exc)[:200]
                f.write(json.dumps(rec) + "\n")
                f.flush()
                done.append(idx)
                continue
            # 算子 token 的 value 不是契约（真二进制不初始化），只留 CONST 的值
            rec["tok"] = [[int(t), ("%016x" % bits(v)) if int(t) == 17 else "0000000000000000"]
                          for (t, v) in toks]
            evs = []
            for (x, y, dy) in XY:
                try:
                    evs.append("%016x" % bits(EO.call_eval(Gw, toks, x, y, dy)))
                except Exception as exc:
                    evs.append("ERR:" + repr(exc)[:60])
            rec["ev"] = evs
            f.write(json.dumps(rec) + "\n")
            f.flush()
            done.append(idx)
    return k, len(done)


def run_real(matrix, jobs, tag="real"):
    os.makedirs(WORK, exist_ok=True)
    todo = [(c["id"], c["text"]) for c in matrix]
    got = {}
    t0 = time.time()
    for round_no in (1, 2):
        if not todo:
            break
        jobs_n = max(1, min(jobs, len(todo)))
        shards = [todo[i::jobs_n] for i in range(jobs_n)]
        tasks = [(k, shard, os.path.join(WORK, "%s.r%d.%d.jsonl" % (tag, round_no, k)))
                 for k, shard in enumerate(shards) if shard]
        print("  第 %d 轮：%d 条 / %d 进程" % (round_no, len(todo), len(tasks)), flush=True)
        import multiprocessing as mp
        ctx = mp.get_context("fork")
        with ctx.Pool(len(tasks)) as pool:
            for k, n in pool.imap_unordered(_real_worker, tasks):
                pass
        for _, _, path in tasks:
            if not os.path.exists(path):
                continue
            for line in open(path, encoding="utf-8"):
                if line.strip():
                    r = json.loads(line)
                    got[r["i"]] = r
        todo = [(i, t) for (i, t) in todo if i not in got]
        if todo:
            print("  ⚠️ %d 条没跑出来（worker 中途死掉？），重试一次" % len(todo), flush=True)
    dt = time.time() - t0
    missing = [i for (i, _) in [(c["id"], c["text"]) for c in matrix] if i not in got]
    if missing:
        print("  ⚠️ 仍然缺 %d 条：%s" % (len(missing), missing[:10]))
    return got, dt


# ---------------------------------------------------------------------------
# 比对
# ---------------------------------------------------------------------------
def compare(matrix, real, ours, verbose=True, limit=None):
    stat = collections.Counter()
    percat = collections.defaultdict(collections.Counter)
    bad = []
    for c in (matrix if limit is None else matrix[:limit]):
        i = c["id"]
        cat = c["cat"]
        r = real.get(i)
        o = ours[i]
        percat[cat]["n"] += 1          # 条数（不是"槽位"数：nan_payload 按槽位计，别加进来）
        if r is None:
            stat["real_missing"] += 1
            percat[cat]["real_missing"] += 1
            continue
        if "err" in r:
            stat["real_err"] += 1
            percat[cat]["real_err"] += 1
            bad.append((i, cat, c["text"], "real_err", r["err"], ""))
            continue
        rt = r["tok"]
        ot = o["tok"]
        if [t for t, _ in rt] != [t for t, _ in ot]:
            stat["tokens"] += 1
            percat[cat]["tokens"] += 1
            bad.append((i, cat, c["text"], "tokens",
                        ";".join("%d:%s" % (t, v) for t, v in rt)[:400],
                        ";".join("%d:%s" % (t, v) for t, v in ot)[:400]))
            continue
        if any(a[1] != b_[1] for a, b_ in zip(rt, ot) if a[0] == 17):
            stat["const"] += 1
            percat[cat]["const"] += 1
            bad.append((i, cat, c["text"], "const",
                        ";".join("%d:%s" % (t, v) for t, v in rt)[:400],
                        ";".join("%d:%s" % (t, v) for t, v in ot)[:400]))
            continue
        ev_bad = None
        for k, (a, b_) in enumerate(zip(r["ev"], o["ev"])):
            if a == b_:
                continue
            if a.startswith("ERR"):
                ev_bad = ("eval_err", a, b_)
                break
            if is_nan_bits(a) and is_nan_bits(b_):
                stat["nan_payload"] += 1
                percat[cat]["nan_payload"] += 1
                continue
            ev_bad = ("eval", a, b_)
            break
        if ev_bad:
            stat[ev_bad[0]] += 1
            percat[cat][ev_bad[0]] += 1
            bad.append((i, cat, c["text"], ev_bad[0], ev_bad[1], ev_bad[2]))
            continue
        stat["ok"] += 1
        percat[cat]["ok"] += 1
    if verbose:
        print("\n总账：%s" % dict(stat))
        print("%-16s %6s %6s %6s %6s %6s %6s %6s" % ("形状", "条数", "ok", "tokens", "const", "eval", "err", "nan位"))
        for cat in sorted(percat):
            s = percat[cat]
            print("%-16s %6d %6d %6d %6d %6d %6d %6d" % (
                cat, s["n"], s["ok"], s["tokens"], s["const"], s["eval"],
                s["real_err"] + s.get("eval_err", 0) + s["real_missing"], s["nan_payload"]))
        for (i, cat, text, kind, a, b_) in bad[:40]:
            print("  ! %-6s #%-5d %-10s %r\n      真: %s\n      我: %s" % (kind, i, cat, text[:80], a, b_))
    return stat, percat, bad


TAG_CH = "0123456789abcdefghij"          # tag 0..19 → 一个字符（CONST=17 → 'h'）


def enc_tokens(tok):
    """紧凑 token 串：每个 token 一个 tag 字符；CONST（tag 17 = 'h'）后面直接跟 16 位十六进制位串。

    解码：读到 'h' 就再吃 16 个字符当 f64 位模式；其余字符就是一个算子 tag。
    （3.4 MB 的 `tag:16hex` 写法压到 ~1/3，参考 `fixtures/callgraph_fingerprint.tsv` 的量级。）
    """
    out = []
    for t, bits_s in tok:
        out.append(TAG_CH[t])
        if t == 17:
            out.append(bits_s)
    return "".join(out)


def dec_tokens(s):
    """enc_tokens 的逆：返回 [(tag, bits_str)]。"""
    out = []
    i = 0
    while i < len(s):
        tag = TAG_CH.index(s[i])
        i += 1
        if tag == 17:
            out.append((tag, s[i:i + 16]))
            i += 16
        else:
            out.append((tag, "0000000000000000"))
    return out


def write_truth(matrix, real):
    with open(TRUTH_PATH, "w", encoding="utf-8") as f:
        f.write("# 真二进制 0x4ed670（解析）/ 0x4eb9d0（求值）的答案；生成器 tools/parse_matrix.py\n")
        f.write("# 列：id \t cat \t tokens \t eval_bits（7 组 (x,y,dy)，顺序见 tools/parse_matrix.py 的 XY）\n")
        f.write("# tokens 编码：每个 token = 1 个 tag 字符（0-9 / a-j，见 TAG_CH）；tag 'h'(=17 CONST)\n")
        f.write("#   后面紧跟 16 位十六进制 = 该常量的 f64 位模式。原文见 fixtures/parse_matrix.jsonl 的 id。\n")
        for c in matrix:
            r = real.get(c["id"])
            if r is None or "err" in r:
                f.write("%d\t%s\tERR\t%s\n" % (c["id"], c["cat"],
                                                (r or {}).get("err", "missing").replace("\t", " ")))
                continue
            f.write("%d\t%s\t%s\t%s\n" % (c["id"], c["cat"], enc_tokens(r["tok"]),
                                            ",".join(r["ev"])))
    print("写入 %s（%d 条）" % (TRUTH_PATH, len(matrix)))


def load_truth():
    """读回真值夹具 → {id: {"cat", "tok": [(tag,bits)], "ev": [16hex]}}。"""
    out = {}
    for line in open(TRUTH_PATH, encoding="utf-8"):
        if line.startswith("#") or not line.strip():
            continue
        parts = line.rstrip("\n").split("\t")
        if len(parts) < 3:
            continue
        i, cat = int(parts[0]), parts[1]
        if parts[2] == "ERR":
            out[i] = {"cat": cat, "err": parts[3] if len(parts) > 3 else "err"}
            continue
        out[i] = {"cat": cat, "tok": dec_tokens(parts[2]),
                  "ev": parts[3].split(",") if len(parts) > 3 else []}
    return out


def check_truth(matrix, ours):
    """**不用 unicorn 的回归闸**：拿入库的真值夹具逐条比我方一侧。

    改了 `src/math_function.rs` 之后跑这一条就够了（矩阵 + 真值都在仓库里，纯文本）：
    `$PY tools/parse_matrix.py --ours && $PY tools/parse_matrix.py --check-truth`
    """
    truth = load_truth()
    stat = collections.Counter()
    bad = []
    for c in matrix:
        i = c["id"]
        t = truth.get(i)
        o = ours[i]
        if t is None:
            stat["no_truth"] += 1
            continue
        if "err" in t:
            stat["truth_err"] += 1
            continue
        if [a for a, _ in t["tok"]] != [a for a, _ in o["tok"]]:
            stat["tokens"] += 1
            bad.append((i, c["cat"], c["text"], "tokens",
                        ";".join("%d:%s" % (a, b) for a, b in t["tok"])[:300],
                        ";".join("%d:%s" % (a, b) for a, b in o["tok"])[:300]))
            continue
        if any(a[1] != b[1] for a, b in zip(t["tok"], o["tok"]) if a[0] == 17):
            stat["const"] += 1
            bad.append((i, c["cat"], c["text"], "const", "", ""))
            continue
        diff_ev = [k for k, (a, b) in enumerate(zip(t["ev"], o["ev"])) if a != b
                   and not (is_nan_bits(a) and is_nan_bits(b))]
        if diff_ev:
            stat["eval"] += 1
            bad.append((i, c["cat"], c["text"], "eval", str(diff_ev), ""))
            continue
        stat["ok"] += 1
    print("回归闸（读 %s）：%s" % (os.path.basename(TRUTH_PATH), dict(stat)))
    for row in bad[:20]:
        print("  ! %s" % (row,))
    return len(bad) == 0


def esc(s):
    return s.replace("\\", "\\\\").replace("\t", "\\t")


# ---------------------------------------------------------------------------
# 正对照：故意把"我方"那一侧改坏几条，验证比对器真的抓得到
# ---------------------------------------------------------------------------
def selftest(matrix, real, ours):
    bad_kinds = {}
    ours2 = {i: json.loads(json.dumps(o)) for i, o in ours.items()}
    idx_ok = [c["id"] for c in matrix if "err" not in real.get(c["id"], {"err": 1})]
    pick = [i for i in idx_ok if len(ours2[i]["tok"]) >= 2][:5]
    if len(pick) < 4:
        print("正对照：可用样本不足（%d）" % len(pick))
        return False
    # ① tag 改坏
    i = pick[0]
    ours2[i]["tok"][0][0] = (ours2[i]["tok"][0][0] + 1) % 20
    bad_kinds[i] = "tokens"
    # ② CONST 值改一位
    i = pick[1]
    for t in ours2[i]["tok"]:
        if t[0] == 17:
            t[1] = "3ff0000000000000" if t[1] != "3ff0000000000000" else "4000000000000000"
            bad_kinds[i] = "const"
            break
    # ③ 少一个 token
    i = pick[2]
    ours2[i]["tok"].pop()
    bad_kinds[i] = "tokens"
    # ④ 求值结果改一位
    i = pick[3]
    ours2[i]["ev"][0] = "3ff0000000000000" if ours2[i]["ev"][0] != "3ff0000000000000" else "4000000000000000"
    bad_kinds[i] = "eval"
    stat, percat, bad = compare(matrix, real, ours2, verbose=False)
    caught = {b[0]: b[3] for b in bad}
    ok = True
    for i, want in bad_kinds.items():
        got = caught.get(i, "未抓到")
        if got != want:
            print("  ✗ 正对照 #%d 期望 %s，实际 %s" % (i, want, got))
            ok = False
        else:
            print("  ✓ 正对照 #%d 抓到 %s" % (i, got))
    extra = len(bad) - len(bad_kinds)
    print("  正对照结论：%s（注入 %d 条，报出 %d 条，多报 %d 条）"
          % ("通过" if ok and extra == 0 else "失败", len(bad_kinds), len(bad), extra))
    return ok and extra == 0


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------
def load_matrix(path=MATRIX_PATH):
    return [json.loads(l) for l in open(path, encoding="utf-8") if l.strip()]


def load_ours(path):
    rows = [json.loads(l) for l in open(path, encoding="utf-8") if l.strip()]
    out = {}
    for r in rows:
        out[r["i"]] = r
    return out


def main():
    ap = argparse.ArgumentParser(description="math_function 解析器/求值器差分矩阵")
    ap.add_argument("--gen", action="store_true", help="重新生成 fixtures/parse_matrix.jsonl")
    ap.add_argument("--run", action="store_true", help="并行跑真二进制（unicorn）")
    ap.add_argument("--ours", action="store_true", help="跑我方一侧（cargo example）")
    ap.add_argument("--compare", action="store_true", help="比对并写真值夹具")
    ap.add_argument("--check-truth", action="store_true",
                    help="拿入库的真值夹具比对我方一侧（**不需要 unicorn**，回归用）")
    ap.add_argument("--all", action="store_true", help="gen + run + ours + compare")
    ap.add_argument("--selftest", action="store_true", help="正对照：故意弄坏几条，验证比对器")
    ap.add_argument("--speed", action="store_true", help="串行 vs 并行加速比")
    ap.add_argument("--real-all", action="store_true", help="跑**全部**真语料原文（不进夹具）")
    ap.add_argument("--jobs", type=int, default=16)
    ap.add_argument("--cases", type=int, default=240, help="--speed 的子集大小")
    ap.add_argument("--limit", type=int, default=0, help="只跑前 N 条（调试用）")
    args = ap.parse_args()
    setup_env()
    os.makedirs(WORK, exist_ok=True)
    real_path = os.path.join(WORK, "real.json")
    ours_path = os.path.join(WORK, "ours.jsonl")

    if args.gen or args.all:
        t0 = time.time()
        b = gen_matrix()
        with open(MATRIX_PATH, "w", encoding="utf-8") as f:
            for c in b.cases:
                f.write(json.dumps(c, ensure_ascii=False) + "\n")
        print("矩阵 %d 条 → %s（%.1fs）" % (len(b.cases), MATRIX_PATH, time.time() - t0))
        for cat, n in sorted(b.stat.items()):
            print("  %-16s %5d" % (cat, n))

    if args.real_all:
        texts = load_real_texts()
        matrix = [{"id": i, "cat": "real_all", "text": t} for i, t in enumerate(texts)]
        print("全量真语料 %d 条（去重）" % len(matrix))
        real, dt = run_real(matrix, args.jobs, tag="realall")
        print("真二进制跑完：%d 条 / %.1fs（%.0f 条/s 总吞吐）" % (len(real), dt, len(real) / dt))
        ours, _ = run_ours(matrix, os.path.join(WORK, "ours_realall.jsonl"))
        stat, percat, bad = compare(matrix, real, ours)
        json.dump({"stat": dict(stat), "seconds": dt, "n": len(matrix)},
                  open(os.path.join(WORK, "realall.json"), "w"), indent=1)
        if bad:
            with open(os.path.join(WORK, "realall_bad.tsv"), "w", encoding="utf-8") as f:
                for row in bad:
                    f.write("\t".join(str(x) for x in row) + "\n")
            print("不一致明细 → %s" % os.path.join(WORK, "realall_bad.tsv"))
        return

    if args.speed:
        matrix = load_matrix()
        if args.limit:
            matrix = matrix[:args.limit]
        # **分层**取样（按步长抽，覆盖所有形状），不是取前 N 条 ——
        # 前 N 条全是 enum_binary 的短式子，测出来的单条耗时偏低。
        stride = max(1, len(matrix) // max(1, args.cases))
        sub = matrix[::stride][:args.cases]
        print("=== 串行（jobs=1，%d 条）===" % len(sub))
        r1, dt1 = run_real(sub, 1, tag="speed_serial")
        print("=== 并行（jobs=%d，%d 条）===" % (args.jobs, len(sub)))
        r2, dt2 = run_real(sub, args.jobs, tag="speed_par")
        same = r1.keys() == r2.keys() and all(r1[k]["tok"] == r2[k]["tok"] for k in r1 if "tok" in r1[k])
        print("\n串行 %.1fs / 并行 %.1fs ⇒ 加速比 **%.2f×**（%d 条，结果一致=%s）"
              % (dt1, dt2, dt1 / dt2, len(sub), same))
        return

    if args.all or args.run:
        matrix = load_matrix()
        if args.limit:
            matrix = matrix[:args.limit]
        print("并行跑真二进制：%d 条 / %d 进程" % (len(matrix), args.jobs))
        real, dt = run_real(matrix, args.jobs)
        json.dump({str(k): v for k, v in real.items()}, open(real_path, "w"))
        print("真二进制：%d 条 / **%.1fs**（单进程估计 %.0fs，加速比 %.1f×）"
              % (len(real), dt, len(real) * 0.30, len(real) * 0.30 / dt))

    if args.all or args.ours:
        matrix = load_matrix()
        if args.limit:
            matrix = matrix[:args.limit]
        ours, dt = run_ours(matrix, ours_path)
        print("我方一侧：%d 条 / %.1fs" % (len(ours), dt))

    if args.check_truth:
        matrix = load_matrix()
        if args.limit:
            matrix = matrix[:args.limit]
        ours = load_ours(ours_path)
        ok = check_truth(matrix, ours)
        sys.exit(0 if ok else 1)

    if args.all or args.compare or args.selftest:
        matrix = load_matrix()
        if args.limit:
            matrix = matrix[:args.limit]
        real = {int(k): v for k, v in json.load(open(real_path)).items()} \
            if os.path.exists(real_path) else {}
        ours = load_ours(ours_path)
        if not real or not ours:
            raise SystemExit("缺 %s 或 %s —— 先跑 --run / --ours" % (real_path, ours_path))
        stat, percat, bad = compare(matrix, real, ours)
        write_truth(matrix, real)
        summary = {"n": len(matrix), "stat": dict(stat),
                   "percat": {k: dict(v) for k, v in percat.items()}}
        json.dump(summary, open(os.path.join(WORK, "summary.json"), "w"), indent=1)
        if bad:
            with open(os.path.join(WORK, "bad.tsv"), "w", encoding="utf-8") as f:
                for row in bad:
                    f.write("\t".join(str(x) for x in row) + "\n")
            print("不一致明细 → %s" % os.path.join(WORK, "bad.tsv"))
        if args.selftest:
            selftest(matrix, real, ours)


if __name__ == "__main__":
    main()
