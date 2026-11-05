# 127 · AI 说话：进化出来的函数怎么上线

状态：**已接线**（分支 `agent/ai-speaks`），`cargo test --offline` = **279 条全绿**（272 → 279）。
证据分级沿用仓库约定（`AGENTS.md` 三条硬规矩）：**逐位验证**（`cargo test` 覆盖）/ **观测**
（能指到地址或字节，或真实的报文帧）/ **推断**（写明缺什么证据）/ **未知**。
地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。

---

## 0. 一句话

> `docs/117` §4 记的那个缺口（"AI 真的每回合进化，但报文里那条 `FunctionInfo` 的文本
> 仍然来自客户端"）**关上了**：`function::to_text`（`0x4ec160`，`docs/119` 1032/1032 对拍）
> 现在有调用点，`Match::begin_next_turn` 在进化之后就把这个士兵的 token 写成文本广播出去，
> 开火时用的也**就是同一份文本**，于是修正版的 `FunctionInfo` 里带的是 AI 真正打出去的那个函数。
> 两条报文的形态**逐字段照真实抓包**（下面 §2 的帧号可复查）：
> 提交时 `corrected_function` 是 `null`，**修正版在开火那一刻才发**
> （`CalculatingResults → FunctionInfo → FunctionPoints`，`docs/43` 的既有顺序一字未动）。

---

## 1. 缺口在哪（接线前）

| 事实 | 等级 |
|---|---|
| `aigen` 的产物是 **token**（`FunctionSlot.tokens`）；协议要的是**文本** | **观测**（`src/aigen.rs`、`docs/24` §6.2 的真实帧） |
| `token → 文本` = `0x4ec160`，已还原并逐字符对拍 1032/1032 | **逐位验证**（`docs/119`，`fixtures/to_text.tsv`） |
| 但没人调它 ⇒ 报文里那条 `FunctionInfo` 的文本来自客户端的 `FunctionUpdateRequest` | **观测**（接线前的 `src/serverloop.rs`） |

⇒ 接线前，AI "会思考、不会说话"：`TurnLog` 里只有 token 计数，线上看不到它的函数。

---

## 2. 真实报文长什么样（先看真值，再谈实现）

原始字节流抓包（`tools/capture_net_raw.js`，`docs/24` §4.5、`docs/49` 的复现命令）。
同一进程的两条 socket 都在里面：

* **fd 16 `out`** = 对局服务器的**下行**序列（`TickReply` / `TeamInfo` / `FunctionInfo` / `FunctionPoints` …），
  共 259 帧；
* **fd 15 `out`** = 同一进程作为**客户端**的请求（`NewConnectionRequest` / `FunctionUpdateRequest` /
  `FunctionFireRequest` …），共 105 帧。

帧号 = 该方向流内的序号（从 0 起）。下面每一行都是**原文**，不是改写过的：

```text
fd16  73 {"FunctionInfo":{"owner_id":233,"function":"","corrected_function":null}}       ← 开局四条空函数
fd16  74 {"FunctionInfo":{"owner_id":231,"function":"","corrected_function":null}}
fd16  75 {"FunctionInfo":{"owner_id":230,"function":"","corrected_function":null}}
fd16  76 {"FunctionInfo":{"owner_id":234,"function":"","corrected_function":null}}

fd15  36 {"FunctionUpdateRequest":{"player_id":232,"function":"cos((0.19*(6.70+x)))"}}   ← 机器人交函数
fd16 154 {"TurnInfo":{"player_id":232,"soldier_id":234}}
fd16 155 {"EndOfTurnInfo":{"end_of_turn_tick":3959,"time_mode":"Timer1m"}}
fd16 156 {"GameStateInfo":{"game_state":"WaitingForFunctions"}}
fd16 158 {"FunctionInfo":{"owner_id":234,"function":"cos((0.19*(6.70+x)))","corrected_function":null}}   ← 提交版
fd16 159 {"FunctionFire":{"soldier_id":234}}
fd16 160 {"GameStateInfo":{"game_state":"CalculatingResults"}}
fd16 161 {"FunctionInfo":{"owner_id":234,"function":"cos((0.19*(6.70+x)))","corrected_function":"cos((0.19*(6.70+x))) + 3.204"}}   ← 修正版
fd16 162 {"FunctionPoints":{"owner_id":234,"start_x":11.962551145879328,"start_y":3.7443842094294837,"end_x":-18.75932385411869}}
```

### 2.1 三条形态（都按字节）

| # | 结论 | 证据 |
|---|---|---|
| 1 | 一个回合有**两条** `FunctionInfo`：**提交版**（`corrected_function: null`）在 `WaitingForFunctions` 之后、开火之前；**修正版**在 `CalculatingResults` 与 `FunctionPoints` 之间 | **观测**：帧 158/161、帧 200/203、帧 73–76 的初始批 |
| 2 | `owner_id` 是 **`soldier_id`**（不是 `player_id`）：`FunctionUpdateRequest{player_id:232}` 的回执是 `owner_id:234`，与同回合 `FunctionPoints.owner_id` 相同 | **观测**：帧 36 → 158 → 162（`AGENTS.md` 硬事实、`docs/79`） |
| 3 | 修正文本 = **"函数文本 + 带符号的常数（三位小数）"**：`+ 3.204` / `- 3.420` / `- 11.416` | **观测**：帧 161/143/182/203 |

第 3 条顺带把修正常数的**定义**又验了一遍：帧 161 的 `start_x = 11.9626`、`start_y = 3.7444`，
`flip = true`（队 2），`cos(0.19*(6.70-11.9626)) = 0.5404` ⇒ `c = 3.7444 - 0.5404 = 3.204` ✓；
帧 143 的函数是空串（`f ≡ 0`）⇒ `c = y0 = -3.4200779` ⇒ 文本 `" - 3.420"` ✓。

> ⚠️ **这一条暴露了 `src/shot.rs::corrected_text` 的一个形态 bug**（详见 §5 第 1 条）：
> 它写死 `format!("{} + {:.3}", …)`，于是 `c < 0` 时发出去的是 `" + -3.420"`，
> 而真机是 `" - 3.420"`。仓库里其实**已经有**对的那个函数（`shot::correction_text`，
> 返回 `" - 1.000"` 那种形态），只是 `corrected_text` 没用它。本轮**不许改 `shot.rs`**，
> 所以只记录 + 在 `docs/127` 与交付说明里点名，见 §5。

### 2.2 机器人也是客户端

`player_id = 232` 的名字是 `Agent Smith`（`docs/92` 的电脑玩家名单），它交上来的两条函数
（`cos((0.19*(6.70+x)))`、`abs((abs((-0.57+x))+-4.85))`）都是 `to_text` 的产物
（全括号、`+-4.85` 这种负常量的写法，人手不会这么打），走的却是**和人类完全一样**的两条请求：

```text
FunctionUpdateRequest{player_id, function}  →  FunctionFireRequest{player_id}
```

⇒ 真游戏里"AI 说话"不是一条特殊报文，而是**机器人客户端替它的士兵提交了一个函数**。
本仓库的机器人住在服务端，所以这两处改由服务端代劳（§3、§5 第 2 条）。

---

## 3. 接线点

### 3.1 `src/game.rs`

| 位置 | 作用 |
|---|---|
| `Match::ai_function_text(soldier_id)` | **`function::to_text` 的唯一调用点**：槽里的 token → 文本（`None` = 没有槽） |
| `Match::ai_submit_info(soldier_id)` | 拼**提交版** `FunctionInfo{owner_id: soldier_id, function, corrected_function: null}` |
| `Match::begin_next_turn` | 进化**之后**把上面那条 `push` 进本回合的报文（只在 `evolution.is_some()` 时） |

顺序（`begin_next_turn` 的产物，与 §2 的帧 154–158 一一对应）：

```text
TurnInfo{p,s} → EndOfTurnInfo → GameStateInfo(WaitingForFunctions) → FunctionInfo{s, 文本, null}
```

**修正版不在这里发**：它仍然由 `Match::fire` 产出（`docs/43` 的顺序），
`CalculatingResults → FunctionInfo{s, 文本, "f(x) ± c"} → FunctionPoints → …`。

`corrected_function` 的文本就是 `crate::shot::corrected_text(&text, x0, y0, flip)`，
与 `docs/119` §10 给下一步写的三行接口一致：

```rust
let text = crate::function::to_text(&slot.tokens);            // ai_function_text
let corrected = crate::shot::corrected_text(&text, x0, y0, flip);   // Match::fire 内部
```

### 3.2 `src/serverloop.rs`

| 位置 | 改法 | 依据 |
|---|---|---|
| `FunctionUpdateRequest` 的回执 | `owner_id` 从 **`player_id`** 改成 **本回合的 `soldier_id`**；电脑玩家的回执文本用 AI 的，而不是客户端交上来的 | **观测**：帧 36 → 158。此前那行发的是 `player_id`，是个 bug（同一局里玩家 230 与士兵 230 甚至同号，错得看不出来） |
| `FunctionFireRequest` | 电脑玩家的开火函数 = `Match::ai_function_text(soldier)`（客户端交的 `"0.00"` 不生效） | **观测**：机器人自己交函数、自己开火（§2.2）；**重建版自有规则**见 §5 第 2 条 |
| `TurnRecord` | 增 `function_text` / `corrected`（**不进协议**） | 让"该发什么"与"发了什么"在日志里能对上，端到端测试就是拿它比报文 |

人类玩家**一点没变**：他们的函数仍来自 `FunctionUpdateRequest`，回执也仍是 `corrected_function: null`。

---

## 4. 怎么验证的

### 4.1 端到端（真 TCP，`tests/server_match.rs::ai_functions_are_broadcast_and_round_trip_over_tcp`）

两个电脑玩家（229/230）× 2 名士兵，`seed = [11; 32]`，`ai_budget_secs: None`
（墙钟截断属 `docs/06` §17.2 的 B 类，测试里关掉；难度 1）。客户端很笨：每回合只发
`FunctionUpdateRequest{"0.00"}` + `FunctionFireRequest`。逐回合断言：

| # | 断言 | 它在证明什么 |
|---|---|---|
| 1 | 回合开始（还没开火）就有一条 `FunctionInfo`，`owner_id` == 本回合 `soldier_id`，`function` **非空**且**没有空格** | AI 真的说话了；`to_text` 不加空格（`docs/119` §3.1） |
| 2 | `parse(text)` 非空，且 `to_text(parse(text)) == text` | **往返闭合**（这批文本是不动点，`docs/119` §5.4 的那 7.4% 例外本局没踩到） |
| 3 | `FunctionUpdateRequest` 的回执 `owner_id` 仍是 `soldier_id`，且**客户端的 `"0.00"` 改不动**电脑玩家的函数 | `owner_id` 语义 + "AI 才是作者" |
| 4 | 开火窗口恰好是 `GameStateInfo(CalculatingResults) → FunctionInfo → FunctionPoints`，中间那条 `function` 与第 1 步**逐字符相同** | 顺序照真机（帧 160–162）；**打出去的就是它说的那条** |
| 5 | `corrected_function` == `"{text} + {:.3}"`，常数用**独立路径**复算：`c = y0 - eval(parse(text), ±x0)`，`x0/y0` 取自同一回合 `FunctionPoints.start_x/start_y`，`flip` 取自 `TeamInfo`；并断言小数位恰好 3 位 | 形态与真实样本一致（§2.1 第 3 条），且**不复用**被测函数的私有计算 |
| 6 | 至少打 4 回合；`TurnLog` 里每回合的 `function_text` 与报文逐字符相同、`evolved == true`、`function_tokens > 0` | 不是"某一回合过了就算"，且日志与线上同源 |

本 seed 的实际走向（7 回合，两 AI 轮流说话，先手仍是 `soldiers[1] = 231`）：

```text
231 "-13.99" → 232 "5.53" → 230 "14.42" → 232 "5.89" → 231 "-13.99" → 232 "5.89" → 230 "14.42"
```

> 难度 1 的进化在这条测试里收敛到**常函数**（水平线）——那是 AI 的合法产物，
> 也顺带覆盖了"负常量"的文本形态（`-13.99` = `neg(const 13.99)`，`docs/119` §5.3）。
> 曲线形态由 `src/aigen.rs` 的既有测试覆盖，不是本条的任务。

### 4.2 单元测试（`src/game.rs`，6 条）

| 测试 | 钉住的行为 |
|---|---|
| `ai_turn_broadcasts_its_function_as_text` | 回合头三条 + `FunctionInfo` 一条；`owner_id == soldier_id`；文本 == 槽里 token 的 `to_text`；`corrected_function` 为 `null`；与 `TurnStart.evolution` 同源 |
| `human_turns_do_not_speak` | 人类玩家一条都不多（不替人类编函数） |
| `each_ai_speaks_for_its_own_soldier` | 两个 AI 各发各的，`owner_id` 各自是自己的士兵，文本各自对得上自己的槽（互换就红） |
| `ai_function_info_owner_is_the_soldier_not_the_player` | 报文里只有 `owner_id`、没有 `player_id` 键；开火那条也是同一个 owner |
| `empty_slot_speaks_the_captured_empty_form` | 空槽 ⇒ `{"FunctionInfo":{"owner_id":231,"function":"","corrected_function":null}}`（与帧 74 同形）；空函数的修正文本 == 抓样 `" + 10.990"` |
| `ai_text_round_trips_through_the_parser` | 四回合的 AI 文本全部是 `parse`/`to_text` 的**不动点** |

### 4.3 条数

```
lib 231 · broker 4 · events 6 · evidence 6 · golden 4 · live_hits 12 · plane 6
server_match 3 · to_text 7                                   ⇒  279 passed（起点 272）
```

七个新增 = `src/game.rs` 6 条 + `tests/server_match.rs` 1 条。**既有断言一条都没改**，
`tests/server_match.rs` 里原有的两条（含 `rotation_and_ai_are_wired_over_tcp`）原样通过。

---

## 5. 重建版自有规则与已知偏差（都写明白，不藏在代码里）

| # | 事项 | 真游戏 | 我们 | 理由 / 缺什么 |
|---|---|---|---|---|
| 1 | `corrected_function` 在 `c < 0` 时的形态 | `" - 3.420"`（**观测**：帧 143/182/203） | `" + -3.420"`（`shot::corrected_text` 写死 `" + "`） | `src/shot.rs` **不在本轮可改范围**；修法是把 `corrected_text` 改成 `format!("{}{}", text, correction_text(&toks, x0, y0, flip))`——那个函数**已经在仓库里**且形态正确（`" + 0.591"` / `" - 1.000"`），改完 `docs/24` §6.2 的两个正样本仍然逐字通过。端到端测试第 5 条钉的是**当前**形态，修好后要同步改成 §2.1 第 3 条的真机形态 |
| 2 | 电脑玩家的函数由谁决定 | 机器人**客户端**自己算、自己交（帧 36/60） | **服务端**决定，客户端交上来的文本对它不生效 | 本仓库没有"机器人客户端"这一层，AI 住在 `Match` 里。写在这里是因为它是**架构差异**，不是复原结论 |
| 3 | 提交版 `FunctionInfo` 的**时机** | `TurnInfo` 之后 45–226 tick（`docs/98` §5，机器人算 0.75–3.8 s） | 与 `TurnInfo` **同一次调用**发出（tick 字段仍是回合开始的 `now_ms`，字节不变） | 与 `docs/117` §6.1 记的是同一条 B 类偏差（墙钟），不改进度公式 |
| 4 | 同文本重复提交要不要再播 | `docs/83` §1：复用上一回合的函数**不会**再播 `FunctionInfo`；`0x4789b3`/`0x4789dc` 把两份文本各算一遍、`0x478a03` 调 `memcmp`、`0x478b23` 把结果写成槽前一个字节，看着就是"变了才播" | **无条件播**（AI 每回合都重新进化，就当它是新提交） | **推断**：那段比较的上下游没读完（`docs/119` §9-E 同款缺口）。本局出现过同一文本两次（`231 "-13.99"`），两条都发了。若后续实现去重，端到端测试第 1 条要改成"提交**或**开火那条里能看到它" |
| 5 | 不在自己回合里提交 `FunctionUpdateRequest` 时的 `owner_id` | 没观测到这种时序 | 退回 `player_id`（并注明是**未知**） | 抓包里所有 `FunctionUpdateRequest` 都在本人回合内 |

---

## 6. 仍未确定

| # | 项 | 缺什么 |
|---|---|---|
| ① | **同时回合制**（`docs/86`）里多个 AI 同 tick 说话的顺序 | 房间状态写死 `SequentialTurns`（`docs/117` §6-⑨）；`SimultaneousTurns` 的发射时序**没实现**，也没有抓包对照。本轮的"多个 AI 各说各的"是在**顺序回合制**下验证的（每个玩家各自回合一条）。接线本身不需要为同时模式改：每条 `FunctionInfo` 都由**该士兵自己的** `begin_next_turn` 产出（同一个函数、同一条报文），缺的只是"谁先谁后"——而那也是 `docs/113` 记的 A 类（批内顺序来自 HashMap 迭代序，缺一份未捕获的熵） |
| ② | 提交版 `FunctionInfo` 的去重条件（§5 第 4 条） | `0x4789b3`–`0x478b23` 那段 `memcmp` 的上下游（谁读那个字节、读了之后发不发报文） |
| ③ | `corrected_function` 在 `c = 0` / `-0.0` / `NaN` 时的符号 | `correction_text` 按 `c >= 0.0` 取 `" + "`、`NaN` 走 `' + NaN'`（`src/shot.rs` 的注释），但 `corrected_text` 走的是 `{:.3}`；真机这三例都没抓到 |
| ④ | 客户端拿到 `FunctionInfo.function` 后是否按**文本**重画曲线 | `docs/119` §9-C 的老问题；若重画，§5 第 1 条那种形态差异会让"客户端画的"与"服务端算的"不一致 |
| ⑤ | 机器人是否每回合都真的提交（而不是复用） | 只有一局抓包里的两次机器人提交（帧 36/60）；`docs/83` §1 的另一条观测是"复用就不播"，两者不矛盾但也没合并成一个结论 |

---

## 7. 结论分级

| 结论 | 级别 |
|---|---|
| `0x4ec160`（token→文本）就是这个缺口的补丁；`FunctionInfo.function` 是它的产物 | **观测**（`docs/119` §4.3 的三个外部调用点 + 本文件 §2 的帧：机器人的两条函数都是 `to_text` 的形态） |
| 一个回合两条 `FunctionInfo`（提交版 `null` / 修正版带 `f(x) ± c`），位置如 §2 | **观测**（帧 154–162、200–203；`docs/43`） |
| `FunctionInfo.owner_id` = `soldier_id` | **观测**（帧 36 → 158 → 162）+ `AGENTS.md` 硬事实 |
| 接线后 AI 的函数进报文、可 `parse` 回来、与开火用的那条逐字符相同 | **逐位验证**（§4.1 的 6 条断言 + §4.2 的 6 条单测） |
| `corrected_function` 的常数 = `y0 - f(±x0)`（三位小数） | **逐位验证**（测试用 `eval` 独立复算）+ **观测**（帧 143/161 的 `start_x/start_y` 手算吻合） |
| `corrected_function` 在 `c < 0` 时的**符号形态**我们与真机不同 | **观测**（帧 143/182/203 vs `shot::corrected_text`）；修法见 §5 第 1 条，**未改** |
| 机器人的函数来自服务端而不是客户端 | **重建版自有规则**（§5 第 2 条） |
| 同时回合制、去重、`c = 0/NaN` 的形态 | **未知**（§6） |

---

## 8. 复现

```bash
cd .agent-work/aispeak/repo
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=/home/user/MyProject/gw2/.agent-work/aispeak/target
cargo test --offline                                   # 279 passed
cargo test --offline --lib game::tests                 # 含 6 条新单测
cargo test --offline --test server_match               # 3 条端到端（本条新增 1 条）
cargo test --offline --test server_match ai_functions -- --nocapture
```

看线上到底发了什么（`eprintln!` 里有 `RECV` / `STARTED` / `DONE`）：

```bash
cargo run --offline --quiet --example serve_match -- 127.0.0.1:39997
```

重抓真机报文（需要本机有游戏；见 `docs/49` §）：

```bash
node tools/capture_net_raw.js 20000 60    # 原始字节流，按 u16 大端长度前缀重组
```

---

## 9. 这次碰了哪些文件

| 文件 | 动作 |
|---|---|
| `src/game.rs` | 模块注释改成"AI 说话"的现状；新增 `ai_function_text` / `ai_submit_info`；`begin_next_turn` 多发一条提交版 `FunctionInfo`；**新增 6 条单测** |
| `src/serverloop.rs` | `FunctionUpdateRequest` 回执的 `owner_id` 改成 `soldier_id` 且电脑玩家用 AI 文本；`FunctionFireRequest` 用 AI 文本开火；`TurnRecord` 增 `function_text` / `corrected`；模块注释补"AI 说话"一节 |
| `tests/server_match.rs` | **新增** `ai_functions_are_broadcast_and_round_trip_over_tcp` + 3 个解析小工具；原有 2 条断言一字未改 |
| `docs/127-ai-speaks.md` | **新增**（本文） |

**没动**：`src/api.rs`、`src/aigen.rs`、`src/turn.rs`、`src/protocol.rs`、`src/function/`、
`src/shot.rs`、`src/levelgen*.rs`、`client/`、`docs/INDEX.md`、`docs/HANDOVER.md`、
任何既有夹具与既有测试的断言。
