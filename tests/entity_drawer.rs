//! `entity_drawer` 的验收断言。
//!
//! ⚠️ 按 `AGENTS.md` §0.2：**这些条数是回归保护，不是复原证据**。
//! 这里的每一个期望值都抄自 `libgraphwarrust.so` 的机器码，
//! 注释里给出指令地址 / `.rodata` 槽地址，方便回到反汇编逐条复核。
//!
//! 目标函数：`EntityDrawer::draw` = **vaddr `0x4b2890`**（体 `0x4b2890..=0x4b3fea`）。

use gw2_rebuild::entity_drawer::{
    arrow_head, arrow_primitives, entity_primitives, half_angle, ring_primitives, segment_angles,
    Color, EntityRingInput, Primitive, Vec2, ARC_ANTIALIASED, ARC_POINT_COUNT, ARC_WIDTH,
    HALF_ANGLE_MAX_RAD, HALF_ANGLE_MIN_RAD, HALF_ANGLE_SWEEP_RAD, SEGMENT_COUNT,
    SEGMENT_STEP2_RAD, SEGMENT_STEP_RAD,
};

fn input(v: f64, base: Option<f64>) -> EntityRingInput {
    EntityRingInput {
        center: Vec2::new(3.0, -2.0),
        pos_radius: 0.5716666,
        v,
        base,
        color: Color::from_packed_rgb(0x00_68_7d), // #7D6800（真帧取色，字节序低=R）
    }
}

// ---------------------------------------------------------------------------
// 常量：全是 `.rodata` 立即数
// ---------------------------------------------------------------------------

#[test]
fn constants_match_rodata_slots() {
    // arc 折点数：mov $0xa,%esi @0x4b3655
    assert_eq!(ARC_POINT_COUNT, 10);
    // 线宽：槽 0x1bb2b4 = 0x3e99999a
    assert_eq!(ARC_WIDTH, 0.3_f32);
    // 抗锯齿：xor %ecx,%ecx @0x4b366a
    assert!(!ARC_ANTIALIASED);
    // 段数：三段方位角 @0x4b2e9d..0x4b2ebe
    assert_eq!(SEGMENT_COUNT, 3);
    // 槽 0x19f9e8 / 0x19fbf8（= 2π/3、4π/3）
    assert_eq!(SEGMENT_STEP_RAD, 2.0943951023931953);
    assert_eq!(SEGMENT_STEP2_RAD, 4.1887902047863905);
    assert!((SEGMENT_STEP_RAD - std::f64::consts::FRAC_PI_2 * 4.0 / 3.0).abs() < 1e-15);
    // 槽 0x19f2e8 / 0x19f0a8（π/6、1.0733774899765125）
    assert_eq!(HALF_ANGLE_MIN_RAD, 0.5235987755982988);
    assert_eq!(HALF_ANGLE_MAX_RAD, 1.0733774899765125);
    // minsd @0x4b359c ⇒ 封顶
    assert!(HALF_ANGLE_MAX_RAD > HALF_ANGLE_MIN_RAD);
    assert_eq!(
        HALF_ANGLE_SWEEP_RAD,
        1.0733774899765125 - 0.5235987755982988
    );
}

// ---------------------------------------------------------------------------
// 三段方位角
// ---------------------------------------------------------------------------

#[test]
fn three_segments_are_120_degrees_apart() {
    let a = segment_angles(120.0); // A0 = 120/120 = 1 rad
    assert_eq!(a.len(), 3);
    assert_eq!(a[0], 1.0);
    assert_eq!(a[1], 1.0 + 2.0943951023931953);
    assert_eq!(a[2], 1.0 + 4.1887902047863905);

    // 间隔正好 2π/3（== 120°）
    let d1 = a[1] - a[0];
    let d2 = a[2] - a[1];
    assert!((d1 - d2).abs() < 1e-15);
    assert!((d1 - 2.0 * std::f64::consts::PI / 3.0).abs() < 1e-16);

    // A0 = V/120（divsd 槽 0x19f620）
    assert!((segment_angles(240.0)[0] - 2.0).abs() < 1e-15);
}

// ---------------------------------------------------------------------------
// 半张角 H
// ---------------------------------------------------------------------------

#[test]
fn half_angle_matches_the_two_branches() {
    // 哈希表查不到 ⇒ 直接用 π/6（`0x4b34e6 je 0x4b35b4`）
    assert_eq!(half_angle(999.0, None), HALF_ANGLE_MIN_RAD);

    // H = π/6 + (V − base)/60，未到顶（60.0 槽 0x19f918）
    // 取 V − base = 12 ⇒ H = π/6 + 0.2（未触顶）
    let h = half_angle(132.0, Some(120.0));
    assert!((h - (HALF_ANGLE_MIN_RAD + 0.2)).abs() < 1e-15);
    // V − base = 60 ⇒ π/6 + 1 已越过封顶 ⇒ 取封顶
    assert_eq!(half_angle(180.0, Some(120.0)), HALF_ANGLE_MAX_RAD);

    // V == base ⇒ 恰好 π/6
    assert!((half_angle(120.0, Some(120.0)) - HALF_ANGLE_MIN_RAD).abs() < 1e-15);

    // 封顶：π/6 + (V−base)/60 = 1.0733774899765125 时 V−base = 32.9867250…
    let cap_delta = (HALF_ANGLE_MAX_RAD - HALF_ANGLE_MIN_RAD) * 60.0;
    assert!((half_angle(120.0 + cap_delta, Some(120.0)) - HALF_ANGLE_MAX_RAD).abs() < 1e-15);
    // 越过封顶仍取封顶
    assert_eq!(
        half_angle(120.0 + cap_delta + 100.0, Some(120.0)),
        HALF_ANGLE_MAX_RAD
    );

    // 单调不减（张开 → 闭合）
    let mut prev = -1.0;
    let mut v = 120.0;
    while v <= 400.0 {
        let h = half_angle(v, Some(120.0));
        assert!(h >= prev);
        prev = h;
        v += 1.0;
    }
}

// ---------------------------------------------------------------------------
// 弧的实参
// ---------------------------------------------------------------------------

#[test]
fn arcs_carry_the_exact_draw_arc_arguments() {
    let inp = input(180.0, Some(120.0));
    let prims = ring_primitives(inp);
    assert_eq!(prims.len(), 3);

    let h = half_angle(180.0, Some(120.0));
    let angles = segment_angles(180.0);
    // 半径 = f32(0.5716666) * 1.5f32（cvtsd2ss @0x4b35ba / mulss @0x4b35be）
    let expect_r = (0.5716666_f32 * 1.5_f32) as f64;

    for (i, p) in prims.iter().enumerate() {
        match p {
            Primitive::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                point_count,
                color,
                width,
                antialiased,
            } => {
                assert_eq!(*center, Vec2::new(3.0, -2.0));
                assert_eq!(*radius, expect_r);
                // start = A_i − H（subsd @0x4b3620），end = A_i + H（addsd @0x4b362c）
                assert_eq!(*start_angle, angles[i] - h);
                assert_eq!(*end_angle, angles[i] + h);
                assert_eq!(*point_count, 10);
                // 0.3f32 加宽成 f64 后是 0.30000001192092896（原版也是 f32 常量）
                assert_eq!(*width, 0.3_f32 as f64);
                assert!(!*antialiased);
                assert_eq!(*color, Color::from_packed_rgb(0x00_68_7d));
            }
            other => panic!("expected Arc, got {:?}", other),
        }
    }
}

#[test]
fn arc_is_symmetric_about_its_segment_angle() {
    let inp = input(200.0, Some(130.0));
    let angles = segment_angles(inp.v);
    for (i, p) in ring_primitives(inp).iter().enumerate() {
        if let Primitive::Arc {
            start_angle,
            end_angle,
            ..
        } = p
        {
            let mid = 0.5 * (start_angle + end_angle);
            assert!((mid - angles[i]).abs() < 1e-15);
            // 张角 = 2H
            let h = half_angle(inp.v, inp.base);
            assert!((end_angle - start_angle - 2.0 * h).abs() < 1e-15);
        }
    }
}

#[test]
fn open_ring_is_narrower_than_closed_ring() {
    // 张开：H = π/6 ⇒ 单段 60°，三段合计 180°
    let open = ring_primitives(input(120.0, Some(120.0)));
    // 闭合：H = 1.07337748998 ⇒ 单段 ≈123°，三段 ≈369° > 360°
    let closed = ring_primitives(input(120.0 + 200.0, Some(120.0)));

    let span = |p: &Primitive| match p {
        Primitive::Arc {
            start_angle,
            end_angle,
            ..
        } => end_angle - start_angle,
        _ => unreachable!(),
    };
    let open_span = span(&open[0]);
    let closed_span = span(&closed[0]);
    assert!((open_span - std::f64::consts::PI / 3.0).abs() < 1e-15); // 60°
    assert!((closed_span - 2.0 * HALF_ANGLE_MAX_RAD).abs() < 1e-15);
    assert!(closed_span > 2.0 * std::f64::consts::PI / 3.0);
}

// ---------------------------------------------------------------------------
// 箭头（draw_polygon）
// ---------------------------------------------------------------------------

#[test]
fn arrow_head_is_the_three_points_from_0x4b9a70() {
    // θ = 0，r = 1 ⇒ 顶点可闭式写出
    let p = arrow_head(Vec2::new(0.0, 0.0), 1.0, 0.0);
    assert_eq!(p[0], Vec2::new(1.0, 0.0));

    let c = 0.20943951023931953_f64.cos(); // cos 12°
    let s = 0.20943951023931953_f64.sin(); // sin 12°
    assert!((c - 0.9781476007338057).abs() < 1e-15);
    assert!((s - 0.20791169081775934).abs() < 1e-15);

    // P1 = (r+0.5)·(cos −6°… 实为 θ−12°), P2 = (r+0.5)·(θ+12°)
    assert!((p[1].x - 1.5 * c).abs() < 1e-15);
    assert!((p[1].y + 1.5 * s).abs() < 1e-15);
    assert!((p[2].x - 1.5 * c).abs() < 1e-15);
    assert!((p[2].y - 1.5 * s).abs() < 1e-15);

    // 顶点在环心半径上，底边在外侧 r+0.5
    assert!((p[0].x.hypot(p[0].y) - 1.0).abs() < 1e-15);
    assert!((p[1].x.hypot(p[1].y) - 1.5).abs() < 1e-15);
    assert!((p[2].x.hypot(p[2].y) - 1.5).abs() < 1e-15);
}

#[test]
fn three_arrows_share_angles_and_colour_with_the_arcs() {
    let inp = input(180.0, Some(120.0));
    let arrows = arrow_primitives(inp);
    assert_eq!(arrows.len(), 3);

    let angles = segment_angles(inp.v);
    let r = (0.5716666_f32 * 1.5_f32) as f64;

    for (i, p) in arrows.iter().enumerate() {
        match p {
            Primitive::Polygon { points, colors } => {
                assert_eq!(points.len(), 3);
                // 每个箭头 push 同一个颜色 3 份（0x4b9c3c 起三次 movupd (%r14)）
                assert_eq!(colors.len(), 3);
                assert!(colors.iter().all(|c| *c == inp.color));
                let expect = arrow_head(inp.center, r, angles[i]);
                for (got, want) in points.iter().zip(expect.iter()) {
                    assert!((got.x - want.x).abs() < 1e-15);
                    assert!((got.y - want.y).abs() < 1e-15);
                }
            }
            other => panic!("expected Polygon, got {:?}", other),
        }
    }
}

// ---------------------------------------------------------------------------
// 颜色
// ---------------------------------------------------------------------------

#[test]
fn packed_colour_is_divided_by_255_per_channel() {
    // divps 槽 0x19c7c0 = {255,255,255,1}
    let c = Color::from_packed_rgb(0x00_68_7d); // R=0x7d, G=0x68, B=0x00
    assert!((c.r - 125.0 / 255.0).abs() < 1e-6);
    assert!((c.g - 104.0 / 255.0).abs() < 1e-6);
    assert_eq!(c.b, 0.0);
    // 第 4 通道不参与除法 ⇒ 恒 1.0（与真帧"不透明"一致）
    assert_eq!(c.a, 1.0);
    assert_eq!(Color::from_packed_rgb(0x00_00_00), Color::new(0.0, 0.0, 0.0, 1.0));
    assert_eq!(
        Color::from_packed_rgb(0x00_ff_ff),
        Color::new(1.0, 1.0, 0.0, 1.0)
    );
}

// ---------------------------------------------------------------------------
// 整串
// ---------------------------------------------------------------------------

#[test]
fn entity_primitives_emits_arrows_then_arcs() {
    let prims = entity_primitives(input(180.0, Some(120.0)));
    assert_eq!(prims.len(), 6);
    // 地址序：0x4b30b7/0x4b30e8/0x4b3119（箭头）先于 0x4b366c/0x4b36fd/0x4b378a（弧）
    assert!(matches!(prims[0], Primitive::Polygon { .. }));
    assert!(matches!(prims[1], Primitive::Polygon { .. }));
    assert!(matches!(prims[2], Primitive::Polygon { .. }));
    assert!(matches!(prims[3], Primitive::Arc { .. }));
    assert!(matches!(prims[4], Primitive::Arc { .. }));
    assert!(matches!(prims[5], Primitive::Arc { .. }));
}

#[test]
fn radius_scale_is_applied_in_f32_then_widened() {
    use gw2_rebuild::entity_drawer::{drawn_radius, RADIUS_SCALE};
    // 1.5 在两侧各是一条常量：0x1b9b98(f32) 与 0x19f8e0(f64)
    assert_eq!(RADIUS_SCALE, 1.5);
    assert_eq!(drawn_radius(2.0), 3.0);
    // 先窄化到 f32 再乘，故与纯 f64 乘法在末位可能不同——这里钉住"按 f32 算"这一事实
    assert_eq!(drawn_radius(0.5716666), (0.5716666_f32 * 1.5_f32) as f64);
    let pure_f64 = 0.5716666_f64 * 1.5_f64;
    assert!(
        (drawn_radius(0.5716666) - pure_f64).abs() < 1e-6,
        "f32 往返与 f64 直乘应当只差末尾几位"
    );
    // 非负性（半径量不该出现负值；原版没做 abs，这里只做自洽检查）
    assert!(drawn_radius(0.0) == 0.0);
}

// ===========================================================================
// 以下是 §2（其余 9 个函数）的断言。**上面 11 个测试一条没动。**
//
// ⚠️ 同样按 `AGENTS.md` §0.2：条数是回归保护，不是复原证据。
// 每个期望值仍抄自真二进制，注释给 x86 / arm64 两侧的指令或槽地址。
// ===========================================================================

use gw2_rebuild::entity_drawer::{
    death_alpha, name_label_layout, offset_triangle, sprite_transform, team_mirror,
    team_mirror_sign, EntityChildNode, NameLabelLayout, SpriteTransform, DEATH_FADE_DIVISOR,
    DEATH_FADE_TICKS, ENTITY_CHILD_NODES, ENTITY_DRAWER_FUNCS, ENTITY_DRAWER_INIT_ADDR,
    NAME_LABEL_SCENE, PIXELS_PER_UNIT_DIVISOR, READY_EVENTS, READY_FLAG_OFFSET, RESIZED_HANDLER,
    RESIZED_SIGNAL, SPRITE_BASE_SIZE, SPRITE_FLIP_V, SPRITE_SCALE_DIVISOR,
    SPRITE_SCALE_DIVISOR_NEG, SPRITE_TEXTURE_FILTER,
};

// ---------------------------------------------------------------------------
// 地址表（arm64 `.symtab` 名字 ↔ x86 vaddr）
// ---------------------------------------------------------------------------

#[test]
fn ten_functions_are_pinned_with_addr_and_arm64_size() {
    // 10 个源级名字，一个不多一个不少
    assert_eq!(ENTITY_DRAWER_FUNCS.len(), 10);
    let names: Vec<&str> = ENTITY_DRAWER_FUNCS.iter().map(|(n, _, _)| *n).collect();
    for want in [
        "draw",
        "ready",
        "process",
        "update_sprite",
        "update_name_label",
        "draw_offset_triangle",
        "get_entity_hat_sprite",
        "get_entity_name_label",
        "get_entity_face_sprite",
        "get_entity_skin_sprite",
    ] {
        assert!(names.contains(&want), "missing {}", want);
    }

    // `draw` 是上一轮已经钉死的锚，必须与既有结论一致
    let draw = ENTITY_DRAWER_FUNCS.iter().find(|(n, _, _)| *n == "draw").unwrap();
    assert_eq!(draw.1, 0x4b_2890);
    assert_eq!(draw.2, 5300); // arm64 0x4b9430 st_size

    // `ready` / `process` 是 `fixtures/name_to_addr.tsv` 的**唯一锚**，同样必须一致
    let ready = ENTITY_DRAWER_FUNCS.iter().find(|(n, _, _)| *n == "ready").unwrap();
    assert_eq!(ready.1, 0x4b_4130);
    let process = ENTITY_DRAWER_FUNCS.iter().find(|(n, _, _)| *n == "process").unwrap();
    assert_eq!(process.1, 0x4b_4b60);

    // 本轮新定名的两个「未知」锚：靠 arm64 结构指纹 + x86 常量落在 0x4b9530..0x4b9e70 之间
    let unl = ENTITY_DRAWER_FUNCS
        .iter()
        .find(|(n, _, _)| *n == "update_name_label")
        .unwrap();
    assert_eq!(unl.1, 0x4b_9810);
    let dot = ENTITY_DRAWER_FUNCS
        .iter()
        .find(|(n, _, _)| *n == "draw_offset_triangle")
        .unwrap();
    // 就是既有代码里那个"匿名包装 0x4b9a70"——箭头三角形的真名
    assert_eq!(dot.1, 0x4b_9a70);

    // 三个贴图 getter 在 arm64 上是**同样大小**（同一段代码 3 份）
    let sz = |n: &str| {
        ENTITY_DRAWER_FUNCS
            .iter()
            .find(|(m, _, _)| *m == n)
            .unwrap()
            .2
    };
    assert_eq!(sz("get_entity_hat_sprite"), sz("get_entity_face_sprite"));
    assert_eq!(sz("get_entity_hat_sprite"), sz("get_entity_skin_sprite"));

    // `init` 不在 10 个里
    assert_eq!(ENTITY_DRAWER_INIT_ADDR, 0x4b_3ff0);
    assert!(!names.contains(&"init"));
}

// ---------------------------------------------------------------------------
// `ready`：1 个信号 + 6 个后端事件
// ---------------------------------------------------------------------------

#[test]
fn ready_connects_one_signal_and_subscribes_six_events() {
    // 信号名 7 字节 / 处理函数名 10 字节（arm64 mov w2,#0x7 @0x4babc0，
    // 与 from_object_method 的长 10 @0x4bab84）
    assert_eq!(RESIZED_SIGNAL, "resized");
    assert_eq!(RESIZED_SIGNAL.len(), 7);
    assert_eq!(RESIZED_HANDLER, "on_resized");
    assert_eq!(RESIZED_HANDLER.len(), 10);

    assert_eq!(READY_EVENTS.len(), 6);
    assert_eq!(
        READY_EVENTS,
        [
            "pos_info_event",
            "life_info_event",
            "entity_removed_event",
            "skin_graphics_info_event",
            "face_graphics_info_event",
            "hat_graphics_info_event",
        ]
    );
    // 两侧是同一串连续字面量：arm64 0x1a1391..0x1a13f2 / x86 0x1c2a95..0x1c2af6
    for e in READY_EVENTS {
        assert!(e.ends_with("_event"), "{}", e);
    }
    // arm64 0x1a1391 起 6 条**首尾相接**（相邻两条的起始地址差 == 前一条的长度）：
    // 0x1a1391→0x1a139f→0x1a13ae→0x1a13c2→0x1a13da→0x1a13f2，长度 14/15/20/24/24/23
    let want_len = [14usize, 15, 20, 24, 24, 23];
    for (e, w) in READY_EVENTS.iter().zip(want_len) {
        assert_eq!(e.len(), w, "{}", e);
    }
    // 首尾相接 ⇒ 6 条拼起来正好是一条连续的 .rodata 区段
    let total: usize = want_len.iter().sum();
    assert_eq!(total, 0x1a13f2 - 0x1a1391 + 23);

    // `strb w8(1),[x20,#0x108]` @ arm64 0x4babd4 / `mov BYTE PTR [rbx+0x108],0x1` @ x86 0x4b41be
    assert_eq!(READY_FLAG_OFFSET, 0x108);
}

// ---------------------------------------------------------------------------
// 士兵是**子节点**画的，不是 `draw()` 画的
// ---------------------------------------------------------------------------

#[test]
fn soldiers_are_child_nodes_not_draw_primitives() {
    // 4 类子节点，按 process 的首次取用顺序 = 叠放顺序
    assert_eq!(ENTITY_CHILD_NODES.len(), 4);
    assert_eq!(ENTITY_CHILD_NODES[0], EntityChildNode::SkinSprite);
    assert_eq!(ENTITY_CHILD_NODES[1], EntityChildNode::FaceSprite);
    assert_eq!(ENTITY_CHILD_NODES[2], EntityChildNode::HatSprite);
    assert_eq!(ENTITY_CHILD_NODES[3], EntityChildNode::NameLabel);

    // 三个是 Sprite2D，最后一个是 RichTextLabel
    assert!(EntityChildNode::SkinSprite.is_sprite());
    assert!(EntityChildNode::FaceSprite.is_sprite());
    assert!(EntityChildNode::HatSprite.is_sprite());
    assert!(!EntityChildNode::NameLabel.is_sprite());

    // 贴图方法名来自 arm64 `.symtab`（GraphwarTextureLoader::get_*_texture）
    assert_eq!(
        EntityChildNode::SkinSprite.texture_method(),
        Some("get_skin_texture")
    );
    assert_eq!(
        EntityChildNode::FaceSprite.texture_method(),
        Some("get_face_texture")
    );
    assert_eq!(
        EntityChildNode::HatSprite.texture_method(),
        Some("get_hat_texture")
    );
    assert_eq!(EntityChildNode::NameLabel.texture_method(), None);

    // 名字标签是 load(tscn) + instantiate_as::<RichTextLabel>()
    assert_eq!(NAME_LABEL_SCENE, "res://name_label.tscn");

    // 三个 Sprite2D 建好后的两条统一设置
    assert_eq!(SPRITE_TEXTURE_FILTER, 4); // mov w1,#0x4 @0x4bfe10
    assert!(SPRITE_FLIP_V); // mov w1,#0x1 @0x4bfe30
}

// ---------------------------------------------------------------------------
// 队伍色镜像判据（update_sprite 与 update_name_label 共用）
// ---------------------------------------------------------------------------

#[test]
fn team_mirror_is_the_bit_test_from_the_binary() {
    // (c & 0x101) == 0x100 ⇒ 镜像
    assert!(team_mirror(0x0000_0100));
    assert!(team_mirror(0xff00_0100));
    assert!(!team_mirror(0x0000_0101)); // 红也置位 ⇒ 不镜像
    assert!(!team_mirror(0x0000_0000));

    // 蓝不置位 且 alpha != 0 ⇒ 镜像
    assert!(team_mirror(0x0100_0000));
    assert!(!team_mirror(0x0101_0000)); // 蓝置位 ⇒ 该条不成立
    assert!(!team_mirror(0x0000_0000)); // alpha == 0 ⇒ 该条不成立

    // 第一条成立时，第二条是否成立都不影响
    assert!(team_mirror(0x0100_0100)); // 两条都成立
    assert!(team_mirror(0x0000_0100 | 0x0101_0000)); // 第一条成立、第二条不成立

    // 符号形式就是 fneg + fcsel
    assert_eq!(team_mirror_sign(0x0000_0100), -1.0);
    assert_eq!(team_mirror_sign(0x0000_0000), 1.0);
}

// ---------------------------------------------------------------------------
// `update_sprite` 的变换
// ---------------------------------------------------------------------------

#[test]
fn sprite_transform_matches_update_sprite_arithmetic() {
    // 常量先自查
    assert_eq!(PIXELS_PER_UNIT_DIVISOR, 50.0); // 槽 0x1b9fc8 = 0x42480000
    assert_eq!(SPRITE_BASE_SIZE, 128.0); // 槽 0x19c634（向量第 2 lane）
    assert_eq!(SPRITE_SCALE_DIVISOR, 60.0); // 槽 0x19d1b0 = 0x42700000
    assert_eq!(SPRITE_SCALE_DIVISOR_NEG, -60.0); // 槽 0x1baacc = 0xC2700000

    // 无镜像（c = 0 ⇒ 两条都不成立 ⇒ sign = +1）
    let size = Vec2::new(500.0, 300.0);
    let k = 500.0_f32 / 50.0;
    let t = sprite_transform(size, 0, 256, 3.0, -2.0);
    assert_eq!(
        t,
        SpriteTransform {
            position: Vec2::new(
                (500.0_f32 * 0.5 + k * 3.0) as f64,
                (300.0_f32 * 0.5 - k * (-2.0)) as f64,
            ),
            scale: Vec2::new(
                ((1.0_f32 / 60.0) * (128.0_f32 / 256.0)) as f64,
                ((k / -60.0) * (128.0_f32 / 256.0)) as f64,
            ),
            flip_h: true, // x = 3 > 0
        }
    );
    // 具体数值，方便肉眼核对：w/2 + 10·3 = 280，h/2 + 10·2 = 170
    assert!((t.position.x - 280.0).abs() < 1e-3);
    assert!((t.position.y - 170.0).abs() < 1e-3);
    // scale.y 天然为负（屏幕 y 向下 / 棋盘 y 向上）
    assert!(t.scale.y < 0.0);
    assert!(t.scale.x > 0.0);

    // 镜像（c = 0x0000_0100 ⇒ sign = -1）⇒ x 反向、scale.x 反号
    let m = sprite_transform(size, 0x0000_0100, 256, 3.0, -2.0);
    assert!((m.position.x - 220.0).abs() < 1e-3); // 250 - 30
    assert!((m.position.y - 170.0).abs() < 1e-3); // y 不受镜像影响
    assert!(m.scale.x < 0.0);
    assert!(m.scale.y < 0.0);

    // flip_h 只看 x 的符号，与队伍色无关
    assert!(sprite_transform(size, 0, 256, 0.1, 0.0).flip_h);
    assert!(!sprite_transform(size, 0, 256, 0.0, 0.0).flip_h); // 0 不算 > 0
    assert!(!sprite_transform(size, 0, 256, -0.1, 0.0).flip_h);

    // 贴图越宽，缩放越小（128/tex_w）
    let a = sprite_transform(size, 0, 128, 1.0, 1.0);
    let b = sprite_transform(size, 0, 256, 1.0, 1.0);
    assert!((a.scale.x / b.scale.x - 2.0).abs() < 1e-5);
}

// ---------------------------------------------------------------------------
// `update_name_label` 的布局
// ---------------------------------------------------------------------------

#[test]
fn name_label_layout_matches_update_name_label_arithmetic() {
    let label = Vec2::new(500.0, 300.0);
    let min = Vec2::new(80.0, 40.0);
    let k = 500.0_f32 / 50.0; // 10

    // 士兵在原点、无镜像：cx = 250 + 0 - 40 = 210（在 [0, w-mw] 内 ⇒ 不改）
    let l = name_label_layout(label, min, 0, 0.0, 0.0, 1.0);
    assert_eq!(
        l,
        NameLabelLayout {
            position: Vec2::new(210.0, (150.0 + k * -1.5 - 20.0) as f64),
            // modulate RGB 恒 (1,1,1)，第 4 通道 = alpha
            modulate: Color::new(1.0, 1.0, 1.0, 1.0),
        }
    );
    // base = 150 - 15 - 20 = 115 ≥ 0 ⇒ 不翻面
    assert!((l.position.y - 115.0).abs() < 1e-3);
    assert_eq!(l.modulate.r, 1.0);
    assert_eq!(l.modulate.g, 1.0);
    assert_eq!(l.modulate.b, 1.0);

    // alpha 透传进 modulate.a（死亡淡出就是靠这个）
    let faded = name_label_layout(label, min, 0, 0.0, 0.0, 0.25);
    assert_eq!(faded.modulate.a, 0.25);
    assert_eq!(faded.position, l.position); // alpha 不影响位置

    // 下界：cx 被 max(cx, 0) 夹住（fcmp/fcsel mi @0x4bf7b4）
    let far_left = name_label_layout(label, min, 0x0000_0100, 100.0, 0.0, 1.0);
    assert_eq!(far_left.position.x, 0.0);
    // 同样输入、无镜像时不会被夹（cx = 250 + 1000 - 40 = 1210 > 0）
    let no_mirror = name_label_layout(label, min, 0, 100.0, 0.0, 1.0);
    assert!(no_mirror.position.x > 0.0);

    // 上界：mw + cx > w 时取 w - mw（fcmp @0x4bf7d8）
    let far_right = name_label_layout(label, min, 0, 100.0, 0.0, 1.0);
    assert_eq!(far_right.position.x, 500.0 - 80.0);

    // y 下界：base < 0 时改用 alt = base + (k·1.75 + mh/2)（fcmp/fcsel mi @0x4bf7ec）
    // base = 150 - 10·y - 15 - 20；y = 20 ⇒ base = -85 < 0 ⇒ 取 alt = -85 + 37.5 = -47.5
    let below = name_label_layout(label, min, 0, 0.0, 20.0, 1.0);
    let base = 150.0_f32 - 10.0 * 20.0 - 15.0 - 20.0;
    assert!(base < 0.0);
    let alt = base + (k * 1.75 + 20.0);
    assert!((below.position.y - alt as f64).abs() < 1e-3);
    // 原版**没有**再做一次 max(·,0)：alt 本身仍可为负（这里就是 -47.5）。
    // 只断言"确实换用了另一支"，不去假装它被夹到了正数。
    assert!(below.position.y > base as f64);
    // y 够大（base ≥ 0）时就不换支：y = 5 ⇒ base = 150-50-15-20 = 65
    let stays = name_label_layout(label, min, 0, 0.0, 5.0, 1.0);
    assert!((stays.position.y - 65.0).abs() < 1e-3);
}

// ---------------------------------------------------------------------------
// 死亡淡出
// ---------------------------------------------------------------------------

#[test]
fn death_fade_is_one_minus_tick_over_sixty() {
    // cmp w8,#0x3c @0x4bbb38；d12 = 0xC04E000000000000 = -60.0f64
    assert_eq!(DEATH_FADE_TICKS, 60);
    assert_eq!(DEATH_FADE_DIVISOR, -60.0);
    assert_eq!(DEATH_FADE_DIVISOR.to_bits(), 0xC04E_0000_0000_0000);

    // t = 0 ⇒ 全不透明；t = 30 ⇒ 0.5；t = 59 ⇒ 1/60
    assert_eq!(death_alpha(0), 1.0);
    assert!((death_alpha(30) - 0.5).abs() < 1e-15);
    assert!((death_alpha(59) - (1.0 / 60.0)).abs() < 1e-15);
    // t ≥ 60 ⇒ 原版走"藏起来"分支，本函数返回 0
    assert_eq!(death_alpha(60), 0.0);
    assert_eq!(death_alpha(u32::MAX), 0.0);
    // 单调不增
    let mut prev = 2.0;
    for t in 0..200u32 {
        let a = death_alpha(t);
        assert!(a <= prev);
        assert!((0.0..=1.0).contains(&a));
        prev = a;
    }
}

// ---------------------------------------------------------------------------
// `draw_offset_triangle` = draw() 里的一个箭头
// ---------------------------------------------------------------------------

#[test]
fn offset_triangle_is_the_arrow_polygon_from_draw() {
    let c = Vec2::new(3.0, -2.0);
    let r = 0.857;
    let theta = 1.0;
    let col = Color::from_packed_rgb(0x00_68_7d);

    let p = offset_triangle(c, r, theta, col);
    match &p {
        Primitive::Polygon { points, colors } => {
            // 顶点与既有 `arrow_head` 完全一致（几何结论没变，只是补了真名）
            assert_eq!(points.as_slice(), &arrow_head(c, r, theta));
            // 同一个颜色 3 份（x86 0x4b9c1e/0x4b9c3c/0x4b9c5c）
            assert_eq!(colors.len(), 3);
            assert!(colors.iter().all(|x| *x == col));
        }
        other => panic!("expected Polygon, got {:?}", other),
    }

    // `draw` 的 3 个箭头 == 3 次 `draw_offset_triangle`
    let inp = input(180.0, Some(120.0));
    let scale = (0.5716666_f32 * 1.5_f32) as f64;
    let angles = segment_angles(inp.v);
    let expect: Vec<Primitive> = angles
        .iter()
        .map(|&a| offset_triangle(inp.center, scale, a, inp.color))
        .collect();
    assert_eq!(arrow_primitives(inp), expect);
}
