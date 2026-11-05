# 134 · 客户端联机层：**真的能连服务器了**

状态：**现行**。分支 `agent/net-client`（基线 `a1934b7`）。
本文回答五件事：① 线上分帧与消息怎么落到 GDScript；② 联机的三通道（加入 / LAN 发现 / broker）
各自的状态机；③ 接进了哪几屏、界面状态怎么变；④ **哪些是原版观测、哪些是我们自己的选择**；
⑤ 还差什么（尤其是**进了 PlayScreen 之后**的对局同步）。

> 前置：`24-network-protocol.md`（传输层 + 真实抓包）、`96-protocol-coverage.md`（73 个事件、
> 外部标签 JSON、判别值不上线）、`33-server-handshake.md`（我们自己的服务端 7 条状态同步）、
> `62-server-loop.md`（能跑完整一局的服务端）、`115-broker.md`（broker 协议 + 我们的实现）。
> `132-android-build.md` §3 给的 15 个 `GraphwarBackend` 方法名是**骨架参照**（§2.2）。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 与抓包原文逐字节相同（`cargo test` 或自检里对拍）；
**观测** = 能指到夹具行号 / 地址 / 运行输出；
**推断** = 有间接证据、缺什么写在旁边；**未知** = 没有证据。
另加一档 **自定** = 本项目自己的选择，**不是**复原结论（与 `docs/108` §7 的用法一致）。

---

## 0. 一页结论

| 问题 | 答案 |
|---|---|
| 现在客户端能连服务器吗 | **能**。TCP 真的连、`u16` 大端帧真的收发、7 条状态同步真的解析 |
| `ConnectingPopUp` 还是模拟的吗 | **不是**。`await create_timer(1.2)` 那条假链路已删除；成败都由 socket 事件决定 |
| 连的是谁 | 本轮验证的是**我们自己的服务端**（`examples/serve_match`），不是原版服务器 |
| broker 用上了吗 | 用上了（`Global` 页）。但地址**默认空**：原版默认从 `graphwar.com` 取，本项目不发外网请求 |
| LAN 房间列表真的能用吗 | **能收**（UDP 42968，裸 9 字段 JSON + 换行）。**我们没有广播循环**（原版在 `graphwar_lan_broadcaster_v6.rs`，`src/` 本轮不动） |
| 进去之后能打完一局吗 | **协议层能**：客户端提交函数 + 开火，服务端广播 `FunctionPoints` 等结算报文（§6 有实测）。**界面层不能**：PlayScreen 还按本地规则画（§7） |
| 断线/失败真的走弹窗吗 | **是**。连不上、握手超时、对端关闭三种都落到 `DisconnectedPopUp`（§4.2） |
| 数字 | 客户端自检 **245 → 393**（+148，0 失败 0 跳过）；`cargo test --offline` **296**（未改 Rust） |

---

## 1. 传输层：分帧与消息

### 1.1 帧（**逐位验证**）

`docs/24` §1：`u16` **大端**长度 + JSON 负载，长度**只算负载**（实测帧头 `0x0031` = 49 字节）。
原版同一个写入函数 `0x3d89e0` 里有 `cmp $0xffff`（超长报
`Failed to write to tcp_stream. Message too long.`）与 `rol $0x8`（转大端）——
所以**对局连接与 broker 连接是同一套帧**（`docs/115` §1）。

`client/src/net/frame.gd::GwNetFrame`：

| API | 作用 | 对拍 |
|---|---|---|
| `encode(payload)` | 一帧；负载 > `u16` 返回空（**不截断**） | 自检：帧长/大端/负载逐字节 |
| `encode_many(list)` | 多条拼一段（等价 `src/server.rs::to_wire`） | 自检：7 条一次写入再切回来 |
| `decode_buffer(buf)` | 尽量多切帧，返回 `{frames, rest, bad_utf8}` | 自检：半帧/粘帧/**逐字节喂入**都能重组 |
| `parse_message(text)` | 单键对象 → `{kind, body, text}` | 自检：多键拒绝、非 JSON 拒绝、空体接受 |

> **半帧必须留着**：`rest` 是"还没到齐的那一帧的已到部分"，不是"少掉的尾巴"。
> 这条自检一开始写错过（断言成 3 字节），是**测试把它纠回来的**。

### 1.2 消息形状（**逐位 + 观测**）

* 全部是单键对象 `{"TypeName":{字段…}}`（`docs/24` §2）；`ClearObstacles` 的空体 `{}` 也算对象。
* 线上走 serde 的**外部标签**枚举：变体名就是键，**没有数值判别值**（`docs/96` §4）。
  ⇒ 客户端补一条事件只需要"名字 + 字段"，不需要知道 Rust 里的编号。
* 客户端**只拼原文、不做"解析再序列化"**：`JSON.stringify` 会重排/加空格，
  而线上字节要能逐字节对拍。所以发送路径一律是 `'{"Kind":<body_json>}'` 模板，
  字符串参数过 `GwNetFrame.json_string()` 转义（函数原文里可能有 `"`/`\`）。

### 1.3 黄金样本写在客户端里（**逐位验证**）

`client/src/net/wire_samples.gd::GwWireSamples` 抄了 10 条**抓包原文**，每条注明夹具行号：

| 常量 | 夹具 | 用途 |
|---|---|---|
| `NEW_CONNECTION_REQUEST` / `TICK_REQUEST` / `GAME_START_REQUEST` / `FUNCTION_UPDATE_REQUEST` / `FUNCTION_FIRE_REQUEST` | `protocol_match.jsonl:2/4/18/19/21` | 客户端发出去的请求 |
| `STATE_SYNC`（7 条） | `protocol_match.jsonl:22/23/30/32/25/26/27` | 握手应答（顺序即抓包顺序，`docs/33` §1） |
| `BROADCAST_1` | `protocol_match.jsonl:81` | LAN 广播（**裸**对象、无事件外壳、无长度前缀） |
| `VERSION_INFO` / `GAME_INFO_1` / `GAME_INFO_2` | `protocol_match.jsonl:83/84/85` | broker 会话 |

⚠️ 为什么要抄一份：`client/` 是**独立 Godot 工程**（`res://` 根是 `client/`），
运行时读不到仓库根的 `fixtures/`。抄进客户端才能让 `bash client/run.sh selftest`
在任何机器上只依赖仓库里的字节做对拍。**抄写一个空格都没动**。

自检里有一条"构造器 ↔ 黄金样本"的互锁：`GwWireSamples.broadcast(...)` 按
`src/protocol.rs::Broadcast::to_json` 的键序拼出来的默认样本，必须与 `BROADCAST_1`
**逐字节相同**。任何一边漂了都会红。

---

## 2. 代码结构：`client/src/net/`

### 2.1 为什么是 GDScript，不是 Rust

原版的联机在 Rust 侧（`graphwar_client_connection_handler.rs` 8 个函数、
`graphwar_backend.rs` 15 个方法）。我们**没有**反编译它们的编排层（`docs/132` §3 只做到符号级）。
而线上行为已经被抓包 + 我们自己的服务端钉死了（`docs/24`/`33`/`62`/`115`），
所以本轮把"协议怎么走"写进 GDScript，直接复用 Godot 自带的
`StreamPeerTCP` / `TCPServer` / `PacketPeerUDP`，**不动 `src/`**（本轮 0 行 Rust 改动）。

| 文件 | 类型 | 职责 |
|---|---|---|
| `frame.gd` | `GwNetFrame`（静态） | 分帧、解消息、取字段、JSON 转义 |
| `wire_samples.gd` | `GwWireSamples`（静态） | 抓包原文 + 广播构造器 |
| `link.gd` | `GwNetLink` | 一条 TCP 连接：连接/超时/重试/读缓冲/切帧/发送 |
| `session.gd` | `GwNetSession` | **加入一局**的状态机 + 房间模型 + 发送 API |
| `discovery.gd` | `GwLanDiscovery` | UDP 监听 LAN 广播 → 房间表（TTL） |
| `broker.gd` | `GwBrokerClient` | broker 会话：`VersionInfo` → `GameInfo`/`GameRemoved` |
| `net.gd` | `GwNet`（Node） | 外观：`App.net`，每帧 poll 三条通道，向上发信号 |

分层的理由：`GwNetLink` 只懂字节，`GwNetSession` 只懂事件，`GwNet` 只懂"给界面发什么信号"。
自检可以直接拿 `GwNetFrame`/`GwLanDiscovery.parse_broadcast` 做纯函数断言（**不碰 socket**）。

### 2.2 与原版 15 个方法名的对照（**推断**）

`docs/132` §3 给的符号表当骨架用，**只到这个程度**：名字对得上，语义没法逐条证明。

| 原版方法（Android 符号） | 本轮对应物 | 档 |
|---|---|---|
| `start_client_to_local_server` / `start_client_to_remote_server` | `GwNet.join(host, port)`（两条走同一条路径） | **推断**（原版怎么区分本地/远程未观测） |
| `start_client_to_replay` | —（回放播放器整块没做，`HANDOVER` §0 第 1 项） | **未知** |
| `start_lan_broadcaster` / `stop_lan_broadcaster` | **没有广播端**；只有 `GwLanDiscovery`（监听） | **未知**（广播循环未实现） |
| `start_server_broker_handler` / `stop_server_broker_handler` | `GwBrokerClient`（**浏览方**）；`announce()` 是房主方向的推断实现，界面未调用 | **推断**（`docs/115` §4） |
| `subscribe_to_event` / `subscribe_to_event_with_id` / `resubscribe_to_events` | `GwNetSession.message_received(kind, body)` 单一分发点 | **推断** |
| `setup_level` / `new` / `process` / `button_pressed` / `get_next_clip_checkpoint` | GDScript 侧各自等价物 / 不适用 | **未知** |

---

## 3. 三条通道

### 3.1 `GwNetSession`：加入一局（**观测**为主）

```text
idle → connecting ──TCP 连上──→ handshaking ──7 条状态同步到齐（或静默 0.30 s）──→ joined
           │                        │                                                 │
           └──连不上/超时──→ failed  └──5 s 超时──→ failed                             ├─ TurnInfo/EndOfTurnInfo → started
                                                                                      ├─ EndingGame → ended
                                                                                      └─ 对端关闭   → closed
```

| 行为 | 依据 | 档 |
|---|---|---|
| 连上先发 `NewConnectionRequest{major_version:"2.0",minor_version:"60"}` | 抓包 | **逐位**（自检比对客户端发出的头两字节 + 原文） |
| 服务端按固定顺序回 7 条（`NewConnection`→`TurnModeInfo`→`GameStateInfo`→`GameNameInfo`→`AxisModeInfo`→`FunctionModeInfo`→`TimeModeInfo`） | `docs/33` §1 | **观测** |
| "7 条到齐 = 握手完成"；只回一部分时用 **静默 0.30 s** 兜底 | 我们的服务端一次 write 吐 7 条；真实服务器是否分批**未知** | **推断** |
| 加入后发 `NewPlayerRequest{connection_id}` | 原版加入时发（`docs/24` §3） | **观测**（消息存在）／**自定**（我们的服务端忽略它） |
| 加入后**自动**发 `GameStartRequest` | 真游戏是**房主**按 Start 才发；我们的服务端是"等客户端来开局"的 harness | **自定**（`auto_start`，可关） |
| 收到的事件落到 `room` 模型：房间名/四个模式串/玩家表/实体坐标/生死/障碍/回合/函数原文 | `docs/24` §3–§6 的字段 | **观测** |
| `EndingGame` = 正常结束，**不弹**断线框 | `src/serverloop.rs` 发完就关 socket | **观测** |

房间模型 `session.room`（PlayScreen 侧将来要读的就是它）：

```text
connection_id, room_name, game_state, axis_mode, function_mode, turn_mode, time_mode,
players{ player_id: {team, soldiers[], connection_id} },
entities{ id: {kind, x, y, radius, team, player_id, alive} },
obstacles[ {x,y,r} ],                       # AddObstacle 的顺序
turn{player_id, soldier_id}, turn_count, end_of_turn_tick, current_tick,
functions{ owner_id(=soldier_id): {function, corrected} }, ending
```

发送 API：`send_new_connection_request` / `send_new_player_request` / `send_start_game` /
`send_tick_request` / `submit_function(player_id, text)` / `fire(player_id)`。

### 3.2 `GwLanDiscovery`：Local 房间列表（**观测** + 一条**推断**的行为）

* 端口 **42968**：原版在 LanLobby 绑 `UDP 0.0.0.0:42968`（`ss -ltnup` 实测，`docs/115` §5.3）。
* 报文形状：**裸的 9 字段对象 + 换行**；带事件外壳、`{"game_info":…}`、裸对象但**没有**换行
  的变体**全部被原版丢弃**（5 组受控实验，`docs/115` §5.3）。
* **枚举校验**：原版还丢弃了 `"time_mode":"Timers1m"` 那一条。我们的解释是
  "该字段被反序列化成枚举，非法字符串让整条报文解析失败"，因此要求
  `time_mode` 匹配 `Timer<正整数><s|m>` —— 这是 **推断**（只观测到"被丢弃"，
  没有观测到判定发生在哪一层）。
* 房间表：`address` 为键（同一地址再来一条 = **更新**，不是新行）、TTL 12 s、
  最多 64 条、顺序按 `address` 排 —— 后三条都是 **自定**（原版 TTL / 顺序**未知**）。

**没有广播端**：原版房主侧是 `graphwar_lan_broadcaster_v6.rs`（IPv6 `ff02::1`，
`docs/24` §7.2）。本轮不动 `src/`，所以"我们的服务端出现在列表里"是这样演的：
由测试（或 §8 的人工命令）发一条**同形状**的报文，`address` 指向真服务端的 `host:port`。
自检里就是这么做的：起 `serve_match` → 灌一条广播 → Local 页出现那一行 → 点 Join → 真连上。

### 3.3 `GwBrokerClient`：Global 房间列表（**逐位**）

```text
连上 → {"VersionInfo":{"version":"2.0.60","system":"linux"}}      ← 抓包原文
     → {"GameInfo":{"game_info":{9 字段}}}  × N                  ← 房间表
     → {"GameRemoved":{"game_address":"host:port"}}              ← 房间消失
```

* 帧与对局连接**同一套**（`docs/115` §1）；`GameInfo` 的 9 字段/键序/取值写法**逐位**
  （`fixtures/protocol_match.jsonl:84–86`）。
* `GameRemoved` **从未抓到过真实报文**（`docs/115` §2.3），形状来自序列化代码 ⇒ **推断**。
* broker 地址**默认空**：原版默认从 `https://graphwar.com/graphwar_II_ip.txt` 取
  （`docs/115` §4）。本项目**不发任何外网请求**，只认 `GW2_BROKER_ADDR`
  或界面显式设置（`App.net.set_broker_address`）。地址为空时 Global 页仍是那 4 条占位行
  —— 与 `docs/129` §4.4 的真帧口径一致。
* 房主方向（把自己的房间登记到 broker）不在本轮界面路径上；`announce()` 留着，
  是**推断**实现（`docs/115` §4：除此之外没有别的通道能到 broker）。

---

## 4. 接进了哪几屏

### 4.1 界面与信号

```text
JoinIpPopUp ──Join（host:port 校验，观测 docs/92 §4.7）──┐
LanLobby 房间行 ──Join（address 来自广播/ broker）───────┤
                                                        ▼
                                          App.connect_to(address)
                                                    │
                                     ConnectingPopUp（真的开 socket）
                                                    │
                     ┌──────────────────────────────┴───────────────────────┐
              joined（7 条到齐）                                    join_failed / disconnected
                     ▼                                                      ▼
          PlayScreen（online = true）                              DisconnectedPopUp
```

| 屏 / 文件 | 改动 |
|---|---|
| `client/src/app/app.gd` | 新增 `net` / `online` / `online_room` / `net_error`；`_submit_join_ip` 改为真连；新增 `connect_to`、`_cancel_connect`、5 个 `_on_net_*` 处理、`_sync_state_from_net`；`OK_DISCONNECTED` 与 `_leave_to_main_menu` 的联机分支 |
| `client/src/screens/lan_lobby.gd` | Local 页读 LAN 房间表；Global 页读 broker 房间表（无数据时用占位行）；房间行的 Join 真的连那一行的 `address` |
| `client/src/screens/{join_ip,connecting,disconnected}_pop_up.gd` + `connect_popups.gd` | **未改**。三个弹窗的结构/文案/按钮本来就是逐像素对好的（`docs/130`），本轮只换了**谁在什么时候开它们** |
| `client/tools/selftest.gd` | 只**新增**断言（+148，见 §6.3） |
| `client/src/screens/play_screen.gd` | **未改**（别人独占）。需要它配合的部分见 §7 |
| `src/`（Rust） | **未改** |

### 4.2 界面状态怎么变（自检逐条断言）

| 时刻 | 可观察的界面事实 |
|---|---|
| 点 Join | `JoinIpPopUp` 关、`ConnectingPopUp` 开、`App.online == false` |
| 连上但服务端还不说话 | **1.4 s 后仍在 `ConnectingPopUp`**（这条专门盯住"以前 1.2 s 计时器假装断线"） |
| 7 条到齐 | `ConnectingPopUp` 关、`PlayScreen` 可见、`current_screen == "PlayScreen"` |
| 同上 | `state.room_name` / `state.turn_mode` / `state.time_mode` / `state.axis_mode` / `state.function_mode` 全部来自服务端字符串；`App.online_room` 有快照 |
| 服务端把开局批发出完 | `_sync_state_from_net` 再刷一次 + `PlayScreen.begin_match()` 重建战场 |
| 连不上 / 握手超时 | `DisconnectedPopUp` 开、`ConnectingPopUp` 关、`online` 仍假、`net_error` 记下原因 |
| 对端断开 | `DisconnectedPopUp` 开、`online` 置假；`OK` 后回主菜单 |
| 点 Cancel | socket 真的放掉（`net.session == null`），两个弹窗都关 |

---

## 5. 哪些是原版观测、哪些是我们的选择

| 结论 | 档 | 依据 / 缺什么 |
|---|---|---|
| 对局与 broker 都是 `u16` 大端 + JSON | **逐位** | `docs/24` §1、`docs/115` §1；自检对拍帧头/负载 |
| 握手 7 条的名单与顺序 | **观测** | `docs/33` §1 + 夹具 7 行 |
| 73 个事件名、外部标签、无判别值 | **观测** | `docs/96` §4；本轮不新增事件名 |
| LAN 是 UDP 42968 + 裸 9 字段 + 换行 | **观测** | `docs/115` §5.3 的 5 组对照实验 |
| `Timers1m` 被原版丢弃 | **观测** | 同上（变体 E） |
| "非法枚举 ⇒ 整条丢弃"的机制 | **推断** | 只看到结果；不知道判定在 serde 还是别处 |
| `GameInfo` 9 字段、键序、`VersionInfo` 内容 | **逐位** | `fixtures/protocol_match.jsonl:83–86` |
| `GameRemoved{game_address}` | **推断** | 序列化代码读出来的形状，**从未抓到过** |
| 模式串 → `GwModes` 档位（`Turn/Timer1m/Timer3m/EveryUnit/NormalFunction`） | **观测** | 抓包实际取值就这些 |
| 其余档位串（`Timer2m/Timer5m/Every5Units/MainAxes/NoAxis/…`）的映射 | **推断** | 按 `docs/92` §6 的档位循环 + `docs/118` §7.1 的措辞对齐，**没有抓包** |
| `Team1/Team2` → 0/1 | **观测** | `docs/105`；抓包有 `Team1` |
| 客户端发 `NewPlayerRequest` | **观测**（消息存在）/ **自定**（我们仍然发，服务端忽略） | `docs/24` §3、`docs/33` §4 |
| 加入后自动 `GameStartRequest` | **自定**（偏离） | 真游戏由房主触发；我们的服务端等客户端开局 |
| 连接超时 3 s / 重试 0 次 / 退避 0.25 s | **自定** | 原版重试策略**未知**（`docs/132` §3 只有方法名） |
| 握手超时 5 s、静默兜底 0.30 s | **自定** | 同上 |
| LAN 房间 TTL 12 s、最多 64 条、按 address 排序 | **自定** | 原版 TTL / 顺序**未知** |
| broker 地址默认空 + `GW2_BROKER_ADDR` | **自定**（安全边界） | 原版默认从 graphwar.com 取；本项目不发外网请求 |
| `VersionInfo` 里仍写 `2.0.60`/`linux` | **自定**（刻意） | 我们的客户端版本是 `main.gd::CLIENT_VERSION`；为了与该会话形态一致照抄抓包 |
| 房间行的 `Join` = 连该行 `address` | **自定** | 原版点 Join 之后的行为**未知**（离线环境里无反应） |
| 断线后 `OK` 回主菜单 | **自定** | 原版断线后停在哪屏**未知** |
| 联机玩家名退化成 `Player <id>`（第一个用本机档案名） | **自定** | 服务端不发 `NameInfo`（`docs/33` §4） |
| 一个玩家有几名士兵 = `SoldierInfo` 的计数 | **观测** | `docs/24` §3 的实体分配 |
| 我们的服务端不实现 `PlayerInfo` ⇒ 客户端不知道自己是哪个 player | **观测（缺口）** | `docs/33` §4 明确列了"玩家加入未实现" |

---

## 6. 端到端记录（本轮实测）

### 6.1 命令

```bash
# 1) 编译内核与服务端样例
export CARGO_HOME=/path/to/ws/.cargo-home
export CARGO_TARGET_DIR=/path/to/work/target
cargo build --offline --example serve_match

# 2) 自检（会把这台服务端拉起来跑一遍完整的"发现 → 加入 → 开局 → 打一发"）
GW2_NET_SERVER=$CARGO_TARGET_DIR/debug/examples/serve_match \
  bash client/run.sh selftest
```

（找不到 `serve_match` 时这一条**显式 skip** 并打印原因，不静默通过。）

### 6.2 服务端 + 客户端两侧的实测输出（原样摘录）

```text
LISTEN 127.0.0.1:44525                       ← 我们自己的服务端（examples/serve_match）
ACCEPT 127.0.0.1:48802                       ← 客户端连上
RECV NewConnectionRequest                    ← 抓包原文的那一帧
RECV NewPlayerRequest                        ← 加入时发（服务端忽略，docs/33 §4）
[gw2/net] joined 127.0.0.1:44525 (connection_id=228)   ← 7 条到齐 = 握手完成
RECV GameStartRequest                        ← 客户端替房主开局（自定，§5）
STARTED obstacles=13 players=4 order=[229, 230] bots=[] ai=level1 budget=Some(25.0)
RECV FunctionUpdateRequest                   ← 提交 "0.00"
RECV FunctionFireRequest                     ← 开火
DONE
SERVER_DONE 127.0.0.1:44525
```

（障碍数每次不同：关卡由服务端用 `/dev/urandom` 作种子现生成，`docs/62` §1。
两次实测分别是 9 个与 13 个。）

客户端侧同时断言（节选）：

```text
假服务器：客户端第一帧是 NewConnectionRequest 的抓包原文字节        ← 逐位
假服务器：1.4 s 后仍在 ConnectingPopUp（没有假断线计时器）
假服务器：7 条到齐后 joined
真服务端：Local 列表收到这台服务端的广播（1 条）
真服务端：Local 页那一行的 address 就是它（127.0.0.1:44525）
真服务端：从 Local 列表点 Join 真的连上并完成握手
真服务端：收到 NewConnection / TurnModeInfo / … / TimeModeInfo   ← 7 条逐条断言
真服务端：开局那一批到了（障碍 13、SoldierInfo 4）
真服务端：两名玩家 / 每人 2 名士兵 / App.online_room 记下 4 名士兵
真服务端：FunctionUpdateRequest 发出去了 → 收到 FunctionInfo 回执
真服务端：开火后收到 FunctionPoints（服务器只广播首末点）
真服务端：修正后的函数文本进了 room.functions（`f(x) ± c`）
```

### 6.3 自检分层（用例 ↔ 能证明什么）

| 用例 | 依赖 | `_expect` 调用点 | 能证明什么 |
|---|---|---|---|
| `_test_net_framing` | 无 socket | 33 | 分帧/解析/广播/地址/档位映射与抓包原文一致 |
| `_test_net_join_fake_server` | 进程内 `TCPServer` | 29 | 抓包那 7 条（**拆成 3 段、段内切断帧**）→ 界面真的进 PlayScreen；1.4 s 后没有假断线 |
| `_test_net_failure_path` | 关着的端口 | 11 | 连不上 → `DisconnectedPopUp`；Cancel 真的放掉 socket；只写 IP 不连 |
| `_test_net_link_retry` | 先空端口、后起服务端 | 4 | 连接失败后**真的重试**并连上（`GwNetLink.retries`） |
| `_test_net_mid_session_disconnect` | 假服务器 | 8 | 对端断开 → `DisconnectedPopUp`；`OK` 回主菜单 |
| `_test_net_lan_discovery` | UDP 回环 | 19 | Local 页显示广播里的房间；非法枚举被丢弃；点 Join 真的进去 |
| `_test_net_broker_global` | 进程内 broker | 10 | Global 页显示 broker 的 `GameInfo`；`GameRemoved` 删行；地址清空回落占位 |
| `_test_net_real_server` | `examples/serve_match` | 30 | 与**我们自己的服务端**：发现 → 加入 → 开局 → 提交 → 开火 → 收结算 |

静态合计 **144 个调用点**；实际执行 **+148 条**（其中一条在"7 个消息名"的循环里跑 7 次，
另外 3 个调用点在条件分支里没被走到 —— 所以 393 − 245 = 148）。
缺 `serve_match` 时那条用例走 `_skip`：**357 passed / 0 failed / 1 skipped**。

### 6.4 回归

```text
SELFTEST 393 passed, 0 failed          ← 起点 245
cargo test --offline: 296 passed, 0 failed   ← 起点 296（本轮 0 行 Rust 改动）
godot --headless --path client --import      ← 新脚本已注册（GwNet* 7 个 class_name）
dump: @@KERNEL available=1 hit_test=1 methods=ping,path_calculate,hit_test,level_obstacles …
```

---

## 7. 仍未做 / 已知边界

### 7.1 ⚠️ 进了 PlayScreen 之后**断在这里**

**协议层通了，画面层没通**：`App.online_room`（快照）与 `App.net.session.room`（完整模型）
已经把服务端的一切摆好了，但 `play_screen.gd` **一个字都还没读**（本轮不归我改）。
现在的表现是：PlayScreen 用**本地规则**摆士兵（`_spawn_position`）与障碍（非战役 ⇒ 空），
所以你会看到"连上了、房间状态对了、但战场不是服务器那一份"。

PlayScreen 侧要做的四件事（交接清单）：

1. **士兵/障碍来源**：`begin_match()` 里判 `App.online` 为真时，
   用 `App.net.session.room.entities`（`kind == "soldier"`）与 `room.obstacles` 建
   `_soldiers` / `_obstacles`，**不要**再调 `_spawn_position`；队伍用实体的 `team`。
2. **轮到谁**：`room.turn{player_id, soldier_id}` 决定当前射手；`App.online` 为真时
   不该让玩家在自己不是射手时开火（原版的禁用逻辑**未观测**，这是推断的做法）。
3. **开火**：`App.online` 为真时 Fire 不再走本地结算，而是
   `session.submit_function(player_id, text)` + `session.fire(player_id)`；
   然后等 `FunctionPoints`（**只有起点与终点 x**，`docs/24` §6.1）与
   `FunctionInfo.corrected_function`，用**我们自己的采样器**复算弹道。
4. **随事件刷屏**：订阅 `session.message_received(kind, body)`，
   处理 `PosInfo` / `LifeInfo` / `EffectInfo` / `QueueDestroyObstacle` / `TurnInfo` /
   `EndOfTurnInfo`（回合时钟）/ `TickReply`（tick 60 Hz，`docs/35`）。

还有一个**结构性缺口**：我们的服务端不发 `PlayerInfo`（`docs/33` §4 明确列了"玩家加入未实现"），
所以客户端**不知道自己是哪个 `player_id`**（`session.my_player_id == -1`）。
本轮自检里"打一发"是借 `TurnInfo` 里的玩家 id 打的（**临时办法**，写在用例注释里）。
要真正分清"我/对手"，得先让服务端实现 `NewPlayerRequest → PlayerInfo/TeamInfo/ColorInfo/SoldierInfo`。

### 7.2 其它未做（逐条）

| 缺什么 | 状态 | 说明 |
|---|---|---|
| **房主侧 UDP 广播循环** | **未做** | 原版 `graphwar_lan_broadcaster_v6.rs`（`ff02::1`）；本轮只有监听端。我们的服务端要靠外部发一条广播才能在 Local 页出现（§8 给命令） |
| **建房后注册到 broker** | **未做** | 界面走的是本机建房（`Create Game` → GameSetup → 本地 Bot 局）；`announce()` 只是协议层面的推断实现 |
| **对局中的重连** | **未做** | `GwNetLink.retries` 只覆盖"连接阶段"；连上之后断了就只弹断线框，不自动重连（原版策略**未知**） |
| `GameHeartbeat` | **未知** | 从未抓到；broker 侧 TTL 是自定 |
| **颜色/名字/皮肤/段位同步** | **未做** | 服务端不发；PlayScreen 仍用本地队色表（`docs/102`）与档案名 |
| **"最后一批数据 + 立刻关闭"** | **已知边界** | Godot 4 的 `StreamPeerTCP` poll 到 EOF 后不再交数据，所以对端"发完就关"的那一批会丢（与 `read_frame` 的 `read_exact` 失败同性质） |
| **回放连接**（`start_client_to_replay`） | **未做** | 回放播放器整块还没做（`HANDOVER` §0 第 1 项） |
| **`ConnectingPopUp` 显示地址** | **不做** | 原版文案是固定的 `Connecting...`（`docs/130` 逐像素对齐过），不给它加东西 |

### 7.3 与旧文档的冲突（就地更正的记录）

`docs/108` §7 与 `commands.gd` 里曾写着"真正的 TCP 连接/握手属于 Rust 侧，本项目没有网络层，
所以校验通过 → ConnectingPopUp →（1.2 s）→ DisconnectedPopUp，**这条链路是模拟的**"。
**该说法自本文起作废**：链路现在是真的。`docs/108`/`commands.gd` 本轮不在可改范围内，
所以更正记录只写在这里（`commands.gd` 里 `JOIN_JOIN_IP` 那行的 🔶 标注同理）。

---

## 8. 怎么复跑（含 GUI 手动路径）

### 8.1 自检（推荐，全自动）

见 §6.1。没有 Rust 构建产物时会显式 skip 那一条：**357 passed / 0 failed / 1 skipped**。

### 8.2 GUI 手动：让我们的服务端出现在 Local 列表里

```bash
# 终端 A：起我们自己的服务端（TCP）
cargo run --offline --example serve_match -- 127.0.0.1:39993

# 终端 B：替它发一条 LAN 广播（我们的服务端没有广播循环，见 §7.2）
python3 - <<'PY'
import socket, json
info = {"room_name":"Graphwar Game","game_state":"Setup","current_player_count":1,
        "address":"127.0.0.1:39993","axis_mode":"EveryUnit","function_mode":"NormalFunction",
        "turn_mode":"SequentialTurns","time_mode":"Timer1m","locked":False}
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.sendto((json.dumps(info, separators=(",", ":")) + "\n").encode(), ("127.0.0.1", 42968))
PY

# 终端 C：客户端 → 多人 → Local 页出现一行 "Graphwar Game / 1" → 点 Join
bash client/run.sh
```

> 上面这行 `json.dumps` 的字节**已经验证过**：与 `GwWireSamples.broadcast(...)`
> 拼出来的 227 字节**完全相同**，且 `GwLanDiscovery.parse_broadcast` 能解出 9 个字段。
> （广播每 12 s 重发一次才不会被 TTL 淘汰；手动测一次就够。）

不想发广播就直接 **Join IP → `127.0.0.1:39993`**，效果一样（`docs/92` §4.7：
地址**必须**带端口，只写 IP 时原版毫无反应，我们也一样）。

### 8.3 broker（Global 页）

```bash
# 终端 A：我们自己的 broker（src/broker.rs）
cargo run --offline --example serve_broker -- 127.0.0.1:42970
# 终端 B：让客户端去连它（默认空 = 不连任何 broker）
GW2_BROKER_ADDR=127.0.0.1:42970 bash client/run.sh
# → 多人 → Global 页：显示 broker 当前房间表；没有房间时仍是 4 条占位行
```

---

## 9. 文件清单

新增：

```text
client/src/net/frame.gd          分帧 + 解消息（GwNetFrame）
client/src/net/wire_samples.gd   抓包原文 + 广播构造器（GwWireSamples）
client/src/net/link.gd           一条 TCP 连接（GwNetLink）
client/src/net/session.gd        加入一局的状态机 + 房间模型（GwNetSession）
client/src/net/discovery.gd      LAN 房间发现（GwLanDiscovery）
client/src/net/broker.gd         broker 浏览会话（GwBrokerClient）
client/src/net/net.gd            联机层外观（GwNet，App.net）
docs/134-net-client.md           本文
```

修改：

```text
client/src/app/app.gd            联机层接线 + 真连接（只动网络相关分支）
client/src/screens/lan_lobby.gd  Local/Global 两页的真实数据源 + 房间行 Join
client/tools/selftest.gd         只新增断言（+148）
```

**未改**：`play_screen.gd`、`replay_lobby.gd`、`player_screen.gd`、`client/src/ui/*`、
`src/`（Rust 0 行改动）、`docs/INDEX.md`、`docs/HANDOVER.md`、
三个连接弹窗的脚本（`connect_popups.gd` / `join_ip_pop_up.gd` / `connecting_pop_up.gd` /
`disconnected_pop_up.gd`）。
