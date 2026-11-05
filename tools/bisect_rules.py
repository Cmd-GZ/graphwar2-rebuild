#!/usr/bin/env python3
"""normal 模式二分循环：两种 `lo`/`hi` 更新的对照实验（`docs/91`）。

真值规则（`libgraphwarrust.so` @ `0x3c2530`，逐条指令见 `docs/91` §3）：

    mid = (lo+hi)/2 ; chord = hypot(mid-x, f(mid)-y)
    容差满足 → 半步复检 mid2 = (x+mid)/2
        hi = mid2                       <- 0x3c2a0a（先写后判）
        chord <= chord2 ? (lo = x, continue)      <- 0x3c2a47 / 0x3c2a4d
                        : accept(mid)

两个变体：
  base —— 现行 `graphwar_curve.py` / `path.rs`：复检失败只做 `lo = x`，`hi` 不动。
  hi2  —— 上面那条真规则：再补 `hi = mid2`。

用法：
  python3 tools/bisect_rules.py --trace     # 只看 §6 那条轨迹（不需要 unicorn）
  ../.venv/bin/python tools/bisect_rules.py --oracle   # 24 例逐点对照真二进制
  ../.venv/bin/python tools/bisect_rules.py --oracle --quick   # 只跑前 8 例
"""

import argparse
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)                 # gw2-rebuild/
ROOT = os.path.dirname(SUB)                 # 分析仓库根（含 graphwar_curve.py）
sys.path.insert(0, ROOT)

import graphwar_curve as G                  # noqa: E402


# ---------------------------------------------------------------- 二分复刻 ----

def calc(tokens, x0, y0, step, flip, variant):
    """normal 模式采样器（只复刻 normal 分支，ODE 不在此工具范围内）。"""
    ev = G.Evaluator(tokens)
    pts = [(x0, y0)]
    x = -x0 if flip else x0
    y = y0
    n = G.point_budget(step)
    count = 0
    while True:
        lo, hi = x, x + step
        accepted = False
        for _ in range(100):
            mid = (lo + hi) * G.HALF
            fm = ev.eval(mid, 0.0, 0.0)
            chord = G._hypot(mid - x, fm - y)
            if not (G.LENGTH_TOL > abs(chord - step)):
                if chord > step or G._nan(chord):
                    hi = mid
                else:
                    lo = mid
                continue
            mid2 = (x + mid) * G.HALF
            fm2 = ev.eval(mid2, 0.0, 0.0)
            chord2 = G._hypot(mid2 - x, fm2 - y)
            if not (chord > chord2):        # 与 ucomisd+jbe 逐位等价（NaN 走退回）
                lo = x
                if variant == 'hi2':
                    hi = mid2               # 0x3c2a0a
                continue
            px = -mid if flip else mid
            if G._nan(fm) or G._nan(px):
                return pts
            if abs(mid) > G.MAX_ABS_X or abs(fm) > G.MAX_ABS_Y:
                return pts
            pts.append((px, fm))
            count += 1
            x, y = mid, fm
            accepted = True
            break
        if not accepted or count == n:
            return pts


# ------------------------------------------------------------------- 用例 ----

CASE = ("(abs(cos((-1.43*x)))*8.65) + 4.635142442441488",
        -1.3401100016801577, 7.565109751327434, 0.5, False)

SUITE = [
    ('sin x',        'sin(x)',                  5.0,  2.0, 0.5, False),
    ('cos x',        'cos(x)',                  5.0,  2.0, 0.5, False),
    ('abs x',        'abs(x)',                  5.0,  2.0, 0.5, False),
    ('x^2',          'x^2',                     1.0,  2.0, 0.5, False),
    ('x^3',          'x^3',                     1.0,  2.0, 0.5, False),
    ('1/x',          '1/x',                     3.0,  2.0, 0.5, False),
    ('tan x',        'tan(x)',                  0.7,  2.0, 0.5, False),
    ('exp x',        'exp(x)',                  1.0,  2.0, 0.5, False),
    ('sqrt x',       'sqrt(x)',                 4.0,  2.0, 0.5, False),
    ('log x',        'log(x)',                  4.0,  2.0, 0.5, False),
    ('ln x',         'ln(x)',                   4.0,  2.0, 0.5, False),
    ('sin*x',        'sin(x)*x',                5.0,  2.0, 0.5, False),
    ('steep20',      'sin(20*x)',               1.0,  2.0, 0.5, False),
    ('steep50',      'sin(50*x)',               1.0,  2.0, 0.5, False),
    ('sym centre',   'abs(sin(30*x))*3',       -1.2,  1.0, 0.5, False),
    ('flip sin',     'sin(x)',                  5.0,  2.0, 0.5, True),
    ('flip steep20', 'sin(20*x)',               1.0,  2.0, 0.5, True),
    ('flip x^2',     'x^2',                     1.0,  2.0, 0.5, True),
    ('flip 1/x',     '1/x',                     3.0,  2.0, 0.5, True),
    ('flip tan',     'tan(x)',                  0.7,  2.0, 0.5, True),
    ('step0.1 sin3', 'sin(3*x)',                2.0,  1.0, 0.1, False),
    ('step0.1 stp20', 'sin(20*x)',              1.0,  2.0, 0.1, False),
    ('step0.1 abs',  'abs(sin(7*x))*2',         1.0,  1.0, 0.1, False),
    ('doc89 heavy',  CASE[0],                   CASE[1], CASE[2], CASE[3], CASE[4]),
]


# ------------------------------------------------------------------ 模式 1 ----

def trace():
    """打印 doc89 case 第 3→4 点那一段二分：base 与 hi2 各自走到了哪里。"""
    text, x0, y0, step, flip = CASE
    toks = G.parse_function(text)
    ev = G.Evaluator(toks)
    for variant in ('base', 'hi2'):
        x, y = (-x0 if flip else x0), y0
        pts = [(x0, y0)]
        while len(pts) <= 4:
            lo, hi = x, x + step
            rows, wrap = [], None
            mid = None
            for _ in range(100):
                mid = (lo + hi) * G.HALF
                fm = ev.eval(mid, 0.0, 0.0)
                chord = G._hypot(mid - x, fm - y)
                rows.append((lo, hi, mid, chord))
                if not (G.LENGTH_TOL > abs(chord - step)):
                    if chord > step or G._nan(chord):
                        hi = mid
                    else:
                        lo = mid
                    continue
                mid2 = (x + mid) * G.HALF
                fm2 = ev.eval(mid2, 0.0, 0.0)
                chord2 = G._hypot(mid2 - x, fm2 - y)
                if not (chord > chord2):
                    wrap = (len(rows) - 1, x, hi, mid2)
                    lo = x
                    if variant == 'hi2':
                        hi = mid2
                    continue
                break
            if len(pts) == 4:
                print('=== variant %s ===' % variant)
                print('第 3→4 点：共 %d 次迭代，P4 = %.15f  chord = %.15f'
                      % (len(rows), mid, rows[-1][3]))
                if wrap:
                    it, bxp, oldhi, m2 = wrap
                    newhi = m2 if variant == 'hi2' else oldhi
                    print('  it=%d 处复检不通过（chord <= chord2）→ lo = x = %.15f,'
                          ' hi = %.15f（区间宽 %.4g）'
                          % (it, bxp, newhi, newhi - bxp))
                for i, (l, h, m, c) in enumerate(rows):
                    print('   it=%2d lo=%+.15f hi=%+.15f mid=%+.15f chord=%.15f'
                          % (i, l, h, m, c))
                print()
            pts.append((mid, fm))
            x, y = mid, fm
            if mid is None:
                return


# ------------------------------------------------------------------ 模式 2 ----

def oracle(quick=False):
    sys.path.insert(0, os.path.join(ROOT, 'analysis'))
    from gwemu import Gw

    g = Gw()
    cases = SUITE[:8] if quick else SUITE
    ok = {'base': 0, 'hi2': 0}
    for name, text, x0, y0, step, flip in cases:
        raw = G.parse_function(text)
        toks = G.shifted_tokens(raw, G.correction(raw, x0, y0, flip))
        real = g.call_path(toks, x0, y0, step, int(flip), 0)
        res = {}
        for v in ('base', 'hi2'):
            m = calc(toks, x0, y0, step, flip, v)
            if m == real:
                res[v] = 'OK'
                ok[v] += 1
            else:
                idx = next((i for i, (p, q) in enumerate(zip(m, real)) if p != q),
                           min(len(m), len(real)))
                res[v] = 'DIFF@%d (%d vs %d)' % (idx, len(m), len(real))
        print('%-14s step=%-4s flip=%-5s real=%-6d base=%-22s hi2=%s'
              % (name, step, flip, len(real), res['base'], res['hi2']), flush=True)
    print()
    print('TOTAL %d cases: base %d correct, hi2 %d correct'
          % (len(cases), ok['base'], ok['hi2']))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--trace', action='store_true', help='打印 doc89 case 的二分轨迹')
    ap.add_argument('--oracle', action='store_true', help='对真二进制逐点对拍')
    ap.add_argument('--quick', action='store_true', help='只跑前 8 例')
    a = ap.parse_args()
    if a.trace:
        trace()
    if a.oracle:
        oracle(a.quick)
    if not (a.trace or a.oracle):
        ap.print_help()


if __name__ == '__main__':
    main()
