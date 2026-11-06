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

## 用法

    python3 tools/coverage_matrix.py            # 重新生成两个产物 + 打印总账
    python3 tools/coverage_matrix.py --check    # 只校验，不写文件（不一致则退出码 1）

需要 `analysis/funcstarts_ghidra_va.pkl`（**不是** `funcstarts.pkl`，那张 `.eh_frame`
的表是坏的，见 `docs/02` §12）和仓库根目录的原版 `.so`（都不入库）。
capstone 只在 `.venv` 里；系统 `python3` 缺它时，本脚本会自动用 `../.venv/bin/python`
重新执行自己，所以上面的两条命令原样可用。

产物：
    docs/coverage-matrix.tsv   机器可读（本脚本唯一真源）
    docs/149-coverage-matrix.md  人读（也是本脚本生成的，避免手抄数字过期）
"""

import hashlib
import os
import pickle
import re
import struct
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
    return os.path.dirname(ROOT)


UPSTREAM = _find_upstream()                  # 主仓库（分析仓库）
SO = os.path.join(UPSTREAM, "libgraphwarrust.so")
FUNCSTARTS = os.path.join(UPSTREAM, "analysis", "funcstarts_ghidra_va.pkl")
TSV_PATH = os.path.join(ROOT, "docs", "coverage-matrix.tsv")
MD_PATH = os.path.join(ROOT, "docs", "149-coverage-matrix.md")

TOOL_VERSION = "1"


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
# 这张表就是"进度尺"的读数口径；改它必须同时改 evidence，否则 --check 会
# 拿旧 TSV 对新结果失败。
# ---------------------------------------------------------------------------
M = {}


def mod(module, scope, ours, status, evidence):
    M[module] = dict(module=module, scope=scope, ours=tuple(ours),
                     status=status, evidence=evidence)


# ---- 58 个 graphwar_*.rs（`[a-z_]+` 的正则会漏掉带数字的 v6，实际是 58 个）----
mod("graphwar_ad_switch.rs", "graphwar", ("src/graphwar_ad_switch.rs",), "观测",
    "6 条全是 gdext 惰性注册驱动；两个 getter trampoline + owned(+0x18) 已钉死")
mod("graphwar_audio_control.rs", "graphwar", (), "未做",
    "docs/111：SoundHandler / -15f / -5f / 32767 归一化已还原，但没接进 src/（client/ 是仿制）")
mod("graphwar_backend.rs", "graphwar", (), "未做",
    "docs/132：读出 15 个方法名（start_server / lan_broadcaster / broker …），不是 Godot 类；无实现")
mod("graphwar_button.rs", "graphwar", ("src/graphwar_button.rs",), "观测",
    "6/6 方法全解（init/ready/process/button_down/pressed/button_up）+16 条名字分派入口；docs/16 §18")
mod("graphwar_campaign_control.rs", "graphwar", (), "未做",
    "docs/118 §7：21 关数据已从 0x4ad380 解出、campaign.save 格式已解；src/ 里还没有这张表")
mod("graphwar_client_connection_handler.rs", "graphwar", (), "未做",
    "docs/134 只覆盖 client/ 的 GDScript 联机；原版这个 Rust handler 未还原")
mod("graphwar_color_manager.rs", "graphwar", (), "未做",
    "docs/102：两张 6 色队色表 + 随机起点线性探测已解出；src/ 无实现")
mod("graphwar_computer_player_manager.rs", "graphwar", ("src/graphwar_computer_player_manager.rs",), "观测",
    "6/7 定位：HashMap<u32,BotData>（步长 0x78）+ SoldierBotData（0x58）。**不是掉线托管**：唯一入口是 bot_added_event(10)；level 阈值 4/11/26/101/251，精英数=种群/5，limit=5/5/10/15/25/200（docs/13 §25）")
mod("graphwar_crash_reporter.rs", "graphwar", (), "未知",
    "全二进制**没有一个** panic 锚落在这个文件上，也没有任何地址级观测")
mod("graphwar_draw_util.rs", "graphwar", (), "未做",
    "docs/140：准备圈/弹道画法已从机器码扒出；绘制落 client/ GDScript，src/ 无实现")
mod("graphwar_effect_manager.rs", "graphwar", (), "未知",
    "全仓库 0 处提及：无锚点之外的信息、无观测、无实现")
mod("graphwar_entity_effect_manager.rs", "graphwar", (), "未知",
    "全仓库 0 处提及：只有名字，没有行为证据")
mod("graphwar_event_handler.rs", "graphwar", ("src/events.rs",), "观测",
    "docs/114：0x3d38f0 的「事件名 → 判别值」表逐条解出（四道自校验）；事件枚举与分派已落码")
mod("graphwar_event.rs", "graphwar", ("src/events.rs",), "观测",
    "docs/114：72 个判别值实测；但 serde 键序只在 41 行抓包上同序，仍是推断（docs/96 §9.1）")
mod("graphwar_function_evolutionator.rs", "graphwar", ("src/graphwar_function_evolutionator.rs",), "观测",
    "docs/AI.md：权重/长度/装配/交叉/变异/适应度全部从反汇编 + 40 万次运行时抽样得出；无逐字节对拍")
mod("graphwar_function_input.rs", "graphwar", ("src/graphwar_function_input.rs",), "观测",
    "17 条函数；7 个原版名 + gui_input 0x4fedc0；**输入框只做一步预处理：删空格**（docs/03 §13）")
mod("graphwar_function_manager.rs", "graphwar", ("src/game.rs",), "观测",
    "docs/112 §2.1：7 个函数的存取结构逐条读出（HashMap<u32, 112 字节>、Vec<Token>）；"
    "我方 Match::functions 同构，但没有逐字节对拍")
mod("graphwar_game_client.rs", "graphwar", ("src/graphwar_game_client.rs",), "观测",
    "36 个函数分 8 类（docs/16 §17）：8 种请求事件判别值 11/12/16/17/18/19/20/21 + 0x48 记录形状 + "
    "add_bot_0..5 -> 3/10/25/100/250/1000 + 0x517690 的 3 订阅名/14 manager；6 个入口函数语义未知")
mod("graphwar_game_recorder.rs", "graphwar", (), "未知",
    "只有 docs/02 一行（对局录制），无地址级观测、无实现")
mod("graphwar_game_server.rs", "graphwar", ("src/graphwar_game_server.rs", "src/server.rs"), "逐位",
    "docs/43/46：一回合 12 条报文与抓包逐字节一致；docs/62 真实 TCP 跑到 EndingGame")
mod("graphwar_game_state_manager.rs", "graphwar", ("src/game.rs", "src/turnstate.rs"), "逐位",
    "docs/41：6 状态 7 事件、13 次迁移在真实对局里逐个相同")
mod("graphwar_graphics_manager.rs", "graphwar", (), "未知",
    "只有 docs/02 一行（图形资源管理），无地址级观测、无实现")
mod("graphwar_lan_broadcaster_v6.rs", "graphwar", (), "未做",
    "docs/07/24：局域网广播报文已抓包；docs/134 里只有 client/ GDScript 侧，src/ 无实现")
mod("graphwar_lan_vbox.rs", "graphwar", ("src/graphwar_lan_vbox.rs",), "观测",
    "13 个 FUNC 全登记；process = 0x4f90b0（唯一锚，16 个三元组）。**不消费任何事件**——每帧直读两张表（LAN 广播器 +0x128/+0x1a0 或 broker +0x240/+0x248）；行节点 = 新场景实例 + 按 SocketAddr 缓存复用；Join 走 mpmc 通道下一帧才连；**没有排序**（HashMap 迭代序）（docs/09 §10）")
mod("graphwar_level_generator.rs", "graphwar", ("src/graphwar_level_generator.rs", "src/graphwar_level_generator/tables.rs"), "逐位",
    "docs/61/56：障碍 x/y/r 的 to_bits() 全等，13/13 逐位复现；docs/55 公式闭合")
mod("graphwar_level_manager.rs", "graphwar", ("src/graphwar_level_manager.rs",), "观测",
    "锚点最密（590 个 Location / 40 个函数）；LEVELS[21]+SETUPS[21]+campaign.save 逐位（docs/10 §12），"
    "40 个锚函数里 26 个已还原、14 个仅分类；level_info/campaign.save 簇无锚，落地时可能要搬去 campaign_control")
mod("graphwar_life_manager.rs", "graphwar", ("src/graphwar_life_manager.rs",), "逐位",
    "docs/65：穿过即死/友军致死/无血量在真实对局与回放里逐个相同；docs/41 状态迁移一致")
mod("graphwar_lobby_control.rs", "graphwar", (), "未做",
    "docs/112 只读出它与战役/名字模块的调用关系；界面在 client/ GDScript，src/ 无实现")
mod("graphwar_local_player_manager.rs", "graphwar", (), "未做",
    "docs/112 §13 记录了结构观测；src/ 无对应实现")
mod("graphwar_logger.rs", "graphwar", (), "未做",
    "docs/112 记录 log 入口的结构观测；src/ 无实现")
mod("graphwar_long_press_animation.rs", "graphwar", ("src/graphwar_long_press_animation.rs",), "观测",
    "2.0.60 只剩 draw（0x4d8100）：angle=elapsed×4π、>2π 结束⇒0.5s、黑白两次绘制；9 个常量逐比特")
mod("graphwar_mode_manager.rs", "graphwar", (), "未做",
    "docs/88 §7：TurnMode/FunctionMode/AxisMode/TimeMode 的取值已从 .rodata 解出；模块本体未复原")
mod("graphwar_module_container.rs", "graphwar", ("src/graphwar_module_container.rs",), "观测",
    "docs/02 §22：类型本体 = HashMap<TypeId, Rc<dyn Any>>（48 字节 / 1 字段），"
    "3 个泛型方法 × 25 型 = 68 个单态化；三块两侧都等距且首尾背靠背"
    "（arm64 0x49e520/0x49f9d4/0x4a20a4，x86 0x4694f0/0x46a490/0x46c5a0），"
    "25 型真名 + size + 六个入口全部定名；装配在调用者（GameClient::new 22 / "
    "game_server_thread 20 / Backend 1×2），容器自己没有 init；"
    "Location 只有 get_module_wk 一个（30:28，RefCell::borrow 的 track_caller）"
    "⇒ add/get 零锚是正确结果")
mod("graphwar_name_manager.rs", "graphwar", (), "未做",
    "docs/112 §0：HashMap<u32, String>、entry 32 字节、NameInfo 判别值 60 已逐位解出；src/ 无实现")
mod("graphwar_nodes.rs", "graphwar", (), "未做",
    "docs/112 有结构观测；src/ 无实现")
mod("graphwar_obstacle_manager.rs", "graphwar", ("src/plane.rs",), "逐位",
    "只覆盖掩膜部分：0x4f15b0 光栅化与 is_occupied 与真二进制逐字节 / 300 随机点一致"
    "（docs/39/08），摧毁 = 挖 r=0.5 洞；障碍的生成与生命周期未落 src/。"
    "⚠ 文件级归属未定：class_probe 给出的本模块锚函数是 0x3c64c0/0x3f00b0/0x3f2480/0x3f27d0，"
    "0x4f15b0 不在其中（docs/02 §17.1 记录了这处冲突，不强行圆）")
mod("graphwar_path_calculator.rs", "graphwar", ("src/graphwar_path_calculator.rs",), "逐位",
    "docs/91：补上 hi=mid2 后 24/24 用例整条点序列与真二进制逐位相等。注意：采样器 "
    "0x3c2530 本体无 panic 锚 ⇒ 只归属到 1 个辅助函数，orig_funcs 远小于模块规模")
mod("graphwar_plane_pos_manager.rs", "graphwar", ("src/plane.rs",), "观测",
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
mod("graphwar_player_manager.rs", "graphwar", (), "未知",
    "只有 docs/02 一行（玩家），无地址级观测、无实现")
mod("graphwar_player_screen_control.rs", "graphwar", (), "未知",
    "只有 docs/02 一行（玩家界面），无地址级观测；界面在 client/ GDScript")
mod("graphwar_play_screen_control.rs", "graphwar", ("src/graphwar_play_screen_control.rs",), "观测",
    "docs/14-turn.md §9 逐函数规格（10 个可归属函数 / 64 个锚）；界面视图仍在 client/ GDScript（仿制），"
    "但控制器本体已落在 src/graphwar_play_screen_control.rs。旧行写「未做」与 src/lib.rs/文件头冲突 ⇒ 按代码更正")
mod("graphwar_rank_manager.rs", "graphwar", (), "未做",
    "docs/112 §5：RankInfo 判别值 67、结构观测；段位算法在本模块内算不出来（负结果）")
mod("graphwar_remote_event_handler.rs", "graphwar", ("src/events.rs",), "观测",
    "docs/110/96：远端事件名与判别值已解；我方 events.rs 实现枚举 + 编解码，无逐字节对拍")
mod("graphwar_replay_control.rs", "graphwar", (), "未做",
    "docs/133 的回放播放器在 client/ GDScript；src/graphwar_replay_player.rs 只做文件解析与推进，"
    "不含这个控制器（文件里的 ReplayPlayer 时钟是我方自定模型，见 docs/02 §17.4）")
mod("graphwar_replay_player.rs", "graphwar", ("src/graphwar_replay_player.rs",), "观测",
    "docs/136/142：消息级对拍 400 份回放里 71 份完全一致 ⇒ 已实现但未到逐位")
mod("graphwar_root_node.rs", "graphwar", ("src/graphwar_root_node.rs",), "观测",
    "Screen×15 + Command×12 + 0x3af850 切屏决策表 + 构造初值（docs/16 §15.10）；"
    "0x3a5760 主体 20288B 刻意未猜")
mod("graphwar_server_broker_handler.rs", "graphwar", (), "未做",
    "原模块是**客户端侧**的 broker 处理器，未做；src/_scaffold/broker.rs 是我们自建的 broker 服务端，"
    "不是它的复原（见 docs/115）")
mod("graphwar_server_connection_handler.rs", "graphwar", ("src/server.rs",), "逐位",
    "docs/33：新连接的状态同步与真实客户端逐字节一致")
mod("graphwar_setup_control.rs", "graphwar", ("src/graphwar_setup_control.rs",), "观测",
    "70 锚 -> 37 函数（矩阵的 21 是压平口径）；四格模式/时间档/玩家条目/Start-Cancel 已落码（docs/16 §16），"
    "18 个 gdext 薄壳与 3 个事件回呼语义未知")
mod("graphwar_shot_calculator.rs", "graphwar", ("src/graphwar_shot_calculator.rs", "src/graphwar_shot_calculator/settle.rs"), "逐位",
    "docs/86 §14：82 发并集 152/152、漏报 0、时刻判决 38:0；docs/122 按 id 排除射手是"
    "指令级。注意：0x3c2bc0 那个 12 KB 主函数**没有逐条还原**，逐位的是行为不是函数体")
mod("graphwar_soldier_manager.rs", "graphwar", ("src/graphwar_soldier_manager.rs",), "观测",
    "5/6 定位：HashMap<u32,u32>（士兵→主人）。只管「谁的」，**轮转不查它**（turn_manager 自持 Vec<soldier_id>）；entity_removed 时比值命中就发键 => 玩家没了士兵跟着消失（docs/10 §13）")
mod("graphwar_target_manager.rs", "graphwar", (), "未做",
    "docs/123：重置规则形状已钉住（翻 alive + 广播 LifeInfo）但判定点未知，397 份回放 0 次"
    "部分命中 ⇒ 无真值；docs/131 §4 的落地在 client/（自述为推断），src/ 只有 Target 数据结构")
mod("graphwar_team_manager.rs", "graphwar", ("src/graphwar_team_manager.rs",), "观测",
    "7/7 定位：Mutex<HashMap<u32, GraphwarTeam>>（桶步长 8，值 1 字节）。**唯一分队规则 0x3c1b40：进人少的队、平手进 Team1**；无队长/选边/每队容量，上限整房 10 人（docs/10 §13）")
mod("graphwar_texture_loader.rs", "graphwar", (), "未知",
    "只有 docs/02 一行（贴图加载），无地址级观测、无实现")
mod("graphwar_tick_handler.rs", "graphwar", ("src/graphwar_tick_handler.rs",), "逐位",
    "docs/35：113 个真实样本，TickReply 逐字节一致")
mod("graphwar_turn_manager.rs", "graphwar", ("src/graphwar_turn_manager.rs",), "逐位",
    "docs/98/117：射手轮转 = Vec<soldier_id> + 游标，在真实 TCP 上断言「首发射手 = soldiers[1]」"
    "「跳过死者」；docs/41 状态迁移逐个相同")
mod("graphwar_video_writer.rs", "graphwar", (), "未知",
    "只有 docs/02 一行（录像导出），无地址级观测、无实现")

# ---- 非 graphwar_ 前缀：同样是原版 src/ 里的文件，但只有 math_function 等少数是真游戏模块 ----
mod("math_function.rs", "other", ("src/math_function.rs", "src/math_function/to_text.rs"), "逐位",
    "docs/03：90 例文法 + 70 条真值夹具与真二进制对拍。**解析器 0x4ed670 的 panic 锚就落在这个"
    "文件上**（math_function.rs:75）⇒ 函数语言的解析器属于本模块，不属于 graphwar_function_input")
mod("polish_notation_converter.rs", "other", (), "未做",
    "2026-11 更正：本模块的锚函数是 0x4f5450（:36:52 / :116:55，tools/class_probe.py），**不是** 0x4ec160。0x4ec160 的 panic 分支引的是 math_function.rs:285:28（docs/119 §4.6）⇒ 我方 to_text.rs 归 math_function.rs；本模块未实现")
mod("entity_drawer.rs", "other", ("src/entity_drawer.rs",), "观测",
    "10 个函数全解：draw/ready/process/update_sprite/update_name_label/draw_offset_triangle/get_entity_{hat,name_label,face,skin}。**士兵/靶子不是图元而是子节点**——3 个 Sprite2D"
    "（skin/face/hat 贴图）+ 1 个 RichTextLabel（res://name_label.tscn），懒建缓存于 4 张 HashMap，"
    "由 process 每帧摆位置；draw() 只画准星（3 弧 + 3 箭头）。docs/16 §24")
mod("effect_drawer.rs", "other", ("src/effect_drawer.rs",), "观测",
    "7 个 pub fn / 36 常量：draw_circle×4 + polygon×1 + line×2 + 一次 draw_set_transform，半径 0.05/0.06；"
    "tick 制 60 tick = 1 秒，物理子步 1/600 s，相位推进 600 tick；与 client/ 有 10 处出入（docs/16 §21）")
mod("axis_drawer.rs", "other", ("src/axis_drawer.rs",), "观测",
    "3 个 pub fn / 24 常量：四档轴模式与全部数值逐条登记（docs/16 §21）")
mod("obstacle_drawer.rs", "other", (), "未知",
    "非 graphwar_ 前缀；除模块名外没有一条能指到地址或报文的观测")
mod("function_drawer.rs", "other", (), "未知",
    "非 graphwar_ 前缀；除模块名外没有任何观测")
mod("button_manager.rs", "other", (), "未知",
    "非 graphwar_ 前缀；除模块名外没有任何观测")
mod("function_button_manager.rs", "other", (), "未知",
    "非 graphwar_ 前缀；除模块名外没有任何观测")
mod("function_history_recorder.rs", "other", (), "未知",
    "非 graphwar_ 前缀；除模块名外没有任何观测")
mod("player_showcase.rs", "other", ("src/player_showcase.rs",), "观测",
    "10 个函数全解：ready 0x4e6480 / on_text_change 0x4efd70 / process 0x4e68f0 / draw 0x4e5400 + 6 个 gdext 生成物。**三次 draw_texture_rect_full**（皮肤→脸→帽），与颜色/段位无关；读根节点 +0x16a8 的 GraphwarPlayerConfig（名字+0x00/皮肤+0x60/脸+0x78/帽+0x90/变更计数+0xa8），process 轮询计数才重画（docs/16 §25）")
mod("server_broker_request_connection.rs", "other", (), "未知",
    "非 graphwar_ 前缀；docs/115 只覆盖 broker 线格式，这个连接对象本身没看过")
mod("backtracer.rs", "other", (), "未做",
    "2024 版符号表：graphwarrust::backtracer 4 个 direct 符号（自带回溯器，不是外部 crate）")
mod("function_token.rs", "other", (), "未做",
    "2024 版符号表：graphwarrust::function_token 1 个符号 + 类型 FunctionToken")
mod("graphwar_event_queue.rs", "graphwar", (), "未做",
    "2024 版符号表：graphwarrust::graphwar_event_queue 2 个符号 + 类型 GraphwarEventQueue；"
    "docs/ 里此前完全没有记录")
mod("graphwar_game_info.rs", "graphwar", (), "未做",
    "2024 版符号表：graphwar_game_info 独立模块（GameInfo **不在** graphwar_event.rs，"
    "旧文档把它当成 event 的一部分是错的）")
mod("graphwar_player_config.rs", "graphwar", (), "未做",
    "2024 版符号表：graphwar_player_config 8 个 direct 符号；docs/ 里此前完全没有记录")
mod("graphwar_replay_reader.rs", "graphwar", (), "未做",
    "2024 版符号表：graphwar_replay_reader 2 个符号；与 graphwar_replay_player 是两个模块")
mod("graphwar_stream_buffer.rs", "graphwar", (), "未做",
    "2024 版符号表：graphwar_stream_buffer 2 个符号；旧文档只在 2024 版字符串里见过这个名字")
mod("weak_caller.rs", "other", (), "未做",
    "**旧判定已推翻**：2024 版符号表里有 graphwarrust::weak_caller 的 27 个 direct 符号"
    "（签名形如 fn(&Weak<..>,..)），是 **crate 内模块**，不是第三方依赖（docs/02 §18）。"
    "旧依据 docs/02 §9 作废")

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
    ("src/_scaffold/gdext.rs", "GDExtension 入口脚手架（原版的入口在 Godot 模块侧，不在这 58+12 个文件里）"),
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
                    done_modules=sum(1 for r in sel if r["status"] in ("逐位", "观测", "推断")))

    totals = {s: tally(s) for s in ("graphwar", "other")}
    grand = dict(
        modules=sum(t["modules"] for t in totals.values()),
        graphwar=totals["graphwar"], other=totals["other"],
        attributed=sum(t["attributed"] for t in totals.values()),
        implemented=sum(t["implemented"] for t in totals.values()),
        by_status={st: sum(t["by_status"][st] for t in totals.values()) for st in STATUS_ORDER},
        unknown_modules=sum(t["unknown_modules"] for t in totals.values()),
        done_modules=sum(t["done_modules"] for t in totals.values()),
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
    )
    return rows, spot_rows, totals, grand, meta, by_module, fn_counts, files


# ---------------------------------------------------------------------------
# 渲染
# ---------------------------------------------------------------------------
TSV_HEADER = ("# coverage-matrix.tsv — 由 tools/coverage_matrix.py 生成，**不要手改**\n"
              "# 重算：python3 tools/coverage_matrix.py    只校验：--check\n"
              "# orig_funcs：能按 panic::Location 硬归属的函数数（可度量下限，不是模块规模）\n"
              "# contested：该函数同时引用了别的模块 Location（已按锚点数压平，不重复计数）\n"
              "# anchors：该模块名下的 panic::Location 结构体个数（引用点密度）\n"
              "# ours_funcs：ours_file 里 fn 的条数（注释与字符串已剔除）\n")

TSV_COLUMNS = ["module", "scope", "orig_funcs", "contested", "anchors",
               "ours_file", "ours_funcs", "status", "evidence"]


def render_tsv(rows, meta):
    out = [TSV_HEADER]
    for k in ("tool_version", "binary_sha256", "funcstarts_sha256", "src_tree_sha256",
              "funcstarts_n", "src_modules", "loc_structs", "attributed_funcs", "contested_funcs",
              "src_files", "src_fns"):
        out.append("# %s: %s\n" % (k, meta[k]))
    out.append("\t".join(TSV_COLUMNS) + "\n")
    for r in rows:
        out.append("\t".join(str(r[c]) for c in TSV_COLUMNS) + "\n")
    return "".join(out)


def fmt_pct(a, b):
    return "%.1f%%" % (100.0 * a / b) if b else "—"


def render_md(rows, spot_rows, totals, grand, meta):
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
    for name, t in (("58 个 `graphwar_*.rs`", g), ("%d 个非 graphwar 的游戏模块" % o["modules"], o)):
        A("| %s | %d | %d | %d | %d | %d | %d | %d | %d |"
          % (name, t["modules"], t["attributed"], t["implemented"],
             t["by_status"]["逐位"], t["by_status"]["观测"], t["by_status"]["推断"],
             t["by_status"]["未做"], t["unknown_modules"]))
    A("")
    A("还有 `weak_caller.rs`（40 个归属函数）按 `docs/02` §9 判为第三方依赖库，"
      "单独一行留在 TSV 里备查，**不计入**任何合计。\n")
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
    for scope, title in (("graphwar", "2.1 58 个 `graphwar_*.rs`"),
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
    A("**58 个 `graphwar_*.rs` 里有 %d 个**属于这一类 —— 判据是：没有实现、"
      "且除模块名之外**没有任何**能指到地址/报文/真机的观测。\n" % len(unknown))
    A("| module | orig_funcs | anchors | 现状 |")
    A("|---|---|---|---|")
    for r in unknown:
        A("| `%s` | %d | %d | 只有名字 |" % (r["module"], r["orig_funcs"], r["anchors"]))
    A("")
    zero_anchor = [r for r in rows if r["scope"] == "graphwar" and r["anchors"] == 0]
    A("另外还有 **%d 个模块一个 panic 锚都没有**（连「它有没有函数」都答不出来）：%s。"
      % (len(zero_anchor), "、".join("`%s`" % r["module"] for r in zero_anchor)))
    A("它们的 `orig_funcs` 是 0，**不代表模块为空**，只代表这个方法对它们完全失效。\n")
    A("非 graphwar 前缀里还有 %d 个未知模块（`obstacle_drawer` / `function_drawer` / "
      "`button_manager` / `function_button_manager` / `function_history_recorder` / "
      "`player_showcase` / `server_broker_request_connection`），它们的共同点是"
      "**连「是不是游戏代码」都只有名字可依**。\n"
      % sum(1 for r in rows if r["status"] == "未知" and r["scope"] == "other"))
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
    A("2. **模块级分母（58）比函数级分母更可信，但它也是下限**：58 个名字是从二进制里的 "
      "`src/graphwar_*.rs` 字面量直接读出来的，只要模块里有一个 panic 点就会被列出来；"
      "**完全没有 panic 的模块不会出现**，所以真实的模块数只能 ≥ 58。函数级归属则只有 %s。"
      % fmt_pct(meta["attributed_funcs"], meta["funcstarts_n"]))
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
    A("| 「57 个模块」 | 各处 | 按 `src/graphwar_[a-z_]+\\.rs` 数是 57；"
      "**加数字字符后是 58**（`graphwar_lan_broadcaster_v6.rs` 被漏了）。本表用 58 |")
    A("| 「`fixtures/module_funcs.tsv` 的地址清单」 | `docs/02` §14.2 | 那张表已自我更正为「不能判定函数身份」；"
      "本表用 Ghidra 边界 + 冲突压平重算 |")
    A("| 「解析器 `0x4ed670` 在 `graphwar_function_input.rs`」 | `docs/02` §1 | "
      "该函数体内唯一的 Location 锚是 `math_function.rs:75`，按证据归 `math_function.rs`；"
      "所以函数语言的实现落在本表的 `other` 口径，不在 58 个 graphwar 模块里 |")
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
    check = "--check" in argv
    rows, spot_rows, totals, grand, meta, _bm, _fn, _files = build()
    tsv = render_tsv(rows, meta)
    md = render_md(rows, spot_rows, totals, grand, meta)

    if check:
        problems = []
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
    print("wrote %s" % os.path.relpath(TSV_PATH, ROOT))
    print("wrote %s" % os.path.relpath(MD_PATH, ROOT))
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
    return ("原版可归属函数 %d 个，分布在 %d 个模块（58 个 graphwar_*.rs 占 %d、"
            "%d 个非 graphwar 游戏模块占 %d）；我们已实现 %d 个函数，其中 逐位 %d / 观测 %d / "
            "推断 %d；未做 %d；完全未知的模块 %d 个（graphwar 里 %d 个）。"
            % (grand["attributed"], grand["modules"],
               grand["graphwar"]["attributed"], grand["other"]["modules"],
               grand["other"]["attributed"], grand["implemented"],
               grand["by_status"]["逐位"], grand["by_status"]["观测"], grand["by_status"]["推断"],
               grand["by_status"]["未做"], grand["unknown_modules"],
               grand["graphwar"]["unknown_modules"]))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
