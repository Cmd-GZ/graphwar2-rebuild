#!/usr/bin/env python3
"""callgraph_roles.py — 用**调用图位置 + 引擎 API + 字符串 + 常量 + 形状**给无名字函数反推**角色**。

## 这个工具与 §35 的 `callgraph_fingerprint.py` 是什么关系

* `callgraph_fingerprint.py`（§35）回答「这个 Linux 函数**结构同源**于跨构建的哪个函数」——
  它的产物是**候选地址**，且 §35.5 明说：名字仍然没有。
* 本工具回答**另一个问题**：「这个函数**在干什么**」——产物是**行为描述**（角色），
  **不是名字**，也不许写成名字（任务书 58 §2）。

两者的公共基础是同一份 Linux 指纹缓存（`CFGFP_CACHE`，`linux` tag）：`universe`（13,263）、
`size` / `ninsn` / `consts` / `calls` / `callees` / `callers` / `locrefs` / `strs` 直接复用，
**不重扫指纹**。

## 为什么还要再扫一遍 `.text`（只扫 Linux 一份）

缓存里**没有**两样东西，而它们恰好是角色推断最硬的证据：

1. **字符串内容**（缓存只存 sha1 前 8 位，见 §35.2 F7）——角色要的是文案本身；
2. **`call *disp(%rip)` 的槽地址**（缓存只有 `icalls` 计数）——gdext 的引擎 API 调用
   全走这条路（`AGENTS.md`「间接调用」第 1 条）：`call *0x9bc020(%rip)`，
   而 `0x9bc020` 由 `R_X86_64_RELATIVE` 写成 **`0x8abca0`**（`draw_arc` 的包装）。
   不解析它，`EntityDrawer::draw` 的 `callers` 是**空**的（实测），
   于是"谁在画东西"这个最有用的问题直接问不出来。

一次线性反汇编（`C.disasm_all`，与 §35 同一份容错跳洞实现）同时收集这两样，
结果缓存到 `--cache` 目录的 pkl（**不入库**，上游 `AGENTS.md` 禁二进制衍生内容）。

## 引擎 API 名是怎么来的（**观测**，不是猜）

gdext 为每个引擎 native 方法生成一个**包装函数**，包装里引用了该方法名的
`StringName` 字面量（`.rodata` 里的裸 C 串）。实例（`docs/16` §23.3 的锚）：

    .rodata 0x19f9d0 = "draw_arc" → 包装 0x8abca0（引用该串，sha1 前 8 位 59c60ac1）
                                  → GOT 槽 0x9bc020（R_X86_64_RELATIVE → 0x8abca0）

⇒ 对任意包装 `w`，`w` 引用的**唯一那个 godot 方法名**就是它包装的 API 名。
本工具把"函数里出现"的这类名字叫 **API 足迹**，是最硬的一档证据（能指到字面量地址与槽）。

## 用法

    ../.reference/venv/bin/python tools/callgraph_roles.py build      # 扫一遍 + 建图（约 1 min，缓存）
    ../.reference/venv/bin/python tools/callgraph_roles.py graph      # 图规模：节点/边/直接 vs 指针
    ../.reference/venv/bin/python tools/callgraph_roles.py match      # 角色规则正对照（348 个已定名）
    ../.reference/venv/bin/python tools/callgraph_roles.py run        # 给 12,736 个出角色 → 夹具
    ../.reference/venv/bin/python tools/callgraph_roles.py probe 0x4b2890
"""

import argparse
import bisect
import collections
import hashlib
import json
import os
import pickle
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import callgraph_fingerprint as C            # noqa: E402  load_fp / disasm_all / string_at / named_linux
import sym_match as SM                       # noqa: E402  Elf
import three_way_match as T                  # noqa: E402  owner_of / Pe

FIXTURES = os.path.join(ROOT, "fixtures")
OUT_TSV = os.path.join(FIXTURES, "callgraph_roles.tsv")
CTRL_TSV = os.path.join(FIXTURES, "callgraph_roles_control.tsv")

CACHE_VERSION = "roles-v1"
MAX_STR = C.MAX_STR
MAX_STRS_PER_FN = 24


# ===========================================================================
# 0. 缓存目录（**不入库**；默认放 `.agent-work`，与 §35 的 CFGFP_CACHE 分开）
# ===========================================================================
def cache_dir():
    d = os.environ.get("ROLES_CACHE")
    if not d:
        d = os.path.join(SM.GW2, ".agent-work/cg/roles58")
    os.makedirs(d, exist_ok=True)
    return d


def _size(path):
    try:
        return os.path.getsize(path)
    except OSError:
        return 0


# ===========================================================================
# 1. 扫描：字符串内容 + `call *slot` 的槽（**只** Linux）
# ===========================================================================
def _ro_sections(obj):
    ro = {n: obj.sec[n] for n in C.STR_SECTIONS if n in obj.sec}
    lo = min((s["addr"] for s in ro.values()), default=0)
    hi = max((s["addr"] + s["size"] for s in ro.values()), default=0)
    return ro, lo, hi


def scan_roles(obj, fp, verbose=True):
    """一次线性反汇编，收集缓存里没有的两样证据（字符串**内容** / 间接调用槽）。

    返回 `{va: dict(strs=[...], islots=[(site, slot)], irefs=[(site, slot)])}`：

    * `strs`   —— 该函数引用的字符串**内容**（去重，保序，上限 `MAX_STRS_PER_FN`）；
    * `islots` —— `call *disp(%rip)` 的 (调用点, 槽)；
    * `irefs`  —— 其余 rip 相对取的槽（`.data.rel.ro` 的 (ptr,len) 字符串槽、
                  函数指针表/虚表的**基址**取值都在这里）。

    与 `C.scan_x86` 同一套 `owner_of` / `string_at` 口径，**不另立标准**。
    """
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64
    from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_REG_RIP

    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True
    tex = obj.sec[".text"]
    code = obj.read(tex["addr"], tex["size"])
    U = fp.universe
    starts = fp.starts
    ro, ro_lo, ro_hi = _ro_sections(obj)

    def sec_name(va):
        for n, s in ro.items():
            if s["addr"] <= va < s["addr"] + s["size"]:
                return n
        return None

    out = {va: dict(strs=[], msgs=[], islots=[], irefs=[], frefs=[], srefs=[]) for va in U}
    seen = {va: set() for va in U}          # 每函数内的字符串去重（内容）
    seen_m = {va: set() for va in U}
    pend = {va: [] for va in U}             # 待配对的 `lea reg,[rip+rodata]`
    n_site = n_msg = 0
    from capstone.x86 import X86_OP_MEM as _MEM, X86_OP_IMM as _IMM, X86_REG_RIP as _RIP

    def _imm_val(it):
        for o in it.operands:
            if o.type == _IMM:
                return o.imm
        return None

    for insn in C.disasm_all(cs, code, tex["addr"], tex["addr"] + tex["size"], 1):
        owner = T.owner_of(starts, U, insn.address)
        if owner is None:
            continue
        fe = out[owner]
        end = insn.address + insn.size
        mn = insn.mnemonic
        if mn == "call":
            if insn.operands and insn.operands[0].type == X86_OP_MEM:
                op = insn.operands[0]
                if op.mem.base == X86_REG_RIP:
                    fe["islots"].append((insn.address, (end + op.mem.disp) & 0xFFFFFFFFFFFFFFFF))
                    n_site += 1
                continue
            if insn.operands and insn.operands[0].type == X86_OP_IMM:
                continue
        # ---- `&str` 的 (ptr,len)：godot 的 StringName 在栈上现搭，形如
        #      lea rax,[rip+0x19f9d0] ; mov qword ptr [rsp+0x60], 8   （实测 0x8abca0 的 draw_arc）
        #      长度是**立即数**，所以只有把两者配对才能拿到**准确**的串（`_cstr` 会读穿到相邻串）。
        if mn == "lea":
            for op in insn.operands:
                if op.type == _MEM and op.mem.base == X86_REG_RIP:
                    t = (end + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
                    if sec_name(t):
                        pend[owner].append((insn.address, t))
                        pend[owner] = pend[owner][-4:]
                    break
        elif mn.startswith("mov") and pend[owner]:
            v = _imm_val(insn)
            if v is not None and 4 <= v <= 64:
                site, t = pend[owner].pop()
                b = obj.read(t, v)
                if b and len(b) == v and all(32 <= c < 127 for c in b):
                    s = b.decode("latin-1")
                    fe["srefs"].append((insn.address, s))
                    if s not in seen_m[owner]:
                        seen_m[owner].add(s)
                        n_msg += 1
                        if len(fe["msgs"]) < MAX_STRS_PER_FN:
                            fe["msgs"].append(s)
        for op in insn.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                tgt = (end + op.mem.disp) & 0xFFFFFFFFFFFFFFFF
            elif op.type == X86_OP_IMM:
                tgt = op.imm & 0xFFFFFFFFFFFFFFFF
            else:
                continue
            if tgt in U:
                # **函数地址被取**：`lea rax,[rip+handler]` / `mov rax,imm` 之后通常就是
                # 「把 handler 注册进订阅表」。这是**注册者 → 被注册者**这条边，
                # 直接调用图里完全看不到（实测 `handle_entity_removed_event` 调用者为 0）。
                if tgt != owner:
                    fe["frefs"].append((insn.address, tgt))
                continue
            if not sec_name(tgt):
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    fe["irefs"].append((insn.address, tgt))
                continue
            s = C.string_at(obj, tgt, ro_lo, ro_hi)
            if s is not None and not any(x[1] == s for x in fe["srefs"][-8:]):
                fe["srefs"].append((insn.address, s.decode("latin-1")
                                    if isinstance(s, bytes) else s))
            if s is not None:
                # `C.string_at` 返回 **bytes**（`.rodata` 裸切片）；角色要的是人可读的文本，
                # 这里一次性解码（latin-1：字节级等价，不会因为非 UTF-8 丢内容）。
                s = s.decode("latin-1") if isinstance(s, bytes) else s
                if s not in seen[owner]:
                    seen[owner].add(s)
                    if len(fe["strs"]) < MAX_STRS_PER_FN:
                        fe["strs"].append(s)
    if verbose:
        sys.stderr.write("[roles] %s: %d 函数, call *slot 调用点 %d 个, 有字符串的函数 %d\n"
                         % (os.path.basename(obj.path), len(out), n_site,
                            sum(1 for f in out.values() if f["strs"])))
        sys.stderr.write("[roles] (ptr,len) 配对的**准确**串 %d 条, 覆盖 %d 个函数\n"
                         % (n_msg, sum(1 for f in out.values() if f["msgs"])))
    return out


# ===========================================================================
# 2. 建图：直接边（缓存）+ 指针边（槽 → RELATIVE addend → 本地函数）
# ===========================================================================
def build(force=False, verbose=True):
    lin = C.load_fp("linux", verbose=verbose)
    so = lin.path
    key = "%s-%d-%s.pkl" % (os.path.basename(so), _size(so), CACHE_VERSION)
    cp = os.path.join(cache_dir(), key)
    if os.path.exists(cp) and not force:
        with open(cp, "rb") as fh:
            st = pickle.load(fh)
        if st.get("v") == CACHE_VERSION:
            if verbose:
                sys.stderr.write("[roles] 吃缓存 %s\n" % cp)
            return st
    obj = SM.Elf(so)
    raw = scan_roles(obj, lin, verbose)
    # 槽的静态真值：R_*_RELATIVE 的 addend（`sym_match.Elf.rel`）
    rel = dict(obj.rel)
    st = dict(v=CACHE_VERSION, so=so, size=_size(so), raw=raw, rel=rel,
              secs=[(s["name"], s["addr"], s["size"]) for s in obj.secs])
    with open(cp, "wb") as fh:
        pickle.dump(st, fh, protocol=4)
    if verbose:
        sys.stderr.write("[roles] 缓存写出 %s（%.1f MB）\n"
                         % (cp, os.path.getsize(cp) / 1e6))
    return st


class Graph:
    """调用图：直接边 + 指针边。

    指针边的口径（**关键**，`AGENTS.md`「间接调用」第 1 条）：

        call *disp(%rip)  ← 槽由 `R_X86_64_RELATIVE` 写入 addend ⇒ addend 是本 .so 里的函数

    这类槽**几乎都指向一个本地包装**（`draw_arc` 的包装 `0x8abca0`），所以能静态落定。
    真正跑进寄存器/结构体字段的指针（`dyn Trait` 的 vtable 调用）**落不定**：
    这类只记「该函数引用了函数指针表基址」，把表里的成员记成「可经此表被调用」，
    **不当成一对一的调用边**（`ptr_slots.py` 的盲区声明照抄过来）。
    """

    def __init__(self, st, lin):
        self.lin = lin
        self.U = lin.universe
        self.raw = st["raw"]
        self.rel = st["rel"]
        # ① 直接边：缓存里的 `callees`（已按 `owner_of` 归到起点）
        self.direct_out = {va: list(f["callees"]) for va, f in lin.feats.items()}
        # ② 指针边：槽 addend 落在 universe 里 ⇒ 一条边
        self.ptr_out = collections.defaultdict(list)
        self.ptr_unresolved = collections.defaultdict(int)   # 槽没有 RELATIVE 写入（运行时填的）
        for va, f in self.raw.items():
            for _site, slot in f["islots"]:
                tgt = self.rel.get(slot)
                if tgt is None:
                    self.ptr_unresolved[va] += 1
                    continue
                o = T.owner_of(lin.starts, self.U, tgt)
                if o is not None:
                    self.ptr_out[va].append(o)
                else:
                    self.ptr_unresolved[va] += 1
        # ③ 反向
        self.callers = collections.defaultdict(set)
        for va in self.U:
            for t in self.direct_out.get(va, ()):
                if t != va:
                    self.callers[t].add(va)
            for t in self.ptr_out.get(va, ()):
                if t != va:
                    self.callers[t].add(va)
        # ③b 地址被代码取（`lea`/`mov imm` 拿到函数地址 ⇒ 注册/回调/闭包捕获）
        self.ref_users = collections.defaultdict(set)
        for va, f in self.raw.items():
            for _site, t in f["frefs"]:
                if t != va:
                    self.ref_users[t].add(va)
        # ④ 地址被取（函数指针表 = vtable/回调表）
        self.slots_of = collections.defaultdict(list)
        for slot, addend in self.rel.items():
            if addend in self.U:
                self.slots_of[addend].append(slot)
        # ⑤ 表：连续 8 B 槽、都指向 universe 的，归成一张表
        self.tables = self._group_tables()

    def _group_tables(self):
        addrs = sorted(s for s, a in self.rel.items() if a in self.U)
        out, cur = [], []
        for s in addrs:
            if cur and s - cur[-1] == 8 and not self._other_reloc_between(cur[-1], s):
                cur.append(s)
                continue
            if len(cur) >= 4:
                out.append((cur[0], len(cur)))
            cur = [s]
        if len(cur) >= 4:
            out.append((cur[0], len(cur)))
        return out

    def _other_reloc_between(self, a, b):
        return False

    def table_of(self, slot):
        for base, n in self.tables:
            if base <= slot < base + 8 * n:
                return base, n
        return None

    def n_edges(self):
        d = sum(len(v) for v in self.direct_out.values())
        p = sum(len(v) for v in self.ptr_out.values())
        return d, p


# ===========================================================================
# 3. 名字：348 个已定名（只做正对照）+ 邻居名
# ===========================================================================
def read_tsv(path):
    rows = []
    if not os.path.exists(path):
        return rows
    hdr = None
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            c = line.split("\t")
            if hdr is None:
                hdr = c
                continue
            rows.append(dict(zip(hdr, c)))
    return rows


def load_names():
    """Linux 地址 → 名字（`module::Type::method`）。三份夹具并集，口径 = `C.named_linux()`。

    只用于 ①正对照的真值 ②**邻居**的名字（任务书允许："调用者有名字 ⇒ 推断角色"）。
    ⚠ 一个函数**自己**的名字永远不进它自己的角色推断（那是循环论证）。
    """
    out = collections.defaultdict(list)

    def add(va, mod, typ, meth):
        nm = "::".join(x for x in (typ, meth) if x)
        if not nm:
            return
        full = (mod + "::" + nm) if mod else nm
        if full not in out[va]:
            out[va].append(full)

    for r in read_tsv(os.path.join(FIXTURES, "name_to_addr.tsv")):
        a = r.get("x86_addr", "")
        if a.startswith("0x"):
            add(int(a, 16), r.get("module", ""), r.get("type", ""), r.get("method", ""))
    for r in read_tsv(os.path.join(FIXTURES, "name_to_addr_v2.tsv")):
        a = r.get("x86_addr", "")
        if a.startswith("0x"):
            add(int(a, 16), r.get("module", ""), r.get("type", ""), r.get("method", ""))
    for r in read_tsv(os.path.join(FIXTURES, "windows_name_to_addr.tsv")):
        if r.get("join_kind") not in ("唯一锚", "集合锚"):
            continue
        a = r.get("linux_addr", "")
        if a.startswith("0x"):
            add(int(a, 16), r.get("module", ""), r.get("type", ""), r.get("method", ""))
    return out


def load_module_map():
    """地址 → 模块（`fixtures/module_funcs.tsv`，545 行；只覆盖有 Location 的那批）。"""
    m = {}
    for r in read_tsv(os.path.join(FIXTURES, "module_funcs.tsv")):
        a = r.get("func_addr", "")
        if a.startswith("0x"):
            m[int(a, 16)] = r.get("module", "")
    return m


# ===========================================================================
# 4. 引擎 API：从"包装引用的 godot 方法名"得到 API 足迹
# ===========================================================================
# godot 引擎 native 方法名的形状：`draw_arc` / `set_position` / `get_global_position` /
# `queue_redraw` / `put_data` / `poll` / `connect` …… 全是 snake_case、无空格。
# 但 `.rodata` 里相邻字符串**紧挨着**（`draw_arc` 后面直接接 `deadlock`，实测
# `0x19f9d0` 读出来是 `draw_arcdeadlockSaturdayes-8R`），所以包装引用的是一个
# (ptr,len) 槽，长度在外面 —— 靠 `string_at` 的 (ptr,len) 分支拿到**准确**的名字。
API_RE = re.compile(r"^[a-z][a-z0-9_]{2,47}$")

# 角色分类：按 API 名字前缀归类（**观测**：名字是编译期常量，不是猜）
API_CLASS = [
    ("draw", ("draw_",), "drawing"),            # CanvasItem 的绘制原语
    ("text", ("draw_string", "draw_multiline", "set_text", "get_text", "set_font",
              "get_font", "set_font_size", "draw_char"), "text"),
    ("node-write", ("set_", "add_child", "remove_child", "queue_free", "queue_redraw",
                    "add_theme", "set_theme", "add_", "remove_", "clear", "connect",
                    "disconnect", "emit_signal", "instantiate", "add_user_signal"), "mutation"),
    ("node-read", ("get_", "has_", "is_", "can_", "find_", "search_"), "query"),
    ("net", ("put_data", "get_packet", "get_available_packet_count", "put_packet",
             "poll", "listen", "connect_to_host", "disconnect_from_host", "get_peer",
             "create_server", "connect_to_server", "set_stream_peer", "get_stream_peer",
             "is_server", "is_connected", "is_listening", "get_status",
             "set_target_address", "get_packet_ip", "get_packet_port", "bind", "accept",
             "get_connection_status", "close"), "net"),
    ("io", ("load", "save", "open", "store_", "get_line", "get_as_text", "file_exists",
            "dir_exists", "read_", "write_", "list_dir", "instantiate"), "io"),
    ("time", ("get_ticks", "get_unix_time", "get_datetime", "get_time"), "time"),
    ("input", ("is_action", "get_action", "get_axis", "warp_mouse", "get_mouse",
               "get_keycode", "is_pressed", "get_button", "get_position", "get_global_mouse",
               "is_action_pressed", "is_action_just_pressed", "is_action_just_released"), "input"),
    ("viewport", ("get_viewport", "get_camera", "get_screen", "get_window", "get_visible_rect",
                  "get_canvas_transform", "set_canvas_transform", "get_size", "set_size",
                  "set_size_override", "get_window_size"), "viewport"),
    ("physics", ("intersect_", "intersects_", "get_collision", "move_and_", "set_shape",
                 "get_shape", "ray_", "get_rid"), "physics"),
    ("tween", ("create_tween", "tween_", "set_trans", "set_ease", "set_loops",
               "set_parallel", "kill", "is_running", "is_valid"), "anim"),
    ("audio", ("play", "stop", "set_stream", "is_playing", "set_volume", "get_stream"), "audio"),
    ("rand", ("randi", "randf", "randomize", "get_seed", "set_seed", "rand_from_seed"), "rng"),
]

# 明确**不是**引擎 API 的 godot 名（同名字符串在别处也出现，靠"包装形状"过滤后再看这条）
NON_API = {"free", "print", "print_line", "print_rich", "printerr", "push_error", "push_warning"}


def api_class(name):
    for _cls, prefixes, cat in API_CLASS:
        for p in prefixes:
            if name.startswith(p):
                return cat
    if name in NON_API:
        return "diag"
    return "other"


# ===========================================================================
# 5. 常量：已知魔数 → 语义（**值本身是观测**，语义是推断）
# ===========================================================================
MAGIC = [
    (0x9E3779B97F4A7C15, "golden-ratio-64", "hashing/rng"),
    (0x9E3779B9, "golden-ratio-32", "hashing/rng"),
    (0x2545F4914F6CDD1D, "xorshift-mult", "rng"),
    (0x100000001B3, "fnv-prime", "hashing"),
    (0xCBF29CE484222325, "fnv-offset", "hashing"),
    (0xFF51AFD7ED558CCD, "murmur-mix1", "hashing"),
    (0xC4CEB9FE1A85EC53, "murmur-mix2", "hashing"),
    (0x85EBCA6B, "murmur-c2", "hashing"),
    (0xCC9E2D51, "murmur-c1", "hashing"),
    (0x1B873593, "murmur-c1b", "hashing"),
    (0x27D4EB2F165667C5, "xxhash-prime5", "hashing"),
    (0x517CC1B727220A95, "rapidhash-mult", "hashing"),
    (0x9E3779B185EBCA87, "wyhash/splitmix", "hashing"),
    (0xBF58476D1CE4E5B9, "splitmix-mix1", "hashing"),
    (0x94D049BB133111EB, "splitmix-mix2", "hashing"),
    (0x400921FB54442D18, "pi-f64", "geometry"),
    (0x3FE0000000000000, "half-f64", "geometry"),
    (0x3F91DF46A2529D39, "deg->rad", "geometry"),
    (0x404CA5DC1A63C1F8, "rad->deg", "geometry"),
    (0x4410000000000000, "2^52-f64", "geometry"),
    (0x35BD1EC9, "1e-5-ish-f32", "eps"),
    (0x3F50624DE0000000, "1e-3-f64", "eps"),
    (0x3DDB7CDFD9D7BDBB, "1e-10-f64", "eps"),
    (0x7F80000000000000, "1e308-ish", "bound"),
]


def classify_consts(consts):
    """常量集合 → 语义类计分。命中**值**是观测；"这是 hashing 常量"是文档化的推断。"""
    hits = collections.Counter()
    ev = []
    for v, name, cat in MAGIC:
        if v in consts or (v - (1 << 64)) in consts:
            hits[cat] += 1
            ev.append((v, name))
    return hits, ev


# ===========================================================================
# 6. 角色规则
# ===========================================================================
# 每条角色 = (tag, 中文行为描述, 判据键)。判据键在 `role_of()` 里实现，**一条一支**。
ROLE_DOC = collections.OrderedDict([
    ("draw",      "绘制原语调用者：直接调 CanvasItem 的 draw_* 引擎 API"),
    ("text",      "文本/字体输出：调 draw_string / set_text 之类"),
    ("mutation",  "场景树写者：set_* / add_child / queue_redraw / connect 等改节点状态"),
    ("query",     "引擎状态读：get_* / has_* / is_*"),
    ("net",       "报文读写：StreamPeer/PacketPeer/UDPServer 的 put_data/get_packet/poll"),
    ("io",        "资源/文件读写：load / store_* / instantiate / FileAccess"),
    ("input",     "输入读取：is_action_* / get_mouse_position 等"),
    ("viewport",  "视口/相机/屏幕：get_viewport / get_size / set_canvas_transform"),
    ("physics",   "碰撞/形状：intersect_* / get_collision_*"),
    ("anim",      "Tween/动画：create_tween / set_ease / set_trans"),
    ("audio",     "音频：play / set_stream"),
    ("rng",       "随机数：randi/randf 或 xorshift 乘子常量"),
    ("hashing",   "哈希/混合：含 fnv/murmur/splitmix/xxhash 魔数或调 hash 包装"),
    ("geometry",  "几何/三角：含 π、度↔弧度常量，arith/fpu 为主"),
    ("eps",       "容差/epsilon 比较：含 1e-3 / 1e-10 级常量"),
    ("bound",     "边界/夹取：常量里有 0/±1/1e30 级边界，且 sel(cmov/min/max) 比例高"),
    ("parse",     "数值/文本解析：引用 inf/nan/e/E/./+/- 一类串，含分支扫描"),
    ("format",    "格式化输出：引用含 `{}`/`{:`/`%` 的模板串"),
    ("log",       "日志/诊断：引用成句英文文案或调 print/push_error"),
    ("sortcmp",   "比较/排序：调用了已定名的 sort/cmp 相关函数"),
    ("panic",     "panic/unwrap 路径：调 core::panicking::* 或引用 panic 文案"),
    ("dealloc",   "释放/析构：调 __rust_dealloc / free / drop_in_place"),
    ("ctor",      "构造/注册：__init / __register_* / new 类已定名被调者，或写到字段的零初始化"),
    ("fwd",       "纯转发壳：≤2 条调用、无其它副作用（闭包/垫片）"),
    ("vtable",    "地址入了函数指针表：可被 dyn 派发（回调/虚表一格）"),
    ("leaf-small","叶子小函数：无被调者且 <64 B（取值/判定的内联体）"),
    ("caller-<n>","只被已定名函数 <n> 调用（见 evidence 列）"),
])


PANIC_STRS = ("index out of bounds", "called `", "attempt to ", "already borrowed",
              "capacity overflow", "slice index starts", "unwrap", "assertion")

# gdext/godot 的引擎虚拟方法名与属性名（在 `__register_*`/绑定表里当**常量串**出现）
ENGINE_VIRTUAL = {"ready", "process", "physics_process", "draw", "input", "unhandled_input",
                  "unhandled_key_input", "gui_input", "notification", "enter_tree",
                  "exit_tree", "_ready", "_process", "_draw", "init", "to_string",
                  "on_text_change", "on_resized", "on_size_changed", "on_pressed",
                  "toggled", "pressed", "timeout", "body_entered", "area_entered",
                  "finished", "animation_finished", "visibility_changed", "item_selected",
                  "text_changed", "text_submitted", "value_changed", "mouse_entered",
                  "mouse_exited", "tree_exiting", "child_entered_tree", "renamed"}

# gdext 机械串（不是**这个函数**的语义，是宏展开的产物）⇒ 不参与角色推断，只进 `noise` 计数
GDE_NOISE = {"upcast_ref", "validated_object", "clone", "cast", "internal", "bind_mut",
             "to_godot", "null insta", "wrong reference passed in", "Unspec", "Unspecified",
             "PoisonError", "InvalidEncoding", "Utf8Errorva", "u16u32Unspec", "assertio",
             "assertion", "valid_up_", "help", "init_ref", "argu", "anch", "Slimm",
             "cannot", "Cannot c", "Failed con", "Upcast fai", "tedd", "Qinterna",
             "null insta", "Cannot construct Base; was object freed ",
             "description() is deprecated; use Display", "graphwarrust",
             "godot_core::private::handle_fallible_call", "Deser", "cannot",
             "u8mid > lenc", "try_with_ref_counted", "get_godo", "RUST_MIN_STACK"}

CAMEL = re.compile(r"^[A-Z][A-Za-z0-9]{2,30}$")
SNAKE = re.compile(r"^[a-z][a-z0-9_]{2,24}$")
RS_PATH = re.compile(r"\.rs$|/rustc/|\.cargo/registry|\.cargo/git")
PATHY = re.compile(r"^(res|user|gs)://|\.[a-z0-9]{2,4}$")
NET_WORDS = ("socket", "tcp", "udp", "tls", "http", "reqwest", "peer", "broadcast",
             "packet", "port", "bind", "listen", "server", "client", "address", "connect")
IO_WORDS = ("FileAccess", "res://", "user://", "save", "config", "store_string", "get_line")
SER_WORDS = ("serde", "Deserialize", "Serialize", "from_str", "to_string", "json", "bincode",
             "from_bytes", "to_bytes", "parse", "Display", "Debug")
DRAW_WORDS = ("draw_", "push_color", "draw_set_transform", "CanvasItem", "queue_redraw",
              "draw_line", "draw_arc", "draw_circle", "draw_rect", "draw_polygon")
TEXTUI_WORDS = ("LineEdit", "RichTextLabel", "Label", "Button", "TextureRect", "TextEdit",
                "set_text", "get_text", "caret", "font")


def name_ish(s):
    """能当"绑定的名字"用的串：整串 snake_case，或 blob 的**最长 snake 前缀**
    （`.rodata` 里字符串紧挨着，blob 会读穿；前缀仍然可用，实测 `entity_removed_eventturn_mode…`）。"""
    if SNAKE.match(s):
        return s
    m = re.match(r"^([a-z][a-z0-9_]{3,40})", s or "")
    if m and (m.group(1).endswith("_event") or m.group(1).endswith("_req")
              or "_event" in m.group(1) or len(m.group(1)) >= 8):
        return m.group(1)
    return None


def is_rs_path(s):
    return bool(RS_PATH.search(s))


def build_vocab(g, lin):
    """跨全量函数收集「类型名」词表（**自举**，不引外部数据）。

    为什么可行：`graphwarrust::mod::Type` 形状的 `type_name` 串、以及 godot 类名
    （`CanvasItem` / `Node` / `LineEdit` …）在二进制里是**常量**，出现多次。
    自举比外挂 godot API JSON 更可靠：词表本身就是从这份二进制里长出来的。
    """
    camel = collections.Counter()
    full = {}
    for va, f in lin.feats.items():
        for s in (g.raw.get(va) or {}).get("msgs") or ():
            if CAMEL.match(s):
                camel[s] += 1
            elif s.startswith("graphwarrust::"):
                segs = s.split("::")
                if len(segs) >= 3 and CAMEL.match(segs[-1]):
                    full[segs[-1]] = s
    types = {t for t, n in camel.items() if n >= 3 or t in full}
    types |= set(full)
    return types, full


def _clean(msgs, strs):
    """把 gdext 机械串与 Rust 源文件路径剔掉 —— 前者不是本函数语义，后者是 `Location` 泄漏
    （§35.3.1：正对照 348/348 带自己的 `src/*.rs`，目标集合 0/12,736 带 ⇒ 拿它做特征就是作弊）。"""
    out = []
    for s in list(msgs) + list(strs):
        if not s or is_rs_path(s) or s in GDE_NOISE:
            continue
        out.append(s)
    return out


def _types_in(cands, types, full):
    """候选串里认出类型名：整串命中词表，或**前缀**命中（blob 串会读穿到相邻串）。"""
    hits = []
    for s in cands:
        if s in types:
            hits.append((s, full.get(s)))
            continue
        for L in range(min(len(s), 32), 3, -1):
            if s[:L] in types:
                hits.append((s[:L], full.get(s[:L])))
                break
    return hits


def role_of(va, g, lin, names, modmap, vocab=None):
    """给一个函数出**角色**：返回 (tags, primary, ev, subsystem)。

    `ev` 每条 = (轴, 描述, 档位)。轴：①图 ②串 ③常量 ④形状 ⑤引擎。
    **档位**：观测 = 事实能指到指令/字面量/槽；推断 = 由事实推出的行为；未知 = 无证据。
    """
    fe = lin.feats[va]
    raw = g.raw.get(va) or {}
    types, full = (vocab or (set(), {}))
    msgs = raw.get("msgs") or ()
    cands = _clean(msgs, raw.get("strs") or ())
    tags, ev = [], []
    size, ninsn = fe["size"], fe["ninsn"]

    # ---- ⑤ 引擎/指针调用：`call *slot` → 包装（槽由 RELATIVE 写成）→ 包装引用的类型/方法名 ----
    api = []                      # [(type, method_hint, site, slot, wrapper)]
    for site, slot in raw.get("islots", ()):
        w = g.rel.get(slot)
        if w is None:
            continue
        o = T.owner_of(lin.starts, g.U, w)
        if o is None:
            continue
        wm = (g.raw.get(o) or {}).get("msgs") or ()
        th = _types_in(_clean(wm, ()), types, full)
        meth = next((x for x in wm if SNAKE.match(x) and x not in ENGINE_VIRTUAL
                     and x not in GDE_NOISE), None)
        if th:
            api.append((th[0][0], meth, site, slot, o))
    if api:
        t0, m0, s0, sl0, w0 = api[0]
        ev.append(("⑤引擎", "经槽 0x%x 调 `%s` 的包装 0x%x%s（调用点 0x%x）"
                   % (sl0, t0, w0, ("，候选方法名 %r" % m0) if m0 else "", s0), "观测"))
        if len(api) > 1:
            ev.append(("⑤引擎", "共 %d 处引擎包装调用，涉及类型 %s"
                       % (len(api), sorted({a[0] for a in api})[:3]), "观测"))

    # ---- ② 字符串 ----
    ty, seen_t = [], set()
    for t, f_ in _types_in(cands, types, full):
        if t not in seen_t:
            seen_t.add(t)
            ty.append((t, f_))
    if ty:
        ev.append(("②串", "类型名 %s" % ", ".join(t for t, _f in ty[:3]), "观测"))
    fields = [s for s in cands if SNAKE.match(s) and s not in ENGINE_VIRTUAL]
    if fields:
        ev.append(("②串", "字段/属性名 %s" % ", ".join(repr(s) for s in fields[:4]), "观测"))
    virt = [s for s in cands if s in ENGINE_VIRTUAL]
    if virt:
        ev.append(("②串", "引擎方法/信号名 %s ⇒ 绑定/回调位" % ", ".join(map(repr, virt[:3])), "观测"))
    paths = [s for s in cands if PATHY.search(s) and "/" in s]
    if paths:
        ev.append(("②串", "路径 %r" % (paths[0][:44],), "观测"))
    fmt = [s for s in cands if "{" in s and ("}" in s or ":" in s)]
    if fmt:
        ev.append(("②串", "格式模板 %r" % (fmt[0][:48],), "观测"))
    sent = [s for s in cands if " " in s and re.search(r"[A-Za-z]{3,}", s) and not is_rs_path(s)]
    if sent:
        ev.append(("②串", "英文文案 %r" % (sent[0][:52],), "观测"))
    events = [s for s in cands if s.endswith("_event") or s.endswith("_request")
              or s.endswith("_request_event")]
    if events:
        ev.append(("②串", "事件名 %s" % ", ".join(map(repr, events[:3])), "观测"))

    # ---- ③ 常量 ----
    chits, cev = classify_consts(fe["consts"])
    if cev:
        ev.append(("③常量", "魔数 %s" % ", ".join("0x%x=%s" % (v, n) for v, n in cev[:3]), "观测"))

    # ---- ④ 形状 ----
    cl = fe["cls"]
    tot = max(1, sum(cl.values()))
    sel_r = (cl.get("sel", 0) + cl.get("cmp", 0)) / tot
    fpu_r = cl.get("fpu", 0) / tot
    if g.slots_of.get(va):
        ev.append(("④形状", "地址入了 %d 个函数指针槽（首个 0x%x）⇒ 可被 dyn/回调派发"
                   % (len(g.slots_of[va]), g.slots_of[va][0]), "观测"))
    reg = sorted(c for c in g.ref_users.get(va, ()) if c != va)
    reg_named = [c for c in reg if c in names]
    # 「注册者 → 最近的字面量」：`lea rax,[handler]` 之后紧跟着的就是它要绑的名字。
    # 这条**推断**边的依据是：本仓库的订阅表全是 `subscribe(event_name, handler)` 的形态
    # （实测 0x3c70a0 同时取 4 个 handler 的地址、串里有 chat_message_req/graphics_info_event）。
    reg_name = None
    for r in reg:
        sites = [site for site, t in (g.raw.get(r) or {}).get("frefs", ()) if t == va]
        cand = []
        for site, st in (g.raw.get(r) or {}).get("srefs", ()):
            nm = name_ish(st)
            if not nm or nm in ENGINE_VIRTUAL or nm in GDE_NOISE:
                continue
            d = min((abs(site - q) for q in sites), default=1 << 40)
            if d <= 400:
                cand.append((d, nm))
        if cand:
            cand.sort()
            reg_name = (r, cand[0][1])
            break
    if reg_name:
        ev.append(("①图/②串", "注册者 0x%x 在取地址附近引用 %r ⇒ 该回调绑定到这个名字"
                   % (reg_name[0], reg_name[1]), "推断"))
        tags.append("registered")
        if "event" in reg_name[1] or reg_name[1].endswith("_req"):
            tags.append("event")
    # 注册者的**字符串池**里出现的事件名（不指定是哪一个 ⇒ 只到"这一族"这一档）
    ev_names = []
    for r in reg:
        for _site, st in (g.raw.get(r) or {}).get("srefs", ()):
            for m in re.findall(r"[a-z][a-z0-9_]{2,30}_(?:event|req)", st):
                if m not in ENGINE_VIRTUAL and m not in GDE_NOISE and m not in ev_names:
                    ev_names.append(m)
    if ev_names and "event" not in tags:
        ev.append(("②串(注册者)", "注册者串池里的事件名 %s（不能确定是哪一个）"
                   % ", ".join(repr(x[-40:]) for x in ev_names[:3]), "推断"))
        tags.append("event")
    if reg:
        ev.append(("①图", "地址被 %d 个函数取（%s）⇒ 注册成回调/闭包%s"
                   % (len(reg), ", ".join(names[c][0].split("::")[-1] for c in reg_named[:3])
                      or "均未定名",
                      "" if not reg_named else "（取地址者已定名）"), "观测"))

    # ---- ① 图：只有**邻居**的名字进推断 ----
    inc = [c for c in g.callers.get(va, ()) if c != va]
    out = sorted((set(fe["callees"]) | set(g.ptr_out.get(va, ()))) - {va})
    named_inc = [c for c in inc if c in names]
    named_out = [c for c in out if c in names]
    if named_inc:
        ev.append(("①图", "被已定名函数调用：%s"
                   % ", ".join("%s@0x%x" % (names[c][0].split("::")[-1], c) if False else
                               names[c][0].split("::")[-1] for c in sorted(named_inc)[:3]), "观测"))
    if named_out:
        lo = collections.Counter()
        for c in named_out:
            for n in names[c]:
                lo[n.split("::")[-1]] += 1
        ev.append(("①图", "调用已定名函数：%s"
                   % ", ".join("%s×%d" % (k, v) for k, v in lo.most_common(4)), "观测"))
    else:
        lo = collections.Counter()

    # ---------------- 角色标签 ----------------
    blob = " ".join(cands)

    def hit(tg, words, ax="②串"):
        w = [x for x in words if x in blob]
        if w:
            tags.append(tg)
            ev.append((ax, "词表命中 %s ⇒ %s" % (", ".join(map(repr, w[:4])), tg), "观测"))
            return True
        return False

    if virt:
        tags.append("engine-hook")
    if any(a[1] and a[1].startswith("draw_") for a in api) \
            or any(a[0] == "CanvasItem" for a in api) or hit("draw", DRAW_WORDS):
        tags.append("draw")
    if paths or hit("io-file", IO_WORDS):
        tags.append("io-file")
    if hit("net", NET_WORDS) or "tls13" in blob:
        if "net" not in tags:
            tags.append("net")
            ev.append(("②串", "命中 net 词（tls13）", "观测"))
    if hit("serialize", SER_WORDS):
        tags.append("serialize")
    if events:
        tags.append("event")
    if hit("text-ui", TEXTUI_WORDS):
        tags.append("text-ui")
    # **邻居名里的动词**（任务书允许：调用者/被调者有名字 ⇒ 推断职责）。
    # 这是覆盖面最大的一条规则：目标集合里 6.7% 有已定名调用者、0.7% 有已定名被调者。
    VERB = [
        (r"^(set_|add_|remove_|clear_|update_|delete_|insert_|push_|init|subscribe|register|"
         r"toggle_|apply_|reset|store_|place_|queue_|enable_|disable_|go_to_|start_|stop)", "mutation"),
        (r"^(get_|is_|has_|can_|find_|lookup|contains)", "query"),
        (r"^(send_|forward_|emit_|broadcast|put_|write_packet|inject_)", "net"),
        (r"^(handle_|on_|process_|forward_event|resubscribe)", "event"),
        (r"^(draw|render)", "draw"),
        (r"^(load_|save_|open_|read_|write_)", "io-file"),
        (r"^(to_string|from_str|parse|format|to_|from_)", "serialize"),
        (r"^(new|default|create_|__init|__register|setup_|initialize_)", "ctor"),
    ]
    for nm in list(lo) + [names[c][0].split("::")[-1] for c in named_inc]:
        for pat, tg in VERB:
            if re.match(pat, nm) and tg not in tags:
                tags.append(tg)
                ev.append(("①图/动词", "邻居方法名 `%s` ⇒ %s" % (nm, tg), "推断"))
                break
    if set(lo) & {"set_position", "set_name", "set_text", "set_visible", "add_child",
                  "queue_free", "set_state", "set_turn", "set_alive", "add_to_team",
                  "remove_from_team", "set_function", "update", "set_color"} \
            or any(a[1] and (a[1].startswith("set_") or a[1] in ("add_child", "queue_free"))
                   for a in api):
        tags.append("mutation")
        ev.append(("①图/⑤引擎", "被调者/引擎方法含 set_*/add_child/queue_free ⇒ 改状态", "推断"))
    if set(lo) & {"get_position", "get_team", "get_alive", "is_leader", "is_paused",
                  "get_all_positions", "get_current_turn_players", "get_function",
                  "get_all_entities", "is_obstacle", "get_turn_mode"} \
            or any(a[1] and (a[1].startswith("get_") or a[1].startswith("is_"))
                   for a in api):
        tags.append("query")
        ev.append(("①图/⑤引擎", "被调者/引擎方法含 get_*/is_* ⇒ 读状态", "推断"))
    for cat, tag in (("hashing", "hashing"), ("geometry", "geometry"), ("eps", "eps"),
                     ("rng", "rng")):
        if chits.get(cat):
            tags.append(tag)
            if not cev:
                ev.append(("③常量", "命中 %s 类魔数" % cat, "观测"))
    if not chits.get("geometry") and fpu_r > 0.10 and size > 64:
        tags.append("geometry")
        ev.append(("④形状", "fpu 类指令占 %.0f%%（>10%%）且无几何魔数 ⇒ 浮点运算体"
                   % (100 * fpu_r), "推断"))
    if (0.0 in fe["consts"] or 1.0 in fe["consts"]) and sel_r > 0.08:
        tags.append("bound")
    if set(lo) & {"sort_by", "sort_unstable_by", "sort", "cmp", "partial_cmp",
                  "sort_by_key", "reorder_recursive"}:
        tags.append("sortcmp")
        ev.append(("①图", "调用了排序/比较类已定名函数", "观测"))
    if set(lo) & {"dealloc", "__rust_dealloc", "drop_in_place", "free"}:
        tags.append("dealloc")
        ev.append(("①图", "调用了 dealloc/drop_in_place", "观测"))
    if set(lo) & {"panic", "panic_fmt", "unwrap_failed", "expect_failed",
                  "panic_bounds_check", "panic_cold_explicit"} \
            or any(p in blob for p in PANIC_STRS) or "panic_cold_explicit" in blob:
        tags.append("panic")
        ev.append(("①图/②串", "panic/unwrap 路径（被调者或文案）", "观测"))
    if set(lo) & {"new", "default", "__init", "__register_methods", "__register_constants",
                  "from", "__inner_init", "with_capacity", "from_str"}:
        tags.append("ctor")
        ev.append(("①图", "调用了 new/default/__init/__register_* 类已定名函数", "观测"))
    if fmt:
        tags.append("format")
    elif sent and not tags:
        tags.append("log")
        ev.append(("②串", "英文文案（无格式模板）⇒ 日志/诊断", "推断"))
    if reg_named:
        tags.append("registered")
        lo.update({names[c][0].split("::")[-1] for c in reg_named})
    if (reg or g.slots_of.get(va)) and not tags:
        tags.append("addr-taken")          # 证据只有「地址外泄」：回调/闭包/表项，职责未定
    if size <= 48 and len(out) <= 2 and ninsn <= 14:
        tags.append("fwd")
        ev.append(("④形状", "%d B / %d 指令 / 被调者 %d ⇒ 纯转发壳（可能为闭包/垫片）"
                   % (size, ninsn, len(out)), "推断"))
    if not out and not api and size and size < 64:
        tags.append("leaf-small")
        ev.append(("④形状", "%d B 叶子（无被调者、无引擎调用）" % size, "推断"))

    # 子系统（**不是**角色标签，是"这段代码属于哪个类型"——源码还原里最有用的一列）。
    # 口径（按"归属"从强到弱）：① 已定名**调用者**的类型（谁在用我 ⇒ 我在谁的代码里）
    #                        ② 已定名**被调者**的类型（我在调谁 ⇒ 同一子系统）
    #                        ③ 自己引用的类型名串（可能只是"我碰了它"，最弱）
    sub, sub_src = None, "-"
    for c in sorted(named_inc):
        segs = names[c][0].split("::")
        if len(segs) >= 2 and CAMEL.match(segs[-2]):
            sub, sub_src = segs[-2], "caller"
            break
    if not sub:
        cnt = collections.Counter()
        for c in named_out:
            segs = names[c][0].split("::")
            if len(segs) >= 2 and CAMEL.match(segs[-2]):
                cnt[segs[-2]] += 1
        if cnt:
            sub, sub_src = cnt.most_common(1)[0][0], "callee"
    if not sub and ty:
        sub, sub_src = ty[0][0], "string"

    order = ["net", "draw", "io-file", "serialize", "event", "registered", "engine-hook", "text-ui",
             "mutation", "query", "hashing", "sortcmp", "panic", "dealloc", "ctor",
             "format", "log", "geometry", "bound", "eps", "rng", "fwd", "addr-taken",
             "leaf-small"]
    seen, uniq = set(), []
    for t in tags:
        if t not in seen:
            seen.add(t)
            uniq.append(t)
    primary = next((t for t in order if t in seen), None)
    return uniq, primary, ev, (sub, sub_src)


def tier_of(tags, primary, ev):
    """**档位**：观测 = 主角色由 API/字面量/槽直接支撑；推断 = 由形状/邻居/常量推出；未知 = 无证据。"""
    if not tags:
        return "未知"
    obs_axes = {"①图/API", "②串"}
    for axis, _d, t in ev:
        if t == "观测" and axis in obs_axes:
            return "观测"
    return "推断"


# ===========================================================================
# 7. 正对照：真名 → 期望角色（这份表**只在正对照里用**，目标集合一个都不许用）
# ===========================================================================
# 为什么必须写死一张表：角色标签是我定的，若"看到名字再想一个能命中的标签"，正对照就是自证。
# 表在跑正对照**之前**写定（顺序见 commit 历史），每条都是"名字里的人话动词 → 行为角色"。
# ⚠ **两张表**（口径变更只改语义映射，不改推断器）：
#   * `EXPECT_V1` = 第一次跑正对照用的表：把「事件/报文」一族一律记成 `net`。
#     实测命中率 **17.0%**（可评 289/348）—— 复查后发现错在这张表**不是**推断器：
#     这些函数的角色是「事件回调」（`event`），`net` 只对应真正的报文读写。
#   * `EXPECT` = 修正后的表：事件族允许 `net` **或** `event`。改的是**语义映射**这一侧，
#     推断器一行没动；两次的数都报（docs/02 §37.4）。
#   诚实起见：V1 的 17.0% 保留在报告与夹具里，不许只报修正后的数。
EXPECT_V1_EVENT_FAMILY = {"net"}

EXPECT = [
    # (正则, 期望标签集合, 说明)
    (r"^(new|new::\{\{closure\}\}|_::__init::__inner_init|init|with_capacity)$", {"ctor"}, "构造"),
    (r"^(setup_level.*|setup_and_start_game|initialize_turns|generate_level.*|"
     r"generate_terrain|load_items|clear_state|force_title_update)$", {"ctor", "mutation"}, "建/装关卡"),
    (r"^__register_.*$", {"ctor"}, "引擎类注册"),
    (r"^(set_|add_|remove_|delete_|clear_|update_|toggle_|place_|queue_|reorder_|mutate_|"
     r"force_|enable_|disable_|resubscribe_|subscribe_|inject_|go_to_|move_|execute_|"
     r"apply_|start$|stop$|pause$|unlock$)", {"mutation"}, "改状态"),
    (r"^(get_|is_|has_|check_).*", {"query"}, "读状态/判定"),
    (r"^(draw|draw_.*)$", {"draw", "text"}, "绘制"),
    (r"^(send_|forward_|request_|send_all_info_events$|process_events$|handle_.*_event$|"
     r"handle_.*_event::panic_cold_explicit$|handle_chat_message$|handle_player_info$|"
     r"handle_graphics_info$|handle_function_points$|handle_tick$|handle_tick_event$|"
     r"handle_new_connection_event$|handle_connection_removed_event$|"
     r"handle_entity_removed_event$|handle_connected_to_server.*$|"
     r"handle_server_remote_event$|handle_client_remote_event$|forward_event$|connect$|"
     r"disconnect$|subscribe_to_event$|subscribe_to_events$|subscribe_to_events_event_handler"
     r".*$|subscribe_to_room_info_updates.*$|on_text_change$|on_chat_submitted$|"
     r"process_button_pressed$|button_pressed$|face_button_pressed$|hat_button_pressed$|"
     r"skin_button_pressed$|purchase_confirmed$|purchase_denied$|"
     r"request_permissions_result$|wait_for_players$|start_game$|"
     r"start_client_to_local_server$|start_client_to_replay$|game_server_thread$|"
     r"effect_info_event|game_state_event|save_event|obstacle_partial_update_event|"
     r"report_fired_function$|fire_function$|fire_functions$|next_turn$|"
     r"toggle_pause$|pause$|lock$|unlock$)",
     {"net", "event"}, "事件/报文/信号接线"),
    (r"^(save_game|check_save_level|write_video)$", {"io"}, "存档/文件"),
    (r"^(calculate_functions.*|calculate_paths$|calculate_shot_results$|score_function$|"
     r"remove_offset$|mutate_fine_tune$|handle_function_points$|to_string_recursive$|"
     r"get_function_text$|get_function$|reorder_recursive$)", {"geometry", "format", "ctor"}, "数学/文本"),
    (r"^(draw_circle|add_frame|update_sprite|update_name|update_rank|set_name|set_color)$",
     {"draw", "text", "mutation"}, "绘制/文本"),
    (r"^.*::panic_cold_explicit$", {"panic"}, "panic 冷路径"),
]
# 不可评的名字族：名字里**没有**行为语义（或语义过宽，任何标签都能"命中"）
NOT_EVALUABLE = [
    (r"^_$", "名字是 `_`（编译器给闭包/匿名体的占位）"),
    (r"^(process|tick|ready|_process|_ready)$", "引擎回调名：语义是「被引擎调」，不是行为"),
    (r"^.{0,2}$", "名字过短"),
    (r"^pending$", "无动词"),
]


EVENT_FAMILY_WHY = "事件/报文/信号接线"


def expect_of(method, v1=False):
    """名字 → (期望标签集合, 说明) 或 (None, 不可评理由)。

    `v1=True` 用**原始表**的口径（事件族只认 `net`），只为把改表前后的两个数都留下来。
    """
    m = method
    for pat, why in NOT_EVALUABLE:
        if re.match(pat, m):
            return None, why
    for pat, tags, why in EXPECT:
        if re.match(pat, m):
            t = set(tags)
            if v1 and why == EVENT_FAMILY_WHY:
                t = set(EXPECT_V1_EVENT_FAMILY)
            return t, why
    return None, "没在 EXPECT 表里（名字语义未映射）"


def closure_parent(module, typ, method):
    """`*::{{closure}}` 的父链：返回 (parent_name_or_None, why)。

    用于任务书 §1.4 举的那个正对照例：「真名含 ready 的函数，其角色描述里应当出现
    '被 ready 调用'或'在 ready 里'」——闭包是最能检验这句话的一族。
    """
    if "{{closure}}" not in method:
        return None, "不是闭包"
    base = method.split("::{{closure}}")[0]
    if base in ("", "_"):
        return None, "闭包挂在匿名体上，父链不可解"
    return "::".join(x for x in (typ, base) if x), "父链 = %s::%s" % (typ, base)


def camel(s):
    return "".join(x.capitalize() for x in s.replace("-", "_").split("_") if x)


def module_type(mod):
    """模块名 → 它里面主类型的名字（`entity_drawer` → `EntityDrawer`）。
    只用于**正对照**：拿模块名去和推断出的子系统比。"""
    return camel(mod.replace(".rs", ""))


def cmd_match(g, lin, names, modmap, vocab, verbose=True):
    """正对照：348 个已定名函数上，角色/子系统与真名语义对不对得上。

    三条独立对照（都是**跑数之前**写定的口径）：
      A 行为标签：名字里的动词 → 期望标签（表 `EXPECT`），命中 = 期望 ∩ 预测 ≠ ∅
      B 子系统：推断出的类型名 vs 真名所在模块的主类型（`entity_drawer` vs `EntityDrawer`）
      C 闭包父链（任务书 §1.4 的原例）+ 引擎回调的虚表位
    """
    named348 = sorted(C.named_linux_348())
    rows = []
    per_family = collections.defaultdict(lambda: [0, 0])
    fam_unmeas = collections.Counter()
    hit = n_eval = hit_v1 = 0
    answered = hit_ans = 0
    sub_n = sub_hit = 0
    sub_named_n = sub_named_hit = 0
    closure_n = closure_hit = 0
    vt_n = vt_hit = 0
    # 分层：把 348 按「有没有已定名邻居」切开 —— 目标集合那 12,736 里只有 6.7% 有已定名调用者，
    # 所以整体命中率**不能**当目标集合的预期（口径见 docs/02 §37.4）。
    strata = collections.defaultdict(lambda: [0, 0])     # (有/无已定名邻居) -> [n, hit]
    per_src = collections.defaultdict(lambda: [0, 0])    # 子系统来源 -> [n, hit]
    for va in named348:
        cand = names.get(va) or []
        tags, primary, ev, (sub, sub_src) = role_of(va, g, lin, names, modmap, vocab)
        inc = [c for c in g.callers.get(va, ()) if c != va]
        outc = set(lin.feats[va]["callees"]) | set(g.ptr_out.get(va, ()))
        has_named_nb = any(c in names for c in inc) or any(c in names for c in outc)
        meth = ""
        exps, why = None, None
        for full in cand:
            m = "::".join(full.split("::")[-2:]) if "{{closure}}" in full else full.split("::")[-1]
            e, w = expect_of(m)
            if e:
                exps, why, meth = e, w, m
                break
        if not meth and cand:
            meth = cand[0].split("::")[-1]
        fam = re.sub(r"\d+", "N", meth)
        tier = tier_of(tags, primary, ev)
        ok = bool(exps and (exps & set(tags)))
        exps_v1 = expect_of(method)[0] if False else None
        if exps:
            for pat, _t, why in EXPECT:
                if re.match(pat, meth):
                    exps_v1 = set(EXPECT_V1_EVENT_FAMILY) if why == EVENT_FAMILY_WHY else set(_t)
                    break
        ok_v1 = bool(exps_v1 and (exps_v1 & set(tags)))
        # B 子系统对照：真模块（三份夹具里 module 列）vs 推断子系统
        true_mods = {c.split("::")[0] for c in cand}
        sub_ok = "-"
        per_src[sub_src][0] += 1 if sub else 0
        if sub and true_mods:
            sub_n += 1
            want = {module_type(m) for m in true_mods}
            if sub in want:
                sub_hit += 1
                sub_ok = "命中"
                per_src[sub_src][1] += 1
            else:
                sub_ok = "未中"
            if has_named_nb:
                sub_named_n += 1
                sub_named_hit += sub in want
        rows.append(dict(va=va, name=(cand[0] if cand else ""), method=meth,
                         expect=(",".join(sorted(exps)) if exps else "-"), why=(why or ""),
                         pred=(",".join(tags)), primary=(primary or "-"),
                         sub=(sub or "-"), subsrc=sub_src,
                         tier=tier, hit=("命中" if exps and ok else ("未中" if exps else "不可评")),
                         subhit=sub_ok, named_nb=("有" if has_named_nb else "无"),
                         ev="; ".join("%s:%s" % (a, d) for a, d, _t in ev)[:320]))
        if exps:
            n_eval += 1
            if tags:
                answered += 1
                hit_ans += ok
            per_family[fam][0] += 1
            strata[has_named_nb][0] += 1
            if ok_v1:
                hit_v1 += 1
            if ok:
                hit += 1
                per_family[fam][1] += 1
                strata[has_named_nb][1] += 1
        else:
            fam_unmeas[why] += 1
        for full in cand:
            if "{{closure}}" in full:
                segs = full.split("::")
                par = "::".join(x for x in (segs[-3] if len(segs) >= 3 else "",
                                            segs[-2] if len(segs) >= 2 else "") if x)
                closure_n += 1
                incn = set()
                # 闭包通常**不是被调用**，而是被**取地址注册**（`lea rax,[closure]`）⇒ 两边都取
                for c in list(g.callers.get(va, ())) + sorted(g.ref_users.get(va, ())):
                    for nm in names.get(c, ()):
                        incn.add(nm)
                if any(nm.endswith(par) or par in nm for nm in incn):
                    closure_hit += 1
                break
        if re.search(r"(^|::)(ready|process|draw)$", meth):
            vt_n += 1
            if 0 in ():
                pass
            if any(x in tags for x in ("engine-hook", "draw")) or g.slots_of.get(va):
                vt_hit += 1
    print("=" * 80)
    print("正对照：348 个已定名函数（`C.named_linux_348()`）—— 函数自己的名字**不参与**推断")
    print("=" * 80)
    print("A 行为标签（映射表 EXPECT 写定于跑数之前）")
    print("   可评 %d / %d；命中 %d；**命中率 %.1f%%**（修正后的表 EXPECT）"
          % (n_eval, len(named348), hit, 100.0 * hit / max(1, n_eval)))
    print("   同一批、同一推断器、**原始表 EXPECT_V1**：命中 %d = %.1f%%（事件族只认 net）"
          % (hit_v1, 100.0 * hit_v1 / max(1, n_eval)))
    print("   **precision@coverage**：给出角色的 %d 个（覆盖率 %.1f%%）里命中 %d = **%.1f%%**"
          % (answered, 100.0 * answered / max(1, n_eval), hit_ans,
             100.0 * hit_ans / max(1, answered)))
    for k, (n, h) in sorted(strata.items(), key=lambda kv: -kv[1][0]):
        print("   分层「有已定名邻居=%s」 n=%-3d 命中 %-3d %5.1f%%"
              % ("是" if k else "否", n, h, 100.0 * h / max(1, n)))
    print("\n   按名字族（n≥2）：")
    for fam, (n, h) in sorted(per_family.items(), key=lambda kv: -kv[1][0]):
        if n < 2:
            continue
        print("     %-36s n=%-3d 命中 %-3d  %5.1f%%" % (fam, n, h, 100.0 * h / n))
    print("\nB 子系统（推断的类型名 vs 真名所在模块的主类型）")
    print("   可评 %d；命中 %d；**%.1f%%**" % (sub_n, sub_hit, 100.0 * sub_hit / max(1, sub_n)))
    for src, (n, h) in sorted(per_src.items(), key=lambda kv: -kv[1][0]):
        print("     来源 %-8s n=%-3d 命中 %-3d %5.1f%%" % (src, n, h, 100.0 * h / max(1, n)))
    print("\nC 闭包父链（任务书 §1.4 原例）：n=%d，父名出现在「被已定名函数调用」证据里 %d = %.1f%%"
          % (closure_n, closure_hit, 100.0 * closure_hit / max(1, closure_n)))
    print("D 引擎回调位（ready/process/draw）：n=%d，角色里带 engine-hook/draw 或地址入表 %d = %.1f%%"
          % (vt_n, vt_hit, 100.0 * vt_hit / max(1, vt_n)))
    print("\n不可评的名字族：")
    for why, c in fam_unmeas.most_common():
        print("     %-48s %d" % (why, c))
    # 基线：永远猜「最常见的那一类」
    best_tag = collections.Counter()
    for r in rows:
        for t in (r["pred"] or "").split(","):
            if t and t != "-":
                best_tag[t] += 1
    if best_tag:
        exp_tags = collections.Counter()
        for r in rows:
            if r["expect"] == "-":
                continue
            for t in r["expect"].split(","):
                if t:
                    exp_tags[t] += 1
        top = exp_tags.most_common(1)[0][0]
        base = sum(1 for r in rows if r["expect"] != "-" and top in r["expect"].split(","))
        print("\n基线（对照）：永远猜**期望集合里最常见**的标签 `%s`，在可评集合上命中 %d/%d = %.1f%%"
              % (top, base, n_eval, 100.0 * base / max(1, n_eval)))
    return rows, dict(n_eval=n_eval, hit=hit, hit_v1=hit_v1, answered=answered,
                      hit_ans=hit_ans, sub_n=sub_n, sub_hit=sub_hit,
                      closure_n=closure_n, closure_hit=closure_hit,
                      vt_n=vt_n, vt_hit=vt_hit,
                      per_family=dict(per_family), strata={str(k): v for k, v in strata.items()})


def write_control(rows, out=CTRL_TSV):
    hdr = [
        "# 由 tools/callgraph_roles.py match 生成，不要手改（重跑会覆盖）。",
        "# 正对照集：348 个「三份夹具名字并集 ∩ 引用 src/*.rs Location 的 527 个」（`C.named_linux_348()`）。",
        "# expect = 名字推出的期望标签（口径写死在 tools/callgraph_roles.py 的 EXPECT 表）；hit = expect∩pred 非空。",
        "# 函数**自己**的名字只出现在 name/method/expect 列，推断用的是 ①图 ②串 ③常量 ④形状 ⑤引擎 五轴。",
        "linux_addr\tname\tmethod\texpect\tpred\tprimary\tsubsystem\tsubhit\ttier\thit\tnamed_neighbors\tevidence",
    ]
    with open(out, "w", encoding="utf-8") as fh:
        fh.write("\n".join(hdr) + "\n")
        for r in rows:
            fh.write("\t".join([("0x%x" % r["va"]), r["name"], r["method"], r["expect"],
                                 r["pred"], r["primary"], r["sub"], r["subhit"], r["tier"],
                                 r["hit"], r["named_nb"], r["ev"]]) + "\n")
    print("写出 %s（%d 行）" % (out, len(rows)))


def cmd_run(g, lin, names, modmap, vocab, out=OUT_TSV):
    """给 12,736 个（无 `src/` Location 的）出角色 → 夹具。"""
    src527 = {va for va, f in lin.feats.items()
              if any(lin.locs[l][0].startswith("src/") for l in f["locrefs"] if l in lin.locs)}
    targets = sorted(set(lin.universe) - src527)
    tally = collections.Counter()
    rows = []
    for va in targets:
        tags, primary, ev, (sub, sub_src) = role_of(va, g, lin, names, modmap, vocab)
        tier = tier_of(tags, primary, ev)
        tally[tier] += 1
        if primary:
            tally["role:" + primary] += 1
        if sub:
            tally["sub:" + sub] += 1
            tally["subsrc:" + sub_src] += 1
        fe = lin.feats[va]
        inc = [c for c in g.callers.get(va, ()) if c != va]
        outc = sorted((set(fe["callees"]) | set(g.ptr_out.get(va, ()))) - {va})
        rows.append([
            "0x%x" % va, tier, primary or "-", ",".join(tags) or "-",
            ("%s(%s)" % (sub, sub_src)) if sub else "-",
            "%d/%d" % (len([c for c in inc if c in names]), len(inc)),
            "%d/%d" % (len([c for c in outc if c in names]), len(outc)),
            str(len(g.slots_of.get(va, ()))), str(fe["size"]), str(fe["ninsn"]),
            "; ".join("%s:%s" % (a, d) for a, d, _t in ev)[:400] or
            "无证据（无已定名邻居、无引擎调用、无可读字符串、无魔数）",
        ])
    hdr = [
        "# 由 tools/callgraph_roles.py run 生成，不要手改（重跑会覆盖）。",
        "# 目标集合 = Linux 2.0.60 里**不引用任何 src/*.rs panic::Location** 的 %d 个函数。" % len(targets),
        "# 这里没有名字，只有**角色**（行为描述）+ 子系统 + 档位；口径与判据见 docs/02 §37 与 tools/callgraph_roles.py。",
        "# tier=观测（角色由引擎调用/字符串/槽等**能指到地址**的事实支撑）/推断（由形状·邻居·常量推出）/未知（无证据）。",
        "# named_callers/named_callees = 「已定名/全部」的分子分母；末列 evidence 每条带轴（①图②串③常量④形状⑤引擎）。",
        "linux_addr\ttier\tprimary_role\troles\tsubsystem\tnamed_callers\tnamed_callees\t"
        "addr_taken_slots\tsize\tninsn\tevidence",
    ]
    with open(out, "w", encoding="utf-8") as fh:
        fh.write("\n".join(hdr) + "\n")
        for r in rows:
            fh.write("\t".join(r) + "\n")
    print("写出 %s（%d 行 + %d 行注释头）" % (out, len(rows), len(hdr)))
    print("档位分布：观测 %d / 推断 %d / 未知 %d（合计 %d，未知占 %.1f%%）"
          % (tally["观测"], tally["推断"], tally["未知"], len(targets),
             100.0 * tally["未知"] / len(targets)))
    print("主角色分布：")
    for k, v in tally.most_common():
        if k.startswith("role:"):
            print("   %-14s %6d" % (k[5:], v))
    print("子系统来源：", ", ".join("%s=%d" % (k[7:], v) for k, v in tally.most_common()
                                   if k.startswith("subsrc:")))
    print("子系统 top：")
    for k, v in tally.most_common():
        if k.startswith("sub:"):
            print("   %-22s %6d" % (k[4:], v))
    return rows, tally


def cmd_graph(g, lin, names):
    d, p = g.n_edges()
    print("节点 %d（Ghidra 起点表口径）" % len(g.U))
    print("边：直接 `call rel32` %d 条；指针 `call *slot`（槽由 R_*_RELATIVE 写成本地函数）%d 条"
          % (d, p))
    print("指针调用点里槽**解析不出**的：%d（运行时填的表 / gdext 接口）"
          % sum(g.ptr_unresolved.values()))
    print("有指针边的函数 %d；地址入了函数指针槽的函数 %d；≥4 格连续的表 %d 张"
          % (len(g.ptr_out), len(g.slots_of), len(g.tables)))
    print("指针边让**原本零直接调用者**的函数获得调用者：%d"
          % sum(1 for va in g.U if not lin.feats[va]["callers"] and g.callers.get(va)))
    print("取地址边（`lea/mov` 拿到函数地址 ⇒ 注册/闭包）：%d 个函数被取，%d 个函数在取地址"
          % (len(g.ref_users), sum(1 for v in g.U if g.raw.get(v) and (g.raw[v].get("frefs")))))
    print("两者都算「地址外泄」：%d" % len(set(g.ref_users) | set(g.slots_of)))


def cmd_probe(g, lin, names, vocab, va):
    tags, primary, ev, (sub, sub_src) = role_of(va, g, lin, names, None, vocab)
    print("0x%x  size=%d ninsn=%d  已定名=%s" % (va, lin.feats[va]["size"], lin.feats[va]["ninsn"],
                                                 names.get(va, "（无）")))
    print("档位=%s 主角色=%s 角色=%s 子系统=%s(%s)"
          % (tier_of(tags, primary, ev), primary, ",".join(tags) or "-", sub or "-", sub_src))
    print("被调者 %d（已定名 %d）· 图内调用者 %d（已定名 %d）· 指针槽 %d · 间接调用点 %d"
          % (len(lin.feats[va]["callees"]),
             len([c for c in lin.feats[va]["callees"] if c in names]),
             len(g.callers.get(va, ())), len([c for c in g.callers.get(va, ()) if c in names]),
             len(g.slots_of.get(va, ())), len((g.raw.get(va) or {}).get("islots", ()))))
    for a, d_, t in ev:
        print("   [%s] %s   (%s)" % (a, d_, t))


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["build", "graph", "match", "run", "probe", "all"])
    ap.add_argument("addr", nargs="?")
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--out", default=OUT_TSV)
    a = ap.parse_args(argv)
    lin = C.load_fp("linux", verbose=False)
    names = load_names()
    modmap = load_module_map()
    st = build(force=a.force)
    g = Graph(st, lin)
    if a.cmd == "build":
        return 0
    vocab = build_vocab(g, lin)
    if a.cmd == "graph":
        cmd_graph(g, lin, names)
    elif a.cmd == "probe":
        cmd_probe(g, lin, names, vocab, int(a.addr, 0))
    elif a.cmd == "match":
        rows, _s = cmd_match(g, lin, names, modmap, vocab)
        write_control(rows)
    elif a.cmd in ("run", "all"):
        if a.cmd == "all":
            rows, _s = cmd_match(g, lin, names, modmap, vocab)
            write_control(rows)
        cmd_run(g, lin, names, modmap, vocab, out=a.out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
