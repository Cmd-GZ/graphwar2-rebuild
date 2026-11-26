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
    pub fn set_function_points(&mut self, soldier_id: u32, x: f64, y: f64) {
        // 0x456810 — 写终点后 add_event(tag 37 FunctionPoints)
        self.points.insert(soldier_id, (x, y));
        self.outbox.push(Event::FunctionPoints);
    }

    /// 读回终点（原版没有单独的 getter）。
    pub fn points_of(&self, soldier_id: u32) -> Option<(f64, f64)> {
        self.points.get(&soldier_id).copied()
    }

    /// 原版 `get_active_functions_on_tick`（x86 **`0x456df0`**）：本 tick 还在场的那些 id。
    ///
    /// 在场判据（**观测**，`docs/16-client.md` §42.2 的三条指令级结论）：
    /// `tick > active_tick`（**严格**大于），且 `30*(tick-active_tick) < points.len() + 1800`。
    ///
    /// ⚠️ 本表不存整条曲线（曲线点由 `FunctionDrawer` 那边持有）⇒ **点的条数按 0 算**，
    /// 窗口退化成 `1800/30 = 60` tick。这是本层的**差异**，不是原版行为。
    pub fn get_active_functions_on_tick(&self, tick: u32) -> Vec<u32> {
        // 0x456df0 — 谓词：tick > active_tick（严格）且 30*(tick-active) < len+1800
        let mut ids: Vec<u32> = self
            .active_tick
            .iter()
            .filter(|&(_id, &active)| {
                if tick <= active {
                    return false;
                }
                let d = tick - active;
                30u64 * d as u64 <= 1800
            })
            .map(|(&id, _)| id)
            .collect();
        ids.sort_unstable();
        ids
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
}
