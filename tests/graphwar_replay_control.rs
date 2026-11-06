//! `graphwar_replay_control` 的验收：**列表异步加载 + 按 tick 播放**。
//!
//! ⚠️ 用 `#[path]` 把四个源码文件编进本测试二进制（本模块依赖
//! `events::Event`、`turnstate::GameState`、`graphwar_game_recorder::EventInfo`），
//! **不需要** `src/lib.rs` 加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `four_mode_buttons` | 四个按钮名 | **逐位**（`.rodata` 相邻字面量） |
//! | `entry_scene_and_label` | `res://replay_entry.tscn` / `ReplayName` | **逐位** |
//! | `update_rebuilds_the_list` | 重建条目 + 选中项失效清空 | **观测**（`0x4caeec`） |
//! | `worker_loads_every_name_in_order` | 逐份解析后送回 | **观测**（`0x4cac88` 的 `load_games`/`load_game`/`send`） |
//! | `process_advances_only_when_playing` | 60 tick/秒、floor | **推断**（时钟字段未解，口径同 replay_player） |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/turnstate.rs"]
mod turnstate;
#[path = "../src/graphwar_game_recorder.rs"]
mod graphwar_game_recorder;
#[path = "../src/graphwar_replay_control.rs"]
mod graphwar_replay_control;

use graphwar_game_recorder::EventInfo;
use graphwar_replay_control::*;

#[test]
fn four_mode_buttons() {
    assert_eq!(MODE_BUTTONS.len(), 4);
    assert_eq!(MODE_BUTTONS[0], "AxisModeButton");
    assert_eq!(MODE_BUTTONS[1], "FunctionModeButton");
    assert_eq!(MODE_BUTTONS[2], "TurnModeButton");
    assert_eq!(MODE_BUTTONS[3], "TimeModeButton");
}

#[test]
fn entry_scene_and_label() {
    assert_eq!(ENTRY_SCENE, "res://replay_entry.tscn");
    assert_eq!(ENTRY_NAME_LABEL, "ReplayName");
    assert_eq!(SET_BUTTON_ICON, "set_button_icon");
}

#[test]
fn update_rebuilds_the_list() {
    let mut c = GraphwarReplayScreenControl::new();
    c.update(vec!["1782642754".into(), "1782640919".into()]);
    assert_eq!(c.entries.len(), 2);
    assert_eq!(c.entries[0].name, "1782642754");

    c.selected = Some(1);
    // 重建后下标越界 ⇒ 选择清空（原版先 queue_free 再重建）
    c.update(vec!["1782642754".into()]);
    assert_eq!(c.entries.len(), 1);
    assert_eq!(c.selected, None);

    // 下标仍有效 ⇒ 保留
    let mut c = GraphwarReplayScreenControl::new();
    c.update(vec!["a".into(), "b".into()]);
    c.selected = Some(0);
    c.update(vec!["x".into(), "y".into()]);
    assert_eq!(c.selected, Some(0));
}

#[test]
fn worker_loads_every_name_in_order() {
    let mut c = GraphwarReplayScreenControl::new();
    let names = vec!["a".to_string(), "b".to_string()];
    let mut seen: Vec<String> = Vec::new();

    c.update_replay_list(&names, |name| {
        seen.push(name.to_string());
        vec![EventInfo {
            tick: 1,
            event: "TurnInfo".to_string(),
            body: format!("{{\"name\":\"{}\"}}", name),
        }]
    });

    assert_eq!(seen, names, "顺序 = load_games 返回的顺序");
    assert_eq!(c.ready.len(), 2);

    // 主线程按到达顺序取
    let first = c.take_ready().unwrap();
    assert!(first[0].body.contains("\"a\""));
    let second = c.take_ready().unwrap();
    assert!(second[0].body.contains("\"b\""));
    assert!(c.take_ready().is_none());
}

#[test]
fn process_advances_only_when_playing() {
    let mut c = GraphwarReplayScreenControl::new();
    c.process(1.0);
    assert_eq!(c.cursor_tick, 0, "没在播放就不该动");

    c.playing = true;
    c.process(0.5); // 0.5s * 60 = 30 tick
    assert_eq!(c.cursor_tick, 30);
    c.process(0.016); // 0.96 -> floor 0
    assert_eq!(c.cursor_tick, 30);
    c.process(2.0);
    assert_eq!(c.cursor_tick, 150);
    assert!(c.is_playing());

    // 负的 dt 不倒带
    c.process(-1.0);
    assert_eq!(c.cursor_tick, 150);
}
