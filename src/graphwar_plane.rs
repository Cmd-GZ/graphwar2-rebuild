//! `graphwar_plane.rs` —— 原版棋盘平面 `GraphwarPlane`（**12 个 GDExtension 类里的第 4 个**）的复原。
//!
//! **地址口径**：全文一律 **ELF 虚地址**（`objdump` 口径）；Ghidra/REA 地址 = 本值 + `0x100000`
//! （只加一次）。arm64 地址只用于**定名**（2.0.60 的 Android 构建没 strip），绝不当 x86 用。
//!
//! # 1. 名字从哪来，地址怎么钉的
//!
//! * **名字**：`.rea-work/apk/arm64/.../libgraphwarrust.so` 的 `.symtab`（2.0.60）。
//!   `graphwarrust::graphwar_plane::GraphwarPlane` 下**有源级名字**的 `FUNC` 一共 8 个方法
//!   + 2 个 `ready::{{closure}}` + gdext 入口（`__register_methods` / `__register_constants` /
//!   4 个 `_::__init::__inner_init`）。类名本身来自 `docs/02` §16.2 那三份 12 元清单。
//! * **地址**：Linux x86-64 `2026-08-28/libgraphwarrust.so`（sha256 `9a538115…`，
//!   与上游根目录那份**同一文件**）。
//! * **判据**：**不是**函数大小（`docs/02` §19 已证否）。用的是下面三类指纹，
//!   每一条都能用 `tools/name_bridge.py` 的同一套数据重跑：
//!
//! | # | 指纹 | 谁靠它定死 |
//! |---|---|---|
//! | 1 | **`panic::Location` 键集**：`src/graphwar_plane.rs:(line,col)` 在两侧各有 **19** 个键，**一个不多一个不少**，且按函数分组后完全同构 | `draw` / `ready` / `process` / `obstacle_partial_update_event` / 4 个 gdext 入口 |
//! | 2 | **被调者多重集**：两侧逐个相等 | `ready` 的 3×`subscribe_to_event`；`process` 的 2×`ObstacleDrawer::update` + 1×`get_team_overview` |
//! | 3 | **调用者集合 + 首尾相接**：同一批调用者、相邻函数严丝合缝 | `on_resized` / `clear_state` / `update_entities`（见 [`PLANE_TAIL_CHAIN`]） |
//!
//! ⚠️ 本轮**不采信** `tools/class_probe.py` 的地址：它读上游 `analysis/*.pkl`，那些 pkl 与
//! `2026-08-28` 这份 `.so` **不是同一次构建**（同一行号 `:274:129` 它给 `0x4b7a70`，按重定位 +
//! capstone 重扫给 `0x4b5df0`；`:218:129` 它给 `0x4b8bd0`，重扫给 `0x4b7a70`）。
//! 所以本文地址**全部**由本轮重扫得出；`fixtures/name_to_addr.tsv` 里那几行 `唯一锚`
//! 与本轮结论一致，可交叉核对。
//!
//! # 2. 它在游戏里是什么
//!
//! `Control` 子类，棋盘本体：**画棋盘框**、**每帧驱动障碍绘制器**、**给障碍/士兵做脏矩形**、
//! **持有 3 个 `FunctionDrawer` 子节点**（函数曲线画在子节点里，不在 `draw` 里）。
//!
//! 一次 `draw`（`0x4b5df0`）只调 **2 个** `CanvasItem` 图元（穷举见 `docs/16-client.md` §26）：
//!
//! | 图元 | GOT 槽 | 调用点 | 实参 | 档位 |
//! |---|---|---|---|---|
//! | `CanvasItem::draw_set_transform` | `0x9bc018` | `0x4b607d` | 平移 / 旋转 0 / 缩放都取自控件尺寸（与 `AxisDrawer::draw` 同构） | **观测** |
//! | `CanvasItem::draw_rect` | `0x9bc028` | `0x4b61ab` | `Rect2(-25,-15,50,30)`、`Color(0,0,0,1)`、`filled=false`、`width=0.2`、`aa=false` | **逐位** |
//!
//! ⇒ **弹道不在这里**：`draw` 里 `draw_line` 的调用点 = **0**。
//!
//! # 2.1 那弹道在哪？（有界穷举的结果，详见 `docs/16-client.md` §26）
//!
//! 全二进制只有 **7 个函数**调用过任何 `CanvasItem` 绘图原语（逐 GOT 槽穷举，含
//! `mov slot,%reg` + `call *%reg` 的寄存器间接形态）：`EntityDrawer::draw`、
//! `GraphwarLongPressAnimation::draw`、`AxisDrawer::draw`、`EffectDrawer::draw`、
//! `PlayerShowcase::draw`、本类的 `draw`、以及 `0x4993c0`。
//! 其中 `draw_line` 共 **10** 个调用点：`AxisDrawer::draw` **8** 个（网格线）、
//! `EffectDrawer::draw` **2** 个（爆炸十字）——**没有一个是曲线**。
//!
//! 曲线（含开火弹道）是 **`Line2D` 子节点**：`Line2D::add_point` 全二进制只有 **1** 个
//! 调用点（`0x498fab`），就在 **`function_drawer::FunctionDrawer::update_function`**
//! （`0x498c70`）里；`Line2D::set_width` 2 个（`0x499050`）、`set_default_color` 2 个
//! （`0x498c70` / `0x4992b0`）。这三个函数**都由 `GraphwarPlane::draw` 调用** ⇒
//! 平面只负责"驱动"，图元在子节点里。
//!
//! # 3. 与 `src/plane.rs` 的关系（本轮搬家）
//!
//! `plane.rs` 是历史上的**混装**文件（`plane` + `plane_pos_manager` + 疑似 `obstacle_manager`，
//! 登记在 `docs/02` §17.4）。本文件是 `graphwar_plane` 的正式归属地。本轮从 `plane.rs`
//! **只搬走属于本模块的那一部分**：[`to_pixel`]（像素变换）。
//!
//! 为什么它属于本模块：`×1000 / 50`、`+500`、`+300` 这三个式子是**逐条**从
//! `GraphwarPlane::obstacle_partial_update_event`（`0x4be3e0`）里读出来的，该函数的
//! `panic::Location` 锚是 `src/graphwar_plane.rs:174:18`（唯一定名）。
//! 掩膜侧（`0x4f11e0` / `0x4f15b0`）**各自**内联了同一个式子，那两处伪代码带的是
//! `src/graphwar_obstacle_manager.rs` 的锚 ⇒ 掩膜代码留在 `plane.rs`，不搬。
//!
//! `plane::to_pixel` 现在是本文件的**再导出**（`pub use`），所以 `_scaffold/` 与
//! `plane.rs` 的既有断言**一个字都不用改**。
//!
//! # 4. 未知（照实写，见 [`UNKNOWNS`]）
//!
//! 实体 tag `0x28` / `0x29` 各代表哪一类、`draw` 里那些游戏内被调者各自画什么、
//! `on_resized` 那三个 `Control::set_*` 设的是哪三个属性 —— 都**没有**证据，一律标「未知」。

// ---------------------------------------------------------------------------
// 地址表
// ---------------------------------------------------------------------------

/// 类名（`docs/02` §16.2 的三份 12 元清单；`downcast` / `cast` 用的字面量长 **13**）。
pub const CLASS_NAME: &str = "GraphwarPlane";

/// 类名列表字面量槽（`.rodata`；`graphwarrust::graphwar_plane::GraphwarPlane` 长 43 的那条在 `0x1c1b89`）。
pub const CLASS_NAME_SLOT: u64 = 0x1c41b7;

/// `GraphwarPlane::obstacle_partial_update_event` 的 `panic::Location` 槽
/// （`src/graphwar_plane.rs:174:18`，`.data.rel.ro` `0x95e930`；`lea` @ `0x4be510`）。
pub const OBSTACLE_PARTIAL_UPDATE_LOCATION_SLOT: u64 = 0x95e930;

/// 一个方法的地址记录。
///
/// `x86_span` 是**到下一个 Ghidra 函数起点的距离**（含尾部对齐填充，所以它 ≥ 真实函数体）。
/// 别拿它当"函数大小"做判据 —— `docs/02` §19 已证否。
#[derive(Clone, Copy, Debug)]
pub struct FuncAddr {
    /// 原版源码级名字（arm64 `.symtab`，去掉了 `::h…` 哈希后缀）。
    pub name: &'static str,
    /// x86-64 虚地址（`0x4b…`）。Ghidra = 本值 + `0x100000`。
    pub x86: u64,
    /// 到下一个 Ghidra 函数起点的距离（含填充）。
    pub x86_span: u64,
    /// arm64 2.0.60 的虚地址（**只用于定名**）。
    pub arm64: u64,
    /// arm64 侧 `.symtab` 的 `st_size`。
    pub arm64_len: u64,
    /// 证据档位（见模块文档 §1 的三类指纹）。
    pub evidence: &'static str,
}

/// 本模块**有源级名字**的 8 个方法 + 1 个 `ready::{{closure}}`（`观测`），按 x86 地址排序。
///
/// （另一个 `ready::{{closure}}`（arm64 `0x4be380`）只做 `RawVec::grow_one`，没有独立指纹，
/// 所以**不列**——它没有一条能钉死 x86 地址的证据，宁可缺也不猜。）
///
/// 两侧相对次序完全一致，这本身也是一条旁证。
pub const PLANE_FUNCS: [FuncAddr; 9] = [
    FuncAddr {
        name: "draw",
        x86: 0x4b5df0,
        x86_span: 0x1590,
        arm64: 0x4bc54c,
        arm64_len: 4592,
        evidence: "观测(key)：9 个 Location 键 {84,87,90,93,108,110,274,276,292} 与 arm64 draw 逐个相等",
    },
    FuncAddr {
        name: "init",
        x86: 0x4b7380,
        x86_span: 0x06f0,
        arm64: 0x4bd73c,
        arm64_len: 1336,
        evidence: "观测(key)：`src/obstacle_drawer.rs:37:97` 两侧双向唯一（内联进来的 ObstacleDrawer 构造）",
    },
    FuncAddr {
        name: "ready",
        x86: 0x4b7a70,
        x86_span: 0x05a0,
        arm64: 0x4bdc74,
        arm64_len: 1400,
        evidence: "观测(key+call)：4 个 Location 键 {218,222,230,238} 相等；3× subscribe_to_event 两侧相等",
    },
    FuncAddr {
        name: "ready::{{closure}}",
        x86: 0x4b8210,
        x86_span: 0x01e0,
        arm64: 0x4be1ec,
        arm64_len: 404,
        evidence: "观测(call)：两侧都调 obstacle_partial_update_event（arm64 0x4be1ec → 0x4c38d8；x86 0x4b8210 → 0x4be3e0）",
    },
    FuncAddr {
        name: "process",
        x86: 0x4b83f0,
        x86_span: 0x03f0,
        arm64: 0x4be534,
        arm64_len: 956,
        evidence: "观测(key+call)：2 个 Location 键 {254,256} 相等；2× ObstacleDrawer::update + 1× get_team_overview 两侧相等",
    },
    FuncAddr {
        name: "on_resized",
        x86: 0x4bddb0,
        x86_span: 0x0120,
        arm64: 0x4c3308,
        arm64_len: 256,
        evidence: "观测(chain)：唯一游戏内调用者 = ready（两侧相同）；两侧都紧接 clear_state",
    },
    FuncAddr {
        name: "clear_state",
        x86: 0x4bded0,
        x86_span: 0x03a0,
        arm64: 0x4c3408,
        arm64_len: 900,
        evidence: "观测(chain)：调用者 = GraphwarRootNode::process + process_button_pressed（两侧相同）；体内 queue_free ×2",
    },
    FuncAddr {
        name: "update_entities",
        x86: 0x4be270,
        x86_span: 0x0170,
        arm64: 0x4c378c,
        arm64_len: 332,
        evidence: "观测(chain)：调用者只有 GraphwarRootNode::process_button_pressed（两侧相同）",
    },
    FuncAddr {
        name: "obstacle_partial_update_event",
        x86: 0x4be3e0,
        x86_span: 0x0150,
        arm64: 0x4c38d8,
        arm64_len: 260,
        evidence: "观测(key)：`src/graphwar_plane.rs:174:18` 两侧双向唯一",
    },
];

/// gdext 入口（不是原版写的方法，是 `#[gdextension]` 生成的样板；列出来是为让地址表**完整**）。
///
/// 四个 `:68:1` 组：arm64 `{0x4b8e60 __register_methods, 0x4b8ff0 __register_constants,
/// 0x4c2000, 0x4c21c0}` ↔ x86 `{0x4b2260, 0x4b23e0, 0x4bc6b0, 0x4bcab0}`。
/// **组内怎么配对**只有「相对次序 + 等长」两条弱证据 ⇒ 标**推断**，不硬指定。
pub const PLANE_GDEXT_ENTRIES: [FuncAddr; 6] = [
    FuncAddr {
        name: "_::__init::__inner_init(:33:10)",
        x86: 0x4bc490,
        x86_span: 0x0220,
        arm64: 0x4c1e28,
        arm64_len: 472,
        evidence: "观测(key)：`src/graphwar_plane.rs:33:10` 两侧双向唯一",
    },
    FuncAddr {
        name: "__register_methods",
        x86: 0x4b2260,
        x86_span: 0x0180,
        arm64: 0x4b8e60,
        arm64_len: 400,
        evidence: "推断：`:68:1` 组内按相对次序 + 等长(384↔400)配对；调用者是 gdext 的 register_user_methods_constants",
    },
    FuncAddr {
        name: "__register_constants",
        x86: 0x4b23e0,
        x86_span: 0x0180,
        arm64: 0x4b8ff0,
        arm64_len: 400,
        evidence: "推断：同上；两侧内容几乎相同（取全局锁 → 什么都不注册 → 解锁）",
    },
    FuncAddr {
        name: "_::__init::__inner_init(:179:1)",
        x86: 0x4bc860,
        x86_span: 0x0250,
        arm64: 0x4c1c58,
        arm64_len: 464,
        evidence: "观测(key)：`src/graphwar_plane.rs:179:1` 两侧双向唯一",
    },
    FuncAddr {
        name: "_::__init::__inner_init(:68:1)a",
        x86: 0x4bc6b0,
        x86_span: 0x01b0,
        arm64: 0x4c2000,
        arm64_len: 448,
        evidence: "推断：`:68:1` 组内相对次序",
    },
    FuncAddr {
        name: "_::__init::__inner_init(:68:1)b",
        x86: 0x4bcab0,
        x86_span: 0x01b0,
        arm64: 0x4c21c0,
        arm64_len: 436,
        evidence: "推断：`:68:1` 组内相对次序（`docs/08` §3 记的那条 lazy static 初始化就是这一族）",
    },
];

/// `on_resized` → `clear_state` → `update_entities` → `obstacle_partial_update_event`
/// 的**首尾相接链**：两侧都对得上，这是本轮钉死中间三个函数的结构指纹。
///
/// 不变量：`prev.x86 + prev.x86_span == next.x86`（`tests/graphwar_plane.rs` 里有断言）。
pub const PLANE_TAIL_CHAIN: [u64; 4] = [0x4bddb0, 0x4bded0, 0x4be270, 0x4be3e0];

// ---------------------------------------------------------------------------
// 坐标变换（`GraphwarPlane::obstacle_partial_update_event`，0x4be3e0）
// ---------------------------------------------------------------------------
//
// 逐指令对应（`objdump -d --start-address=0x4be3e0 --stop-address=0x4be530`）：
//
//   movsd  0x18(%r14),%xmm0        ; size
//   call   *0x4fce63(%rip)         ; 一次转换（身份未定，见 UNKNOWNS）
//   mulsd  0x19fbd0(%rip)          ; × 1000.0
//   divsd  0x19faa8(%rip)          ; ÷ 50.0
//   maxsd/minsd/cvttsd2si          ; 饱和截断到 i32（界 = ±2^31，槽 0x19fbc0 / 0x19f7e0）
//   movupd 0x8(%r14),%xmm0         ; x, y
//   mulpd  0x19d420(%rip)          ; × (1000, 1000)
//   divpd  0x19ce00(%rip)          ; ÷ (50, 50)
//   addpd  0x19ee90(%rip)          ; + (500, 300)
//   psubd / paddd × 半径            ; 角点 (px∓r, py∓r)
//   Vec push（cap +0x50 / ptr +0x58 / len +0x60）

/// 画布宽度（像素）：`500` 是中心、满幅 1000。**观测**（`.rodata` `(500,300)` @ `0x19ee90`）
pub const CANVAS_W: u32 = crate::plane::CANVAS_W;

/// 画布高度（像素）：`300` 是中心 ⇒ 600。**观测**
pub const CANVAS_H: u32 = crate::plane::CANVAS_H;

/// 棋盘在游戏单位下的宽（`x ∈ [-25, 25]`）。**观测**（除数常量 `(50,50)` @ `0x19ce00`）
pub const BOARD_W: f64 = crate::plane::BOARD_W;

/// 棋盘在游戏单位下的高（`y ∈ [-15, 15]`）。
pub const BOARD_H: f64 = crate::plane::BOARD_H;

/// 每游戏单位对应的像素数：`1000 / 50`。
pub const PX_PER_UNIT: f64 = CANVAS_W as f64 / BOARD_W;

/// x 方向比例常量 `1000.0`：`.rodata` 槽 `0x19fbd0`（`mulsd` @ `0x4be425`）。**逐位**
pub const X_SCALE_MUL: f64 = 1000.0;
/// 除数常量 `50.0`：槽 `0x19faa8`（`divsd` @ `0x4be42d`）。**逐位**
pub const X_SCALE_DIV: f64 = 50.0;
/// 平移向量 `(500.0, 300.0)`：槽 `0x19ee90`（`addpd` @ `0x4be470`，一次加两轴）。**逐位**
pub const PIXEL_OFFSET: (f64, f64) = (500.0, 300.0);

/// 游戏坐标 → 像素坐标（**逐位**，见上面的逐指令表）。
///
/// 先乘 1000 再除 50（不写成 `*20`）：浮点舍入顺序会影响边界点，原版就是这么编译的。
/// `as i32` 在 Rust 里是**饱和**转换且 `NaN → 0`，与原版那串
/// `maxsd/minsd/cvttsd2si + cmovp` 钳位等价（界 ±2^31 分别在槽 `0x19fbc0` / `0x19f7e0`）。
///
/// 这条函数**原来在 `src/plane.rs`**，本轮按归属搬到这里；`plane.rs` 仍以 `pub use`
/// 再导出，所以调用点无需改动。
#[inline]
pub fn to_pixel(x: f64, y: f64) -> (i32, i32) {
    let px = x * X_SCALE_MUL / X_SCALE_DIV + PIXEL_OFFSET.0;
    let py = y * X_SCALE_MUL / X_SCALE_DIV + PIXEL_OFFSET.1;
    (px as i32, py as i32)
}

/// 像素空间里的**脏矩形**：`(min_x, min_y, max_x, max_y)`，四个 `i32` 共 16 字节。
///
/// ⚠️ 这**不是** Godot 的 `Rect2i`（那个是 `position + size`）。原版存的是**角点**：
/// `psubd` 得 `(px−r, py−r)`、`paddd` 得 `(px+r, py+r)`，再一次 16 字节存进 `Vec`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

impl PixelRect {
    /// 宽（像素）。原版不存这个值，纯方便调用方。
    pub const fn width(self) -> i32 {
        self.max_x - self.min_x
    }
    /// 高（像素）。
    pub const fn height(self) -> i32 {
        self.max_y - self.min_y
    }
}

/// `0x28`：`obstacle_partial_update_event` 只接受的两种实体 tag 之一。
///
/// 它是 Rust **niche 优化过的 enum 判别值**（`mov (%rsi),%rax; xor 0x8000…; test; cmovs`），
/// 不是手写常量。tag ↔ 实体种类（士兵 / 障碍 / 靶子）**未知**，见 [`UNKNOWNS`]。
pub const ENTITY_TAG_A: u32 = 0x28;
/// `0x29`：另一个被接受的 tag。语义**未知**。
pub const ENTITY_TAG_B: u32 = 0x29;

/// tag 是否属于 `obstacle_partial_update_event` 接受的那两种。
#[inline]
pub fn is_partial_update_tag(tag: u32) -> bool {
    tag == ENTITY_TAG_A || tag == ENTITY_TAG_B
}

/// 实体（`x, y, size`，游戏单位）→ 像素框。
///
/// 半径 = `size * 1000 / 50`（同一套饱和转换）；框 = 中心像素 ∓ 半径 —— **正方形**，
/// 不是圆（与掩膜那边的 [`crate::plane::rasterize`] 不同，这里只是脏矩形）。
pub fn obstacle_rect(x: f64, y: f64, size: f64) -> PixelRect {
    let (px, py) = to_pixel(x, y);
    let r = (size * X_SCALE_MUL / X_SCALE_DIV) as i32;
    PixelRect {
        min_x: px - r,
        min_y: py - r,
        max_x: px + r,
        max_y: py + r,
    }
}

/// `GraphwarPlane::obstacle_partial_update_event`（`0x4be3e0`）的**纯数据**版本：
/// 把实体的像素框推进脏矩形队列。
///
/// 原版行为（**观测**）：
///
/// * tag ∈ {`0x28`, `0x29`} → 算框 + `Vec::push`；
/// * 其它 tag → **panic**（走 `core::panicking::panic`，消息字面量是 `.rodata` `0x1c2bce` 的
///   `"explicit panic"`，即 Rust 的无参 `panic!()`）。**不是**静默跳过。
pub fn obstacle_partial_update_event(
    rects: &mut Vec<PixelRect>,
    tag: u32,
    x: f64,
    y: f64,
    size: f64,
) -> PixelRect {
    if !is_partial_update_tag(tag) {
        // 与原版同构：无参 panic（`"explicit panic"`）。
        panic!();
    }
    let rect = obstacle_rect(x, y, size);
    rects.push(rect);
    rect
}

// ---------------------------------------------------------------------------
// `draw`（0x4b5df0）的图元常量
// ---------------------------------------------------------------------------

/// 棋盘框 `Rect2(-25, -15, 50, 30)`：`.rodata` 16 字节槽 `0x19cc20`（`movaps` @ `0x4b617d`）。
///
/// **逐位**。与 [`crate::axis_drawer::BOARD_RECT`] 是**同一个槽**——两个类画同一个框。
pub const BOARD_RECT: (f32, f32, f32, f32) = (-25.0, -15.0, 50.0, 30.0);

/// 棋盘框颜色 `Color(0, 0, 0, 1)`（黑）：槽 `0x19cf70`，`lea` @ `0x4b618c`。**逐位**
pub const BOARD_COLOR: (f32, f32, f32, f32) = (0.0, 0.0, 0.0, 1.0);

/// 棋盘框线宽 `0.2f32`：槽 `0x1ba598`（`cd cc 4c 3e`），`movss` @ `0x4b619b`。**逐位**
pub const BOARD_WIDTH: f32 = 0.2;

/// `filled` = **false**（`xor %ecx,%ecx` @ `0x4b61a6`）⇒ 画的是**空心框**。**观测**
pub const BOARD_FILLED: bool = false;

/// `antialiased` = **false**（`xor %r8d,%r8d` @ `0x4b61a8`）。**观测**
pub const BOARD_ANTIALIASED: bool = false;

/// `draw` 里 `CanvasItem::draw_set_transform` 的 GOT 槽（`.got` `0x9bc018`，
/// `call *(%rip)` @ `0x4b607d`）。**观测**
pub const DRAW_SET_TRANSFORM_SLOT: u64 = 0x9bc018;

/// `draw` 里 `CanvasItem::draw_rect` 的 GOT 槽（`.got` `0x9bc028`，`call *(%rip)` @ `0x4b61ab`）。**观测**
pub const DRAW_RECT_SLOT: u64 = 0x9bc028;

/// `CanvasItem::draw_line` 的 GOT 槽（本节只用来做**有界否定**的比对基准）。
///
/// 全 `.text` 只有 **10** 个调用点引用它：`AxisDrawer::draw`（`0x4e05b0`）**8** 个 +
/// `EffectDrawer::draw`（`0x4e1520`）**2** 个。`GraphwarPlane::draw` 里是 **0** 个。
pub const DRAW_LINE_SLOT: u64 = 0x9bc228;

/// `GraphwarPlane::draw` 里 `draw_line` 的调用点数 = **0**（有界否定的结论）。
///
/// 详见 `docs/16-client.md` §26。
pub const DRAW_LINE_CALLS_IN_PLANE: usize = 0;

// ---------------------------------------------------------------------------
// `init` / `ready` / `process` 的意图结构
// ---------------------------------------------------------------------------

/// `init` 里建出来并 `add_child` 的 `FunctionDrawer` 子节点数。
///
/// arm64 `init`（`0x4bd73c`）：`Gd::default_instance` ×3、`ExAddChild::new` ×3、
/// `Node::add_child_f…` ×3；x86 `0x4b7380`：`0x4db030` ×3 + 三组
/// `0x406d20` / `0x407030` / `0x406520`。**观测**
pub const FUNCTION_DRAWER_CHILDREN: usize = 3;

/// `process` 每帧调 `ObstacleDrawer::update` 的次数：两侧都是 **2**（`0x3ee190` ×2）。**观测**
pub const OBSTACLE_DRAWER_UPDATES_PER_FRAME: usize = 2;

/// `ready` 里 `Object::connect` 的信号名字面量：`.rodata` `0x1c2a8e`，长 **7**。**观测**
pub const RESIZED_SIGNAL: &str = "resized";
/// `resized` 的处理者名（`Callable::from_object_method` 的实参）：`0x1c2a7a`，长 **10**。**观测**
pub const RESIZED_HANDLER: &str = "on_resized";

/// `ready` 里按顺序订阅的 3 个后端事件（名字 + `.rodata` 槽）。**观测**
///
/// 三条字面量在 `.rodata` 里**首尾相接**（长 18 / 20 / 19），与 `EntityDrawer::ready`
/// 是同一套写法：`0x4362f0`(from_object_method) + `0x3a0fb0`(connect) + 3× `0x44b7f0`(subscribe)。
pub const READY_EVENTS: [(&str, u64); 3] = [
    ("add_obstacle_event", 0x1c2b32),
    ("destroy_obstacle_event", 0x1c2b44),
    ("clear_obstacles_event", 0x1c2b5a),
];

/// `draw` 里按**出现次序**调到的游戏内函数（能定名的那些）。
///
/// 这张表只回答「`draw` 碰了谁」，**不**回答「谁画什么」——后者多数未解（见 [`UNKNOWNS`]）。
pub const DRAW_GAME_CALLS: [(&str, u64); 15] = [
    ("graphwar_draw_util::get_team_overview", 0x4be6e0),
    ("<未定名>：Line2D::set_width ×2 的配置函数", 0x499050),
    ("<未定名>", 0x46bb90),
    ("<未定名>", 0x46a770),
    ("<未定名>", 0x46b5d0),
    ("<未定名>", 0x46aa50),
    ("graphwar_tick_handler::get_fractional_tick", 0x519530),
    ("graphwar_function_manager::get_active_functions_on_tick", 0x456df0),
    ("<未定名>", 0x3bf010),
    ("<未定名>：FunctionDrawer 家族", 0x498b70),
    ("function_drawer::FunctionDrawer::update_function", 0x498c70),
    ("<未定名>：FunctionDrawer 家族，Line2D::set_default_color", 0x4992b0),
    ("<未定名>：FunctionDrawer 家族", 0x4971e0),
    // 引用的是提示文字（.rodata 0x1c21a7）："Write a function of x to hit the target"
    ("<未定名>：draw_circle ×5", 0x4993c0),
    ("<未定名>", 0x46abc0),
];

/// 子节点的运行时句柄占位：真值是 Godot 对象 id（本仓库不链接 Godot，只留类型）。
pub type NodeRef = u64;

/// 本类的**意图结构**：不依赖 Godot 运行时，只描述「原版身上挂着哪些东西」。
///
/// 每组字段都有独立证据（见各自注释）；**字段名都是描述性的**（原版名字未知）。
#[derive(Clone, Debug, Default)]
pub struct GraphwarPlane {
    /// 3 个 `FunctionDrawer` 子节点（[`FUNCTION_DRAWER_CHILDREN`]，`init` 观测）。
    /// 函数曲线由它们画，**不在** `draw` 里（`docs/16-client.md` §26）。
    pub function_drawers: Vec<NodeRef>,
    /// 障碍绘制器（每帧 [`OBSTACLE_DRAWER_UPDATES_PER_FRAME`] 次，`process` 观测；
    /// `init` / drop glue 里出现 `ObstacleDrawer` 也印证本类持有它）。
    pub obstacle_drawers: Vec<NodeRef>,
    /// 脏矩形队列（`obstacle_partial_update_event` 往里 push；原版是 `Vec`，
    /// 字段在 `+0x50`(cap) / `+0x58`(ptr) / `+0x60`(len) —— rustc 会重排字段序）。
    pub partial_updates: Vec<PixelRect>,
    /// `ready` 之后的标志位。**推断**：`ready` 里确有写字段的指令，但偏移与真名未定。
    pub ready: bool,
}

impl GraphwarPlane {
    /// 空状态（`init` 之前的形状）。
    pub fn new() -> Self {
        Self::default()
    }

    /// `init` 的意图：建 [`FUNCTION_DRAWER_CHILDREN`] 个 `FunctionDrawer` 并挂成子节点。
    ///
    /// `allocate` 对应原版的 `Gd::default_instance` + `Node::add_child`（真版本由 Godot 提供）。
    pub fn init_children(&mut self, mut allocate: impl FnMut(&'static str) -> NodeRef) {
        for _ in 0..FUNCTION_DRAWER_CHILDREN {
            let node = allocate("FunctionDrawer");
            self.function_drawers.push(node);
        }
    }

    /// `ready` 的意图：connect 一个信号 + 按顺序订阅 [`READY_EVENTS`]。
    ///
    /// 返回 `(信号名, 处理者名)` 与事件名清单，由调用方（客户端 / GDExtension）照着接。
    pub fn ready_plan(&mut self) -> ((&'static str, &'static str), Vec<&'static str>) {
        self.ready = true;
        (
            (RESIZED_SIGNAL, RESIZED_HANDLER),
            READY_EVENTS.iter().map(|(n, _)| *n).collect(),
        )
    }

    /// `process` 的意图：每帧把障碍绘制器刷 [`OBSTACLE_DRAWER_UPDATES_PER_FRAME`] 次，
    /// 并读一次队伍颜色（`GraphwarDrawUtil::get_team_overview`，`0x4be6e0`）。
    ///
    /// 返回值 = (刷几次, 是否读队伍颜色)。
    pub fn process_plan(&self) -> (usize, bool) {
        (OBSTACLE_DRAWER_UPDATES_PER_FRAME, true)
    }

    /// 收一个障碍/士兵的脏矩形（`obstacle_partial_update_event` 的纯数据入口）。
    pub fn on_obstacle_partial_update(&mut self, tag: u32, x: f64, y: f64, size: f64) -> PixelRect {
        obstacle_partial_update_event(&mut self.partial_updates, tag, x, y, size)
    }
}

// ---------------------------------------------------------------------------
// 未解
// ---------------------------------------------------------------------------

/// 本模块**明确不知道**的东西（照实写；每条都写清缺什么证据）。
pub const UNKNOWNS: [&str; 6] = [
    "实体 tag 0x28 / 0x29 各是哪一类（士兵 / 障碍 / 靶子）：0x4be3e0 只做判别，没有类型名可比",
    "draw 里那些游戏内被调者的语义：其中 5 个连 Location 锚都没有（无 module 可归）",
    "on_resized 里 3 次 Control::set_* 设的是哪三个属性（走 GOT 槽，方法名未解）",
    "obstacle_partial_update_event 开头那次 call *0x9bb288（0x4be41f）是什么转换：入参是 size，出参直接乘 1000",
    "clear_state 里引用 EntityDrawer / EffectDrawer 类名的用途（只是 cast 目标？还是清子节点？）",
    "GraphwarPlane 的字段真名与偏移（只确证了那个 Vec 的三个槽 +0x50 / +0x58 / +0x60）",
];

// ---------------------------------------------------------------------------
// 回归保护（本模块自己的不变量，**不是**复原证据；见 AGENTS.md §0.2）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_matches_the_observed_formula() {
        assert_eq!(to_pixel(0.0, 0.0), (500, 300));
        assert_eq!(to_pixel(1.0, 1.0), (520, 320));
        assert_eq!(to_pixel(25.0, 15.0), (1000, 600));
        assert_eq!(to_pixel(-25.0, -15.0), (0, 0));
        // 向零截断（不是 floor）：-0.05*20+500 = 499.0
        assert_eq!(to_pixel(-0.05, 0.0), (499, 300));
    }

    #[test]
    fn rect_is_center_plus_minus_radius() {
        let r = obstacle_rect(0.0, 0.0, 1.0); // 半径 = 20 px
        let want = PixelRect { min_x: 480, min_y: 280, max_x: 520, max_y: 320 };
        assert_eq!(r, want);
        assert_eq!(r.width(), 40);
        assert_eq!(r.height(), 40);
    }

    #[test]
    fn only_two_tags_are_accepted() {
        assert!(is_partial_update_tag(0x28));
        assert!(is_partial_update_tag(0x29));
        assert!(!is_partial_update_tag(0x27));
        assert!(!is_partial_update_tag(0x2a));
    }

    #[test]
    #[should_panic]
    fn wrong_tag_panics_like_the_binary() {
        let mut v = Vec::new();
        obstacle_partial_update_event(&mut v, 0x27, 0.0, 0.0, 1.0);
    }

    fn span_of(addr: u64) -> u64 {
        PLANE_FUNCS
            .iter()
            .chain(PLANE_GDEXT_ENTRIES.iter())
            .find(|f| f.x86 == addr)
            .map(|f| f.x86_span)
            .expect("地址必须在表里")
    }

    #[test]
    fn tail_chain_is_abutting() {
        for w in PLANE_TAIL_CHAIN.windows(2) {
            assert_eq!(w[0] + span_of(w[0]), w[1], "{:#x} 与 {:#x} 应当首尾相接", w[0], w[1]);
        }
    }

    #[test]
    fn address_table_is_sane() {
        for f in PLANE_FUNCS.iter().chain(PLANE_GDEXT_ENTRIES.iter()) {
            assert!((0x37b940..0x94eb70).contains(&f.x86), "{} 不在 .text 里", f.name);
            assert!(f.x86_span >= 0x100, "{} 的跨度太小", f.name);
            assert!(f.arm64_len > 0, "{} 缺 arm64 长度", f.name);
        }
        // 两侧相对次序一致（两条独立构建的旁证）
        let order_ok = |xs: Vec<u64>| xs.windows(2).all(|w| w[0] < w[1]);
        assert!(order_ok(PLANE_FUNCS.iter().map(|f| f.x86).collect()));
        assert!(order_ok(PLANE_FUNCS.iter().map(|f| f.arm64).collect()));
    }

    #[test]
    fn moved_transform_is_the_same_function() {
        // 搬家后 `plane::to_pixel` 是同一项的再导出 ⇒ 任意点的结果必须相同。
        for x in [-25.0, -1.5, 0.0, 0.05, 7.25] {
            assert_eq!(crate::plane::to_pixel(x, x), to_pixel(x, x));
        }
    }

    #[test]
    fn init_children_and_plans() {
        let mut p = GraphwarPlane::new();
        let mut n = 0;
        p.init_children(|_| {
            n += 1;
            n as NodeRef
        });
        assert_eq!(p.function_drawers.len(), FUNCTION_DRAWER_CHILDREN);
        let ((sig, handler), events) = p.ready_plan();
        assert_eq!((sig, handler), (RESIZED_SIGNAL, RESIZED_HANDLER));
        assert_eq!(events, READY_EVENTS.iter().map(|(n, _)| *n).collect::<Vec<_>>());
        assert!(p.ready);
        assert_eq!(p.process_plan(), (OBSTACLE_DRAWER_UPDATES_PER_FRAME, true));
        let r = p.on_obstacle_partial_update(ENTITY_TAG_A, 0.0, 0.0, 0.5);
        assert_eq!(p.partial_updates, vec![r]);
        assert_eq!(r.width(), 20); // 半径 = 0.5*20 = 10 ⇒ 宽 20
    }

    #[test]
    fn plane_never_calls_draw_line() {
        // 有界否定：`draw` 的两个图元槽里没有 `draw_line` 的槽。
        assert_ne!(DRAW_SET_TRANSFORM_SLOT, DRAW_LINE_SLOT);
        assert_ne!(DRAW_RECT_SLOT, DRAW_LINE_SLOT);
        assert_eq!(DRAW_LINE_CALLS_IN_PLANE, 0);
    }
}
