#!/usr/bin/env python3
"""C3 对拍：**同一个 ChaCha key 下**，我方 AI 进化器的生成/补齐/变异/工厂 与真二进制逐 token 相同吗？

背景（`docs/06` §73.4、`docs/13` §26.4）：T23 已证明

* 真二进制的 RNG 状态 `+0x120` 是 ChaCha 的 key，写进去就能把随机流**钉死**；
* 注入后二进制生成的输出块与我方 `ChaCha12`（`rand_chacha` 布局、12 轮）**逐位相同**；
* 二进制**按 u32 字**取数（`mov 0x10(%r13,%rcx,4),%r12d`，游标 +1），
  而 `next_u64`（`0x502b20`）一次吃 **2 个字**。

T23 卡在 C3：我方抽样接口只有 `FnMut() -> u64`（2 字/次）⇒ 同 key 下从第 2 个 token 起分叉。
T28 把内部改成“字流”（1 字/次）后，这条就能闭合了。本脚本产出**真值夹具**
`fixtures/ai_gen_parity.tsv`，由 `src/graphwar_function_evolutionator.rs` 里的新测试读入比对：

    tag 列必须**逐个相同**；const token 还要比 f64 的**位型**（`value_bits`）。
    非 const token 的 `value` 字段在二进制里是**上一枚常量的残值**（生成本来就不写它），
    所以夹具照抄但测试不比（见 TSV 里 `value_kind` 列）。

被调的真二进制函数（vaddr）：

| 名字 | vaddr | 签名（从指令读出） |
|---|---|---|
| `random_function` | `0x4ef020` | `fn(sret *mut Vec<Token>, mode: u8)`；**不补齐**，`n<=0` 返回空 Vec |
| `assemble`（补齐） | `0x4ee0e0` | `fn(sret *mut Vec<Token>, mode: u8, src: &Vec<Token>)` |
| `mutate` | `0x4ee720` | `fn(sret *mut Vec<Token>, src: &Vec<Token>)` |
| `factory`（子代工厂） | `0x4ee3a0` | `fn(sret *mut Vec<Token>, mode: &u8, parent: &Vec<Token>)` |

用法：

    .reference/venv/bin/python tools/ai_gen_parity.py            # 写 fixtures/ai_gen_parity.tsv
    .reference/venv/bin/python tools/ai_gen_parity.py --print    # 只打印

⚠️ 依赖 `.reference/analysis-repo` 的 `gwemu`（unicorn）与 `.reference/game/libgraphwarrust.so`。
"""

import argparse
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

from ai_rng_inject import (  # noqa: E402  复用 T23 的注入链
    GOT_RNG_STATE,
    STATE_BLOCK,
    STATE_KEY,
    chacha12_words,
    load_emu,
    make_state,
    stub_state_accessor,
)

A_RANDOM_FUNCTION = 0x4EF020
A_ASSEMBLE = 0x4EE0E0
A_MUTATE = 0x4EE720
A_FACTORY = 0x4EE3A0

TOKEN_SIZE = 16  # {tag: u64, value: f64}


# ---------------------------------------------------------------------------
# 真二进制侧
# ---------------------------------------------------------------------------
def _rng_setup(em, key):
    st = make_state(em, key)
    em.wq(GOT_RNG_STATE, stub_state_accessor(em, st))
    return st


def _vec_in(em, tokens):
    """把 [(tag, value_bits)] 铺进一个二进制侧 `Vec<Token>`（cap/ptr/len @ +0/+8/+0x10）。"""
    n = len(tokens)
    cap = max(n, 1)
    buf = em.alloc(cap * TOKEN_SIZE + TOKEN_SIZE)
    em.wr(buf, b"\x00" * (cap * TOKEN_SIZE))
    for i, (tag, bits) in enumerate(tokens):
        em.wq(buf + i * TOKEN_SIZE, tag & 0xFFFFFFFFFFFFFFFF)
        em.wq(buf + i * TOKEN_SIZE + 8, bits & 0xFFFFFFFFFFFFFFFF)
    vec = em.alloc(0x18)
    em.wq(vec, cap)
    em.wq(vec + 8, buf)
    em.wq(vec + 0x10, n)
    return vec


def _vec_out(em):
    vec = em.alloc(0x18)
    em.wr(vec, b"\x00" * 0x18)
    return vec


def _read_vec(em, vec):
    cap, ptr, ln = em.rq(vec), em.rq(vec + 8), em.rq(vec + 0x10)
    out = []
    for i in range(min(ln, 512)):
        tag = em.rq(ptr + i * TOKEN_SIZE)
        bits = em.rq(ptr + i * TOKEN_SIZE + 8)
        out.append((tag, bits, cap, ln))
    return out, cap, ln


def run_binary(Gw, which, key, mode, tokens=()):
    em = Gw()
    _rng_setup(em, key)
    out = _vec_out(em)
    if which == "gen":
        em.call(A_RANDOM_FUNCTION, [out, mode], [])
    elif which == "assemble":
        src = _vec_in(em, tokens)
        em.call(A_ASSEMBLE, [out, mode, src], [])
    elif which == "mutate":
        src = _vec_in(em, tokens)
        em.call(A_MUTATE, [out, src], [])
    elif which == "factory":
        src = _vec_in(em, tokens)
        mode_buf = em.alloc(8)
        em.wr(mode_buf, bytes([mode & 0xFF]) + b"\x06\xfa\x99\x88\x77\x66\x55")
        em.call(A_FACTORY, [out, mode_buf, src], [])
    else:
        raise SystemExit("unknown case %r" % which)
    toks, cap, ln = _read_vec(em, out)
    idx = em.rq(em.rq(GOT_RNG_STATE)) if False else None  # 游标在 state 里，见下
    return toks, cap, ln


def find_key_with_first_word_positive():
    """工厂的第一笔抽样是 **1 个字**（0x4ee42a），符号决定走不走变异臂 ⇒ 需要正字来打交叉/重生臂。"""
    for a in range(1, 300):
        key = bytes((a + i * 37) & 0xFF for i in range(32))
        if chacha12_words(key, 1)[0] < 0x80000000:
            return key
    raise SystemExit("找不到首字为正的 key")


# ---------------------------------------------------------------------------
# 夹具行
# ---------------------------------------------------------------------------
def fmt_tokens(toks):
    return ",".join("%d:%016x" % (t, b) for t, b, _c, _l in toks)


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--print", dest="do_print", action="store_true")
    ap.add_argument("--out", default=os.path.join(ROOT, "fixtures/ai_gen_parity.tsv"))
    args = ap.parse_args(argv)

    Gw = load_emu()
    k1 = bytes(range(32))
    k1b = bytes((b * 7 + 3) & 0xFF for b in range(32))
    kpos = find_key_with_first_word_positive()

    # 夹具用的输入 token 串（tag 编号 = `src/lib.rs` 的 `Function` 判别值）
    X = 0
    ADD, SUB, MUL, DIV = 3, 4, 6, 7
    CONST = 17

    keys = {}
    keys["k1"] = k1
    keys["k1b"] = k1b
    keys["kpos"] = kpos
    for i in range(12):
        keys["k2_%d" % i] = bytes((i * 53 + j * 17 + 5) & 0xFF for j in range(32))

    def C0(v):
        return (CONST, struct.unpack("<Q", struct.pack("<d", v))[0])

    # 需要“补齐”的输入（need > 0）：只有 (Add) 一个 token
    needs_fill = [(ADD, 0)]
    # 需要“截断”的输入（有多余尾巴）
    short = [(ADD, 0), C0(1.5), (X, 0), (MUL, 0), (X, 0), C0(5.0), (X, 0), C0(7.0)]
    parent20 = [C0(1.0), (MUL, 0)] + [C0(float(i)) for i in range(18)]
    parent5 = [C0(2.0), (ADD, 0), (X, 0), C0(3.0), (X, 0)]

    cases = []
    for kn in ("k1", "k1b") + tuple("k2_%d" % i for i in range(12)):
        cases.append(("gen_m0_" + kn, "gen", kn, 0, ()))
    for kn in ("k1", "k2_0", "kpos"):
        cases.append(("gen_m1_" + kn, "gen", kn, 1, ()))
    cases += [
        ("assemble_trunc_m0_k1", "assemble", "k1", 0, tuple(short)),
        ("assemble_trunc_m1_k1", "assemble", "k1", 1, tuple(short)),
        ("assemble_fill_m0_k1", "assemble", "k1", 0, tuple(needs_fill)),
        ("assemble_fill_m0_k2", "assemble", "k2_0", 0, tuple(needs_fill)),
        ("assemble_fill_m1_k1", "assemble", "k1", 1, tuple(needs_fill)),
        ("mutate_m0_k1", "mutate", "k1", 0, tuple(short)),
        ("mutate_m0_k2", "mutate", "k2_0", 0, tuple(short)),
        ("mutate_m0_kpos", "mutate", "kpos", 0, tuple(needs_fill)),
        # 工厂三臂：k1 首字 = 0xfff931f2（负）⇒ 变异臂；kpos 首字为正 ⇒ 交叉 / 重生
        ("factory_mutate_k1", "factory", "k1", 0, tuple(parent20)),
        ("factory_cross_kpos", "factory", "kpos", 0, tuple(parent20)),
        ("factory_rebirth_kpos", "factory", "kpos", 0, tuple(parent5)),
        ("factory_cross_kpos_m1", "factory", "kpos", 1, tuple(parent20)),
    ]
    # 工厂：跨 12 个 key 各打一发 parent20（走交叉臂）与 parent5（走重生臂），
    # 这样至少有几行的产出能区分“交叉的裸切片”与“按 arity 走子树”（长度会不同）。
    # `[Add]×20` 作父代：交叉切片 = `[Add]×r2` ⇒ 补齐后长度 = **2·r2+1**，
    # 于是“输出长度”直接编码 r2 ⇒ 能同时判 #3（裸切片）与 #4（切点吃 2 个字）。
    parent_add20 = [(ADD, 0)] * 20
    for i in range(12):
        kn = "k2_%d" % i
        cases.append(("factory_c20_" + kn, "factory", kn, 0, tuple(parent20)))
        cases.append(("factory_p5_" + kn, "factory", kn, 0, tuple(parent5)))
        cases.append(("factory_add20_" + kn, "factory", kn, 0, tuple(parent_add20)))
    cases.append(("assemble_add5_k1", "assemble", "k1", 0, tuple([(ADD, 0)] * 5)))
    cases.append(("assemble_add5_kpos", "assemble", "kpos", 0, tuple([(ADD, 0)] * 5)))
    rows = []
    for name, which, kname, mode, toks in cases:
        got, cap, ln = run_binary(Gw, which, keys[kname], mode, toks)
        rows.append(
            "\t".join(
                [
                    name,
                    which,
                    kname,
                    keys[kname].hex(),
                    str(mode),
                    ",".join("%d:%016x" % t for t in toks),
                    fmt_tokens(got),
                    str(ln),
                ]
            )
        )
        print("%-24s %-9s mode=%d in=%2d → len=%2d tags=%s"
              % (name, which, mode, len(toks), ln, [t for t, _b, _c, _l in got]))
    header = "\t".join(
        ["case", "fn", "key_name", "key_hex", "mode", "in_tokens", "out_tokens", "out_len"]
    )
    text = header + "\n" + "\n".join(rows) + "\n"
    if not args.do_print:
        with open(args.out, "w") as f:
            f.write(text)
        print("\n写入 %s（%d 行 + 表头）" % (args.out, len(rows)))
    else:
        print("\n" + text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
