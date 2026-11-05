//! 打印一帧 LAN 广播 JSON，用于与真实抓包逐字节比对。
//!
//! ```bash
//! cargo run --quiet --example emit_broadcast
//! ```
fn main() {
    // 可选参数：<address>，默认用 [::1]:35217
    let addr = std::env::args().nth(1).unwrap_or_else(|| "[::1]:35217".to_string());
    let b = gw2_rebuild::protocol::Broadcast {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        current_player_count: 1,
        address: addr,
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
        locked: false,
    };
    println!("{}", b.to_json());
}
