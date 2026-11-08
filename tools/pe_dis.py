#!/usr/bin/env python3
"""tools/pe_dis.py —— 在 **Windows 2.0.60 `graphwarrust.dll`** 里按名字/地址取证。

为什么需要它：`AGENTS.md`「间接调用与函数归属」第 1 条要求"函数指针走 GOT 槽"，
但那套工具（`tools/addr_probe.py`）只认 ELF。Windows 这份是**同版本（2.0.60）、
同指令集（x86-64）、带 COFF 符号表**的构建，查"谁调谁"最快；缺的只是一把能按
`RVA / 名字` 直接反汇编并就地注解（call 目标名字、rip 引用串）的刀。

## 口径（与 `tools/win_names.py` 一致，别混）

* PE 里 COFF 符号的 `Value` 是**节内偏移**，函数地址 = `.text` 的 `VirtualAddress` + `Value`；
  `func_va = ImageBase + RVA`（本 DLL ImageBase = `0x180000000`）。
* 本工具打印一律 **RVA**（= `func_va - ImageBase`），与 `fixtures/windows_name_to_addr.tsv`
  的第 1 列（`0x18013xxxx` = ImageBase+RVA）差一个 ImageBase —— 想对账就减 `0x180000000`。
* 函数边界用 `.pdata`（`RUNTIME_FUNCTION`，12 字节/条），比符号表精确。

## 用法

    ../.venv/bin/python tools/pe_dis.py sym 'GraphwarGameClient'      # 按名字列符号
    ../.venv/bin/python tools/pe_dis.py dis 0x133390                  # 按 RVA 反汇编
    ../.venv/bin/python tools/pe_dis.py at 0x133390                   # 这个 RVA 落在哪个函数
    ../.venv/bin/python tools/pe_dis.py func 'GraphwarGameClient::process'

`--dll` / `GW2_WIN_DLL` 指定二进制，缺省 `.reference/windows/graphwarrust-2.0.60.dll`
（旧路径 `.reference/steam/windows-2.0.60/graphwarrust.dll` 是同一份，见 README 的 sha256）。
"""
import argparse
import os
import re
import struct
import sys

from capstone import Cs, CS_ARCH_X86, CS_MODE_64

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)

REL = os.path.join('.reference', 'windows', 'graphwarrust-2.0.60.dll')
REL_OLD = os.path.join('.reference', 'steam', 'windows-2.0.60', 'graphwarrust.dll')


def _walk_up(rel):
    """`.reference/` 在主仓库根，子 agent 的 worktree 比它深几层 ⇒ 逐级向上找。"""
    d = HERE
    for _ in range(12):
        p = os.path.join(d, rel)
        if os.path.isfile(p):
            return p
        nd = os.path.dirname(d)
        if nd == d:
            break
        d = nd
    return None


DEFAULT_DLL = _walk_up(REL)
FALLBACK = _walk_up(REL_OLD)


def find_dll(explicit=None):
    for p in (explicit, os.environ.get('GW2_WIN_DLL'), DEFAULT_DLL, FALLBACK):
        if p and os.path.isfile(p):
            return p
    raise SystemExit('找不到 Windows DLL；用 --dll 指定或设 GW2_WIN_DLL')


class PE:
    def __init__(self, path):
        self.path = path
        self.data = open(path, 'rb').read()
        d = self.data
        assert d[:2] == b'MZ', 'not PE'
        e_lfanew = struct.unpack_from('<I', d, 0x3c)[0]
        assert d[e_lfanew:e_lfanew + 4] == b'PE\0\0', 'not PE'
        coff = e_lfanew + 4
        self.machine, self.nsec, _, self.psym, self.nsym, self.opt_size, self.chars = \
            struct.unpack_from('<HHIIIHH', d, coff)
        opt = coff + 20
        self.magic = struct.unpack_from('<H', d, opt)[0]
        if self.magic == 0x20b:
            self.image_base = struct.unpack_from('<Q', d, opt + 24)[0]
        else:
            self.image_base = struct.unpack_from('<I', d, opt + 28)[0]
        # sections
        sec_off = opt + self.opt_size
        self.secs = []
        for i in range(self.nsec):
            o = sec_off + 40 * i
            name = d[o:o + 8].rstrip(b'\0').decode('latin1')
            vsize, vaddr, rawsize, rawptr = struct.unpack_from('<IIII', d, o + 8)
            self.secs.append(dict(name=name, vsize=vsize, vaddr=vaddr,
                                  rawsize=rawsize, rawptr=rawptr))
        self.text = next((s for s in self.secs if s['name'] == '.text'), None)
        self.syms = self._parse_coff()

    def _parse_coff(self):
        """COFF 符号表 → [(rva, name)]；只保留函数（sclass 2 = external, type>>4 == 2）。"""
        d, out = self.data, []
        if not self.psym or not self.nsym:
            return out
        strtab = self.psym + 18 * self.nsym
        i = 0
        while i < self.nsym:
            o = self.psym + 18 * i
            name_b, value, secnum, typ, sclass, naux = struct.unpack_from('<8sIhHBB', d, o)
            i += 1 + naux
            if typ >> 4 != 2 or sclass != 2 or secnum <= 0:
                continue
            if secnum > len(self.secs):
                continue
            sec = self.secs[secnum - 1]
            if name_b[:4] == b'\0\0\0\0':
                so = struct.unpack_from('<I', name_b, 4)[0]
                end = d.index(b'\0', strtab + so)
                name = d[strtab + so:end].decode('latin1')
            else:
                name = name_b.rstrip(b'\0').decode('latin1')
            out.append((sec['vaddr'] + value, name))
        out.sort()
        self._sym_rvas = [a for a, _ in out]
        return out

    def name_at(self, rva):
        """RVA 落在哪个符号里（最近的前驱符号）。"""
        import bisect
        idx = bisect.bisect_right(self._sym_rvas, rva) - 1
        if idx < 0:
            return None, 0
        a, n = self.syms[idx]
        return n, rva - a

    def rva2off(self, rva):
        for s in self.secs:
            if s['vaddr'] <= rva < s['vaddr'] + max(s['vsize'], s['rawsize']):
                off = s['rawptr'] + (rva - s['vaddr'])
                return off
        return None

    def read(self, rva, n):
        off = self.rva2off(rva)
        return self.data[off:off + n] if off is not None else b''

    def cstr_at_rva(self, rva, maxlen=80):
        b = self.read(rva, maxlen)
        z = b.find(b'\0')
        if z >= 0:
            b = b[:z]
        try:
            s = b.decode('utf-8')
        except Exception:  # noqa: BLE001
            return None
        if len(s) >= 3 and all(32 <= ord(c) < 127 for c in s):
            return s
        return None


def dis(pe, rva, n, cs):
    code = pe.read(rva, n)
    base = pe.image_base + rva
    out = []
    for ins in cs.disasm(code, base):
        note = ''
        # call/jmp 目标 → 名字
        if ins.mnemonic in ('call', 'jmp') and ins.operands and ins.operands[0].type == 1:
            t = ins.operands[0].imm
            nm, delta = pe.name_at(t - pe.image_base)
            note = '  ; %s%s' % (short(nm) if nm else '?', '' if not delta else '+0x%x' % delta)
        # rip 相对引用 → 串 / 名字
        for op in ins.operands:
            if op.type == 3 and op.mem.base == 41:  # X86_REG_RIP
                tgt = ins.address + ins.size + op.mem.disp
                trva = tgt - pe.image_base
                s = pe.cstr_at_rva(trva) if trva >= 0 else None
                if s:
                    note += '   @0x%x %r' % (trva, s[:60])
                else:
                    nm, delta = pe.name_at(trva)
                    if nm and delta == 0:
                        note += '   [0x%x = %s]' % (trva, short(nm))
        out.append('  0x%x  %-8s %-40s%s' % (ins.address - pe.image_base,
                                            ins.mnemonic, ins.op_str, note))
    return '\n'.join(out)


def short(name):
    """把 legacy mangled 名压成人看得懂的尾巴。"""
    if not name:
        return ''
    m = re.match(r'^_ZN\d+graphwarrust(.*)E$', name)
    if not m:
        return name[:70]
    body = m.group(1)
    parts, i = [], 0
    while i < len(body):
        j = i
        while j < len(body) and body[j].isdigit():
            j += 1
        if j == i:
            break
        ln = int(body[i:j])
        parts.append(body[j:j + ln])
        i = j + ln
    parts = [p for p in parts if not re.match(r'^\d*h[0-9a-f]{16}$', p)]
    return '::'.join(parts[-3:]) if parts else name[:70]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--dll', default=None)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('sym'); p.add_argument('pat'); p.add_argument('--limit', type=int, default=60)
    p = sub.add_parser('dis'); p.add_argument('rva'); p.add_argument('--n', type=lambda s: int(s, 0),
                                                                    default=0x120)
    p = sub.add_parser('at'); p.add_argument('rva')
    p = sub.add_parser('func'); p.add_argument('pat')
    args = ap.parse_args()
    pe = PE(find_dll(args.dll))
    cs = Cs(CS_ARCH_X86, CS_MODE_64)
    cs.detail = True

    if args.cmd == 'sym':
        rx = re.compile(args.pat)
        n = 0
        for rva, name in pe.syms:
            if rx.search(name):
                print('0x%06x  %s' % (rva, name))
                n += 1
                if n >= args.limit:
                    break
        print('== %d hit(s) ==' % n)
    elif args.cmd == 'at':
        rva = int(args.rva, 0)
        nm, delta = pe.name_at(rva)
        print('0x%x -> %s (+0x%x)' % (rva, nm, delta))
    elif args.cmd == 'dis':
        print(dis(pe, int(args.rva, 0), args.n, cs))
    elif args.cmd == 'func':
        rx = re.compile(args.pat)
        for rva, name in pe.syms:
            if rx.search(name):
                print('### 0x%x  %s' % (rva, short(name)))
                print(dis(pe, rva, 0x200, cs))
                return


if __name__ == '__main__':
    main()
