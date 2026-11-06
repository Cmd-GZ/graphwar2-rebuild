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

⚠️ **未知**：`0x9ba978` / `0x9bb030` 这两个 GOT 槽**没有**解出 gdext 引擎侧的成员名
（`set_visible`? `connect`?）。
缺的证据：运行时读 GOT 槽指向的引擎函数指针符号（Frida / 调试器）。
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
（`set_visible` / `set_disabled` / `set_process`）**没有**从二进制确认。
缺的证据：该 GOT 槽指向的引擎函数指针符号。

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
