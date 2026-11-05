# 96 · 网络事件完整清单：73 个名字，实现 50 个

状态：**现行**。本文档取代自己 2026-11 的旧版（旧版只覆盖 `.rodata` 的一小段，
结论是"44 个名字、缺 24 个"）。旧结论的更正记录保留在 §8。

## 1. 方法（可复现）

事件名在 `.rodata` 里是**连写**的，而且前面粘着 `struct`、后面也粘着 `struct`：

```
...LockInfoEvent with 1 elementstruct NameInfoEvent with 2 elementsstruct RankInfo...
```

所以直接正则 `\b[A-Za-z]+Event\b` 会漏掉一大半（`GameHeartbeatEvent` 会变成
`GameHeartbeat` + `Eventstruct`）。四步：

```bash
strings -n 4 libgraphwarrust.so > .rea-work/strings_all.txt
```

1. `Event(?=struct|variant)` → `Event `（把后缀粘连切开）；
2. CamelCase 分词，取以 `Event` 结尾的 token（这样 `XXXEventYYY` 不会被整段吞掉）；
3. 去掉 `InputEvent`（Godot 自己的类）和 4 个**粘连残留**（见下）；
4. 在 `src/` + `tests/` 里按**词边界**查找（`NameInfo` 不能命中 `GameNameInfo`）。

剔除的 4 个粘连残留（都是两个真事件名黏在一起，不是新事件）：

`BanConnectionReqNameRequestEvent`、`NewConnectionReqLockRequestEvent`、
`RemovePlayerRequSoldierInfoEvent`、`UpdateRequestGameRemovedEvent`

⇒ **73 个不同事件名**。这个数字是**全二进制**口径，不再只是某一段 `.rodata`。

## 2. 结论

| | 数量 |
|---|---|
| 二进制里的事件名（全量提取） | **73** |
| `src/` + `tests/` 已实现 | **50** |
| **缺** | **23** |

旧版说"二进制里 44、缺 24"，方向对、数字偏；现在是有据可查的 73/50/23。
（旧版"缺 24"里有 5 个其实早就实现了，只是当时没按词边界查。）

## 3. 缺的 23 个

"字段数"来自 `.rodata` 里 serde 的 `<Name>Event with N elements`，**可信**；
`—` = 没找到这条字符串，字段数未知。

| 事件 | 字段数 |
|---|---|
| `BanConnectionRequest` | 2 |
| `BotAdded` | 3 |
| `ChatMessage` | 2 |
| `ChatMessageRequest` | 2 |
| `ClearBansRequest` | — |
| `ClearObstacles` | — |
| `EntityEffectInfo` | 2 |
| `FaceGraphicsInfo` | 2 |
| `FaceGraphicsRequest` | 2 |
| `GameHeartbeat` | — |
| `GameRemoved` | 1 |
| `HatGraphicsInfo` | 2 |
| `HatGraphicsRequest` | 2 |
| `LockInfo` | 1 |
| `LockRequest` | — |
| `NameInfo` | — |
| `NameRequest` | 2 |
| `RankInfo` | 2 |
| `RankRequest` | 2 |
| `SkinGraphicsInfo` | 2 |
| `SkinGraphicsRequest` | 2 |
| `TurnLimitInfo` | — |
| `VersionInfo` | 2 |

其中 **`NameInfo` / `NameRequest` / `RankInfo` / `RankRequest` / `LockInfo` / `LockRequest`**
是一组：改名、排名、锁房。`Skin/Face/Hat` 六个是外观。`ChatMessage*` 两个是聊天
（`docs/106` 记了界面上有聊天按钮，但协议这边一直没实现）。

## 4. 判别值根本不上线（这条把旧版的顾虑解掉了）

replay 里有 **380 份**真对局录像（`user://replays/`，gzip + JSONL），每一行长这样：

```json
{"event":{"LifeInfo":{"entity_id":3,"alive":false}},"tick":42}
```

事件被写成 **`{变体名: 字段}`** —— 这是 serde 的**外部标签**枚举，名字就是键，
**没有任何数值判别值**。TCP 传输是 `u16` 大端长度 + 同一份 JSON（`docs/24`）。

⇒ 三条推论，都要写清楚：

1. **`enum Event` 的数值判别值不参与协议**，它既不出现在线路上，也不出现在 replay 里；
2. 因此"补一个事件"只需要**名字 + 字段**，**不需要**知道它在 Rust 里的编号。
   旧版 §7 那句"别用这 44 个名字去增删变体"的顾虑，对**协议正确性**不成立；
3. 反过来说，`src/events.rs` 里那 33 个 `= N` 至今仍**没有独立验证**，
   但它现在被降级为"内部注记"——**不影响任何可观测行为**。
   不要再花时间去找它的判别表。

## 5. 字段名：能拿到词汇表，拿不到逐事件归属

- **能拿到**：`.rodata` 里有一大段 snake_case 字段名（`soldier_id`、`corrected_function`、
  `subscribed_event_type`、`game_address`…），提取脚本见 §1。
- **拿不到**：字段名与结构体的**归属**。字符串里字段名紧挨着 `struct XEvent with N elements`，
  但那是内存布局的巧合，不是语义顺序。我试过"取 `with N elements` 前面的 N 个小写
  token"：16 个"解出"，绝大多数是垃圾（`s`、`element`），
  只有 `GameInfo` 的 9 个字段（`room_name game_state current_player_count address
  axis_mode function_mode turn_mode time_mode locked`）明显对得上——那个已经写进
  `src/events.rs` 了。
- ⇒ **不要把 §5 的词汇表硬塞进结构体。** 要定字段，得靠 decompile 或运行时抓包。

## 6. 建议补齐顺序

| 优先级 | 事件 | 理由 |
|---|---|---|
| 1 | `NameInfo` / `NameRequest`、`RankInfo` / `RankRequest`、`LockInfo` / `LockRequest` | 一局联机里房主锁房、玩家改名、显示段位都走这条；缺了要么表现为"界面卡住"，要么被当成未知事件忽略 |
| 2 | `ChatMessage` / `ChatMessageRequest` | 界面上有聊天（`docs/106` #6），协议里没有 |
| 3 | `Skin/Face/Hat` 六个 | 外观选择；等客户端界面落地再排 |
| 4 | `GameHeartbeat`、`GameRemoved`、`VersionInfo`、`BotAdded`、`TurnLimitInfo`、`EntityEffectInfo` | 心跳/房间消失/版本握手/加机器人 |
| 5 | `ClearObstacles`、`ClearBansRequest` | 管理类，单机跑不到 |

## 7. 证据等级

| 结论 | 等级 |
|---|---|
| 二进制里有这 **73** 个事件名 | **观测**（全量字符串提取，脚本可重跑） |
| `XEvent with N elements` 里的 N = 字段数 | **观测**（serde 原始文案） |
| 我们实现了 50 个 | **观测**（词边界 grep `src/`+`tests/`） |
| 线上走的是外部标签 JSON、无判别值 | **观测**（380 份 replay + `docs/24` 的 TCP 帧） |
| 这 73 个都是"线上会收发的事件" | **推断**——依据是命名规律与已知事件同源；23 个里没有一个抓到过真实流量 |

## 8. 历史更正（旧版写错了什么）

| 旧断言 | 现状 |
|---|---|
| "二进制里有 44 个事件名" | **73**。旧版只扫了 `0x1bc000`–`0x1bc900` 一段，且正则被粘连吃掉一半 |
| "我们缺 24 个" | **缺 23 个**，而且是另一个集合（旧版把 5 个已实现的算成缺，又漏了 `NameInfo` 等） |
| §7"这段 `.rodata` 不是判别值表，不要据此改 `events.rs`" | **仍然成立**，但**不再重要**：判别值不上线（§4）。补事件只看名字和字段 |
| "完整清单可能大于 57，需要用运行时枚举才能定死" | 已定到 **73**（静态口径）。要再往上，只能靠运行时抓未知事件 |
