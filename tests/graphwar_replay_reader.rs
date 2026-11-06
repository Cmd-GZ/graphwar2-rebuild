//! `graphwar_replay_reader` 的验收：**文件层**（目录名 + gzip + 逐行 JSON）。
//!
//! ⚠️ 用 `#[path]` 把源码编进本测试二进制 ⇒ **不需要** `src/lib.rs` 加 `pub mod`。
//! 依赖链：本模块 → `_scaffold::protocol` / `_scaffold::replay_inflate` /
//! `graphwar_game_recorder` →（后者）`events` / `turnstate`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `replay_dir_is_the_literal` | `"user://replays/"` 15 字节、自带斜杠 | **逐位**（arm64 `.rodata 0x19bdef` + `mov w1,#0xf`） |
//! | `replay_file_does_not_double_the_slash` | 拼名不插分隔符 | **逐位**（同上） |
//! | `load_games_keeps_order` | 名字原序、空名跳过 | **观测**（`0x3da200` 8 字节步长 + `Vec::from_iter`） |
//! | `load_game_parses_jsonl` | 每行 → `EventInfo{tick,event,body}` | **观测**（`0x3da714`） |
//! | `load_game_unzips` | gzip 进、文本出 | **观测**（`0x3da48c` GzDecoder） |
//! | `bad_line_is_skipped_not_fatal` | 坏行丢、其余保留 | **观测**（`0x3da684` drop 后回循环头） |
//! | `empty_is_an_error` | 一行都没解出来 ⇒ `Empty` | **推断**（上游对空回放的处理未逐条解） |
//! | `no_clock_in_this_layer` | 本层不推进 tick ⇒ 分工 (b) | **观测**（被调者里无时间函数） |
#![allow(dead_code)]

// `#[path]` 只在**顶层** mod 上可靠（基准目录 = 本文件目录 `tests/`）；
// 内层 `mod _scaffold { .. }` 会把基准变成 `tests/_scaffold/`（该目录不存在，
// 中间的 `..` 也就无解），所以这里先平铺编进来再重导出成源码期望的路径。
#[path = "../src/_scaffold/protocol.rs"]
mod protocol_src;
#[path = "../src/_scaffold/replay_inflate.rs"]
mod inflate_src;

mod _scaffold {
    pub mod protocol {
        pub use crate::protocol_src::*;
    }
    pub mod replay_inflate {
        pub use crate::inflate_src::*;
    }
}
#[path = "../src/events.rs"]
mod events;
#[path = "../src/turnstate.rs"]
mod turnstate;
#[path = "../src/graphwar_game_recorder.rs"]
mod graphwar_game_recorder;
#[path = "../src/graphwar_replay_reader.rs"]
mod graphwar_replay_reader;

use graphwar_game_recorder::EventInfo;
use graphwar_replay_reader::*;

/// 一条与真实回放逐字同形的行（`docs/133` §2）。
const LINE_A: &str = r#"{"event":{"FunctionInfo":{"owner_id":635,"function":"(-0.35*(-11.40+x))","corrected_function":null}},"tick":1234}"#;
const LINE_B: &str = r#"{"event":{"TickInfo":{"current_tick":5}},"tick":1235}"#;

#[test]
fn replay_dir_is_the_literal() {
    assert_eq!(REPLAY_DIR, "user://replays/");
    assert_eq!(REPLAY_DIR.len(), REPLAY_DIR_LEN);
    assert!(REPLAY_DIR.ends_with('/'), "字面量自带结尾斜杠");
    assert_eq!(FILE_MODE_READ, 1);
}

#[test]
fn replay_file_does_not_double_the_slash() {
    assert_eq!(replay_file("1782642754"), "user://replays/1782642754");
    assert_eq!(replay_file(""), "user://replays/");
    // 与 recorder 侧同一口径
    assert_eq!(
        graphwar_game_recorder::replay_path(1782642754),
        replay_file("1782642754")
    );
}

#[test]
fn load_games_keeps_order() {
    let names = load_games(vec!["1782640919", "1782642754"]);
    assert_eq!(names, vec!["1782640919", "1782642754"]);
    // 空名跳过（目录里不该有，但迭代器模板会产出）
    assert_eq!(load_games(vec!["", "a"]), vec!["a"]);
    assert!(load_games(Vec::<String>::new()).is_empty());
}

#[test]
fn load_game_parses_jsonl() {
    let text = format!("{LINE_A}\n{LINE_B}\n");
    let evs = load_game_at("user://replays/1782642754", text.as_bytes()).unwrap();
    assert_eq!(evs.len(), 2);
    assert_eq!(evs[0].tick, 1234);
    assert_eq!(evs[0].event, "FunctionInfo");
    assert!(evs[0].body.contains("\"owner_id\":635"));
    assert_eq!(evs[1].tick, 1235);
    assert_eq!(evs[1].event, "TickInfo");
    // 名字拼出来的路径不参与解析，但不该 panic
    assert_eq!(
        load_game_bytes("1782642754", text.as_bytes()).unwrap().len(),
        2
    );
    assert_eq!(last_tick(&evs), Some(1235));
    assert_eq!(event_names(&evs), vec!["FunctionInfo", "TickInfo"]);
}

#[test]
fn load_game_unzips() {
    // _scaffold 里有一个只用 std 的 gunzip；这里用它的压缩方向不存在，
    // 所以直接验证"非 gzip 也照样能读"+"gzip 魔数分流"。
    let text = format!("{LINE_A}\n");
    assert!(!_scaffold::replay_inflate::is_gzip(text.as_bytes()));
    let evs = load_game_at("p", text.as_bytes()).unwrap();
    assert_eq!(evs.len(), 1);

    // 造一条最小 gzip：deflate 存储块（stored block），CRC32 用 0
    let mut gz = vec![
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, // 头
    ];
    let body = text.as_bytes();
    gz.push(0x01); // BFINAL=1, BTYPE=00 (stored)
    gz.extend_from_slice(&(body.len() as u16).to_le_bytes());
    gz.extend_from_slice(&(!(body.len() as u16)).to_le_bytes());
    gz.extend_from_slice(body);
    gz.extend_from_slice(&[0, 0, 0, 0]); // CRC32（本 inflate 不校验）
    gz.extend_from_slice(&(body.len() as u32).to_le_bytes()); // ISIZE
    assert!(_scaffold::replay_inflate::is_gzip(&gz));
    let evs = load_game_at("p", &gz).unwrap();
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].event, "FunctionInfo");
}

#[test]
fn bad_line_is_skipped_not_fatal() {
    let text = format!("{LINE_A}\nnot json\n{{\"event\":{{}},\"tick\":1}}\n{LINE_B}\n");
    let evs = load_game_at("p", text.as_bytes()).unwrap();
    assert_eq!(evs.len(), 2, "坏行丢掉，好行保留");
    assert_eq!(evs[0].tick, 1234);
    assert_eq!(evs[1].tick, 1235);
}

#[test]
fn empty_is_an_error() {
    assert_eq!(
        load_game_at("p", b""),
        Err(ReplayReadError::Empty)
    );
    assert_eq!(
        load_game_at("p", b"garbage\n"),
        Err(ReplayReadError::Empty)
    );
    assert_eq!(ReplayReadError::Empty.as_str(), "empty");
    assert_eq!(ReplayReadError::Io.as_str(), "io");
    assert_eq!(ReplayReadError::NotUtf8.as_str(), "not-utf8");
}

#[test]
fn no_clock_in_this_layer() {
    // 分工 (b)：本层只吐事件表，tick 完全由文件带来、原样返回。
    let text = format!("{LINE_A}\n{LINE_B}\n");
    let evs = load_game_at("p", text.as_bytes()).unwrap();
    let ticks: Vec<u32> = evs.iter().map(|e| e.tick).collect();
    assert_eq!(ticks, vec![1234, 1235], "tick 不重算、不排序");
    // 回放里 tick 有回退的真实情况（docs/133 §2.3）：本层不修
    let back = format!("{LINE_B}\n{LINE_A}\n");
    let evs = load_game_at("p", back.as_bytes()).unwrap();
    let ticks: Vec<u32> = evs.iter().map(|e| e.tick).collect();
    assert_eq!(ticks, vec![1235, 1234], "文件顺序即输出顺序");
}

#[test]
fn event_info_is_the_recorder_type() {
    // 返回类型就是 graphwar_game_recorder::EventInfo（arm64 的 drop_in_place 写的也是它）
    let evs: Vec<EventInfo> = load_game_at("p", format!("{LINE_A}\n").as_bytes()).unwrap();
    assert_eq!(evs[0].to_json_line().contains("\"tick\":1234"), true);
}
