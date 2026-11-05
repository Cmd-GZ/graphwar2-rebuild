# 117 · 服务端接线：射手轮转 + 按士兵的 AI 进化

状态：**已接线**（分支 `agent/server-wiring`），`cargo test` = **216 条全绿**（接线前 205）。
证据分级沿用仓库约定：**逐位验证**（`cargo test` 覆盖）/ **观测**（能指到地址或字节）/
**推断**（写明缺什么）/ **未知**。地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。

本文是**接线说明**，不是新的逆向结论：轮转规则由 `docs/98` 定案、实现见 `docs/109`、
玩家顺序不可复现见 `docs/47`、逐位口径见 `docs/06` §17。本文只记录
"接进了哪个函数""端到端怎么断言""哪些地方仍然没证据"。

---

## 0. 一句话

> `docs/109` §6.1 留的尾巴（"规则已实现并可测 ≠ 端到端走新规则"）补上了：
> 回合开始走 `TurnManager::begin_turn`（三态），**先定射手、再只为它**跑一次
> `SoldierEvolver::evolve_soldier`，**25 s 预算默认接上**；`serverloop` 的
> "谁下一个开火"不再由它自己算。
> 仍然没接的：`token → 文本`（AI 的函数上不了线）、回合超时的**检测**、游标初值的**配置入口**。

---

## 1. 接了什么

| 文件 | 改动 | 依据 |
|---|---|---|
| `src/game.rs` | 新增 `TurnManager` / `SoldierFunctions` / `LifeState` / AI 配置四块状态；新增入口 `Match::begin_next_turn`、`Match::begin_next_turn` 内部的 `Match::evolve_for_soldier`；`Match::register_player` / `set_bot`；`fire()` 顺手把死者记进生命表 | `docs/98` §0–§3、`docs/109` §2/§4、`docs/105` |
| `src/serverloop.rs` | 回合推进改成 `advance()`（走 `Match::begin_next_turn`）；`Config` 增加 `player_order` / `bot_players` / `ai_level` / `ai_mode` / `ai_budget_secs` / `ai_pop_scope`；新增 `TurnRecord` / `TurnLog` 与 `serve_stream_logged` | `docs/47`（顺序由调用方给）、`docs/67`（难度档）、`docs/93`（25 s） |
| `tests/server_match.rs` | 新增端到端 `rotation_and_ai_are_wired_over_tcp`；原 `plays_a_full_match_over_tcp` **只换了夹具种子**（理由见 §5） | — |
| `docs/117-server-wiring.md` | 本文 | — |

**没动**：`src/turn.rs`、`src/aigen.rs`（**签名一字未改**，只调用）、`src/protocol.rs`、
`src/life.rs`、`src/gdext*.rs`、`client/`、`docs/INDEX.md`、`docs/HANDOVER.md`。

### 1.1 两个接线入口

```rust
// ① src/game.rs —— 一个回合的开始（唯一入口）
Match::begin_next_turn(player_id, timed_out, now_ms, time_mode) -> TurnStart
    // a) turn::TurnManager::begin_turn(player_id, life_state_of(&life), timed_out) -> TurnDecision
    // b) Announce{soldier_id != 0} ⇒ evolve_for_soldier(player_id, soldier_id, flip, turn_no)
    // c) decision.broadcast() == Some((p, s)) 才调既有 Match::begin_turn(p, s, …) 拼报文
    //    （Some(0) 仍发、None 不发 —— 就是 broadcast() 那个开关）

// ② src/serverloop.rs —— 下一回合轮到谁
advance(m, order, at, log, now_ms, time_mode) -> Advance
    // Some(id≠0) 停在这一位；Some(0) 把 TurnInfo{p,0} 放进报文后继续找；
    // Skip 一条报文都不发，继续找；最多 order.len() 次。
```

`Match::begin_turn(player_id, soldier_id, …)`（旧 API）**保留原样**，只负责拼
`TurnInfo → EndOfTurnInfo → GameStateInfo` 三条报文，`Match::new` 的签名也没变，
所以 `docs/109` §5 那批既有单测一条没改。

### 1.2 三态落地（`docs/98` §1.4 / `docs/109` §2.2）

| `TurnDecision` | 报文 | AI | 分级 |
|---|---|---|---|
| `Announce{id}`，`id != 0` | 发 `TurnInfo{p,id}` + 两条 | 为 `id` 进化 | **逐位验证**（`turn_rotation_decides_the_shooter`） |
| `Announce{0}`（有士兵但全死） | **仍发** `TurnInfo{p,0}` | 不跑（没有士兵） | **观测**（`0x463fd0`）/ 单测 `all_dead_roster_still_announces_zero` |
| `None`（空士兵表） | 按 `EmptyRoster` 策略（默认 `AnnounceZero` **也发 0**） | 不跑 | **观测**（同上）+ **推断**（"空表也进这条分支"仍差一个现场，`docs/109` §6-C） |

`docs/109` §2.4 那处文档冲突（§1.4 说发 0、§7 伪代码说不发）**原样保留**：
`turn.rs` 的两个策略一个都没删，服务端用默认的 `AnnounceZero`（按 objdump）。

### 1.3 AI：先定射手，再只为它进化

```rust
// game.rs::evolve_for_soldier —— 三种情况**不跑**（返回 None）
soldier_id == 0        // 全灭哨兵，真游戏也没有 FunctionFire（docs/98 §1.4）
!is_bot(player_id)     // computer_player_manager 只服务电脑玩家（docs/98 §4）
射手不在场上            // 已死 / 没登记 ⇒ 没有局面可评
// 跑的时候：
ScoringContext { x0, y0, flip, step: COARSE_STEP, mask: &self.mask,
                 targets: 场上除射手以外的人, limit: params.limit }
SoldierEvolver::new(aigen::params(level), mode, ctx).with_budget(budget)
    .evolve_soldier(&mut self.functions, soldier_id, turn_no, pop, &mut next)
```

* **`flip` 从轮转来**：`TurnDecision::flip()`（= 队伍，`docs/105`）。`serverloop` 里
  开火时也改成读 `TurnManager::flip_for`，"谁开火"和"朝哪边"因此只有一个来源；
* **槽按士兵**：写回的是 `soldier_id` 自己的槽，同玩家的另一个士兵完全不动
  （单测 `ai_evolves_only_for_computer_players_and_only_its_own_soldier`）；
* **每回合重算**：`evolve_soldier` 覆盖写 + `updated_turn` 前进（`docs/109` §4.2 已测）；
* **种群**：`aigen` 不持有种群，接线层按 `PopScope`（默认按士兵）缓存上一代存活者，
  见 §6 第 ② 条。

### 1.4 25 s 预算：默认接上

```rust
AiConfig::default().budget_secs == Some(aigen::GAME_TIME_BUDGET_SECS)   // = Some(25.0)
Config::default().ai_budget_secs   == Some(25.0)
```

`docs/109` §6.1 记的"`evolve()` 默认传 `None`，25 s 上限实际没生效"在**接线路径**上不再成立：
`Match` 的 AI 配置默认值就是 25 s，并且原样交给 `SoldierEvolver::with_budget`。
（`aigen::evolve` 这个便捷函数本身仍是 `None`，没改它的签名——接线路径不走它。）

| 断言 | 分级 |
|---|---|
| 常量 = 25.0（`0x51b7cf cmp $0x19`） | **观测**（`aigen` 的 `game_budget_constant_matches_disassembly`） |
| `with_budget` 到点就停 | **逐位验证**（`aigen::time_budget_stops_early_without_changing_semantics`） |
| 本接线层把 25 s 传进去 | **逐位验证**（`ai_budget_defaults_to_the_game_value` 断言默认值与转发字段） |

⚠️ 一旦预算真的截断，这一局就**不再逐位**（`docs/06` §17.2 的 B 类：取决于机器速度）。
所以端到端测试传 `ai_budget_secs: None`，只保留"到点就停"的语义而不引入墙钟。

---

## 2. `src/serverloop.rs`：回合推进怎么变了

改之前：

```rust
turn += 1;
let alive = players.filter(还在场上);            // 玩家表是"士兵表"的形状
let p = alive[turn % alive.len()];                // ← 谁开火由这里拍
```

改之后：**玩家表只用来存局面**（坐标/函数/队伍），"谁开火"一律问 `Match::begin_next_turn`：

```text
GameStartRequest
   ├ 生成障碍（levelgen，seed）
   ├ 生成士兵（固定布局，见 §5）+ 每玩家的士兵表 rosters
   ├ Match::new(...).with_ai(AiConfig).with_ai_seed(seed)
   ├ register_player(玩家, 队伍, 士兵表)   ← 顺序**原样**入表，不排序
   ├ set_bot(...)                          ← Config::bot_players
   └ at = order.len()-1; advance(...)      ← 第一回合落在 order[0]

FunctionFireRequest（当前玩家）
   ├ fire(8 条) → tick(到期销毁) → end_turn
   └ advance(...)  → 下一位的 TurnInfo{p,id} + 两条
```

* `order`（玩家出场顺序）= `Config::player_order`；留空则用**构造顺序**。
  **两者都是"调用方/配置给"**，本文件不推导（`docs/47`）；
* `Some(0)` 的处理：把 `TurnInfo{p,0}` 发出去后**立刻**推进到下一位
  （这回合没有 `FunctionFire`，不推进就会卡住）。真游戏靠哪一层推进**未知**
  （`docs/98` §6-D/E），这一条是**重建版自有规则**；
* 若一圈都没有能开火的玩家 ⇒ 结束连接（`current = None`）。

### 2.1 观测日志（不进协议）

`TurnLog { records: Vec<TurnRecord> }`，每条 =
`{player_id, soldier_id: Option<i64>, skip_reason, flip, evolved, function_tokens}`。
`serve_stream_logged()` 把它交回来。

**为什么需要它**：AI 进化出来的函数是 **token**，而 `token → 文本`（`0x4ec160`）没实现
（`docs/109` §6-H），所以 AI 的产物**在报文里根本看不到**。这条日志是
"AI 真的为这个士兵跑过"的唯一可观测痕迹。它**不写进协议**（不新增事件，不猜键名）。

---

## 3. 端到端测试怎么断言

`tests/server_match.rs::rotation_and_ai_are_wired_over_tcp`（真 TCP，2 个玩家 × 2 个士兵，
`seed = [11; 32]`，`bot_players = [229, 230]`，`ai_budget_secs = None`），客户端很笨：
每回合都发 `FunctionUpdateRequest{"0.00"}` + `FunctionFireRequest`。断言分四组：

| # | 断言 | 它在证明什么 |
|---|---|---|
| 1 | `announcements[0] == (229, soldiers[1]=231)`，且**第一个玩家的首发射手必须是 `soldiers[1]` 而不是 `[0]`** | 游标初值 0 + **先自增再取**（`docs/109` §2.2）；若是"取当前再自增"这条会立刻红 |
| 2 | 每次广播的 `soldier_id` == 测试自己按 `docs/98` §1.2 实现的轮转模型（活人集取自报文的 `LifeQueuedDeath`） | 轮转 + **跳过死者** + 回绕都在线上成立；模型是测试里**独立写的**，不复用被测代码 |
| 3 | 玩家出场顺序严格交替（`229 → 230 → 229 → …`） | 玩家顺序来自配置/构造顺序（`docs/47`），不是 HashMap 序 |
| 4 | 日志里**每条广播了非 0 士兵的记录都 `evolved == true` 且 `function_tokens > 0`**，且 `evolved_soldiers() == announced` | AI 为**那一个**士兵进化过，且顺序与射手序列一一对应 |
| 5 | 每个记录的 `flip` == 该士兵 `TeamInfo` 的队伍（`Team1 → false / Team2 → true`） | `flip` 由队伍决定（`docs/105`），不看坐标 |
| 6 | 这条局能打完（`EndingGame`），且**至少死过一个人** | "跳过死者"不是空断言 |

本 seed 的实际走向（7 回合）：`231 → 232 → 230 → 232 → 231 → 232 → 230`，
第 0 回合 231 打死 233、第 1 回合该玩家就**跳过死掉的 233** 广播 232（第 2 组断言第一次生效），
第 6 回合 230 打死 232 ⇒ Team2 全灭 ⇒ `EndingGame`。

### 3.1 另外 10 条单测（`src/game.rs`）

无遮挡的四人对局，所以"这一发打死谁"是写死的，不依赖关卡随机：

| 测试 | 钉住的行为 |
|---|---|
| `turn_rotation_decides_the_shooter` | 轮转序列 `231 → 233 → 230 → 232 → 231`、三态报文、`flip` 由队伍 |
| `dead_soldier_is_skipped_by_the_rotation` | 打死 233 后该玩家广播 232 |
| `all_dead_roster_still_announces_zero` | `Some(0)` **仍发** `soldier_id:0`，且不进化 |
| `timeout_skips_the_player_without_moving_the_cursor` | `SkipReason::TurnTimeout` + 不发报文 + 不进化 + 游标不动 |
| `unknown_player_is_skipped` | `SkipReason::UnknownPlayer` |
| `ai_evolves_only_for_computer_players_and_only_its_own_soldier` | 非电脑玩家不进化；进化写进**射手自己的**槽 |
| `evolution_needs_a_real_soldier_on_the_field` | `soldier_id = 0` / 不在场上 ⇒ 不跑 |
| `wiring_is_deterministic_for_the_same_seed` | 同 seed 同消息 ⇒ 四个槽**逐位相同**（`docs/06` §17.1 的口径） |
| `ai_budget_defaults_to_the_game_value` | 25 s 默认 + 转发 |
| `population_scope_is_explicit` | `PopScope` 两种读法在缓存条数上分叉（4 vs 2） |

### 3.2 `tests/server_match.rs` 为什么要改（以及改了什么）

**只改了一处：种子 `[3; 32]` → `[11; 32]`；所有断言一字未动。**

理由（行为后果，不是为了让测试过）：接线后先手从 `soldiers[0]`（230）变成
`soldiers[1]`（231），同一颗种子的局面走向因此变了。`[3; 32]` 会走到
"231 被 233 打死、232 被 230 打死"的**交叉局面**：两队各剩一名士兵，且分别停在
`y = 0` 与 `y = −4` 两条横排上 —— 客户端只会发 `"0.00"`（水平线），
**几何上永远打不到对方**，于是 `assert!(ended)`（`EndingGame`）不可能成立。
这不是接线写错，而是"固定直线客户端 + 固定布局"的必然僵局。

做法：保留全部断言（含 `EndingGame`、`SoldierInfo` 条数 4 条、握手序列），
换成一颗能打完的种子。取值过程（可复现，未入库的临时脚本）：
把 `serverloop` 的回合推进在进程内重放（同样的 `levelgen` 种子 + 每回合 `fire("0.00")`），
扫 `1..=64` 号种子，能在 12 回合内打出 `EndingGame` 的是
**11（7 回合）/ 17（9）/ 32（9）/ 52（11）/ 55（3）/ 63（11）**；取 11。

> 这是**夹具取值**，不是结论：任何种子都不改变服务端行为，只改变关卡障碍。
> 换种子换来的是"这条测试还能跑完一局"，不是"把断言放宽"。

---

## 4. 也没接的：AI 的产物还没上线

`token → 文本`（`0x4ec160`，`docs/98` §4 已定位，逐 case 未还原）没实现 ⇒
`FunctionSlot.tokens` 有了，但**发不出 `FunctionInfo{owner_id: soldier_id, function: "…"}`**。
所以本接线后：

* AI **真的**每回合为该士兵进化（`Match.evolutions` / `TurnLog` 可观测）；
* 进化的结果**进了这个士兵自己的槽**；
* 但报文里那条 `FunctionInfo` 的文本仍然来自客户端的 `FunctionUpdateRequest`。

⇒ 端到端"AI 会打函数"这件事**仍未闭环**，缺的就是 `0x4ec160`。这也是本文件
把"AI 接线"标成**逐位验证（状态转移）/ 未知（线上产物）**的原因。

---

## 5. 重建版自有规则（明确声明，不是复原结论）

| # | 规则 | 为什么必须自定 | 真游戏 |
|---|---|---|---|
| 1 | **玩家出场顺序** = `Config::player_order`，留空则用玩家表的构造顺序 | `docs/47`：来自 HashMap 迭代序（OS 熵播种的 SipHash），**不可确定性复现** | **未知/不可复现** |
| 2 | 服务端的**士兵布局**（每队 `per_team` 人，`x = ∓10`、`y = -4 + 4k`） | 真实关卡是随机的，这里是"固定以便复现"（`docs/62` 早已声明） | 随机 |
| 3 | `Some(0)`（该玩家全灭）之后**立刻**推进到下一位，并把 `TurnInfo{p,0}` 先发出去 | 不推进就会卡住；真游戏靠哪一层推进**没有观测** | **未知**（`docs/98` §6-D/E） |
| 4 | 默认 `ai_level = 1` | `RoomState` 没有难度字段；真游戏难度是房主设置（`docs/67` 六档） | 房主设置 |
| 5 | 默认 `ai_pop_scope = 按士兵` | `docs/98` §6-A 的键（player / soldier）**没观测到插入点** | **未知** |

---

## 6. 仍未确定（别当已解决）

| # | 未知 | 缺什么 | 本实现的取法 |
|---|---|---|---|
| ① | **游标初值**（`docs/98` §6-B）。三例 replay 吻合 0，但 `1782763022` 首取 `2509` 不符 | 更多 replay / 二进制初始化点 | 只能是 `PlayerTurns::new` 的 0。**`TurnManager::add_player` 没有 index 参数**，所以接线层**没法**把它做成配置项 —— 这是唯一一个"要改 `turn.rs` 才能接的东西"，本轮**没改**（`turn.rs` 是别的 agent 刚交付的），需要时加一个 `add_player_with_index` 即可 |
| ② | **进化种群的归属**（`docs/98` §6-A）：键是 player 还是 soldier | 插入点 | 显式开关 `PopScope`，默认**按士兵**；单测证明两种读法可观测地分叉 |
| ③ | **回合超时的检测**（用户口述"超时 = 跳过该玩家"，**未独立验证**） | 谁触发超时、超时那一刻 `TurnInfo` 是否已广播、游标推不推 | 只做了**分支**：`begin_next_turn(..., timed_out=true)` ⇒ `SkipReason::TurnTimeout`，游标不动；`serverloop` **永远传 `false`**（没有任何计时线程，加计时器等于引入新的墙钟不确定源） |
| ④ | **`Some(0)` 之后谁推进**（§5 第 3 条） | `0x463da0` 两分支的发射时序（`docs/98` §6-D） | 重建版立刻推进 |
| ⑤ | **空士兵表**到底发不发 `TurnInfo{p,0}`（`docs/109` §2.4 的文档冲突） | 一个"空表玩家进 `0x463da0`"的现场 | 用默认 `AnnounceZero`（按 objdump），`EmptyRoster::Skip` 保留 |
| ⑥ | **`token → 文本`（`0x4ec160`）** | 逐 case 还原 | 未实现 ⇒ AI 产物上不了线（§4） |
| ⑦ | **第 0 代种群**（`docs/110`） | 那条"缓存空 ⇒ 每槽随机 + 截断 + 变异"的链路 | `seed_population` 仍用 `initial_population`（与 `docs/109` 一致，未退步也未前进） |
| ⑧ | **变体 B**（`docs/110`：`function_mode != Normal` 走 `0x51d080`） | 实现 | **未实现**；`AiConfig.mode` 是 `aigen::VarMode`（`docs/94` 的变量权重），**没有**从 `room.function_mode` 推导——那个字符串到 `VarMode`/变体 A/B 的对应没有证据 |
| ⑨ | **顺序模式 vs 同时模式** | `docs/98` §6-D | 只跑 `SequentialTurns`（`serverloop` 的房间状态写死），同时模式没接 |
| ⑩ | **整局逐位** | 回放对拍（`docs/06` §17.3 的"预测下一条报文并逐字节比对"） | 没做。本轮的验收口径只有 §3 那条：**给定同一串消息 ⇒ 状态转移逐位** |

### 6.1 一处已知的时序偏差（写明白）

真实链路是**先发 `TurnInfo`，AI 随后才算**（`docs/98` §5 观测：bot 的
`TurnInfo → FunctionFire` 差 **45–226 tick**，即 0.75–3.8 s）。本实现把
"定射手 → 进化 → 拼报文"放在**同一次调用**里，于是 `TurnInfo` 的**投递时刻**被
进化推迟了（报文里的 tick 仍取回合开始时的 `now_ms`，**字节不变**）。

* 报文内容：不受影响（**逐位验证**：`turn_rotation_decides_the_shooter` 断言三条报文）；
* 投递时刻：受影响。它属于 `docs/06` §17.2 的 **B 类**（墙钟），本来就不进逐位口径；
* 想让顺序完全对齐，就是把 `begin_next_turn` 拆成"定射手/拼报文"与"进化"两次调用；
  本轮没拆，因为 AI 的产物还上不了线（§4），拆了也没有可观测收益。

---

## 7. 结论分级汇总

| 结论 | 级别 |
|---|---|
| 射手由 `TurnManager` 轮转决定：每玩家一张 `Vec<soldier_id>` + 游标、**先自增再取**、跳死者、全灭发 0 | **观测**（`0x463f27`–`0x464072`，`docs/98` §1.2）+ **逐位验证**（三局 replay + `turn.rs` 单测） |
| 接线后"谁开火"确实走这条规则 | **逐位验证**（`game.rs` 单测 + 端到端 §3） |
| AI 只为判出的那个士兵进化一次，写它自己的槽，每回合重算 | **观测**（`docs/98` §3.2 三行 tick）+ **逐位验证**（`game.rs`/`aigen` 单测） |
| 25 s 预算已接上（默认值 + 转发） | **逐位验证**（字段断言）+ **观测**（`0x51b7cf` 常量） |
| AI 的函数出现在报文里 | **未知**（缺 `0x4ec160`） |
| 玩家出场顺序 / 游标初值 / 种群归属 / 超时 / `Some(0)` 的推进 | **未知**（§5、§6） |
| 整局逐位 | **未知**（回放对拍未做） |

---

## 8. 复现

```bash
cd .agent-work/wire/repo
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=/home/user/MyProject/gw2/.agent-work/wire/target
cargo test --offline                       # 216 passed（接线前 205）

# 只看接线相关的几条
cargo test --offline --lib game::tests     # 含 10 条新单测
cargo test --offline --test server_match   # 2 条端到端（一条原有、一条新增）
```

跑一局看日志（`eprintln!` 里有 `STARTED … order=[...] bots=[...] ai=levelN budget=...`）：

```bash
cargo run --offline --quiet --example serve_match -- 127.0.0.1:39997
```
