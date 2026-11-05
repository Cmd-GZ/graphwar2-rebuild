# 136 · 回放消息级对拍：口径、实现、语料统计

> ⚠️ **本文的数字已被 [`docs/142`](142-diff-converge.md) 取代。**
> 本文写在"服务端模拟步长还是 0.5"之前，当时（以及修步长后立刻重测）的基线是 **0–1/400**。
> 后续在 `142` 里做了四处有真值的修正（采样模式跟 `FunctionModeInfo`、修正文本按模式、
> 爆炸点≠停点、死效类型由皮肤定），**完全一致率已到 71/400 (17.8%)**。
> 本文的价值是**口径与工具**（`src/replaydiff.rs` 的比对方法），不是最新跑分。

状态：**现行**。本轮补的是 `docs/06-verification.md` §17.3 第 3 步——
**"预测我方本该发出的下一条消息"并与回放里的下一条逐条比**。
§17.3 当时写着"还没有这一环，是验收链上最大的一块空白"；本文把它补上，并把
"比什么/不比什么/为什么"钉成可判定的口径。

证据分级沿用仓库约定：**逐位验证**（`cargo test` 覆盖）/ **观测**（能指到地址、字节或运行时值）/
**推断**（写明缺什么）/ **未知**。

---

## 0. 一句话

> 380+ 份真回放现在能**逐条对拍**了：把它们喂进 `game::Match::fire`，比"我方产出的报文"
> 与"回放里的报文"。当前**按发货状态（`fire` 用 step=0.5、不支持 DiffEq）只有 18/1079
> 个结算批逐字节相同**；但几何探针显示，**换回真服务端的 0.01 步长后，739 发里 605 发的
> `end_x` 逐位相同**（step=0.5 时只有 27 发）。⇒ 最大的一处缺口不是物理，是
> `src/game.rs:516` 那个写死的 `0.5`。

---

## 1. 口径：比什么、不比什么、为什么

### 1.1 三条轴（`docs/06` §17.1）

| 轴 | 本文怎么判 |
|---|---|
| **状态** | 死亡集合（`LifeInfo{alive:false}`）与挖洞点（`DestroyObstacle`）按**集合**比，不比时刻 |
| **消息字节** | 每条报文按 `{"Kind":{...}}` 的**原始文本**逐字节比；字段顺序、数值的写法都要一致 |
| **几何** | `FunctionPoints.end_x`、`EffectInfo(Explosion).pos_*` 逐位比，并另按 1e-9/1e-3/5e-2 分档 |

### 1.2 不比什么（每条都有理由，不是为了让结果好看）

| 不比 | 为什么 | 依据 |
|---|---|---|
| **`tick` / `active_tick` / `start_tick` / `end_of_turn_tick` / `current_tick` 的**值**** | B 类：墙钟。钉住两份熵之后仍然只差这些字段 | `docs/06` §17.2、`docs/113` §5.2（**观测**：`end_of_turn_tick` 3688 vs 3687） |
| **`effect_id` 的绝对值** | 会话级自增计数器：抓包里从 236 起，回放里从 5xx 起，回放不给初值映射。改成只比"同批内严格递增"这个结构不变量 | 本轮**观测**：1079/1079 批双方都严格递增 |
| **我方根本不产的消息** | 那是"缺口"，不是"分歧"。排除但**逐类计数**（§4 的表），绝不静默 | 见 §4.3 |
| **我方产、回放永不记录的消息** | 回放的**记录域**比协议窄：`LifeQueuedDeath`、`QueueDestroyObstacle` 在 400 份里出现 **0 次**，但抓包里都有 | **观测**（`docs/113` §5.2 抓到过 `LifeQueuedDeath`） |
| **批内同类消息的先后** | A 类：HashMap 迭代序（`docs/113`）。口径改为**块内多重集**相等即"内容命中"，顺序另记一个指标 | `docs/113` §1.1、§7.1 |
| **同一 tick 里连续完全相同的重复报文** | 回放记录器会重复记（`LifeInfo` 两次、`DestroyObstacle` 两次）。折叠并计数 | **观测**：全语料 **1774** 条相邻完全重复 |
| **`EffectInfo` 的 `AshEffect` / `SparkEffect`** | 我方没有这两类特效的发射器（Ash 363 条、Spark 54 条） | §4.3 |

### 1.3 为什么口径必须先排除，而不是"先全绿再说"

`docs/06` §17.2 已经按"**结果是否由输入决定**"分了两类。对拍口径如果把这些混进来，
指标就永远到不了 100%，于是"没绿"不再有信息量——**一个永远失败的检查等于没有检查**。
所以本文把每个排除项都写成**可复核的断言**（字段名、消息种类、现象），
并且**排除项本身也有计数**（§4.3、§4.5），一旦某条假设不再成立会立刻在数字里露出来。

### 1.4 顺序：两档指标，不合并

- **内容命中（多重集）**：把一批里的报文投影后，按"连续同 kind 的块"分组，
  块内**按多重集**比 → 顺序无关。对应 `docs/113` 的 A 类退化口径。
- **顺序命中（含批内顺序）**：块内**有序**序列也相同。

两个数都报（§4.1）。**内容命中而顺序不命中**是合法状态（A 类），单独计数。
但**跨批**的顺序变化不算 A 类：那是输入与几何对不上，必须失败
（`tests/replay_diff.rs::order_across_batches_is_a_real_failure` 就是这条的负样本）。

---

## 2. 实现结构

新模块 `src/replaydiff.rs`（**独立**，不碰 `game.rs` / `serverloop.rs` / `protocol.rs` /
`api.rs` / `client/`），一个命令行 `examples/replay_diff.rs`，一份验收 `tests/replay_diff.rs`。

| 层 | 内容 | 为什么这么写 |
|---|---|---|
| `replaydiff::gz` | 纯 std 的 **gzip/DEFLATE** 解码，校验 CRC32 + ISIZE，支持多成员 | 本 crate 零依赖（`Cargo.toml` 的 `[dependencies]` 是空的），回放是 gzip（`docs/78` §2）。引 `flate2` 就把 `cargo test --offline` 绑到 registry 上。**注**：`docs/133` 的回放播放器另有一份 `src/replay_inflate.rs`（同一件事，独立实现）。本轮为不互相依赖各留一份；**建议合并时收敛成一份**（本模块只用 `gz::gunzip` / `gz::is_gzip` 两个入口） |
| `replaydiff::wire` | **保原始文本**的 JSON 读取：每个值都留 `raw`，数字区分 `1.0` / `1` / `1e-7` | `docs/06` §17.4 点名过"`1.0` 写成 `1`"这类偏差，`parse` 成 f64 再比就抓不到 |
| `Replay` | 一份回放 → 有序 `Msg` 流 + 解压字节数 | 只读，不落盘 |
| `split_batches` | 切**结算批**：`GameStateInfo(CalculatingResults)` … `(ExecutingResults)` | 实测同时回合制一次结算会发 **N 个 `FunctionInfo` + N 个 `FunctionPoints`**，全在同一个窗口里、**按消息类型分组**（§4.4）。以"一发"为单位切会切错 |
| `Inputs` | 从回放攒**输入态**：模式四元组、障碍、`PosInfo`、`TeamInfo`、`LifeInfo` | 位置/队伍/函数原文是"已知量"（`docs/06` §17.2：顺序按回放给定的消费），不是预测目标 |
| `Predictor` | 一个回放一个 `game::Match`，按 `FunctionPoints` 顺序逐发 `fire()` | 用**被测实现本身**产报文，不是另写一份拼装器 |
| `compare_batch` | 投影 → 折叠 → 分块 → 多重集 → 字段级 diff | 见 §1.2–§1.4；差异分类见 `DiffClass` |
| `StateMask` | 用**回放声明**的障碍/洞建掩膜（抗锯齿环可选） | 几何探针要与状态解耦：只问"同一发同一步长，停点一不一样" |
| `run_file` / `run_dir` | 逐文件 → 逐批 → 第一分歧 + 全量 diff 计数 | 输出的是"第一处"，不是"不一致" |

### 2.1 一次开火的 "预测 vs 观测" 长什么样

```
回放（观测）: FunctionFire | Calc | FunctionInfo(修正) | FunctionPoints | Sync
              | [EffectInfo(Death)]* | FunctionActive | EffectInfo(Explosion) | Executing
                         ↕ 逐条比（投影+折叠+分块+多重集+字段）
我方（预测）: Calc | FunctionInfo(修正) | FunctionPoints | Sync
              | [LifeQueuedDeath + EffectInfo(Death)]* | [QueueDestroyObstacle]
              | FunctionActive | EffectInfo(Explosion) | Executing
```

`LifeQueuedDeath` / `QueueDestroyObstacle` 是回放不记录的（§1.2），投影时丢掉并计数。

### 2.2 跑法

```bash
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=/home/user/MyProject/gw2/.agent-work/repldiff/target

# 全语料（只读；约 30 s）
cargo run --release --offline --example replay_diff -- --show 8

# 单份精细追踪（逐批：回放种类序列 / 我方种类序列 / 差异 / 死亡 / 挖洞 / 0.01 探针）
cargo run --release --offline --example replay_diff -- --trace 1782641048

cargo test --offline            # 296 → 308 条
```

---

## 3. 语料

`~/.local/share/godot/app_userdata/Graphwar/replays/`（**只读**，不入库）。

| 项 | 数 | 档 |
|---|---|---|
| 文件 | **400** | 观测 |
| 解压失败 / CRC 不符 | **0**（CRC32 + ISIZE 逐份校验） | 逐位验证（`gz::gunzip`） |
| 空回放（20 字节、解压 0 字节） | 2 | 观测 |
| 一份都没有"有开火的结算批" | 119（只到 Setup / 中途退出） | 观测 |
| 报文 | **103 104** | 观测 |
| 解压后体积 | 10.11 MB | 观测 |
| 结算批 | 1087（**1079** 有开火） | 观测 |
| 开火 | **1289** 发 | 观测 |
| 单发批 / 多发批 | 950 / **130**（2~8 发） | 观测 |
| 回合模式（按批） | `NormalFunction` 564 / `DiffEqFunction` 515 | 观测 |
| 时间模式 | `TimerInf` 905 / `Timer1m` 327 次 `EndOfTurnInfo` | 观测 |

回放消息词汇表 **32 类**（完整表见 `--show` 输出）。与 `docs/96` 的 73 个协议事件比，
回放只记其中一小半——它是**客户端侧记录域**，不是协议全集（**观测**）。

---

## 4. 结果：跑全部 400 份

### 4.1 消息级（**按发货状态**：`Match::fire`，step=0.5、只走 Normal）

| 指标 | 数 | 占比 |
|---|---|---|
| 结算批**内容**命中（多重集） | **18 / 1079** | 1.7% |
| 结算批**含批内顺序**命中 | 18 / 1079 | 1.7% |
| **消息种类序列**命中 | **815 / 1079** | **75.5%** |
| `effect_id` 结构不变量（双方严格递增） | 1079 / 1079 | 100% |
| 延迟段**死亡集合**命中 | 169 / 382 | 44.2%（多报 19 / **漏报 192** / 报错人 2） |
| 延迟段**挖洞点**命中 | 0 / 245 | 0% |
| **完全一致的回放** | **0 / 400** | 0% |

⇒ 结论：**"逐位相同"现在可以被判定，但按发货状态判不过去。** 这不是口径放水的地方，
是 §5 的待办清单。

### 4.2 几何探针（把"消息不对"拆成"物理不对"还是"序列化/口径不对"）

同一发、同一起点、同一个掩膜，只改**弦长步长**：

| 模式 | 步长 | `end_x` 逐位相同 | ≤1e-9 | ≤1e-3 | ≤5e-2 | >5e-2 |
|---|---|---|---|---|---|---|
| Normal | **0.01**（真服务端） | **605** | 9 | 20 | 75 | 30 |
| Normal | 0.5（`fire` 现值） | 27 | 9 | 53 | 142 | 508 |
| DiffEq | 0.01 | 490 | 0 | 15 | 41 | 4 |
| DiffEq | 0.5 | 14 | 11 | 31 | 68 | 426 |

（共 739 发 Normal、550 发 DiffEq = 1289。）

再按"我方判定撞障碍 / 自由飞行"拆开（Normal，0.01）：

| 类别 | 逐位相同 | >5e-2 |
|---|---|---|
| 自由飞行（到边界） | 400 | 22 |
| 撞障碍 | 205 | 8 |

⇒ **物理（采样器 + 掩膜）在 0.01 下基本是对的**：739 发里 738 发落在 5e-2 以内（= 1 像素）。

**对照实验（反证）**：把光栅化的**抗锯齿环**关掉、只做硬填，Normal@0.01 的逐位相同数
从 605 掉到 **434**、>5e-2 从 30 涨到 **114**。⇒ 环是对的，不是它把停点顶偏的。
（探针自带这个开关，`StateMask::without_ring`。）

### 4.3 我方还不产的消息（缺口，逐类计数）

| 消息 | 条数 | 说明 |
|---|---|---|
| `EntityEffectInfo` | 6889 | 实体特效（Death/Spark/Ash） |
| `LifeInfo` | 6584 | 只有 `alive:true` 在 `serverloop` 里手拼过，**死亡状态更新没有发射器** |
| `Skin/Face/HatGraphicsInfo` | 4518 ×3 | 外观 |
| `SoldierInfo` / `PlayerInfo` / `TeamInfo` / `PosInfo` / `ColorInfo` / `NameInfo` | 4553 / 1490 / 6427 / 5318 / 1556 / 1183 | 只在握手/加入链里，**本轮当输入消费**，没有接回放的输入流 |
| `AddObstacle` / `ClearObstacles` | 4410 / 380 | 同上级别生成链 |
| `FunctionFire` | 1301 | 服务端不回声（抓包里它是客户端请求的回执） |
| `EntityRemoved` / `RemoveTeam` | 657 / 442 | 断线/移除 |
| `EffectInfo/AshEffect` / `SparkEffect` | 363 / 54 | 特效发射器缺失 |
| `TargetInfo` / `TurnLimitInfo` | 252 / 21 | 关卡规则 |

这些**不计入分歧**，但也不计入通过——它们就是 §6 的待办。

### 4.4 分歧集中在哪

全部字段级差异（不只第一处）：

| 消息 \| 字段 \| 成因 | 次数 |
|---|---|---|
| `FunctionPoints.end_x` 数值差 >1e-3 | **1005** |
| `EffectInfo.pos_x` 数值差 >1e-3 | 761 |
| `EffectInfo.pos_y` 数值差 >1e-3 | 693 |
| `FunctionInfo.corrected_function` 字符串 | **536** |
| `FunctionInfo.<缺失>` | 130 |
| `EffectInfo/DeathEffect.<块种类>`（我方多一块） | 106 |
| `FunctionPoints.<缺失>` | 90 |

每份回放的**第一处分歧**按成因：

| 成因 | 份数 |
|---|---|
| 批内消息种类序列（我方多/少一块） | 126 |
| 字符串/枚举值（`corrected_function`） | 83 |
| 数值差 >1e-3（`end_x`） | 66 |
| 数值末位差 | 6 |

按字段路径：`FunctionInfo.corrected_function` 83、`<批内第 4 块种类>` 75、
`FunctionPoints.end_x` 72、`<第 7 块>` 39、`<第 6 块>` 12。

### 4.5 一个典型的第一处分歧（真样本）

```
1782640520  批 #0 tick=3397 发数=1  [数值（末位差）]
   FunctionPoints.end_x
   我方 = 10.495432991770882
   回放 = 10.495832466868457
   （同一发的 EffectInfo.pos_y：我方 -14.801039938895798 / 回放 -14.992205073094762）
```

同一发、同一起点、同一条函数，`end_x` 差 4.0e-4、`pos_y` 差 0.19——因为这条曲线极陡
（`dy/dx ≈ 475`），x 上一个像素带内的差被放大成 y 上的 0.19。**几何探针**显示：
把步长换成 0.01，这一发的 `end_x` 就逐位相同。⇒ 这条分歧的成因是 §5 第 1 条，不是物理。

另一类（多为 `corrected_function`）：

```
1782760485  批 #0
   FunctionInfo.corrected_function
   我方 = "…sin(cos(2588 + NaN"      ← 我们给 NaN 也加了后缀
   回放 = "…sin(cos(2588"             ← DiffEq 模式下**根本不加后缀**
```

---

## 5. 本轮对拍**发现**的实现问题（每条都可复核）

| # | 发现 | 档 | 证据 |
|---|---|---|---|
| 1 | **`game::Match::fire` 用 step = 0.5**，而真服务端是 0.01（`AGENTS.md` 硬事实、`docs/89`） | **观测**（代码 `src/game.rs:516`）+ 探针量化（§4.2） | 0.01 下 605/739 逐位相同，0.5 下 27/739 |
| 2 | **`corrected_function` 按模式不同**：Normal = 文本 + `" ± c.ddd"`（740/740，其中 9 条的函数部分要先重新序列化，见第 3 条）；**DiffEq = 函数原文、无后缀**（**550/550**，400 份语料零例外） | **观测** | 本轮全语料统计；我们的 `shot::corrected_text` 无条件加后缀 ⇒ DiffEq 的 550 发全错 |
| 3 | Normal 模式下有 **9/740** 的修正文本不是"原文 + 后缀"，而是把函数**重新序列化**后再加后缀（例：`5/155sin(4exp(4*6cos(` → `5/155sin(4exp(4*6cos())) + 6.055`，补括号） | **观测**（9 例逐条列出） | 与 `docs/119` 的 `to_text` 同构，**推断**：`corrected = to_text(parse(f)) + suffix` |
| 4 | **`protocol::emit_function_info` 不转义字符串**：玩家输入 `sqrt(\y)` 时产出**非法 JSON** | **观测**（1 条） | `json_str` 存在但没被调用；回放里存的是合法转义 `sqrt(\\y)` |
| 5 | **同时回合制一次结算 N 发**，且按**消息类型分组**：`FunctionInfo ×N` → `FunctionPoints ×N` → `Sync` → 特效 → `FunctionActive/Explosion` 交替 → `Executing`。`fire()` 是"一发一个完整窗口" | **观测**（130 个窗口 ≥2 发，最多 8 发） | 例：`1793883985` 的 8 发窗口，65 条报文 |
| 6 | 回放记录器会**重复记录**同一 tick 的相同报文 | **观测**（1774 条） | `LifeInfo`×2、`DestroyObstacle`×2 相邻出现 |
| 7 | `LifeInfo{alive:false}` **比开火晚**（落在后续回合），死亡特效 `EffectInfo(DeathEffect)` 反而在窗口内 | **观测** | `docs/113` 也记录过同类延迟；归属规则见 `run_file` 注释 |
| 8 | **爆炸点 ≠ 停点**：`EffectInfo(Explosion).pos_x` 与 `FunctionPoints.end_x` 相同只有 1050/1289（81%），不同时中位差 3.9e-5、最大 9.4e-3；而我们的 `pos_y` 与回放的 `pos_y` 在 Normal@0.01 下只有 33/739 逐位相同，其余 668 发全部落在 **1e-3 ~ 5e-2**（= 1 像素）之间 | **观测**；**机制未知**（缺 `EffectInfo` 发射路径的复核） | 两个数不可能来自同一个点 ⇒ 真游戏有两条计算路径 |
| 9 | 抗锯齿环是对的（关掉更差） | **观测**（反证实验） | §4.2 |

> 第 2 条顺带解释了 `docs/96` §3.1 里"`corrected_function` 带不带后缀"的旧困惑：
> **不是随机，是模式**。Normal 740 / DiffEq 550，两个集合零交叠。

---

## 6. 要让它真全绿，还缺什么（按投入产出排序）

1. **`fire()` 的步长**（一行）：`0.5` → `crate::SERVER_STEP`。§4.2 显示这是最大的一块
   （`end_x`/`pos_x`/`pos_y` 的 2459 次差异里绝大多数由它解释）。**注意这会改变
   `docs/86` 里"用 0.5 出的死亡集合"那批历史数字的可比性。**
2. **DiffEq 模式**（`Match::fire` 硬编码 `Mode::Normal`，且修正文本规则不同）：515 个批、
   550 发。要动的地方：`resolve(..., mode)` 的入参、`corrected_text` 的模式分支。
3. **同时回合制的事务化**：把"一批 N 发"做成一个 turn 级事务，报文按类型分组
   （`docs/86` §8 的"事件生成"那一层）。130 个批、其中 8 发批 6 个。
4. **`corrected_text` 改用 `to_text(parse(f))`**（9/740）+ **`emit_function_info` 走 `json_str`**
   （1 条非法 JSON）。
5. **爆炸点/停点两条路径**（§5 第 8 条）：需要回二进制复核 `FunctionPoints.end_x` 与
   `EffectInfo.pos` 各自的来源；这一条不解决，消息级"逐位"永远差最后一格。
6. **补齐未产出的消息类型**（§4.3），优先级：`LifeInfo`（死亡状态）→`EntityEffectInfo`
   →`FunctionFire`→外观/实体类。
7. **开局"全量重发"批（13 类，约占 45% 报文）**：本轮没覆盖——它要求把回放的**加入/开局
   输入流**接进 `serverloop`（现在只接了结算）。口径已经就绪：块内多重集 + 内容逐字节
   （`docs/113` 的 A 类只影响顺序）。
8. **死亡集合**（169/382）：主要成因是"漏报 192"——我方弹道过早停在环像素上
   （可复现例：`1782641048`，δ=0.0175 的环像素挡住了我们，真游戏穿过去了）。
   与第 5 条同源，需要一起复核。

---

## 7. 验收

```
cargo test --offline     # 本轮新增 12 条；当时 main 上 296 -> 308
                         # 变基到 main（含 docs/133/134/135）之后是 314 -> 326
```

`tests/replay_diff.rs` 覆盖：

| 测试 | 覆盖 |
|---|---|
| `complete_replay_runs_and_matches` | ① 一条完整回放跑通、逐批一致 |
| `single_field_difference_is_reported` | ② 人为改一处 `end_x` ⇒ 指名到字段 + 差异值对得上 |
| `numeric_format_difference_is_classified_separately` | `10` vs `10.0` 判"数值格式"，真值差不算格式 |
| `field_order_difference_is_reported` | 字段顺序差异 |
| `order_inside_batch_is_multiset_by_declared_policy` | ③ 批内乱序 ⇒ 内容命中、顺序不命中（A 类口径） |
| `order_across_batches_is_a_real_failure` | 跨批换序 ⇒ **必须失败**（口径不能变成"什么都放过"） |
| `declared_exclusions_are_really_excluded` | tick / `effect_id` / 不产 / 不记 四类排除确实生效 |
| `consecutive_duplicate_messages_are_collapsed` | 重复报文折叠 |
| `gunzip_matches_a_real_dynamic_huffman_stream` / `gunzip_rejects_corrupted_streams` | 自带解压器（动态 Huffman + CRC 拒收） |
| `plain_jsonl_and_gzip_both_load` | 两种容器 |
| `real_corpus_smoke_if_present` | 有真语料就跑最小 3 份（**没有则打印原因跳过**，不静默通过） |

合成回放由**我们自己的 `Match::fire`** 生成，所以"期望值"与"预测"在构造上一致，
任何差异只可能来自测试里那一处人为改动——这样 ② 才是真负样本。

---

## 8. 与既有文档的关系

- `docs/06` §17.3 的"第 3 步"由本文补上；§17.2 的三处退化口径原样沿用，未放宽。
- `docs/113` 的 A 类（批内顺序）落成了"块内多重集"这条可执行口径。
- `docs/86` §14 的"停点差 1e-6~4e-4"在 0.01 步长下复现（739 发里 738 发 ≤5e-2=1 像素，
  605 发逐位相同），**但它当时没提 `fire()` 用的是 0.5**——本轮把这件事显式化了。
- `docs/96` §3.1 关于 `corrected_function` 的疑问由 §5 第 2 条回答：按模式分，不是随机。
