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
//! ⚠️ 本轮**不采信** `tools/class_probe.py` **v1** 的地址：它把**已经是虚地址**的引用点
//! 又过了一遍 `off2va()`，于是整体偏 `+0x1000`（同一行号 `:274:129` 它给 `0x4b7a70`，
//! 按重定位 + capstone 重扫给 `0x4b5df0`；`:218:129` 它给 `0x4b8bd0`，重扫给 `0x4b7a70`）。
//! **2026-11-07 更正**：真因**不是** pkl 过期（`analysis/xrefs.pkl` 与本版 `.so` 逐条一致，
//! 5301/5301），而是上面那个 `off2va` 平移；`tools/class_probe.py` 已修为 v2，
//! 且新增 `tools/loc_xrefs.py`（只读 `.so`）。详见 `docs/16-client.md` §24.1。
//! 所以本文地址**全部**由本轮重扫得出；`fixtures/name_to_addr.tsv` 里那几行 `唯一锚`
//! 与本轮结论一致，可交叉核对。
//!
//! # 2. 它在游戏里是什么
//!
//! `Control` 子类，棋盘本体：**画棋盘框**、**每帧驱动障碍绘制器**、**给障碍/士兵做脏矩形**、
//! **持有 3 个 `FunctionDrawer` 子节点**（函数曲线画在子节点里，不在 `draw` 里）。
//!
//! 一次 `draw`（`0x4b5df0`）只调 **2 个** `CanvasItem` 图元（穷举见 `docs/16-client.md` §23）：
//!
//! | 图元 | GOT 槽 | 调用点 | 实参 | 档位 |
//! |---|---|---|---|---|
//! | `CanvasItem::draw_set_transform` | `0x9bc018` | `0x4b607d` | 平移 / 旋转 0 / 缩放都取自控件尺寸（与 `AxisDrawer::draw` 同构） | **观测** |
//! | `CanvasItem::draw_rect` | `0x9bc028` | `0x4b61ab` | `Rect2(-25,-15,50,30)`、`Color(0,0,0,1)`、`filled=false`、`width=0.2`、`aa=false` | **逐位** |
//!
//! ⇒ **弹道不在这里**：`draw` 里 `draw_line` 的调用点 = **0**。
//!
//! # 2.1 那弹道在哪？（有界穷举的结果，详见 `docs/16-client.md` §23）
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
/// 详见 `docs/16-client.md` §23。
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
pub struct PlaneIntent {
    /// 3 个 `FunctionDrawer` 子节点（[`FUNCTION_DRAWER_CHILDREN`]，`init` 观测）。
    /// 函数曲线由它们画，**不在** `draw` 里（`docs/16-client.md` §23）。
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

impl PlaneIntent {
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

// ---------------------------------------------------------------------------
// T80. gdext 登记存储 / 注册驱动 / `__init::__inner_init`（本轮落码的 6 条宏产物）
// ---------------------------------------------------------------------------
//
// 这 6 条是 `GraphwarPlane` 的**宏产物**：源码层对应物是 `#[derive(GodotClass)]` /
// `#[godot_api]` 属性，不是手写 fn（`tools/function_coverage.py` 的 R3 判法）。本仓把每条
// **机器函数**落成一个 fn：文档给该机器函数的 FDE / `panic::Location` / `.data` 静态槽 /
// 重定位槽 / 调用链，函数体复述它的**可观测动作**（遍历登记表 / 依次调用胶水）。
// 台账：`fixtures/f3_rest_anchors.tsv`；审计：`docs/06` §122。
//
// ## `__registration_storage::STORAGE`（arm64 每类一份 56 B）
//
// arm64 `.symtab`：`GraphwarPlane::__registration_storage::STORAGE` @ arm64 `0x8b42c0`
// （56 B；`tools/arm64_sym.py find __registration_storage` 共 12 份，每类一份）。
// x86 侧同构的 `.data` 静态块（**观测**：每条地址都是本节某条指令的 rip 目标）：
//
// | 偏移 | x86 地址 | 读者 | 语义 |
// |---|---|---|---|
// | +0x00 | `0x9c0e48` | 两条注册驱动的 `lock cmpxchg` / `xchg` | 状态字（原子） |
// | +0x04 | `0x9c0e4c` | `movzx` 读 / `mov byte,1` 写 | poison 字节 |
// | +0x10 | `0x9c0e58` | `__register_methods` | 表 A 指针 |
// | +0x18 | `0x9c0e60` | `__register_methods` | 表 A 长度 |
// | +0x20 | `0x9c0e68` | 读全 8 槽那条 `__init::__inner_init` | 未定 |
// | +0x28 | `0x9c0e70` | `__register_constants` | 表 B 指针 |
// | +0x30 | `0x9c0e78` | `__register_constants` | 表 B 长度 |
//
// ⚠️ `.data` 初值（直接读 ELF）：`[0x9c0e58]` = **8**、`[0x9c0e60]` = **0**、
// `[0x9c0e70]` = **8**、`[0x9c0e78]` = **0** ⇒ 指针初值不是合法指针、长度初值为 0 ⇒ **静态初值下循环不执行**；表在运行期被填充（填充者未知）。
// 表的**填充者未知**（`UNKNOWNS` 里有这一条；缺运行时观测）。
//
// ## 两条注册驱动的名字 = **推断**；体 = 观测
//
// 它们被**同一个 3 指令 thunk** 依次取用：`0x40e620`: `push rax` /
// `call *0x9bb5e0` / `pop rax` / `jmp *0x9bb5e8` ⇒ A 先于 B。
// arm64 `.symtab` 每类恰好两条 `ImplementsGodotApi::__register_methods` /
// `::__register_constants`（各 400 B，methods 地址更低）；Windows 2.0.60 COFF 同名两条
// （`fixtures/windows_name_to_addr.tsv`，`无一对一锚`）。⇒ A = `__register_methods`、
// B = `__register_constants`。**顺序是观测，名字本身仍是推断**（缺 gdext 版本的 trait 声明序）。

/// 表 A 的两个 `.data` 槽：`(指针, 长度)` —— `__register_methods` 读它们。
pub const REGISTER_METHODS_TABLE_SLOTS: (u64, u64) = (0x9c0e58, 0x9c0e60);
/// 表 B 的两个 `.data` 槽：`(指针, 长度)` —— `__register_constants` 读它们。
pub const REGISTER_CONSTANTS_TABLE_SLOTS: (u64, u64) = (0x9c0e70, 0x9c0e78);
/// 依次取用这两条注册驱动的 thunk：`push rax` / `call *A` / `pop rax` / `jmp *B`。
pub const REGISTER_THUNK: u64 = 0x40e620;
/// 两条注册驱动在重定位表里的槽（A = methods，B = constants）。
pub const REGISTER_SLOTS: (u64, u64) = (0x9bb5e0, 0x9bb5e8);

/// 一条 gdext 胶水 / 引擎调用（原版 `call` 指令）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlueCall {
    /// 调用点（vaddr）。
    pub site: u64,
    /// 被调者（vaddr）；间接调用时是重定位槽的 addend。
    pub target: u64,
    /// 间接调用经过的重定位槽（直接 `call rel32` 时为 0）。
    pub slot: u64,
}

/// gdext 胶水的宿主：原版每条 `call` 直接落到引擎 / gdext；本仓把它抽象成一个动作。
pub trait GlueHost {
    /// 执行一条调用。
    fn call(&mut self, c: GlueCall);
}

/// 按地址顺序把该机器函数 FDE 内**全部** `call` 指令交给宿主，返回条数。
///
/// ⚠️ 清单是**普查**（census）：panic 冷臂、drop、全局构造的调用**都在里面**。
/// 「哪几条属于直线路径」本轮没做控制流分析 ⇒ 清单**不是**执行序列（档位：清单=观测，
/// 直线划分=未知）。
pub fn replay_calls(calls: &[GlueCall], host: &mut dyn GlueHost) -> usize {
    for c in calls {
        host.call(*c);
    }
    calls.len()
}

/// 遍历一张「登记项表」并逐项调用，返回条数。
///
/// 原版的表是 `(ptr,len)` 两个静态槽（见本节 `STORAGE` 表）；这里把它作为参数表达 ——
/// 本仓没有「gdext 全局登记存储」这一层（宏产物，见 `AGENTS.md` §4）。
pub fn run_register_table(table: &[fn()]) -> usize {
    for f in table {
        f();
    }
    table.len()
}

/// `0x4b2260` —— `GraphwarPlane::__register_methods`（**名字推断**，**体观测**）
///
/// FDE `0x4b2260..0x4b23d9`（`0x179` = 377 B）；`panic::Location` = `src/graphwar_plane.rs:68:1`。
/// 重定位槽 `0x9bb5e0`（同一 thunk 的 A 半）。
///
/// ```text
/// 0x4b2274  lock cmpxchg dword ptr [rip + 0x50ebcc], ecx  @0x9c0e48 (<bin 0000000000000000>)
/// 0x4b229a  movzx   eax, byte ptr [rip + 0x50ebab]   @0x9c0e4c (<bin 0000000000000000>)
/// 0x4b22a9  mov     r15, qword ptr [rip + 0x50ebb0]  @0x9c0e60 (<bin 0000000000000000>)
/// 0x4b22b5  mov     r12, qword ptr [rip + 0x50eb9c]  @0x9c0e58 (<bin 0800000000000000>)
/// 0x4b22d0  call    qword ptr [r12 + r13]
/// 0x4b22e1  movabs  rax, 0x7fffffffffffffff          imm=0x7fffffffffffffff
/// 0x4b22f9  xchg    dword ptr [rip + 0x50eb49], eax  @0x9c0e48 (<bin 0000000000000000>)
/// 0x4b239a  mov     byte ptr [rip + 0x50eaab], 1     @0x9c0e4c (<bin 0000000000000000>)
/// ```
///
/// 状态机（逐条在上面）：抢状态字 → poison 字节检查 → **遍历表 A**（`call *(%r12,%r13)`，步长 8）
/// → `xchg` 释放（老值 == 2 走唤醒）→ poison 时置字节 1；poison 后再进入走
/// `Result::unwrap()` 的 Err 臂（panic 面 `0x1c3b13`）。
///
/// 落码：只落「逐项遍历登记表」这一步（状态机 / 唤醒 / poison panic 在本仓没有对应层，
/// 见本节文档与 `UNKNOWNS`）。档位：**观测**（体）+ **推断**（名）。
pub fn register_methods(table: &[fn()]) -> usize {
    // 0x4b2260 本函数起点；FDE 0x4b2260..0x4b23d9（377 B）
    // 0x4b22d0  call qword ptr [r12 + r13]   ← 本行模型：逐项调用表 A
    run_register_table(table)
}

/// `0x4b23e0` —— `GraphwarPlane::__register_constants`（**名字推断**，**体观测**）
///
/// 与 [`register_methods`] **逐指令同形**，唯一差别 = 读的是**表 B**
/// （`0x9c0e70` 指针 / `0x9c0e78` 长度）。FDE `0x4b23e0..0x4b2559`
/// （`0x179` = 377 B）；`panic::Location` = `src/graphwar_plane.rs:68:1`；重定位槽 `0x9bb5e8`。
/// 档位：**观测**（体）+ **推断**（名）。
pub fn register_constants(table: &[fn()]) -> usize {
    // 0x4b23e0 本函数起点；FDE 0x4b23e0..0x4b2559（377 B）
    // 与 register_methods 同形：表 B 的指针/长度 = 0x9c0e70 / 0x9c0e78
    run_register_table(table)
}

/// `0x4bc490` —— `GraphwarPlane` 的 `_::__init::__inner_init`（Location `33:10`）
///
/// FDE `0x4bc490..0x4bc6a9`（`0x219` = 537 B）；重定位槽 `0x9ba260`
/// （该类 4 个 `__init::__inner_init` 落在连续 4 槽里，见本节文档头）。
/// 名字 / Location：**观测**（`fixtures/windows_name_to_addr.tsv` 的 `唯一锚`）。
///
/// 调用链（顺序 = 指令顺序，逐条见 [`INNER_INIT_L33_CALLS`]）：
/// * `0x4bc4dc` `call 0x890850`（槽 0x9bb1b8）
/// * `0x4bc536` `call 0x460bb0`（槽 0x9baaf0）
/// * `0x4bc5e7` `call 0x8f5cc0`（槽 0x9ba9c8）
/// * `0x4bc5f2` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4bc62f` `call 0x925430`（槽 0x9ba678）
/// * `0x4bc63e` `call 0x4ffbbb`
/// * `0x4bc64f` `call 0x8f5db0`（槽 0x9ba9d8）
/// * `0x4bc65a` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4bc672` `call 0x925623`（槽 0x9ba668）
/// * `0x4bc678` `call 0x925623`（槽 0x9ba668）
/// * `0x4bc688` `call 0x4da1e0`
/// * `0x4bc68f` `call 0x9254ab`（槽 0x9ba5b8）
/// * `0x4bc698` `call 0x4da440`
/// * `0x4bc69d` `call 0x925623`（槽 0x9ba668）
/// * `0x4bc6a3` `call 0x9254ab`（槽 0x9ba5b8）
///
/// 档位：**观测**（体：FDE + 槽 + 每条 `call`）。
pub fn inner_init_l33(host: &mut dyn GlueHost) -> usize {
    // 0x4bc490 本函数起点；FDE 0x4bc490..0x4bc6a9（537 B）
    replay_calls(INNER_INIT_L33_CALLS, host)
}

pub const INNER_INIT_L33_CALLS: &[GlueCall] = &[
    // 0x4bc4dc: call 0x890850（经槽 0x9bb1b8）
    GlueCall { site: 0x4bc4dc, target: 0x890850, slot: 0x9bb1b8 },
    // 0x4bc536: call 0x460bb0（经槽 0x9baaf0）
    GlueCall { site: 0x4bc536, target: 0x460bb0, slot: 0x9baaf0 },
    // 0x4bc5e7: call 0x8f5cc0（经槽 0x9ba9c8）
    GlueCall { site: 0x4bc5e7, target: 0x8f5cc0, slot: 0x9ba9c8 },
    // 0x4bc5f2: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4bc5f2, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4bc62f: call 0x925430（经槽 0x9ba678）
    GlueCall { site: 0x4bc62f, target: 0x925430, slot: 0x9ba678 },
    // 0x4bc63e: call 0x4ffbbb（直接）
    GlueCall { site: 0x4bc63e, target: 0x4ffbbb, slot: 0x0 },
    // 0x4bc64f: call 0x8f5db0（经槽 0x9ba9d8）
    GlueCall { site: 0x4bc64f, target: 0x8f5db0, slot: 0x9ba9d8 },
    // 0x4bc65a: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4bc65a, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4bc672: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bc672, target: 0x925623, slot: 0x9ba668 },
    // 0x4bc678: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bc678, target: 0x925623, slot: 0x9ba668 },
    // 0x4bc688: call 0x4da1e0（直接）
    GlueCall { site: 0x4bc688, target: 0x4da1e0, slot: 0x0 },
    // 0x4bc68f: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4bc68f, target: 0x9254ab, slot: 0x9ba5b8 },
    // 0x4bc698: call 0x4da440（直接）
    GlueCall { site: 0x4bc698, target: 0x4da440, slot: 0x0 },
    // 0x4bc69d: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bc69d, target: 0x925623, slot: 0x9ba668 },
    // 0x4bc6a3: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4bc6a3, target: 0x9254ab, slot: 0x9ba5b8 },
];

/// `0x4bc6b0` —— `GraphwarPlane` 的 `_::__init::__inner_init`（Location `68:1`）
///
/// FDE `0x4bc6b0..0x4bc856`（`0x1a6` = 422 B）；重定位槽 `0x9ba270`
/// （该类 4 个 `__init::__inner_init` 落在连续 4 槽里，见本节文档头）。
/// 名字 / 归属：**推断**（4 槽连续 + arm64 每类 4 个同名符号；缺 gdext vtable 槽序对齐）。
///
/// 调用链（顺序 = 指令顺序，逐条见 [`INNER_INIT_L68A_CALLS`]）：
/// * `0x4bc71d` `call 0x460bb0`（槽 0x9baaf0）
/// * `0x4bc794` `call 0x8f5cc0`（槽 0x9ba9c8）
/// * `0x4bc79f` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4bc7dc` `call 0x925430`（槽 0x9ba678）
/// * `0x4bc7eb` `call 0x4ffbbb`
/// * `0x4bc7fc` `call 0x8f5db0`（槽 0x9ba9d8）
/// * `0x4bc807` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4bc81f` `call 0x925623`（槽 0x9ba668）
/// * `0x4bc825` `call 0x925623`（槽 0x9ba668）
/// * `0x4bc835` `call 0x4da1e0`
/// * `0x4bc83c` `call 0x9254ab`（槽 0x9ba5b8）
/// * `0x4bc845` `call 0x4da440`
/// * `0x4bc84a` `call 0x925623`（槽 0x9ba668）
/// * `0x4bc850` `call 0x9254ab`（槽 0x9ba5b8）
///
/// 档位：**观测**（体：FDE + 槽 + 每条 `call`）。
pub fn inner_init_l68a(host: &mut dyn GlueHost) -> usize {
    // 0x4bc6b0 本函数起点；FDE 0x4bc6b0..0x4bc856（422 B）
    replay_calls(INNER_INIT_L68A_CALLS, host)
}

pub const INNER_INIT_L68A_CALLS: &[GlueCall] = &[
    // 0x4bc71d: call 0x460bb0（经槽 0x9baaf0）
    GlueCall { site: 0x4bc71d, target: 0x460bb0, slot: 0x9baaf0 },
    // 0x4bc794: call 0x8f5cc0（经槽 0x9ba9c8）
    GlueCall { site: 0x4bc794, target: 0x8f5cc0, slot: 0x9ba9c8 },
    // 0x4bc79f: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4bc79f, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4bc7dc: call 0x925430（经槽 0x9ba678）
    GlueCall { site: 0x4bc7dc, target: 0x925430, slot: 0x9ba678 },
    // 0x4bc7eb: call 0x4ffbbb（直接）
    GlueCall { site: 0x4bc7eb, target: 0x4ffbbb, slot: 0x0 },
    // 0x4bc7fc: call 0x8f5db0（经槽 0x9ba9d8）
    GlueCall { site: 0x4bc7fc, target: 0x8f5db0, slot: 0x9ba9d8 },
    // 0x4bc807: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4bc807, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4bc81f: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bc81f, target: 0x925623, slot: 0x9ba668 },
    // 0x4bc825: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bc825, target: 0x925623, slot: 0x9ba668 },
    // 0x4bc835: call 0x4da1e0（直接）
    GlueCall { site: 0x4bc835, target: 0x4da1e0, slot: 0x0 },
    // 0x4bc83c: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4bc83c, target: 0x9254ab, slot: 0x9ba5b8 },
    // 0x4bc845: call 0x4da440（直接）
    GlueCall { site: 0x4bc845, target: 0x4da440, slot: 0x0 },
    // 0x4bc84a: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bc84a, target: 0x925623, slot: 0x9ba668 },
    // 0x4bc850: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4bc850, target: 0x9254ab, slot: 0x9ba5b8 },
];

/// `0x4bc860` —— `GraphwarPlane` 的 `_::__init::__inner_init`（Location `179:1`）
///
/// FDE `0x4bc860..0x4bcaaf`（`0x24f` = 591 B）；重定位槽 `0x9ba268`
/// （该类 4 个 `__init::__inner_init` 落在连续 4 槽里，见本节文档头）。
/// 名字 / Location：**观测**（`fixtures/windows_name_to_addr.tsv` 的 `唯一锚`）。
///
/// 调用链（顺序 = 指令顺序，逐条见 [`INNER_INIT_L179_CALLS`]）：
/// * `0x4bc92c` `call 0x460bb0`（槽 0x9baaf0）
/// * `0x4bc9ed` `call 0x8f5cc0`（槽 0x9ba9c8）
/// * `0x4bc9f8` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4bca35` `call 0x925430`（槽 0x9ba678）
/// * `0x4bca44` `call 0x4ffbbb`
/// * `0x4bca55` `call 0x8f5db0`（槽 0x9ba9d8）
/// * `0x4bca60` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4bca78` `call 0x925623`（槽 0x9ba668）
/// * `0x4bca7e` `call 0x925623`（槽 0x9ba668）
/// * `0x4bca8e` `call 0x4da1e0`
/// * `0x4bca95` `call 0x9254ab`（槽 0x9ba5b8）
/// * `0x4bca9e` `call 0x4da440`
/// * `0x4bcaa3` `call 0x925623`（槽 0x9ba668）
/// * `0x4bcaa9` `call 0x9254ab`（槽 0x9ba5b8）
///
/// 档位：**观测**（体：FDE + 槽 + 每条 `call`）。
pub fn inner_init_l179(host: &mut dyn GlueHost) -> usize {
    // 0x4bc860 本函数起点；FDE 0x4bc860..0x4bcaaf（591 B）
    replay_calls(INNER_INIT_L179_CALLS, host)
}

pub const INNER_INIT_L179_CALLS: &[GlueCall] = &[
    // 0x4bc92c: call 0x460bb0（经槽 0x9baaf0）
    GlueCall { site: 0x4bc92c, target: 0x460bb0, slot: 0x9baaf0 },
    // 0x4bc9ed: call 0x8f5cc0（经槽 0x9ba9c8）
    GlueCall { site: 0x4bc9ed, target: 0x8f5cc0, slot: 0x9ba9c8 },
    // 0x4bc9f8: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4bc9f8, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4bca35: call 0x925430（经槽 0x9ba678）
    GlueCall { site: 0x4bca35, target: 0x925430, slot: 0x9ba678 },
    // 0x4bca44: call 0x4ffbbb（直接）
    GlueCall { site: 0x4bca44, target: 0x4ffbbb, slot: 0x0 },
    // 0x4bca55: call 0x8f5db0（经槽 0x9ba9d8）
    GlueCall { site: 0x4bca55, target: 0x8f5db0, slot: 0x9ba9d8 },
    // 0x4bca60: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4bca60, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4bca78: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bca78, target: 0x925623, slot: 0x9ba668 },
    // 0x4bca7e: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bca7e, target: 0x925623, slot: 0x9ba668 },
    // 0x4bca8e: call 0x4da1e0（直接）
    GlueCall { site: 0x4bca8e, target: 0x4da1e0, slot: 0x0 },
    // 0x4bca95: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4bca95, target: 0x9254ab, slot: 0x9ba5b8 },
    // 0x4bca9e: call 0x4da440（直接）
    GlueCall { site: 0x4bca9e, target: 0x4da440, slot: 0x0 },
    // 0x4bcaa3: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bcaa3, target: 0x925623, slot: 0x9ba668 },
    // 0x4bcaa9: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4bcaa9, target: 0x9254ab, slot: 0x9ba5b8 },
];

/// `0x4bcab0` —— `GraphwarPlane` 的 `_::__init::__inner_init`（Location `68:1`）
///
/// FDE `0x4bcab0..0x4bcc58`（`0x1a8` = 424 B）；重定位槽 `0x9ba258`
/// （该类 4 个 `__init::__inner_init` 落在连续 4 槽里，见本节文档头）。
/// 名字 / 归属：**推断**（4 槽连续 + arm64 每类 4 个同名符号；缺 gdext vtable 槽序对齐）。
///
/// 调用链（顺序 = 指令顺序，逐条见 [`INNER_INIT_L68B_CALLS`]）：
/// * `0x4bcb0c` `call 0x8c5120`（槽 0x9baaf8）
/// * `0x4bcb45` `call 0x8c5120`（槽 0x9baaf8）
/// * `0x4bcb9f` `call 0x8f5cc0`（槽 0x9ba9c8）
/// * `0x4bcbaa` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4bcbf0` `call 0x925430`（槽 0x9ba678）
/// * `0x4bcbff` `call 0x8f5db0`（槽 0x9ba9d8）
/// * `0x4bcc07` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4bcc21` `call 0x925623`（槽 0x9ba668）
/// * `0x4bcc27` `call 0x925623`（槽 0x9ba668）
/// * `0x4bcc37` `call 0x4da330`
/// * `0x4bcc3e` `call 0x9254ab`（槽 0x9ba5b8）
/// * `0x4bcc47` `call 0x4da440`
/// * `0x4bcc4c` `call 0x925623`（槽 0x9ba668）
/// * `0x4bcc52` `call 0x9254ab`（槽 0x9ba5b8）
///
/// 档位：**观测**（体：FDE + 槽 + 每条 `call`）。
pub fn inner_init_l68b(host: &mut dyn GlueHost) -> usize {
    // 0x4bcab0 本函数起点；FDE 0x4bcab0..0x4bcc58（424 B）
    replay_calls(INNER_INIT_L68B_CALLS, host)
}

pub const INNER_INIT_L68B_CALLS: &[GlueCall] = &[
    // 0x4bcb0c: call 0x8c5120（经槽 0x9baaf8）
    GlueCall { site: 0x4bcb0c, target: 0x8c5120, slot: 0x9baaf8 },
    // 0x4bcb45: call 0x8c5120（经槽 0x9baaf8）
    GlueCall { site: 0x4bcb45, target: 0x8c5120, slot: 0x9baaf8 },
    // 0x4bcb9f: call 0x8f5cc0（经槽 0x9ba9c8）
    GlueCall { site: 0x4bcb9f, target: 0x8f5cc0, slot: 0x9ba9c8 },
    // 0x4bcbaa: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4bcbaa, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4bcbf0: call 0x925430（经槽 0x9ba678）
    GlueCall { site: 0x4bcbf0, target: 0x925430, slot: 0x9ba678 },
    // 0x4bcbff: call 0x8f5db0（经槽 0x9ba9d8）
    GlueCall { site: 0x4bcbff, target: 0x8f5db0, slot: 0x9ba9d8 },
    // 0x4bcc07: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4bcc07, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4bcc21: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bcc21, target: 0x925623, slot: 0x9ba668 },
    // 0x4bcc27: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bcc27, target: 0x925623, slot: 0x9ba668 },
    // 0x4bcc37: call 0x4da330（直接）
    GlueCall { site: 0x4bcc37, target: 0x4da330, slot: 0x0 },
    // 0x4bcc3e: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4bcc3e, target: 0x9254ab, slot: 0x9ba5b8 },
    // 0x4bcc47: call 0x4da440（直接）
    GlueCall { site: 0x4bcc47, target: 0x4da440, slot: 0x0 },
    // 0x4bcc4c: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4bcc4c, target: 0x925623, slot: 0x9ba668 },
    // 0x4bcc52: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4bcc52, target: 0x9254ab, slot: 0x9ba5b8 },
];



#[cfg(test)]
mod tests {
    use super::*;

    // ---- T80：gdext 宏产物落码的自校验（期望值全部来自反汇编）-----------------

    /// 记录宿主：把收到的调用按顺序存下来。
    struct Recorder(Vec<GlueCall>);
    impl GlueHost for Recorder {
        fn call(&mut self, c: GlueCall) {
            self.0.push(c);
        }
    }

    /// `__register_methods` / `__register_constants` 同形：只落「遍历登记表」。
    #[test]
    fn register_tables_are_run_entry_by_entry() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static HITS: AtomicUsize = AtomicUsize::new(0);
        fn t1() {
            HITS.fetch_add(1, Ordering::SeqCst);
        }
        fn t2() {
            HITS.fetch_add(10, Ordering::SeqCst);
        }
        let table: [fn(); 2] = [t1, t2];
        assert_eq!(run_register_table(&table), 2);
        assert_eq!(HITS.load(Ordering::SeqCst), 11);
        assert_eq!(run_register_table(&[]), 0);
    }

    /// 表槽 / thunk / 重定位槽：每个数都是本节某条指令的 rip 目标。
    #[test]
    fn storage_slots_and_thunk_are_the_disassembly_constants() {
        assert_eq!(REGISTER_THUNK & 0xf, 0, "thunk 表 16 字节对齐");
        assert_eq!(REGISTER_SLOTS.1 - REGISTER_SLOTS.0, 8, "A/B 两个槽相邻");
        assert_eq!(REGISTER_METHODS_TABLE_SLOTS.1 - REGISTER_METHODS_TABLE_SLOTS.0, 8);
        assert_eq!(REGISTER_CONSTANTS_TABLE_SLOTS.1 - REGISTER_CONSTANTS_TABLE_SLOTS.0, 8);
    }

    /// 每条 `__init::__inner_init` 的调用**普查**条数（从各自指令流数出来的）。
    #[test]
    fn inner_init_call_census_matches_the_disassembly() {
        assert_eq!(INNER_INIT_L33_CALLS.len(), 15, "l33 的 call 普查条数");
        assert_eq!(INNER_INIT_L68A_CALLS.len(), 14, "l68a 的 call 普查条数");
        assert_eq!(INNER_INIT_L179_CALLS.len(), 14, "l179 的 call 普查条数");
        assert_eq!(INNER_INIT_L68B_CALLS.len(), 14, "l68b 的 call 普查条数");
    }

    /// 普查里的每条调用点必须落在**该函数自己的 FDE** 内。
    #[test]
    fn inner_init_call_sites_lie_inside_their_own_fde() {
        for c in INNER_INIT_L33_CALLS {
            assert!((0x4bc490..0x4bc6a9).contains(&c.site), "l33: {:#x} 不在 FDE", c.site);
        }
        for c in INNER_INIT_L68A_CALLS {
            assert!((0x4bc6b0..0x4bc856).contains(&c.site), "l68a: {:#x} 不在 FDE", c.site);
        }
        for c in INNER_INIT_L179_CALLS {
            assert!((0x4bc860..0x4bcaaf).contains(&c.site), "l179: {:#x} 不在 FDE", c.site);
        }
        for c in INNER_INIT_L68B_CALLS {
            assert!((0x4bcab0..0x4bcc58).contains(&c.site), "l68b: {:#x} 不在 FDE", c.site);
        }
    }

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
        let mut p = PlaneIntent::new();
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


// ===========================================================================
// 原版方法体：原版 `GraphwarPlane` 的 gdext 节点类（`Control` 子类）（T57 从 `src/graphwar_plane.rs` 搬回）
// ===========================================================================
#[cfg(feature = "gdext-bindings")]
mod node_class {
//! [T57 已搬回原版模块] 这里是**原版 原版 `GraphwarPlane` 的 gdext 节点类（`Control` 子类）** 的方法体。
//!
//! 2026-11-27 从 `src/graphwar_plane.rs` 搬进 `src/graphwar_plane.rs`：原版把 `#[derive(GodotClass)]`
//! 的 struct 与方法体**写在同一个 `graphwar_plane.rs` 里**，证据：
//! `panic::Location` 证据：`ready 0x4b7a70` → `src/graphwar_plane.rs:218:129`、`process 0x4b83f0` → `:254:133`、`draw 0x4b5df0` → `:110:95`；`init 0x4b7380` 的唯一 Location 落 `src/obstacle_drawer.rs:37:97` = 内联进来的 `ObstacleDrawer::new` 的锚（不是本模块的锚，归属靠 funcstart + arm64 `.symtab` 名字双证据）。
//!
//! **本仓唯一的结构差异**：原版整个 crate 都是 gdext 扩展（不需要 `cfg`）；本仓 `cargo test`
//! （不带 `gdext-bindings`）必须能编，所以这一层挂在
//! `#[cfg(feature = "gdext-bindings")] mod node_class` 下，由 `pub use node_class::*;` 把对外路径
//! 抬平 ⇒ `crate::graphwar_plane::<类名>` 这类路径与原版一致。
//!
//! ---
//! 以下是搬运前的原始说明（保留：里面逐条写着与原版的差别与证据档）。
//!
//!
//! # 为什么会有这一份
//!
//! 原版的 `GraphwarPlane` 是**棋盘本体**（`docs/16-client.md` §23）：它自己只画**一个空心黑框**
//! （`draw_set_transform` ×1 + `draw_rect` ×1，`Rect2(-25,-15,50,30)`、黑、宽 `0.2`、
//! **不填充**），曲线**不在**它里面画——它 `init` 里建 **3 个 `FunctionDrawer` 子节点**，
//! 弹道是 `Line2D` 子节点，由那些子节点画（§26.4 的**有界否定**：`draw_line` 在本类里 0 次）。
//!
//! `src/graphwar_plane.rs` 已经把「建几个子节点 / 脏矩形怎么算 / 每帧刷几次 / 画哪个框」
//! 复原成纯逻辑（`GraphwarPlane::init_children` / `ready_plan` / `process_plan` /
//! `obstacle_partial_update_event` / `BOARD_*`），但按约定**不碰引擎**。本文件是那层壳：
//!
//! ```text
//! src/graphwar_plane.rs                 纯逻辑：子节点意图 / 脏矩形 / 棋盘框常量 / 信号名
//! src/graphwar_plane.rs ← 本文件：真的 add_child、真的 draw_rect、真的接 resized
//! ```
//!
//! # 本切片做四件本类自己的事（`FunctionDrawer` 已由 §40 的节点类补上）
//!
//! 1. **建 3 个子节点**（`init` 观测）——它们现在是**真的 `FunctionDrawer`**
//!    （`src/function_drawer.rs`，`docs/16-client.md` §40）；
//! 2. **脏矩形**：`obstacle_partial_update_event` 的纯数据版 + 每帧冲刷；
//! 3. **画棋盘框**：`draw_set_transform` + `draw_rect`，常量全取 `graphwar_plane::BOARD_*`；
//! 4. **喂函数曲线**（`docs/16-client.md` §41）：`draw` 末尾按
//!    "`get_active_functions_on_tick`（`0x456df0`）给的 id → 每 tick 露 30 个点"往第 0 个
//!    `FunctionDrawer` 里塞点（`update_function`，`0x4b69ec`）。点的**来源**
//!    （`GraphwarFunctionManager`，原版由 `set_function_points` `0x456810` 写）还没复原
//!    ⇒ 现在由调用方经 [`GraphwarPlane::set_function_points`] 注入。
//!
//! 障碍 / 士兵**不在本文件**：它们分别归 `ObstacleDrawer` / `EntityDrawer`（都没有节点类）。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 子节点 | `init` 里 `Gd::default_instance::<FunctionDrawer>()` ×3 + `Node::add_child` ×3 | 同样 3 个 `FunctionDrawer`（`FunctionDrawer::new_alloc()`）——**不再是 `Control` 占位**（§42） | **观测**（个数 / 类名） |
//! | 子节点的名字 | 没 `set_name` ⇒ 引擎自动名 `@FunctionDrawer@N`（`docs/16` §2.1.10 的 dump 里同族的 `@AxisDrawer@N` / `@EntityDrawer@N` 都是自动名） | 也没 `set_name` ⇒ 自动名 `@FunctionDrawer@N`。**运行时观测**的值是 `@FunctionDrawer@N`（`docs/16` §38.2） | 原版**观测**；本类**不伪造**那个名字 |
//! | 建子节点的时机 | `init` | **`ready`**（`#[func] build_children()` 也能手动调）。理由：gdext 的 `init` 里 `Base` 还没构造完（`InitState::Uninitialized`），`base_mut()` 用不了 | 原版**观测**；时机差是**gdext 约束** |
//! | `draw` 的图元 | `draw_set_transform` ×1（槽 `0x9bc018`）+ `draw_rect` ×1（槽 `0x9bc028`）；`draw_line` **0** 次 | 同 | **逐位**（常量槽）/ **观测**（调用点） |
//! | `draw_set_transform` 的**实参** | 只确证了 GOT 槽，**实参没读到** | 用 `to_pixel` 那套换算当默认值（`PIXEL_OFFSET` / `PX_PER_UNIT`），两个导出属性可改 | 原版**未知**；默认值是**我方选择** |
//! | `ready` 的 3 个订阅 | `add_obstacle_event` / `destroy_obstacle_event` / `clear_obstacles_event`（`READY_EVENTS`） | **没订**（要 `GraphwarBackend`）；只把信号名与事件名**从纯逻辑里取出来用** | 名字与顺序**观测**；订阅**未做** |
//! | `resized` → `on_resized` | 信号名 `"resized"`、处理者名 `"on_resized"`（`.rodata` 两条字面量） | 真连（信号名与处理者名都取 `graphwar_plane::RESIZED_*`，不另写字符串） | **观测** |
//! | `process` 每帧 | `ObstacleDrawer::update` ×2 + 读一次队色（`process_plan` ⇒ `(2, true)`） | 用 `process_plan()` 的 `2` 当"本帧刷几次"并计数；**没有** `ObstacleDrawer` 节点、**没有**队色 | 数字**观测**；刷的是空操作 |
//! | `on_resized` 里那 3 次 `Control::set_*` | 走 GOT 槽，**方法名未解**（`UNKNOWNS` 第 3 条） | 只 `queue_redraw()` | 原版**未知** |
//! | `obstacle_partial_update_event` 的坏 tag | **`panic!()`**（消息是 `.rodata` 的 `"explicit panic"`） | **先拦**：`is_partial_update_tag` 为假就 `godot_warn!` + 返回空数组 | 原版**观测**；本类**故意不同**（panic 穿过 FFI 会 abort 整个进程） |
//! | `clear_state` / `update_entities` | 两个方法存在（`0x4bded0` / `0x4be270`），语义未解 | **不实现**（`UNKNOWNS` 第 5 条：`clear_state` 引用 `EntityDrawer` / `EffectDrawer` 类名的用途未知） | **未知** |
//! | 曲线的点从哪来 | `GraphwarFunctionManager` 里存的采样点（bucket+0x58/+0x60），由 `set_function_points`（`0x456810`）写入 | **本类自己存一份**（`FunctionEntry`）+ 调用方注入；那个模块还没有 `src/` 文件 | 原版**观测**；存储是**替身**（§41.4） |
//! | 曲线的上场速率 | `new_len = min(len, trunc(30*(fractional_tick - active_tick)))`，`30.0` = `.rodata 0x19ef90` | 同（[`revealed_points`]） | **逐位**（常量）/ **观测**（算式） |
//! | tick 从哪来 | 每帧从 `GraphwarTickHandler` 读：u32 tick（`0x4b6317` 的 `mov 0x58(%rbp),%ebx`）+ `get_fractional_tick`（`0x4b631a` → `0x519530`） | **本类自己持有一个 [`GraphwarTickHandler`]**，调它的 `get_fractional_tick()`；[`GraphwarPlane::set_fractional_tick`] 只是把这台虚拟时钟摆到指定读数的**接线口** | 算式**观测**（`docs/16-client.md` §48.1）；时钟摆位是**我方接线** |
//! | 淡出 | `update_function_fade`（`0x4b6a0e`），alpha = `get_function_fade_on_tick` 的返回 | **没接**：那个函数**没有 x86 落点**（走 GOT 槽 `0x9bbd08`），算式未知 | **未知** |
//! | `update_transform`（位置/宽度/缩放） | `0x4b6147`：`x = w/2`、`y = h/2`、`scale = w/50`（f32 50.0 = `.rodata 0x1b9fc8`）、`flip` = `get_team_overview` 的位组合 | **接上了**（[`GraphwarPlane::apply_update_transform`]）：最终两个分量 = `flip ? (-s,-s) : (s,-s)`，与 `draw_set_transform` 同一套 | **观测**（§48.2 逐指令） |
//! | 棋盘内部的底色 | **不是平面画的**：`GraphwarPlane::draw` 只有空心黑框；白底在 `AxisDrawer::draw`（`0x4e069f`–`0x4e06ca`）的 `draw_rect(filled=true, (1,1,1,1))` | [`GraphwarPlane::board_fill`] 导出属性（**默认关**）：打开就照 AxisDrawer 的实参补画那块白底 | 原版**观测**；由平面代画是**我方接线**（`AxisDrawer` 节点还没实现 `draw`） |
//! | 每帧重画的来源 | `process` 里没有 `queue_redraw` 的直接调用（未解） | `_process` 里"**还有该喂没喂的点**（应喂 > 已喂）就 `queue_redraw`"（§43 修的：原来按"应喂 < 全长"判，最后一小段永远喂不进去） | **我方选择** |
//!
//! ⇒ 这个类证明的是「**子节点意图 / 脏矩形 / 棋盘框常量 / 喂点路径**能真的落到引擎上」，
//! **不是**"棋盘已经能玩"：曲线现在真的会往 `Line2D` 里长点（headless 实测点数 60→90→100），
//! 但①点的生产者没复原（要外部注入）、②`update_transform` 没接（位置/粗细还不对）、
//! ③障碍与士兵也没有节点。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/draw_classes_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! GraphwarPlane : Control
//!   @export board_scale : float = 20.0    # 棋盘单位 → 像素（PX_PER_UNIT = 1000/50）
//!   @export flip_y      : bool  = true    # 棋盘 y 向上、画布 y 向下 ⇒ 默认取负（原版**永远**取负）
//!   @export mirror_x    : bool  = false   # 原版的 `flip`（镜像 x）= get_team_overview 的位组合
//!   @export board_fill  : bool  = false   # 打开就补画 AxisDrawer 的白色棋盘底（原版在 0x4e069f）
//!
//!   build_children() -> int                       # 建 3 个 FunctionDrawer 子节点（幂等），返回个数
//!   function_drawer_count() -> int                # 纯逻辑记下的子节点数（FUNCTION_DRAWER_CHILDREN）
//!   to_pixel(x, y) -> Vector2i                    # graphwar_plane::to_pixel 的搬运
//!   on_obstacle_partial_update(tag, x, y, size) -> PackedInt32Array
//!                                                 # [min_x, min_y, max_x, max_y]；坏 tag 返回空
//!   partial_update_count() -> int
//!   drain_partial_updates() -> int                # 冲刷队列并返回冲掉几个
//!   ready_event_names() -> PackedStringArray      # 原版 ready 会订的 3 个事件（本类没订）
//!   draw_call_count() / drawn_rect_total() -> int
//!   process_call_count() / obstacle_update_total() / resized_call_count() / ready_call_count() -> int
//!
//!   # 曲线喂点（docs/16-client.md §41）
//!   set_fractional_tick(tick) -> float            # 原版每帧自己从 tick handler 读
//!   fractional_tick() -> float
//!   set_function_points(id, points, active_tick, color) -> int   # points 是扁平 f64 [x0,y0,x1,y1,…]
//!   clear_function_points() -> int
//!   active_function_ids() -> PackedInt32Array     # = get_active_functions_on_tick(0x456df0) 的返回集
//!   feed_functions() -> int                       # = 原版 draw 尾部那段循环；返回本帧追加几点
//!   function_count() / function_point_count(id) -> int
//!   function_write_total() / function_point_total() / removed_function_total() / reveal_redraw_count() -> int
//! ```
//!
//! # 写这种类必须知道的几条（`docs/16-client.md` §34.4 实测过，这里只记与本类有关的）
//!
//! 1. **`Gd<Node>` 不保活**：子节点归父节点所有，`add_child` 之后把 `Gd` 丢掉是安全的；
//!    反过来"父节点被回收而手里还留着 `Gd`"就成了野句柄（碰一下就 `1337` + 卡死）。
//!    ⇒ 本类**只**把子节点的 `instance_id()`（`u64`，纯逻辑的 `NodeRef`）记进
//!    [`graphwar_plane::GraphwarPlane::function_drawers`]，一个 `Gd` 都不留。
//! 2. **树外 `add_child` 只记父子关系**：`--script` 模式的 `_initialize()` 跑在根节点进树之前，
//!    那一刻子节点的 `_ready` / `_process` 不会来。本类不依赖它们（`build_children` 自己就完事）。
//! 3. **`resized` 只在 `ready` 里连** ⇒ 树外 `set_size` 不会触发 `on_resized`（连都还没连）。
//! 4. **不用写 `set_process(true)`**：实现了 `_process` 的扩展类由 Godot 4.7 自己开。
//! 5. **`base_mut()` 是独占借用** ⇒ `add_child` 那段里不能同时碰 `self.plane`。本类的做法：
//!    先把父节点取成**局部** `Gd<Node>`（`self.to_gd().upcast()`），再把
//!    `self.plane.init_children(|_| …)` 的闭包只捕获那个局部的父节点。



use godot::classes::{Control, IControl, Node};
use godot::prelude::*;

use crate::function_drawer::FunctionDrawer;
use crate::obstacle_drawer::ObstacleDrawer;
use crate::graphwar_color_manager::{GraphwarColor, TEAM1_COLORS, TEAM2_COLORS, TEAM_POOL_LEN};
use crate::graphwar_plane::{self as gp, PixelRect};
use crate::graphwar_tick_handler::{GraphwarTickHandler, NANOS_PER_SECOND, NANOS_PER_TICK};

/// 默认的棋盘缩放：`PX_PER_UNIT = CANVAS_W / BOARD_W = 1000 / 50 = 20`。
///
/// `PX_PER_UNIT` 本身是**观测**（`to_pixel` 的换算），但把它当 `draw_set_transform` 的
/// `scale` 用是**我方选择**（原版那次变换的实参没读到，见文件头对照表）。
const DEFAULT_BOARD_SCALE: f64 = gp::PX_PER_UNIT;

// ---------------------------------------------------------------------------
// 曲线喂点：常量与调用点（**都带地址**，见 `docs/16-client.md` §41）
// ---------------------------------------------------------------------------

/// `GraphwarFunctionManager::get_active_functions_on_tick`（x86 `0x456df0`）在整份 `.text` 里的
/// **唯一**调用点（`scan.py --callers` 扫 0x37b940 起 0x5d36f0 字节的全部 `call rel32`）：
/// `0x4b6339`，落在 `GraphwarPlane::draw`（`0x4b5df0`）体内。
///
/// 它就是任务书里"查 `0x456df0` 到底画什么"的答案：**它不画任何东西**——`0x456df0` 是
/// `GraphwarFunctionManager` 上的一个**取值器**，签名 `fn(&self, tick: u32) -> Vec<u32>`
/// （`rdi` = sret 输出槽、`rsi` = self、`edx` = tick；见 `0x456df8`–`0x456e8f`），
/// 谓词逐条翻在 [`is_active_function`] 里。画曲线的是它返回的那串 id 走进
/// `FunctionDrawer::update_function`（`0x498c70`）。
pub const ACTIVE_FUNCTIONS_CALL_SITE: u32 = 0x004b_6339;

/// `FunctionDrawer::update_function`（x86 `0x498c70`）的**唯一**调用点：`0x4b69ec`。
///
/// 实参（`0x4b69d4`–`0x4b69e9`）：`rdi` = 绘制器、`esi` = player id、`rdx` = 点数组指针、
/// `rcx` = 点数组长度、`r8` = `new_len`、`r9d` = 打包颜色。
/// ⇒ 原版里**喂点的就是 `GraphwarPlane::draw`**，不是别的模块。
pub const UPDATE_FUNCTION_CALL_SITE: u32 = 0x004b_69ec;

/// `FunctionDrawer::remove_function`（x86 `0x498b70`）的**唯一**调用点：`0x4b6418`
/// （同一个 `draw` 里，"不在场了"的那几条要删掉）。
pub const REMOVE_FUNCTION_CALL_SITE: u32 = 0x004b_6418;

/// `FunctionDrawer::update_transform`（`0x499050`）的**唯一**调用点：`0x4b6147`
/// （在 `GraphwarPlane::draw` 里；`callers 0x499050` = 1，**不在** `ready` `0x4b7a70` 里）。
///
/// 三个实参照 `docs/16-client.md` §48.2；本类由 [`GraphwarPlane::apply_update_transform`] 落地。
pub const UPDATE_TRANSFORM_CALL_SITE: u32 = 0x004b_6147;

// ---------------------------------------------------------------------------
// 棋盘变换：`update_transform`（0x4b6147）与 `draw_set_transform`（0x4b607d）的实参
// ---------------------------------------------------------------------------

/// 棋盘宽（单位）：`draw_rect` 的 `Rect2(-25,-15,50,30)`（`.rodata 0x19cc20`）第 3 个分量，
/// 也是 `divss` 的那个除数（f32 **50.0** = `.rodata 0x1b9fc8`，`0x4b6104` / `0x4e0639`）。
pub const BOARD_WIDTH_UNITS: f32 = 50.0;

/// 棋盘高（单位）：同一个 `Rect2` 的第 4 个分量（f32 **30.0** = `.rodata 0x1ba12c`，
/// 与 `0.5` 挤在同一个 8 字节池 `0x1ba128` 里）。
pub const BOARD_HEIGHT_UNITS: f32 = 30.0;

/// f32 **0.5**（`.rodata 0x1ba128` 的低 4 字节）：`0x4b6031`/`0x4b603f`/`0x4b604f` 的 `mulss`，
/// 也出现在 `AxisDrawer::draw`（`0x4e064b`）——两处都是“半宽半高”。
pub const HALF: f32 = 0.5;

/// `get_team_overview`（`0x4be6e0`）→ `flip`：**逐位**翻自 `0x4b600d`–`0x4b6029`
/// （`draw_set_transform` 的 x 分量）与 `0x4b610c`–`0x4b6129`（`update_transform` 的 `esi`）——
/// 两处是同一个表达式：
///
/// ```text
/// 4b6012  cmp  ebp,0x1000000 ; setae cl      ; cl = (flags >= 2^24)
/// 4b601b  test ebp,0x10000   ; sete  dl      ; dl = !(flags & 2^16)
/// 4b6024  and  dl,cl                         ; A  = (flags >= 2^24) && !bit16
/// 4b600d  and  eax,0x101                     ; eax = flags & 0x101
/// 4b605c  cmp  eax,0x100                     ; B  = ((flags & 0x101) == 0x100)
/// 4b6129  or   dl,al                         ; flip = A | B
/// ```
pub fn flip_from_overview_bits(flags: u32) -> bool {
    let a = flags >= 0x0100_0000 && (flags & 0x0001_0000) == 0;
    let b = (flags & 0x0000_0101) == 0x0000_0100;
    a || b
}

/// `update_transform`（`0x4b6147`）/ `draw_set_transform`（`0x4b607d`）的实参：**位置 + 一个标量**。
///
/// ```text
/// 4b60c9  call get_size()            ; xmm0 = w, xmm1 = h（槽 0x9bb000 = Control::get_size）
/// 4b60d4  movss [rsp+8],xmm0         ; w
/// 4b60da  movss [rsp+0x10],xmm1      ; h
/// 4b60f0  movss xmm1,[rsp+0x10] ; 4b60f6 mulss xmm1,0.5   ; y = h*0.5  （arg2）
/// 4b60fa  movss xmm2,[rsp+8]    ; 4b6104 divss xmm2,50.0  ; s = w/50   （arg3，**f32 标量**）
/// 4b6100  mulss xmm0,xmm2(0.5)                          ; x = w*0.5  （arg1）
/// ```
///
/// ⇒ 返回 `((x, y), s)`。**`s` 是标量**：`update_transform` 内部用 `pxor` 与 `.rodata
/// 0x19e880`（四个 `-0.0`）把它的符号位翻出来当 y 分量（`0x499088`/`0x499092` 存两份、
/// `0x499232`–`0x499241` 各取一份喂 `set_scale` 的两个 f32）⇒ `set_scale` 实际收到
/// `flip ? (-s,-s) : (s,-s)`，**不是** `Vector2(w/50, 0)`。旧文（§42.5 第 4 条）说的
/// “自相矛盾”就是把它当成了 `Vector2` 的 y 分量。
pub fn board_transform_args(size: Vector2) -> ((f32, f32), f32) {
    let (w, h) = (size.x, size.y);
    let s = if w > 0.0 { w / BOARD_WIDTH_UNITS } else { 0.0 };
    ((w * HALF, h * HALF), s)
}

/// 由标量 `s`、原版 `flip`、我方 `flip_y` 得到**最终**的两个分量。
///
/// 逐指令（`0x4b6001`–`0x4b6063`）：
///
/// ```text
/// xmm4 = -s                                   ; 0x4b6001 的 pxor，**永远取负**
/// xmm1 = flip ? -s : +s                       ; 0x4b6029 的 test dl（flip = A|B）
/// xmm3 = ((flags&0x101)==0x100) ? -s : xmm1   ; 0x4b6059–0x4b6063
/// ⇒ scale = (xmm3, xmm4) = (flip ? -s : s, **-s**)
/// ```
///
/// ⇒ **x 分量受 `flip` 控制（镜像），y 分量永远取负**（棋盘 y 向上、画布 y 向下）。
/// 原版没有"y 是否取负"的开关；本类的 `flip_y` 是**我方**属性（默认 `true` = 原版行为），
/// 只为让复现工具能关掉它。
pub fn board_scale_xy(s: f32, flip: bool, flip_y: bool) -> (f32, f32) {
    let sx = if flip { -s } else { s };
    let sy = if flip_y { -s } else { s };
    (sx, sy)
}

/// 棋盘内部的白底：**原版在 `AxisDrawer::draw` 里画**（`0x4e069f`–`0x4e06ca`）：
///
/// ```text
/// 4e069f  movaps xmm0,[0x19cc20]   ; Rect2(-25,-15,50,30)
/// 4e06ab  lea    rdx,[0x19ed00]    ; Color(1,1,1,1)
/// 4e06b7  movss  xmm0,[0x1b9d08]   ; width = -1.0f32
/// 4e06c2  mov    ecx,1             ; filled = true
/// 4e06c7  xor    r8d,r8d           ; antialiased = false
/// 4e06ca  call   *0x9bc028         ; CanvasItem::draw_rect
/// ```
///
/// 平面自己**不**画它：`0x4b61ab` 那次是 `filled=false` 的空心黑框（见 `graphwar_plane::BOARD_*`）。
pub const BOARD_FILL_COLOR: (f32, f32, f32, f32) = (1.0, 1.0, 1.0, 1.0);

/// `AxisDrawer::draw` 那次 `draw_rect` 的宽度实参：f32 `-1.0`（`.rodata 0x1b9d08`）。
pub const BOARD_FILL_WIDTH: f32 = -1.0;

/// 每 tick "露"出多少个点（**逐位**）：`.rodata 0x19ef90` 的 8 字节是
/// `00 00 00 00 00 00 3e 40` = f64 **30.0**，调用点 `0x4b67ba` 的 `mulsd [0x19ef90]`：
///
/// ```text
/// 4b67a0  mov  -0x4(%r12),%eax        ; active_tick（u32，value+0x74）
/// 4b67a8  cvtsi2sd %rax,%xmm0
/// 4b67ad  movsd  0x130(%rsp),%xmm1    ; get_fractional_tick() 的返回值
/// 4b67b6  subsd  %xmm0,%xmm1          ; tick - active_tick
/// 4b67ba  mulsd  0x19ef90,%xmm1       ; × 30.0
/// 4b67c2  cvttsd2si …                 ; 饱和转 u64（负数 → 0）
/// 4b6812  cmp %r13,%r12 / cmovb       ; new_len = min(points.len(), 上面那个数)
/// ```
///
/// ⇒ 曲线是**逐 tick 长出来**的：每 tick 补 30 个点。
pub const REVEAL_POINTS_PER_TICK: f64 = 30.0;

/// 一条曲线"还在场"的尾巴（**观测**）：`0x3be60b` 的 `add $0x708,%edi`（= **1800**）加在
/// `points.len()` 上，再与 `30*(tick-active_tick)` 做**无符号**比较（`0x3be611` 的 `jae`）：
///
/// ```text
/// 3be5f6  mov %eax,%edx ; sub -0x4(%rcx),%edx   ; edx = tick - active_tick（u32）
/// 3be5fb  jbe  skip                             ; tick <= active_tick ⇒ 不在场
/// 3be5fd  mov -0x18(%rcx),%rdi                  ; points.len()（value+0x60）
/// 3be601  lea (%rdx,%rdx,1),%r8d / shl $5,%edx / sub %r8d,%edx   ; edx = 30*d
/// 3be60b  add $0x708,%edi                       ; points.len() + 1800
/// 3be611  cmp %edi,%edx ; jae skip              ; 30*d >= len+1800 ⇒ 不在场
/// ```
///
/// ⇒ 揭示完之后再留 **1800/30 = 60 tick**（淡出窗口，`get_function_fade_on_tick` 的分子，
/// 那个函数**没定位**，见 §41.5）。
pub const ACTIVE_TAIL_POINTS: usize = 1800;

/// 本帧该"露"几个点。逐条翻自 `0x4b67a0`–`0x4b6815`（见 [`REVEAL_POINTS_PER_TICK`]）。
///
/// 负数（tick < active_tick）⇒ 0；超过总点数 ⇒ 总点数。
/// 原版是 `cvttsd2si` + 一段饱和处理（`cvttsd2si` 对超界给 `i64::MIN`，随后 `sar`/`and`/`or`
/// 拼回"饱和到 u64::MAX"，再 `cmova` 比 `2^64`）—— Rust 的 `as u64` 正好是同一套语义。
pub fn revealed_points(fractional_tick: f64, active_tick: i64, total: usize) -> usize {
    let d = (fractional_tick - active_tick as f64) * REVEAL_POINTS_PER_TICK;
    // `f64 as u64` 正好是原版那套语义：NaN / 负数 ⇒ 0，`+∞` / 超界 ⇒ `u64::MAX`（再被
    // `points.len()` 夹住）。**不要**加 `is_finite()` 之类的过滤——那会把 `+∞` 变成 0。
    let n = d as u64;
    (n.min(usize::MAX as u64) as usize).min(total)
}

/// 本帧这发还在不在场 —— `get_active_functions_on_tick`（`0x456df0`）的谓词，
/// 逐条翻自 `0x3be5e6`–`0x3be611`（那个取值器的过滤逻辑被内联进了 `0x3be540`）。
///
/// 三条同时成立才算在场：
///
/// 1. `value+0x70 == 1`（`0x3be5e6` 的 `cmpl $0x1,-0x8(%rbp,%rcx,1)`）—— 那个字段的**名字未知**
///    （`FunctionState` 的哪个变体不知道），本类用"注入了点"代替；
/// 2. `value+0x50 != i64::MIN`（`0x3be5f0` 的 `cmp %r13,-0x28(%rcx)`；`r13 = 0x8000_0000_0000_0000`）
///    —— 某个 8 字节 `Option` 字段是 `Some`，**字段语义未知**，本类不做这个判定；
/// 3. `active_tick < tick`（**严格**，无符号）且 `30*(tick-active_tick) < points.len() + 1800`。
pub fn is_active_function(integer_tick: i64, active_tick: i64, total: usize) -> bool {
    if integer_tick <= active_tick {
        return false;
    }
    let d = (integer_tick - active_tick) as u64;
    // 30*d 与原版一样按 u32/无符号比；d 不会超过 i64 范围，转 u32 之前先夹一下。
    let revealed = d.saturating_mul(REVEAL_POINTS_PER_TICK as u64);
    revealed < (total as u64).saturating_add(ACTIVE_TAIL_POINTS as u64)
}

/// 一帧该补几个 tick（[`GraphwarPlane::drive_frame`] 的纯逻辑；定步 16_666_666 ns/tick）。
///
/// 原版 `GraphwarTickHandler::tick`（`0x519840`）每调一次 `accum += 16_666_666 ns`、`tick += 1`，
/// 而"该不该调"由调用者按帧决定（`graphwar_game_client.rs` 每帧一次，见 `drive_frame` 的
/// 调用链注释）。本函数取 `(帧内余数纳秒, 帧间隔秒)`，返回 `(该补几个 tick, 新的余数)`。
///
/// * `delta` 非有限 / 负数 ⇒ 0 个 tick（余数不动）；
/// * 一帧最多补 `max_catchup` 个（防长卡顿后一次追几千个），超了余数清零（不再滚雪球）。
pub fn ticks_for_frame(accum_nanos: u64, delta_seconds: f64, max_catchup: u32) -> (u32, u64) {
    let step = crate::graphwar_tick_handler::NANOS_PER_TICK as u64;
    let add = if delta_seconds.is_finite() {
        (delta_seconds.clamp(0.0, 3600.0) * crate::graphwar_tick_handler::NANOS_PER_SECOND as f64)
            .round()
            .max(0.0) as u64
    } else {
        0
    };
    let mut acc = accum_nanos.saturating_add(add);
    let mut n = 0u32;
    while acc >= step && n < max_catchup {
        acc -= step;
        n += 1;
    }
    if n == max_catchup && max_catchup > 0 {
        acc = 0;
    }
    (n, acc)
}

/// 一条函数在本类里的存储（**替代未复原的 `GraphwarFunctionManager` 的 value**）。
///
/// 字段与偏移来自 `GraphwarPlane::draw` 的读者（hashbrown bucket 步长 `0x78`，键在 `+0x00`）：
///
/// ```text
/// 4b6670  add %r12,%rsi                        ; rsi = 本条 bucket 末尾（键在 bucket+0x00）
/// 4b66e9  mov -0x20(%rsi),%rax → 0x140(%rsp)   ; 点数组指针（update_function 的 rdx）
/// 4b66f5  mov -0x18(%rsi),%rax → 0x138(%rsp)   ; 点数组长度（update_function 的 rcx）
/// 4b6794  cmpb $0x0,-0x8(%r12)                 ; 一个 bool（=1 才继续往下喂）
/// 4b67a0  mov -0x4(%r12),%eax                  ; active_tick（u32）
/// ```
///
/// ⇒ 点数组是 **`Vec<(f64, f64)>`**：16 字节/点、`x` 在 `+0`、`y` 在 `+8`
/// （与 `src/function_drawer.rs::POINT_STRIDE` 的逐位结论一致）。
///
/// ⚠️ **这不是复原出来的结构体**：`graphwar_function_manager` 模块**还没有 `src/` 文件**
/// （`docs/coverage-matrix.tsv` 第 36 行把它算进 `src/game.rs`），所以本类先在棋盘类里放一份
/// 最小存储，好让"原版唯一的喂点路径"能真的跑起来。见 `docs/16-client.md` §41.4。
#[derive(Debug, Clone)]
struct FunctionEntry {
    /// 采样点（原版 value 里的 `Vec<(f64,f64)>`：ptr 在 `+0x58`、len 在 `+0x60`）。
    points: Vec<(f64, f64)>,
    /// 起飞 tick（原版 value+0x74，u32）。
    active_tick: i64,
    /// 队色 `0x00RRGGBB`（原版由 `get_team_overview` 的两张表查出来；本类由调用方给，见 §41.5）。
    color: u32,
    /// 这个 id 的 `active_tick` 被 [`GraphwarPlane::set_function_active_tick`] 改过几次
    /// （**我方计数**：验收要证"起飞 tick 真的来自 `FunctionActive` 事件"）。
    active_tick_writes: u32,
    /// 这条函数的 `Line2D` 是不是已经被"不在场"那条路删掉了（`0x4b6418`）。
    ///
    /// **entry 本身不删**（见 `feed_functions` ① 的注释）：原版管理器那一格留着，
    /// 所以 `set_function_active_tick` 迟到还能让它重新进窗口。
    line_removed: bool,
    /// **我方记账**：这个 id 已经通过 `FunctionDrawer::update_function` 喂进去几个点。
    ///
    /// 原版这个数字也存在——在 `FunctionDrawer` 的 value 里（`usize` 已写入数，
    /// `docs/16-client.md` §40.3a 的 `bucket+0x28`）。本类不从那边读，因为
    /// `FunctionDrawer::written()` 是 `#[func]`（GDScript 侧的方法），Rust 侧拿不到；
    /// 记一份是为了让 [`GraphwarPlane::has_revealing_function`] 能判断
    /// "**还有该喂但没喂的点**"（§43 的验收：曲线要能揭到最后一个点）。
    /// 曲线不会变短（`append_window`），所以这个数**只增不减**；同一 id 被重新注入点数组时
    /// 归 0（重新对齐，不会造成重复追加——`append_window` 仍是唯一判定）。
    fed: usize,
}

/// `PixelRect`（**角点**：`min_x/min_y/max_x/max_y`）→ `PackedInt32Array`。
///
/// 不转成 `Rect2i`：原版存的就是角点（`psubd` / `paddd` 那两条），换成
/// `position + size` 的形状就把"原版不是 `Rect2i`"这个事实抹掉了。
fn pixels_of(r: PixelRect) -> PackedInt32Array {
    let mut out = PackedInt32Array::new();
    out.push(r.min_x);
    out.push(r.min_y);
    out.push(r.max_x);
    out.push(r.max_y);
    out
}

/// `GraphwarPlane`：棋盘本体（`Control` 子类）。画棋盘框 + 持 3 个 `FunctionDrawer` + 收脏矩形。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct GraphwarPlane {
    base: Base<Control>,

    /// 棋盘单位 → 像素的缩放（默认 [`DEFAULT_BOARD_SCALE`]）。
    #[export]
    board_scale: f64,

    /// 是否把 y 轴翻过来（棋盘 y 向上、Godot 画布 y 向下）。
    ///
    /// ⚠️ 原版的 `flip` **不是**一个属性：它是 `get_team_overview`（`0x4be6e0`）返回的那
    /// 4 个位算出来的（见 [`flip_from_overview_bits`]，指令 `0x4b600d`–`0x4b6029`）。
    /// 本类还没有那条游戏状态接线 ⇒ 用这个导出属性当替身（**我方选择**）。
    #[export]
    flip_y: bool,

    /// **原版的 `flip`**（镜像 x）：`get_team_overview`（`0x4be6e0`）返回的 4 个位算出来的
    /// （[`flip_from_overview_bits`]，指令 `0x4b600d`–`0x4b6029`）。
    ///
    /// 本类还没有游戏状态接线 ⇒ 用这个导出属性当替身（**我方选择**，默认 `false` = 不镜像）。
    #[export]
    mirror_x: bool,

    /// 打开就照 `AxisDrawer::draw` 的实参把棋盘内部填成**白底**（`0x4e069f`–`0x4e06ca`）。
    ///
    /// **默认关**的理由：原版这块底不是平面画的，而我们别的验收脚本按
    /// “每次 `_draw` 发 1 条 `draw_rect`”断言（`draw_classes_check.gd` / `assemble_check.gd`）；
    /// 打开会变成 2 条。⇒ 做成显式开关，播放屏要白底时置真。
    #[export]
    board_fill: bool,

    /// 纯逻辑的**意图结构**：3 个 `FunctionDrawer` 的 `NodeRef` + 脏矩形队列 + ready 标志。
    ///
    /// 原版对应 3 个 `Gd<FunctionDrawer>` 字段 + 一个 `Vec<PixelRect>`（`+0x50/+0x58/+0x60`）；
    /// 本类只存 id（文件头第 1 条）。
    plane: gp::PlaneIntent,

    /// 障碍层（`ObstacleDrawer`，`docs/16-client.md` §45）。**默认不建**，按需
    /// [`GraphwarPlane::build_obstacle_drawer`]。
    ///
    /// 原版这一层是**平面的一个普通字段**（`obstacle_drawer.rs:37:97` 的 Location 落在
    /// `GraphwarPlane::init` 里）；本类把它落成子节点，所以留一个 `Gd` 句柄——它与节点同生共死
    /// （文件头第 1 条：句柄不能比节点活得久）。
    ///
    /// ⚠️ **为什么不自动建**：既有验收脚本断言过平面的子节点数（`draw_classes_check.gd`
    /// 的 `3`、`assemble_check.gd` 的 `1`/`4`）。自动建会打破它们 ⇒ 改成显式调用。
    obstacle_drawer: Option<Gd<ObstacleDrawer>>,

    /// `_draw` 被引擎调用过几次。
    draw_calls: i64,
    /// 累计发出过几条 `draw_rect`（恒 `draw_calls`，除非 `_draw` 中途返回）。
    rect_total: i64,
    /// `_process` 被引擎调用过几次。
    process_calls: i64,
    /// 累计"刷了 ObstacleDrawer 几次"（`process_plan()` 说每帧 2 次）。
    obstacle_update_total: i64,
    /// `ready` 被引擎调用过几次。
    ready_calls: i64,
    /// `on_resized` 被调到过几次。
    resized_calls: i64,
    /// 累计从脏矩形队列里**冲掉**了几个框。
    flushed_total: i64,
    /// 因为脏矩形而请求重画的次数。
    dirty_redraws: i64,

    /// 每个 player id 一条函数（**替代未复原的 `GraphwarFunctionManager`**，见 [`FunctionEntry`]）。
    ///
    /// 用 `BTreeMap` 只为确定性：原版是 hashbrown `HashMap`，迭代顺序不参与任何判定
    /// （与 `src/function_drawer.rs::CurveStore` 同一个理由）。
    functions: std::collections::BTreeMap<i32, FunctionEntry>,

    /// tick 源：**本类自己持有**一个 [`GraphwarTickHandler`]。
    ///
    /// 原版 `draw` 每帧从它读两样东西：`0x4b6317` 的 `mov 0x58(%rbp),%ebx`（u32 tick，
    /// 喂 `get_active_functions_on_tick`）与 `0x4b631a` 的 `call 0x519530`
    /// （`get_fractional_tick`，f64，参与 `new_len`）。那个对象在游戏状态里
    /// （`draw` 里 `mov r14,[r12+0x238]` 借出来的 `Ref<GraphwarTickHandler>`）；本类还没有
    /// 那条接线 ⇒ 自己 `new` 一个（`base = 0`，`now` 用 [`GraphwarPlane::set_fractional_tick`] 摆）。
    ///
    /// 算式是**逐条复原**的 [`GraphwarTickHandler::get_fractional_tick`]（`0x519530`），
    /// 不再由场景注入一个 f64（旧实现是 `fractional_tick: f64`，`docs/16-client.md` §48.1）。
    tick_source: GraphwarTickHandler,

    /// 本类调过几次 `update_transform`（原版 `0x4b6147`：每次 `_draw` 一次）。
    transform_total: i64,

    /// 上一次 `update_transform` 的实参 `(x, y, s, flip)`（**我方计数**，验收用）。
    last_transform: (f64, f64, f64, bool),

    /// 累计调了几次 `FunctionDrawer::update_function`（**我方计数**）。
    function_write_total: i64,
    /// 累计调了几次 `FunctionDrawer::update_function_fade`（`0x4b6a0e`；**我方计数**）。
    function_fade_total: i64,
    /// 上一次交给 `update_function_fade` 的 alpha（`0x455a40` 的返回值）。
    last_fade_alpha: f64,
    /// `drive_frame` 攒下的**帧内余数**纳秒（< 一个 tick 的量）。
    frame_sub_nanos: u64,
    /// 累计调了几次 `drive_frame`（**我方计数**）。
    frame_drive_total: i64,
    /// 累计被 `drive_frame` 推进了几个 tick（**我方计数**）。
    frame_tick_total: i64,
    /// 累计追加了几个点（**我方计数**，与 `FunctionDrawer::add_point_total()` 应当一致）。
    function_point_total: i64,
    /// 累计因为"不在场"而 `remove_function` 掉几条（`0x4b6418`）。
    removed_function_total: i64,
    /// 累计"本帧有曲线正在揭示"⇒ 请求重画的次数。
    reveal_redraws: i64,
}

#[godot_api]
impl IControl for GraphwarPlane {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            board_scale: DEFAULT_BOARD_SCALE,
            flip_y: true,
            board_fill: false,
            mirror_x: false,
            // 纯逻辑的 `new()` = `Default::default()`：3 个 id 表 + 空脏队列 + ready=false。
            plane: gp::PlaneIntent::new(),
            obstacle_drawer: None,
            draw_calls: 0,
            rect_total: 0,
            process_calls: 0,
            obstacle_update_total: 0,
            ready_calls: 0,
            resized_calls: 0,
            flushed_total: 0,
            dirty_redraws: 0,
            functions: std::collections::BTreeMap::new(),
            tick_source: GraphwarTickHandler::new(0, 0),
            transform_total: 0,
            last_transform: (0.0, 0.0, 0.0, false),
            function_write_total: 0,
            function_fade_total: 0,
            last_fade_alpha: 1.0,
            frame_sub_nanos: 0,
            frame_drive_total: 0,
            frame_tick_total: 0,
            function_point_total: 0,
            removed_function_total: 0,
            reveal_redraws: 0,
        }
    }

    /// `_ready`：原版在这里连 `resized` + 订 3 个事件。
    ///
    /// 本类：① 建 3 个 `FunctionDrawer` 子节点（原版在 `init`，见文件头对照表）；② 用纯逻辑给的
    /// **信号名 / 处理者名**连 `resized`；③ 3 个事件**没订**（事件名照样能从
    /// [`GraphwarPlane::ready_event_names`] 读到，只是没真的订阅）。
    fn ready(&mut self) {
        self.ready_calls += 1;

        // ① 子节点（原版 `init` 观测：3 个 FunctionDrawer）。
        self.build_children();

        // ② `ready_plan()` 同时把纯逻辑的 `ready` 标志置 true，并给出 (信号名, 处理者名)。
        let ((signal, handler), _events) = self.plane.ready_plan();
        let myself = self.to_gd();
        let callable = myself.callable(handler);
        let err = self.base_mut().connect(signal, &callable);
        if err != godot::global::Error::OK {
            godot_warn!("GraphwarPlane::ready 连接 {signal} 失败: {err:?}");
        }
    }

    /// `_process`：原版每帧把 `ObstacleDrawer::update` 刷 **2** 次并读一次队色
    /// （`process_plan() -> (2, true)`）。
    ///
    /// 本类：照 `2` 计数（没有 `ObstacleDrawer` 节点可刷），
    /// 再把脏矩形队列冲掉——冲掉过东西才 `queue_redraw()`。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;

        let (updates, _reads_team) = self.plane.process_plan();
        self.obstacle_update_total += updates as i64;

        // 原版 `process` 每帧真的把 `ObstacleDrawer::update` 刷 **2** 次（`process_plan()` 的
        // `2`）。本类照做——**有障碍层就真刷**，没有就只有计数（§45）。同一帧刷 2 次是原版
        // 观测到的事实，不是我们的节流；为什么 2 次仍然**未知**。
        if let Some(drawer) = self.obstacle_drawer.clone() {
            let plane: Gd<Control> = self.to_gd().upcast::<Control>();
            for _ in 0..updates {
                drawer.clone().bind_mut().update(plane.clone());
            }
        }

        let flushed = self.plane.partial_updates.len() as i64;
        if flushed > 0 {
            self.plane.partial_updates.clear();
            self.flushed_total += flushed;
            self.dirty_redraws += 1;
            self.base_mut().queue_redraw();
        }

        // 曲线是**在 `_draw` 里**一帧一帧长出来的（原版：`0x4b69ec` 落在 `draw` 体内），
        // 而引擎只在"队列里有重画请求"时才调 `_draw` ⇒ 有曲线正在揭示就必须请求重画，
        // 否则那条线一帧都长不出来。
        //
        // ⚠️ **证据缺口（推断）**：原版 `process`（`0x4b83f0`）里只有
        // `ObstacleDrawer::update` ×2 + `get_team_overview`，**没有** `queue_redraw` 的直接调用，
        // 所以"谁让平面每帧重画"**没解出来**。本类按"还有该喂没喂的点就重画"补上
        // （判据与那个死角见 `has_revealing_function` 的文档），档位：我方选择。
        if self.has_revealing_function() {
            self.reveal_redraws += 1;
            self.base_mut().queue_redraw();
        }
    }

    /// `_draw`：原版 `GraphwarPlane::draw`（vaddr `0x4b5df0`）。
    ///
    /// 只有两个图元：`draw_set_transform` ×1（`0x4b607d`，槽 `0x9bc018`）+
    /// `draw_rect` ×1（`0x4b61ab`，槽 `0x9bc028`）。`draw_line` **0 次**（有界否定）。
    fn draw(&mut self) {
        self.draw_calls += 1;

        // 原版 `0x4b60c9` 的 `Control::get_size()`（槽 `0x9bb000` = `0x8925f0`），
        // 随后 `0x4b60d4`/`0x4b60da` 把两个 f32 落到栈上。见 [`board_transform_args`]。
        let size = self.base().get_size();
        let ((x, y), s) = board_transform_args(size);
        // **我方兜底**（原版无此分支）：尺寸还没定下来（树外 / 第一次 `_draw` 之前
        // `get_size()` 可能是 0）时退回导出属性，否则 `_transform` 会把整条线缩成 0。
        let s = if s > 0.0 { s } else { self.board_scale as f32 };
        // 原版 `flip`（镜像 x）来自 `get_team_overview` 的位组合；本类还没有那条接线 ⇒
        // 用导出属性 `mirror_x` 当替身（**我方选择**）。`flip_y` 是另一件事（y 取负，
        // 原版**永远**取负），只为本类复现工具留的开关。
        let flip = self.mirror_x;
        let (sx, sy) = board_scale_xy(s, flip, self.flip_y);
        let draw_fill = self.board_fill;
        let mut extra_rects = 0i64;

        {
            let mut base = self.base_mut();
            // 实参来自原版 `draw_set_transform`（`0x4b607d`，槽 `0x9bc018`）：
            // `pos = size × 0.5`、`rotation = 0`、`scale = flip ? (-s,-s) : (s,-s)`，`s = size.x/50`。
            base.draw_set_transform_ex(Vector2::new(x, y))
                .rotation(0.0)
                .scale(Vector2::new(sx, sy))
                .done();

            // 四个常量全部来自 `graphwar_plane::BOARD_*`（逐位：`.rodata 0x19cc20` / `0x19cf70`
            // / `0x1ba598`，两个 `xor` 给的 `false`）——原版 `0x4b61ab` 就画这一条**空心黑框**。
            let rect = Rect2::new(
                Vector2::new(gp::BOARD_RECT.0, gp::BOARD_RECT.1),
                Vector2::new(gp::BOARD_RECT.2, gp::BOARD_RECT.3),
            );
            let color = Color::from_rgba(
                gp::BOARD_COLOR.0,
                gp::BOARD_COLOR.1,
                gp::BOARD_COLOR.2,
                gp::BOARD_COLOR.3,
            );
            base.draw_rect_ex(rect, color)
                .filled(gp::BOARD_FILLED)
                .width(gp::BOARD_WIDTH)
                .antialiased(gp::BOARD_ANTIALIASED)
                .done();

            // 【**我方接线**，默认关】棋盘内部的白底：原版由 `AxisDrawer::draw` 画
            // （它是**子节点**，引擎按"父自绘 → 子自绘"的顺序 ⇒ 白底会盖住黑框靠内的那一半；
            // 这里照同一个顺序，不额外调 z_index）
            // （`0x4e069f`–`0x4e06ca`，filled=true / Color(1,1,1,1) / width=-1 / aa=false）。
            // `AxisDrawer` 节点还没有 `draw`（`src/_scaffold/axis_drawer_class.rs` 明写“没接管
            // `draw`”）⇒ 播放屏要“不再是工程底色”时把 `board_fill` 置真。
            if draw_fill {
                let fill = Rect2::new(
                    Vector2::new(gp::BOARD_RECT.0, gp::BOARD_RECT.1),
                    Vector2::new(gp::BOARD_RECT.2, gp::BOARD_RECT.3),
                );
                let c = Color::from_rgba(
                    BOARD_FILL_COLOR.0,
                    BOARD_FILL_COLOR.1,
                    BOARD_FILL_COLOR.2,
                    BOARD_FILL_COLOR.3,
                );
                base.draw_rect_ex(fill, c)
                    .filled(true)
                    .width(BOARD_FILL_WIDTH)
                    .antialiased(false)
                    .done();
                extra_rects += 1;
            }
        }
        self.rect_total += 1 + extra_rects;

        // 原版 `0x4b6147`：画完框就摆曲线容器（**在**喂点循环 `0x4b6339` 之前）。
        self.apply_update_transform((x, y), s, flip);

        // 原版 `draw` 在画完棋盘框之后（`0x4b61ab` 之后）紧接着就是那一段喂点循环
        // （`0x4b6339` → `0x4b69ec`）。本类放在这里，位置与原版一致。
        self.feed_functions();
    }
}

#[godot_api]
impl GraphwarPlane {
    /// 建 3 个 `FunctionDrawer` 子节点（`FUNCTION_DRAWER_CHILDREN`）。幂等：已经有就不重复建。
    ///
    /// 名字**不自拟**：原版没 `set_name` ⇒ 引擎自动名（运行时是 `@FunctionDrawer@N`）。
    ///
    /// 返回子节点个数（恒 `3`）。
    #[func]
    fn build_children(&mut self) -> i64 {
        if !self.plane.function_drawers.is_empty() {
            return self.plane.function_drawers.len() as i64;
        }

        // 父节点取成**局部** `Gd<Node>`：闭包里就不必再借 `self`（`base_mut()` 是独占借用）。
        let mut parent: Gd<Node> = self.to_gd().upcast::<Node>();

        // `init_children` 把闭包返回的 id 依次记进 `plane.function_drawers`（纯逻辑的意图结构）。
        self.plane.init_children(|_class_name| {
            // 原版这里是 `Gd::default_instance::<FunctionDrawer>()`；现在是**真的**
            // `FunctionDrawer` 节点类（`docs/16-client.md` §38）。
            let child = FunctionDrawer::new_alloc();
            let node = child.clone().upcast::<Node>();
            parent.add_child(&node);
            // 原版没 `set_name` ⇒ 自动名（`@FunctionDrawer@N`）。
            // 句柄到这里就可以丢掉：节点归**父节点**所有（文件头第 1 条）。
            // `InstanceId::to_i64()` 就是原版 `NodeRef` 要的那个 u64（纯逻辑只拿它当不透明 id）。
            child.instance_id().to_i64() as u64
        });
        debug_assert_eq!(self.plane.function_drawers.len(), gp::FUNCTION_DRAWER_CHILDREN);
        self.plane.function_drawers.len() as i64
    }

    /// 纯逻辑记下的子节点数（`FUNCTION_DRAWER_CHILDREN` = 3）；还没建时是 0。
    #[func]
    fn function_drawer_count(&self) -> i64 {
        self.plane.function_drawers.len() as i64
    }

    // -----------------------------------------------------------------------
    // 障碍层（`ObstacleDrawer`，`docs/16-client.md` §45）
    // -----------------------------------------------------------------------

    /// 建障碍层：一个 [`ObstacleDrawer`] 子节点。幂等：已经有了就直接返回 `1`。
    ///
    /// 返回：平面上现在有几个 `ObstacleDrawer` 子节点（`0` / `1`）。
    ///
    /// **为什么是显式调用而不是 `ready` 里自动建**：见字段 `obstacle_drawer` 的注释
    /// （既有脚本断言过子节点数）。建完由 [`GraphwarPlane::process`] 每帧刷 2 次。
    #[func]
    fn build_obstacle_drawer(&mut self) -> i64 {
        if self.obstacle_drawer.is_some() {
            return 1;
        }
        let drawer = ObstacleDrawer::new_alloc();
        let node = drawer.clone().upcast::<Node>();
        self.base_mut().add_child(&node);
        self.obstacle_drawer = Some(drawer);
        1
    }

    /// 障碍层建了吗（`0` / `1`）。
    #[func]
    fn obstacle_drawer_count(&self) -> i64 {
        i64::from(self.obstacle_drawer.is_some())
    }

    /// 障碍层自己记的 `update` 次数（**由那一侧计数**，用来证明"平面真的在每帧驱动它"）。
    #[func]
    fn obstacle_drawer_updates(&self) -> i64 {
        match self.obstacle_drawer.clone() {
            Some(drawer) => drawer.bind().update_call_count(),
            None => 0,
        }
    }

    /// 棋盘坐标 → 像素（`graphwar_plane::to_pixel`：`x*1000/50 + 500`、`y*1000/50 + 300`）。
    #[func]
    fn to_pixel(&self, x: f64, y: f64) -> Vector2i {
        let (px, py) = gp::to_pixel(x, y);
        Vector2i::new(px, py)
    }
    /// 收一个脏矩形：`obstacle_partial_update_event` 的纯数据版。
    ///
    /// 返回 `PackedInt32Array[min_x, min_y, max_x, max_y]`（**角点**，不是 `Rect2i`）。
    ///
    /// ⚠️ 坏 tag 原版是 **`panic!()`**；本类**先拦**并返回空数组（panic 穿过 FFI 会 abort
    /// 整个进程）——这是一处**故意**与原版不同的行为，见文件头对照表。
    #[func]
    fn on_obstacle_partial_update(
        &mut self,
        tag: i64,
        x: f64,
        y: f64,
        size: f64,
    ) -> PackedInt32Array {
        let tag = tag.clamp(0, u32::MAX as i64) as u32;
        if !gp::is_partial_update_tag(tag) {
            godot_warn!(
                "GraphwarPlane::on_obstacle_partial_update 坏 tag {tag}（原版在这里 panic!）"
            );
            return PackedInt32Array::new();
        }
        pixels_of(self.plane.on_obstacle_partial_update(tag, x, y, size))
    }

    /// 脏矩形队列现在有几个。
    #[func]
    fn partial_update_count(&self) -> i64 {
        self.plane.partial_updates.len() as i64
    }

    // -----------------------------------------------------------------------
    // 函数曲线：喂点（原版 `GraphwarPlane::draw` 尾部那段循环，`0x4b6339` → `0x4b69ec`）
    // -----------------------------------------------------------------------

    /// 把本类的**虚拟时钟**摆到 `get_fractional_tick() == tick` 的那一刻（**我方接线口**）。
    ///
    /// 原版没有这个方法：它每帧直接从游戏状态里的 `GraphwarTickHandler` 读
    /// （`0x4b6317` 的 u32 tick + `0x4b631a` 的 `get_fractional_tick`）。本类还没有那条
    /// 接线 ⇒ 握住一个自己的 `GraphwarTickHandler`，用这个入口推进它：
    ///
    /// ```text
    /// n   = trunc(tick)                     ; ⇒ tick_source.current_tick（原版 M+0x40）
    /// sub = tick - n                        ; ⇒ 累加器之外的那点时间
    /// accum = n × 16_666_666 ns             ; ⇒ 原版 M+0x10/+0x18（`tick()` 每次加的量）
    /// now   = accum + round(sub × 0.016666666 × 1e9)   ; ⇒ 写进原版 M+0x20/+0x28
    /// ```
    ///
    /// 因为 [`GraphwarTickHandler::get_fractional_tick`] 就是
    /// `tick + ((now-base)-accum) / 0.016666666`，所以**整数 tick 读回来逐位相等**
    /// （`now - accum == 0` ⇒ 小数部分恰好 0.0）——验收脚本按整数 tick 断言不会因浮点抖动变红。
    ///
    /// 返回读回的 `get_fractional_tick()`。
    ///
    /// **T88**：这是"把时钟**对齐**到某个 tick"的入口（= 原版客户端收到
    /// `TickReply{current_tick}` 后把 `GraphwarTickHandler` 拨到服务端 tick 轴那件事，
    /// 观测依据 `docs/06` §60.2.1：原版不回 `TickReply` 时 S8 里**没有弹道**——客户端
    /// 刚连上时 tick ≈ 0，够不到 `FunctionActive.active_tick=615`）。
    /// **每帧推进**走 [`GraphwarPlane::drive_frame`]，不要拿这个入口当帧驱动。
    #[func]
    fn set_fractional_tick(&mut self, tick: f64) -> f64 {
        let t = if tick.is_finite() { tick.max(0.0) } else { 0.0 };
        let n = t.trunc();
        let sub = t - n;
        let n_u32 = n.min(u32::MAX as f64) as u32;
        self.tick_source.current_tick = n_u32;
        self.tick_source.elapsed_nanos = n_u32 as u64 * NANOS_PER_TICK as u64;
        let sub_nanos = (sub * crate::graphwar_tick_handler::tick_seconds()
            * NANOS_PER_SECOND as f64)
            .round()
            .max(0.0) as u64;
        let now = self.tick_source.elapsed_nanos + sub_nanos;
        self.tick_source.set_now(
            now / NANOS_PER_SECOND as u64,
            (now % NANOS_PER_SECOND as u64) as u32,
        );
        self.frame_sub_nanos = 0; // 轴被拨过 ⇒ 帧内余数作废（否则会多补/少补一个 tick）
        self.fractional_tick()
    }

    /// 现在的 tick = **原版 `GraphwarTickHandler::get_fractional_tick`（`0x519530`）的读数**。
    #[func]
    fn fractional_tick(&self) -> f64 {
        self.tick_source.get_fractional_tick()
    }

    /// **每帧推进时钟**（原版 `GraphwarGameClient::process` 每帧一次的那条驱动链）。
    ///
    /// 原版调用链（`观测`，地址口径 ELF vaddr）：
    ///
    /// ```text
    /// GraphwarGameClient::{0x515dc0, 0x516fd0}   per-frame（process / drive_frame）
    ///   → 槽 0x9bc468 → 0x518400   (graphwar_game_client.rs:132)
    ///     → 槽 0x9baf38 → 0x518240 (graphwar_game_client.rs:139)
    ///       → 0x519840 GraphwarTickHandler::tick   ; accum += 16_666_666 ns; tick += 1
    /// ```
    ///
    /// `tick()` 是**定步**的：每 `16_666_666 ns` 一个 tick（`0x51986c` 的 `add eax,0xfe502a`，
    /// `docs/06` §63 的真机读数也是 60.00 tick/s）。所以这里按帧间隔 `delta` 攒纳秒，
    /// 攒够一个 tick 就叫一次 [`GraphwarTickHandler::tick`]（一帧最多补
    /// [`GraphwarPlane::MAX_CATCHUP_TICKS`] 个，防长卡顿后一次追几千个 tick）。
    ///
    /// 不变量与 [`GraphwarPlane::set_fractional_tick`] 一致：
    /// `get_fractional_tick() == current_tick + (now-base-accum)/tick_seconds`，
    /// 且 `now-base = accum + 帧内余数` ⇒ 整数 tick 读数稳定、小数部分是真实的帧内相位。
    ///
    /// 返回推进后的 `get_fractional_tick()`。
    ///
    /// ⚠️ 本方法**不**喂点、不重画：喂点仍是原版那条路（`draw` 尾部 → [`feed_functions`]），
    /// 由 GDScript 侧每帧调 `queue_redraw`（`docs/06` §136）。
    #[func]
    fn drive_frame(&mut self, delta: f64) -> f64 {
        /// 一帧最多补几个 tick（10 s @ 60 Hz）：长卡顿后不让一帧追几千个。
        const MAX_CATCHUP_TICKS: u32 = 600;
        let (n, acc) = ticks_for_frame(self.frame_sub_nanos, delta, MAX_CATCHUP_TICKS);
        for _ in 0..n {
            self.tick_source.tick();
        }
        self.frame_sub_nanos = acc;
        self.frame_drive_total += 1;
        self.frame_tick_total += n as i64;
        // 时钟的 `now` 跟着累加器走（与 `set_fractional_tick` 同一条不变量）。
        let total = self.tick_source.elapsed_nanos + self.frame_sub_nanos;
        self.tick_source.set_now(
            total / NANOS_PER_SECOND as u64,
            (total % NANOS_PER_SECOND as u64) as u32,
        );
        // 原版 `tick()` 把 `TickEvent`/`TickRequest` 推进事件队列；本类没有那条队列
        // （订阅者是 `GraphwarEventHandler`，我方走 GDScript 的 net 会话）⇒ 这里丢掉，
        // 否则每帧一条会无限长。**这是本层的差**，登记在 `docs/06` §136。
        self.tick_source.outbox.clear();
        self.fractional_tick()
    }

    /// 累计调过几次 [`GraphwarPlane::drive_frame`]（**我方计数**，验收用）。
    #[func]
    fn frame_drive_total(&self) -> i64 {
        self.frame_drive_total
    }

    /// 累计被 [`GraphwarPlane::drive_frame`] 推进过几个 tick（**我方计数**，验收用）。
    #[func]
    fn frame_tick_total(&self) -> i64 {
        self.frame_tick_total
    }

    /// 原版 `draw` 里那个 u32 tick（`0x4b6317` 的 `mov 0x58(%rbp),%ebx` = `M+0x40`）。
    #[func]
    fn integer_tick_now(&self) -> i64 {
        self.tick_source.current_tick as i64
    }

    /// 累计调过几次 `update_transform`（原版 `0x4b6147`）。
    #[func]
    fn transform_total(&self) -> i64 {
        self.transform_total
    }

    /// 上一次 `update_transform` 的实参：`{x, y, s, flip}`（**我方计数**，验收用）。
    #[func]
    fn last_transform(&self) -> VarDictionary {
        let (x, y, s, flip) = self.last_transform;
        let mut d = VarDictionary::new();
        d.set("x", x);
        d.set("y", y);
        d.set("s", s);
        d.set("flip", flip);
        d.set("flip_y", self.flip_y);
        let (sx, sy) = board_scale_xy(s as f32, flip, self.flip_y);
        d.set("scale_x", sx as f64);
        d.set("scale_y", sy as f64);
        d
    }

    /// `get_team_overview`（`0x4be6e0`）→ `flip`：把纯函数 [`flip_from_overview_bits`] 暴露出来，
    /// 好让 GDScript 侧按反汇编真值表对拍（`0x4b600d`–`0x4b6029`）。
    #[func]
    fn flip_from_overview_bits(&self, flags: i64) -> bool {
        flip_from_overview_bits(flags as u32)
    }

    /// 队色池（**逐位**）：`init_team_colors`（x86 `0x465070` / arm64 `0x49c660`）里的两张 6 色表。
    ///
    /// 原版是 `HashMap<u8, Vec<GraphwarColor>>`（键 = 队号 0/1）；本方法按队号取那一池，
    /// 返回 6 个 `Color`（`GraphwarColor` 三字节 = r,g,b，alpha 恒 1）。
    /// 表本体在 [`crate::graphwar_color_manager`]（`TEAM1_COLORS` / `TEAM2_COLORS`）。
    #[func]
    fn team_color_pool(&self, team: i64) -> PackedColorArray {
        let pool: &[GraphwarColor; TEAM_POOL_LEN] = match team {
            0 => &TEAM1_COLORS,
            1 => &TEAM2_COLORS,
            _ => return PackedColorArray::new(), // 原版没有这个队 ⇒ `add_team_color` 走随机兜底
        };
        let mut out = PackedColorArray::new();
        for c in pool {
            let (r, g, b) = c.triple();
            out.push(Color::from_rgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                1.0,
            ));
        }
        out
    }

    /// 注入一条函数的采样点。
    ///
    /// ⚠️ **这不是原版 `GraphwarPlane` 的方法**：原版这三件事分别属于
    /// `GraphwarFunctionManager::set_function_points`（`0x456810`）、
    /// `set_function_active_tick`（`0x456b80`）与队色（`GraphwarDrawUtil::get_team_overview`，
    /// `0x4be6e0`，`draw` 里调 2 次）。那个模块**还没有 `src/` 文件**，而本轮只许改这个文件 ⇒
    /// 先在这里放一份最小存储 + 入口，好让原版**唯一的**喂点路径能真的跑起来（§41.4）。
    ///
    /// * `points`：**扁平 f64 数组** `[x0, y0, x1, y1, …]`。原版的点是 `(f64, f64)`（16 B/点，
    ///   `x` 在 `+0`、`y` 在 `+8`）——用 f64 而不是 `PackedVector2Array` 是为了不在入口就掉精度；
    ///   奇数长度时**丢掉最后一个**。
    /// * `active_tick`：起飞 tick（原版 value+0x74 的 u32）。
    /// * `color`：`0x00RRGGBB`，只在第一次建 `Line2D` 时用（同 `FunctionDrawer::update_function`）。
    ///
    /// 返回存下来的点数；`player_id` 超出 i32 ⇒ `-1`。
    ///
    /// 同一 id 重复注入 ⇒ **覆盖**点数组。已经画出去的点不会缩回去（`update_function` 的
    /// "变短不加点"，`0x498d5a`）——要让曲线从 0 重来，得先让它离开活跃窗口（或重建节点）。
    #[func]
    fn set_function_points(
        &mut self,
        player_id: i64,
        points: PackedFloat64Array,
        active_tick: i64,
        color: i64,
    ) -> i64 {
        let Ok(id) = i32::try_from(player_id) else {
            godot_warn!("GraphwarPlane::set_function_points player_id {player_id} 超出 i32");
            return -1;
        };
        let flat = points.as_slice();
        let mut pts: Vec<(f64, f64)> = Vec::with_capacity(flat.len() / 2);
        let mut i = 0;
        while i + 1 < flat.len() {
            pts.push((flat[i], flat[i + 1]));
            i += 2;
        }
        let len = pts.len() as i64;
        self.functions.insert(
            id,
            FunctionEntry {
                points: pts,
                active_tick,
                color: (color as u32) & 0x00ff_ffff,
                active_tick_writes: 0,
                line_removed: false,
                fed: 0,
            },
        );
        len
    }

    /// 原版 `GraphwarFunctionManager::set_function_active_tick`（x86 **`0x456b80`**）：
    /// 写起飞 tick（`FunctionState.value+0x74`）并发一条 `FunctionActive`(36)。
    ///
    /// 为什么单独一个入口：原版就是**两个独立的 setter**（`set_function_points` `0x456810`
    /// / `set_function_active_tick` `0x456b80`）⇒ 到达顺序无关。联机上 `FunctionPoints`
    /// 先到、`FunctionActive` 后到是常态（`fixtures/frame_match_events.txt` [phase2] 就是这个顺序）。
    ///
    /// tag 证据（load 码）：`0x456cd3 lea rax,[r9+0x25]`（`r9 = 0x7fff…ffff`）⇒ `0x8000…0024` = 36；
    /// 投递 `0x456ce1 call [0x9bac80]` → `0x3d3530 add_event`。
    ///
    /// 返回值：改到了几条（0 = 这个 id 还没有点序列 / 1 = 改了一条）。
    #[func]
    fn set_function_active_tick(&mut self, player_id: i64, active_tick: i64) -> i64 {
        // 0x456b80 — 写 value+0x74 后 add_event(tag 36 FunctionActive)
        let Ok(id) = i32::try_from(player_id) else {
            godot_warn!("GraphwarPlane::set_function_active_tick player_id {player_id} 超出 i32");
            return -1;
        };
        match self.functions.get_mut(&id) {
            Some(entry) => {
                entry.active_tick = active_tick.max(0);
                entry.active_tick_writes += 1;
                1
            }
            None => 0,
        }
    }

    /// 清掉所有函数（下次 `draw` 会把对应的 `Line2D` 也删掉）。
    ///
    /// 返回清掉几条。
    #[func]
    fn clear_function_points(&mut self) -> i64 {
        let n = self.functions.len() as i64;
        self.functions.clear();
        n
    }

    /// 本 tick 还在场的函数 id（= `get_active_functions_on_tick`（`0x456df0`）的返回集）。
    ///
    /// 谓词见 [`is_active_function`]（三条，逐条翻自 `0x3be5e6`–`0x3be611`）。
    #[func]
    fn active_function_ids(&self) -> PackedInt32Array {
        let mut out = PackedInt32Array::new();
        let tick = self.integer_tick();
        for (id, entry) in &self.functions {
            if is_active_function(tick, entry.active_tick, entry.points.len()) {
                out.push(*id);
            }
        }
        out
    }

    /// **原版 `GraphwarPlane::draw` 尾部那段喂点循环**（`0x4b6339`–`0x4b6a13`）。
    ///
    /// ```text
    /// for id in function_manager.get_active_functions_on_tick(tick)   ; 0x4b6339
    ///     entry = manager[id]                                         ; 0x4b65d9 起的 hashbrown 查表
    ///     if entry.state != 1 || entry.opt == i64::MIN { continue }    ; 0x3be5e6 / 0x3be5f0
    ///     new_len = min(points.len(), 30*(fractional_tick - entry.active_tick))
    ///                                                                 ; 0x4b67a0–0x4b6812
    ///     drawer.update_function(id, points, points.len, new_len, color)
    ///                                                                 ; 0x4b69ec
    ///     drawer.update_function_fade(id, get_function_fade_on_tick(id, tick))
    ///                                                                 ; 0x4b69fe + 0x4b6a0e
    /// for id in <不再在场的>                                            ; 0x4b6410
    ///     drawer.remove_function(id)                                  ; 0x4b6418
    /// ```
    ///
    /// **本类做到的**：① 活跃谓词（三条里第 3 条）；② `new_len` 的算式与常量；
    /// ③ `update_function` 的实参形状（点前缀 + 队色）；④ 不再在场的走 `remove_function`。
    ///
    /// **本类做不到 / 故意没做**：
    /// * `entry.state != 1` 与 `entry.opt != i64::MIN` 两条判定（字段语义未知，§41.5）；
    /// * `update_function_fade`：`get_function_fade_on_tick` **没有 x86 落点**（走 GOT 槽
    ///   `0x9bbd08`，是个方法指针调用），alpha 的算式没解出来 ⇒ 不发明；
    /// * 喂给**哪一条** `FunctionDrawer`：原版三次调用都用 `self+0x80` 那**一个**
    ///   （`0x4b6132` 的 `lea 0x80(%rax),%rdi`）⇒ 本类固定喂第 0 个子节点（"`self+0x80` 是 3 个里的
    ///   第一个"是**推断**：3 个 `Gd` 字段的排列没有直接证据）。
    ///
    /// 返回本帧追加了几个点（0 = 没有新增，-1 = 子节点还没建）。
    #[func]
    fn feed_functions(&mut self) -> i64 {
        if self.functions.is_empty() {
            return 0;
        }
        let Some(mut drawer) = self.function_drawer(0) else {
            return -1;
        };
        let tick = self.integer_tick();
        let ftick = self.fractional_tick();

        // ① 已经不在场的：`remove_function`（`0x4b6418` → `0x498b70`）。
        //
        // ⚠️ **只删 `Line2D`，不删管理器的 entry**（T88 更正）。原版 `0x4b6418` 调的是
        // `FunctionDrawer::remove_function`（`queue_free` + 删抽屉自己那张表），
        // `GraphwarFunctionManager` 里那一格**原样留着** —— 所以 `set_function_active_tick`
        // 迟到（联机上 `FunctionPoints` 先到、`FunctionActive` 后到是常态）仍然能生效：
        // 起飞 tick 一改，这条函数就能重新进窗口、重新建线画出来。
        // 以前这里顺手 `self.functions.remove(&id)`，等于把"迟到的起飞 tick"永远丢掉。
        let stale: Vec<i32> = self
            .functions
            .iter()
            .filter(|(_, e)| !is_active_function(tick, e.active_tick, e.points.len()))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some(e) = self.functions.get_mut(&id) {
                if e.line_removed {
                    continue; // 线已经删过了，别每帧重复调
                }
                e.line_removed = true;
                e.fed = 0; // 线重建后 `append_window` 的"已写入数"从 0 起
            }
            if drawer.bind_mut().remove_function(id as i64) {
                self.removed_function_total += 1;
            }
        }

        // ② 还在场的：喂点。
        //
        // ⚠️ 只喂**在场集合**里的（= `get_active_functions_on_tick(tick)` 的返回集，`0x4b6339`）。
        // T88 更正：entry 现在不随"出窗口"删除了（见 ①），所以这里**不能**再遍历
        // `self.functions.keys()`——那样会把已经出窗口的函数每帧重新画出来（探针实测：
        // 出窗口后 `Line2D` 又长出 780 点，等于活跃窗口完全失效）。
        let ids: Vec<i32> = self
            .functions
            .iter()
            .filter(|(_, e)| is_active_function(tick, e.active_tick, e.points.len()))
            .map(|(id, _)| *id)
            .collect();
        let mut appended = 0i64;
        for id in ids {
            let mut prefix = PackedVector2Array::new();
            let color;
            let new_len;
            {
                let entry = &self.functions[&id];
                new_len = revealed_points(ftick, entry.active_tick, entry.points.len());
                for point in entry.points.iter().take(new_len) {
                    // 原版：`movsd` ×2 + `cvtsd2ss` ⇒ `add_point(Vector2, -1)`。
                    prefix.push(Vector2::new(point.0 as f32, point.1 as f32));
                }
                color = entry.color as i64;
            }
            // `new_len` 在这里就是 `prefix.len()`：原版把全长与 `new_len` 分开传，
            // 而 `FunctionDrawer` 只补 `points[written..new_len]` ⇒ 传前缀是**等价**的
            // （§40.4b 已记过这条；`append_window` 仍是唯一判定）。
            if let Some(entry) = self.functions.get_mut(&id) {
                entry.line_removed = false;
            }
            appended += drawer.bind_mut().update_function(id as i64, prefix, color);
            // 记账：本 id 现在"该喂到"哪一点（只增；曲线不会变短）。
            if let Some(entry) = self.functions.get_mut(&id) {
                entry.fed = entry.fed.max(new_len);
            }
            self.function_write_total += 1;
            // **淡出**：原版紧接着就调 `get_function_fade_on_tick`（`0x4b69fe` → 槽 `0x9bbd08`
            // → `0x455a40`），把返回值当 alpha 交给 `FunctionDrawer::update_function_fade`
            // （`0x4b6a0e` → `0x4992b0`）。算式与指令见 `graphwar_function_manager` §八。
            //
            // `update_function_fade` 是 `function_drawer.rs` 里的**非 `pub`** `#[func]`
            // ⇒ 只能走动态调用（与本文件 `update_transform` 同一条路；原版本身也是
            // 走重定位槽的间接调用 —— `0x4b69fe call *0x9bbd08`）。
            let alpha = crate::graphwar_function_manager::function_fade_on_tick(
                tick as u32,
                self.functions[&id].active_tick as u32,
                self.functions[&id].points.len(),
            );
            let applied = drawer
                .call(
                    "update_function_fade",
                    &[(id as i64).to_variant(), alpha.to_variant()],
                )
                .to::<f64>();
            if applied >= 0.0 {
                self.function_fade_total += 1;
                self.last_fade_alpha = applied;
            }
        }
        self.function_point_total += appended;
        appended
    }

    /// 存着几条函数。
    #[func]
    fn function_count(&self) -> i64 {
        self.functions.len() as i64
    }

    /// 某条函数现在存了几个点（没有这个 id ⇒ `-1`）。
    #[func]
    fn function_point_count(&self, player_id: i64) -> i64 {
        i32::try_from(player_id)
            .ok()
            .and_then(|id| self.functions.get(&id))
            .map(|e| e.points.len() as i64)
            .unwrap_or(-1)
    }

    /// 累计调了几次 `FunctionDrawer::update_function`（**我方计数**）。
    #[func]
    fn function_write_total(&self) -> i64 {
        self.function_write_total
    }

    /// 累计调了几次 `FunctionDrawer::update_function_fade`（`0x4b6a0e`；**我方计数**）。
    #[func]
    fn function_fade_total(&self) -> i64 {
        self.function_fade_total
    }

    /// 上一次交给 `update_function_fade` 的 alpha（= `get_function_fade_on_tick` 的返回值）。
    #[func]
    fn last_fade_alpha(&self) -> f64 {
        self.last_fade_alpha
    }

    /// 某条函数的 `active_tick` 被 `set_function_active_tick` 改过几次（**我方计数**：
    /// 验收用它证"起飞 tick 真的来自 `FunctionActive` 事件"，而不是恒 0）。
    #[func]
    fn function_active_tick_writes(&self, player_id: i64) -> i64 {
        i32::try_from(player_id)
            .ok()
            .and_then(|id| self.functions.get(&id))
            .map(|e| e.active_tick_writes as i64)
            .unwrap_or(-1)
    }

    /// 某条函数当前的起飞 tick（原版 `value+0x74`；没有这个 id ⇒ `-1`）。
    #[func]
    fn function_active_tick_of(&self, player_id: i64) -> i64 {
        i32::try_from(player_id)
            .ok()
            .and_then(|id| self.functions.get(&id))
            .map(|e| e.active_tick)
            .unwrap_or(-1)
    }

    /// 累计追加了几个点（**我方计数**，应与 `FunctionDrawer::add_point_total()` 相等）。
    #[func]
    fn function_point_total(&self) -> i64 {
        self.function_point_total
    }

    /// 累计因为"不在场"而删掉几条线（`0x4b6418`）。
    #[func]
    fn removed_function_total(&self) -> i64 {
        self.removed_function_total
    }

    /// 因为"有曲线正在揭示"而请求重画的次数（`_process`）。
    #[func]
    fn reveal_redraw_count(&self) -> i64 {
        self.reveal_redraws
    }

    /// 冲刷脏矩形队列，返回冲掉几个（`_process` 每帧也会自动冲一次）。
    #[func]
    fn drain_partial_updates(&mut self) -> i64 {
        let n = self.plane.partial_updates.len() as i64;
        self.plane.partial_updates.clear();
        self.flushed_total += n;
        n
    }

    /// 原版 `ready` 会按顺序订阅的 3 个事件名（`READY_EVENTS`）——**本类没订**，只读名字。
    #[func]
    fn ready_event_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for (name, _slot) in gp::READY_EVENTS {
            out.push(&GString::from(name));
        }
        out
    }

    /// `resized` 信号的处理者（原版名字就是 `on_resized`，`.rodata 0x1c2a7a`）。
    ///
    /// 它是 `#[func]` ⇒ GDScript 也能直接调（验收脚本用它验"树外不会自动触发"）。
    #[func]
    fn on_resized(&mut self) {
        self.resized_calls += 1;
        // 原版这里做 3 次 `Control::set_*`，方法名未解（`UNKNOWNS` 第 3 条）⇒ 只重画。
        self.base_mut().queue_redraw();
    }

    /// `_draw` 被引擎调用过几次。
    #[func]
    fn draw_call_count(&self) -> i64 {
        self.draw_calls
    }

    /// 累计发出过几条 `draw_rect`。
    #[func]
    fn drawn_rect_total(&self) -> i64 {
        self.rect_total
    }

    /// `_process` 被引擎调用过几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// 累计"刷了障碍绘制器几次"（每帧 `OBSTACLE_DRAWER_UPDATES_PER_FRAME` = 2 次）。
    #[func]
    fn obstacle_update_total(&self) -> i64 {
        self.obstacle_update_total
    }

    /// `_ready` 被引擎调用过几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `on_resized` 被调到过几次。
    #[func]
    fn resized_call_count(&self) -> i64 {
        self.resized_calls
    }

    /// 累计从脏矩形队列里冲掉过几个框。
    #[func]
    fn flushed_total(&self) -> i64 {
        self.flushed_total
    }

    /// 因为脏矩形而请求重画的次数。
    #[func]
    fn dirty_redraw_count(&self) -> i64 {
        self.dirty_redraws
    }
}

/// 不走 `#[godot_api]` 的内部小工具（`#[godot_api]` 块里只能放 `#[func]` / signal / constant）。
impl GraphwarPlane {
    /// 整数 tick：原版是 `[tick_handler+0x58]` 的 **u32**（`0x4b6317` 的 `mov 0x58(%rbp),%ebx`
    /// = `GraphwarTickHandler` 的 `M+0x40`，`tick()` 里 `inc` 的那个计数）——
    /// 所以这里读 [`GraphwarTickHandler::current_tick`]，**不**从 f64 取整（`tick 177.9` 时
    /// 两者都是 177，但 177.9 的 f64 取整依赖浮点，原版没有这一步）。
    fn integer_tick(&self) -> i64 {
        self.tick_source.current_tick as i64
    }

    /// 原版 `update_transform`（`0x4b6147`）的落地：把 `(x, y, s, flip)` 交给每个
    /// `FunctionDrawer`（原版的目标是 `self+0x80` 那个绘制器；本类是 3 个里的第 0 个 —— §42.4）。
    ///
    /// ⚠️ **喂给 [`FunctionDrawer::update_transform`] 的 `scale` 是“预取负”过的**：
    /// 原版 `0x499232`–`0x499241` 把 `s` 与 `-s` 分别当两个 f32 喂 `set_scale`
    /// （`xmm0 = [rsp+0x20]` 原值 / `xmm1 = [rsp+0x10]` 取负那份），而本类的 `FunctionDrawer`
    /// （另一个文件）拿到 `flip` 为真时**两个分量都取负**、为假时原样用。
    /// ⇒ 想得到 `flip ? (-s,-s) : (s,-s)`，就得传 `flip ? (s,s) : (s,-s)`。
    ///
    /// 返回动了几条线（3 个绘制器加起来）。
    fn apply_update_transform(&mut self, (x, y): (f32, f32), s: f32, flip: bool) -> i64 {
        // 目标值 = [`board_scale_xy`]；`FunctionDrawer` 的 `flip` 分支会把两个分量都取负，
        // 所以 `flip` 为真时先喂"反的那份"，它取负之后就正好是目标值。
        let (sx, sy) = board_scale_xy(s, flip, self.flip_y);
        let scale = if flip {
            Vector2::new(-sx, -sy)
        } else {
            Vector2::new(sx, sy)
        };
        let n = self.plane.function_drawers.len() as i32;
        let mut touched = 0i64;
        for i in 0..n {
            if let Some(mut d) = self.function_drawer(i) {
                // `update_transform` 是 `function_drawer.rs` 的 `#[func]`（非 `pub`）
                // ⇒ 只能走动态调用（与 GDScript 侧同一条路）。
                let _ = d.call(
                    "update_transform",
                    &[
                        (x as f64).to_variant(),
                        (y as f64).to_variant(),
                        scale.to_variant(),
                        flip.to_variant(),
                    ],
                );
                touched += 1;
            }
        }
        self.transform_total += 1;
        self.last_transform = (x as f64, y as f64, s as f64, flip);
        touched
    }

    /// 第 `index` 个 `FunctionDrawer` 子节点（原版 `init` 建 3 个）。
    ///
    /// 不存 `Gd` 句柄：本类只记 instance id（文件头第 1 条），用的时候从 `base()` 现取。
    fn function_drawer(&self, index: i32) -> Option<Gd<FunctionDrawer>> {
        let child: Gd<Node> = self.base().get_child(index)?;
        child.try_cast::<FunctionDrawer>().ok()
    }

    /// 还有"该喂但没喂"的点 ⇒ `_process` 要请求重画。
    ///
    /// **判据是"已喂 vs 应喂"，不是"应喂 < 全长"**。早先写的是后者
    /// （`revealed_points(...) < points.len()`），它有一个真实的死角：当
    /// `revealed_points` 第一次**恰好等于全长**时（`30*t >= len`，而 `len` 往往不是 30 的
    /// 整数倍），这一帧已经算"揭完"⇒ `_process` 不再请求重画 ⇒ 引擎不会为它再调一次
    /// `_draw` ⇒ **最后不足 30 个点永远喂不进去**（`docs/16-client.md` §43 里 5314 点的
    /// 真实弹道就是卡在 5310）。改成比 [`FunctionEntry::fed`]（本 id 已喂到第几点）：
    /// `应喂 > 已喂` 就必须再来一帧。
    ///
    /// ⚠️ 与 [`GraphwarPlane::process`] 里那条一样，这是**我方选择**：原版 `process`
    /// （`0x4b83f0`）里**没有** `queue_redraw` 的直接调用，"谁让平面每帧重画"仍未解
    /// （§42.5 第 6 条）。
    fn has_revealing_function(&self) -> bool {
        let tick = self.integer_tick();
        self.functions.values().any(|e| {
            !e.line_removed
                && is_active_function(tick, e.active_tick, e.points.len())
                && revealed_points(self.fractional_tick(), e.active_tick, e.points.len()) > e.fed
        })
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 3 个子节点这个数字来自纯逻辑，不是本类自己写的 3。
    #[test]
    fn child_count_comes_from_the_pure_module() {
        assert_eq!(gp::FUNCTION_DRAWER_CHILDREN, 3);
        let mut plane = gp::PlaneIntent::new();
        let mut n = 0;
        plane.init_children(|_name| {
            n += 1;
            n as u64
        });
        assert_eq!(plane.function_drawers.len(), gp::FUNCTION_DRAWER_CHILDREN);
        assert_eq!(plane.function_drawers, vec![1, 2, 3]);
    }

    /// 默认缩放取的是 `PX_PER_UNIT`（1000/50 = 20），棋盘框是 50×30 个单位 ⇒ 1000×600 像素。
    #[test]
    fn default_scale_maps_the_board_onto_the_canvas() {
        assert!((DEFAULT_BOARD_SCALE - 20.0).abs() < 1e-9);
        assert!((gp::BOARD_RECT.2 * DEFAULT_BOARD_SCALE as f32 - gp::CANVAS_W as f32).abs() < 1e-3);
        assert!((gp::BOARD_RECT.3 * DEFAULT_BOARD_SCALE as f32 - gp::CANVAS_H as f32).abs() < 1e-3);
        // 棋盘框的角在像素空间里正好是 (0,0) 与 (1000,600)。
        let (x0, y0) = gp::to_pixel(gp::BOARD_RECT.0 as f64, gp::BOARD_RECT.1 as f64);
        assert_eq!((x0, y0), (0, 0), "左下角 → 画布原点");
        let (x1, y1) = gp::to_pixel(
            (gp::BOARD_RECT.0 + gp::BOARD_RECT.2) as f64,
            (gp::BOARD_RECT.1 + gp::BOARD_RECT.3) as f64,
        );
        assert_eq!((x1, y1), (1000, 600), "右上角 → 画布右下角");
    }

    /// 脏矩形的换算与手算一致，且**只接受两个 tag**。
    #[test]
    fn partial_update_rect_matches_the_hand_computed_pixels() {        // (5, 5) → (600, 400)；半径 = size*1000/50 = 1.5*20 = 30。
        let mut plane = gp::PlaneIntent::new();
        let r = plane.on_obstacle_partial_update(gp::ENTITY_TAG_A, 5.0, 5.0, 1.5);
        assert_eq!(
            r,
            PixelRect {
                min_x: 570,
                min_y: 370,
                max_x: 630,
                max_y: 430
            }
        );
        assert_eq!((r.width(), r.height()), (60, 60), "正方形，不是圆");
        assert_eq!(plane.partial_updates.len(), 1);
        // 第二个合法 tag 也收。
        plane.on_obstacle_partial_update(gp::ENTITY_TAG_B, -5.0, -5.0, 0.5);
        assert_eq!(plane.partial_updates.len(), 2);
        assert!(!gp::is_partial_update_tag(0));
        assert!(!gp::is_partial_update_tag(0x27));
    }

    /// `ready_plan()` 给出的信号名 / 处理者名 / 3 个事件名就是本类用的那几个字符串。
    #[test]
    fn ready_plan_supplies_the_names_the_node_class_uses() {
        let mut plane = gp::PlaneIntent::new();
        let ((signal, handler), events) = plane.ready_plan();
        assert_eq!(signal, "resized");
        assert_eq!(handler, "on_resized");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], "add_obstacle_event");
        assert_eq!(events[1], "destroy_obstacle_event");
        assert_eq!(events[2], "clear_obstacles_event");
        assert!(plane.ready, "ready_plan() 会把标志置 true");
    }

    /// `process_plan()` 说每帧刷 2 次、读一次队色；本类只用了那个 2。
    #[test]
    fn process_plan_reports_two_obstacle_updates_per_frame() {
        let plane = gp::PlaneIntent::new();
        assert_eq!(
            plane.process_plan(),
            (gp::OBSTACLE_DRAWER_UPDATES_PER_FRAME, true)
        );
        assert_eq!(gp::OBSTACLE_DRAWER_UPDATES_PER_FRAME, 2);
    }

    // -----------------------------------------------------------------------
    // 曲线喂点：常量与算式（**不碰引擎**）
    // -----------------------------------------------------------------------

    /// `30.0` 是从 `.rodata 0x19ef90` 逐位读出来的，不是我方挑的数。
    #[test]
    fn reveal_rate_is_the_bit_exact_rodata_constant() {
        assert_eq!(REVEAL_POINTS_PER_TICK, 30.0);
        assert_eq!(REVEAL_POINTS_PER_TICK.to_bits(), 0x403e_0000_0000_0000);
        // `0x708` = 1800（`0x3be60b` 的 `add $0x708,%edi`）。
        assert_eq!(ACTIVE_TAIL_POINTS, 1800);
        assert_eq!(ACTIVE_TAIL_POINTS / REVEAL_POINTS_PER_TICK as usize, 60, "60 tick 的淡出窗");
    }

    /// `new_len = min(points.len(), trunc(30*(fractional_tick - active_tick)))`（`0x4b67a0`–`0x4b6812`）。
    #[test]
    fn revealed_points_follows_the_decoded_formula() {
        // 还没起飞 ⇒ 0（负数走的是 0，不是回绕）。
        assert_eq!(revealed_points(9.0, 10, 100), 0);
        assert_eq!(revealed_points(10.0, 10, 100), 0);
        // 起飞 2 个 tick ⇒ 60 点。
        assert_eq!(revealed_points(12.0, 10, 100), 60);
        // 小数部分参与（原版是 f64 乘 30 再截断）。
        assert_eq!(revealed_points(12.5, 10, 100), 75);
        assert_eq!(revealed_points(12.999, 10, 100), 89, "89.97 ⇒ 截断成 89");
        // 满了就封顶（`0x4b6812` 的 `cmovb`）。
        assert_eq!(revealed_points(100.0, 10, 100), 100);
        // NaN / 无穷不 panic。
        assert_eq!(revealed_points(f64::NAN, 10, 100), 0);
        assert_eq!(revealed_points(f64::INFINITY, 10, 100), 100);
    }

    /// 活跃谓词（`0x3be5f6`–`0x3be611`）：`active_tick < tick` 且 `30*d < len + 1800`。
    #[test]
    fn active_predicate_matches_the_decoded_window() {
        // 严格大于：tick == active_tick 不算在场（`jbe`）。
        assert!(!is_active_function(10, 10, 100));
        assert!(!is_active_function(9, 10, 100));
        assert!(is_active_function(11, 10, 100));
        // 1800 点 = 60 tick 的尾巴：len=100 ⇒ 100+1800=1900 ⇒ d < 1900/30 = 63.33 ⇒ d ≤ 63。
        assert!(is_active_function(10 + 63, 10, 100), "d=63 ⇒ 1890 < 1900");
        assert!(!is_active_function(10 + 64, 10, 100), "d=64 ⇒ 1920 >= 1900");
        // 空曲线也有 60 tick 的尾巴（len=0 ⇒ 只留 1800/30 = 60）。
        assert!(is_active_function(10 + 59, 10, 0));
        assert!(!is_active_function(10 + 60, 10, 0));
    }

    /// `drive_frame` 的定步逻辑（`0x519840` 的 `add $0xfe502a` = 16_666_666 ns/tick）：
    /// 每过一个 tick 周期补 1 个 tick，余数留到下一帧（帧率抖动不会丢/多 tick）。
    #[test]
    fn drive_frame_advances_at_the_fixed_60hz_step() {
        const P: u64 = crate::graphwar_tick_handler::NANOS_PER_TICK as u64;
        // 正好一个周期（用纳秒精确表示，避免 `1.0/60.0` 的 0.67 ns 表示误差）⇒ 1 个 tick、余数 0
        let step_s = P as f64 / crate::graphwar_tick_handler::NANOS_PER_SECOND as f64;
        assert_eq!(ticks_for_frame(0, step_s, 600), (1, 0));
        // 1/60 s 的 f64 表示是 16666666.67 ns ⇒ round **进到** 16666667 ⇒ 余 1 ns
        // （原版同样按纳秒记累加器，1 ns 的余数不丢，下一帧会补进去）
        assert_eq!(ticks_for_frame(0, 1.0 / 60.0, 600), (1, 1));
        // 一秒 = 1e9 ns ⇒ 60 个 tick（60×16666666 = 999999960）⇒ 余 40 ns
        assert_eq!(ticks_for_frame(0, 1.0, 600), (60, 40));
        // 半周期 ⇒ 0 个 tick，余数 = 半个周期
        assert_eq!(ticks_for_frame(0, 0.5 / 60.0, 600), (0, P / 2));
        // 余数跨帧累计：两次半周期 ⇒ 1 个 tick
        let (_, rest) = ticks_for_frame(0, 0.5 / 60.0, 600);
        assert_eq!(ticks_for_frame(rest, 0.5 / 60.0, 600), (1, 0));
        // 卡顿保护：一帧最多 600 个，超出把余数清零（不滚雪球）
        assert_eq!(ticks_for_frame(0, 100.0, 600), (600, 0));
        // 非有限 / 负数 ⇒ 不动
        assert_eq!(ticks_for_frame(0, f64::NAN, 600), (0, 0));
        assert_eq!(ticks_for_frame(0, -1.0, 600), (0, 0));
        assert_eq!(ticks_for_frame(P / 3, f64::INFINITY, 600), (0, P / 3));
    }
}
}

#[cfg(feature = "gdext-bindings")]
pub use node_class::*;
