//! 打印一帧 LAN 广播 JSON，用于与真实抓包逐字节比对。
//!
//! ```bash
//! cargo run --quiet --example emit_broadcast
//! ```
fn main() {
    // 可选参数：<address>，默认用 [::1]:35217
    let addr = std::env::args().nth(1).unwrap_or_else(|| "[::1]:35217".to_string());
    let b = gw2_rebuild::_scaffold::protocol::Broadcast {
        room_name: "Graphwar Game".into(),
        game_state: gw2_rebuild::turnstate::GameState::Setup,
        current_player_count: 1,
        address: addr,
        axis_mode: gw2_rebuild::graphwar_mode_manager::AxisMode::EveryUnit,
        function_mode: gw2_rebuild::graphwar_mode_manager::FunctionMode::NormalFunction,
        turn_mode: gw2_rebuild::graphwar_turn_manager::TurnMode::SequentialTurns,
        time_mode: gw2_rebuild::graphwar_mode_manager::TimeMode::Timer1m,
        locked: false,
    };
    println!("{}", b.to_json());
}
