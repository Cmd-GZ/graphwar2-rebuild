# 07 · `CG/docs-renumber`：文档编号与交叉引用

**优先级 P2。纯文档，零风险，适合给不熟悉逆向的机器做。**

## 现状

`docs/16-client.md` 的小节编号有 **4 个空号**：

```text
§19  §20  §23  §34
```

（历史合并时留下的：两批 agent 都写了同一个号，事后改号留下了洞。）

## 要做什么

1. 把 `docs/16-client.md` 的 `## NN.` 标题**重排成连续编号**；
2. **同步改**所有指向旧编号的交叉引用：
   - 同文件内的 `§NN` / `§NN.M`；
   - 其它文档里指向 `docs/16` §NN 的引用；
   - `src/` 注释里提到 `docs/16 §NN` 的地方（**只改数字，不改文字**）。
3. 用脚本核对（自己写，放 `tools/`）：

```text
grep '^## ' docs/16-client.md | 编号连续、无重复
全仓 §NN 引用：每条都能找到对应小节（无死链）
```

⚠️ **只改编号，不动正文一个字。** 如果发现某处引用本来就指错了（指向的小节内容对不上），
**记下来单独报告**，不要顺手改内容。

## 允许改的文件（**只许这些**）

- `docs/16-client.md`（仅标题编号 + 内部 §引用）
- 其它 `docs/*.md` 里指向它的 §引用（**仅数字**）
- `src/**` 注释里的 `docs/16 §NN`（**仅数字**）
- 新增 `tools/check_doc_refs.py`

⚠️ **不许改**：任何文档的正文内容、`docs/INDEX.md` 的条目结构（可以改编号）、
`docs/HANDOVER.md`、代码逻辑。

## 验收

```text
python3 tools/check_doc_refs.py      # 退出码 0：无空号、无重复、无死链
cargo test --offline                 # 1499 全绿（注释改动不该影响）
git diff --stat                      # 变更行数以"数字替换"为主，正文零改动
```

⚠️ 改了 `src/` 的注释会让 coverage 的树哈希变 —— **必须重跑并提交两个生成物**：

```bash
python3 tools/coverage_matrix.py
python3 tools/coverage_matrix.py --check    # 必须 OK
```

## 汇报（≤8 行）

分支 + sha、改了多少处编号、改了多少处交叉引用、
`check_doc_refs.py` 的退出码、**发现哪些引用本来就指错了**（只报告不改）、`cargo` 条数。
