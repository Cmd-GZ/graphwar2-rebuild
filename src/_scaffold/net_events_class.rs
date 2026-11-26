//! **不是原版的东西**：客户端侧「线上事件 → 复原 manager」的**派发桥**。
//!
//! # 这一份补的是什么
//!
//! 原版客户端收到服务端事件后，由 **`GraphwarRemoteEventHandler::handle_server_remote_event`**
//! （x86 **`0x3e6df0`** / arm64 **`0x3e2830`**，分派落在 `tag - 2` 上）按判别值调到各个 manager 的
//! setter（每个 manager 的模块文档里都记着自己那几个调用点，见下表）。
//! 我方**这个模块没有复元**（`docs/02-module-map.md` §17 把 `graphwar_remote_event_handler.rs`
//! 记成"只落了枚举与名字表，171 个锚点的分发/订阅行为未实现"），于是客户端侧这条链路是**断的**：
//! 界面层（`client/src/net/session.gd::_apply`）只把少数事件落到自己的 `room` 字典上，
//! 复原出来的 22 个 manager 一个字节都收不到 —— 靶子/名字/外观/特效/实体删除全都到不了绘制层。
//!
//! 本文件就是那条**断链的接线**（scaffold，不是复原物）：
//!
//! ```text
//! client/src/net/session.gd（解析 JSON、管自己的 room 模型）
//!   └─ GwNetEvents.<事件名>(…字段…)          ← 本文件（一个方法 = 一条线上事件）
//!        └─ crate::_scaffold::root_wiring::KernelBus（22 个 manager 的容器）
//! ```
//!
//! # 逐事件的"原版落到哪个方法"（证据出处）
//!
//! | 线上事件（tag） | 本文件的方法 | 调到的 manager 方法 | 证据 | 档 |
//! |---|---|---|---|---|
//! | `PosInfo`(26) | [`GwNetEvents::pos_info`] | `GraphwarPlanePosManager::set_position` | `graphwar_plane_pos_manager` 模块文档"谁调 `set_position`"：`graphwar_remote_event_handler`（x86 `0x3e2830` 内 `0x3e3347`） | **观测** |
//! | `NameInfo`(60) | [`GwNetEvents::name_info`] | `GraphwarNameManager::set_name` | `graphwar_name_manager` §4：arm64 全量扫 `bl set_name` 只有两个调用点，其一 = `handle_server_remote_event` 的 `0x3e707c` | **观测**（负结果扫法） |
//! | `SkinGraphicsInfo`(51) | [`GwNetEvents::skin_graphics_info`] | `GraphwarGraphicsManager::set_skin_graphics` | `graphwar_graphics_manager` §4：`0x3e6f34` | **观测** |
//! | `FaceGraphicsInfo`(53) | [`GwNetEvents::face_graphics_info`] | `set_face_graphics` | 同上：`0x3e794c` | **观测** |
//! | `HatGraphicsInfo`(55) | [`GwNetEvents::hat_graphics_info`] | `set_hat_graphics` | 同上：`0x3e7f18` | **观测** |
//! | `SkinGraphicsInfo`(51) 的副作用 | 同上 | `GraphwarEntityEffectManager::add_entity_effect(id, Ash)` | `graphwar_game_state_manager` §1 第 6 条：订 `skin_graphics_info_event`，处理器 `0x4741a0` 发一条 tag 56 `{id, 3}`；`bot_` 前缀判据见 `graphwar_entity_effect_manager` §6.1 | **观测** |
//! | `EntityEffectInfo`(56) | [`GwNetEvents::entity_effect_info`] | `GraphwarEntityEffectManager::add_entity_effect` | `graphwar_entity_effect_manager` §5 第 6 行：`0x3e8554`（照抄载荷） | **观测** |
//! | `EffectInfo`(44) | [`GwNetEvents::effect_info`] | `GraphwarEffectManager::add_effect` | `graphwar_effect_manager` §1：`add_effect` 登记 + 广播 `EffectInfo` | **观测** |
//! | `EntityRemoved`(14) | [`GwNetEvents::entity_removed`] | 9 个 manager 各自的 `handle_entity_removed_event` | 每个模块文档里都有自己那条订阅（`graphwar_target_manager` 闭包 `0x3daca0` 断言判别值 14、`graphwar_team_manager` `0x3c1f40`/`0x499570`、`graphwar_entity_effect_manager` `0x3e9c58` …） | **观测**（逐 manager） |
//! | `RemoveTeam`(25) | [`GwNetEvents::remove_team`] | `TeamTable::remove_from_team` | `graphwar_team_manager` §1：`remove_from_team` "**删到了才**广播 `RemoveTeam`" ⇒ 收到同一条要在本地删同一格 | **观测** |
//! | `TeamInfo`(24) | [`GwNetEvents::team_info`] | `TeamTable::add_to_team` | 同上：`add_to_team`（`0x3c17d0`）写表 + 广播 `TeamInfo` | **观测** |
//! | `ColorInfo`(61) | [`GwNetEvents::color_info`] | `GraphwarColorManager::set_color` | `graphwar_color_manager` 模块文档：`remote_event_handler::handle_server_remote_event` → `set_color`（`0x3e7e9c`），服务端颜色**直接采纳** | **观测** |
//! | `SoldierInfo`(15) | [`GwNetEvents::soldier_info`] | `SoldierTable::add_soldier` | `graphwar_soldier_manager`：`add_soldier` 内联块 x86 `0x474a10` | **观测** |
//! | `TargetInfo`(23) | [`GwNetEvents::target_info`] | `GraphwarTargetManager::add_target` | `graphwar_target_manager` §1：`add_target` 无独立函数体（内联进 `0x46f310`），广播端是 `TargetInfo`；**客户端侧收到它调谁没有直接证据** | **推断**（缺 remote handler 分派表的逐位对照） |
//! | `LifeInfo`(27) | [`GwNetEvents::life_info`] | `LifeState::set_alive` | `graphwar_life_manager`：`set_alive` 是 `LifeInfo` 的唯一生产者；客户端侧收到后的落点**没有单独证据** | **推断** |
//! | `PlayerInfo`(13) | [`GwNetEvents::player_info`] | `GraphwarPlayerManager::add_player` | `graphwar_player_manager`：`add_player` 在 x86 侧被**内联进** `remote_event_handler` | **推断**（内联点未逐位对照） |
//! | `FunctionPoints`(37) | [`GwNetEvents::function_points`] | `FunctionTable::set_function_points` | `graphwar_game_state_manager` §1 第 5 条：订 `function_points_event`，处理器 `0x4758b0`（**只做转发**） | **观测**（订阅）/ **推断**（转给哪张表） |
//! | `FunctionActive`(36) | [`GwNetEvents::function_active`] | `FunctionTable::set_function_active_tick` | `graphwar_function_evolutionator` §2：`FunctionActive.owner_id` 就是 `soldier_id` | **推断** |
//! | `FunctionInfo`(35) | [`GwNetEvents::function_info`] | `FunctionTable::set_function`（文本走 `math_function::parse`） | `graphwar_function_manager`：`set_*` 会发 `FunctionInfo`；解析器 `0x4ed670` | **推断** |
//! | `TurnInfo`(43) | [`GwNetEvents::turn_info`] | `TurnManager::set_turn`（= `TurnData::set_turn` `0x4638f0`） | `graphwar_turn_manager`：`set_turn` 公告"这个玩家现在用这个士兵" | **观测**（写操作）/ **推断**（客户端侧入口） |
//! | `TurnModeInfo`(47) | [`GwNetEvents::turn_mode_info`] | `TurnManager::set_turn_mode`（`0x462d20`） | `graphwar_turn_manager`：写入点含远端事件处理器（`0x3e41ae`） | **观测** |
//! | `AxisModeInfo`(45) / `FunctionModeInfo`(46) / `TimeModeInfo`(48) | 三个 `*_mode_info` | `GraphwarModeManager::set_{axis,function,time}_mode` | `graphwar_mode_manager` 模块文档：三个 setter 的调用点都在 `handle_server_remote_event` 里 | **观测** |
//! | `FunctionFire`(33) | [`GwNetEvents::function_fire`] | **没有落点** | 见下 | **未知** |
//!
//! ## 为什么 `FunctionFire` 不硬接
//!
//! 客户端收到 `FunctionFire{soldier_id}` 之后原版做什么**没有证据**：它不在
//! `GraphwarGameStateManager` 订阅的 7 条里，也不是任何 manager 的广播输入（全库唯一生产者是
//! 服务端 `Match::fire`）。弹道动画由 `FunctionPoints` / `FunctionActive` / `EffectInfo` 三条驱动，
//! 所以本文件对这条只**记账**、不猜一个 manager 调用 —— 宁可留空，也不造一个"看起来接上了"的假链路。
//!
//! # 与原版的差别（诚实登记）
//!
//! 1. **分派载体**：原版是 `handle_server_remote_event` 里一张 `switch (tag - 2)`；本层是
//!    `#[func]` 一事件一方法（GDScript 侧解析 JSON 后按名调用）。判别值 → 方法名这张表在
//!    `fixtures/net_event_handlers.tsv`，与本文件同源。
//! 2. **载荷形态**：原版 `Event` 是 0x48 字节的 tagged union；本层直接吃解析后的字段。
//!    字段名/顺序/类型仍按 `src/_scaffold/protocol.rs::WIRE_SPEC`（74/74 逐位）。
//! 3. **manager 产出的事件**：原版 manager 拿得到那条**共享**队列（`GraphwarGameClient +0x1d8`）；
//!    本层每个 manager 只有自己的 `outbox`，所以这里显式把 `take_outbox()` 搬进
//!    [`KernelBus::queue`]（与 [`KernelBus::set_graphics`] 已有的搬法同一种，tag/顺序不变）。
//! 4. **`TeamTable` / `ObstacleTable` 的发件箱没有搬**：这两个的 outbox 是我方装配体的形状
//!    （`TeamEventKind` / 判别值），不是 `Event`；它们只在"广播给对端"时才有用，本桥用不到。

use godot::classes::RefCounted;
use godot::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use crate::_scaffold::root_wiring::{install_kernel_bus, kernel_bus};
#[cfg(test)]
use crate::_scaffold::root_wiring::{KernelBus, RootWiring};
#[cfg(test)]
use crate::events::Event;
#[cfg(test)]
use crate::graphwar_color_manager::GraphwarColor;
#[cfg(test)]
use crate::graphwar_effect_manager::EffectType;
use crate::graphwar_graphics_manager::GraphicsKind;
#[cfg(test)]
use crate::graphwar_mode_manager::{AxisMode, FunctionMode, TimeMode};
#[cfg(test)]
use crate::graphwar_team_manager::GraphwarTeam;
#[cfg(test)]
use crate::graphwar_turn_manager::TurnMode;

/// **T49**：派发层的语义已经搬回
/// [`crate::graphwar_remote_event_handler`]（原版 `GraphwarRemoteEventHandler::handle_client_remote_event`
/// `0x3dee00`）。本文件现在只剩 **GDExtension 外壳**（`GwNetEvents`，原版没有）。
///
/// 下面这行 `pub use` 保证老路径 `_scaffold::net_events_class::{WireEvent, Calls, dispatch, parse_*, is_bot_skin}`
/// **一个字都不用改** ⇒ 调用方零改动（`client/src/net/session.gd` 经 `GwNetEvents` 间接用）。
pub use crate::graphwar_remote_event_handler::{
    dispatch, is_bot_skin, parse_axis_mode, parse_effect_type, parse_function_mode, parse_team,
    parse_time_mode, parse_turn_mode, Calls, WireEvent,
};


/// 内核通道装了没有（界面层可用来把"链路断了"显示出来，而不是静默什么都不做）。
pub fn dispatch_installed() -> bool {
    kernel_bus().is_some()
}

/// 装上内核通道（幂等）。**必须先装再分派**，否则 [`dispatch`] 一条都落不下去。
pub fn ensure_installed() {
    let _ = install_kernel_bus();
}

// ---------------------------------------------------------------------------
// 5. GDExtension 外壳（一个 `#[func]` = 一条线上事件）
// ---------------------------------------------------------------------------

/// 客户端侧的事件派发桥（**scaffold**：原版是 `GraphwarRemoteEventHandler`）。
///
/// GDScript 用法（见 `client/src/net/session.gd`）：
///
/// ```text
/// var _net_events := GwNetEvents.new()      # 懒建；跨局复用
/// _net_events.reset()                        # 装内核通道（幂等）
/// _net_events.pos_info(230, -5.156, -9.889, 0.5)   # 返回调到的 manager 方法（诊断用）
/// ```
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwNetEvents {
    base: Base<RefCounted>,
    /// 已分派的条数（观测口）。
    applied: i64,
    /// 每类事件分派过几条（观测口）。
    per_name: std::collections::BTreeMap<&'static str, i64>,
    /// 最近一次分派调到的 manager 方法（换行分隔）。
    last_calls: String,
}

#[godot_api]
impl IRefCounted for GwNetEvents {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            base,
            applied: 0,
            per_name: std::collections::BTreeMap::new(),
            last_calls: String::new(),
        }
    }
}

#[godot_api]
impl GwNetEvents {
    /// 装内核通道（幂等：已装就复用，**不重置** manager 表）。
    ///
    /// 返回 `true` = 通道在（可以分派了）。
    #[func]
    fn reset(&mut self) -> bool {
        ensure_installed();
        self.applied = 0;
        self.per_name.clear();
        self.last_calls.clear();
        dispatch_installed()
    }

    /// 通道装了没有（没装 ⇒ 后面每个 `#[func]` 都返回空串）。
    #[func]
    fn installed(&self) -> bool {
        dispatch_installed()
    }

    /// 已分派的事件条数。
    #[func]
    fn applied(&self) -> i64 {
        self.applied
    }

    /// 某一类事件分派过几条。
    #[func]
    fn applied_of(&self, name: GString) -> i64 {
        let key = name.to_string();
        self.per_name.iter().find(|(k, _)| **k == key).map(|(_, v)| *v).unwrap_or(0)
    }

    /// 最近一次调到的 manager 方法（换行分隔；空串 = 这条事件没有落点）。
    #[func]
    fn last_calls(&self) -> GString {
        GString::from(self.last_calls.as_str())
    }

    /// 5 `NewConnection{connection_id}`。
    #[func]
    fn new_connection(&mut self, connection_id: i64) -> GString {
        self.record(WireEvent::NewConnection { connection_id })
    }

    /// 13 `PlayerInfo{player_id, connection_id}`。
    #[func]
    fn player_info(&mut self, player_id: i64, connection_id: i64) -> GString {
        self.record(WireEvent::PlayerInfo { player_id, connection_id })
    }

    /// 15 `SoldierInfo{soldier_id, player_id}`。
    #[func]
    fn soldier_info(&mut self, soldier_id: i64, player_id: i64) -> GString {
        self.record(WireEvent::SoldierInfo { soldier_id, player_id })
    }

    /// 23 `TargetInfo{target_id}`。
    #[func]
    fn target_info(&mut self, target_id: i64) -> GString {
        self.record(WireEvent::TargetInfo { target_id })
    }

    /// 24 `TeamInfo{entity_id, team}`。
    #[func]
    fn team_info(&mut self, entity_id: i64, team: GString) -> GString {
        self.record(WireEvent::TeamInfo { entity_id, team: &team.to_string() })
    }

    /// 25 `RemoveTeam{entity_id}`。
    #[func]
    fn remove_team(&mut self, entity_id: i64) -> GString {
        self.record(WireEvent::RemoveTeam { entity_id })
    }

    /// 14 `EntityRemoved{entity_id}`。
    #[func]
    fn entity_removed(&mut self, entity_id: i64) -> GString {
        self.record(WireEvent::EntityRemoved { entity_id })
    }

    /// 26 `PosInfo{entity_id, pos_x, pos_y, radius}`。
    #[func]
    fn pos_info(&mut self, entity_id: i64, pos_x: f64, pos_y: f64, radius: f64) -> GString {
        self.record(WireEvent::PosInfo { entity_id, x: pos_x, y: pos_y, radius })
    }

    /// 27 `LifeInfo{entity_id, alive}`。
    #[func]
    fn life_info(&mut self, entity_id: i64, alive: bool) -> GString {
        self.record(WireEvent::LifeInfo { entity_id, alive })
    }

    /// 28 `LifeQueuedDeath{entity_id, tick}`。
    #[func]
    fn life_queued_death(&mut self, entity_id: i64, tick: i64) -> GString {
        self.record(WireEvent::LifeQueuedDeath { entity_id, tick: tick.max(0) as u32 })
    }

    /// 40 `AddObstacle{pos_x, pos_y, radius}`。
    #[func]
    fn add_obstacle(&mut self, pos_x: f64, pos_y: f64, radius: f64) -> GString {
        self.record(WireEvent::AddObstacle { x: pos_x, y: pos_y, radius })
    }

    /// 39 `ClearObstacles{}`。
    #[func]
    fn clear_obstacles(&mut self) -> GString {
        self.record(WireEvent::ClearObstacles)
    }

    /// 42 `QueueDestroyObstacle{pos_x, pos_y, radius, tick}`。
    #[func]
    fn queue_destroy_obstacle(&mut self, pos_x: f64, pos_y: f64, radius: f64, tick: i64) -> GString {
        self.record(WireEvent::QueueDestroyObstacle { x: pos_x, y: pos_y, radius, tick })
    }

    /// 41 `DestroyObstacle{pos_x, pos_y, radius}`。
    #[func]
    fn destroy_obstacle(&mut self, pos_x: f64, pos_y: f64, radius: f64) -> GString {
        self.record(WireEvent::DestroyObstacle { x: pos_x, y: pos_y, radius })
    }

    /// 43 `TurnInfo{player_id, soldier_id}`。
    #[func]
    fn turn_info(&mut self, player_id: i64, soldier_id: i64) -> GString {
        self.record(WireEvent::TurnInfo { player_id, soldier_id })
    }

    /// 47 `TurnModeInfo{turn_mode}`。
    #[func]
    fn turn_mode_info(&mut self, turn_mode: GString) -> GString {
        self.record(WireEvent::TurnModeInfo { turn_mode: &turn_mode.to_string() })
    }

    /// 45 `AxisModeInfo{axis_mode}`。
    #[func]
    fn axis_mode_info(&mut self, axis_mode: GString) -> GString {
        self.record(WireEvent::AxisModeInfo { axis_mode: &axis_mode.to_string() })
    }

    /// 46 `FunctionModeInfo{function_mode}`。
    #[func]
    fn function_mode_info(&mut self, function_mode: GString) -> GString {
        self.record(WireEvent::FunctionModeInfo { function_mode: &function_mode.to_string() })
    }

    /// 48 `TimeModeInfo{time_mode}`。
    #[func]
    fn time_mode_info(&mut self, time_mode: GString) -> GString {
        self.record(WireEvent::TimeModeInfo { time_mode: &time_mode.to_string() })
    }

    /// 51 `SkinGraphicsInfo{entity_id, graphics_str}`。
    #[func]
    fn skin_graphics_info(&mut self, entity_id: i64, graphics_str: GString) -> GString {
        self.record(WireEvent::SkinGraphicsInfo {
            entity_id,
            graphics_str: &graphics_str.to_string(),
        })
    }

    /// 53 `FaceGraphicsInfo{entity_id, graphics_str}`。
    #[func]
    fn face_graphics_info(&mut self, entity_id: i64, graphics_str: GString) -> GString {
        self.record(WireEvent::FaceGraphicsInfo {
            entity_id,
            graphics_str: &graphics_str.to_string(),
        })
    }

    /// 55 `HatGraphicsInfo{entity_id, graphics_str}`。
    #[func]
    fn hat_graphics_info(&mut self, entity_id: i64, graphics_str: GString) -> GString {
        self.record(WireEvent::HatGraphicsInfo {
            entity_id,
            graphics_str: &graphics_str.to_string(),
        })
    }

    /// 56 `EntityEffectInfo{entity_id, effect_type}`。
    #[func]
    fn entity_effect_info(&mut self, entity_id: i64, effect_type: GString) -> GString {
        self.record(WireEvent::EntityEffectInfo { entity_id, effect_type: &effect_type.to_string() })
    }

    /// 60 `NameInfo{entity_id, name}`。
    #[func]
    fn name_info(&mut self, entity_id: i64, name: GString) -> GString {
        self.record(WireEvent::NameInfo { entity_id, name: &name.to_string() })
    }

    /// 61 `ColorInfo{entity_id, color:{r,g,b}}`。
    #[func]
    fn color_info(&mut self, entity_id: i64, r: i64, g: i64, b: i64) -> GString {
        self.record(WireEvent::ColorInfo {
            entity_id,
            r: r.clamp(0, 255) as u8,
            g: g.clamp(0, 255) as u8,
            b: b.clamp(0, 255) as u8,
        })
    }

    /// 33 `FunctionFire{soldier_id}`。
    #[func]
    fn function_fire(&mut self, soldier_id: i64) -> GString {
        self.record(WireEvent::FunctionFire { soldier_id })
    }

    /// 35 `FunctionInfo{owner_id, function, corrected_function}`。
    #[func]
    fn function_info(&mut self, owner_id: i64, function: GString, corrected: GString) -> GString {
        self.record(WireEvent::FunctionInfo {
            owner_id,
            function: &function.to_string(),
            corrected: &corrected.to_string(),
        })
    }

    /// 36 `FunctionActive{owner_id, active_tick}`。
    #[func]
    fn function_active(&mut self, owner_id: i64, active_tick: i64) -> GString {
        self.record(WireEvent::FunctionActive { owner_id, active_tick })
    }

    /// 37 `FunctionPoints{owner_id, start_x, start_y, end_x}`。
    #[func]
    fn function_points(&mut self, owner_id: i64, start_x: f64, start_y: f64, end_x: f64) -> GString {
        self.record(WireEvent::FunctionPoints { owner_id, start_x, start_y, end_x })
    }

    /// 44 `EffectInfo{effect_id, pos_x, pos_y, effect_type, start_tick}`。
    #[func]
    fn effect_info(
        &mut self,
        effect_id: i64,
        pos_x: f64,
        pos_y: f64,
        effect_type: GString,
        start_tick: i64,
    ) -> GString {
        self.record(WireEvent::EffectInfo {
            effect_id,
            x: pos_x,
            y: pos_y,
            effect_type: &effect_type.to_string(),
            start_tick,
        })
    }

    /// 自检用：把这条链路相关的 manager 表原样打出来（`client/tools` 的验收入口与
    /// `tools/frame_capture.py --flow match` 的日志都读它）。
    ///
    /// 键：`installed` / `applied` / `calls` / `names` / `skins` / `faces` / `hats` /
    /// `targets` / `positions` / `colors` / `teams` / `alive` / `entity_effects` / `effects` /
    /// `active_ticks` / `points`。
    #[func]
    fn dump(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(bus) = kernel_bus() else {
            d.set("installed", false);
            d.set("applied", self.applied);
            return d;
        };
        d.set("installed", true);
        d.set("applied", self.applied);
        d.set("calls", &GString::from(self.last_calls.as_str()));

        let wiring = bus.borrow();
        let w = wiring.wiring();

        let mut names = VarDictionary::new();
        if let Some(m) = w.name_manager() {
            let m = m.borrow();
            for id in 0..=4096u32 {
                if let Some(name) = m.name_of(id) {
                    names.set(id as i64, &GString::from(name));
                }
            }
        }
        d.set("names", &names);

        for (kind, key) in [
            (GraphicsKind::Skin, "skins"),
            (GraphicsKind::Face, "faces"),
            (GraphicsKind::Hat, "hats"),
        ] {
            let mut table = VarDictionary::new();
            if let Some(m) = w.graphics_manager() {
                let m = m.borrow();
                for (id, name) in m.table(kind).iter() {
                    table.set(*id as i64, &GString::from(name.as_str()));
                }
            }
            d.set(key, &table);
        }

        let mut targets: Array<i64> = Array::new();
        if let Some(m) = w.target_manager() {
            let m = m.borrow();
            for id in m.targets() {
                targets.push(i64::from(id));
            }
        }
        d.set("targets", &targets);

        let mut positions: Array<f64> = Array::new();
        if let Some(slot) = w.plane_pos_manager() {
            let inner = Rc::clone(&slot.borrow().0);
            let m = inner.borrow();
            for id in m.get_all_positions_sorted() {
                if let Some(p) = m.get_position(id) {
                    positions.push(f64::from(id));
                    positions.push(p[0]);
                    positions.push(p[1]);
                    positions.push(p[2]);
                }
            }
        }
        d.set("positions", &positions);

        let mut colors: Array<i64> = Array::new();
        if let Some(m) = w.color_manager() {
            let m = m.borrow();
            for id in 0..=4096u32 {
                if let Some(c) = m.color_of(id) {
                    colors.push(i64::from(id));
                    colors.push(i64::from(c.r));
                    colors.push(i64::from(c.g));
                    colors.push(i64::from(c.b));
                }
            }
        }
        d.set("colors", &colors);

        let mut teams = VarDictionary::new();
        if let Some(m) = w.team_manager() {
            let m = m.borrow();
            for id in 0..=4096u32 {
                if let Some(t) = m.get_team(id) {
                    teams.set(id as i64, t as i64);
                }
            }
        }
        d.set("teams", &teams);

        let mut alive = VarDictionary::new();
        if let Some(m) = w.life_manager() {
            let m = m.borrow();
            for id in m.get_all_entities() {
                alive.set(id as i64, i64::from(m.get_alive(id)));
            }
        }
        d.set("alive", &alive);

        let mut entity_effects = VarDictionary::new();
        if let Some(m) = w.entity_effect_manager() {
            let m = m.borrow();
            for id in 0..=4096u32 {
                if let Some(kind) = m.get(id) {
                    entity_effects.set(id as i64, i64::from(kind.code()));
                }
            }
        }
        d.set("entity_effects", &entity_effects);

        let mut effects: Array<i64> = Array::new();
        if let Some(m) = w.effect_manager() {
            let m = m.borrow();
            for eid in 0..=4096u32 {
                if let Some(e) = m.get(eid) {
                    effects.push(i64::from(eid));
                    effects.push(i64::from(e.effect_type.code()));
                    effects.push(i64::from(e.start_tick));
                }
            }
        }
        d.set("effects", &effects);

        let mut active_ticks = VarDictionary::new();
        let mut points = VarDictionary::new();
        if let Some(m) = w.function_manager() {
            let m = m.borrow();
            for id in 0..=4096u32 {
                if let Some(t) = m.table.active_tick_of(id) {
                    active_ticks.set(id as i64, i64::from(t));
                }
                if let Some((x, y)) = m.table.points_of(id) {
                    points.set(id as i64, Vector2::new(x as f32, y as f32));
                }
            }
        }
        d.set("active_ticks", &active_ticks);
        d.set("points", &points);
        d
    }
}

impl GwNetEvents {
    /// 分派一条事件，并把这次调到的 manager 方法记下来（诊断/自检用）。
    fn record(&mut self, ev: WireEvent<'_>) -> GString {
        let name = ev.name();
        let calls = dispatch(&ev);
        self.applied += 1;
        *self.per_name.entry(name).or_insert(0) += 1;
        self.last_calls = calls.join("\n");
        GString::from(self.last_calls.as_str())
    }
}

// ---------------------------------------------------------------------------
// 6. 单测（**不碰引擎**：只验"线上字段 → manager 方法"这张映射表与落表结果）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 装一条**新的**内核通道（每个测试一条，互不干扰）。
    fn fresh_bus() -> Rc<RefCell<KernelBus>> {
        crate::_scaffold::root_wiring::uninstall_kernel_bus();
        crate::_scaffold::root_wiring::install_kernel_bus()
    }

    /// 把 `bus` 里的容器借出来跑一段（测试里读表用）。
    fn with_wiring<T>(bus: &Rc<RefCell<KernelBus>>, f: impl FnOnce(&RootWiring) -> T) -> T {
        let borrowed = bus.borrow();
        f(borrowed.wiring())
    }

    /// `fixtures/frame_match_events.txt` 的 30 个事件（去掉纯记账的两条）。
    ///
    /// 坐标/取值**逐字照抄**那份预注册剧本（不是这里编的）。
    fn pitch() -> Vec<WireEvent<'static>> {
        vec![
            WireEvent::NewConnection { connection_id: 228 },
            WireEvent::PlayerInfo { player_id: 229, connection_id: 228 },
            WireEvent::TeamInfo { entity_id: 229, team: "Team1" },
            WireEvent::ColorInfo { entity_id: 229, r: 125, g: 104, b: 0 },
            WireEvent::SoldierInfo { soldier_id: 230, player_id: 229 },
            WireEvent::TeamInfo { entity_id: 230, team: "Team1" },
            WireEvent::EntityEffectInfo { entity_id: 230, effect_type: "DeathEffect" },
            WireEvent::SoldierInfo { soldier_id: 231, player_id: 229 },
            WireEvent::EntityRemoved { entity_id: 231 },
            WireEvent::RemoveTeam { entity_id: 231 },
            WireEvent::TargetInfo { target_id: 232 },
            WireEvent::TeamInfo { entity_id: 232, team: "Team2" },
            WireEvent::SkinGraphicsInfo { entity_id: 232, graphics_str: "target" },
            WireEvent::EntityEffectInfo { entity_id: 232, effect_type: "SparkEffect" },
            WireEvent::ClearObstacles,
            WireEvent::PosInfo {
                entity_id: 230,
                x: -5.156306989169929,
                y: -9.889980889323086,
                radius: 0.5,
            },
            WireEvent::PosInfo {
                entity_id: 232,
                x: 9.782072976693868,
                y: 4.595393963132992,
                radius: 0.5,
            },
            WireEvent::LifeInfo { entity_id: 230, alive: true },
            WireEvent::LifeInfo { entity_id: 232, alive: true },
            WireEvent::FunctionInfo {
                owner_id: 230,
                function: "",
                corrected: "",
            },
            WireEvent::TurnInfo { player_id: 229, soldier_id: 230 },
            WireEvent::NameInfo { entity_id: 229, name: "cmdgz" },
            WireEvent::SkinGraphicsInfo { entity_id: 229, graphics_str: "face_paint_2" },
            WireEvent::SkinGraphicsInfo { entity_id: 230, graphics_str: "face_paint_2" },
            WireEvent::FaceGraphicsInfo { entity_id: 229, graphics_str: "asian" },
            WireEvent::FaceGraphicsInfo { entity_id: 230, graphics_str: "asian" },
            WireEvent::HatGraphicsInfo { entity_id: 229, graphics_str: "asian_hat" },
            WireEvent::HatGraphicsInfo { entity_id: 230, graphics_str: "asian_hat" },
            WireEvent::FunctionFire { soldier_id: 230 },
            WireEvent::FunctionInfo {
                owner_id: 230,
                function: "x+2sin(20x)",
                corrected: "x+2sin(20x) - 3.694",
            },
            WireEvent::FunctionPoints {
                owner_id: 230,
                start_x: -5.156306989169929,
                start_y: -9.889980889323086,
                end_x: 2.5793570733273476,
            },
            WireEvent::FunctionActive { owner_id: 230, active_tick: 615 },
            WireEvent::EffectInfo {
                effect_id: 234,
                x: 2.5793570733273476,
                y: 0.8236080576517795,
                effect_type: "ExplosionEffect",
                start_tick: 1281,
            },
        ]
    }

    /// 12 条"原本没有处理器"的事件，这一轮**都必须**落到某个 manager 的方法上。
    #[test]
    fn the_twelve_previously_unhandled_events_all_land_in_a_manager() {
        let _bus = fresh_bus();
        let mut all: Vec<&'static str> = Vec::new();
        for ev in pitch() {
            all.extend(dispatch(&ev));
        }
        for must in [
            "target_manager::add_target",
            "name_manager::set_name",
            "graphics_manager::set_skin_graphics",
            "graphics_manager::set_face_graphics",
            "graphics_manager::set_hat_graphics",
            "entity_effect_manager::add_entity_effect",
            "plane_pos_manager::handle_entity_removed_event",
            "team_manager::remove_from_team",
            "function_manager::set_function",
            "function_manager::set_function_points",
            "function_manager::set_function_active_tick",
            "effect_manager::add_effect",
        ] {
            assert!(all.iter().any(|c| *c == must), "缺落点 {must}；实得 {all:?}");
        }
        // `FunctionFire`：**故意**没有落点（证据缺口），钉住它，谁改谁看见
        let fire = dispatch(&WireEvent::FunctionFire { soldier_id: 230 });
        assert!(fire.is_empty(), "FunctionFire 不该硬接一个 manager；实得 {fire:?}");
    }

    /// 落表结果（不是"调了方法"，是"表里真有那条数据"）。
    #[test]
    fn the_pitch_lands_in_the_manager_tables() {
        let bus = fresh_bus();
        for ev in pitch() {
            dispatch(&ev);
        }
        with_wiring(&bus, |w| {
            // 名字（`NameInfo{229,"cmdgz"}`）
            let name = w.name_manager().expect("name manager");
            assert_eq!(name.borrow().name_of(229), Some("cmdgz"));
            // 外观三兄弟：229/230 = face_paint_2 / asian / asian_hat；靶子 232 的 skin = target
            let g = w.graphics_manager().expect("graphics manager");
            assert_eq!(
                g.borrow().table(GraphicsKind::Skin).get(&230).map(String::as_str),
                Some("face_paint_2")
            );
            assert_eq!(
                g.borrow().table(GraphicsKind::Face).get(&229).map(String::as_str),
                Some("asian")
            );
            assert_eq!(
                g.borrow().table(GraphicsKind::Hat).get(&230).map(String::as_str),
                Some("asian_hat")
            );
            assert_eq!(
                g.borrow().table(GraphicsKind::Skin).get(&232).map(String::as_str),
                Some("target")
            );
            // 靶子表
            assert!(w.target_manager().expect("target manager").borrow().contains(232));
            // 位置表（230 的落点逐字照剧本）
            let slot = w.plane_pos_manager().expect("plane pos manager");
            let inner = Rc::clone(&slot.borrow().0);
            let pos = inner.borrow().get_position(230).expect("230 的位置");
            assert!((pos[0] + 5.156306989169929).abs() < 1e-12);
            // 队伍：229/230 在 Team1，232 在 Team2；231 被 `RemoveTeam` 删过 ⇒ 查不到
            let team = w.team_manager().expect("team manager");
            assert_eq!(team.borrow().get_team(229), Some(GraphwarTeam::Team1));
            assert_eq!(team.borrow().get_team(232), Some(GraphwarTeam::Team2));
            assert_eq!(team.borrow().get_team(231), None);
            // 实体效果：230 Death / 232 Spark
            let ee = w.entity_effect_manager().expect("entity effect manager");
            assert_eq!(ee.borrow().get(230), Some(EffectType::Death));
            assert_eq!(ee.borrow().get(232), Some(EffectType::Spark));
            // 特效：234 Explosion @1281
            let fx = w.effect_manager().expect("effect manager");
            assert_eq!(fx.borrow().last_effect_info(234).map(|i| i.start_tick), Some(1281));
            // 函数槽：230 的 active_tick 与起点
            let f = w.function_manager().expect("function manager");
            assert_eq!(f.borrow().table.active_tick_of(230), Some(615));
            assert!(f.borrow().table.points_of(230).is_some());
            // 回合：TurnInfo{229,230}
            assert_eq!(w.turn_manager().expect("turn manager").borrow().current_player(), Some(229));
            // 颜色（ColorInfo{229,125,104,0}）
            let c = w.color_manager().expect("color manager");
            assert_eq!(c.borrow().color_of(229), Some(GraphwarColor::new(125, 104, 0)));
        });
    }

    /// `EntityRemoved` 的扇出：9 个 manager 各一次（顺序固定，便于日后对拍）。
    #[test]
    fn entity_removed_fans_out_to_every_manager() {
        let _bus = fresh_bus();
        let calls = dispatch(&WireEvent::EntityRemoved { entity_id: 231 });
        assert_eq!(
            calls,
            vec![
                "plane_pos_manager::handle_entity_removed_event",
                "name_manager::handle_entity_removed_event",
                "graphics_manager::handle_entity_removed_event",
                "target_manager::handle_entity_removed_event",
                "entity_effect_manager::handle_entity_removed_event",
                "life_manager::handle_entity_removed_event",
                "color_manager::handle_entity_removed_event",
                "team_manager::handle_entity_removed_event",
                "turn_manager::handle_entity_removed_event",
            ]
        );
    }

    /// 线上字符串只认观测过的拼写；**不认识的返回 `None`**（不猜）。
    #[test]
    fn wire_strings_only_accept_observed_spellings() {
        assert_eq!(parse_team("Team2"), Some(GraphwarTeam::Team2));
        assert_eq!(parse_team("team2"), None);
        assert_eq!(parse_turn_mode("SequentialTurns"), Some(TurnMode::SequentialTurns));
        assert_eq!(parse_axis_mode("EveryUnit"), Some(AxisMode::EveryUnit));
        assert_eq!(parse_axis_mode("Every5Units"), None);
        assert_eq!(parse_function_mode("NormalFunction"), Some(FunctionMode::NormalFunction));
        assert_eq!(parse_time_mode("TimerInf"), Some(TimeMode::TimerInf));
        assert_eq!(parse_time_mode("Timer2m"), Some(TimeMode::Timer2m));
        assert_eq!(parse_effect_type("ExplosionEffect"), Some(EffectType::Explosion));
        assert_eq!(parse_effect_type("Spark"), None);
        assert!(is_bot_skin("bot_3"));
        assert!(!is_bot_skin("bot_"));
        assert!(!is_bot_skin("face_paint_2"));
    }

    /// 判别值表与 `protocol.rs::WIRE_SPEC` 同源（事件名 → tag 一个不多一个不少）。
    #[test]
    fn tags_match_the_wire_spec() {
        for ev in pitch() {
            let spec = crate::_scaffold::protocol::wire_spec(ev.name())
                .unwrap_or_else(|| panic!("{} 不在 WIRE_SPEC 里", ev.name()));
            assert_eq!(spec.tag, Some(ev.tag()), "{} 的 tag 对不上", ev.name());
        }
    }
}
