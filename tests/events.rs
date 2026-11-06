//! 事件枚举与生命表的验收。

use gw2_rebuild::events::{
    Event, GameInfo, DISPATCH_NAME_TAGS, GAME_INFO_FIELD_COUNT, NAMED_VARIANT_COUNT,
};
use gw2_rebuild::graphwar_life_manager::{LifeState, Status};

/// 名字 → 判别值全表（`docs/114` §3，逐条来自 `0x3d38f0`）。
/// 每行左边是 `.rodata`/立即数里重建出的名字，右边是该分支 `mov $N,%ebx`
/// 的立即数（注释里是相等测试的地址）。
const MEASURED: &[(&str, u16)] = &[
    ("tick_event", 0),                          // 0x3d392a
    ("tick_request_event", 1),                  // 0x3d3c7e
    ("tick_reply_event", 2),                    // 0x3d3f9c
    ("subscribe_event", 3),                     // 0x3d3fd1
    ("new_connection_request_event", 4),        // 0x3d43a0
    ("new_connection_event", 5),                // 0x3d4184
    ("connection_removed_event", 6),            // 0x3d3e3d
    ("connected_to_server_event", 7),           // 0x3d3bcd
    ("new_player_request_event", 8),            // 0x3d3e69
    ("new_bot_request_event", 9),               // 0x3d44c4
    ("bot_added_event", 10),                    // 0x3d3ffc
    ("change_team_request_event", 11),          // 0x3d3bf9
    ("remove_player_request_event", 12),        // 0x3d428b
    ("player_info_event", 13),                  // 0x3d3b12
    ("entity_removed_event", 14),               // 0x3d41b0
    ("soldier_info_event", 15),                 // 0x3d3cad
    ("add_soldier_request_event", 16),          // 0x3d3c25
    ("remove_soldier_request_event", 17),       // 0x3d43cc
    ("axis_mode_change_request_event", 18),     // 0x3d45f7
    ("function_mode_change_request_event", 19), // 0x3d3952（memcmp）
    ("turn_mode_change_request_event", 20),     // 0x3d4623
    ("time_mode_change_request_event", 21),     // 0x3d466d
    ("game_name_request_event", 22),            // 0x3d3a47
    ("target_info_event", 23),                  // 0x3d3b41
    ("team_info_event", 24),                    // 0x3d401d
    ("remove_team_event", 25),                  // 0x3d3b70
    ("pos_info_event", 26),                     // 0x3d3c50
    ("life_info_event", 27),                    // 0x3d4041
    ("life_queued_death_event", 28),            // 0x3d3a73
    ("game_name_info_event", 29),               // 0x3d41dc
    ("game_state_info_event", 30),              // 0x3d44f0
    ("game_start_request_event", 31),           // 0x3d3e95
    ("function_fire_request_event", 32),        // 0x3d42b7
    ("function_fire_event", 33),                // 0x3d40e0
    ("function_update_request_event", 34),      // 0x3d46b8
    ("function_info_event", 35),                // 0x3d410c
    ("function_active_event", 36),              // 0x3d451c
    ("function_points_event", 37),              // 0x3d4548
    ("functions_calculated_event", 38),         // 0x3d397c
    ("clear_obstacles_event", 39),              // 0x3d4574
    ("add_obstacle_event", 40),                 // 0x3d3cdc
    ("destroy_obstacle_event", 41),             // 0x3d446d
    ("queue_destroy_obstacle_event", 42),       // 0x3d43f8
    ("turn_info_event", 43),                    // 0x3d4057
    ("effect_info_event", 44),                  // 0x3d3b9f
    ("axis_mode_info_event", 45),               // 0x3d4208
    ("function_mode_info_event", 46),           // 0x3d3ec1
    ("turn_mode_info_event", 47),               // 0x3d4234
    ("time_mode_info_event", 48),               // 0x3d4260
    ("end_of_turn_info_event", 49),             // 0x3d4499
    ("skin_graphics_request_event", 50),        // 0x3d42e3
    ("skin_graphics_info_event", 51),           // 0x3d3eed
    ("face_graphics_request_event", 52),        // 0x3d430f
    ("face_graphics_info_event", 53),           // 0x3d3f19
    ("hat_graphics_request_event", 54),         // 0x3d39a8
    ("hat_graphics_info_event", 55),            // 0x3d3a9f
    ("entity_effect_info_event", 56),           // 0x3d3f45
    ("turn_limit_info_event", 57),              // 0x3d45a0
    ("turn_count_info_event", 58),              // 0x3d45cc
    ("name_request_event", 59),                 // 0x3d3d0b
    ("name_info_event", 60),                    // 0x3d4071
    ("color_info_event", 61),                   // 0x3d3fb7
    ("game_info_event", 62),                    // 0x3d408b
    ("game_removed_event", 64),                 // 0x3d3d3a（63 缺号）
    ("rank_request_event", 66),                 // 0x3d3d69（65 缺号）
    ("rank_info_event", 67),                    // 0x3d40a5
    ("lock_request_event", 68),                 // 0x3d3d98
    ("lock_info_event", 69),                    // 0x3d40bf
    ("chat_message_request_event", 70),         // 0x3d39d4
    ("chat_message_event", 71),                 // 0x3d3dc7
    ("ban_connection_request_event", 72),       // 0x3d4424
    ("clear_bans_request_event", 73),           // 0x3d3f71
];

#[test]
fn dispatch_name_to_discriminant_is_pinned() {
    // 数量先钉住：多一条少一条都会让下面两条断言失去意义
    assert_eq!(MEASURED.len(), 72, "0x3d38f0 解出 72 条名字");
    assert_eq!(DISPATCH_NAME_TAGS.len(), NAMED_VARIANT_COUNT);
    assert_eq!(MEASURED.len(), NAMED_VARIANT_COUNT);

    for (i, (name, tag)) in MEASURED.iter().enumerate() {
        // 1) 名字 → 变体（0x3d38f0 的做法：比名字，置 ebx）
        let ev = Event::from_dispatch_name(name)
            .unwrap_or_else(|| panic!("名字 {name} 应能反查到变体"));
        assert_eq!(ev.wire_tag(), *tag, "名字 {name} 的判别值应为 {tag}");

        // 2) 源码里的对照表必须与真值表逐条同序同值
        let (n2, e2) = DISPATCH_NAME_TAGS[i];
        assert_eq!(n2, *name, "第 {i} 行的名字顺序与实测表不一致");
        assert_eq!(e2, ev, "第 {i} 行的变体与实测表不一致");
    }
}

#[test]
fn discriminants_are_sparse_and_sorted() {
    // 判别值不是 0..N 的稠密编号：63 / 65 缺号（全 .text 没有 tag 63/65），
    // 74 只出现一次（0x3d3660）且名字未知，不占本枚举的号。
    let tags: Vec<u16> = MEASURED.iter().map(|(_, t)| *t).collect();
    for w in tags.windows(2) {
        assert!(w[0] < w[1], "实测表必须按判别值严格递增：{w:?}");
    }
    assert_eq!(tags[0], 0);
    assert_eq!(*tags.last().unwrap(), 73);
    // 旧版「连续 0..=32」的假设已被证伪：0..=73 这一段只有两个空洞
    let holes: Vec<u16> = (0..=73u16).filter(|n| !tags.contains(n)).collect();
    assert_eq!(holes, vec![63, 65]);
}

#[test]
fn event_variants_are_the_measured_ones() {
    // 抽几条旧编号写错最多的钉子（旧值 → 实测值）
    assert_eq!(Event::PlayerInfo.wire_tag(), 13); // 旧 10
    assert_eq!(Event::EntityRemoved.wire_tag(), 14); // 旧 11
    assert_eq!(Event::SoldierInfo.wire_tag(), 15); // 旧 12
    assert_eq!(Event::FunctionPoints.wire_tag(), 37); // 旧 31
    assert_eq!(Event::FunctionsCalculated.wire_tag(), 38); // 旧 32
    assert_eq!(Event::TickEvent.wire_tag(), 0);
    assert_eq!(Event::NameInfo.wire_tag(), 60);
    assert_eq!(Event::RankInfo.wire_tag(), 67);
    assert_eq!(Event::ClearBansRequest.wire_tag(), 73);
}

#[test]
fn game_info_field_count_is_pinned() {
    assert_eq!(GAME_INFO_FIELD_COUNT, 9);
    let g = GameInfo::default();
    // 九个字段都能默认构造
    assert_eq!(g.current_player_count, 0);
    assert!(!g.locked);
}

#[test]
fn life_state_two_tables() {
    let mut s = LifeState::default();
    assert_eq!(s.lens(), (0, 0));

    // mode != 1：只写第一张表
    s.set(7, Status::Alive, None);
    assert_eq!(s.status(7), Some(Status::Alive));
    assert_eq!(s.lens(), (1, 0));

    // mode == 1：同时写第二张表
    s.set(7, Status::Dead, Some(120));
    assert_eq!(s.status(7), Some(Status::Dead));
    assert_eq!(s.lens(), (1, 1));
    assert_eq!(s.take_queued_death(7), Some(120));
    assert_eq!(s.lens(), (1, 0));
}

#[test]
fn unknown_soldier_is_not_assumed() {
    let s = LifeState::default();
    assert_eq!(s.status(42), None); // 不默认存活，也不默认死亡
}
