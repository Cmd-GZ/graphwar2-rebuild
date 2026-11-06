//! `GraphwarPlane`（`src/graphwar_plane.rs`）的验收。
//!
//! ⚠️ 按 `AGENTS.md` §0.2：这些条数是**回归保护**，不是复原证据。
//! 复原证据是每条断言注释里指到的**反汇编地址**与 `docs/16-client.md` §26 的表格。
//!
//! 本文件的每一组都对应一条**观测**：
//!
//! | 断言组 | 证据 |
//! |---|---|
//! | [`transform_*`] | `0x4be3e0` 的 `mulsd 0x19fbd0` / `divsd 0x19faa8` / `addpd 0x19ee90`（逐位） |
//! | [`address_table_*`] | arm64 `.symtab` 定名 + x86 的 Location 键集 / 调用者集合（观测） |
//! | [`ready_*`] | `0x1c2a8e` `resized` / `0x1c2a7a` `on_resized` / 3 条事件字面量（观测） |
//! | [`plane_does_not_draw_the_trajectory`] | 全二进制 `draw_line` GOT 槽 `0x9bc228` 的 8 个调用点（有界否定） |

use gw2_rebuild::graphwar_plane::*;

// ---------------------------------------------------------------------------
// 变换与脏矩形（`obstacle_partial_update_event`，0x4be3e0）
// ---------------------------------------------------------------------------

#[test]
fn transform_is_the_board_mapping() {
    assert_eq!(to_pixel(0.0, 0.0), (500, 300));
    assert_eq!(to_pixel(-25.0, -15.0), (0, 0));
    assert_eq!(to_pixel(25.0, 15.0), (1000, 600));
    // 20 px / 游戏单位
    assert_eq!(PX_PER_UNIT, 20.0);
}

#[test]
fn transform_truncates_toward_zero_like_cvttsd2si() {
    // 实测：游戏坐标 -0.05 → 像素 499（-0.05*20+500 = 499.0 向零截断）
    assert_eq!(to_pixel(-0.05, 0.0), (499, 300));
    assert_eq!(to_pixel(0.05, 0.0), (501, 300));
    // 正负对称的截断（不是 floor）
    assert_eq!(to_pixel(0.0, -0.05).1, 299);
}

#[test]
fn transform_saturates_and_maps_nan_to_zero() {
    // `as i32` 饱和（原版是 maxsd/minsd + cvttsd2si，NaN 走 cmovp 归 0）
    assert_eq!(to_pixel(f64::INFINITY, f64::NEG_INFINITY), (i32::MAX, i32::MIN));
    assert_eq!(to_pixel(f64::NAN, f64::NAN), (0, 0));
}

#[test]
fn moved_transform_is_shared_with_plane_module() {
    // 搬家后 `plane::to_pixel` 是 `graphwar_plane::to_pixel` 的再导出（同一项）
    for x in [-25.0, -0.05, 0.0, 0.05, 3.5] {
        assert_eq!(gw2_rebuild::plane::to_pixel(x, x), to_pixel(x, x));
    }
}

#[test]
fn obstacle_rect_is_a_square_dirty_box() {
    let r = obstacle_rect(0.0, 0.0, 1.0); // 半径 = 1.0*1000/50 = 20 px
    assert_eq!((r.min_x, r.min_y, r.max_x, r.max_y), (480, 280, 520, 320));
    assert_eq!(r.width(), 40, "框是正方形，不是圆");
    assert_eq!(r.height(), 40);
    // 半径 0.5（`QueueDestroyObstacle` 用的那个值）→ 20 px 见方
    let h = obstacle_rect(0.0, 0.0, 0.5);
    assert_eq!((h.width(), h.height()), (20, 20));
}

#[test]
fn partial_update_accepts_only_tags_0x28_and_0x29() {
    let mut v = Vec::new();
    obstacle_partial_update_event(&mut v, ENTITY_TAG_A, 0.0, 0.0, 1.0);
    obstacle_partial_update_event(&mut v, ENTITY_TAG_B, 1.0, 1.0, 1.0);
    assert_eq!(v.len(), 2);
    assert_eq!(v[1], obstacle_rect(1.0, 1.0, 1.0));
}

#[test]
#[should_panic]
fn other_tags_panic_not_skip() {
    // 0x4be509 走 core::panicking::panic（.rodata 0x1c2bce = "explicit panic"）
    let mut v = Vec::new();
    obstacle_partial_update_event(&mut v, 0x2a, 0.0, 0.0, 1.0);
}

// ---------------------------------------------------------------------------
// 地址表（名字 ↔ 两侧地址）
// ---------------------------------------------------------------------------

#[test]
fn address_table_has_all_eight_source_level_methods() {
    let names: Vec<&str> = PLANE_FUNCS.iter().map(|f| f.name).collect();
    for want in [
        "draw",
        "init",
        "ready",
        "process",
        "on_resized",
        "clear_state",
        "update_entities",
        "obstacle_partial_update_event",
    ] {
        assert!(names.contains(&want), "缺方法 {want}");
    }
    // `ready::{{closure}}` 也是源级名（ready 里注册的两个回调之一）
    assert!(names.contains(&"ready::{{closure}}"));
}

#[test]
fn the_four_location_anchored_methods_have_the_documented_addresses() {
    // 与 `fixtures/name_to_addr.tsv` 的 `唯一锚` 行逐个相同（可独立复核）
    let get = |n: &str| PLANE_FUNCS.iter().find(|f| f.name == n).map(|f| f.x86).unwrap();
    assert_eq!(get("draw"), 0x4b5df0);
    assert_eq!(get("ready"), 0x4b7a70);
    assert_eq!(get("process"), 0x4b83f0);
    assert_eq!(get("obstacle_partial_update_event"), 0x4be3e0);
    // `init` 的锚是 `src/obstacle_drawer.rs:37:97`（内联），也是唯一锚
    assert_eq!(get("init"), 0x4b7380);
}

#[test]
fn the_three_chain_pinned_methods_are_abutting_on_both_arches() {
    // x86：on_resized → clear_state → update_entities → obstacle_partial_update_event
    for w in PLANE_TAIL_CHAIN.windows(2) {
        let span = PLANE_FUNCS.iter().find(|f| f.x86 == w[0]).unwrap().x86_span;
        assert_eq!(w[0] + span, w[1], "{:#x} 与 {:#x} 必须首尾相接", w[0], w[1]);
    }
    // arm64 侧同样的相接关系（定名的那一侧，独立构建）
    let arm = |n: &str| {
        let f = PLANE_FUNCS.iter().find(|f| f.name == n).unwrap();
        (f.arm64, f.arm64_len)
    };
    for (a, b) in [("on_resized", "clear_state"), ("clear_state", "update_entities"),
                   ("update_entities", "obstacle_partial_update_event")] {
        let (sa, la) = arm(a);
        let (sb, _) = arm(b);
        assert_eq!(sa + la, sb, "arm64 {a} 与 {b} 必须首尾相接");
    }
}

#[test]
fn every_address_lives_in_dot_text_and_orders_agree() {
    for f in PLANE_FUNCS.iter().chain(PLANE_GDEXT_ENTRIES.iter()) {
        assert!((0x37b940..0x94eb70).contains(&f.x86), "{}: {:#x}", f.name, f.x86);
        assert!(f.arm64_len > 0, "{} 缺 arm64 长度", f.name);
    }
    let ordered = |xs: Vec<u64>| xs.windows(2).all(|w| w[0] < w[1]);
    assert!(ordered(PLANE_FUNCS.iter().map(|f| f.x86).collect()));
    assert!(ordered(PLANE_FUNCS.iter().map(|f| f.arm64).collect()));
}

#[test]
fn gdext_entries_are_labelled_as_inference_when_they_are() {
    // 四个 `:68:1` 函数的**组内配对**只有次序 + 等长两条弱证据 ⇒ 必须标"推断"
    let inferred = PLANE_GDEXT_ENTRIES.iter().filter(|f| f.evidence.starts_with("推断")).count();
    assert_eq!(inferred, 4, "四个 `:68:1` 组必须标推断");
    let observed = PLANE_GDEXT_ENTRIES.iter().filter(|f| f.evidence.starts_with("观测")).count();
    assert_eq!(observed, 2, "`:33:10` / `:179:1` 两条有唯一锚，是观测");
}

// ---------------------------------------------------------------------------
// `ready` / `init` / `process` 的意图结构
// ---------------------------------------------------------------------------

#[test]
fn ready_connects_resized_and_subscribes_three_events() {
    let mut p = GraphwarPlane::new();
    let ((signal, handler), events) = p.ready_plan();
    // 字面量槽 0x1c2a8e / 0x1c2a7a（长 7 / 10）
    assert_eq!(signal, "resized");
    assert_eq!(handler, "on_resized");
    // 三条字面量在 .rodata 首尾相接：0x1c2b32 / 0x1c2b44 / 0x1c2b5a
    assert_eq!(events, ["add_obstacle_event", "destroy_obstacle_event", "clear_obstacles_event"]);
    assert_eq!(READY_EVENTS[0].1, 0x1c2b32);
    assert_eq!(READY_EVENTS[2].1, 0x1c2b5a);
    assert!(p.ready);
}

#[test]
fn init_makes_three_function_drawers() {
    let mut p = GraphwarPlane::new();
    p.init_children(|class| {
        assert_eq!(class, "FunctionDrawer");
        7
    });
    assert_eq!(p.function_drawers, vec![7, 7, 7]);
    assert_eq!(FUNCTION_DRAWER_CHILDREN, 3);
}

#[test]
fn process_updates_obstacles_twice_a_frame() {
    let p = GraphwarPlane::new();
    let (updates, reads_team_color) = p.process_plan();
    assert_eq!(updates, 2, "两侧都是 2× ObstacleDrawer::update");
    assert!(reads_team_color, "0x4be6e0 get_team_overview 每帧一次");
}

// ---------------------------------------------------------------------------
// 悬案：弹道在哪画（有界否定）
// ---------------------------------------------------------------------------

#[test]
fn plane_does_not_draw_the_trajectory() {
    // `GraphwarPlane::draw`（0x4b5df0）只 call 两个 CanvasItem 图元槽：
    assert_eq!(DRAW_SET_TRANSFORM_SLOT, 0x9bc018);
    assert_eq!(DRAW_RECT_SLOT, 0x9bc028);
    // 而 draw_line 的槽是：
    assert_eq!(DRAW_LINE_SLOT, 0x9bc228);
    assert_ne!(DRAW_SET_TRANSFORM_SLOT, DRAW_LINE_SLOT);
    assert_ne!(DRAW_RECT_SLOT, DRAW_LINE_SLOT);
    assert_eq!(DRAW_LINE_CALLS_IN_PLANE, 0);
}

#[test]
fn board_is_drawn_as_a_hollow_black_rect() {
    // 0x4b617d movaps [0x19cc20] / 0x4b618c lea [0x19cf70] / 0x4b619b movss [0x1ba598]
    assert_eq!(BOARD_RECT, (-25.0, -15.0, 50.0, 30.0));
    assert_eq!(BOARD_COLOR, (0.0, 0.0, 0.0, 1.0));
    assert_eq!(BOARD_WIDTH, 0.2);
    assert!(!BOARD_FILLED, "`xor %ecx,%ecx` @0x4b61a6 ⇒ filled=false");
    assert!(!BOARD_ANTIALIASED);
    // 与 axis_drawer 用的是同一个 .rodata 槽 0x19cc20（那边是 Rect2，这边是四元组）
    let ax = gw2_rebuild::axis_drawer::BOARD_RECT;
    assert_eq!(
        (ax.pos.x, ax.pos.y, ax.size.x, ax.size.y),
        BOARD_RECT,
        "两个类画的是同一个棋盘框"
    );
}

#[test]
fn class_name_and_unknowns_are_recorded() {
    assert_eq!(CLASS_NAME, "GraphwarPlane");
    // 未知必须照实留档（不是空表）
    assert!(UNKNOWNS.len() >= 5);
    assert!(UNKNOWNS.iter().any(|u| u.contains("0x28")));
}
