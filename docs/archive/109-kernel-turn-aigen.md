# 109 · 内核两处缺口的实现：回合射手轮转 + 按士兵的 AI 进化层

状态：**已实现**（本分支 `agent/kernel-turn-aigen`），`cargo test` **172 条全绿**。
证据分级沿用仓库约定：**逐位验证**（`cargo test` 覆盖）/ **观测**（能指到地址或字节）/
**推断**（写明未证实）/ **未知**。地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。

本文是**实现说明**，不是新的逆向结论：射手规则由 `docs/98` 定案、`flip` 由 `docs/105` 定案。
本文只记录"落地成了什么形状""哪些地方二进制的字面与文档不同""还有什么没定"。

---

## 0. 一句话

> `docs/98` 的两条实现缺口补上了：
> ① `src/graphwar_turn_manager.rs` 从**队伍交替**（`alternating_order`，调用方给顺序）改成
> **每玩家 `Vec<soldier_id>` + 游标**的轮转（先自增再取、`& 1` 判活、全灭发 0）；
> ② `src/graphwar_function_evolutionator.rs` 加了**按士兵**的一层：回合系统先定 `soldier_id`，AI 只为它跑一次进化，
> 结果写进**这个士兵自己的**函数槽，**每回合重算、不共享**。
> 顺带：`flip` 由**队伍**决定（接口里没有坐标参数）、回合超时 = 跳过该玩家（**用户口述**，做成显式分支）。

---

## 1. 改动清单

| 文件 | 改动 | 依据 |
|---|---|---|
| `src/graphwar_turn_manager.rs` | **重写**：`Team` / `PlayerTurns` / `TurnManager` / `TurnDecision` / `EmptyRoster`；删掉 `alternating_order` | `docs/98` §1.2、§2、§7；`docs/105`；objdump（见 §2.2） |
| `src/graphwar_life_manager.rs` | 新增 `MISSING_STATE = 2` 与 `is_alive_byte()`（`& 1` 才是活）+ 2 条测试 | `docs/98` §1.3（`0x454950` / `0x463fbb`） |
| `src/graphwar_function_evolutionator.rs` | 新增"按士兵的进化层"：`FunctionSlot` / `SoldierFunctions` / `SoldierTurn` / `SoldierEvolver` + 6 条测试 | `docs/98` §3、§7 第 3 条 |
| `docs/109-kernel-turn-aigen.md` | 本文 | — |
| `docs/INDEX.md` | 加一行（§1.4 联机/回合 那张表） | 仓库约定 |
| `docs/47-turn-order-random.md` | §7 就地加一句"接口已换"的注（原句指向的 `alternating_order` 已不存在） | 防死引用 |

`src/` 之外**没有**动别的代码：`game.rs`、`_scaffold/protocol.rs`、`graphwar_game_server.rs` 均未改，
所以既有 21 条验收测试（`tests/`）一行没动也全绿（§5）。

---

## 2. `src/graphwar_turn_manager.rs`：轮转模型

### 2.1 数据结构

```rust
pub struct PlayerTurns { soldiers: Vec<i64>, index: usize }   // 4 字段表的 Rust 形态
pub struct PlayerEntry { team: Team, turns: PlayerTurns }
pub struct TurnManager { players: BTreeMap<i64, PlayerEntry>, empty_roster: EmptyRoster }
```

* `Team::{One, Two}`：`flip()` = `false` / `true`；`Team::from_id(3..)` 返回 `None`
  （`docs/105` §4 没观测到 3 队，不猜方向）；
* `TurnManager` 用 **`BTreeMap`** 而不是 `HashMap`：玩家之间的遍历顺序在真游戏里
  **本来就不可复现**（`docs/47`），这里至少保证我们自己的顺序是确定的；
* **不建模玩家出场顺序**（`docs/47` 判定为 HashMap 迭代序），由调用方给。

### 2.2 `next_shooter`：三态返回

```rust
pub fn next_shooter(&mut self, alive: impl Fn(i64) -> bool) -> Option<i64> {
    let n = self.soldiers.len();
    if n == 0 { return None; }                       // 空表：没有"当前士兵"
    if self.index >= n { self.index %= n; }
    for _ in 0..n {                                  // 最多 len 次
        self.index = (self.index + 1) % n;           // ① 先自增再取
        let s = self.soldiers[self.index];           // ②
        if alive(s) { return Some(s); }              // ③④
    }
    Some(SOLDIER_ID_NONE)                            // ⑤ 全灭 = 0
}
```

| 返回 | 含义 | 协议后果 |
|---|---|---|
| `Some(id)`, `id != 0` | 找到活着的射手 | 发 `TurnInfo{player, id}` |
| `Some(0)` | 有士兵但**全都不活** | **仍要**发 `TurnInfo{player, 0}`（`docs/98` §1.4） |
| `None` | 士兵表**为空** | 由 [`EmptyRoster`] 策略决定（见 §2.4） |

`Some(0)` 与 `None` 的差别是**有协议后果**的（`docs/98` §7），所以用 `Option` 而不是
用 0 一个值糊过去；`TurnDecision::broadcast()` 就是这个差别的开关。

### 2.3 本轮**新读**的二进制证据（objdump，逐条）

`docs/98` 给的是反编译片段；我按仓库规矩（`AGENTS.md`：结论要能指到字节）用 objdump 复核了一遍，
顺带发现**一处文档与二进制的字面冲突**（§2.4）：

```bash
objdump -d --start-address=0x463da0 --stop-address=0x464780 libgraphwarrust.so
```

| 行为 | 指令（原文） | vaddr |
|---|---|---|
| `len = entry-0x10`，`test` 后 `je` 到发 0 的块 | `mov -0x10(%rdx,%rax,8),%r15` / `test %r15,%r15` / `je 463fd0` | `0x463f27`、`0x463f2f` |
| 循环次数 = `len` | `dec %r14` / `jne 463f40` | `0x463fc7`–`0x463fca` |
| `index = (index+1) % len` 写回 `entry-0x8` | `mov -0x8(%rbp),%rax` / `inc %rax` / `div %r15` / `mov %rdx,-0x8(%rbp)` | `0x463f40`–`0x463f58` |
| `soldier = *(entry-0x18 + index*4)` | `mov -0x18(%rbp),%rcx` / `mov (%rcx,%rdx,4),%ebx` | `0x463f5c`、`0x463fa6` |
| 判活 = 调 `life_manager` 后只看最低位 | `call 454950` / `test $0x1,%al` / `jne 464050` | `0x463fb6`–`0x463fbd` |
| 活着：发 `{player_id, soldier_id}` | `mov -0x28(%rbp),%eax`→`+0x60`；`mov %ebx,0x64(%rsp)` | `0x46406b`–`0x464072` |
| **空表 / 全灭：发 `{player_id, 0}`** | `mov %ebx,0x60(%rsp)`；`movl $0x0,0x64(%rsp)` | `0x464016`–`0x464021` |

⇒ `docs/98` §1.1 的四个字段（key `-0x28`、`len -0x10`、指针 `-0x18`、游标 `-0x8`）与
§1.2 的五步**全部复现**，`docs/13` §4 那张"0x28 步长表"就是它。

### 2.4 ⚠️ 一处文档冲突：空 `Vec` 到底发不发 `TurnInfo{p, 0}`

`docs/98` 里两处字面不一致：

| 处 | 说法 |
|---|---|
| §1.4 观测 | `0x463da0` 的 map 分支对 `len == 0` **直接发 `TurnInfo{p, 0}`** |
| §7 伪代码注释 | `if n == 0 { return None; }  // 不发 TurnInfo` |

**objdump 站 §1.4**：`0x463f2f` 的 `je 0x463fd0` 与"试满 `len` 次"落下的是**同一个块**
（`0x463fd0`），那块把 `soldier_id` 写成 0 后发事件。也就是说**两种空走同一条路，都发 0**。

实现上把两件事**分开**（这是本文件最重要的一条设计决定）：

* **原语层** `PlayerTurns::next_shooter`：空表返回 `None`（对应 `0x4628d0` getter 不 push，
  `docs/98` §1.4 上表第一行）——这一层只回答"取不取得到"，不谈广播；
* **广播层** `TurnManager::begin_turn`：由 `EmptyRoster` 策略决定
  * `AnnounceZero`（**默认**，按 objdump / §1.4）：空表也发 `{player, 0}`；
  * `Skip`（按 §7 伪代码的字面）：不发。

⇒ 默认行为**跟二进制**，同时保留"按文档字面"的开关。**哪一个是真行为**严格来说仍差一步：
需要一个"士兵表为空的玩家进到 `0x463da0`"的现场（replay 或 REA 复核调用点）。
所以这一条在 §6 里仍登记为**未定**，只是现在的默认值与观测一致。

### 2.5 存活判定

`graphwar_life_manager.rs` 新增：

```rust
pub const MISSING_STATE: u8 = 2;                     // 0x454950 查不到时的返回值
pub const fn is_alive_byte(state: u8) -> bool { state & 1 != 0 }
```

* `0`（死）与 `2`（查不到 / 未上场）**都不可开火**（`docs/98` §1.3）；
* 任意奇数都算活——这是 `test $0x1` 的实际语义，不是"只有 1"；
* `graphwar_turn_manager::life_state_of(&LifeState)` 把 `Alive/Dead/没记录` 映射成 `1/0/2`。

### 2.6 `flip` 由队伍决定

`Team::flip()`：Team1 → `false`（朝 `+x`）、Team2 → `true`（朝 `-x`），`docs/105` 的
773 个位置样本 0 例外。

**不用坐标比大小猜**这件事由**类型**保证：`TurnManager` / `PlayerTurns` / `Team` 的
**任何接口里都没有坐标参数**，所以"用起点与终点 x 比大小"在结构上不可能出现。
AI 侧拿到的是 `TurnDecision::flip()`，直接填进 `graphwar_function_evolutionator::ScoringContext.flip`。

### 2.7 回合超时 = 跳过该玩家（**用户口述**）

用户口述："回合超时 = 跳过该玩家"（`docs/HANDOVER.md` §7.10 第 2 条；`docs/106` 亦有登记）。
**未独立验证**，所以只做成一个显式分支：

```rust
TurnManager::begin_turn(player_id, life_state, timed_out /* bool */)
  timed_out == true  ⇒  TurnDecision::Skip { reason: SkipReason::TurnTimeout }
```

实现里对它**没有**替游戏做主的三件事（都写进了代码注释与 §6）：

1. 超时**是否推进游标** —— 本实现**不推进**（等价于"这回合没轮到过他"）；
2. 超时那一刻 `TurnInfo` **是否已经广播过**（若已广播，调用方应忽略 `Skip`）；
3. 超时是"跳过一次"还是"这个士兵也算用掉一次轮转"。

---

## 3. API 迁移

### 3.1 删除与替换

| 旧 API（`main`） | 新 API | 说明 |
|---|---|---|
| `graphwar_turn_manager::alternating_order(teams, start_team, rounds) -> Vec<i64>` | **删除**。改用 `PlayerTurns::next_shooter*` + `TurnManager::begin_turn` | 全仓库**只有 `graphwar_turn_manager.rs` 自己的测试**调用它（`grep` 过：`src/`、`tests/`、`examples/` 里没有别的调用点），所以删除不影响任何生产代码 |
| （无） | `Team`、`PlayerTurns`、`TurnManager`、`TurnDecision`、`SkipReason`、`EmptyRoster`、`SOLDIER_ID_NONE`、`life_state_of` | 新增 |
| （无） | `graphwar_life_manager::MISSING_STATE`、`graphwar_life_manager::is_alive_byte` | 新增 |
| （无） | `graphwar_function_evolutionator::{FunctionSlot, SoldierFunctions, SoldierTurn, SoldierEvolver}` | 新增；**没有改动**既有 `aigen` API（`evolve` / `evolve_with_budget` / `next_generation` / `ScoringContext` 签名一字未动） |

### 3.2 旧测试怎么办

`alternating_order` 的 5 条测试按新语义**改写**（不是硬塞着让它还能过）：

| 旧测试 | 现在 |
|---|---|
| `captured_turns_always_alternate_teams` | 保留"相邻两次来自不同队伍"这条**观测**，但改用 `TurnManager` 表达，见 `captured_sequence_is_reachable_with_per_player_cursors` |
| `reproduces_case_a_with_explicit_team_order` / `..._case_b_starting_from_team2` | 合并进上一条（同一局抓包，靠**每玩家初始游标**复现） |
| `single_team_just_cycles` | 由 `cursor_increments_before_reading` 覆盖（单表轮转 + 回绕） |
| `empty_input_is_safe` | 由 `empty_roster_returns_none` + `empty_roster_announces_zero_by_default` 覆盖 |

⚠️ `captured_sequence_is_reachable_with_per_player_cursors` 里给 229 号玩家显式设了
`index = 1`（抓包首发是 `230`，而初值 0 会先取 `soldiers[1] = 231`）。**初始游标是两个
自由参数**（`docs/98` §6-B 未定），这个测试只证明"轮转模型能复现抓包"，**不是**结论。

### 3.3 对 `docs/47` §7 的影响

`docs/47` §7 原句是"`src/graphwar_turn_manager::alternating_order` 保持'调用方给顺序'的接口"。
该函数已删，所以在那句后面就地加了一行注（指向本文），避免死引用。
**`docs/47` 的核心结论不变**：玩家出场顺序仍不可复现，仍由调用方给——`TurnManager`
这一层只是**不再冒充**"顺序"这件事。

---

## 4. `src/graphwar_function_evolutionator.rs`：按士兵的进化层

### 4.1 三个硬结论（`docs/98` §3，replay 观测）

1. **AI 不选射手**：`soldier_id` 由回合系统先定（§2 的 `TurnDecision`）；
2. **槽按士兵**：`FunctionInfo.owner_id` / `FunctionActive.owner_id` 就是 `soldier_id`，
   开局每个士兵各收一条**空**函数；
3. **每回合重算**：同一士兵下一次轮到时文本又变了（tick 168 / 12269 / 26038 三行不重样）。

### 4.2 结构

```rust
pub struct FunctionSlot { pub owner_id: i64, pub tokens: Vec<Token>, pub updated_turn: Option<u64> }
pub struct SoldierFunctions { slots: BTreeMap<i64, FunctionSlot> }   // soldier_id → 槽
pub struct SoldierTurn { pub soldier_id: i64, pub turn: u64, pub tokens: Vec<Token>,
                         pub survivors: Vec<Individual> }
pub struct SoldierEvolver<'a> { params, mode, ctx: ScoringContext<'a>, budget_secs }
```

入口：

```rust
let out = evolver.evolve_soldier(&mut slots, soldier_id, turn_no, pop, &mut next);
```

* `evolve_soldier` 内部就是 `evolve_with_budget(...)` + 写回槽
  （`0x3f27d0` 的 Rust 形态），`out.tokens == survivors[0].tokens`；
* `with_budget(Some(GAME_TIME_BUDGET_SECS))` = 真游戏的 25 s 墙钟（`docs/93`）；
  `None` = 不限时（测试要可复现）；
* **没有"玩家 → 槽"这一层**：从类型上就不可能把同一玩家的两个士兵混在一个槽里。
* **`flip` 从哪来**：`ScoringContext.flip` 由调用方用 `TurnDecision::flip()`（= 队伍）填，
  **不是**从坐标推的（`docs/105`）。

### 4.3 为什么种群**不**放进槽

`docs/98` §6-A 明确未定：`computer_player_manager` 那张 `HashMap<i32, V>` 的键是
`player_id` 还是 `soldier_id`——也就是"进化的**种群**是每人一份还是每士兵一份"。
**输出函数按士兵存**这一点不受影响（§3 已观测），但**种群**的归属没观测到插入点。

所以 `SoldierEvolver` **不持有种群**：种群由调用方传进来，`SoldierTurn::survivors`
交回给调用方，由它决定下一回合复用给谁。这不是省事，是不想让一个**未知**变成本实现的
隐含约定。

### 4.4 典型调用链（与真链路对齐）

```text
turn_manager（src/graphwar_turn_manager.rs）        选好 soldier_id，给出 flip（来自队伍）
   ↓
computer_player_manager（未实现）  取该士兵的局面
   ↓
SoldierEvolver::evolve_soldier    为**这一个**士兵进化（25 s 预算）
   ↓
SoldierFunctions::set             写进 soldier_id 自己的槽
   ↓
FunctionInfo{owner_id = soldier_id}   ← 文本化仍未实现，见 §6-H
```

---

## 5. 测试

`cargo test`（自己的 `CARGO_TARGET_DIR`，离线）：

```
lib        138 passed   （改动前 116，+22）
events       5
evidence     6
golden       4
live_hits   12
plane        6
server_match 1
------------------------------
合计       172 passed / 0 failed   （改动前 150）
```

新增 22 条的分布：

| 模块 | 条数 | 覆盖 |
|---|---|---|
| `graphwar_turn_manager.rs` | 19（原本 5 条，删/改写后净 +14） | 轮转顺序（先自增）、**三局 replay 逐发复刻**、跳死人、全灭 `Some(0)`、`Some(0)` vs `None` 的广播差别、单士兵、空队（两种策略）、`flip` 由队伍、抓包序列可达、**超时跳过**、未知玩家、状态字节 `& 1`、`LifeState` 适配、`soldier_id=0` 拒绝入表 |
| `graphwar_life_manager.rs` | 2 | 生命表往返；`MISSING_STATE == 2` 与"只有最低位算活" |
| `graphwar_function_evolutionator.rs` | 6 | 开局空槽；**两士兵互不干扰**；**同一士兵下一回合重算（覆盖）**；槽里是第 0 名；预算接到 25 s；`soldier_id` 来自调用方 |

三条 replay 测试是**直接用 `docs/98` §2 的真值**当断言（不是自己推的例子）：

| 测试 | 真值来源 |
|---|---|
| `replay_1782763805_player_7443_rotation` | `docs/98` §2.1：`7445 → 7448 → 7444` |
| `replay_1793884702_player_1053_rotation` | §2.2：`1054 → 1055 → 1085 → 1083` |
| `replay_1793874543_player_621_alternates` | §2.3：`623 → 627 → 628 → 627 → …`（并复刻 623 中途阵亡） |

`aigen` 的两条关键测试用"空场下 token 最少者必赢"把结果**钉死**，不依赖 RNG：

* `two_soldiers_of_one_player_have_separate_slots`：A 种 `[Const 0.0]`、B 种 `[Const 7.0]`，
  断言两个槽内容不同、且进化 A 时 B 的槽（内容 + `updated_turn`）完全不动；
* `same_soldier_is_recomputed_next_turn`：同一士兵第 1 回合 `[Const 0.0]`、第 2 回合
  `[Const 7.0]`，断言槽被**覆盖**、`updated_turn` 前进 ⇒ 不是"算一次缓存住"。

> 说明：`pop` 里**只放一条**个体、且它是 1 token 时，`next_generation` 的工厂子代
> （随机重生，长度 ≥1）在空场下不可能比它更短；同分时 `sort_by` 是稳定排序，
> 克隆排在子代前面 ⇒ 第 0 名确定。这是本测试的**构造依据**，不是对 AI 行为的假设。

---

## 6. 仍未确定（明确列出，别当已解）

| # | 未知 | 缺什么 | 影响 |
|---|---|---|---|
| A | **游标初值**（`docs/98` §6-B）：三例吻合 0，但 `1782763022` 首取 `2509` 不符 | 更多 replay / 二进制的初始化点 | 本实现默认 0，另给 `with_index` |
| B | **`Vec<soldier_id>` 的填充顺序**（`docs/98` §6-C）：实测 = `SoldierInfo` 创建顺序，但没跟到 push 点 | `SoldierInfo` 容器的插入点 | **若来自 HashMap 遍历 ⇒ 每局不同 ⇒ 整局不可复现**（与 `docs/47` 同级） |
| C | 空 `Vec` 是否真的发 `TurnInfo{0}`（§2.4 的文档冲突） | 一个"空表玩家进 `0x463da0`"的现场 | 默认按 objdump（发 0），保留 `EmptyRoster::Skip` |
| D | 进化**种群**的归属（`docs/98` §6-A）：`computer_player_manager` 的 map 键是 player 还是 soldier | 插入点 | 本实现不持有种群（§4.3） |
| E | 两种回合模式的**发射时序**（`docs/98` §6-D） | 分包时序细读 | 本实现只做单次判决，不管时序 |
| F | **`SequentialTurns` vs `SimultaneousTurns` 的玩家顺序**（`docs/47`） | 不可复现（判为 HashMap 序） | 由调用方给；本模块不冒充 |
| G | **回合超时**的三件事（§2.7） | 用户口述之外的验证 | 只留分支，不猜游标/时序 |
| H | **token → 文本**（`0x4ec160`）未实现 | 该函数的逐 case 还原 | 槽里只能存 token；`FunctionInfo.function` 的文本仍要外部给 |
| I | 第 0 代初始种群生成路径（`docs/95` §5、`HANDOVER` §7.11.1） | 同上 | `seed_population` 仍用 `initial_population` |
| J | 词法/语义：`soldier_id = 0` 在真游戏里是否可能是合法士兵 | 未观测到 0 号士兵 | 本实现把 0 当哨兵，入表会 panic |

### 6.1 还没接线（明确说明）

`TurnManager` 与 `SoldierEvolver` **只到模块层**，还没有接进现有装配：

* `src/game.rs::begin_turn(player_id, soldier_id, ...)` 仍由**调用方**给 `soldier_id`
  （它只负责拼报文），没有改成从 `TurnManager` 取；
* `src/graphwar_game_server.rs`（自建客户端联机用的 harness 服务端）里的
  `current = Some((p.player_id, p.soldier_id))` 来自 harness 自己的玩家表，
  不是轮转算出来的；
* `src/graphwar_function_evolutionator.rs` 的进化也还没有被任何回合推进调用（此前就没人调用 `evolve`）。

不改的理由：`HANDOVER.md` §7.11.2 把**服务端接线**列为"暂缓"（等协议补全），
而改 `game.rs` 的签名会牵动 `tests/server_match.rs` 等验收测试。
⇒ 现在的状态是"规则已实现并可测"，**不是**"端到端已经走新规则"。

---

## 7. 合并时**还要改**的文档（本分支没改，留给父 agent）

这几处现在**已经过时**，但改它们超出本分支范围（或会与其它分支撞车）：

| 位置 | 现在的说法 | 合并后应改成 |
|---|---|---|
| `docs/00-status.md` §0 "内核侧仍缺 ① ②" | "`src/graphwar_turn_manager.rs` 还没实现…""`aigen` 缺…" | 已实现，见 `109` |
| `docs/HANDOVER.md` §0 同两条 | 同上 | 同上 |
| `docs/AI.md` §8.4 末段"对实现的缺口" | "`src/graphwar_turn_manager.rs` 需要实现这套轮转…" | 改成"已实现，见 `109`" |
| `docs/98` §7 的伪代码注释 | `if n == 0 { return None; } // 不发 TurnInfo` | 与 §1.4 冲突已记在本文 §2.4；`98` 是结论文档，未改 |
| `docs/47` §7 | "`alternating_order` 保持…" | 已就地加注（指向本文） |

---

## 8. 复现

```bash
cd .agent-work/kernel/repo
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=/home/user/MyProject/gw2/.agent-work/kernel/target
cargo test --offline          # 172 passed

# 那条新证据（空表与全灭共用发 0 的块）
objdump -d --start-address=0x463da0 --stop-address=0x464780 libgraphwarrust.so \
  | sed -n '/463f2f:/p;/463fc7:/p;/463fca:/p;/463fd0:/p;/464016:/p'
```
