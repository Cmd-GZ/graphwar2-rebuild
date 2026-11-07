//! 原版 `src/graphwar_obstacle_manager.rs` —— **80 字节的 `GraphwarObstacleManager` 的字段布局**
//! （任务 22，第四节：`dispatch/22-manager-layout.md`）。
//!
//! 本文件只放**布局**（`size_of` / `offset_of` 会被断言）。障碍**光栅化/挖洞**的纯逻辑仍在
//! `src/plane.rs`（`docs/39`）；本文件**不**重复实现它。
//!
//! ⚠️ **挂载方式（临时）**：与 `graphwar_function_manager.rs` 相同——本轮 `src/lib.rs` 在禁区，
//! 由 `src/_scaffold/root_wiring.rs` 用 `#[path = "../graphwar_obstacle_manager.rs"]` 挂进 crate。
//! `lib.rs` 可改之后换成 `pub mod graphwar_obstacle_manager;`，本文件不用改。
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
//! | `draw_circle` 写入网格的**具体字节值** | **推断**（非 `0xff`） | 没逐条读 660 字节函数体里的 `strb` 常量 |
//! | 网格索引公式的**出处**（`1000/50`、`+500`、`+300`） | **观测**（常量与算式） | 常量含义（棋盘 50 单位 → 1000 像素）是**推断** |
//! | `Vec<[f64;3]>` 里三个 f64 的**语义** | **推断**（中心 x/y + 半径） | `is_destruction_queued` 只做逐分量 `ucomisd` 比较，没有语义；`draw_circle` 的入参是 `(x, y, r)` 但**顺序未证** |
//! | `queue_destroy_obstacle` 的**到期时间算法** | **未知** | 键是"tick"这一点由 `handle_tick_event` 用 tick 查表**观测**到；`now + ?` 的加法没解（§44.7 照旧） |
//! | 我们这份布局的**行为** | 未做 | 掩膜/挖洞纯逻辑仍在 `src/plane.rs`；本文件只有布局 |

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};

use crate::graphwar_event_queue::GraphwarEventQueue;
use crate::graphwar_module_container::GraphwarModuleContainer;

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
