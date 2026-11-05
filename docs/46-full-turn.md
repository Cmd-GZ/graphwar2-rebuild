# 46 · 一整回合的报文序列（12 条）

`docs/43` 验证了"开火"那一段；这一轮把**回合的头尾**也补上，于是 `Match` 能产出
一整回合的完整报文序列。

## 1. 序列

```text
TurnInfo{player_id, soldier_id}            ← 轮到谁
EndOfTurnInfo{end_of_turn_tick, time_mode} ← 回合截止
GameStateInfo(WaitingForFunctions)         ← 等待提交

GameStateInfo(CalculatingResults)          ← 收到开火
FunctionInfo(修正后)
FunctionPoints{owner_id, start_x, start_y, end_x}
GameStateInfo(SynchronizingResults)
QueueDestroyObstacle{pos_x, pos_y, radius:0.5, tick}
FunctionActive{owner_id, active_tick}
EffectInfo(ExplosionEffect)
GameStateInfo(ExecutingResults)

TurnCountInfo{turn_count}                  ← 回合结束
（若某队全灭，紧跟 EndingGame）
```

## 2. 实现

```rust
pub fn begin_turn(&mut self, player_id, soldier_id, now_ms, time_mode) -> Vec<String>
pub fn fire(&mut self, owner_id, function_text, x0, y0, flip, now_ms) -> Vec<String>
pub fn end_turn(&mut self) -> Vec<String>
pub fn tick(&mut self, now_tick) -> Vec<String>        // 到期的 DestroyObstacle
```

新增序列化：`emit_end_of_turn`（逐字节测试用抓包样本
`{"EndOfTurnInfo":{"end_of_turn_tick":3685,"time_mode":"Timer1m"}}` 验证）。

## 3. 测试

`game::tests::full_turn_sequence_matches_capture`：把 `begin_turn + fire + end_turn`
的产物逐条解析，断言种类序列恰好是

```
TurnInfo, EndOfTurnInfo, GameStateInfo,
GameStateInfo, FunctionInfo, FunctionPoints, GameStateInfo,
QueueDestroyObstacle, FunctionActive, EffectInfo, GameStateInfo,
TurnCountInfo
```

共 **12 条**，顺序与抓包一致。

## 4. 尚未覆盖

- 一局**多回合连续**的回放（回合之间的 `TurnInfo` 由调度器决定，而队内顺序规则未知，见 `docs/28`）；
- 客户端提交函数之前的 `FunctionInfo`（未修正版）由客户端发出，本实现只产出服务端侧；
- `DestroyObstacle` 的插入位置（本实现由 `tick()` 单独产出，未揉进回合序列的断言）。
