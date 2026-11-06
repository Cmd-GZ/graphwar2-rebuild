//! `graphwar_rank_manager` 的验收：**段位从哪来 / `RankInfo` 的载荷形状**。
//!
//! ⚠️ 用 `#[path]` 直接编进本测试二进制 ⇒ **不需要** `src/lib.rs` 里先加 `pub mod`；
//! 两份源码只依赖 `std`。父 agent 加上 `pub mod` 后可改成
//! `use gw2_rebuild::graphwar_rank_manager::*;` 而不改断言。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `two_subscriptions` | `new_connection_event`/`entity_removed_event` → 两个方法 | **逐位**（两个闭包的 `adr` 目标） |
//! | `tags_match_the_event_table` | 67/66/5/14 与 `events.rs` 一致 | **逐位**（两侧独立） |
//! | `rank_info_is_flag_value_player` | 载荷 `+8/+0xc/+0x10` 三个 dword | **逐位**（arm64 `stp`+`str` 与 x86 三处 `mov` 互校） |
//! | `set_rank_writes_or_removes` | `op == 1` 写入，否则撤销，两条路都发一条 | **逐位**（`cmp w2,#1`） |
//! | `send_all_sets_flag_to_one` | 广播时 `+8` 恒为 1 | **逐位**（x86 `mov dword [rsp+8], 1`） |
//! | `one_map_not_two` | 只有一张表（`self+0x18`，hasher `+0x38`） | **逐位**（`hash(self+0x38)`，docs/112 §5.3 修正） |
//! | `module_emits_67_not_66` | 本模块只发 67 | **观测**（4 个函数里没有 tag 66 的构造） |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/graphwar_rank_manager.rs"]
mod graphwar_rank_manager;

use graphwar_rank_manager::*;

/// 两条订阅，处理方法分别是"全量重播"与"删实体"。
#[test]
fn two_subscriptions() {
    assert_eq!(SUBSCRIPTIONS.len(), 2);
    assert_eq!(SUBSCRIPTIONS[0].name, "new_connection_event");
    assert_eq!(SUBSCRIPTIONS[0].handler, "send_all_info_events");
    assert_eq!(SUBSCRIPTIONS[1].name, "entity_removed_event");
    assert_eq!(SUBSCRIPTIONS[1].handler, "handle_entity_removed_event");
    // 两条名字都是 20 字节（`mov w2, #0x14`）。
    assert_eq!(SUBSCRIPTIONS[0].name.len(), 0x14);
    assert_eq!(SUBSCRIPTIONS[1].name.len(), 0x14);
}

/// 判别值与项目事件表逐条一致。
#[test]
fn tags_match_the_event_table() {
    assert_eq!(EVENT_RANK_INFO, events::Event::RankInfo as u16);
    assert_eq!(EVENT_RANK_REQUEST, events::Event::RankRequest as u16);
    assert_eq!(SUBSCRIPTIONS[0].tag, events::Event::NewConnection as u16);
    assert_eq!(SUBSCRIPTIONS[1].tag, events::Event::EntityRemoved as u16);
    assert_eq!(RankInfo::TAG, 67);
    assert_eq!(RankInfo::tag_word(), 0x8000_0000_0000_0043);
}

/// `RankInfo` 的载荷是 **三个 dword**：`+0x08` 标志、`+0x0c` 段位、`+0x10` 玩家 id。
///
/// arm64 `set_rank`：`stp w21,w19,[sp,#0x10]` + `str w20,[sp,#0x18]`（载荷基址 `sp+8`）；
/// x86 同一函数：`[rsp+0x18]=edx` / `[rsp+0x1c]=ecx` / `[rsp+0x20]=esi`（载荷基址 `rsp+0x10`）。
/// 两条编码给出的相对偏移完全相同。
#[test]
fn rank_info_is_flag_value_player() {
    assert_eq!(
        (RANK_INFO_FLAG_OFFSET, RANK_INFO_VALUE_OFFSET, RANK_INFO_PLAYER_OFFSET),
        (0x08, 0x0c, 0x10)
    );
    assert_eq!(RankInfo::PAYLOAD_SIZE, 12);

    let e = RankInfo { flag: 1, rank: 17, player_id: 42 };
    assert_eq!(e.triple(), (1, 17, 42));
}

/// `set_rank(player_id, op, rank)`：`op == 1` 写入，否则撤销；**两条路都发一条 (67)**。
#[test]
fn set_rank_writes_or_removes() {
    let mut m = GraphwarRankManager::new();

    let e = m.set_rank(42, SET_RANK_INSERT, 17);
    assert_eq!(e.triple(), (1, 17, 42), "写入路：flag=1、value=rank、player=id");
    assert_eq!(m.rank_of(42), Some(17));
    assert_eq!(m.len(), 1);

    // 覆盖写。
    m.set_rank(42, SET_RANK_INSERT, 18);
    assert_eq!(m.rank_of(42), Some(18));
    assert_eq!(m.len(), 1);

    // 撤销路：表里删掉，但事件照样发（flag = 传进来的 op）。
    let e = m.set_rank(42, 0, 18);
    assert_eq!(e.triple(), (0, 18, 42));
    assert_eq!(m.rank_of(42), None);
    assert!(m.is_empty());

    // 撤销一个不存在的玩家也不崩。
    let e = m.set_rank(99, 0, 0);
    assert_eq!(e.player_id, 99);
    assert!(m.is_empty());
}

/// 广播：每条都是 `flag = 1`，其余两个字段来自表。
#[test]
fn send_all_sets_flag_to_one() {
    let mut m = GraphwarRankManager::new();
    m.set_rank(1, SET_RANK_INSERT, 5);
    m.set_rank(2, SET_RANK_INSERT, 9);

    let all = m.send_all_info_events();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|i| i.flag == RANK_INFO_FLAG_SET));
    assert_eq!(all[0].triple(), (1, 5, 1));
    assert_eq!(all[1].triple(), (1, 9, 2));

    // 空表广播出空向量。
    assert!(GraphwarRankManager::new().send_all_info_events().is_empty());
}

/// 只有**一张**表：`self + 0x18`，hasher 在表基 + 0x20（= `self + 0x38`）。
///
/// `docs/112` §5.3 读成"两张 map"，是因为把 `find(self+0x38, &key)` 里的
/// **hasher 指针**当成了第二张表；arm64 `add x0, x22, #0x38 ; bl <hash>` 直接否掉那种读法。
#[test]
fn one_map_not_two() {
    assert_eq!(RANK_MAP_OFFSET, 0x18);
    assert_eq!(RANK_MAP_HASHER_OFFSET, 0x20);
    assert_eq!(RANK_MAP_OFFSET + RANK_MAP_HASHER_OFFSET, 0x38);
}

/// 删实体：删到了返回 true，没删到返回 false。
#[test]
fn entity_removed_drops_one() {
    let mut m = GraphwarRankManager::new();
    m.set_rank(7, SET_RANK_INSERT, 3);
    assert!(m.handle_entity_removed_event(7));
    assert!(!m.handle_entity_removed_event(7));
    assert!(m.is_empty());
}

/// 本模块**只发 67**，不发起 66；而且里面没有任何"算段位"的算术。
#[test]
fn module_emits_67_not_66() {
    assert_ne!(EVENT_RANK_INFO, EVENT_RANK_REQUEST);
    assert_eq!(EVENT_RANK_INFO, 67);
    assert_eq!(EVENT_RANK_REQUEST, 66);

    // 结构性的负结果：唯一的"分支"是 op 与 1 的比较，表里存的是外部给的值，
    // 不做任何换算（`set_rank(.., op, rank)` 原样入表）。
    let mut m = GraphwarRankManager::new();
    for (id, rank) in [(1u32, 0u32), (2, 21), (3, 1000)] {
        m.set_rank(id, SET_RANK_INSERT, rank);
    }
    assert_eq!(m.rank_of(1), Some(0));
    assert_eq!(m.rank_of(2), Some(21));
    assert_eq!(m.rank_of(3), Some(1000), "值不被截断/换算 ⇒ 换算不在本模块");
}
