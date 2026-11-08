# 51 · `agent/fingerprint-v2`：去掉**泄漏特征**重训指纹，把准确率重新量一遍

**优先级 P0（算力活，给你们那台 32 G / 16C32T / 7945HX 正合适）。**

## 0. 为什么必须做

审计（`docs/159`）查明：**84.2% 这个数被泄漏撑高了**。

```
`strs` 特征其实就是 `Location` 的文件名：
  正对照 348/348 都含自己的 `src/*.rs` 串，而目标集合只有 11.8%
  把它清零 → 留出半 84.2% → **73.3%**（连 `consts` 一起清 71.5%）
另外 `hist` 权重清零后两半 top-1 **一个都不变** ⇒ 它没提供信息
```

⇒ 现在这套打分器**在"看得见文件名"的数据上被评估、却在"看不见"的数据上使用**。
`docs/02` §35 的口径已改成「≈80–95% 逐条待核」，但那只是**如实承认**，不是**修好**。

## 1. 要做什么

1. **去掉泄漏特征**：`strs`（Location 文件名）与跨构建不可用的 `consts` 默认**关掉**或降权；
   `hist` 权重为 0 也保留（它不提供信息，留着只会让人误以为有贡献）；
2. **在"无泄漏"设置下重新调参**（这一步是算力活：8 片 × 多组权重的网格）；
3. **诚实重测**：留出半 / 调参半两半都报；再报一次"清零前后"的对照；
4. **重出夹具**（`fixtures/callgraph_fingerprint.tsv`），并更新 `docs/02` §35 的口径为**新的实测数**；
5. 若重训后仍显著低于 80%，**如实写"结构指纹的天花板就在这儿"**并给出证据 —— 那也是结论。

## 2. 硬约束

- **不许再用任何"目标集合看不见、正对照看得见"的特征**（这是本次的核心）；
- **单射保持全局**（上一轮已修：目标被 ≥2 行认领 ⇒ 全部降级 + `multi-claim`）；
- 数字不许退：`cargo test --offline` **1638**、三轴 `fire 647/1075`、`batch 647/1075`、
  死亡 `366/385`、挖洞 `207/294`；headless `entity_drawer 292 / assemble 217 / plane_on_screen 96`；
- 提交信息用 `-F 文件`。

## 3. 环境（你们那台）

- 克隆到本地任意目录，分支 `agent/fingerprint-v2`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 四份构建：`<WS>/.reference/steam/{2026-08-28/libgraphwarrust.so, android-2.0.60/…, windows-2.0.60/graphwarrust.dll, windows-early2026/…}`
- 工具：`tools/callgraph_fingerprint.py`（`build`/`verify`/`tune2`/`calib`/`run`/`merge`/`report`/`probe`）
- 审计报告：`docs/159-audit-fingerprint.md`（先读）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/fingerprint-v2`

## 4. 可以改的文件

- `tools/callgraph_fingerprint.py`、`fixtures/callgraph_fingerprint.tsv`
- `docs/02-module-map.md` §35（口径 + 新数）、`docs/159`（标"已按建议重训"）

❌ 不许改 `src/`、`client/`、`dispatch/`

## 5. 验收

```text
① 关掉哪些特征、为什么
② 重训后的留出半 / 调参半 top-1、top-5（与 84.2 / 73.3 对照）
③ 重出后各档位计数（高置信/推断/歧义/未知）
④ 若仍低于 80%：把"天花板"写清（这算合格交付）
⑤ cargo test 1638；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、关掉哪些特征、重训后两半准确率、档位计数、是否触及天花板、`cargo` 结果。
