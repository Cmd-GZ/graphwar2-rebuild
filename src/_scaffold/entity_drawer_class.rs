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
//! | `draw` | 三段弧 + 三个箭头（准星） | **没实现**（`_draw` 一个 `draw_*` 都不调） | 准星几何是**观测**（`entity_drawer::ring_primitives`），但本切片只做子节点 |
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
//!   ready_call_count() / process_call_count() / resized_call_count() -> int
//! ```
//!
//! 最后三个计数是**观测点**：没有它们，headless 里分不清"引擎没回调"和"回调了但没干活"。
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
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;
        self.relayout();
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
    #[func]
    fn remove_entity(&mut self, id: i64) -> bool {
        let Some(entity) = self.entities.remove(&id) else {
            return false;
        };
        for sprite in &entity.sprites {
            sprite.clone().upcast::<Node>().queue_free();
        }
        entity.name_label.clone().upcast::<Node>().queue_free();
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
}
