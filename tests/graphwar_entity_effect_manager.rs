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
//! | `soldier_birth_registers_by_skin_prefix` | 士兵出生：空/人类皮肤 ⇒ `Death`，`bot_*` ⇒ `Ash` | **逐位**（`0x4ab09c`/`0x4ab120`/`0x4ab270` 的 `mov w2,#1/#3`） |
//! | `target_birth_registers_spark` | 靶子出生 ⇒ `Spark` | **逐位**（`0x4a58d0` 的 `mov w2,#0x2`） |
//! | `late_skin_only_rewrites_on_the_bot_prefix` | 迟到的皮肤只有 `bot_*` 改写 | **逐位**（`0x4aa358` 的 `b.ne`） |
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

// ---------------------------------------------------------------------------
// 5. **登记点**：谁在什么时候写进这张表（`docs/16` §37，2026-11-07 新解）
// ---------------------------------------------------------------------------

/// 士兵出生：`add_soldier_to_player`（arm64 `0x4aaab4`）里三次内联登记。
///
/// 空皮肤 ⇒ `DeathEffect`（`0x4ab09c`）；`bot_` 前缀 ⇒ `AshEffect`（`0x4ab270`）；
/// 其余 ⇒ `DeathEffect`（`0x4ab120`）。三条都是"事件载荷字节 + `HashMap::insert`
/// 的值"两处立即数互校，见模块文档 §6.1。
#[test]
fn soldier_birth_registers_by_skin_prefix() {
    assert_eq!(BOT_SKIN_PREFIX, "bot_");
    assert_eq!(BOT_SKIN_PREFIX.as_bytes(), &[0x62, 0x6f, 0x74, 0x5f]);

    let mut m = GraphwarEntityEffectManager::new();
    m.register_soldier(1, None);
    m.register_soldier(2, Some(""));
    m.register_soldier(3, Some("yellow"));
    m.register_soldier(4, Some("bot_"));
    m.register_soldier(5, Some("bot_0"));
    m.register_soldier(6, Some("bot_5"));

    assert_eq!(m.death_effect_of(1), Some(EffectType::Death), "没有皮肤信息 ⇒ Death");
    assert_eq!(m.death_effect_of(2), Some(EffectType::Death), "空串 ⇒ Death");
    assert_eq!(m.death_effect_of(3), Some(EffectType::Death), "人类皮肤 ⇒ Death");
    // 长度闸是 `cmp x8,#0x3 / b.hi`：**恰好 4 字节**就算过；"bot_" 本身已满足前缀。
    assert_eq!(m.death_effect_of(4), Some(EffectType::Ash));
    assert_eq!(m.death_effect_of(5), Some(EffectType::Ash));
    assert_eq!(m.death_effect_of(6), Some(EffectType::Ash));
    assert_eq!(m.len(), 6);
}

/// 靶子出生：`add_target`（`0x4a5704`）恒登记 `SparkEffect`（`0x4a58d0` 的 `mov w2, #0x2`）。
#[test]
fn target_birth_registers_spark() {
    assert_eq!(TARGET_SKIN, "target");
    let mut m = GraphwarEntityEffectManager::new();
    m.register_target(350);
    assert_eq!(m.death_effect_of(350), Some(EffectType::Spark));
}

/// 迟到的皮肤：`handle_graphics_info`（`0x4aa304`）**只有 `bot_` 前缀**会改写，
/// 其余名字原版直接 `b.ne` 跳出去、什么都不做。
#[test]
fn late_skin_only_rewrites_on_the_bot_prefix() {
    let mut m = GraphwarEntityEffectManager::new();
    m.register_soldier(880, None);
    assert_eq!(m.death_effect_of(880), Some(EffectType::Death));

    assert!(!m.apply_skin_graphics(880, "yellow"), "人类皮肤不该改写");
    assert!(!m.apply_skin_graphics(880, "target"), "靶子皮肤不该改写");
    assert!(!m.apply_skin_graphics(880, "bot"), "短于 4 字节不该改写（`cmp x8,#0x3`）");
    assert_eq!(m.death_effect_of(880), Some(EffectType::Death));

    assert!(m.apply_skin_graphics(880, "bot_0"), "bot_0 必须改写");
    assert_eq!(m.death_effect_of(880), Some(EffectType::Ash));
    // 未登记的实体也会被这条路径写进去（`handle_graphics_info` 不看表里有没有）
    assert!(m.apply_skin_graphics(881, "bot_2"));
    assert_eq!(m.death_effect_of(881), Some(EffectType::Ash));
}
