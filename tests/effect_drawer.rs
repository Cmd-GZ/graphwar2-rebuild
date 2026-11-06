//! `effect_drawer` 的验收断言。
//!
//! ⚠️ 按 `AGENTS.md` §0.2：**这些条数是回归保护，不是复原证据**。
//! 每个期望值要么是 `.rodata` 的字节、要么是某条指令的立即数，注释给出地址。
//!
//! 涉及函数（x86 vaddr；Ghidra = +0x100000）：
//! `draw` `0x4e1520`、`ready` `0x4e25b0`、`tick_event` 闭包 `0x4e2a30`、
//! `effect_info_event` 闭包 `0x4e2d50`、`process` `0x4e2f80`、`step_effect` `0x4eb450`。

use gw2_rebuild::effect_drawer::{
    alpha_at, draw_transform, effect_data_offset, effect_primitive, frame_primitives,
    polygon_alpha, step_effects, ticks_for_delta, Color, EffectData, Phase, Primitive, ProcessStep,
    Vec2D, ALPHA_GAIN, ALPHA_PHASE6, ANTIALIASED, CLASS_NAME, COLOR_PHASE0, COLOR_PHASE23,
    COLOR_PHASE45, COLOR_PHASE6, DRAW_VADDR, EFFECT_STRIDE, FIXED_STEP_SECONDS, HALF, INFO_CLOSURE_VADDR,
    PHASE_TICKS, PROCESS_STEPS, PROCESS_VADDR, RADIUS_LARGE, RADIUS_SMALL, READY_SUBSCRIPTIONS,
    READY_VADDR, SCALE_DIVISOR, STATE_TERMINAL_MASK, STEP_EFFECT_VADDR, SUBSCRIBED_EVENTS,
    TICK_CLOSURE_VADDR, TICKS_PER_SECOND, TIME_PROBE_LOCATION, TIME_PROBE_THRESHOLD_NS,
    WIDTH_UNUSED,
};

// ---------------------------------------------------------------------------
// 地址与接线
// ---------------------------------------------------------------------------

#[test]
fn addresses_and_wiring() {
    // draw 体 0x4e1520..=0x4e25af（0x4e25af 是 int3），ready 紧随其后
    assert_eq!(DRAW_VADDR, 0x4e1520);
    assert_eq!(READY_VADDR, 0x4e25b0);
    assert_eq!(TICK_CLOSURE_VADDR, 0x4e2a30);
    assert_eq!(INFO_CLOSURE_VADDR, 0x4e2d50);
    assert_eq!(PROCESS_VADDR, 0x4e2f80);
    assert_eq!(STEP_EFFECT_VADDR, 0x4eb450);
    // 五个函数按地址递增（与原版链接顺序一致）
    let order = [
        DRAW_VADDR,
        READY_VADDR,
        TICK_CLOSURE_VADDR,
        INFO_CLOSURE_VADDR,
        PROCESS_VADDR,
    ];
    assert!(order.windows(2).all(|w| w[0] < w[1]));

    // 订阅的两个事件名（.rodata 0x1c4225 长 17、0x1c4236 长 10）
    assert_eq!(SUBSCRIBED_EVENTS, ["effect_info_event", "tick_event"]);
    assert_eq!(SUBSCRIBED_EVENTS[0].len(), 17);
    assert_eq!(SUBSCRIBED_EVENTS[1].len(), 10);
    assert_eq!(READY_SUBSCRIPTIONS[0].0, "effect_info_event");
    assert_eq!(READY_SUBSCRIPTIONS[1].0, "tick_event");
    assert_eq!(READY_SUBSCRIPTIONS[0].1, 0x1c4225);
    assert_eq!(READY_SUBSCRIPTIONS[1].1, 0x1c4236);
    // 闭包 vtable：0x95fb50 → 0x4e2d50，0x95fb78 → 0x4e2a30
    assert_eq!(READY_SUBSCRIPTIONS[0].2, 0x95fb50);
    assert_eq!(READY_SUBSCRIPTIONS[1].2, 0x95fb78);

    // downcast 用的类名字面量（0x1c1cf5，长 41）
    assert_eq!(CLASS_NAME.len(), 41);

    // process 的探针：阈值 1e9 ns、位置串 24 字节（0x1c4240）
    assert_eq!(TIME_PROBE_THRESHOLD_NS, 1_000_000_000);
    assert_eq!(TIME_PROBE_LOCATION, "src/effect_drawer.rs:507");
    assert_eq!(TIME_PROBE_LOCATION.len(), 24);

    // process 的五步
    assert_eq!(PROCESS_STEPS.len(), 5);
    assert_eq!(PROCESS_STEPS[0], ProcessStep::StartTimeProbe);
    assert_eq!(PROCESS_STEPS[2], ProcessStep::RetainAlive);
    assert_eq!(PROCESS_STEPS[3], ProcessStep::QueueRedraw);
}

// ---------------------------------------------------------------------------
// 时间常数（本轮的核心新结论）
// ---------------------------------------------------------------------------

#[test]
fn time_constants_are_bit_exact() {
    // 槽 0x19f918 = 0x404e800000000000 = 60.0
    assert_eq!(TICKS_PER_SECOND.to_bits(), 0x404e_0000_0000_0000);
    // 槽 0x19ed20 / 0x19ed28 / 0x19f9f0 = 0x3f5b4e81b4e81b4f = 1/600
    assert_eq!(FIXED_STEP_SECONDS.to_bits(), 0x3f5b_4e81_b4e8_1b4f);
    assert!((FIXED_STEP_SECONDS - 1.0 / 600.0).abs() < 1e-18);
    // 相位推进 0x258 = 600 tick
    assert_eq!(PHASE_TICKS, 600);
    assert_eq!(PHASE_TICKS, 0x258);
    // 每个 tick 恰好 10 个子步
    assert!((TICKS_PER_SECOND * FIXED_STEP_SECONDS - 0.1).abs() < 1e-15);
    // 终态位掩码 0x5b（mov ecx,0x5b @ 0x4eb4bb / mov w11,#0x5b @ arm64 0x444d94）
    assert_eq!(STATE_TERMINAL_MASK, 0x5b);
    assert_eq!(STATE_TERMINAL_MASK, 0b0101_1011);
    // 透明度系数：槽 0x1b9534 = 0x3f666666 = 0.9
    assert_eq!(ALPHA_GAIN.to_bits(), 0x3f66_6666);
}

#[test]
fn delta_to_ticks_matches_the_clamp_sequence() {
    // 0x4eb479..0x4eb4a3：60*delta → max(0) → min(4294967295.0) → cvttsd2si
    assert_eq!(ticks_for_delta(0.0), 0);
    assert_eq!(ticks_for_delta(1.0 / 60.0), 1); // 一帧 = 1 tick
    assert_eq!(ticks_for_delta(1.0), 60); // 一秒 = 60 tick
    assert_eq!(ticks_for_delta(10.0), 600); // 十秒 = 600 tick = 一个相位
    assert_eq!(ticks_for_delta(-1.0), 0); // max(0, …) 夹住负值
    assert_eq!(ticks_for_delta(1e30), u32::MAX); // min(4294967295.0, …)
    // 截断而不是四舍五入（cvttsd2si）
    assert_eq!(ticks_for_delta(0.99 / 60.0), 0);
}

// ---------------------------------------------------------------------------
// EffectData 布局
// ---------------------------------------------------------------------------

#[test]
fn effect_data_layout_is_48_bytes() {
    // 元素步长：lea r15,[rax+rax*2]（rax = len<<4）@ 0x4eb471/0x4eb475 ⇒ len × 48
    assert_eq!(EFFECT_STRIDE, 0x30);
    assert_eq!(EFFECT_STRIDE, 48);
    // 字段偏移
    assert_eq!(effect_data_offset::POS, 0x00);
    assert_eq!(effect_data_offset::VEL, 0x10);
    assert_eq!(effect_data_offset::START_TICK, 0x20);
    assert_eq!(effect_data_offset::DURATION_TICKS, 0x24);
    assert_eq!(effect_data_offset::PHASE, 0x28);
    // 位置 2×f64 + 速度 2×f64 = 32 字节，正好到 0x20
    assert_eq!(effect_data_offset::VEL, effect_data_offset::POS + 16);
    assert_eq!(effect_data_offset::START_TICK, effect_data_offset::VEL + 16);
    // 两个 u32 挨着，相位字节在 0x28；0x2c 起 4 字节未知
    assert_eq!(effect_data_offset::DURATION_TICKS, effect_data_offset::START_TICK + 4);
    assert_eq!(effect_data_offset::PHASE, effect_data_offset::DURATION_TICKS + 4);
    assert!(EFFECT_STRIDE - effect_data_offset::PHASE > 1);
}

#[test]
fn effect_data_uses_48_bytes_of_rust() {
    // 我们的结构体不必须逐位等价，但至少不能比原版大
    assert!(std::mem::size_of::<EffectData>() <= EFFECT_STRIDE as usize);
}

// ---------------------------------------------------------------------------
// 存活与透明度
// ---------------------------------------------------------------------------

#[test]
fn lifetime_uses_start_plus_duration() {
    // 0x4e1950：edx = start + duration; cmp edx, now; jb 跳过绘制
    let e = EffectData::new(Vec2D::new(1.0, 2.0), 100, 30, Phase::CircleSmall);
    assert!(e.is_alive_at(100));
    assert!(e.is_alive_at(130)); // start + duration 那一刻还画
    assert!(!e.is_alive_at(131)); // 下一秒就不画了
    assert!(e.is_alive_at(99)); // 还没开始也算活（原版是无符号比较）
}

#[test]
fn alpha_fades_linearly_to_zero() {
    // alpha = max(0, (1 - t) * 0.9)
    assert_eq!(alpha_at(100, 100, 100), 0.9); // t = 0
    assert!((alpha_at(150, 100, 100) - 0.45).abs() < 1e-6); // t = 0.5
    assert_eq!(alpha_at(200, 100, 100), 0.0); // t = 1
    assert_eq!(alpha_at(300, 100, 100), 0.0); // 过期后夹到 0
    assert_eq!(alpha_at(90, 100, 100), 0.9); // now <= start ⇒ 默认 0.9
    assert_eq!(alpha_at(100, 100, 0), 0.9); // duration = 0 的退化情形

    // 相位 1 的分母是 0.9 而不是 duration：alpha = max(0, (1 - t/0.9) * 0.9)
    assert_eq!(polygon_alpha(0.0), 0.9);
    assert!(polygon_alpha(0.45) < 0.9);
    assert_eq!(polygon_alpha(0.9), 0.0);
    assert_eq!(polygon_alpha(2.0), 0.0);
}

// ---------------------------------------------------------------------------
// step_effect
// ---------------------------------------------------------------------------

#[test]
fn step_integrates_with_fixed_1_over_600() {
    let mut fx = [EffectData {
        pos: Vec2D::new(0.0, 0.0),
        vel: Vec2D::new(600.0, -600.0), // 600 单位/秒 ⇒ 每子步 1 单位
        start_tick: 0,
        duration_ticks: 600,
        phase: Phase::CircleSmall,
    }];
    // 一帧 = 1/60 s = 10 个子步
    step_effects(&mut fx, 0, 1.0 / 60.0, |_| false);
    assert!((fx[0].pos.x - 10.0).abs() < 1e-9);
    assert!((fx[0].pos.y + 10.0).abs() < 1e-9);

    let mut fx2 = [EffectData {
        pos: Vec2D::new(0.0, 0.0),
        vel: Vec2D::new(600.0, 0.0),
        start_tick: 0,
        duration_ticks: 600,
        phase: Phase::CircleSmall,
    }];
    // delta = 0 走"快路径"：不积分
    step_effects(&mut fx2, 0, 0.0, |_| false);
    assert_eq!(fx2[0].pos, Vec2D::ZERO);
}

#[test]
fn step_stops_on_obstacle() {
    // 0x4eb585 call is_obstacle + 0x4eb592 movupd [r13],xmm0（清零速度）
    let mut fx = [EffectData {
        pos: Vec2D::new(0.0, 0.0),
        vel: Vec2D::new(600.0, 0.0),
        start_tick: 0,
        duration_ticks: 600,
        phase: Phase::CircleSmall,
    }];
    let mut hits = 0;
    step_effects(&mut fx, 0, 1.0 / 60.0, |p| {
        if p.x > 4.0 {
            hits += 1;
            true
        } else {
            false
        }
    });
    assert!(hits > 0, "is_obstacle 必须被调用");
    assert_eq!(fx[0].vel, Vec2D::ZERO, "撞到障碍后速度清零");
    // 位置停在第 5 个子步附近（1,2,3,4,5 单位处判定为真）
    assert!(fx[0].pos.x >= 4.0 && fx[0].pos.x <= 5.0, "pos.x = {}", fx[0].pos.x);
}

#[test]
fn step_advances_terminal_and_non_terminal_phases_differently() {
    // bt 0x5b, phase ⇒ {0,1,3,4,6} 是终态，不推进
    // 0x5b = 0b0101_1011 ⇒ 位 0、1、3、4、6 置位
    for p in [Phase::CircleSmall, Phase::Polygon, Phase::CircleLargeB, Phase::LineA, Phase::CircleDark] {
        assert!(p.is_terminal(), "{p:?} 应该在 0x5b 掩码里");
    }
    // {2, 5} 不在掩码里 ⇒ 可以推进
    assert!(!Phase::CircleLargeA.is_terminal());
    assert!(!Phase::LineB.is_terminal());
    for b in 0u8..7 {
        let p = Phase::from_byte(b).unwrap();
        let expect = (STATE_TERMINAL_MASK >> b) & 1 != 0;
        assert_eq!(p.is_terminal(), expect, "phase {b}");
    }

    // delta <= 0 的快路径：到期就把 2 推到 3、其余推到 6，并把 start_tick 加 600
    let mut fx = [EffectData::new(Vec2D::ZERO, 0, 600, Phase::CircleLargeA)];
    step_effects(&mut fx, 0, 0.0, |_| false);
    assert_eq!(fx[0].phase, Phase::CircleLargeB); // 2 → 3（mov byte [rbx+rax+0x28],3 @ 0x4eb4f6）
    assert!(fx[0].phase.is_terminal(), "3 恰好在 0x5b 掩码里：换到 3 之后就落地不再推进");
    assert_eq!(fx[0].start_tick, PHASE_TICKS); // start += 0x258（add edx,0x258 @ 0x4eb4c2）

    // 还没到期（start_tick > elapsed）时什么都不变
    let mut fx = [EffectData::new(Vec2D::ZERO, 100, 600, Phase::CircleLargeA)];
    step_effects(&mut fx, 0, 0.0, |_| false);
    assert_eq!(fx[0].phase, Phase::CircleLargeA);
    assert_eq!(fx[0].start_tick, 100);
}

// ---------------------------------------------------------------------------
// 图元
// ---------------------------------------------------------------------------

#[test]
fn per_phase_primitives() {
    let size = (1600.0, 900.0);
    let mk = |phase: Phase| EffectData::new(Vec2D::new(3.0, -2.0), 0, 100, phase);

    // 相位 0：小圆 r=0.05，(0.6,0.6,0.1, α)
    match effect_primitive(&mk(Phase::CircleSmall), 0, true, size).unwrap() {
        Primitive::Circle { radius, color, filled, width, antialiased, .. } => {
            assert_eq!(radius, RADIUS_SMALL);
            assert_eq!(radius.to_bits(), 0x3d4c_cccd); // 槽 0x1ba860
            assert_eq!((color.r, color.g, color.b), (0.6, 0.6, 0.1));
            assert_eq!(color.a, 0.9);
            assert!(filled);
            assert_eq!(width, WIDTH_UNUSED);
            assert!(!antialiased);
        }
        other => panic!("相位 0 应该是圆：{other:?}"),
    }

    // 相位 2/3：大圆 r=0.06，(0.7,0.7,0.0, α)
    for p in [Phase::CircleLargeA, Phase::CircleLargeB] {
        match effect_primitive(&mk(p), 0, true, size).unwrap() {
            Primitive::Circle { radius, color, .. } => {
                assert_eq!(radius, RADIUS_LARGE);
                assert_eq!(radius.to_bits(), 0x3d75_c28f); // 槽 0x1b9e98
                assert_eq!((color.r, color.g, color.b), (0.7, 0.7, 0.0));
            }
            other => panic!("相位 {p:?} 应该是圆：{other:?}"),
        }
    }

    // 相位 6：大圆 + (0.1,0.1,0.1)，alpha 固定 0.7（不是淡出）
    match effect_primitive(&mk(Phase::CircleDark), 50, true, size).unwrap() {
        Primitive::Circle { radius, color, .. } => {
            assert_eq!(radius, RADIUS_LARGE);
            assert_eq!(color, Color::new(0.1, 0.1, 0.1, 0.7));
            assert_eq!(color.a, ALPHA_PHASE6);
        }
        other => panic!("相位 6 应该是圆：{other:?}"),
    }

    // 相位 4/5：线段，(0.9,0.9,0.1, α)
    for p in [Phase::LineA, Phase::LineB] {
        match effect_primitive(&mk(p), 50, true, size).unwrap() {
            Primitive::Line { color, .. } => {
                assert_eq!((color.r, color.g, color.b), (0.9, 0.9, 0.1));
            }
            other => panic!("相位 {p:?} 应该是线：{other:?}"),
        }
    }

    // 相位 1：多边形（顶点未知，但"画多边形"这个事实必须保留）
    match effect_primitive(&mk(Phase::Polygon), 0, true, size).unwrap() {
        Primitive::Polygon { colors, .. } => {
            assert_eq!(colors.len(), 1);
            assert_eq!((colors[0].r, colors[0].g, colors[0].b), (0.6, 0.6, 0.1));
        }
        other => panic!("相位 1 应该是多边形：{other:?}"),
    }

    // 基色常量与相位表的对应关系
    assert_eq!(Phase::CircleSmall.base_color(), COLOR_PHASE0);
    assert_eq!(Phase::CircleLargeA.base_color(), COLOR_PHASE23);
    assert_eq!(Phase::LineA.base_color(), COLOR_PHASE45);
    assert_eq!(Phase::CircleDark.base_color(), COLOR_PHASE6);
}

#[test]
fn expired_effects_are_not_drawn() {
    let e = EffectData::new(Vec2D::ZERO, 10, 20, Phase::CircleSmall);
    assert!(effect_primitive(&e, 30, true, (1600.0, 900.0)).is_some());
    assert!(effect_primitive(&e, 31, true, (1600.0, 900.0)).is_none());
}

#[test]
fn frame_starts_with_one_transform() {
    let size = (1600.0, 900.0);
    let fx = [
        EffectData::new(Vec2D::new(0.0, 0.0), 0, 100, Phase::CircleSmall),
        EffectData::new(Vec2D::new(1.0, 1.0), 0, 100, Phase::LineA),
        // 过期的一条不应该出现在帧里
        EffectData::new(Vec2D::new(2.0, 2.0), 0, 1, Phase::CircleSmall),
    ];
    let prims = frame_primitives(&fx, 50, true, size);
    assert_eq!(prims.len(), 1 + 2);
    assert!(matches!(prims[0], Primitive::SetTransform(_)));

    // 变换：position = size/2、rotation = 0、scale = (W/50, -W/50)
    let t = draw_transform(size.0, size.1, true);
    assert_eq!(t.position, (800.0, 450.0));
    assert_eq!(t.rotation, 0.0);
    assert_eq!(t.scale, (32.0, -32.0));
    assert_eq!(t.scale.0, size.0 / SCALE_DIVISOR);
    // flip_y = false 时不取反
    assert_eq!(draw_transform(size.0, size.1, false).scale, (32.0, 32.0));
}

#[test]
fn phase_table_has_seven_entries() {
    // 跳表 0x1c3b8c 的有效项 0..=6
    assert_eq!(Phase::COUNT, 7);
    for b in 0u8..7 {
        assert!(Phase::from_byte(b).is_some(), "phase {b}");
    }
    assert!(Phase::from_byte(7).is_none());
    // 圆的半径只有两种，且只有 4 个相位是圆
    let circles = (0u8..7)
        .filter_map(Phase::from_byte)
        .filter(|p| p.circle_radius().is_some())
        .count();
    assert_eq!(circles, 4);
    // antialiased 恒为 false（每个调用点前 xor ecx,ecx）
    assert!(!ANTIALIASED);
}
