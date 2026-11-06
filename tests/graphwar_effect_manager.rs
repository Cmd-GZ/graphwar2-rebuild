//! `graphwar_effect_manager` 的验收：**特效登记、寿命 3600 tick、四类型判别值**。
//!
//! ⚠️ 这里用 `#[path]` 直接把 `src/graphwar_effect_manager.rs` 与 `src/events.rs`
//! 编进本测试二进制，所以**不需要** `src/lib.rs` 里先加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `effect_type_codes_are_declaration_order` | 0..3 = Explosion/Death/Spark/Ash | **逐位**（`.rodata 0x19caf8` 首尾相接的名字） |
//! | `subscription_is_tick_event` | 只订 `tick_event`（长度 10） | **逐位**（`0x40bc0c` 的 `mov w2,#0xa`） |
//! | `add_effect_registers_and_broadcasts` | 登记 + 一条 `EffectInfo`(44) | **逐位**（`0x40bb20`） |
//! | `lifetime_is_3600_ticks` | 边界：`start+3599` 留、`start+3600` 删 | **逐位**（`add w0,w0,#0xe10`） |
//! | `tick_emits_nothing` | 回收不发事件 | **逐位**（`tick` 里没有 `add_event`） |
//! | `payload_layout_matches_the_event_offsets` | +0x18 id / +0x1c tick / +0x20 type | **逐位**（与 `audio_control` 的 `+0x20` 分支互校） |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/graphwar_effect_manager.rs"]
mod graphwar_effect_manager;

use events::Event;
use graphwar_effect_manager::*;

#[test]
fn effect_type_codes_are_declaration_order() {
    assert_eq!(EFFECT_TYPE_VARIANTS.len(), 4);
    assert_eq!(EffectType::Explosion.code(), 0);
    assert_eq!(EffectType::Death.code(), 1);
    assert_eq!(EffectType::Spark.code(), 2);
    assert_eq!(EffectType::Ash.code(), 3);

    assert_eq!(EffectType::Explosion.name(), "ExplosionEffect");
    assert_eq!(EffectType::Death.name(), "DeathEffect");
    assert_eq!(EffectType::Spark.name(), "SparkEffect");
    assert_eq!(EffectType::Ash.name(), "AshEffect");

    for (i, n) in EFFECT_TYPE_VARIANTS.iter().enumerate() {
        assert_eq!(EffectType::from_code(i as u8).unwrap().name(), *n);
        assert_eq!(EffectType::from_code(i as u8).unwrap().code(), i as u8);
    }
    // 4 是越界：原版 deserialize 到这一步就报 unknown variant。
    assert_eq!(EffectType::from_code(4), None);
}

#[test]
fn subscription_is_tick_event() {
    let m = GraphwarEffectManager::new();
    assert_eq!(m.subscribed, vec!["tick_event"]);
    assert_eq!(GraphwarEffectManager::SUBSCRIPTION.len(), 10);
}

#[test]
fn add_effect_registers_and_broadcasts() {
    let mut m = GraphwarEffectManager::new();
    assert!(m.is_empty());

    m.add_effect(237, (-10.452779035793856, -2.651563472451839), EffectType::Death, 514);

    assert_eq!(m.len(), 1);
    assert_eq!(m.outbox, vec![Event::EffectInfo]);
    assert_eq!(EffectInfo::TAG, 44);
    assert_eq!(Event::EffectInfo.wire_tag(), 44);

    let got = m.last_effect_info(237).unwrap();
    assert_eq!(got.effect_id, 237);
    assert_eq!(got.start_tick, 514);
    assert_eq!(got.effect_type, EffectType::Death);
    assert_eq!(got.pos_x, -10.452779035793856);
    assert_eq!(got.pos_y, -2.651563472451839);
}

#[test]
fn lifetime_is_3600_ticks() {
    assert_eq!(EFFECT_LIFETIME_TICKS, 3600);

    let mut m = GraphwarEffectManager::new();
    m.add_effect(1, (0.0, 0.0), EffectType::Spark, 1000);

    // 差一 tick 还在
    assert!(m.tick(1000 + 3599).is_empty());
    assert_eq!(m.len(), 1);

    // 正好 3600 就回收（判据是 `start + 3600 <= now`，`b.hi` 跳过）
    assert_eq!(m.tick(1000 + 3600), vec![1]);
    assert!(m.is_empty());
}

#[test]
fn tick_emits_nothing() {
    let mut m = GraphwarEffectManager::new();
    m.add_effect(1, (0.0, 0.0), EffectType::Explosion, 0);
    let before = m.outbox.len();
    let _ = m.tick(99999);
    assert_eq!(m.outbox.len(), before, "回收不该广播事件");
    assert_eq!(before, 1);
}

#[test]
fn tick_removes_every_expired_entry() {
    let mut m = GraphwarEffectManager::new();
    m.add_effect(1, (0.0, 0.0), EffectType::Explosion, 0);
    m.add_effect(2, (1.0, 1.0), EffectType::Death, 5000);
    m.add_effect(3, (2.0, 2.0), EffectType::Ash, 5001);

    let mut dead = m.tick(5000);
    dead.sort_unstable();
    // now=5000：1 号（0+3600）到期；2 号（5000+3600）未到；3 号未到
    assert_eq!(dead, vec![1]);
    assert_eq!(m.len(), 2);

    let mut dead = m.tick(5001 + 3600);
    dead.sort_unstable();
    assert_eq!(dead, vec![2, 3]);
    assert!(m.is_empty());
}

#[test]
fn payload_layout_matches_the_event_offsets() {
    // 事件里：+0x18 effect_id、+0x1c start_tick、+0x20 effect_type（u8）。
    // 客户端 `audio_control::effect_info_event` 就是按 `+0x20` 的 u8 选音效。
    let mut m = GraphwarEffectManager::new();
    for (i, t) in [EffectType::Explosion, EffectType::Death, EffectType::Spark, EffectType::Ash]
        .into_iter()
        .enumerate()
    {
        m.add_effect(100 + i as u32, (0.0, 0.0), t, 7);
    }
    assert_eq!(m.outbox.len(), 4);
    for i in 0..4u32 {
        assert_eq!(m.get(100 + i).unwrap().effect_type.code(), i as u8);
        assert_eq!(m.get(100 + i).unwrap().start_tick, 7);
    }
}
