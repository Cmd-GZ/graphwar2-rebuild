//! **不是原版的东西**：`GraphwarModuleContainer` 的**装载器** —— 按原版
//! `GraphwarGameClient::new` 的**注册序**把 **22 个 manager** 装进容器，并只走 `TypeId` 取回。
//!
//! # 这一份解决什么
//!
//! 容器本体（[`crate::graphwar_module_container::GraphwarModuleContainer`]，
//! `HashMap<TypeId, Rc<dyn Any>>` / 48 字节 / 3 个方法）已经有实现、有 17 条结构断言，
//! 但**没有任何东西往里塞过模块**：原版的 25 个 `add_module` 全在**别的模块**里
//! （`GraphwarGameClient::new` 22 条、`game_server_thread` 20 条、
//! `GraphwarBackend::start_*` 各 1 条，见 `graphwar_module_container::REGISTRATION_SITES`）。
//!
//! 第一轮（`docs/16-client.md` §38）只装了 3 个（`GameStateManager` / `TurnManager` /
//! `PlayerManager`），而且**顺序是我方挑的**。本轮（§41）把**顺序 + 逐条类型**都对到了
//! 指令级证据上，并装齐 22 条。
//!
//! ```text
//! 原版 GraphwarGameClient::new（arm64 0x409c70 / x86 0x517690）22 条 add_module
//!   ├─ 每条的型 = 调用点前一条 `bl <Type>::new` 的符号名（arm64 .symtab 原文）
//!   ├─ 每条的型 = add_module 里那个 `dyn Any` vtable 的 drop glue 名（交叉复核；
//!   │             3 个 72 字节型的 drop glue 被 LLVM 合并，见 §41 的"合并"一节）
//!   └─ 顺序 = 调用点地址序；两侧的 `size_of::<RefCell<M>>` 序列 22 个字**逐位相等**
//! ```
//!
//! # 22 条注册序（`Wired::ALL`，与 [`SLOTS`] 一一对应）
//!
//! | # | 原版类型 | `size_of::<M>()` | arm64 `<Type>::new` | 我方类型 | 档位 |
//! |---|---|---|---|---|---|
//! | 1 | `GraphwarTickHandler` | 72 | `0x40b528` | —（空占位） | 类型**观测**；无实现 |
//! | 2 | `GraphwarClientConnectionHandler` | 112 | `0x48b048` | —（空占位） | 类型**观测**；无实现 |
//! | 3 | `GraphwarLocalPlayerManager` | 64 | 内联（`0x409eac` 处 `alloc 0x58`） | [`GraphwarLocalPlayerManager`] | 类型**观测** |
//! | 4 | `GraphwarPlayerManager` | 72 | `0x49d170` | [`GraphwarPlayerManager`] | 类型**观测** |
//! | 5 | `GraphwarSoldierManager` | 72 | `0x49d590` | [`SoldierTable`] | 类型**观测** |
//! | 6 | `GraphwarPlanePosManager` | 24 | `0x3b89a0` | —（空占位） | 类型**观测**；无实现 |
//! | 7 | `GraphwarGameStateManager` | 296 | `0x4ade6c` | [`Match`] | 类型**观测** |
//! | 8 | `GraphwarFunctionManager` | 32 | 内联（`Rc::new_cyclic_in` `0x40a158`） | —（空占位） | 类型**观测**；无实现 |
//! | 9 | `GraphwarTargetManager` | 72 | `0x3dae34` | [`GraphwarTargetManager`] | 类型**观测** |
//! | 10 | `GraphwarTeamManager` | 32 | `0x49960c` | [`TeamTable`] | 类型**观测** |
//! | 11 | `GraphwarLifeManager` | 72 | `0x4977b8` | [`LifeState`] | 类型**观测** |
//! | 12 | `GraphwarObstacleManager` | 80 | `0x426300` | —（空占位） | 类型**观测**；无实现 |
//! | 13 | `GraphwarTurnManager` | 32 | `0x49adf8` | [`TurnManager`] | 类型**观测** |
//! | 14 | `GraphwarEffectManager` | 72 | `0x40bc0c` | [`GraphwarEffectManager`] | 类型**观测** |
//! | 15 | `GraphwarModeManager` | 32 | `0x498090` | [`GraphwarModeManager`] | 类型**观测** |
//! | 16 | `GraphwarGraphicsManager` | 168 | `0x49e394` | [`GraphwarGraphicsManager`] | 类型**观测** |
//! | 17 | `GraphwarEntityEffectManager` | 72 | `0x3e9f40` | [`GraphwarEntityEffectManager`] | 类型**观测** |
//! | 18 | `GraphwarComputerPlayerManager` | 72 | `0x4b02a4` | [`ComputerPlayers`] | 类型**观测** |
//! | 19 | `FunctionHistoryRecorder` | 48 | `0x49d9a0` | [`History`] | 类型**观测** |
//! | 20 | `GraphwarNameManager` | 72 | `0x40af68` | [`GraphwarNameManager`] | 类型**观测** |
//! | 21 | `GraphwarColorManager` | 120 | `0x49cbe8` | [`GraphwarColorManager`] | 类型**观测** |
//! | 22 | `GraphwarRankManager` | 72 | `0x498d2c` | [`GraphwarRankManager`] | 类型**观测** |
//!
//! **顺序的档位是"逐位"**：两个二进制里 22 条调用的
//! `size_of::<RefCell<M>>` 序列完全相同
//! （`80,120,72,80,80,32,304,40,80,40,80,88,40,80,40,176,80,80,56,80,128,80`），
//! 逐字对得上，所以"位置 → 尺寸"没有错位的余地；"尺寸 → 型"由 arm64 的符号给出。
//!
//! **22 条里没有重复型**（22 个型各一次；25 型中客户端注册 22 个，
//! 缺的 3 个是 `GraphwarBanList` / `GraphwarServerConnectionHandler` / `GraphwarGameRecorder`
//! —— 全 `.text` 扫下来，这 3 个的 `add_module` 调用点只在
//! `game_server_thread`（20 条，含前两个）与 `GraphwarBackend::start_*`（各 1 条）里）。
//!
//! # `Wired` 的两档：真类型 / 空占位
//!
//! * [`SlotKind::RealType`]（**17** 个）：容器里装的是**本仓库已有的复原类型**
//!   （各自模块里那个结构体）。**行为不完整**——本文件只做"构造 + 注册"，
//!   不驱动任何 manager 的逻辑（任务书 §4 明确禁止在本轮实现那 19 个的行为）。
//! * [`SlotKind::Stub`]（**5** 个：`TickHandler` / `ClientConnectionHandler` /
//!   `PlanePosManager` / `FunctionManager` / `ObstacleManager`）：这 5 个原版型在本仓库
//!   **没有可构造的复原类型**（`graphwar_tick_handler.rs` 只有两个自由函数；
//!   `graphwar_plane_pos_manager` / `graphwar_function_manager` / `graphwar_obstacle_manager`
//!   连文件都没有，逻辑分别散在 `src/plane.rs` / `src/game.rs`；`client_connection_handler`
//!   只有常量偏移表）。按 `AGENTS.md` §0 第 3 条，**不编一个像样的 manager**，
//!   只放一个**零行为的空占位** `XxxSlot`，并在 [`Wired::status`] 里如实报 `"stub"`
//!   （**不是** `"assembled"`）。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 注册几个 | `GraphwarGameClient::new` **22** 条 `add_module` | **22**（17 真类型 + 5 空占位） | 22 是**逐位**（§41）；17/5 是我方现状 |
//! | 注册顺序 | 22 条的调用序 | **同序**（[`Wired::ALL`] / [`SLOTS`]） | **逐位**（两架构尺寸序列相等） |
//! | 谁在装 | `GraphwarGameClient::new`（客户端对象） | 本文件（`GraphwarRootNode` 调的 `assemble_demo`） | 装载者**不同**，容器语义相同 |
//! | 各 manager 是否**共享**实例 | `GameStateManager` 内部是否也持 `TurnManager`：**未解** | **不共享**：容器里是各自独立实例；`Match` 自带一个内部 `TurnManager` / `LifeState` / `GraphwarEntityEffectManager` | 独立性是**我方形状**；原版是否共享 `未知` |
//! | 尺寸 | `size_of::<M>()` = 24 … 296（[`SLOTS`] 的 `size`） | 我方类型尺寸**与之不等**（例：`Match` 远大于 296） | 原版尺寸**观测**；我方差异是**必然**（结构未按字节复原） |
//! | 构造参数 | 各 `new` 的实参（大多来自 `GameClient` 的字段/配置） | 演示数据（[`DEMO_SOLDIERS`] / [`DEMO_PLAYERS`] / [`demo_room`] / [`DEMO_LOCAL_PROFILE`]） | **我方选择**（假数据） |
//! | `Weak` 环 | 各 manager 持 `Weak<RefCell<GraphwarModuleContainer>>`（§22.5） | 22 个都**不**反向持有容器，`get_module_wk` 这条路没接 | 原版**观测**；本层未用 |
//!
//! # 暴露给别处的契约（不含引擎）
//!
//! ```text
//! Wired::ALL / COUNT / REAL_COUNT / STUB_COUNT / type_name / module_path / status
//! Wired::from_type_name / from_module_path
//! SLOTS: [Slot; 22]                        # 逐条台账（含 arm64 new 地址与我方类型名）
//! RootWiring::empty()                      # 空容器（用来验"按类型查不到"那一侧）
//! RootWiring::assemble_demo()              # 按原版顺序装 22 个
//! RootWiring::container() -> Rc<RefCell<GraphwarModuleContainer>>
//! RootWiring::len() -> usize
//! RootWiring::has(Wired) -> bool                # TypeId 键在不在（contains）
//! RootWiring::retrievable(Wired) -> bool        # get_module::<M>() 真的 downcast 成功
//! RootWiring::has_name(&str) / retrievable_name(&str)
//! RootWiring::registered() -> Vec<Wired>        # 按类型查成立的那几个，注册序
//! RootWiring::game_state() / turn_manager() / player_manager() -> Option<Rc<RefCell<T>>>
//! slot_status(&str) -> "assembled" | "stub" | "placeholder" | "unknown"
//! ```
//!
//! 引擎那一侧（`GraphwarRootNode`）只调用这些，**不碰 `HashMap` 的内部**。

use std::cell::RefCell;
use std::rc::Rc;

use crate::function_history_recorder::History;
use crate::game::Match;
use crate::graphwar_color_manager::GraphwarColorManager;
use crate::graphwar_computer_player_manager::ComputerPlayers;
use crate::graphwar_effect_manager::GraphwarEffectManager;
use crate::graphwar_entity_effect_manager::GraphwarEntityEffectManager;
use crate::graphwar_graphics_manager::GraphwarGraphicsManager;
use crate::graphwar_life_manager::LifeState;
use crate::graphwar_local_player_manager::{GraphwarLocalPlayerManager, LocalPlayerProfile};
use crate::graphwar_mode_manager::{AxisMode, FunctionMode, GraphwarModeManager, TimeMode};
use crate::graphwar_module_container::{self, GraphwarModuleContainer};
use crate::graphwar_name_manager::GraphwarNameManager;
use crate::graphwar_player_manager::GraphwarPlayerManager;
use crate::graphwar_rank_manager::GraphwarRankManager;
use crate::graphwar_shot_calculator::settle::Target;
use crate::graphwar_soldier_manager::SoldierTable;
use crate::graphwar_target_manager::GraphwarTargetManager;
use crate::graphwar_team_manager::TeamTable;
use crate::graphwar_turn_manager::{Team, TurnManager};
use crate::server::RoomState;

// ---------------------------------------------------------------------------
// 1. 装哪 22 个、按什么序（名字是 `graphwar_module_container::MODULES` 的原文）
// ---------------------------------------------------------------------------

/// 本层真正装进容器的 manager。**名字取 [`graphwar_module_container::MODULES`] 的原文**，
/// **顺序 = 原版 `GraphwarGameClient::new` 的注册序**（`docs/16-client.md` §41）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Wired {
    /// `GraphwarTickHandler`（72 字节）。**空占位**：本仓库只有 `tick_at` 那个自由函数。
    TickHandler,
    /// `GraphwarClientConnectionHandler`（112 字节）。**空占位**。
    ClientConnectionHandler,
    /// `GraphwarLocalPlayerManager`（64 字节）⇔ [`GraphwarLocalPlayerManager`]。
    LocalPlayerManager,
    /// `GraphwarPlayerManager`（72 字节）⇔ [`GraphwarPlayerManager`]。
    PlayerManager,
    /// `GraphwarSoldierManager`（72 字节）⇔ [`SoldierTable`]。
    SoldierManager,
    /// `GraphwarPlanePosManager`（24 字节）。**空占位**（逻辑在 `src/plane.rs` 的自由函数里）。
    PlanePosManager,
    /// `GraphwarGameStateManager`（296 字节）⇔ [`Match`]（那个模块的"运行实体那一半"）。
    GameStateManager,
    /// `GraphwarFunctionManager`（32 字节）。**空占位**（函数槽在 `game::Match` 里）。
    FunctionManager,
    /// `GraphwarTargetManager`（72 字节）⇔ [`GraphwarTargetManager`]。
    TargetManager,
    /// `GraphwarTeamManager`（32 字节）⇔ [`TeamTable`]。
    TeamManager,
    /// `GraphwarLifeManager`（72 字节）⇔ [`LifeState`]。
    LifeManager,
    /// `GraphwarObstacleManager`（80 字节）。**空占位**（掩膜逻辑在 `src/plane.rs`）。
    ObstacleManager,
    /// `GraphwarTurnManager`（32 字节）⇔ [`TurnManager`]。
    TurnManager,
    /// `GraphwarEffectManager`（72 字节）⇔ [`GraphwarEffectManager`]。
    EffectManager,
    /// `GraphwarModeManager`（32 字节）⇔ [`GraphwarModeManager`]。
    ModeManager,
    /// `GraphwarGraphicsManager`（168 字节）⇔ [`GraphwarGraphicsManager`]。
    GraphicsManager,
    /// `GraphwarEntityEffectManager`（72 字节）⇔ [`GraphwarEntityEffectManager`]。
    EntityEffectManager,
    /// `GraphwarComputerPlayerManager`（72 字节）⇔ [`ComputerPlayers`]。
    ComputerPlayerManager,
    /// `FunctionHistoryRecorder`（48 字节）⇔ [`History`]。
    FunctionHistoryRecorder,
    /// `GraphwarNameManager`（72 字节）⇔ [`GraphwarNameManager`]。
    NameManager,
    /// `GraphwarColorManager`（120 字节）⇔ [`GraphwarColorManager`]。
    ColorManager,
    /// `GraphwarRankManager`（72 字节）⇔ [`GraphwarRankManager`]。
    RankManager,
}

/// 一个槽的两种档位（**这是本层自己的分类**，不是原版的东西）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// 容器里装的是本仓库已有的复原类型（行为可能不完整，但类型是真的）。
    RealType,
    /// 容器里装的是**零行为的空占位**：原版这一型在本仓库没有可构造的类型。
    Stub,
}

/// 一个注册槽的台账（[`SLOTS`] 的元素）。
///
/// `size` / `arm64_new` 是**原版的数**；`our_type` 是**我方**的类型路径（`—` = 空占位）。
#[derive(Debug, Clone, Copy)]
pub struct Slot {
    /// 本层的枚举。
    pub wired: Wired,
    /// 原版类型名（`MODULES[i].name` 的原文）。
    pub type_name: &'static str,
    /// 原版模块文件名（不带 `.rs`）。
    pub module_path: &'static str,
    /// `size_of::<M>()`（= `dyn Any` vtable 的 size 字 − 8；两架构相同）。
    pub size: u32,
    /// arm64 2.0.60 里 `<Type>::new` 的地址；`0` = 该型构造在调用点**被内联**
    /// （`LocalPlayerManager` / `FunctionManager` 两个，见 [`Slot::new_note`]）。
    pub arm64_new: u32,
    /// 我方类型路径（`"graphwar_turn_manager::TurnManager"` 之类）；空占位为 `"—"`。
    pub our_type: &'static str,
    /// 真类型 / 空占位。
    pub kind: SlotKind,
}

impl Slot {
    /// 这一槽的类型名是从哪条指令读出来的（给人读的一句话，不是新证据）。
    pub const fn new_note(&self) -> &'static str {
        match self.wired {
            Wired::LocalPlayerManager => {
                "构造被内联在调用点：`0x409eac` 处 `alloc(0x58, 8)`（0x58−0x18 = 64 = size_of::<M>()），\
                 类型由 vtable 的 `drop_in_place<RefCell<GraphwarLocalPlayerManager>>` 给出"
            }
            Wired::FunctionManager => {
                "构造经 `Rc::<RefCell<M>>::new_cyclic_in`（arm64 `0x40a158`，`GraphwarFunctionManager::new` \
                 被内联进闭包），类型由 vtable 的 `drop_in_place<RefCell<GraphwarFunctionManager>>` 给出"
            }
            _ => "调用点前一条 `bl <Type>::new` 的符号名（arm64 `.symtab` 原文）",
        }
    }
}

// ---------------------------------------------------------------------------
// 2. 22 条的逐条台账（顺序 = 注册序 = 原版调用序）
// ---------------------------------------------------------------------------
//
// ⚠️ 这张表与 `graphwar_module_container::MODULES` 是**两个不同的东西**：
//    MODULES = 25 个被注册的**类型**（按模块名字典序，查地址用）；
//    SLOTS   = 一个**装配点**（`GraphwarGameClient::new`）发出的 22 条调用，**有序**。
//    两者用 `type_name` 关联，单测里核对（名字在 MODULES 里 + 尺寸一致）。

/// 22 条注册槽，顺序 = 原版 `GraphwarGameClient::new` 的调用序。
pub const SLOTS: [Slot; Wired::COUNT] = [
    Slot { wired: Wired::TickHandler, type_name: "GraphwarTickHandler",
           module_path: "graphwar_tick_handler", size: 72, arm64_new: 0x40b528,
           our_type: "—", kind: SlotKind::Stub },
    Slot { wired: Wired::ClientConnectionHandler, type_name: "GraphwarClientConnectionHandler",
           module_path: "graphwar_client_connection_handler", size: 112, arm64_new: 0x48b048,
           our_type: "—", kind: SlotKind::Stub },
    Slot { wired: Wired::LocalPlayerManager, type_name: "GraphwarLocalPlayerManager",
           module_path: "graphwar_local_player_manager", size: 64, arm64_new: 0,
           our_type: "graphwar_local_player_manager::GraphwarLocalPlayerManager",
           kind: SlotKind::RealType },
    Slot { wired: Wired::PlayerManager, type_name: "GraphwarPlayerManager",
           module_path: "graphwar_player_manager", size: 72, arm64_new: 0x49d170,
           our_type: "graphwar_player_manager::GraphwarPlayerManager", kind: SlotKind::RealType },
    Slot { wired: Wired::SoldierManager, type_name: "GraphwarSoldierManager",
           module_path: "graphwar_soldier_manager", size: 72, arm64_new: 0x49d590,
           our_type: "graphwar_soldier_manager::SoldierTable", kind: SlotKind::RealType },
    Slot { wired: Wired::PlanePosManager, type_name: "GraphwarPlanePosManager",
           module_path: "graphwar_plane_pos_manager", size: 24, arm64_new: 0x3b89a0,
           our_type: "—", kind: SlotKind::Stub },
    Slot { wired: Wired::GameStateManager, type_name: "GraphwarGameStateManager",
           module_path: "graphwar_game_state_manager", size: 296, arm64_new: 0x4ade6c,
           our_type: "game::Match", kind: SlotKind::RealType },
    Slot { wired: Wired::FunctionManager, type_name: "GraphwarFunctionManager",
           module_path: "graphwar_function_manager", size: 32, arm64_new: 0,
           our_type: "—", kind: SlotKind::Stub },
    Slot { wired: Wired::TargetManager, type_name: "GraphwarTargetManager",
           module_path: "graphwar_target_manager", size: 72, arm64_new: 0x3dae34,
           our_type: "graphwar_target_manager::GraphwarTargetManager", kind: SlotKind::RealType },
    Slot { wired: Wired::TeamManager, type_name: "GraphwarTeamManager",
           module_path: "graphwar_team_manager", size: 32, arm64_new: 0x49960c,
           our_type: "graphwar_team_manager::TeamTable", kind: SlotKind::RealType },
    Slot { wired: Wired::LifeManager, type_name: "GraphwarLifeManager",
           module_path: "graphwar_life_manager", size: 72, arm64_new: 0x4977b8,
           our_type: "graphwar_life_manager::LifeState", kind: SlotKind::RealType },
    Slot { wired: Wired::ObstacleManager, type_name: "GraphwarObstacleManager",
           module_path: "graphwar_obstacle_manager", size: 80, arm64_new: 0x426300,
           our_type: "—", kind: SlotKind::Stub },
    Slot { wired: Wired::TurnManager, type_name: "GraphwarTurnManager",
           module_path: "graphwar_turn_manager", size: 32, arm64_new: 0x49adf8,
           our_type: "graphwar_turn_manager::TurnManager", kind: SlotKind::RealType },
    Slot { wired: Wired::EffectManager, type_name: "GraphwarEffectManager",
           module_path: "graphwar_effect_manager", size: 72, arm64_new: 0x40bc0c,
           our_type: "graphwar_effect_manager::GraphwarEffectManager", kind: SlotKind::RealType },
    Slot { wired: Wired::ModeManager, type_name: "GraphwarModeManager",
           module_path: "graphwar_mode_manager", size: 32, arm64_new: 0x498090,
           our_type: "graphwar_mode_manager::GraphwarModeManager", kind: SlotKind::RealType },
    Slot { wired: Wired::GraphicsManager, type_name: "GraphwarGraphicsManager",
           module_path: "graphwar_graphics_manager", size: 168, arm64_new: 0x49e394,
           our_type: "graphwar_graphics_manager::GraphwarGraphicsManager",
           kind: SlotKind::RealType },
    Slot { wired: Wired::EntityEffectManager, type_name: "GraphwarEntityEffectManager",
           module_path: "graphwar_entity_effect_manager", size: 72, arm64_new: 0x3e9f40,
           our_type: "graphwar_entity_effect_manager::GraphwarEntityEffectManager",
           kind: SlotKind::RealType },
    Slot { wired: Wired::ComputerPlayerManager, type_name: "GraphwarComputerPlayerManager",
           module_path: "graphwar_computer_player_manager", size: 72, arm64_new: 0x4b02a4,
           our_type: "graphwar_computer_player_manager::ComputerPlayers",
           kind: SlotKind::RealType },
    Slot { wired: Wired::FunctionHistoryRecorder, type_name: "FunctionHistoryRecorder",
           module_path: "function_history_recorder", size: 48, arm64_new: 0x49d9a0,
           our_type: "function_history_recorder::History", kind: SlotKind::RealType },
    Slot { wired: Wired::NameManager, type_name: "GraphwarNameManager",
           module_path: "graphwar_name_manager", size: 72, arm64_new: 0x40af68,
           our_type: "graphwar_name_manager::GraphwarNameManager", kind: SlotKind::RealType },
    Slot { wired: Wired::ColorManager, type_name: "GraphwarColorManager",
           module_path: "graphwar_color_manager", size: 120, arm64_new: 0x49cbe8,
           our_type: "graphwar_color_manager::GraphwarColorManager", kind: SlotKind::RealType },
    Slot { wired: Wired::RankManager, type_name: "GraphwarRankManager",
           module_path: "graphwar_rank_manager", size: 72, arm64_new: 0x498d2c,
           our_type: "graphwar_rank_manager::GraphwarRankManager", kind: SlotKind::RealType },
];

/// 槽位状态：**已装配 · 真类型**（容器里按类型查得到，且这一型有复原实现）。
pub const SLOT_ASSEMBLED: &str = "assembled";
/// 槽位状态：**已装配 · 空占位**（容器里查得到，但装的是零行为的占位类型）。
pub const SLOT_STUB: &str = "stub";
/// 槽位状态：**占位**（原版 25 型里有它，但**这个装配点不注册**它，容器里没有键）。
pub const SLOT_PLACEHOLDER: &str = "placeholder";
/// 槽位状态：**未知**（原版 25 型里也没有这个名字）。
pub const SLOT_UNKNOWN: &str = "unknown";

impl Wired {
    /// 本层装了几个（**= 原版 `GraphwarGameClient::new` 的 22 条 `add_module`**）。
    pub const COUNT: usize = 22;

    /// 其中"我方有真类型"的个数。
    pub const REAL_COUNT: usize = 17;

    /// 其中"空占位"的个数（= 25 型里没有复原实现的那 5 个）。
    pub const STUB_COUNT: usize = Wired::COUNT - Wired::REAL_COUNT;

    /// 全部 22 个，顺序 = **原版注册序**（`docs/16-client.md` §41 的表）。
    pub const ALL: [Wired; Self::COUNT] = [
        Wired::TickHandler,
        Wired::ClientConnectionHandler,
        Wired::LocalPlayerManager,
        Wired::PlayerManager,
        Wired::SoldierManager,
        Wired::PlanePosManager,
        Wired::GameStateManager,
        Wired::FunctionManager,
        Wired::TargetManager,
        Wired::TeamManager,
        Wired::LifeManager,
        Wired::ObstacleManager,
        Wired::TurnManager,
        Wired::EffectManager,
        Wired::ModeManager,
        Wired::GraphicsManager,
        Wired::EntityEffectManager,
        Wired::ComputerPlayerManager,
        Wired::FunctionHistoryRecorder,
        Wired::NameManager,
        Wired::ColorManager,
        Wired::RankManager,
    ];

    /// 原版类型名（`MODULES[i].name` 的原文）。
    pub const fn type_name(self) -> &'static str {
        match self {
            Wired::TickHandler => "GraphwarTickHandler",
            Wired::ClientConnectionHandler => "GraphwarClientConnectionHandler",
            Wired::LocalPlayerManager => "GraphwarLocalPlayerManager",
            Wired::PlayerManager => "GraphwarPlayerManager",
            Wired::SoldierManager => "GraphwarSoldierManager",
            Wired::PlanePosManager => "GraphwarPlanePosManager",
            Wired::GameStateManager => "GraphwarGameStateManager",
            Wired::FunctionManager => "GraphwarFunctionManager",
            Wired::TargetManager => "GraphwarTargetManager",
            Wired::TeamManager => "GraphwarTeamManager",
            Wired::LifeManager => "GraphwarLifeManager",
            Wired::ObstacleManager => "GraphwarObstacleManager",
            Wired::TurnManager => "GraphwarTurnManager",
            Wired::EffectManager => "GraphwarEffectManager",
            Wired::ModeManager => "GraphwarModeManager",
            Wired::GraphicsManager => "GraphwarGraphicsManager",
            Wired::EntityEffectManager => "GraphwarEntityEffectManager",
            Wired::ComputerPlayerManager => "GraphwarComputerPlayerManager",
            Wired::FunctionHistoryRecorder => "FunctionHistoryRecorder",
            Wired::NameManager => "GraphwarNameManager",
            Wired::ColorManager => "GraphwarColorManager",
            Wired::RankManager => "GraphwarRankManager",
        }
    }

    /// 原版模块文件名（不带 `.rs`，`MODULES[i].module_path` 的原文）。
    pub const fn module_path(self) -> &'static str {
        match self {
            Wired::TickHandler => "graphwar_tick_handler",
            Wired::ClientConnectionHandler => "graphwar_client_connection_handler",
            Wired::LocalPlayerManager => "graphwar_local_player_manager",
            Wired::PlayerManager => "graphwar_player_manager",
            Wired::SoldierManager => "graphwar_soldier_manager",
            Wired::PlanePosManager => "graphwar_plane_pos_manager",
            Wired::GameStateManager => "graphwar_game_state_manager",
            Wired::FunctionManager => "graphwar_function_manager",
            Wired::TargetManager => "graphwar_target_manager",
            Wired::TeamManager => "graphwar_team_manager",
            Wired::LifeManager => "graphwar_life_manager",
            Wired::ObstacleManager => "graphwar_obstacle_manager",
            Wired::TurnManager => "graphwar_turn_manager",
            Wired::EffectManager => "graphwar_effect_manager",
            Wired::ModeManager => "graphwar_mode_manager",
            Wired::GraphicsManager => "graphwar_graphics_manager",
            Wired::EntityEffectManager => "graphwar_entity_effect_manager",
            Wired::ComputerPlayerManager => "graphwar_computer_player_manager",
            Wired::FunctionHistoryRecorder => "function_history_recorder",
            Wired::NameManager => "graphwar_name_manager",
            Wired::ColorManager => "graphwar_color_manager",
            Wired::RankManager => "graphwar_rank_manager",
        }
    }

    /// 这一槽在**本层**的档位：真类型 / 空占位。
    pub const fn kind(self) -> SlotKind {
        match self {
            Wired::TickHandler
            | Wired::ClientConnectionHandler
            | Wired::PlanePosManager
            | Wired::FunctionManager
            | Wired::ObstacleManager => SlotKind::Stub,
            _ => SlotKind::RealType,
        }
    }

    /// 静态槽位状态（[`SLOT_ASSEMBLED`] / [`SLOT_STUB`]）——**这是"表里说装了什么"**，
    /// 运行时判据是 [`RootWiring::has`] / [`RootWiring::retrievable`]。
    pub const fn status(self) -> &'static str {
        match self.kind() {
            SlotKind::RealType => SLOT_ASSEMBLED,
            SlotKind::Stub => SLOT_STUB,
        }
    }

    /// 这一槽的台账。
    pub fn slot(self) -> &'static Slot {
        SLOTS.iter().find(|s| s.wired == self).expect("Wired 与 SLOTS 必须一一对应")
    }

    /// 原版类型名 → 本层枚举。未知名字返回 `None`（**不猜**，也不做大小写归一）。
    pub fn from_type_name(name: &str) -> Option<Wired> {
        Self::ALL.into_iter().find(|w| w.type_name() == name)
    }

    /// 原版模块名 → 本层枚举。
    pub fn from_module_path(path: &str) -> Option<Wired> {
        Self::ALL.into_iter().find(|w| w.module_path() == path)
    }
}

/// 这个名字属于本层的装配表吗？（**运行时判据**是 [`RootWiring::has_name`]）
///
/// 两者分工：本函数只看 [`Wired`] 的静态表，`has_name` 真去容器里按 `TypeId` 查。
/// 验收脚本两条都要断言（否则"表里写了"和"真的装进去了"分不开）。
pub fn is_wired_name(name: &str) -> bool {
    Wired::from_type_name(name).is_some()
}

/// 这个名字在原版 25 型台账里吗（[`graphwar_module_container::MODULES`]）。
pub fn is_module_type_name(name: &str) -> bool {
    graphwar_module_container::find(name).is_some()
}

/// 四态槽位判据（**纯静态**，不看容器）：装配表 → `assembled`/`stub`；25 型台账 → `placeholder`。
pub fn slot_status(name: &str) -> &'static str {
    match Wired::from_type_name(name) {
        Some(w) => w.status(),
        None if is_module_type_name(name) => SLOT_PLACEHOLDER,
        None => SLOT_UNKNOWN,
    }
}

// ---------------------------------------------------------------------------
// 3. 5 个零行为的空占位（**我方形状**，不是原版类型）
// ---------------------------------------------------------------------------

/// 空占位：原版 `GraphwarTickHandler`（72 字节）在本仓库**没有**可构造的类型
/// （`src/graphwar_tick_handler.rs` 只有 `TICKS_PER_SECOND` 与 `tick_at` 两个自由项）。
///
/// 它进容器只证明"**这一槽在注册序里存在**"，**不**证明原版行为。
pub struct TickHandlerSlot;

/// 空占位：原版 `GraphwarClientConnectionHandler`（112 字节）。
///
/// `src/graphwar_client_connection_handler.rs` 只有字段偏移表（`HandlerField`）与事件枚举，
/// 没有那个 `RefCell` 里的结构体。
pub struct ClientConnectionHandlerSlot;

/// 空占位：原版 `GraphwarPlanePosManager`（24 字节）。
///
/// 该模块**没有** `src/graphwar_plane_pos_manager.rs`；已知的东西（`px = 20x + 500` 之类）
/// 以自由函数形式落在 `src/plane.rs` / `src/graphwar_plane.rs`。
pub struct PlanePosManagerSlot;

/// 空占位：原版 `GraphwarFunctionManager`（32 字节）。
///
/// 该模块**没有** `src/graphwar_function_manager.rs`；函数槽在 [`crate::game::Match`] 里
/// （`SoldierFunctions`，那是 `graphwar_function_evolutionator` 的类型，**不是**本模块的），
/// 所以不把它冒充成这个槽——按 `AGENTS.md` §0 第 3 条，**不猜类型归属**。
pub struct FunctionManagerSlot;

/// 空占位：原版 `GraphwarObstacleManager`（80 字节）。
///
/// 该模块**没有** `src/graphwar_obstacle_manager.rs`；掩膜光栅化与 `is_occupied`
/// 落在 `src/plane.rs`（见 `docs/39` / `docs/08`），但那两个自由函数不是"那个 80 字节的结构体"。
pub struct ObstacleManagerSlot;

// ---------------------------------------------------------------------------
// 4. 演示数据（**我方选择**：不是原版的对局数据）
// ---------------------------------------------------------------------------

/// 一个假士兵。坐标单位与 `Target` 一致（棋盘单位，`x ∈ [-25, 25]`、`y ∈ [-15, 15]`）。
#[derive(Debug, Clone, Copy)]
pub struct DemoSoldier {
    /// 实体 id（真游戏从 1 起，`0` 是"没有射手"的哨兵，见 `graphwar_turn_manager::SOLDIER_ID_NONE`）。
    pub id: i64,
    pub x: f64,
    pub y: f64,
    /// 半径（`Target::radius`；验收夹具里用 `0.5`）。
    pub radius: f64,
    /// 名字标签文本（原版从 `NameManager` / backtracer 来，这里**我方给**）。
    pub name: &'static str,
}

/// 3 个假士兵：队伍一（`x ≤ 0`）两人、队伍二（`x ≥ 0`）一人。
///
/// 左右分布照 `Team` 的观测判据（`graphwar_turn_manager::Team`：`One` = `x ≤ 0` 朝 `+x`，
/// `Two` = `x ≥ 0` 朝 `-x`）；**具体坐标与名字是本层自己选的**，不是原版数据。
pub const DEMO_SOLDIERS: [DemoSoldier; 3] = [
    DemoSoldier { id: 1, x: -15.0, y: -8.0, radius: 0.5, name: "P1-1" },
    DemoSoldier { id: 2, x: -8.0, y: 8.0, radius: 0.5, name: "P1-2" },
    DemoSoldier { id: 3, x: 12.0, y: 4.0, radius: 0.5, name: "P2-1" },
];

/// 一个假玩家：`player_id` + 队伍 + 它的士兵 + 连接号。
#[derive(Debug, Clone, Copy)]
pub struct DemoPlayer {
    pub player_id: i64,
    pub team: Team,
    /// 士兵 id（顺序即轮转顺序；`PlayerTurns` 的"填充顺序"那条未知项不在这里解决）。
    pub soldiers: &'static [i64],
    /// 连接号（`GraphwarPlayerManager` 的 `player_id → connection_id` 那张表）。
    pub connection_id: u32,
}

/// 2 个假玩家：`player_id 1` 拿士兵 1/2，`player_id 2` 拿士兵 3。
pub const DEMO_PLAYERS: [DemoPlayer; 2] = [
    DemoPlayer { player_id: 1, team: Team::One, soldiers: &[1, 2], connection_id: 1 },
    DemoPlayer { player_id: 2, team: Team::Two, soldiers: &[3], connection_id: 2 },
];

/// 演示房间状态：6 个字段**照搬** `tests/graphwar_game_state_manager.rs::room()`
/// （那 6 个取值来自抓包状态同步报文，不是这里编的）。
pub fn demo_room() -> RoomState {
    RoomState {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
    }
}

/// 本机玩家配置：原版来自 `GraphwarPlayerConfig`（`root+0x16a8`），那四个字段
/// （名字 / 皮肤 / 脸 / 帽子）**本仓库没有复原**。
///
/// ⇒ 这里给**空串**：`LocalPlayerManager` 只需要一个能构造的值，
/// 而**编一个资产名**（`"default_skin"` 之类）会变成没有证据的假数据。
pub const DEMO_LOCAL_PROFILE: LocalPlayerProfile = LocalPlayerProfile {
    name: String::new(),
    skin: String::new(),
    face: String::new(),
    hat: String::new(),
};

/// [`DEMO_SOLDIERS`] → `Match::new` 要的 `Vec<Target>`。
pub fn demo_targets() -> Vec<Target> {
    DEMO_SOLDIERS
        .iter()
        .map(|s| Target { id: s.id, x: s.x, y: s.y, radius: s.radius })
        .collect()
}

// ---------------------------------------------------------------------------
// 5. 装载器
// ---------------------------------------------------------------------------

/// 一个**已装配**的模块容器：`Rc<RefCell<GraphwarModuleContainer>>`。
///
/// 原版容器只暴露 `add_module` / `get_module` / `get_module_wk` 三个泛型方法（§22.5），
/// **没有遍历口**；"装了哪几个"这件事在原版里根本不可查（调用者自己知道）。
/// 本层把"按类型查得到"这件事**现查容器**——所有对外回答都是从 `TypeId` 出发的。
#[derive(Clone)]
pub struct RootWiring {
    /// 容器本体（与 `GraphwarGameClient` 持有的是同一种 `Rc<RefCell<_>>`，§22.5）。
    container: Rc<RefCell<GraphwarModuleContainer>>,
}

/// `add_module::<M>(Rc::new(RefCell::new(m)))` 的一行版（形状照 §22.5 的
/// `Rc::clone → TypeId::of::<M>() → HashMap::insert`，只是把 `Rc::new` 也写在一处）。
fn add<M: 'static>(container: &mut GraphwarModuleContainer, module: M) {
    container.add_module(Rc::new(RefCell::new(module)));
}

impl RootWiring {
    /// 一个空容器：什么都不装（用来验"按类型查不到"那一侧，别拿它当"装配失败"）。
    pub fn empty() -> Self {
        Self { container: Rc::new(RefCell::new(GraphwarModuleContainer::new())) }
    }

    /// **按原版注册序装配 22 个 manager**（本文件的主入口）。
    ///
    /// 每一槽都走原版的形状 `Rc::new(RefCell::new(<M as>::new(…)))` + `add_module::<M>`；
    /// 17 个的 `new` 是各自模块里**已有**的实现，本函数不再写第二份逻辑。
    /// 5 个空占位（[`SlotKind::Stub`]）只构造一个零行为的值。
    ///
    /// 演示数据见 [`DEMO_SOLDIERS`] / [`DEMO_PLAYERS`] / [`demo_room`] /
    /// [`DEMO_LOCAL_PROFILE`]（**我方选择**）。
    pub fn assemble_demo() -> Self {
        let mut c = GraphwarModuleContainer::new();

        // ① GraphwarTickHandler —— 空占位（无复原类型）。
        add(&mut c, TickHandlerSlot);

        // ② GraphwarClientConnectionHandler —— 空占位。
        add(&mut c, ClientConnectionHandlerSlot);

        // ③ GraphwarLocalPlayerManager —— 本机玩家（配置四个字段我方给空串）。
        add(&mut c, GraphwarLocalPlayerManager::new(DEMO_LOCAL_PROFILE));

        // ④ GraphwarPlayerManager —— `player_id → connection_id` 那张表。
        let mut players = GraphwarPlayerManager::new();
        for p in DEMO_PLAYERS {
            // id 都在 `u32` 范围内（1/2），溢出不 panic：直接跳过。
            if let Ok(id) = u32::try_from(p.player_id) {
                players.add_player(id, p.connection_id);
            }
        }
        add(&mut c, players);

        // ⑤ GraphwarSoldierManager —— `soldier_id → owner` 那张表。
        let mut soldiers = SoldierTable::new();
        for p in DEMO_PLAYERS {
            if let Ok(owner) = u32::try_from(p.player_id) {
                for s in p.soldiers {
                    if let Ok(soldier_id) = u32::try_from(*s) {
                        soldiers.add_soldier(soldier_id, owner);
                    }
                }
            }
        }
        add(&mut c, soldiers);

        // ⑥ GraphwarPlanePosManager —— 空占位。
        add(&mut c, PlanePosManagerSlot);

        // ⑦ GraphwarGameStateManager —— 我方 = `game::Match`（运行实体那一半）。
        //    `Match::new` 的 4 个实参：房间状态 / 障碍 / 已出生的士兵 / tick 基准。
        //    障碍取空表（演示没有障碍数据，**不是**"原版没有障碍"）。
        let mut game = Match::new(demo_room(), Vec::new(), demo_targets(), (0, 0));
        // `register_player` = 原版 `request_new_player` + `add_soldier_to_player` 的接线形态
        // （写队伍 / 开函数槽 / 置生命为 alive / 进 `TurnManager`），见 `src/game.rs`。
        for p in DEMO_PLAYERS {
            game.register_player(p.player_id, p.team, p.soldiers);
        }
        add(&mut c, game);

        // ⑧ GraphwarFunctionManager —— 空占位（函数槽在 `Match` 里，不冒充）。
        add(&mut c, FunctionManagerSlot);

        // ⑨ GraphwarTargetManager —— `id → alive` 那张表（空表，演示没有靶子）。
        add(&mut c, GraphwarTargetManager::new());

        // ⑩ GraphwarTeamManager —— `player_id → team` 那张表。
        let mut teams = TeamTable::new();
        for p in DEMO_PLAYERS {
            if let Ok(id) = u32::try_from(p.player_id) {
                let team = match p.team {
                    Team::One => crate::graphwar_team_manager::GraphwarTeam::Team1,
                    Team::Two => crate::graphwar_team_manager::GraphwarTeam::Team2,
                };
                teams.add_to_team(id, team);
            }
        }
        add(&mut c, teams);

        // ⑪ GraphwarLifeManager —— 两张表（`LifeState::default()` = 空表）。
        add(&mut c, LifeState::default());

        // ⑫ GraphwarObstacleManager —— 空占位。
        add(&mut c, ObstacleManagerSlot);

        // ⑬ GraphwarTurnManager —— 直接 `TurnManager::new()` + 2 个玩家。
        let mut turns = TurnManager::new();
        for p in DEMO_PLAYERS {
            turns.add_player(p.player_id, p.team, p.soldiers.to_vec());
        }
        add(&mut c, turns);

        // ⑭ GraphwarEffectManager —— 效果表（空表）。
        add(&mut c, GraphwarEffectManager::new());

        // ⑮ GraphwarModeManager —— 三个 mode。初值照 `demo_room()` 的抓包字符串
        //    手对（`EveryUnit` / `NormalFunction` / `Timer1m`）；映射本身是 `docs/07` §5.2
        //    的判别值，不是接线逻辑（原版这三个值由 `GameInfo` 驱动）。
        add(
            &mut c,
            GraphwarModeManager::new(
                AxisMode::EveryUnit,
                FunctionMode::NormalFunction,
                TimeMode::Timer1m,
            ),
        );

        // ⑯ GraphwarGraphicsManager —— 图形登记表（空表）。
        add(&mut c, GraphwarGraphicsManager::new());

        // ⑰ GraphwarEntityEffectManager —— `entity_id → effect kind` 那张表（空表）。
        add(&mut c, GraphwarEntityEffectManager::new());

        // ⑱ GraphwarComputerPlayerManager —— 电脑玩家表（空表：演示没有 bot）。
        add(&mut c, ComputerPlayers::new());

        // ⑲ FunctionHistoryRecorder —— 只写不删的 `Vec<FunctionData>`（空）。
        add(&mut c, History::new());

        // ⑳ GraphwarNameManager —— 名字表（空表）。
        add(&mut c, GraphwarNameManager::new());

        // ㉑ GraphwarColorManager —— 颜色表（空表）。
        add(&mut c, GraphwarColorManager::new());

        // ㉒ GraphwarRankManager —— 军衔表（空表）。
        add(&mut c, GraphwarRankManager::new());

        Self { container: Rc::new(RefCell::new(c)) }
    }

    /// 容器本体（原版 `GraphwarRootNode` 那条链的起点：`Rc<RefCell<GraphwarModuleContainer>>`）。
    pub fn container(&self) -> Rc<RefCell<GraphwarModuleContainer>> {
        Rc::clone(&self.container)
    }

    /// 容器里现在有几个模块（= `HashMap::len`，原版 `get_module` 的空表快路径就是它，§22.5）。
    pub fn len(&self) -> usize {
        self.container.borrow().len()
    }

    /// 容器是不是空的。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// **按类型**查键在不在：这一型在容器里吗？（`TypeId` 比对，不看数量、不看名字）
    ///
    /// 判据是 `HashMap::contains_key(&TypeId::of::<M>())`——**不含** downcast。
    /// 想验"能取回来一个活的 `Rc<RefCell<M>>`"用 [`RootWiring::retrievable`]。
    pub fn has(&self, which: Wired) -> bool {
        let c = self.container.borrow();
        match which {
            Wired::TickHandler => c.contains::<TickHandlerSlot>(),
            Wired::ClientConnectionHandler => c.contains::<ClientConnectionHandlerSlot>(),
            Wired::LocalPlayerManager => c.contains::<GraphwarLocalPlayerManager>(),
            Wired::PlayerManager => c.contains::<GraphwarPlayerManager>(),
            Wired::SoldierManager => c.contains::<SoldierTable>(),
            Wired::PlanePosManager => c.contains::<PlanePosManagerSlot>(),
            Wired::GameStateManager => c.contains::<Match>(),
            Wired::FunctionManager => c.contains::<FunctionManagerSlot>(),
            Wired::TargetManager => c.contains::<GraphwarTargetManager>(),
            Wired::TeamManager => c.contains::<TeamTable>(),
            Wired::LifeManager => c.contains::<LifeState>(),
            Wired::ObstacleManager => c.contains::<ObstacleManagerSlot>(),
            Wired::TurnManager => c.contains::<TurnManager>(),
            Wired::EffectManager => c.contains::<GraphwarEffectManager>(),
            Wired::ModeManager => c.contains::<GraphwarModeManager>(),
            Wired::GraphicsManager => c.contains::<GraphwarGraphicsManager>(),
            Wired::EntityEffectManager => c.contains::<GraphwarEntityEffectManager>(),
            Wired::ComputerPlayerManager => c.contains::<ComputerPlayers>(),
            Wired::FunctionHistoryRecorder => c.contains::<History>(),
            Wired::NameManager => c.contains::<GraphwarNameManager>(),
            Wired::ColorManager => c.contains::<GraphwarColorManager>(),
            Wired::RankManager => c.contains::<GraphwarRankManager>(),
        }
    }

    /// **按类型取回**：`get_module::<M>()` 真的 downcast 成功吗？
    ///
    /// 比 [`RootWiring::has`] 强一档：它走完整的 `get_module` 路径
    /// （`hash_one` → 探测 → `Any::type_id` 比对 → `Rc::downcast`，§22.5），
    /// 所以"键在但值取不回来"这种坏情况会被它抓住。
    pub fn retrievable(&self, which: Wired) -> bool {
        let c = self.container.borrow();
        match which {
            Wired::TickHandler => c.get_module::<TickHandlerSlot>().is_some(),
            Wired::ClientConnectionHandler => c.get_module::<ClientConnectionHandlerSlot>().is_some(),
            Wired::LocalPlayerManager => c.get_module::<GraphwarLocalPlayerManager>().is_some(),
            Wired::PlayerManager => c.get_module::<GraphwarPlayerManager>().is_some(),
            Wired::SoldierManager => c.get_module::<SoldierTable>().is_some(),
            Wired::PlanePosManager => c.get_module::<PlanePosManagerSlot>().is_some(),
            Wired::GameStateManager => c.get_module::<Match>().is_some(),
            Wired::FunctionManager => c.get_module::<FunctionManagerSlot>().is_some(),
            Wired::TargetManager => c.get_module::<GraphwarTargetManager>().is_some(),
            Wired::TeamManager => c.get_module::<TeamTable>().is_some(),
            Wired::LifeManager => c.get_module::<LifeState>().is_some(),
            Wired::ObstacleManager => c.get_module::<ObstacleManagerSlot>().is_some(),
            Wired::TurnManager => c.get_module::<TurnManager>().is_some(),
            Wired::EffectManager => c.get_module::<GraphwarEffectManager>().is_some(),
            Wired::ModeManager => c.get_module::<GraphwarModeManager>().is_some(),
            Wired::GraphicsManager => c.get_module::<GraphwarGraphicsManager>().is_some(),
            Wired::EntityEffectManager => c.get_module::<GraphwarEntityEffectManager>().is_some(),
            Wired::ComputerPlayerManager => c.get_module::<ComputerPlayers>().is_some(),
            Wired::FunctionHistoryRecorder => c.get_module::<History>().is_some(),
            Wired::NameManager => c.get_module::<GraphwarNameManager>().is_some(),
            Wired::ColorManager => c.get_module::<GraphwarColorManager>().is_some(),
            Wired::RankManager => c.get_module::<GraphwarRankManager>().is_some(),
        }
    }

    /// 按**原版类型名**查（名字 → [`Wired`] → `TypeId`）。未知名字一律 `false`。
    pub fn has_name(&self, type_name: &str) -> bool {
        match Wired::from_type_name(type_name) {
            Some(which) => self.has(which),
            None => false,
        }
    }

    /// 按**原版类型名**做 downcast 式取回（[`RootWiring::retrievable`] 的名字版）。
    pub fn retrievable_name(&self, type_name: &str) -> bool {
        match Wired::from_type_name(type_name) {
            Some(which) => self.retrievable(which),
            None => false,
        }
    }

    /// 按类型查**成立**的那几个，顺序 = 注册序（判据仍是容器）。
    pub fn registered(&self) -> Vec<Wired> {
        Wired::ALL.into_iter().filter(|w| self.has(*w)).collect()
    }

    /// 注册序台账的名字（[`RootWiring::registered`] 的字符串形态）。
    pub fn registered_names(&self) -> Vec<&'static str> {
        self.registered().into_iter().map(|w| w.type_name()).collect()
    }

    /// 装配序里"我方有真类型"的几个。
    pub fn real_managers(&self) -> Vec<Wired> {
        self.registered().into_iter().filter(|w| w.kind() == SlotKind::RealType).collect()
    }

    /// 装配序里"空占位"的几个。
    pub fn stub_managers(&self) -> Vec<Wired> {
        self.registered().into_iter().filter(|w| w.kind() == SlotKind::Stub).collect()
    }

    /// `get_module::<GameStateManager>()`（原版 arm64 `0x4a0094` 那个单态化）。
    pub fn game_state(&self) -> Option<Rc<RefCell<Match>>> {
        self.container.borrow().get_module::<Match>()
    }

    /// `get_module::<TurnManager>()`（原版 arm64 `0x49fd34`）。
    pub fn turn_manager(&self) -> Option<Rc<RefCell<TurnManager>>> {
        self.container.borrow().get_module::<TurnManager>()
    }

    /// `get_module::<PlayerManager>()`（原版 arm64 `0x4a0244`）。
    pub fn player_manager(&self) -> Option<Rc<RefCell<GraphwarPlayerManager>>> {
        self.container.borrow().get_module::<GraphwarPlayerManager>()
    }

    /// `get_module::<SoldierManager>()`（原版 arm64 `0x4a1b94`）。
    pub fn soldier_manager(&self) -> Option<Rc<RefCell<SoldierTable>>> {
        self.container.borrow().get_module::<SoldierTable>()
    }

    /// `get_module::<TeamManager>()`（原版 arm64 `0x4a0fc4`）。
    pub fn team_manager(&self) -> Option<Rc<RefCell<TeamTable>>> {
        self.container.borrow().get_module::<TeamTable>()
    }

    /// `get_module::<ModeManager>()`（原版 arm64 `0x4a0754`）。
    pub fn mode_manager(&self) -> Option<Rc<RefCell<GraphwarModeManager>>> {
        self.container.borrow().get_module::<GraphwarModeManager>()
    }
}

// ---------------------------------------------------------------------------
// 6. 单测（**不碰引擎**：本文件零 Godot 依赖）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 22 个名字都必须能在原版 25 型台账里查到（名字不是我们编的），
    /// 且 `SLOTS` 与 `Wired::ALL` 一一对应、尺寸与 25 型台账一致。
    #[test]
    fn the_twenty_two_names_come_from_the_module_ledger() {
        assert_eq!(Wired::ALL.len(), Wired::COUNT);
        assert_eq!(Wired::COUNT, 22);
        assert_eq!(SLOTS.len(), Wired::COUNT);
        for (i, w) in Wired::ALL.into_iter().enumerate() {
            let entry = graphwar_module_container::find(w.type_name())
                .unwrap_or_else(|| panic!("{} 不在 MODULES 里", w.type_name()));
            assert_eq!(entry.module_path, w.module_path(), "{} 的模块名对不上", w.type_name());
            assert_eq!(Wired::from_type_name(w.type_name()), Some(w));
            assert_eq!(Wired::from_module_path(w.module_path()), Some(w));
            // SLOTS 与 ALL 同序、同型
            let slot = &SLOTS[i];
            assert_eq!(slot.wired, w, "SLOTS[{}] 与 Wired::ALL[{}] 不同序", i, i);
            assert_eq!(slot.type_name, w.type_name());
            assert_eq!(slot.module_path, w.module_path());
            // 尺寸：两份台账必须说同一个数（25 型台账 vs 本文件）
            assert_eq!(slot.size, entry.size, "{} 的 size_of 对不上", w.type_name());
        }
        // 模块名互不相同（否则 `from_module_path` 会歧义）
        let mut paths: Vec<&str> = Wired::ALL.iter().map(|w| w.module_path()).collect();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), Wired::COUNT);
    }

    /// 顺序是**原版注册序**，不是字典序、也不是按尺寸排——逐条钉死。
    ///
    /// 这一条就是 `docs/16-client.md` §41.2 那张表的机器可读版：
    /// 两架构的 `size_of::<RefCell<M>>` 序列完全相同，所以顺序错一位就会在这里红。
    #[test]
    fn registration_order_is_the_original_call_order() {
        let names: Vec<&str> = Wired::ALL.iter().map(|w| w.type_name()).collect();
        assert_eq!(
            names,
            vec![
                "GraphwarTickHandler",
                "GraphwarClientConnectionHandler",
                "GraphwarLocalPlayerManager",
                "GraphwarPlayerManager",
                "GraphwarSoldierManager",
                "GraphwarPlanePosManager",
                "GraphwarGameStateManager",
                "GraphwarFunctionManager",
                "GraphwarTargetManager",
                "GraphwarTeamManager",
                "GraphwarLifeManager",
                "GraphwarObstacleManager",
                "GraphwarTurnManager",
                "GraphwarEffectManager",
                "GraphwarModeManager",
                "GraphwarGraphicsManager",
                "GraphwarEntityEffectManager",
                "GraphwarComputerPlayerManager",
                "FunctionHistoryRecorder",
                "GraphwarNameManager",
                "GraphwarColorManager",
                "GraphwarRankManager",
            ]
        );
        // 22 个字 = 两架构逐位相等的那串 `size_of::<RefCell<M>>`
        let refcell_sizes: Vec<u32> = SLOTS.iter().map(|s| s.size + 8).collect();
        assert_eq!(
            refcell_sizes,
            vec![
                80, 120, 72, 80, 80, 32, 304, 40, 80, 40, 80, 88, 40, 80, 40, 176, 80, 80, 56, 80,
                128, 80
            ]
        );
    }

    /// 没装的原版型一律 `placeholder`；装了的分两档（真类型 / 空占位）。
    #[test]
    fn unwired_module_types_are_placeholders() {
        // 客户端**不**注册的那 3 型（25 − 22）：服务端 / backend 才装
        assert_eq!(slot_status("GraphwarBanList"), SLOT_PLACEHOLDER);
        assert_eq!(slot_status("GraphwarServerConnectionHandler"), SLOT_PLACEHOLDER);
        assert_eq!(slot_status("GraphwarGameRecorder"), SLOT_PLACEHOLDER);
        // 空占位的 5 型
        assert_eq!(slot_status("GraphwarTickHandler"), SLOT_STUB);
        assert_eq!(slot_status("GraphwarObstacleManager"), SLOT_STUB);
        // 真类型
        assert_eq!(slot_status("GraphwarTurnManager"), SLOT_ASSEMBLED);
        // 胡编的名字
        assert_eq!(slot_status("nonsense"), SLOT_UNKNOWN);
        assert_eq!(slot_status("graphwar_turn_manager"), SLOT_UNKNOWN, "只认类型名，不认模块名");
        // 25 型里 22 个是本层的槽（17 真 + 5 空），3 个是 placeholder
        let assembled = graphwar_module_container::MODULES
            .iter()
            .filter(|m| slot_status(m.name) == SLOT_ASSEMBLED)
            .count();
        let stubs = graphwar_module_container::MODULES
            .iter()
            .filter(|m| slot_status(m.name) == SLOT_STUB)
            .count();
        let placeholders = graphwar_module_container::MODULES
            .iter()
            .filter(|m| slot_status(m.name) == SLOT_PLACEHOLDER)
            .count();
        assert_eq!(assembled, Wired::REAL_COUNT);
        assert_eq!(stubs, Wired::STUB_COUNT);
        assert_eq!(placeholders, 3);
        assert_eq!(assembled + stubs, Wired::COUNT);
        assert_eq!(graphwar_module_container::MODULES.len(), 25);
    }

    /// 空容器：22 个都查不到（这一条是"按类型断言"的反面——数量 0，但不是靠数量判的）。
    #[test]
    fn empty_container_has_none_of_the_twenty_two() {
        let wiring = RootWiring::empty();
        assert_eq!(wiring.len(), 0);
        assert!(wiring.is_empty());
        for w in Wired::ALL {
            assert!(!wiring.has(w), "{} 不该在空容器里", w.type_name());
            assert!(!wiring.retrievable(w), "{} 不该能从空容器取回", w.type_name());
            assert!(!wiring.has_name(w.type_name()));
            assert!(!wiring.retrievable_name(w.type_name()));
        }
        assert!(wiring.game_state().is_none());
        assert!(wiring.turn_manager().is_none());
        assert!(wiring.player_manager().is_none());
        assert!(wiring.soldier_manager().is_none());
        assert!(wiring.team_manager().is_none());
        assert!(wiring.mode_manager().is_none());
        assert!(wiring.registered().is_empty());
    }

    /// 装配后：22 个都能**按类型**查/取回，且 `get_module` 能 downcast。
    #[test]
    fn assemble_registers_twenty_two_modules_retrievable_by_type_id() {
        let wiring = RootWiring::assemble_demo();
        assert_eq!(wiring.len(), Wired::COUNT);
        for w in Wired::ALL {
            assert!(wiring.has(w), "{} 装配后查不到（键不在）", w.type_name());
            assert!(wiring.retrievable(w), "{} 装配后取不回来（downcast 失败）", w.type_name());
            assert!(wiring.has_name(w.type_name()));
            assert!(wiring.retrievable_name(w.type_name()));
        }
        assert_eq!(wiring.registered().len(), Wired::COUNT);
        assert_eq!(wiring.registered_names(), Wired::ALL.iter().map(|w| w.type_name()).collect::<Vec<_>>());
        assert_eq!(wiring.real_managers().len(), Wired::REAL_COUNT);
        assert_eq!(wiring.stub_managers().len(), Wired::STUB_COUNT);
        // 客户端不注册的 3 型：容器里确实没有键
        for name in ["GraphwarBanList", "GraphwarServerConnectionHandler", "GraphwarGameRecorder"] {
            assert!(!wiring.has_name(name), "{} 不该在客户端容器里", name);
        }

        // 取回的对象是活的：3 个假士兵 / 2 个玩家
        let game = wiring.game_state().expect("GameStateManager 取不回来");
        assert_eq!(game.borrow().soldiers.len(), DEMO_SOLDIERS.len());
        let turns = wiring.turn_manager().expect("TurnManager 取不回来");
        assert_eq!(turns.borrow().player(1).map(|p| p.turns.len()), Some(2));
        assert_eq!(turns.borrow().player(2).map(|p| p.turns.len()), Some(1));
        let players = wiring.player_manager().expect("PlayerManager 取不回来");
        assert_eq!(players.borrow().len(), DEMO_PLAYERS.len());
        assert_eq!(players.borrow().connection_of(1), Some(1));
        assert_eq!(players.borrow().connection_of(2), Some(2));

        // 新装的几个：取回来读一次，证明不是"只有个键"
        let soldiers = wiring.soldier_manager().expect("SoldierManager 取不回来");
        assert_eq!(soldiers.borrow().len(), DEMO_SOLDIERS.len());
        let teams = wiring.team_manager().expect("TeamManager 取不回来");
        assert_eq!(teams.borrow().len(), DEMO_PLAYERS.len());
        let modes = wiring.mode_manager().expect("ModeManager 取不回来");
        assert_eq!(modes.borrow().axis_mode, AxisMode::EveryUnit);
        assert_eq!(modes.borrow().time_mode, TimeMode::Timer1m);

        // 同一个 `Rc`：通过容器两次取回，是同一个对象
        let a = wiring.turn_manager().unwrap();
        let b = wiring.turn_manager().unwrap();
        assert!(Rc::ptr_eq(&a, &b), "两次 get_module 必须拿到同一个 Rc");
    }

    /// 演示数据自洽：士兵 id 不重复、两队都有人、玩家表的士兵 id 都在士兵表里。
    #[test]
    fn demo_data_is_self_consistent() {
        let mut ids: Vec<i64> = DEMO_SOLDIERS.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), DEMO_SOLDIERS.len());
        // id 从 1 起（0 是"没有射手"的哨兵）
        assert!(ids.iter().all(|id| *id > 0));

        let targets = demo_targets();
        assert_eq!(targets.len(), DEMO_SOLDIERS.len());
        assert_eq!(targets[0].id, DEMO_SOLDIERS[0].id);
        assert_eq!(targets[0].x, DEMO_SOLDIERS[0].x);

        let mut rostered: Vec<i64> = DEMO_PLAYERS.iter().flat_map(|p| p.soldiers.iter().copied()).collect();
        rostered.sort_unstable();
        assert_eq!(rostered, ids, "玩家表的士兵 id 必须与士兵表一致");

        // 队伍分布：一队两人、二队一人（左右分布照 `Team` 的观测判据）
        let team_one: Vec<i64> =
            DEMO_PLAYERS.iter().filter(|p| p.team == Team::One).flat_map(|p| p.soldiers.iter().copied()).collect();
        assert_eq!(team_one, vec![1, 2]);
        for s in DEMO_SOLDIERS {
            if team_one.contains(&s.id) {
                assert!(s.x <= 0.0, "队伍一（One）的士兵在 {} 侧", s.x);
            } else {
                assert!(s.x >= 0.0, "队伍二（Two）的士兵在 {} 侧", s.x);
            }
            assert!(s.radius > 0.0);
            assert!(!s.name.is_empty());
        }
        assert_eq!(demo_room().turn_mode, "SequentialTurns");
        // 本机配置是**空串**（我们不复原资产名，见 `DEMO_LOCAL_PROFILE` 的文档）
        assert!(DEMO_LOCAL_PROFILE.name.is_empty());
    }

    /// `clone()` 出来的是**同一个容器**（`Rc` 克隆），不是深拷贝。
    #[test]
    fn clone_shares_the_same_container() {
        let a = RootWiring::assemble_demo();
        let b = a.clone();
        assert!(Rc::ptr_eq(&a.container(), &b.container()));
        assert_eq!(b.len(), Wired::COUNT);
    }
}
