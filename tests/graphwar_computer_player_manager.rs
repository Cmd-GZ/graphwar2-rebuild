//! `graphwar_computer_player_manager` 的验收：**谁会成为 Bot / 难度怎么分档**。
//!
//! ⚠️ 用 `#[path]` 直接把 `src/graphwar_computer_player_manager.rs` 编进本测试二进制，
//! 这样本分支**不需要**改 `src/lib.rs`。证据见该文件模块文档。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `six_tiers_from_five_boundaries` | `level` → 六档，阈值 `4/11/26/101/251` | **观测**（`0x478fc0` 的链式比较） |
//! | `boundaries_are_generations_plus_one` | 阈值 = `docs/13-ai.md` §1"代数"列 + 1 | **观测**（两条独立证据互证） |
//! | `elite_count_is_population_over_five` | 精英数 `10/20/30/40/50/50` = 种群/5，`limit` = §1 的 `limit` 列 | **观测**（调用点 `case 10` 的链） |
//! | `name_pool_is_ten_and_random` | 名字池 **10** 项，与 `level` 无数据流 | **观测**（`.data.rel.ro 0x959cd8` + `% 10`） |
//! | `profile_requests_are_five` | 建档发 5 条 request：59/50/52/54/66 | **观测** |
//! | `bot_is_created_only_by_bot_added` | 建档入口唯一；**没有**掉线/超时接管 | **观测**（订阅表 + `0x3e2830` 的 `case 10`） |
//! | `turn_info_needs_state_three` | 三道门：tag=43 / 表非空 / `state == 3` | **观测** |
#![allow(dead_code)]

#[path = "../src/graphwar_computer_player_manager.rs"]
mod graphwar_computer_player_manager;

use graphwar_computer_player_manager::*;

/// `level` 的六个区间，逐个边界钉。
#[test]
fn six_tiers_from_five_boundaries() {
    let cases = [
        (0u32, 0u8),
        (3, 0),
        (4, 1),
        (10, 1),
        (11, 2),
        (25, 2),
        (26, 3),
        (100, 3),
        (101, 4),
        (250, 4),
        (251, 5),
        (u32::MAX, 5),
    ];
    for (level, tier) in cases {
        assert_eq!(tier_for_level(level), tier, "level={level}");
        assert_eq!(skin_for_level(level), SKIN_PER_TIER[tier as usize]);
    }
    assert_eq!(LEVEL_BOUNDS, [3, 10, 25, 100, 250]);
    assert_eq!(SKIN_PER_TIER.len(), 6);
}

/// 阈值 = "代数" + 1：`level <= 代数上限` 就落进那一档。
///
/// 断言的写法刻意绕开"直接抄表"：对每一档，`代数` 落在本档、`代数 + 1` 落到下一档。
#[test]
fn boundaries_are_generations_plus_one() {
    let generations = [3u32, 10, 25, 100, 250];
    for (tier, generations) in generations.iter().enumerate() {
        assert_eq!(tier_for_level(*generations) as usize, tier);
        if tier + 1 < SKIN_PER_TIER.len() {
            assert_eq!(tier_for_level(generations + 1) as usize, tier + 1);
        }
    }
}

/// 精英数 = 种群/5；第五、六档都是 50（种群 250）。
#[test]
fn elite_count_is_population_over_five() {
    let population = [50u32, 100, 150, 200, 250, 250];
    let probe = [0u32, 4, 11, 26, 101, 251];
    for tier in 0..6usize {
        let level = probe[tier];
        assert_eq!(elite_for_level(level), population[tier] / 5, "tier {tier}");
        assert_eq!(ELITE_PER_TIER[tier], population[tier] / 5);
        let bot = NewBot::from_level(1, level);
        assert_eq!(bot.elite_count, population[tier] / 5);
        assert_eq!(bot.per_slot_individuals, 5);
        assert_eq!(bot.limit, [5u32, 5, 10, 15, 25, 200][tier]);
        assert_eq!(bot.tier() as usize, tier);
    }
}

/// 名字是**随机**的 6 选 1，与 `level` 没有数据流（这篇对旧表的一处更正）。
#[test]
fn name_pool_is_ten_and_random() {
    assert_eq!(BOT_NAMES.len(), 10);
    // 十项互不相同
    for i in 0..10 {
        for j in (i + 1)..10 {
            assert_ne!(BOT_NAMES[i], BOT_NAMES[j], "名字池有重复");
        }
    }
    // 同一个 level，换下标就换名字 —— level 不参与
    let a = BotProfile::new(101, 0);
    let b = BotProfile::new(101, 3);
    assert_eq!(a.skin, b.skin);
    assert_ne!(a.name, b.name);
    assert_eq!(a.name, "Deep Thought");
    assert_eq!(b.name, "Agent Smith");
    // % 10 回绕
    assert_eq!(bot_name(10), BOT_NAMES[0]);
    assert_eq!(bot_name(19), BOT_NAMES[9]);
    // 旧文档表里的 6 个名字都在池里 ⇒ 那张表只是抽样，不是映射
    for name in ["ChatGPT", "Wolfram", "Alpha Zordon", "Multivac", "Deep Blue", "Deep Thought"] {
        assert!(BOT_NAMES.contains(&name), "{name} 在池里");
    }
}

/// 建档顺序：name / skin / face / hat / rank 五条 request。
#[test]
fn profile_requests_are_five() {
    let profile = BotProfile::new(101, 2); // Skynet, bot_4
    assert_eq!(profile.name, "Skynet");
    assert_eq!(profile.skin, "bot_4");
    assert_eq!(profile.face, "no_face");
    assert_eq!(profile.hat, "no_hat");
    let events = add_computer_player_events(9, &profile);
    assert_eq!(events.len(), 5);
    assert_eq!(
        events.iter().map(|e| e.discriminant()).collect::<Vec<u8>>(),
        vec![59, 50, 52, 54, 66]
    );
    assert_eq!(
        events[0],
        BotEvent::NameRequest {
            player_id: 9,
            name: "Skynet"
        }
    );
    assert_eq!(events[1].tag(), 0x8000_0000_0000_0032);
    assert_eq!(events[4].tag(), 0x8000_0000_0000_0042);
}

/// **Bot 只有一个建立入口**：`bot_added_event`（判别值 10）。
///
/// 订阅表里**没有** `connection_removed_event`、没有超时事件 ⇒ 这个模块里
/// 不存在"掉线/超时 ⇒ 我接管"的分支。
#[test]
fn bot_is_created_only_by_bot_added() {
    assert_eq!(EVENT_BOT_ADDED, 10);
    let names: Vec<&str> = SUBSCRIBED_EVENTS.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        vec!["tick_event", "turn_info_event", "entity_removed_event"]
    );
    for forbidden in [
        "connection_removed_event",
        "new_connection_event",
        "game_state_info_event",
        "tick_request_event",
    ] {
        assert!(
            !names.contains(&forbidden),
            "{forbidden} 不在本模块的订阅里 ⇒ 本模块不做掉线接管"
        );
    }
    // 建档只需要 level 两个数
    let bot = NewBot::from_level(5, 26);
    assert_eq!(bot.level, 26);
    assert_eq!(bot.connection_id, 5);
}

/// 三道门：tag 由调用方保证；表空 / 不在表里 / `state != 3` 全部 `None`。
#[test]
fn turn_info_needs_state_three() {
    let mut bots = ComputerPlayers::new();
    let turn = TurnInfo {
        player_id: 4,
        soldier_id: 40,
    };
    assert!(bots.handle_turn_info_event(turn).is_none(), "表空 ⇒ 什么都不做");
    bots.add_computer_player(NewBot::from_level(4, 3));
    assert!(bots.handle_turn_info_event(turn).is_some());
    assert_eq!(BOT_STATE_READY, 3);
    bots.get_mut(4).unwrap().state = 1;
    assert!(bots.handle_turn_info_event(turn).is_none(), "state != 3 ⇒ 不放行");
    // 士兵有 SoldierBotData 时把那个字节带出去
    bots.get_mut(4).unwrap().state = BOT_STATE_READY;
    bots.get_mut(4)
        .unwrap()
        .soldiers
        .insert(40, SoldierBotData { byte_50: 7 });
    let req = bots.handle_turn_info_event(turn).unwrap();
    assert_eq!(req.flag, 7);
    assert_eq!(req.new_bot.level, 3);
}

/// `entity_removed_event` 把该连接从 Bot 表里摘掉（不是"暂停"，是真删）。
#[test]
fn entity_removed_erases_the_bot() {
    let mut bots = ComputerPlayers::new();
    bots.add_computer_player(NewBot::from_level(40, 0));
    assert_eq!(bots.ready_count(), 1);
    assert!(bots.handle_entity_removed_event(40));
    assert_eq!(bots.len(), 0);
    assert!(!bots.handle_entity_removed_event(40));
    assert_eq!(TAG_ENTITY_REMOVED & 0xff, EVENT_ENTITY_REMOVED as u64);
    assert_eq!(TAG_TURN_INFO & 0xff, EVENT_TURN_INFO as u64);
}
