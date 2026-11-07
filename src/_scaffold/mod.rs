//! **不是原版的东西**：我们的桥接层 / 工具层 / 装配层。
//!
//! 原版 `libgraphwarrust.so` 里的 `src/` 只有 **58 个 `graphwar_*.rs` + 12 个
//! 非 `graphwar_` 前缀的游戏模块**（名单见 `docs/coverage-matrix.tsv`）。这个目录里的
//! 文件**在原版里没有对应物**，它们是：
//!
//! * 我们要把复原出来的算法接进 Godot / C / 客户端所必需的**外壳**
//!   （`gdext.rs` / `gdext_class.rs` / `capi.rs` / `api.rs`）；
//! * 我们为了**验证**而写的工具（`replaydiff.rs` / `diffconv.rs` / `ai_explain.rs`）；
//! * 原版用**第三方 crate** 做、不是游戏模块的替代实现（`rng.rs` = `rand_chacha`，
//!   `replay_inflate.rs` = `flate2`）；
//! * 我们自己立起来的**服务端**（`broker.rs`——原版只有连 broker 的客户端）。
//!
//! 规则（`AGENTS.md` §0 与铁律 3）：**自造的东西不许挂原版名**。所以它们被收进
//! `src/_scaffold/`，一眼能看出"这不是从二进制解出来的"。
//!
//! 反过来，**这个目录之外**的每个文件都必须能指到原版模块：
//!
//! * `graphwar_*.rs`：58 个原版模块之一（一对一见 `docs/02-module-map.md` §17.1）；
//! * `math_function.rs` / `entity_drawer.rs`：12 个非 `graphwar_` 游戏模块之一（§17.2）；
//! * `events.rs` / `game.rs` / `plane.rs` / `server.rs` / `turnstate.rs`：**混装**了
//!   多个原版模块、暂未拆（登记在 §17.4）；
//! * `lib.rs`：crate 根。
//!
//! 每个文件"做什么、凭什么"的完整清单见 `README.md`（同目录）。

pub mod ai_explain;
pub mod api;
pub mod axis_drawer_class;
pub mod broker;
pub mod capi;
pub mod diffconv;
// `FunctionDrawer` 的真节点类（`docs/16-client.md` §42）：每个 player id 一条 `Line2D`，
// `update_function` 走纯逻辑的增量追加窗口（`function_drawer::append_window`）。
// 只在 `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod function_drawer_class;
// `EntityDrawer` 的真节点类（`docs/16-client.md` §37）：把 `entity_drawer.rs` 的纯逻辑挂成
// 3 个 `Sprite2D` + 1 个 `RichTextLabel` 的子节点树。只在 `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod entity_drawer_class;
// 三个「基类不是 Control」的类：`GraphwarRootNode`（`Node`）、`GraphwarLanVBox`
// （`VBoxContainer`）、`GraphwarAdSwitch`（基类未定，按 `Node` 落）。用同一套模板，
// 只有虚方法块不同（`docs/16-client.md` 本轮新增的那一节）。只在 `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod graphwar_ad_switch_class;
#[cfg(feature = "gdext-bindings")]
pub mod graphwar_lan_vbox_class;
#[cfg(feature = "gdext-bindings")]
pub mod graphwar_root_node_class;

// 三个"要信号接线"的类（`docs/16-client.md` §39）：`Button` / `LineEdit` / `Control` 子类，
// 验自造 `#[signal]`、连引擎原生信号、外部注册闭包表这三条路。同样只在 `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod graphwar_button_class;
#[cfg(feature = "gdext-bindings")]
pub mod graphwar_function_input_class;
#[cfg(feature = "gdext-bindings")]
pub mod graphwar_setup_control_class;

// `EffectDrawer` 的真节点类（`docs/16-client.md` §39）：把 `effect_drawer.rs` 的图元清单落到
// 真的 `draw_circle` / `draw_polygon` / `draw_line` 上。只在 `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod effect_drawer_class;
// 手写入口与 gdext 入口都导出 `gdext_rust_init`，一个 .so 里只能有一个：
// 打开 `gdext-bindings` 时用官方 gdext 那份（`gdext_mod.rs`），关掉手写这份。
#[cfg(not(feature = "gdext-bindings"))]
pub mod gdext;
pub mod gdext_class;
// 官方 gdext 版的绑定层骨架（`docs/18-build.md` §10）。默认不编译，不引入任何依赖。
#[cfg(feature = "gdext-bindings")]
pub mod gdext_mod;
// `GraphwarPlane` 的真节点类（§39）：3 个占位子节点 + 脏矩形队列 + 棋盘框。
#[cfg(feature = "gdext-bindings")]
pub mod graphwar_plane_class;
// `GraphwarLongPressAnimation` 的真节点类（§39）：两次 `draw_arc` + `_process` 推进相位。
#[cfg(feature = "gdext-bindings")]
pub mod long_press_animation_class;
// `PlayerShowcase` 的真节点类（§39）：读档案 + 三层 `draw_texture_rect` + 计数器闸门。
#[cfg(feature = "gdext-bindings")]
pub mod player_showcase_class;
pub mod protocol;
pub mod replay_inflate;
pub mod replaydiff;
pub mod rng;
