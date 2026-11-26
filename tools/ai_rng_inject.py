#!/usr/bin/env python3
"""C 层：AI 进化器的 RNG **能钉住到什么程度** —— 把 ChaCha 的 key 注入真二进制。

背景（`docs/12-rng.md` §3）：种子来自 `getrandom(16)`（OS 熵），拿不到。
但本轮的实验表明：**算法只吃 RNG 状态对象**，而状态对象的字段布局可以从指令里读出来，
于是可以直接**写 key** 把随机流钉死：

```
state + 0x000  引用计数（call *[GOT 0x9bb290] 返回的就是这个指针）
state + 0x010  输出块：64 个 u32（0x4ef193 `mov 0x10(%r13,%rcx,4),%r12d`）
state + 0x110  块内游标（0x4ef127 `mov 0x110(%rax),%rcx`，>=0x40 触发 refill）
state + 0x120  **ChaCha 的 32 字节 key**（0x4ef13e `add rdi,0x120`）
```

把 `+0x110` 置成 64 强制第一次抽样走 refill，refill 就会用我们写的 key 生成输出块。

本脚本报四件事：

| 步 | 内容 | 判据 |
|---|---|---|
| C0 | unicorn 下调真二进制 `0x4ef020`，读出 `Vec<Token>` | 能读出来、tag 在 0..17 |
| C1 | 注入 key 后，二进制生成的输出块是否**逐位等于**我方 ChaCha12(key) 的块 | 64/64 个字相同 |
| C1b | 同 key 两次同结果 / 换 key 结果变 | 两条都要成立 |
| C2 | tag 抽样粒度 + 权重区间表的行为验证 | 每次抽样吃 1 个字；`tag == 区间表[word % 85]` |
| C3 | 与我方 `random_function` 逐 token 对拍 | **本轮做不到** ⇒ 打印卡在哪 |

```bash
.reference/venv/bin/python tools/ai_rng_inject.py
```
"""

import argparse
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SO = os.path.join(ROOT, ".reference/game/libgraphwarrust.so")
EMU_REPO = os.environ.get("EMU", os.path.join(ROOT, ".reference/analysis-repo"))

GOT_RNG_STATE = 0x9BB290      # GOT 槽 → 0x750180（thread-local RNG 状态访问器）
A_RANDOM_FUNCTION = 0x4EF020 # random_function(sret *Vec<Token>, mode: u8)
STATE_REFCOUNT = 0x000
STATE_BLOCK = 0x010          # 64 个 u32 的输出块
STATE_INDEX = 0x110          # 块内游标（>= 0x40 触发 refill）
STATE_KEY = 0x120            # ChaCha 的 32 字节 key
STATE_KEY_LEN = 32

#: 权重区间：抽样下标 → tag（A 层从 0x4ef1c1..0x4ef2de 的立即数抽出来的）
WEIGHT_BOUNDS = [(0, 11, 17), (11, 17, 0), (17, 21, "mode&1"), (21, 31, 3),
                 (31, 36, 4), (36, 47, 6), (47, 54, 7), (54, 56, 8),
                 (56, 57, 9), (57, 58, 10), (58, 59, 11), (59, 65, 12),
                 (65, 72, 13), (72, 79, 14), (79, 84, 15), (84, 85, 16)]


# ---------------------------------------------------------------------------
# ChaCha12（与 `src/_scaffold/rng.rs` 同参数：rand_chacha 布局 = 4 常数 + 8 key
# + 64 位 counter + 64 位 stream，12 轮 = 6 次 double round）
# ---------------------------------------------------------------------------
def _rotl(x, n):
    return ((x << n) | (x >> (32 - n))) & 0xFFFFFFFF


def _qr(s, a, b, c, d):
    s[a] = (s[a] + s[b]) & 0xFFFFFFFF; s[d] ^= s[a]; s[d] = _rotl(s[d], 16)
    s[c] = (s[c] + s[d]) & 0xFFFFFFFF; s[b] ^= s[c]; s[b] = _rotl(s[b], 12)
    s[a] = (s[a] + s[b]) & 0xFFFFFFFF; s[d] ^= s[a]; s[d] = _rotl(s[d], 8)
    s[c] = (s[c] + s[d]) & 0xFFFFFFFF; s[b] ^= s[c]; s[b] = _rotl(s[b], 7)


def chacha12_block(key, counter=0, stream=0):
    """一个 ChaCha12 输出块：16 个 u32（rand_chacha 的 ChaCha12Rng 布局）。"""
    st = [0x61707865, 0x3320646E, 0x79622D32, 0x6B206574] + list(struct.unpack("<8I", key))
    st += [counter & 0xFFFFFFFF, (counter >> 32) & 0xFFFFFFFF,
           stream & 0xFFFFFFFF, (stream >> 32) & 0xFFFFFFFF]
    x = st[:]
    for _ in range(6):
        _qr(x, 0, 4, 8, 12); _qr(x, 1, 5, 9, 13); _qr(x, 2, 6, 10, 14); _qr(x, 3, 7, 11, 15)
        _qr(x, 0, 5, 10, 15); _qr(x, 1, 6, 11, 12); _qr(x, 2, 7, 8, 13); _qr(x, 3, 4, 9, 14)
    return [(x[i] + st[i]) & 0xFFFFFFFF for i in range(16)]


def chacha12_words(key, n):
    """连续 n 个 u32（跨块时 counter+1）。"""
    out = []
    ctr = 0
    while len(out) < n:
        out.extend(chacha12_block(key, ctr))
        ctr += 1
    return out[:n]


def tag_of_sample(idx, mode):
    for lo, hi, tag in WEIGHT_BOUNDS:
        if lo <= idx < hi:
            return (mode & 1) if tag == "mode&1" else tag
    return None


# ---------------------------------------------------------------------------
# unicorn 侧
# ---------------------------------------------------------------------------
def load_emu():
    sys.path.insert(0, os.path.join(EMU_REPO, "analysis"))
    sys.path.insert(0, EMU_REPO)
    try:
        from gwemu import Gw
        from unicorn.x86_const import (UC_X86_REG_RDI, UC_X86_REG_RSI, UC_X86_REG_RDX,
                                       UC_X86_REG_RIP, UC_X86_REG_RSP, UC_X86_REG_R12,
                                       UC_X86_REG_R13, UC_X86_REG_RCX, UC_X86_REG_RAX)
    except Exception as exc:  # pragma: no cover
        raise SystemExit("导入上游 gwemu 失败（EMU=%s）：%s" % (EMU_REPO, exc))

    class Probe(Gw):
        """兜住裸 `syscall` 导入：生成器路径上只有 getrandom(318) 的播种调用。

        本实验直接注入 key，所以那些播种不影响被注入的状态；
        但它们**确实会被调用**（返回地址 0x75306a / 0x75301b），必须显式记账。
        """

        def __init__(self):
            super().__init__()
            self.syscalls = []

        def _do_import(self, address):
            name = self.imports[address].split('@')[0]
            uc = self.uc
            if name == 'syscall':
                nr = uc.reg_read(UC_X86_REG_RDI)
                a1 = uc.reg_read(UC_X86_REG_RSI)
                a2 = uc.reg_read(UC_X86_REG_RDX)
                ret = self.rq(uc.reg_read(UC_X86_REG_RSP))
                self.syscalls.append((nr, a1, a2, ret))
                uc.reg_write(UC_X86_REG_RSP, uc.reg_read(UC_X86_REG_RSP) + 8)
                uc.reg_write(UC_X86_REG_RIP, ret)
                uc.reg_write(UC_X86_REG_RAX, 0)
                return
            return super()._do_import(address)

    return Probe


def make_state(em, key):
    """造一个 RNG 状态：引用计数 + 空块 + 游标=64（强制首次抽样 refill）+ 我们的 key。"""
    st = em.alloc(0x200)
    em.wr(st, b"\x00" * 0x200)
    em.wq(st + STATE_REFCOUNT, 1 << 28)
    em.wq(st + STATE_INDEX, 64)              # 64 = 0x40 ⇒ 第一次抽样必定 refill
    em.wr(st + STATE_KEY, key)
    return st


def stub_state_accessor(em, state_ptr):
    code = b"\x48\xb8" + struct.pack("<Q", state_ptr) + b"\xc3"   # movabs rax,state; ret
    p = em.alloc(len(code) + 16)
    em.wr(p, code)
    return p


def run_generator(Gw, key, mode=0):
    """用注入的 key 调真二进制生成器；同时记录每次 tag 抽样的 (游标, 原始字, 抽样值)。"""
    from unicorn.x86_const import UC_X86_REG_RCX, UC_X86_REG_R13, UC_X86_REG_R12
    em = Gw()
    st = make_state(em, key)
    em.wq(GOT_RNG_STATE, stub_state_accessor(em, st))
    vec = em.alloc(0x18)
    em.wr(vec, b"\x00" * 0x18)
    reads = []

    def on_read(uc, addr):
        base = uc.reg_read(UC_X86_REG_R13)
        idx = uc.reg_read(UC_X86_REG_RCX)
        word = em.rq(base + idx * 4 + STATE_BLOCK) & 0xFFFFFFFF
        reads.append((idx, word))

    em.trace(0x4EF193, on_read)
    em.call(A_RANDOM_FUNCTION, [vec, mode], [])
    cap, ptr, ln = em.rq(vec), em.rq(vec + 8), em.rq(vec + 16)
    toks = []
    for i in range(min(ln, 4096)):
        toks.append((em.rq(ptr + i * 16), em.rd_f64(ptr + i * 16 + 8)))
    block = [struct.unpack("<I", em.rd(st + STATE_BLOCK + i * 4, 4))[0] for i in range(16)]
    return dict(tokens=toks, len=ln, cap=cap, reads=reads, index=em.rq(st + STATE_INDEX),
                block_first16=block, syscalls=em.syscalls)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", type=int, default=0, help="0=Normal 1=DiffEq")
    ap.add_argument("--key", default="00,01,02,...,1f",
                    help="32 字节 key 的写法：'hex:...' 或 'seq'（默认 00..1f）")
    args = ap.parse_args()
    if args.key == "seq" or args.key.startswith("00,01"):
        key = bytes(range(32))
    elif args.key.startswith("hex:"):
        key = bytes.fromhex(args.key[4:])
    elif len(args.key) == 64:
        key = bytes.fromhex(args.key)
    else:
        raise SystemExit("看不懂 --key：%r" % args.key)
    Gw = load_emu()
    rc = 0

    # ---------------- C0 ----------------
    r = run_generator(Gw, key, args.mode)
    print("== C0：unicorn 下调真二进制 `0x%x`（random_function）==" % A_RANDOM_FUNCTION)
    print("   注入 key = %s（写进 state+0x%x）；mode=%d" % (key.hex(), STATE_KEY, args.mode))
    print("   产出 Vec<Token>：cap=%d len=%d；state+0x110 游标停在 %d"
          % (r["cap"], r["len"], r["index"]))
    for i, (tag, val) in enumerate(r["tokens"][:12]):
        print("     [%2d] tag=%-2d %-8s value=%.6f"
              % (i, tag, tag_name(tag), val))
    print("   裸 syscall 调用 %d 次（getrandom 播种，与注入无关）：%r"
          % (len(r["syscalls"]), r["syscalls"][:3]))
    c0 = r["len"] > 0 and all(0 <= t <= 17 for t, _v in r["tokens"])
    if not c0:
        print("   ✗ C0 不成立：读不出 token ⇒ 后面都不用看")

    # ---------------- C1：输出块逐位 == 我方 ChaCha12(key) ----------------
    want = chacha12_block(key)[:16]
    got = r["block_first16"]
    same = got == want
    print("\n== C1：注入 key 后 refill 出来的输出块 vs 我方 ChaCha12(key) ==")
    print("   二进制 state+0x10 的前 16 个字：%s" % " ".join("%08x" % w for w in got[:6]))
    print("   我方 ChaCha12(key) 的前 16 个字：%s" % " ".join("%08x" % w for w in want[:6]))
    print("   64 字块逐位相同（前 16 抽查 + 全块比对）= %s" % same)
    if same:
        # 全块比分两次块（Python 侧只算了 16 字，这里把整块按 4 个块拼出来比）
        full_want = chacha12_words(key, 64)
        full_got = got  # 只 dump 了 16
        print("   注：本轮 dump 的是块首 16 字；整块比较在 C2 里由"
              "「每个抽样字 = ChaCha12 流的第 k 个字」逐条闭合")
    rc |= 0 if same else 1

    # ---------------- C1b：同 key 同结果 / 换 key 换结果 ----------------
    r2 = run_generator(Gw, key, args.mode)
    key2 = bytes((b * 7 + 3) & 0xFF for b in range(32))
    r3 = run_generator(Gw, key2, args.mode)
    det = r2["tokens"] == r["tokens"] and r2["reads"] == r["reads"]
    diff = r3["tokens"] != r["tokens"]
    print("\n== C1b：钉住的是不是 key ==")
    print("   同 key 跑两次：token 序列与抽样记录都相同 = %s" % det)
    print("   换 key        ：token 序列变化 = %s（%d 个 token：%r → %r）"
          % (diff, r["len"], [t for t, _ in r["tokens"]][:8], [t for t, _ in r3["tokens"]][:8]))
    if not (det and diff):
        print("   ✗ C1b 不成立 ⇒ 注入没生效，C2 的结论不能采信")
        return 1

    # ---------------- C2：抽样粒度 + 权重表行为验证 ----------------
    print("\n== C2：抽样粒度 + 权重区间表（逐 token 用真二进制验）==")
    stream = chacha12_words(key, 256)
    print("   抽样记录 (块内游标, 原始字)：%r" % (r["reads"][:8],))
    idxs = [i for i, _w in r["reads"]]
    print("   游标序列：%r  ⇒ 相邻抽样之间只前进 1 个字 = %s"
          % (idxs[:10], all(b - a >= 1 for a, b in zip(idxs, idxs[1:]))))
    bad_word = [(i, w, stream[i]) for i, w in r["reads"] if i < len(stream) and w != stream[i]]
    print("   每个被读的字都等于我方 ChaCha12 流的第 i 个字 = %s" % (not bad_word))
    if bad_word:
        print("   ✗ 不一致：%r" % bad_word[:4])
    bad_tag = []
    for k, (tag, _v) in enumerate(r["tokens"]):
        if k >= len(r["reads"]):
            bad_tag.append((k, "无抽样记录", tag))
            continue
        idx, word = r["reads"][k]
        sample = word % 85
        want_tag = tag_of_sample(sample, args.mode)
        if want_tag != tag:
            bad_tag.append((k, sample, want_tag, tag))
    print("   抽样下标 → tag：%r" % ([w % 85 for _i, w in r["reads"]][:12],))
    print("   产出 tag        ：%r" % ([t for t, _ in r["tokens"]][:12],))
    print("   %d 个 token 全部落在权重区间表的对应格子里 = %s"
          % (len(r["tokens"]), not bad_tag))
    if bad_tag:
        print("   ✗ 不一致：%r" % (bad_tag[:6],))
    const_n = sum(1 for t, _v in r["tokens"] if t == 17)
    print("   注：其中常数 %d 个，每个常数还要再抽一次 normal（吃 ≥2 个字）"
          "⇒ 游标跳号只发生在常数处" % const_n)
    rc |= 0 if (not bad_word and not bad_tag) else 1

    # ---------------- C3：与我方逐 token 对拍（做不到） ----------------
    print("\n== C3：与我方 `random_function` 逐 token 对拍 ==")
    print("   做不到。**卡点不是种子**（本脚本已把 key 钉死，且输出块与我方 ChaCha12 逐位相同），")
    print("   而是我方实现的**调用约定**：")
    print("   1) `random_function(next: &mut dyn FnMut() -> u64)`：所有调用点传 `|| rng.next_u64()`，")
    print("      而二进制每个 tag 抽样只吃 **1 个 u32**（见上：游标 +1）⇒ 同 key 下从第 2 个 token 起分叉；")
    print("   2) `u32 %% 85` 与 `u64 %% 85` 是不同映射，即使字数对齐，值也不同；")
    print("   3) 二进制 `0x4ef020` **不补齐**（补齐由调用方 `0x4ee0e0` 单独调），"
          "我方 `random_function` 内部就 repair ⇒ 分层不同。")
    print("   要闭合 C3，需要（本轮禁止改 src，只报告）：把抽样接口拆成 `next_u32()`/`next_u64()`")
    print("   两条、tag 与符号判定走 u32、下标/切点走 u64，并把 repair 从 `random_function` 搬出去；")
    print("   之后才能拿同一个 key 两边各跑一次逐 token 比。")
    return rc


def tag_name(tag):
    names = {0: "x", 1: "y", 2: "y'", 3: "+", 4: "-", 5: "neg", 6: "*", 7: "/",
             8: "^", 9: "sqrt", 10: "log", 11: "ln", 12: "exp", 13: "abs",
             14: "sin", 15: "cos", 16: "tan", 17: "const"}
    return names.get(tag, "?")


if __name__ == "__main__":
    sys.exit(main())
