//! 原版 `src/graphwar_obstacle_manager.rs` —— **80 字节的 `GraphwarObstacleManager` 的字段布局**
//! （任务 22，第四节：`dispatch/22-manager-layout.md`）。
//!
//! 本文件只放**布局**（`size_of` / `offset_of` 会被断言）。障碍**光栅化/挖洞**的纯逻辑仍在
//! `src/plane.rs`（`docs/39`）；本文件**不**重复实现它。
//!
//! **挂载方式（已转正，2026-11-26 更正本文）**：与 `graphwar_function_manager.rs` 相同 ——
//! 早先靠 `src/_scaffold/root_wiring.rs` 的 `#[path = "../graphwar_obstacle_manager.rs"]`
//! 临时挂进 crate，**现在 `src/lib.rs` 已直接 `pub mod graphwar_obstacle_manager;`**，本段旧描述已过期。
//!
//! # 一、这个模块里有什么（`观测`，arm64 2.0.60 `.symtab` 原文）
//!
//! | 函数 | arm64 | x86 2.0.60 | `panic::Location` |
//! |---|---|---|---|
//! | `GraphwarObstacleManager::new` | `0x426300` (284) | `0x3f2360` | `src/graphwar_obstacle_manager.rs:208:40` |
//! | `GraphwarObstacleManager::is_obstacle` | `0x425474` (528) | `0x3f15b0` | `:89:28`（与 `draw_circle` 共用锚） |
//! | `GraphwarObstacleManager::add_obstacle` | `0x425684` (492) | `0x3f17a0` | `:258:26` |
//! | `GraphwarObstacleManager::clear_obstacles` | `0x425870` (452) | `0x3f19a0` | `:252:41` |
//! | `GraphwarObstacleManager::destroy_obstacle` | `0x425a34` (492) | `0x3f1b30` | `:263:26` |
//! | `GraphwarObstacleManager::queue_destroy_obstacle` | `0x426094` (620) | `0x3f20f0` | `:269:26` |
//! | `GraphwarObstacleManager::handle_tick_event` | `0x425c20` (272) | `0x3f1d30` | `:295:18` |
//! | `GraphwarObstacleManager::generate_obstacles` | `0x425d30` (868) | — | 无锚 |
//! | `GraphwarObstacleData::draw_circle` | `0x4251e0` (660) | `0x3f11e0` | `:89:28` |
//! | `GraphwarObstacleData::add_obstacle` | — | — | DLL `0x180103750` |
//! | `GraphwarObstacleData::get_send_data` | — | — | DLL `0x180103790` |
//! | `GraphwarObstacleData::destroy_obstacle` | — | — | DLL `0x180103820` |
//! | `GraphwarObstacleData::new` | — | — | DLL `0x180103860` |
//! | `GraphwarObstacleManager::is_destruction_queued` | — | — | DLL `0x180104450` |
//!
//! `GraphwarObstacleData` 的 4 个方法在 arm64 / x86 Linux 那份里**全被内联**（没有独立符号），
//! 只有 Windows 2.0.60 DLL 还留着名字——这条也是本节敢给内层结构定型的直接原因。
//!
//! # 二、80 字节 = **5 个字段**（`观测`，三份构建逐偏移一致）
//!
//! | 偏移 | 宽 | 类型 | 证据（指令地址） | 档位 |
//! |---|---|---|---|---|
//! | `+0x00` | 8 | `Weak<RefCell<GraphwarObstacleManager>>`（弱自句柄） | `new` 里做 `Weak::clone`：`cmn x21,#-1`（`Weak::new` 哨兵）→ `ldr x8,[x21,#0x8]`/`adds`/`str`（弱计数 +1）→ 塞进订阅闭包：arm64 `0x42633c`–`0x42635c`；x86 `0x3f2399`–`0x3f23a3`（`cmp r15,-1`、`inc [r15+8]`）；DLL `0x1801047f8`–`0x180104806`。构造点把 `RcBox` 指针写进该槽：arm64 `0x419b70` `stp xzr, x19, [x19, #0x10]`、DLL `0x18013f634` `mov %rsi,0x18(%rsi)`。析构交给 `drop_in_place<Weak<RefCell<GraphwarObstacleManager>>>`（`0x416ff8` / DLL `0x18013c480`） | **观测** |
//! | `+0x08` | 8 | `Rc<RefCell<GraphwarEventQueue>>` | 4 个入口都读它、`+0x10` 是 `RefCell` 借用位、`+0x18` 当 `&mut GraphwarEventQueue` 传给 `add_event`：arm64 `add_obstacle` `0x42572c`、`clear_obstacles` `0x4258d4`、`destroy_obstacle` `0x425adc`、`queue_destroy_obstacle` `0x42620c`；x86 `0x3f184b` / `0x3f1a05` / `0x3f1bde` / `0x3f2245`；DLL `0x180103b6f` 等。构造点第二实参（arm64 `x20`、DLL `%rdi`） | **观测** |
//! | `+0x10` | 8 | `Weak<RefCell<GraphwarModuleContainer>>` | `new` 的第三个实参（arm64 `x21`→`RcBox+0x28`；x86 `r15`→`[rbx+0x28]`；DLL `%rbx`）。析构 `drop_in_place<Weak<RefCell<GraphwarModuleContainer>>>`（arm64 `0x419c2c`、DLL `0x18013f6e7`） | **观测** |
//! | `+0x18` | 8 | `Arc<Mutex<GraphwarObstacleData>>` | **每个方法的第一句**都读这个槽：arm64 `0x425488` / `0x4256a8` / `0x425888` / `0x425a58`；x86 `0x3f15bc` / `0x3f17b4` / `0x3f19b1` / `0x3f1b44`。锁字 `+0x10`、中毒位 `+0x14`、数据 `+0x18`（数据里 `ptr@+0x20`、`len@+0x28`）。析构 `Arc::drop_slow` + 存在 `drop_in_place<ArcInner<Mutex<GraphwarObstacleData>>>`（DLL `0x18013c810`） | **观测** |
//! | `+0x20` | 48 | `HashMap<u32, Vec<[f64; 3]>>`（待销毁队列） | `RawTable`：`ctrl@+0x20`、`bucket_mask@+0x28`、`growth_left@+0x30`、`items@+0x38`，`RandomState@+0x40`（16 B）。决定性指令：arm64 `0x425c50` `add x0, x0, #0x40` → `BuildHasher::hash_one`（`0x3ec7f0`）、`0x425c68` `add x0, x19, #0x20` → `RawTable::remove_entry`（`0x3c6620`）；x86 `0x3f2110` `lea r12,[rdi+0x40]`、`0x3f2128` `mov rcx,[r14+0x20]`；DLL `0x18010455f` / `0x180104577`。键 = 4 字节（`movl %esi, -0x20(%rcx,%r8)`，`cmpl %esi,-0x20(%r12)`）；值 = `Vec<[f64; 3]>`（插入时 `RawVec::grow_one`、元素步长 `0x18`、析构 `dealloc(ptr, 24*cap)`）；构造时 `RandomState` 来自线程局部 `KEYS`（DLL `0x18013f60f`–`0x18013f62c`） | **观测** |
//!
//! 尺寸的独立证据：`RcBox<RefCell<GraphwarObstacleManager>>` 的分配是
//! **`0x68` = 104 = 24 + 80**（arm64 `0x419aa4`、DLL `0x18013f562`，错误路径的
//! `handle_alloc_error(size=0x68)` 也对得上）。`+0x20 + 48 = 0x50 = 80` 自洽。
//!
//! `new` 的签名（`观测`）：`new(&mut GraphwarEventHandler, Rc<RefCell<GraphwarEventQueue>>,
//! Weak<RefCell<GraphwarModuleContainer>>) -> Rc<RefCell<Self>>`——第一个实参在构造完之后
//! 被拿去 `GraphwarEventHandler::subscribe_to_event("tick_event", len 0xa, 闭包)`
//! （arm64 `0x4263a0`、x86 `0x3f23f0` 经 GOT `0x9bac88`），这是这个管理器**唯一**一条订阅。
//!
//! # 三、内层 `GraphwarObstacleData` = **24 字节**：一张 1000×600 的字节网格（`观测`）
//!
//! `ArcInner<Mutex<GraphwarObstacleData>>` 的分配是 **`0x30` = 48 = 16 + 8 + 24**
//! （arm64 `0x419b00`、DLL `0x18013f5c6`）⇒ 内层 24 字节，正好是一个 `Vec<u8>` 的尺寸
//! （本工具链 `Vec` 的内存序是 `cap@0 / ptr@8 / len@0x10`，见 `Vec` 布局自检）：
//!
//! | `Data+` | 内容 | 证据 |
//! |---|---|---|
//! | `+0x00` | `cap` = **600000** | `movq $0x927c0, (%rsi)`（DLL `0x180103895`）；arm64 `0x419b24` |
//! | `+0x08` | `ptr`（600000 字节缓冲） | `%rdi`（`__rust_alloc(0x927c0, 1)` 的结果） |
//! | `+0x10` | `len` = **600000** | `movq $0x927c0, 0x10(%rsi)` |
//!
//! 缓冲用 `memset(0xff, 600000)` 初始化（DLL `0x18010388e`、arm64 `0x419af8`）⇒ **`0xff` = 空**。
//! 网格尺寸与索引式（`观测`，arm64 `is_obstacle` `0x4254c4`–`0x425534`，全是 `双精度` 常量）：
//!
//! ```text
//! x = trunc(1000.0 * p.x / 50.0 + 500.0)   ; 合法区间 [0, 999]   （cmp #0x3e7）
//! y = trunc(1000.0 * p.y / 50.0 + 300.0)   ; 合法区间 [0, 599]   （cmp #0x257）
//! index = y * 1000 + x                     ; mul w9, w9, #1000
//! 有障碍 ⇔ grid[index] != 0xff
//! ```
//!
//! ⇒ 600000 = 1000 × 600，与 `GraphwarObstacleData` 的 24 字节完全对得上。
//! **未解**：`draw_circle` 往网格里写的是什么值（`推断`：非 `0xff`，具体值没逐条读）。
//!
//! # 四、与既有代码/文档的关系（就地更正）
//!
//! * `docs/16-client.md` §44.0 / §44.7 写「`GraphwarObstacleManager` 的字段**一个都没解**」
//!   ⇒ **本节起过期**：80 字节 5 个字段全部落到指令，见 §二。
//! * 排队销毁的键是 **4 字节**、值是 `Vec<[f64; 3]>`（`观测`）。`docs/44` 的"延迟 165 tick"
//!   仍是我方实测值，与本节无关（那条讲的是**时间**，不是**布局**）。
//!
//! # 五、仍是未知 / 推断（诚实清单）
//!
//! | 项 | 档位 | 缺什么 |
//! |---|---|---|
//! | `draw_circle` 写入网格的**具体字节值** | ~~推断~~ → **逐位**（T49 更正） | **不是常量，是第 5 个实参**：`0x3f143a mov byte ptr [r8+r12], dil`（`dil` = 入口 `edx` 存到 `[rsp+0x10]` 的那个字节，`0x3f1411` 取回）。两个调用点各自传常量：`add_obstacle` `0x3f1815 xor edx,edx` = **0x00**；`destroy_obstacle` `0x3f1ba5 mov edx,0xff` = **0xff** |
//! | 网格索引公式的**出处**（`1000/50`、`+500`、`+300`） | **观测**（常量与算式） | 常量含义（棋盘 50 单位 → 1000 像素）是**推断** |
//! | `Vec<[f64;3]>` 里三个 f64 的**语义** | ~~推断~~ → **逐位**（T49 更正） | 顺序 = **(x, y, radius)**：`0x3f2275 movups [rax+rcx*8], xmm1`（xmm1 = `[r15]` 两个 f64 = 位置对）、`0x3f227f movsd [rax+rcx*8+0x10], xmm0`（xmm0 = 入口的第一实参 = 半径） |
//! | `queue_destroy_obstacle` 的**到期时间算法** | ~~未知~~ → **逐位**（T49 更正） | **函数体内没有那个加法**：`0x3f20fe mov ebx,edx` 直接把**第三实参 tick** 当 HashMap 键（`0x3f210c` 起用它做 `hash_one`）。"延迟多久"由**调用方**决定，不在本模块 |
//! | **`handle_tick_event` 的错误路径** | **逐位** | 判别值必须 = `0x8000000000000000`（**tag 0 = `TickEvent`**），否则 `0x3f1d40 cmp rax,0x8000000000000000; jno → 0x3f1e08` 走 **`explicit panic`**（串 `0x1bfcbf`，长度 `0xe`）。我方保留 `Result` 返回形状（调用方不动），`Err` 分支注明原版是 panic |
//! | 我们这份布局的**行为** | **已落码**（T49） | 见 §六：`ObstacleTable` 的 6 个方法（原位在 `_scaffold/root_wiring.rs`） |
//!
//! # 六、行为层 `ObstacleTable`（T49：从 `_scaffold/root_wiring.rs` 搬回本模块）
//!
//! ⚠️ **这个类型名不是原版的**：原版把行为写成 `GraphwarObstacleManager` 自己的方法（`impl` 就在
//! 本文件）。我方历史上把它写成了 `_scaffold/root_wiring.rs` 里的一张"表"，违反
//! `AGENTS.md` §4「文件名就是证据」——T36 审计（`docs/06` §86.4）把它列为第 ① 处。
//! 本轮（T49）把**方法体**搬回本模块，`_scaffold/root_wiring.rs` 只留
//! `pub use crate::graphwar_obstacle_manager::ObstacleTable;`，**调用方零改动**。
//!
//! 搬过来时按指令改了三处**我方自造**的语义（不是原样搬迁）：
//!
//! 1. `queue_destroy_obstacle` 的键改成**实参 tick**（原先我方写 `now_tick + DESTROY_DELAY_TICKS`）；
//! 2. `handle_tick_event` 改成**按键等值删整条**（原先我方扫"所有 `due <= now`"）；
//! 3. `pending` 从 `Vec<(i64, x, y, r)>` 换成原版 `+0x20` 那个 `HashMap<u32, Vec<[f64; 3]>>`。
//!
//! 另外删掉了我方发明的第二个状态 `obstacles: Vec<(f64, f64, f64)>`（原版 80 字节里**没有**
//! 这张表，见 §二），掩膜改为持有原版内层类型 [`GraphwarObstacleData`]（就是 `+0x18` 里那把
//! `Arc<Mutex<_>>` 装的东西）。


use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};

use crate::events::Event;
use crate::graphwar_event_queue::GraphwarEventQueue;
use crate::graphwar_life_manager::TICK_EVENT_TAG;
use crate::graphwar_module_container::GraphwarModuleContainer;
use crate::plane::{empty_mask, is_occupied, rasterize};

/// 原版 `GraphwarObstacleData`（**24 字节**，`观测`）：一张 1000×600 的字节网格。
///
/// 内存序与 `Vec<u8>` 完全相同（`cap@0 / ptr@8 / len@0x10`），`cap == len == 600000`。
/// 原版是"只含一个 `Vec<u8>` 的 newtype"还是裸 `Vec<u8>`，**在布局层不可区分**
/// （`推断`：newtype；依据是它有自己的类型名与 4 个方法）。
pub struct GraphwarObstacleData {
    /// 网格：`index = y * 1000 + x`，`0xff` = 空。
    pub cells: Vec<u8>,
}

/// 网格宽（像素列）：`x = trunc(1000 * p.x / 50 + 500)` 的上界 + 1（`观测`）。
pub const GRID_COLS: usize = 1000;
/// 网格高（像素行）：`y = trunc(1000 * p.y / 50 + 300)` 的上界 + 1（`观测`）。
pub const GRID_ROWS: usize = 600;
/// 空标记：原版 `memset(0xff, 600000)`（`观测`）。
pub const CELL_EMPTY: u8 = 0xff;

impl GraphwarObstacleData {
    /// 原版 `GraphwarObstacleData::new`（DLL `0x180103860`）：分配 600000 字节、填 `0xff`。
    pub fn new() -> Self {
        Self { cells: vec![CELL_EMPTY; GRID_COLS * GRID_ROWS] }
    }

    /// 原版 `is_obstacle` 的索引式（`观测`，arm64 `0x4254c4`–`0x425534`）。
    pub fn index_of(x: f64, y: f64) -> Option<usize> {
        let ix = (1000.0 * x / 50.0 + 500.0).trunc();
        let iy = (1000.0 * y / 50.0 + 300.0).trunc();
        if !(0.0..GRID_COLS as f64).contains(&ix) || !(0.0..GRID_ROWS as f64).contains(&iy) {
            return None;
        }
        Some(iy as usize * GRID_COLS + ix as usize)
    }
}

impl Default for GraphwarObstacleData {
    fn default() -> Self {
        Self::new()
    }
}

/// 原版 `GraphwarObstacleManager` —— **80 字节 / 5 个字段**（`观测`）。
///
/// ⚠️ 这是**布局**，不是行为：掩膜光栅化与挖洞的纯逻辑在 `src/plane.rs`，
/// 排队销毁的**时间**语义仍是 `docs/44` 的实测值。字段偏移由本文件末尾单测断言。
pub struct GraphwarObstacleManager {
    /// `+0x00`：指向自己的弱句柄（`new` 里克隆一份塞进 `tick_event` 的订阅闭包）。
    pub self_weak: Weak<RefCell<GraphwarObstacleManager>>,
    /// `+0x08`：事件队列（`add_obstacle` / `clear_obstacles` / `destroy_obstacle` /
    /// `queue_destroy_obstacle` 四个入口都往里 `add_event`）。
    pub event_queue: Rc<RefCell<GraphwarEventQueue>>,
    /// `+0x10`：容器弱句柄。
    pub container: Weak<RefCell<GraphwarModuleContainer>>,
    /// `+0x18`：网格（`Arc<Mutex<…>>`，每个方法都先拿这把锁）。
    pub data: Arc<Mutex<GraphwarObstacleData>>,
    /// `+0x20`（48 B）：`tick → 该 tick 要销毁的圆`（`(x, y, r)` 三元组）。
    ///
    /// 键 4 字节、值 `Vec<[f64; 3]>`（元素 24 B）——`观测`：插入走 `RawVec::grow_one`、
    /// 析构按 `dealloc(ptr, 24 * cap)` 释放、`handle_tick_event` 用 tick 直接 `remove_entry`。
    pub pending_destroy: HashMap<u32, Vec<[f64; 3]>>,
}

/// 编译期硬闸：原版 `GraphwarObstacleManager` 是 **80 字节**。
const _: () = assert!(std::mem::size_of::<GraphwarObstacleManager>() == 80);
/// 编译期硬闸：内层 `GraphwarObstacleData` 是 **24 字节**（一个 `Vec<u8>` 的尺寸）。
const _: () = assert!(std::mem::size_of::<GraphwarObstacleData>() == 24);

impl GraphwarObstacleManager {
    /// 原版尺寸（80 B），给 `root_wiring` 的台账用。
    pub const SIZE: usize = std::mem::size_of::<GraphwarObstacleManager>();

    /// 造一个**空壳**实例：字段类型/顺序照原版，网格是 600000 字节的 `0xff`。
    ///
    /// 这不是原版的 `new`（原版 `new` 还要 `&mut GraphwarEventHandler` 去订阅 `tick_event`）。
    pub fn skeleton() -> Self {
        Self {
            self_weak: Weak::new(),
            event_queue: Rc::new(RefCell::new(GraphwarEventQueue::new())),
            container: Weak::new(),
            data: Arc::new(Mutex::new(GraphwarObstacleData::new())),
            pending_destroy: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 行为层（原版 `GraphwarObstacleManager` 的 6 个方法；T49 从 `_scaffold/root_wiring.rs` 搬回）
// ---------------------------------------------------------------------------

/// 障碍管理的**行为**：掩膜网格 + 排队销毁表 + 发件箱。
///
/// 见 §六：类型名是我方的（原版没有独立类型，行为是 `GraphwarObstacleManager` 自己的方法），
/// 但**每个方法体都在下面指到了原版 vaddr**，且都是按指令校正过的语义。
///
/// 与布局类型 [`GraphwarObstacleManager`] 的分工：那个是**字段布局**（`size_of == 80` 硬闸），
/// 这个是**行为**；两者字段并不一一对应（布局里的 `event_queue`/`container`/`self_weak` 是
/// 接线用的对象图，本层没有对象图 ⇒ 用 `subscribed`/`outbox` 的观测形状代替，这是
/// **我方接线**，不是复原物）。
pub struct ObstacleTable {
    /// 原版 `+0x18` 那把 `Arc<Mutex<…>>` 的内层数据（1000×600 网格，`0xff` = 空）。
    ///
    /// 原版是 `Rc` 里共享的（`new` 造、5 个方法各自 `lock`）；本层没有共享者 ⇒ 直接持有值。
    data: GraphwarObstacleData,
    /// 原版 `+0x20`（48 B）：`tick → 该 tick 要销毁的圆`。
    ///
    /// 键 = **调用方传进来的 tick**（`0x3f20fe`）；值元素 = `[x, y, radius]`（`0x3f2275`/`0x3f227f`）。
    pending: HashMap<u32, Vec<[f64; 3]>>,
    /// 订阅名（原版 `new` 末尾那一条 `tick_event`，`0x3f23f0` 经 GOT `0x9bac88`）。
    pub subscribed: Vec<&'static str>,
    /// 发件箱（原版是共享 `GraphwarEventQueue` 上的 `add_event`；本层先存后搬）。
    pub outbox: Vec<Event>,
}

impl Default for ObstacleTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ObstacleTable {
    /// 原版 `GraphwarObstacleManager::new`（x86 **`0x3f2360`**）：空网格 + 空排队表 + 1 条订阅。
    pub fn new() -> Self {
        // 0x3f2360 — 网格 `memset(0xff, 600000)`、`pending` 空、末尾 subscribe('tick_event')
        Self {
            data: GraphwarObstacleData::new(),
            pending: HashMap::new(),
            subscribed: vec!["tick_event"],
            outbox: Vec::new(),
        }
    }

    /// 原版 `add_obstacle`（x86 **`0x3f17a0`**）：往网格里画实心圆 + 发 `AddObstacle`(40)。
    ///
    /// 实参：`(radius: f64, pos: &(f64, f64))`（`xmm0` / `rsi`，见 `0x3f17ae`–`0x3f1803`）；
    /// fill 常量 = **`0x00`**（`0x3f1815 xor edx,edx`）；载荷 = `pos` 对 + `radius`
    /// （`0x3f186b [rsp+0x28]`、`0x3f1870 [rsp+0x38]`）；tag = 40（`0x3f1876 add r13,0x29`）。
    pub fn add_obstacle(&mut self, x: f64, y: f64, radius: f64) {
        // 0x3f17a0 — draw_circle(fill=0x00) 然后 add_event(tag 40)
        rasterize(&mut self.data.cells, x, y, radius, 0x00);
        self.outbox.push(Event::AddObstacle);
    }

    /// 原版 `is_obstacle`（x86 **`0x3f15b0`**）：单点是否被挡。
    ///
    /// 语义（**逐位**）：先算 `ix = sat_i32(1000*x/50 + 500)`、`iy = sat_i32(1000*y/50 + 300)`；
    /// **越界直接算命中**（`0x3f1669 mov al,1` 在两条 `cmp …; ja` **之前**）；界内
    /// `grid[iy*1000+ix] != 0xff`（`0x3f1697 cmp byte ptr [rax+rdi],0xff` / `setne`）。
    ///
    /// 这正是 `plane::is_occupied` 的语义（那个函数是同一地址的逐行对照版），所以直接复用。
    pub fn is_obstacle(&self, x: f64, y: f64) -> bool {
        // 0x3f15b0 — 越界=真；界内 grid[idx] != 0xff
        is_occupied(&self.data.cells, x, y)
    }

    /// 原版 `queue_destroy_obstacle`（x86 **`0x3f20f0`**）：把"到期销毁"登记进 `pending[tick]`。
    ///
    /// 实参（**逐位**）：`(pos: &(f64, f64) = rsi, radius: f64 = xmm0, tick: u32 = edx)`；
    /// `0x3f20fe mov ebx,edx` ⇒ **键就是这个 tick，函数体内没有 `now + delay`**；
    /// 值插入 `(x, y, radius)`；随后发 `QueueDestroyObstacle`(42)（`0x3f22b2 movabs 0x800000000000002a`，
    /// 载荷 = `pos` + `radius` + `tick`：`0x3f22a3`/`0x3f22a8`/`0x3f22ae`）。
    ///
    /// ⚠️ **就地更正**：原先本方法（在 `_scaffold`）写的是 `now_tick + DESTROY_DELAY_TICKS`（165），
    /// 那是我方从实测倒推出来的**推断**，指令里没有这个加法。延迟多少由**调用方**决定。
    pub fn queue_destroy_obstacle(&mut self, tick: u32, x: f64, y: f64, radius: f64) {
        // 0x3f20f0 — pending[tick].push([x,y,radius])，然后 add_event(tag 42)
        self.pending.entry(tick).or_default().push([x, y, radius]);
        self.outbox.push(Event::QueueDestroyObstacle);
    }

    /// 原版 `destroy_obstacle`（x86 **`0x3f1b30`**）：立刻挖洞 + 发 `DestroyObstacle`(41)。
    ///
    /// fill 常量 = **`0xff`**（`0x3f1ba5 mov edx,0xff` ⇒ 刷回"空"）；tag = 41（`0x3f1c09 add r13,0x2a`）。
    pub fn destroy_obstacle(&mut self, x: f64, y: f64, radius: f64) {
        // 0x3f1b30 — draw_circle(fill=0xff) 然后 add_event(tag 41)
        rasterize(&mut self.data.cells, x, y, radius, 0xFF);
        self.outbox.push(Event::DestroyObstacle);
    }

    /// 原版 `clear_obstacles`（x86 **`0x3f19a0`**）：整张网格刷回 `0xff` + 发 `ClearObstacles`。
    ///
    /// `0x3f19fa mov esi,0xff` → `memset(ptr, 0xff, len)`（GOT `0x9bad08`）；
    /// tag = 39（`docs/02-module-map.md` 的事件表，`Event::ClearObstacles`）。
    pub fn clear_obstacles(&mut self) {
        // 0x3f19a0 — memset(0xff) 然后 add_event(tag 39)
        self.data.cells = empty_mask();
        self.outbox.push(Event::ClearObstacles);
    }

    /// 原版 `handle_tick_event`（x86 **`0x3f1d30`**）：收到 `TickEvent` ⇒ 销毁**这个 tick** 登记的那些圆。
    ///
    /// **逐位**：
    /// * `0x3f1d40 cmp rax,0x8000000000000000; jno → 0x3f1e08` ⇒ 判别值必须是 **0**，
    ///   否则走 `explicit panic`（原版**没有**"忽略坏事件"的分支）；
    /// * `0x3f1d56 mov eax,[rsi+8]` ⇒ 键 = 载荷里的 tick；
    /// * `0x3f1d80 call 0x4863a0` = `remove_entry`（**按键等值删整条**，不是扫 `due <= now`）；
    /// * 然后按 `0x18` 步长遍历该 `Vec<[f64;3]>`，每个元素调 `destroy_obstacle`（`0x3f1dd0`）。
    ///
    /// 返回这次销毁掉的 `(x, y)`（原版返回 `()`；这里保留 `Result` 形状是为了不改调用方，
    /// `Err` = 原版 panic 的那条路径）。
    pub fn handle_tick_event(&mut self, tag: u16, tick: u32) -> Result<Vec<(f64, f64)>, ()> {
        // 0x3f1d30 — 判别值必须 = TickEvent(0)（原版否则 panic）；remove_entry(tick) 后逐个 destroy
        if tag != TICK_EVENT_TAG {
            return Err(());
        }
        let Some(entry) = self.pending.remove(&tick) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::with_capacity(entry.len());
        for [x, y, r] in entry {
            self.destroy_obstacle(x, y, r);
            out.push((x, y));
        }
        Ok(out)
    }

    /// 掩膜（只读；验收/单测用）。原版没有这个 getter，它读的是 `+0x18` 那个 `Arc` 里的 `Vec`。
    pub fn mask(&self) -> &[u8] {
        &self.data.cells
    }

    /// 还在排队的销毁条数（`pending` 里所有 `Vec` 的长度和）。
    ///
    /// ⚠️ 原版没有这个方法；它存在的理由是 `assemble_check.gd` 的既有断言读它。
    pub fn pending_len(&self) -> usize {
        self.pending.values().map(|v| v.len()).sum()
    }

    /// 排队表里**有哪些 tick**（升序）。原版没有 getter；验收用。
    pub fn pending_ticks(&self) -> Vec<u32> {
        let mut ticks: Vec<u32> = self.pending.keys().copied().collect();
        ticks.sort_unstable();
        ticks
    }

    /// 取走发件箱。
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

    /// 5 个字段偏移 + 总尺寸 == 原版（`观测`，三份构建逐偏移一致，见文件头表格）。
    #[test]
    fn layout_matches_the_original_80_bytes() {
        assert_eq!(size_of::<GraphwarObstacleManager>(), 80, "原版 80 字节");
        assert_eq!(offset_of!(GraphwarObstacleManager, self_weak), 0x00);
        assert_eq!(offset_of!(GraphwarObstacleManager, event_queue), 0x08);
        assert_eq!(offset_of!(GraphwarObstacleManager, container), 0x10);
        assert_eq!(offset_of!(GraphwarObstacleManager, data), 0x18);
        assert_eq!(offset_of!(GraphwarObstacleManager, pending_destroy), 0x20);
        // +0x20 + 48 = 0x50 = 80
        assert_eq!(size_of::<HashMap<u32, Vec<[f64; 3]>>>(), 48);
        assert_eq!(0x20 + size_of::<HashMap<u32, Vec<[f64; 3]>>>(), 80);
    }

    /// 内层 24 字节 = 一个 `Vec<u8>`（`观测`）；网格 1000×600 = 600000。
    #[test]
    fn inner_data_is_a_600000_byte_grid() {
        assert_eq!(size_of::<GraphwarObstacleData>(), 24);
        assert_eq!(offset_of!(GraphwarObstacleData, cells), 0);
        assert_eq!(size_of::<Vec<u8>>(), 24);
        assert_eq!(GRID_COLS * GRID_ROWS, 600_000);
        let d = GraphwarObstacleData::new();
        assert_eq!(d.cells.len(), 600_000);
        assert!(d.cells.iter().all(|b| *b == CELL_EMPTY));
        // 索引式：(0,0) 落在网格中心 ⇒ 500 + 1000*300
        assert_eq!(GraphwarObstacleData::index_of(0.0, 0.0), Some(300 * 1000 + 500));
        assert_eq!(GraphwarObstacleData::index_of(-25.0, -15.0), Some(0));
        assert_eq!(GraphwarObstacleData::index_of(25.0, 15.0), None); // x=1000 越界
    }

    /// 空壳实例的 5 个字段都可取回。
    #[test]
    fn skeleton_has_the_five_fields() {
        let m = GraphwarObstacleManager::skeleton();
        assert_eq!(m.self_weak.strong_count(), 0);
        assert_eq!(m.container.strong_count(), 0);
        assert_eq!(m.event_queue.borrow().len(), 0);
        assert_eq!(m.data.lock().unwrap().cells.len(), 600_000);
        assert!(m.pending_destroy.is_empty());
        assert_eq!(GraphwarObstacleManager::SIZE, 80);
    }
}
