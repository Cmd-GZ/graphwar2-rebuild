//! T109 P6–P9 落码的验收：play_screen_control `clear_state` + root_node 2 条
//! + computer_player_manager `computer_function_thread`。
//!
//! 期望值全部来自 arm64 反汇编的立即数 / 分支 / 尾调用目标。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `clear_state_clears_the_debug_label` | 字段 `+0x38` 的 RichTextLabel 被清一次 | **观测**（`0x3deb38 b 0x7b5858`） |
//! | `show_pop_up_sets_visible_true` | 命中 ⇒ `set_visible(true)`；未命中 ⇒ 什么都不做 | **观测**（`0x3a5e84 mov w1,#0x1` + `0x3a5ea4 b set_visible`） |
//! | `help_predicate_is_or_two_equals_fourteen` | `(kind\|2)==0xe` ⇔ 打印并退出 | **观测**（`0x3ab1dc orr` / `0x3ab1e0 cmp`） |
//! | `thread_branch_selects_old_or_new` | `use_old` ⇒ 老算法，否则新算法；最后一定 send | **观测**（`0x4aff58 tbz` / `0x4aff88` / `0x4affc0` / `0x4affd0`） |

#![allow(dead_code)]

use gw2_rebuild::graphwar_computer_player_manager::computer_function_thread;
use gw2_rebuild::graphwar_play_screen_control::{DebugLabelSink, GraphwarPlayScreenControl};
use gw2_rebuild::graphwar_root_node::{
    print_help_and_exit_if_requested, show_pop_up, HelpOutcome, PopUpHost, CLAP_ERROR_INNER_SIZE,
    CLAP_ERROR_KIND_OFFSET, CLAP_PRINT_KINDS_EQ, CLAP_PRINT_KINDS_MASK,
    POPUP_TABLE_ITEMS_OFFSET, POPUP_TABLE_STRIDE,
};

// ── clear_state ───────────────────────────────────────────────────────────────

#[derive(Default)]
struct MockLabelSink {
    label: u32,
    cleared: Vec<u32>,
}

impl DebugLabelSink for MockLabelSink {
    type Label = u32;
    fn debug_text_label(&mut self) -> u32 {
        self.label // 原版：self+0x38 那个 RichTextLabel
    }
    fn rich_text_clear(&mut self, label: u32) {
        self.cleared.push(label);
    }
}

#[test]
fn clear_state_clears_the_debug_label() {
    let mut sink = MockLabelSink {
        label: 0x38,
        ..Default::default()
    };
    assert_eq!(GraphwarPlayScreenControl::clear_state(&mut sink), 1);
    assert_eq!(sink.cleared, vec![0x38], "只清 +0x38 那个标签，且只清一次");
}

// ── show_pop_up ───────────────────────────────────────────────────────────────

#[derive(Default)]
struct MockPopUps {
    /// 表里有这个节点吗
    node: Option<u64>,
    /// (`节点`, `visible`)
    calls: Vec<(u64, bool)>,
}

impl PopUpHost for MockPopUps {
    type Node = u64;
    fn popup_by_name(&mut self, name: &str) -> Option<u64> {
        if name == "EndGamePopUp" {
            self.node
        } else {
            None
        }
    }
    fn upcast_canvas_item(&mut self, node: u64) -> u64 {
        node
    }
    fn set_visible(&mut self, node: u64, visible: bool) {
        self.calls.push((node, visible));
    }
}

#[test]
fn show_pop_up_sets_visible_true() {
    // 偏移/步长常数（`0x3a5d80 ldr x8,[x0,#0x18]` / `0x3a5dac mov w24,#0x28`）
    assert_eq!(POPUP_TABLE_ITEMS_OFFSET, 0x18);
    assert_eq!(POPUP_TABLE_STRIDE, 0x28);

    // 命中 ⇒ set_visible(true)
    let mut host = MockPopUps {
        node: Some(0xabc),
        ..Default::default()
    };
    assert!(show_pop_up(&mut host, "EndGamePopUp"));
    assert_eq!(host.calls, vec![(0xabc, true)], "置可见的实参是 true（0x3a5e84 w1=#1）");

    // 未命中 ⇒ 只有 ret（0x3a5e48），不 panic、不置任何东西
    let mut host = MockPopUps::default();
    assert!(!show_pop_up(&mut host, "NoSuchPopUp"));
    assert!(host.calls.is_empty());
}

// ── print_help_and_exit_if_requested ─────────────────────────────────────────

#[test]
fn help_predicate_is_or_two_equals_fourteen() {
    // 判据常数（全部来自指令）
    assert_eq!(CLAP_PRINT_KINDS_MASK, 0x2);
    assert_eq!(CLAP_PRINT_KINDS_EQ, 0x0e);
    assert_eq!(CLAP_ERROR_KIND_OFFSET, 0xdd);
    assert_eq!(CLAP_ERROR_INNER_SIZE, 0xe0);

    // 解析成功 ⇒ Proceed（0x3ab1c8 的非 Err 臂）
    assert_eq!(print_help_and_exit_if_requested(None), HelpOutcome::Proceed);

    // (kind|2)==14 的两个取值 ⇒ 打印并退出
    assert_eq!(
        print_help_and_exit_if_requested(Some(0x0e)),
        HelpOutcome::PrintedAndExited
    );
    assert_eq!(
        print_help_and_exit_if_requested(Some(0x0c)),
        HelpOutcome::PrintedAndExited,
        "0x0c | 0x2 = 0x0e ⇒ 同一臂"
    );

    // 其它 kind ⇒ 走 drop_in_place<ErrorInner> + dealloc(0xe0,8) 那条路（Proceed）
    for kind in [0u8, 1, 2, 3, 0x0d, 0x0f, 0xff] {
        assert_eq!(
            print_help_and_exit_if_requested(Some(kind)),
            HelpOutcome::Proceed,
            "kind={kind:#x} 不该进打印臂"
        );
    }
}

// ── computer_function_thread ─────────────────────────────────────────────────

#[test]
fn thread_branch_selects_old_or_new() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));

    // 老算法（`0x4aff88`）
    let l = log.clone();
    let ok = computer_function_thread(
        true,
        move || {
            l.borrow_mut().push("new_evolutionator");
            7u32
        },
        {
            let l = log.clone();
            move |x| {
                l.borrow_mut().push("calculate_functions_old");
                x + 1
            }
        },
        {
            let l = log.clone();
            move |x| {
                l.borrow_mut().push("calculate_functions");
                x + 100
            }
        },
        {
            let l = log.clone();
            move |r| {
                l.borrow_mut().push("send");
                assert_eq!(r, 8);
                true
            }
        },
    );
    assert!(ok);
    assert_eq!(
        *log.borrow(),
        vec!["new_evolutionator", "calculate_functions_old", "send"]
    );

    // 新算法（`0x4affc0`）——`tbz w25,#0` 为 0 的那支
    let log2 = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let l = log2.clone();
    let ok = computer_function_thread(
        false,
        move || {
            l.borrow_mut().push("new_evolutionator");
            7u32
        },
        {
            let l = log2.clone();
            move |x| {
                l.borrow_mut().push("calculate_functions_old");
                x + 1
            }
        },
        {
            let l = log2.clone();
            move |x| {
                l.borrow_mut().push("calculate_functions");
                x + 100
            }
        },
        {
            let l = log2.clone();
            move |r| {
                l.borrow_mut().push("send");
                assert_eq!(r, 107);
                false
            }
        },
    );
    assert!(!ok, "send 的结果原样返回");
    assert_eq!(
        *log2.borrow(),
        vec!["new_evolutionator", "calculate_functions", "send"]
    );
}
