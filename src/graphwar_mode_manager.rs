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

use crate::events::Event;

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
///
/// # 事件流（任务 40 补，`wired` 的第二件）
///
/// `new`（x86 `0x5186c0`）**只订阅 1 条**（`观测`）：
///
/// ```text
/// 0x518753  lea rsi,'new_connection_event' / 0x518761 mov edx,0x14
/// 0x51875a  lea r8, slot 0x9626c8 (+0x20 = 0x9626e8 → 跳板 0x5186a0 → 0x5185a0 send_all_info_events)
/// 0x51876c  call [slot 0x9bac88] -> 0x3d38f0          ; subscribe_to_event
/// ```
///
/// ⚠️ 这与 `docs/16` §41 的"15 号槽 `assembled`"相比**多一条新事实**：
/// 该模块订阅的是 `new_connection_event`（不是四个 `*_mode_change_request`），
/// 也就是说"收到新连接就把三个模式重播一遍"，**不**从服务器遥控里订阅模式变更。
///
/// `new` 同时给出 32 字节布局（`观测`，`0x5186ff`–`0x51870b`，管理体基址 = `rax+0x18`）：
/// `+0x00` 弱自句柄 / `+0x08` 事件队列 `Rc` / `+0x10` 容器 `Weak` /
/// `+0x18` `word 0x0301`（= 两个模式字节 `0x01/0x03`，第三个 `+0x1a = 0`）。
/// 本类型只保留"三个模式"的语义，`+0x00..0x10` 那三个借用字段**不复原**（我方形状）。
///
/// 发方向 3 条（`观测`，arm64 `0x497dc8` / `0x497ec0` / `0x497e44` + `0x497f3c`）：
/// `AxisModeInfo`(45) / `FunctionModeInfo`(46) / `TimeModeInfo`(48)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphwarModeManager {
    pub axis_mode: AxisMode,
    pub function_mode: FunctionMode,
    pub time_mode: TimeMode,
    /// 订阅名（`new` 的那一条，`观测`）。
    pub subscribed: Vec<&'static str>,
    /// 发件箱（判别值；载荷见 [`ModeInfoEvent`]）。
    pub outbox: Vec<Event>,
}

impl Default for GraphwarModeManager {
    fn default() -> Self {
        Self::new(AxisMode::default(), FunctionMode::default(), TimeMode::default())
    }
}

/// `new` 订阅的那一条（`0x518753` 的 `lea rsi` + `mov edx, 0x14`）。
pub const SUBSCRIBED_EVENTS: [&str; 1] = ["new_connection_event"];

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

impl ModeInfoEvent {
    /// 这条事件在发件箱里对应的判别值（`docs/07` §3.3 的 45 / 46 / 48）。
    pub const fn event(self) -> Event {
        match self {
            ModeInfoEvent::AxisModeInfo(_) => Event::AxisModeInfo,
            ModeInfoEvent::FunctionModeInfo(_) => Event::FunctionModeInfo,
            ModeInfoEvent::TimeModeInfo(_) => Event::TimeModeInfo,
        }
    }
}

impl GraphwarModeManager {
    pub fn new(axis_mode: AxisMode, function_mode: FunctionMode, time_mode: TimeMode) -> Self {
        Self {
            axis_mode,
            function_mode,
            time_mode,
            subscribed: SUBSCRIBED_EVENTS.to_vec(),
            outbox: Vec::new(),
        }
    }

    /// `new_connection_event` 的处理（跳板 `0x5186a0` → `0x5185a0`）：三个模式全量重播。
    pub fn handle_new_connection_event(&mut self) -> [ModeInfoEvent; 3] {
        let events = self.send_all_info_events();
        self.outbox.extend(events.iter().map(|e| e.event()));
        events
    }

    /// 取走发件箱。
    pub fn take_outbox(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }

    /// 对应 `0x497dc8`。原版落值后**立即** `add_event`，不校验。
    pub fn set_axis_mode(&mut self, mode: AxisMode) -> ModeInfoEvent {
        self.axis_mode = mode;
        self.outbox.push(Event::AxisModeInfo);
        ModeInfoEvent::AxisModeInfo(mode)
    }

    /// 对应 `0x497ec0`。
    pub fn set_function_mode(&mut self, mode: FunctionMode) -> ModeInfoEvent {
        self.function_mode = mode;
        self.outbox.push(Event::FunctionModeInfo);
        ModeInfoEvent::FunctionModeInfo(mode)
    }

    /// 对应 `0x497e44`。
    pub fn set_time_mode(&mut self, mode: TimeMode) -> ModeInfoEvent {
        self.time_mode = mode;
        self.outbox.push(Event::TimeModeInfo);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 订阅方向：`new`（`0x5186c0`）只订一条 `new_connection_event`。
    #[test]
    fn new_registers_the_single_observed_subscription() {
        let m = GraphwarModeManager::new(AxisMode::EveryUnit, FunctionMode::NormalFunction, TimeMode::Timer1m);
        assert_eq!(m.subscribed, vec!["new_connection_event"]);
        assert_eq!(SUBSCRIBED_EVENTS.len(), 1, "`0x51876c` 只有一条 subscribe 调用点");
        assert_eq!(m.outbox_len(), 0);
        // `Default` 与 `new` 同一个形状（否则 `default()` 会静默丢掉订阅）
        assert_eq!(GraphwarModeManager::default().subscribed, m.subscribed);
    }

    /// 发方向：三个 setter 各发一条（45 / 46 / 48），落值都能读回。
    #[test]
    fn setters_write_back_and_emit() {
        let mut m = GraphwarModeManager::default();
        m.set_axis_mode(AxisMode::EveryFive);
        assert_eq!(m.axis_mode, AxisMode::EveryFive);
        assert_eq!(m.outbox, vec![Event::AxisModeInfo]);
        m.set_function_mode(FunctionMode::DiffEqFunction);
        assert_eq!(m.function_mode, FunctionMode::DiffEqFunction);
        assert_eq!(m.outbox.last(), Some(&Event::FunctionModeInfo));
        m.set_time_mode(TimeMode::Timer3m);
        assert_eq!(m.time_mode, TimeMode::Timer3m);
        assert_eq!(m.outbox.last(), Some(&Event::TimeModeInfo));
        assert_eq!(m.outbox_len(), 3);
        // 判别值与 `docs/07` §3.3 全表的 45 / 46 / 48 一致
        assert_eq!(
            (Event::AxisModeInfo.wire_tag(), Event::FunctionModeInfo.wire_tag(), Event::TimeModeInfo.wire_tag()),
            (45, 46, 48)
        );
        assert_eq!(m.take_outbox().len(), 3);
        assert_eq!(m.outbox_len(), 0);
    }

    /// `new_connection_event` 的处理 = 三个模式全量重播（跳板 `0x5186a0` → `0x5185a0`）。
    #[test]
    fn new_connection_replays_three_modes() {
        let mut m = GraphwarModeManager::new(AxisMode::OnlyMain, FunctionMode::NormalFunction, TimeMode::TimerInf);
        let replayed = m.handle_new_connection_event();
        assert_eq!(replayed.len(), 3);
        assert_eq!(replayed[0], ModeInfoEvent::AxisModeInfo(AxisMode::OnlyMain));
        assert_eq!(replayed[1], ModeInfoEvent::FunctionModeInfo(FunctionMode::NormalFunction));
        assert_eq!(replayed[2], ModeInfoEvent::TimeModeInfo(TimeMode::TimerInf));
        assert_eq!(m.outbox_len(), 3, "重播也要进发件箱");
        assert_eq!(m.axis_mode, AxisMode::OnlyMain, "重播不改表");
    }
}
