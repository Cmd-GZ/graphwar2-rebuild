//! 对局录像：`GraphwarGameRecorder` —— **把每条事件连同当时的 tick 写成 gzip JSONL**。
//!
//! 本模块回答三件事：**录什么**（32 条 `*_info_event` 的全部事件）、
//! **什么时候落盘**（`GameStateInfo == EndingGame` 那一刻）、
//! **写到哪**（`user://replays/<unix 秒>`，gzip 压缩，一行一个 JSON）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（3 个，全部已定位）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x3d7e3c` / 840 | `0x3d5260` / 1240 | — | `Rc::new` 缓冲 + 自指 `Weak` + 32 条订阅 | 逐位 |
//! | `subscribe_to_events` 闭包 | `0x3d7b54` / 744 | `0x3d4fa0` / 704 | `:107:105` `:108:41` | 记录 + 判 `EndingGame` 触发落盘 | 逐位 |
//! | `save_game` | `0x3d8184` / 2720 | `0x3d5740` / 2368 | `:150:81` `:156:74` | 建目录、拼名、gzip 写盘 | 逐位 |
//!
//! 闭包与 `save_game` 的 x86 尺寸比 = 0.95 / 0.87（闸 0.61–1.19），
//! 且两侧 Locations 逐枚相同 ⇒ 两个名字都是钉死的。
//! `new` 在 x86 侧不是"内联进宿主"（旧写法），它有**自己的 FDE**（`0x3d5260..0x3d5738`，0x4d8 字节）
//! —— 见 `docs/06` §132.2。
//!
//! ## 2. 数据结构（逐位）—— 0x50 是 **`RcInner`**，结构体本身是 **0x38**
//!
//! ```text
//! GraphwarGameRecorder                              ; size_of = 0x38 = 56
//!   +0x00 : Vec<EventInfo>                         ; 24B；字段序 cap/ptr/len = +0x00/+0x08/+0x10
//!   +0x18 : Weak<RefCell<GraphwarGameRecorder>>    ; **自指** Weak（new 里 Rc::downgrade 自己）
//!   +0x20 : Rc<RefCell<GraphwarEventQueue>>        ; new 的实参 2（直接存，不拆）
//!   +0x28 : Weak<RefCell<GraphwarModuleContainer>> ; new 的实参 3（取 tick 走它的 get_module_wk）
//!   +0x30 : u8                                     ; 上一条 GameStateInfo 的 game_state，初值 8
//!
//! RcInner<RefCell<GraphwarGameRecorder>>            ; __rust_alloc(0x50, 8) 的那个 0x50
//!   +0x00 : strong = 1
//!   +0x08 : weak   = 1 + 1（自指 Weak）+ 32（32 个闭包各 clone 一份）
//!   +0x10 : RefCell 借用标志（构造期与闭包内 = -1 = 独占借用）
//!   +0x18 : GraphwarGameRecorder（上面那个 0x38）
//!
//! EventInfo = { event: GraphwarEvent /*0x48 字节*/, tick: u32 }  ; 0x50 = 80 字节
//! ```
//!
//! **`0x50` 的口径**（T78 记成"结构体尺寸 0x50 = 3×24B 字段 + byte@0x48"，本轮改正）：
//! `0x50 = 0x10（strong+weak）+ 0x08（RefCell 借用标志）+ 0x38（结构体）`。
//! ⇒ 结构体从 RcInner 的 `+0x18` 开始，所以 `last_state` **在结构体内是 `+0x30`、
//! 从 RcInner 基址算是 `+0x48`** —— 旧文档写的 `+0x48` 是后一个口径，两个都对，混着写才成了矛盾。
//!
//! 逐条证据（x86 = 本机 `2026-08-28` 构建，arm64 = 2.0.60 未 strip 构建，两架构布局相同）：
//!
//! | 结论 | 指令 / 数据 |
//! |---|---|
//! | 尺寸 0x50 且是 `RcInner` | x86 `0x3d528c mov edi,0x50` + `0x3d5291 mov esi,8` → `__rust_alloc`；`0x3d52d8 mov qword [rax],1`（strong=1）；`0x3d52a8 mov qword [rax+8],2`（weak=2 = 1+自指）；arm64 `Rc::drop_slow` 里 `mov w1,#0x50` 的 `__rust_dealloc` 由 **weak 归零**触发 ⇒ 该窗口是 RcInner |
//! | 结构体 = 0x38（56） | ① `0x50 - 0x10 - 0x08 = 0x38`；② `GraphwarModuleContainer` 注册表 `add_module` 的 size 实参 **56**（`src/graphwar_module_container.rs:396`）；③ arm64 `drop_in_place<RefCell<GraphwarGameRecorder>>`（`0x4b2864`）里 `ldr x0,[x19,#0x20]`/`ldr x0,[x19,#0x30]`/`ldur x8,[x19,#0x8]` 三个字段偏移只在 0x38 下自洽 |
//! | `Vec<EventInfo>` 在结构体 `+0x00` | 闭包 `0x3d5049 lea r12,[rbx+0x18]` 作为 `grow_one` 的 `&Vec`；`0x3d5090 mov r13,[rbx+0x28]` = len、`0x3d5094 cmp r13,[rbx+0x18]` = cap、`0x3d50a3 mov rax,[rbx+0x20]` = ptr；元素步长 `lea rcx,[r13*4]; add rcx,r13; movups …[rax+rcx*?]` = 5·len·16 = `0x50` |
//! | `+0x18` = 自指 `Weak<RefCell<Self>>` | x86 `new` `0x3d52c8 mov qword [rax+0x30], rax`（写入的就是 RcInner 基址）；`0x3d5658 inc qword [rbx+8]`（每次订阅一次 `Weak` clone）；arm64 符号表 `drop_in_place<alloc::rc::Weak<RefCell<GraphwarGameRecorder>>>`（`0x417434`）+ `Rc::drop_slow<RefCell<GraphwarGameRecorder>>`（`0x41c228`）在 `+0x30`（= 结构体 `+0x18`）读的就是这个 Weak |
//! | `+0x20` = `Rc<RefCell<GraphwarEventQueue>>` | `0x41c228` 在 `+0x38`（= 结构体 `+0x20`）做 `dec` 后调 `0x41a4b8`，而后者 `drop_in_place<RefCell<GraphwarEventQueue>>` + `dealloc(0x58)` ⇒ `RcInner<RefCell<GraphwarEventQueue>>` = 0x58；`new` 实参 1（x86 `rsi` / arm64 `x1`）直接写进这里 |
//! | `+0x28` = `Weak<RefCell<GraphwarModuleContainer>>` | 闭包 `0x3d500a lea rdi,[rbx+0x40]`（= 结构体 `+0x28`）传进 `GraphwarModuleContainer::get_module_wk`，而它第一句 `ldr x20,[x0]` + `cmn x20,#-1` 就是读一个 `Weak`；`Weak::drop` 的 `dealloc` 尺寸 **0x48** ⇒ `RcInner<RefCell<GraphwarModuleContainer>>` = 0x48 ⇒ 容器 = `0x30` = `HashMap<TypeId, Rc<dyn Any>>`（`src/graphwar_module_container.rs:546` 的 48 字节） |
//! | `+0x30` = `u8` 状态，初值 **8** | x86 `0x3d52d4 mov byte [rax+0x48], 8`、闭包 `0x3d510a cmp byte [rbx+0x48],6` / `0x3d51a3 mov byte [rbx+0x48],6` / `0x3d5141 mov byte [rbx+0x48],bpl`；arm64 `0x3d7fd0 strb w23,[x0,#0x48]` / `0x3d7c80 ldrb w8,[x19,#0x48]` |
//!
//! ## 3. 录什么、何时录（逐位）
//!
//! 闭包体（arm64 `0x3d7b54` / x86 `0x3d4fa0`）：
//!
//! ```text
//! let me = self_weak.upgrade()?;                          ; Weak::new() 的 -1 哨兵直接 return
//! let mut guard = me.borrow_mut();                        ; borrow 标志 0 → -1（-1 = 独占借用）
//! let tick = get_module_wk(&me.module_container)?.borrow()[+0x58];   ; 当前 tick（u32）
//! me.events.push(EventInfo { event: event.clone(), tick })
//!
//! if event.tag == GameStateInfo(30) {
//!     let state = event.payload[+0x8] as u8
//!     if state == 6 /* EndingGame */ {
//!         if me.last_state != 6 { me.save_game() }         ; 0x3d7d70 / 0x3d519f
//!         me.last_state = 6
//!     } else if state == 1 /* Setup */ && me.last_state == 6 {
//!         me.events.clear()                                ; 新一局：清缓冲（0x3d7c8c）
//!         me.last_state = 1
//!     } else { me.last_state = state }
//! }
//! ; guard 释放：borrow 标志 +1 → 0（0x3d5155 inc qword [rbx+0x10]）
//! ```
//!
//! 32 条订阅名（`new` 里 32 个 `'static` 串 + 32 个长度立即数），**同一个闭包**：
//! 符号表里 `subscribe_to_events::{{closure}}` 只有**一个** hash ⇒ 32 个 `Box`
//! 装的是同一个闭包类型。
//!
//! | # | 事件名 | # | 事件名 | # | 事件名 | # | 事件名 |
//! |---|---|---|---|---|---|---|---|
//! | 1 | `team_info_event` | 9 | `game_state_info_event` | 17 | `turn_info_event` | 25 | `skin_graphics_info_event` |
//! | 2 | `player_info_event` | 10 | `function_fire_event` | 18 | `effect_info_event` | 26 | `face_graphics_info_event` |
//! | 3 | `entity_removed_event` | 11 | `function_info_event` | 19 | `axis_mode_info_event` | 27 | `hat_graphics_info_event` |
//! | 4 | `soldier_info_event` | 12 | `function_active_event` | 20 | `function_mode_info_event` | 28 | `entity_effect_info_event` |
//! | 5 | `target_info_event` | 13 | `function_points_event` | 21 | `turn_mode_info_event` | 29 | `turn_limit_info_event` |
//! | 6 | `remove_team_event` | 14 | `clear_obstacles_event` | 22 | `time_mode_info_event` | 30 | `turn_count_info_event` |
//! | 7 | `pos_info_event` | 15 | `add_obstacle_event` | 23 | `end_of_turn_info_event` | 31 | `name_info_event` |
//! | 8 | `life_info_event` | 16 | `destroy_obstacle_event` | 24 | `game_name_info_event` | 32 | `color_info_event` |
//!
//! 32 条里 **24 条以 `_info_event` 结尾**，另外 **8 条**是同样由服务端单向广播、
//! 但名字不带 `_info` 的：`entity_removed_event` / `remove_team_event` /
//! `function_fire_event` / `function_active_event` / `function_points_event` /
//! `clear_obstacles_event` / `add_obstacle_event` / `destroy_obstacle_event`。
//! **`*_request_event` 一条都没订** ⇒ 录像里只有权威事件流（客户端的请求进不去）。
//!
//! ## 4. 写到哪、什么格式（逐位）
//!
//! `save_game`（arm64 `0x3d8184`）的固定序列：
//!
//! ```text
//! DirAccess::make_dir_recursive_absolute("user://replays/", 15)         ; 0x3d81b8
//! let secs = SystemTime::now().duration_since(UNIX_EPOCH).as_secs()     ; 0x3d81bc-0x3d81f4
//! let path = "user://replays/".to_owned() + &secs.to_string()           ; 0x3d8240-0x3d827c
//! FileAccess::open(path, /*WRITE=*/2)                                    ; 0x3d8290
//! GzEncoder::new(file, ..) + 每条记录一行 JSON                          ; flate2 1.0.30
//! ```
//!
//! 一行 JSON 的构造（`0x3d830c` 起，手写而非 `serde_json::to_writer`）：
//!
//! ```text
//! '{"event":' + <GraphwarEvent 的 serde 外部标签 JSON> + ',"tick":' + N + '}'
//! ```
//!
//! 两个字段名 `"event"` / `"tick"` 是 `.rodata` 里的字面量（`0x19bb8d` / `0x19332c`），
//! 与 `src/graphwar_replay_player.rs` 解析 400 份真实回放时用的格式**逐字相同**
//! ⇒ 格式这条是**双向印证**的（写侧反汇编 + 读侧 400 份真数据）。
//! 压缩侧同样印证：`save_game` 引用的 `flate2-1.0.30/src/gz/write.rs` 与
//! `zio.rs` 的 `panic::Location` 说明写的是 **gzip**（读侧 `gunzip` 也在用）。
//!
//! ## 5. 我方与真值的差异（自造部分，逐条登记）
//!
//! * **`save_game` 不真写盘**：原版建目录 + `FileAccess::open` + `GzEncoder` 逐行写；
//!   本 crate 不引外部依赖（`flate2` / `gzip` 都不在依赖表里），`save_game` 只返回
//!   `Vec<EventInfo>`（`save_game_text` 再拼成行）。压缩口径见
//!   `src/_scaffold/replay_inflate.rs` 与 `docs/16-client.md` §29。
//!   ⇒ **落盘字节**由 `tests/graphwar_game_recorder.rs::save_game_bytes_are_frozen_across_the_layout_change`
//!   逐字节冻结（改结构前后必须完全相同）。
//! * **tick 是实参**：原版闭包里 tick 是自己从 `get_module_wk(&module_container)` 拿的
//!   （`?.borrow()[+0x58]`）；本 crate 由调用方把 tick 传进 `record`。
//!   `module_container` 字段仍然按真值落位并接上（`new` 的第 3 个实参），
//!   但**那个 `get_module_wk::<T>` 的 `T` 还没定**（符号表里泛型被擦除，只知读 `+0x58`
//!   当 tick —— 与 `src/graphwar_effect_manager.rs:117` 同一处未结案）⇒ 不编语义，标 **推断**。
//! * **`detached()`**：原版没有无参构造（`new` 一定带 3 个实参）。本 crate 为单测/独立使用
//!   提供它，显式标"自造入口"。
//! * **不注册闭包**：原版 `new` 里 32 次 `subscribe_to_event(name, Box<closure>)`；
//!   本 crate 的 `GraphwarEventHandler` 目前只登记名字（见该模块 §tick 派发），
//!   ⇒ `new` 把 32 条名字喂进去（条数、顺序与真值一致），闭包体本身由 `record*` 直接实现。
//!
//! ## 6. 未知
//!
//! * 文件名**没有扩展名**，只有 unix 秒（`as_secs`）——同一秒开两局会互相覆盖，
//!   原版就这样，不是我们抄错；
//! * 记录**没有上限**（闭包里没有任何截断），一局很长时内存线性增长；
//! * `EventInfo` 的 `tick` 是 u32 还是 u64：结构体内偏移 0x48 + 元素 0x50 只说明
//!   「4 字节 + 4 字节填充」，按 `[+0x58]` 是 u32 推断为 **u32**；
//! * `last_state` 的初值 **8**：`GraphwarGameState` 只有 0..=7 八个变体
//!   （`src/turnstate.rs:38`）⇒ 8 是"还没见过任何状态"的哨兵，但原版源码里它是怎么写出来的
//!   （`8`／`VARIANTS.len()`／别的枚举）**证不到**，只知值 = 8；因为闭包里只有 `==6` / `==1`
//!   两个比较，初值取任何 ∉{1,6} 的值行为都相同 ⇒ 语义影响为 0。

use core::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::events::Event;
use crate::graphwar_event_handler::GraphwarEventHandler;
use crate::graphwar_event_queue::GraphwarEventQueue;
use crate::graphwar_module_container::GraphwarModuleContainer;
use crate::turnstate::GameState;

/// 录像目录（含结尾斜杠，长度 15）。
pub const REPLAY_DIR: &str = "user://replays/";

/// `EventInfo` 在内存里的步长（`0x50`）。
pub const EVENT_INFO_STRIDE: usize = 0x50;

/// `GraphwarEvent` 的字节数（`EventInfo` 里事件本体占的部分）。
pub const GRAPHWAR_EVENT_SIZE: usize = 0x48;

/// `size_of::<GraphwarGameRecorder>()` = **56**（逐位）。
///
/// 来源三条：`__rust_alloc(0x50,8)` 的是 `RcInner`（`0x50 - 0x10 - 0x08 = 0x38`）、
/// `add_module` 的 size 实参 56（`src/graphwar_module_container.rs:396`）、
/// 以及 arm64 `drop_in_place<RefCell<GraphwarGameRecorder>>` 的字段偏移。
pub const RECORDER_SIZE: usize = 0x38;

/// `size_of::<RefCell<GraphwarGameRecorder>>()` = `借用标志(8) + 结构体(0x38)`。
pub const REFCELL_RECORDER_SIZE: usize = 0x40;

/// `RcInner<RefCell<GraphwarGameRecorder>>` 的字节数 = `strong(8) + weak(8) + 0x40`。
///
/// 这就是 `new` 开头 `__rust_alloc(0x50, 8)`（x86 `0x3d528c`）的真实含义 ——
/// **不是**结构体尺寸。
pub const RC_INNER_SIZE: usize = 0x50;

/// `last_state` 在**结构体内**的偏移（从 `RcInner` 基址算是 `0x48`）。
pub const OFFSET_LAST_STATE: usize = 0x30;

/// 32 条订阅名，顺序 = `new` 里写入顺序（模块文档 §3 的表）。
pub const SUBSCRIPTIONS: [&str; 32] = [
    "team_info_event",
    "player_info_event",
    "entity_removed_event",
    "soldier_info_event",
    "target_info_event",
    "remove_team_event",
    "pos_info_event",
    "life_info_event",
    "game_state_info_event",
    "function_fire_event",
    "function_info_event",
    "function_active_event",
    "function_points_event",
    "clear_obstacles_event",
    "add_obstacle_event",
    "destroy_obstacle_event",
    "turn_info_event",
    "effect_info_event",
    "axis_mode_info_event",
    "function_mode_info_event",
    "turn_mode_info_event",
    "time_mode_info_event",
    "end_of_turn_info_event",
    "game_name_info_event",
    "skin_graphics_info_event",
    "face_graphics_info_event",
    "hat_graphics_info_event",
    "entity_effect_info_event",
    "turn_limit_info_event",
    "turn_count_info_event",
    "name_info_event",
    "color_info_event",
];

/// 触发落盘 / 清空的两个状态（`docs/07` §3.5 的 `GraphwarGameState`）。
pub const STATE_ENDING_GAME: u8 = GameState::EndingGame as u8; // 6
pub const STATE_SETUP: u8 = GameState::Setup as u8; // 1

/// `last_state` 的初值 —— **8**（逐位：x86 `0x3d52d4 mov byte [rax+0x48], 8`、
/// arm64 `0x3d7fd0 strb w23, [x0, #0x48]`）。
///
/// `GameState` 只有 0..=7 八个变体 ⇒ 8 是"还没见过任何状态"的哨兵；
/// 闭包里对它的比较只有 `== 6` 与 `== 1` 两处，所以取任何 ∉{1,6} 的初值行为相同。
pub const INITIAL_LAST_STATE: u8 = 8;

/// 录像记录里的事件本体 —— 原版 `EventInfo.event` 的类型是 **`GraphwarEvent`**
/// （`0x48` 字节的 serde **外部标签枚举**：74 个事件变体各带自己的载荷）。
///
/// 我方没有把 74 个变体做成枚举（那需要 74 个载荷结构体，见 `docs/06` §82.4-3 的登记），
/// 而是用 `name`（外部标签的那个 key）+ `body`（载荷原文）**两串承载同一个类型**。
/// ⇒ 字段数已对上（`EventInfo` = 2），但 `GraphwarEvent` 的**内部表示仍是登记项**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphwarEvent {
    /// 事件名（serde 外部标签的那个 key，例如 `"FunctionInfo"`）。
    pub name: String,
    /// 该事件的 JSON 载荷（原样透传，**不重排、不省略**）。
    pub body: String,
}

/// 一条录像记录（`EventInfo`）—— **2 个字段**，声明顺序 `event` → `tick`。
///
/// 逐位证据（`docs/06` §103.3）：
/// * `E1`：`.data.rel.ro` 槽 `0x950560` → `struct EventInfo with 2 elements`（`0x1bcba0`）；
/// * 字段名 `event`(`0x1bc845`) / `tick`(`0x1b925c`)；**顺序**来自 serde 生成的
///   `visit_map` 末尾 `missing_field` 的调用顺序（`0x397592` event 先于 `0x397843` tick，
///   二者在 `Deserialize` visitor `0x396de0` 里）；
/// * 内存布局：`event@0x00`（0x48 字节）、`tick@0x48`（**4 字节**：`save_game` `0x3d5740`
///   在 `0x3d62ac` 用 `mov ecx, dword ptr [rax+rsi+0x48]` 读它）、元素步长 `0x50`
///   （记录闭包 `0x3d4fa0` 的 `Vec` 扩容）。
///
/// ⚠️ 改前我方是 **3** 字段且顺序反了（`tick, event, body`）—— 那是 T32 §82.4-2
/// 「我方没有这个结构体」之外的另一处偏差，本轮按 `fixtures/struct_audit_plan2.txt` 修掉。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventInfo {
    pub event: GraphwarEvent,
    pub tick: u32,
}

impl EventInfo {
    /// 一行 JSON：`{"event":{"<Name>":<body>},"tick":<N>}`。
    ///
    /// 键序 `event` → `tick` = **声明顺序**（`E3` 的 missing_field 顺序 + 400 份真实回放逐字相同）。
    pub fn to_json_line(&self) -> String {
        if self.event.body.is_empty() {
            format!("{{\"event\":{{\"{}\":{{}}}},\"tick\":{}}}", self.event.name, self.tick)
        } else {
            format!(
                "{{\"event\":{{\"{}\":{}}},\"tick\":{}}}",
                self.event.name, self.event.body, self.tick
            )
        }
    }

    /// 事件名（外部标签 key）。
    pub fn name(&self) -> &str {
        &self.event.name
    }

    /// 事件载荷原文。
    pub fn body(&self) -> &str {
        &self.event.body
    }
}

/// 录像文件名：`user://replays/<unix 秒>`（**没有扩展名**，模块文档 §4）。
pub fn replay_path(unix_secs: u64) -> String {
    format!("{}{}", REPLAY_DIR, unix_secs)
}

/// 对局录像器 —— **5 个字段、0x38 字节**，布局见模块文档 §2。
///
/// `#[repr(C)]` 让偏移可复算（原版是 Rust 默认布局；这 5 个字段的对齐都是 8 或 1，
/// 默认布局与 `repr(C)` 同形），偏移由本文件的布局 pin 单测钉死。
///
/// 旧形态（T78 之前）是 `{events, last_state@0x18, subscribed, saves}` = 0x40，
/// 后两个字段是本 crate 自造的、**原版没有** —— 已删（`saves`/`subscribed`
/// 的统计改由调用方/测试自己做）。
pub struct GraphwarGameRecorder {
    /// `+0x00`：录像缓冲（`Vec<EventInfo>`，24 字节）。
    events: Vec<EventInfo>,
    /// `+0x18`：**自指** Weak（原版 `new` 里 `Rc::downgrade(&this)`；
    /// 闭包 `subscribe_to_events::{{closure}}` 捕捉的就是它，用来 upgrade 回 `Rc`）。
    self_weak: Weak<RefCell<GraphwarGameRecorder>>,
    /// `+0x20`：出站事件队列（`new` 的实参 2；原版直接存 `Rc`，不拆）。
    event_queue: Rc<RefCell<GraphwarEventQueue>>,
    /// `+0x28`：模块容器 Weak（`new` 的实参 3；原版闭包里
    /// `get_module_wk(&self.module_container)?.borrow()[+0x58]` 拿当前 tick）。
    module_container: Weak<RefCell<GraphwarModuleContainer>>,
    /// `+0x30`：上一条 `GameStateInfo` 的 `game_state`（初值 [`INITIAL_LAST_STATE`] = 8；
    /// 从 `RcInner` 基址算是 `+0x48`）。
    last_state: u8,
}

impl core::fmt::Debug for GraphwarGameRecorder {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GraphwarGameRecorder")
            .field("events", &self.events)
            .field("event_queue", &self.event_queue)
            .field("last_state", &self.last_state)
            .finish_non_exhaustive()
    }
}

impl Default for GraphwarGameRecorder {
    /// 等价于 [`GraphwarGameRecorder::detached`]（本 crate 自造入口，原版没有）。
    fn default() -> Self {
        Self::detached()
    }
}

impl GraphwarGameRecorder {
    /// 原版 `new`（arm64 `0x3d7e3c` / x86 `0x3d5260`）：`Rc::new` + 自指 `Weak` + 32 条订阅。
    ///
    /// 逐位对应（x86 地址）：
    ///
    /// ```text
    /// 0x3d528c  __rust_alloc(0x50, 8)                  → Rc::new(RefCell::new(Self { .. }))
    /// 0x3d52d8  [rax]    = 1                           → strong = 1
    /// 0x3d52a8  [rax+8]  = 2                           → weak   = 1 + 自指那次 downgrade
    /// 0x3d52e3  [rax+0x10] = -1                        → borrow_mut（构造期一直占着）
    /// 0x3d52c8  [rax+0x30] = rax                       → self_weak = Rc::downgrade(&this)
    /// 0x3d52cc  [rax+0x38] = 实参1                     → event_queue
    /// 0x3d52d0  [rax+0x40] = 实参2                     → module_container
    /// 0x3d52d4  [rax+0x48] = 8                         → last_state = 8
    /// 0x3d5658  inc qword [rbx+8]   ×32                → 每订阅一次 Weak::clone
    /// 0x3d569a  call subscribe_to_event ×32            → 32 条订阅
    /// 0x3d56aa  inc qword [rbx+0x10]                   → guard 释放（borrow 标志 -1 → 0）
    /// ```
    ///
    /// 返回值是 `Rc<RefCell<Self>>`（不是 `Self`）—— 因为原版要存自指 `Weak`，
    /// 只能在 `Rc` 里建好之后再 `downgrade` 回自己。
    pub fn new(
        event_handler: &mut GraphwarEventHandler,
        event_queue: Rc<RefCell<GraphwarEventQueue>>,
        module_container: Weak<RefCell<GraphwarModuleContainer>>,
    ) -> Rc<RefCell<Self>> {
        let this = Rc::new(RefCell::new(Self {
            events: Vec::new(),
            self_weak: Weak::new(),
            event_queue,
            module_container,
            last_state: INITIAL_LAST_STATE,
        }));
        {
            // 原版构造期一直持着 `borrow_mut`（RcInner `+0x10` = -1），最后 `inc` 释放。
            let mut me = this.borrow_mut();
            me.self_weak = Rc::downgrade(&this);
            for name in SUBSCRIPTIONS {
                // 原版这里是 `subscribe_to_event(name, Box::new(同一个闭包))`；
                // 我方 handler 目前只登记名字（模块文档 §5）。
                event_handler.subscribe_to_event(name);
            }
        }
        this
    }

    /// **本 crate 自造入口**（原版没有无参构造）：自带的空队列 + 两个悬空 `Weak`。
    ///
    /// 单测 / 独立使用走这里；要和真值一致的对象走 [`GraphwarGameRecorder::new`]。
    pub fn detached() -> Self {
        Self {
            events: Vec::new(),
            self_weak: Weak::new(),
            event_queue: Rc::new(RefCell::new(GraphwarEventQueue::new())),
            module_container: Weak::new(),
            last_state: INITIAL_LAST_STATE,
        }
    }

    pub fn events(&self) -> &[EventInfo] {
        &self.events
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn last_state(&self) -> u8 {
        self.last_state
    }

    /// 自指 `Weak`（原版 `+0x18`）。`detached()` 造出来的对象里它是空的（`Weak::new()`）。
    pub fn self_weak(&self) -> &Weak<RefCell<GraphwarGameRecorder>> {
        &self.self_weak
    }

    /// 出站队列（原版 `+0x20`）。
    pub fn event_queue(&self) -> &Rc<RefCell<GraphwarEventQueue>> {
        &self.event_queue
    }

    /// 模块容器 Weak（原版 `+0x28`）。
    pub fn module_container(&self) -> &Weak<RefCell<GraphwarModuleContainer>> {
        &self.module_container
    }

    /// 录一条事件（arm64 `0x3d7b54` 的前半段）。
    ///
    /// 返回 `true` 表示这一次调用触发了 `save_game`（`EndingGame` 的上升沿）。
    pub fn record(&mut self, tick: u32, event: Event, body: &str) -> bool {
        let name = format!("{:?}", event);
        self.events.push(EventInfo {
            event: GraphwarEvent { name, body: body.to_string() },
            tick,
        });
        self.on_event_state(event)
    }

    /// 录一条 `GameStateInfo` 并跑状态机（`0x3d7c60` 起的那段）。
    ///
    /// 返回 `true` 表示触发了落盘。
    pub fn record_game_state(&mut self, tick: u32, state: u8) -> bool {
        let body = format!("{{\"game_state\":{}}}", state);
        self.events.push(EventInfo {
            event: GraphwarEvent { name: "GameStateInfo".to_string(), body },
            tick,
        });
        self.on_game_state(state)
    }

    /// 状态机本体（与「是否真的写了盘」解耦，便于测试）。
    fn on_event_state(&mut self, event: Event) -> bool {
        if event != Event::GameStateInfo {
            return false;
        }
        let state = parse_state(self.events[self.events.len() - 1].body());
        self.on_game_state(state)
    }

    fn on_game_state(&mut self, state: u8) -> bool {
        if state == STATE_ENDING_GAME {
            let prev = self.last_state;
            self.last_state = STATE_ENDING_GAME;
            if prev != STATE_ENDING_GAME {
                self.save_game();
                return true;
            }
            return false;
        }
        if state == STATE_SETUP && self.last_state == STATE_ENDING_GAME {
            // 新一局开始：丢掉上一局的缓冲（原版连 `drop_in_place` 都是一个循环）。
            self.events.clear();
            self.last_state = STATE_SETUP;
            return false;
        }
        self.last_state = state;
        false
    }

    /// 落盘（arm64 `0x3d8184` / x86 `0x3d5740`）—— 原版签名是 `fn save_game(&self)`（**不可变借用**）。
    ///
    /// 原版是「建目录 → 拼名 → `FileAccess::open` → `GzEncoder` → 逐行写」。
    /// 本 crate 不引外部依赖（`flate2` / `gzip` 都不在依赖表里），
    /// 所以这里只产生**未压缩的行**；压缩口径见 `src/_scaffold/replay_inflate.rs`
    /// 与 `docs/16-client.md` §29。
    pub fn save_game(&self) -> Vec<EventInfo> {
        self.events.clone()
    }

    /// 落盘时的完整文本（每行一条，行尾 `\n`），供对拍 / 调试。
    pub fn save_game_text(&self) -> String {
        let events = self.save_game();
        let mut out = String::new();
        for e in &events {
            out.push_str(&e.to_json_line());
            out.push('\n');
        }
        out
    }
}

fn parse_state(body: &str) -> u8 {
    // body 形如 `{"game_state":3}`；容错：取最后一个 ':' 后的数字。
    let digits: String = body
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.chars().rev().collect::<String>().parse().unwrap_or(0)
}

/// 布局 / 语义 **pin 单测**：把 `docs/06` §132 那套逐位结论钉在编译期可见的形状上。
///
/// 判据都来自**真二进制**（x86 `2026-08-28` 为主、arm64 2.0.60 未 strip 版配名字），
/// 不是"我方实现看起来对"。
#[cfg(test)]
mod layout_pins {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    /// 结构体 5 个字段的偏移与总尺寸（模块文档 §2 那张表的机检版）。
    #[test]
    fn recorder_layout_matches_the_binary_window() {
        assert_eq!(size_of::<GraphwarGameRecorder>(), RECORDER_SIZE);
        assert_eq!(size_of::<GraphwarGameRecorder>(), 56, "add_module 的 size 实参");
        assert_eq!(align_of::<GraphwarGameRecorder>(), 8);
        assert_eq!(offset_of!(GraphwarGameRecorder, events), 0x00);
        assert_eq!(offset_of!(GraphwarGameRecorder, self_weak), 0x18);
        assert_eq!(offset_of!(GraphwarGameRecorder, event_queue), 0x20);
        assert_eq!(offset_of!(GraphwarGameRecorder, module_container), 0x28);
        assert_eq!(offset_of!(GraphwarGameRecorder, last_state), OFFSET_LAST_STATE);
        assert_eq!(offset_of!(GraphwarGameRecorder, last_state), 0x30);
        // `Vec<EventInfo>` 自己占 24 字节 ⇒ 下一个字段必须正好接在 +0x18。
        assert_eq!(size_of::<Vec<EventInfo>>(), 24);
        assert_eq!(size_of::<Weak<RefCell<GraphwarGameRecorder>>>(), 8);
        assert_eq!(size_of::<Rc<RefCell<GraphwarEventQueue>>>(), 8);
        assert_eq!(size_of::<Weak<RefCell<GraphwarModuleContainer>>>(), 8);
    }

    /// `__rust_alloc(0x50, 8)` 的 **0x50 是 RcInner，不是结构体**（本轮最关键的更正）。
    #[test]
    fn rust_alloc_0x50_is_the_rc_inner_not_the_struct() {
        assert_eq!(size_of::<RefCell<GraphwarGameRecorder>>(), REFCELL_RECORDER_SIZE);
        // RcInner = { strong, weak, value }
        assert_eq!(
            size_of::<RefCell<GraphwarGameRecorder>>() + 2 * size_of::<usize>(),
            RC_INNER_SIZE
        );
        assert_eq!(RC_INNER_SIZE, 0x50);
        // 从 RcInner 基址算 last_state 才落在 `+0x48`（旧文档的口径）：
        // value(RefCell) 在 +0x10，结构体在 +0x18，结构体内的 +0x30。
        assert_eq!(2 * size_of::<usize>() + 8 + OFFSET_LAST_STATE, 0x48);
    }

    /// `EventInfo` 的**真值**步长 0x50 = 事件本体 0x48 + tick(4) + 4 填充。
    ///
    /// ⚠️ 我方 `GraphwarEvent` 还是**占位表示**（`{name: String, body: String}` = 0x30），
    /// 不是原版那个 0x48 的 74 变体枚举 ⇒ 我方 `EventInfo` 实际是 0x38。
    /// 这是**已登记的差异**（模块文档 §5 / `docs/06` §82.4-3），本轮结构对齐**没有**碰它；
    /// 谁把 `GraphwarEvent` 做成真枚举时，这条测试会红、并提示同步这里与文档。
    #[test]
    fn event_info_stride_constants_are_0x50() {
        // 真值（`mov w8,#0x50` + `madd` + `mov ecx,[rax+rsi+0x48]`）
        assert_eq!(EVENT_INFO_STRIDE, 0x50);
        assert_eq!(GRAPHWAR_EVENT_SIZE, 0x48);
        assert_eq!(EVENT_INFO_STRIDE - GRAPHWAR_EVENT_SIZE, 8, "tick(4) + 4 填充");
        assert_eq!(size_of::<u32>(), 4);
        // 我方占位形态（差异已登记，不算回归）
        assert_eq!(size_of::<GraphwarEvent>(), 0x30, "两个 String 的占位");
        assert_eq!(offset_of!(EventInfo, event), 0x00);
        assert_eq!(offset_of!(EventInfo, tick), 0x30);
        assert_eq!(size_of::<EventInfo>(), 0x38);
    }

    /// `new` 的三条接线（自指 Weak / 队列 Rc / 容器 Weak）+ 32 条订阅真的落到 handler 上。
    #[test]
    fn new_wires_self_weak_queue_container_and_32_subscriptions() {
        let mut handler = GraphwarEventHandler::new();
        let queue = Rc::new(RefCell::new(GraphwarEventQueue::new()));
        let container = Rc::new(RefCell::new(GraphwarModuleContainer::new()));
        let rec = GraphwarGameRecorder::new(&mut handler, queue.clone(), Rc::downgrade(&container));

        // `Rc::weak_count` = 原始 weak 计数 - 1（强引用自带的那个"隐式 weak"）。
        // 我方：`Rc::new`(1) + 自指 `Rc::downgrade`(1) ⇒ weak_count = 1。
        // ⚠️ 原版这里是 **33**（原始 34）：每条订阅的 `Box<closure>` 各持一份 `Weak` clone
        // （x86 `0x3d5658 inc qword [rbx+8]` ×32）。我方 handler 只登记名字、不存闭包
        // ⇒ 少 32（模块文档 §5 已登记）。这条断言钉的是**我方**形态，不是真值形态。
        assert_eq!(Rc::weak_count(&rec), 1);
        assert_eq!(Rc::strong_count(&rec), 1);

        // 自指 Weak：upgrade 出来的就是同一个 Rc（原版 `[rax+0x30] = rax`）。
        let upgraded = rec.borrow().self_weak().upgrade().expect("自指 Weak 必须能 upgrade");
        assert!(Rc::ptr_eq(&upgraded, &rec));
        // 队列是**同一个** Rc（原版直接存实参，不拆不 clone 内容）。
        assert!(Rc::ptr_eq(rec.borrow().event_queue(), &queue));
        // 容器 Weak 能 upgrade 回同一个。
        let back = rec.borrow().module_container().upgrade().expect("容器 Weak 必须能 upgrade");
        assert!(Rc::ptr_eq(&back, &container));

        // 32 条订阅各一次（原版 0x3d80c4 调 32 次 subscribe_to_event）。
        for name in SUBSCRIPTIONS {
            assert_eq!(handler.subscriber_count(name), 1, "{name} 没订上");
        }
        assert_eq!(handler.subscribers().len(), SUBSCRIPTIONS.len());
    }

    /// `last_state` 初值 8（逐位），且 8 **不是**任何 `GameState` 判别值 ⇒ 是哨兵。
    #[test]
    fn initial_last_state_is_eight_and_outside_the_game_state_range() {
        assert_eq!(INITIAL_LAST_STATE, 8);
        assert_eq!(GameState::VARIANTS.len(), 8);
        assert!(
            !GameState::VARIANTS.iter().any(|s| *s as u8 == INITIAL_LAST_STATE),
            "8 不该是合法 GameState 判别值"
        );
        // 两个初值口径都必须落在 8（`detached` 与 `new`）。
        assert_eq!(GraphwarGameRecorder::detached().last_state(), INITIAL_LAST_STATE);
        let mut h = GraphwarEventHandler::new();
        let rec = GraphwarGameRecorder::new(
            &mut h,
            Rc::new(RefCell::new(GraphwarEventQueue::new())),
            Weak::new(),
        );
        assert_eq!(rec.borrow().last_state(), INITIAL_LAST_STATE);
    }

    /// 状态机三条（与真值闭包逐条对应）：上升沿落盘、重复不落盘、`Setup` 清缓冲。
    #[test]
    fn state_machine_pins() {
        let mut r = GraphwarGameRecorder::detached();
        assert!(!r.record_game_state(1, 5)); // ExecutingResults
        assert!(r.record_game_state(2, STATE_ENDING_GAME), "EndingGame 上升沿必须落盘");
        assert!(!r.record_game_state(3, STATE_ENDING_GAME), "重复 EndingGame 不落盘");
        assert_eq!(r.len(), 3, "落盘不清缓冲（原版 0x3d7d5c 只写 last_state）");
        assert!(!r.record_game_state(4, STATE_SETUP), "Setup 不落盘");
        assert_eq!(r.len(), 0, "Setup 且上一条是 EndingGame ⇒ 清缓冲");
        assert_eq!(r.last_state(), STATE_SETUP);
    }

    /// `save_game` 是**不可变**借用（原版 `fn save_game(&self)`），且不改缓冲。
    #[test]
    fn save_game_takes_shared_borrow() {
        let mut r = GraphwarGameRecorder::detached();
        r.record(1, Event::TurnInfo, "{}");
        let shared: &GraphwarGameRecorder = &r;
        assert_eq!(shared.save_game().len(), 1);
        assert_eq!(shared.save_game_text(), shared.events()[0].to_json_line() + "\n");
        assert_eq!(r.len(), 1);
    }
}
