//! `graphwar_server_connection_handler.rs`：服务端侧的**每连接处理器**
//! （原版 `GraphwarServerConnectionHandler`）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **ELF vaddr**（= 模块内偏移 = RVA；Ghidra/REA 的反编译地址 = 本地址 + `0x100000`）。
//!
//! # 1. 为什么这个文件存在（边界证据）
//!
//! 本模块此前**没有同名文件**，内容混在 `src/server.rs`。T24 的逐函数边界审计
//! （[`docs/06`](../../docs/06-verification.md) §74、`fixtures/boundary_audit.tsv`，
//! 判据 = `panic::Location` 硬归属）判出**六条** `mismatch`，全部单模块、一边倒：
//!
//! | 地址 | 符号名（三向真名，`fixtures/name_to_addr_v2.tsv`） | 首锚 `(line:col)` | 锚点行数 |
//! |---|---|---|---|
//! | `0x45a560` | `GraphwarServerConnectionHandler::forward_event` | `350:38` | 1（`:350`） |
//! | `0x45a6b0` | `…::handle_clear_bans_request_event` | `247:97` | 2（`:247 :248`） |
//! | `0x45a7b0` | `…::handle_connection_removed_event` | `224:97` | 3（`:224 :225 :316`） |
//! | `0x45aab0` | `…::forward_connection_specific_event` | `326:18` | 2（`:326 :336`） |
//! | `0x45ad30` | `…::handle_ban_connection_request_event` | `233:105` | 4（`:233 :234 :238 :242`） |
//! | `0x45b020` | `…::tick` | `206:42` | **8**（`:206 :207 :208 :258 :259 :291 :292 :303`） |
//!
//! 六条的 `panic::Location` 主文件**全部**是 `src/graphwar_server_connection_handler.rs`
//! ⇒ `AGENTS.md` §4「**文件名就是证据**」。本文件即按该证据从 `src/server.rs` 切出
//! （T30 / `task-27`，2026-11-26），`src/server.rs` 留 `pub use` 再导出 ⇒ **公开路径不变**。
//!
//! ## 1.1 两条已被别的证据否掉的候选（写在这里省得下一个人重查）
//!
//! * `0x4500c0`（`new`）：`fixtures/name_to_addr_v2.tsv` 里它的候选模块是本模块（4 个真名锚，
//!   首锚 `50:59`），但台账判它 **`multi`** —— 主文件是 `graphwar_game_server.rs`（54 锚）
//!   + 另 4 个模块，其中本模块只占 3 个。`docs/06` §65 / §72 已按 `Location` 结案：
//!   `new` 属 `graphwar_game_server.rs`，**不是本模块**。按 §4 不搬。
//! * `0x45b020` 在 Windows 的 `.symtab` 里同时记成 `subscribe_to_events_event_handler::{{closure}}`
//!   （`fixtures/name_to_addr_v2.tsv` 两行同址）⇒ **`tick` 本身就是订阅 `tick_event` 的那个闭包**
//!   （**观测**：两条架构的符号指向同一地址）。
//!
//! # 2. 我方现状（这个文件里装了什么 / 什么还没落码）
//!
//! | 内容 | 档位 | 证据 |
//! |---|---|---|
//! | 新连接时的 **7 条状态同步**：`RoomState` + `on_new_connection`（顺序即抓包顺序） | **逐位**（行为）+ **观测**（归属） | 与真实客户端抓包**逐字节**一致（`docs/33`、`fixtures/protocol_match.jsonl`；`state_sync_matches_captured_bytes` 钉住） |
//! | `to_wire`（每帧 `u16` 大端长度 + JSON） | **观测** | 抓包帧布局（`wire_encoding_matches_frame_layout` 钉住） |
//! | 上面那 **6 条有锚函数**的**函数体** | **未落码（0 个函数体）** | 台账只有地址与 Location 行号；行为没有逐条还原 |
//! | `on_tick_request`（`TickReply` 应答） | 仍在 `src/server.rs` | `docs/06` §74.4 的建议：`server.rs` 只留这一层薄封装（tick 公式本体在 `src/graphwar_tick_handler.rs`） |
//!
//! ⚠️ **新连接那 7 条同步的原版落点，是本模块里"没有 `Location` 锚"的那部分函数**
//! （台账 527 只收能按 `Location` 硬归属的函数，所以那部分不在 §1 的表里）。
//! 它归本模块的依据是 `docs/33` 的抓包 + `docs/02-module-map.md` §17.4 的登记（**观测**），
//! 不是这 6 个锚 —— 因此本文件建立**不**等于那 6 个函数已经还原（同 `docs/06` §74.3 口径）。
//!
//! # 3. 结构
//!
//! `RoomState` = 一个房间的静态状态（房名 + 7 个模式/状态字段的字符串形态）。
//! ⚠️ 字段**类型**是按抓包文本挑的最小表示（`String`），**不是**从二进制读出来的布局；
//! 原版这个结构的字段偏移在本 checkout 没有证据（**未知**）。
//!
//! # 4. 公开路径（调用方 0 改动）
//!
//! `src/server.rs` 里有
//! `pub use crate::graphwar_server_connection_handler::{on_new_connection, to_wire, RoomState};`，
//! 所以 `crate::server::{RoomState, on_new_connection, to_wire}` 这三条老路径继续有效
//! （`src/game.rs` / `src/graphwar_game_server.rs` / `src/graphwar_game_state_manager.rs` /
//! `src/_scaffold/{root_wiring,diffconv,replaydiff}.rs` / `tests/*` / `examples/serve_handshake.rs`
//! 一个都不用改，见 `fixtures/boundary_split_plan.txt` §2）。

/// 原版模块名（arm64 `.symtab` 里的 `graphwarrust::graphwar_server_connection_handler`）。
pub const MODULE: &str = "graphwar_server_connection_handler";

/// `panic::Location` 里逐字节出现的源文件名 —— 与 `MODULE` 同名，就是 §1 那六条的证据侧。
pub const SOURCE_FILE: &str = "src/graphwar_server_connection_handler.rs";

/// 与真二进制对照的锚定强度（判据同 `fixtures/name_to_addr.tsv`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// 双向唯一 `(file,line,col)` 锚。
    Unique,
    /// 无直接一对一锚，但按「一个名字只对应一个地址」消去后只剩一个候选。
    DeducedByElimination,
    /// 该函数不引用任何 `Location`，名字无法落地（纯胶水/纯内联）。
    NoAnchor,
}

/// 一个已归属的函数（**证据行**，不是行为实现）。
///
/// 没有 arm64 列：本机 `.reference/upstream/` 只有 x86 的 `libgraphwarrust.so`，
/// 别的文档引用的 arm64 地址在本 checkout 无法复算 ⇒ 宁可不给，也不抄二手数字。
#[derive(Debug, Clone, Copy)]
pub struct Func {
    pub name: &'static str,
    /// x86 steam 2026-08-28 构建的 ELF vaddr（RVA）。
    pub x86: u64,
    /// `fixtures/name_to_addr_v2.tsv` 记的第一个 `(line, col)` 锚。
    pub loc_first: (u32, u32),
    /// `fixtures/boundary_audit.tsv` 里该函数落在本文件的锚点行数。
    pub loc_lines: u32,
    pub link: Link,
}

/// 本模块**有 `Location` 锚的全部 6 个函数**（= 台账里那 6 条 `mismatch`）。
///
/// `0x4500c0`（`new`）不在本表：台账判 `multi`、主文件 `graphwar_game_server.rs`（见 §1.1）。
pub const FUNCTIONS: &[Func] = &[
    Func { name: "GraphwarServerConnectionHandler::forward_event", x86: 0x45_a560, loc_first: (350, 38), loc_lines: 1, link: Link::Unique },
    Func { name: "GraphwarServerConnectionHandler::handle_clear_bans_request_event", x86: 0x45_a6b0, loc_first: (247, 97), loc_lines: 2, link: Link::Unique },
    Func { name: "GraphwarServerConnectionHandler::handle_connection_removed_event", x86: 0x45_a7b0, loc_first: (224, 97), loc_lines: 3, link: Link::Unique },
    Func { name: "GraphwarServerConnectionHandler::forward_connection_specific_event", x86: 0x45_aab0, loc_first: (326, 18), loc_lines: 2, link: Link::Unique },
    Func { name: "GraphwarServerConnectionHandler::handle_ban_connection_request_event", x86: 0x45_ad30, loc_first: (233, 105), loc_lines: 4, link: Link::Unique },
    Func { name: "GraphwarServerConnectionHandler::tick", x86: 0x45_b020, loc_first: (206, 42), loc_lines: 8, link: Link::Unique },
];

// ---------------------------------------------------------------------------
// 1. 状态结构
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// 2. 新连接时的状态同步（7 条，顺序即抓包顺序）
// ---------------------------------------------------------------------------

/// 新连接建立后要发给客户端的一串消息（顺序即抓包顺序）。
///
/// 依据真实抓包：客户端发 `NewConnectionRequest` 之后，服务端**按固定顺序**回一串消息：
///
/// ```text
/// NewConnection -> TurnModeInfo -> GameStateInfo -> GameNameInfo
///               -> AxisModeInfo -> FunctionModeInfo -> TimeModeInfo
/// ```
///
/// 这一段是纯状态，没有随机成分，所以可以做到**逐字节**一致（`docs/33`）。
///
/// 还没实现：玩家加入（`NewPlayerRequest`）、图形/名字/段位同步、tick 节拍、
/// 开火结算的广播（见 `docs/33` 与本文件 §2 的"未落码"表）。
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
///
/// ⚠️ 这一条**没有独立的 `Location` 锚**（不在台账 527 里）：它跟着唯一用户
/// `on_new_connection` 的发送侧走（**推断**：发送侧胶水）。归属证据只有"谁用它"。
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
        let (f, used) = crate::_scaffold::protocol::Frame::decode(&wire).unwrap();
        assert_eq!(f.payload, msgs[0]);
        assert_eq!(used, wire.len());
    }
}
