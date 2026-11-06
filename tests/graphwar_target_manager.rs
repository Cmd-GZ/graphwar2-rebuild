//! `graphwar_target_manager` 的验收：**靶子表里没有「复原」这件事**，
//! 复原的判定点在 `queue_end_turn_trigger` 的闭包里。
//!
//! ⚠️ 这里用 `#[path]` 直接把 `src/graphwar_target_manager.rs` 与 `src/events.rs`
//! 编进本测试二进制，所以**不需要** `src/lib.rs` 里先加 `pub mod`。
//! 两份源码都只依赖 `std`，因此这种引法是合法的；父 agent 加上 `pub mod` 之后
//! 本文件可以改成 `use gw2_rebuild::graphwar_target_manager::*;` 而不改断言。
//!
//! 每条断言指向的证据见 `src/graphwar_target_manager.rs` 的模块文档
//! （地址一律 vaddr，Ghidra 地址 = vaddr + `0x100000`）：
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `two_subscriptions_are_the_20_byte_names` | 两条订阅名与长度（20/20） | **逐位**（`0x3dae34` 的两个 `mov w2,#0x14`） |
//! | `tags_match_the_event_table` | 14 = `EntityRemoved`、23 = `TargetInfo` | **逐位**（`movk #0x8000,lsl#48` 立即数 + `events.rs`） |
//! | `register_then_rebroadcast` | 重播 = 每个 id 一条 `TargetInfo` | **逐位**（`0x3da9bc` 的循环体） |
//! | `removal_needs_tag_14` | 判别值不符直接 `Err`（原版 `panic!()`） | **逐位**（`0x3dad08` 的 `subs/ b.ne`） |
//! | `removed_target_is_not_rebroadcast` | 删号后重播里没有它 | **逐位**（`RawTable::remove_entry`） |
//! | `no_alive_state_means_reset_must_live_elsewhere` | 本模块**没有** alive 字段 | **逐位**（桶距 4 + 值尾随 0 字节） |
//! | `reset_condition_truth_table` | 复原的三个条件（`0x4abc80`–`0x4abc94`） | **观测** |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/graphwar_target_manager.rs"]
mod graphwar_target_manager;

use events::Event;
use graphwar_target_manager::*;

#[test]
fn two_subscriptions_are_the_20_byte_names() {
    assert_eq!(SUBSCRIPTIONS.len(), 2);
    assert_eq!(SUBSCRIPTIONS[0], "new_connection_event");
    assert_eq!(SUBSCRIPTIONS[1], "entity_removed_event");
    // 两条都是 20 字节：`new` 里两次 `mov w2, #0x14`。
    for s in SUBSCRIPTIONS {
        assert_eq!(s.len(), 20, "{} 的长度不是 20", s);
    }

    let mgr = GraphwarTargetManager::new();
    assert_eq!(mgr.subscribed, SUBSCRIPTIONS.to_vec());
}

#[test]
fn tags_match_the_event_table() {
    assert_eq!(ENTITY_REMOVED_TAG, 14);
    assert_eq!(TARGET_INFO_TAG, 23);
    assert_eq!(Event::EntityRemoved.wire_tag(), ENTITY_REMOVED_TAG);
    assert_eq!(Event::TargetInfo.wire_tag(), TARGET_INFO_TAG);
    // 分派名字也要对得上（`0x3d38f0` 的表）。
    assert_eq!(
        Event::from_dispatch_name("target_info_event"),
        Some(Event::TargetInfo)
    );
}

#[test]
fn register_then_rebroadcast() {
    let mut mgr = GraphwarTargetManager::new();
    for id in [1099u32, 1100, 1101] {
        mgr.add_target(id);
    }
    assert_eq!(mgr.len(), 3);

    let sent = mgr.send_all_info_events();
    assert_eq!(sent.len(), 3);
    // 每个 id 一条 `TargetInfo`（判别值 23）。
    assert_eq!(mgr.outbox.len(), 3);
    assert!(mgr.outbox.iter().all(|e| *e == Event::TargetInfo));

    let mut got = sent;
    got.sort_unstable();
    assert_eq!(got, vec![1099, 1100, 1101]);
}

#[test]
fn removal_needs_tag_14() {
    let mut mgr = GraphwarTargetManager::new();
    mgr.add_target(7);

    // 判别值不是 14 ⇒ 原版走 `panic!()`（串 `0x19be09` = `explicit panic`，长度 14）。
    assert_eq!(mgr.handle_entity_removed_event(23, 7), Err(()));
    assert!(mgr.contains(7), "判别值不符时不该动表");

    assert_eq!(mgr.handle_entity_removed_event(14, 7), Ok(()));
    assert!(!mgr.contains(7));
    // 删不存在的号不报错（`remove_entry` 找不到就不动）。
    assert_eq!(mgr.handle_entity_removed_event(14, 7), Ok(()));
}

#[test]
fn removed_target_is_not_rebroadcast() {
    let mut mgr = GraphwarTargetManager::new();
    mgr.add_target(1);
    mgr.add_target(2);
    let _ = mgr.handle_entity_removed_event(14, 1);
    let sent = mgr.send_all_info_events();
    assert_eq!(sent, vec![2]);
}

/// 本模块**没有** alive 概念：`add_target` 只写 id，`send_all_info_events` 只读 id。
///
/// 所以「部分命中 ⇒ 全部复原」**不可能**由本模块实现 —— 它连谁死了都不知道。
/// 全库构造 `LifeInfo`(27) 的位置只有 `life_manager::send_all_info_events`
/// 与 `life_manager::set_alive` 两处。
#[test]
fn no_alive_state_means_reset_must_live_elsewhere() {
    let mut mgr = GraphwarTargetManager::new();
    mgr.add_target(5);
    // 唯一的「状态」就是 id 在不在集合里。
    assert!(mgr.contains(5));
    assert_eq!(mgr.len(), 1);
    // 没有任何 API 能表达「这个靶子现在死了但保留」：表里只有 id。
    // （`should_reset_targets` 是**另一个**模块里那段内联判断的投影，不是本表的状态。）
    assert!(!src_decld_alive(), "表结构里不该出现 alive 字段");
}

/// 编译期论证：本模块的公开 API 里没有任何以 alive 命名的方法。
fn src_decld_alive() -> bool {
    let src = include_str!("../src/graphwar_target_manager.rs");
    // 只看函数签名行（`pub fn ...`），避免把文档里的说明算进来。
    src.lines()
        .filter(|l| l.trim_start().starts_with("pub fn "))
        .any(|l| l.contains("alive"))
}

/// 复原判定 = 三个条件的合取（arm64 `0x4abc80`–`0x4abc94`）。
#[test]
fn reset_condition_truth_table() {
    // 偶数 + 两队都活着 + 标志位 1 ⇒ 复原
    assert!(should_reset_targets(0, true, true, 1));
    assert!(should_reset_targets(2, true, true, 1));
    // 任一条不满足 ⇒ 不复原
    assert!(!should_reset_targets(1, true, true, 1), "w23 奇数");
    assert!(!should_reset_targets(0, false, true, 1), "Team1 全灭");
    assert!(!should_reset_targets(0, true, false, 1), "Team2 全灭");
    assert!(!should_reset_targets(0, true, true, 0), "标志位不是 1");
    assert!(!should_reset_targets(0, true, true, 2), "标志位不是 1");
}
