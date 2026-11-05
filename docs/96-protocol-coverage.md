# 96 · 网络事件完整清单：73 个名字，73 个已实现

状态：**现行**。本文档取代自己 2026-11 的旧版（旧版只覆盖 `.rodata` 的一小段，
结论是"44 个名字、缺 24 个"）。旧结论的更正记录保留在 §8。

**2026-11-06 更新**：§3 的 23 个缺口**已全部补进 `src/protocol.rs`**（见 §9）。
其中 **13 个**有真实抓包、键名与键序逐位验证；**10 个**字段名未证实，
按"宁可透传、不猜键名"的原则实现成原样透传。

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

字段数走另一条路：`strings` 里直接有 serde 的
`struct <Name>Event with N elements` 原文（`.rodata` 里是**一整段连写**，一次能看全）：

```bash
strings -t d libgraphwarrust.so | grep 'with .* element'
```

## 2. 结论

| | 数量 |
|---|---|
| 二进制里的事件名（全量提取） | **73** |
| `src/` + `tests/` 已实现 | **73** |
| **缺** | **0** |

旧版说"二进制里 44、缺 24"，方向对、数字偏；上一版是 73/50/23，本轮补齐到 73/73/0。
（旧版"缺 24"里有 5 个其实早就实现了，只是当时没按词边界查。）

## 3. 原缺的 23 个：现在怎么实现的

"N（serde 原文）"来自 `.rodata` 的 `<Name>Event with N elements`，**观测**、可信。
"实现"一列是本轮代码形状（`src/protocol.rs` 末节；测试 `protocol::supplemented_event_tests`）。

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

### 3.1 为什么 10 个只做"透传"

透传 = 结构体**不声明任何字段**：解析时把整段 `{…}` 的键与原始值文本按顺序存下，
序列化时原样拼回。它保证：

* 不会写出一条**错误的线格式**（键名错一个字符就是另一条消息）；
* 将来出现的新字段不会丢；
* 但**不能**用来"构造"这些事件——只能解析与转发。

理由见 §5：词汇表能拿到，**逐事件归属拿不到**。`docs/07` §3 里那些"已知字段"
是**相邻字符串**读出来的，不是从该事件的 `visit_map` 读出来的。所以在拿到一次
真实抓包之前，把它们写成结构体字段就是把推断当结论。

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

## 4.5 ⚠️ 两种名字别混：**线上键** 是 CamelCase，**内部分派键** 是 snake_case

`docs/112` 发现 `0x3d38f0` 里被比较的键是 **`name_info_event` / `rank_info_event`** 这种
**snake_case + `_event`** 形态，而真实抓包/replay 里的 JSON 键是 **`NameInfo` / `RankInfo`**。

两套名字都真实存在，但**用途不同**：

| 名字形态 | 出现在哪 | 是不是线格式 |
|---|---|---|
| `NameInfo`（CamelCase） | 抓包、380 份 replay 的 JSON 键 | **是**——`src/protocol.rs` 用的就是这套，已逐行对拍 |
| `name_info_event`（snake_case） | `.rodata` 里被 `0x3d38f0` 比较的键；serde 的字段名表 | **不是**——内部结构体字段名/分派键 |

⇒ **补协议时写 CamelCase**。`docs/112` 提到的那组 snake_case 名字是内部分派层的，
把它当成线上键会写出错误的线格式。

## 4.6 新的线索：判别值**可能**可以解出来（但别急着改 `events.rs`）

`docs/112` 报告：`0x3d38f0` 的**长度跳表 + 两段 16 字节 SIMD 比较 + 常量折叠**
可以逐位解出「事件名 ↔ 数字」20+ 条（`name_info_event`=60、`rank_info_event`=67、
`entity_removed_event`=14、`player_info_event`=13 …），且 `K+16 == len` 全中可自校验。

这条线索**很有价值**，因为它可能终于能验证 `src/events.rs` 那 33 个悬空的判别值。
但**在改 `events.rs` 之前必须先分清一件事**：

> 那些数字是 **`enum Event` 的判别值**，还是**分派表的 handler 下标**？

两者都能解释"名字 ↔ 数字"，但只有前者能拿去改 `events.rs`。目前 `docs/112` 只说
"构造处 + 反查表两处独立命中"，**没有直接证据**区分这两者。

另外注意量级：`0x3d38f0` 给出的数字已经到 **67**，而 `events.rs` 现在只有 **33 个变体**
⇒ 无论那些数字是什么，`events.rs` 的**枚举本身是不完整的**，这一点已可确认。

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

### 5.1 有一条弱规律可以利用（但仍然不是证据）

`struct XEvent with N elements` 这一段串里，**某个变体的字段名如果在这个变体处
"首次出现"**，就会紧挨着排在它前面。它对得上所有**有抓包可验证**的样本：

| 串里的样子 | 抓包实测 | 是否吻合 |
|---|---|---|
| `versionsystem` + `VersionInfoEvent with 2` | `{version,system}` | ✅ |
| `game_address` + `GameRemovedEvent with 1` | —（无抓包） | 未知 |
| `messagestruct ChatMessageEvent with 2` | —（无抓包） | 未知 |
| `turn_limitstruct TurnLimitInfoEvent with 1` | —（无抓包） | 未知 |
| `player_connection_idleader_connection_id` + `BanConnectionRequestEvent with 2` | —（无抓包） | 未知 |
| `corrected_function` + `FunctionInfoEvent with 3` | `{owner_id,function,corrected_function}` | ✅ |
| `active_tick` + `FunctionActiveEvent with 2` | `{owner_id,active_tick}` | ✅ |
| `effect_ideffect_typestart_tick` + `EffectInfoEvent with 5` | 5 个字段里这 3 个 | ✅ |
| `turn_count` + `TurnCountInfoEvent with 1` | `{turn_count}` | ✅ |
| `game_name` + `GameNameInfoEvent with 1` | `{game_name}` | ✅ |

规律成立，但**它只说明"这些名字属于这个变体"**，不说明**没出现的那个字段叫什么**
（serde 对重复字段名只存一份，第二个字段名被去重到别处去了）。
所以 `ChatMessage`（N=2，只看到 `message`）依然没法补全 —— 见 §3.1。

## 6. 建议补齐顺序（本轮已全部做完）

| 优先级 | 事件 | 状态 |
|---|---|---|
| 1 | `NameInfo` / `NameRequest`、`RankInfo` / `RankRequest`、`LockInfo` / `LockRequest` | ✅ 4 强类型 + 2 透传 |
| 2 | `ChatMessage` / `ChatMessageRequest` | ✅ 2 透传（键名未证实） |
| 3 | `Skin` / `Face` / `Hat` 六个 | ✅ 6 强类型 |
| 4 | `GameHeartbeat`、`GameRemoved`、`VersionInfo`、`BotAdded`、`TurnLimitInfo`、`EntityEffectInfo` | ✅ 3 强类型 + 3 透传 |
| 5 | `ClearObstacles`、`ClearBansRequest` | ✅ 2 透传 |
| — | `BanConnectionRequest`（§3 表里有，原优先级表漏了） | ✅ 透传 |

## 7. 证据等级

| 结论 | 等级 |
|---|---|
| 二进制里有这 **73** 个事件名 | **观测**（全量字符串提取，脚本可重跑） |
| `XEvent with N elements` 里的 N = 字段数 | **观测**（serde 原始文案；23 个里 20 个有原文，3 个没有） |
| 我们实现了 73 个（名字层面） | **观测**（词边界 grep `src/`+`tests/` 命中 73/73） |
| 线上走的是外部标签 JSON、无判别值 | **观测**（380 份 replay + `docs/24` 的 TCP 帧） |
| 其中 14 种/41 行的**键名、键序、值的写法** | **逐位验证**（解析→重新序列化，逐字节等于抓包原文） |
| 这 73 个都是"线上会收发的事件" | **推断**——依据是命名规律与已知事件同源 |
| ~~23 个里没有一个抓到过真实流量~~ | **已更正**：错。23 个里 **14 个**在 `fixtures/` 里早有真报文（见 §8） |
| 9 个透传事件的字段名 | **未知**（词汇表有候选，归属未证实；见 §3.1、§5.1） |
| `GameHeartbeat` / `ClearBansRequest` 的字段数 | **未知**（连 serde 的 `with N elements` 串都没有） |
| 这 23 个事件里没有浮点字段 | **观测**（41 行抓包原文里只有整数/字符串/`null`/空对象） |

## 8. 历史更正（旧版与本版上一稿写错了什么）

| 旧断言 | 现状 |
|---|---|
| "二进制里有 44 个事件名" | **73**。旧版只扫了 `0x1bc000`–`0x1bc900` 一段，且正则被粘连吃掉一半 |
| "我们缺 24 个" | 先更正为缺 **23**（上一稿），本轮补齐后缺 **0** |
| §7"这段 `.rodata` 不是判别值表，不要据此改 `events.rs`" | **仍然成立**，但**不再重要**：判别值不上线（§4） |
| "完整清单可能大于 57，需要用运行时枚举才能定死" | 已定到 **73**（静态口径）。要再往上，只能靠运行时抓未知事件 |
| 上一稿 §3 把 `NameInfo` / `LockRequest` / `TurnLimitInfo` 的 N 记成 `—` | **错**。三条 serde 串都在（N=2 / 1 / 1），是提取时漏了。真正没有 N 串的只有 `GameHeartbeat` / `ClearObstacles` / `ClearBansRequest` |
| 上一稿 §7"23 个里没有一个抓到过真实流量" | **错**。`fixtures/protocol.jsonl`（最早的协议抓包提交）里就有 `NameInfo`、`RankInfo`、`Skin/Face/HatGraphicsInfo` 等 **14 种**，共 **41 行**——写上一稿时没按事件名回查夹具 |
| 上一稿 §6 的补齐顺序表漏了 `BanConnectionRequest` | 它也在 §3 的 23 个里，本轮一并实现 |

## 9. 本轮实现（2026-11-06，分支 `agent/protocol-events`）

代码在 `src/protocol.rs` 末尾一节，测试在 `protocol::supplemented_event_tests`
（**33 条**：23 条逐事件往返 + 10 条结构性/边界测试；`cargo test` 由 172 变 **205** 全绿）。

对外 API：

| 项 | 作用 |
|---|---|
| 13 个强类型结构体 | `NameInfo` / `NameRequest` / `RankInfo` / `RankRequest` / `Skin`·`Face`·`Hat` ×2 / `EntityEffectInfo` / `VersionInfo` / `BotAdded`，各有 `parse(&RawMessage)` 与 `to_msg()` |
| 10 个透传结构体 | `LockInfo` / `LockRequest` / `ChatMessage` / `ChatMessageRequest` / `GameHeartbeat` / `GameRemoved` / `TurnLimitInfo` / `ClearObstacles` / `ClearBansRequest` / `BanConnectionRequest`，只有 `extra: Vec<(String,String)>` |
| `AnyEvent` | 按线上键名分派的枚举，`parse` / `kind` / `to_msg` / `extra_fields` |
| `SUPPLEMENTED_EVENT_NAMES` | 23 个键名，测试与本文档对账用 |
| `SERDE_FIELD_COUNTS` | 23 个 N（`None` = `.rodata` 没有这条串，**不是** 0） |
| `CAPTURED_TYPED_EVENTS` | 有抓包且建了强类型的 13 个 |

**所有 23 个事件都支持未知字段原样保留并回发**：强类型以外的字段进 `extra`
（键 + 原始值文本，按出现顺序），`to_msg()` 里原样拼回。

### 9.1 "逐位相同"的适用范围（别读成"已对齐"）

验收口径按 [`06-verification.md`](06-verification.md) §17.1 的三条（**状态 / 消息字节 / 几何**）：
本节只动**消息字节**这一条，而且只覆盖这 23 个事件的**往返**——
状态与几何一概不涉及（它们由 `docs/86`/`docs/91` 等各自负责）。

* **逐位**：14 种 / 41 行真实抓包的**往返**（解析→重新序列化，字节相等）。
  其中 13 种是强类型（键名、键序、整数/`null`/字符串的写法都由抓包定死）；
  `ClearObstacles` 是空体。整数用 `i64::to_string()`，与 serde_json 的整数表示一致；
  23 个事件里没有浮点字段（观测），所以不存在 `1.0` vs `1` 的问题。
* **逐位（元数据）**：23 个事件名与 20 个字段数 N 来自 serde 的原始文案。
* **未对拍**：9 个透传事件的**字段名与字段顺序**没有任何真实流量佐证；
  它们能解析、能原样转发，但**不能**用来构造报文。
* **未验证**：强类型结构体的**键序**是抓包里观察到的顺序；理由是 serde 结构体
  按声明顺序序列化（推断，依据是 41 行抓包全部同序）。若某台服务器的 serde 版本
  改变顺序，往返仍然成立、与新服务器的对拍则未做过。
* **未验证**：这些事件在**玩法规约**上的语义（谁在什么时机发）——本轮只做协议形状。
