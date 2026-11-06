//! 最后 8 个可归属函数（5 个小模块）的验收：`function_drawer` /
//! `function_history_recorder` / `function_button_manager` / `button_manager` / `obstacle_drawer`。
//!
//! ⚠️ 按 `AGENTS.md` §0 第 2/5 条：**这些断言是回归保护，不是复原证据。**
//! 每条右侧的真值要么是**指令立即数**、要么是 `.rodata` 比特、要么是 arm64 `.symtab` 真名
//! —— 出处在 `src/<模块>.rs` 的注释里逐条写着。
//!
//! ⚠️ 用 `#[path]` 直接编进本测试二进制 ⇒ **不需要** `src/lib.rs` 里先加 `pub mod`
//! （`src/lib.rs` 本轮不在白名单；加哪几行见 `docs/16-client.md` §36）。
//!
//! 地址口径：`x86` 一律 **ELF 虚地址**（`objdump` 口径），Ghidra/REA 地址 = 本值 + `0x100000`；
//! `arm64` 只用于定名，**绝不**当 x86 地址用。
//!
//! | 测试 | 钉住的东西 |
//! |---|---|
//! | `eight_functions_and_the_twins` | 五个模块合计 **8 个**可归属函数（1+1+2+2+2），外加 `button_manager` 的那个同槽孪生 |
//! | `fd_*` | `function_drawer`：5 个方法 / 1 个锚 / 增量追加语义 |
//! | `fhr_*` | `function_history_recorder`：32 字节记录 / 不容量上限 / 不去重 |
//! | `fbm_*` | `function_button_manager`：8 个分发目标 / 唯一调用者 |
//! | `bm_*` | `button_manager`：9936 B 的 `new` / 2 份 `find_buttons` 单态化 |
//! | `od_*` | `obstacle_drawer`：`update` 的比值 / 7 个 Location / 字面量 |
#![allow(dead_code)]

#[path = "../src/function_drawer.rs"]
mod function_drawer;
#[path = "../src/function_history_recorder.rs"]
mod function_history_recorder;
#[path = "../src/function_button_manager.rs"]
mod function_button_manager;
#[path = "../src/button_manager.rs"]
mod button_manager;
#[path = "../src/obstacle_drawer.rs"]
mod obstacle_drawer;

use button_manager as bm;
use function_button_manager as fbm;
use function_drawer as fd;
use function_history_recorder as fhr;
use obstacle_drawer as od;

/// 同源闸：x86 字节数 ÷ arm64 字节数 必须落在 `0.61–1.19`。
fn inside_same_source_gate(r: f64) -> bool {
    (0.61..=1.19).contains(&r)
}

// ---------------------------------------------------------------------------
// 0. 总量：8 个可归属函数就是"进度尺的最后 8 格"
// ---------------------------------------------------------------------------

/// 每个模块"`panic::Location` 落在几个函数里"—— 这是**可归属**的口径。
fn location_functions(locs: &[(u32, u32, u32, u32)]) -> usize {
    let mut v: Vec<u32> = locs.iter().map(|l| l.3).collect();
    v.sort_unstable();
    v.dedup();
    v.len()
}

/// 五个模块合计 **8 个**可归属函数：`1 + 1 + 2 + 2 + 2`。
///
/// 真值：`tools/loc_xrefs.py` 全量重扫（5301 个 Location / 152 万条指令），
/// 按「引用了 `src/<模块>.rs` 的 Location」分组后的函数个数。
#[test]
fn eight_functions_and_the_twins() {
    let per_module = [
        location_functions(&fd::LOCATIONS),
        location_functions(&fhr::LOCATIONS),
        location_functions(&fbm::LOCATIONS),
        location_functions(&bm::LOCATIONS),
        location_functions(&od::LOCATIONS),
    ];
    assert_eq!(per_module, [1, 1, 2, 2, 2]);
    assert_eq!(per_module.iter().sum::<usize>(), 8);

    // button_manager 的 `:29:26` 还被**第二个**函数引用（同一个槽），所以原始扫描
    // 得到的是 3 个引用者。这个孪生**不计入**上面的 8，但必须记着它存在：
    // arm64 侧对应 `new::{{closure}}`（96 B）与 `drop_in_place<…{{closure}}>`（28 B）。
    let bm_extra = bm::LOCATION_29_SECOND_FUNCTION;
    assert_eq!(bm_extra, 0x003b_70b0);
    assert!(!bm::LOCATIONS.iter().any(|l| l.3 == bm_extra));
}

// ---------------------------------------------------------------------------
// 1. function_drawer
// ---------------------------------------------------------------------------

/// 2.0.60（arm64）里 `FunctionDrawer` 有 **5 个**方法，名字逐个对 `.symtab`。
///
/// 真值：`readelf -sW android-2.0.60/libgraphwarrust.so | c++filt` 的
/// `graphwarrust::function_drawer::FunctionDrawer::*`（外加 2 份 `drop_in_place`）。
#[test]
fn fd_five_methods_from_the_arm64_symtab() {
    let names: Vec<&str> = fd::FUNCTIONS.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        vec![
            "remove_function",
            "update_function",
            "update_transform",
            "update_function_fade",
            "draw",
        ]
    );
    assert_eq!(fd::FUNCTIONS.len(), 5);
    // arm64 的 st_size 逐个抄
    assert_eq!(
        fd::FUNCTIONS.iter().map(|f| f.arm64_len).collect::<Vec<_>>(),
        vec![276, 848, 528, 276, 560]
    );
    assert_eq!(fd::FUNCTIONS.iter().map(|f| f.arm64).collect::<Vec<_>>(), vec![
        0x0039_0a2c,
        0x0039_0b40,
        0x0039_0e90,
        0x0039_10a0,
        0x0039_11b4,
    ]);
}

/// 只有 `update_function` 是**唯一锚**（`0x498c70`，Location `45:38`）；
/// 另外 3 个 x86 地址是靠被调者多重集配的；`draw` 在 x86 侧没落地。
#[test]
fn fd_only_update_function_is_location_anchored() {
    assert_eq!(fd::anchored_in_x86(), 4);
    let u = fd::func("update_function").unwrap();
    assert_eq!(u.x86, Some(0x0049_8c70));
    assert_eq!(u.x86_len, 0x3e0); // 992
    assert!(fd::func("draw").unwrap().x86.is_none());

    assert_eq!(fd::LOCATIONS.len(), 1);
    assert_eq!(fd::LOCATIONS[0].0, 45);
    assert_eq!(fd::LOCATIONS[0].1, 38);
    assert_eq!(fd::LOCATIONS[0].2, 0x0095_a270);
    assert_eq!(fd::LOCATIONS[0].3, 0x0049_8c70);
}

/// 四个配对全在同源闸里（`draw` 没落地址，不参与）。
#[test]
fn fd_size_ratios_are_inside_the_gate() {
    for f in &fd::FUNCTIONS {
        if let Some(r) = fd::size_ratio(f) {
            assert!(inside_same_source_gate(r), "{} 比值 {r}", f.name);
        }
    }
    // 逐个钉死，防止悄悄漂
    let r = |n: &str| fd::size_ratio(fd::func(n).unwrap()).unwrap();
    assert!((r("remove_function") - 0.9275).abs() < 1e-3);
    assert!((r("update_function") - 1.1698).abs() < 1e-3);
    assert!((r("update_transform") - 1.1515).abs() < 1e-3);
    assert!((r("update_function_fade") - 0.9855).abs() < 1e-3);
}

/// **a 问的答案**：增量追加。`add_point` 的 index 恒 `-1`、点占 16 字节、
/// `new_len <= 已写入数` 时一个点都不加。
#[test]
fn fd_add_point_is_incremental_append() {
    assert_eq!(fd::ADD_POINT_INDEX, -1);
    assert_eq!(fd::POINT_STRIDE, 16);
    assert_eq!(fd::POINT_X_OFFSET, 0);
    assert_eq!(fd::POINT_Y_OFFSET, 8);
    // 全二进制唯一的 add_point 调用点就在 update_function 里
    assert_eq!(fd::ADD_POINT_SLOT, 0x009b_bf98);
    assert_eq!(fd::ADD_POINT_CALL_SITE, 0x0049_8fab);
    let u = fd::func("update_function").unwrap().x86.unwrap();
    assert!((u..u + 0x3e0).contains(&fd::ADD_POINT_CALL_SITE));

    assert_eq!(fd::append_window(0, 5), Some(0..5));
    assert_eq!(fd::append_window(5, 7), Some(5..7));
    assert_eq!(fd::append_window(7, 7), None); // 变短/不变 ⇒ 什么都不做
    assert_eq!(fd::append_window(7, 3), None);
}

/// 模型层：一条曲线第 n 帧只补新增的那段，绝不重画。
#[test]
fn fd_curve_store_only_appends_the_new_tail() {
    let mut s = fd::CurveStore::new();
    assert!(s.is_empty());
    // 第 1 帧：0 → 5，补 5 个点
    assert_eq!(s.points_to_append(7, 5), 5);
    assert_eq!(s.commit(7, 5), Some(0..5));
    assert_eq!(s.written(7), Some(5));
    // 第 2 帧：5 → 7，只补 2 个
    assert_eq!(s.commit(7, 7), Some(5..7));
    assert_eq!(s.points_to_append(7, 7), 0);
    // 函数变短：什么都不做，`written` 不被回退
    assert_eq!(s.commit(7, 2), None);
    assert_eq!(s.written(7), Some(7));
    // 另一个玩家 id 是另一条线
    assert_eq!(s.commit(9, 1), Some(0..1));
    assert_eq!(s.len(), 2);
}

/// `GraphwarPlane::init` 会建 **3 个** `FunctionDrawer` 子节点（§26.3）。
#[test]
fn fd_three_children_per_plane() {
    assert_eq!(fd::CHILDREN_PER_PLANE, 3);
}

/// **两版不一致**：2024-07-30 的符号表里有 `clear_functions` / `set_z_index`，
/// 2.0.60 arm64 里没有 —— 以 Android 为准，记录差异。
#[test]
fn fd_legacy_only_method_names() {
    assert_eq!(fd::LEGACY_2024_ONLY, ["clear_functions", "set_z_index"]);
    for n in fd::LEGACY_2024_ONLY {
        assert!(fd::func(n).is_none(), "{n} 不该在 2.0.60 的名单里");
    }
}

// ---------------------------------------------------------------------------
// 2. function_history_recorder
// ---------------------------------------------------------------------------

/// 3 个函数、3 个 Location（`:66:123` / `:67:62` / `:78:18`）、x86 唯一落地 `0x3f0e40`。
#[test]
fn fhr_three_functions_and_three_locations() {
    assert_eq!(fhr::FUNCTIONS.len(), 3);
    let h = fhr::func("handle_function_fire_event").unwrap();
    assert_eq!(h.x86, Some(0x003f_0e40));
    assert_eq!(h.x86_len, 483);
    assert_eq!(h.arm64, 0x0049_d71c);
    assert_eq!(h.arm64_len, 596);
    assert!(inside_same_source_gate(fhr::size_ratio(h).unwrap()));

    let lines: Vec<u32> = fhr::LOCATIONS.iter().map(|l| l.0).collect();
    assert_eq!(lines, vec![66, 67, 78]);
    assert_eq!(fhr::LOCATIONS[2].3, 0x003f_0e40);
}

/// 记录布局：**32 字节** = `String`(24) + `u32`(24..28) + `i32`(28..32)。
#[test]
fn fhr_record_layout_is_32_bytes() {
    assert_eq!(fhr::RECORD_SIZE, 32);
    assert_eq!(fhr::TEXT_OFFSET, 0);
    assert_eq!(fhr::COLOR_OFFSET, 24);
    assert_eq!(fhr::PLAYER_OFFSET, 28);
    assert_eq!(fhr::TEXT_OFFSET + 24, fhr::COLOR_OFFSET);
    assert_eq!(fhr::COLOR_OFFSET + 4, fhr::PLAYER_OFFSET);
    assert_eq!(fhr::PLAYER_OFFSET + 4, fhr::RECORD_SIZE);
}

/// 事件闸：判别值 `0x8000_0000_0000_0023`（`0x3f0e40` 开头的 `cmp` 立即数），
/// `+0x20` 那个 `Option` 的 `None` niche 是 `i64::MIN`（相等就直接 `return`）。
#[test]
fn fhr_event_gate_and_absence_niche() {
    assert_eq!(fhr::EVENT_FIRE_TAG, 0x8000_0000_0000_0023);
    assert_eq!(fhr::EVENT_FIRE_TAG as i64, -0x7fff_ffff_ffff_ffdd);
    assert!(fhr::is_fire_event(0x8000_0000_0000_0023));
    assert!(!fhr::is_fire_event(0x8000_0000_0000_0022));

    assert_eq!(fhr::OPTION_NONE, i64::MIN);
    assert!(fhr::is_absent(i64::MIN));
    assert!(!fhr::is_absent(0));
}

/// **b 问的答案**：没有上限（`MAX_ENTRIES = None`）、不去重（同一个函数连发两次就是两条）。
#[test]
fn fhr_no_cap_and_no_dedup() {
    assert_eq!(fhr::MAX_ENTRIES, None);
    assert!(!fhr::DEDUP);

    let mut h = fhr::History::new();
    assert!(h.is_empty());
    assert!(h.on_function_fire("sin(x)", 0x00ff_0000, 1));
    assert!(h.on_function_fire("sin(x)", 0x00ff_0000, 1)); // 重复也照收
    assert!(h.on_function_fire("x^2", 0x0000_ff00, 2));
    assert_eq!(h.len(), 3);
    assert_eq!(h.count_of("sin(x)"), 2);
    assert_eq!(h.entries()[2].text, "x^2");
    assert_eq!(h.entries()[2].color, 0x0000_ff00);
    assert_eq!(h.entries()[2].player, 2);
}

// ---------------------------------------------------------------------------
// 3. function_button_manager
// ---------------------------------------------------------------------------

/// 3 个函数，两个 x86 落地（`0x3f0270` / `0x3f06b0`），两个比值都在闸里。
#[test]
fn fbm_three_functions_and_two_pairs() {
    assert_eq!(fbm::FUNCTIONS.len(), 3);
    let new = fbm::func("new").unwrap();
    assert_eq!(new.x86, Some(0x003f_0270));
    assert_eq!(new.x86_len, 1088);
    assert_eq!(new.arm64_len, 924);
    let cl = fbm::func("new::{{closure}}").unwrap();
    assert_eq!(cl.x86, Some(0x003f_06b0));
    assert_eq!(cl.x86_len, 1936);
    assert_eq!(cl.arm64_len, 2040);
    assert!((fbm::size_ratio(new).unwrap() - 1.1775).abs() < 1e-3);
    assert!((fbm::size_ratio(cl).unwrap() - 0.9490).abs() < 1e-3);
    assert!(fbm::func("find_buttons").unwrap().x86.is_none());
}

/// 闭包分发到 **8 个** `GraphwarFunctionInput` 方法（任务问的 c 问的一半答案）。
#[test]
fn fbm_dispatches_to_eight_input_methods() {
    assert_eq!(fbm::DISPATCH.len(), 8);
    let names: Vec<&str> = fbm::DISPATCH.iter().map(|d| d.0).collect();
    assert_eq!(
        names,
        vec![
            "clear",
            "insert_text_at_caret",
            "delete_token_at_caret",
            "delete_token_after_caret",
            "move_caret_left",
            "move_caret_right",
            "move_caret_home",
            "move_caret_end",
        ]
    );
    // 一个都不许是"插入函数名"这种语义捷径
    for n in &names {
        assert!(!n.contains("sin") && !n.contains("sqrt"));
        assert!(fbm::dispatch_target(n).is_some());
    }
    assert_eq!(fbm::CLOSURE_ASSERTS, 3);
    assert_eq!(fbm::DISPATCH[0].1, 0x0044_daec);
}

/// 唯一调用者 = `button_manager::ButtonManager::new`（跨模块对账）。
///
/// ⚠️ **地址空间**：`ONLY_CALLER` / `NEW_CALLER` / `FUNCTION_BUTTON_MANAGER_NEW`
/// 直接抄自 REA 的 `procedure_callers`/`procedure_callees` ⇒ **Ghidra 口径**
/// （= vaddr + `0x100000`）；而 `FUNCTIONS[].x86` 与 `LOCATIONS` 是 **vaddr**。
/// 这里是全文件唯一一处跨口径对照，故意写开来防"只加一次"那条老坑。
#[test]
fn fbm_only_caller_is_button_manager_new() {
    assert_eq!(fbm::ONLY_CALLER, 0x003a_2e10 + 0x100000);
    assert!(bm::by_x86(0x003a_2e10).is_some());
    // 反过来：`button_manager::new` 里能定名的子管理器就是 `function_button_manager::new`
    assert_eq!(
        bm::FUNCTION_BUTTON_MANAGER_NEW,
        fbm::func("new").unwrap().x86.unwrap() + 0x100000
    );
}

/// `find_buttons`：深度优先、先序，**非按钮节点不终止递归**。
#[test]
fn fbm_find_buttons_is_preorder_dfs() {
    use fbm::Node;
    let tree = Node::branch(
        "FunctionInputPanel",
        vec![
            Node::leaf("Clear", true),
            Node::branch(
                "Keypad",
                vec![
                    Node::leaf("Seven", true),
                    Node::branch("Spacer", vec![Node::leaf("Deep", true)]),
                    Node::leaf("Label", false),
                ],
            ),
            Node::leaf("Enter", true),
        ],
    );
    let mut got = Vec::new();
    fbm::find_buttons(&tree, &mut got);
    assert_eq!(got, vec!["Clear", "Seven", "Deep", "Enter"]);
}

// ---------------------------------------------------------------------------
// 4. button_manager
// ---------------------------------------------------------------------------

/// `new` 只能按 x86 地址认（arm64 侧被内联，没有独立符号 ⇒ 比值闸不适用）。
#[test]
fn bm_new_is_the_big_inlined_body() {
    assert_eq!(bm::FUNCTIONS.len(), 3);
    let f = bm::by_x86(0x003a_2e10).unwrap();
    assert_eq!(f.x86_len, 9936);
    assert_eq!(f.arm64, None); // 内联 ⇒ 没符号
    assert_eq!(bm::size_ratio(f), None);
    assert_eq!(bm::NEW_CALLER, 0x0052_fdf0);
    assert_eq!(bm::NEW_CALLEE_COUNT, 54);
}

/// 两份 `find_buttons` 单态化：`button_manager` 与 `function_button_manager` 各一份，
/// **算法必须逐字相同**（两侧机器码都是 440 B、被调者逐个相同）。
#[test]
fn bm_two_monomorphizations_of_find_buttons() {
    assert_eq!(bm::FIND_BUTTONS_MONOMORPHIZATIONS, 2);
    assert_eq!(bm::FIND_BUTTONS_PAIRS[0].0, 0x003a_1a6c);
    assert_eq!(bm::FIND_BUTTONS_PAIRS[1].0, 0x0042_4494);
    assert_eq!(
        bm::find_buttons_arm64("function_button_manager"),
        fbm::func("find_buttons").map(|f| f.arm64)
    );

    // 同一棵树，两个模块的模型必须给出同样的顺序
    use bm::Node;
    let tree = Node::branch(
        "Root",
        vec![
            Node::leaf("A", true),
            Node::branch("Sub", vec![Node::leaf("B", true), Node::leaf("C", false)]),
        ],
    );
    let mut a = Vec::new();
    bm::find_buttons(&tree, &mut a);
    assert_eq!(a, vec!["A", "B"]);
}

/// 3 个 Location，其中 `:29:26` 被**两个**函数共用（同一个槽 `0x951660`）。
#[test]
fn bm_three_locations_two_functions() {
    assert_eq!(bm::LOCATIONS.len(), 3);
    let lines: Vec<u32> = bm::LOCATIONS.iter().map(|l| l.0).collect();
    assert_eq!(lines, vec![24, 26, 29]);
    let slots: Vec<u32> = bm::LOCATIONS.iter().map(|l| l.2).collect();
    assert_eq!(slots, vec![0x0095_1648, 0x0095_1630, 0x0095_1660]);
    // 24:12 / 26:26 在 new 里，29:26 在闭包（其一）里
    assert_eq!(bm::LOCATIONS[0].3, 0x003a_2e10);
    assert_eq!(bm::LOCATIONS[1].3, 0x003a_2e10);
    assert_eq!(bm::LOCATIONS[2].3, 0x003a_ae60);
}

// ---------------------------------------------------------------------------
// 5. obstacle_drawer
// ---------------------------------------------------------------------------

/// `update`：x86 `0x3ee190`（5216 B）↔ arm64 `0x422590`（4688 B），比值 1.11 在闸里。
#[test]
fn od_update_pair_and_ratio() {
    let u = od::func("update").unwrap();
    assert_eq!(u.x86, 0x003e_e190);
    assert_eq!(u.x86_len, 5216);
    assert_eq!(u.arm64, 0x0042_2590);
    assert_eq!(u.arm64_len, 4688);
    assert!(inside_same_source_gate(od::size_ratio(u)));
    assert!((od::size_ratio(u) - 1.1126).abs() < 1e-3);
}

/// 7 个 Location：6 个在 `update` 里，第 7 个（`:37:97`）在 **`GraphwarPlane::init``** 体内。
#[test]
fn od_seven_locations_and_the_plane_init_one() {
    assert_eq!(od::LOCATIONS.len(), 7);
    let in_update = od::LOCATIONS.iter().filter(|l| l.3 == 0x003e_e190).count();
    assert_eq!(in_update, 6);
    let plane = od::LOCATIONS.iter().find(|l| l.3 == 0x004b_7380).unwrap();
    assert_eq!((plane.0, plane.1), (37, 97));
    assert_eq!(plane.2, 0x0095_4ea0);
    // init 自己的地址（docs/16-client.md §26.2）
    assert_eq!(od::func("GraphwarPlane::init（体内带 obstacle_drawer.rs:37:97）").unwrap().x86, 0x004b_7380);
}

/// 字面量：地址是 vaddr、长度必须等于字符串长度（`.rodata` 的 `vaddr == 文件偏移`）。
#[test]
fn od_literals_and_lengths() {
    assert_eq!(od::LITERALS.len(), 7);
    for (text, va, len, _) in od::LITERALS {
        assert_eq!(len, text.len(), "{text} 的长度");
        assert!(va >= 0x0019_0000 && va < 0x002a_0000, "{text} 落在 .rodata 里");
    }
    assert_eq!(od::literal("Sprite2D").unwrap().1, 0x0019_fb58);
    assert_eq!(od::literal("ImageTexture").unwrap().1, 0x001b_fc03);
    assert_eq!(od::literal("create_from_image").unwrap().1, 0x001b_fc0f);
    assert_eq!(od::literal("set_texture").unwrap().1, 0x001b_fc8c);
    // `create_from_image` 紧跟 `ImageTexture`（连排字符串池）
    assert_eq!(
        od::literal("ImageTexture").unwrap().1 + 12,
        od::literal("create_from_image").unwrap().1
    );
}

/// `GraphwarPlane::process` 每帧调 `update` **2 次**；每次体内 `create_from_data` 出现 2 次。
#[test]
fn od_two_updates_per_frame() {
    assert_eq!(od::UPDATE_CALLS_PER_FRAME, 2);
    assert_eq!(od::images_per_frame(2), 4);
    assert!(od::OBSTACLE_DATA_GUARDED_BY_MUTEX);
    assert!(od::CALLED_API.contains(&"Image::create_from_data"));
    assert!(od::CALLED_API.contains(&"hashbrown::rustc_entry"));
    // 本模块**不调**任何绘图图元（这是它的定性结论）
    for primitive in ["draw_rect", "draw_circle", "draw_line", "draw_polygon"] {
        assert!(!od::CALLED_API.contains(&primitive), "{primitive} 不该出现在这里");
    }
}
