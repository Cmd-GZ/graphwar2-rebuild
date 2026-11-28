#!/usr/bin/env python3
"""T107 / fixtures/denom_serde_evidence.tsv 生成器（纯 stdlib + capstone）。

用法（$WT = 工作树根）：
    $WT/.reference/venv/bin/python fixtures/denom_serde_collect.py \
        --so $WT/.reference/steam/android-2.0.60/libgraphwarrust.so \
        --ledger fixtures/denom_outside.tsv
输出：fixtures/denom_serde_evidence.tsv（覆盖写）

判据（P1–P6 与判决规则）见 fixtures/denom_serde_plan.txt §1，本脚本只做机械判定，
不写任何语义解释。
"""
import argparse, collections, re, struct, sys

try:
    from capstone import Cs, CS_ARCH_ARM64, CS_MODE_ARM
    from capstone.arm64 import ARM64_OP_REG, ARM64_OP_IMM
except ImportError:
    raise SystemExit("需要 capstone（用 .reference/venv/bin/python 跑）")

EXT = ("serde", "serde_json", "core", "std", "alloc", "chrono", "hashbrown",
       "memchr", "itoa", "ryu", "gimli", "addr2line", "__rustc", "_Unwind")

# (android vaddr, module, owner type, method)
TARGETS = [
    (0x4691a4, "graphwar_event", "GraphwarEvent", "deserialize"),
    (0x4693c0, "graphwar_event", "GraphwarEvent", "make_new_connection_request_event"),
    (0x46947c, "graphwar_event", "GraphwarEvent", "serialize"),
    (0x4695ac, "graphwar_event", "GraphwarEvent", "serialize"),
    (0x469c54, "graphwar_event", "GraphwarEvent", "deserialize"),
    (0x40b834, "graphwar_effect_manager", "EffectType", "deserialize"),
    (0x46b210, "graphwar_game_info", "GameInfo", "serialize"),
    (0x4981c8, "graphwar_mode_manager", "FunctionMode", "deserialize"),
    (0x49849c, "graphwar_mode_manager", "TimeMode", "serialize"),
    (0x498574, "graphwar_mode_manager", "AxisMode", "deserialize"),
    (0x498860, "graphwar_mode_manager", "TimeMode", "deserialize"),
    (0x4999a0, "graphwar_team_manager", "GraphwarTeam", "deserialize"),
    (0x49be14, "graphwar_turn_manager", "TurnMode", "deserialize"),
    (0x49c0e8, "graphwar_color_manager", "GraphwarColor", "serialize"),
    (0x4a52f4, "graphwar_game_state_manager", "GraphwarGameState", "serialize"),
    (0x4a53e8, "graphwar_game_state_manager", "GraphwarGameState", "deserialize"),
]

_ESC = dict([("$LT$", "<"), ("$GT$", ">"), ("$u20$", " "), ("$u27$", "'"),
             ("$u5b$", "["), ("$u5d$", "]"), ("$u7b$", "{"), ("$u7d$", "}"),
             ("$u28$", "("), ("$u29$", ")"), ("$u21$", "!"), ("$u26$", "&"),
             ("$u2b$", "+"), ("$u3a$", ":"), ("$u2c$", ","), ("$RP$", "::"),
             ("$BP$", "*"), ("$C$", ","), ("$u5f$", "_"), ("$u7e$", "~"),
             ("$u22$", chr(34)), ("$u3d$", "="), ("$u2f$", "/"), ("$u2d$", "-"),
             ("$u3c$", "<"), ("$u3e$", ">"), ("$u2e$", ".")])


class Elf:
    def __init__(self, path):
        self.data = open(path, "rb").read()
        d = self.data
        if d[:4] != b"\x7fELF" or d[4] != 2 or d[5] != 1 or struct.unpack_from("<H", d, 0x12)[0] != 183:
            raise SystemExit("%s: 不是 ELF64 LE AArch64" % path)
        (shoff,) = struct.unpack_from("<Q", d, 0x28)
        shentsize, shnum, shstrndx = struct.unpack_from("<HHH", d, 0x3A)
        hdrs = [struct.unpack_from("<IIQQQQI", d, shoff + i * shentsize) for i in range(shnum)]
        shstr = hdrs[shstrndx][4]
        self.secs = {}
        for h in hdrs:
            nm, typ, flags, addr, off, size, link = h
            e = d.index(b"\0", shstr + nm)
            self.secs[d[shstr + nm:e].decode()] = dict(addr=addr, off=off, size=size)

    def read(self, va, n):
        for s in self.secs.values():
            if s["addr"] and s["addr"] <= va < s["addr"] + s["size"]:
                o = s["off"] + (va - s["addr"])
                return self.data[o:o + n]
        return None

    def cstr(self, va, maxn=96):
        b = self.read(va, maxn)
        if b is None:
            return None
        e = b.find(b"\0")
        return None if e < 0 else b[:e].decode("utf-8", "replace")

    def symbols(self):
        sym, strtab = self.secs[".symtab"], self.secs[".strtab"]
        out = []
        for i in range(sym["size"] // 24):
            o = sym["off"] + i * 24
            st_name, _i, _o, _sh, value, size = struct.unpack_from("<IBBHQQ", self.data, o)
            if not st_name:
                continue
            b = strtab["off"] + st_name
            e = self.data.index(b"\0", b)
            out.append((self.data[b:e].decode("utf-8", "replace"), value, size))
        return out


def pretty(n):
    if n.startswith("_ZN"):
        n = n[3:]
        segs, i = [], 0
        while i < len(n) and n[i].isdigit():
            j = i
            while j < len(n) and n[j].isdigit():
                j += 1
            ln = int(n[i:j])
            segs.append(n[j:j + ln])
            i = j + ln
        n = "::".join(segs)
    for k, v in _ESC.items():
        n = n.replace(k, v)
    return n


def crate_root(n):
    """把符号名归一成"最外层 crate 名"。处理 `<impl serde..X for Y>`、`<$RF$mut serde_json..Z>`、
    `_<core..marker..PhantomData<T> as serde..de..DeserializeSeed>`、v0 `_RNv...` 这几种形态。"""
    m = n.lstrip("_")
    if m.startswith("<"):
        m = m[1:]
        for pre in ("$RF$mut ", "$RF$", "&mut ", "&"):
            if m.startswith(pre):
                m = m[len(pre):]
    return re.split(r"\.\.|::|>|,", m)[0]


def external(n):
    r = crate_root(n)
    if n.startswith("_Unwind") or n.startswith("__rust_") or r in ("Unwind_Resume", "rust_alloc",
                                                                  "rust_dealloc", "rust_realloc"):
        return True
    return r in EXT or r == "" or n.startswith("_R") or r.startswith("$")


def is_serde_impl(n):
    """crate 内**另一个 serde 实现**（derive 族内部调用）—— 不是游戏逻辑。"""
    return "_<impl serde.." in n or "<impl serde.." in n


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--so", required=True)
    ap.add_argument("--ledger", default="fixtures/denom_outside.tsv")
    ap.add_argument("--out", default="fixtures/denom_serde_evidence.tsv")
    a = ap.parse_args()

    elf = Elf(a.so)
    text = elf.secs[".text"]
    tlo, thi = text["addr"], text["addr"] + text["size"]
    addr_syms = collections.defaultdict(list)
    for nm, va, sz in elf.symbols():
        if tlo <= va < thi:
            addr_syms[va].append((pretty(nm), sz))

    def size_of(va):
        v = [s for n, s in addr_syms.get(va, []) if s > 0]
        return max(v) if v else 0

    def names_of(va):
        return sorted({n for n, s in addr_syms.get(va, []) if not n.startswith("$")})

    md = Cs(CS_ARCH_ARM64, CS_MODE_ARM)
    md.detail = True

    def analyze(va):
        sz = size_of(va)
        code = elf.read(va, sz)
        bls, strs, adrp = [], [], {}
        for ins in md.disasm(code, va):
            m = ins.mnemonic
            if m == "adrp":
                adrp[ins.operands[0].reg] = ins.operands[1].imm
            elif m == "add" and len(ins.operands) == 3 and ins.operands[1].type == ARM64_OP_REG \
                    and ins.operands[2].type == ARM64_OP_IMM:
                r = ins.operands[1].reg
                if r in adrp:
                    s = elf.cstr(adrp[r] + ins.operands[2].imm)
                    if s:
                        strs.append(s)
                adrp.pop(ins.operands[0].reg, None)
            elif m == "adr":
                s = elf.cstr(ins.operands[1].imm)
                if s:
                    strs.append(s)
            elif m == "bl":
                bls.append(ins.operands[0].imm)
        calls = []
        for t in bls:
            ns = names_of(t)
            calls.append((t, ns[0] if ns else "?"))
        return calls, strs

    # 台账：拿 T105 对同族兄弟符号的判决（C2 用）
    ledger = {}
    for line in open(a.ledger, encoding="utf-8"):
        if line.startswith("#"):
            continue
        p = line.rstrip("\n").split("\t")
        if len(p) > 16 and p[0].startswith("0x"):
            ledger[int(p[0], 16)] = p

    rows = []
    for va, mod, ty, meth in TARGETS:
        sz = size_of(va)
        sym = (names_of(va) or ["?"])[0]
        kind = "trait-impl" if re.search(r"_::_<impl serde\.\.", sym) else "inherent"
        tm = re.search(r"<impl serde\.\.(ser|de)\.\.(Serialize|Deserialize)", sym)
        trait = "serde::%s::%s" % (tm.group(1), tm.group(2)) if tm else "-"
        # P2：同 (module, Type) 的 derive 私有辅助类型
        pat = re.compile(r"graphwarrust\.\.%s\.\.[^>]*?%s>" % (re.escape(mod), re.escape(ty)))
        fam = []
        for v2, nm2 in addr_syms.items():
            for n2, s2 in nm2:
                if s2 <= 0 or not pat.search(n2):
                    continue
                if "__Visitor" in n2 or "__FieldVisitor" in n2 or re.search(r"__Field>", n2):
                    fam.append((v2, s2, n2))
        famtxt = "hit" if fam else "miss"
        famdet = ";".join("0x%x:%s" % (v2, ("V" if "__Visitor" in n2 and "__FieldVisitor" not in n2
                                           else ("FV" if "__FieldVisitor" in n2 else "F")))
                          for v2, s2, n2 in sorted(fam))
        # P4
        calls, strs = analyze(va)
        own = [c for c in calls if not external(c[1])]                  # P4 原口径（crate 自有调用）
        own_game = [c for c in own if not is_serde_impl(c[1])]           # §8-E1 细化：扣掉 crate 内其它 serde impl
        # P1..P4 → verdict（§1.6）
        if kind == "inherent":
            verdict, crit = "real-missing(手写)", "P1(固有方法)"
            tgt = [c for c in own]
            if tgt:
                crit += "+P3:bl->" + ",".join("0x%x" % c[0] for c in tgt)
        else:
            if fam and not own_game:
                verdict, crit = "na:other/serde-derive", "P1(trait-impl)+P2(家族命中)+P4(own_game=0)"
            elif fam and own_game:
                verdict, crit = "unknown", "P1+P2 但 P4 own_game>0"
            else:
                verdict, crit = "unknown", "P1 但 P2 miss"
        famled = []
        for v2, s2, n2 in sorted(fam):
            r = ledger.get(v2)
            if r:
                famled.append("%s=%s/%s/%s" % ("0x%x" % v2, r[13], r[14], r[6]))
        rows.append(dict(va=va, sz=sz, mod=mod, ty=ty, meth=meth, kind=kind, trait=trait,
                         fam=famtxt, famdet=famdet, famled=";".join(famled) or "-",
                         nbl=len(calls), nown=len(own), nownG=len(own_game),
                         ownl=",".join("0x%x" % c[0] for c in own) or "-",
                         owngamel=",".join("0x%x" % c[0] for c in own_game) or "-",
                         strs="|".join("".join(c if 32 <= ord(c) < 127 else "." for c in t)[:40]
                                       for t in strs[:4] if any(32 <= ord(c) < 127 for c in t)) or "-",
                         verdict=verdict, crit=crit, sym=sym))

    # ---------------- 对照 ----------------
    # C1 / C1b
    ownrows = [r for r in rows if r["kind"] == "inherent"]
    c1 = all(r["verdict"].startswith("real-missing") for r in ownrows)
    g3 = [r for r in rows if r["va"] == 0x46947c][0]
    c1b = ownrows and "0x4695ac" in g3["ownl"]
    # C2：13 条 trait impl 的兄弟符号在 T105 台账里已是 macro-derive / volume-gate
    n_c2_ok = 0
    for r in rows:
        if r["kind"] != "trait-impl":
            continue
        r2 = ledger.get(r["va"])
        for v2, s2, n2 in [(0, 0, "")]:
            pass
    tri = [r for r in rows if r["kind"] == "trait-impl"]
    for r in tri:
        if r["famled"] == "-":
            continue
        good = all(("na:other/macro-derive" in t) or ("unknown/volume-gate" in t and "macro-derive" in t)
                   for t in r["famled"].split(";"))
        if good:
            n_c2_ok += 1
    # C3：若规则写成 P2 or P4 ⇒ derive，会误判几条
    c3 = [r for r in rows if (r["fam"] == "hit" or r["nown"] == 0) and r["kind"] == "inherent"]
    # C4：全 .text 里 serde trait impl 的条数
    allimpl = set()
    allimpl_any = set()
    for va, ns in addr_syms.items():
        for n, s in ns:
            if s > 0 and re.search(r"_<impl serde\.\.(ser|de)\.\.(Serialize|Deserialize) for ", n):
                allimpl_any.add(va)
                if n.startswith("graphwarrust::"):
                    allimpl.add(va)
    # C5a / C5b
    c5a = sum(1 for r in rows if (r["fam"] == "hit" and r["nownG"] == 0))
    c5b = sum(1 for r in tri if r["fam"] == "miss")
    # C5b' 有效替代对照：手写 trait impl（bytes crate 的 Debug 2 条）+ 20 个非 serde 的 crate 类型
    neg_impl = [va for va, ns in addr_syms.items() for n, s in ns
                if s > 0 and re.search(r"<impl core\.\.fmt\.\.Debug for ", n)]
    neg_fam = 0
    for va in neg_impl:
        for n, s in addr_syms[va]:
            if "Debug for " not in n or s <= 0:
                continue
            ty = n.split("Debug for ", 1)[1].split(">")[0].split("..")[-1]
            if not any(("__Visitor" in n2 or "__FieldVisitor" in n2) and ty in n2 and s2 > 0
                       for ns2 in addr_syms.values() for n2, s2 in ns2):
                neg_fam += 1
    # 20 个没有 serde 实现的 crate 类型（取 .symtab 里 `graphwarrust..mod..Type::` 形态的前 20 个非 serde 类型）
    cand = set()
    for va, ns in addr_syms.items():
        for n, s in ns:
            if s <= 0 or not n.startswith("graphwarrust::"):
                continue
            m = re.match(r"graphwarrust::[a-z_0-9]+::([A-Z][A-Za-z0-9_]+)::", n)
            if m:
                cand.add(m.group(1))
    cand = sorted(cand)[:20]
    neg20 = 0
    for ty in cand:
        if not any(("__Visitor" in n2 or "__FieldVisitor" in n2) and ty in n2 and s2 > 0
                   for ns2 in addr_syms.values() for n2, s2 in ns2):
            neg20 += 1

    out = []
    out.append("# denom_serde_evidence.tsv —— T107：分母外 serde 类 **16 条机器码 / 14 个源码组**的逐条证据与裁决")
    out.append("# 判据：[fixtures/denom_serde_plan.txt] §1（P1–P6 + §1.6 判决规则，**预注册、跑完不改**）")
    out.append("# 生成：$WT/.reference/venv/bin/python fixtures/denom_serde_collect.py \\")
    out.append("#         --so .reference/steam/android-2.0.60/libgraphwarrust.so --ledger fixtures/denom_outside.tsv")
    out.append("# 地址口径：**Android 2.0.60 arm64 ELF vaddr**（未 strip，sha256 8a612e95…54df）；与 Linux x86-64 侧不可换算")
    out.append("# 列：android_addr | st_size | module | owner_type | method | kind | trait")
    out.append("#     | P2_family | P2_family_detail | P2_family_in_T105_ledger | n_bl | n_bl_crate_own | n_bl_own_game")
    out.append("#     | own_game_bl_list | own_crate_bl_list")
    out.append("#     | rodata_immediates | verdict | criterion")
    out.append("#  kind = inherent（`module::Type::method`，derive 永不生成） / trait-impl（`module::_::_<impl serde::…for Type>::…`）")

    out.append("# --- 对照（plan §3；全过才允许出裁决） ---")
    out.append("# C1  正对照（手写侧）G1/G2/G3 = 固有方法 ⇒ 判 手写：%s" % ("✓ %d/%d" % (sum(1 for r in ownrows if r['verdict'].startswith('real-missing')), len(ownrows)) if c1 else "✗"))
    out.append("# C1b P3 调用配位：0x46947c 体内 bl 到 0x4695ac（同 Type 的 trait impl）：%s" % ("✓" if c1b else "✗"))
    out.append("# C2  同族一致性：13 条 trait impl 的 __Visitor/__FieldVisitor 兄弟符号在 T105 台账里已是 na:other/macro-derive（或 form=macro-derive 的 volume-gate）：%d/13" % n_c2_ok)
    out.append("# C3  自我证伪点：规则若写成『P2 or P4 ⇒ derive』会误判的条数 = **%d**（固有方法里 P2 或 own=0 的）⇒ P1 不可省" % len(c3))
    out.append("# C4  全量无遗漏：.text 里 **game crate** 的 `<impl serde::{ser::Serialize|de::Deserialize} for …>` 去重 = **%d** 条"
               "（全二进制含 serde 自己的 Option<T> 实现共 %d 条），与本轮 13 条 trait impl %s"
               % (len(allimpl), len(allimpl_any),
                  "逐条相同 ✓" if allimpl == set(r["va"] for r in tri) else "不同 ✗"))
    out.append("# C5a 去掉 P1 重跑 ⇒ 误判成 derive = **%d** 条（>0 才证明 P1 必要）" % c5a)
    out.append("# C5b 去掉 P2 重跑 ⇒ 13 条里判 unknown = **%d** 条 ⇒ **vacuous（13/13 全命中家族，样本里没有负例）**，"
               "见 plan §8-E2 的替代对照 C5b′" % c5b)
    out.append("# C5b′ 有效替代对照（P2 的**特异性**）："
               "⑴ 手写 trait impl（`<impl core::fmt::Debug for bytes::…>` 2 条）家族命中 = %d/2（期望 0）；"
               "⑵ 20 个非 serde 的 crate 类型家族命中 = %d/20（期望 0，即 20/20 miss）"
               % (len(neg_impl) - neg_fam if neg_impl else 0, 20 - neg20))
    out.append("# --- 计数 ---")
    out.append("# verdict: real-missing(手写) %d 条 / na:other/serde-derive %d 条 / unknown %d 条（共 %d 条 = 14 组）"
               % (sum(1 for r in rows if r['verdict'].startswith('real-missing')),
                  sum(1 for r in rows if r['verdict'].startswith('na:')),
                  sum(1 for r in rows if r['verdict'] == 'unknown'), len(rows)))
    for r in rows:
        out.append("\t".join([
            "0x%x" % r["va"], str(r["sz"]), r["mod"], r["ty"], r["meth"], r["kind"], r["trait"],
            r["fam"], r["famdet"], r["famled"], str(r["nbl"]), str(r["nown"]),
            str(r["nownG"]), r["owngamel"], r["ownl"],
            r["strs"], r["verdict"], r["crit"],
        ]))
    open(a.out, "w", encoding="utf-8").write("\n".join(out) + "\n")
    print("wrote %s (%d data rows)" % (a.out, len(rows)))
    for o in out[:40]:
        print(o)


if __name__ == "__main__":
    main()
