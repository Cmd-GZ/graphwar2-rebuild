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
