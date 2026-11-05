//! `entity_drawer.rs` — 原版棋盘实体绘制（Rust 侧）的**图元级**复原。
//!
//! 复原对象：`EntityDrawer::draw` = **vaddr `0x4b2890`**（Ghidra `0x5b2890`），
//! 函数体 `0x4b2890..=0x4b3fea`（`0x4b3feb` 起是 `int3` 填充；`0x4b3ff0` 是下一个函数）。
//!
//! 这个模块**不调用任何 Godot API**。它把二进制里读出来的
//! 几何 / 颜色 / 线宽 / 折点数 / 层序，表达成一份"要画什么"的清单，
//! 由客户端（GDScript）或 GDExtension 照着画。
//!
//! # 它画什么（证据档位见每一条）
//!
//! `EntityDrawer::draw` 的绘制原语只有两类：
//!
//! | 原语 | 包装函数 | GOT 槽 | 调用点 | 档位 |
//! |---|---|---|---|---|
//! | `draw_arc` ×3 | `0x8abca0` | `0x9bc020` | `0x4b366c` `0x4b36fd` `0x4b378a` | **观测** |
//! | `draw_polygon` ×3（经 `0x4b9a70`） | — | — | `0x4b30b7` `0x4b30e8` `0x4b3119` | **观测** |
//!
//! ⚠️ 定位这些原语**必须**走 GOT 槽 + `.rela.dyn`：包装函数没有 `call rel32` 调用者，
//! 只看直接调用会得出"原版什么都不画"的错误结论。
//!
//! **不含**：`draw_texture_rect`（士兵皮肤/脸/帽子）与 `draw_line`（弹道）。
//! 本函数里这两者的调用点数 = **0**（同一张 GOT 表全量扫过），
//! 所以士兵贴图与弹道由**别的**函数画（`docs/16-client.md` §12 记了已知线索）。

/// 圆周率，仅用于把二进制里的弧度临界值表达成人能读的角度。
pub const PI: f64 = std::f64::consts::PI;

// ---------------------------------------------------------------------------
// 常量表：每一个都指明 `.rodata` 地址与在指令里的用法，方便复核
// ---------------------------------------------------------------------------

/// 每段弧的折点数。`mov $0xa,%esi` @ `0x4b3655` / `0x4b36e6` / `0x4b3773`。**观测**
pub const ARC_POINT_COUNT: u32 = 10;

/// 弧的线宽（f32）。槽 `0x1bb2b4` = `0x3e99999a` = `0.3f32`；
/// `movss 0x1bb2b4(%rip),%xmm5` @ `0x4b3662` / `0x4b36f3` / `0x4b3780`。**逐位（立即数）**
pub const ARC_WIDTH: f32 = 0.3;

/// 抗锯齿开关：`xor %ecx,%ecx` @ `0x4b366a`（每次调用前都清零）⇒ **关**。**观测**
pub const ARC_ANTIALIASED: bool = false;

/// 段数。三段方位角为 `A0 = V/120`、`A0 + 2π/3`、`A0 + 4π/3`。**逐位（立即数）**
pub const SEGMENT_COUNT: usize = 3;

/// 相邻两段的方位角差 `2π/3`。槽 `0x19f9e8` = `2.0943951023931953`。**逐位（立即数）**
pub const SEGMENT_STEP_RAD: f64 = 2.0943951023931953;

/// `A2 = A0 + 4π/3` 直接用的一条独立常量。槽 `0x19fbf8` = `4.1887902047863905`。**逐位（立即数）**
pub const SEGMENT_STEP2_RAD: f64 = 4.1887902047863905;

/// `A0 = V / 120.0`。槽 `0x19f620` = `120.0`，`divsd` @ `0x4b2e9d`。**逐位（立即数）**
pub const ANGLE_DIVISOR: f64 = 120.0;

/// 半张角的下限 / 默认值 `π/6`（30°）。槽 `0x19f2e8` = `0.5235987755982988`。**逐位（立即数）**
pub const HALF_ANGLE_MIN_RAD: f64 = 0.5235987755982988;

/// 半张角的封顶 `1.0733774899765125`（≈61.5°）。槽 `0x19f0a8`，`minsd` @ `0x4b359c`。**逐位（立即数）**
pub const HALF_ANGLE_MAX_RAD: f64 = 1.0733774899765125;

/// 半张角随 `V` 增长的除数 `60.0`。槽 `0x19f918`，`divsd` @ `0x4b358c`。**逐位（立即数）**
pub const HALF_ANGLE_GROWTH_DIVISOR: f64 = 60.0;

/// 绘制半径 = `PosInfo.radius × 1.5`
/// * 弧：`mulss 0x1b9b98(%rip)` @ `0x4b35be`，槽 `0x1b9b98` = `1.5f32`。**逐位（立即数）**
/// * 箭头：`mulsd 0x19f8e0(%rip)` @ `0x4b3083`，槽 `0x19f8e0` = `1.5f64`。**逐位（立即数）**
/// 两者是**同一个 1.5**，所以环心半径与箭头基半径共用一个量。
pub const RADIUS_SCALE: f64 = 1.5;

/// 箭头（`0x4b9a70`）的张角半宽 = `π/30`（6°，即整口 12°）。
/// 槽 `0x19fb68` = `0.20943951023931953`，`addsd` @ `0x4b9ac0`（加）
/// 与 `0x19fc80` = `-0.20943951023931953`，`addsd` @ `0x4b9b74`（减）。**逐位（立即数）**
pub const ARROW_HALF_SPREAD_RAD: f64 = 0.20943951023931953;

/// 箭头外沿比环心半径多出的量 `0.5`。槽 `0x19f6d0`，`addsd` @ `0x4b9b5c`。**逐位（立即数）**
pub const ARROW_RADIUS_DELTA: f64 = 0.5;

// ---------------------------------------------------------------------------
// 基础类型
// ---------------------------------------------------------------------------

/// 二维点。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    /// 极坐标偏移：`self + r·(cos θ, sin θ)`。
    pub fn polar(self, r: f64, theta: f64) -> Self {
        Self {
            x: self.x + r * theta.cos(),
            y: self.y + r * theta.sin(),
        }
    }
}

/// RGBA 颜色，分量已经归一化到 `[0, 1]`（与 Godot `Color` 同域）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// 把打包成 `u32` 的颜色（低字节 = R）展开成 `Color`。
    ///
    /// 依据：`0x4b347a` 起读 `-0x4(%rcx,%r8,8)` 的第 0 字节（`movzbl`）与
    /// 第 1..2 字节（`movzwl`），转 f32 后 `divps {255,255,255,1}`（槽 `0x19c7c0`）
    /// ⇒ 三个通道各除以 255。**观测**（第 4 通道未参与除法 ⇒ 直接取 1.0，**推断**，与真帧不透明一致）
    pub fn from_packed_rgb(packed: u32) -> Self {
        Self {
            r: (packed & 0xff) as f32 / 255.0,
            g: ((packed >> 8) & 0xff) as f32 / 255.0,
            b: ((packed >> 16) & 0xff) as f32 / 255.0,
            a: 1.0,
        }
    }
}

/// 一个待绘制图元。字段即 `CanvasItem` 调用实参，客户端可一对一映射。
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    /// `CanvasItem::draw_arc(center, radius, start_angle, end_angle, point_count, color, width, antialiased)`
    Arc {
        center: Vec2,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        point_count: u32,
        color: Color,
        width: f64,
        antialiased: bool,
    },
    /// `CanvasItem::draw_polygon(points, colors)` —— 实心多边形。
    Polygon {
        points: Vec<Vec2>,
        colors: Vec<Color>,
    },
}

// ---------------------------------------------------------------------------
// 几何
// ---------------------------------------------------------------------------

/// 三段弧的方位角 `A_i`（弧度）。
///
/// `A0 = V / 120`；`A1 = A0 + 2.0943951023931953`；`A2 = A0 + 4.1887902047863905`。
/// 指令：`0x4b2e9d divsd` / `0x4b2eb1 addsd` / `0x4b2ebe addsd`。
/// **逐位（立即数）**；三段间隔正好 `2π/3`（120°）。
pub fn segment_angles(v: f64) -> [f64; SEGMENT_COUNT] {
    let a0 = v / ANGLE_DIVISOR;
    [a0, a0 + SEGMENT_STEP_RAD, a0 + SEGMENT_STEP2_RAD]
}

/// 单段弧的半张角 `H = min(π/6 + (V − base)/60, 1.0733774899765125)`。
///
/// `base` = 该士兵在哈希表里的"起始量"（`map[i]`，读 `-0x8(%r8)`）；
/// 查不到时 `H` 直接取 `π/6`（`0x4b34e6 je 0x4b35b4`）。
/// 指令：`0x4b3586 subsd` / `0x4b358c divsd` / `0x4b3594 addsd` / `0x4b359c minsd`。
/// **逐位（立即数）**。
pub fn half_angle(v: f64, base: Option<f64>) -> f64 {
    match base {
        None => HALF_ANGLE_MIN_RAD,
        Some(b) => {
            let h = HALF_ANGLE_MIN_RAD + (v - b) / HALF_ANGLE_GROWTH_DIVISOR;
            if h < HALF_ANGLE_MAX_RAD {
                h
            } else {
                HALF_ANGLE_MAX_RAD
            }
        }
    }
}

/// 实际送进 `draw_arc` 的半径：`f32(pos_radius) × 1.5f32`，随后按 `f64` 使用。
///
/// 注意是**先转 f32 再乘**（`cvtsd2ss` @ `0x4b35ba`，`mulss` @ `0x4b35be`），
/// 与箭头侧的 `f64×1.5`（`0x4b3083 mulsd`）不是同一条算式；两者各按各自精度实现。
pub fn drawn_radius(pos_radius: f64) -> f64 {
    (pos_radius as f32 * RADIUS_SCALE as f32) as f64
}

/// 箭头（实心多边形）的三个顶点，按原版 **push 顺序**。
///
/// `0x4b9a70` 里逐点 `center + r·(cos θ, sin θ)` 后 `push`：
///
/// 1. `P0 = C + r·(cos θ, sin θ)`                      —— 顶点落在环心半径上
/// 2. `P1 = C + (r+0.5)·(cos(θ−6°), sin(θ−6°))`
/// 3. `P2 = C + (r+0.5)·(cos(θ+6°), sin(θ+6°))`
///
/// 指令：`0x4b9b13`（P0）/ `0x4b9b56`（`r += 0.5`）/ `0x4b9b6e`（`θ − 6°`）/
/// `0x4b9b92`（P1）/ `0x4b9bcf`（P2）。**观测**（指令顺序 + 立即数地址）
///
/// 形态：**向心三角**（尖端在环心半径上，底边在外侧 `r+0.5`、张 12°）。
/// 与真帧实测"外沿等半径扇形、外径/环心 = 1.55"独立吻合（**推断**，见 docs/16-client.md §12）。
pub fn arrow_head(center: Vec2, r: f64, theta: f64) -> [Vec2; 3] {
    let r_outer = r + ARROW_RADIUS_DELTA;
    [
        center.polar(r, theta),
        center.polar(r_outer, theta - ARROW_HALF_SPREAD_RAD),
        center.polar(r_outer, theta + ARROW_HALF_SPREAD_RAD),
    ]
}

/// 一次绘制调用的全部输入。对应 `EntityDrawer::draw` 内层循环取到的一具士兵。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityRingInput {
    /// 棋盘坐标（`PosInfo` 的第 1、2 个 f64；栈槽 `0xb0` / `0xa8`）。
    pub center: Vec2,
    /// `PosInfo` 的第 3 个 f64（栈槽 `0x90` → `0x60`）——**未标定的半径量**。
    pub pos_radius: f64,
    /// 驱动张开/闭合的时间量 `V`（`0x4b2e10` 返回的 f64，栈槽 `0x128`）。
    pub v: f64,
    /// 该士兵在哈希表里的起始量；`None` ⇒ `H` 取 `π/6`。
    pub base: Option<f64>,
    /// 该士兵的颜色（哈希表里取出的打包 RGB）。
    pub color: Color,
}

/// 三段弧的图元清单。
///
/// 每段：`start = A_i − H`、`end = A_i + H`、`point_count = 10`、`width = 0.3`、
/// `antialiased = false`。**逐位（立即数）**
pub fn ring_primitives(input: EntityRingInput) -> Vec<Primitive> {
    let h = half_angle(input.v, input.base);
    let r = drawn_radius(input.pos_radius);
    segment_angles(input.v)
        .iter()
        .map(|&a| Primitive::Arc {
            center: input.center,
            radius: r,
            start_angle: a - h,
            end_angle: a + h,
            point_count: ARC_POINT_COUNT,
            color: input.color,
            width: ARC_WIDTH as f64,
            antialiased: ARC_ANTIALIASED,
        })
        .collect()
}

/// 三个箭头（实心多边形）的图元清单，角度与三段弧一一对应。
///
/// 原版每个箭头 push `colors` 的**同一个颜色 3 份**（`0x4b9c3c` / `0x4b4c5c` / `0x4b4c76`
/// 三次 `movupd (%r14)` 后 push）。**观测**
pub fn arrow_primitives(input: EntityRingInput) -> Vec<Primitive> {
    let r = drawn_radius(input.pos_radius);
    segment_angles(input.v)
        .iter()
        .map(|&a| Primitive::Polygon {
            points: arrow_head(input.center, r, a).to_vec(),
            colors: vec![input.color; 3],
        })
        .collect()
}

/// `EntityDrawer::draw` 对**一具士兵**输出的全部图元，按代码里的先后顺序：
/// 先 3 个箭头（`0x4b30b7`…），再 3 段弧（`0x4b366c`…）。
///
/// ⚠️ 两者各自被不同的条件挡住（箭头侧在 `0x4b307d` 之后、弧侧在
/// `0x4b32e2 test %r14b,%r14b` 与 `0x4b34c3 test %bl,%bl` 之后，
/// 两处的分支目标都是同一个循环尾 `0x4b37f0`），
/// **这两个条件是否互斥 = 未知**（没把两处哈希表语义解出来）。
/// 本函数按"同一具士兵两组都画"给出清单，顺序取代码地址序（**观测**）。
pub fn entity_primitives(input: EntityRingInput) -> Vec<Primitive> {
    let mut out = arrow_primitives(input);
    out.extend(ring_primitives(input));
    out
}

/// 半张角从张开到闭合走过的总角度（弧度）——给客户端做动画插值用。
///
/// `= 1.0733774899765125 − π/6`（≈31.5°）。**逐位**
pub const HALF_ANGLE_SWEEP_RAD: f64 = HALF_ANGLE_MAX_RAD - HALF_ANGLE_MIN_RAD;
