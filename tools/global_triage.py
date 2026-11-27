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
    $PY tools/global_triage.py --control           # 正/负对照（T82 的 153 条 + T71 的 31 条）
    $PY tools/global_triage.py                     # 全量：写 fixtures/global_triage.tsv + 打印统计
    $PY tools/global_triage.py --dump 0x4600e0 ... # 单条取证（人工复核用）
    $PY tools/global_triage.py --sample 20         # 预注册抽样（人工复核用）

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


def build_all(verbose=True):
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

    fails = 0
    print("\n=== A：T71 台账 12 条 std 单态化（期望 na:std-mono）===")
    print("     ⚠ 这些地址本身在 527 台账里 ⇒ 第 1 列是「抄台账」的**回归闸**，"
          "第 2 列是**不看台账**的独立判（只靠 L1–L3 证据）")
    for a in CTRL_STD:
        s = int(a, 16)
        r, r2 = judge(s), judge(s, False)
        ok = r["cls"] == CLS_STD
        fails += 0 if ok else 1
        print("  %s %-10s 台账=%-13s 独立=%-13s %s"
              % ("✓" if ok else "✗", a, r["cls"], r2["cls"],
                 (r2 if r2["cls"] == CLS_STD else r)["evid"][:74]))
    print("\n=== B：T71 台账 19 条 gdext（16 期望 na:gdext-glue / 3 源码闭包期望 unknown）===")
    for a in CTRL_GDEXT:
        s = int(a, 16)
        r, r2 = judge(s), judge(s, False)
        want = CLS_UNK if a in CTRL_GDEXT_CLOSURE else CLS_GDX
        ok = r["cls"] == want
        fails += 0 if ok else 1
        print("  %s %-10s want=%-13s 台账=%-13s 独立=%-13s %s"
              % ("✓" if ok else "✗", a, want, r["cls"], r2["cls"],
                 (r2 if r2["cls"] == want else r)["evid"][:62]))
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
    print("\n=== D：T82 的 153 条 F3（回归闸：不许把 F3 判坏）===")
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
    print("  T82 153 条：一致 %d ／ 台账已变档（现为 F1/F2，**不是本工具判错**，是 T82 之后"
          "有人把这些函数落码了）%d ／ 真不一致 %d" % (agree, drift, n - agree - drift))
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
        fh.write("# lin_F3 = **今天仍是** F3 的条数（T82 判过的 153 条里，35 条已被后续轮次落码）；lin_F3_real = 其中 T82 判 real-missing\n")
        fh.write("# gap_proxy = android_fns − lin_F1F2（**代理估计**：Android 比 Linux 少 654 条函数，平台层差异未扣）\n")
        fh.write("module\tandroid_fns\tlin527\tlin_F1F2\tlin_F3\tlin_F3_real\tgap_proxy\n")
        rows = []
        for m in allmods:
            key = m[:-3] if m.endswith(".rs") else m
            rows.append((m, mods.get(key, 0), lin.get(m, 0), lin_f12.get(m, 0),
                         f3.get(m, 0), f3real.get(m, 0),
                         mods.get(key, 0) - lin_f12.get(m, 0)))
        rows.sort(key=lambda r: (-r[1], -r[2]))
        for r in rows:
            fh.write("\t".join(str(x) for x in r) + "\n")
    print("写出 %s（%d 个模块）" % (MODULES_TSV, len(rows)))
    return 0


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
def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--build-index", action="store_true")
    p.add_argument("--control", action="store_true")
    p.add_argument("--stats", action="store_true")
    p.add_argument("--dump", nargs="+", metavar="VADDR")
    p.add_argument("--sample", action="store_true")
    p.add_argument("--android-census", action="store_true")
    p.add_argument("--seed", type=int, default=20261127)
    p.add_argument("--no-write", action="store_true")
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
    if a.android_census:
        load_f3_label()
        return cmd_android_census(a)
    ctx, starts, data, rows = build_all(verbose=True)
    stats(rows)
    if not a.no_write and not a.stats:
        path = write_tsv(rows)
        print("\n写出 %s（%d 行）" % (path, len(rows)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
