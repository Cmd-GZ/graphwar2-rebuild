//! **不是原版的东西**：把复原的 [`crate::graphwar_lan_vbox`] 挂成**真 Godot 节点类**的绑定外壳。
//!
//! # 为什么会有这一份
//!
//! 原版 `GraphwarLanVBox` 是大厅里的**房间列表容器**，基类 **`VBoxContainer`**
//! （档位：**观测**，arm64 `.symtab` 的 `<GraphwarLanVBox as godot_core::gen::classes::v_box_container::re_export::IVBoxContainer>::process`
//! 把基类名写在了符号里，见 `docs/09-network.md` §10）。`src/graphwar_lan_vbox.rs` 已经把
//! 数据模型、选源、行的增删判据、Join 通道、**「没有排序」**都复原成纯逻辑（一行引擎调用都没有），
//! 本文件只做两件事：
//!
//! 1. 让 `process` / `init` 真的被引擎调到（`IVBoxContainer`）；
//! 2. 让"源表里的一行 → 一个子节点"这条**投影**真的发生 —— 顺序**原样照抄**源表顺序，
//!    因为原版 `process`（14524 B）里 **0 处 `sort*`**（`docs/09` §10「排序：没有排序」）。
//!
//! # 换基类：本文件与 [`super::entity_drawer_class`]（`Control`）的唯一结构性差别
//!
//! | 项 | `Control` 基类 | `VBoxContainer` 基类（本文件） |
//! |---|---|---|
//! | 虚方法 trait | `impl IControl` | **`impl IVBoxContainer`** |
//! | `Base<…>` 的类型 | `Base<Control>` | **`Base<VBoxContainer>`** |
//! | `init` 签名 | `fn init(base: Base<Control>) -> Self` | **`fn init(base: Base<VBoxContainer>) -> Self`** |
//! | 可覆写的虚方法集 | `ready` / `process` / `draw` / … | **同一套**（`IVBoxContainer` 上溯 `IBoxContainer` → `IContainer` → `IControl`，`VBoxContainer` 自己不新增虚方法） |
//! | `#[func]` / `#[export]` | `#[godot_api] impl 类型名 { … }` | **一个字都不用改**（同一套宏、同一个块） |
//!
//! ⇒ **换基类只改"虚方法块"那 3 处**（trait 名、`Base<T>` 类型、`init` 参数类型）；
//! `#[func]` / 属性机制与基类无关。本轮三份文件是同一模板的三个实例，
//! 实测结果写在 `docs/16-client.md` 新增的那一节里。
//!
//! ⚠️ 一个**实测到**的差别（不是错误）：`VBoxContainer` 会**自动纵向排列**子节点，
//! 而这正是"显示顺序 = `add_child` 顺序"的机制来源；纯 `Node` 没有这个行为。
//! 所以验收脚本里"3 个子项 y 递增"这条断言，只在 `VBoxContainer` 基类下成立。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 行的来源 | 每帧读 `GraphwarRootNode` 的模块容器（`+0x1668`）里 LAN 广播器 / broker 的 `HashMap<SocketAddr, GameInfo>` | **由 GDScript 传进来**（`update_game_list`）—— 容器与两个 handler 都还没接到节点层 | 观测（`docs/09` §10）/ 本文件是**我方选择** |
//! | 行节点 | `load("res://game_room_entry.tscn")` + `instantiate` | **手工造** `Control` + `Label(RoomName)` + `Button(JoinButton)`；场景文件不在 `client/` 里 | 节点名是**观测**（`ROW_NODE_NAMES`）；造法是**我方选择** |
//! | 7 个字段的投影 | 房名 / 人数（`"  N  "`）/ `locked` / 4 个模式图标 | 只投**房名**、**人数**、**`locked` → `JoinButton.disabled`**；4 个模式图标需要贴图表（本切片没有） | 观测（前 3 项）/ 未做（后 4 项） |
//! | 选源 Local/Global | `self+0x68` 的 bool（**谁写它未知**） | `#[export] show_global` 属性（GDScript 写） | **推断**（`docs/09` §10 未知 ③） |
//! | 可见性闸门 | `is_visible_in_tree()`，假就整帧不刷 | 同（树外按"可见"处理，见 [`GraphwarLanVBox::update_game_list`]） | 观测 / 树外这一条是我方约定 |
//! | Join | 投 `Sender<SocketAddr>`，**下一帧** `process` 排空才 `start_client_to_remote_server` | 投同一个通道；排空后**只记账**（没有 `GraphwarBackend`） | 观测（通道 + 时序）/ 未接后端 |
//! | 排序 | **没有**（源表 `HashMap` 迭代序 → `add_child` 序） | 没有（源表序 = 调用方给的序） | 观测（0 处 `sort*`） |
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
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! GraphwarLanVBox : VBoxContainer
//!   @export show_global : bool     # 原版 self+0x68（推断：属性可写）
//!
//!   update_game_list(addrs, room_names, player_counts, locked) -> int
//!       # addrs = ["127.0.0.1:35245", …]，顺序**就是**源表顺序（不排序）
//!   row_count() -> int
//!   row_names() -> PackedStringArray      # 子节点名，按子节点顺序（= add_child 顺序）
//!   row_texts() -> PackedStringArray      # 每行 RoomName 标签的文本
//!   set_row_locked(addr, locked) -> bool  # 只改 JoinButton.disabled（不重建行）
//!   source_name() -> String               # "Lan" / "Broker"
//!   press_join(addr) -> bool              # 投进通道，**不**立刻连
//!   drain_join_requests() -> PackedStringArray
//!   pending_join_request_count() -> int
//!   process_call_count() / drained_total() -> int   # 观测点
//! ```

use std::collections::BTreeMap;
use std::net::SocketAddr;

use godot::classes::{Button, Control, IVBoxContainer, Label, VBoxContainer};
use godot::prelude::*;

use crate::graphwar_lan_vbox::{
    self, LanVBox, RowAction, PLAYER_COUNT_PAD, ROW_NODE_NAMES,
};

/// 行根节点名的前缀。**不是原版常量**：原版的行是场景实例（根节点名来自 `.tscn`），
/// 这里手工造节点，需要一个能和源表地址一一对应的名字（见 [`node_name_for`]）。
pub const ROW_NODE_PREFIX: &str = "RoomEntry";

/// 行根节点的最小高度（像素）。**我方选择**：`VBoxContainer` 按子节点的最小尺寸排，
/// 高度为 0 的行挤在一起就分不出顺序了。
pub const ROW_MIN_HEIGHT: f32 = 24.0;

/// `ROW_NODE_NAMES` 里本切片用到的两个名字（原版行场景里的节点名，**观测**）。
///
/// * `RoomName`（8 字节）= 房名标签；
/// * `JoinButton`（10 字节）= Join 按钮（原版只 `set_disabled`，处理器挂在它上面）。
pub const ROW_NAME_LABEL: &str = ROW_NODE_NAMES[1].0;
/// 见 [`ROW_NAME_LABEL`]。
pub const ROW_JOIN_BUTTON: &str = ROW_NODE_NAMES[0].0;

/// 一行持有的两个子节点 + 它对应的地址。
struct RowNodes {
    /// 行根节点（`Control`），挂在本控件下。
    root: Gd<Control>,
    /// 房名标签（`RoomName`）。
    name_label: Gd<Label>,
    /// Join 按钮（`JoinButton`）。
    join_button: Gd<Button>,
}

/// 源表里的一个地址 → 一个行节点。
///
/// 键是 `SocketAddr`（原版 `self.rows` 的键，**观测**：`ROWS_OFFSET` 那张
/// `HashMap<SocketAddr, Gd<行>>`，桶步长 48 = 32 + 16）；值是行节点。
type Rows = BTreeMap<SocketAddr, RowNodes>;

/// 把地址变成合法的 Godot 节点名（Godot 不允许节点名里出现 `.` `:` 等字符）。
///
/// 例：`127.0.0.1:35245` → `RoomEntry_127_0_0_1_35245`。
/// 一一对应 ⇒ 验收脚本可以直接拿节点名比"记录顺序"。
fn node_name_for(addr: &SocketAddr) -> String {
    let text = addr.to_string();
    let sanitized: String = text
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{ROW_NODE_PREFIX}_{sanitized}")
}

/// 人数标签的文本：`"  N  "`（前后各 [`PLAYER_COUNT_PAD`] 个空格）。
///
/// 档位：**观测**（原版 `0x4517bc` 往文本尾部写 `0x2020` = 两个空格；真机 dump
/// 的字段值就是 `"  0  "`，`docs/16-client.md` §160）。
///
/// ⚠️ 这里**没有**构造 [`graphwar_lan_vbox::RoomEntry`]：那个结构体有 7 个字段，
/// 其中 4 个是模式字符串，本切片不画模式图标、拿不到真值。所以只搬了补空格这一条规则，
/// 并用单测 `player_count_text_matches_the_module` 钉住它与 `RoomEntry::player_count_text`
/// **逐字节一致**（改了任何一边，那条测试就红）。
fn player_count_text(count: i32) -> String {
    let pad = " ".repeat(PLAYER_COUNT_PAD);
    format!("{pad}{count}{pad}")
}

/// 把 `"ip:port"` 解析成 [`SocketAddr`]（失败就跳过这一条，不 panic）。
fn parse_addr(text: &str) -> Option<SocketAddr> {
    text.trim().parse::<SocketAddr>().ok()
}

/// `GraphwarLanVBox`：大厅房间列表容器。
///
/// 行的**显示顺序 = 源表顺序**（不排序）：先出现的先 `add_child`，之后只刷新不重建。
#[derive(GodotClass)]
#[class(base = VBoxContainer)]
pub struct GraphwarLanVBox {
    base: Base<VBoxContainer>,

    /// 原版 `self+0x68`：`false` = Local（LAN 广播器的表），`true` = Global（broker 的表）。
    ///
    /// 档位：偏移/默认值 = **观测**（`init` `strb wzr,[x20,#0x68]`）；「它能被 GDScript 写」
    /// 是**推断**（`docs/09` §10 未知 ③：谁写这个字节至今没有逐指令对上）。
    #[export]
    show_global: bool,

    /// 纯逻辑模型（Join 通道 / 行表 / 选源 / 可见性闸门）。算术本体在 `src/graphwar_lan_vbox.rs`。
    model: LanVBox,

    /// 地址 → 行节点。**顺序不靠这张表**（`BTreeMap` 会按地址排），靠子节点顺序。
    rows: Rows,

    /// `_process` 被调到几次（观测点）。
    process_calls: i64,
    /// `_ready` 被调到几次（观测点）。
    ready_calls: i64,
    /// 累计从 Join 通道排空了多少个请求（观测点）。
    drained_total: i64,
}

impl GraphwarLanVBox {
    /// 把 `#[export] show_global` 灌进模型（属性是唯一真源，模型只读它）。
    fn sync_model_from_property(&mut self) {
        self.model.set_show_global(self.show_global);
    }

    /// 把引擎的可见性灌进模型的闸门字段。
    ///
    /// 原版 `process` 第 ② 步只有一句 `is_visible_in_tree()`（arm64 `0x450dd8`）。
    /// 这里多一条**我方约定**：**树外当可见**。理由 = 复刻 `entity_drawer_class` 那批实测到的
    /// 坑（`docs/16` §37.4 第 3 条）：`--script` 模式下节点先建、后进树，若树外一律判"不可见"，
    /// 验收脚本第一帧一行都建不出来，测的就不是投影逻辑了。
    fn sync_model_visibility_from_engine(&mut self) {
        let base = self.base();
        let gate = base.is_visible_in_tree() || !base.is_inside_tree();
        self.model.set_visible(gate);
    }

    /// 造一行：`Control` 根 + `RoomName` 标签 + `JoinButton` 按钮。
    ///
    /// 三个节点名都是**原版行场景里的名字**（[`ROW_NAME_LABEL`] / [`ROW_JOIN_BUTTON`]）；
    /// 行根节点名是我方约定（原版来自 `.tscn`），见 [`node_name_for`]。
    fn new_row(&mut self, addr: &SocketAddr, room_name: &str, count: i32, locked: bool) -> RowNodes {
        let mut root = Control::new_alloc();
        root.set_name(&StringName::from(node_name_for(addr).as_str()));
        root.set_custom_minimum_size(Vector2::new(0.0, ROW_MIN_HEIGHT));

        let mut label = Label::new_alloc();
        label.set_name(&StringName::from(ROW_NAME_LABEL));
        label.set_text(&GString::from(room_name));
        label.set_position(Vector2::new(0.0, 0.0));
        root.add_child(&label.clone().upcast::<godot::classes::Node>());

        let mut button = Button::new_alloc();
        button.set_name(&StringName::from(ROW_JOIN_BUTTON));
        // 原版：`BaseButton::set_disabled(&join_button, w1)`，`w1` 来自该行记录的一个 bool
        // （`0x4515a4` → `0x4515ac`）；`join_enabled() == !locked` 是**推断**
        // （9 字段里唯一的布尔）。
        button.set_disabled(locked);
        root.add_child(&button.clone().upcast::<godot::classes::Node>());

        // 行根节点挂到本控件下：`add_child` 的先后 = `VBoxContainer` 里的显示顺序。
        self.base_mut().add_child(&root.clone().upcast::<godot::classes::Node>());

        // 人数文本投影到标签上（`RoomName` 只放房名；人数放 `JoinButton` 的 tooltip，
        // 这样一行只有两个子节点就能同时看到三项里的两项 —— 我方化简）。
        label.set_tooltip_text(&GString::from(player_count_text(count).as_str()));

        RowNodes {
            root,
            name_label: label,
            join_button: button,
        }
    }
}

#[godot_api]
impl IVBoxContainer for GraphwarLanVBox {
    fn init(base: Base<VBoxContainer>) -> Self {
        Self {
            base,
            show_global: graphwar_lan_vbox::DEFAULT_SHOW_GLOBAL,
            model: LanVBox::new(),
            rows: BTreeMap::new(),
            process_calls: 0,
            ready_calls: 0,
            drained_total: 0,
        }
    }

    /// `_ready`。原版**没有** `ready`（`docs/09` §10：它不订阅任何 `Event`，74 个事件一个都不在这里）
    /// —— 这里只加一个计数观测点，**不做任何初始化**。
    fn ready(&mut self) {
        self.ready_calls += 1;
    }

    /// `_process`：原版做五步（排空 Join 通道 / 可见性闸门 / 取根节点 / 逐行投影 / 清理过期行）。
    ///
    /// 本切片只做**第 ① 步**：排空 Join 通道并记账（没有 `GraphwarBackend`，
    /// 不能真的 `start_client_to_remote_server`）。②–⑤ 需要源表，
    /// 由 GDScript 用 [`GraphwarLanVBox::update_game_list`] 显式驱动。
    ///
    /// 不写 `set_process(true)`：Godot 4.7 对实现了该虚方法的扩展类自动开
    /// （`docs/16` §37.4 第 5 条，本轮三份类**再次**实测到同一结论）。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;
        let drained = self.model.drain_join_requests();
        self.drained_total += drained.len() as i64;
    }
}

#[godot_api]
impl GraphwarLanVBox {
    /// 把一张房间表投影成行节点（原版 `process` 的 ④⑤ 两步）。
    ///
    /// # 参数（四张平行数组，下标对应同一行）
    ///
    /// * `addrs` —— `"ip:port"`；**顺序就是源表顺序**，本方法**不排序**；
    /// * `room_names` —— 房名（缺省用地址本身）；
    /// * `player_counts` —— 人数，投影成 `"  N  "`（[`player_count_text`]）；
    /// * `locked` —— 非 0 = 锁房 ⇒ `JoinButton.disabled = true`。
    ///
    /// # 返回
    ///
    /// 投影完之后的行数。
    ///
    /// # 判据（全部来自 `src/graphwar_lan_vbox.rs`，本方法不复述算术）
    ///
    /// * 新地址 → `Create`（建节点）；已有地址 → `Refresh`（只刷字段，**不重建**）；
    /// * 源表里已经没有的地址 → `queue_free` 并丢掉句柄；
    /// * 可见性闸门为假 ⇒ 整帧什么都不做（原版第 ② 步）。
    #[func]
    fn update_game_list(
        &mut self,
        addrs: PackedStringArray,
        room_names: PackedStringArray,
        player_counts: PackedInt32Array,
        locked: PackedByteArray,
    ) -> i64 {
        self.sync_model_from_property();
        self.sync_model_visibility_from_engine();

        let addrs = addrs.as_slice();
        let names = room_names.as_slice();
        let counts = player_counts.as_slice();
        let locked = locked.as_slice();

        let order: Vec<SocketAddr> = addrs.iter().filter_map(|s| parse_addr(&s.to_string())).collect();

        // ① 计划（纯逻辑；`plan_frame` 原样保留传入顺序，不排序、不去重）
        let plan = self.model.sync_rows(&order);

        for (addr, action) in plan.order.iter().zip(plan.actions.iter()) {
            let index = addrs
                .iter()
                .position(|s| parse_addr(&s.to_string()).as_ref() == Some(addr))
                .unwrap_or(usize::MAX);
            let name = names
                .get(index)
                .map(|s| s.to_string())
                .unwrap_or_else(|| addr.to_string());
            let count = counts.get(index).copied().unwrap_or(0);
            let is_locked = locked.get(index).copied().unwrap_or(0) != 0;

            match action {
                // ② 新地址：建一次节点（原版 `load` + `instantiate` + `add_child` 各一次）
                RowAction::Create => {
                    let row = self.new_row(addr, &name, count, is_locked);
                    self.rows.insert(*addr, row);
                }
                // ③ 已有地址：只刷新字段（原版 `rustc_entry` 命中那条路）
                RowAction::Refresh => {
                    if let Some(row) = self.rows.get_mut(addr) {
                        row.name_label.set_text(&GString::from(name.as_str()));
                        row.name_label.set_tooltip_text(&GString::from(player_count_text(count).as_str()));
                        row.join_button.set_disabled(is_locked);
                    }
                }
            }
        }

        // ④ 源表里已经没有的地址：`queue_free` 后丢掉句柄（句柄与节点必须同生共死，
        //    否则下一次 `upcast_ref` 会 1337 并卡死进程，`docs/16` §37.4 第 2 条）
        for addr in plan.stale.iter() {
            if let Some(mut row) = self.rows.remove(addr) {
                row.root.queue_free();
            }
        }

        self.rows.len() as i64
    }

    /// 当前行数。
    #[func]
    fn row_count(&self) -> i64 {
        self.rows.len() as i64
    }

    /// 子节点名，**按子节点顺序**（= `add_child` 顺序 = 源表顺序）。
    ///
    /// 这是"顺序 = 插入顺序（无排序）"这条验收的观测口：名字由 [`node_name_for`] 从地址
    /// 一一映射出来。
    #[func]
    fn row_names(&self) -> PackedStringArray {
        let base = self.base();
        let count = base.get_child_count();
        let mut out = PackedStringArray::new();
        for i in 0..count {
            if let Some(child) = base.get_child(i) {
                out.push(&GString::from(child.get_name().to_string().as_str()));
            }
        }
        out
    }

    /// 每行 `RoomName` 标签的文本，**按子节点顺序**。
    #[func]
    fn row_texts(&self) -> PackedStringArray {
        let base = self.base();
        let count = base.get_child_count();
        let mut out = PackedStringArray::new();
        for i in 0..count {
            let Some(child) = base.get_child(i) else {
                continue;
            };
            let text = child
                .try_get_node_as::<Label>(ROW_NAME_LABEL)
                .map(|l| l.get_text())
                .unwrap_or_default();
            out.push(&text);
        }
        out
    }

    /// 某行的 `JoinButton.disabled`，按地址查。行不存在返回 `false`。
    #[func]
    fn row_join_disabled(&self, addr: GString) -> bool {
        parse_addr(&addr.to_string())
            .and_then(|a| self.rows.get(&a))
            .map(|row| row.join_button.is_disabled())
            .unwrap_or(false)
    }

    /// 只改某一行的 `locked`（`JoinButton.disabled`），**不重建行**。
    #[func]
    fn set_row_locked(&mut self, addr: GString, locked: bool) -> bool {
        let Some(parsed) = parse_addr(&addr.to_string()) else {
            return false;
        };
        let Some(row) = self.rows.get_mut(&parsed) else {
            return false;
        };
        row.join_button.set_disabled(locked);
        true
    }

    /// 这一帧读哪张表：`"Lan"`（Local）或 `"Broker"`（Global）。
    ///
    /// 取 `&mut self` 是因为要先把 `#[export] show_global`（属性的唯一真源）灌进模型：
    /// 属性是引擎侧写的，Rust 侧不会自动收到通知（没有 setter 钩子 —— 那要 `#[setter]`，
    /// 原版没有证据表明它有）。
    #[func]
    fn source_name(&mut self) -> GString {
        self.sync_model_from_property();
        GString::from(match self.model.source() {
            graphwar_lan_vbox::Source::Lan => "Lan",
            graphwar_lan_vbox::Source::Broker => "Broker",
        })
    }

    /// 点一下 Join：把地址投进本控件自己的通道（**不**直接连）。
    ///
    /// 档位：**观测**（原版闭包 `0x4491d8` 调 `mpmc::Sender::send`，实参是 `SocketAddr`）。
    /// 原版按下时会先重查两张表（`0x449174`/`0x449188`）—— 那需要模块容器，本切片没有。
    #[func]
    fn press_join(&mut self, addr: GString) -> bool {
        let Some(parsed) = parse_addr(&addr.to_string()) else {
            return false;
        };
        self.model.press_join(parsed);
        true
    }

    /// `process` 第 ① 步：排空通道，返回这一帧要连的地址（顺序 = 投递顺序）。
    #[func]
    fn drain_join_requests(&mut self) -> PackedStringArray {
        let drained = self.model.drain_join_requests();
        self.drained_total += drained.len() as i64;
        let mut out = PackedStringArray::new();
        for a in drained {
            out.push(&GString::from(a.to_string().as_str()));
        }
        out
    }

    /// 通道里还没被取走的请求数。
    #[func]
    fn pending_join_request_count(&self) -> i64 {
        self.model.pending_join_requests() as i64
    }

    /// `_process` 被引擎调到几次（观测点）。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// `_ready` 被引擎调到几次（观测点）。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphwar_lan_vbox::RoomEntry;

    /// 人数文本的两条式子必须逐字节一致（本类只搬了补空格规则，没有搬整个 `RoomEntry`）。
    #[test]
    fn player_count_text_matches_the_module() {
        let entry = |n: u32| RoomEntry {
            room_name: String::from("room"),
            player_count: n,
            locked: false,
            axis_mode: "EveryUnit",
            function_mode: "NormalFunction",
            turn_mode: "SequentialTurns",
            time_mode: "Timer1m",
        };
        for n in [0u32, 1, 2, 9, 10, 99] {
            assert_eq!(
                player_count_text(n as i32),
                entry(n).player_count_text(),
                "n = {n}"
            );
        }
        assert_eq!(player_count_text(0), "  0  ");
        assert_eq!(player_count_text(12), "  12  ");
        assert_eq!(PLAYER_COUNT_PAD, 2);
    }

    /// 地址 → 节点名是一一映射（验收脚本靠它比顺序）。
    #[test]
    fn node_name_is_a_bijection_on_addresses() {
        let a: SocketAddr = "127.0.0.1:35245".parse().unwrap();
        let b: SocketAddr = "127.0.0.10:35245".parse().unwrap();
        assert_eq!(node_name_for(&a), "RoomEntry_127_0_0_1_35245");
        assert_ne!(node_name_for(&a), node_name_for(&b), "不能撞名");
        // Godot 节点名不允许出现 `.` / `:` —— 生成的名字里一个都没有
        assert!(!node_name_for(&a).contains('.'));
        assert!(!node_name_for(&a).contains(':'));
        assert!(node_name_for(&a).starts_with(ROW_NODE_PREFIX));
        // 解析同上
        assert_eq!(parse_addr("127.0.0.1:35245"), Some(a));
        assert_eq!(parse_addr("  127.0.0.1:35245 "), Some(a));
        assert_eq!(parse_addr("not-an-addr"), None);
    }

    /// 本切片用到的两个行内节点名就是原版常量（不是重新编的名字）。
    #[test]
    fn row_child_names_come_from_the_module() {
        assert_eq!(ROW_NAME_LABEL, "RoomName");
        assert_eq!(ROW_JOIN_BUTTON, "JoinButton");
        assert_eq!(ROW_NAME_LABEL.len(), 8);
        assert_eq!(ROW_JOIN_BUTTON.len(), 10);
        assert!(ROW_NODE_NAMES.iter().any(|(n, _)| *n == ROW_NAME_LABEL));
        assert!(ROW_NODE_NAMES.iter().any(|(n, _)| *n == ROW_JOIN_BUTTON));
    }

    /// 模型层「不排序」的判据与本类的投影方向一致：源表顺序原样保留。
    #[test]
    fn projection_keeps_source_order_without_sorting() {
        let mut model = LanVBox::new();
        let mk = |last: u8| -> SocketAddr {
            format!("127.0.0.1:{last}").parse().unwrap()
        };
        // 故意给一个"倒"的源表序
        let order = vec![mk(9), mk(3), mk(1)];
        let plan = model.sync_rows(&order);
        assert_eq!(plan.order, order, "计划顺序必须原样保留");
        assert_eq!(
            plan.actions,
            vec![RowAction::Create, RowAction::Create, RowAction::Create]
        );
        // 再来一遍：全是 Refresh（行被复用，不重建、顺序不动）
        let plan = model.sync_rows(&order);
        assert_eq!(
            plan.actions,
            vec![RowAction::Refresh, RowAction::Refresh, RowAction::Refresh]
        );
        // 源表少两条 ⇒ 进 stale
        let plan = model.sync_rows(&[mk(3)]);
        let mut stale = plan.stale.clone();
        stale.sort_unstable();
        assert_eq!(stale, vec![mk(1), mk(9)]);
    }
}
