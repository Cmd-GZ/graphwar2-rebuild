//! 名字表：`GraphwarNameManager`（`player_id → String` 一张表 + 一条广播）。
//!
//! 本模块回答三件事：**玩家名字存哪**、**`NameInfo`(60) 的载荷到底几个字段**、
//! **改名的入口是谁**（→ 不是本地 setter，是两个远程事件处理器，§4）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律 **vaddr**（Ghidra = vaddr + `0x100000`）。arm64 侧来自未 strip 的
//! 2.0.60 `libgraphwarrust.so` 的 `.symtab`（函数名+长度直接可读）。
//!
//! ## 1. 函数清单（4 个，全部已定位）
//!
//! | 函数 | x86 vaddr / 字节 | arm64 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | **`0x519100`** / 472 | `0x40af68` / 552 | 34 | 订阅 `new_connection_event` + `entity_removed_event` | 逐位（两个闭包；x86 侧任务 74 复核） |
//! | `set_name` | `0x519350` / 256 | `0x40b190` / 264 | 57 | 写表 + 发 `NameInfo`(60) | 逐位 |
//! | `send_all_info_events` | `0x518f10` / 288 | `0x40ad58` / 284 | 69 | 遍历整表逐条发 (60) | 逐位 |
//! | `handle_entity_removed_event` | `0x519030` / 144 | `0x40ae74` / 152 | 79 | 删一条 | 逐位 |
//!
//! `new` 的 2 个闭包在 arm64 上是独立符号（`0x40af0c` / 48 B、`0x40af3c` / 44 B），
//! 两者都用 `adr x1, <handler>` 把方法地址交给 `WeakCaller::call` —— 这就是
//! "哪个事件名接哪个方法"的唯一证据来源（§2）。
//! x86 ÷ arm64 尺寸比：`0.97 / 1.01 / 0.95`，全部落在 0.61–1.19 的闸内。
//!
//! ## 2. 两条订阅（逐位）
//!
//! arm64 `new` 里两次 `subscribe_to_event`，名字都是长度 **20** 的立即数：
//!
//! | 事件名 | 判别值 | 处理函数 | 证据 |
//! |---|---|---|---|
//! | `new_connection_event` | 5 | `send_all_info_events`（闭包 `0x40af3c`） | 逐位 |
//! | `entity_removed_event` | 14 | `handle_entity_removed_event`（闭包 `0x40af0c`） | 逐位 |
//!
//! ## 3. `NameInfo`(60) 的载荷：`String` + `u32`（逐位，两条独立编码互校）
//!
//! arm64 `set_name` 尾段（事件基址 `sp+0x18`）：
//!
//! ```text
//! mov  x9, #0x3c ; movk x9, #0x8000, lsl #48 ; str x9, [sp,#0x18]   ; tag = 60
//! ldr  q0, [x19]        ; 入参 &String 的前 16 字节（cap, ptr）
//! stur q0, [sp, #0x20]  ; → 载荷 +0x00 = cap、+0x08 = ptr
//! ldr  x8, [x19, #0x10] ; len
//! str  x8, [sp, #0x30]  ; → 载荷 +0x10 = len
//! str  w20, [sp, #0x38] ; → 载荷 +0x18 = player_id
//! ```
//!
//! ⇒ 载荷 = `{ String @+0x00（cap/ptr/len）, u32 player_id @+0x18 }`，共 **0x20** 字节，
//! 事件总长 0x28（`Event` 自身是 0x48，其余为零）。
//! 与 `docs/112` §4.4 的"field0 = `String`、field1 = `u32`"逐条吻合。
//!
//! ⚠️ **`set_name` 发的 `String` 是入参，不是表里那份副本**（它先把入参 clone 进表，
//! 之后又从 `x19`（入参）读三次）——所以"写表"和"广播"读的是同一份字节的两份拷贝。
//!
//! ## 4. 改名的入口：两个远程事件处理器（逐位，负结果）
//!
//! arm64 全量扫 `bl 0x40b190`（`set_name`）只有 **2 个调用点**：
//!
//! | 调用点 | 所在函数 | 角色 |
//! |---|---|---|
//! | `0x3e4f50` | `GraphwarRemoteEventHandler::handle_client_remote_event` | 服务端处理客户端消息 |
//! | `0x3e707c` | `GraphwarRemoteEventHandler::handle_server_remote_event` | 客户端处理服务端消息 |
//!
//! 两处都是先 `ldr w24, [<event>+0x20]`（= 载荷里的 `player_id`）再 clone 载荷里的
//! `String`，然后 `set_name(player_id, &name)`。⇒ **`name_manager` 自己没有任何本地
//! setter 调用者**：名字只从远端进来，本模块只负责"存 + 广播"。
//!
//! 与两个名字事件的关系（`docs/07` §7.2.1 已把判别值定死）：
//!
//! * **59 `name_request_event`** = **写入方向**的载体。全 `.text` 只有两处构造：
//!   `local_player_manager::handle_player_info`（本机玩家首次拿到 `player_id` 时，
//!   见 `graphwar_local_player_manager` §5）与 `computer_player_manager::add_computer_player`
//!   （bot 名）。载荷形状与本模块的 (60) **完全相同**：`{String, u32}`。
//! * **60 `name_info_event`** = **本模块的输出**（`set_name` 一条、`send_all_info_events` 全表）。
//!
//! 远端那两个调用点到底比的是 59 还是 60，属于 `remote_event_handler` 的判别表，
//! 本模块无法单独判定 ⇒ **推断**：59 进、60 出（缺的是远端那张分派表的逐位对照）。
//!
//! ## 5. 与 `graphics_manager` 的分工（**这是"名字权威源"问题的答案**）
//!
//! `graphwar_graphics_manager` 的 `+0x18 / +0x48 / +0x78` 三张表**不是玩家名字**：
//! 它们的三对 setter 叫 `set_skin_graphics` / `set_face_graphics` / `set_hat_graphics`，
//! 发的是 51 / 53 / 55 三条 `*_graphics_info_event`，值是**外观资源名**（同样是 `String`）。
//! 三张表与本模块的 `HashMap<u32,String>` 形状一致，这是"看起来像名字"的唯一原因。
//!
//! ⇒ **玩家名字的权威源是 `name_manager`**；显示层（`entity_drawer` /
//! `graphics_manager`）只是订阅 (60) 的消费者。
//!
//! ## 6. 未确定
//!
//! * 远端那两个调用点判的是 59 还是 60（§4 末）。
//! * `handle_entity_removed_event` 里 `find` 用 `self+0x38`、`remove` 用 `self+0x18`
//!   （`self+0x38` 是同一张表的 `RandomState`）——arm64 的 `hash(self+0x38)` 已经把
//!   这个歧义解掉，`docs/112` §4.3 的"两处基址不同"是误读。
//!
//! ## 7. 72 B 布局：`new` 的写指令逐条（任务 74，G1 结案）
//!
//! `docs/161` §4 的 G1（"`+0x38`→`+0x48` 那 8 字节没有写入点 / 基址口径 `0x50` vs `0x58`"）
//! 本轮在 x86 `0x519100` 上逐条读完，**两个问题都是同一个误读**：
//!
//! ```text
//! 0x519126 alloc(size 0x60, align 8)          ← RcInner(16) + RefCell<M>(80)，M = 72
//!   rbx = 盒基：+0x00 strong / +0x08 weak / +0x10 RefCell borrow 标志
//!   M 基 = rbx + 0x18 = RefCell 的 +0x08
//!     M+0x00 ← rbx            0x519183      Weak<RefCell<Self>>（`Rc::new_cyclic_in` 的自句柄）
//!     M+0x08 ← rsi            0x519187      构造第 2 实参
//!     M+0x10 ← rdx            0x51918b      构造第 3 实参
//!     M+0x18 ← [0x961fe8]     0x51919a      ctrl(=0x19c030 空组) + bucket_mask(=0)
//!     M+0x28 ← [0x961ff8]     0x51918f      growth_left(=0) + items(=0)
//!     M+0x38 ← rax(thread_rng) 0x5191a5     RandomState.k0
//!     M+0x40 ← rdx(thread_rng) 0x5191a9     RandomState.k1   ← 上一轮说"没写入"的就是它
//! ```
//!
//! ⇒ ① **基址口径**：盒是 `0x60`（= 16 + 80），不是 `0x50` 也不是 `0x58`；
//! `0x58` 那个数是 `RcInner(16) + 64`，按 64 字节的 manager 猜的，本模块是 72。
//! ② `+0x38` 起那 16 字节是 `RandomState`（两个 `u64`），**不是 8 字节标志 + 8 字节空洞**；
//! 72 B 的每一个字节都有写入点，**没有未解字段**。表 = `+0x18`（ctrl..items 32 B）、
//! hasher = `+0x38`（16 B，`+0x48` = 表尾）。
//!
//! ## 8. 事件流接线（任务 74 就地复核，x86 逐条；`wired` 的第二件）
//!
//! ```text
//! new 0x519100
//!   0x5191f0 lea rsi,[0x1c63fb] 'new_connection_event' + 0x5191fe mov edx,0x14
//!   0x5191f7 lea r8,<0x9627e0>  +0x20 = 0x962800 → 0x5190c0 → lea rsi,[0x518f10]
//!   0x519209 call [0x9bac88] → 0x3d38f0     == subscribe_to_event
//!   0x51923c lea rsi,[0x1c646d] 'entity_removed_event' + 0x51924a mov edx,0x14
//!   0x519243 lea r8,<0x962808>  +0x20 = 0x962828 → 0x5190e0 → lea rsi,[0x519030]
//!   0x519255 call [0x9bac88]
//!
//! 发（两处，tag word 0x8000_0000_0000_003c = 60）
//!   send_all_info_events 0x518f10: 0x518fcd movabs 0x8000…003c ; 0x518fe5 call [0x9bac80] → 0x3d3530
//!   set_name 0x519350: 0x5193af movabs 0x8000…0000 后 +0x3c ⇒ 60 ; 0x5193e8 call [0x9bac80]
//!   handle_entity_removed_event 0x519030: **体内零 `add_event`** ⇒ 删一条不发事件
//! ```
//!
//! ## 9. 复跑
//!
//! ```bash
//! ../.venv/bin/python tools/addr_probe.py dis    0x519100   # 2 条订阅 + 72 B 全部写入点
//! ../.venv/bin/python tools/addr_probe.py dis    0x518f10   # 广播 60
//! ../.venv/bin/python tools/addr_probe.py dis    0x519350   # set_name 60
//! ../.venv/bin/python tools/addr_probe.py dis    0x5190c0   # 跳板 → 0x518f10
//! ../.venv/bin/python tools/addr_probe.py dis    0x5190e0   # 跳板 → 0x519030
//! ```

use crate::events::Event;

/// `name_info_event` 的判别值。
pub const EVENT_NAME_INFO: u16 = 60;
/// `name_request_event` 的判别值（本模块**不发**，见模块文档 §4）。
pub const EVENT_NAME_REQUEST: u16 = 59;
/// `new_connection_event` 的判别值（订阅 1）。
pub const EVENT_NEW_CONNECTION: u16 = 5;
/// `entity_removed_event` 的判别值（订阅 2）。
pub const EVENT_ENTITY_REMOVED: u16 = 14;

/// 名字表在 `self` 里的起始偏移。
pub const NAME_MAP_OFFSET: usize = 0x18;
/// 名字表 hasher 相对表基的偏移（`hash(self + 0x38)`）。
pub const NAME_MAP_HASHER_OFFSET: usize = 0x20;
/// 名字表 entry 的字节数（`shl eax, 5` ⇒ 32 = u32 key + 24 字节 `String` + 对齐）。
pub const NAME_MAP_ENTRY_SIZE: usize = 0x20;
/// `Rc<RefCell<Self>>` 盒的字节数：`0x519126` 的 `alloc(0x60, 8)` = RcInner(16) + RefCell(80)。
pub const RC_BOX_SIZE: usize = 0x60;
/// 自句柄（`Weak<RefCell<Self>>`）在 `Self` 里的偏移（写入点 `0x519183`）。
pub const SELF_WEAK_OFFSET: usize = 0x00;
/// `RandomState` 的字节数 = 两个 `u64`（`+0x38`→`+0x48` 这段**不是**空洞，见模块文档 §7）。
pub const HASHER_SIZE: usize = 0x10;

/// `NameInfo`(60) 载荷里 `String` 的偏移。
pub const NAME_INFO_STRING_OFFSET: usize = 0x00;
/// `NameInfo`(60) 载荷里 `player_id` 的偏移。
pub const NAME_INFO_PLAYER_OFFSET: usize = 0x18;

/// 一条订阅。
#[derive(Debug, Clone, Copy)]
pub struct Subscription {
    /// 事件名。
    pub name: &'static str,
    /// 判别值。
    pub tag: u16,
    /// 处理方法名。
    pub handler: &'static str,
}

/// `GraphwarNameManager::new` 的两条订阅（逐位，见模块文档 §2）。
pub const SUBSCRIPTIONS: [Subscription; 2] = [
    Subscription {
        name: "new_connection_event",
        tag: EVENT_NEW_CONNECTION,
        handler: "send_all_info_events",
    },
    Subscription {
        name: "entity_removed_event",
        tag: EVENT_ENTITY_REMOVED,
        handler: "handle_entity_removed_event",
    },
];

/// `name_info_event`(60) 的载荷（`String` + `u32`，见模块文档 §3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameInfo {
    /// 载荷 `+0x00`：名字（Rust `String`，字段序 cap/ptr/len）。
    pub name: String,
    /// 载荷 `+0x18`：玩家 id。
    pub player_id: u32,
}

impl NameInfo {
    /// 事件判别值。
    pub const TAG: u16 = EVENT_NAME_INFO;
    /// 载荷字节数（24 字节 `String` + 4 字节 id + 4 字节对齐）。
    pub const PAYLOAD_SIZE: usize = 0x20;
    /// 载荷里 `String` 的字节数。
    pub const STRING_SIZE: usize = 24;
    /// 完整 tag word。
    pub const fn tag_word() -> u64 {
        0x8000_0000_0000_0000 | EVENT_NAME_INFO as u64
    }
}

/// `GraphwarNameManager` 的可判定状态：一张 `player_id → 名字` 表。
///
/// 原版是 `HashMap<u32, String>`（entry 0x20 字节）；这里用有序表代替，只为让断言稳定。
///
/// # 事件流（任务 74 补，`wired` 的第二件；见模块文档 §8）
///
/// `new`（x86 `0x519100`）订阅 [`SUBSCRIPTIONS`] 那两条：
/// `new_connection_event`@`0x519209` → 跳板 `0x5190c0` → `0x518f10`（整表重播）；
/// `entity_removed_event`@`0x519255` → 跳板 `0x5190e0` → `0x519030`（删一条）。
/// 发方向两处都在 `add_event`（`0x9bac80`）上：`0x518fe5`（重播）与 `0x5193e8`（`set_name`），
/// tag 都是 `0x8000_0000_0000_003c`(60)；`handle_entity_removed_event` 体内**零** `add_event`。
///
/// 发件箱只记判别值（[`Event::NameInfo`]），载荷仍由 [`NameInfo`] 返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphwarNameManager {
    /// `HashMap<u32, String>`（`self + 0x18`）。
    pub names: std::collections::BTreeMap<u32, String>,
    /// 订阅名（`new` 的两条 `subscribe`，顺序 = 调用序，`观测`）。
    pub subscribed: Vec<&'static str>,
    /// 发件箱（判别值；载荷见 [`NameInfo`]）。
    pub outbox: Vec<Event>,
}

impl Default for GraphwarNameManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphwarNameManager {
    /// 新建空管理器 + 订阅 [`SUBSCRIPTIONS`] 那两条（原版 `new` 做的同一件事）。
    pub fn new() -> Self {
        Self {
            names: std::collections::BTreeMap::new(),
            subscribed: SUBSCRIPTIONS.iter().map(|s| s.name).collect(),
            outbox: Vec::new(),
        }
    }

    /// `set_name(player_id, &name)`：写表 + 发一条 (60)。
    ///
    /// 逐位要点（模块文档 §3）：先 `clone` 入参写表（旧值立刻 `dealloc`），
    /// 再从**入参**取三字段造事件 —— 不是从表里那份副本取。
    pub fn set_name(&mut self, player_id: u32, name: &str) -> NameInfo {
        self.names.insert(player_id, name.to_string());
        self.outbox.push(Event::NameInfo);
        NameInfo { name: name.to_string(), player_id }
    }

    /// `new_connection_event` 的处理（跳板 `0x5190c0` → `0x518f10`）：**整表重播**。
    ///
    /// 返回值 = 重播出去的载荷（顺序 = 原版 `HashMap` 迭代序，不可复现）。
    pub fn handle_new_connection_event(&mut self) -> Vec<NameInfo> {
        let events = self.send_all_info_events();
        self.outbox.extend(events.iter().map(|_| Event::NameInfo));
        events
    }

    /// `send_all_info_events`：整表逐条发 (60)。
    ///
    /// 原版遍历顺序由 `HashMap` 的 `RandomState` 决定 ⇒ **不可复现**；本实现用有序表
    /// 只为让断言稳定（**不代表**原版顺序，标推断）。
    pub fn send_all_info_events(&self) -> Vec<NameInfo> {
        self.names
            .iter()
            .map(|(id, name)| NameInfo { name: name.clone(), player_id: *id })
            .collect()
    }

    /// `handle_entity_removed_event`：从表里删掉该玩家，返回是否删到了。
    ///
    /// 逐位：`0x519030` 体内**没有** `add_event` ⇒ 这条处理器不发事件（只删表）。
    pub fn handle_entity_removed_event(&mut self, player_id: u32) -> bool {
        self.names.remove(&player_id).is_some()
    }

    /// 取走发件箱（与原版 `GraphwarEventQueue` 的"取空"同口径）。
    pub fn take_outbox(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }

    /// 查名字（表里没有就是 `None`）。
    pub fn name_of(&self, player_id: u32) -> Option<&str> {
        self.names.get(&player_id).map(|s| s.as_str())
    }

    /// 表里有多少人。
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// 表是否为空。
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn name_info_is_string_then_u32() {
        assert_eq!(NameInfo::PAYLOAD_SIZE, 0x20);
        assert_eq!(NameInfo::STRING_SIZE, 24);
        assert_eq!((NAME_INFO_STRING_OFFSET, NAME_INFO_PLAYER_OFFSET), (0, 0x18));
        assert_eq!(NameInfo::tag_word(), 0x8000_0000_0000_003c);
    }

    #[test]
    fn set_then_remove() {
        let mut m = GraphwarNameManager::new();
        let e = m.set_name(7, "Ann");
        assert_eq!(e.name, "Ann");
        assert_eq!(e.player_id, 7);
        assert_eq!(m.name_of(7), Some("Ann"));
        assert!(m.handle_entity_removed_event(7));
        assert_eq!(m.name_of(7), None);
        assert!(!m.handle_entity_removed_event(7));
    }

    /// 72 B 布局的三个结论（模块文档 §7，G1 结案）：盒 0x60、自句柄 `+0x00`、
    /// hasher **16** 字节 ⇒ `+0x18` 表 + 32 B + `+0x38` hasher 16 B = 72，没有空洞。
    #[test]
    fn layout_covers_all_seventy_two_bytes() {
        assert_eq!(RC_BOX_SIZE, 0x60);
        assert_eq!(RC_BOX_SIZE, 16 + 8 + 72, "RcInner(16) + borrow 标志(8) + M(72)");
        assert_eq!(SELF_WEAK_OFFSET, 0);
        assert_eq!(NAME_MAP_OFFSET, 0x18);
        assert_eq!(NAME_MAP_HASHER_OFFSET, 0x20);
        assert_eq!(NAME_MAP_OFFSET + 0x20, 0x38, "hasher 在 self+0x38");
        assert_eq!(HASHER_SIZE, 0x10);
        assert_eq!(0x38 + HASHER_SIZE, 72, "hasher 结束 = 结构体尾（没有别的字段）");
    }

    /// `new` 的两条订阅在代码侧落地（顺序 = `0x519209` 后 `0x519255`）。
    #[test]
    fn new_registers_the_two_subscriptions() {
        let m = GraphwarNameManager::new();
        assert_eq!(m.subscribed, vec!["new_connection_event", "entity_removed_event"]);
        assert_eq!(m.subscribed.len(), SUBSCRIPTIONS.len());
        assert!(m.outbox.is_empty());
    }

    /// 发方向两处：`set_name` 一条 (60)、`handle_new_connection_event` 按表长重播，
    /// `handle_entity_removed_event` **一条都不发**（`0x519030` 体内零 `add_event`）。
    #[test]
    fn outbox_tracks_the_two_emitters() {
        let mut m = GraphwarNameManager::new();
        m.set_name(7, "Ann");
        assert_eq!(m.take_outbox(), vec![Event::NameInfo]);

        let replayed = m.handle_new_connection_event();
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0], NameInfo { name: "Ann".into(), player_id: 7 });
        assert_eq!(m.take_outbox(), vec![Event::NameInfo]);

        assert!(m.handle_entity_removed_event(7));
        assert!(m.outbox.is_empty(), "删一条不发事件（0x519030 无 add_event）");
    }
}
