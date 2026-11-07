//! `axis_drawer.rs` — 原版坐标轴 / 网格绘制（`AxisDrawer::draw`）的**图元级**复原。
//!
//! 复原对象：`AxisDrawer::draw` = **x86 vaddr `0x4e05b0`**（体 `0x4e05b0..=0x4e0c1f`，
//! 长 `0x670`；`0x4e0c20` 起是下一个函数）。
//!
//! # 这个函数是怎么钉死的（三条独立证据）
//!
//! | 证据 | 内容 | 档位 |
//! |---|---|---|
//! | arm64 2.0.60 的 `.symtab` | `<graphwarrust::axis_drawer::AxisDrawer as godot_core::gen::classes::control::re_export::IControl>::draw` @ arm64 `0x43b400`，长 **1492** 字节；它只调 `Control::get_size`、`CanvasItem::draw_set_transform_full`、`draw_rect_full`、`draw_line_full` | **观测（符号名）** |
//! | x86 侧结构 | `0x4e05b0` 的调用点集合 + 常量集合与 arm64 逐项对上（下表） | **观测（指令 / `.rodata`）** |
//! | 归属 | `.data.rel.ro` 槽 `0x9bb7d0` 的 `R_X86_64_RELATIVE` addend = `0x4e05b0`，该槽被 `0x426100`（一个 `ImplementsGodotVirtual::__virtual_call::virtual_fn` shim）`call`。同族槽 `0x9bb4c8/0x9bb4d0/0x9bb4d8` 指向 `0x4e01f0/0x4e0070/0x4e0130`，这三个函数都 `lea` 到 `.rodata` 的 `"AxisDrawer"`（槽 `0x1c41b7`，长 10） | **观测** |
//!
//! `axis_drawer` 在原版里**只有 1 个真正的函数**：`draw`。其余是 gdext 样板
//! （`GodotClass::class_id`、`__register_methods`、`__virtual_call::virtual_fn`、
//! `_::__init::__inner_init`）。arm64 `.symtab` 里 `graphwarrust::axis_drawer::`
//! 前缀的 `FUNC` 符号一共就 1 条。
//!
//! # 它画什么
//!
//! 一次 `draw` 只画三样东西，**没有文字、没有贴图、没有圆**：
//!
//! | # | 原语 | 次数 | 何时 | 证据 |
//! |---|---|---|---|---|
//! | 1 | `draw_set_transform(pos = size/2, rot = 0, scale = (W/50, -W/50))` | 1 | 无条件 | `0x4e0667`，槽 `0x9bc018` |
//! | 2 | `draw_rect(rect = (-25,-15,50,30), color = 白, filled = true, width = -1, aa = false)` | 1 | 无条件 | `0x4e06ca`，槽 `0x9bc028` + `.rodata` `0x19cc20`/`0x19ed00` |
//! | 3 | `draw_line(...)` | 2 / 20 / 102 | 见 [`AxisMode`] | `0x4e0775` `0x4e0815` `0x4e089f` `0x4e0944` `0x4e09c6` `0x4e0a74` `0x4e0b02` `0x4e0b62` |
//!
//! ⚠️ 容易搞错的地方：**这里没有 `draw_string`**。整数刻度标签不是这个类画的
//! （整个二进制里 `CanvasItem::draw_string` 的 method-bind 根本不存在——`.rodata`
//! 那张 `CanvasItem` 方法名表只有 16 项，见 `docs/16-client.md` §18）。

// ---------------------------------------------------------------------------
// 地址
// ---------------------------------------------------------------------------

/// `AxisDrawer::draw` 的 x86-64 虚地址（`objdump` 口径）。Ghidra/REA 地址 = `0x5e05b0`。
pub const DRAW_VADDR: u64 = 0x4e05b0;
/// 函数体长度（下一个函数 `0x4e0c20` 起）。
pub const DRAW_LEN: u64 = 0x670;
/// 同一个函数在 arm64 2.0.60 里的地址（`.symtab` 有名字，见模块文档）。
pub const DRAW_ARM64_VADDR: u64 = 0x43b400;
/// arm64 侧的函数长度（`readelf -sW` 的 size 列）。
pub const DRAW_ARM64_LEN: u64 = 1492;

/// 类名字面量（`downcast` 用）：槽 `0x1c41b7`，长 **10**（`0x4e0692` 的 `mov $0xa,%edx`）。
pub const CLASS_NAME: &str = "AxisDrawer";

// ---------------------------------------------------------------------------
// 常量表 —— 每一条都给出 `.rodata` 槽地址（`f32`，小端）
// ---------------------------------------------------------------------------

/// 棋盘 x 下界 `-25.0f32`。槽 `0x1b9670`，`movss` @ `0x4e087a` / `0x4e0999` / `0x4e0a54` / `0x4e0b3a`。
pub const X_MIN: f32 = -25.0;
/// 棋盘 x 上界 `25.0f32`。槽 `0x1b9b8c`，`movss` @ `0x4e09a1` / `0x4e0a5c` / `0x4e0b42`。
pub const X_MAX: f32 = 25.0;
/// 棋盘 y 下界 `-15.0f32`。槽 `0x1bb818`，`movss` @ `0x4e0755` / `0x4e0872` / `0x4e0924` / …
pub const Y_MIN: f32 = -15.0;
/// 棋盘 y 上界 `15.0f32`。槽 `0x1b9268`，`movss` @ `0x4e0760` / `0x4e0882` / `0x4e092f` / …
pub const Y_MAX: f32 = 15.0;

/// 背景矩形 `Rect2(-25, -15, 50, 30)`：16 字节槽 `0x19cc20`
/// （`00 00 c8 c1` / `00 00 70 c1` / `00 00 48 42` / `00 00 f0 41`），`movaps` @ `0x4e069f`。
pub const BOARD_RECT: Rect2 = Rect2 {
    pos: Vec2::new(X_MIN, Y_MIN),
    size: Vec2::new(50.0, 30.0),
};

/// 背景色 `Color(1,1,1,1)`：槽 `0x19ed00`，`lea` @ `0x4e06ab`。**观测**
pub const BOARD_COLOR: Color = Color::new(1.0, 1.0, 1.0, 1.0);

/// 细网格色 `Color(0.9, 0.9, 0.9, 1.0)`：槽 `0x19eba0`（`66 66 66 3f` ×3 + `00 00 80 3f`），
/// `movaps` @ `0x4e0746` / `0x4e07e6`。**观测**
pub const MINOR_COLOR: Color = Color::new(0.9, 0.9, 0.9, 1.0);

/// 粗网格色 `Color(0.7, 0.7, 0.7, 1.0)`：槽 `0x19d5f0`（`33 33 33 3f` ×3 + `00 00 80 3f`），
/// `movaps` @ `0x4e0866` / `0x4e0915` / `0x4e0a45`。**观测**
pub const MAJOR_COLOR: Color = Color::new(0.7, 0.7, 0.7, 1.0);

/// 主轴线色 `Color(0.2, 0.2, 0.2, 1.0)`：槽 `0x19c430`（`cd cc 4c 3e` ×3 + `00 00 80 3f`），
/// `movaps` @ `0x4e0aa6`。**观测**
pub const AXIS_COLOR: Color = Color::new(0.2, 0.2, 0.2, 1.0);

/// 网格线宽 `0.05f32`：槽 `0x1ba860`（`cd cc 4c 3d`），`movss` @ `0x4e076b` / `0x4e080b` / …
pub const GRID_WIDTH: f32 = 0.05;
/// 主轴线宽 `0.075f32`：槽 `0x1ba864`（`9a 99 99 3d`），`movss` @ `0x4e0aef` / `0x4e0b4f`。
pub const AXIS_WIDTH: f32 = 0.075;
/// `draw_rect` 的 `width` 参数 `-1.0f32`：槽 `0x1b9d08`，`movss` @ `0x4e06b7`。
/// （`filled = true` 时引擎忽略它，但原版确实传了 `-1`。）
pub const RECT_WIDTH: f32 = -1.0;

/// 缩放分母 `50.0f32`（= [`X_MAX`] − [`X_MIN`]）：槽 `0x1b9fc8`，`divss` @ `0x4e0639`。
pub const SCALE_DIVISOR: f32 = 50.0;
/// 居中系数 `0.5f32`：槽 `0x1ba128`，`mulss` @ `0x4e0659` / `0x4e065d`。
pub const HALF: f32 = 0.5;
/// 让 y 轴朝上的取反：`movaps` 读 16 字节 `-0.0`（槽 `0x19e880`）再 `xorps` @ `0x4e0641`/`0x4e0648`。
pub const Y_FLIP: f32 = -0.0;
/// 旋转角 `0.0f32`（`xorps %xmm2,%xmm2` @ `0x4e0661`）。
pub const ROTATION: f32 = 0.0;
/// `draw_*` 的 `antialiased` 参数恒为 `false`（每个调用点之前都有 `xor %edx,%edx`）。
pub const ANTIALIASED: bool = false;

/// 细网格间隔（棋盘单位）：x 循环从 `r12d = -25`（`mov $0xffffffe7` @ `0x4e06eb`）到 `+25`。
pub const MINOR_STEP: i32 = 1;
/// 粗网格间隔（棋盘单位）：`lea ebx,[rax+4]` + 每轮 `add eax,5`（`0x4e08c0`、`0x4e08e5`）。
pub const MAJOR_STEP: i32 = 5;

// ---------------------------------------------------------------------------
// 基础类型（只描述"要画什么"，不调引擎）
// ---------------------------------------------------------------------------

/// 二维点（`f32`，与 Godot `Vector2` 同域）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// RGBA（分量已归一化，与 Godot `Color` 同域）。
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
}

/// 轴对齐矩形（与 Godot `Rect2` 同域）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect2 {
    pub pos: Vec2,
    pub size: Vec2,
}

/// 原版 `AxisDrawer::draw` 发出的原语。只有三种，一一对应二进制里的三种调用。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primitive {
    /// `CanvasItem::draw_set_transform(position, rotation, scale)`。
    SetTransform {
        position: Vec2,
        rotation: f32,
        scale: Vec2,
    },
    /// `CanvasItem::draw_rect(rect, color, filled, width, antialiased)`。
    Rect {
        rect: Rect2,
        color: Color,
        filled: bool,
        width: f32,
        antialiased: bool,
    },
    /// `CanvasItem::draw_line(from, to, color, width, antialiased)`。
    Line {
        from: Vec2,
        to: Vec2,
        color: Color,
        width: f32,
        antialiased: bool,
    },
}

/// 坐标轴模式。判别值来自 `0x4e06da` / `0x4e06e3` / `0x4e0832` / `0x4e0836` / `0x4e0a9e`：
///
/// ```text
/// movzx eax, byte [self + 0x18]   ; self+0x18 = axis_mode: u8
/// cmp   al, 3   / jne  …          ; == 3 才画细网格
/// cmp   al, 2   / jb   …          ; >= 2 才画粗网格
/// test  al, al  / je   …          ; != 0 才画两条主轴
/// ```
///
/// 4 个变体名出自 `docs/06-verification.md` §18.2（`EveryUnit`、`EveryFive`、
/// `OnlyMain`、`NoAxis`）。**"判别值 = 变体声明顺序"是推断**：`.rodata` 的 serde
/// 字符串块（`0x1c642b` 起）按 `NoAxis`、`EveryFive`、`EveryUnit` 排列，
/// `OnlyMain` 被常量去重到别处（`0x19f3a0`）；所以 `0=NoAxis … 3=EveryUnit`
/// 是从**代码行为**（0 一条线都不画、3 画最细的网格）反推的，缺的是
/// `AxisMode::deserialize` 的跳表到判别值的直接映射。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AxisMode {
    /// 只画白底，不画任何线。
    NoAxis = 0,
    /// 白底 + 两条主轴（x=0、y=0）。
    OnlyMain = 1,
    /// 再加每 5 单位一条的粗网格。
    EveryFive = 2,
    /// 再加每 1 单位一条的细网格。
    EveryUnit = 3,
}

impl AxisMode {
    /// 四个取值，按判别值升序。
    pub const ALL: [AxisMode; 4] = [
        AxisMode::NoAxis,
        AxisMode::OnlyMain,
        AxisMode::EveryFive,
        AxisMode::EveryUnit,
    ];

    /// 判别值 → 变体。**推断**：越界判别值（u8 只可能是 4..=255）在原版里
    /// 会一路落到"`!= 0` 的轴分支"（因为三个判断是 `==3` / `>=2` / `!=0`），
    /// 也就是**画两条轴但不画网格**。这里用 `None` 表达"不是已知变体"，
    /// 由调用方决定怎么处理；[`axis_primitives`] 按原版行为处理越界值。
    pub fn from_discriminant(v: u8) -> Option<AxisMode> {
        AxisMode::ALL.iter().copied().find(|m| *m as u8 == v)
    }

    /// `al == 3`：画细网格。
    pub fn draws_minor_grid(self) -> bool {
        matches!(self, AxisMode::EveryUnit)
    }

    /// `al >= 2`：画粗网格。
    pub fn draws_major_grid(self) -> bool {
        (self as u8) >= AxisMode::EveryFive as u8
    }

    /// `al != 0`：画两条主轴。
    pub fn draws_main_axis(self) -> bool {
        (self as u8) != 0
    }

    /// 从 u8 还原并按原版行为绘制（越界值 = 画轴、不画网格）。
    pub fn from_raw(v: u8) -> AxisMode {
        match AxisMode::from_discriminant(v) {
            Some(m) => m,
            // 越界：`==3` 不成立、`>=2` 成立、`!=0` 成立 ⇒ 只画粗网格 + 轴。
            None => AxisMode::EveryFive,
        }
    }
}

// ---------------------------------------------------------------------------
// 绘制计划
// ---------------------------------------------------------------------------

/// 一个棋盘单位在画布上的像素数（= `size.x / 50`）。
///
/// 指令：`movss xmm1, [size.x]; divss xmm1, [0x1b9fc8 = 50.0]` @ `0x4e1743`（effect）
/// / `0x4e0639`（axis）。
pub fn unit_px(size: Vec2) -> f32 {
    size.x / SCALE_DIVISOR
}

/// `AxisDrawer::draw` 的完整图元清单。顺序与二进制完全一致。
///
/// 1. `draw_set_transform`（`0x4e0667`）
/// 2. `draw_rect`（`0x4e06ca`）
/// 3. 细网格：竖直 `x = -25..=25`（51 条）+ 水平 `y = -15..=15`（31 条）——仅 `EveryUnit`
/// 4. 粗网格：左边界 `x=-25` 1 条 + 竖直 `x = -20..=25` 10 条 + 下边界 `y=-15` 1 条
///    + 水平 `y = -10..=15` 6 条（= 20 条）——`>= EveryFive`
/// 5. 主轴：`x=0` 竖线 + `y=0` 横线——`!= NoAxis`
pub fn axis_primitives(mode: AxisMode, size: Vec2) -> Vec<Primitive> {
    let unit = unit_px(size);
    let mut out = Vec::new();

    // ① 变换：原点搬到画布中心；scale = (W/50, -W/50) ⇒ 棋盘坐标直接当世界坐标用。
    out.push(Primitive::SetTransform {
        position: Vec2::new(size.x * HALF, size.y * HALF),
        rotation: ROTATION,
        scale: Vec2::new(unit, Y_FLIP - unit),
    });

    // ② 白底。
    out.push(Primitive::Rect {
        rect: BOARD_RECT,
        color: BOARD_COLOR,
        filled: true,
        width: RECT_WIDTH,
        antialiased: ANTIALIASED,
    });

    // ③ 细网格：x 从 -25 到 +25（含），y 从 -15 到 +15（含）。
    if mode.draws_minor_grid() {
        let mut x = X_MIN as i32;
        while x <= X_MAX as i32 {
            out.push(vline(x as f32, MINOR_COLOR, GRID_WIDTH));
            x += MINOR_STEP;
        }
        let mut y = Y_MIN as i32;
        while y <= Y_MAX as i32 {
            out.push(hline(y as f32, MINOR_COLOR, GRID_WIDTH));
            y += MINOR_STEP;
        }
    }

    // ④ 粗网格。原版把**左边界**和**下边界**单独画一次（`0x4e089f`、`0x4e09c6`），
    //    循环起点是 -20 / -10（`mov $0xffffffe8` / `mov $0xfffffff2`，再 `lea ebx,[rax+4]`）。
    if mode.draws_major_grid() {
        out.push(vline(X_MIN, MAJOR_COLOR, GRID_WIDTH)); // 0x4e089f
        let mut x = X_MIN as i32 + MAJOR_STEP;
        while x <= X_MAX as i32 {
            out.push(vline(x as f32, MAJOR_COLOR, GRID_WIDTH));
            x += MAJOR_STEP;
        }
        out.push(hline(Y_MIN, MAJOR_COLOR, GRID_WIDTH)); // 0x4e09c6
        let mut y = Y_MIN as i32 + MAJOR_STEP;
        while y <= Y_MAX as i32 {
            out.push(hline(y as f32, MAJOR_COLOR, GRID_WIDTH));
            y += MAJOR_STEP;
        }
    }

    // ⑤ 两条主轴，线更粗、色更深。
    if mode.draws_main_axis() {
        // 纵轴 x=0：`xmm1=-15, xmm3=+15, xmm0=xmm2=0` @ 0x4e0ada..0x4e0b02
        out.push(Primitive::Line {
            from: Vec2::new(0.0, Y_MIN),
            to: Vec2::new(0.0, Y_MAX),
            color: AXIS_COLOR,
            width: AXIS_WIDTH,
            antialiased: ANTIALIASED,
        });
        // 横轴 y=0：`xmm0=-25, xmm2=+25, xmm1=xmm3=0` @ 0x4e0b3a..0x4e0b62
        out.push(Primitive::Line {
            from: Vec2::new(X_MIN, 0.0),
            to: Vec2::new(X_MAX, 0.0),
            color: AXIS_COLOR,
            width: AXIS_WIDTH,
            antialiased: ANTIALIASED,
        });
    }

    out
}

/// 竖直网格线 `x = x`，`y` 走满 `[Y_MIN, Y_MAX]`（`xmm1 = -15`、`xmm3 = +15`）。
fn vline(x: f32, color: Color, width: f32) -> Primitive {
    Primitive::Line {
        from: Vec2::new(x, Y_MIN),
        to: Vec2::new(x, Y_MAX),
        color,
        width,
        antialiased: ANTIALIASED,
    }
}

/// 水平网格线 `y = y`，`x` 走满 `[X_MIN, X_MAX]`（`xmm0 = -25`、`xmm2 = +25`）。
fn hline(y: f32, color: Color, width: f32) -> Primitive {
    Primitive::Line {
        from: Vec2::new(X_MIN, y),
        to: Vec2::new(X_MAX, y),
        color,
        width,
        antialiased: ANTIALIASED,
    }
}

/// 某个模式下 `draw_line` 的**条数**（不构造图元，便于断言）。
///
/// * `NoAxis` → 0
/// * `OnlyMain` → 2
/// * `EveryFive` → 2 + (1+10) + (1+6) = 20
/// * `EveryUnit` → 20 + 51 + 31 = 102
pub fn line_count(mode: AxisMode) -> usize {
    let minor = if mode.draws_minor_grid() {
        (X_MAX as i32 - X_MIN as i32 + 1) + (Y_MAX as i32 - Y_MIN as i32 + 1)
    } else {
        0
    };
    let major = if mode.draws_major_grid() {
        let v = 1 + (X_MAX as i32 - X_MIN as i32) / MAJOR_STEP; // 左边界 + 循环
        let h = 1 + (Y_MAX as i32 - Y_MIN as i32) / MAJOR_STEP;
        v + h
    } else {
        0
    };
    let axes = if mode.draws_main_axis() { 2 } else { 0 };
    minor as usize + major as usize + axes
}

// 判别值 = 变体声明顺序（见 [`AxisMode`] 的档位说明）。
const _: () = {
    assert!(AxisMode::NoAxis as u8 == 0);
    assert!(AxisMode::OnlyMain as u8 == 1);
    assert!(AxisMode::EveryFive as u8 == 2);
    assert!(AxisMode::EveryUnit as u8 == 3);
};
