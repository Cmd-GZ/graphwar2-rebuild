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
//! | `0x3d38f0` `subscribe_to_event`（名字 → 判别值） | **有**（表 + 查表端 + 登记面） | 本文件：`DISPATCH_NAME_TAGS` / `Event::from_dispatch_name` / `NAMED_VARIANT_COUNT` / [`GraphwarEventHandler::subscribe_to_event`] |
//! | `0x3d3630` `process_events`（把发件箱里的事件投给订阅者） | **有落点**（T49 从 `_scaffold` 搬回） | 本文件：[`GraphwarEventHandler::process_events`]；抽象声明是 `src/graphwar_game_client.rs` 的 `EventHost::process_events`（那是 game_client 的 trait） |
//!
//! # 2b. `GraphwarEventHandler` 的订阅表（T49 搬回来的那部分）
//!
//! ⚠️ **T49 之前**：本文件只有"名字 → 判别值"那张表，订阅表与投递泵写在
//! `src/_scaffold/root_wiring.rs` 的 `KernelBus` 里（`KernelBus::subscribe` /
//! `KernelBus::pump`）—— 违反 `AGENTS.md` §4「文件名就是证据」（原版这两条函数的
//! `panic::Location` 主文件就是**本文件**）。T36 审计（`docs/06` §86.4）把这列为第 ③ 处。
//!
//! **现在**：[`GraphwarEventHandler`] 持有订阅表（名字 → 订阅者个数）、投递记录、泵次数；
//! `KernelBus` 只保留 `pub use` 来的类型 + **薄转发**，调用方（`net_events_class`、
//! `entity_drawer_class`、`graphwar_game_client::ProcessLoop`）一行没改。
//!
//! **差异（我方形状，照实登记）**：
//!
//! * 原版订阅者是**闭包**（`Vec<Box<dyn Fn(&GraphwarEvent)>>` 一类，`docs/16` §46 记的
//!   `[EntityDrawer+0x108]=1` 就是它们干的事），本层只记**个数**——因为我们没有一个
//!   统一的"事件对象"可传，消费者是各自回表里**拉**数据的（同一份 `docs/16` §46 的结论）；
//! * `0x3d3630` 的真实形态（订阅表容器、投递顺序、丢弃规则）**只有 1 个 Location 锚**
//!   ⇒ 本层只落实**已观测到**的两条语义：「按名字查表」「没有订阅者的丢掉」，
//!   不伪造顺序/容量细节（档位：**观测**）。
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

use std::collections::HashMap;

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

// ---------------------------------------------------------------------------
// 6. `GraphwarEventHandler` 本体（T49：订阅表 + 投递泵，从 `_scaffold/root_wiring.rs` 搬回）
// ---------------------------------------------------------------------------

/// 原版 `GraphwarEventHandler` 的**订阅面 + 投递面**。
///
/// 原版这个对象挂在 `GraphwarBackend` / `GraphwarGameClient` 那条链上（`docs/16` §46），
/// 它的两份职责都由本模块的两条函数给出：
///
/// * [`Self::subscribe_to_event`]（`0x3d38f0`）：事件名 → 判别值 → 往订阅表里登记；
/// * [`Self::process_events`]（`0x3d3630`）：把事件队列里的事件投给订阅表里有的那些。
///
/// ⚠️ 差异（见模块文档 §2b）：订阅者在本层只是**计数**，不是闭包；投递顺序照队列顺序，
/// 没订阅的丢掉。
#[derive(Debug, Default)]
pub struct GraphwarEventHandler {
    /// 事件名（`0x3d38f0` 那张表里的 72 个之一）→ 订阅者个数。
    subscribers: HashMap<&'static str, usize>,
    /// 已经投递给本地订阅者的事件（观测点；按投递顺序）。
    delivered: Vec<Event>,
    /// [`Self::process_events`] 被调过几次（观测点）。
    pumps: i64,
}

impl GraphwarEventHandler {
    /// 空表（原版构造点内联在 `GraphwarBackend` 的 `new` 里，没有独立符号）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 原版 `subscribe_to_event`（x86 **`0x3d38f0`**）：登记一个订阅。
    ///
    /// 名字必须是 [`DISPATCH_NAME_TAGS`] 里那 72 个之一（`0x3d38f0` 就是那张"比名字 →
    /// `mov $N,%ebx`"的表，两道自校验 72/72、17/17）；不在表里的名字**返回 `false`**
    /// （原版走 `len - 10` 跳表 `0x1be9c8` + 逐字节比较，落不到任何一支）。
    pub fn subscribe_to_event(&mut self, name: &str) -> bool {
        // 0x3d38f0 — 名字命中 72 条表里的一条就登记；未命中不登记
        match DISPATCH_NAME_TAGS.iter().find(|(n, _)| *n == name) {
            Some((n, _)) => {
                *self.subscribers.entry(n).or_insert(0) += 1;
                true
            }
            None => false,
        }
    }

    /// 原版 `process_events`（x86 **`0x3d3630`**）：把队列里的事件投给订阅者。
    ///
    /// 已观测到的两条语义（**观测**，`0x3d3630` 只有 1 个 Location 锚 `:467`）：
    /// **按名字查订阅表**、没人订阅的丢掉；投递顺序 = 队列顺序。
    ///
    /// 入参是本帧从队列里取走的那批事件（原版在体内调 `GraphwarEventQueue::send_buffer`；
    /// 本层把"取走"留给调用方，因为队列句柄不在本对象上——**我方接线**，照实登记）。
    /// 返回值 = 这一次真正投递出去的事件（按队列顺序）。
    pub fn process_events(&mut self, drained: Vec<Event>) -> Vec<Event> {
        // 0x3d3630 — 按名字查订阅表 → 有人订阅才投递（无人订阅的丢）
        self.pumps += 1;
        let mut out = Vec::new();
        for ev in drained {
            let subscribed = dispatch_name(ev).is_some_and(|n| self.subscribers.contains_key(n));
            if subscribed {
                self.delivered.push(ev);
                out.push(ev);
            }
        }
        out
    }

    /// 某个事件名上挂了几个订阅者（原版没有这个 getter；验收用）。
    pub fn subscriber_count(&self, name: &str) -> usize {
        self.subscribers.get(name).copied().unwrap_or(0)
    }

    /// 订阅表原样给出（`(名字, 个数)`，按名字排序 —— `HashMap` 序不可依赖）。
    pub fn subscribers(&self) -> Vec<(&'static str, usize)> {
        let mut out: Vec<(&'static str, usize)> =
            self.subscribers.iter().map(|(k, v)| (*k, *v)).collect();
        out.sort_unstable_by_key(|(k, _)| *k);
        out
    }

    /// 已投递事件（按投递顺序）。
    pub fn delivered(&self) -> &[Event] {
        &self.delivered
    }

    /// [`Self::process_events`] 被调过几次。
    pub fn pumps(&self) -> i64 {
        self.pumps
    }
}

/// 事件 → [`DISPATCH_NAME_TAGS`] 里的派发名（没有名字的两条 = `GameHeartbeat` / `VersionInfo`）。
///
/// 这是 `0x3d38f0` 那张表的**反向索引**（`process_events` 用它把事件对到订阅表），
/// 档位 **推断**：原版 `process_events` 是不是这么反查的，没有逐条指令证据（见模块文档 §2b）。
pub fn dispatch_name(event: Event) -> Option<&'static str> {
    DISPATCH_NAME_TAGS.iter().find(|(_, e)| *e == event).map(|(name, _)| *name)
}
