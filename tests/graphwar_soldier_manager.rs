//! `graphwar_soldier_manager` 的验收：**士兵与队伍/轮转是什么关系**。
//!
//! ⚠️ 用 `#[path]` 直接把 `src/graphwar_soldier_manager.rs` 编进本测试二进制，
//! 这样本分支**不需要**改 `src/lib.rs`。证据见该文件模块文档。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `one_row_per_soldier_two_columns` | 表是 `HashMap<u32, u32>`（桶步长 8） | **观测**（`0x555570` 每次搬 8 字节） |
//! | `owner_removed_cascades_entity_removed` | 玩家没了 ⇒ 他的士兵各发一条 `entity_removed` | **观测**（`0x444fd0` 比的是 value、发的是 key） |
//! | `cascade_cannot_self_ignite` | 级联事件回来不会二次触发 | **推断**（读代码，无运行时验证） |
//! | `cascade_does_not_erase_rows` | 级联里没有 `erase` | **观测**（`0x444fd0` 只有发送） |
//! | `soldier_info_is_the_raw_bucket` | 重发时载荷 = 表里那一对 | **观测** |
#![allow(dead_code)]

#[path = "../src/graphwar_soldier_manager.rs"]
mod graphwar_soldier_manager;

use graphwar_soldier_manager::*;

/// 一条士兵一行（键=士兵、值=主人），一个主人可以有多条。
#[test]
fn one_row_per_soldier_two_columns() {
    let mut table = SoldierTable::new();
    for soldier in 10..14u32 {
        table.set_silent(soldier, 1);
    }
    table.set_silent(20, 2);
    assert_eq!(table.get_soldiers_from_owner(1).len(), 4);
    assert_eq!(table.get_soldiers_from_owner(2), vec![20]);
    assert_eq!(table.owner_of(12), Some(1));
    assert_eq!(table.owner_of(99), None);
    assert_eq!(table.len(), 5);
    assert!(!table.is_empty());
}

/// 玩家 1 被删 ⇒ 他名下 4 个士兵各发一条 `entity_removed`，玩家自己不出现。
#[test]
fn owner_removed_cascades_entity_removed() {
    let mut table = SoldierTable::new();
    table.set_silent(10, 1);
    table.set_silent(11, 1);
    table.set_silent(20, 2);
    let events = table.handle_entity_removed_event(1);
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.tag() == TAG_ENTITY_REMOVED));
    assert_eq!(
        events,
        vec![
            SoldierEvent::EntityRemoved { entity_id: 10 },
            SoldierEvent::EntityRemoved { entity_id: 11 },
        ]
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SoldierEvent::EntityRemoved { entity_id } if *entity_id == 1)),
        "级联不会把玩家自己再播一遍"
    );
}

/// 级联事件回来时值不匹配（值仍是玩家号），所以不会自激。
#[test]
fn cascade_cannot_self_ignite() {
    let table = {
        let mut t = SoldierTable::new();
        t.set_silent(10, 1);
        t
    };
    // 拿士兵号当输入：没有谁的主人是 10 ⇒ 什么都不发
    assert!(table.handle_entity_removed_event(10).is_empty());
    // 拿主人号：发一条，但那条事件里的 id 是士兵号，回来时依旧不匹配
    let first = table.handle_entity_removed_event(1);
    assert_eq!(first.len(), 1);
    match first[0] {
        SoldierEvent::EntityRemoved { entity_id } => {
            assert!(table.handle_entity_removed_event(entity_id).is_empty());
        }
        _ => panic!("级联只能发 entity_removed"),
    }
}

/// 级联不发 `erase`：行还在（观测），所以"表变短"不是这条路的语义。
#[test]
fn cascade_does_not_erase_rows() {
    let mut table = SoldierTable::new();
    table.set_silent(10, 1);
    let _ = table.handle_entity_removed_event(1);
    assert_eq!(table.len(), 1);
    assert_eq!(table.owner_of(10), Some(1));
}

/// 新连接进来时的整表重发：一桶一条，载荷就是那一对 `(键, 值)`。
#[test]
fn soldier_info_is_the_raw_bucket() {
    let mut table = SoldierTable::new();
    table.set_silent(5, 1);
    table.set_silent(6, 2);
    let events = table.send_all_info_events();
    assert_eq!(events.len(), 2);
    for event in &events {
        assert_eq!(event.tag(), TAG_SOLDIER_INFO);
        assert_eq!(event.payload_len(), 8);
        match event {
            SoldierEvent::SoldierInfo { first, second } => {
                assert_eq!(table.owner_of(*first), Some(*second), "载荷就是 (键, 值)")
            }
            _ => panic!("重发只能发 soldier_info"),
        }
    }
}

/// 订阅名字自洽（长度 = `strlen`，反编译里两次 `0x14`）。
#[test]
fn subscription_names_are_self_consistent() {
    assert_eq!(SUBSCRIBED_EVENTS.len(), 2);
    for (name, len) in SUBSCRIBED_EVENTS {
        assert_eq!(name.len(), len, "{name}");
    }
    assert_eq!(EVENT_SOLDIER_INFO, 15, "soldier_info_event");
    assert_eq!(EVENT_ENTITY_REMOVED, 14, "entity_removed_event");
}
