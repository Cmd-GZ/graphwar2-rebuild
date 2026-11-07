//! **不是原版的东西**：把复原的 [`crate::obstacle_drawer`] 挂成**真 Godot 节点类**的绑定外壳。
//!
//! # 为什么会有这一份
//!
//! 原版 `ObstacleDrawer`（`src/obstacle_drawer.rs`，arm64 `update` = `0x422590` / 4688 B）
//! 是**唯一**把障碍物送上屏的东西：它把 1000×600 的掩膜拼成一张 **`LA8`** 的 `Image`
//! （`Image::create_from_data(1000, 600, false, 1, ..)`），`ImageTexture` 上传，
//! 再交给**一个** `Sprite2D`，按平面的 `Control::get_size()` 摆位置与缩放。
//! 纯逻辑模块只留了地址表/常量表（它按约定**不碰引擎**），本文件是那层薄壳：
//!
//! ```text
//! src/obstacle_drawer.rs                 纯逻辑：Location / 字面量 / 调用表 / 每帧 2 次
//! src/plane.rs                           纯逻辑：掩膜光栅化（rasterize / empty_mask）
//! src/_scaffold/obstacle_drawer_class.rs ← 本文件：真的拼 Image、真的建 ImageTexture 与 Sprite2D
//! ```
//!
//! **算术本体只有一份**：掩膜仍是 `plane::rasterize`，像素映射仍是本文件那两个有地址的常量
//! （[`OBSTACLE_LUMA`] / [`alpha_of`]，见下），本文件不复制任何复原出来的算式。
//!
//! # 基类：**原版没有基类**（这条必须先说清，别把它当成"已复原的 Control"）
//!
//! 任务书问"基类要有证据"。本轮查完的答案是：**原版 `ObstacleDrawer` 根本不是 Godot 类**，
//! 所以它没有基类可抄。证据（**观测**，arm64 2.0.60 未 strip）：
//!
//! | 判据 | 结果 | 说明 |
//! |---|---|---|
//! | `__registration_storage::STORAGE` 符号 | **12 个**，`ObstacleDrawer` **不在其中** | 该符号是 gdext 给"注册成 Godot 类"的类型生成的；12 个与 `docs/16` §35.3 那张表逐个对上 |
//! | `impl I<Base>`（符号名里的 `gen::classes::<base>::`） | **11 个类**，`ObstacleDrawer` **不在其中** | 同 §35.3 的复现命令 |
//! | `.rodata` 里的类名字面量（`GraphwarAdSwitch` 那种 `type_name` 游程） | **没有** `ObstacleDrawer` | 全文件只有 4 处 `ObstacleDrawer`，全是 `.strtab`/`.rodata` 里的 **mangled 符号名** |
//! | 运行时的真·节点树 dump（`docs/16` §2.1.10） | `GraphwarPlane` 的子节点只有 `@AxisDrawer@N` / `@EntityDrawer@N` / `@EffectDrawer@N` / `@Line2D@N` | **没有** `@ObstacleDrawer@N` |
//! | `GraphwarPlane::init`（`0x4b7380`）里有 `obstacle_drawer.rs:37:97` 的 Location | 有 | ⇒ 它是在平面 `init` 里**当普通字段构造**的（`src/obstacle_drawer.rs` 的 `FUNCTIONS[1]` 已记） |
//!
//! 正对照（证明扫法有效）：同一个命令扫出来的 12 个类里既有 `entity_drawer::EntityDrawer`
//! 也有 `function_drawer` **之外**的 11 个 `I<Base>`；而 `FunctionDrawer` 与 `ObstacleDrawer`
//! 都**不在**那 12 个里 —— 这不是"我扫漏了"，是一条**有界**的负面结论。
//!
//! ⇒ 基类 `Control` 是**推断 + 我方选择**，两条理由（都不是原版证据）：
//!
//! 1. **同族**：四个 `*Drawer` 里凡是真注册成类的（`AxisDrawer` / `EffectDrawer` / `EntityDrawer`）
//!    基类**全是 `Control`**（§35.3 观测）；`ObstacleDrawer` 与它们同属"平面上的绘制层"。
//! 2. **它按 `Control` 的语义布局**：`update` 里那两次 `Control::get_size()` 拿的是**平面的**
//!    尺寸（`0x422608` / `0x42262c`），Sprite2D 的 `position` / `scale` 直接由它算出（见下）。
//!    `Control` 是唯一"有 `get_size()` 的容器"。
//!
//! **缺的证据**（写清楚，不硬说定死）：原版那个结构体在场景树里**没有对应节点**，
//! 所以"它该是 `Control` 还是 `Node2D`"没有可观测对象；要定死只能拿到 2024 版带
//! `impl I<…>` 的符号表的二进制，或抓到原版 `GraphwarPlane::init` 里那次构造的
//! `Gd::default_instance` 目标类名。⇒ 本类的 `Control` 是**推断**。
//!
//! # 本轮从指令里读出来的东西（**观测 / 逐位**，都带地址）
//!
//! ## (a) 结构体只有 4 个字段（**观测**：`drop_in_place<ObstacleDrawer>` @ `0x39518c`，268 B）
//!
//! 那个 drop glue 只碰 3 个需要析构的字段 + 1 个不需要析构的，拼起来正好是一条 24/24/16/24 的链：
//!
//! ```text
//! +0x00  Gd<ImageTexture>     24 B   drop: upcast_ref<RefCounted> + unreference（0x3951bc）
//! +0x18  Gd<Sprite2D>         24 B   drop: 无（Node 不是 RefCounted；`Gd<Node>` 不保活）
//! +0x30  PackedByteArray      16 B   drop: PackedElement::ffi_destroy（0x3951f8）
//! +0x40  Gd<Image>            24 B   drop: 同上（cleanup 路径里直接写 drop_in_place<Gd<Image>>）
//! ```
//!
//! ⇒ **只有一张图、一个贴图、一个 sprite**（不是"每个障碍一个 Sprite2D"），
//! 且 **结构体里没有 `HashMap`、没有父节点句柄** ⇒ `src/obstacle_drawer.rs` 文档里那句
//! "`hashbrown::rustc_entry` ×3 按 key 缓存那些节点"读作"缓存的是**障碍数据**里的表"更合适
//! —— 那 3 次 entry 的 map 不在本结构体的字段里（本文件只照实测的 4 个字段落）。
//!
//! ## (b) 图是 **1000×600 / `LA8` / 无 mipmap**（**逐位**）
//!
//! 两处 `Image::create_from_data`（`0x422780` / `0x422d1c`）的实参完全相同：
//!
//! ```text
//! 0x422764  mov w0, #0x3e8     ; 1000
//! 0x422768  mov w1, #0x258     ; 600
//! 0x42276c  mov w2, wzr        ; use_mipmaps = false
//! 0x422770  mov w3, #0x1       ; Format = 1 = LA8（Godot 4：L8=0 / LA8=1 / R8=2 / RG8=3 / RGB8=4 / RGBA8=5）
//! 0x422760  add x4, x19, #0x30 ; &self.pixels（+0x30 那个 PackedByteArray）
//! ```
//!
//! 600×1000 与 `plane::CANVAS_W/H` 一致 ⇒ 掩膜像素与图片像素**一对一**。
//!
//! ## (c) 掩膜字节 → 像素：**L = 0、A = `!m`**（**逐位**）
//!
//! `0x422cbc`–`0x422cf0` 那个循环（每个 mask 字节 push 两个字节）：
//!
//! ```text
//! 0x422cbc  ldrb  w23, [x22, x20]   ; m = mask[i]
//! 0x422cc0  sturh wzr, [x29,#-0xa8] ; pair = [0, 0]
//! 0x422ccc  bl    op_push_back      ; 推 L = 0（`sturb wzr` 写的就是它）
//! 0x422cd0  mvn   w8, w23           ; !m
//! 0x422cd8  sturb w8, [x29,#-0xa7]  ; pair[1] = !m
//! 0x422ce4  bl    op_push_back      ; 推 A = !m
//! ```
//!
//! 掩膜口径（`plane::rasterize`）：`0x00` = 障碍、`0xFF` = 空 ⇒ **障碍 = 不透明黑（0,255）、
//! 空 = 全透明（0,0）**。同一个循环在另一条路径上改用 `ffi_index_mut(2*(row*1000+col)+1)`
//! （`0x422c2c`）直接写 **A 通道**，写的还是 `mvn`（`0x422c38`）——两条路径的映射一致。
//!
//! ## (d) Sprite2D 的摆法（**观测 / 逐位**）
//!
//! ```text
//! 0x422608 / 0x42262c  Control::get_size ×2      ; 平面的像素尺寸（原版调两次，本类调一次，同值）
//! 0x42260c  fmov s2, #0.5                        ; 0.5
//! 0x42261c  fmul s8, s0, s2  / fmul s9, s1, s2   ; position = size * 0.5（居中）
//! 0x422630  mov w8, #0x447a0000                  ; 1000.0f32
//! 0x422644  fdiv s10, s0, s1                     ; k = size.x / 1000
//! 0x42266c  fneg s1, s10                         ; scale.y = -k（永远取负）
//! 0x422678  fcsel s0, s1, s10, ne                ; scale.x = (w5 != 0) ? -k : k
//! ```
//!
//! 那个 `w5` 是第 5 个入参（`0x4225fc` 存进 `w20`），语义**未知**（本类命名 [`ObstacleDrawer::flip_x`]）。
//!
//! ## (e) `update` 的入参形状（**观测**，语义多半**未知**）
//!
//! ```text
//! x0        = &mut self
//! x1, x2    = 一个 16 B 的 RawGd（`stp x1,x2,[sp,#0x50]` → 0x4225dc），
//!             紧接着 `upcast_ref`（字面量 `"upcast_ref"`，`0x19d85a`，长 10）再 `Control::get_size`
//!             ⇒ **平面节点**
//! x3        = 指针（`mov x19, x3`），它的 `+0x40` 被当成 `GraphwarModuleContainer` 用
//!             （`ldr x19,[x19,#0x40]` → `get_module(x19+0x18)` @ `0x4226a0`）
//! x4        = 一份 `ptr/len` 借用（`[x23]`/`[x23+0x8]`/`[x23+0x20]`）——**障碍数据**那一侧
//! w5        = bool（见 (d)）
//! ```
//!
//! `add_child_full` 的父（`0x4231cc`~`0x423224`）是**传入的那个节点**再 `upcast_ref` 一次：
//! ⇒ 原版把 Sprite2D 挂到**平面上**（因为本结构体不是节点、也不存父句柄）。
//! **本类的差别**：把 sprite 挂成自己的子节点（任务书要求"`Sprite2D` 子节点"），
//! 于是 sprite 的坐标原点 = 本节点的原点；只要本节点贴在平面左上角（`position = (0,0)`），
//! `position = size * 0.5` 的语义与原版**逐像素相同**。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 基类 | **没有**（不是 Godot 类，是 `GraphwarPlane` 的字段） | `Control`（`add_child` 要求它是节点） | 原版**观测**（负面）；`Control` 是**推断** |
//! | 像素缓冲 | `PackedByteArray`（+0x30），逐字节 `op_push_back` | 先拼 `Vec<u8>` 再 `PackedByteArray::from(..)` 一次交出去 | 布局**观测**；批量是**我方选择**（逐字节 FFI 每帧 120 万次，不划算） |
//! | 贴图 | `ImageTexture::create_from_image` 一次 + `update` N 次（+0x00 那个字段就在为 `update` 留着） | 同：首次 `create_from_image`，之后 `Image::set_data` + `ImageTexture::update` | **观测**（两条 API 都在 `CALLED_API` 里） |
//! | `create_from_data` 每帧 2 次 | 是（`0x422780` / `0x422d1c`，两条路径） | 首次 1 次 + 之后 `set_data` 1 次/次 `update` | 次数**观测**；化简是**我方选择**（原版两条路径各管什么**未知**） |
//! | 谁喂掩膜 | `GraphwarObstacleManager` 的 `Mutex<GraphwarObstacleData>`（每帧锁一次） | 本类自己持一张掩膜（`add_obstacle` / `carve_hole` 改它），掩膜算法仍是 `plane::rasterize` | 锁**观测**；"自己持"是**化简**（那个 manager 还没节点化） |
//! | 脏矩形 | 由 `graphwar_plane` 持有；本模块入参里没有矩形 | **同**：本类也不收矩形，每次刷整张图 | 分工**观测**（`src/obstacle_drawer.rs` 的文档已记） |
//! | 每帧调几次 | 2（`process_plan()` = `(2, true)`） | 由平面调 2 次，本类只计数 | **观测** |
//! | 名字 | 无（不是节点，没有 `set_name` 的机会） | 不 `set_name` ⇒ 引擎自动名 `@ObstacleDrawer@N` | 我方选择（与 §40 的 `FunctionDrawer` 一致） |
//!
//! ⇒ 这个类证明的是「**掩膜能不能真的落成一张上屏的 `LA8` 贴图，并随掩膜变化而更新**」，
//! **不是**"障碍物已经画得和原版一样"：原版的**颜色**还没解（本类是黑色，见 (c)，
//! 那是从 `L=0` 逐位读出来的，不是我们挑的颜色），`w5` 的语义**未知**。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --features gdext-bindings
//! cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/obstacle_drawer_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! ObstacleDrawer : Control
//!   @export flip_x : bool                 # scale.x 取负（原版第 5 个入参 w5，语义未知）
//!
//!   add_obstacle(x, y, radius) -> int     # 掩膜画实心圆（fill 0x00），返回此刻被占像素数
//!   carve_hole(x, y, radius) -> int       # 掩膜挖洞（fill 0xFF），返回此刻被占像素数
//!   clear_obstacles() -> int              # 掩膜刷空（返回 0）
//!   update(plane: Control) -> bool        # 原版 update：拼图 → 贴图 → 摆 sprite
//!   mask_value_at(px, py) -> int          # 掩膜探针（0 / 255；越界 -1）
//!   occupied_pixels() -> int              # 掩膜里 != 0xFF 的像素数
//!
//!   child_count() -> int                  # 子节点数（首次 update 之后恒 1）
//!   sprite_count() -> int                 # 子节点里有几个 Sprite2D
//!   texture_is_null() -> bool
//!   image_size() -> Vector2i              # 恒 (1000, 600)
//!   image_format() -> int                 # 恒 1（LA8）
//!   sprite_position() / sprite_scale() -> Vector2   # 最近一次摆的值
//!   update_call_count() / image_build_count() / image_set_data_count() /
//!   texture_update_count() / sprite_build_count() -> int
//! ```
//!
//! 最后那组计数是**观测点**：没有它们，headless 里分不清"没刷"和"刷了但贴图没变"。

use godot::classes::image::Format;
use godot::classes::{Control, IControl, Image, ImageTexture, Node, Sprite2D, Texture2D};
use godot::obj::EngineEnum;
use godot::prelude::*;

use crate::obstacle_drawer as od;
use crate::plane::{self, CANVAS_H, CANVAS_W, EMPTY};

// ---------------------------------------------------------------------------
// 1. 从指令里读出来的常量（每条都带地址；**不是我方挑的数**）
// ---------------------------------------------------------------------------

/// 图的格式：Godot 4 的 `Format::LA8`（枚举值 **1**）。
///
/// **逐位**：`0x422770` / `0x422d18` 的 `mov w3, #0x1`（见文件头 (b)）。
pub const IMAGE_FORMAT: Format = Format::LA8;

/// `Format::LA8` 的枚举值（GDScript 侧看到的就是这个数）。
///
/// Godot 4 的 `Image::Format`：`L8 = 0`、`LA8 = 1`、`R8 = 2`、`RG8 = 3`、`RGB8 = 4`、`RGBA8 = 5`。
/// 验收脚本直接断言 `image_format() == 1`，抄错枚举会立刻红。
pub const IMAGE_FORMAT_LA8: i64 = 1;

/// 障碍像素的**亮度**分量：`0`（黑）。
///
/// **逐位**：`0x422cd4` 的 `sturb wzr, [x29,#-0xa8]`（push 之前那一半被清零），见文件头 (c)。
pub const OBSTACLE_LUMA: u8 = 0;

/// 空像素的亮度分量：同样是 `0`（那条 `sturb wzr` 对每个像素都执行）。
pub const EMPTY_LUMA: u8 = 0;

/// 掩膜字节 → alpha（`!m`，即 `255 - m`）。**逐位**：`0x422cd0` 的 `mvn w8, w23`。
///
/// 掩膜口径（`plane::rasterize` 的 `fill`）：`0x00` = 障碍、[`EMPTY`]（`0xFF`）= 空
/// ⇒ 障碍处 alpha = `!0x00` = **255（不透明）**，空处 alpha = `!0xFF` = **0（全透明）**。
pub fn alpha_of(mask_byte: u8) -> u8 {
    !mask_byte
}

/// `Sprite2D.position = 平面尺寸 × 它`。**逐位**：`0x42260c` 的 `fmov s2, #0.5`。
pub const SPRITE_POSITION_FACTOR: f32 = 0.5;

/// `scale` 的分母：**平面的宽 / 1000**。**逐位**：`0x422630` 的 `mov w8, #0x447a0000`（1000.0f32）。
pub const SCALE_DIVISOR: f32 = 1000.0;

/// 一个像素几个字节：`LA8` = 亮度 1 + alpha 1。
pub const BYTES_PER_PIXEL: usize = 2;

/// 一张图多少字节（`1000 × 600 × 2` = 1,200,000）。
pub const IMAGE_BYTES: usize = CANVAS_W as usize * CANVAS_H as usize * BYTES_PER_PIXEL;

/// 掩膜 → `LA8` 像素（**纯函数**，单测与验收脚本用的是同一份）。
///
/// 对每个掩膜字节 `m`：`(L, A) = (0, !m)`（见文件头 (c)）。
///
/// `mask` 长度应为 [`CANVAS_W`] × [`CANVAS_H`]；`out` 会被重写成 `2 × mask.len()` 字节。
/// 长度不匹配时按**较短的**那份算（不 panic：headless 里宁可少画也不 abort 整个进程）。
pub fn fill_pixels(mask: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(mask.len() * BYTES_PER_PIXEL);
    for &m in mask {
        out.push(OBSTACLE_LUMA);
        out.push(alpha_of(m));
    }
}

/// 平面尺寸 + `flip_x` → `(position, scale)`。
///
/// 逐条对应文件头 (d)：
///
/// * `position = size * 0.5`；
/// * `k = size.x / 1000`；
/// * `scale = (flip_x ? -k : k, -k)`（y **永远**取负——游戏 y 向上、画布 y 向下）。
///
/// 返回的是 `Sprite2D` 该设的值（本类直接喂给 `set_position` / `set_scale`）。
pub fn sprite_transform(size: (f32, f32), flip_x: bool) -> ((f32, f32), (f32, f32)) {
    let k = size.0 / SCALE_DIVISOR;
    let position = (size.0 * SPRITE_POSITION_FACTOR, size.1 * SPRITE_POSITION_FACTOR);
    let scale = (if flip_x { -k } else { k }, -k);
    (position, scale)
}

// ---------------------------------------------------------------------------
// 2. 类本体（注册 + 虚方法 + 属性 + 方法）
// ---------------------------------------------------------------------------

/// `ObstacleDrawer`：把 1000×600 的障碍掩膜落成一张 `LA8` `ImageTexture` + 一个 `Sprite2D`。
///
/// 字段**照原版那 4 个**（文件头 (a) 的 drop glue 证据）：
/// `texture` / `sprite` / `pixels` / `image`，只是像素缓冲从 `PackedByteArray` 换成
/// `Vec<u8>`（拼完一次性交给引擎，见对照表）。
///
/// **本类不实现 `ready` / `process`**：原版 `ObstacleDrawer` 只有 `update` 与 drop glue
/// （`src/obstacle_drawer.rs` 的 `FUNCTIONS`），调用它的节奏由平面给（每帧 2 次）
/// ⇒ 也不写 `set_process(true)`（见 `docs/16` §33.4 第 5 条）。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct ObstacleDrawer {
    base: Base<Control>,

    /// `scale.x` 取负（原版第 5 个入参 `w5`，**语义未知**，见文件头 (d)）。
    ///
    /// 默认 `false` ⇒ `scale = (k, -k)`，验收脚本按这个手算。
    #[export]
    flip_x: bool,

    /// 障碍掩膜（`CANVAS_W × CANVAS_H`），`0x00` = 障碍、`0xFF` = 空。
    ///
    /// 原版这张表在 `GraphwarObstacleManager` 的 `Mutex<GraphwarObstacleData>` 里
    /// （每帧锁一次，`0x4226e4` 那条 `ldxr`）；那个 manager 还没节点化 ⇒ 本类自己持一张，
    /// 改它的仍是 `plane::rasterize`（同一份纯逻辑）。
    mask: Vec<u8>,

    /// `LA8` 像素缓冲（原版 `+0x30` 的 `PackedByteArray`；这里是拼完再交出去的 `Vec<u8>`）。
    pixels: Vec<u8>,

    /// 原版 `+0x00`：`Gd<ImageTexture>`（`drop_in_place` 里 `upcast_ref<RefCounted>` + `unreference`）。
    texture: Option<Gd<ImageTexture>>,

    /// 原版 `+0x40`：`Gd<Image>`（cleanup 路径里那条 `drop_in_place<Gd<Image>>`）。
    ///
    /// 留着它是为了 `Image::set_data` + `ImageTexture::update`（原版 `CALLED_API` 里有 `update`）。
    image: Option<Gd<Image>>,

    /// 原版 `+0x18`：`Gd<Sprite2D>`（drop glue 里**没有**它 —— `Gd<Node>` 不保活）。
    sprite: Option<Gd<Sprite2D>>,

    /// 最近一次摆出去的 `position` / `scale`（观测点；headless 里比对用）。
    last_position: Vector2,
    last_scale: Vector2,

    /// `update` 被调用过几次（原版每帧 2 次）。
    update_calls: i64,
    /// `Image::create_from_data` 调过几次（原版每帧 2 次，见对照表）。
    image_builds: i64,
    /// `Image::set_data` 调过几次（本类替代"第二次 `create_from_data`"）。
    image_set_data: i64,
    /// `ImageTexture::create_from_image` / `update` 加起来几次。
    texture_uploads: i64,
    /// `Sprite2D::new_alloc` 调过几次（原版只在首次建，`add_child_full` 只在 `0x423224`）。
    sprite_builds: i64,
    /// `add_obstacle` / `carve_hole` / `clear_obstacles` 各调过几次。
    mask_edits: i64,
}

#[godot_api]
impl IControl for ObstacleDrawer {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            flip_x: false,
            mask: plane::empty_mask(),
            pixels: Vec::new(),
            texture: None,
            image: None,
            sprite: None,
            last_position: Vector2::ZERO,
            last_scale: Vector2::ZERO,
            update_calls: 0,
            image_builds: 0,
            image_set_data: 0,
            texture_uploads: 0,
            sprite_builds: 0,
            mask_edits: 0,
        }
    }

}

#[godot_api]
impl ObstacleDrawer {
    /// 加一个障碍：往掩膜里画实心圆（`fill = 0x00`），返回此刻**被占**的像素数。
    ///
    /// 算法是 `plane::rasterize`（与 `GraphwarObstacleManager::add_obstacle` `0x3f17a0` 同构，
    /// 已 unicorn 对拍）——本类**不重新实现**光栅化。
    #[func]
    fn add_obstacle(&mut self, x: f64, y: f64, radius: f64) -> i64 {
        plane::rasterize(&mut self.mask, x, y, radius, 0x00);
        self.mask_edits += 1;
        self.occupied_pixels()
    }

    /// 挖一个洞：同一条光栅化，`fill = [EMPTY]`（`0xFF`）。
    ///
    /// 纯逻辑侧对应 `GraphwarObstacleManager::destroy_obstacle`（`0x3f1b30`）里的那次
    /// `rasterize(mask, .., 0xFF)`。
    #[func]
    fn carve_hole(&mut self, x: f64, y: f64, radius: f64) -> i64 {
        plane::rasterize(&mut self.mask, x, y, radius, EMPTY);
        self.mask_edits += 1;
        self.occupied_pixels()
    }

    /// 清空掩膜（`GraphwarObstacleManager::clear_obstacles` `0x3f19a0`）。返回 `0`。
    #[func]
    fn clear_obstacles(&mut self) -> i64 {
        self.mask = plane::empty_mask();
        self.mask_edits += 1;
        self.occupied_pixels()
    }

    /// 原版 `ObstacleDrawer::update`（`0x3ee190`，5216 B）。
    ///
    /// 顺序照原版（文件头 (b)(d)）：
    ///
    /// 1. 读平面的 `Control::get_size()`（原版调 2 次，本类 1 次——同值）；
    /// 2. 掩膜 → `LA8` 像素；
    /// 3. `Image::create_from_data(1000, 600, false, LA8, ..)`（首次）/ `Image::set_data`（之后）；
    /// 4. `ImageTexture::create_from_image`（首次）/ `ImageTexture::update`（之后）；
    /// 5. `Sprite2D`（首次 `new_alloc` + `set_texture` + `add_child`）；
    /// 6. `set_position(size*0.5)` / `set_scale(±k, -k)`。
    ///
    /// 返回：这次**真的**把图刷上去了吗（贴图与 sprite 都在就是 `true`）。
    #[func]
    pub fn update(&mut self, plane: Gd<Control>) -> bool {
        self.update_calls += 1;

        let size = plane.get_size();
        let dims = (size.x, size.y);

        // ② 掩膜 → LA8 像素（映射与原版逐位一致：L=0、A=!m）。
        fill_pixels(&self.mask, &mut self.pixels);
        let data = PackedByteArray::from(&self.pixels[..]);

        // ③④ 图与贴图：首次 create，之后就地更新（原版留着 `Gd<Image>` 就是为了这条）。
        let first_time = self.image.is_none();
        if first_time {
            let Some(image) = Image::create_from_data(
                CANVAS_W as i32,
                CANVAS_H as i32,
                false,
                IMAGE_FORMAT,
                &data,
            ) else {
                godot_error!("ObstacleDrawer::update Image::create_from_data 返回空（尺寸非法？）");
                return false;
            };
            self.image_builds += 1;
            let Some(texture) = ImageTexture::create_from_image(&image) else {
                godot_error!("ObstacleDrawer::update ImageTexture::create_from_image 返回空");
                return false;
            };
            self.texture_uploads += 1;
            self.texture = Some(texture);
            self.image = Some(image);
        } else {
            if let Some(image) = self.image.as_mut() {
                image.set_data(
                    CANVAS_W as i32,
                    CANVAS_H as i32,
                    false,
                    IMAGE_FORMAT,
                    &data,
                );
                self.image_set_data += 1;
            }
            if let (Some(texture), Some(image)) = (self.texture.as_mut(), self.image.as_ref()) {
                texture.update(image);
                self.texture_uploads += 1;
            }
        }

        // ⑤ sprite：只建一次（原版 `add_child_full` 在 `0x423224`，一条路）。
        if self.sprite.is_none() {
            let Some(texture) = self.texture.clone() else {
                return false;
            };
            let mut sprite = Sprite2D::new_alloc();
            sprite.set_texture(&texture.upcast::<Texture2D>());
            let node = sprite.clone().upcast::<Node>();
            // 原版挂到**平面**上（文件头 (e)）；本类挂到自己身上（任务书要求子节点）。
            self.base_mut().add_child(&node);
            self.sprite_builds += 1;
            self.sprite = Some(sprite);
        }

        // ⑥ 摆位置 / 缩放（逐位常量在 [`sprite_transform`] 里）。
        let ((px, py), (sx, sy)) = sprite_transform(dims, self.flip_x);
        if let Some(sprite) = self.sprite.as_mut() {
            sprite.set_position(Vector2::new(px, py));
            sprite.set_scale(Vector2::new(sx, sy));
        }
        self.last_position = Vector2::new(px, py);
        self.last_scale = Vector2::new(sx, sy);

        self.texture.is_some() && self.sprite.is_some()
    }

    /// 掩膜探针：`(px, py)` 处的字节（`0` = 障碍、`255` = 空、`-1` = 越界）。
    ///
    /// 越界返回 `-1` 而**不是**"算命中"：`plane::is_occupied` 的口径（越界算撞墙）属于
    /// **碰撞**，这里只是探针，两者别混。
    #[func]
    fn mask_value_at(&self, px: i64, py: i64) -> i64 {
        if px < 0 || py < 0 || px >= CANVAS_W as i64 || py >= CANVAS_H as i64 {
            return -1;
        }
        match self.mask.get(py as usize * CANVAS_W as usize + px as usize) {
            Some(&v) => v as i64,
            None => -1,
        }
    }

    /// 掩膜里 `!= 0xFF` 的像素数（"有多少个像素被障碍挡住"）。
    #[func]
    fn occupied_pixels(&self) -> i64 {
        self.mask.iter().filter(|&&b| b != EMPTY).count() as i64
    }

    /// 子节点数（首次 `update` 之后恒 `1`）。
    #[func]
    fn child_count(&self) -> i64 {
        self.base().get_child_count() as i64
    }

    /// 子节点里有几个是 `Sprite2D`（原版只有一个，见文件头 (a)）。
    #[func]
    fn sprite_count(&self) -> i64 {
        let mut count = 0;
        for i in 0..self.base().get_child_count() {
            let Some(child) = self.base().get_child(i) else {
                continue;
            };
            if child.is_class("Sprite2D") {
                count += 1;
            }
        }
        count
    }

    /// 贴图还是空的吗（首次 `update` 之前是 `true`）。
    #[func]
    fn texture_is_null(&self) -> bool {
        self.texture.is_none()
    }

    /// 图的尺寸（恒 `(1000, 600)`；没建图时 `(0, 0)`）。
    #[func]
    fn image_size(&self) -> Vector2i {
        match self.image.as_ref() {
            Some(image) => image.get_size(),
            None => Vector2i::ZERO,
        }
    }

    /// 图的格式（恒 [`IMAGE_FORMAT_LA8`] = 1；没建图时 `-1`）。
    #[func]
    fn image_format(&self) -> i64 {
        match self.image.as_ref() {
            Some(image) => image.get_format().ord() as i64,
            None => -1,
        }
    }

    /// 最近一次 `set_position` 的值（观测点）。
    #[func]
    fn sprite_position(&self) -> Vector2 {
        self.last_position
    }

    /// 最近一次 `set_scale` 的值（观测点）。
    #[func]
    fn sprite_scale(&self) -> Vector2 {
        self.last_scale
    }

    /// `update` 被调用过几次。
    #[func]
    pub fn update_call_count(&self) -> i64 {
        self.update_calls
    }

    /// `Image::create_from_data` 调过几次。
    #[func]
    fn image_build_count(&self) -> i64 {
        self.image_builds
    }

    /// `Image::set_data` 调过几次。
    #[func]
    fn image_set_data_count(&self) -> i64 {
        self.image_set_data
    }

    /// `ImageTexture` 的 `create_from_image` + `update` 一共几次。
    #[func]
    fn texture_update_count(&self) -> i64 {
        self.texture_uploads
    }

    /// `Sprite2D` 建过几个（原版只有一个，恒 `<= 1`）。
    #[func]
    fn sprite_build_count(&self) -> i64 {
        self.sprite_builds
    }

    /// 掩膜被改过几次（`add` / `carve` / `clear` 之和）。
    #[func]
    fn mask_edit_count(&self) -> i64 {
        self.mask_edits
    }

    /// 原版每帧被 `GraphwarPlane::process` 调几次（来自纯逻辑
    /// [`obstacle_drawer::UPDATE_CALLS_PER_FRAME`] = 2）。
    ///
    /// 暴露它是为了让验收脚本**不写死 2**：数字只有一个出处。
    #[func]
    fn updates_per_frame(&self) -> i64 {
        od::UPDATE_CALLS_PER_FRAME as i64
    }

    /// 一帧里本类会产出几张图（纯逻辑 [`obstacle_drawer::images_per_frame`]，入参按
    /// 原版 `update` 体内 `create_from_data` 的**2 次**算）。
    #[func]
    fn images_per_frame(&self) -> i64 {
        od::images_per_frame(2) as i64
    }

    /// 贴图上某个像素的 alpha（`0..=255`）。没建图时 `-1`。
    ///
    /// 这条是**引擎侧**探针：它经过 `Image` 真的取到了像素，所以"挖洞之后贴图变了"
    /// 这件事不是只看我们的计数器。`LA8` 下 `Image::get_pixel` 的 alpha 就是 `A/255`。
    #[func]
    fn texture_alpha_at(&self, px: i64, py: i64) -> i64 {
        let Some(image) = self.image.as_ref() else {
            return -1;
        };
        if px < 0 || py < 0 || px >= CANVAS_W as i64 || py >= CANVAS_H as i64 {
            return -1;
        }
        let color = image.get_pixel(px as i32, py as i32);
        (color.a * 255.0 + 0.5) as i64
    }
}

// ---------------------------------------------------------------------------
// 3. 单测（**不碰引擎**：只测纯函数与"类用的是哪份常量"）
// ---------------------------------------------------------------------------
//
// 真正的验收在 `client/tools/obstacle_drawer_check.gd`（headless，真引擎）。
// 这里钉住的是"像素映射 / 摆法 / 每帧次数"这三份**有地址的**常量没被改跑偏。

#[cfg(test)]
mod tests {
    use super::*;

    /// 掩膜字节 → 像素：障碍不透明、空全透明（`0x422cd0` 的 `mvn`）。
    #[test]
    fn mask_byte_maps_to_luma_and_alpha() {
        assert_eq!(alpha_of(0x00), 255, "障碍：不透明");
        assert_eq!(alpha_of(EMPTY), 0, "空：全透明");
        assert_eq!(OBSTACLE_LUMA, 0);
        assert_eq!(EMPTY_LUMA, 0);

        let mut out = Vec::new();
        fill_pixels(&[0x00, EMPTY, 0x80], &mut out);
        assert_eq!(out, vec![0, 255, 0, 0, 0, 127], "每个掩膜字节 = (L=0, A=!m)");
        assert_eq!(out.len(), 3 * BYTES_PER_PIXEL);
    }

    /// 整张图的字节数 = 1000×600×2（原版 `create_from_data` 的实参，逐位）。
    #[test]
    fn image_bytes_match_the_1000x600_la8_immediates() {
        assert_eq!(CANVAS_W, 1000, "0x3e8");
        assert_eq!(CANVAS_H, 600, "0x258");
        assert_eq!(IMAGE_FORMAT_LA8, 1, "Format::LA8 = 1");
        assert_eq!(IMAGE_BYTES, 1_200_000);

        let mask = plane::empty_mask();
        assert_eq!(mask.len(), CANVAS_W as usize * CANVAS_H as usize);
        assert!(mask.iter().all(|&b| b == EMPTY), "空掩膜全是 0xFF");

        let mut out = Vec::new();
        fill_pixels(&mask, &mut out);
        assert_eq!(out.len(), IMAGE_BYTES, "空掩膜也产出整张图（全透明）");
        assert!(out.chunks(2).all(|px| px == [0, 0]), "全透明像素");
    }

    /// Sprite2D 的摆法：`position = size*0.5`、`scale = (±size.x/1000, -size.x/1000)`。
    #[test]
    fn sprite_transform_matches_the_immediates() {
        let ((px, py), (sx, sy)) = sprite_transform((500.0, 300.0), false);
        assert_eq!((px, py), (250.0, 150.0), "0x42261c / 0x42262c 的 ×0.5");
        assert_eq!((sx, sy), (0.5, -0.5), "k = 500/1000、y 永远取负");

        let ((px2, py2), (sx2, sy2)) = sprite_transform((1000.0, 600.0), true);
        assert_eq!((px2, py2), (500.0, 300.0));
        assert_eq!((sx2, sy2), (-1.0, -1.0), "flip_x ⇒ scale.x 取负（0x422678 的 fcsel）");
    }

    /// 每帧次数来自纯逻辑（`process_plan()` 说 2 次；`create_from_data` 体内 2 次）。
    #[test]
    fn per_frame_counts_come_from_the_pure_logic() {
        assert_eq!(od::UPDATE_CALLS_PER_FRAME, 2);
        assert_eq!(od::images_per_frame(2), 4);
        // 纯逻辑里 `update` 的 x86 地址就是它唯一的方法符号（`0x3ee190`）。
        assert_eq!(od::func("update").map(|f| f.x86), Some(0x003e_e190));
    }
}
