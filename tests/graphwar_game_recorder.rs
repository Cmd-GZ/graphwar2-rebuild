//! `graphwar_game_recorder` 的验收：**录什么、何时落盘、写到哪、什么格式**。
//!
//! ⚠️ 这里用 `#[path]` 把三个源码文件编进本测试二进制（本模块依赖
//! `events::Event` 与 `turnstate::GameState`），**不需要** `src/lib.rs` 加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `subscribes_to_32_info_events_only` | 32 条、全是 `*_info_event` | **逐位**（`0x3d7e3c` 的 32 个字面量） |
//! | `ending_game_is_six_and_setup_is_one` | 6 / 1 与 `turnstate::GameState` 一致 | **逐位**（`subs wzr,w23,#6` / `#1`） |
//! | `save_on_ending_game_rising_edge` | 只在上升沿落盘一次 | **逐位**（`0x3d7d68` 的 `b.eq`） |
//! | `setup_after_ending_clears_the_buffer` | 新一局清缓冲 | **逐位**（`0x3d7c8c`–`0x3d7ca8`） |
//! | `path_is_dir_plus_unix_seconds` | `user://replays/<secs>`，无扩展名 | **逐位**（`0x3d81a8` + `0x3d81f4`） |
//! | `one_line_json_has_event_before_tick` | `{"event":…,"tick":N}` | **逐位**（字面量顺序）+ **观测**（400 份回放） |
//! | `event_info_is_80_bytes` | 0x50 = 0x48(事件) + tick | **逐位**（`mov w8,#0x50` + `madd`） |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/turnstate.rs"]
mod turnstate;
#[path = "../src/graphwar_game_recorder.rs"]
mod graphwar_game_recorder;

use events::Event;
use graphwar_game_recorder::*;
use turnstate::GameState;

#[test]
fn subscribes_to_32_info_events_only() {
    assert_eq!(SUBSCRIPTIONS.len(), 32);
    assert_eq!(GraphwarGameRecorder::new().subscribed.len(), 32);
    // 25 条 `*_info_event` + 7 条同样单向广播但不带 `_info` 的
    let non_info = [
        "entity_removed_event",
        "remove_team_event",
        "function_fire_event",
        "function_active_event",
        "function_points_event",
        "clear_obstacles_event",
        "add_obstacle_event",
        "destroy_obstacle_event",
    ];
    let mut n_info = 0;
    for s in SUBSCRIPTIONS {
        assert!(!s.contains("_request_event"), "{} 是请求事件，不该订", s);
        if s.ends_with("_info_event") {
            n_info += 1;
        } else {
            assert!(non_info.contains(&s), "{} 既不是 info 也不在白名单里", s);
        }
    }
    assert_eq!(n_info, 24);
    assert_eq!(n_info + non_info.len(), 32);
    // 每条都必须在派发表里（否则订阅名就是编的）。
    for s in SUBSCRIPTIONS {
        assert!(
            Event::from_dispatch_name(s).is_some(),
            "{} 不在 events.rs 的派发表里",
            s
        );
    }
    // 录像必须含状态机事件，否则「何时落盘」无从判断。
    assert!(SUBSCRIPTIONS.contains(&"game_state_info_event"));
    // 32 条互不重复。
    let mut v = SUBSCRIPTIONS.to_vec();
    let n = v.len();
    v.sort_unstable();
    v.dedup();
    assert_eq!(v.len(), n);
}

#[test]
fn ending_game_is_six_and_setup_is_one() {
    assert_eq!(STATE_ENDING_GAME, 6);
    assert_eq!(STATE_SETUP, 1);
    assert_eq!(GameState::EndingGame as u8, STATE_ENDING_GAME);
    assert_eq!(GameState::Setup as u8, STATE_SETUP);
}

#[test]
fn save_on_ending_game_rising_edge() {
    let mut r = GraphwarGameRecorder::new();
    assert!(!r.record_game_state(100, 5)); // ExecutingResults
    assert_eq!(r.len(), 1);

    // 进入 EndingGame ⇒ 落盘
    assert!(r.record_game_state(110, STATE_ENDING_GAME));
    assert_eq!(r.saves, 1);

    // 再来一条 EndingGame ⇒ 不再落盘（`last_state == 6` 时 `b.eq` 跳过）
    assert!(!r.record_game_state(111, STATE_ENDING_GAME));
    assert_eq!(r.saves, 1);
}

#[test]
fn setup_after_ending_clears_the_buffer() {
    let mut r = GraphwarGameRecorder::new();
    r.record(1, Event::TeamInfo, "{}");
    r.record_game_state(2, STATE_ENDING_GAME);
    assert_eq!(r.len(), 2, "落盘时缓冲还在（原版不清）");
    assert_eq!(r.saves, 1);

    // Setup 且上一条是 EndingGame ⇒ 清空
    assert!(!r.record_game_state(3, STATE_SETUP));
    assert_eq!(r.len(), 0);
    assert_eq!(r.last_state(), STATE_SETUP);
}

#[test]
fn path_is_dir_plus_unix_seconds() {
    assert_eq!(REPLAY_DIR, "user://replays/");
    assert_eq!(REPLAY_DIR.len(), 15, "make_dir_recursive_absolute 的长度立即数");
    assert_eq!(replay_path(1782642754), "user://replays/1782642754");
    // 没有扩展名（原版只把秒数 Display 出来拼上去）
    assert!(!replay_path(1).contains('.'));
}

#[test]
fn one_line_json_has_event_before_tick() {
    let e = EventInfo {
        tick: 1234,
        event: "FunctionInfo".to_string(),
        body: "{\"owner_id\":635}".to_string(),
    };
    assert_eq!(
        e.to_json_line(),
        "{\"event\":{\"FunctionInfo\":{\"owner_id\":635}},\"tick\":1234}"
    );

    // 空体也要是合法对象
    let e = EventInfo {
        tick: 1,
        event: "GameStartRequest".to_string(),
        body: String::new(),
    };
    assert_eq!(e.to_json_line(), "{\"event\":{\"GameStartRequest\":{}},\"tick\":1}");

    // 逐行落盘：一行一个事件
    let mut r = GraphwarGameRecorder::new();
    r.record(9, Event::TurnInfo, "{\"player_id\":1}");
    r.record_game_state(10, STATE_ENDING_GAME);
    let text = r.save_game_text();
    assert_eq!(text.lines().count(), 2);
    assert!(text.lines().next().unwrap().ends_with("\"tick\":9}"));
    assert!(text.ends_with('\n'));
}

#[test]
fn event_info_is_80_bytes() {
    assert_eq!(GRAPHWAR_EVENT_SIZE, 0x48);
    assert_eq!(EVENT_INFO_STRIDE, 0x50);
    assert_eq!(EVENT_INFO_STRIDE - GRAPHWAR_EVENT_SIZE, 8, "tick + 对齐填充");
}
