# 09 · 联机与传输层：帧 / 握手 / 服务端 / broker

> **本篇解决什么问题**：客户端怎么连、线上字节长什么样、服务端要按什么顺序回什么、
> broker 怎么把房间列出来——以及我们自己的实现与它的边界。
>
> **本篇合并自**（全文与逐条证据轨迹都在 `archive/`，这里只留现行事实与关键表）：

| 原文件 | 内容 | 归档 |
|---|---|---|
| `24-network-protocol.md` | 传输层、帧格式、29 种消息的真实样本 | [`archive/24-network-protocol.md`](archive/24-network-protocol.md) |
| `32-netcode-implementation.md` | `src/_scaffold/protocol.rs` 的帧编解码与事件实现 | [`archive/32-netcode-implementation.md`](archive/32-netcode-implementation.md) |
| `33-server-handshake.md` | 新连接的状态同步 7 条 | [`archive/33-server-handshake.md`](archive/33-server-handshake.md) |
| `36-match-server.md` | `Match` 对象组成 | [`archive/36-match-server.md`](archive/36-match-server.md) |
| `62-server-loop.md` | 完整一局的服务端循环 | [`archive/62-server-loop.md`](archive/62-server-loop.md) |
| `115-broker.md` | broker（房间列表服务器）协议 + 我们的实现 | [`archive/115-broker.md`](archive/115-broker.md) |

证据标记沿用全仓库四档：**逐位验证 / 观测 / 推断 / 未知**（定义见 [`06-verification.md`](06-verification.md) §1）。
**事件枚举本身**（判别值 / 名字 / 字段清单）不在本篇，在 [`07-protocol.md`](07-protocol.md)。

## 1. 传输层、帧与真实报文样本

### 1.1 传输层（观测）

| 用途 | 传输 | 编码 | 端口 |
|---|---|---|---|
| LAN 房间发现 | UDP 广播 | 明文 JSON，**无长度前缀** | 抓到发往 `55463` |
| 对局通信 | TCP | **`u16` 大端长度 + JSON** | 服务器地址形如 `[::1]:35245` |

长度字段**只算负载**：实测帧头 `0x0031` = 49，后面正好 49 字节 JSON。

#### 1.1.1 LAN 广播样本（观测）

```json
{"room_name":"Graphwar Game","game_state":"Setup","current_player_count":2,
 "address":"[::1]:35245","axis_mode":"EveryUnit","function_mode":"NormalFunction",
 "turn_mode":"SequentialTurns","time_mode":"Timers1m","locked":false}
```

`time_mode` 实测取值只有 `Timer1m` / `Timer3m`（两次抓包共 385 次出现，
`Timers1m` 出现 **0** 次）。

> **更正**：本文件早先写过"广播拼 `Timers1m`、TCP 拼 `Timer1m`，两处不一致"。
> 那是我凭印象写下的**错误结论**，实测数据否定了它。教训：文档里的每一个字面量
> 都必须来自抓包数据的直接引用，不能凭记忆补全。

### 1.2 消息格式

全部是单键对象：`{"TypeName":{字段...}}`。字段只观测到字符串、整数、
以及嵌套的小对象（`color` 的 `{r,g,b}`）。**没有观测到数组**。

### 1.3 消息目录（29 种，全部有真实样本）

#### 1.3.1 握手
| 消息 | 样本 |
|---|---|
| `GameNameRequest` | `{"game_name":"Graphwar Game"}` |
| `NewConnectionRequest` | `{"major_version":"2.0","minor_version":"60"}` |
| `NewConnection` | `{"connection_id":228}` |
| `TickRequest` | `{}` |
| `TickReply` | `{"current_tick":5}` |

#### 1.3.2 房间设置
`GameStateInfo{game_state}`、`GameNameInfo{game_name}`、
`AxisModeInfo{axis_mode}`、`FunctionModeInfo{function_mode}`、
`TurnModeInfo{turn_mode}`、`TimeModeInfo{time_mode}`

取值实测：`axis_mode="EveryUnit"`、`function_mode="NormalFunction"`、
`turn_mode="SequentialTurns"`、`time_mode="Timer1m"`、`game_state="Setup"`。

#### 1.3.3 玩家与实体
`NewPlayerRequest{connection_id}`、`PlayerInfo{player_id,connection_id}`、
`TeamInfo{entity_id,team}`、`ColorInfo{entity_id,color{r,g,b}}`、
`SoldierInfo{soldier_id,player_id}`、`EntityEffectInfo{entity_id,effect_type}`

实测：`player_id=229`、`soldier_id=230`、`team="Team1"`、
`color={r:149,g:5,b:62}`、`effect_type="DeathEffect"`。

#### 1.3.4 外观/元数据（Request → Info 成对）
`NameRequest/NameInfo{entity_id,name}`、
`SkinGraphicsRequest/SkinGraphicsInfo{entity_id,graphics_str}`、
`FaceGraphicsRequest/FaceGraphicsInfo{entity_id,graphics_str}`、
`HatGraphicsRequest/HatGraphicsInfo{entity_id,graphics_str}`、
`RankRequest/RankInfo{entity_id,rank}`

实测取值：`name="user"`、`graphics_str="yellow"` / `"regular_eyes"` / `"soldier_helmet"`、`rank=0`。

#### 1.3.5 Bot
`NewBotRequest{connection_id,level}`、`BotAdded{connection_id,player_id,level}`（实测 `level=3`）

### 1.4 对局内消息（第二轮抓包）

`tools/capture_net_raw.js` 改抓**原始字节流**再按帧重组（TCP 会拆包，不能按调用边界当帧），
得到 166 帧、去重 109 条，存为 `fixtures/protocol_match.jsonl`。

#### 1.4.1 关卡障碍 `AddObstacle`

```json
{"AddObstacle":{"pos_x":-14.167697818619251,"pos_y":-8.472137403369004,"radius":3.901519740713089}}
```

同一局抓到 **12~14 个**障碍。半径里同时出现过 **0.8**（两次，像最小值）和 **5.27**（大值）；
坐标可超出棋盘（`pos_y=18.32` 超过 `|y|≤15`），说明障碍允许落在场外。

#### 1.4.2 实体位置 `PosInfo`

```json
{"PosInfo":{"entity_id":231,"pos_x":-21.604890565331004,"pos_y":5.894946389140589,"radius":0.5}}
```

士兵 `radius` **恒为 0.5**（4 条样本一致）。

#### 1.4.3 生死 `LifeInfo`

```json
{"LifeInfo":{"entity_id":231,"alive":true}}
```

#### 1.4.4 提交的函数 `FunctionInfo`

```json
{"FunctionInfo":{"owner_id":234,"function":"exp(sin(x))","corrected_function":null}}
{"FunctionInfo":{"owner_id":231,"function":"","corrected_function":null}}
```

⇒ **协议里直接带着玩家写的函数原文**。空串表示该实体尚未提交。
这是"回合语义"最直接的证据来源：谁在什么时候提交了什么。

#### 1.4.5 其他新抓到的
`VersionInfo`、`TimeModeChangeRequest`、`FunctionUpdateRequest`。


## 2. 我们实现的帧编解码层（`src/_scaffold/protocol.rs`）

### 2.1 已实现（`src/_scaffold/protocol.rs`）

| 能力 | API |
|---|---|
| 帧编解码（`u16` 大端 + 负载） | `Frame::encode` / `Frame::decode` |
| 解析任意消息 | `parse_message` + `get_str/get_int/get_f64/get_bool/get_color` |
| LAN 广播解析与**重新序列化** | `Broadcast::parse` / `Broadcast::to_json` |
| 常用消息序列化 | `emit_turn_info` / `emit_life_queued_death` / `emit_function_points` / `emit_effect_info` |


## 3. 握手：新连接的状态同步（7 条，逐条对拍）

### 3.1 抓包里的顺序

客户端发 `NewConnectionRequest` 之后，服务端**按固定顺序**回 7 条：

```text
NewConnection{connection_id}
TurnModeInfo{turn_mode}
GameStateInfo{game_state}
GameNameInfo{game_name}
AxisModeInfo{axis_mode}
FunctionModeInfo{function_mode}
TimeModeInfo{time_mode}
```

这一段没有随机成分，所以可以做到逐字节一致。

### 3.2 实现

`src/server.rs`：

```rust
pub struct RoomState { room_name, game_state, axis_mode, function_mode, turn_mode, time_mode }
pub fn on_new_connection(state: &RoomState, connection_id: i64) -> Vec<String>
pub fn to_wire(messages: &[String]) -> Vec<u8>   // 每帧 u16 大端长度 + JSON
```

`examples/serve_handshake.rs` 是一个最小 TCP 服务端：收到任意一帧后回上述 7 帧。

### 3.3 验证（两条）

#### 3.3.1 逐字节

`server::tests::state_sync_matches_captured_bytes`：把抓包夹具里的这 7 条
与我们的输出逐条比较，**全部相同**。

#### 3.3.2 真实 socket 往返

```bash
cargo run --quiet --example serve_handshake -- 127.0.0.1:39991
# 另开一个 Python 客户端发一帧
```

实测：

```
服务端: ACCEPTED 127.0.0.1:59000
        GOT {"NewConnectionRequest":{"major_version":"2.0","minor_version":"60"}}
        SENT 7 frames, 324 bytes
客户端: 收到帧数: 7
        {"NewConnection":{"connection_id":228}}
        {"TurnModeInfo":{"turn_mode":"SequentialTurns"}}
        ...
```

⇒ **我们的服务端能被真实 TCP 客户端连上并吐出格式正确的帧**。


## 4. `Match` 对象：把已验证的零件装成能开火的服务端

### 4.1 组成

```rust
pub struct Match {
    room: RoomState,          // 房间状态（server）
    obstacles: Vec<(f64,f64,f64)>,
    soldiers: Vec<Target>,    // 存活士兵（死者会被移除）
    tick_base: (i64, i64),    // tick 基准（tick 模块）
    mask: Vec<u8>,            // 障碍光栅化结果（plane）
    next_effect_id: i64,
}

pub fn fire(&mut self, owner_id, function_text, x0, y0, flip, now_ms) -> Vec<String>
```

`fire` 一次产出整串**可直接写进 socket 的报文**：

```text
FunctionPoints -> [LifeQueuedDeath -> EffectInfo(DeathEffect)]*
                -> [QueueDestroyObstacle] -> EffectInfo(ExplosionEffect)
```

tick 由 `graphwar_tick_handler::tick_at` 给出（60 Hz）。**死掉的士兵会从场上移除**，
否则后续射击会重复计死（这条是测试逼出来的）。

### 4.2 验证（真实关卡回放）

`tests/live_hits.rs::match_replays_the_death_level_through_the_protocol_layer`：
把死亡关卡的 6 次射击按顺序喂给 `Match`，然后

- 每发首条必须是 `FunctionPoints`，其 `end_x` 与抓到的终点相差 < 0.5；
- 每条产出都必须是**我们自己的解析器能解析的合法消息**；
- 复算出的致死对象必须落在抓包实际死过的集合里。

结果：6 发终点全部对上，产出消息全部合法。


## 5. 完整一局的服务端循环（`src/graphwar_game_server.rs`）

### 5.1 装配了什么

`src/graphwar_game_server.rs` 把已有零件串成一个真正的服务端：

```text
NewConnectionRequest  -> 状态同步 7 条                     (09-network.md)
GameStartRequest      -> 生成关卡 + 士兵/队伍/位置/生命 + WaitingForFunctions
FunctionUpdateRequest -> 记下该玩家的函数
FunctionFireRequest   -> begin_turn -> fire（8 条）-> tick（到期销毁）-> end_turn
                      -> 某队全灭则 EndingGame              (14-turn.md)
```

关卡由 `graphwar_level_generator::generate`（`11-levelgen.md`）现生成，种子来自 `/dev/urandom`，
随机数用 `_scaffold::rng::ChaCha12`（与游戏同算法，`12-rng.md`）。

### 5.2 验证：真实 TCP 上打完一局

`tests/server_match.rs`（**自动化，`cargo test` 覆盖**）：

1. 服务端在 `127.0.0.1:0` 起监听，测试线程跑 `serve_stream`；
2. 客户端连上后发 `NewConnectionRequest`，断言握手序列**恰好是**那 7 条、顺序一致；
3. 发 `GameStartRequest`，断言：生成了障碍（>0）、4 名士兵、
   首回合 `TurnInfo`、最后一条是 `GameStateInfo`；
4. 按 `TurnInfo` 轮流开火（直线 `0.00`），最多 12 回合，**要求出现 `EndingGame`**。

实测：`test result: ok. 1 passed ... finished in 7.03s`。

手工跑同样的事：

```bash
cargo run --quiet --offline --example serve_match -- 127.0.0.1:39993 &
python3 tools/client_match.py 39993
```

实测输出（节选）：

```
开局: {'AddObstacle': 18, 'SoldierInfo': 4, ..., 'TurnInfo': 1}
第 1 发 (player 229): 15 条 -> [..., 'LifeQueuedDeath', ..., 'QueueDestroyObstacle', ...,
                              'DestroyObstacle', 'TurnCountInfo', 'TurnInfo', 'EndOfTurnInfo', ...]
第 2 发 (player 229): 13 条 -> [..., 'LifeQueuedDeath', ..., 'TurnCountInfo', 'GameStateInfo']
对局结束（EndingGame）
```

服务端日志：`ENDED at turn 2` —— 我方两发直线打掉对方两名士兵，队伍全灭。

### 5.3 修掉的两个 bug（都是测试逼出来的）

1. **没校验回合归属**：同一个玩家连发 8 次都被接受。已加 `current` 校验。
2. **取错了士兵**：一个玩家有两名士兵，按 `player_id` 查表会取到**已阵亡**的那个
   （日志里 `SOLDIER_DEAD 232`）。改成跟踪 `(player_id, soldier_id)`。

### 5.4 简化之处（明确写出来）

- **颜色分配**：用固定值占位（规则仍未知，`10-campaign.md`）；
- **士兵站位**：两队对称固定（真实关卡是随机的），便于复现；
- **朝向/`flip`**：按队伍固定（Team1 向右、Team2 向左），不是游戏里的真实规则；
- **tick 节拍**：由服务端在每次开火后立即推进到"销毁到期"，没有做真实的时间轮；
- **玩家加入流程**：只接受一个连接，没有 `NewPlayerRequest` 的完整分配逻辑。

这些都写在代码注释里，不是遗漏而是**尚未验证的选择**。

### 5.5 自建客户端连真游戏的服务器：试过，**没打通**（原 `64`）

动机：生命规则需要"按需制造"友军误伤 / 一发多杀场景，而随机 AI 撞不上（`65` §5）。
最直接的办法是自己写客户端连真服务器，提交瞄准队友的函数再开火。

> 2026-11 归并：原 `archive/64-join-real-server-attempt.md` 已移入 `docs/archive/`。

#### 5.5.1 进展

| 步骤 | 状态 |
|---|---|
| 找到服务器端口 | ✅ `ss -tlnp` 显示 Godot 监听 `0.0.0.0:46868` |
| 用真实帧格式发 `NewConnectionRequest` | ✅ 已发（`u16` 大端长度 + JSON，与抓包一致，`24` §1） |
| 收到服务端应答 | ❌ **没有任何回应** |
| 加入为玩家 | 未到达 |

#### 5.5.2 当时排除掉的三个环境坑（仍然有效）

- **`/tmp` 是每条命令独立的 tmpfs**：日志要写工作区，否则下一次命令就看不到（`90` §1 #4）；
- **`pkill -f Godot` 会连自己那条命令一起杀**：要按 PID 精确清理；
- **`ss -tln` 不带进程名**，匹配不到 Godot，必须 `-tlnp`。

#### 5.5.3 没通的可能原因（**未验证**，照抄原判断）

1. 连接时刻不对：端口在游戏启动早期就存在，但服务器可能要到"创建房间"后才处理请求；
2. 首包可能不是 `NewConnectionRequest`：只按抓包里出现的第一条 JSON 推断，
   **尚未核对连接建立后最初的原始字节**；
3. 监听 socket 可能不是对局协议（fd 8），对局用的是另一个端口。

#### 5.5.4 结论

这条路**尚未打通**，但每一步的卡点都具体。下一步优先核对"连接建立后服务端/客户端最先发的原始字节"。
`README` 的"网络协议"行把它记为待重试项。
### 5.6 一个必须记录的提取陷阱（失败教训，仍然有效）

我一开始断言"复算致死对象恰好是 `{230, 231}`"，测试**失败**了——只复算出 231。

原因不在模型，而在**抓包提取**：`FunctionInfo` 会随回合更新，
而我提取时只保留了每个实体的**最后一条**函数。杀死 230 的那发用的是
`(x/3.02)`，属于更早的回合，那条记录被后来的覆盖掉了。

⇒ 教训：**从抓包提取"随时间变化的状态"时必须保留时间线**，
不能按实体只留最后一条；否则会拿到"事后"的值去解释"事前"的行为。

因此这条测试只能断言"不越界"（复算出的死者都在抓包实际死过的集合内），
不能断言精确相等。要精确回放，提取时需要把每次 `FunctionInfo` 与随后的
`FunctionFire` 配对保留。

## 6. Broker：房间列表服务器

### 6.1 证据等级约定

| 档 | 含义 |
|---|---|
| **逐位验证** | `cargo test` 里有对拍：抓包原文 → 解析 → 重新序列化，字节相等 |
| **观测** | 能指到具体地址/字节/运行输出（objdump、夹具、netns 实测） |
| **推断** | 有间接证据但没直接观测；**下面每条都写明缺什么** |
| **未知** | 没有任何证据 |

### 6.2 传输层：和客户端↔服务器**同一套**帧

**逐位 + 观测**：

* 对局连接是 `u16` **大端**长度 + JSON 负载（`09-network.md` §1，实测帧头 `0x0031` = 49 字节负载）。
* broker 连接用的**是同一个写入函数** `0x3d89e0`：`graphwar_server_broker_handler` 的
  连接函数 `0x437ce0` 在 `0x437ef8` 直接 `call 0x3d89e0`。
* `0x3d89e0` 本体里能看到这两条决定性指令：
  * `0x3d8ab0: cmp $0xffff,%rbx` —— 长度必须塞得进 `u16`（否则走 `Message too large`）；
  * `0x3d8b7b: rol $0x8,%cx` —— 16 位长度做字节翻转，即**大端**。
* 同一函数的错误串：`Failed to serialize event.` / `Failed to write to tcp_stream. Message too long.`

⇒ broker 会话 = `[u16 BE len][JSON payload] [u16 BE len][JSON payload] …`，**没有换行、没有魔数**。

### 6.3 消息：全部是单键对象 `{"TypeName":{字段}}`

对局协议里已经确立的形状（`09-network.md` §2、`07-protocol.md`）在这里同样成立，且 broker 段的
三个消息**都能在序列化代码里逐字节读出来**：

#### 6.3.1 `VersionInfo` —— 会话第一帧（客户端 → broker）

```
{"VersionInfo":{"version":"2.0.60","system":"linux"}}
```

* **逐位验证**：真实报文（`fixtures/protocol_match.jsonl` 第 83 行）。
* **观测（静态）**：`0x437ce0` 在连上 broker 之后构造事件并调用发送函数，事件里那条
  5 字节字符串是**在栈上直接写出来的** `"linux"`（`0x437e97`：`movl $0x756e696c` = `linu`，
  `0x437ea6`：`movb $0x78` = `x`）。`system` 取的是编译期平台常量，`version` 是 `2.0` + `60`
  拼出来的（二进制里没有 `"2.0.60"` 这个字面量）。

#### 6.3.2 `GameInfo` —— broker 推给客户端的房间行

```
{"GameInfo":{"game_info":{"room_name":"US Room 2","game_state":"ExecutingResults",
 "current_player_count":2,"address":"69.164.213.144:35981","axis_mode":"EveryUnit",
 "function_mode":"NormalFunction","turn_mode":"SequentialTurns","time_mode":"Timer1m",
 "locked":false}}}
```

* **逐位验证**：`fixtures/protocol_match.jsonl` 第 84–86 行是**三条真实报文**；
  `tests/broker.rs` 与 `src/_scaffold/broker.rs` 的单测把它们解析后重新序列化，断言**字节相等**。
* **观测（静态）**：外壳由序列化函数 `0x50ff30` 逐字节写出——
  `'{'`（`movb $0x7b`）→ `"GameInfo"`（`0x19f398`，长度 8）→ `':'` → `'{'` →
  `"game_info"`（`0x1c5410`，长度 9）→ 内层结构体 → `'}'` → `'}'`。
* **观测（静态）**：内层结构体的序列化是 `0x5060d0`，9 个键名按
  `room_name, game_state, current_player_count, address, axis_mode, function_mode,
  turn_mode, time_mode, locked` 的顺序写（键名地址 `0x1c5875`/`0x1c587e`/…），最后补 `'}'`。
  它唯一的**直接**调用者是 LAN 广播 `0x457790`；事件路径经 trait 间接调用所以看不见直接调用边
  —— 这解释了"为什么只有一个直接调用者，但事件里确实带着同一个对象"。
* `tests/broker.rs` 里还有一条**运行期**证据（见 §5.3）：原版客户端接受一个**裸的** 9 字段
  `GameInfo` 对象并把房间行画了出来，说明这 9 个字段的**取值写法**（枚举是字符串、
  `locked` 是布尔、`current_player_count` 是整数）是对的。

#### 6.3.3 `GameRemoved` —— 房间消失

```
{"GameRemoved":{"game_address":"1.2.3.4:35981"}}
```

* **推断**（**从未抓到过真实报文**）。依据：序列化函数 `0x511de0` 写出
  `'{' "GameRemoved"(10 字节, 0x1c5703) ':' '{' "game_address"(12 字节, 0x1c585c) ':' 值 '}' '}'`。
  字段数 N=1 与 `.rodata` 里 `struct GameRemovedEvent with 1 element` 一致（`07-protocol.md` §5.1）。
* **缺什么**：没有一次真实流量证明 broker 真的会在房间消失时发这条、也没有证明字段类型是字符串。

#### 6.3.4 `Subscribe` —— 从未在 broker 段出现过

```
{"Subscribe":{"subscriber_id":<整数>,"subscribed_event_type":"<事件名>"}}
```

* **观测（静态）**：序列化函数 `0x5135d0` 写出上述形状（键名 `0x1c57d4`/`0x1c57e1`），N=2。
* **未知**：谁发、发给谁。抓到的 broker 会话里**没有**它，所以我们的 broker 只在收到它时
  重发一次房间表（把它当"刷新"），不依赖它。

### 6.4 broker 手里的"房间表"是什么结构

| 结论 | 等级 |
|---|---|
| 表项 = `GameInfo` 结构（9 字段） | **逐位验证**（真实报文 + 序列化代码） |
| 表项的**键** = `GameInfo.address`（`"host:port"` 字符串） | **推断**——依据是 `GameRemoved` 只带一个 `game_address`，且它的取值集合必然来自 `GameInfo.address`；没有抓到"更新同一房间"的流量来直接证明 |
| 同一 `address` 再次出现 = **更新**（人数/状态/锁变化），不是新行 | **推断**——真实抓包里 3 条 `GameInfo` 的 `address` 各不相同，没抓到过重复地址 |
| `game_address` 与 `address` 是**同一个值** | **推断**（字段名不同、语义相同）；`GameRemoved` 没有别的字段可以表示"哪个房间" |
| 房间列表的**顺序** | **未知**（我们的实现按地址排序，只为可复现，不作为复原结论） |

`GameInfo` 的 9 个字段（键序即序列化顺序）：

| 字段 | 类型 | 实测取值 |
|---|---|---|
| `room_name` | 字符串 | `"Graphwar Game"`、`"US Room 1/2/3"` |
| `game_state` | 字符串（枚举） | `"Setup"`、`"ExecutingResults"`（`07-protocol.md` 另有 `WaitingForFunctions`/`EndingGame`） |
| `current_player_count` | 整数 | `0`、`2` |
| `address` | 字符串 | `"69.164.213.144:35981"`、`"[::1]:35245"` |
| `axis_mode` | 字符串（枚举） | `"EveryUnit"` |
| `function_mode` | 字符串（枚举） | `"NormalFunction"` |
| `turn_mode` | 字符串（枚举） | `"SequentialTurns"` |
| `time_mode` | 字符串（枚举） | `"Timer1m"`（**不是** `Timers1m`，`09-network.md` §1.1 已更正） |
| `locked` | 布尔 | `false` |

### 6.5 房主怎么注册：长连接，不是心跳式上报

**推断（较强）**，依据都是静态的：

1. broker 侧有**专门的线程**：`graphwar_server_broker_handler.rs`
   （`0x4588d0` 里引用了该文件的 `:94` 日志：`failed to spawn thread`、
   `Failed to join connection thread.`）。
2. 这个线程**建连**（`0x437ce0`：解析地址 → `connect` → 发首帧；失败记
   `Failed to connect to broker at {}`，位置 `0x437d7d` 引用 `0x1a96a2`）。
3. 它**还读**这条连接：`0x4588d0` 调用 `0x3e5a80`，而 `0x3e5a80`
   （`src/server_broker_request_connection.rs:100`）是一个
   `Failed to read tcp_stream.` + `read_frame`（`0x3d86a0`）的循环。
   ⇒ broker **也会往房主那侧推事件**（推什么**未知**）。
4. 地址只有两个来源（`0x4581d0`）：命令行的 `--broker_address`，否则
   `GET https://graphwar.com/graphwar_II_ip.txt`（失败记 `:83`
   `Failed to get global server address.`，解析失败记 `:78`）。

**"房间在 = 连接在"** 是我们的实现口径；真实超时值**未知**
（`GameHeartbeat` 事件存在但**从未抓到过**，`07-protocol.md` §3 也把它列为"字段数都未知"）。

### 6.6 本地实验：做了什么、结果如何、边界怎么守的

#### 6.6.1 边界（先说结论）

* 全程 **只在本机**：所有客户端运行都在
  `unshare -rn`（只有 `lo` 的**新网络命名空间**）里，broker 地址写死 `127.0.0.1`。
  命名空间里没有默认路由，**任何**外发连接都不可能成功。
* 一次都没有连 `graphwar.com`。任务允许的那**一次** `curl https://graphwar.com/graphwar_II_ip.txt`
  **没有使用**——协议已经从仓库里已有的真实抓包定死了，不需要它。
* 没有任何规避检测/伪装/限流绕过的实现。

#### 6.6.2 想让原版客户端连我们的 broker —— **没做成**，原因是硬的

跑了 6 组受控实验（`--broker_address` 的各种写法），**客户端一次都没连到我们的 listener**：

| # | 传参 | 结果 |
|---|---|---|
| 1 | `--broker_address 127.0.0.1:42970` | 无连接；日志 `src/graphwar_server_broker_handler.rs:83 - Failed to get global server address.` |
| 2 | `--broker_address zzz`（故意非法） | **没有任何 clap 报错** ⇒ 参数根本没被解析 |
| 3 | `--broker-address 127.0.0.1:42970`（kebab） | 无连接（Godot 把未知参数拼成**一个**字符串） |
| 4 | `--broker-address=127.0.0.1:42970`（单 token） | 无连接 |
| 5 | 上面加 `--broadcast-port/--logger-port/--server-port` | **端口不变**（仍是 UDP 42968 / TCP 46868）⇒ CLI 完全无效 |
| 6 | **干净 argv**（只带游戏参数、`cwd`=工程目录、Xvfb 下窗口模式跑） | 仍走 `:83` 分支 |

结论（**推断，但证据收敛**）：这个构建里 **命令行参数是死的**。Rust 的
`std::env::args()` 从 `lang_start` 填充的静态量取 argv，而 GDExtension 是被 Godot
`dlopen` 的 cdylib，`lang_start` 从不运行 ⇒ `args()` 为空；二进制里也没有任何
`OS.get_cmdline_args` / `get_cmdline_user_args` 的方法名串（`grep cmdline` = 0 命中）。

于是**唯一**实际生效的地址来源是那次 HTTPS 请求。要把它改到本机，等价于要一张
`graphwar.com` 的证书：二进制的 TLS 根证书是**编进 rodata 的**（有 `rustls-webpki`、
`UnknownIssuer` 等串；**没有** `rustls-native-certs` / `platform-verifier` /
`SSL_CERT_FILE`），所以注入本地 CA 不可行。**这就是我们停下的原因**——再往前要么连
graphwar.com（越界），要么改二进制绕过证书校验（不做）。

#### 6.6.3 做成了什么：用原版客户端验证了**同一个房间表项**的线格式

broker 走不通，但"房间列表"这套东西还有第二个入口：**LAN 发现**
（`09-network.md` §1.1，UDP、明文 JSON、无长度前缀）。在同一个隔离命名空间里：

1. `ss -ltnup` 实测客户端在 LanLobby 绑 **UDP `0.0.0.0:42968`**（和 `127.0.0.0/8` 里的
   房间服务器 TCP 46868 不是一回事）。
2. 往 `127.0.0.1:42968` / `[::1]:42968` / `ff02::1%lo:42968` 灌候选报文：
   5 个变体各带一个独特 `room_name`，看哪一个出现在 UI 里。
3. 结果：**只有变体 C（裸的 9 字段 `GameInfo` + 结尾换行 `"\n"`）被接受**。
   `GraphwarLanVBox` 里出现一行 `game_room_entry.tscn`：
   `RoomName = "VAR-C-nl"`、`PlayerCount = "2"`。
   不带换行的裸对象（A）、事件包裹形式（B）、`{"game_info":…}`（D）、
   `"Timers1m"`（E）**全部被丢弃**。

⇒ 两条**运行期**结论：

* 房间表项的 9 个字段、键名与取值写法（枚举当字符串、`locked` 布尔、人数整数）是**对的**；
* LAN 通道的分帧是**换行**，而 broker 通道的分帧是 **`u16` 大端长度**（§1）——两条通道
  **不是**同一个分帧，别混用。

#### 6.6.4 仓库里本来就有一次真实的 broker 会话

`fixtures/protocol_match.jsonl` 第 83–86 行就是抓到的 broker 段（浏览方视角），
本次静态还原的三条形状（`VersionInfo` / `GameInfo` 外壳 / `GameInfo` 内层 9 字段）
**与它逐字节一致**。这构成了本文件最硬的一条对拍：不是"我推断它应该长这样"，
而是"代码读出来的形状 = 真实流量的字节"。

### 6.7 我们实现的 broker

#### 6.7.1 为什么放在 `src/_scaffold/broker.rs`（而不是 `tools/broker.rs`）

因为它要**进 `cargo test`**：复用 `src/_scaffold/protocol.rs` 里已被抓包钉死的
`Frame`（u16 大端帧）/ `Broadcast`（9 字段房间行）/ `VersionInfo`，并且能用两条普通
TCP 连接做端到端断言。放 `tools/` 只能靠人工跑；放 `src/` 才能被验收入口覆盖。
（`tools/` 在本仓库是"分析脚本"，不是产品代码。）

#### 6.7.2 行为表

| 触发 | broker 的行为 | 依据 |
|---|---|---|
| 新连接 | 记下写端，等第一帧 | — |
| 收到**首帧**（真实会话是 `VersionInfo`） | 打日志，然后把当前房间表**逐条**用 `GameInfo` 推回去 | **逐位**（真实会话：`VersionInfo` 之后紧跟 3 条 `GameInfo`） |
| 收到 `GameInfo` | 以 `address` 为键 upsert；广播给**其它**连接 | **推断**（房主方向未抓到；没有别的通道能到达 broker） |
| 收到 `GameRemoved` | 删键；广播给其它连接 | **推断**（消息本身从未抓到） |
| 收到 `GameHeartbeat` | 只刷新该连接名下房间的时间戳 | **推断**（`GameHeartbeat` 字段数都未知，不解析字段） |
| 收到 `Subscribe` | 把当前表再推一次 | **未知方向**，只做兼容 |
| 其它消息 | 忽略 | — |
| 连接断开 | 删掉该连接登记的所有房间，并广播 `GameRemoved` | **推断**（长连接注册） |
| 超过 `room_ttl`（默认 120 s）没更新 | 删房并广播 `GameRemoved` | **推断**（真实 TTL 未知） |

#### 6.7.3 用法

```bash
# 起一台（默认 0.0.0.0:42970，帮助文本里的示例端口）
cargo run --offline --example serve_broker -- 127.0.0.1:42970
```

#### 6.7.4 测试（12 条，全部不需要真游戏）

`src/_scaffold/broker.rs` 内 8 条 + `tests/broker.rs` 4 条：

* **逐位**：真实抓包的 4 条 broker 报文 → 解析 → 重新序列化 → 字节相等；
  `VersionInfo` 的 `2.0.60/linux` 字段；`GameInfo` 外壳字面量；
* 帧：`u16` 大端前缀、半包不整帧、`Message too large` 的边界；
* 房间表：upsert 覆盖、`remove_owner`、`expire`、快照排序；
* **端到端（真 TCP）**：房主连上登记 → 浏览客连上就收到这条房间；更新人数/状态后
  浏览客收到新记录；`GameRemoved` 到达；**房主掉线后浏览客收到 `GameRemoved`**。

### 6.8 与原版的差异（必须写清）

| 项 | 原版 | 我们的 |
|---|---|---|
| 地址来源 | 命令行 `--broker_address`，否则 HTTPS 取 `graphwar_II_ip.txt` | 只看命令行参数（`serve_broker`），没有任何拉取逻辑 |
| 房主登记 | **未观测** | 房主在同一条连接上发 `GameInfo`（推断） |
| 心跳/超时 | **未知**（`GameHeartbeat` 从未抓到） | 连接断开即删房 + 120 s 空闲兜底（推断值） |
| 房间顺序 | **未知** | 按 `address` 排序（只求可复现） |
| 重复连接 | **未知** | 不限制（相同 `address` 后到者覆盖，且归属换到新连接） |
| 鉴权/版本校验 | **未知**（`VersionInfo` 收到了，但没证据表明它会拒绝旧版本） | 只打日志，不拒绝 |
| `Subscribe` | **未知** | 收到就重发快照 |
| 错误处理 | `Failed to connect to broker at {addr}` 等 | 静默丢弃坏帧/坏连接，日志走 stderr |

### 6.9 仍未确定

| 问题 | 状态 | 缺什么 |
|---|---|---|
| 客户端（浏览方）侧的 broker 连接代码在哪 | **未知** | 抓到的那次会话是浏览方发出的 `VersionInfo`，而发 `VersionInfo` 的函数 `0x437ce0` 在 `graphwar_server_broker_handler.rs` 里、且**没有直接调用者**（线程/函数指针入口）⇒ 我们**推断**两端共用这段连接代码，但没能定位浏览方的调用点 |
| 房主如何登记/刷新房间 | **未知** | 需要一次房主视角的真实抓包 |
| broker 会不会回推事件给房主（`0x3e5a80` 在读） | **推断会读**，内容**未知** | 同上 |
| 房间的存活/超时策略 | **未知** | `GameHeartbeat` 从未抓到 |
| `GameRemoved` 真实报文 | **推断**（只有序列化代码） | 一次房间消失的抓包 |
| 房间列表顺序、分页、上限 | **未知** | 房间数 >3 的抓包 |
| 官方 broker 端口 | **未知** | 只能从 `graphwar_II_ip.txt` 得知，未读取 |


## 7. 还没抓到的部分（明确标注）

- ~~对局内消息~~ **已捕获**（见 4.5）：障碍、位置、生死、函数原文都有了；
- 仍缺：**开火结算的结果消息**（打中谁/伤害多少）与**回合推进消息**——
  这一局只跑了 60 秒且 Bot 只提交了一次函数；
- 服务器对局中广播给多客户端的顺序与去重规则未知；
- 二进制里的序列化实现（是手写还是 serde）未定位——目前只按**线上字节**还原。

⇒ 联机这一行现在是"**传输层与消息目录已还原并有真实样本**"，
但"完整对局协议"还差对局内消息的抓取。


## 8. 开火结算（第四轮抓包，240 秒对局）

### 8.1 结算相关消息

```json
{"FunctionActive":{"owner_id":234,"active_tick":10948}}
{"FunctionPoints":{"owner_id":234,"start_x":1.651220991201045,"start_y":4.638020825690482,"end_x":-16.405029008797804}}
{"QueueDestroyObstacle":{"pos_x":-16.405029008797804,"pos_y":4.638,"radius":0.5,"tick":11012}}
{"EffectInfo":{"effect_id":236,"pos_x":-16.405029008797804,"pos_y":4.638,"effect_type":"ExplosionEffect","start_tick":11012}}
```

**关键结构（观测）**：

- 服务器**只广播起点和终点 x**（`FunctionPoints`），**不广播轨迹本身**——
  所以轨迹必须由各客户端用同一个采样器重算，这正是"行为等价"要求的地方；
- 命中点 = `QueueDestroyObstacle` 的 `pos_x/pos_y`，`radius` 是**被摧毁实体的半径**
  （士兵恒为 0.5）；`tick` 与 `EffectInfo.start_tick` 相同，二者是同一次命中的两条记录；
- 命中后播 `ExplosionEffect`。

### 8.2 修正文本（实测）

```json
{"FunctionInfo":{"owner_id":234,"function":"0.00","corrected_function":"0.00 + 4.638"}}
{"FunctionInfo":{"owner_id":231,"function":"","corrected_function":" + 10.990"}}
```

`4.638` / `10.990` 正是各自的 `start_y`，证实修正是 `f(x) + c` 且**以文本形式拼接**；
`src/graphwar_shot_calculator::corrected_text` 复刻了这个格式（三位小数），并有针对这两个真实样本的测试。

### 8.3 用真实弹道对拍

`FunctionPoints` 给了起点与终点，`FunctionInfo` 给了函数原文，于是可以复算：

| 起点 | 抓到终点 x | 方向 | 我们的轨迹是否覆盖终点 |
|---|---|---|---|
| `1.651, 4.638` | `-16.405` | `flip=true`（向左） | **是** |
| `-19.513, 10.990` | `-12.041` | `flip=false`（向右） | **是** |

⇒ 两发的爆炸点都落在我们算出的轨迹上，且方向由实测确定（不是猜的）。

**还没做到的**：本局 **没有出现** `alive:false` —— 12 次 `EntityEffectInfo` 里
没有死亡事件，可能因为这一局双方各 2 名士兵且都在打空/打在障碍上。
因此"伤害数值/死亡判定"仍未取得真值。


## 9. 服务发现：房间列表从哪来（2026-11，纯字符串证据）

上一轮用户问"房间列表的来源"，当时答不出。**现在有答案了**：`.so` 里留着完整的
命令行参数说明（clap 生成的 help 文本），直接写明了四套地址/端口的用途。

### 9.1 原文字符串

| 名字 | 原文（逐字） |
|---|---|
| `BROKER_ADDRESS` | `Address of the broker server to connect (e.g. 127.0.0.1:42970). If not specified, the address is fetched from https://graphwar.com/graphwar_II_ip.txt` |
| `BROADCAST_PORT` | `Port used for LAN game discovery broadcasts` |
| `LOGGER_PORT` | `Port used for the debug logger` |
| `SERVER_PORT` | `Port the game server listens on when hosting a game. If not specified, the OS picks a free port` |
| `CRASH_REPORT_ADDRESS` | `Address of the crash report collector server (e.g. 127.0.0.1:42972). If not specified, the game's global server address is reused with CRASH_REPORT_PORT` |

配套的错误/日志串：

```
Failed to parse --broker_address:          （clap 解析失败）
Failed to connect to broker at             （连不上）
Failed to get global server address.       （取 graphwar_II_ip.txt 失败）
Failed to get global server address for crash reporting.
Failed to serialize game info.             src/graphwar_lan_broadcaster_v6.rs:78
https://graphwar.com/graphwar_II_ip.txt    （出现两次：broker 与崩溃上报各一次）
[broadcast_port / logger_port] 是必填参数："The following required argument was not provided: broadcast_port"
```

### 9.2 由此可确定的结构（观测 + 推断分层）

| 结论 | 等级 |
|---|---|
| 局域网发现走 **IPv6 广播**（`ff02::1` = 全节点多播，且模块名是 `graphwar_lan_broadcaster_v6.rs`） | **观测**（字符串 `ff02::1` + 模块名 + `Failed to serialize game info.` 的定位） |
| "房间列表"来自 **broker 服务器**，地址默认从 `https://graphwar.com/graphwar_II_ip.txt` 取 | **观测**（help 文本逐字） |
| broker 与崩溃上报**共用同一个 IP 文件**，崩溃上报换端口 | **观测**（同一句 help） |
| 客户端**连 broker 拿房间表**，再直连房主 | **推断**——字符串只说 "connect to broker"；房间表的具体消息（`GameInfo` 广播）已在抓包里见过，但没抓到 broker 这一段 |
| 各端口的具体数值 | **未知**——只有 `broadcast_port` / `logger_port` 是必填，说明由**启动方传入**，不在二进制里 |

### 9.3 对复现的影响

- 我们**不需要**复现 graphwar.com 的 broker 才能单机/局域网跑一局：
  `ServerPort` + 直连已经够（`09-network.md`）。
- 但要复现"进游戏能看到房间列表"这个体验，就得**自己写一个 broker**，
  且它的消息格式**我们没抓到过**（拿不到 graphwar 的服务端）。
- `--broker_address` 可指向任意地址，所以**可以用我们自己的 broker 顶替**做实验——
  这条是纯推断，未验证。

⇒ 更新对用户那个问题的回答：**房间列表来自一台 broker；地址从 graphwar.com 的一个
文本文件里取；局域网发现是另一套（IPv6 多播）。**

**2026-11-06 补充（`09-network.md`）**：broker 协议**已经还原**（与对局同一套帧，`u16` 大端长度 + JSON；
会话 = 客户端先发 `VersionInfo{version,system}`，broker 逐条推 `GameInfo{game_info}`），
并已实现 `src/_scaffold/broker.rs`。但发现一件更要紧的事：

> **`--broker_address` 在这个构建里完全不生效**（6 组受控实验：snake / kebab / `=` /
> 加必填端口 / 非法值 / 干净 argv 全都无效；二进制里也没有任何 `cmdline` 方法名串）。
> 客户端取地址的**唯一**路径是 HTTPS 取 `graphwar_II_ip.txt`，而 TLS 根证书**编在 `.rodata` 里**
> ⇒ **原版客户端无法在本地被改指**，除非去连 graphwar.com 或改二进制绕过证书校验。

所以"接入官方 global room"不是"协议通了就能连"——是**原版客户端在设计上只认那一个地址**。

---

## 10. 大厅房间列表控件 `GraphwarLanVBox`（2026-11，纯静态还原）

> 本节回答"**玩家看到的大厅列表是谁画的、按什么顺序、什么时候变**"，
> 并给出 `src/graphwar_lan_vbox.rs` 的全部出处。证据来源：
> Android **2.0.60** 的 arm64 `libgraphwarrust.so`（`.symtab` **未 strip**，权威名字与函数边界）
> + Linux **2.0.60** 的 x86-64 `libgraphwarrust.so`（strip，靠 `panic::Location` 跨架构锚对位）。
> 复现命令与 `docs/144` §5 同：`readelf -sW … | c++filt | grep lan_vbox`、
> `llvm-objdump -d …`（arm64），x86 侧用 `analysis/funcstarts_ghidra_va.pkl` 的函数边界。

### 10.1 一句话结论

`GraphwarLanVBox` **不是**一个"收到事件就重建列表"的控件，而是**每帧把一张
`HashMap<SocketAddr, GameInfo>` 投影成行节点**的容器：源表里有的地址有行、没有的行被
`queue_free`；行的内容每帧从**活表**重取（不是建行时的快照）；**不排序**；
Join 按钮**不直接连**，而是把该行地址投进自己的 `mpmc` 通道，**下一帧**才
`start_client_to_remote_server`。

### 10.2 它是 12 个 GDExtension 类里**唯一有 `process` 的大函数**那几个之一

| 项 | 值 | 等级 |
|---|---|---|
| 类名 / 基类 | `GraphwarLanVBox` / `VBoxContainer` | 观测（`.symtab` 的 `IVBoxContainer::*`） |
| 源文件 | `src/graphwar_lan_vbox.rs`（2.0.60 里 **≥174 行**） | 观测（`panic::Location`） |
| arm64 `FUNC` 数 | **13** + 6 个 `drop_in_place` 胶水 | 观测 |
| `process` | arm64 `0x450c20` / **14 524 B**；**x86 `0x4f90b0`** | 观测（唯一锚，20 个 `(file,line,col)` 与 arm64 逐一相同） |
| x86 到下一个函数起点 | `0x4fd1b0`（跨度 16640，**上界不是大小**） | 观测 |

**14 524 B 不是一个巨型函数**：`Local`/`Global` 两条分支各有一份**逐字节相同**的行更新代码
（arm64 `0x450f70`–`0x452170` 与 `0x45218c`–`0x4533a0`，主循环跨度都是 `0xb14`）。
x86 侧还有第二条**独立**证据：`res://game_room_entry.tscn` 字面量在全 `.text` 只有 **2** 条
交叉引用（`0x4f97ed`、`0x4fac67`），`JoinButton` **4** 条、`RoomName`/`PlayerCount` 各 **2** 条
—— **全部落在 `process` 里**，2/4 的分布正是"两分支各一套"。

### 10.3 数据模型（arm64，全部观测）

实例结构 **`0x70` 字节**：

| 偏移 | 字段 | 证据 |
|---|---|---|
| `+0x00` | `Sender<SocketAddr>`（16 B） | `init` `0x450b54` 写 `{1,ptr}`；闭包 `0x4491d8` 调 `mpmc::Sender::send` |
| `+0x10` | `Receiver<SocketAddr>`（16 B） | `try_recv(&self+0x10)`（`0x450c54` / `0x450d98`） |
| `+0x20` | `base: Base<VBoxContainer>`（24 B） | `assert_constructed(self+0x20)` → `is_visible_in_tree` |
| `+0x38` | `HashMap<SocketAddr, Gd<行>>`（48 B，桶 48 = 地址 32 + `Gd` 16） | 清理循环 `0x452018`；`0x4514c4` 的桶步长 `0x30` |
| `+0x68` | `show_global: bool`（默认 **false = Local**） | `init` `strb wzr`；`process` `0x450ed4` 读 |

**通道载荷是 `SocketAddr` 的硬证据**：`init` 的 unwind 路径引
`drop_in_place<Counter<mpmc::list::Channel<core::net::socket_addr::SocketAddr>>>`（`0x433b54`）。

### 10.4 房间表从哪来：**两套源，一个 bool 切换**

| `show_global` | 模块槽（在根节点的模块容器里） | 它的表 | 逐行取值 |
|---|---|---|---|
| `false`（Local） | `container+0x128` = `GraphwarLanBroadcasterV6` | `container+0x1a0`（桶步长 `0x78`=120） | `get_game_info`（arm64 `0x3ddce8`） |
| `true`（Global） | `container+0x240` = `GraphwarServerBrokerHandler` | `container+0x248` | `get_game_info`（arm64 `0x489b98`） |

根节点用 `get_node_by_name(self, "GraphwarRootNode", 16)` 取到，容器在 `root+0x1668`、
贴图加载器在 `root+0x16a0`。

⇒ **`GraphwarLanVBox` 不订阅任何 `Event`**：74 个事件一个都不在这里（无 `ready`、无
`subscribe_to_events*`、无 `Event` 判别值比较）。房间的增删是 broadcaster / broker 各自
改自己的表，本控件每帧**读内存表**。这与 `docs/144` §1.1/§3.1 的运行期结论一致。

### 10.5 每帧五步（`process`）

```text
① 排空 Join 通道（try_recv 循环）→ 每个地址 root.backend.start_client_to_remote_server(&addr)
② if !self.base().is_visible_in_tree() { return }        // 不可见 ⇒ 整帧不刷
③ root = get_node("GraphwarRootNode")；按 self+0x68 选 Local / Global 表
④ 逐行：self.rows.entry(addr)（hashbrown::rustc_entry）——缺行才 load 场景 + instantiate + add_child，
   然后无条件刷 房名 / 人数 / 4 个模式图标 / Join 可点性
⑤ 清理：遍历 self.rows，若 get_game_info(addr) 已取不到 ⇒ queue_free(行) 并移除
```

第 ⑤ 步的判据是**再查一次源表**，不是"比两份快照"——所以"出大厅 → 广播器停 → 自己那一行
下一帧消失"（`docs/144` §1.2）在实现上就是这一步。

### 10.6 行节点：新场景实例 + **按地址缓存复用**（不是对象池，也不是整页重建）

* 新地址：`load("res://game_room_entry.tscn", 26)` → `instantiate_as` → `add_child`（各一次）。
* 已存在的地址：**复用同一个节点**，只刷字段。
* 删行：`queue_free`。
* 行内 7 个节点全部**按名字递归查找**（`GraphwarNodes::get_node_by_name_rec`），
  与真机节点树（`docs/16-client.md` §160）逐字对上：
  `JoinButton`(10) / `RoomName`(8) / `PlayerCount`(11) / `AxisModeButton`(14) /
  `FunctionModeButton`(18) / `TurnModeButton`(14) / `TimeModeButton`(14)
  （括号里是指令里的长度立即数；arm64 `.rodata` 分别在 `0x19dd01`/`0x179000`/`0x19dd0b`/
  `0x19dd16`/`0x19dd24`/`0x19dd36`/`0x19dd44`，x86 在 `0x1c4394`/`0x19ef98`/`0x1c439e`/
  `0x1c43a9`/`0x1c43b7`/`0x1c43c9`/`0x1c43d7`）。

两条**玩家能看见**的细节：

| 细节 | 实现 | 等级 |
|---|---|---|
| 人数列是**补空格**的字符串 `"  {n}  "`（前后各 2 个空格） | `0x4517bc` `mov w9,#0x2020` + `strh` 写两个 `' '` 再 `Label::set_text` | 观测（与真机 dump 的 `"  0  "` 逐字符相符） |
| `locked` 的房间 **Join 不可点** | `0x4515a4` 从记录取一个字节 → `0x4515ac` `BaseButton::set_disabled` | 观测（字节→禁用）；**推断**（该字节叫 `locked`：9 字段里唯一的布尔） |

### 10.7 Join 是"投递 + 下一帧连接"

给 `JoinButton` 挂的是一个 **56 字节的 boxed 闭包**（环境 = `Sender`(16) + `SocketAddr`(32) +
容器指针(8)，正好是 `__rust_alloc(0x38, 8)` 的大小；闭包本体在 arm64 `0x449124`，264 B）：

```text
先查 LAN 表（container+0x128），再查 broker 表（container+0x240）——按下的那一刻重查
都没有 ⇒ 什么都不做（这一行已经不是房间了）
有 ⇒ Sender::send(该行的 SocketAddr)   // 送进本控件自己的通道
```

下一帧 `process` ① 才 `start_client_to_remote_server`。⇒ 连点两下在**同一帧内**不会发两次，
跨帧才会（通道不去重，本节实测无去重代码）。

### 10.8 排序：**没有排序，顺序不是信息**

* `process` 里 **0 处** `sort*`，也没有 `BTreeMap`。
* 插入顺序 = 源表 `HashMap` 的迭代顺序；显示顺序 = `VBoxContainer` 子节点顺序 = `add_child` 顺序。
* `HashMap` 用 `RandomState`（每进程随机种子）⇒ **同一张房间表，两次启动的顺序可以不同**（推断）。
* **去重**由"键 = `SocketAddr`"天然完成，不存在重复行。

⇒ 写攻略/复现时**不要**把大厅里房间的先后当成任何含义（不是按人数、不是按房名、不是按状态）。

### 10.9 与 `client/lan_lobby.gd` 的出入（**只登记，不改 `client/`**）

| # | 原版 | 我们的 `client/` | 影响 |
|---|---|---|---|
| 1 | 行按地址缓存复用，只改 4 个字段 | `_rebuild_list()` 整页重建 | 滚动位置/动画被重置 |
| 2 | Join 走内部通道，下一帧才连 | Join 直接 `connect_to()` | 连点两下可能连两次 |
| 3 | 没有地址就没有行 | 4 条**自定**占位行（无地址） | 见 `docs/144` §2 |
| 4 | `PlayerCount` = `"  {n}  "` | 直接写数字 | 列宽/对齐的像素差 |
| 5 | `locked` → `JoinButton.disabled` | 占位行用 `mouse_filter=IGNORE` 代替 | 语义相近、实现不同 |
| 6 | 无排序 | 无排序 | 一致 |

### 10.10 仍未确定

1. `try_recv` 的收尾判据 `cmp w8,#2` 的变体映射（按 niche 编码读 `2` = `Empty`；那么
   `Disconnected` 会被当成一条地址——一个**永不触发的潜在 bug**）。**推断**。
2. 闭包投递前 `记录+0x18` 的 `u16 == 2` 跳过判据是什么意思：载荷类型是 `SocketAddr`
   （判别值只有 0/1），**未知**。
3. `show_global`（`self+0x68`）**谁写**：`process` 只读；`docs/archive/112-meta-managers.md` §2 的 `0x3ef990`
   （取 `ShowLocalButton`/`ShowGlobalButton`/`GraphwarLanVBox` 并设文本/图标）最像，
   但没逐指令对上。**未知**。
4. `get_join_address`：2024 版 `.symtab` 有，2.0.60 arm64 **没有**这个符号。删了还是内联了，
   **未知**。
5. `load`/`instantiate`/`add_child` 是否在逐行循环体内（语言级结构）。**未定**，
   但不影响可测判据（新建必须一次、复用不重建）。
6. Join 处理器那个 `Vec<Box<dyn FnMut(&str)>>` 落在哪个结构体的 `+0x38`：同一个被 `bind`
   的对象还带 `+0x1668`（模块容器）⇒ 更像 `GraphwarRootNode` 而不是行里的按钮。**未定**。

实现与逐条对拍：`src/graphwar_lan_vbox.rs` + `tests/graphwar_lan_vbox.rs`。
⚠️ 按 `AGENTS.md` §0.2，那两处的 `cargo test` **只是回归保护，不是复原证据**；
本节所有结论的证据是上面的地址、`.symtab` 记录与 `.rodata` 字节。

## 11. 客户端联机层的两个模块（2026-11，arm64 未 strip 构建静态还原）

本节回答四个"想自己写客户端连原版服务端"必须知道的问题：
**(a)** 连接状态机有几个状态、**（b）**地址从哪来、**（c）**心跳/重连/超时、
**（d）** LAN 与 Global 两条路怎么分工。

### 11.1 证据基础（先说清楚凭什么）

主证据是 **Android 2.0.60 未 strip 构建**的 `lib/arm64-v8a/libgraphwarrust.so`：
`.symtab` 18793 个 `FUNC`，名字是完整 Rust mangled name ⇒ 不用猜函数归属。

两条架构怎么对上：靠 **`panic::Location`**（`{file:&'static str, line:u32, col:u32}`，24 字节，
在 `.data.rel.ro` 且里面的指针是 `R_AARCH64_RELATIVE` 重定位）。
把 arm64 的 `.rela.dyn` 解析出来，就能枚举"某个源文件的所有 panic 点"；
这些 `(行, 列)` 与 x86 构建 `fixtures/name_to_addr.tsv` 里的行号**逐条相同** ⇒ 同一份源码。

本节的"观测"级结论都是这么来的：**arm64 地址** = ELF 虚地址（`readelf -sW`/`objdump` 口径），
**x86 地址** = 本仓库既有口径（Ghidra = `+0x100000`）。
复现只需 `readelf` + 任意反汇编器（本文用的是 capstone：解 `adrp`+`add`/`ldr` 的 rip 相对
链得到 `.rodata` 字面量，再按 `.rela.dyn` 的 addend 还原 `&str`/`Location` 里的指针）。

两个模块（`docs/02-module-map.md` §17 的模块名，文件名同名）：

| 模块 | arm64 函数数 | x86 `fixtures/module_funcs.tsv` | 角色 |
|---|---|---|---|
| `graphwar_client_connection_handler` | 8 方法 + 3 订阅闭包（+1 连接线程本体） | 8 条 | 客户端连接状态机 + 帧收发 |
| `graphwar_backend` | 18 方法 | 6 条 | 联机层总开关（装/拆子模块、每帧 `process`） |

> **更正 `docs/07` 的口径**：`graphwar_backend` 不是"15 个方法"而是 **18 个**
> （`.symtab` 数出来的：`new/setup_level/stop_client/stop_server/start_server/button_pressed/
> subscribe_to_event/stop_lan_broadcaster/resubscribe_to_events/start_lan_broadcaster/
> start_client_to_replay/get_next_clip_checkpoint/stop_server_broker_handler/
> subscribe_to_event_with_id/start_server_broker_handler/start_client_to_local_server/
> start_client_to_remote_server/process`）。`docs/19-android.md` 说"它才是联机层不是广告"这条是对的。

### 11.2 （a）连接状态机：**4 个状态**，与 `GameState`（8 个）是**两套**

`enum ConnectionState` 是 **1 字节**枚举，放在 `Arc<Mutex<ConnectionState>>` 里
（`Mutex` 的 `futex` 在 `+0x10`、`poison` 在 `+0x14`、**数据首字节 `+0x15`**；
`get_connection_state` 就是 `ldrb w0,[x20,#0x15]`，arm64 `0x48ae70`）。

| 判别值 | 谁写 | arm64 |
|---|---|---|
| 0 `Disconnected` | `disconnect()`，关流之后**无条件**写 | `0x48aa10 strb wzr` |
| 1 `Connecting` | `connect()`，起线程**之前** | `0x48b900 strb #1` |
| 2 `Connected` | 连接线程：TCP 握手成功（`set_nonblocking` 之后） | `0x48de10 strb #2` |
| 3 `Failed` | 连接线程：`connect_timeout` 返回 `Err` | `0x48dd1c strb #3` |

唯一的读取者是 `tick()`，判据是 `cmp w8,#2; b.lo` + `b.ne`（`0x48b4a4`/`0x48b4b0`），
把 `0..=3` 切成三段：

```text
if self.latch { return }                       // 0x48b488 ldrb w8,[x0,#0x68]
state 0 | 1  -> 什么都不做                      // 连接线程还在跑 / 已经断线
state 2      -> 发 NewConnectionRequestEvent    // 0x48b4f4
state >= 3   -> GameStateManager::set_state(7)  // 0x48b514 -> 0x48ae398
latch = true                                   // 0x48b4b8 / 0x48b518
```

**闩锁（`self+0x68`）是这个设计的关键**：`connect()` 清零（`0x48b928`）、`tick()` 置 1。
每个 `connect()` 状态机只动作一次——没有它，状态 2 会在每一帧重发 `NewConnectionRequest`。

与 `docs/14` 的 `GameState`（8 个）**毫无关系**：那是 `GraphwarGameStateManager` 里的东西。
两者唯一的交点是上表最后一行——**连接失败时把 `GameState` 置 7**（该编号对应哪个界面**未知**，
只有"失败 → 7"是观测）。

"握手完成"用的是**另一个**字段，不是 `ConnectionState`：

* `self+0x00`（`u32`）= 0/1：「已握手」标志，`handle_connected_to_server` 写 1；
* `self+0x04`（`u32`）= 服务端分配的 `connection_id`（来自 `NewConnection{connection_id}`）。

```text
handle_connected_to_server(&mut self, ev):
    if ev.tag != 0x8000_0000_0000_0007 { panic!("explicit panic") }  // 0x48af80, LOC 249:18
    self.connected = 1; self.connection_id = ev.connection_id;       // 0x48af94 stp w9,w8,[x0]
```

上面那个 `0x8000_0000_0000_000N` 是 **`GraphwarEvent` 的 Rust 内部判别值**，
**不是** `docs/07` 的协议 tag（协议 tag 是 serde 侧的事）。本模块只碰两个：
`…0007` = ConnectedToServer、`…0006` = 断线事件（`.rodata 0x198680` 的 16 字节常量）。

### 11.3 （b）地址从哪来：**两条起点，都只认 IP 字面量**

1. **本机 / 局域网（host）**：`start_client_to_local_server`（arm64 `0x407848`）
   在栈上**逐字段拼** `SocketAddr`：
   `strh #1`（枚举变体 1 = `V6`）→ 16 字节来自 `.rodata 0x197680`
   （**全零 + 末尾 1 = `::1`**）→ `flowinfo`/`scope_id` 清 0 → `strh` 写端口。

   ⇒ **本机联机走 IPv6 环回 `[::1]:<port>`，不是 `127.0.0.1`**，与 §1.1 抓到的
   `"address":"[::1]:35245"` 完全一致。端口现取（`GraphwarGameServer::get_server_port()`），
   且 `self+0x60 == 4` 或端口为 `None` 就**直接返回** ⇒ 必须先起服务端。

2. **远端 / 直连**：`start_client_to_remote_server(&mut self, addr: &SocketAddr)`
   （arm64 `0x407b9c`）——它**不解析任何字符串**，拿到的已经是解析好的 `SocketAddr`。
   解析发生在调用方，用的是 `SocketAddr::from_str`（该符号全库只有一份，arm64 `0x83f174`）：
   * `GraphwarRootNode::process_button_pressed`（`0x3a6d48`）：`0x3a9400` 解析 → `0x3a9470` 调它；
   * LAN 大厅点某一行：`IVBoxContainer::process`（`0x450c20`）→ `0x450d68` 调它。

   ⇒ **只认 `IP:port` 字面量**：`[::1]:35245` 可以（IPv6 要带方括号），
   `example.com:1234` **不行**——`from_str` 是纯文法解析，**没有 DNS**。
   这条对"自己写客户端"很关键：想连域名得自己先解析成 IP。

3. **全局地址的文本**从哪来：一个 `thread::spawn` 闭包里
   `reqwest::blocking::get("https://graphwar.com/graphwar_II_ip.txt")`（39 字节，
   arm64 `0x48e130` 的 `0x48e168` + `mov w1,#0x27`），失败记
   `"Failed to get global server address."`（36 字节，`0x1a06f4`），并缓存到
   `"user://local_ip.config"`（`0x1a16ca`，`godot_core::tools::save_load::{load,save}` 成对使用）。
   缓存**文件格式未知**（没读到过真实文件）。
   全部来源都拿不到时兜底 `"127.0.0.1:8080"`（`0x1a067a`，同样过 `from_str`）。
   ⇒ §6.6.2 记录的"这个构建里命令行参数是死的，实际只有 HTTPS 一条路"与本节一致。

### 11.4 （c）心跳 / 重连 / 超时：**没有心跳，也不重连**

* **`GameHeartbeat`（协议 tag 63）不由本模块发，也不由它订阅。**
  证据：`new` 的订阅表里**没有**它（见 §11.6）；`tick` 里唯一发出去的帧是
  `NewConnectionRequest`；整个 `.rodata` 里 `game_heartbeat_event` 出现 **0 次**
  （只有 serde 侧的结构体名 `GameHeartbeatEvent`，`0x19ea9c`/`0x19f6a1`），
  `subscribe_to_event` 的名字分派器里也没有它。
  ⇒ §6.5 里"GameHeartbeat 存在但从未抓到过"有了新解释：**这个构建里没有代码会发它**。
* **唯一的时间常量是连接超时 30 秒**：`TcpStream::connect_timeout(addr, Duration::from_secs(30))`
  （`0x48dccc mov w1,#0x1e` + `0x48dcd0 mov w2,wzr`）。没有重连退避、没有 keepalive 周期。
* **断线靠错误发现**，三个入口同一个动作（§11.6 的 `forward_event` / `tick` 读循环 / `disconnect`）：

| 入口 | 触发 | arm64 | 动作 |
|---|---|---|---|
| `forward_event` | `write_event` 返回 `Err` | `0x48acc4` | 把断线事件 `…0006` **本地**派发给 `GraphwarRemoteEventHandler` |
| `tick` 读循环 | `read_event` 返回 `Err` 且 `ErrorKind != WouldBlock(13)` | `0x48b67c cmp w8,#0xd` | 同上（且只在已握手时派发：`0x48b698 tbz w8,#0`） |
| `disconnect()` | 玩家主动断 | `0x48a8c8 cmp w8,#1` | 先把断线事件 `write_event` **写出去**，再关流、再置 0 |

  非阻塞读空（`WouldBlock`）被当成"这一帧没数据"直接吞掉——**不是**断线。
* 读写错误路径**不写** `ConnectionState`（**推断**：`tick` 只读状态不写，`disconnect` 才写 0；
  没有观测到"错误之后状态变成什么"）。

### 11.5 （d）LAN 与 Global：**两个独立字段，不是二选一开关**

`GraphwarBackend` 把四个子模块**内联**在肚子里：

| 偏移 | 类型 | 通道 |
|---|---|---|
| `+0x110` | `GraphwarLanBroadcasterV`（内联） | LAN：UDP / IPv6 组播 `ff02::1`（本地常量 `::1`）/ 明文 JSON / **换行分帧** |
| `+0x228` | `Option<GraphwarServerBrokerHandler>`（内联） | Global：TCP / **`u16` 大端长度 + JSON** |
| `+0x1e0` | `Option<GraphwarGameClient>` | 客户端（内含 §11.2 的 handler） |
| `+0x220` | `Rc<RefCell<GraphwarModuleContainer>>` | 模块容器 |

* **房主两条一起装**：`GraphwarRootNode::process_button_pressed`（`0x3a6d48`）在同一个分支里
  连续调 `start_lan_broadcaster`（`0x3a79e8` 等 6 处）与 `start_server_broker_handler`
  （`0x3a7a28` 等 6 处）。⇒ 原版"开房"= 两路都挂，**互不排斥**。
* **"看哪一路"不是本模块的事**：本模块每帧只负责把活着的子对象挨个 `process` 一遍
  （顺序固定，见 §11.6）。挑哪一路显示是 `GraphwarLanVBox` 的 `show_global`
  （§10.10 第 3 条已记为"谁写未知"）。
* **两路在大厅里汇成同一张表**：`IVBoxContainer::process`（`0x450c20`）里
  `GraphwarLanBroadcasterV::get_game_info` 与 `GraphwarServerBrokerHandler::get_game_info`
  **都调**，两处返回的都是同一个 9 字段 `GameInfo`（§6.4 逐位验证过的那张）。

`process()`（arm64 `0x407ef0`）的固定顺序，两个守卫：

```text
if client.is_some() {                       // 0x407f18
    if mode != 2 { ReplayPlayer::process }  // 0x407f20 cmp x8,#2
    GameClient::process                     // 0x407f38
}
if lan.is_some() {                          // 0x407f48
    if backend.state != 4 && lan.ready {    // 0x407f54 cmp x8,#4 / 0x407f60 cbz
        if let Some(port) = server_port() { LanBroadcaster::set_room_info(...) }
    }
    LanBroadcaster::process                 // 0x408224
}
if broker.is_some() { Broker::process }     // 0x40822c → 尾调用 0x48a078
```

`set_room_info` 的入参（`0x3ddeb4` 是它的被调者）把房间名、人数、`game_state`、
`axis_mode`/`function_mode`/`turn_mode`/`time_mode`/`locked` 与端口一起塞进广播——
即 §1.1 那条 9 字段 LAN 报文就是从这里出去的。

### 11.6 客户端 handler：订阅 26 个事件名，只做"转发"

`new`（arm64 `0x48b048`）在 `Rc::new_cyclic` 里订阅：

* `tick_event`（长度 `0xa`，`0x1a016a`）→ `tick`
* `connected_to_server_event`（长度 `0x19`，`0x1a01ce`）→ `handle_connected_to_server`
* **24 个** `*_request_event`（栈上 24 项 `(&str, usize)` 表，`0x48b15c..0x48b298`，
  `mov w25,#1` + `cmp x25,#0x19` 循环 24 次）→ `forward_event`

24 个名字，**顺序 = 表顺序（不是字母序）**：

```text
tick_request_event            new_connection_request_event   new_player_request_event
game_start_request_event      function_fire_request_event    function_update_request_event
change_team_request_event     remove_player_request_event    add_soldier_request_event
remove_soldier_request_event  axis_mode_change_request_event function_mode_change_request_event
turn_mode_change_request_event time_mode_change_request_event game_name_request_event
new_bot_request_event         name_request_event             skin_graphics_request_event
face_graphics_request_event   hat_graphics_request_event     functions_calculated_event
rank_request_event            lock_request_event             chat_message_request_event
```

`forward_event` 只有一句话：拿 `Mutex<GraphwarStreamBuffer>` 的锁，`write_event`，
出错就按 §11.4 走断线。**不解析、不改写**。

三个闭包分别把 `weak_caller` 打到 `forward_event`(`0x48ac54`)、
`handle_connected_to_server`(`0x48af7c`)、`tick`(`0x48b468`)（各 44–48 字节的
`adr x1,<目标>` + `b WeakCaller::call`）。

**最小可连通客户端**（给"自己写客户端"的人）：

```text
TCP 连到 <ip>:<port>  ->  发 {"NewConnectionRequest":{"major_version":"2.0","minor_version":"60"}}
                      <-  收 {"NewConnection":{"connection_id":N}}  （之后可以什么都不发）
```

版本串的来源是硬的：`make_new_connection_request_event`（arm64 `0x4693c0`）在堆上
逐字节写 `"2.0"`（`mov w9,#0x2e32`+`mov w8,#0x30`）与 `"60"`（`mov w9,#0x3036`）。

### 11.7 只能标"推断/未知"的部分

| # | 结论 | 等级 | 缺什么 |
|---|---|---|---|
| 1 | 状态 2 = "TCP 已通、尚未握手" | **观测**（`0x48de10` 在 `set_nonblocking` 成功后写 2；`tick` 见 2 就发握手帧） | — |
| 2 | 状态 3 = "连接失败"，`GameState=7` 是失败态 | 状态迁移是**观测**；`7` 的界面语义**未知** | 没有 7 的枚举名/画面证据 |
| 3 | 读写错误后 `ConnectionState` 保持原值 | **推断** | 没观测到错误之后的状态字节 |
| 4 | `read_event` 的 `0x…4a` 返回值 = EOF 还是"暂时无事件" | **未知** | 两个"无事件"返回值只分得出哪个走错误路 |
| 5 | `"127.0.0.1:8080"` 的确切生效点 | **推断**：它被 `SocketAddr::from_str` 解析（`0x487fe8`/`0x4880e0`），位于 `subscribe_to_event` 体内（内联进来的解析器），何时落到这条兜底没跑起来验证 | 真机抓一次"没网"启动 |
| 6 | `user://local_ip.config` 的文件格式 | **未知** | 没读到真实文件 |
| 7 | `GraphwarBackend+0x00 == 2`（跳过回放）、`+0x60 == 4`（本机联机守卫）、`+0x180` 的意义 | **未知** | 只观测到比较，没有写点归属 |
| 8 | 订阅表里第 25 个栈槽（`0x196af0` 的 `(0, 0x18)`） | **未知**：循环只读 24 项，这个槽没有被读 | 可能是编译器残留 |

### 11.8 实现与验收

* `src/graphwar_client_connection_handler.rs`：`ConnectionState`（4 变体）、
  `tick_action`（三段分派 + 闩锁）、`state_after_connect_attempt`、26 个事件名、
  结构体/`Mutex` 布局常量、16 条 `Location` 表。
* `src/graphwar_backend.rs`：18 个方法的表 + 12 条 `Location`、`process_steps`
  （5 步 + 两个守卫）、`ClientTarget`/`HostChannel`、地址常量（`::1`、`ff02::1`、
  `graphwar_II_ip.txt`、`local_ip.config`、`127.0.0.1:8080`）、默认房名。
* 两者**都不碰网络、不碰文件**，只有常量与纯判定函数——"意图结构"而不是实现。
* ⚠️ 按 `AGENTS.md` §0.2，`tests/graphwar_client_connection_handler.rs` 与
  `tests/graphwar_backend.rs` 的通过**只是回归保护，不是复原证据**；
  本节所有结论的证据是上面的地址、`.symtab` 记录与 `.relro`/`.rodata` 字节。

## 12. 联机/流这一组的收尾：帧层 / 回放读取 / LAN 广播 / broker 两件（2026-11）

本节收掉五个模块 —— `graphwar_stream_buffer` / `graphwar_replay_reader` /
`graphwar_lan_broadcaster_v6` / `graphwar_server_broker_handler` /
`server_broker_request_connection`。其中**三个的 `panic::Location` 锚数为 0**
（`graphwar_stream_buffer`、`graphwar_replay_reader`、
`server_broker_request_connection`），所以对位方法换成**别的东西**，见 §12.1。

### 12.1 无锚模块怎么对位（方法，之后还会用）

| 手法 | 用在谁 | 判据 |
|---|---|---|
| **唯一字面量 join** | `graphwar_stream_buffer`、`graphwar_replay_reader` | 某条串在**整份 x86 `.so` 里只被 N 个函数引用**，arm64 侧同一串只在 M 个函数里出现，N=M 且语义可分辨 ⇒ 配对。`"Message too large"` 1↔1；`"user://replays/"` 2↔2 |
| **被调者指纹 + 锚点排除** | `server_broker_request_connection` | 它唯一"已对位"的被调者是 `GraphwarStreamBuffer::read_event`（x86 `0x3d86a0`）；全 `.text` 调它的只有 3 个函数，另外 2 个各有自己的 `src` 锚 ⇒ 剩下的那个就是它 |
| **不行就不给地址** | 两个 `get_game_info`、broker 的 `new` | 既无锚、又无可判等字面量、尺寸又小 ⇒ x86 列写"无锚"，**不猜** |

⚠️ **尺寸比闸（0.61–1.19）不是判据，是排除器**：本轮有 3 处**出闸但已判等**的配对
（`write_event` 1.456、`load_game` 1.469、broker `process` 1.226）。出闸的一律在表里注明
**靠什么判等**，不许只写"尺寸不符所以未知"。

### 12.2 `graphwar_stream_buffer`（2 个函数）——**它就是帧层**

| 函数 | arm64 | x86 | 比 |
|---|---|---|---|
| `read_event`  | `0x45c51c` / 764  | `0x3d86a0` / 832  | 1.089 |
| `write_event` | `0x45c818` / 1044 | `0x3d89e0` / 1520 | 1.456（靠字面量判等） |

x86 `0x3d89e0` **就是 §6 里"broker 连接用的写入函数"**（`cmp $0xffff` + `rol $0x8`），
所以这个帧层是**对局 / 服务端 / broker 三条 TCP 通道共用**的，不是哪一条私有：

| 方向 | 调用方（arm64） |
|---|---|
| `write_event` | `GraphwarRemoteEventHandler::handle_client_remote_event`(`0x3e3ad0`)、`GraphwarClientConnectionHandler::{disconnect,forward_event}`(`0x48a8a0`/`0x48ac54`)、`GraphwarServerConnectionHandler::{forward_event,forward_connection_specific_event}`(`0x48badc`/`0x48c02c`) |
| `read_event` | `ServerBrokerRequestConnectionClient::process`(`0x3ea0cc`)、`GraphwarClientConnectionHandler::tick`(`0x48b468`)、`GraphwarServerConnectionHandler::tick`(`0x48c644`) |

**问题 (a) 的答案**：它不是"TCP 拆包缓冲"，**内部没有字节队列**——它靠
`TcpStream::peek` 前瞻，整帧没到之前**一个字节都不消费**：

```text
write_event(&self, ev):  serialize → u16 大端(len) write_all(2) → write_all(len)
read_event(&self):       peek(2) → len=大端 → len>0x1000 报错
                       → peek(len+2)（不足 ⇒ Ok(None)，不消费）
                       → read_exact(len+2) → 负载 = buf[2..] → serde_json
```

`push/read` 语义因此是：**写侧"整帧一次推出去"（两段 `write_all`，限长 `0xffff`）、
读侧"整帧一次取走"（`read_exact(len+2)`，限长 `0x1000`）**，中间没有部分帧状态。
⇒ 顺带结掉 §11.7 第 4 条：`0x…4a` = `Ok(None)` = **"整帧未到"**，不是 EOF；
EOF 走 peek 返回 0 字节那条（`Err("Connection Lost")`）。

错误串（`.rodata`，长度取自指令里的 `mov w,#len`）：
`"Message too large"`(17) / `"Connection Lost"`(15) / `"Failed to serialize event."`(26) /
`"Failed to write to tcp_stream."`(30) / `"Failed to write to tcp_stream. Message too long."`(48)。
⚠️ `"Connection Lost"` 在 x86 构建里**逐字节搜不到**——两版这条消息不同（差异**未知**）。

### 12.3 `graphwar_replay_reader`（2 个函数）——**问题 (b)**

| 函数 | arm64 | x86 | 比 |
|---|---|---|---|
| `load_games` | `0x3da190` / 480  | `0x3d76f0` / 608  | 1.267 |
| `load_game`  | `0x3da370` / 1612 | `0x3d5740` / 2368 | 1.469 |

**答案 (b)：文件归 `replay_reader`，时钟归 `replay_player`。**

| | `graphwar_replay_reader` | `graphwar_replay_player` |
|---|---|---|
| 读文件 | **是**：`FileAccess::open(path, 1)` + `get_buffer()` + `RefCounted::unreference()` | 不碰 |
| 解压 | **是**：`flate2::gz::bufread::GzDecoder` + `io::append_to_string`（整份解成 `String`） | 不碰 |
| 拆行/解 JSON | **是**：`StrSearcher` 逐行 + `serde_json` → `graphwar_game_recorder::EventInfo` | 消费 `Vec<EventInfo>` |
| 推进 tick | **不碰**（被调者里**没有任何时间/tick 函数**） | **是**：`tick += floor(dt*60*speed)` |

* 目录字面量 `"user://replays/"`，**15 字节、自带结尾斜杠**（arm64 `.rodata 0x19bdef` +
  `mov w1,#0xf`；x86 `0x1bf7d7` + `mov $0xf`）。拼名不插分隔符；
* `load_games` 用**静态** `DirAccess::get_files_at`，结果是 `Vec<String>`（元素 8 字节 =
  `GString` 大小：arm64 `lsl x23,x21,#3`，x86 `mov $0x8,%r14d`）；
* `load_game` **坏行跳过、不中断**（`Result` 被 `drop` 后回到循环头）。

### 12.4 `graphwar_lan_broadcaster_v6`（5 个函数，4 个有 x86 对位）

| 函数 | arm64 | x86 | 比 |
|---|---|---|---|
| `get_game_info` | `0x3ddce8` / 460  | **无锚** | — |
| `set_room_info` | `0x3ddeb4` / 432  | `0x4572c0` / 448  | 1.037 |
| `new`           | `0x3de064` / 312  | `0x457480` / 384  | 1.231（超闸，但有**唯一锚** `:35:51`） |
| `start`         | `0x3de19c` / 372  | `0x457600` / 400  | 1.075 |
| `process`       | `0x3de310` / 2040 | `0x457790` / 2208 | 1.082 |

**问题 (c) 的两个答案：**

1. **广播周期 = 1 秒**（§11 里没写）。`process` 的时间闸是
   `Instant::now() - self.last_broadcast > Duration::from_secs(1)`
   （`0x3de7b8`/`0x3de7c4` 求差，`0x3de7c8`/`0x3de7cc`/`0x3de7d4` 三段比较合成 `> 1s`；
   正好 1.000 s **不发**）。`new` 把 `Instant::now()` 写进 `+0x48`/`+0x50` ⇒ 起表后
   第一帧也是 1 秒后才发。**不是每帧发**。
2. **收到的一方按"发送方 `SocketAddr`"去重 + 10 秒过期**。房间表
   `HashMap<SocketAddr, (GameInfo, Instant)>` 在 `+0x78`（桶步长 `0x78` = 120 B，
   与 §10 里 vbox 记的同一常量互证）；`GameInfo` 解析成功后 `insert` 到**发送方地址**这个键上
   ⇒ 同一个 host 连发只留一条（后写覆盖），不同 host 同房名则**两条**。
   每帧扫表、`Instant::now() - entry.instant` 的 `secs >= 10` 就删（`0x3de6e4 cmp x0,#0xa`）
   ⇒ TTL **10 秒**（host 掉线后最长 10 秒仍在别人的列表里，**推断**）。

另外两条新事实：

* **广播里的 `address` 恒为 `[::1]:<port>`**：`set_room_info` 内部
  `IpAddr::from_str("::1")`（arm64 `0x3ddf08`；x86 字面量 `::1` 在 `0x4572ee`）。
  这给 §1.1 那 385 个 `"[::1]:35245"` 样本提供了**成因**（当时只记"观测"）；
* **帧边界 = UDP 数据报边界**：收侧 `recv_from` 到 **1000 字节**栈缓冲
  （`mov w2,#0x3e8`），`str::from_utf8` → `serde_json` → 插表；发侧 `send_to` 一条
  序列化好的 JSON，**不追加 `\n`、不追加 NUL**。⇒ §11.5 说的"换行分帧"应理解为
  "面向行的文本协议"，本构建里并不真的发换行（**观测**：`GameInfo::serialize` 之后
  直接 `send_to`）。

结构：总长 **`0xc0`**（`start_lan_broadcaster` 整块拷 6×32 字节到 `backend+0x110`），
socket fd 在 `+0xb8`（**`-1` = 还没 bind**，`new` 写死），组播地址副本在 `+0xa8`，
表在 `+0x78`（`ctrl` / `mask` / `growth` / `items` / `hasher` 依次 `+0x78`/`+0x80`/`+0x88`/`+0x90`/`+0x98`）。
⚠️ §10 里 vbox 读的 `+0x1a0` **不是**这张表 ⇒ vbox 读的是另一张（**未知**）。

### 12.5 `graphwar_server_broker_handler`（5 个函数 + 1 个线程体）

| 函数 | arm64 | x86 | 比 | 判据 |
|---|---|---|---|---|
| `get_game_info` | `0x489b98` / 460  | **无锚** | — | 无锚、无字面量 ⇒ 不给 |
| `new`           | `0x489d64` / 236  | **无锚** | — | 同上 |
| `stop`          | `0x489e50` / 344  | `0x4586d0` / 288 | 0.837 | 唯一锚 `:90:52` |
| `start`         | `0x489fa8` / 208  | `0x4587f0` / 224 | 1.077 | **推断**：夹在 stop/process 之间 + 尺寸 |
| `process`       | `0x48a078` / 2088 | `0x4588d0` / 2560 | 1.226（超闸） | 4 个锚命中 **3** 个 |
| `start::{{closure}}` | `0x48e130` / 1984 | `0x437ce0` / 1344 | 0.677 | 两个文件的 `Location` 同时命中（见 §12.6） |

* 三个字段：房间表 `HashMap<SocketAddr, GameInfo>`（桶步长同 `0x78`）、
  `Mutex<Option<ServerBrokerRequestConnectionClient>>`、线程 `JoinHandle`
  （`0x48a020` 一拿到就 `drop` ⇒ **分离线程**）；
* `process` 一帧：`lock()` → `client.process()` → 事件按 `GameInfo`/`GameRemoved`
  分别 `insert` / `remove_entry`（两张符号都在）→ 落尾 `drop_in_place<time_probe::TimeProbe>`；
* `TimeProbe` 的打印阈值常量 **`0.005`**（`0x179fa0`，`ldr d1,[x8,#0xfa0]`）⇒
  "耗时 > 5 ms 才打印"是原版**留在里面的计时日志**（**推断**）；
* **表键是房间地址**（`GameInfo.address`），与 LAN 那一路的"发送方地址"**不同**；
* 本模块**没有** TTL 扫描（无 `Instant::now - entry.instant`）⇒ Global 一路的过期
  完全依赖 broker 推 `GameRemoved`（该报文**从未抓到**，形状来自序列化代码）⇒ **未知**。

### 12.6 `server_broker_request_connection`（**只有 1 个函数**）

| 函数 | arm64 | x86 | 比 | 判据 |
|---|---|---|---|---|
| `process` | `0x3ea0cc` / 688 | `0x3e5a80` / 816 | 1.186 | 被调者指纹（§12.1 第 2 行） |

它是**房主连 broker 的那条客户端连接**（`Mutex<Option<…Client>>` 里，符号可证），
`process` 就干一件事：循环 `GraphwarStreamBuffer::read_event` 把已到的整帧榨成
`Vec<GraphwarEvent>`（`Ok(None)`/`WouldBlock` 即停，其它 `Err` 打日志后停）。

⚠️ **"连上 + `set_nonblocking` + 写第一帧"没有独立符号**：它被内联进
`GraphwarServerBrokerHandler::start` 起的**线程体**（arm64 `0x48e130` = x86 `0x437ce0`）。
证据是那个函数里**同时**出现
`src/graphwar_server_broker_handler.rs:60:55` 与
`src/server_broker_request_connection.rs:66:42` 两条 `Location`，外加字面量
`"Cannot set non-blocking"` 与一次 `GraphwarStreamBuffer::write_event`。
⇒ `fixtures/module_funcs.tsv` 把 `0x437ce0` 记到 `graphwar_server_broker_handler.rs`
是**对的一半**：它同时属于两个文件。**"谁持有它"不能反推"它在哪个模块"**，这就是活例。

第一帧（**推断**，抓包一侧）：`{"VersionInfo":{"version":"2.0.60","system":"linux"}}`，
版本串逐字节可知（`0x4693c0` 里 `mov w9,#0x2e32`=`"2."`、`mov w9,#0x3036`=`"60"`）。

### 12.7 实现与验收

* `src/graphwar_stream_buffer.rs` / `graphwar_replay_reader.rs` /
  `graphwar_lan_broadcaster_v6.rs` / `graphwar_server_broker_handler.rs` /
  `server_broker_request_connection.rs`：常量 + 纯判定函数 + 无 IO 的状态机骨架
  （socket / 文件 / 线程全部由调用方注入）。
* 验收在 `tests/graphwar_stream_buffer.rs`、`tests/graphwar_replay_reader.rs`、
  `tests/graphwar_lan_broadcaster_v6.rs`、`tests/graphwar_server_broker_handler.rs`、
  `tests/server_broker_request_connection.rs`，用 `#[path]` 把源码编进测试二进制
  ⇒ **`src/lib.rs` 不需要改**也能跑（与 `tests/graphwar_replay_control.rs` 同一手法）。
* ⚠️ 按 `AGENTS.md` §0.2：这些测试通过**只是回归保护，不是复原证据**。
  本节全部结论的证据是上面列出的地址、`.symtab` 名字/尺寸、`.rodata` 字面量与
  `panic::Location` 键集。

## 12. 角色划分补一条：`EffectInfo.pos` 传到线上的是"位置"，不是"算好的爆炸点"（2026-11-08，`dispatch/26`）

§8.1 记着"服务器只广播起点和终点 x（`FunctionPoints`），不广播轨迹本身 ⇒ 轨迹由各客户端重算"。
本节补上另一半，并纠正一种读法：

1. **`EffectInfo`(44) / `QueueDestroyObstacle`(42) 里的坐标是权威端自己算出来的**，
   不是从别的客户端收来的。生产者在 `GraphwarGameStateManager::execute_results`，
   读的是**它自己进程里的点数组**（`0x472865 movdqu xmm0,[rdx+rcx-0x10]`，见
   [`155`](archive/155-round-log.md) §36.2）。
2. **所以"从线上收到的 `pos_y`"不是"不可复现的外来值"**：它是同一套代码用同一套输入
   算出来的数，接收方（与我们的内核）本来就该逐位算得一样。
3. **反证是量化过的**：末位残差率在**战役单机**回放（无远端）是 28.7%（175/609），
   在联机房间回放是 24.4%（81/332）——**有没有远端没有影响**（
   `$PY tools/posy_source_attrib.py --dir <R> --conv conv.jsonl`）。
   ⇒ 第四轮报告 §3 ③ 提出的"那 154 批是别的机器算的"这条假设**证伪**。
4. **真正走了"线上位置"那条路的只有非爆炸特效**：`DeathEffect` / `SparkEffect` / `AshEffect`
   的 `pos` 有 96.8% / 100% / 100% 逐位等于某个实体的 `PosInfo`（点位表），而
   `ExplosionEffect` 只有 4.3%。⇒ 那张点位表**确实**由
   `RemoteEventHandler::handle_server_remote_event` 写入（§33.4 的静态结论），
   但爆炸点不在它上面。

⚠️ 仍然未知（不影响上面四条）：联机房里"跑 `execute_results` 的是房主还是每个客户端各跑一份"
没有回放对能证明（缺一份"同一发、两个客户端各录一次"的回放）。
