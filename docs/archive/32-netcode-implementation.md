# 32 · 网络层实现：编码能逐字节还原

协议在 `docs/24` 已经还原；这一层把它变成**能跑、能对拍**的代码。

## 1. 已实现（`src/protocol.rs`）

| 能力 | API |
|---|---|
| 帧编解码（`u16` 大端 + 负载） | `Frame::encode` / `Frame::decode` |
| 解析任意消息 | `parse_message` + `get_str/get_int/get_f64/get_bool/get_color` |
| LAN 广播解析与**重新序列化** | `Broadcast::parse` / `Broadcast::to_json` |
| 常用消息序列化 | `emit_turn_info` / `emit_life_queued_death` / `emit_function_points` / `emit_effect_info` |

## 2. 逐字节验证（真实抓包）

### 2.1 广播

```
$ cargo run --quiet --example emit_broadcast -- "[::1]:43717"
{"room_name":"Graphwar Game","game_state":"Setup","current_player_count":1,
 "address":"[::1]:43717","axis_mode":"EveryUnit","function_mode":"NormalFunction",
 "turn_mode":"SequentialTurns","time_mode":"Timer1m","locked":false}
```

与抓包里那条**完全相同**（键序、无空格、布尔小写都对）。

测试 `reserializes_captured_broadcasts_byte_for_byte` 对夹具里**每一条**广播
做"解析 → 重新序列化 → 与原串比较"，全部相等。

### 2.2 四类消息

| 消息 | 测试 | 结果 |
|---|---|---|
| `TurnInfo` | `emitted_turn_info_matches_captured` | 逐字节一致 |
| `LifeQueuedDeath` | `emitted_life_queued_death_matches_captured` | 逐字节一致 |
| `FunctionPoints` | `emitted_function_points_matches_captured_floats` | 逐字节一致（**浮点格式也对**） |
| `EffectInfo` | `emitted_effect_info_matches_captured` | 逐字节一致 |

浮点这条值得单说：抓包里的 `-16.405029008797804` 是**最短往返**表示，
Rust 的 `{}` 对 `f64` 也是最短往返，所以两边一致。换任何"固定小数位"的格式化都会失败。

## 3. 还没做的

- **服务端状态机**：收到 `NewConnectionRequest` 之后要回什么、什么时候广播
  `PlayerInfo`/`TeamInfo`/`SoldierInfo`，这条链条还没实现；
- **`TickRequest`/`TickReply` 的节拍**：抓包里 145 对往返，但驱动条件未知；
- **tick 值**（见 `docs/31`）：弹丸飞行时间模型缺失。
