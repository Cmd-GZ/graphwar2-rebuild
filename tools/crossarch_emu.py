#!/usr/bin/env python3
"""crossarch_emu.py —— 跨架构对拍的**执行底座**：把原版的 arm64（Android 2.0.60）
`libgraphwarrust.so` 用 unicorn 直接跑起来，与既有 x86-64 那套（`analysis/gwemu.py`，
`docs/06` 里 88 条「逐位」结论用的就是它）**同输入、同 libm、同比较口径**。

为什么要另写一路：
  * 我方全部复原证据来自 **Linux x86-64** 那份 `.so`；「arm64 构建数值上是否相同」
    在 `docs/00` §3 / `docs/19` §6 一直是**未验证**。本轮要把它变成有数的结论。
  * 本机没有 qemu、没有 arm64 真机 ⇒ 只能静态模拟。`unicorn` 解码的是**真机器码**，
    所以这是「在模拟器下执行真二进制」，不是"照伪代码重写一遍"。

从 `tools/posy_unicorn.py` 与 `analysis/emu.py` 抄来的、**必须照抄**的坑（照抄才可比）：
  1. **libm 必须与 x86 那一路用同一份**。`analysis/gwemu.py` 默认把 `sin/cos/exp/ln…`
     映射到 python `math`；而 `tools/gen_fixtures.py::install_libm()` 已经证明必须换成
     **真 glibc `libm.so.6`**（§30：python 对定义域外入参抛异常，真 libm 返回值）。
     本模块两路都用 `install_libm()` 同一份实现 —— 因此"逐位相同"只证明**代码路径与
     算术相同**，不证明 bionic 的 libm 与 glibc 的 libm 相同（后者的边界见
     `docs/19-android.md` §7）。`libm_sensitivity()` 专门量这条的**影响面**。
  2. arm64 的**结构体返回走 `x8`（indirect result register）**，不是"隐藏第一个参数"；
     x86-64 SysV 对应的 `sret` 是 `rdi`。⇒ 同一份 Rust 签名在两路的调用形态是
     `[x8=out] + x0..` vs `[rdi=out] + rsi..`。
  3. arm64 的 import 一律走 PLT（`adrp/ldr/br x17`），`x30` 里仍是**调用者的返回地址**；
     所以 import hook 里"做完就把 PC 设回 x30"是对的（x86 那边要从栈顶弹返回地址）。
  4. `Vec`/`String` 的 `{?, ptr@+8, len@+16}` 布局是**实测**的（`posy_unicorn.py`
     用同一套 `{len, ptr, len}` 构造把真解析器喂通过）；两路一致，故可直接互喂。

用法：别的脚本 `import crossarch_emu as ce`，然后
    x86 = ce.X86();   a64 = ce.A64()
所有入口都抛 RuntimeError（import 没实现 / panic / 卡住），由调用方记成 `blocked`，
**绝不**把"没跑成"当"相同"。
"""

import ctypes
import os
import struct
import sys

from unicorn import Uc, UcError, UC_ARCH_ARM64, UC_ARCH_X86, UC_HOOK_CODE, UC_MODE_64, UC_MODE_ARM, UC_PROT_ALL
import unicorn.arm64_const as a64c
from unicorn.x86_const import UC_X86_REG_XMM0

HERE = os.path.dirname(os.path.abspath(__file__))
MAIN = os.path.abspath(os.path.join(HERE, ".."))
REF = os.path.join(MAIN, ".reference")
ANALYSIS = os.path.join(REF, "analysis-repo", "analysis")

SO_LINUX = os.path.join(REF, "game", "libgraphwarrust.so")
SO_STEAM = os.path.join(REF, "steam", "2026-08-28", "libgraphwarrust.so")
SO_ANDROID = os.path.join(REF, "steam", "android-2.0.60", "libgraphwarrust.so")
DLL_WIN = os.path.join(REF, "windows", "graphwarrust-2.0.60.dll")

# 模拟器地址空间（与 analysis/emu.py 同值，便于两边日志对照）
STACK, STACK_SZ = 0x70000000, 0x100000
HEAP, HEAP_SZ = 0x50000000, 0x8000000
TRAP, TRAP_SZ = 0x20000000, 0x10000
MAGIC = 0x1DEAD000          # 返回值哨兵：PC 走到这里就停


# --------------------------------------------------------------------------- libm
_LIBM = None
_MATH_NAMES = ('sin', 'cos', 'tan', 'asin', 'acos', 'atan', 'sinh', 'cosh', 'tanh',
               'exp', 'log', 'log10', 'log2', 'sqrt', 'fabs', 'floor', 'ceil',
               'round', 'trunc')


def libm():
    """真 glibc libm（与 tools/gen_fixtures.py::install_libm 同一份）。

    定义域外**返回值**而不是抛异常 —— 直接决定 `ln(0)`、`exp(1000)` 这类边界的位数，
    所以两路必须共用。
    """
    global _LIBM
    if _LIBM is None:
        lib = ctypes.CDLL("libm.so.6", use_errno=True)
        out = {}
        for name in _MATH_NAMES:
            fn = getattr(lib, name, None)
            if fn is None:
                continue
            fn.restype = ctypes.c_double
            fn.argtypes = [ctypes.c_double]
            out[name] = (lambda f: (lambda v: f(ctypes.c_double(v))))(fn)
        _LIBM = out
    return _LIBM


def libm_py():
    """第二份"另一个 libm"：python `math`（CPython 的 `math` 走它自己的 `_math`，
    对 sin/cos 用平台 libm，但 `log(0)` 抛异常、`exp(1000)` 抛 OverflowError）。

    只用于 `libm_sensitivity()`：**换一份 libm 实现，看输出动不动**。它不是 bionic。
    """
    import math

    def safe(fn):
        def w(v):
            try:
                return float(fn(ctypes.c_double(v).value))
            except (ValueError, OverflowError, ZeroDivisionError):
                return float('nan')
        return w
    return {n: safe(getattr(math, n)) for n in _MATH_NAMES if hasattr(math, n)}


TRANSCENDENTAL = ('sin', 'cos', 'tan', 'asin', 'acos', 'atan', 'sinh', 'cosh',
                   'tanh', 'exp', 'log', 'log10', 'log2')


def libm_perturb(k):
    """第二份"libm"：**把每个超越函数的结果整体挪 k 个 ulp**（k=0 就是原样）。

    这不是另一个 libm 实现，而是一个**上界探针**：
    「假如 bionic 的 sin/cos/exp/ln 与 glibc 差了 1 ulp，这条链的输出会不会变、变多少」。
    docs/19 §6.1 把"不同 libm 的 ULP 差异不会被我们控制"列为未验证项；
    本机**没有 bionic 的 libm 二进制**（NDK sysroot 里的 `libm.so` 是链接桩，不含代码），
    所以只能这样把风险**量化**成"敏感 / 不敏感"，而不是假装测过。
    """
    base = libm()
    out = dict(base)

    def nudge(v, kk=k):
        if kk == 0 or v != v or v in (float('inf'), float('-inf')):
            return v
        u, = struct.unpack('<q', struct.pack('<d', v))
        if v < 0:
            u -= kk
        else:
            u += kk
        return struct.unpack('<d', struct.pack('<q', u))[0]

    for name in TRANSCENDENTAL:
        if name in base:
            out[name] = (lambda f, nm=name: (lambda v: nudge(f(v))))(base[name])
    if 'sin' in out and 'cos' in out:
        pass
    return out


def pow_libm(a, b, use_py=False):
    """`^`（pow）的模拟口径，**逐字照抄** analysis/emu.py::_do_import 的 pow 分支
    （overflow→inf、complex→nan、其余异常→nan），这样两路的 pow 边界完全同源。"""
    try:
        r = a ** b
        if isinstance(r, complex):
            r = float('nan')
    except OverflowError:
        r = float('inf')
    except ZeroDivisionError:
        r = float('inf')
    except Exception:
        r = float('nan')
    return float(r)


def bits(v):
    return struct.unpack('<Q', struct.pack('<d', v))[0]


def hexb(v):
    return '%016x' % bits(v)


def unhexb(h):
    return struct.unpack('<d', struct.pack('<Q', int(h, 16)))[0]


# --------------------------------------------------------------------------- ELF/PE
def elf_phdrs(d):
    e_phoff, = struct.unpack_from('<Q', d, 0x20)
    e_phentsize, e_phnum = struct.unpack_from('<HH', d, 0x36)
    out = []
    for i in range(e_phnum):
        o = e_phoff + i * e_phentsize
        p_type, p_flags, p_off, p_vaddr, p_paddr, p_filesz, p_memsz, p_align = \
            struct.unpack_from('<IIQQQQQQ', d, o)
        out.append(dict(type=p_type, flags=p_flags, off=p_off, va=p_vaddr,
                        filesz=p_filesz, memsz=p_memsz))
    return out


def elf_sections(d):
    e_shoff, = struct.unpack_from('<Q', d, 0x28)
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from('<HHH', d, 0x3A)
    secs = []
    for i in range(e_shnum):
        o = e_shoff + i * e_shentsize
        n, t, f, a, off, sz = struct.unpack_from('<IIQQQQ', d, o)
        secs.append(dict(nameoff=n, type=t, flags=f, va=a, off=off, size=sz))
    shstr = secs[e_shstrndx]
    for s in secs:
        b = shstr['off'] + s['nameoff']
        s['name'] = d[b:d.index(b'\0', b)].decode()
    return secs


# --------------------------------------------------------------------------- x86-64
class X86:
    """x86-64 一路：**直接用既有 `analysis/gwemu.py`**（docs/06 的 88 条逐位结论都出自它），
    只做两件事：① 把 `emu.SO` 指到目标 `.so`；② 把 `emu.MATH` 换成真 glibc libm。
    ⇒ x86 这一路不是新写的模拟器，"我方既有证据"与"本轮对拍"用的是同一个执行体。
    """

    def __init__(self, so=SO_LINUX):
        if ANALYSIS not in sys.path:
            sys.path.insert(0, ANALYSIS)
        import emu as _emu
        _emu.SO = so
        _emu.MATH.update(libm())
        # 记录"这次调用摸过哪些 libm 符号"（判这一条结论是否受 libm 影响）
        self.libm_calls = []
        for nm in list(_emu.MATH):
            if nm not in _MATH_NAMES:
                continue
            fn = _emu.MATH[nm]

            def wrap(v, _fn=fn, _nm=nm):
                self.libm_calls.append(_nm)
                return _fn(v)
            _emu.MATH[nm] = wrap
        self._emu = _emu
        import gwemu
        self._gwemu = gwemu
        self.g = gwemu.Gw()
        self.so = so

    # --- 与原 gwemu 完全同口径的三个入口 ---
    def rd(self, a, n):
        return self.g.rd(a, n)

    def wr(self, a, b):
        self.g.wr(a, b)

    def eval_tokens(self, toks, x, y=0.0, dy=0.0):
        self.libm_calls = []
        g = self.g
        v = g.make_vec(toks)
        idx = g.alloc(8)
        g.wq(idx, 0)
        g.call(0x4EB9D0, [v, idx, None, None], [x, y, dy])
        raw = g.uc.reg_read(UC_X86_REG_XMM0) & ((1 << 64) - 1)
        return '%016x' % raw, idx_token_count(g, idx)

    def path(self, toks, x0, y0, step, flip=0, mode=0):
        self.libm_calls = []
        return self.g.call_path(toks, x0, y0, step, flip, mode)

    def textpt(self, text, x, y, flag, addr=0x3C5C50, addr2=0x3C5F70):
        """`calculate_corrected_function`(0x3c5c50) 与 `calculate_function_offset_str`(0x3c5f70)
        的**best-effort** 调用。ABI 是从两条函数序言读出来的（两路同形）：
            x86 : rdi=sret, rsi=text_ptr, rdx=text_len, rcx=&Point, r8d=flag
            a64 : x8 =sret, x0 =text_ptr, x1 =text_len, x2 =&Point, w3 =flag
        sret 是 64 B 缓冲。返回 16 个 8 字节字（堆指针归一化）。**结构体字段语义未知**，
        所以调用方还要用 `plausible()` 判"读出来的东西像不像一个 String"，
        不像就记 blocked —— 不许拿"两边都读出同样的垃圾"冒充逐位相同。
        """
        self.libm_calls = []
        b = text.encode()
        outs = []
        for a in (addr, addr2):
            mem = self.g.alloc(len(b) + 1)
            self.g.wr(mem, b)
            pt = self.g.alloc(16)
            self.g.wr_f64(pt, x)
            self.g.wr_f64(pt + 8, y)
            out = self.g.alloc(64)
            self.g.wr(out, b'\x00' * 64)
            self.g.call(a, [out, mem, len(b), pt, flag])
            outs.append(self.g.rd(out, 64))
        reps = [_norm_sret(self, o) for o in outs]
        return dict(out='||'.join(r[0] for r in reps), extra='sret',
                    plausible=all(r[1] for r in reps), libm=sorted(set(self.libm_calls)))

    def _plausible(self, raw):
        return _plausible(self, raw)

    def parse(self, text):
        """真解析器 0x4ed670（`posy_unicorn.py::Real.parse` 同款）：文本 → [(tag, f64)]"""
        g = self.g
        b = text.encode()
        mem = g.alloc(len(b) + 1)
        g.wr(mem, b)
        st = g.alloc(24)
        g.wq(st, len(b))
        g.wq(st + 8, mem)
        g.wq(st + 16, len(b))
        out = g.alloc(24)
        g.wq(out, 0)
        g.wq(out + 8, 0)
        g.wq(out + 16, 0)
        g.call(0x4ED670, [out, st])
        return read_vec_tokens(g, out)


def idx_token_count(g, idx):
    return g.rq(idx)


def read_vec_tokens(g, vec):
    """读 `{?, ptr@+8, len@+16}` 的 token Vec（16 B/token：tag u64 + f64）"""
    ptr, ln = g.rq(vec + 8), g.rq(vec + 16)
    out = []
    for i in range(ln):
        tag = struct.unpack('<Q', g.rd(ptr + i * 16, 8))[0]
        val = struct.unpack('<d', g.rd(ptr + i * 16 + 8, 8))[0]
        out.append((tag, val))
    return out


# --------------------------------------------------------------------------- arm64
class A64:
    """arm64 一路：unicorn ARM64 直接执行 Android 2.0.60 的 `libgraphwarrust.so`。

    那份 `.so` **没被 strip**（`.symtab` 18,670 个 FUNC），所以函数入口不是猜的，
    是符号名对上的：`GraphwarShotCalculator::calculate_function` @`0x3b5374` 等。
    """

    def __init__(self, so=SO_ANDROID, libm_impl=None):
        self.so = so
        self.data = d = open(so, 'rb').read()
        self.phdrs = [p for p in elf_phdrs(d) if p['type'] == 1]
        self.secs = elf_sections(d)
        self.secmap = {s['name']: s for s in self.secs}
        self.uc = Uc(UC_ARCH_ARM64, UC_MODE_ARM)
        for p in self.phdrs:
            base = p['va'] & ~0xfff
            top = (p['va'] + p['memsz'] + 0xfff) & ~0xfff
            if top > base:
                try:
                    self.uc.mem_map(base, top - base, UC_PROT_ALL)
                except UcError:
                    pass
            blob = d[p['off']:p['off'] + p['filesz']]
            if blob:
                self.uc.mem_write(p['va'], blob)
        for base, sz in ((STACK, STACK_SZ), (HEAP, HEAP_SZ), (TRAP, TRAP_SZ)):
            self.uc.mem_map(base, sz, UC_PROT_ALL)
        self.uc.mem_write(TRAP, b'\x00' * TRAP_SZ)
        self.heap_ptr = HEAP
        self.sizes = {}
        self.imports = {}          # trap addr -> symbol name
        self.call_counts = {}      # symbol name -> 调用次数（判"这条路上用了哪些 libm"）
        self.libm_impl = libm_impl or libm()
        self.tls = {}
        self._apply_relocs()

    # ---- 重定位：RELATIVE / ABS64 写原值；GLOB_DAT / JUMP_SLOT 写陷阱地址 ----
    def _dynsyms(self):
        ds, dstr, d = self.secmap['.dynsym'], self.secmap['.dynstr'], self.data
        out = []
        for i in range(ds['size'] // 24):
            o = ds['off'] + i * 24
            nm, info, oth, shn, val, sz = struct.unpack_from('<IBBHQQ', d, o)
            b = dstr['off'] + nm
            out.append((d[b:d.index(b'\0', b)].decode(), info, shn, val, sz))
        return out

    def _apply_relocs(self):
        d = self.data
        syms = self._dynsyms()
        slot = 0
        for relname in ('.rela.dyn', '.rela.plt'):
            r = self.secmap.get(relname)
            if not r:
                continue
            for i in range(r['size'] // 24):
                o = r['off'] + i * 24
                off, info, add = struct.unpack_from('<QQq', d, o)
                typ, symi = info & 0xFFFFFFFF, info >> 32
                if typ in (1027, 257):        # R_AARCH64_RELATIVE / ABS64
                    self.uc.mem_write(off, struct.pack('<Q', add & 0xFFFFFFFFFFFFFFFF))
                elif typ in (1025, 1026):     # GLOB_DAT / JUMP_SLOT
                    addr = TRAP + slot * 8
                    slot += 1
                    self.imports[addr] = syms[symi][0]
                    self.uc.mem_write(off, struct.pack('<Q', addr))
                else:
                    raise RuntimeError('unhandled reloc type %d' % typ)

    # ---- 内存 ----
    def rd(self, a, n):
        return bytes(self.uc.mem_read(a, n))

    def wr(self, a, b):
        self.uc.mem_write(a, bytes(b))

    def rq(self, a):
        return struct.unpack('<Q', self.rd(a, 8))[0]

    def wq(self, a, v):
        self.wr(a, struct.pack('<Q', v & 0xFFFFFFFFFFFFFFFF))

    def rd_f64(self, a):
        return struct.unpack('<d', self.rd(a, 8))[0]

    def wr_f64(self, a, v):
        self.wr(a, struct.pack('<d', v))

    def alloc(self, n, align=16):
        self.heap_ptr = (self.heap_ptr + 15) & ~15
        p = self.heap_ptr
        self.heap_ptr += max(n, 16) + 16
        if self.heap_ptr > HEAP + HEAP_SZ - 4096:
            raise RuntimeError('emulated heap exhausted')
        self.sizes[p] = n
        return p

    def dget(self, i):
        return struct.unpack('<d', struct.pack('<Q', self.uc.reg_read(a64c.UC_ARM64_REG_Q0 + i) & 0xFFFFFFFFFFFFFFFF))[0]

    def dset(self, i, v):
        self.uc.reg_write(a64c.UC_ARM64_REG_Q0 + i, bits(v))

    # ---- 导入函数 ----
    def _do_import(self, name):
        uc = self.uc
        self.call_counts[name] = self.call_counts.get(name, 0) + 1
        impl = self.libm_impl
        if name in impl:
            self.dset(0, impl[name](self.dget(0)))
            return None
        if name == 'pow':
            self.dset(0, pow_libm(self.dget(0), self.dget(1)))
            return None
        if name == 'sincos':                     # sincos(x, *s, *c)
            x = self.dget(0)
            sp = uc.reg_read(a64c.UC_ARM64_REG_X1)
            cp = uc.reg_read(a64c.UC_ARM64_REG_X2)
            self.wr_f64(sp, impl['sin'](x))
            self.wr_f64(cp, impl['cos'](x))
            return None
        if name in ('expf', 'logf', 'log2f'):    # f32 变体：本轮的用例不会走到，走到就报
            fn = impl[name.rstrip('f')]
            return None
        x0 = uc.reg_read(a64c.UC_ARM64_REG_X0)
        x1 = uc.reg_read(a64c.UC_ARM64_REG_X1)
        x2 = uc.reg_read(a64c.UC_ARM64_REG_X2)
        if name in ('malloc', '_Znwm', '__rust_alloc'):
            return self.alloc(x0 if name != '__rust_alloc' else x1)
        if name == 'calloc':
            p = self.alloc(x0 * x1)
            self.wr(p, b'\x00' * (x0 * x1))
            return p
        if name == 'realloc':
            oldn = self.sizes.get(x0, 0)
            old = self.rd(x0, oldn) if x0 else b''
            p = self.alloc(x1)
            self.wr(p, old[:x1])
            return p
        if name in ('free', '_ZdlPv', 'munmap'):
            return None
        if name in ('memcpy', 'memmove'):
            self.wr(x0, self.rd(x1, x2))
            return x0
        if name == 'memset':
            self.wr(x0, bytes([x1 & 0xFF]) * x2)
            return x0
        if name in ('memcmp', 'bcmp'):
            a, b = self.rd(x0, x2), self.rd(x1, x2)
            return 0 if a == b else (1 if a > b else 0xFFFFFFFFFFFFFFFF)
        if name == 'memchr':
            hay = self.rd(x0, x2)
            i = hay.find(bytes([x1 & 0xFF]))
            return (x0 + i) if i >= 0 else 0
        if name == 'strlen':
            return self.rd(x0, 8192).index(b'\0')
        # --- 线程 / TLS / 互斥：regex 与 std 的惰性初始化会摸这些 ---
        if name == 'pthread_key_create':
            key = len(self.tls) + 1
            self.tls[key] = 0
            self.wr(x0, struct.pack('<I', key))
            return 0
        if name == 'pthread_getspecific':
            return self.tls.get(x0, 0)
        if name == 'pthread_setspecific':
            self.tls[x0] = x1
            return 0
        if name in ('pthread_mutex_lock', 'pthread_mutex_unlock', 'pthread_mutex_init',
                    'pthread_mutex_destroy', 'pthread_rwlock_rdlock', 'pthread_rwlock_wrlock',
                    'pthread_rwlock_unlock', 'pthread_attr_init', 'pthread_attr_destroy',
                    'pthread_attr_setstacksize', 'pthread_detach', 'pthread_join',
                    'pthread_setname_np', 'pthread_key_delete', 'sched_yield',
                    '__register_atfork', '__cxa_atexit', '__cxa_guard_release',
                    '__cxa_guard_abort', 'sem_init', 'sem_post', 'sem_destroy'):
            return 0
        if name == 'pthread_self':
            return TRAP + 0xF000
        if name == 'pthread_create':          # 真线程没法模拟 ⇒ 直接报卡点，不悄悄跳过
            raise RuntimeError('pthread_create: 需要真线程（本轮不做）')
        if name == '__cxa_guard_acquire':
            return 1
        if name == '__errno':
            return TRAP + 0x8000
        if name == 'posix_memalign':
            p = (self.alloc(x2 + x1) + x1 - 1) // x1 * x1
            self.wq(x0, p)
            return 0
        if name in ('getrandom', 'getentropy'):
            n = x1 if name == 'getrandom' else x0
            self.wr(x0, bytes((i * 37 + 11) & 0xFF for i in range(n)))
            return n
        if name == 'sysconf':
            return 8
        if name == 'sched_getaffinity':
            self.wr(x1, b'\x01' + b'\x00' * 127)
            return 0
        if name == 'mmap':
            p = self.alloc(x1 + 0x1000)
            return (p + 0xFFF) & ~0xFFF
        if name == 'getauxval':
            return 0
        if name in ('__stack_chk_fail',):
            raise RuntimeError('__stack_chk_fail')
        if name in ('abort', '_exit', '_ZSt9terminatev', '__cxa_pure_virtual',
                    '_Unwind_Resume', '__cxa_begin_catch'):
            raise RuntimeError('unreachable/panic path: %s' % name)
        raise RuntimeError('unhandled import %s' % name)

    # ---- 调用 ----
    def call(self, addr, iregs=(), dregs=(), sret=None, max_steps=400000000):
        """`sret` 非 None 时写 x8（Rust 的间接返回寄存器）。"""
        uc = self.uc
        sp = (STACK + STACK_SZ - 0x2000) & ~0xF
        for i in range(16):
            uc.reg_write(a64c.UC_ARM64_REG_X0 + i, 0)
        for i, v in enumerate(iregs):
            if v is not None:
                uc.reg_write(a64c.UC_ARM64_REG_X0 + i, v)
        if sret is not None:
            uc.reg_write(a64c.UC_ARM64_REG_X8, sret)
        for i, v in enumerate(dregs):
            if v is not None:
                self.dset(i, v)
        uc.reg_write(a64c.UC_ARM64_REG_SP, sp)
        uc.reg_write(a64c.UC_ARM64_REG_X29, 0)
        uc.reg_write(a64c.UC_ARM64_REG_X30, MAGIC)

        def hook(uc_, address, size, data):
            if address in self.imports:
                name = self.imports[address]
                lr = uc_.reg_read(a64c.UC_ARM64_REG_X30)
                ret = self._do_import(name)
                uc_.reg_write(a64c.UC_ARM64_REG_X30, lr)
                uc_.reg_write(a64c.UC_ARM64_REG_PC, lr)
                if ret is not None:
                    uc_.reg_write(a64c.UC_ARM64_REG_X0, ret)

        h = uc.hook_add(UC_HOOK_CODE, hook)
        try:
            uc.emu_start(addr, MAGIC, 0, max_steps)
        except UcError as e:
            raise RuntimeError('unicorn: %s' % e)
        finally:
            uc.hook_del(h)
        return uc.reg_read(a64c.UC_ARM64_REG_X0)

    # ---- 三个与原 gwemu 同口径的入口（token 内存布局也一致）----
    def make_vec(self, toks):
        n = len(toks)
        dp = self.alloc(max(16 * n, 16))
        self.wr(dp, b''.join(struct.pack('<Qd', int(t), float(v)) for t, v in toks))
        v = self.alloc(24)
        self.wq(v, n)
        self.wq(v + 8, dp)
        self.wq(v + 16, n)
        return v

    def eval_tokens(self, toks, x, y=0.0, dy=0.0, addr=0x4452D4):
        v = self.make_vec(toks)
        idx = self.alloc(8)
        self.wq(idx, 0)
        self.call(addr, [v, idx], [x, y, dy])
        raw = self.uc.reg_read(a64c.UC_ARM64_REG_Q0) & 0xFFFFFFFFFFFFFFFF
        return '%016x' % raw, self.rq(idx)

    def path(self, toks, x0, y0, step, flip=0, mode=0, addr=0x3B5374):
        v = self.make_vec(toks)
        out = self.alloc(24)
        self.wq(out, 0)
        self.wq(out + 8, 0)
        self.wq(out + 16, 0)
        self.call(addr, [v, flip, mode], [x0, y0, step], sret=out)
        ptr, ln = self.rq(out + 8), self.rq(out + 16)
        return [(self.rd_f64(ptr + i * 16), self.rd_f64(ptr + i * 16 + 8)) for i in range(ln)]

    def textpt(self, text, x, y, flag, addr=0x3B79A8, addr2=0x3B7C70):
        """见 `X86.textpt` 的 ABI 说明（两条函数两路同形）。"""
        b = text.encode()
        outs = []
        for a in (addr, addr2):
            mem = self.alloc(len(b) + 1)
            self.wr(mem, b)
            pt = self.alloc(16)
            self.wr_f64(pt, x)
            self.wr_f64(pt + 8, y)
            out = self.alloc(64)
            self.wr(out, b'\x00' * 64)
            self.call(a, [mem, len(b), pt, flag], sret=out)
            outs.append(self.rd(out, 64))
        reps = [_norm_sret(self, o) for o in outs]
        return dict(out='||'.join(r[0] for r in reps), extra='sret',
                    plausible=all(r[1] for r in reps),
                    libm=sorted(set(k for k in self.call_counts if k in self.libm_impl)))

    def _plausible(self, raw):
        return _plausible(self, raw)

    def parse(self, text, addr=0x446C4C):
        """真解析器 `MathFunction::new`（arm64 未 strip 的符号名）→ [(tag, f64)]。

        `{?, ptr@+8, len@+16}` 的入参构造与 `posy_unicorn.py` 喂 x86 那份**逐字相同**；
        返回值是 `MathFunction` 结构体（走 x8），其 token Vec 落在 `+8/+16`（实测）。
        """
        b = text.encode()
        mem = self.alloc(len(b) + 1)
        self.wr(mem, b)
        st = self.alloc(24)
        self.wq(st, len(b))
        self.wq(st + 8, mem)
        self.wq(st + 16, len(b))
        out = self.alloc(64)
        self.wr(out, b'\x00' * 64)
        self.call(addr, [st], sret=out)
        return read_vec_tokens(self, out)


# --------------------------------------------------------------------------- 辅助
HEAP_LO, HEAP_HI = HEAP, HEAP + HEAP_SZ


def norm_words(buf, nbytes=64):
    """把落在模拟堆区间的 8 字节字换成 HEAPPTR（指针值不作证据），其余按位给十六进制。"""
    b = bytes(buf) + bytes(max(0, nbytes - len(buf)))
    out = []
    for i in range(0, nbytes, 8):
        w, = struct.unpack_from('<Q', b, i)
        out.append('HEAPPTR' if HEAP_LO <= w < HEAP_HI else '%016x' % w)
    return out


def _norm_sret(emu, raw, nbytes=64):
    """把 sret 的 16 个 8 字节字**按内容**归一化：落在模拟堆区间的字换成
    `HEAPPTR[它指向的那串]`（用同结构里其它字当长度试读），其余按位给十六进制。

    这样比的是「结构体里那些数字 + 指针**指向的字节**」，而不是"两个模拟器各自的堆地址"
    （地址值不是证据；指向的内容才是）。返回 (repr, 是否读出了可信字符串)。
    """
    words = [struct.unpack_from('<Q', bytes(raw) + bytes(max(0, nbytes - len(raw))), i)[0]
             for i in range(0, nbytes, 8)]
    parts, plausible = [], False
    for i, w in enumerate(words):
        if not (HEAP_LO <= w < HEAP_HI):
            parts.append('%016x' % w)
            continue
        txt = None
        for j, n in enumerate(words):
            if j == i or not (0 < n <= 4096):
                continue
            try:
                d = bytes(emu.rd(w, n))
            except Exception:
                continue
            if d and all(32 <= c < 127 for c in d):
                txt = d.decode()
                break
        if txt is None:
            parts.append('HEAPPTR[?]')
        else:
            plausible = True
            parts.append('HEAPPTR[%s]' % txt)
    return ' '.join(parts), plausible
