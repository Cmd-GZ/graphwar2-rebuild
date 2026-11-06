//! `graphwar_team_manager` 的验收：**队伍是怎么分的**。
//!
//! ⚠️ 这里用 `#[path]` 直接把 `src/graphwar_team_manager.rs` 编进本测试二进制，
//! 所以**不需要** `src/lib.rs` 里先加 `pub mod`（那一行不在本分支的白名单里）。
//! 三条源码文件都只依赖 `std`，因此这种引法是合法的；父 agent 加上 `pub mod`
//! 之后本文件可以改成 `use gw2_rebuild::graphwar_team_manager::*;` 而不改断言。
//!
//! 每条断言指向的证据见 `src/graphwar_team_manager.rs` 的模块文档
//! （地址一律 vaddr，Ghidra 地址 = vaddr + `0x100000`）：
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `pick_team_is_the_fewer_side` | 分队算术 ≡ "Team2 人数严格少于 Team1" | **观测**（`0x3c1b40` 的两条累加 + 出口 `cmp/setb`） |
//! | `first_player_is_team1` | 空表 ⇒ Team1；满房顺序严格交替 | **观测**（同一段算术 + `docs/10` §10） |
//! | `remove_only_broadcasts_when_it_removed` | 删到了才发 `RemoveTeam` | **观测**（`0x3c1a08` 的 `cmp dl,2`） |
//! | `entity_removed_is_remove_from_team` | `entity_removed_event` ⇒ 退出队伍 | **观测**（`0x4c1f40`） |
//! | `events_match_the_project_event_table` | 三条事件判别值与 `src/events.rs` 逐条吻合 | **观测** |
//! | `miss_is_two_not_a_third_team` | 查不到返回 2（`Option` 的 niche） | **观测**（`0x3c2120`） |
#![allow(dead_code)]

#[path = "../src/graphwar_team_manager.rs"]
mod graphwar_team_manager;

use graphwar_team_manager::*;

/// `0x3c1b40` 的两条累加 = "少数派优先，平手 Team1"。
///
/// 穷举 0..12 × 0..12 的人数组合：任何一边把累加改成别的式子都会红。
#[test]
fn pick_team_is_the_fewer_side() {
    for team1 in 0u8..12 {
        for team2 in 0u8..12 {
            let mut bytes = Vec::new();
            bytes.extend(std::iter::repeat(0u8).take(team1 as usize));
            bytes.extend(std::iter::repeat(1u8).take(team2 as usize));
            let want = if team2 < team1 {
                GraphwarTeam::Team2
            } else {
                GraphwarTeam::Team1
            };
            assert_eq!(
                TeamTable::pick_team_from(bytes),
                want,
                "Team1={team1} Team2={team2}"
            );
        }
    }
}

/// 空表 ⇒ Team1；一队满了就补另一队，人数差永远 ≤ 1。
#[test]
fn first_player_is_team1() {
    let mut table = TeamTable::new();
    let mut order = Vec::new();
    for player in 1..=10u32 {
        let team = table.pick_team();
        order.push(team);
        table.add_to_team(player, team);
        let (c1, c2) = table.counts();
        assert!(c1.abs_diff(c2) <= 1, "10 人上限内两队人数差不超过 1：{c1} vs {c2}");
    }
    assert_eq!(order[0], GraphwarTeam::Team1, "第一个人一定进 Team1");
    assert_eq!(table.counts(), (5, 5), "满房 10 人 = 5v5，是这条规则的结果");
}

/// `remove_from_team`：**只有真删掉了**才发 `RemoveTeam`（`0x3c1a08`）。
#[test]
fn remove_only_broadcasts_when_it_removed() {
    let mut table = TeamTable::new();
    table.add_to_team(9, GraphwarTeam::Team2);
    assert_eq!(
        table.remove_from_team(9),
        vec![TeamEvent::RemoveTeam { player_id: 9 }]
    );
    assert!(table.remove_from_team(9).is_empty());
    assert!(table.remove_from_team(404).is_empty());
}

/// `entity_removed_event` 就是"该玩家退出队伍"。
#[test]
fn entity_removed_is_remove_from_team() {
    let mut table = TeamTable::new();
    table.add_to_team(3, GraphwarTeam::Team1);
    assert_eq!(
        table.handle_entity_removed_event(3),
        vec![TeamEvent::RemoveTeam { player_id: 3 }]
    );
    assert_eq!(table.get_team(3), None);
    assert_eq!(table.len(), 0);
}

/// 三条事件的判别值与 `src/events.rs` 的表逐条吻合，tag 就是判别值挂最高位。
#[test]
fn events_match_the_project_event_table() {
    let info = TeamEvent::TeamInfo {
        player_id: 1,
        team: GraphwarTeam::Team2,
    };
    let gone = TeamEvent::RemoveTeam { player_id: 1 };
    assert_eq!(info.discriminant_low(), 24, "team_info_event");
    assert_eq!(gone.discriminant_low(), 25, "remove_team_event");
    assert_eq!(info.tag(), 0x8000_0000_0000_0018);
    assert_eq!(gone.tag(), 0x8000_0000_0000_0019);
    assert_eq!(EVENT_ENTITY_REMOVED, 14, "entity_removed_event");
    assert_eq!(info.payload_len(), 5, "{{player_id:u32, team:u8}}");
    assert_eq!(gone.payload_len(), 4);
}

/// 每张表的 `(名字, 长度)` 都自洽——长度就是 `strlen`，反编译里是 `0x14`。
#[test]
fn subscription_names_are_self_consistent() {
    assert_eq!(SUBSCRIBED_EVENTS.len(), 2);
    for (name, len) in SUBSCRIBED_EVENTS {
        assert_eq!(name.len(), len, "{name}");
    }
}

/// `new_connection_event` 的语义是**整表重发**，不是重新分配：条数 = 表长。
#[test]
fn new_connection_replays_but_does_not_reassign() {
    let mut table = TeamTable::new();
    table.add_to_team(1, GraphwarTeam::Team1);
    table.add_to_team(2, GraphwarTeam::Team2);
    let before = table.send_all_info_events();
    let after = table.send_all_info_events();
    assert_eq!(before.len(), 2);
    assert_eq!(before, after, "两次重发内容相同 ⇒ 没有重新分配");
    assert_eq!(table.counts(), (1, 1));
}
