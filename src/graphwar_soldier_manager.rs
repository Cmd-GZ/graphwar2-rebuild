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
//! | `add_soldier` | **未定**（见 §6-A） | `0x49d2fc` / 144 B | 往表里写一条 | 观测（arm64 符号）+ 未知（x86 地址） |
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
//! ## 2. 数据结构：`Mutex<HashMap<u32, u32>>`（观测）
//!
//! 和 `graphwar_team_manager` 同一个骨架（`&self + 0x18` 是表、`+0x38` 是
//! `RandomState`、`+0x10/+0x14` 是锁字与 poison 字节），但**桶步长同样是 8 字节，
//! 值是 4 字节**：
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
//! ## 3. 键值极性：`soldier_id → owner_id`（推断，证据见 §4）
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
//! | A | `add_soldier` 的 x86 地址 | `fixtures/name_to_addr.tsv` 给的两个候选 `0x3e2830` / `0x474a10` 都**不是**它（前者是 `remote_event_handler` 的大分发器，后者是 `game_state_manager` 里建玩家的函数，arm64 长度分别是它的十几倍）；真正的 144 B 版本没被锚到 |
//! | B | `add_soldier` 有没有广播 `SoldierInfo` | 没读到函数体。本实现**默认广播**（与 `send_all_info_events` 同形），标推断 |
//! | C | `get_soldiers_from_owner` 的返回形态 | 名字是复数，但表只有两列、一次查找只能返回一个 `u32`；若源码是扫描，函数体应远大于 92 B。**冲突未解** |
//! | D | 表里键值极性的**直接**证据 | 只有级联方向（§4）这一条间接证据 + serde/事件字段顺序未读 |
//! | E | 士兵**出生**由谁发起 | `game_state_manager` 的 `0x474a10` 一带会克隆一名玩家的士兵 `Vec` 给新实体号；出生流程不在本模块 |

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

/// `soldier_id → owner_id` 那张表（原版在 `Mutex` 里，这里由调用方决定锁）。
#[derive(Debug, Default, Clone)]
pub struct SoldierTable {
    owners: HashMap<u32, u32>,
}

impl SoldierTable {
    /// 空表。
    pub fn new() -> Self {
        Self::default()
    }

    /// `add_soldier`：写一行并（推断）广播 `SoldierInfo`。
    ///
    /// ⚠️ 函数体没读到（§6-A/B），这里按 `send_all_info_events` 的同形写：
    /// 广播的是 `(soldier_id, owner_id)` 这一对。
    pub fn add_soldier(&mut self, soldier_id: u32, owner_id: u32) -> Vec<SoldierEvent> {
        self.owners.insert(soldier_id, owner_id);
        vec![SoldierEvent::SoldierInfo {
            first: soldier_id,
            second: owner_id,
        }]
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
}
