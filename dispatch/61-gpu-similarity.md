# 61 · `agent/gpu-similarity`：用 **GPU** 做结构指纹的**大规模相似度检索**

**优先级 P1。这是本项目里**唯一真正适合 GPU** 的活（矩阵型、可切块、无分支）。**

> 下游机器：**RTX 4060**（混合显卡模式，可能要用 `prime-run` 才能在独显上跑；
> 不确定就先 `nvidia-smi` 看有没有进程/能不能分配，**不通就退回 CPU 但必须报实测**）。

## 0. 为什么是它

`51`/`57` 把结构指纹的准确率顶到 **72.7%**，卡点是"**特征不好**"，而不是"算得慢"。
但真要做"新特征 × 全量交叉"（12,736 × 51,985），CPU 上是 **6.6 亿对**的相似度，
**这才是 GPU 该上场的形状**：

```
· 每对分数 = 约 16 个标量特征的加权和 + 几个集合交并
· 无分支、可切块、结果只需 top-K
⇒ 纯 GEMM / 广播运算，天然适合 4060
```

## 1. 要做什么

1. **先把特征矩阵物化**（从 `tools/callgraph_fingerprint.py` 的 `build` 产物导出）：
   ```
   Linux 侧 (12,736 + 348) × F  与  Windows 侧 51,985 × F
   连续特征（size 比、直方图余弦、常量数…）放成 float32 矩阵；
   集合特征（被调者/字符串分桶）用 multi-hot 稀疏矩阵
   ```
2. **GPU 上算 top-K**：
   ```
   连续部分 = 一次大 GEMM（切块，显存 ≤ 4 GB）
   集合部分 = multi-hot 的 交集 = A @ Bᵀ，用同一套切块
   最后融合成总分，取每行 top-5
   ```
   实现可用 `torch`（CUDA）或 `cupy`；**不要**为了 GPU 而引入编译型扩展。
3. **正对照必须同口径**：拿 348 个已定名样本，比较
   **GPU 版 top-1/top-5 与 CPU 版逐条一致**（不一致就是实现 bug，不是"精度差异"）；
4. **报加速比**：墙钟 CPU 单进程 vs GPU，写明 GPU 型号、显存占用、是否 `prime-run` 生效；
5. **若 GPU 不可用**（驱动/混合显卡没配好）：**退回 CPU + numpy（BLAS 多线程）**，
   仍要给加速比，并写清"为什么没用上 GPU"。**不许卡在这里。**

## 2. 硬约束

- **在 GPU 上只做"算"，不做"特征设计"** —— 特征口径必须与 51 号完全一致，否则结果不可比；
- 不许改 `src/`、`client/`、`dispatch/`；
- 提交信息用 `-F 文件`；**别把大矩阵入库**（只入 top-K 结果表）。

## 3. 环境

- 分支 `agent/gpu-similarity`
- 四份构建：`<WS>/.reference/steam/{2026-08-28, android-2.0.60, windows-2.0.60, windows-early2026}`
- 工具：`tools/callgraph_fingerprint.py`（`build`/`verify`/`run`/`merge`）
- GPU 自检：
  ```bash
  nvidia-smi                    # 有没有卡、驱动版本
  prime-run nvidia-smi          # 混合显卡下可能必须这样调
  python -c "import torch; print(torch.cuda.is_available())"
  ```
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/gpu-similarity`

## 4. 验收

```text
① 特征矩阵规模（行×列、float32 显存占用）
② GPU 是否用上（型号、显存峰值、是否 prime-run）；没用上就写清原因
③ **GPU vs CPU 的 top-1/top-5 逐条一致**（348 个正对照）
④ 加速比（墙钟；给两边的绝对时间）
⑤ 产物（top-K 结果表）位置
⑥ cargo test 1659；coverage --check OK；check_doc_refs rc=0
```

## 5. 汇报（≤8 行）

分支+sha、矩阵规模、GPU 是否用上、正对照是否逐条一致、加速比、产物位置、`cargo` 结果。
