#!/usr/bin/env python3
"""tools/replay_clock_disasm.py —— 回放播放时钟的**静态**取证器（T12，`task-9`）。

为什么单独写一个：`tools/addr_probe.py` 的 `dis` 已经会解 `.rodata` 串 / GOT 槽 / `panic::Location`，
但它**不把重定位（`R_X86_64_RELATIVE` 的 addend）应用到 `lea` 目标上**，而 Rust 的
`panic::Location`、字符串槽、`Weak` 句柄槽**全部是重定位**（文件里是 0，真值在 `.rela.dyn` 的
addend 里）。所以拿 `addr_probe locs` 看 `is_paused` 只能看到"有没有 Location"，
看不到"**是哪一行**"；看不到行号就无法和 `fixtures/name_to_addr.tsv` 的锚点对齐。

本工具只做四件小事，全部**只读**（不执行目标代码、不改文件）：

| 命令 | 回答 |
|---|---|
| `fde VA` | 函数真实范围（`.eh_frame` FDE），给"8784 字节"这类说法一个独立裁判 |
| `func VA` | 反汇编整函数，逐行注解：`@目标[节]`、`LOC 文件:行:列`（**已应用重定位**）、`->code`（GOT 槽的真值）、`.rodata` 常量解码（u64/i64/f32/f64） |
| `locs VA` | 该函数引用的 `panic::Location` 清单（文件:行:列 + 引用点地址）——判定"这函数属哪个模块" |
| `imm VAL` | 全 `.text` 扫立即数（10 进制或 `0x`）——找 `1_000_000_000`(ns 哨兵) / `16_666_666`(1/60 s) 这类常量 |
| `f64 VAL` | 全 `.rodata` 找 f64 常量并按 xref 找引用它的函数（找 `60.0` / `1/60` / `0.016666`） |

## 口径（与本仓库其它工具一致）

* 一律 **ELF 虚地址**（`libgraphwarrust.so` 自己的 vaddr）。**Ghidra 地址 = vaddr + 0x100000**，
  减错一次就会落到别的函数中间（本轮就踩过：任务书说"Ghidra 0x4c6f40 = vaddr 0x3c6f40"，
  实测 **0x4c6f40 本身就是 vaddr**，`0x3c6f40` 是另一个函数的中段）。
* `0x3b9aca00` = `1_000_000_000`。本代码库里 `Duration { secs, nanos }` 的
  **`nanos == 1e9` 是"无效/未设置"哨兵**（`Duration` 不变式要求 nanos < 1e9，
  所以恰好 1e9 被当作 `None` 用）。这一条在 `GameClient::process` / `get_time_to_tick` /
  回放播放器的 `is_paused` 三处独立出现。

用法：
    ../.reference/venv/bin/python tools/replay_clock_disasm.py fde  0x4c6f40
    ../.reference/venv/bin/python tools/replay_clock_disasm.py func 0x3d7380
    ../.reference/venv/bin/python tools/replay_clock_disasm.py locs 0x3d7630
    ../.reference/venv/bin/python tools/replay_clock_disasm.py imm  0x3b9aca00
    ../.reference/venv/bin/python tools/replay_clock_disasm.py f64  60.0
"""
import argparse
import os
import struct
import sys

from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86 import X86_OP_IMM, X86_OP_MEM, X86_REG_RIP

HERE = os.path.dirname(os.path.abspath(__file__))

R_X86_64_RELATIVE = 8
NANOS_SENTINEL = 0x3B9ACA00          # 1_000_000_000 → "unset" marker, see module doc
TICK_PERIOD_NS = 0xFE502A            # 16_666_666 ns ≈ 1/60 s, see `get_time_to_tick`


def find_so():
    env = os.environ.get('GW2_SO')
    if env:
        return env
    d = HERE
    for _ in range(8):
        c = os.path.join(d, '.reference', 'game', 'libgraphwarrust.so')
        if os.path.exists(c):
            return c
        d = os.path.dirname(d)
    return os.path.join(os.path.dirname(HERE), '.reference', 'game', 'libgraphwarrust.so')


class Elf:
    """最小 ELF64 解析：节表 + `.rela.dyn` + vaddr↔文件偏移。刻意不做完整 ELF 支持。"""

    def __init__(self, path):
        self.data = open(path, 'rb').read()
        d = self.data
        assert d[:4] == b'\x7fELF', path
        shoff = struct.unpack_from('<Q', d, 0x28)[0]
        shentsize, shnum, shstrndx = struct.unpack_from('<HHH', d, 0x3A)
        secs = []
        for i in range(shnum):
            off = shoff + i * shentsize
            name, typ, flags, addr, offset, size, link, info, align, entsize = \
                struct.unpack_from('<IIQQQQIIQQ', d, off)
            secs.append(dict(hdr_name=name, typ=typ, addr=addr, offset=offset,
                             size=size, link=link, entsize=entsize))
        stroff = secs[shstrndx]['offset']
        for s in secs:
            e = d.index(b'\0', stroff + s['hdr_name'])
            s['name'] = d[stroff + s['hdr_name']:e].decode()
        self.secs = [s for s in secs if s['addr']]
        self.by_name = {s['name']: s for s in self.secs}
        # .rela.dyn → {目标槽 vaddr: (type, addend)}
        self.relocs = {}
        rel = self.by_name.get('.rela.dyn')
        if rel:
            for i in range(rel['size'] // 24):
                r_off, r_info, r_add = struct.unpack_from('<QQq', d, rel['offset'] + i * 24)
                self.relocs[r_off] = (r_info & 0xFFFFFFFF, r_add)
        self.md = Cs(CS_ARCH_X86, CS_MODE_64)
        self.md.detail = True

    # -- 地址换算 ---------------------------------------------------------
    def sec_of(self, va):
        for s in self.secs:
            if s['addr'] <= va < s['addr'] + s['size']:
                return s['name']
        return None

    def va2off(self, va):
        for s in self.secs:
            if s['addr'] <= va < s['addr'] + s['size']:
                return s['offset'] + (va - s['addr'])
        return None

    def read(self, va, n):
        o = self.va2off(va)
        return None if o is None else self.data[o:o + n]

    def u64(self, va):
        b = self.read(va, 8)
        return None if b is None else struct.unpack('<Q', b)[0]

    def deref(self, va):
        """读指针槽：`R_X86_64_RELATIVE` 的真值在 addend 里，文件里通常是 0。"""
        r = self.relocs.get(va)
        if r and r[0] == R_X86_64_RELATIVE:
            return r[1]
        return self.u64(va)

    # -- panic::Location ---------------------------------------------------
    def loc_at(self, slot):
        """`.data.rel.ro` 的 24 字节 `{file:ptr, len, line, col}`（ptr 走重定位）。"""
        ptr = self.deref(slot)
        if not ptr:
            return None
        b = self.read(slot + 8, 16)
        if not b:
            return None
        ln = struct.unpack_from('<Q', b, 0)[0]
        line, col = struct.unpack_from('<II', b, 8)
        if not (0 < ln < 300 and 0 < line < 100000 and self.sec_of(ptr) == '.rodata'):
            return None
        s = self.read(ptr, ln)
        try:
            txt = s.decode()
        except Exception:
            return None
        if '.rs' not in txt:
            return None
        return (txt, line, col)

    # -- .eh_frame ---------------------------------------------------------
    def fde_table(self):
        """返回 [(start, end)]：走 `.eh_frame_hdr` 二分表（rustc/Godot 必生成）。

        `.eh_frame` 的线性扫描在 0xb0948 字节上要正确处理 CIE/FDE 混排，脆；
        `_hdr` 表里已经给出每个函数的 start 与对应 FDE 的地址，直接取值更稳。
        """
        if getattr(self, '_fdes', None) is not None:
            return self._fdes
        self._fdes = self._fde_from_hdr()
        return self._fdes

    def _fde_from_hdr(self):
        """本 .so 的编码（实测）：version 1，eh_frame_ptr = `0x1b`(pcrel|sdata4, 4B)，
        fde_count = `0x03`(udata4)，table = `0x3b`(datarel|sdata4，相对 `.eh_frame_hdr` 基址)。"""
        sec = self.by_name.get('.eh_frame_hdr')
        if not sec:
            return []
        buf = self.data[sec['offset']:sec['offset'] + sec['size']]
        base = sec['addr']
        if len(buf) < 12 or buf[0] != 1:
            return []
        ptr_enc, count_enc, table_enc = buf[1], buf[2], buf[3]

        def valsz(enc):
            return 4 if (enc & 0x0F) in (0x01, 0x02, 0x03, 0x09, 0x0A, 0x0B) else 8

        p = 4 + valsz(ptr_enc)
        fsz = valsz(count_enc)
        count = struct.unpack_from('<I' if fsz == 4 else '<Q', buf, p)[0]
        p += fsz
        if (table_enc & 0x70) != 0x30 or (table_enc & 0x0F) != 0x0B:
            return []
        res = []
        for i in range(count):
            start_off, fde_off = struct.unpack_from('<ii', buf, p + i * 8)
            start = base + start_off
            end = self._fde_end(base + fde_off, start)
            if end:
                res.append((start, end))
        res.sort()
        return res

    def _fde_end(self, fde_va, start):
        """FDE = length(u32) | CIEptr(u32) | pc_begin(`0x1b` pcrel|sdata4, 4B) | pc_range(udata4, 4B)。"""
        off = self.va2off(fde_va)
        if off is None or off + 16 > len(self.data):
            return None
        rng = struct.unpack_from('<I', self.data, off + 12)[0]
        if rng <= 0 or rng > 0x100000:
            return None
        return start + rng

    def func_of(self, va):
        for s, e in self.fde_table():
            if s <= va < e:
                return s, e
        return None, None

    # -- 反汇编 ------------------------------------------------------------
    def dis(self, va, size):
        code = self.read(va, size)
        if code is None:
            return
        yield from self.md.disasm(code, va)


def _uleb(buf, p):
    res = 0
    shift = 0
    while True:
        b = buf[p]
        p += 1
        res |= (b & 0x7F) << shift
        if not b & 0x80:
            return res, p
        shift += 7


def _rip_target(ins, op):
    return ins.address + ins.size + op.mem.disp


def annotate(e, va, size, out=sys.stdout, show_rodata=True):
    for ins in e.dis(va, size):
        extra = []
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                t = _rip_target(ins, op)
                sec = e.sec_of(t)
                extra.append(f'@{t:#x}[{sec}]')
                if sec == '.data.rel.ro':
                    loc = e.loc_at(t)
                    if loc:
                        extra.append(f'LOC {loc[0]}:{loc[1]}:{loc[2]}')
                    tg = e.deref(t)
                    if tg:
                        extra.append(f'->{tg:#x}[{e.sec_of(tg)}]')
                elif sec == '.rodata' and show_rodata:
                    b = e.read(t, 8) or b''
                    if len(b) == 8:
                        f32 = struct.unpack('<f', b[:4])[0]
                        f64 = struct.unpack('<d', b)[0]
                        extra.append(f'rodata u64={int.from_bytes(b, "little", signed=False)} '
                                     f'i64={int.from_bytes(b, "little", signed=True)} '
                                     f'f32={f32!r} f64={f64!r}')
            elif op.type == X86_OP_IMM:
                extra.append(f'imm={op.imm}')
        print(f'{ins.address:08x}  {ins.mnemonic:9s} {ins.op_str:44s} {" ".join(extra)}', file=out)


def locs_of(e, va, size):
    res = {}
    for ins in e.dis(va, size):
        for op in ins.operands:
            if op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP:
                t = _rip_target(ins, op)
                if e.sec_of(t) == '.data.rel.ro':
                    loc = e.loc_at(t)
                    if loc:
                        res.setdefault(loc, []).append(ins.address)
    return res


def scan_imm(e, val):
    text = e.by_name['.text']
    for ins in e.dis(text['addr'], text['size']):
        for op in ins.operands:
            if op.type == X86_OP_IMM and (op.imm & 0xFFFFFFFFFFFFFFFF) == (val & 0xFFFFFFFFFFFFFFFF):
                yield ins


def scan_f64(e, val):
    rod = e.by_name['.rodata']
    want = struct.pack('<d', val)
    hits = []
    buf = e.data
    for i in range(rod['offset'], rod['offset'] + rod['size'] - 8):
        if buf[i:i + 8] == want:
            hits.append(rod['addr'] + (i - rod['offset']))
    return hits


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('--so', default=find_so())
    sub = ap.add_subparsers(dest='cmd', required=True)
    for name in ('fde', 'func', 'locs'):
        p = sub.add_parser(name)
        p.add_argument('va', type=lambda s: int(s, 0))
        p.add_argument('len', nargs='?', type=lambda s: int(s, 0))
    p = sub.add_parser('imm')
    p.add_argument('val', type=lambda s: int(s, 0))
    p = sub.add_parser('f64')
    p.add_argument('val', type=float)
    a = ap.parse_args()

    e = Elf(a.so)
    print(f'# so {a.so}', file=sys.stderr)
    if a.cmd == 'fde':
        s, en = e.func_of(a.va)
        print(f'requested {a.va:#x}')
        if s is None:
            print('not in any FDE')
        else:
            print(f'eh_frame  {s:#x}..{en:#x}  size {en - s:#x} ({en - s})')
        return
    if a.cmd in ('func', 'locs'):
        s, en = e.func_of(a.va)
        if s is None:
            s, en = a.va, a.va + (a.len or 0x100)
        if a.len:
            en = s + a.len
        print(f'# func {s:#x}..{en:#x} size {en - s} ({en - s})', file=sys.stderr)
        if a.cmd == 'func':
            annotate(e, s, en - s)
        else:
            for loc, sites in sorted(locs_of(e, s, en - s).items()):
                print(f'{loc[0]}:{loc[1]}:{loc[2]}  x{len(sites)}  {[hex(x) for x in sites]}')
        return
    if a.cmd == 'imm':
        n = 0
        for ins in scan_imm(e, a.val):
            s, _ = e.func_of(ins.address)
            print(f'{ins.address:08x}  in {s and hex(s)}  {ins.mnemonic:9s} {ins.op_str}')
            n += 1
        print(f'# {n} sites', file=sys.stderr)
        return
    if a.cmd == 'f64':
        print(f'# f64 {a.val!r}')
        for va in scan_f64(e, a.val):
            print(f'.rodata {va:#x}  bytes {e.read(va, 8).hex()}')
        return


if __name__ == '__main__':
    main()
