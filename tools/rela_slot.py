#!/usr/bin/env python3
"""查"谁**经指针**用了这个函数"—— 从 `.rela.dyn` 那一侧查起（`dispatch/69`）。

和 [`tools/addr_probe.py callers`](addr_probe.py) 的分工：那边给**结论**（调用点 + 使用者），
这边给**重定位证据**（`.rela.dyn` 的原始条目、槽落在哪个节、以及全 `.text` 里对这个槽的
每一条 rip 引用是**读**还是**写**）。两边的数字应当一致 —— 不一致就说明有一边的扫法漏了。

为什么必须走这一条：Rust 的 `dyn Trait` / 回调 / 泛型单态化之后的方法表都落在
`.data.rel.ro` 或 `.got`，指向它们的**不是** `call rel32`，而是
`mov [rip+x], <imm32/r_addend>` 由 **ld.so 在装载时**按 `R_X86_64_RELATIVE` 写进去的。

```
usage:
    ../.venv/bin/python tools/rela_slot.py 0x3c5f70
    ../.venv/bin/python tools/rela_slot.py 0x3c5f70 --so <别的 libgraphwarrust.so>

正对照（扫法有效性的自证，见 AGENTS.md 合并纪律 A4）：同一份 `.so` 上用已知的槽
`0x9bac98`（文档里有名的"采样器入口"）跑一遍，它必须报"1 个槽、2 条 `call` 读、0 条写"。
```

口径：一律 **ELF 虚地址**（Ghidra 地址 = 本工具地址 + 0x100000，只加一次）。
"""
import argparse
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from loc_xrefs import Elf  # noqa: E402

R_X86_64_RELATIVE = 8
WRITE_MNEMONICS = ('mov', 'movabs', 'movsd', 'movss', 'movd', 'movq', 'movaps', 'movups',
                   'movdqu', 'movdqa', 'movnti', 'xchg', 'add', 'sub', 'and', 'or', 'xor')


def _find_so():
    d = HERE
    for _ in range(12):
        p = os.path.join(d, '.reference', 'steam', '2026-08-28', 'libgraphwarrust.so')
        if os.path.exists(p):
            return p
        d = os.path.dirname(d)
    raise SystemExit('找不到 .reference/steam/2026-08-28/libgraphwarrust.so（用 --so 指）')


def rela_entries(elf):
    """产出 `.rela.dyn` 里每一条 `(index, r_offset, r_info, r_addend)`。"""
    s = elf.by['.rela.dyn']
    c = elf.data[s['offset']:s['offset'] + s['size']]
    for i in range(0, len(c), 24):
        off, info, add = struct.unpack_from('<QQq', c, i)
        yield i // 24, off, info, add


def section_of(elf, addr):
    for s in elf.secs:
        if s['type'] == 8 or not s['size']:      # SHT_NOBITS（.bss/.tbss）没有文件内容但仍有地址
            pass
        if s['addr'] and s['addr'] <= addr < s['addr'] + s['size']:
            return s['name']
    return '?'


def rip_refs_text(elf, targets):
    """扫全 `.text`：返回 `[(insn_addr, insn_text, slot, is_write)]`（只记命中 targets 的）。"""
    from capstone import Cs, CS_ARCH_X86, CS_MODE_64
    from capstone.x86 import X86_OP_MEM, X86_REG_RIP
    t = elf.by['.text']
    code = elf.data[t['offset']:t['offset'] + t['size']]
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    out = []
    for insn in md.disasm(code, t['addr']):
        a = insn.address
        for i, op in enumerate(insn.operands):
            if op.type != X86_OP_MEM or op.mem.base != X86_REG_RIP:
                continue
            slot = a + insn.size + op.mem.disp
            if slot not in targets:
                continue
            mn = insn.mnemonic
            # 目的地是内存（第一个操作数）且是"写内存"的指令 ⇒ 写；`call/jmp/lea/cmp/test` 都是读
            is_write = (i == 0 and mn.split()[0] in WRITE_MNEMONICS
                        and not mn.startswith('cmp') and not mn.startswith('test'))
            out.append((a, '%s %s' % (mn, insn.op_str), slot, is_write))
    return out


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument('target', help='函数虚地址，例如 0x3c5f70')
    ap.add_argument('--so', default=None)
    ap.add_argument('--no-text-scan', action='store_true', help='只查重定位，不扫 .text（快）')
    a = ap.parse_args(argv)

    target = int(a.target, 0)
    so = a.so or _find_so()
    elf = Elf(so)
    print('SO     %s' % so)
    print('TARGET 0x%x  (%s)' % (target, section_of(elf, target)))

    slots = {}
    total = 0
    for idx, off, info, add in rela_entries(elf):
        total += 1
        typ = info & 0xffffffff
        if typ == R_X86_64_RELATIVE and add == target:
            slots[off] = idx
    print('RELA   .rela.dyn 条目 %d 条；addend == 0x%x 的槽 %d 个' % (total, target, len(slots)))
    if not slots:
        print('\n⇒ **没有**任何 `R_X86_64_RELATIVE` 指向它：没人把这个地址当函数指针存进静态槽。')
        print('  （仍可能经寄存器/结构体字段传递 ⇒ 静态扫法有盲区，照 AGENTS.md 写"未观测到"。）')
        return 0
    for off, idx in sorted(slots.items()):
        print('  slot 0x%x   （.rela.dyn[%d]，落在 %s）' % (off, idx, section_of(elf, off)))

    if a.no_text_scan:
        return 0
    print('\n扫全 `.text`（%d 字节）里对上面这些槽的 rip 引用……' % elf.by['.text']['size'])
    refs = rip_refs_text(elf, set(slots))
    try:
        from loc_xrefs import parse_eh_frame_hdr
        starts = sorted(parse_eh_frame_hdr(elf))
    except Exception as e:  # noqa: BLE001
        print('  （.eh_frame 解不出，不标函数归属：%s）' % e)
        starts = []
    import bisect
    print('  %d 条引用（读 %d / 写 %d）'
          % (len(refs), sum(1 for r in refs if not r[3]), sum(1 for r in refs if r[3])))
    for insn_addr, text, slot, is_write in refs:
        owner = ''
        if starts:
            i = bisect.bisect_right(starts, insn_addr) - 1
            owner = '  in 0x%x' % starts[i] if i >= 0 else ''
        print('  0x%08x  %-8s slot 0x%x  %s%s' % (
            insn_addr, 'WRITE' if is_write else 'read', slot, text, owner))
    n_w = sum(1 for r in refs if r[3])
    print('\n⇒ 写这个槽的代码：**%d 条**。%s' % (
        n_w,
        '唯一的写入者是**装载器**（ld.so 按 `.rela.dyn` 的 `R_X86_64_RELATIVE` 填 addend）。'
        if n_w == 0 else '见上面的 WRITE 行 —— 运行时还有别的写入者。'))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
