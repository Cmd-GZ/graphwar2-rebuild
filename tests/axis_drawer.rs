//! `axis_drawer` 的验收断言。
//!
//! ⚠️ 按 `AGENTS.md` §0.2：**这些条数是回归保护，不是复原证据**。
//! 每个期望值都抄自 `libgraphwarrust.so` 的机器码，注释里给出指令地址 /
//! `.rodata` 槽地址，方便回到反汇编逐条复核。
//!
//! 目标函数：`AxisDrawer::draw` = **x86 vaddr `0x4e05b0`**（体长 `0x670`），
//! arm64 2.0.60 同名符号 @ `0x43b400`（长 1492）。

use gw2_rebuild::axis_drawer::{
    axis_primitives, line_count, unit_px, AxisMode, Color, Primitive, Rect2, Vec2, AXIS_COLOR,
    AXIS_WIDTH, BOARD_COLOR, BOARD_RECT, DRAW_ARM64_VADDR, DRAW_LEN, DRAW_VADDR, GRID_WIDTH,
    MAJOR_COLOR, MAJOR_STEP, MINOR_COLOR, MINOR_STEP, RECT_WIDTH, SCALE_DIVISOR, X_MAX, X_MIN,
    Y_MAX, Y_MIN,
};

fn size() -> Vec2 {
    Vec2::new(1600.0, 900.0)
}

// ---------------------------------------------------------------------------
// 常量：全部抄 `.rodata`
// ---------------------------------------------------------------------------

#[test]
fn constants_match_rodata_slots() {
    // 棋盘边界：槽 0x1b9670 / 0x1b9b8c / 0x1bb818 / 0x1b9268
    assert_eq!(X_MIN, -25.0);
    assert_eq!(X_MAX, 25.0);
    assert_eq!(Y_MIN, -15.0);
    assert_eq!(Y_MAX, 15.0);

    // 缩放分母：槽 0x1b9fc8，divss @ 0x4e0639
    assert_eq!(SCALE_DIVISOR, 50.0);
    assert_eq!(SCALE_DIVISOR, X_MAX - X_MIN);

    // 线宽：槽 0x1ba860 = 0x3d4ccccd（0.05）、槽 0x1ba864 = 0x3d99999a（0.075）
    assert_eq!(GRID_WIDTH.to_bits(), 0x3d4c_cccd);
    assert_eq!(AXIS_WIDTH.to_bits(), 0x3d99_999a);

    // 背景矩形：16 字节槽 0x19cc20 = [-25, -15, 50, 30]
    assert_eq!(BOARD_RECT.pos, Vec2::new(-25.0, -15.0));
    assert_eq!(BOARD_RECT.size, Vec2::new(50.0, 30.0));
    assert_eq!(BOARD_RECT.size.x, X_MAX - X_MIN);
    assert_eq!(BOARD_RECT.size.y, Y_MAX - Y_MIN);

    // draw_rect 的 width 参数：槽 0x1b9d08 = -1.0（filled 时被忽略，但原版确实传）
    assert_eq!(RECT_WIDTH, -1.0);

    // 网格间隔：细 = 1（0x4e06eb 的 -25 起、步长 1），粗 = 5（0x4e08e5 的 add eax,5）
    assert_eq!(MINOR_STEP, 1);
    assert_eq!(MAJOR_STEP, 5);
}

#[test]
fn colors_are_the_three_rodata_quadruples() {
    // 槽 0x19ed00：66.. 全 1
    assert_eq!(BOARD_COLOR, Color::new(1.0, 1.0, 1.0, 1.0));
    // 槽 0x19eba0：66 66 66 3f ×3 + 00 00 80 3f
    assert_eq!(MINOR_COLOR, Color::new(0.9, 0.9, 0.9, 1.0));
    // 槽 0x19d5f0：33 33 33 3f ×3 + 00 00 80 3f
    assert_eq!(MAJOR_COLOR, Color::new(0.7, 0.7, 0.7, 1.0));
    // 槽 0x19c430：cd cc 4c 3e ×3 + 00 00 80 3f
    assert_eq!(AXIS_COLOR, Color::new(0.2, 0.2, 0.2, 1.0));
    // 网格/轴的灰阶必须严格递减（细 > 粗 > 轴）
    assert!(MINOR_COLOR.r > MAJOR_COLOR.r);
    assert!(MAJOR_COLOR.r > AXIS_COLOR.r);
}

#[test]
fn addresses() {
    // 名字来源：arm64 .symtab 的 <… as IControl>::draw
    assert_eq!(DRAW_VADDR, 0x4e05b0);
    assert_eq!(DRAW_LEN, 0x670);
    assert_eq!(DRAW_VADDR + DRAW_LEN, 0x4e0c20); // 下一个函数起点
    assert_eq!(DRAW_ARM64_VADDR, 0x43b400);
}

// ---------------------------------------------------------------------------
// 变换
// ---------------------------------------------------------------------------

#[test]
fn transform_is_centered_and_isotropic() {
    let p = axis_primitives(AxisMode::NoAxis, size());
    match p[0] {
        Primitive::SetTransform {
            position,
            rotation,
            scale,
        } => {
            // position = size * 0.5（mulss [0x1ba128 = 0.5] @ 0x4e0659/0x4e065d）
            assert_eq!(position, Vec2::new(800.0, 450.0));
            assert_eq!(rotation, 0.0);
            // scale = (W/50, -W/50)：divss @ 0x4e0639 + xorps(-0.0) @ 0x4e0648
            assert_eq!(scale.x, 32.0);
            assert_eq!(scale.y, -32.0);
            assert_eq!(scale.x, -scale.y);
            // 各向同性：一个棋盘单位在 x/y 上一样长（y 只是翻了个向）
            assert_eq!(unit_px(size()), scale.x);
            assert_eq!(unit_px(size()) * SCALE_DIVISOR, size().x);
        }
        other => panic!("第 0 条不是 SetTransform：{other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 计数：三种模式各自的 draw_line 条数
// ---------------------------------------------------------------------------

#[test]
fn line_counts_per_mode() {
    // NoAxis：一条线都不画
    assert_eq!(line_count(AxisMode::NoAxis), 0);
    // OnlyMain：x=0 与 y=0
    assert_eq!(line_count(AxisMode::OnlyMain), 2);
    // EveryFive：(X: 1 左边界 + 10) + (Y: 1 下边界 + 6) + 2 轴 = 20
    assert_eq!(line_count(AxisMode::EveryFive), 20);
    // EveryUnit：20 + 竖直 51 + 水平 31 = 102
    assert_eq!(line_count(AxisMode::EveryUnit), 102);
    // 与构造出的图元数一致
    for m in AxisMode::ALL {
        let n = axis_primitives(m, size())
            .iter()
            .filter(|p| matches!(p, Primitive::Line { .. }))
            .count();
        assert_eq!(n, line_count(m), "mode = {m:?}");
    }
}

#[test]
fn primitive_totals_per_mode() {
    // 每条模式都有 1 条 transform + 1 条背景 rect
    for m in AxisMode::ALL {
        let p = axis_primitives(m, size());
        assert!(matches!(p[0], Primitive::SetTransform { .. }));
        assert!(matches!(p[1], Primitive::Rect { .. }));
        assert_eq!(p.len(), 2 + line_count(m));
    }
}

// ---------------------------------------------------------------------------
// 背景矩形
// ---------------------------------------------------------------------------

#[test]
fn background_rect_is_the_whole_board() {
    let p = axis_primitives(AxisMode::NoAxis, size());
    match p[1] {
        Primitive::Rect {
            rect,
            color,
            filled,
            width,
            antialiased,
        } => {
            assert_eq!(rect, Rect2 {
                pos: Vec2::new(-25.0, -15.0),
                size: Vec2::new(50.0, 30.0),
            });
            assert_eq!(color, Color::new(1.0, 1.0, 1.0, 1.0));
            // ecx = 1（0x4e06c2）；r8d = 0（0x4e06c7）
            assert!(filled);
            assert!(!antialiased);
            assert_eq!(width, -1.0);
        }
        other => panic!("第 1 条不是 Rect：{other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 网格 / 轴的具体坐标
// ---------------------------------------------------------------------------

#[test]
fn minor_grid_runs_over_the_whole_plane() {
    let p = axis_primitives(AxisMode::EveryUnit, size());
    let lines: Vec<(Vec2, Vec2)> = p
        .iter()
        .filter_map(|q| match q {
            Primitive::Line { from, to, color, width, .. } if *color == MINOR_COLOR => {
                assert_eq!(*width, GRID_WIDTH);
                Some((*from, *to))
            }
            _ => None,
        })
        .collect();

    // 竖直：x = -25..=25（51 条），y 从 -15 到 +15
    let vertical: Vec<_> = lines
        .iter()
        .filter(|(a, b)| a.x == b.x && a.y == Y_MIN && b.y == Y_MAX)
        .collect();
    assert_eq!(vertical.len(), 51);
    assert_eq!(vertical.first().unwrap().0.x, X_MIN);
    assert_eq!(vertical.last().unwrap().0.x, X_MAX);

    // 水平：y = -15..=15（31 条），x 从 -25 到 +25
    let horizontal: Vec<_> = lines
        .iter()
        .filter(|(a, b)| a.y == b.y && a.x == X_MIN && b.x == X_MAX)
        .collect();
    assert_eq!(horizontal.len(), 31);
    assert_eq!(horizontal.first().unwrap().0.y, Y_MIN);
    assert_eq!(horizontal.last().unwrap().0.y, Y_MAX);

    assert_eq!(vertical.len() + horizontal.len(), lines.len());
}

#[test]
fn major_grid_is_every_five_units() {
    let p = axis_primitives(AxisMode::EveryFive, size());
    let mut xs: Vec<i32> = Vec::new();
    let mut ys: Vec<i32> = Vec::new();
    for q in &p {
        if let Primitive::Line { from, to, color, .. } = q {
            if *color == MAJOR_COLOR {
                if from.x == to.x {
                    xs.push(from.x as i32);
                } else {
                    ys.push(from.y as i32);
                }
            }
        }
    }
    xs.sort_unstable();
    ys.sort_unstable();
    // 竖直：左边界 -25 + 循环 -20..=25 ⇒ [-25, -20, -15, -10, -5, 0, 5, 10, 15, 20, 25]
    assert_eq!(xs, vec![-25, -20, -15, -10, -5, 0, 5, 10, 15, 20, 25]);
    // 水平：下边界 -15 + 循环 -10..=15 ⇒ [-15, -10, -5, 0, 5, 10, 15]
    assert_eq!(ys, vec![-15, -10, -5, 0, 5, 10, 15]);
}

#[test]
fn main_axis_is_two_thick_lines_through_the_origin() {
    let p = axis_primitives(AxisMode::OnlyMain, size());
    let axes: Vec<_> = p
        .iter()
        .filter_map(|q| match q {
            Primitive::Line { from, to, color, width, antialiased } => {
                assert_eq!(*color, AXIS_COLOR);
                assert_eq!(*width, AXIS_WIDTH);
                assert!(!antialiased);
                Some((*from, *to))
            }
            _ => None,
        })
        .collect();
    assert_eq!(axes.len(), 2);
    // 顺序：竖轴（x=0）先发（0x4e0b02），横轴（y=0）后发（0x4e0b62）
    assert_eq!(axes[0], (Vec2::new(0.0, Y_MIN), Vec2::new(0.0, Y_MAX)));
    assert_eq!(axes[1], (Vec2::new(X_MIN, 0.0), Vec2::new(X_MAX, 0.0)));
}

// ---------------------------------------------------------------------------
// 模式判别值
// ---------------------------------------------------------------------------

#[test]
fn axis_mode_discriminants_and_predicates() {
    // 判别值 = 变体顺序（推断，见 src/axis_drawer.rs 的 AxisMode 文档）
    assert_eq!(AxisMode::NoAxis as u8, 0);
    assert_eq!(AxisMode::OnlyMain as u8, 1);
    assert_eq!(AxisMode::EveryFive as u8, 2);
    assert_eq!(AxisMode::EveryUnit as u8, 3);

    // 三个判断与反汇编的三条比较一一对应：
    //   cmp al,3 / cmp al,2 / test al,al
    assert!(!AxisMode::NoAxis.draws_minor_grid());
    assert!(!AxisMode::NoAxis.draws_major_grid());
    assert!(!AxisMode::NoAxis.draws_main_axis());

    assert!(!AxisMode::OnlyMain.draws_minor_grid());
    assert!(!AxisMode::OnlyMain.draws_major_grid());
    assert!(AxisMode::OnlyMain.draws_main_axis());

    assert!(!AxisMode::EveryFive.draws_minor_grid());
    assert!(AxisMode::EveryFive.draws_major_grid());
    assert!(AxisMode::EveryFive.draws_main_axis());

    assert!(AxisMode::EveryUnit.draws_minor_grid());
    assert!(AxisMode::EveryUnit.draws_major_grid());
    assert!(AxisMode::EveryUnit.draws_main_axis());

    for m in AxisMode::ALL {
        assert_eq!(AxisMode::from_discriminant(m as u8), Some(m));
    }
    assert_eq!(AxisMode::from_discriminant(4), None);
    // 越界值按原版行为退化成"画轴 + 粗网格"
    assert_eq!(AxisMode::from_raw(9), AxisMode::EveryFive);
}
