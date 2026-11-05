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

`time_mode` 实测取值只有 `Timer1m` / `Timer3m`（两次抓包共 385 次出现，
`Timers1m` 出现 **0** 次）。

> **更正**：本文件早先写过"广播拼 `Timers1m`、TCP 拼 `Timer1m`，两处不一致"。
> 那是我凭印象写下的**错误结论**，实测数据否定了它。教训：文档里的每一个字面量
> 都必须来自抓包数据的直接引用，不能凭记忆补全。

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

## 4.5 对局内消息（第二轮抓包，关键）

`tools/capture_net_raw.js` 改抓**原始字节流**再按帧重组（TCP 会拆包，不能按调用边界当帧），
得到 166 帧、去重 109 条，存为 `fixtures/protocol_match.jsonl`。

### 关卡障碍 `AddObstacle`

```json
{"AddObstacle":{"pos_x":-14.167697818619251,"pos_y":-8.472137403369004,"radius":3.901519740713089}}
```

同一局抓到 **12~14 个**障碍。半径里同时出现过 **0.8**（两次，像最小值）和 **5.27**（大值）；
坐标可超出棋盘（`pos_y=18.32` 超过 `|y|≤15`），说明障碍允许落在场外。

### 实体位置 `PosInfo`

```json
{"PosInfo":{"entity_id":231,"pos_x":-21.604890565331004,"pos_y":5.894946389140589,"radius":0.5}}
```

士兵 `radius` **恒为 0.5**（4 条样本一致）。

### 生死 `LifeInfo`

```json
{"LifeInfo":{"entity_id":231,"alive":true}}
```

### 提交的函数 `FunctionInfo`

```json
{"FunctionInfo":{"owner_id":234,"function":"exp(sin(x))","corrected_function":null}}
{"FunctionInfo":{"owner_id":231,"function":"","corrected_function":null}}
```

⇒ **协议里直接带着玩家写的函数原文**。空串表示该实体尚未提交。
这是"回合语义"最直接的证据来源：谁在什么时候提交了什么。

### 其他新抓到的
`VersionInfo`、`TimeModeChangeRequest`、`FunctionUpdateRequest`。

## 4.6 回合与开火（第三轮抓包，46 种消息）

跑满 150 秒的对局后，协议里出现了完整的回合/开火链路：

| 阶段 | 消息 | 样本 |
|---|---|---|
| 开局 | `GameStartRequest` | `{}` |
| 清场 | `ClearObstacles` | `{}` |
| 摆障碍 | `AddObstacle` ×N | `{"pos_x":..,"pos_y":..,"radius":..}` |
| 轮到谁 | `TurnInfo` | `{"player_id":229,"soldier_id":231}` |
| 第几回合 | `TurnCountInfo` | `{"turn_count":0}` |
| 回合截止 | `EndOfTurnInfo` | `{"end_of_turn_tick":10895,"time_mode":"Timer3m"}` |
| 提交函数 | `FunctionUpdateRequest` | `{"player_id":229,"function":""}` |
| 请求开火 | `FunctionFireRequest` | `{"player_id":232}` |
| 宣布开火 | `FunctionFire` | `{"soldier_id":233}` |
| 结果 | `LifeInfo` / `EntityEffectInfo` | `{"entity_id":230,"alive":true}` / `{"effect_type":"DeathEffect"}` |

⇒ **"谁在第几回合、什么时候提交了什么函数、谁开了火"全部在协议里**，
而且带 `end_of_turn_tick` 这样的时间戳，回合语义不再需要猜。

### 抓包里踩的两个坑

1. **双计**：`recv` 内部会调用 `recvfrom`，两个 hook 各抓一次 → 数据重复一遍；
2. **长度前缀重复**：入向数据里先单独读 2 字节长度，随后收到的块又以同样的
   前缀开头（`00 27 00 27 {...}`），按朴素帧解析会卡住。
   最终改用**扫描 JSON 对象**的方式提取，对前缀差异免疫。

## 5. 还没抓到的部分（明确标注）

- ~~对局内消息~~ **已捕获**（见 4.5）：障碍、位置、生死、函数原文都有了；
- 仍缺：**开火结算的结果消息**（打中谁/伤害多少）与**回合推进消息**——
  这一局只跑了 60 秒且 Bot 只提交了一次函数；
- 服务器对局中广播给多客户端的顺序与去重规则未知；
- 二进制里的序列化实现（是手写还是 serde）未定位——目前只按**线上字节**还原。

⇒ 联机这一行现在是"**传输层与消息目录已还原并有真实样本**"，
但"完整对局协议"还差对局内消息的抓取。

## 6. 开火结算（第四轮抓包，240 秒对局）

### 6.1 结算相关消息

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

### 6.2 修正文本（实测）

```json
{"FunctionInfo":{"owner_id":234,"function":"0.00","corrected_function":"0.00 + 4.638"}}
{"FunctionInfo":{"owner_id":231,"function":"","corrected_function":" + 10.990"}}
```

`4.638` / `10.990` 正是各自的 `start_y`，证实修正是 `f(x) + c` 且**以文本形式拼接**；
`src/shot.rs::corrected_text` 复刻了这个格式（三位小数），并有针对这两个真实样本的测试。

### 6.3 用真实弹道对拍

`FunctionPoints` 给了起点与终点，`FunctionInfo` 给了函数原文，于是可以复算：

| 起点 | 抓到终点 x | 方向 | 我们的轨迹是否覆盖终点 |
|---|---|---|---|
| `1.651, 4.638` | `-16.405` | `flip=true`（向左） | **是** |
| `-19.513, 10.990` | `-12.041` | `flip=false`（向右） | **是** |

⇒ 两发的爆炸点都落在我们算出的轨迹上，且方向由实测确定（不是猜的）。

**还没做到的**：本局 **没有出现** `alive:false` —— 12 次 `EntityEffectInfo` 里
没有死亡事件，可能因为这一局双方各 2 名士兵且都在打空/打在障碍上。
因此"伤害数值/死亡判定"仍未取得真值。
