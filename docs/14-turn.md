# 14 · 回合：tick 时钟、状态机、同时回合制与轮转

> **本篇解决什么问题**：一局在时间轴上怎么走——tick 频率、回合状态机有哪几个状态、
> 回合什么时候截止、障碍摧毁为什么延迟、回合怎么计数与结束、同时回合制（SimultaneousTurns）
> 的结算顺序与广播分组、以及服务端怎么把"轮到谁"和 AI 接起来。
>
> **本篇合并自**：

| 原文件 | 内容 | 归档 |
|---|---|---|
| `35-tick-clock.md` | 60 ticks/秒（113 个真实样本） | [`archive/35-tick-clock.md`](archive/35-tick-clock.md) |
| `41-turn-state-machine.md` | 6 状态 7 事件，13 次迁移 | [`archive/41-turn-state-machine.md`](archive/41-turn-state-machine.md) |
| `42-end-of-turn.md` | `end_of_turn_tick` = 回合截止时刻 | [`archive/42-end-of-turn.md`](archive/42-end-of-turn.md) |
| `44-destroy-delay.md` | `DestroyObstacle` 延迟 ≈165 tick | [`archive/44-destroy-delay.md`](archive/44-destroy-delay.md) |
| `45-turn-count-and-ending.md` | `TurnCountInfo` / `EndingGame` | [`archive/45-turn-count-and-ending.md`](archive/45-turn-count-and-ending.md) |
| `86-simultaneous-turns.md` | 同时回合制的击杀结算顺序 | [`archive/86-simultaneous-turns.md`](archive/86-simultaneous-turns.md) |
| `141-sim-turns.md` | 同时回合制三段流程 / 广播分组 / 对拍 | [`archive/141-sim-turns.md`](archive/141-sim-turns.md) |
| `117-server-wiring.md` | 服务端接线：射手轮转 + 按士兵 AI 进化 | [`archive/117-server-wiring.md`](archive/117-server-wiring.md) |
| `13-turns.md` | 转发页（正文早已在 `archive/13-turns.md`，事实并入 `02` §13.3） | — |

报文**顺序**与事件枚举在 [`07-protocol.md`](07-protocol.md)；射手由谁定（AI 不选）在
[`13-ai.md`](13-ai.md) §8.4；轮转的内核实现见本文 §8。证据标记沿用四档。

## 1. tick 时钟：60 ticks/秒

### 1.1 数据

给抓包加上时间戳后，收集到 113 个 `TickReply` 样本（时间戳 + `current_tick`），
存为 `fixtures/ticks.tsv`。


## 2. 回合状态机：6 状态 / 7 事件

### 2.1 观测到的序列（真实对局，3 回合）

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


## 3. `end_of_turn_tick`：回合截止时刻

### 3.1 先确认频率相同

用抓包时间戳对它做回归：

```
Δt=9.06s  Δtick=547  → 60.4 /s
Δt=4.13s  Δtick=248  → 60.0 /s
Δt=4.36s  Δtick=265  → 60.8 /s
```

**和 `current_tick` 一样是 60 Hz**，所以差的不是频率，而是**基准/含义**。

### 3.2 真正的含义：回合截止

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

### 3.3 实现

```rust
// src/graphwar_tick_handler.rs
pub fn turn_duration_ticks(time_mode: &str) -> Option<i64>
pub fn end_of_turn_tick(turn_start_tick: i64, time_mode: &str) -> Option<i64>
```

#### 3.3.1 四档实测（改 harness 的回合时间按钮抓到的）

| 模式 | 实测 `end - 当时 current_tick` | 应为 | |
|---|---|---|---|
| `Timer1m` | 3620 / 3659 | 3600 | ✓ |
| `Timer2m` | 7227 / 7205 | 7200 | ✓ |
| `Timer3m` | 10820 … 10859 | 10800 | ✓ |
| `Timer5m` | 18019 / 18041 | 18000 | ✓ |

差值总是比标称值大 20~60 —— 因为抓包时刻比"回合真正开始"晚一点，与前面的解释一致。

#### 3.3.2 全部档位（按键循环实测）

把回合时间按钮按 0~6 次，得到的循环顺序是：

```
Timer1m → Timer2m → Timer3m → Timer5m → TimerInf → Timer30s → (回到 Timer1m)
```

| 模式 | 实测 `end - 当时 current_tick` | 应为 |
|---|---|---|
| `Timer1m` | 3620 / 3619 | 3600 |
| `Timer2m` | 7227 | 7200 |
| `Timer3m` | 10820 / 10819 | 10800 |
| `Timer5m` | 18019 | 18000 |
| `Timer30s` | 1819 | 1800 |
| `TimerInf` | **−66**（当时 cur=66 ⇒ 线上值为 `0`） | 0（哨兵） |

⇒ **六档全部实测**。`TimerInf` 用 `0` 表示"没有截止"，
所以实现里 `end_of_turn_tick()` 返回 `None`，写报文时用 `end_of_turn_tick_wire()` 得到 `0`。


## 4. `DestroyObstacle` 为什么延迟 ≈165 tick

### 4.1 为什么会有延迟

抓包里障碍被击中时先发 `QueueDestroyObstacle`（带 tick），
过一段时间才发 `DestroyObstacle`。对应游戏里大概是"爆炸动画播完再真正移除"。

### 4.2 实测（三对配对样本）

用时间戳把同一坐标的 `QueueDestroyObstacle` 与 `DestroyObstacle` 配对：

| 队列时的 tick | 实测间隔 |
|---|---|
| 277 | 2.783 s ≈ **167.0 tick** |
| 277 | 2.733 s ≈ **164.0 tick** |
| 277 | 2.732 s ≈ **163.9 tick** |

均值 **165 tick ≈ 2.75 秒**，三次非常一致，所以取常量 `165`。


## 5. 回合计数与结束条件

### 5.1 `TurnCountInfo` 出现在回合结束

抓包里它总是紧跟在一串 `TickReply` 之后、下一个状态之前：

```json
{"TurnCountInfo":{"turn_count":4}}
{"GameStateInfo":{"game_state":"EndingGame"}}
```

序列实测为 `0, 1, 2, 3, 4`（每回合 +1）。

**回绕规则仍未观测到**——这一局到 4 就结束了，没有出现 `4 → 0`。
（更早的某次抓包里见过 `0,1,2,3,4,0`，但那次的 `0` 更可能是**重开新局**后的第一个计数，
而不是同一局内的回绕。样本不足以区分，所以**不下结论**。）

### 5.2 `EndingGame` 的触发：某队全灭

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


## 6. 同时回合制：几何并行、事件按飞行时间串行

### 6.1 实际规则（结论）

> **每回合开始时把所有存活士兵放上桌；每发的弹道独立算出；
> 弹丸按自己的速度沿弹道飞，飞到谁就把谁标死（并立刻从"可命中集合"里去掉）；
> 一个士兵只死一次，归属=最先飞到他身上的那颗弹丸。**

这既不是"按某个固定顺序串行"，也不是"各自独立、可以重复死"，
而是**按飞行时间串行**（时间顺序是弹道的函数，不是编号的函数）。

支撑数字：
* 死亡时刻 = 最早到达时刻：151 个死亡里，残差中位 **-0.4 tick**，
  **60%** 落在 ±2 tick 内，p90 12 tick（把停点算错的发排除后：n=130，中位 -0.5，**66%** 在 ±2 tick 内）；
* 每发速度 ≈ **0.29 单位/tick**（= 5.8 px/tick = 348 px/s ≈ 0.35 屏宽/秒）：
  82 发里 75 发能一对一配上爆炸事件，隐含速度中位 0.29、p10~p90 = 0.255~0.300；
* 同一回合所有发的 `active_tick` **完全相同**（13/13）⇒ 确实是"同时起飞"。

### 6.2 证据等级

| 结论 | 等级 | 依据 |
|---|---|---|
| 弹道采样 / 掩膜 / 修正常数 | **逐位验证** | `15-target-life.md`/`84`；本次另证：**93 发的修正常数串与 replay 记录的 `corrected_function` 93/93 逐字符相同**；且**实况步长 0.01 下顺序回合制对照局 11/11 回合、31/31 死亡完全一致**（§3.2） |
| 实况步长 = 0.01 | **观测（强）** | 82 发里曲线完整的 63 发，停点与游戏 `end_x` **中位差 0.000000**（0.5 时 0.13）；与 `06-verification.md` 独立一致 |
| 死亡集合 = 逐发独立并集（4 局 13 回合） | **观测（强）** | 修复采样器后，实况 0.01 下 **152/152、多报 0 漏报 0**（**§14**）；修复前 0.01 是 129/152（漏 23），那 23 个漏报**全部可归因到被截断的曲线**（§5.1） |
| 死亡时刻 = 最早到达 | **观测（强）** | 0.5 下 34 例可判别 **34:0**（中位误差 1.55 vs 105.2 tick）；修复后 0.01 下 **38 例 38:0**（§14，剔除了截断干扰） |
| 不存在任何固定串行顺序 | **观测** | 3 个偏序约束环 + 6 种候选顺序 ≤54%（均基于 0.5 的完整弹道） |
| 一个士兵一回合只死一次 | **观测（强）** | 152/152 唯一死亡特效；4 发命中者仍只 1 条消息 |
| "弹丸按恒定速度沿弹道飞" | **推断** | 用爆炸 tick 反推的速度在 0.15~0.45 内一致；缺"逐 tick 位置"的直接观测（要 hook 才拿得到） |
| 内部实现细节（每 tick 查位置 / 死者出表） | **推断** | 与所有观测一致，但没有反汇编/运行时证据；缺的是 `life_manager` 的逐 tick 逻辑 |
| 两发**同时**到达同一士兵（并列）时谁赢 | **未知** | 61 个多命中样本里，同一士兵两发到达时刻的最小间隔是 **4.0 tick**（4702 回合 2 的 1055：15702 vs 15706，观测 15700）—— 样本里没有真正的并列 |

### 6.3 对实现的影响（**没有改物理**）

* `graphwar_shot_calculator::settle::resolve`、`src/graphwar_path_calculator.rs`、`src/plane.rs`、`src/graphwar_shot_calculator.rs` **不需要改**：
  "每发独立算"正是现在的行为，而真游戏的死亡集合也确实是并集。
* **要改的是事件生成**（`graphwar_shot_calculator::settle::events` / `src/game.rs::Match::fire`）：
  现在每发各自产出 `LifeQueuedDeath`，如果以后要做"一个回合多发同时结算"，
  需要 (a) 先算完所有发的弹道与命中，(b) 把 (到达 tick, 受害者) 按 tick 排序，
  (c) **每个受害者只产出一次死亡事件**，(d) 同一受害者被多发的其余命中直接丢弃。
  依据：§5.2 的 152/152 与 §4.2 的 34:0。
* 这次**没有改动任何 `src/` 代码**，也没有改测试。


## 7. 同时回合制的三段流程与广播分组

### 7.1 一句话

> 同时回合制的一批结算**不是**"每发一个完整窗口"，而是**一个事务**：
> 前缀 `FunctionInfo ×N` / `FunctionPoints ×N` **按类型分组**，死亡块**按弹丸飞行时间排序**，
> 后缀 `(FunctionActive + EffectInfo(爆炸)) × N` **每发一对**。
> 这三条在 130 个真回放多发包上**全部对上**（剔掉死亡块后 130/130，含死亡块 93/130，
> 剩下 37 个全部由"死亡集合 / 灰烬特效"这两个**别的**缺口造成，没有一个是分组错）。
> 实现上 `Match::fire_batch` 一次产出一个批；`Match::fire` 退化成 `N = 1`，
> 单发报文**逐字节不变**（`cargo test` 有回归用例）。
> 对拍上：同时回合制的**消息种类序列**从 **5/108（4.6%）→ 77/108（71.3%）**，
> **多发批的批结构从 0/130 → 130/130**。

---

### 7.2 完整流程（三段）

真回放的形状（`观测`，`1793884702` / `1793883985` 每一回合都相同）：

```text
                    ┌─ 收集阶段 ────────────────────────────────────────────┐
                    │ FunctionInfo(owner, "", null) × 全部士兵   ← 函数槽复位 │
                    │ TurnCountInfo{turn_count}                              │
                    │ TurnInfo{player_id, soldier_id} × P    ← 每人一条，同 tick │
                    │ EndOfTurnInfo{0, time_mode} × 1        ← **只有一条**    │
                    │ GameStateInfo(WaitingForFunctions) × 1                 │
                    │                                                        │
                    │ （各客户端异步交 FunctionUpdateRequest / FunctionFire）  │
                    └────────────────────────────────────────────────────────┘
                    ┌─ 结算阶段 ────────────────────────────────────────────┐
                    │ GameStateInfo(CalculatingResults)                       │
                    │ FunctionInfo(owner, 原文, 修正后) × N   ← **按类型分组**  │
                    │ FunctionPoints(owner, 起点, end_x) × N  ← **按类型分组** │
                    │ GameStateInfo(SynchronizingResults)                     │
                    └────────────────────────────────────────────────────────┘
                    ┌─ 广播阶段 ────────────────────────────────────────────┐
                    │ [EffectInfo(死亡/灰烬)] × D            ← **按飞行时间排序** │
                    │ [QueueDestroyObstacle] × B             ← 回放不记录      │
                    │ (FunctionActive(owner, active_tick)                     │
                    │  + EffectInfo(爆炸)) × N               ← **每发一对**    │
                    │ GameStateInfo(ExecutingResults)                         │
                    └────────────────────────────────────────────────────────┘
                    延迟段（下一回合里陆续到达）：
                    │ LifeInfo{entity_id, alive:false} × D    ← 真正的死亡时刻  │
                    │ DestroyObstacle × B                     ← 约 165 tick 后 │
```

关键数字（逐条见 §2–§4）：

| 事实 | 数 | 档 |
|---|---|---|
| 多发批里 `\|FunctionInfo\| = \|FunctionPoints\| = \|FunctionActive\| = \|爆炸特效\| = N` | **130 / 130** | 观测 |
| 三种消息的 `owner_id` 序列**完全相同** | **130 / 130** | 观测 |
| 死亡块**全部**落在 `SynchronizingResults` 之后、第一个 `FunctionActive` 之前 | **87 / 87**（有死亡块的批） | 观测 |
| 死亡块按 `start_tick` **升序** | **106 / 106** | 观测 |
| 前缀与后缀的 owner 顺序 = **本回合 `TurnInfo` 的公告顺序**（取实际提交的那些） | **130 / 130** | 观测 |
| 同一回合所有 `FunctionActive.active_tick` 相同 | 13 / 13 回合（`14-turn.md` §6） | 观测 |

#### 7.2.1 实现落点

| 层 | 类型 / 函数 | 做什么 |
|---|---|---|
| 收集 | `graphwar_turn_manager::RoundCollection<T>` | 公告名单 → 攒提交 → **攒齐或超时**即可结算；顺序 = 公告顺序 |
| 收集 | `game::Match::begin_simultaneous_turn` | 每人一条 `TurnInfo` + **一条** `EndOfTurnInfo` + **一条** `WaitingForFunctions`；返回每人的 `TurnStart`（含 AI 进化结果） |
| 结算 + 广播 | `game::Match::fire_batch(&[Submission])` | 一次产出**整个结算批**；`fire()` 现在是它的 `N = 1` 特例 |
| 几何 | `graphwar_shot_calculator::settle::ShotOutcome.hits: Vec<Hit>` | 每次命中带 `arc`（沿弹道弧长）与 `index`，用来给死亡块排序 |
| 接线 | `graphwar_game_server::start_sim_round` / `resolve_sim_round` | `Config::turn_mode = "SimultaneousTurns"` 时走这条路；`tests/sim_turns.rs` 有一条真 TCP e2e |

`SequentialTurns` 一个字节都没变：`Match::fire` 改成 `fire_batch(&[一发])` 之后，
单发批的报文种类序列、`effect_id` 序号、坐标与之前**完全相同**
（`tests/sim_turns.rs::single_shot_is_byte_identical_to_fire` 是回归用例，
`cargo test` 里 273 条 lib 测试也一条没动）。

---

### 7.3 广播阶段：分组与顺序

#### 7.3.1 规则（`观测`，130 个多发批全中）

> **前缀按类型分组，后缀按发交替，死亡块按飞行时间排。**

1. `FunctionInfo × N` —— 连排，**同一顺序**；
2. `FunctionPoints × N` —— 连排，**与①同一顺序**；
3. 死亡特效块 —— `EffectInfo(DeathEffect)` 与 `EffectInfo(AshEffect)` **混在一起**、
   **一个连续的块**，按 `start_tick` 升序（106/106）；
4. `(FunctionActive(owner_i), EffectInfo(ExplosionEffect))` —— **每发一对**，
   顺序与①②相同（130/130）；
5. `QueueDestroyObstacle` 的位置**回放里没有证据**（`06-verification.md` §1.2：400 份里 0 次），
   本实现沿用单发实现的"死亡块之后、`FunctionActive` 之前"，这样 `N = 1` 时与旧代码逐字节相同。

#### 7.3.2 "谁排前面"：`TurnInfo` 的公告顺序（**不是** A 类的自由处置）

一开始我也以为这是一个 HashMap 迭代序（A 类，`12-rng.md`），**实测否掉了这个偷懒结论**：

| 候选规则 | 命中 |
|---|---|
| 士兵 id 升序 | 44 / 130 |
| 士兵 id 降序 | 55 / 130 |
| 开火请求到达序（`FunctionFire`） | 0（`1793883985` 的发 0 到达最早、批里排第 7） |
| 到达时刻序（爆炸 `start_tick`） | 0（同一批的爆炸 `start_tick` 是 32861/32953/…，与批序无关） |
| **本回合 `TurnInfo` 公告顺序 ∩ 实际提交者** | **130 / 130** |

⇒ **规则定出来了**：批内射击者的顺序 = **本回合每人一条的那组 `TurnInfo` 的顺序**，
去掉 `soldier_id == 0`（无兵可开火）的、去掉**公告了但没提交**的。

> 这条同时解释了对拍里 9 个"看起来对不上"的批：`TurnInfo` 公告了 3 个人、结算批只有 2 发
> （例：`1782642661` 公告 `[153655, 153990, 153430]`，批里只有 `[153655, 153430]`）。
> **不是顺序不同，是有人没交**。

**残余的 A 类**只有一层：**`TurnInfo` 那组本身的顺序**来自玩家表的 HashMap 迭代序
（`12-rng.md` §1.1 已实测：钉住 `getrandom(16)` 后跨进程一致）。本实现不碰它——
`begin_simultaneous_turn` **照抄调用方给的 `players` 顺序**，接线层（`graphwar_game_server`）给的是
`Config::player_order` 或玩家表的构造顺序。所以：

* **批内分组与顺序：规则已定，130/130 复现**；
* **`TurnInfo`/批的玩家排列：A 类**，需要与 `12-rng.md` 同一份熵钉住才能逐局复现。

#### 7.3.3 死亡块里 `DeathEffect` 与 `AshEffect` 混排

106 个带死亡块的多发批里，`DeathEffect` 218 条、`AshEffect` 208 条，两者**交错**
（例：`AshEffect×1, DeathEffect×1, AshEffect×2`），但整体仍按 `start_tick` 升序。
**哪种情况发哪一个：未知**（缺 `EffectInfo` 发射路径的复核）。
当前实现**只发 `DeathEffect`**（与改动前一致），`AshEffect` 仍是 `06-verification.md` §4.3 记的缺口。

---

### 7.4 仍不确定的

| # | 未确定 | 缺什么 | 等级 |
|---|---|---|---|
| 1 | `TurnInfo` / 批内玩家排列的**具体置换** | 与 `12-rng.md` 同一份 `getrandom(16)` 熵；本实现照抄调用方顺序 | **A 类**（不可由输入决定） |
| 2 | **收集阶段的超时**：多久算超时、超时后那名玩家是否仍消耗一次轮转 | 二进制或抓包里的超时路径；`EndOfTurnInfo.end_of_turn_tick` 在真回放里**恒为 0**（`1793884702` 三回合），60Hz tick 已上万 ⇒ 那个字段不能当截止。`Config::sim_turn_timeout_ticks` 只留旋钮，**不猜数值** | **未知** |
| 3 | `DeathEffect` vs `AshEffect` 的选择规则 | `EffectInfo` 发射路径的复核；本轮只观测到"两者混排、整体按 `start_tick` 升序" | **未知** |
| 4 | `QueueDestroyObstacle` 在批里的位置 | 回放不记录这类消息（400 份 0 次）；抓包里能拿到但没做逐批对位 | **未知**（实现里显式声明是重建版规则） |
| 5 | **两发同时到达**同一士兵（真并列）谁赢 | 样本里最小间隔 4.0 tick，没有并列（`14-turn.md` §7） | **未知**；实现用 `(弧长, 发序号, id)` 兜底 |
| 6 | 弹丸飞行速度 | 只知道中位 ≈0.29 单位/tick（p10~p90 0.255~0.300）；**排序不需要它**，只有"绝对时刻"才需要 | **推断** |
| 7 | `active_tick` 的取值 | 观测到"同批所有发相同"，但绝对值来自哪个时钟没解；实现里用 `now_tick` | **推断** |
| 8 | 回合开头那两条 `FunctionInfo("", null) × 全部士兵` 与 `TurnCountInfo` | 只观测到形状，没有发射器；本轮**没有实现**（不在批窗口内，不影响对拍口径） | **未知** |
| 9 | `DiffEq` 模式的同时回合制 | `Match::fire_batch` 与 `fire` 一样硬编码 `Mode::Normal`；515 个 DiffEq 批本轮仍未覆盖 | **未知** |

---


## 8. 服务端接线：射手轮转 + 按士兵 AI 进化

### 8.1 一句话

> `13-ai.md` §6.1 留的尾巴（"规则已实现并可测 ≠ 端到端走新规则"）补上了：
> 回合开始走 `TurnManager::begin_turn`（三态），**先定射手、再只为它**跑一次
> `SoldierEvolver::evolve_soldier`，**25 s 预算默认接上**；`graphwar_game_server` 的
> "谁下一个开火"不再由它自己算。
> 仍然没接的：`token → 文本`（AI 的函数上不了线）、回合超时的**检测**、游标初值的**配置入口**。

---


## 9. 对局界面控制器 `graphwar_play_screen_control.rs`（10 个可归属函数）

> **本篇为什么有这一节**：回合推进 / 准备态 / 弹道生命周期 / 胜负时机这四件事，
> `client/src/screens/play_screen.gd` 里的写法是**照 `.tscn` 与截图重写的仿制品**，
> 时序没有证据（`AGENTS.md` §0 第 4 条），已实测出多处与原版不符。
> 原版的这些逻辑在 Rust 的 `graphwar_play_screen_control` 里。本节给出**截至目前的**逐函数规格。
>
> 证据口径：本节全部是 **观测**（能指到 vaddr / `.rodata` 串）或显式 **未知**。
> 没有"逐位"级结论——逐位对拍的对象是采样器/求值器这类纯函数。
> 地址一律 **ELF vaddr**；Ghidra/REA 地址 = 本地址 + `0x100000`。
> 实现落在 `src/graphwar_play_screen_control.rs`，回归用例在 `tests/play_control.rs`。

### 9.1 模块全景：10 个函数与它们之间的直接调用

函数边界取自 `analysis/funcstarts_ghidra_va.pkl`；`0x3d9b50 … 0x3dcbb0` 这 8 个
**首尾相接**（前一个的结束地址 = 后一个的开始地址），边界互相印证。

| # | vaddr | 大小 | 直接调用者（`analysis/insns.pkl` 全量扫） | 已解出的角色 | 档位 |
|---|---|---|---|---|---|
| 1 | `0x3d9000` | `0x3c0` | `0x3da130` ×2 | 取 `VictoryPlayerContainer1` / `SkinIcon`、`set_texture`、写 `f(x) = ` 标签 | 观测 |
| 2 | `0x3d9b50` | `0x2f0` | `0x3a5760` ×4、`0x3b0830` ×3 | 5 个节点置 `(0,0,0,1,1)` ⇒ **回放视图** | 观测 |
| 3 | `0x3d9e40` | `0x2f0` | `0x3b0830` ×4 | 同 5 个节点置 `(1,1,1,0,0)` ⇒ **对局视图** | 观测 |
| 4 | `0x3da130` | `0x12b0` | `0x3a5760` ×1 | **胜负横幅**（`TeamVictoryLabel` / `SelfVictoryLabel` / 头像） | 观测 |
| 5 | `0x3db3e0` | `0x1090` | `0x3da130` ×3 | 被胜负重绘调用 3 次，内部逻辑**未解** | 未知 |
| 6 | `0x3dc470` | `0x1c0` | `0x3b0830`、`0x3dc7f0` | 小整数键哈希表查找 + 取 `Gd<T>` | 推断 |
| 7 | `0x3dc630` | `0x1c0` | `0x3b0830`、`0x3dc7f0` | 同上（另一张表） | 推断 |
| 8 | `0x3dc7f0` | `0x3c0` | `0x3a2e10` ×1 | **订阅两个信号**，并造闭包（见 §9.2） | 观测 |
| 9 | `0x3dcbb0` | `0x2250` | `0x3a5760` ×1 | 模块内最大函数；含 `f(x) = ` 与 `LeftButtonGrid` 串，语义**未解** | 未知 |
| 10 | `0x522070` | `0x480` | `0x3dc7f0` ×1 | 取 `TimerLabel`/`ViewFunctionButton`/`ShowNames`/`PlayPauseButton`/`YButton` + `player_info_event` | 观测（角色推断） |

调用图（只画本模块相关）：

```text
0x3a2e10 (root_node) ──→ 0x3dc7f0 ──→ {0x522070, 0x3dc630, 0x3dc470}
0x3a5760 (root_node) ──→ 0x3d9b50 ×4, 0x3da130, 0x3dcbb0
0x3b0830 (root_node) ──→ 0x3d9b50 ×3, 0x3d9e40 ×4, 0x3dc470, 0x3dc630
0x3da130 ──→ 0x3d9000 ×2, 0x3db3e0 ×3
```

⇒ **整个模块没有一条直接调用进 `function_active_event` / `function_points_event` 的处理链**
（见 §9.3）。唯一只有一条行号信息：`src/graphwar_play_screen_control.rs:561`
（`.rodata` `0x1bf9a7` 紧跟 `ViewFunctionsLabel` 之后），被 `0x3dc7f0` 与 `0x3dcbb0` 引用。

### 9.2 它订阅哪些事件（问题 1）

`0x3dc7f0` 里 **`call *0x9bb030`**（GOT 槽，`Object::connect` 口径）只出现 **2 次**：

| 信号名 | `.rodata` | 长度立即数 | `lea` 点 | `connect` 调用点 |
|---|---|---|---|---|
| `function_info_event` | `0x1bf61a` | `0x13` = 19 | `0x3dc98b` | `0x3dc9a1` |
| `function_fire_event` | `0x1bf607` | `0x13` = 19 | `0x3dca26` | `0x3dca3c` |

同一函数还做两件事（观测）：
1. `get_node("ViewFunctionsLabel")`（`0x1bf9a7`，长 `0x12` = 18；调用 `0x3dc856` → `0x3ab790`），
   紧接着字符串 `"upcast_ref"`（`0x1bf7e7`，长 `0xa` = 10）→ 是把节点**上抛**成具体类型；
2. 调 `0x522070` 造闭包（`0x3dc8b4`），该闭包捕获 `TimerLabel` / `ViewFunctionButton` 等（§9.1 #10）。

**15 条远程事件名**在 `.rodata` 里是**一段连续字面量**（`0x1bf5b0` 起），整段只被
**`0x3d5260`** 一个函数引用——那是"名字 → 分派"的表，**不在本模块**。
本模块内部只引用了其中两个名字，且是 `connect` 形式。

⇒ **观测**：`play_screen_control` 直接订阅的只有 `function_info_event` 与 `function_fire_event`。
⇒ 这同时说明 **`TurnInfo` / `EndOfTurnInfo` / `GameStateInfo` 不是由这一层直接 connect 的**；
它们走 `0x3d5260` 那条名字表（另有模块收），本模块只是被 `root_node` 的
`0x3a5760` / `0x3b0830` **主动调用**（§9.1 调用图）。

⚠️ **槽已解、成员名未解**（2026-11 复核，`docs/151` §2.4）：

* `0x9ba978` → `R_X86_64_RELATIVE` addend **`0x8ab200`**（gdext 生成的方法包装，
  与 `draw_arc` 的包装 `0x8abca0` 同一段）。成员名见 `docs/16` §23.6（`CanvasItem::set_visible`，推断强）。
* `0x9bb030` → addend **`0x514b40`**，那是**游戏自己的函数**（落在客户端 `0x514xxx` 段），
  **不是** gdext 引擎成员 —— 原文把它和 `0x9ba978` 并列成"引擎侧成员名未解"，口径是错的。
  使用者：`0x3dc9a1`、`0x3dca3c`、`0x4b436d`、`0x4b4428`、`0x4b44e3`、`0x4b4595` …
本节的"`connect`"是依据"串名 + 长度 + 紧跟 callable 实参"三点做出的**强推断**，不是逐位结论。

### 9.3 弹道什么时候清掉（问题 3）——**否定证据**

`docs/05-shot-pipeline.md` 记着"飞完清"，但一直没找到二进制判定点。本轮给出的结论是**否定的**：

> **观测**：`graphwar_play_screen_control.rs` 的 10 个函数里，**没有一个**直接引用
> `function_active_event` / `function_points_event`（`analysis/xrefs.pkl` 全量，
> 该两串的引用点只有 `0x3d5260` 一处，在别的模块）。
> ⇒ "弹道清除"**不发生在这一层**。

那它在哪？本轮**未解出**。已排除/已收窄的范围：
* 不是 `play_screen_control`（本节）；
* `entity_drawer` 是绘制层，`docs/05` 已记其调用面；
* 剩下最可能的落点是 `root_node`（`0x3a5760` / `0x3b0830`）或 `entity_drawer`，
  但**必须**用 `function_points_event` 的**接收方**反查才能定罪。

缺的证据：`0x3d5260` 那张"名字 → 分派"表里，`function_points_event` 与
`function_active_event` 各自映射到哪个处理函数的**实参**（表项是重定位，
要用 `.rela.dyn` 的 addend 还原，`tools/ptr_slots.py` 可做）。

### 9.4 "准备完毕"怎么判（问题 2）

实测已知：**回合闭合要等 Fire**。本轮的二进制侧证据只到这一步：

* **观测**：本模块 `connect` 了 `function_fire_event`（§9.2）；
* **观测**：本模块**没有** `connect` 任何"输入框文本变化 / 定时器到期"类的信号
  （订阅表只有两条，见 §9.2）；
* **推断**：因此"准备完毕"**不可能**是本模块靠输入框非空推出来的——它能看到的只有
  `function_info_event`（函数信息回填）和 `function_fire_event`（开火）。
  与实测"闭合要等 Fire"一致。

⚠️ **未知**：`function_fire_event` 的处理体里到底判断了什么（是否校验"输入框非空"、
是否置 `FireContainer` 的可用态）。缺的证据：`0x522070` 里那 5 个节点
（`TimerLabel` / `ViewFunctionButton` / `ShowNames` / `PlayPauseButton` / `YButton`）
各自被调哪些 setter——`0x522070` 长 `0x480` 且以 GOT 间接调用为主，本轮未逐条解析。

### 9.5 胜负在哪里展示（问题 4）

**观测**：`0x3da130` 是本模块里唯一引用胜负字面量的函数，也是唯一被 `root_node`
（`0x3a5760`，`0x3a6fbf` 处）调用的"展示"函数。

它引用的 `.rodata`：

| 用途 | 字面量 | 地址 |
|---|---|---|
| 队胜利 | `Team 1 Victory!` / `Team 2 Victory!` | `0x1bf91e` / `0x1bf934` |
| 自己 | `YOU WON` / `YOU DID NOT WIN` / `YOU lOST`（**小写 L**） | `0x1bf92d` / `0x1bf943` / `0x19fcc8` |
| 平局 | `Tie!` | `0x1bb6d8` |
| 标签节点 | `TeamVictoryLabel` / `SelfVictoryLabel` | `0x19eb60` / `0x19da90` |
| 头像节点 | `SkinIcon` / `FaceIcon` / `HatIcon` | `0x19f3d0` / `0x19f180` / `0x1bf968` |
| 容器 | `VictoryPlayerContainer`（`+1` 变体） | `0x1bf952` / `0x1bf86a` |

它内部还调 `0x3d9000` ×2（重写 `f(x) = ` 标签、`set_texture`）与 `0x3db3e0` ×3。

⚠️ **未知**：**"哪种结局显示哪条"没解出来**。缺的证据：`0x3da130` 里那几个分支的
**输入**——是 `GameStateInfo` 的字段、`TurnCountInfo` 的计数、还是
`player_manager` 的队号。所以 `src/graphwar_play_screen_control.rs` 只把 6 条字面量与枚举写死，
**不实现选择逻辑**。

#### 与 `game_state_manager` 的 `state = 6` 怎么衔接 —— **未知**

本轮**没有**找到连接点。已知的边界事实只有两条：
* `0x3da130` 的唯一调用者是 `root_node` 的 `0x3a5760`（不是 `game_state_manager`）；
* `game_state_info_event` 那条名字**不在**本模块的订阅表里（§9.2）。

⇒ 衔接点在 `root_node`（`0x3a5760`）里，而 `0x3a5760`（`graphwar_root_node.rs`）
本轮**未解**。缺的证据：`0x3a6fbf` 那处调用前的比较指令（拿什么跟 `6` 比）。

### 9.6 它和 `function_input` / `entity_drawer` 的边界（问题 5）

* **观测**：本模块只碰**节点名**（`§9.1` 的 20 个名字）与**信号名**（2 条），
  不碰任何几何（没有 `draw_arc` / `draw_line` 的调用，没有平面常量）。
* **推断**：因此"画什么"在 `entity_drawer`，"输入框里是什么"在 `function_input`
  那个子类；本模块只做**开关与文本**：谁可见（§9.7）、标签写什么（`f(x) = `、胜负）、
  订阅哪两个事件（§9.2）。
* ⚠️ **未知**：`0x3dcbb0`（`0x2250`）与 `0x3db3e0`（`0x1090`）两个大函数没解，
  它们完全可能包含与 `function_input` / `entity_drawer` 的交互。**不要**把上面这条
  当成"边界已定"。

### 9.7 唯一完整解出的一对函数：`0x3d9b50` / `0x3d9e40`（控件模式）

两个函数**结构完全相同**（同样 5 次 `get_node` → 上抛 → `call *0x9ba978`），
唯一差别是传入的布尔值**恰好互补**：

| 节点 | `get_node` 调用点（`0x3d9b50`） | `0x3d9b50` 的 `esi` | `0x3d9e40` 的 `esi` | 指令地址（`0x3d9b50` / `0x3d9e40`） |
|---|---|---|---|---|
| `LeftButtonGrid` | `0x3d9b94` | **0** | **1** | `xor %esi,%esi` @`0x3d9be2` / `mov $0x1,%esi` @`0x3d9ecf` |
| `RightButtonGrid` | `0x3d9c16` | **0** | **1** | `0x3d9c64` / `0x3d9f51` |
| `FireContainer` | `0x3d9c98` | **0** | **1** | `0x3d9ce6` / `0x3d9fd3` |
| `LeftReplaysButtonsGrid` | `0x3d9d1a` | **1** | **0** | `mov $0x1,%esi` @`0x3d9d65` / `xor` |
| `RightReplaysButtonsGrid` | `0x3d9d99` | **1** | **0** | `0x3d9de3` / `xor` |

⇒ 语义（**推断**，方向唯一）：`0x3d9b50` = **回放视图**（对局控件关、回放控件开），
`0x3d9e40` = **对局视图**（对局控件开、回放控件关）。
实现：`src/graphwar_play_screen_control.rs::UiMode::{Play, Replay}` + `widget_flags()`。

⚠️ **推断**的部分：`*0x9ba978` 收一个 `bool`，作用于刚取到的节点——但成员名
（`set_visible` / `set_disabled` / `set_process`）**没有**从 x86 侧直接确认。
槽的 addend 已实测 = **`0x8ab200`**（`docs/151` §2.4）；成员名靠 `docs/16` §23.6 的
arm64 侧 `bl` 证据，档位**推断（强）**。
⚠️ 顺带更正：同一句里被并列的 `0x9baa10`（`docs/16` §15.9）**不是** GOT 槽里的引擎成员，
它是 `R_X86_64_GLOB_DAT` 的 **`bcmp@GLIBC_2.2.5`**（libc）。

### 9.8 进了 Rust 的部分 / 仍是未知

| 进 `src/graphwar_play_screen_control.rs` | 判据地址 |
|---|---|
| `UiMode::{Play, Replay}` + `widget_flags()` + `widgets()` + `flag_of()` | `0x3d9b50` / `0x3d9e40`（§9.7 全表） |
| `SUBSCRIBED_SIGNALS` + `subscribed_remote_events()` | `0x3dc98b` / `0x3dca26`（connect 实参） |
| `REMOTE_EVENT_NAMES`（15 条，只记顺序不记判别值） | `0x1bf5b0` 起的连续字面量 |
| `VICTORY_TEXTS` + `VictoryBanner::text()`（**只做字面量映射**） | `0x1bf91e` … `0x1bb6d8` |
| `PLAY_WIDGET_NODES` / `REPLAY_WIDGET_NODES` / `OTHER_NODES` / `ACTION_NODES` | §9.1 的各 `get_node` 点 |
| `PANIC_LOCATION = ("src/graphwar_play_screen_control.rs", 561)` | `0x1bf9a7`+18 |

**仍是未知（按重要性）**：

1. **胜负条件 → 横幅**的映射（§9.5）——缺 `0x3da130` 分支输入的解析；
2. **`state = 6` 的衔接点**（§9.5）——缺 `0x3a5760`（root_node）的解；
3. **弹道清除的真实判定点**（§9.3）——缺 `0x3d5260` 名字表的重定位还原；
4. **`0x3dcbb0` / `0x3db3e0` 两个大函数**（合计 `0x32e0` 字节）**完全没解**；
5. **GOT 槽 `0x9ba978` / `0x9bb030` 的引擎侧成员名**——缺运行时观测；
6. **函数指针派发**：本节所有"没有引用 X"都是**直接调用**口径，
   未用 `tools/ptr_slots.py` 复查 `play_screen_control` 的函数有没有被塞进表里。

> 本轮**没有**改 `client/`、`src/graphwar_path_calculator.rs`、`src/_scaffold/protocol.rs`、`src/game.rs`。
> 只新增 `src/graphwar_play_screen_control.rs`、`tests/play_control.rs`，`src/lib.rs` 加一行 `pub mod`，
> 以及本节。

---

## 10. `0x3dcbb0` / `0x3db3e0` 规格（本轮新增：play_screen_control 的两个大函数）

> **本节解决什么**：`docs/14` §9.1 第 5、9 号函数（`0x3db3e0` 0x1090 B、`0x3dcbb0` 0x2250 B）
> 上一轮是**未知**；`docs/00` §0.4 把它列成"下一轮最高性价比"第 2 项。
> 本节给出两个函数的**可照写规格**，并回答一个关键否证：**`0x3dcbb0` 不是「回合推进 /
> game_state 分派」**。
>
> **地址口径**：ELF vaddr；Ghidra/REA = 本地址 + `0x100000`。两处入口都用
> REA `procedure_info` 与 `analysis/funcstarts_ghidra_va.pkl` 双查过（返回的
> `mapped_address` 与请求一致，不是"文件偏移回落"的假结果）。

### 10.1 先给结论（三句话）

1. **`0x3dcbb0` = 「把一份数据刷到对局界面」的聚合刷新函数**，不是状态机。
   证据：全函数 1491 条指令里**没有任何一处**把某个枚举字段与状态值比较
   （`cmp` 只有空指针判定与 `cmp dword [rax+0xe0],1`、`cmp dword [rax],1` 两处布尔式判定），
   也**没有引用任何远程事件名**（见 §10.3 的字面量表）。
2. **`0x3db3e0` = 往胜利横幅里填一名玩家的头像/皮肤**（`set_texture` ×N），
   `0x3da130` 连调 3 次 ⇒ 3 个展示位。
3. **真正的"状态→界面"反应点在 `root_node 0x3a5760`、`graphwar_game_state_manager` 与
   `graphwar_remote_event_handler 0x3dee00`**，不在本模块（§10.5 给全证据）。

### 10.2 `0x3dcbb0`（观测）

```text
vaddr 0x3dcbb0   Ghidra 0x4dcbb0   Ghidra 记 length 0x1CB8（7 352 B）
body 分 8 段（0x4dcbb0-0x4dd4c0 / 0x4dd4d0-0x4dd8e6 / 0x4dd8f0-0x4dd960 /
 0x4dd970-0x4ddafd / 0x4ddb00-0x4de842 / 0x4debdd-0x4debf9 /
 0x4dec47-0x4dec62 / 0x4dedcb-0x4deddf），span 0x2230 ⇒ 有跳转表孤岛
299 个基本块 · 54 个已解析被调 · 10 处未解析间接调用 · 末尾 0x3deddb 是 call 0x94b340（panic，Ghidra 标 no-return）
```

**入参**（观测，实参来自唯一调用点 `0x3a6aaf`）：

| 寄存器 | 含义 |
|---|---|
| `rdi` | `&mut T_a` —— 指向 `[[self+0x16c8]+0x18]`（`RefCell` 的载荷，**独占借用**：`0x3a6a77 mov qword[r14+0x10],-1`） |
| `rsi` | `&T_b` —— 指向 `[[self+0x1668]+0x18]`（**共享借用**：`0x3a6aa7` 里 `+1`） |
| 返回 | `void`（无返回值使用点） |

⇒ **推断（结构证据强）**：`root_node` 在 `+0x1668` / `+0x16c8` 各持一个
**`Rc<RefCell<T>>`**：`+0x0` 强引用计数、`+0x8` 弱引用计数、`+0x10` `RefCell` 借用标记
（`-1` = 独占、`n` = 共享 n）、`+0x18` 载荷 —— 这个四段布局在 `0x3a6a60`-`0x3a6ab8`
被完整用到，且同一形状在 `0x3a2e10`（`_ready`）里被写入（`0x3a3249`、`0x3a3df2`）。
缺的证据：`T_a` / `T_b` 的**具体类型**（反编译里只看得到 `[T_b+0x1e0]`、`[T_b+0x220]`
两个字段；`0x1e0` 判空后若为 0 直接跳到 `0x3de38e`）。

**函数体开头（观测）**：

```text
0x3dcbc7  call *0x9ba628                       ; 引擎/hash 相关
0x3dcbcd  edi=0x27, esi=1 ; call *0x9ba630     ; 分配 39 字节
0x3dcbe9  movups [rax+0x10], "y_screen_control.rs:561"   (0x1bf9c9)
0x3dcbf4  movdqu [rax],   "src/graphwar_play_screen_control.rs:561" (0x1bf9b9)
0x3dcc00  movabs rax, 0x3136353a73722e6c  → [r15+0x1f]      ; 拼完 39 字节
0x3dcc0e  call *0x9ba8f8                       ; 取 Location/回溯点
0x3dcc4b  cmp qword [rsi+0x1e0], 0 ; je 0x3de38e   ; ← 主要提前返回分支
0x3dcc68  r12 = [rsi+0x220]                    ; 一个借用的数据源
```

⇒ 函数进入时就**在堆上拼出 `src/graphwar_play_screen_control.rs:561`**
（39 字节 = 35 + `:561`），说明它自己带 `panic::Location` 561；
而 `0x3dcbb0` 真实引用的 `Location` 行有 **18 个**：`275 278 292 307 310 345 350 351 352 353 354 363 364 365 527 530 535 544`（**2026-11-07 更正**，`docs/16` §24.4 第 4 行）。旧文写的 `233/234/235/236/239/244/250/256`（8 个）是 `class_probe` v1 的 `+0x1000` 平移 artifact——那 8 个是 v1 误归给它的 17 行里的一段，逐条对得上。

**它引用的 `.rodata`（33 处 rip 相对；字面量表，全部按 `(ptr,len)` 或立即数还原）**：

| vaddr | 内容 | 出现的指令 |
|---|---|---|
| `0x1bf8bb` | `     f(x) = `（前 5 空格后接 `LeftButtonGrid`…一池） | `0x3dcfea` |
| `0x19f8d0` | `dy/dx = `（同池还有 `TurnInfo`/`bind_mut`/`LockInfo`） | `0x3dcf64` |
| `0x1bf8a0` 池 | `LeftButtonGridRightButtonGridFireContainerLeftReplaysButtonsGridRightReplaysButtonsGrid` | `0x3dcfea` 起 |
| `0x19eb50` | `50.0f, 50.0f` + `TeamVictoryLabel` | `0x3dd711`、`0x3dda65` |
| `0x19c7c0` | 颜色 `[255,255,255,1.0]`（`divps` 用它） | `0x3dd760`、`0x3ddaac` |
| `0x19c780` | 一段常量向量（`0x0C`/`0x0E` 字节模式）+ `…entity_effect_in…name_request_eve` 池尾；**语义未知** | `0x3de3a1` |
| `0x1bf7e7` | `upcast_ref`（len 10） | 9 处 |
| `0x1c0a8c` | `failed to call instance_id() on destroyed object…`（godot-rust） | `0x3de640`、`0x3de65e` |
| `0x1bf9c9`/`0x1bf9b9` | `…play_screen_control.rs:561` | `0x3dcbe9`、`0x3dcbf4` |

**调用的本仓库函数（观测）**：`0x3d9590` ×2、`0x3ec130` ×2、`0x3ec6f0` ×2、`0x3ec410`、
`0x3e6750`，以及**自己 ×2**（`0x3dcbb0` 内部自递归）。
⚠️ 这 5 个被调者**都不在 `fixtures/module_funcs.tsv` 的 545 条归属表里**（未归属），
所以"它调了哪个模块"这一层也是未知的一部分。

**判据汇总 ⇒ 语义**：
- 它**不按状态分派**（无状态比较）；
- 它做的是 `f(x) = ` / `dy/dx = ` 两处**文本**、5 个控件组
  （`Left/RightButtonGrid`、`FireContainer`、`Left/RightReplaysButtonsGrid` ——
  正好是 `0x3d9b50`/`0x3d9e40` 那一对管的 5 个节点）、`TeamVictoryLabel`、
  `PlayReplayButton` + `res://unmute.svg`、以及 `divps` 颜色调制；
- ⇒ **推断（方向唯一）**：`0x3dcbb0` = "对局界面的一次全量刷新"，
  由 `root_node` 在数据变化时调用一次；对应源码应是
  **第 233-256 行那一段（8 个 `unwrap`/`expect`）+ 若干 `get_node`+`set_text` 调用**。

⚠️ **未知**（三条，都要在实现时留接口）：
1. `[T_b+0x1e0]` / `[T_b+0x220]` 是什么（`0x1e0` 为 0 时走 `0x3de38e` 分支）；
2. `cmp dword [rax+0xe0],1`（`0x3dcf26`）、`cmp dword [rax],1`（`0x3dd807`）判的是哪个字段；
3. 自递归那 2 处（`0x3dcbb0` 调自己）是泛型单态化还是真的递归，未定。

### 10.3 与 `function_active_event` / `function_points_event` 的关系：**没有关系**（否证）

`docs/14` §9.3 已证"本模块 10 个函数没有一条直接引用这两个事件名"。本轮把它加强为**全库口径**：

```bash
# 全 .text 里引用这两条字面量的函数（rip 相对，一次扫描）
function_active_event  -> 0x3d5260(play_screen_control 的名字表) · 0x4500c0(server_connection_handler) · 0x477bc0(game_state_manager)
function_points_event  -> 0x3d5260 · 0x4500c0 · 0x477bc0
```

⇒ **观测**：
- `0x3dcbb0` **不在**这三个引用者里；它的 33 处 `.rodata` 引用里**一条事件名都没有**。
- 所以「弹道清除 / 命中结算」既不在 `0x3dcbb0`，也不在 `0x3db3e0`。
- 收口点仍是 `0x3d5260`（名字→分派表）与 `graphwar_game_state_manager 0x477bc0`。

### 10.4 `0x3db3e0`（观测：往胜利横幅填一名玩家）

```text
vaddr 0x3db3e0  Ghidra 0x4db3e0  length 0xFB8（4 024 B），139 个基本块
Ghidra 签名：undefined FUN_004db3e0(long param_1, undefined4 param_2, undefined4 param_3, long param_4)
调用者：0x3da130（胜负横幅）在 0x3dac83 / 0x3daca8 / 0x3dadf3 连调 3 次
```

**入参（推断，按调用点形状）**：
| 寄存器 | 含义 |
|---|---|
| `rdi` | `&容器` —— 里面 `[+0x10]`/`[+0x18]`/`[+0x20]` 三个字段被用到（节点句柄） |
| `esi` / `edx` | 两个 `u32`：槽位/队伍一类的小整数（`0x3db3e0.c:94` 里只做 `CONCAT44` 透传，`param_3` 还进了 `0xa245b0(…,10)`） |
| `rcx` | 一个 `Rc<RefCell<…>>` 形状的**玩家对象**（`[+0x10]` 借用标记、`[+0x18]` 载荷，`0x3db3e0.c:228-238`） |

**它做的事（观测，逐条能指到行）**：
1. `get_node("VictoryPlayerContainer")`（`0x1bf952`，16 字节 `movdqu` 于 `0x3db41d`）；
2. 依次 `get_node`："SkinIcon"（`0x19f3d0`，len 8）、"FaceIcon"（`0x19f180`，len 8）、
   "HatIcon"（`0x1bf968`，len 7，反编译写作 `&DAT_002bf968`）；
   三次都带 "clone"（`0x1bfe73`，len 5）作为中间方法名；
3. 从玩家对象取两处数据（`0x57dec0(obj+0x10)`、`0x57dec0(obj+0x16)`）；
4. `0x507580(x, "to_godot", 8)` → `0x4a01a0(…, "set_texture", 11, …)` →
   `0x505d20(...)`：**动态调用 `set_texture`**（"set_texture" 在 `0x1bf8b0`，len `0xb`）。
5. 失败路径用的是 `"Upcast failed. This is a bug; please report it."`（`0x1bf881`，len `0x2f`）。

⇒ **推断（方向唯一）**：`0x3db3e0` = **把一名玩家的皮肤/脸/帽三张贴图刷到胜利横幅的一个展示位**。
与 `docs/14` §9.5「`0x3da130` 是唯一引用胜负字面量的函数」互补：`0x3da130` 决定**显示谁**，
`0x3db3e0` 负责**画那名玩家的头像**。
⚠️ **未知**：`param_2`/`param_3` 到底是"第几个展示位"还是"(队伍,序号)"；
以及那 3 次调用各自对应哪个展示位（调用点 `0x3dac83`/`0x3daca8` 只隔 `0x25` 字节，
像是同一个双人分支里连填两格，第三格在 `0x3dadf3`）。

### 10.5 那"真正的分派"在哪（把线索钉住）

| 落点 | 证据 | 档位 |
|---|---|---|
| `root_node 0x3a5760` | `0x3a7cd8 cmp byte [self+0x1705], 6` → 命中后对 `[self+0x1668]` 载荷载荷做两次间接调用（`*0x9ba990`、`*0x9ba998`，`0x3a7d03`/`0x3a7d33`），再按 `[self+0x58]==1` 分支 | 观测 |
| `root_node 0x3afcf0` | 同一条 `cmp byte [rdi+0x1705], 6`（`0x3afd1a`），且被 `0x3a5760` 在 `0x3a7cd3` 调 | 观测 |
| `graphwar_game_state_manager 0x477bc0` | 引用 `function_points_event`（`0x1c1978`） | 观测（仅字面量） |
| `graphwar_remote_event_handler 0x3dee00` | **11 个 `src/graphwar_play_screen_control.rs` 的 `Location` 锚点**（:275 :278 :292 :345 :350 :351 :352 :527 :530 :535 :544）落在它体内 ⇒ 本模块一大批 UI 反应被**内联进了 remote_event_handler** | 观测（锚点）／推断（内联归属） |

⇒ **必须修正 `docs/14` §9.1 的口径**：`0x3dcbb0` 之后的下一个函数是
**`0x3dee00`（`graphwar_remote_event_handler.rs`，2449 行反编译）**，
而它体内含 11 个 `play_screen_control` 的行号锚点。
所以「`play_screen_control` 只做开关与文本」这条（§9.6）要改成：
**本模块只做"被调用的一次性刷新"；事件到达时的反应有一部分写在 `remote_event_handler` 里。**

### 10.6 进 `src/play_control.rs` 的部分（本轮可落地的）

| 新条目 | 判据 |
|---|---|
| `refresh_all(&mut T_a, &T_b)` 的骨架 + 9 个 `F(x) = ` / `dy/dx = ` 文本常量 | §10.2 字面量表 |
| `VICTORY_SLOT_COUNT = 3` + `fill_victory_slot(container, idx, player)` | §10.4 三次调用点 |
| `VICTORY_AVATAR_NODES = ["SkinIcon","FaceIcon","HatIcon"]` + `SET_TEXTURE = "set_texture"` | §10.4 的 `get_node`/动态调用 |
| `GAME_STATE_EQ_6` 常量与**它的不确定语义**（只能是常量 + 注释） | §10.5 |

⚠️ **不要**把 §10.2 的刷新函数写成"状态机"：它没有状态比较。
⚠️ **不要**给 `[+0x1705]==6` 编一个语义 —— 本轮只证到"被比较"，语义仍是 **未知**（见 `docs/16` §15.3 的警告）。

---

## 11. `graphwar_game_state_manager.rs`：45 个可归属函数 ↔ 地址 ↔ 我方实现

> **本节解决什么问题**：对局状态机（`docs/41`）、回合计数与结束（`docs/45`）、
> 同时回合制（`docs/86`/`docs/141`）、胜负判定（`docs/150`）这四件事早就逐位对拍过，
> 但实现一直按"运行实体 / 迁移表"摊在 `src/game.rs` 与 `src/turnstate.rs` 两个文件里，
> **不是原版的函数边界**。本节把 45 个原版函数逐个落到地址上，说明分别对应我方哪段代码，
> 并把能补的补掉（`docs/02` §17.4 把这两个文件登记为"混装（待拆）"）。
>
> 证据口径：地址一律 **ELF vaddr**；Ghidra/REA 地址 = 本地址 + `0x100000`。
> 函数边界取自 `analysis/funcstarts_ghidra_va.pkl`；被调者多重集、`.rodata` 串、
> `panic::Location` 都从 `analysis/insns.pkl` / `xrefs.pkl` 现算。
> **arm64 2.0.60 未 strip**（`.symtab` 里有真名与长度）用作交叉校验：x86 尺寸与 arm64
> 尺寸的比值在 0.61–1.19 之间，凡偏出这个区间的归属一律**否掉**（见 §11.1 第 2 条）。

### 11.1 先纠正三处归属（本轮新发现）

1. **`report_fired_function` 不在 `0x3e2830`。** 旧表（`fixtures/name_to_addr.tsv` 的
   `唯一锚` 行）把它钉在 `0x3e2830`，但那个地址是
   `GraphwarRemoteEventHandler::handle_server_remote_event`（**12512 字节**、51 个直接调用、
   23 个各模块 manager 的 setter 全在里面），而 arm64 上 `report_fired_function` 只有
   **236 字节**（`0x4ab5dc`，体内只有一次 `GraphwarEventQueue::add_event`）。
   **53 倍**的尺寸差不可能是同一个函数 ⇒ 旧归属是"一行内联进来的 `Location`"造成的
   单锚误并。结论：`report_fired_function` 在 x86 上**没有独立函数体**（被内联进
   `0x3e2830` 与 `request_fire_function` 两条路径）。
2. **尺寸比是判归属的硬门槛。** 34 条有 x86 地址的行里，33 条能与 arm64 对上，比值
   落在 **0.61–1.19**（`queue_end_turn_trigger` 的 arm64 只剩闭包，1376/1420 = 0.97，
   同区间）；唯一例外就是上面那条 **53 倍**。以后新归属先进这道闸。
3. **三个 serde 函数与 `set_turn_count` 本轮反而解出来了**（旧表全标"未知/歧义"）：

   | 名字 | x86 | 判定依据 |
   |---|---|---|
   | `serialize` | **`0x46f020`**（160 B） | 取 `[rdi]` 判别字节 → 查 `.rodata 0x1c17c0` 的 **8 项**跳表 → 每组 `(ptr,len)` → 调 `serialize_str`（`0x3bdf00`） |
   | `deserialize` | **`0x46f0c0`**（592 B） | 同一组 8 个变体名的第二张跳表在 `.rodata 0x1c17e0`；体内 4 个调用全落在 JSON 解析器 `0x37bd70/0x37f440/0x37f7f0/0x37ca10` 上 |
   | `visit_str` | **`0x47a930`**（432 B） | `lea rcx,[rdx-5]; cmp rcx,0xf`（长度 5..=20）+ `.rodata 0x1c1814` 的 **16 项**按长度跳表，然后逐项 `xor`/`pcmpeqb` 比字节，命中后写判别字节 |
   | `set_turn_count` | **未知（x86 内联）** | arm64 有独立体 `0x4a8460`（124 B，与 `set_turn_limit` 同形同长）；x86 上它的 panic 锚只剩三处调用点（`0x3e2830`/`0x46f690`/`0x4759f0`），也就是旧表给的三个"歧义候选"——三个都是**别人** |

   地址顺序也是独立旁证：arm64 的 `serialize`(0x4a52f4) → `deserialize`(0x4a53e8) →
   `add_target`(0x4a5704) 与 x86 的 `0x46f020` → `0x46f0c0` → `0x46f310` **同序**，
   三段尺寸比分别为 0.66 / 0.74 / 0.83。

### 11.2 45 行对照表（核心交付物）

`锚` 列：`唯一` = 双向唯一 `panic::Location`；`消去` = 无一对一锚，按"一名一址"消去后唯一；
`内联` = x86 上没有独立体；`结构` = 无 `Location`，靠结构（跳表 / 变体名 / 顺序 / 尺寸比）判定。
`我方` 列的位置都是本仓文件 + 条目名。

| # | 原版函数（`GraphwarGameStateManager::` 省） | x86 | x86 字节 | arm64 字节 | 锚 | 它干什么（证据） | 我方 |
|---|---|---|---|---|---|---|---|
| 1 | `serialize` | `0x46f020` | 160 | 244 | 结构 | `GraphwarGameState` 8 变体名之一，`serialize_str`（§11.1-3） | 未实现（`turnstate::GameState::as_str` 是同一组名字） |
| 2 | `deserialize` | `0x46f0c0` | 592 | 796 | 结构 | 同上，走 JSON 反序列化器 | 未实现（`GameState::from_str` 等价） |
| 3 | `visit_str` | `0x47a930` | 432 | 652 | 结构 | 按长度跳表 + 逐字节比出变体 | `turnstate::GameState::from_str` |
| 4 | `add_target` | `0x46f310` | 896 | 1076 | 唯一 | `TeamManager::add_to_team` + `set_skin_graphics` + 4 次模块存取器（行号 593–603） | **`game::Match::add_target`** ✅同名 |
| 5 | `start_game` | `0x46f690` | 4528 | 3996 | 唯一 | `get_team`×2 / `set_function` / `TurnManager::next_turn`（191–255、572–625） | 部分：`Match::new` + `begin_turn` |
| 6 | `handle_tick` | `0x470840` | 816 | 784 | 唯一 | tick 事件转发 + **回合截止⇒自动开火** + 时间上限⇒回 `Setup`（§11.3） | **`Match::handle_tick`** / `tick` / `turnstate::should_auto_fire` ✅新增 |
| 7 | `set_all_alive` | `0x470b70` | 832 | 892 | 唯一 | `LifeManager::set_alive` ×2（两类实体各一趟，607–616） | **`Match::set_all_alive`** ✅新增 |
| 8 | `fire_functions` | `0x470eb0` | 5456 | 4864 | 唯一 | 取当前回合士兵 → 位置/队伍/函数原文 → 算弹道 → `set_function_points` → **调 `execute_results`** | **`Match::fire_functions`** ✅新增边界 |
| 9 | `set_turn_count` | 未知（内联） | — | 124 | 内联 | `str w1,[x0,#0x118]` + `add_event`（tag `0x3a` = 58） | **`Match::set_turn_count`** ✅新增 |
| 10 | `set_turn_limit` | `0x472400` | 112 | 124 | 消去 | `mov [rdi+0xc8],esi` / `[rdi+0xcc],edx` + `add_event`（tag `0x39` = 57） | **`Match::set_turn_limit`** ✅同名 |
| 11 | `execute_results` | `0x472470` | 4272 | 3712 | 唯一 | `set_function_active_tick` / `set_queued_death` / `queue_destroy_obstacle` / `get_position` | **`Match::execute_results`** ✅新增边界 |
| 12 | `get_alive_teams` | `0x473520` | 640 | 680 | 唯一 | 枚举**生命表** → `get_alive` → `get_team`，返回 `(Team1,Team2)` 两位（867–876） | **`Match::get_alive_teams`** ✅新增 |
| 13 | `remove_all_players` | `0x4737a0` | 544 | 532 | 唯一 | 一次模块存取器 + 释放；485–486、554 | 未实现（清空玩家入口） |
| 14 | `request_new_player` | `0x4739c0` | 832 | 1356 | 消去 | `add_to_team` + `add_team_color`（282–292） | 部分：`Match::register_player` |
| 15 | `handle_chat_message` | `0x473d00` | 336 | 380 | 唯一 | 无直接调用，只做转发（锚 1187:18） | 未实现（无事件总线） |
| 16 | `request_change_team` | `0x473e50` | 848 | 936 | 唯一 | `add_to_team`×2 + `get_team` + `add_team_color`（306–323） | 未实现（`set_team` 只有实体级） |
| 17 | `handle_graphics_info` | `0x4741a0` | 752 | 784 | 唯一 | 写外观三件套（1147–1154） | 未实现（图形侧属 `graphwar_graphics_manager`） |
| 18 | `setup_and_start_game` | `0x474490` | 1408 | 1184 | 唯一 | `get_team`×2 + `LevelGenerator::generate_level_custom` → `start_game`（458–474） | 未实现（关卡生成在 `graphwar_level_generator`） |
| 19 | `add_soldier_to_player` | `0x474a10` | 2736 | 2856 | 唯一 | `get_team`/`add_to_team` + `set_hat/face/skin_graphics`（503–538） | 部分：`Match::register_player` |
| 20 | `report_fired_function` | 未知（内联） | — | 236 | 内联 | 一次 `EventQueue::add_event`（arm64 `0x4ab5dc`） | 未实现（我方 `FunctionActive` 在 `execute_results` 里发） |
| 21 | `request_fire_function` | `0x4754c0` | 208 | 224 | 唯一 | 调 `fire_functions` + `0x3caa20`（HashMap 泛型实例，不是方法，见 §11.8-6）；锚 655:111 | 部分：`Match::fire`（我们没有"请求→校验"两段） |
| 22 | `request_remove_player` | `0x475590` | 800 | 812 | 唯一 | 334–340、554 | 未实现 |
| 23 | `handle_function_points` | `0x4758b0` | 320 | 300 | 唯一 | 无直接调用，只做转发（1129/1135） | 部分：报文由 `execute_results` 发 |
| 24 | `queue_end_turn_trigger` | `0x4759f0` | 1376 | 1420（closure） | 唯一 | `get_alive_teams`×2 + `get_alive` + `set_alive` + `next_turn` + `set_state`（839–906） | **`Match::end_turn`** + `Match::ending` |
| 25 | `request_update_function` | `0x475f50` | 976 | 916 | 唯一 | `get_current_turn_soldiers` + `FunctionManager::set_function`（635–642） | 未实现（收集阶段在 `graphwar_game_server`） |
| 26 | `set_player_num_soldiers` | `0x476320` | 400 | 460 | 唯一 | 转调 `add_soldier_to_player` / `remove_soldier_from_player`（370–371） | 未实现 |
| 27 | `request_axis_mode_change` | `0x4764b0` | 320 | 380 | 唯一 | 写轴模式 + 一条 info（393–395） | 未实现（`RoomState.axis_mode` 是静态串） |
| 28 | `request_time_mode_change` | `0x4765f0` | 320 | 384 | 唯一 | 同上（426–428） | 未实现（`time_mode` 是 `begin_turn` 入参） |
| 29 | `request_turn_mode_change` | `0x476730` | 256 | 332 | 唯一 | `TurnManager::get_turn_mode` + `set_turn_mode`（415–417） | 未实现（`docs/117` §6：写死 `SequentialTurns`） |
| 30 | `remove_soldier_from_player` | `0x476830` | 496 | 528 | 唯一 | 544–559 | 未实现 |
| 31 | `handle_entity_removed_event` | `0x476a20` | 1824 | 1648 | 唯一 | `get_alive` + `get_position` + **`fire_functions`**（1064–1086） | 部分：死亡落地在 `execute_results` 尾部 |
| 32 | `handle_new_connection_event` | `0x477140` | 720 | 740 | 唯一 | 1031–1048 | 未实现 |
| 33 | `request_function_mode_change` | `0x477410` | 304 | 368 | 唯一 | 写函数模式 + 一条 info（404–406） | 未实现 |
| 34 | `handle_connection_removed_event` | `0x477540` | 1440 | 1300 | 唯一 | 调 **`execute_results`** 一次（1096–1114）；"掉线者按已提交处理"是**推断** | 未实现 |
| 35–41 | `subscribe_to_events_event_handler` ×7 | 内联进 `0x477bc0` | — | 7×48 | 内联 | 7 个 `GraphwarEventHandler::subscribe_to_event`（事件名见 §11.7） | 未实现（无事件总线） |
| 42 | `new` | `0x477bc0` | 992 | 1004 | 唯一 | 建表 + 7 次订阅（116–118） | **`Match::new`** |
| 43 | `set_lock` | `0x477fa0` | 112 | 124 | 消去 | 一个字节 + 一条 info（454） | 未实现 |
| 44 | `set_name` | `0x478010` | 176 | 196 | 消去 | 写名字 + 一条 info（203） | 未实现（`RoomState.room_name` 静态） |
| 45 | `set_state` | `0x4780c0` | 560 | 652 | 唯一 | `mov [rdi+0x120], sil` + 一条 `GameStateInfo`（tag `0x1e` = 30，与 `src/events.rs` 一致）+ **按状态分支**（§11.4） | `turnstate::GameState` / `next_state` / `winner_byte` |

合计：**45 行 = 34 个有 x86 地址 + 4 个内联（`set_turn_count`、`report_fired_function`、
7 个订阅闭包算 1 组）+ 3 个 serde（本轮新解出）**。
按"我方有没有对应实现"分：**同名或逐位同构 10 条**（`add_target` / `handle_tick` /
`set_all_alive` / `fire_functions` / `execute_results` / `get_alive_teams` / `set_turn_count` /
`set_turn_limit` / `queue_end_turn_trigger`→`end_turn`+`ending` / `new`）、
**部分或等价 7 条**（`visit_str`→`from_str`、`start_game`、`request_new_player`、
`add_soldier_to_player`、`request_fire_function`、`handle_function_points`、
`handle_entity_removed_event`）、**未实现 28 条**。

### 11.3 `handle_tick`：回合超时的**检测点**在这里

`docs/117` §6 记着"回合超时的**检测**没有证据"。现在有了：检测就在 `0x470840`。
这个函数干三件事，第三件是回 `Setup`：

```text
① 0x470880–0x470989  tick 事件转发：取订阅者表，逐个 call [vtable+0x20]，再释放
② 0x470989–0x4709f1  cmp BYTE [self+0x120], 2      ; state == WaitingForFunctions（判别值 2）
                     call TurnManager::get_end_of_turn_tick (0x462ec0)
                     cmp  eax, 1                    ; 取到截止 tick
                     cmp  now_tick, end             ; now_tick >= end_of_turn_tick
                     cmp  BYTE [self+0x11c], 0      ; "有人在等"标志
                     → call fire_functions (0x470eb0)
③ 0x470a04–0x470a61  cmp DWORD [self+0xc0], 1       ; 另一对 (标志, 截止 tick)
                     cmp now_tick, [self+0xc4]      ; now_tick > 截止
                     cmp BYTE [self+0x11c], 0
                     → [self+0xc0] = 0; state = 1 (Setup); 推一条事件
```

② 判的是**时钟**，不是"谁按了按钮"：`end_of_turn_tick`（`docs/42` 的回合截止时刻）
一到，就把当前回合士兵已提交的函数打出去 —— 也就是**超时即按已提交内容结算**。
它有三个合取项（状态 / 截止 tick / `[+0x11c]`），一项都不能省。

arm64 侧的偏移与 x86 完全一致，可以互相印证（`0x4a6bd0 ldrb w8,[x19,#0x120]`、
`0x4a6c2c`/`0x4a6c68 ldrb w8,[x19,#0x11c]`、`0x4a6c7c strb w8,[x19,#0x120]`）。

落成代码：`turnstate::should_auto_fire(state, now_tick, end_of_turn_tick, waiting)`（纯函数，
真值表在 `tests/graphwar_game_state_manager.rs`）。**没有接线**：`Match` 上没有 `[+0x11c]`
那一位，开火仍由调用方在收集阶段决定（`docs/117` §6 的其余两条尾巴不受影响）。

③ 那一对 `[+0xc0]`/`[+0xc4]` 与 `set_turn_limit` 写的 `[+0xc8]`/`[+0xcc]` 是**两对**不同字段
（前者只在这条分支里读，写点还没找到）⇒ ③ 的**语义仍是推断**，只记结构，不实现。

### 11.4 胜负字节：`set_state(EndingGame)` 内部的四行表

`docs/150` 已经把"某队没有存活实体"这条判据钉死了；本节补的是**胜方那个字节**
（`[self+0x11f]`）到底怎么算出来 —— 它在 `set_state` 里，不在 `queue_end_turn_trigger` 里：

```text
0x478136  call get_alive_teams        ; 返回 (al = Team1 有活体, dl = Team2 有活体)
0x47813b  test al,1
0x47813d  je   0x47822f               ; Team1 死 → al = 2 - (dl&1)
0x478143  xor  eax,eax
0x478145  test dl,1
0x478148  je   0x478236               ; Team2 死 → 写 0（Team1 赢）
0x47814e  cmp  DWORD [rbx+0xc8],1     ; 设过回合上限？
0x478155  adc  al,1                   ; CF = ([+0xc8] < 1) ⇒ 设过上限写 1，否则写 2
0x478236  mov  BYTE [rbx+0x11f],al
```

| `Team1` 有活体 | `Team2` 有活体 | 设过上限 | 字节 | 含义 |
|---|---|---|---|---|
| 否 | 否 | — | `2` | 没人赢 |
| 否 | 是 | — | `1` | `Team2` |
| 是 | 否 | — | `0` | `Team1` |
| 是 | 是 | 否 | `2` | 没人赢 |
| 是 | 是 | 是 | `1` | `Team2` |

⚠️ 注意这一列是**"设过上限"**（`[+0xc8]` 是 `set_turn_limit` 的第一个入参），
**不是"上限已用尽"**。两者只在"两边都还有人却进了 `EndingGame`"这一种收场上分叉，
而那种收场只可能由上限触发 ⇒ 我方 `Match::ending` 用 `turn_count >= limit` 判，
在**可达输入上逐例一致**（`ending_agrees_with_the_winner_table_on_reachable_inputs`）。

落成代码：`turnstate::winner_byte(t1_alive, t2_alive, turn_limit_set) -> u8`。
我方 `Ending.winner` 存的是**实体队伍号**（`set_team` 记的 u8），原版这个字节是
**队伍序号**（0/1/2）—— 换算关系见两者的文档，别混。

### 11.5 三个判定点：`get_alive_teams` / `set_all_alive` / `add_target`

* **`get_alive_teams`（`0x473520`）** 数的是**生命表里的活实体**：
  `get_all_entities`(0x453c60) → 逐个 `get_alive`(0x454950) → `get_team`(0x3c2120)。
  士兵与靶子同表（靶子有 `LifeInfo`，玩家实体没有），所以打靶关与对战关**同一条规则**
  （与 `docs/150` §Q2 一致）。我方 `team_has_survivor` 是同一个判据的两种枚举方式，
  新加的 `Match::get_alive_teams` 把它收成原版那两位。
* **`set_all_alive`（`0x470b70`）** 体内**两次** `LifeManager::set_alive`、零报文：
  两类实体各刷一趟存活位。我方 `Match::set_all_alive` 两趟（靶子表 + 生命表）；
  **差异**：我们被打死的士兵会被 `execute_results` 从 `self.soldiers` 摘掉（站位丢了），
  复活不了 —— 真值那边 `set_alive(true)` 也只是翻存活位、不重建实体，所以行为面一致。
* **`add_target`（`0x46f310`）** 的直接调用是 `TeamManager::add_to_team`（写队伍）+
  `GraphicsManager::set_skin_graphics`（写外观）+ 4 次模块存取器；
  **队伍是入参**（`docs/150`：127/127 个靶子都是 `TeamInfo{Team2}`，但那是调用方给的）。
  我方 `add_target` 只有"靶子表 + 队伍"两件事（外观属别的模块）。

### 11.6 `set_turn_count` / `set_turn_limit`：两个同形 setter

arm64 上这两个函数**紧挨着、同样 124 字节、体内同样只有一次 `add_event`**（§11.1-3）。
x86 上只剩 `set_turn_limit`（`0x472400`，112 字节）：

```text
0x472405  mov DWORD [rdi+0xc8], esi     ; 第一个 u32 入参 = "设过上限"标志
0x47240b  mov DWORD [rdi+0xcc], edx     ; 第二个 u32 入参 = 上限本身
0x472433  movabs rax, 0x8000000000000039  ; 事件 tag 0x39 = 57
0x472447  call [GOT 0x9bac80]            ; EventQueue::add_event
```

⇒ 它是 **(标志, 上限)** 两个 u32 的 setter，报文里只出现 `turn_limit` 一个字段
（语料 21 条全是 `{"TurnLimitInfo":{"turn_limit":N}}`，`docs/123` §5.1）。

**两侧一起看就闭环了**（arm64 的字段偏移与 x86 相同）：

```text
set_turn_count 0x4a8470  str  w1, [x0, #0x118]      ; turn_count
               0x4a8480  mov  x9, #0x3a             ; tag 58
               0x4a8488  str  w1, [sp, #0x10]       ; payload = 计数
               0x4a84a0  bl   EventQueue::add_event

set_turn_limit 0x4a84f0  stp  w1, w2, [x0, #0xc8]   ; (标志, 上限) 两个 u32
               0x4a8508  mov  x8, #0x39             ; tag 57
               0x4a8500  stp  w1, w2, [sp, #0x10]   ; payload = 两个 u32
```

两个 tag 与**本仓自己的** `src/events.rs` 完全一致：`TurnLimitInfo = 57`、
`TurnCountInfo = 58`（另 `GameStateInfo = 30` = `set_state` 的 `0x1e`）。这条交叉校验
不依赖任何反编译文字：一边是二进制里的立即数，一边是我们按抓包排出来的 tag 表。
⇒ `set_turn_count` 的 payload 就是 `turn_count` 一个字段。

**上限/结束判定不在这两个 setter 里**（在 `queue_end_turn_trigger` 与 `ending`）。
我方两条都按这个形状落地。

### 11.7 我方按原版边界归位后的分工

重组**只挪位置、不改行为**（既有断言一条没动）。两个文件现在这样分工：

| 文件 | 负责的原版函数 | 说明 |
|---|---|---|
| `src/game.rs` | `new`(42) / `add_target`(4) / `set_all_alive`(7) / `get_alive_teams`(12) / `set_turn_limit`(10) / `set_turn_count`(9) / `handle_tick`(6) / `fire_functions`(8) / `execute_results`(11) / `queue_end_turn_trigger`(24) / `start_game`(5，部分) | 运行实体 `Match`。每段代码前面有一条 `// 原版边界：…` 横幅，写明原版函数名 + 地址 + 我方条目名 |
| `src/turnstate.rs` | `GraphwarGameState`（1/2/3 的枚举侧）+ `set_state`(45) 的状态字节与胜方字节 + `handle_tick`(6) 的超时判据 | 状态枚举（8 变体）、迁移表（我方模型，非原版函数）、`should_auto_fire`、`winner_byte` |

新增的原版名入口（都不改既有行为，全部有成对的测试）：

| 新增 | 原版 | 一句话 |
|---|---|---|
| `Match::set_all_alive` | `0x470b70` | 两趟翻存活位 |
| `Match::get_alive_teams` | `0x473520` | `(Team1 有活体, Team2 有活体)` |
| `Match::set_turn_count` | arm64 `0x4a8460`（x86 内联） | 写计数 + `TurnCountInfo` |
| `Match::handle_tick` | `0x470840` | 原版名的入口（已落地的到期动作） |
| `Match::fire_functions` | `0x470eb0` | 前一半：纯几何（只算弹道，不改状态） |
| `Match::execute_results` | `0x472470` | 后一半：合并死亡 + 广播 + 落地 |
| `turnstate::GameState::{Connecting, Disconnected}` + `VARIANTS` + `index` | 变体表 `0x9586d0` | 枚举补齐 8 个变体，判别值 `0..=7` |
| `turnstate::should_auto_fire` | `0x470989`–`0x4709ec` | 超时判据（纯函数，未接线） |
| `turnstate::winner_byte` | `0x478133`–`0x478236` | 胜方字节四行表 |

`fire_batch` 现在是 `fire_functions` + `execute_results` 的**顺序复合**（`N=1` 时与
`fire` 逐字节相同，`one_shot_fire_is_a_one_element_batch` 钉住这条）。

`new` 里那 7 个订阅的事件名（`.rodata` `0x1c18ad` 起，逐条 `lea` 立即数）：
`new_connection_event`、`entity_removed_event`、`tick_event`、`connection_removed_event`、
`function_points_event`、`skin_graphics_info_event`、`chat_message_event`。
这 7 条正好对上 arm64 里 7 个 `subscribe_to_events_event_handler::{{closure}}`（每个 48 字节）
⇒ 订阅函数本身在 x86 上被内联进 `new`，没有独立体。

### 11.8 仍然未知 / 未实现

1. **`set_turn_count` 的 x86 地址**：只能证明它被内联（§11.1-3），**没有**地址；
   旧表给的三个候选全是别人的函数体。
2. **`report_fired_function` 的 x86 地址**：同上。它在 arm64 上只做"把'已开火'塞进事件队列"，
   在 x86 上被内联进 `0x3e2830`（远端事件派发器）。
3. **`handle_tick` 的第 ③ 段**（`[+0xc0]`/`[+0xc4]` ⇒ 回 `Setup`）：只读到**结构**，
   字段语义（是不是"整局时间上限"）**没有写点证据** ⇒ 推断，不实现。
4. **`[+0x11c]` 那一位**（"有人在等"）：`handle_tick` 的三段都要它，但写点未定位
   ⇒ `should_auto_fire` 只把它当入参。
5. **29 条"未实现"**（§11.2 表里）：绝大多数是**事件总线 / 房间设置 / 图形 / 关卡生成**
   的接线，落在别的原版模块（`graphwar_event_handler`、`graphwar_graphics_manager`、
   `graphwar_level_generator`、`graphwar_player_manager` …），不属于本模块的"状态机内核"。
6. **`0x3caa20`**：被 `module_funcs.tsv` 归到本模块，但它是 **HashMap 探测的泛型实例**
   （SIMD 比较 + 两个 `panic::Location` 落在 658/660 行，即 `request_fire_function` 的行段内）
   ⇒ 不是原版方法，不该进本模块的函数名单。本表不含它。

### 11.9 复现

```text
# 45 条真名（fixtures/name_to_addr.tsv，join_kind 列就是可信度）
awk -F'\t' '$1=="graphwar_game_state_manager"' fixtures/name_to_addr.tsv

# 尺寸交叉校验（arm64 有真名与长度，x86 用 Ghidra 边界）
nm -C -S --defined-only .rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so | grep game_state_manager

# 单条函数的直接调用表 / 串 / Location 行号（本仓库之外的分析索引）
../.venv/bin/python tools/callers.py 0x470840
```


## 12. 挖洞点 = 爆炸点（`DestroyObstacle` 的位置语义）

> 这一节回答 `docs/06-verification.md` §24.7 那根"挖洞点轴"的机制问题
> （`dispatch/14-hole-axis`）：延迟段 291 个可比批里我方只有 66 个命中，225 个不命中，
> 其中 **200 例"我方这个点在该回放的任何 `DestroyObstacle` 里都不存在"**。
> **结论：一根坐标轴的问题，不是机制分叉** —— 我方拿"采样停点"当挖洞点，
> 原版拿"爆炸点"当挖洞点；两者 x 逐位相同，只差 y。

### 12.1 观测（逐位，全语料，不依赖我方任何模型）

```text
回放里 DestroyObstacle 的 (pos_x,pos_y) 与**同一结算批**的
EffectInfo(ExplosionEffect) 的 (pos_x,pos_y) **逐位相同**：503 / 503
```

401 份可解回放、全部 `DestroyObstacle`（**503 条**）一条不差。**等级：观测（逐位）**。

> 顺带一条结构差异：503 条里有 **162 处**是"同一份回放、同一坐标发了两条"
> （179 处 1 条 + 162 处 2 条 = 341 个不同坐标），即**一次挖洞可能发 2 条完全相同的
> `DestroyObstacle`**；我方每条只发 1 条。命中率口径只看"点在不在集合里"，
> 不受这条影响，但它是**另一个**未定性（为什么会重复，未知）。

⇒ 原版挖洞用的点是**客户端画爆炸图案的那个点**，不是弹道算出来的采样停点。
两者的关系（一个批次 1782640836 批#1 的实例）：

| 量 | 值 |
|---|---|
| `FunctionPoints.end_x`（回放的弹道停点，只发布 x） | `21.50120415902081` |
| 我方采样停点（`settle::resolve` 的 `stop`） | `(21.50120415902081, -7.964500098012195e-05)` |
| 回放 `ExplosionEffect` = `DestroyObstacle` | `(21.50120415902081, 0.0)` |
| 该发修正文本 | `… + 9.311` |

x 相同（都是 `end_x`），y 差 8e-5。原因是修正文本里的常数**只印 3 位小数**
（`corrected_function`），客户端/服务端拿**印出来的文本**在 `x = end_x` 处重算了一次 y；
我方 `stop` 是**原始文本**采样出来的 y。这个差正是 §24.2 `Y2` 类
（3 位小数常数回代）的同一个量，只是这次落在挖洞点上。

### 12.2 200 例的四类分桶（互斥，加总 == 200）

口径与 `docs/06-verification.md` §24.7 完全一致（`bit_exact_attrib.py` 的
`destroy_checked / destroy_ok`，逐文件重置、点按 `repr` 逐位比）。
分母：225 个不命中批 = **25 例末位差（0 < d ≤ 1e-12，不算"点位不存在"）** + **200 例**。

| 类 | 定义（可复算） | 例数 |
|---|---|---|
| a) 我方多挖了 | 我方挖洞点在**全文件**任何 `DestroyObstacle` 的 0.5 之外 | **0** |
| b) 位置对但时间不对 | 我方挖洞点逐位等于文件里某条 `DestroyObstacle`，但不在本批延迟窗口 | **0** |
| c) 把不该挖的障碍当可挖 | 挖洞点不在回放关卡的任何障碍里（画布边界除外）**且**没有对应销毁点 | **0** |
| d) 其它 | 剩下的 | **200** |
|  | 合计 | **200** ✅ |

d) 的子因（同一套工具输出）：

| 子因 | 例数 | 含义 |
|---|---|---|
| d1 | **192** | 我方**爆炸点**逐位 == 本批窗口 `DestroyObstacle` —— 纯"参照点用错"（拿停点当挖洞点） |
| d2 | 4 | 爆炸点与销毁点差 ≤ 0.02（掩膜边界差一两个采样点，0.01/步） |
| d3 | 4 | 差 > 0.02（真几何分歧：0.031 / 0.047 / 0.056 / 0.49） |

⇒ **主因 = d1（192 / 200 = 96%）**：回放**挖了**这个洞、位置也**对**、时间也**对**，
只是我方发出去的点用了采样 y。a)/b)/c) 三类**一例都没有** —— 这三类的旁证在
`tools/hole_axis_attrib.py` 的输出里（200 例里"挖洞点→全文件销毁点"的最大距离
只有 0.4931，没有任何一例 > 0.5）。

### 12.3 每类 3 个样本（坐标 + tick）

`a)/b)/c)` 均为 0 例 → 无样本；下表给 d 的三个子因各 3 个（`tools/hole_axis_attrib.py` 直接复现）：

| 子因 | 回放/批 | 回放弹道停点 `end_x` | 我方停点 | 我方挖洞点（= 爆炸点） | 回放 `DestroyObstacle`@tick |
|---|---|---|---|---|---|
| d1 | `1782640836` #1 | `21.50120415902081` | `(…, -7.9645e-05)` | `(21.50120415902081, 0.0)` | `(21.50120415902081, 0.0)` @3834 |
| d1 | `1782641987` #0 | `4.354012878988948` | `(…, 13.9208899130673)` | `(4.354012878988948, 13.920934976301991)` | 同左 @35058 |
| d1 | `1782641987` #1 | `17.499171559312664` | `(…, -4.875e-04)` | `(17.499171559312664, 0.0)` | 同左 @38384 |
| d2 | `1782646337` #0 | `-6.322417849086799` | `(-6.324605349086797, -0.9087028206628336)` | `(-6.324605349086797, -0.9091835182922221)` | `(-6.322417849086799, -0.9011605629508884)` @17591 |
| d2 | `1782760054` #0 | —（DiffEq 局不发布） | `(11.139536214933742, 1.7555053360198483)` | 同停点 | `(11.139536214933742, 1.7555053360225297)` @188632 |
| d2 | `1783010723` #0 | — | `(-0.6203154930872411, -7.957330328004617)` | `(-0.6203154930872411, -7.957208451610688)` | `(-0.6204717430872412, -7.958302201610689)` @63810 |
| d3 | `1782674270` #1 | — | `(13.197677340163356, -5.00034948477338)` | `(13.197677340163356, -5.0)` | `(13.244552340163349, -5.0)` @32135 |
| d3 | `1782677943` #1 | — | `(17.020271404337663, -8.99998211076159)` | `(17.020271404337663, -9.000197346015902)` | `(17.051521404337663, -9.0214236391402)` @25405 |
| d3 | `1782761135` #0 | — | `(4.202802029801951, 7.843e-05)` | `(4.202802029801951, -2.8e-14)` | `(4.259052029801953, -2.7e-13)` @6037 |

（d2/d3 的 x 差 0.031~0.056 = **3~6 个采样点**，全在障碍边缘：我方掩膜在
`r+1` 像素的抗锯齿环上判"占用"，比原版的圆边界早/晚几个采样点。）

### 12.4 二进制：`queue_destroy_obstacle` 的入队路径

`execute_results`（`0x472470`–`0x47351a`）**不是在遍历弹道点，而是在排空一个事件队列**：

```text
0x4727c0  mov rax, rbx / add rbx, 0x20      ; 每条记录 0x20 字节
0x4727d0  mov rbp, [rax]                    ; +0x00 判别值
0x4727e1  mov r15d, [rax+0x8]               ; +0x08 两个 u32（变体 0 当"点数组指针"用）
0x4727e5  mov r12d, [rax+0xc]
0x472804  movsxd rcx, [rip-0x2b100b + rdx*4] ; 跳表 @0x1c1800
0x472812  jmp rcx
```

跳表实测（`tools/` 里没有专用脚本，用 capstone 读 0x1c1800 的 32 位偏移）：

| 表项 | 判别值 | 处理块 | 干什么 |
|---|---|---|---|
| idx 0 | 小整数 | `0x472814` | 死亡块：`points[idx-1]` → `0x456b80`（`set_queued_death`）+ 死效 |
| idx 1 | `0x8000000000000000` | **`0x472c61`** | **挖洞块**：`(u32,u32) + (x,y)` → `queue_destroy_obstacle` |
| idx 2 | `0x8000000000000001` | `0x472a53` | （本轮未展开） |
| idx 3/4 | `0x8000000000000002/3` | `0x4727c0` | 跳过 |

挖洞块读的记录与实参（`0x472c61`–`0x472d19`）：

```text
记录（0x20 字节）: +0x00 判别值 | +0x08 f64 pos_x | +0x10 f64 pos_y | +0x18 u64（语义未知）

0x472c65  mov [rsp+0x130], r15d      ; r15d = 记录 +0x08（pos_x 的低半）
0x472c6d  mov [rsp+0x134], r12d      ; r12d = 记录 +0x0c（pos_x 的高半）
0x472c75  movdqu xmm0, [rax]         ; rax = 记录 +0x10 → 16 字节 (pos_y, 那个 u64)
0x472c81  movdqu [rsp+0x138], xmm0   ; 于是 rsp+0x130 = (pos_x, pos_y, u64) 24 字节
0x472ce6  mov eax, 0x88888889 / imul r13 / shr r13, 0x24 / lea edx,[r12+r13] / add edx, 0x3c
                                     ; edx(tick) = [模块+0x58] + X/30 + 60，X = 那个 u64 的高 dword
0x472d05  add rdi, 0x18              ; 第 1 实参 = ObstacleManager 的字段
0x472d09  lea rsi, [rsp+0x130]       ; 第 2 实参 = &(pos_x, pos_y, u64)
0x472d11  movq xmm0, [rip-0x2d3649]  ; 第 4 实参 = 常量 @0x19F6D0 = **0.5**
0x472d19  call 0x3f20f0              ; ObstacleManager::queue_destroy_obstacle
```

> ⚠️ 这里顺带改正 `docs/06-verification.md` §24.6(b) 的一处读法：那个魔术数
> `0x88888889 / shr 0x24` 是**除以 30**，不是除以 60（0..20000 逐个验过，只有 30 全对）。
> 该式同时用在死亡块（`0x472aad`）和挖洞块上，即两处 tick 都是
> `[模块+0x58] + X/30 + 60`；`X` 与 `[模块+0x58]` 的语义**本轮未定**（见 §12.6）。
> `[模块+0x58]` 那个模块由 `0x46d040` 取（`execute_results` 里的两处都用它），
> 返回的是 `Option<&T>`（`test rax,rax; je …` 判空）。

被调函数 `ObstacleManager::queue_destroy_obstacle` = **`0x3f20f0`**（vaddr；`.text` 段
`file = vaddr - 0x1000`）：

```text
0x3f20f0  push rbp/r15/r14/r13/r12/rbx; sub rsp,0x68     ; (self, &(pos_x,pos_y,…), tick:u32, radius:f64)
0x3f211c  call 0x47dec0                                  ; SwissTable 哈希，键 = edx 那个 u32 = tick
0x3f217b  cmp dword [rbp-0x20], ebx                      ; 键比较
0x3f224b  movups xmm1, [r15]                             ; 值里的 (pos_x,pos_y) 一对 f64
0x3f2279  movsd xmm0, [rsp+8] / movsd [rax+rcx*8+0x10], xmm0   ; 值 = (pos_x,pos_y,radius)
0x3f22a3  movups [rsp+0x28], xmm1                        ; 事件载荷: pos_x,pos_y (16 B)
0x3f22a8  movsd [rsp+0x38], xmm0                         ;           radius    (8 B)
0x3f22ae  mov [rsp+0x40], ebx                            ;           tick      (4 B)
0x3f22b2  movabs rax, 0x800000000000002a                 ; **tag 42** = queue_destroy_obstacle
0x3f22c6  call qword [rip+0x5c89b4]                      ; 事件队列 add_event
```

⇒ **入队条件（观测，指令级）**：`execute_results` 的事件队列里出现
"判别值 `0x8000000000000000`"（挖洞变体）的记录，就调用一次
`queue_destroy_obstacle(&mut <ObstacleManager 字段>, &(pos_x, pos_y, …), tick, **0.5**)`，
并把 `(pos_x,pos_y,0.5,tick)` 作为 tag 42 事件推进队列。
**`pos_x/pos_y` 就是那发的爆炸点**（§12.1 的 503/503），不是采样停点 —— 也就是说
"往哪挖"这件事在原版里**没有独立算式**，是直接复用爆炸点。

**未定**：这条记录是谁生产的（`calculate_paths` 一侧没追）。因此"撞什么算撞"这一层
（掩膜判据 vs 圆判据、`r+1` 抗锯齿环算不算）本轮没有指令级证据，见 §12.6。

### 12.5 落地修正与前后数字

`src/game.rs`：新增 `hole_point()`（`diff_eq` 局直接返回停点；其余局按
`to_text(parse(f))` + 补括号 + 3 位小数后缀重算 y，与
`_scaffold::diffconv::explosion_point` 同一条规则），`execute_results` 的挖洞块
（`QueueDestroyObstacle` / 掩膜挖洞 / 延迟的 `DestroyObstacle`）**三处都改用它**。

| 口径 | before | after | 复现 |
|---|---|---|---|
| **真实代码路径**（`--drive batch`，`Match::execute_results` 发的报文） | **1 / 259** | **134 / 259**（51.7%）| 见 §12.7 第 2 步 |
| 收敛层口径（`--drive conv`，§24.7 那根轴的同一 291 分母） | **66 / 291** | **198 / 291**（68.0%）| 见 §12.7 第 3 步 |
| 不命中 225 批 → 四类 | — | a 0 / b 0 / c 0 / d 200 | 见 §12.7 第 3 步 |

> ⚠️ **审计更正（[`docs/153`](153-audit-downstream.md) §1-B/§1-C，2026-11-07 夜独立复算）**：
> **`after` 两列都不是"合并后代码跑出来的数"**，请按下面这行读：
>
> | 口径 | 合并后**实测** | 说明 |
> |---|---|---|
> | 真实代码路径 `--drive batch`（`8755db9e`） | **139 / 258** | §12.7 第 2 步可复跑；表里的 134/259 复现不出来（分母也不同） |
> | 真实代码路径 `--drive batch`（master，含 `b49c3150` 括号修复） | **140 / 262** | 同上命令，`--drive fire` 轴是 139/262 |
> | 收敛层 `--drive conv` | **66 / 291**（未变） | `198/291` 是 `tools/hole_axis_attrib.py` 的「**改成**「挖洞点 = 爆炸点」**以后**」，即**反事实预测**：本任务的修改只落在 `src/game.rs`，`_scaffold/diffconv.rs` 未改 |

`cargo test --offline` 1514 通过（基线 1512 + 本次新增 2 条：
`game::tests::hole_point_is_the_explosion_point_not_the_sampled_stop`、
`game::tests::queued_and_expired_destroy_points_are_identical`）；
`python3 tools/coverage_matrix.py --check` OK。

### 12.6 仍然未知 / 缺什么证据

| # | 未定 | 缺什么 | 等级 |
|---|---|---|---|
| 1 | 挖洞事件（判别值 `0x8000…0`）的**生产者** | 顺着事件队列的 `Vec` 写入点反查（`calculate_paths` 一侧），或 frida 在 `0x472c61` 断点抓一次真实撞障碍的记录 | **未知** |
| 2 | `tick = [模块+0x58] + X/30 + 60` 里三项的语义（尤其 `X` 是点序号、距离还是时间） | 同一批里同时抓到 `active_tick`、`ExplosionEffect.start_tick`、`DestroyObstacle` tick 三者 + 该发点数 | **未知**（`DelayDestroy = 165` 仍是抓包经验值） |
| 3 | d2/d3 那 8 例的边界差 | 逐像素掩膜对照（`docs/142` §4 的 ring/fill 工具） | **未知** |
| 4 | 反方向（回放挖了、我方没撞） | 本轴只比"我方 blocked 且有销毁点"的批；反向要先有"漏挖"的探针 | **未知** |

### 12.7 复现（三份产物 + 三条命令）

```bash
export CARGO_HOME=<ws>/.reference/cargo-home
export CARGO_TARGET_DIR=<ws>/.agent-work/<你的工作区>/target      # 私有 target，别串味
R=<ws>/.reference/replays/gw2-replays-snapshot/replays             # 只读语料

# 1) 我方逐发几何（停点 / 爆炸点 / 挖洞点），产出 hole.jsonl
cargo run --release --offline --example hole_axis_probe -- --dir $R --json $OUT/hole.jsonl

# 2) 真实代码路径的"延迟段挖洞点命中"（Match::execute_results 的报文 → 实测 1/259 → 139/258，
#    合并 b49c3150 之后 140/262；见 §12.5 的审计更正）
cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive batch --policy strict --json $OUT/batch.jsonl
python3 tools/bit_exact_attrib.py --json $OUT/batch.jsonl

# 3) §24.7 那根轴的同一 291 分母 + 200 例四类分桶 + 每类样本
cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive conv --policy strict --json $OUT/conv.jsonl
python3 tools/hole_axis_attrib.py --conv $OUT/conv.jsonl --hole $OUT/hole.jsonl --dir $R
```

`tools/hole_axis_attrib.py` 会先打印"口径对齐：66 / 291"（对不上就说明口径漂了），
再打印 `before/after`、四类计数（带 `== 200` 的加总校验）、d 的三个子因、每子因 3 个样本。
