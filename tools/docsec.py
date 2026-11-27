#!/usr/bin/env python3
"""docsec.py —— `docs/06` / `docs/16` 这类"按 `## N.` 编号追加"的文档的合并工具。

## 为什么有这一份
2026-11-26 一天里，同一个坑踩了 **四次**：多轮合并把冲突标记嵌套进这两个文件；我用临时脚本
"按节号重排"时，**两个分支用了同一个节号**，`dict[key]=block` 静默覆盖，**丢掉了一节正文**
（T33 的 `GraphwarRootNode` 那一节就这样没了一次，靠 `git show` 找回）。

## 用法
```bash
python3 tools/docsec.py check  docs/06-verification.md          # 只体检：重复号 / 断号 / 残留冲突标记
python3 tools/docsec.py sort   docs/06-verification.md          # 按节号物理重排（不动正文）
python3 tools/docsec.py merge  docs/06-verification.md \
        --take 79:agent/unblock-views --renumber 79:78          # 从别的分支取 §79，落到 §78
```
`merge` 的语义：**先把当前文件里同号的那节报出来并拒绝执行**（除非给 `--replace`），
再按 `--take <from_rev>:<rev>` 取一节、按 `--renumber <from>:<to>` 改号后插入。
它**永远**不会静默覆盖：同号就是错，必须显式 `--replace` 或先给目标号让号。

## 判定
- `重复号`：同一个 `## N.` 出现 ≥2 次 ⇒ **错误**（历史事故的直接原因）；
- `空心节`：某节正文 < 3 行而紧邻下一节 ⇒ **错误**（2026-11-27 真发生过：手工解冲突把 §113 的 217 行正文吞掉，只剩标题；
  而 `check_doc_refs.py` 只查编号连续性，抓不到）；
- `断号`：`N` 与 `N+1` 之间缺号 ⇒ 警告（`docs/06` 的 §43 历史上本就缺）；
- `冲突标记`：`<<<<<<<` / `=======` / `>>>>>>>` ⇒ **错误**。
"""
import argparse
import re
import subprocess
import sys

SEC = re.compile(r'^## (\d+)\.')


def split_sections(text):
    """→ (前言, [(编号, [行...]), ...])，保持物理顺序。"""
    pre, blocks, cur = [], [], None
    for line in text.split('\n'):
        m = SEC.match(line)
        if m:
            cur = [int(m.group(1)), [line]]
            blocks.append(cur)
        elif cur is None:
            pre.append(line)
        else:
            cur[1].append(line)
    return pre, blocks


def report(path):
    text = open(path, encoding='utf-8').read()
    pre, blocks = split_sections(text)
    nums = [n for n, _ in blocks]
    seen, dup = set(), []
    for n in nums:
        if n in seen:
            dup.append(n)
        seen.add(n)
    gaps = [n for n in range(1, (max(nums) if nums else 1) + 1) if n not in seen]
    markers = [i + 1 for i, l in enumerate(text.split('\n'))
               if l.startswith('<<<<<<<') or l.startswith('>>>>>>>') or l.strip() == '=======']
    print(f"{path}: 节数 {len(nums)}｜范围 {min(nums) if nums else '-'}..{max(nums) if nums else '-'}")
    if dup:
        print(f"  ✗ 重复节号: {sorted(set(dup))} ⇒ 会被静默覆盖，必须手工改号")
    if gaps:
        print(f"  ⚠ 断号: {gaps}")
    if markers:
        print(f"  ✗ 残留冲突标记行: {markers[:8]}{' …' if len(markers) > 8 else ''}")
    # 空心节检测：2026-11-27 引入（起因：手工解冲突把 §113 的 217 行正文整体丢掉，只剩标题，
    # 而 check_doc_refs 只查编号连续性 ⇒ 没人发现）。判据：某节正文 < 3 行且紧邻下一节。
    hollow = []
    for idx, (n, body) in enumerate(blocks):
        nxt = blocks[idx + 1][0] if idx + 1 < len(blocks) else None
        if nxt is not None and len(body) < 3:
            hollow.append((n, len(body) - 1, nxt))
    if hollow:
        print("  ✗ 空心节（正文 <3 行，疑似合并时正文被吞）: "
              + ", ".join(f"§{n}(={bl}行)→下一个§{nx}" for n, bl, nx in hollow))
    if not dup and not markers and not hollow:
        print("  ✓ 无重复号、无冲突标记、无空心节")
    return 1 if (dup or markers or hollow) else 0


def sort_file(path):
    text = open(path, encoding='utf-8').read()
    pre, blocks = split_sections(text)
    nums = [n for n, _ in blocks]
    dup = sorted({n for n in nums if nums.count(n) > 1})
    if dup:
        print(f"✗ 有重复节号 {dup}，拒绝重排（否则会丢正文）", file=sys.stderr)
        return 2
    blocks.sort(key=lambda b: b[0])
    open(path, 'w', encoding='utf-8').write('\n'.join(pre + [l for _, b in blocks for l in b]))
    print(f"{path}: 已按节号重排，末节 {blocks[-1][0] if blocks else '-'}")
    return 0


def git_show(rev, path):
    return subprocess.run(['git', 'show', f'{rev}:{path}'],
                          capture_output=True, text=True, check=True).stdout


def merge_section(path, takes, renumbers, replace):
    text = open(path, encoding='utf-8').read()
    pre, blocks = split_sections(text)
    have = {n for n, _ in blocks}
    for spec in takes:
        src_no, rev = spec.split(':', 1)
        src_no = int(src_no)
        _, foreign = split_sections(git_show(rev, path))
        mapping = {int(a): int(b) for a, b in
                   (x.split(':') for x in renumbers)}
        dst_no = mapping.get(src_no, src_no)
        if dst_no in have and not replace:
            print(f"✗ 目标 §{dst_no} 已存在（来自 {rev} 的 §{src_no}）⇒ 请先让号（--renumber {src_no}:<空号>）", file=sys.stderr)
            return 3
        block = dict(foreign).get(src_no)
        if block is None:
            print(f"✗ {rev} 里没有 §{src_no}（现有 {sorted(foreign)[-4:]}）", file=sys.stderr)
            return 4
        # 改号：标题 + 内部 ### N.M + 交叉引用 §N.M
        fixed = []
        for l in block:
            if re.match(rf'^## {src_no}\.', l):
                l = re.sub(rf'^## {src_no}\.', f'## {dst_no}.', l)
            elif re.match(rf'^### {src_no}\.', l):
                l = re.sub(rf'^### {src_no}\.', f'### {dst_no}.', l)
            l = re.sub(rf'§{src_no}\.(\d+)', rf'§{dst_no}.\1', l)
            fixed.append(l)
        blocks = [b for b in blocks if b[0] != dst_no] if replace else blocks
        blocks.append([dst_no, fixed])
        have.add(dst_no)
        print(f"  取 {rev} §{src_no} → 落 §{dst_no}")
    blocks.sort(key=lambda b: b[0])
    open(path, 'w', encoding='utf-8').write('\n'.join(pre + [l for _, b in blocks for l in b]))
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('cmd', choices=['check', 'sort', 'merge'])
    ap.add_argument('path')
    ap.add_argument('--take', action='append', default=[], metavar='N:REV')
    ap.add_argument('--renumber', action='append', default=[], metavar='FROM:TO')
    ap.add_argument('--replace', action='store_true')
    a = ap.parse_args()
    if a.cmd == 'check':
        return report(a.path)
    if a.cmd == 'sort':
        return sort_file(a.path)
    rc = merge_section(a.path, a.take, a.renumber, a.replace)
    return rc or report(a.path)


if __name__ == '__main__':
    sys.exit(main())
