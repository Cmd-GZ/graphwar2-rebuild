//! `graphwar_remote_event_handler.rs`：客户端侧 / 服务端侧的**远端事件处理器**
//! （原版 `GraphwarRemoteEventHandler`）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **ELF vaddr**（= 模块内偏移 = RVA；Ghidra/REA 的反编译地址 = 本地址 + `0x100000`）。
//!
//! # 1. 为什么这个文件存在（边界证据）
//!
//! 本模块此前**没有同名文件**，内容混在 `src/events.rs`。T24 的逐函数边界审计
//! （[`docs/06`](../../docs/06-verification.md) §74、`fixtures/boundary_audit.tsv`，
//! 判据 = `panic::Location` 硬归属）判出两条 `mismatch`：
//!
//! | 地址 | 符号名（三向真名，`fixtures/name_to_addr_v2.tsv`） | `panic::Location` 主文件 | 首锚 `(line:col)` | 锚点行数 | 判定 |
//! |---|---|---|---|---|---|
//! | `0x3dee00` | `GraphwarRemoteEventHandler::handle_client_remote_event` | `src/graphwar_remote_event_handler.rs` | `143:99` | **80** | `mismatch`（原 `ours=src/events.rs`） |
//! | `0x3e5910` | `GraphwarRemoteEventHandler::is_leader` | 同上 | `135:103` | 2（`:135` `:136`） | 同上 |
//!
//! 两条都单模块、一边倒 ⇒ `AGENTS.md` §4「**文件名就是证据**」：原版有这个模块，
//! 我们就要有这个同名 `.rs`。本文件即按该证据建立（T30 / `task-27`，2026-11-26）。
//!
//! ⚠️ **本文件的成立只说明"布局/文件名对上"，不代表这两个函数的行为复原了。**
//! 这与 `docs/06` §74.3 对台账 `match` 的口径一致（`match` 数的是"能被 Location 钉住的
//! 函数里原版文件与我方布局同名"，不是"复原了多少行为"）。
//!
//! # 2. 我方现状（诚实清单）
//!
//! | 原版函数 | 我方状态 | 说明 |
//! |---|---|---|
//! | `0x3dee00` `handle_client_remote_event` | **未落码（0 个函数体）** | 原版这一条就有 80 个 Location 锚、反编译约 2449 行：它是"客户端/服务端收到一条事件 → 落到对应 manager"的大分派体，体内还内联了大批别的模块的 setter |
//! | `0x3e5910` `is_leader` | **未落码（0 个函数体）** | 见 §3 的指令级观测；语义仍缺 arm64 对照与调用点分析 |
//! | `0x3e2830` `handle_server_remote_event`（第三个别名函数） | 不搬 | 台账判 **`multi`**（主文件 88 锚 + 另 8 个模块各 1–5 锚，`docs/06` §74.5）⇒ `AGENTS.md` §4「边界无证据不猜不搬」，**只登记、不碰** |
//!
//! ⚠️ **T49 更正**：这段派发层的**语义**已经搬回本文件（§5）——`WireEvent` / `dispatch` /
//! `parse_*` / `is_bot_skin` 现在住在这里，`_scaffold/net_events_class.rs` 只剩 GDExtension 外壳。
//!
//! 我方现在能指到"远端事件"这条链路的只有**自造桥接**，按 §4 不进复原层：
//! `src/_scaffold/net_events_class.rs`（`GwNetEvents`：线上事件 → 复原 manager 的派发桥，
//! 文件头自述"**不是复原物**"，因为本模块没复原所以客户端这条链路是断的）。
//! 枚举/判别值侧的证据在 `src/events.rs`（`enum Event`，74 变体）与
//! `src/graphwar_event_handler.rs`（`DISPATCH_NAME_TAGS`，72 条名字→判别值）。
//!
//! # 3. `is_leader` 的指令级观测（`tools/addr_probe.py dis 0x3e5910`，**观测**）
//!
//! ```text
//! 0x3e5910  push rbp / r14 / rbx ; sub rsp,0x10
//! 0x3e5918  mov  ebx, esi                     ; 第二参数（u32）留着后面用
//! 0x3e591a  call 0x46d8c0                     ; module_container::get_module_wk 的单态化族
//! 0x3e591f  test rax,rax ; je -> panic        ; Rc 为空 ⇒ unwrap 失败路径
//! 0x3e592d  mov  rcx,[rax+0x10]               ; Rc 强计数
//! 0x3e5931  movabs rdx,0x7fffffffffffffff ; cmp rcx,rdx ; jae -> panic   ; Rc 溢出检查
//! 0x3e5944  lea  rdx,[rcx+1] ; mov [rax+0x10],rdx                          ; 计数 +1
//! 0x3e594c  mov  r14,[rax+0x48] ; test r14,r14 ; je -> 尾部                ; 表长度/桶数
//! 0x3e5959  mov  rdx,[rax+0x30]                                            ; hashbrown 控制字节块
//! 0x3e595d  movdqa xmm0,[rdx] ; pmovmskb r9d,xmm0 ; xor r9d,0xffff
//! 0x3e5980  ... add rdx,-0x80 / add rsi,0x10 / pmovmskb ...（0x80 字节一组扫）
//! 0x3e599a  tzcnt edi,r9d ; shl edi,3 ; sub r8,rdi                          ; 命中槽地址
//! 0x3e59a8  mov  ebp,[r8-4]                                                ; 槽里的键（u32）
//! ```
//!
//! **观测**：形状是 `is_leader(u32) -> bool`（取第二个参数）；先 `get_module_wk` 拿一个
//! `Rc` 对象（强计数 +0x10，`Rc` 溢出检查在 `0x7fff…`），再对 `+0x30`/`+0x48` 那对字段做
//! **hashbrown** 查表（`pmovmskb`/`tzcnt` 是标准库 `HashMap` 的控制字节扫描），键是 4 字节。
//! **推断**：传入的 u32 是 connection id / player id，答案是"这个连接是不是房主"。
//! **缺的证据**：arm64 2.0.60 的同函数对照、`0x46d8c0` 到底是哪个单态化实例、调用点分析
//! （谁判 leader）——这三样都没有 ⇒ **只记观测，不落码**。
//!
//! # 4. 公开路径
//!
//! 本文件**没有 `fn`**，因此没有需要 `pub use` 再导出的老路径；原 `src/events.rs` 里
//! 属于本模块的只是"枚举 + 名字表"，那两样按 `panic::Location` 分别归
//! `graphwar_event.rs`（留在 `events.rs`）与 `graphwar_event_handler.rs`（T30 同日切出）。

/// 原版模块名（arm64 `.symtab` 里的 `graphwarrust::graphwar_remote_event_handler`）。
pub const MODULE: &str = "graphwar_remote_event_handler";

/// `panic::Location` 里逐字节出现的源文件名 —— 与上面 `MODULE` 同名，这就是
/// `AGENTS.md` §4 说的"文件名就是证据"的物证侧（台账判据直接读这个字面量）。
pub const SOURCE_FILE: &str = "src/graphwar_remote_event_handler.rs";

/// 与真二进制对照的锚定强度。分级沿用 `fixtures/name_to_addr.tsv` 的判据：
/// 同一个 `(file, line, col)` 在两条架构上都**只被这一个函数引用**才叫 `Unique`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// 双向唯一 `(file,line,col)` 锚。
    Unique,
    /// 无直接一对一锚，但按「一个名字只对应一个地址」消去后只剩一个候选。
    DeducedByElimination,
    /// 该函数不引用任何 `Location`，名字无法落地（纯胶水/纯内联）。
    NoAnchor,
}

/// 一个已归属的函数（证据行，不是行为实现）。
///
/// ⚠️ 为什么没有 arm64 列：本机 checkout 的 `.reference/upstream/` 只有 x86 的
/// `libgraphwarrust.so` 与 `analysis/funcstarts_ghidra_va.pkl`，**没有** arm64 `.so`；
/// 别的文档里引用的 arm64 地址在本 checkout 无法复算 ⇒ 宁可不给，也不抄二手数字
/// （`AGENTS.md` §0.3「每条结论必须标证据等级」）。
#[derive(Debug, Clone, Copy)]
pub struct Func {
    pub name: &'static str,
    /// x86 steam 2026-08-28 构建的 ELF vaddr（RVA）。
    pub x86: u64,
    /// `fixtures/name_to_addr_v2.tsv` 记的第一个 `(line, col)` 锚（真名的落地键）。
    pub loc_first: (u32, u32),
    /// `fixtures/boundary_audit.tsv` 里该函数落在本文件的**锚点行数**（不是"行号个数"以外的口径）。
    pub loc_lines: u32,
    pub link: Link,
}

/// 本模块**这两条**（= 台账里判 `mismatch` 的那两条）函数的证据行。
///
/// 第三个别名函数 `handle_server_remote_event`（`0x3e2830`，主文件 88 锚 + 另 8 个模块各 1–5 锚）
/// 台账判 `multi` ⇒ 按 `AGENTS.md` §4 **只登记、不搬、不列进本表**（`docs/06` §74.5）。
pub const FUNCTIONS: &[Func] = &[
    Func {
        name: "GraphwarRemoteEventHandler::handle_client_remote_event",
        x86: 0x3d_ee00,
        loc_first: (143, 99),
        loc_lines: 80,
        link: Link::Unique,
    },
    Func {
        name: "GraphwarRemoteEventHandler::is_leader",
        x86: 0x3e_5910,
        loc_first: (135, 103),
        loc_lines: 2,
        link: Link::Unique,
    },
];

// ---------------------------------------------------------------------------
// 5. 派发层（T49：从 `src/_scaffold/net_events_class.rs` 搬回本模块）
// ---------------------------------------------------------------------------
//
// # 为什么这段属于本模块
//
// 原版客户端收到服务端事件后由 **`GraphwarRemoteEventHandler::handle_client_remote_event`**
// （x86 **`0x3dee00`**，`panic::Location` 在本文件 **80 个锚**、首锚 `:143:99`；见 §1 的表）
// 分派到各 manager 的 setter。我方历史上把这段——"一条线上事件落到哪个 manager 的哪个方法"
// ——写在了 `src/_scaffold/net_events_class.rs`（文件头自述"**不是复原物**"），
// 违反 `AGENTS.md` §4「文件名就是证据」。T36 审计（`docs/06` §86.4）把这列为第 ④ 处。
//
// # 搬的是什么、没搬什么
//
// * **搬**：[`WireEvent`]（30 个线上变体）、[`WireEvent::tag`] / [`WireEvent::name`]、
//   [`Calls`]、[`dispatch`]（逐事件落点表）、[`parse_team`] 等 6 个"线上字符串 → 判别值"
//   解析器、[`is_bot_skin`]，以及三个接线小工具 `manager` / `drain` / `set_graphics`；
// * **没搬**（留在 scaffold，见台账 `a-*` / `n-05`）：`GwNetEvents` 的 40 个 `#[func]`
//   ——那是 **GDExtension 外壳，原版没有**；`ensure_installed` / `dispatch_installed`
//   ——那是本仓的 thread-local 通道（原版的对象图是 节点→backend→容器，本层没有）。
//
// # 形态差异（诚实登记，别当成逐位复原）
//
// 1. 原版分派体吃 **0x48 字节的 tagged union `Event`**，并在体内内联了大量 setter；
//    本层吃的是解析后的字段（[`WireEvent`]），再调复原 manager 的同名 setter。
// 2. 判别值 → 落点这张表**逐事件档位不同**：`PosInfo` / `NameInfo` / `Skin|Face|HatGraphics` /
//    `EntityEffect` / `EffectInfo` / `EntityRemoved` / `TeamInfo` / `RemoveTeam` / `ColorInfo` /
//    `SoldierInfo` / 三个 `*_mode_info` 是 **观测**（每个 manager 的模块文档都记着自己那几个
//    调用点地址）；`TargetInfo` / `LifeInfo` / `PlayerInfo` / `Function*` 是 **推断**
//    （客户端侧入口没有直接证据）；`FunctionFire` 是 **未知**（有意识不接）。
// 3. 本层用 `KernelBus`（thread-local）取 manager，原版是 `GraphwarBackend +0x220` 的容器
//    ⇒ 本文件**依赖 scaffold 的接线层**（`crate::_scaffold::root_wiring`）。这不是"复原"，
//    是把语义放回它该在的文件之后仍然缺少的对象图（登记在 `docs/06` §99 遗留）。

use crate::events::Event;
use crate::graphwar_effect_manager::EffectType;
use crate::graphwar_mode_manager::{AxisMode, FunctionMode, TimeMode};
use crate::graphwar_team_manager::GraphwarTeam;
use crate::graphwar_turn_manager::TurnMode;

// ⚠️ 下面这几个只在客户端特性下存在：派发体要取 22 个 manager，靠的是 scaffold 的容器接线
// （原版的对象图是 节点→backend→容器 `GraphwarBackend +0x220`，本层没有）⇒ 这部分是**接线**，
// 不是复原物；关掉 `gdext-bindings` 时本文件的**纯语义**（`WireEvent` / `dispatch_name` 类表 /
// 解析器）照旧可用，`dispatch` 本体不编译。
#[cfg(feature = "gdext-bindings")]
use std::cell::RefCell;
#[cfg(feature = "gdext-bindings")]
use std::rc::Rc;
#[cfg(feature = "gdext-bindings")]
use crate::_scaffold::root_wiring::{kernel_bus, KernelBus, RootWiring};
#[cfg(feature = "gdext-bindings")]
use crate::graphwar_color_manager::GraphwarColor;
#[cfg(feature = "gdext-bindings")]
use crate::graphwar_graphics_manager::GraphicsKind;

// ---------------------------------------------------------------------------
// 1. 线上事件（纯 Rust 形态：字段名/顺序照 `protocol.rs::WIRE_SPEC`）
// ---------------------------------------------------------------------------

/// 一条**客户端可能收到**的线上事件。
///
/// 只放客户端真会收到的那一半（服务端 → 客户端）；上行请求（`*Request`）不在这里。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WireEvent<'a> {
    /// 5 `{connection_id}`。
    NewConnection { connection_id: i64 },
    /// 13 `{player_id, connection_id}`。
    PlayerInfo { player_id: i64, connection_id: i64 },
    /// 15 `{soldier_id, player_id}`。
    SoldierInfo { soldier_id: i64, player_id: i64 },
    /// 23 `{target_id}`。
    TargetInfo { target_id: i64 },
    /// 24 `{entity_id, team}`。
    TeamInfo { entity_id: i64, team: &'a str },
    /// 25 `{entity_id}`。
    RemoveTeam { entity_id: i64 },
    /// 14 `{entity_id}`。
    EntityRemoved { entity_id: i64 },
    /// 26 `{entity_id, pos_x, pos_y, radius}`。
    PosInfo { entity_id: i64, x: f64, y: f64, radius: f64 },
    /// 27 `{entity_id, alive}`。
    LifeInfo { entity_id: i64, alive: bool },
    /// 28 `{entity_id, tick}`。
    LifeQueuedDeath { entity_id: i64, tick: u32 },
    /// 40 `{pos_x, pos_y, radius}`。
    AddObstacle { x: f64, y: f64, radius: f64 },
    /// 39 `{}`。
    ClearObstacles,
    /// 42 `{pos_x, pos_y, radius, tick}`。
    QueueDestroyObstacle { x: f64, y: f64, radius: f64, tick: i64 },
    /// 41 `{pos_x, pos_y, radius}`。
    DestroyObstacle { x: f64, y: f64, radius: f64 },
    /// 43 `{player_id, soldier_id}`。
    TurnInfo { player_id: i64, soldier_id: i64 },
    /// 47 `{turn_mode}`。
    TurnModeInfo { turn_mode: &'a str },
    /// 45 `{axis_mode}`。
    AxisModeInfo { axis_mode: &'a str },
    /// 46 `{function_mode}`。
    FunctionModeInfo { function_mode: &'a str },
    /// 48 `{time_mode}`。
    TimeModeInfo { time_mode: &'a str },
    /// 51 `{entity_id, graphics_str}`。
    SkinGraphicsInfo { entity_id: i64, graphics_str: &'a str },
    /// 53 `{entity_id, graphics_str}`。
    FaceGraphicsInfo { entity_id: i64, graphics_str: &'a str },
    /// 55 `{entity_id, graphics_str}`。
    HatGraphicsInfo { entity_id: i64, graphics_str: &'a str },
    /// 56 `{entity_id, effect_type}`。
    EntityEffectInfo { entity_id: i64, effect_type: &'a str },
    /// 60 `{entity_id, name}`。
    NameInfo { entity_id: i64, name: &'a str },
    /// 61 `{entity_id, color:{r,g,b}}`。
    ColorInfo { entity_id: i64, r: u8, g: u8, b: u8 },
    /// 33 `{soldier_id}`。
    FunctionFire { soldier_id: i64 },
    /// 35 `{owner_id, function, corrected_function}`。
    FunctionInfo { owner_id: i64, function: &'a str, corrected: &'a str },
    /// 36 `{owner_id, active_tick}`。
    FunctionActive { owner_id: i64, active_tick: i64 },
    /// 37 `{owner_id, start_x, start_y, end_x}`。
    FunctionPoints { owner_id: i64, start_x: f64, start_y: f64, end_x: f64 },
    /// 44 `{effect_id, pos_x, pos_y, effect_type, start_tick}`。
    EffectInfo { effect_id: i64, x: f64, y: f64, effect_type: &'a str, start_tick: i64 },
}

impl WireEvent<'_> {
    /// 线上判别值（`Event::wire_tag()`；`protocol.rs::WIRE_SPEC` 74/74 逐位）。
    pub fn tag(&self) -> u16 {
        match self {
            WireEvent::NewConnection { .. } => Event::NewConnection.wire_tag(),
            WireEvent::PlayerInfo { .. } => Event::PlayerInfo.wire_tag(),
            WireEvent::SoldierInfo { .. } => Event::SoldierInfo.wire_tag(),
            WireEvent::TargetInfo { .. } => Event::TargetInfo.wire_tag(),
            WireEvent::TeamInfo { .. } => Event::TeamInfo.wire_tag(),
            WireEvent::RemoveTeam { .. } => Event::RemoveTeam.wire_tag(),
            WireEvent::EntityRemoved { .. } => Event::EntityRemoved.wire_tag(),
            WireEvent::PosInfo { .. } => Event::PosInfo.wire_tag(),
            WireEvent::LifeInfo { .. } => Event::LifeInfo.wire_tag(),
            WireEvent::LifeQueuedDeath { .. } => Event::LifeQueuedDeath.wire_tag(),
            WireEvent::AddObstacle { .. } => Event::AddObstacle.wire_tag(),
            WireEvent::ClearObstacles => Event::ClearObstacles.wire_tag(),
            WireEvent::QueueDestroyObstacle { .. } => Event::QueueDestroyObstacle.wire_tag(),
            WireEvent::DestroyObstacle { .. } => Event::DestroyObstacle.wire_tag(),
            WireEvent::TurnInfo { .. } => Event::TurnInfo.wire_tag(),
            WireEvent::TurnModeInfo { .. } => Event::TurnModeInfo.wire_tag(),
            WireEvent::AxisModeInfo { .. } => Event::AxisModeInfo.wire_tag(),
            WireEvent::FunctionModeInfo { .. } => Event::FunctionModeInfo.wire_tag(),
            WireEvent::TimeModeInfo { .. } => Event::TimeModeInfo.wire_tag(),
            WireEvent::SkinGraphicsInfo { .. } => Event::SkinGraphicsInfo.wire_tag(),
            WireEvent::FaceGraphicsInfo { .. } => Event::FaceGraphicsInfo.wire_tag(),
            WireEvent::HatGraphicsInfo { .. } => Event::HatGraphicsInfo.wire_tag(),
            WireEvent::EntityEffectInfo { .. } => Event::EntityEffectInfo.wire_tag(),
            WireEvent::NameInfo { .. } => Event::NameInfo.wire_tag(),
            WireEvent::ColorInfo { .. } => Event::ColorInfo.wire_tag(),
            WireEvent::FunctionFire { .. } => Event::FunctionFire.wire_tag(),
            WireEvent::FunctionInfo { .. } => Event::FunctionInfo.wire_tag(),
            WireEvent::FunctionActive { .. } => Event::FunctionActive.wire_tag(),
            WireEvent::FunctionPoints { .. } => Event::FunctionPoints.wire_tag(),
            WireEvent::EffectInfo { .. } => Event::EffectInfo.wire_tag(),
        }
    }

    /// 线上事件名（= `WIRE_SPEC` 的 `name`）。
    pub fn name(&self) -> &'static str {
        match self {
            WireEvent::NewConnection { .. } => "NewConnection",
            WireEvent::PlayerInfo { .. } => "PlayerInfo",
            WireEvent::SoldierInfo { .. } => "SoldierInfo",
            WireEvent::TargetInfo { .. } => "TargetInfo",
            WireEvent::TeamInfo { .. } => "TeamInfo",
            WireEvent::RemoveTeam { .. } => "RemoveTeam",
            WireEvent::EntityRemoved { .. } => "EntityRemoved",
            WireEvent::PosInfo { .. } => "PosInfo",
            WireEvent::LifeInfo { .. } => "LifeInfo",
            WireEvent::LifeQueuedDeath { .. } => "LifeQueuedDeath",
            WireEvent::AddObstacle { .. } => "AddObstacle",
            WireEvent::ClearObstacles => "ClearObstacles",
            WireEvent::QueueDestroyObstacle { .. } => "QueueDestroyObstacle",
            WireEvent::DestroyObstacle { .. } => "DestroyObstacle",
            WireEvent::TurnInfo { .. } => "TurnInfo",
            WireEvent::TurnModeInfo { .. } => "TurnModeInfo",
            WireEvent::AxisModeInfo { .. } => "AxisModeInfo",
            WireEvent::FunctionModeInfo { .. } => "FunctionModeInfo",
            WireEvent::TimeModeInfo { .. } => "TimeModeInfo",
            WireEvent::SkinGraphicsInfo { .. } => "SkinGraphicsInfo",
            WireEvent::FaceGraphicsInfo { .. } => "FaceGraphicsInfo",
            WireEvent::HatGraphicsInfo { .. } => "HatGraphicsInfo",
            WireEvent::EntityEffectInfo { .. } => "EntityEffectInfo",
            WireEvent::NameInfo { .. } => "NameInfo",
            WireEvent::ColorInfo { .. } => "ColorInfo",
            WireEvent::FunctionFire { .. } => "FunctionFire",
            WireEvent::FunctionInfo { .. } => "FunctionInfo",
            WireEvent::FunctionActive { .. } => "FunctionActive",
            WireEvent::FunctionPoints { .. } => "FunctionPoints",
            WireEvent::EffectInfo { .. } => "EffectInfo",
        }
    }
}

// ---------------------------------------------------------------------------
// 2. 分派（纯逻辑、零引擎依赖 —— 单测直接调这里）
// ---------------------------------------------------------------------------

/// 一条事件分派后**实际调到的** manager 方法（按调用顺序，形如 `"name_manager::set_name"`）。
///
/// 这是**自检口**：GDScript 侧拿它证明"事件真的进了哪个 manager 的哪个方法"，不用相信注释；
/// Rust 侧的单测也用它钉住映射表。
pub type Calls = Vec<&'static str>;

/// 把一条事件派发给复原出来的 manager，返回**调到的 manager 方法**。
///
/// 返回空表有两种原因：① 这条事件本来就没有落点（`FunctionFire` / `NewConnection`，见文件头）；
/// ② 内核通道没装（先调 [`ensure_installed`]）。
#[cfg(feature = "gdext-bindings")]
pub fn dispatch(ev: &WireEvent<'_>) -> Calls {
    let Some(bus) = kernel_bus() else {
        return Calls::new();
    };
    let mut calls = Calls::new();
    match *ev {
        WireEvent::NewConnection { .. } => {
            // 原版各 manager 的 `handle_new_connection_event` 是**服务端**侧"整表重播"，
            // 客户端收到这条只记 `connection_id`（界面层 `session.gd` 自己管）。
        }
        WireEvent::PlayerInfo { player_id, connection_id } => {
            if let (Ok(pid), Ok(cid)) = (u32::try_from(player_id), u32::try_from(connection_id)) {
                if let Some(m) = manager(&bus, |w| w.player_manager()) {
                    m.borrow_mut().add_player(pid, cid);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("player_manager::add_player");
                }
            }
        }
        WireEvent::SoldierInfo { soldier_id, player_id } => {
            if let (Ok(sid), Ok(pid)) = (u32::try_from(soldier_id), u32::try_from(player_id)) {
                if let Some(m) = manager(&bus, |w| w.soldier_manager()) {
                    m.borrow_mut().add_soldier(sid, pid);
                    calls.push("soldier_manager::add_soldier");
                }
            }
        }
        WireEvent::TargetInfo { target_id } => {
            if let Ok(id) = u32::try_from(target_id) {
                if let Some(m) = manager(&bus, |w| w.target_manager()) {
                    m.borrow_mut().add_target(id);
                    calls.push("target_manager::add_target");
                }
            }
        }
        WireEvent::TeamInfo { entity_id, team } => {
            if let (Ok(id), Some(team)) = (u32::try_from(entity_id), parse_team(team)) {
                if let Some(m) = manager(&bus, |w| w.team_manager()) {
                    m.borrow_mut().add_to_team(id, team);
                    calls.push("team_manager::add_to_team");
                }
            }
        }
        WireEvent::RemoveTeam { entity_id } => {
            if let Ok(id) = u32::try_from(entity_id) {
                if let Some(m) = manager(&bus, |w| w.team_manager()) {
                    m.borrow_mut().remove_from_team(id);
                    calls.push("team_manager::remove_from_team");
                }
            }
        }
        WireEvent::EntityRemoved { entity_id } => {
            let Ok(id) = u32::try_from(entity_id) else { return calls };
            let tag = Event::EntityRemoved.wire_tag();
            // 每个 manager 自己都订了 `entity_removed_event`（逐 manager 证据见文件头表）。
            if let Some(slot) = manager(&bus, |w| w.plane_pos_manager()) {
                let inner = Rc::clone(&slot.borrow().0);
                let _ = inner.borrow().handle_entity_removed_event(tag, id);
                calls.push("plane_pos_manager::handle_entity_removed_event");
            }
            if let Some(m) = manager(&bus, |w| w.name_manager()) {
                m.borrow_mut().handle_entity_removed_event(id);
                drain(&bus, m.borrow_mut().take_outbox());
                calls.push("name_manager::handle_entity_removed_event");
            }
            if let Some(m) = manager(&bus, |w| w.graphics_manager()) {
                m.borrow_mut().handle_entity_removed_event(id);
                drain(&bus, m.borrow_mut().take_outbox());
                calls.push("graphics_manager::handle_entity_removed_event");
            }
            if let Some(m) = manager(&bus, |w| w.target_manager()) {
                let _ = m.borrow_mut().handle_entity_removed_event(tag, id);
                calls.push("target_manager::handle_entity_removed_event");
            }
            if let Some(m) = manager(&bus, |w| w.entity_effect_manager()) {
                let _ = m.borrow_mut().handle_entity_removed_event(tag, id);
                drain(&bus, m.borrow_mut().take_outbox());
                calls.push("entity_effect_manager::handle_entity_removed_event");
            }
            if let Some(m) = manager(&bus, |w| w.life_manager()) {
                let _ = m.borrow_mut().handle_entity_removed_event(tag, id);
                drain(&bus, m.borrow_mut().take_outbox());
                calls.push("life_manager::handle_entity_removed_event");
            }
            if let Some(m) = manager(&bus, |w| w.color_manager()) {
                m.borrow_mut().handle_entity_removed_event(id);
                drain(&bus, m.borrow_mut().take_outbox());
                calls.push("color_manager::handle_entity_removed_event");
            }
            if let Some(m) = manager(&bus, |w| w.team_manager()) {
                m.borrow_mut().handle_entity_removed_event(id);
                calls.push("team_manager::handle_entity_removed_event");
            }
            if let Some(m) = manager(&bus, |w| w.turn_manager()) {
                m.borrow_mut().handle_entity_removed_event(i64::from(id));
                drain(&bus, m.borrow_mut().take_outbox());
                calls.push("turn_manager::handle_entity_removed_event");
            }
        }
        WireEvent::PosInfo { entity_id, x, y, radius } => {
            if let Ok(id) = u32::try_from(entity_id) {
                // `KernelBus::set_position` = `GraphwarPlanePosManager::set_position`（`0x3c64c0`）
                if bus.borrow().set_position(id, [x, y, radius]) {
                    calls.push("plane_pos_manager::set_position");
                }
            }
        }
        WireEvent::LifeInfo { entity_id, alive } => {
            if let Ok(id) = u32::try_from(entity_id) {
                if let Some(m) = manager(&bus, |w| w.life_manager()) {
                    m.borrow_mut().set_alive(id, alive);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("life_manager::set_alive");
                }
            }
        }
        WireEvent::LifeQueuedDeath { entity_id, tick } => {
            if let Ok(id) = u32::try_from(entity_id) {
                if let Some(m) = manager(&bus, |w| w.life_manager()) {
                    m.borrow_mut().set_queued_death(id, tick);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("life_manager::set_queued_death");
                }
            }
        }
        WireEvent::AddObstacle { x, y, radius } => {
            if let Some(m) = manager(&bus, |w| w.obstacle_manager()) {
                m.borrow_mut().table.add_obstacle(x, y, radius);
                calls.push("obstacle_manager::add_obstacle");
            }
        }
        WireEvent::ClearObstacles => {
            if let Some(m) = manager(&bus, |w| w.obstacle_manager()) {
                m.borrow_mut().table.clear_obstacles();
                calls.push("obstacle_manager::clear_obstacles");
            }
        }
        WireEvent::QueueDestroyObstacle { x, y, radius, tick } => {
            // T49：原版 `queue_destroy_obstacle`（`0x3f20f0`）的**键就是线上载荷里的 tick**
            // （`0x3f20fe mov ebx,edx`，体内没有 `now + delay`）⇒ 这里把 tick 直接当键传；
            // 负数 tick（线上不会出现）按 0 处理，不 panic。
            let tick = u32::try_from(tick).unwrap_or(0);
            if let Some(m) = manager(&bus, |w| w.obstacle_manager()) {
                m.borrow_mut().table.queue_destroy_obstacle(tick, x, y, radius);
                calls.push("obstacle_manager::queue_destroy_obstacle");
            }
        }
        WireEvent::DestroyObstacle { x, y, radius } => {
            if let Some(m) = manager(&bus, |w| w.obstacle_manager()) {
                m.borrow_mut().table.destroy_obstacle(x, y, radius);
                calls.push("obstacle_manager::destroy_obstacle");
            }
        }
        WireEvent::TurnInfo { player_id, soldier_id } => {
            if let Some(m) = manager(&bus, |w| w.turn_manager()) {
                m.borrow_mut().set_turn(player_id, soldier_id);
                drain(&bus, m.borrow_mut().take_outbox());
                calls.push("turn_manager::set_turn");
            }
        }
        WireEvent::TurnModeInfo { turn_mode } => {
            if let Some(mode) = parse_turn_mode(turn_mode) {
                if let Some(m) = manager(&bus, |w| w.turn_manager()) {
                    m.borrow_mut().set_turn_mode(mode);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("turn_manager::set_turn_mode");
                }
            }
        }
        WireEvent::AxisModeInfo { axis_mode } => {
            if let Some(mode) = parse_axis_mode(axis_mode) {
                if let Some(m) = manager(&bus, |w| w.mode_manager()) {
                    m.borrow_mut().set_axis_mode(mode);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("mode_manager::set_axis_mode");
                }
            }
        }
        WireEvent::FunctionModeInfo { function_mode } => {
            if let Some(mode) = parse_function_mode(function_mode) {
                if let Some(m) = manager(&bus, |w| w.mode_manager()) {
                    m.borrow_mut().set_function_mode(mode);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("mode_manager::set_function_mode");
                }
            }
        }
        WireEvent::TimeModeInfo { time_mode } => {
            if let Some(mode) = parse_time_mode(time_mode) {
                if let Some(m) = manager(&bus, |w| w.mode_manager()) {
                    m.borrow_mut().set_time_mode(mode);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("mode_manager::set_time_mode");
                }
            }
        }
        WireEvent::SkinGraphicsInfo { entity_id, graphics_str } => {
            if set_graphics(&bus, GraphicsKind::Skin, entity_id, graphics_str) {
                calls.push("graphics_manager::set_skin_graphics");
            }
            // `GraphwarGameStateManager::handle_graphics_info`（`0x4741a0`）的副作用：
            // `SkinGraphicsInfo{bot_*}` ⇒ 登记 `AshEffect`（`0x4aa464`，判别值 3）。
            if is_bot_skin(graphics_str) {
                if let (Ok(id), Some(m)) =
                    (u32::try_from(entity_id), manager(&bus, |w| w.entity_effect_manager()))
                {
                    m.borrow_mut().add_entity_effect(id, EffectType::Ash);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("entity_effect_manager::add_entity_effect(Ash)");
                }
            }
        }
        WireEvent::FaceGraphicsInfo { entity_id, graphics_str } => {
            if set_graphics(&bus, GraphicsKind::Face, entity_id, graphics_str) {
                calls.push("graphics_manager::set_face_graphics");
            }
        }
        WireEvent::HatGraphicsInfo { entity_id, graphics_str } => {
            if set_graphics(&bus, GraphicsKind::Hat, entity_id, graphics_str) {
                calls.push("graphics_manager::set_hat_graphics");
            }
        }
        WireEvent::EntityEffectInfo { entity_id, effect_type } => {
            if let (Ok(id), Some(kind)) = (u32::try_from(entity_id), parse_effect_type(effect_type))
            {
                if let Some(m) = manager(&bus, |w| w.entity_effect_manager()) {
                    m.borrow_mut().add_entity_effect(id, kind);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("entity_effect_manager::add_entity_effect");
                }
            }
        }
        WireEvent::NameInfo { entity_id, name } => {
            if let Ok(id) = u32::try_from(entity_id) {
                if let Some(m) = manager(&bus, |w| w.name_manager()) {
                    m.borrow_mut().set_name(id, name);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("name_manager::set_name");
                }
            }
        }
        WireEvent::ColorInfo { entity_id, r, g, b } => {
            if let Ok(id) = u32::try_from(entity_id) {
                if let Some(m) = manager(&bus, |w| w.color_manager()) {
                    m.borrow_mut().set_color(id, GraphwarColor::new(r, g, b));
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("color_manager::set_color");
                }
            }
        }
        WireEvent::FunctionFire { .. } => {
            // **没有落点**（证据缺口，见文件头"为什么 FunctionFire 不硬接"）：不猜。
        }
        WireEvent::FunctionInfo { owner_id, function, corrected } => {
            let _ = corrected; // 修正文本只做交叉证据；token 槽存的是**原始**函数
            if let (Ok(id), Some(m)) =
                (u32::try_from(owner_id), manager(&bus, |w| w.function_manager()))
            {
                let tokens = crate::math_function::parse(function);
                m.borrow_mut().table.set_function(id, tokens);
                drain(&bus, m.borrow_mut().table.take_outbox());
                calls.push("function_manager::set_function");
            }
        }
        WireEvent::FunctionActive { owner_id, active_tick } => {
            if let (Ok(id), Ok(tick)) = (u32::try_from(owner_id), u32::try_from(active_tick)) {
                if let Some(m) = manager(&bus, |w| w.function_manager()) {
                    m.borrow_mut().table.set_function_active_tick(id, tick);
                    drain(&bus, m.borrow_mut().table.take_outbox());
                    calls.push("function_manager::set_function_active_tick");
                }
            }
        }
        WireEvent::FunctionPoints { owner_id, start_x, start_y, end_x } => {
            let _ = end_x; // 终点只给界面画"落点标记"；表里存的是**起点**
            if let Ok(id) = u32::try_from(owner_id) {
                if let Some(m) = manager(&bus, |w| w.function_manager()) {
                    m.borrow_mut().table.set_function_points(id, start_x, start_y);
                    drain(&bus, m.borrow_mut().table.take_outbox());
                    calls.push("function_manager::set_function_points");
                }
            }
        }
        WireEvent::EffectInfo { effect_id, x, y, effect_type, start_tick } => {
            if let (Ok(eid), Some(kind), Ok(tick)) = (
                u32::try_from(effect_id),
                parse_effect_type(effect_type),
                u32::try_from(start_tick),
            ) {
                if let Some(m) = manager(&bus, |w| w.effect_manager()) {
                    m.borrow_mut().add_effect(eid, (x, y), kind, tick);
                    drain(&bus, m.borrow_mut().take_outbox());
                    calls.push("effect_manager::add_effect");
                }
            }
        }
    }
    calls
}

// ---------------------------------------------------------------------------
// 3. 线上字符串 → 判别值（只认**观测到过的**拼写，其余一律 `None`，不猜）
// ---------------------------------------------------------------------------

/// `TeamInfo.team`：`"Team1"` / `"Team2"`（`GraphwarTeam::deserialize` 的变体名，`观测`）。
pub fn parse_team(s: &str) -> Option<GraphwarTeam> {
    match s {
        "Team1" => Some(GraphwarTeam::Team1),
        "Team2" => Some(GraphwarTeam::Team2),
        _ => None,
    }
}

/// `TurnModeInfo.turn_mode`：`"SequentialTurns"` / `"SimultaneousTurns"`（抓包观测到的两个）。
pub fn parse_turn_mode(s: &str) -> Option<TurnMode> {
    match s {
        "SequentialTurns" => Some(TurnMode::SequentialTurns),
        "SimultaneousTurns" => Some(TurnMode::SimultaneousTurns),
        _ => None,
    }
}

/// `AxisModeInfo.axis_mode`：只观测到 `"EveryUnit"`（其余拼写**未知** ⇒ `None`）。
pub fn parse_axis_mode(s: &str) -> Option<AxisMode> {
    match s {
        "EveryUnit" => Some(AxisMode::EveryUnit),
        _ => None,
    }
}

/// `FunctionModeInfo.function_mode`：只观测到 `"NormalFunction"`。
pub fn parse_function_mode(s: &str) -> Option<FunctionMode> {
    match s {
        "NormalFunction" => Some(FunctionMode::NormalFunction),
        _ => None,
    }
}

/// `TimeModeInfo.time_mode`：`Timer30s` / `Timer1m` / `Timer2m` / `Timer3m` / `Timer5m` / `TimerInf`
/// （抓包里出现过后四个里的 1m/2m/3m/Inf；30s 与 5m 按 `graphwar_mode_manager::TimeMode` 的变体名补全）。
pub fn parse_time_mode(s: &str) -> Option<TimeMode> {
    match s {
        "Timer30s" => Some(TimeMode::Timer30s),
        "Timer1m" => Some(TimeMode::Timer1m),
        "Timer2m" => Some(TimeMode::Timer2m),
        "Timer3m" => Some(TimeMode::Timer3m),
        "Timer5m" => Some(TimeMode::Timer5m),
        "TimerInf" => Some(TimeMode::TimerInf),
        _ => None,
    }
}

/// `*EffectInfo.effect_type`：serde 变体名 = `EffectType` 变体名 + `Effect`
/// （判别值见 `graphwar_effect_manager` §2；四个名字在抓包/回放里都出现过）。
pub fn parse_effect_type(s: &str) -> Option<EffectType> {
    match s {
        "ExplosionEffect" => Some(EffectType::Explosion),
        "DeathEffect" => Some(EffectType::Death),
        "SparkEffect" => Some(EffectType::Spark),
        "AshEffect" => Some(EffectType::Ash),
        _ => None,
    }
}

/// `bot_` 前缀的皮肤 ⇒ 登记 `AshEffect`（`graphwar_entity_effect_manager` §6.1，**逐位**）。
pub fn is_bot_skin(s: &str) -> bool {
    s.len() > 4 && s.starts_with("bot_")
}

// ---------------------------------------------------------------------------
// 4. 小工具（取 manager / 搬 outbox）
// ---------------------------------------------------------------------------

/// 从内核通道里按类型取一个 manager（`Rc` 克隆出来，借用只活在这一次调用里）。
#[cfg(feature = "gdext-bindings")]
fn manager<T: 'static, F>(bus: &Rc<RefCell<KernelBus>>, get: F) -> Option<Rc<RefCell<T>>>
where
    F: Fn(&RootWiring) -> Option<Rc<RefCell<T>>>,
{
    let borrowed = bus.borrow();
    get(borrowed.wiring())
}

/// 把 manager 发出的事件搬进那条**共享队列**（原版 manager 拿到的就是同一个 `Rc`）。
#[cfg(feature = "gdext-bindings")]
fn drain(bus: &Rc<RefCell<KernelBus>>, events: Vec<Event>) {
    if events.is_empty() {
        return;
    }
    let queue = bus.borrow().queue();
    let mut q = queue.borrow_mut();
    for ev in events {
        q.add_event(ev);
    }
}

#[cfg(feature = "gdext-bindings")]
fn set_graphics(
    bus: &Rc<RefCell<KernelBus>>,
    kind: GraphicsKind,
    entity_id: i64,
    name: &str,
) -> bool {
    let Ok(id) = u32::try_from(entity_id) else { return false };
    bus.borrow().set_graphics(kind, id, name)
}
