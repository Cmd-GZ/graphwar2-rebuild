//! 服务端状态机（第一段：新连接时的完整状态同步）。
//!
//! 依据真实抓包：客户端发 `NewConnectionRequest` 之后，服务端**按固定顺序**回一串消息：
//!
//! ```text
//! NewConnection -> TurnModeInfo -> GameStateInfo -> GameNameInfo
//!               -> AxisModeInfo -> FunctionModeInfo -> TimeModeInfo
//! ```
//!
//! 这一段是纯状态，没有随机成分，所以可以做到**逐字节**一致。
//!
//! 还没实现：玩家加入（`NewPlayerRequest`）、图形/名字/段位同步、tick 节拍、
//! 开火结算的广播（见 `docs/33`）。

/// 一个房间的静态状态。
#[derive(Debug, Clone, PartialEq)]
pub struct RoomState {
    pub room_name: String,
    pub game_state: String,
    pub axis_mode: String,
    pub function_mode: String,
    pub turn_mode: String,
    pub time_mode: String,
}

/// 新连接建立后要发给客户端的一串消息（顺序即抓包顺序）。
pub fn on_new_connection(state: &RoomState, connection_id: i64) -> Vec<String> {
    vec![
        format!(r#"{{"NewConnection":{{"connection_id":{}}}}}"#, connection_id),
        format!(r#"{{"TurnModeInfo":{{"turn_mode":"{}"}}}}"#, state.turn_mode),
        format!(r#"{{"GameStateInfo":{{"game_state":"{}"}}}}"#, state.game_state),
        format!(r#"{{"GameNameInfo":{{"game_name":"{}"}}}}"#, state.room_name),
        format!(r#"{{"AxisModeInfo":{{"axis_mode":"{}"}}}}"#, state.axis_mode),
        format!(r#"{{"FunctionModeInfo":{{"function_mode":"{}"}}}}"#, state.function_mode),
        format!(r#"{{"TimeModeInfo":{{"time_mode":"{}"}}}}"#, state.time_mode),
    ]
}

/// 把一串消息编码成线上字节流（每帧 `u16` 大端长度 + JSON）。
pub fn to_wire(messages: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for m in messages {
        out.extend_from_slice(&(m.len() as u16).to_be_bytes());
        out.extend_from_slice(m.as_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 从抓包夹具里取出服务端那段状态同步消息（顺序与抓包一致）。
    fn captured_state_sync() -> Vec<String> {
        // 每类只取**第一条**（夹具里每类最多 3 条样本）
        const KINDS: [&str; 7] = [
            "NewConnection",
            "TurnModeInfo",
            "GameStateInfo",
            "GameNameInfo",
            "AxisModeInfo",
            "FunctionModeInfo",
            "TimeModeInfo",
        ];
        let mut out: Vec<String> = Vec::new();
        for kind in KINDS {
            let prefix = format!("{{\"{}\"", kind);
            if let Some(line) = include_str!("../fixtures/protocol_match.jsonl")
                .lines()
                .map(|l| l.trim())
                .find(|l| l.starts_with(&prefix))
            {
                out.push(line.to_string());
            }
        }
        out
    }

    #[test]
    fn state_sync_matches_captured_bytes() {
        let state = RoomState {
            room_name: "Graphwar Game".into(),
            game_state: "Setup".into(),
            axis_mode: "EveryUnit".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
        };
        let ours = on_new_connection(&state, 228);
        let captured = captured_state_sync();
        assert_eq!(ours.len(), captured.len(), "消息条数与抓包不符");
        for (i, (a, b)) in ours.iter().zip(captured.iter()).enumerate() {
            assert_eq!(a, b, "第 {i} 条消息与抓包不一致");
        }
    }

    #[test]
    fn wire_encoding_matches_frame_layout() {
        let msgs = vec![r#"{"X":{"y":1}}"#.to_string()];
        let wire = to_wire(&msgs);
        assert_eq!(&wire[..2], &(msgs[0].len() as u16).to_be_bytes());
        // 能被自己的解码器读回
        let (f, used) = crate::protocol::Frame::decode(&wire).unwrap();
        assert_eq!(f.payload, msgs[0]);
        assert_eq!(used, wire.len());
    }
}
