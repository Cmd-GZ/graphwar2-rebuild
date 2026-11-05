# 44 · `DestroyObstacle` 的延迟：≈165 tick

## 1. 为什么会有延迟

抓包里障碍被击中时先发 `QueueDestroyObstacle`（带 tick），
过一段时间才发 `DestroyObstacle`。对应游戏里大概是"爆炸动画播完再真正移除"。

## 2. 实测（三对配对样本）

用时间戳把同一坐标的 `QueueDestroyObstacle` 与 `DestroyObstacle` 配对：

| 队列时的 tick | 实测间隔 |
|---|---|
| 277 | 2.783 s ≈ **167.0 tick** |
| 277 | 2.733 s ≈ **164.0 tick** |
| 277 | 2.732 s ≈ **163.9 tick** |

均值 **165 tick ≈ 2.75 秒**，三次非常一致，所以取常量 `165`。

## 3. 实现

```rust
pub const DESTROY_DELAY_TICKS: i64 = 165;

// Match
pending_destroy: Vec<(i64 /*到期 tick*/, String /*报文*/)>,
pub fn tick(&mut self, now_tick: i64) -> Vec<String>   // 发出所有到期的 DestroyObstacle
```

`fire()` 在被阻挡时，除了挖洞，还会把 `DestroyObstacle` 排进队列（`now + 165`）；
`tick()` 在推进时把到期的发出去，且不会重复发。

## 4. 测试

`game::tests::destroy_is_delayed_by_measured_ticks`：

- 刚排队时 `tick()` 不返回任何东西；
- `到期 - 1` 仍不返回；
- `到期` 恰好返回一条 `DestroyObstacle`，`radius` 是 `0.5`；
- 再往后推进不重复发。

## 5. 还没做的

- **`TurnCountInfo`**：实测序列 `0,1,2,3,4,0`，回绕规则还没定
  （可能是"每 5 回合清零"，也可能是"每轮多少回合"，样本不足以判定）；
- `TickReply` 的**客户端节拍**（服务端只是被动应答，这部分已经实现）。
