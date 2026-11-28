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
//! # 士兵/靶子**不在** `draw` 里（本轮补全，见 §2.2 与 `docs/16-client.md` §21）
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

// ---------------------------------------------------------------------------
// §2.0b  四个取子节点的函数（`get_entity_*`）—— T102 落码
// ---------------------------------------------------------------------------
//
// 原版有 **4 个 lazy get-or-create 函数**，各管一张 `HashMap<i32, Gd<…>>`：
//
// | 函数 | x86 | arm64 | `st_size` | x86 FDE |
// |---|---|---|---|---|
// | `get_entity_skin_sprite` | `0x4bb6b0` | `0x4c1068` | 1236 | `0x60d` |
// | `get_entity_face_sprite` | `0x4bb0a0` | `0x4c0b94` | 1236 | `0x60d` |
// | `get_entity_hat_sprite`  | `0x4b9e70` | `0x4bfbb0` | 1236 | `0x62c` |
// | `get_entity_name_label`  | `0x4ba4a0` | `0x4c0084` | 2832 | `0xbf9` |
//
// **判别性证据（逐位）**：三个 sprite getter **逐指令同形**，只在"读哪张表"这一对上差 0x30：
//
// ```text
// skin  cmp qword ptr [rsi+0x60],0     lea rdi,[r15+0x68]
// face  cmp qword ptr [rsi+0x90],0     lea rdi,[r15+0x98]
// hat   cmp qword ptr [rsi+0xc0],0     lea rdi,[r15+0xc8]
// label             [rsi+0xd8]                          [rsi+0xf0]
// ```
//
// 步长 `0x30` = `HashMap<u32, String>` 的 48 B（`docs/16` §46.3 记过）；基址寄存器是
// `GraphwarGraphicsManager`（`0x46a8e0` = `get_module::<GraphwarGraphicsManager>`），
// `+0x18` 是 `RcBox` 数据起点 ⇒ 指令里的 `+0x60/+0x90/+0xc0/+0xd8` 对应管理器自己的
// `+0x48/+0x78/+0xa8/+0xc0` 四张表。
// 「这三格是**名字**表」= **观测**（`GraphwarGraphicsManager::set_*_graphics` 写 +0x18/+0x48/+0x78，
// `docs/16` §46.3 的链路 + `0x46a8e0` 定名）；哪一格是 skin/face/hat = 本表（`st_size` 与
// `get_skin_texture`/`get_face_texture`/`get_hat_texture` 的调用者集合两端一致）。

/// 三个 sprite getter 各自读的**名字表**位置：`[(函数名, 48 B 块起点, 指令里的 `cmp` 立即数, 层)]`。
///
/// 两个数都给，因为它们是**两个不同的事实**：
/// * **块起点** = `HashMap<u32, String>` 在 `GraphwarGraphicsManager` 里的字段偏移（0x48 / 0x78 / 0xa8）；
/// * **`cmp` 立即数** = 指令里真正出现的那个数（0x60 / 0x90 / 0xc0）= 块起点 **+ 0x18**，
///   即 `HashMap` 内部第 4 个字（`items`/`ctrl` 那一位；`+0x18` 与 [`Self`] 无关，
///   是**该结构自己**的布局）。
/// 0x30 的步长 = 一个 `HashMap<u32, String>` 的大小（48 B）。
/// 档位：块起点 = **观测**（`docs/16` §46.3 的三张 setter 表）；`cmp` 立即数 = **逐位**。
pub const ENTITY_GETTER_NAME_MAP_OFFSETS: [(&str, u32, u32, EntityChildNode); 3] = [
    ("get_entity_skin_sprite", 0x48, 0x60, EntityChildNode::SkinSprite),
    ("get_entity_face_sprite", 0x78, 0x90, EntityChildNode::FaceSprite),
    ("get_entity_hat_sprite", 0xa8, 0xc0, EntityChildNode::HatSprite),
];

/// `get_entity_name_label` 读的第 4 张表：块起点 `0xd8`（= 帽子那张 `0xa8` + 0x30）。
///
/// 指令里的两个立即数是 `[rsi+0xd8]`（块首）与 `[rsi+0xf0]`（= 0xd8 + 0x18，同一位）。
/// 档位：**逐位**（`addr_probe.py dis 0x4ba4a0`）。
pub const ENTITY_GETTER_NAME_LABEL_MAP_BASE: u32 = 0xd8;

/// 见 [`ENTITY_GETTER_NAME_LABEL_MAP_BASE`]：`cmp` 用的那个立即数（= 块起点 + 0x18）。
pub const ENTITY_GETTER_NAME_LABEL_CMP_IMMEDIATE: u32 = 0xf0;

/// 三个 sprite getter 建完节点后依次调的两个引擎方法（thunk 里那两对 `(名,长度)` 字面量）。
///
/// ```text
/// 槽 0x9bc0b8 → thunk 0x8ad2e0 → ('CanvasItem',10) + ('set_texture_filter',18)
/// 槽 0x9bc0c0 → thunk 0x89e550 → ('Sprite2D',8)   + ('set_flip_v',10)
/// 槽 0x9bb260 → thunk 0x8aeff0 → ('Node',4)       + ('add_child',9)
/// ```
///
/// ⇒ `set_texture_filter(4)` → `set_flip_v(true)` → `add_child`，**实参取**
/// [`SPRITE_TEXTURE_FILTER`] / [`SPRITE_FLIP_V`]（arm64 `0x4bfe10` / `0x4bfe30` 的立即数）。
/// 档位：槽→thunk→名字 = **逐位**；"这三个调用属于创建路径" = **观测**（调用序在 body 里）。
pub const ENTITY_GETTER_ENGINE_CALLS: [(&str, u32, &str); 3] = [
    ("set_texture_filter", 0x9bc0b8, "CanvasItem"),
    ("set_flip_v", 0x9bc0c0, "Sprite2D"),
    ("add_child", 0x9bb260, "Node"),
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
/// 证据见 [`ENTITY_CHILD_NODES`] 与 `docs/16-client.md` §21。
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

    /// 这一层在 [`ENTITY_CHILD_NODES`] 里的序号（= `add_child` 顺序 = 叠放顺序）。
    ///
    /// 原版**没有**这个函数：它靠 4 张独立 `HashMap` 各管一层。我方把 4 个句柄合成一条
    /// `Vec`（见 `EntityNodes` 的注释），所以需要一个"层 → 下标"的映射把
    /// `get_entity_{skin,face,hat}_sprite` 接回原版的**函数边界**。档位：**我方化简**
    /// （下标顺序 = [`ENTITY_CHILD_NODES`] 的声明序，而那一条是**观测**）。
    pub const fn sprite_index(self) -> Option<usize> {
        match self {
            EntityChildNode::SkinSprite => Some(0),
            EntityChildNode::FaceSprite => Some(1),
            EntityChildNode::HatSprite => Some(2),
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
/// scale    = ( (s·k/60)·(128/tex_w) ,  (k/−60)·(128/tex_w) )
/// flip_h   = x > 0.0
/// ```
///
/// # 2026-11-26 更正：x 分量原来**漏了 `k`**（真 bug，两个架构都对过）
///
/// 旧版写的是 `scale.x = (s/60)·(128/tex_w)` —— 于是一具士兵的精灵恒为
/// `128 × (1/60) ≈ 2.13 px` 宽（而真机帧里是 **46–54 px 量级**），看起来像"根本没画"。
/// 这个错是**客户端像素对拍**发现的（`docs/06` §55：回放屏 8.07% 差异里最重的一块就是士兵）。
///
/// 指令级复核（x86，`0x4b9530` 的尾部；常量都是我读 `.rodata` 验过的）：
/// ```text
/// 4b9674  movss  xmm3,[rsp]        ; size.x
/// 4b9679  divss  xmm3,[0x1b9fc8]   ; k = size.x / 50.0        （50.0f）
/// 4b9681  movaps xmm4,[0x19e880]   ; {−0.0,−0.0,−0.0,−0.0}
/// 4b9688  xorps  xmm4,xmm3         ; −k
/// 4b968e  test dl,1 / 4b9693 movaps xmm1,xmm3   ; ±k（团队镜像选择）
/// 4b96ab  movaps xmm4,xmm1         ; xmm4 = ±k
/// 4b96ae  movaps [rsp+0x60],xmm4   ; **存 ±k**（后面 scale.x 用）
/// 4b9714  movaps xmm0,[0x19c630]   ; {0.0, 128.0, 0.0, 0.0}
/// 4b9720  movss  xmm0,xmm2         ; xmm0 = {±k, 128.0, …}    ← 低 lane 是 ±k
/// 4b9724  movaps xmm2,[0x19d1b0]   ; {60.0, 0, 0, 0}
/// 4b972b  unpcklps xmm2,xmm1       ; {60.0, tex_w, …}
/// 4b972e  divps  xmm0,xmm2         ; {±k/60, 128/tex_w, …}
/// 4b9731  movaps xmm2,[rsp+0x20]   ; {k,0,0,0}
/// 4b9736  divss  xmm2,[0x1baacc]   ; k / (−60)
/// 4b9741  shufps xmm1,xmm0,0x55    ; xmm1 = 128/tex_w
/// 4b9745  mulss  xmm0,xmm1         ; scale.x = (±k/60)·(128/tex_w)   ← 带 k
/// 4b9749  mulss  xmm1,xmm2         ; scale.y = (128/tex_w)·(k/−60)
/// 4b974d  call   set_scale(Vector2)
/// ```
/// **arm64（`0x4bf418`）结构相同**：`fdiv s12,s11,s0`（k=size.x/50）→ `fneg`
/// → `fcsel s13,s0,s12,ne`（±k）→ 两个 `fmul` 都以 `s1 = 128/tex_w` 收尾
/// （`fmul s0,s0,s1` / `fmul s1,s2,s1`），即两个分量都带 `k`。
/// ⇒ 旧公式是**复原错误**，不是实现选择；已按指令改回。
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
            // ⚠️ **2026-11-26 更正**：x 分量原来漏了 `k`（写成 `s/60`），见本函数文档的
            // "2026-11-26 更正"一段 —— 两个架构的指令都表明 x 分量是 `±k/60`。
            ((s * k / SPRITE_SCALE_DIVISOR) * unit) as f64,
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

/// 本模块**没解出来**的东西，照实登记（`docs/16-client.md` §21 同步）。
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


// ===========================================================================
// 原版方法体：原版 `EntityDrawer` 的 gdext 节点类（`Control` 子类）（T57 从 `src/entity_drawer.rs` 搬回）
// ===========================================================================
#[cfg(feature = "gdext-bindings")]
mod node_class {
//! [T57 已搬回原版模块] 这里是**原版 原版 `EntityDrawer` 的 gdext 节点类（`Control` 子类）** 的方法体。
//!
//! 2026-11-27 从 `src/entity_drawer.rs` 搬进 `src/entity_drawer.rs`：原版把 `#[derive(GodotClass)]`
//! 的 struct 与方法体**写在同一个 `entity_drawer.rs` 里**，证据：
//! `panic::Location` 证据：`ready 0x4b4130` → `src/entity_drawer.rs:532:129`、`process 0x4b4b60` →
// `:587:133`、`draw 0x4b2890` → `:606:133`；`get_entity_{skin,face,hat}_sprite 0x4bb6b0/0x4bb0a0/0x4b9e70`、
// `get_entity_name_label 0x4ba4a0`、`update_sprite 0x4b9530` 的 Location 也全落本文件
// （`fixtures/name_to_addr_v2.tsv` + `tools/addr_probe.py locs`）。
//!
//! **本仓唯一的结构差异**：原版整个 crate 都是 gdext 扩展（不需要 `cfg`）；本仓 `cargo test`
//! （不带 `gdext-bindings`）必须能编，所以这一层挂在
//! `#[cfg(feature = "gdext-bindings")] mod node_class` 下，由 `pub use node_class::*;` 把对外路径
//! 抬平 ⇒ `crate::entity_drawer::EntityDrawer` 这类路径与原版一致。
//!
//! ---
//! 以下是搬运前的原始说明（保留：里面逐条写着与原版的差别与证据档）。
//!
//!
//! # 为什么会有这一份
//!
//! 原版的 `EntityDrawer` 是一个 **`Control` 子类**（`docs/16-client.md` §21）：它自己**不画士兵**，
//! 而是给每个实体建 **4 个子节点**——3 个 `Sprite2D`（皮肤 / 脸 / 帽子）+ 1 个 `RichTextLabel`（名字）——
//! 每帧由 `process` 调 `update_sprite` / `update_name_label` 摆位置。
//!
//! `src/entity_drawer.rs` 已经把那 10 个函数的**算术**复原出来了，但它按约定**不碰引擎**（一行 Godot
//! 调用都没有）。本文件是"让那份算术**真的挂出节点树**"的那层薄壳：
//!
//! ```text
//! src/entity_drawer.rs         纯逻辑：常量表 / 几何 / 布局算式 / 子节点种类（无引擎）
//! src/entity_drawer.rs   ← 本文件：建节点、读引擎尺寸、把算式结果写回节点
//! ```
//!
//! 算术本体**只有一份**，仍然在 `src/entity_drawer.rs` 里；本文件不复制任何算式。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 贴图 | `GraphwarTextureLoader` 的三张 `HashMap<String, Gd<Texture2D>>`（`+0x150/+0x180/+0x1b0`）按**名字**查，名字来自 `EntityDrawer` 自己的 `HashMap<i32, String>`（`+0x30`，见 §51） | **接上了**：`crate::graphwar_texture_loader` 的三张表给名字→`res://skins|faces|hats/*.svg`，再从 `res://assets/<dir>/<name>.ctex` 加载真贴图；取不到才退回**程序生成的纯色 `ImageTexture`**（颜色由我方选定） | 原版的表/名字/查找是**观测**（x86 `0x4bb6b0` 的 `cmpq $0x0,0x168(%r13)` = 皮肤表）；`assets/*.ctex` 是 pck 解出的真资源；纯色是**我方兜底** |
//! | 名字文本 | `GraphwarNameManager` / backtracer | 调用方用 [`EntityDrawer::set_entity_name`] 给 | **未解**（名字从哪来不归本模块） |
//! | 名字标签预制体 | `load("res://name_label.tscn").instantiate_as::<RichTextLabel>()` | **不加载场景**，按 `recovered-src/name_label.tscn` 的 6 个属性手工造（[`new_name_label`]） | 场景属性是**观测**（pck 回收资源）；属性语义是**推断** |
//! | 名字标签的 `theme` | 内联 `Theme` + `StyleBoxFlat`（bg/border 全 alpha 0） | 同值重建 | **观测**（`recovered-src/name_label.tscn`） |
//! | `ready` | 连 `resized` 信号 + 订 **6 个**后端事件 | 只连 `resized`；6 个事件**未接**（`GraphwarBackend` 还没有） | 信号名/方法名是**观测**（原版常量）；事件订阅**未做** |
//! | `process` | 每帧 `update_sprite` ×3、`update_name_label` ×2、死亡淡出、4 次 `HashMap::retain` | 每帧重摆所有实体（`relayout`）；`retain` 的判据仍是 `entity_drawer::UNKNOWNS` 第 2 条 | 布局是**观测**；清理判据**未知** |
//! | `draw` | 三段弧 + 三个箭头（准星）；角度 / 线宽 / 折点 / 抗锯齿全是立即数 | **实现了**：`entity_drawer::entity_primitives` 的 6 条图元 → 3 次 `draw_polygon`（箭头）+ 3 次 `draw_arc`（弧） | 准星几何 **观测**（`docs/archive/140-plane-visual.md` §2 逐条立即数）；坐标单位是**推断**（见下两行） |
//! | 准备圈的**输入** | `draw` 从两张哈希表取中心的 x/y、半径、时钟 `V`、`map[i]`（栈槽 `[rsp+0xb0]/[rsp+0xa8]`、`[rsp+0x90]`，`map[i]` 在 `0x4b34d3`–`0x4b35ae` 那段读） | 调用方用 `set_ring()` / `set_ring_base()` 给 | 原版**观测**（`0x4b2e94`–`0x4b35b4`）；本类**我方接线**（`GraphwarBackend` 的 `pos_info_event` 没接） |
//! | 准备圈的**坐标** | `PosInfo` 的第 1/2/3 个 f64 是否已是画布像素：**未知** | 按 [`ring_center_px`] 那条棋盘 → 像素投影（`k = size.x / 50`，与 sprite 同一条算式），半径乘同一个 `k` | **推断**（缺"PosInfo 的单位"这一条证据） |
//! | 准备圈的**颜色** | 哈希表里的打包 RGB（`0x4b347a`） | 导出属性 `team_color`（`relayout` 也用它） | **我方接线**（队色表 `get_team_overview` 没接，`docs/16-client.md` §49.5 第 7 条） |
//! | 准备圈的**实体顺序 / x,y,radius 来源** | `draw` 在 `0x4b2e32` 调 `GraphwarPlanePosManager::get_all_positions`（**散列序**），循环里 `0x4b314b` 逐 id `get_position` 取三份 f64；门是 `get_alive`(`0x454950`) + `is_on_turn`(`0x462b50`)（任务 41 新证） | 读本类持有的那张表：[`GraphwarPlanePosManager::positions_sorted`]（先 `get_all_positions` 再按 **id 升序**）→ `x/y/radius`；**门**：`gate_attach()` 拿到两份真 manager 后就是 `get_alive != 0` ∧ `is_on_turn`（任务 46），拿不到 / 开关关掉 / 表空 ⇒ 回到 `prep` 那条代偿路径 | 原版读表与两个门**观测**（`docs/16` §46/§48；第二处 `process` 在 `0x4b4f24`）；**排序是我方差异**（原版无排序调用）；门回落与开关是**我方接线** |
//! | 子节点命名 | 3 个 sprite 是引擎自动名 `@Sprite2D@N`；名字标签是场景根名 `RichTextLabel`；本类自己挂在 `GraphwarPlane` 下时也是自动名 `@EntityDrawer@N` | 同：sprite **不命名**、名字标签叫 `RichTextLabel` | **观测**（`docs/16` §2.1.10 的节点树 dump） |
//!
//! ⇒ 这个类解决的是"**能不能真的建出一棵节点树、并把纯逻辑算出来的位置写进去**"，
//! **不是**"`EntityDrawer` 已经复原完了"。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/entity_drawer_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! EntityDrawer : Control
//!   @export team_color : int      # 打包队伍色（低字节 = R，见 entity_drawer::Color::from_packed_rgb）
//!   @export texture_size : int    # **兜底**纯色贴图的边长（像素），默认 8
//!   @export assets_root : String  # 资产根（默认 "res://assets"；空串 ⇒ 不碰磁盘，等价"没有 assets 的机器"）
//!   @export pos_table_enabled : bool  # true（默认）= `_draw` 读位置表；false ⇒ 走 `prep` 代偿路径
//!   @export gate_query_enabled : bool # true（默认）= 门走真 manager 查询（**attach 过才有**）；
//!                                     # false 或没 attach / 拿不到 manager ⇒ 回落 `prep`（任务 46）
//!
//!   spawn_entity(id: int, x: float, y: float) -> int   # 建 4 个子节点并摆位，返回建了几个（恒 4）
//!   remove_entity(id: int) -> bool                     # queue_free 那 4 个子节点并丢掉句柄
//!   entity_count() -> int
//!
//!   # ---- 贴图（`GraphwarTextureLoader` 三张表的入口，§51）----
//!   set_entity_look(id: int, skin: String, face: String, hat: String) -> bool
//!   entity_look(id: int) -> PackedStringArray          # [skin, face, hat]
//!   sprite_texture_path(id: int, layer: int) -> String  # 该层贴图的 resource_path（纯色兜底 = ""）
//!   texture_stats() -> PackedInt64Array                # [加载成功, 退化纯色, 缺失名字数]
//!   missing_assets() -> PackedStringArray              # 缺失的 "skins/<名>" 之类（去重、保序）
//!   assets_available() -> bool                         # 资产根下有 MANIFEST.tsv
//!   relayout() -> void                                 # 手动跑一遍「摆位置」
//!   set_entity_name(id: int, name: String) -> void
//!   set_entity_dead_ticks(id: int, ticks: int) -> float  # 走 entity_drawer::death_alpha，返回 alpha
//!
//!   # ---- 准备圈（`draw`，vaddr 0x4b2890）----
//!   set_ring(id: int, x: float, y: float, pos_radius: float, v: float) -> void
//!   set_ring_base(id: int, base: float) -> bool   # 哈希表命中 ⇒ H 走 min(π/6+(V−base)/60, 61.5°)
//!   clear_ring_base(id: int) -> bool              # 查不到 ⇒ H 恒 π/6（0x4b34e6 je 0x4b35b4）
//!   clear_ring(id: int) -> bool                   # 这具士兵**不在**准备态 ⇒ 它的环一个图元都不发
//!   ring_count() -> int
//!
//!   # ---- 位置表（`GraphwarPlanePosManager`，任务 41：`_draw`/`relayout` 的数据源）----
//!   pos_set_position(id: int, x: float, y: float, radius: float) -> bool   # 写表 + 发 PosInfo(26)
//!   pos_remove(id: int) -> bool                   # 删一格（只认 tag 14，不发事件）
//!   pos_count() -> int
//!   pos_ids_raw() -> PackedInt64Array             # get_all_positions() 的原序（**散列序**，观测用）
//!   pos_ids_sorted() -> PackedInt64Array          # 排序版（id 升序；`_draw` 用的就是这个）
//!   pos_get_position(id: int) -> PackedFloat64Array   # [pos_x, pos_y, radius]
//!   pos_drawn_ids() -> PackedInt64Array           # 最近一次 `_draw` 实际画的 id 顺序
//!   pos_event_count() -> int                      # 表的事件队列长度（set_position 每条 +1）
//!
//!   # ---- 门控（任务 46：`get_alive` 0x454950 + `is_on_turn` 0x462b50 的真查询）----
//!   gate_attach() -> bool          # 从 KernelBus 的容器 get_module 取 Life/Turn 两份 manager
//!   gate_detach() -> bool          # 丢掉缓存（模拟"拿不到 manager"⇒ 回落 prep）
//!   gate_available() -> bool       # 有没有 attach 到两份 manager
//!   gate_enabled() -> bool         # 开关当前值（= 导出属性 gate_query_enabled）
//!   gate_set_enabled(on: bool) -> bool      # 改开关（false ⇒ 一律走 prep 近似）
//!   gate_source() -> String        # 最近一次 `_draw` 实际用的是 "managers" 还是 "prep"
//!   gate_queried() -> int          # 累计走真查询判过的 id 次数（观测点）
//!   gate_alive(id: int) -> int     # LifeManager::get_alive 的原始状态字节（1 活 / 0 死 / 2 查不到）
//!   gate_on_turn(id: int) -> bool  # TurnManager::is_on_turn（0x462b50）
//!   gate_current_players() -> PackedInt64Array    # get_current_turn_players（0x4627c0；按 +0x58 两支）
//!   gate_current_soldiers() -> PackedInt64Array   # get_current_turn_soldiers（0x4628d0；按 +0x58 两支）
//!   gate_set_alive(id: int, alive: bool) -> bool  # 真调 LifeManager::set_alive（0x454b40；验收生产者）
//!   gate_set_turn(player_id: int, soldiers: PackedInt64Array) -> bool  # add_player（验收生产者）
//!   gate_advance_turn(player_id: int) -> int      # next_turn 的轮转那一半：推进到下一个活着的士兵
//!   # ---- 回合模式 / 顺序支（任务 50：TurnData[+0x58]、set_turn 0x4638f0）----
//!   gate_turn_mode() -> int                       # get_turn_mode（0x462c10）：0 顺序 / 1 同时
//!   gate_set_turn_mode(mode: int) -> bool         # set_turn_mode（0x462d20）：写字节 + 发 TurnModeInfo(47)
//!   gate_announce_turn(player_id: int, soldier_id: int) -> bool   # set_turn（0x4638f0）
//!   gate_turn_order() -> PackedInt64Array         # 出场序表（TurnData[+0x8]/[+0x10]）
//!   gate_current_player() -> int                  # 当前玩家游标（TurnData[+0x18]）；-1 = 没有
//!   gate_sequential_state() -> bool               # 顺序支所需状态齐不齐（= 会不会回落）
//!   gate_sequential_fallback() -> bool            # 回落开关当前值（默认 true）
//!   gate_set_sequential_fallback(on: bool) -> bool  # 改回落开关（false = 逐字二进制：空集）
//!
//!   ready_call_count() / process_call_count() / resized_call_count() -> int
//!   draw_call_count() -> int                      # `_draw` 被引擎调到几次
//!   drawn_arc_total() / drawn_polygon_total() -> int
//!   last_arc_count() -> int                       # 最近一次 `_draw` 发了几条弧（一具士兵 = 3）
//!   last_arc(i: int) -> PackedFloat64Array        # 12 个数：cx, cy, r, start, end, 折点数, 线宽, AA, r, g, b, a
//!   last_polygon(i: int) -> PackedVector2Array    # 箭头顶点（画布像素）
//! ```
//!
//! 计数与 `last_*` 是**观测点**：没有它们，headless 里分不清"引擎没回调 `_draw`"和
//! "回调了但一个图元都没发"，也钉不住"发出去的半径/角度/线宽是不是那几条立即数"。
//! `last_arc` 返回的是 **f32 加宽成 f64** 的值——即引擎真正收到的数，不是我们内部的 f64。
//!
//! # 写这种类必须知道的五条（**本轮全部实测**，`docs/16-client.md` §33.4）
//!
//! 1. **`Gd<Node>` 不保活。** `RawGd::drop` 的文档写死了"手动管理的对象什么也不做"
//!    （`godot-core/src/obj/raw_gd.rs`）⇒ 缓存句柄既不会延长寿命、也不会代替 `free`。
//!    节点归**父节点**所有：父节点被回收时子节点由引擎一起回收（验收脚本第 14 组断言）。
//!    所以 [`EntityDrawer::remove_entity`] 必须显式 `queue_free`。
//! 2. **句柄必须先失效再用不。** 外部把某个子节点 `free()` 掉之后再调 `relayout()`，
//!    gdext 的 balanced safeguard 会打出
//!    `Sprite2D::upcast_ref: access to instance with ID … after it has been freed`，
//!    整次 `#[func]` 调用变成 `Invalid call error code 1337`，**进程随后卡死**（60 s 超时被杀）。
//!    ⇒ 句柄与节点必须同生共死；本切片没有加 `is_instance_valid` 防御（原版也不加）。
//! 3. **树外 `add_child` 只记父子关系。** `--script` 模式的 `_initialize()` 跑在根节点进树之前，
//!    那一刻 `is_inside_tree()` 还是 false、子节点的 `_ready` / `_process` 都不会来。
//!    本类**不依赖**这些：`spawn_entity` 自己调一次 `relayout()`，位置立刻就是对的。
//! 4. **`resized` 是 `_ready` 里才连的** ⇒ 树外 `set_size` 不会自动重摆（连都还没连）。
//! 5. **不用 `set_process(true)`**：实测（把它删掉重编）`_process` 照样被调到
//!    —— Godot 4.7 对"实现了该虚方法的扩展类"自动开；原版 `ready` 的证据里也没有这一句。
//!    另外 `self.entities.get_mut(id)` 与 `self.relayout()` **不能写在同一个表达式里**
//!    （`E0499`，两次可变借用），要先结束前者的借用。



use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use godot::classes::image::Format;
use godot::classes::{
    canvas_item::TextureFilter, text_server::AutowrapMode, Control, FileAccess, IControl, Image,
    ImageTexture, Node, ResourceLoader, RichTextLabel, Sprite2D, StyleBoxFlat, Texture2D, Theme,
};
use godot::obj::Singleton;
use godot::prelude::*;

use crate::entity_drawer::{self, EntityChildNode, Vec2 as BoardVec2};
use crate::graphwar_event_queue::GraphwarEventQueue;
use crate::graphwar_life_manager::{
    is_drawable_alive_byte, LifeState, MISSING_STATE,
};
use crate::graphwar_plane_pos_manager::{
    GraphwarPlanePosManager, PlanePos, ENTITY_REMOVED_EVENT_TAG,
};
use crate::graphwar_texture_loader::{GraphwarTextureLoader, TextureKind};
use crate::graphwar_turn_manager::{Team, TurnManager, TurnMode};
use crate::player_showcase::{DEFAULT_FACE, DEFAULT_HAT, DEFAULT_SKIN};

// ---------------------------------------------------------------------------
// 1. 贴图：`GraphwarTextureLoader` 的三张表 + 从 `client/assets/` 加载真贴图
// ---------------------------------------------------------------------------
//
// **原版为什么是"三张 Sprite2D"而不是纯色方块**（证据链，详见 `docs/16-client.md` §51）：
//
// ```text
// GraphwarGraphicsManager::set_{skin,face,hat}_graphics   x86 0x468c70/0x468b60/0x468a50
//   → HashMap<u32 /*实体 id*/, String /*名字*/>          +0x18 / +0x48 / +0x78
//   → 事件 51 / 53 / 55（skin|face|hat_graphics_info_event）
//   → EntityDrawer::ready 订了这三张事件                  x86 0x4b4130
//        ⚠️ 订阅闭包**只置** `self[+0x108] = 1`（x86 0x4b4990），不写任何名字表
//   → EntityDrawer::get_entity_*_sprite **当场**取名字    x86 0x4bb6b0 / 0x4bb0a0 / 0x4b9e70
//        └─ get_module::<GraphwarGraphicsManager>（x86 0x46a8e0）+ 内联 get_*_graphics(实体 id)
//           三张表分别读 ctrl@+0x18/+0x48/+0x78（RcBox 数据从 +0x18 起 ⇒ 指令里是 +0x30/+0x60/+0x90）
//   → GraphwarTextureLoader::get_{skin,face,hat}_texture   arm64 0x40c1cc/0x40c0a4/0x40bf7c
//       表 +0x150 / +0x180 / +0x1b0，键是名字（String）
//   → Sprite2D.set_texture(tex)                          x86 0x4bb6b0 里内联的那段
//
// ⇒ **旧说法"EntityDrawer 有 HashMap<i32,String>（+0x30）"作废**（2026-11-08，任务 42）：
//   名字不落在 EntityDrawer 里，每次取图现查。证据见 `docs/16-client.md` §46.3。
// ```
//
// **名字从哪来**（不是玩家 id、不是队色）：皮肤/脸/帽子三张表都以**字符串**为键
// （`get_*_texture` 里是 hash + `memcmp`，可见 `src/graphwar_texture_loader.rs` §2），
// 初值 = `GraphwarPlayerConfig::new` 的三条字面量 `yellow` / `regular_eyes` / `soldier_helmet`
// （arm64 `0x39152c..0x3915a4`）。真机节点树 dump 也印证了这一点：
// `@Sprite2D@N.texture = res://skins/yellow.svg`、`faces/regular_eyes.svg`、
// `hats/soldier_helmet.svg`（`docs/16-client.md` §2.1.10，**观测**）。
//
// **贴图本体从哪来**：`new` 里 119 条 `res://skins|faces|hats|ranks/*.svg`
// （arm64 `0x40c2f4`，`src/graphwar_texture_loader.rs` §4 全表）。本客户端**不往仓库里放任何图片**
// （`AGENTS.md` 硬规矩 3）：真贴图在 `client/assets/`（使用者自己的 pck 解出来的 `.ctex`），
// 那是 gitignore 的。⇒ 这里按 `res://<dir>/<name>.svg` 换算成 `res://assets/<dir>/<name>.ctex` 去加载，
// **取不到就退回程序生成的纯色**（缺资产 / 名字不在表里 / 没 `load` 权限，全都走这条路）。

/// **兜底**纯色贴图的默认边长（像素）。原版没有这个值——真贴图 512×512（皮肤/脸/帽子），
/// `faces/no_face` 与 `ranks/*` 是 128×128（pck 解出的 `.ctex` 头 `GST2` 里读出来的，**观测**）。
/// 纯色只在"取不到真贴图"时出现，边长是**我方选择**。
pub const DEFAULT_TEXTURE_SIZE: i32 = 8;

/// 资产根目录。与 `client/src/ui/assets.gd` 的 `DEFAULT_DIR` 是**同一个字符串**
/// （那边是仿制品自己的资产层，缺资产时的行为由 `main.gd` 决定；本类不许改 `client/src/`）。
pub const DEFAULT_ASSETS_ROOT: &str = "res://assets";

/// 资产清单文件名。`client/src/ui/assets.gd` 的 `GwAssets::available()` 也以它为准
/// （"解了一半的目录不算就绪"）。
pub const ASSETS_MANIFEST: &str = "MANIFEST.tsv";

/// 依次尝试的容器扩展名。`.ctex` = pck 解出来的 `CompressedTexture2D`（`tools/extract_assets.py`
/// 的产物）；后三个给"手上有完整工程、直接放原件"的机器用（与 `GwAssets::TEXTURE_EXTS` 同序）。
pub const TEXTURE_EXTS: [&str; 4] = ["ctex", "png", "webp", "svg"];

/// 一个实体的外观：三张贴图的**名字**（`GraphwarTextureLoader` 三张表的键）。
///
/// 原版这三条字符串在 `EntityDrawer` 的 `HashMap<i32, String>`（`+0x30`，键 = 实体 id）里，
/// 由 `ready` 订的三个 graphics 事件写入。本切片还没有事件通道 ⇒ 由调用方用
/// [`EntityDrawer::set_entity_look`] 给；默认值 = `GraphwarPlayerConfig` 的三条默认字面量
/// （[`DEFAULT_SKIN`] / [`DEFAULT_FACE`] / [`DEFAULT_HAT`]，`src/player_showcase.rs` 里记了地址）。
#[derive(Clone, Debug, PartialEq, Eq)]
struct EntityLook {
    skin: String,
    face: String,
    hat: String,
}

impl EntityLook {
    /// `GraphwarPlayerConfig::new` 的三条默认外观名（arm64 `0x39152c..0x3915a4`）。**观测**
    fn default_look() -> Self {
        Self {
            skin: DEFAULT_SKIN.to_string(),
            face: DEFAULT_FACE.to_string(),
            hat: DEFAULT_HAT.to_string(),
        }
    }

    /// 这一层该查哪个名字。`None` = 名字标签（没有贴图）。
    fn name_for(&self, kind: EntityChildNode) -> Option<&str> {
        match kind {
            EntityChildNode::SkinSprite => Some(&self.skin),
            EntityChildNode::FaceSprite => Some(&self.face),
            EntityChildNode::HatSprite => Some(&self.hat),
            EntityChildNode::NameLabel => None,
        }
    }
}

/// 子节点种类 → `GraphwarTextureLoader` 的哪张表（`+0x150` / `+0x180` / `+0x1b0`）。
fn texture_kind(kind: EntityChildNode) -> Option<TextureKind> {
    match kind {
        EntityChildNode::SkinSprite => Some(TextureKind::Skin),
        EntityChildNode::FaceSprite => Some(TextureKind::Face),
        EntityChildNode::HatSprite => Some(TextureKind::Hat),
        EntityChildNode::NameLabel => None,
    }
}

/// 原版 loader 表的键 → 本客户端磁盘上的候选路径。
///
/// `res://skins/yellow.svg` → `["res://assets/skins/yellow.ctex", ….png, ….webp, ….svg]`
///
/// 这是**纯字符串换算**（不碰引擎），所以能单测：原版表里那些 `res://<dir>/<name>.svg`
/// 一条都不改，只是末段扩展名按 [`TEXTURE_EXTS`] 逐个试（pck 解出来的是容器格式）。
///
/// ⚠️ 原版那条笔误 `res://hats//no_hat.svg`（双斜杠）**不修**（`src/graphwar_texture_loader.rs`
/// 模块文档 §4）：它进 `has_name` 那条表是第二份 `no_hat`，换算后仍指向同一个文件。
pub fn asset_candidates(assets_root: &str, res_path: &str) -> Vec<String> {
    let rel = res_path.strip_prefix("res://").unwrap_or(res_path);
    // 表里的路径都以 `.svg` 结尾；换成候选扩展名时只取"目录 + 名字"。
    let stem = rel.strip_suffix(".svg").unwrap_or(rel);
    TEXTURE_EXTS
        .iter()
        .map(|ext| format!("{assets_root}/{stem}.{ext}"))
        .collect()
}

/// 默认队伍色：纯红（打包 `u32` 的低字节 = R ⇒ `0x000000ff`）。
///
/// 这个值让 [`entity_drawer::team_mirror`] 返回 `false`（镜像符号 `+1`），
/// 所以验收脚本里所有位置都能手算——**不是**从原版读出来的默认值。
pub const DEFAULT_TEAM_COLOR: i64 = 0x0000_00ff;

/// 三层贴图**退回纯色**时用的颜色（RGBA，`0..=1`）。**我方选择**：真贴图在 `client/assets/`，
/// 这里只保证"取不到真贴图时也别是黑块"。
///
/// 顺序与 [`entity_drawer::ENTITY_CHILD_NODES`] 的前三项（皮肤 / 脸 / 帽子）一致。
const LAYER_COLORS: [(f32, f32, f32, f32); 3] = [
    (0.93, 0.76, 0.62, 1.0), // 皮肤
    (1.00, 1.00, 1.00, 1.0), // 脸
    (0.20, 0.32, 0.62, 1.0), // 帽子
];

/// 把 `(r, g, b, a)` 填成 `edge × edge` 的纯色 `ImageTexture`。
///
/// 返回 `Option`：`Image::create_from_data` 与 `ImageTexture::create_from_image` 在引擎侧都可能
/// 返回空（`Image` 长宽非法等），gdext 把这种情况表达成 `Option`，不 panic。
fn solid_texture(edge: i32, rgba: (f32, f32, f32, f32)) -> Option<Gd<Texture2D>> {
    let edge = edge.max(1);
    let bytes_per_pixel = 4usize;
    let pixel_count = (edge as usize) * (edge as usize);

    // 每个像素 4 字节，RGBA8。`Color` 的分量是 f32 的 0..=1，这里乘 255 取整（+0.5 四舍五入）。
    let channel = |v: f32| -> u8 { (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8 };
    let pixel = [
        channel(rgba.0),
        channel(rgba.1),
        channel(rgba.2),
        channel(rgba.3),
    ];

    let mut data = PackedByteArray::new();
    data.resize(pixel_count * bytes_per_pixel);
    for i in 0..pixel_count {
        for (k, &b) in pixel.iter().enumerate() {
            data[i * bytes_per_pixel + k] = b;
        }
    }

    let image = Image::create_from_data(edge, edge, false, Format::RGBA8, &data)?;
    let texture = ImageTexture::create_from_image(&image)?;
    Some(texture.upcast::<Texture2D>())
}

/// 贴图种类 → 纯色。键是**原版 `GraphwarTextureLoader` 上的方法名**（
/// [`EntityChildNode::texture_method`] 的返回值），不是我们自己编的名字。
fn layer_color(texture_method: &str) -> (f32, f32, f32, f32) {
    // 三个 sprite 已经按 ENTITY_CHILD_NODES 的顺序排好 ⇒ 用序号取色；
    // 万一顺序变了，退化成「第一个颜色」而不是 panic（贴图还在，只是颜色重复）。
    let index = entity_drawer::ENTITY_CHILD_NODES
        .iter()
        .filter(|k| k.is_sprite())
        .position(|k| k.texture_method() == Some(texture_method));
    let index = match index {
        Some(i) if i < LAYER_COLORS.len() => i,
        _ => 0,
    };
    LAYER_COLORS[index]
}

/// 从资产目录里读一张**真贴图**：`res://skins/yellow.svg` → `res://assets/skins/yellow.ctex`。
///
/// 返回 `(实际命中的路径, Gd<Texture2D>)`。下面这些情况都返回 `None`（**都不是错误**，
/// 调用方会退回纯色并记一条"缺什么"）：
///
/// * `assets_root` 空串 ⇒ 这台机器没有 `client/assets/`（`--script` 时常见）；
/// * [`asset_candidates`] 的四个扩展名一个都不在；
/// * `load()` 返回空 / 返回的不是 `Texture2D`（例如同名文件不是纹理）。
///
/// 先 `FileAccess::file_exists` 再 `load`：`load()` 找不到文件时会在引擎里打一条
/// 红字错误，而"没装资产"是**正常状态**（`client/assets/` 是 gitignore 的），不该刷错误。
fn load_asset_texture(assets_root: &str, res_path: &str) -> Option<(String, Gd<Texture2D>)> {
    if assets_root.is_empty() {
        return None;
    }
    for candidate in asset_candidates(assets_root, res_path) {
        let path = GString::from(candidate.as_str());
        if !FileAccess::file_exists(&path) {
            continue;
        }
        let mut loader = ResourceLoader::singleton();
        let Some(resource) = loader.load(&path) else {
            continue;
        };
        if let Ok(texture) = resource.try_cast::<Texture2D>() {
            return Some((candidate, texture));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 2. 名字标签：照 `recovered-src/name_label.tscn` 的属性造（不加载那个场景）
// ---------------------------------------------------------------------------

/// 名字标签的初始尺寸 = `recovered-src/name_label.tscn` 的 `offset_right` / `offset_bottom`。
///
/// 那个文件的根节点没有写 anchor ⇒ 四个 anchor 全是默认 0、四个 offset 里只有 right/bottom
/// 有值 ⇒ 控件左上角在 `(0, 0)`、尺寸 `25 × 40`。**观测（pck 回收资源，`docs/16` §21 提到的
/// `res://name_label.tscn`）**
pub const NAME_LABEL_SIZE: (f32, f32) = (25.0, 40.0);

/// 造一个与 `recovered-src/name_label.tscn` 等价的 `RichTextLabel`。
///
/// 逐项对应（左 = 场景文件里的键，右 = 这里设的属性）：
///
/// | `name_label.tscn` | 这里 |
/// |---|---|
/// | 根节点 `name="RichTextLabel"`、`type="RichTextLabel"` | `RichTextLabel::new_alloc()` + `set_name("RichTextLabel")` |
/// | `offset_right = 25.0` / `offset_bottom = 40.0` | [`NAME_LABEL_SIZE`] |
/// | `fit_content = true` | `set_fit_content(true)`（最小尺寸跟着文本走，名字一长布局就变） |
/// | `scroll_active = false` | `set_scroll_active(false)` |
/// | `autowrap_mode = 0` | `AutowrapMode::OFF` |
/// | 内联 `Theme`：`RichTextLabel/styles/normal` = `StyleBoxFlat`（bg `(0.6,0.6,0.6,0)`、四边 2px、边框 `(0,0,0,0)`、圆角 20） | 同值重建 |
///
/// 边框与底色**全是 alpha 0**，所以看上去没有框——它是原版就这么写的，不是我们省掉的。
/// **观测（pck 回收资源）**；Godot 侧属性的语义是**推断**（照 Godot 4 文档）。
fn new_name_label() -> Gd<RichTextLabel> {
    let mut label = RichTextLabel::new_alloc();
    label.set_name(&StringName::from("RichTextLabel"));
    label.set_fit_content(true);
    label.set_scroll_active(false);
    label.set_autowrap_mode(AutowrapMode::OFF);
    label.set_size(Vector2::new(NAME_LABEL_SIZE.0, NAME_LABEL_SIZE.1));

    // 内联 Theme（只有一条 `RichTextLabel/styles/normal`）。
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(Color::from_rgba(0.6, 0.6, 0.6, 0.0));
    style.set_border_width_all(2);
    style.set_border_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    style.set_corner_radius_all(20);
    let mut theme = Theme::new_gd();
    theme.set_stylebox(
        &StringName::from("normal"),
        &StringName::from("RichTextLabel"),
        &style.upcast::<godot::classes::StyleBox>(),
    );
    label.set_theme(&theme);
    // 描边：**观测**（真帧上名字是"队色笔画 + 黑描边"，`orig` 帧士兵名 `cmdgz` 的笔画色
    // (125,104,0) = 该士兵 `ColorInfo` 的颜色，笔画四周是黑）。RichTextLabel 的描边由
    // `outline_size`（constants）+ `font_outline_color`（colors）两个 theme 项给；
    // **数值是推断**：4 取自同一份 `.tscn` 里 `ViewFunctionsLabel` 用的 `[outline_size=4]`
    // （同一工程的描边惯例），缺的证据是 `Graphwar` 侧给 name_label 设描边的**那条调用**。
    label.add_theme_constant_override(&StringName::from("outline_size"), 4);
    label.add_theme_color_override(
        &StringName::from("font_outline_color"),
        Color::from_rgba(0.0, 0.0, 0.0, 1.0),
    );
    label
}

// ---------------------------------------------------------------------------
// 3. 一个实体的 4 个子节点
// ---------------------------------------------------------------------------

/// 一个实体持有的 4 个子节点 + 它的棋盘状态。
///
/// 对应原版 `EntityDrawer` 的 4 张 `HashMap<i32, Gd<…>>`（字段偏移 `+0xa8/+0xc0/…`，
/// 名字是**推断**）——这里合成**一张** `HashMap`，装 4 个句柄。**这是化简，不是原版形态**
/// （原版 4 张表的插入/删除时机可能不同，我们没证据）。
struct EntityNodes {
    /// 三个贴图层，顺序 = `ENTITY_CHILD_NODES` 里的三个 sprite（皮肤 / 脸 / 帽子）。
    sprites: Vec<Gd<Sprite2D>>,
    /// 名字标签。
    name_label: Gd<RichTextLabel>,
    /// 这个实体的外观（三张贴图的名字）。
    ///
    /// 原版是 `HashMap<i32, String>`（`+0x30`，键 = 实体 id）：`get_entity_skin_sprite`
    /// （x86 `0x4bb6b0`）先按实体 id 从里面取名字，再拿名字去 `GraphwarTextureLoader` 查贴图
    /// ——`cmp -0x20(%r8),%r12d` 比 id、`-0x10/-0x8` 取 `(ptr,len)`，然后 `hash(0x170)`+`0x150` 桶
    /// 就是皮肤表。**观测**
    look: EntityLook,
    /// 棋盘坐标 `(x, y)`。
    board: BoardVec2,
    /// 名字文本（`RichTextLabel::text`）。
    name: GString,
    /// 死亡后过了多少刻；`None` = 还活着。驱动 `entity_drawer::death_alpha`。
    dead_ticks: Option<u32>,
}

// ---------------------------------------------------------------------------
// 3.5  准备圈（`EntityDrawer::draw`，vaddr `0x4b2890`）
// ---------------------------------------------------------------------------
//
// 纯逻辑那一侧**已经复原完了**，一条算术都不在本文件里：
//
//   0x4b2e94..0x4b2ee4  A_i = V/120 + {0, 2π/3, 4π/3}      → entity_drawer::segment_angles
//   0x4b34d3..0x4b35ae  H = min(π/6 + (V−map[i])/60, 61.5°) → entity_drawer::half_angle
//   0x4b35ba/0x4b35be   r = f32(pos_radius) × 1.5f32        → entity_drawer::drawn_radius
//   0x4b9a70            r += 0.5、θ ∓ 6°、3 个顶点           → entity_drawer::arrow_head
//   0x4b366c/…/0x4b378a 3 次 `draw_arc`（10 折点 / 0.3 宽 / 关 AA） → entity_drawer::ring_primitives
//
// 本文件只做**两件**纯接线的事，两件都标了档位：
//
// 1. **坐标**：**不用接线 —— 原版就是棋盘单位**。`EntityDrawer::draw` 在 `0x4b2af8` 调
//    `draw_set_transform(pos = size×0.5, rotation = 0, scale = (mirror ? -k : k, -k))`、
//    `k = size.x/50`（`0x4b2a80 divss [0x1b9fc8]`），**所有** `draw_arc` / `draw_polygon`
//    的实参都在这条变换之下 ⇒ `[rsp+0xb0]/[rsp+0xa8]` 是 `PosInfo` 的 f64 原值（棋盘单位），
//    半径 = `pos_radius × 1.5` 棋盘单位，线宽 `0.3` 也是棋盘单位。见 [`ring_transform`]。
//    （2026-11-26 本轮由指令定死；旧版这里标的是"未知/推断"，做法是逐条预投影成像素。）
// 2. **颜色**：原版从哈希表取打包 RGB；这里用导出属性 `team_color`（**我方接线**）。

/// 一具士兵一次 `_draw` 发出的图元条数：**3 个箭头（`draw_polygon`）+ 3 段弧（`draw_arc`）**。
///
/// 顺序取 [`entity_drawer::entity_primitives`]（= 代码地址序：箭头 `0x4b30b7`/`0x4b30e8`/`0x4b3119`
/// 在前，弧 `0x4b366c`/`0x4b36fd`/`0x4b378a` 在后）。**观测**
pub const RING_PRIMITIVES_PER_ENTITY: usize = 6;

/// 棋盘坐标 → **控件像素**：`(size.x/2 + s·k·x, size.y/2 − k·y)`。
///
/// **`draw()` 现在不走这条了**（原版走 `draw_set_transform`，见 [`ring_transform`]）：
/// 本函数是那条变换的**等价形式**，留给单元测试做对拍。
///
/// 直接把 [`entity_drawer::sprite_transform`] 的 `position` 拿来用 ——
/// 这样"环心落在士兵身上"靠的是**同一条**算式，而不是本文件里第二份投影
/// （`tex_width` 只影响 `scale`，不影响 `position`，所以随便传 1）。
///
/// 档位：投影本身是**观测**（`0x4b9679` 的 `divss [0x1b9fc8]=50.0` 等）；
/// **"准备圈的坐标也走这条投影"是推断**（见上）。
pub fn ring_center_px(size: BoardVec2, packed: u32, board: BoardVec2) -> BoardVec2 {
    entity_drawer::sprite_transform(size, packed, 1, board.x, board.y).position
}

/// 棋盘单位 → 控件像素的比例 `k = size.x / 50`（与 [`ring_center_px`] 里那个 `k` 同一个）。
///
/// **`draw()` 现在不用它了**（改为让引擎的 `draw_set_transform` 乘这个比例，见
/// [`ring_transform`]）；留着是为了给单元测试/别的消费者做"两条路等价"的对拍。
pub fn ring_canvas_scale(size: BoardVec2) -> f64 {
    (size.x as f32 / entity_drawer::PIXELS_PER_UNIT_DIVISOR) as f64
}

/// `EntityDrawer::draw` 开头那条 `draw_set_transform` 的四个实参
/// `(pos.x, pos.y, scale.x, scale.y)`。**`rotation` 恒 `0.0`**（`xorps xmm2,xmm2`）。
///
/// 指令（x86 `0x4b2af8` 的调用点，槽 `0x9bc018` = `CanvasItem::draw_set_transform`）：
///
/// ```text
/// 4b2a3d  call   get_size()             ; 槽 0x9bb000 ⇒ Control::get_size（w/xmm0、h/xmm1）
/// 4b2a43  movss  [rsp+0xb8],xmm0        ; w
/// 4b2a4c  movss  [rsp+0x68],xmm1        ; h
/// 4b2a5a  call   0x4be6e0               ; get_team_overview ⇒ packed 色（镜像位）
/// 4b2a80  divss  xmm1,[0x1b9fc8]        ; k = w / 50.0f           （50.0f 与 update_sprite 同一个槽）
/// 4b2a88  movss  xmm3,[0x1ba128]        ; 0.5f
/// 4b2ac0  movss  xmm0,[rsp+0xb8]        ; pos.x = w * 0.5
/// 4b2ac9  mulss  xmm0,xmm3
/// 4b2acd  movss  xmm1,[rsp+0x68]        ; pos.y = h * 0.5
/// 4b2ad3  mulss  xmm1,xmm3
/// 4b2ad7  movss  [rsp+0x68],xmm1
/// 4b2aea  lea    rdi,[rsp+0x78]
/// 4b2aef  xorps  xmm2,xmm2              ; rotation = 0.0
/// 4b2af2  movss  xmm1,[rsp+0x68]
/// 4b2af8  call   *[0x9bc018]            ; draw_set_transform(pos, rot=0, scale=(xmm3, xmm4))
/// ```
///
/// `scale` 的两个分量：`xmm4 = -k` 恒定；`xmm3` = 镜像判据（`get_team_overview` 的
/// `(c&0x101)==0x100`）选 `-k` 还是 `±k` —— 与 [`entity_drawer::team_mirror`] 同一条判据
/// （`0x4b2a9a`–`0x4b2ae7` 与 `0x4b9892`–`0x4b98c7` 逐条同型）。⇒ **y 永远取负**。
///
/// ⇒ **由此定死一条此前标"未知"的事**：`EntityDrawer::draw` 里 `draw_arc` / `draw_polygon`
/// 收到的坐标**是棋盘单位**（`-25..25` / `PosInfo` 的 f64 原值），不是像素；投影由这条变换做。
/// **观测（逐条指令 + 常量槽）**
pub fn ring_transform(size: Vector2, packed: u32) -> (f32, f32, f32, f32) {
    let k = size.x / entity_drawer::PIXELS_PER_UNIT_DIVISOR as f32;
    let flip = entity_drawer::team_mirror(packed);
    let sx = if flip { -k } else { k };
    (size.x * 0.5, size.y * 0.5, sx, -k)
}

/// 把纯逻辑的 [`entity_drawer::Color`]（RGBA 四个 f32）搬成 Godot 的 `Color`。
fn color_of(c: entity_drawer::Color) -> Color {
    Color::from_rgba(c.r, c.g, c.b, c.a)
}

/// 一具士兵的「准备态」输入 = 原版 `draw` 从两张哈希表里读到的东西。
///
/// 字段与 `docs/140` §2 的栈槽一一对应：`board` = `[rsp+0xb0]/[rsp+0xa8]`、
/// `pos_radius` = `[rsp+0x90] → [rsp+0x60]`、`v` = `0x4b2e10` 的返回值、
/// `base` = `map[i]`（`0x4b3586` 的 `subsd`）。
///
/// **原版是 `HashMap<i32, …>`**；这里用 `BTreeMap`（遍历顺序稳定，断言可复现）。
#[derive(Clone, Copy, Debug)]
struct PrepRing {
    /// 棋盘坐标 `(x, y)`（与 [`EntityDrawer::spawn_entity`] 同口径）。
    board: BoardVec2,
    /// `PosInfo` 的第 3 个 f64 —— **未标定的半径量**（乘 1.5 才是画出来的半径）。
    pos_radius: f64,
    /// 驱动张开/闭合的时间量 `V`（`0x4b2e10 call 0x519530` 的返回值）。
    v: f64,
    /// 哈希表里该士兵的起始量；`None` ⇒ `H` 恒 `π/6`（`0x4b34e6 je 0x4b35b4`）。
    base: Option<f64>,
}

/// 门控用的两份真 manager（任务 46）——原版 `draw` 每帧 `get_module` 取回来的那两个。
///
/// ```text
/// 0x4b3199  call 0x454950   ; GraphwarLifeManager::get_alive(id)   ← 门 1
/// 0x4b319e  mov  r14d, eax  ;        状态字节（1 活 / 0 死 / 2 查不到）
/// 0x4b31ac  call 0x462b50   ; GraphwarTurnManager::is_on_turn(id)  ← 门 2
/// 0x4b31b1  mov  ebx, eax   ;        bool
/// 0x4b32e2  test r14b, r14b ; je 跳过   ⇒ 门 1：**整字节非零**才算过（2 也算过）
/// 0x4b34c3  test bl, bl     ; je 跳过   ⇒ 门 2
/// ```
///
/// **我方接线**：原版在 `draw` 里 `[root+0x1668] → backend+0x238 → get_module`（x86
/// `0x46c2c0` Life / `0x46b740` Turn）；本层没有那条对象图，改由 [`EntityDrawer::gate_attach`]
/// 从 `_scaffold::root_wiring::KernelBus` 的容器里**按类型取回**（仍是同一次 `get_module`），
/// 缓存成本字段。**拿不到 ⇒ `None` ⇒ 回落 `prep`**（`docs/16` §48）。
#[derive(Clone)]
struct GateManagers {
    /// `GraphwarLifeManager`（⇔ [`LifeState`]）。
    life: Rc<RefCell<LifeState>>,
    /// `GraphwarTurnManager`（⇔ [`TurnManager`]）。
    turn: Rc<RefCell<TurnManager>>,
}

impl GateManagers {
    /// 两个门同时为真才画（原版 `0x4b32e2` 与 `0x4b34c3` 两处 `je` 跳过）。
    ///
    /// 门 1 用 [`is_drawable_alive_byte`]（**整字节非零**，不是 `& 1`）；
    /// 门 2 用 [`TurnManager::is_on_turn`]（`0x462b50`）。id 域：位置表的键是 `u32`，
    /// 越界（`i64` 负数 / 超过 `u32`）⇒ 两个门都查不到 ⇒ 不画。
    fn passes(&self, entity_id: i64) -> bool {
        let Ok(id) = u32::try_from(entity_id) else {
            return false;
        };
        if !is_drawable_alive_byte(self.life.borrow().get_alive(id)) {
            return false;
        }
        self.turn.borrow().is_on_turn(entity_id)
    }
}

/// 一次 `draw_arc` 的实参 —— **已经转成 f32**（= 引擎真正收到的数），留给验收脚本对拍。
///
/// 槽位顺序照 `CanvasItem::draw_arc(center, radius, start_angle, end_angle, point_count,
/// color, width, antialiased)`；这正是 x86 包装 `0x8abca0` 的形参表：
/// `xmm0,xmm1 = center` / `xmm2 = radius` / `xmm3 = start` / `xmm4 = end` /
/// `esi = point_count` / `rdx = &Color` / `xmm5 = width` / `ecx = antialiased`。
/// **观测**（`docs/140` §2.2）
#[derive(Clone, Copy, Debug)]
struct ArcRecord {
    center: (f32, f32),
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    point_count: i32,
    width: f32,
    antialiased: bool,
    color: (f32, f32, f32, f32),
}

// ---------------------------------------------------------------------------
// 4. 类本体（注册 + 虚方法 + 属性 + 方法）
// ---------------------------------------------------------------------------

/// `EntityDrawer`：给每个实体建 3 个 `Sprite2D` + 1 个 `RichTextLabel` 的 `Control`。
///
/// 子节点顺序**直接取** [`entity_drawer::ENTITY_CHILD_NODES`]（皮肤 / 脸 / 帽子 / 名字），
/// 即原版 `process` 里首次取用它们的顺序 —— 这个顺序同时是叠放顺序（`add_child` 的先后）。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct EntityDrawer {
    base: Base<Control>,

    /// 打包队伍色（低字节 = R）。默认 [`DEFAULT_TEAM_COLOR`]。
    ///
    /// 原版这个颜色来自 `GraphwarDrawUtil::get_team_overview`（`docs/16` §21 的镜像判据）；
    /// 本切片**由调用方给**——队伍管理还没接进来。
    #[export]
    team_color: i64,

    /// **兜底**纯色贴图的边长（像素）。
    #[export]
    texture_size: i64,

    /// 资产根目录（`res://`）。默认 [`DEFAULT_ASSETS_ROOT`]。
    ///
    /// **这是我方接线**：原版没有这个属性——贴图由 `GraphwarRootNode` 构造的那一个
    /// `GraphwarTextureLoader` 统一加载（`INode::init`，arm64 `0x39a44c`）。本切片还没有那个
    /// 持有者，所以每个实例自己读盘。把它设成空串就等价于"这台机器没有 `client/assets/`"，
    /// 验收脚本用这一手**在任何机器上**都能断言退化分支。
    #[export]
    assets_root: GString,

    /// **位置表开关**（任务 41）：`true` = `_draw` 读 [`Self::pos_manager`] 这张表；
    /// `false` = 回到代偿路径（[`Self::prep`] 自己那套坐标，`set_ring` 填的）。
    ///
    /// **这是我方接线**：原版没有这个开关——`draw` 无条件读位置表（`0x4b2e32`）。
    /// 留在导出属性上是为了"表空了/接线坏了还能退回上一轮的行为"，验收脚本两种都断言。
    #[export]
    pos_table_enabled: bool,

    /// **门控开关**（任务 46）：`true`（默认）= 门走真 manager 查询（`get_alive` + `is_on_turn`），
    /// 但**得先 [`Self::gate_attach`] 拿到两份 manager**；`false` / 没 attach / 拿不到
    /// ⇒ 回落 [`Self::prep`] 成员近似（第九轮 41 号那条代偿路径）。
    ///
    /// **这是我方接线**：原版 `draw` 无条件从容器 `get_module` 取那两个 manager
    /// （`0x46c2c0` / `0x46b740`），没有开关。开关留在这里是因为**播放屏那侧还没有人把
    /// 生命/回合状态喂进容器**（`client/src/**` 是禁区）——一把默认接上会让准备圈少东西
    /// （`AGENTS.md` 合并纪律 3b 记过的同类事故），所以默认路径与上一轮逐字相同。
    #[export]
    gate_query_enabled: bool,

    /// **箭头开关**（默认 `true`）：`_draw` 发不发那 3 个 `draw_polygon`（准备圈的外凸箭头）。
    ///
    /// 原版箭头与三段弧**各有各的门**（同一次 `_draw` 里两段互不相干的控制流）：
    /// * 弧侧：`0x4b32e2 test %r14b,%r14b` → `0x4b32e5 je 0x4b37f0`（假 ⇒ 这具士兵不发弧）；
    /// * 箭头侧：先在 `get_all_positions()` 的 `Vec` 里找到 id（`0x4b2f65 je 0x4b2fa1`），
    ///   再拿 id 去 **`[[rsp+0x120]+0x20]` 那张 `Vec<u32>`** 里顺序查（`0x4b2fc0 cmp [rax+rdx],ebx`
    ///   / `0x4b2fc3 je 0x4b37f0`）——**查不到就整块跳过**（连弧都不发）。
    ///   那张表是什么（回合顺序？准备顺序？）= **未知**。
    /// ⇒ 回放真帧（本样本，冻结回放 `1793897298`）上**只有三段弧、没有箭头**：
    /// 径向直方图在 `r ∈ [15.3, 23.3] px` 之外**一个像素都没有**（箭头外沿会到 `r+0.5` 单位
    /// ≈ 32 px）。⇒ 播放屏把本属性设成 `false`。**观测**（真帧径向分布）；
    /// 机制（那张表的内容）**未知**。
    ///
    /// 这是**我方接线**（原版没有这个开关）：门没解出来之前，只能由调用方按"观测到的形态"选。
    #[export]
    show_ring_arrows: bool,

    /// 实体 id → 4 个子节点。`BTreeMap` 而不是 `HashMap`：遍历顺序稳定，日志/断言可复现。
    entities: BTreeMap<i64, EntityNodes>,

    /// `GraphwarTextureLoader` 的三张表：名字 → `res://skins|faces|hats/*.svg`。
    ///
    /// 纯逻辑（表的内容、getter 的命中/未命中语义）在 `src/graphwar_texture_loader.rs`，
    /// 本文件**不复制**那张 119 条的表，只把 getter 查出来的路径交给引擎去 `load`。
    loader: GraphwarTextureLoader,

    /// 贴图缓存：`(种类, 名字)` → `Gd<Texture2D>`（**每个类实例一份**）。
    ///
    /// 原版是 loader 里的三张 `HashMap<String, Gd<Texture2D>>`（全二进制共享一份，挂在
    /// `GraphwarRootNode +0x16a0` 的 `Rc<RefCell<…>>` 里）。这里退化成每实例一份，键多带一个
    /// 「种类」——因为同一个名字（`no_face` / `bot_0` 这种）在 skins 与 faces 两张表里都有，
    /// 光用名字会串表。**我方化简**（共享那一层要等 `GraphwarRootNode` 接上）。
    textures: BTreeMap<(u8, String), Gd<Texture2D>>,

    /// 从磁盘加载成功的贴图数（观测点：验收脚本拿它区分"真贴图"与"纯色兜底"）。
    loaded_textures: i64,
    /// 退化成纯色的贴图数（观测点）。
    fallback_textures: i64,
    /// 缺哪些资产（`"skins/yellow"` 这种，去重、保序；`push_warning` 也用它）。
    missing_assets: Vec<String>,

    /// 准备态：实体 id → 这一帧的 `PosInfo` + 时钟 `V`。
    ///
    /// `_draw` **只画这张表里的实体**（原版 `draw` 也只遍历自己那两张哈希表）。
    /// 空表 ⇒ 一个绘制图元都不发 —— 这就是"关掉准备态就不画"的那条分支。
    ///
    /// 任务 41 起它**只是门 + 时钟**：实体的 id 顺序与 `x/y/radius` 改由
    /// [`Self::pos_manager`] 的位置表给（表空时仍用这里的坐标，即代偿路径）。
    prep: BTreeMap<i64, PrepRing>,

    /// **位置表**（原版 `GraphwarPlanePosManager`，24 B / `docs/16` §46）。
    ///
    /// 原版 `EntityDrawer::draw`（x86 `0x4b2890`）从根节点取它的路径是：
    ///
    /// ```text
    /// [root+0x1668] → 游戏客户端单元 → [+0x238] 模块容器
    ///   → get_module::<GraphwarPlanePosManager>(0x46ad30) → Rc<RefCell<Manager>>
    ///   → get_all_positions(0x3c66a0) → 逐 id get_position(0x3c62b0)
    /// ```
    ///
    /// **我方接线差异**：本任务不许改 `root_wiring.rs` / `graphwar_root_node.rs`
    /// （容器取回那条路在那边），所以本类**自己持有一个**真类型的
    /// `Rc<RefCell<GraphwarPlanePosManager>>`；表的内容由 [`Self::pos_set_position`] 写入
    /// （等价于原版 `pos_info_event` 那条生产者）。**不是**"从容器里取回来"的那一份。
    pos_manager: Rc<RefCell<GraphwarPlanePosManager>>,

    /// 门控用的两份真 manager（任务 46）。`None` = **拿不到**（没 `gate_attach` / 容器里没有）
    /// ⇒ `_draw` 的门回落 [`Self::prep`] 成员近似。取回路径见 [`Self::gate_attach`]。
    gate_managers: Option<GateManagers>,

    /// 最近一次 `_draw` 的门数据源：`"managers"`（真查询）或 `"prep"`（近似/代偿）。
    gate_source: &'static str,
    /// 累计"用真查询判过的 id"次数（观测点：证明门真的走了 manager，不是 `prep`）。
    gate_queried: i64,

    /// 最近一次 `_draw` **实际画出来的实体 id**，按发出顺序（观测点：排序有没有生效）。
    last_drawn_ids: Vec<i64>,

    /// `_draw` 被引擎调用过几次（观测点）。
    draw_calls: i64,
    /// 累计发出的 `draw_arc` 条数（观测点）。
    arc_total: i64,
    /// 累计发出的 `draw_polygon` 个数（观测点）。
    polygon_total: i64,
    /// 最近一次 `_draw` 发出的弧实参（**引擎真正收到的 f32 值**，单位 = **棋盘单位**
    /// —— 因为原版把投影交给 `draw_set_transform`，见 [`ring_transform`]）。
    last_arcs: Vec<ArcRecord>,
    /// 最近一次 `_draw` 发出的箭头顶点（同样是**棋盘单位**）。
    last_polys: Vec<Vec<Vector2>>,
    /// 最近一次 `_draw` 用的 `draw_set_transform` 实参：`[pos.x, pos.y, scale.x, scale.y]`
    /// （`rotation` 恒 0，所以不记）。**引擎真正收到的那四个 f32**。
    last_transform: [f64; 4],

    /// `_ready` 被调用过几次（观测点）。
    ready_calls: i64,
    /// `_process` 被调用过几次（观测点）。
    process_calls: i64,
    /// `on_resized` 被调用过几次（观测点）。
    resized_calls: i64,

    // ---- 内核事件通道（任务 42，§46）----
    /// 真的订上去几条（原版 `ready` 里那 6 条；`0` = 还没调 `kernel_subscribe`）。
    kernel_subscriptions: i64,
    /// 原版的脏标志 `+0x108`（六条订阅闭包唯一会写的那一格；`process` 末尾清 0）。
    kernel_dirty: bool,
    /// `kernel_pump()` 被调过几次。
    kernel_pumps: i64,
    /// 累计投递给本类的订阅事件条数。
    kernel_events: i64,
    /// 累计"脏了 → 回表里拉"的次数。
    kernel_pulls: i64,
    /// 累计从位置表拉到的条目数。
    kernel_pulled: i64,
    /// 累计因为外观表变了而重贴图的实体数。
    kernel_graphics_applied: i64,
    /// 最近投递的事件判别值（最多留 64 条）。
    kernel_last_tags: Vec<i64>,
}

#[godot_api]
impl IControl for EntityDrawer {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            team_color: DEFAULT_TEAM_COLOR,
            texture_size: DEFAULT_TEXTURE_SIZE as i64,
            assets_root: GString::from(DEFAULT_ASSETS_ROOT),
            pos_table_enabled: true,
            gate_query_enabled: true,
            // 原版箭头与弧各有各的门（见字段文档）；默认发箭头 = 与上一轮行为逐字相同。
            show_ring_arrows: true,
            entities: BTreeMap::new(),
            // 原版这个对象由 `GraphwarRootNode` 构造一次（arm64 `0x39a44c`）；本切片每实例一份。
            loader: GraphwarTextureLoader::new(),
            textures: BTreeMap::new(),
            loaded_textures: 0,
            fallback_textures: 0,
            missing_assets: Vec::new(),
            prep: BTreeMap::new(),
            // 位置表：原版从模块容器取回（`0x4b2b28`），本类自己持一份（见字段文档）。
            // 事件队列是原版 `new` 的第二个实参（`Rc<RefCell<GraphwarEventQueue>>`）。
            pos_manager: GraphwarPlanePosManager::new(Rc::new(RefCell::new(
                GraphwarEventQueue::new(),
            ))),
            // 门控 manager 一开始**拿不到**：原版由节点层 `get_module` 取回（见字段文档），
            // 本层要等 `gate_attach()` 从 KernelBus 的容器里取 ⇒ 默认回落 `prep`。
            gate_managers: None,
            gate_source: "prep",
            gate_queried: 0,
            last_drawn_ids: Vec::new(),
            draw_calls: 0,
            arc_total: 0,
            polygon_total: 0,
            last_arcs: Vec::new(),
            last_polys: Vec::new(),
            last_transform: [0.0; 4],
            ready_calls: 0,
            process_calls: 0,
            resized_calls: 0,
            kernel_subscriptions: 0,
            kernel_dirty: false,
            kernel_pumps: 0,
            kernel_events: 0,
            kernel_pulls: 0,
            kernel_pulled: 0,
            kernel_graphics_applied: 0,
            kernel_last_tags: Vec::new(),
        }
    }

    /// `_ready`：原版在这里连 `resized` 信号（[`entity_drawer::RESIZED_SIGNAL`] →
    /// [`entity_drawer::RESIZED_HANDLER`]）并订 6 个后端事件（[`entity_drawer::READY_EVENTS`]）。
    ///
    /// 本切片**只连信号**：6 个事件要 `GraphwarBackend`，那个模块还没接到节点层。
    ///
    /// 这里**没有** `set_process(true)`，两条理由：
    ///
    /// * 原版 `ready` 的证据里没有这一句（`entity_drawer::READY_EVENTS` 之后写的是 `+0x108` 那个 bool）；
    /// * **实测不需要**——把这一句删掉重编，`process_call_count` 照样是 2（Godot 4.7 对
    ///   "实现了 `_process` 虚方法"的扩展类会自动开；D 档：**观测**）。
    fn ready(&mut self) {
        self.ready_calls += 1;

        // `Callable::from_object_method(self, "on_resized")` 的 gdext 写法：
        // 先拿一个 `Gd<Self>`，再造指向自己方法的 Callable。
        let myself = self.to_gd();
        let callable = myself.callable(entity_drawer::RESIZED_HANDLER);
        let err = self
            .base_mut()
            .connect(entity_drawer::RESIZED_SIGNAL, &callable);
        if err != godot::global::Error::OK {
            // 连不上不是致命错（`resized` 只影响"窗口变大后重摆"，`process` 每帧也会摆）。
            godot_warn!("EntityDrawer::ready 连接 resized 失败: {err:?}");
        }
    }

    /// `_process`：原版每帧调 `update_sprite` ×3 + `update_name_label` ×2 并做死亡淡出
    /// （`docs/16-client.md` §21）。这里收敛成一次 [`Self::relayout`] ——
    /// 摆位置的算术仍然来自 `entity_drawer::sprite_transform` / `name_label_layout`。
    ///
    /// 末尾那句 `queue_redraw()` 是**准备圈**要的：`_draw` 只在引擎认为"这块画布脏了"时才来，
    /// 而准备圈的驱动量 `V` 每帧都在变（原版 `draw` 也是每帧重算角度）。
    /// 子节点（sprite / 标签）不受影响：它们自己有自己的 `NOTIFICATION_DRAW`。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;
        self.relayout();
        self.base_mut().queue_redraw();
    }

    /// `_draw`：原版 `EntityDrawer::draw`（vaddr `0x4b2890`，5984 B）里**准星那一段**。
    ///
    /// 原版对**每一具**准备态士兵发 6 条图元：3 次 `draw_polygon`（`0x4b30b7` / `0x4b30e8` /
    /// `0x4b3119`，各自经 `0x4b9a70`）画外凸箭头、3 次 `draw_arc`（`0x4b366c` / `0x4b36fd` /
    /// `0x4b378a`，经 GOT 槽 `0x9bc020`）画三段弧。本函数逐条照发，顺序也照代码地址序。
    ///
    /// ```text
    /// 图元来源：entity_drawer::entity_primitives(EntityRingInput)
    ///           = arrow_primitives (3 × Polygon)  ++  ring_primitives (3 × Arc)
    /// 实参来源：Arc{radius,start,end,point_count,width,antialiased}  ← 全是立即数（docs/140 §2）
    /// 投影：一条 draw_set_transform（0x4b2af8，槽 0x9bc018）—— 实参保持**棋盘单位**，
    ///       见 [`ring_transform`]；本文件不逐条预投影（2026-11-26 更正，见函数体注释）。
    /// ```
    ///
    /// ⚠️ **要画东西得先有准备态**：`prep` 空表时这里一条图元都不发（原版也只看自己的哈希表）。
    /// 谁来填这张表 = 还没接的 `pos_info_event`（`entity_drawer::READY_EVENTS` 第 1 个）。
    ///
    /// **任务 41**：循环的 id / 顺序 / `x,y,radius` 改由位置表给
    /// （[`Self::rings_for_frame`]），`prep` 退化成"门 + 时钟"。
    fn draw(&mut self) {
        self.draw_calls += 1;

        // ① 先把这一帧要发的图元**全部算成实参**：纯逻辑（`entity_primitives`）。
        //    `self.base_mut()` 会独占借用 `&mut self`，那段里碰不到 `self.xxx`（`E0499`）。
        //
        // **2026-11-26 更正（本轮）**：实参一律保持**棋盘单位**，投影交给 `draw_set_transform`
        // ——这就是原版的结构（见 [`ring_transform`] 的指令证据）。以前这里把每个实参逐条
        // 预投影成画布像素（`ring_center_px` / `radius × k`），漏了两件**只有变换才能正确
        // 表达**的事：① 线宽 `0.3` 是棋盘单位（要变成 `0.3 × k` 像素）；② y 轴在变换里被
        // 取负（镜像/上下翻转），角度也跟着翻 —— 预投影时角度原样送进去 ⇒ 三段弧的**方位
        // 是上下镜像的**。两条都在同一处坏，而当时环是"看不见"的（线宽 0.3 px），所以没暴露。
        let size = self.base().get_size();
        let packed = self.team_color as u32;
        let ring_color = entity_drawer::Color::from_packed_rgb(packed);
        let arrows_on = self.show_ring_arrows;

        // **位置表就接在这里**：原版 `draw` 的循环头（`0x4b3130`）就是从
        // `get_all_positions` 的 `Vec<u32>` 里取 id（见 `rings_for_frame` 的文档）。
        let rings = self.rings_for_frame();

        let mut arcs: Vec<ArcRecord> = Vec::new();
        let mut polys: Vec<(Vec<Vector2>, Color)> = Vec::new();

        for (_id, prep) in &rings {
            let input = entity_drawer::EntityRingInput {
                center: prep.board,
                pos_radius: prep.pos_radius,
                v: prep.v,
                base: prep.base,
                color: ring_color,
            };

            for prim in entity_drawer::entity_primitives(input) {
                match prim {
                    entity_drawer::Primitive::Arc {
                        radius,
                        start_angle,
                        end_angle,
                        point_count,
                        color: arc_color,
                        width,
                        antialiased,
                        ..
                    } => {
                        arcs.push(ArcRecord {
                            // 环心 = `PosInfo` 的 x/y（棋盘单位，`[rsp+0xb0]/[rsp+0xa8]`）。
                            center: (prep.board.x as f32, prep.board.y as f32),
                            // `radius` 已经是 `f32(pos_radius) × 1.5f32`（棋盘单位）。
                            radius: radius as f32,
                            // 角度**不改**：原版直接送 `A_i ∓ H`（`subsd`/`addsd` 之后 cvtsd2ss），
                            // y 翻转由 `draw_set_transform` 的 `-k` 完成。
                            start_angle: start_angle as f32,
                            end_angle: end_angle as f32,
                            point_count: point_count as i32,
                            // 线宽 `0.3` 是**棋盘单位**（槽 `0x1bb2b4`），变换会乘成 `0.3×k` 像素。
                            width: width as f32,
                            antialiased,
                            color: (arc_color.r, arc_color.g, arc_color.b, arc_color.a),
                        });
                    }
                    entity_drawer::Primitive::Polygon { points, colors } => {
                        if !arrows_on {
                            // 原版箭头侧另有一道门（见 `show_ring_arrows` 的文档）；关掉时
                            // 只画三段弧 —— 这就是回放真帧上的形态。
                            continue;
                        }
                        let pts: Vec<Vector2> = points
                            .iter()
                            .map(|p| Vector2::new(p.x as f32, p.y as f32))
                            .collect();
                        let c = colors.first().copied().unwrap_or(ring_color);
                        polys.push((pts, color_of(c)));
                    }
                }
            }
        }

        // ② 逐条发出去。计数在局部变量里累计，`base` 的作用域结束后再写回字段。
        let mut arc_n = 0i64;
        let mut poly_n = 0i64;
        {
            let (tx, ty, sx, sy) = ring_transform(size, packed);
            self.last_transform = [tx as f64, ty as f64, sx as f64, sy as f64];
            let mut base = self.base_mut();
            // 原版 `0x4b2af8` 的 `draw_set_transform`（槽 `0x9bc018`）：
            // `pos = size × 0.5`、`rotation = 0`、`scale = (mirror ? -k : k, -k)`、`k = size.x/50`。
            base.draw_set_transform_ex(Vector2::new(tx, ty))
                .rotation(0.0)
                .scale(Vector2::new(sx, sy))
                .done();
            for a in &arcs {
                // 后三个参数在 Godot 4 有默认值 ⇒ gdext 生成的是 `draw_arc_ex` builder。
                // 这里三个都显式给：`width` 是 0.3、`antialiased` 是 false —— 都是立即数。
                base.draw_arc_ex(
                    Vector2::new(a.center.0, a.center.1),
                    a.radius,
                    a.start_angle,
                    a.end_angle,
                    a.point_count,
                    Color::from_rgba(a.color.0, a.color.1, a.color.2, a.color.3),
                )
                .width(a.width)
                .antialiased(a.antialiased)
                .done();
                arc_n += 1;
            }

            for (pts, c) in &polys {
                // `CanvasItem::draw_polygon` 少于 3 个顶点会打印引擎错误 ⇒ 顶点不足就不调
                // （`arrow_head` 恒给 3 个，这条防御只在几何被改坏时才生效）。
                if pts.len() < 3 {
                    continue;
                }
                let mut pv = PackedVector2Array::new();
                for p in pts {
                    pv.push(*p);
                }
                // 原版每个箭头 push **同一个颜色 3 份**（`0x4b9c3c`/`0x4b4c5c`/`0x4b4c76`），
                // 所以份数 = 顶点数（**观测**）。
                let mut cv = PackedColorArray::new();
                for _ in 0..pts.len() {
                    cv.push(*c);
                }
                base.draw_polygon(&pv, &cv);
                poly_n += 1;
            }
            // 复位（**我方防御**）：原版 `_draw` 里只有这一个变换、发完就结束，没有复位那一句；
            // 这里复位一句，免得本类将来在同一帧里加别的图元时被这条变换带歪。
            base.draw_set_transform_ex(Vector2::new(0.0, 0.0))
                .rotation(0.0)
                .scale(Vector2::new(1.0, 1.0))
                .done();
        }

        self.arc_total += arc_n;
        self.polygon_total += poly_n;
        self.last_arcs = arcs;
        self.last_polys = polys.into_iter().map(|(p, _)| p).collect();
        // 观测点：这一帧**实际画的 id 顺序**（验收脚本拿它断言"排序生效"）。
        self.last_drawn_ids = rings.iter().map(|(id, _)| *id).collect();
    }
}

#[godot_api]
impl EntityDrawer {
    /// 给一个实体建 **4 个子节点**（3 个 `Sprite2D` + 1 个 `RichTextLabel`）并摆好位置。
    ///
    /// 幂等：同一个 `id` 再调一次不会重复建节点（原版也是"懒建 + 复用"，见
    /// `entity_drawer::get_entity_*_sprite`），只会用新的坐标重摆。
    ///
    /// 返回：这个实体现在有**几个子节点**（恒 `4`，即 `ENTITY_CHILD_NODES.len()`）。
    #[func]
    fn spawn_entity(&mut self, id: i64, x: f64, y: f64) -> i64 {
        let board = BoardVec2::new(x, y);
        if let Some(existing) = self.entities.get_mut(&id) {
            existing.board = board;
            self.relayout();
            return 4;
        }

        let mut sprites: Vec<Gd<Sprite2D>> = Vec::new();
        let mut name_label: Option<Gd<RichTextLabel>> = None;
        // 新实体先按**默认外观**建（`GraphwarPlayerConfig` 的三条默认字面量）；
        // 想换成 bot / 靶子的外观就接着调 [`Self::set_entity_look`]（原版是 graphics 事件写进来的）。
        let look = EntityLook::default_look();

        // 顺序**直接取**原版常量：皮肤 → 脸 → 帽子 → 名字（叠放顺序就是 `add_child` 的先后）。
        for kind in entity_drawer::ENTITY_CHILD_NODES {
            if kind.is_sprite() {
                // **不显式命名**：`docs/16` §2.1.10 的节点树 dump 里这三个 sprite 就叫
                // `@Sprite2D@N`（Godot 的自动名）⇒ 原版也没给它们起名字。**观测**
                let mut sprite = Sprite2D::new_alloc();

                // 贴图：名字 → `GraphwarTextureLoader` 的表 → `res://…svg` → `client/assets/*.ctex`。
                // 取不到就给纯色兜底（"没有 assets 的机器"这条路也要能跑）。
                if let Some(name) = look.name_for(kind) {
                    if let Some(texture) = self.texture_for(kind, name) {
                        sprite.set_texture(&texture);
                    }
                }
                // 两个**原版常量**（`entity_drawer::SPRITE_TEXTURE_FILTER` / `SPRITE_FLIP_V`）：
                // 三个 sprite 建好后统一 `texture_filter = 4`、`flip_v = true`。
                sprite.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
                sprite.set_flip_v(entity_drawer::SPRITE_FLIP_V);

                // `add_child` 必须在**持有 `&mut self` 的时候**做：节点树是引擎状态，
                // 而 `self.base_mut()` 只是把 `Base` 借出去，不会重入我们自己的方法。
                let node = sprite.clone().upcast::<Node>();
                self.base_mut().add_child(&node);
                sprites.push(sprite);
            } else {
                // 原版是 `load("res://name_label.tscn").instantiate_as::<RichTextLabel>()`；
                // 那个场景不在 `client/` 里，所以按它回收出来的属性**直接造一个**
                // （逐项对应见 [`new_name_label`] 的表格）。
                let mut label = new_name_label();
                label.set_text(&GString::from(""));
                let node = label.clone().upcast::<Node>();
                self.base_mut().add_child(&node);
                name_label = Some(label);
            }
        }

        // `name_label` 一定是 `Some`：`ENTITY_CHILD_NODES` 里有且只有一个非 sprite。
        let Some(name_label) = name_label else {
            godot_error!("EntityDrawer::spawn_entity 没建出名字标签（ENTITY_CHILD_NODES 被改了？）");
            return 0;
        };

        self.entities.insert(
            id,
            EntityNodes {
                sprites,
                name_label,
                look,
                board,
                name: GString::from(""),
                dead_ticks: None,
            },
        );
        self.relayout();
        self.entities
            .get(&id)
            .map(|e| (e.sprites.len() + 1) as i64)
            .unwrap_or(0)
    }

    /// 移除一个实体：4 个子节点 `queue_free`，并把缓存句柄丢掉。
    ///
    /// 用 `queue_free`（延迟到本帧末）而不是 `free`：`free` 在信号/物理回调里是危险的，
    /// 而 `remove_entity` 完全可能被这类回调调到。⇒ 调用方**要等一帧**才看得到子节点消失。
    ///
    /// 顺手把它的准备态也删掉：实体没了还在画环没有意义（原版 `entity_removed_event`
    /// 那条路径也会把哈希表里那一项去掉；**具体时机未知**，这里取"同一刻"）。
    #[func]
    fn remove_entity(&mut self, id: i64) -> bool {
        let Some(entity) = self.entities.remove(&id) else {
            return false;
        };
        for sprite in &entity.sprites {
            sprite.clone().upcast::<Node>().queue_free();
        }
        entity.name_label.clone().upcast::<Node>().queue_free();
        if self.prep.remove(&id).is_some() {
            self.base_mut().queue_redraw();
        }
        true
    }

    /// 当前持有几个实体。
    #[func]
    fn entity_count(&self) -> i64 {
        self.entities.len() as i64
    }

    /// 手动跑一遍「把每个实体的 4 个子节点摆到该在的地方」。
    ///
    /// 每帧 `process` 已经会跑一次；这个方法是给两种场景用的：
    /// 节点还没进树（`process` 不会来）、以及验收脚本要在"刚 spawn 完"的同一帧里断言。
    #[func]
    fn relayout(&mut self) {
        // 先读尺寸再用：`self.base()` 与 `self.entities` 是**不同字段**，可以同时借。
        let size = self.base().get_size();
        let packed = self.team_color as u32;
        let control_size = BoardVec2::new(size.x as f64, size.y as f64);

        // **同一条位置表的第二个消费方**（任务 41）：原版 `EntityDrawer::process`
        // （x86 `0x4b4b60`）在 `0x4b4f24` 调 `get_all_positions`、`0x4b5004` 调
        // `get_position`，拿到的 x/y 就是 `update_sprite` 用的那两个值（sprite 与名字标签
        // 都按它摆）。⇒ 表里有这个 id 就以表为准；表空/开关关掉 ⇒ 用实体自己的坐标。
        let table = if self.pos_table_enabled {
            self.pos_manager.borrow().positions_sorted()
        } else {
            Vec::new()
        };

        // **子节点只从原版那 4 个函数取**（T102）：`get_entity_{skin,face,hat}_sprite` +
        // `get_entity_name_label`。原版 `process` 也是这么拿的（`docs/16` §21.5 的调用序）。
        // 先收 id 列表再做：那 4 个 getter 要 `&mut self`，而下面还要 `&self.entities` 取
        // `board` / `dead_ticks` —— 顺序固定成「取句柄 → 再读状态」，就没有两份可变借用。
        let ids: Vec<i64> = self.entities.keys().copied().collect();
        for id in ids {
            let mut sprites = [
                self.get_entity_skin_sprite(id),
                self.get_entity_face_sprite(id),
                self.get_entity_hat_sprite(id),
            ];
            let Some(mut name_label) = self.get_entity_name_label(id) else {
                continue;
            };
            let Some(entity) = self.entities.get(&id) else {
                continue;
            };
            // 表命中 ⇒ 棋盘坐标换成表里的 x/y（`[*]` 是 `PlanePos = [f64;3]`）；
            // 不命中 ⇒ 仍用 `spawn_entity` / `set_ring` 给的那一份（代偿路径）。
            // 只影响本帧的摆位，**不写回** `entity.board`（表清空后不会留残值）。
            let board = match table.iter().find(|(table_id, _)| *table_id as i64 == id) {
                Some((_, pos)) => BoardVec2::new(pos[0], pos[1]),
                None => entity.board,
            };
            let dead_ticks = entity.dead_ticks;
            // 三个贴图层：位置 / 缩放 / 水平翻面全部来自 `entity_drawer::sprite_transform`。
            for sprite in sprites.iter_mut().flatten() {
                // 原版每一层都读自己贴图的宽度（`Texture2D::get_width`）。
                let tex_width = sprite
                    .get_texture()
                    .map(|t| t.get_width().max(1) as u32)
                    .unwrap_or(1);
                let transform = entity_drawer::sprite_transform(
                    control_size,
                    packed,
                    tex_width,
                    board.x,
                    board.y,
                );
                sprite.set_position(Vector2::new(
                    transform.position.x as f32,
                    transform.position.y as f32,
                ));
                sprite.set_scale(Vector2::new(transform.scale.x as f32, transform.scale.y as f32));
                sprite.set_flip_h(transform.flip_h);
            }

            // 名字标签：位置 / 不透明度来自 `entity_drawer::name_label_layout`，
            // 死亡淡出的 alpha 来自 `entity_drawer::death_alpha`。
            let alpha = match dead_ticks {
                None => 1.0,
                Some(ticks) => entity_drawer::death_alpha(ticks),
            };
            //
            // ⚠️ **第 1 个实参是"控件尺寸"，不是标签自己的尺寸**（2026-11-26 更正）：原版
            // `update_name_label` 的 `k = label_size.x / 50` 是**棋盘单位 → 像素**的比例，
            // 只能来自 `EntityDrawer` 自己的 `get_size()`（1000×600 ⇒ k = 20）；公式后两行
            // 把结果夹进 `[0, w − mw]` 也只有在 `w` = 控件宽时才有意义（`tests/entity_drawer.rs`
            // 那条 `name_label_layout_matches_update_name_label_arithmetic` 传的就是控件尺寸
            // 500×300）。以前这里把 `name_label.get_size()`（58×40）当第一个实参 ⇒ k = 1.16、
            // `cx` 被夹到 0 ⇒ 名字标签被摆到控件**左上角**（实测 `pos=(0, 14.7)`），
            // 于是士兵头上没有 `cmdgz`（真帧：标签在 (772,783)–(842,854)）。
            let min_size = name_label.clone().get_minimum_size();
            let layout = entity_drawer::name_label_layout(
                BoardVec2::new(size.x as f64, size.y as f64),
                BoardVec2::new(min_size.x as f64, min_size.y as f64),
                packed,
                board.x,
                board.y,
                alpha,
            );
            name_label.set_position(Vector2::new(
                layout.position.x as f32,
                layout.position.y as f32,
            ));
            name_label.set_modulate(Color::from_rgba(
                layout.modulate.r,
                layout.modulate.g,
                layout.modulate.b,
                layout.modulate.a,
            ));
            // 原版：`t >= 60` 走"藏起来"那条分支（见 `entity_drawer::death_alpha` 的文档）。
            name_label.set_visible(layout.modulate.a > 0.0);
        }
    }

    /// 设置某个实体的名字文本（`RichTextLabel::text`）。
    ///
    /// 名字**从哪来**不归本模块（原版走名字管理器 / backtracer，`entity_drawer::UNKNOWNS` 之外的事），
    /// 所以这里由调用方给。设完立刻重摆一次：文本变了 ⇒ 最小尺寸变了 ⇒ 布局跟着变。
    #[func]
    fn set_entity_name(&mut self, id: i64, name: GString) {
        // 先把「改文本」做完再 `relayout()`：`relayout` 借 `*self`，
        // 而 `entities.get_mut()` 的借用必须在那之前结束（否则 E0499）。
        {
            let Some(entity) = self.entities.get_mut(&id) else {
                return;
            };
            entity.name = name.clone();
            entity.name_label.clone().set_text(&name);
        }
        self.relayout();
    }

    /// 给某个实体的名字标签**上色**（队色）。
    ///
    /// 真帧观测：士兵名 `cmdgz` 的笔画色 = **(125,104,0)**，与这名士兵 `ColorInfo` 的
    /// 颜色逐值相同（`_soldiers[i]["color"]`）⇒ 名字用的是**这个玩家自己的队色**，不是
    /// `EntityDrawer` 那个全局 `team_color`。
    ///
    /// ⚠️ **我方接线**（推断）：原版这条颜色从哪来没解出来（`update_name_label` 的算术只
    /// 产出 modulate 的 alpha，RGB 恒 1；颜色更可能是建 label 时按实体设的 theme override，
    /// 或者由 graphics/color 事件链写进标签）。缺的证据：`get_entity_name_label`
    /// （x86 `0x4bb…`）里对 `RichTextLabel` 的颜色/描边 theme 项的写入指令。
    #[func]
    fn set_entity_name_color(&mut self, id: i64, color: Color) {
        let Some(entity) = self.entities.get(&id) else {
            return;
        };
        entity.name_label.clone().add_theme_color_override(
            &StringName::from("default_color"),
            color,
        );
    }

    /// 设置"死了多少刻"（`None` 等价 = 还活着），返回这一帧的名字标签不透明度。
    ///
    /// 算术是 `entity_drawer::death_alpha`（`1 + ticks/(−60)`，封底 0；`ticks >= 60` ⇒ 0）。
    /// 负的 `ticks` 表示"复活"。
    #[func]
    fn set_entity_dead_ticks(&mut self, id: i64, ticks: i64) -> f64 {
        let dead_ticks = if ticks < 0 { None } else { Some(ticks as u32) };
        {
            let Some(entity) = self.entities.get_mut(&id) else {
                return 1.0;
            };
            entity.dead_ticks = dead_ticks;
        }
        self.relayout();
        match dead_ticks {
            None => 1.0,
            Some(t) => entity_drawer::death_alpha(t),
        }
    }

    /// `_ready` 被引擎调用过几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// 这一帧要画哪些实体、按什么顺序（**位置表接线**，任务 41）。
    ///
    /// 原版 `EntityDrawer::draw` 的循环头（x86 `0x4b3130`）就是从
    /// `PlanePosManager::get_all_positions`（`0x4b2e32`）返回的 `Vec<u32>` 里逐个取 id：
    ///
    /// ```text
    /// 0x4b3135  mov  r15d, [rcx]      ; id = *it（**原序**，那一段没有排序调用）
    /// 0x4b314b  call get_position     ; None ⇒ 跳过（0x4b3155 jne 0x4b37f0）
    /// 0x4b315b…0x4b317f               ; x/y/radius ← 三份 f64（[rsp+0xb0]/[rsp+0xa8]/[rsp+0x60]）
    /// 0x4b3199 call get_alive         ; 门 1（GraphwarLifeManager，0x454950）
    /// 0x4b31ac call is_on_turn        ; 门 2（GraphwarTurnManager，0x462b50）
    /// ```
    ///
    /// 本函数的规则（每一条都写清出处）：
    ///
    /// 1. **表非空 ⇒ 表驱动**：id 与顺序来自
    ///    [`GraphwarPlanePosManager::positions_sorted`]（= `get_all_positions` 后按 id 升序）。
    ///    排序是**我方差异**：原版直接用散列序（顺序 `未知`），见 `docs/16` §46 与本文件头。
    /// 2. **门**（任务 46）：**attach 到两份真 manager 且开关打开**时用真查询 —
    ///    `get_alive(id) != 0` ∧ `is_on_turn(id)`（`0x4b3199` / `0x4b31ac` 两个调用，
    ///    判据在 `0x4b32e2` / `0x4b34c3`）。**拿不到 manager / 开关关掉**时回落 `prep`
    ///    成员近似（上一轮的行为，逐字保留：第九轮 41 号的教训，见 `docs/16` §48）。
    /// 3. **`V`**：真查询那一支用 `GraphwarTickHandler::get_fractional_tick`
    ///    （原版 `0x4b2e10`，本层经 KernelBus 取，`kernel_set_clock` 可摆）；`prep` 那一支
    ///    仍用 `prep` 里的 `v`。`base`（`map[i]`）两支撑都来自 `prep`（来源仍 `未知`）。
    ///    ⇒ 表里有、`prep` 里没有的实体：**真查询那一支照画**（`V` 从 tick handler 取，
    ///    `base = None`），`prep` 那一支仍**不画**（它的门就是 `prep` 成员）。
    /// 4. **表空（或 [`Self::pos_table_enabled`] = `false`）⇒ 代偿路径**：用 `prep` 自己那套
    ///    坐标（`set_ring` 填的，`play_screen.gd::_sync_entities` 走的就是它）—— 这条路径
    ///    **保留可回退**，本轮没有删。
    fn rings_for_frame(&mut self) -> Vec<(i64, PrepRing)> {
        if self.pos_table_enabled {
            let table = self.pos_manager.borrow().positions_sorted();
            if !table.is_empty() {
                // 门的数据源：开关关掉 / 没 attach ⇒ `None`（= 上一轮那条 `prep` 近似）。
                let gating =
                    if self.gate_query_enabled { self.gate_managers.clone() } else { None };
                self.gate_source = if gating.is_some() { "managers" } else { "prep" };
                // 真查询那一支的 `V` = `get_fractional_tick`（原版 0x4b2e10）。
                let clock = if gating.is_some() { self.kernel_clock() } else { 0.0 };
                let mut out = Vec::with_capacity(table.len());
                for (id, pos) in table {
                    let id = i64::from(id);
                    let (visible, v) = match &gating {
                        Some(gate) => {
                            self.gate_queried += 1;
                            (gate.passes(id), clock)
                        }
                        // 近似：`prep` 成员（上一轮的门），`v` 也取 `prep` 的。
                        None => {
                            let Some(known) = self.prep.get(&id) else { continue };
                            (true, known.v)
                        }
                    };
                    if !visible {
                        continue;
                    }
                    out.push((
                        id,
                        PrepRing {
                            // x/y/radius **来自位置表**（原版三条 movsd）。
                            board: BoardVec2::new(pos[0], pos[1]),
                            pos_radius: pos[2],
                            v,
                            // `map[i]` 仍只由 `prep` 给（来源 `未知`，见文档第 3 条）。
                            base: self.prep.get(&id).and_then(|p| p.base),
                        },
                    ));
                }
                return out;
            }
        }
        // 代偿路径：`prep` 是 `BTreeMap` ⇒ 天然 id 升序，与上一轮行为逐字相同。
        self.gate_source = "prep";
        self.prep.iter().map(|(id, prep)| (*id, *prep)).collect()
    }

    /// 这一帧的时钟 `V`：原版 `draw` 里 `0x4b2e10 call 0x519530`
    /// （`GraphwarTickHandler::get_fractional_tick`）的读数，本层经 KernelBus 取。
    ///
    /// 没有通道时返回 `0.0`（与 `KernelBus::fractional_tick` 的 `None` 分支同值）。
    fn kernel_clock(&self) -> f64 {
        match crate::_scaffold::root_wiring::kernel_bus() {
            Some(bus) => bus.borrow().fractional_tick(),
            None => 0.0,
        }
    }

    // -----------------------------------------------------------------------
    // 位置表（`GraphwarPlanePosManager`，任务 41）
    // -----------------------------------------------------------------------
    //
    // 原版没有这一组 `#[func]`：表由 `pos_info_event` 的生产者写（`GraphwarRemoteEventHandler`
    // → `set_position`），绘制侧（`draw`/`process`）只读。本类自己持有那张表（见字段文档），
    // 所以**读写两端都在这里**：下面的 `pos_set_position` / `pos_remove` 等价于生产者，
    // 其余是只读观测口（验收脚本用它断言"读的是这张表、顺序是排过序的"）。

    /// 往位置表写一格：`set_position(id, (x, y, radius))`（原版 x86 `0x3c64c0`）。
    ///
    /// 三份 f64 的语义与线格式 `PosInfo` 的 `pos_x` / `pos_y` / `radius` 对齐
    /// （`PlanePos` 的精确写法仍是**推断**，见 `graphwar_plane_pos_manager` 文件头 §四）。
    /// 副作用与原版一致：**同时发一条 `PosInfo`(tag 26)** 到本表的事件队列
    /// （[`Self::pos_event_count`] 数得到）。
    ///
    /// 返回 `false` = `id` 超出 `u32`（原版的键就是 `u32`）。
    #[func]
    fn pos_set_position(&mut self, id: i64, x: f64, y: f64, radius: f64) -> bool {
        let Ok(id) = u32::try_from(id) else {
            return false;
        };
        let pos: PlanePos = [x, y, radius];
        self.pos_manager.borrow().set_position(id, pos);
        self.base_mut().queue_redraw();
        true
    }

    /// 从位置表删一格：`handle_entity_removed_event`（原版 x86 `0x3c6b40`，只认 tag 14）。
    ///
    /// 返回"原来有没有"（原版这条路径**不发事件**）。
    #[func]
    fn pos_remove(&mut self, id: i64) -> bool {
        let Ok(id) = u32::try_from(id) else {
            return false;
        };
        let removed = self
            .pos_manager
            .borrow()
            .handle_entity_removed_event(ENTITY_REMOVED_EVENT_TAG, id)
            .unwrap_or(false);
        if removed {
            self.base_mut().queue_redraw();
        }
        removed
    }

    /// 位置表里有几个实体（= `get_all_positions().len()`）。
    #[func]
    fn pos_count(&self) -> i64 {
        self.pos_manager.borrow().get_all_positions().len() as i64
    }

    /// `get_all_positions()` 的**原序**（散列序，`未知`）—— 只用于**观测**"原版那个顺序"，
    /// 绘制顺序**不许**依赖它。
    #[func]
    fn pos_ids_raw(&self) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        for id in self.pos_manager.borrow().get_all_positions() {
            out.push(id as i64);
        }
        out
    }

    /// [`GraphwarPlanePosManager::get_all_positions_sorted`]：id 升序 —— **`_draw` 用的就是这个**。
    #[func]
    fn pos_ids_sorted(&self) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        for id in self.pos_manager.borrow().get_all_positions_sorted() {
            out.push(id as i64);
        }
        out
    }

    /// 表里某个 id 的三份 f64：`[pos_x, pos_y, radius]`；没有这一格 ⇒ 空数组。
    #[func]
    fn pos_get_position(&self, id: i64) -> PackedFloat64Array {
        let mut out = PackedFloat64Array::new();
        let Ok(id) = u32::try_from(id) else {
            return out;
        };
        if let Some(pos) = self.pos_manager.borrow().get_position(id) {
            for v in pos {
                out.push(v);
            }
        }
        out
    }

    /// 最近一次 `_draw` **实际画的实体 id 顺序**（观测点：排序有没有生效）。
    #[func]
    fn pos_drawn_ids(&self) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        for id in &self.last_drawn_ids {
            out.push(*id);
        }
        out
    }

    /// 位置表那条事件队列里积了几条（`set_position` 每条 +1，tag 26 `PosInfo`）。
    #[func]
    fn pos_event_count(&self) -> i64 {
        self.pos_manager.borrow().event_queue.borrow().len() as i64
    }

    // -----------------------------------------------------------------------
    // 门控（任务 46）：`get_alive`(0x454950) + `is_on_turn`(0x462b50) 的真查询
    // -----------------------------------------------------------------------
    //
    // 原版 `EntityDrawer::draw` 的循环体（x86 `0x4b3130`–`0x4b34c3`）：
    //
    // ```text
    // 0x4b3199  call 0x454950      ; get_alive(id)  → 状态字节（1 活 / 0 死 / 2 查不到）
    // 0x4b319e  mov  r14d, eax
    // 0x4b31ac  call 0x462b50      ; is_on_turn(id) → bool
    // 0x4b31b1  mov  ebx, eax
    // 0x4b32e2  test r14b, r14b    ; 门 1：== 0（死）⇒ 跳过（**2 "查不到" 也算过**）
    // 0x4b34c3  test bl, bl        ; 门 2：false ⇒ 跳过
    // ```
    //
    // 两个 manager 原版从 `[root+0x1668] → backend+0x238 → get_module` 取
    // （Life `0x46c2c0` / Turn `0x46b740`）；本层经 `KernelBus` 的容器取回**同一类型**的那一份
    // （[`Self::gate_attach`]）。**拿不到**（没装通道 / 容器里没有）⇒ `gate_managers = None`
    // ⇒ `_draw` 的门回落 `prep` 成员近似 —— 上一轮的行为一条没删。
    //
    // 下面 3 个（`gate_set_alive` / `gate_set_turn` / `gate_advance_turn`）是**验收用的生产者入口**：
    // 原版这些写操作
    // 分别在 `graphwar_remote_event_handler`（生命 / 回合状态同步）与 `game_state_manager` 里，
    // 本层还没有那些模块，所以让验收脚本直接调 manager 的真方法（不是新造一套状态）。

    /// 从 `KernelBus` 的容器里**按类型取回** `LifeManager` + `TurnManager`（= 原版 `draw` 里
    /// 那两次 `get_module`，x86 `0x46c2c0` / `0x46b740`），缓存到本类。
    ///
    /// 返回 `false` = **拿不到**（通道没装，或容器里缺其中一份）⇒ 门回落 `prep`。
    /// 原版每帧在 `draw` 里现取；本层缓存一次（`Rc` 共享，取回的是同一份 manager）。
    #[func]
    fn gate_attach(&mut self) -> bool {
        let Some(bus) = crate::_scaffold::root_wiring::kernel_bus() else {
            return false;
        };
        let (life, turn) = {
            let bus = bus.borrow();
            (bus.wiring().life_manager(), bus.wiring().turn_manager())
        };
        match (life, turn) {
            (Some(life), Some(turn)) => {
                self.gate_managers = Some(GateManagers { life, turn });
                true
            }
            _ => false,
        }
    }

    /// 丢掉缓存的 manager（模拟"拿不到 manager"），返回原来有没有。
    ///
    /// 之后 `_draw` 的门回落 `prep`（验收脚本用它断言回落分支）。
    #[func]
    fn gate_detach(&mut self) -> bool {
        self.gate_managers.take().is_some()
    }

    /// 有没有 attach 到两份 manager。
    #[func]
    fn gate_available(&self) -> bool {
        self.gate_managers.is_some()
    }

    /// 开关当前值（= 导出属性 `gate_query_enabled`）。
    #[func]
    fn gate_enabled(&self) -> bool {
        self.gate_query_enabled
    }

    /// 改开关，返回改完的值。`false` ⇒ 即使 attach 过也一律走 `prep` 近似。
    #[func]
    fn gate_set_enabled(&mut self, on: bool) -> bool {
        self.gate_query_enabled = on;
        self.base_mut().queue_redraw();
        on
    }

    /// 最近一次 `_draw` 的门数据源：`"managers"`（真查询）/ `"prep"`（回落/近似）。
    #[func]
    fn gate_source(&self) -> GString {
        GString::from(self.gate_source)
    }

    /// 累计"用真查询判过的 id"次数（观测点：>0 才证明门真的走了 manager）。
    #[func]
    fn gate_queried(&self) -> i64 {
        self.gate_queried
    }

    /// `LifeManager::get_alive`（x86 `0x454950`）的**原始状态字节**：`1` 活 / `0` 死 /
    /// `2` 查不到（`MISSING_STATE`）。拿不到 manager ⇒ `-1`。
    #[func]
    fn gate_alive(&self, id: i64) -> i64 {
        let Some(gate) = &self.gate_managers else {
            return -1;
        };
        let Ok(id) = u32::try_from(id) else {
            return -1;
        };
        i64::from(gate.life.borrow().get_alive(id))
    }

    /// `TurnManager::is_on_turn`（x86 `0x462b50`）= `id ∈ 当前士兵 ∪ 本回合玩家`。
    #[func]
    fn gate_on_turn(&self, id: i64) -> bool {
        match &self.gate_managers {
            Some(gate) => gate.turn.borrow().is_on_turn(id),
            None => false,
        }
    }

    /// `GraphwarTurnData::get_current_turn_players`（x86 `0x4627c0`）。
    ///
    /// 按 `TurnData[+0x58]` 分两支（任务 50）：同时回合 = 玩家表全部键；顺序回合 = 当前玩家一个
    /// （出场序表 `[+0x8]/[+0x10]` 的第 `[+0x18]` 个）。顺序支缺状态时见
    /// [`TurnManager::sequential_fallback`]（默认回落 = 旧行为）。
    #[func]
    fn gate_current_players(&self) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        if let Some(gate) = &self.gate_managers {
            for id in gate.turn.borrow().get_current_turn_players() {
                out.push(id);
            }
        }
        out
    }

    /// `GraphwarTurnData::get_current_turn_soldiers`（x86 `0x4628d0`）。
    ///
    /// 同时回合 = 每玩家 `soldiers[cursor]`；顺序回合 = **当前玩家**的 `soldiers[cursor]`
    /// （`0x4629de`–`0x462ab4`；该玩家不在玩家表里 ⇒ 空集，不换人）。
    #[func]
    fn gate_current_soldiers(&self) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        if let Some(gate) = &self.gate_managers {
            for id in gate.turn.borrow().get_current_turn_soldiers() {
                out.push(id);
            }
        }
        out
    }

    /// `GraphwarTurnManager::get_turn_mode`（x86 `0x462c10`）：`TurnData[+0x58]` 那个字节。
    ///
    /// `0` = 顺序回合、`1` = 同时回合（判别值与名字见 `graphwar_turn_manager` 模块文档 §7.1）。
    /// 拿不到 manager ⇒ `-1`。
    #[func]
    fn gate_turn_mode(&self) -> i64 {
        match &self.gate_managers {
            Some(gate) => i64::from(gate.turn.borrow().turn_mode().byte()),
            None => -1,
        }
    }

    /// **验收用的生产者**：真调 `GraphwarTurnManager::set_turn_mode`（x86 `0x462d20`）。
    ///
    /// 只收 `0` / `1`（严格版；二进制本身不校验，非 0 一律当同时回合）。原版这条写操作在
    /// 远端事件处理器（`TurnModeInfo`，`0x3e41ae`）、游戏服（`0x4500c0`）与 `game_state_manager`
    /// 的两态翻转（`0x476730`）里；本层还没有那些模块。
    #[func]
    fn gate_set_turn_mode(&mut self, mode: i64) -> bool {
        let Some(gate) = self.gate_managers.clone() else {
            return false;
        };
        let Ok(byte) = u8::try_from(mode) else {
            return false;
        };
        let Some(mode) = TurnMode::from_known_byte(byte) else {
            return false;
        };
        gate.turn.borrow_mut().set_turn_mode(mode);
        self.base_mut().queue_redraw();
        true
    }

    /// **验收用的生产者**：真调 `GraphwarTurnData::set_turn`（x86 `0x4638f0`）——
    /// 公告"这个玩家现在用这个士兵"，同时把**当前玩家游标**移到它身上。
    ///
    /// `soldier_id == 0` ⇒ 删掉该玩家的整条记录（`0x463a8b`），出场序表不动。
    #[func]
    fn gate_announce_turn(&mut self, player_id: i64, soldier_id: i64) -> bool {
        let Some(gate) = self.gate_managers.clone() else {
            return false;
        };
        gate.turn.borrow_mut().set_turn(player_id, soldier_id);
        self.base_mut().queue_redraw();
        true
    }

    /// 玩家**出场序表**（`TurnData[+0x8]/[+0x10]`，只由 `set_turn` 追加）。
    #[func]
    fn gate_turn_order(&self) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        if let Some(gate) = &self.gate_managers {
            for id in gate.turn.borrow().turn_order() {
                out.push(*id);
            }
        }
        out
    }

    /// 当前玩家游标指向谁（`TurnData[+0x18]` 那一格）；`-1` = 没有（序表空 / 拿不到 manager）。
    #[func]
    fn gate_current_player(&self) -> i64 {
        match &self.gate_managers {
            Some(gate) => gate.turn.borrow().current_player().unwrap_or(-1),
            None => -1,
        }
    }

    /// 顺序支需要的状态齐不齐（模式 = 顺序 **且** 游标指得到一个玩家）。
    /// `false` + 顺序模式 ⇒ 走回落开关那条路。
    #[func]
    fn gate_sequential_state(&self) -> bool {
        match &self.gate_managers {
            Some(gate) => gate.turn.borrow().sequential_state_available(),
            None => false,
        }
    }

    /// 回落开关当前值（`TurnManager::sequential_fallback`，默认 `true`）。
    #[func]
    fn gate_sequential_fallback(&self) -> bool {
        match &self.gate_managers {
            Some(gate) => gate.turn.borrow().sequential_fallback(),
            None => false,
        }
    }

    /// 改回落开关，返回改完的值。`false` ⇒ 顺序支缺状态时按二进制返回**空集**。
    #[func]
    fn gate_set_sequential_fallback(&mut self, on: bool) -> bool {
        let Some(gate) = self.gate_managers.clone() else {
            return false;
        };
        let on = gate.turn.borrow_mut().set_sequential_fallback(on);
        self.base_mut().queue_redraw();
        on
    }

    /// **验收用的生产者**：真调 `GraphwarLifeManager::set_alive`（x86 `0x454b40`）。
    ///
    /// 原版这条写操作在远端事件处理器里（`life_info_event` 那条链）；本层还没有那个模块。
    /// 拿不到 manager ⇒ `false`。
    #[func]
    fn gate_set_alive(&mut self, id: i64, alive: bool) -> bool {
        let Some(gate) = self.gate_managers.clone() else {
            return false;
        };
        let Ok(id) = u32::try_from(id) else {
            return false;
        };
        gate.life.borrow_mut().set_alive(id, alive);
        self.base_mut().queue_redraw();
        true
    }

    /// **验收用的生产者**：在那个 `TurnManager` 上登记一个玩家（`player_id` + 轮转士兵表）。
    ///
    /// 游标从 0 起（[`crate::graphwar_turn_manager::PlayerTurns::new`] 的口径）。
    #[func]
    fn gate_set_turn(&mut self, player_id: i64, soldiers: PackedInt64Array) -> bool {
        let Some(gate) = self.gate_managers.clone() else {
            return false;
        };
        let ids: Vec<i64> = (0..soldiers.len()).map(|i| soldiers[i]).collect();
        gate.turn.borrow_mut().add_player(player_id, Team::One, ids);
        self.base_mut().queue_redraw();
        true
    }

    /// **验收用的生产者**：推进该玩家的游标到**下一个活着的士兵**（= `next_turn` `0x463da0`
    /// 的轮转那一半，存活谓词就是 `life_manager` 状态字节 `& 1`，`0x463fbb`）。
    ///
    /// 返回新的"当前士兵"，`-1` = 拿不到 manager / 没有该玩家。
    #[func]
    fn gate_advance_turn(&mut self, player_id: i64) -> i64 {
        let Some(gate) = self.gate_managers.clone() else {
            return -1;
        };
        let life = Rc::clone(&gate.life);
        // `TurnManager::next_shooter` 收的是**状态字节**（`& 1` 判活由它内部做，
        // 与 `0x463fbb` 的 `test $0x1` 同一个谓词）；越界 id 按"查不到"（2）处理。
        let next = gate.turn.borrow_mut().next_shooter(player_id, |soldier_id| {
            match u32::try_from(soldier_id) {
                Ok(id) => life.borrow().get_alive(id),
                Err(_) => MISSING_STATE,
            }
        });
        self.base_mut().queue_redraw();
        next.unwrap_or(-1)
    }

    // -----------------------------------------------------------------------
    // 准备圈（`_draw` 的输入与观测口）
    // -----------------------------------------------------------------------

    /// 把一具士兵标成「准备态」，给出这一帧的 `PosInfo` + 时钟 `V`。
    ///
    /// 参数口径与原版 `draw` 读到的一致：
    ///
    /// * `x` / `y` —— **棋盘坐标**（与 [`Self::spawn_entity`] 同一个口径；`draw` 里那两个栈槽
    ///   `[rsp+0xb0]` / `[rsp+0xa8]` 的单位**未知**，本类按棋盘坐标处理，见文件头对照表）；
    /// * `pos_radius` —— `PosInfo` 的第 3 个 f64（**未标定**：乘 1.5 才是画出来的半径）；
    /// * `v` —— 驱动张开/闭合的那个时间量（`0x4b2e10 call 0x519530` 的返回值，原版**不知道是什么时钟**）。
    ///
    /// 同一个 `id` 再调一次 = 更新 `x/y/radius/V`（`base` 保留，除非用
    /// [`Self::clear_ring_base`] 清掉）——原版那两张哈希表也是"每帧覆盖"。
    #[func]
    fn set_ring(&mut self, id: i64, x: f64, y: f64, pos_radius: f64, v: f64) {
        // 更新时**保留**已有的 base：原版 `map[i]` 是"该士兵的起始量"，不随每帧调用而变。
        let base = self.prep.get(&id).and_then(|p| p.base);
        self.prep.insert(
            id,
            PrepRing {
                board: BoardVec2::new(x, y),
                pos_radius,
                v,
                base,
            },
        );
        self.base_mut().queue_redraw();
    }

    /// 对应 `0x4b34e0`–`0x4b34e6` 的**查表命中**：哈希表里有这具士兵的起始量
    /// ⇒ `H = min(π/6 + (V − base)/60, 1.0733774899765125)`。
    ///
    /// 没标成准备态（没调过 [`Self::set_ring`]）时返回 `false` 且什么也不做。
    #[func]
    fn set_ring_base(&mut self, id: i64, base: f64) -> bool {
        let ok = match self.prep.get_mut(&id) {
            Some(prep) => {
                prep.base = Some(base);
                true
            }
            None => false,
        };
        if ok {
            self.base_mut().queue_redraw();
        }
        ok
    }

    /// 对应 `0x4b34e6 je 0x4b35b4`：**查不到** ⇒ `H` 恒为 `π/6`（60° 单段）。
    #[func]
    fn clear_ring_base(&mut self, id: i64) -> bool {
        let ok = match self.prep.get_mut(&id) {
            Some(prep) => {
                prep.base = None;
                true
            }
            None => false,
        };
        if ok {
            self.base_mut().queue_redraw();
        }
        ok
    }

    /// 把这具士兵从准备态里删掉 ⇒ 它的环与箭头**一个图元都不再发**。
    ///
    /// 返回"之前是否在准备态"。这就是"关掉准备态就不画"的那个开关
    /// （原版靠 `draw` 里 `test %r14b,%r14b` / `test %bl,%bl` 两处分支实现，语义**未知**）。
    #[func]
    fn clear_ring(&mut self, id: i64) -> bool {
        let removed = self.prep.remove(&id).is_some();
        if removed {
            self.base_mut().queue_redraw();
        }
        removed
    }

    /// 现在有几具士兵处于准备态（`_draw` 只画这些）。
    #[func]
    fn ring_count(&self) -> i64 {
        self.prep.len() as i64
    }

    /// `_draw` 被引擎调用过几次（观测点）。
    #[func]
    fn draw_call_count(&self) -> i64 {
        self.draw_calls
    }

    /// 累计发出过几条 `draw_arc`（观测点）。
    #[func]
    fn drawn_arc_total(&self) -> i64 {
        self.arc_total
    }

    /// 累计发出过几个 `draw_polygon`（箭头，观测点）。
    #[func]
    fn drawn_polygon_total(&self) -> i64 {
        self.polygon_total
    }

    /// 最近一次 `_draw` 发了几条弧（一具准备态士兵恒 `3`）。
    #[func]
    fn last_arc_count(&self) -> i64 {
        self.last_arcs.len() as i64
    }

    /// 最近一次 `_draw` 用的 `draw_set_transform` 实参 `[pos.x, pos.y, scale.x, scale.y]`
    /// （`rotation` 恒 0 ⇒ 不返回；还没画过 ⇒ 四个 0）。
    ///
    /// 期望值 = [`ring_transform`]：`pos = size × 0.5`、`scale = (mirror ? -k : k, -k)`、
    /// `k = size.x / 50`。
    #[func]
    fn last_transform(&self) -> PackedFloat64Array {
        let mut out = PackedFloat64Array::new();
        for v in self.last_transform {
            out.push(v);
        }
        out
    }

    /// 最近一次 `_draw` 的第 `index` 条弧的**全部实参**，按 `draw_arc` 的形参序：
    ///
    /// ```text
    /// [cx, cy, radius, start_angle, end_angle, point_count, width, antialiased, r, g, b, a]
    /// ```
    ///
    /// 都是 **f32 加宽成 f64**的值 = 引擎真正收到的数。⚠️ 单位是**棋盘单位**（不是画布像素）：
    /// 原版把投影交给 `draw_set_transform`（见 [`ring_transform`]），所以 `cx/cy` 就是
    /// `PosInfo` 的 x/y 原值、`radius = pos_radius × 1.5`、`width = 0.3`。越界返回空数组。
    #[func]
    fn last_arc(&self, index: i64) -> PackedFloat64Array {
        let mut out = PackedFloat64Array::new();
        let Ok(index) = usize::try_from(index) else {
            return out;
        };
        let Some(a) = self.last_arcs.get(index) else {
            return out;
        };
        for v in [
            a.center.0 as f64,
            a.center.1 as f64,
            a.radius as f64,
            a.start_angle as f64,
            a.end_angle as f64,
            a.point_count as f64,
            a.width as f64,
            if a.antialiased { 1.0 } else { 0.0 },
            a.color.0 as f64,
            a.color.1 as f64,
            a.color.2 as f64,
            a.color.3 as f64,
        ] {
            out.push(v);
        }
        out
    }

    /// 最近一次 `_draw` 的第 `index` 个箭头多边形的顶点（**画布像素**）。
    ///
    /// 3 个顶点，按原版 push 顺序：尖端（半径 `r` 上）、底边（半径 `r+0.5`、`θ−6°`）、
    /// 底边（`θ+6°`）。越界返回空数组。
    #[func]
    fn last_polygon(&self, index: i64) -> PackedVector2Array {
        let mut out = PackedVector2Array::new();
        let Ok(index) = usize::try_from(index) else {
            return out;
        };
        let Some(points) = self.last_polys.get(index) else {
            return out;
        };
        for p in points {
            out.push(*p);
        }
        out
    }

    /// `_process` 被引擎调用过几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// `on_resized` 被引擎调用过几次（`resized` 信号真的连上了吗）。
    #[func]
    fn resized_call_count(&self) -> i64 {
        self.resized_calls
    }

    /// `resized` 信号的接收方法（原版 [`entity_drawer::RESIZED_HANDLER`] 就叫这个名字）。
    ///
    /// 它是 `#[func]` 所以能被 `Callable::from_object_method` 找到，也就意味着**GDScript 也能直接调**。
    #[func]
    fn on_resized(&mut self) {
        self.resized_calls += 1;
        self.relayout();
    }

    // -----------------------------------------------------------------------
    // 贴图（`GraphwarTextureLoader` 三张表的入口，§51）
    // -----------------------------------------------------------------------

    /// 换一个实体的外观（三张贴图的名字），返回 `false` = 没有这个实体。
    ///
    /// **这是我方接线**：原版没有这个 `#[func]`。原版的名字**不在** `EntityDrawer` 的任何字段里——
    /// `get_entity_{skin,face,hat}_sprite` 每次取图都当场 `get_module::<GraphwarGraphicsManager>`
    /// 再查那三张表（`+0x18/+0x48/+0x78`，键 = 实体 id），见 `docs/16-client.md` §46.3。
    /// 真链路（事件 → `kernel_pump` 拉表 → `apply_look`）已经接上；这个入口留给调用方直接指定名字。
    ///
    /// 名字就是 `GraphwarTextureLoader` 三张表的键：皮肤 23 个、脸 17 个、帽子 28 个
    /// （`src/graphwar_texture_loader.rs` 的 `SKINS` / `FACES` / `HATS`）。靶子用
    /// `("target", "no_face", "no_hat")`——`res://skins/target.svg` 就在皮肤表里。
    #[func]
    fn set_entity_look(&mut self, id: i64, skin: GString, face: GString, hat: GString) -> bool {
        self.apply_look(
            id,
            EntityLook {
                skin: skin.to_string(),
                face: face.to_string(),
                hat: hat.to_string(),
            },
        )
    }

    /// 真正换装的那一半：写 `look` + 三张 `Sprite2D.texture` + 重摆。
    ///
    /// [`Self::set_entity_look`]（手工入口）与内核拉取（[`Self::kernel_pump`] 的第 ③ 步，
    /// 原版 `get_entity_*_sprite` 那条路）**共用这一份**，所以两条路的贴图/缩放/relayout
    /// 一定一致。
    fn apply_look(&mut self, id: i64, look: EntityLook) -> bool {
        if !self.entities.contains_key(&id) {
            return false;
        }

        // 先把三张贴图取出来（`texture_for` 要 `&mut self`），再改实体——
        // 否则 `self.entities.get_mut(id)` 与 `self.texture_for(…)` 会撞两次可变借用（E0499）。
        let mut textures: Vec<(EntityChildNode, Option<Gd<Texture2D>>)> = Vec::new();
        for kind in entity_drawer::ENTITY_CHILD_NODES.iter().filter(|k| k.is_sprite()) {
            let name = look.name_for(*kind).unwrap_or("");
            textures.push((*kind, self.texture_for(*kind, name)));
        }

        let Some(entity) = self.entities.get_mut(&id) else {
            return false;
        };
        entity.look = look;
        for (sprite, (_kind, texture)) in entity.sprites.iter_mut().zip(textures.iter()) {
            if let Some(texture) = texture {
                sprite.set_texture(texture);
            }
        }
        // 换图之后 `tex_w` 可能变（512 / 128 两种），`scale` 里那个 `128/tex_w` 要重算。
        self.relayout();
        true
    }

    /// 某个实体当前的三条外观名：`[skin, face, hat]`；没有这个实体 ⇒ 空数组。
    #[func]
    fn entity_look(&self, id: i64) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        let Some(entity) = self.entities.get(&id) else {
            return out;
        };
        out.push(&GString::from(entity.look.skin.as_str()));
        out.push(&GString::from(entity.look.face.as_str()));
        out.push(&GString::from(entity.look.hat.as_str()));
        out
    }

    /// 某个实体第 `layer` 层（0 皮肤 / 1 脸 / 2 帽子）贴图的 `resource_path`。
    ///
    /// 真贴图 ⇒ `res://assets/skins/yellow.ctex` 这种；**纯色兜底 ⇒ 空串**
    /// （`ImageTexture::create_from_image` 造出来的资源没有路径）。验收脚本就靠这一条
    /// 区分"真贴图"与"退化"。
    #[func]
    fn sprite_texture_path(&self, id: i64, layer: i64) -> GString {
        let empty = GString::new();
        let Some(entity) = self.entities.get(&id) else {
            return empty;
        };
        let Some(sprite) = usize::try_from(layer).ok().and_then(|i| entity.sprites.get(i)) else {
            return empty;
        };
        sprite
            .get_texture()
            .map(|texture| texture.get_path())
            .unwrap_or(empty)
    }

    /// 贴图统计：`[从磁盘加载成功数, 退化纯色数, 缺失名字数]`（观测点）。
    #[func]
    fn texture_stats(&self) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        out.push(self.loaded_textures);
        out.push(self.fallback_textures);
        out.push(self.missing_assets.len() as i64);
        out
    }

    /// 缺哪些资产（`"skins/yellow"` 这种，去重、保序）。空 = 全部命中。
    #[func]
    fn missing_assets(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for missing in &self.missing_assets {
            out.push(&GString::from(missing.as_str()));
        }
        out
    }

    /// 资产目录就绪吗（`<assets_root>/MANIFEST.tsv` 在不在）。
    ///
    /// 判据与 `client/src/ui/assets.gd` 的 `GwAssets::available()` 一致：以清单文件为准，
    /// "解了一半的目录不算就绪"。`assets_root` 为空串 ⇒ `false`。
    #[func]
    fn assets_available(&self) -> bool {
        if self.assets_root.is_empty() {
            return false;
        }
        let manifest = format!("{}/{}", self.assets_root, ASSETS_MANIFEST);
        FileAccess::file_exists(&GString::from(manifest.as_str()))
    }

    // -----------------------------------------------------------------------
    // 四个取子节点的函数（原版 `get_entity_*`，T102 落码）
    // -----------------------------------------------------------------------
    //
    // 原版 `process` 每帧对每个实体调这 4 个（`docs/16` §21.5 的调用序：
    // skin → face → hat → label，三次 `?` 早退）。返回值 = 子节点句柄，
    // **取不到就不画**（原版是 `Option`，`?` 掉整具实体）。
    //
    // **我方化简（明写）**：原版是 4 张独立 `HashMap<i32, Gd<…>>` + 4 个**各自重复**
    // get-or-create 体的 1236/2832 B 函数；我方是 1 张 `BTreeMap<i64, EntityNodes>` 装 4 个句柄、
    // 节点在 [`Self::spawn_entity`] 里**急建**。所以这里的 4 个函数是"取"而不是"懒建"，
    // 而 `spawn_entity` 那一侧走的是同一套常量（[`SPRITE_TEXTURE_FILTER`] / [`SPRITE_FLIP_V`] /
    // [`ENTITY_CHILD_NODES`]）⇒ 节点形状与原版一致，**差异只在时机**（急建 vs 首次取用时建）。
    // 这条差异登记在 `docs/06` §148.6。

    /// 原版 `EntityDrawer::get_entity_skin_sprite` —— **x86 `0x4bb6b0` / arm64 `0x4c1068`**（1236 B）。
    ///
    /// 体（**观测**；本轮逐条读 `0x4bb6b0`，`addr_probe.py dis 0x4bb6b0`）：
    ///
    /// ```text
    /// cmp qword ptr [rsi+0x60],0        ; ① skin 名字表空？空 ⇒ 直接返回 None（je 返回路径）
    /// call 0x47dec0                     ; ② 用 u32 实体 id 走 hashbrown 的 hash
    /// call 0x46a8e0                     ; ③ get_module::<GraphwarGraphicsManager>（docs/16 §46.3）
    /// call 0x47dec0 / 0x47daa0          ; ④ 再两次 hash（名字表 / loader 表各一把）
    /// call 0x3ed540                     ; ⑤ Sprite2D 的 default_instance（引用串 'Sprite2D'）
    /// call *[0x9bc0b8]                  ; ⑥ CanvasItem::set_texture_filter(4)
    /// call *[0x9bc0c0]                  ; ⑦ Sprite2D::set_flip_v(true)
    /// call *[0x9bb260]                  ; ⑧ Node::add_child
    /// ```
    ///
    /// 返回 `Option<Gd<Sprite2D>>`（表未命中 ⇒ `None`；名字表未命中同样 `None`）。
    fn get_entity_skin_sprite(&mut self, id: i64) -> Option<Gd<Sprite2D>> {
        self.entity_sprite(id, EntityChildNode::SkinSprite)
    }

    /// 原版 `EntityDrawer::get_entity_face_sprite` —— **x86 `0x4bb0a0` / arm64 `0x4c0b94`**（1236 B）。
    ///
    /// 与 skin 那个**逐指令同形**，只有一处结构性差异（**逐位**）：
    /// `cmp qword ptr [rsi+0x90],0` / `lea rdi,[r15+0x98]`（= 脸的**名字**表 `+0x78` + `RcBox` 的 `+0x18`）。
    /// 两条 FDE 尺寸也**逐字节相同**（都是 `0x60d`，`addr_probe.py fde 0x4bb0a0` / `0x4bb6b0`）
    /// ——这是"两个函数是同一段源码的两份实例"的独立旁证。
    fn get_entity_face_sprite(&mut self, id: i64) -> Option<Gd<Sprite2D>> {
        self.entity_sprite(id, EntityChildNode::FaceSprite)
    }

    /// 原版 `EntityDrawer::get_entity_hat_sprite` —— **x86 `0x4b9e70` / arm64 `0x4bfbb0`**（1236 B）。
    ///
    /// 表偏移 `cmp qword ptr [rsi+0xc0],0` / `lea rdi,[r15+0xc8]`（帽子名字表 `+0xa8` + `0x18`）。
    /// ⚠️ 它的 x86 FDE 比 face/skin 多 **0x1f B**（`0x62c` vs `0x60d`，`addr_probe.py fde`）：
    /// arm64 三份 `st_size` 是**完全相同**的 1236 ⇒ 多出来的是 x86 侧一处展开差异（**未解**，
    /// 缺的证据：逐指令 diff 到具体那 31 字节）。**这一条不影响返回值语义**（表/节点/调用序三者一致）。
    fn get_entity_hat_sprite(&mut self, id: i64) -> Option<Gd<Sprite2D>> {
        self.entity_sprite(id, EntityChildNode::HatSprite)
    }

    /// 原版 `EntityDrawer::get_entity_name_label` —— **x86 `0x4ba4a0` / arm64 `0x4c0084`**（2832 B）。
    ///
    /// 这是 4 个里唯一"建节点"路径有明确物证的（**观测**）：
    ///
    /// ```text
    /// [rsi+0xd8] / [rsi+0xf0]                       ; 第 4 张表（label 缓存，再 +0x30）
    /// lea …  'res://name_label.tscn'                ; 见 NAME_LABEL_SCENE（0x1c2b1d）
    /// PackedScene::instantiate_as::<RichTextLabel>() ; 节点类型由那次 cast 定死
    /// Node::add_child
    /// ```
    ///
    /// 我方对应：节点由 [`new_name_label`] 造（逐项照抄 `name_label.tscn` 的 Theme/StyleBox），
    /// 这里只取。**边界**：原版一次调用里既查名字表又建节点，我方查+建分在
    /// `spawn_entity` 与这里两处（登记 §148.6）。
    ///
    /// 体 > `VOL_CAP = 0x800`（0xbf9）⇒ 按预注册 §7 L4 **整函数读过**才下的这条结论：
    /// 逐指令读下来剩下的部分全是 hashbrown 的表操作与 `RichTextLabel` 的 8 个属性写入，
    /// **没有第三张表、没有第二条创建路径**。
    fn get_entity_name_label(&mut self, id: i64) -> Option<Gd<RichTextLabel>> {
        let entity = self.entities.get(&id)?;
        Some(entity.name_label.clone())
    }

    /// 三个 sprite getter 的共同形状（**我方化简的公共体**，见本节开头的说明）。
    ///
    /// 判据与 [`Self::get_entity_skin_sprite`] 里那张调用序表一致：
    /// 表未命中（这里 = 没有这个实体）⇒ `None`；命中 ⇒ 克隆句柄（原版 `Gd` 是引用计数智能指针，
    /// 克隆只加引用、不复制节点）。
    fn entity_sprite(&mut self, id: i64, kind: EntityChildNode) -> Option<Gd<Sprite2D>> {
        let index = kind.sprite_index()?;
        let entity = self.entities.get(&id)?;
        entity.sprites.get(index).cloned()
    }

    /// 取某一层、某个名字的贴图（懒加载 + 缓存 + 兜底）。
    ///
    /// 三步，顺序就是原版 `get_entity_skin_sprite`（x86 `0x4bb6b0`）那三步：
    ///
    /// ```text
    /// ① 名字 → loader 的表 → res://skins|faces|hats/<名>.svg   （原版：HashMap<String, Gd<Texture2D>>
    ///                                                            的 getter，表 +0x150/+0x180/+0x1b0）
    /// ② 路径 → 磁盘上的真贴图（本切片：res://assets/<dir>/<名>.ctex）→ Sprite2D.set_texture
    /// ③ 拿不到 ⇒ **纯色兜底**（原版没有这一条：`get_*_texture` 未命中直接返回 None，
    ///    调用方 `?` 掉；我们的客户端必须能在没有 assets 的机器上跑）
    /// ```
    ///
    /// 返回 `Gd<Texture2D>` 的**克隆**：`Gd` 是引用计数智能指针（`ImageTexture` 是 `RefCounted`），
    /// 缓存里留一份、给 sprite 挂一份，两份都有效。
    fn texture_for(&mut self, kind: EntityChildNode, name: &str) -> Option<Gd<Texture2D>> {
        let texture_kind = texture_kind(kind)?;
        let cache_key = (texture_kind as u8, name.to_string());
        if let Some(texture) = self.textures.get(&cache_key) {
            return Some(texture.clone());
        }

        // ① 原版 loader 的三张表（纯逻辑那侧）按名字查路径。
        let res_path = match texture_kind {
            TextureKind::Skin => self.loader.get_skin_texture(name).cloned(),
            TextureKind::Face => self.loader.get_face_texture(name).cloned(),
            TextureKind::Hat => self.loader.get_hat_texture(name).cloned(),
        };

        // ② 名字在表里 ⇒ 去资产目录要真贴图。
        let mut texture = None;
        if let Some(res_path) = res_path {
            if let Some((_path, loaded)) = load_asset_texture(&self.assets_root.to_string(), &res_path)
            {
                self.loaded_textures += 1;
                texture = Some(loaded);
            } else {
                // 名字合法、资产不在（或资产根为空）⇒ 缺的就是这一份。
                self.note_missing(&res_path);
            }
        } else {
            // 名字不在原版表里：**不是**资产问题，是我们（或调用方）给了个不存在的名字。
            self.note_missing_unknown(texture_kind, name);
        }

        // ③ 兜底：程序生成的纯色（边长 `texture_size`，颜色按层）。
        let texture = texture.unwrap_or_else(|| {
            self.fallback_textures += 1;
            solid_texture(self.texture_size as i32, layer_color(kind.texture_method().unwrap_or("")))
                .unwrap_or_else(|| {
                    godot_error!("EntityDrawer: 连纯色 ImageTexture 都建不出来（texture_size = {}）", self.texture_size);
                    ImageTexture::new_gd().upcast::<Texture2D>()
                })
        });

        self.textures.insert(cache_key, texture.clone());
        Some(texture)
    }

    /// 记一条"缺这份资产"，并在**第一次**遇到时打一条警告（不刷屏）。
    ///
    /// `res_path` 是原版表里的 `res://skins/yellow.svg`；日志里写成 `skins/yellow` 更好读。
    fn note_missing(&mut self, res_path: &str) {
        let rel = res_path
            .strip_prefix("res://")
            .unwrap_or(res_path)
            .strip_suffix(".svg")
            .unwrap_or(res_path.strip_prefix("res://").unwrap_or(res_path));
        if self.missing_assets.iter().any(|m| m == rel) {
            return;
        }
        self.missing_assets.push(rel.to_string());
        if self.assets_root.is_empty() {
            godot_warn!("EntityDrawer: 没有资产目录（assets_root 为空）⇒ {rel} 退化成纯色；把 assets_root 设回 \"{DEFAULT_ASSETS_ROOT}\" 再跑 tools/extract_assets.py 就能拿回真贴图");
        } else {
            godot_warn!(
                "EntityDrawer: 缺少贴图资产 {rel}（{} 下没有 {rel}.ctex/png/webp/svg）⇒ 退化成纯色",
                self.assets_root
            );
        }
    }

    /// 名字不在 `GraphwarTextureLoader` 的表里时记一条（这类"缺"不是资产问题）。
    fn note_missing_unknown(&mut self, kind: TextureKind, name: &str) {
        let dir = match kind {
            TextureKind::Skin => "skins",
            TextureKind::Face => "faces",
            TextureKind::Hat => "hats",
        };
        let rel = format!("{dir}/{name}");
        if self.missing_assets.iter().any(|m| m == &rel) {
            return;
        }
        self.missing_assets.push(rel.clone());
        godot_warn!("EntityDrawer: 名字 {name:?} 不在 GraphwarTextureLoader 的 {dir} 表里（退化成纯色）");
    }

    // -----------------------------------------------------------------------
    // 内核事件通道（任务 42，`docs/16-client.md` §46）—— **本文件唯一的一处新增接线**
    // -----------------------------------------------------------------------
    //
    // 原版这条链（逐环节地址见 §46）：
    //
    // ```text
    // ready  (x86 0x4b4130 / arm64 0x4bab44)
    //   └─ 6 × GraphwarBackend::subscribe_to_event   x86 0x514b40 / arm64 0x406e80
    //        pos_info_event / life_info_event / entity_removed_event /
    //        skin|face|hat_graphics_info_event     (entity_drawer::READY_EVENTS)
    //        ⇒ 六条订阅共用**同一个闭包**   x86 0x4b4990 / arm64 0x4bb378
    //          闭包体只有一句： self[+0x108] = 1   （x86 0x4b4a13 `mov byte [rax+0x108],1`）
    // process (x86 0x4b4b60) / draw (x86 0x4b2890)
    //   └─ **拉**：PlanePosManager::get_all_positions/get_position（x86 0x3c66a0 / 0x3c62b0）
    //             get_entity_*_sprite → GraphicsManager 的三张表（x86 0x46a8e0 + 内联查表）
    //   └─ process 末尾 self[+0x108] = 0         （x86 0x4b5409）
    // ```
    //
    // ⇒ **事件不带数据**，它只是"脏"信号；数据是每帧从 manager 的表里拉的。
    // 这就是把"按回合状态代填 prep / 按 id 判靶子代填外观"换成真链路的那一步：
    // `prep` 与 `EntityLook` 现在由**位置表/外观表**填，不再由调用方猜。
    //
    // ⚠️ 我方差异（**scaffold**）：原版拿 manager 的路径是
    // `节点 → GraphwarRootNode +0x1668 → GraphwarBackend +0x220 → 容器 → get_module`；
    // 本层没有那条对象图 ⇒ 走 `_scaffold::root_wiring` 的 thread-local 通道
    // （`install_kernel_bus` / `kernel_bus`），两步 `get_module` 仍是原版语义。

    /// 这个类订阅的那 6 条事件（= [`entity_drawer::READY_EVENTS`] 的判别值）。
    ///
    /// `READY_EVENTS` 是**名字**（`.rodata` 原文），这里把它换成 `0x3d38f0` 解出的判别值；
    /// 单测 `kernel_events_are_the_six_ready_names` 钉住两边同源。
    fn is_kernel_event(tag: u16) -> bool {
        entity_drawer::READY_EVENTS.iter().any(|name| {
            crate::events::Event::from_dispatch_name(name)
                .is_some_and(|event| event.wire_tag() == tag)
        })
    }

    /// 订阅那 6 条事件（= 原版 `ready` 里的 6 次 `GraphwarBackend::subscribe_to_event`）。
    ///
    /// 返回真的订上去几条（原版 = 6）。第一次调用会**装**一条内核事件通道
    /// （[`crate::_scaffold::root_wiring::install_kernel_bus`]，幂等）。
    #[func]
    fn kernel_subscribe(&mut self) -> i64 {
        let bus = crate::_scaffold::root_wiring::install_kernel_bus();
        let mut bus = bus.borrow_mut();
        let mut n = 0;
        for name in entity_drawer::READY_EVENTS {
            if bus.subscribe(name) {
                n += 1;
            }
        }
        self.kernel_subscriptions = n;
        n
    }

    /// 往位置表里写一格：真调 `GraphwarPlanePosManager::set_position`（x86 `0x3c64c0`）。
    ///
    /// **这是给验收脚本用的生产端入口**（原版这条调用在 `graphwar_remote_event_handler`
    /// x86 `0x3e2830` 的 `0x3e3347`，本 crate 还没有那个模块）。返回 `false` = 通道没装。
    #[func]
    fn kernel_push_position(&mut self, id: i64, x: f64, y: f64, radius: f64) -> bool {
        let Ok(id) = u32::try_from(id) else {
            return false;
        };
        match crate::_scaffold::root_wiring::kernel_bus() {
            Some(bus) => bus.borrow().set_position(id, [x, y, radius]),
            None => false,
        }
    }

    /// 往外观表里写一格：真调 `GraphwarGraphicsManager::set_{skin,face,hat}_graphics`。
    ///
    /// `layer`：0 皮肤 / 1 脸 / 2 帽子（对应三张表 `+0x18` / `+0x48` / `+0x78`）。
    /// **验收用的生产端入口**（原版调用点在远端事件处理器与 game_state_manager，见 §46.3）。
    #[func]
    fn kernel_push_graphics(&mut self, id: i64, layer: i64, name: GString) -> bool {
        let Ok(id) = u32::try_from(id) else {
            return false;
        };
        let kind = match layer {
            0 => crate::graphwar_graphics_manager::GraphicsKind::Skin,
            1 => crate::graphwar_graphics_manager::GraphicsKind::Face,
            2 => crate::graphwar_graphics_manager::GraphicsKind::Hat,
            _ => return false,
        };
        match crate::_scaffold::root_wiring::kernel_bus() {
            Some(bus) => bus.borrow().set_graphics(kind, id, &name.to_string()),
            None => false,
        }
    }

    /// **一帧的通道处理**：先投递（只置脏），脏了就回表里拉（位置 + 外观）。
    ///
    /// 返回这一次投递给本类的订阅事件条数（`-1` = 通道没装）。
    /// 拉的动作与返回值分开记（[`Self::kernel_pulled`] / [`Self::ring_count`]），
    /// 因为原版这两步在**不同函数**里（`process_events` 置脏、`process`/`draw` 拉）。
    #[func]
    fn kernel_pump(&mut self) -> i64 {
        let Some(bus) = crate::_scaffold::root_wiring::kernel_bus() else {
            return -1;
        };

        // ① 投递：原版 `GraphwarEventHandler::process_events`（arm64 `0x486970`）→ 六条闭包
        let delivered = {
            let mut bus = bus.borrow_mut();
            bus.pump()
        };
        let mut mine = 0i64;
        for event in &delivered {
            let tag = event.wire_tag();
            self.kernel_last_tags.push(i64::from(tag));
            if Self::is_kernel_event(tag) {
                mine += 1;
            }
        }
        // 观测口只留最近 64 条（长会话里别无限长）
        if self.kernel_last_tags.len() > 64 {
            let drop = self.kernel_last_tags.len() - 64;
            self.kernel_last_tags.drain(..drop);
        }
        self.kernel_pumps += 1;
        self.kernel_events += mine;
        if mine > 0 {
            // 原版闭包唯一的一句：`self[+0x108] = 1`
            self.kernel_dirty = true;
        }

        // ② 拉位置表（原版 `draw` 0x4b2e32/0x4b314b、`process` 0x4b4f24/0x4b5004）
        //
        // ⚠️ **每次都拉**，不是"脏了才拉"：原版 `draw` 里 `get_all_positions` / `get_position`
        // 是无条件调用的（`0x4b2e32` / `0x4b314b` 在直线代码上，前面没有拿 `+0x108` 做分支）；
        // `+0x108` 那个脏标志在 `process` 里只管"要不要建新实体节点"那一段（`0x4b5044`）。
        let positions = bus.borrow().positions();
        let clock = bus.borrow().fractional_tick();
        let mut live: Vec<i64> = Vec::with_capacity(positions.len());
        for (id, pos) in positions {
            let id = i64::from(id);
            live.push(id);
            // `map[i]`（`base`）照旧由 `set_ring_base` 那条路给，拉的时候**保留**（§46.6 未解项）
            let base = self.prep.get(&id).and_then(|p| p.base);
            self.prep.insert(
                id,
                PrepRing {
                    board: BoardVec2::new(pos[0], pos[1]),
                    pos_radius: pos[2],
                    v: clock,
                    base,
                },
            );
        }
        // 表里没有的 ⇒ 不再画。原版 `draw` 遍历的就是这张表的**全部键**（`get_all_positions`）。
        self.prep.retain(|id, _| live.contains(id));
        self.kernel_pulled += live.len() as i64;

        // ③ 拉外观表（原版 `get_entity_{skin,face,hat}_sprite` 里那三次 `get_module` + 内联查表）
        let skins = bus.borrow().graphics(crate::graphwar_graphics_manager::GraphicsKind::Skin);
        let faces = bus.borrow().graphics(crate::graphwar_graphics_manager::GraphicsKind::Face);
        let hats = bus.borrow().graphics(crate::graphwar_graphics_manager::GraphicsKind::Hat);
        let mut looks: BTreeMap<i64, EntityLook> = BTreeMap::new();
        for (id, name) in skins {
            looks.entry(i64::from(id)).or_insert_with(EntityLook::default_look).skin = name;
        }
        for (id, name) in faces {
            looks.entry(i64::from(id)).or_insert_with(EntityLook::default_look).face = name;
        }
        for (id, name) in hats {
            looks.entry(i64::from(id)).or_insert_with(EntityLook::default_look).hat = name;
        }
        for (id, look) in looks {
            let changed = self.entities.get(&id).is_some_and(|entity| entity.look != look);
            if changed {
                self.apply_look(id, look);
                self.kernel_graphics_applied += 1;
            }
        }

        // ④ 每帧末尾清脏（原版 `process` x86 `0x4b5409` `mov byte [rbx+0x108],0`）——
        //    标志只活到"这一帧处理完"，所以 `kernel_dirty()` 读的是"上一次 pump 之后有没有新事件"。
        self.kernel_dirty = false;
        self.kernel_pulls += 1;
        self.base_mut().queue_redraw();
        mine
    }

    /// 摆虚拟时钟（`V` = 原版 `draw` 里 `0x4b2e10 call 0x519530` 那个读数）。
    ///
    /// **我方接线口**（同 `crate::graphwar_plane` 的 `set_fractional_tick`）：
    /// 容器里那个 `GraphwarTickHandler` 的基准是 `(0, 0)`，真读数会是 epoch 量级；
    /// 验收要可复现的角度就必须能把时钟摆到指定读数。`NaN` = 恢复读 tick handler。
    #[func]
    fn kernel_set_clock(&mut self, tick: f64) -> bool {
        let Some(bus) = crate::_scaffold::root_wiring::kernel_bus() else {
            return false;
        };
        bus.borrow_mut()
            .set_clock(if tick.is_nan() { None } else { Some(tick) });
        true
    }

    /// 时钟读数来自哪儿（`"override"` / `"tick_handler"` / `"none"`）。
    #[func]
    fn kernel_clock_source(&mut self) -> GString {
        match crate::_scaffold::root_wiring::kernel_bus() {
            Some(bus) => {
                let source = bus.borrow().clock_source();
                GString::from(source)
            }
            None => GString::from("none"),
        }
    }

    /// 原版的"脏"标志 `+0x108`（六条订阅闭包唯一会写的那一格）。
    #[func]
    fn kernel_dirty(&self) -> bool {
        self.kernel_dirty
    }

    /// `kernel_subscribe()` 真的订上去几条（原版 = 6）。
    #[func]
    fn kernel_subscriptions(&self) -> i64 {
        self.kernel_subscriptions
    }

    /// `kernel_pump()` 被调过几次。
    #[func]
    fn kernel_pumps(&self) -> i64 {
        self.kernel_pumps
    }

    /// 累计**投递给本类**的订阅事件条数。
    #[func]
    fn kernel_events(&self) -> i64 {
        self.kernel_events
    }

    /// 累计"因为脏了而回表里拉"的次数。
    #[func]
    fn kernel_pulls(&self) -> i64 {
        self.kernel_pulls
    }

    /// 累计从位置表里拉到的条目数（每次拉累加这个 ID 的总条数）。
    #[func]
    fn kernel_pulled(&self) -> i64 {
        self.kernel_pulled
    }

    /// 累计因为外观表变了而重贴图的实体数。
    #[func]
    fn kernel_graphics_applied(&self) -> i64 {
        self.kernel_graphics_applied
    }

    /// 最近若干次投递的事件判别值（按投递顺序，最多保留 64 条）。
    #[func]
    fn kernel_last_tags(&self) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        for tag in self.kernel_last_tags.iter().rev().take(64).rev() {
            out.push(*tag);
        }
        out
    }
}

// ---------------------------------------------------------------------------
// 5. 单测（**不碰引擎**：只测「这个类用的是哪份常量/哪份算式」）
// ---------------------------------------------------------------------------
//
// 真正的验收在 `client/tools/entity_drawer_check.gd`（headless，真引擎）。
// 这里只钉住"类里没有第二份算术"这件事——它把 GDScript 侧手算的期望值也提前钉在 Rust 里，
// 两边对不上时至少有一边会红。

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认外观 = `GraphwarPlayerConfig::new` 那三条字面量（arm64 `0x39152c..0x3915a4`）。
    ///
    /// 真机节点树 dump 里皮肤正好是 `res://skins/yellow.svg`（`docs/16-client.md` §2.1.10）。
    /// 这条测试钉住"我们没换成别的默认值"。
    #[test]
    fn default_look_is_the_player_config_triple() {
        let look = EntityLook::default_look();
        assert_eq!(look.skin, "yellow");
        assert_eq!(look.face, "regular_eyes");
        assert_eq!(look.hat, "soldier_helmet");
        // 三个名字**必须**在 `GraphwarTextureLoader` 的三张表里（否则一上手就退化）
        assert!(crate::graphwar_texture_loader::SKINS.contains(&look.skin.as_str()));
        assert!(crate::graphwar_texture_loader::FACES.contains(&look.face.as_str()));
        assert!(crate::graphwar_texture_loader::HATS.contains(&look.hat.as_str()));
    }

    /// 资产路径换算：`res://skins/yellow.svg` → `res://assets/skins/yellow.ctex`（+ 三个备选）。
    #[test]
    fn asset_candidates_only_swap_the_extension() {
        let got = asset_candidates(DEFAULT_ASSETS_ROOT, "res://skins/yellow.svg");
        assert_eq!(got.len(), TEXTURE_EXTS.len());
        assert_eq!(got[0], "res://assets/skins/yellow.ctex");
        assert_eq!(got[1], "res://assets/skins/yellow.png");
        assert_eq!(got[2], "res://assets/skins/yellow.webp");
        assert_eq!(got[3], "res://assets/skins/yellow.svg");
        // 目录与名字一个字不改（原版表的 119 条是证据，换算层不许"顺手修"）
        for c in &got {
            assert!(c.starts_with("res://assets/skins/yellow."), "{c}");
        }
        // 资产根可换（验收脚本用空串模拟"没有 assets 的机器"）
        assert_eq!(
            asset_candidates("", "res://faces/no_face.svg")[0],
            "/faces/no_face.ctex"
        );
    }

    /// 三张表里的 **68** 个名字（23 皮肤 + 17 脸 + 28 帽子）都能换算出一组候选路径。
    ///
    /// 名字表在 `src/graphwar_texture_loader.rs`（原版 `.rodata` 逐条抠的），本文件不复制。
    #[test]
    fn every_table_name_maps_to_candidates() {
        use crate::graphwar_texture_loader::{path_of, TextureKind, FACES, HATS, SKINS};
        for name in SKINS {
            let c = asset_candidates(DEFAULT_ASSETS_ROOT, &path_of(TextureKind::Skin, name));
            assert_eq!(c.len(), TEXTURE_EXTS.len());
            assert!(c[0].starts_with("res://assets/skins/"), "{name}: {}", c[0]);
        }
        for name in FACES {
            let c = asset_candidates(DEFAULT_ASSETS_ROOT, &path_of(TextureKind::Face, name));
            assert!(c[0].starts_with("res://assets/faces/"), "{name}: {}", c[0]);
        }
        for name in HATS {
            let c = asset_candidates(DEFAULT_ASSETS_ROOT, &path_of(TextureKind::Hat, name));
            assert!(c[0].starts_with("res://assets/hats/"), "{name}: {}", c[0]);
        }
        assert_eq!(SKINS.len() + FACES.len() + HATS.len(), 68);
    }

    /// 三张 sprite 各对应 loader 的一张表；名字标签没有表。
    #[test]
    fn texture_kind_matches_the_three_loader_tables() {
        assert_eq!(texture_kind(EntityChildNode::SkinSprite), Some(TextureKind::Skin));
        assert_eq!(texture_kind(EntityChildNode::FaceSprite), Some(TextureKind::Face));
        assert_eq!(texture_kind(EntityChildNode::HatSprite), Some(TextureKind::Hat));
        assert_eq!(texture_kind(EntityChildNode::NameLabel), None);
        // 缓存键要带种类：`bot_0` 在 skins 与 faces 两张表里都有，光用名字会串表。
        for kind in entity_drawer::ENTITY_CHILD_NODES.iter().filter(|k| k.is_sprite()) {
            let look = EntityLook::default_look();
            assert!(look.name_for(*kind).is_some(), "{kind:?} 应该有名字");
        }
        assert_eq!(EntityLook::default_look().name_for(EntityChildNode::NameLabel), None);
    }

    /// 子节点顺序：前三个是 sprite，最后一个是名字标签（原版 `ENTITY_CHILD_NODES`）。
    #[test]
    fn child_node_order_is_three_sprites_then_label() {
        let kinds = entity_drawer::ENTITY_CHILD_NODES;
        assert_eq!(kinds.len(), 4);
        assert_eq!(LAYER_COLORS.len(), 3, "三层贴图各有颜色");
        for (i, kind) in kinds.iter().enumerate() {
            assert_eq!(
                kind.is_sprite(),
                i < 3,
                "第 {i} 个应是 {:?}",
                if i < 3 { "sprite" } else { "名字标签" }
            );
            assert_eq!(kind.is_sprite(), kind.texture_method().is_some());
        }
        assert_eq!(kinds[3], EntityChildNode::NameLabel);
    }

    /// 三层贴图的颜色**互不相同**（不然验收脚本里"三个 sprite 贴图不同"就没意义）。
    #[test]
    fn layer_colors_are_distinct_and_cover_all_sprites() {
        let mut seen = std::collections::BTreeSet::new();
        for kind in entity_drawer::ENTITY_CHILD_NODES.iter().filter(|k| k.is_sprite()) {
            let method = kind.texture_method().expect("sprite 一定有贴图方法名");
            let color = layer_color(method);
            // `color` 是 f32 元组，转成整数键比较。
            let key = [
                (color.0 * 1000.0) as i64,
                (color.1 * 1000.0) as i64,
                (color.2 * 1000.0) as i64,
            ];
            assert!(seen.insert(key), "{method} 的颜色与别人重复");
        }
        assert_eq!(seen.len(), 3);
    }

    /// `relayout` 用的算式与 GDScript 侧手算的期望值一致：**这是那个节点验收脚本的 Rust 版**。
    ///
    /// 控件 `500×300`、队伍色 `0x000000ff`（不翻面）、贴图宽 `8`、棋盘 `(5, 5)`：
    /// `k = 500/50 = 10`、`unit = 128/8 = 16`
    /// ⇒ `position = (250 + 1·10·5, 150 − 10·5) = (300, 100)`
    /// ⇒ `scale = ((1·10/60)·16, (10/−60)·16) = (160/60, −160/60) = (2.6667, −2.6667)`、`flip_h = true`。
    /// ⚠️ **2026-11-26 更正**：旧期望是 `(16/60, −160/60)` —— x 分量漏了 `k`（复原错误，
    /// 见 `src/entity_drawer.rs::sprite_transform` 的"2026-11-26 更正"段与 `docs/06` §58）。
    /// 本条在 `--features gdext-bindings` 下才编译，所以默认 `cargo test` 抓不到它。
    #[test]
    fn sprite_transform_matches_the_hand_computed_expectation() {
        let t = entity_drawer::sprite_transform(
            BoardVec2::new(500.0, 300.0),
            DEFAULT_TEAM_COLOR as u32,
            8,
            5.0,
            5.0,
        );
        assert!((t.position.x - 300.0).abs() < 1e-4, "x={}", t.position.x);
        assert!((t.position.y - 100.0).abs() < 1e-4, "y={}", t.position.y);
        assert!((t.scale.x - 160.0 / 60.0).abs() < 1e-6, "sx={}", t.scale.x);
        assert!((t.scale.y + 160.0 / 60.0).abs() < 1e-6, "sy={}", t.scale.y);
        // 同一张贴图等比：两分量绝对值相等（旧公式下二者差 10 倍）
        assert!((t.scale.x.abs() - t.scale.y.abs()).abs() < 1e-6);
        assert!(t.flip_h, "x > 0 ⇒ flip_h");
    }

    /// 默认队伍色不许翻面（验收脚本靠这一点手算位置）。
    #[test]
    fn default_team_color_does_not_mirror() {
        assert!(!entity_drawer::team_mirror(DEFAULT_TEAM_COLOR as u32));
        assert_eq!(
            entity_drawer::team_mirror_sign(DEFAULT_TEAM_COLOR as u32),
            1.0
        );
        // 绿色队伍（`g` 置位且 `r` 不置位）⇒ 翻面。判据见 `entity_drawer::team_mirror`。
        assert!(entity_drawer::team_mirror(0x0000_ff00));
        assert_eq!(entity_drawer::team_mirror_sign(0x0000_ff00), -1.0);
    }

    /// 死亡淡出：验收脚本要断言的三档（`0 → 1.0`、`30 → 0.5`、`>=60 → 0.0`）。
    #[test]
    fn death_alpha_three_reference_points() {
        assert_eq!(entity_drawer::death_alpha(0), 1.0);
        assert!((entity_drawer::death_alpha(30) - 0.5).abs() < 1e-12);
        assert_eq!(entity_drawer::death_alpha(60), 0.0);
        assert_eq!(entity_drawer::death_alpha(1000), 0.0);
    }

    /// 名字标签布局：直接用原版算式跑一遍，确认不会 panic 且位置在控件内。
    #[test]
    fn name_label_layout_stays_inside_the_control() {
        let layout = entity_drawer::name_label_layout(
            BoardVec2::new(120.0, 20.0), // 标签尺寸
            BoardVec2::new(100.0, 18.0), // 最小尺寸
            DEFAULT_TEAM_COLOR as u32,
            5.0,
            5.0,
            1.0,
        );
        assert!((0.0..=120.0).contains(&layout.position.x), "{:?}", layout);
        assert_eq!(layout.modulate.a, 1.0);
    }

    /// 准备圈的投影就是 sprite 那条投影（本文件**没有第二份**棋盘→像素算式）。
    ///
    /// 控件 `500×300`、不翻面、棋盘 `(5, 5)` ⇒ `k = 10` ⇒ `(250 + 50, 150 − 50) = (300, 100)`，
    /// 与 [`sprite_transform_matches_the_hand_computed_expectation`] 那两个数**必须一样**。
    #[test]
    fn ring_projection_reuses_the_sprite_projection() {
        let size = BoardVec2::new(500.0, 300.0);
        let center = ring_center_px(size, DEFAULT_TEAM_COLOR as u32, BoardVec2::new(5.0, 5.0));
        assert!((center.x - 300.0).abs() < 1e-4, "x={}", center.x);
        assert!((center.y - 100.0).abs() < 1e-4, "y={}", center.y);
        assert!((ring_canvas_scale(size) - 10.0).abs() < 1e-9);
    }

    /// 一具准备态士兵发 **6** 条图元：先 3 个箭头（`Polygon`）、再 3 段弧（`Arc`），
    /// 且弧的实参就是 `docs/140` §2 的那几条立即数（10 折点 / 0.3 宽 / 关 AA / `∓H`）。
    ///
    /// 这条测试钉的是"脚手架**没有**偷偷改算术"：它把
    /// [`entity_drawer::entity_primitives`] 原样跑一遍，逐条对形参。
    #[test]
    fn ring_issues_three_arrows_then_three_arcs_with_the_immediate_values() {
        let input = entity_drawer::EntityRingInput {
            center: BoardVec2::new(5.0, 5.0),
            pos_radius: 2.0,
            v: 0.0,
            base: None,
            color: entity_drawer::Color::from_packed_rgb(0x0068_7d),
        };
        let prims = entity_drawer::entity_primitives(input);
        assert_eq!(prims.len(), RING_PRIMITIVES_PER_ENTITY);

        let h = entity_drawer::HALF_ANGLE_MIN_RAD; // base 查不到 ⇒ 恒 π/6
        let angles = entity_drawer::segment_angles(0.0);
        let radius = entity_drawer::drawn_radius(2.0);

        for (i, prim) in prims.iter().enumerate() {
            match prim {
                entity_drawer::Primitive::Polygon { points, colors } => {
                    assert!(i < 3, "箭头必须都在弧之前（代码地址序）：i={i}");
                    assert_eq!(points.len(), 3);
                    assert_eq!(colors.len(), 3, "同一个颜色 3 份（0x4b9c3c/0x4b4c5c/0x4b4c76）");
                    // 尖端在 `r` 上、底边两点在 `r + 0.5` 上
                    let d0 = (points[0].x - 5.0).hypot(points[0].y - 5.0);
                    let d1 = (points[1].x - 5.0).hypot(points[1].y - 5.0);
                    assert!((d0 - radius).abs() < 1e-12, "d0={d0} r={radius}");
                    assert!(
                        (d1 - (radius + entity_drawer::ARROW_RADIUS_DELTA)).abs() < 1e-12,
                        "d1={d1}"
                    );
                }
                entity_drawer::Primitive::Arc {
                    radius: r,
                    start_angle,
                    end_angle,
                    point_count,
                    width,
                    antialiased,
                    ..
                } => {
                    assert!(i >= 3, "弧必须在箭头之后：i={i}");
                    assert_eq!(*point_count, entity_drawer::ARC_POINT_COUNT); // 10
                    assert_eq!(*width, entity_drawer::ARC_WIDTH as f64); // 0.3f32 加宽
                    assert!(!*antialiased); // 关 AA
                    assert_eq!(*r, radius); // f32(pos_radius) × 1.5f32
                    let a = angles[i - 3];
                    assert_eq!(*start_angle, a - h); // subsd @0x4b3620
                    assert_eq!(*end_angle, a + h); // addsd @0x4b362c
                }
            }
        }
    }

    /// 闭合态封顶：`V` 再大，`H` 也只到 `1.0733774899765125`（≈61.5°，单段 123°）。
    #[test]
    fn closed_ring_caps_at_the_documented_half_angle() {
        assert_eq!(
            entity_drawer::half_angle(60.0, Some(0.0)),
            entity_drawer::HALF_ANGLE_MAX_RAD
        );
        // 张开：`V == base` ⇒ H 就是 π/6（单段 60°）
        assert_eq!(
            entity_drawer::half_angle(0.0, Some(0.0)),
            entity_drawer::HALF_ANGLE_MIN_RAD
        );
    }

    /// 通道订阅的那 6 条事件 = `ready` 里那 6 个名字（`0x1c2a95..`），判别值逐条对上。
    ///
    /// 这条把**名字**（`.rodata` 原文，`entity_drawer::READY_EVENTS`）与**判别值**
    /// （`0x3d38f0` 解出来的 `src/events.rs`）钉在一起——两边任何一边改了这里就红。
    #[test]
    fn kernel_events_are_the_six_ready_names() {
        let tags: Vec<u16> = entity_drawer::READY_EVENTS
            .iter()
            .map(|name| {
                crate::events::Event::from_dispatch_name(name)
                    .unwrap_or_else(|| panic!("{name} 不在 0x3d38f0 的派发表里"))
                    .wire_tag()
            })
            .collect();
        assert_eq!(tags, vec![26, 27, 14, 51, 53, 55], "pos/life/removed/skin/face/hat");
        for tag in tags {
            assert!(EntityDrawer::is_kernel_event(tag), "tag {tag} 应被认成订阅事件");
        }
        // 反例：`TurnInfo`(43) 不在那 6 条里（原版 `ready` 里也没有它）
        assert!(!EntityDrawer::is_kernel_event(43));
    }

    // -----------------------------------------------------------------------
    // T102：四个 `get_entity_*` 落码时钉住的**反汇编事实**
    // -----------------------------------------------------------------------
    //
    // 期望值**全部来自 `addr_probe.py dis` 的指令**（`docs/06` §148.4 附了逐条指令），
    // 不是来自我方实现的输出（`AGENTS.md` §0.5）。

    /// 三个 sprite getter 的表偏移：**步长恰 0x30**、`cmp` 立即数恰 = 块起点 + 0x18。
    ///
    /// 这两条一起才说明"三个函数是同一段源码的三份实例、只差读哪张表"：
    /// 若某个函数读的是**另一类**字段（例如 8 B 的 `Gd` 槽），步长就不会是 0x30。
    #[test]
    fn getter_name_map_offsets_step_by_hashmap_size() {
        let rows = entity_drawer::ENTITY_GETTER_NAME_MAP_OFFSETS;
        // 层序与原版调用序一致（skin → face → hat，`docs/16` §21.5）
        assert_eq!(
            rows.map(|r| r.3),
            [
                EntityChildNode::SkinSprite,
                EntityChildNode::FaceSprite,
                EntityChildNode::HatSprite
            ]
        );
        // `HashMap<u32, String>` = 48 B ⇒ 三张表相邻
        assert_eq!(rows[1].1 - rows[0].1, 0x30, "face 表应紧跟 skin 表");
        assert_eq!(rows[2].1 - rows[1].1, 0x30, "hat 表应紧跟 face 表");
        // 指令里的 `cmp qword ptr [rsi+imm],0`：imm 恒 = 块起点 + 0x18
        for (name, base, imm, _kind) in rows {
            assert_eq!(imm, base + 0x18, "{name} 的 cmp 立即数应 = 表起点 + 0x18");
        }
        // 实测值（逐位，`addr_probe.py dis 0x4bb6b0 / 0x4bb0a0 / 0x4b9e70`）
        assert_eq!(rows[0].2, 0x60, "skin 的 cmp 立即数（0x4bb6b0）");
        assert_eq!(rows[1].2, 0x90, "face 的 cmp 立即数（0x4bb0a0）");
        assert_eq!(rows[2].2, 0xc0, "hat 的 cmp 立即数（0x4b9e70）");
    }

    /// 第 4 张表（名字标签）的块起点 = 帽子那张 + 0x30；`cmp` 立即数 = 起点 + 0x18。
    ///
    /// 逐位：`addr_probe.py dis 0x4ba4a0` 里出现 `[rsi+0xd8]` 与 `[rsi+0xf0]` 两个立即数。
    #[test]
    fn name_label_map_is_the_fourth_hashmap() {
        let hat_base = entity_drawer::ENTITY_GETTER_NAME_MAP_OFFSETS[2].1;
        let label_base = entity_drawer::ENTITY_GETTER_NAME_LABEL_MAP_BASE;
        assert_eq!(label_base, hat_base + 0x30, "第 4 张表紧跟帽子表");
        assert_eq!(label_base, 0xd8, "0x4ba4a0 里的 [rsi+0xd8]");
        assert_eq!(
            entity_drawer::ENTITY_GETTER_NAME_LABEL_CMP_IMMEDIATE,
            label_base + 0x18,
            "0x4ba4a0 里的 [rsi+0xf0]"
        );
    }

    /// 建节点的三个引擎调用 + 实参常量：`set_texture_filter(4)` → `set_flip_v(true)` → `add_child`。
    ///
    /// 槽→thunk→(类名,方法名) 三对字面量是**逐位**（`docs/06` §148.2 的 C2 自校验）；
    /// 实参 `4` / `true` 是 arm64 `0x4bfe10` / `0x4bfe30` 的立即数（**观测**）。
    #[test]
    fn getter_creation_path_is_three_engine_calls() {
        let calls = entity_drawer::ENTITY_GETTER_ENGINE_CALLS;
        assert_eq!(calls.map(|c| c.0), ["set_texture_filter", "set_flip_v", "add_child"]);
        assert_eq!(calls.map(|c| c.2), ["CanvasItem", "Sprite2D", "Node"]);
        // 槽号是 `.got` 里的偏移（逐位）
        assert_eq!(calls.map(|c| c.1), [0x9bc0b8, 0x9bc0c0, 0x9bb260]);
        // 实参常量的真值（同一份 `.rodata`/立即数）
        assert_eq!(entity_drawer::SPRITE_TEXTURE_FILTER, 4);
        assert!(entity_drawer::SPRITE_FLIP_V);
    }

    /// 层 → 下标映射与 `ENTITY_CHILD_NODES` 的声明序一致；名字标签不是 sprite。
    ///
    /// 这条钉的是[我方化简]的**唯一**自由量（`EntityNodes.sprites` 的下标序）：
    /// 只要它等于 `ENTITY_CHILD_NODES` 的序，四个 getter 的返回就与原版 4 张表一一对应。
    #[test]
    fn sprite_index_mirrors_child_node_order() {
        let sprites: Vec<EntityChildNode> = entity_drawer::ENTITY_CHILD_NODES
            .iter()
            .copied()
            .filter(|k| k.is_sprite())
            .collect();
        assert_eq!(sprites.len(), 3);
        for (expected, kind) in sprites.iter().enumerate() {
            assert_eq!(kind.sprite_index(), Some(expected), "{kind:?} 的下标");
        }
        assert_eq!(EntityChildNode::NameLabel.sprite_index(), None);
        // `texture_method` 与 `sprite_index` 必须同进同退（三层都有贴图方法、标签都没有）
        for kind in entity_drawer::ENTITY_CHILD_NODES {
            assert_eq!(kind.is_sprite(), kind.texture_method().is_some());
            assert_eq!(kind.is_sprite(), kind.sprite_index().is_some());
        }
    }
}
}

#[cfg(feature = "gdext-bindings")]
pub use node_class::*;
