//! 事件枚举的**形状**复原。
//!
//! 证据：`.rodata` 里这些变体名**连续排列**（`docs/07` §2），所以顺序即 Rust
//! `enum Event` 的声明顺序，也是 bincode 类编码的判别值顺序。
//!
//! 局限（必须记住）：
//! - 只有变体**顺序**是对二进制读取的结果；每个变体的**字段**只还原了一部分
//!   （见 `docs/07` §3），所以这里只表达"有哪些变体、谁在前"，不带字段。
//! - 字符串表尾部还有若干变体未被连续覆盖，本枚举**不完整**。
//! - 线格式（bincode / postcard / JSON）未证实，因此这里的编号**不应**被当作
//!   "线上字节"来用。

/// 联机事件枚举。判别值 = 在 `.rodata` 变体名表里的位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Event {
    TickRequest = 0,
    TickReply = 1,
    Subscribe = 2,
    NewConnectionRequest = 3,
    NewConnection = 4,
    ConnectionRemoved = 5,
    ConnectedToServer = 6,
    NewBotRequest = 7,
    ChangeTeamRequest = 8,
    RemovePlayerRequest = 9,
    PlayerInfo = 10,
    EntityRemoved = 11,
    SoldierInfo = 12,
    AddSoldierRequest = 13,
    RemoveSoldierRequest = 14,
    AxisModeChangeRequest = 15,
    FunctionModeChangeRequest = 16,
    TurnModeChangeRequest = 17,
    TimeModeChangeRequest = 18,
    GameNameRequest = 19,
    TargetInfo = 20,
    RemoveTeam = 21,
    PosInfo = 22,
    LifeQueuedDeath = 23,
    GameNameInfo = 24,
    GameStateInfo = 25,
    FunctionFireRequest = 26,
    FunctionFire = 27,
    FunctionUpdateRequest = 28,
    FunctionInfo = 29,
    FunctionActive = 30,
    FunctionPoints = 31,
    FunctionsCalculated = 32,
}

impl Event {
    /// 判别值，等于枚举位置。
    pub const fn wire_tag(self) -> u16 {
        self as u16
    }
}

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
