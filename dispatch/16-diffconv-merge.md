# 16 · `agent/diffconv-merge`：把 `diffconv` 的坐标计算并回内核

**优先级 P1。纯工程债，第二轮报告 §5 点名的"跨任务结构债"。**

## 问题

现在**两处**各算一遍坐标：

```text
src/_scaffold/diffconv.rs     收敛层自己算（为对拍方便）
src/game.rs / 内核             真实实现
```

第二轮 §5 写明：这两份**已经出现过不一致**（一份按修正文本、一份按原始文本取点），
导致同一批数据在两条路径上得出不同结论。**这种债不修，后面每次对拍都要先怀疑"是驱动错了还是内核错了"。**

## 要做什么

1. 找出 `diffconv` 里所有"重新算坐标/重新算点"的位置（**列清单**，别急着改）；
2. 逐个改成**调用内核同一函数**（`graphwar_path_calculator` / `settle` 里的既有入口，
   需要新入口就加，**不要复制逻辑**）；
3. 改完验证：两条路径对同一批回放给出**逐位相同**的中间结果（点序列、stop、killed）；
   **这是本任务的核心验收**，不是"测试还绿"。

## 环境

- 克隆到 `<WS>/.agent-work/diffconv/repo`，分支 `agent/diffconv-merge`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/diffconv/target`
- 快照：`<WS>/.agent-work/replay-snapshot/`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/diffconv-merge`

## 可以改的文件

- `src/_scaffold/diffconv.rs`、`src/_scaffold/replaydiff.rs`
- `examples/`（只加不改）、`tests/`
- `docs/06-verification.md` 追加一节

❌ 不许改 `src/game.rs` 的结算逻辑、`*_class.rs`、`client/`。

## 验收

```text
清单：diffconv 里重复计算点的位置 N 处 → 全部改为调内核
一致性：同一批回放，两条路径的点序列/stop/killed 逐位相同（给出断言与样本数）
回归：内容命中 324、种类序列 1074 这两个数**不许退**（退了就是并错了）
cargo test --offline 全绿（基线 1519）
```

## 汇报（≤8 行）

分支+sha、重复计算点几处、改成调哪个内核入口、
一致性断言怎么写的（样本数）、两个既有数字有没有退、`cargo` 条数。
