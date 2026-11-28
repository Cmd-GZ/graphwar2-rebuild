//! `graphwar_ban_list`：封禁表 `GraphwarBanList`（原版模块，**96 字节、2 个方法**）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律 **arm64 vaddr**（未 strip 的 Android 2.0.60 `.symtab`，符号名就是源码名）。
//!
//! ## 0. 这个模块此前不在任何清单里（本轮发现，2026-11-28 T109）
//!
//! * `docs/coverage-matrix.tsv`（79 行）里**没有** `graphwar_ban_list`；
//! * `docs/02-module-map.md` 的 78 模块表里也**没有**它（只有 `docs/02` §22.3 的
//!   `GraphwarBanList | graphwar_ban_list | size 96` 这一行间接证明它存在）；
//! * Linux 2.0.60 的 `.so` 里 `"graphwar_ban_list.rs"` 这个 **Location 文件名串出现 0 次**
//!   ⇒ 该模块**一条 `panic::Location` 都没有** ⇒ 527 分母（Location 口径）结构上看不见它，
//!   `docs/coverage-matrix.tsv` 的模块清单也一样看不见。
//!   （对照：Android `.symtab` 里这个名字出现了 2 个函数。）
//!
//! ⇒ 模块分母「78 = 62 + 16」**至少少 2 个**（本模块 + `graphwar_version`，见 `docs/06` §153）。
//!
//! ## 1. 函数清单（Android 侧**只有 2 个**，都没进过 527 分母）
//!
//! | 函数 | arm64 vaddr / 字节 | 源码行 | 职责 | 档位 |
//! |---|---|---|---|---|
//! | `new` | `0x4086a0` / 348 | — | 建两个空 `HashMap`（各调一次 `RandomState::new` 的 thread-local） | **观测** |
//! | `is_connection_banned` | `0x408460` / 576 | — | `u32` 连接号 → 是否封禁 | **观测** |
//!
//! 两个函数的地址范围都恰好接到下一个符号（`0x408460 + 0x240 = 0x4086a0`、
//! `0x4086a0 + 0x15c = 0x4087fc` 之后是 `brk`/`_Unwind_Resume` 收尾），**无重叠符号**。
//!
//! ## 2. 结构体形状（96 字节 = 2 × 48，**观测** + 1 处**推断**）
//!
//! `is_connection_banned` 里两次 hashbrown 查表用的是**两套独立字段**（偏移全部直读）：
//!
//! | 表 | ctrl | bucket_mask | `items`（空判） | hasher | 跨度 |
//! |---|---|---|---|---|---|
//! | 表 A（先建，`+0x00`） | `+0x00` | `+0x08` | **`+0x18`** | `+0x20` | `0x00..0x30` |
//! | 表 B（`+0x30`） | `+0x30` | `+0x38` | **`+0x48`** | `+0x50` | `0x30..0x60` |
//!
//! 两个表各 48 字节（`ctrl 8 + mask 8 + growth_left 8 + items 8 + RandomState 16`）
//! ⇒ `0x60 = 96`，与 `graphwar_module_container::MODULES` 里登记
//! `GraphwarBanList` 的 `size: 96`（模块容器的 vtable `size` 字）**逐值相符** ——
//! 这是两条独立证据的交叉验证，不是循环论证（一条来自本函数指令，一条来自
//! 注册表 vtable，见 `src/graphwar_module_container.rs`）。
//!
//! `new`（`0x408760 alloc(0x78, 8)`）分配的**不是** 96 而是 **120 = 0x78** 字节：
//! `RcInner<RefCell<GraphwarBanList>>` = `strong 8 + weak 8 + RefCell borrow 8 + value 96`。
//! 独立的符号证据：`0x4087f0 bl drop_in_place<RcInner<RefCell<GraphwarBanList>>>`
//! （符号名 `0x416c64`）与 `0x41c3c0 Rc::drop_slow`。⇒ 原版 `new` 的返回类型是
//! **`Rc<RefCell<GraphwarBanList>>`**（**观测**），不是裸 `Self`。
//!
//! ## 3. `is_connection_banned` 的语义（**观测**，逐条指令）
//!
//! ```text
//! 0x408470  ldr x8,[x0,#0x48]; cbz x8 → 0x408678(mov w0,wzr)   ; 表 B 空 ⇒ false
//! 0x408484  add x0,x0,#0x50 / hash_one(&connection: u32)        ; 用表 B 的 hasher
//! 0x4084a4  ldp x10,x9,[x19,#0x30] → ctrl/mask；探测循环        ; hashbrown find
//! 0x4084d8  ldur w14,[x21,#-0x18]; cmp w20,w14；不等则继续探测  ; 键 = 4 字节 u32
//! 0x408524  ldr x8,[x19,#0x18]; cbz x8 → 0x408678               ; 表 A 空 ⇒ false
//! 0x40852c  add x0,x0,#0x20 / hash_one(取到的那条 value)         ; 用表 A 的 hasher
//! 0x408538  ldp x10,x9,[x19] → ctrl/mask；探测循环              ; 表 A find（键 = 同一个 value）
//! 0x40868c  mov w0,#1                                            ; 两表都命中 ⇒ true
//! ```
//!
//! ⇒ `is_connection_banned(c) = 表B[c] 存在 ∧ 表A[表B[c]] 存在`（**观测**）。
//! 表 A 的元素类型与表 B 的 value 是**同一个类型**（同一块字节被两次 hash/比较）。
//!
//! ### 3.1 value 的机器形状（**观测**）与它的**业务含义**（**未知**）
//!
//! 表 B 的元素步长 **24 = 0x18**（`0x40849c mov w11,#0x18` + `mneg x14,x14,x11`），
//! 槽位 = `键 u32(4) + value(20)`。value 的判别位在 **value+0 的最低位**
//! （`0x408548 ldurb w8,[x21,#-0x14]` + `0x40854c tbz w8,#0x0`）：
//!
//! | 变体（按判别位） | 载荷 | 读它的指令 |
//! |---|---|---|
//! | LSB = 0 | `u32` @ value+1 | `0x4085e8 ldur w11,[x21,#-0x13]` |
//! | LSB = 1 | `(u64, u64)` @ value+1 / value+9 | `0x408554` / `0x408550` |
//!
//! ⚠️ 这两个变体**叫什么、代表什么**（玩家？IP？原因串？）**没有证据**：
//! 表里没有字符串、没有写表点的定位（见 §5）。本实现按机器形状命名为
//! [`BanTarget::Word`] / [`BanTarget::Pair`] 并**明确标成"形状已知、含义未知"**。
//!
//! ## 4. 调用者与它造成的**可见行为**（**观测**，这条比函数本身有用）
//!
//! `is_connection_banned` 全 `.text` 里只有 **1 个 `bl` 调用点**：
//! `0x3e5948`，在 `GraphwarRemoteEventHandler::handle_client_remote_event`（`0x3e3ad0`）里。
//! 它拿到 `RcInner` 指针后 `add x0,x0,#0x18`（= value 起点，与 §2 的 RcInner 布局一致）、
//! `+0x10` 递增/递减（= `RefCell::borrow` / `borrow` 结束），然后：
//!
//! * **返回 true** ⇒ `0x3e5960` 起 `alloc(0x15, 1)` 并逐字节拷 `.rodata 0x19bff0` 起的
//!   **21 字节 = `"You have been kicked."`**，再 `0x3e59b8 bl GraphwarEventQueue::add_event`
//!   ⇒ **封禁的效果 = 往事件队列里塞一条 "You have been kicked."**；
//! * 返回 false ⇒ `0x3e5a3c` 继续正常处理该客户端事件。
//!
//! ## 5. 未知 / 未做
//!
//! * **写表点未定位**：两个表的 `insert` 在哪、`BanConnectionRequest` 变成什么 value，
//!   本轮没解（要追 `handle_ban_connection_request_event` 那一侧的数据流，
//!   且它与 `graphwar_server_connection_handler` 里的 `bans` 是不是同一张表**尚未证**）。
//! * **value 的业务含义未知**（§3.1）。
//! * 表 A 是 `HashSet<BanTarget>` 还是 `HashMap<BanTarget, V>`：**推断**取 `HashSet`
//!   （两侧比较的字节完全一致、且没有看到 V 的额外步长），`V != ()` 的情形未排除。
//! * 本模块**没有** `panic::Location` ⇒ 任何"原版第几行"的说法都不成立。

use std::collections::{HashMap, HashSet};

/// `size_of::<GraphwarBanList>()`：**96**。
///
/// 档位：**观测** —— 本模块 `is_connection_banned` 的字段偏移（§2 表）算出 `0x60`，
/// 且与 `graphwar_module_container::MODULES` 里 `GraphwarBanList { size: 96 }` 一字不差。
pub const SIZE: usize = 96;

/// `new` 真正分配的那块内存：`RcInner<RefCell<GraphwarBanList>>` = **120 字节**。
///
/// 档位：**观测**（`0x408760 mov w0,#0x78` + `0x408764 mov w1,#8` + `bl __rust_alloc`；
/// 收尾 `0x4087f0 bl drop_in_place<RcInner<RefCell<GraphwarBanList>>>`）。
pub const RC_INNER_SIZE: usize = 0x78;

/// 封禁表里"那一头"的值类型 —— **机器形状是观测，业务含义是未知**。
///
/// 形状（`is_connection_banned` 的读法，见模块文档 §3.1）：判定位 = 第 0 字节的最低位；
/// `Word` 载荷 4 字节、`Pair` 载荷 16 字节（两个 `u64`），整个槽位 20 字节。
///
/// ⚠️ `Word` / `Pair` 这两个名字**不是原版名字**（原版名字没有证据），
/// 只是"一个 4 字节变体 + 一个 16 字节变体"的占位。业务含义（玩家 id？地址？原因？）**未知**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BanTarget {
    /// 判定位 LSB = 0：载荷是 1 个 `u32`（`0x4085e8`）。
    Word(u32),
    /// 判定位 LSB = 1：载荷是 2 个 `u64`（`0x408554` / `0x408550`）。
    Pair(u64, u64),
}

/// 封禁表：**两个** `HashMap`（各 48 字节 = `表 A` + `表 B`）。
///
/// 字段顺序 = 原版偏移顺序：`banned` 在 `+0x00`（表 A，先建），
/// `by_connection` 在 `+0x30`（表 B）。两个都用 `std` 默认的 `RandomState`
/// （`new` 里两次 `thread_local::Storage::get` + 计数器自增，`0x4086bc` / `0x4086f8`）。
///
/// 字段**公开**只是为了测试能造状态：原版的写入点（`insert`）本轮**没有定位**（见模块文档 §5），
/// 所以这里故意**不提供** `ban()` 之类的方法 —— 那是编语义。
#[derive(Debug, Default)]
pub struct GraphwarBanList {
    /// 表 A（`+0x00`）：被"登记"的那些值本身。
    pub banned: HashSet<BanTarget>,
    /// 表 B（`+0x30`）：连接号 → 它对应的那个值。
    pub by_connection: HashMap<u32, BanTarget>,
}

impl GraphwarBanList {
    /// `GraphwarBanList::new`（arm64 **`0x4086a0`** / 348 B）。
    ///
    /// 原版返回 `Rc<RefCell<GraphwarBanList>>`（§2 的 120 字节分配），这里返回裸 `Self`
    /// —— 所有权包装是**桥接层的事**（`src/_scaffold` 的模块容器），不进原版模块文件。
    pub fn new() -> Self {
        // 两个默认 hasher 的构造 = 原版两次 thread-local 取数（0x4086bc / 0x4086f8）。
        Self {
            banned: HashSet::new(),
            by_connection: HashMap::new(),
        }
    }

    /// `GraphwarBanList::is_connection_banned`（arm64 **`0x408460`** / **576** B）：
    /// 这个连接号是否被封。
    ///
    /// 三条提前返回的**空判**是原版逐条可读的（顺序不能换）：
    /// 1. `0x408470 ldr x8,[x0,#0x48]; cbz x8 ⇒ false` —— 表 B 的 `items == 0`；
    /// 2. 表 B 里查不到这个 `u32` ⇒ `false`（`0x4084f0` 那组 ctrl 全 `0xffff…` ⇒ 未命中）；
    /// 3. `0x408524 ldr x8,[x19,#0x18]; cbz x8 ⇒ false` —— 表 A 的 `items == 0`；
    /// 4. 表 A 命中 ⇒ `0x40868c mov w0,#1`（`true`），否则 `false`。
    pub fn is_connection_banned(&self, connection: u32) -> bool {
        if self.by_connection.is_empty() {
            return false;
        }
        let Some(target) = self.by_connection.get(&connection) else {
            return false;
        };
        if self.banned.is_empty() {
            return false;
        }
        self.banned.contains(target)
    }
}
