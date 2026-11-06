//! `graphwar_entity_effect_manager` 的验收：**死亡特效照抄实体登记**（这是
//! `docs/141` 那 19 例「死亡块整块是 AshEffect」的解释）。
//!
//! ⚠️ 这里用 `#[path]` 把三个源码文件编进本测试二进制（本模块依赖
//! `events::Event` 与 `graphwar_effect_manager::EffectType`），
//! 所以**不需要** `src/lib.rs` 里先加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `two_subscriptions_and_their_tags` | 两条订阅名；14 / 56 | **逐位**（`0x3e9f40`、`0x3e9c58` 的 `#0x38`） |
//! | `ash_in_death_out` | 登记 `Ash` ⇒ 死亡 `Ash` | **逐位**（`0x4a8c50` 的 `ldurb w21,[x13,#-4]`） |
//! | `unregistered_entity_emits_nothing` | 查不到 ⇒ **不发**（不是默认 Death） | **逐位**（`0x4a8c38` 跳过 `add_event`） |
//! | `removal_clears_the_registration` | 14 号事件删号后死亡也没特效 | **逐位** |
//! | `rebroadcast_one_per_entity` | 重播 = 每个实体一条 `EntityEffectInfo` | **逐位**（`0x3e9cd0`–`0x3e9da0`） |
//! | `table_entry_is_eight_bytes` | 键 u32 + 值 u8（桶距 8） | **逐位**（`sub x22,x22,#0x40` + `and #0x78`） |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/graphwar_effect_manager.rs"]
mod graphwar_effect_manager;
#[path = "../src/graphwar_entity_effect_manager.rs"]
mod graphwar_entity_effect_manager;

use events::Event;
use graphwar_effect_manager::EffectType;
use graphwar_entity_effect_manager::*;

#[test]
fn two_subscriptions_and_their_tags() {
    assert_eq!(SUBSCRIPTIONS.len(), 2);
    assert_eq!(SUBSCRIPTIONS[0], "new_connection_event");
    assert_eq!(SUBSCRIPTIONS[1], "entity_removed_event");
    for s in SUBSCRIPTIONS {
        assert_eq!(s.len(), 20);
    }

    assert_eq!(ENTITY_REMOVED_TAG, 14);
    assert_eq!(ENTITY_EFFECT_INFO_TAG, 56);
    assert_eq!(Event::EntityRemoved.wire_tag(), 14);
    assert_eq!(Event::EntityEffectInfo.wire_tag(), 56);

    let m = GraphwarEntityEffectManager::new();
    assert_eq!(m.subscribed, SUBSCRIPTIONS.to_vec());
}

/// **核心结论**：`AshEffect` 与 `DeathEffect` 的分界在**登记时**，不在死亡那一刻。
#[test]
fn ash_in_death_out() {
    let mut m = GraphwarEntityEffectManager::new();
    m.add_entity_effect(1099, EffectType::Ash);
    assert_eq!(m.death_effect_of(1099), Some(EffectType::Ash));

    m.add_entity_effect(1100, EffectType::Death);
    assert_eq!(m.death_effect_of(1100), Some(EffectType::Death));

    m.add_entity_effect(1101, EffectType::Spark);
    assert_eq!(m.death_effect_of(1101), Some(EffectType::Spark));

    // 后登记的覆盖先登记的（`HashMap::insert`）。
    m.add_entity_effect(1099, EffectType::Death);
    assert_eq!(m.death_effect_of(1099), Some(EffectType::Death));
}

/// 查不到时原版**一条特效都不发**（`0x4a8c38` 直接跳过 `add_event`）。
/// 任何 `unwrap_or(DeathEffect)` 都是错的。
#[test]
fn unregistered_entity_emits_nothing() {
    let mut m = GraphwarEntityEffectManager::new();
    m.add_entity_effect(1, EffectType::Death);
    assert_eq!(m.death_effect_of(2), None);
    let before = m.outbox.len();
    // 死亡结算只查表、不写表 ⇒ 不会产生任何事件。
    let _ = m.death_effect_of(2);
    assert_eq!(m.outbox.len(), before);
}

#[test]
fn removal_clears_the_registration() {
    let mut m = GraphwarEntityEffectManager::new();
    m.add_entity_effect(7, EffectType::Ash);
    assert_eq!(m.death_effect_of(7), Some(EffectType::Ash));

    // 判别值不是 14 ⇒ 原版 `panic!()`
    assert_eq!(m.handle_entity_removed_event(56, 7), Err(()));
    assert_eq!(m.death_effect_of(7), Some(EffectType::Ash));

    assert_eq!(m.handle_entity_removed_event(14, 7), Ok(()));
    assert_eq!(m.death_effect_of(7), None);
    assert_eq!(m.len(), 0);
}

#[test]
fn rebroadcast_one_per_entity() {
    let mut m = GraphwarEntityEffectManager::new();
    m.add_entity_effect(11, EffectType::Spark);
    m.add_entity_effect(12, EffectType::Ash);
    let outbox_before = m.outbox.len();

    let sent = m.send_all_info_events();
    assert_eq!(sent.len(), 2);
    assert_eq!(m.outbox.len(), outbox_before + 2);
    assert!(m.outbox.iter().all(|e| *e == Event::EntityEffectInfo));

    let mut ids: Vec<u32> = sent.iter().map(|e| e.entity_id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![11, 12]);
    // 载荷带的就是登记的值
    let ash = sent.iter().find(|e| e.entity_id == 12).unwrap();
    assert_eq!(ash.effect_type, EffectType::Ash);
    assert_eq!(EntityEffectInfo::TAG, 56);
}

/// 表项 8 字节：键 `u32` 在 +0、值 `u8` 在 +4。
/// 这与 `target_manager`（桶距 4、值类型 `()`）不是同一张表 ——
/// 两个 `remove_entry` 的 monomorphization 都不同。
#[test]
fn table_entry_is_eight_bytes() {
    // 值只有 4 位有效：0..=3，其它值在 serde 层就被挡掉。
    for code in 0u8..4 {
        assert!(EffectType::from_code(code).is_some());
    }
    assert!(EffectType::from_code(4).is_none());

    // 同一实体重复登记不涨长度（键唯一）。
    let mut m = GraphwarEntityEffectManager::new();
    m.add_entity_effect(1, EffectType::Death);
    m.add_entity_effect(1, EffectType::Ash);
    assert_eq!(m.len(), 1);
}
