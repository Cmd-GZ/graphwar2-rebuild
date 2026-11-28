//! T109 P4–P5 落码的验收：campaign 文件层 2 条 + level_manager 1 条 + game_server 1 条
//! + function_input 1 条 + audio 的 2 个**真名**入口。
//!
//! 期望值全部来自 arm64 反汇编里的立即数 / 分支 / 字符串 —— 不是来自我方输出。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `current_level_reads_save` | 读 `campaign.save` → `level_<n>` → `n` | **观测**（`0x392fb8 bl get_level_as_int`） |
//! | `current_level_none_when_unreadable` | 打不开 ⇒ `None` | **观测**（`0x393060 mov w19,wzr`） |
//! | `save_current_level_decision` | 新 ≤ 旧不写 / 旧坏必写 / 新坏不写 | **观测**（`0x3932a0`/`0x3932b4` 两次 parse） |
//! | `level_data_is_keyed_by_bytes` | 键 = 按内容相等的字节串 | **观测**（`0x3fc7c0 bl memcmp`） |
//! | `level_data_value_is_72_bytes` | 值宽度 72 | **观测**（`0x3fc770 mov w24,#0x58` − 0x10） |
//! | `server_port_once_only` | 只取一次 + 两个 u16 缓存 | **观测**（`0x482aa0` 哨兵 3 / `0x482a70` 标志） |
//! | `server_port_log_strings` | 41 字节原话 + `src/graphwar_game_server.rs:175` | **观测**（`.rodata 0x1a00f9` / `0x186a40`） |
//! | `clear_defers_the_literal_method_name` | `"clear"` 5 字节 | **观测**（`.rodata 0x19de23`） |
//! | `audio_real_names_match_the_aliases` | 真名入口与旧名别名同结果 | **逐位**（槽表 + 音量由 T58 定标） |

#![allow(dead_code)]

use gw2_rebuild::graphwar_audio_control::{GraphwarAudioControl, SoundSlot};
use gw2_rebuild::graphwar_campaign_control::{get_current_level, save_current_level};
use gw2_rebuild::graphwar_function_input::clear as finput_clear;
use gw2_rebuild::graphwar_game_server::{
    ServerPortSlot, LOG_FMT_FILE_LINE, PORT_CHANNEL_OFFSET, PORT_CHANNEL_SENTINEL,
    PORT_FLAG_OFFSET, PORT_VALUE_OFFSET, SERVER_BIND_FAILED_PREFIX, SERVER_THREAD_DIED,
};
use gw2_rebuild::graphwar_level_manager::{LevelData, LevelDataTable, LEVEL_DATA_SIZE};

#[test]
fn current_level_reads_save() {
    // 正常情况下文件内容是 `level_<n>`（0 基、无尾换行）
    assert_eq!(get_current_level(Some("level_0")), Some(0));
    assert_eq!(get_current_level(Some("level_7")), Some(7));
    assert_eq!(get_current_level(Some("level_20")), Some(20));
}

#[test]
fn current_level_none_when_unreadable() {
    // 0x392f4c 打开失败 ⇒ 0x393060 mov w19,wzr ⇒ Option 的 None
    assert_eq!(get_current_level(None), None);
    // 内容解析不出数字 ⇒ get_level_as_int 的 None（原版走 unwrap_failed，见模块文档）
    assert_eq!(get_current_level(Some("garbage")), None);
    // 多一个换行就解析失败 —— 这是原版 `parse::<u32>()` 不跳空白的直接后果
    assert_eq!(get_current_level(Some("level_20\n")), None);
}

#[test]
fn save_current_level_decision() {
    // 新 > 旧 ⇒ 写（true）
    assert!(save_current_level(Some("level_3"), "level_5"));
    // 新 <= 旧 ⇒ 不写
    assert!(!save_current_level(Some("level_5"), "level_5"));
    assert!(!save_current_level(Some("level_5"), "level_3"));
    // 旧值无效（文件不存在）⇒ 必写
    assert!(save_current_level(None, "level_0"));
    // 新串无效而旧值有效 ⇒ 不写（不拿坏串覆盖好存档）
    assert!(!save_current_level(Some("level_5"), "junk"));
}

#[test]
fn level_data_is_keyed_by_bytes() {
    let mut table = LevelDataTable::default();
    // 0x3fc744 的空表短路
    assert!(table.get_level_data(b"anything").is_none());

    let mut d = LevelData::default();
    d.0[0] = 0x11;
    table.insert(b"level_0", d);
    assert!(table.get_level_data(b"level_0").is_some());
    assert_eq!(table.get_level_data(b"level_0").unwrap().0[0], 0x11);
    // 键按**内容**相等（0x3fc7c0 bl memcmp）：同内容新切片命中、差一个字节不命中
    assert!(table.get_level_data(&b"level_0".to_vec()).is_some());
    assert!(table.get_level_data(b"level_1").is_none());
    assert!(table.get_level_data(b"level_").is_none());
}

#[test]
fn level_data_value_is_72_bytes() {
    // 0x3fc770 mov w24,#0x58（槽位步长 0x58）− 0x10（键 16 字节 Box<[u8]>）= 0x48
    assert_eq!(LEVEL_DATA_SIZE, 72);
    assert_eq!(LEVEL_DATA_SIZE, 0x58 - 0x10);
    assert_eq!(std::mem::size_of::<LevelData>(), LEVEL_DATA_SIZE);
}

#[test]
fn server_port_once_only() {
    // 字段偏移与哨兵（全部来自指令立即数）
    assert_eq!(PORT_FLAG_OFFSET, 0x74);
    assert_eq!(PORT_VALUE_OFFSET, 0x76);
    assert_eq!(PORT_CHANNEL_OFFSET, 0x20);
    assert_eq!(PORT_CHANNEL_SENTINEL, 3);

    // ① 没通道、没标志 ⇒ (false, 0)
    let mut slot = ServerPortSlot::default();
    assert_eq!(slot.get_server_port(), (false, 0));
    // 第二次仍然 (false, 0)：原版靠把通道第一个字打成 3 来记"取过"
    assert_eq!(slot.get_server_port(), (false, 0));

    // ② 通道里有端口 ⇒ (true, port)，并写进两个 u16 缓存
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(27015u16).unwrap();
    let mut slot = ServerPortSlot {
        channel: Some(rx),
        ..Default::default()
    };
    assert_eq!(slot.get_server_port(), (true, 27015));
    assert_eq!(slot.got_port, 1);
    assert_eq!(slot.port, 27015);
    // 已缓存 ⇒ 即使通道被拿走，也直接命中第一条臂（0x482a70 的 cbz 不跳）
    assert_eq!(slot.get_server_port(), (true, 27015));

    // ③ 发送端已死 ⇒ (false, 端口)，且标志仍是 0（原版两条错误臂都落 mov w20,wzr）
    let (tx, rx) = std::sync::mpsc::channel::<u16>();
    drop(tx);
    let mut slot = ServerPortSlot {
        channel: Some(rx),
        ..Default::default()
    };
    assert_eq!(slot.get_server_port(), (false, 0));
    assert_eq!(slot.got_port, 0);
    assert!(slot.channel.is_none(), "通道已被取走（原版写哨兵 3）");
}

#[test]
fn server_port_log_strings() {
    // 41 字节 = 0x482b00 mov w0,#0x29
    assert_eq!(SERVER_THREAD_DIED, "Server thread died before reporting port.");
    assert_eq!(SERVER_THREAD_DIED.len(), 41);
    assert_eq!(SERVER_THREAD_DIED.len(), 0x29);
    assert_eq!(SERVER_BIND_FAILED_PREFIX, "Server failed to bind: ");
    // 日志格式串里写着源码文件名与行号 —— 本模块第一条非 Location 的"源码级"证据
    assert_eq!(LOG_FMT_FILE_LINE, "src/graphwar_game_server.rs:175");
}

#[test]
fn clear_defers_the_literal_method_name() {
    // .rodata 0x19de23 = "clear"，call_deferred 的长度实参 0x44db30 mov w2,#0x5
    assert_eq!(finput_clear(), "clear");
    assert_eq!(finput_clear().len(), 5);
}

#[test]
fn audio_real_names_match_the_aliases() {
    // 真名入口（0x4c46f4 / 0x4c488c）与旧名别名必须同语义
    let mut a = GraphwarAudioControl::new();
    let pop = a.soldier_info_event();
    assert_eq!(pop.slot, SoundSlot::Pop);
    assert_eq!(pop.value, 0);
    assert_eq!(SoundSlot::Pop.volume_db(), 0.0);
    assert_eq!(a.pending(), 1);

    let mut b = GraphwarAudioControl::new();
    let pop2 = b.on_soldier_info();
    assert_eq!(pop2.slot, pop.slot, "别名与真名同槽");

    let mut c = GraphwarAudioControl::new();
    let unpop = c.entity_removed_event();
    assert_eq!(unpop.slot, SoundSlot::Unpop);
    assert_eq!(unpop.value, 0);
    assert_eq!(SoundSlot::Unpop.volume_db(), -5.0);
    let mut d = GraphwarAudioControl::new();
    assert_eq!(d.on_entity_removed().slot, unpop.slot);
}
