# 41 · 回合状态机

## 1. 观测到的序列（真实对局，3 回合）

```
Setup
→ WaitingForFunctions
→ CalculatingResults      ← 玩家开火
→ SynchronizingResults    ← 服务端算出结果
→ ExecutingResults        ← 到达激活 tick
→ WaitingForFunctions     ← 回合结束
→ …（重复）…
→ EndingGame              ← 分出胜负
→ Setup                   ← 重开
```

3 个回合的序列完全一致，说明这是**状态机**而不是偶然顺序。

## 2. 实现

`src/turnstate.rs`：

```rust
pub enum GameState { Setup, WaitingForFunctions, CalculatingResults,
                     SynchronizingResults, ExecutingResults, EndingGame }
pub enum TurnEvent { GameStarted, Fire, ResultsComputed, Activate, TurnEnded, GameEnded, Reset }
pub fn next_state(state, event) -> Option<GameState>   // None = 该事件在此状态无意义
```

测试 `reproduces_captured_state_sequence`：把抓包里那 13 次迁移逐一喂进去，
结果与抓到的 14 个状态**逐个相同**。

## 3. 未确定的部分

- **各状态持续多久**：`EndOfTurnInfo.end_of_turn_tick` 实测为
  `10886 / 11184 / 11457`（相邻差 ~270~300），而我们的 60Hz tick 时钟
  在同一时刻只有 `65 / 126 / 426` 这种量级 —— **两者对不上**，
  说明 `end_of_turn_tick` 用的是另一套时间基准（可能是应用启动以来的计数）。
  这套基准尚未还原，所以状态机的**时间驱动**还没实现。
- **`FunctionActive.active_tick`**（实测 168 / 515 / 739）同样与 60Hz 时钟对不上。
- 触发 `EndingGame` 的条件（什么时候判定胜负）未单独验证。

## 4. 结论

状态机的**迁移结构**已经可以运行并有真实序列验证；
**时间驱动**（每个状态持续多少 tick）仍缺一套时间基准的解释。
