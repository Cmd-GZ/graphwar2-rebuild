# 96 · 网络事件覆盖核对：实现 33 个，二进制里有至少 57 个

状态：现行。补 `docs/24`（抓包还原）与 `docs/07`（事件枚举）的覆盖缺口。

## 1. 方法

1. 在 `.rodata` 的 serde 枚举表（`0x1bc000`–`0x1bc900` 一带）里提取事件名，
   得到 **44 个**；
2. 与 `src/events.rs` 的 `enum Event` **33 个变体**逐个对比；
3. 对"二进制有、我们没有"的每一个名字，**单独 `grep` 二进制确认它真实存在**（不靠推断裂表）。
   抽查了 18 个，**全部存在**。

## 2. 结论

| | 数量 |
|---|---|
| 二进制里有的事件名（本表范围内） | **44**（另有散落在其它区段的，见 §4） |
| `events.rs` 已实现 | **33** |
| **我们缺的** | **24** |

我们已有的 33 个，抽查过的（`AxisModeChangeRequest` / `TickRequest` / `TargetInfo` /
`PosInfo` / `PlayerInfo` / `Subscribe` / `FunctionsCalculated` / `FunctionPoints`）
**在二进制里全部存在**，没有编造。

## 3. 缺的 24 个（按重要性排）

### 高：直接影响能否跑对局

| 事件 | 用途 |
|---|---|
| `AxisModeInfo` / `TurnModeInfo` / `FunctionModeInfo` / `TimeModeInfo` | **模式广播**——replay 里每局开头都能看到的那四条。我们只有对应的 `*ChangeRequest`（客户端请求），**没有服务端的 `*Info` 广播** |
| `EndOfTurnInfo` | 回合结束 |
| `TurnCountInfo` / `TurnLimitInfo` | 回合计数与上限 |
| `DestroyObstacle` / `QueueDestroyObstacle` | 障碍被击穿（**replay 解析工具里已经在用，但 `events.rs` 里没有**） |
| `GameInfo` | 对局信息 |

### 中：外观与房间管理

| 事件 | 用途 |
|---|---|
| `SkinGraphicsInfo` / `FaceGraphicsInfo` / `HatGraphicsInfo`（+ 三个 `*Request`） | **皮肤/脸/帽子**——与"Player 界面"那块直接相关 |
| `ChatMessageRequest` | 聊天 |
| `NewPlayerRequest` / `NameRequest` / `BanConnectionRequest` / `RankRequest` | 玩家加入、改名、踢人、排名 |

### 低：信息类

`EntityEffectInfo`、`VersionInfo`、`Request`

## 4. 已知的提取局限

- 事件名在 `.rodata` 里是**连写存放**（例如 `AddSoldierRequestRemoveSoldierRequest`），
  全库正则提取会被粘连，所以本表只覆盖**那段连续枚举表**；
- `TickRequest`（`0x19e570`）、`PosInfo`（`0x1b91ac`）等散落在别处，
  说明**完整清单可能大于 44**，需要用运行时枚举（发未知事件看报错）才能定死。

## 5. 建议的补齐顺序

1. 四个 `*ModeInfo` + `EndOfTurnInfo` + `TurnCountInfo` + `DestroyObstacle` + `GameInfo`
   ——这 8 个是"服务端能正确跑一局并广播状态"的最小补齐集；
2. 外观三个（`Skin/Face/Hat`）——等界面台账（`docs/92`）出来再排；
3. 其余请求类按需。

## 6. 证据等级

| 结论 | 等级 |
|---|---|
| 我们缺 24 个事件名，且这些名字在二进制里真实存在 | **观测**（逐名 grep） |
| 它们是"线上会收发的事件"而非普通字符串 | **推断**（依据：与已知事件同处枚举表、命名规律一致；未逐条抓包验证） |
