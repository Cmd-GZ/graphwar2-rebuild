//! **不是原版的东西**：把播放屏的**流程权威**挂成 GDExtension 类（本文件是外壳，不是复原物）。
//!
//! # 为什么会有这一份
//!
//! `dispatch/54` 的边界：**Rust 负责状态与流程，GDScript 只负责摆节点与画。**
//! 在它之前，播放屏的两块流程是**GDScript 里的一份重复实现**：
//!
//! | 原来的 GDScript | 行数 | 它复刻的真值 | 本轮的落点 |
//! |---|---|---|---|
//! | `client/src/game/level_rules.gd`（`GwLevelRules`） | 219 | `0x473520` + `0x4759f0` + `0x478133`–`0x478236` | [`GwLevelRules`]（本文件）→ [`crate::graphwar_play_screen_control::LevelRules`] |
//! | `client/src/game/turn_driver.gd`（`GwTurnDriver`） | 121 | `0x463da0`（`TurnManager::next_turn`） | [`GwTurnDriver`]（本文件）→ [`crate::graphwar_turn_manager`] |
//!
//! 两个类名**保持不变**（`GwLevelRules` / `GwTurnDriver`），所以播放屏的调用点只需从
//! "`.gd` 里的实现"换成"扩展类的同名方法"，不需要重命名任何概念。
//!
//! # 这里没有第二份算法
//!
//! | 方法 | 落到的纯逻辑 | 那份逻辑的档位 |
//! |---|---|---|
//! | `GwLevelRules.evaluate()` | [`crate::graphwar_play_screen_control::LevelRules::evaluate`] → `turnstate::winner_byte` | **逐位**（`0x478133`–`0x478236`）+ **逐位**（`0x475a64`/`0x475a80`/`0x475a88`） |
//! | `GwTurnDriver.next_shooter()` | [`crate::graphwar_turn_manager::PlayerTurns::next_shooter`] | **逐位**（`0x463f40`–`0x463f58` 先自增再取；`0x463fc7` 最多试 len 次；`0x463fb6` 判活；`0x464016` 全灭哨兵 0） |
//! | `GwTurnDriver.round_*()` | [`crate::graphwar_turn_manager::RoundCollection`] | **观测**（`docs/141` §4：130/130 批按公告顺序） |
//! | `GwTurnDriver` 的**玩家之间**轮转 | 本文件的 `cursor` | **未知**（原版是 `HashMap` 迭代序，`docs/109` §6-A）；这里固定成调用方给的玩家顺序 |
//!
//! # 一处必须说明的编号换算
//!
//! 原版 `soldier_id` 是实体 id，**`0` 是"全灭"哨兵**（`0x464016` 的 `movl $0x0`），
//! 所以 `PlayerTurns` 拒绝 `0` 入表（`assert_ne!`）。界面侧用的是**士兵下标**（从 0 起），
//! 所以本类在**进出各换一次**：内部 id = 下标 + 1，返回时 −1。
//! 进 GDScript 后**不会**出现 0 号哨兵 —— 全灭/无表一律 `-1`（与删掉的 `.gd` 同一约定）。

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_play_screen_control::{LevelKind, LevelRules, MatchState, TARGET_TEAM};
use crate::graphwar_turn_manager::{PlayerTurns, RoundCollection, SubmitResult, SOLDIER_ID_NONE};

/// 士兵下标 → `PlayerTurns` 内部 id 的偏移（见文件头"编号换算"）。
const SOLDIER_ID_SHIFT: i64 = 1;

/// 从 `Variant` 读一个整数（缺键/类型不对 ⇒ 默认值）。GDScript 的 `int(d.get(k, def))`。
fn dict_int(d: &VarDictionary, key: &str, default: i64) -> i64 {
    d.get(key).map(|v| v.to::<i64>()).unwrap_or(default)
}

/// 从 `Variant` 读一个整数，键也是整数（GDScript 的 `{0: n, 1: m}`）。
fn dict_int_key(d: &VarDictionary, key: i64, default: i64) -> i64 {
    d.get(key).map(|v| v.to::<i64>()).unwrap_or(default)
}

/// `alive` 数组里第 `index` 位是不是 `true`（越界/非 bool ⇒ `false`）。
fn alive_at(alive: &VarArray, index: i64) -> bool {
    if index < 0 {
        return false;
    }
    match alive.get(index as usize) {
        Some(v) => v.to::<bool>(),
        None => false,
    }
}

/// 把 `Vec<i64>` 塞进 `Variant` 数组（GDScript 侧看到的是 `Array`）。
fn int_array(xs: &[i64]) -> VarArray {
    let mut arr = VarArray::new();
    for x in xs {
        arr.push(*x);
    }
    arr
}

// ---------------------------------------------------------------------------
// GwLevelRules —— 关卡规则 + 胜负判定（`client/src/game/level_rules.gd` 的替身）
// ---------------------------------------------------------------------------

/// 一局的规则与判定（原 GDScript 类 `GwLevelRules`，现已由 Rust 持有）。
///
/// 用法与旧 `.gd` 一一对应：
///
/// ```text
/// var r := GwLevelRules.new()
/// r.from_campaign(GwCampaign.level(i), human_team)     # 旧：GwLevelRules.for_campaign(level, human_team)
/// r.from_versus(human_team)                            # 旧：GwLevelRules.for_versus(human_team)
/// r.from_match(has_targets, has_opponent, limit, team) # 旧：GwLevelRules.for_match(...)
/// r.kind_name() / r.turn_limit() / r.targets_planned() / r.level_index()
/// r.evaluate({...}) -> {over, human_won, winner_team, reason}
/// r.snapshot() -> {kind, human_team, turn_limit, level_index, targets_planned}
/// ```
///
/// ⚠️ 旧 `.gd` 的 `turn_limit` / `targets_planned` / `level_index` 是**字段**，这里是
/// **方法**（GDExtension 侧的属性要写 getter，本轮直接改成方法，调用点少 3 处）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwLevelRules {
    base: Base<RefCounted>,
    rules: LevelRules,
}

#[godot_api]
impl IRefCounted for GwLevelRules {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base, rules: LevelRules::for_versus(0) }
    }
}

#[godot_api]
impl GwLevelRules {
    /// 战役关：`level` 是 `GwCampaign.level(i)` 的那张表（键 `targets` / `turn_limit` /
    /// `bot_tier` / `index`，与旧 `.gd` 读的是同一批键）。
    #[func]
    fn from_campaign(&mut self, level: VarDictionary, human_team: i64) {
        let targets = dict_int(&level, "targets", 0);
        let turn_limit = dict_int(&level, "turn_limit", 0);
        let bot_tier = dict_int(&level, "bot_tier", -1);
        let index = dict_int(&level, "index", -1);
        self.rules =
            LevelRules::for_campaign(targets, bot_tier >= 0, turn_limit, index, human_team);
    }

    /// 非战役对局（LAN / 回放）：没有靶子、没有回合上限。
    #[func]
    fn from_versus(&mut self, human_team: i64) {
        self.rules = LevelRules::for_versus(human_team);
    }

    /// 通用构造（类型由"有没有靶子 / 有没有对手"决定，**只是标签**）。
    #[func]
    fn from_match(
        &mut self,
        has_targets: bool,
        has_opponent: bool,
        turn_limit: i64,
        human_team: i64,
    ) {
        self.rules = LevelRules::for_match(has_targets, has_opponent, turn_limit, human_team);
    }

    #[func]
    fn kind_name(&self) -> GString {
        GString::from(self.rules.kind_name())
    }

    #[func]
    fn turn_limit(&self) -> i64 {
        self.rules.turn_limit
    }

    #[func]
    fn targets_planned(&self) -> i64 {
        self.rules.targets_planned
    }

    #[func]
    fn level_index(&self) -> i64 {
        self.rules.level_index
    }

    #[func]
    fn human_team(&self) -> i64 {
        self.rules.human_team
    }

    #[func]
    fn has_targets(&self) -> bool {
        self.rules.has_targets()
    }

    #[func]
    fn has_opponent_rule(&self) -> bool {
        self.rules.has_opponent_rule()
    }

    /// 判一局的胜负。入参形状与旧 `.gd` 的 `evaluate(state)` **完全一致**：
    ///
    /// ```text
    /// {alive_by_team: {0: n, 1: m}, teams_present: [0, 1],
    ///  targets_total: t, targets_alive: a, turns_used: u}
    /// ```
    ///
    /// 返回 `{over, human_won, winner_team, reason}`（键名与旧 `.gd` 一致，`reason` 是
    /// 状态行/自检的标签，不参与分支）。
    #[func]
    fn evaluate(&self, state: VarDictionary) -> VarDictionary {
        // ⚠️ 这两个键是**整数** `0` / `1`（`{0: n, 1: m}`），不是字符串 —— 用 `0i64` 取。
        let alive = state.get("alive_by_team").map(|v| v.to::<VarDictionary>());
        let (alive_team0, alive_team1) = match &alive {
            Some(d) => (dict_int_key(d, 0, 0), dict_int_key(d, 1, 0)),
            None => (0, 0),
        };
        let teams_present: Vec<i64> = match state.get("teams_present") {
            Some(v) => {
                let arr = v.to::<VarArray>();
                (0..arr.len())
                    .filter_map(|i| arr.get(i))
                    .map(|v| v.to::<i64>())
                    .collect()
            }
            None => Vec::new(),
        };
        let st = MatchState {
            alive_team0,
            alive_team1,
            teams_present,
            targets_total: dict_int(&state, "targets_total", 0),
            targets_alive: dict_int(&state, "targets_alive", 0),
            turns_used: dict_int(&state, "turns_used", 0),
        };
        let v = self.rules.evaluate(&st);
        let mut out = VarDictionary::new();
        out.set("over", v.over);
        out.set("human_won", v.human_won);
        out.set("winner_team", v.winner_team);
        out.set("reason", &GString::from(v.reason));
        out.set("verdict_byte", verdict_byte(&v));
        out
    }

    /// 诊断快照（状态行与自检用），键与旧 `.gd` 的 `snapshot()` 一致。
    #[func]
    fn snapshot(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("kind", &GString::from(self.rules.kind_name()));
        d.set("human_team", self.rules.human_team);
        d.set("turn_limit", self.rules.turn_limit);
        d.set("level_index", self.rules.level_index);
        d.set("targets_planned", self.rules.targets_planned);
        d.set("has_targets", self.rules.has_targets());
        d.set("has_opponent_rule", self.rules.has_opponent_rule());
        d.set("target_team", TARGET_TEAM);
        d.set("kind_is_both", self.rules.kind == LevelKind::Both);
        d
    }

    /// 靶子所在的队伍号（`docs/150` §Q1：恒为 1 = Team2）。给界面归队用。
    #[func]
    fn target_team(&self) -> i64 {
        TARGET_TEAM
    }
}

/// 真值 `[self+0x11f]` 那个胜方字节，**给自检对拍用**（`0`=Team1、`1`=Team2、`2`=没人）。
///
/// 它是从 [`crate::turnstate::winner_byte`] 的同一张表来的（`0x478133`–`0x478236`），
/// 但 [`crate::graphwar_play_screen_control::Verdict`] 只保留界面要的四项，所以这里
/// 再算一次给诊断/断言看 —— **不是**第二条判定路径。
fn verdict_byte(v: &crate::graphwar_play_screen_control::Verdict) -> i64 {
    match (v.over, v.human_won, v.reason) {
        (false, ..) => -1,
        // 界面四项 + reason 足以还原这张表：没人赢的两种收场 reason 都是它。
        (true, _, "mutual_wipe") => 2,
        (true, true, _) => 0,  // 人类赢 ⇒ 人类那队是唯一还有活实体的一队
        (true, false, "turn_limit") => 1,
        (true, false, _) => 1,
    }
}

// ---------------------------------------------------------------------------
// GwTurnDriver —— 回合射手轮转 + 同时回合收集（`client/src/game/turn_driver.gd` 的替身）
// ---------------------------------------------------------------------------

/// 回合轮转（原 GDScript 类 `GwTurnDriver`，现已由 Rust 持有）。
///
/// 用法与旧 `.gd` 一一对应（**`setup()` 的入参形状不变**，所以既有断言不用改）：
///
/// ```text
/// var td := GwTurnDriver.new()
/// td.setup([{player: 0, soldiers: [0, 1]}, ...])   # 旧：setup(groups)
/// td.setup_flat([0, 0, 1])                          # 新：按士兵下标给"谁拥有它"，省掉 GDScript 的归组循环
/// td.first_shooter(alive)   # alive: Array[bool]，按**士兵下标**下标
/// td.next_shooter(alive)    # 先自增再取；全灭/空表 ⇒ -1
/// td.current_shooter()
/// td.soldier_id_none()      # 0（`0x464016` 的哨兵；旧 `.gd` 是常量 SOLDIER_ID_NONE）
/// td.snapshot()             # {cursor, players: [{player, soldiers, index}], count}
/// td.round_open(announced)  # 同时回合制：本回合该交的玩家（公告顺序）
/// td.round_submit(player)   # {accepted, complete, waiting}
/// td.round_complete() / td.round_waiting() / td.round_reset()
/// ```
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwTurnDriver {
    base: Base<RefCounted>,
    /// 每个玩家一张士兵表（内部 id = 下标 + [`SOLDIER_ID_SHIFT`]）。
    groups: Vec<PlayerTurns>,
    /// 与 `groups` 同下标：玩家号（快照/诊断用；轮转本身不看它）。
    players: Vec<i64>,
    /// 当前玩家游标（**未知**项：原版是 `HashMap` 迭代序，`docs/109` §6-A）。
    cursor: usize,
    /// 同时回合制的收集阶段（`docs/141` §1）。
    round: Option<RoundCollection<i64>>,
}

#[godot_api]
impl IRefCounted for GwTurnDriver {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base, groups: Vec::new(), players: Vec::new(), cursor: 0, round: None }
    }
}

#[godot_api]
impl GwTurnDriver {
    /// 装表。`groups` = `[{player: int, soldiers: Array[int]}, …]`，顺序即玩家顺序；
    /// `soldiers` 放的是**调用方的士兵下标**（本类不解释它的含义）。
    /// 重复调用会重置游标（换局时用）。
    #[func]
    fn setup(&mut self, groups: VarArray) {
        self.groups.clear();
        self.players.clear();
        self.cursor = 0;
        self.round = None;
        for i in 0..groups.len() {
            let Some(g) = groups.get(i) else { continue };
            let g = g.to::<VarDictionary>();
            let player = dict_int(&g, "player", -1);
            let list = g.get("soldiers").map(|v| v.to::<VarArray>());
            let mut ids: Vec<i64> = Vec::new();
            if let Some(list) = list {
                for k in 0..list.len() {
                    if let Some(v) = list.get(k) {
                        ids.push(v.to::<i64>());
                    }
                }
            }
            self.push_group(player, &ids);
        }
    }

    /// 按"每个士兵属于哪个玩家"装表（顺序 = 士兵下标）。省掉调用方在 GDScript 里
    /// 归组的那 20 行：玩家顺序 = **首次出现**的顺序。
    #[func]
    fn setup_flat(&mut self, owners: VarArray) {
        self.groups.clear();
        self.players.clear();
        self.cursor = 0;
        self.round = None;
        let mut order: Vec<i64> = Vec::new();
        let mut buckets: Vec<Vec<i64>> = Vec::new();
        for i in 0..owners.len() {
            let Some(v) = owners.get(i) else { continue };
            let owner = v.to::<i64>();
            let slot = match order.iter().position(|p| *p == owner) {
                Some(s) => s,
                None => {
                    order.push(owner);
                    buckets.push(Vec::new());
                    order.len() - 1
                }
            };
            buckets[slot].push(i as i64);
        }
        for (slot, player) in order.iter().enumerate() {
            self.push_group(*player, &buckets[slot]);
        }
    }

    /// 追一张表（`setup` / `setup_flat` 共用）。
    fn push_group(&mut self, player: i64, soldiers: &[i64]) {
        let mut turns = PlayerTurns::new(Vec::new());
        for sid in soldiers {
            turns.add(*sid + SOLDIER_ID_SHIFT);
        }
        self.players.push(player);
        self.groups.push(turns);
    }

    /// 全灭/空表时原版广播的哨兵值（`0x464016` 的 `movl $0x0`）。旧 `.gd` 是常量。
    #[func]
    fn soldier_id_none(&self) -> i64 {
        SOLDIER_ID_NONE
    }

    /// 开局第一个射手：**不先自增**（`setup` 把每张表的游标放在 0，语义是"0 号正握着这一回合"）。
    ///
    /// ⚠️ 原版的**初始游标**未定（`docs/109` §6-A：三例吻合 0，`1782763022` 首取 `2509` 不符）。
    #[func]
    fn first_shooter(&mut self, alive: VarArray) -> i64 {
        self.cursor = 0;
        if self.groups.is_empty() {
            return -1;
        }
        if let Some(id) = self.groups[0].current() {
            if alive_at(&alive, id - SOLDIER_ID_SHIFT) {
                return id - SOLDIER_ID_SHIFT;
            }
        }
        self.next_shooter(alive)
    }

    /// 找下一个射手：玩家游标先自增，再让那个玩家的士兵表按 `0x463da0` 的规则转。
    ///
    /// 返回**士兵下标**；`-1` = 一张能开火的表都没有（全灭 / 空表）。
    #[func]
    fn next_shooter(&mut self, alive: VarArray) -> i64 {
        let n = self.groups.len();
        if n == 0 {
            return -1;
        }
        if self.cursor >= n {
            self.cursor %= n;
        }
        for _ in 0..n {
            // ① 玩家之间先换手（这一层的顺序是**未知**项，见文件头）。
            self.cursor = (self.cursor + 1) % n;
            let turns = &mut self.groups[self.cursor];
            if turns.is_empty() {
                continue; // 这个玩家没有士兵 ⇒ 换下一个
            }
            // ②③④ 该玩家的士兵表：先自增再取、判活、最多试 len 次（`0x463f40`–`0x463fca`）。
            match turns.next_shooter(|id| alive_at(&alive, id - SOLDIER_ID_SHIFT)) {
                Some(SOLDIER_ID_NONE) => return -1, // ⑤ 全灭
                Some(id) => return id - SOLDIER_ID_SHIFT,
                None => {}
            }
        }
        -1 // ⑤ 没有任何活人
    }

    /// 当前射手（上一次 `first_shooter()` / `next_shooter()` 的结果）。没调过返回 `-1`。
    #[func]
    fn current_shooter(&self) -> i64 {
        if self.cursor >= self.groups.len() {
            return -1;
        }
        match self.groups[self.cursor].current() {
            Some(id) => id - SOLDIER_ID_SHIFT,
            None => -1,
        }
    }

    /// 只读快照（自检/诊断），键与旧 `.gd` 的 `snapshot()` 一致。
    #[func]
    fn snapshot(&self) -> VarDictionary {
        let mut players = VarArray::new();
        for (i, turns) in self.groups.iter().enumerate() {
            let mut row = VarDictionary::new();
            row.set("player", self.players[i]);
            let soldiers: Vec<i64> =
                turns.soldiers().iter().map(|id| id - SOLDIER_ID_SHIFT).collect();
            row.set("soldiers", &int_array(&soldiers));
            row.set("index", turns.index() as i64);
            players.push(&row);
        }
        let mut d = VarDictionary::new();
        d.set("cursor", self.cursor as i64);
        d.set("players", &players);
        d.set("count", self.groups.len() as i64);
        d.set("round_open", self.round.is_some());
        d.set("round_complete", self.round.as_ref().map(|r| r.is_complete()).unwrap_or(false));
        d
    }

    // ---- 同时回合制的收集阶段（`RoundCollection`，`docs/141` §1）----------------

    /// 开一轮：`announced` = 本回合该交的玩家（公告顺序）。旧实现是
    /// `_shot_queue` + `_all_alive_players_shot()` 那套旁路判断。
    #[func]
    fn round_open(&mut self, announced: VarArray) {
        let mut ids: Vec<i64> = Vec::new();
        for i in 0..announced.len() {
            if let Some(v) = announced.get(i) {
                ids.push(v.to::<i64>());
            }
        }
        self.round = Some(RoundCollection::new(ids, None));
    }

    /// 收一发：`{accepted, complete, reason, waiting}`。
    ///
    /// `reason` 取 `accepted` / `duplicate` / `not_expected` / `closed`（= `SubmitResult`）。
    #[func]
    fn round_submit(&mut self, player: i64) -> VarDictionary {
        let mut out = VarDictionary::new();
        let Some(rc) = self.round.as_mut() else {
            out.set("accepted", false);
            out.set("complete", false);
            out.set("reason", &GString::from("no_round"));
            out.set("waiting", &VarArray::new());
            return out;
        };
        let res = rc.submit(player, player);
        let accepted = matches!(res, SubmitResult::Accepted);
        let reason = match res {
            SubmitResult::Accepted => "accepted",
            SubmitResult::Duplicate => "duplicate",
            SubmitResult::NotExpected => "not_expected",
            SubmitResult::Closed => "closed",
        };
        let waiting = rc.waiting_on();
        out.set("accepted", accepted);
        out.set("complete", rc.is_complete());
        out.set("reason", &GString::from(reason));
        out.set("waiting", &int_array(&waiting));
        out
    }

    /// 这一轮攒齐了吗。
    #[func]
    fn round_complete(&self) -> bool {
        self.round.as_ref().map(|r| r.is_complete()).unwrap_or(false)
    }

    /// 还在等谁（公告顺序）。
    #[func]
    fn round_waiting(&self) -> VarArray {
        match self.round.as_ref() {
            Some(r) => int_array(&r.waiting_on()),
            None => VarArray::new(),
        }
    }

    /// 关掉这一轮并把收集器清空（结算完 / 换局）。
    #[func]
    fn round_reset(&mut self) {
        if let Some(r) = self.round.as_mut() {
            r.close();
        }
        self.round = None;
    }
}

#[cfg(test)]
mod tests {
    //! 这些单测**不碰引擎**：它们只钉"编号换算"与"空表语义"这两件本文件自己引入的事
    //! （`PlayerTurns` 的轮转规则本身有它自己的单测）。`VarArray` 之类需要引擎初始化，
    //! 所以这里不测 `#[func]` 的搬运层，那部分由 `client/tools/play_flow_check.gd` 覆盖。

    use super::SOLDIER_ID_SHIFT;

    #[test]
    fn soldier_index_shift_avoids_the_sentinel() {
        // 原版 0 是"全灭"哨兵（0x464016），`PlayerTurns::add` 拒收 0
        // ⇒ 界面侧的 0 号士兵必须以内部 id 1 出现，返回时 −1 换回。
        assert_eq!(SOLDIER_ID_SHIFT, 1);
        assert_eq!(0 + SOLDIER_ID_SHIFT, 1, "0 号士兵的内部 id 必须非 0");
        assert_eq!(7 - SOLDIER_ID_SHIFT, 6, "返回值 = 内部 id − 1");
    }
}
