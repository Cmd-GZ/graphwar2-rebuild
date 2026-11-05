//! `graphwar_play_screen_control.rs` 的复原验收（判据逐条对拍上面 `src/play_control.rs`）。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//! 每一条右侧的真值都能指到反汇编地址；地址与口径见 `docs/14-turn.md` §9。

use gw2_rebuild::play_control::*;

/// `0x3d9b50` / `0x3d9e40` 这一对：5 个节点的布尔值**恰好互补**。
///
/// 二进制依据（每条都能在 `objdump -d --start-address=0x3d9b50 --stop-address=0x3da130`
/// 里对上）：
/// * `0x3d9b50`：`xor %esi,%esi` @`0x3d9be2` / `0x3d9c64` / `0x3d9ce6`，
///   `mov $0x1,%esi` @`0x3d9d65` / `0x3d9de3`；
/// * `0x3d9e40`：`mov $0x1,%esi` @`0x3d9ecf` / `0x3d9f51` / `0x3d9fd3`，
///   `xor %esi,%esi` 在最后两处。
#[test]
fn widget_flags_are_exact_and_complementary() {
    assert_eq!(UiMode::Replay.widget_flags(), [false, false, false, true, true]);
    assert_eq!(UiMode::Play.widget_flags(), [true, true, true, false, false]);

    let a = UiMode::Replay.widget_flags();
    let b = UiMode::Play.widget_flags();
    for i in 0..5 {
        assert_ne!(a[i], b[i], "第 {i} 个节点在两个函数里必须取反");
    }
}

/// 5 个节点的**名字与顺序**必须与二进制里的 `get_node` 调用顺序一致：
/// `0x3d9b94` LeftButtonGrid → `0x3d9c16` RightButtonGrid → `0x3d9c98` FireContainer
/// → `0x3d9d1a` LeftReplaysButtonsGrid → `0x3d9d99` RightReplaysButtonsGrid。
#[test]
fn widget_order_matches_binary_node_lookup_order() {
    let names: Vec<&str> = UiMode::Play.widgets().iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        vec![
            "LeftButtonGrid",
            "RightButtonGrid",
            "FireContainer",
            "LeftReplaysButtonsGrid",
            "RightReplaysButtonsGrid",
        ]
    );
}

/// `0x3dc7f0` 只 `connect` 两个信号：`function_info_event`(`0x1bf61a`)、
/// `function_fire_event`(`0x1bf607`)，两处长度立即数都是 `0x13`。
///
/// 反面同样重要：`function_active_event` / `function_points_event`
/// **不在**本模块的订阅表里 —— 这是"弹道不在这一层清"的否定证据。
#[test]
fn subscribed_signals_are_exactly_info_and_fire() {
    assert_eq!(SUBSCRIBED_SIGNALS.len(), 2);
    assert_eq!(subscribed_remote_events(), vec!["function_fire_event", "function_info_event"]);

    for absent in [
        "game_state_info_event",
        "function_active_event",
        "function_points_event",
        "turn_info_event",
        "end_of_turn_info_event",
        "effect_info_event",
        "clear_obstacles_event",
    ] {
        assert!(
            !SUBSCRIBED_SIGNALS.contains(&absent),
            "{absent} 不在 play_screen_control 的直接 connect 表里"
        );
    }
}

/// 15 条远程事件名按 `.rodata`(`0x1bf5b0`) 顺序，且本模块订阅的恰好是其中的
/// 第 3、4 条（`function_fire_event`、`function_info_event`）。
#[test]
fn remote_event_name_table_order() {
    assert_eq!(REMOTE_EVENT_NAMES.len(), 15);
    assert_eq!(REMOTE_EVENT_NAMES[2], "function_fire_event");
    assert_eq!(REMOTE_EVENT_NAMES[3], "function_info_event");
    assert_eq!(REMOTE_EVENT_NAMES[4], "function_active_event");
    assert_eq!(REMOTE_EVENT_NAMES[5], "function_points_event");
    assert_eq!(REMOTE_EVENT_NAMES[14], "time_mode_info_event");
}

/// `0x3da130` 引用的 6 条横幅文字，**逐字**。
/// 注意 `"YOU lOST"` 是小写 L（`.rodata` `0x19fcc8`），原版就是这样。
#[test]
fn victory_texts_are_literal() {
    assert_eq!(VictoryBanner::Team1.text(), "Team 1 Victory!");
    assert_eq!(VictoryBanner::Team2.text(), "Team 2 Victory!");
    assert_eq!(VictoryBanner::SelfWon.text(), "YOU WON");
    assert_eq!(VictoryBanner::SelfDidNotWin.text(), "YOU DID NOT WIN");
    assert_eq!(VictoryBanner::SelfLost.text(), "YOU lOST");
    assert_eq!(VictoryBanner::Tie.text(), "Tie!");

    // 6 条互不相同（没有两条被映射到同一串）。
    let mut v: Vec<&str> = [
        VictoryBanner::Team1,
        VictoryBanner::Team2,
        VictoryBanner::SelfWon,
        VictoryBanner::SelfDidNotWin,
        VictoryBanner::SelfLost,
        VictoryBanner::Tie,
    ]
    .iter()
    .map(|b| b.text())
    .collect();
    v.sort_unstable();
    v.dedup();
    assert_eq!(v.len(), VICTORY_TEXTS.len());
}

/// 节点名清单与二进制里的字面量一致（长度也一致，长度由 `mov $0xNN,%r8d` 给出）。
#[test]
fn node_name_lengths_match_rodata() {
    for n in PLAY_WIDGET_NODES.iter().chain(REPLAY_WIDGET_NODES.iter()) {
        assert!(!n.is_empty());
    }
    assert_eq!(PLAY_WIDGET_NODES[0].len(), 14); // 0x1bf8c7
    assert_eq!(PLAY_WIDGET_NODES[1].len(), 15); // 0x1bf8d5
    assert_eq!(PLAY_WIDGET_NODES[2].len(), 13); // 0x1bf8e4
    assert_eq!(REPLAY_WIDGET_NODES[0].len(), 22); // 0x1bf8f1
    assert_eq!(REPLAY_WIDGET_NODES[1].len(), 23); // 0x1bf907
    assert_eq!(CONNECT_NODE.len(), 18); // 0x1bf9a7，长度立即数 0x12
    assert_eq!(ACTION_NODES[0].len(), 10); // 0x1c651d 「TimerLabel」
    assert_eq!(ACTION_NODES[1].len(), 18); // 0x1c6527 「ViewFunctionButton」
}

/// 函数地址常量（vaddr，不是 Ghidra 地址）自校验：
/// 全部落在 `.text`（`0x37b940 … 0x94eb84`）内，且都出现在 `fixtures/module_funcs.tsv`
/// 的 `graphwar_play_screen_control.rs` 行里。
#[test]
fn module_function_addresses_are_in_text() {
    const TEXT_LO: u64 = 0x37b940;
    const TEXT_HI: u64 = 0x37b940 + 0x5d3244;
    for a in [
        FN_READY_LABELS,
        FN_SET_WIDGETS_A,
        FN_SET_WIDGETS_B,
        FN_SHOW_VICTORY,
        FN_CONNECT,
    ] {
        assert!(a >= TEXT_LO && a < TEXT_HI, "{a:#x} 不在 .text 内");
    }
    // 这几个函数都是 16 字节对齐的（与 funcstarts 一致）。
    for a in [FN_SET_WIDGETS_A, FN_SET_WIDGETS_B, FN_CONNECT] {
        assert_eq!(a % 16, 0, "{a:#x} 未对齐");
    }
}

/// 唯一的 panic 行号：`src/graphwar_play_screen_control.rs:561`。
#[test]
fn panic_location_is_line_561() {
    assert_eq!(PANIC_LOCATION, ("src/graphwar_play_screen_control.rs", 561));
}
