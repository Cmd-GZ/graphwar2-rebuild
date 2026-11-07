# 11 · `agent/endx-sample-point`：`end_x` 差一个采样点

**优先级 P1。** 占失败 **95 批**。机制已实测，差的是**边界取点**。

## 已知（这是最接近"已破案"的一条）

下游实测（`dispatch/reports/CG-2026-11-07.md` §8）：

```text
回放的 end_x   命中  我方 pts[i-1]
回放的爆炸 pos_x 命中 我方 pts[i]
⇒ 两者差一个采样点
```

也就是说：**回放的 `FunctionPoints.end_x` 记的是"最后一个计入的点之前那个"**，
而我们的 `end_x` 取的是最后那个点。

## 要做什么

1. 先把这个"差一个"**在指令级钉死**：回放写 `FunctionPoints` 的那一处（`docs/14` / `docs/136` 里有地址线索），
   `end_x` 到底取 `pts[len-1]` 还是 `pts[len-2]`，**逐条指令确认**；
2. 再决定改哪一层：
   - 若差别在**采样器返回的序列**（多收/少收一个点）⇒ 改采样器（要非常小心，`docs/04` 有 24/24 逐位基线）；
   - 若差别只在**报文取值**（序列对、取值点错）⇒ 只改报文构造，**不动采样器**。
3. 量化：95 批的 `end_x` 偏差应归零或显著下降。

⚠️ **采样器是逐位基线（24/24 用例整条点序列相等）**，动它之前先跑 `cargo test --offline` 里所有 path 相关测试；
   **任何一条变红就立刻回退，改走"只改报文取值"那条路**。

## 允许改的文件

- `src/game.rs`（报文构造那一段）**或** `src/graphwar_path_calculator.rs`（只在证据指向采样器时）
- `tests/`、`examples/`
- `docs/04-path-sampling.md` 或 `docs/136` 追加一节

⚠️ 不许碰 `*_class.rs`、`client/`。

## 验收

```text
end_x 偏差统计：before（95 批差一个点）→ after 必须给出新数字
path 相关测试全绿（采样器若动过，必须是 24/24 那种逐位口径）
cargo test --offline 全绿（1512）；coverage --check OK
```

## 汇报（≤8 行）

分支+sha、**`end_x` 到底取哪个点（指令级结论 + 地址）**、
改的是采样器还是报文、before/after 偏差数、有没有动到采样器。
