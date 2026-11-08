# 57 · `agent/new-evidence`：给结构指纹换**新证据来源**（51 号说"再调权重没用"）

**优先级 P1（算力 + 逆向混合）。**

## 0. 现状（51 号已经把话说死）

```
302 组结构权重网格，留出半全部落在 72.7–73.3% ⇒ **天花板 ≈73%**
再往上**必须引入新证据来源**，不是继续调权重：
  · 机器码级指纹（现在只用 size/直方图/被调者/字符串这类粗特征）
  · 常量 xref（这个函数引用了哪些全局常量 → 与 Windows 侧对）
  · 字符串语义（不是"文件名"这种泄漏特征，而是**真字符串内容**的分桶语义）
```

## 1. 要做什么

1. **先量化"新特征的可用性"**：对 348 个已定名样本，统计三个新特征各自的
   `正对照命中率` 与 `目标集合覆盖率`（**必须两边都报**，否则又是泄漏）：
   ```
   F1 机器码 n-gram / 指令序列哈希（跨编译器不稳定，先测它到底稳不稳）
   F2 常量 xref 集合（引用了哪些地址的常量；Windows/Linux 布局不同 ⇒ 要归一化）
   F3 字符串**内容**分桶（不是文件名；例如"错误信息模板"这类内容特征）
   ```
2. **只在"两集合覆盖率相当"的特征上做融合**（加权/投票），重测留出半；
3. **如实报**：新特征把 72.7% 推到多少；若还是过不了 80%，**明确写"这个方向也到顶了"**；
4. 若某个特征在**跨 ISA**（Android）上反而更稳，单独报（那会是意外收获）。

## 2. 硬约束

- **任何"正对照有、目标集合没有"的特征都不许用**（这是本任务的核心纪律，51 号踩过）；
- 并行要求：**进程池 ≥16**，汇报给墙钟与加速比；
- 数字不许退：`cargo test --offline` **1659**、三轴 `fire 647/1075`、`batch 647/1075`；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/new-evidence`
- 四份构建：`<WS>/.reference/steam/{2026-08-28, android-2.0.60, windows-2.0.60, windows-early2026}`
- 工具：`tools/callgraph_fingerprint.py`（51 号之后的版本）、`tools/rust_sym.py`、
  `tools/three_way_match.py`、python+capstone
- 先读：`docs/02` §35、`docs/159-audit-fingerprint.md`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/new-evidence`

## 4. 可以改的文件

- `tools/`、`fixtures/`（新增/重出）
- `docs/02-module-map.md` §35（追加"新证据来源"小节）
- ❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 三个新特征各自的「正对照命中率 / 目标覆盖率」（两边都报）
② 融合后的留出半 top-1 / top-5
③ 与 72.7% 的对比；有没有过 80%
④ 若没过：写清"哪个特征为什么不行"
⑤ cargo test 1659；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、三个特征的两边数字、融合后准确率、有没有过 80%、结论、`cargo` 结果。
