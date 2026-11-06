//! `graphwar_audio_control` 的验收：**哪个事件放哪个音**。
//!
//! ⚠️ 这里用 `#[path]` 直接把 `src/graphwar_audio_control.rs` 与 `src/events.rs`
//! 编进本测试二进制，所以**不需要** `src/lib.rs` 里先加 `pub mod`（那一行不在本分支
//! 的白名单里）。两份源码都只依赖 `std`，因此这种引法是合法的；父 agent 加上
//! `pub mod` 之后本文件可以改成 `use gw2_rebuild::graphwar_audio_control::*;` 而不改断言。
//!
//! 每条断言指向的证据见 `src/graphwar_audio_control.rs` 的模块文档
//! （地址一律 vaddr，Ghidra = vaddr + `0x100000`）：
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `four_subscriptions_with_the_right_handlers` | 4 条订阅的名字/长度/判别值/处理方法 | **逐位**（`new` 里 4 个 `Box::new(8)` + 4 个 vtable 的 `call_mut`） |
//! | `subscription_tags_match_the_event_table` | 44/15/14/30 与 `events.rs` 一致 | **逐位**（两侧独立） |
//! | `slot_to_sound_is_the_load_order` | `+0x80…+0xc0` = explosion/squash/spark/pop/unpop | **逐位**（构造助手的载荷初始化） |
//! | `volumes_are_three_constants` | -15 / 0 / -5 | **观测**（`fmov`/`movss` 立即数） |
//! | `effect_kind_falls_into_three_slots` | 四臂落三槽 | **逐位**（`cmp/b.gt/b.ne` 结构） |
//! | `queue_element_is_24_bytes` | `Vec` 元素步长 0x18 | **逐位**（`madd len,0x18,ptr`） |
//! | `game_state_one_clears_the_queue` | `state == 1` 才清 | **逐位**（`cmp w8,#1`） |
//! | `mute_is_the_or_of_two_flags` | `+0xd0 | +0xd1` | **逐位**（`orr w8,w19,w22; and #1`） |
//! | `humm_pipeline_constants` | 16000 / 0.1 / 32767 / 1600 | **观测**（`movss` 立即数） |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/graphwar_audio_control.rs"]
mod graphwar_audio_control;

use graphwar_audio_control::*;

/// 4 条订阅，顺序与 `new` 里的代码顺序一致；处理方法名是原版的 `impl` 方法名。
#[test]
fn four_subscriptions_with_the_right_handlers() {
    assert_eq!(SUBSCRIPTIONS.len(), 4);

    let names: Vec<&str> = SUBSCRIPTIONS.iter().map(|s| s.name).collect();
    assert_eq!(
        names,
        vec![
            "effect_info_event",
            "soldier_info_event",
            "entity_removed_event",
            "game_state_info_event",
        ]
    );

    let handlers: Vec<&str> = SUBSCRIPTIONS.iter().map(|s| s.handler).collect();
    assert_eq!(
        handlers,
        vec![
            "effect_info_event",
            "soldier_info_event",
            "entity_removed_event",
            // ⚠️ 方法名叫 game_state_event，订阅的是 game_state_info_event。
            "game_state_event",
        ]
    );

    let tags: Vec<u16> = SUBSCRIPTIONS.iter().map(|s| s.tag).collect();
    assert_eq!(tags, vec![44, 15, 14, 30]);
}

/// 名字长度是 `mov w2, #0x11/0x12/0x14/0x15` 抄下来的，与串本身必须一致。
#[test]
fn subscription_name_lengths_are_the_immediates() {
    let lens: Vec<usize> = SUBSCRIPTIONS.iter().map(|s| s.name.len()).collect();
    assert_eq!(lens, vec![0x11, 0x12, 0x14, 0x15]);
}

/// 判别值必须与项目那份 74 变体的事件表逐条相同（两条独立机制）。
#[test]
fn subscription_tags_match_the_event_table() {
    assert_eq!(SUBSCRIPTIONS[0].tag, events::Event::EffectInfo as u16);
    assert_eq!(SUBSCRIPTIONS[1].tag, events::Event::SoldierInfo as u16);
    assert_eq!(SUBSCRIPTIONS[2].tag, events::Event::EntityRemoved as u16);
    assert_eq!(SUBSCRIPTIONS[3].tag, events::Event::GameStateInfo as u16);
}

/// 五个资源槽 = 载入顺序（构造函数把第 1..5 个资源依次写进 `+0x80…+0xc0`）。
#[test]
fn slot_to_sound_is_the_load_order() {
    let paths: Vec<&str> = SoundSlot::ALL.iter().map(|s| s.path()).collect();
    assert_eq!(
        paths,
        vec![SFX_EXPLOSION, SFX_SQUASH, SFX_SPARK, SFX_POP, SFX_UNPOP]
    );
    assert_eq!(SFX_EXPLOSION, "res://sounds/explosion.mp3");
    assert_eq!(SFX_SQUASH, "res://sounds/squash.mp3");
    assert_eq!(SFX_SPARK, "res://sounds/spark.mp3");
    assert_eq!(SFX_POP, "res://sounds/pop.mp3");
    assert_eq!(SFX_UNPOP, "res://sounds/unpop.mp3");
    // humm.wav 不在这 5 个槽里（它走 AudioStreamGenerator）。
    assert!(!paths.contains(&SFX_HUMM));
}

/// 槽偏移就是判别值，间距 0x10。
#[test]
fn slot_offsets_are_0x80_through_0xc0() {
    assert_eq!(
        SoundSlot::ALL.map(|s| s.offset()),
        [0x80, 0x90, 0xa0, 0xb0, 0xc0]
    );
    assert_eq!(SoundSlot::STRIDE, 0x10);
}

/// 三个音量立即数：特效 -15、pop 0、unpop -5。
#[test]
fn volumes_are_three_constants() {
    assert_eq!(SoundSlot::Explosion.volume_db(), -15.0);
    assert_eq!(SoundSlot::Squash.volume_db(), -15.0);
    assert_eq!(SoundSlot::Spark.volume_db(), -15.0);
    assert_eq!(SoundSlot::Pop.volume_db(), 0.0);
    assert_eq!(SoundSlot::Unpop.volume_db(), -5.0);
    assert_eq!(EFFECT_VOLUME_DB, -15.0);
    assert_eq!(SOLDIER_VOLUME_DB, 0.0);
    assert_eq!(UNPOP_VOLUME_DB, -5.0);
}

/// 四臂落三槽：0 → explosion，1 → squash，2 与其余 → spark。
#[test]
fn effect_kind_falls_into_three_slots() {
    assert_eq!(slot_for_effect_kind(0), SoundSlot::Explosion);
    assert_eq!(slot_for_effect_kind(1), SoundSlot::Squash);
    assert_eq!(slot_for_effect_kind(2), SoundSlot::Spark);
    for k in 3u8..=255 {
        assert_eq!(slot_for_effect_kind(k), SoundSlot::Spark, "kind {k}");
    }
}

/// 事件处理之后队列里那一条记录的形状。
#[test]
fn handlers_enqueue_one_record_each() {
    let mut a = GraphwarAudioControl::new();
    assert_eq!(a.pending(), 0);

    let e = a.on_effect_info(1, 0x1234_5678);
    assert_eq!(e.slot, SoundSlot::Squash);
    assert_eq!(e.value, 0x1234_5678);
    assert!(!e.started);
    assert_eq!(e.path(), SFX_SQUASH);
    assert_eq!(e.volume_db(), -15.0);

    let s = a.on_soldier_info();
    assert_eq!(s.slot, SoundSlot::Pop);
    assert_eq!(s.value, 0);
    assert_eq!(s.volume_db(), 0.0);

    let r = a.on_entity_removed();
    assert_eq!(r.slot, SoundSlot::Unpop);
    assert_eq!(r.value, 0);
    assert_eq!(r.volume_db(), -5.0);

    assert_eq!(a.pending(), 3);
}

/// `Vec<EffectAudioData>` 的元素步长 0x18，字段偏移 +0x10 / +0x14。
#[test]
fn queue_element_is_24_bytes() {
    assert_eq!(EffectAudioData::SIZE, 0x18);
    assert_eq!(EffectAudioData::VALUE_OFFSET, 0x10);
    assert_eq!(EffectAudioData::STARTED_OFFSET, 0x14);
    assert_eq!(
        (QUEUE_CAP_OFFSET, QUEUE_PTR_OFFSET, QUEUE_LEN_OFFSET),
        (0x00, 0x08, 0x10)
    );
}

/// `game_state_info_event` 只有 `payload+0x8 == 1` 才清空队列。
#[test]
fn game_state_one_clears_the_queue() {
    let mut a = GraphwarAudioControl::new();
    for k in 0..4u8 {
        a.on_effect_info(k, k as u32);
    }
    assert_eq!(a.pending(), 4);

    // 其它状态一律不动。
    for state in [0u8, 2, 3, 255] {
        assert_eq!(a.on_game_state_info(state), 0);
        assert_eq!(a.pending(), 4);
    }

    assert_eq!(a.on_game_state_info(GAME_STATE_CLEAR), 4);
    assert_eq!(a.pending(), 0);

    // 空队列上再来一次也是 0。
    assert_eq!(a.on_game_state_info(GAME_STATE_CLEAR), 0);
}

/// 有效静音 = 两个标志相或；两个 setter 各写自己的字节。
#[test]
fn mute_is_the_or_of_two_flags() {
    let mut a = GraphwarAudioControl::new();
    assert!(!a.effective_mute());

    assert!(a.set_muted(true)); // 用户静音 ⇒ 总线静音（返回值就是总线该设的值）
    assert!(a.effective_mute());
    assert_eq!((a.muted, a.hidden_muted), (true, false));

    assert!(a.set_hidden_muted(true));
    a.set_muted(false);
    assert!(a.effective_mute(), "hidden 那个标志仍然有效");
    assert_eq!((a.muted, a.hidden_muted), (false, true));

    a.set_hidden_muted(false);
    assert!(!a.effective_mute());

    assert_eq!(MUTED_OFFSET, 0xd0);
    assert_eq!(HIDDEN_MUTED_OFFSET, 0xd1);
    assert_eq!(MASTER_BUS, "Master");
    assert_eq!(GraphwarAudioControl::SIZE, 0xf0);
}

/// `humm.wav` 那条链路的常量：16000 Hz / 0.1 s / 32767 归一化。
#[test]
fn humm_pipeline_constants() {
    assert_eq!(HUM_MIX_RATE, 16000.0);
    assert_eq!(HUM_BUFFER_LENGTH, 0.1);
    assert_eq!(PCM_FULL_SCALE, 32767.0);
    assert_eq!(GraphwarAudioControl::hum_frames_per_buffer(), 1600);

    // i16 满量程归一化。
    assert_eq!(GraphwarAudioControl::normalize_pcm(32767), 1.0);
    assert_eq!(GraphwarAudioControl::normalize_pcm(-32767), -1.0);
    assert_eq!(GraphwarAudioControl::normalize_pcm(0), 0.0);
}

/// 载荷字段偏移（`effect_info` 的 u8 种类 @+0x20、u32 值 @+0x1c；`game_state` 的 u8 @+0x8）。
#[test]
fn payload_offsets() {
    assert_eq!(EFFECT_INFO_KIND_OFFSET, 0x20);
    assert_eq!(EFFECT_INFO_VALUE_OFFSET, 0x1c);
    assert_eq!(GAME_STATE_OFFSET, 0x08);
    assert_eq!(GAME_STATE_CLEAR, 1);
}
