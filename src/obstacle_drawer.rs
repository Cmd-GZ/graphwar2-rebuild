//! `obstacle_drawer.rs` —— **障碍物的光栅化 + 贴图上屏**（原版 `ObstacleDrawer`）。
//!
//! # 它是什么（一句话）
//!
//! 它**不用任何绘图图元**：把障碍物**画进一张 CPU 侧的 `Image`**（`PackedByteArray` 逐字节填），
//! 再 `ImageTexture::create_from_image` 上传，挂到**自己建的 `Sprite2D`** 上，
//! 最后按平面 `Control::get_size()` 设 `position` / `scale`。
//! ⇒ 障碍物是**节点 + 纹理**，和 `entity_drawer`（士兵）、`function_drawer`（曲线）同一个设计取向：
//! 图元只有 7 个函数会用（`docs/16-client.md` §26.4 的有界穷举），本模块**不在**那 7 个里。
//!
//! # 函数表（arm64 2.0.60 未 strip）
//!
//! | 函数 | arm64 2.0.60 | 字节 | x86 vaddr | 字节 | 判据 |
//! |---|---|---|---|---|---|
//! | `update` | `0x422590` | 4688 | **`0x3ee190`** | 5216 | 6 个 `panic::Location` 键 + 被调者（`Image::create_from_data`×2、`ImageTexture`/`Sprite2D`、`HashMap::rustc_entry`×3）；比值 **1.11** |
//!
//! 本模块在 arm64 侧**只有 `update` 一个**方法符号（另一条是 `drop_in_place<ObstacleDrawer>`，
//! 在 3 个地址各 268 B）。**但** `src/obstacle_drawer.rs` 还有**第 7 个** Location 落在
//! **`GraphwarPlane::init`** 里：
//!
//! | 归属 | x86 vaddr | 字节 | Location |
//! |---|---|---|---|
//! | `GraphwarPlane::init` | **`0x4b7380`** | 1776 | `src/obstacle_drawer.rs:37:97`（槽 `0x954ea0`） |
//!
//! 也就是说 **`ObstacleDrawer` 的构造发生在 `GraphwarPlane::init` 里**（那一行是 `init` 体内的
//! `ObstacleDrawer::new(..)` 调用/字段初始化）。⇒ 平面的 `init` 被 `docs/02` §21 同时记在
//! `graphwar_plane`（3 个 FunctionDrawer）与本节（`obstacle_drawer.rs:37:97`）名下，
//! 两处都对：**同一个函数体里有来自两个源文件的行**。
//!
//! # `update` 干了什么（**观测**，arm64 被调者穷举 + x86 引用字符串穷举）
//!
//! 顺序（按地址）：
//!
//! ```text
//! Control::get_size        ×2   0x422608 / 0x42262c   ; 平面的像素尺寸
//! Node2D::set_position      ×1   0x422658             ; Sprite2D 摆在原点
//! Node2D::set_scale         ×1   0x42267c             ; 缩放 = 1/尺寸 之类
//! GraphwarModuleContainer::get_module ×2  0x4226a0 / 0x422ad0   ; 取管理器
//! Image::create_from_data   ×2   0x422780 / 0x422d1c
//! PackedArray<u8>::op_push_back ×2 + ffi_index_mut      ; 往字节缓冲里填像素
//! ImageTexture::create_from_image                       ; 0x1bfc0f 字面量，x86 4 个调用点
//! ImageTexture::update                                  ; 0x1bfc20 字面量，x86 4 个调用点
//! ImageTexture::__validated_obj → Sprite2D::set_texture ; 0x1bfc8c 字面量
//! Sprite2D::__validated_obj + ExAddChild::new + add_child_full   ; 首次建节点
//! hashbrown::rustc_entry    ×3   0x422828 / 0x422df4 / 0x42306c   ; 按 key 缓存那些节点
//! MutexGuard<GraphwarObstacleData>                      ; 障碍数据来自 GraphwarObstacleManager 的 Mutex
//! ```
//!
//! 涉及的类型/方法字面量（**vaddr**，本版 `.rodata` 的 `vaddr == 文件偏移`）：
//!
//! | 字面量 | vaddr | 长度 |
//! |---|---|---|
//! | `Sprite2D` | `0x19fb58` | 8 |
//! | `ImageTexture` | `0x1bfc03` | 12 |
//! | `create_from_image` | `0x1bfc0f` | 17 |
//! | `update` | `0x1bfc20` | 6 |
//! | `set_texture` | `0x1bfc8c` | 11 |
//! | `upcast_ref` | `0x1bfc97` | 10 |
//! | `clone` | `0x1bfe73` | 5 |
//!
//! # 脏矩形怎么配合（任务问的第 4 点）
//!
//! **分工（观测 + 推断，分开写）**：
//!
//! * **脏矩形在 `graphwar_plane` 手里**，不在本模块：`PixelRect` / `obstacle_rect` /
//!   `obstacle_partial_update_event` 三个东西的落码与证据见 `docs/16-client.md` §26.7。
//!   本模块 6 个 Location（`:51 :62 :92 :113 :120 :128`）里**没有一个**在做"矩形运算"
//!   —— 它们全是 `unwrap`/断言类失败路径。
//! * **谁决定重画哪块**：`GraphwarPlane::obstacle_partial_update_event`（x86 `0x4be3e0`）
//!   收到障碍事件后把矩形塞进平面自己的字段（**观测**，见 §26.3/§26.7）；
//!   `ObstacleDrawer::update` 只负责"按当时的数据把图和纹理刷对"，它自己的入参里
//!   没有任何矩形（**观测**：它只 `get_size()` 拿整张平面的尺寸）。
//! * **每帧调几次**：`GraphwarPlane::process`（x86 `0x4b83f0`）每帧调 `update` **2 次**
//!   （**观测**，§26.3）。为什么是 2 次 —— **未知**（候选：一次整图 + 一次脏区；
//!   候选：两层障碍各一次）。**缺的证据**：对 `update` 的两次入参做运行时抓取。
//!
//! ⇒ 一句话回答：**脏矩形由 `graphwar_plane` 决定并持有，`obstacle_drawer` 只是消费者**；
//! 而"只重画那一块"这个更强的说法，本轮**没有**逐条读到 —— 标 **未知**。
//!
//! # 地址约定
//!
//! `x86` 是 **ELF 虚地址**（Ghidra/REA = 本值 + `0x100000`）；`arm64` 只用于定名。

/// 一个源级函数在两侧的落点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Func {
    /// arm64 2.0.60 的源级真名。
    pub name: &'static str,
    /// arm64 2.0.60 地址。
    pub arm64: u32,
    /// arm64 字节数。
    pub arm64_len: u32,
    /// x86-64 目标的 ELF 虚地址。
    pub x86: u32,
    /// x86 字节数。
    pub x86_len: u32,
    /// 判据。
    pub how: &'static str,
}

/// 本模块在两侧的 2 条落点：`update`（本模块的方法）与 `GraphwarPlane::init`
/// （**另一个模块的方法体里带着本文件的 Location**）。
pub const FUNCTIONS: [Func; 2] = [
    Func {
        name: "update",
        arm64: 0x0042_2590,
        arm64_len: 4688,
        x86: 0x003e_e190,
        x86_len: 5216,
        how: "6 个 panic::Location 键 + 被调者 Image::create_from_data×2 / ImageTexture / Sprite2D / rustc_entry×3；比值 1.11",
    },
    Func {
        name: "GraphwarPlane::init（体内带 obstacle_drawer.rs:37:97）",
        arm64: 0x004b_d73c,
        arm64_len: 1336,
        x86: 0x004b_7380,
        x86_len: 1776,
        how: "Location :37:97 双向唯一（docs/16-client.md §26.2 的 init 行）",
    },
];

/// `src/obstacle_drawer.rs` 的全部 `panic::Location`：**7 个**。
///
/// 字段：`(line, col, Location 结构体地址, 引用它的函数)`。
/// 前 6 条在 `update`（`0x3ee190`）里，最后 1 条在 `GraphwarPlane::init`（`0x4b7380`）里。
pub const LOCATIONS: [(u32, u32, u32, u32); 7] = [
    (37, 97, 0x0095_4ea0, 0x004b_7380),
    (51, 49, 0x0095_4e10, 0x003e_e190),
    (62, 133, 0x0095_4e28, 0x003e_e190),
    (92, 49, 0x0095_4e40, 0x003e_e190),
    (113, 54, 0x0095_4e58, 0x003e_e190),
    (120, 66, 0x0095_4e88, 0x003e_e190),
    (128, 136, 0x0095_4e70, 0x003e_e190),
];

/// 上屏流水线的每一步：`(字面量, vaddr, 长度, 在哪被用)`。
///
/// 全部是 **vaddr**；`.rodata` 在本版 `.so` 里 `vaddr == 文件偏移`，可直接按偏移读。
pub const LITERALS: [(&str, u32, usize, &str); 7] = [
    ("Sprite2D", 0x0019_fb58, 8, "x86 0x4eed3f/0x4eedf9/0x4eeed9 的 cast 目标"),
    ("ImageTexture", 0x001b_fc03, 12, "x86 0x4ee44f/0x4ee91a/0x4eeacc/0x4eec1b 的 cast 目标"),
    ("create_from_image", 0x001b_fc0f, 17, "x86 0x4ee92f/0x4ee9dc/0x4eec22/0x4eecb3 的方法名"),
    ("update", 0x001b_fc20, 6, "x86 0x4ee464/0x4ee519/0x4eead3 的方法名"),
    ("set_texture", 0x001b_fc8c, 11, "x86 0x4eed54/0x4eee0b/0x4eeee0 的方法名"),
    ("upcast_ref", 0x001b_fc97, 10, "x86 0x4ee1df/0x4ee1f6/0x4ee3ec/0x4eece6/0x4eef0c"),
    ("clone", 0x001b_fe73, 5, "x86 0x4ee5cd/0x4ee5e7"),
];

/// 一帧里 `ObstacleDrawer::update` 被 `GraphwarPlane::process` 调 **2 次**（§26.3，观测）。
pub const UPDATE_CALLS_PER_FRAME: usize = 2;

/// 障碍数据来自 `GraphwarObstacleManager` 的 `Mutex<GraphwarObstacleData>`
/// （arm64 的 `MutexGuard<GraphwarObstacleData>` / `PoisonError<MutexGuard<…>>` drop glue 在
/// `update` 体内，`0x42368c` / `0x4236d4`）。
pub const OBSTACLE_DATA_GUARDED_BY_MUTEX: bool = true;

/// 本模块用的 Godot 侧被调者（arm64 穷举，去重后按首次出现排序）。
pub const CALLED_API: [&str; 12] = [
    "Control::get_size",
    "Node2D::set_position",
    "Node2D::set_scale",
    "GraphwarModuleContainer::get_module",
    "Image::create_from_data",
    "PackedArray<u8>::op_push_back",
    "ImageTexture::create_from_image",
    "ImageTexture::update",
    "Sprite2D::set_texture",
    "Node::add_child_full",
    "hashbrown::rustc_entry",
    "RefCounted::unreference",
];

/// 按名字查一条 [`FUNCTIONS`]。
pub fn func(name: &str) -> Option<&'static Func> {
    FUNCTIONS.iter().find(|f| f.name == name)
}

/// 按字面量查一条 [`LITERALS`]。
pub fn literal(text: &str) -> Option<(&'static str, u32, usize, &'static str)> {
    LITERALS.iter().copied().find(|(t, _, _, _)| *t == text)
}

/// x86 与 arm64 的字节数比值（同源闸 `0.61–1.19`）。
pub fn size_ratio(f: &Func) -> f64 {
    f.x86_len as f64 / f.arm64_len as f64
}

/// 同一帧里 `update` 会刷几张图 —— 等于调用次数乘以每次的 `create_from_data` 数。
///
/// `create_from_data` 在 `update` 体内出现 **2 次**（arm64 `0x422780` / `0x422d1c`）。
pub fn images_per_frame(create_from_data_per_update: usize) -> usize {
    UPDATE_CALLS_PER_FRAME * create_from_data_per_update
}
