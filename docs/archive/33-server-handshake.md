# 33 · 服务端：新连接的状态同步（可运行）

## 1. 抓包里的顺序

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

## 2. 实现

`src/server.rs`：

```rust
pub struct RoomState { room_name, game_state, axis_mode, function_mode, turn_mode, time_mode }
pub fn on_new_connection(state: &RoomState, connection_id: i64) -> Vec<String>
pub fn to_wire(messages: &[String]) -> Vec<u8>   // 每帧 u16 大端长度 + JSON
```

`examples/serve_handshake.rs` 是一个最小 TCP 服务端：收到任意一帧后回上述 7 帧。

## 3. 验证（两条）

### 3.1 逐字节

`server::tests::state_sync_matches_captured_bytes`：把抓包夹具里的这 7 条
与我们的输出逐条比较，**全部相同**。

### 3.2 真实 socket 往返

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

## 4. 还没实现（明确）

- **玩家加入**：`NewPlayerRequest` 之后要回 `PlayerInfo`/`TeamInfo`/`ColorInfo`/
  `SoldierInfo`/`EntityEffectInfo`，其中颜色是服务端分配的随机值，尚未还原；
- **元数据同步**：`NameRequest`/`SkinGraphicsRequest`/`FaceGraphicsRequest`/
  `HatGraphicsRequest`/`RankRequest` 的应答（部分内容来自客户端自身）；
- **tick 节拍**：抓包里 145 对 `TickRequest`/`TickReply`，驱动条件未知；
- **开火结算的广播**：`events()` 已能生成事件（`docs/31`），还没接到服务端循环里；
- **tick 值**：弹丸飞行时间模型缺失。
