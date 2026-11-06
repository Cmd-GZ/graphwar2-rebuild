//! `graphwar_graphics_manager` 的验收：**外观索引的三张表**，以及
//! **`GraphwarTextureLoader` 不在这个模块**（这条本身就是验收项）。
//!
//! ⚠️ 用 `#[path]` 直接编进本测试二进制 ⇒ **不需要** `src/lib.rs` 里先加 `pub mod`；
//! 两份源码只依赖 `std`。父 agent 加上 `pub mod` 后可改成
//! `use gw2_rebuild::graphwar_graphics_manager::*;` 而不改断言。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `two_subscriptions` | `new_connection_event`/`entity_removed_event` → 两个方法 | **逐位**（两个闭包的 `adr` 目标） |
//! | `tags_match_the_event_table` | 51/53/55/5/14 与 `events.rs` 一致 | **逐位**（两侧独立） |
//! | `three_maps_are_thirty_bytes_apart` | `+0x18/+0x48/+0x78`，步长 0x30 | **逐位**（三个 setter 与 remove 的偏移） |
//! | `send_all_replays_skin_face_hat` | 三个循环、判别值 `0x33|k` | **逐位**（`tag | 4`） |
//! | `entity_removed_clears_all_three` | 三张表各删一次 | **观测**（`hash(self+0x38/0x68/0x98)` + `remove`） |
//! | `texture_loader_is_its_own_module` | loader 的三张表在 `+0x150/+0x180/+0x1b0`、表项 0x28 | **逐位**（三个 getter 的探测循环） |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/graphwar_graphics_manager.rs"]
mod graphwar_graphics_manager;

use graphwar_graphics_manager::*;

/// 两条订阅，处理方法分别是"全量重播"与"删实体"。
#[test]
fn two_subscriptions() {
    assert_eq!(SUBSCRIPTIONS.len(), 2);
    assert_eq!(SUBSCRIPTIONS[0].name, "new_connection_event");
    assert_eq!(SUBSCRIPTIONS[0].handler, "send_all_info_events");
    assert_eq!(SUBSCRIPTIONS[1].name, "entity_removed_event");
    assert_eq!(SUBSCRIPTIONS[1].handler, "handle_entity_removed_event");
    // 两个订阅名都是 20 字节（`mov w2, #0x14`）。
    assert_eq!(SUBSCRIPTIONS[0].name.len(), 0x14);
    assert_eq!(SUBSCRIPTIONS[1].name.len(), 0x14);
}

/// 三个 info 判别值与项目事件表、以及两条订阅的判别值。
#[test]
fn tags_match_the_event_table() {
    assert_eq!(GraphicsKind::Skin.tag(), events::Event::SkinGraphicsInfo as u16);
    assert_eq!(GraphicsKind::Face.tag(), events::Event::FaceGraphicsInfo as u16);
    assert_eq!(GraphicsKind::Hat.tag(), events::Event::HatGraphicsInfo as u16);
    assert_eq!(SUBSCRIPTIONS[0].tag, events::Event::NewConnection as u16);
    assert_eq!(SUBSCRIPTIONS[1].tag, events::Event::EntityRemoved as u16);

    assert_eq!(
        GraphicsKind::ALL.map(|k| k.tag()),
        [51, 53, 55],
        "0x33 / 0x35 / 0x37"
    );
    assert_eq!(
        GraphicsKind::ALL.map(|k| k.event_name()),
        [
            "skin_graphics_info_event",
            "face_graphics_info_event",
            "hat_graphics_info_event"
        ]
    );
}

/// `send_all_info_events` 用 `0x8000…0033 + k`（k = 0/2/4）造三个判别值。
#[test]
fn tag_words_are_base_or_bits() {
    assert_eq!(GraphicsKind::Skin.tag_bits(), 0x8000_0000_0000_0033);
    assert_eq!(GraphicsKind::Face.tag_bits(), 0x8000_0000_0000_0035);
    assert_eq!(GraphicsKind::Hat.tag_bits(), 0x8000_0000_0000_0037);
    let info = GraphicsInfo {
        kind: GraphicsKind::Hat,
        player_id: 3,
        name: "top_hat".into(),
    };
    assert_eq!(info.tag(), 55);
    assert_eq!(info.tag_word(), 0x8000_0000_0000_0037);
}

/// 三张表 `+0x18 / +0x48 / +0x78`，步长 0x30，`items`/hasher 在表基 +0x18/+0x20。
#[test]
fn three_maps_are_thirty_bytes_apart() {
    assert_eq!(
        GraphicsKind::ALL.map(|k| k.map_offset()),
        [0x18, 0x48, 0x78]
    );
    assert_eq!(MAP_STRIDE, 0x30);
    assert_eq!(MAP_ITEMS_OFFSET, 0x18);
    assert_eq!(MAP_HASHER_OFFSET, 0x20);
    // remove 支用的 hasher 地址 = 表基 + 0x20 ⇒ 0x38 / 0x68 / 0x98。
    assert_eq!(
        GraphicsKind::ALL.map(|k| k.map_offset() + MAP_HASHER_OFFSET),
        [0x38, 0x68, 0x98]
    );
    assert_eq!(GraphicsKind::ID_OFFSET, 0x20);
    assert_eq!(NAME_OFFSET, 0x08);
}

/// 三个 setter 各写自己的表、各发自己的事件。
#[test]
fn setters_write_their_own_table() {
    let mut g = GraphwarGraphicsManager::new();

    let e = g.set_skin_graphics(7, "skin_3");
    assert_eq!((e.kind, e.player_id, e.name.as_str()), (GraphicsKind::Skin, 7, "skin_3"));

    let e = g.set_face_graphics(7, "face_5");
    assert_eq!((e.kind, e.player_id, e.name.as_str()), (GraphicsKind::Face, 7, "face_5"));

    let e = g.set_hat_graphics(7, "hat_1");
    assert_eq!((e.kind, e.player_id, e.name.as_str()), (GraphicsKind::Hat, 7, "hat_1"));

    assert_eq!(g.skins.get(&7).map(String::as_str), Some("skin_3"));
    assert_eq!(g.faces.get(&7).map(String::as_str), Some("face_5"));
    assert_eq!(g.hats.get(&7).map(String::as_str), Some("hat_1"));

    assert_eq!(
        g.graphics_of(7),
        [Some("skin_3".into()), Some("face_5".into()), Some("hat_1".into())]
    );
    assert_eq!(g.graphics_of(8), [None, None, None]);
}

/// 同名玩家被覆盖（`map.insert` 语义，不是 append）。
#[test]
fn set_overwrites() {
    let mut g = GraphwarGraphicsManager::new();
    g.set_skin_graphics(1, "a");
    g.set_skin_graphics(1, "b");
    assert_eq!(g.skins.len(), 1);
    assert_eq!(g.skins.get(&1).map(String::as_str), Some("b"));
}

/// `send_all_info_events`：皮肤 → 脸 → 帽子，每条一个事件，`flag` 无关。
#[test]
fn send_all_replays_skin_face_hat() {
    let mut g = GraphwarGraphicsManager::new();
    g.set_skin_graphics(1, "s1");
    g.set_skin_graphics(2, "s2");
    g.set_face_graphics(1, "f1");
    g.set_hat_graphics(1, "h1");
    g.set_hat_graphics(3, "h3");

    let all = g.send_all_info_events();
    assert_eq!(all.len(), 5);

    let kinds: Vec<GraphicsKind> = all.iter().map(|i| i.kind).collect();
    assert_eq!(
        kinds,
        vec![
            GraphicsKind::Skin,
            GraphicsKind::Skin,
            GraphicsKind::Face,
            GraphicsKind::Hat,
            GraphicsKind::Hat
        ],
        "三个循环的顺序是 皮肤 → 脸 → 帽子"
    );
    let tags: Vec<u16> = all.iter().map(|i| i.tag()).collect();
    assert_eq!(tags, vec![51, 51, 53, 55, 55]);
}

/// `handle_entity_removed_event`：三张表各删一次。
#[test]
fn entity_removed_clears_all_three() {
    let mut g = GraphwarGraphicsManager::new();
    g.set_skin_graphics(9, "s");
    g.set_face_graphics(9, "f");
    g.set_hat_graphics(9, "h");
    g.set_skin_graphics(10, "s10");

    assert_eq!(g.handle_entity_removed_event(9), 3);
    assert_eq!(g.graphics_of(9), [None, None, None]);
    assert_eq!(g.skins.len(), 1);
    assert_eq!(g.handle_entity_removed_event(9), 0);
    assert_eq!(g.handle_entity_removed_event(10), 1);
}

/// **`GraphwarTextureLoader` 不在本模块**：它是 `graphwar_texture_loader`，
/// 三张 `HashMap<String, Gd<Texture2D>>` 在 `+0x150/+0x180/+0x1b0`，表项 0x28 字节。
#[test]
fn texture_loader_is_its_own_module() {
    assert_eq!(TEXTURE_MAP_OFFSETS, [0x150, 0x180, 0x1b0]);
    assert_eq!(TEXTURE_ENTRY_SIZE, 0x28);
    // 三张表同样间距 0x30（与本模块的三张表同构）。
    for w in TEXTURE_MAP_OFFSETS.windows(2) {
        assert_eq!(w[1] - w[0], MAP_STRIDE);
    }

    // 三个 getter 的表偏移与 `TextureGetter` 表一致。
    let offs: Vec<usize> = TEXTURE_GETTERS.iter().map(|g| g.map_offset).collect();
    assert_eq!(offs, TEXTURE_MAP_OFFSETS.to_vec());
    // getter 名字按 皮肤 → 脸 → 帽子 的顺序，与本模块三类外观一一对应。
    let names: Vec<&str> = TEXTURE_GETTERS.iter().map(|g| g.name).collect();
    assert_eq!(
        names,
        vec!["get_skin_texture", "get_face_texture", "get_hat_texture"]
    );
    let kinds: Vec<GraphicsKind> = TEXTURE_GETTERS.iter().map(|g| g.kind).collect();
    assert_eq!(kinds, GraphicsKind::ALL.to_vec());
    for g in TEXTURE_GETTERS {
        assert_eq!(g.arm64_size, 296);
        assert!(g.arm64_addr >= 0x40_0000);
    }

    assert_eq!(TEXTURE_LOADER_CTOR_SITE, 0x39a328);
    assert_eq!(TEXTURE_LOADER_OWNER_OFFSET, 0x16a0);
    // 饿汉式：`new` 21 KB 一口气读完。
    assert_eq!(TEXTURE_LOADER_NEW.1, 21212);
    assert!(TEXTURE_LOADER_NEW.1 > 4 * 4096);
}
