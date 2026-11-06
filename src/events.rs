//! 联机事件枚举：判别值**已按真二进制逐条解出**（`docs/114`）。
//!
//! ## 判别值从哪来
//!
//! `0x3d38f0`（`graphwar_event_handler.rs`）是「事件名 → 判别值」的分派器：先按
//! `len(name) - 10` 查 25 项跳表 `0x1be9c8`，再在每条长度分支里做逐字节相等比较
//! （两条 `pcmpeqb`；短名字走 `movabs` + `xor` 的整数比较；len = 34 走 `memcmp`），
//! 命中后把判别值 `mov $N,%ebx`。72 条名字全部解出，两道自校验都过：
//!
//! 1. 每个相等测试的字节区间并集**恰好铺满** `[0, len)`（72/72，无空洞无冲突）；
//! 2. 17 条会顺手拼一个 `Event` 前缀的分支里，写进 `+0` 的 tag word
//!    `0x8000000000000000 | N` 与 `%ebx` **完全相等**（17/17）。
//!
//! 复现：`../.venv/bin/python tools/event_tags.py`。逐条证据地址见 `docs/114` §3
//! 与 `DISPATCH_NAME_TAGS` 的生成口径（那是「拿名字跟常量比」的那条指令）。
//!
//! ## ⚠️ 旧版本的「顺序 = `.rodata` 变体名表的位置」是错的
//!
//! 本文件此前把 `.rodata` 里一段**连续**的变体名
//! （`TickRequest TickReply Subscribe NewConnectionRequest …`）当成判别值顺序。
//! 实测两者**不一致**，而且差的不是同一个常数：
//!
//! | 变体 | 旧编号（串里的位置） | 实测判别值 |
//! |---|---|---|
//! | `PlayerInfo` | 10 | **13** |
//! | `EntityRemoved` | 11 | **14** |
//! | `SoldierInfo` | 12 | **15** |
//! | `FunctionPoints` | 31 | **37** |
//! | `FunctionsCalculated` | 32 | **38** |
//!
//! 原因是那段串**本身不全**：它没有 `TickEvent`、`NewPlayerRequest`、`BotAdded`、
//! `TeamInfo`、`LifeInfo`、`TurnInfo`、`GameStartRequest`、`Skin/Face/HatGraphicsInfo`、
//! `EntityEffectInfo`、`NameInfo`、`ClearBansRequest` 等变体，所以「位置」根本不是编号。
//! **旧编号整体作废**；`tests/events.rs` 原先那两条「连续 0..=32」的断言也一起作废。
//!
//! ## 判别值不上线（这条仍然成立）
//!
//! 线上走的是 serde 的**外部标签 JSON**（`{"NameInfo":{…}}`），事件被写成
//! `{变体名: 字段}`。判别值既不出现在线路，也不出现在 replay 里
//! （`docs/96` §4，380 份 replay 佐证）。⇒ 本文件的编号是**内部保真**，
//! 改它**不会**改变任何线格式；补事件仍然只需要「名字 + 字段」。
//!
//! ## 枚举现在是完整的 74 个（2026-11-07 收口）
//!
//! **`enum Event` 恰好 74 个变体，判别值 `0..=73`，一个不缺。**
//! 曾经缺的 `63` / `65` 已定死：`GameHeartbeat = 63`、`VersionInfo = 65`。
//!
//! 两条**独立机制**互校（`docs/07` §5.1，各 72/72 全中）：
//! ① x86 侧 `.data.rel.ro 0x961608` 的 serde `VARIANTS` 表（长度 74 来自
//!    `unknown_variant` 的 `mov ecx,0x4a`；元素 `(ptr,len)` 全在 `R_X86_64_RELATIVE` 里，
//!    **文件里是 0**，所以直读/`strings` 都拿不到）；
//! ② arm64 未 strip 侧 `GraphwarEvent::serialize`(`0x4695ac`) 的**按判别值索引的跳转表**
//!    (`0x19e770`，74 格)，`[63]` 臂写 `"GameHeartbeat"`、`[65]` 臂写 `"VersionInfo"`。
//!
//! **旧写法作废**：曾说"`enum Event` 至少有 75 个变体"并把 `0x800000000000004a`(74)
//! 当成第 75 个——那个常量不在 `GraphwarEvent` 的判别值空间里（是别处的 `HashMap<u64,_>` 键）。

/// 联机事件枚举。
///
/// 判别值 = `0x3d38f0` 里 `mov $N,%ebx` 的立即数（`docs/114` §3 全表）。
/// **现在连续了**：74 个变体覆盖 `0..=73`（63/65 已于 2026-11-07 定死，见模块文档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Event {
    /// `tick_event`
    TickEvent = 0,
    /// `tick_request_event`
    TickRequest = 1,
    /// `tick_reply_event`
    TickReply = 2,
    /// `subscribe_event`
    Subscribe = 3,
    /// `new_connection_request_event`
    NewConnectionRequest = 4,
    /// `new_connection_event`
    NewConnection = 5,
    /// `connection_removed_event`
    ConnectionRemoved = 6,
    /// `connected_to_server_event`
    ConnectedToServer = 7,
    /// `new_player_request_event`
    NewPlayerRequest = 8,
    /// `new_bot_request_event`
    NewBotRequest = 9,
    /// `bot_added_event`
    BotAdded = 10,
    /// `change_team_request_event`
    ChangeTeamRequest = 11,
    /// `remove_player_request_event`
    RemovePlayerRequest = 12,
    /// `player_info_event`
    PlayerInfo = 13,
    /// `entity_removed_event`
    EntityRemoved = 14,
    /// `soldier_info_event`
    SoldierInfo = 15,
    /// `add_soldier_request_event`
    AddSoldierRequest = 16,
    /// `remove_soldier_request_event`
    RemoveSoldierRequest = 17,
    /// `axis_mode_change_request_event`
    AxisModeChangeRequest = 18,
    /// `function_mode_change_request_event`（len = 34，唯一走 `memcmp` 的一条）
    FunctionModeChangeRequest = 19,
    /// `turn_mode_change_request_event`
    TurnModeChangeRequest = 20,
    /// `time_mode_change_request_event`
    TimeModeChangeRequest = 21,
    /// `game_name_request_event`
    GameNameRequest = 22,
    /// `target_info_event`
    TargetInfo = 23,
    /// `team_info_event`
    TeamInfo = 24,
    /// `remove_team_event`
    RemoveTeam = 25,
    /// `pos_info_event`
    PosInfo = 26,
    /// `life_info_event`
    LifeInfo = 27,
    /// `life_queued_death_event`
    LifeQueuedDeath = 28,
    /// `game_name_info_event`
    GameNameInfo = 29,
    /// `game_state_info_event`
    GameStateInfo = 30,
    /// `game_start_request_event`
    GameStartRequest = 31,
    /// `function_fire_request_event`
    FunctionFireRequest = 32,
    /// `function_fire_event`
    FunctionFire = 33,
    /// `function_update_request_event`
    FunctionUpdateRequest = 34,
    /// `function_info_event`
    FunctionInfo = 35,
    /// `function_active_event`
    FunctionActive = 36,
    /// `function_points_event`
    FunctionPoints = 37,
    /// `functions_calculated_event`
    FunctionsCalculated = 38,
    /// `clear_obstacles_event`
    ClearObstacles = 39,
    /// `add_obstacle_event`
    AddObstacle = 40,
    /// `destroy_obstacle_event`
    DestroyObstacle = 41,
    /// `queue_destroy_obstacle_event`
    QueueDestroyObstacle = 42,
    /// `turn_info_event`
    TurnInfo = 43,
    /// `effect_info_event`
    EffectInfo = 44,
    /// `axis_mode_info_event`
    AxisModeInfo = 45,
    /// `function_mode_info_event`
    FunctionModeInfo = 46,
    /// `turn_mode_info_event`
    TurnModeInfo = 47,
    /// `time_mode_info_event`
    TimeModeInfo = 48,
    /// `end_of_turn_info_event`
    EndOfTurnInfo = 49,
    /// `skin_graphics_request_event`
    SkinGraphicsRequest = 50,
    /// `skin_graphics_info_event`
    SkinGraphicsInfo = 51,
    /// `face_graphics_request_event`
    FaceGraphicsRequest = 52,
    /// `face_graphics_info_event`
    FaceGraphicsInfo = 53,
    /// `hat_graphics_request_event`
    HatGraphicsRequest = 54,
    /// `hat_graphics_info_event`
    HatGraphicsInfo = 55,
    /// `entity_effect_info_event`
    EntityEffectInfo = 56,
    /// `turn_limit_info_event`
    TurnLimitInfo = 57,
    /// `turn_count_info_event`
    TurnCountInfo = 58,
    /// `name_request_event`
    NameRequest = 59,
    /// `name_info_event`
    NameInfo = 60,
    /// `color_info_event`
    ColorInfo = 61,
    /// `game_info_event`
    GameInfo = 62,
    /// 心跳/保活。**判别值 63 已在 2026-11-07 定死**（`docs/07` §5.1）：
    /// x86 侧 `.data.rel.ro 0x961608` 的 serde `VARIANTS` 表 `[63]`、
    /// 与 arm64 侧 `GraphwarEvent::serialize`(`0x4695ac`) 的跳转表(`0x19e770`) `[63]` 臂
    /// （写 len 13 的 `"GameHeartbeat"`）两条独立机制互校 72/72。
    GameHeartbeat = 63,
    /// `game_removed_event`
    GameRemoved = 64,
    /// 版本握手。**判别值 65 同法定死**（两张表 `[65]` 一致，arm64 臂写 len 11 的 `"VersionInfo"`）。
    VersionInfo = 65,
    /// `rank_request_event`
    RankRequest = 66,
    /// `rank_info_event`
    RankInfo = 67,
    /// `lock_request_event`
    LockRequest = 68,
    /// `lock_info_event`
    LockInfo = 69,
    /// `chat_message_request_event`
    ChatMessageRequest = 70,
    /// `chat_message_event`
    ChatMessage = 71,
    /// `ban_connection_request_event`
    BanConnectionRequest = 72,
    /// `clear_bans_request_event`
    ClearBansRequest = 73,
}

/// `enum Event` 的**全部 74 个变体**，按判别值升序（2026-11-07 定死，`docs/07` §5.1）。
///
/// 用途：不经过 `DISPATCH_NAME_TAGS` 也能校验"某个判别值是否存在于枚举里"——
/// 这很重要，因为 `GameHeartbeat`(63) 与 `VersionInfo`(65) **没有** `*_event` 派发名。
pub const ALL_VARIANTS: [Event; VARIANT_COUNT] = [
    Event::TickEvent, Event::TickRequest, Event::TickReply, Event::Subscribe,
    Event::NewConnectionRequest, Event::NewConnection, Event::ConnectionRemoved, Event::ConnectedToServer,
    Event::NewPlayerRequest, Event::NewBotRequest, Event::BotAdded, Event::ChangeTeamRequest,
    Event::RemovePlayerRequest, Event::PlayerInfo, Event::EntityRemoved, Event::SoldierInfo,
    Event::AddSoldierRequest, Event::RemoveSoldierRequest, Event::AxisModeChangeRequest, Event::FunctionModeChangeRequest,
    Event::TurnModeChangeRequest, Event::TimeModeChangeRequest, Event::GameNameRequest, Event::TargetInfo,
    Event::TeamInfo, Event::RemoveTeam, Event::PosInfo, Event::LifeInfo,
    Event::LifeQueuedDeath, Event::GameNameInfo, Event::GameStateInfo, Event::GameStartRequest,
    Event::FunctionFireRequest, Event::FunctionFire, Event::FunctionUpdateRequest, Event::FunctionInfo,
    Event::FunctionActive, Event::FunctionPoints, Event::FunctionsCalculated, Event::ClearObstacles,
    Event::AddObstacle, Event::DestroyObstacle, Event::QueueDestroyObstacle, Event::TurnInfo,
    Event::EffectInfo, Event::AxisModeInfo, Event::FunctionModeInfo, Event::TurnModeInfo,
    Event::TimeModeInfo, Event::EndOfTurnInfo, Event::SkinGraphicsRequest, Event::SkinGraphicsInfo,
    Event::FaceGraphicsRequest, Event::FaceGraphicsInfo, Event::HatGraphicsRequest, Event::HatGraphicsInfo,
    Event::EntityEffectInfo, Event::TurnLimitInfo, Event::TurnCountInfo, Event::NameRequest,
    Event::NameInfo, Event::ColorInfo, Event::GameInfo, Event::GameHeartbeat,
    Event::GameRemoved, Event::VersionInfo, Event::RankRequest, Event::RankInfo,
    Event::LockRequest, Event::LockInfo, Event::ChatMessageRequest, Event::ChatMessage,
    Event::BanConnectionRequest, Event::ClearBansRequest,
];

/// `0x3d38f0` 里被比较的**名字**与判别值的对照（`docs/114` §3）。
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

impl Event {
    /// 判别值（`0x3d38f0` 里 `mov $N,%ebx` 的立即数）。
    ///
    /// **不要**把它写到线上去：线上是外部标签 JSON，没有数值判别值（`docs/96` §4）。
    pub const fn wire_tag(self) -> u16 {
        self as u16
    }

    /// 按 `0x3d38f0` 比较的 snake_case 名字反查变体（内部派发层用）。
    pub fn from_dispatch_name(name: &str) -> Option<Event> {
        DISPATCH_NAME_TAGS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, e)| *e)
    }
}

/// 具名变体的个数（实测口径：`0x3d38f0` 解出 72 条）。
/// **有派发名字**（`*_event`，即 `0x3d38f0` 那张表里出现过的）的变体数 = 72。
///
/// ⚠️ 与 [`VARIANT_COUNT`] 不同：`GameHeartbeat`(63) 与 `VersionInfo`(65) **没有**
/// `*_event` 名字、也不进名字分派表（它们只被序列化上线），所以枚举有 74 个、具名派发的只有 72 个。
pub const NAMED_VARIANT_COUNT: usize = 72;

/// `enum Event` 的**变体总数** = 74（判别值 `0..=73` 连续无缺号，2026-11-07 定死）。
pub const VARIANT_COUNT: usize = 74;

/// `GameInfo` 结构体（`docs/07` §3.1，观测到 **9 个字段**）。
///
/// 字段名来自 serde 字段名表；字段**类型**未证实，这里用的类型是按名字和
/// 上下文挑的最小表示，**不是**从二进制读出来的布局。
#[derive(Debug, Clone, Default)]
pub struct GameInfo {
    pub room_name: String,
    pub game_state: u8,
    pub current_player_count: u32,
    pub address: String,
    pub axis_mode: u8,
    pub function_mode: u8,
    pub turn_mode: u8,
    pub time_mode: u8,
    pub locked: bool,
}

/// 观测到的字段个数，供测试钉住，防止"顺手加一个字段"。
pub const GAME_INFO_FIELD_COUNT: usize = 9;
