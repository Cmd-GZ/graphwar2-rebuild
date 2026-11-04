# 13 · 回合（graphwar_turn_manager.rs）

状态：**结构已探明，语义未还原**。本章的结论都停在"能指到地址"的层面。

## 1. 模块规模（观测）

Ghidra 边界下 `graphwar_turn_manager.rs` 有 **14 个函数**，全部是小函数
（33～236 行），已在同一批反编译里拿到：

| 虚地址 | 行数 | 直接调用数 |
|---|---|---|
| `0x4628d0` | 236 | 4 |
| `0x4627c0` | 83 | 4 |
| `0x462c10` / `0x462ec0` | 68 / 68 | 6 / 6 |
| `0x462d20` / `0x462fe0` | 81 / 80 | 8 / 8 |
| `0x463170` | 89 | 8 |
| `0x463340` / `0x463460` | 67 / 67 | 7 / 7 |
| `0x463580` / `0x463790` | 85 / 69 | 10 / 7 |
| `0x463da0` | — | 本批未覆盖（在 `docs/10` 里作为 life_manager 的调用者出现） |
| `0x46f690` | — | 位置孤立，未与本批一起反编译 |

## 2. 谁驱动它（观测）

外部调用者 22 个，按模块：

| 模块 | 调用者数 | 说明 |
|---|---|---|
| `graphwar_game_state_manager.rs` | 5 | 主要驱动方 |
| `graphwar_target_manager.rs` | 1（3 次调用） | 又一次出现在关键路径上 |
| `graphwar_mode_manager.rs` | 3 | 模式切换会动回合 |
| `graphwar_play_screen_control.rs` | 1（2 次） | 界面层 |
| `graphwar_server_connection_handler.rs` | 1（2 次） | 服务端连接 |
| `graphwar_game_client.rs` | 1 | 客户端 |

`target_manager` 已经是第三次在关键时刻出现了（前两次：写生命状态、间接调用
`calculate_path`）。它看起来是**玩法核心的十字路口**，值得单独开一轮。

## 3. 一个干净的负结果（观测）

**这 14 个函数里没有任何浮点字面量**（连 `1000.0` 这类毫秒换算都没有）。

含义：回合时限、超时判定这类参数**不在代码里硬编码**。与协议里的
`TurnLimitInfoEvent { turn_limit }`、`TimeModeInfoEvent { time_mode }` 一致——
它们是**数据驱动**的，由服务端下发。

这条负结果能省掉一类死路：不必再在 `turn_manager` 里找"每回合多少秒"的常量。

## 4. 内部形态（观测，语义未知）

最大的 `0x4628d0` 通篇是 **HashMap 探测**：以 `0x28` 字节为步长的开放寻址循环
（`*(int *)(lVar12 + -0x28 + uVar11 * -0x28)` 这类模式），外层套 `RwLock`。
这与 `docs/10` 的 life_manager、`docs/12` 的 evolutionator 是同一套路子：
**这个游戏的模块普遍是"锁 + 哈希表"，没有明显的面向对象层次**。

所以"回合"在这里不是一个大对象，而是若干张表之间的字段更新。
要把语义读出来，得先把那张 `0x28` 步长的表结构解出来——本轮没做。

## 5. 仍是未知

1. 谁决定"轮到谁"（`TurnInfoEvent { soldier_id }` 的生产者）
2. 回合结束的触发条件（时间？开火结算完？全员行动完？）
3. `TurnCountInfoEvent { turn_count }` 的递增点
4. 与 `mode_manager` 的交互：`TurnModeInfoEvent { turn_mode }` 改变什么行为
5. 胜负判定在哪（协议里有 `Team 1/2 Victory!` 文本，但没有找到判定函数）

## 6. 下一轮建议

`target_manager` 三度出现在核心路径上（写生命、算曲线、驱动回合），
从这里切入的收益比继续读回合表更高。
