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
