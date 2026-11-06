//! 队伍归属：`GraphwarTeam`（两队）、`player_id → team` 一张表、三条广播事件，
//! 外加"新玩家进哪一队"的判定。
//!
//! 本模块回答三件事：**队伍是怎么分的**、**换队/离队怎么落地**、**这跟 `docs/10` 的
//! 两张 6 色队色表是什么关系**。颜色**不在这里**（见 [`crate`] 之外的
//! `graphwar_color_manager`，`docs/10-campaign.md` §10）——`team_manager` 只维护
//! `player_id → GraphwarTeam` 这一层。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测**（能指到地址或字节）/ **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）；arm64 侧地址来自未 strip 的
//! 2.0.60 `libgraphwarrust.so` 的 `.symtab`，用来做**跨架构对名**。
//!
//! ## 1. 函数清单（全部已定位）
//!
//! | 函数 | x86 vaddr | arm64 vaddr / 长度 | 职责 | 等级 |
//! |---|---|---|---|---|
//! | `add_to_team` | `0x3c17d0` | `0x498f8c` / 456 B | 上锁、写表、广播 `TeamInfo` | 观测 |
//! | `remove_from_team` | `0x3c1980` | `0x499154` / 488 B | 上锁、删表、**删到了才**广播 `RemoveTeam` | 观测 |
//! | `（分队判定）` | `0x3c1b40` | **arm64 里被内联，无符号** | 数两队人数，返回新玩家该进哪队 | 观测 |
//! | `send_all_info_events` | `0x3c1cf0` | `0x49933c` / 564 B | 遍历整表逐条广播 `TeamInfo` | 观测 |
//! | `handle_entity_removed_event` | `0x3c1f40` | `0x499570` / 60 B | 断言 tag=14，转 `remove_from_team` | 观测 |
//! | `new` | `0x3c1fc0` | `0x49960c` / 396 B | 订阅 `new_connection_event` / `entity_removed_event` | 观测 |
//! | `get_team` | `0x3c2120` | `0x499798` / 520 B | 查表，**查不到返回 2** | 观测 |
//! | `GraphwarTeam::deserialize` | （`0x3cbee8` 一带） | `0x4999a0` / 724 B | serde 变体名 `Team1` / `Team2` | 观测 |
//!
//! `add_to_team` / `remove_from_team` 两个都有"唯一 `panic::Location` 锚"
//! （`fixtures/name_to_addr.tsv`：`src/graphwar_team_manager.rs:89:41` /
//! `:96:41`），`get_team` 在 `:103:26`。**四对**双向唯一锚落在同一个文件，
//! 所以这张表的地址与名字都不是猜的。
//!
//! ## 2. 数据结构：`Mutex<HashMap<u32, GraphwarTeam>>`（观测）
//!
//! 反编译里所有入口的第一段都是同一套：
//!
//! ```text
//! data  = self[0x18]
//! lock  = data[0x10]                 ; std::sync::Mutex 的锁字
//! poison= data[0x14]                 ; poison 字节；置位时走 unwrap 失败
//! map   = HashMap { ctrl: data[0x18], bucket_mask: data[0x20],
//!                   growth_left: data[0x28], items: data[0x30],
//!                   RandomState: data[0x38] }        ; 值按 hashbrown 布局
//! ```
//!
//! 键值对的**桶步长是 8 字节**：键 `u32` 在桶首、值 `u8` 在桶 `+4`
//! （`get_team` 里 `-8 - 8i` 取键、`-4 - 8i` 取值，`0x3c2190` 一带）。
//! ⇒ `HashMap<u32, GraphwarTeam>` 而 `GraphwarTeam` **占 1 字节**。
//!
//! 表用 `RandomState` 做哈希 ⇒ **遍历顺序每进程随机**。这解释了为什么
//! `send_all_info_events` 发出的 `TeamInfo` 顺序不可复现（与 `docs/10` §5.4
//! "颜色不可由种子复现"同源），也提醒任何"按遍历序复刻"的写法都是错的。
//!
//! ## 3. `GraphwarTeam` 是什么（观测 + 推断）
//!
//! * `.rodata` 里 `Team1`（`0x1be2c0`）与 `Team2`（`0x1be2c5`）**紧邻**，是 serde
//!   派生出来的变体名 ⇒ 只有两个变体（观测）。
//! * 值在表里只占 1 字节 ⇒ `#[repr(u8)]` 式的空变体枚举（观测）。
//! * 判别值 `Team1 = 0` / `Team2 = 1` 是**推断**：证据是
//!   `docs/10-campaign.md` §10 那张颜色表（`team_key = 0 → Team1 / 1 → Team2`，
//!   由 1358 个样本 0 例外反推），以及下面 §4 的分队算术只在 0/1 两值下自洽。
//!
//! `get_team` 查不到时返回 **2**——那是 `Option<GraphwarTeam>` 的 niche 编码
//! （`None = 2`），**不是**第三个队。所以对外接口返回 `Option`，不返回哨兵。
//!
//! ## 4. 新玩家进哪一队：**人数少的那队，平手进 Team1**（观测）
//!
//! `0x3c1b40` 遍历整表累加**两个量**，最后返回一个 `bool`：
//!
//! ```text
//! sum     = Σ (team as u32)          // 即 #Team2
//! flipped = Σ ((team as u32) ^ 1)    // 即 #Team1
//! return sum < flipped
//! ```
//!
//! 反汇编里就是两条累加（`0x3c1bbc`：`add %r13d,%ebx` 与
//! `xor $0x1` 后 `add` 进另一个计数器），出口 `0x3c1cbe`：
//! `cmp %ebx,%r12d` / `setb` 写回一个字节。
//!
//! 两个变量在 `team ∈ {0,1}` 上分别等于 `#Team2` 与 `#Team1`，于是
//! **返回值 1（= Team2）当且仅当 `#Team2 < #Team1`**。空表时 `0 < 0` 为假 ⇒
//! 第一个进房的人一定进 **Team1**（与 `docs/10` §10"Team1 是暖色/左侧"一致，
//! 也与"打靶关只有人类、人类队是 Team1"一致）。
//!
//! 本模块把这段算术**照抄**进 [`TeamTable::pick_team`]，而不是抄结论，这样即使
//! 将来发现 `GraphwarTeam` 的判别值不是 0/1，实现也不用改。
//!
//! **调用点**（观测）：`0x4739c0`（`graphwar_player_manager` 的
//! `new_connection_event` 处理）——
//!
//! ```text
//! if game_state_byte[0x120] != 1 -> return None      // 只在"局内"才分队
//! if player_count >= 10          -> return None      // 房间上限 10 人
//! entity = GLOBAL_COUNTER++                          // 0xac10c0，全局实体号自增
//! players.insert(entity, connection_id)
//! send PlayerInfo{entity, connection_id}             // tag 13
//! team = team_manager.pick_team()                    // ← 0x3c1b40
//! team_manager.add_to_team(entity, team)             // ← 0x3c17d0
//! color_manager.assign(entity, team)                 // 颜色：用同一 team 值查表
//! ```
//!
//! ⇒ **分队没有"队长/选边/队伍容量"这些规则**：唯一规则就是"谁少进谁，平手 Team1"，
//! 容量上限是**整房 10 人**（`plVar7[9] < 10`），不是每队 5 人——两队 5v5 是这条
//! 规则在满房时的**结果**，不是一条独立约束。
//!
//! ## 5. 事件（观测）
//!
//! 事件枚举的判别值（低 8 位）与 `src/events.rs` 的表逐条吻合：
//!
//! | 事件 | 判别值 | tag 字面量 | 载荷 |
//! |---|---|---|---|
//! | `team_info_event` | 24 | `0x8000_0000_0000_0018` | `{ player_id: u32, team: u8 }` |
//! | `remove_team_event` | 25 | `0x8000_0000_0000_0019` | `{ player_id: u32 }` |
//! | `entity_removed_event` | 14 | `0x8000_0000_0000_000e` | `{ entity_id: u32 }` |
//!
//! 广播一律走 `FUN_004d3530(事件总线 + 0x18, &事件)`；订阅走
//! `FUN_004d38f0(总线, "名字", 名字长度, 闭包, drop, …)`。`new` 里两次订阅的
//! 名字与长度是 `"new_connection_event"` / 20 与 `"entity_removed_event"` / 20
//! （`0x4c1fca`、`0x4c2027`）。
//!
//! **`send_all_info_events` 是为新连接服务的**：新客户端连进来时，每个 manager
//! 都把自己那张表整表重发一遍（`docs/10` §10 对颜色层记过同一现象，这里在队伍层
//! 再观测到一次）。注意它是**重发，不是重新分配**。
//!
//! ## 6. 与 `docs/10` 两张 6 色队色表的关系
//!
//! * `docs/10` §10 的两张 6 色表（Team1 暖色 / Team2 冷色）**不在本模块**：
//!   那里是 `HashMap<entity_id, Color>` + 双池 + 随机起点线性探测；
//! * 本模块只给出**一个 0/1 的队伍键**，颜色层拿同一个键去选池子
//!   （`0x4739c0` 里 `add_to_team` 与 `assign_color` 用的是**同一个** `uVar6`）；
//! * ⇒ 攻略层面的因果链是：**人数平衡 → 队伍键 → 颜色池 → 侧别（`flip`）**。
//!   换队会**同时**改队伍键与颜色（`docs/10` §4.5：55 次换色全部伴随换队）。
//!
//! ## 7. 仍未确定
//!
//! | # | 未知 | 缺什么 |
//! |---|---|---|
//! | A | `GraphwarTeam` 判别值 0/1 的**直接**证据 | 只有 serde 变体名 + 颜色层的 0/1 约定；没读到枚举布局本身 |
//! | B | `0x3c1b40` 在源码里的**名字** | arm64 侧被内联、无符号；只能按语义命名（本文叫 `pick_team`） |
//! | C | 换队（`ChangeTeamRequest` → 服务端）是否**先** `remove_from_team` 再 `add_to_team` | 服务端只看到 `0x3e2830` 的 `case 0x16`（= tag 24 `TeamInfo`）直接 `add_to_team`；客户端请求到信息事件的转换在别处 |
//! | D | 满 10 人之后新连接的落点 | `new_connection` 侧直接放弃（无事件），但"放弃"是否等于"踢人"未查 |

use std::collections::HashMap;

/// `GraphwarTeam` 的两支。
///
/// 判别值 `Team1 = 0` / `Team2 = 1` 是**推断**（见模块文档 §3）；
/// 变体名 `Team1` / `Team2` 是**观测**（serde 派生名在 `.rodata` 里紧邻）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum GraphwarTeam {
    Team1 = 0,
    Team2 = 1,
}

/// `get_team` 查不到时二进制返回的字面量（`Option` 的 `None` niche）。
///
/// 它**不是**第三个队——把 2 当队伍用会把"没分队"的人算进某一队。
pub const MISSING_TEAM_BYTE: u8 = 2;

impl GraphwarTeam {
    /// 从表里存的字节还原；只认 0 / 1，其余一律 `None`。
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(GraphwarTeam::Team1),
            1 => Some(GraphwarTeam::Team2),
            _ => None,
        }
    }

    /// 写回表里的字节。
    pub const fn as_byte(self) -> u8 {
        self as u8
    }

    /// 取反（换队）。`docs/10` §3.6 说换队伴随 `flip` 取反，两者都源于这个 0/1 键。
    pub const fn other(self) -> Self {
        match self {
            GraphwarTeam::Team1 => GraphwarTeam::Team2,
            GraphwarTeam::Team2 => GraphwarTeam::Team1,
        }
    }
}

/// 事件判别值（低 8 位）——与 `src/events.rs` 的表逐条吻合。
pub const EVENT_ENTITY_REMOVED: u8 = 14;
/// `team_info_event`
pub const EVENT_TEAM_INFO: u8 = 24;
/// `remove_team_event`
pub const EVENT_REMOVE_TEAM: u8 = 25;

/// 线上事件枚举的 tag 字面量：判别值 | 最高位。
///
/// 最高位是枚举布局的结果（带载荷的变体都这么编码），**判别值看低 8 位**。
pub const TAG_TEAM_INFO: u64 = 0x8000_0000_0000_0018;
/// `remove_team_event` 的 tag 字面量。
pub const TAG_REMOVE_TEAM: u64 = 0x8000_0000_0000_0019;
/// `entity_removed_event` 的 tag 字面量（`handle_entity_removed_event` 断言它）。
pub const TAG_ENTITY_REMOVED: u64 = 0x8000_0000_0000_000e;

/// `GraphwarTeamManager::new` 订阅的两条事件（名字, 名字字节数）。
///
/// 长度就是 `strlen`；反编译里是 `0x14`（= 20）两次。
pub const SUBSCRIBED_EVENTS: [(&str, usize); 2] = [
    ("new_connection_event", 20),
    ("entity_removed_event", 20),
];

/// 一条要广播出去的队伍事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamEvent {
    /// `team_info_event`：`{ player_id, team }`。
    TeamInfo { player_id: u32, team: GraphwarTeam },
    /// `remove_team_event`：`{ player_id }`。
    RemoveTeam { player_id: u32 },
}

impl TeamEvent {
    /// 线上 tag 字面量。
    pub const fn tag(&self) -> u64 {
        match self {
            TeamEvent::TeamInfo { .. } => TAG_TEAM_INFO,
            TeamEvent::RemoveTeam { .. } => TAG_REMOVE_TEAM,
        }
    }

    /// 判别值（tag 的低 8 位）。
    pub const fn discriminant_low(&self) -> u8 {
        match self {
            TeamEvent::TeamInfo { .. } => EVENT_TEAM_INFO,
            TeamEvent::RemoveTeam { .. } => EVENT_REMOVE_TEAM,
        }
    }

    /// 载荷字节数（不含 tag）：`TeamInfo` = 4+1，`RemoveTeam` = 4。
    pub const fn payload_len(&self) -> usize {
        match self {
            TeamEvent::TeamInfo { .. } => 5,
            TeamEvent::RemoveTeam { .. } => 4,
        }
    }
}

/// `player_id → GraphwarTeam` 那张表（原版在 `Mutex` 里，这里由调用方决定锁）。
#[derive(Debug, Default, Clone)]
pub struct TeamTable {
    teams: HashMap<u32, GraphwarTeam>,
}

impl TeamTable {
    /// 空表。第一个进房的人会拿到 [`GraphwarTeam::Team1`]（见 [`Self::pick_team`]）。
    pub fn new() -> Self {
        Self::default()
    }

    /// `add_to_team`（`0x3c17d0`）：写表并广播 `TeamInfo`。
    ///
    /// 反编译顺序是**先 insert 后广播**（`FUN_0053f9b0` 写完才 `FUN_004d3530`），
    /// 所以广播时表里已经有这条了。同名玩家重复调用是**覆盖**（`HashMap::insert`）。
    pub fn add_to_team(&mut self, player_id: u32, team: GraphwarTeam) -> Vec<TeamEvent> {
        self.teams.insert(player_id, team);
        vec![TeamEvent::TeamInfo { player_id, team }]
    }

    /// `remove_from_team`（`0x3c1980`）：删表，**只有真删掉了才广播** `RemoveTeam`。
    ///
    /// 这一条是逐位观测：反编译里 `HashMap::remove` 的返回值与 `2` 比较
    /// （`cmp %dl,0x2` / `je`，`0x3c1a08`），`None` 那支直接跳到解锁返回。
    pub fn remove_from_team(&mut self, player_id: u32) -> Vec<TeamEvent> {
        match self.teams.remove(&player_id) {
            Some(_) => vec![TeamEvent::RemoveTeam { player_id }],
            None => Vec::new(),
        }
    }

    /// `get_team`（`0x3c2120`）：查表；查不到是 `None`（二进制里返回 `2`）。
    pub fn get_team(&self, player_id: u32) -> Option<GraphwarTeam> {
        self.teams.get(&player_id).copied()
    }

    /// `send_all_info_events`（`0x3c1cf0`）：整表重发，**不重新分配**。
    ///
    /// ⚠️ 顺序来自 `RandomState` 哈希 ⇒ **每进程不同**，不要按序断言。
    pub fn send_all_info_events(&self) -> Vec<TeamEvent> {
        self.teams
            .iter()
            .map(|(&player_id, &team)| TeamEvent::TeamInfo { player_id, team })
            .collect()
    }

    /// `handle_entity_removed_event`（`0x3c1f40`）：实体被删 = 该玩家退出队伍。
    ///
    /// 二进制先断言 tag 恰为 [`TAG_ENTITY_REMOVED`]，否则 `explicit panic`
    /// （`0x4c1f40` 里的 `FUN_00a25660("explicit panic", 0xe, …)`）。
    pub fn handle_entity_removed_event(&mut self, entity_id: u32) -> Vec<TeamEvent> {
        self.remove_from_team(entity_id)
    }

    /// 分队判定（`0x3c1b40`）：**照抄二进制的两条累加**，不抄结论。
    ///
    /// ```text
    /// sum     = Σ (team as u32)
    /// flipped = Σ ((team as u32) ^ 1)
    /// return if sum < flipped { Team2 } else { Team1 }
    /// ```
    ///
    /// 在 `team ∈ {0,1}` 上 `sum = #Team2`、`flipped = #Team1`，于是
    /// **Team2 人数严格少于 Team1 时才进 Team2，否则进 Team1**（平手 → Team1）。
    pub fn pick_team(&self) -> GraphwarTeam {
        Self::pick_team_from(self.teams.values().map(|t| t.as_byte()))
    }

    /// [`Self::pick_team`] 的内核，直接吃字节流（便于逐值测试）。
    pub fn pick_team_from<I: IntoIterator<Item = u8>>(bytes: I) -> GraphwarTeam {
        let mut sum: u32 = 0;
        let mut flipped: u32 = 0;
        for byte in bytes {
            let v = byte as u32;
            sum = sum.wrapping_add(v);
            flipped = flipped.wrapping_add(v ^ 1);
        }
        if sum < flipped {
            GraphwarTeam::Team2
        } else {
            GraphwarTeam::Team1
        }
    }

    /// 两队人数 `(Team1, Team2)`——攻略用的读数，二进制里没有这个函数。
    pub fn counts(&self) -> (u32, u32) {
        let mut c = (0u32, 0u32);
        for team in self.teams.values() {
            match team {
                GraphwarTeam::Team1 => c.0 += 1,
                GraphwarTeam::Team2 => c.1 += 1,
            }
        }
        c
    }

    /// 表长（= 已在队伍里的玩家数）。
    pub fn len(&self) -> usize {
        self.teams.len()
    }

    /// 表是否为空。
    pub fn is_empty(&self) -> bool {
        self.teams.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 分队算术 = "#Team2 < #Team1"，平手进 Team1。
    ///
    /// 这条把「两条累加的表达式」与「人数比较的结论」钉在一起：任何一边改了，
    /// 这个穷举会立刻红。
    #[test]
    fn pick_team_is_fewer_players_with_tie_to_team1() {
        for c1 in 0u8..12 {
            for c2 in 0u8..12 {
                let mut bytes = Vec::new();
                bytes.extend(std::iter::repeat(0u8).take(c1 as usize));
                bytes.extend(std::iter::repeat(1u8).take(c2 as usize));
                let got = TeamTable::pick_team_from(bytes);
                let want = if c2 < c1 {
                    GraphwarTeam::Team2
                } else {
                    GraphwarTeam::Team1
                };
                assert_eq!(got, want, "c1(Team1)={c1} c2(Team2)={c2}");
            }
        }
    }

    /// 空表 ⇒ Team1；第一个人一定进 Team1。
    #[test]
    fn first_player_joins_team1() {
        let table = TeamTable::new();
        assert_eq!(table.pick_team(), GraphwarTeam::Team1);
        assert_eq!(table.counts(), (0, 0));
    }

    /// 写表 → 广播；删表 → **只有删到了才**广播。
    #[test]
    fn add_and_remove_broadcast_shapes() {
        let mut table = TeamTable::new();
        assert_eq!(
            table.add_to_team(7, GraphwarTeam::Team1),
            vec![TeamEvent::TeamInfo {
                player_id: 7,
                team: GraphwarTeam::Team1
            }]
        );
        assert_eq!(table.get_team(7), Some(GraphwarTeam::Team1));
        assert_eq!(
            table.remove_from_team(7),
            vec![TeamEvent::RemoveTeam { player_id: 7 }]
        );
        assert_eq!(table.get_team(7), None);
        // 再删一次：二进制里 remove 返回 None，什么都不发（`0x3c1a08` 的 `cmp dl,2`）
        assert!(table.remove_from_team(7).is_empty());
    }

    /// `entity_removed_event` 就是 `remove_from_team`；查不到时返回 `None` 而非 2。
    #[test]
    fn entity_removed_drops_the_team_row() {
        let mut table = TeamTable::new();
        table.add_to_team(3, GraphwarTeam::Team2);
        assert_eq!(
            table.handle_entity_removed_event(3),
            vec![TeamEvent::RemoveTeam { player_id: 3 }]
        );
        assert!(table.handle_entity_removed_event(3).is_empty());
        assert_eq!(MISSING_TEAM_BYTE, 2, "二进制查不到时返回 2（niche），不是第三个队");
        assert_eq!(GraphwarTeam::from_byte(MISSING_TEAM_BYTE), None);
        assert_eq!(GraphwarTeam::from_byte(0), Some(GraphwarTeam::Team1));
        assert_eq!(GraphwarTeam::from_byte(1), Some(GraphwarTeam::Team2));
    }

    /// 一直进人时两队**严格交替**：平手进 Team1，之后永远补人少的那队。
    ///
    /// 这条同时钉住"平手进 Team1"与"少的那队先补"——攻略上就是"第 1、3、5、7 个
    /// 进房的人落在 Team1"。
    #[test]
    fn roster_fills_alternately() {
        let mut table = TeamTable::new();
        let mut names = Vec::new();
        for player in 0..8u32 {
            let team = table.pick_team();
            names.push(if team == GraphwarTeam::Team1 { 1 } else { 2 });
            table.add_to_team(player, team);
        }
        assert_eq!(names, vec![1, 2, 1, 2, 1, 2, 1, 2]);
        assert_eq!(table.counts(), (4, 4));
        assert_eq!(table.len(), 8);
        assert!(!table.is_empty());
    }

    /// 整表重发的条数 = 表长；tag 与载荷长度钉死。
    #[test]
    fn send_all_info_replays_the_whole_table() {
        let mut table = TeamTable::new();
        table.add_to_team(1, GraphwarTeam::Team1);
        table.add_to_team(2, GraphwarTeam::Team2);
        let events = table.send_all_info_events();
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.tag() == TAG_TEAM_INFO));
        assert_eq!(TeamEvent::RemoveTeam { player_id: 0 }.payload_len(), 4);
        assert_eq!(TAG_TEAM_INFO & 0xff, EVENT_TEAM_INFO as u64);
        assert_eq!(TAG_REMOVE_TEAM & 0xff, EVENT_REMOVE_TEAM as u64);
        assert_eq!(TAG_ENTITY_REMOVED & 0xff, EVENT_ENTITY_REMOVED as u64);
    }

    /// `new` 订阅的两条事件，名字与长度都要对得上（长度就是 strlen）。
    #[test]
    fn subscription_names_and_lengths() {
        for (name, len) in SUBSCRIBED_EVENTS {
            assert_eq!(name.len(), len, "{name}");
        }
    }
}
