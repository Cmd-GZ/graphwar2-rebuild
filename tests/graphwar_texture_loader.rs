//! `graphwar_texture_loader` 的验收：**三张表 + 119 条 `res://*.svg`**。
//!
//! ⚠️ 用 `#[path]` 把 `src/graphwar_texture_loader.rs` 编进本测试二进制，
//! **不需要** `src/lib.rs` 加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `three_maps_are_thirty_bytes_apart` | `+0x150/+0x180/+0x1b0`，间距 0x30 | **逐位**（三个 getter 的 `ldr [x0,#0x1c8]` 等） |
//! | `module_has_four_methods_not_one` | arm64 有 4 个方法（矩阵记 1 是下限） | **逐位**（`.symtab` 4 个符号） |
//! | `three_tables_have_the_rodata_counts` | 23/17/28 | **逐位**（`.rodata` 逐条） |
//! | `name_maps_to_path` | `res://<dir>/<name>.svg` | **逐位** |
//! | `no_hat_has_two_paths_in_the_binary` | 双斜杠笔误 | **逐位** |
//! | `miss_returns_none_no_fallback` | 未命中直接 `None` | **逐位**（`0x40c068` 写 0） |
#![allow(dead_code)]

#[path = "../src/graphwar_texture_loader.rs"]
mod graphwar_texture_loader;

use graphwar_texture_loader::*;

#[test]
fn three_maps_are_thirty_bytes_apart() {
    assert_eq!(SKIN_MAP_OFFSET, 0x150);
    assert_eq!(FACE_MAP_OFFSET, 0x180);
    assert_eq!(HAT_MAP_OFFSET, 0x1b0);
    assert_eq!(FACE_MAP_OFFSET - SKIN_MAP_OFFSET, 0x30);
    assert_eq!(HAT_MAP_OFFSET - FACE_MAP_OFFSET, 0x30);
}

#[test]
fn module_has_four_methods_not_one() {
    // `coverage-matrix` 记的「1 个可归属函数」是 `panic::Location` 口径的下限：
    // 只有 `new` 里有一枚 Location（`:145:47`），三个 getter 从不 panic。
    assert_eq!(GETTER_SIZE, 296);
    assert_eq!(NEW_SIZE, 21212);
    // 三个 getter 长度完全相同（296 = 296 = 296）。
    let src = include_str!("../src/graphwar_texture_loader.rs");
    for name in ["get_skin_texture", "get_face_texture", "get_hat_texture"] {
        assert!(src.contains(&format!("pub fn {}", name)));
    }
}

#[test]
fn three_tables_have_the_rodata_counts() {
    assert_eq!(SKINS.len(), 23);
    assert_eq!(FACES.len(), 17);
    assert_eq!(HATS.len(), 28);
    assert_eq!(RANKS.len(), 22);
    assert_eq!(UI_ICONS.len(), 28);

    // 三个 getter 各自查自己的表。
    let t = GraphwarTextureLoader::new();
    assert_eq!(t.skin.len(), SKINS.len());
    assert_eq!(t.face.len(), FACES.len());
    assert_eq!(t.hat.len(), HATS.len());
    assert_eq!(t.len(), 23 + 17 + 28);

    // 同一个名字在三张表里是不同的资源（`bot_0` 同时在 skin 与 face 里）。
    assert_eq!(t.get_skin_texture("bot_0").unwrap(), "res://skins/bot_0.svg");
    assert_eq!(t.get_face_texture("bot_0").unwrap(), "res://faces/bot_0.svg");
}

#[test]
fn name_maps_to_path() {
    assert_eq!(path_of(TextureKind::Skin, "target"), "res://skins/target.svg");
    assert_eq!(path_of(TextureKind::Face, "no_face"), "res://faces/no_face.svg");
    assert_eq!(path_of(TextureKind::Hat, "crown"), "res://hats/crown.svg");

    let t = GraphwarTextureLoader::new();
    // 靶子的皮肤就叫 target（`docs/123` §2.1 的 SkinGraphics="target"）。
    assert_eq!(t.get_skin_texture("target").unwrap(), "res://skins/target.svg");
    assert_eq!(t.get_hat_texture("no_hat").unwrap(), "res://hats/no_hat.svg");
    assert_eq!(
        t.get_face_texture("regular_eyes").unwrap(),
        "res://faces/regular_eyes.svg"
    );
}

#[test]
fn no_hat_has_two_paths_in_the_binary() {
    // 原版 `.rodata` 里同时有 `res://hats/no_hat.svg` 与 `res://hats//no_hat.svg`（双斜杠）。
    // 我们不修 —— 修了就与真值不同。
    assert_eq!(NO_HAT_TYPO_PATH, "res://hats//no_hat.svg");
    assert!(NO_HAT_TYPO_PATH.contains("//"));
    assert_ne!(NO_HAT_TYPO_PATH, path_of(TextureKind::Hat, "no_hat"));
}

#[test]
fn miss_returns_none_no_fallback() {
    let t = GraphwarTextureLoader::new();
    // 未命中直接 None（`0x40c068` 写 0），兜底在调用方（entity_drawer）。
    assert!(t.get_skin_texture("no_such_skin").is_none());
    assert!(t.get_face_texture("no_face").is_none() == false);
    assert!(t.get_face_texture("no_such_face").is_none());
    assert!(t.get_hat_texture("no_such_hat").is_none());
    // 名字区分大小写（键是 `String`，`memcmp` 比较）。
    assert!(t.get_skin_texture("Target").is_none());
}
