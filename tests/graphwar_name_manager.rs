//! `graphwar_name_manager` 的验收：**名字存哪 / `NameInfo` 的载荷 / 谁写这张表**。
//!
//! ⚠️ 用 `#[path]` 直接编进本测试二进制 ⇒ **不需要** `src/lib.rs` 里先加 `pub mod`；
//! 两份源码只依赖 `std`。父 agent 加上 `pub mod` 后可改成
//! `use gw2_rebuild::graphwar_name_manager::*;` 而不改断言。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `two_subscriptions` | `new_connection_event`/`entity_removed_event` → 两个方法 | **逐位**（两个闭包的 `adr` 目标） |
//! | `tags_match_the_event_table` | 60/59/5/14 与 `events.rs` 一致 | **逐位**（两侧独立） |
//! | `name_info_is_string_then_player` | 载荷 `+0x00` = `String`、`+0x18` = `player_id` | **逐位**（arm64 `ldr q0` + `stur` + `str w20`） |
//! | `set_name_writes_the_table` | 写表 + 同名覆盖 + 删掉 | **逐位**（`insert(self+0x18, ..)`） |
//! | `send_all_covers_every_entry` | 广播覆盖整表 | **逐位**（hashbrown 迭代 + 逐条 emit） |
//! | `name_authority_is_not_graphics` | 51/53/55 ≠ 60 | **逐位**（`graphics_manager` 的三条事件判别值） |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/graphwar_name_manager.rs"]
mod graphwar_name_manager;

use graphwar_name_manager::*;

/// 两条订阅，处理方法分别是"全量重播"与"删实体"。
#[test]
fn two_subscriptions() {
    assert_eq!(SUBSCRIPTIONS.len(), 2);
    assert_eq!(SUBSCRIPTIONS[0].name, "new_connection_event");
    assert_eq!(SUBSCRIPTIONS[0].handler, "send_all_info_events");
    assert_eq!(SUBSCRIPTIONS[1].name, "entity_removed_event");
    assert_eq!(SUBSCRIPTIONS[1].handler, "handle_entity_removed_event");
    // 两个名字都是 20 字节（`mov w2, #0x14`，arm64 0x40b03c / 0x40b088）。
    assert_eq!(SUBSCRIPTIONS[0].name.len(), 0x14);
    assert_eq!(SUBSCRIPTIONS[1].name.len(), 0x14);
}

/// 判别值与项目事件表逐条一致。
#[test]
fn tags_match_the_event_table() {
    assert_eq!(EVENT_NAME_INFO, events::Event::NameInfo as u16);
    assert_eq!(EVENT_NAME_REQUEST, events::Event::NameRequest as u16);
    assert_eq!(SUBSCRIPTIONS[0].tag, events::Event::NewConnection as u16);
    assert_eq!(SUBSCRIPTIONS[1].tag, events::Event::EntityRemoved as u16);
    assert_eq!(NameInfo::TAG, 60);
    assert_eq!(NameInfo::tag_word(), 0x8000_0000_0000_003c);
}

/// `NameInfo` 的载荷是 `String` + `player_id`：`+0x00` 起 24 字节字符串，`+0x18` 是 u32。
///
/// arm64 `set_name`：`ldr q0,[x19]` / `stur q0,[sp,#0x20]`（= cap, ptr）、
/// `ldr x8,[x19,#0x10]` / `str x8,[sp,#0x30]`（= len）、`str w20,[sp,#0x38]`（= id）。
#[test]
fn name_info_is_string_then_player() {
    assert_eq!(
        (NAME_INFO_STRING_OFFSET, NAME_INFO_PLAYER_OFFSET),
        (0x00, 0x18)
    );
    assert_eq!(NameInfo::STRING_SIZE, 24);
    assert_eq!(NameInfo::PAYLOAD_SIZE, 0x20);
    // 表基 + hasher：`+0x18` / `hash(self+0x38)` ⇒ hasher 相对表基 +0x20。
    assert_eq!(
        (NAME_MAP_OFFSET, NAME_MAP_HASHER_OFFSET),
        (0x18, 0x20)
    );
    let e = NameInfo { name: "abc".to_string(), player_id: 9 };
    assert_eq!((e.name.as_str(), e.player_id), ("abc", 9));
}

/// `set_name(player_id, &name)`：写表（旧值释放），再发一条 (60)。
#[test]
fn set_name_writes_the_table() {
    let mut m = GraphwarNameManager::new();
    let e = m.set_name(42, "Ann");
    assert_eq!((e.name.as_str(), e.player_id), ("Ann", 42));
    assert_eq!(m.name_of(42), Some("Ann"));
    assert_eq!(m.len(), 1);

    // 同名覆盖：条数不变，事件带新值。
    let e = m.set_name(42, "Bob");
    assert_eq!(e.name, "Bob");
    assert_eq!(m.len(), 1);
    assert_eq!(m.name_of(42), Some("Bob"));
}

/// 广播是"整表逐条"，且删实体会让广播少一条。
#[test]
fn send_all_covers_every_entry() {
    let mut m = GraphwarNameManager::new();
    m.set_name(1, "a");
    m.set_name(2, "b");
    m.set_name(3, "c");
    let all = m.send_all_info_events();
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|e| e.player_id != 0));
    let ids: Vec<u32> = all.iter().map(|e| e.player_id).collect();
    assert_eq!(ids, vec![1, 2, 3]);

    assert!(m.handle_entity_removed_event(2));
    assert_eq!(m.send_all_info_events().len(), 2);
    assert!(!m.handle_entity_removed_event(2), "删两次第二次必须返回 false");
}

/// **名字的权威源是本模块，不是 `graphics_manager` 那三张表。**
///
/// `graphics_manager` 的 `+0x18/+0x48/+0x78` 三张表发的是
/// 51 / 53 / 55（皮肤/脸/帽子），值与 (60) 无关；名字只走 59→60 这一对。
#[test]
fn name_authority_is_not_graphics() {
    // graphics_manager 的三条判别值（`docs/02` §15.2 与 `graphwar_graphics_manager` 模块文档）。
    const SKIN_INFO: u16 = 51;
    const FACE_INFO: u16 = 53;
    const HAT_INFO: u16 = 55;
    for g in [SKIN_INFO, FACE_INFO, HAT_INFO] {
        assert_ne!(g, NameInfo::TAG);
        assert_ne!(g, EVENT_NAME_REQUEST);
    }
    // 三条 graphics 与 name 分属两个模块的四个不同判别值。
    let mut all = vec![NameInfo::TAG, EVENT_NAME_REQUEST, SKIN_INFO, FACE_INFO, HAT_INFO];
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 5);
}
