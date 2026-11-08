# 68 · `agent/tooling-hygiene`：两件一直卡着的小事（`merge_check` 大文件闸 + 冻结语料纪律）

**优先级 P2，但每轮都会撞一次，做完就永久省事。**

## 0. 两件事，都有明确诉求

### A. `merge_check.py` 的大文件闸（报告 §7 最后一条：**仍未定**）

```
现状：MAX_KB=100 一刀切，而"文本真值表入库"是常态
  本轮 52（parse_matrix 1.6 MB）、57、61（大结果表）**每次都撞**，每次都要人肉放行
我（上游）临时把 fixtures/ 下的 tsv/jsonl/md/txt/json 放宽到 4 MB —— 但那是权宜
```

要做的：

```
1. 把"大文件"拆成三类，各自有阈值与处置：
   · fixtures/ 下的**文本真值表**（tsv/jsonl/md/txt/json）：默认 ≤8 MB，超过要 `--allow-large`
   · 代码/文档（src/ client/ docs/ tools/ tests/ examples/）：仍 100 KB 卡死
   · **二进制/压缩产物**：任何大小都拦（现规则保持不变）
2. 加 `--allow-large <pattern>`（可多次），并在输出里**逐条登记放行理由**
3. 单测：造三个假分支（大 tsv / 大 md / 大 png）验证三条阈值各自生效
```

### B. **冻结语料纪律**（我上一轮踩的真坑）

```
我把两份新 replay 拷进了 `.agent-work/replay-snapshot/`（401 → 403）
⇒ 三轴数字全变（647 → 654、批 1080 → 1094），护栏校验报 7 项不一致
⇒ 已把新样本移到 `.agent-work/replay-new/`，快照恢复 401 份
```

要做的：

```
1. `tools/bit_exact_attrib.py --check-baseline` 增加一项**语料指纹检查**：
   记录快照目录的"文件名集合 hash（或 401 份的 sha256 清单）"，
   与 guardrails.json 里记的比对；**不一致就报错并指出多了/少了哪些文件**
2. `fixtures/guardrails.json` 里加 `corpus` 字段（文件名清单的 sha256）
3. `AGENTS.md` 的"反复踩过的坑"加一条：
   **护栏语料必须冻结；新样本另开目录（`replay-new/`），不许混进快照**
```

## 2. 硬约束

- 只改工具与文档，**不许改 `src/`、`client/`**；
- 数字不许退：`cargo test --offline` **1671**、三轴 `fire 647/1075`、`batch 647/1075`；
- `merge_check.py` 保持纯标准库、`--run-tests` 之外 <1 s；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/tooling-hygiene`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/tooling-hygiene`

## 4. 可以改的文件

- `tools/merge_check.py`、`tools/bit_exact_attrib.py`、`fixtures/guardrails.json`、`AGENTS.md`
- `docs/06-verification.md`（若需要记录口径）
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 三类阈值各自生效（三个假分支的退出码）
② `--allow-large` 可用，且输出里登记理由
③ 语料指纹检查：故意多拷一份进快照，`--check-baseline` 必须报错并指出文件名
④ AGENTS 加了那条纪律
⑤ cargo test 1671；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、三阈值测试结果、`--allow-large` 用法、语料指纹故意污染的测试结果、`cargo` 结果。
