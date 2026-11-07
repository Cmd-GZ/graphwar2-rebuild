//! **不是原版的东西**：把复原的 [`crate::player_showcase`] 挂成**真 Godot 节点类**的薄壳。
//!
//! # 为什么会有这一份
//!
//! 原版的 `PlayerShowcase` 是**玩家名片预览**（`docs/16-client.md` §25）：四个方法
//! `ready` / `on_text_change` / `process` / `draw`，每件都能指到指令：
//!
//! | 方法 | 原版干什么 | 档位 |
//! |---|---|---|
//! | `ready` | 取 `GraphwarRootNode` → 从 `GraphwarPlayerConfig`（根节点 `+0x16a8`）读**名字** → 灌进 `PlayerNameLineEdit` → **再取一次同一个控件** → `text_changed` 连到自己的 `on_text_change` | **观测**（6 步、两次 `get_node_by_name`） |
//! | `on_text_change` | 名字改回档案（`String::from`）→ `save()` → 变更计数器 **+1** | **观测** |
//! | `process` | `is_visible_in_tree()` 闸门 → 读 `+0xa8` 计数器 → 变了才 `queue_redraw()` | **观测** |
//! | `draw` | 依次画 **皮肤 → 脸 → 帽子** 三层贴图，`draw_texture_rect_full` ×3、`modulate` 恒纯白 | **观测** |
//!
//! `src/player_showcase.rs` 已经把这些复原成纯逻辑（`Showcase::tick` / `draw_plan` /
//! `on_text_change_effect` / `READY_PLAN` / `CFG_*`），但按约定**不碰引擎**。本文件是那层壳：
//!
//! ```text
//! src/player_showcase.rs               纯逻辑：档案字段偏移 / 6 步 ready / 闸门 / 三次绘制
//! src/graphwar_player_config.rs        纯逻辑：GraphwarPlayerConfig（名字 + 三个外观名 + 计数）
//! src/_scaffold/player_showcase_class.rs ← 本文件：持档案、建控件、真的 draw_texture_rect
//! ```
//!
//! # ⚠️ 一处与任务书不同的地方（按证据改，并说明改了什么）
//!
//! 任务书要求「读配置 + 建 **3 个 `Sprite2D` + 1 个标签**」。按 `docs/16` §25 与
//! `src/player_showcase.rs` 的观测，**原版没有这三个 `Sprite2D`**：它的 `draw` 是
//! `draw_texture_rect_full` ×3 画在**自己**身上（`0x4402b0` / `0x44037c` / `0x440444`），
//! 子节点只有 `ready` 里**取**（不是建）的那个 `PlayerNameLineEdit`。
//! 「三个 `Sprite2D` + 一个名字标签」是**兄弟类 `EntityDrawer`** 的形状（§24），两者被记混了。
//!
//! ⇒ 本类的做法：
//!
//! * **默认路径 = 证据**：`_draw` 里三次 `draw_texture_rect`（`draw_plan` 给的顺序与实参）；
//! * 任务书要的 3 个 `Sprite2D` + 1 个标签**仍然提供**，但作为**显式 opt-in 的演示口**
//!   [`PlayerShowcase::build_demo_children`]（默认不建），并在文件与文档里标明**非原版**。
//!   这样"默认跑起来 == 原版形状"，而"任务书要的那个形状"也有一个可断言的方法。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 三层贴图 | `GraphwarTextureLoader`（根节点 `+0x16a0`）的三张 `HashMap`，按**外观名**取 | **程序生成的 8×8 纯色 `ImageTexture`**，颜色按层固定（我方选择）；名字**只**决定"这一层有没有"（空名 ⇒ 无贴图 ⇒ 不画，与 `draw_plan` 的 `if let Some(t)` 同构） | 原版**观测**；纯色是**我方选择** |
//! | 档案从哪来 | 根节点 `+0x16a8` 的 `Rc<RefCell<GraphwarPlayerConfig>>`（`INode::init` 建） | 本类直接持一个 [`GraphwarPlayerConfig`] 值（没有 `GraphwarRootNode` 这个节点类） | 原版**观测**；本类**化简** |
//! | `ready` 第 1 步 | `get_node_by_name::<RootNode>("GraphwarRootNode")` + `.expect()` | 真的 `get_node_or_null("GraphwarRootNode")` ⇒ **拿不到**（没有那个节点），记到 `root_lookup_failures` | 原版**观测**；本类**照形状失败** |
//! | `ready` 第 3/5 步 | 两次 `get_node_by_name::<LineEdit>("PlayerNameLineEdit")` + `.expect()`（**为什么两次未知**，`UNKNOWNS`） | 真的查两次；控件由本类在 `ready` 开头**自己建**（场景本来该提供它） | 两次查找**观测**；"自己建"是**我方选择** |
//! | `ready` 第 6 步 | `text_changed` → `on_text_change` | 真连（`TEXT_CHANGED_LINK` 那对名字） | **观测** |
//! | `on_text_change` 的 `save()` | 真写盘（`FileAccess::open` + `store_string` + `close`） | **不写盘**（headless 验收不该有副作用）；只更新内存里的档案 + 计数 | 原版**逐位**；本类**故意省略** |
//! | `process` 闸门 | `is_visible_in_tree()` 为假 ⇒ 直接返回，**连档案都不碰** | 同（闸门在 `Showcase::tick` 里） | **观测** |
//! | 是否读段位 / 队色 | **不读**（有界否定：10 个函数的具名被调者穷举在 `CALLEES`，无颜色表、无段位） | 同（本类**没有**任何段位 / 队色 / 颜色表的东西），见 [`PlayerShowcase::touches_rank`] | **观测**（有界否定） |
//! | 那 3 个 `Sprite2D` + 1 个标签 | **不存在** | 只在 [`PlayerShowcase::build_demo_children`] 里建，**默认不建** | **非原版**（演示） |
//!
//! ⇒ 这个类证明的是「**档案 → 三层贴图 + 计数器闸门**这条链能真的跑起来」，
//! **不是**"名片外观已复原"（贴图是纯色、档案没有磁盘后端、根节点不存在）。
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
//! PlayerShowcase : Control
//!   @export texture_size : int = 8        # 程序生成贴图的边长
//!
//!   player_name() -> String               # 档案 +0x00
//!   skin() / face() / hat() -> String     # 档案 +0x60 / +0x78 / +0x90
//!   change_counter() -> int               # 档案 +0xa8
//!   ready_steps() -> PackedStringArray     # 原版 ready 的 6 步（READY_PLAN，观测）
//!   name_edit_path() -> String             # "PlayerNameLineEdit"（原版字面量）
//!   on_text_change(text: String) -> int    # 对应原版；返回新的 change_counter
//!   set_player_name(text: String) -> int   # 只改档案（不经过控件、不算 on_text_change）
//!   layer_count() -> int                   # 这一帧 draw_plan 会画几层（0..=3）
//!   available_layers() -> PackedInt32Array # 哪几层有贴图（0/1 各一项）
//!   build_demo_children() -> int           # **非原版**：3 个 Sprite2D + 1 个 Label，返回 4
//!   demo_child_count() -> int
//!   touches_rank() -> bool                 # 恒 false（有界否定）
//!   draw_call_count() / drawn_texture_rect_total() / redraw_request_count() -> int
//!   process_call_count() / ready_call_count() / root_lookup_failures() -> int
//! ```
//!
//! # 写这种类必须知道的几条（`docs/16-client.md` §38.4 实测过，这里只记与本类有关的）
//!
//! 1. **`Gd<Node>` 不保活**：`LineEdit` 子节点归**父节点**所有。本类在 `ready` 里建完就
//!    先 `add_child`、再在同一个 `ready` 里**重新查找**（这也是原版第 5 步的形状），
//!    句柄与节点同生共死。
//! 2. **`base_mut()` 是独占借用** ⇒ `_draw` 里画东西那段不能同时写 `self.rect_total`
//!    （`E0499`）；本类在 `base` 的作用域**之后**累加。
//! 3. **`resized` 之类只在 `ready` 里连**（本类不连信号，但 `text_changed` 是 `ready` 里连的）
//!    ⇒ 树外 `set_text` 也会触发（信号连上就行，不要求进树）；这一点验收脚本会区分。
//! 4. **不用写 `set_process(true)`**：实现了 `_process` 的扩展类由 Godot 4.7 自己开。
//! 5. **`draw_*` 只能在 `NOTIFICATION_DRAW` 里调**（引擎侧 `ERR_DRAW_GUARD`）⇒ 改档案之后
//!    只 `queue_redraw()`。

use godot::classes::image::Format;
use godot::classes::{Control, IControl, Image, ImageTexture, LineEdit, Node, Sprite2D, Texture2D};
use godot::prelude::*;

use crate::graphwar_player_config::GraphwarPlayerConfig;
use crate::player_showcase::{self as ps, TextureKind};

/// 程序生成贴图的默认边长（像素）。**我方选择**（原版贴图在 pck 里）。
pub const DEFAULT_TEXTURE_SIZE: i32 = 8;

/// 三层贴图的纯色（RGBA，`0..=1`）。**我方选择**：原版是 `GraphwarTextureLoader` 里的真贴图。
///
/// 顺序与 [`ps::DRAW_ORDER`]（皮肤 → 脸 → 帽子）一致。
const LAYER_COLORS: [(f32, f32, f32, f32); 3] = [
    (0.93, 0.76, 0.62, 1.0), // 皮肤
    (1.00, 1.00, 1.00, 1.0), // 脸
    (0.20, 0.32, 0.62, 1.0), // 帽子
];

/// 把 `(r, g, b, a)` 填成 `edge × edge` 的纯色 `ImageTexture`。
///
/// 与 `_scaffold/entity_drawer_class.rs::solid_texture` 是同一个做法（没有抽公共函数：
/// 那个文件不在本批的可改清单里，抽出去就得动它）。
fn solid_texture(edge: i32, rgba: (f32, f32, f32, f32)) -> Option<Gd<Texture2D>> {
    let edge = edge.max(1);
    let bytes_per_pixel = 4usize;
    let pixel_count = (edge as usize) * (edge as usize);

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

/// `TextureKind` → [`LAYER_COLORS`] 的下标（按 [`ps::DRAW_ORDER`] 的位置，不按判别值）。
fn layer_index(kind: TextureKind) -> usize {
    ps::DRAW_ORDER
        .iter()
        .position(|k| *k == kind)
        .unwrap_or(0)
}

/// 档案里的某个外观名（皮肤 / 脸 / 帽子）。
fn appearance_name(config: &GraphwarPlayerConfig, kind: TextureKind) -> &str {
    match kind {
        TextureKind::Skin => &config.skin,
        TextureKind::Face => &config.face,
        TextureKind::Hat => &config.hat,
    }
}

/// `PlayerShowcase`：玩家名片预览（`Control` 子类）。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct PlayerShowcase {
    base: Base<Control>,

    /// 程序生成贴图的边长（像素）。
    #[export]
    texture_size: i64,

    /// 玩家外观档案（原版在根节点 `+0x16a8` 的 `Rc<RefCell<…>>` 里）。
    ///
    /// 默认值 = [`ps::DEFAULT_SKIN`] / [`ps::DEFAULT_FACE`] / [`ps::DEFAULT_HAT`]
    /// （`GraphwarPlayerConfig::new` 写进去的三条字面量，**观测**），名字留空。
    config: GraphwarPlayerConfig,

    /// `process` 的闸门状态：只有**一个** `u32`（原版 `+0x18`）。
    showcase: ps::Showcase,

    /// 本次实例有没有已经建过 `PlayerNameLineEdit`（`ready` 幂等）。
    edit_created: bool,

    /// `_draw` 被引擎调用过几次。
    draw_calls: i64,
    /// 累计发出过几条 `draw_texture_rect`。
    rect_total: i64,
    /// `_process` 被引擎调用过几次。
    process_calls: i64,
    /// `_ready` 被引擎调用过几次。
    ready_calls: i64,
    /// 计数器变了 ⇒ `queue_redraw()` 的次数。
    redraw_requests: i64,
    /// `ready` 第 1 步（找 `GraphwarRootNode`）失败过几次——本类**一定**失败（没有那个节点）。
    root_lookup_failures: i64,
    /// `build_demo_children` 建了几个（0 = 没建）。
    demo_children: i64,
}

#[godot_api]
impl IControl for PlayerShowcase {
    fn init(base: Base<Control>) -> Self {
        // 档案默认值 = `GraphwarPlayerConfig::new(load())` 的兜底形状：三个默认外观名 + 计数 0。
        let config = GraphwarPlayerConfig {
            name: String::new(),
            skin: ps::DEFAULT_SKIN.to_string(),
            face: ps::DEFAULT_FACE.to_string(),
            hat: ps::DEFAULT_HAT.to_string(),
            unlocked: Vec::new(),
            change_counter: 0,
        };
        Self {
            base,
            texture_size: DEFAULT_TEXTURE_SIZE as i64,
            config,
            showcase: ps::Showcase::new(),
            edit_created: false,
            draw_calls: 0,
            rect_total: 0,
            process_calls: 0,
            ready_calls: 0,
            redraw_requests: 0,
            root_lookup_failures: 0,
            demo_children: 0,
        }
    }

    /// `_ready`：照 [`ps::READY_PLAN`] 的 6 步走（顺序**观测**）。
    ///
    /// 第 1 步真的会失败（本类没有 `GraphwarRootNode`）；第 3/5 步真的查两次
    /// `PlayerNameLineEdit`；第 6 步真的连 `text_changed`。
    fn ready(&mut self) {
        self.ready_calls += 1;

        // 场景本来该提供 `PlayerNameLineEdit`；本类没有场景 ⇒ 自己建一个（我方选择）。
        // 必须在第 3 步**之前**建，否则那一步查不到东西。
        self.ensure_name_edit();

        // ① LookupRoot：`get_node_by_name::<RootNode>("GraphwarRootNode")` + `.expect()`。
        //    本类没有那个节点 ⇒ 真的拿不到。原版在这里 `.expect()` 会 panic，本类记数后继续
        //    （headless 验收不该因为"整棵场景树还没做出来"而挂）。
        let root = self
            .base()
            .get_node_or_null(&NodePath::from(ps::NODE_ROOT));
        if root.is_none() {
            self.root_lookup_failures += 1;
        }

        // ② ReadName：原版从根节点单元 `+0x16a8` 的载荷上共享借一次取名字。
        //    本类直接读自己持有的档案（化简，见文件头对照表）。
        let name = self.config.name.clone();

        // ③ LookupNameEdit + ④ SetEditText：`LineEdit::set_text(名字)`。
        let first = self.lookup_name_edit();
        if let Some(mut edit) = first {
            edit.set_text(&GString::from(name.as_str()));
        }

        // ⑤ LookupNameEditAgain：原版**再取一次**同一个控件（为什么两次未知，`UNKNOWNS`）。
        let second = self.lookup_name_edit();

        // ⑥ ConnectTextChanged：`text_changed` → `on_text_change`（名字见 `TEXT_CHANGED_LINK`）。
        if let Some(mut edit) = second {
            let (signal, handler) = ps::TEXT_CHANGED_LINK;
            let myself = self.to_gd();
            let callable = myself.callable(handler);
            let err = edit.connect(signal, &callable);
            if err != godot::global::Error::OK {
                godot_warn!("PlayerShowcase::ready 连接 {signal} 失败: {err:?}");
            }
        }
    }

    /// `_process`：原版 `PlayerShowcase::process`（`0x4e68f0`）。
    ///
    /// 闸门 + 计数器轮询整个在 [`ps::Showcase::tick`] 里（不可见 ⇒ `NotVisible`，
    /// 计数器没变 ⇒ `Unchanged`，变了 ⇒ `QueueRedraw`）。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;

        let visible = self.base().is_visible_in_tree();
        let counter = self.config.change_counter;
        if self.showcase.tick(visible, counter) == ps::ProcessOutcome::QueueRedraw {
            self.redraw_requests += 1;
            self.base_mut().queue_redraw();
        }
    }

    /// `_draw`：原版 `PlayerShowcase::draw`（`0x4e5400`）——皮肤 → 脸 → 帽子**三层贴图**。
    ///
    /// 三次 `draw_texture_rect_full` 的实参逐个来自 [`ps::draw_plan`]：
    /// `rect = Rect2((0,0), get_size())`、`tile = false`、`modulate = 纯白`、`transpose = false`。
    /// 没有贴图的层**不画**（`if let Some(t)`），本类也不画任何占位矩形（原版就没有）。
    fn draw(&mut self) {
        self.draw_calls += 1;

        let size = self.base().get_size();
        let available = self.available_flags();
        let plan = ps::draw_plan(size.x, size.y, available);

        let mut drawn = 0i64;
        // 贴图必须在 `base_mut()` **之前**全部取好：`BaseMut<'_, Self>` 握着 `&mut self`，
        // 那段里连 `self.texture_for(..)`（`&self`）都借不到（`E0502`，文件头第 2 条）。
        let textures: Vec<Option<Gd<Texture2D>>> =
            plan.iter().map(|call| self.texture_for(call.kind)).collect();
        {
            let mut base = self.base_mut();
            for (call, texture) in plan.iter().zip(textures.iter()) {
                let Some(texture) = texture else {
                    continue;
                };
                let rect = Rect2::new(
                    Vector2::new(call.rect.x, call.rect.y),
                    Vector2::new(call.rect.w, call.rect.h),
                );
                let modulate = Color::from_rgba(
                    call.modulate[0],
                    call.modulate[1],
                    call.modulate[2],
                    call.modulate[3],
                );
                base.draw_texture_rect_ex(texture, rect, call.tile)
                    .modulate(modulate)
                    .transpose(call.transpose)
                    .done();
                drawn += 1;
            }
        }
        self.rect_total += drawn;
    }
}

#[godot_api]
impl PlayerShowcase {
    /// 建 / 复用名字输入框 `PlayerNameLineEdit`（原版是场景提供的，本类自己建）。
    ///
    /// 名字**取原版字面量** [`ps::NODE_NAME_EDIT`]，长度也是原版记的 18。
    fn ensure_name_edit(&mut self) {
        if self.edit_created {
            return;
        }
        let mut edit = LineEdit::new_alloc();
        edit.set_name(&StringName::from(ps::NODE_NAME_EDIT));
        edit.set_text(&GString::from(self.config.name.as_str()));
        let node = edit.clone().upcast::<Node>();
        self.base_mut().add_child(&node);
        self.edit_created = true;
    }

    /// 按名字查一次 `PlayerNameLineEdit`（原版 `get_node_by_name::<LineEdit>`）。
    fn lookup_name_edit(&self) -> Option<Gd<LineEdit>> {
        let node = self
            .base()
            .get_node_or_null(&NodePath::from(ps::NODE_NAME_EDIT))?;
        node.try_cast::<LineEdit>().ok()
    }

    /// 三层里哪几层有贴图（空外观名 ⇒ 没贴图 ⇒ `draw_plan` 不画那一层）。
    fn available_flags(&self) -> [bool; 3] {
        let mut out = [false; 3];
        for (i, kind) in ps::DRAW_ORDER.iter().enumerate() {
            out[i] = !appearance_name(&self.config, *kind).is_empty();
        }
        out
    }

    /// 懒建某一层的程序生成贴图（`None` = 没有贴图，那一层不画）。
    fn texture_for(&self, kind: TextureKind) -> Option<Gd<Texture2D>> {
        let name = appearance_name(&self.config, kind);
        if name.is_empty() {
            return None;
        }
        solid_texture(self.texture_size as i32, LAYER_COLORS[layer_index(kind)])
    }

    /// 玩家名字（档案 `+0x00`）。
    #[func]
    fn player_name(&self) -> GString {
        GString::from(self.config.name.as_str())
    }

    /// 皮肤名（档案 `+0x60`）。
    #[func]
    fn skin(&self) -> GString {
        GString::from(self.config.skin.as_str())
    }

    /// 脸名（档案 `+0x78`）。
    #[func]
    fn face(&self) -> GString {
        GString::from(self.config.face.as_str())
    }

    /// 帽子名（档案 `+0x90`）。
    #[func]
    fn hat(&self) -> GString {
        GString::from(self.config.hat.as_str())
    }

    /// 变更计数器（档案 `+0xa8`）。
    #[func]
    fn change_counter(&self) -> i64 {
        self.config.change_counter as i64
    }

    /// 原版 `ready` 的 6 步（[`ps::READY_PLAN`]），给验收脚本对着看。
    #[func]
    fn ready_steps(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for step in ps::READY_PLAN {
            out.push(&GString::from(ps::ready_step_label(step)));
        }
        out
    }

    /// 原版那个控件的名字（`"PlayerNameLineEdit"`，字面量长 18）。
    #[func]
    fn name_edit_path(&self) -> GString {
        GString::from(ps::NODE_NAME_EDIT)
    }

    /// 等价原版 `on_text_change`：`name = 新文本` → `save()` → **计数器 +1**。
    ///
    /// `save()` 真的会写盘（`FileAccess` + `store_string`），本类**故意不写**
    /// （headless 验收不该产生副作用）；计数器与名字照原版的顺序更新。
    ///
    /// 返回新的 `change_counter()`。
    #[func]
    fn on_text_change(&mut self, text: GString) -> i64 {
        let effect = ps::on_text_change_effect(&text.to_string());
        self.config.name = effect.new_name;
        if effect.saved {
            // 原版在这里 `GraphwarPlayerConfig::save(&载荷.名字)`。本类不写盘 —— 见文件头。
        }
        // 原版是 `[载荷+0xa8] += 1`：`add w8,w8,#1` 是**回绕**加法。
        self.config.change_counter = self
            .config
            .change_counter
            .wrapping_add(effect.counter_delta);
        self.base_mut().queue_redraw();
        self.config.change_counter as i64
    }

    /// 只改档案里的名字（不过控件、不算 `on_text_change` 的计数 +1）。
    ///
    /// 验收脚本用它准备初始状态；游戏里没有这条路径（名字只能从 `LineEdit` 来）。
    #[func]
    fn set_player_name(&mut self, text: GString) -> i64 {
        self.config.name = text.to_string();
        self.base_mut().queue_redraw();
        self.config.change_counter as i64
    }

    /// 这一帧 `draw_plan` 会画几层（`0..=3`）。
    #[func]
    fn layer_count(&self) -> i64 {
        let flags = self.available_flags();
        ps::draw_plan(0.0, 0.0, flags).len() as i64
    }

    /// 哪几层有贴图（按 [`ps::DRAW_ORDER`] 的顺序，`0` / `1` 各一项）。
    #[func]
    fn available_layers(&self) -> PackedInt32Array {
        let mut out = PackedInt32Array::new();
        for flag in self.available_flags() {
            out.push(i32::from(flag));
        }
        out
    }

    /// **非原版**：建 3 个 `Sprite2D`（皮肤 / 脸 / 帽子，各挂程序生成贴图、名字 = 配置里的
    /// 外观名）+ 1 个 `Label`（玩家名字）。
    ///
    /// 原版**没有**这些子节点（见文件头的对照表），它们只是"读配置 → 建节点"这条链路的演示。
    /// 幂等：建过就不再建。返回这次创建了几个（恒 `4`）。
    #[func]
    fn build_demo_children(&mut self) -> i64 {
        if self.demo_children > 0 {
            return self.demo_children;
        }
        let mut made = 0i64;
        for kind in ps::DRAW_ORDER {
            let name = appearance_name(&self.config, kind).to_string();
            let Some(texture) = solid_texture(self.texture_size as i32, LAYER_COLORS[layer_index(kind)])
            else {
                continue;
            };
            let mut sprite = Sprite2D::new_alloc();
            // 名字 = **档案里的外观名**（这就是"读配置"最直白的演示）。
            sprite.set_name(&StringName::from(name.as_str()));
            sprite.set_texture(&texture);
            let node = sprite.clone().upcast::<Node>();
            self.base_mut().add_child(&node);
            made += 1;
        }
        let mut label = godot::classes::Label::new_alloc();
        label.set_text(&GString::from(self.config.name.as_str()));
        let node = label.clone().upcast::<Node>();
        self.base_mut().add_child(&node);
        made += 1;
        self.demo_children = made;
        made
    }

    /// `build_demo_children` 建了几个（0 = 没建）。
    #[func]
    fn demo_child_count(&self) -> i64 {
        self.demo_children
    }

    /// 本类读不读段位 / 队色 —— 恒 `false`。
    ///
    /// 有界否定：原版本模块 10 个函数的具名被调者穷举在 [`ps::CALLEES`]，
    /// 没有 `graphwar_color_manager`、没有段位函数（[`ps::touches_color_manager`] 也是 `false`）。
    #[func]
    fn touches_rank(&self) -> bool {
        ps::touches_color_manager()
    }

    /// `_draw` 被引擎调用过几次。
    #[func]
    fn draw_call_count(&self) -> i64 {
        self.draw_calls
    }

    /// 累计发出过几条 `draw_texture_rect`。
    #[func]
    fn drawn_texture_rect_total(&self) -> i64 {
        self.rect_total
    }

    /// 计数器变了 ⇒ `queue_redraw()` 的次数。
    #[func]
    fn redraw_request_count(&self) -> i64 {
        self.redraw_requests
    }

    /// `_process` 被引擎调用过几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// `_ready` 被引擎调用过几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `ready` 第 1 步（找 `GraphwarRootNode`）失败过几次。
    #[func]
    fn root_lookup_failures(&self) -> i64 {
        self.root_lookup_failures
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 绘制顺序是**皮肤 → 脸 → 帽子**（后画的盖住先画的），三层颜色互不相同。
    #[test]
    fn draw_order_is_skin_face_hat_and_colors_are_distinct() {
        assert_eq!(
            ps::DRAW_ORDER,
            [TextureKind::Skin, TextureKind::Face, TextureKind::Hat]
        );
        assert_eq!(layer_index(TextureKind::Skin), 0);
        assert_eq!(layer_index(TextureKind::Face), 1);
        assert_eq!(layer_index(TextureKind::Hat), 2);
        assert_ne!(LAYER_COLORS[0], LAYER_COLORS[1]);
        assert_ne!(LAYER_COLORS[1], LAYER_COLORS[2]);
        assert_ne!(LAYER_COLORS[0], LAYER_COLORS[2]);
    }

    /// 三次绘制铺满自身矩形、`tile` / `transpose` 恒 false、`modulate` 恒纯白（`0x19ed00`）。
    #[test]
    fn draw_plan_is_three_white_rects_filling_the_control() {
        let plan = ps::draw_plan(120.0, 160.0, [true, true, true]);
        assert_eq!(plan.len(), 3);
        for (i, call) in plan.iter().enumerate() {
            assert_eq!(call.kind, ps::DRAW_ORDER[i]);
            assert_eq!((call.rect.x, call.rect.y), (0.0, 0.0), "原点恒 (0,0)");
            assert_eq!((call.rect.w, call.rect.h), (120.0, 160.0));
            assert!(!call.tile);
            assert!(!call.transpose);
            assert_eq!(call.modulate, ps::WHITE);
        }
        // 缺一层就不画那一层（`if let Some(t)`）。
        let only_hat = ps::draw_plan(1.0, 1.0, [false, false, true]);
        assert_eq!(only_hat.len(), 1);
        assert_eq!(only_hat[0].kind, TextureKind::Hat);
        assert!(ps::draw_plan(1.0, 1.0, [false, false, false]).is_empty());
    }

    /// `process` 的三道闸门：不可见 ⇒ 不碰档案；没变 ⇒ 不动；变了 ⇒ 要重画。
    #[test]
    fn process_outcome_follows_the_visibility_gate_and_counter() {
        let mut s = ps::Showcase::new();
        assert_eq!(s.tick(false, 5), ps::ProcessOutcome::NotVisible);
        assert_eq!(s.last_counter, 0, "不可见时计数器都不该被记下来");
        assert_eq!(s.tick(true, 0), ps::ProcessOutcome::Unchanged);
        assert_eq!(s.tick(true, 5), ps::ProcessOutcome::QueueRedraw);
        assert_eq!(s.last_counter, 5, "重画之后才记计数器");
        assert_eq!(s.tick(true, 5), ps::ProcessOutcome::Unchanged);
    }

    /// `on_text_change` 的语义：名字改了、存档**真的**发生、计数器 +1（顺序：先存档再加计数）。
    #[test]
    fn text_change_updates_name_saves_and_increments() {
        let e = ps::on_text_change_effect("Alice");
        assert_eq!(e.new_name, "Alice");
        assert!(e.saved);
        assert_eq!(e.counter_delta, 1);
        // 本类用回绕加法（原版 `add w8,w8,#1`）。
        assert_eq!(u32::MAX.wrapping_add(1), 0);
    }

    /// 档案三个默认外观名就是 `GraphwarPlayerConfig::new` 写进去的那三条字面量。
    #[test]
    fn default_appearance_names_come_from_the_pure_module() {
        assert_eq!(ps::DEFAULT_SKIN, "yellow");
        assert_eq!(ps::DEFAULT_FACE, "regular_eyes");
        assert_eq!(ps::DEFAULT_HAT, "soldier_helmet");
        assert_eq!(ps::DEFAULT_LENS, (6, 12, 14));
        let mut cfg = GraphwarPlayerConfig::default();
        cfg.skin = ps::DEFAULT_SKIN.to_string();
        cfg.face = ps::DEFAULT_FACE.to_string();
        cfg.hat = ps::DEFAULT_HAT.to_string();
        assert_eq!(appearance_name(&cfg, TextureKind::Skin), "yellow");
        assert_eq!(appearance_name(&cfg, TextureKind::Face), "regular_eyes");
        assert_eq!(appearance_name(&cfg, TextureKind::Hat), "soldier_helmet");
    }

    /// 有界否定：本模块**不读**段位 / 队色。本类的 `touches_rank()` 就是这一条的直接映射。
    #[test]
    fn the_module_never_touches_rank_or_team_colors() {
        assert!(!ps::touches_color_manager());
        assert!(!ps::CALLEES.iter().any(|c| c.contains("color_manager")));
        assert!(!ps::CALLEES.iter().any(|c| c.contains("rank")));
    }

    /// `ready` 的 6 步与两个节点名都是原版字面量（长度也照原版记的）。
    #[test]
    fn ready_plan_and_node_names_are_the_original_literals() {
        assert_eq!(ps::READY_PLAN.len(), 6);
        assert_eq!(ps::NODE_ROOT, "GraphwarRootNode");
        assert_eq!(ps::NODE_NAME_EDIT, "PlayerNameLineEdit");
        assert_eq!(ps::NODE_ROOT_LEN as usize, ps::NODE_ROOT.len());
        assert_eq!(ps::NODE_NAME_EDIT_LEN as usize, ps::NODE_NAME_EDIT.len());
        assert_eq!(ps::TEXT_CHANGED_LINK, ("text_changed", "on_text_change"));
        assert_eq!(ps::METHOD_ON_TEXT_CHANGE_LEN as usize, "on_text_change".len());
        assert_eq!(ps::SIGNAL_TEXT_CHANGED_LEN as usize, "text_changed".len());
    }
}
