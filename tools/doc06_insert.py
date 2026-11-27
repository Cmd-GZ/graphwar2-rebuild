#!/usr/bin/env python3
"""把分支上的 `docs/06` 新节**按内容**插入 master 版本，按号顺延重编，绝不按号去重。

为什么需要这个工具（2026-11-27 三起真实事故的教训）
--------------------------------------------------
手工解 `docs/06` 冲突时，三次都坏了，而且每次坏法不同：

1. **正文被吞**：把 §113 的 217 行正文整体丢掉，只剩标题（`docsec.py` 后来的"空心节"检查才抓得住）；
2. **正文搬错位置 + 标题张冠李戴**：T74 的正文跑到文件顶部、标题行丢失，12 处外部 `§120.x` 引用全部指错节；
3. **按号去重把整节删掉**：用"同号只留一份"的去重脚本解冲突 ⇒ **T75 的 §121、T71 的 §123、T77 的 §124
   三整节人间蒸发**（编号看着还连续，`docsec.py` 也报 OK，因为"少一节"不违反任何编号规则）。

所以本工具的原则是：**按内容身份（标题+正文的 sha256）去重，而不是按编号去重**；
编号只是排版，插入后统一按原有顺序 + 新节追加的顺序重排。

用法
----
    # 1) 先用 master 版本覆盖回一个"已知 good"的基座（可选，建议）
    git checkout <good-ref> -- docs/06-verification.md
    # 2) 把分支的新节插进来（可多次调用；每次插入到文件末尾再统一排号）
    python3 tools/doc06_insert.py --from-ref origin/agent/f3-heavy --title-match T71
    python3 tools/doc06_insert.py --from-ref origin/agent/protocol-strict --title-match 'protocol'
    # 3) 校验
    python3 tools/docsec.py check docs/06-verification.md
    python3 tools/check_doc_refs.py

`--title-match` 用正则匹配 `## N.` 之后的首行标题；命中多节时会报错要求加 `--index`。
"""
import argparse
import hashlib
import re
import subprocess
import sys

PATH = 'docs/06-verification.md'


def split_sections(text):
    """-> (preamble_lines, [[num, [lines]], ...])，节按文件出现顺序。"""
    pre, blocks, cur = [], [], None
    for line in text.split('\n'):
        m = re.match(r'^## (\d+)\.', line)
        if m:
            cur = [int(m.group(1)), [line]]
            blocks.append(cur)
        elif cur is None:
            pre.append(line)
        else:
            cur[1].append(line)
    return pre, blocks


def body_hash(block):
    """内容身份：去掉 `## N.` 里的号（号是排版）后取 sha256。"""
    lines = list(block)
    # 行内自引用 `§N.M` 也参与归一化，否则同一节被重编过号后 hash 会变
    norm = []
    for i, line in enumerate(lines):
        if i == 0:
            line = re.sub(r'^## \d+\.', '## N.', line)
        line = re.sub(r'§\d+\.(\d+)', r'§N.\1', line)
        line = re.sub(r'^### \d+\.(\d+)', r'### N.\1', line)
        norm.append(line)
    return hashlib.sha256('\n'.join(norm).encode('utf-8')).hexdigest()


def renumber(blocks, mapping):
    """按 mapping{old:int -> new:int} 重写节号、子节号与该节内的自引用。"""
    out = []
    for _, body in blocks:
        old = None
        m = re.match(r'^## (\d+)\.', body[0])
        if m:
            old = int(m.group(1))
        new = mapping.get(old, old)
        fx = []
        for line in body:
            if old is not None and new != old:
                if re.match(rf'^## {old}\.', line):
                    line = re.sub(rf'^## {old}\.', f'## {new}.', line)
                elif re.match(rf'^### {old}\.', line):
                    line = re.sub(rf'^### {old}\.', f'### {new}.', line)
                line = re.sub(rf'§{old}\.(\d+)', rf'§{new}.\1', line)
            fx.append(line)
        out.append(fx)
    return out


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument('--from-ref', required=True, help='分支/提交，如 origin/agent/x')
    ap.add_argument('--title-match', required=True, help='匹配节标题的正则')
    ap.add_argument('--index', type=int, default=0, help='多节命中时选第几个（0 起）')
    ap.add_argument('--path', default=PATH)
    ap.add_argument('--dry-run', action='store_true')
    a = ap.parse_args(argv)

    cur_text = open(a.path, encoding='utf-8').read()
    src_text = subprocess.run(['git', 'show', f'{a.from_ref}:{a.path}'],
                              capture_output=True, text=True, check=True).stdout

    pre, cur = split_sections(cur_text)
    _, src = split_sections(src_text)
    have = {body_hash(b) for _, b in cur}

    pat = re.compile(a.title_match)
    hits = [(n, b) for n, b in src if pat.search(b[0]) and body_hash(b) not in have]
    if not hits:
        print(f"✗ `{a.from_ref}` 里没有匹配 `{a.title_match}` 的**新**节（按内容 hash 判定）")
        return 2
    if len(hits) > 1 and a.index == 0:
        print(f"✗ 命中 {len(hits)} 个新节，请用 --index 指定：")
        for i, (n, b) in enumerate(hits):
            print(f"   [{i}] §{n} {b[0][:80]}")
        return 2
    n, block = hits[a.index]
    print(f"插入 §{n}: {block[0][:90]}")

    # ⚠️ 只给**新插入**的节分配号 = 现有最大号 + 1；**不动任何既有节的号**。
    # （2026-11-27 教训：全局重排 1..N 会把历史缺口 §43/§89 一起"填平"，
    #   还会让所有外部 `§N.M` 引用集体失效 —— 这里必须保守。）
    new_num = max(x for x, _ in cur) + 1
    # ⚠️ 只重编**新插入那一块**（按位置），绝不用 {旧号: 新号} 去映射整个文件 ——
    #    否则会命中既有同名号（2026-11-27 事故：T71 的节原本也叫 §120，映射把 T74 的 §120 一起改掉了）
    blocks = cur + [[new_num, renumber([[n, block]], {n: new_num})[0]]]
    # 统一格式：每块是 [num, [text lines]]；拼装时只取 lines
    # 顺序 = 既有节原序 + 新节（不排序，避免把历史缺口"填平"）
    blocks_sorted = blocks
    text = '\n'.join(pre + [l for _num, lines in blocks_sorted for l in lines])
    if a.dry_run:
        print("(--dry-run，未写盘)")
        return 0
    open(a.path, 'w', encoding='utf-8').write(text)
    print(f"已写入：{len(blocks_sorted)} 节；新节号 §{new_num}")
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
