//! **不是原版的东西**：把复原的 [`crate::graphwar_root_node`] 挂成**真 Godot 节点类**的骨架。
//!
//! # 这一份：**骨架 + 3 个 manager 的真装配**（`docs/16-client.md` §38）
//!
//! 原版 `GraphwarRootNode` 是主场景节点（`docs/16-client.md` §15），它干三件事：
//!
//! 1. **装配**：把屏表（15 个 `Control`）、两个控制器句柄（`+0x1668` / `+0x16c8` 的
//!    `Rc<RefCell<T>>`）、贴图加载器（`+0x16a0`）、以及模块容器里的一批 manager 接起来；
//! 2. **切屏**：`0x3af850` —— 按名字让一个屏可见、其余不可见，进出口时
//!    `call_deferred("create_ad_view"|"destroy_ad_view")`；
//! 3. **状态机**：`0x3a5760`（20288 字节、339 条比较、64 个直接调用目标）+ 命令总线 `0x3b0830`。
//!
//! **本文件做三件事**（第 1 条只做了"22 个 manager 的装配"那一小块，见下）：
//!
//! * 建出 `_ready` 里真的 `get_node` 过的那 3 个 holder（[`READY_NODES`]，**观测**）：
//!   `ScreensControl` / `IAP_holder` / `Ad_holder`，其中 `ScreensControl` 下挂 15 个屏节点、
//!   `Ad_holder` 是**另一个扩展类** [`super::graphwar_ad_switch_class::GraphwarAdSwitch`]；
//! * [`GraphwarRootNode::show_screen`] 真正跑一遍 `graphwar_root_node::apply_show_screen`
//!   （`0x3af850` 决策表的纯逻辑版），把 `set_visible` **转发到真子节点**上；
//! * [`GraphwarRootNode::purchase_confirmed`] / [`GraphwarRootNode::purchase_denied`]
//!   把一次调用**转发给子对象** `Ad_holder`（arm64 `0x3a658c` / `0x3a61b0` 就是写那个
//!   `+0x18` 字节的两条指令）。
//!
//! # 真装配：**22 个 manager 按原版注册序进容器**（`docs/16-client.md` §41）
//!
//! 原版容器里 **22 条 `add_module`**（`GraphwarGameClient::new`，`docs/02` §22.4）；
//! 本类现在把这 **22 个全装上**，装配器在 [`super::root_wiring`]：
//!
//! ```text
//! GraphwarRootNode（本类）
//! ├── ScreensControl
//! │   ├── GameSetup        ← **真节点类** GraphwarSetupControl（Setup 屏）
//! │   ├── PlayScreen       ← **Game 屏**（Control 容器）
//! │   │   └── GraphwarPlane            ← 已有节点类（画棋盘框）
//! │   │       └── @EntityDrawer@N      ← 已有节点类（3 假实体 × 4 子节点）
//! │   └── …其余 13 个屏（Control 占位）
//! ├── IAP_holder / Ad_holder
//! └── GraphwarModuleContainer（25 个 **占位** Node；真装配在 Rust 侧的 TypeId 表里）
//! ```
//!
//! * 容器 = `Rc<RefCell<GraphwarModuleContainer>>`，键是 **`TypeId`**（`docs/02` §22.1），
//!   取回走 `get_module::<M>()`；**没有任何 `+0xNN` 偏移**；
//! * 按类型逐个可查：`container_has_module("GraphwarTurnManager")` 之类，
//!   还能再走一档 `container_module_live(...)`（真 downcast 回来）；
//!   22 型按**四档**记账（2026-11-08 任务 28 起，判据见
//!   [`super::root_wiring`] 的模块文档与 `docs/16` §47）：`wired` 4 / `layout` 2 /
//!   `assembled` 15 / `stub` **1**（只剩 `GraphwarPlanePosManager`：24 B 布局未知）。
//!   ⚠️ 旧文写的"17 个复原类型 + 5 个零行为空占位"是**三档时代**的口径，已作废；
//!   客户端**不**注册的那 3 型
//!   （`GraphwarBanList` / `GraphwarServerConnectionHandler` / `GraphwarGameRecorder`）
//!   如实回 `placeholder`（[`GraphwarRootNode::module_slot_status`]）；
//! * 屏里的 3 个假实体与容器里 `GameStateManager` 的 3 个 `Target` **是同一份数据**
//!   （[`super::root_wiring::DEMO_SOLDIERS`]）。
//!
//! **仍然刻意不做**（`AGENTS.md` §0 第 3 条：没证据就不猜）：
//!
//! | 没做的东西 | 为什么 |
//! |---|---|
//! | `0x3a5760`（20288 B 状态机）的主体 | 原版形状本身没解出来（可能是 `_process` + 大量内联），不猜 |
//! | `0x3b0830`（命令总线）的**全部**键 | 只解到 12 条（[`crate::graphwar_root_node::Command`]），131 KB 反编译没过完 |
//! | **17 个 manager 的行为** | 本轮只做"构造 + 注册"（任务书 §4 明令不实现行为）；容器里是**空表**，没人驱动它们 |
//! | **只剩 1 个槽结构未知** | `GraphwarPlanePosManager`（24 B 布局未知、无事件流）⇒ 档位 `stub`。另外四型已升级：`ClientConnectionHandler`(112 B) / `FunctionManager`(32 B) 布局全解但行为未做完 ⇒ `layout`；`TickHandler` / `ObstacleManager` ⇒ `wired`（`docs/16` §47.2） |
//! | 两个 `Rc<RefCell<T>>`（`+0x1668` / `+0x16c8`） | 那两个控制器的类型与生命周期未解（`docs/16` §15.3 只给了偏移） |
//! | `GraphwarTextureLoader`（`+0x16a0`） | 模块存在（`docs/16` §28），但节点层还没有 |
//!
//! ⚠️ `GraphwarModuleContainer` 节点下的 25 个空 `Node` 名字**就是**原版类名
//! （`GraphwarTurnManager` 之类），但它们仍然只是空节点——**真装配在 Rust 的容器里**，
//! 不在节点树里。两处别混：`module_slot_status()` 说的是**容器**里装没装。
//!
//! # 基类：`Node`（**观测**）
//!
//! arm64 2.0.60 的 `.symtab` 直接把基类写进了符号：
//!
//! ```text
//! <GraphwarRootNode as godot_core::gen::classes::node::re_export::INode>::ready
//! <GraphwarRootNode as godot_core::gen::classes::node::re_export::INode>::process
//! ```
//!
//! ⇒ 基类 = `Node`、且**覆写了 `ready` 与 `process`**（这两条都不是猜的）。
//! 12 个类的同一份对照表见 `docs/16-client.md` 本轮新增的那一节。
//!
//! # 换基类要改什么（本批三个类的实测结论）
//!
//! `Node` 用 `impl INode`、`VBoxContainer` 用 `impl IVBoxContainer`、`Control` 用 `impl IControl`
//! —— **只有那个块要改**（trait 名 + `Base<T>` 类型 + `init` 的参数类型，共 3 处）。
//! `#[godot_api] impl 类名 { #[func] … #[export] … }` 与基类**无关**，一个字都不用改。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/base_classes_check.gd
//! ```
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! GraphwarRootNode : Node
//!   @export ads_slot : bool        # 0x3af850 的第 4 实参（推断：这次切屏允许动广告）
//!
//!   build_scene_graph() -> int     # 幂等；建 3 个 holder + 15 个屏 + N 个 module 占位
//!   holder_names() -> PackedStringArray
//!   screen_count() -> int
//!   screen_names() -> PackedStringArray
//!   show_screen(name) -> int       # 跑一遍 0x3af850 的决策表，返回写了多少次 set_visible
//!   visible_screens() -> PackedStringArray
//!   set_screen_visible(name, visible) -> bool
//!   deferred_log() -> PackedStringArray    # create_ad_view / destroy_ad_view 的调用顺序
//!   ad_holder_present() -> bool
//!   ads_enabled() -> bool                  # 转发给 Ad_holder
//!   purchase_confirmed() / purchase_denied() -> bool   # 原版写 +0x18 的两条
//!   manager_slot_count() -> int
//!   ready_call_count() / process_call_count() -> int
//!
//!   # ---- §42 真装配（容器侧）----
//!   wiring_done() -> bool                         # 容器已经装好（build_scene_graph 会做）
//!   container_module_count() -> int               # 容器里几个模块（= HashMap.len()）
//!   container_module_names() -> PackedStringArray # **按类型查成立**的那几个，注册序
//!   container_has_module(type_name) -> bool       # 按 TypeId 查（不是按数量）
//!   container_game_state_soldiers() -> int        # 取回 GameStateManager 再读（士兵数）
//!   container_turn_manager_soldiers(player_id) -> int
//!   container_player_manager_players() -> int
//!   module_slot_status(type_name) -> String       # "assembled" / "placeholder" / "unknown"
//!   wired_manager_count() -> int                  # 按类型查成立的个数（= 3）
//! ```

use std::collections::BTreeMap;

use godot::classes::{Control, INode, Node};
use godot::prelude::*;

use crate::graphwar_module_container::{self, MODULES};
use crate::graphwar_root_node::{apply_show_screen, Screen, ScreenHost, ALL_SCREENS, READY_NODES};

use super::entity_drawer_class::EntityDrawer;
use super::graphwar_ad_switch_class::GraphwarAdSwitch;
use super::graphwar_plane_class::GraphwarPlane;
use super::graphwar_setup_control_class::GraphwarSetupControl;
use super::root_wiring::{self, RootWiring, DEMO_SOLDIERS, Wired};

/// `_ready` 里三个 holder 的节点名（[`READY_NODES`]，**观测**）。
///
/// 顺序同原版：`ScreensControl` → `IAP_holder` → `Ad_holder`。
pub const HOLDER_NAMES: [&str; 3] = [READY_NODES[0].name, READY_NODES[1].name, READY_NODES[2].name];

/// `Ad_holder` 挂的扩展类名（**推断**：`docs/16` §15.4 记的 `upcast_ref` 目标池里
/// 有 `graphwar_ad_switch::GraphwarAdSwitch`，但没有逐条对上哪个 holder 上抛到哪个类）。
pub const AD_HOLDER_CLASS: &str = crate::graphwar_ad_switch::CLASS;

/// `GraphwarGameClient::new` 注册了多少个 manager（`graphwar_module_container::REGISTRATION_SITES[0]`）。
///
/// 档位：**观测**（arm64 `0x409c70` 的注册点数；`graphwar_module_container` 的
/// `ADD_MODULE_COUNT` = 25 是**表里**的模块类型数，两者不同）。
pub const MANAGER_REGISTRATION_COUNT: usize = graphwar_module_container::REGISTRATION_SITES[0].count;

/// **Setup 屏**的屏名（`docs/16` §15.2 的 `.rodata` 原文 `0x1bda32`，9 字节）。
///
/// 与 `Screen::GameSetup.name()` 相等（单测断言，不另写第二份名字）。
pub const SETUP_SCREEN_NAME: &str = "GameSetup";

/// **Game 屏**的屏名（`docs/16` §15.2 的 `.rodata` 原文 `0x1bd92b`，10 字节）。
///
/// ⚠️ 它是 `0x3af850` 里**被单列处理**的那一屏（进出口要动广告），但这一条不影响
/// 本文件把它当成"Game 屏"用：Game 屏的内容（棋盘 / 士兵）挂在这一屏下面。
pub const GAME_SCREEN_NAME: &str = "PlayScreen";

/// Game 屏里棋盘节点的**节点名**。
///
/// 档位：**观测** —— `docs/16` §2.1.10 的运行时 dump 里那条链是
/// `PlayScreen/.../GamePlane/PlaneContainer/GraphwarPlane`，节点自己的名字就是它。
pub const PLANE_NODE_NAME: &str = "GraphwarPlane";

/// Game 屏（以及棋盘 / 绘制器）的尺寸 = `client/project.godot` 的
/// `display/window/size/viewport_width/height`（1600×900）。
///
/// 档位：**我方选择** —— 原版这一屏的尺寸由 `.tscn` 的容器布局给
/// （`PlayScreen/.../PlaneContainer`），我们没有那套场景；写死一个视口尺寸
/// 只是为了让 `entity_drawer::sprite_transform` 的 `k = size.x / 50` 不至于乘 0。
pub const VIEWPORT_SIZE: (f32, f32) = (1600.0, 900.0);

/// 把 `[`crate::graphwar_root_node::ScreenHost`] 落到真子节点上的适配器。
///
/// 原版这三条对应三个 gdext 调用（`docs/16` §15.5）：
/// * `upcast_ref` + `call *0x9ba978`（收一个 bool）→ [`ScreenHost::set_visible`]；
/// * `0x39fe10` + `call *0x9ba720`（`(out, gd, name, len, 8, 0)`）→ [`ScreenHost::call_deferred`]；
/// * `call *0x9baa20`（无参、返回 bool）→ [`ScreenHost::play_screen_visible`]。
///
/// 三条 GOT 槽的**引擎成员名至今未定名**（`docs/16` §15.9），所以这里只写意图结构。
struct NodeScreenHost<'a> {
    /// 屏名 → 真 `Control` 节点（屏表 `+0x1638`）。
    screens: &'a mut BTreeMap<Screen, Gd<Control>>,
    /// `call_deferred` 的方法名流水（`Ad_holder` 上根本没有这两个方法，只记名字）。
    deferred: &'a mut Vec<GString>,
    /// 真的写出了多少次 `set_visible`（观测点）。
    writes: &'a mut i64,
}

impl ScreenHost for NodeScreenHost<'_> {
    /// `set_visible(entry, visible)`。
    ///
    /// 屏表里只有 [`Screen`] 的 15 个名字；`background` 这类键不在枚举里 ⇒ 跳过
    /// （原版里 `background` 命中的分支本来也是 `Leave`，不调 `set_visible`）。
    fn set_visible(&mut self, screen: &str, visible: bool) {
        let Ok(key) = Screen::from_name(screen) else {
            return;
        };
        if let Some(node) = self.screens.get_mut(&key) {
            node.set_visible(visible);
            *self.writes += 1;
        }
    }

    /// `Ad_holder.call_deferred(method)` —— **只记账**。
    ///
    /// 原版这两个方法由 Android 的 AdMob 插件提供（`docs/19-android.md`），
    /// 本仓库没有那个插件；调一个不存在的方法只会往 stderr 喷错误。
    fn call_deferred(&mut self, method: &str) {
        self.deferred.push(GString::from(method));
    }

    /// `PlayScreen` 当前可见性（原版只对它取这一个 bool，`0x3af9c7` / `0x3afa7d`）。
    fn play_screen_visible(&self) -> bool {
        self.screens
            .get(&Screen::PlayScreen)
            .map(|node| node.is_visible())
            .unwrap_or(false)
    }
}

/// `GraphwarRootNode` 的节点骨架。
///
/// 子节点树（建好之后）：
///
/// ```text
/// GraphwarRootNode            ← 本类（Node）
/// ├── ScreensControl         ← 空 Node 容器（原版 ready 里 get_node 过）
/// │   ├── MainMenu … DisconnectedPopUp   （15 个 Control，名字 = Screen::name()）
/// │   ├── GameSetup          ← **Setup 屏**：GraphwarSetupControl（真节点类）
/// │   └── PlayScreen         ← **Game 屏**：Control 容器
/// │       └── GraphwarPlane            ← 棋盘本体（真节点类）
/// │           └── @EntityDrawer@N      ← 实体绘制器（真节点类，自动名）
/// ├── IAP_holder             ← 空 Node 占位（原版 ready 里 get_node 过）
/// ├── Ad_holder              ← GraphwarAdSwitch（另一个扩展类）
/// └── GraphwarModuleContainer
///     └── GraphwarBanList … （25 个空 Node，名字 = 模块类名；**真装配在 Rust 容器里**）
/// ```
#[derive(GodotClass)]
#[class(base = Node)]
pub struct GraphwarRootNode {
    base: Base<Node>,

    /// `0x3af850` 的第 4 实参（那个 16 字节捕获态里的 bool）。
    ///
    /// 档位：**推断**（`0x3b0830` 的 4 个调用点把它写死 0、`0x3a7bed`（切到 `PlayScreen`）
    /// 写死 1 ⇒ 最自然的语义是"这次切屏允许动广告"；缺的证据：运行时读那个捕获态）。
    #[export]
    ads_slot: bool,

    /// 屏表（原版 `self+0x1638` 的 `HashMap<&'static str, Gd<Node>>`；这里按枚举键排，
    /// 因为原版的**迭代序未知**，见 [`crate::graphwar_root_node::screen_swap_plan`] 的注）。
    screens: BTreeMap<Screen, Gd<Control>>,

    /// `Ad_holder` 那个子对象（**推断**它是 [`GraphwarAdSwitch`]）。
    ad_holder: Option<Gd<GraphwarAdSwitch>>,

    /// **已装配的模块容器**（22 个 manager，顺序 = 原版注册序，见 [`super::root_wiring`]）。
    ///
    /// 原版这条链是 `root+0x1668`（某个 manager 的 `Rc<RefCell<_>>`）→ 该 manager `+0x238`
    /// （`Rc<RefCell<GraphwarModuleContainer>>`）→ `.borrow().get_module::<X>()`
    /// （`docs/02` §22.1）。我们没有那两跳，直接把容器挂在根节点上——**形状不同**，
    /// 但容器的语义（`TypeId` 键 + `Rc<RefCell<M>>` 值）是同一个。
    wiring: Option<RootWiring>,

    /// **Setup 屏**的真节点类（`GameSetup` 那一屏）。
    setup_screen: Option<Gd<GraphwarSetupControl>>,

    /// **Game 屏**里的棋盘本体（`GraphwarPlane`，`PlayScreen/GraphwarPlane`）。
    plane: Option<Gd<GraphwarPlane>>,

    /// 棋盘下的实体绘制器（`GraphwarPlane/@EntityDrawer@N`，**观测**的父子关系）。
    entity_drawer: Option<Gd<EntityDrawer>>,

    /// 三个 holder 是否已经建好（幂等标志）。
    built: bool,

    /// `call_deferred` 的方法名流水。
    deferred_log: Vec<GString>,

    /// `_ready` / `_process` 被调到几次（观测点）。
    ready_calls: i64,
    process_calls: i64,
    /// 累计 `set_visible` 写成功多少次（观测点）。
    visibility_writes: i64,
    /// 最近一次 [`GraphwarRootNode::show_screen`] 的屏名。
    last_queried: GString,
}

impl GraphwarRootNode {
    /// 建出节点骨架（幂等）。原版在 `_ready`（`0x3a2e10`）里装配；这里 `ready` 与每个
    /// `#[func]` 入口都会先调一次，理由同 `graphwar_lan_vbox_class`：`--script` 模式下
    /// 节点可能先进树后建树，验收脚本要在同一帧里断言。
    fn ensure_scene_graph(&mut self) -> i64 {
        if self.built {
            return self.child_slot_count();
        }

        // ⓪ **模块容器装配**（`docs/16` §41）：22 个 manager 按原版注册序、按 `TypeId` 进容器。
        //    先于建屏做，因为 Game 屏里的 3 个假实体与容器里 `GameStateManager` 的
        //    3 个 `Target` 是同一份数据（[`DEMO_SOLDIERS`]）。
        if self.wiring.is_none() {
            self.wiring = Some(RootWiring::assemble_demo());
        }

        // ① ScreensControl + 15 个屏（原版：`get_node("ScreensControl")` 之后写屏表 `+0x1638`）
        let mut screens_control = Node::new_alloc();
        screens_control.set_name(&StringName::from(HOLDER_NAMES[0]));
        for screen in ALL_SCREENS {
            if screen == Screen::GameSetup {
                // Setup 屏：**真节点类** `GraphwarSetupControl`（`docs/16` §35 那个类）。
                // 原版这一屏在 `.tscn` 里，我们这里由这个类自己建替身子节点。
                let mut node = GraphwarSetupControl::new_alloc();
                node.set_name(&StringName::from(SETUP_SCREEN_NAME));
                node.set_visible(false);
                node.set_size(Vector2::new(VIEWPORT_SIZE.0, VIEWPORT_SIZE.1));
                screens_control.add_child(&node.clone().upcast::<Node>());
                self.setup_screen = Some(node.clone());
                // 屏表装的是 `Control`（屏的基类），`GraphwarSetupControl` 是它的子类。
                self.screens.insert(screen, node.upcast::<Control>());
                continue;
            }
            if screen == Screen::PlayScreen {
                // Game 屏：`Control` 容器 + 棋盘 + 实体绘制器（见 `build_game_screen`）。
                // 原版这一屏有 `play_screen_control`（Rust）；我们还没有那个模块的节点层，
                // 所以这里只是一个容器 —— **不是**"已经复原了 PlayScreenControl"。
                let mut node = Control::new_alloc();
                node.set_name(&StringName::from(GAME_SCREEN_NAME));
                node.set_visible(false);
                self.build_game_screen(&mut node);
                screens_control.add_child(&node.clone().upcast::<Node>());
                self.screens.insert(screen, node);
                continue;
            }
            // 其余 13 个屏仍是 `Control` 占位：原版对它们调的是「收一个 bool」的可见性成员
            // （`0x9ba978`，**推断** = `CanvasItem::set_visible`）⇒ 必须是 `CanvasItem` 系。
            let mut node = Control::new_alloc();
            node.set_name(&StringName::from(screen.name()));
            node.set_visible(false);
            screens_control.add_child(&node.clone().upcast::<Node>());
            self.screens.insert(screen, node);
        }
        self.base_mut()
            .add_child(&screens_control.upcast::<Node>());

        // ② IAP_holder：原版只 `get_node` 一次（内购信号挂在它上面），本骨架留空占位
        let mut iap = Node::new_alloc();
        iap.set_name(&StringName::from(HOLDER_NAMES[1]));
        self.base_mut().add_child(&iap.upcast::<Node>());

        // ③ Ad_holder：**另一个扩展类** —— 这就是"持有子对象并转发一次调用"的那条链路
        let mut ad = GraphwarAdSwitch::new_alloc();
        ad.set_name(&StringName::from(HOLDER_NAMES[2]));
        self.base_mut().add_child(&ad.clone().upcast::<Node>());
        self.ad_holder = Some(ad);

        // ④ 管理器**节点槽**：一个空 Node 容器 + 按 `MODULES` 建的空 Node。
        //    ⚠️ 名字是原版类名，但节点是空的 —— **真装配在 Rust 容器里**（第 ⓪ 步），
        //    节点树这一层只是把 25 个槽画出来（`module_slot_status()` 才回答装没装）。
        let mut container = Node::new_alloc();
        container.set_name(&StringName::from("GraphwarModuleContainer"));
        for entry in MODULES {
            let mut slot = Node::new_alloc();
            slot.set_name(&StringName::from(entry.name));
            container.add_child(&slot.upcast::<Node>());
        }
        self.base_mut().add_child(&container.upcast::<Node>());

        self.built = true;
        self.child_slot_count()
    }

    /// Game 屏（`PlayScreen`）的内容：棋盘 + 实体绘制器 + 3 个假实体。
    ///
    /// 节点的**名字与父子关系**都是观测到的（`docs/16` §2.1.10 的运行时 dump）：
    ///
    /// ```text
    /// PlayScreen/.../GraphwarPlane            ← 有自己的名字
    /// GraphwarPlane/@EntityDrawer@N           ← **没有** set_name（引擎自动名）
    /// ```
    ///
    /// 3 个假实体走 [`EntityDrawer`] 的 `spawn_entity`（每个建 3 个 `Sprite2D` + 1 个
    /// `RichTextLabel`，`docs/16` §34）；它们的 id / 坐标 / 名字来自
    /// [`super::root_wiring::DEMO_SOLDIERS`]（**我方选择**的假数据）。
    fn build_game_screen(&mut self, screen: &mut Gd<Control>) {
        let size = Vector2::new(VIEWPORT_SIZE.0, VIEWPORT_SIZE.1);
        screen.set_size(size);

        // 棋盘本体：`GraphwarPlane`（画一个棋盘框 + 3 个 `FunctionDrawer` 占位子节点）。
        let mut plane = GraphwarPlane::new_alloc();
        plane.set_name(&StringName::from(PLANE_NODE_NAME));
        plane.set_size(size);

        // 实体绘制器：原版是 `GraphwarPlane` 的**子节点**、**没** `set_name`
        // ⇒ 这里也不命名（引擎会给 `@EntityDrawer@N`，与 dump 里一致）。
        let mut drawer = EntityDrawer::new_alloc();
        drawer.set_size(size);
        // 3 个假士兵：`spawn_entity` / `set_entity_name` 是本类的 `#[func]`
        // （`entity_drawer_class.rs` 里是私有方法）⇒ 走动态调用，等价于 GDScript 那一侧。
        for soldier in DEMO_SOLDIERS {
            drawer.call(
                "spawn_entity",
                &[soldier.id.to_variant(), soldier.x.to_variant(), soldier.y.to_variant()],
            );
            drawer.call(
                "set_entity_name",
                &[soldier.id.to_variant(), GString::from(soldier.name).to_variant()],
            );
        }

        plane.add_child(&drawer.clone().upcast::<Node>());
        screen.add_child(&plane.clone().upcast::<Node>());

        self.plane = Some(plane);
        self.entity_drawer = Some(drawer);
    }

    /// 根节点当前有几个直接子节点（观测点）。
    fn child_slot_count(&self) -> i64 {
        self.base().get_child_count() as i64
    }
}

#[godot_api]
impl INode for GraphwarRootNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            ads_slot: false,
            screens: BTreeMap::new(),
            ad_holder: None,
            wiring: None,
            setup_screen: None,
            plane: None,
            entity_drawer: None,
            built: false,
            deferred_log: Vec::new(),
            ready_calls: 0,
            process_calls: 0,
            visibility_writes: 0,
            last_queried: GString::new(),
        }
    }

    /// `_ready`（原版 `0x3a2e10`，**观测**：3 个 `get_node` + 14 次 `upcast_ref` + 一批 `connect`）。
    ///
    /// 本骨架只做那 3 个 `get_node` 对应的**节点形状**；`connect`（6 个信号 / 事件）**没做**
    /// —— `connect` 用的 GOT 槽 `0x9bb030` 至今未定名（`docs/14` §9.2）。
    fn ready(&mut self) {
        self.ready_calls += 1;
        self.ensure_scene_graph();
    }

    /// `_process`（原版是 `0x3a5760` 那 20288 字节状态机的一部分，**形状未解**）。
    ///
    /// 骨架里只计数：**不猜**状态机做了什么（`graphwar_root_node` 的未知清单第 8 条）。
    /// 不写 `set_process(true)`：Godot 4.7 自动开（`docs/16` §33.4 第 5 条，本轮再次实测）。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;
    }
}

#[godot_api]
impl GraphwarRootNode {
    /// 建出节点骨架（幂等），返回**直接子节点数**。
    #[func]
    fn build_scene_graph(&mut self) -> i64 {
        self.ensure_scene_graph()
    }

    /// 三个 holder 的名字（顺序同原版 `_ready`）。
    #[func]
    fn holder_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for name in HOLDER_NAMES {
            out.push(&GString::from(name));
        }
        out
    }

    /// 屏表里有多少个屏。
    #[func]
    fn screen_count(&self) -> i64 {
        self.screens.len() as i64
    }

    /// 屏名（按枚举序 = `docs/16` §15.2 的表序；原版是 `HashMap` 迭代序，**未知**）。
    #[func]
    fn screen_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for screen in self.screens.keys() {
            out.push(&GString::from(screen.name()));
        }
        out
    }

    /// **切屏**：跑一遍 `0x3af850` 的决策表（纯逻辑在
    /// [`crate::graphwar_root_node::apply_show_screen`]），把 `set_visible` 转发到真子节点。
    ///
    /// 返回真的写出了多少次 `set_visible`（`background` / 不在屏表里的键不算，也不写）。
    ///
    /// 原版这条路的两个单列分支（`PlayScreen` 进出口要动广告）由
    /// [`graphwar_root_node::ScreenSwap`] 决定，`call_deferred` 的方法名进
    /// [`GraphwarRootNode::deferred_log`]。
    #[func]
    fn show_screen(&mut self, name: GString) -> i64 {
        self.ensure_scene_graph();

        let queried = name.to_string();
        let entries: Vec<&'static str> = self.screens.keys().map(|s| s.name()).collect();
        let ads_slot = self.ads_slot;
        let mut writes = 0i64;

        // `deferred_log` 先取出来再放回去：`NodeScreenHost` 同时要 `&mut screens` 与
        // `&mut deferred`，而后者是 `self` 的一个字段 —— 不能同时从 `self` 借两次。
        let mut deferred = std::mem::take(&mut self.deferred_log);
        {
            let mut host = NodeScreenHost {
                screens: &mut self.screens,
                deferred: &mut deferred,
                writes: &mut writes,
            };
            apply_show_screen(&mut host, &entries, &queried, ads_slot);
        }
        self.deferred_log = deferred;

        self.visibility_writes += writes;
        self.last_queried = name;
        writes
    }

    /// 当前可见的屏名（按枚举序）。
    #[func]
    fn visible_screens(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for (screen, node) in self.screens.iter() {
            if node.is_visible() {
                out.push(&GString::from(screen.name()));
            }
        }
        out
    }

    /// 单独设某个屏的可见性（`screen_swap` 之外的旁路，方便验收脚本摆前置状态）。
    #[func]
    fn set_screen_visible(&mut self, name: GString, visible: bool) -> bool {
        self.ensure_scene_graph();
        let Ok(key) = Screen::from_name(&name.to_string()) else {
            return false;
        };
        let Some(node) = self.screens.get_mut(&key) else {
            return false;
        };
        node.set_visible(visible);
        true
    }

    /// `call_deferred` 的方法名流水（顺序 = 原版的先后：`call_deferred` 先于 `set_visible`）。
    #[func]
    fn deferred_log(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for method in self.deferred_log.iter() {
            out.push(method);
        }
        out
    }

    /// `Ad_holder` 在不在（**推断**它是个 `GraphwarAdSwitch`）。
    #[func]
    fn ad_holder_present(&self) -> bool {
        self.ad_holder.is_some()
    }

    /// 转发给 `Ad_holder`：`are_ads_enabled()` = `!owned`。
    #[func]
    fn ads_enabled(&mut self) -> bool {
        self.ensure_scene_graph();
        match self.ad_holder.clone() {
            Some(holder) => holder.bind().are_ads_enabled(),
            // 没有 Ad_holder ⇒ 广告不可用（原版 `0x3af850` 一开头 `test rbx,rbx; je` 就返回）
            None => false,
        }
    }

    /// `GraphwarRootNode::purchase_confirmed()`：把 `Ad_holder` 的 `owned` 写成 `true`
    /// （arm64 `0x3a658c` `strb w10(=1),[x8,#0x18]`，**观测**）。
    ///
    /// 返回写完之后 `are_ads_enabled()` 的值（买断 ⇒ 关广告 ⇒ `false`）。
    #[func]
    fn purchase_confirmed(&mut self) -> bool {
        self.ensure_scene_graph();
        let Some(mut holder) = self.ad_holder.clone() else {
            return self.ads_enabled();
        };
        holder.bind_mut().write_owned(true);
        self.ads_enabled()
    }

    /// `GraphwarRootNode::purchase_denied()`：`owned = false`（arm64 `0x3a61b0`
    /// `strb wzr,[x8,#0x18]`，**观测**）。
    #[func]
    fn purchase_denied(&mut self) -> bool {
        self.ensure_scene_graph();
        let Some(mut holder) = self.ad_holder.clone() else {
            return self.ads_enabled();
        };
        holder.bind_mut().write_owned(false);
        self.ads_enabled()
    }

    /// 模块容器里有多少个**槽**（`graphwar_module_container::MODULES`，25 个模块类型）。
    #[func]
    fn manager_slot_count(&self) -> i64 {
        MODULES.len() as i64
    }

    /// 最大的一处注册点注册了多少个 manager（`GraphwarGameClient::new` = 22）。
    #[func]
    fn manager_registration_count(&self) -> i64 {
        MANAGER_REGISTRATION_COUNT as i64
    }

    // -----------------------------------------------------------------------
    // §42：容器真装配的观测口（全部走 `TypeId`，没有一处读 `+0xNN`）
    // -----------------------------------------------------------------------

    /// 容器装好了吗（`build_scene_graph` / `_ready` 会做）。
    #[func]
    fn wiring_done(&mut self) -> bool {
        self.ensure_scene_graph();
        self.wiring.is_some()
    }

    /// 容器里现在有几个模块（= `HashMap::len`，原版 `get_module` 的空表快路径就是它）。
    ///
    /// ⚠️ 这只是**一个数**：验收要按类型逐条断言，见
    /// [`GraphwarRootNode::container_has_module`]。
    #[func]
    fn container_module_count(&mut self) -> i64 {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.len() as i64,
            None => -1,
        }
    }

    /// **按类型**查在不在容器里（名字 → `TypeId` → `contains::<M>()`）。
    ///
    /// 名字是原版类型名（`graphwar_module_container::MODULES` 的原文）；未知名字一律 `false`。
    /// 这一条**不是**按数量判的：`container_has_module("GraphwarBanList")` 在装了 3 个之后
    /// 仍然是 `false`（那 22 型只有占位槽，容器里没有键）。
    #[func]
    fn container_has_module(&mut self, type_name: GString) -> bool {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.has_name(&type_name.to_string()),
            None => false,
        }
    }

    /// 按类型查**成立**的那几个名字，顺序 = **原版注册序**（`docs/16-client.md` §41.2 的
    /// 22 条：`GraphwarTickHandler` → … → `GraphwarRankManager`）。
    #[func]
    fn container_module_names(&mut self) -> PackedStringArray {
        self.ensure_scene_graph();
        let mut out = PackedStringArray::new();
        if let Some(w) = self.wiring.as_ref() {
            for name in w.registered_names() {
                out.push(&GString::from(name));
            }
        }
        out
    }

    /// 按类型查成立的个数（[`GraphwarRootNode::container_module_count`] 的**类型版**）。
    #[func]
    fn wired_manager_count(&mut self) -> i64 {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.registered().len() as i64,
            None => -1,
        }
    }

    /// **取回 `GameStateManager` 再读一次**（证明 `get_module` 拿到的是活对象，不是"表里有个键"）。
    ///
    /// 返回 `Match::soldiers.len()`；容器里没有这一型 ⇒ `-1`。
    #[func]
    fn container_game_state_soldiers(&mut self) -> i64 {
        self.ensure_scene_graph();
        let Some(w) = self.wiring.as_ref() else {
            return -1;
        };
        match w.game_state() {
            Some(game) => game.borrow().soldiers.len() as i64,
            None => -1,
        }
    }

    /// 取回 `TurnManager`，读某个玩家的士兵数（`player_id` 不存在 ⇒ `-1`）。
    ///
    /// 原版的 `TurnManager` 是 `player_id → {Vec<soldier_id>, 游标}`（`0x463f27`–`0x463fa6`
    /// 那张表，`docs/98` §1.1）；这个口子读的就是那张表的长度。
    #[func]
    fn container_turn_manager_soldiers(&mut self, player_id: i64) -> i64 {
        self.ensure_scene_graph();
        let Some(w) = self.wiring.as_ref() else {
            return -1;
        };
        let Some(turns) = w.turn_manager() else {
            return -1;
        };
        let turns = turns.borrow();
        match turns.player(player_id) {
            Some(entry) => entry.turns.len() as i64,
            None => -1,
        }
    }

    /// 取回 `PlayerManager`，读 `player_id → connection_id` 那张表的行数。
    #[func]
    fn container_player_manager_players(&mut self) -> i64 {
        self.ensure_scene_graph();
        let Some(w) = self.wiring.as_ref() else {
            return -1;
        };
        match w.player_manager() {
            Some(players) => players.borrow().len() as i64,
            None => -1,
        }
    }

    /// 某个槽位的**复原档位**（四档，按序判断、第一个命中者为准；`docs/16` §47.1）：
    /// `"wired"`（容器 + 事件流 + 最小交互断言）/ `"layout"`（原版字段布局逐字段解出）/
    /// `"assembled"`（容器里是我方复原类型）/ `"stub"`（我方形状，布局未知）。
    /// 另有 `"placeholder"`（原版有这一型，本层没装）/ `"unknown"`（原版 25 型里也没有这名字）。
    ///
    /// 判据是**运行时的**（[`super::root_wiring::RootWiring::has`]），不是静态表：
    /// 装失败就掉回 `placeholder`。
    #[func]
    fn module_slot_status(&mut self, type_name: GString) -> GString {
        self.ensure_scene_graph();
        let name = type_name.to_string();
        if let Some(which) = Wired::from_type_name(&name) {
            let assembled = self.wiring.as_ref().map(|w| w.has(which)).unwrap_or(false);
            return GString::from(if assembled {
                which.status()
            } else {
                root_wiring::SLOT_PLACEHOLDER
            });
        }
        if root_wiring::is_module_type_name(&name) {
            GString::from(root_wiring::SLOT_PLACEHOLDER)
        } else {
            GString::from(root_wiring::SLOT_UNKNOWN)
        }
    }

    /// **按类型取回**（`get_module::<M>()` 真的 downcast 成功吗）。
    ///
    /// 比 [`GraphwarRootNode::container_has_module`] 强一档：那一支只查
    /// `HashMap::contains_key(&TypeId::of::<M>())`，这一支走完整的取回路径
    /// （`hash_one` → 探测 → `Any::type_id` 比对 → `Rc::downcast`，`docs/02` §22.5）。
    #[func]
    fn container_module_live(&mut self, type_name: GString) -> bool {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.retrievable_name(&type_name.to_string()),
            None => false,
        }
    }

    /// 22 个槽里"能取回活对象"的个数（[`GraphwarRootNode::container_module_live`] 的计数版）。
    #[func]
    fn container_live_count(&mut self) -> i64 {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => Wired::ALL.into_iter().filter(|x| w.retrievable(*x)).count() as i64,
            None => -1,
        }
    }

    /// 22 个槽里"我方有真复原类型"的个数（`docs/16` §41.2 的 17）。
    #[func]
    fn container_real_manager_count(&mut self) -> i64 {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.real_managers().len() as i64,
            None => -1,
        }
    }

    /// 22 个槽里**容器类型来源为 `SlotKind::Stub`** 的个数（= **5**，`docs/16` §41.2 那批）。
    ///
    /// ⚠️ 这**不是**档位 `stub` 的个数（那个现在是 **1**，只剩 `GraphwarPlanePosManager`）：
    /// 本函数数的是"装配表里标为占位"的槽，档位则是"按证据判到第几档"。两者口径不同，
    /// 2026-11-08 任务 28 起在 `assemble_check.gd` 里已分别断言（四档计数 + `SlotKind` 计数）。
    /// 函数名保留是为了不破坏既有断言；改名留给下一轮与 gd 断言同批做。
    #[func]
    fn container_stub_manager_count(&mut self) -> i64 {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.stub_managers().len() as i64,
            None => -1,
        }
    }

    /// 这一槽的原版 `size_of::<M>()`（`docs/02` §22.3；**两架构相同**，vtable size 字 − 8）。
    ///
    /// 22 个数按注册序连起来就是 §41.2 那串
    /// `80,120,72,80,80,32,304,40,80,40,80,88,40,80,40,176,80,80,56,80,128,80`（+8 之后）。
    /// 未知名字 ⇒ `-1`。
    #[func]
    fn manager_slot_size(&mut self, type_name: GString) -> i64 {
        match Wired::from_type_name(&type_name.to_string()) {
            Some(w) => w.slot().size as i64,
            None => -1,
        }
    }

    /// 这一槽装的是**我方的哪个类型**（`graphwar_turn_manager::TurnManager` 之类）；
    /// 空占位回 `"—"`；未知名字回空串。
    #[func]
    fn manager_impl_type(&mut self, type_name: GString) -> GString {
        match Wired::from_type_name(&type_name.to_string()) {
            Some(w) => GString::from(w.slot().our_type),
            None => GString::new(),
        }
    }

    /// 本层装了几个 manager（= `Wired::COUNT`，**22** = 原版 `GraphwarGameClient::new`
    /// 的 `add_module` 条数；其中 17 个真类型 + 5 个空占位）。
    #[func]
    fn assembled_manager_total(&self) -> i64 {
        Wired::COUNT as i64
    }

    /// `_ready` 被引擎调到几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `_process` 被引擎调到几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// 累计 `set_visible` 写成功多少次。
    #[func]
    fn visibility_write_count(&self) -> i64 {
        self.visibility_writes
    }

    /// 最近一次 `show_screen` 的屏名。
    #[func]
    fn last_queried_screen(&self) -> GString {
        self.last_queried.clone()
    }

    // ------------------------------------------------------------------
    // ⚠️ 下面两个是**我方自造的观测口**（2026-11 任务 19），原版**没有**这两个方法。
    //
    // 为什么必须加：`RootWiring` 是 Rust 结构体，GDScript 只能调 `#[func]`；
    // 在它之前，headless 侧**没有任何办法**触发一次 manager 交互（只能"数个数"）。
    // 它们只做一件事：把 `RootWiring::interaction_selftest`（5 个 manager 各一次
    // 最小交互，写 → 读回）**原样**交给验收脚本，**自己不返回任何常量**。
    //
    // 口径（`docs/16-client.md` §43）：
    // * **只读**：不改变 22 个槽的注册、不碰 `Match` / `TurnManager` / `PlayerManager`
    //   （那三个是别的断言在读的）；动的只有这 5 个 manager 自己表里的固定 id
    //   （`9001..=9003`）与固定坐标，且每次调用末尾把发件箱取走；
    // * **幂等**：连调两次返回**同一串** `名字=值`（验收脚本就是这么比的）；
    // * **不是复原成果**：它证明的是"装配好的对象能交互"，不证明原版行为。
    // ------------------------------------------------------------------

    /// 5 个 manager 的"一次最小交互"结果，每条一个 `"名字=值"`（27 条）。
    ///
    /// 名字与期望值见 `docs/16-client.md` §43 的表；
    /// Rust 侧的同一条断言是 `root_wiring.rs::tests::
    /// one_minimal_interaction_per_manager_reads_back_the_written_value`。
    #[func]
    fn manager_probe_selftest(&mut self) -> PackedStringArray {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => {
                let mut out = PackedStringArray::new();
                for line in w.interaction_selftest_strings() {
                    out.push(&GString::from(line.as_str()));
                }
                out
            }
            None => PackedStringArray::new(),
        }
    }

    /// 单条查询：[`GraphwarRootNode::manager_probe_selftest`] 里那个 `"名字=值"` 的**值**。
    ///
    /// 未知名字 ⇒ 空串（不是 `"0"`，也不是 panic）。
    #[func]
    fn manager_probe(&mut self, name: GString) -> GString {
        self.ensure_scene_graph();
        let wanted = name.to_string();
        match self.wiring.as_ref() {
            Some(w) => match w
                .interaction_selftest()
                .into_iter()
                .find(|(k, _)| *k == wanted.as_str())
            {
                Some((_, v)) => GString::from(v.as_str()),
                None => GString::new(),
            },
            None => GString::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphwar_root_node::{screen_swap, ScreenSwap, SPECIAL_MAP_KEYS};

    /// 三个 holder 名字就是 `READY_NODES`（不是重新编的）。
    #[test]
    fn holder_names_are_the_ready_nodes() {
        assert_eq!(HOLDER_NAMES.len(), 3);
        assert_eq!(HOLDER_NAMES, ["ScreensControl", "IAP_holder", "Ad_holder"]);
        for (name, node) in HOLDER_NAMES.iter().zip(READY_NODES.iter()) {
            assert_eq!(*name, node.name);
            assert_eq!(name.len(), node.len as usize);
            // holder 不是屏
            assert!(Screen::from_name(name).is_err());
        }
    }

    /// 屏表条目数 = 15，且每个名字都能往返。
    #[test]
    fn screen_table_is_fifteen_round_trip_names() {
        assert_eq!(ALL_SCREENS.len(), 15);
        for s in ALL_SCREENS {
            assert_eq!(Screen::from_name(s.name()), Ok(s));
        }
    }

    /// 切屏决策表：本类转发过去的就是这条式子（用模块的纯函数对一遍）。
    #[test]
    fn forwarding_agrees_with_the_module_decision_table() {
        // 命中非 PlayScreen ⇒ Show（可见性 true）
        assert_eq!(screen_swap("Campaign", "Campaign", false, false), ScreenSwap::Show);
        assert_eq!(
            screen_swap("Campaign", "Campaign", false, false).visible(),
            Some(true)
        );
        // PlayScreen 命中 + ads_slot + 可见 ⇒ 先 destroy_ad_view 再显示
        let hit = screen_swap("PlayScreen", "PlayScreen", true, true);
        assert_eq!(hit, ScreenSwap::ShowAndDestroyAdView);
        assert_eq!(hit.deferred_method(), Some("destroy_ad_view"));
        // PlayScreen 未命中 + ads_slot + 可见 ⇒ 先 create_ad_view 再隐藏
        let miss = screen_swap("PlayScreen", "Campaign", true, true);
        assert_eq!(miss, ScreenSwap::HideAndCreateAdView);
        assert_eq!(miss.deferred_method(), Some("create_ad_view"));
        // background 未命中 ⇒ 不碰
        assert_eq!(screen_swap(SPECIAL_MAP_KEYS[0], "Campaign", true, true), ScreenSwap::Leave);
        assert_eq!(ScreenSwap::Leave.visible(), None);
    }

    /// 管理器计数：25 个槽、22 个注册点（两个数不是一回事）。
    #[test]
    fn manager_counts_are_two_different_numbers() {
        assert_eq!(MODULES.len(), 25);
        assert_eq!(MANAGER_REGISTRATION_COUNT, 22);
        assert!(MANAGER_REGISTRATION_COUNT <= MODULES.len());
        // 表里的名字都是原版类名（首字母大写、带 Graphwar 前缀的占多数）
        let mut names: Vec<&str> = MODULES.iter().map(|m| m.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), MODULES.len(), "模块名不许重复（节点名要唯一）");
        assert!(names.iter().all(|n| !n.is_empty()));
    }

    /// `Ad_holder` 的目标类名与模块常量一致。
    #[test]
    fn ad_holder_class_is_the_module_class() {
        assert_eq!(AD_HOLDER_CLASS, crate::graphwar_ad_switch::CLASS);
        assert_eq!(AD_HOLDER_CLASS, "GraphwarAdSwitch");
    }
}
