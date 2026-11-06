# 07 · 事件协议（联机与玩法状态机）

联机部分整个建立在同一个事件枚举上：客户端与服务端、服务端与 broker、局域网广播，
都在收发 `Event`。本章还原它的**形状**。

证据来源：`.rodata` 里的 serde 字段名表（`0x1bbxxx` / `0x1c5xxx`）与
`struct XxxEvent with N elements` 错误串。这些是**观测**；字段**顺序**、线格式
（bincode / postcard / JSON）与语义仍属**未知**，需反编译 serde 的 `visit_map` 才能定。

## 1. 传输线索（观测）

| 字符串 | 含义推断 |
|---|---|
| `127.0.0.1:8080` | 本地/默认监听地址 |
| `https://graphwar.com/graphwar_II_ip.txt` | 从官网文本拉取全局服务器地址 |
| `ff02::1::` | IPv6 链路本地多播，局域网发现（`graphwar_lan_broadcaster_v6.rs`） |
| `Message too large` / `Message too long` / `Failed to write to tcp_stream.` | TCP + 长度上限 |
| `user://replays/` | 回放落盘路径 |
| `Failed to serialize event.` | 统一序列化入口 |

## 2. 枚举变体顺序（线格式 tag）

字符串表里这些名字**连续排列**，即 Rust `enum Event` 的声明顺序，也是 bincode 类
编码里的判别值顺序：

```
TickRequest, TickReply, Subscribe, NewConnectionRequest, NewConnection,
ConnectionRemoved, ConnectedToServer, NewBotRequest, ChangeTeamRequest,
RemovePlayerRequest, PlayerInfo, EntityRemoved, SoldierInfo, AddSoldierRequest,
RemoveSoldierRequest, AxisModeChangeRequest, FunctionModeChangeRequest,
TurnModeChangeRequest, TimeModeChangeRequest, GameNameRequest, TargetInfo,
RemoveTeam, PosInfo, LifeQueuedDeath, GameNameInfo, GameStateInfo,
FunctionFireRequest, FunctionFire, FunctionUpdateRequest, FunctionInfo,
FunctionActive, FunctionPoints, FunctionsCalculated, ...
```

（尾部还有若干变体，字符串表未连续覆盖，未列全。）

## 3. 事件目录

`N` 来自 `struct XxxEvent with N elements`，是权威字段数；"字段"列是能在相邻字符串里
读到的名字，缺的是被 serde 去重到别处的字段。

### 3.1 开局与房间

| 事件 | N | 已知字段 |
|---|---|---|
| `GameInfo`（结构体） | 9 | `room_name` `game_state` `current_player_count` `address` `axis_mode` `function_mode` `turn_mode` `time_mode` `locked` |
| `GameInfoEvent` | 1 | `game_info` |
| `GameRemovedEvent` | 1 | `game_address` |
| `NewConnectionRequestEvent` | 2 | `major_version` `minor_version` |
| `NewConnectionEvent` | 1 | — |
| `ConnectionRemovedEvent` | 2 | `reason` |
| `ConnectedToServerEvent` | 1 | — |
| `VersionInfoEvent` | 2 | `version` `system` |
| `SubscribeEvent` | 2 | `subscriber_id` `subscribed_event_type` |
| `BanConnectionRequestEvent` | 2 | `player_connection_id` `leader_connection_id` |
| `ClearBansRequestEvent` | ? | — |

### 3.2 玩家、队伍、士兵

| 事件 | N | 已知字段 |
|---|---|---|
| `PlayerInfoEvent` | 2 | — |
| `NewPlayerRequestEvent` | 1 | — |
| `RemovePlayerRequestEvent` | 1 | — |
| `TeamInfoEvent` | 2 | — |
| `RemoveTeamEvent` | 1 | — |
| `ChangeTeamRequestEvent` | 1 | — |
| `ColorInfoEvent` | 2 | `color` |
| `NameInfoEvent` / `NameRequestEvent` | 2 | — |
| `RankInfoEvent` / `RankRequestEvent` | 2 | — |
| `SoldierInfoEvent` | 2 | `soldier_id` |
| `AddSoldierRequestEvent` | 1 | `player_id` |
| `RemoveSoldierRequestEvent` | 1 | — |
| `PosInfoEvent` | 4 | `entity_id` `pos_x` `pos_y` `radius` |
| `LifeInfoEvent` | 2 | `alive` |
| `LifeQueuedDeathEvent` | 2 | — |
| `TargetInfoEvent` | 1 | `target_id` |
| `EntityRemovedEvent` | 1 | `entity_id` |
| `GraphwarColor`（结构体） | 3 | `rgb` |

### 3.3 回合与模式

| 事件 | N | 已知字段 |
|---|---|---|
| `TickEvent` | 1 | `current_tick` |
| `TickRequestEvent` / `TickReplyEvent` | 1 | — |
| `GameHeartbeatEvent` | ? | — |
| `TurnInfoEvent` | 2 | `soldier_id` |
| `TurnCountInfoEvent` | 1 | `turn_count` |
| `TurnLimitInfoEvent` | 1 | `turn_limit` |
| `EndOfTurnInfoEvent` | 2 | — |
| `GameStateInfoEvent` | 1 | `game_state` |
| `TurnModeInfoEvent` / `TurnModeChangeRequestEvent` | 1 | `turn_mode` |
| `TimeModeInfoEvent` / `TimeModeChangeRequestEvent` | 1 | `time_mode` |
| `AxisModeInfoEvent` / `AxisModeChangeRequestEvent` | 1 | `axis_mode` |
| `FunctionModeInfoEvent` / `FunctionModeChangeRequestEvent` | 1 | `function_mode` |
| `GameNameInfoEvent` / `GameNameRequestEvent` | 1 | `game_name` |
| `LockInfoEvent` / `LockRequestEvent` | 1 | — |

### 3.4 开火（本章与 §03/§04/§05 的接缝）

| 事件 | N | 已知字段 |
|---|---|---|
| `FunctionUpdateRequestEvent` | 2 | `event` |
| `FunctionInfoEvent` | 3 | `corrected_function` |
| `FunctionFireRequestEvent` | 1 | — |
| `FunctionFireEvent` | 1 | — |
| `FunctionActiveEvent` | 2 | `active_tick` |
| `FunctionPointsEvent` | **4** | `start_x` `start_y` `end_x`（第 4 个是点数组） |
| `FunctionsCalculatedEvent` | ? | — |

`FunctionPointsEvent` 就是 §05 里逐点还原的那条曲线在网络上的形态：
一个起点 + 一个终点 x + 整条点序列。

### 3.5 障碍、特效、聊天

| 事件 | N | 已知字段 |
|---|---|---|
| `AddObstacleEvent` | 3 | — |
| `DestroyObstacleEvent` | 3 | `pos_x` `pos_y` `radius` |
| `QueueDestroyObstacleEvent` | 4 | — |
| `ClearObstaclesEvent` | ? | — |
| `EffectInfoEvent` | 5 | `effect_id` `effect_type` `start_tick` |
| `EntityEffectInfoEvent` | 2 | `effect_type` |
| `ChatMessageEvent` / `ChatMessageRequestEvent` | 2 | `message` |
| `HatGraphicsInfoEvent` / `SkinGraphicsInfoEvent` / `FaceGraphicsInfoEvent` | 2 | `graphics_str` |

## 4. UI 文本（旁证玩法流程）

`Team 1 Victory!` / `Team 2 Victory!` / `YOU WON` / `YOU DID NOT WIN` —— 胜负是**按队伍**结算的，
与 §3.2 的队伍事件一致。

## 5. 明确未知

- 线格式：字段名存在说明是自描述编码（如 JSON）或仅出现在错误路径；**未证实**。
- 字段**顺序**：表中顺序来自内存布局，不等于 serde 声明顺序。
- `GameInfo.game_state` 的取值集合、`axis_mode` / `turn_mode` / `time_mode` /
  `function_mode` 的枚举值，均未还原。
- 权威端在哪里：`graphwar_game_server.rs` 看起来是房间内权威端，但"谁判定命中/死亡"
  尚未证实。

---

## 6. 事件覆盖：73/73 已实现（原 `96-protocol-coverage`）

> 全文：[`archive/96-protocol-coverage.md`](archive/96-protocol-coverage.md)。


### 6.1 结论

| | 数量 |
|---|---|
| 二进制里的事件名（全量提取） | **73** |
| `src/` + `tests/` 已实现 | **73** |
| **缺** | **0** |

旧版说"二进制里 44、缺 24"，方向对、数字偏；上一版是 73/50/23，本轮补齐到 73/73/0。
（旧版"缺 24"里有 5 个其实早就实现了，只是当时没按词边界查。）

### 6.2 原缺的 23 个：现在怎么实现的

"N（serde 原文）"来自 `.rodata` 的 `<Name>Event with N elements`，**观测**、可信。
"实现"一列是本轮代码形状（`src/_scaffold/protocol.rs` 末节；测试 `_scaffold::protocol::supplemented_event_tests`）。

| 事件 | N（serde 原文） | 实现 | 证据档 |
|---|---|---|---|
| `NameInfo` | 2 | 强类型 `entity_id,name` | **逐位**（夹具 3 行） |
| `NameRequest` | 2 | 强类型 `entity_id,name` | **逐位**（夹具 3 行） |
| `RankInfo` | 2 | 强类型 `entity_id,rank`（`rank` 可为 `null`） | **逐位**（夹具 3 行） |
| `RankRequest` | 2 | 强类型 `entity_id,rank`（`rank` 可为 `null`） | **逐位**（夹具 3 行） |
| `SkinGraphicsInfo` | 2 | 强类型 `entity_id,graphics_str` | **逐位**（夹具 4 行） |
| `SkinGraphicsRequest` | 2 | 强类型 `entity_id,graphics_str` | **逐位**（夹具 3 行） |
| `FaceGraphicsInfo` | 2 | 强类型 `entity_id,graphics_str` | **逐位**（夹具 4 行） |
| `FaceGraphicsRequest` | 2 | 强类型 `entity_id,graphics_str` | **逐位**（夹具 3 行） |
| `HatGraphicsInfo` | 2 | 强类型 `entity_id,graphics_str` | **逐位**（夹具 4 行） |
| `HatGraphicsRequest` | 2 | 强类型 `entity_id,graphics_str` | **逐位**（夹具 3 行） |
| `EntityEffectInfo` | 2 | 强类型 `entity_id,effect_type` | **逐位**（夹具 4 行） |
| `VersionInfo` | 2 | 强类型 `version,system` | **逐位**（夹具 1 行） |
| `BotAdded` | 3 | 强类型 `connection_id,player_id,level` | **逐位**（夹具 2 行） |
| `ClearObstacles` | 无此串 | 透传（实测就是空体 `{}`） | **观测**（夹具 1 行，空体） |
| `LockInfo` | 1 | 透传（词汇表候选 `locked`，未证实） | **推断**（无抓包） |
| `LockRequest` | 1 | 透传（同上） | **推断**（无抓包） |
| `ChatMessage` | 2 | 透传（`message` 有相邻串证据，第二个字段名未知） | **推断**（无抓包） |
| `ChatMessageRequest` | 2 | 透传（同上） | **推断**（无抓包） |
| `GameRemoved` | 1 | 透传（候选 `game_address`，未证实） | **推断**（无抓包） |
| `TurnLimitInfo` | 1 | 透传（候选 `turn_limit`，未证实） | **推断**（无抓包） |
| `BanConnectionRequest` | 2 | 透传（候选 `player_connection_id`,`leader_connection_id`，未证实） | **推断**（无抓包） |
| `GameHeartbeat` | **无此串** | 透传（字段数与字段名都未知） | **未知**（无抓包） |
| `ClearBansRequest` | **无此串** | 透传（字段数与字段名都未知） | **未知**（无抓包） |

汇总：**23 个名字全部能解析、能逐字节回放**；其中 **14 种（41 行）** 有真实抓包，
**13 种**因此建了强类型结构体（键名、键序、值的写法都由抓包定死），
`ClearObstacles` 因为体是空的、没有字段可声明，仍走透传。
**9 种**（`LockInfo`、`LockRequest`、`ChatMessage`、`ChatMessageRequest`、
`GameRemoved`、`TurnLimitInfo`、`BanConnectionRequest`、`GameHeartbeat`、
`ClearBansRequest`）**从未抓到过真实流量**。

#### 6.2.1 为什么 10 个只做"透传"

透传 = 结构体**不声明任何字段**：解析时把整段 `{…}` 的键与原始值文本按顺序存下，
序列化时原样拼回。它保证：

* 不会写出一条**错误的线格式**（键名错一个字符就是另一条消息）；
* 将来出现的新字段不会丢；
* 但**不能**用来"构造"这些事件——只能解析与转发。

理由见 §5：词汇表能拿到，**逐事件归属拿不到**。`07-protocol.md` §3 里那些"已知字段"
是**相邻字符串**读出来的，不是从该事件的 `visit_map` 读出来的。所以在拿到一次
真实抓包之前，把它们写成结构体字段就是把推断当结论。


### 6.3 两种名字别混：线上键是 CamelCase，内部分派键是 snake_case


`02-module-map.md` 发现 `0x3d38f0` 里被比较的键是 **`name_info_event` / `rank_info_event`** 这种
**snake_case + `_event`** 形态，而真实抓包/replay 里的 JSON 键是 **`NameInfo` / `RankInfo`**。

两套名字都真实存在，但**用途不同**：

| 名字形态 | 出现在哪 | 是不是线格式 |
|---|---|---|
| `NameInfo`（CamelCase） | 抓包、380 份 replay 的 JSON 键 | **是**——`src/_scaffold/protocol.rs` 用的就是这套，已逐行对拍 |
| `name_info_event`（snake_case） | `.rodata` 里被 `0x3d38f0` 比较的键；serde 的字段名表 | **不是**——内部结构体字段名/分派键 |

⇒ **补协议时写 CamelCase**。`02-module-map.md` 提到的那组 snake_case 名字是内部分派层的，
把它当成线上键会写出错误的线格式。


### 6.4 判别值已解全（72 条），`events.rs` 已改


> 本节原先写的是"别急着改，先分清 enum 判别值 vs handler 下标"。
> **现在已经分清了，结论是 enum 判别值，已经改了。** 下面是收口的记录。

`07-protocol.md` 把 `0x3d38f0` 那张表解全：**72 条**（覆盖 0..=73，缺 63/65），四道自校验全过
（字节区间铺满 `[0,len)` 72/72、`0x8000…|N` 写进 tag word 17/17、名字在 `.rodata` 找到 69/72、
`0x1bf537` 的排序数组严格递增 32/32）。

**"enum 判别值"的两条硬证据**：
1. 同一个 N 被当成 `0x8000000000000000|N` 写进 `Event` 的 `+0`；
2. 公共尾 `0x3d4c5c` 拿**裸 N** 当 `HashMap<u64,_>`(entry `0x38`) 的 key 走 hashbrown 查表
   ——**不存在**按 N 索引的函数指针数组（handler 下标该有的形状不存在）。

⇒ `src/events.rs` 已从 **33 → 72** 个变体，判别值换成实测值。旧的"顺序 = `.rodata`
变体名表位置"假设**已被就地证伪**（`PlayerInfo` 10→**13**、`FunctionPoints` 31→**37**，
差值 +1..+6 不是常数，那段串缺 5 个变体）。

**线格式仍然一字未动**（判别值不上线，见 §4）。

下面保留原记录，作为"当时为什么不敢改"的档案：

`02-module-map.md` 报告：`0x3d38f0` 的**长度跳表 + 两段 16 字节 SIMD 比较 + 常量折叠**
可以逐位解出「事件名 ↔ 数字」20+ 条（`name_info_event`=60、`rank_info_event`=67、
`entity_removed_event`=14、`player_info_event`=13 …），且 `K+16 == len` 全中可自校验。

这条线索**很有价值**，因为它可能终于能验证 `src/events.rs` 那 33 个悬空的判别值。
但**在改 `events.rs` 之前必须先分清一件事**：

> 那些数字是 **`enum Event` 的判别值**，还是**分派表的 handler 下标**？

两者都能解释"名字 ↔ 数字"，但只有前者能拿去改 `events.rs`。目前 `02-module-map.md` 只说
"构造处 + 反查表两处独立命中"，**没有直接证据**区分这两者。

另外注意量级：`0x3d38f0` 给出的数字已经到 **67**，而 `events.rs` 现在只有 **33 个变体**
⇒ 无论那些数字是什么，`events.rs` 的**枚举本身是不完整的**，这一点已可确认。


## 7. 事件名 ↔ 判别值全表（72 条，原 `114-event-tags`）

> 全文（含三道自校验的逐条字节证据）：[`archive/114-event-tags.md`](02-module-map.md)。

### 7.1 一页结论

| 问题 | 答案 | 等级 |
|---|---|---|
| 解出多少条 | **72 条**「snake_case 名字 → 数字」，覆盖 `0..=73` 除 `63`/`65` 外的全部整数值 | **逐位验证** |
| 自校验 ①（长度）| 每条相等测试的字节区间并集**恰好铺满** `[0, len)`，无空洞无冲突 → **72/72** | **逐位验证** |
| 自校验 ②（tag word）| 17 条分支会顺手拼 `Event` 前缀，`+0` 处的 `0x8000000000000000 \| N` 与 `%ebx` **完全相等** → **17/17** | **逐位验证** |
| 自校验 ③（`.rodata`）| 72 条里 **69 条**能在 `.rodata` 里逐字节找到；剩 3 条是 LLVM 用两条已有串的**重叠片段**拼出来的（§4.3） | **观测** |
| 自校验 ④（排序数组）| `.rodata` `0x1bf537` 那 32 条名字按本次解出的判别值**严格递增**（13,14,15,19,23,…60）→ **32/32**（§4.4） | **观测** |
| 那些数字是什么 | **`enum Event` 的判别值**（tag word 的低位）。**不是** handler 下标 | **逐位验证** |
| `src/events.rs` 旧编号 | **错**。不是"差一个常数"，`PlayerInfo` 10→**13**、`FunctionPoints` 31→**37** | **逐位验证** |
| `enum Event` 有多大 | **≥ 75 个变体**（`0..=73` 之外 74 也被用过）。具名 **72** 个 | 推断（缺：63/65/74 的分派分支或构造点） |
| 线格式 | **完全没变**：线上仍是外部标签 JSON，判别值不上线（`07-protocol.md` §4，380 份 replay） | **观测**（沿用上游结论） |

改动落地：`src/events.rs`（枚举重写 = 72 变体 + 实测值 + 名字对照表）、
`tests/events.rs`（新增「名字 → 判别值」全表钉死）、本文、`tools/event_tags.py`（复现脚本）。
**`cargo test` 205 → 206 全绿**。

---

### 7.2 全表（72 条，按判别值升序）

「相等测试」= 拿名字跟常量比的那条指令；「判别值@」= `mov $N,%ebx` 的地址；
「tag」= 该分支顺手拼出的 `Event` 前缀里的 tag word（只有 17 条有）。

| # | 名字 | len | 分支 | 相等测试 | 判别值@ | tag |
|---|---|---|---|---|---|---|
| 0 | `tick_event` | 10 | `0x3d392a` | `0x3d392a` | `0x3d394b` | — |
| 1 | `tick_request_event` | 18 | `0x3d3c7e` | `0x3d3c7e` | `0x3d4784` | — |
| 2 | `tick_reply_event` | 16 | `0x3d3f9c` | `0x3d3f9c` | `0x3d4798` | — |
| 3 | `subscribe_event` | 15 | `0x3d3fd1` | `0x3d3fd1` | `0x3d47df` | `0x8000…\|03` |
| 4 | `new_connection_request_event` | 28 | `0x3d43a0` | `0x3d43a0` | `0x3d484f` | `0x8000…\|04` |
| 5 | `new_connection_event` | 20 | `0x3d4184` | `0x3d4184` | `0x3d47f3` | — |
| 6 | `connection_removed_event` | 24 | `0x3d3e3d` | `0x3d3e3d` | `0x3d478e` | — |
| 7 | `connected_to_server_event` | 25 | `0x3d3bcd` | `0x3d3bcd` | `0x3d477a` | — |
| 8 | `new_player_request_event` | 24 | `0x3d3e3d` | `0x3d3e69` | `0x3d48e6` | — |
| 9 | `new_bot_request_event` | 21 | `0x3d44c4` | `0x3d44c4` | `0x3d4863` | — |
| 10 | `bot_added_event` | 15 | `0x3d3fd1` | `0x3d3ffc` | `0x3d48f0` | — |
| 11 | `change_team_request_event` | 25 | `0x3d3bcd` | `0x3d3bf9` | `0x3d48d2` | — |
| 12 | `remove_player_request_event` | 27 | `0x3d428b` | `0x3d428b` | `0x3d47fd` | — |
| 13 | `player_info_event` | 17 | `0x3d3b12` | `0x3d3b12` | `0x3d4770` | — |
| 14 | `entity_removed_event` | 20 | `0x3d4184` | `0x3d41b0` | `0x3d48fa` | — |
| 15 | `soldier_info_event` | 18 | `0x3d3c7e` | `0x3d3cad` | `0x3d48dc` | — |
| 16 | `add_soldier_request_event` | 25 | `0x3d3bcd` | `0x3d3c25` | `0x3d3c46` | — |
| 17 | `remove_soldier_request_event` | 28 | `0x3d43a0` | `0x3d43cc` | `0x3d490e` | — |
| 18 | `axis_mode_change_request_event` | 30 | `0x3d45f7` | `0x3d45f7` | `0x3d486d` | — |
| 19 | `function_mode_change_request_event` | 34 | `0x3d3952` | `0x3d3952` | `0x3d396a` | — |
| 20 | `turn_mode_change_request_event` | 30 | `0x3d45f7` | `0x3d4623` | `0x3d4922` | — |
| 21 | `time_mode_change_request_event` | 30 | `0x3d45f7` | `0x3d466d` | `0x3d468a` | — |
| 22 | `game_name_request_event` | 23 | `0x3d3a47` | `0x3d3a47` | `0x3d4766` | `0x8000…\|16` |
| 23 | `target_info_event` | 17 | `0x3d3b12` | `0x3d3b41` | `0x3d48c8` | — |
| 24 | `team_info_event` | 15 | `0x3d3fd1` | `0x3d401d` | `0x3d494a` | — |
| 25 | `remove_team_event` | 17 | `0x3d3b12` | `0x3d3b70` | `0x3d492c` | — |
| 26 | `pos_info_event` | 14 | `0x3d3c50` | `0x3d3c50` | `0x3d3c6b` | — |
| 27 | `life_info_event` | 15 | `0x3d3fd1` | `0x3d4041` | `0x3d4a3f` | — |
| 28 | `life_queued_death_event` | 23 | `0x3d3a47` | `0x3d3a73` | `0x3d48be` | — |
| 29 | `game_name_info_event` | 20 | `0x3d4184` | `0x3d41dc` | `0x3d4989` | `0x8000…\|1d` |
| 30 | `game_state_info_event` | 21 | `0x3d44c4` | `0x3d44f0` | `0x3d4918` | — |
| 31 | `game_start_request_event` | 24 | `0x3d3e3d` | `0x3d3e95` | `0x3d4940` | — |
| 32 | `function_fire_request_event` | 27 | `0x3d428b` | `0x3d42b7` | `0x3d4904` | — |
| 33 | `function_fire_event` | 19 | `0x3d40e0` | `0x3d40e0` | `0x3d47e9` | — |
| 34 | `function_update_request_event` | 29 | `0x3d46b8` | `0x3d46b8` | `0x3d471d` | `0x8000…\|22` |
| 35 | `function_info_event` | 19 | `0x3d40e0` | `0x3d410c` | `0x3d417a` | `0x8000…\|23` |
| 36 | `function_active_event` | 21 | `0x3d44c4` | `0x3d451c` | `0x3d49e4` | — |
| 37 | `function_points_event` | 21 | `0x3d44c4` | `0x3d4548` | `0x3d4a53` | — |
| 38 | `functions_calculated_event` | 26 | `0x3d397c` | `0x3d397c` | `0x3d4727` | — |
| 39 | `clear_obstacles_event` | 21 | `0x3d44c4` | `0x3d4574` | `0x3d4b17` | — |
| 40 | `add_obstacle_event` | 18 | `0x3d3c7e` | `0x3d3cdc` | `0x3d4936` | — |
| 41 | `destroy_obstacle_event` | 22 | `0x3d446d` | `0x3d446d` | `0x3d4859` | — |
| 42 | `queue_destroy_obstacle_event` | 28 | `0x3d43a0` | `0x3d43f8` | `0x3d49da` | — |
| 43 | `turn_info_event` | 15 | `0x3d3fd1` | `0x3d4057` | `0x3d4b03` | — |
| 44 | `effect_info_event` | 17 | `0x3d3b12` | `0x3d3b9f` | `0x3d3bc3` | — |
| 45 | `axis_mode_info_event` | 20 | `0x3d4184` | `0x3d4208` | `0x3d4a49` | — |
| 46 | `function_mode_info_event` | 24 | `0x3d3e3d` | `0x3d3ec1` | `0x3d4a35` | — |
| 47 | `turn_mode_info_event` | 20 | `0x3d4184` | `0x3d4234` | `0x3d4b0d` | — |
| 48 | `time_mode_info_event` | 20 | `0x3d4184` | `0x3d4260` | `0x3d4281` | — |
| 49 | `end_of_turn_info_event` | 22 | `0x3d446d` | `0x3d4499` | `0x3d44ba` | — |
| 50 | `skin_graphics_request_event` | 27 | `0x3d428b` | `0x3d42e3` | `0x3d49d0` | `0x8000…\|32` |
| 51 | `skin_graphics_info_event` | 24 | `0x3d3e3d` | `0x3d3eed` | `0x3d4af9` | `0x8000…\|33` |
| 52 | `face_graphics_request_event` | 27 | `0x3d428b` | `0x3d430f` | `0x3d4396` | `0x8000…\|34` |
| 53 | `face_graphics_info_event` | 24 | `0x3d3e3d` | `0x3d3f19` | `0x3d4b68` | `0x8000…\|35` |
| 54 | `hat_graphics_request_event` | 26 | `0x3d397c` | `0x3d39a8` | `0x3d48b4` | `0x8000…\|36` |
| 55 | `hat_graphics_info_event` | 23 | `0x3d3a47` | `0x3d3a9f` | `0x3d3b08` | `0x8000…\|37` |
| 56 | `entity_effect_info_event` | 24 | `0x3d3e3d` | `0x3d3f45` | `0x3d4bcd` | — |
| 57 | `turn_limit_info_event` | 21 | `0x3d44c4` | `0x3d45a0` | `0x3d4bb9` | — |
| 58 | `turn_count_info_event` | 21 | `0x3d44c4` | `0x3d45cc` | `0x3d45ed` | — |
| 59 | `name_request_event` | 18 | `0x3d3c7e` | `0x3d3d0b` | `0x3d4a2b` | `0x8000…\|3b` |
| 60 | `name_info_event` | 15 | `0x3d3fd1` | `0x3d4071` | `0x3d4baf` | `0x8000…\|3c` |
| 61 | `color_info_event` | 16 | `0x3d3f9c` | `0x3d3fb7` | `0x3d3fc7` | — |
| 62 | `game_info_event` | 15 | `0x3d3fd1` | `0x3d408b` | `0x3d4c50` | — |
| **63** | *缺号* | — | — | — | — | — |
| 64 | `game_removed_event` | 18 | `0x3d3c7e` | `0x3d3d3a` | `0x3d4ab2` | `0x8000…\|40` |
| **65** | *缺号* | — | — | — | — | — |
| 66 | `rank_request_event` | 18 | `0x3d3c7e` | `0x3d3d69` | `0x3d4b21` | — |
| 67 | `rank_info_event` | 15 | `0x3d3fd1` | `0x3d40a5` | `0x3d4c57` | — |
| 68 | `lock_request_event` | 18 | `0x3d3c7e` | `0x3d3d98` | `0x3d4bc3` | — |
| 69 | `lock_info_event` | 15 | `0x3d3fd1` | `0x3d40bf` | `0x3d40cd` | — |
| 70 | `chat_message_request_event` | 26 | `0x3d397c` | `0x3d39d4` | `0x3d3a3d` | `0x8000…\|46` |
| 71 | `chat_message_event` | 18 | `0x3d3c7e` | `0x3d3dc7` | `0x3d3e33` | `0x8000…\|47` |
| 72 | `ban_connection_request_event` | 28 | `0x3d43a0` | `0x3d4424` | `0x3d4463` | — |
| 73 | `clear_bans_request_event` | 24 | `0x3d3e3d` | `0x3d3f71` | `0x3d3f92` | — |

`0x3d4c50` 那条（`game_info_event`）的 `mov $0x3e,%ebx` 是**最后**一条，
它前面还写了一个 `movb $0x7,0x41(%rsp)`——`GameInfo` 的 9 个字段，与
`GAME_INFO_FIELD_COUNT = 9` 相容。〔观测〕

#### 7.2.1 与 `02-module-map.md` §10.1 的对账

- `02-module-map.md` 解出的 **22 条，22 条全对**（包括 `name_info_event`=60、`rank_info_event`=67）。
- 新增 **50 条**。
- `02-module-map.md` 留下的 4 条悬案，**3 条定死、1 条纠正**：
  - `team_info_event` = **24**（`02-module-map.md` 的 24/27 之争 → 24 对，27 是相邻 arm 的 `mov ebx`）；
  - `color_info_event` = **61**（`02-module-map.md` 猜 50，**错**：50 是 `skin_graphics_request_event`）；
  - `len=16` 分支里 `color_info_event` 与 `skin_graphics_request_event` **本来就是两条**，
    长度不同（16 vs 27），`02-module-map.md` 之所以混淆是因为把 50 记到了 16 字节的分支上。
- 顺带把 `02-module-map.md` §9.2 里 5 个"疑似"的请求事件全部证实：
  **59 `name_request_event`、50 `skin_graphics_request_event`、52 `face_graphics_request_event`、
  54 `hat_graphics_request_event`、66 `rank_request_event`**。
- 也把 `02-module-map.md` §13 未确定项 4 的「tag 7 / tag 8 是哪个事件」解掉：
  **7 = `connected_to_server_event`**、**8 = `new_player_request_event`**
  （`0x51a570` 的逻辑因此读得通了：被告知"已连上服务器"→ 记下自己的 connection id →
  发一条 `new_player_request`。**语义这一层仍是推断**，但两个 tag 值是逐位）。

---


### 7.3 那些数字是什么：`enum Event` 的判别值（不是 handler 下标）


`07-protocol.md` §4.6 的护栏：只有能证明是判别值才能改 `events.rs`。下面按"用途 / 取值范围 /
交叉核对"三条走，前两条是**直接证据**。

#### 7.3.1 判据 ①（用途，硬证据）：同一个 `N` 被写成 `Event` 的 tag word

`02-module-map.md` §3 已经**逐位**确立：`Event` 是 0x48 字节，`+0` 是
`0x8000000000000000 | discriminant` 的 tag word，消费者用**整 qword** 比较
（例：`0x51a581` `cmp qword [rsi], 0x8000000000000007`；`0x519030` 的
`lea rax,[r15+0xe]` = `0x8000…0e`）。

`0x3d38f0` 里 17 条分支把**同一个 `N`** 写成这个 tag word（§4.2）。
一个"handler 下标"不会以 `0x8000…|N` 的形态被放进 `Event` 的判别值位置——
除非"下标恰好等于判别值"对 17 条同时成立，而那样两个概念本来就没有区别。
⇒ **`N` 就是 `Event` 的判别值**。〔逐位〕

#### 7.3.2 判据 ②（用途，硬证据）：`N` 是订阅者表的 key，走的是 hash 查表不是数组下标

公共尾 `0x3d4c5c`：

```
3d4c5c  mov  %rbx,(%rsp)              ; key = N（裸判别值）
3d4c60  lea  0x28(%r15),%rbp          ; &self.map
3d4c6a  call 0x47e5f0                 ; hashbrown 查表
3d4cc6  imul $0x38,%r8,%r8            ; entry 步长 0x38 = 56 字节
3d4cca  cmp  %rbx,-0x38(%rax,%r8,1)   ; 比的是 key 本身
```

- 如果是"handler 下标"，形状会是 `mov rax,[table + N*8]; call rax`（**密集数组**）。
  实际是 `HashMap<u64, …>`（entry 56 B）——**没有任何按 `N` 索引的函数指针表**。
- 同一形状的 map 在 `0x3d3630` 也出现；那里拼了一个常量 key `0x800000000000004a`
  （`0x3d3656` `movabs $0x8000000000000000,%rdx` + `0x3d3660` `add $0x4a,%rdx` +
  `0x3d3664` `mov %rdx,(%rsp)`），即 **tag 74**。〔逐位：这两个字节〕
  `02-module-map.md` §10.2 把这个常量判为 `HashMap<u64, 订阅者集合>` 的 key；本文**复核了常量本身**，
  但没有重新推导 `0x3d3630` 后半段的语义（它的比较用的是 `%r14`，由 `0x3d3821`
  的 `xor $0x8000…` 重新算出来，作者没有跟到底）。
  ⇒ 「判别值池至少到 74」**推断**（缺：一个 `Event` 构造点或一个 `cmp qword […] 0x8000…|0x4a` 消费者）。

#### 7.3.3 判据 ③（取值范围）：`0..=73` 且**近乎稠密**

| 观察 | 值 |
|---|---|
| 取值集合 | `{0..=73} \ {63,65}`，共 72 个 |
| 单调 | 与 `.rodata` 里那段「按判别值排序」的名字串（`0x1bf537`，§4.4）**完全同序** |
| 缺号 | 只有 63、65；且全 `.text` 扫 `0x8000…\|63` / `…\|65` 的三种编码形态**一次都没有** |

枚举判别值天然是"声明序号"（这里因为 `0x8000…` 基址而写成 `0x8000… + 声明序号`），
所以"成组相邻 + 只缺两个号"完全符合；而 handler 下标没有理由这样成组
（`skin_graphics_request/info` 50/51、`face_*` 52/53、`hat_*` 54/55、
`rank_request/info` 66/67、`lock_*` 68/69、`chat_*` 70/71、`ban/clear_bans` 72/73
——成对相邻，正是枚举按作者手写顺序编号的样子）。〔逐位〕

#### 7.3.4 判据 ④（交叉核对）：**构造侧**与**名字侧**独立命中同一批数

| 构造点 | 写进 tag word 的常量 | 本次解出的名字 | 一致？ |
|---|---|---|---|
| `0x519350`（`name_manager::set_name`） | `0x8000…3c` = 60 | `name_info_event` = 60 | ✅ |
| `0x518f10`（广播全部名字） | `0x8000…3c` = 60 | 同上 | ✅ |
| `0x519030` / `0x454e80`（处理移除） | `0x8000…0e` = 14 | `entity_removed_event` = 14 | ✅ |
| `0x454d80` / `0x455090`（`rank_manager`） | `0x8000…43` = 67 | `rank_info_event` = 67 | ✅ |
| `0x519f40`（过滤 `player_info_event`） | `0x8000…0d` = 13 | `player_info_event` = 13 | ✅ |
| `0x455e30`（`function_manager::set`） | 35 | `function_info_event` = 35 | ✅ |
| `0x51a570` 处理 tag 7、发 tag 8 | 7 / 8 | `connected_to_server_event` / `new_player_request_event` | ✅ |

⇒ 名字侧（本文）与构造侧（`02-module-map.md` 已逐位验证的常量）**两套独立证据相交**，
交集上 7/7 一致。这是 `02-module-map.md` 说的"构造处 + 反查表两处独立命中"的**完整版**。〔逐位〕

#### 7.3.5 结论

**`0x3d38f0` 返回/写入的数字是 `enum Event` 的判别值，不是 handler 下标。**
⇒ 可以、并且必须据此改 `src/events.rs`。

---


## 8. 一回合的报文顺序（原 `43` / `46`）

> 全文：[`archive/43-turn-message-sequence.md`](archive/43-turn-message-sequence.md)、[`archive/46-full-turn.md`](archive/46-full-turn.md)。

### 8.1 抓包观测（真实对局，一个「被阻挡、没死人」的回合）

```text
FunctionInfo        (客户端提交的函数，修正前)
FunctionFire
GameStateInfo       CalculatingResults
FunctionInfo        (修正后，带 corrected_function 文本)
FunctionPoints
GameStateInfo       SynchronizingResults
QueueDestroyObstacle
FunctionActive
EffectInfo          ExplosionEffect
GameStateInfo       ExecutingResults
…（若干 TickReply）…
DestroyObstacle
…（若干 TickReply）…
TurnCountInfo
```

要点：

- 状态推进**夹在报文中间**：`CalculatingResults` 在函数提交之后、
  `SynchronizingResults` 在 `FunctionPoints` 之后、`ExecutingResults` 在爆炸之后；
- `QueueDestroyObstacle` 早于 `FunctionActive`；
- `DestroyObstacle` 晚于 `ExecutingResults`（要等若干 tick 之后才真正删除）；
- 有死亡的回合里，`LifeQueuedDeath` + `EffectInfo(DeathEffect)` 出现在
  `SynchronizingResults` 之后、`FunctionActive` 之前。

### 8.2 序列

```text
TurnInfo{player_id, soldier_id}            ← 轮到谁
EndOfTurnInfo{end_of_turn_tick, time_mode} ← 回合截止
GameStateInfo(WaitingForFunctions)         ← 等待提交

GameStateInfo(CalculatingResults)          ← 收到开火
FunctionInfo(修正后)
FunctionPoints{owner_id, start_x, start_y, end_x}
GameStateInfo(SynchronizingResults)
QueueDestroyObstacle{pos_x, pos_y, radius:0.5, tick}
FunctionActive{owner_id, active_tick}
EffectInfo(ExplosionEffect)
GameStateInfo(ExecutingResults)

TurnCountInfo{turn_count}                  ← 回合结束
（若某队全灭，紧跟 EndingGame）
```


## 9. 由结算生成协议事件（原 `31-event-generation`）

> 全文：[`archive/31-event-generation.md`](archive/31-event-generation.md)。

### 9.1 事件顺序（观测依据）

真实抓包里一次"打死人"的开火，去掉状态机消息（`GameStateInfo`、`TickReply`、
`FunctionActive`）后，顺序是：

```text
FunctionPoints
LifeQueuedDeath
EffectInfo(effect_type = DeathEffect)
EffectInfo(effect_type = ExplosionEffect)
```

若弹道**被障碍挡住**，还会有一条 `QueueDestroyObstacle`（位置 = 停止点）。

