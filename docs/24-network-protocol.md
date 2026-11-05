# 24 · 网络协议（抓包还原）

来源：在真实对局进程里 hook `libc` 的 `sendto/recvfrom/send/recv`
（`tools/capture_net.js`），抓到**真实报文**；样本原样入库为
`fixtures/protocol.jsonl`（29 条），`src/protocol.rs` 的测试直接吃这些帧。

## 1. 传输层（观测）

| 用途 | 传输 | 编码 | 端口 |
|---|---|---|---|
| LAN 房间发现 | UDP 广播 | 明文 JSON，**无长度前缀** | 抓到发往 `55463` |
| 对局通信 | TCP | **`u16` 大端长度 + JSON** | 服务器地址形如 `[::1]:35245` |

长度字段**只算负载**：实测帧头 `0x0031` = 49，后面正好 49 字节 JSON。

### 1.1 LAN 广播样本（观测）

```json
{"room_name":"Graphwar Game","game_state":"Setup","current_player_count":2,
 "address":"[::1]:35245","axis_mode":"EveryUnit","function_mode":"NormalFunction",
 "turn_mode":"SequentialTurns","time_mode":"Timers1m","locked":false}
```

注意广播里的 `time_mode` 是 `"Timers1m"`，而 TCP 里的 `TimeModeInfo` 是
`"Timer1m"` —— **两处拼写不一致**，是原程序的真实行为，复刻时必须照抄。

## 2. 消息格式

全部是单键对象：`{"TypeName":{字段...}}`。字段只观测到字符串、整数、
以及嵌套的小对象（`color` 的 `{r,g,b}`）。**没有观测到数组**。

## 3. 消息目录（29 种，全部有真实样本）

### 握手
| 消息 | 样本 |
|---|---|
| `GameNameRequest` | `{"game_name":"Graphwar Game"}` |
| `NewConnectionRequest` | `{"major_version":"2.0","minor_version":"60"}` |
| `NewConnection` | `{"connection_id":228}` |
| `TickRequest` | `{}` |
| `TickReply` | `{"current_tick":5}` |

### 房间设置
`GameStateInfo{game_state}`、`GameNameInfo{game_name}`、
`AxisModeInfo{axis_mode}`、`FunctionModeInfo{function_mode}`、
`TurnModeInfo{turn_mode}`、`TimeModeInfo{time_mode}`

取值实测：`axis_mode="EveryUnit"`、`function_mode="NormalFunction"`、
`turn_mode="SequentialTurns"`、`time_mode="Timer1m"`、`game_state="Setup"`。

### 玩家与实体
`NewPlayerRequest{connection_id}`、`PlayerInfo{player_id,connection_id}`、
`TeamInfo{entity_id,team}`、`ColorInfo{entity_id,color{r,g,b}}`、
`SoldierInfo{soldier_id,player_id}`、`EntityEffectInfo{entity_id,effect_type}`

实测：`player_id=229`、`soldier_id=230`、`team="Team1"`、
`color={r:149,g:5,b:62}`、`effect_type="DeathEffect"`。

### 外观/元数据（Request → Info 成对）
`NameRequest/NameInfo{entity_id,name}`、
`SkinGraphicsRequest/SkinGraphicsInfo{entity_id,graphics_str}`、
`FaceGraphicsRequest/FaceGraphicsInfo{entity_id,graphics_str}`、
`HatGraphicsRequest/HatGraphicsInfo{entity_id,graphics_str}`、
`RankRequest/RankInfo{entity_id,rank}`

实测取值：`name="user"`、`graphics_str="yellow"` / `"regular_eyes"` / `"soldier_helmet"`、`rank=0`。

### Bot
`NewBotRequest{connection_id,level}`、`BotAdded{connection_id,player_id,level}`（实测 `level=3`）

## 4. 已实现的部分

`src/protocol.rs`：

- `Frame::encode` / `Frame::decode`：`u16` 大端长度 + 负载；
- `parse_message`：取出类型名与内层对象（带**花括号配平**校验，半截帧拒绝）；
- `get_str` / `get_int` / `get_color`：按字段取值；
- 测试：29 条真实帧全部解析、长度前缀自洽、半包等待、嵌套颜色、非 JSON 拒绝。

## 5. 还没抓到的部分（明确标注）

- **对局内的消息**：本次抓包在 setup 阶段就达到 400 包上限，
  `TurnStart` / 函数提交 / 开火结算 / 掉线重连等**尚未捕获**；
- 服务器对局中广播给多客户端的顺序与去重规则未知；
- 二进制里的序列化实现（是手写还是 serde）未定位——目前只按**线上字节**还原。

⇒ 联机这一行现在是"**传输层与消息目录已还原并有真实样本**"，
但"完整对局协议"还差对局内消息的抓取。
