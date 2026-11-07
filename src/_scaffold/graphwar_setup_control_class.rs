//! **不是原版的东西**：把复原的 [`crate::graphwar_setup_control`] 挂成**真 Godot 节点类**的绑定外壳。
//!
//! # 为什么会有这一份
//!
//! 原版 `GraphwarSetupControl` 是 `Control` 的子类（`IControl`），负责 **GameSetup 这一屏**：
//! 四格模式（轴 / 函数 / 回合 / 时限）、玩家列表、Start / Cancel、聊天。
//! `src/graphwar_setup_control.rs` 已经复原了它的**取值层**（`SetupOptions` / `TimeMode` /
//! `PlayerEntry` / `start_action`），但按约定**不碰引擎**。本文件是薄壳：
//!
//! ```text
//! src/graphwar_setup_control.rs            纯逻辑：四格取值 / 循环 / 玩家条目 / Start 判据
//! src/_scaffold/graphwar_setup_control_class.rs   ← 本文件：建替身子节点、connect、发信号
//! ```
//!
//! # 关于那 21 / 18 / 6 个数（先看清楚本文件**没有**做什么）
//!
//! 原版本模块有 **37** 个带 `Location` 锚的函数；矩阵压平跨模块后记 **21** 个"可归属"。
//! 其中 **18 个是 gdext 薄壳**（`0x4c5b50`…`0x4d96e0`，`0x60–0x140` 字节、体内无字符串），
//! 另有 **6 个**是"虚方法/属性胶水"（`0x4b88d0`…`0x4bd280`，只调 gdext 胶水）——
//! 这 **24 个的语义都未知**（`src/graphwar_setup_control.rs` 模块头第 1 节的两张表）。
//!
//! ⇒ 本文件**不声称**还原了它们。它只做两件有证据的事：
//!
//! 1. [`graphwar_setup_control::FN_SETUP_WIRING`] 里**观测到**的 3 条 `connect`；
//! 2. 四格 / 玩家条目 / Start 那几类**取值逻辑**（纯逻辑文件里已有）挂上节点。
//!
//! ⚠️ 原版**没有**给 4 个模式按钮连 `pressed`（模块头"没解出来的"第 1 条，**观测**）——
//! 所以本文件也**不连**，只提供 `#[func] cycle_*_mode()` 让宿主（或命令总线）来调。
//! 这是"照证据留白"，不是"忘了接"。
//!
//! # 本轮要验的三种机制（结论见 `docs/16-client.md` §39）
//!
//! | # | 机制 | 原版形态 | 本文件怎么用 |
//! |---|---|---|---|
//! | 1 | **自造 `#[signal]`** | 无 | `#[signal] options_changed(4×int)` / `start_requested()` / `exit_requested()` / `chat_message(text)`，用**类型安全生成器**发 |
//! | 2 | **连引擎原生信号**（**别人的**） | `ready` 里 3 条 `connect`，信号来自 **`get_node` 找到的子节点**（[`graphwar_setup_control::FN_SETUP_WIRING`]） | `find_child(name)` → `node.connect(signal, callable(self, handler))` |
//! | 3 | **外部注册闭包表** | **本模块没有**（纯逻辑文件里 0 处 `FnMut`/`Box<dyn>`） | **不伪造**（见 §39.4 的表） |
//!
//! # 与原版的差别（逐条）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 基类 / trait | `Control` + `IControl` | 同 | **观测** |
//! | 11 个 `get_node` 的实参 | `RoomNameLineEdit`/`AxisModeButton`/…/`ChatLineEdit` | 只认 3 个 **有连接的**（另外 8 个的名字记在 `STUB_NODE_NAMES` 里但**不建**） | 名字**观测**；子集是**我方选择** |
//! | 子节点从哪来 | `.tscn` 里现成的 | [`GraphwarSetupControl::build_stub_children`] 自己建（`client/` 没有这一屏的场景） | **替代** |
//! | 3 条 `connect` | `text_changed`/`visibility_changed`/`text_submitted` → 三个 `#[func]` | 同（先按名递归找节点，找不到就跳过并记账） | **观测** |
//! | 4 个模式按钮的点击 | **没连** `pressed`（走按钮 `input_text` → 命令总线的**推断**） | **也不连**；只给 `#[func] cycle_*_mode()` | 未解（`docs/16` §16 未知项） |
//! | `force_title_update` | 方法名来自 `docs/16` §38.6 的类表（`#[func]`），**写哪个节点未知** | 只把标题字符串算出来存字段；不写任何节点 | 行为**未知** |
//! | `FN_EVENT_CALLBACKS` 三个回调 | 职责**未知** | **不实现** | **未知** |
//! | 玩家列表 5 个行构件 | `0x4d3180` 重建 + 4 个按钮 | 只做**数据层**（`PlayerEntry` 的增删改），不建行节点 | 数据**已复原**；视图**未做** |
//!
//! # 怎么打开 / 契约
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/signal_classes_check.gd
//! ```
//!
//! ```text
//! GraphwarSetupControl : Control
//!   signal options_changed(axis: int, function: int, turns: int, time: int)
//!   signal start_requested()
//!   signal exit_requested()
//!   signal chat_message(text: String)
//!
//!   build_stub_children() -> int        # 建 3 个替身子节点，返回建了几个
//!   wired_count() -> int                # ready 里真连上了几条（期望 3）
//!   on_text_change(name) / on_visibility_change() / on_chat_submitted(text) / force_title_update()
//!   cycle_axis_mode() / cycle_function_mode() / cycle_turns_mode() / cycle_time_mode() -> int
//!   axis_mode() / function_mode() / turns_mode() / time_mode() -> int
//!   options_names() -> String           # 四格的线上名字（`|` 分隔）
//!   options_icons() -> String           # 四格的图标路径（`|` 分隔）
//!   timer_label() -> String             # 无限档返回 "inf"
//!   has_turn_clock() -> bool
//!   room_name() / title_text() / chat_log() -> String
//!   add_human(name, team, soldiers) / add_bot(level, name, team, soldiers) -> int
//!   add_soldier(i) / remove_soldier(i) / change_team(i) -> bool
//!   entry_count() -> int
//!   entries_text() -> String            # `name:team:soldiers[:bot]`，`|` 分隔
//!   start() -> bool                     # 真开始返回 true；静默无操作返回 false（原版是纯静默）
//!   exit() -> bool
//!   ready_call_count() / text_change_count() / visibility_change_count() -> int
//!   chat_submitted_count() / title_update_count() / options_changed_count() -> int
//! ```
//!
//! # 三条要注意的
//!
//! 1. **信号来自子节点**（与前两个类不同）：先 `find_child` 再 `connect`，
//!    找不到就**不 panic**（原版 `get_node` 找不到会报错，但那是场景缺节点的错）。
//! 2. **`set_text` 不会发 `text_changed`**（Godot 4 只在用户编辑时发）⇒ 验收脚本用
//!    `emit_signal` 充当引擎那一侧；这不影响"信号真的连上了"这条结论。
//! 3. **`SetupAction::SilentNoOp` 是原版**：没对手时 Start **什么都不做、也不提示**
//!    （`docs/16` §2.1.5 观测）。本类的 `start()` 就返回 `false` 且**不**发信号。

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::{Control, IControl, LineEdit};
use godot::obj::WithUserSignals;
use godot::prelude::*;

use crate::graphwar_level_manager::{AxisMode, FunctionMode, TurnMode};
use crate::graphwar_setup_control::{
    self as setup, PlayerEntry, SetupAction, SetupOptions, Team, TimeMode, ROOM_DEFAULT_OPTIONS,
};

/// 原版 `FN_SETUP_WIRING` 里 `get_node` 的 11 个名字（`0x19ea20`…`0x1c39fe`）。
///
/// 本文件只**建**其中 3 个有连接的（见 [`STUB_NODE_NAMES`]）；另外 8 个名字留在这里
/// 是为了让"我们建了哪几个"可核对，不是"原版只有 3 个"。
pub const ALL_GET_NODE_NAMES: [&str; 11] = [
    "RoomNameLineEdit",
    "AxisModeButton",
    "FunctionModeButton",
    "TurnModeButton",
    "TurnTimeButton",
    "LockButton",
    "ChatButtonSetup",
    "ChatButtonPlay",
    "ChatPopUp",
    "ChatBox",
    "ChatLineEdit",
];

/// 本文件真的建出来的替身节点（**我方选择**：只建有连接的那几个）。
pub const STUB_NODE_NAMES: [&str; 3] = ["RoomNameLineEdit", "ChatPopUp", "ChatLineEdit"];

/// GameSetup 这一屏的控制器。
///
/// 值全在 [`SetupOptions`] / `Vec<PlayerEntry>` 里（纯逻辑）；本结构只做"信号 → 纯逻辑 → 信号"。
pub struct GraphwarSetupControlStateMarker;

/// GameSetup 这一屏的控制器（`Control` 子类）。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct GraphwarSetupControl {
    base: Base<Control>,

    /// 四格模式（`GameInfo` 的 4 个字段）。
    options: SetupOptions,

    /// 玩家条目（原版由 `FN_TEAM_LIST_REBUILD` `0x4d3180` 重建）。
    entries: Vec<PlayerEntry>,

    /// 房间名（`RoomNameLineEdit.text_changed` 来的）。
    room_name: GString,

    /// 标题字符串（`force_title_update` 的产物；原版写哪个节点**未知**）。
    title: GString,

    /// 聊天记录（[`ChatMessage`] 的落点）。
    chat_log: Vec<String>,

    /// `_ready` 里真连上了几条信号（期望 = 3）。
    wired: i64,

    /// 没找到 / 连不上的节点名（记账，不 panic）。
    missing: Rc<RefCell<Vec<String>>>,

    ready_calls: i64,
    text_change_calls: i64,
    visibility_change_calls: i64,
    chat_submitted_calls: i64,
    title_update_calls: i64,
    options_changed_calls: i64,
}

/// 一条聊天消息（`chat_message` 信号的载荷在 GDScript 侧是 `String`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage(pub String);

#[godot_api]
impl IControl for GraphwarSetupControl {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            options: ROOM_DEFAULT_OPTIONS,
            entries: Vec::new(),
            room_name: GString::new(),
            title: GString::new(),
            chat_log: Vec::new(),
            wired: 0,
            missing: Rc::new(RefCell::new(Vec::new())),
            ready_calls: 0,
            text_change_calls: 0,
            visibility_change_calls: 0,
            chat_submitted_calls: 0,
            title_update_calls: 0,
            options_changed_calls: 0,
        }
    }

    /// `_ready`（原版 `0x4cf670` 的接线部分）：**机制 2** —— 连**子节点**的原生信号。
    ///
    /// 原版三条（`docs/16` §16 的 `0x4cf670` 段）：
    ///
    /// ```text
    /// RoomNameLineEdit.text_changed   → on_text_change
    /// ChatPopUp.visibility_changed    → on_visibility_change
    /// ChatLineEdit.text_submitted     → on_chat_submitted
    /// ```
    ///
    /// 找节点用 `find_child`（递归）—— 对应原版 `GraphwarNodes::get_node_by_name_rec`。
    /// 找不到就记进 `missing` 并跳过（原版 `get_node` 会报错，但那是**场景缺节点**的错，
    /// 不是本类的逻辑）。
    fn ready(&mut self) {
        self.ready_calls += 1;
        let wiring: [(&str, &str, &str); 3] = [
            ("RoomNameLineEdit", "text_changed", "on_text_change"),
            ("ChatPopUp", "visibility_changed", "on_visibility_change"),
            ("ChatLineEdit", "text_submitted", "on_chat_submitted"),
        ];

        let myself = self.to_gd();
        for (node_name, signal, handler) in wiring {
            let Some(node) = self.base().find_child_ex(node_name).owned(false).done() else {
                self.missing.borrow_mut().push(node_name.to_owned());
                continue;
            };
            let callable = myself.callable(handler);
            let err = node.clone().connect(signal, &callable);
            if err == godot::global::Error::OK {
                self.wired += 1;
            } else {
                self.missing
                    .borrow_mut()
                    .push(format!("{node_name}.{signal}: {err:?}"));
            }
        }
    }
}

#[godot_api]
impl GraphwarSetupControl {
    /// **机制 1**：自造的、四个 `int` 参数的信号（四格模式整组变了）。
    #[signal]
    fn options_changed(axis: i64, function: i64, turns: i64, time: i64);

    /// **机制 1**：自造的、无参信号（`SetupAction::StartMatch` 时发）。
    #[signal]
    fn start_requested();

    /// **机制 1**：自造的、无参信号（返回键）。
    #[signal]
    fn exit_requested();

    /// **机制 1**：自造的、带 `String` 的信号（聊天）。
    #[signal]
    fn chat_message(text: GString);

    /// 建 3 个**替身**子节点（`client/` 里没有 GameSetup 这一屏的场景）。
    ///
    /// 幂等：已经有了就跳过。返回当前"名字对得上"的替身个数。
    ///
    /// ⚠️ 这是**替代**：原版这些节点在 `.tscn` 里，本类只 `get_node` 不建。
    #[func]
    fn build_stub_children(&mut self) -> i64 {
        // `RoomNameLineEdit`：原版是个 `LineEdit`（`text_changed → on_text_change`）。
        if self.base().find_child_ex("RoomNameLineEdit").owned(false).done().is_none() {
            let mut edit = LineEdit::new_alloc();
            edit.set_name("RoomNameLineEdit");
            let node = edit.clone().upcast::<Node>();
            self.base_mut().add_child(&node);
        }
        // `ChatPopUp`：一个容器 `Control`（`visibility_changed → on_visibility_change`），
        // 底下挂 `ChatLineEdit`（原版树是 `…/ChatContainer2/ChatLineEdit`）。
        if self.base().find_child_ex("ChatPopUp").owned(false).done().is_none() {
            let mut popup = Control::new_alloc();
            popup.set_name("ChatPopUp");
            popup.set_visible(false);
            let node = popup.clone().upcast::<Node>();
            self.base_mut().add_child(&node);
        }
        if self.base().find_child_ex("ChatLineEdit").owned(false).done().is_none() {
            let mut edit = LineEdit::new_alloc();
            edit.set_name("ChatLineEdit");
            let node = edit.clone().upcast::<Node>();
            if let Some(mut popup) = self.base().find_child_ex("ChatPopUp").owned(false).done() {
                popup.add_child(&node);
            } else {
                self.base_mut().add_child(&node);
            }
        }
        let mut found = 0;
        for name in STUB_NODE_NAMES {
            if self.base().find_child_ex(name).owned(false).done().is_some() {
                found += 1;
            }
        }
        found
    }

    /// `_ready` 里真连上了几条（期望 3；缺节点时更小）。
    #[func]
    fn wired_count(&self) -> i64 {
        self.wired
    }

    /// 没找到 / 没连上的节点名（`|` 分隔）。
    #[func]
    fn missing_nodes(&self) -> GString {
        GString::from(join_text(&self.missing.borrow()).as_str())
    }

    /// `RoomNameLineEdit.text_changed` 的落点（原版方法名 `on_text_change`）。
    #[func]
    fn on_text_change(&mut self, new_text: GString) {
        self.text_change_calls += 1;
        self.room_name = new_text;
        self.force_title_update();
    }

    /// `ChatPopUp.visibility_changed` 的落点（原版方法名 `on_visibility_change`）。
    #[func]
    fn on_visibility_change(&mut self) {
        self.visibility_change_calls += 1;
    }

    /// `ChatLineEdit.text_submitted` 的落点（原版方法名 `on_chat_submitted`）。
    ///
    /// 原版还会把消息交给 `chat_message_event`（`0x4c…` 的后端事件）；那条链没接，
    /// 所以这里只发自造信号 + 记本地日志。
    #[func]
    fn on_chat_submitted(&mut self, text: GString) {
        self.chat_submitted_calls += 1;
        if self.chat_log.len() >= 8 {
            self.chat_log.remove(0);
        }
        self.chat_log.push(text.to_string());

        self.emit_chat_message(&text);
    }

    /// 标题：四格的线上名字拼起来（原版 `force_title_update` **写哪个节点未知**）。
    #[func]
    fn force_title_update(&mut self) {
        self.title_update_calls += 1;
        let names = self.options.names();
        self.title = GString::from(names.join(" ").as_str());
    }

    /// 标题字符串（`force_title_update` 的产物）。
    #[func]
    fn title_text(&self) -> GString {
        self.title.clone()
    }

    /// 房间名（`on_text_change` 收到的最新文本）。
    #[func]
    fn room_name(&self) -> GString {
        self.room_name.clone()
    }

    /// 聊天记录（`|` 分隔）。
    #[func]
    fn chat_log(&self) -> GString {
        GString::from(join_text(&self.chat_log).as_str())
    }

    /// 轴模式循环到下一档（**推断**的点击循环；原版没给这个按钮连 `pressed`）。
    #[func]
    fn cycle_axis_mode(&mut self) -> i64 {
        self.options.axis = self.options.axis.next();
        self.after_options_changed()
    }

    /// 函数模式切换另一档。
    #[func]
    fn cycle_function_mode(&mut self) -> i64 {
        self.options.function = self.options.function.next();
        self.after_options_changed()
    }

    /// 回合模式切换另一档。
    #[func]
    fn cycle_turns_mode(&mut self) -> i64 {
        self.options.turns = self.options.turns.next();
        self.after_options_changed()
    }

    /// 回合时限循环到下一档（六档循环；原版观测到按 1 次 → `01:59`、按 2 次 → `02:59`）。
    #[func]
    fn cycle_time_mode(&mut self) -> i64 {
        self.options.time = self.options.time.next();
        self.after_options_changed()
    }

    /// 轴模式判别值。
    #[func]
    fn axis_mode(&self) -> i64 {
        self.options.axis.as_u8() as i64
    }

    /// 函数模式判别值。
    #[func]
    fn function_mode(&self) -> i64 {
        self.options.function.as_u8() as i64
    }

    /// 回合模式判别值。
    #[func]
    fn turns_mode(&self) -> i64 {
        self.options.turns.as_u8() as i64
    }

    /// 回合时限判别值。
    #[func]
    fn time_mode(&self) -> i64 {
        self.options.time.as_u8() as i64
    }

    /// 四格的线上名字（`|` 分隔，顺序 axis / function / turn / time）。
    #[func]
    fn options_names(&self) -> GString {
        GString::from(self.options.names().join("|").as_str())
    }

    /// 四格的图标路径（`|` 分隔，同样四格顺序）。
    #[func]
    fn options_icons(&self) -> GString {
        GString::from(self.options.icons().join("|").as_str())
    }

    /// 回合时限的计时器标签（`02:59` 等）；无限档返回 `"inf"`（原版显示什么**未知**）。
    #[func]
    fn timer_label(&self) -> GString {
        match self.options.timer_label() {
            Some(label) => GString::from(label.as_str()),
            None => GString::from("inf"),
        }
    }

    /// 这一屏显示不显示倒计时。
    #[func]
    fn has_turn_clock(&self) -> bool {
        self.options.has_turn_clock()
    }

    /// 加一个人类玩家，返回新的条目数。
    #[func]
    fn add_human(&mut self, name: GString, team: i64, soldiers: i64) -> i64 {
        let team = team_of(team);
        let soldiers = soldiers.clamp(0, setup::MAX_SOLDIERS_PER_PLAYER as i64) as u8;
        self.entries.push(PlayerEntry::human(name.to_string(), team, soldiers));
        self.entries.len() as i64
    }

    /// 加一个 Bot（难度档 `0..=5`），返回新的条目数。
    #[func]
    fn add_bot(&mut self, level: i64, name: GString, team: i64, soldiers: i64) -> i64 {
        let team = team_of(team);
        let soldiers = soldiers.clamp(0, setup::MAX_SOLDIERS_PER_PLAYER as i64) as u8;
        let level = level.clamp(0, 5) as u8;
        self.entries
            .push(PlayerEntry::bot(level, name.to_string(), team, soldiers));
        self.entries.len() as i64
    }

    /// `AddSoldierButton`：给第 `index` 行加一个士兵（饱和在 5）。
    #[func]
    fn add_soldier(&mut self, index: i64) -> bool {
        match self.entries.get_mut(index.max(0) as usize) {
            Some(entry) => entry.add_soldier(),
            None => false,
        }
    }

    /// `RemoveSoldierButton`：给第 `index` 行减一个士兵（饱和在 0）。
    #[func]
    fn remove_soldier(&mut self, index: i64) -> bool {
        match self.entries.get_mut(index.max(0) as usize) {
            Some(entry) => entry.remove_soldier(),
            None => false,
        }
    }

    /// `ChangeTeamButton`：第 `index` 行换队。
    #[func]
    fn change_team(&mut self, index: i64) -> bool {
        match self.entries.get_mut(index.max(0) as usize) {
            Some(entry) => {
                entry.change_team();
                true
            }
            None => false,
        }
    }

    /// 现在有几行玩家。
    #[func]
    fn entry_count(&self) -> i64 {
        self.entries.len() as i64
    }

    /// 每行 `name:team:soldiers[:botN]`，`|` 分隔。
    #[func]
    fn entries_text(&self) -> GString {
        let rows: Vec<String> = self
            .entries
            .iter()
            .map(|entry| {
                let team = match entry.team {
                    Team::One => "1",
                    Team::Two => "2",
                };
                match entry.bot_level {
                    Some(level) => format!("{}:{}:{}:bot{}", entry.name, team, entry.soldiers, level),
                    None => format!("{}:{}:{}", entry.name, team, entry.soldiers),
                }
            })
            .collect();
        GString::from(rows.join("|").as_str())
    }

    /// ` Start `：有对手 → 发 `start_requested` 并返回 `true`；没对手 → **纯静默**，返回 `false`。
    ///
    /// 判据是 [`setup::start_action`]（`can_start` = 两队都有人），原版观测：
    /// 不加 Bot 直接 Start 是**静默无操作**（`docs/16` §2.1.5）。
    #[func]
    fn start(&mut self) -> bool {
        match setup::start_action(&self.entries) {
            SetupAction::StartMatch => {
                self.emit_start_requested();
                true
            }
            SetupAction::SilentNoOp => false,
            // `start_action` 只会返回上面两种；`Back` 分支留着是为了 match 穷尽。
            SetupAction::Back(_) => false,
        }
    }

    /// 返回键：发 `exit_requested`（去向 [`setup::exit_action`] = `LanLobby`，**推断**）。
    #[func]
    fn exit(&mut self) -> bool {
        self.emit_exit_requested();
        true
    }

    /// `_ready` 被调到几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `on_text_change` 被调到几次。
    #[func]
    fn text_change_count(&self) -> i64 {
        self.text_change_calls
    }

    /// `on_visibility_change` 被调到几次。
    #[func]
    fn visibility_change_count(&self) -> i64 {
        self.visibility_change_calls
    }

    /// `on_chat_submitted` 被调到几次。
    #[func]
    fn chat_submitted_count(&self) -> i64 {
        self.chat_submitted_calls
    }

    /// `force_title_update` 被调到几次。
    #[func]
    fn title_update_count(&self) -> i64 {
        self.title_update_calls
    }

    /// `options_changed` 发了几次。
    #[func]
    fn options_changed_count(&self) -> i64 {
        self.options_changed_calls
    }

    // -----------------------------------------------------------------------
    // 内部
    // -----------------------------------------------------------------------

    /// 四格变了之后：刷标题 + 发 `options_changed`，返回当前轴模式判别值。
    fn after_options_changed(&mut self) -> i64 {
        self.force_title_update();
        self.options_changed_calls += 1;
        let axis = self.options.axis.as_u8() as i64;
        let function = self.options.function.as_u8() as i64;
        let turns = self.options.turns.as_u8() as i64;
        let time = self.options.time.as_u8() as i64;
        // 机制 1：类型安全生成器（四个 i64 参数）。
        self.signals()
            .options_changed()
            .emit(axis, function, turns, time);
        axis
    }

    /// **机制 1**：无参信号。
    fn emit_start_requested(&mut self) {
        self.signals().start_requested().emit();
    }

    /// **机制 1**：无参信号。
    fn emit_exit_requested(&mut self) {
        self.signals().exit_requested().emit();
    }

    /// **机制 1**：带 `String` 参数的信号。
    fn emit_chat_message(&mut self, text: &GString) {
        let payload = text.clone();
        self.signals().chat_message().emit(&payload);
    }
}

/// `0`/其它 ⇒ 一队；`2` ⇒ 二队。
fn team_of(raw: i64) -> Team {
    if raw == 2 {
        Team::Two
    } else {
        Team::One
    }
}

/// 把一串字符串接成 `a|b|c`。
///
/// **返回 `String` 而不是 `GString`**：`GString::from` 要调引擎的构造函数，没有 Godot 进程
/// 就 panic，所以纯 Rust 的那一半留在 `String` 上，到 `#[func]` 边界才转。
fn join_text<S: AsRef<str>>(items: &[S]) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push('|');
        }
        out.push_str(item.as_ref());
    }
    out
}

/// 让 `AxisMode` / `FunctionMode` / `TurnMode` / `TimeMode` 的 `next()` 在编译期被用到
/// （它们只经 `#[func]` 调用，单测里也直接跑一遍）。
#[allow(dead_code)]
fn cycle_probe() -> (AxisMode, FunctionMode, TurnMode, TimeMode) {
    (
        AxisMode::EveryUnit.next(),
        FunctionMode::NormalFunction.next(),
        TurnMode::SequentialTurns.next(),
        TimeMode::Timer1m.next(),
    )
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 四个模式的循环顺序 = 判别值序（`docs/16` §16：图标 id 序 = 名字表序）。
    #[test]
    fn mode_cycles_follow_the_discriminant_order() {
        let (axis, function, turns, time) = cycle_probe();
        assert_eq!(axis, AxisMode::NoAxis);
        assert_eq!(function, FunctionMode::DiffEqFunction);
        assert_eq!(turns, TurnMode::SimultaneousTurns);
        assert_eq!(time, TimeMode::Timer2m, "Timer1m +1 = Timer2m（= 按 1 次 02:59）");
    }

    /// 新建房间的四格初值（`ROOM_DEFAULT_OPTIONS`，**观测**）。
    #[test]
    fn room_defaults_are_the_observed_new_room_game_info() {
        let options = ROOM_DEFAULT_OPTIONS;
        assert_eq!(options.axis, AxisMode::EveryUnit);
        assert_eq!(options.function, FunctionMode::NormalFunction);
        assert_eq!(options.turns, TurnMode::SequentialTurns);
        assert_eq!(options.time, TimeMode::Timer1m);
        assert_eq!(
            options.names(),
            [
                "EveryUnit",
                "NormalFunction",
                "SequentialTurns",
                "Timer1m"
            ]
        );
        assert_eq!(options.timer_label().as_deref(), Some("00:59"));
        assert!(options.has_turn_clock());
    }

    /// 按 1 次 / 2 次 `TurnTimeButton` 的标签 —— `docs/16` §2.1.10 的真帧（`01:59` / `02:59`）。
    #[test]
    fn clicking_time_mode_once_and_twice_matches_the_observed_frames() {
        let once = ROOM_DEFAULT_OPTIONS.time.next();
        let twice = once.next();
        assert_eq!(once.timer_label().as_deref(), Some("01:59"));
        assert_eq!(twice.timer_label().as_deref(), Some("02:59"));
        // 六档循环回到起点，且无限档没有标签。
        assert_eq!(TimeMode::TimerInf.next(), TimeMode::Timer30s);
        assert_eq!(TimeMode::TimerInf.timer_label(), None);
    }

    /// Start 的判据：**两队都有人**才真开始；只有一个队是静默无操作（**观测**）。
    #[test]
    fn start_is_silent_until_both_teams_have_someone() {
        assert!(!setup::can_start(&[]));
        let one_team = [PlayerEntry::human("A", Team::One, 1)];
        assert!(!setup::can_start(&one_team));
        assert_eq!(setup::start_action(&one_team), SetupAction::SilentNoOp);

        let both = [
            PlayerEntry::human("A", Team::One, 2),
            PlayerEntry::bot(3, "Deep Blue", Team::Two, 2),
        ];
        assert!(setup::can_start(&both));
        assert_eq!(setup::start_action(&both), SetupAction::StartMatch);
    }

    /// 士兵数饱和在 5 / 0（5 个槽是观测；"到 5 不再涨"是推断）。
    #[test]
    fn soldier_count_saturates_at_five_and_zero() {
        let mut entry = PlayerEntry::human("A", Team::One, 5);
        assert!(!entry.add_soldier());
        assert_eq!(entry.soldiers, 5);
        for _ in 0..5 {
            assert!(entry.remove_soldier());
        }
        assert!(!entry.remove_soldier());
        assert_eq!(entry.soldiers, 0);
        assert_eq!(entry.soldier_slots().len(), 5);
    }

    /// 11 个 `get_node` 名字里只建了 3 个替身 —— 这个"子集"关系要钉住。
    #[test]
    fn stub_nodes_are_a_subset_of_the_original_get_node_names() {
        for name in STUB_NODE_NAMES {
            assert!(
                ALL_GET_NODE_NAMES.contains(&name),
                "{name} 不在原版 11 个 get_node 名字里"
            );
        }
        assert_eq!(ALL_GET_NODE_NAMES.len(), 11);
        assert_eq!(STUB_NODE_NAMES.len(), 3);
    }

    /// `team_of` 的映射（`0`/其它 → 一队，`2` → 二队）。
    #[test]
    fn team_of_maps_two_to_team_two() {
        assert_eq!(team_of(0), Team::One);
        assert_eq!(team_of(1), Team::One);
        assert_eq!(team_of(2), Team::Two);
    }
}
