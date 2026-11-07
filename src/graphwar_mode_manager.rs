//! 原版 `src/graphwar_mode_manager.rs` —— **三套"模式"的持有者**。
//!
//! ## 一句话结论（2026-11-07，`docs/16-client.md` §30.4）
//!
//! `GraphwarModeManager` 拿的是 **`AxisMode` / `FunctionMode` / `TimeMode` 三套**，
//! **`TurnMode` 不在这个模块里** —— 它在 `graphwar_turn_manager`
//! （arm64 符号 `graphwar_turn_manager::GraphwarTurnManager::set_turn_mode`；
//! 它的 `TurnMode` 序列化臂在 x86 目标 `0x479310`，`docs/07` §5.3.3）。
//! 所以「四套模式一个主人」是**错的**：
//! 3 + 1 两个主人，`docs/16-client.md` §16 那张"四格模式"表指的是**界面上的四个按钮**，
//! 不是同一个 Rust 结构体。
//!
//! ## 谁写它（arm64 2.0.60 符号表 + `bl` 图，`观测`）
//!
//! | 方法 | 唯一的直接调用者 |
//! |---|---|
//! | `set_axis_mode` | `GraphwarRemoteEventHandler::handle_server_remote_event` |
//! | `set_function_mode` | `GraphwarRemoteEventHandler::handle_server_remote_event` |
//! | `set_time_mode` | `GraphwarLevelManager::setup_level`、`GraphwarRemoteEventHandler::handle_server_remote_event` |
//!
//! 三个 setter 的**函数体是同一个形状**（各 124 字节）：
//! `borrow_mut → 写 1 个字节 → GraphwarEventQueue::add_event ×1`。
//! 也就是说 setter **只负责落值和广播**，没有任何校验、没有 `TurnMode`、没有"重算"。
//!
//! ## 它是不是"当前模式的唯一真相"？——**不是**
//!
//! 三条否证（都是 `观测`）：
//!
//! 1. **写入者是"服务器遥控"**：`handle_server_remote_event` 是**收到服务器事件**才写，
//!    客户端的 UI 按钮不直接写这里——它走
//!    `GraphwarGameStateManager::request_{axis,function,turn,time}_mode_change`
//!    → 发 `AxisModeChangeRequest`(18)/`FunctionModeChangeRequest`(19)/
//!    `TurnModeChangeRequest`(20)/`TimeModeChangeRequest`(21)（`docs/07` §4）→
//!    **服务器**回 `AxisModeInfo`(45) 等才落值。所以权威在服务器，本模块是**副本**。
//! 2. **同一个值还有第二个副本**：`GraphwarSetupControl` 的按钮图标
//!    （`AxisModeButton`/`TimeModeButton` …）由 `GraphwarTextureLoader::get_time_mode_texture`
//!    按模式挑 `res://turn_30s.svg` 这一类贴图（`docs/16-client.md` §28）。
//! 3. **第 4 套模式在主人在别处**，见上。
//!
//! ## 原版函数清单（arm64 符号，`观测`）
//!
//! `GraphwarModeManager`: `new`、`set_axis_mode`、`set_function_mode`、`set_time_mode`、
//! `send_all_info_events`、`subscribe_to_events_event_handler`（只有 `{{closure}}`）。
//! 另有 `AxisMode`/`FunctionMode`/`TimeMode` 的 serde `serialize`/`deserialize`
//! （枚举变体取值见 `docs/07` §5.2，**本模块不重抄**）。
//!
//! 注意 `0x4b07a0` 上**三个** `__FieldVisitor::expecting` 是同一地址（`14` 字节的
//! 同一个函数体）——链接器把三个枚举的同形小函数合并了，别误读成"只有一个枚举"。

/// `axis_mode`（`docs/07` §5.2，判别值 = 声明顺序，`逐位`）。
///
/// 语义：坐标轴网格画多密。`OnlyMain` 只画主轴，`EveryUnit` 每格都画。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AxisMode {
    #[default]
    NoAxis = 0,
    OnlyMain = 1,
    EveryFive = 2,
    EveryUnit = 3,
}

/// `function_mode`（`docs/07` §5.2，`逐位`）。2 个变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum FunctionMode {
    #[default]
    NormalFunction = 0,
    DiffEqFunction = 1,
}

/// `time_mode`（`docs/07` §5.2，`逐位`）。6 个变体，**按秒数升序**，最后一个是无限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum TimeMode {
    #[default]
    Timer30s = 0,
    Timer1m = 1,
    Timer2m = 2,
    Timer3m = 3,
    Timer5m = 4,
    TimerInf = 5,
}

impl TimeMode {
    /// 每回合的秒数；`TimerInf` 没有时限（原版 `turn_inf.svg`，无倒计时）。
    pub const fn seconds(self) -> Option<u32> {
        match self {
            TimeMode::Timer30s => Some(30),
            TimeMode::Timer1m => Some(60),
            TimeMode::Timer2m => Some(120),
            TimeMode::Timer3m => Some(180),
            TimeMode::Timer5m => Some(300),
            TimeMode::TimerInf => None,
        }
    }

    /// 界面上那张图标的资源名（`docs/16-client.md` §28 的 `.rodata` 实测值）。
    pub const fn icon(self) -> &'static str {
        match self {
            TimeMode::Timer30s => "res://turn_30s.svg",
            TimeMode::Timer1m => "res://turn_1m.svg",
            TimeMode::Timer2m => "res://turn_2m.svg",
            TimeMode::Timer3m => "res://turn_3m.svg",
            TimeMode::Timer5m => "res://turn_5m.svg",
            TimeMode::TimerInf => "res://turn_inf.svg",
        }
    }
}

/// 三个模式的**同一个**持有者。
///
/// `TurnMode` 故意**不在这里**（见模块文档）。加进来会与
/// `graphwar_turn_manager` 形成两个真相，而机器码里没有第二个副本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GraphwarModeManager {
    pub axis_mode: AxisMode,
    pub function_mode: FunctionMode,
    pub time_mode: TimeMode,
}

/// `send_all_info_events` 一次吐哪三个事件（判别值取自 `docs/07` §4）。
///
/// 顺序照 `GraphwarModeManager::send_all_info_events` 的函数体
/// （arm64 `0x497f3c`，296 字节，3 条 `add_event`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeInfoEvent {
    /// 判别值 45，字段 `axis_mode`
    AxisModeInfo(AxisMode),
    /// 判别值 46，字段 `function_mode`
    FunctionModeInfo(FunctionMode),
    /// 判别值 48，字段 `time_mode`
    TimeModeInfo(TimeMode),
}

impl GraphwarModeManager {
    pub fn new(axis_mode: AxisMode, function_mode: FunctionMode, time_mode: TimeMode) -> Self {
        Self { axis_mode, function_mode, time_mode }
    }

    /// 对应 `0x497dc8`。原版落值后**立即** `add_event`，不校验。
    pub fn set_axis_mode(&mut self, mode: AxisMode) -> ModeInfoEvent {
        self.axis_mode = mode;
        ModeInfoEvent::AxisModeInfo(mode)
    }

    /// 对应 `0x497ec0`。
    pub fn set_function_mode(&mut self, mode: FunctionMode) -> ModeInfoEvent {
        self.function_mode = mode;
        ModeInfoEvent::FunctionModeInfo(mode)
    }

    /// 对应 `0x497e44`。
    pub fn set_time_mode(&mut self, mode: TimeMode) -> ModeInfoEvent {
        self.time_mode = mode;
        ModeInfoEvent::TimeModeInfo(mode)
    }

    /// 对应 `0x497f3c`：三个事件按 `45 / 46 / 48` 顺序全量广播。
    pub fn send_all_info_events(&self) -> [ModeInfoEvent; 3] {
        [
            ModeInfoEvent::AxisModeInfo(self.axis_mode),
            ModeInfoEvent::FunctionModeInfo(self.function_mode),
            ModeInfoEvent::TimeModeInfo(self.time_mode),
        ]
    }
}
