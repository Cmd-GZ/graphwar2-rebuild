//! 回合推进：**射手轮转**（谁开火）、**队伍决定 `flip`**（朝哪边开火）、
//! 以及"回合超时 = 跳过该玩家"的分支。
//!
//! 证据分级沿用仓库约定：**逐位验证**（`cargo test` 覆盖）/ **观测**（能指到地址或字节）/
//! **推断**（写明未证实）/ **未知**。地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 射手由 `turn_manager` 轮转决定，AI **不选**射手（`docs/98`）
//!
//! 每个玩家一张 `Vec<soldier_id>` + 一个游标 `index`（`0x28` 步长的 HashMap entry）：
//!
//! ```text
//! index   = (index + 1) % len       ① 先自增再取（旧值不用）
//! soldier = vec[index]              ②
//! if life_state(soldier) & 1 { 发 TurnInfo{player, soldier}; 该玩家结束 }   ③④
//! 否则继续转，最多 len 次
//! 一个活的都没有 → 发 TurnInfo{player, 0}                                    ⑤
//! ```
//!
//! 本实现的地址级依据（用 objdump 逐条读的，不是抄文档）：
//!
//! | 行为 | 指令 | vaddr | 等级 |
//! |---|---|---|---|
//! | entry 步长 `0x28`、`len = entry-0x10` | `mov -0x10(%rdx,%rax,8),%r15`（`rax = 5k`） | `0x463f27` | 观测 |
//! | 循环次数 = `len` | `dec %r14` / `jne`（`r14 = r15`） | `0x463fc7`–`0x463fca` | 观测 |
//! | `index = (index+1) % len` 写回 | `inc` → `div %r15` → `mov %rdx,-0x8(%rbp)` | `0x463f40`–`0x463f58` | 观测 |
//! | 取士兵 `vec[index]` | `mov -0x18(%rbp),%rcx` / `mov (%rcx,%rdx,4),%ebx` | `0x463f5c`–`0x463fa6` | 观测 |
//! | 存活判定 = 调 `life_manager` 后 `test $0x1` | `call 454950` / `test $0x1,%al` / `jne` | `0x463fb6`–`0x463fbd` | 观测 |
//! | **`len == 0` 与"全灭"走同一个发 0 的块** | `test %r15,%r15` / `je 463fd0`；循环耗尽也落到 `463fd0` | `0x463f2f`、`0x463fca` → `0x463fd0` | 观测 |
//! | 发 `{player_id, soldier_id}` 事件（活着的士兵） | `mov -0x28(%rbp),%eax`→`+0x60`；`mov %ebx,0x64` | `0x46406b`–`0x464072` | 观测 |
//! | 发 `{player_id, 0}`（空 / 全灭） | `mov %ebx,0x60`；`movl $0x0,0x64` | `0x464016`–`0x464021` | 观测 |
//!
//! 交叉证据：`docs/98` §2 的三局 replay **逐发吻合**（本文三条测试原样复刻）。
//!
//! > ⚠️ **一处文档与二进制的字面冲突**：`docs/98` §7 的伪代码注释写
//! > `if n == 0 { return None; }  // 不发 TurnInfo`，而 §1.4 的观测说 `0x463da0` 的
//! > 分支对 `len == 0` **直接发 `TurnInfo{p, 0}`**。objdump 站 §1.4 这边：
//! > `0x463f2f` 的 `je 0x463fd0` 与"试满 len 次"落下的是**同一个块**，那块把
//! > `soldier_id` 写成 0 后发事件。所以广播层默认 [`EmptyRoster::AnnounceZero`]；
//! > [`EmptyRoster::Skip`] 保留给"按 §7 字面"的调用方。**两层语义分开**：
//! > [`PlayerTurns::next_shooter`] 对空 `Vec` 仍返回 `None`（`0x4628d0` getter 不 push，
//! > 见 `docs/98` §1.4 上表第一行），广播层再按策略决定发不发 0。
//!
//! ## 2. 存活判定：状态字节 `& 1`，查不到是 2
//!
//! `0x454950`（`graphwar_life_manager.rs`）返回**一个字节**，查不到时返回 **2**
//! （`docs/98` §1.3）；调用方只认最低位，所以 `0`（死）与 `2`（未上场/查不到）
//! 都算"不能开火"。这条规则落在 [`crate::life::is_alive_byte`]。
//!
//! ## 3. `flip` 由**队伍**决定，不看坐标（`docs/105`）
//!
//! Team1 恒 `x ≤ 0`、朝 `+x`（`flip = false`）；Team2 恒 `x ≥ 0`、朝 `-x`（`flip = true`）。
//! 773 个 `PosInfo` 样本 0 例外。注意本模块的接口里**没有任何坐标参数**——
//! 结构上就不可能"用起点/终点 x 比大小去猜"。
//!
//! ## 4. 回合超时 = **跳过该玩家**（⚠️ **用户口述，未独立验证**）
//!
//! 用户口述："回合超时 = 跳过该玩家"（`docs/HANDOVER.md` §7.10 第 2 条）。
//! 未在二进制或 replay 里独立验证过，所以这里只把它做成一个**显式分支**
//! （[`TurnDecision::Skip`] + [`SkipReason::TurnTimeout`]），不改其它语义。
//!
//! **仍未确定**（实现里**没有**替它做主）：
//!
//! * 超时跳过时，该玩家的士兵游标**是否推进**——本实现**不推进**（等于"这回合没轮到过他"）；
//! * 超时的那一刻，`TurnInfo` 是否**已经广播过**（若已广播，调用方拿到 `Skip` 时应忽略它）；
//! * 超时是"这个玩家跳过一次"还是"这个玩家的士兵也算用掉一次轮转"。
//!
//! ## 5. 仍未确定（不属本模块，但会喂进来）
//!
//! | # | 未知 | 谁负责 |
//! |---|---|---|
//! | A | 游标初值（`docs/98` §6-B）：三个 replay 例都吻合初值 0（首取 `soldiers[1]`），但 `1782763022` 首取 `2509` 不符 | 需回二进制/更多样本；本实现默认 0，另给 [`PlayerTurns::with_index`] |
//! | B | `Vec<soldier_id>` 的**填充顺序**（`docs/98` §6-C）：实测 = `SoldierInfo` 创建顺序，但没跟到 push 点；若来自 HashMap 遍历则每局不同 ⇒ 不可复现 | 调用方（`SoldierInfo` 的容器） |
//! | C | **玩家出场顺序**（`docs/47`）：判为 HashMap 迭代序 ⇒ 不可确定性复现。本模块**不管**玩家顺序，由调用方给 | 调用方 |
//! | D | `SequentialTurns` 与 `SimultaneousTurns` 两个分支的发射时序（`docs/98` §6-D） | 未解 |
//! | E | 空 `Vec` 到底走哪条分支（§1 上方那条冲突） | 需 REA 复核调用点；本实现给两个策略 |

use std::collections::BTreeMap;

use crate::life::{is_alive_byte, LifeState, Status, MISSING_STATE};

/// "没有射手"的哨兵 id。
///
/// 真游戏的 `soldier_id` 从 1 起（`SoldierInfo` 里从未出现 0），所以 0 被用作
/// "全灭 / 空队"的哨兵（`docs/98` §1.4，replay `1793884702` 实测到 `TurnInfo{soldier_id: 0}`）。
/// [`PlayerTurns::add`] 会拒绝把 0 当真实士兵 id。
pub const SOLDIER_ID_NONE: i64 = 0;

// ---------------------------------------------------------------------------
// 队伍
// ---------------------------------------------------------------------------

/// 一支队伍。只建模观测到的两方（`docs/105` §4：Team3/Team4 未观测到）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Team {
    /// `x ≤ 0`，朝 `+x` 开火。
    One,
    /// `x ≥ 0`，朝 `-x` 开火。
    Two,
}

impl Team {
    /// 协议里的队伍编号（`TeamInfo` / `Match::set_team` 用的是 `1`/`2`）。
    pub const fn id(self) -> u8 {
        match self {
            Team::One => 1,
            Team::Two => 2,
        }
    }

    /// 编号 → 队伍。**3 及以上返回 `None`**：`docs/105` §4 明确"未观测到四人以上队伍"，
    /// 与其猜一个方向，不如让调用方知道它落在已知模型之外。
    pub const fn from_id(id: u8) -> Option<Self> {
        match id {
            1 => Some(Team::One),
            2 => Some(Team::Two),
            _ => None,
        }
    }

    /// 开火方向翻转位。**由队伍决定**，与坐标无关（`docs/105`，773 个位置样本 0 例外）。
    ///
    /// `flip = false` ⇒ 朝 `+x`；`flip = true` ⇒ 朝 `-x`（与 `path::calculate` /
    /// `shot::correction` 的 `flip` 参数同义）。
    pub const fn flip(self) -> bool {
        match self {
            Team::One => false,
            Team::Two => true,
        }
    }
}

// ---------------------------------------------------------------------------
// 一个玩家的轮转状态
// ---------------------------------------------------------------------------

/// 一个玩家的 `{ Vec<soldier_id>, index }`——`0x28` 步长那张表的 Rust 形态
/// （`docs/98` §1.1：key、`Vec` 指针、`len`、游标四个字段，已在 `0x463f27`–`0x463fa6`
/// 逐条核对）。
///
/// 顺序 = 这张 `Vec` 的存储顺序，**不是** id 排序、不是坐标、不是随机
/// （`docs/98` §2.4 逐条推翻）。`Vec` 里装谁、什么顺序由调用方决定（§5-B）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerTurns {
    /// 该玩家的士兵 id，顺序即轮转顺序。
    soldiers: Vec<i64>,
    /// 游标。语义是"**上一次**取到的位置"——下一次先自增再取。
    index: usize,
}

impl PlayerTurns {
    /// 游标从 0 起（`docs/98` §2 三个 replay 例都吻合；初值本身仍是未知，见 §5-A）。
    pub fn new(soldiers: Vec<i64>) -> Self {
        let mut p = Self { soldiers: Vec::new(), index: 0 };
        for id in soldiers {
            p.add(id);
        }
        p
    }

    /// 指定游标初值——`docs/98` §6-B 说初值未知（`1782763022` 首取不是 `soldiers[1]`），
    /// 所以留一个显式入口，默认值不被当成结论。
    pub fn with_index(soldiers: Vec<i64>, index: usize) -> Self {
        let mut p = Self::new(soldiers);
        p.index = index;
        p
    }

    /// 追加一个士兵。`soldier_id == 0` 是哨兵，不允许入表（会与"全灭"混淆）。
    pub fn add(&mut self, soldier_id: i64) {
        assert_ne!(
            soldier_id, SOLDIER_ID_NONE,
            "soldier_id 0 是\"全灭\"哨兵，不能作为真实士兵入表（docs/98 §1.4）"
        );
        self.soldiers.push(soldier_id);
    }

    /// 士兵列表（顺序 = 轮转顺序）。
    pub fn soldiers(&self) -> &[i64] {
        &self.soldiers
    }

    pub fn len(&self) -> usize {
        self.soldiers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.soldiers.is_empty()
    }

    /// 当前游标。
    pub fn index(&self) -> usize {
        self.index
    }

    /// "当前回合的士兵"——即 `0x4628d0` 那个 getter 的语义。
    ///
    /// 它**不推进**游标，空表返回 `None`（`docs/98` §1.4 上表第一行：getter 对
    /// `len == 0` 不 push）。
    pub fn current(&self) -> Option<i64> {
        self.soldiers.get(self.index).copied()
    }

    /// 取**下一个**射手。返回值语义（`docs/98` §7，两者协议后果不同）：
    ///
    /// * `Some(id)`，`id != 0`：发 `TurnInfo{player_id, id}`；
    /// * `Some(`[`SOLDIER_ID_NONE`]`)` = `Some(0)`：**全灭**，仍要发
    ///   `TurnInfo{player_id, 0}`（`0x463fd0`，replay `1793884702` 实测到）；
    /// * `None`：`Vec` 为空 ⇒ 没有"当前士兵"。**广播与否由 [`TurnManager`] 决定**
    ///   （见 [`EmptyRoster`]）；这一层只如实回答"取不到"。
    ///
    /// `alive` 是"能开火吗"的谓词；要直接用 `life_manager` 的原始状态字节，
    /// 走 [`PlayerTurns::next_shooter_by_state`]。
    pub fn next_shooter(&mut self, alive: impl Fn(i64) -> bool) -> Option<i64> {
        let n = self.soldiers.len();
        if n == 0 {
            return None;
        }
        // 调用方可能在两次调用之间改过 Vec，先把游标拉回合法区间。
        if self.index >= n {
            self.index %= n;
        }
        // ① 先自增再取；③ 不活就继续转；最多 len 次（④）。
        for _ in 0..n {
            self.index = (self.index + 1) % n;
            let soldier = self.soldiers[self.index];
            if alive(soldier) {
                return Some(soldier);
            }
        }
        // ⑤ 试满 len 次仍无 → 全灭。
        Some(SOLDIER_ID_NONE)
    }

    /// 直接用 `life_manager` 的状态字节判活：**只有 `& 1` 为真才算活**
    /// （`0x463fbb` 的 `test $0x1,%al`；查不到返回 2 ⇒ 不可开火，`docs/98` §1.3）。
    pub fn next_shooter_by_state(&mut self, state: impl Fn(i64) -> u8) -> Option<i64> {
        self.next_shooter(|id| is_alive_byte(state(id)))
    }
}

// ---------------------------------------------------------------------------
// 空士兵表的两种处理
// ---------------------------------------------------------------------------

/// `Vec<soldier_id>` 为空时，广播层怎么做。
///
/// 这两个变体对应 `docs/98` 里**字面上不一致**的两处：
///
/// * [`EmptyRoster::AnnounceZero`]：`0x463da0` 的 `len == 0` 分支与"全灭"共用
///   `0x463fd0` 这个发 0 的块（objdump 核实：`0x463f2f` 的 `je 463fd0`）。
/// * [`EmptyRoster::Skip`]：`docs/98` §7 伪代码的注释
///   `if n == 0 { return None; } // 不发 TurnInfo`。
///
/// 默认按前者（二进制优先，见模块注释 §1 的冲突说明）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmptyRoster {
    /// 仍发 `TurnInfo{player_id, 0}`（`0x463fd0`，观测）。
    #[default]
    AnnounceZero,
    /// 不发 `TurnInfo`（`docs/98` §7 伪代码的注释）。
    Skip,
}

// ---------------------------------------------------------------------------
// 一次"轮到该玩家"的判决
// ---------------------------------------------------------------------------

/// 为什么不发 `TurnInfo`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// 这个 `player_id` 没登记过（调用侧的错，不该出现在正常流程里）。
    UnknownPlayer,
    /// 士兵表为空（`EmptyRoster::Skip` 时才会出现；`docs/98` §1.4 的"两种空"之一）。
    NoSoldiers,
    /// 回合超时——**用户口述**（`docs/HANDOVER.md` §7.10 第 2 条），**未独立验证**。
    TurnTimeout,
}

/// 轮到该玩家时的判决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnDecision {
    /// 广播 `TurnInfo{player_id, soldier_id}`。`soldier_id == 0` 表示全灭/空队
    /// （同样要广播，见 [`EmptyRoster`]）。
    Announce {
        player_id: i64,
        soldier_id: i64,
        /// 这一回合的开火方向翻转位，由**队伍**决定（`docs/105`）。
        flip: bool,
    },
    /// 本回合不发 `TurnInfo`。
    Skip { player_id: i64, reason: SkipReason },
}

impl TurnDecision {
    /// 要广播的话是 `Some((player_id, soldier_id))`。
    ///
    /// 这个函数就是 `Some(0)` 与 `None` 的**协议区别**所在：
    /// 全灭时返回 `Some((p, 0))`（发），空队 + `EmptyRoster::Skip` 时返回 `None`（不发）。
    pub fn broadcast(self) -> Option<(i64, i64)> {
        match self {
            TurnDecision::Announce { player_id, soldier_id, .. } => Some((player_id, soldier_id)),
            TurnDecision::Skip { .. } => None,
        }
    }

    /// 这一回合的射手（不需要区分"发不发"时用它）。
    pub fn soldier_id(self) -> Option<i64> {
        self.broadcast().map(|(_, soldier_id)| soldier_id)
    }

    /// 这一回合的 `flip`；被跳过时是 `None`。
    pub fn flip(self) -> Option<bool> {
        match self {
            TurnDecision::Announce { flip, .. } => Some(flip),
            TurnDecision::Skip { .. } => None,
        }
    }

    pub fn skip_reason(self) -> Option<SkipReason> {
        match self {
            TurnDecision::Skip { reason, .. } => Some(reason),
            TurnDecision::Announce { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// 管理器：玩家 → (队伍, 轮转状态)
// ---------------------------------------------------------------------------

/// 一个玩家的登记项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerEntry {
    pub team: Team,
    pub turns: PlayerTurns,
}

/// 玩家 → `{队伍, Vec<soldier_id>, 游标}`。
///
/// **不**建模玩家出场顺序：`docs/47` 已判定那是 HashMap 迭代序（OS 熵播种的 SipHash），
/// 不可确定性复现，必须由调用方给。这里只用 `BTreeMap` 保证**玩家之间的遍历顺序确定**，
/// 免得同一个测试两次跑出两种顺序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnManager {
    players: BTreeMap<i64, PlayerEntry>,
    empty_roster: EmptyRoster,
}

impl Default for TurnManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnManager {
    pub fn new() -> Self {
        Self { players: BTreeMap::new(), empty_roster: EmptyRoster::default() }
    }

    /// 换掉"空士兵表"的策略（默认 [`EmptyRoster::AnnounceZero`]，理由见模块注释 §1）。
    pub fn with_empty_roster(mut self, policy: EmptyRoster) -> Self {
        self.empty_roster = policy;
        self
    }

    pub fn empty_roster(&self) -> EmptyRoster {
        self.empty_roster
    }

    /// 登记一个玩家。同一 `player_id` 再登记一次会**替换**（返回旧值）。
    pub fn add_player(&mut self, player_id: i64, team: Team, soldiers: Vec<i64>) -> Option<PlayerEntry> {
        self.players.insert(player_id, PlayerEntry { team, turns: PlayerTurns::new(soldiers) })
    }

    pub fn player(&self, player_id: i64) -> Option<&PlayerEntry> {
        self.players.get(&player_id)
    }

    /// 玩家 → `flip`。**只有队伍参与**，没有坐标参数（`docs/105` §3）。
    pub fn flip_for(&self, player_id: i64) -> Option<bool> {
        self.players.get(&player_id).map(|e| e.team.flip())
    }

    /// 直接推进该玩家的游标（不产生判决）。未知玩家返回 `None`。
    pub fn next_shooter(&mut self, player_id: i64, state: impl Fn(i64) -> u8) -> Option<i64> {
        self.players.get_mut(&player_id)?.turns.next_shooter_by_state(state)
    }

    /// 本回合轮到 `player_id`：决定"发不发 `TurnInfo`、发给谁、朝哪边"。
    ///
    /// * `state`：`life_manager` 的状态字节（`& 1` 为真才算活，`docs/98` §1.3）；
    /// * `timed_out`：回合是否超时。**用户口述**"超时 = 跳过该玩家"，未独立验证，
    ///   且**不推进游标**（见模块注释 §4）。
    ///
    /// 注意：它**不**产生 `FunctionFire`。全灭时 `soldier_id = 0`，真游戏也没有
    /// `FunctionFire`（`docs/98` §1.4）。
    pub fn begin_turn(
        &mut self,
        player_id: i64,
        state: impl Fn(i64) -> u8,
        timed_out: bool,
    ) -> TurnDecision {
        let policy = self.empty_roster;
        let Some(entry) = self.players.get_mut(&player_id) else {
            return TurnDecision::Skip { player_id, reason: SkipReason::UnknownPlayer };
        };
        let flip = entry.team.flip();
        // 超时分支（用户口述）：连射手都不选，游标原地不动。
        if timed_out {
            return TurnDecision::Skip { player_id, reason: SkipReason::TurnTimeout };
        }
        match entry.turns.next_shooter_by_state(state) {
            Some(soldier_id) => TurnDecision::Announce { player_id, soldier_id, flip },
            None => match policy {
                // 观测（0x463fd0）：空表也发 {player, 0}。
                EmptyRoster::AnnounceZero => TurnDecision::Announce {
                    player_id,
                    soldier_id: SOLDIER_ID_NONE,
                    flip,
                },
                EmptyRoster::Skip => TurnDecision::Skip { player_id, reason: SkipReason::NoSoldiers },
            },
        }
    }
}

/// 把 [`LifeState`] 适配成"状态字节查询"。
///
/// 表里没有记录时返回 [`MISSING_STATE`]（= 2，`0x454950` 查不到时的返回值），
/// 于是它和"死"一样不可开火（`docs/98` §1.3）。
pub fn life_state_of(life: &LifeState) -> impl Fn(i64) -> u8 + '_ {
    move |soldier_id: i64| {
        match u32::try_from(soldier_id).ok().and_then(|id| life.status(id)) {
            Some(Status::Alive) => 1,
            Some(Status::Dead) => 0,
            None => MISSING_STATE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// 用"活着的士兵集合"造状态字节查询：集合里是 1，不在里面是 0（死）。
    fn alive_set(ids: &[i64]) -> impl Fn(i64) -> u8 + '_ {
        let set: BTreeSet<i64> = ids.iter().copied().collect();
        move |id: i64| u8::from(set.contains(&id))
    }

    /// 一直活着。
    fn all_alive(_: i64) -> u8 {
        1
    }

    // -----------------------------------------------------------------------
    // ① 轮转顺序：先自增再取
    // -----------------------------------------------------------------------

    /// `docs/98` §1.2 第 ① 步：游标**先自增**，所以初值 0 时首取 `soldiers[1]`。
    #[test]
    fn cursor_increments_before_reading() {
        let mut p = PlayerTurns::new(vec![10, 20, 30]);
        assert_eq!(p.index(), 0);
        let mut got = Vec::new();
        for _ in 0..4 {
            got.push(p.next_shooter_by_state(all_alive).unwrap());
        }
        assert_eq!(got, vec![20, 30, 10, 20], "先自增再取，且按 len 回绕");
        assert_eq!(p.index(), 1, "游标写回的是取到的位置");
    }

    /// 用 `with_index` 指定别的初值（`docs/98` §6-B：初值本身未定）。
    #[test]
    fn explicit_initial_cursor_is_honoured() {
        let mut p = PlayerTurns::with_index(vec![10, 20, 30], 2);
        assert_eq!(p.next_shooter_by_state(all_alive), Some(10), "(2+1)%3 = 0");
    }

    // -----------------------------------------------------------------------
    // ② 跳过死人：三局 replay 原样复刻（docs/98 §2）
    // -----------------------------------------------------------------------

    /// `1782763805` 玩家 7443（HAL）：士兵 `[7444..7448]`，实际 `7445 → 7448 → 7444`。
    #[test]
    fn replay_1782763805_player_7443_rotation() {
        let mut p = PlayerTurns::new(vec![7444, 7445, 7446, 7447, 7448]);
        let mut got = Vec::new();
        for _ in 0..3 {
            got.push(p.next_shooter_by_state(alive_set(&[7444, 7445, 7448])).unwrap());
        }
        assert_eq!(got, vec![7445, 7448, 7444], "2 与 3 号已死，应被跳过");
    }

    /// `1793884702` 玩家 1053：士兵 `[1083,1054,1055,1084,1085]`，实际 `1054 → 1055 → 1085 → 1083`。
    #[test]
    fn replay_1793884702_player_1053_rotation() {
        let mut p = PlayerTurns::new(vec![1083, 1054, 1055, 1084, 1085]);
        let mut got = Vec::new();
        for _ in 0..4 {
            got.push(p.next_shooter_by_state(alive_set(&[1083, 1054, 1055, 1085])).unwrap());
        }
        assert_eq!(got, vec![1054, 1055, 1085, 1083], "1084 死后才轮到 1085，然后回绕");
    }

    /// `1793874543` 玩家 621（只剩两个活人）：严格交替 7 次。
    ///
    /// 注意这一局的 `alive` 是**会变的**：第 4 发之前 623 也阵亡了
    /// （`docs/98` §2.3 的预测里 1 号位已经是死人），之后只剩 627 / 628。
    #[test]
    fn replay_1793874543_player_621_alternates() {
        let mut p = PlayerTurns::new(vec![622, 623, 627, 629, 628]);
        let mut alive: Vec<i64> = vec![623, 627, 628];
        let expected = [623, 627, 628, 627, 628, 627, 628, 627];
        for (i, expect) in expected.iter().enumerate() {
            if i == 3 {
                alive.retain(|id| *id != 623); // 623 在这一发之前已死
            }
            assert_eq!(p.next_shooter_by_state(alive_set(&alive)), Some(*expect), "第 {i} 发");
        }
    }

    // -----------------------------------------------------------------------
    // ③ 全灭 → Some(0)（与 None 不同）
    // -----------------------------------------------------------------------

    /// 试满 `len` 次仍无活人 ⇒ `Some(0)`；且再次调用还是 `Some(0)`（游标回到原点）。
    #[test]
    fn all_dead_returns_some_zero() {
        let mut p = PlayerTurns::new(vec![7, 8, 9]);
        let dead = alive_set(&[]);
        assert_eq!(p.next_shooter_by_state(&dead), Some(SOLDIER_ID_NONE));
        assert_eq!(p.next_shooter_by_state(&dead), Some(SOLDIER_ID_NONE));
        assert_eq!(p.index(), 0, "试满 len 次后游标回到起点");
    }

    /// `Some(0)` 与 `None` 的**协议后果**不同：前者发 `TurnInfo{0}`，后者不发。
    #[test]
    fn some_zero_broadcasts_but_none_does_not() {
        // 全灭：Some(0) → 仍然广播
        let mut manager = TurnManager::new();
        manager.add_player(621, Team::One, vec![622, 623]);
        let dead = manager.begin_turn(621, alive_set(&[]), false);
        assert_eq!(dead, TurnDecision::Announce { player_id: 621, soldier_id: 0, flip: false });
        assert_eq!(dead.broadcast(), Some((621, 0)), "全灭仍要广播 soldier_id = 0");

        // 空表 + Skip 策略：None → 不广播
        let mut skip = TurnManager::new().with_empty_roster(EmptyRoster::Skip);
        skip.add_player(621, Team::One, vec![]);
        let none = skip.begin_turn(621, alive_set(&[]), false);
        assert_eq!(none, TurnDecision::Skip { player_id: 621, reason: SkipReason::NoSoldiers });
        assert_eq!(none.broadcast(), None, "None 语义：不发 TurnInfo");
    }

    // -----------------------------------------------------------------------
    // ④ 单士兵
    // -----------------------------------------------------------------------

    /// `len == 1`：`(0+1)%1 = 0`，永远是同一个人；死了就是全灭。
    #[test]
    fn single_soldier_is_taken_every_turn() {
        let mut p = PlayerTurns::new(vec![42]);
        assert_eq!(p.next_shooter_by_state(all_alive), Some(42));
        assert_eq!(p.next_shooter_by_state(all_alive), Some(42));
        assert_eq!(p.next_shooter_by_state(alive_set(&[])), Some(SOLDIER_ID_NONE));
        assert_eq!(p.len(), 1);
    }

    // -----------------------------------------------------------------------
    // ⑤ 空队
    // -----------------------------------------------------------------------

    /// 空 `Vec`：原语返回 `None`（getter 没有"当前士兵"），不 panic、不推进。
    #[test]
    fn empty_roster_returns_none() {
        let mut p = PlayerTurns::new(vec![]);
        assert!(p.is_empty());
        assert_eq!(p.current(), None);
        assert_eq!(p.next_shooter_by_state(all_alive), None);
        assert_eq!(p.index(), 0, "空表不该改游标");
    }

    /// 广播层：默认（`0x463fd0` 观测）空表也发 `soldier_id = 0`。
    #[test]
    fn empty_roster_announces_zero_by_default() {
        let mut manager = TurnManager::new();
        manager.add_player(1, Team::Two, vec![]);
        assert_eq!(manager.empty_roster(), EmptyRoster::AnnounceZero);
        let d = manager.begin_turn(1, alive_set(&[]), false);
        assert_eq!(d, TurnDecision::Announce { player_id: 1, soldier_id: 0, flip: true });
        assert_eq!(d.broadcast(), Some((1, 0)));
    }

    /// 反过来显式选 `Skip` 也能拿到"不发"的语义。
    #[test]
    fn empty_roster_skip_does_not_broadcast() {
        let mut manager = TurnManager::new().with_empty_roster(EmptyRoster::Skip);
        manager.add_player(1, Team::Two, vec![]);
        assert_eq!(manager.begin_turn(1, alive_set(&[]), false).broadcast(), None);
    }

    // -----------------------------------------------------------------------
    // 队伍 → flip（docs/105）
    // -----------------------------------------------------------------------

    /// `flip` 只由队伍决定：Team1 → 向右（`false`），Team2 → 向左（`true`）。
    #[test]
    fn flip_comes_from_the_team() {
        assert!(!Team::One.flip(), "Team1 在 x<0 侧，朝 +x");
        assert!(Team::Two.flip(), "Team2 在 x>0 侧，朝 -x");
        assert_eq!(Team::from_id(1), Some(Team::One));
        assert_eq!(Team::from_id(2), Some(Team::Two));
        assert_eq!(Team::from_id(3), None, "3 队及以上未观测，不猜方向");
        assert_eq!(Team::One.id(), 1);
        assert_eq!(Team::Two.id(), 2);
    }

    /// 判决里带的 `flip` 与队伍一致——注意接口里**根本没有**坐标参数，
    /// 所以"用起点/终点 x 比大小去猜方向"在结构上就不可能（`docs/105` §3）。
    #[test]
    fn begin_turn_carries_team_flip() {
        let mut manager = TurnManager::new();
        manager.add_player(229, Team::One, vec![230, 231]);
        manager.add_player(232, Team::Two, vec![233, 234]);
        assert_eq!(manager.flip_for(229), Some(false));
        assert_eq!(manager.flip_for(232), Some(true));
        let a = manager.begin_turn(229, all_alive, false);
        let b = manager.begin_turn(232, all_alive, false);
        assert_eq!(a.flip(), Some(false));
        assert_eq!(b.flip(), Some(true));
        assert_eq!(a.soldier_id(), Some(231), "初值 0 ⇒ 首取 soldiers[1]");
        assert_eq!(b.soldier_id(), Some(234));
    }

    /// 抓包里十局的"相邻两次出手来自不同队伍"——那是**玩家顺序**的性质（`docs/47`），
    /// 本模块不管顺序，但轮转模型必须能复现它（给定每个玩家的初始游标）。
    ///
    /// ⚠️ 初始游标是两个自由参数（`docs/98` §6-B 未定），这里显式给出**只为复现抓包**，
    /// 不当结论。
    #[test]
    fn captured_sequence_is_reachable_with_per_player_cursors() {
        let captured = [230, 234, 231, 233, 230, 234, 231, 233, 230];
        let player_of = |s: i64| if s == 230 || s == 231 { 229 } else { 232 };
        let mut manager = TurnManager::new();
        manager.add_player(229, Team::One, vec![230, 231]);
        manager.add_player(232, Team::Two, vec![233, 234]);
        // 抓包里 229 的首发是 230 ⇒ 该玩家初值 1；232 的首发是 234 ⇒ 初值 0。
        manager.players.get_mut(&229).unwrap().turns = PlayerTurns::with_index(vec![230, 231], 1);

        let mut got = Vec::new();
        for &expect in captured.iter() {
            // 玩家顺序沿用抓包（上一发属于谁，下一发就换另一个玩家）
            let player_id = player_of(expect);
            got.push(manager.begin_turn(player_id, all_alive, false).soldier_id().unwrap());
        }
        assert_eq!(got, captured);
    }

    // -----------------------------------------------------------------------
    // 回合超时 = 跳过该玩家（用户口述，未独立验证）
    // -----------------------------------------------------------------------

    /// 超时 ⇒ `Skip(TurnTimeout)`，且**不推进游标**（这一条是实现选择，游戏里未知）。
    #[test]
    fn timeout_skips_the_player_without_advancing_the_cursor() {
        let mut manager = TurnManager::new();
        manager.add_player(1, Team::One, vec![10, 20, 30]);

        let timed_out = manager.begin_turn(1, all_alive, true);
        assert_eq!(timed_out, TurnDecision::Skip { player_id: 1, reason: SkipReason::TurnTimeout });
        assert_eq!(timed_out.broadcast(), None, "超时不发 TurnInfo");
        assert_eq!(timed_out.flip(), None);
        assert_eq!(manager.player(1).unwrap().turns.index(), 0, "超时不动游标");

        // 下一回合正常轮到该玩家时，仍按原游标取 —— 等于"这回合没轮到过他"。
        let normal = manager.begin_turn(1, all_alive, false);
        assert_eq!(normal.soldier_id(), Some(20));
        assert_eq!(manager.player(1).unwrap().turns.index(), 1);
    }

    #[test]
    fn unknown_player_is_reported() {
        let mut manager = TurnManager::new();
        let d = manager.begin_turn(999, all_alive, false);
        assert_eq!(d, TurnDecision::Skip { player_id: 999, reason: SkipReason::UnknownPlayer });
        assert_eq!(manager.flip_for(999), None);
        assert_eq!(manager.next_shooter(999, all_alive), None);
    }

    // -----------------------------------------------------------------------
    // 存活判定（life_manager 状态字节）
    // -----------------------------------------------------------------------

    /// 只有 `& 1` 为真才算活：0（死）与 2（查不到）都不可开火。
    #[test]
    fn state_byte_low_bit_decides_life() {
        assert!(!is_alive_byte(0));
        assert!(is_alive_byte(1));
        assert!(!is_alive_byte(MISSING_STATE), "查不到 = 2 ⇒ 不可开火");
        assert!(is_alive_byte(3), "任何奇数都算活（只看最低位）");
        assert!(is_alive_byte(0xFF));

        let mut p = PlayerTurns::new(vec![1, 2, 3]);
        // 1 号是 3（奇=活）⇒ 直接选中；2 号是 2（查不到）、3 号是 0（死）都不选。
        let states = |id: i64| match id {
            1 => 3,
            2 => MISSING_STATE,
            _ => 0,
        };
        assert_eq!(p.next_shooter_by_state(states), Some(1));
    }

    /// `LifeState` 适配器：Alive → 1、Dead → 0、没记录 → 2。
    #[test]
    fn life_state_adapter_maps_missing_to_two() {
        let mut life = LifeState::default();
        life.set(5, Status::Alive, None);
        life.set(6, Status::Dead, None);
        let f = life_state_of(&life);
        assert_eq!(f(5), 1);
        assert_eq!(f(6), 0);
        assert_eq!(f(7), MISSING_STATE);

        let mut p = PlayerTurns::new(vec![6, 7, 5]);
        assert_eq!(p.next_shooter_by_state(f), Some(5), "死人与查不到的都要跳过");
    }

    /// `soldier_id == 0` 不能当真实士兵（会与"全灭"混淆）。
    #[test]
    #[should_panic(expected = "全灭")]
    fn zero_is_rejected_as_a_soldier_id() {
        let _ = PlayerTurns::new(vec![1, 0]);
    }
}
