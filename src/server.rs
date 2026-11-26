//! 服务端侧的**薄封装层**（2026-11-26 T30 起）。
//!
//! 本文件现在只剩 **`on_tick_request`**（`TickReply` 应答）—— 一层薄封装：tick 公式本体在
//! [`crate::graphwar_tick_handler`]（`tick_at`）。`docs/06` §74.4 的建议就是
//! "`server.rs` 只留 tick 薄封装"。
//!
//! ## 为什么搬走了一半
//!
//! 逐函数边界审计（`fixtures/boundary_audit.tsv`、`docs/06` §74）判出本文件里 6 个函数的
//! `panic::Location` 主文件全是 **`src/graphwar_server_connection_handler.rs`**
//! （`0x45a560` / `0x45a6b0` / `0x45a7b0` / `0x45aab0` / `0x45ad30` / `0x45b020`）——
//! 那是原版的一个**独立模块**，而我们此前没有同名文件。按 `AGENTS.md` §4「文件名就是证据」，
//! 新连接状态的同步（`RoomState` / `on_new_connection` / `to_wire`）已搬进
//! [`crate::graphwar_server_connection_handler`]（T30 / `task-27`；见 `docs/06` §80）。
//!
//! 下面三条 `pub use` 让 `crate::server::{RoomState, on_new_connection, to_wire}` 这些
//! **老路径一个字都不变** —— `src/game.rs` / `src/graphwar_game_server.rs` /
//! `src/graphwar_game_state_manager.rs` / `src/_scaffold/{root_wiring,diffconv,replaydiff}.rs` /
//! `tests/*` / `examples/serve_handshake.rs` 全部零改动。
//!
//! 还没实现：玩家加入（`NewPlayerRequest`）、图形/名字/段位同步、tick 节拍、
//! 开火结算的广播（见 `docs/33`）。

// ---- 从 graphwar_server_connection_handler.rs 再导出（保老路径，调用方 0 改动）----
pub use crate::graphwar_server_connection_handler::{on_new_connection, to_wire, RoomState};

/// 对 `TickRequest` 的应答。
///
/// 实测：`current_tick = base_tick + round(经过毫秒 * 60 / 1000)`（见 `crate::graphwar_tick_handler`）。
pub fn on_tick_request(base_tick: i64, base_ms: i64, now_ms: i64) -> String {
    let t = crate::graphwar_tick_handler::tick_at(base_tick, base_ms, now_ms);
    format!(r#"{{"TickReply":{{"current_tick":{}}}}}"#, t)
}

#[cfg(test)]
mod tick_tests {
    use super::*;

    #[test]
    fn tick_reply_matches_captured_format() {
        // 抓包里的样本形如 {"TickReply":{"current_tick":5}}
        let captured: Vec<&str> = include_str!("../fixtures/protocol_match.jsonl")
            .lines()
            .map(|l| l.trim())
            .filter(|l| l.starts_with("{\"TickReply\""))
            .collect();
        assert!(!captured.is_empty(), "夹具里应有 TickReply");
        for line in captured {
            let value: i64 = line
                .trim_start_matches(r#"{"TickReply":{"current_tick":"#)
                .trim_end_matches("}}")
                .parse()
                .expect("应能取出 current_tick");
            // 用基准值构造同样的 tick，输出必须与抓包逐字节相同
            let ours = on_tick_request(value, 0, 0);
            assert_eq!(ours, line, "TickReply 序列化与抓包不一致");
        }
    }
}

// ---- 以下为搬迁前的旧测试（`on_tick_request` 之外的部分）已随代码搬走 ----
// `state_sync_matches_captured_bytes` / `wire_encoding_matches_frame_layout`
// 两条测试现在在 `src/graphwar_server_connection_handler.rs` 的 `tests` 模块里
// （断言与期望值**一个字都没改**，只是跟着被测函数走）。
