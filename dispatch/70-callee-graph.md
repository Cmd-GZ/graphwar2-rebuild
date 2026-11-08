# 70 · `agent/callee-graph`：建**间接调用图** —— 一次性救活 6 个特征（算力活）

**优先级 P0。报告 §9-3 说这是"目前唯一有把握能把 78% 往上推"的方向。**

## 0. 现状

```
58 号量出：直接 `call rel32` 34,374 条 vs **指针 `call *slot` 52,577 条**
⇒ **64.6% 的调用点现在根本没进图**
⇒ 因此 `callee / ncal / cbsig / rbsig` 等 **6 个特征"两侧不可比"**（正对照有、目标集合没有）
   —— 这正是 51/57 反复踩的"泄漏/不可比"陷阱的另一种形态
```

## 1. 要做什么（重活，**必须吃满 16 核**）

1. **解析函数指针派发**：对四份构建，把 `call *0x…(%rip)` / `call *%reg` 落到具体目标：
   ```
   · `.data.rel.ro` + `.rela.dyn` 的 `R_X86_64_RELATIVE`（Linux）
   · Windows PE 的 `.rdata` 指针 + `.pdata`/重定位（COFF 符号表可用）
   · 一个槽被多处写入时要保留**多目标**（不许猜一个）
   ```
2. **给出每个函数的三个量**：直接被调者集合、指针被调者集合、被取地址次数（`lea [fn]`）；
3. **把 6 个"不可比"特征改成"两侧可比"**：条件必须是**两边都能算**（不是一边有、一边没有），
   然后重测留出半准确率；
4. **正对照**：348 个已定名函数上，先把"图本身"验一遍（例如已知的 `0x9bac98` 槽必须能解析出目标）。

## 2. 并行要求

```
· 四份构建 × 按 `.text` 分片 → 进程池 --jobs ≥16
· 汇报给 墙钟 与 加速比；接近串行不算完成
· ⚠️ 这是**图构建**，不是矩阵乘 —— **不要上 GPU**（63 号的教训：先看接口形状）
```

## 3. 纪律

- **负结论要有正对照**（已知槽验扫描器）；
- 数字不许退：`cargo test --offline` **1693**、三轴 `fire 647/1075`、`batch 647/1075`；
- 大结果表放 `fixtures/`（tsv，≤8 MB 免放行；超了用 `--allow-large 'PATTERN=理由'`）；
- 提交信息用 `-F 文件`。

## 4. 环境

- 分支 `agent/callee-graph`
- 四份构建 + `analysis/funcstarts_ghidra_va.pkl`（**别用**错的 `funcstarts.pkl`）
- 工具：`tools/ptr_slots.py`、`tools/callers.py`、`tools/addr_probe.py`、python+capstone
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/callee-graph`

## 5. 验收

```text
① 图规模（直接边 / 指针边 / 多目标槽 / 无法解析的调用点各多少）
② 正对照：已知槽能解析出目标（贴证据）
③ 6 个特征怎么改成"两侧可比"的；改完留出半 top-1（与 78.18 对照）
④ 并行：墙钟 + 加速比
⑤ cargo test 1693；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、图规模、正对照结论、6 特征改法与新的留出半、加速比、`cargo` 结果。
