#!/usr/bin/env python3
"""arm64 未 strip 版的**符号表**：按名字查地址、按地址查函数、扫「谁把某个 64 位常量写进内存」。

## 为什么需要它

x86-64 目标 `2026-08-28/libgraphwarrust.so` 是 **stripped** 的，函数→模块只能靠
`panic::Location`（`tools/addr_probe.py locs`）投票；而 `panic::Location` **只覆盖会 panic 的
函数**（`unwrap`/越界/`expect`）。一个不 panic 的普通构造函数（例如某个事件变体的生产点）
在 x86 侧可能一条 `Location` 都没有 ⇒ 那一路查不到模块归属。

Android 版 **没 strip**，`.symtab` 里有完整 Rust legacy mangled 名，里面直接写着
`graphwarrust::graphwar_<module>::<Type>::<fn>`。本工具把这个符号表变成两件东西：

| 命令 | 回答 |
|---|---|
| `find RE`   | 名字匹配正则的符号（地址 + 字节数），默认按地址排序 |
| `at VA`     | 这个地址落在哪个符号里（含"落在函数中部"的偏移） |
| `mod NAME`  | 某个模块名（`graphwar_ef<mod>.rs` 的前缀）下的全部函数 |
| `const HEX` | 全 `.text` 扫「把 64 位常量 materialize 出来」的窗口：`movz/movk` 链 + 字面量池 |
| `dis VA`    | 反汇编一小段（只用 stdlib 的 `objdump`，不做完整解码） |

## 口径与陷阱

* 一律 **arm64 虚地址**（就是 `.symtab` 的 `st_value`）。**不要**拿去和 x86 地址互相换算 ——
  两份 `.so` 的布局完全不同；跨架构只用**结构**对应（调用关系 / 常量 / 串）。
* 一份"未 strip"的 Rust 库里有**同名同址的多个符号**（`.llvm.` 后缀的克隆、`$u7b$` 闭包），
  所以 `find` 会打印同一个地址多行；`at` 用**最小包含区间**挑最内层。
* `const` 命令的**正对照**：`--const 800000000000002a` 必须命中
  `GraphwarObstacleManager::queue_destroy_obstacle`（tag 42，见 `docs/14` §12.4）。
  扫不到就说明扫法坏了，不是"没有生产者"。

用法：
    ../.venv/bin/python tools/arm64_sym.py find 'queue_destroy'
    ../.venv/bin/python tools/arm64_sym.py at 0x5c1234
    ../.venv/bin/python tools/arm64_sym.py mod graphwar_effect_manager
    ../.venv/bin/python tools/arm64_sym.py const 800000000000002c
    ../.venv/bin/python tools/arm64_sym.py dis 0x5c1234 --n 40
"""

import argparse
import os
import re
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

CANDIDATES = (
    ".reference/steam/android-2.0.60/libgraphwarrust.so",
    ".rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so",
)


def find_so():
    env = os.environ.get("GW2_ARM64_SO")
    if env:
        return env
    base = ROOT
    for _ in range(6):  # 克隆比仓库根深几层时逐级向上找
        for c in CANDIDATES:
            p = os.path.join(base, c)
            if os.path.exists(p):
                return p
        base = os.path.dirname(base)
    raise SystemExit("找不到 arm64 未 strip 版；用 --so 指定或设 GW2_ARM64_SO")


class Elf:
    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            self.data = f.read()
        d = self.data
        if d[:4] != b"\x7fELF" or d[4] != 2 or d[5] != 1:
            raise SystemExit("%s: 不是 ELF64 little-endian" % path)
        if struct.unpack_from("<H", d, 0x12)[0] != 183:
            raise SystemExit("%s: 不是 AArch64（e_machine != 183）" % path)
        (shoff,) = struct.unpack_from("<Q", d, 0x28)
        shentsize, shnum, shstrndx = struct.unpack_from("<HHH", d, 0x3A)
        hdrs = []
        for i in range(shnum):
            o = shoff + i * shentsize
            name, typ, flags, addr, off, size, link = struct.unpack_from("<IIQQQQI", d, o)
            hdrs.append(dict(name=name, type=typ, flags=flags, addr=addr,
                             off=off, size=size, link=link))
        shstr = hdrs[shstrndx]["off"]
        self.secs = {}
        for h in hdrs:
            end = d.index(b"\0", shstr + h["name"])
            self.secs[d[shstr + h["name"]:end].decode()] = h
        self._syms = None

    def read(self, va, n):
        for s in self.secs.values():
            if s["addr"] and s["addr"] <= va < s["addr"] + s["size"]:
                o = s["off"] + (va - s["addr"])
                if o + n <= len(self.data):
                    return self.data[o:o + n]
        return None

    def text(self):
        s = self.secs[".text"]
        return s["addr"], s["off"], s["size"]

    def symbols(self):
        """[(name, value, size)]，含大小 0 的（`at` 会用相邻起点补区间）。"""
        if self._syms is not None:
            return self._syms
        sym, strtab = self.secs[".symtab"], self.secs[".strtab"]
        out = []
        for i in range(sym["size"] // 24):
            o = sym["off"] + i * 24
            st_name, _info, _other, _shndx, value, size = struct.unpack_from("<IBBHQQ", self.data, o)
            if not st_name:
                continue
            base = strtab["off"] + st_name
            end = self.data.index(b"\0", base)
            out.append((self.data[base:end].decode("utf-8", "replace"), value, size))
        self._syms = out
        return out


# ---------------------------------------------------------------- 名字美化
_ESC = {"$LT$": "<", "$GT$": ">", "$u20$": " ", "$u27$": "'", "$u5b$": "[", "$u5d$": "]",
        "$u7b$": "{", "$u7d$": "}", "$u28$": "(", "$u29$": ")", "$u21$": "!", "$u26$": "&",
        "$u2b$": "+", "$u3a$": ":", "$u2c$": ",", "$RP$": "::", "$BP$": "*", "$C$": ","}


def pretty(name):
    """legacy `_ZN...E` 的最简还原：长度前缀切段 + `$xx$` 转义，够读即可。"""
    n = name
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
        if segs and segs[-1] == "":
            segs.pop()
        n = "::".join(s for s in segs if s)
    for k, v in _ESC.items():
        n = n.replace(k, v)
    n = re.sub(r"17h[0-9a-f]{16}E$", "", n)
    n = re.sub(r"\.llvm\.\d+$", "", n)
    n = re.sub(r"([A-Za-z_]\w*)1[0-9a-f]{15}$", r"\1", n)  # 极老格式的 hash 尾巴
    return n


def short_mod(pname):
    """`graphwarrust::graphwar_obstacle_manager::X` → `graphwar_obstacle_manager.rs`。"""
    m = re.match(r"graphwarrust::(graphwar_[a-z0-9_]+|[a-z0-9_]+)::", pname)
    return (m.group(1) + ".rs") if m else ""


# ---------------------------------------------------------------- 常量扫描
def materialize_windows(ctx, val, span=0x40):
    """找「把 64 位常量写出来」的位置。

    两种形态：
      ① `movz`/`movk` 链：同一个寄存器上把各 16 位段拼起来。**必须逐位重建后等于目标值**
         才算命中（早期版本只比 `movz` 的 imm 与 `movk #0x8000,lsl#48`，会把
         `mov x, #0x8000000000000000` + 任意 `movk x,#0x8000,lsl#48` 误配成
         `0x8000000000000038` 这类无关常量 —— 已修）。
      ② 字面量池：8 字节小端等于 `val` 的数据（`.rodata` / `.text` / `.data.rel.ro`），
         报池地址；调用方再用 `dis` 找 `ldr Xt,[pc,#imm]`。
    返回 (movz_chain_hits, literal_pool_addrs)。
    """
    d = ctx.data
    want = struct.pack("<Q", val)
    pool = []
    for name in (".rodata", ".text", ".data.rel.ro"):
        s = ctx.secs.get(name)
        if not s or not s["size"]:
            continue
        blob = d[s["off"]:s["off"] + s["size"]]
        start = 0
        while True:
            k = blob.find(want, start)
            if k < 0:
                break
            pool.append((name, s["addr"] + k))
            start = k + 1
    s = ctx.secs[".text"]
    blob = d[s["off"]:s["off"] + s["size"]]
    n = len(blob) // 4
    words = struct.unpack_from("<%dI" % n, blob, 0)
    chains = []
    for i, w in enumerate(words):
        # MOVZ: 0xD2800000(64) / 0x52800000(32)；MOVN: 0x92800000 / 0x12800000
        if (w & 0x7F800000) == 0x52800000:
            rd, imm, shift = w & 0x1F, (w >> 5) & 0xFFFF, ((w >> 21) & 3) * 16
            cur = imm << shift
        elif (w & 0x7F800000) == 0x12800000:
            rd, imm, shift = w & 0x1F, (w >> 5) & 0xFFFF, ((w >> 21) & 3) * 16
            cur = (~(imm << shift)) & 0xFFFFFFFFFFFFFFFF
        else:
            continue
        if cur == val:
            chains.append(s["addr"] + 4 * i)
            continue
        for j in range(i + 1, min(i + span // 4, n)):
            w2 = words[j]
            if (w2 & 0x7F800000) not in (0x72800000, 0x32800000):
                continue
            if (w2 & 0x1F) != rd:
                continue
            m = ((w2 >> 5) & 0xFFFF) << (((w2 >> 21) & 3) * 16)
            cur = (cur & ~m) | m
            if cur == val:
                chains.append(s["addr"] + 4 * i)
                break
    return chains, pool


def cmd_find(ctx, pat, limit):
    rx = re.compile(pat, re.I)
    rows = []
    for name, value, size in ctx.symbols():
        p = pretty(name)
        if rx.search(p) or rx.search(name):
            rows.append((value, size, p))
    rows.sort()
    for value, size, p in rows[:limit]:
        print("%#010x  %6d  %-12s %s" % (value, size, short_mod(p), p))
    print("== %d hit(s) ==" % len(rows))
    return 0


def cmd_at(ctx, va):
    hits = []
    for name, value, size in ctx.symbols():
        if value == 0:
            continue
        if value <= va < value + max(size, 1):
            hits.append((value + max(size, 1) - va, size, value, name))
    if not hits:
        print("%#x: 不在任何符号的 [value, value+size) 内（size=0 的符号不算）" % va)
        return 1
    hits.sort()
    for _span, size, value, name in hits[:4]:
        print("+%#-6x of %#x (+%d)  %s" % (va - value, value, size, pretty(name)))
    return 0


def cmd_mod(ctx, name):
    rx = re.compile(r"graphwarrust::%s::" % re.escape(name))
    rows = sorted((v, s, pretty(n)) for n, v, s in ctx.symbols() if rx.search(pretty(n)))
    for value, size, p in rows:
        print("%#010x  %6d  %s" % (value, size, p))
    print("== %d hit(s) ==" % len(rows))
    return 0


def owner_of(ctx, va, cache={}):
    """落在 [value, value+size) 的**最内层**符号名（未 strip 的库同名同址很多，取最小包含区间）。"""
    if ctx.path not in cache:
        cache[ctx.path] = sorted((v, max(sz, 1), pretty(n)) for n, v, sz in ctx.symbols() if v)
    best = ("", 1 << 62)
    for v, sz, p in cache[ctx.path]:
        if v <= va < v + sz and sz < best[1]:
            best = (p, sz)
    return best[0]


def cmd_callers(ctx, va):
    """全 `.text` 扫 `bl`（掩码 0xFC000000 == 0x94000000），目标 == VA 的调用点。

    只认 **直接调用**；Rust 的函数指针派发（vtable / fn ptr）不在此列 ——
    与 `tools/callers.py` 同一条口径警告。
    """
    s = ctx.secs[".text"]
    blob = ctx.data[s["off"]:s["off"] + s["size"]]
    words = struct.unpack_from("<%dI" % (len(blob) // 4), blob, 0)
    hits = []
    for i, w in enumerate(words):
        if (w & 0xFC000000) != 0x94000000:
            continue
        imm = w & 0x3FFFFFF
        if imm & 0x2000000:
            imm -= 0x4000000
        if s["addr"] + 4 * i + imm * 4 == va:
            hits.append(s["addr"] + 4 * i)
    seen = {}
    for h in hits:
        seen.setdefault(owner_of(ctx, h) or "?", []).append(h)
    print("== bl 到 %#x 的调用点: %d ==" % (va, len(hits)))
    for o, hs in sorted(seen.items(), key=lambda kv: -len(kv[1])):
        print("  %-84s x%-2d  %s" % (o[:84], len(hs), " ".join("%#x" % x for x in hs[:6])))
    return 0


def cmd_bl(ctx, va, n):
    """一个函数里所有 `bl` 的目标 + 符号名（看它调了谁）。"""
    blob = ctx.read(va, n)
    if not blob:
        print("读取失败")
        return 1
    words = struct.unpack_from("<%dI" % (len(blob) // 4), blob, 0)
    for i, w in enumerate(words):
        if (w & 0xFC000000) != 0x94000000:
            continue
        imm = w & 0x3FFFFFF
        if imm & 0x2000000:
            imm -= 0x4000000
        t = va + 4 * i + imm * 4
        print("%#010x  bl %#010x  %s" % (va + 4 * i, t, owner_of(ctx, t)))
    return 0


def cmd_const(ctx, spec):
    val = int(spec, 16) & 0xFFFFFFFFFFFFFFFF
    chains, pool = materialize_windows(ctx, val)
    print("== %#018x ==" % val)
    print("-- 字面量池里出现该值: %d 处 --" % len(pool))
    for name, a in pool[:20]:
        print("  %s %#x" % (name, a))
    print("-- movz/movk 链: %d 处 --" % len(chains))
    for a in chains[:40]:
        owner = ""
        for name, value, size in ctx.symbols():
            if value and value <= a < value + max(size, 1):
                owner = pretty(name)
        print("  %#x   in %s" % (a, owner))
    return 0


def cmd_dis(ctx, va, n):
    s = ctx.secs[".text"]
    a0 = s["addr"] + (va - s["addr"])
    off = s["off"] + (a0 - s["addr"])
    objdump = os.environ.get("OBJDUMP", "objdump")
    try:
        out = subprocess.run(
            [objdump, "-d", "--start-address=%#x" % a0, "--stop-address=%#x" % (a0 + n),
             ctx.path],
            capture_output=True, text=True, check=False).stdout
    except FileNotFoundError:
        out = ""
    if not out.strip():
        print("(objdump 不可用；用 readelf/llvm-objdump)  节内偏移 %#x" % off)
        return 1
    print(out)
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--so", default=None)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("find"); p.add_argument("pat"); p.add_argument("--limit", type=int, default=80)
    p.set_defaults(fn=lambda c, a: cmd_find(c, a.pat, a.limit))
    p = sub.add_parser("at"); p.add_argument("va")
    p.set_defaults(fn=lambda c, a: cmd_at(c, int(a.va, 16)))
    p = sub.add_parser("mod"); p.add_argument("name")
    p.set_defaults(fn=lambda c, a: cmd_mod(c, a.name))
    p = sub.add_parser("const"); p.add_argument("value")
    p.set_defaults(fn=lambda c, a: cmd_const(c, a.value))
    p = sub.add_parser("dis"); p.add_argument("va"); p.add_argument("--n", type=lambda s: int(s, 0), default=0x60)
    p.set_defaults(fn=lambda c, a: cmd_dis(c, int(a.va, 16), a.n))
    p = sub.add_parser("callers"); p.add_argument("va")
    p.set_defaults(fn=lambda c, a: cmd_callers(c, int(a.va, 16)))
    p = sub.add_parser("bl"); p.add_argument("va"); p.add_argument("--n", type=lambda s: int(s, 0), default=0x800)
    p.set_defaults(fn=lambda c, a: cmd_bl(c, int(a.va, 16), a.n))
    a = ap.parse_args(argv)
    ctx = Elf(a.so or find_so())
    return a.fn(ctx, a)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
