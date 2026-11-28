#!/usr/bin/env python3
"""全局缺口总清算：把 **13,263 个 `.text` 可分析函数**逐条分类（度量口径审计，只审计不改码）。

## 为什么有这个工具

`fixtures/function_coverage.tsv` 的分母 **527** 是「能按 `panic::Location`（`src/*.rs`）硬归属到
某个原版源文件的函数数」——`tools/function_coverage.py` 的文件头自己写明这是
**「可度量下限，不是模块规模」**。而 `docs/02` §12 的 **13,263** 是 Ghidra 侧 `.text` 内
全部可分析函数。两者差 12,736 条，**从来没有人逐条说过这 12,736 条是什么**。

T82（`docs/06` §129）给了 F3 的 153 条的判据（`na:std-mono` / `na:gdext-glue` /
`na:other` / `real-missing` / `unknown`），但只覆盖 F3。本工具把同一套类别名扩到**全部 13,263 条**，
用**可复算的硬证据**判，产出 `fixtures/global_triage.tsv`。

## 判据（预注册见 `fixtures/global_triage_plan.txt`，先于本脚本提交）

**第一层 = 硬归属（`panic::Location`，机械可复算）**：`.data.rel.ro` 里
`{file:&'static str, len:u64, line:u32, col:u32}` 结构，`.rela.dyn` 的
`R_X86_64_RELATIVE` 还原指针。T82 只接受 `src/` 前缀（⇒ 527 条），本工具**接受一切文件路径**：

| file 前缀/形态 | 归属 | 类 |
|---|---|---|
| `src/<name>.rs` | 本 crate（游戏 + 少数 remap 进来的三方） | 交给 527 台账 / `na:other` 细分 |
| `/rustc/<hex>/library/{core,alloc,std,…}/…` | **Rust 标准库** | `na:std-mono` |
| `…/.cargo/registry/src/…/<crate>-<ver>/…` | **第三方 crate** | `godot*`/`gdext*` ⇒ `na:gdext-glue`；其余 ⇒ `na:other` |

**第二层 = 结构证据（只对 ≤ 体积上限的函数生效；上限来自 T82 §129.3 的实测）**：
`G1` `lock cmpxchg` 写可写全局 + 零直接调用点；`G1b` 零调用点 + 地址在 `.rela.dyn` 数据表 + `0x78` 步长；
`G2` `RefCell` 借用惯用法 `[r+0x10] 0→-1` + `SLOT≥1`；`G3` 唯一调用者不引用任何 Location；
`A/B/C` std 排序单态化（三段自递归 + 元素步长 / `ucomisd`+`jp` 比较器 + 整块搬运或 `cmov`）。

**第三层 = 字符串族证据**：引用了 `res://` / Godot 引擎词汇 / 游戏业务词表 ⇒ 游戏侧；
只有 std panic 模板串 ⇒ 弱证据（不单独定案）。

**判不了就是 `unknown`**：证据不足不硬猜（AGENTS.md §3）。

## 用法

    PY=$WT/.reference/venv/bin/python
    $PY tools/global_triage.py --build-index       # 全 .text 反汇编 + 特征索引（约 1 分钟，缓存到 .agent-work）
    $PY tools/global_triage.py --control           # 正/负对照（T82 的 F3 裁决 + T71 的 31 条）
    $PY tools/global_triage.py                     # 全量：写 fixtures/global_triage.tsv + 打印统计
    $PY tools/global_triage.py --android-census    # 兄弟构建真名普查 + fixtures/global_triage_modules.tsv
    $PY tools/global_triage.py --denom-outside     # T105：527 分母**之外**的真实源码函数 → fixtures/denom_outside.tsv
                                                   #   （预注册 fixtures/denom_outside_plan.txt；C0–C5 全过才写文件）
                                                   #   --control-only 只跑对照；--no-noloc 让 --android-census 输出与 T105 之前逐字节相同
    $PY tools/global_triage.py --dump 0x4600e0 ... # 单条取证（人工复核用）
    $PY tools/global_triage.py --sample 20         # 预注册抽样（人工复核用）

    # T96 新增（预注册 fixtures/global_triage_refresh_plan.txt）：
    $PY tools/global_triage.py --no-manual --out BASE.tsv   # 不含 T93 人工裁决的基线（不写 fixtures）
    $PY tools/global_triage.py --self-check --baseline BASE.tsv   # 自校验 S1/S2/S3（只读，rc≠0 即失败）
    $PY tools/global_triage.py --write-diff OLD.tsv          # 逐行改判台账 fixtures/global_triage_refresh_diff.tsv

`T93_MANUAL_NA`（49 行 → `na:other`）是**唯一**在五层判据之外的人工裁决表，应用在 L4/L5 **之后**；
它的地址集合每次运行都与 `fixtures/gap_modcontainer.tsv` 交叉校验（多一条/少一条直接退出）。

地址一律 **ELF vaddr（RVA）**。本工具**不改**任何既有计分工具。
"""

from __future__ import annotations

import argparse
import bisect
import collections
import hashlib
import json
import os
import pickle
import random
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import addr_probe as ap  # noqa: E402
from capstone import Cs, CS_ARCH_X86, CS_MODE_64  # noqa: E402
from capstone.x86 import (  # noqa: E402
    X86_OP_IMM, X86_OP_MEM, X86_OP_REG, X86_REG_RIP,
)

OUT = os.path.join(ROOT, "fixtures", "global_triage.tsv")
OUT_SAMPLE = os.path.join(ROOT, "fixtures", "global_triage_sampling.tsv")
WORK = os.path.join(ROOT, ".agent-work", "metric-total")
INDEX_PKL = os.path.join(WORK, "text_index.pkl")
BOUNDARY = os.path.join(ROOT, "fixtures", "boundary_audit.tsv")
F3TSV = os.path.join(ROOT, "fixtures", "f3_triage.tsv")
COV = os.path.join(ROOT, "fixtures", "function_coverage.tsv")

# ---------------------------------------------------------------------------
# 预注册常量（写死；改这里必须同时改 fixtures/global_triage_plan.txt）
# ---------------------------------------------------------------------------
VOL_CAP = 0x721           # T82 §129.3：na 结构规则在 >0x721 字节的函数上是假阳性
RUSTC_RE = re.compile(rb"/rustc/[0-9a-f]{8,}/library/([a-z_]+)/")
REGISTRY_RE = re.compile(rb"/\.cargo/registry/src/[^/]+/([A-Za-z0-9_.\-]+)/")
# gdext 在这份构建里是 **git checkout**（不是 crates.io）：`/.cargo/git/checkouts/gdext-<hash>/<rev>/godot-core/…`
GIT_RE = re.compile(rb"/\.cargo/git/checkouts/[^/]+/[0-9a-f]+/([A-Za-z0-9_.\-]+)/")
BUILD_RE = re.compile(rb"/build/([A-Za-z0-9_.\-]+)/out/")
# std 自己内嵌的三方（`/rust/deps/miniz_oxide-0.8.9/src/…`）⇒ 属标准库构建
RUSTDEPS_RE = re.compile(rb"/rust/deps/([A-Za-z0-9_.\-]+)/")
GODEXT_CRATE = re.compile(r"^(godot|gdext|godot4|godot-core|godot-ffi|godot-macros|godot-cell)")
# `#[derive(Serialize/Deserialize)]` 的宏产物特征串
DERIVE_RE = re.compile(r"struct [A-Za-z0-9_]+ with \d+ element|"
                       r"struct [A-Z][A-Za-z0-9_]*(Event|Info|Request|Reply|Message|Data|Params)"
                       r"(?![a-z])|"
                       r"missing field|unknown field|invalid type|invalid length|"
                       r"duplicate field|deserialize_struct|serialize_struct|"
                       r"^a sequence$|^a map$|^struct variant|"
                       r"__Field|__Visitor|Expected::|EndOfLine|LineCol")

# ---------------------------------------------------------------------------
# 类别
# ---------------------------------------------------------------------------
CLS_IMPL = "implemented"          # 已在 527 台账里判为 F1/F2（有实现）
CLS_REAL = "real-missing"
CLS_STD = "na:std-mono"
CLS_GDX = "na:gdext-glue"
CLS_OTHER = "na:other"
CLS_UNK = "unknown"
CLASSES = [CLS_IMPL, CLS_REAL, CLS_STD, CLS_GDX, CLS_OTHER, CLS_UNK]

# ---------------------------------------------------------------------------
# 三层证据：Location 归属
# ---------------------------------------------------------------------------
_LOC_CACHE = {}


def any_location(ctx, slot):
    """`{file:&'static str, len:u64, line:u32, col:u32}`；**任何**文件路径都接受。

    与 `addr_probe.maybe_location` 的唯一区别：不要求 `src/` 前缀。
    返回 (file, line, col) 或 None。
    """
    if slot in _LOC_CACHE:
        return _LOC_CACHE[slot]
    out = None
    for d in (0, -8, -16):
        s = slot + d
        if s not in ctx.rel:
            continue
        p = ctx.rel[s]
        try:
            ln = struct.unpack("<Q", ctx.read(s + 8, 8))[0]
            line, col = struct.unpack("<II", ctx.read(s + 16, 8))
        except Exception:
            continue
        if not (0 < ln <= 300):
            continue
        raw = ctx.read(p, ln) or b""
        if len(raw) != ln or not all(0x20 <= c < 0x7F for c in raw):
            continue
        txt = raw.decode("ascii")
        if not (txt.endswith(".rs") or ".rs:" in txt or "/src/" in txt):
            continue
        if not (0 < line < 400000 and 0 < col < 4000):
            continue
        out = (txt, line, col)
        break
    _LOC_CACHE[slot] = out
    return out


def file_family(f):
    """文件路径 → (family, detail)。family ∈ {crate, std, gdext, registry, other}"""
    b = f.encode()
    if f.startswith("src/"):
        return "crate", f
    m = RUSTC_RE.search(b)
    if m:
        return "std", m.group(1).decode()
    m = RUSTDEPS_RE.search(b)                      # std 内嵌三方 ⇒ 也算标准库构建
    if m:
        return "std", "deps:" + m.group(1).decode()
    if "/gdext-" in f or "/godot-core/" in f or "/godot-ffi/" in f or "/godot-cell/" in f:
        return "gdext", f
    m = GIT_RE.search(b)
    if m:
        crate = m.group(1).decode()
        return ("gdext" if GODEXT_CRATE.match(crate) else "registry"), crate
    m = BUILD_RE.search(b)
    if m:
        crate = m.group(1).decode()
        return ("gdext" if GODEXT_CRATE.match(crate) else "registry"), "build:" + crate
    m = REGISTRY_RE.search(b)
    if m:
        crate = m.group(1).decode()
        return ("gdext" if GODEXT_CRATE.match(crate) else "registry"), crate
    if "/library/" in f:
        return "std", "?"
    return "other", f


# ---------------------------------------------------------------------------
# 第一层：全 .text 索引（per-funcstart 特征 + 引用解析）
# ---------------------------------------------------------------------------
def _exec_ranges(ctx):
    return [(a, b) for a, b in ctx.exec_ranges if b - a > 4096]


def _sec_of(ctx, va):
    for s in ctx.elf.secs:
        if s["size"] and s["addr"] <= va < s["addr"] + s["size"]:
            return s["name"]
    return None


def _in_exec(ctx, va):
    for a, b in ctx.exec_ranges:
        if a <= va < b:
            return True
    return False


def resolve_target(ctx, t, strcache):
    """rip 相对目标的类型与内容。返回 (kind, payload)。"""
    if t in strcache:
        return strcache[t]
    if _in_exec(ctx, t):
        out = ("code", None)
        strcache[t] = out
        return out
    loc = any_location(ctx, t)
    if loc:
        out = ("loc", loc[0])          # 只存**文件路径**；族由 classify 现算（改判据不必重建索引）
        strcache[t] = out
        return out
    # 重定位槽里的裸指针 → 代码指针 / 字符串 / 数据
    if t in ctx.rel:
        p = ctx.rel[t]
        if _in_exec(ctx, p):
            out = ("code", None)      # 函数指针表 / vtable 槽 ⇒ 也是 gdext 派的强特征
            strcache[t] = out
            return out
        raw = ctx.cstr(p, 160)
        if len(raw) >= 4 and all(0x20 <= c < 0x7F for c in raw):
            out = ("str", raw.decode("ascii"))
            strcache[t] = out
            return out
        out = ("slot", None)
        strcache[t] = out
        return out
    raw = ctx.cstr(t, 160)
    if len(raw) >= 4 and all(0x20 <= c < 0x7F for c in raw):
        out = ("str", raw.decode("ascii"))
    else:
        out = ("data", None)
    strcache[t] = out
    return out


def build_index(verbose=True):
    ctx = ap.Ctx(ap.SO_DEFAULT)
    p = os.environ.get("GW2_FUNCSTARTS") or _find_funcstarts()
    raw = pickle.load(open(p, "rb"))
    starts = sorted(int(x, 16) if isinstance(x, str) else int(x) for x in raw)
    key = "%s:%d:%d:%s" % (os.path.abspath(ctx.so), os.path.getsize(ctx.so),
                           os.stat(ctx.so).st_mtime_ns,
                           hashlib.sha256(b",".join(b"%x" % s for s in starts)).hexdigest()[:16])
    if os.path.exists(INDEX_PKL):
        try:
            k, data = pickle.load(open(INDEX_PKL, "rb"))
            if k == key:
                if verbose:
                    print("index cache 命中：%s" % INDEX_PKL)
                return ctx, starts, data
        except Exception:
            pass
    os.makedirs(WORK, exist_ok=True)
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    strcache = {}
    funcs = {}
    callers = collections.defaultdict(list)
    fde_set = set(ctx.fde_lo)
    t0 = __import__("time").time()
    for i, s in enumerate(starts):
        nxt = starts[i + 1] if i + 1 < len(starts) else s + 16
        ex = ctx.fde_extent(s)
        size_fde = (ex[1] - ex[0]) if (ex and s in fde_set) else 0
        blob = ctx.read(s, nxt - s)
        f = dict(size=nxt - s, size_fde=size_fde, n_insn=0, self_calls=0, strides=[],
                 ucomisd=0, jp=0, bulk=0, cmov=0, atomics=[], borrow=[],
                 n_direct_call=0, n_plt=0, callees=[], rips=[], ret_like=False)
        rips = []
        for ins in md.disasm(blob, s):
            f["n_insn"] += 1
            m = ins.mnemonic
            ops = ins.operands
            if m == "call" and ops and ops[0].type == X86_OP_IMM:
                tgt = ops[0].imm & 0xFFFFFFFFFFFFFFFF
                f["n_direct_call"] += 1
                # 调用目标**按 funcstart 归属**（与 tools/propagate.py 同口径：
                # 线性反汇编与 FDE 边界有漂移，调用目标不一定正好是 funcstart）
                ti = bisect.bisect_right(starts, tgt) - 1
                owner = starts[ti] if ti >= 0 else None
                if owner == s:
                    f["self_calls"] += 1
                elif owner is not None:
                    f["callees"].append(owner)
                    callers[owner].append(s)
                    sn = _sec_of(ctx, tgt)
                    if sn in (".plt", ".plt.sec", ".plt.got"):
                        f["n_plt"] += 1
            if m in ("imul", "mul") and len(ops) == 3 and ops[2].type == X86_OP_IMM:
                v = ops[2].imm
                if 0x20 <= v <= 0x10000:
                    f["strides"].append(v)
            if m.startswith("ucomis") or m in ("comisd", "comiss"):
                f["ucomisd"] += 1
            if m in ("jp", "jpe", "jnp", "jpo"):
                f["jp"] += 1
            if m in ("movups", "movaps", "movdqu", "movdqa"):
                f["bulk"] += 1
            if m.startswith("cmov"):
                f["cmov"] += 1
            if m.endswith("cmpxchg") and 240 in (getattr(ins, "prefix", None) or ()):
                for op in ops:
                    if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                        t = ins.address + ins.size + op.mem.disp
                        if _sec_of(ctx, t) in (".data", ".bss", ".got", ".data.rel.ro") \
                                and t not in f["atomics"]:
                            f["atomics"].append(t)
            if m == "mov" and len(ops) == 2 and ops[0].type == X86_OP_MEM \
                    and ops[1].type == X86_OP_IMM:
                if ops[1].imm & 0xFFFFFFFFFFFFFFFF == 0xFFFFFFFFFFFFFFFF \
                        and ops[0].mem.disp == 0x10:
                    f["borrow"].append(ins.address)
            for op in ops:
                if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                    rips.append(ins.address + ins.size + op.mem.disp)
        f["rips"] = rips
        f["ret_like"] = bool(f["n_insn"] <= 3 and blob)
        funcs[s] = f
    # 引用解析
    for s, f in funcs.items():
        for t in f["rips"]:
            if t not in strcache:
                resolve_target(ctx, t, strcache)
        f["rips"] = sorted(set(f["rips"]))
    data = dict(funcs=funcs, callers=dict(callers), targets=strcache)
    pickle.dump((key, data), open(INDEX_PKL, "wb"))
    if verbose:
        print("index 建好：%d funcs，%d 个 rip 目标，%.1fs → %s"
              % (len(funcs), len(strcache), __import__("time").time() - t0, INDEX_PKL))
    return ctx, starts, data


def _find_funcstarts():
    d = ROOT
    for _ in range(6):
        c = os.path.join(d, ".reference", "upstream", "analysis", "funcstarts_ghidra_va.pkl")
        if os.path.exists(c):
            return c
        c = os.path.join(d, "analysis", "funcstarts_ghidra_va.pkl")
        if os.path.exists(c):
            return c
        d = os.path.dirname(d)
    raise SystemExit("找不到 funcstarts_ghidra_va.pkl（设 GW2_FUNCSTARTS）")


# ---------------------------------------------------------------------------
# 台账读取
# ---------------------------------------------------------------------------
def read_tsv(path, need_hex_first=True):
    rows = []
    hdr = None
    with open(path, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("#") or not ln.strip():
                continue
            p = ln.rstrip("\n").split("\t")
            if hdr is None:
                hdr = p
                continue
            if need_hex_first and not p[0].startswith("0x"):
                continue
            rows.append(dict(zip(hdr, p)))
    return rows


def read_boundary():
    return {int(r["addr"], 16): r for r in read_tsv(BOUNDARY)}


def read_cov():
    """`fixtures/function_coverage.tsv` **没有表头行**（列名写在文件头注释里）⇒ 位置解析。"""
    out = []
    cols = ["addr", "name", "name_src", "module", "ours", "ours_loc", "bucket",
            "sub", "pin", "type_check", "verdict", "why", "missing"]
    with open(COV, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("#") or not ln.strip():
                continue
            p = ln.rstrip("\n").split("\t")
            if not p[0].startswith("0x"):
                continue
            d = dict(zip(cols, p))
            d["addr"] = d["addr"].split(".")[0]
            out.append(d)
    return out


# ---------------------------------------------------------------------------
# 分类
# ---------------------------------------------------------------------------
# **强**游戏证据：出现即几乎不可能属于 std/三方（`res://` 资源路径、类名、场景文件）
GAME_STRONG = re.compile(r"res://|graphwar|Graphwar|\.tscn|\.tres|\.pck|"
                         r"GraphwarRootNode|GraphwarButton|EntityDrawer|EffectDrawer|"
                         r"GraphwarPlane|GraphwarLanVBox|GraphwarSetupControl", )
# **弱**游戏证据：`fire/shot/target/effect/draw_/texture/scene/rank/level_/turn_/sound/replay/lan_/bot_`
# ⚠ 抽样复核实测：`.rodata` 的串是**连续排布**的，`cstr(160)` 会把相邻串一起读出来 ⇒
#    弱词在"三方 crate 的 serde 字段名 blob"里也会命中（例 openh264 / ron / clap 的字段名）。
#    因此**弱信号不单独定案**：只降级为 `unknown / str-weak-game`（保守，仍计入缺口）。
GAME_WEAK = re.compile(r"campaign|soldier|Soldier|team_|obstacle|turn_|effect|Effect|fx_|"
                       r"sound|Sound|replay|Replay|rank|Rank|lan_|bot_|Bot|level_|"
                       r"scene|Scene|sprite|Sprite|texture|Texture|draw_|vertex|"
                       r"polygon|Polygon|fire|Fire|shot|Shot|target|Target|_event")
GDX_STRONG = re.compile(r"godot|Godot|GDExtension|gdextension|Variant|ObjectDB|ClassDB|"
                        r"ptrcall|_bind_methods|GodotClass|ExtensionLibrary|Dictionary|"
                        r"PackedVector|GString|StringName|Callable|NodePath|RefCounted|"
                        r"upcast_ref|downcast_ref|Upcast failed|null instance ID|"
                        r"validated_objectarray_to_variant|resized|init_ref\(\) failed|"
                        r"Cannot convert|godot_ffi|GDExtensionClassCreationInfo|"
                        r"globalize_path|ProjectSettings|call_deferred|Gd<|godot_core::|"
                        r"InstanceId|emit_signal|get_node|add_child|ClassDB|"
                        r"GDExtensionClass|VariantType|PropertyInfo|MethodInfo")
# 三方 crate 的词汇（只作 L3 弱证据的**细分**用；硬证据仍是 L1e 的 registry Location）
VENDOR_STRONG = [
    ("regex", re.compile(r"teddymasks|rabinkarp|RabinKarp|Onepass|backtrack|hybriddfa|"
                         r"ReverseSuffix|group_info|minimum_len|regex-automata|prefilter")),
    ("rustls/webpki/ring", re.compile(r"DHKEM_|HKDF_SHA|CertificateRevocation|revocation_date|"
                                      r"signed_data|BorrowedCertRevocationList|next_update|"
                                      r"KeyCompromise|CaCompromise|webpki|rustls|tls1[23]|"
                                      r"expand 32-byte k|chacha|poly1305|ECDH|X25519|P-256")),
    ("serde/serde_json", re.compile(r"invalid type: |missing field `|unknown variant `|"
                                    r"expected value at line|trailing characters")),
    ("http/hyper/tokio", re.compile(r"missing URI scheme|invalid HTTP version|hyper|"
                                    r"header name|StatusCode|content-length|chunked|tokio")),
    ("clap", re.compile(r"error: unexpected argument|USAGE:|--help|clap")),
    ("flate2/miniz_oxide", re.compile(r"deflate|inflate|zlib|adler|huffman")),
    ("hound", re.compile(r"wav|WAVE|sample rate|RIFF")),
    ("openh264", re.compile(r"openh264|SVC|NAL unit|h264|H\.264")),
    ("neli", re.compile(r"netlink|NETLINK|nlmsg|rtnetlink")),
    ("chrono/time", re.compile(r"chrono|timestamp|offset_from_utc|%Y-%m-%d|Parsed")),
    ("base64/percent-encoding/idna/url", re.compile(r"invalid base64|base64|percent|punycode|"
                                                    r"URL parse|relative URL")),
    ("log/tracing", re.compile(r"tracing|TRACE|DEBUG|WARN")),
]
STD_PANIC = re.compile(r"^(\/rustc\/|library\/|"
                       r"called `Option::|called `Result::|index out of bounds|"
                       r"slice index starts|range end index|attempt to |"
                       r"capacity overflow|already borrowed|assertion failed|"
                       r"misaligned pointer|unwrap|panic|invalid utf-8|"
                       r"cannot access a Thread Local|"
                       r"a formatting trait implementation)")


def struct_na(s, f, callers, size_freq):
    """第二层结构证据（T82 §129.1 的 6 条，只对 `max(size,size_fde) ≤ VOL_CAP` 生效）。

    返 dict 命中即 na，否则 None。**A/B/C 全不中 ⇒ 不判 std**（T82 预注册同纪律）。
    """
    if max(f["size"], f["size_fde"]) > VOL_CAP:
        return None
    hits = []
    A = f["self_calls"] >= 2 and bool(f["strides"])
    cmp_hit = f["ucomisd"] >= 2 and f["jp"] >= 2
    pair = size_freq.get(f["size"], 0) >= 2
    B = cmp_hit and (f["bulk"] >= 2 or f["cmov"] >= 4)
    C = cmp_hit and pair
    nc = len(callers.get(s, ()))
    if bool(f["atomics"]) and nc == 0:
        hits.append("G1")
    if nc == 0 and len(f.get("slots") or ()) >= 1 and 0x78 in f["strides"]:
        hits.append("G1b")
    if bool(f["borrow"]) and nc <= 1 and len(f.get("slots") or ()) >= 1:
        hits.append("G2")
    if hits:
        return dict(cls=CLS_GDX, sub="struct:" + "+".join(hits),
                    evid="结构证据 %s（体积 %#x ≤ 上限 %#x；调用点 %d、表槽 %d）"
                         % ("+".join(hits), max(f["size"], f["size_fde"]), VOL_CAP, nc,
                            len(f.get("slots") or ())),
                    layer="L2-struct")
    if A or B or C:
        return dict(cls=CLS_STD, sub="struct:sortmono",
                    evid="std 单态化特征 %s：自递归 %d/步长 %s/比较器 %d+%d/搬运 %d/cmov %d/尺寸成对 %s（%#x 字节）"
                         % ("A" if A else "" + "B" if B else "" + "C" if C else "",
                            f["self_calls"], ",".join("%#x" % v for v in f["strides"][:2]),
                            f["ucomisd"], f["jp"], f["bulk"], f["cmov"], pair,
                            max(f["size"], f["size_fde"])),
                    layer="L2-struct")
    return None


def classify(ctx, s, f, callers, bnd, cov, cls_index, size_freq, shared_locs,
             use_ledger=True):
    """返回 dict(cls, sub, evid, layer)。

    `use_ledger=False` ⇒ **不看 527 台账 / T82 台账**，只靠 L1–L3 的独立证据判。
    控制组的「独立」列用这个，这样 A/B 组才是真正的独立复核，而不是把 T82 的结论抄一遍。
    """
    size = f["size"]
    size_eff = max(size, f["size_fde"])
    # --- 第一层：Location 硬归属 ---
    fam_files = collections.Counter()
    fam_lines = collections.defaultdict(list)
    locs = []
    for t in f["rips"]:
        k = cls_index.get(t)
        if not k or k[0] != "loc":
            continue
        L = any_location(ctx, t)
        if not L:
            continue
        fam, det = file_family(L[0])
        locs.append((fam, L))
        fam_files[(fam, L[0])] += 1
        fam_lines[fam].append(L[1])
    fams = {fam for fam, _ in locs}
    has_crate = "crate" in fams
    if has_crate and use_ledger:
        row = cov.get(s)
        b = bnd.get(s)
        # 527 台账里 F1/F2 ⇒ 已实现；F3 ⇒ 交给 T82 的判据
        bucket = (row or {}).get("bucket") or ""
        if bucket in ("F1", "F2"):
            return dict(cls=CLS_IMPL, sub=bucket,
                        evid="Location src/… 硬归属；T38 台账 %s（已实现）" % bucket,
                        layer="L1-location")
        if bucket == "F3":
            t82 = F3_LABEL.get(s)
            if t82:
                return dict(cls=t82, sub="T82", evid="T82 §129 逐条裁决（见 fixtures/f3_triage.tsv）",
                            layer="L1-t82")
        # 引用 crate Location 但不在 527：**先**走 T82 的结构 na 规则（宏产物也会引用模块 Location，
        # 例 0x4e3b00 = `__register_methods` 引用 graphwar_lan_vbox.rs:54 仍判 na:gdext-glue）。
        sh = struct_na(s, f, callers, size_freq)
        if sh:
            return sh
        # 与 527 里某条**共享同一个 crate Location** ⇒ 同一源码函数被 Ghidra 切出来的另一块，
        # **不是**新的缺口（T82 §129.2 的 4 条"边界不在 .eh_frame 起点"就是这一类）。
        mine = {"%s:%d:%d" % L for fam, L in locs if fam == "crate"}
        if mine & shared_locs:
            return dict(cls=CLS_OTHER, sub="dup:ghidra-split",
                        evid="与 527 中某条共享 crate Location ⇒ 同一源码函数的另一块切分，不重复计缺口",
                        layer="L1-location")
        # 有 src/ Location 但不在 527（罕见）
        return dict(cls=CLS_REAL, sub="crate-loc-unmapped",
                    evid="引用 src/… Location 但不在 527 台账（%s）" % (b.get("loc_main_file") if b else "—"),
                    layer="L1-location")
    # --- 第一层 b：std / gdext / 三方 registry Location ---
    if "gdext" in fams:
        crates = sorted({c for fam, c in fam_files if fam == "gdext"})
        return dict(cls=CLS_GDX, sub="loc-gdext",
                    evid="gdext/godot 的 Location（git checkout 或 build 产物）⇒ %s" % ", ".join(crates[:3]),
                    layer="L1-location")
    if "registry" in fams:
        crates = sorted({c for fam, c in fam_files if fam == "registry"})
        sub = "3rd:" + ",".join(crates[:3])
        isg = any(GODEXT_CRATE.match(c) for c in crates)
        return dict(cls=CLS_GDX if isg else CLS_OTHER,
                    sub=("gdext" if isg else "third-party"),
                    evid=".cargo/registry Location ⇒ 三方 crate：%s" % ", ".join(crates[:4]),
                    layer="L1-location")
    if "std" in fams:
        mods = sorted({(m or "?").split("/")[0].split("-")[0] for fam, m in fam_files if fam == "std"})
        return dict(cls=CLS_STD, sub="std:" + ",".join(mods[:3]),
                    evid="/rustc/…/library/ 或 /rust/deps/ 的 Location ⇒ 标准库代码（%s）"
                         % ",".join(mods[:3]),
                    layer="L1-location")
    if "other" in fams:
        files = sorted({m for fam, m in fam_files if fam == "other"})
        return dict(cls=CLS_OTHER, sub="loc-other",
                    evid="非 std/非 registry 的 Location：%s" % ", ".join(files[:2]),
                    layer="L1-location")
    # --- 第二层：结构证据（体积上限内） ---
    sh = struct_na(s, f, callers, size_freq)
    if sh:
        return sh
    # --- 第三层：字符串族 ---
    strs = []
    for t in f["rips"]:
        k = cls_index.get(t)
        if k and k[0] == "str":
            strs.append(k[1])
    gdx_s = [x for x in strs if GDX_STRONG.search(x)]
    game_s = [x for x in strs if GAME_STRONG.search(x)]
    derive_s = [x for x in strs if DERIVE_RE.search(x)]
    if derive_s and not gdx_s:
        return dict(cls=CLS_OTHER, sub="derive:serde",
                    evid="`#[derive(Serialize/Deserialize)]` 宏产物特征串（%s）⇒ 源码层只需写 struct + derive"
                         % derive_s[0][:56],
                    layer="L3-str")
    if gdx_s:
        return dict(cls=CLS_GDX, sub="str-godot",
                    evid="引用 Godot 引擎/gdext 字符串（%s%s）⇒ gdext 侧产物"
                         % (gdx_s[0][:56], "；同时含类名/方法名 blob，是方法注册壳" if game_s else ""),
                    layer="L3-str")
    if game_s:
        return dict(cls=CLS_REAL, sub="str-game",
                    evid="引用**强**游戏侧串（res:// / 类名 / 场景文件）：%s" % game_s[0][:64],
                    layer="L3-str")
    weak_s = [x for x in strs if GAME_WEAK.search(x)]
    if weak_s:
        return dict(cls=CLS_UNK, sub="str-weak-game",
                    evid="只命中**弱**游戏词（%s）；`.rodata` 串连续排布会把相邻串一起读出，"
                         "弱词在三方 crate 的字段名 blob 里也会命中 ⇒ 不单独定案"
                         % weak_s[0][:48],
                    layer="L3-str")
    # std panic 模板串：只作弱证据，不单独定案 ⇒ unknown
    if strs and all(STD_PANIC.search(x) for x in strs[:6]) and len(strs) >= 1:
        return dict(cls=CLS_UNK, sub="str-std-only",
                    evid="只引用 std panic 模板串（弱证据，不单独定案）", layer="L3-str")
    return dict(cls=CLS_UNK, sub="no-evidence",
                evid="无 Location、无结构证据、无可判串 ⇒ 判不了", layer="L-none")


# T82 的 153 条裁决（从 f3_triage.tsv 读）
F3_LABEL = {}


def load_f3_label():
    global F3_LABEL
    F3_LABEL = {}
    if not os.path.exists(F3TSV):
        return
    started = False
    with open(F3TSV, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("# ---- F2-scan"):
                break                      # F2-scan 分节**单独计数**，不进 153 的算术
            if ln.startswith("vaddr\t"):
                started = True
                continue
            if not started or not ln.strip():
                continue
            p = ln.rstrip("\n").split("\t")
            if len(p) >= 5 and p[0].startswith("0x"):
                F3_LABEL[int(p[0], 16)] = p[4]



# ---------------------------------------------------------------------------
# **T93 人工裁决表**（预注册：`fixtures/global_triage_refresh_plan.txt` §2.2 / §3.2）
#
# 为什么这张表必须存在：L2 的结构规则（`struct_na` 的 A/B/C 组）只认 **std 排序/比较单态化**；
# `GraphwarModuleContainer::add_module::<M>` / `get_module::<M>` 是**本 crate 自己的泛型单态化**
# （TypeId 常量 + `dyn Any` vtable + `HashMap::insert`/`hash_one`），一个 `panic::Location` 都没有
# ⇒ 进不了按 Location 建的 527，只能落到 L5 布局 / L4 调用图扩散 ⇒ 旧快照把它们误判成
# `real-missing/prop:game`（"把已实现函数的邻居也算成 real"，T86 §134.7 第 1 条自己预告过）。
#
# 证据（**逐行**，不是"按名字猜"）：`fixtures/gap_modcontainer.tsv` 69 行 × T93 `docs/06` §139.2–§139.4 ——
# 三个等距块（x86 `0x4694f0 + i·0xa0` / `0x46a490 + i·0x170` / `0x46c5a0 + i·0x220`，独立按函数体形态复算）
# + 同构建 TypeId 常量锚定 + 装配点调用序配对（4 处 22/20/1/1）⇒ 48 条 = **2 个泛型方法的其它实例化副本**
# + 1 条 `core::ptr::drop_in_place<Weak<RefCell<..>>>`（编译器生成）⇒ **源码函数 3 个，源码级缺口 0**。
#
# 表的形状：`addr -> (sub, note)`；**地址集合运行时与 gap_modcontainer.tsv 交叉校验**（`_manual_cross_check()`），
# 少一条/多一条/子类不符 ⇒ 直接退出（不许静默放宽）。
# 应用位置 = L4/L5 **之后**（`apply_manual`），所以改判面恰好是这 49 行（自校验 S3 靠这条逐行核）。
# ---------------------------------------------------------------------------
T93_MANUAL_NA = {
# 49 行 = 48 单态化副本（A:add_module 25 型 / B:get_module 23 型）+ 1 编译器 drop glue（D）
    0x4694f0: ("own-generic-mono", "A M=GraphwarLocalPlayerManager"),
    0x469590: ("own-generic-mono", "A M=GraphwarColorManager"),
    0x469630: ("own-generic-mono", "A M=GraphwarEffectManager"),
    0x4696d0: ("own-generic-mono", "A M=GraphwarServerConnectionHandler"),
    0x469770: ("own-generic-mono", "A M=GraphwarTeamManager"),
    0x469810: ("own-generic-mono", "A M=GraphwarTickHandler"),
    0x4698b0: ("own-generic-mono", "A M=GraphwarGraphicsManager"),
    0x469950: ("own-generic-mono", "A M=GraphwarClientConnectionHandler"),
    0x4699f0: ("own-generic-mono", "A M=GraphwarBanList"),
    0x469a90: ("own-generic-mono", "A M=GraphwarModeManager"),
    0x469b30: ("own-generic-mono", "A M=GraphwarLifeManager"),
    0x469bd0: ("own-generic-mono", "A M=FunctionHistoryRecorder"),
    0x469c70: ("own-generic-mono", "A M=GraphwarGameRecorder"),
    0x469d10: ("own-generic-mono", "A M=GraphwarEntityEffectManager"),
    0x469db0: ("own-generic-mono", "A M=GraphwarFunctionManager"),
    0x469e50: ("own-generic-mono", "A M=GraphwarComputerPlayerManager"),
    0x469ef0: ("own-generic-mono", "A M=GraphwarRankManager"),
    0x469f90: ("own-generic-mono", "A M=GraphwarPlayerManager"),
    0x46a030: ("own-generic-mono", "A M=GraphwarTurnManager"),
    0x46a0d0: ("own-generic-mono", "A M=GraphwarPlanePosManager"),
    0x46a170: ("own-generic-mono", "A M=GraphwarSoldierManager"),
    0x46a210: ("own-generic-mono", "A M=GraphwarGameStateManager"),
    0x46a2b0: ("own-generic-mono", "A M=GraphwarNameManager"),
    0x46a350: ("own-generic-mono", "A M=GraphwarTargetManager"),
    0x46a3f0: ("own-generic-mono", "A M=GraphwarObstacleManager"),
    0x46a490: ("own-generic-mono", "B M=GraphwarPlayerManager"),
    0x46a600: ("own-generic-mono", "B M=GraphwarEffectManager"),
    0x46a770: ("own-generic-mono", "B M=GraphwarSoldierManager"),
    0x46a8e0: ("own-generic-mono", "B M=GraphwarGraphicsManager"),
    0x46aa50: ("own-generic-mono", "B M=GraphwarTickHandler"),
    0x46abc0: ("own-generic-mono", "B M=GraphwarModeManager"),
    0x46ad30: ("own-generic-mono", "B M=GraphwarPlanePosManager"),
    0x46aea0: ("own-generic-mono", "B M=GraphwarGameRecorder"),
    0x46b010: ("own-generic-mono", "B M=FunctionHistoryRecorder"),
    0x46b180: ("own-generic-mono", "B M=GraphwarClientConnectionHandler"),
    0x46b2f0: ("own-generic-mono", "B M=GraphwarServerConnectionHandler"),
    0x46b460: ("own-generic-mono", "B M=GraphwarComputerPlayerManager"),
    0x46b5d0: ("own-generic-mono", "B M=GraphwarColorManager"),
    0x46b740: ("own-generic-mono", "B M=GraphwarTurnManager"),
    0x46b8b0: ("own-generic-mono", "B M=GraphwarTeamManager"),
    0x46ba20: ("own-generic-mono", "B M=GraphwarNameManager"),
    0x46bb90: ("own-generic-mono", "B M=GraphwarFunctionManager"),
    0x46bd00: ("own-generic-mono", "B M=GraphwarObstacleManager"),
    0x46be70: ("own-generic-mono", "B M=GraphwarGameStateManager"),
    0x46bfe0: ("own-generic-mono", "B M=GraphwarRankManager"),
    0x46c150: ("own-generic-mono", "B M=GraphwarTargetManager"),
    0x46c2c0: ("own-generic-mono", "B M=GraphwarLifeManager"),
    0x46c430: ("own-generic-mono", "B M=GraphwarLocalPlayerManager"),
    0x527e80: ("drop-glue", "D M=GraphwarModuleContainer"),
}

MANUAL_LAYER = "L1-t93"
GAP_MODCONTAINER = os.path.join(ROOT, "fixtures", "gap_modcontainer.tsv")


def _load_gap_modcontainer():
    """读 T93 的逐行台账（只读引用，不改它）。返回 {addr: row}；只取 `group ∉ {C}` 的 49 行。"""
    if not os.path.exists(GAP_MODCONTAINER):
        return None
    rows, hdr = {}, None
    with open(GAP_MODCONTAINER, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("#") or not ln.strip():
                continue
            p = ln.rstrip("\n").split("\t")
            if hdr is None:
                hdr = p
                continue
            if not p[0].startswith("0x"):
                continue
            d = dict(zip(hdr, p))
            if d["linux_addr"] in ("", "—"):
                continue
            if d["group"].startswith("C"):
                continue                    # C 组 20 条**本来就在 527**（F1），不改判
            rows[int(d["linux_addr"], 16)] = d
    return rows


def _manual_cross_check():
    """返回错误串（None = 通过）。用 T93 台账交叉校验人工表的 49 个地址与子类。"""
    want = _load_gap_modcontainer()
    if want is None:
        return "缺 %s（T93 逐行台账）⇒ 人工表无法交叉校验" % GAP_MODCONTAINER
    got = set(T93_MANUAL_NA)
    if set(want) != got:
        extra = sorted("%#x" % a for a in got - set(want))
        miss = sorted("%#x" % a for a in set(want) - got)
        return "人工表与 gap_modcontainer.tsv 不一致：多 %s / 少 %s" % (extra, miss)
    bad = []
    for a, d in want.items():
        if d["subclass"] != T93_MANUAL_NA[a][0]:
            bad.append("%#x 台账=%s 人工表=%s" % (a, d["subclass"], T93_MANUAL_NA[a][0]))
    if bad:
        return "子类不一致：%s" % "; ".join(bad)
    n_own = sum(1 for v in T93_MANUAL_NA.values() if v[0] == "own-generic-mono")
    n_drop = sum(1 for v in T93_MANUAL_NA.values() if v[0] == "drop-glue")
    if (n_own, n_drop) != (48, 1):
        return "子类计数应为 48 own-generic-mono + 1 drop-glue，实为 %d + %d" % (n_own, n_drop)
    return None


def apply_manual(rows, verbose=True):
    """把 T93 已逐行取证的 49 行改判 `na:other`。**在 L4/L5 之后调用**（改判面恰好 49 行）。"""
    err = _manual_cross_check()
    if err:
        raise SystemExit("T93_MANUAL_NA 自校验失败：%s" % err)
    before = collections.Counter()
    hit = 0
    for r in rows:
        a = int(r["addr"], 16)
        ent = T93_MANUAL_NA.get(a)
        if ent is None:
            continue
        sub, note = ent
        before["%s/%s" % (r["cls"], r["sub"])] += 1
        r["cls"] = CLS_OTHER
        r["sub"] = sub
        r["layer"] = MANUAL_LAYER
        r["evid"] = ("T93 逐行证据（docs/06 §139.2–§139.4 + fixtures/gap_modcontainer.tsv）：本 crate 泛型方法的"
                     "单态化副本 / 编译器 drop glue，原版源码里没有独立 `fn` ⇒ 不算缺口（%s）" % note)
        hit += 1
    if hit != len(T93_MANUAL_NA):
        raise SystemExit("apply_manual 只命中 %d/%d 行（地址不在 13,263 里？）"
                         % (hit, len(T93_MANUAL_NA)))
    if verbose:
        print("\n=== T93 人工裁决（L4/L5 之后；预注册 plan §2.2/§3.2）===")
        print("  改判 %d 行 → na:other（48 own-generic-mono + 1 drop-glue）；改判前分布：" % hit)
        for k, v in before.most_common():
            print("    %-28s %3d" % (k, v))
    return hit


def build_all(verbose=True, use_manual=True):
    ctx, starts, data = build_index(verbose=verbose)
    load_f3_label()
    funcs = data["funcs"]
    callers = {k: v for k, v in data["callers"].items()}
    idx = data["targets"]
    bnd = read_boundary()
    cov = {int(r["addr"], 16): r for r in read_cov()}
    size_freq = collections.Counter(f["size"] for f in funcs.values())
    # 每个函数的 .rela.dyn 表槽数
    rev = collections.defaultdict(list)
    for slot, tgt in ctx.rel.items():
        rev[tgt].append(slot)
    for s, f in funcs.items():
        f["slots"] = rev.get(s, [])
    # 共享 Location（判 G4 用；T82 口径：同一 Location 被 ≥2 条**无名**行共享）
    owners = collections.defaultdict(set)
    for s, f in funcs.items():
        nm = (cov.get(s) or {}).get("name_src") or ""
        if nm.startswith("v2"):
            continue
        for t in f["rips"]:
            k = idx.get(t)
            if k and k[0] == "loc" and k[1].startswith("src/"):
                L = any_location(ctx, t)
                if L:
                    owners["%s:%d:%d" % L].add(s)
    shared = {l for l, ss in owners.items() if len(ss) >= 2}
    if verbose:
        print("体积上限 %#x；有 .rela.dyn 表槽的函数 %d；共享 Location 组 %d"
              % (VOL_CAP, sum(1 for f in funcs.values() if f["slots"]), len(shared)))
    rows = []
    for s in starts:
        f = funcs[s]
        r = classify(ctx, s, f, callers, bnd, cov, idx, size_freq, shared)
        name = (cov.get(s) or {}).get("name") or ""
        b = bnd.get(s) or {}
        rows.append(dict(
            addr="%#x" % s, name=name, size="%#x" % f["size"],
            size_fde=("%#x" % f["size_fde"]) if f["size_fde"] else "—",
            module=b.get("loc_main_file") or "—",
            attributed=("hard" if s in bnd else "—"),
            cls=r["cls"], sub=r["sub"], layer=r["layer"], evid=r["evid"],
            callers=len(callers.get(s, ())), slots=len(f["slots"]),
        ))
    apply_region_prior(rows, verbose=verbose)      # 先补布局局部性（强于邻居投票）
    apply_propagation(rows, data, verbose=verbose)
    if use_manual:
        apply_manual(rows, verbose=verbose)        # **最后一步**：T93 人工裁决（改判面恰好 49 行）
    return ctx, starts, data, rows


# ---------------------------------------------------------------------------
# 第四层：调用图**族**扩散（推断层；与 `docs/02` §10 的 885 同一套规则，标签换成"族"）
# ---------------------------------------------------------------------------
PROP_ROUNDS = 3
PROP_MIN_EDGES = 2
PROP_MAJORITY = 2.0 / 3.0

HARD_LAYERS = ("L1-location", "L1-t82", "L2-struct", "L3-str")
FAMILY_CLS = {"std": CLS_STD, "gdext": CLS_GDX, "third": CLS_OTHER, "game": CLS_REAL}


def family_of(r):
    """一行 → 族。`dup:ghidra-split` 是**本 crate 代码被重复切分**，族仍是 game。"""
    if r["sub"].startswith("dup:"):
        return "game"
    return {CLS_IMPL: "game", CLS_REAL: "game", CLS_STD: "std",
            CLS_GDX: "gdext", CLS_OTHER: "third"}.get(r["cls"])


def _adjacency(data):
    """func -> 邻居集合（直接调用边，双向）。"""
    funcs = data["funcs"]
    nbr = collections.defaultdict(set)
    for tgt, cs in data["callers"].items():
        for c in cs:
            if tgt in funcs and c in funcs:
                nbr[c].add(tgt)
                nbr[tgt].add(c)
    return nbr


def propagate(data, seeds, rounds=PROP_ROUNDS, min_edges=PROP_MIN_EDGES,
              majority=PROP_MAJORITY):
    """`seeds`: addr -> label。返回 (label, origin)，origin ∈ {seed, inferred}。"""
    nbr = _adjacency(data)
    label = dict(seeds)
    origin = {a: "seed" for a in label}
    for _rnd in range(rounds):
        newly = 0
        for a in list(nbr.keys()):
            if a in label:
                continue
            votes = collections.Counter(label[n] for n in nbr[a] if n in label)
            if not votes:
                continue
            tot = sum(votes.values())
            mod, v = votes.most_common(1)[0]
            if tot >= min_edges and v / tot >= majority:
                label[a] = mod
                origin[a] = "inferred"
                newly += 1
        if not newly:
            break
    return label, origin


def apply_propagation(rows, data, verbose=True):
    """把硬证据得到的**族**标签沿调用图扩散；只给 `unknown` 行补类，`layer` 记 `L4-prop`。"""
    seeds = {}
    for r in rows:
        if r["layer"] in HARD_LAYERS + ("L5-region",):
            fam = family_of(r)
            if fam:
                seeds[int(r["addr"], 16)] = fam
    label, origin = propagate(data, seeds)
    n_seed = sum(1 for o in origin.values() if o == "seed")
    filled = collections.Counter()
    for r in rows:
        if r["cls"] != CLS_UNK:
            continue
        fam = label.get(int(r["addr"], 16))
        if not fam:
            continue
        r["cls"] = FAMILY_CLS[fam]
        r["sub"] = "prop:" + fam
        r["layer"] = "L4-prop"
        r["evid"] = ("调用图 3 轮扩散（规则同 docs/02 §10：≥%d 条已标注邻居边且同一族占 ≥%.0f%%）"
                     " ⇒ 推断为 %s；**这是推断，不是观测**" % (PROP_MIN_EDGES, PROP_MAJORITY * 100, fam))
        filled[fam] += 1
    if verbose:
        print("\n=== L4 调用图族扩散（推断层）===")
        print("  种子（硬证据）%d；扩散后共标注 %d（推断 %d）"
              % (n_seed, len(label), len(label) - n_seed))
        for k, v in filled.most_common():
            print("    补 %-6s %5d 条" % (k, v))
    return label, origin


REGION = 0x10000          # 64 KiB：链接器按编译单元/库排布，同 region 的族高度一致
REGION_MIN = 6            # region 内至少 6 条硬证据
REGION_MAJORITY = 0.85    # 且同族占比 ≥85%


def apply_region_prior(rows, verbose=True):
    """**L5 布局局部性（推断）**：链接器把同一 crate/编译单元的代码连续排布。

    先用**硬证据**的族给每个 64 KiB region 投票；region 内硬证据 ≥REGION_MIN 条且同一族
    ≥REGION_MAJORITY 时，把该 region 仍未定的函数判为该族（`layer=L5-region`）。
    这条规则**是可错推断**：足以否掉"把 openh264 的 C++ 函数当成游戏代码"这类传播误判
    （抽样复核第一轮就抓到了 29/70 条这样的误判）。
    """
    votes = collections.defaultdict(collections.Counter)
    for r in rows:
        if r["layer"] in HARD_LAYERS:
            fam = family_of(r)
            if fam:
                votes[int(r["addr"], 16) // REGION][fam] += 1
    good = {}
    for reg, c in votes.items():
        tot = sum(c.values())
        fam, v = c.most_common(1)[0]
        if tot >= REGION_MIN and v / tot >= REGION_MAJORITY:
            good[reg] = (fam, v, tot)
    filled = collections.Counter()
    for r in rows:
        reg = int(r["addr"], 16) // REGION
        g = good.get(reg)
        if not g:
            continue
        fam, v, tot = g
        now = family_of(r) if r["cls"] != CLS_UNK else None
        if now is not None and now == fam:
            continue                       # 已一致
        if now is not None and r["layer"] == "L1-location":
            continue                       # **硬证据优先于布局推断**，不许被 region 覆盖
        if now is not None and r["layer"] in HARD_LAYERS and fam in ("game",):
            pass                           # gdext/std/third 的强结构证据也保留（下面 continue）
        if now is not None and r["layer"] in ("L2-struct", "L3-str") and fam != "game":
            continue                       # 强结构/强串证据不覆盖
        r["cls"] = FAMILY_CLS[fam]
        r["sub"] = "region:" + fam
        r["layer"] = "L5-region"
        r["evid"] = ("布局局部性推断：region %#x 内硬证据 %d 条里 %d 条是 %s（≥%.0f%%）"
                     " ⇒ 该 region 的未定函数判 %s；**是推断不是观测**"
                     % (reg * REGION, tot, v, fam, REGION_MAJORITY * 100, fam))
        filled[fam] += 1
    if verbose:
        print("\n=== L5 布局局部性（推断层）===")
        print("  可用 region %d 个（硬证据 ≥%d 条且同族 ≥%.0f%%）"
              % (len(good), REGION_MIN, REGION_MAJORITY * 100))
        for k, v in filled.most_common():
            print("    补 %-6s %5d 条" % (k, v))
    return good


def module_propagation(data, bnd, verbose=True):
    """**527 vs 885 的复算**：种子 = 527 条硬归属（模块 = `boundary_audit` 的 loc_main_file），
    规则与 `tools/propagate.py` 一致（3 轮 / ≥2 条已标注邻居 / ≥2/3 多数）。"""
    seeds = {}
    for a, r in bnd.items():
        mod = (r.get("loc_main_file") or "").replace("src/", "")
        if mod:
            seeds[a] = mod
    label, origin = propagate(data, seeds)
    obs = sum(1 for o in origin.values() if o == "seed")
    inf = len(label) - obs
    if verbose:
        print("\n=== 527 vs 885 复算（同一套扩散规则，种子换成今天的 527）===")
        print("  种子（观测/硬归属）%d ⇒ 3 轮扩散后累计 %d（推断 %d）"
              % (obs, len(label), inf))
        per = collections.Counter(label.values())
        print("  模块数 %d；按合计前 8：" % len(per))
        for m, v in per.most_common(8):
            o = sum(1 for a, x in label.items() if x == m and origin[a] == "seed")
            print("    %-42s 观测 %3d + 推断 %3d = %3d" % (m, o, v - o, v))
    return label, origin


def write_tsv(rows, path=OUT):
    cols = ["addr", "name", "size", "size_fde", "module", "attributed",
            "cls", "sub", "layer", "callers", "slots", "evid"]
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("# global_triage.tsv —— 由 tools/global_triage.py 生成，**不要手改**\n")
        fh.write("# 一行 = 一个 Ghidra funcstart（13,263 个 .text 可分析函数），六类分类见文件头判据\n")
        fh.write("# 重算：.reference/venv/bin/python tools/global_triage.py\n")
        fh.write("# 地址口径 = ELF vaddr（RVA）；size = funcstart 分区，size_fde = .eh_frame 范围（非 FDE 起点为 —）\n")
        fh.write("# layer=L1-t93 的 %d 行 = **T93 人工裁决**（本 crate 泛型单态化副本 / drop glue，源码层无独立 fn）；\n"
                 "#   证据 fixtures/gap_modcontainer.tsv + docs/06 §139、§142.3；表在 tools/global_triage.py::T93_MANUAL_NA\n"
                 % len(T93_MANUAL_NA))
        fh.write("\t".join(cols) + "\n")
        for r in rows:
            fh.write("\t".join(str(r[c]) for c in cols) + "\n")
    return path


def stats(rows, verbose=True):
    c = collections.Counter(r["cls"] for r in rows)
    n = len(rows)
    if not verbose:
        return c
    print("\n=== 13,263 条全量分类 ===")
    tot = 0
    for k in CLASSES:
        v = c.get(k, 0)
        tot += v
        print("  %-14s %6d  %5.2f%%" % (k, v, 100.0 * v / n))
    print("  %-14s %6d" % ("合计", tot))
    # 分层计数
    lc = collections.Counter(r["layer"] for r in rows)
    print("\n=== 证据层 ===")
    for k, v in lc.most_common():
        print("  %-16s %6d" % (k, v))
    return c


# ---------------------------------------------------------------------------
# 控制组
# ---------------------------------------------------------------------------
def cmd_control(argv):
    ctx, starts, data = build_index(verbose=True)
    load_f3_label()
    funcs = data["funcs"]
    callers = data["callers"]
    idx = data["targets"]
    bnd = read_boundary()
    cov = {int(r["addr"], 16): r for r in read_cov()}
    size_freq = collections.Counter(f["size"] for f in funcs.values())
    rev = collections.defaultdict(list)
    for slot, tgt in ctx.rel.items():
        rev[tgt].append(slot)
    for s, f in funcs.items():
        f["slots"] = rev.get(s, [])
    from f3_triage import CTRL_STD, CTRL_GDEXT, CTRL_GDEXT_CLOSURE, CTRL_REAL, CTRL_REAL_EXTRA

    def judge(s, use_ledger=True):
        return classify(ctx, s, funcs[s], callers, bnd, cov, idx, size_freq, set(),
                        use_ledger=use_ledger)

    def ledger_moved(s):
        """控制组的期望是**当年**的台账状态；现在 bucket 变 F1/F2 ⇒ 已被落码，
        台账模式判 `implemented` 是**台账说得对**，不是判据错（与 D 组同一条豁免）。
        独立列（use_ledger=False）仍然照旧，那才是判据真正被检的地方。"""
        return (cov.get(s) or {}).get("bucket") in ("F1", "F2")

    drift_ab = collections.Counter()
    fails = 0
    print("\n=== A：T71 台账 12 条 std 单态化（期望 na:std-mono）===")
    print("     ⚠ 这些地址本身在 527 台账里 ⇒ 第 1 列是「抄台账」的**回归闸**，"
          "第 2 列是**不看台账**的独立判（只靠 L1–L3 证据）")
    for a in CTRL_STD:
        s = int(a, 16)
        r, r2 = judge(s), judge(s, False)
        ok = r["cls"] == CLS_STD
        moved = (not ok) and ledger_moved(s)
        if moved:
            drift_ab["A"] += 1
        fails += 0 if (ok or moved) else 1
        print("  %s %-10s 台账=%-13s 独立=%-13s %s"
              % ("✓" if ok else ("△" if moved else "✗"), a, r["cls"], r2["cls"],
                 "台账已变档（现 F1/F2）" if moved else (r2 if r2["cls"] == CLS_STD else r)["evid"][:74]))
    print("\n=== B：T71 台账 19 条 gdext（16 期望 na:gdext-glue / 3 源码闭包期望 unknown）===")
    for a in CTRL_GDEXT:
        s = int(a, 16)
        r, r2 = judge(s), judge(s, False)
        want = CLS_UNK if a in CTRL_GDEXT_CLOSURE else CLS_GDX
        ok = r["cls"] == want
        moved = (not ok) and ledger_moved(s)
        if moved:
            drift_ab["B"] += 1
        fails += 0 if (ok or moved) else 1
        print("  %s %-10s want=%-13s 台账=%-13s 独立=%-13s %s"
              % ("✓" if ok else ("△" if moved else "✗"), a, want, r["cls"], r2["cls"],
                 "台账已变档（现 F1/F2，T87/T89 之后该函数已被落码）" if moved
                 else (r2 if r2["cls"] == want else r)["evid"][:62]))
    if drift_ab:
        print("  △ 台账已变档 %d 条（A %d / B %d）—— 控制组的**期望**是当年的台账状态，"
              "那些函数后来被落码了 ⇒ 台账模式判 implemented 是对的（与 D 组同一条豁免）；"
              "判据本身仍由「独立」列检验" % (sum(drift_ab.values()), drift_ab["A"], drift_ab["B"]))
    diag = 0
    print("\n=== C：T71 落码的真业务 2 条 + 负对照 3 条（期望非 na）===")
    for a in CTRL_REAL + CTRL_REAL_EXTRA:
        s = int(a, 16)
        r, r2 = judge(s), judge(s, False)
        ok = not r["cls"].startswith("na:")            # 只有**台账模式**算控制组失败
        if not r2["cls"].startswith("na:"):
            diag += 1
        fails += 0 if ok else 1
        print("  %s %-10s 台账=%-13s 独立=%-13s %s"
              % ("✓" if ok else "✗", a, r["cls"], r2["cls"], r["evid"][:58]))
    print("\n=== D：T82 的 F3 裁决 %d 条（回归闸：不许把 F3 判坏）===" % len(F3_LABEL))
    n = agree = drift = 0
    for s, want in sorted(F3_LABEL.items()):
        if s not in funcs:
            print("  ? %#x 不在 13263" % s)
            continue
        n += 1
        r = judge(s)
        if r["cls"] == want:
            agree += 1
            continue
        b = (cov.get(s) or {}).get("bucket")
        if b in ("F1", "F2"):
            drift += 1
            continue
        print("  ✗ %-10s T82=%-14s 本工具=%-14s %s"
              % ("%#x" % s, want, r["cls"], r["evid"][:58]))
    print("  T82 的 %d 条裁决：一致 %d ／ 台账已变档（现为 F1/F2，**不是本工具判错**，是 T82 之后"
          "有人把这些函数落码了）%d ／ 真不一致 %d" % (n, agree, drift, n - agree - drift))
    fails += 0 if agree + drift == n else 1
    print("\n  [诊断] C 组独立模式（不看台账）判对 %d/5：结构规则在真业务大函数上会假阳性 2 条"
          "（`0x3b4ec0` / `0x519840`），已登记 docs/06 §132.4/§132.5；它们**只影响无 Location 的行**，"
          "故 L2 结构规则只占 41 条" % diag)
    print("\n控制组结论（预注册 A–D 的**台账模式**）：%s（失败项 %d）"
          % ("全对 ✓" if fails == 0 else "有失败 ✗", fails))
    return 0 if fails == 0 else 1



# ---------------------------------------------------------------------------
# **同源兄弟构建的真名普查**（Android 2.0.60 的 `.symtab` 没被剥离）
#   这是回答"那 12,736 条到底是什么"的**唯一硬证据来源**：同一份源码的另一份构建里，
#   名字还在。Linux 那 13,263 条绝大多数没有 Location ⇒ 用兄弟构建的**构成比例**做代理，
#   并明确标注：这是**推断（代理）**，不是逐条归属。
# ---------------------------------------------------------------------------
ANDROID_SO = os.path.join(ROOT, ".reference", "steam", "android-2.0.60", "libgraphwarrust.so")
CENSUS_TSV = os.path.join(ROOT, "fixtures", "global_triage_android_census.tsv")
MODULES_TSV = os.path.join(ROOT, "fixtures", "global_triage_modules.tsv")
STD_CRATES = {"core", "alloc", "std", "proc_macro", "panic_unwind", "panic_abort", "unwind",
              "compiler_builtins", "hashbrown", "libunwind", "addr2line", "gimli", "object"}
GODOT_CRATES = {"godot_core", "godot_ffi", "godot", "gdext", "godot_macros", "godot_cell"}


def _android_syms():
    import subprocess
    out = subprocess.run(["readelf", "-SW", ANDROID_SO], capture_output=True, text=True).stdout
    secs = {}
    for ln in out.splitlines():
        m = re.match(r"\s*\[\s*\d+\]\s+(\S+)\s+(\S+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)", ln)
        if m:
            secs[m.group(1)] = (int(m.group(3), 16), int(m.group(5), 16))
    lo, sz = secs[".text"]
    out = subprocess.run(["readelf", "-sW", ANDROID_SO], capture_output=True, text=True).stdout
    by = collections.defaultdict(list)
    for ln in out.splitlines():
        p = ln.split()
        if len(p) < 8 or p[3] != "FUNC" or p[6] == "UND":
            continue
        try:
            val = int(p[1], 16)
        except ValueError:
            continue
        if not (lo <= val < lo + sz):
            continue
        name = ln.split(None, 7)[7].strip() if len(ln.split(None, 7)) > 7 else p[7]
        by[val].append(name)
    return by


def _crate_tokens(name, reg3):
    out = set()
    for k in STD_CRATES | GODOT_CRATES | reg3 | {"graphwarrust"}:
        if ("%d%s" % (len(k), k)) in name or (k + "..") in name or (k + "::") in name:
            out.add(k)
    for m in re.finditer(r"Cs[0-9A-Za-z]{16}_(\d+)([A-Za-z0-9_]+)", name):
        L = int(m.group(1)); t = m.group(2)
        if len(t) == L:
            out.add(t)
    return out


def cmd_android_census(argv):
    """写 `fixtures/global_triage_android_census.tsv` + `fixtures/global_triage_modules.tsv`。"""
    if not os.path.exists(ANDROID_SO):
        raise SystemExit("缺 Android 2.0.60 构建：%s" % ANDROID_SO)
    reg3 = set()
    for x in ("aho-corasick", "anstream", "anstyle", "base64", "bytes", "chrono",
              "clap_builder", "clap_lex", "flate2", "hound", "http", "httparse", "hyper",
              "hyper-util", "idna", "ipnet", "log", "memchr", "miniz_oxide", "neli",
              "once_cell", "openh264", "percent-encoding", "rand", "regex", "regex-automata",
              "regex-syntax", "reqwest", "ring", "rustls", "serde", "serde_json", "socket2",
              "spin", "strsim", "tiny_http", "tinyvec", "tokio", "tower", "tracing-core",
              "unicode-bidi", "unicode-normalization", "untrusted", "url", "want", "webpki",
              "hyper-rustls", "http-body-util", "futures-channel", "futures-core",
              "futures-util", "chunked_transfer", "socket2", "httparse", "httpdate"):
        reg3.add(x.replace("-", "_"))
    by = _android_syms()

    def classify(names):
        toks = set()
        for n in names:
            toks |= _crate_tokens(n, reg3)
        n0 = names[0]
        is_game = any("graphwarrust" in n for n in names)
        glue = any(("drop_in_place" in n or "drop$LT" in n) for n in names)
        if is_game:
            if any(t in STD_CRATES for t in toks) and "12graphwarrust" not in n0:
                return "game:std-glue" if glue else "game:generic"
            return "game"
        if any(t in GODOT_CRATES for t in toks):
            return "gdext"
        for k in ("core", "alloc", "std", "panic_unwind", "panic_abort", "unwind",
                  "compiler_builtins", "hashbrown", "libunwind"):
            if k in toks:
                return k
        if toks:
            t = sorted(toks)[0]
            return ("gdext" if t in GODOT_CRATES else "3rd:" + t)
        if re.match(r"^(_ZN7Wels|Wels|_GLOBAL__N_1)", n0) or n0.startswith("__cxa"):
            return "cpp:openh264"
        return "other"

    cen = collections.Counter()
    mods = collections.Counter()
    shape = collections.Counter()
    for a, names in by.items():
        f = classify(names)
        cen[f] += 1
        if f.startswith("game"):
            n0 = names[0]
            m = re.search(r"12graphwarrust(\d+)([A-Za-z0-9_]+)", n0)
            mod = m.group(2)[:int(m.group(1))] if m else None
            if mod is None:
                m2 = re.search(r"graphwarrust\.\.([a-z0-9_]+)", n0)
                mod = m2.group(1) if m2 else "<none>"
            mods[mod] += 1
            if any("drop_in_place" in n or "drop$LT" in n for n in names):
                shape["drop-glue"] += 1
            elif "closure" in n0:
                shape["closure"] += 1
            else:
                shape["fn"] += 1
    tot = len(by)
    agg = collections.Counter()
    for k, v in cen.items():
        agg[k.split(":")[0]] += v
    with open(CENSUS_TSV, "w", encoding="utf-8") as fh:
        fh.write("# global_triage_android_census.tsv —— Android 2.0.60 `.symtab` 真名普查（tools/global_triage.py --android-census）\n")
        fh.write("# 用途：Linux 13,263 条里绝大多数没有 Location ⇒ 用**同源兄弟构建**的真名构成做代理（**推断，不是逐条归属**）\n")
        fh.write("# 口径：`.text` 内 FUNC 符号按地址去重；名字 = .symtab 原始 mangled 名；族 = 名字里的 crate token\n")
        fh.write("# Android=12609 个 .text 函数；Linux=13263（同一套源码的另一份构建，平台层差异 ~654 条）\n")
        fh.write("scope\tkey\tcount\n")
        for k, v in agg.most_common():
            fh.write("family\t%s\t%d\n" % (k, v))
        for k, v in sorted(cen.items(), key=lambda kv: -kv[1]):
            fh.write("detail\t%s\t%d\n" % (k, v))
        for k, v in sorted(shape.items()):
            fh.write("game_shape\t%s\t%d\n" % (k, v))
        for k, v in sorted(mods.items(), key=lambda kv: -kv[1]):
            fh.write("game_module\t%s\t%d\n" % (k, v))
        fh.write("total\tall\t%d\n" % tot)
    print("写出 %s（%d 条函数；game %d）" % (CENSUS_TSV, tot,
                                        sum(v for k, v in cen.items() if k.startswith("game"))))
    # ---- 模块表：Android 真名计数 + Linux 527 台账 ----
    noloc = None if getattr(argv, "no_noloc", False) else load_noloc_counts()
    if getattr(argv, "no_noloc", False):
        print("（--no-noloc：不写 lin_fns_noloc 列，输出与 T105 之前逐字节相同）")
    elif noloc is None:
        print("⚠ 缺 fixtures/denom_outside.tsv ⇒ 不出 lin_fns_noloc 列"
              "（先跑 $PY tools/global_triage.py --denom-outside）")
    bnd = read_boundary()
    cov = {int(r["addr"], 16): r for r in read_cov()}
    lin = collections.Counter()
    lin_f12 = collections.Counter()
    for a, r in cov.items():
        mod = os.path.basename(bnd.get(a, {}).get("loc_main_file") or "")
        if mod:
            lin[mod] += 1
            if r["bucket"] in ("F1", "F2"):
                lin_f12[mod] += 1
    f3 = collections.Counter()
    f3real = collections.Counter()
    for a, lab in F3_LABEL.items():
        if (cov.get(a) or {}).get("bucket") != "F3":
            continue                      # T82 之后已落码（现为 F1/F2）的不算今天的缺口
        mod = os.path.basename(bnd.get(a, {}).get("loc_main_file") or "")
        f3[mod] += 1
        if lab == CLS_REAL:
            f3real[mod] += 1
    allmods = sorted(set(k + ".rs" if not k.endswith(".rs") else k for k in mods)
                     | set(lin) | set(f3))
    with open(MODULES_TSV, "w", encoding="utf-8") as fh:
        fh.write("# global_triage_modules.tsv —— 按模块的全局缺口表（tools/global_triage.py --android-census）\n")
        fh.write("# android_fns = 同源 Android 构建里该模块的**真名**函数数（观测，但属**兄弟构建**）\n")
        fh.write("# lin527 = Linux 侧能按 panic::Location 硬归属到该模块的函数数；lin_F1F2 = 其中已实现\n")
        fh.write("# lin_F3 = **今天仍是** F3 的条数（口径 = 在 f3_triage.tsv 主表里且 function_coverage.tsv 现在 bucket=F3；"
                 "T82 原判 153 条，到 2026-11-28 已有 11 条落码归档、1 条 T89 新登记；主表另留 6 条 T94 落码后待归档的陈旧行被本列过滤）；"
                 "lin_F3_real = 其中 T82 判 real-missing\n")
        fh.write("# gap_proxy = android_fns − lin_F1F2（**代理估计**：Android 比 Linux 少 654 条函数，平台层差异未扣）\n")
        if noloc is not None:
            fh.write("# lin_fns_noloc = **T105 新增**：该模块「有源码身份但**没有** `panic::Location`」的**原版源码函数**数\n")
            fh.write("#   （= `fixtures/denom_outside.tsv` 里 verdict 以 `/denom:no-loc` 结尾、且 form=business 的**源码组**数；\n")
            fh.write("#    源码组 = 同一条泛型 fn 的多条单态化机器码合并后的个数。判据预注册 fixtures/denom_outside_plan.txt）\n")
            fh.write("#   ⚠️ 这一列**不在** gap_proxy 里：gap_proxy 是机器码减法，本列是源码函数减法；两者**不可相加**。\n")
        hdr_cols = "module\tandroid_fns\tlin527\tlin_F1F2\tlin_F3\tlin_F3_real\tgap_proxy"
        if noloc is not None:
            hdr_cols += "\tlin_fns_noloc"
        fh.write(hdr_cols + "\n")
        rows = []
        for m in allmods:
            key = m[:-3] if m.endswith(".rs") else m
            row = [m, mods.get(key, 0), lin.get(m, 0), lin_f12.get(m, 0),
                   f3.get(m, 0), f3real.get(m, 0), mods.get(key, 0) - lin_f12.get(m, 0)]
            if noloc is not None:
                row.append((noloc or {}).get(m, 0))
            rows.append(tuple(row))
        rows.sort(key=lambda r: (-r[1], -r[2]))
        for r in rows:
            fh.write("\t".join(str(x) for x in r) + "\n")
    print("写出 %s（%d 个模块）" % (MODULES_TSV, len(rows)))
    return 0


# ---------------------------------------------------------------------------
# T105：**527 分母之外**的真实源码函数（"分母外缺口"）全量量化
#   预注册：fixtures/denom_outside_plan.txt（commit 342b534，**先提交后跑**）
#   产出：fixtures/denom_outside.tsv（一行 = Android 游戏本 crate 的一条机器码）
#   用途：`--android-census` 给 `fixtures/global_triage_modules.tsv` 加 `lin_fns_noloc` 列；
#         `tools/function_coverage.py` 读同一份 TSV 给 527 台账加同名列并打第四口径 D。
#
#   为什么：527 分母用 `panic::Location` 建 ⇒ **体内没有 Location 的原版函数永远进不了分母、
#   也永远不出现在"缺口"里**。t97（root_node 3 条）/ t102（graphwar_plane 3 条）/
#   t101（graphwar_nodes 18 条 = 2 个源码函数）三个 agent 各自撞上同一堵墙。
# ---------------------------------------------------------------------------
DENOM_TSV = os.path.join(ROOT, "fixtures", "denom_outside.tsv")
DENOM_BINARY = os.path.join(ROOT, ".reference", "steam", "2026-08-28", "libgraphwarrust.so")
DENOM_VOL_CAP = 0x800                 # T82/T85 M4，**逐字继承**，一条阈值都没改
_DENOM_NOISE = ("/rustc/", "/.cargo/", "cargo/registry")
_DENOM_HASH = re.compile(r"::h[0-9a-f]{16}$")
_DENOM_CLOSURE = "{{closure}}"
# 形态 → (类别, 子类)。形态判据见预注册 §2.2
DENOM_FORM_CLASS = {
    "drop-glue": ("na:other", "drop-glue"),
    "reg-table": ("na:gdext-glue", "reg-table"),
    "call-shim": ("na:gdext-glue", "call-shim"),
    "ctor-shell": ("na:gdext-glue", "ctor-shell"),
    "virtual-shim": ("na:gdext-glue", "virtual-shim"),
    "class-id": ("na:gdext-glue", "class-id"),
    "clap-derive": ("na:other", "clap-derive"),
    "macro-derive": ("na:other", "macro-derive"),
    "closure": ("na:other", "closure-in-fn"),
}


def _denom_game_key(path):
    return path.endswith(".rs") and not any(n in path for n in _DENOM_NOISE)


def _denom_android():
    """G = Android `.text` 内任一 `.symtab` 名含 `graphwarrust` 的地址（去重）。

    枚举口径与 `_android_syms()` 同源（同一份 `.symtab`），但这里**保留 size** 并算
    `real_span = min(st_size, 下一个 FUNC 地址 − 本地址)`（T101 §149：`st_size` 会被重叠符号骗）。
    """
    import naming_android as NA
    side = NA.Side(ANDROID_SO, "android")
    t = side.text
    lo, hi = t["addr"], t["addr"] + t["size"]
    syms = [s for s in side.elf.symtab()
            if s["type"] == NA.STT_FUNC and s["size"] > 0 and lo <= s["value"] < hi]
    by, size = collections.defaultdict(list), {}
    for s in syms:
        by[s["value"]].append(s["name"])
        size[s["value"]] = max(size.get(s["value"], 0), s["size"])
    addrs = sorted(a for a, ns in by.items() if any("graphwarrust" in n for n in ns))
    starts = sorted(by)
    nxt = {v: (starts[i + 1] if i + 1 < len(starts) else hi) for i, v in enumerate(starts)}
    return side, by, size, nxt, addrs


def _denom_demangle(by, addrs):
    import naming_android as NA
    flat, owner = [], []
    for va in addrs:
        for n in by[va]:
            flat.append(n)
            owner.append(va)
    dem = NA.demangle_all(flat)
    out = collections.defaultdict(list)
    for va, d in zip(owner, dem):
        out[va].append(d)
    return out


def _denom_pick(dems):
    """预注册 §2.0：能解出游戏模块的第一个名字；否则 `.symtab` 顺序第一个。"""
    import naming_android as NA
    for d in dems:
        p = NA.parse_name(d)
        if p is not None:
            return d, p
    return dems[0], NA.parse_name(dems[0])


def _denom_ident(d, parsed):
    """源码身份 (module, owner, method)：抹泛型实参、抹 h<hash>、`{{closure}}` 归父。"""
    d = _DENOM_HASH.sub("", d)
    if _DENOM_CLOSURE in d:
        d = d.split(_DENOM_CLOSURE)[0]
    method = None
    if ">::" in d:
        method = d[d.rindex(">::") + 3:]
    elif "::" in d:
        method = d.split("::")[-1]
    if method:
        method = re.sub(r"<.*", "", method).strip(":<> ")
    if not method and parsed:
        method = re.sub(r"<.*", "", parsed[2] or "").strip()
    module = (parsed[0] if parsed else None) or "<none>"
    owner = (parsed[1] if parsed else None) or "_"
    return (module, owner, method or "_")


def _denom_form(d, mangled):
    """预注册 §2.2 形态判定（顺序 = §2.5 优先级）。"""
    if "drop_in_place" in mangled or "drop_in_place" in d:
        return "drop-glue"
    if "ptrcall_fn" in mangled or "varcall_fn" in mangled:
        return "call-shim"          # 先于 reg-table：T97 对 `__register_exports::ptrcall_fn` 判 call-shim
    for k in ("__register_methods", "__register_constants", "__register_exports"):
        if k in mangled:
            return "reg-table"      # __register_exports：§9 勘误 E1（T97 已发表先例 0x43f634）
    if ("virtual_fn" in mangled or "virtual_call" in mangled
            or "__default_virtual_call" in mangled):
        return "virtual-shim"
    if "class_id" in mangled:
        return "class-id"
    if "__inner_init" in mangled or "__init" in mangled:
        return "ctor-shell"
    if "clap_builder::derive" in d or "clap::derive" in d:
        return "clap-derive"
    if re.search(r" as serde::de::(Visitor|Expected)>", d) \
            or re.search(r" as core::clone::Clone>::clone", d):
        return "macro-derive"
    if _DENOM_CLOSURE in d:
        return "closure"
    return "business"


def _denom_android_feats(side, ranges):
    import naming_android as NA
    feats = NA.extract(side, ranges, arm64=True)
    out = {}
    for va, f in feats.items():
        game = sorted({"%s:%d" % (os.path.basename(p), ln)
                       for p, ln in f["locs"] if _denom_game_key(p)})
        other = sorted({"%s:%d" % (os.path.basename(p), ln)
                        for p, ln in f["locs"] if not _denom_game_key(p)})
        out[va] = dict(size=f["size"], game=game, other=other, calls=tuple(f["calls"]))
    return out


def _denom_arm64_callsites(side):
    """全 `.text` 一遍：{target: [bl 调用点…]}。"""
    from capstone import Cs, CS_ARCH_ARM64, CS_MODE_ARM
    md = Cs(CS_ARCH_ARM64, CS_MODE_ARM)
    md.detail = True
    t = side.text
    code = side.elf.data[t["offset"]:t["offset"] + t["size"]]
    out = collections.defaultdict(list)
    for ins in md.disasm(code, t["addr"]):
        if ins.mnemonic == "bl" and ins.operands and ins.operands[0].type == 2:
            out[ins.operands[0].imm].append(ins.address)
    return out


def _denom_relative_addends(elf):
    import naming_android as NA
    c = collections.Counter()
    for _roff, typ, _sym, radd in NA.rela_entries(elf, ".rela.dyn"):
        if typ == 1027:                    # R_AARCH64_RELATIVE
            c[radd] += 1
    return c


def _denom_linux_keys():
    """L527 的 Location 键集：**只用已入库的 `fixtures/boundary_audit.tsv`**（不重扫 Linux）。"""
    out = {}
    for ln in open(os.path.join(ROOT, "fixtures", "boundary_audit.tsv"), encoding="utf-8"):
        if ln.startswith("#"):
            continue
        p = ln.rstrip("\n").split("\t")
        if p[0] == "addr" or not p[0].startswith("0x"):
            continue
        lines = set(int(x) for x in p[4].split(",") if x.strip().isdigit())
        others = {}
        for chunk in p[6].split(";"):
            m = re.match(r"src/(\S+?)\((\d+)\)", chunk.strip())
            if m:
                others[m.group(1)] = int(m.group(2))
        out[int(p[0], 16)] = dict(name=p[1], main=os.path.basename(p[3]), lines=lines,
                                  others=others)
    return out


def _denom_linux_selftest(lin):
    """预注册 §5 C5：用同一套 Location 解析对 527 行复算 `(主文件, 主文件行数)` 与台账对照。"""
    import naming_android as NA
    import pickle
    l = NA.Side(DENOM_BINARY, "linux")
    g = pickle.load(open(NA.find(NA.GHIDRA_REL), "rb"))
    gs = sorted(int(x, 16) if isinstance(x, str) else int(x) for x in g)
    fdes = dict(NA.fde_ranges(l.elf))
    starts = sorted(set(gs) | set(fdes))
    pos = {v: i for i, v in enumerate(starts)}
    ranges = {}
    for va in lin:
        if va in fdes:
            ranges[va] = fdes[va]
        else:
            i = pos.get(va)
            nxt = starts[i + 1] if (i is not None and i + 1 < len(starts)) else va + 0x40
            ranges[va] = max(0x10, nxt - va)
    feats = NA.extract(l, ranges, arm64=False)
    same, diff, noloc, detail = 0, 0, 0, []
    for va, rec in sorted(lin.items()):
        per = collections.Counter()
        for path, _line in feats[va]["locs"]:
            if _denom_game_key(path):
                per[os.path.basename(path)] += 1
        if not per:
            noloc += 1
            detail.append((va, "复算无 game 键", "%s×%d" % (rec["main"], len(rec["lines"]))))
            continue
        main = min(per, key=lambda m: (-per[m], m))     # = boundary_audit.pick_main 同规则
        if main == rec["main"] and per[main] == len(rec["lines"]):
            same += 1
        else:
            diff += 1
            detail.append((va, "%s×%d" % (main, per[main]),
                           "%s×%d" % (rec["main"], len(rec["lines"]))))
    return dict(total=len(lin), same=same, diff=diff, noloc=noloc, detail=detail)


def _denom_ours_index():
    """{module: [(path, lineno, text)]}；module→我方文件 用 `docs/coverage-matrix.tsv` 的 ours 列。"""
    matrix = os.path.join(ROOT, "docs", "coverage-matrix.tsv")
    files = {}
    for ln in open(matrix, encoding="utf-8"):
        if ln.startswith("#"):
            continue
        p = ln.rstrip("\n").split("\t")
        if len(p) < 6 or not p[0].endswith(".rs"):
            continue
        mod = p[0][:-3]
        ours = p[5].strip()
        files[mod] = [x for x in re.split(r"[+,]", ours) if x.strip().endswith(".rs")]
    out, extra = {}, {}
    for fn in sorted(os.listdir(os.path.join(ROOT, "src"))):
        if not fn.endswith(".rs"):
            continue
        path = os.path.join(ROOT, "src", fn)
        try:
            rows = [(path, i, t) for i, t in
                    enumerate(open(path, encoding="utf-8", errors="replace"), 1)]
        except OSError:
            continue
        extra[fn[:-3]] = rows
    for mod in set(list(files) + list(extra)):
        want = [os.path.basename(x) for x in files.get(mod, [])] or [mod + ".rs"]
        bag = []
        for fn in want:
            key = fn[:-3] if fn.endswith(".rs") else fn
            bag.extend(extra.get(key, []))
        if not bag:                        # 映射文件不在 src（如被拆/改名）⇒ 回退到同名文件
            bag = extra.get(mod, [])
        out[mod] = bag
    return out, extra


def _denom_find_fn(rows, method):
    if not method or method == "_":
        return None
    pat = re.compile(r"\bfn\s+" + re.escape(method) + r"\s*(?:<|\()")
    for path, i, t in rows or []:
        s = t.lstrip()
        if s.startswith("//") or s.startswith("#["):
            continue
        if pat.search(t):
            return "%s:%d" % (os.path.relpath(path, ROOT), i)
    return None


def _denom_ours_lookup(module, method, own_idx, all_idx):
    """三级：①本模块映射文件里（强）②别的 src 文件里同名 fn（弱，跨模块同名）③_scaffold。"""
    hit = _denom_find_fn(own_idx.get(module), method)
    if hit:
        return "own-module", hit
    for mod, rows in all_idx.items():
        if mod == module:
            continue
        hit = _denom_find_fn(rows, method)
        if hit:
            return "elsewhere", hit
    d = os.path.join(ROOT, "src", "_scaffold")
    if os.path.isdir(d):
        pat = re.compile(r"\bfn\s+" + re.escape(method) + r"\s*(?:<|\()")
        for fn in sorted(os.listdir(d)):
            if not fn.endswith(".rs"):
                continue
            try:
                txt = open(os.path.join(d, fn), encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            if pat.search(txt):
                return "scaffold-only", "src/_scaffold/" + fn
    return "missing", ""


def _denom_classify(side, by, size, nxt, addrs, dems, feats, bl, rel, lin, own_idx, all_idx):
    rows = []
    for va in addrs:
        d, parsed = _denom_pick(dems[va])
        ident = _denom_ident(d, parsed)
        form = _denom_form(d, by[va][0])
        f = feats[va]
        ks = set()
        for k in f["game"]:
            fn, _, line = k.rpartition(":")
            ks.add((fn, int(line)))
        k_join = sorted(lv for lv, lr in lin.items()
                        if ks & {(lr["main"], x) for x in lr["lines"]})
        n_join = sorted(lv for lv, lr in lin.items()
                        if _denom_norm(lr["name"]) == _denom_norm(d))
        rows.append(dict(addr=va, d=d, ident=ident, form=form, st_size=size[va],
                         real_span=min(size[va], max(1, nxt[va] - va)),
                         n_sym=len(by[va]), game=f["game"], other=f["other"],
                         rel32=len(bl.get(va, ())), slot=rel.get(va, 0),
                         k_join=k_join, n_join=n_join))
    return rows


def _denom_norm(d):
    d = _DENOM_HASH.sub("", d).split(_DENOM_CLOSURE)[0]
    d = re.sub(r"<.*", "", d).replace("graphwarrust::", "")
    if d.endswith(".rs"):
        d = d[:-3]
    return "::".join(x for x in d.split("::") if x)


def _denom_verdicts(rows, own_idx, all_idx):
    grp_loc, grp_ours = {}, {}
    for r in rows:
        k = r["ident"]
        grp_loc[k] = grp_loc.get(k, False) or bool(r["game"])
        if k not in grp_ours:
            grp_ours[k] = _denom_ours_lookup(k[0], k[2], own_idx, all_idx)
    for r in rows:
        form = r["form"]
        if form != "business":
            cat, sub = DENOM_FORM_CLASS[form]
            if r["st_size"] > DENOM_VOL_CAP:
                r["verdict"], r["subclass"] = "unknown", "volume-gate"
                r["note"] = ("宏形态但 st_size 0x%x > VOL_CAP 0x%x ⇒ 逐字继承 T82 §2.5 降 unknown"
                             "（T97 有同款先例 unknown/volcap:past-cap）" % (r["st_size"], DENOM_VOL_CAP))
            else:
                r["verdict"], r["subclass"] = cat, sub
                r["note"] = "名字形态=%s；st_size 0x%x ≤ VOL_CAP" % (form, r["st_size"])
            continue
        if grp_loc[r["ident"]]:
            r["verdict"], r["subclass"] = "denom:in", "has-game-loc"
            r["note"] = "组内至少一条地址体内有游戏 Location 键（观测）⇒ 源码函数有 panic 点 ⇒ 不属分母外"
        else:
            how, where = grp_ours[r["ident"]]
            r["verdict"] = {"own-module": "implemented", "elsewhere": "implemented-alt",
                            "scaffold-only": "scaffold-only"}.get(how, "real-missing")
            r["verdict"] += "/denom:no-loc"
            r["subclass"] = "denom:no-loc"
            r["ours_hint"], r["ours_how"] = where, how
            r["note"] = ("组内全部地址 gameLOC=0（**观测**）⇒ 结构上进不了 527 分母"
                         "（Linux 侧亦无 = **推断**，缺跨构建键）"
                         + ("；我方 %s: %s" % (how, where) if where else ""))
        if r["st_size"] > DENOM_VOL_CAP:
            r["note"] += "；st_size 0x%x > VOL_CAP（business 不受体积闸约束，只登记）" % r["st_size"]
    return rows, grp_loc, grp_ours


def _denom_control(rows, by_verdict, verbose=True):
    """预注册 §5 的 C0–C5。返回 (ok, 明细行)。"""
    import naming_android as NA
    out = []
    ok = True

    def gap_rows(path, iver=10, isub=11):
        rows_ = {}
        for ln in open(os.path.join(ROOT, path), encoding="utf-8"):
            if ln.startswith("#"):
                continue
            p = ln.rstrip("\n").split("\t")
            if len(p) > isub and p[0].startswith("0x"):
                rows_[int(p[0], 16)] = dict(size=p[1], name=p[2], ver=p[iver], sub=p[isub])
        return rows_

    def get(va):
        return by_verdict.get(va)

    # C0：口径自校验
    c0_g = len(rows)
    c0_l = len(_denom_linux_keys())
    out.append(("C0", "|G| = %d（期望 1244）、|L527| = %d（期望 527）" % (c0_g, c0_l),
                c0_g == 1244 and c0_l == 527))
    ok &= out[-1][2]
    # C1/C2/C3：t101 18 / t102 3 / t97 3
    for tag, path, kw, n_exp in (("C1", "fixtures/gap_nodes.tsv", {}, 18),
                                 ("C2", "fixtures/gap_plane.tsv", dict(iver=3, isub=4), 3),
                                 ("C3", "fixtures/gap_rootnode.tsv", {}, 3)):
        g = gap_rows(path, **kw)
        want = [va for va, r in g.items() if r["ver"].startswith("real-missing")]
        bad = []
        for va in want:
            r = get(va)
            if r is None or not r["verdict"].endswith("denom:no-loc") or r["game"]:
                bad.append((va, r["verdict"] if r else "缺失",
                            len(r["game"]) if r else -1))
        out.append((tag, "%s：%d/%d 条判进 denom:no-loc 且 gameLOC=0" % (path, len(want) - len(bad), len(want)),
                    not bad and len(want) == n_exp))
        if bad:
            out.append((tag, "   偏差 %s" % bad, False))
        ok &= out[-1][2]
    # C4a：负对照（有 Location 的必须 denom:in）
    r = get(0x39A6F0)
    good = bool(r) and r["verdict"] == "denom:in" and len(r["game"]) > 0
    out.append(("C4a", "android 0x39a6f0（root_node::ready，K=%s）判 %s"
                % (len(r["game"]) if r else "?", r["verdict"] if r else "缺失"), good))
    ok &= good
    # C4b：t97 的 27 条 na:* 一条都不许变 business
    g = gap_rows("fixtures/gap_rootnode.tsv")
    na_rows = [(va, r) for va, r in g.items() if r["ver"].startswith("na:")]
    bad = [(va, get(va)["verdict"] if get(va) else "缺失") for va, _r in na_rows
           if not (get(va) and get(va)["verdict"].startswith("na:"))]
    out.append(("C4b", "t97 27 条 na:* 里仍判 na:* 的 %d / %d" % (len(na_rows) - len(bad), len(na_rows)),
                not bad and len(na_rows) == 27))
    if bad:
        out.append(("C4b", "   偏差 %s" % bad[:6], False))
    ok &= out[-1][2]
    # C4c：T98 三条手写方法的边界（serialize/deserialize 不许被 macro-derive 吞掉）
    t98 = [r for r in rows
           if r["ident"][0] == "graphwar_event"
           and r["ident"][2] in ("serialize", "deserialize",
                                 "make_new_connection_request_event")]
    bad = [r["addr"] for r in t98 if r["form"] != "business"]
    out.append(("C4c", "T98 手写方法 %d 条（机器码）form=business %d 条（serialize/deserialize 没被吞）"
                % (len(t98), len(t98) - len(bad)), bool(t98) and not bad))
    ok &= out[-1][2]
    # C4d：体积闸负对照
    c4d = [r for r in rows if r["st_size"] > DENOM_VOL_CAP and r["form"] != "business"]
    bad = [r["addr"] for r in c4d if r["verdict"].startswith("na:")]
    out.append(("C4d", "宏形态且 st_size>VOL_CAP 的 %d 条里判 na:* 的 %d 条（必须 0）"
                % (len(c4d), len(bad)), not bad and bool(c4d)))
    ok &= out[-1][2]
    # C5：Linux 侧 LOC 复算
    c5 = _denom_linux_selftest(_denom_linux_keys())
    pct = 100.0 * c5["same"] / c5["total"]
    out.append(("C5", "Linux 527 行 LOC 复算 vs boundary_audit：%d/%d = %.1f%%（≥95%% 才算过）"
                % (c5["same"], c5["total"], pct), pct >= 95.0))
    ok &= out[-1][2]
    if verbose:
        for tag, msg, good_ in out:
            print("  %-4s %s %s" % (tag, "✓" if good_ else "✗", msg))
        if c5["detail"]:
            print("       C5 残差 %d 条：" % len(c5["detail"]))
            for va, got, want in c5["detail"][:12]:
                print("         %#010x 复算 %-38s 台账 %s" % (va, got, want))
    return ok, out, c5


def cmd_denom_outside(argv=None):
    """`--denom-outside`：跑 C0–C5 → 全量判决 → 写 `fixtures/denom_outside.tsv`。"""
    import naming_android as NA
    control_only = bool(getattr(argv, "control_only", False))
    print("=== T105 分母外量化（预注册 fixtures/denom_outside_plan.txt）")
    side, by, size, nxt, addrs = _denom_android()
    dems = _denom_demangle(by, addrs)
    lin = _denom_linux_keys()
    print("  G = %d 条机器码；L527 = %d 行" % (len(addrs), len(lin)))
    print("\n=== 全量特征抽取（LOC / rel32 / slot）…", flush=True)
    feats = _denom_android_feats(side, {va: size[va] for va in addrs})
    bl = _denom_arm64_callsites(side)
    rel = _denom_relative_addends(side.elf)
    own_idx, all_idx = _denom_ours_index()
    rows = _denom_classify(side, by, size, nxt, addrs, dems, feats, bl, rel, lin,
                          own_idx, all_idx)
    rows, grp_loc, grp_ours = _denom_verdicts(rows, own_idx, all_idx)
    by_verdict = {r["addr"]: r for r in rows}

    print("\n=== 正/负对照（预注册 §5）")
    ok, ctl, c5 = _denom_control(rows, by_verdict)
    if not ok:
        print("\n✗ 对照未全过 ⇒ 按预注册 **不写文件**、不出全量数。")
        return 2

    # ---- 计数 ----
    groups = collections.defaultdict(list)
    for r in rows:
        groups[r["ident"]].append(r)
    agg = collections.Counter(r["verdict"] for r in rows)
    form_mat = collections.Counter((r["form"], r["verdict"]) for r in rows)

    # 组级判定用 **form=business 的实例**（闭包实例按 §2.4 归父，不单独算源码函数；
    # 它们可能是 na:other/closure-in-fn，但父函数仍是 denom:no-loc 的一个源码函数）
    def grp_of(prefix):
        out = {}
        for k, v in groups.items():
            bs = [x for x in v if x["form"] == "business"]
            if bs and all(x["verdict"] == prefix + "/denom:no-loc" for x in bs):
                out[k] = bs                    # 只保留 business 实例（= 该源码函数的机器码）
        return out
    g_miss, g_impl, g_alt, g_scaf = (grp_of("real-missing"), grp_of("implemented"),
                                     grp_of("implemented-alt"), grp_of("scaffold-only"))
    x_total = len(g_miss) + len(g_impl) + len(g_alt) + len(g_scaf)
    x_mc = sum(len(v) for v in list(g_miss.values()) + list(g_impl.values())
               + list(g_alt.values()) + list(g_scaf.values()))
    mixed_form = sum(1 for k, v in groups.items()
                     if len([x for x in v if x["form"] == "business"]) != len(v))
    per_mod = collections.Counter()
    per_mod_mc = collections.Counter()
    per_mod_impl = collections.Counter()
    for k, v in list(g_miss.items()) + list(g_impl.items()) + list(g_alt.items()) + list(g_scaf.items()):
        per_mod[k[0]] += 1
        per_mod_mc[k[0]] += len(v)
        if k in g_impl or k in g_alt:
            per_mod_impl[k[0]] += 1
    print("\n=== 计数（机器码 %d / 源码组 %d）" % (len(rows), len(groups)))
    for k, v in sorted(agg.items()):
        print("   %-34s %d" % (k, v))
    print("\n   form × verdict：")
    for (f, v), n in sorted(form_mat.items()):
        print("     %-14s %-34s %4d" % (f, v, n))
    print("\n   **分母外真实源码函数 X = %d**（机器码 %d）："
          "real-missing %d / implemented(本模块) %d / implemented(跨模块同名) %d / scaffold-only %d"
          % (x_total, x_mc, len(g_miss), len(g_impl), len(g_alt), len(g_scaf)))
    # mixed 组灵敏度
    mixed = sum(1 for v in groups.values()
                if any(x["game"] for x in v) and any(not x["game"] for x in v))
    print("   混合组（同组部分实例有 game 键）%d 组 ⇒ 若改「全部实例有键才算 in」，X 最多再涨 %d"
          % (mixed, mixed))
    print("   另有 %d 组的组内混着 `closure` 实例（按 §2.4 归父，不单独算源码函数）" % mixed_form)

    if control_only:
        print("\n（--control-only：不写文件）")
        return 0

    # ---- 写 TSV ----
    head = []
    head.append("denom_outside.tsv —— T105：**527 分母之外**的真实源码函数全量量化（do not edit）")
    head.append("重算：$PY tools/global_triage.py --denom-outside   判据：fixtures/denom_outside_plan.txt（预注册，commit 342b534）")
    head.append("地址口径：ELF vaddr（RVA）。android_addr = **Android 2.0.60 arm64**；linux 侧只出现在 linux_pair 列的 K/N-join 里")
    head.append("来源：Android = .reference/steam/android-2.0.60/libgraphwarrust.so（未 strip，sha256 8a612e95…54df）")
    head.append("      Linux   = .reference/steam/2026-08-28/libgraphwarrust.so（stripped，sha256 9a538115…b187）")
    head.append("G = Android .text 内任一 .symtab 名含 graphwarrust 的地址（本表 %d 行）；L527 = fixtures/boundary_audit.tsv 的 527 行" % len(rows))
    head.append("判据（预注册 §2）：形态 → 类别（na:* 要求 st_size ≤ VOL_CAP 0x800，超限降 unknown/volume-gate）；")
    head.append("  business 再按**组级**判：组内任一条体内有**游戏** panic::Location 键 ⇒ denom:in；全组无 ⇒ denom:no-loc；")
    head.append("  源码组 = (module, owner, method) 抹泛型实参/抹 h<hash>/closure 归父 ⇒ 同组多条机器码 = 1 个源码函数。")
    head.append("列：android_addr | st_size | real_span | module | owner | method | form | n_sym | game_loc | other_loc")
    head.append("    | rel32 | slot | linux_pair | verdict | subclass | src_fn_group | ours_src | evidence | note")
    head.append("  st_size = .symtab 的 st_size（T101 §149：会被重叠符号骗）；real_span = min(st_size, 下一个 FUNC − 本地址)，**只入表不进判决**")
    head.append("  game_loc = Android 侧命中的**游戏** Location 键（basename:line）；other_loc = std/godot/第三方键（不是游戏键）")
    head.append("  rel32 = 全 .text 里 bl 直呼本地址的调用点条数；slot = .rela.dyn 的 R_AARCH64_RELATIVE addend == 本地址的条数（T89 口径，只入表）")
    head.append("  linux_pair 三态：K-join:<linux vaddr>（(file,line) 键集相交 = 观测同源）/ N-join:<linux vaddr>（源码身份名相等 = 推断）")
    head.append("                  / —(L空)（无任何跨构建键 —— 不是\"没有对应函数\"，是\"冻结夹具证不到对应\"）")
    head.append("  evidence：gameLOC 为空是**观测**；\"Linux 侧也没有 Location\"、\"该由我们写\"是**推断**（缺跨构件键：Linux stripped + 无 Location 无键）")
    head.append("计数（机器码 %d / 源码组 %d）：" % (len(rows), len(groups)))
    for k, v in sorted(agg.items()):
        head.append("  %-34s %d" % (k, v))
    head.append("**分母外真实源码函数 X = %d 组 / %d 条机器码**：" % (x_total, x_mc))
    head.append("  real-missing %d 组 / %d 条｜implemented(本模块) %d 组 / %d 条｜implemented-alt(跨模块同名, 弱证据) %d 组 / %d 条｜scaffold-only %d 组 / %d 条"
                % (len(g_miss), sum(len(v) for v in g_miss.values()),
                   len(g_impl), sum(len(v) for v in g_impl.values()),
                   len(g_alt), sum(len(v) for v in g_alt.values()),
                   len(g_scaf), sum(len(v) for v in g_scaf.values())))
    head.append("⚠️ 「implemented」= 我方 `src/` 里有同名 `fn`（名义 + 模块级文件映射），**不是**逐位/行为证据；")
    head.append("   「implemented-alt」名字命中的是**别的模块**的 fn，只算弱证据（可能不是同一个函数）。X（分母）与二者的拆分无关。")
    head.append("正/负对照（本轮实跑，全过）：")
    for tag, msg, good_ in ctl:
        head.append("  %-4s %s %s" % (tag, "✓" if good_ else "✗", msg))
    head.append("按模块（X 组数 / 机器码 / 其中已实现）：")
    for m, n in per_mod.most_common():
        head.append("  %-34s %3d 组 / %4d 条 / 已实现 %d" % (m, n, per_mod_mc[m], per_mod_impl[m]))
    with open(DENOM_TSV, "w", encoding="utf-8") as fh:
        for ln in head:
            fh.write("# " + ln + "\n")
        fh.write("android_addr\tst_size\treal_span\tmodule\towner\tmethod\tform\tn_sym\tgame_loc"
                 "\tother_loc\trel32\tslot\tlinux_pair\tverdict\tsubclass\tsrc_fn_group\tours_src"
                 "\tevidence\tnote\n")
        for r in sorted(rows, key=lambda x: x["addr"]):
            if r["k_join"]:
                pair = "K-join:" + ";".join("0x%06x" % x for x in r["k_join"])
            elif r["n_join"]:
                pair = "N-join:" + ";".join("0x%06x" % x for x in r["n_join"])
            else:
                pair = "—(L空)"
            mod, own, meth = r["ident"]
            ev = "观测:gameLOC=%d" % len(r["game"])
            if r["verdict"].endswith("denom:no-loc"):
                ev += "；推断:Linux 侧无键可证"
            fh.write("\t".join([
                "0x%06x" % r["addr"], str(r["st_size"]), str(r["real_span"]),
                mod, own, meth, r["form"], str(r["n_sym"]),
                ",".join(r["game"]) or "0", ",".join(r["other"]) or "0",
                str(r["rel32"]), str(r["slot"]), pair, r["verdict"], r["subclass"],
                "%s::%s::%s" % (mod, own, meth), r.get("ours_hint") or "—",
                ev, r.get("note", ""),
            ]) + "\n")
    print("\n写出 %s（%d 行）" % (DENOM_TSV, len(rows)))
    return 0


def load_noloc_counts():
    """读 `fixtures/denom_outside.tsv` → {module(.rs 后缀): 分母外源码函数数}。

    缺文件返回 None（调用方按"没有这列"处理，**不猜测、不静默填 0**）。
    """
    if not os.path.exists(DENOM_TSV):
        return None
    groups = {}
    for ln in open(DENOM_TSV, encoding="utf-8"):
        if ln.startswith("#"):
            continue
        p = ln.rstrip("\n").split("\t")
        if p[0] == "android_addr" or not p[0].startswith("0x"):
            continue
        if not p[13].endswith("denom:no-loc"):
            continue
        groups[p[15]] = p[3]
    cnt = collections.Counter(groups.values())
    return {m + ".rs": n for m, n in cnt.items()}


# ---------------------------------------------------------------------------
# 单条取证 / 抽样
# ---------------------------------------------------------------------------
def cmd_dump(addrs):
    ctx = ap.Ctx(ap.SO_DEFAULT)
    _, starts, data = build_index(verbose=False)
    funcs = data["funcs"]
    idx = data["targets"]
    for a in addrs:
        s = int(a, 16)
        f = funcs.get(s)
        if not f:
            # 找所属 funcstart
            i = bisect.bisect_right(starts, s) - 1
            s = starts[i]
            f = funcs[s]
            print("\n%#x 不是 funcstart，落到所属 %#x" % (int(a, 16), s))
        print("\n=== %#x size=%#x size_fde=%#x insn=%d callers=%d slots=%d ==="
              % (s, f["size"], f["size_fde"], f["n_insn"], len(data["callers"].get(s, ())),
                 len(f["slots"])))
        locs = []
        strs = []
        for t in f["rips"]:
            k = idx.get(t)
            if not k:
                continue
            if k[0].startswith("loc:"):
                L = any_location(ctx, t)
                locs.append((k[0], "%s:%d:%d" % L if L else k[1]))
            elif k[0] == "str":
                strs.append(k[1])
        for fam, L in locs[:6]:
            print("  LOC[%s] %s" % (fam, L))
        for x in strs[:10]:
            print("  STR  %s" % x[:110])
        print("  self_calls=%d strides=%s ucomisd=%d jp=%d bulk=%d cmov=%d atomics=%d borrow=%d"
              % (f["self_calls"], ",".join("%#x" % v for v in f["strides"][:3]),
                 f["ucomisd"], f["jp"], f["bulk"], f["cmov"], len(f["atomics"]), len(f["borrow"])))
        print("  callees=%s" % ",".join("%#x" % c for c in f["callees"][:8]))


def cmd_sample(args):
    """**预注册抽样 + 人工复核清单**（种子写死，可复算）。"""
    import random as _r
    _r.seed(args.seed)
    rows = [r for r in read_tsv(OUT) if r["addr"].startswith("0x")]
    ctx, starts, data, _rows = build_all(verbose=False)
    funcs = data["funcs"]
    idx = data["targets"]
    by = collections.defaultdict(list)
    for r in rows:
        by[r["cls"]].append(r)
    PLAN = [(CLS_REAL, 0.10), (CLS_STD, 0.02), (CLS_GDX, 0.02),
            (CLS_OTHER, 0.02), (CLS_UNK, 0.01)]
    picked = []
    print("=== 预注册抽样（seed=%d）===" % args.seed)
    for cls, frac in PLAN:
        pool = sorted(by.get(cls, []), key=lambda r: r["addr"])
        k = max(1, int(round(len(pool) * frac)))
        sel = sorted(_r.sample(pool, min(k, len(pool))), key=lambda r: r["addr"])
        print("  %-14s 池 %5d × %4.1f%% ⇒ 抽 %3d" % (cls, len(pool), frac * 100, len(sel)))
        for r in sel:
            picked.append((cls, r, len(pool), k))
    with open(OUT_SAMPLE, "w", encoding="utf-8") as fh:
        fh.write("# global_triage_sampling.tsv —— **预注册抽样复核**（tools/global_triage.py --sample 生成）\n")
        fh.write("# 抽样率：real-missing 10%%；na:std-mono / na:gdext-glue / na:other 各 2%%；unknown 1%%\n")
        fh.write("#            （计划书 fixtures/global_triage_plan.txt 写的是 na 与 unknown 各 5%%；\n")
        fh.write("#             5%% 合计约 690 条不可能逐条人看 ⇒ **入库取样率下调并在此写明**，误差按实抽样本算）\n")
        fh.write("# seed=%d；verdict 由人工逐条读 dis/串/Location 后填写（agree / disagree / unclear）\n" % args.seed)
        fh.write("cls\taddr\tsub\tlayer\tsize\tmodule\tsample_n\tpool_n\tverdict\tnote\n")
        for cls, r, pool_n, k in picked:
            fh.write("\t".join([cls, r["addr"], r["sub"], r["layer"], r["size"], r["module"],
                                str(k), str(pool_n), "", ""]) + "\n")
    print("\n写出 %s（%d 条）" % (OUT_SAMPLE, len(picked)))
    # 人工复核清单（不提交，只给本轮读）
    dump = os.path.join(WORK, "review_dump.txt")
    with open(dump, "w", encoding="utf-8") as fh:
        for cls, r, pool_n, k in picked:
            a = int(r["addr"], 16)
            f = funcs.get(a)
            if not f:
                continue
            locs, strs = [], []
            for t in f["rips"]:
                kk = idx.get(t)
                if not kk:
                    continue
                if kk[0] == "loc":
                    L = any_location(ctx, t)
                    if L:
                        locs.append(L[0].split("/")[-1] + ":" + str(L[1]))
                elif kk[0] == "str":
                    strs.append(kk[1])
            nb = set(data["callers"].get(a, ()))
            clsof = {}
            for rr in read_tsv(OUT):
                if rr["addr"].startswith("0x"):
                    clsof[int(rr["addr"], 16)] = rr
            nbl = []
            for x in list(nb)[:6]:
                rr = clsof.get(x)
                if rr:
                    nm = rr["name"] or ""
                    nbl.append("%s(%s%s)" % ("%#x" % x, rr["cls"],
                                             (":" + nm.split("::")[-1][:18]) if nm else ""))
            nbc = collections.Counter(clsof[x]["cls"] for x in nb if x in clsof)
            fh.write("\n[%s] %s size=%s layer=%s sub=%s module=%s callers=%d slots=%d\n"
                     % (cls, r["addr"], r["size"], r["layer"], r["sub"], r["module"],
                        len(nb), len(f["slots"])))
            fh.write("   locs: %s\n" % ("; ".join(locs[:3]) or "—"))
            fh.write("   strs: %s\n" % (" | ".join(x[:60] for x in strs[:4]) or "—"))
            fh.write("   feat: self=%d strides=%s cmp=%d/%d bulk=%d cmov=%d atomics=%d borrow=%d\n"
                     % (f["self_calls"], ",".join("%#x" % v for v in f["strides"][:2]),
                        f["ucomisd"], f["jp"], f["bulk"], f["cmov"],
                        len(f["atomics"]), len(f["borrow"])))
            fh.write("   nbr: %s | 邻居类分布 %s\n"
                     % ("; ".join(nbl) or "—", dict(nbc.most_common(4))))
    print("复核清单（不提交）：%s" % dump)




# ---------------------------------------------------------------------------
# 自校验（预注册：fixtures/global_triage_refresh_plan.txt §5 的 S1–S3）
#   只读：不写任何 fixtures。rc≠0 = 失败。
# ---------------------------------------------------------------------------
MODULES_TSV_X = MODULES_TSV          # 同一路径（下面用得到，起个别名防止被误改）


def _expected_modules():
    """独立重算模块表（只用 read_boundary/read_cov/f3 标签 + Android 普查表，**不经过 classify**）。"""
    bnd = read_boundary()
    cov = {int(r["addr"], 16): r for r in read_cov()}
    lin, f12 = collections.Counter(), collections.Counter()
    for a, r in cov.items():
        mod = os.path.basename(bnd.get(a, {}).get("loc_main_file") or "")
        if mod:
            lin[mod] += 1
            if r["bucket"] in ("F1", "F2"):
                f12[mod] += 1
    f3, f3real = collections.Counter(), collections.Counter()
    for a, lab in F3_LABEL.items():
        if (cov.get(a) or {}).get("bucket") != "F3":
            continue
        mod = os.path.basename(bnd.get(a, {}).get("loc_main_file") or "")
        f3[mod] += 1
        if lab == CLS_REAL:
            f3real[mod] += 1
    mods = {}
    if os.path.exists(CENSUS_TSV):
        with open(CENSUS_TSV, encoding="utf-8") as fh:
            for ln in fh:
                if ln.startswith("game_module\t"):
                    _, k, v = ln.rstrip("\n").split("\t")
                    mods[k] = int(v)
    allmods = sorted(set(k + ".rs" if not k.endswith(".rs") else k for k in mods) | set(lin) | set(f3))
    rows = []
    for m in allmods:
        key = m[:-3] if m.endswith(".rs") else m
        rows.append((m, mods.get(key, 0), lin.get(m, 0), f12.get(m, 0),
                     f3.get(m, 0), f3real.get(m, 0), mods.get(key, 0) - f12.get(m, 0)))
    rows.sort(key=lambda r: (-r[1], -r[2]))
    return rows


def _read_modules_tsv(path):
    out = {}
    with open(path, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("#"):
                continue
            p = ln.rstrip("\n").split("\t")
            if p[0] == "module" or len(p) < 7:
                continue
            # T105：第 8 列 lin_fns_noloc 不参与 S1 比对（S1 的口径是 T96 那 6 列）
            out[p[0]] = tuple(int(x) for x in p[1:7])
    return out


def cmd_self_check(argv):
    fails = []

    def bad(msg):
        fails.append(msg)
        print("  ✗ %s" % msg)

    # ---- S1：模块表逐值 = 独立重算（含 §4.1 的合计期望） ----
    print("\n=== S1 模块表（fixtures/global_triage_modules.tsv）逐值核 ===")
    if not os.path.exists(MODULES_TSV_X):
        bad("缺 %s" % MODULES_TSV_X)
    else:
        want_rows = _expected_modules()
        want = {r[0]: tuple(r[1:]) for r in want_rows}
        got = _read_modules_tsv(MODULES_TSV_X)
        tot = dict(android=sum(r[1] for r in want_rows), lin527=sum(r[2] for r in want_rows),
                   f12=sum(r[3] for r in want_rows), f3=sum(r[4] for r in want_rows),
                   f3real=sum(r[5] for r in want_rows))
        print("  独立重算：模块 %d／android_fns %d／lin527 %d／lin_F1F2 %d／lin_F3 %d／lin_F3_real %d"
              % (len(want_rows), tot["android"], tot["lin527"], tot["f12"], tot["f3"], tot["f3real"]))
        if set(want) != set(got):
            bad("模块集合不一致：多 %s / 少 %s"
                % (sorted(set(got) - set(want))[:5], sorted(set(want) - set(got))[:5]))
        else:
            diff = [m for m in want if want[m] != got[m]]
            if diff:
                for m in diff[:10]:
                    bad("模块行不一致 %s：期望 %s 实为 %s" % (m, want[m], got[m]))
            else:
                print("  ✓ %d 个模块逐值相同（含 §4.1 预注册的 15 行变化）" % len(want))
        # 行序也要一致（表是机读表）
        if os.path.exists(MODULES_TSV_X):
            order = [m for m in _read_modules_tsv(MODULES_TSV_X)]
            if order != [r[0] for r in want_rows]:
                bad("模块行序与独立重算不同（前 3 个差异：期望 %s 实为 %s）"
                    % ([r[0] for r in want_rows][:3], order[:3]))

    # ---- S2：527 行的类逐条 = 今天台账的期望 ----
    # 注意（本轮实测更正预注册的写法）：layer=`L1-location` **不等于**"在 527 里" ——
    # 走 registry/std/gdext Location 分支的行 layer 也是 L1-location（2579 条，不在 527）。
    # 所以 S2 的正确写法是**以 527 台账为循环对象**：bucket F1/F2 ⇒ implemented；bucket F3 ⇒ f3_triage 标签。
    print("\n=== S2 527 行的类（按 bucket/f3 标签逐条核）===")
    gt = {}
    with open(OUT, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("#"):
                continue
            p = ln.rstrip("\n").split("\t")
            if len(p) < 12 or not p[0].startswith("0x"):
                continue
            gt[int(p[0], 16)] = dict(zip(["addr", "name", "size", "size_fde", "module", "attributed",
                                          "cls", "sub", "layer", "callers", "slots", "evid"], p))
    cov = {int(r["addr"], 16): r for r in read_cov()}
    lc = collections.Counter(gt[a]["layer"] for a in cov if a in gt)
    print("  527 行的 layer 分布：%s" % dict(lc.most_common()))
    n_f12 = n_f12_ok = n_f3 = n_f3_ok = 0
    anomalies = []
    for a, row in cov.items():
        g = gt.get(a)
        if not g:
            bad("%#x 在 527 台账里但不在 13,263 输出里" % a)
            continue
        b = row["bucket"]
        if b in ("F1", "F2"):
            n_f12 += 1
            if g["cls"] == CLS_IMPL:
                n_f12_ok += 1
            else:
                bad("%#x bucket=%s ⇒ 期望 implemented，实为 %s" % (a, b, g["cls"]))
        elif b == "F3":
            n_f3 += 1
            want = F3_LABEL.get(a)
            if not want:
                bad("%#x 是 F3 但 f3_triage.tsv 没有它的裁决" % a)
            elif g["cls"] == want:
                n_f3_ok += 1
            elif g["layer"] in ("L5-region", "L4-prop"):
                anomalies.append((a, b, want, g["cls"], g["sub"], g["layer"]))
            else:
                bad("%#x bucket=F3 标签=%s ⇒ 期望 %s，实为 %s/%s（layer=%s）"
                    % (a, want, want, g["cls"], g["sub"], g["layer"]))
        else:
            bad("%#x 的 bucket = %r" % (a, b))
    print("  F1/F2 行 %d（全是 implemented：%d）；F3 行 %d（按 T82 标签对上 %d，落在推断层 %d）"
          % (n_f12, n_f12_ok, n_f3, n_f3_ok, len(anomalies)))
    # 推断层异常行是**旧快照就有**的既有性质（旧 7 条 → 今天 %d 条），不在本轮修的范围：
    for a, b, want, got, sub, layer in anomalies:
        print("    △ %#x F3 标签=%s，但该地址落在 %s（%s/%s）⇒ 台账在推断层被覆盖（既有性质，登记不改）"
              % (a, want, layer, got, sub))
    if len(anomalies) > 7:
        bad("落在推断层的 527 行 %d 条 > 旧快照的 7 条（本轮不该变多）" % len(anomalies))

    # ---- S3：MANUAL 面 = 恰好 49 行（默认跑 vs --no-manual 基线） ----
    print("\n=== S3 T93 MANUAL 面（默认 vs --no-manual）===")
    err = _manual_cross_check()
    if err:
        bad(err)
    else:
        print("  ✓ 人工表 49 地址与 fixtures/gap_modcontainer.tsv 逐条相等（48 own-generic-mono + 1 drop-glue）")
    n_manual = sum(1 for r in gt.values() if r["layer"] == MANUAL_LAYER)
    if n_manual != len(T93_MANUAL_NA):
        bad("输出里 layer=L1-t93 的行 %d ≠ 人工表 %d" % (n_manual, len(T93_MANUAL_NA)))
    for a, (sub, _n) in T93_MANUAL_NA.items():
        r = gt.get(a)
        if not r:
            bad("%#x 不在输出里" % a)
        elif (r["cls"], r["sub"]) != (CLS_OTHER, sub):
            bad("%#x 应为 %s/%s，实为 %s/%s" % (a, CLS_OTHER, sub, r["cls"], r["sub"]))
    base = getattr(argv, "baseline", None)
    if base:
        if not os.path.exists(base):
            bad("基线文件不存在：%s" % base)
        else:
            old = {}
            with open(base, encoding="utf-8") as fh:
                for ln in fh:
                    if ln.startswith("#"):
                        continue
                    p = ln.rstrip("\n").split("\t")
                    if len(p) < 12 or not p[0].startswith("0x"):
                        continue
                    old[int(p[0], 16)] = p
            changed = {a for a in gt if a in old and
                       (old[a][6], old[a][7]) != (gt[a]["cls"], gt[a]["sub"])}
            want = set(T93_MANUAL_NA)
            extra = sorted("%#x" % a for a in changed - want)
            missing = sorted("%#x" % a for a in want - changed)
            print("  --no-manual 基线 ↔ 默认：类别/子类有差异的行 %d（期望恰好 %d）"
                  % (len(changed), len(want)))
            if extra:
                bad("出现第 50 类改判（人工表之外）：%s" % extra[:10])
            if missing:
                bad("人工表里这些行没被改判：%s" % missing[:10])
            if not extra and not missing:
                print("  ✓ 改判面恰好 = 人工表 49 行，没有多一条")
    else:
        print("  （未给 --baseline，跳过\"改判面恰好 49 行\"的差集核；建议："
              "--no-manual --out <path> 后带 --baseline <path> 重跑）")

    print("\n自校验结论：%s（失败 %d 项）" % ("通过 ✓" if not fails else "失败 ✗", len(fails)))
    return 0 if not fails else 1


def cmd_write_diff(old_path):
    """写 `fixtures/global_triage_refresh_diff.tsv`：逐行改判台账（哪一行、原值、新值、依据）。"""
    if not os.path.exists(old_path):
        raise SystemExit("找不到旧快照：%s" % old_path)
    old = {}
    with open(old_path, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("#"):
                continue
            p = ln.rstrip("\n").split("\t")
            if len(p) < 12 or not p[0].startswith("0x"):
                continue
            old[int(p[0], 16)] = dict(zip(["addr", "name", "size", "size_fde", "module", "attributed",
                                           "cls", "sub", "layer", "callers", "slots", "evid"], p))
    new = {}
    with open(OUT, encoding="utf-8") as fh:
        for ln in fh:
            if ln.startswith("#"):
                continue
            p = ln.rstrip("\n").split("\t")
            if len(p) < 12 or not p[0].startswith("0x"):
                continue
            new[int(p[0], 16)] = dict(zip(["addr", "name", "size", "size_fde", "module", "attributed",
                                           "cls", "sub", "layer", "callers", "slots", "evid"], p))
    cov = {int(r["addr"], 16): r for r in read_cov()}
    out = []
    kinds = collections.Counter()

    def add(kind, a, cause, ev):
        kinds[kind] += 1
        o, n = old.get(a), new.get(a)
        out.append(dict(kind=kind, addr="%#x" % a,
                        old_name=(o or {}).get("name") or "—",
                        name=(n or {}).get("name") or (o or {}).get("name") or "—",
                        module=(n or {}).get("module") or "—",
                        old_cls=(o or {}).get("cls") or "(不在旧快照)",
                        old_sub=(o or {}).get("sub") or "—",
                        old_layer=(o or {}).get("layer") or "—",
                        new_cls=(n or {}).get("cls") or "(不在新表)",
                        new_sub=(n or {}).get("sub") or "—",
                        new_layer=(n or {}).get("layer") or "—",
                        cause=cause, evidence=ev))

    for a in sorted(set(old) | set(new)):
        o, n = old.get(a), new.get(a)
        if a in T93_MANUAL_NA:
            sub, note = T93_MANUAL_NA[a]
            add("manual-t93", a,
                "T93 人工裁决：%s（源码层没有独立 fn）" % note,
                "docs/06 §139.2–§139.4 + fixtures/gap_modcontainer.tsv（%s）" % sub)
            continue
        if o is None or n is None:
            add("row-set", a, "13,263 funcstart 集合变了（不该发生）", "—")
            continue
        cls_changed = o["cls"] != n["cls"]
        sub_changed = (o["cls"], o["sub"]) != (n["cls"], n["sub"])
        name_changed = o["name"] != n["name"]
        if not cls_changed and not sub_changed and not name_changed:
            continue
        row = cov.get(a)
        bucket = row["bucket"] if row else ""
        if not cls_changed and sub_changed and o["cls"] == CLS_IMPL == n["cls"]:
            # 类没变、只把 F2（有实现无物证）升成 F1（有物证）：T87/T89/T94 定名/落码带来的物证升档
            add("sub-refresh", a,
                "527 台账物证升档 %s→%s（类不变，仍是 implemented）" % (o["sub"], n["sub"]),
                "fixtures/function_coverage.tsv 今天 bucket=%s" % bucket)
            continue
        if cls_changed:
            if n["layer"] in ("L1-location", "L1-t82"):
                if o["cls"] == CLS_IMPL:
                    kind = "ledger-lost"
                    cause = ("527 台账 F 档下滑：旧快照 F1/F2 ⇒ 今天 F3（%s）⇒ 按 f3_triage 的 T82 裁决改判"
                             % bucket)
                    ev = "fixtures/function_coverage.tsv 今天 bucket=%s；fixtures/f3_triage.tsv 标签=%s" % (
                        bucket, F3_LABEL.get(a, "—"))
                elif n["cls"] == CLS_IMPL:
                    kind = "ledger-gain"
                    cause = "527 台账 F 档上抬：旧快照 F3/T82 ⇒ 今天 F%s（被落码/定名）" % bucket[1:]
                    ev = "fixtures/function_coverage.tsv 今天 bucket=%s" % bucket
                else:
                    kind = "t82-label"
                    cause = "f3_triage 标签被人工裁决改过（T89 §138.5）"
                    ev = "fixtures/f3_triage.tsv 今天标签=%s（旧快照标签见 old_sub）" % F3_LABEL.get(a, "—")
                add(kind, a, cause, ev)
            else:
                add("cascade", a, "L4/L5 级联（种子族变了：34 行换成 third/gdext/std、15 行换成 game）",
                    "逐行 region/调用图归因见 docs/06 §142.4.1（%s/%s → %s/%s）"
                    % (o["cls"], o["layer"], n["cls"], n["layer"]))
        else:
            add("name-refresh", a, "T87/T89 定名结果进了 527 台账的 name 列（类未变）",
                "fixtures/function_coverage.tsv name 列")
    cols = ["kind", "addr", "old_name", "name", "module", "old_cls", "old_sub", "old_layer",
            "new_cls", "new_sub", "new_layer", "cause", "evidence"]
    path = os.path.join(ROOT, "fixtures", "global_triage_refresh_diff.tsv")
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("# global_triage_refresh_diff.tsv —— T96：T86 旧快照 → 今天 的**逐行改判台账**（由 tools/global_triage.py --write-diff 生成，不要手改）\n")
        fh.write("# 旧快照 = HEAD 版 fixtures/global_triage.tsv（T86 d877738 产出）；新表 = 同目录 global_triage.tsv\n")
        fh.write("# kind：ledger-lost=527 台账 F 档下滑导致 implemented 掉档 / ledger-gain=台账上抬后改判 implemented /\n")
        fh.write("#       sub-refresh=类不变、F2→F1 物证升档 / t82-label=f3_triage 标签被人工裁决改过 /\n")
        fh.write("#       manual-t93=T93 人工裁决 49 行 / cascade=L4-L5 级联 / name-refresh=只换名字（名字变化在所有行都可比 old_name 列）\n")
        fh.write("# 计数：" + " / ".join("%s %d" % (k, v) for k, v in kinds.most_common()) + "\n")
        fh.write("# 判据与期望值见 fixtures/global_triage_refresh_plan.txt（预注册）；本轮逐行说明见 docs/06 §142\n")
        fh.write("\t".join(cols) + "\n")
        for r in sorted(out, key=lambda x: (x["kind"], int(x["addr"], 16))):
            fh.write("\t".join(str(r[c]) for c in cols) + "\n")
    print("\n写出 %s（%d 行）" % (path, len(out)))
    for k, v in kinds.most_common():
        print("  %-14s %5d" % (k, v))
    return 0


# ---------------------------------------------------------------------------
def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--build-index", action="store_true")
    p.add_argument("--control", action="store_true")
    p.add_argument("--stats", action="store_true")
    p.add_argument("--dump", nargs="+", metavar="VADDR")
    p.add_argument("--sample", action="store_true")
    p.add_argument("--android-census", action="store_true")
    p.add_argument("--denom-outside", dest="denom_outside", action="store_true",
                   help="T105：量化 527 分母**之外**的真实源码函数 → fixtures/denom_outside.tsv"
                        "（预注册 fixtures/denom_outside_plan.txt；C0–C5 全过才写）")
    p.add_argument("--control-only", dest="control_only", action="store_true",
                   help="--denom-outside 只跑 C0–C5 正/负对照，不写文件")
    p.add_argument("--no-noloc", dest="no_noloc", action="store_true",
                   help="旧行为：--android-census 不写 lin_fns_noloc 列（输出与 T105 之前逐字节相同）")
    p.add_argument("--seed", type=int, default=20261127)
    p.add_argument("--no-write", action="store_true")
    p.add_argument("--no-manual", action="store_true",
                   help="不应用 T93 人工裁决表（复现\"未人工改判\"的基线，自校验 S3 用）")
    p.add_argument("--out", metavar="PATH", help="TSV 写到别处（默认 fixtures/global_triage.tsv）")
    p.add_argument("--self-check", action="store_true",
                   help="自校验 S1/S2/S3（只读，不写 fixtures）；可配 --baseline <--no-manual 的输出>")
    p.add_argument("--baseline", metavar="PATH", help="--self-check 的基线（--no-manual --out 的产物）")
    p.add_argument("--write-diff", metavar="OLD_TSV",
                   help="写 fixtures/global_triage_refresh_diff.tsv（逐行改判台账），OLD_TSV = 旧快照")
    a = p.parse_args(argv)
    if a.build_index:
        build_index()
        return 0
    if a.control:
        return cmd_control(a)
    if a.dump:
        cmd_dump(a.dump)
        return 0
    if a.sample:
        cmd_sample(a)
        return 0
    if a.denom_outside:
        return cmd_denom_outside(a)
    if a.android_census:
        load_f3_label()
        return cmd_android_census(a)
    if a.self_check:
        load_f3_label()
        return cmd_self_check(a)
    if a.write_diff:
        return cmd_write_diff(a.write_diff)
    ctx, starts, data, rows = build_all(verbose=True, use_manual=not a.no_manual)
    stats(rows)
    if not a.no_write and not a.stats:
        path = write_tsv(rows, a.out or OUT)
        print("\n写出 %s（%d 行；人工裁决表 %s）"
              % (path, len(rows), "关(--no-manual)" if a.no_manual else "开"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
