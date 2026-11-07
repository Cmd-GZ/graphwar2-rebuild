//! **不是原版的东西**：把复原的 [`crate::graphwar_root_node`] 挂成**真 Godot 节点类**的骨架。
//!
//! # 这一份只做「骨架」，不做「装配」
//!
//! 原版 `GraphwarRootNode` 是主场景节点（`docs/16-client.md` §15），它干三件事：
//!
//! 1. **装配**：把屏表（15 个 `Control`）、两个控制器句柄（`+0x1668` / `+0x16c8` 的
//!    `Rc<RefCell<T>>`）、贴图加载器（`+0x16a0`）、以及模块容器里的一批 manager 接起来；
//! 2. **切屏**：`0x3af850` —— 按名字让一个屏可见、其余不可见，进出口时
//!    `call_deferred("create_ad_view"|"destroy_ad_view")`；
//! 3. **状态机**：`0x3a5760`（20288 字节、339 条比较、64 个直接调用目标）+ 命令总线 `0x3b0830`。
//!
//! **本文件只做第 1 条的"形状"和第 2 条的"链路"**：
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
//! **刻意不做**（`AGENTS.md` §0 第 3 条：没证据就不猜）：
//!
//! | 没做的东西 | 为什么 |
//! |---|---|
//! | `0x3a5760`（20288 B 状态机）的主体 | 原版形状本身没解出来（可能是 `_process` + 大量内联），不猜 |
//! | `0x3b0830`（命令总线）的**全部**键 | 只解到 12 条（[`crate::graphwar_root_node::Command`]），131 KB 反编译没过完 |
//! | 25 个 manager 的**真实**装配 | 各 manager 的行为属别的模块；本文件只按 `graphwar_module_container::MODULES` 建**占位子节点**，让"根节点持有管理器子节点"这条形状可断言 |
//! | 两个 `Rc<RefCell<T>>`（`+0x1668` / `+0x16c8`） | 那两个控制器的类型与生命周期未解（`docs/16` §15.3 只给了偏移） |
//! | `GraphwarTextureLoader`（`+0x16a0`） | 模块存在（`docs/16` §34），但节点层还没有 |
//!
//! ⚠️ 占位子节点的名字**就是**原版类名（`GraphwarTurnManager` 之类），但它们只是空 `Node`
//! —— **没有脚本、没有方法、没有行为**。别把它们当成"那个 manager 已经有节点了"。
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
//! ```

use std::collections::BTreeMap;

use godot::classes::{Control, INode, Node};
use godot::prelude::*;

use crate::graphwar_module_container::{self, MODULES};
use crate::graphwar_root_node::{apply_show_screen, Screen, ScreenHost, ALL_SCREENS, READY_NODES};

use super::graphwar_ad_switch_class::GraphwarAdSwitch;

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
/// │   └── MainMenu … DisconnectedPopUp   （15 个 Control，名字 = Screen::name()）
/// ├── IAP_holder             ← 空 Node 占位（原版 ready 里 get_node 过）
/// ├── Ad_holder              ← GraphwarAdSwitch（另一个扩展类）
/// └── GraphwarModuleContainer
///     └── GraphwarBanList … （25 个空 Node，名字 = 模块类名）
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

        // ① ScreensControl + 15 个屏（原版：`get_node("ScreensControl")` 之后写屏表 `+0x1638`）
        let mut screens_control = Node::new_alloc();
        screens_control.set_name(&StringName::from(HOLDER_NAMES[0]));
        for screen in ALL_SCREENS {
            // 屏是 `Control`：原版对它们调的是「收一个 bool」的可见性成员
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

        // ④ 管理器占位：一个空 Node 容器 + 按 `MODULES` 建的空 Node。
        //    ⚠️ 名字是原版类名，但节点是空的 —— 装配（构造、注册、`Rc<RefCell<_>>`）**没做**。
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
    /// 不写 `set_process(true)`：Godot 4.7 自动开（`docs/16` §37.4 第 5 条，本轮再次实测）。
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
