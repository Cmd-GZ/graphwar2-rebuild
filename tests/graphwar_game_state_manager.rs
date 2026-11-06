//! **`graphwar_game_state_manager` 的原版函数边界**（回归保护，不是复原证据）。
//!
//! 本文件钉住的是 `docs/14-turn.md` §11 那张**45 行对照表**里"能测"的部分：
//!
//! | 断言 | 钉住的原版事实 | 证据地址 |
//! |---|---|---|
//! | [`state_variants_and_their_indices_are_the_binary_table`] | `GraphwarGameState` 有 **8** 个变体，顺序固定 | `.data.rel.ro 0x9586d0`–`0x958740`（`[&str; 8]` 静态表）+ `serialize` `0x46f020` 的 8 项跳表 `0x1c17c0` |
//! | [`winner_byte_matches_the_set_state_table`] | 胜方字节的四行表 + `adc` 那一行 | `set_state` `0x478133`–`0x478236` |
//! | [`auto_fire_needs_the_deadline_the_state_and_the_armed_flag`] | 回合截止 ⇒ 自动开火 的三个合取条件 | `handle_tick` `0x470989`–`0x4709ec` |
//! | [`alive_teams_count_live_entities_of_both_kinds`] | 判活数的是**生命表**里的活实体（士兵 ∪ 靶子） | `get_alive_teams` `0x473520` |
//! | [`set_all_alive_restores_targets_and_life_bits`] | `set_all_alive` 两趟 `set_alive` | `0x470b70`（`0x454b40` ×2） |
//! | [`set_turn_count_and_limit_emit_their_own_info`] | 两个 setter 各自发一条 info | `set_turn_limit` `0x472400`（tag `0x39`）/ arm64 `set_turn_count` `0x4a8460` |
//! | [`one_shot_fire_is_a_one_element_batch`] | `fire_functions` 与 `execute_results` 的切分没改报文 | `fire_batch` = 两者的顺序复合 |
//! | [`fire_functions_is_a_pure_geometry_pass`] | 前一半只算弹道，不落状态 | `0x470eb0` 里只写 `FunctionPoints` |
//! | [`handle_tick_carries_the_destroy_queue`] | `handle_tick` 至少要吃下到期动作 | `0x470840` |
//!
//! ⚠️ 按 `AGENTS.md` §0 第 2 条：**这些测试全绿不构成复原证据**。证据是上表的地址；
//! 测试只防"有人把边界又糊回去"。

use gw2_rebuild::game::{Match, Submission};
use gw2_rebuild::graphwar_shot_calculator::settle::Target;
use gw2_rebuild::server::RoomState;
use gw2_rebuild::turnstate::{next_state, should_auto_fire, winner_byte, GameState, TurnEvent};

fn room() -> RoomState {
    RoomState {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
    }
}

fn soldier(id: i64, x: f64, y: f64) -> Target {
    Target { id, x, y, radius: 0.5 }
}

fn kinds(msgs: &[String]) -> Vec<String> {
    msgs.iter()
        .map(|m| {
            let body = m.trim_start_matches('{');
            body.split_once('"').map(|(_, r)| r.split('"').next().unwrap_or("")).unwrap_or("")
                .to_string()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. `GraphwarGameState`：8 个变体与判别值
// ---------------------------------------------------------------------------

/// 原版的变体表是**一张静态数组**，所以"有哪些状态"和"谁是第几个"都不是猜的。
///
/// `.data.rel.ro` `0x9586d0`–`0x958740` 连续 8 个 `(ptr, len)` = 16 字节项，
/// `len` 依次 `10/5/19/18/20/16/10/12`；`serialize`（`0x46f020`）按判别值查
/// `.rodata 0x1c17c0` 的 8 项跳表去取其中一项 —— 判别值就是声明序。
#[test]
fn state_variants_and_their_indices_are_the_binary_table() {
    let want: [(GameState, u8, &str); 8] = [
        (GameState::Connecting, 0, "Connecting"),
        (GameState::Setup, 1, "Setup"),
        (GameState::WaitingForFunctions, 2, "WaitingForFunctions"),
        (GameState::CalculatingResults, 3, "CalculatingResults"),
        (GameState::SynchronizingResults, 4, "SynchronizingResults"),
        (GameState::ExecutingResults, 5, "ExecutingResults"),
        (GameState::EndingGame, 6, "EndingGame"),
        (GameState::Disconnected, 7, "Disconnected"),
    ];
    assert_eq!(GameState::VARIANTS.len(), want.len(), "原版是 8 个变体");
    for (i, (state, index, name)) in want.iter().enumerate() {
        assert_eq!(GameState::VARIANTS[i], *state, "第 {i} 个变体顺序不符");
        assert_eq!(state.index(), *index, "{name} 的判别值不符");
        assert_eq!(state.as_str(), *name);
        assert_eq!(GameState::from_str(name), Some(*state));
        // 表里的长度也必须对得上：这是"名字没写错"的独立校验。
        assert_eq!(name.len(), [10, 5, 19, 18, 20, 16, 10, 12][i], "{name} 的长度不符");
    }
}

/// `handle_tick` 用 `cmp BYTE [self+0x120], 2` 判"正在等开火"，而 2 必须是
/// `WaitingForFunctions` —— 这条把两处观测绑在一起（`0x470989` / `0x4780cc`）。
#[test]
fn waiting_for_functions_is_index_two() {
    assert_eq!(GameState::WaitingForFunctions.index(), 2);
}

// ---------------------------------------------------------------------------
// 2. 胜方字节（`set_state` 的 `EndingGame` 分支）
// ---------------------------------------------------------------------------

/// `0x478133`–`0x478236` 的四行表。
#[test]
fn winner_byte_matches_the_set_state_table() {
    // Team1 死、Team2 死 → 2（`mov al,2` / `sub al,dl`，dl=0）
    assert_eq!(winner_byte(false, false, false), 2);
    // Team1 死、Team2 活 → 1（`mov al,2; sub al,1`）
    assert_eq!(winner_byte(false, true, false), 1);
    assert_eq!(winner_byte(false, true, true), 1);
    // Team1 活、Team2 死 → 0（`xor eax,eax` 之后直接写）
    assert_eq!(winner_byte(true, false, false), 0);
    assert_eq!(winner_byte(true, false, true), 0);
    // 两边都活：`cmp dword[+0xc8],1` + `adc al,1` ⇒ 设过上限 1（Team2），否则 2（没人）
    assert_eq!(winner_byte(true, true, true), 1);
    assert_eq!(winner_byte(true, true, false), 2);
}

/// 我方 [`Match::ending`] 在**可达**收场上与那张表一致。
///
/// 可达 = "进了 `EndingGame` 的那些输入"：某队全灭，或上限用尽。两边都还有人却要
/// 拿到 `Ending`，只可能是上限用尽（`queue_end_turn_trigger` 的两个入口之一）。
#[test]
fn ending_agrees_with_the_winner_table_on_reachable_inputs() {
    // Team1 有士兵；Team2 只有一个靶子，先把它打死 ⇒ Team2 全灭 ⇒ 胜方 Team1。
    let mut m = Match::new(room(), vec![], vec![soldier(10, -5.0, -5.1)], (0, 0));
    m.set_team(10, 1); // Team::One.id()
    m.add_target(1099, 2); // Team::Two.id()
    assert_eq!(m.get_alive_teams(), (true, true));
    assert!(m.ending().is_none(), "两边都还有人、又没上限 ⇒ 不结束");

    assert!(m.kill_target(1099));
    assert_eq!(m.get_alive_teams(), (true, false));
    let e = m.ending().expect("Team2 全灭应结束");
    assert_eq!(e.winner, Some(1), "胜方应是 Team1 的实体队伍号");
    assert_eq!(winner_byte(true, false, false), 0, "原版这个字节是 0 = Team1");
}

// ---------------------------------------------------------------------------
// 3. `handle_tick` 的自动开火条件
// ---------------------------------------------------------------------------

/// `0x470989`–`0x4709ec` 的三个合取项，一项都不能少。
#[test]
fn auto_fire_needs_the_deadline_the_state_and_the_armed_flag() {
    let end = Some(10800);
    // 正确：等待中 + 有截止 + 到点了 + 有人在等
    assert!(should_auto_fire(GameState::WaitingForFunctions, 10800, end, true));
    assert!(should_auto_fire(GameState::WaitingForFunctions, 20000, end, true));
    // 差一个 tick 就不行（`jb` 是严格小于才跳过 ⇒ 相等算到点）
    assert!(!should_auto_fire(GameState::WaitingForFunctions, 10799, end, true));
    // 状态不是 WaitingForFunctions（含我们抓包里见过的另外 6 个）
    for s in GameState::VARIANTS {
        if s == GameState::WaitingForFunctions {
            continue;
        }
        assert!(!should_auto_fire(s, 1 << 40, end, true), "{s:?} 不该自动开火");
    }
    // 没有截止 tick（`get_end_of_turn_tick` 返回空）
    assert!(!should_auto_fire(GameState::WaitingForFunctions, 1 << 40, None, true));
    // `[self+0x11c] == 0`：没人在等
    assert!(!should_auto_fire(GameState::WaitingForFunctions, 1 << 40, end, false));
}

// ---------------------------------------------------------------------------
// 4. `get_alive_teams` / `set_all_alive`
// ---------------------------------------------------------------------------

/// 两类实体都进判活：士兵（在 `soldiers` 里）+ 靶子（`targets` 表 + 生命位）。
#[test]
fn alive_teams_count_live_entities_of_both_kinds() {
    let mut m = Match::new(room(), vec![], vec![soldier(10, -5.0, -5.1)], (0, 0));
    m.set_team(10, 1);
    m.add_target(1099, 2);
    m.add_target(1100, 2);
    assert_eq!(m.get_alive_teams(), (true, true));

    // 靶子全灭 ⇒ Team2 没活体；士兵还在 ⇒ Team1 有
    assert!(m.kill_target(1099));
    assert_eq!(m.get_alive_teams(), (true, true), "还剩一个靶子");
    assert!(m.kill_target(1100));
    assert_eq!(m.get_alive_teams(), (true, false));

    // 只有靶子的队伍也一样（打靶关的形态）
    let mut t = Match::new(room(), vec![], vec![soldier(10, -5.0, -5.1)], (0, 0));
    t.set_team(10, 1);
    t.add_target(2000, 2);
    assert_eq!(t.get_alive_teams(), (true, true));
}

/// `set_all_alive`（`0x470b70`）：两趟 `LifeManager::set_alive` —— 靶子一趟、生命表一趟。
#[test]
fn set_all_alive_restores_targets_and_life_bits() {
    let mut m = Match::new(room(), vec![], vec![soldier(10, -5.0, -5.1)], (0, 0));
    m.set_team(10, 1);
    m.add_target(1099, 2);
    m.add_target(1100, 2);
    assert_eq!(m.targets_alive(), 2);

    assert!(m.kill_target(1099));
    assert!(m.kill_target(1100));
    assert_eq!(m.targets_alive(), 0);
    assert_eq!(m.get_alive_teams(), (true, false));

    m.set_all_alive();
    assert_eq!(m.targets_alive(), 2, "靶子应全部复活");
    assert_eq!(m.get_alive_teams(), (true, true));
    // 幂等：再调一次不变
    m.set_all_alive();
    assert_eq!(m.targets_alive(), 2);
}

// ---------------------------------------------------------------------------
// 5. 两个 setter 各自的报文
// ---------------------------------------------------------------------------

#[test]
fn set_turn_count_and_limit_emit_their_own_info() {
    let mut m = Match::new(room(), vec![], vec![], (0, 0));
    // 原版语料里 21 条全是 {"TurnLimitInfo":{"turn_limit":N}}（docs/123 §5.1）
    let msgs = m.set_turn_limit(6);
    assert_eq!(msgs, vec![r#"{"TurnLimitInfo":{"turn_limit":6}}"#.to_string()]);
    assert_eq!(m.turn_limit, Some(6));

    // `set_turn_count` 只写计数 + 发一条；判结束不在这里
    let msgs = m.set_turn_count(4);
    assert_eq!(msgs, vec![r#"{"TurnCountInfo":{"turn_count":4}}"#.to_string()]);
    assert_eq!(m.turn_count, 4);
    assert!(m.ending().is_none(), "4 < 6，上限还没用尽");

    m.set_turn_count(6);
    assert!(m.ending().is_some(), "上限用尽应判结束");
}

// ---------------------------------------------------------------------------
// 6. `fire_functions` / `execute_results` 的切分
// ---------------------------------------------------------------------------

/// `fire_batch` 只是两者的顺序复合：单发批必须与 `fire` 逐字节相同（既有断言
/// `fire_emits_captured_turn_sequence` 已钉住 `fire` 的报文，这里钉住等号）。
#[test]
fn one_shot_fire_is_a_one_element_batch() {
    let mut a = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![soldier(10, 5.0, 0.0)], (5, 0));
    let mut b = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![soldier(10, 5.0, 0.0)], (5, 0));
    let via_fire = a.fire(10, "0.00", 5.0, 0.0, true, 1000);
    let via_batch = b.fire_batch(
        &[Submission { owner_id: 10, function_text: "0.00", x0: 5.0, y0: 0.0, flip: true }],
        1000,
    );
    assert_eq!(via_fire, via_batch);
    assert_eq!(kinds(&via_fire)[0], "GameStateInfo");
    assert!(via_fire[0].contains("CalculatingResults"));
}

/// 前一半（`0x470eb0`）只算几何：不摘士兵、不挖洞、不推队列。
#[test]
fn fire_functions_is_a_pure_geometry_pass() {
    let mut m = Match::new(
        room(),
        vec![],
        vec![soldier(10, 5.0, 0.0), soldier(11, -2.0, 0.0)],
        (5, 0),
    );
    m.set_team(10, 1);
    m.set_team(11, 2);
    let sub = Submission { owner_id: 10, function_text: "0.00", x0: 5.0, y0: 0.0, flip: true };
    let first = m.fire_functions(&[sub]);
    let second = m.fire_functions(&[sub]);
    assert_eq!(first, second, "同样的输入必须给同样的弹道");
    assert_eq!(m.soldiers.len(), 2, "纯几何不能摘掉被打死的士兵");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].killed, vec![11], "这一发应扫过 11 号");
}

/// 后半段的落地在 `execute_results` 里：摘人、写生命表。
#[test]
fn execute_results_lands_the_deaths() {
    let mut m = Match::new(
        room(),
        vec![],
        vec![soldier(10, 5.0, 0.0), soldier(11, -2.0, 0.0)],
        (5, 0),
    );
    m.set_team(10, 1);
    m.set_team(11, 2);
    let msgs = m.fire(10, "0.00", 5.0, 0.0, true, 1000);
    assert!(kinds(&msgs).iter().any(|k| k == "LifeQueuedDeath"));
    assert_eq!(m.soldiers.len(), 1, "死掉的士兵应被摘掉");
    assert_eq!(m.soldiers[0].id, 10);
    assert!(matches!(m.life().status(11), Some(gw2_rebuild::graphwar_life_manager::Status::Dead)));
}

// ---------------------------------------------------------------------------
// 7. `handle_tick`
// ---------------------------------------------------------------------------

/// `handle_tick`（`0x470840`）至少要吃下"到期动作"这件事；我们落地的是
/// `DestroyObstacle` 队列（原版的另两段见 `should_auto_fire` 与文档 §11.3）。
#[test]
fn handle_tick_carries_the_destroy_queue() {
    let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (0, 0));
    let msgs = m.fire(1, "0.00", 10.0, 0.0, true, 1000);
    assert!(msgs.iter().any(|s| s.starts_with(r#"{"QueueDestroyObstacle""#)));
    let queued_at = 60;
    assert!(m.handle_tick(queued_at).is_empty(), "刚排队时不该立刻删除");
    let due = m.handle_tick(queued_at + gw2_rebuild::game::DESTROY_DELAY_TICKS);
    assert_eq!(due.len(), 1);
    assert!(due[0].starts_with(r#"{"DestroyObstacle""#));
}

// ---------------------------------------------------------------------------
// 8. 迁移表口径没被这次重组动过（`docs/41`）
// ---------------------------------------------------------------------------

/// 13 次迁移的那条链只经过抓包里见过的 6 个状态；新增的 `Connecting` /
/// `Disconnected` 不在迁移图里（`_ => None`），这两条断言把这个边界钉住。
#[test]
fn extension_states_have_no_transition() {
    for ev in [
        TurnEvent::Fire,
        TurnEvent::ResultsComputed,
        TurnEvent::Activate,
        TurnEvent::TurnEnded,
        TurnEvent::GameEnded,
        TurnEvent::GameStarted,
        TurnEvent::Reset,
    ] {
        assert_eq!(next_state(GameState::Connecting, ev), None, "Connecting 没有观察到的迁移");
        assert_eq!(next_state(GameState::Disconnected, ev), None, "Disconnected 没有观察到的迁移");
    }
    // 既有的 13 次迁移照旧
    assert_eq!(
        next_state(GameState::Setup, TurnEvent::GameStarted),
        Some(GameState::WaitingForFunctions)
    );
    assert_eq!(
        next_state(GameState::ExecutingResults, TurnEvent::GameEnded),
        Some(GameState::EndingGame)
    );
}
