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
// T57：`function_drawer_class` 的 gdext 节点类已搬回 `src/function_drawer.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `function_drawer.rs` 里（`panic::Location` 为证）。
// T57：`entity_drawer_class` 的 gdext 节点类已搬回 `src/entity_drawer.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `entity_drawer.rs` 里（`panic::Location` 为证）。
// 三个「基类不是 Control」的类：`GraphwarRootNode`（`Node`）、`GraphwarLanVBox`
// （`VBoxContainer`）、`GraphwarAdSwitch`（基类未定，按 `Node` 落）。用同一套模板，
// 只有虚方法块不同（`docs/16-client.md` 本轮新增的那一节）。只在 `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod graphwar_ad_switch_class;
// T57：`graphwar_lan_vbox_class` 的 gdext 节点类已搬回 `src/graphwar_lan_vbox.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `graphwar_lan_vbox.rs` 里（`panic::Location` 为证）。
// T57：`graphwar_root_node_class` 的 gdext 节点类已搬回 `src/graphwar_root_node.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `graphwar_root_node.rs` 里（`panic::Location` 为证）。

// T57：`graphwar_button_class` 的 gdext 节点类已搬回 `src/graphwar_button.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `graphwar_button.rs` 里（`panic::Location` 为证）。
// T57：`graphwar_function_input_class` 的 gdext 节点类已搬回 `src/graphwar_function_input.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `graphwar_function_input.rs` 里（`panic::Location` 为证）。
#[cfg(feature = "gdext-bindings")]
pub mod graphwar_setup_control_class;

// T57：`effect_drawer_class` 的 gdext 节点类已搬回 `src/effect_drawer.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `effect_drawer.rs` 里（`panic::Location` 为证）。
// 结算弹窗（`EndGamePopUp`）的**数据桥**（`docs/06` §77）：把胜方字节
// （`turnstate::winner_byte`，`0x478133`–`0x478236`）与 `.rodata` 里那 6 条胜负横幅
// （`graphwar_play_screen_control::VICTORY_TEXTS`，`0x3da130`）接给 GDScript。
// 本文件**没有第二份判据**，只搬运算好的东西。只在 `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod endgame_class;
// 手写入口与 gdext 入口都导出 `gdext_rust_init`，一个 .so 里只能有一个：
// 打开 `gdext-bindings` 时用官方 gdext 那份（`gdext_mod.rs`），关掉手写这份。
#[cfg(not(feature = "gdext-bindings"))]
pub mod gdext;
pub mod gdext_class;
// 官方 gdext 版的绑定层骨架（`docs/18-build.md` §10）。默认不编译，不引入任何依赖。
#[cfg(feature = "gdext-bindings")]
pub mod gdext_mod;
// T57：`graphwar_plane_class` 的 gdext 节点类已搬回 `src/graphwar_plane.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `graphwar_plane.rs` 里（`panic::Location` 为证）。
// T57：`long_press_animation_class` 的 gdext 节点类已搬回 `src/graphwar_long_press_animation.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `graphwar_long_press_animation.rs` 里（`panic::Location` 为证）。
// T57：`obstacle_drawer_class` 的 gdext 节点类已搬回 `src/obstacle_drawer.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `obstacle_drawer.rs` 里（`panic::Location` 为证）。
// 客户端侧「线上事件 → 复原 manager」的**派发桥**（`docs/06` §67 那一轮）：
// 原版 `GraphwarRemoteEventHandler::handle_server_remote_event`（x86 `0x3e6df0`）那个模块
// 我方**没有复元**，客户端收到事件后 22 个 manager 一个字节都拿不到。本模块按"线上事件 →
// manager 方法"的证据表把那条断链接上（`GwNetEvents`），只在 `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod net_events_class;
// 两个**原版 Rust 模块里本来就有**的时间常量的取值口（T42）：连接超时
// `graphwar_client_connection_handler::CONNECT_TIMEOUT_SECS = 30`（arm64 `0x48dccc mov w1,#0x1e`）
// 与 LAN 房间 TTL `graphwar_lan_broadcaster_v6::ENTRY_TTL_SECS = 10`（arm64 `0x3de6e4 cmp x0,#0xa`）。
// `client/src/net/*.gd` 以前自己编了 3 s / 12 s，现在一律经这里问 Rust 要。
// 只在 `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod net_constants_class;
// 函数输入 / 键盘的**规则出口**（T42 ③）：把 `client/src/play/function_input.gd` 的编辑规则
// （8 个 `match` 分支）与 `keyboard_input.gd` 的"按键 → 字符"规则收进 Rust ——
// 落点是 `graphwar_function_input::Buffer`（`0x4f3210`/`0x4f3910`/`0x4f44c0`）与
// `classify_key`（`gui_input` 折叠跳转表 `0x1c3d5c`，198 项里 28 项）。`.gd` 只留视图。
// 只在 `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod function_input_rules_class;
// T57：`player_showcase_class` 的 gdext 节点类已搬回 `src/player_showcase.rs` —— 原版把 `#[derive(GodotClass)]`
// struct 与方法体写在同一个 `player_showcase.rs` 里（`panic::Location` 为证）。
// 播放屏的**流程权威**（`dispatch/54`，`docs/16-client.md` §52）：把原来写在
// `client/src/game/level_rules.gd`（219 行）与 `turn_driver.gd`（121 行）里的流程搬进 Rust。
// `GwLevelRules` = 胜负判定（`0x473520`/`0x4759f0`/`0x478133`–`0x478236`）；
// `GwTurnDriver` = 射手轮转（`0x463da0`）+ 同时回合收集（`RoundCollection`）。只在
// `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod play_flow_class;
pub mod protocol;
// 模块容器装载器 + 3 个 manager 的真装配（`docs/16-client.md` §38）：按 `TypeId` 把
// `GameStateManager` / `TurnManager` / `PlayerManager` 装进 `GraphwarModuleContainer`。
#[cfg(feature = "gdext-bindings")]
pub mod root_wiring;
// 回放的**驱动**（`dispatch/81`，`docs/16-client.md` §56）：把"回放怎么往前走"
// 从 GDScript（每帧 `tick += dt × 60`）搬成 Rust 的定步累加器 + **每 tick 泵一次**
// —— 依据是 `docs/06` §44 量到的 `pumps ≡ ticks`（原版泵只在 tick 那一圈里跑）。
// 纯逻辑（无 gdext），所以 `cargo test` 直接测；外壳类在 `replay_driver_class.rs`。
pub mod replay_driver;
// `GwReplayDriver` 的 GDExtension 外壳（播放屏的回放模式用它驱动）。只在
// `gdext-bindings` 下编译。
#[cfg(feature = "gdext-bindings")]
pub mod replay_driver_class;
pub mod replay_inflate;
// 回放**列表行**的元数据（关卡名 / 四个模式 / 文件名时间戳），纯逻辑，
// `cargo test` 直接测（`docs/06` §56）。GDExtension 外壳在 `replay_meta_class.rs`。
pub mod replay_meta;
#[cfg(feature = "gdext-bindings")]
pub mod replay_meta_class;
pub mod replaydiff;
pub mod rng;
