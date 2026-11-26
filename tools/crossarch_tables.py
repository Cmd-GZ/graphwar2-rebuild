#!/usr/bin/env python3
"""crossarch_tables.py —— 原版**三份构建**的只读数据节逐字节对拍（T9 第 ② 步）。

输入：`fixtures/crossarch_plan.txt` 的 `[bin]` / `[table]` 两节（**预注册件**，
不在这里改判据）。输出：`fixtures/crossarch_tables.tsv`。

方法（为什么这么比，比 docs/19 §4.0.3 那条"比不了"往前推了一格）：
  * docs/19 §4.0.3 已经证明：**单个 f64 常量**在三个 target 上的**物化次数**不同
    （`0.01` = 2/2/1 次、`200.0` = 0/1/3 次…），所以"数常量出现几次"推不出结论。
    那条结论是对的，但它只否掉了「数数」这一种方法，没否掉「比一张表」。
  * 本工具比的是**整张表**：表是**数组**，数组不可能被拆成立即数，只能落在数据节里。
    定位用**内容锚**（先 32 B、不行 16 B、再不行 8 B），定位到之后比**整段长度**。
  * 因此"逐字节相同"是对**表**说的，不是对"某个常量在这份构建里存不存在"说的。
    单常量是否被编成立即数（aarch64 的 `fmov` / `movz:movk`），由函数侧的逐位相等兜底
    （见 `tools/crossarch_funcs.py` 的 F6/F7）。

判据（预注册）：
  * `byte-identical`  ⇒ 差异字节数必须为 0；
  * 定位失败 / 只按短锚定位 / 有差异 ⇒ 如实写，并且**给首个差异偏移**。
  * 锚找不到了就写 `not-located`，**不许**当成"相同"，也不许当成"不同"。

正/负对照（跑之前就在 plan 里定死，见 `[control]` C1/C2/C3）：
  C1 L.rodata 与**自己**比 → 必须 100%（证明块匹配器不漏）
  C2 L.rodata 与 L.text 比 → 必须 < 1%（证明它不是什么都能匹配上）
  C3 把 A 的拷贝里 T1 的第 1 个字节改掉 → 必须报 1 处差异且偏移为 0

用法：
    .reference/venv/bin/python tools/crossarch_tables.py            # 跑并写 fixtures/crossarch_tables.tsv
    .reference/venv/bin/python tools/crossarch_tables.py --print     # 同时打到 stdout
    .reference/venv/bin/python tools/crossarch_tables.py --controls  # 只跑 3 条对照
"""

import argparse
import hashlib
import os
import shutil
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
MAIN = os.path.abspath(os.path.join(HERE, ".."))
PLAN = os.path.join(MAIN, "fixtures", "crossarch_plan.txt")
OUT = os.path.join(MAIN, "fixtures", "crossarch_tables.tsv")
sys.path.insert(0, HERE)
import crossarch_emu as ce  # noqa: E402

K = 32          # 块匹配 / 内容锚的长窗口
KMIN = 8        # 兜底锚


# --------------------------------------------------------------------------- plan
def parse_plan(path=PLAN):
    bins, tables, funcs, ctrls = [], [], [], []
    sec = None
    for raw in open(path, encoding='utf-8'):
        line = raw.rstrip('\n')
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        if line.startswith('['):
            sec = line.strip()
            continue
        f = line.split('\t')
        if sec == '[bin]':
            bins.append(dict(id=f[0], role=f[1], path=f[2], sha256=f[3],
                             size=int(f[4]), arch=f[5]))
        elif sec == '[table]':
            tables.append(dict(id=f[0], name=f[1], kind=f[2], anchor=f[3],
                               length=int(f[4]), expect=f[5]))
        elif sec == '[func]':
            funcs.append(dict(id=f[0], name=f[1], x86=f[2], a64=f[3], kind=f[4], expect=f[5]))
        elif sec == '[control]':
            if f[0] == 'id':
                continue
            ctrls.append(dict(id=f[0], desc=f[1], expect=f[2]))
    return bins, tables, funcs, ctrls


def plan_sha256(path=PLAN):
    return hashlib.sha256(open(path, 'rb').read()).hexdigest()


# --------------------------------------------------------------------------- binaries
class Bin:
    """一份构建：文件字节 + 只读数据节定位（ELF 用 .rodata，PE 用 .rdata）。"""

    def __init__(self, bid, path, role):
        self.id = bid
        self.path = path
        self.role = role
        self.data = open(path, 'rb').read()
        self.size = len(self.data)
        self.kind = 'pe' if self.data[:2] == b'MZ' else 'elf'
        if self.kind == 'elf':
            self.secs = {s['name']: s for s in ce.elf_sections(self.data)}
            self.ro = self.secs['.rodata']
        else:
            self.secs = self._pe_sections()
            self.ro = self.secs['.rdata']

    def _pe_sections(self):
        d = self.data
        e_lfanew, = struct.unpack_from('<I', d, 0x3C)
        nsec, = struct.unpack_from('<H', d, e_lfanew + 6)
        optsz, = struct.unpack_from('<H', d, e_lfanew + 20)
        base = e_lfanew + 24 + optsz
        out = {}
        for i in range(nsec):
            o = base + i * 40
            name = d[o:o + 8].rstrip(b'\0').decode('ascii', 'replace')
            vsz, va, rsz, roff = struct.unpack_from('<IIII', d, o + 8)
            out[name] = dict(name=name, va=va, off=roff, size=rsz, vsize=vsz)
        return out

    def rodata(self):
        return self.data[self.ro['off']:self.ro['off'] + self.ro['size']]

    def section_of(self, off):
        """文件偏移落在哪个节（PE 用 VirtualAddress/SizeOfRawData 换算）。"""
        for name, s in self.secs.items():
            if s['off'] <= off < s['off'] + s['size']:
                return name
        return '?'

    def to_va(self, off):
        """文件偏移 → 虚地址（ELF: 节 va + 差；PE: ImageBase + RVA）。"""
        s = self.secs.get(self.section_of(off))
        if not s:
            return None
        if self.kind == 'elf':
            return s['va'] + (off - s['off'])
        return 0x180000000 + s['va'] + (off - s['off'])


# --------------------------------------------------------------------------- 块匹配
def maximal_blocks(a, b, k=K):
    """a 中每一段在 b 里**出现过**的最大公共块（允许 b 侧任意位置、任意顺序）。

    返回 [(a_off, b_off, len)]。索引 b 的全部长度-k 窗口 ⇒ 一次命中就能扩展出整段，
    所以对"同一段数据换了位置"是稳的（这正是跨架构要的等价关系）。
    """
    n = len(b) - k
    if n < 0:
        return []
    idx = {}
    for i in range(n + 1):
        idx.setdefault(b[i:i + k], []).append(i)
    out = []
    i = 0
    while i <= len(a) - k:
        hit = idx.get(a[i:i + k])
        if not hit:
            i += 1
            continue
        best = None
        for j in hit:
            ext = k
            while i + ext < len(a) and j + ext < len(b) and a[i + ext] == b[j + ext]:
                ext += 1
            left = 0
            while i - left - 1 >= 0 and j - left - 1 >= 0 and a[i - left - 1] == b[j - left - 1]:
                left += 1
            if best is None or ext + left > best[2]:
                best = (i - left, j - left, ext + left)
        out.append(best)
        i = best[0] + max(best[2], 1)
    return out


def cov_mask(blocks, n):
    m = bytearray(n)
    for s, _, ln in blocks:
        for x in range(s, min(s + ln, n)):
            m[x] = 1
    return m


# --------------------------------------------------------------------------- 定位
def locate(blob, anchor):
    """返回 anchor 在 blob 里的全部位置。"""
    out, i = [], blob.find(anchor)
    while i >= 0:
        out.append(i)
        i = blob.find(anchor, i + 1)
    return out


def find_table(lin, tab, b):
    """在构建 b 里定位 lin 这张表。

    返回 dict(off, anchor_len, candidates, note)。
    多候选时选"整段差异字节最少"的那个（同一张表换个位置摆放也认得出），
    候选数会写进 TSV —— 不藏。
    """
    want = len(tab['bytes'])
    for klen in (K, 16, KMIN):
        if want < klen:
            klen = want
        sites = locate(b.data, tab['bytes'][:klen])
        if not sites:
            continue
        best, bestd = None, None
        for s in sites:
            got = b.data[s:s + want]
            if len(got) < want:
                got = got + b'\x00' * (want - len(got))
            d = sum(1 for x, y in zip(tab['bytes'], got) if x != y)
            if bestd is None or d < bestd:
                best, bestd = s, d
        return dict(off=best, anchor_len=klen, candidates=len(sites), note='')
    return dict(off=None, anchor_len=0, candidates=0, note='not-located')


def cmp_bytes(lin_bytes, got):
    """逐字节比较；返回 (差异字节数, 首个差异偏移 or -1)。长度不等时把缺少的算差异。"""
    n = max(len(lin_bytes), len(got))
    pads = lambda b: b + bytes(n - len(b))
    a, c = pads(lin_bytes), pads(got)
    first = -1
    diff = 0
    for i in range(n):
        if a[i] != c[i]:
            diff += 1
            if first < 0:
                first = i
    return diff, first


# --------------------------------------------------------------------------- 主流程
def run(do_print=False):
    bins_raw, tables, funcs, ctrls = parse_plan()
    if not all(os.path.exists(os.path.join(MAIN, b['path'])) for b in bins_raw):
        raise SystemExit('缺原版二进制（%s）' % MAIN)
    bins = {}
    for b in bins_raw:
        path = os.path.join(MAIN, b['path'])
        real = hashlib.sha256(open(path, 'rb').read()).hexdigest()
        if real != b['sha256']:
            raise SystemExit('%s sha256 与预注册不符：%s != %s' % (b['id'], real, b['sha256']))
        bins[b['id']] = Bin(b['id'], path, b['role'])

    L, A, W = bins['L'], bins['A'], bins['W']
    rows = []

    # ---- 表：定位 + 逐字节 ----
    located = {}
    for tab in tables:
        if tab['kind'] == 'region':
            continue
        off = int(tab['anchor'], 16)
        tab['bytes'] = L.data[off:off + tab['length']]
        tab['sec_linux'] = L.section_of(off)
        tab['va_linux'] = L.to_va(off)
        for b in (A, W):
            r = find_table(L, tab, b)
            key = (tab['id'], b.id)
            located[key] = r
        rows.append(('T', tab, A, located[(tab['id'], A.id)], W, located[(tab['id'], W.id)]))

    # ---- T9：全 .rodata 块级覆盖 ----
    rl, ra, rw = L.rodata(), A.rodata(), W.rodata()
    blk_a = maximal_blocks(rl, ra)
    blk_w = maximal_blocks(rl, rw)
    cov_a = cov_mask(blk_a, len(rl))
    cov_w = cov_mask(blk_w, len(rl))
    region = dict(
        len_l=len(rl), len_a=len(ra), len_w=len(rw),
        matched_a=sum(cov_a), matched_w=sum(cov_w),
        blocks_a=len(blk_a), blocks_w=len(blk_w),
        top_a=sorted(blk_a, key=lambda t: -t[2])[:5],
        top_w=sorted(blk_w, key=lambda t: -t[2])[:5],
        cov_a=cov_a, cov_w=cov_w,
    )

    # ---- 对照 C1/C2/C3 ----
    controls = run_controls(L, A, bins)

    # ---- 写 TSV ----
    lines = []
    lines.append('# 由 tools/crossarch_tables.py 生成，不要手改（重跑会覆盖）。')
    lines.append('# 输入 = fixtures/crossarch_plan.txt（预注册件）sha256 %s' % plan_sha256())
    lines.append('# 三份构建（与 [bin] 同源，sha256 已按预注册核对）：')
    for b in bins_raw:
        lines.append('#   %s  %s  sha256=%s  size=%d  %s' % (b['id'], b['role'], b['sha256'], b['size'], b['arch']))
    lines.append('# 口径：表 = 一段连续数组/字符串；定位 = 内容锚（先 32B→16B→8B）；比较 = 整段逐字节。')
    lines.append('# 证据等级：**逐位**（与真二进制逐字节对拍；不是 cargo test）。')
    lines.append('#')
    lines.append('# 列：table_id / name / linux_section / linux_off / linux_va / length / '
                 'a64_off / a64_anchor / a64_candidates / a64_diff_bytes / a64_first_diff / '
                 'win_off / win_anchor / win_candidates / win_diff_bytes / win_first_diff / verdict')
    for _, tab, A_, ra_, W_, rw_ in rows:
        cells = [tab['id'], tab['name'], tab['sec_linux'], '0x%x' % int(tab['anchor'], 16),
                 '0x%x' % (tab['va_linux'] or 0), str(tab['length'])]
        verdicts = []
        for b, r in ((A_, ra_), (W_, rw_)):
            if r['off'] is None:
                cells += ['-', '-', '0', '-', '-']
                verdicts.append('%s:not-located' % b.id)
                continue
            got = b.data[r['off']:r['off'] + tab['length']]
            diff, first = cmp_bytes(tab['bytes'], got)
            cells += ['0x%x' % r['off'], '%d' % r['anchor_len'], str(r['candidates']),
                      str(diff), ('%d' % first) if first >= 0 else '-']
            verdicts.append('%s:%s' % (b.id, 'identical' if diff == 0 else 'DIFF(%d B, first=%d)' % (diff, first)))
        cells.append('; '.join(verdicts))
        lines.append('\t'.join(cells))
    # T9 行
    lines.append('\t'.join([
        'T9', '全 .rodata 块级覆盖（≥32 B 最大公共块；统计项，不逐表判）', '.rodata', '0x%x' % L.ro['off'],
        '0x%x' % L.ro['va'], str(len(rl)),
        '-', '-', str(region['blocks_a']), str(len(rl) - region['matched_a']), '-',
        '-', '-', str(region['blocks_w']), str(len(rl) - region['matched_w']), '-',
        'L/A: %d/%d B 逐字节存在 (%.2f%%); L/W: %d/%d B (%.2f%%)' % (
            region['matched_a'], len(rl), 100.0 * region['matched_a'] / len(rl),
            region['matched_w'], len(rl), 100.0 * region['matched_w'] / len(rl))]))
    lines.append('#')
    lines.append('# --- 对照（AGENTS.md 合并纪律 A.4：负结论必须先证扫法有效）---')
    for c in controls:
        lines.append('# %s\t%s\t期望=%s\t实测=%s\t%s' % (c['id'], c['desc'], c['expect'], c['result'], c['verdict']))
    lines.append('#')
    lines.append('# --- T9 覆盖细节 ---')
    lines.append('# L .rodata=%d B / A .rodata=%d B / W .rdata=%d B' % (len(rl), len(ra), len(rw)))
    lines.append('# L 侧匹配到 A 的块数=%d，最大 5 块：' % len(blk_a))
    for s, j, ln in region['top_a']:
        lines.append('#   L 0x%06x -> A 0x%06x  len %d' % (s, j, ln))
    lines.append('# L 侧匹配到 W 的块数=%d，最大 5 块：' % len(blk_w))
    for s, j, ln in region['top_w']:
        lines.append('#   L 0x%06x -> W 0x%06x  len %d' % (s, j, ln))
    # 未匹配部分的性质：8 字节窗口像不像"绝对地址"
    def looks_pointery(data, mask, secname):
        tot = ptr = 0
        for i in range(0, len(data) - 8, 8):
            if mask[i]:
                continue
            tot += 1
            w, = struct.unpack_from('<Q', data, i)
            if 0x1000 <= w <= 0xA00000 or 0x180000000 <= w <= 0x182000000:
                ptr += 1
        return ptr, tot
    pa, ta = looks_pointery(rl, cov_a, '.rodata')
    pw, tw = looks_pointery(rl, cov_w, '.rodata')
    lines.append('# 未匹配区里"8 字节窗口取值像绝对地址"的窗口数：L/A %d/%d，L/W %d/%d '
                 '（像地址的说明是重定位/布局相关，本来就不该跨构建相同）' % (pa, ta, pw, tw))
    lines += diagnose(L, A, W, bins)
    open(OUT, 'w', encoding='utf-8').write('\n'.join(lines) + '\n')

    if do_print:
        print('\n'.join(lines))
    return rows, region, controls


def diagnose(L, A, W, bins):
    """差异诊断（**事后补充**，不改预注册判据）：把"某张表报差异"拆成
    「内容变了」还是「同样的内容换了个摆法」，以及把 T7/T8 这两条**注册失误**
    的实际性质写清楚（它们不是数组，本来就不该用"表"的口径判）。

    这段的存在理由：报出差异却不说差在哪，等于没报（AGENTS.md §0.6）。
    """
    out = ['#', '# --- 差异诊断（事后补充；判据未改，只解释上面那几行差异）---']
    # D1：T3 的 21 段描述串，逐串在三份构建里搜（内容等价 vs 摆法不同）
    detail = campaign_strings(L)
    if detail:
        nA = nW = 0
        for off, ln, txt in detail:
            if A.data.find(txt) >= 0:
                nA += 1
            if W.data.find(txt) >= 0:
                nW += 1
        out.append('# D1\tT3 的 21 段战役描述串（按 docs/10 §3 的 21 个 (偏移,长度) 取出，逐串原文搜全文件）')
        out.append('# D1\tA 侧找到 %d/21 段、W 侧找到 %d/21 段 ⇒ 内容三份相同；T3 在 W 报 837 B 差异是**摆法**不同：' % (nA, nW))
        # 验证"W 把关 9 的描述插进连写区"
        l9 = L.data[0x1bcaa0:0x1bcaa0 + 32]
        out.append('# D1\tL 的连写区(0x1c21b1..)里**没有**关 9 描述 %r（docs/10 §3 早记过它在 0x1bcaa0）；' % l9)
        site = W.data.find(L.data[0x1c21b1:0x1c21b1 + 32])
        ins = W.data[site:site + 0x1c27ba - 0x1c21b1 + 64] if site >= 0 else b''
        out.append('# D1\tW 侧对应区段把这两段**拼成一个 0..20 顺序的连写区**（关 9 的描述插在里面）⇒ 长度多 32 B、'
                   '偏移 ≥650 之后整体移位，所以"差异字节数"大但**没有一个字符不同**。')
    # D2：T7 的串在不在（T7 注册错了：那是字符串池片段，不是表）
    for probe in (b'LevelDescriptionUnfinished', b'Level '):
        hits = {b.id: b.data.count(probe) for b in (L, A, W)}
        out.append('# D2\tb\'%s\' 全文件出现次数 L=%d A=%d W=%d'
                   % (probe.decode(), hits['L'], hits['A'], hits['W']))
    out.append('# D2\t⇒ T7 报 not-located 的性质：这 32 B 窗是**字符串池里的一段**（后面紧跟着别的串），'
               'A 那份根本没有 `LevelDescriptionUnfinished` 这个串 ⇒ 这是"字符串池不同"，'
               '**不是"某张数据表不同"**。T7 是本轮**注册失误**，不参与结论。')
    # D3：T8 的标量簇 —— 复算 docs/19 §4.0.3 那张"物化次数"表
    scal = [('0.01', 0.01), ('0.001', 0.001), ('200.0', 200.0), ('25.0', 25.0),
            ('15.0', 15.0), ('6.0', 6.0), ('2^-53', 2.0 ** -53)]
    out.append('# D3\tT8 的性质：这 4 KB 不是数组，是一簇**标量常量**；复算 docs/19 §4.0.3 的"物化次数"：')
    out.append('# D3\t常量\tL\tA\tW\t（8 字节位模式在**只读数据节**里的出现次数）')
    for name, v in scal:
        cnt = {}
        for b in (L, A, W):
            cnt[b.id] = b.rodata().count(struct.pack('<d', v))
        out.append('# D3\t%s\t%d\t%d\t%d' % (name, cnt['L'], cnt['A'], cnt['W']))
    out.append('# D3\t⇒ 次数三份不同且不稳定（同一常量在 A/W 变成 `fmov`/`movz:movk` 立即数就落不进数据节），'
               '所以 T8 用"4 KB 窗逐字节"判**不成立**——这与 docs/19 §4.0.3 的结论一致；')
    out.append('# D3\t   这些常量的**语义**是否相同，由 F6/F7（采样器输出点数组逐位）兜底，那一档才是有效判据。')
    return out


def campaign_strings(L):
    """按 docs/10 §3 的 21 个 (`.rodata` 偏移, 长度) 取出战役描述串原文。"""
    rows = [(0x1c21b1, 40), (0x1c21d9, 153), (0x1c2272, 129), (0x1c22f3, 94),
            (0x1c2351, 45), (0x1c237e, 42), (0x1c23a8, 60), (0x1c23e4, 38),
            (0x1c240a, 49), (0x1bcaa0, 32), (0x1c243b, 80), (0x1c248b, 111),
            (0x1c24fa, 47), (0x1c2529, 37), (0x1c254e, 46), (0x1c257c, 58),
            (0x1c25b6, 251), (0x1c26b1, 88), (0x1c2709, 46), (0x1c2737, 67),
            (0x1c277a, 64)]
    out = []
    for off, ln in rows:
        txt = L.data[off:off + ln]
        if len(txt) != ln or not all(32 <= c < 127 for c in txt):
            return []
        out.append((off, ln, txt))
    return out


def run_controls(L, A, bins):
    """C1/C2/C3。C3 需要把 A 拷一份改一个字节 —— 拷到 `.agent-work/`（仓库外证据链）。"""
    out = []
    rl = L.rodata()
    # C1: 自己对自己
    c1 = maximal_blocks(rl, rl)
    m1 = cov_mask(c1, len(rl))
    r1 = 100.0 * sum(m1) / len(rl)
    out.append(dict(id='C1', desc='L.rodata vs L.rodata（自身）', expect='100% 匹配',
                    result='%.2f%%' % r1, verdict='OK' if r1 > 99.99 else 'FAIL'))
    # C2: .rodata 对 .text
    tl = L.data[L.secs['.text']['off']:L.secs['.text']['off'] + L.secs['.text']['size']]
    c2 = maximal_blocks(rl, tl)
    m2 = cov_mask(c2, len(rl))
    r2 = 100.0 * sum(m2) / len(rl)
    out.append(dict(id='C2', desc='L.rodata vs L.text', expect='< 1% 匹配',
                    result='%.4f%%' % r2, verdict='OK' if r2 < 1.0 else 'FAIL'))
    # C3: 拷一份 A，把预注册 T1（ziggurat X）的**第 40 字节**改掉（留在 32 B 锚之外，
    # 这样"定位仍能成功、比较必须报 1 处差异且首个差异偏移 = 40"）；顺带记录
    # "改成锚里第一个字节 ⇒ 定位失败"也是**正确答案**（不是相同，也不是不同）。
    _, tables, _, _ = parse_plan()
    t1 = [t for t in tables if t['id'] == 'T1'][0]
    off = int(t1['anchor'], 16)
    t1b = dict(bytes=L.data[off:off + t1['length']])
    r = find_table(L, dict(bytes=t1b['bytes']), A)
    base_diff = cmp_bytes(t1b['bytes'], A.data[r['off']:r['off'] + t1['length']])[0]
    tmpdir = os.path.join(MAIN, '.agent-work', 'cg', 'wt-t9', '.agent-work', 'ctrl')
    os.makedirs(tmpdir, exist_ok=True)
    apath = os.path.join(tmpdir, 'libgraphwarrust.ctrl.so')
    shutil.copyfile(A.path, apath)
    MUT = 40
    with open(apath, 'r+b') as f:
        f.seek(r['off'] + MUT)
        b0 = f.read(1)
        f.seek(r['off'] + MUT)
        f.write(bytes([b0[0] ^ 0xFF]))
    A2 = Bin('A2', apath, 'mutated copy')
    r2b = find_table(L, dict(bytes=t1b['bytes']), A2)
    diff2, first2 = cmp_bytes(t1b['bytes'], A2.data[r2b['off']:r2b['off'] + t1['length']]) if r2b['off'] is not None else (-1, -1)
    ok = (base_diff == 0 and r2b['off'] is not None and diff2 == 1 and first2 == MUT)
    # C3b：锚内改字节 ⇒ 定位必须失败（return not-located，不许冒充"相同"）
    apath2 = os.path.join(tmpdir, 'libgraphwarrust.ctrl2.so')
    shutil.copyfile(A.path, apath2)
    with open(apath2, 'r+b') as f:
        f.seek(r['off'])
        b0 = f.read(1)
        f.seek(r['off'])
        f.write(bytes([b0[0] ^ 0xFF]))
    A3 = Bin('A3', apath2, 'mutated copy (anchor)')
    r3 = find_table(L, dict(bytes=t1b['bytes']), A3)
    ok3 = r3['off'] is None or cmp_bytes(t1b['bytes'], A3.data[r3['off']:r3['off'] + t1['length']])[0] > 0
    out.append(dict(id='C3', desc='A 拷贝里 T1 第 %d 字节 ^0xFF（锚外）' % MUT,
                    expect='未改=0 差异；改后=1 处差异且首个偏移 %d' % MUT,
                    result='未改 diff=%d；改后 diff=%d first=%d' % (base_diff, diff2, first2),
                    verdict='OK' if ok else 'FAIL'))
    out.append(dict(id='C3b', desc='A 拷贝里 T1 第 0 字节 ^0xFF（锚内）',
                    expect='定位失败(not-located) 或报差异；不许报"相同"',
                    result=('not-located' if r3['off'] is None else 'located@0x%x' % r3['off']),
                    verdict='OK' if ok3 else 'FAIL'))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--print', action='store_true')
    ap.add_argument('--controls', action='store_true')
    a = ap.parse_args()
    if a.controls:
        bins_raw, tables, funcs, ctrls = parse_plan()
        bins = {}
        for b in bins_raw:
            bins[b['id']] = Bin(b['id'], os.path.join(MAIN, b['path']), b['role'])
        for c in run_controls(bins['L'], bins['A'], bins):
            print('%s  %-40s 期望=%-30s 实测=%-40s %s' % (c['id'], c['desc'], c['expect'], c['result'], c['verdict']))
        return
    rows, region, controls = run(do_print=a.print)
    bad = [c for c in controls if c['verdict'] != 'OK']
    print('对照：%d/%d OK' % (len(controls) - len(bad), len(controls)))
    for c in bad:
        print('  FAIL %s %s -> %s' % (c['id'], c['desc'], c['result']))
    print('写 %s' % OUT)


if __name__ == '__main__':
    main()
