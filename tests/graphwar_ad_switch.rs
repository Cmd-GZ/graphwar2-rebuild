//! `graphwar_ad_switch.rs` 的复验收据（逐条对拍 `src/graphwar_ad_switch.rs`）。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//!
//! 地址口径：本文件的地址是 **ELF 虚地址**（`objdump` 口径，Ghidra = +`0x100000`）；
//! 引自 `docs/19-android.md` 的地址是 **Android arm64**，下文标明。

use gw2_rebuild::graphwar_ad_switch as ads;

/// 判据 1：`are_ads_enabled() = !owned`，且 `owned` 初值 `true`。
///
/// 真值：arm64 ptrcall 实例 `0x377cf4` = `ldrb w22,[obj+0x18]` → `eor w8,w22,#1`
/// → 写回 out（`docs/19-android.md` §3.0.3）；初值来自 create 里写 `1`
/// （`docs/19` §2.10②，跨类判别器证明该槽是"按类不同的实例字段"）。
#[test]
fn ads_enabled_is_not_owned_with_default_true() {
    assert_eq!(ads::DEFAULT_OWNED, true);
    assert!(ads::ads_enabled_from_owned(false));
    assert!(!ads::ads_enabled_from_owned(true));

    let s = ads::AdSwitch::new();
    assert_eq!(s.owned(), true);
    assert_eq!(s.are_ads_enabled(), false, "首次启动：广告关闭");
    assert_eq!(ads::AdSwitch::default(), s);
    assert_eq!(ads::AdSwitch::with_owned(false).are_ads_enabled(), true);
}

/// 判据 2：`owned` 在实例结构里的偏移是 `+0x18`。
///
/// 真值：`docs/19` §2.10① —— gdext 的实例结构偏移定标 + 跨类判别器
/// （另一个 Control 派生类在同槽写 `3`）。
#[test]
fn owned_field_offset_is_0x18() {
    assert_eq!(ads::OWNED_FIELD_OFFSET, 0x18);
}

/// 判据 3：`get_ad_unit_id()` 返回 38 字节字面量，且**唯一**引用它的是
/// `get_ad_unit_id` 的两个 trampoline。
///
/// 真值：`.rodata` `0x1c43ff`，长度 38（`docs/19` §3.0.3 的 arm64 地址是
/// `0x19dd6c`，长度同为 38）；`0x42e500` / `0x42e920` 两处 `lea` 指向它，
/// 而这两个函数正是 `0x4f0430` / `0x4f0370` 两个注册闭包调用的 trampoline。
#[test]
fn ad_unit_id_is_38_bytes_and_points_at_the_test_unit() {
    assert_eq!(ads::AD_UNIT_ID.len(), 38);
    assert_eq!(ads::AD_UNIT_ID.len() as u32, ads::AD_UNIT_ID_LEN);
    assert_eq!(
        ads::AD_UNIT_ID,
        "ca-app-pub-3940256099942544/6300978111",
        "Google 官方测试单元，原版没有换正式 id"
    );
    assert_eq!(ads::AdSwitch::new().get_ad_unit_id(), ads::AD_UNIT_ID);
    // 引用这条字面量的两条指令就是 get_ad_unit_id 的两个 trampoline
    assert_eq!(ads::AD_UNIT_ID_REFS, [0x42e500, 0x42e920]);
    assert_eq!(ads::AD_UNIT_ID_REFS, [ads::EXPORTS[1].trampoline_b, ads::EXPORTS[1].trampoline_a]);
}

/// 判据 4：全二进制只有 **6 条** `FUNC` 属于本模块，且形状都是 gdext 惰性注册驱动。
///
/// 真值：`docs/19` §3.0.3 明写「`graphwar_ad_switch` 在全二进制里只有 6 条 `FUNC`
/// 符号」；x86 侧用 `panic::Location` 归属也正好 6 个（`14:1` ×4、`7:10`、`45:1`）。
#[test]
fn six_functions_all_gdext_registration_drivers() {
    assert_eq!(ads::FUNCTIONS.len(), 6);
    let mut addrs: Vec<u64> = ads::FUNCTIONS.iter().map(|f| f.vaddr).collect();
    addrs.sort_unstable();
    assert_eq!(
        addrs,
        vec![0x4e5100, 0x4e5280, 0x4e9730, 0x4e98e0, 0x4e9af0, 0x4e9c90]
    );
    for f in ads::FUNCTIONS.iter() {
        assert_eq!(ads::ghidra(f.vaddr), f.vaddr + 0x100000);
        // 每个都引用了本文件的 Location（归属证据）
        assert!(f.anchor.is_some());
    }
    // 只有三行 Location ⇒ 原版这个文件很短（docs/02 §18 A 表：7–45 行）
    assert_eq!(ads::ANCHOR_LINES, [(7, 10), (14, 1), (45, 1)]);
}

/// 判据 5：这个类对外只有两个 getter，没有 setter。
///
/// 真值：注册闭包只注册了 `are_ads_enabled` / `get_ad_unit_id`
/// （`.rodata` `0x1c4425` 长度 15 / `0x1c4434` 长度 14）；
/// 4 个闭包的 trampoline 各两个，成对出现（ptrcall + varcall）。
#[test]
fn two_exports_only_getters() {
    let names: Vec<&str> = ads::EXPORTS.iter().map(|e| e.name).collect();
    assert_eq!(names, ["are_ads_enabled", "get_ad_unit_id"]);
    for e in ads::EXPORTS.iter() {
        assert_eq!(e.name.len() as u32, e.name_len, "{}", e.name);
        assert!(ads::export(e.name).is_some());
    }
    // 没有 set_owned / set_ads_enabled 之类
    for absent in ["set_owned", "set_ads_enabled", "unlock", "lock"] {
        assert!(ads::export(absent).is_none(), "{absent} 不应存在");
    }
}

/// 判据 6：`owned` 只能被 `GraphwarRootNode` 的两个购买回调改。
///
/// 真值：`docs/19` §2.10③ —— 穷举全 `.text` 落在 `[reg,#0x18]` 的 167 处字节写，
/// 只有 `purchase_confirmed`（arm64 `0x3a658c`，写 1）与 `purchase_denied`
/// （arm64 `0x3a61b0`，写 0）属于本类；本模块**没有**任何写点。
#[test]
fn owned_is_written_only_by_the_root_node() {
    assert!(ads::MODULE_HAS_NO_OWNED_WRITER);
    assert_eq!(ads::OWNED_WRITERS.len(), 2);
    assert_eq!(ads::OWNED_WRITERS[0].0, "GraphwarRootNode::purchase_confirmed");
    assert_eq!(ads::OWNED_WRITERS[0].1, true);
    assert_eq!(ads::OWNED_WRITERS[1].0, "GraphwarRootNode::purchase_denied");
    assert_eq!(ads::OWNED_WRITERS[1].1, false);

    // 两个回调的效果与 AdSwitch 的方法一致
    let mut s = ads::AdSwitch::new();
    s.purchase_denied();
    assert_eq!(s.are_ads_enabled(), true);
    s.purchase_confirmed();
    assert_eq!(s.are_ads_enabled(), false);
}

/// 判据 7：本模块**不是**联机层，广告视图的开关在别的模块里。
///
/// 真值：`docs/16-client.md` §15 —— 命令 → `0x3af850` →
/// `call_deferred("create_ad_view")` / `("destroy_ad_view")`；
/// `graphwar_backend.rs` 与广告无关（`docs/19` 更正了 `docs/02` §14.4 的旧归类）。
#[test]
fn ad_view_chain_is_outside_this_module() {
    assert_eq!(ads::AD_VIEW_CHAIN.0, "create_ad_view");
    assert_eq!(ads::AD_VIEW_CHAIN.1, "destroy_ad_view");
    assert_eq!(ads::AD_VIEW_CHAIN.2, 0x3af850);
    // 本模块既没有 create_ad_view 也没有 destroy_ad_view 的字面量
    assert!(ads::export("create_ad_view").is_none());
    assert!(ads::export("destroy_ad_view").is_none());
}

/// 判据 8：类名/源文件名/未知项都写清楚了。
#[test]
fn class_identity_and_unknowns() {
    assert_eq!(ads::CLASS, "GraphwarAdSwitch");
    assert_eq!(ads::CLASS.len() as u32, ads::CLASS_LEN);
    assert_eq!(ads::CLASS_LITERAL, 0x19e6a0);
    assert_eq!(ads::SOURCE_FILE, "src/graphwar_ad_switch.rs");
    assert_eq!(ads::UNKNOWNS.len(), 3);
    for u in ads::UNKNOWNS.iter() {
        assert!(!u.is_empty());
    }
}
