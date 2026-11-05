# 42 · `end_of_turn_tick` 解开了：回合截止时刻

`docs/41` 留了一个疑点：`EndOfTurnInfo.end_of_turn_tick` 的值是 `10885 / 11432 / 11680 / 11945`
这种量级，而同一时刻的 `current_tick` 只有 `65 / 606 / 846 / 1146` —— 两套数对不上。

## 1. 先确认频率相同

用抓包时间戳对它做回归：

```
Δt=9.06s  Δtick=547  → 60.4 /s
Δt=4.13s  Δtick=248  → 60.0 /s
Δt=4.36s  Δtick=265  → 60.8 /s
```

**和 `current_tick` 一样是 60 Hz**，所以差的不是频率，而是**基准/含义**。

## 2. 真正的含义：回合截止

假设 `end_of_turn_tick = 回合开始时的 tick + 回合时长`，反推"回合开始 tick"：

| 当时的 `current_tick` | `end_of_turn_tick` | 反推开始 tick（减 10800） | 偏差 |
|---|---|---|---|
| 65 | 10885 | 85 | +20 |
| 606 | 11432 | 632 | +26 |
| 846 | 11680 | 880 | +34 |
| 1086 | 11945 | 1145 | +59 |
| 1146 | 11945 | 1145 | **−1** |

`10800 = 180 秒 × 60`，正是 `Timer3m`。反推出的开始 tick 与当时的 `current_tick`
相差 0~60（抓包时刻与"回合真正开始"不同步所致），**模型成立**。

⇒ `end_of_turn_tick` 是**这一回合的截止时刻**，不是时钟；
偏移量的漂移来自"玩家提前提交 → 回合提前结束 → 下一个截止时刻重新计算"。

## 3. 实现

```rust
// src/tick.rs
pub fn turn_duration_ticks(time_mode: &str) -> Option<i64>
pub fn end_of_turn_tick(turn_start_tick: i64, time_mode: &str) -> Option<i64>
```

- **已验证**：`Timer3m` = 10800 tick（180 s）；
- **推断**（文档里标了）：其余档位按"名字里的时间 × 60"。
  `Timer1m`/`Timer2m`/`Timer5m`/`Timer30s` 没有取到独立样本；
- `TimerInf`（无限时）返回 `None`（没有截止）。

## 4. 还剩什么

- 其它 `time_mode` 档位的实际 tick 数（需要分别抓包验证）；
- 回合**提前结束**的判定（所有玩家提交后是否立即推进）；
- `FunctionActive.active_tick`（实测 168/515/739）是否也属于这套基准，尚未验证。
