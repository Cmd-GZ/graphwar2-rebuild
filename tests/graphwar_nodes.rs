//! `graphwar_nodes.rs` 的复原验收。
//!
//! ⚠️ 按 `AGENTS.md` §0.2：**这些断言是回归保护，不是复原证据。**
//! 每一条右侧的真值都能指到反汇编地址 / arm64 符号名，出处见
//! `src/graphwar_nodes.rs` 的文档注释与 `docs/16-client.md` §28。
//!
//! 引入方式用 `#[path]`：本模块当前**未**在 `src/lib.rs` 注册（`pub mod` 由合并方统一加），
//! 所以集成测试直接引源文件——这样在注册之前也能跑，注册之后也不会重复定义。
//! 同一手法在 `tests/half_step.rs` 里已有先例。

#[path = "../src/graphwar_nodes.rs"]
mod graphwar_nodes;

use graphwar_nodes::*;

/// 判据 1：11 个单态化 —— 数与 `coverage_matrix` 的 `orig_funcs` 相同，地址严格升序、
/// 首尾相接（相邻入口差 ≤ 0x100，中间没有别的函数）。
///
/// 二进制依据：`0x3aaea0, 0x3aafa0, 0x3ab0a0, 0x3ab190 … 0x3ab890` 这 11 个入口
/// 在 `.eh_frame`（`tools/loc_xrefs.py` 的 FDE 表）里首尾相接，大小 0xf0–0x100。
#[test]
fn judgement_1_eleven_thunks_are_contiguous() {
    assert_eq!(GET_NODE_BY_NAME.len(), 11);
    assert_eq!(GET_NODE_BY_NAME.len(), ATTRIBUTED_FUNCS);
    assert_eq!(GET_NODE_BY_NAME[0], 0x3aaea0);
    assert_eq!(GET_NODE_BY_NAME[10], 0x3ab890);
    for w in GET_NODE_BY_NAME.windows(2) {
        assert!(w[0] < w[1]);
        assert!(w[1] - w[0] <= 0x100, "0x{:x} 与 0x{:x} 之间还有别的函数", w[0], w[1]);
    }
}

/// 判据 2：唯一锚 `src/graphwar_nodes.rs:8:62`，`len = 21`。
///
/// 二进制依据：`.data.rel.ro` 的 `0x951678` 处 24 字节 = `{ptr, len=0x15, line=8, col=0x3e}`；
/// `ptr` 由 `R_X86_64_RELATIVE` 写入，addend 指向字面量 `src/graphwar_nodes.rs`。
#[test]
fn judgement_2_panic_anchor_is_the_only_one() {
    assert_eq!(PANIC_ANCHOR.0, "src/graphwar_nodes.rs");
    assert_eq!(PANIC_ANCHOR.0.len(), 21);
    assert_eq!(PANIC_ANCHOR.1, 8);
    assert_eq!(PANIC_ANCHOR.2, 62);
    assert_eq!(PANIC_LOCATION_SLOT, 0x951678);
}

/// 判据 3：12 个函数引用该锚，扣掉被判 contested 的 `0x3a2e10`（`GraphwarRootNode::_ready`）
/// 之后正好 11 个 —— 这是"11 个可归属函数"的完整推导。
#[test]
fn judgement_3_contested_one_is_excluded() {
    assert_eq!(ANCHOR_REFERRING_FUNCS, 12);
    assert_eq!(ANCHOR_REFERRING_FUNCS - 1, ATTRIBUTED_FUNCS);
    assert_eq!(CONTESTED_FN, 0x3a2e10);
    assert_eq!(accessor(CONTESTED_FN), None, "root_node 的 _ready 不属于本模块的 11 个");
    // 0x3a2e10 紧邻第一个单态化之前：0x3a2e10 + 0x26d0 = 0x3a54e0，其后才是 0x3aaea0。
    assert!(CONTESTED_FN < GET_NODE_BY_NAME[0]);
}

/// 判据 4：11 个类型专属尾巴互不相同 ⇒ 11 个不同的 `T`（但 `T` 是什么仍是未知）。
#[test]
fn judgement_4_eleven_distinct_type_tails() {
    let mut v = TYPE_TAILS.to_vec();
    v.sort_unstable();
    v.dedup();
    assert_eq!(v.len(), 11);
    assert_eq!(ACCESSORS.len(), 11);
    // 尾巴都不落在 11 个入口上（它们是另一批 monomorph，不是薄壳本身）。
    for a in ACCESSORS {
        assert!(accessor(a.tail).is_none());
        assert_eq!(accessor_by_tail(a.tail), Some(a));
    }
}

/// 判据 5：`upcast_ref` + "Upcast failed" 是这 11 个函数体的唯一 `Gd` 操作。
///
/// 二进制依据：11 个函数体里 `lea ...%r12` 的立即数全是 `0x1bd8f2`（`upcast_ref`，10 字节），
/// panic 字面量是 `0x1bd8fc`（47 字节 = "Upcast failed. This is a bug; please report it."）。
#[test]
fn judgement_5_accessor_op_is_upcast_ref() {
    assert_eq!(NODE_ACCESSOR_GD_OP, GdOp::UpcastRef);
    assert_eq!(GD_OP_WRAPPER_A, 0x4068f0);
    assert_eq!(GD_OP_WRAPPER_B, 0x4066c0);
    // 全表只有这四个操作名（181 + 295 个调用点逐条扫过）。
    assert_eq!(GdOp::ALL.len(), 4);
    for op in GdOp::ALL {
        assert_eq!(GdOp::from_str(op.as_str()), Some(op));
    }
}

/// 判据 6：节点名表的长度列自洽，且 11 条与 `docs/16` §16.2.1 早已量过的名字重合。
#[test]
fn judgement_6_node_name_lengths() {
    for (n, l) in NODE_NAMES {
        assert_eq!(n.len(), l, "{n} 的长度应是 {l}");
    }
    // 与 §16.2.1 重合的 11 条（那边量的是 `.rodata` 地址，这边量的是长度）。
    let cross = [
        ("RoomNameLineEdit", 16),
        ("AxisModeButton", 14),
        ("FunctionModeButton", 18),
        ("TurnModeButton", 14),
        ("TurnTimeButton", 14),
        ("LockButton", 10),
        ("ChatButtonSetup", 15),
        ("ChatButtonPlay", 14),
        ("ChatPopUp", 9),
        ("ChatBox", 7),
        ("ChatLineEdit", 12),
    ];
    for (n, l) in cross {
        assert_eq!(name_len(n), Some(l), "{n} 与 §16.2.1 对不上");
    }
}

/// 判据 7：`GraphwarRootNode` 是本模块最热的实参（24 个调用点里 15 个点名它）。
#[test]
fn judgement_7_root_node_is_the_hottest_name() {
    assert_eq!(name_len("GraphwarRootNode"), Some(16));
    assert_eq!(names_of(0x3ab590), vec!["GraphwarRootNode"]);
    // 调用方里包含本仓库已实现的三个界面控制器。
    let callers = CALLERS.iter().find(|(f, _)| *f == 0x3ab590).unwrap().1;
    assert!(callers.contains(&0x4f7940), "player_screen_control::skin_button_pressed 必须在内");
    assert!(callers.contains(&0x4e6480), "player_showcase::ready 必须在内");
    assert!(callers.contains(&0x4b4130), "entity_drawer::ready 必须在内");
}

/// 判据 8：arm64 的 29 个符号 = 11 + 17 + 1，且薄壳/递归的大小差一个数量级。
#[test]
fn judgement_8_arm64_symbol_split() {
    assert_eq!(ARM64_SYMBOLS, 29);
    assert_eq!(ARM64_GET_NODE_BY_NAME.len(), 11);
    assert_eq!(ARM64_GET_NODE_BY_NAME_REC.len(), 17);
    assert_eq!(ARM64_GET_NODE_BY_NAME.len() + ARM64_GET_NODE_BY_NAME_REC.len() + 1, ARM64_SYMBOLS);
    assert_eq!(ARM64_GET_NODE_BY_TYPE_REC.1, 636);
    // 薄壳 204/228；递归体 772–816 ⇒ 后者至少大 3 倍。
    for (_, sz) in ARM64_GET_NODE_BY_NAME {
        assert!(sz == 204 || sz == 228);
    }
    assert_eq!(NAME_SOURCE, "android-2.0.60-arm64 .symtab");
}

/// 判据 9：`NAMES_BY_ACCESSOR` 闭合（每个名字都登记了长度、每个单态化都存在）。
#[test]
fn judgement_9_names_by_accessor_is_closed() {
    for (f, names) in NAMES_BY_ACCESSOR {
        assert!(accessor(f).is_some());
        assert_eq!(names_of(f).as_slice(), names);
        for n in names {
            assert!(name_len(n).is_some(), "{n} 没登记长度");
        }
    }
    // 已配对的实参去重后共 34 条。
    let uniq: std::collections::BTreeSet<&str> =
        NAMES_BY_ACCESSOR.iter().flat_map(|(_, ns)| ns.iter().copied()).collect();
    assert_eq!(uniq.len(), 34);
}

/// 判据 10：调用点合计 80，且没有任何调用方本身就是 11 个单态化之一（不成环）。
#[test]
fn judgement_10_callers_do_not_form_a_cycle() {
    assert_eq!(DIRECT_CALL_SITES, 80);
    assert_eq!(accessors_with_callers(), 11);
    for (f, callers) in CALLERS {
        assert!(accessor(f).is_some());
        assert!(!callers.is_empty(), "0x{f:x} 没有直接调用点");
        for c in callers {
            assert!(accessor(*c).is_none(), "调用方 0x{c:x} 也是单态化，成环了");
        }
    }
}

/// 判据 11：诚实清单在（`AGENTS.md` §0.3：做不到的必须写出来）。
#[test]
fn judgement_11_unknowns_are_declared() {
    let u = unknowns();
    assert_eq!(u.len(), 6);
    assert!(u.iter().any(|s| s.contains("T 是什么类型")));
}
