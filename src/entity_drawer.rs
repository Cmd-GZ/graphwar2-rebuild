//! `entity_drawer.rs` — 原版棋盘实体绘制（Rust 侧）的复原。
//!
//! **范围**：本模块的 **10 个有源级名字的函数**全部落地址并解算术（§2 起）；
//! §1 是其中的 `draw` = **vaddr `0x4b2890`**（Ghidra `0x5b2890`），
//! 函数体 `0x4b2890..=0x4b3fea`（`0x4b3feb` 起是 `int3` 填充；`0x4b3ff0` 是 `IControl::init`）。
//!
//! 这个模块**不调用任何 Godot API**。它把二进制里读出来的
//! 几何 / 颜色 / 线宽 / 折点数 / 层序，表达成一份"要画什么"的清单，
//! 以及一份"子节点该摆成什么样"的清单，
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
//! # 士兵/靶子**不在** `draw` 里（本轮补全，见 §2.2 与 `docs/16-client.md` §24）
//!
//! `draw` 里 `draw_texture_rect` 与 `draw_line` 的调用点数都是 **0**
//! （同一张 GOT 表全量扫过）——这是对的，但**不代表**"士兵由别的函数用 `draw_*` 画"。
//! 实际情况是：`EntityDrawer` 有一棵**子节点树**，
//!
//! * 皮肤 / 脸 / 帽子 = 3 个 `Sprite2D`（贴图来自 `GraphwarTextureLoader`），
//! * 名字 = 1 个 `RichTextLabel`（`res://name_label.tscn`），
//!
//! 由 `get_entity_*_sprite` / `get_entity_name_label` **懒建并缓存**，
//! 由 `process` 每帧调 [`sprite_transform`] / [`name_label_layout`] 摆位置。
//! ⇒ **`draw()` 只画准星（三段弧 + 三个箭头）；弹道仍不归本模块**（见 [`UNKNOWNS`] 第 7 条所在节）。

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

// ===========================================================================
// §2  本模块其余 9 个函数：地址表 + 逐个规格
// ===========================================================================
//
// 名字来源：**2.0.60 Android arm64 未 strip 的 `.symtab`**
// （`.rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so`，14,992 KB）。
// 地址来源：Linux x86-64 `2026-08-28/libgraphwarrust.so`（`sha256 9a538115…`）。
// 两边 `src/*.rs` 的 `panic::Location` 三元组 2,035 个、交集 2,035、差 0
// ⇒ 同一份源码，所以**跨架构按语义指纹定位**是合法的，只需逐条给判据。
//
// 判据的写法（本节 9 条全部这样验过，不只靠函数大小——`docs/02` §19 已证否）：
//   * arm64 侧用 `llvm-objdump --arch-name=aarch64` 反汇编（大小取自 `.symtab` 的 `st_size`）；
//   * 把 arm64 的**结构性指纹**（调用了哪些具名函数、`.rodata` 里的浮点常量、
//     方法名字符串的**内容与长度**）搬到 x86 侧找唯一匹配；
//   * 最强的一条是 `process`：它在 arm64 与 x86 两侧**同址性**地各调
//     `get_entity_{skin,face,hat}_sprite` 2 次、`get_entity_name_label` 2 次、
//     `update_sprite` 3 次、`update_name_label` 2 次。多重集逐一相等 ⇒
//     这一次校验同时钉死了 6 个地址，而不是 6 个独立的猜测。

/// `EntityDrawer` 10 个有源级名字的方法：`(源级名, x86 vaddr, arm64 st_size)`。
///
/// 第 2 列是 **x86-64** 的虚地址；Ghidra 里要 `+0x100000`。
/// 第 3 列是 arm64 的字节数，留着是为了让人一眼看出 x86/arm64 的比值都落在
/// `1.02 … 1.28`（不是判据，只是自洽检查）。
pub const ENTITY_DRAWER_FUNCS: [(&str, u32, u32); 10] = [
    // 唯一的 Godot 虚函数 `_draw`：只画环 + 箭头，不画贴图
    ("draw", 0x4b_2890, 5300),
    // `_ready`：连 1 个信号 + 订 6 个后端事件
    ("ready", 0x4b_4130, 2100),
    // `_process`：每帧驱动 4 个子节点的位置/缩放/可见性
    ("process", 0x4b_4b60, 3696),
    // 把一具士兵的贴图节点摆到棋盘坐标对应的像素位置
    ("update_sprite", 0x4b_9530, 632),
    // 把名字标签摆到士兵上方/下方，并做死亡淡出
    ("update_name_label", 0x4b_9810, 496),
    // 画一支向心三角（`draw_polygon`）——`draw` 的 3 个箭头就是它
    ("draw_offset_triangle", 0x4b_9a70, 816),
    ("get_entity_hat_sprite", 0x4b_9e70, 1236),
    ("get_entity_name_label", 0x4b_a4a0, 2832),
    ("get_entity_face_sprite", 0x4b_b0a0, 1236),
    ("get_entity_skin_sprite", 0x4b_b6b0, 1236),
];

/// `EntityDrawer` 的 `_init`（**不在上面 10 个里**，是 gdext 的对象构造器）。
///
/// `0x4b3ff0`（320 B；arm64 `0x4ba8e4`，608 B）——位置在 `draw` 结束与 `ready` 之间，
/// 且体内 `call __tls_get_addr` 取对象 id 计数器、`movups` 从 `.data.rel.ro`
/// （x86 `0x95e2e0` / `0x95e2f0`）搬默认字段值写 `+0x18/+0x28/+0x48/+0x58`，
/// 与 arm64 同构 ⇒ 是 `IControl::init`（构造器），不是业务逻辑。**观测**
pub const ENTITY_DRAWER_INIT_ADDR: u32 = 0x4b_3ff0;

// ---------------------------------------------------------------------------
// §2.1  `ready`：1 个信号 + 6 个后端事件
// ---------------------------------------------------------------------------

/// `ready` 里 `Object::connect` 的信号名（x86 `0x1c2a8e` = arm64 `0x1a138a`，长 7）。
/// `mov w2,#0x7` @ arm64 `0x4babc0` / x86 同位置。**观测（字符串内容 + 长度）**
pub const RESIZED_SIGNAL: &str = "resized";

/// `resized` 的接收方法名（x86 `0x1c2a7a` = arm64 `0x1a1380`，长 10）。
/// `Callable::from_object_method` @ arm64 `0x4bab88`。**观测**
pub const RESIZED_HANDLER: &str = "on_resized";

/// `ready` 里 6 次 `GraphwarBackend::subscribe_to_event` 的事件名，**按调用顺序**。
///
/// x86 `0x1c2a95 / 0x1c2aa3 / 0x1c2ab2 / 0x1c2ac6 / 0x1c2ade / 0x1c2af6`
/// ＝ arm64 `0x1a1391 / 0x1a139f / 0x1a13ae / 0x1a13c2 / 0x1a13da / 0x1a13f2`，
/// 两侧是**同一串连续字面量**（偏移差恒为 0x7a 与 0x7a），长度逐个一致。**观测**
pub const READY_EVENTS: [&str; 6] = [
    "pos_info_event",
    "life_info_event",
    "entity_removed_event",
    "skin_graphics_info_event",
    "face_graphics_info_event",
    "hat_graphics_info_event",
];

/// `ready` 里置位的 `bool` 字段偏移（在"连完信号、订完事件"之后立刻写）。
/// arm64 `0x4babd4`：`strb w8(1),[x20,#0x108]`；x86 `0x4b41be`：
/// `mov BYTE PTR [rbx+0x108],0x1`（两侧同一位置、同一字节值）。字段名 = **推断**（"已就绪"）。
pub const READY_FLAG_OFFSET: usize = 0x108;

// ---------------------------------------------------------------------------
// §2.2  士兵由**子节点**画，不是由 `draw()` 画（本节最重要的一条）
// ---------------------------------------------------------------------------

/// 名字标签的预制体路径（x86 `0x1c2b1d` = arm64 `0x1a14b5`）。
///
/// `get_entity_name_label` 里 `godot_core::tools::save_load::load("res://name_label.tscn")`
/// → `PackedScene::instantiate_as::<RichTextLabel>()`。**观测（字符串 + 两个具名调用）**
pub const NAME_LABEL_SCENE: &str = "res://name_label.tscn";

/// 士兵由哪几个**子节点**组成。每个实体 id 在 4 张 `HashMap<i32, Gd<…>>` 里
/// 各有一份（懒建 + 复用），`process` 每帧取出来摆位置。
///
/// ⇒ **`EntityDrawer::draw` 里没有 `draw_texture_rect` 不是漏，
/// 士兵/靶子根本不是 `draw()` 画的**——是下面这 4 个 `CanvasItem` 子节点。
/// 证据见 [`ENTITY_CHILD_NODES`] 与 `docs/16-client.md` §24。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityChildNode {
    /// `Sprite2D`，贴图 = `GraphwarTextureLoader::get_skin_texture`
    SkinSprite,
    /// `Sprite2D`，贴图 = `GraphwarTextureLoader::get_face_texture`
    FaceSprite,
    /// `Sprite2D`，贴图 = `GraphwarTextureLoader::get_hat_texture`
    HatSprite,
    /// `RichTextLabel`，实例自 `res://name_label.tscn`
    NameLabel,
}

impl EntityChildNode {
    /// 是否为 `Sprite2D`（三个贴图层），`false` ⇒ 名字标签。
    pub const fn is_sprite(self) -> bool {
        !matches!(self, EntityChildNode::NameLabel)
    }

    /// 懒建时取的贴图方法名（`GraphwarTextureLoader` 上的具名方法，
    /// 来自 arm64 `.symtab`：`get_skin_texture` / `get_face_texture` / `get_hat_texture`）。
    /// 名字标签没有贴图 ⇒ `None`。**观测（符号名）**
    pub const fn texture_method(self) -> Option<&'static str> {
        match self {
            EntityChildNode::SkinSprite => Some("get_skin_texture"),
            EntityChildNode::FaceSprite => Some("get_face_texture"),
            EntityChildNode::HatSprite => Some("get_hat_texture"),
            EntityChildNode::NameLabel => None,
        }
    }
}

/// `EntityDrawer` 拥有的 4 类子节点，按 `process` 里**首次取用**的顺序。
/// 顺序即叠放顺序（`Node::add_child` 的先后）：皮肤在最下、帽子在最上。**观测（调用顺序）**
pub const ENTITY_CHILD_NODES: [EntityChildNode; 4] = [
    EntityChildNode::SkinSprite,
    EntityChildNode::FaceSprite,
    EntityChildNode::HatSprite,
    EntityChildNode::NameLabel,
];

/// 三个贴图 `Sprite2D` 建好后统一设的 `CanvasItem.texture_filter` = **4**。
///
/// `mov w1,#0x4` @ arm64 `0x4bfe10` → `CanvasItem::set_texture_filter`。
/// Godot 4 的 `CanvasItem.TEXTURE_FILTER_LINEAR_WITH_MIPMAPS` = 4。**观测（立即数）**
/// （枚举名是查 Godot 文档得来的 = **推断**；数值本身是观测。）
pub const SPRITE_TEXTURE_FILTER: i32 = 4;

/// 三个贴图 `Sprite2D` 建好后统一 `set_flip_v(true)`。
/// `mov w1,#0x1` @ arm64 `0x4bfe30` → `Sprite2D::set_flip_v`。**观测**
pub const SPRITE_FLIP_V: bool = true;

// ---------------------------------------------------------------------------
// §2.3  共用的"镜像"判据：队伍色决定左右翻面
// ---------------------------------------------------------------------------

/// `GraphwarDrawUtil::get_team_overview` 返回色里，`x86 0x4be6e0` / `arm64 0x4c3b38`。
/// 颜色是 packed `u32`（低字节 = R，与 [`Color::from_packed_rgb`] 同一字节序）。
///
/// 判据（`update_sprite` 与 `update_name_label` **各算一遍、算式完全相同**）：
///
/// ```text
/// mirror = ((c & 0x101) == 0x100)                         // 绿置位 且 红不置位
///        | ((c & 0x10000) == 0 && (c >> 24) != 0)         // 蓝不置位 且 alpha != 0
/// ```
///
/// 指令（x86 `0x4b9892..0x4b98c7`，arm64 `0x4bf548..0x4bf57c`）：
/// `test eax,0x10000` / `sete cl` / `cmp eax,0x1000000` / `setae dl` / `and dl,cl` /
/// `and eax,0x101` / `cmp eax,0x100` / `je` ⇒ 负号。**观测（逐条立即数）**
/// 语义（"按队伍色翻面"）= **推断**。
pub fn team_mirror(packed: u32) -> bool {
    let green_bit = packed & 0x101 == 0x100;
    let alpha = if packed & 0x1_0000 != 0 {
        0
    } else {
        packed >> 24
    };
    green_bit || alpha & 1 != 0
}

/// [`team_mirror`] 的符号形式：`-1.0` / `+1.0`（原版就是 `fneg` + `fcsel`/`xorps`）。
pub fn team_mirror_sign(packed: u32) -> f32 {
    if team_mirror(packed) { -1.0 } else { 1.0 }
}

/// 棋盘单位 → 控件像素的比例除数：`k = 控件宽 / 50.0`。
///
/// 槽 x86 `0x1b9fc8` = `0x42480000` = `50.0f32`（arm64 是 `mov w8,#0x42480000` 立即数）。
/// `divss xmm3,[0x1b9fc8]` @ x86 `0x4b9679`（`update_sprite`）
/// 与 `divss xmm0,[0x1b9fc8]` @ x86 `0x4b987d`（`update_name_label`）——**同一个常量两处用**。**观测**
pub const PIXELS_PER_UNIT_DIVISOR: f32 = 50.0;

/// 贴图缩放基准边长 `128.0`。
///
/// 它**不是一条独立常量**：arm64 用 `movi v1.2s,#0x43,lsl #24`（立即数 128.0f32，
/// 不进 `.rodata`），x86 则从槽 `0x19c630` 取一个 4-float 向量、**用它的第 2 个 lane**
/// （`0x19c630` = `0.0`、`0x19c634` = **`128.0`**、`0x19c638` = `0.0`），
/// `divps` @ x86 `0x4b972e`、`shufps …,0x55` @ `0x4b9741` 把 lane1 广播出来。**观测**
pub const SPRITE_BASE_SIZE: f32 = 128.0;

/// 贴图缩放的除数 `60.0`（槽 x86 `0x19d1b0` = `0x42700000`；arm64 `mov w8,#0x42700000`）。
/// 用在 **x 方向**：`scale.x = (sign / 60.0) · (128 / tex_width)`。
/// `divps` @ x86 `0x4b972e`。**观测**
pub const SPRITE_SCALE_DIVISOR: f32 = 60.0;

/// 贴图缩放的除数 `-60.0`（槽 x86 `0x1baacc` = `0xC2700000`；arm64 `mov w8,#-0x3d900000`）。
/// 用在 **y 方向**：`scale.y = (k / -60.0) · (128 / tex_width)`
/// ⇒ y 方向天然是负的，这就是 Godot 里"屏幕 y 向下、棋盘 y 向上"的翻转，
/// 和下面 [`sprite_transform`] 里 `h/2 − k·y` 的减号是同一件事。`divss` @ x86 `0x4b9736`。**观测**
pub const SPRITE_SCALE_DIVISOR_NEG: f32 = -60.0;

/// 一具士兵的三个 `Sprite2D` 该被摆成什么样。
///
/// 字段即 `Node2D` 调用的实参，客户端可一对一映射。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteTransform {
    /// `Node2D::set_position`（x86 GOT `0x9bb230` / arm64 `0x7ca2a0`）
    pub position: Vec2,
    /// `Node2D::set_scale`（x86 GOT `0x9bb238` / arm64 `0x7ca50c`）
    pub scale: Vec2,
    /// `Sprite2D::set_flip_h`（arm64 `0x7bd260`）；`x > 0.0`
    pub flip_h: bool,
}

/// `update_sprite`（x86 `0x4b9530` / arm64 `0x4bf418`，632 B）的全部算术。
///
/// 入参：`self` 的 `Control` 尺寸 `size`（`Control::get_size`，返回两个 `f32`）、
/// 队伍色 `packed`（[`team_mirror`]）、贴图宽度 `tex_width`（`Texture2D::get_width`，
/// `i32` → `f32` 走 `scvtf` @ arm64 `0x4bf5a4`）、棋盘坐标 `(x, y)`。
///
/// 设 `k = size.x / 50`、`s = `[`team_mirror_sign`]：
///
/// ```text
/// position = ( size.x/2 + s·k·x ,  size.y/2 − k·y )
/// scale    = ( (s/60)·(128/tex_w) ,  (k/−60)·(128/tex_w) )
/// flip_h   = x > 0.0
/// ```
///
/// 逐条指令对应见 [`sprite_transform`]。**观测**（除"这套变换的含义"是 **推断**）
pub fn sprite_transform(
    size: Vec2,
    packed: u32,
    tex_width: u32,
    x: f64,
    y: f64,
) -> SpriteTransform {
    let k = size.x as f32 / PIXELS_PER_UNIT_DIVISOR;
    let s = team_mirror_sign(packed);
    let fx = x as f32;
    let fy = y as f32;
    let tex = tex_width as f32;
    let unit = SPRITE_BASE_SIZE / tex;
    SpriteTransform {
        position: Vec2::new(
            (size.x as f32 * 0.5 + s * k * fx) as f64,
            (size.y as f32 * 0.5 - k * fy) as f64,
        ),
        scale: Vec2::new(
            ((s / SPRITE_SCALE_DIVISOR) * unit) as f64,
            ((k / SPRITE_SCALE_DIVISOR_NEG) * unit) as f64,
        ),
        flip_h: x > 0.0,
    }
}

// ---------------------------------------------------------------------------
// §2.4  名字标签的布局
// ---------------------------------------------------------------------------

/// 名字标签一帧的摆法。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NameLabelLayout {
    /// `Control::set_position_full`（arm64 `0x7d7278`）的 `(x, y)`；第三个实参恒 `false`
    /// （`mov w1,wzr` @ arm64 `0x4bf824`）。
    pub position: Vec2,
    /// `CanvasItem::set_modulate`（arm64 `0x7e6ff0`）的 `Color`。
    /// RGB 恒 `(1,1,1)`（`fmov v4.2s,#1.0` + `str w8(1.0)` @ arm64 `0x4bf7d4/0x4bf7e0`；
    /// x86 侧是 `0x19e880` 那组立即数），第 4 通道 = 入参 `alpha`。**观测**
    pub modulate: Color,
}

/// `update_name_label`（x86 `0x4b9810` / arm64 `0x4bf690`，496 B）的全部算术。
///
/// 设 `k = label_size.x / 50`、`s = `[`team_mirror_sign`]、
/// `mw/mh = `Control::get_minimum_size` 的两个分量（`RichTextLabel` 自己算出来的
/// 最小尺寸）、`(x, y)` = 士兵的棋盘坐标、`alpha` = 第 3 个 `f64` 实参。
///
/// ```text
/// cx      = w/2 + s·k·x − mw/2                 // 水平居中到士兵上
/// cx'     = max(cx, 0)                          // fcmp/fcsel mi @0x4bf7b4
/// x_pos   = if mw + cx' > w { w − mw } else { cx' }   // 上界，fcmp @0x4bf7d8
///
/// base    = (h/2 − k·y) + k·(−1.5) − mh/2       // 先往上 −1.5k
/// alt     = base + (k·1.75 + mh/2)              // 越界时翻到下面
/// y_pos   = if base < 0.0 { alt } else { base } // fcmp/fcsel mi @0x4bf7ec
/// ```
///
/// 末尾 `CanvasItem::set_visible(true)`（`mov w1,#0x1` @ arm64 `0x4bf840`）。
/// **观测**（算术逐条对得上；"名字压在士兵上方、越顶就翻到下方"是 **推断**）
pub fn name_label_layout(
    label_size: Vec2,
    min_size: Vec2,
    packed: u32,
    x: f64,
    y: f64,
    alpha: f64,
) -> NameLabelLayout {
    let (w, h) = (label_size.x as f32, label_size.y as f32);
    let (mw, mh) = (min_size.x as f32, min_size.y as f32);
    let k = w / PIXELS_PER_UNIT_DIVISOR;
    let s = team_mirror_sign(packed);
    let (fx, fy) = (x as f32, y as f32);

    let cx = w * 0.5 + s * k * fx - mw * 0.5;
    let cx = if cx < 0.0 { 0.0 } else { cx };
    let x_pos = if mw + cx > w { w - mw } else { cx };

    let base = (h * 0.5 - k * fy) + k * (-1.5) - mh * 0.5;
    let alt = base + (k * 1.75 + mh * 0.5);
    let y_pos = if base < 0.0 { alt } else { base };

    NameLabelLayout {
        position: Vec2::new(x_pos as f64, y_pos as f64),
        modulate: Color::new(1.0, 1.0, 1.0, alpha as f32),
    }
}

// ---------------------------------------------------------------------------
// §2.5  死亡淡出
// ---------------------------------------------------------------------------

/// 死亡淡出持续多少个"刻"：`cmp w8,#0x3c` @ arm64 `0x4bbb38`（`0x3c` = 60）。**观测**
pub const DEATH_FADE_TICKS: u32 = 60;

/// 淡出的除数 `-60.0`：`mov x8,#-0x3fb2000000000000` ⇒ `0xC04E000000000000` = `-60.0f64`
/// @ arm64 `0x4bb850`，`fdiv d0,d0,d12` @ `0x4bbb44`。**逐位**
///
/// 注意它与 [`SPRITE_SCALE_DIVISOR_NEG`] 的 `-60.0f32`（`0xC2700000`）**不是**同一个常量：
/// 一个是 `f64` 位型、一个是 `f32` 位型，只是数值相等。
pub const DEATH_FADE_DIVISOR: f64 = -60.0;

/// 死亡后第 `dead_ticks` 刻的名字标签不透明度。
///
/// `process` 的死亡分支（arm64 `0x4bbb24..0x4bbb54`）：
///
/// ```text
/// t = current_tick.saturating_sub(death_tick)   // subs + csel lo @0x4bbb30/0x4bbb34
/// if t >= 60 { hide label }                     // b.hs @0x4bbb3c
/// alpha = max(1.0 + t/(−60.0), 0.0)             // ucvtf/div/add/fmaxnm @0x4bbb40..0x4bbb4c
/// if alpha <= 0.0 { skip }                      // fcmp/b.le @0x4bbb50
/// ```
///
/// 本函数对 `t >= 60` 直接返回 `0.0`（原版走的是"藏起来"那条分支，**不**调
/// `update_name_label`；两者对外等价，但分支结构不同——写在这里注明差异）。
/// **观测**
pub fn death_alpha(dead_ticks: u32) -> f64 {
    if dead_ticks >= DEATH_FADE_TICKS {
        return 0.0;
    }
    let a = dead_ticks as f64 / DEATH_FADE_DIVISOR + 1.0;
    if a > 0.0 {
        a
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// §2.6  `draw_offset_triangle` = `draw()` 里那 3 个箭头
// ---------------------------------------------------------------------------

/// `draw_offset_triangle`（x86 `0x4b9a70` / arm64 `0x4bf880`，816 B）画的那一个图元。
///
/// 它就是 [`arrow_head`] 的三个顶点 + **同一个颜色 3 份**，最后
/// `Object::call("draw_polygon", [PackedVector2Array, PackedColorArray])`：
///
/// * 方法名 `"draw_polygon"`（12 字节）：x86 `0x1c2bad`，`mov ecx,0xc` @ `0x4b9d41`；
///   arm64 `0x1a14a9`，`mov w2,#0xc` @ `0x4bfa94`。**观测**
/// * 三次 push 颜色都取同一个源：x86 `movupd xmm0,[r14]` @ `0x4b9c1e/0x4b9c3c/0x4b9c5c`；
///   arm64 `ldr q0,[x20]` @ `0x4bf9c8/0x4bf9e0`。**观测**
/// * 角度常量：`+0.20943951023931953` 槽 x86 `0x19fb68`（`addsd` @ `0x4b9ac0`）、
///   `−0.20943951023931953` 槽 x86 `0x19fc80`（`addsd` @ `0x4b9b74`）——两个**独立**槽，
///   不是同一个值的正负号。**逐位**
/// * `r += 0.5`：槽 x86 `0x19f6d0` = `0.5f64`，`addsd` @ `0x4b9b5c`。**逐位**
///
/// ⇒ 既有 [`arrow_head`] / [`arrow_primitives`] 的几何结论**不需要改**，
/// 本节只是把这个"匿名包装函数 `0x4b9a70`"的**真名**补上。**观测**
pub fn offset_triangle(center: Vec2, r: f64, theta: f64, color: Color) -> Primitive {
    Primitive::Polygon {
        points: arrow_head(center, r, theta).to_vec(),
        colors: vec![color; 3],
    }
}

// ---------------------------------------------------------------------------
// §2.7  诚实清单
// ---------------------------------------------------------------------------

/// 本模块**没解出来**的东西，照实登记（`docs/16-client.md` §24 同步）。
///
/// * `process` 里"要不要画名字"的那个 `bool` 取自某个模块对象的 `+0xa1`
///   （`ldrb w9,[x27,#0xa1]` @ arm64 `0x4bb748`），**哪个模块 / 字段真名 = 未知**。
/// * 4 张 `HashMap::retain`（arm64 `0x4bbc54/0x4bbc60/0x4bbc6c/0x4bbc78`）的
///   谓词只看得出是"清掉已消失实体的节点"，**"消失"的判据 = 未知**。
/// * `process` 里有一个 `TimeProbe`（`Instant::now` / `elapsed` / `std::io::_print`）
///   ⇒ 开了帧耗时打印；**打印内容 = 未解**（只有一句 `: dt:` 片段）。
/// * 贴图节点 `+0xc0/+0xa8` 等偏移是 `HashMap` 的字段位置，**字段名 = 推断**。
/// * `ready` 之后 `+0x108` 那个 `bool` 的名字（"已就绪"）是 **推断**。
pub const UNKNOWNS: [&str; 5] = [
    "名字开关 bool 的所属模块与偏移来源（arm64 0x4bb748 的 [+0xa1]）",
    "4 次 HashMap::retain 的\"实体已消失\"判据",
    "TimeProbe 打印的具体格式串",
    "4 个 HashMap 字段名（只有偏移：0xa8/0xc0 等）",
    "ready 之后 +0x108 那个 bool 的真名",
];
