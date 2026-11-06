//! 原版 `src/graphwar_game_info.rs` —— **局域网房间信标的载荷**（`GameInfo` 结构体）。
//!
//! ## 归属更正（2026-11-07，`docs/16-client.md` §33.9）
//!
//! `GameInfo` **不在** `graphwar_event.rs` 里。它的 serde impl 的完整路径是
//! `graphwarrust::graphwar_game_info::_::<impl serde::ser::Serialize for
//!  graphwarrust::graphwar_game_info::GameInfo>` —— 模块段是 `graphwar_game_info`。
//! 本 crate 现在把结构体放在 [`crate::events`] 里（历史原因，那个文件先写），
//! **文件级归属是错的**，正确位置是本文件。本轮**没有搬**
//! （`docs/02-module-map.md` §17.4 的规矩：一个文件混了多个原版模块而边界无证据时不猜不搬），
//! 只在这里登记并给出可核对的依据。
//!
//! ## 它为什么在"0 个可归属函数"这一档里
//!
//! 本模块**没有任何自己的函数**：两个 impl 都生成在 `_::` 这个匿名模块下，
//! 且 `Serialize` 是泛型单态化、`Visitor` 是 serde derive 的样板
//! （`docs/02-module-map.md` §18.1 的 `expecting` 归类）。
//! **0 个可归属 ≠ 没内容**：结构体本身、它的 9 个字段、它的两条进出口全在这里。
//!
//! ## 两条进出口（`观测`，arm64 `bl` 图的**被调用方**方向）
//!
//! | 方向 | 谁在动它 |
//! |---|---|
//! | 序列化 `GameInfo::serialize`（arm64 `0x46b210`，1264 字节） | `GraphwarLanBroadcasterV6::process`（UDP 广播房间信息） |
//! | 反序列化 `__FieldVisitor::visit_str`（arm64 `0x46cd9c`，588 字节） | `serde_json` 的 `deserialize_struct`（收广播的一侧） |
//!
//! `serialize` 的 `bl` 表里 `serde_json::ser::format_escaped_str` ×10、
//! `SerializeStruct::serialize_field` ×4、`serialize_entry` ×1 ——
//! 10 次转义 + 4 次字段 + 1 次 map entry，与"9 个字段里有一组是嵌套结构"吻合
//! （`docs/07` §3.4）。**具体哪 4 个走 `serialize_field`、哪几个走 map**
//! 本轮未逐条对齐，标 `未知`。
//!
//! ## 报文形态
//!
//! LAN 发现是 **JSON**（`serde_json`，不是二进制协议）——
//! 这与 `docs/09-network.md` 里"局域网用 `GameInfo` 广播"一致，
//! 也解释了为什么这个结构体可以同时被 `GraphwarLanBroadcasterV6` 与
//! `GraphwarLanVBox`（`update_game_list`）两边用：
//!
//! ```text
//! graphwarrust::core::ptr::drop_in_place<
//!     core::result::Result<graphwar_game_info::GameInfo, serde_json::error::Error>>
//! ```
//!
//! 那个 `drop_in_place` 符号本身就是"`GameInfo` 从 `serde_json` 里出/入"的硬证据。

pub use crate::events::{GameInfo, GAME_INFO_FIELD_COUNT};

/// 字段名（serde 键），按 `docs/07` §3.4 的顺序。
///
/// `逐位`：`visit_str` 里逐个字符串比较（同一套手法见 `docs/07` §5.2 的枚举）。
pub const GAME_INFO_FIELDS: [&str; GAME_INFO_FIELD_COUNT] = [
    "room_name",
    "game_state",
    "current_player_count",
    "address",
    "axis_mode",
    "function_mode",
    "turn_mode",
    "time_mode",
    "locked",
];

/// LAN 广播里这个结构体的 serde 名字（`serialize_newtype_variant` 的第二个参数）。
///
/// `观测`：`GameInfo::serialize` 的调用者之一是
/// `<&mut serde_json::ser::Serializer as Serializer>::serialize_newtype_variant`
/// ⇒ `GameInfo` 在这条链上是**新类型变体的载荷**，包它的外层枚举是
/// `graphwar_event::GameInfoEvent`（判别值 62，`docs/07` §5.1）。
pub const SERDE_NEWTYPE_HOST: &str = "GameInfoEvent";

/// LAN 广播的判别值（`docs/07` §5.1 的 VARIANTS 表第 62 格，`逐位`）。
pub const GAME_INFO_EVENT_TAG: u16 = 62;
