//! `graphwar_event_handler.rs`：**事件名 → 判别值**的分派层（原版 `GraphwarEventHandler`）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **ELF vaddr**（Ghidra/REA 的反编译地址 = 本地址 + `0x100000`）。
//!
//! # 1. 为什么这个文件存在（边界证据）
//!
//! 本模块此前**没有同名文件**，内容混在 `src/events.rs`（`enum Event` + 本文件那张表 + `GameInfo`）。
//! T24 的逐函数边界审计（[`docs/06`](../../docs/06-verification.md) §74、
//! `fixtures/boundary_audit.tsv`）按 `panic::Location` 硬归属，判出两条 `mismatch`：
//!
//! | 地址 | 符号名（`fixtures/name_to_addr_v2.tsv` 三向匹配的真名） | `panic::Location` 主文件 | 锚点（原版行号） |
//! |---|---|---|---|
//! | `0x3d3630` | `GraphwarEventHandler::process_events` | `src/graphwar_event_handler.rs` | 1（`:467`） |
//! | `0x3d38f0` | `GraphwarEventHandler::subscribe_to_event` | 同上 | 3（`:396 :407 :454`） |
//!
//! 单模块、一边倒 ⇒ `AGENTS.md` §4「**文件名就是证据**」：原版有这个模块，我们就要有这个同名
//! `.rs`。本文件即按该证据从 `src/events.rs` 切出来的落点（T30 / `task-27`，2026-11-26）。
//!
//! # 2. 我方在这两个函数上有什么、没有什么
//!
//! | 原版函数 | 我方状态 | 落在哪 |
//! |---|---|---|
//! | `0x3d38f0` `subscribe_to_event`（名字 → 判别值） | **有**（表 + 查表端） | 本文件：`DISPATCH_NAME_TAGS` / `Event::from_dispatch_name` / `NAMED_VARIANT_COUNT` |
//! | `0x3d3630` `process_events`（把发件箱里的事件投给订阅者） | **没有独立函数体** | 泵的实现在 `src/_scaffold/root_wiring.rs`（`KernelBus::pump`，我方自造桥接、原版无对应物 ⇒ 按 §4 不进复原层）；抽象声明是 `src/graphwar_game_client.rs` 的 `EventHost::process_events`（那是 game_client 的 trait） |
//!
//! ⚠️ **不伪造 `process_events` 的函数体**：`0x3d3630` 的真实形态（订阅表容器、投递顺序、
//! 丢弃规则）只有 1 个 Location 锚，函数体未逐条还原 ⇒ 本文件只登记落点，不猜实现。
//!
//! # 3. 判别值从哪来（`0x3d38f0` 的四道自校验）
//!
//! `0x3d38f0` 先按 `len(name) - 10` 查 25 项跳表 `0x1be9c8`，再在每条长度分支里做逐字节相等
//! 比较（两条 `pcmpeqb`；短名字走 `movabs` + `xor` 的整数比较；len = 34 走 `memcmp`），
//! 命中后把判别值 `mov $N,%ebx`。72 条名字全部解出，两道自校验都过：
//!
//! 1. 每个相等测试的字节区间并集**恰好铺满** `[0, len)`（72/72，无空洞无冲突）；
//! 2. 17 条会顺手拼一个 `Event` 前缀的分支里，写进 `+0` 的 tag word
//!    `0x8000000000000000 | N` 与 `%ebx` **完全相等**（17/17）。
//!
//! 复现：`.reference/venv/bin/python tools/event_tags.py`；逐条证据地址见
//! [`docs/archive/114`](../../docs/archive/114-event-tags.md) §3 与下面 `DISPATCH_NAME_TAGS`
//! 的生成口径（「拿名字跟常量比」的那条指令）。
//!
//! # 4. 判别值不上线（这条仍然成立）
//!
//! 线上走的是 serde 的**外部标签 JSON**（`{"NameInfo":{…}}`），事件被写成 `{变体名: 字段}`。
//! 判别值既不出现在线路，也不出现在 replay 里（`docs/96` §4，380 份 replay 佐证）。
//! ⇒ 判别值是**内部保真**（`0x3d38f0` 的分派键），改它**不会**改变任何线格式。
//!
//! # 5. 公开路径
//!
//! `src/events.rs` 仍在 `pub use` 这两个名字，所以 `crate::events::DISPATCH_NAME_TAGS` /
//! `crate::events::NAMED_VARIANT_COUNT` / `Event::from_dispatch_name` 三条老路径**一个字都没变**
//! （调用方零改动，见 `fixtures/boundary_split_plan.txt` §2）。

use crate::events::Event;

/// `0x3d38f0` 里被比较的**名字**与判别值的对照（`docs/archive/114` §3）。
///
/// 这张表是「名字 → 判别值」两侧的唯一对照：左边是 `.rodata`/立即数里逐字节
/// 重建出来的名字，右边是该分支 `mov $N,%ebx` 的立即数。
/// 按判别值升序；**长度不是 72 就是抄错了**（`tests/events.rs` 会断言）。
pub const DISPATCH_NAME_TAGS: &[(&str, Event)] = &[
    ("tick_event", Event::TickEvent),
    ("tick_request_event", Event::TickRequest),
    ("tick_reply_event", Event::TickReply),
    ("subscribe_event", Event::Subscribe),
    ("new_connection_request_event", Event::NewConnectionRequest),
    ("new_connection_event", Event::NewConnection),
    ("connection_removed_event", Event::ConnectionRemoved),
    ("connected_to_server_event", Event::ConnectedToServer),
    ("new_player_request_event", Event::NewPlayerRequest),
    ("new_bot_request_event", Event::NewBotRequest),
    ("bot_added_event", Event::BotAdded),
    ("change_team_request_event", Event::ChangeTeamRequest),
    ("remove_player_request_event", Event::RemovePlayerRequest),
    ("player_info_event", Event::PlayerInfo),
    ("entity_removed_event", Event::EntityRemoved),
    ("soldier_info_event", Event::SoldierInfo),
    ("add_soldier_request_event", Event::AddSoldierRequest),
    ("remove_soldier_request_event", Event::RemoveSoldierRequest),
    ("axis_mode_change_request_event", Event::AxisModeChangeRequest),
    ("function_mode_change_request_event", Event::FunctionModeChangeRequest),
    ("turn_mode_change_request_event", Event::TurnModeChangeRequest),
    ("time_mode_change_request_event", Event::TimeModeChangeRequest),
    ("game_name_request_event", Event::GameNameRequest),
    ("target_info_event", Event::TargetInfo),
    ("team_info_event", Event::TeamInfo),
    ("remove_team_event", Event::RemoveTeam),
    ("pos_info_event", Event::PosInfo),
    ("life_info_event", Event::LifeInfo),
    ("life_queued_death_event", Event::LifeQueuedDeath),
    ("game_name_info_event", Event::GameNameInfo),
    ("game_state_info_event", Event::GameStateInfo),
    ("game_start_request_event", Event::GameStartRequest),
    ("function_fire_request_event", Event::FunctionFireRequest),
    ("function_fire_event", Event::FunctionFire),
    ("function_update_request_event", Event::FunctionUpdateRequest),
    ("function_info_event", Event::FunctionInfo),
    ("function_active_event", Event::FunctionActive),
    ("function_points_event", Event::FunctionPoints),
    ("functions_calculated_event", Event::FunctionsCalculated),
    ("clear_obstacles_event", Event::ClearObstacles),
    ("add_obstacle_event", Event::AddObstacle),
    ("destroy_obstacle_event", Event::DestroyObstacle),
    ("queue_destroy_obstacle_event", Event::QueueDestroyObstacle),
    ("turn_info_event", Event::TurnInfo),
    ("effect_info_event", Event::EffectInfo),
    ("axis_mode_info_event", Event::AxisModeInfo),
    ("function_mode_info_event", Event::FunctionModeInfo),
    ("turn_mode_info_event", Event::TurnModeInfo),
    ("time_mode_info_event", Event::TimeModeInfo),
    ("end_of_turn_info_event", Event::EndOfTurnInfo),
    ("skin_graphics_request_event", Event::SkinGraphicsRequest),
    ("skin_graphics_info_event", Event::SkinGraphicsInfo),
    ("face_graphics_request_event", Event::FaceGraphicsRequest),
    ("face_graphics_info_event", Event::FaceGraphicsInfo),
    ("hat_graphics_request_event", Event::HatGraphicsRequest),
    ("hat_graphics_info_event", Event::HatGraphicsInfo),
    ("entity_effect_info_event", Event::EntityEffectInfo),
    ("turn_limit_info_event", Event::TurnLimitInfo),
    ("turn_count_info_event", Event::TurnCountInfo),
    ("name_request_event", Event::NameRequest),
    ("name_info_event", Event::NameInfo),
    ("color_info_event", Event::ColorInfo),
    ("game_info_event", Event::GameInfo),
    ("game_removed_event", Event::GameRemoved),
    ("rank_request_event", Event::RankRequest),
    ("rank_info_event", Event::RankInfo),
    ("lock_request_event", Event::LockRequest),
    ("lock_info_event", Event::LockInfo),
    ("chat_message_request_event", Event::ChatMessageRequest),
    ("chat_message_event", Event::ChatMessage),
    ("ban_connection_request_event", Event::BanConnectionRequest),
    ("clear_bans_request_event", Event::ClearBansRequest),
];

/// 具名变体的个数（实测口径：`0x3d38f0` 解出 72 条）。
/// **有派发名字**（`*_event`，即 `0x3d38f0` 那张表里出现过的）的变体数 = 72。
///
/// ⚠️ 与 [`crate::events::VARIANT_COUNT`] 不同：`GameHeartbeat`(63) 与 `VersionInfo`(65) **没有**
/// `*_event` 名字、也不进名字分派表（它们只被序列化上线），所以枚举有 74 个、具名派发的只有 72 个。
pub const NAMED_VARIANT_COUNT: usize = 72;

/// `Event` 的**派发名查表端** —— `0x3d38f0` 里那一长串「比名字、命中就 `mov $N,%ebx`」的重建。
///
/// 为什么这个 `impl` 块在本文件而不在 `src/events.rs`：它的唯一证据就是 `0x3d38f0`
/// （`GraphwarEventHandler::subscribe_to_event`）那条函数，`panic::Location` 落在
/// `src/graphwar_event_handler.rs`（见模块文档 §1）。Rust 允许同 crate 的 `impl` 块分居两文件，
/// 所以类型留在 `events.rs`、查表端在这里，而**调用路径 `Event::from_dispatch_name` 不变**。
impl Event {
    /// 按 `0x3d38f0` 比较的 snake_case 名字反查变体（内部派发层用）。
    pub fn from_dispatch_name(name: &str) -> Option<Event> {
        DISPATCH_NAME_TAGS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, e)| *e)
    }
}
