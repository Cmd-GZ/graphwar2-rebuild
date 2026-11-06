//! `graphwar_player_manager` 的验收：**4 个方法（不是 3 个） / `PlayerInfo` 的载荷 / 级联 (6)**。
//!
//! ⚠️ 用 `#[path]` 直接编进本测试二进制 ⇒ **不需要** `src/lib.rs` 里先加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `four_methods_not_three` | 模块确实有 4 个方法（`add_player` 是第 4 个） | **逐位**（arm64 `.symtab` 独立符号 `0x49ce1c`） |
//! | `two_subscriptions` | `new_connection_event`/`entity_removed_event` → 两个方法 | **逐位**（闭包 `adr` 目标） |
//! | `player_info_is_two_u32` | 载荷 `+0x08` = `player_id`、`+0x0c` = `connection_id` | **逐位**（`stp w20,w19` + 全表拷贝 `ldur d0`） |
//! | `add_player_writes_the_table` | key = `player_id`、value = `connection_id` | **逐位**（`insert(self+0x18, w1, w2)`） |
//! | `cascade_only_when_connection_is_empty` | 该连接最后一人被删 ⇒ 发 (6) | **逐位**（`cmpq $0, len` + 事件 `+0x20`） |
//! | `tags_match_the_event_table` | 13/6/5/14 与 `events.rs` 一致 | **逐位** |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/graphwar_player_manager.rs"]
mod graphwar_player_manager;

use graphwar_player_manager::*;

/// 模块有 4 个方法：`new` / `add_player` / `send_all_info_events` / `handle_entity_removed_event`。
///
/// `docs/02` §18.1 只列了 3 个 —— 因为 `add_player` 在 x86 侧被内联进
/// `remote_event_handler`（`panic::Location` `:66:26` / `:74:39` 落在宿主函数体内）。
#[test]
fn four_methods_not_three() {
    // 四个方法都在这里被点名（编译期即证明它们存在）。
    let mut m = GraphwarPlayerManager::new();
    let e = m.add_player(1, 2);
    let all = m.send_all_info_events();
    assert_eq!(all.len(), 1);
    let casc = m.handle_entity_removed_event(1);
    assert_eq!(e.player_id, 1);
    assert_eq!(m.send_all_info_events().len(), 0, "删完再广播就是 0 条");
    assert_eq!(casc, Some(ConnectionRemoved { connection_id: 2 }));
}

/// 两条订阅，处理方法分别是"全量重播"与"删实体"。
#[test]
fn two_subscriptions() {
    assert_eq!(SUBSCRIPTIONS.len(), 2);
    assert_eq!(SUBSCRIPTIONS[0].name, "new_connection_event");
    assert_eq!(SUBSCRIPTIONS[0].handler, "send_all_info_events");
    assert_eq!(SUBSCRIPTIONS[1].name, "entity_removed_event");
    assert_eq!(SUBSCRIPTIONS[1].handler, "handle_entity_removed_event");
    assert_eq!(SUBSCRIPTIONS[0].name.len(), 0x14);
    assert_eq!(SUBSCRIPTIONS[1].name.len(), 0x14);
}

/// `PlayerInfo`(13) 的载荷是**两个 u32**，`player_id` 在前。
#[test]
fn player_info_is_two_u32() {
    assert_eq!(PlayerInfo::PAYLOAD_SIZE, 8);
    assert_eq!(
        (PLAYER_INFO_PLAYER_OFFSET, PLAYER_INFO_CONNECTION_OFFSET),
        (0x08, 0x0c)
    );
    assert_eq!(PlayerInfo::tag_word(), 0x8000_0000_0000_000d);
    // 一张表：`hash(self+0x38)` ⇒ hasher 相对表基 +0x20。
    assert_eq!((PLAYER_MAP_OFFSET, PLAYER_MAP_HASHER_OFFSET), (0x18, 0x20));
}

/// `add_player` 写的是 `player_id → connection_id`，并向 (13) 广播同两个数。
#[test]
fn add_player_writes_the_table() {
    let mut m = GraphwarPlayerManager::new();
    let e = m.add_player(7, 100);
    assert_eq!((e.player_id, e.connection_id), (7, 100));
    assert_eq!(m.connection_of(7), Some(100));

    let all = m.send_all_info_events();
    assert_eq!(all, vec![PlayerInfo { player_id: 7, connection_id: 100 }]);
}

/// 级联：**该连接的最后一个人**被删掉时才发 `ConnectionRemoved`(6)。
#[test]
fn cascade_only_when_connection_is_empty() {
    let mut m = GraphwarPlayerManager::new();
    m.add_player(1, 100);
    m.add_player(2, 100);
    m.add_player(3, 200);

    assert_eq!(m.players_of_connection(100), vec![1, 2]);
    assert_eq!(m.handle_entity_removed_event(1), None, "同连接还有人 ⇒ 不级联");
    assert_eq!(
        m.handle_entity_removed_event(2),
        Some(ConnectionRemoved { connection_id: 100 }),
        "最后一人 ⇒ 级联"
    );
    assert_eq!(m.handle_entity_removed_event(3), Some(ConnectionRemoved { connection_id: 200 }));
    assert_eq!(m.handle_entity_removed_event(3), None, "已经没了 ⇒ 什么都不发");
}

/// 判别值与项目事件表逐条一致；级联事件是 (6)。
#[test]
fn tags_match_the_event_table() {
    assert_eq!(EVENT_PLAYER_INFO, events::Event::PlayerInfo as u16);
    assert_eq!(EVENT_CONNECTION_REMOVED, events::Event::ConnectionRemoved as u16);
    assert_eq!(SUBSCRIPTIONS[0].tag, events::Event::NewConnection as u16);
    assert_eq!(SUBSCRIPTIONS[1].tag, events::Event::EntityRemoved as u16);
    assert_eq!(ConnectionRemoved::tag_word(), 0x8000_0000_0000_0006);
    // 级联事件里 `connection_id` 的位置（消费者 `game_state_manager` 读 `+0x20`）。
    assert_eq!(CONNECTION_REMOVED_ID_OFFSET, 0x20);
    assert_eq!(ConnectionRemoved::NONE_MARKER, 0x8000_0000_0000_0000);
}
