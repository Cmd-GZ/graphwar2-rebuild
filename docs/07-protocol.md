# 07 · 事件协议（联机与玩法状态机）

联机部分整个建立在同一个事件枚举上：客户端与服务端、服务端与 broker、局域网广播，
都在收发 `Event`。本章还原它的**形状**。

> **本轮已把字段名与字段顺序定死。** 办法不是猜、也不是 `strings`：
> `#[derive(Serialize)]` 生成的代码**就是**线格式的定义 —— `serialize_field("键名", 值)`
> 的**调用顺序就是线上键序**，键名是 `.rodata` 里的 `&'static str`（`lea rsi,[rip+X]` +
> `mov edx,<len>`，**不以 NUL 结尾**，所以 `strings` 会把相邻字面量粘成一条、搜不到）。
> 复现：`../.venv/bin/python tools/wire_fields.py --verify`；
> 验收：`tests/wire_format.rs`。详见 §3。

证据来源：`.rodata` 里的 serde 字段名表（`0x1bbxxx` / `0x1c5xxx`）、
`struct XxxEvent with N elements` 错误串，以及**每个事件的 `Serialize` 实现本体**。
这些都是**观测/逐位**；**线格式已证实是 JSON**（`serde_json`，见 §3.2），语义仍部分未知（§5）。

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

## 3. 事件目录：74 个事件的线上形状（**权威表**）

### 3.1 怎么读这张表（第三方照着就能拼报文）

* **外层**是单键对象：`{"<事件名>":<体>}`。`<事件名>` 见下表第一列，**区分大小写**。
* **体**普通情况下是对象 `{…}`，字段**按表中顺序**出现。空体事件就是 `{}`。
* 传输层：TCP 每帧 = `u16 大端长度` + JSON 负载，**长度只算负载**（§1）。
* 判别值（tag）**不上线**：线路上的键是**名字**，不是数字。tag 列只用于和
  `src/events.rs` 对账（§7），第三方构造报文时**不需要**它。

### 3.2 这张表怎么来的（可复现）

`#[derive(Serialize)]` 里一次 `serialize_field` 被 LLVM 内联成三条连续指令：

```text
lea  rsi, [rip + 键名]     ; &'static str.ptr
mov  edx, LEN              ; &'static str.len（== 该字面量的真实字节数，这是自校验）
call <serialize_field 单态> ; 同一单态 ⇒ 同一值类型
```

⇒ **按地址顺序收 `lea rsi` 那一串 = 线上键序**。`serialize_struct("XxxEvent", N)` 的
名字在 `serde_json` 里**被丢掉**（只进错误串），所以线上看不到 `…Event`。

三道交叉验证（都跑在 `tools/wire_fields.py --verify` 里）：

| # | 验证 | 结果 |
|---|---|---|
| ① | 字段个数 == `.rodata` 里 `struct <Name>Event with N elements` 的 N | **67/67** |
| ② | `Deserialize` 的 `visit_map` 里能找到同名键的字节比较 | **67/67 全部字段** |
| ③ | 真实抓包（`fixtures/protocol*.jsonl`）的键序 == 本表 | **48 种事件 + 1 条 LAN 广播，全部相同** |

`N` 串只有 67 条：另外 **7 个事件的 `Serialize` 一次 `serialize_field` 都不调**
（体恒为 `{}`，空体结构体本来就不需要 `invalid_length` 文案）——
这就是"事件字段数"与"N 串条数"对不上的原因，不是缺数据。

### 3.3 全表（74 条，按判别值升序；查找请按名字）

`str\|null` = `Option<String>`，`i64\|null` = `Option<i64>`，`enum(str)` = 枚举但**线上是字符串**
（**完整**取值集合见 §5.2，已穷举，不再只是"已知取值"）。

判别值一列现在**没有 `—`**：`63 = GameHeartbeat`、`65 = VersionInfo` 由 §5.1 定死。

| `Tick` | `0` | `current_tick`: i64 | 逐位（无抓包） |
| `TickRequest` | `1` | （空体 `{}`） | 逐位 + 观测（2 行） |
| `TickReply` | `2` | `current_tick`: i64 | 逐位 + 观测（4 行） |
| `Subscribe` | `3` | `subscriber_id`: i64, `subscribed_event_type`: str | 逐位（无抓包） |
| `NewConnectionRequest` | `4` | `major_version`: str, `minor_version`: str | 逐位 + 观测（2 行） |
| `NewConnection` | `5` | `connection_id`: i64 | 逐位 + 观测（2 行） |
| `ConnectionRemoved` | `6` | `connection_id`: i64, `reason`: str\|null | 逐位（无抓包） |
| `ConnectedToServer` | `7` | `connection_id`: i64 | 逐位（无抓包） |
| `NewPlayerRequest` | `8` | `connection_id`: i64 | 逐位 + 观测（2 行） |
| `NewBotRequest` | `9` | `connection_id`: i64, `level`: i64 | 逐位 + 观测（2 行） |
| `BotAdded` | `10` | `connection_id`: i64, `player_id`: i64, `level`: i64 | 逐位 + 观测（2 行） |
| `ChangeTeamRequest` | `11` | `player_id`: i64 | 逐位（无抓包） |
| `RemovePlayerRequest` | `12` | `player_id`: i64 | 逐位（无抓包） |
| `PlayerInfo` | `13` | `player_id`: i64, `connection_id`: i64 | 逐位 + 观测（3 行） |
| `EntityRemoved` | `14` | `entity_id`: i64 | 逐位（无抓包） |
| `SoldierInfo` | `15` | `soldier_id`: i64, `player_id`: i64 | 逐位 + 观测（4 行） |
| `AddSoldierRequest` | `16` | `player_id`: i64 | 逐位（无抓包） |
| `RemoveSoldierRequest` | `17` | `player_id`: i64 | 逐位（无抓包） |
| `AxisModeChangeRequest` | `18` | （空体 `{}`） | 逐位（无抓包） |
| `FunctionModeChangeRequest` | `19` | （空体 `{}`） | 逐位（无抓包） |
| `TurnModeChangeRequest` | `20` | （空体 `{}`） | 逐位 + 观测（1 行） |
| `TimeModeChangeRequest` | `21` | （空体 `{}`） | 逐位 + 观测（1 行） |
| `GameNameRequest` | `22` | `game_name`: str | 逐位 + 观测（2 行） |
| `TargetInfo` | `23` | `target_id`: i64 | 逐位（无抓包） |
| `TeamInfo` | `24` | `entity_id`: i64, `team`: enum(str) | 逐位 + 观测（4 行） |
| `RemoveTeam` | `25` | `entity_id`: i64 | 逐位（无抓包） |
| `PosInfo` | `26` | `entity_id`: i64, `pos_x`: f64, `pos_y`: f64, `radius`: f64 | 逐位 + 观测（3 行） |
| `LifeInfo` | `27` | `entity_id`: i64, `alive`: bool | 逐位 + 观测（8 行） |
| `LifeQueuedDeath` | `28` | `entity_id`: i64, `tick`: i64 | 逐位 + 观测（2 行） |
| `GameNameInfo` | `29` | `game_name`: str | 逐位 + 观测（2 行） |
| `GameStateInfo` | `30` | `game_state`: enum(str) | 逐位 + 观测（8 行） |
| `GameStartRequest` | `31` | （空体 `{}`） | 逐位 + 观测（1 行） |
| `FunctionFireRequest` | `32` | `player_id`: i64 | 逐位 + 观测（1 行） |
| `FunctionFire` | `33` | `soldier_id`: i64 | 逐位 + 观测（1 行） |
| `FunctionUpdateRequest` | `34` | `player_id`: i64, `function`: str | 逐位 + 观测（2 行） |
| `FunctionInfo` | `35` | `owner_id`: i64, `function`: str, `corrected_function`: str\|null | 逐位 + 观测（3 行） |
| `FunctionActive` | `36` | `owner_id`: i64, `active_tick`: i64 | 逐位（无抓包） |
| `FunctionPoints` | `37` | `owner_id`: i64, `start_x`: f64, `start_y`: f64, `end_x`: f64 | 逐位（无抓包） |
| `FunctionsCalculated` | `38` | （空体 `{}`） | 逐位（无抓包） |
| `ClearObstacles` | `39` | （空体 `{}`） | 逐位 + 观测（1 行） |
| `AddObstacle` | `40` | `pos_x`: f64, `pos_y`: f64, `radius`: f64 | 逐位 + 观测（3 行） |
| `DestroyObstacle` | `41` | `pos_x`: f64, `pos_y`: f64, `radius`: f64 | 逐位 + 观测（1 行） |
| `QueueDestroyObstacle` | `42` | `pos_x`: f64, `pos_y`: f64, `radius`: f64, `tick`: i64 | 逐位（无抓包） |
| `TurnInfo` | `43` | `player_id`: i64, `soldier_id`: i64 | 逐位 + 观测（2 行） |
| `EffectInfo` | `44` | `effect_id`: i64, `pos_x`: f64, `pos_y`: f64, `effect_type`: enum(str), `start_tick`: i64 | 逐位 + 观测（5 行） |
| `AxisModeInfo` | `45` | `axis_mode`: enum(str) | 逐位 + 观测（2 行） |
| `FunctionModeInfo` | `46` | `function_mode`: enum(str) | 逐位 + 观测（2 行） |
| `TurnModeInfo` | `47` | `turn_mode`: enum(str) | 逐位 + 观测（3 行） |
| `TimeModeInfo` | `48` | `time_mode`: enum(str) | 逐位 + 观测（4 行） |
| `EndOfTurnInfo` | `49` | `end_of_turn_tick`: i64, `time_mode`: enum(str) | 逐位 + 观测（1 行） |
| `SkinGraphicsRequest` | `50` | `entity_id`: i64, `graphics_str`: str | 逐位 + 观测（3 行） |
| `SkinGraphicsInfo` | `51` | `entity_id`: i64, `graphics_str`: str | 逐位 + 观测（4 行） |
| `FaceGraphicsRequest` | `52` | `entity_id`: i64, `graphics_str`: str | 逐位 + 观测（3 行） |
| `FaceGraphicsInfo` | `53` | `entity_id`: i64, `graphics_str`: str | 逐位 + 观测（4 行） |
| `HatGraphicsRequest` | `54` | `entity_id`: i64, `graphics_str`: str | 逐位 + 观测（3 行） |
| `HatGraphicsInfo` | `55` | `entity_id`: i64, `graphics_str`: str | 逐位 + 观测（4 行） |
| `EntityEffectInfo` | `56` | `entity_id`: i64, `effect_type`: enum(str) | 逐位 + 观测（4 行） |
| `TurnLimitInfo` | `57` | `turn_limit`: i64\|null | 逐位（无抓包） |
| `TurnCountInfo` | `58` | `turn_count`: i64 | 逐位 + 观测（1 行） |
| `NameRequest` | `59` | `entity_id`: i64, `name`: str | 逐位 + 观测（3 行） |
| `NameInfo` | `60` | `entity_id`: i64, `name`: str | 逐位 + 观测（3 行） |
| `ColorInfo` | `61` | `entity_id`: i64, `color`: `{r,g,b}` | 逐位 + 观测（3 行） |
| `GameInfo` | `62` | `game_info`: `GameInfo` | 逐位 + 观测（3 行） |
| `GameHeartbeat` | `63` | （空体 `{}`） | 逐位 + 逐位（arm64 跳转表，§5.1） |
| `GameRemoved` | `64` | `game_address`: str（地址） | 逐位（无抓包） |
| `VersionInfo` | `65` | `version`: str, `system`: str | 逐位 + 观测（1 行）+ 逐位（arm64 跳转表，§5.1） |
| `RankRequest` | `66` | `entity_id`: i64, `rank`: i64\|null | 逐位 + 观测（3 行） |
| `RankInfo` | `67` | `entity_id`: i64, `rank`: i64\|null | 逐位 + 观测（3 行） |
| `LockRequest` | `68` | `lock`: bool | 逐位（无抓包） |
| `LockInfo` | `69` | `lock`: bool | 逐位（无抓包） |
| `ChatMessageRequest` | `70` | `entity_id`: i64, `message`: str | 逐位（无抓包） |
| `ChatMessage` | `71` | `entity_id`: i64, `message`: str | 逐位（无抓包） |
| `BanConnectionRequest` | `72` | `player_connection_id`: i64, `leader_connection_id`: i64 | 逐位（无抓包） |
| `ClearBansRequest` | `73` | （空体 `{}`） | 逐位（无抓包） |

### 3.4 三个负载结构体（嵌在上面的字段里）

| 结构体 | 字段（按线序） | 出现在 | 档位 |
|---|---|---|---|
| `GameInfo` | `room_name`, `game_state`, `current_player_count`, `address`, `axis_mode`, `function_mode`, `turn_mode`, `time_mode`, `locked` | `GameInfo.game_info`；**以及 LAN 广播的裸对象** | 逐位 + 观测（3 行） |
| `GraphwarColor` | `r`, `g`, `b` | `ColorInfo.color` | 逐位 + 观测（3 行） |
| `EventInfo` | `tick`, `event` | 未观测到上线（内部排序用） | 逐位（无抓包） |

LAN 广播（UDP 发现，§1）的负载**就是** `GameInfo` 这 9 个字段，**没有外层键**：

```json
{"room_name":"Graphwar Game","game_state":"Setup","current_player_count":1,
 "address":"[::1]:43717","axis_mode":"EveryUnit","function_mode":"NormalFunction",
 "turn_mode":"SequentialTurns","time_mode":"Timer1m","locked":false}
```

⚠️ **两个曾经写错的字段名，本轮就地更正**：

| 位置 | 旧文档 | 实测 | 证据 |
|---|---|---|---|
| `LockInfo` / `LockRequest` 的字段 | `locked`（候选） | **`lock`** | 字面量在 `0x1ba58c`（4 字节），`Serialize` 在 `0x50ca30` / `0x50e420` |
| `GraphwarColor` 的字段 | `rgb`（候选） | **`r` / `g` / `b`** | `Serialize` 在 `0x464c00`，三次 1 字节 `serialize_field`（`0x1c18f5/6/7`） |

`rgb` 这个"字段"是**假象**：那三个 1 字节字面量 `r`/`g`/`b` 在 `.rodata` 里紧挨着，
`strings` 把它读成了一条 `rgb`。`locked` 则是 `GameInfo` 的字段名（那个确实叫 `locked`），
被错套到了 `LockInfo` 上 —— **两个结构体的字段名不同，别互相套用**。

### 3.5 验收：旧"serde 按声明顺序序列化"那条推断，**已升级成观测**

早先的 13 个强类型事件只有一条弱证据：41 行抓包里键序相同，
而"serde 按字段声明顺序序列化"本身是**推断**。本轮用二进制把这条推断验掉了：

| 问题 | 结果 | 档位 |
|---|---|---|
| 抓包键序 == 二进制 `serialize_field` 顺序？ | **一致（13/13 个强类型事件，40 行）** | 逐位 + 观测 |
| 其余抓到的事件呢？ | **一致（48 种单键事件 + 1 条裸 LAN 广播，共 133 行）** | 逐位 + 观测 |
| 出现 `rename` / `skip_serializing_if` 了吗？ | **没有**：67 个事件的 `serialize_field` 次数与 `struct … with N elements` 的 N 逐个相等，且键名与 Rust 字段名同名（无改名痕迹） | 逐位 |

⇒ **"serde 按声明顺序序列化"在本二进制上成立**，而且线上键序现在有**两个互相独立**的
证据源（产出报文的代码 / 真实抓包）。若哪天两者不一致，会先炸
`tests/wire_format.rs::capture_key_order_matches_binary`。

复现：

```bash
GW2_UPSTREAM=.. ../.venv/bin/python tools/wire_fields.py --verify   # 表 + 三道自校验
cargo test --offline --test wire_format                              # 与抓包/与表对账
```

## 4. UI 文本（旁证玩法流程）

`Team 1 Victory!` / `Team 2 Victory!` / `YOU WON` / `YOU DID NOT WIN` —— 胜负是**按队伍**结算的，
与 §3.2 的队伍事件一致。

## 5. 判别值与枚举取值（缺口 A / B，**本轮已闭合**）

之前这里挂着两条"未知"：`VersionInfo` / `GameHeartbeat` 的判别值（§5.1），
以及各线上字符串枚举的**完整**取值集合（§5.2）。两条现在都**穷举**掉了，
下面是做法与判据；真正剩下的未知在 §5.3。

复现（两个脚本都是 stdlib only，`--verify` 不自洽退出码 1）：

```bash
../.venv/bin/python tools/wire_enums.py        --verify   # 缺口 B（x86 目标版）
../.venv/bin/python tools/event_tags_arm64.py  --verify   # 缺口 A（arm64 未 strip 版）
```

两个脚本都会把**被分析文件的 sha256** 打出来，结论绑定到具体字节：

| 工件 | sha256 | 来源 |
|---|---|---|
| `libgraphwarrust.so`（x86-64，stripped） | `9a538115…cabed187` | `.reference/steam/2026-08-28/`（= Steam 2026-08-28 版，`MANIFEST.tsv` 第 4 行） |
| `libgraphwarrust.so`（arm64，**未 strip**） | `8a612e95…` | `Graphwar II_1.0.apks` → `split_config.arm64_v8a.apk` |

下文所有 `0x…` 都是 **ELF 虚地址**（= REA/Ghidra 地址 − `0x100000`），
x86 与 arm64 各自一套、不要混用。

### 5.1 缺口 A：`63 = GameHeartbeat`、`65 = VersionInfo`

**先说结论的唯一性判据**：`enum GraphwarEvent` 的**声明顺序**就是判别值，
而 serde 的 `VARIANTS: &[&str; 74]` 是**按声明顺序**排的全部变体名 —— 所以
「`VARIANTS[63]` 是谁」这个问句本身就有唯一答案。难点只是**找到那张表**。
两条**互相独立**的证据给出同一张表、同一个位置：

| # | 证据 | 内容 | 档 |
|---|---|---|---|
| ① | x86 目标版 `.data.rel.ro` **`0x961608`** 的 `VARIANTS` | 74 项；`[62]=GameInfo`，**`[63]=GameHeartbeat`**，`[64]=GameRemoved`，**`[65]=VersionInfo`**，`[66]=RankRequest`；且 `docs/07` §7 那 **72 条解出的判别值逐个落在同名格上（72/72）** | 逐位 |
| ② | arm64 未 strip 版 `GraphwarEvent::serialize`（`0x4695ac`）的**判别值跳转表** | 表在 `0x19e770`（u16，单位 4 B，臂区基址 `0x4695e4`）；`[63]` 臂 `0x4698e4` → 写长度 **13** 的 `"GameHeartbeat"`（`.rodata 0x19f207`），`[65]` 臂 `0x4699bc` → 写长度 **11** 的 `"VersionInfo"`（`0x19f21f`）；整表 **74 格**，`[74]` 起填充为 0（绕回第 0 臂） | 逐位 |

为什么 ① 里的"位置 = 判别值"不是假设？三条一起看：

* serde derive 对枚举生成的 `deserialize_enum(NAME, VARIANTS, V)` **就是**按声明顺序拿
  `VARIANTS` 建 `__Field::__field0..__fieldN`，而 `serialize_*_variant(NAME, N, ...)`
  的 `N` 用的是同一个下标 —— 两者由同一个 `syn` 遍历产出，**不存在只改一个的可能**；
* ① 上 72 条**已知**判别值（本来是走完全不同的路径 `0x3d38f0` 的名字分派表解出来的）
  在这张表里**逐个同格**，包括那些"看起来会错"的（`PlayerInfo`=13、`FunctionPoints`=37、
  `ColorInfo`=61）—— 若是巧合，72 条同时巧合的概率是 0；
* ② 的下标**不是**"表里的位置"，而是运行期从 `Event` 的 tag word 算出来的**真判别值**
  （`eor x11,x10,#0x8000000000000000` 把 `tag` 还原成 `N`），因此 ② 与 ① 的机制完全不同。

⇒ 缺口 A 不是"两个候选之一"，是**定死的**：`GameHeartbeat = 63`、`VersionInfo = 65`。
`src/events.rs` 里"候选对应是推断"那句话可以删了（本轮未改该文件，见 §5.4）。

**顺带更正一处旧结论**：`enum Event` 不是"**≥ 75** 个变体"，是**恰好 74 个**（`0..=73`）——
① 的 `N` 是 `mov ecx, 0x4a`（=74）这个立即数，② 的跳转表只有 74 格、第 75 格起是填充。
所以 `0x3d3660` 那个 `0x800000000000004a`（=74）**不在** `GraphwarEvent` 的判别值空间里，
它的语义仍是未知（`docs/112` §10.2 判为 `HashMap` key，本轮未复核）。

#### 5.1.1 为什么旧路子找不到它们（留档）

旧文档说"全 `.text` 里找不到 materialise 它们常量的地方"，指的是
`0x8000000000000000 | N` 在**构造点**的三种编码形态（`or` / `add` / `lea`）。
缺号的两条**恰好没有构造点**：`GameHeartbeat` 只在"心跳"里被发，`VersionInfo` 也只在
建连握手时发 —— 都在别的 crate 路径上，本库里只有**序列化实现**与**名字**。
所以"扫不到常量"是对的，但**结论**（"所以不知道"）是错的：判别值不必从构造点拿，
`Serialize` 的跳转表下标本身就是它。

### 5.2 缺口 B：7 个线上枚举的**完整**取值集合

| 枚举 | 线上字段 | 个数 | 判别值 → 名字（= 声明顺序） |
|---|---|---|---|
| `AxisMode` | `axis_mode` | 4 | 0 `NoAxis` · 1 `OnlyMain` · 2 `EveryFive` · 3 `EveryUnit` |
| `FunctionMode` | `function_mode` | 2 | 0 `NormalFunction` · 1 `DiffEqFunction` |
| `TurnMode` | `turn_mode` | 2 | 0 `SequentialTurns` · 1 `SimultaneousTurns` |
| `TimeMode` | `time_mode` | 6 | 0 `Timer30s` · 1 `Timer1m` · 2 `Timer2m` · 3 `Timer3m` · 4 `Timer5m` · 5 `TimerInf` |
| `GraphwarGameState` | `game_state` | 8 | 0 `Connecting` · 1 `Setup` · 2 `WaitingForFunctions` · 3 `CalculatingResults` · 4 `SynchronizingResults` · 5 `ExecutingResults` · 6 `EndingGame` · 7 `Disconnected` |
| `GraphwarTeam` | `team` | 2 | 0 `Team1` · 1 `Team2` |
| `EffectType` | `effect_type` | 4 | 0 `ExplosionEffect` · 1 `DeathEffect` · 2 `SparkEffect` · 3 `AshEffect` |

#### 5.2.1 穷尽性判据（这是"为什么就这些"的全部理由）

`#[derive(Deserialize)]` 对枚举会生成 `deserialize_enum("AxisMode", VARIANTS, V)`，
其中 `VARIANTS: &'static [&'static str]` 是**全部**变体名、**按声明顺序**。
认不出来时走 `Error::unknown_variant(值, VARIANTS)`，那条错误路径把 `VARIANTS`
**地址与长度都写成了立即数**：

```text
lea  r8, [rip + VARIANTS]     ; 4c 8d 05 <disp32>   （或 lea rdx, …）
mov  ecx, N                   ; b9 <imm32>          N = 变体个数
call <unknown_variant>
```

所以判据是**结构性的、可穷举的**，不是"抓到过这几个"：

1. **闭集**：全 `.text` 扫这个指令形状，命中**恰好 8 张表** —— `GraphwarEvent` 一张，
   加上表里这 7 个值枚举各一张。**没有第 9 张**，所以本二进制里没有别的 serde 枚举。
   这个形状是 `serde` + `serde_json` 对枚举的**固定**代码生成，漏一个就说明漏了一个 enum。
   （旁证：若按「`.data.rel.ro` 里连续 (指针,长度) 对」去扫，能扫出 64 段 `&[&str]`，
   其中 62 段落在 `chrono` / `regex` / `reqwest` / `neli` 的依赖代码里，
   全都**不在**上面这个指令形状上 —— 它们不是 serde 枚举。）
2. **个数由代码给**：每张表的 `N` 是 `mov ecx, N` 的立即数，不是数出来的。
3. **内容由重定位给**：表元素是 `(&'static str, len)`，指针在 `.rela.dyn` 的
   `R_X86_64_RELATIVE` 里（**文件里是 0**，直接读文件读出来是空 —— 这也是旧文档
   在 `.rodata` 里找不全的原因之一）。`len` 是精确长度，不靠 NUL 终止。
4. **顺序 = 判别值**：`VARIANTS` 是声明顺序，`__Field::__fieldN` 与
   `serialize_*_variant(_, N, _)` 用同一个 N。

#### 5.2.2 校准点：`GameState` 必须能复现

方法有效性的硬判据：`GraphwarGameState` 的真值此前已由三条独立线索记为
`.data.rel.ro 0x9586d0` 的 `[&str; 8]`，**字节长 10/5/19/18/20/16/10/12**
（`docs/14` §11、`src/turnstate.rs`）。本方法解出来**一字不差**：

```
0x9586d0 N=8  Connecting(10) Setup(5) WaitingForFunctions(19) CalculatingResults(18)
              SynchronizingResults(20) ExecutingResults(16) EndingGame(10) Disconnected(12)
```

⇒ 长度、顺序、地址三项全中，方法可信。`tools/wire_enums.py --verify` 把它写成硬断言。

⚠️ 这 7 组取值本轮**只落在文档与本工具里**，没有写进 `src/`：见 §5.4。

#### 5.2.3 与 arm64 版交叉（判别值顺序的独立复核）

x86 版是 stripped，拿不到符号；arm64 版有符号，可以直接读 `Serialize` 的实现，
**它的顺序不经过 `VARIANTS`**（是 `ldrb` 拿判别值 + `adrp/add` 拿名字字面量）：

| 枚举 | arm64 证据 | 决定性指令 |
|---|---|---|
| `GraphwarGameState` | `0x4a52f4` | `ldrb w8,[x0]` 后 8 路分派，各臂 `(ptr,len)` 长度 10/5/19/18/20/16/10/12 |
| `AxisMode` | `0x4b07b4`（`visit_str`） | 6/8/9 字节比较，回填 `__field` 下标 0..3 |
| `TimeMode` | `0x4b08cc`（`visit_str`） | 同上，下标 0..5（`Timer1m/2m/3m/5m` 用 `add w13,w10,#0x10/0x20/0x40,lsl #12` 折叠成同一模板） |
| `EffectType` | `0x415f00`（`visit_str`） | 同上，下标 0..3 |
| `FunctionMode` | `0x470014` | `tst w21,#1` + `csel x2, DiffEqFunction, NormalFunction, ne` |
| `TurnMode` | `0x479310` | `tbz w21,#0` → `SequentialTurns`，否则 `SimultaneousTurns` |
| `GraphwarTeam` | `0x478574` | `cmp w10,#0` + `csel x2, Team2, Team1, ne` |

7 个枚举**逐个与 x86 的 `VARIANTS` 顺序一致**（`tools/wire_enums.py --verify` 校验集合，
`tools/event_tags_arm64.py` 用同一套符号锚定手法校验 `GraphwarEvent`）。
反过来，`GraphwarEvent` 的 74 条在 arm64 上是 **72/72 与 x86 名字分派表同值**（§5.1），
两套证据互为对方的校准点。

#### 5.2.4 顺带说明 `Subscribe.subscribed_event_type`

这个字段是 **`str`，不是枚举**（`Serialize` 侧走 `String` 单态 `0x3cb250`），
所以它**没有** `VARIANTS` 表 —— 它收的是 §7 那套 **snake_case 事件名**
（`tick_event` / `name_info_event` …，正是 `0x3d38f0` 比较的那 72 条）。
取值集合 = §7.2 全表左列；这一条是**推断**（有分派代码与字段同源，但没有抓到过
带该字段的真实报文），缺的证据是**一条 `Subscribe` 报文**。

### 5.3 仍然未知（本轮之后剩下的）

- **语义**：字段名与顺序是定死的，但"这个值是什么意思"仍有没定的：
  `address` / `game_address` 的具体地址格式（线上观测为字符串，如 `"[::1]:43717"` /
  `"69.164.213.144:35981"`）。
- **`EventInfo{tick,event}` 是否上线**：结构体读出来了（§3.4），但没观测到它作为
  报文键出现，也没有抓包 —— 用途**未知**。
- **权威端在哪里**：`graphwar_game_server.rs` 看起来是房间内权威端，但"谁判定命中/死亡"
  尚未证实。
- **`0x800000000000004a`（=74）是什么 key**：见 §5.1 末尾，它是某个 `HashMap` 的 key，
  但**不是** `GraphwarEvent` 的判别值。

### 5.4 `src/` 侧为什么一处没改（交下一轮，一次改完）

本轮的结论**全部落在文档与工具里**，`src/` 一个字节都没动。这**不是漏做**，是两处硬约束：

1. **`src/events.rs` 不在本轮允许改的范围内。** 它的模块文档还写着
   "63/65 的候选是 `GameHeartbeat` / `VersionInfo`，**属推断**" 和
   "`enum Event` **至少有 75 个变体**"——按 §5.1 这两句都该改成
   **74 个变体、63 = `GameHeartbeat`、65 = `VersionInfo`，逐位**。
2. **改 `src/` 任何文件都会让 `docs/coverage-matrix.tsv` 的 `src_tree_sha256` 变**，
   于是 `tools/coverage_matrix.py --check` 要求**同一个提交里**重跑生成器、
   一起提交 `docs/coverage-matrix.tsv` 与 `docs/149-coverage-matrix.md`。
   （本轮试过只加一条纯注释：`--check` 立刻失败，差异就是那一行 `src_tree_sha256`。）

⇒ 下一轮把这几件事**当一个原子提交**做完：

| 要改的 | 改成什么 |
|---|---|
| `src/events.rs` | 枚举补 `GameHeartbeat = 63`、`VersionInfo = 65` 两个变体；模块文档改成"74 个变体、逐位" |
| `tests/events.rs` | 「名字 → 判别值」全表加这两条 |
| `src/_scaffold/protocol.rs` | `WIRE_SPEC` 里这两条的 `tag: None` → `Some(63)` / `Some(65)`（`docs/07` §3.3 已按此写）；再加一张 `WIRE_VALUE_ENUMS` 常量表承载 §5.2 那 7 组 |
| `tests/wire_format.rs` | `tag_column_agrees_with_events_rs` 的"只有 2 个没有判别值"断言随之失效，改成 0 个 |
| `docs/coverage-matrix.tsv`、`docs/149-coverage-matrix.md` | 重跑 `python3 tools/coverage_matrix.py` 生成 |

**判据不变**：改了以后 `cargo test` 必须仍全绿；这两个判别值是**逐位**，不是推断。


---

## 6. 事件覆盖：74/74 已实现（原 `96-protocol-coverage`）

> 全文：[`archive/96-protocol-coverage.md`](archive/96-protocol-coverage.md)。


### 6.1 结论

| | 数量 |
|---|---|
| 二进制里的事件名（按 `Serialize` 实现全量提取） | **74** |
| `src/_scaffold/protocol.rs` 已实现（`WIRE_SPEC` 74 条 + `emit_*`） | **74** |
| **缺** | **0** |

**73 → 74 的原因（本轮更正）**：上一版的 73 = `src/events.rs` 里 **72 个有判别值的**变体
+ `GameHeartbeat`。它漏掉了 **`VersionInfo`** —— 那个事件有 `Serialize` 实现
（`0x50d3a0`）、有线上名字、也有抓包（夹具 1 行），只是**判别值未知**所以不在
`events.rs` 里。按"二进制里有几个 `Serialize` 实现"数，是 **74**。

数字沿革：44 → 50/23 → 73/73/0 → **74/74/0**。旧版"缺 24"里有 5 个其实早就实现了，
只是当时没按词边界查。

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
| `ClearObstacles` | 无此串（空体） | 强类型（无字段，体恒 `{}`） | **逐位** + 观测（夹具 1 行，空体） |
| `LockInfo` | 1 | 强类型 `lock: bool` | **逐位**（无抓包；键名由 `0x50ca30` 读出） |
| `LockRequest` | 1 | 强类型 `lock: bool` | **逐位**（无抓包；`0x50e420`） |
| `ChatMessage` | 2 | 强类型 `entity_id: i64, message: str` | **逐位**（无抓包；`0x511320`） |
| `ChatMessageRequest` | 2 | 强类型 `entity_id: i64, message: str` | **逐位**（无抓包；`0x5124e0`） |
| `GameRemoved` | 1 | 强类型 `game_address: str`（与 `GameInfo.address` 同类型） | **逐位**（无抓包；`0x511de0`） |
| `TurnLimitInfo` | 1 | 强类型 `turn_limit: i64\|null`（与 `RankInfo.rank` 同类型） | **逐位**（无抓包；`0x50f5d0`） |
| `BanConnectionRequest` | 2 | 强类型 `player_connection_id: i64, leader_connection_id: i64` | **逐位**（无抓包；`0x513cd0`） |
| `GameHeartbeat` | 无此串（空体） | 强类型（无字段，体恒 `{}`） | **逐位**（无抓包；`0x510a90` 一次 `serialize_field` 都没有） |
| `ClearBansRequest` | 无此串（空体） | 强类型（无字段，体恒 `{}`） | **逐位**（无抓包；`0x5137c0` 同上） |

汇总：**23 个名字全部是强类型、能解析、能逐字节回放、也能从零构造**（`emit_*`）。
其中 **14 种（41 行）** 有真实抓包；**其余 9 种**从未抓到过真实流量
（`LockInfo`、`LockRequest`、`ChatMessage`、`ChatMessageRequest`、`GameRemoved`、
`TurnLimitInfo`、`BanConnectionRequest`、`GameHeartbeat`、`ClearBansRequest`）——
它们的键名与键序**改由二进制的 `serialize_field` 调用序定死**，
档位从**推断/未知**升到**逐位**（口径见 §3.2）。

#### 6.2.1 那 10 个原来为什么只做"透传"（**已作废，留档**）

透传 = 结构体**不声明任何字段**：解析时把整段 `{…}` 的键与原始值文本按顺序存下，
序列化时原样拼回。当时的理由是：词汇表能拿到，**逐事件归属拿不到** ——
§3 里那些"已知字段"是**相邻字符串**读出来的，不是从该事件的 `Serialize` 读出来的，
所以在拿到一次真实抓包之前，把它们写成结构体字段就是把推断当结论。

**这条理由本轮不成立了**，因为找到了比抓包更硬的证据：`serialize_field` 的**调用序**
就是产出报文的代码本体。10 个全部升级成强类型，并各自配了 `emit_*`。
`extra` 保留（原透传实现的唯一优点：对端多发字段也不丢）。

⚠️ 因此 `_scaffold::protocol::supplemented_event_tests` 里
`passthrough_events_declare_no_field_names` 这条断言**已被替换**为
`upgraded_events_declare_the_binary_field_names`（"字段名与键序必须逐字等于二进制读出值"），
另有 `passthrough_keeps_escaped_strings_verbatim` 的断言对象从"透传的 `extra`"
改为"强类型字段 + 未知字段的 `extra`"，意图不变。**这是本轮唯一改动的既有断言。**


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
| `enum Event` 有多大 | **恰好 74 个变体**（判别值 `0..=73`，全部具名；见 §5.1） | **逐位**（§5.1 ① 的 `N = 0x4a` 立即数 + ② 的 74 格跳转表）。旧写的「≥75 个变体 / 具名 72 个」**已作废** |
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
| **63** | `game_heartbeat_event`（`0x3d38f0` 里**没有分支**，故本表前五列皆无；值由 §5.1 独立定出） | — | — | — | — | — |
| 64 | `game_removed_event` | 18 | `0x3d3c7e` | `0x3d3d3a` | `0x3d4ab2` | `0x8000…\|40` |
| **65** | `version_info_event`（`0x3d38f0` 里**没有分支**，故本表前五列皆无；值由 §5.1 独立定出） | — | — | — | — | — |
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
| 缺号 | 只有 63、65；且全 `.text` 扫 `0x8000…\|63` / `…\|65` 的三种编码形态**一次都没有**。⇒ 这只能说明「没有构造点」，**不能**推出「值未知」—— 两条的值已由 §5.1 定死（63 = `GameHeartbeat`、65 = `VersionInfo`） |

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

