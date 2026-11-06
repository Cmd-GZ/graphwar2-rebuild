//! 段位：`GraphwarRankManager`（`player_id → 段位值` 一张表 + 一条广播）。
//!
//! 本模块回答三件事：**段位从哪来**、**`RankInfo`(67) 的载荷到底几个字段**、
//! **"分数 → 段位"的换算在不在本模块**（→ 不在，§4 是可复现的负结果）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（4 个，全部已定位）
//!
//! | 函数 | x86 vaddr / 字节 | arm64 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x454f30` / 352 | `0x498d2c` / 396 | 33 | 订阅 `new_connection_event` + `entity_removed_event` | 逐位 |
//! | `set_rank` | `0x455090` / 208 | `0x498eb8` / 212 | 61 | 写/删一张表 + 发 `RankInfo`(67) | 逐位 |
//! | `send_all_info_events` | `0x454d80` / 256 | `0x498b5c` / 252 | 73 | 遍历整表逐条发 (67) | 逐位 |
//! | `handle_entity_removed_event` | `0x454e80` / 112 | `0x498c58` / 120 | 83 | 删一条 | 观测 |
//!
//! 四个方法名 + 四条 `panic::Location` 锚双向唯一（`fixtures/name_to_addr.tsv`），
//! x86 ÷ arm64 尺寸比 = 0.89 / 0.98 / 1.02 / 0.93，全部落在 0.61–1.19 内。
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
//! 依据：`set_rank` 的删除支 `hash(self + 0x38, &key)`；
//! `send_all_info_events` 里 `shl eax, 3`（x86）取桶、`movq xmm0, [rcx - 8]` 取整个 entry。
//!
//! ⚠️ **`docs/112` §5.3 的"两张 map"是误读**：x86 的 `find(self+0x38, &A)` /
//! `remove(self+0x18…)` 里 `self+0x38` 是**同一张表（基址 0x18）的 hasher**，
//! 不是第二张表；arm64 的 `add x0, x22, #0x38 ; bl <hash>` 直接证明了这一点。
//!
//! ## 3. `RankInfo`(67) 的载荷：**三个 dword**（逐位，两条独立编码互校）
//!
//! arm64 `set_rank` 尾段：
//!
//! ```text
//! stp w21, w19, [sp, #0x10]   ; +0x10 = B, +0x14 = C
//! str w20,       [sp, #0x18]  ; +0x18 = A
//! mov x8, #0x43 ; movk x8, #0x8000, lsl #48
//! str x8, [sp, #0x8]          ; 载荷起点 +0x08 = tag
//! emit(self[8] + 0x18, sp + 8)
//! ```
//!
//! ⇒ 载荷 = `{ +0x08 = B（u32）, +0x0c = C（u32）, +0x10 = A（u32）}`。
//! x86 侧同一函数写 `[rsp+0x18]=edx` / `[rsp+0x1c]=ecx` / `[rsp+0x20]=esi`
//! （载荷基址 `rsp+0x10`，所以相对载荷同样是 `+8/+0xc/+0x10`）——**逐位吻合**。
//!
//! `send_all_info_events` 侧：`pshufd 0xe1` 把 entry 的 `(key,value)` 交换后写
//! `+0xc`，并给 `+8` 写常量 **1** ⇒ 广播时 `+8` 恒为 1。
//!
//! 于是唯一的自洽读法是
//! **`+0x08` = 有无段位（1 = 有，0 = 撤销），`+0x0c` = 段位值，`+0x10` = player_id**，
//! 与 `serde` 报的 `struct RankInfoEvent with 2 element` 的冲突解释见 `docs/10` §「段位的三个 dword」：
//! 序列化字段是 2 个（`rank` / `player_id`），`+0x08` 那个标志不参与序列化（**推断**，
//! 缺的是 `serialize` 分支的逐指令对照）。
//!
//! ## 4. "分数 → 段位"的换算**不在本模块**（可复现的负结果）
//!
//! 4 个函数里**没有** `div` / `sdiv` / `udiv`、没有阈值 `cmp` 立即数、没有任何
//! 22 项表。它们只做三件事：读/写一张 `HashMap<u32,u32>`、订阅两条事件、把表内容广播成 (67)。
//!
//! ## 5. 段位从哪来（问题的正面答案）
//!
//! arm64 全量扫描 `bl 0x498eb8`（`set_rank`）只有 **2 个调用点**：
//!
//! | 调用点 | 所在函数 |
//! |---|---|
//! | `0x3e4fe4` | `GraphwarRemoteEventHandler::handle_client_remote_event`（`0x3e3ad0`） |
//! | `0x3e7000` | `GraphwarRemoteEventHandler::handle_server_remote_event`（`0x3e6df0`） |
//!
//! ⇒ 段位**只从远端事件进入**（客户端与服务器两条远程事件处理路径各一处），
//! 既不是本地存档（`user://campaign.save` 与本模块无关，`docs/112` §7），
//! 也不是本机算出来的。
//!
//! 与 tag 66 / 67 的关系：
//!
//! * **67 `rank_info_event`**：本模块**发**（`set_rank` 发一条、`send_all_info_events` 发全表）。
//! * **66 `rank_request_event`**：本模块**不发**（全 `.text` 里 4 个函数都没有 tag 66 的构造）。
//!   发起侧在 `GraphwarLocalPlayerManager`（x86 `0x519f40`，玩家首次出现时连发 5 个请求：
//!   name 59 / skin 50 / face 52 / hat 54 / rank 66，`docs/112` §9.2）——**推断**那一档，
//!   因为发起侧只有 x86 侧的立即数证据。
//!
//! ## 6. 未确定
//!
//! * 段位值的**值域**：22 张 `res://ranks/rank_0..21.svg` 只证明存在编号 0..21 的图形资源，
//!   不能证明"22 段位"，也不能证明 `rank` 值域就是 0..21（`docs/112` §5.5 的负结果仍成立）。
//! * `+0x08` 那个标志是否真的不进序列化（见 §3 末）。
//! * `handle_entity_removed_event`（120 B）只有"删一条"的形状，x86 侧无锚 ⇒ **观测**级。

/// `rank_info_event` 的判别值。
pub const EVENT_RANK_INFO: u16 = 67;
/// `rank_request_event` 的判别值（本模块**不发**，发起侧在 `local_player_manager`）。
pub const EVENT_RANK_REQUEST: u16 = 66;
/// `new_connection_event` 的判别值（订阅 1）。
pub const EVENT_NEW_CONNECTION: u16 = 5;
/// `entity_removed_event` 的判别值（订阅 2）。
pub const EVENT_ENTITY_REMOVED: u16 = 14;

/// 段位表在 `self` 里的起始偏移。
pub const RANK_MAP_OFFSET: usize = 0x18;
/// 段位表 hasher 相对表基的偏移（`hash(self + 0x38)`）。
pub const RANK_MAP_HASHER_OFFSET: usize = 0x20;

/// `RankInfo`(67) 载荷里"有无段位"字段的偏移。
pub const RANK_INFO_FLAG_OFFSET: usize = 0x08;
/// `RankInfo`(67) 载荷里段位值的偏移。
pub const RANK_INFO_VALUE_OFFSET: usize = 0x0c;
/// `RankInfo`(67) 载荷里 `player_id` 的偏移。
pub const RANK_INFO_PLAYER_OFFSET: usize = 0x10;
/// 广播时 `+0x08` 的恒定值。
pub const RANK_INFO_FLAG_SET: u32 = 1;

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

/// `GraphwarRankManager::new` 的两条订阅（逐位）。
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

/// `rank_info_event`(67) 的载荷（三个 dword，见模块文档 §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankInfo {
    /// `+0x08`：1 = 有段位，0 = 撤销。
    pub flag: u32,
    /// `+0x0c`：段位值。
    pub rank: u32,
    /// `+0x10`：玩家 id。
    pub player_id: u32,
}

impl RankInfo {
    /// 事件判别值。
    pub const TAG: u16 = EVENT_RANK_INFO;
    /// 载荷字节数（三个 dword）。
    pub const PAYLOAD_SIZE: usize = 12;
    /// 完整 tag word。
    pub const fn tag_word() -> u64 {
        0x8000_0000_0000_0000 | EVENT_RANK_INFO as u64
    }

    /// 按载荷偏移 `(flag, rank, player_id)` 的三元组（广播/设置两条路都用它）。
    pub const fn triple(&self) -> (u32, u32, u32) {
        (self.flag, self.rank, self.player_id)
    }
}

/// `set_rank` 的第二个实参：`1` = 写入，其它 = 撤销。
///
/// 名字是**推断**：这个值只被拿去与 1 比较，语义只能从"1 走 insert、否则走 remove"反推。
pub const SET_RANK_INSERT: u32 = 1;

/// `GraphwarRankManager` 的可判定状态：一张 `player_id → 段位` 表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphwarRankManager {
    /// `HashMap<u32, u32>`（用有序表代替，只为让断言稳定；见 `send_all_info_events` 的说明）。
    pub ranks: std::collections::BTreeMap<u32, u32>,
}

impl GraphwarRankManager {
    /// 新建空管理器（原版还会订阅 [`SUBSCRIPTIONS`] 两条事件）。
    pub fn new() -> Self {
        Self::default()
    }

    /// `set_rank(player_id, op, rank)`：`op == 1` 写入，其它撤销；**两条路都发一条 (67)**。
    ///
    /// 返回值就是该发的那条事件（`flag` 恒等于 `op`，与 `send_all_info_events` 的
    /// "恒为 1" 不同——这是两条路唯一的差别）。
    pub fn set_rank(&mut self, player_id: u32, op: u32, rank: u32) -> RankInfo {
        if op == SET_RANK_INSERT {
            self.ranks.insert(player_id, rank);
        } else {
            self.ranks.remove(&player_id);
        }
        RankInfo { flag: op, rank, player_id }
    }

    /// `send_all_info_events`：整表逐条发 (67)，`flag` 恒为 [`RANK_INFO_FLAG_SET`]。
    ///
    /// 原版遍历顺序由 `HashMap` 的 `RandomState` 决定 ⇒ **不可复现**；本实现用有序表
    /// 只为让断言稳定（**不代表**原版顺序，标推断）。
    pub fn send_all_info_events(&self) -> Vec<RankInfo> {
        self.ranks
            .iter()
            .map(|(id, rank)| RankInfo {
                flag: RANK_INFO_FLAG_SET,
                rank: *rank,
                player_id: *id,
            })
            .collect()
    }

    /// `handle_entity_removed_event`：从表里删掉该玩家，返回是否删到了。
    pub fn handle_entity_removed_event(&mut self, player_id: u32) -> bool {
        self.ranks.remove(&player_id).is_some()
    }

    /// 查段位（表里没有就是 `None`）。
    pub fn rank_of(&self, player_id: u32) -> Option<u32> {
        self.ranks.get(&player_id).copied()
    }

    /// 表里有多少人。
    pub fn len(&self) -> usize {
        self.ranks.len()
    }

    /// 表是否为空。
    pub fn is_empty(&self) -> bool {
        self.ranks.is_empty()
    }
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn rank_info_is_three_dwords() {
        assert_eq!(RankInfo::PAYLOAD_SIZE, 12);
        assert_eq!(
            (RANK_INFO_FLAG_OFFSET, RANK_INFO_VALUE_OFFSET, RANK_INFO_PLAYER_OFFSET),
            (8, 0xc, 0x10)
        );
        assert_eq!(RankInfo::tag_word(), 0x8000_0000_0000_0043);
    }

    #[test]
    fn set_then_remove() {
        let mut m = GraphwarRankManager::new();
        let e = m.set_rank(7, SET_RANK_INSERT, 3);
        assert_eq!(e.triple(), (1, 3, 7));
        assert_eq!(m.rank_of(7), Some(3));
        let e = m.set_rank(7, 0, 3);
        assert_eq!(e.triple(), (0, 3, 7));
        assert_eq!(m.rank_of(7), None);
    }
}
