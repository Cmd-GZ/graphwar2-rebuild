#!/usr/bin/env python3
"""tools/frame_diff.py —— 三层视觉对拍（L1 尺寸/主色 → L2 6x4 分区 → L3 逐像素）。

判据**不在这里定**：门槛写死在 `fixtures/frame_diff_thresholds.txt`（预注册，94 号），
本脚本只把那张表算出来，并在阈值处**原样引用**同一组常数（`--self-check` 会核对两边是否一致）。

输入 = `tools/frame_capture.py` 的两份产物目录（各含 `frames/*.png` + `manifest.json`）。
输出 = 逐屏结论（TSV）+ 24 块明细（TSV）+ 完整 JSON（含噪声地板、差异 bbox）。

用法：

    python3 tools/frame_diff.py --orig <orig> --mine <mine> \
        --out fixtures/frame_diff_results.tsv --blocks-out fixtures/frame_diff_blocks.tsv \
        --json <工作区>/frame_diff.json [--png <工作区>/frame_diff_heat.png]

依赖：Pillow + numpy（本机系统 `python3` 有；`.reference/venv` **没有** ⇒ 别用 venv 跑）。
"""
import argparse
import json
import os
import sys

import numpy as np
from PIL import Image

# ── 预注册门槛（`fixtures/frame_diff_thresholds.txt` §3；两边必须一致）───────────────
L1_MEAN_MAX = 2.0          # 每通道全局均值 |Δ| ≤ 2.0/255
L1_TOP_SHARE_PP = 2.0      # 第一主色占比差 ≤ 2.0 个百分点
L2_BLOCK_MEAN_MAX = 2.0    # 每块每通道均值 |Δ| ≤ 2.0/255
L3_DIFF_RATIO_MAX = 0.0010  # 差异像素比例 ≤ 0.10 %
L3_MAXDIFF_MAX = 8         # 最大通道差 ≤ 8/255
GRID_COLS, GRID_ROWS = 6, 4
EXPECT_W, EXPECT_H = 1920, 1160
THRESHOLDS_SHA = '9cc9fcda764b6c3dc7036c6cd2f1c049e0a2d44ed74b15745ff68f3ec4da2873'


def load(path):
    im = Image.open(path).convert('RGB')
    return np.asarray(im).astype(np.int16)


def top_color(arr):
    """第一主色 = 像素数最多的 RGB 三元组。"""
    packed = (arr[:, :, 0].astype(np.uint32) << 16) | \
             (arr[:, :, 1].astype(np.uint32) << 8) | arr[:, :, 2].astype(np.uint32)
    vals, counts = np.unique(packed, return_counts=True)
    i = int(np.argmax(counts))
    v = int(vals[i])
    return ((v >> 16) & 255, (v >> 8) & 255, v & 255), float(counts[i]) / packed.size


def blocks(arr):
    H, W = arr.shape[:2]
    out = []
    for r in range(GRID_ROWS):
        for c in range(GRID_COLS):
            y0, y1 = r * H // GRID_ROWS, (r + 1) * H // GRID_ROWS
            x0, x1 = c * W // GRID_COLS, (c + 1) * W // GRID_COLS
            out.append((r, c, arr[y0:y1, x0:x1]))
    return out


def diff_stats(a, b):
    d = np.abs(a - b)                      # HxWx3
    dmax = d.max(axis=2)
    diff_px = int((dmax > 0).sum())
    total = dmax.size
    stats = {'diff_px': diff_px, 'total_px': total,
             'diff_ratio': diff_px / float(total),
             'max_channel_diff': int(dmax.max()) if diff_px else 0}
    if diff_px:
        ys, xs = np.nonzero(dmax > 0)
        stats['bbox'] = [int(xs.min()), int(ys.min()), int(xs.max()), int(ys.max())]
    else:
        stats['bbox'] = None
    return stats, dmax


def compare_screen(a_path, b_path, name, threshold_ok):
    a, b = load(a_path), load(b_path)
    row = {'screen': name,
           'size_orig': '%dx%d' % (a.shape[1], a.shape[0]),
           'size_mine': '%dx%d' % (b.shape[1], b.shape[0])}
    row['l1_size_ok'] = (a.shape == b.shape)
    row['l1_size_ok'] = row['l1_size_ok'] and a.shape[1] == EXPECT_W and a.shape[0] == EXPECT_H
    if a.shape != b.shape:
        row['verdict'] = '不可比（尺寸不同）'
        return row, None, None
    # ---- L1 ----
    mean_a = a.reshape(-1, 3).mean(axis=0)
    mean_b = b.reshape(-1, 3).mean(axis=0)
    dmean = np.abs(mean_a - mean_b)
    row['l1_dmean'] = [round(float(x), 3) for x in dmean]
    row['l1_dmean_max'] = round(float(dmean.max()), 3)
    tc_a, share_a = top_color(a)
    tc_b, share_b = top_color(b)
    row['l1_top_color'] = ['#%02X%02X%02X' % tc_a, '#%02X%02X%02X' % tc_b]
    row['l1_top_share_pp'] = round(abs(share_a - share_b) * 100.0, 3)
    l1_ok = (row['l1_size_ok'] and row['l1_dmean_max'] <= L1_MEAN_MAX
             and tc_a == tc_b and row['l1_top_share_pp'] <= L1_TOP_SHARE_PP)
    row['l1_ok'] = bool(l1_ok)
    # ---- L2 ----
    blk_rows = []
    worst = None
    n_over = 0
    for (r, c, A), (_, _, B) in zip(blocks(a), blocks(b)):
        bd = np.abs(A.reshape(-1, 3).mean(axis=0) - B.reshape(-1, 3).mean(axis=0))
        st, _ = diff_stats(A, B)
        rec = {'screen': name, 'row': r, 'col': c,
               'dmean_rgb': [round(float(x), 3) for x in bd],
               'dmean_max': round(float(bd.max()), 3),
               'diff_px': st['diff_px'], 'diff_ratio': round(st['diff_ratio'], 6),
               'max_channel_diff': st['max_channel_diff'],
               'bbox': st['bbox'],
               'over': bool(bd.max() > L2_BLOCK_MEAN_MAX)}
        blk_rows.append(rec)
        if rec['over']:
            n_over += 1
        if worst is None or rec['dmean_max'] > worst['dmean_max']:
            worst = rec
    row['l2_max_block_dmean'] = worst['dmean_max'] if worst else None
    row['l2_worst_block'] = '%d,%d' % (worst['row'], worst['col']) if worst else ''
    row['l2_blocks_over'] = n_over
    row['l2_ok'] = bool(n_over == 0)
    # ---- L3（只在 L1/L2 都过时才算数）----
    st, dmax = diff_stats(a, b)
    row['l3_diff_px'] = st['diff_px']
    row['l3_diff_ratio'] = round(st['diff_ratio'], 6)
    row['l3_max_channel_diff'] = st['max_channel_diff']
    row['l3_bbox'] = st['bbox']
    l3_ok = (st['diff_ratio'] <= L3_DIFF_RATIO_MAX
             and st['max_channel_diff'] <= L3_MAXDIFF_MAX)
    row['l3_ok'] = bool(l3_ok)
    row['l3_evaluated'] = bool(l1_ok and row['l2_ok'])
    # ---- 判定 ----
    if not row['l1_size_ok']:
        row['verdict'] = '不过（L1 尺寸）'
    elif not row['l1_ok']:
        row['verdict'] = '不过（L1 主色/均值）'
    elif not row['l2_ok']:
        row['verdict'] = '不过（L2，%d 块超限）' % n_over
    elif l3_ok:
        row['verdict'] = '过'
    else:
        row['verdict'] = '不过（L3）'
    if threshold_ok is False:
        row['notes'] = '门槛文件 sha256 与脚本内常数不符'
    return row, blk_rows, dmax


def noise_floor(paths):
    """同一侧同一屏的多帧两两差异像素比例（**噪声地板**，不是门槛）。"""
    arrs = [load(p) for p in paths]
    ratios = []
    for i in range(len(arrs)):
        for j in range(i + 1, len(arrs)):
            if arrs[i].shape != arrs[j].shape:
                continue
            st, _ = diff_stats(arrs[i], arrs[j])
            ratios.append(round(st['diff_ratio'], 6))
    return {'pairs': ratios,
            'max': max(ratios) if ratios else None,
            'mean': round(float(np.mean(ratios)), 6) if ratios else None}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--orig', required=True, help='原版一侧的产物目录（含 frames/ 与 manifest.json）')
    ap.add_argument('--mine', required=True, help='我方一侧的产物目录')
    ap.add_argument('--out', required=True, help='逐屏结论 TSV')
    ap.add_argument('--blocks-out', default=None, help='24 块明细 TSV')
    ap.add_argument('--json', default=None, help='完整 JSON')
    ap.add_argument('--png', default=None, help='差异热图（只给人看，不入库）')
    ap.add_argument('--screens', default='menu,prep,shot')
    ap.add_argument('--note', action='append', default=[],
                    help='写进两个 TSV 顶部的 `#` 注释行（来历/复跑命令），可给多次')
    ap.add_argument('--thresholds', default=os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        'fixtures', 'frame_diff_thresholds.txt'),
        help='预注册门槛文件（脚本会核对 sha256）')
    args = ap.parse_args(argv)

    import hashlib
    threshold_ok = None
    if os.path.exists(args.thresholds):
        with open(args.thresholds, 'rb') as f:
            sha = hashlib.sha256(f.read()).hexdigest()
        threshold_ok = (sha == THRESHOLDS_SHA)
        if not threshold_ok:
            print('[frame_diff] ⚠ 门槛文件 sha256 = %s ≠ 脚本内 %s' % (sha, THRESHOLDS_SHA),
                  file=sys.stderr)
    else:
        print('[frame_diff] ⚠ 找不到门槛文件 %s' % args.thresholds, file=sys.stderr)

    screens = [s for s in args.screens.split(',') if s]
    results, all_blocks, report = [], [], {'thresholds_sha': THRESHOLDS_SHA,
                                           'thresholds_ok': threshold_ok,
                                           'thresholds': {
                                               'l1_mean_max': L1_MEAN_MAX,
                                               'l1_top_share_pp': L1_TOP_SHARE_PP,
                                               'l2_block_mean_max': L2_BLOCK_MEAN_MAX,
                                               'l3_diff_ratio_max': L3_DIFF_RATIO_MAX,
                                               'l3_maxdiff_max': L3_MAXDIFF_MAX,
                                               'grid': [GRID_COLS, GRID_ROWS]},
                                           'screens': {}}
    for s in screens:
        fo = [os.path.join(args.orig, 'frames', '%s_%d.png' % (s, i + 1)) for i in range(3)]
        fm = [os.path.join(args.mine, 'frames', '%s_%d.png' % (s, i + 1)) for i in range(3)]
        if not (os.path.exists(fo[0]) and os.path.exists(fm[0])):
            print('[frame_diff] 跳过 %s（缺帧）' % s, file=sys.stderr)
            continue
        row, blk, dmax = compare_screen(fo[0], fm[0], s, threshold_ok)
        row['noise_floor_orig'] = noise_floor([p for p in fo if os.path.exists(p)])
        row['noise_floor_mine'] = noise_floor([p for p in fm if os.path.exists(p)])
        for k in ('noise_floor_orig', 'noise_floor_mine'):
            pk = row[k]['pairs']
            row[k.replace('noise_floor_', 'noise_max_')] = row[k]['max']
            row[k.replace('noise_floor_', 'noise_mean_')] = row[k]['mean']
            del row[k]['pairs']
            row[k]['pairs'] = pk
        results.append(row)
        if blk:
            all_blocks.extend(blk)
        report['screens'][s] = row
        # 热图（附加产物，不入库）
        if args.png and dmax is not None:
            heat = np.zeros((dmax.shape[0], dmax.shape[1], 3), dtype=np.uint8)
            m = dmax > 0
            heat[..., 0] = np.where(m, np.clip(dmax * 8, 0, 255), 0).astype(np.uint8)
            heat[..., 1] = np.where(m, 40, 0).astype(np.uint8)
            Image.fromarray(heat).save(args.png.replace('.png', '_%s.png' % s))
        print('[frame_diff] %-5s %s  跨侧差异 %.3f%%  max|Δ|=%d  L2 超限 %d 块  噪声地板 原版 %.3f%% / 我方 %.3f%%'
              % (s, row['verdict'], row.get('l3_diff_ratio', 0) * 100,
                 row.get('l3_max_channel_diff', 0), row.get('l2_blocks_over', 0),
                 (row.get('noise_max_orig') or 0) * 100, (row.get('noise_max_mine') or 0) * 100),
              file=sys.stderr)

    cols = ['screen', 'verdict', 'l1_size_ok', 'l1_ok', 'l1_dmean_max', 'l1_dmean',
            'l1_top_color', 'l1_top_share_pp', 'l2_ok', 'l2_blocks_over',
            'l2_max_block_dmean', 'l2_worst_block', 'l3_evaluated', 'l3_diff_px',
            'l3_diff_ratio', 'l3_max_channel_diff', 'l3_bbox', 'noise_max_orig',
            'noise_max_mine']
    notes = list(args.note) + ['门槛 = fixtures/frame_diff_thresholds.txt sha256 %s（预注册）' % THRESHOLDS_SHA]
    with open(args.out, 'w') as f:
        for n in notes:
            f.write('# %s\n' % n)
        f.write('\t'.join(cols) + '\n')
        for r in results:
            vals = []
            for c in cols:
                v = r.get(c, '')
                if isinstance(v, list):
                    v = ','.join(str(x) for x in v)
                vals.append(str(v))
            f.write('\t'.join(vals) + '\n')
    print('[frame_diff] TSV -> %s' % args.out, file=sys.stderr)

    if args.blocks_out and all_blocks:
        bcols = ['screen', 'row', 'col', 'dmean_max', 'dmean_rgb', 'diff_px',
                 'diff_ratio', 'max_channel_diff', 'over', 'bbox']
        with open(args.blocks_out, 'w') as f:
            for n in notes:
                f.write('# %s\n' % n)
            f.write('\t'.join(bcols) + '\n')
            for r in all_blocks:
                vals = []
                for c in bcols:
                    v = r.get(c, '')
                    if isinstance(v, list):
                        v = ','.join(str(x) for x in v)
                    vals.append(str(v))
                f.write('\t'.join(vals) + '\n')
        print('[frame_diff] blocks TSV -> %s' % args.blocks_out, file=sys.stderr)

    if args.json:
        with open(args.json, 'w') as f:
            json.dump(report, f, ensure_ascii=False, indent=1)
        print('[frame_diff] JSON -> %s' % args.json, file=sys.stderr)
    return 0


if __name__ == '__main__':
    sys.exit(main())
