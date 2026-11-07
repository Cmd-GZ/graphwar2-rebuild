# 10 · `agent/multi-shot-wiring`：多发批结构接线（**第二收益**）

**优先级 P0。** 占失败 **123 批**；接线后种类序列预计 **818 → 918**。

## 现象

多发批（一个结算批里 ≥2 发）的**报文结构**与我们产的不一致：

```text
我方 = GameStateInfo | FunctionInfo | FunctionPoints | GameStateInfo | FunctionActive | EffectInfo/ExplosionEffect | GameStateInfo
回放 = GameStateInfo | FunctionInfo | FunctionPoints | GameStateInfo | EffectInfo/SparkEffect | FunctionActive | EffectInfo/ExplosionEffect | GameStateInfo
```

回放里 `EffectInfo` 出现在 `FunctionActive` **之前**，我们放在后面；而且少了一条。

## 已知（别重推）

- **`diffconv` 里已经修好了**（`src/_scaffold/diffconv.rs`），只差**接进对拍驱动**；
- 归因表：`dispatch/reports/CG-2026-11-07.md` §2；
- `docs/141` 是同时回合制的三段流程（收集 → 结算 → 广播）与分组顺序的权威描述。

## 要做什么

1. 先确认 `diffconv` 里的修正是**什么语义**（它按什么重排/补哪一条），别盲接；
2. 把它接进 `replaydiff` 的预测路径（**只接驱动，不改 `src/` 的游戏逻辑**）；
3. 量化：种类序列应 **818 → ~918**；内容命中是否也涨，如实报。

⚠️ 如果发现"接线后某些单发批反而变差"，说明 `diffconv` 的规则不该无条件应用——
   **按条件接**并在文档里写清判据。

## 允许改的文件

- `src/_scaffold/replaydiff.rs`、`src/_scaffold/diffconv.rs`
- `examples/`（只加不改）
- `docs/` 对应主题追加

⚠️ **不许改** `src/game.rs` 的结算逻辑（那是 09 的地盘）、别的 `*_class.rs`、`client/`。

## 验收

```text
种类序列命中：before 818 / 1080 → after 必须给出确切数
多发批（≥2 发）单独统计一次
单发批不许变差（若变差，说明条件没加对）
cargo test --offline 全绿（1512）；coverage --check OK
```

## 汇报（≤8 行）

分支+sha、`diffconv` 那条修正的语义一句话、接线位置、
before/after 三个数字（总/多发批/单发批）、有没有变差的批次。
