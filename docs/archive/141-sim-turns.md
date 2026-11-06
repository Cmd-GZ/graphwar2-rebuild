# 141 · 同时回合制（SimultaneousTurns）：三段流程、广播分组与顺序、对拍

状态：**现行**。本文把 `docs/86` 的"结算顺序"结论落成**可执行的实现**（收集 → 结算 →
广播三段），并把对拍里最大的一处结构分歧（"一次结算按消息类型分组发 N 发"）**定成规则**，
再用 `examples/sim_turns.rs --replay-sim` 在 401 份真回放上量化。

证据分级沿用仓库约定：**逐位验证**（`cargo test` 覆盖）/ **观测**（能指到地址、字节或
运行时值）/ **推断**（写明缺什么）/ **未知**。

---

## 0. 一句话

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

## 1. 完整流程（三段）

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
| 同一回合所有 `FunctionActive.active_tick` 相同 | 13 / 13 回合（`docs/86` §6） | 观测 |

### 1.1 实现落点

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

## 2. 结算阶段：几何并行，事件按弧长串行

### 2.1 几何并行（沿用 `docs/86` §5.1 的结论，本轮把它写进代码）

批量结算时，**每一发的弹道都对着"批开始那一刻"的士兵表与掩膜算**，谁死不影响别人：

```rust
// game.rs::fire_batch —— 先把 N 发全部算完，再动状态
for s in shots { outcomes.push(resolve(..., &self.mask, &others)); }
```

依据：`docs/86` §5.1 的 **152/152、多报 0**——死亡集合恰好等于"逐发独立"的并集，
而且**不需要**把已死者从目标表里移除（士兵不挡弹道，`docs/30`）。

### 2.2 事件按**飞行时间**串行

一个士兵在一个回合里**只死一次**，归属**最先飞到他身上的那颗弹丸**
（`docs/86` §4.2：34:0（step=0.5）、38:0（step=0.01）；§5.2：152/152 唯一死亡特效）。

本轮把它实现成"**按弧长排序**"，并且这个换算**不需要知道弹丸速度**：

> 同一回合所有弹丸的 `active_tick` **完全相同**（13/13 回合，`docs/86` §6）⇒
> 所以"谁先到" ⇔ "谁的弧长更短"。那个 ≈0.29 单位/tick 的平均速度**不参与排序**。

实现：

```rust
// graphwar_shot_calculator::settle::resolve_points —— 沿弹道累计弧长，每次命中记下来
arc += ((px - qx).powi(2) + (py - qy).powi(2)).sqrt();
hits.push(Hit { id: t.id, arc, index });

// game.rs::fire_batch —— 合并后按 (弧长, 发序号, 士兵 id) 升序，同一士兵只留第一次
merged.sort_by(|a, b| a.0.partial_cmp(&b.0)... .then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
```

**排序键为什么是三元组**：弧长相等时用发序号、再用士兵 id 兜底，纯粹是为了**确定性**。
真游戏在"两发同时到达"时怎么判**仍然未知**（`docs/86` §7：样本里最近的一对差 4.0 tick，
没有真并列）。这条兜底规则是**重建版自有规则**，不影响任何已观测样本。

---

## 3. 广播阶段：分组与顺序（本轮的核心产出）

### 3.1 规则（`观测`，130 个多发批全中）

> **前缀按类型分组，后缀按发交替，死亡块按飞行时间排。**

1. `FunctionInfo × N` —— 连排，**同一顺序**；
2. `FunctionPoints × N` —— 连排，**与①同一顺序**；
3. 死亡特效块 —— `EffectInfo(DeathEffect)` 与 `EffectInfo(AshEffect)` **混在一起**、
   **一个连续的块**，按 `start_tick` 升序（106/106）；
4. `(FunctionActive(owner_i), EffectInfo(ExplosionEffect))` —— **每发一对**，
   顺序与①②相同（130/130）；
5. `QueueDestroyObstacle` 的位置**回放里没有证据**（`docs/136` §1.2：400 份里 0 次），
   本实现沿用单发实现的"死亡块之后、`FunctionActive` 之前"，这样 `N = 1` 时与旧代码逐字节相同。

### 3.2 "谁排前面"：`TurnInfo` 的公告顺序（**不是** A 类的自由处置）

一开始我也以为这是一个 HashMap 迭代序（A 类，`docs/113`），**实测否掉了这个偷懒结论**：

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
（`docs/113` §1.1 已实测：钉住 `getrandom(16)` 后跨进程一致）。本实现不碰它——
`begin_simultaneous_turn` **照抄调用方给的 `players` 顺序**，接线层（`graphwar_game_server`）给的是
`Config::player_order` 或玩家表的构造顺序。所以：

* **批内分组与顺序：规则已定，130/130 复现**；
* **`TurnInfo`/批的玩家排列：A 类**，需要与 `docs/113` 同一份熵钉住才能逐局复现。

### 3.3 死亡块里 `DeathEffect` 与 `AshEffect` 混排

106 个带死亡块的多发批里，`DeathEffect` 218 条、`AshEffect` 208 条，两者**交错**
（例：`AshEffect×1, DeathEffect×1, AshEffect×2`），但整体仍按 `start_tick` 升序。
**哪种情况发哪一个：未知**（缺 `EffectInfo` 发射路径的复核）。
当前实现**只发 `DeathEffect`**（与改动前一致），`AshEffect` 仍是 `docs/136` §4.3 记的缺口。

---

## 4. 对拍实测（旧 / 新）

### 4.1 口径（一个字都没改）

与 `docs/136` §1.2–§1.4 **完全相同**：同一份 `_scaffold::replaydiff::Policy`、同一个
`_scaffold::replaydiff::compare_batch`（投影 → 折叠重复 → 分块 → 块内多重集 → 字段级 diff）。
唯一改的是**预测驱动**：

* **驱动 A（旧）**：每发调一次 `Match::fire`，把 N 次的报文拼起来 —— 这正是
  `_scaffold::replaydiff::Predictor::predict` 现在的做法；
* **驱动 B（新）**：一个批调一次 `Match::fire_batch`。

两者跑同一份语料、同一个 `Match`，所以差异只可能来自驱动方式。

```bash
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=/home/user/MyProject/gw2/.agent-work/simturns/target
cargo run --release --offline --example sim_turns -- --replay-sim
# 语料目录可用第二个位置参数覆盖；GW2_REPLAY_DIR 也认
```

### 4.2 语料（本轮**定格快照**：401 份）

语料目录是活的（本轮跑的过程中还多出 1 份），所以下面所有数字都跑在一份
**复制的定格快照**（401 份、1.9 MB）上，可复跑。

| 回合模式 | 文件 | 有开火 | 结算批 | 开火 |
|---|---|---|---|---|
| `SimultaneousTurns` | 68 | 59 | 108 | 269 |
| `SequentialTurns` | 322 | 223 | 972 | 1020 |
| 无 `TurnModeInfo` | 11 | 0 | 0 | 0 |
| **合计** | **401** | **282** | **1080** | **1290** |

多发批（`>= 2` 发）共 **130** 个：`SimultaneousTurns` 101 个、`SequentialTurns` 29 个
（后者是同 tick 撞在一起的巧合，形状一样）。

### 4.3 结果

| 指标 | 驱动 A（旧） | 驱动 B（新） |
|---|---|---|
| **同时回合制：结算批内容命中** | 0 / 108（0.0%） | 0 / 108（0.0%） |
| **同时回合制：消息种类序列命中** | **5 / 108（4.6%）** | **77 / 108（71.3%）** |
| 顺序回合制：消息种类序列命中 | 818 / 972（84.2%） | 839 / 972（86.3%） |
| **多发批：消息种类序列命中** | **0 / 130（0.0%）** | **93 / 130（71.5%）** |
| **多发批：批结构（剔掉死亡块）** | **0 / 130（0.0%）** | **130 / 130（100%）** |
| 全语料：消息种类序列 | 823 / 1080（76.2%） | **916 / 1080（84.8%）** |
| 全语料：批内容命中 | 21 / 1080（1.9%） | 21 / 1080（1.9%） |
| 全语料：完全一致的回放 | 1 / 401（0.2%） | 1 / 401（0.2%） |

**"批结构"** 这一行是本轮的核心指标：把死亡/灰烬特效块**整个剔掉**之后，块的种类序列
是否相同。它把"分组与排序规则对不对"和"死亡集合对不对"两件事彻底分开。

### 4.4 剩下 37 个多发批：**没有一个是分组错**

驱动 B 仍失败的 37 个多发批，按第一处分歧拆开，只有两类，**两类都是死亡块的"在不在"**：

| 现象 | 个数 | 成因 |
|---|---|---|
| 我方有 1 个死亡特效块、回放**没有** | **19** | 这 19 个批的死亡块**整块都是 `AshEffect`**（实测：130 个多发批里"死亡块只含 `AshEffect`"的正好 **19** 个）。`Policy` 把 `AshEffect` 当"我方不产"投影掉 ⇒ 比较器眼里回放没有死亡块。**是缺口，不是分组错。** |
| 我方没有死亡块、回放**有** | **18** | 死亡集合漏报（`docs/136` §6 第 8 条：我方弹道在环像素上过早停住）。**是几何/掩膜缺口，不是分组错。** |

⇒ 把死亡块这一项修好（Ash 发射器 + 死亡集合），同时回合制的种类序列应当直接到 108/108。

### 4.5 与 `docs/136` 的历史数字对比

`docs/136` §4.1 的表是**改步长之前**（`fire` 写死 `step = 0.5`）跑的；主库后来把步长
改成 `SERVER_STEP`（0.01），所以基线本身也动了。三个时点：

| 时点 | 结算批内容 | 消息种类序列 | 完全一致 |
|---|---|---|---|
| `docs/136`（step=0.5，400 份） | 18 / 1079（1.7%） | 815 / 1079（75.5%） | **0 / 400（0%）** |
| 本轮起点（step=0.01，400 份） | 21 / 1079（1.9%） | 822 / 1079（76.2%） | **1 / 400（0.2%）** |
| 本轮起点（step=0.01，401 份定格快照） | 21 / 1080（1.9%） | 823 / 1080（76.2%） | 1 / 401（0.2%） |
| **本轮之后（驱动 B，401 份）** | 21 / 1080（1.9%） | **916 / 1080（84.8%）** | 1 / 401（0.2%） |

> **步长那一改的净效果**：完全一致 0 → 1 份、种类序列 75.5% → 76.2%、
> 内容 1.7% → 1.9%。比 `docs/136` 预期的要小，因为**"完全一致"这条判据同时要求
> `corrected_function` / `end_x` / `pos_x` / `pos_y` 全部逐字节相同**，
> 而 `EffectInfo.pos_y` 这条二路径问题（`docs/136` §5 第 8 条）本轮没有解决。

### 4.6 **完全一致率仍然是 0**——但原因换了一批

同时回合制 59 份"有开火"的回放，**完全一致 0 / 59**（驱动 A 与 B 都是 0）。
不是这次没修好，而是"完全一致"要求**每一个字段**都对，而目前同时回合制的批还差三样：

| 差距 | 影响 |
|---|---|
| 死亡集合（漏报 / 多报） | `docs/136` §6 第 8 条；本语料上延迟段死亡集合 211/379（55.7%） |
| `AshEffect` 发射器缺失 | 19/130 多发批；全语料 363 条 |
| 爆炸点 = 停点假设不成立 | `EffectInfo.pos_y` 与我们的停点只有 33/739 逐位相同（`docs/136` §5 第 8 条） |

⇒ **同时回合制不再是结构上的瓶颈**。下一步的收益在死亡集合与爆炸点那两条上，
它们对 `SequentialTurns` 的收益同样大（96.2% 的结算批还没内容命中）。

---

## 5. 仍不确定的

| # | 未确定 | 缺什么 | 等级 |
|---|---|---|---|
| 1 | `TurnInfo` / 批内玩家排列的**具体置换** | 与 `docs/113` 同一份 `getrandom(16)` 熵；本实现照抄调用方顺序 | **A 类**（不可由输入决定） |
| 2 | **收集阶段的超时**：多久算超时、超时后那名玩家是否仍消耗一次轮转 | 二进制或抓包里的超时路径；`EndOfTurnInfo.end_of_turn_tick` 在真回放里**恒为 0**（`1793884702` 三回合），60Hz tick 已上万 ⇒ 那个字段不能当截止。`Config::sim_turn_timeout_ticks` 只留旋钮，**不猜数值** | **未知** |
| 3 | `DeathEffect` vs `AshEffect` 的选择规则 | `EffectInfo` 发射路径的复核；本轮只观测到"两者混排、整体按 `start_tick` 升序" | **未知** |
| 4 | `QueueDestroyObstacle` 在批里的位置 | 回放不记录这类消息（400 份 0 次）；抓包里能拿到但没做逐批对位 | **未知**（实现里显式声明是重建版规则） |
| 5 | **两发同时到达**同一士兵（真并列）谁赢 | 样本里最小间隔 4.0 tick，没有并列（`docs/86` §7） | **未知**；实现用 `(弧长, 发序号, id)` 兜底 |
| 6 | 弹丸飞行速度 | 只知道中位 ≈0.29 单位/tick（p10~p90 0.255~0.300）；**排序不需要它**，只有"绝对时刻"才需要 | **推断** |
| 7 | `active_tick` 的取值 | 观测到"同批所有发相同"，但绝对值来自哪个时钟没解；实现里用 `now_tick` | **推断** |
| 8 | 回合开头那两条 `FunctionInfo("", null) × 全部士兵` 与 `TurnCountInfo` | 只观测到形状，没有发射器；本轮**没有实现**（不在批窗口内，不影响对拍口径） | **未知** |
| 9 | `DiffEq` 模式的同时回合制 | `Match::fire_batch` 与 `fire` 一样硬编码 `Mode::Normal`；515 个 DiffEq 批本轮仍未覆盖 | **未知** |

---

## 6. 验收

```bash
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=/home/user/MyProject/gw2/.agent-work/simturns/target
cargo test --offline          # 333 → 347（新增 tests/sim_turns.rs 14 条）
```

`tests/sim_turns.rs` 的 14 条：

| 测试 | 钉住的东西 | 档 |
|---|---|---|
| `two_shots_resolve_in_one_batch` | 两发一批的种类序列 + 三个 owner 序列一致 | 观测（130/130 批） |
| `batch_shape_scales_with_n` | `N` 变大只改条数、不改形状 | 观测 |
| `mutual_kills_in_the_same_batch` | 弹道不交叉时谁都不死（对照） | 观测 |
| `mutual_kill_both_die_in_one_batch` | **两发互相击杀 / 同 tick 双亡**，死亡块位置 | 观测 |
| `deaths_are_ordered_by_flight_time` | **死亡块按弧长排**，与发序可判别 | 观测（`docs/86` §4.2） |
| `one_soldier_dies_once_attributed_to_the_earliest_shot` | 一个士兵只死一次、归最早到达 | 观测（152/152） |
| `blocked_shot_does_not_stop_the_other` | **一发被墙挡、另一发照常命中** | 观测 |
| `single_shot_is_byte_identical_to_fire` | `fire` = `fire_batch(N=1)`，单发报文逐字节不变 | 逐位验证 |
| `collection_waits_for_every_announced_player` | 攒齐才结算；顺序 = 公告顺序 | 观测 |
| `collection_resolves_on_timeout_without_the_silent_players` | 超时也结算，没交的不出现 | 观测（9 个真样本） |
| `collection_rejects_duplicates_and_strangers` | 重复 / 名单外 / 已关闭 | 逐位验证 |
| `empty_announcement_is_immediately_complete` | 空名单不卡回合 | 逐位验证 |
| `a_full_simultaneous_round_runs_all_three_phases` | 收集 → 结算 → 广播三段连通 | 观测 |
| `simultaneous_round_over_tcp_emits_one_grouped_batch` | **真 TCP**：攒齐前一个字都不回；攒齐后回**一个**批 | 观测 |

`tests/server_match.rs` 的断言**一条没动**（3 passed / 1 ignored），
`Config::turn_mode` 默认 `"SequentialTurns"`，所以那两条 e2e 走的还是原路径。

---

## 7. 与既有文档的关系

* `docs/86` §6 的"按飞行时间串行"由本文**落成代码**（`graphwar_shot_calculator::settle::Hit.arc` +
  `fire_batch` 的排序），§8 那条"要改的是事件生成"在这里做完了；
* `docs/136` §5 第 5 条（"同时回合制一次结算按消息类型分组发 N 发"）由本文**定成规则**，
  §6 第 3 条（"同时回合制的事务化"）由本文完成；
* `docs/113` 的 A 类在这里**收窄**：批内**分组与顺序**不再是 A 类（规则可复现），
  只剩"`TurnInfo` 那组玩家排列本身"还是 A 类；
* `docs/00-status` / `HANDOVER` §0 里"同时回合制结算（152/152、时刻判决 38:0）"
  说的是**物理与死亡集合**；本文补的是**报文层的分组与顺序**。
