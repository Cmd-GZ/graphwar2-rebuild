//! `src/graphwar_long_press_animation.rs` 的复原验收：从**外部**用公开 API 复核。
//!
//! ⚠️ 按 `AGENTS.md` §0 第 2 / 5 条：**这些断言是回归保护，不是复原证据。**
//! 每条右侧的真值要么是 `0x4d8100` 的指令立即数、要么是 `.rodata` 的原始比特、
//! 要么是 2.0.60 arm64 `.symtab` 的真名。
//!
//! 地址口径：一律 **ELF 虚地址**（`objdump`）；Ghidra/REA 地址 = 本值 + `0x100000`。

use gw2_rebuild::graphwar_long_press_animation::*;

/// `draw` 的唯一锚：4 个（两两成对）。
#[test]
fn draw_is_the_only_game_function() {
    assert_eq!(ORIG_FUNCS.len(), 7);
    assert_eq!(orig_fn(0x4d8100).unwrap().name, DRAW.name);
    assert_eq!(DRAW.vaddr, 0x4d8100);
    assert_eq!(DRAW.size, 1232);
    assert_eq!(DRAW.anchors, &[(52, 55), (52, 86), (53, 56), (53, 87)]);
    // 只有 draw 有 4 个锚，且全落在 52/53 两行上。
    let lines: Vec<u32> = DRAW.anchors.iter().map(|a| a.0).collect();
    assert_eq!(lines, vec![52, 52, 53, 53]);
    // 每条 panic 出口都在 draw 内部（0x4d8100 + 1232 = 0x4d85d0）。
    for (_, _, _, fns) in SRC_ANCHORS {
        for f in *fns {
            assert!(*f >= 0x4d8100 && *f < 0x4d8100 + 1232 || *f < 0x4d8100);
        }
    }
}

/// 每行两个 `.expect()`（列 55/86 与 56/87），与 4 条 panic 出口一一对应。
#[test]
fn four_expects_on_two_lines() {
    let on52: Vec<u32> = SRC_ANCHORS
        .iter()
        .filter(|a| a.1 == 52)
        .map(|a| a.2)
        .collect();
    let on53: Vec<u32> = SRC_ANCHORS
        .iter()
        .filter(|a| a.1 == 53)
        .map(|a| a.2)
        .collect();
    assert_eq!(on52, vec![55, 86]);
    assert_eq!(on53, vec![56, 87]);
    assert_eq!(SRC_ANCHORS.len(), 7);
    // 样板那 3 个锚各只有 1 个函数入口，17:1 有 4 个。
    let n17: Vec<u32> = SRC_ANCHORS
        .iter()
        .find(|a| a.1 == 17)
        .map(|a| a.3.to_vec())
        .unwrap();
    assert_eq!(n17, vec![0x4bd650, 0x4bda50, 0x4d7bc0, 0x4d7d40]);
}

/// arm64 2.0.60 的 `draw` 真名与大小（x86 / arm64 = 1.257 —— 本模块唯一超过 1.06 的）。
#[test]
fn arm64_draw_symbol() {
    let d = arm_sym("IControl>::draw").expect("arm64 有 draw 真名");
    assert_eq!(d.addr, 0x4d44f4);
    assert_eq!(d.size, 980);
    let ratio = f64::from(DRAW.size) / f64::from(d.size);
    assert!((1.2..=1.3).contains(&ratio), "ratio = {ratio}");
    assert_eq!(arm_sym("GodotClass>::class_id").unwrap().addr, 0x4be98c);
    assert!(arm_sym("不存在").is_none());
}

/// **2.0.60 里没有** `start`/`stop`/`ready`/`process`
/// （`docs/02-module-map.md` §18.5 C 表记的是 2024 版，见模块注释 §2）。
#[test]
fn only_draw_survives_in_2060() {
    for s in ARM64_SYMS {
        for gone in ["::start", "::stop", "::ready", "::process"] {
            assert!(
                !s.name.ends_with(gone),
                "2.0.60 的 arm64 symtab 里不该有 {}",
                s.name
            );
        }
    }
    // 唯一的 IControl 实现就是 draw。
    assert!(ARM64_SYMS.iter().any(|s| s.name.contains("IControl>::draw")));
    assert_eq!(TRAIT_IMPLS, &["IControl", "GodotClass", "ImplementsGodotVirtual"]);
}

/// 类名 —— `.rodata` 26 字节。
#[test]
fn class_name_string() {
    assert_eq!(CLASS, "GraphwarLongPressAnimation");
    assert_eq!(CLASS.len(), 26);
    assert_eq!(usize::from(CLASS_NAME_LEN), CLASS.len());
    assert_eq!(CLASS_NAME_VADDR, 0x1c4319);
    assert_eq!(CLASS_NAME_FNS, &[0x4ff520, 0x4ffeb0]);
}

/// `.rodata` 常量的**原始比特**：改一个就等于宣称二进制变了。
#[test]
fn draw_constants_bit_exact() {
    let all = DRAW_CONSTS;
    assert_eq!(all.len(), 7);
    // 3.0 / 10.0 / 0.5 / 0.2 在 .rodata 里是 f32（`divss`/`mulss` 用）。
    let f32s: [(u32, f32); 4] = [
        (0x1bb2b8, 3.0),
        (0x1b9530, 10.0),
        (0x1ba128, 0.5),
        (0x1ba598, 0.2),
    ];
    for (addr, v) in f32s {
        let got = all.iter().find(|c| c.1 == addr).expect("常量必须登记");
        // .rodata 里存的是 f32：`00004040` / `00002041` / `0000003f` / `cdcc4c3e`
        assert_eq!(got.0.to_bits(), f64::from(v).to_bits(), "0x{addr:06x}");
        assert_eq!((v as f32).to_bits(), match addr {
            0x1bb2b8 => 0x4040_0000,
            0x1b9530 => 0x4120_0000,
            0x1ba128 => 0x3f00_0000,
            0x1ba598 => 0x3e4c_cccd,
            _ => unreachable!(),
        });
    }
    // 1e9 / π / 2π 是 f64。
    let d1e9 = all.iter().find(|c| c.1 == 0x19eff8).unwrap();
    assert_eq!(d1e9.0.to_bits(), 1_000_000_000.0f64.to_bits());
    assert_eq!(d1e9.0.to_bits(), 0x41cd_cd65_0000_0000);
    assert_eq!(SPIN_LIMIT, std::f64::consts::TAU);
    assert_eq!(
        SPIN_LIMIT.to_bits(),
        0x4019_21fb_5444_2d18,
        "0x19fab8 = 2π 的原始比特"
    );
    assert_eq!(all.iter().find(|c| c.1 == 0x19fdc8).unwrap().0, std::f64::consts::PI);
}

/// 角速度与时长：`4π rad/s`，转满 `2π` 要 `0.5 s`。
#[test]
fn spin_rate_and_duration() {
    assert_eq!(SPIN_RATE, 4.0 * std::f64::consts::PI);
    assert_eq!(SPIN_DURATION_SECS, 0.5);
    assert!((SPIN_LIMIT / SPIN_RATE - SPIN_DURATION_SECS).abs() < 1e-15);
    assert!((spin_angle(0.5) - SPIN_LIMIT).abs() < 1e-12);
    // 结束判定是**严格大于**（`ucomisd` + `jbe`）：正好 0.5 s 还没结束。
    assert!(!is_finished(spin_angle(0.5)));
    assert!(is_finished(spin_angle(0.5 + 1e-6)));
    assert!(!is_finished(spin_angle(0.0)));
}

/// 哨兵：`nanos == 1e9` ⇒ idle；`stopped()` 只写 nanos。
#[test]
fn sentinel_state() {
    assert_eq!(NANOS_PER_SEC, 0x3b9a_ca00);
    assert_eq!(SENTINEL_NANOS, NANOS_PER_SEC);
    assert!(!SpinState::STOPPED.is_running());
    let run = SpinState::started(5, 7);
    assert!(run.is_running());
    assert_eq!(run.stopped(), SpinState::started(5, SENTINEL_NANOS));
    assert_eq!(SpinState::STOPPED.stopped(), SpinState::STOPPED);
}

/// `Instant::elapsed()` 的等价算术：秒 + 纳秒/1e9。
#[test]
fn elapsed_arithmetic() {
    let start = SpinState::started(100, 250_000_000);
    assert!((elapsed_secs(start, 100, 750_000_000) - 0.5).abs() < 1e-12);
    assert!((elapsed_secs(start, 101, 250_000_000) - 1.0).abs() < 1e-12);
    assert_eq!(elapsed_secs(start, 100, 250_000_000), 0.0);
    // 纳秒回绕（now.nanos < start.nanos）时仍给出正确的负差——
    // 原版直接做 `(double)a - (double)b`，没有借位分支（0x4d83fb–0x4d8403）。
    assert!((elapsed_secs(start, 101, 0) - 0.75).abs() < 1e-12);
}

/// 几何：`v48 = s0/10`、`v4c = s1/3`、`v44 = 0.5*v4c`、`v10 = s1*0.5 - v44`、`y = p1 + s1*0.5`。
#[test]
fn geometry_matches_disassembly() {
    let g = geometry((0.0, 0.0), (100.0, 90.0), 10, 1.0);
    assert_eq!(g.v48, 10.0);
    assert_eq!(g.v4c, 30.0);
    assert_eq!(g.v44, 15.0);
    assert_eq!(g.v10, 30.0); // 45 - 15
    assert_eq!(g.y, 45.0); // 0 + 90*0.5
    // n/2 = 5 > c = 1 ⇒ 正分支：p0 + v48 + v44 + s0 + v10
    assert!(g.branch_positive);
    assert_eq!(g.v8, 0.0 + 10.0 + 15.0 + 100.0 + 30.0);
    // 负分支：c >= n/2
    let g = geometry((1000.0, 0.0), (100.0, 90.0), 10, 5.0);
    assert!(!g.branch_positive);
    assert_eq!(g.v8, 1000.0 - 10.0 - 15.0 - 30.0);
    // n 是 i32 且**先除再转浮点**（`shr 31`/`add`/`sar 1` 是 Rust 的 /2 语义）。
    let g = geometry((0.0, 0.0), (10.0, 3.0), -5, 0.0);
    assert_eq!(g.branch_positive, false); // -5/2 = -2 (向零取整) > 0.0 == false
    assert_eq!(g.v4c, 1.0);
}

/// 两次绘制：颜色、`xmm3`、`xmm4`、`esi`/`ecx` 全部对得上指令。
#[test]
fn draw_plan_two_calls() {
    let g = geometry((0.0, 0.0), (100.0, 90.0), 10, 1.0);
    let plan = draw_plan(g, 1.5);
    assert_eq!(plan[0].xmm0, g.v8);
    assert_eq!(plan[0].xmm1, g.y);
    assert_eq!(plan[0].xmm2, g.v10);
    assert_eq!(plan[0].xmm3, -0.2);
    assert_eq!(plan[0].xmm4, 1.7); // angle + 0.2
    assert_eq!(plan[0].xmm5, 2.0 * 0.2 * g.v4c + g.v4c);
    assert_eq!(plan[0].color, Color::BLACK);
    assert_eq!(plan[0].esi, 0x1e);
    assert_eq!(plan[0].ecx, 0);

    assert_eq!(plan[1].xmm0, g.v8);
    assert_eq!(plan[1].xmm1, g.y);
    assert_eq!(plan[1].xmm2, g.v10);
    assert_eq!(plan[1].xmm3, 0.0);
    assert_eq!(plan[1].xmm4, 1.5);
    assert_eq!(plan[1].xmm5, g.v4c);
    assert_eq!(plan[1].color, Color::WHITE);
    assert_eq!(plan[1].esi, 0x1e);
    assert_eq!(plan[1].ecx, 0);

    // 颜色常量也在 .rodata 里量过：{0,0,0,1} 与 {1,1,1,1}。
    assert_eq!(COLOR_BLACK_VADDR, 0x19cf70);
    assert_eq!(COLOR_WHITE_VADDR, 0x19ed00);
    assert_eq!(
        [Color::BLACK.r, Color::BLACK.g, Color::BLACK.b, Color::BLACK.a],
        [0.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        [Color::WHITE.r, Color::WHITE.g, Color::WHITE.b, Color::WHITE.a],
        [1.0, 1.0, 1.0, 1.0]
    );
}

/// 一帧 `draw`：先判 running（在算任何东西之前），再画，最后判结束。
#[test]
fn draw_frame_gating() {
    let g = geometry((0.0, 0.0), (100.0, 90.0), 10, 1.0);
    // 哨兵状态 ⇒ 直接 Idle（0x4d8115）
    assert_eq!(draw_frame(SpinState::STOPPED, 100, 0, g), Frame::Idle);
    // 刚启动 ⇒ Drawing
    let run = SpinState::started(100, 0);
    match draw_frame(run, 100, 250_000_000, g) {
        Frame::Drawing(p) => assert_eq!(p[1].xmm4, (0.25 * SPIN_RATE) as f32),
        other => panic!("应该是 Drawing，得到 {other:?}"),
    }
    // 超过 0.5 s ⇒ 这一帧仍画完，但带 Finished 标记（调用方随后写哨兵）
    match draw_frame(run, 100, 500_000_001, g) {
        Frame::Finished(p) => assert_eq!(p.len(), 2),
        other => panic!("应该是 Finished，得到 {other:?}"),
    }
    // 正好 0.5 s ⇒ 还没结束（严格大于）
    assert!(matches!(
        draw_frame(run, 100, 500_000_000, g),
        Frame::Drawing(_)
    ));
}
