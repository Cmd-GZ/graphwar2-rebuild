//! **不是原版的东西**：把复原的 [`crate::entity_drawer`] 挂成**真 Godot 节点类**的绑定外壳。
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
//! src/_scaffold/entity_drawer_class.rs   ← 本文件：建节点、读引擎尺寸、把算式结果写回节点
//! ```
//!
//! 算术本体**只有一份**，仍然在 `src/entity_drawer.rs` 里；本文件不复制任何算式。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 贴图 | `GraphwarTextureLoader` 的三张 `HashMap<String, Gd<Texture2D>>`（`+0x150/+0x180/+0x1b0`）按名字 `memcmp` 取 | **程序生成的 8×8 纯色 `ImageTexture`**（颜色由我方选定） | 原版是**观测**；纯色是**我方选择**（不依赖原版 pck） |
//! | 名字文本 | `GraphwarNameManager` / backtracer | 调用方用 [`EntityDrawer::set_entity_name`] 给 | **未解**（名字从哪来不归本模块） |
//! | 名字标签预制体 | `load("res://name_label.tscn").instantiate_as::<RichTextLabel>()` | **不加载场景**，按 `recovered-src/name_label.tscn` 的 6 个属性手工造（[`new_name_label`]） | 场景属性是**观测**（pck 回收资源）；属性语义是**推断** |
//! | 名字标签的 `theme` | 内联 `Theme` + `StyleBoxFlat`（bg/border 全 alpha 0） | 同值重建 | **观测**（`recovered-src/name_label.tscn`） |
//! | `ready` | 连 `resized` 信号 + 订 **6 个**后端事件 | 只连 `resized`；6 个事件**未接**（`GraphwarBackend` 还没有） | 信号名/方法名是**观测**（原版常量）；事件订阅**未做** |
//! | `process` | 每帧 `update_sprite` ×3、`update_name_label` ×2、死亡淡出、4 次 `HashMap::retain` | 每帧重摆所有实体（`relayout`）；`retain` 的判据仍是 `entity_drawer::UNKNOWNS` 第 2 条 | 布局是**观测**；清理判据**未知** |
//! | `draw` | 三段弧 + 三个箭头（准星）；角度 / 线宽 / 折点 / 抗锯齿全是立即数 | **实现了**：`entity_drawer::entity_primitives` 的 6 条图元 → 3 次 `draw_polygon`（箭头）+ 3 次 `draw_arc`（弧） | 准星几何 **观测**（`docs/archive/140-plane-visual.md` §2 逐条立即数）；坐标单位是**推断**（见下两行） |
//! | 准备圈的**输入** | `draw` 从两张哈希表取中心的 x/y、半径、时钟 `V`、`map[i]`（栈槽 `[rsp+0xb0]/[rsp+0xa8]`、`[rsp+0x90]`，`map[i]` 在 `0x4b34d3`–`0x4b35ae` 那段读） | 调用方用 `set_ring()` / `set_ring_base()` 给 | 原版**观测**（`0x4b2e94`–`0x4b35b4`）；本类**我方接线**（`GraphwarBackend` 的 `pos_info_event` 没接） |
//! | 准备圈的**坐标** | `PosInfo` 的第 1/2/3 个 f64 是否已是画布像素：**未知** | 按 [`ring_center_px`] 那条棋盘 → 像素投影（`k = size.x / 50`，与 sprite 同一条算式），半径乘同一个 `k` | **推断**（缺"PosInfo 的单位"这一条证据） |
//! | 准备圈的**颜色** | 哈希表里的打包 RGB（`0x4b347a`） | 导出属性 `team_color`（`relayout` 也用它） | **我方接线**（队色表 `get_team_overview` 没接，`docs/16-client.md` §49.5 第 7 条） |
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
//!   @export texture_size : int    # 程序生成贴图的边长（像素），默认 8
//!
//!   spawn_entity(id: int, x: float, y: float) -> int   # 建 4 个子节点并摆位，返回建了几个（恒 4）
//!   remove_entity(id: int) -> bool                     # queue_free 那 4 个子节点并丢掉句柄
//!   entity_count() -> int
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

use std::collections::BTreeMap;

use godot::classes::image::Format;
use godot::classes::{
    canvas_item::TextureFilter, text_server::AutowrapMode, Control, IControl, Image, ImageTexture,
    RichTextLabel, Node, Sprite2D, StyleBoxFlat, Texture2D, Theme,
};
use godot::prelude::*;

use crate::entity_drawer::{self, EntityChildNode, Vec2 as BoardVec2};

// ---------------------------------------------------------------------------
// 1. 程序生成贴图（替代原版从 pck 取的皮肤 / 脸 / 帽子）
// ---------------------------------------------------------------------------

/// 程序生成贴图的默认边长（像素）。8×8 是**我方选择**（原版贴图尺寸在 pck 里，见 `docs/16` §21）。
pub const DEFAULT_TEXTURE_SIZE: i32 = 8;

/// 默认队伍色：纯红（打包 `u32` 的低字节 = R ⇒ `0x000000ff`）。
///
/// 这个值让 [`entity_drawer::team_mirror`] 返回 `false`（镜像符号 `+1`），
/// 所以验收脚本里所有位置都能手算——**不是**从原版读出来的默认值。
pub const DEFAULT_TEAM_COLOR: i64 = 0x0000_00ff;

/// 三层贴图的纯色（RGBA，`0..=1`）。**我方选择**：原版是 pck 里的真贴图，
/// 这里只要"能拿到 `Gd<Texture2D>` 并挂上去"。
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
// 1. **坐标**：棋盘坐标 → 控件像素。原版 `draw` 里 `[rsp+0xb0]/[rsp+0xa8]` 是不是已经是像素
//    **未知**（`docs/140` §2.2 只读到"来自士兵坐标"）；这里复用 [`ring_center_px`]
//    —— 也就是 `sprite_transform` 那条**同一份**投影（`k = size.x/50`、y 轴翻转、镜像符号）。
//    半径按**同一个 `k`** 放大（假设 `pos_radius` 与中心的 x/y 同单位，因为它们同属一个 `PosInfo`）。
//    **推断**：若哪天证出 `PosInfo` 已经是像素，把这里的 `k` 换成 1.0 即可，算术一个字不用动。
// 2. **颜色**：原版从哈希表取打包 RGB；这里用导出属性 `team_color`（**我方接线**）。

/// 一具士兵一次 `_draw` 发出的图元条数：**3 个箭头（`draw_polygon`）+ 3 段弧（`draw_arc`）**。
///
/// 顺序取 [`entity_drawer::entity_primitives`]（= 代码地址序：箭头 `0x4b30b7`/`0x4b30e8`/`0x4b3119`
/// 在前，弧 `0x4b366c`/`0x4b36fd`/`0x4b378a` 在后）。**观测**
pub const RING_PRIMITIVES_PER_ENTITY: usize = 6;

/// 棋盘坐标 → **控件像素**：`(size.x/2 + s·k·x, size.y/2 − k·y)`。
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
/// 半径用它放大：`r_px = drawn_radius(pos_radius) × k`。**推断**（同 [`ring_center_px`] 的说明）。
pub fn ring_canvas_scale(size: BoardVec2) -> f64 {
    (size.x as f32 / entity_drawer::PIXELS_PER_UNIT_DIVISOR) as f64
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

    /// 程序生成贴图的边长（像素）。
    #[export]
    texture_size: i64,

    /// 实体 id → 4 个子节点。`BTreeMap` 而不是 `HashMap`：遍历顺序稳定，日志/断言可复现。
    entities: BTreeMap<i64, EntityNodes>,

    /// 贴图缓存：`GraphwarTextureLoader` 方法名 → `Gd<Texture2D>`（懒建，**每个类实例一份**）。
    ///
    /// 原版是 loader 里的三张 `HashMap`（全局共享）；这里退化成每实例一份，
    /// 因为我们还没有 `GraphwarTextureLoader` 这个节点。
    textures: BTreeMap<&'static str, Gd<Texture2D>>,

    /// 准备态：实体 id → 这一帧的 `PosInfo` + 时钟 `V`。
    ///
    /// `_draw` **只画这张表里的实体**（原版 `draw` 也只遍历自己那两张哈希表）。
    /// 空表 ⇒ 一个绘制图元都不发 —— 这就是"关掉准备态就不画"的那条分支。
    prep: BTreeMap<i64, PrepRing>,

    /// `_draw` 被引擎调用过几次（观测点）。
    draw_calls: i64,
    /// 累计发出的 `draw_arc` 条数（观测点）。
    arc_total: i64,
    /// 累计发出的 `draw_polygon` 个数（观测点）。
    polygon_total: i64,
    /// 最近一次 `_draw` 发出的弧实参（**引擎真正收到的 f32 值**）。
    last_arcs: Vec<ArcRecord>,
    /// 最近一次 `_draw` 发出的箭头顶点（**画布像素**）。
    last_polys: Vec<Vec<Vector2>>,

    /// `_ready` 被调用过几次（观测点）。
    ready_calls: i64,
    /// `_process` 被调用过几次（观测点）。
    process_calls: i64,
    /// `on_resized` 被调用过几次（观测点）。
    resized_calls: i64,
}

#[godot_api]
impl IControl for EntityDrawer {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            team_color: DEFAULT_TEAM_COLOR,
            texture_size: DEFAULT_TEXTURE_SIZE as i64,
            entities: BTreeMap::new(),
            textures: BTreeMap::new(),
            prep: BTreeMap::new(),
            draw_calls: 0,
            arc_total: 0,
            polygon_total: 0,
            last_arcs: Vec::new(),
            last_polys: Vec::new(),
            ready_calls: 0,
            process_calls: 0,
            resized_calls: 0,
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
    /// 本文件新增的两步（都是接线，不是算术）：
    ///   center   棋盘坐标 → 画布像素  = ring_center_px(…)
    ///   radius   × k（k = size.x/50）= ring_canvas_scale(…)
    /// ```
    ///
    /// ⚠️ **要画东西得先有准备态**：`prep` 空表时这里一条图元都不发（原版也只看自己的哈希表）。
    /// 谁来填这张表 = 还没接的 `pos_info_event`（`entity_drawer::READY_EVENTS` 第 1 个）。
    fn draw(&mut self) {
        self.draw_calls += 1;

        // ① 先把这一帧要发的图元**全部算成实参**：纯逻辑（`entity_primitives`）+ 投影。
        //    `self.base_mut()` 会独占借用 `&mut self`，那段里碰不到 `self.xxx`（`E0499`）。
        let size = self.base().get_size();
        let control_size = BoardVec2::new(size.x as f64, size.y as f64);
        let packed = self.team_color as u32;
        let scale = ring_canvas_scale(control_size);
        let ring_color = entity_drawer::Color::from_packed_rgb(packed);

        let mut arcs: Vec<ArcRecord> = Vec::new();
        let mut polys: Vec<(Vec<Vector2>, Color)> = Vec::new();

        for prep in self.prep.values() {
            let center_px = ring_center_px(control_size, packed, prep.board);
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
                            center: (center_px.x as f32, center_px.y as f32),
                            // `radius` 已经是 `f32(pos_radius) × 1.5f32`，这里只差棋盘→像素那一乘。
                            radius: (radius * scale) as f32,
                            // 角度**不改**：原版直接送 `A_i ∓ H`（`subsd`/`addsd` 之后 cvtsd2ss）。
                            start_angle: start_angle as f32,
                            end_angle: end_angle as f32,
                            point_count: point_count as i32,
                            width: width as f32,
                            antialiased,
                            color: (arc_color.r, arc_color.g, arc_color.b, arc_color.a),
                        });
                    }
                    entity_drawer::Primitive::Polygon { points, colors } => {
                        // 顶点在棋盘坐标里算（`r`、`r+0.5`、`θ∓6°`），逐点投影到画布像素。
                        let pts: Vec<Vector2> = points
                            .iter()
                            .map(|p| {
                                let q = ring_center_px(control_size, packed, *p);
                                Vector2::new(q.x as f32, q.y as f32)
                            })
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
            let mut base = self.base_mut();
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
        }

        self.arc_total += arc_n;
        self.polygon_total += poly_n;
        self.last_arcs = arcs;
        self.last_polys = polys.into_iter().map(|(p, _)| p).collect();
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

        // 顺序**直接取**原版常量：皮肤 → 脸 → 帽子 → 名字（叠放顺序就是 `add_child` 的先后）。
        for kind in entity_drawer::ENTITY_CHILD_NODES {
            if kind.is_sprite() {
                // **不显式命名**：`docs/16` §2.1.10 的节点树 dump 里这三个 sprite 就叫
                // `@Sprite2D@N`（Godot 的自动名）⇒ 原版也没给它们起名字。**观测**
                let mut sprite = Sprite2D::new_alloc();

                // 贴图：原版从 `GraphwarTextureLoader` 按名字取；这里程序生成（见文件头对照表）。
                if let Some(texture) = self.texture_for(kind) {
                    sprite.set_texture(&texture);
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

        for entity in self.entities.values_mut() {
            // 三个贴图层：位置 / 缩放 / 水平翻面全部来自 `entity_drawer::sprite_transform`。
            for sprite in &mut entity.sprites {
                // 原版每一层都读自己贴图的宽度（`Texture2D::get_width`）。
                let tex_width = sprite
                    .get_texture()
                    .map(|t| t.get_width().max(1) as u32)
                    .unwrap_or(1);
                let transform = entity_drawer::sprite_transform(
                    control_size,
                    packed,
                    tex_width,
                    entity.board.x,
                    entity.board.y,
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
            let alpha = match entity.dead_ticks {
                None => 1.0,
                Some(ticks) => entity_drawer::death_alpha(ticks),
            };
            let label_size = entity.name_label.clone().get_size();
            let min_size = entity.name_label.clone().get_minimum_size();
            let layout = entity_drawer::name_label_layout(
                BoardVec2::new(label_size.x as f64, label_size.y as f64),
                BoardVec2::new(min_size.x as f64, min_size.y as f64),
                packed,
                entity.board.x,
                entity.board.y,
                alpha,
            );
            entity.name_label.set_position(Vector2::new(
                layout.position.x as f32,
                layout.position.y as f32,
            ));
            entity.name_label.set_modulate(Color::from_rgba(
                layout.modulate.r,
                layout.modulate.g,
                layout.modulate.b,
                layout.modulate.a,
            ));
            // 原版：`t >= 60` 走"藏起来"那条分支（见 `entity_drawer::death_alpha` 的文档）。
            entity.name_label.set_visible(layout.modulate.a > 0.0);
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

    /// 最近一次 `_draw` 的第 `index` 条弧的**全部实参**，按 `draw_arc` 的形参序：
    ///
    /// ```text
    /// [cx, cy, radius, start_angle, end_angle, point_count, width, antialiased, r, g, b, a]
    /// ```
    ///
    /// 都是 **f32 加宽成 f64**的值 = 引擎真正收到的数。越界返回空数组。
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

    /// 懒建 + 缓存某一层的程序生成贴图。
    ///
    /// 返回 `Gd<Texture2D>` 的**克隆**：`Gd` 是引用计数智能指针（`ImageTexture` 是 `RefCounted`），
    /// 缓存里留一份、给 sprite 挂一份，两份都有效。
    fn texture_for(&mut self, kind: EntityChildNode) -> Option<Gd<Texture2D>> {
        let method = kind.texture_method()?;
        if let Some(texture) = self.textures.get(method) {
            return Some(texture.clone());
        }
        let texture = solid_texture(self.texture_size as i32, layer_color(method))?;
        self.textures.insert(method, texture.clone());
        Some(texture)
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
    /// ⇒ `scale = (16/60, (10/−60)·16) = (0.2667, −2.6667)`、`flip_h = true`。
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
        assert!((t.scale.x - 16.0 / 60.0).abs() < 1e-6, "sx={}", t.scale.x);
        assert!((t.scale.y + 160.0 / 60.0).abs() < 1e-6, "sy={}", t.scale.y);
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
}
