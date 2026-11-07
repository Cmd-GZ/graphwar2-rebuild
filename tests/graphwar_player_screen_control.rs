//! `graphwar_player_screen_control.rs` 的复原验收。
//!
//! ⚠️ 按 `AGENTS.md` §0.2：**这些断言是回归保护，不是复原证据。**
//! 每一条右侧的真值都能指到反汇编地址 / arm64 符号名，出处见
//! `src/graphwar_player_screen_control.rs` 的文档注释与 `docs/16-client.md` §29。
//!
//! 引入方式用 `#[path]`：本模块当前**未**在 `src/lib.rs` 注册（`pub mod` 由合并方统一加）。
//! 这里同时引 `graphwar_nodes`，因为本模块**全部**节点都是经它取的——
//! 那条跨模块接缝本身就是判据之一。

#[path = "../src/graphwar_player_screen_control.rs"]
mod graphwar_player_screen_control;

#[path = "../src/graphwar_nodes.rs"]
mod graphwar_nodes;

use graphwar_player_screen_control::*;

/// 判据 1：8 个可归属函数，8 条 `唯一锚`（`fixtures/name_to_addr.tsv`）。
///
/// join 方式是 `panic::Location` 的 `(file, line, col)` 双向唯一，所以
/// "名字 ↔ x86 地址"这一跳可信；地址本身来自 `.eh_frame` 的 FDE 边界。
#[test]
fn judgement_1_eight_anchored_functions() {
    assert_eq!(ATTRIBUTED.len(), 8);
    assert_eq!(ATTRIBUTED.len(), ATTRIBUTED_FUNCS);
    assert_eq!(ATTRIBUTED[0], ("load_items", 0x4f57d0, 0x1fa0));
    assert_eq!(ATTRIBUTED[6], ("unlock", 0x4f8990, 0x140));
    assert_eq!(ATTRIBUTED[7], ("pending", 0x4f8ad0, 0x140));
    // 严格按地址升序、互不重叠（`load_items` 之后有 0x30 的填充，所以不要求首尾相接）。
    for w in ATTRIBUTED.windows(2) {
        assert!(w[0].1 < w[1].1, "必须按地址升序");
        assert!(
            w[0].1 + w[0].2 as u64 <= w[1].1,
            "0x{:x} 与 0x{:x} 重叠了",
            w[0].1,
            w[1].1
        );
    }
}

/// 判据 2：三个按钮回调各写 `GraphwarPlayerConfig` 的**一个不同字段**。
///
/// **本节最强的一条证据**，两条独立路径：
/// * arm64 具名 `bl`：`skin_button_pressed` → `set_skin`，`face_button_pressed` → `set_face`，
///   `hat_button_pressed` → `set_hat`；
/// * x86 槽的 `R_X86_64_RELATIVE` addend：`0x9bb6f0→0x417e20`、`0x9bb6e8→0x417d50`、
///   `0x9bb6e0→0x417c70`，三个函数体分别写 `+0x60` / `+0x78` / `+0x90` 并 `inc [rbx+0xa8]`。
#[test]
fn judgement_2_buttons_write_config_fields() {
    assert_eq!(BUTTON_ACTIONS.len(), 3);
    assert_eq!(field_of_callback(FN_SKIN_BUTTON_PRESSED), Some(ConfigField::Skin));
    assert_eq!(field_of_callback(FN_FACE_BUTTON_PRESSED), Some(ConfigField::Face));
    assert_eq!(field_of_callback(FN_HAT_BUTTON_PRESSED), Some(ConfigField::Hat));

    // 字段偏移与 `docs/16` §22.9 的布局表逐条相同。
    let expect = [
        (ConfigField::Skin, 0x60u32, "yellow", "set_skin"),
        (ConfigField::Face, 0x78, "regular_eyes", "set_face"),
        (ConfigField::Hat, 0x90, "soldier_helmet", "set_hat"),
    ];
    for (f, off, def, setter) in expect {
        assert_eq!(f.offset(), off);
        assert_eq!(f.default_value(), def);
        assert_eq!(f.setter(), setter);
    }
    // 三个槽各不相同（同一条指令序列里不能复用同一个槽）。
    let mut slots: Vec<u64> = BUTTON_ACTIONS.iter().map(|a| a.setter_slot).collect();
    slots.sort_unstable();
    slots.dedup();
    assert_eq!(slots.len(), 3);
    // 三个 setter 入口互不相同，且都是函数入口（与单态化无关）。
    let mut addrs: Vec<u64> = BUTTON_ACTIONS.iter().map(|a| a.setter_addr).collect();
    addrs.sort_unstable();
    addrs.dedup();
    assert_eq!(addrs.len(), 3);
    assert_eq!(addrs, vec![0x417c70, 0x417d50, 0x417e20]);
}

/// 判据 3：`lock` / `pending` / `unlock` 只动可见性，三者两两不同，
/// `lock` 与 `pending` **恰好互补**，`unlock` 两个都隐藏。
///
/// 二进制依据：6 条 `call *0x9ba978`，`%esi` 逐条读出（见 [`BlockState`] 的表）。
#[test]
fn judgement_3_block_states() {
    assert_eq!(BlockState::Locked.blocks(), [("SkinBlock", true), ("PendingBlock", false)]);
    assert_eq!(BlockState::Pending.blocks(), [("SkinBlock", false), ("PendingBlock", true)]);
    assert_eq!(BlockState::Unlocked.blocks(), [("SkinBlock", false), ("PendingBlock", false)]);

    let l = BlockState::Locked.visibility();
    let p = BlockState::Pending.visibility();
    let u = BlockState::Unlocked.visibility();
    for i in 0..2 {
        assert_ne!(l[i], p[i], "lock 与 pending 必须互补");
        assert!(!u[i], "unlock 两个块都隐藏");
    }
    // 三个状态与三个函数一一对应。
    assert_eq!(BlockState::Locked.fn_addr(), 0x4f8850);
    assert_eq!(BlockState::Pending.fn_addr(), 0x4f8ad0);
    assert_eq!(BlockState::Unlocked.fn_addr(), 0x4f8990);
    for s in [BlockState::Locked, BlockState::Pending, BlockState::Unlocked] {
        assert_eq!(BlockState::from_fn_addr(s.fn_addr()), Some(s));
    }
}

/// 判据 4：`0x9ba978` = `CanvasItem::set_visible`（推断，强）——
/// 补上 `docs/14-turn.md` §9.3 与 `docs/16-client.md` §15.9 登记的"未知"。
#[test]
fn judgement_4_set_visible_slot() {
    assert_eq!(SET_VISIBLE_SLOT, 0x9ba978);
    assert_eq!(SET_VISIBLE_WRAPPER, 0x8ab200);
    // 只有本模块这三个"纯可见性"函数用它，且每函数正好 2 次（2 个块）。
    for s in [BlockState::Locked, BlockState::Pending, BlockState::Unlocked] {
        assert_eq!(s.blocks().len(), 2);
    }
}

/// 判据 5：22 条 `panic::Location` 覆盖 8 个函数；`load_items` 独占 9 条
/// （自己 1 条 + 三个被内联的 `load_*` 各 2 条 + 1 条）。
///
/// 二进制依据：`tools/loc_xrefs.py` 从 `.rela.dyn` 的 addend 解出全部 22 个
/// `src/graphwar_player_screen_control.rs:line:col`，再按 FDE 边界归属。
#[test]
fn judgement_5_anchor_lines_cover_eight_functions() {
    assert_eq!(ANCHOR_LINES.len(), 22);
    // 三个"借用/解包/借用计数溢出"三元组：
    assert_eq!(lines_of(28), vec![118, 127, 131]); // 三个按钮回调的 borrow_mut
    assert_eq!(lines_of(107), vec![252, 260, 268]); // lock / pending / unlock
    assert_eq!(lines_of(113), vec![230, 255, 263, 271]); // update_bg + 三个 set_visible 之后
    // load_items 那一堆（113 自己 + 内联进来的三组）。
    for l in [113u32, 135, 139, 156, 159, 165, 183, 186, 192] {
        assert!(ANCHOR_LINES.iter().any(|(line, _)| *line == l), "缺 {l}");
    }
    assert_eq!(LOADER_LINES, [("load_skins", 135), ("load_faces", 156), ("load_hats", 183)]);
}

/// 判据 6：三个 `load_*` 已被内联进 `load_items`（arm64 只剩 52 字节的 `::{{closure}}`），
/// 这解释了 `fixtures` 为什么给它们标"未知 —— 不引用任何 panic::Location"。
#[test]
fn judgement_6_loaders_are_inlined() {
    assert_eq!(INLINED_LOADERS.len(), 3);
    for (name, _, sz) in INLINED_LOADERS {
        assert_eq!(sz, 52, "{name} 应当是 52 字节的 closure");
        assert!(name.contains("closure"));
    }
    assert_eq!(UNANCHORED_ARM64.len(), 3);
    // insert / update_in_vbox 在 arm64 有符号、在 x86 未定 —— 只登记，不猜。
    assert!(UNANCHORED_ARM64.iter().any(|(n, _, _)| *n == "insert_button_in_vbox"));
}

/// 判据 7：本模块**全部**节点都经 `graphwar_nodes` 取；用到的三个单态化
/// 必须是 `graphwar_nodes` 那 11 个的子集（跨模块接缝）。
#[test]
fn judgement_7_nodes_accessors_are_a_subset() {
    assert_eq!(NODE_ACCESSORS_USED.len(), 3);
    for a in NODE_ACCESSORS_USED {
        assert!(
            graphwar_nodes::accessor(a).is_some(),
            "0x{a:x} 不在 graphwar_nodes 的 11 个单态化里"
        );
    }
    // 三个单态化都被本模块的 8 个函数调用过（反查 graphwar_nodes 的调用方表）。
    for a in NODE_ACCESSORS_USED {
        let callers = graphwar_nodes::CALLERS.iter().find(|(f, _)| *f == a).unwrap().1;
        let hit = callers.iter().any(|c| ATTRIBUTED.iter().any(|t| t.1 == *c));
        assert!(hit, "0x{a:x} 没有本模块的调用方");
    }
    // 本模块用到的 6 个节点名都在 graphwar_nodes 的名字表里，长度一致。
    for (n, l) in NODE_NAMES {
        assert_eq!(graphwar_nodes::name_len(n), Some(l), "{n} 与 graphwar_nodes 不一致");
    }
}

/// 判据 8：节点名 ↔ 函数的分工（观测）。
#[test]
fn judgement_8_nodes_by_function() {
    assert_eq!(NODES_BY_FUNC.len(), 6);
    let f = |k: &str| NODES_BY_FUNC.iter().find(|(n, _)| *n == k).unwrap().1;
    assert_eq!(f("load_items"), &["SkinsVbox", "FacesVbox", "HatsVbox"][..]);
    assert_eq!(f("update_buttons_background"), &["FacesVbox", "HatsVbox"][..]);
    assert_eq!(f("skin_button_pressed"), &["GraphwarRootNode"][..]);
    assert!(f("face_button_pressed").is_empty());
    assert!(f("hat_button_pressed").is_empty());
    assert_eq!(BLOCKS, ["SkinBlock", "PendingBlock"]);
    // 每个登记的名字长度都对。
    for (n, l) in NODE_NAMES {
        assert_eq!(n.len(), l);
    }
}

/// 判据 9：`load_items` 是**造控件**的那一个（`PackedScene::instantiate_as` +
/// `Node::add_child_full` + `Button::set_button_icon`）。
#[test]
fn judgement_9_load_items_instantiates() {
    assert_eq!(LOAD_ITEMS_GODOT_CALLS.len(), 8);
    assert!(LOAD_ITEMS_GODOT_CALLS.iter().any(|s| s.contains("instantiate_as")));
    assert!(LOAD_ITEMS_GODOT_CALLS.iter().any(|s| s.contains("add_child_full")));
    assert!(LOAD_ITEMS_GODOT_CALLS.iter().any(|s| s.contains("set_button_icon")));
    // `load_items` 也调 `get_node_by_name_rec`（递归找容器里的子控件）。
    let callees = GAME_CALLEES.iter().find(|(n, _)| *n == "load_items").unwrap().1;
    assert!(callees.iter().any(|c| c.contains("get_node_by_name_rec")));
}

/// 判据 10：具名被调者表里，`set_skin` / `set_face` / `set_hat` 各自只出现在
/// **自己那个**回调下面（互不串台）。
#[test]
fn judgement_10_setters_do_not_cross() {
    let has = |f: &str, needle: &str| {
        GAME_CALLEES
            .iter()
            .find(|(n, _)| *n == f)
            .map(|(_, cs)| cs.iter().any(|c| c.contains(needle)))
            .unwrap_or(false)
    };
    assert!(has("skin_button_pressed", "set_skin"));
    assert!(!has("skin_button_pressed", "set_face"));
    assert!(!has("skin_button_pressed", "set_hat"));
    assert!(has("face_button_pressed", "set_face"));
    assert!(!has("face_button_pressed", "set_skin"));
    assert!(has("hat_button_pressed", "set_hat"));
    assert!(!has("hat_button_pressed", "set_skin"));
    // 三个可见性函数下面只有 set_visible，没有 set_*。
    assert!(has("lock / unlock / pending", "set_visible"));
    assert!(!has("lock / unlock / pending", "set_skin"));
}

/// 判据 11：诚实清单在（`AGENTS.md` §0.3）。
#[test]
fn judgement_11_unknowns_are_declared() {
    let u = unknowns();
    assert_eq!(u.len(), 6);
    assert!(u.iter().any(|s| s.contains("insert_button_in_vbox")));
    assert_eq!(NAME_ROWS, 13);
}
