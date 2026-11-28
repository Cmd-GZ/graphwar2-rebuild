#!/usr/bin/env python3
"""按**原版的模块**算复原覆盖率，而不是按我们自己的测试条数。

## 这个脚本回答什么

`cargo test` / `selftest` 的条数是回归保护，不是复原证据（`AGENTS.md` §0.2）。
复原进度唯一有意义的口径是：**原版有多少函数/模块，我们还原了多少**。

本脚本给出两半，各自独立可复核：

1. **原版这一半**：`libgraphwarrust.so` 里每个 `src/<file>.rs` 模块名下有
   多少个函数能**硬归属**。归属靠 `core::panic::Location`：
   Location 是 `{file: &str, line: u32, col: u32}`，它在 `.data.rel.ro` 里的
   `file` 指针是 `R_X86_64_RELATIVE` 重定位，**addend 才是文件名字面量的地址**
   （直接搜裸指针一无所获，这是本项目踩过的坑）。于是：
   重定位 addend → 文件名字面量 → 校验 `len` 字段 → 读出 `line/col`；
   再用 capstone 扫 `.text` 找引用该 Location 的指令，落到 Ghidra 的函数边界上。
2. **我们这一半**：`src/*.rs` 里每个文件的 `fn` 条数，以及模块级证据档。

## 三个必须说清的口径（不这样读会得出错误结论）

* **`orig_funcs` 是可度量下限，不是模块规模。**
  只有带 panic 点的函数才留锚。纯计算模块系统性偏低：求值器 `0x4eb9d0`
  和采样器 `0x3c2530` 的整个函数体里**一个** `Location` 引用都没有，
  所以它们归属不出来。`graphwar_path_calculator.rs` 因此只有 1 个归属函数，
  而它实际装了采样器。**不要拿 `orig_funcs` 当"这个模块有多大"。**
* **一个函数可能引用两个模块的 Location**（内联把别的文件的行号带了进来）。
  这类函数单独计入 `contested`，并按"贡献锚点数最多、并列取字典序最小"的
  规则只算给一个模块，保证各模块相加 == 总归属数，不重复计数。
* **只有 `src/*.rs` 算实现。** `client/` 里那套 GDScript 是照 `.tscn` 和截图
  重写的仿制品，不是原版源码（`AGENTS.md` §0.4），所以它**不**计入任何模块的
  `status`。同样的道理，`api.rs` / `capi.rs` 这类自造装配层也不编到原版模块名下。

## status 五档的判定规则（严格按证据，不许乐观）

| status | 判据 |
|---|---|
| `逐位` | 有 src/ 实现，且**已与真二进制/真抓包逐字节或逐位对拍通过** |
| `观测` | 有 src/ 实现，依据是反汇编/运行时/真机观测，但没有逐位对拍 |
| `推断` | 有 src/ 实现，但依据是推断（写明缺什么证据） |
| `未做` | **没有 src/ 实现**，但原版这块有地址级观测或文档描述 |
| `未知` | **没有实现，且除了模块名没有任何证据**（连"它干什么"都不知道） |

前四档都可以写 `未做`/`未知` 之外的东西，但 `未做`/`未知` 一定意味着
`ours_file` 为空 —— `--check` 会验证这条不变式。

## 模块清单从哪来（2026-11-28 T112 修正：分母 78 → 82）

⚠️ **本表的"模块"这一维以前也是 Location 口径建的，它同样有盲区。**
Location 只能看见"有 panic 点的模块"：Linux `.so` 里 `<模块>.rs` 字面量出现 **0 次**的模块
（本轮实测 10 个）在旧口径下**连一行都不会有**，于是"分母"本身被系统性漏算
（T109 在 `docs/06` §153.1-② 报的正是这个：`graphwar_ban_list` / `graphwar_version` 整模块缺行）。

现在模块清单的**主证据是未 strip 的符号表**（与逆向目标同版本）：

    .reference/steam/android-2.0.60/libgraphwarrust.so   # arm64，file 报 "not stripped"

判据（口径与 `docs/02` §18.3 同一套，预注册见 `fixtures/module_denom_plan.txt`）：

* **J1'**（分母用这一档）：demangle 后符号文本**任意位置**出现 `graphwarrust::<m>::`
  ⇒ 该版源码里模块 `<m>` 存在（Rust 名字解析要求模块存在）。0 个自有函数的模块
  （`time_probe`）也靠这一档才看得见。
* **J1**（更严，只证明"编译出过函数/静态量"）：定义路径以 `graphwarrust::<m>::` 开头。
  两个读数都报，差集点名（目标版差 1 个 = `time_probe`）。
* 模块名 = 路径**第 2 段**；类型名/方法名/闭包/静态项名都在第 3 段及以后 ⇒ 天然不算模块。

`--check` 会拿这份符号表**重算模块集合**，与下表逐名对账：**少一个模块、多一个模块、
`sym_funcs` 对不上，都直接失败**（这是 T109 那种"模块静默缺行"的防回归闸）。
符号表不在本机时该项 **SKIP 并打印警告**（不静默通过）；Linux 二进制 / `.pkl` 缺失仍是硬失败。

`sym_funcs` 列 = 该模块在 Android 符号表里的 **FUNC 条数**（模块规模的可度量下限）。
⚠️ **它不进 `orig_funcs`、不进 527、不进任何总账分子** —— 两种归属方法混在一列会静默
改掉招牌数字的含义；要报就分开报并带轴名（`527` = Location 可归属 / `sym_funcs` 合计 = 符号表 FUNC）。

## 用法

    python3 tools/coverage_matrix.py            # 重新生成三个产物 + 打印总账
    python3 tools/coverage_matrix.py --check    # 只校验，不写文件（不一致则退出码 1）
    python3 tools/coverage_matrix.py --modules  # 只跑模块分母审计（写 fixtures/module_denom_audit.tsv
                                                #   + 打印正/负对照），不碰 docs/ 两个生成物

需要 `analysis/funcstarts_ghidra_va.pkl`（**不是** `funcstarts.pkl`，那张 `.eh_frame`
的表是坏的，见 `docs/02` §12）和仓库根目录的原版 `.so`（都不入库）。
capstone 只在 `.venv` 里；系统 `python3` 缺它时，本脚本会自动用 `../.venv/bin/python`
重新执行自己，所以上面的两条命令原样可用。

产物：
    docs/coverage-matrix.tsv   机器可读（本脚本唯一真源）
    docs/149-coverage-matrix.md  人读（也是本脚本生成的，避免手抄数字过期）
    fixtures/module_denom_audit.tsv  模块分母的三方差集表 + 正/负对照（T112）

模块清单**不**从 Location 建（那是 t109 报的那个结构性盲区的根因），而是从**未 strip** 的
Android 2.0.60 `.symtab` 的符号路径读；`--check` 会重算并与上表逐名对账（见文件头
「模块清单从哪来」与 `fixtures/module_denom_plan.txt`）。
"""

import hashlib
import os
import pickle
import re
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)                 # gw2-rebuild/


def _find_upstream():
    """定位上游分析仓库（放 `libgraphwarrust.so` 的那一层）。

    正常布局里它是 `gw2-rebuild/` 的兄弟目录；子 agent 把仓库克隆到
    `.agent-work/<topic>/repo` 时要往上多找两层，所以这里逐级上溯。
    `COVERAGE_MATRIX_UPSTREAM` 可以硬指。两个输入同时存在才算命中 ——
    找错层会让脚本读到别的二进制。
    """
    forced = os.environ.get("COVERAGE_MATRIX_UPSTREAM")
    cands = [forced] if forced else []
    # 2026-11-26：本机（主线机）的外部件全在 checkout 内的 `.reference/` 里，
    # 既不是"兄弟目录"也不是上溯 4 层能找到的位置 —— 旧写法在这台机上直接报
    # 「找不到原版二进制 /home/CG/Myprojects/libgraphwarrust.so」（同类陷阱见 docs/18 §10.3.4）。
    cands.append(os.path.join(ROOT, ".reference", "upstream"))
    cands.append(os.path.join(ROOT, ".reference", "game"))
    d = ROOT
    for _ in range(4):
        d = os.path.dirname(d)
        cands.append(d)
    for c in cands:
        if not c:
            continue
        if (os.path.exists(os.path.join(c, "libgraphwarrust.so"))
                and os.path.exists(os.path.join(c, "analysis", "funcstarts_ghidra_va.pkl"))):
            return c
    raise SystemExit(
        "coverage_matrix: 找不到上游分析目录（需要同时有 libgraphwarrust.so 与 "
        "analysis/funcstarts_ghidra_va.pkl）。\n"
        "本机应为 %s/.reference/upstream；可用 COVERAGE_MATRIX_UPSTREAM=<dir> 硬指。"
        % ROOT)


UPSTREAM = _find_upstream()                  # 主仓库（分析仓库）
SO = os.path.join(UPSTREAM, "libgraphwarrust.so")
FUNCSTARTS = os.path.join(UPSTREAM, "analysis", "funcstarts_ghidra_va.pkl")
TSV_PATH = os.path.join(ROOT, "docs", "coverage-matrix.tsv")
MD_PATH = os.path.join(ROOT, "docs", "149-coverage-matrix.md")
AUDIT_PATH = os.path.join(ROOT, "fixtures", "module_denom_audit.tsv")

# 模块清单的主证据：**未 strip** 的 Android 2.0.60（与逆向目标同版本）。
# 它不是必需的输入（Linux 那两份才是），缺失时模块自校验 SKIP 并警告。
ANDROID_SO = (os.environ.get("MODULE_DENOM_ANDROID")
              or os.path.join(ROOT, ".reference", "steam", "android-2.0.60",
                              "libgraphwarrust.so"))

TOOL_VERSION = "2"      # 2 = 模块分母改由未 strip 符号表定（78 → 82）+ sym_funcs 列


# ---------------------------------------------------------------------------
# 模块清单：从未 strip 的 Android 2.0.60 `.symtab` 读**符号路径**（不是 Location）
#   只用 binutils（`readelf` + `c++filt`，与 `docs/02` §18.9 同一套），不引第三方库。
#   返回 None = 这份二进制不在本机（SKIP），不让它变成硬依赖。
# ---------------------------------------------------------------------------
_MOD_ANY = re.compile(r"graphwarrust(?:\[[0-9a-f]+\])?::([A-Za-z0-9_]+)(?:::|\b)")
_MOD_DEF = re.compile(r"^graphwarrust::([A-Za-z0-9_]+)::")
_SYM_LINE = re.compile(r"^\s*\d+:\s+([0-9a-f]+)\s+(\d+)\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)\s+(\S+)\s*$")


def android_symtab_modules(so=None, legacy_only=False):
    """→ {模块: {"func": n, "obj": n, "j1": bool}}；符号表不在本机 → None。

    口径（`fixtures/module_denom_plan.txt` §A/§D）：
    * 只收**已定义**的 `FUNC`/`OBJECT`（`Ndx != UND`），**LOCAL 也算** ——
      `graphwar_id_maker` 整模块只剩一个 `LOCAL OBJECT`（`CURRENT_ID`）；
      把它当"没有 FUNC 就不算模块"正是 T109 数出 65（而不是 66）的原因。
    * 名字统一过 `c++filt -s rust`：legacy `_ZN…` 与 v0 `_RN…` 一起解
      （v0 里 crate 写成 `graphwarrust[<hex>]`，正则已覆盖）。
    * `j1` = 该模块有**定义路径**符号（更严）；`func`/`obj` 是按**定义路径**数的条数。
    * `legacy_only=True` 只解 `_ZN`，用于 §C 的 NC6 健全性对照。
    """
    so = so or ANDROID_SO
    if not os.path.exists(so):
        return None
    try:
        raw = subprocess.run(["readelf", "-sW", so], capture_output=True,
                             text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError) as exc:
        print("coverage_matrix: ⚠ 读不了未 strip 符号表（%s）：%s" % (so, exc),
              file=sys.stderr)
        return None
    defined = []                       # [(type, mangled)]
    for line in raw.splitlines():
        m = _SYM_LINE.match(line)
        if not m:
            continue
        _val, _size, typ, _bind, _vis, ndx, name = m.groups()
        if ndx != "UND" and typ in ("FUNC", "OBJECT"):
            defined.append((typ, name))
    if not defined:
        return None
    names = sorted({n for _t, n in defined})
    if legacy_only:
        names = [n for n in names if n.startswith("_ZN")]
        if not names:
            return None
    try:
        demangled = subprocess.run(["c++filt", "-s", "rust"], input="\n".join(names),
                                   capture_output=True, text=True, check=True).stdout.splitlines()
    except (OSError, subprocess.CalledProcessError) as exc:
        print("coverage_matrix: ⚠ c++filt 不可用：%s" % exc, file=sys.stderr)
        return None
    if len(demangled) != len(names):            # c++filt 吃掉了空行之类
        return None
    dem = dict(zip(names, demangled))
    out = {}
    for typ, name in defined:
        if legacy_only and not name.startswith("_ZN"):
            continue
        d = dem[name]
        for m in _MOD_ANY.finditer(d):
            out.setdefault(m.group(1) + ".rs", {"func": 0, "obj": 0, "j1": False})
        m = _MOD_DEF.match(d)
        if m:
            # 键一律带 `.rs`：与 `docs/coverage-matrix.tsv` 的 module 列、
            # `find_literals()` 的模块名同一套写法（否则对账时会整表"差集全满"）。
            rec = out.setdefault(m.group(1) + ".rs", {"func": 0, "obj": 0, "j1": False})
            rec["func" if typ == "FUNC" else "obj"] += 1
            rec["j1"] = True
    return out



# ---------------------------------------------------------------------------
# capstone 自举：仓库约定分析脚本用 ../.venv/bin/python 跑，而任务书要求
# `python3 tools/coverage_matrix.py --check` 也能直接工作。缺 capstone 时
# 把自己换到 .venv 的解释器上重跑一次，比让人记两套命令更不容易出错。
# ---------------------------------------------------------------------------
def _bootstrap_interpreter():
    try:
        import capstone  # noqa: F401
        return
    except ImportError:
        pass
    if os.environ.get("COVERAGE_MATRIX_BOOTSTRAPPED") == "1":
        raise SystemExit(
            "coverage_matrix: 当前解释器没有 capstone，且自动换用 .venv 后仍然没有。\n"
            "  请先 `python3 -m venv .venv && .venv/bin/pip install -r requirements.txt`\n"
            "  或显式用 `COVERAGE_MATRIX_PYTHON=/path/to/python` 指定解释器。")
    cand = os.environ.get("COVERAGE_MATRIX_PYTHON") or os.path.join(UPSTREAM, ".venv", "bin", "python")
    if not os.path.exists(cand):
        raise SystemExit(
            "coverage_matrix: 缺 capstone，且找不到 %s。\n"
            "  本脚本需要 capstone 做 rip-relative 交叉引用。" % cand)
    env = dict(os.environ, COVERAGE_MATRIX_BOOTSTRAPPED="1")
    os.execve(cand, [cand, os.path.abspath(__file__)] + sys.argv[1:], env)


_bootstrap_interpreter()

from capstone import Cs, CS_ARCH_X86, CS_MODE_64           # noqa: E402
from capstone.x86 import X86_OP_MEM, X86_OP_IMM, X86_REG_RIP  # noqa: E402


# ---------------------------------------------------------------------------
# 最小 ELF64 读取（不依赖上游 analysis/elf.py，工具要能单独重跑）
# ---------------------------------------------------------------------------
def load_elf(path):
    with open(path, "rb") as fh:
        data = fh.read()
    if data[:4] != b"\x7fELF" or data[4] != 2:
        raise SystemExit("coverage_matrix: %s 不是 ELF64" % path)
    (shoff,) = struct.unpack_from("<Q", data, 0x28)
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", data, 0x3A)
    raw = []
    for i in range(shnum):
        off = shoff + i * shentsize
        name, typ, flags, addr, offset, size = struct.unpack_from("<IIQQQQ", data, off)
        raw.append(dict(name_off=name, type=typ, flags=flags, addr=addr, offset=offset, size=size))
    strtab = raw[shstrndx]
    base = strtab["offset"]
    for s in raw:
        end = data.index(b"\0", base + s["name_off"])
        s["name"] = data[base + s["name_off"]:end].decode()
    return data, {s["name"]: s for s in raw}, raw


class Elf:
    def __init__(self, path):
        self.path = path
        self.data, self.by_name, self.sections = load_elf(path)
        self.sha256 = hashlib.sha256(self.data).hexdigest()
        self._alloc = [s for s in self.sections if s["size"] and s["addr"] and (s["flags"] & 0x2)]

    def off(self, addr):
        for s in self._alloc:
            if s["addr"] <= addr < s["addr"] + s["size"]:
                return s["offset"] + (addr - s["addr"])
        return None

    def u64(self, addr):
        o = self.off(addr)
        if o is None or o + 8 > len(self.data):
            return None
        return struct.unpack_from("<Q", self.data, o)[0]

    def u32(self, addr):
        o = self.off(addr)
        if o is None or o + 4 > len(self.data):
            return None
        return struct.unpack_from("<I", self.data, o)[0]

    def sec(self, name):
        s = self.by_name.get(name)
        if s is None or not s["size"]:
            raise SystemExit("coverage_matrix: 缺 section %s" % name)
        return s


# ---------------------------------------------------------------------------
# 第一步：把「文件名字面量 + 行号」从 .rela.dyn 里还原出来
# ---------------------------------------------------------------------------
SRC_LITERAL = re.compile(rb"src/([A-Za-z0-9_]+\.rs)")


def find_literals(E):
    """所有 `src/<name>.rs` 字面量 -> {vaddr: (module, 整段文本)}。"""
    lits = {}
    for s in E._alloc:
        blob = E.data[s["offset"]:s["offset"] + s["size"]]
        for m in SRC_LITERAL.finditer(blob):
            lits[s["addr"] + m.start()] = (m.group(1).decode(), m.group(0))
    return lits


def count_literals(E):
    """{模块: alloc 段里 `src/<模块>.rs` 字面量出现次数} —— 0 次 = Location 口径的盲区。"""
    cnt = {}
    for s in E._alloc:
        blob = E.data[s["offset"]:s["offset"] + s["size"]]
        for m in SRC_LITERAL.finditer(blob):
            k = m.group(1).decode()
            cnt[k] = cnt.get(k, 0) + 1
    return cnt


def resolve_locations(E, lits):
    """解析 .data.rel.ro 里的 panic::Location 结构体。

    结构体布局（`core::panic::Location`）：+0 `file.ptr`（重定位，addend=字面量地址）、
    +8 `file.len`、+16 `line: u32`、+20 `col: u32`。
    只有 `len == 字面量长度` 才算命中 —— 这是自校验，去掉它会把任意
    `(ptr, len)` 对当成 Location。
    """
    rela = E.sec(".rela.dyn")
    addend_to_slots = {}
    for i in range(rela["size"] // 24):
        o = rela["offset"] + i * 24
        r_offset, r_info, r_addend = struct.unpack_from("<QQq", E.data, o)
        if (r_info & 0xFFFFFFFF) == 8:                 # R_X86_64_RELATIVE
            addend_to_slots.setdefault(r_addend, []).append(r_offset)

    locs = {}
    for addend, slots in addend_to_slots.items():
        hit = lits.get(addend)
        if hit is None:
            continue
        module, blob = hit
        for slot in slots:
            if E.u64(slot + 8) != len(blob):
                continue
            line = E.u32(slot + 16)
            col = E.u32(slot + 20)
            if line is None or col is None:
                continue
            # 合理区间自校验：行号/列号不可能是 0 或天文数字
            if not (1 <= line < 100000) or not (1 <= col < 4096):
                continue
            locs[slot] = (module, line, col)
    return locs


# ---------------------------------------------------------------------------
# 第二步：扫 .text，找引用 Location 的指令，落到 Ghidra 函数边界
# ---------------------------------------------------------------------------
def sweep_references(E, locs):
    """返回 {location_slot: [insn 地址, ...]}（只收命中 Location 的指令）。"""
    text = E.sec(".text")
    blob = E.data[text["offset"]:text["offset"] + text["size"]]
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    refs = {}
    for ins in md.disasm(blob, text["addr"]):
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                tgt = (ins.address + ins.size + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
            elif op.type == X86_OP_IMM:
                tgt = op.imm & 0xFFFFFFFFFFFFFFFF
            else:
                continue
            if tgt in locs:
                refs.setdefault(tgt, []).append(ins.address)
    return refs


def load_funcstarts(path):
    with open(path, "rb") as fh:
        starts = pickle.load(fh)
    return sorted(starts)


def attribute(funcstarts, locs, refs):
    """函数 -> 它引用到的模块集合。

    返回 (per_func, contested)：per_func 是 {func: (module, anchor_count)}，
    每个函数只在**一个**模块下出现；contested 是被压平的函数集合。
    """
    import bisect
    votes = {}                       # func -> {module: set(line)}
    for slot, addrs in refs.items():
        module, line, _col = locs[slot]
        for a in addrs:
            i = bisect.bisect_right(funcstarts, a) - 1
            if i < 0:
                continue
            fn = funcstarts[i]
            votes.setdefault(fn, {}).setdefault(module, set()).add(line)

    per_func, contested = {}, set()
    for fn, bymod in votes.items():
        if len(bymod) > 1:
            contested.add(fn)
        # 锚点数最多者胜，并列取模块名字典序 —— 纯确定性，不依赖 dict 顺序
        module = min(bymod, key=lambda m: (-len(bymod[m]), m))
        per_func[fn] = (module, len(bymod[module]))
    return per_func, contested


# ---------------------------------------------------------------------------
# 第三步：我们这一侧 —— src/*.rs 的 fn 条数
# ---------------------------------------------------------------------------
# 字符字面量：`'a'`、`'\n'`、`'\x1b'`、`'\u{1F600}'`
CHAR_LIT = re.compile(r"'(?:\\x[0-9a-fA-F]{2}|\\u\{[0-9a-fA-F]{1,6}\}|\\.[^']*|[^\\'])'")


def strip_rust_noise(src):
    """去掉注释与字符串/字符字面量，避免把注释里的 `fn` 数进来。"""
    out = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            j = src.find("*/", i + 2)
            i = n if j < 0 else j + 2
            out.append(" ")
            continue
        if c == "r" and i + 1 < n and src[i + 1] in '#"':
            m = re.match(r'r(#*)"', src[i:])
            if m:
                close = '"' + m.group(1)
                j = src.find(close, i + m.end())
                i = n if j < 0 else j + len(close)
                out.append(" ")
                continue
        if c == "'":
            # **只**把真正的字符字面量当字面量。`'a` / `'static` 是生命周期，
            # 按引号配对去吞会一路吃到下一个 `'`，把整段代码（含 fn）都删掉。
            m = CHAR_LIT.match(src, i)
            if m:
                i = m.end()
                out.append(" ")
            else:
                out.append(c)
                i += 1
            continue
        if c == '"':
            i += 1
            while i < n:
                if src[i] == "\\":
                    i += 2
                    continue
                if src[i] == '"':
                    i += 1
                    break
                i += 1
            out.append(" ")
            continue
        out.append(c)
        i += 1
    return "".join(out)


FN_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def count_fns(path):
    with open(path, encoding="utf-8", errors="replace") as fh:
        return len(FN_RE.findall(strip_rust_noise(fh.read())))


def src_files():
    out = []
    base = os.path.join(ROOT, "src")
    for dirpath, _dirs, files in os.walk(base):
        for name in sorted(files):
            if name.endswith(".rs"):
                full = os.path.join(dirpath, name)
                out.append(os.path.relpath(full, ROOT).replace(os.sep, "/"))
    return sorted(out)


def src_tree_sha256(files):
    h = hashlib.sha256()
    for rel in files:
        h.update(rel.encode())
        h.update(b"\0")
        with open(os.path.join(ROOT, rel), "rb") as fh:
            h.update(hashlib.sha256(fh.read()).digest())
    return h.hexdigest()


# ---------------------------------------------------------------------------
# 第四步：模块 -> 我们的实现 / 证据档（唯一的人工判断部分，全部写明依据）
#
# 字段：ours = 参与实现该模块的 src/*.rs（空元组表示没有实现）
#       status / evidence 见文件头那张表。
#       sym_funcs = Android 2.0.60 `.symtab` 里该模块的 FUNC 条数（**规模参考列**，
#                   不进 orig_funcs / 不进 527；默认从下面 ANDROID_SYM_FUNCS 取）。
# 这张表就是"进度尺"的读数口径；改它必须同时改 evidence，否则 --check 会
# 拿旧 TSV 对新结果失败。
# ---------------------------------------------------------------------------
# ---------------------------------------------------------------------------
# 模块规模的**手工列/覆盖表**（2026-11-28 T112）：Android 2.0.60 未 strip `.symtab` 的
# **FUNC 条数**（定义路径 = `graphwarrust::<模块>::…`）。这就是"Location 缺失的模块"
# 也能有规模数字的载体 —— 之前这类模块的 `orig_funcs` 只能是 0，与"没测过"分不开。
#
# 复算（只依赖 binutils，任何机器可重跑；`--check` 会在符号表存在时逐值核对本表）：
#   readelf -sW .reference/steam/android-2.0.60/libgraphwarrust.so \
#     | awk '$4=="FUNC"||$4=="OBJECT"' | ... → c++filt -s rust → 取路径第 2 段
#   ⚠️ 手工数容易漂（T109 就数成 65）：所以 `--check` 拿活的符号表对，不许改本表了事。
# ⚠️ 口径：这是**编译出来的函数条数**（含闭包/单态化/drop glue），不是原版源码里的
#    方法个数；同样**不是**进度分子。
# ---------------------------------------------------------------------------
ANDROID_SYM_FUNCS = {
    # scope=graphwar（66 个模块）
    "graphwar_ad_switch.rs": 8,
    "graphwar_audio_control.rs": 13,
    "graphwar_backend.rs": 19,
    "graphwar_ban_list.rs": 2,                  # T112 补行（T109 落码）
    "graphwar_button.rs": 13,
    "graphwar_campaign_control.rs": 4,
    "graphwar_client_connection_handler.rs": 10,
    "graphwar_color_manager.rs": 9,
    "graphwar_computer_player_manager.rs": 9,
    "graphwar_crash_reporter.rs": 3,
    "graphwar_draw_util.rs": 1,
    "graphwar_effect_manager.rs": 5,
    "graphwar_entity_effect_manager.rs": 3,
    "graphwar_event.rs": 5,
    "graphwar_event_handler.rs": 2,
    "graphwar_event_queue.rs": 2,
    "graphwar_function_evolutionator.rs": 4,
    "graphwar_function_input.rs": 24,
    "graphwar_function_manager.rs": 10,
    "graphwar_game_client.rs": 14,
    "graphwar_game_info.rs": 1,
    "graphwar_game_recorder.rs": 3,
    "graphwar_game_server.rs": 7,
    "graphwar_game_state_manager.rs": 44,
    "graphwar_graphics_manager.rs": 8,
    "graphwar_id_maker.rs": 0,                  # T112 补行：只剩 1 个 LOCAL OBJECT
    "graphwar_lan_broadcaster_v6.rs": 5,
    "graphwar_lan_vbox.rs": 5,
    "graphwar_level_generator.rs": 6,
    "graphwar_level_manager.rs": 47,
    "graphwar_life_manager.rs": 12,
    "graphwar_lobby_control.rs": 5,
    "graphwar_local_player_manager.rs": 6,
    "graphwar_logger.rs": 3,
    "graphwar_long_press_animation.rs": 4,
    "graphwar_mode_manager.rs": 10,
    "graphwar_module_container.rs": 68,
    "graphwar_name_manager.rs": 6,
    "graphwar_nodes.rs": 29,
    "graphwar_obstacle_manager.rs": 10,
    "graphwar_path_calculator.rs": 1,
    "graphwar_plane.rs": 10,
    "graphwar_plane_pos_manager.rs": 8,
    "graphwar_play_screen_control.rs": 13,
    "graphwar_player_config.rs": 8,
    "graphwar_player_manager.rs": 6,
    "graphwar_player_screen_control.rs": 13,
    "graphwar_rank_manager.rs": 6,
    "graphwar_remote_event_handler.rs": 3,
    "graphwar_replay_control.rs": 4,
    "graphwar_replay_player.rs": 9,
    "graphwar_replay_reader.rs": 2,
    "graphwar_root_node.rs": 24,
    "graphwar_server_broker_handler.rs": 5,
    "graphwar_server_connection_handler.rs": 12,
    "graphwar_setup_control.rs": 27,
    "graphwar_shot_calculator.rs": 4,
    "graphwar_soldier_manager.rs": 7,
    "graphwar_stream_buffer.rs": 2,
    "graphwar_target_manager.rs": 3,
    "graphwar_team_manager.rs": 9,
    "graphwar_texture_loader.rs": 4,
    "graphwar_tick_handler.rs": 6,
    "graphwar_turn_manager.rs": 17,
    "graphwar_version.rs": 1,                   # T112 补行（T109 落码）
    "graphwar_video_writer.rs": 3,
    # scope=other（16 个模块）
    "axis_drawer.rs": 4,
    "backtracer.rs": 2,
    "button_manager.rs": 2,
    "effect_drawer.rs": 6,
    "entity_drawer.rs": 13,
    "function_button_manager.rs": 3,
    "function_drawer.rs": 5,
    "function_history_recorder.rs": 3,
    "function_token.rs": 1,
    "math_function.rs": 10,
    "obstacle_drawer.rs": 1,
    "player_showcase.rs": 7,
    "polish_notation_converter.rs": 1,
    "server_broker_request_connection.rs": 1,
    "time_probe.rs": 0,                         # T112 补行：0 FUNC / 0 方法，只有 drop glue
    "weak_caller.rs": 35,
}

M = {}


def mod(module, scope, ours, status, evidence, sym_funcs=None):
    if sym_funcs is None:
        sym_funcs = ANDROID_SYM_FUNCS.get(module)
    M[module] = dict(module=module, scope=scope, ours=tuple(ours),
                     status=status, evidence=evidence, sym_funcs=sym_funcs)


def _src_graphwar_files():
    """`src/` 顶层 `graphwar_*.rs` 的**文件**个数（现算，不写死）。"""
    import glob
    return len(glob.glob(os.path.join(ROOT, "src", "graphwar_*.rs")))


def _src_graphwar_files_alpha():
    r"""只匹配 `graphwar_[a-z_]+\.rs` 时的个数 —— 会漏掉带数字的 `graphwar_lan_broadcaster_v6.rs`。"""
    import re
    d = os.path.join(ROOT, "src")
    if not os.path.isdir(d):
        return 0
    pat = re.compile(r"^graphwar_[a-z_]+\.rs$")
    return sum(1 for f in os.listdir(d) if pat.match(f))


def graphwar_prefixed():
    """**有同名文件**的 `scope=graphwar` 模块数（现值 **58**）。

    2026-11-26 修：本文件原来把 58 写死在 9 处文案里（`docs/149` 因此出现
    "表头 58 / 模块数 62"的自相矛盾）。**数字一律算出来**，别再手写。

    口径（`docs/02` §17 开头）：`scope=graphwar` 共 **66** 个模块（T112 修正前是 62）。
    **数字一律算出来** —— 它随 `src/` 与模块清单变化，任何写死的读数都会过期
    （本函数上面那段注释里的 58/62/59 就是 2026-11-26 之前的读数，已作废）。

    T112（2026-11-28）为什么从 62 变 66：
      +3 个**整模块缺行**的（`graphwar_ban_list` / `graphwar_version` / `graphwar_id_maker`，
          前两个 T109 已落码、第三个未做）
      +1 个**scope 记错**的（`graphwar_texture_loader`：带 `graphwar_` 前缀，旧表记成 `other`）
    ⇒ 分母 78 = 62+16 修成 **82 = 66+16**；口径与正负对照见 `fixtures/module_denom_plan.txt`。
    """
    return sum(1 for k, v in M.items()
               if v["scope"] == "graphwar" and ("src/" + k) in v["ours"])


# ---- graphwar 作用域模块（66 个 = 62 个旧行 + 3 个 T112 补的缺行 + 1 个 T112 改回 graphwar 的 texture_loader）----
# 名字全带 `graphwar_` 前缀 —— 正则别用 `[a-z_]+`，会漏 `graphwar_lan_broadcaster_v6`

mod("graphwar_ad_switch.rs", "graphwar", ("src/graphwar_ad_switch.rs",), "观测",
    "6 条全是 gdext 惰性注册驱动；两个 getter trampoline + owned(+0x18) 已钉死")
mod("graphwar_audio_control.rs", "graphwar", ("src/graphwar_audio_control.rs",), "观测",
    "9 函数 + 4 订阅闭包。订阅 effect_info(44，按 +0x20 的 u8 四臂落三槽) / soldier_info(15) / entity_removed(14) / game_state_info(30)；槽 +0x80/90/a0/b0/c0 = explosion/squash/spark/pop/unpop，音量 -15/-15/-15/0/-5 dB；state==1 清待播队列（docs/16 §30）")
mod("graphwar_backend.rs", "graphwar", ("src/graphwar_backend.rs",), "观测",
    "**18 个方法**（旧文记 15，偏少）。LAN 与 broker 是两个独立字段**（+0x110 组播 ff02::1 / +0x228 broker），开房时一起装；「看哪一路」在 GraphwarLanVBox::show_global，不在这层（docs/09 §11）")
mod("graphwar_button.rs", "graphwar", ("src/graphwar_button.rs",), "观测",
    "6/6 方法全解（init/ready/process/button_down/pressed/button_up）+16 条名字分派入口；docs/16 §18")
mod("graphwar_campaign_control.rs", "graphwar", ("src/graphwar_campaign_control.rs",), "观测",
    "定性完成；get_current_level 返回 Option 给 local_player_manager 的 RankRequest 用（docs/16 §33）")
mod("graphwar_client_connection_handler.rs", "graphwar", ("src/graphwar_client_connection_handler.rs",), "观测",
    "11 FUNC（8 方法 + 3 订阅闭包 + 连接线程）。ConnectionState 4 个变体（0/1/2/3），**与 GameState(8) 是两套**，唯一交点：失败时 set_state(7)。本机地址是 [::1]:port（**不是 127.0.0.1**）；远端只认 IP 字面量、**无 DNS**；connect_timeout 30s、**无重连**、**不发也不订阅心跳**（docs/09 §11）")
mod("graphwar_color_manager.rs", "graphwar", ("src/graphwar_color_manager.rs",), "观测",
    "7/7（含 GraphwarColor::serialize）：两张 6 色表就是 init_team_colors 里的字面量；分配 = **队号选池** -> rand::random::<u64>()%6 起点 -> +1 线性探测跳过 used -> 6 个全占则 (r%180,g%180,b%180)。**不是进房顺序、不是 player_id 取模**（docs/10 §15）")
mod("graphwar_computer_player_manager.rs", "graphwar", ("src/graphwar_computer_player_manager.rs",), "观测",
    "6/7 定位：HashMap<u32,BotData>（步长 0x78）+ SoldierBotData（0x58）。**不是掉线托管**：唯一入口是 bot_added_event(10)；level 阈值 4/11/26/101/251，精英数=种群/5，limit=5/5/10/15/25/200（docs/13 §25）")
mod("graphwar_crash_reporter.rs", "graphwar", ("src/graphwar_crash_reporter.rs",), "观测",
    "崩溃日志的**上报端**（与落盘端 backtracer 是两个模块），**不抓栈**。端口 42972（mov w8,#0xa7dc ×2），三级回退：CLI/env --crash_report_address -> 全局地址+42972 -> pending。钩子在 GraphwarRootNode::ready +0x30/+0x44 => 每次进主场景装一次。2024 版 0 符号 0 串 => 后加功能（docs/16 §33）")
mod("graphwar_draw_util.rs", "graphwar", ("src/graphwar_draw_util.rs",), "观测",
    "只有 get_team_overview(0x4be6e0)，**不画东西**（有界否定）（docs/16 §33）")
mod("graphwar_effect_manager.rs", "graphwar", ("src/graphwar_effect_manager.rs",), "观测",
    "5（含 EffectType::deserialize）：EffectType 4 个取值已穷举（ExplosionEffect/DeathEffect/SparkEffect/AshEffect）（docs/16 §32）")
mod("graphwar_entity_effect_manager.rs", "graphwar", ("src/graphwar_entity_effect_manager.rs",), "观测",
    "3 + 内联 add_entity_effect。登记表 = u32->u8 的 entity_effect_map（全库唯一，桶距 8）；**死亡特效照抄出生登记**：execute_results 用 effect_type=map[entity_id]，查不到就一条都不发（0x4a8c38）=> Ash/Death 的分界在出生登记处，正是 docs/141 那 19 例的成因（docs/16 §32）")
mod("graphwar_event_handler.rs", "graphwar", ("src/graphwar_event_handler.rs",), "观测",
    "docs/114：0x3d38f0 的「事件名 → 判别值」表逐条解出（四道自校验）；表 + 查表端已落码。**2026-11-26 T30 切出同名文件**：0x3d3630/0x3d38f0 两条 panic::Location 全落 src/graphwar_event_handler.rs（fixtures/boundary_audit.tsv），故从 src/events.rs 搬入 `DISPATCH_NAME_TAGS` / `NAMED_VARIANT_COUNT` / `Event::from_dispatch_name`，老路径由 pub use 保持；⚠ process_events 仍**无独立函数体**（泵在 _scaffold/root_wiring.rs，docs/06 §80）")
mod("graphwar_event.rs", "graphwar", ("src/events.rs",), "观测",
    "docs/114：72 个判别值实测；但 serde 键序只在 41 行抓包上同序，仍是推断（docs/96 §9.1）")
mod("graphwar_function_evolutionator.rs", "graphwar", ("src/graphwar_function_evolutionator.rs",), "观测",
    "docs/AI.md：权重/长度/装配/交叉/变异/适应度全部从反汇编 + 40 万次运行时抽样得出；无逐字节对拍")
mod("graphwar_function_input.rs", "graphwar", ("src/graphwar_function_input.rs",), "观测",
    "17 条函数；7 个原版名 + gui_input 0x4fedc0；**输入框只做一步预处理：删空格**（docs/03 §13）")
mod("graphwar_function_manager.rs", "graphwar", ("src/graphwar_function_manager.rs",), "观测",
    "docs/112 §2.1：7 个函数的存取结构逐条读出（HashMap<u32, 112 字节>、Vec<Token>）；"
    "我方 Match::functions 同构，但没有逐字节对拍")
mod("graphwar_game_client.rs", "graphwar", ("src/graphwar_game_client.rs",), "观测",
    "36 个函数分 8 类（docs/16 §17）：8 种请求事件判别值 11/12/16/17/18/19/20/21 + 0x48 记录形状 + "
    "add_bot_0..5 -> 3/10/25/100/250/1000 + 0x517690 的 3 订阅名/14 manager；6 个入口函数语义未知")
mod("graphwar_game_recorder.rs", "graphwar", ("src/graphwar_game_recorder.rs",), "观测",
    "3：订 32 条服务端广播事件（24 条 *_info_event + 8 条不带 info 的；*_request_event 一条不订）；GameStateInfo==6 EndingGame 上升沿落盘 user://replays/<unix 秒>（gzip + 每行 {event,tick}），==1 Setup 且上一条是 6 时清缓冲（docs/16 §32）")
mod("graphwar_game_server.rs", "graphwar", ("src/graphwar_game_server.rs", "src/server.rs"), "逐位",
    "docs/43/46：一回合 12 条报文与抓包逐字节一致；docs/62 真实 TCP 跑到 EndingGame")
mod("graphwar_game_state_manager.rs", "graphwar",
    ("src/game.rs", "src/turnstate.rs", "src/graphwar_game_state_manager.rs"), "逐位",
    "docs/41：6 状态 7 事件、13 次迁移在真实对局里逐个相同；"
    "**任务 95** 起事件层（7 条订阅 + 3 个处理器）在 src/graphwar_game_state_manager.rs（文件名 = 原版模块名）")
mod("graphwar_graphics_manager.rs", "graphwar", ("src/graphwar_graphics_manager.rs",), "观测",
    "6 函数。**TextureLoader 不在本模块**——它是独立模块 graphwar_texture_loader（三张 HashMap<String,Gd<Texture2D>> @ +0x150/+0x180/+0x1b0，按名字 memcmp 索引，GraphwarRootNode 的 INode impl 里 0x39a328 唯一构造、21KB 饿汉加载）。本模块只管 player_id→名字 三张表（+0x18/+0x48/+0x78）与 51/53/55 三条 info 事件（docs/16 §31）")
mod("graphwar_lan_broadcaster_v6.rs", "graphwar", ("src/graphwar_lan_broadcaster_v6.rs",), "观测",
    "5/5（get_game_info 两侧都无锚，不给 x86 地址）。广播周期 **1 秒**（elapsed > 1s，正好 1.000s 不发）；收侧去重 = 房间表键发送方 SocketAddr + **10 秒 TTL**（cmp x0,#0xa）；**广播的 address 恒为 [::1]:<port>**（set_room_info 里 IpAddr::from_str(「::1」)）（docs/09 §12）")
mod("graphwar_lan_vbox.rs", "graphwar", ("src/graphwar_lan_vbox.rs",), "观测",
    "13 个 FUNC 全登记；process = 0x4f90b0（唯一锚，16 个三元组）。**不消费任何事件**——每帧直读两张表（LAN 广播器 +0x128/+0x1a0 或 broker +0x240/+0x248）；行节点 = 新场景实例 + 按 SocketAddr 缓存复用；Join 走 mpmc 通道下一帧才连；**没有排序**（HashMap 迭代序）（docs/09 §10）")
mod("graphwar_level_generator.rs", "graphwar", ("src/graphwar_level_generator.rs", "src/graphwar_level_generator/tables.rs"), "逐位",
    "docs/61/56：障碍 x/y/r 的 to_bits() 全等，13/13 逐位复现；docs/55 公式闭合")
mod("graphwar_level_manager.rs", "graphwar", ("src/graphwar_level_manager.rs",), "观测",
    "锚点最密（590 个 Location / 40 个函数）；LEVELS[21]+SETUPS[21]+campaign.save 逐位（docs/10 §12），"
    "40 个锚函数里 26 个已还原、14 个仅分类；level_info/campaign.save 簇无锚，落地时可能要搬去 campaign_control")
mod("graphwar_life_manager.rs", "graphwar", ("src/graphwar_life_manager.rs",), "逐位",
    "docs/65：穿过即死/友军致死/无血量在真实对局与回放里逐个相同；docs/41 状态迁移一致")
mod("graphwar_lobby_control.rs", "graphwar", ("src/graphwar_lobby_control.rs",), "观测",
    "3/3（旧记 1，两个 show_* 在 x86 内联进 new）：大厅**模式开关**（ShowLocalButton/ShowGlobalButton），互斥 set_pressed + 切模式时把 LAN 列表整表 queue_free；列表本体归 lan_vbox（docs/10 §15）")
mod("graphwar_local_player_manager.rs", "graphwar", ("src/graphwar_local_player_manager.rs",), "观测",
    "4/4：**本机 player_id 的单一真相**（+0x18 已连上 / +0x1c connection_id / Vec<u32>）。首次入列五连发 = 59 NameRequest / 50 / 52 / 54 / 66 RankRequest；**66 的发起侧升为逐位**（全 .text 无第二处）（docs/10 §15）")
mod("graphwar_logger.rs", "graphwar", ("src/graphwar_logger.rs",), "观测",
    "定性完成；HTTP 报文格式未解（docs/16 §33）")
mod("graphwar_long_press_animation.rs", "graphwar", ("src/graphwar_long_press_animation.rs",), "观测",
    "2.0.60 只剩 draw（0x4d8100）：angle=elapsed×4π、>2π 结束⇒0.5s、黑白两次绘制；9 个常量逐比特")
mod("graphwar_mode_manager.rs", "graphwar", ("src/graphwar_mode_manager.rs",), "观测",
    "只有**三套**（Axis/Function/Time），TurnMode 在 turn_manager；三个 setter 各 124B 形状相同（写 1 字节 + add_event），写入者是 handle_server_remote_event => 它是**服务器状态的副本**，不是唯一真相（docs/16 §33）")
mod("graphwar_module_container.rs", "graphwar", ("src/graphwar_module_container.rs",), "观测",
    "docs/02 §22：类型本体 = HashMap<TypeId, Rc<dyn Any>>（48 字节 / 1 字段），"
    "3 个泛型方法 × 25 型 = 68 个单态化；三块两侧都等距且首尾背靠背"
    "（arm64 0x49e520/0x49f9d4/0x4a20a4，x86 0x4694f0/0x46a490/0x46c5a0），"
    "25 型真名 + size + 六个入口全部定名；装配在调用者（GameClient::new 22 / "
    "game_server_thread 20 / Backend 1×2），容器自己没有 init；"
    "Location 只有 get_module_wk 一个（30:28，RefCell::borrow 的 track_caller）"
    "⇒ add/get 零锚是正确结果")
mod("graphwar_name_manager.rs", "graphwar", ("src/graphwar_name_manager.rs",), "观测",
    "4/4：HashMap<u32,String>，**名字的权威源**（graphics_manager 三张表是皮肤/脸/帽资源名，不是名字）。改名链路：59 NameRequest 进 -> set_name（唯一调用者 remote_event_handler）-> 60 NameInfo 广播（docs/10 §15）")
mod("graphwar_nodes.rs", "graphwar", ("src/graphwar_nodes.rs",), "观测",
    "11/11：**不造节点**，是全客户端唯一的「按名字/按类型在场景树里找节点」入口——get_node_by_name::<T>（11 个单态化薄壳）/ get_node_by_name_rec（17）/ get_node_by_type_rec（1）；另给 40 个节点名 + 80 个调用点三张表（docs/16 §28）")
mod("graphwar_obstacle_manager.rs", "graphwar", ("src/graphwar_obstacle_manager.rs",), "逐位",
    "只覆盖掩膜部分：0x4f15b0 光栅化与 is_occupied 与真二进制逐字节 / 300 随机点一致"
    "（docs/39/08），摧毁 = 挖 r=0.5 洞；障碍的生成与生命周期未落 src/。"
    "⚠ 文件级归属未定：class_probe 给出的本模块锚函数是 0x3c64c0/0x3f00b0/0x3f2480/0x3f27d0，"
    "0x4f15b0 不在其中（docs/02 §17.1 记录了这处冲突，不强行圆）")
mod("graphwar_path_calculator.rs", "graphwar", ("src/graphwar_path_calculator.rs",), "逐位",
    "docs/91：补上 hi=mid2 后 24/24 用例整条点序列与真二进制逐位相等。注意：采样器 "
    "0x3c2530 本体无 panic 锚 ⇒ 只归属到 1 个辅助函数，orig_funcs 远小于模块规模")
mod("graphwar_plane_pos_manager.rs", "graphwar", ("src/graphwar_plane_pos_manager.rs",), "观测",
    "docs/140：px = 20x + 500 由常量反查得出、准备圈画法已扒出；没有端到端逐位对拍")
mod("graphwar_plane.rs", "graphwar", ("src/graphwar_plane.rs", "src/plane.rs"), "观测",
    "12 个 GDExtension 类里的第 4 个，本轮补齐（docs/16-client.md §26）：8 个方法 + 1 个 "
    "`ready::{{closure}}` + 6 个 gdext 入口全部落到 x86 地址，判据是 arm64 `.symtab` 定名 + "
    "三类指纹（19 个 Location 键集 / 被调者多重集 / 首尾相接链）。`draw` 的图元逐位"
    "（`Rect2(-25,-15,50,30)` + `Color(0,0,0,1)` + width `0.2` + `filled=false`），"
    "`obstacle_partial_update_event` 的坐标变换逐位（常量槽 `0x19fbd0`/`0x19faa8`/`0x19ee00`…）。"
    "⚠ 旧行写 `逐位` 是按**掩膜光栅化**算的，而掩膜属 `graphwar_obstacle_manager`"
    "（同一张表里另有一行）⇒ 本模块自身只到 `观测`：没有整函数的逐字节对拍。"
    "`src/plane.rs` 里剩下的掩膜 / 换算归 obstacle_manager 与 plane_pos_manager，见 docs/02 §17.4")
mod("graphwar_player_manager.rs", "graphwar", ("src/graphwar_player_manager.rs",), "观测",
    "4/4（旧记 3，add_player 在 x86 被内联、arm64 有独立符号）：player_id -> connection_id；该连接最后一人被删时级联 ConnectionRemoved(6)（docs/10 §15）")
mod("graphwar_player_screen_control.rs", "graphwar", ("src/graphwar_player_screen_control.rs",), "观测",
    "8/8：**就是写 GraphwarPlayerConfig 的地方**——set_skin/set_face/set_hat 三条具名 bl 直证，三个 setter 写 +0x60/+0x78/+0x90 且 inc dword [+0xa8]，与 docs/16 §22.9 布局逐字段吻合（§29）")
mod("graphwar_play_screen_control.rs", "graphwar", ("src/graphwar_play_screen_control.rs",), "观测",
    "docs/14-turn.md §9 逐函数规格（10 个可归属函数 / 64 个锚）；界面视图仍在 client/ GDScript（仿制），"
    "但控制器本体已落在 src/graphwar_play_screen_control.rs。旧行写「未做」与 src/lib.rs/文件头冲突 ⇒ 按代码更正")
mod("graphwar_rank_manager.rs", "graphwar", ("src/graphwar_rank_manager.rs",), "观测",
    "4 函数。段位**只从远端事件进**（RemoteEventHandler::handle_{client,server}_remote_event 各一处 set_rank），模块内零换算；RankInfo(67) = 三个 dword（+8 标志 / +0xc 段位 / +0x10 player_id）；66 由 local_player_manager 发（推断）（docs/10 §14）")
mod("graphwar_remote_event_handler.rs", "graphwar", ("src/graphwar_remote_event_handler.rs",), "推断",
    "docs/110/96：远端事件名与判别值已解。**2026-11-26 T30 切出同名文件**：0x3dee00/0x3e5910 两条 panic::Location 全落本文件（fixtures/boundary_audit.tsv），故从 events.rs 的混装里拆出独立文件，文件里装 MODULE/SOURCE_FILE/2 条 Func 证据行 + is_leader 指令级观测。⚠ **两个函数体仍未落码（0 个 fn）** ⇒ 本行只记到 `推断`（`观测`/`逐位` 都要求'有 src/ 实现'，见本脚本文件头的 status 表）；缺的证据：arm64 2.0.60 对照、0x46d8c0 是哪个单态化实例、调用点分析（docs/06 §80 遗留 2）")
mod("graphwar_replay_control.rs", "graphwar", ("src/graphwar_replay_control.rs",), "观测",
    "4：回放控制；时钟字段仍未解（docs/16 §32.7）")
mod("graphwar_replay_player.rs", "graphwar", ("src/graphwar_replay_player.rs",), "观测",
    "docs/136/142：消息级对拍 400 份回放里 71 份完全一致 ⇒ 已实现但未到逐位")
mod("graphwar_root_node.rs", "graphwar", ("src/graphwar_root_node.rs",), "观测",
    "Screen×15 + Command×12 + 0x3af850 切屏决策表 + 构造初值（docs/16 §15.10）；"
    "0x3a5760 主体 20288B 刻意未猜")
mod("graphwar_server_broker_handler.rs", "graphwar", ("src/graphwar_server_broker_handler.rs",), "观测",
    "6/6（含线程体；get_game_info/new 无锚不给地址）。Global 一路无 TTL 扫描——疑靠 GameRemoved，该报文从未抓到（docs/09 §12）")
mod("graphwar_server_connection_handler.rs", "graphwar", ("src/graphwar_server_connection_handler.rs",), "逐位",
    "docs/33：新连接的状态同步与真实客户端逐字节一致。**2026-11-26 T30 切出同名文件**：0x45a560/6b0/7b0/ab0/ad0/b020 六条 panic::Location 全落本文件（fixtures/boundary_audit.tsv），故 RoomState/on_new_connection/to_wire 从 src/server.rs 搬入（server.rs 只留 tick 薄封装 + pub use 再导出）；⚠ 那 6 个有锚函数的**函数体**仍未落码（docs/06 §80）")
mod("graphwar_setup_control.rs", "graphwar", ("src/graphwar_setup_control.rs",), "观测",
    "70 锚 -> 37 函数（矩阵的 21 是压平口径）；四格模式/时间档/玩家条目/Start-Cancel 已落码（docs/16 §16），"
    "18 个 gdext 薄壳与 3 个事件回呼语义未知")
mod("graphwar_shot_calculator.rs", "graphwar", ("src/graphwar_shot_calculator.rs", "src/graphwar_shot_calculator/settle.rs"), "逐位",
    "docs/86 §14：82 发并集 152/152、漏报 0、时刻判决 38:0；docs/122 按 id 排除射手是"
    "指令级。注意：0x3c2bc0 那个 12 KB 主函数**没有逐条还原**，逐位的是行为不是函数体")
mod("graphwar_soldier_manager.rs", "graphwar", ("src/graphwar_soldier_manager.rs",), "观测",
    "5/6 定位：HashMap<u32,u32>（士兵→主人）。只管「谁的」，**轮转不查它**（turn_manager 自持 Vec<soldier_id>）；entity_removed 时比值命中就发键 => 玩家没了士兵跟着消失（docs/10 §13）")
mod("graphwar_target_manager.rs", "graphwar", ("src/graphwar_target_manager.rs",), "观测",
    "3 + 内联 add_target。**只是 HashSet<u32>：无 alive 字段、唯一写操作是 remove**。「打不全就全部复原」的判定点**不在这里**（见 game_state_manager 那条）（docs/16 §32）")
mod("graphwar_team_manager.rs", "graphwar", ("src/graphwar_team_manager.rs",), "观测",
    "7/7 定位：Mutex<HashMap<u32, GraphwarTeam>>（桶步长 8，值 1 字节）。**唯一分队规则 0x3c1b40：进人少的队、平手进 Team1**；无队长/选边/每队容量，上限整房 10 人（docs/10 §13）")
mod("graphwar_texture_loader.rs", "graphwar", ("src/graphwar_texture_loader.rs",), "观测",
    "**4，不是 1**（旧记 1 是 Location 口径下限：三个 getter 从不 panic）。三张 HashMap<String,_> @ +0x150/+0x180/+0x1b0（步长 0x30、表项 0x28），按名字 memcmp，21KB 饿汉加载；唯一构造点 0x39a328 在 GraphwarRootNode 的 INode impl 里（docs/16 §31/§32）")
mod("graphwar_tick_handler.rs", "graphwar", ("src/graphwar_tick_handler.rs",), "逐位",
    "docs/35：113 个真实样本，TickReply 逐字节一致")
mod("graphwar_turn_manager.rs", "graphwar", ("src/graphwar_turn_manager.rs",), "逐位",
    "docs/98/117：射手轮转 = Vec<soldier_id> + 游标，在真实 TCP 上断言「首发射手 = soldiers[1]」"
    "「跳过死者」；docs/41 状态迁移逐个相同")
mod("graphwar_video_writer.rs", "graphwar", ("src/graphwar_video_writer.rs",), "观测",
    "3：**真的是录视频**——openh264::Encoder + minimp4（60 fps，轨道名 Graphwar Clip），FileAccess 一次写盘；尺寸向上取偶。本仓只做结构，不产 MP4（无 openh264 依赖）（docs/16 §32）")

# ---- T112（2026-11-28）补齐 3 个**整模块缺行**的 graphwar 模块：它们的 Location 串在 Linux .so 里 0 次 ----
# 判据与正/负对照见 `fixtures/module_denom_plan.txt`；三方差集表见 `fixtures/module_denom_audit.tsv`。
mod("graphwar_ban_list.rs", "graphwar", ("src/graphwar_ban_list.rs",), "观测",
    "**Android 2.0.60 `.symtab`: 2 FUNC**（`GraphwarBanList::new` 0x4086a0 / `is_connection_banned` 0x408460）。"
    "96 字节值 + 120 B `RcInner<RefCell<…>>`；两张表 ctrl+0x00/mask+0x08/items+0x18/hasher+0x20。"
    "Linux `.so` 里 `src/graphwar_ban_list.rs` 串 **0 次** ⇒ 旧 Location 口径**整模块看不见**（T109 落码，T112 补行）。"
    "封禁的可见效果 = 入队 `\"You have been kicked.\"`（docs/06 §153.3/§153.5）")
mod("graphwar_version.rs", "graphwar", ("src/graphwar_version.rs",), "观测",
    "**Android 2.0.60 `.symtab`: 1 FUNC**（`graphwar_version_get_full_version` 0x3913e4）。返回**编译期常量** "
    "\"2.0.60\"（4 处立即数拼 6 字节 + sret 3 字）。Linux `.so` 里该模块的 Location 串 **0 次**（T109 落码，T112 补行）")
mod("graphwar_id_maker.rs", "graphwar", (), "未做",
    "**T112 新发现：整模块缺行，而且 `FUNC = 0`** —— Android `.symtab` 里只剩 **1 个 LOCAL OBJECT** "
    "`graphwarrust::graphwar_id_maker::GraphwarIdMaker::get_unique_id::CURRENT_ID`（4 字节 @ arm64 `0x8b4378`）。"
    "`get_unique_id` 本体**被内联**（无独立 FUNC 边界），所以 T109 的「只看 FUNC」扫法数出 65 个模块时漏了它。"
    "我方**没有** `src/graphwar_id_maker.rs` ⇒ 未做；缺的证据：`CURRENT_ID` 的初值/步进（读 .data 段）+ 调用点"
    "（`get_unique_id` 被谁内联进去）。**这是本轮模块级完成度从 100 % 掉下来的 2 个之一，不许含糊**（docs/06 §157）")

# ---- 非 graphwar_ 前缀：同样是原版 src/ 里的文件，但只有 math_function 等少数是真游戏模块 ----
mod("math_function.rs", "other", ("src/math_function.rs", "src/math_function/to_text.rs"), "逐位",
    "docs/03：90 例文法 + 70 条真值夹具与真二进制对拍。**解析器 0x4ed670 的 panic 锚就落在这个"
    "文件上**（math_function.rs:75）⇒ 函数语言的解析器属于本模块，不属于 graphwar_function_input")
mod("polish_notation_converter.rs", "other", ("src/polish_notation_converter.rs",), "逐位",
    "**是活代码**：唯一调用者 0x4edd1f（在 MathFunction::new 0x4ed670 内），另有两条自递归。分词产物是**中缀**且含 tag 18/19 括号；括号/优先级/补 CONST 0 全在这里=> docs/03 §13「解析器自己就做分词」只对了一半。原版正则 77B、优先级表 18 项已逐字节拿到（docs/16 §33）。"
    "**2026-11-26 升档（T13）**：本模块唯一可归属函数 0x4f5450 `reorder_recursive` 在 unicorn 下对真二进制跑 18 组输入 **18/18 逐位相同**（正对照：求值器 20/20、采样器 4/4、解析器 tag 流 14/14；±1 ulp 变异必报差异）⇒ 观测→逐位，见 `docs/06` §64 与 `fixtures/bit_exact_sweep.tsv`")
mod("entity_drawer.rs", "other", ("src/entity_drawer.rs",), "观测",
    "10 个函数全解：draw/ready/process/update_sprite/update_name_label/draw_offset_triangle/get_entity_{hat,name_label,face,skin}。**士兵/靶子不是图元而是子节点**——3 个 Sprite2D"
    "（skin/face/hat 贴图）+ 1 个 RichTextLabel（res://name_label.tscn），懒建缓存于 4 张 HashMap，"
    "由 process 每帧摆位置；draw() 只画准星（3 弧 + 3 箭头）。docs/16 §24")
mod("effect_drawer.rs", "other", ("src/effect_drawer.rs",), "观测",
    "7 个 pub fn / 36 常量：draw_circle×4 + polygon×1 + line×2 + 一次 draw_set_transform，半径 0.05/0.06；"
    "tick 制 60 tick = 1 秒，物理子步 1/600 s，相位推进 600 tick；与 client/ 有 10 处出入（docs/16 §21）")
mod("axis_drawer.rs", "other", ("src/axis_drawer.rs",), "观测",
    "3 个 pub fn / 24 常量：四档轴模式与全部数值逐条登记（docs/16 §21）")
mod("obstacle_drawer.rs", "other", ("src/obstacle_drawer.rs",), "观测",
    "2 函数（update + 落在 GraphwarPlane::init 的那条）。**脏矩形由 graphwar_plane 决定并持有**；ObstacleDrawer::update 连矩形参数都没有，只把障碍光栅化成 Image -> ImageTexture -> Sprite2D，每帧被 GraphwarPlane::process 调 2 次（docs/16 §36）")
mod("function_drawer.rs", "other", ("src/function_drawer.rs",), "观测",
    "5 方法（x86 落 4，唯一锚 0x498c70 = update_function）。**自己不画图元**：持 HashMap<i32(player id), (Gd<Line2D>, 已写入点数)>，GraphwarPlane::init 建 3 个并 add_child；update_function 仅在 new_len > 已写入数时，对 points[written..new_len] 逐点 Line2D::add_point（16B/点，两个 f64）=> **增量追加，不是整条重画**；变短时一点不加（docs/16 §36）")
mod("button_manager.rs", "other", ("src/button_manager.rs",), "观测",
    "3 函数（9936 B 的 new + 两份 find_buttons 单态化）；地址只钉住 0x3a2e10（docs/16 §36）")
mod("function_button_manager.rs", "other", ("src/function_button_manager.rs",), "观测",
    "3 函数：函数输入键盘的**按键分发器**——按钮把自带 &str 交进来，闭包分发给 8 个 GraphwarFunctionInput 方法；按钮文本来自场景（Rust 里 0 个函数名字面量）（docs/16 §36）")
mod("function_history_recorder.rs", "other", ("src/function_history_recorder.rs",), "观测",
    "3 函数：**无上限、无预留、不去重**（推送就是 Vec::push）；上下键在两份二进制里都没有落点（ui_up/ui_down 各 0 处、无箭头键码）=> **只写不读，消费者未知**（docs/16 §36）")
mod("player_showcase.rs", "other", ("src/player_showcase.rs",), "观测",
    "10 个函数全解：ready 0x4e6480 / on_text_change 0x4efd70 / process 0x4e68f0 / draw 0x4e5400 + 6 个 gdext 生成物。**三次 draw_texture_rect_full**（皮肤→脸→帽），与颜色/段位无关；读根节点 +0x16a8 的 GraphwarPlayerConfig（名字+0x00/皮肤+0x60/脸+0x78/帽+0x90/变更计数+0xa8），process 轮询计数才重画（docs/16 §25）")
mod("server_broker_request_connection.rs", "other", ("src/server_broker_request_connection.rs",), "观测",
    "1/1；process 的 x86 0x3e5a80 是推断（调用指纹+相邻性，非唯一锚）（docs/09 §12）")
mod("backtracer.rs", "other", ("src/backtracer.rs",), "观测",
    "panic 钩子写 user://crash_log.txt；两侧都**没有** Backtrace::capture（名字误导）（docs/16 §33）")
mod("function_token.rs", "other", ("src/function_token.rs",), "观测",
    "16B token {tag,value} + 字符串表；tag 20=无效、`(`=18 `)`=19、**ˆ(U+02C6) 也是 ^**（新发现）（docs/16 §33）")
mod("graphwar_event_queue.rs", "graphwar", ("src/graphwar_event_queue.rs",), "观测",
    "**不是 manager**：GraphwarGameClient 以 Rc<RefCell<>> 持有（new+0xcdc），其余 manager 拿 Weak；add_event 有 200 余个直接调用点 => 全游戏发件箱。反常：send_buffer 只 1 个直接调用者（docs/16 §33）")
mod("graphwar_game_info.rs", "graphwar", ("src/graphwar_game_info.rs",), "观测",
    "GameInfo 结构体（9 字段）在此模块，**不在** graphwar_event.rs；我方只做 pub use 复用（docs/16 §33）")
mod("graphwar_player_config.rs", "graphwar", ("src/graphwar_player_config.rs",), "观测",
    "PlayerShowcase 读的 config（名字+0x00/皮肤+0x60/脸+0x78/帽+0x90/变更计数+0xa8）；存档分隔符未解（docs/16 §33）")
mod("graphwar_replay_reader.rs", "graphwar", ("src/graphwar_replay_reader.rs",), "观测",
    "2/2：文件归它（FileAccess::open + get_buffer + GzDecoder + StrSearcher 逐行 -> Vec<EventInfo>，被调者里**零时间函数**）；时钟归 replay_player（tick += dt*60*speed）（docs/09 §12）")
mod("graphwar_stream_buffer.rs", "graphwar", ("src/graphwar_stream_buffer.rs",), "观测",
    "2/2：**不是 TCP 拆包队列**——内部无字节队列，靠 TcpStream::peek 前瞻：写侧 u16 大端(len)+两段 write_all（限 0xffff）；读侧 peek 2 -> 校验 <=0x1000 -> peek(len+2) 不足即 Ok(None) **一个字节都不消费** -> read_exact 整帧取走。顺带结掉 docs/09 §11.7#4：0x…4a 是「整帧未到」，不是 EOF（docs/09 §12）")
mod("time_probe.rs", "other", (), "未做",
    "**T112 新发现：整模块缺行，`FUNC = 0` 且 `OBJECT = 0`** —— 该模块在 Android 2.0.60 里只剩一条泛型实例化"
    "`core::ptr::drop_in_place<graphwarrust::time_probe::TimeProbe>`（即 `TimeProbe` 是会被 drop 的值类型）。"
    "`docs/02` §18.3 在 2024-07-30 版上记的同样是「0 函数 / 0 方法，只在 impl / 泛型实参里露面」⇒ 两版一致。"
    "我方没有 `src/time_probe.rs` ⇒ 未做；缺的证据：`TimeProbe` 的字段与用途（既无 Location、也无自有符号，"
    "静态侧只能拿到类型名）。判为**模块存在、但要还原的内容未知**（docs/06 §157）")
mod("weak_caller.rs", "other", ("src/weak_caller.rs",), "观测",
    "40 个符号定位：x86 目标 39/39 个单态化（23 个 call + 16 个 call_0，两块各自无空洞）+ 第 40 个 "
    "0x3d4fa0 查明是内联了 call 的 GraphwarGameRecorder 方法。结构 = Weak<RefCell<T>>，一个类型参数、8 字节、"
    "self 按值消费（尾部自减弱计数，调用方必须 clone）。upgrade()==None => 静默跳过返回 ()；"
    "唯一 panic 是 RefCell 已被可变借出。是 call_0<F> / call<A,F> **两个**泛型 x 26 个类型，不是回调派发器。"
    "**旧口径全部作废**：scope=dep / 「1 个泛型 27 个符号」/ 「40 个函数」/ 「gdext 胶水」（docs/02 §34）")

SCOPE_ORDER = {"graphwar": 0, "other": 1, "dep": 2}
STATUS_ORDER = ["逐位", "观测", "推断", "未做", "未知"]


# ---------------------------------------------------------------------------
# 第五步：手工抽查（写进文档，也要能被 --check 验证没漂）
# 每项：(模块, 期望归属函数数, 核对方法) —— 期望值是我**手数**出来的，
# 不是脚本算的；脚本算出来对不上就直接失败。
# ---------------------------------------------------------------------------
SPOT_CHECKS = [
    ("graphwar_plane.rs", 10,
     "docs/02 §11 独立写下的 10 个虚地址（0x4b2260 … 0x4be3e0）与本脚本算出的集合**逐个相同**；"
     "objdump 复核 0x4be3e0 体内 0x4be510 处是 `lea 0x4a0419(%rip),%rdx # 95e930`，"
     "该槽即 Location{len=21=len(\"src/graphwar_plane.rs\"), line=174}"),
    ("graphwar_life_manager.rs", 9,
     "docs/02 §13.1 那张独立写的表正好列了 9 个虚地址，与本脚本算出的集合**逐个相同**；"
     "objdump 复核 0x454950 体内 0x454ada 是 `lea 0x501c7f(%rip),%r8 # 956760`（life_manager.rs:114）"),
    ("graphwar_turn_manager.rs", 14,
     "docs/02 §13.3 正文写「14 个函数」但只列了 13 个地址；本脚本算出的 14 个里多出 0x4638f0，"
     "objdump 确认 0x4638f0 是 `push rbp/r15/r14/r13/r12/rbx; sub rsp,0x68` 的正规 prologue，"
     "且是 Ghidra 函数表的精确入口 ⇒ 正文的 14 是对的，表的 13 漏列"),
]


# ---------------------------------------------------------------------------
# 第六步：我们自造的、原版没有的东西（D2）
# ---------------------------------------------------------------------------
SELF_MADE = [
    ("src/_scaffold/api.rs", "内核 → GDScript/JSON 的对外 API；原版把这件事交给 Godot 模块反射，没有这一层"),
    ("src/_scaffold/capi.rs", "C ABI，纯为把我们自己的实现暴露出去做对拍；原版无此物"),
    ("src/_scaffold/replaydiff.rs", "回放**消息级**对拍器；原版没有这种工具模块"),
    ("src/_scaffold/diffconv.rs", "上一条的收敛层；同上"),
    ("src/_scaffold/replay_inflate.rs", "手写 DEFLATE/gzip；原版用的是第三方 crate，不是游戏模块"),
    ("src/_scaffold/rng.rs", "ChaCha12 的重写；原版是 rand_chacha crate，不是游戏模块"),
    ("src/_scaffold/gdext.rs", "GDExtension 入口脚手架（原版的入口在 Godot 模块侧，不在这 78 个模块里）"),
    ("src/_scaffold/gdext_class.rs", "同上：classdb_register_extension_class4 的封装"),
    ("src/_scaffold/broker.rs", "**我们自己的一台 broker 服务端**；原版只有连 broker 的客户端，没有服务端"),
    ("src/_scaffold/protocol.rs", "报文编解码装配层；原版没有同名模块，编解码分散在各 handler 里"),
    ("src/_scaffold/ai_explain.rs", "AI 决策链解释器（工具）；不引入新行为，只把已对过地址的零件按原版顺序跑一遍"),
    ("client/", "整个 Godot 客户端是照 .tscn 与截图重写的仿制品，**不是**原版源码（AGENTS §0.4）"),
]


# ---------------------------------------------------------------------------
# 汇总
# ---------------------------------------------------------------------------
def build():
    if not os.path.exists(SO):
        raise SystemExit("coverage_matrix: 找不到原版二进制 %s（本仓库不入库，见 README）" % SO)
    if not os.path.exists(FUNCSTARTS):
        raise SystemExit("coverage_matrix: 找不到 %s（由 analysis/build_index.py 生成）" % FUNCSTARTS)

    E = Elf(SO)
    lits = find_literals(E)
    locs = resolve_locations(E, lits)
    funcstarts = load_funcstarts(FUNCSTARTS)
    refs = sweep_references(E, locs)
    per_func, contested = attribute(funcstarts, locs, refs)

    files = src_files()
    loc_lit = count_literals(E)          # 模块 -> "src/<模块>.rs" 字面量次数（0 = 盲区）
    fn_counts = {f: count_fns(os.path.join(ROOT, f)) for f in files}
    tree_sha = src_tree_sha256(files)

    # 每个模块的归属函数集合
    by_module = {}
    for fn, (module, _n) in per_func.items():
        by_module.setdefault(module, []).append(fn)
    contested_by_module = {}
    for fn in contested:
        contested_by_module.setdefault(per_func[fn][0], []).append(fn)
    anchors_by_module = {}
    for slot, (module, _l, _c) in locs.items():
        anchors_by_module[module] = anchors_by_module.get(module, 0) + 1

    # 自校验（文档 §1.4 写的那条）：解析器必须落 math_function.rs，求值器必须确实没有锚。
    # 换二进制时这两条会先炸，比让人读着旧散文猜要可靠。
    assert 0x4ED670 in by_module.get("math_function.rs", []), (
        "自校验失败：解析器 0x4ed670 没有落在 math_function.rs")
    assert 0x4EB9D0 not in per_func, (
        "自校验失败：求值器 0x4eb9d0 居然有 Location 锚了 —— 文档 §1.4 需要改写")

    rows = []
    for module in sorted(M, key=lambda m: (SCOPE_ORDER[M[m]["scope"]], m)):
        rec = M[module]
        if rec["status"] in ("未做", "未知"):
            assert not rec["ours"], "--check 不变式：%s 标了 %s 却有 ours_file" % (module, rec["status"])
        else:
            assert rec["ours"], "--check 不变式：%s 标了 %s 却没有 ours_file" % (module, rec["status"])
        rows.append(dict(
            module=module,
            scope=rec["scope"],
            orig_funcs=len(by_module.get(module, [])),
            contested=len(contested_by_module.get(module, [])),
            anchors=anchors_by_module.get(module, 0),
            ours_file="+".join(rec["ours"]),
            ours_funcs=sum(fn_counts.get(f, 0) for f in rec["ours"]),
            status=rec["status"],
            evidence=rec["evidence"],
            sym_funcs=(rec["sym_funcs"] if rec["sym_funcs"] is not None else ""),
        ))

    # ---- 抽查断言 ----
    spot_rows = []
    for module, expect, how in SPOT_CHECKS:
        got = len(by_module.get(module, []))
        spot_rows.append((module, expect, got, how))
        assert got == expect, "抽查失败：%s 期望 %d 个归属函数，脚本算出 %d" % (module, expect, got)

    # ---- 总账 ----
    def tally(scope):
        sel = [r for r in rows if r["scope"] == scope]
        attributed = sum(s["orig_funcs"] for s in sel)
        t = {st: 0 for st in STATUS_ORDER}
        for r in sel:
            t[r["status"]] += r["orig_funcs"]
        implemented = t["逐位"] + t["观测"] + t["推断"]
        return dict(modules=len(sel), attributed=attributed, by_status=t,
                    implemented=implemented,
                    unknown_modules=sum(1 for r in sel if r["status"] == "未知"),
                    # T112：`by_status["未做"]` 累的是**函数数**（新补的模块 orig_funcs=0 ⇒ 它仍是 0），
                    # 模块级的"没实现"必须单独数，否则"未做 0"会被读成"模块都做完了"。
                    pending_modules=sum(1 for r in sel if r["status"] in ("未做", "未知")),
                    done_modules=sum(1 for r in sel if r["status"] in ("逐位", "观测", "推断")))

    totals = {s: tally(s) for s in ("graphwar", "other")}
    grand = dict(
        modules=sum(t["modules"] for t in totals.values()),
        graphwar=totals["graphwar"], other=totals["other"],
        attributed=sum(t["attributed"] for t in totals.values()),
        implemented=sum(t["implemented"] for t in totals.values()),
        by_status={st: sum(t["by_status"][st] for t in totals.values()) for st in STATUS_ORDER},
        unknown_modules=sum(t["unknown_modules"] for t in totals.values()),
        pending_modules=sum(t["pending_modules"] for t in totals.values()),
        done_modules=sum(t["done_modules"] for t in totals.values()),
        sym_funcs=sum((r["sym_funcs"] or 0) for r in rows if isinstance(r["sym_funcs"], int)),
    )

    meta = dict(
        tool_version=TOOL_VERSION,
        binary_sha256=E.sha256,
        funcstarts_sha256=hashlib.sha256(open(FUNCSTARTS, "rb").read()).hexdigest(),
        src_tree_sha256=tree_sha,
        funcstarts_n=len(funcstarts),
        text_lo=funcstarts[0],
        text_hi=funcstarts[-1],
        src_modules=len(lits),
        loc_structs=len(locs),
        loc_files=len({v[0] for v in locs.values()}),
        attributed_funcs=len(per_func),
        contested_funcs=len(contested),
        src_files=len(files),
        src_fns=sum(fn_counts.values()),
        module_denom_n=len(M),
    )
    return rows, spot_rows, totals, grand, meta, by_module, fn_counts, files, loc_lit, E



# ---------------------------------------------------------------------------
# 模块分母：三方差集表 + 正/负对照（T112）
#
# 这张表回答的是「**分母**对不对」，不是「实现了多少」：
#   Android 有没有（未 strip 符号表）/ 矩阵有没有 / src/ 顶层有没有 / 判定
# 生成物：`fixtures/module_denom_audit.tsv`（`--check` 会比对，口径见
# `fixtures/module_denom_plan.txt`）。
# ---------------------------------------------------------------------------
AUDIT_COLUMNS = ["module", "android", "matrix", "src_top", "verdict",
                 "android_j1", "loc_lit", "orig_funcs", "sym_funcs", "note"]

# T112 本轮**改了矩阵**的模块（判定列要能说清"这条是这轮新补的"）
T112_ADDED_IMPLEMENTED = {"graphwar_ban_list.rs", "graphwar_version.rs"}
T112_ADDED_MISSING = {"graphwar_id_maker.rs", "time_probe.rs"}
T112_SCOPE_FIXED = {"graphwar_texture_loader.rs"}

# 负对照：这些**不许**出现在模块集合里（crate 名 / 类型名 / 静态项名 / 外部 crate / 闭包）
NEGATIVE_CONTROLS = ["graphwarrust", "GraphwarTextureLoader", "GraphwarBanList",
                     "GraphwarIdMaker", "TimeProbe", "CURRENT_ID", "core", "alloc",
                     "std", "libc", "gdext", "{{closure}}", "graphwar"]


def _doc024_modules():
    """`docs/02` §18.3 A 表的 78 个模块名（来自**另一版、另一架构**的符号表）。

    只用于 PC5（跨版本一致性）。文档结构一变就解析不出来 ⇒ 返回 [] 并让对照 SKIP，
    绝不因为解析失败而"通过"。
    """
    path = os.path.join(ROOT, "docs", "02-module-map.md")
    try:
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
    except OSError:
        return []
    head = "#### A. 权威模块清单"
    if head not in text:
        return []
    body = text[text.index(head):]
    tail = body.find("**没有一个自己的函数**")
    if tail > 0:
        body = body[:tail]
    out = []
    for line in body.splitlines():
        m = re.match(r"\|\s*`([a-z0-9_]+)`\s*\|", line)
        if m:
            out.append(m.group(1) + ".rs")
    return out


def module_denominator_audit(rows, loc_lit, android=None, legacy=None):
    """→ (tsv 文本, 摘要 dict)。`android is None` ⇒ 符号表不在本机，返回 (None, {...SKIP})。"""
    if android is None:
        return None, dict(available=False)
    matrix = {r["module"]: r for r in rows}
    src_dir = os.path.join(ROOT, "src")
    controls = []           # [(编号, 描述, PASS/SKIP, 证据)]
    notes = {}              # module -> 备注（其它观察，不影响判定）
    if os.path.exists(os.path.join(src_dir, "graphwar_event.rs")) \
            and "src/graphwar_event.rs" not in (matrix.get("graphwar_event.rs", {}).get("ours_file") or ""):
        notes["graphwar_event.rs"] = ("⚠ `src/graphwar_event.rs` 存在，但矩阵的 ours_file 只指 "
                                      "`src/events.rs` ⇒ 该模块的内容分在两个文件里（本轮**只登记、未改**）")

    # ---- 正对照 ----
    def pc(tag, ok, ev):
        controls.append((tag, ok, ev))

    pc("PC1", android.get("graphwar_ban_list.rs", {}).get("func") == 2,
       "Android `graphwar_ban_list` FUNC=2（期望 2）")
    pc("PC2", android.get("graphwar_version.rs", {}).get("func") == 1,
       "Android `graphwar_version` FUNC=1（期望 1）")
    tl = android.get("graphwar_texture_loader.rs", {})
    pc("PC3", ("graphwar_texture_loader.rs" in android
               and matrix.get("graphwar_texture_loader.rs", {}).get("scope") == "graphwar"),
       "Android 有 texture_loader（FUNC=%s）且矩阵 scope 已修为 graphwar" % tl.get("func"))
    missing_from_android = sorted(set(matrix) - set(android))
    pc("PC4", not missing_from_android,
       "矩阵 − Android = %s（期望空）" % (missing_from_android or "∅"))
    d24 = _doc024_modules()
    if d24:
        d24_missing = sorted(set(d24) - set(android))
        pc("PC5", not d24_missing,
           "docs/02 §18.3 的 %d 个名字 ⊂ Android（差集 %s）" % (len(d24), d24_missing or "∅"))
    else:
        controls.append(("PC5", None, "docs/02 §18.3 表解析不出来 ⇒ SKIP"))
    idm = android.get("graphwar_id_maker.rs", {})
    pc("PC6", (idm.get("func") == 0 and idm.get("obj", 0) >= 1 and idm.get("j1") is True),
       "`graphwar_id_maker` FUNC=0 / OBJECT=%s / J1=%s ⇒「只按 FUNC 数」会漏它（T109 的 65 而非 66）"
       % (idm.get("obj"), idm.get("j1")))

    # ---- 负对照 ----
    bad = sorted({n for n in NEGATIVE_CONTROLS if n in android})
    pc("NC1-5", not bad, "负对照名单里没有一个进了模块集合（%s）；名单 %d 个"
       % (bad or "无越界", len(NEGATIVE_CONTROLS)))
    if legacy is None:
        controls.append(("NC6", None, "legacy-only 解名结果不可用 ⇒ SKIP"))
    else:
        diff = sorted(set(legacy) ^ set(android))
        pc("NC6", not diff, "只解 legacy 与 legacy+v0 的模块集合一致（对称差 %s）" % (diff or "∅"))

    # ---- 表体 ----
    lines = []
    for module in sorted(android, key=lambda m: (SCOPE_ORDER[matrix[m]["scope"]]
                                                 if m in matrix else 1, m)):
        rec = matrix.get(module)
        src_top = os.path.exists(os.path.join(src_dir, module))
        a = android[module]
        j1 = ("FUNC=%d,OBJECT=%d" % (a["func"], a["obj"])) if a["j1"] else "-（只在 impl/泛型实参里露面）"
        if rec is None:
            verdict = "⚠ 原版有、矩阵缺行"
        elif module in T112_ADDED_IMPLEMENTED:
            verdict = "T112 补行：**已实现**（T109 落码，观测）"
        elif module in T112_ADDED_MISSING:
            verdict = "T112 补行：**未实现**（我方无文件，未做）"
        elif module in T112_SCOPE_FIXED:
            verdict = "T112 改 scope：other→graphwar"
        elif rec["status"] in ("未做", "未知"):
            verdict = "一致（模块存在、我方无文件，%s）" % rec["status"]
        else:
            verdict = "一致"
        note = notes.get(module, "")
        lines.append((module, "yes", (rec["scope"] if rec else "-"), "yes" if src_top else "no",
                      verdict, j1, loc_lit.get(module, 0),
                      (rec["orig_funcs"] if rec else ""),
                      (rec["sym_funcs"] if rec else ""), note))
    for module in sorted(set(matrix) - set(android)):
        rec = matrix[module]
        lines.append((module, "no", rec["scope"], "yes" if os.path.exists(
            os.path.join(src_dir, module)) else "no", "⚠ 矩阵有、原版该版无",
            "-", loc_lit.get(module, 0), rec["orig_funcs"], rec["sym_funcs"], ""))

    summary = dict(available=True, controls=controls, n_android=len(android),
                   n_matrix=len(matrix), n_src_top=sum(1 for m in matrix
                                                       if os.path.exists(os.path.join(src_dir, m))),
                   missing=sorted(set(android) - set(matrix)),
                   extra=sorted(set(matrix) - set(android)),
                   j1=sum(1 for v in android.values() if v["j1"]),
                   j1_missing=sorted(m for m, v in android.items() if not v["j1"]),
                   j1_prime=len(android),
                   sym_funcs=sum(v["func"] for v in android.values()),
                   loc_blind=sorted(m for m in matrix if loc_lit.get(m, 0) == 0),
                   n_union=None)
    if d24:
        summary["n_union"] = len(set(android) | set(d24))

    head = [
        "# module_denom_audit.tsv —— 模块分母三方差集表（T112；模块 78 → %d）" % len(android),
        "# 口径/判据/正负对照：fixtures/module_denom_plan.txt（§A 判据、§B 正对照、§C 负对照）",
        "# 主证据：%s（arm64 2.0.60，`.symtab` 未 strip）" % os.path.relpath(ANDROID_SO, ROOT),
        "# 旁证：docs/02 §18.3 的 78 个名字（2024-07-30 x86-64 未 strip 版；本机**没有**那份二进制，只能用已入库的表）",
        "# 列：module | android=J1'（任意路径出现）| matrix=矩阵 scope | src_top=src/<模块> 顶层同名文件"
        " | verdict=判定 | android_j1=J1 定义路径的 FUNC/OBJECT 条数"
        " | loc_lit=Linux .so alloc 段里 `src/<模块>.rs` 字面量次数（0 = 旧 Location 口径的盲区）"
        " | orig_funcs=矩阵的 Location 可归属函数数 | sym_funcs=Android FUNC 条数 | note=其它观察",
        "# 三读：N_2.0.60(J1')=%d；N_2.0.60(J1)=%d（差 = %s）；N_union(J1')=%s"
        % (len(android), summary["j1"],
           sorted(m for m, v in android.items() if not v["j1"]) or "∅",
           summary["n_union"] if summary["n_union"] is not None else "SKIP"),
        "# 旧 Location 口径的盲区：矩阵里 %d 个模块的 `src/<模块>.rs` 字面量在 Linux .so 里 **0 次**"
        " ⇒ 它们当初能被列出来全靠人工补行，不是靠扫" % len(summary["loc_blind"]),
        "# 正/负对照（先跑，全过才算数）：",
    ]
    for tag, ok, ev in controls:
        state = "SKIP" if ok is None else ("PASS" if ok else "FAIL")
        head.append("#   %-6s %-4s %s" % (tag, state, ev))
    text = "\n".join(head) + "\n" + "\t".join(AUDIT_COLUMNS) + "\n"
    text += "".join("\t".join(str(x) for x in row) + "\n" for row in lines)
    return text, summary


def _audit_digest():
    """从**已入库**的 `fixtures/module_denom_audit.tsv` 头部还原 `docs/149` 要用的摘要。

    为什么不在渲染时直接用活的符号表：`docs/149` 必须**机器无关**（换台没有 Android `.so` 的
    机器跑 `--check` 也得逐字节一致）。所以渲染来源是**入库的表**，而"表是否新鲜"由 `--check`
    单独去比（符号表在时比对整份文件，不在时 SKIP 并警告）。
    """
    try:
        with open(AUDIT_PATH, encoding="utf-8") as fh:
            head = [line for line in fh if line.startswith("#")]
    except OSError:
        return None
    blob = "".join(head)
    m = re.search(r"N_2\.0\.60\(J1'\)=(\d+)；N_2\.0\.60\(J1\)=(\d+)（差 = (\[[^\]]*\])）；"
                  r"N_union\(J1'\)=(\S+)", blob)
    if not m:
        return None
    j1_missing = re.findall(r"'([^']*)'", m.group(3))     # 只含字符串的列表字面量，不用 eval
    blind = re.search(r"矩阵里 (\d+) 个模块", blob)
    controls = []
    for line in head:
        c = re.match(r"#\s{1,3}(PC\d(?:-\d)?|NC\d(?:-\d)?)\s+(PASS|FAIL|SKIP)\s+(.*)$", line)
        if c:
            controls.append((c.group(1), {"PASS": True, "FAIL": False, "SKIP": None}[c.group(2)],
                             c.group(3).strip()))
    if not controls:
        return None
    n_android = int(m.group(1))
    return dict(available=True, from_fixture=True,
                n_android=n_android, n_matrix=n_android, j1=int(m.group(2)),
                j1_missing=list(j1_missing),
                n_union=(int(m.group(4)) if m.group(4).isdigit() else None),
                loc_blind=[""] * (int(blind.group(1)) if blind else 0),
                controls=controls)

# ---------------------------------------------------------------------------
# 渲染
# ---------------------------------------------------------------------------
TSV_HEADER = ("# coverage-matrix.tsv — 由 tools/coverage_matrix.py 生成，**不要手改**\n"
              "# 重算：python3 tools/coverage_matrix.py    只校验：--check\n"
              "# 模块清单来源：Android 2.0.60 未 strip `.symtab` 的符号路径（**不是** Location）\n"
              "#   T112 修正：旧口径按 Location 建清单，.so 里没有 `src/<模块>.rs` 字面量的模块"
              "连行都不会有（实测 10 个）\n"
              "# orig_funcs：能按 panic::Location 硬归属的函数数（可度量下限，不是模块规模）\n"
              "# contested：该函数同时引用了别的模块 Location（已按锚点数压平，不重复计数）\n"
              "# anchors：该模块名下的 panic::Location 结构体个数（引用点密度）\n"
              "# ours_funcs：ours_file 里 fn 的条数（注释与字符串已剔除）\n"
              "# sym_funcs：Android 符号表里该模块的 FUNC 条数（**规模参考列**，不进 orig_funcs、"
              "不进 527）\n")

# `sym_funcs` **只许追加到末尾**：tools/audit_src.py / bitexact_classify.py / boundary_audit.py /
# global_triage.py / file_structure.py / function_coverage.py / bitexact_plan.py 都按列下标读 0–8。
TSV_COLUMNS = ["module", "scope", "orig_funcs", "contested", "anchors",
               "ours_file", "ours_funcs", "status", "evidence", "sym_funcs"]


def render_tsv(rows, meta):
    out = [TSV_HEADER]
    for k in ("tool_version", "binary_sha256", "funcstarts_sha256", "src_tree_sha256",
              "funcstarts_n", "src_modules", "loc_structs", "attributed_funcs", "contested_funcs",
              "src_files", "src_fns", "module_denom_n"):
        out.append("# %s: %s\n" % (k, meta[k]))
    out.append("\t".join(TSV_COLUMNS) + "\n")
    for r in rows:
        out.append("\t".join(str(r[c]) for c in TSV_COLUMNS) + "\n")
    return "".join(out)


def fmt_pct(a, b):
    return "%.1f%%" % (100.0 * a / b) if b else "—"


def render_md(rows, spot_rows, totals, grand, meta, md_summary=None):
    g, o = totals["graphwar"], totals["other"]
    L = []

    def A(line):
        # 每行一个换行；显式写了 "\n" 的（段落分隔）保持不变
        L.append(line if line.endswith("\n") else line + "\n")
    A("# 149 · 覆盖率矩阵：按**原版模块**算的进度尺\n")
    A("> 本文件由 `tools/coverage_matrix.py` 生成，**不要手改**。改口径请改脚本，然后重跑。\n")
    A("> `python3 tools/coverage_matrix.py` 重新生成；`--check` 只校验不写，不一致就退出码 1。\n")
    A("")
    A("## 0. 一句话总账\n")
    A("```")
    A(grand_line(grand))
    A("```")
    A("")
    A("拆成两个口径（下面的数字全部由脚本算出，重跑即复现）：\n")
    A("| 口径 | 模块数 | 可归属函数 | 已实现 | 逐位 | 观测 | 推断 | 未做 | 未知模块 |")
    A("|---|---|---|---|---|---|---|---|---|")
    other_prefixed = sum(1 for r in rows if r["scope"] == "other" and r["module"].startswith("graphwar_"))
    for name, t in (("%d 个有同名 `graphwar_*.rs` 文件的模块（`scope=graphwar` 共 %d 个）" % (graphwar_prefixed(), g["modules"]), g),
                    ("%d 个 `scope=other` 的模块（%d 个不带 `graphwar_` 前缀%s）"
                     % (o["modules"], o["modules"] - other_prefixed,
                        ("、%d 个带前缀" % other_prefixed) if other_prefixed
                        else "；带 `graphwar_` 前缀的已于 T112 全部归回 graphwar"), o)):
        A("| %s | %d | %d | %d | %d | %d | %d | %d | %d |"
          % (name, t["modules"], t["attributed"], t["implemented"],
             t["by_status"]["逐位"], t["by_status"]["观测"], t["by_status"]["推断"],
             t["by_status"]["未做"], t["unknown_modules"]))
    A("")
    A("`weak_caller.rs` 在上表里算进 **scope=`other`** 那一档（39 个 `orig_funcs`，TSV 里单列一行）。"
      "**审计 2026-11-08 更正**：旧文写「40 个归属函数 …… **不计入**任何合计」，两处都不对——"
      "① 表值是 **39**（`docs/coverage-matrix.tsv` 的 `orig_funcs` 列，另 1 个符号查明是内联进 "
      "`GraphwarGameRecorder` 的方法，不是本模块的函数）；② 它**确实**计入 `other` 的 97 与总数的 527"
      "（430 + 97 = 527）。它也不是第三方依赖：`docs/02` §34 已按符号表判它是 crate 内模块"
      "（`graphwarrust::weak_caller`），只是文件名不带 `graphwar_` 前缀。\n")
    A("### 0.1 这个分母有多大、有多可信\n")
    A("| 事实 | 值 |")
    A("|---|---|")
    A("| 二进制函数边界（Ghidra 全量 / 落在 `.text`、本机函数表里有） | 13503 / **%d** |" % meta["funcstarts_n"])
    A("| `panic::Location` 结构体（`len` 字段自校验后） | **%d** |" % meta["loc_structs"])
    A("| 其中涉及的 `src/*.rs` 文件 | %d |" % meta["loc_files"])
    A("| 至少有一个 Location 锚的函数（=%d 里） | **%d**（%s） |"
      % (meta["funcstarts_n"], meta["attributed_funcs"],
         fmt_pct(meta["attributed_funcs"], meta["funcstarts_n"])))
    A("| 其中同时引用两个以上模块 Location 的（已压平，不重复计数） | %d |" % meta["contested_funcs"])
    A("")
    A("**归属率只有 %s，而且偏得厉害 —— 这是本表最重要的一条警告。**\n"
      % fmt_pct(meta["attributed_funcs"], meta["funcstarts_n"]))
    A("只有带 panic 点（`unwrap`/`expect`/越界/`panic!`）的函数才留锚。纯计算函数没有："
      "求值器 `0x4eb9d0` 和采样器 `0x3c2530` 的**整个函数体里一个 `Location` 引用都没有**，"
      "所以它们根本归属不出来。具体后果是 `graphwar_path_calculator.rs` 的 `orig_funcs` 只有 1，"
      "而采样器就装在那个模块里。\n")
    A("⇒ **`orig_funcs` 只能当「有多少函数能被硬证据钉到某个源文件」的下限，不能当模块规模。**"
      "写「这个模块有 N 个函数」是错的。\n")
    # ---- 0.2 模块分母（T112）----
    su = md_summary or {}
    A("### 0.2 模块分母：%d 个（2026-11-28 T112 从 78 修正）\n" % grand["modules"])
    if su.get("available"):
        A("**旧的 78 = 62 + 16 是错的**，错在**分母的建法**，不在实现：模块清单以前也是按 "
          "`panic::Location` 建的，而 Linux `.so` 里 `src/<模块>.rs` 字面量出现 **0 次**的模块"
          "（本表里就有 **%d 个**）**连一行都不会有** ⇒ 分母被系统性漏算。\n"
          % len(su.get("loc_blind") or []))
        A("现口径（预注册 `fixtures/module_denom_plan.txt`）：模块清单取**未 strip 的 Android 2.0.60 "
          "`.symtab` 的符号路径**（与逆向目标同版本），模块名 = 路径第 2 段。\n")
        A("| 读数 | 值 | 说明 |")
        A("|---|---|---|")
        A("| **N（总账用这个）** = N_2.0.60(J1') | **%d** | 符号路径**任意位置**出现 "
          "`graphwarrust::<m>::`（含 impl / 泛型实参） |" % su["n_matrix"])
        A("| N_2.0.60(J1) | %d | 更严：只在**定义路径**里出现的（差 %d 个：%s） |"
          % (su["j1"], su["n_android"] - su["j1"],
             "、".join("`%s`" % m for m in sorted(su.get("j1_missing") or [])) or "—"))
        A("| N_union(J1') | %s | 并上 `docs/02` §18.3 的 78 个名字（2024-07-30 x86-64 版）；"
          "两者并集仍是 %d ⇒ **没有「只有 2024 版才有」的模块被漏掉** |"
          % (su.get("n_union") if su.get("n_union") is not None else "SKIP", su["n_matrix"]))
        A("")
        A("拆开：**%d 个 `scope=graphwar` + %d 个 `scope=other`**。这轮改的 4 处：\n"
          % (g["modules"], o["modules"]))
        A("| 模块 | 本轮处理 | 原版证据 |")
        A("|---|---|---|")
        A("| `graphwar_ban_list.rs` | **补行**：已实现（T109 落码，观测） | Android `.symtab` FUNC=2 |")
        A("| `graphwar_version.rs` | **补行**：已实现（T109 落码，观测） | Android `.symtab` FUNC=1 |")
        A("| `graphwar_id_maker.rs` | **补行**：**未实现**（我方无文件） | Android 只剩 1 个 "
          "`LOCAL OBJECT`（`get_unique_id::CURRENT_ID`），`get_unique_id` 已被内联 ⇒ "
          "只数 FUNC 会漏（T109 数出 65 而非 66 就是这里） |")
        A("| `time_probe.rs` | **补行**：**未实现**（0 函数 / 0 方法） | Android 只剩 "
          "`core::ptr::drop_in_place<…TimeProbe>`；2024 版记录相同 |")
        A("| `graphwar_texture_loader.rs` | **改 scope**：`other` → `graphwar`（分类错，不是新模块） "
          "| 名字带 `graphwar_` 前缀 |")
        A("")
        A("⚠️ **诚实读法**：分母变大让**模块级完成度从 100 %% 掉到 %d/%d = %s** ——"
          " 但新增的 4 个里 **2 个是早就做完的**（`ban_list` / `version`，只是不在分母里，"
          "等于旧读数里白送了两个模块），**真正没做的是 %d 个**（`graphwar_id_maker` / `time_probe`）。"
          "别读成「突然多出 4 个模块的活」——真正要动手的只有这 2 个。\n"
          % (grand["done_modules"], grand["modules"],
             fmt_pct(grand["done_modules"], grand["modules"]), su["n_matrix"] and
             (su["n_matrix"] - grand["done_modules"])))
        A("`sym_funcs` 列合计 **%d**（Android 符号表 FUNC）—— 这是**另一条轴**，"
          "**不并入** 527（Location 可归属），两者不可混报。\n"
          % (grand.get("sym_funcs") or 0))
        bad = [c for c in su.get("controls", []) if c[1] is False]
        skip = [c for c in su.get("controls", []) if c[1] is None]
        A("正/负对照 %d 条：PASS %d / FAIL %d / SKIP %d（明细见 "
          "`fixtures/module_denom_audit.tsv` 头部；三方差集表也在那里）。\n"
          % (len(su.get("controls", [])),
             len(su.get("controls", [])) - len(bad) - len(skip), len(bad), len(skip)))
    else:
        A("⚠️ 未 strip 的 Android 2.0.60 符号表**不在本机** ⇒ 模块分母的自动对账本轮 **SKIP**"
          "（`fixtures/module_denom_audit.tsv` 也是按上次的符号表生成的，"
          "`--check` 会因此不核对模块集合）。设 `MODULE_DENOM_ANDROID=<path>` 再跑即可。\n")
    A("## 1. 方法（可复现）\n")
    A("### 1.1 原版这一半\n")
    A("1. 扫所有 alloc 段里 `src/<name>.rs` 字面量的虚拟地址；")
    A("2. 解析 `.rela.dyn` 的 `R_X86_64_RELATIVE`：`addend` 等于某个字面量地址时，"
      "该重定位的 `r_offset` 就是 `panic::Location` 结构体的 `file.ptr` 槽；")
    A("3. 自校验：`struct+8` 必须恰好等于字面量长度，`line/col` 必须是合理正整数；"
      "不满足就丢掉（去掉这步会把任意 `(ptr,len)` 对当成 Location）；")
    A("4. capstone 线性扫 `.text`，收集 rip-relative 与立即数引用；")
    A("5. 引用地址 → 用 `analysis/funcstarts_ghidra_va.pkl`（**Ghidra 边界**）二分到所属函数；"
      "`funcstarts.pkl` 那张 `.eh_frame` 的表是坏的，不能用（`docs/02` §12）。\n")
    A("**地址空间**：本表全部是 **ELF 虚地址**。Ghidra/REA 地址 = 虚地址 + `0x100000`，"
      "只能加一次。函数表本身已经是虚地址（`fs[0] = %#x`、`fs[-1] = %#x`，正好等于 `.text` 区间），"
      "四个已知入口 `0x4eb9d0`/`0x4ed670`/`0x3c2530`/`0x3c5f70` 在表里**精确命中** —— 这同时钉死了"
      "地址口径。\n" % (meta["text_lo"], meta["text_hi"]))
    A("### 1.2 我们这一半\n")
    A("`src/**/*.rs` 里 `fn` 的条数，先剔除注释、字符串与字符字面量再匹配（否则注释里的 `fn` 会被数进来）。"
      "**只有 `src/` 算实现**：`client/` 那套 GDScript 是仿制品（`AGENTS.md` §0.4），不计。\n")
    A("### 1.3 五档怎么判\n")
    A("| status | 判据 |")
    A("|---|---|")
    A("| `逐位` | 有 src/ 实现，且已与真二进制/真抓包**逐字节或逐位对拍通过** |")
    A("| `观测` | 有 src/ 实现，依据是反汇编/运行时/真机观测，但没有逐位对拍 |")
    A("| `推断` | 有 src/ 实现，依据是推断（evidence 里写缺什么） |")
    A("| `未做` | **没有** src/ 实现，但原版这块有地址级观测或文档描述 |")
    A("| `未知` | **没有**实现，且除了模块名没有任何证据 |")
    A("")
    A("`--check` 会验证不变式：`未做`/`未知` 的 `ours_file` 必须为空，其余三档必须非空。\n")
    A("### 1.4 自校验：那条「求值器/解析器都应落 math_function.rs」只成立一半\n")
    A("`AGENTS.md` 把「`0x4eb9d0`（求值器）/ `0x4ed670`（解析器）应落 `src/math_function.rs`」"
      "当成这套方法的自校验。实测（本脚本每次运行都会重算）：\n")
    A("| 函数 | 体内的 Location 引用 | 归属 |")
    A("|---|---|---|")
    A("| `0x4ed670`（解析器，0xa70 字节） | `0x4edf1f: lea -> 槽 0x95fec0` = "
      "`math_function.rs:75` | ✅ `math_function.rs` |")
    A("| `0x4eb9d0`（求值器，0x131 字节） | **一条都没有**（全函数只有 3 处 rip 引用，"
      "分别指向 `.rodata` 常量和两个 `jmp *GOT`） | ❌ 归属不出来 |")
    A("")
    A("`0x4eb9d0` 是自递归的求值器（`call 0x4eb9d0` 两次递归左右操作数），"
      "用 `bt` 位测试分派 tag，**整条路径没有 panic**，所以没有 `Location` 可抓。\n")
    A("⇒ 这条自校验**只能验解析器**。求值器要归属，得靠人工反编译或调用图推断。"
      "这不否定它属于 `math_function.rs`（`docs/03` 的函数语言证据链仍然成立），"
      "但它**不是**这套方法能自动得出的结论，本表也不假装能。\n")
    A("## 2. 逐模块矩阵\n")
    A("`ours_funcs` 是整个文件的 `fn` 条数，**不是**「这个模块实现了多少个函数」；"
      "`src/game.rs` 一个文件服务多个原版模块，所以那些行会重复出现同一个数。\n")
    for scope, title in (("graphwar", "2.1 %d 个有同名 `graphwar_*.rs` 文件的模块" % graphwar_prefixed()),
                         ("other", "2.2 非 `graphwar_` 前缀的游戏模块")):
        sel = [r for r in rows if r["scope"] == scope]
        A("### %s\n" % title)
        A("| module | orig_funcs | anchors | ours_file | ours_funcs | status |")
        A("|---|---|---|---|---|---|")
        for r in sel:
            A("| `%s` | %d | %d | %s | %d | %s |"
              % (r["module"], r["orig_funcs"], r["anchors"],
                 ("`%s`" % r["ours_file"]) if r["ours_file"] else "—",
                 r["ours_funcs"], r["status"]))
        A("")
    A("完整的 evidence（每行一句话的依据）在 `docs/coverage-matrix.tsv`，"
      "那张表是本脚本的唯一真源，本页的表格由它渲染。\n")
    A("## 3. 哪些模块连「它干什么」都不知道（问题 1）\n")
    unknown = [r for r in rows if r["status"] == "未知" and r["scope"] == "graphwar"]
    A("**%d 个有同名文件的 graphwar 模块里有 %d 个**属于这一类 —— 判据是：没有实现、"
      "且除模块名之外**没有任何**能指到地址/报文/真机的观测。\n"
      % (graphwar_prefixed(), len(unknown)))
    A("| module | orig_funcs | anchors | 现状 |")
    A("|---|---|---|---|")
    for r in unknown:
        A("| `%s` | %d | %d | 只有名字 |" % (r["module"], r["orig_funcs"], r["anchors"]))
    A("")
    zero_anchor = [r for r in rows if r["scope"] == "graphwar" and r["anchors"] == 0]
    A("另外还有 **%d 个模块一个 panic 锚都没有**（连「它有没有函数」都答不出来）：%s。"
      % (len(zero_anchor), "、".join("`%s`" % r["module"] for r in zero_anchor)))
    A("它们的 `orig_funcs` 是 0，**不代表模块为空**，只代表这个方法对它们完全失效。\n")
    other_unknown = [r["module"] for r in rows if r["status"] == "未知" and r["scope"] == "other"]
    A("非 graphwar 前缀里还有 %d 个未知模块（%s），它们的共同点是"
      "**连「是不是游戏代码」都只有名字可依**。\n"
      % (len(other_unknown), " / ".join("`%s`" % m for m in other_unknown)) if other_unknown
      else "非 graphwar 前缀里没有「未知」模块（未实现的模块见 §0.2 与 TSV 的 `status` 列）。\n")
    A("## 4. 我们自造的、原版没有的东西（问题 2）\n")
    A("这一栏**不算复原**，列出来是为了别把「能跑」当「还原了」。")
    A("判据：`src/` 里没有任何原版 `src/<file>.rs` 与它对应，或它实现的是原版明确没有的东西。\n")
    A("| 位置 | 为什么它不是复原 |")
    A("|---|---|")
    for path, why in SELF_MADE:
        A("| `%s` | %s |" % (path, why))
    A("")
    A("合计 **%d 处**。其中 `src/_scaffold/broker.rs` 和 `client/` 最容易误判：前者是我们自己写的"
      "服务端（原版只有连 broker 的客户端），后者整个是仿制品。\n" % len(SELF_MADE))
    A("## 5. 抽查：三个模块的函数数是手数出来的\n")
    A("`--check` 会重算并断言下面三个数没漂。\n")
    A("| module | 手数 | 脚本 | 怎么核的 |")
    A("|---|---|---|---|")
    for module, expect, got, how in spot_rows:
        A("| `%s` | %d | %d | %s |" % (module, expect, got, how))
    A("")
    A("三个抽查里两个（`graphwar_plane.rs` 10 个、`graphwar_life_manager.rs` 9 个）"
      "与 `docs/02` §11 / §13.1 **独立写下**的地址表**逐个相同**；"
      "第三个 `graphwar_turn_manager.rs` 正文写 14、表里只列 13，本脚本算出 14，"
      "多出的 `0x4638f0` 用 objdump 确认是函数入口。\n")
    A("## 6. 怎么读这把尺（以及它现在量不了什么）\n")
    A("1. **不要**把 `orig_funcs` 相加当「原版函数总数」。它是可归属下限，"
      "而且系统性漏掉所有纯计算模块（求值器、采样器就是现成的反例）。")
    A("2. **模块级分母（`scope=graphwar` 共 %d 个，其中 %d 个有同名 `src/graphwar_*.rs`；"
      "总数 %d）现在**不再**是「Location 下限」，而是**未 strip 符号表**的读数（§0.2）："
      "符号路径证明模块存在，与「这个模块有没有 panic 点」无关。它仍然只是**那一个构建**的下限"
      "（整模块被内联到只剩 `drop glue` 时只能看见类型名），所以要跨版本并集（N_union）一起来读。"
      "**函数级**归属仍只有 %s（Location 口径，见 §0.1）。"
      % (g["modules"], graphwar_prefixed(), grand["modules"],
         fmt_pct(meta["attributed_funcs"], meta["funcstarts_n"])))
    A("3. 想提高函数级覆盖，只有两条路：(a) 把 `0x3c2bc0` / `0x3c2530` 这类无锚函数"
      "**逐个反编译并人工归属**；(b) 沿调用图从锚点扩散 —— 但那是**推断**，"
      "`docs/02` §10 做过（885/13263，6.7%，hub 节点被高估），不能进总账。")
    A("4. 一次 `--check` 只要几秒到一分钟（取决于 capstone 扫 `.text`），"
      "**`src/` 一改，`ours_funcs` 就会漂，`--check` 会失败** —— 这是故意的："
      "过期矩阵比没有矩阵更糟。\n")
    A("## 7. 与既有文档的口径差异（别以为是矛盾）\n")
    A("| 旧口径 | 出处 | 本表怎么处理 |")
    A("|---|---|---|")
    A("| 「544 个函数归属 / 56 个模块」 | `docs/02` §8 | 那张表把引用两个模块的函数在两个模块下**各算一次**。"
      "本表压平后不重复计数，并把跨模块数单独列为 `contested` |")
    A("| 「885/13263，观测 441 + 推断 444」 | `docs/02` §10 | 那是**调用图扩散**，属推断层。本表只收硬证据，"
      "不把扩散数写进总账 |")
    A("| 「57 个模块」 | 各处 | 旧读数 `57` 已作废（连它自己的两个口径都不对）："
      "按**文件** `src/graphwar_*.rs` 是 **%d** 个；只匹配 `[a-z_]+` 会漏掉"
      "`graphwar_lan_broadcaster_v6.rs` ⇒ **%d**。本表的口径是**模块**："
      "有同名文件的模块 **%d** 个（%d 个文件里有一个不是模块名，见 §17）。计数一律算出来、不写死 |"
      % (_src_graphwar_files(), _src_graphwar_files_alpha(), graphwar_prefixed(), _src_graphwar_files()))
    A("| 「`fixtures/module_funcs.tsv` 的地址清单」 | `docs/02` §14.2 | 那张表已自我更正为「不能判定函数身份」；"
      "本表用 Ghidra 边界 + 冲突压平重算 |")
    A("| 「解析器 `0x4ed670` 在 `graphwar_function_input.rs`」 | `docs/02` §1 | "
      "该函数体内唯一的 Location 锚是 `math_function.rs:75`，按证据归 `math_function.rs`；"
      "所以函数语言的实现落在本表的 `other` 口径，不在带前缀的那 %d 个模块里 |" % graphwar_prefixed())
    A("")
    A("## 8. 复现\n")
    A("```bash")
    A("python3 tools/coverage_matrix.py --check   # 只校验（缺 capstone 会自动换 .venv 解释器）")
    A("python3 tools/coverage_matrix.py           # 重新生成 TSV 与本页")
    A("```")
    A("")
    A("输入摘要（也写在 TSV 头部，`--check` 会核对）：\n")
    A("| 输入 | SHA-256 / 值 |")
    A("|---|---|")
    A("| `libgraphwarrust.so` | `%s` |" % meta["binary_sha256"])
    A("| `analysis/funcstarts_ghidra_va.pkl` | `%s` |" % meta["funcstarts_sha256"])
    A("| `src/` 树（%d 个文件、%d 条 `fn`） | `%s` |"
      % (meta["src_files"], meta["src_fns"], meta["src_tree_sha256"]))
    A("")
    return _tidy("".join(L))


def _tidy(md):
    """在标题/表格/清单**起段**处补空行 —— GFM 的表格和标题都必须顶格起段。

    只在"块种类切换"时插空行，否则表里每两行之间都会插一条空行。
    """
    out = []
    in_code = False
    prev = "p"
    for line in md.splitlines():
        if line.startswith("```"):
            if not in_code and out and out[-1].strip():
                out.append("")
            out.append(line)
            in_code = not in_code
            prev = "c"
            continue
        if in_code:
            out.append(line)
            continue
        if line.startswith("#"):
            kind = "h"
        elif line.startswith("|"):
            kind = "t"
        elif re.match(r"^\s*(\d+\.|-) ", line):
            kind = "l"
        else:
            kind = "p"
        if kind != "p" and kind != prev and out and out[-1].strip():
            out.append("")
        out.append(line)
        prev = kind
    return "\n".join(out) + "\n"


def _drift_reason():
    """告诉人这次漂移是"二进制变了"还是"src/ 变了" —— 后者只要重跑一次。"""
    try:
        with open(TSV_PATH, encoding="utf-8") as fh:
            head = fh.read(4096)
    except OSError:
        return None
    def field(name):
        m = re.search(r"^# %s: (\S+)$" % re.escape(name), head, re.M)
        return m.group(1) if m else None
    got_bin, got_pkl = field("binary_sha256"), field("funcstarts_sha256")
    if got_bin and got_bin != _sha256_file(SO):
        return "原版二进制变了（不是 src/ 的锅）：函数归属需要重新核对"
    if got_pkl and got_pkl != _sha256_file(FUNCSTARTS):
        return "函数表变了：先确认它仍由 analysis/build_index.py 从同一份 .so 生成"
    return "`src/` 变了（多数情况）：重跑一次即可，数字会自动跟上"


def _sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# ---------------------------------------------------------------------------
def main(argv):
    check = "--check" in argv or "--modules" in argv
    modules_only = "--modules" in argv
    rows, spot_rows, totals, grand, meta, _bm, _fn, _files, loc_lit, _E = build()

    # ---- 模块分母（T112）：三方差集表 + 正/负对照 ----
    android = android_symtab_modules()
    legacy = android_symtab_modules(legacy_only=True)
    audit_tsv, audit = module_denominator_audit(rows, loc_lit, android, legacy)
    module_problems = []
    if audit.get("available"):
        # 硬断言 1：矩阵的模块集合必须与未 strip 符号表的集合**逐个相同**
        # （t109 那种"整模块静默缺行"以后必须在这里炸，不许只写进报告）
        if audit["missing"]:
            module_problems.append("符号表里有、矩阵缺行：%s" % audit["missing"])
        if audit["extra"]:
            module_problems.append("矩阵有、该版符号表里没有：%s" % audit["extra"])
        # 硬断言 2：手工列 sym_funcs 必须与活的符号表相符
        drift = sorted(m for m in android
                       if m in M and M[m]["sym_funcs"] is not None
                       and M[m]["sym_funcs"] != android[m]["func"])
        if drift:
            module_problems.append("sym_funcs 与符号表不符：%s"
                                   % ["%s 表=%s 实测=%d" % (m, M[m]["sym_funcs"], android[m]["func"])
                                      for m in drift])
        bad = [c for c in audit["controls"] if c[1] is False]
        for tag, _ok, ev in bad:
            module_problems.append("对照 %s FAIL：%s" % (tag, ev))
    else:
        print("coverage_matrix: ⚠ 未 strip 符号表不在本机（%s）⇒ 模块分母自校验 SKIP"
              % os.path.relpath(ANDROID_SO, ROOT), file=sys.stderr)

    digest = _audit_digest()
    if digest is None:
        digest = audit          # 入库表缺/解析不出：退回落账的活结果（或 SKIP 文案）
    tsv = render_tsv(rows, meta)
    md = render_md(rows, spot_rows, totals, grand, meta, digest)

    if modules_only:
        # 只关心模块分母：写 fixture + 打印对照，不碰 docs/ 两个生成物
        for tag, ok, ev in audit.get("controls", []):
            print("  %-6s %-4s %s" % (tag, "SKIP" if ok is None else ("PASS" if ok else "FAIL"), ev))
        if audit_tsv is None:
            print("模块分母审计：SKIP（缺符号表）")
            return 0 if not module_problems else 1
        with open(AUDIT_PATH, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(audit_tsv)
        print("wrote %s" % os.path.relpath(AUDIT_PATH, ROOT))
        print("模块分母：N_2.0.60(J1')=%d（J1=%d）、矩阵 %d 个、sym_funcs 合计 %d"
              % (audit["n_android"], audit["j1"], audit["n_matrix"], audit["sym_funcs"]))
        for p in module_problems:
            print("  ✗", p)
        return 0 if not module_problems else 1

    if check:
        problems = list(module_problems)
        if audit_tsv is not None:
            if not os.path.exists(AUDIT_PATH):
                problems.append("缺文件：%s" % os.path.relpath(AUDIT_PATH, ROOT))
            else:
                with open(AUDIT_PATH, encoding="utf-8") as fh:
                    if fh.read() != audit_tsv:
                        problems.append("%s 与重算结果不一致（模块三方差集表过期）"
                                        % os.path.relpath(AUDIT_PATH, ROOT))
        for path, want in ((TSV_PATH, tsv), (MD_PATH, md)):
            if not os.path.exists(path):
                problems.append("缺文件：%s" % os.path.relpath(path, ROOT))
                continue
            with open(path, encoding="utf-8") as fh:
                got = fh.read()
            if got != want:
                gl, wl = got.splitlines(), want.splitlines()
                diff = next((i for i in range(min(len(gl), len(wl))) if gl[i] != wl[i]),
                            min(len(gl), len(wl)))
                problems.append("%s 与重算结果不一致（第一处差异在第 %d 行）"
                                % (os.path.relpath(path, ROOT), diff + 1))
        if problems:
            print("coverage_matrix --check: 失败")
            for p in problems:
                print("  ✗", p)
            why = _drift_reason()
            if why:
                print("  → " + why)
            print("  → 跑 `python3 tools/coverage_matrix.py` 重新生成后一起提交。")
            return 1
        print("coverage_matrix --check: OK")
        print("  %s" % grand_line(grand))
        return 0

    for path, text in ((TSV_PATH, tsv), (MD_PATH, md)):
        with open(path, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(text)
        print("wrote %s" % os.path.relpath(path, ROOT))
    if audit_tsv is not None:
        with open(AUDIT_PATH, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(audit_tsv)
        print("wrote %s" % os.path.relpath(AUDIT_PATH, ROOT))
    for p in module_problems:
        print("  ⚠", p)
    print()
    print(grand_line(grand))
    print()
    print("归属：%d / %d 个函数（%s），跨模块 %d 个；Location 锚 %d 个，涉及 %d 个源文件名"
          % (meta["attributed_funcs"], meta["funcstarts_n"],
             fmt_pct(meta["attributed_funcs"], meta["funcstarts_n"]),
             meta["contested_funcs"], meta["loc_structs"], meta["loc_files"]))
    for scope, t in (("graphwar", totals["graphwar"]), ("other", totals["other"])):
        print("  %-9s 模块 %2d，可归属 %3d，已实现 %3d（逐位 %d / 观测 %d / 推断 %d），"
              "未做 %d，未知模块 %d"
              % (scope, t["modules"], t["attributed"], t["implemented"],
                 t["by_status"]["逐位"], t["by_status"]["观测"], t["by_status"]["推断"],
                 t["by_status"]["未做"], t["unknown_modules"]))
    return 0


def grand_line(grand):
    return ("原版可归属函数 %d 个（Location 口径），分布在 %d 个模块（scope=graphwar %d + other %d；"
            "其中 %d 个有同名 `graphwar_*.rs` 文件）；我们已实现 %d 个函数，其中 逐位 %d / 观测 %d / "
            "推断 %d；未做 %d；完全未知的模块 %d 个（graphwar 里 %d 个）；"
            "**没实现的模块 %d 个**（`未做`/`未知`；这里的「未做 %d」是**函数**条数，别混）"
            "；`sym_funcs` 合计 %d（Android 符号表 FUNC，**另一条轴，不进上面的分母**）。"
            % (grand["attributed"], grand["modules"],
               grand["graphwar"]["modules"], grand["other"]["modules"], graphwar_prefixed(),
               grand["implemented"],
               grand["by_status"]["逐位"], grand["by_status"]["观测"], grand["by_status"]["推断"],
               grand["by_status"]["未做"], grand["unknown_modules"],
               grand["graphwar"]["unknown_modules"], grand["pending_modules"],
               grand["by_status"]["未做"], grand.get("sym_funcs") or 0))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
