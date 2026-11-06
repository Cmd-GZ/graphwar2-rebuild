//! 玩家条目：`GraphwarPlayerManager`（`player_id → connection_id` 一张表 + 两条事件）。
//!
//! 本模块回答三件事：**"玩家 ↔ 连接"这张表长什么样**、**`PlayerInfo`(13) 的载荷**、
//! **一个连接的最后一个人被删掉时会发生什么**（→ 级联一条 `ConnectionRemoved`(6)，§4）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（4 个，全部已定位）
//!
//! | 函数 | x86 vaddr / 字节 | arm64 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x455410` / 237 | `0x49d170` / 396 | 36 | 订阅 `new_connection_event` + `entity_removed_event` | 逐位（两个闭包） |
//! | `add_player` | **已内联**（无独立符号） | `0x49ce1c` / 148 | 66, 74 | 写表 + 发 `PlayerInfo`(13) | 逐位 |
//! | `send_all_info_events` | `0x455160` / 207 | `0x49ceb0` / 240 | 91 | 遍历整表逐条发 (13) | 逐位 |
//! | `handle_entity_removed_event` | `0x455250` / 294 | `0x49cfa0` / 372 | 120, 124 | 删一条 + 判"该连接是否已无人" | 逐位 |
//!
//! **这就是本批"未知"那个模块的答案：它有 4 个方法，不是 3 个。**
//! `add_player` 在 x86 侧被内联进 `remote_event_handler`（`panic::Location`
//! `src/graphwar_player_manager.rs:66:26` / `:74:39` 只出现在**宿主函数**里：
//! `0x3e2830`、`0x4739c0`、`0x4500c0`、`0x477140`），所以 `docs/02` §18.1 的
//! "3 个方法"漏了它；arm64 因为未 strip，`GraphwarPlayerManager::add_player` 是独立符号。
//! 这也解释了 `docs/02` §18.1 为什么给 player_manager 的行范围是 `36–124（4）`——
//! 锚点 4 个（36/91/120/124），而 66/74 那两个落到了别的函数的体内。
//!
//! ## 2. 数据形状（逐位）
//!
//! 一张表，在 `self + 0x18`，hashbrown 步长 8 ⇒ `HashMap<u32, u32>`：
//!
//! ```text
//! +0x18 ctrl        +0x20 bucket_mask
//! +0x28 growth_left +0x30 items
//! +0x38 RandomState            ← hasher 在表基 + 0x20
//! ```
//!
//! `handle_entity_removed_event` 里 `hash(self + 0x38)` + `remove_entry(self + 0x18, ..)`
//! 又一次给出 `docs/10` §14.3 那条"一张表，不是两张"的同一形状
//! （`self+0x38` 是**同一张表的 hasher**，不是第二张表）。
//!
//! ## 3. `PlayerInfo`(13) 的载荷：两个 u32（逐位）
//!
//! arm64 `add_player`（事件基址 `sp+0x8`）：
//!
//! ```text
//! stp w20, w19, [sp, #0x10]     ; +0x08 = w1（player_id）、+0x0c = w2（connection_id）
//! mov x8, #0xd ; movk x8, #0x8000, lsl #48 ; str x8, [sp,#0x8]   ; tag = 13
//! ```
//!
//! 参数顺序（`w1` = 第一个 u32，`w2` = 第二个 u32）由 `self+0x18` 表的 key 决定：
//! `insert(self + 0x18, w1, w2)` ⇒ **key = `player_id`、value = `connection_id`**。
//! `send_all_info_events` 侧把 entry 的 8 字节整块 `ldur d0, [.., #-0x8]` 拷进载荷，
//! 顺序与表的 (key, value) 一致 —— 两条独立路径给出同一个 (player_id, connection_id) 次序。
//!
//! ⇒ 载荷 = `{ player_id @+0x08, connection_id @+0x0c }`，共 **8** 字节。
//!
//! 与 `local_player_manager` 的对应关系由此闭合：那边 `handle_player_info` 读的是
//! `event+8`（`player_id`，往 `Vec` 里 push）和 `event+0xc`（`connection_id`，与自己比）。
//!
//! ## 4. `handle_entity_removed_event`：删一条 + **级联**
//!
//! ```text
//! assert tag == 14
//! let id = event.u32(+0x08)
//! let old = self.players.remove(&id)              ; 返回旧 connection_id（Option<u32>）
//! if let Some(conn) = old {
//!     let rest: Vec<u32> = self.players.iter()
//!         .filter(|(_, c)| **c == conn).map(|(p, _)| *p).collect();
//!     if rest.is_empty() {                        ; ← 关键：**该连接已经没人**
//!         emit(ConnectionRemoved { .., conn @+0x20 })
//!     }
//! }
//! ```
//!
//! 逐位依据（x86 `0x455250`）：`remove_entry` 返回的 `Option<u32>` 判别位在
//! `[rsp+0x28]`、值在 `[rsp+0x30]`，值被搬到 `ebp` 再存 `[rsp+0xc]` 供 filter 闭包用；
//! `SpecFromIterNested::from_iter` 的返回位（`Vec` 的 cap/ptr/len）在 `rsp+0x10/0x18/0x20`，
//! 紧接着 `cmpq $0x0, 0x20(%rsp)` 判 **len == 0**；随后才发事件，事件里
//! `mov %r12,0x30(%rsp)`（`+0x08` = `0x8000…`）、`mov %ebp,0x48(%rsp)`（`+0x20` = 那个 `conn`）。
//!
//! ⇒ 语义：**一个连接的最后一条玩家记录被删掉时，替它发一条 `ConnectionRemoved`(6)**。
//! 这是"玩家没了 ⇒ 连接也该收摊"的唯一发起处（`player_manager` 是**唯一**构造 (6) 的
//! 业务模块之一，另一处是 `server_connection_handler` 自己转发，见 `docs/09-network.md`）。
//!
//! ⚠️ `ConnectionRemoved` 的载荷里 `+0x08` 那 24 字节是什么，**不在本模块**：
//! 消费者 `game_state_manager::handle_connection_removed_event` 只读 `+0x20` 的 u32
//! （=`connection_id`），`server_connection_handler` 那边把 `+0x08` 当 `String` 用
//! 并用 `0x8000_0000_0000_0000` 当 `None` 哨兵。本模块级联时写的就是那个哨兵
//! ⇒ 只陈述"该字段在这里恒为哨兵值"，语义列**未知**。
//!
//! ## 5. 未确定
//!
//! * `ConnectionRemoved` 载荷 `+0x08` 那 24 字节的语义（§4 末）。
//! * `handle_entity_removed_event` 的 `entity id` 与 `player_id` 是不是同一个 id 空间：
//!   代码把 `event+8` 当作本表 key 用，属于**同一 id** 的直接推论；但事件名里叫
//!   `entity`，`docs/10` §13.5 记过"玩家没了 ⇒ 他的士兵各发一条 `entity_removed`"，
//!   两件事只能靠"这张表的 key 是 player_id"这一条串起来 ⇒ **推断**。

/// `player_info_event` 的判别值。
pub const EVENT_PLAYER_INFO: u16 = 13;
/// `connection_removed_event` 的判别值（本模块**级联时发**）。
pub const EVENT_CONNECTION_REMOVED: u16 = 6;
/// `new_connection_event` 的判别值（订阅 1）。
pub const EVENT_NEW_CONNECTION: u16 = 5;
/// `entity_removed_event` 的判别值（订阅 2）。
pub const EVENT_ENTITY_REMOVED: u16 = 14;

/// 玩家表在 `self` 里的起始偏移。
pub const PLAYER_MAP_OFFSET: usize = 0x18;
/// 玩家表 hasher 相对表基的偏移（`hash(self + 0x38)`）。
pub const PLAYER_MAP_HASHER_OFFSET: usize = 0x20;

/// `PlayerInfo`(13) 载荷里 `player_id` 的偏移。
pub const PLAYER_INFO_PLAYER_OFFSET: usize = 0x08;
/// `PlayerInfo`(13) 载荷里 `connection_id` 的偏移。
pub const PLAYER_INFO_CONNECTION_OFFSET: usize = 0x0c;

/// `connection_removed_event`(6) 载荷里 `connection_id` 的偏移（消费者侧观测，见模块文档 §4）。
pub const CONNECTION_REMOVED_ID_OFFSET: usize = 0x20;
/// 级联发 (6) 时载荷 `+0x08` 的常量哨兵（x86 `mov %r12,0x30(%rsp)`，`r12 = 0x8000…`）。
pub const CONNECTION_REMOVED_NONE_MARKER: u64 = 0x8000_0000_0000_0000;

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

/// `GraphwarPlayerManager::new` 的两条订阅（逐位，见模块文档 §1）。
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

/// `player_info_event`(13) 的载荷（两个 u32，见模块文档 §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerInfo {
    /// `+0x08`：玩家 id（表 key）。
    pub player_id: u32,
    /// `+0x0c`：连接 id（表 value）。
    pub connection_id: u32,
}

impl PlayerInfo {
    /// 事件判别值。
    pub const TAG: u16 = EVENT_PLAYER_INFO;
    /// 载荷字节数（两个 u32）。
    pub const PAYLOAD_SIZE: usize = 8;
    /// 完整 tag word。
    pub const fn tag_word() -> u64 {
        0x8000_0000_0000_0000 | EVENT_PLAYER_INFO as u64
    }
}

/// 级联发出去的 `connection_removed_event`(6)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionRemoved {
    /// `+0x20`：已经没有玩家的那个连接 id。
    pub connection_id: u32,
}

impl ConnectionRemoved {
    /// 事件判别值。
    pub const TAG: u16 = EVENT_CONNECTION_REMOVED;
    /// 载荷 `+0x08` 处的恒定哨兵（见模块文档 §4 末）。
    pub const NONE_MARKER: u64 = CONNECTION_REMOVED_NONE_MARKER;
    /// 完整 tag word。
    pub const fn tag_word() -> u64 {
        0x8000_0000_0000_0000 | EVENT_CONNECTION_REMOVED as u64
    }
}

/// `GraphwarPlayerManager` 的可判定状态：一张 `player_id → connection_id` 表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphwarPlayerManager {
    /// `HashMap<u32, u32>`（`self + 0x18`；这里用有序表让断言稳定）。
    pub players: std::collections::BTreeMap<u32, u32>,
}

impl GraphwarPlayerManager {
    /// 新建空管理器（原版还会订阅 [`SUBSCRIPTIONS`] 两条事件）。
    pub fn new() -> Self {
        Self::default()
    }

    /// `add_player(player_id, connection_id)`：写表 + 发一条 (13)。
    pub fn add_player(&mut self, player_id: u32, connection_id: u32) -> PlayerInfo {
        self.players.insert(player_id, connection_id);
        PlayerInfo { player_id, connection_id }
    }

    /// `send_all_info_events`：整表逐条发 (13)。
    ///
    /// 原版顺序由 `HashMap` 的 `RandomState` 决定 ⇒ 不可复现；这里只保证内容。
    pub fn send_all_info_events(&self) -> Vec<PlayerInfo> {
        self.players
            .iter()
            .map(|(player_id, connection_id)| PlayerInfo {
                player_id: *player_id,
                connection_id: *connection_id,
            })
            .collect()
    }

    /// `handle_entity_removed_event(entity_id)`：删一条；若**该连接已无人**，返回要级联的 (6)。
    pub fn handle_entity_removed_event(&mut self, entity_id: u32) -> Option<ConnectionRemoved> {
        let conn = self.players.remove(&entity_id)?;
        let still_here = self.players.values().any(|c| *c == conn);
        if still_here {
            None
        } else {
            Some(ConnectionRemoved { connection_id: conn })
        }
    }

    /// 查某玩家的连接 id。
    pub fn connection_of(&self, player_id: u32) -> Option<u32> {
        self.players.get(&player_id).copied()
    }

    /// 某个连接下还有几个玩家。
    pub fn players_of_connection(&self, connection_id: u32) -> Vec<u32> {
        self.players
            .iter()
            .filter(|(_, c)| **c == connection_id)
            .map(|(p, _)| *p)
            .collect()
    }

    /// 表里有多少人。
    pub fn len(&self) -> usize {
        self.players.len()
    }

    /// 表是否为空。
    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn player_info_is_two_u32() {
        assert_eq!(PlayerInfo::PAYLOAD_SIZE, 8);
        assert_eq!(
            (PLAYER_INFO_PLAYER_OFFSET, PLAYER_INFO_CONNECTION_OFFSET),
            (0x08, 0x0c)
        );
        assert_eq!(PlayerInfo::tag_word(), 0x8000_0000_0000_000d);
        assert_eq!(ConnectionRemoved::tag_word(), 0x8000_0000_0000_0006);
        assert_eq!(CONNECTION_REMOVED_ID_OFFSET, 0x20);
    }

    #[test]
    fn cascade_only_on_last_player_of_connection() {
        let mut m = GraphwarPlayerManager::new();
        m.add_player(1, 100);
        m.add_player(2, 100);
        m.add_player(3, 200);
        assert_eq!(m.players_of_connection(100), vec![1, 2]);
        // 同连接还有一个人 ⇒ 不级联。
        assert_eq!(m.handle_entity_removed_event(1), None);
        // 最后一个 ⇒ 级联一条 (6)。
        assert_eq!(
            m.handle_entity_removed_event(2),
            Some(ConnectionRemoved { connection_id: 100 })
        );
        // 不存在的 id ⇒ 什么也不发。
        assert_eq!(m.handle_entity_removed_event(9), None);
    }
}
