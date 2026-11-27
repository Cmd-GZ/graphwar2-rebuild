//! 原版 `src/graphwar_function_manager.rs` —— **32 字节的 `GraphwarFunctionManager` 的字段布局**
//! （任务 22，第四节：`dispatch/22-manager-layout.md`）。
//!
//! 本文件只放**布局**（`size_of` / `offset_of` 会被断言）：原版这个模块的**行为**
//! （`FunctionState` 里到底存什么、淡出曲线怎么算）**仍未复原**，见文末"仍是未知"。
//!
//! **挂载方式（已转正，2026-11-26 更正本文）**：早期 `src/lib.rs` 在禁区时，本文件是用
//! `#[path = "../graphwar_function_manager.rs"] pub mod graphwar_function_manager;` 从
//! `src/_scaffold/root_wiring.rs` 临时挂进 crate 的；**现在 `src/lib.rs` 已经直接
//! `pub mod graphwar_function_manager;` / `pub mod graphwar_obstacle_manager;`**
//! （同一批迁回 crate 根，两个文件一行没改），本段旧描述已过期。
//!
//! # 一、这个模块里有什么（`观测`，arm64 2.0.60 `.symtab` 原文）
//!
//! | 函数 | arm64 | x86 2.0.60 | `panic::Location` |
//! |---|---|---|---|
//! | `GraphwarFunctionManager::set_function` | `0x488a9c` (1460) | `0x455e30` | `src/graphwar_function_manager.rs:152:105` |
//! | `GraphwarFunctionManager::get_function` | `0x4887f4` (680) | `0x455b60` | `:165:26` |
//! | `GraphwarFunctionManager::get_function_text` | `0x489050` (552) | `0x4565f0` | `:169:26` |
//! | `GraphwarFunctionManager::set_function_active_tick` | `0x4895ec` (620) | `0x456b80` | `:181:41` |
//! | `GraphwarFunctionManager::set_function_points` | `0x489278` (884) | `0x456810` | `:192:41` |
//! | `GraphwarFunctionManager::get_active_functions_on_tick` | `0x489858` (416) | `0x456df0` | `:208:37` |
//! | `GraphwarFunctionManager::get_active_functions_on_tick_no_fade` | `0x4899f8` (416) | `0x456f80` | `:213:37` |
//! | `GraphwarFunctionData::get_function` | `0x488460` (376) | — | 无锚 |
//! | `GraphwarFunctionData::set_function` | `0x4885d8` (204) | — | 无锚 |
//! | `GraphwarFunctionData::get_function_fade_on_tick` | `0x4886a4` (336) | — | 无锚 |
//! | `GraphwarFunctionManager::new` | **内联**（见 §三） | **内联** `0x5236e0` | — |
//!
//! arm64 侧另有 `GraphwarFunctionManager::new::{{closure}}` 的 **drop glue**
//! `0x4168f0`（128 B，与障碍管理器那个被 LLVM 合并成同一份），说明构造走的是
//! `Rc::new_cyclic`（闭包型工厂），因此**没有独立的 `new` 符号**。
//!
//! # 二、32 字节 = 4 个 8 字节字段（`观测`，三份构建逐偏移一致）
//!
//! | 偏移 | 宽 | 类型 | 证据（指令地址） | 档位 |
//! |---|---|---|---|---|
//! | `+0x00` | 8 | `Weak<RefCell<GraphwarFunctionManager>>`（弱自句柄） | 构造点把 **`RcBox` 指针本身**写进该槽：arm64 `0x419d44` `stp xzr, x19, [x19, #0x10]`（第 2 字 = `RcBox+0x18` = `m+0x00`）、x86 `0x5237f3` `mov [rbx+0x18], rbx`、Windows DLL `0x18013f476` `mov %rsi,0x18(%rsi)`；析构把它交给 `drop_in_place<Weak<RefCell<GraphwarFunctionManager>>>`（arm64 `0x419df0`、DLL `0x18013f4f9`） | **观测** |
//! | `+0x08` | 8 | `Rc<RefCell<GraphwarEventQueue>>` | `set_function` 读它 → `+0x10` 是 `RefCell` 借用位（写 `-1`）→ `+0x18` 当 `&mut GraphwarEventQueue` 传给 `GraphwarEventQueue::add_event`：arm64 `0x488dc4` → `0x488e30`（`bl 0x3d7a40`，符号名 = `add_event`）；x86 `0x4562e8` → `0x45638f`（GOT `0x9bac80`）；`set_function_active_tick` 同（arm64 `0x489708`、x86 `0x456ce1`）。构造点第二字（arm64 闭包参数 `x20`）、析构做 `Rc` 强计数 `-1`/`drop_slow` | **观测** |
//! | `+0x10` | 8 | `Weak<RefCell<GraphwarModuleContainer>>` | `set_function` 把 `self+0x10` 当 `&Weak` 传给 `GraphwarModuleContainer::get_module_wk`：arm64 `0x488ac0` `add x0, x0, #0x10` → `bl 0x4a34c4`（该函数第一句 `ldr x20,[x0]` + `cmn x20,#1` = `Weak::new` 哨兵判定）；x86 `0x455e5a` → `call 0x46d040`。析构 `drop_in_place<Weak<RefCell<GraphwarModuleContainer>>>`（arm64 `0x419dcc`、DLL `0x18013f4e2`） | **观测** |
//! | `+0x18` | 8 | `Arc<Mutex<GraphwarFunctionData>>` | **7 个方法的第一句**都是读这个槽：arm64 `0x488814` / `0x488c48` / `0x489068` / `0x489298` / `0x489608` / `0x48986c` / `0x489a0c`；x86 `0x455b74` / `0x456604` / `0x45682b` / `0x456b9e` / `0x456e03` / `0x456f93`。`ArcInner` 布局 `{strong@0, weak@8, Mutex@0x10}`，`std::sync::Mutex` 的锁字在 `+0x10`、中毒位 `+0x14`、数据 `+0x18`（arm64 `ldaxr w9,[x+0x10]`、`ldrb w8,[x+0x14]`；x86 `lock cmpxchg dword [x+0x10]`）；析构 `Arc::drop_slow` | **观测** |
//!
//! **尺寸的两条独立证据**（都不依赖 `docs` 里的 vtable 表）：
//!
//! 1. `Rc<RefCell<GraphwarFunctionManager>>` 的 `RcBox` 分配是 **`0x38` = 56 = 24 + 32**
//!    （arm64 `0x419ca4` `mov w0, #0x38`；x86 `0x523704` `mov edi, 0x38`；
//!    DLL `0x18013f381` 同一常量，且错误路径 `0x18013f4a2` 的 `handle_alloc_error(size=0x38)` 对得上）；
//! 2. `add_module` 的 `dyn Any` vtable 第 1 字 = `size_of::<RefCell<M>>()` = **40** = 32 + 8（`docs/16-client.md` §41.2 第 8 行，两架构逐字相同）。
//!
//! # 三、`new` 在哪、实参是什么（`观测`）
//!
//! 原版没有独立的 `new` 符号——构造被**整个内联**进 `Rc::…::new_cyclic_in` 里：
//!
//! | 构建 | 构造体地址 | 说明 |
//! |---|---|---|
//! | arm64 2.0.60 | `0x419c80`（`Rc<T,A>::new_cyclic_in`，416 B） | 被 `GraphwarGameClient::new` 在 `0x40a158` 直接调用 |
//! | x86 2.0.60 | `0x5236e0`（208 B） | 被 `GraphwarGameClient::new` `0x517b9f` 调用，紧接 `add_module::<GraphwarFunctionManager>` `0x469db0` |
//! | Windows 2.0.60 DLL | `0x18013f360` | `GraphwarFunctionManager::new`（`0x1800d9ab0`）是个 16 B 的 thunk，只做 `jmp` |
//!
//! 三份构建的字段写入顺序**完全一致**（`movq %rsi,0x18(%rsi)` / `mov %rdi,0x20(%rsi)` /
//! `mov %rbx,0x28(%rsi)` / `mov %rax,0x30(%rsi)`），即
//! `new(event_queue: Rc<RefCell<GraphwarEventQueue>>, container: Weak<RefCell<GraphwarModuleContainer>>)`
//! ——两个实参就是管理器里 `+0x08` / `+0x10` 两个字段，弱自句柄由 `Rc::new_cyclic` 的
//! `&Weak` 直接给出（所以 `+0x00` 里存的就是 `RcBox` 地址，见上表）。
//!
//! # 四、内层 `GraphwarFunctionData` = 48 字节（`观测`），就是一个 `HashMap<u32, FunctionState>`
//!
//! `ArcInner<Mutex<GraphwarFunctionData>>` 的分配是 **`0x48` = 72 = 16 + 8 + 48**
//! （arm64 `0x419d2c`、x86 `0x5237aa`、DLL `0x18013f42e`，三份都是这个常量）⇒ 内层 **48 字节**。
//! 48 = `HashMap<u32, _>` 的固定尺寸（`RawTable` 32 + `RandomState` 16），字段偏移也逐字对上：
//!
//! | `Data+` | 内容 | 证据 |
//! |---|---|---|
//! | `+0x00` | `RawTable.ctrl` | `ldp x9, x10, [x21, #0x18]`（arm64 `0x4898ac`）；x86 `mov r13,[r15+0x18]` |
//! | `+0x08` | `RawTable.bucket_mask` | 同上第 2 字 |
//! | `+0x10` | `RawTable.growth_left` | `cmp qword [r15+0x30], 0`（DLL `0x1800d9538` 一带） |
//! | `+0x18` | `RawTable.items` | 同上 |
//! | `+0x20` | `RandomState`（16 B） | `lea rdi, [r15+0x38]` = `&hasher`（`H` 前缀的 `hash_one` 调用点） |
//!
//! 值类型 `FunctionState` 的**大小**可由 bucket 步长反推：`docs/16-client.md` §42.2 已测得
//! bucket 步长 **`0x78` = 120**，而 `(u32, FunctionState)` 的 `size` = `align_up(8 + S, 8)` ⇒
//! `S ∈ {112}`（`S` 含 `f64`/`Vec`，`align = 8`，故必须是 8 的倍数）⇒ **112 字节（`推断`）**。
//! 缺的证据：`GraphwarFunctionData` 里没有一处把 112 字节整体 `memcpy`/`drop`，
//! 也没有 `size_of::<FunctionState>()` 的直接可读常量。
//!
//! # 五、就地更正（`AGENTS.md` §0.6）：`set_*` **会**发事件
//!
//! `docs/16-client.md` §44.2 / §44.3 写「`function_manager` 的四个 `set_*` 体内
//! **零条** `GraphwarEventQueue::add_event`（`0x3d3530`）⇒ 它不发事件」。**这条是错的**，
//! 成因是**只扫了 `call rel32` 直调、漏了 GOT 间接调用**（`AGENTS.md`
//! 「间接调用与函数归属」第 1 条那个坑）：
//!
//! ```text
//! x86 0x455e30 (set_function)          0x45638f  call qword [rip+..] slot 0x9bac80 -> 0x3d3530
//! x86 0x456810 (set_function_points)   0x4569e1  call qword [rip+..] slot 0x9bac80 -> 0x3d3530
//! x86 0x456b80 (set_function_active_tick) 0x456ce1  call qword [rip+..] slot 0x9bac80 -> 0x3d3530
//! arm64 0x488a9c (set_function)        0x488e30  bl 0x3d7a40   ; .symtab 名 = GraphwarEventQueue::add_event
//! ```
//!
//! `set_function` 发的是 **tag `0x23` = 35 = `FunctionInfo`**（arm64 `0x488e18`
//! `mov x8, #0x23; movk x8, #0x8000, lsl #48`，载荷里带 `(id, 文本, tick)`），
//! 与 `docs/02-module-map.md:524` 对 `0x455e30` 的既有记载
//! （「再发事件 tag 35」）一致——也就是说 §44 那条结论同时**与本仓自己的 docs/02 冲突**。
//!
//! # 六、仍是未知 / 推断（诚实清单）
//!
//! | 项 | 档位 | 缺什么 |
//! |---|---|---|
//! | `FunctionState`（112 B）的字段语义 | **未知** | 只从 §42.2 的三条谓词知道 `+0x50` 是个 `Option`-like（与 `i64::MIN` 比）、`+0x70` 是判别值 `1`、`+0x74` 是 `active_tick`(u32)、`+0x58/+0x60` 是 `Vec<Vector2D<f64>>` 的 ptr/len；**没有**逐字段定名 |
//! | `FunctionState` 的精确大小 | **推断**（112 B） | 缺「一次性 `memcpy`/`drop` 整结构」或 `size_of` 常量；只有 bucket 步长 `0x78` 反推（见 §四） |
//! | `+0x00` 弱自句柄**谁在用** | **未知** | 7 个方法**没有一处**读 `m+0x00`；本模块又不订阅事件（构造体内无 `subscribe`）⇒ 只能靠"别的模块替它订阅"来解释，未观测到 |
//! | `get_active_functions_on_tick` 的两条谓词语义 | **未知** | §42.5 已列（`value+0x70 == 1`、`value+0x50 != i64::MIN`） |
//! | 我们这份布局的**行为** | **已落码**（T49） | 见 §七：`FunctionTable` 的 6 个方法（原位在 `_scaffold/root_wiring.rs`）；`src/game.rs` 的自由函数是另一条（对局服务器侧的求值） |
//! | 「函数多久算活跃」 | **已解开**（T88，见 §八） | 窗口 = `len/30 + 60` tick（揭示 `len/30` + 淡出 `60`）；**不**再是未知 |
//! | `get_function_fade_on_tick` 的算式 | **已解开**（T88，见 §八） | x86 落点 = **`0x455a40`**（此前记为"没有落点、走 GOT 槽 `0x9bbd08`"——那个槽的 addend 就是它） |
//!
//! # 八、活跃窗口与淡出（T88 落码；逐位翻自 x86 `0x3be540` / `0x3bf620` / `0x455a40`）
//!
//! 三个函数构成"一条弹道画多久"的全部语义，**之前全是空白**：
//!
//! | 函数 | 地址 | 算式（原版指令） |
//! |---|---|---|
//! | `get_active_functions_on_tick` | `0x456df0` → 内联谓词 `0x3be540` | `+0x70 == 1` ∧ `+0x50 != i64::MIN` ∧ `tick > active_tick`（**严格**）∧ `30·d < len + 1800` |
//! | `get_active_functions_on_tick_no_fade` | `0x456f80` → 谓词 `0x3bf620` | 同上，但末项是 `30·d < len`（**没有** `+1800` 的尾巴） |
//! | `get_function_fade_on_tick` | **`0x455a40`** | 前三个条件不满足 ∨ `d ≤ len/30` ⇒ **`1.0`**；否则 `max(0.0, 1.0 − (d − len/30)/60)` |
//!
//! 关键指令（`0x3be5e6`–`0x3be613`，`get_active_functions_on_tick` 的过滤循环体）：
//!
//! ```text
//! 3be5e6  cmpl $0x1,-0x8(%rbp,%rcx,1)   ; value+0x70 == 1
//! 3be5f0  cmp  %r13,-0x28(%rcx)         ; r13 = 0x8000_0000_0000_0000 = i64::MIN
//! 3be5f6  mov  %eax,%edx / sub -0x4(%rcx),%edx   ; d = tick - value+0x74（u32）
//! 3be5fb  jbe  skip                     ; tick <= active_tick ⇒ 不在场（**严格大于**）
//! 3be5fd  mov  -0x18(%rcx),%rdi         ; len = value+0x60（u32）
//! 3be601  lea  (%rdx,%rdx,1),%r8d / shl $5,%edx / sub %r8d,%edx   ; 30·d
//! 3be60b  add  $0x708,%edi              ; len + 1800（0x708）
//! 3be611  cmp  %edi,%edx / jae skip     ; 30·d >= len+1800 ⇒ 不在场
//! ```
//!
//! 淡出（`0x455a40`，全部指令）：
//!
//! ```text
//! 455afc  cmpl $0x1,-0x8(%rcx) / jne  default   ; +0x70 != 1 ⇒ 1.0
//! 455b04  cmp  %rax,-0x28(%rcx) / jo   default  ; +0x50 == i64::MIN ⇒ 1.0
//! 455b0a  sub  -0x4(%rcx),%ebx / jbe   default  ; d <= 0 ⇒ 1.0
//! 455b0f  mov  -0x18(%rcx),%rax / mov %eax,%eax  ; len（u64 → 截成 u32）
//! 455b15  mov  $0x88888889,%ecx / imul %rax,%rcx / shr $0x24,%rcx  ; len/30（无符号）
//! 455b22  sub  %ecx,%ebx / jbe  default          ; d <= len/30 ⇒ 1.0（还在揭示）
//! 455b29  cvtsi2sd %rbx,%xmm0
//! 455b2e  divsd  0x19fab0,%xmm0                  ; 0xc04e… = **-60.0**
//! 455b36  addsd  0x19f570,%xmm0                  ; 0x3ff0… = **1.0**
//! 455b42  maxsd  %xmm1,%xmm0                     ; max(alpha, 0.0)；xmm1 = 0
//! 455b48  default: movq 0x19f570,%xmm0 ⇒ 1.0
//! ```
//!
//! ⇒ **活跃窗口 = 揭示 `len/30` tick + 淡出 `60` tick**（60 Hz ⇒ 满打满算 `len/30 + 60` tick ≈ 1 s 淡出）。
//! 窗口一过，`GraphwarPlane::draw` 走 `FunctionDrawer::remove_function`（`0x4b6418`，`0x498b70`）把线删掉；
//! 淡出期间每帧走 `update_function_fade(id, alpha)`（`0x4b6a0e` → `0x4992b0`）。
//!
//! ⚠️ `+0x70` / `+0x50` 两个字段的**名字与语义仍未知**；本层用"表里有这一格"代替这两个判定
//! （登记为本层的差，不是原版行为）。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};

use crate::events::Event;
use crate::graphwar_event_queue::GraphwarEventQueue;
use crate::graphwar_module_container::GraphwarModuleContainer;
use crate::math_function;
use crate::Token;

/// 原版 `FunctionState`（`HashMap<u32, FunctionState>` 的值）——**字段未解**。
///
/// 只做到两件事：**大小 112 字节**（`推断`：由 bucket 步长 `0x78` = `align_up(8 + S, 8)` 且
/// `S` 含 `f64`/`Vec`（`align = 8`，故 `S` 是 8 的倍数）反推），以及它是 `HashMap` 的值。
///
/// 已知的三个字段位置（`docs/16-client.md` §42.2，`观测`，相对**值**起点）：
/// `+0x50` 与 `i64::MIN` 比较（`Option`-like）、`+0x70` 判别值 `== 1`、
/// `+0x74` 是 `u32` 的 `active_tick`、`+0x58`/`+0x60` 是点数组的 `ptr`/`len`。
/// 本类型故意**不给字段名**（按 `AGENTS.md` §0 第 3 条，宁少不猜）。
#[derive(Default)]
pub struct FunctionState {
    /// 112 字节的不透明占位（`推断`，见类型文档）。
    _unresolved: [u64; 14],
}

impl FunctionState {
    /// 占位大小（= 112 B，`推断`）。测试断言它。
    pub const SIZE: usize = std::mem::size_of::<FunctionState>();
}

/// 原版 `GraphwarFunctionData`（**48 字节**，`观测`）= 一张 `HashMap<u32, FunctionState>`。
///
/// 证据：`ArcInner<Mutex<GraphwarFunctionData>>` 的分配常量 `0x48` = 72 = 16 + 8 + 48
/// （arm64 `0x419d2c` / x86 `0x5237aa` / DLL `0x18013f42e`），且 4 组字段偏移
/// （`+0x00` ctrl、`+0x08` bucket_mask、`+0x10` growth_left、`+0x18` items、`+0x20` hasher）
/// 与 `HashMap` 逐字对上。**没有**额外字段。
#[derive(Default)]
pub struct GraphwarFunctionData {
    /// `玩家/函数 id → 状态`（键宽 4 字节：bucket 里 key 在 `+0x00`，值 112 B）。
    pub functions: HashMap<u32, FunctionState>,
}

/// 原版 `GraphwarFunctionManager` —— **32 字节 / 4 个 8 字节字段**（`观测`）。
///
/// ⚠️ 这是**布局**，不是行为：原版那 7 个方法的语义**没有**在这里复原
/// （见本文件 §六）。字段顺序与偏移来自三份构建（arm64 / x86 Linux / Windows DLL）
/// 的一致指令序列；`size_of` 与 `offset_of` 由本文件末尾的单测断言。
pub struct GraphwarFunctionManager {
    /// `+0x00`：指向**自己**的弱句柄（`Rc::new_cyclic` 给的那个 `Weak` 的克隆）。
    pub self_weak: Weak<RefCell<GraphwarFunctionManager>>,
    /// `+0x08`：事件队列（`set_function` / `set_function_active_tick` 用它 `add_event`）。
    pub event_queue: Rc<RefCell<GraphwarEventQueue>>,
    /// `+0x10`：容器弱句柄（`set_function` 用它 `get_module_wk::<GraphwarTickHandler>()`）。
    pub container: Weak<RefCell<GraphwarModuleContainer>>,
    /// `+0x18`：`Arc<Mutex<…>>` —— 7 个方法都先拿这把锁。
    pub data: Arc<Mutex<GraphwarFunctionData>>,
}

/// 编译期硬闸：原版 `GraphwarFunctionManager` 是 **32 字节**。
const _: () = assert!(std::mem::size_of::<GraphwarFunctionManager>() == 32);
/// 编译期硬闸：内层 `GraphwarFunctionData` 是 **48 字节**（= 一张 `HashMap`）。
const _: () = assert!(std::mem::size_of::<GraphwarFunctionData>() == 48);
/// 编译期硬闸：`FunctionState` 的占位必须是 **112 字节**（`推断`，见类型文档）。
const _: () = assert!(std::mem::size_of::<FunctionState>() == 112);

// ---------------------------------------------------------------------------
// 八、活跃窗口 / 淡出（T88；纯函数，逐条翻自 x86 指令，见文件头 §八）
// ---------------------------------------------------------------------------

/// 每 tick 揭示几个点（`0x3be601`–`0x3be608` 的 `30·d`；同一常量在
/// `GraphwarPlane::draw` 的喂点循环 `0x4b67a0`–`0x4b6812` 里也是 30）。
pub const REVEAL_POINTS_PER_TICK: u32 = 30;

/// 揭示完之后还能在场的"尾巴"（`0x3be60b` 的 `add $0x708,%edi` = **1800** 点）。
///
/// `1800 / 30 = 60` tick —— 与 [`function_fade_on_tick`] 的分母 `60` 是同一个数
/// （淡出窗口 1 s @ 60 Hz）。
pub const ACTIVE_TAIL_POINTS: u64 = 1800;

/// 一条函数在本 tick **还在场**（`get_active_functions_on_tick` `0x456df0` 的谓词，
/// 真身在 `0x3be540`；见文件头 §八的指令表）。
///
/// ```text
/// tick > active_tick（**严格**） ∧ 30·(tick-active_tick) < points.len() + 1800
/// ```
///
/// 另两条原版条件（`value+0x70 == 1`、`value+0x50 != i64::MIN`）在本层用"表里有这一格"
/// 代替 —— 那两个字段的语义**未知**（见文件头 §六）。
pub fn is_active_on_tick(tick: u32, active_tick: u32, points_len: usize) -> bool {
    // 0x3be5fb：`jbe skip` —— tick <= active_tick ⇒ 不在场
    let Some(d) = tick.checked_sub(active_tick).filter(|&d| d > 0) else {
        return false;
    };
    // 0x3be601–0x3be611：30·d 与 len+1800 的**无符号**比较
    let revealed = (d as u64) * (REVEAL_POINTS_PER_TICK as u64);
    revealed < (points_len as u64).saturating_add(ACTIVE_TAIL_POINTS)
}

/// `get_active_functions_on_tick_no_fade`（**`0x456f80`**，真身在 `0x3bf620`）的谓词：
/// 同样的三条前置，末项是 `30·d < points.len()`（**没有** `+1800` 的尾巴）。
///
/// 用它问"这条函数**还在揭示**吗"（而不是"还在出生窗口里吗"）。
pub fn is_active_on_tick_no_fade(tick: u32, active_tick: u32, points_len: usize) -> bool {
    let Some(d) = tick.checked_sub(active_tick).filter(|&d| d > 0) else {
        return false;
    };
    let revealed = (d as u64) * (REVEAL_POINTS_PER_TICK as u64);
    revealed < points_len as u64
}

/// 还在场的**最大 `d`**（`d = tick − active_tick`），`d` 从这里+1 起就不在场。
///
/// 由 [`is_active_on_tick`] 直接解出来：`30·d < len+1800` ⇔ `d ≤ ⌊(len+1799)/30⌋`。
/// `len = 0`（还没注入点）⇒ **59**；`len = 780` ⇒ **85**。
pub fn active_window_ticks(points_len: usize) -> u64 {
    let n = (points_len as u64).saturating_add(ACTIVE_TAIL_POINTS - 1);
    n / REVEAL_POINTS_PER_TICK as u64
}

/// 原版 `GraphwarFunctionData::get_function_fade_on_tick`（x86 **`0x455a40`**）的算式。
///
/// 返回这一帧该给的 **alpha**：
///
/// * 未到场 / 还在揭示（`d <= len/30`） ⇒ **`1.0`**；
/// * 揭示完之后线性淡出：`alpha = max(0.0, 1.0 − (d − len/30)/60)`；
/// * 窗口外（`d ≥ len/30 + 60`）⇒ 被 `max(...,0.0)` 夹到 **`0.0`**
///   （此后再过一两 tick 就被 `remove_function` 删掉）。
///
/// 调用点：`GraphwarPlane::draw` 的喂点循环尾部
/// `0x4b69fe call [0x9bbd08]` → `0x455a40`，结果直接进 `FunctionDrawer::update_function_fade`
/// （`0x4b6a0e`）。**此前文档把这条记为"没有 x86 落点"**：那是只看 `call rel32` 的结论；
/// 槽 `0x9bbd08` 的 `.rela.dyn` addend 就是 `0x455a40`（`tools/addr_probe.py callers` 可复算）。
pub fn function_fade_on_tick(tick: u32, active_tick: u32, points_len: usize) -> f64 {
    // 0x455b0a：`sub %ecx,%ebx` / `jbe default`（d <= 0 ⇒ 1.0）
    let Some(d) = tick.checked_sub(active_tick).filter(|&d| d > 0) else {
        return 1.0;
    };
    // 0x455b0f–0x455b1e：`len`（截成 u32）无符号除 30（magic 0x88888889 >> 36）
    let reveal_ticks = (points_len as u32) / REVEAL_POINTS_PER_TICK;
    // 0x455b22：`sub %ecx,%ebx` / `jbe default`（还在揭示 ⇒ 1.0）
    let Some(d2) = d.checked_sub(reveal_ticks).filter(|&x| x > 0) else {
        return 1.0;
    };
    // 0x455b2e–0x455b42：`d2 / (-60.0) + 1.0` 再 `max(…, 0.0)`
    (1.0 - d2 as f64 / 60.0).max(0.0)
}

impl GraphwarFunctionManager {
    /// 原版尺寸（32 B），给 `root_wiring` 的台账用。
    pub const SIZE: usize = std::mem::size_of::<GraphwarFunctionManager>();

    /// 造一个**空壳**实例：字段类型与顺序照原版，内容全是"空"。
    ///
    /// 这不是原版的 `new`（原版 `new` 要一个真事件队列 + 真容器弱句柄，见本文件 §三）；
    /// 它只让 `root_wiring` 的容器里有**真布局的对象**可取回。
    pub fn skeleton() -> Self {
        Self {
            self_weak: Weak::new(),
            event_queue: Rc::new(RefCell::new(GraphwarEventQueue::new())),
            container: Weak::new(),
            data: Arc::new(Mutex::new(GraphwarFunctionData::default())),
        }
    }

    /// 原版 7 个方法都先拿的那把锁（`Arc<Mutex<GraphwarFunctionData>>`）的地址。
    /// 给验收做只读探针（原版没有这个方法，它只是"布局确实被那个字段占着"的把手）。
    pub fn data_mutex_ptr(&self) -> *const Mutex<GraphwarFunctionData> {
        Arc::as_ptr(&self.data)
    }
}

// ---------------------------------------------------------------------------
// 七、行为层 `FunctionTable`（T49：从 `_scaffold/root_wiring.rs` 搬回本模块）
// ---------------------------------------------------------------------------
//
// ⚠️ **类型名不是原版的**：原版把行为写成 `GraphwarFunctionManager` 自己的 6 个方法
// （`impl` 就在本文件）。我方历史上写成了 `_scaffold/root_wiring.rs` 里的一张"表"，
// 违反 `AGENTS.md` §4「文件名就是证据」——T36 审计（`docs/06` §86.4）把这列为第 ② 处。
//
// 搬过来时**没有原样搬迁**，按指令改了一处最要紧的：
//
// **三个 `set_*` 会发事件**（原先我方 `outbox` 一条都不写，档位因此停在 `layout`）。
// 这条不是我方选择，是原版行为，三处立即数都读到了（**逐位**）：
//
// ```text
// 0x455e30 set_function             0x456377 movabs rax,0x7fffffffffffffff ; 0x456381 add rax,0x24
//                                   ⇒ 0x8000000000000023 = tag 0x23 = 35 FunctionInfo
//                                   ; 载荷 [rsp+0x70..0x88] = (id, 文本 ptr/len, tick)
//                                   ; 0x45638f call [GOT 0x9bac80] -> 0x3d3530 add_event
// 0x456b80 set_function_active_tick  0x456cd3 lea rax,[r9+0x25]  ⇒ tag 0x24 = 36 FunctionActive
//                                   ; 载荷 [rsp+0x18]=id, [rsp+0x1c]=active_tick
// 0x456810 set_function_points       0x4569d3 lea rax,[r8+0x26]  ⇒ tag 0x25 = 37 FunctionPoints
//                                   ; 载荷 xmm = 两个 f64、[rsp+0x30]=id
// ```
//
// ⇒ 本层的 `outbox` 现在按这三条推进去（载荷里的字段本层还不建结构，只记 tag 对应的变体，
// 这是**我方形状**的差异，照实登记）。
//
// ⚠️ 档位字符串仍是 `layout`（`root_wiring::slot_status("GraphwarFunctionManager")`）：
// 它被 `client/tools/assemble_check.gd`（`:144` 与 `TIER_TALLY`）钉死，而本任务禁止改
// `client/**` ⇒ **两边要同时改才能升档**，登记在 `docs/06` §99 的遗留里。

/// 函数槽表：**我方形状**的行为层，接的是原版 `GraphwarFunctionManager` 的 6 个入口。
///
/// 字段与布局的关系：原版把状态全装在 `+0x18` 那个 `Arc<Mutex<GraphwarFunctionData>>` 里
/// （一张 `HashMap<u32, FunctionState>`，`FunctionState` 112 B 未逐字段解出，见 §六）。
/// 本层把 `FunctionState` 里**已知用途**的三小块拆成三张表：
/// 文本 token（`set_function` / `get_function` / `get_function_text`）、`active_tick`、
/// 终点 `(x, y)`——这是**我方形状**，不是原版字段的逐字节对应。
#[derive(Debug)]
pub struct FunctionTable {
    /// `soldier_id → token 序列`（原版一格一士兵，`docs/98` §3.1）。
    slots: HashMap<u32, Vec<Token>>,
    /// `set_function_active_tick` 写进去的那个 tick。
    active_tick: HashMap<u32, u32>,
    /// `set_function_points` 写进去的终点（`FunctionPoints.end_x/end_y`）。
    points: HashMap<u32, (f64, f64)>,
    /// `set_function_points` 那条点序列的**长度**（原版 `value+0x60` 的 `Vec` len）。
    ///
    /// 活跃窗口的两条判据都要它（`30·d < len + 1800` / `30·d < len`，见 §八），
    /// 而本层只存终点 ⇒ 由调用方（`GraphwarPlane` 注点时）把长度一并报进来。
    /// **T88 新增**：在这之前本层按 `len = 0` 算，窗口退化成恒 60 tick（那是错的）。
    points_len: HashMap<u32, usize>,
    /// 订阅名（**恒空 = 原版这一型也不订阅**，这是正面证据：构造体内无
    /// `EventHandler::subscribe`，见模板文档 §三）。
    pub subscribed: Vec<&'static str>,
    /// 发件箱：三个 `set_*` 按原版各发一条（tag 35 / 36 / 37，见 §七 的指令表）。
    pub outbox: Vec<Event>,
}

impl Default for FunctionTable {
    fn default() -> Self {
        Self {
            slots: HashMap::new(),
            active_tick: HashMap::new(),
            points: HashMap::new(),
            points_len: HashMap::new(),
            subscribed: Vec::new(),
            outbox: Vec::new(),
        }
    }
}

impl FunctionTable {
    /// 原版 `GraphwarFunctionManager::new`（**内联**，x86 `0x5236e0` / arm64 `0x419c80`）。
    ///
    /// 原版 `new` 要一条真事件队列 + 容器弱句柄（`Rc::new_cyclic`）；本层只造空表（差异照实登记）。
    pub fn new() -> Self {
        // 0x5236e0 — 原版构造体内联在 Rc::new_cyclic_in；本层给空表
        Self::default()
    }

    /// 原版 `set_function`（x86 **`0x455e30`**）：写一格 + **发 `FunctionInfo`(35)**。
    ///
    /// tag 证据：`0x456377 movabs 0x7fff…ffff` → `0x456381 add rax,0x24` ⇒ `0x8000…0023` = 35；
    /// 投递：`0x45638f call [GOT 0x9bac80]` → `0x3d3530 GraphwarEventQueue::add_event`。
    /// 载荷是 `(id, 文本, tick)`（`0x456346`–`0x456370` 的三个字），本层暂只记 tag。
    pub fn set_function(&mut self, soldier_id: u32, tokens: Vec<Token>) {
        // 0x455e30 — 写表后 add_event(tag 35 FunctionInfo)
        self.slots.insert(soldier_id, tokens);
        self.outbox.push(Event::FunctionInfo);
    }

    /// 原版 `get_function`（x86 **`0x455b60`**）：读一格（函数体首句读 `self+0x18` 那把锁）。
    pub fn get_function(&self, soldier_id: u32) -> Option<&[Token]> {
        // 0x455b60 — 读 self+0x18 的 HashMap 里的一格
        self.slots.get(&soldier_id).map(|v| v.as_slice())
    }

    /// 原版 `get_function_text`（x86 **`0x4565f0`**）：那一格的文本。
    ///
    /// 文本生成本身走 `math_function::to_text`（`docs/119` 1032/1032 对拍）。
    pub fn get_function_text(&self, soldier_id: u32) -> Option<String> {
        // 0x4565f0 — 取那一格再吐文本（Location :169:26）
        self.get_function(soldier_id).map(math_function::to_text)
    }

    /// 原版 `set_function_active_tick`（x86 **`0x456b80`**）：记在场起始 tick + **发 `FunctionActive`(36)**。
    ///
    /// tag 证据：`0x456cd3 lea rax,[r9+0x25]`（`r9 = 0x7fff…ffff`）⇒ `0x8000…0024` = 36；
    /// 载荷 `[rsp+0x18]=id`、`[rsp+0x1c]=active_tick`；投递 `0x456ce1`。
    pub fn set_function_active_tick(&mut self, soldier_id: u32, tick: u32) {
        // 0x456b80 — 写 active_tick 后 add_event(tag 36 FunctionActive)
        self.active_tick.insert(soldier_id, tick);
        self.outbox.push(Event::FunctionActive);
    }

    /// 读回 `active_tick`（原版没有单独的 getter，`get_active_functions_on_tick` 内联读它）。
    pub fn active_tick_of(&self, soldier_id: u32) -> Option<u32> {
        self.active_tick.get(&soldier_id).copied()
    }

    /// 原版 `set_function_points`（x86 **`0x456810`**）：记终点 + **发 `FunctionPoints`(37)**。
    ///
    /// tag 证据：`0x4569d3 lea rax,[r8+0x26]`（`r8 = 0x7fff…ffff`）⇒ `0x8000…0025` = 37；
    /// 载荷 `xmm` 两个 f64 + `[rsp+0x30]` 的 id；投递 `0x4569e1`。
    ///
    /// 第三个实参在原版是**整条点序列**（`Vec<(f64,f64)>`，16 B/点，见 `0x4569a2`–`0x4569ce`
    /// 里那段"取首点 + 取末点 x"的载荷拼装），本层只留终点 + 长度两样东西。
    pub fn set_function_points(&mut self, soldier_id: u32, x: f64, y: f64) {
        // 0x456810 — 写终点后 add_event(tag 37 FunctionPoints)
        self.points.insert(soldier_id, (x, y));
        self.outbox.push(Event::FunctionPoints);
    }

    /// 把点序列的**长度**一并报进来（原版那一步就是 `set_function_points` 的
    /// `Vec` len；本层拆成两个入口是因为我们只存终点）。
    ///
    /// 活跃窗口的两条判据都要它（见 §八），不报就按 `len = 0` 算（窗口恰好 60 tick）。
    pub fn set_function_points_len(&mut self, soldier_id: u32, len: usize) {
        self.points_len.insert(soldier_id, len);
    }

    /// 读回点序列长度（原版没有单独的 getter，两个 `get_active_*` 内联读 `value+0x60`）。
    pub fn points_len_of(&self, soldier_id: u32) -> usize {
        self.points_len.get(&soldier_id).copied().unwrap_or(0)
    }

    /// 读回终点（原版没有单独的 getter）。
    pub fn points_of(&self, soldier_id: u32) -> Option<(f64, f64)> {
        self.points.get(&soldier_id).copied()
    }

    /// 原版 `get_active_functions_on_tick`（x86 **`0x456df0`**）：本 tick 还在场的那些 id。
    ///
    /// 判据 = [`is_active_on_tick`]（`tick > active_tick` **严格**，且
    /// `30·(tick−active_tick) < points.len() + 1800`），逐条翻自 `0x3be5e6`–`0x3be613`。
    ///
    /// **T88 更正**：此前这里按 `points.len() = 0` 算（窗口恒 60 tick），因为本层当时
    /// 不知道点序列有多长；现在由 [`FunctionTable::set_function_points_len`] 报进来。
    pub fn get_active_functions_on_tick(&self, tick: u32) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .active_tick
            .iter()
            .filter(|&(&id, &active)| is_active_on_tick(tick, active, self.points_len_of(id)))
            .map(|(&id, _)| id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// 原版 `get_active_functions_on_tick_no_fade`（x86 **`0x456f80`**）：只问"还在揭示吗"
    /// （末项没有 `+1800` 的尾巴，见 [`is_active_on_tick_no_fade`]）。
    pub fn get_active_functions_on_tick_no_fade(&self, tick: u32) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .active_tick
            .iter()
            .filter(|&(&id, &active)| is_active_on_tick_no_fade(tick, active, self.points_len_of(id)))
            .map(|(&id, _)| id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// 原版 `get_function_fade_on_tick`（x86 **`0x455a40`**）：这一帧该给的 alpha。
    ///
    /// 表里没有这一格（或还没 `set_function_active_tick` / `set_function_points`）
    /// ⇒ **`1.0`**（原版那三条 `default` 臂都是 `movsd 0x19f570` = 1.0）。
    pub fn get_function_fade_on_tick(&self, soldier_id: u32, tick: u32) -> f64 {
        let Some(&active) = self.active_tick.get(&soldier_id) else {
            return 1.0;
        };
        function_fade_on_tick(tick, active, self.points_len_of(soldier_id))
    }

    /// 槽数（原版没有；验收用）。
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// 槽是否空（原版没有；验收用）。
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// 取走发件箱（三个 `set_*` 各推进去一条，见 §七）。
    pub fn take_outbox(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    /// 8 个字段偏移 + 总尺寸 == 原版（`观测`，三份构建逐偏移一致，见文件头表格）。
    #[test]
    fn layout_matches_the_original_32_bytes() {
        assert_eq!(size_of::<GraphwarFunctionManager>(), 32, "原版 32 字节");
        assert_eq!(offset_of!(GraphwarFunctionManager, self_weak), 0x00);
        assert_eq!(offset_of!(GraphwarFunctionManager, event_queue), 0x08);
        assert_eq!(offset_of!(GraphwarFunctionManager, container), 0x10);
        assert_eq!(offset_of!(GraphwarFunctionManager, data), 0x18);
        for f in [
            size_of::<Weak<RefCell<GraphwarFunctionManager>>>(),
            size_of::<Rc<RefCell<GraphwarEventQueue>>>(),
            size_of::<Weak<RefCell<GraphwarModuleContainer>>>(),
            size_of::<Arc<Mutex<GraphwarFunctionData>>>(),
        ] {
            assert_eq!(f, 8, "4 个字段都是单指针");
        }
    }

    /// 内层 48 字节 = 一张 `HashMap<u32, FunctionState>`（`观测`）。
    #[test]
    fn inner_data_is_one_hashmap_of_48_bytes() {
        assert_eq!(size_of::<GraphwarFunctionData>(), 48);
        assert_eq!(offset_of!(GraphwarFunctionData, functions), 0);
        assert_eq!(size_of::<HashMap<u32, FunctionState>>(), 48);
        // `推断`：112 B 是让 `(u32, FunctionState)` 的步长 = 0x78 的唯一 8 倍数。
        assert_eq!(FunctionState::SIZE, 112);
        // Rust 把 `u32` 排在偏移 0、值排在偏移 8 ⇒ `(u32, FunctionState)` = 8 + 112 = 120 = 0x78，
        // 与 §42.2 实测的 bucket 步长逐字节一致（这是"112"这个反推的落地自检）。
        assert_eq!(std::mem::size_of::<(u32, FunctionState)>(), 0x78);
    }

    /// 空壳实例的四个字段都是"空"值（不 panic、可取回）。
    #[test]
    fn skeleton_has_the_four_empty_fields() {
        let m = GraphwarFunctionManager::skeleton();
        assert!(m.self_weak.strong_count() == 0);
        assert!(m.container.strong_count() == 0);
        assert!(!m.data_mutex_ptr().is_null());
        assert_eq!(m.event_queue.borrow().len(), 0);
        assert_eq!(GraphwarFunctionManager::SIZE, 32);
    }

    // ── §八：活跃窗口 / 淡出（T88；期望值全部来自反汇编，不来自本实现）──────────────

    /// `get_active_functions_on_tick`（`0x3be5e6`–`0x3be613`）：
    /// `tick > active_tick` **严格**，且 `30·d < len + 1800`。
    #[test]
    fn active_window_is_strict_and_has_an_1800_point_tail() {
        // 严格大于：tick == active_tick 不在场（0x3be5fb 的 `jbe`）
        assert!(!is_active_on_tick(615, 615, 780));
        assert!(is_active_on_tick(616, 615, 780));

        // 0 点（还没注入）⇒ 尾巴 1800/30 = 60 tick：d=59 ⇒ 30·59 = 1770 < 1800 还在场
        assert!(is_active_on_tick(59, 0, 0), "d=59 ⇒ 1770 < 1800");
        assert!(!is_active_on_tick(60, 0, 0), "d=60 ⇒ 1800 不 < 1800");
        assert_eq!(active_window_ticks(0), 59);

        // 780 点 = 26 tick 揭示完 ⇒ d ≤ 85（30·85 = 2550 < 2580）
        assert_eq!(active_window_ticks(780), 85);
        assert!(is_active_on_tick(615 + 85, 615, 780), "d=85 ⇒ 2550 < 2580 ⇒ 还在场");
        assert!(!is_active_on_tick(615 + 86, 615, 780), "d=86 ⇒ 30·86 = 2580 = 780+1800 ⇒ 不在场");
    }

    /// `get_active_functions_on_tick_no_fade`（`0x3bf620`）：末项 `30·d < len`，**没有**尾巴。
    #[test]
    fn no_fade_variant_stops_as_soon_as_reveal_completes() {
        assert!(is_active_on_tick_no_fade(615 + 25, 615, 780), "d=25: 750 < 780 还在揭示");
        assert!(!is_active_on_tick_no_fade(615 + 26, 615, 780), "d=26: 780 不 < 780 ⇒ 揭完");
        assert!(!is_active_on_tick_no_fade(615, 615, 780), "tick == active_tick 不成立");
    }

    /// `get_function_fade_on_tick`（`0x455a40`）：未在场 / 还在揭示 ⇒ **1.0**；
    /// 之后 `max(0, 1 − (d − len/30)/60)`。
    #[test]
    fn fade_is_one_until_reveal_ends_then_linear_over_60_ticks() {
        // len = 780 ⇒ len/30 = 26（magic 除法，无符号）
        assert_eq!(function_fade_on_tick(615, 615, 780), 1.0, "d=0 ⇒ 1.0（0x455b0d 的 jbe）");
        assert_eq!(function_fade_on_tick(614, 615, 780), 1.0, "tick < active ⇒ 1.0（无符号下溢支）");
        assert_eq!(function_fade_on_tick(615 + 26, 615, 780), 1.0, "d == len/30 ⇒ 1.0（0x455b24）");
        // d = 27 ⇒ d2 = 1 ⇒ 1 − 1/60
        assert!((function_fade_on_tick(615 + 27, 615, 780) - (1.0 - 1.0 / 60.0)).abs() < 1e-12);
        // d = 86 ⇒ d2 = 60 ⇒ 恰好 0.0
        assert_eq!(function_fade_on_tick(615 + 86, 615, 780), 0.0);
        // 再往后被 max(…, 0.0) 夹住（0x455b42 的 `maxsd`）
        assert_eq!(function_fade_on_tick(615 + 200, 615, 780), 0.0);
        // len = 0 ⇒ len/30 = 0 ⇒ d=1 就开始淡出（第 1 tick 后 alpha = 1−1/60）
        assert!((function_fade_on_tick(1, 0, 0) - (1.0 - 1.0 / 60.0)).abs() < 1e-12);
        // 0x455a4c：HashMap 空/没这一格 ⇒ 1.0（本层由 `get_function_fade_on_tick` 兜）
        let f = FunctionTable::new();
        assert_eq!(f.get_function_fade_on_tick(7, 1000), 1.0, "没这一格 ⇒ 1.0");
    }

    /// `FunctionTable` 的三张表接线：`active_tick` + `points_len` 一起决定窗口，
    /// 淡出走同一份算式（这张表是**我方形状**，原版状态在 `FunctionState` 里）。
    #[test]
    fn table_window_uses_the_reported_point_count() {
        let mut f = FunctionTable::new();
        f.set_function_active_tick(230, 615);
        // 没报长度 ⇒ 尾巴 1800/30 = 60 tick（d ≤ 59）
        assert!(f.get_active_functions_on_tick(616).contains(&230));
        assert!(f.get_active_functions_on_tick(674).contains(&230), "d=59 ⇒ 1770 < 1800 ⇒ 还在场");
        assert!(f.get_active_functions_on_tick(675).is_empty(), "d=60 ⇒ 1800 不 < 1800 ⇒ 出窗口");

        // 报 780 点 ⇒ d ≤ 85
        f.set_function_points_len(230, 780);
        assert!(f.get_active_functions_on_tick(615 + 85).contains(&230));
        assert!(f.get_active_functions_on_tick(615 + 86).is_empty());
        // no_fade：揭完即退（26 tick）
        assert!(f.get_active_functions_on_tick_no_fade(615 + 25).contains(&230));
        assert!(f.get_active_functions_on_tick_no_fade(615 + 26).is_empty());
        // 淡出在表上也能读
        assert_eq!(f.get_function_fade_on_tick(230, 641), 1.0, "d=26 = len/30 ⇒ 1.0");
        assert!((f.get_function_fade_on_tick(230, 642) - (1.0 - 1.0 / 60.0)).abs() < 1e-12);
        f.take_outbox();
    }
}
