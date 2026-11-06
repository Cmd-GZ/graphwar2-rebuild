//! `graphwar_lobby_control` 的验收：**3 个函数 / 互斥按钮 / 清 LAN 列表**。
//!
//! ⚠️ 用 `#[path]` 直接编进本测试二进制 ⇒ **不需要** `src/lib.rs` 里先加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `three_functions_not_one` | `new` + `show_local_button` + `show_global_button` | **逐位**（arm64 `.symtab`：`0x423de4` / `0x4237e0` / `0x423ae0`） |
//! | `new_starts_on_local` | `new` 末尾 `bl show_local_button` | **逐位**（`0x42413c`） |
//! | `buttons_are_exclusive` | 两个 `set_pressed` 一真一假 | **逐位**（`0x42381c` 与 `0x423838` 两个 `set_pressed`） |
//! | `toggling_frees_every_lan_row` | 每次切换都把列表行 `queue_free` 并清空 | **逐位**（`Node::queue_free` `0x7c9338` + hashmap erase） |
//! | `node_names_and_offsets` | 三个节点名与 `+0x18/+0x30/+0x48` | **逐位**（`get_node_by_name` 的三个字符串） |
//! | `callbacks_are_button_side` | 两个回调注册在按钮对象里（不是 Godot `connect`） | **逐位**（`Vec<Box<dyn FnMut(&str)>>`，元素 16 字节，偏移 `+0x48`） |
#![allow(dead_code)]

#[path = "../src/graphwar_lobby_control.rs"]
mod graphwar_lobby_control;

use graphwar_lobby_control::*;

/// 模块有 3 个函数：`new` 加上两个 `show_*`（`docs/02` §18.1 只数了 1 个）。
#[test]
fn three_functions_not_one() {
    let mut me = GraphwarLobbyControl::new();
    me.show_local_button();
    me.show_global_button();
    assert_eq!(GraphwarLobbyControl::CALLBACKS.len(), 2);
}

/// `new` 装配完**立刻**按一次"本地"（原版 `0x42413c` 的 `bl show_local_button`）。
#[test]
fn new_starts_on_local() {
    let me = GraphwarLobbyControl::new();
    assert!(me.buttons.show_local_pressed);
    assert!(!me.buttons.show_global_pressed);
    assert!(me.lan_rows.is_empty());
}

/// 两个按钮互斥：一个 `true` 另一个必 `false`（`BaseButton::set_pressed` 两次）。
#[test]
fn buttons_are_exclusive() {
    let mut me = GraphwarLobbyControl::new();
    me.show_global_button();
    assert_eq!(
        me.buttons,
        LobbyButtons { show_local_pressed: false, show_global_pressed: true }
    );
    me.show_local_button();
    assert_eq!(
        me.buttons,
        LobbyButtons { show_local_pressed: true, show_global_pressed: false }
    );
    // 无论按哪个，两个状态都不允许同时为真。
    assert!(!(me.buttons.show_local_pressed && me.buttons.show_global_pressed));
}

/// 切换会释放 LAN 列表里的**每一行**（原版对每行 `Node::queue_free` 并从 map 里擦掉）。
#[test]
fn toggling_frees_every_lan_row() {
    let mut me = GraphwarLobbyControl::new();
    me.push_lan_row(1);
    me.push_lan_row(2);
    me.push_lan_row(3);
    assert_eq!(me.show_global_button(), vec![1, 2, 3]);
    assert!(me.lan_rows.is_empty());
    // 列表空着的时候按也不出错。
    assert!(me.show_local_button().is_empty());
    // 清空是"释放"，不是"记住"。
    me.push_lan_row(4);
    assert_eq!(me.clear_lan_list(), vec![4]);
}

/// 三个节点名与三个偏移。
#[test]
fn node_names_and_offsets() {
    assert_eq!(NODE_SHOW_LOCAL, "ShowLocalButton");
    assert_eq!(NODE_SHOW_GLOBAL, "ShowGlobalButton");
    assert_eq!(NODE_LAN_VBOX, "GraphwarLanVBox");
    assert_eq!(
        (SHOW_LOCAL_OFFSET, SHOW_GLOBAL_OFFSET, LAN_VBOX_OFFSET),
        (0x18, 0x30, 0x48)
    );
    // 节点名的长度（`get_node_by_name` 的第三个参数是 15 / 16 / 15）。
    assert_eq!(NODE_SHOW_LOCAL.len(), 15);
    assert_eq!(NODE_SHOW_GLOBAL.len(), 16);
    assert_eq!(NODE_LAN_VBOX.len(), 15);
}

/// 回调挂在**按钮对象**上（`Vec<Box<dyn FnMut(&str)>>`），不是 Godot 的 `connect`。
#[test]
fn callbacks_are_button_side() {
    assert_eq!(BUTTON_CALLBACKS_OFFSET, 0x48);
    assert_eq!(LAN_ROW_STRIDE, 0x30);
    let (a, b) = (GraphwarLobbyControl::CALLBACKS[0], GraphwarLobbyControl::CALLBACKS[1]);
    assert_eq!(a, (NODE_SHOW_LOCAL, "show_local_button"));
    assert_eq!(b, (NODE_SHOW_GLOBAL, "show_global_button"));
}
