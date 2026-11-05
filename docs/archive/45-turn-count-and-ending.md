# 45 · 回合计数与结束条件

## 1. `TurnCountInfo` 出现在回合结束

抓包里它总是紧跟在一串 `TickReply` 之后、下一个状态之前：

```json
{"TurnCountInfo":{"turn_count":4}}
{"GameStateInfo":{"game_state":"EndingGame"}}
```

序列实测为 `0, 1, 2, 3, 4`（每回合 +1）。

**回绕规则仍未观测到**——这一局到 4 就结束了，没有出现 `4 → 0`。
（更早的某次抓包里见过 `0,1,2,3,4,0`，但那次的 `0` 更可能是**重开新局**后的第一个计数，
而不是同一局内的回绕。样本不足以区分，所以**不下结论**。）

## 2. `EndingGame` 的触发：某队全灭

决定性证据：`TurnCountInfo{turn_count:4}` 紧接着就是 `EndingGame`，
而就在这一回合，我方最后一名士兵（`230`）被判定死亡：

```json
{"LifeQueuedDeath":{"entity_id":230,"tick":1058}}
{"EffectInfo":{"entity_id":230,...,"effect_type":"DeathEffect"}}
...
{"LifeInfo":{"entity_id":230,"alive":false}}
{"TurnCountInfo":{"turn_count":4}}
{"GameStateInfo":{"game_state":"EndingGame"}}
```

本局我方两名士兵（`230`、`231`）全部阵亡，敌方两名存活 ⇒ 判定结束。

## 3. 实现

```rust
// Match
pub fn set_team(&mut self, soldier_id: i64, team: u8)
pub fn end_turn(&mut self) -> Vec<String>
//   -> ["TurnCountInfo", ("GameStateInfo(EndingGame)" 当某队已无存活士兵)]
```

测试 `end_turn_counts_and_detects_annihilation`：
先一回合不死人 → 只有 `TurnCountInfo`；再打死敌方唯一士兵 → 多一条 `EndingGame`。

> 写这条测试时踩了一个坑：`Match::new` 一开始把士兵默认登记成"0 号队"，
> 于是 `end_turn` 误判"0 队全灭"。改成**不预设队伍**、由 `set_team` 显式登记后才正确。

## 4. 还没做

- `TurnCountInfo` 的回绕（需要一局打满更多回合的样本）；
- `EndingGame` 之后的重开流程（`Reset` 回 `Setup` 已在状态机里，但没有真实序列验证）。
