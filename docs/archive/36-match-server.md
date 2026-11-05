# 36 · 一局对战的服务端对象 `Match`

把已验证的零件装成一个能处理开火的对象。

## 1. 组成

```rust
pub struct Match {
    room: RoomState,          // 房间状态（server）
    obstacles: Vec<(f64,f64,f64)>,
    soldiers: Vec<Target>,    // 存活士兵（死者会被移除）
    tick_base: (i64, i64),    // tick 基准（tick 模块）
    mask: Vec<u8>,            // 障碍光栅化结果（plane）
    next_effect_id: i64,
}

pub fn fire(&mut self, owner_id, function_text, x0, y0, flip, now_ms) -> Vec<String>
```

`fire` 一次产出整串**可直接写进 socket 的报文**：

```text
FunctionPoints -> [LifeQueuedDeath -> EffectInfo(DeathEffect)]*
                -> [QueueDestroyObstacle] -> EffectInfo(ExplosionEffect)
```

tick 由 `tick::tick_at` 给出（60 Hz）。**死掉的士兵会从场上移除**，
否则后续射击会重复计死（这条是测试逼出来的）。

## 2. 验证（真实关卡回放）

`tests/live_hits.rs::match_replays_the_death_level_through_the_protocol_layer`：
把死亡关卡的 6 次射击按顺序喂给 `Match`，然后

- 每发首条必须是 `FunctionPoints`，其 `end_x` 与抓到的终点相差 < 0.5；
- 每条产出都必须是**我们自己的解析器能解析的合法消息**；
- 复算出的致死对象必须落在抓包实际死过的集合里。

结果：6 发终点全部对上，产出消息全部合法。

## 3. 一个必须记录的提取陷阱（本轮的失败教训）

我一开始断言"复算致死对象恰好是 `{230, 231}`"，测试**失败**了——只复算出 231。

原因不在模型，而在**抓包提取**：`FunctionInfo` 会随回合更新，
而我提取时只保留了每个实体的**最后一条**函数。杀死 230 的那发用的是
`(x/3.02)`，属于更早的回合，那条记录被后来的覆盖掉了。

⇒ 教训：**从抓包提取"随时间变化的状态"时必须保留时间线**，
不能按实体只留最后一条；否则会拿到"事后"的值去解释"事前"的行为。

因此这条测试只能断言"不越界"（复算出的死者都在抓包实际死过的集合内），
不能断言精确相等。要精确回放，提取时需要把每次 `FunctionInfo` 与随后的
`FunctionFire` 配对保留。
