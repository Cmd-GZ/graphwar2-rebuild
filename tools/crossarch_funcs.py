#!/usr/bin/env python3
"""crossarch_funcs.py —— 原版 **x86-64（Linux）与 arm64（Android）两份构建**，
在 unicorn 下**同输入执行同一批数值核心函数**，输出**逐位**比较（T9 第 ③ 步）。

输入：`fixtures/crossarch_plan.txt` 的 `[func]` / `[in.*]`（**预注册件**）。
      ⚠️ 输入集**只从预注册件里读**；本脚本没有"内置用例"，也没有默认值。
输出：`fixtures/crossarch_funcs.tsv`

两路执行体（为什么这样才叫"对拍"）：
  * **x86-64 一路** = `analysis/gwemu.py`（我方 88 条「逐位」结论用的同一个执行体），
    只把 `emu.MATH` 换成**真 glibc libm**（与 `tools/gen_fixtures.py::install_libm` 同款，
    理由见那条函数注释：python `math` 对定义域外**抛异常**、真 libm **返回值**）。
  * **arm64 一路** = `tools/crossarch_emu.py::A64`：unicorn ARM64 直接跑 Android 2.0.60
    那份 `.so` 的**真机器码**。函数入口不是猜的 —— 那份 `.so` 没 strip，入口来自
    `.symtab`（例：`GraphwarShotCalculator::calculate_function` @`0x3b5374`）。
  * 两路的**导入函数（libm/内存）用同一份实现** ⇒ 输出逐位相同证明的是
    **「代码路径 + 算术」同**；**不**证明 bionic 的 libm 与 glibc 的 libm 同。
    后者的影响面由 `--libm-probe` 单独量（换第二份 libm 实现，看输出是否动）。

判据（预注册）：`bit-identical`。
  * eval  ：返回 f64 的 64 位 + 消耗 token 数，都相同；
  * path  ：点数组长度相同、**每个点的 16 字节**都相同；
  * parse ：token 流长度相同、每条 (tag u64, f64 64 位) 都相同；
  * textpt：返回结构体前 64 字节，其中落在模拟堆区间的 8 字节字（指针）归一化后相同。
  差 1 ulp = **不同**。跑不起来 = `blocked` + 原因，**不许**记成相同。

用法：
    .reference/venv/bin/python tools/crossarch_funcs.py                  # 全跑
    .reference/venv/bin/python tools/crossarch_funcs.py --only F6 F7     # 只跑几个
    .reference/venv/bin/python tools/crossarch_funcs.py --libm-probe     # 追加 libm 敏感度
    .reference/venv/bin/python tools/crossarch_funcs.py --controls       # 只跑对照 C4/C5/C6
"""

import argparse
import hashlib
import os
import shutil
import struct
import sys
import traceback

HERE = os.path.dirname(os.path.abspath(__file__))
MAIN = os.path.abspath(os.path.join(HERE, ".."))
PLAN = os.path.join(MAIN, "fixtures", "crossarch_plan.txt")
OUT = os.path.join(MAIN, "fixtures", "crossarch_funcs.tsv")
sys.path.insert(0, HERE)
import crossarch_emu as ce  # noqa: E402
import crossarch_tables as ct  # noqa: E402  (复用 plan 解析 / 二进制装载)

HEAP_LO, HEAP_HI = ce.HEAP, ce.HEAP + ce.HEAP_SZ


# --------------------------------------------------------------------------- inputs
def parse_tokens(s):
    out = []
    for part in s.split(';'):
        part = part.strip()
        if not part:
            continue
        tag, val = part.split(':')
        if val.startswith('#'):
            out.append((int(tag), ce.unhexb(val[1:])))
        else:
            out.append((int(tag), ce.unhexb(val)))
    return out


def load_inputs():
    """从预注册件读 `[in.F*]`。字段含义由该函数的 kind 决定。"""
    heads = {}
    cur = None
    for raw in open(PLAN, encoding='utf-8'):
        line = raw.rstrip('\n')
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        if line.startswith('[in.'):
            cur = line[4:-1]
            heads[cur] = []
            continue
        if line.startswith('['):
            cur = None
            continue
        if cur is None:
            continue
        heads[cur].append(line.split('\t'))
    return heads


# --------------------------------------------------------------------------- 比较
def norm_words(buf, nbytes=64):
    """把落在模拟堆区间的 8 字节字换成 HEAPPTR（两边堆布局一致，但不该拿地址当证据）"""
    out = []
    b = buf + bytes(max(0, nbytes - len(buf)))
    for i in range(0, nbytes, 8):
        w, = struct.unpack_from('<Q', b, i)
        out.append('HEAPPTR' if HEAP_LO <= w < HEAP_HI else '%016x' % w)
    return out


def is_nan_bits(h):
    try:
        return ce.unhexb(h) != ce.unhexb(h)
    except Exception:
        return False


def classify(xs, as_):
    """把"不同"分成两类：
      nan-only : 两边都是 NaN，只有**符号/载荷**不同（IEEE-754 对 NaN 的位模式不作规定，
                 x86-64 的"无效运算默认 NaN"是 0xFFF8..，AArch64 是 0x7FF8..）；
      value    : 真数值差异（差 1 ulp 也算这一类，必须逐条查）。
    """
    if xs == as_:
        return 'bit-identical'
    if xs.count(';') != as_.count(';') or len(xs.split(';')) != len(as_.split(';')):
        return 'DIFF:length'
    px, pa = xs.split(';'), as_.split(';')
    kinds = set()
    for u, v in zip(px, pa):
        if u == v:
            continue
        fu, fv = u.split(','), v.split(',')
        if len(fu) != len(fv):
            kinds.add('value')
            continue
        for a, b in zip(fu, fv):
            if a == b:
                continue
            kinds.add('nan-only' if (is_nan_bits(a) and is_nan_bits(b)) else 'value')
    if kinds == {'nan-only'}:
        return 'DIFF:nan-only'
    return 'DIFF:' + ('+'.join(sorted(kinds)) or 'value')


def compact(out, limit=160):
    """TSV 里不塞整条点数组（一组 0.001 步长的采样有 5 万个点、几十万字符）。
    改成 **长度 + sha256 + 首末项** —— 判据一点没松：长度相同且 sha256 相同 ⇔ 字节全同。
    """
    if len(out) <= limit:
        return out
    import hashlib as _h
    parts = out.split(';')
    return 'n=%d sha256=%s head=%s tail=%s' % (
        len(parts), _h.sha256(out.encode()).hexdigest()[:16], parts[0], parts[-1])


def points_bytes(pts):
    return ';'.join('%016x,%016x' % (ce.bits(x), ce.bits(y)) for x, y in pts)


def tokens_bytes(toks):
    return ';'.join('%d:%016x' % (t, ce.bits(v)) for t, v in toks)


# --------------------------------------------------------------------------- 单侧调用
def run_case_x86(x, kind, row):
    """x86-64 一路。kind: eval / path / parse / textpt"""
    if kind == 'eval':
        cid, toks, xb, yb, dyb = row
        val, nidx = x.eval_tokens(parse_tokens(toks), ce.unhexb(xb), ce.unhexb(yb), ce.unhexb(dyb))
        return dict(kind=kind, out=val, extra='ntok=%d' % nidx, libm=sorted(set(x.libm_calls)))
    if kind == 'path':
        cid, toks, xb, yb, sb, flip, mode = row
        pts = x.path(parse_tokens(toks), ce.unhexb(xb), ce.unhexb(yb), ce.unhexb(sb),
                     int(flip), int(mode))
        return dict(kind=kind, out=points_bytes(pts), extra='n=%d' % len(pts), libm=sorted(set(x.libm_calls)))
    if kind == 'parse':
        cid, text = row
        toks = x.parse(text)
        return dict(kind=kind, out=tokens_bytes(toks), extra='ntok=%d' % len(toks), libm=[])
    if kind == 'textpt':
        cid, text, xb, yb, flag = row
        return x.textpt(text, ce.unhexb(xb), ce.unhexb(yb), int(flag))
    raise RuntimeError('unknown kind %s' % kind)


def run_case_a64(a, kind, row):
    if kind == 'eval':
        cid, toks, xb, yb, dyb = row
        val, nidx = a.eval_tokens(parse_tokens(toks), ce.unhexb(xb), ce.unhexb(yb), ce.unhexb(dyb))
        return dict(kind=kind, out=val, extra='ntok=%d' % nidx,
                    libm=sorted(k for k in a.call_counts if k in ce.libm()))
    if kind == 'path':
        cid, toks, xb, yb, sb, flip, mode = row
        pts = a.path(parse_tokens(toks), ce.unhexb(xb), ce.unhexb(yb), ce.unhexb(sb),
                     int(flip), int(mode))
        return dict(kind=kind, out=points_bytes(pts), extra='n=%d' % len(pts),
                    libm=sorted(k for k in a.call_counts if k in ce.libm()))
    if kind == 'parse':
        cid, text = row
        toks = a.parse(text)
        return dict(kind=kind, out=tokens_bytes(toks), extra='ntok=%d' % len(toks),
                    libm=sorted(k for k in a.call_counts if k in ce.libm()))
    if kind == 'textpt':
        cid, text, xb, yb, flag = row
        return a.textpt(text, ce.unhexb(xb), ce.unhexb(yb), int(flag))
    raise RuntimeError('unknown kind %s' % kind)


# --------------------------------------------------------------------------- 主流程
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--only', nargs='*')
    ap.add_argument('--libm-probe', action='store_true')
    ap.add_argument('--controls', action='store_true')
    ap.add_argument('--no-write', action='store_true')
    a = ap.parse_args()

    bins_raw, tables, funcs, ctrls = ct.parse_plan()
    inputs = load_inputs()

    if a.controls:
        do_controls()
        return

    x86 = ce.X86()
    a64 = ce.A64()

    rows = []
    func_status = {}
    for f in funcs:
        fid = f['id']
        if a.only and fid not in a.only:
            continue
        cases = inputs.get(fid, [])
        libm_hit = set()
        n_ok = n_bad = n_blk = 0
        for row in cases:
            cid = row[0]
            try:
                a64.call_counts = {}
                rx = run_case_x86(x86, f['kind'], row)
                libm_hit |= set(rx.get('libm') or [])
                rx_err = None
            except Exception as e:
                rx, rx_err = None, '%s: %s' % (type(e).__name__, e)
            try:
                a64.call_counts = {}
                ra = run_case_a64(a64, f['kind'], row)
                libm_hit |= set(ra.get('libm') or [])
                ra_err = None
            except Exception as e:
                ra, ra_err = None, '%s: %s' % (type(e).__name__, e)

            if rx is None and ra is None:
                n_blk += 1
                rows.append(dict(fid=fid, kind=f['kind'], cid=cid, status='blocked',
                                 cls='blocked:both', x86='-', a64='-',
                                 note='both blocked: x86[%s] a64[%s]' % (rx_err, ra_err), libm=''))
            elif rx is None or ra is None:
                n_blk += 1
                rows.append(dict(fid=fid, kind=f['kind'], cid=cid, status='blocked',
                                 cls='blocked:one-side',
                                 x86=(rx['out'] if rx else '-'), a64=(ra['out'] if ra else '-'),
                                 note=('x86 blocked: %s' % rx_err) if rx is None else ('a64 blocked: %s' % ra_err),
                                 libm=''))
            else:
                cls = classify(rx['out'], ra['out'])
                # eval / parse 还要比"消耗的 token 数"这类副信号
                extra_same = True
                if f['kind'] in ('eval',):
                    extra_same = rx['extra'] == ra['extra']
                if not extra_same and cls == 'bit-identical':
                    cls = 'DIFF:token-count'
                ok = (cls == 'bit-identical')
                n_ok += 1 if ok else 0
                n_bad += 0 if ok else 1
                if 'plausible' in rx and not (rx['plausible'] and ra['plausible']):
                    n_blk += 1
                    rows.append(dict(fid=fid, kind=f['kind'], cid=cid, status='blocked',
                                     cls='blocked:sret-not-plausible', x86=rx['out'], a64=ra['out'],
                                     note='sret 结构体读不出可信 String（字段语义未知）', libm=''))
                    n_ok -= 0
                else:
                    rows.append(dict(fid=fid, kind=f['kind'], cid=cid,
                                     status='bit-identical' if ok else 'DIFF', cls=cls,
                                     x86=rx['out'], a64=ra['out'],
                                     note='%s | %s' % (rx['extra'], ra['extra']),
                                     libm=','.join(sorted(set((rx.get('libm') or []) + (ra.get('libm') or [])))) or '-'))
        func_status[fid] = dict(ok=n_ok, bad=n_bad, blocked=n_blk,
                                libm=sorted(libm_hit), total=len(cases))
        sys.stderr.write('%s %-28s ok=%d diff=%d blocked=%d libm=%s\n'
                         % (fid, f['name'][:28], n_ok, n_bad, n_blk, sorted(libm_hit)))

    if not a.no_write:
        probe = run_libm_probe(funcs, inputs) if a.libm_probe else None
        ctl = do_controls(quiet=True)
        write_tsv(bins_raw, funcs, rows, func_status, libm_probe=probe, ctl_lines=ctl)
        for line in ctl:
            print(line)
    bad = [r for r in rows if r['status'] == 'DIFF']
    print('合计 %d 组：逐位相同 %d / 不同 %d / blocked %d'
          % (len(rows), sum(1 for r in rows if r['status'] == 'bit-identical'),
             len(bad), sum(1 for r in rows if r['status'] == 'blocked')))
    for r in bad:
        print('  DIFF %s/%s\n    x86=%s\n    a64=%s' % (r['fid'], r['cid'], r['x86'][:120], r['a64'][:120]))
    for r in rows:
        if r['status'] == 'blocked':
            print('  BLOCKED %s/%s  %s' % (r['fid'], r['cid'], r['note']))


def write_tsv(bins_raw, funcs, rows, func_status, libm_probe=None, ctl_lines=None):
    L = []
    L.append('# 由 tools/crossarch_funcs.py 生成，不要手改（重跑会覆盖）。')
    L.append('# 输入 = fixtures/crossarch_plan.txt（**预注册件**）sha256 %s' % ct.plan_sha256())
    L.append('# 执行体：x86-64 = analysis/gwemu.py（+ 真 glibc libm）；arm64 = tools/crossarch_emu.py')
    L.append('#         （unicorn ARM64 跑 Android 2.0.60 的真机器码，入口来自该 .so 未 strip 的 .symtab）')
    L.append('# 证据等级：**逐位**（在模拟器下执行真机器码 + 逐位比较；不是 cargo test）')
    L.append('# 判据：bit-identical；差 1 ulp = 不同；跑不起来 = blocked（绝不记成相同）')
    L.append('# 输出列里超过 160 字符的（整条点数组）写成 `n=点数 sha256=前16位 head=首项 tail=末项`：'
             '长度+内容摘要相同 ⇔ 逐字节相同，判据没松，只是不把几十万字符塞进 fixture。')
    L.append('#')
    L.append('# 列：func_id / kind / case_id / status / cls / x86_out / a64_out / note / libm_syms')
    L.append('# cls：bit-identical / DIFF:value（真数值差异）/ DIFF:nan-only（两边都是 NaN，'
             '只有符号或载荷不同 —— IEEE-754 不规定 NaN 的位模式）/ blocked:*')
    for f in funcs:
        if f['id'] not in func_status:
            continue
        for r in rows:
            if r['fid'] != f['id']:
                continue
            L.append('\t'.join([r['fid'], r['kind'], r['cid'], r['status'], r['cls'],
                                compact(r['x86']), compact(r['a64']), r['note'],
                                r['libm'] or '-']))
    L.append('#')
    L.append('# --- 每函数汇总 ---')
    L.append('# func_id\tname\tkind\tcases\tbit-identical\tDIFF:value\tDIFF:nan-only\tblocked\tlibm_syms')
    for f in funcs:
        if f['id'] not in func_status:
            continue
        st = func_status[f['id']]
        mine = [r for r in rows if r['fid'] == f['id']]
        nv = sum(1 for r in mine if r['cls'].startswith('DIFF') and 'nan-only' not in r['cls'])
        nn = sum(1 for r in mine if 'nan-only' in r['cls'])
        L.append('\t'.join([f['id'], f['name'], f['kind'], str(st['total']),
                            str(st['ok']), str(nv), str(nn), str(st['blocked']),
                            (','.join(st['libm']) if st['libm'] else '-')]))
    if libm_probe:
        L.append('#')
        L.append('# --- libm 风险量化（**只把 arm64 一侧**的超越函数 ±1 ulp，模拟 bionic≠glibc）---')
        L.append('# 基线 = 两路都用 glibc libm。探针 = a64 一侧 sin/cos/tan/exp/log/log10 的结果整体 ±1 ulp，')
        L.append('# x86 侧保持 glibc ⇒ "两路差多少" 就是"bionic 与 glibc 差 1 ulp"时的漂移上界。')
        L.append('# func_id\tcase_id\t基线(两路)\ta64+1ulp vs x86\ta64-1ulp vs x86\t对 libm 敏感?')
        for r in libm_probe:
            L.append('\t'.join([r['fid'], r['cid'], compact(r['base']), compact(r['plus1']),
                                compact(r['minus1']), r['sensitive']]))
        sens = [r for r in libm_probe if r['sensitive'] == 'YES']
        L.append('# 对 libm 1 ulp 漂移敏感：%d / %d 组' % (len(sens), len(libm_probe)))
        for fid in sorted(set(r['fid'] for r in libm_probe)):
            tot = [r for r in libm_probe if r['fid'] == fid]
            ss = [r for r in tot if r['sensitive'] == 'YES']
            L.append('#   %s: %d/%d 敏感' % (fid, len(ss), len(tot)))
    L.append('#')
    L.append('# --- 对照（AGENTS.md 合并纪律 A.4）---')
    L.append('# C4 x86 同输入跑两遍 → 100%% 相同（执行体确定性）；')
    L.append('# C5 输入一端改 1 ulp → 必须报不同（比较器不是空判）；')
    L.append('# C6 把 A 拷贝里 Vector2D::length 的 fsqrt 改成 fneg（4 字节）→ 采样器用例必须成批报差异')
    L.append('#    （证明 arm64 这一路跑的是**真机器码**，不是"照符号名假装算了一遍"）。')
    for line in (ctl_lines or []):
        L.append('#   ' + line)
    open(OUT, 'w', encoding='utf-8').write('\n'.join(L) + '\n')
    print('写 %s' % OUT)


# --------------------------------------------------------------------------- libm 风险量化
def ulp_span(a_hex, b_hex):
    """两个 f64 位模式差多少 ulp（同号按整数差；跨零/NaN 就按位差给个大数）。"""
    ia = int(a_hex, 16)
    ib = int(b_hex, 16)
    sa = ia >> 63
    sb = ib >> 63
    if sa != sb:
        return None
    return abs(ia - ib)


def describe_divergence(xref, aalt, big):
    """`aalt` 与 `xref` 的差距（点数组按分量逐个比）——返回人类可读的一行。"""
    if xref == aalt:
        return 'identical'
    px, pa = xref.split(';'), aalt.split(';')
    if len(px) != len(pa):
        return 'point-count %d vs %d' % (len(px), len(pa))
    ndiff = 0
    worst = 0
    for u, v in zip(px, pa):
        fu, fv = u.split(','), v.split(',')
        for a, b in zip(fu, fv):
            if a == b:
                continue
            ndiff += 1
            sp = ulp_span(a, b)
            if sp is not None and sp < 1 << 30:
                worst = max(worst, sp)
            else:
                worst = None
    if ndiff == 0:
        return 'identical'
    return ('%d/%d 个分量差' % (ndiff, sum(len(p.split(',')) for p in px))) +            ('，最大 %d ulp' % worst if worst else '，含不可比的位模式')


def run_libm_probe(funcs, inputs):
    """**这是本轮最重要的一条边界量化**：docs/19 §6.1 只说"不同 libm 的 ULP 差异不可控"，
    本机又拿不到 bionic 的 libm 二进制（NDK sysroot 的 `libm.so` 只是链接桩、不含代码）。
    那就不假装测过，改成把风险**量出来**：

      基线：两路都用 glibc libm（两路相同的那些用例，证明的是"代码路径 + 算术"同）；
      探针：**只把 arm64 一侧**的每一个超越函数结果整体 ±1 ulp（模拟"bionic 与 glibc
            差 1 ulp"这个最坏情形），x86 一侧保持 glibc —— 然后量"两路输出差多少"。

    读到 `identical` ⇒ **就算 bionic 的 sin/cos/exp/ln/pow 与 glibc 差 1 ulp，这条链
    在这组输入上的输出仍然逐位相同**（结论对 libm 不敏感，是强结论）；
    读到 `…个分量差，最大 k ulp` ⇒ 结论降级成"在 glibc libm 下相同"，k 就是漂移上界。
    """
    x86 = ce.X86()
    a64 = ce.A64()
    a64p = ce.A64(libm_impl=ce.libm_perturb(+1))
    a64m = ce.A64(libm_impl=ce.libm_perturb(-1))
    out = []
    for f in funcs:
        for row in inputs.get(f['id'], []):
            cid = row[0]
            try:
                ref_x = run_case_x86(x86, f['kind'], row)['out']
                ref_a = run_case_a64(a64, f['kind'], row)['out']
                dp = describe_divergence(ref_x, run_case_a64(a64p, f['kind'], row)['out'], 1)
                dm = describe_divergence(ref_x, run_case_a64(a64m, f['kind'], row)['out'], -1)
                base = describe_divergence(ref_x, ref_a, 0)
                sensitive = 'no' if (dp == 'identical' and dm == 'identical') else 'YES'
                out.append(dict(fid=f['id'], cid=cid, base=base, plus1=dp, minus1=dm,
                                sensitive=sensitive))
            except Exception as e:
                out.append(dict(fid=f['id'], cid=cid, base='ERR:%s' % e, plus1='-', minus1='-',
                                sensitive='ERR'))
    return out


# --------------------------------------------------------------------------- 对照
def do_controls(quiet=False):
    res = []
    def emit(line):
        res.append(line)
        if not quiet:
            print(line)
    bins_raw, tables, funcs, ctrls = ct.parse_plan()
    inputs = load_inputs()
    x86 = ce.X86()
    a64 = ce.A64()

    # C4：x86 同输入跑两遍
    f, row = [x for x in funcs if x['id'] == 'F1'][0], inputs['F1'][0]
    r1 = run_case_x86(x86, f['kind'], row)['out']
    r2 = run_case_x86(x86, f['kind'], row)['out']
    emit('C4  x86 同输入两遍：%s（%s）' % ('OK' if r1 == r2 else 'FAIL', '100% 相同' if r1 == r2 else 'DIFF'))

    # C5：把输入的一端改 1 ulp，比较器必须报不同（证明比较不是空判）
    f, row = [x for x in funcs if x['id'] == 'F6'][0], list(inputs['F6'][3])
    p1 = run_case_x86(x86, f['kind'], row)['out']
    detail = []
    for k, name in ((2, 'x0'), (3, 'y0'), (4, 'step')):
        r2 = list(row)
        r2[k] = '%016x' % (int(row[k], 16) + 1)
        p2 = run_case_x86(x86, f['kind'], r2)['out']
        detail.append('%s+1ulp=%s' % (name, 'DIFF' if p1 != p2 else '同'))
        if name != 'step' and p1 == p2:
            emit('C5  FAIL：%s +1 ulp 竟然没让输出变（比较器或用例有问题）' % name)
            return res
    emit('C5  逐项 +1 ulp：%s  OK（x0/y0 必须变；step 是否变取决于用例灵敏度，如实记）'
         % ', '.join(detail))

    # C6：把 A 拷贝里 Vector2D::length 的 fsqrt 改成 fneg（4 字节），采样器用例必须成批报差异
    tmpdir = os.path.join(MAIN, '.agent-work', 'cg', 'wt-t9', '.agent-work', 'ctrl')
    os.makedirs(tmpdir, exist_ok=True)
    apath = os.path.join(tmpdir, 'libgraphwarrust.fneg.so')
    shutil.copyfile(ce.SO_ANDROID, apath)
    # 找 Vector2D<f64>::length 里的 fsqrt d0,d0（编码 0x1E61C000），改成 fneg d0,d0（0x1E614000）
    from capstone import Cs, CS_ARCH_ARM64, CS_MODE_ARM
    a64ref = ce.A64()
    body = a64ref.data
    start = 0x7AC7B4
    off = None
    for s in a64ref.phdrs:
        if s['va'] <= start < s['va'] + s['filesz']:
            off = s['off'] + (start - s['va'])
            break
    md = Cs(CS_ARCH_ARM64, CS_MODE_ARM)
    hit = None
    for ins in md.disasm(body[off:off + 200], start):
        if ins.mnemonic == 'fsqrt' and ins.op_str.replace(' ', '') == 'd0,d0':
            hit = ins.address
            break
    if hit is None:
        emit('C6  FAIL：Vector2D::length 里没找到 fsqrt d0,d0')
        return res
    off2 = None
    for s in a64ref.phdrs:
        if s['va'] <= hit < s['va'] + s['filesz']:
            off2 = s['off'] + (hit - s['va'])
            break
    enc, = struct.unpack_from('<I', body, off2)
    with open(apath, 'r+b') as fh:
        fh.seek(off2)
        fh.write(struct.pack('<I', 0x1E614000))
    mut = ce.A64(so=apath)
    f, rows = [x for x in funcs if x['id'] == 'F6'][0], inputs['F6']
    diffs = 0
    for row in rows:
        try:
            base = run_case_a64(a64ref, f['kind'], row)['out']
            alt = run_case_a64(mut, f['kind'], row)['out']
            if base != alt:
                diffs += 1
        except Exception:
            pass
    emit('C6  %s@0x%x fsqrt→fneg（原编码 0x%08x）：%d/%d 个采样器用例变了  %s'
         % ('Vector2D::length', hit, enc, diffs, len(rows), 'OK' if diffs > 0 else 'FAIL'))
    return res


if __name__ == '__main__':
    main()
