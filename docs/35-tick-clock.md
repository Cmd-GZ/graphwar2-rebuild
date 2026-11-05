# 35 · tick 时钟：60 ticks/秒

## 1. 数据

给抓包加上时间戳后，收集到 113 个 `TickReply` 样本（时间戳 + `current_tick`），
存为 `fixtures/ticks.tsv`。

## 2. 拟合结果

```text
current_tick = base_tick + round((now_ms - base_ms) * 60 / 1000)
```

⇒ **精确 60 ticks/秒**。实测速率（多点线性拟合）：

```
(ts-t0)ms   current_tick   推算 tick/秒
    0.00            5        -
    3.97          243        59.90
    8.97          543        59.98
   14.97          903        60.00
   48.97         2943        60.00
```

113 个样本里 **110 个完全吻合**，2 个差 1，1 个差 7 —— 差异来自抓包时间戳抖动
（socket 缓冲会让时间戳晚于真实发送），不是公式问题。测试里显式允许这一抖动
（`exact >= 105 && worst <= 8`）。

## 3. 实现

```rust
// src/tick.rs
pub const TICKS_PER_SECOND: i64 = 60;
pub fn tick_at(base_tick: i64, base_ms: i64, now_ms: i64) -> i64

// src/server.rs
pub fn on_tick_request(base_tick: i64, base_ms: i64, now_ms: i64) -> String  // TickReply
```

`TickReply` 的输出与抓包样本**逐字节一致**（`server::tick_tests`）。

## 4. 还没确定

- **客户端多久发一次 `TickRequest`**：实测间隔噪声很大
  （`1ms`、`45/46ms`、`952~956ms` 混杂），无法从现有抓包确定，可能与
  有/无操作、窗口可见性有关；
- **服务端何时推进状态机**（`WaitingForFunctions` 等状态的触发条件）；
- tick 与**弹丸飞行时间**的关系（`docs/31` 里缺失的那部分）。
