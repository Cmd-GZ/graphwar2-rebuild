//! 大厅控制器：`GraphwarLobbyControl`（一对互斥按钮 + LAN 列表的"清空"）。
//!
//! 本模块回答两件事：**这 1 个函数到底是"控制器"还是别的东西**（→ 3 个函数：
//! 一对互斥按钮 + 装配它们的 `new`，§1）、**它和 `lan_vbox` 怎么分工**（§3）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（3 个；`docs/02` §18.1 的"1 个"只数了 `new`）
//!
//! | 函数 | x86 vaddr / 字节 | arm64 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x3ef990` / 1760 | `0x423de4` / 1624 | 28/29/30/42/47/52 | 取 3 个节点 + 注册 2 个回调 + 立刻调一次 `show_local_button` | 逐位 |
//! | `show_local_button` | **已内联进 `new`** | `0x4237e0` / 768 | — | 本地按钮按下：`set_pressed(true/false)` + 清 LAN 列表 | 逐位 |
//! | `show_global_button` | **已内联进 `new`** | `0x423ae0` / 772 | — | 镜像 | 逐位 |
//!
//! x86 侧 6 条 `panic::Location`（`:28:122/:29:124/:30:110/:42:37/:47:37/:52:23`）
//! **全部落在 `0x3ef990` 一个函数体内** ⇒ 两个 `show_*` 在 x86 上是内联的，
//! 这正是 `docs/02` §18.1 只数出 1 个方法的原因。arm64 未 strip，两个 `show_*`
//! 是独立符号（768 / 772 B，几乎同长 —— 因为两者只差"哪个按钮 `set_pressed(true)`"）。
//!
//! ## 2. `new` 做了什么（逐位）
//!
//! ```text
//! get_node_by_name("ShowLocalButton")            -> self + 0x18   ; 第三个参数 = 15
//! get_node_by_name("ShowGlobalButton")           -> self + 0x30   ; 第三个参数 = 16
//! get_node_by_name::<GraphwarLanVBox>("GraphwarLanVBox") -> self + 0x48
//! 按钮 A 的回调表 push(闭包 → show_local_button)   ; 闭包 0x42443c
//! 按钮 B 的回调表 push(闭包 → show_global_button)  ; 闭包 0x424468
//! show_local_button()                            ; 0x42413c，构造完立刻跑一次
//! ```
//!
//! 两个闭包都用 `adr x1, <方法>` + `b WeakCaller::call_`（`0x3d6cdc`）——
//! 与各 manager 的 `subscribe_to_event` 是**同一套弱引用派发**，但这里是
//! **按钮的回调表**而不是事件表：
//!
//! ```text
//! mov w0<len+1> ; str x10, [<button binding>+0x48]
//! add x8, x8, x24, lsl #4        ; 元素 16 字节
//! ```
//!
//! ⇒ 按钮对象里有一个 `Vec<Box<dyn FnMut(&str)>>`（`Box<dyn FnMut>` = 16 字节），
//! 偏移 `+0x48` —— 这就是 `graphwar_button` 的 `GraphwarButton` 自己的回调表，
//! **不是** Godot 的 `connect`（`new` 里没有任何 `connect`/信号名字符串）。
//!
//! ## 3. 两个 `show_*` 与 `lan_vbox` 的分工（逐位）
//!
//! `show_local_button`（`show_global_button` 完全镜像）：
//!
//! ```text
//! self+0x18 -> set_pressed(true)      ; BaseButton::set_pressed
//! self+0x30 -> set_pressed(false)     ; ← 互斥（单选对）
//! self+0x48 -> bind_mut()             ; 取 GraphwarLanVBox 的 Rust 侧
//!   for 每一行（entry 步长 0x30）:
//!         Node::queue_free()          ; 0x7c9338 ← 把列表行节点**释放**
//!         <hashmap erase: ctrl 写 0xff，items -= 1>
//! ```
//!
//! ⇒ 分工：
//!
//! * `lobby_control` = **"我方"大厅的开关**：两个按钮互斥 + 在切模式时**清掉 LAN 列表**；
//! * `lan_vbox` = **列表本体**：它持有 `HashMap<_, 48 字节行>`（每行一个 `Gd<Node>`），
//!   负责行的增删与 `queue_free`。`show_*` 在这里做的是**直接调用它的清空路径**
//!   （被内联进 `show_*` 的 40 余条指令），而不是发事件。
//! * `setup_control` 不属于本模块：本模块只处理"本地/全球"这一个开关，不管开局流程。
//!
//! ⚠️ 被内联的那段在清空前先读 `ldrb w8, [binding+0x68]` 并 `cbz` 跳过
//! ⇒ 列表对象里有一个"已初始化/非空"标志，字段语义**未知**（只观测到存在）。
//!
//! ## 4. 未确定
//!
//! * 两个按钮回调收到的 `&str` 是什么（`FnMut(&str)` 的形状是逐位的，实参由
//!   `GraphwarButton` 的派发方决定，不在本模块）。
//! * `+0x68` 那个标志位的语义（§3 末）。
//! * 两个按钮的**图标/文本**：`docs/112` §8 记过"设文本/图标（推断）"；
//!   本轮在 arm64 上**只看到** `set_pressed`，没有任何 `set_text`/`set_texture` 调用
//!   ⇒ 该推断**降级为未观测**（`docs/112` §8 那一句应删）。

/// 节点名：本地大厅按钮。
pub const NODE_SHOW_LOCAL: &str = "ShowLocalButton";
/// 节点名：全球大厅按钮。
pub const NODE_SHOW_GLOBAL: &str = "ShowGlobalButton";
/// 节点名：LAN 列表容器。
pub const NODE_LAN_VBOX: &str = "GraphwarLanVBox";

/// `ShowLocalButton` 在 `self` 里的偏移。
pub const SHOW_LOCAL_OFFSET: usize = 0x18;
/// `ShowGlobalButton` 在 `self` 里的偏移。
pub const SHOW_GLOBAL_OFFSET: usize = 0x30;
/// `GraphwarLanVBox` 在 `self` 里的偏移。
pub const LAN_VBOX_OFFSET: usize = 0x48;

/// 按钮回调表在按钮对象里的偏移（`Vec<Box<dyn FnMut(&str)>>`，元素 16 字节）。
pub const BUTTON_CALLBACKS_OFFSET: usize = 0x48;
/// LAN 列表行的步长（`mov w9, #0x30`）。
pub const LAN_ROW_STRIDE: usize = 0x30;

/// 一对互斥按钮的 `pressed` 状态（原版是 `BaseButton::set_pressed` 的两个布尔）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LobbyButtons {
    /// `ShowLocalButton.pressed`（`self + 0x18`）。
    pub show_local_pressed: bool,
    /// `ShowGlobalButton.pressed`（`self + 0x30`）。
    pub show_global_pressed: bool,
}

/// `GraphwarLobbyControl` 的可判定状态。
///
/// 原版三个字段都是 `Gd<...>`（`+0x18` 按钮、`+0x30` 按钮、`+0x48` `GraphwarLanVBox`）；
/// 这里用"按钮状态 + LAN 列表行 id"表达同一件事，不引 Godot。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphwarLobbyControl {
    /// 两个按钮的 `pressed` 状态。
    pub buttons: LobbyButtons,
    /// `GraphwarLanVBox` 里的行（`queue_free` 时按 id 释放）。
    pub lan_rows: Vec<u32>,
}

impl GraphwarLobbyControl {
    /// `new`：装配（取节点 + 注册回调）之后**立刻**执行一次 `show_local_button`。
    ///
    /// 原版这一步是 `bl 0x4237e0`（`0x42413c`），即"默认停在本地方案"。
    pub fn new() -> Self {
        let mut me = Self::default();
        me.show_local_button();
        me
    }

    /// `show_local_button`：本地 `true` / 全球 `false`，然后清 LAN 列表。
    ///
    /// 返回被 `queue_free` 掉的行 id（原版的 `Node::queue_free`）。
    pub fn show_local_button(&mut self) -> Vec<u32> {
        self.buttons.show_local_pressed = true;
        self.buttons.show_global_pressed = false;
        self.clear_lan_list()
    }

    /// `show_global_button`：镜像。
    pub fn show_global_button(&mut self) -> Vec<u32> {
        self.buttons.show_local_pressed = false;
        self.buttons.show_global_pressed = true;
        self.clear_lan_list()
    }

    /// 两个 `show_*` 共用的那段（`GraphwarLanVBox` 的清空路径，被内联进两者）。
    pub fn clear_lan_list(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.lan_rows)
    }

    /// 往 LAN 列表里加一行（由 `lan_vbox` 负责调用；本模块只保证清空语义可断言）。
    pub fn push_lan_row(&mut self, row_id: u32) {
        self.lan_rows.push(row_id);
    }

    /// `new` 注册的那两个回调的方法名（`FnMut(&str)`，见模块文档 §2）。
    pub const CALLBACKS: [(&'static str, &'static str); 2] = [
        (NODE_SHOW_LOCAL, "show_local_button"),
        (NODE_SHOW_GLOBAL, "show_global_button"),
    ];
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn new_starts_on_local_and_clears() {
        let me = GraphwarLobbyControl::new();
        assert_eq!(
            me.buttons,
            LobbyButtons { show_local_pressed: true, show_global_pressed: false }
        );
        assert!(me.lan_rows.is_empty());
    }

    #[test]
    fn toggle_is_exclusive_and_frees_rows() {
        let mut me = GraphwarLobbyControl::new();
        me.push_lan_row(11);
        me.push_lan_row(12);
        let freed = me.show_global_button();
        assert_eq!(freed, vec![11, 12]);
        assert_eq!(
            me.buttons,
            LobbyButtons { show_local_pressed: false, show_global_pressed: true }
        );
        assert!(me.lan_rows.is_empty(), "清空后列表必须为空");
        // 再按本地：没有行可释放。
        assert!(me.show_local_button().is_empty());
    }
}
