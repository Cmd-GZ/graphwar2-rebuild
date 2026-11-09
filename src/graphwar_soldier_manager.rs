//! 士兵管理：`soldier_id → owner_id` 一张两列表、三条广播事件、
//! 以及"玩家没了 ⇒ 他的士兵实体也发一条删除"的**级联**。
//!
//! 本模块回答：**`soldier_manager` 到底管什么**、它跟 `graphwar_turn_manager`
//! 的**游标轮转**怎么衔接、士兵死了/玩家走了分别走哪条路。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单
//!
//! | 函数 | x86 vaddr | arm64 vaddr / 长度 | 职责 | 等级 |
//! |---|---|---|---|---|
//! | `add_soldier` | **内联进 `0x474a10`**（正文 `0x474b38`–`0x474bab`，见 §6-A） | `0x49d2fc` / 144 B | 往表里写一条 + **发 `SoldierInfo`(15)** | 观测（x86 内联块 + Location `:65:26`） |
//! | `send_all_info_events` | `0x455570` | `0x49d38c` / 236 B | 整表重发 `SoldierInfo` | 观测 |
//! | `get_soldiers_from_owner` | `0x455660`（**推断**） | `0x49d478` / 92 B | 按 owner 取士兵 | 推断 |
//! | `handle_entity_removed_event` | `0x4556d0` | `0x49d4d4` / 96 B | 断言 tag=14 → 级联删除 | 观测 |
//! | `new` | `0x455760` | `0x49d590` / 396 B | 订阅 `new_connection_event` / `entity_removed_event` | 观测 |
//! | 两个 `{{closure}}` | `0x455720` / `0x455740` | `0x49d534` / `0x49d560`（44 / 48 B） | 转发到上面两个方法 | 观测 |
//! | （级联实现） | `0x444fd0` | arm64 未见同名符号（内联） | 遍历表、命中即播 `EntityRemoved` | 观测 |
//!
//! `send_all_info_events`（`:87:30`）、`handle_entity_removed_event`（`:105:18`）、
//! `new`（`:35:25`）三个都有**唯一 `panic::Location` 锚**，
//! 且各自的反编译里指到 `src/graphwar_soldier_manager.rs` 的**三个不同**字符串
//! （`0x956950` / `0x956968` / `0x9569d0`）——所以这三个的归属不是猜的。
//!
//! ## 2. 数据结构：`Rc<RefCell<{ +0x00 弱自句柄 / +0x08 事件通道 / +0x10 容器弱引用 / +0x18 表 }>>`
//!
//! > **2026-11-09（任务 77）就地更正**：本节此前写「`+0x10/+0x14` 是锁字与 poison 字节」。
//! > **没有锁**：`+0x10` 是 `RefCell` 的借用字（`0` = 未借、`-1` = 写借）。
//! > 旧说法是把 `0x4555d9 cmp [r12+0x10],0` 误读成了"加锁"。
//!
//! 三处独立读法互相吻合（逐位）：
//!
//! ```text
//! new 0x455760 的建盒 0x522890：alloc 0x60 = RcInner 16 + RefCell 8 + 72
//!     M 基 = 盒 + 0x18；send_all_info_events 0x455570：
//!       0x45557e mov r15,[rdi+0x30]   ; 表 items（M+0x18 表的 +0x18）
//!       0x45558b mov r12,[rdi+8]      ; 事件通道 Rc（M+0x08）
//!       0x45558f mov r13,[rdi+0x18]   ; 表 ctrl   （M+0x18）
//!     add_soldier 的内联块 0x474b38–0x474bab（r14 = 盒）：
//!       0x474b3d cmp [r14+0x10],0 / 0x474b48 mov [r14+0x10],-1  ; RefCell 写借
//!       0x474b5d lea rdi,[r14+0x30]   ; 表基 = 盒+0x30 = M+0x18
//!       0x474b6a mov r15,[r14+0x20]   ; 事件通道 = 盒+0x20 = M+0x08
//! ```
//!
//! **桶步长同样是 8 字节，值是 4 字节**：
//!
//! ```text
//! send_all_info_events（0x555570）每个桶取一整条 8 字节：
//!     local_70 = *(u64 *)(bucket)        ; = { key: u32, value: u32 }
//!     tag      = 0x800000000000000f      ; = soldier_info_event (15)
//!     send( tag, local_70 )              ; ⇒ 载荷就是把桶原样搬过去
//! ```
//!
//! 一次 8 字节拷贝就够 ⇒ 事件的字段顺序就是 `(key, value)`，中间没有换位。
//! 也就是说源码大概是
//!
//! ```text
//! for (a, b) in &self.<map> { send(SoldierInfoEvent { a: *a, b: *b }) }
//! ```
//!
//! 被编译器合成成了一条 `mov r64, [bucket]`。
//!
//! ## 3. 键值极性：`soldier_id → owner_id`（**观测**，任务 77 结案）
//!
//! `add_soldier` 的内联块把两个值同时说清楚了：
//!
//! ```text
//! 0x474b50 mov ebx,1 ; 0x474b55 lock xadd [0x9c10c0], ebx   ; 全局原子计数器取号 ⇒ 新的**士兵 id**
//! 0x474b61 mov esi,ebx                                      ; insert 的 key    = 新兵 id
//! 0x474b63 mov edx,ebp                                      ; insert 的 value  = 传入的 owner
//! 0x474b65 call 0x442330                                    ; hashbrown insert（表 = 盒+0x30 = M+0x18）
//! 0x474b85 mov [rsp+0x30], ebx  /  0x474b89 mov [rsp+0x34], ebp   ; 事件载荷 +0x08 / +0x0c
//! ```
//!
//! ⇒ 键是**刚发号的士兵实体**、值是**调用方给的 owner** = "士兵 → 主人"（与 §4 的级联方向一致，
//! 两条独立证据同向）。

//!
//! ## 4. 级联：`EntityRemoved{owner}` ⇒ 每个士兵也发一条 `EntityRemoved`（观测）
//!
//! `handle_entity_removed_event`（`0x4556d0`）只做一件事：把
//! `(表, &entity_id, 事件总线)` 交给 `0x444fd0`。后者遍历整表：
//!
//! ```text
//! for (k, v) in &map {
//!     if v == removed_entity {                       // 比的是 **value**
//!         send(tag = 0x800000000000000e /* entity_removed */, entity_id = k);
//!     }
//! }
//! ```
//!
//! 比的是**值**、发出去的 `entity_id` 是**键**。所以：
//!
//! * 键 = 士兵实体号、值 = 玩家实体号 ⇒ "**玩家被删 ⇒ 他的士兵也各发一条删除**"，
//!   语义成立；反过来（键=玩家、值=士兵）会变成"士兵死了 ⇒ 玩家被删"，不成立。
//! * 这条也在攻略上有用：**玩家退房后，他的士兵不是"变成尸体"而是"实体消失"**。
//! * 级联发出去的事件再回来时值不匹配（值仍是玩家号），所以**不会自激**——这一点
//!   是读代码得出的，没有运行时验证（推断）。
//!
//! ## 5. 与 `graphwar_turn_manager` 的衔接
//!
//! 两个模块**各存一份、互不引用**：
//!
//! | | `soldier_manager` | `turn_manager`（`docs/98`、`docs/14`） |
//! |---|---|---|
//! | 表 | `HashMap<u32, u32>`（士兵 → 主） | `HashMap<u32, (Vec<u32>, usize)>`（玩家 → 名单 + 游标） |
//! | 桶步长 | **8**（两列） | **0x28 = 40**（4 键 + 24 `Vec` + 8 游标 + 填充） |
//! | 谁写它 | `add_soldier` / 级联删除 | 建局时按 `SoldierInfo` 创建顺序 push |
//! | 用途 | "这个士兵是谁的"、"整表重发给新连接" | "下一个谁开火" |
//!
//! ⇒ **轮转不查 `soldier_manager`**：`turn_manager` 自己那份 `Vec<soldier_id>` 就是
//! 全部输入（`docs/98` §1.4 的 `0x463da0` 只调 `life_manager` 判活，没有一次调
//! 本模块）。所以"某个士兵从 `soldier_manager` 里被删掉"**不会**自动把它从
//! `turn_manager` 的轮转名单里摘掉；两份表的一致性由**共同的 `EntityRemoved` 流**
//! 维持（`entity_removed_event` 三个 manager 都订阅）。
//!
//! ## 6. 仍未确定
//!
//! | # | 未知 | 缺什么 |
//! |---|---|---|
//! | ~~A~~ **已结案（任务 77）** | `add_soldier` 的 x86 地址 | **它没有独立 x86 函数**：内联进 `GraphwarGameStateManager::add_soldier_to_player`（x86 `0x474a10`），正文 `0x474b38`–`0x474bab`。判据 ① 该函数就是调 `set_skin/face/hat_graphics`（`0x474d91`/`0x474eca`/`0x47500c`）的那个（arm64 `add_soldier_to_player` 0x4aaab4 同三个被调者）② `addr_probe.py locs 0x474a10` 命中 **`src/graphwar_soldier_manager.rs:65:26`** 一条（其余 17 条是 game_state_manager）③ 体内 `0x474b5d` 的表基/`0x474b6a` 的事件通道与 §2 的 `M+0x18`/`M+0x08` 逐字对上 |
//! | ~~B~~ **已结案（任务 77）：会发** | `add_soldier` 有没有广播 `SoldierInfo`(15) | **发**。`0x474b8d movabs 0x8000000000000000` + `0x474b97 add rax,0xf` = tag 15 → `0x474ba5 call [0x9bac80]`（add_event）。**这正是 `addr_probe.py tags` 漏掉的第四种 materialize 形态**（基址 + `add`），它被记到 tag 0 的桶里（2235 处），所以旧普查只看到 `send_all_info_events` 的 1 处 |
//! | C | `get_soldiers_from_owner` 的返回形态 | 名字是复数，但表只有两列、一次查找只能返回一个 `u32`；若源码是扫描，函数体应远大于 92 B。**冲突未解** |
//! | ~~D~~ **已结案（任务 77）** | 表里键值极性的**直接**证据 | 两条独立证据同向：`0x474b50` 原子计数器发的新号当 **key**、`ebp`（传入的 owner）当 **value**（§3）；级联方向（§4）也只在"键=士兵"时语义成立 |
//! | E | 士兵**出生**由谁发起 | `game_state_manager` 的 `0x474a10` 一带会克隆一名玩家的士兵 `Vec` 给新实体号；出生流程不在本模块 |

//! ## 7. 事件流接线（任务 77 逐条复核，x86；`wired` 的第二件）
//!
//! ```text
//! new 0x455760
//!   0x4557cc lea rsi,[0x1c0eef] 'new_connection_event' + 0x4557da mov edx,0x14
//!   0x4557d3 lea r8,<0x956980>   +0x20 = 0x455740 → lea rsi,[0x455570] send_all_info_events
//!   0x4557e5 call [0x9bac88] → 0x3d38f0     == subscribe_to_event
//!   0x45581d lea rsi,[0x1c0e12] 'entity_removed_event' + 0x45582b mov edx,0x14
//!   0x455824 lea r8,<0x9569a8>   +0x20 = 0x455720 → lea rsi,[0x4556d0] handle_entity_removed_event
//!   0x455836 call [0x9bac88] → 0x3d38f0
//!
//! 发（三处，两个 tag）
//!   send_all_info_events 0x455570: 0x455600 movabs …0f ; 0x455616 call [0x9bac80] → 0x3d3530
//!   add_soldier（内联）  0x474a10: 0x474b8d + 0x474b97（基址+0xf = 15）; 0x474ba5 call [0x9bac80]
//!   级联 0x444fd0（由 0x4556d0 调用）: 发 tag 14（`EntityRemoved`）
//! ```
//!
//! 闭包槽 `+0x20` 的两条跳板是现成的正对照：`0x455740` 体内唯一一条 `lea rsi` 指
//! `0x455570`、`0x455720` 指 `0x4556d0` —— 所以处理器不是靠"名字像"猜的。
//!
//! ## 8. 复跑
//!
//! ```bash
//! ../.venv/bin/python tools/addr_probe.py dis 0x455760   # 2 条订阅（闭包槽 0x956980 / 0x9569a8）
//! ../.venv/bin/python tools/addr_probe.py dis 0x455570   # 整表重播 15（0x455600 / 0x455616）
//! ../.venv/bin/python tools/addr_probe.py dis 0x474a10   # add_soldier 的内联块（0x474b38–0x474bab）
//! ../.venv/bin/python tools/addr_probe.py locs 0x474a10  # 命中 src/graphwar_soldier_manager.rs:65:26
//! ../.venv/bin/python tools/addr_probe.py dis 0x4556d0   # 级联入口 → 0x444fd0
//! ```

use std::collections::HashMap;

/// `soldier_info_event` 的判别值（与 `src/events.rs` 一致）。
pub const EVENT_SOLDIER_INFO: u8 = 15;
/// `entity_removed_event` 的判别值（`handle_entity_removed_event` 断言它）。
pub const EVENT_ENTITY_REMOVED: u8 = 14;

/// `soldier_info_event` 的 tag 字面量（`send_all_info_events` 里的立即数）。
pub const TAG_SOLDIER_INFO: u64 = 0x8000_0000_0000_000f;
/// `entity_removed_event` 的 tag 字面量（级联里发出的正是它）。
pub const TAG_ENTITY_REMOVED: u64 = 0x8000_0000_0000_000e;

/// `GraphwarSoldierManager::new` 订阅的两条事件（名字, 名字字节数）。
pub const SUBSCRIBED_EVENTS: [(&str, usize); 2] = [
    ("new_connection_event", 20),
    ("entity_removed_event", 20),
];

/// 一条要广播出去的士兵事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoldierEvent {
    /// `soldier_info_event`：载荷就是表里那一对 `(键, 值)`。
    ///
    /// 字段名按 `src/events.rs` 记作 `{ player_id, soldier_id }`；**哪个是键未定**
    /// （见模块文档 §6-D），所以这里用中性名 `first/second` 以免编造。
    SoldierInfo { first: u32, second: u32 },
    /// `entity_removed_event`：`{ entity_id }`。
    EntityRemoved { entity_id: u32 },
}

impl SoldierEvent {
    /// 线上 tag 字面量。
    pub const fn tag(&self) -> u64 {
        match self {
            SoldierEvent::SoldierInfo { .. } => TAG_SOLDIER_INFO,
            SoldierEvent::EntityRemoved { .. } => TAG_ENTITY_REMOVED,
        }
    }

    /// 载荷字节数（不含 tag）。
    pub const fn payload_len(&self) -> usize {
        4 * match self {
            SoldierEvent::SoldierInfo { .. } => 2,
            SoldierEvent::EntityRemoved { .. } => 1,
        }
    }
}

/// `soldier_id → owner_id` 那张表（原版在 `Rc<RefCell<_>>` 里，这里由调用方决定借用）。
///
/// # 事件流（任务 77 补，`wired` 的第二件；见模块文档 §7）
///
/// `new`（x86 `0x455760`）订阅 [`SUBSCRIBED_EVENTS`] 那两条：
/// `new_connection_event`@`0x4557e5` → 闭包槽 `0x956980`（`+0x20` = `0x455740`）→ `0x455570`；
/// `entity_removed_event`@`0x455836` → 闭包槽 `0x9569a8`（`+0x20` = `0x455720`）→ `0x4556d0`。
/// 发方向：`send_all_info_events` 每行一条 (15)（`0x455600`/`0x455616`）、`add_soldier`
/// 也发一条 (15)（内联块 `0x474b8d`+`0x474b97` → `0x474ba5`）、级联发 (14)（`0x444fd0`）。
#[derive(Debug, Default, Clone)]
pub struct SoldierTable {
    owners: HashMap<u32, u32>,
    /// 订阅名（`new` 的两条 `subscribe`，顺序 = 调用序，`观测`）。
    pub subscribed: Vec<&'static str>,
    /// 发件箱（载荷见 [`SoldierEvent`]；本模块的三处发送都记成它）。
    ///
    /// 用**本模块自己的**枚举而不是 `crate::events::Event`，理由与
    /// [`TeamTable`](crate::graphwar_team_manager::TeamTable) 同款：`tests/graphwar_soldier_manager.rs`
    /// 把本文件 `#[path]` 编进测试二进制时**不**带 `src/events.rs`（本分支不许改 `tests/`），
    /// 所以这里不能引用 `crate::`。`SoldierEvent::tag()` 给出的仍是线上 tag word。
    pub outbox: Vec<SoldierEvent>,
}

impl SoldierTable {
    /// 空表 + 两条订阅（原版 `new` 做的同一件事）。
    pub fn new() -> Self {
        Self {
            owners: HashMap::new(),
            subscribed: SUBSCRIBED_EVENTS.iter().map(|(name, _)| *name).collect(),
            outbox: Vec::new(),
        }
    }

    /// `add_soldier`：写一行并**发一条 `SoldierInfo`(15)**。
    ///
    /// 逐位（任务 77 结案，见模块文档 §6-B）：x86 侧没有独立函数，正文内联在
    /// `GraphwarGameStateManager::add_soldier_to_player`（`0x474a10`）的
    /// `0x474b38`–`0x474bab`：写表 `0x474b65`、发 (15) `0x474b8d`+`0x474b97` → `0x474ba5`。
    /// 载荷就是 `(新兵 id, owner)` 这一对（`+0x08` / `+0x0c`）。
    pub fn add_soldier(&mut self, soldier_id: u32, owner_id: u32) -> Vec<SoldierEvent> {
        self.owners.insert(soldier_id, owner_id);
        let event = SoldierEvent::SoldierInfo { first: soldier_id, second: owner_id };
        self.outbox.push(event);
        vec![event]
    }

    /// 静默写一行（不给事件）——给"照抄表内容"的调用方用。
    pub fn set_silent(&mut self, soldier_id: u32, owner_id: u32) {
        self.owners.insert(soldier_id, owner_id);
    }

    /// `get_soldiers_from_owner`（推断地址 `0x455660`）：扫描取值等于 `owner_id` 的键。
    ///
    /// 顺序来自 `RandomState` 哈希 ⇒ 每进程不同（见模块文档 §2）。
    pub fn get_soldiers_from_owner(&self, owner_id: u32) -> Vec<u32> {
        let mut out: Vec<u32> = self
            .owners
            .iter()
            .filter(|(_, &owner)| owner == owner_id)
            .map(|(&soldier, _)| soldier)
            .collect();
        out.sort_unstable();
        out
    }

    /// 单个士兵的主人（`None` = 表里没有这个士兵）。
    pub fn owner_of(&self, soldier_id: u32) -> Option<u32> {
        self.owners.get(&soldier_id).copied()
    }

    /// `send_all_info_events`（`0x455570`）：整表重发，**不重新分配**。
    ///
    /// ⚠️ 顺序来自 `RandomState`，不要按序断言。
    pub fn send_all_info_events(&self) -> Vec<SoldierEvent> {
        self.owners
            .iter()
            .map(|(&first, &second)| SoldierEvent::SoldierInfo { first, second })
            .collect()
    }

    /// `new_connection_event` 的处理（闭包槽 `0x956980` → `0x455740` → `0x455570`）：整表重播。
    ///
    /// 原版每遍历到一行就 `add_event` 一条 (15)（`0x455600`/`0x455616`）；本实现把
    /// 同样条数记进发件箱并返回载荷。
    pub fn handle_new_connection_event(&mut self) -> Vec<SoldierEvent> {
        let events = self.send_all_info_events();
        self.outbox.extend(events.iter().copied());
        events
    }

    /// `handle_entity_removed_event`（`0x4556d0` + `0x444fd0`）：级联。
    ///
    /// 值等于被删实体的每一行，都把它的**键**作为 `EntityRemoved` 再播一次；
    /// 行本身**不在这里删**（反编译里那段没有 `erase`，`0x444fd0` 只发事件）。
    pub fn handle_entity_removed_event(&self, entity_id: u32) -> Vec<SoldierEvent> {
        let mut out: Vec<SoldierEvent> = self
            .owners
            .iter()
            .filter(|(_, &owner)| owner == entity_id)
            .map(|(&soldier, _)| SoldierEvent::EntityRemoved { entity_id: soldier })
            .collect();
        out.sort_unstable_by_key(|e| match e {
            SoldierEvent::EntityRemoved { entity_id } => *entity_id,
            _ => 0,
        });
        out
    }

    /// `handle_entity_removed_event` 的**记录版**（同一段 `0x4556d0` + `0x444fd0`）：
    /// 级联出去的每条 `EntityRemoved` 同时进发件箱。
    ///
    /// 为什么有两个入口：`tests/graphwar_soldier_manager.rs` 用 `&self` 的纯函数版
    /// （它把本文件 `#[path]` 编进测试二进制，**本分支不许改 `tests/`**）；接线层的断言
    /// 要"确实发了 N 条"，所以多一个 `&mut` 的记录口。两版**返回同一组事件**。
    pub fn handle_entity_removed_event_emitting(&mut self, entity_id: u32) -> Vec<SoldierEvent> {
        let out = self.handle_entity_removed_event(entity_id);
        self.outbox.extend(out.iter().copied());
        out
    }

    /// 取走发件箱（与原版 `GraphwarEventQueue` 的"取空"同口径）。
    pub fn take_outbox(&mut self) -> Vec<SoldierEvent> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }

    /// 表长（= 已知士兵数）。
    pub fn len(&self) -> usize {
        self.owners.len()
    }

    /// 表是否为空。
    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 两列表：写进去能按主人取回来。
    #[test]
    fn owner_lookup_round_trip() {
        let mut table = SoldierTable::new();
        table.set_silent(100, 1);
        table.set_silent(101, 1);
        table.set_silent(200, 2);
        assert_eq!(table.get_soldiers_from_owner(1), vec![100, 101]);
        assert_eq!(table.get_soldiers_from_owner(2), vec![200]);
        assert!(table.get_soldiers_from_owner(3).is_empty());
        assert_eq!(table.owner_of(101), Some(1));
        assert_eq!(table.owner_of(999), None);
        assert_eq!(table.len(), 3);
    }

    /// 级联：玩家没了 ⇒ 他的每个士兵各发一条 `EntityRemoved`，**玩家自己不发**。
    #[test]
    fn removing_an_owner_cascades_to_his_soldiers() {
        let mut table = SoldierTable::new();
        table.set_silent(100, 1);
        table.set_silent(101, 1);
        table.set_silent(200, 2);
        assert_eq!(
            table.handle_entity_removed_event(1),
            vec![
                SoldierEvent::EntityRemoved { entity_id: 100 },
                SoldierEvent::EntityRemoved { entity_id: 101 },
            ]
        );
        // 拿一个士兵号当输入：没有谁的主人是它 ⇒ 不发（⇒ 不会自激）
        assert!(table.handle_entity_removed_event(100).is_empty());
        assert_eq!(table.len(), 3, "级联不发 erase，行还在（观测）");
    }

    /// 整表重发 = 每个桶一行；tag 与载荷长度钉死。
    #[test]
    fn send_all_info_emits_one_pair_per_row() {
        let mut table = SoldierTable::new();
        table.set_silent(5, 1);
        table.set_silent(6, 1);
        let events = table.send_all_info_events();
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.tag() == TAG_SOLDIER_INFO));
        assert_eq!(TAG_SOLDIER_INFO & 0xff, EVENT_SOLDIER_INFO as u64);
        assert_eq!(TAG_ENTITY_REMOVED & 0xff, EVENT_ENTITY_REMOVED as u64);
        assert_eq!(
            SoldierEvent::SoldierInfo {
                first: 0,
                second: 0
            }
            .payload_len(),
            8
        );
        assert_eq!(SoldierEvent::EntityRemoved { entity_id: 0 }.payload_len(), 4);
    }

    /// `new` 订阅的两条事件，名字与长度对得上。
    #[test]
    fn subscription_names_and_lengths() {
        for (name, len) in SUBSCRIBED_EVENTS {
            assert_eq!(name.len(), len, "{name}");
        }
    }

    /// 订阅已在 `new` 里落码（`wired` 的第一件）；发件箱起点是空的。
    #[test]
    fn new_subscribes_the_two_events() {
        let table = SoldierTable::new();
        assert_eq!(table.subscribed, vec!["new_connection_event", "entity_removed_event"]);
        assert_eq!(table.subscribed.len(), SUBSCRIBED_EVENTS.len());
        assert!(table.outbox.is_empty());
        assert!(table.is_empty());
    }

    /// 发方向：`add_soldier` 发一条 (15)（内联块 `0x474b8d`），整表重播 N 条，
    /// 级联发 (14)——这三条是**三处不同**的 `add_event`，条数必须各自对得上。
    #[test]
    fn outbox_tracks_the_three_emitters() {
        let mut table = SoldierTable::new();
        table.add_soldier(100, 1);
        assert_eq!(
            table.take_outbox(),
            vec![SoldierEvent::SoldierInfo { first: 100, second: 1 }]
        );
        table.set_silent(101, 1);
        table.set_silent(200, 2);
        assert!(table.outbox.is_empty(), "set_silent 不发事件");
        // 整表重播：三行 ⇒ 三条 (15)
        assert_eq!(table.handle_new_connection_event().len(), 3);
        assert_eq!(table.outbox_len(), 3);
        let _ = table.take_outbox();
        // 级联：玩家 1 名下有 100/101 ⇒ 两条 (14)
        let cascaded = table.handle_entity_removed_event_emitting(1);
        assert_eq!(cascaded.len(), 2);
        assert_eq!(
            table.outbox,
            vec![
                SoldierEvent::EntityRemoved { entity_id: 100 },
                SoldierEvent::EntityRemoved { entity_id: 101 },
            ]
        );
        // 拿士兵号当输入 ⇒ 没有谁的主人是它 ⇒ 一条都不发
        let _ = table.take_outbox();
        assert!(table.handle_entity_removed_event_emitting(100).is_empty());
        assert_eq!(table.outbox_len(), 0);
    }
}
