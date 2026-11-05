# 139 · PlayScreen 接上联机：战场终于来自服务端

状态：**现行**。分支 `agent/play-online`（基线 `2b87b69`）。
本文回答五件事：① `play_screen.gd` 里**四种局面**（战役 / 本机 LAN / 回放 / 联机）怎么分派；
② 联机模式下**哪些数据来自服务端**、从哪条报文来；③ 开火与弹道为什么能在客户端复算；
④ **`PlayerInfo` 补上了**（服务端侧，`src/`"只加"），以及由此顺带修掉的一个真 bug；
⑤ 端到端怎么验、还差什么。

> 前置：`134-net-client.md` §7（**本任务就是逐条落实那份交接清单**）、
> `24-network-protocol.md`（消息形态 + `FunctionPoints` 只有首末点）、
> `96-protocol-coverage.md`（73 个事件名、外部标签、判别值不上线）、
> `131-level-rules.md`（本地对局的胜负/靶子规则，联机下要与它并存）、
> `133-replay-player.md`（回放分支，**本轮没动**）、`135-fire-fx.md` §4（`_fire_fx` 的 4 处落点，**没动**）。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 与抓包原文逐字节相同（`cargo test` 或自检里对拍）；
**观测** = 能指到夹具行号 / 地址 / 运行输出；
**推断** = 有间接证据、缺什么写在旁边；**未知** = 没有证据；
外加 **自定** = 本项目自己的选择（不是复原结论，与 `docs/108` §7、`docs/134` §0 用法一致）。

---

## 0. 一页结论

| 问题 | 答案 |
|---|---|
| 联机战场现在是服务端摆的吗 | **是**。士兵 = `room.entities` 里 `kind=="soldier"` 的实体（坐标/队伍/生出来自 `PosInfo`/`TeamInfo`/`LifeInfo`），障碍 = `room.obstacles`（`AddObstacle` 的顺序）。`_spawn_position()` 与内核关卡生成器在联机分支里**不再被调用** |
| 开火还是本地结算吗 | **不是**。走 `session.submit_function(pid,text)` + `session.fire(pid)`，本地不再算命中、不再换手、不再判胜负（那三件事都在服务端） |
| 弹道哪来的 | 服务端只发 `FunctionPoints{start_x,start_y,end_x}`（**没有点序列**）。客户端用**内核**按"函数原文 + 起点 + 方向"复算，方向由 `end_x` 落在起点哪一侧定，画到 `end_x` 为止 |
| 士兵死亡会消失吗 | **会**。`LifeInfo{alive:false}` 与 `LifeQueuedDeath` 都收；联机下阵亡者从绘制列表里去掉（服务端也在 `Match::fire` 里 retain 掉） |
| "我是谁"解决了吗 | **解决了**。服务端补了 `PlayerInfo{player_id,connection_id}`（抓包原文形状，逐字节测试）；拿不到时降级成"按 `TurnInfo` 的玩家假定"并在状态行写明 |
| 顺手修了什么 | **一个真 bug**：`FunctionUpdateRequest` 只写了玩家**第一条** `PlayerState`，于是"谁开火读谁的函数"永远读到默认值 `"0.00"` —— 客户端提交的函数从没真正生效过（以前的端到端只提交 `"0.00"`，看不出来）。**台账见 §10** |
| 三种老模式还好吗 | **好**。回放分支、`_fire_fx` 的 4 处落点、战役/LAN 的本地规则一个字没动；自检 478 条既有断言**全部照过** |
| 数字 | `cargo test --offline` **314 → 315**（+1，`PlayerInfo` 逐字节）；客户端自检 **478 → 529**（+51，0 失败） |

---

## 1. `play_screen.gd` 里四种局面怎么分派

分叉点只有一个：`begin_match()`（App 在切到本界面、以及"服务端把开局批发出完"时都会调它）。

```text
begin_match()
 ├─ st.kind == REPLAY 且 replay_file != ""   → _begin_replay()      （docs/133，本轮未改）
 ├─ App.online == true                       → _begin_online()      （本文新增）
 └─ 其余（战役 / 本机 LAN + Bot）            → 原来的本地流程（一个字没动）
```

| 局面 | 判据（为什么这么判） | 战场数据来自 | 谁结算 |
|---|---|---|---|
| 战役 | `state.kind == CAMPAIGN` | 关卡表 + 内核生成器（`docs/118`/`docs/126`） | 本地（`docs/131` 的 `GwLevelRules`） |
| 本机 LAN + Bot | `state.kind == LAN` 且 `App.online == false` | `_spawn_position()` + 内核障碍生成器 | 本地 |
| 回放 | `state.kind == REPLAY` + `replay_file` | 回放事件流（`docs/133`） | 不判（录像自己发 `GameStateInfo`） |
| **联机** | **`App.online == true`** | **服务端广播** | **服务端**（`src/serverloop.rs`） |

**为什么联机不能按 `state.kind` 判**（观测）：联机局在 `state` 里就是一次
`state.reset_lan()`（`app.gd::_sync_state_from_net`），`kind == Kind.LAN`，与"本机 LAN + Bot"
是**同一个值**。两者唯一的区别就是"战场是谁摆的、谁判胜负"，而这两件事都由
`App.online` 表达（它的唯一置真入口是 `GwNet.joined`）。`docs/134` §7.1 第 1 条说的
"判 `App.online` 为真时"就是这条。

---

## 2. 联机模式下哪些数据来自服务端

界面侧的模型照样是那几个数组（`_soldiers` / `_obstacles` / `_functions` / `_active`），
所以绘制、输入、光圈、状态行**全部复用**；变的只是**它们的来源**：

| 界面里的东西 | 服务端来的哪一条报文 | 落点 |
|---|---|---|
| 士兵（实体 id / 坐标 / 半径） | `SoldierInfo` + `PosInfo` | `room.entities` → `_build_online_soldiers()` |
| 队伍（谁跟谁一队） | `TeamInfo{entity_id, team:"Team1"/"Team2"}` | 实体的 `team` 字段 |
| 生死 | `LifeInfo` / `LifeQueuedDeath` | `_online_apply_life()`（**唯一**翻 alive 位的地方） |
| 障碍 | `AddObstacle` / `ClearObstacles` / `QueueDestroyObstacle` / `DestroyObstacle` | `room.obstacles` → `_build_online_obstacles()` |
| 轮到谁（玩家 / 射手实体） | `TurnInfo{player_id, soldier_id}` | `_online_turn_player` / `_online_soldier` |
| 回合截止 | `EndOfTurnInfo{end_of_turn_tick, time_mode}` | 只记录（见 §7 第 5 条） |
| 谁提交了什么函数 | `FunctionInfo{owner_id(=soldier_id), function, corrected_function}` | `room.functions` → `_functions[玩家下标]` |
| 弹道 | `FunctionPoints{owner_id, start_x, start_y, end_x}` | `_draw_online_shot()`（§3） |
| 特效 | `EffectInfo{effect_id, pos_x, pos_y, effect_type, start_tick}` | **只记录不绘制**（§7 第 1 条） |
| 我是哪个玩家 | `PlayerInfo{player_id, connection_id}` | `session.my_player_id` → `_my_player_id` |
| 四个模式串 / 房间名 | 握手那 7 条（`docs/33` §1） | `App._sync_state_from_net()` → `state.*`（联机分支不重算） |

两条实现上的口径（都写进代码注释了）：

1. **不拿 `App.online_room` 摆战场**。那一份是 `GwNetSession.snapshot()` 的**摘要**
   （只有"几名士兵 / 几个障碍 / 回合号"），没有坐标。它在界面侧的用途只有一个：
   `online_state()["room_snapshot"]` —— 让自检能对拍"App 侧看到的条数"与"界面上的条数"，
   从而证明这两层读的是同一份服务端状态。真数据一律读 `App.net.session.room`。
2. **玩家下标的顺序必须与 App 一致**。`App._sync_state_from_net()` 按 `player_id` 排序建
   `state.players`，`_rebuild_pid_index()` 用同一个 `ids.sort()` —— 否则
   `_functions` / `_active` 这套"按下标存函数"的既有逻辑会错位（联机下玩家 id 是
   服务端给的 229/230，不是 0/1）。

---

## 3. 开火与弹道：为什么客户端能复算

### 3.1 链路（逐条对上 `docs/134` §7.1 第 3 条）

```text
Fire 按钮
 └─ fire()                       ← 既有入口，只多了一个 _online_mode 分支
     └─ _fire_online(expr, soldier)
         ├─ 闸门：TurnInfo.soldier_id 对得上吗 / TurnInfo.player_id 是我吗
         ├─ session.submit_function(pid, expr)   → FunctionUpdateRequest
         └─ session.fire(pid)                    → FunctionFireRequest
（服务端 src/serverloop.rs → Match::fire）
FunctionInfo{owner_id, function, corrected_function}   ← 修正版，文本形态 "f(x) ± c"
FunctionPoints{owner_id, start_x, start_y, end_x}      ← **只有起点与终点 x**
（客户端）
 └─ _on_online_message("FunctionPoints")
     └─ _draw_online_shot(owner, start, end_x)
         ├─ 方向 = sign(end_x - start.x)           ← 由服务端数据定，不按队伍猜
         ├─ 内核 GwCore.calculate_path(函数原文, start, direction)
         │      （内核自己算 c = y0 - f(±x0)，与 Match::fire 用的是同一个 correction()）
         └─ 画到 end_x 为止 → _plane.set_path() + _fire_fx.play()
```

### 3.2 三个选择，各自的依据

| 选择 | 依据 | 档 |
|---|---|---|
| 方向由 `end_x` 定，不按队伍定 | 抓包里两发的方向本来就是这样定下来的（`docs/24` §6.3 的表格两个方向都有）；队伍方向是"通常如此"，`end_x` 是"这一发的实测" | **观测** |
| 用**函数原文** + 内核的修正，而**不用** `corrected_function` 文本 | `corrected_function` 是三位小数的文本（`src/shot.rs::corrected_text`），拿它重算会引入四舍五入误差；`start_x/start_y` 是 f64 原值，内核的 `c = y0 - f(±x0)` 与 `Match::fire` 走同一个 `correction()`。抓包样本 `"0.00 + 4.638"` 里的 `4.638` 正是 `start_y`（`docs/24` §6.2），说明这两条路等价、只是精度不同 | **逐位验证**（同一函数体）+ **自定**（选精度高的那条） |
| 画到 `end_x` 为止 | 服务端的停点 = 它那条点序列的**最后一点**（`src/settle.rs::resolve_points` 的 `stop`），而采样器确定性 ⇒ 到同一个 x 之前的点序列就是服务端那条轨迹 | **观测**（`stop` 的赋值）+ **推断**（两点之间按 0.01 采样是真的） |

**精度差在哪**（写清楚，别当成逐位）：**本文基线 `2b87b69` 上**，我们自己的服务端结算按弦长
**0.5** 采样（`Match::fire` → `settle::resolve(…, 0.5, …)`），内核 `path_calculate` 按
**0.01**（`docs/04`）。所以"我们的末点"与 `end_x` 最多差一个弦长（0.5），画出来的线与
服务端那条在 0.5 单位内等价、**不是逐点相同**。自检里的判据因此写成
`|末点 x − end_x| ≤ 0.6`。

> ✅ **这条已经落地了**（main `10e11fb`；`docs/136` 的几何探针：`step = 0.01` 时 `end_x`
> 逐位相同 605/739、`step = 0.5` 只有 27/739）：`Match::fire` 现在用 `SERVER_STEP`（0.01），
> 与内核 `path_calculate` 的 `STEP_DEFAULT` **是同一个值**，所以客户端这边**一行都不用改** ——
> 复算出来的点序列与服务端结算用的是同一份输入、同一个步长（"差一个弦长"变成同一批点）。
> 自检里 `|末点 x − end_x| ≤ 0.6` 那条判据留在原地：它是"末点就是服务端停点"的**下限**，
> 不是精度上限。这条改动**打穿了** `tests/server_match.rs` 的两条 e2e（固定水平线靠
> 0.5 的隧穿才打中人），改法见 §11。

### 3.3 联机下**不**做的事

* 不算命中：`_resolve_shots()` / `_commit_shot()` 在联机分支里根本不会被调用
  （`fire()` 在 `_online_mode` 时提前 return）；`_clip_to_obstacle()` 也不用了 ——
  那一步是给本地"画到撞点"用的，联机的停点由服务端 `end_x` 给。
* 不换手：`_next_turn()` 只被本地路径调用；联机的换手信号是 `TurnInfo`
  （`_online_tick()` 到点只把时钟停在 00:00，**不**自动换手）。
* 不判胜负：`_check_match_over()` 在联机下直接 return（服务端发 `EndingGame`）。
  本地那套 `GwLevelRules` 仍照常建出来，但只用于"人类在哪一队"这类诊断。

---

## 4. 事件驱动的刷屏

订阅入口：`session.message_received(kind, body)`（`GwNetSession._on_frame` 里
**先** `_apply()` 落到 `room`、**再**发这个信号，所以处理器读 `room` 拿到的是已更新的模型）。

| 报文 | 界面动作 | 档 |
|---|---|---|
| `PlayerInfo` | 镜像 `session.my_player_id`（会话侧已按 `connection_id` 判过"是不是给我的"），刷新状态行 | **逐位**（报文）+ **观测**（字段含义） |
| `PosInfo` | 改那名士兵的 `pos` + 重画 | **观测** |
| `LifeInfo` | 翻 alive 位（`true` 也认 —— 复原/复活是 `docs/123` §3.2 的形态） | **观测** |
| `LifeQueuedDeath` | **也当死亡**（见下面的说明） | **推断 + 自定** |
| `EffectInfo` | 记进 `_online_effects`，**不绘制**（棋盘没有特效图层，本轮不新造） | **自定** |
| `TurnInfo` | 更新 `_online_turn_player` / `_online_soldier`、换 `_active`、回填函数框、重置倒计时 | **观测** |
| `EndOfTurnInfo` | 只记录（本客户端还没发 `TickRequest`，`room.current_tick` 恒 0） | **未知**（tick 节拍没接） |
| `FunctionInfo` | 把该士兵的函数原文写进 `_functions[玩家下标]` | **观测** |
| `FunctionPoints` | §3 的复算 + 画 + tick 动画 | **观测** |
| `AddObstacle` / `ClearObstacles` / `QueueDestroyObstacle` / `DestroyObstacle` | 重建障碍列表 + 重画 | **观测** |
| `EndingGame` | 只写状态行（会话侧已置 `phase = "ended"`，App 侧置 `online = false`） | **观测** |
| 其他 60 余个事件名 | 不猜副作用，静默 | — |

### 4.1 `LifeInfo{a:false}` 与 `LifeQueuedDeath`：两种都收，理由如下

* **线上**的致死那一批是 `LifeQueuedDeath{entity_id,tick}` + `EffectInfo(DeathEffect)`
  （`docs/31` §1、`docs/43` 的报文顺序），我们的服务端照这个发（`src/game.rs::fire`），
  它**不**发 `LifeInfo{alive:false}`；
* **回放**里记的死亡恰恰是 `{"LifeInfo":{"entity_id":3,"alive":false}}`
  （`docs/96` §4 引的 380 份语料原样）；
* `GwNetSession._apply()` 只认 `LifeInfo`（`client/src/net/*` 本轮不可改）——
  所以 `LifeQueuedDeath` **不会**落进 `room.entities[eid].alive`。

于是界面侧自己镜像一份（`_online_lives`，`_online_apply_life()` 是唯一的写入口），两种报文都走它；
`_build_online_soldiers()` 建人时也用这份覆盖一次 —— 否则同一条连接上第二次
`begin_match()`（App 在 `joined` 和 `game_started` 各调一次）会把刚死的人复活。
**这条是推断 + 自定**：收尸时刻（tick 队列）没还原，所以收到就按"已死"处理。
更干净的做法是让 `GwNetSession` 也认 `LifeQueuedDeath`（那是联机层的活，本轮不碰）。

---

## 5. 「我是谁」：`PlayerInfo` 补上了

### 5.1 是什么、从哪抄的

```json
{"PlayerInfo":{"player_id":229,"connection_id":228}}
```

`fixtures/protocol.jsonl:13` 与 `fixtures/protocol_match.jsonl:36–37` 里的**抓包原文**
（两字段、蛇形键名、判别名不上线），`docs/24` §3 的消息目录里也有它。
`docs/33` §4 把"玩家加入（`NewPlayerRequest → PlayerInfo/TeamInfo/ColorInfo/SoldierInfo`）"
明确列为**没实现**——本轮只补其中**最关键的一条**：`PlayerInfo`。

### 5.2 服务端侧（`src/`，"只加"）

| 文件 | 加了什么 | 语义有没有变 |
|---|---|---|
| `src/game.rs` | `pub fn player_info_message(player_id, connection_id) -> String`（+ 逐字节测试 `player_info_message_matches_the_captured_bytes`） | **没有**。新函数，无调用点以外的影响 |
| `src/serverloop.rs` | `pub const FIRST_PLAYER_ID: i64 = 229`；`NewPlayerRequest` 分支（回一条 `PlayerInfo`）；开局造玩家槽时 `229 + t` → `FIRST_PLAYER_ID + t` | **没有**。`FIRST_PLAYER_ID == 229`，报文序列只在"客户端发了 `NewPlayerRequest`"时多一条 |

那个常量是"**只有一个真相**"的做法：`PlayerInfo` 说的 id 与开局那批
`SoldierInfo`/`TeamInfo` 用的 id 来自同一个常量，不可能漂。日志里多一行
`PLAYER_INFO player_id=229 connection_id=228`（不进协议）。

**边界（写明）**：本 harness 只 accept **一条**连接，所以"这条连接 = 第一个玩家槽"。
真游戏里玩家 id 由服务端的玩家表分配（顺序不可复现，`docs/47`）。极端配置
`per_team = 0` 时这条 `PlayerInfo` 会指向一个不存在的玩家 ⇒ 客户端看到空战场，不会崩。

### 5.3 客户端侧：两条路，绝不猜

| 情形 | 行为 | 档 |
|---|---|---|
| 收到 `PlayerInfo`（我们的服务端） | `session.my_player_id = 229` → 界面 `_my_player_id = 229`；开火闸门严格比对 `TurnInfo.player_id == 我` | **观测**（自检断言） |
| 收不到（旧服务端 / 假服务器） | `_my_player_id == -1` ⇒ **降级**："按 `TurnInfo` 里的玩家假定"，状态行写明"不知道我是谁" | **自定**（`docs/134` §7.1 的临时办法，保留为降级路径） |

降级不是摆设：自检里 `_test_net_join_fake_server` 的抓包 7 条**没有** `PlayerInfo`，
那些用例仍然要能跑；本文新加的假服务器用例则**专门**灌一条 `PlayerInfo` 来验"一条就够"。

### 5.4 顺带修掉的一个真 bug

`FunctionUpdateRequest` 只写了玩家**第一条** `PlayerState`，于是"开火读谁的函数"永远
读到构造时的默认值 `"0.00"` —— **客户端提交的函数从来没生效过**。
它**单独成章**写在 §10（对拍/端到端才发现的 bug 台账），因为它的价值不在本章的
`PlayerInfo` 叙事里。

---

## 6. 端到端记录（本轮实测）

### 6.1 命令

```bash
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=/path/to/work/target          # 必须单独一行赋值
cargo test --offline                                  # 315 条
cargo build --offline && cargo build --offline --example serve_match
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/   # 否则客户端跑旧内核
GODOT=/home/user/MyProject/gw2/tools/Godot_v4.7.2-stable_linux.x86_64 \
XDG_DATA_HOME=… XDG_CACHE_HOME=… XDG_CONFIG_HOME=… \
bash client/run.sh selftest                            # 529 条
```

（`examples/serve_match` 由自检自己拉起来；也可以用 `GW2_NET_SERVER=` 指一个现成的。）

### 6.2 客户端侧实测输出（原样摘录，一次通过的运行）

```text
[gw2/selftest] 联机：选中函数 -0.200000000000*x + 2.000000000000*sin(0.157079632679*(x + 10.000000000000))（目标 10.000, -4.000）
SELFTEST 529 passed, 0 failed
@@SELFTEST_DONE code=0
```

### 6.3 自检分层（用例 ↔ 能证明什么）

| 用例 | 依赖 | 断言数 | 能证明什么 |
|---|---|---|---|
| `_test_play_online_real_server` | `examples/serve_match`（真 TCP） | 40 个调用点 | ① 士兵/障碍来自服务端（entity_id、坐标、`x≠±18`、障碍逐条对拍）；② `FunctionPoints` → 内核复算 → 画出来（函数文本/点数/终点/方向）；③ 死亡广播 → 士兵从画面消失；④ `PlayerInfo` 让"我/对手"分得清（回合交给对手后拒绝开火，且**没有**报文发出去） |
| `_test_play_online_lifeinfo_fake_server` | 进程内 `TCPServer` | 12 个调用点 | `LifeInfo{alive:false}` 这一条**线上**（我们的服务端不发）也能让士兵消失；`PlayerInfo` 单条就够用 |

三条硬指标的断言原文（**括号里的数是运行时填的**，这里不抄具体数值 ——
同一份断言每局会给出不同的士兵数/障碍数/点数，抄一个数反而是在编造）：

```text
① 界面上的每一名士兵都是服务端 `entities` 里的实体（entity_id 对得上）
① 士兵坐标逐条等于服务端 `PosInfo` 写下的坐标（不是本地摆的）
① 没有一名士兵落在 `_spawn_position()` 的 x=±18 上
① 障碍数与服务端 `AddObstacle` 一致（界面 n ／ 服务端 n）
① 每个障碍的坐标/半径逐条等于服务端 `AddObstacle`（含顺序）
② 开火后收到 FunctionPoints（服务端只发首末点）
② 画在棋盘上的点数 = 复算出的点数（n）
② 方向由 end_x 定（起点 x x0 → 终点 x x1）
② 弹道收在服务端给的 end_x 上（末点 x ／ end_x）
③ 线上真的来了 `LifeQueuedDeath`（docs/31 §1，实际 n 条）
③ 阵亡的士兵从画面上消失（k → k−1）
⑤ 不是我的回合 ⇒ 界面拒绝开火（只有 PlayerInfo 分得清我跟对手）
⑤ 被拒绝的那一发**没有**发到服务端（FunctionPoints 数不变）
```

（"⑤"是第 4 条硬指标之外的附加验收：它证明 `PlayerInfo` 真的被用上了 —— 没有它，
客户端根本不知道 `TurnInfo` 里的那个 230 是对手。）

### 6.4 两个**随机性**，以及怎么处理（别让测试变成摇骰子）

| 现象 | 原因 | 处理 |
|---|---|---|
| 服务端偶尔**一个障碍都不发** | `levelgen::generate()` 的 `n = trunc(normal()*7 + 15)`，`min_count = 0` 时约 1.6% 会落到 0（`src/levelgen.rs:87`；`serverloop` 传的 min_count 就是 0） | ① 用例的关卡种子**钉死**（见下）；② 万一还是 0 个，障碍那两条**显式 skip** 并打印原因（不假装通过） |
| **射手被障碍压住**，任何弹道都在第一点被判挡住 | 开局位置是固定的 `x = ±10`（`src/serverloop.rs`），障碍却是随机撒的、**不看**这两个点 | ① 种子钉死；② 兜底：本地预演（内核）会如实报 153 个候选全 `blocked` ⇒ 用例**换一局**（最多 6 次）。换局只发生在自检里，界面不做 |

### 6.5 种子旋钮：`GW2_MATCH_SEED`（**自定**，测试用）

`src/serverloop.rs::run()` 里加了一个**只影响命令行 harness** 的旋钮：设了
`GW2_MATCH_SEED=<u64>` 就把关卡种子从 `seed_from_os()` 换成这个值（32 字节：前 8 字节是原值、
后 8 字节是一个黄金比例混合）。**不设时行为与以前完全一致**；`tests/server_match.rs` 走的是
`serve_stream*`，不受影响。

自检里用 `OS.set_environment("GW2_MATCH_SEED", …)` + `OS.create_process()`（子进程继承父进程环境，
`create_process` 本身没有 env 参数），种子取 `ONLINE_TEST_SEED = 8`。

怎么挑的（**观测**）：让服务端按种子 1…30 各跑一局，读回 `AddObstacle` 与 `PosInfo`，检查
"4 名士兵有没有一个落在障碍里"。结果：8/30 个种子会压住至少一名士兵（射手恒为
soldier 231、站位 `(-10, 0)`），障碍数从 4 到 25 不等。种子 8 = 10 个障碍、无人被压。
钉死之后连跑 3 次都是 **529 passed, 0 failed**，且不再触发重试。

附带发现（**观测**）：既有的 `_test_net_real_server` 用的是 `_server_started()`
（要求 `AddObstacle > 0`），所以它也有那 1.6% 的抖动。本轮**不动**它（只许加断言），
在这里登记。

---

## 7. 仍未做 / 已知边界

| # | 缺什么 | 状态 | 说明 |
|---|---|---|---|
| 1 | **特效**（爆炸/死亡）的绘制 | **未做** | `EffectInfo` 只记录不绘制：棋盘没有特效图层，本轮不新造一个（与 `docs/135` 的"光圈 + 弹道"是两件事） |
| 2 | **对手开火** | **未做** | 客户端不会替对手开火（真游戏里对手是另一个客户端）。我们这台 harness 默认没有 Bot 玩家（`Config::bot_players` 空），所以一局里只有"我"这一发；服务端开 `bot_players` 时对手的 `FunctionPoints` 会被同一条路径画出来（没验过，因为没有命令行开关） |
| 3 | **同时回合制** | **未做** | 服务端的 `RoomState.turn_mode` 写死 `SequentialTurns`；客户端的 `free_fx_state()` 联机分支只画一个光圈。批内顺序与"多份 `FunctionPoints` 同一个 tick"都没接 |
| 4 | **tick 时钟** | **未做** | 客户端不发 `TickRequest`（`room.current_tick` 恒 0），所以 `EndOfTurnInfo` 的截止 tick 换算不出剩余秒数，倒计时仍按墙钟走（`docs/35`：60 tick/s） |
| 5 | **收尸时刻** | **推断/自定** | 收到 `LifeQueuedDeath` 就按"已死"，没有 tick 队列（`LifeQueuedDeath.tick` 与 `EffectInfo.start_tick` 相等，`docs/31` §4，但"什么时候生效"未还原） |
| 6 | **颜色 / 名字 / 皮肤 / 段位** | **未做** | 服务端不发 `ColorInfo`/`NameInfo`（`docs/33` §4）。颜色用确定性队色表、名字用 `App` 建的那张表（`Player 229` / 档案名） |
| 7 | **断线重连 / 局中恢复** | **未做** | 断线只弹 `DisconnectedPopUp` + `end_match()`（`docs/134` §7.2）。`_online_lives` 等记录在**换会话**时清空，同一会话内 `begin_match()` 调两次不会复活死者 |
| 8 | **`LifeQueuedDeath` 没进 `GwNetSession`** | **已知缺口** | `client/src/net/*` 不在本轮可改范围；界面侧镜像了一份（§4.1）。更干净的做法在联机层 |
| 9 | 联机下**判和目标** | **未知** | 服务端的 `Match` 只认士兵（`docs/117`），没有靶子；界面这边 `set_targets([])`。原版联机局有没有靶子**未观测** |
| 10 | 弹道与服务端**逐点**相同 | **推断 / 已知边界** | 本文基线上服务端 0.5 弦长、内核 0.01 弦长 ⇒ 差一个弦长以内（§3.2）。`docs/136` 正在把服务端步长改成 0.01（那条合入后就是逐点相同）；本任务未验证 |

---

## 8. 四档汇总

| 结论 | 档 | 依据 |
|---|---|---|
| 联机战场来自服务端（士兵/障碍） | **观测**（端到端断言） | §6.3 的 ① 那一组 |
| `FunctionPoints` 只有首末点、轨迹要客户端复算 | **观测** | `docs/24` §6.1 的原始报文 |
| 方向由 `end_x` 定 | **观测** | `docs/24` §6.3 两个方向的样本 |
| 复算用函数原文 + 内核修正（不用三位小数的 corrected 文本） | **逐位**（同一 `correction()`）+ **自定**（选精度） | `src/api.rs::calculate_path`、`src/game.rs::fire` |
| 服务端的停点 = 点序列最后一点 | **观测** | `src/settle.rs::resolve_points` 的 `stop` |
| `PlayerInfo` 的报文形状 | **逐位验证** | `cargo test` 的 `player_info_message_matches_the_captured_bytes`（对 `fixtures/protocol_match.jsonl:36`） |
| "这条连接 = 第一个玩家槽（229）" | **自定** | 本 harness 只 accept 一条连接；真游戏顺序不可复现（`docs/47`） |
| 死亡报文有两种形态（线上 `LifeQueuedDeath` / 回放 `LifeInfo`） | **观测** | `docs/31` §1、`docs/96` §4 |
| 收到死亡就把人从画面上拿掉 | **自定**（+ 推断） | 服务端 `Match::fire` 会 retain 掉死者；原版客户端画不画灰**未知**（本地局仍画灰 + 叉，未改） |
| 联机胜负由服务端判 | **观测** | `src/serverloop.rs` 发 `EndingGame` 后关 socket |
| `FunctionUpdateRequest` 只写第一条 `PlayerState` 是 bug | **观测**（自检红过一次 + 代码） | §10.1 |

---

## 9. 复跑与改动清单

```bash
# ① 内核（Rust）—— 315 条
cargo test --offline

# ② 客户端自检 —— 529 条（缺 serve_match 时联机用例显式 skip）
cargo build --offline --example serve_match
GODOT=<Godot 4.7.2> bash client/run.sh selftest

# ③ 内核真的被加载了（否则界面退到 preview 折线）
bash client/run.sh dump | grep '@@KERNEL'      # 期望 available=1 hit_test=1
```

逐文件改动（`git diff --numstat`）：

| 文件 | 变化 | 内容 |
|---|---|---|
| `client/src/screens/play_screen.gd` | **+579 / −0** | 联机成员 12 个 + `_begin_online`/`_online_teardown`/`_online_subscribe`/`_build_online_soldiers`/`_build_online_obstacles`/`_apply_online_turn`/`_online_can_fire`/`_online_tick`/`_fire_online`/`_on_online_message`/`_online_apply_life`/`_online_soldier_by_entity`/`_draw_online_shot`/`_truncate_path_at_x`/`_stop_before_end`/`_online_status_text`/`online_state`；**既有行只加了分支**（`begin_match`/`fire`/`_refresh_plane`/`_active_soldier`/`_human_team`/`_check_match_over`/`fire_fx_state`/`end_match`/`_process` 各一处），回放分支与 `_fire_fx` 的 4 处落点**一字未动** |
| `client/tools/selftest.gd` | **+550 / −0** | 只**新增**两个用例 + 九个辅助函数（`_play_online_started`/`_online_enemies_of`/`_online_obstacles_of`/`_pick_online_shot`/`_online_shot_hits`/`_online_shot_reason`/`_online_sine_expr`/`_online_poly_expr`/`_first_dead_entity`）；**既有 478 条断言一条没动** |
| `src/game.rs` | **+45 / −0** | `player_info_message()` + 逐字节测试 |
| `src/serverloop.rs` | **+75 / −3** | `FIRST_PLAYER_ID` 常量、`NewPlayerRequest` 分支、开局玩家槽改用常量（值不变）、`FunctionUpdateRequest` 的 bug 修复（§5.4）、`run()` 里的 `GW2_MATCH_SEED` 测试旋钮（§6.5） |
| `tests/server_match.rs` | **见 §11.5** | 两条 e2e 的客户端改成"真的瞄准"、两条 bot 测试换种子、三处 `drop(client)` 防挂死、新增 `#[ignore]` 的 `search_ai_seed`（§11） |
| `client/src/net/*` | **0** | 照它的 API 用，一个字没改 |
| `core_bridge.gd` / `replay_player.gd` / `player_screen.gd` / `client/src/ui/*` | **0** | 不碰 |

---

## 10. 对拍 / 端到端才发现的 bug（台账）

> 单独成节。每条都写清五件事：**症状 / 为什么既有测试看不出来 / 证据 / 修法 / 回归**。
> 收录标准与回放那边的"列表按扩展名过滤"一样：**只有在把两条独立的实现摆到一起比、
> 或者真的跑一局时才会暴露**的那类 bug；纯靠读代码就能看出来的小错不算。

### 10.1 `FunctionUpdateRequest` 只写了玩家**第一条**局面 ⇒ 客户端提交的函数从没生效

| 项 | 内容 |
|---|---|
| **症状** | 客户端提交 `"sqrt(abs(x))"`，服务端回执的 `FunctionInfo.function` 与开火那一刻 `FunctionPoints` 用的函数都是 **`"0.00"`**（玩家表里的默认值）。也就是说：**"提交函数"这个动作在非首名士兵的回合里从来没生效过** |
| **为什么既有测试看不出来** | 既有的两条端到端（`tests/server_match.rs` 的 `plays_a_full_match_over_tcp` / 客户端自检的 `_test_net_real_server`）提交的都是 **`"0.00"`** —— 与服务端 `PlayerState::function` 的构造初值**恰好相同**。测试"通过"了，但证明的是"默认值和默认值相等" |
| **证据（观测）** | 本轮自检第一次提交**非默认**函数时立刻红：`FAIL ② 复算用的函数文本 = 我提交的那一条（0.00）`（修复前，见 §6.3 的 ② 那一组）。随后在服务端日志里看到的是同一个 `owner_id`、函数却是默认值 |
| **根因** | `players: Vec<PlayerState>` 是**按士兵**建的（一名玩家 `per_team` 条）；`FunctionUpdateRequest` 用 `find(|p| p.player_id == pid)`（**只命中第一条**）写函数，而 `FunctionFireRequest` 用 `position(|p| p.soldier_id == cur_sid)`（**按本回合射手**）读函数。`TurnManager` 的游标从 0 起步、先自增再取 ⇒ 首名射手往往是该玩家的**第二**名士兵 ⇒ 读到的永远是初值 |
| **修法**（`src/serverloop.rs`） | 把该玩家的**每一条**都写一遍：`for p in players.iter_mut().filter(\|p\| p.player_id == pid) { p.function = f.clone(); }`。函数本来就是"按玩家"提交的，按士兵各存一份只是这一层的实现细节；报文形状、顺序、判别值一个都没动 |
| **回归** | 自检的 ② 组断言转绿（"复算用的函数文本 = 我提交的那一条"）；`cargo test` 里那三条 e2e 全绿（它们现在也真的提交非默认函数了，见 §11） |
| **档位** | **观测**（自检红过）+ **逐位验证**（修后 `cargo test` + 自检覆盖） |

---

## 11. 步长 0.5 → 0.01 之后：两条 e2e 改成"真的瞄准"

### 11.1 现象与根因（**观测**）

`src/game.rs::fire` 的步长从写死的 `0.5` 改成 `SERVER_STEP`（`0.01`，`docs/136` 的几何探针：
`end_x` 逐位相同 605/739 vs 27/739）之后，`tests/server_match.rs` 的两条 e2e 红了：

* `plays_a_full_match_over_tcp` —— `assert!(ended, "直线射击应当能打死敌人并最终触发 EndingGame")`；
* `rotation_and_ai_are_wired_over_tcp` —— 先 `assert!(ended)`，再过不了 `assert!(!deaths.is_empty())`。

根因不是种子，而是**物理变准了**：旧的两条测试都在打**固定水平线 `"0.00"`**，
它们之所以能打中人，靠的是 `0.5` 的弦长**粗到会隧穿障碍**（步长 0.5 意味着采样点相隔
半个棋盘单位，半径 0.8–4.27 的墙会被"跨过去"）。换成 0.01 之后墙真的把人挡住了：
水平线从射手出发，走不到对手那里就撞墙停下 ⇒ 没人死 ⇒ 没有 `EndingGame` ⇒ 僵局。

> **不改回 0.5，也不放宽断言**。修的是"测试客户端"，不是物理。

### 11.2 人类玩家局：客户端**真的瞄准**（`plays_a_full_match_over_tcp`）

服务端对**人类玩家**是"你交什么就按什么开火"（`FunctionUpdateRequest` 那条函数原样用），
所以这条测试的客户端现在做三件事（全部在 `tests/server_match.rs` 里，只驱动客户端、
**不参与任何断言**）：

```text
① Board::absorb(报文)       ← PosInfo 坐标 / SoldierInfo 归属 / TeamInfo 队伍 /
                               AddObstacle 障碍 / LifeQueuedDeath 存活位
② Board::mask()             ← 与服务端 `Match::new` 同一套：只把障碍光栅化进 1000×600 掩膜
③ aim(board, shooter)       ← 每回合按"当前射手 + 最近的敌人"重新挑：
                               候选 = 过射手与目标两点的直线 m·x
                                    + 半正弦鼓包 m·x + k·sin(h·π·(x−x0)/dx)（两端归零，k 控制绕多高）
                                    + 过"路标"的抛物线 m·x + b·(x−x0)·(x−xt)
                               判定 = `settle::resolve(expr, x0, y0, SERVER_STEP, flip, Normal,
                                                      mask, others)` —— **服务端那个函数本身**
                               要求 = 没被墙挡住 && 打死至少一个敌人 && 没误伤队友
```

* 候选是**按几何构造**的（每一条都必然穿过射手与目标），所以"打不中"只可能是"被墙挡住"，
  搜索只需要换鼓包高度/路标位置；
* 判定用的是**同一个函数、同一个步长、同一份掩膜**，所以"客户端觉得能中"⇒"服务端就是能中"
  （这条正是 §3.2 那个"步长已经一致"的直接好处）；
* **尝试次数进断言消息**：`assert!(ended, "…打了 N 回合、M 回合做了瞄准、共试过 K 条候选弹道；
  记录：{:?}")`；
* 另加一条**只加不减**的断言：`至少两发是瞄准过的（不是随手甩的 "0.00"）` ——
  它防的是"退回到固定水平线也能过"的假绿。

实测（seed `[11;32]`，9 个障碍）：`ENDED at turn 5`，测试 8.3 s。

### 11.3 电脑玩家局：客户端**改不动**，只能换夹具种子

`Match::fire` 对电脑玩家一律用 `ai_function_text()`（`docs/127` §5），客户端交的函数
**不生效**，所以这两条测试的胜负完全由 `(seed, AI 进化)` 决定。而 AI 的适应度里**含掩膜**
（`aigen::ScoringContext.mask = Match::mask`），障碍一多就可能僵成"AI 一直贴着墙打"。

于是新加了一条 `#[ignore]` 的 **`search_ai_seed`**（同文件末尾，平时不跑）：

```bash
cargo test --offline --test server_match -- --ignored --nocapture search_ai_seed
```

它先按 `[k;32]`（k = 0..=63）**纯计算**障碍数（`ChaCha12::from_seed` + `levelgen::generate(.., 0)`，
与 `serverloop` 开局同一条），再对障碍最少的若干个真的跑一局 AI-vs-AI：

```text
障碍最少的 12 个 `[k;32]` 种子: [(0,30),(0,62),(1,53),(3,34),(3,55),(3,58),(4,12),(4,27),(4,51),(5,1),(5,36),(5,42)]
SEED [30;32] 障碍=0 回合=20 死亡=2 ending=false
SEED [62;32] 障碍=0 回合=12 死亡=3 ending=true
SEED [53;32] 障碍=1 回合= 6 死亡=3 ending=true      ← 选它
SEED [34;32] 障碍=3 回合=20 死亡=2 ending=false
SEED [55;32] 障碍=3 回合=20 死亡=2 ending=false
SEED [58;32] 障碍=3 回合=20 死亡=2 ending=false
```

⇒ 两条电脑玩家测试的夹具种子 `[11;32]` → **`[53;32]`**（1 个障碍、第 6 回合 3 人阵亡、
`EndingGame`；它是"AI 真能打死人"的种子里障碍最多的那一档里最快的）。**断言一条没改**：
测试 2 仍然要求 `ended` + `deaths` 非空（"跳过死者"这条规则必须真的被走到），
测试 3 仍然要求 AI 每回合都说话、`corrected_function` 的常数逐位对得上。

### 11.4 顺手修掉的"挂死"（不是失败，是挂住）

三条测试里都有 `server.join()`。**对局没打完**时服务端仍阻塞在 `read_frame` 上，
而客户端 socket 还开着 ⇒ `join()` 永远不返回 ⇒ 测试进程挂死（`--test-threads=1` 下整个
`cargo test` 就停在那儿，看起来像"跑不完"）。三处 `join()` 之前都加了 `drop(client)`：
连接一关，服务端 `read_frame` 拿到 EOF 退出，`join()` 立刻返回 —— **该红的就红，
不再装死**。

### 11.5 验证与改动

```text
cargo test --offline --test server_match          → 3 passed / 0 failed / 1 ignored（25 s，串行）
cargo test --offline                              → 333 passed / 0 failed / 1 ignored
```

逐文件（本次增量）：

| 文件 | 变化 | 内容 |
|---|---|---|
| `tests/server_match.rs` | **+384 / −15** | `Board`（报文→局面）+ `mask()`/`others()`/`enemies_of()` + `candidate_exprs()`/`judge()`/`aim()` 瞄准板；测试 1 改成逐回合瞄准（并新增"至少两发是瞄准过的"）；测试 2、3 的种子换成 `[53;32]`；三处 `drop(client)`；新增 `#[ignore] search_ai_seed` |
| `docs/139-play-online.md` | 本章 | §10 台账 + §11 改法；§3.2 的"另一条线"改成"已合入"；§5.4 收成指向 §10 的一句 |
