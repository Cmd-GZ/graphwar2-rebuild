//! `graphwar_module_container.rs` —— 各 manager 的**注册表 / 查询口**（原版 `GraphwarModuleContainer`）。
//!
//! # 一页结论（先看这段，它推翻了此前的一条主流猜测）
//!
//! **它不是"按固定偏移持有 25 个 manager 的结构体"。** 原版的整个类型是：
//!
//! ```text
//! pub struct GraphwarModuleContainer {
//!     modules: HashMap<TypeId, Rc<dyn Any>>,      // 唯一字段，48 字节
//! }
//! ```
//!
//! 判据（`观测`，全部可复算）：
//!
//! * `GraphwarModuleContainer::add_module` 的第 3 个实参是
//!   **一个 4 字的 `dyn Any` vtable**：`[drop_in_place::<RefCell<M>>, size_of::<RefCell<M>>,
//!   align_of::<RefCell<M>>, <_ as Any>::type_id]`
//!   （arm64 从 `0x858aa8` 起、x86 从 `0x958398` 起，每型一个、间距 `0x20`）；
//! * 键是 **`TypeId`（16 字节）**，不是字符串、也不是字段偏移：x86 `add_module` 里
//!   `movups <rodata>,%xmm0` + `lea <vtable>,%rcx` + `call hashbrown::HashMap::insert`
//!   （arm64 对应 `ldr q0,[x8]` + `adrp/add` + `bl HashMap::insert`）；
//! * `get_module` 走 `hash_one` + 线性探测，步长 `0x20` = **32 字节实体**
//!   （16 字节 `TypeId` 键 + 16 字节 `Rc<dyn Any>` 胖指针）——
//!   这是 `HashMap<TypeId, Rc<dyn Any>>` 的实体布局，不是"字段偏移"；
//! * 容器自身的实例大小 **48 字节**：`RawTable`（32）+ `RandomState`（16）。
//!   交叉证据：`GraphwarGameClient::new` 为 `Rc<RefCell<GraphwarModuleContainer>>`
//!   申请 **`0x48` = 72 字节** = Rc 头 16 + `RefCell` 借用标志 8 + 容器值 48，
//!   随后每次 `add_module` 的 `self` 都是 `分配基址 + 0x18`
//!   （arm64 `0x409df4: add x0, x21, #0x18`）。
//!
//! ⇒ **"各 manager 在 `+0xNN` 偏移"这条假设不成立**：
//!
//! * `+0x128` / `+0x240` 是 **`GraphwarLanVBox` 自己的字段**
//!   （`LanVBox.lan_broadcaster` / `LanVBox.server_broker_handler`，
//!   `docs/144-lobby-fixes.md` §1 的逐指令记录；`docs/09-network.md` §10.4
//!   把它们写成"根节点模块容器里的模块槽"，**那个措辞要按本节更正**——
//!   容器里没有槽）；
//! * `+0x16a0` / `+0x16a8` 是 `GraphwarRootNode` 的 `GraphwarTextureLoader` /
//!   `GraphwarPlayerConfig`（`docs/16-client.md` §15 / §25）；
//! * `GraphwarRootNode` 里真正通往容器的那条链是
//!   `root+0x1668`（某个 manager 的 `Rc<RefCell<_>>`）→ 该 manager `+0x238`
//!   （`Rc<RefCell<GraphwarModuleContainer>>`）→ `.borrow().get_module::<X>()`
//!   （arm64 `0x39d3ac`–`0x39d3e8`）。
//!
//! 容器里**一个偏移都不存**——存的是 `TypeId`，取回来要 `downcast`。
//!
//! # 68 个函数是什么
//!
//! 全模块只有 **3 个方法**（`add_module` / `get_module` / `get_module_wk`，
//! 见 `tools/symtab_dump.py methods graphwar_module_container`）；
//! 2.0.60 的 68 = 三个泛型方法对 25 个 manager 类型的单态化：
//!
//! | 方法 | 单态化个数 | arm64 块首 | x86 块首 | 块内步长（arm64 / x86） |
//! |---|---|---|---|---|
//! | `add_module` | **25** | `0x49e520` | `0x4694f0` | `212` / `160` |
//! | `get_module` | **23** | `0x49f9d4` | `0x46a490` | `432` / `368` |
//! | `get_module_wk` | **20** | `0x4a20a4` | `0x46c5a0` | `644` / `544` |
//!
//! **版本差**（顺带量到，`观测`）：2024-07-30 那一版是 **24 / 23 / 19 = 66** 个
//! 单态化（`tools/symtab_dump.py modules` 的 66 就是这个数），2.0.60 变成
//! **25 / 23 / 20 = 68** ⇒ 两年里**多注册了 1 个模块、多要了 1 个弱句柄**。
//! 究竟是哪一型新增的：`未知`（没做 2024↔2.0.60 的逐型对齐）。
//!
//! 六个块都是**严格等距、无空洞**（`base + i × stride`，`i` 取遍 `0..n-1`），
//! 这一条本身就是一个强判据：`tests/graphwar_module_container.rs` 用它反查整张表。
//!
//! 没有 `new` / `process` / 事件分派：**装配在调用者手里**——
//! arm64 `GraphwarGameClient::new`（`0x409c70`）注册 22 个、
//! `graphwar_game_server::game_server_thread`（`0x483574`）注册 20 个、
//! `GraphwarBackend::start_*`（`0x407848` / `0x407b9c`）各注册 1 个
//! `GraphwarGameRecorder`。x86 侧同形（`0x517690` / `0x4500c0` / `0x515590` / `0x5159a0`），
//! 见 [`REGISTRATION_SITES`]。三个装配点都在**别的模块**里，所以本文件不"装配"它们。
//!
//! # 为什么 `panic::Location` 在这模块上几乎全哑
//!
//! * `add_module` / `get_module` **一个 panic 点都没有**（无 `unwrap`、无索引、
//!   无 `expect`），所以两版符号表里它们**都没有** `Location` 锚，
//!   `fixtures/name_to_addr.tsv` 把它们全标成「未知」——这是**正确**的，
//!   不是数据残缺，也不是工具坏了。
//! * `get_module_wk` 有**且仅有 1 个**锚：`src/graphwar_module_container.rs:30:28`，
//!   20 个单态化共用。它来自 `RefCell::borrow` 的 `#[track_caller]`：
//!   `panic_already_mutably_borrowed` 抛错时带的是**调用者**的 `Location`，
//!   也就是源码第 **30** 行第 **28** 列那个 `.borrow()`。
//!   这条同时是"`get_module_wk` 自己会 `borrow()`"的**签名级**证据。
//! * 因此本模块的 x86 地址**不是**从 `Location` 来的，而是三路互证：
//!   ①4 字 `dyn Any` vtable 的 `size` 字（= `size_of::<RefCell<M>>()`）、
//!   ②`add_module` 里的 `TypeId` 常量（即 `HashMap` 的键）、
//!   ③**两个装配点的调用序**（同一份源码，两架构同序；位置与尺寸序列逐一相等）。
//!   详见 `docs/02-module-map.md` §22 的复算脚本。
//!
//! # 三个方法（形状从指令推，语义在 `tests/` 里验）
//!
//! ```text
//! add_module<M: 'static>(&mut self, module: Rc<RefCell<M>>)
//!     键 = TypeId::of::<M>()；值 = module 变成 Rc<dyn Any>（胖指针进桶）
//!     HashMap::insert 的返回值（被替换掉的旧值）被就地丢弃
//!
//! get_module<M: 'static>(&self) -> Option<Rc<RefCell<M>>>
//!     items == 0 快返回 → hash_one(&hash_builder, &TypeId::of::<M>())
//!     → 32 字节实体线性探测 → 命中后 Rc::clone
//!     → 经 vtable 第 4 字调 Any::type_id → 与 TypeId::of::<RefCell<M>>() 比对
//!     → 相等则 Rc::downcast::<RefCell<M>>()，不等则丢掉克隆返回 None
//!     （注意：**键**是 M 的 TypeId，**downcast 目标**是 RefCell<M> 的 TypeId，
//!       这两个 TypeId 不相等——本模块最容易写错的一处）
//!
//! get_module_wk<M: 'static>(container: &Weak<RefCell<Self>>) -> Option<Weak<RefCell<M>>>
//!     Weak::upgrade（ptr == usize::MAX 判 None；strong == 0 判 None）
//!     → Rc::borrow（借用标志 +1；已是 -1 则 panic_already_mutably_borrowed）
//!     → 同上查表 + downcast → Rc::downgrade → 放掉 Ref
//! ```
//!
//! 两个细节是**签名**层面的证据、不是猜测：
//!
//! * `get_module` 的 `self` 是**容器值本身**（`&Self`；偏移 0 起就是 `HashMap`：
//!   `ldr x8,[x0,#0x18]` 读 `RawTable.items`、`add x0,x0,#0x20` 才是 `RandomState`），
//!   调用点形如 `……borrow().get_module::<X>()`（arm64 `0x39d3e4` 前一条正是
//!   `add x0, x21, #0x18`，`x21` 来自 `ldr x21,[x20,#0x238]` 的
//!   `Rc<RefCell<GraphwarModuleContainer>>` 字段）；
//! * `get_module_wk` 的实参是 **`&Weak<RefCell<Self>>`**（不是 `&Rc`）：
//!   函数头 `ldr x20,[x0]; cmn x20,#1; b.eq <None>` 就是 `Weak::inner()` 的
//!   `ptr == usize::MAX` 判据（`Weak::new()` 的哨兵），随后 `ldr x8,[x20]; cbz`
//!   才是 `Weak::upgrade` 的 strong-count 判据。
//!
//! ⇒ 用法上的分工也就定了：**从 `&Rc` 拿强引用走 `get_module`**（容器自己或
//! `GameClient` 这类强持有者），**从 `Weak` 拿弱引用走 `get_module_wk`**
//! （各 manager 手里只有 `Weak`，用来打断 container ↔ module 的环）。
//!
//! # 25 个被注册的模块（[`MODULES`]）
//!
//! 类型名与模块路径是 arm64 2.0.60 `.symtab` 的**原文**
//! （`drop_in_place<RefCell<…>>` 与 `<Type>::new`），`size` 是 `size_of::<M>()`
//! （= vtable 第 2 字 − 8）。三个地址槽 `add` / `get` / `wk`，`0` 表示**该型没有这个单态化**。
//!
//! 覆盖关系（三路一致，`tests/` 逐条断言）：
//!
//! * `add_module` 25 个 = 全部 25 型；
//! * `get_module` 23 个 = 25 型 **减去** `GraphwarBanList` 与
//!   `GraphwarEntityEffectManager`（这两个只走 `get_module_wk`，不需要强引用）；
//! * `get_module_wk` 20 个 = 25 型 **再减去** `FunctionHistoryRecorder` /
//!   `GraphwarLocalPlayerManager` / `GraphwarClientConnectionHandler` /
//!   `GraphwarServerConnectionHandler` / `GraphwarGameRecorder`
//!   （这五个只由持有者强引用，从不需要弱句柄）。
//!
//! 第 26 个 `RefCell` 单态化 `graphwar_event_queue::GraphwarEventQueue`
//! **不在**表里：它有 `drop_in_place<RefCell<…>>`，但没有对应的 `add_module`
//! ——说明它虽然也是"被 `Rc<RefCell<_>>` 包着的模块"，却**不注册进本容器**。
//! 这是否意味着它挂在别处，本文件不下结论（`未知`）。
//!
//! # 档位
//!
//! | 结论 | 档位 | 依据 |
//! |---|---|---|
//! | `HashMap<TypeId, Rc<dyn Any>>`、48 字节 | **观测** | vtable 四字 + `hash_one` + 32 字节实体 + 72 字节 `Rc` 分配 |
//! | 三个方法的签名与语义 | **观测** | 两侧反汇编逐指令；`Weak::inner` / `borrow` / `downcast` 形状可辨识 |
//! | 68 个单态化的地址（arm64 + x86） | **观测** | 等距块 + `TypeId` 常量 + 装配点调用序三路互证 |
//! | 25 个类型名 | **观测** | arm64 `.symtab`；x86 侧靠"尺寸序列 + 调用序"对齐后**逐一相等** |
//! | `size_of::<M>()` | **观测** | = vtable 第 2 字 − 8（`RefCell` 借用标志 8 字节） |
//! | 「`get_module_wk` 的锚 = 第 30 行 28 列」 | **推断** | `#[track_caller]` 是 std 的实现事实；原版源码行号本身无法直接复核 |
//! | manager 内部布局（如 GameRecorder 的 `Weak` 字段在 `+0x40`） | **观测**（举例） | `Weak::upgrade` 内联序列；但各 manager 内部字段**不属本模块**，不在此表 |
//!
//! # 复算命令（不要凭本文件的数字，自己跑）
//!
//! ```text
//! # 1) arm64 2.0.60 未 strip：三个方法名 + 68 个单态化
//! #    （注意：`grep … | grep FUNC | wc -l` 是 69 —— 多出来那条是
//! #     drop_in_place<Weak<RefCell<GraphwarModuleContainer>>>，不是方法）
//! readelf -sW .rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so \
//!   | grep graphwar_module_container | grep FUNC \
//!   | grep -c 'GraphwarModuleContainer1[03]'                     # 68
//! # 分项：25 add_module / 23 get_module / 20 get_module_wk
//!
//! # 2) 每个 add_module 的 vtable：adrp/add 出 .data.rel.ro 地址；
//! #    该地址的 R_AARCH64_RELATIVE addend 就是 drop_in_place::<RefCell<M>>
//! #    （.rodata 里那个 16 字节常量是 TypeId::of::<M>()）
//! llvm-objdump -d --no-show-raw-insn --start-address=0x49e520 \
//!   --stop-address=0x49e5f4 <arm64 so>
//!
//! # 3) x86 侧：先由 Location 30:28 定出 20 个 get_module_wk
//! #    （.data.rel.ro 0x9586b8 处的 {file,len,30,28} 记录谁引用），
//! #    再按 add=160 / get=368 / wk=544 的等距块向低地址展开，
//! #    最后用 GraphwarGameClient::new 的调用序给 25 个 add_module 定名。
//! ```
//!
//! # 本文件里哪些东西**不是**原版的
//!
//! * [`Slots`] / [`ModuleEntry`] / [`Block`] / [`MODULES`] / [`REGISTRATION_SITES`]
//!   是**我们的恢复台账**（数据结构），原版里只有一个 `HashMap`；
//! * [`GraphwarModuleContainer::len`] / [`GraphwarModuleContainer::is_empty`] /
//!   [`GraphwarModuleContainer::contains`] 是从 `HashMap` 语义**直接可推**的便捷口，
//!   原版没有导出它们（`get_module` 的 `items == 0` 快路径等价于 `is_empty`）。
//!   它们的用处是让测试不靠 `downcast` 反复造轮子。

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

// ---------------------------------------------------------------------------
// 常量：三个单态化块（地址是 ELF vaddr；Ghidra / REA 地址 = + 0x100000）
// ---------------------------------------------------------------------------

/// `GraphwarModuleContainer` 实例的字节数（`HashMap<TypeId, Rc<dyn Any>>`）。
///
/// 反证链：`GraphwarGameClient::new` 申请 `0x48` 字节给
/// `Rc<RefCell<GraphwarModuleContainer>>`；Rc 头 16 + `RefCell` 借用标志 8
/// ⇒ 容器值 48，且 `add_module` 的 `self` 恒为 `分配基址 + 0x18`。
pub const CONTAINER_SIZE: usize = 48;

/// `HashMap` 实体的字节数：16（`TypeId` 键）+ 16（`Rc<dyn Any>` 胖指针）。
///
/// 判据：`get_module` 探测循环里的 `sub x15, x10, x15, lsl #5`（`× 32`）。
pub const BUCKET_ENTRY_SIZE: usize = 32;

/// `dyn Any` vtable 的字数：`drop_in_place` / `size` / `align` / `type_id`。
pub const ANY_VTABLE_WORDS: usize = 4;

/// `get_module_wk` 唯一的 `panic::Location`：`src/graphwar_module_container.rs:30:28`。
///
/// 来自 `RefCell::borrow` 的 `#[track_caller]`（见文件头）。
pub const WK_BORROW_LOCATION: (u32, u32) = (30, 28);

/// `add_module` 单态化的个数。
pub const ADD_MODULE_COUNT: usize = 25;
/// `get_module` 单态化的个数。
pub const GET_MODULE_COUNT: usize = 23;
/// `get_module_wk` 单态化的个数。
pub const GET_MODULE_WK_COUNT: usize = 20;
/// 本模块函数总数（2024 版符号表 + 2.0.60 两架构实测，三者相等）。
pub const FUNCTION_COUNT: usize = ADD_MODULE_COUNT + GET_MODULE_COUNT + GET_MODULE_WK_COUNT;

/// 一个单态化在某架构上的三处入口。
///
/// `get` / `wk` 的 `0` 表示**该型确实没有这个单态化**（不是数据缺失）：
/// 25 型里有 2 型没有 `get`、5 型没有 `wk`，见文件头的覆盖关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slots {
    /// `add_module` 入口。
    pub add: u32,
    /// `get_module` 入口；`0` = 无。
    pub get: u32,
    /// `get_module_wk` 入口；`0` = 无。
    pub wk: u32,
}

impl Slots {
    /// 该型是否有 `get_module` 单态化。
    pub const fn has_get(&self) -> bool {
        self.get != 0
    }
    /// 该型是否有 `get_module_wk` 单态化。
    pub const fn has_wk(&self) -> bool {
        self.wk != 0
    }
}

/// 一个被容器注册的模块类型（[`MODULES`] 的元素）。
///
/// 名字是 arm64 2.0.60 `.symtab` 的**原文**，不是我们起的别名。
#[derive(Debug, Clone, Copy)]
pub struct ModuleEntry {
    /// 类型名，如 `"GraphwarModeManager"`。
    pub name: &'static str,
    /// 原版模块（`src/<module_path>.rs`），如 `"graphwar_mode_manager"`。
    pub module_path: &'static str,
    /// `size_of::<M>()`（= vtable 的 `size` 字 − 8）。
    pub size: u32,
    /// arm64 2.0.60 的三个入口。
    pub arm64: Slots,
    /// x86 2.0.60（Linux Steam）的三个入口。
    pub x86: Slots,
}

/// 三个单态化块在某一架构上的位置（块内严格等距）。
#[derive(Debug, Clone, Copy)]
pub struct Block {
    /// 块首：第 0 个单态化的入口。
    pub base: u32,
    /// 块内步长：每往后一个单态化前进多少字节。
    pub stride: u32,
    /// 块内单态化个数。
    pub count: usize,
}

impl Block {
    /// 第 `i` 个单态化的入口（`i < count`）。
    pub const fn nth(&self, i: usize) -> u32 {
        self.base + (i as u32) * self.stride
    }

    /// 这个地址属于本块吗？是则返回块内序号。
    ///
    /// 判据用"等距 + 不越界"，不查表——所以它能反过来校验 [`MODULES`]。
    pub const fn index_of(&self, addr: u32) -> Option<usize> {
        if addr < self.base || self.stride == 0 {
            return None;
        }
        let off = addr - self.base;
        if off % self.stride != 0 {
            return None;
        }
        let i = (off / self.stride) as usize;
        if i < self.count {
            Some(i)
        } else {
            None
        }
    }
}

/// arm64 2.0.60 的 `add_module` 块。
pub const ARM64_ADD_MODULE: Block = Block { base: 0x49e520, stride: 212, count: ADD_MODULE_COUNT };
/// arm64 2.0.60 的 `get_module` 块。
pub const ARM64_GET_MODULE: Block = Block { base: 0x49f9d4, stride: 432, count: GET_MODULE_COUNT };
/// arm64 2.0.60 的 `get_module_wk` 块。
pub const ARM64_GET_MODULE_WK: Block =
    Block { base: 0x4a20a4, stride: 644, count: GET_MODULE_WK_COUNT };

/// x86 2.0.60（Linux Steam）的 `add_module` 块。
pub const X86_ADD_MODULE: Block = Block { base: 0x4694f0, stride: 160, count: ADD_MODULE_COUNT };
/// x86 2.0.60 的 `get_module` 块。
pub const X86_GET_MODULE: Block = Block { base: 0x46a490, stride: 368, count: GET_MODULE_COUNT };
/// x86 2.0.60 的 `get_module_wk` 块。
pub const X86_GET_MODULE_WK: Block = Block { base: 0x46c5a0, stride: 544, count: GET_MODULE_WK_COUNT };

/// 一处"装配点"：谁在往容器里塞模块。
///
/// 容器自己没有 `init`——25 个 `add_module` 由这几处调用者发出，
/// 且两架构的**调用序相同**（这是 x86 侧定名的第三路证据）。
#[derive(Debug, Clone, Copy)]
pub struct RegistrationSite {
    /// 说明（原版符号名，去 hash）。
    pub what: &'static str,
    /// arm64 2.0.60 入口。
    pub arm64: u32,
    /// x86 2.0.60 入口。
    pub x86: u32,
    /// 该处发出的 `add_module` 调用数。
    pub count: usize,
}

/// 四个装配点（`GraphwarBackend::start_*` 是两条孪生单态化，各注册一次
/// `GraphwarGameRecorder`，故各记 1）。
///
/// ⚠️ `22 + 20 + 1 + 1 = 44 > 25`：同一型会被**多处**注册（每个
/// `GameClient` / 每个服务端线程都有自己的容器），所以这里是
/// "调用点个数"，不是"类型个数"。
pub const REGISTRATION_SITES: &[RegistrationSite] = &[
    RegistrationSite { what: "GraphwarGameClient::new", arm64: 0x409c70, x86: 0x517690, count: 22 },
    RegistrationSite {
        what: "graphwar_game_server::game_server_thread",
        arm64: 0x483574,
        x86: 0x4500c0,
        count: 20,
    },
    RegistrationSite {
        what: "GraphwarBackend::start_* (第 1 个单态化)",
        arm64: 0x407848,
        x86: 0x515590,
        count: 1,
    },
    RegistrationSite {
        what: "GraphwarBackend::start_* (第 2 个单态化)",
        arm64: 0x407b9c,
        x86: 0x5159a0,
        count: 1,
    },
];

/// 25 个被注册的模块，按模块名字典序（`FunctionHistoryRecorder` 是唯一不带
/// `Graphwar` 前缀的，按 `function_history_recorder` 排在此处）。
///
/// 顺序本身没有语义：用 [`find`] / [`find_by_module`] / [`slot_of`] 查，
/// 不要按序号编程。
pub const MODULES: &[ModuleEntry] = &[
    ModuleEntry { name: "GraphwarBanList", module_path: "graphwar_ban_list", size: 96,
                  arm64: Slots { add: 0x49e79c, get: 0, wk: 0x4a3240 },
                  x86: Slots { add: 0x4699f0, get: 0, wk: 0x46d260 } },
    ModuleEntry { name: "GraphwarClientConnectionHandler", module_path: "graphwar_client_connection_handler", size: 112,
                  arm64: Slots { add: 0x49f758, get: 0x4a05a4, wk: 0 },
                  x86: Slots { add: 0x469950, get: 0x46b180, wk: 0 } },
    ModuleEntry { name: "GraphwarColorManager", module_path: "graphwar_color_manager", size: 120,
                  arm64: Slots { add: 0x49e6c8, get: 0x4a1684, wk: 0x4a20a4 },
                  x86: Slots { add: 0x469590, get: 0x46b5d0, wk: 0x46dd00 } },
    ModuleEntry { name: "GraphwarComputerPlayerManager", module_path: "graphwar_computer_player_manager", size: 72,
                  arm64: Slots { add: 0x49e5f4, get: 0x4a0e14, wk: 0x4a4660 },
                  x86: Slots { add: 0x469e50, get: 0x46b460, wk: 0x46c5a0 } },
    ModuleEntry { name: "GraphwarEffectManager", module_path: "graphwar_effect_manager", size: 72,
                  arm64: Slots { add: 0x49f0b8, get: 0x49f9d4, wk: 0x4a2328 },
                  x86: Slots { add: 0x469630, get: 0x46a600, wk: 0x46cc00 } },
    ModuleEntry { name: "GraphwarEntityEffectManager", module_path: "graphwar_entity_effect_manager", size: 72,
                  arm64: Slots { add: 0x49ebc0, get: 0, wk: 0x4a2830 },
                  x86: Slots { add: 0x469d10, get: 0, wk: 0x46dae0 } },
    ModuleEntry { name: "GraphwarFunctionManager", module_path: "graphwar_function_manager", size: 32,
                  arm64: Slots { add: 0x49ea18, get: 0x4a1834, wk: 0x4a39cc },
                  x86: Slots { add: 0x469db0, get: 0x46bb90, wk: 0x46e9c0 } },
    ModuleEntry { name: "GraphwarGameRecorder", module_path: "graphwar_game_recorder", size: 56,
                  arm64: Slots { add: 0x49f5b0, get: 0x4a19e4, wk: 0 },
                  x86: Slots { add: 0x469c70, get: 0x46aea0, wk: 0 } },
    ModuleEntry { name: "GraphwarGameStateManager", module_path: "graphwar_game_state_manager", size: 296,
                  arm64: Slots { add: 0x49ee3c, get: 0x4a0094, wk: 0x4a3748 },
                  x86: Slots { add: 0x46a210, get: 0x46be70, wk: 0x46c7c0 } },
    ModuleEntry { name: "GraphwarGraphicsManager", module_path: "graphwar_graphics_manager", size: 168,
                  arm64: Slots { add: 0x49e944, get: 0x49fb84, wk: 0x4a4dec },
                  x86: Slots { add: 0x4698b0, get: 0x46a8e0, wk: 0x46d480 } },
    ModuleEntry { name: "GraphwarLifeManager", module_path: "graphwar_life_manager", size: 72,
                  arm64: Slots { add: 0x49eaec, get: 0x4a1324, wk: 0x4a48e4 },
                  x86: Slots { add: 0x469b30, get: 0x46c2c0, wk: 0x46d6a0 } },
    ModuleEntry { name: "GraphwarLocalPlayerManager", module_path: "graphwar_local_player_manager", size: 64,
                  arm64: Slots { add: 0x49f334, get: 0x4a0c64, wk: 0 },
                  x86: Slots { add: 0x4694f0, get: 0x46c430, wk: 0 } },
    ModuleEntry { name: "GraphwarModeManager", module_path: "graphwar_mode_manager", size: 32,
                  arm64: Slots { add: 0x49e520, get: 0x4a0754, wk: 0x4a5070 },
                  x86: Slots { add: 0x469a90, get: 0x46abc0, wk: 0x46e140 } },
    ModuleEntry { name: "GraphwarNameManager", module_path: "graphwar_name_manager", size: 72,
                  arm64: Slots { add: 0x49ec94, get: 0x4a1174, wk: 0x4a3ed4 },
                  x86: Slots { add: 0x46a2b0, get: 0x46ba20, wk: 0x46df20 } },
    ModuleEntry { name: "GraphwarObstacleManager", module_path: "graphwar_obstacle_manager", size: 80,
                  arm64: Slots { add: 0x49f82c, get: 0x4a14d4, wk: 0x4a4b68 },
                  x86: Slots { add: 0x46a3f0, get: 0x46bd00, wk: 0x46ebe0 } },
    ModuleEntry { name: "GraphwarPlanePosManager", module_path: "graphwar_plane_pos_manager", size: 24,
                  arm64: Slots { add: 0x49efe4, get: 0x4a0904, wk: 0x4a25ac },
                  x86: Slots { add: 0x46a0d0, get: 0x46ad30, wk: 0x46c9e0 } },
    ModuleEntry { name: "GraphwarPlayerManager", module_path: "graphwar_player_manager", size: 72,
                  arm64: Slots { add: 0x49f260, get: 0x4a0244, wk: 0x4a3c50 },
                  x86: Slots { add: 0x469f90, get: 0x46a490, wk: 0x46d8c0 } },
    ModuleEntry { name: "GraphwarRankManager", module_path: "graphwar_rank_manager", size: 72,
                  arm64: Slots { add: 0x49ef10, get: 0x49fee4, wk: 0x4a2ab4 },
                  x86: Slots { add: 0x469ef0, get: 0x46bfe0, wk: 0x46e360 } },
    ModuleEntry { name: "GraphwarServerConnectionHandler", module_path: "graphwar_server_connection_handler", size: 128,
                  arm64: Slots { add: 0x49f684, get: 0x4a1ef4, wk: 0 },
                  x86: Slots { add: 0x4696d0, get: 0x46b2f0, wk: 0 } },
    ModuleEntry { name: "GraphwarSoldierManager", module_path: "graphwar_soldier_manager", size: 72,
                  arm64: Slots { add: 0x49f18c, get: 0x4a1b94, wk: 0x4a2d38 },
                  x86: Slots { add: 0x46a170, get: 0x46a770, wk: 0x46e580 } },
    ModuleEntry { name: "GraphwarTargetManager", module_path: "graphwar_target_manager", size: 72,
                  arm64: Slots { add: 0x49f408, get: 0x4a1d44, wk: 0x4a4158 },
                  x86: Slots { add: 0x46a350, get: 0x46c150, wk: 0x46ee00 } },
    ModuleEntry { name: "GraphwarTeamManager", module_path: "graphwar_team_manager", size: 32,
                  arm64: Slots { add: 0x49f4dc, get: 0x4a0fc4, wk: 0x4a43dc },
                  x86: Slots { add: 0x469770, get: 0x46b8b0, wk: 0x46e7a0 } },
    ModuleEntry { name: "GraphwarTickHandler", module_path: "graphwar_tick_handler", size: 72,
                  arm64: Slots { add: 0x49e870, get: 0x4a0ab4, wk: 0x4a34c4 },
                  x86: Slots { add: 0x469810, get: 0x46aa50, wk: 0x46d040 } },
    ModuleEntry { name: "GraphwarTurnManager", module_path: "graphwar_turn_manager", size: 32,
                  arm64: Slots { add: 0x49f900, get: 0x49fd34, wk: 0x4a2fbc },
                  x86: Slots { add: 0x46a030, get: 0x46b740, wk: 0x46ce20 } },
    ModuleEntry { name: "FunctionHistoryRecorder", module_path: "function_history_recorder", size: 48,
                  arm64: Slots { add: 0x49ed68, get: 0x4a03f4, wk: 0 },
                  x86: Slots { add: 0x469bd0, get: 0x46b010, wk: 0 } },
];

/// 哪个架构（两者都是 **2.0.60**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    /// Android arm64（未 strip，本模块全部类型名的来源）。
    Arm64,
    /// Linux x86-64 Steam（Ghidra 地址 = 本地址 + `0x100000`）。
    X86,
}

/// 三个方法之一。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// `add_module`
    Add,
    /// `get_module`
    Get,
    /// `get_module_wk`
    Wk,
}

/// 按类型名查（`"GraphwarModeManager"`），大小写敏感。
pub fn find(name: &str) -> Option<&'static ModuleEntry> {
    MODULES.iter().find(|m| m.name == name)
}

/// 按原版模块名查（`"graphwar_mode_manager"`，不带 `.rs`）。
pub fn find_by_module(module_path: &str) -> Option<&'static ModuleEntry> {
    MODULES.iter().find(|m| m.module_path == module_path)
}

/// 某个架构上 `Method` 对应的块。
pub const fn block_of(arch: Arch, method: Method) -> Block {
    match (arch, method) {
        (Arch::Arm64, Method::Add) => ARM64_ADD_MODULE,
        (Arch::Arm64, Method::Get) => ARM64_GET_MODULE,
        (Arch::Arm64, Method::Wk) => ARM64_GET_MODULE_WK,
        (Arch::X86, Method::Add) => X86_ADD_MODULE,
        (Arch::X86, Method::Get) => X86_GET_MODULE,
        (Arch::X86, Method::Wk) => X86_GET_MODULE_WK,
    }
}

/// 某型在某架构上某方法的入口（`None` = 该型没有这个单态化）。
pub fn addr_of(entry: &ModuleEntry, arch: Arch, method: Method) -> Option<u32> {
    let slots = match arch {
        Arch::Arm64 => entry.arm64,
        Arch::X86 => entry.x86,
    };
    match method {
        Method::Add => Some(slots.add),
        Method::Get => if slots.has_get() { Some(slots.get) } else { None },
        Method::Wk => if slots.has_wk() { Some(slots.wk) } else { None },
    }
}

/// 反查：这个地址是哪个型、哪个方法？（两架构通用）
///
/// 命中唯一由"三块互不相交 + 块内等距"保证；`None` 表示它不在本模块的 68 个里。
///
/// ```text
/// slot_of(Arch::X86, 0x46a490) -> Some(("GraphwarPlayerManager", Method::Get))
/// slot_of(Arch::X86, 0x4694f0) -> Some(("GraphwarLocalPlayerManager", Method::Add))
/// slot_of(Arch::X86, 0x469500) -> None      # 块内不对齐 160
/// ```
pub fn slot_of(arch: Arch, addr: u32) -> Option<(&'static ModuleEntry, Method)> {
    for method in [Method::Add, Method::Get, Method::Wk] {
        if block_of(arch, method).index_of(addr).is_none() {
            continue;
        }
        if let Some(e) = MODULES.iter().find(|m| addr_of(m, arch, method) == Some(addr)) {
            return Some((e, method));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 原版类型本体
// ---------------------------------------------------------------------------

/// 原版 `GraphwarModuleContainer`：**一个** `TypeId` 索引的模块表。
///
/// 布局（48 字节，全部 `观测`）：
///
/// | 偏移 | 字段 | 依据 |
/// |---|---|---|
/// | `+0x00` | `RawTable.bucket_mask` | `get_module`: `ldp x10, x9, [x19]`（`x10` = ctrl、`x9` = mask） |
/// | `+0x08` | `RawTable.ctrl` | 同上 |
/// | `+0x10` | `RawTable.growth_left` | 探测 / 扩容路径 |
/// | `+0x18` | `RawTable.items` | `get_module`: `ldr x8,[x0,#0x18]; cbz` = 空表快路径 |
/// | `+0x20` | `RandomState`（16 字节） | `get_module`: `add x0, x0, #0x20` 后调 `hash_one` |
///
/// 也就是说：**这里的偏移是 `std::collections::HashMap` 的偏移，不是 manager 的偏移**。
#[derive(Default)]
pub struct GraphwarModuleContainer {
    modules: HashMap<TypeId, Rc<dyn Any>>,
}

impl GraphwarModuleContainer {
    /// 空表。原版没有导出的 `new`（构造点内联在调用者里），这是等价物。
    ///
    /// `Default` 与它等价：`HashMap::new()` 不等于 `HashMap::default()` 的唯一区别
    /// 是前者不做 `RandomState::new()` 的**延迟**初始化，语义一致。
    pub fn new() -> Self {
        Self { modules: HashMap::new() }
    }

    /// `add_module<M>(&mut self, module: Rc<RefCell<M>>)` —— arm64 `0x49e520+212i`，
    /// x86 `0x4694f0+160i`。
    ///
    /// 逐指令形状（x86 `0x4694f0`，`观测`）：
    ///
    /// ```text
    /// incq   (%rsi)                 ; Rc::clone —— 容器自己也要持一份
    /// movups <TypeId>,%xmm0         ; 键 = TypeId::of::<M>()，16 字节
    /// lea    <dyn Any vtable>,%rcx  ; 值 = (data, vtable)
    /// call   hashbrown::HashMap::insert
    ///                               ; 返回旧值 → 立刻 drop（同型重复注册 = 替换）
    /// decq   (%rsi)                 ; 释放入参那一份
    /// ```
    ///
    /// 键是 `TypeId::of::<M>()`、值却是 `RefCell<M>` 的 `dyn Any`
    /// —— 这两个 `TypeId` **不相等**，见 [`GraphwarModuleContainer::get_module`]。
    /// 插入的隐式 unsize（`Rc<RefCell<M>> → Rc<dyn Any>`）正是 vtable 里
    /// `drop` / `size` / `align` 都针对 `RefCell<M>` 的原因。
    pub fn add_module<M: 'static>(&mut self, module: Rc<RefCell<M>>) {
        self.modules.insert(TypeId::of::<M>(), module);
    }

    /// `get_module<M>(&self) -> Option<Rc<RefCell<M>>>` —— arm64 `0x49f9d4+432i`，
    /// x86 `0x46a490+368i`。
    ///
    /// 形状：`items == 0` 快返回 → `hash_one` → 32 字节实体线性探测 →
    /// 命中后 `Rc::clone` → 经 vtable 调 `Any::type_id` →
    /// 与 `TypeId::of::<RefCell<M>>()` 比对 → 相等则 `Rc::downcast::<RefCell<M>>()`。
    ///
    /// 类型不符**只当没找到**（`None`），不 panic：`downcast` 的 `Err` 分支
    /// 在反汇编里就是"丢掉克隆 → 返回 0"。
    pub fn get_module<M: 'static>(&self) -> Option<Rc<RefCell<M>>> {
        let any: &Rc<dyn Any> = self.modules.get(&TypeId::of::<M>())?;
        Rc::clone(any).downcast::<RefCell<M>>().ok()
    }

    /// `get_module_wk<M>(container: &Weak<RefCell<Self>>) -> Option<Weak<RefCell<M>>>`
    /// —— arm64 `0x4a20a4+644i`，x86 `0x46c5a0+544i`。
    ///
    /// 这是**唯一**带 panic 点的那个：`Weak::upgrade` 之后要 `Rc::borrow`，
    /// 借不出就 `panic_already_mutably_borrowed`——`Location` 记成
    /// `graphwar_module_container.rs:30:28`（[`WK_BORROW_LOCATION`]），
    /// 20 个单态化共用这一个锚。这也是"本模块 Location 稀缺"的**原因**。
    ///
    /// 实参为什么是 `&Weak` 而不是 `&self`：调用者（各 manager）自己只握着
    /// `Weak<RefCell<GraphwarModuleContainer>>`（用来打断 container ↔ module 环），
    /// 所以它先 `upgrade`、拿到 `Rc` 再 `borrow`。
    /// 指令证据：函数头 `ldr x20,[x0]; cmn x20,#1; b.eq <None>` 正是
    /// `Weak::inner()` 的 `ptr == usize::MAX` 判据（`Weak::new()` 的哨兵）。
    pub fn get_module_wk<M: 'static>(container: &Weak<RefCell<Self>>) -> Option<Weak<RefCell<M>>> {
        let this = container.upgrade()?; // Weak::inner + strong-count 判据
        let guard = this.borrow(); // ← 唯一的 panic 点（30:28）
        let any: &Rc<dyn Any> = guard.modules.get(&TypeId::of::<M>())?;
        let rc = Rc::clone(any).downcast::<RefCell<M>>().ok()?;
        Some(Rc::downgrade(&rc))
    }

    /// 表里有几个模块（原版：`RawTable.items` 字段 `+0x18`）。
    ///
    /// 原版没有导出它；`get_module` 的 `cbz x8`（`items == 0`）快路径
    /// 等价于 [`GraphwarModuleContainer::is_empty`]。补上是**为了测试可写**，
    /// 见文件头"哪些不是原版的"。
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// 表是否为空。
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// 有没有注册过 `M`（只看键，不 downcast，不 clone）。
    ///
    /// 因为键就是 `TypeId::of::<M>()`，它与 `get_module::<M>().is_some()`
    /// 在本模块里恒等价；留着是为了给不需要 `Rc` 的调用点省一次克隆。
    pub fn contains<M: 'static>(&self) -> bool {
        self.modules.contains_key(&TypeId::of::<M>())
    }
}

#[cfg(test)]
mod smoke {
    //! 只做"类型能编译 + 最基本的往返"检查；判据在 `tests/graphwar_module_container.rs`。
    use super::*;

    #[test]
    fn container_is_48_bytes_and_roundtrips() {
        assert_eq!(std::mem::size_of::<GraphwarModuleContainer>(), CONTAINER_SIZE);
        let mut c = GraphwarModuleContainer::new();
        assert!(c.is_empty());
        let m = Rc::new(RefCell::new(7u8));
        c.add_module(Rc::clone(&m));
        assert_eq!(c.len(), 1);
        assert_eq!(*c.get_module::<u8>().unwrap().borrow(), 7);
        assert!(c.get_module::<u16>().is_none());
    }
}
