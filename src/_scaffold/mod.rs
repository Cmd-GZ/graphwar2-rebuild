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
pub mod broker;
pub mod capi;
pub mod diffconv;
pub mod gdext;
pub mod gdext_class;
pub mod protocol;
pub mod replay_inflate;
pub mod replaydiff;
pub mod rng;
