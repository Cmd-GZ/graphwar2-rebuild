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
//! 第三块（T31 本轮）不是"某份 `.gd` 的替身"，而是**本屏从来就没有过的那个东西**：
//! 原版的**对局状态机**（8 个 `GameState`，`[self+0x120]` 那一个字节）。播放屏在此之前
//! 只有一列 `_shot_in_flight` 布尔当投影 ⇒ [`GwTurnState`]（本文件）→ [`crate::turnstate::next_state`]。
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
//! | `GwShotFlow.judge()` | [`crate::graphwar_shot_calculator::settle::resolve_points_owned`] | **逐位**（规则本体；射手按 id 排除 = `0x3c4452`/`0x3c3e81`） |
//! | `GwTurnState.apply()` / `can_fire()` | [`crate::turnstate::next_state`] / `GameState::index` | 状态字节 **逐位**（变体表 `.data.rel.ro 0x9586d0`、`set_state` `0x4780cc`、`cmp BYTE [self+0x120],2` `0x470989`）；迁移序列 **观测**（真报文 `GameStateInfo`） |
//!
//! # 一处必须说明的编号换算
//!
//! 原版 `soldier_id` 是实体 id，**`0` 是"全灭"哨兵**（`0x464016` 的 `movl $0x0`），
//! 所以 `PlayerTurns` 拒绝 `0` 入表（`assert_ne!`）。界面侧用的是**士兵下标**（从 0 起），
//! 所以本类在**进出各换一次**：内部 id = 下标 + 1，返回时 −1。
//! 进 GDScript 后**不会**出现 0 号哨兵 —— 全灭/无表一律 `-1`（与删掉的 `.gd` 同一约定）。

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_game_state_manager::{PlannedKill, ShotEvent, ShotLifecycle};
use crate::graphwar_play_screen_control::{
    BotTurn, LevelKind, LevelRules, MatchState, TurnAction, TurnClock, TARGET_TEAM,
};
use crate::graphwar_shot_calculator::settle::{resolve_points_owned, Target};
use crate::graphwar_turn_manager::{PlayerTurns, RoundCollection, SubmitResult, SOLDIER_ID_NONE};
use crate::turnstate::{GameState, TurnEvent};

/// 士兵下标 → `PlayerTurns` 内部 id 的偏移（见文件头"编号换算"）。
const SOLDIER_ID_SHIFT: i64 = 1;

/// 靶子 id 在 `Target` 表里的上偏移（区分"士兵下标"与"靶子 id"）。
///
/// 数值与 `client/src/screens/play_screen.gd` 的 `TARGET_ID_BASE` **同值**（1000），
/// 但两处用途不同：界面上它给**实体 id** 编不撞的号；这里只用来把
/// [`crate::graphwar_shot_calculator::settle::Target`] 的一张表切成两类。
/// 它是我方的映射约定，**不是**原版常量。
const TARGET_ID_BASE: i64 = 1000;

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

    /// `GwTurnState` 的字符串事件表必须覆盖 `TurnEvent` 的**每一个**变体。
    ///
    /// 这一条防的是"Rust 侧加了变体、GDScript 侧的名字表没跟上"：
    /// 迁移表本体在 `turnstate::next_state`（有它自己的真值序列测试），这里只钉
    /// **名字 → 变体**这一层，纯逻辑、不碰引擎。
    #[test]
    fn turn_event_names_cover_every_variant() {
        use crate::turnstate::TurnEvent;
        let all = [
            ("fire", TurnEvent::Fire),
            ("results_computed", TurnEvent::ResultsComputed),
            ("activate", TurnEvent::Activate),
            ("turn_ended", TurnEvent::TurnEnded),
            ("game_ended", TurnEvent::GameEnded),
            ("game_started", TurnEvent::GameStarted),
            ("reset", TurnEvent::Reset),
        ];
        for (name, ev) in all {
            assert_eq!(parse_turn_event(name), Some(ev), "事件名 `{name}` 必须映射到 {ev:?}");
        }
        assert_eq!(parse_turn_event("nope"), None, "不认识的名字必须给 None（不改状态）");
        // 反向：每个 `TurnEvent` 变体都得有一个名字（少一个就等于 GDScript 永远发不出它）。
        for ev in all.iter().map(|(_, e)| *e) {
            assert!(
                all.iter().any(|(n, e)| *e == ev && parse_turn_event(n) == Some(ev)),
                "{ev:?} 没有对应的名字"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// GwShotFlow —— 弹道生命周期（`play_screen.gd` 那 6 个函数的"决定"部分）
// ---------------------------------------------------------------------------
//
// **不是原版的东西**：原版没有这个类；它是"把 `_begin_shot` / `_advance_presentation` /
// `_apply_kills_up_to` / `_on_shot_arrived` / `_finish_round` / `flush_shot` 的**判断**
// 挪到 Rust"之后留下的那台状态机外壳。逻辑本体在
// [`crate::graphwar_game_state_manager::ShotLifecycle`]（2026-11-26 按 `Location` 证据搬家的落点）。
//
// 视图层只剩三件事：递数据（`begin`）、递"画到第几个点"（`advance`）、执行事件。

/// 一发弹道的生命周期（原版 `handle_tick` + `execute_results` 的状态推进）。
///
/// ```text
/// var f := GwShotFlow.new()
/// f.begin(total_points, judged_kills, judged_stop, false)   # 开火
/// var events := f.advance(reveal)                           # 每帧
/// for e in events: ...                                      # 只按 kind 摆节点
/// var s := f.settle()                                       # {settled, turns_used}
/// f.reset()                                                 # 换局
/// ```
///
/// `kills` 的每一项（由播放屏的命中表给）：
/// `{kind: "soldier"/"target", point: int, index: int, target_id: int, pos: Vector2}`。
/// 返回的事件：
/// `{kind: "soldier_killed", point, index, pos}` /
/// `{kind: "target_killed", point, target_id, pos}` /
/// `{kind: "arrived", stop}`。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwShotFlow {
    base: Base<RefCounted>,
    flow: ShotLifecycle,
}

#[godot_api]
impl IRefCounted for GwShotFlow {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base, flow: ShotLifecycle::new() }
    }
}

/// 从 Variant 取一个 `Vector2`（缺键/类型不对 ⇒ `(0.0, 0.0)`）。
fn vec2_of(v: Option<Variant>) -> (f64, f64) {
    match v {
        Some(v) => {
            let p = v.to::<Vector2>();
            (p.x as f64, p.y as f64)
        }
        None => (0.0, 0.0),
    }
}

/// `(f64, f64)` → `Vector2`（Godot 侧是 f32 构建，与手写层同样掉一遍精度）。
fn to_vec2(p: (f64, f64)) -> Vector2 {
    Vector2::new(p.0 as f32, p.1 as f32)
}

/// 一条命中记录 → [`PlannedKill`]（`kind` 缺省按 `soldier`）。
fn planned_kill_of(d: &VarDictionary) -> PlannedKill {
    let point = dict_int(d, "point", 0).max(0) as usize;
    let pos = vec2_of(d.get("pos"));
    let kind = d.get("kind").map(|v| v.to::<GString>().to_string()).unwrap_or_default();
    if kind == "target" {
        PlannedKill::target(point, dict_int(d, "target_id", -1), pos)
    } else {
        PlannedKill::soldier(point, dict_int(d, "index", -1), pos)
    }
}

/// 一张命中表 → [`PlannedKill`] 列表。
fn planned_kills_of(list: &VarArray) -> Vec<PlannedKill> {
    let mut out: Vec<PlannedKill> = Vec::new();
    for i in 0..list.len() {
        if let Some(v) = list.get(i) {
            out.push(planned_kill_of(&v.to::<VarDictionary>()));
        }
    }
    out
}

/// [`ShotEvent`] → GDScript 字典。
fn event_dict(e: &ShotEvent) -> VarDictionary {
    let mut d = VarDictionary::new();
    match e {
        ShotEvent::SoldierKilled { point, index, pos } => {
            d.set("kind", &GString::from("soldier_killed"));
            d.set("point", *point as i64);
            d.set("index", *index);
            d.set("pos", to_vec2(*pos));
        }
        ShotEvent::TargetKilled { point, target_id, pos } => {
            d.set("kind", &GString::from("target_killed"));
            d.set("point", *point as i64);
            d.set("target_id", *target_id);
            d.set("pos", to_vec2(*pos));
        }
        ShotEvent::Arrived { stop } => {
            d.set("kind", &GString::from("arrived"));
            d.set("stop", to_vec2(*stop));
        }
    }
    d
}

#[godot_api]
impl GwShotFlow {
    /// 换局：连回合数一起归零。
    #[func]
    fn reset(&mut self) {
        self.flow.reset();
    }

    /// 开一发。`already_settled = true` 用于压平模式（automation）与联机（本地不结算）。
    ///
    /// `kills` / `target_kills` 是**两张表**（士兵的与靶子的），与命中表同形。
    #[func]
    fn begin(
        &mut self,
        total_points: i64,
        kills: VarArray,
        target_kills: VarArray,
        stop: Vector2,
        already_settled: bool,
    ) {
        let mut planned = planned_kills_of(&kills);
        let targets = planned_kills_of(&target_kills);
        self.flow.begin(
            total_points.max(0) as usize,
            std::mem::take(&mut planned),
            targets,
            (stop.x as f64, stop.y as f64),
            already_settled,
        );
    }

    /// 推进到"画了 `reveal` 个点"，返回这一帧该执行的事件（顺序 = 弹道先后）。
    #[func]
    fn advance(&mut self, reveal: i64) -> VarArray {
        let reveal = if reveal < 0 { 0 } else { reveal as usize };
        let events = self.flow.advance(reveal);
        let mut out = VarArray::new();
        for e in &events {
            out.push(&event_dict(e));
        }
        out
    }

    /// 把在飞的那一发直接跑完（自检/诊断用）。
    #[func]
    fn flush(&mut self) -> VarArray {
        let events = self.flow.flush();
        let mut out = VarArray::new();
        for e in &events {
            out.push(&event_dict(e));
        }
        out
    }

    /// 把还没落地的命中全部落地，**不吐到达事件**（"一次结算一整批弹道"那条路：
    /// 同时回合攒齐 / 外部来弹。那一批里没有单条弹道在飞）。
    #[func]
    fn land_all(&mut self) -> VarArray {
        let events = self.flow.land_all();
        let mut out = VarArray::new();
        for e in &events {
            out.push(&event_dict(e));
        }
        out
    }

    /// 收一个回合。**同一发只收一次**（`{settled, turns_used}`）。
    #[func]
    fn settle(&mut self) -> VarDictionary {
        let o = self.flow.settle();
        let mut d = VarDictionary::new();
        d.set("settled", o.settled);
        d.set("turns_used", o.turns_used);
        d
    }

    /// 半路丢弃（换局 / 联机断线）。
    #[func]
    fn abort(&mut self) {
        self.flow.abort();
    }

    #[func]
    fn in_flight(&self) -> bool {
        self.flow.in_flight()
    }

    #[func]
    fn arrived(&self) -> bool {
        self.flow.arrived()
    }

    #[func]
    fn revealed(&self) -> i64 {
        self.flow.revealed() as i64
    }

    #[func]
    fn total_points(&self) -> i64 {
        self.flow.total_points() as i64
    }

    #[func]
    fn pending_kills(&self) -> i64 {
        self.flow.pending_soldiers() as i64
    }

    #[func]
    fn pending_targets(&self) -> i64 {
        self.flow.pending_targets() as i64
    }

    #[func]
    fn kills_landed(&self) -> i64 {
        self.flow.kills_landed()
    }

    #[func]
    fn targets_landed(&self) -> i64 {
        self.flow.targets_landed()
    }

    #[func]
    fn settled(&self) -> bool {
        self.flow.settled()
    }

    #[func]
    fn turns_used(&self) -> i64 {
        self.flow.turns_used()
    }

    /// 一次开火的**命中表**（原版 `SynchronizingResults`：服务端在开火那一刻就算完结果）。
    ///
    /// 这一格原来在 `.gd` 的 `_judge_shots()` 里 —— 它自己建实体表、自己调内核
    /// `hit_test`、再**用同一个判据重算一遍点序号**（`_first_point_within()` 的注释
    /// 写着"与内核判死是同一条规则…所以这里按同一个判据再算一次序号"）。
    /// 现在判定本体只走 [`crate::graphwar_shot_calculator::settle::resolve_points_owned`]
    /// **一份**，它回的 `Hit{index, arc}` 就是"第几点打中的"，不需要第二次扫描。
    ///
    /// 与 `.gd` 旧实现的三处**语义对齐**（不是改写，是照原版指令）：
    ///
    /// 1. **射手按实体 id 排除**：`0x3c4452`（有弹道点的分支）/ `0x3c3e81`（空弹道分支）
    ///    比较的都是开火记录 `+0x18` 的射手 id；旧 `.gd` 用的是 `is_same()` 对象同一性
    ///    （界面侧没有实体 id，只能拿字典引用比）。这里改成传**士兵下标**当 id。
    /// 2. **死者不进表**：命中表是开火那一刻对着存活表算的（原版同样在 `SynchronizingResults`）。
    /// 3. **`stop` 取弹道实际停点**（撞障碍 = 撞击点，否则最后一点）—— 与 `docs/14` §12.1 一致。
    ///
    /// 入参形状（与 `.gd` 一一对应）：
    ///
    /// ```text
    /// shots       [{points: PackedVector2Array, shooter: int}]   # shooter = 士兵下标，-1 = 不排除
    /// soldiers    _soldiers 原表：[{pos: Vector2, alive: bool}, …]   # 下标 = 士兵下标
    /// target_rows GwTargetSet.entities()：[{pos, radius, target_id}, …]（只含活的）
    /// obstacles   _obstacles：[{pos: Vector2, size: float}, …]
    /// soldier_radius  GwCore.SOLDIER_HIT_RADIUS（PosInfo.radius 恒 0.5，docs/150 §Q1）
    /// ```
    ///
    /// 返回 `{kills, target_kills, stop, blocked, sources, note}`：
    /// `kills[i] = {kind:"soldier", index, point, pos}`、
    /// `target_kills[i] = {kind:"target", target_id, point, pos}`（都按 `point` 升序 = 弹道先后）。
    #[func]
    fn judge(
        &mut self,
        shots: VarArray,
        soldiers: VarArray,
        target_rows: VarArray,
        obstacles: VarArray,
        soldier_radius: f64,
    ) -> VarDictionary {
        // ① 掩膜只放**障碍**（`docs/30` §2：把士兵画进掩膜会让弹道错误地停在士兵处）。
        let mut mask = crate::plane::empty_mask();
        for i in 0..obstacles.len() {
            let Some(v) = obstacles.get(i) else { continue };
            let o = v.to::<VarDictionary>();
            let (x, y) = vec2_of(o.get("pos"));
            let size = o.get("size").map(|v| v.to::<f64>()).unwrap_or(0.0);
            crate::plane::rasterize(&mut mask, x, y, size, 0x00);
        }

        // ② 参评实体表：士兵（id = **士兵下标**）+ 靶子（id = TARGET_ID_BASE + target_id）。
        let mut targets: Vec<Target> = Vec::new();
        let mut soldier_pos: Vec<(f64, f64)> = vec![(0.0, 0.0); soldiers.len()];
        for i in 0..soldiers.len() {
            let Some(v) = soldiers.get(i) else { continue };
            let s = v.to::<VarDictionary>();
            let pos = vec2_of(s.get("pos"));
            soldier_pos[i] = pos;
            if !s.get("alive").map(|v| v.to::<bool>()).unwrap_or(false) {
                continue; // 死者不进表（③ 命中表对着**开火那一刻**的存活表算）
            }
            targets.push(Target { id: i as i64, x: pos.0, y: pos.1, radius: soldier_radius });
        }
        let mut target_pos: Vec<(i64, (f64, f64))> = Vec::new();
        for i in 0..target_rows.len() {
            let Some(v) = target_rows.get(i) else { continue };
            let t = v.to::<VarDictionary>();
            let pos = vec2_of(t.get("pos"));
            let radius = t.get("radius").map(|v| v.to::<f64>()).unwrap_or(soldier_radius);
            let tid = dict_int(&t, "target_id", -1);
            if tid < 0 {
                continue;
            }
            target_pos.push((tid, pos));
            targets.push(Target { id: TARGET_ID_BASE + tid, x: pos.0, y: pos.1, radius });
        }

        // ③ 逐发判定。停点/拦下是**跨发**累计的（同时回合的整批结算要合并成一份）。
        let mut kills: Vec<(usize, i64, (f64, f64))> = Vec::new();
        let mut target_kills: Vec<(usize, i64, (f64, f64))> = Vec::new();
        let mut stop = (0.0f64, 0.0f64);
        let mut blocked = false;
        let mut judged: i64 = 0;
        for i in 0..shots.len() {
            let Some(v) = shots.get(i) else { continue };
            let sh = v.to::<VarDictionary>();
            let points: Vec<(f64, f64)> = match sh.get("points") {
                Some(p) => p
                    .to::<PackedVector2Array>()
                    .as_slice()
                    .iter()
                    .map(|p| (p.x as f64, p.y as f64))
                    .collect(),
                None => Vec::new(),
            };
            if points.is_empty() {
                continue; // 没有弹道点 ⇒ 这一发不参与（旧 `.gd` 同一个 `continue`）
            }
            let shooter = dict_int(&sh, "shooter", -1);
            let owner = if shooter >= 0 { Some(shooter) } else { None };
            // ↓ 判定本体：与 `graphwar_shot_calculator::settle::resolve`（以及内核
            //   `hit_test`）**同一个函数**，这里没有第二份实现。
            let out = resolve_points_owned(&mask, &targets, &points, owner);
            judged += 1;
            blocked = blocked || out.blocked;
            stop = out.stop;
            // `hits` 的顺序 = 弹道经过的先后（`Hit.arc` 递增）；`index` = 第几点打中的。
            for h in &out.hits {
                if h.id >= TARGET_ID_BASE {
                    let tid = h.id - TARGET_ID_BASE;
                    let pos = target_pos
                        .iter()
                        .find(|(t, _)| *t == tid)
                        .map(|(_, p)| *p)
                        .unwrap_or((0.0, 0.0));
                    target_kills.push((h.index, tid, pos));
                } else {
                    let pos = soldier_pos.get(h.id as usize).copied().unwrap_or((0.0, 0.0));
                    kills.push((h.index, h.id, pos));
                }
            }
        }
        // 按**弹道点序号**升序（多个死者时先被打到的先落；`docs/65` §1 的 tick 语义）。
        kills.sort_by_key(|k| k.0);
        target_kills.sort_by_key(|k| k.0);

        let mut kill_rows = VarArray::new();
        for (point, index, pos) in &kills {
            let mut d = VarDictionary::new();
            d.set("kind", &GString::from("soldier"));
            d.set("index", *index);
            d.set("point", *point as i64);
            d.set("pos", to_vec2(*pos));
            kill_rows.push(&d);
        }
        let mut target_rows_out = VarArray::new();
        for (point, tid, pos) in &target_kills {
            let mut d = VarDictionary::new();
            d.set("kind", &GString::from("target"));
            d.set("target_id", *tid);
            d.set("point", *point as i64);
            d.set("pos", to_vec2(*pos));
            target_rows_out.push(&d);
        }
        let mut out = VarDictionary::new();
        out.set("kills", &kill_rows);
        out.set("target_kills", &target_rows_out);
        out.set("stop", to_vec2(stop));
        out.set("blocked", blocked);
        // `sources` / `note` 只是状态行（`_status_label.visible = false`，自检读）的措辞，
        // 不参与任何分支；旧 `.gd` 从 `GwCore.hit_test` 拿的那两个键在 gdext 版里
        // 其实不存在（`HIT_KEYS` 只有 ok/hits/hit_points/first_hit/stop/blocked/error），
        // 一直是 `"<null>"`。这里给一个真实的来源名。
        let mut sources = VarArray::new();
        for _ in 0..judged {
            sources.push(&GString::from("resolve_points_owned"));
        }
        out.set("sources", &sources);
        out.set("note", &GString::from(""));
        out
    }

    /// 只读快照（自检/诊断）。
    #[func]
    fn snapshot(&self) -> VarDictionary {
        let s = self.flow.snapshot();
        let mut d = VarDictionary::new();
        d.set("in_flight", s.in_flight);
        d.set("revealed", s.revealed as i64);
        d.set("total_points", s.total_points as i64);
        d.set("pending", s.pending as i64);
        d.set("kills_landed", s.kills_landed);
        d.set("targets_landed", s.targets_landed);
        d.set("settled", s.settled);
        d.set("turns_used", s.turns_used);
        d
    }
}

// ---------------------------------------------------------------------------
// GwTurnState —— 对局**状态机本体**（8 个 `GameState` + `turnstate::next_state`）
// ---------------------------------------------------------------------------
//
// **不是原版的东西**：原版没有这个类 —— 那台状态机是
// `graphwar_game_state_manager` 里的一个**字节** `[self+0x120]`（`set_state` = `0x4780cc`），
// 8 个变体的名字与判别值在 `.data.rel.ro 0x9586d0`–`0x958740` 那张表里。
// 本类只是把那**一个字节 + 那张迁移表**抬到 GDScript 能问的地方。
//
// 逻辑本体在 [`crate::turnstate`]（`GameState` / `TurnEvent` / `next_state`），
// 本文件**没有**第二份迁移表。

/// 对局状态机（原版 `GraphwarGameState`，8 个变体）。
///
/// ```text
/// var st := GwTurnState.new()          # 初始 = Setup（原版 `[+0x120]` 的新局值）
/// st.apply("game_started")             # Setup -> WaitingForFunctions
/// st.can_fire()                        # 只有 WaitingForFunctions 才是 true
/// st.apply("fire")                     # -> CalculatingResults
/// st.apply("results_computed")         # -> SynchronizingResults
/// st.apply("activate")                 # -> ExecutingResults
/// st.apply("turn_ended")               # -> WaitingForFunctions
/// st.apply("game_ended")               # -> EndingGame（**只从 ExecutingResults** 出发）
/// st.state()                           # 状态名（与协议里的 `GameStateInfo` 同名）
/// st.state_byte()                      # 判别值（写进 `[+0x120]` 的那个字节）
/// st.reset()                           # 换局 / 退出：**无条件**回 Setup
/// ```
///
/// `apply()` 在"该事件在当前状态下无意义"时返回空串并**不改状态**
/// （= 原版 `next_state` 的 `None`；`set_state` 不会被调 ⇒ 字节不动）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwTurnState {
    base: Base<RefCounted>,
    state: GameState,
}

#[godot_api]
impl IRefCounted for GwTurnState {
    fn init(base: Base<RefCounted>) -> Self {
        // 原版新局的状态字节是 `Setup`（1）：`0x475bac` 那条 `mov BYTE [self+0x120], 6` 是
        // 收场（`EndingGame`），开局则由 `root_node` 那侧写 —— 本类取 `turnstate` 表的起点。
        Self { base, state: GameState::Setup }
    }
}

/// GDScript 侧的事件名 → [`TurnEvent`]（名字与 `docs/14` §2 的状态图一一对应）。
///
/// `pub(crate)` 只为单测能直接验这张表；`#[func]` 走字符串入口。
fn parse_turn_event(name: &str) -> Option<TurnEvent> {
    Some(match name {
        "fire" => TurnEvent::Fire,
        "results_computed" => TurnEvent::ResultsComputed,
        "activate" => TurnEvent::Activate,
        "turn_ended" => TurnEvent::TurnEnded,
        "game_ended" => TurnEvent::GameEnded,
        "game_started" => TurnEvent::GameStarted,
        "reset" => TurnEvent::Reset,
        _ => return None,
    })
}

#[godot_api]
impl GwTurnState {
    /// 当前状态名（= 协议 `GameStateInfo` 里的字符串，`turnstate::GameState::as_str`）。
    #[func]
    fn state(&self) -> GString {
        GString::from(self.state.as_str())
    }

    /// 判别值（写进 `[+0x120]` 的那个字节；`turnstate::GameState::index`）。
    #[func]
    fn state_byte(&self) -> i64 {
        self.state.index() as i64
    }

    /// 施加一个事件。返回**新**状态名；空串 = 该事件在当下无意义（状态没变）。
    #[func]
    fn apply(&mut self, event: GString) -> GString {
        let Some(ev) = parse_turn_event(&event.to_string()) else {
            return GString::new();
        };
        match crate::turnstate::next_state(self.state, ev) {
            Some(next) => {
                self.state = next;
                GString::from(next.as_str())
            }
            None => GString::new(),
        }
    }

    /// 现在能不能开火 = `state == WaitingForFunctions`。
    ///
    /// 依据：`turnstate::next_state` 里 `Fire` **只**从 `WaitingForFunctions` 出发；
    /// 同一个字节也是 `handle_tick` 超时判据的第一项（`0x470989` 的
    /// `cmp BYTE [self+0x120], 2`）—— 两处读的是**同一格**。
    #[func]
    fn can_fire(&self) -> bool {
        self.state == GameState::WaitingForFunctions
    }

    /// 是不是开局态（`Setup`）。
    #[func]
    fn is_setup(&self) -> bool {
        self.state == GameState::Setup
    }

    /// 是不是收场态（`EndingGame`）。
    #[func]
    fn is_ending(&self) -> bool {
        self.state == GameState::EndingGame
    }

    /// 换局 / 退出对局：**无条件**回 `Setup`。
    ///
    /// 与 `apply("reset")` 的区别：迁移表里 `Reset` 只从 `EndingGame` 出发（表里那条），
    /// 而本屏"中途退出对局"（`end_match()`）可能不在 `EndingGame` ⇒ 这是**我方**的
    /// 清场入口，不假装它是一条原版迁移。
    #[func]
    fn reset(&mut self) {
        self.state = GameState::Setup;
    }

    /// 只读快照（自检/诊断）。
    #[func]
    fn snapshot(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("state", &GString::from(self.state.as_str()));
        d.set("state_byte", self.state.index() as i64);
        d.set("can_fire", self.can_fire());
        d
    }
}

// ---------------------------------------------------------------------------
// GwTurnClock —— 回合时钟（超时谓词的流程部分，`0x470989`）
// ---------------------------------------------------------------------------
//
// **不是原版的东西**：原版这圈流程在 `graphwar_game_state_manager::handle_tick`
// （`0x470840`）里，没有单独的类。本类只把"回合开始 → 截止 tick → 动作"这一段
// 与 [`crate::turnstate::should_auto_fire`] 的谓词接起来。

/// 回合时钟（tick 制，60 Hz）。
///
/// ```text
/// var c := GwTurnClock.new()
/// c.set_time_mode(state.time_mode)   # 0..5，与 GwModes.TimeMode 同一套判别值
/// c.restart()                        # 回合开始
/// c.advance(delta)                   # 每帧
/// c.decide(in_flight, running)       # "auto_fire" / "nothing"
/// c.seconds_left()                   # 倒计时（-1.0 = 无限时档）
/// ```
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwTurnClock {
    base: Base<RefCounted>,
    clock: TurnClock,
}

#[godot_api]
impl IRefCounted for GwTurnClock {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base, clock: TurnClock::default() }
    }
}

#[godot_api]
impl GwTurnClock {
    /// 换时间档（`0 = Timer30s … 4 = Timer5m, 5 = TimerInf`），并**重置回合**。
    #[func]
    fn set_time_mode(&mut self, time_mode: i64) {
        self.clock.set_time_mode(if (0..=5).contains(&time_mode) { time_mode as u8 } else { 5 });
    }

    /// 回合开始：计时归零、超时闩清零。
    #[func]
    fn restart(&mut self) {
        self.clock.restart();
    }

    /// 推进时钟（按秒），返回本回合已过 tick。
    #[func]
    fn advance(&mut self, seconds: f64) -> i64 {
        self.clock.advance(seconds)
    }

    /// 判"现在该不该自动开火"（三个合取项，见 `TurnClock::decide`）。
    /// 返回 `"auto_fire"` 或 `"nothing"`。
    #[func]
    fn decide(&mut self, in_flight: bool, running: bool) -> GString {
        match self.clock.decide(in_flight, running) {
            TurnAction::AutoFire => GString::from("auto_fire"),
            TurnAction::Nothing => GString::from("nothing"),
        }
    }

    /// 倒计时剩几秒；`-1.0` = 无限时档（原版标签是空白）。
    #[func]
    fn seconds_left(&self) -> f64 {
        self.clock.seconds_left().unwrap_or(-1.0)
    }

    /// 这一档有没有截止时刻。
    #[func]
    fn has_deadline(&self) -> bool {
        self.clock.turn_duration_ticks().is_some()
    }

    /// 本回合的时长（tick）；`-1` = 没有截止。
    #[func]
    fn duration_ticks(&self) -> i64 {
        self.clock.turn_duration_ticks().unwrap_or(-1)
    }

    #[func]
    fn elapsed_ticks(&self) -> i64 {
        self.clock.elapsed_ticks()
    }

    #[func]
    fn auto_fired_total(&self) -> i64 {
        self.clock.auto_fired_total()
    }

    /// 只读快照（自检/诊断）。
    #[func]
    fn snapshot(&self) -> VarDictionary {
        let s = self.clock.snapshot();
        let mut d = VarDictionary::new();
        d.set("time_mode", s.time_mode as i64);
        d.set("duration_ticks", s.duration_ticks.unwrap_or(-1));
        d.set("elapsed_ticks", s.elapsed_ticks);
        d.set("seconds_left", s.seconds_left.unwrap_or(-1.0));
        d.set("auto_fired", s.auto_fired);
        d.set("auto_fired_total", s.auto_fired_total);
        d
    }
}

// ---------------------------------------------------------------------------
// GwBotTurn —— AI 回合闸门（**我方自定**）
// ---------------------------------------------------------------------------
//
// ⚠️ 原版是 `aigen` 进化器，"AI 什么时候开火"**没有观测**（`docs/16` §52.6 #3）。
// 搬到 Rust 的理由是"判据只能有一份"，不是"更像原版"。逻辑本体在
// [`crate::graphwar_play_screen_control::BotTurn`]。

/// AI 的思考闸门 + 一发**自定**的瞄准函数。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwBotTurn {
    base: Base<RefCounted>,
    bot: BotTurn,
}

#[godot_api]
impl IRefCounted for GwBotTurn {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base, bot: BotTurn::default() }
    }
}

#[godot_api]
impl GwBotTurn {
    /// 配思考时长（**自定**值；旧 `.gd` 是 0.6 s）。
    #[func]
    fn configure(&mut self, think_seconds: f64) {
        self.bot = BotTurn::new(think_seconds);
    }

    /// 轮到 AI：起倒计时。
    #[func]
    fn arm(&mut self) {
        self.bot.arm();
    }

    /// 离开 AI 的回合 / 停局：取消倒计时。
    #[func]
    fn disarm(&mut self) {
        self.bot.disarm();
    }

    #[func]
    fn armed(&self) -> bool {
        self.bot.armed()
    }

    #[func]
    fn think_left(&self) -> f64 {
        self.bot.think_left()
    }

    #[func]
    fn think_seconds(&self) -> f64 {
        self.bot.think_seconds()
    }

    /// 推进一帧；返回"现在该让它开火了"。
    #[func]
    fn tick(&mut self, delta: f64, can_fire: bool) -> bool {
        self.bot.tick(delta, can_fire)
    }

    /// 一发**自定**的函数串（一条过射手、指向最近敌人的直线）。
    ///
    /// `others` = 按士兵下标顺序的 `[{x, y, team, alive}, …]`。
    #[func]
    fn aim_expression(&self, shooter: Vector2, team: i64, others: VarArray) -> GString {
        let mut list: Vec<(f64, f64, i64, bool)> = Vec::new();
        for i in 0..others.len() {
            let Some(v) = others.get(i) else { continue };
            let d = v.to::<VarDictionary>();
            list.push((
                d.get("x").map(|v| v.to::<f64>()).unwrap_or(0.0),
                d.get("y").map(|v| v.to::<f64>()).unwrap_or(0.0),
                dict_int(&d, "team", -1),
                d.get("alive").map(|v| v.to::<bool>()).unwrap_or(false),
            ));
        }
        let expr =
            crate::graphwar_play_screen_control::bot_aim_expression(
                (shooter.x as f64, shooter.y as f64, team),
                &list,
            );
        GString::from(expr.as_str())
    }
}
