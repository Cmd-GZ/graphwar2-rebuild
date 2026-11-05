# 31 · 由结算生成协议事件

`src/settle.rs` 里新增 `ShotEvent` 与 `events()`：把一次开火的结算结果
翻译成协议上会出现的报文序列。

## 1. 事件顺序（观测依据）

真实抓包里一次"打死人"的开火，去掉状态机消息（`GameStateInfo`、`TickReply`、
`FunctionActive`）后，顺序是：

```text
FunctionPoints
LifeQueuedDeath
EffectInfo(effect_type = DeathEffect)
EffectInfo(effect_type = ExplosionEffect)
```

若弹道**被障碍挡住**，还会有一条 `QueueDestroyObstacle`（位置 = 停止点）。

## 2. 实现

```rust
pub enum ShotEvent {
    FunctionPoints { owner_id, start_x, start_y, end_x },
    LifeQueuedDeath { entity_id },
    DeathEffect  { entity_id, x, y },   // 位置是士兵站立处
    QueueDestroyObstacle { x, y, radius }, // 仅当被阻挡
    ExplosionEffect { x, y },           // 位置是停止点
}

pub fn events(owner_id, x0, y0, outcome, targets) -> Vec<ShotEvent>
```

## 3. 验证（真实数据）

`tests/live_hits.rs::generated_events_follow_captured_protocol_sequence`：

```
首条   FunctionPoints{owner_id: 234}
次条   LifeQueuedDeath{entity_id: 230}
三条   DeathEffect{230, x=-10.452779035793856, y=-2.651563472451839}
末条   ExplosionEffect{x ≈ -24.9936}
并且   未被阻挡 → 不产生 QueueDestroyObstacle
```

死亡效果点与抓包**逐位相同**（`1e-9` 容差），爆炸点误差 < 1（受采样步长影响）。

## 4. 明确没建模的部分（差异记录）

- **tick 值**：抓包里 `LifeQueuedDeath.tick` 与 `EffectInfo.start_tick` 相等（如 514），
  而 `QueueDestroyObstacle.tick` 与爆炸的 `start_tick` 相差 0~1（见过 10974 与 12265/12266）。
  这些 tick 取决于**弹丸飞行时间**，属于未还原的时间模型，所以本实现不带 tick。
- **`FunctionActive.active_tick`**：它出现在 `FunctionPoints` 之后，同样依赖时间模型。
- **一次击杀多名士兵**时各 tick 的先后：样本未覆盖。
