//! `graphwar_ban_list` / `graphwar_version` 的验收（T109 落码的两个**分母外模块**）。
//!
//! 两个模块此前连 `docs/coverage-matrix.tsv` 都没有行（`panic::Location` = 0 条 ⇒
//! Location 口径的模块清单看不见它们）。本文件钉住的是**指令级事实**：
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `ban_list_size_matches_module_registry` | `size_of` 与注册表 `size: 96` 一致 | **观测**（`0x408460` 的字段偏移 + vtable `size`） |
//! | `empty_ban_list_bans_nobody` | 表 B `items==0` ⇒ false | **观测**（`0x408470 ldur/cbz`） |
//! | `unknown_connection_is_not_banned` | 表 B 查不到 ⇒ false | **观测**（探测循环未命中） |
//! | `connection_known_but_table_a_empty` | 表 A `items==0` ⇒ false | **观测**（`0x408524 ldr/cbz`） |
//! | `banned_only_when_both_tables_hit` | 两表都命中 ⇒ true | **观测**（`0x40868c mov w0,#1`） |
//! | `ban_target_variants_are_distinct` | 两个变体不相等、可作键 | **观测**（判定位 LSB） |
//! | `full_version_is_the_compile_time_literal` | `"2.0.60"` | **观测**（`0x391410/0x39140c/0x391448/0x391474`） |

#![allow(dead_code)]

use gw2_rebuild::graphwar_ban_list::{BanTarget, GraphwarBanList, RC_INNER_SIZE, SIZE};
use gw2_rebuild::graphwar_module_container;
use gw2_rebuild::graphwar_version::{graphwar_version_get_full_version, FULL_VERSION};

#[test]
fn ban_list_size_matches_module_registry() {
    // 一条来自本模块指令（字段偏移算出 0x60），一条来自注册表 vtable 的 size 字 —— 两条独立证据。
    assert_eq!(SIZE, 96);
    let entry = graphwar_module_container::find("GraphwarBanList").expect("注册表里应有 GraphwarBanList");
    assert_eq!(entry.size as usize, SIZE, "注册表 size 必须与本模块算出的 0x60 相同");
    assert_eq!(entry.module_path, "graphwar_ban_list");
    // 原版 `new` 分配的 120 = RcInner<RefCell<..>> = 8(strong)+8(weak)+8(borrow)+96(value)
    assert_eq!(RC_INNER_SIZE, 0x78);
    assert_eq!(RC_INNER_SIZE, 16 + 8 + SIZE);
}

#[test]
fn empty_ban_list_bans_nobody() {
    let list = GraphwarBanList::new();
    // 0x408470 ldr x8,[x0,#0x48]; cbz x8 → 0x408678(mov w0,wzr)
    assert!(!list.is_connection_banned(0));
    assert!(!list.is_connection_banned(7));
    assert!(!list.is_connection_banned(u32::MAX));
}

#[test]
fn unknown_connection_is_not_banned() {
    let mut list = GraphwarBanList::new();
    list.banned.insert(BanTarget::Word(1));
    list.by_connection.insert(42, BanTarget::Word(1));
    // 表 B 非空，但查不到 43 ⇒ false（探测循环走到底 → 0x408678）
    assert!(list.is_connection_banned(42));
    assert!(!list.is_connection_banned(43));
}

#[test]
fn connection_known_but_table_a_empty() {
    let mut list = GraphwarBanList::new();
    // 只有表 B（`+0x30`）有内容，表 A（`+0x00`）为空 ⇒ 0x408524 的空判直接 false
    list.by_connection.insert(42, BanTarget::Pair(0x11, 0x22));
    assert!(!list.is_connection_banned(42));
}

#[test]
fn banned_only_when_both_tables_hit() {
    let mut list = GraphwarBanList::new();
    list.by_connection.insert(42, BanTarget::Pair(0x11, 0x22));
    list.banned.insert(BanTarget::Pair(0x11, 0x22));
    assert!(list.is_connection_banned(42));

    // 表 A 里只有**别的**值 ⇒ 不命中 ⇒ false
    let mut other = GraphwarBanList::new();
    other.by_connection.insert(42, BanTarget::Pair(0x11, 0x22));
    other.banned.insert(BanTarget::Pair(0x11, 0x23));
    assert!(!other.is_connection_banned(42));

    // `Word` 变体同样能命中（两个变体共用同一条"两表都命中"路径）
    let mut word = GraphwarBanList::new();
    word.by_connection.insert(7, BanTarget::Word(9));
    word.banned.insert(BanTarget::Word(9));
    assert!(word.is_connection_banned(7));
}

#[test]
fn ban_target_variants_are_distinct() {
    // 判定位（value+0 的最低位）决定读 4 字节还是 16 字节 ⇒ 两类值不可能相等
    assert_ne!(BanTarget::Word(0), BanTarget::Pair(0, 0));
    assert_eq!(BanTarget::Word(5), BanTarget::Word(5));
    assert_eq!(BanTarget::Pair(1, 2), BanTarget::Pair(1, 2));
    // 二者的名字**不是原版名字**（原版名字无证据），只是形状占位 —— 见模块文档 §3.1
}

#[test]
fn full_version_is_the_compile_time_literal() {
    // "2.0.60" 的 6 个字节逐个来自立即数：'2' 0x32 / '.' 0x2e / '0' 0x30 / '.' 0x2e / '6' 0x36 / '0' 0x30
    assert_eq!(FULL_VERSION, "2.0.60");
    assert_eq!(FULL_VERSION.len(), 6);
    assert_eq!(graphwar_version_get_full_version(), FULL_VERSION);
    let s = graphwar_version_get_full_version();
    assert_eq!(s.as_bytes(), &[0x32, 0x2e, 0x30, 0x2e, 0x36, 0x30]);
}
