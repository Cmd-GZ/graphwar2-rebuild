//! `graphwar_color_manager` 的验收：**两张 6 色表 / 分配规则 / `ColorInfo` 的载荷 / 谁设颜色**。
//!
//! ⚠️ 用 `#[path]` 直接编进本测试二进制 ⇒ **不需要** `src/lib.rs` 里先加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `two_tables_are_exactly_docs_10_tables` | 12 个值逐元素等于 `docs/10` §10 的两张表 | **逐位**（arm64 `init_team_colors` 的 12 组 `strh`+`strb`） |
//! | `tables_are_disjoint` | 两表不相交、各 6 个 | **逐位** |
//! | `base_is_random_mod_six_then_probe_plus_one` | 起点 = `random::<u64>() % 6`，探测 `+1` 回绕 | **逐位**（`udiv`+`msub`、`cmp`+`csinc`） |
//! | `used_colors_are_skipped` | 已占用的颜色被跳过 | **逐位**（线性扫 `used` 的 3 字节） |
//! | `fallback_is_random_rgb_mod_180` | 6 个全占 ⇒ `(r%180,g%180,b%180)` | **逐位**（`and #0xff` / `cmp #0xb4` / `csel`） |
//! | `team_key_selects_the_pool` | 队号选池；没有该队 ⇒ 兜底 | **逐位**（`HashMap<u8,_>` 的键是 1 字节） |
//! | `set_color_overrides_and_broadcasts` | 服务端 (61) 到了就采纳，不查池子 | **逐位**（`set_color` 只有 emit + insert） |
//! | `color_info_payload_offsets` | 载荷 `+0x00` id、`+0x04..6` 颜色 | **逐位**（`str w1` / `strh w2` / `strb w9`） |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/graphwar_color_manager.rs"]
mod graphwar_color_manager;

use graphwar_color_manager::*;

/// `docs/10` §10 的两张表 —— 逐元素对拍（顺序就是内存里的顺序）。
#[test]
fn two_tables_are_exactly_docs_10_tables() {
    let team1: Vec<(u8, u8, u8)> = TEAM1_COLORS.iter().map(|c| c.triple()).collect();
    assert_eq!(
        team1,
        vec![
            (186, 42, 32),
            (238, 114, 0),
            (125, 104, 0),
            (122, 160, 0),
            (149, 5, 62),
            (234, 0, 219),
        ]
    );
    let team2: Vec<(u8, u8, u8)> = TEAM2_COLORS.iter().map(|c| c.triple()).collect();
    assert_eq!(
        team2,
        vec![
            (107, 0, 234),
            (17, 0, 108),
            (21, 83, 221),
            (0, 203, 163),
            (0, 116, 26),
            (2, 85, 63),
        ]
    );
    assert_eq!(TEAM_POOL_LEN, 6);
    assert_eq!((TEAM1_KEY, TEAM2_KEY), (0, 1));
}

/// 两表不相交（12 个值两两不同）——这是"同时活着的玩家颜色互不相同"的**唯一**原因。
#[test]
fn tables_are_disjoint() {
    let mut all: Vec<(u8, u8, u8)> = Vec::new();
    all.extend(TEAM1_COLORS.iter().map(|c| c.triple()));
    all.extend(TEAM2_COLORS.iter().map(|c| c.triple()));
    assert_eq!(all.len(), 12);
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 12, "两张表必须两两不同");
}

/// 起点是 `random::<u64>() % 6`，之后**沿表 +1 线性探测**（不是"下一个空闲下标"）。
#[test]
fn base_is_random_mod_six_then_probe_plus_one() {
    let mut m = GraphwarColorManager::new();

    // 起点 7 % 6 = 1 ⇒ Team1 的第 2 个。
    let mut rng = SeqRng::new(vec![7], vec![]);
    let e = m.add_team_color(1, TEAM1_KEY, &mut rng);
    assert_eq!(e.color.triple(), TEAM1_COLORS[1].triple());

    // 同一个起点再来一次：第 2 个已被占 ⇒ 探测到第 3 个（+1）。
    let mut rng = SeqRng::new(vec![7], vec![]);
    let e = m.add_team_color(2, TEAM1_KEY, &mut rng);
    assert_eq!(e.color.triple(), TEAM1_COLORS[2].triple());

    // 起点 5（= 最后一个）被占掉 ⇒ 探测回绕到 0（`csinc` 那条）。
    m.set_color(50, TEAM1_COLORS[5]);
    let mut rng = SeqRng::new(vec![5], vec![]);
    let e = m.add_team_color(3, TEAM1_KEY, &mut rng);
    assert_eq!(e.color.triple(), TEAM1_COLORS[0].triple());
}

/// `used` 里的颜色会被跳过（同一颜色的活人不会出现两次）。
#[test]
fn used_colors_are_skipped() {
    let mut m = GraphwarColorManager::new();
    // 占掉 Team1[0] 与 Team1[1]。
    m.set_color(100, TEAM1_COLORS[0]);
    m.set_color(101, TEAM1_COLORS[1]);
    // 起点 0 ⇒ 跳到 2。
    let mut rng = SeqRng::new(vec![0], vec![]);
    let e = m.add_team_color(1, TEAM1_KEY, &mut rng);
    assert_eq!(e.color.triple(), TEAM1_COLORS[2].triple());
    assert_eq!(m.color_of(1), Some(TEAM1_COLORS[2]));
    assert_eq!(m.len(), 3);
}

/// 6 个全被占 ⇒ 兜底 `(random::<u8>() % 180, ×3)`（不是回绕、不是换色相）。
#[test]
fn fallback_is_random_rgb_mod_180() {
    let mut m = GraphwarColorManager::new();
    for (i, c) in TEAM1_COLORS.iter().enumerate() {
        m.set_color(100 + i as u32, *c);
    }
    // 起点随意，但 6 个都被占 ⇒ 走到兜底；三个 u8 分别取模 180。
    let mut rng = SeqRng::new(vec![0], vec![181, 0, 200]);
    let e = m.add_team_color(1, TEAM1_KEY, &mut rng);
    assert_eq!(e.color.triple(), (1, 0, 20), "181%180=1、0%180=0、200%180=20");
    // 兜底色同样写进占用表，但**不一定**与池子里的颜色不同（原版不判重）。
    assert_eq!(m.color_of(1), Some(e.color));
}

/// 队号选池：`0` → Team1、`1` → Team2；没有这个队 ⇒ 兜底随机色。
#[test]
fn team_key_selects_the_pool() {
    let mut m = GraphwarColorManager::new();
    let mut rng = SeqRng::new(vec![0, 0], vec![10, 20, 30]);
    let a = m.add_team_color(1, TEAM1_KEY, &mut rng);
    let b = m.add_team_color(2, TEAM2_KEY, &mut rng);
    assert_eq!(a.color.triple(), TEAM1_COLORS[0].triple());
    assert_eq!(b.color.triple(), TEAM2_COLORS[0].triple());

    // 队号 2 不存在 ⇒ 直接兜底（三个 u8 各 %180）。
    let mut rng = SeqRng::new(vec![], vec![10, 20, 30]);
    let c = m.add_team_color(3, 2, &mut rng);
    assert_eq!(c.color.triple(), (10, 20, 30));
    assert!(m.pool(2).is_none());
    assert_eq!(m.pool(TEAM1_KEY).map(|p| p.len()), Some(6));
}

/// 服务端 (61) 到了就采纳（`set_color` 不查池子、不判重），并且可以覆盖同一个实体。
#[test]
fn set_color_overrides_and_broadcasts() {
    let mut m = GraphwarColorManager::new();
    let e = m.set_color(5, GraphwarColor::new(1, 2, 3));
    assert_eq!((e.entity_id, e.color.triple()), (5, (1, 2, 3)));
    // 覆盖成池子外的颜色也必须原样接受。
    let e = m.set_color(5, GraphwarColor::new(9, 9, 9));
    assert_eq!(e.color.triple(), (9, 9, 9));
    assert_eq!(m.len(), 1);
    // 广播与占用表一致。
    assert_eq!(m.send_all_info_events(), vec![e]);
    // 删实体 ⇒ 颜色回到池子（表里没了）。
    assert!(m.handle_entity_removed_event(5));
    assert!(!m.handle_entity_removed_event(5));
    assert!(m.is_empty());
}

/// `ColorInfo`(61) 的载荷：`+0x00` = `entity_id`，`+0x04..+0x06` = r/g/b。
#[test]
fn color_info_payload_offsets() {
    assert_eq!(ColorInfo::PAYLOAD_SIZE, 8);
    assert_eq!(
        (
            COLOR_INFO_ENTITY_OFFSET,
            COLOR_INFO_R_OFFSET,
            COLOR_INFO_G_OFFSET,
            COLOR_INFO_B_OFFSET
        ),
        (0x00, 0x04, 0x05, 0x06)
    );
    assert_eq!(ColorInfo::tag_word(), 0x8000_0000_0000_003d);
    // 表基与 hasher：`used` @0x18（hasher +0x20）、`pools` @0x48（hasher +0x20）。
    assert_eq!((USED_MAP_OFFSET, POOL_MAP_OFFSET), (0x18, 0x48));
    assert_eq!((USED_MAP_HASHER_OFFSET, POOL_MAP_HASHER_OFFSET), (0x20, 0x20));
    // 颜色是 3 字节，字段序 r/g/b（`Serialize` 的三个字段名）。
    assert_eq!(GraphwarColor::SIZE, 3);
    assert_eq!(GraphwarColor::FIELD_NAMES, ["r", "g", "b"]);
}

/// 判别值与项目事件表一致；两条订阅与 `docs/10` §10 的"重连即重播"对得上。
#[test]
fn tags_match_the_event_table() {
    assert_eq!(EVENT_COLOR_INFO, events::Event::ColorInfo as u16);
    assert_eq!(SUBSCRIPTIONS[0].tag, events::Event::NewConnection as u16);
    assert_eq!(SUBSCRIPTIONS[1].tag, events::Event::EntityRemoved as u16);
    assert_eq!(SUBSCRIPTIONS[0].handler, "send_all_info_events");
    assert_eq!(SUBSCRIPTIONS[1].handler, "handle_entity_removed_event");
    assert_eq!(RANDOM_COLOR_MODULO, 180);
}
