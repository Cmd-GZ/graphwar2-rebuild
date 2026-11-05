# 43 · 一回合的报文顺序

## 1. 抓包观测（真实对局，一个「被阻挡、没死人」的回合）

```text
FunctionInfo        (客户端提交的函数，修正前)
FunctionFire
GameStateInfo       CalculatingResults
FunctionInfo        (修正后，带 corrected_function 文本)
FunctionPoints
GameStateInfo       SynchronizingResults
QueueDestroyObstacle
FunctionActive
EffectInfo          ExplosionEffect
GameStateInfo       ExecutingResults
…（若干 TickReply）…
DestroyObstacle
…（若干 TickReply）…
TurnCountInfo
```

要点：

- 状态推进**夹在报文中间**：`CalculatingResults` 在函数提交之后、
  `SynchronizingResults` 在 `FunctionPoints` 之后、`ExecutingResults` 在爆炸之后；
- `QueueDestroyObstacle` 早于 `FunctionActive`；
- `DestroyObstacle` 晚于 `ExecutingResults`（要等若干 tick 之后才真正删除）；
- 有死亡的回合里，`LifeQueuedDeath` + `EffectInfo(DeathEffect)` 出现在
  `SynchronizingResults` 之后、`FunctionActive` 之前。

## 2. 实现

`game::Match::fire` 现在按这个顺序产出报文：

```text
GameStateInfo(CalculatingResults)
FunctionInfo(修正后)
FunctionPoints
GameStateInfo(SynchronizingResults)
[LifeQueuedDeath, EffectInfo(DeathEffect)]*     ← 有死者时
[QueueDestroyObstacle]                          ← 被阻挡时
FunctionActive
EffectInfo(ExplosionEffect)
GameStateInfo(ExecutingResults)
```

新增的序列化函数（`src/protocol.rs`，都有逐字节测试）：
`emit_game_state`、`emit_function_info`、`emit_function_active`。

## 3. 验证

`game::tests::fire_emits_captured_turn_sequence`：断言种类序列恰好是

```
GameStateInfo, FunctionInfo, FunctionPoints, GameStateInfo,
QueueDestroyObstacle, FunctionActive, EffectInfo, GameStateInfo
```

并且三个状态名分别是 `CalculatingResults` / `SynchronizingResults` / `ExecutingResults`。

`game::tests::killed_soldier_appears_between_sync_and_active`：死亡消息出现在
`FunctionActive` 之前。

## 4. 仍未实现

- `DestroyObstacle`（延迟删除）——它要等若干 tick，需要"已排队待删"的状态表；
- `TurnCountInfo` 的推进与回绕；
- `TickReply` 的节拍（客户端多久发一次 `TickRequest` 尚未确定，见 `docs/35`）。
