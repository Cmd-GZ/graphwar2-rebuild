//! **不是原版的东西**：`GraphwarModuleContainer` 的**装载器** —— 把 3 个复原出来的
//! manager 真装进容器，并按 `TypeId` 取回。
//!
//! # 这一份解决什么
//!
//! 容器本体（[`crate::graphwar_module_container::GraphwarModuleContainer`]，
//! `HashMap<TypeId, Rc<dyn Any>>` / 48 字节 / 3 个方法）已经有实现、有 17 条结构断言，
//! 但**没有任何东西往里塞过模块**：原版的 25 个 `add_module` 全在**别的模块**里
//! （`GraphwarGameClient::new` 22 条、`game_server_thread` 20 条、
//! `GraphwarBackend::start_*` 各 1 条，见 `graphwar_module_container::REGISTRATION_SITES`）。
//!
//! 本文件就是"装配者"这一侧的最小可用形态：**3 个 manager**（能开一局的最小集合），
//! 每个都走 `container.add_module::<M>(Rc::new(RefCell::new(M::new(…))))`，
//! 取回走 `container.get_module::<M>()`。**没有任何 `+0xNN` 偏移参与**——
//! 容器里存的是 `TypeId`，不是字段槽（`docs/02-module-map.md` §22.1 的否定结论）。
//!
//! ```text
//! 原版                                            本文件
//! GraphwarGameClient::new (0x409c70) 22 条  →     assemble_demo() 3 条
//!   add_module::<GraphwarGameStateManager>          → Rc<RefCell<crate::game::Match>>
//!   add_module::<GraphwarTurnManager>               → Rc<RefCell<graphwar_turn_manager::TurnManager>>
//!   add_module::<GraphwarPlayerManager>             → Rc<RefCell<graphwar_player_manager::GraphwarPlayerManager>>
//! get_module::<M>() （arm64 0x49f9d4+432i）  →     RootWiring::{game_state,turn_manager,player_manager}
//! ```
//!
//! # 装的是哪 3 个，`new` 在哪（每一型都能指到证据）
//!
//! | 原版类型（`MODULES` 的原文） | 我方等价类型 | 我方 `new` | 装配点的档位 |
//! |---|---|---|---|
//! | `GraphwarGameStateManager`（296 字节） | [`crate::game::Match`]（运行实体那一半） | `Match::new(room, obstacles, soldiers, tick_base)` | 类型名**观测**；`Match` 是**我方**拆分（`src/game.rs` 头部的分工说明） |
//! | `GraphwarTurnManager`（32 字节） | [`crate::graphwar_turn_manager::TurnManager`] | `TurnManager::new()` | 类型名**观测**；结构**我方**（`BTreeMap`，见该文件 §1） |
//! | `GraphwarPlayerManager`（72 字节） | [`crate::graphwar_player_manager::GraphwarPlayerManager`] | `GraphwarPlayerManager::new()` | 类型名**观测** |
//!
//! 三个 `new` 都是**已有实现**（本文件不再写第二份逻辑，只负责 `Rc<RefCell<_>>` + 注册）。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 注册几个 | `GraphwarGameClient::new` **22** 条 `add_module` | **3**（剩下的 19 个 manager 还没有可 `new` 的实现，**如实留白**） | 22 是**观测**；3 是**本轮范围**（任务书 §要做什么） |
//! | 注册顺序 | 22 条的**调用序**在两侧逐位相同（§22.4），但**逐条的型名**没做跨架构对齐 | `GameStateManager → TurnManager → PlayerManager`（= 任务书 §2 的顺序） | **我方选择**（不是观测到的原版顺序） |
//! | 3 个 manager 是否**共享**实例 | `GameStateManager` 内部是否也持 `TurnManager`：**未解** | **不共享**：容器里是 3 个独立实例；`Match` 自己还带一个内部 `TurnManager`（`src/game.rs` 的接线层） | 独立性是**我方形状**；原版是否共享 `未知` |
//! | 尺寸 | `size_of::<M>()` = 296 / 32 / 72（vtable 第 2 字 − 8） | 我方类型的 `size_of` **与这些数不相等**（`Match` 带 `RoomState` / `Vec` 等） | 原版尺寸**观测**；我方差异是**必然**（结构未按字节复原） |
//! | 演示数据（3 士兵 / 2 玩家 / 房间 6 字段） | 真对局的 `SoldierInfo` / `TeamInfo` / `GameInfo` | [`DEMO_SOLDIERS`] / [`DEMO_PLAYERS`] / [`demo_room`] 写死 | **我方选择**（假数据；`RoomState` 的 6 个取值照搬 `tests/graphwar_game_state_manager.rs::room()`） |
//! | 容器与根的持有关系 | 容器在 `Rc<RefCell<_>>` 里，多个 manager 持 `Weak` 打断环（§22.5） | 本文件只持 `Rc`，**没有 `Weak` 环**：3 个 manager 都不反向持有容器 | 原版**观测**；本层还没接 `get_module_wk` 那条路 |
//!
//! ⇒ 本文件证明的是「**按 `TypeId` 注册/取回这条链真的能跑**」，
//! **不是**"22 个 manager 都装好了"——剩下 19 个一个都没装。
//!
//! # 暴露给别处的契约（不含引擎）
//!
//! ```text
//! Wired::ALL / type_name() / module_path() / from_type_name()
//! RootWiring::empty()                      # 空容器（用来验"按类型查不到"那一侧）
//! RootWiring::assemble_demo()              # 装 3 个
//! RootWiring::container() -> Rc<RefCell<GraphwarModuleContainer>>
//! RootWiring::len() -> usize
//! RootWiring::has(Wired) -> bool           # 按类型（TypeId）
//! RootWiring::has_name(&str) -> bool       # 名字 → 类型 → TypeId（未知名字一律 false）
//! RootWiring::registered() -> Vec<Wired>   # 按类型查成立的那几个，注册序
//! RootWiring::game_state() / turn_manager() / player_manager() -> Option<Rc<RefCell<T>>>
//! slot_status(&str) -> "assembled" | "placeholder" | "unknown"
//! ```
//!
//! 引擎那一侧（`GraphwarRootNode`）只调用这些，**不碰 `HashMap` 的内部**。

use std::cell::RefCell;
use std::rc::Rc;

use crate::game::Match;
use crate::graphwar_module_container::{self, GraphwarModuleContainer};
use crate::graphwar_player_manager::GraphwarPlayerManager;
use crate::graphwar_shot_calculator::settle::Target;
use crate::graphwar_turn_manager::{Team, TurnManager};
use crate::server::RoomState;

// ---------------------------------------------------------------------------
// 1. 装哪 3 个（名字是 `graphwar_module_container::MODULES` 的原文）
// ---------------------------------------------------------------------------

/// 本层真正装进容器的 manager。名字取 [`graphwar_module_container::MODULES`] 的原文。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Wired {
    /// `GraphwarGameStateManager`（原版 `graphwar_game_state_manager`，296 字节）。
    ///
    /// 我方等价类型 = [`crate::game::Match`]（那个模块的"运行实体那一半"）。
    GameStateManager,
    /// `GraphwarTurnManager`（原版 `graphwar_turn_manager`，32 字节）。
    TurnManager,
    /// `GraphwarPlayerManager`（原版 `graphwar_player_manager`，72 字节）。
    PlayerManager,
}

impl Wired {
    /// 本层装了几个（**不是**容器的 25 个模块类型，也不是 22 个注册点）。
    pub const COUNT: usize = 3;

    /// 全部 3 个，顺序 = 本层的**注册序**（= 任务书 §2 的顺序，**我方选择**）。
    pub const ALL: [Wired; Self::COUNT] = [
        Wired::GameStateManager,
        Wired::TurnManager,
        Wired::PlayerManager,
    ];

    /// 原版类型名（`MODULES[i].name` 的原文）。
    pub const fn type_name(self) -> &'static str {
        match self {
            Wired::GameStateManager => "GraphwarGameStateManager",
            Wired::TurnManager => "GraphwarTurnManager",
            Wired::PlayerManager => "GraphwarPlayerManager",
        }
    }

    /// 原版模块文件名（不带 `.rs`，`MODULES[i].module_path` 的原文）。
    pub const fn module_path(self) -> &'static str {
        match self {
            Wired::GameStateManager => "graphwar_game_state_manager",
            Wired::TurnManager => "graphwar_turn_manager",
            Wired::PlayerManager => "graphwar_player_manager",
        }
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

/// 槽位状态：**已装配**（容器里真能按类型查到）/ **占位**（原版有这一型，我们没装）/
/// **未知**（原版 25 型里也没有这个名字）。
pub const SLOT_ASSEMBLED: &str = "assembled";
/// 见 [`SLOT_ASSEMBLED`]。
pub const SLOT_PLACEHOLDER: &str = "placeholder";
/// 见 [`SLOT_ASSEMBLED`]。
pub const SLOT_UNKNOWN: &str = "unknown";

/// 静态判据：这个名字属于本层的装配表吗？（**运行时判据**是 [`RootWiring::has_name`]）
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

/// 三态槽位判据（**纯静态**，不看容器）：装配表 → `assembled`；25 型台账 → `placeholder`。
pub fn slot_status(name: &str) -> &'static str {
    if is_wired_name(name) {
        SLOT_ASSEMBLED
    } else if is_module_type_name(name) {
        SLOT_PLACEHOLDER
    } else {
        SLOT_UNKNOWN
    }
}

// ---------------------------------------------------------------------------
// 2. 演示数据（**我方选择**：不是原版的对局数据）
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

/// [`DEMO_SOLDIERS`] → `Match::new` 要的 `Vec<Target>`。
pub fn demo_targets() -> Vec<Target> {
    DEMO_SOLDIERS
        .iter()
        .map(|s| Target { id: s.id, x: s.x, y: s.y, radius: s.radius })
        .collect()
}

// ---------------------------------------------------------------------------
// 3. 装载器
// ---------------------------------------------------------------------------

/// 一个**已装配**的模块容器：`Rc<RefCell<GraphwarModuleContainer>>` + 注册序台账。
///
/// 为什么要包一层：原版容器只暴露 `add_module` / `get_module` / `get_module_wk`
/// 三个泛型方法（§22.5），**没有遍历口**；"装了哪几个"这件事在原版里根本不可查
/// （调用者自己知道）。本层把它记下来，仅仅是为了让验收脚本能按**类型**逐条断言，
/// 而不是数一个总数。
#[derive(Clone)]
pub struct RootWiring {
    /// 容器本体（与 `GraphwarGameClient` 持有的是同一种 `Rc<RefCell<_>>`，§22.5）。
    container: Rc<RefCell<GraphwarModuleContainer>>,
    /// 注册序台账（= [`Wired::ALL`] 的子序列，见 [`RootWiring::registered`]）。
    ///
    /// ⚠️ 它是**台账**，不是判据：所有对外回答都现查容器（[`RootWiring::has`]）。
    ledger: Vec<Wired>,
}

impl RootWiring {
    /// 一个空容器：什么都不装（用来验"按类型查不到"那一侧，别拿它当"装配失败"）。
    pub fn empty() -> Self {
        Self { container: Rc::new(RefCell::new(GraphwarModuleContainer::new())), ledger: Vec::new() }
    }

    /// **装配 3 个 manager**（本文件的主入口）。
    ///
    /// 每个都按原版的形状走 `Rc::new(RefCell::new(<M as>::new(…)))` + `add_module::<M>`；
    /// 三个 `new` 都是各自模块里**已有**的实现，本函数不再写第二份逻辑。
    ///
    /// 演示数据见 [`DEMO_SOLDIERS`] / [`DEMO_PLAYERS`] / [`demo_room`]（**我方选择**）。
    pub fn assemble_demo() -> Self {
        let mut container = GraphwarModuleContainer::new();
        let mut ledger = Vec::with_capacity(Wired::COUNT);

        // ① GraphwarGameStateManager —— 我方 = `game::Match`（运行实体那一半）。
        //    `Match::new` 的 4 个实参：房间状态 / 障碍 / 已出生的士兵 / tick 基准。
        //    障碍取空表（演示没有障碍数据，**不是**"原版没有障碍"）。
        let mut game = Match::new(demo_room(), Vec::new(), demo_targets(), (0, 0));
        // `register_player` = 原版 `request_new_player` + `add_soldier_to_player` 的接线形态
        // （写队伍 / 开函数槽 / 置生命为 alive / 进 `TurnManager`），见 `src/game.rs`。
        for p in DEMO_PLAYERS {
            game.register_player(p.player_id, p.team, p.soldiers);
        }
        container.add_module(Rc::new(RefCell::new(game)));
        ledger.push(Wired::GameStateManager);

        // ② GraphwarTurnManager —— 直接 `TurnManager::new()` + 2 个玩家。
        let mut turns = TurnManager::new();
        for p in DEMO_PLAYERS {
            turns.add_player(p.player_id, p.team, p.soldiers.to_vec());
        }
        container.add_module(Rc::new(RefCell::new(turns)));
        ledger.push(Wired::TurnManager);

        // ③ GraphwarPlayerManager —— `player_id → connection_id` 那张表。
        let mut players = GraphwarPlayerManager::new();
        for p in DEMO_PLAYERS {
            // id 都在 `u32` 范围内（1/2），溢出不 panic：直接跳过。
            if let Ok(id) = u32::try_from(p.player_id) {
                players.add_player(id, p.connection_id);
            }
        }
        container.add_module(Rc::new(RefCell::new(players)));
        ledger.push(Wired::PlayerManager);

        Self { container: Rc::new(RefCell::new(container)), ledger }
    }

    /// 容器本体（原版 `GraphwarRootNode` 那条链的起点：`Rc<RefCell<GraphwarModuleContainer>>`）。
    pub fn container(&self) -> Rc<RefCell<GraphwarModuleContainer>> {
        Rc::clone(&self.container)
    }

    /// 容器里现在有几个模块（= `RawTable.items`，原版 `get_module` 的空表快路径就是它，§22.5）。
    pub fn len(&self) -> usize {
        self.container.borrow().len()
    }

    /// 容器是不是空的。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// **按类型**查：这一型在容器里吗？（`TypeId` 比对，不看数量、不看名字）
    pub fn has(&self, which: Wired) -> bool {
        let container = self.container.borrow();
        match which {
            Wired::GameStateManager => container.contains::<Match>(),
            Wired::TurnManager => container.contains::<TurnManager>(),
            Wired::PlayerManager => container.contains::<GraphwarPlayerManager>(),
        }
    }

    /// 按**原版类型名**查（名字 → [`Wired`] → `TypeId`）。未知名字一律 `false`。
    pub fn has_name(&self, type_name: &str) -> bool {
        match Wired::from_type_name(type_name) {
            Some(which) => self.has(which),
            None => false,
        }
    }

    /// 按类型查**成立**的那几个，顺序 = 注册序（台账的子序列；判据仍是容器）。
    pub fn registered(&self) -> Vec<Wired> {
        self.ledger.iter().copied().filter(|w| self.has(*w)).collect()
    }

    /// 注册序台账的名字（[`RootWiring::registered`] 的字符串形态）。
    pub fn registered_names(&self) -> Vec<&'static str> {
        self.registered().into_iter().map(|w| w.type_name()).collect()
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
}

// ---------------------------------------------------------------------------
// 4. 单测（**不碰引擎**：本文件零 Godot 依赖）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 3 个名字都必须能在原版 25 型台账里查到（名字不是我们编的）。
    #[test]
    fn the_three_names_come_from_the_module_ledger() {
        assert_eq!(Wired::ALL.len(), Wired::COUNT);
        assert_eq!(Wired::COUNT, 3);
        for w in Wired::ALL {
            let entry = graphwar_module_container::find(w.type_name())
                .unwrap_or_else(|| panic!("{} 不在 MODULES 里", w.type_name()));
            assert_eq!(entry.module_path, w.module_path(), "{} 的模块名对不上", w.type_name());
            assert_eq!(Wired::from_type_name(w.type_name()), Some(w));
            assert_eq!(Wired::from_module_path(w.module_path()), Some(w));
            assert_eq!(slot_status(w.type_name()), SLOT_ASSEMBLED);
        }
        // 模块名互不相同（否则 `from_module_path` 会歧义）
        let mut paths: Vec<&str> = Wired::ALL.iter().map(|w| w.module_path()).collect();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), Wired::COUNT);
    }

    /// 没装的原版型一律是 `placeholder`（不是 `assembled`，也不是 `unknown`）。
    #[test]
    fn unwired_module_types_are_placeholders() {
        assert_eq!(slot_status("GraphwarBanList"), SLOT_PLACEHOLDER);
        assert_eq!(slot_status("GraphwarSoldierManager"), SLOT_PLACEHOLDER);
        assert_eq!(slot_status("FunctionHistoryRecorder"), SLOT_PLACEHOLDER);
        assert_eq!(slot_status("nonsense"), SLOT_UNKNOWN);
        assert_eq!(slot_status("graphwar_turn_manager"), SLOT_UNKNOWN, "只认类型名，不认模块名");
        // 25 型里只有 3 个是 assembled
        let assembled = graphwar_module_container::MODULES
            .iter()
            .filter(|m| slot_status(m.name) == SLOT_ASSEMBLED)
            .count();
        assert_eq!(assembled, Wired::COUNT);
        assert_eq!(graphwar_module_container::MODULES.len(), 25);
    }

    /// 空容器：3 个都查不到（这一条是"按类型断言"的反面——数量 0，但不是靠数量判的）。
    #[test]
    fn empty_container_has_none_of_the_three() {
        let wiring = RootWiring::empty();
        assert_eq!(wiring.len(), 0);
        assert!(wiring.is_empty());
        for w in Wired::ALL {
            assert!(!wiring.has(w), "{} 不该在空容器里", w.type_name());
            assert!(!wiring.has_name(w.type_name()));
            assert!(wiring.game_state().is_none());
            assert!(wiring.turn_manager().is_none());
            assert!(wiring.player_manager().is_none());
        }
        assert!(wiring.registered().is_empty());
    }

    /// 装配后：3 个都能**按类型**取回，而且是同一个 `Rc`（容器里存的就是它）。
    #[test]
    fn assemble_registers_three_modules_retrievable_by_type_id() {
        let wiring = RootWiring::assemble_demo();
        assert_eq!(wiring.len(), Wired::COUNT);
        for w in Wired::ALL {
            assert!(wiring.has(w), "{} 装配后查不到", w.type_name());
            assert!(wiring.has_name(w.type_name()));
        }
        assert_eq!(wiring.registered().len(), Wired::COUNT);
        assert_eq!(
            wiring.registered_names(),
            vec!["GraphwarGameStateManager", "GraphwarTurnManager", "GraphwarPlayerManager"]
        );
        // 没装的类型仍然查不到（容器里确实只有 3 个键）
        assert!(!wiring.has_name("GraphwarBanList"));
        assert!(!wiring.has_name("GraphwarSoldierManager"));

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

        // 同一个 `Rc`：通过容器两次取回，改一次两边都看得到
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
