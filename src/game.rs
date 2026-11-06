//! 一局对战的服务端对象：把已有的零件装起来。
//!
//! 组成：
//!
//! * 房间状态（`server::RoomState`）
//! * 关卡障碍（用于建掩膜）
//! * 士兵（致死判定）
//! * tick 基准（`graphwar_tick_handler::tick_at`）
//! * **射手轮转**（`graphwar_turn_manager::TurnManager`）与**按士兵的 AI 进化**（`graphwar_function_evolutionator::SoldierEvolver`）
//!   —— 见下面的"接线"一节
//!
//! `Match::fire` 处理一次开火，产出**这条链路上要广播的消息**：
//!
//! ```text
//! FunctionPoints -> [LifeQueuedDeath -> EffectInfo(DeathEffect)]*
//!                -> [QueueDestroyObstacle] -> EffectInfo(ExplosionEffect)
//! ```
//!
//! 与 `settle::events` 的区别：这里直接产出**协议报文文本**（带 tick），
//! 可以直接写进 socket。
//!
//! ## 接线（`docs/117-server-wiring.md`）
//!
//! 2026-11 之前，"谁开火"由**调用方**给 `soldier_id`，`aigen` 的进化**没有调用点**，
//! 于是 `docs/98`/`docs/109` 的规则"已实现并可测"却"端到端没走"。现在：
//!
//! ```text
//! Match::begin_next_turn(player_id, timed_out, now_ms, time_mode)
//!   ① graphwar_turn_manager::TurnManager::begin_turn  -> TurnDecision（三态）
//!   ② 只为判出来的 soldier_id 调 graphwar_function_evolutionator::SoldierEvolver::evolve_soldier（25 s 预算）
//!   ③ decision.broadcast() == Some((p, s)) 才发报文；Skip 时 messages 为空
//! ```
//!
//! ## AI 说话：`token → 文本 → FunctionInfo`（`docs/127-ai-speaks.md`）
//!
//! 2026-11 之前，`aigen` 的产物是 token，而 `token → 文本`（`0x4ec160`）没有调用点，
//! 于是 AI"会思考、不会说话"。现在 `function::to_text`（`docs/119`，1032/1032 对拍）
//! 就是那个缺口，接线后一个 AI 回合里有**两条** `FunctionInfo`，形态各自照真实抓包
//! （`docs/127` §2 的原始字节流帧号）：
//!
//! ```text
//! TurnInfo{p,s} -> EndOfTurnInfo -> GameStateInfo(WaitingForFunctions)
//! FunctionInfo{owner_id: s, function: <AI 文本>, corrected_function: null}   ← 提交（帧 135/158/200）
//! ……客户端 FunctionFireRequest……
//! GameStateInfo(CalculatingResults)
//! FunctionInfo{owner_id: s, function: <AI 文本>, corrected_function: "f(x) ± c"}  ← 修正（帧 143/161/203）
//! FunctionPoints{owner_id: s, …}
//! ```
//!
//! `owner_id` 一律是 **`soldier_id`**（`AGENTS.md` 硬事实；抓包里 `FunctionInfo.owner_id`
//! 与同一回合的 `FunctionPoints.owner_id` 相同，都不是 `player_id`）。
//! 修正版**只**在开火那一刻发（`docs/43` 的既有顺序不改）。
//! 修正文本的运算符**带符号**（`c >= 0` 写 `" + "`、`c < 0` 写 `" - "`；抓包帧 143/182/203），
//! 由 `shot::corrected_text` 复用 `correction_text` 产出（`docs/127` §5 第 1 条，已修）。
//!
//! **没接的**（都写在 `docs/117` §6 / `docs/127` §6）：回合超时的**检测**没有证据，
//! 本层只把 `timed_out` 当输入参数；游标初值取 `PlayerTurns::new` 的 0
//! （`TurnManager::add_player` 没有 index 入口，见 `docs/98` §6-B）；
//! 同时回合制没接（房间状态写死 `SequentialTurns`）。

use crate::graphwar_function_evolutionator::{self, Individual, SoldierEvolver, SoldierFunctions, SoldierTurn, VarMode};
use crate::graphwar_life_manager::{LifeState, Status};
use crate::plane::{empty_mask, rasterize};
use crate::protocol::{emit_effect_info, emit_function_points, emit_life_queued_death};
use crate::rng::ChaCha12;
use crate::server::RoomState;
use crate::settle::{resolve, Target};
use crate::graphwar_tick_handler::tick_at;
use crate::graphwar_turn_manager::{self, TurnDecision, TurnManager};

/// 一局对战的服务端状态。
pub struct Match {
    pub room: RoomState,
    pub obstacles: Vec<(f64, f64, f64)>,
    pub soldiers: Vec<Target>,
    /// tick 基准：`(基准 tick, 基准毫秒)`
    pub tick_base: (i64, i64),
    /// 障碍光栅化后的掩膜（决定弹道停点）
    mask: Vec<u8>,
    /// 已经用掉的效果 id（抓包里从 236 开始递增）
    next_effect_id: i64,
    /// 实体（士兵 **和** 靶子）→ 队伍（用于判定"某队全灭"）。
    pub teams: std::collections::BTreeMap<i64, u8>,
    /// 这一局出现过的所有队伍。
    all_teams: std::collections::BTreeSet<u8>,
    /// **靶子表**：`id -> alive`。靶子在真值里就是 `Team2` 的普通实体，**没有**独立规则
    /// （`docs/150`：127/127 份回放里靶子都有 `TargetInfo` + `TeamInfo{Team2}`，
    /// 且**都没有** `SoldierInfo`；造靶子的 `0x46f310` 的 `team` 是**入参**）。
    /// ⇒ 它和士兵一样进 [`Match::team_has_survivor`]，走**同一条**"某队全灭"规则。
    pub targets: std::collections::BTreeMap<i64, bool>,
    /// 回合上限：`Some(n)` = 原版发过 `TurnLimitInfo{turn_limit: n}`（构造点 `0x472400`）；
    /// `None` = 原版**根本不发**这条事件（`docs/123` §5.1 #4：无上限是"不发"，不是发 0）。
    pub turn_limit: Option<i64>,
    /// 已完成的回合数（`TurnCountInfo.turn_count`）。
    pub turn_count: i64,
    /// 当前回合的起始 tick 与时间模式。
    turn_start_tick: i64,
    time_mode: String,
    /// 已排队但还没到期的 `DestroyObstacle`：`(到期 tick, 报文原文)`。
    ///
    /// 实测延迟 ≈165 tick（2.75 秒），见 `docs/44`。
    pending_destroy: Vec<(i64, String)>,

    // ---- 接线层：射手轮转 + 按士兵的 AI 进化（`docs/109` §6.1 说的"还没接线"）----
    /// 玩家 → `{队伍, Vec<soldier_id>, 游标}`（`docs/98` §1.1 的 `0x28` 步长表）。
    turns: TurnManager,
    /// 每个士兵一个函数槽（`docs/98` §3.1：一士兵一格，开局全空）。
    functions: SoldierFunctions,
    /// 生命状态表（`life_manager` 的 Rust 形态）：轮转据此判活（`0x463fb6` → `0x454950`）。
    ///
    /// 与 `self.soldiers`（场上活着的 `Target`）**分工不同**：这张表记得"死过"，
    /// 于是死掉的士兵返回 `0`（死）而不是 `2`（查不到）。
    life: LifeState,
    /// AI 参数与预算（难度档 `docs/67`；预算 `docs/93`）。
    ai: AiConfig,
    /// 进化用的随机流。**确定性**：同一 seed ⇒ 同一串（`docs/47` §6：真游戏的 AI
    /// 消费主流，这里给一条独立的、可复现的流）。
    ai_rng: ChaCha12,
    /// 上一代的存活者，键 = 种群归属键（[`PopScope`]，`docs/98` §6-A **未定**）。
    ai_pops: std::collections::BTreeMap<i64, Vec<Individual>>,
    /// 哪些 `player_id` 是**电脑玩家**（`computer_player_manager` 的那张表）：
    /// 只有它们的回合才跑 AI 进化。人类玩家走同一条 `TurnInfo` 路径，但不进化。
    bots: std::collections::BTreeSet<i64>,
    /// 已经跑过的进化次数。**可观测副作用**：端到端测试用它断言"AI 真的为这个士兵
    /// 进化过"（进化的产物是 token，暂时上不了线，见 `docs/117` §4）。
    pub evolutions: u64,
}

/// 进化种群的归属（`docs/98` §6-A **未定**）。
///
/// 二进制里 `computer_player_manager` 那张 `HashMap<i32, V>` 的键是 `player_id`
/// 还是 `soldier_id` **没观测到插入点**，所以"种群是每人一份还是每士兵一份"没有结论。
/// `graphwar_function_evolutionator::SoldierEvolver` 因此**不持有种群**（`docs/109` §4.3）；归属这个决定落在
/// 接线层，这里做成**显式开关**，默认按士兵，绝不把它写成隐含约定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopScope {
    /// 每个士兵一份种群（键 = `soldier_id`）。
    #[default]
    Soldier,
    /// 每个玩家一份种群（键 = `player_id`），同一玩家的士兵共用。
    Player,
}

/// AI 接线配置。默认 = 真游戏口径（25 s 预算 + 难度 1）。
///
/// * `level`：难度档 1..=6（`docs/67` 的六档参数）。**默认 1 是重建版的取值**——
///   真游戏里难度是房主设置，我们的 `RoomState` 没有这个字段（`docs/117` §6）；
/// * `mode`：`graphwar_function_evolutionator::VarMode`（`docs/94`）。**没有**从 `room.function_mode` 推——
///   那个字符串到 `VarMode` 的对应没有证据，不猜；
/// * `budget_secs`：`Some(25.0)` = 真游戏的墙钟上限（`0x51b7cf` `cmp $0x19`，
///   `docs/93`）；`None` = 不限时，测试用（保证可复现）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiConfig {
    pub level: u8,
    pub mode: VarMode,
    pub budget_secs: Option<f64>,
    pub pop_scope: PopScope,
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig {
            level: 1,
            mode: VarMode::Normal,
            // 真游戏的 25 s（`docs/93`）：**这一条就是"预算实际生效"的接线点**。
            budget_secs: Some(graphwar_function_evolutionator::GAME_TIME_BUDGET_SECS),
            pop_scope: PopScope::default(),
        }
    }
}

/// 本回合"轮到谁"的接线结果。
#[derive(Debug, Clone, PartialEq)]
pub struct TurnStart {
    /// 轮转判决（三态：`Announce{id}` / `Announce{0}` / `Skip`，`docs/109` §2.2）。
    pub decision: TurnDecision,
    /// 要广播的报文。`Skip` 时为空；`Announce{0}` 时是 `TurnInfo{p,0}` 那三条。
    pub messages: Vec<String>,
    /// AI 为该士兵跑的进化结果。被跳过 / `soldier_id == 0` / 该玩家不是电脑玩家 /
    /// 射手不在场上时都是 `None`。
    pub evolution: Option<SoldierTurn>,
}

/// `QueueDestroyObstacle` 到 `DestroyObstacle` 的实测延迟（tick）。
///
/// 三次配对样本为 163.9 / 164.0 / 167.0 tick，取 165。
pub const DESTROY_DELAY_TICKS: i64 = 165;

// ---------------------------------------------------------------------------
// 同时回合制：一发的提交（收集阶段的产物）
// ---------------------------------------------------------------------------

/// 一名玩家在**本回合**提交的那一发（`docs/141` §1 的收集阶段产出）。
///
/// 为什么要有这个类型：`SequentialTurns` 一回合只有一发，`Match::fire` 用一个
/// 函数签名就够了；`SimultaneousTurns` 一回合是 **N 发一个事务**（130 个真回放批，
/// 2~8 发），报文按类型分组而不是"一发一个完整窗口"，所以必须先把提交攒起来再结算。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Submission<'a> {
    /// 开火的实体（士兵 id）。
    pub owner_id: i64,
    /// **未修正**的函数原文。
    pub function_text: &'a str,
    /// 起点（士兵站位）。
    pub x0: f64,
    pub y0: f64,
    /// 朝向：由队伍决定（`graphwar_turn_manager::Team::flip`，`docs/105`）。
    pub flip: bool,
}

// ---------------------------------------------------------------------------
// 玩家加入：`PlayerInfo`（补 `docs/33` §4 的缺口；接线与文档见 `docs/139`）
// ---------------------------------------------------------------------------

/// `PlayerInfo{player_id, connection_id}` 的报文文本。
///
/// **形状是抓包原文**（两个字段、蛇形键名、判别名不上线，`docs/96` §4）：
/// `fixtures/protocol.jsonl:13` 与 `fixtures/protocol_match.jsonl:36–37` 都是
/// `{"PlayerInfo":{"player_id":229,"connection_id":228}}`。
///
/// 为什么需要它：真客户端靠这条消息知道"**我是哪个 player_id**"。没有它，客户端只能
/// 退化成"按 `TurnInfo` 里的玩家假定"（`docs/134` §7.1 记的就是这个缺口）。
/// 本函数只负责**报文文本**，什么时候发由 `graphwar_game_server::serve` 决定（收到
/// `NewPlayerRequest` 时回一条）。
pub fn player_info_message(player_id: i64, connection_id: i64) -> String {
    format!(
        r#"{{"PlayerInfo":{{"player_id":{},"connection_id":{}}}}}"#,
        player_id, connection_id
    )
}

/// 一局为什么结束（真值 `0x4759f0` 的两个结束入口）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndingReason {
    /// 某队没有存活实体了（`0x475a80` / `0x475a88` 两条 `je END`）。
    TeamWiped,
    /// 回合上限用尽（`0x475a64`–`0x475a7a`）。**独立**于上面那条的第二个入口。
    TurnLimit,
}

/// 结束判定的结果。`winner` 的取值照真值写进 `[self+0x11f]` 的那个字节：
/// `Some(0)` = Team1、`Some(1)` = Team2、`None` = 两边都没活实体（真值里的 `2`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ending {
    pub reason: EndingReason,
    pub winner: Option<u8>,
}

impl Match {
    pub fn new(
        room: RoomState,
        obstacles: Vec<(f64, f64, f64)>,
        soldiers: Vec<Target>,
        tick_base: (i64, i64),
    ) -> Self {
        let mut mask = empty_mask();
        for (x, y, r) in &obstacles {
            rasterize(&mut mask, *x, *y, *r, 0x00);
        }
        // 队伍归属由调用方通过 `set_team` 登记（不预设，否则会误判"0 队全灭"）
        let teams = std::collections::BTreeMap::new();
        let all_teams = std::collections::BTreeSet::new();
        Match {
            room, obstacles, soldiers, tick_base, mask,
            next_effect_id: 236, teams, all_teams, turn_count: 0,
            targets: std::collections::BTreeMap::new(),
            turn_limit: None,
            turn_start_tick: 0, time_mode: "Timer1m".to_string(),
            pending_destroy: Vec::new(),
            turns: TurnManager::new(),
            functions: SoldierFunctions::new(),
            life: LifeState::default(),
            ai: AiConfig::default(),
            // 默认 seed 全 0：**确定性**优先（端到端测试要可复现），
            // 需要别的流就用 `with_ai_seed`。
            ai_rng: ChaCha12::from_seed(&[0u8; 32]),
            ai_pops: std::collections::BTreeMap::new(),
            bots: std::collections::BTreeSet::new(),
            evolutions: 0,
        }
    }

    /// 换掉 AI 配置（难度 / 模式 / 预算 / 种群归属）。
    pub fn with_ai(mut self, cfg: AiConfig) -> Self {
        self.ai = cfg;
        self
    }

    /// 换掉 AI 随机流的种子。**同一 (seed, 消息序列, ai_budget_secs=None) ⇒ 同一局**。
    pub fn with_ai_seed(mut self, seed: &[u8; 32]) -> Self {
        self.ai_rng = ChaCha12::from_seed(seed);
        self
    }

    /// 当前的 AI 配置（测试用它断言"25 s 预算确实接上了"）。
    pub fn ai_config(&self) -> AiConfig {
        self.ai
    }

    /// 当前的函数槽表（`soldier_id → tokens`）。
    pub fn functions(&self) -> &SoldierFunctions {
        &self.functions
    }

    /// 当前的轮转状态。
    pub fn turns(&self) -> &TurnManager {
        &self.turns
    }

    /// 当前的生命状态表。
    pub fn life(&self) -> &LifeState {
        &self.life
    }

    /// 缓存里有几份"上一代存活者"。
    ///
    /// 它是 [`PopScope`] 的可观测后果：按士兵 ⇒ 最多 = 士兵数，按玩家 ⇒ 最多 = 玩家数
    /// （`docs/98` §6-A 的两种读法在这里分叉）。
    pub fn ai_population_entries(&self) -> usize {
        self.ai_pops.len()
    }

    /// 把一个玩家登记成**电脑玩家**（它的回合才会跑 AI 进化）。
    pub fn set_bot(&mut self, player_id: i64) {
        self.bots.insert(player_id);
    }

    /// 这个玩家是不是电脑玩家。
    pub fn is_bot(&self, player_id: i64) -> bool {
        self.bots.contains(&player_id)
    }

    /// 登记一个玩家：队伍 + 它的士兵表。
    ///
    /// **士兵顺序原样入表**，不做排序、不做分组、不做推导：
    ///
    /// * `docs/98` §2 观测到轮转顺序 = `SoldierInfo` 创建顺序，但 §6-C 说没跟到 push 点；
    /// * `docs/47` 判定玩家出场顺序不可复现（HashMap 迭代序）。
    ///
    /// ⇒ 这两件事**只能由调用方给**（[`Match::register_player`] 的 `soldiers` 参数与
    /// 调用方自己的玩家顺序），本方法不做任何猜测。
    ///
    /// 副作用：每个士兵各开一格空函数槽（`docs/98` §3.1）、登记队伍（供全灭判定）、
    /// 初始化为"活着"（`life_manager` 那张表）。
    pub fn register_player(&mut self, player_id: i64, team: graphwar_turn_manager::Team, soldiers: &[i64]) {
        for id in soldiers {
            self.set_team(*id, team.id());
            self.functions.register(*id);
            if let Ok(v) = u32::try_from(*id) {
                self.life.set(v, Status::Alive, None);
            }
        }
        self.turns.add_player(player_id, team, soldiers.to_vec());
    }

    /// **回合开始的接线入口**：先由轮转定射手 → 再只为它跑一次 AI 进化 → 再拼报文。
    ///
    /// 三态处理（`docs/98` §1.4 / `docs/109` §2.2）由 `TurnDecision::broadcast()` 一个
    /// 开关决定，不在这里再分叉：
    ///
    /// | `decision` | 报文 | AI |
    /// |---|---|---|
    /// | `Announce{id}`, `id != 0` | 发 `TurnInfo{p,id}` + `EndOfTurnInfo` + `GameStateInfo` | 为 `id` 进化 |
    /// | `Announce{0}`（全灭） | **仍发** `TurnInfo{p,0}`（`0x463fd0` 观测） | 不跑（没有士兵） |
    /// | `Skip{reason}` | **不发** | 不跑 |
    ///
    /// `timed_out` 是**用户口述**的"回合超时 = 跳过该玩家"
    /// （`docs/HANDOVER` §7.10 第 2 条，**未独立验证**）——本方法只把它当输入，
    /// 传输成 `SkipReason::TurnTimeout`，不替游戏决定"游标推不推进""此时
    /// `TurnInfo` 是否已广播"（`docs/109` §2.7 的三件事，仍是**未知**）。
    pub fn begin_next_turn(
        &mut self,
        player_id: i64,
        timed_out: bool,
        now_ms: i64,
        time_mode: &str,
    ) -> TurnStart {
        let decision = {
            let state = graphwar_turn_manager::life_state_of(&self.life);
            self.turns.begin_turn(player_id, state, timed_out)
        };
        // **先定射手、再进化**——顺序本身是结论的一部分（`docs/98` §0：AI 不选射手）。
        let turn_no = self.turn_count.max(0) as u64;
        let evolution = match decision {
            TurnDecision::Announce { player_id, soldier_id, flip } => {
                self.evolve_for_soldier(player_id, soldier_id, flip, turn_no)
            }
            TurnDecision::Skip { .. } => None,
        };
        let mut messages = Vec::new();
        if let Some((p, s)) = decision.broadcast() {
            messages = self.begin_turn(p, s, now_ms, time_mode);
            // **AI 说话**：进化刚把这个士兵的槽写过，于是把槽里的 token 写成文本广播。
            // 只有**真的进化过**才发 —— 没进化的回合（非电脑玩家 / 被跳过 / 全灭）
            // 没有"提交了一个函数"这件事，发一条空 `FunctionInfo` 反而是假消息。
            if evolution.is_some() {
                if let Some(info) = self.ai_submit_info(s) {
                    messages.push(info);
                }
            }
        }
        TurnStart { decision, messages, evolution }
    }

    /// **同时回合制的回合开始**：给每个玩家都定一次射手，并**一次**广播。
    ///
    /// 与 [`Match::begin_next_turn`]（逐玩家轮转、一回合只定一个人）的区别，全部来自
    /// 真回放（`观测`，`docs/141` §1，`1793884702` 的每个回合都是这个形状）：
    ///
    /// ```text
    /// TurnInfo{player, soldier} × P     ← 每个玩家一条，**同一个 tick**，公告顺序
    /// EndOfTurnInfo × 1                 ← 只有一条（不是每人一条）
    /// GameStateInfo(WaitingForFunctions) × 1
    /// ```
    ///
    /// 返回的两样东西：要广播的报文，以及**每个玩家**的判决 + 进化结果
    /// （[`TurnStart`]，**按公告顺序**）。收集阶段的等待名单 = 其中
    /// `decision.broadcast()` 给出 `soldier_id != 0` 的那些 `player_id`。
    ///
    /// 那个顺序同时就是结算批里射击者的顺序（`docs/141` §4，130/130 批实测），
    /// 所以调用方把它原样交给 [`crate::graphwar_turn_manager::RoundCollection`] 即可。
    ///
    /// 全灭的玩家照样拿到 `TurnInfo{p, 0}`（`0x463fd0` 的观测），但**不在等待名单里**：
    /// 他没有士兵可以开火，等他只会把回合卡死。真回放里正是这样（9 条 `TurnInfo`、
    /// 8 发结算批）。
    pub fn begin_simultaneous_turn(
        &mut self,
        players: &[i64],
        now_ms: i64,
        time_mode: &str,
    ) -> (Vec<String>, Vec<TurnStart>) {
        let start_tick = self.tick_now(now_ms);
        self.turn_start_tick = start_tick;
        self.time_mode = time_mode.to_string();
        let turn_no = self.turn_count.max(0) as u64;

        let mut starts = Vec::with_capacity(players.len());
        for pid in players {
            let decision = {
                let state = graphwar_turn_manager::life_state_of(&self.life);
                self.turns.begin_turn(*pid, state, false)
            };
            // 与逐玩家路径同序：**先定射手、再进化**（`docs/98` §0）。
            let evolution = match decision {
                TurnDecision::Announce { player_id, soldier_id, flip } => {
                    self.evolve_for_soldier(player_id, soldier_id, flip, turn_no)
                }
                TurnDecision::Skip { .. } => None,
            };
            starts.push(TurnStart { decision, messages: Vec::new(), evolution });
        }

        let mut msgs = Vec::with_capacity(players.len() + 3);
        for st in &starts {
            // Skip 的玩家一条报文都没有（`docs/98` §1.4）；Announce（含 soldier 0）都发。
            if let Some((p, s)) = st.decision.broadcast() {
                msgs.push(crate::protocol::emit_turn_info(p, s));
            }
        }
        // `EndOfTurnInfo` 与 `GameStateInfo` 各**一条**。
        //
        // ⚠️ `end_of_turn_tick` 在真回放里恒为 **0**（`1793884702` 三回合全是
        // `{"end_of_turn_tick":0,"time_mode":"TimerInf"}`），而 `graphwar_tick_handler::end_of_turn_tick_wire`
        // 给的是 `start + 时长`。这两个数在 `docs/41` 就没对上过，是**已知分歧**；
        // 本函数沿用逐玩家路径的发法（不在这里另造一个数）。
        let end = crate::graphwar_tick_handler::end_of_turn_tick_wire(start_tick, time_mode);
        msgs.push(crate::protocol::emit_end_of_turn(end, time_mode));
        msgs.push(crate::protocol::emit_game_state("WaitingForFunctions"));
        (msgs, starts)
    }

    /// 该士兵函数槽里的**文本**：`function::to_text`（`0x4ec160`）在本仓库的唯一调用点。    ///
    /// 为什么需要它：`aigen` 的产物是 token（[`FunctionSlot::tokens`](crate::graphwar_function_evolutionator::FunctionSlot)），
    /// 协议 `FunctionInfo.function` 要的是文本，中间只有这一步（`docs/119`、`docs/127`）。
    /// `None` = 这个士兵没有槽（没登记过）。
    ///
    /// ⚠️ 它**不是单射**（`docs/119` §5.3）：π 写成 `3.14`、负常量写成 `neg` 的文本形态、
    /// 缺操作数的空位在文本里不存在。这三条都是真二进制行为，不在这里修。
    pub fn ai_function_text(&self, soldier_id: i64) -> Option<String> {
        self.functions.tokens(soldier_id).map(crate::function::to_text)
    }

    /// AI 这一回合**提交**时要发的那条 `FunctionInfo`（`docs/127` §2.1、§3.1）。
    ///
    /// 形态逐字段照真实抓包（`docs/127` §2 的原始字节流帧 **73–76 / 135 / 158 / 200**）：
    ///
    /// ```text
    /// {"FunctionInfo":{"owner_id":233,"function":"abs((abs((-0.57+x))+-4.85))","corrected_function":null}}
    /// ```
    ///
    /// * `owner_id` = **soldier_id**（`AGENTS.md` 硬事实；抓包里同回合 `FunctionPoints.owner_id` 相同）；
    /// * `corrected_function` = **null** —— 修正版（`f(x) ± c`）不在这一刻发，
    ///   它由 [`Match::fire`] 夹在 `CalculatingResults` 与 `FunctionPoints` 之间发出（帧 143/161/203）。
    ///
    /// 空槽（开局、还没进化）返回 `function: ""`，与抓包里那条空函数完全同形。
    pub fn ai_submit_info(&self, soldier_id: i64) -> Option<String> {
        let text = self.ai_function_text(soldier_id)?;
        Some(crate::protocol::emit_function_info(soldier_id, &text, None))
    }

    /// 只为**一个**士兵跑一次进化，并把结果写进**它自己的**槽（`docs/98` §3）。
    ///
    /// 不选射手（`soldier_id` 由调用方/轮转给定）、不与同玩家的其它士兵共享槽、
    /// 每回合重算（`evolve_soldier` 内部覆盖写，`docs/109` §4.2）。
    ///
    /// 以下情况返回 `None`（**不跑**）：
    ///
    /// * `soldier_id == 0`：全灭哨兵，真游戏也没有 `FunctionFire`（`docs/98` §1.4）；
    /// * 该玩家不是电脑玩家：`computer_player_manager` 只服务电脑玩家（`docs/98` §4）；
    /// * 射手不在场上（已死 / 没登记）：没有局面可评。
    pub fn evolve_for_soldier(
        &mut self,
        player_id: i64,
        soldier_id: i64,
        flip: bool,
        turn_no: u64,
    ) -> Option<SoldierTurn> {
        if soldier_id == graphwar_turn_manager::SOLDIER_ID_NONE || !self.is_bot(player_id) {
            return None;
        }
        // 射手的局面（不在场上 ⇒ 没有这一回合的进化）。
        let (x0, y0) = {
            let t = self.soldiers.iter().find(|t| t.id == soldier_id)?;
            (t.x, t.y)
        };
        let key = match self.ai.pop_scope {
            PopScope::Soldier => soldier_id,
            PopScope::Player => player_id,
        };
        let params = graphwar_function_evolutionator::params(self.ai.level);
        let mode = self.ai.mode;
        let budget = self.ai.budget_secs;
        let pop = self.ai_pops.remove(&key);
        // 评分上下文借用 mask/soldiers，写回借用 functions/ai_rng —— 拆开借，互不重叠。
        let out = {
            let Match { soldiers, mask, functions, ai_rng, .. } = self;
            let others: Vec<Target> = soldiers.iter().filter(|t| t.id != soldier_id).copied().collect();
            let ctx = graphwar_function_evolutionator::ScoringContext {
                x0,
                y0,
                flip,
                // `evaluate` 内部按 `Phase` 覆盖成 0.5 / 0.01，这里只是基值。
                step: graphwar_function_evolutionator::COARSE_STEP,
                mask: &mask[..],
                targets: &others,
                limit: params.limit,
            };
            let evolver = SoldierEvolver::new(params, mode, ctx).with_budget(budget);
            let mut next = || ai_rng.next_u64();
            // 第 0 代：没有缓存就重抽（`docs/110` 说真游戏 gen-0 走的也是同一个槽循环，
            // 但那条链路的 `seed_population` 仍用 `initial_population`，见 `docs/117` §6）。
            let pop = match pop {
                Some(p) => p,
                None => evolver.seed_population(&mut next),
            };
            evolver.evolve_soldier(functions, soldier_id, turn_no, pop, &mut next)
        };
        // 存活者留给"下一次轮到这个键"当初始种群（归属键见 [`PopScope`]）。
        self.ai_pops.insert(key, out.survivors.clone());
        self.evolutions += 1;
        Some(out)
    }

    /// 当前时刻的 tick。
    pub fn tick_now(&self, now_ms: i64) -> i64 {
        tick_at(self.tick_base.0, self.tick_base.1, now_ms)
    }

    /// 登记某名实体（士兵 **或** 靶子）属于哪支队伍。
    ///
    /// 真值里队伍是**实体**的属性，不是"士兵"的属性：造靶子的 `0x46f310` 用
    /// `movzx edx,bpl`（它的第 2 个参数）去写 `TeamInfo`，也就是**靶子带队伍**
    /// （`docs/150`：127/127 份回放都是 `TeamInfo{Team2}`）。所以靶子也走这张表。
    pub fn set_team(&mut self, soldier_id: i64, team: u8) {
        self.teams.insert(soldier_id, team);
        self.all_teams.insert(team);
    }

    // ---- 靶子（真值 `graphwar_target_manager.rs` + `life_manager` 的形态）----
    //
    // `docs/150` 把三件事分开了：
    //   ① 靶子**是** `Team2` 的实体（所以打靶关的胜负 = `Team2` 全灭，不是另一条规则）；
    //   ② 靶子**不是**士兵（回放里 127/127 个靶子都没有 `SoldierInfo`），
    //      所以它不参与射手轮转、不进 `soldiers`（那张表是"能被弹道打死、能开火"的集合）；
    //   ③ 存活位在生命表里（靶子有 `LifeInfo`），胜负判定数的正是生命表里的活实体。

    /// 造一个靶子（真值 `0x46f310`）：登记 id、**写队伍**、初始 `alive = true`。
    pub fn add_target(&mut self, id: i64, team: u8) {
        self.targets.insert(id, true);
        self.set_team(id, team);
    }

    /// 靶子被打死（等价于一条 `LifeInfo{alive:false}`）。返回是否真的从活翻成死。
    pub fn kill_target(&mut self, id: i64) -> bool {
        match self.targets.get_mut(&id) {
            Some(alive) if *alive => {
                *alive = false;
                true
            }
            _ => false,
        }
    }

    /// 靶子**复原**（等价于一条 `LifeInfo{alive:true}`，真值 `0x454b40(id, true)`）。
    ///
    /// 形状是硬约束（`docs/123` §2.2/§4.3）：**只翻 alive 位**，不新建 id、不发
    /// `TargetInfo`、不碰掩膜。返回是否真的从死翻成活。
    pub fn restore_target(&mut self, id: i64) -> bool {
        match self.targets.get_mut(&id) {
            Some(alive) if !*alive => {
                *alive = true;
                true
            }
            _ => false,
        }
    }

    /// 靶子总数（`graphwar_target_manager` 的那张集合）。
    pub fn targets_total(&self) -> usize {
        self.targets.len()
    }

    /// 还活着的靶子数。
    pub fn targets_alive(&self) -> usize {
        self.targets.values().filter(|a| **a).count()
    }

    /// 这个 id 是不是靶子。
    pub fn is_target(&self, id: i64) -> bool {
        self.targets.contains_key(&id)
    }

    /// **某队还有没有存活实体**（在场士兵 ∪ 存活靶子）。
    ///
    /// 这就是胜负判定的**唯一判据**（真值 `0x473520` 的返回值：`(Team1 有活人, Team2 有活人)`）。
    /// 它数的是**生命表里的活实体**，靶子有 `LifeInfo` ⇒ 算在内；玩家实体没有 `LifeInfo`
    /// ⇒ 不算在内（`docs/150` §Q2：语料里 217 次 `EndingGame` 全部符合这条）。
    pub fn team_has_survivor(&self, team: u8) -> bool {
        // 士兵：死掉的已经被 `fire` 从 `self.soldiers` 里摘掉（`docs/150` §Q2 的注）
        let soldier = self
            .soldiers
            .iter()
            .any(|s| self.teams.get(&s.id).copied() == Some(team));
        if soldier {
            return true;
        }
        self.targets
            .iter()
            .any(|(id, alive)| *alive && self.teams.get(id).copied() == Some(team))
    }

    /// 设回合上限（真值 `0x472400`：写 `[self+0xc8]=1`、`[self+0xcc]=n`，再发 `TurnLimitInfo`）。
    ///
    /// 报文形状是**观测**：语料里 21 条全是 `{"TurnLimitInfo":{"turn_limit":N}}`
    /// （N ∈ {1,5,6,10}），没有第二条字段。
    pub fn set_turn_limit(&mut self, limit: i64) -> Vec<String> {
        self.turn_limit = Some(limit);
        vec![format!(r#"{{"TurnLimitInfo":{{"turn_limit":{limit}}}}}"#)]
    }

    /// 这一局结束了吗？`None` = 继续。
    ///
    /// 对应真值 `0x4759f0`（回合末）里的**两个**结束入口，按它的顺序：
    ///
    /// ```text
    /// 0x475a64  cmp turn_count, turn_limit      ; ① 回合上限用尽（且上限被设过）
    /// 0x475a6b  setae  /  0x475a77 test bpl,r15b
    /// 0x475a80  test al,1  je END               ; ② Team1 没有活实体
    /// 0x475a88  test dl,1  je END               ; ③ Team2 没有活实体
    /// ```
    ///
    /// 胜方（`0x475bfb`–`0x475c14` 写进 `[self+0x11f]` 的那个字节）：
    /// `0`=Team1、`1`=Team2、`2`=没人（两边都没活实体）。
    pub fn ending(&self) -> Option<Ending> {
        let limit_hit = matches!(self.turn_limit, Some(n) if self.turn_count >= n);
        let wiped_any = self
            .all_teams
            .iter()
            .any(|t| !self.team_has_survivor(*t));
        if !limit_hit && !wiped_any {
            return None;
        }
        let alive: Vec<u8> = self
            .all_teams
            .iter()
            .copied()
            .filter(|t| self.team_has_survivor(*t))
            .collect();
        let winner = if alive.len() == 1 {
            Some(alive[0])
        } else if alive.len() >= 2 && limit_hit {
            // 真值 `0x475c06`：`cmp ebp,1` 之后 `adc al,1` —— 上限用尽而两边都还有人时，
            // 胜方被**写死成 1（Team2）**。战役里 Team2 = 关卡方（靶子/对手），
            // 所以"上限用尽 ⇒ 玩家输"是原版行为，不是我们的发明。
            Some(1)
        } else {
            // 两边都没活实体：真值写 `2`（= 没人赢）。语料里没有这种收场。
            None
        };
        let reason = if limit_hit {
            EndingReason::TurnLimit
        } else {
            EndingReason::TeamWiped
        };
        Some(Ending { reason, winner })
    }

    /// 开始一个回合：`TurnInfo` -> `EndOfTurnInfo` -> `GameStateInfo(WaitingForFunctions)`。
    ///
    /// 顺序照真实抓包（`docs/43`）。
    pub fn begin_turn(&mut self, player_id: i64, soldier_id: i64, now_ms: i64, time_mode: &str) -> Vec<String> {
        let start_tick = self.tick_now(now_ms);
        self.turn_start_tick = start_tick;
        self.time_mode = time_mode.to_string();
        let end = crate::graphwar_tick_handler::end_of_turn_tick_wire(start_tick, time_mode);
        vec![
            crate::protocol::emit_turn_info(player_id, soldier_id),
            crate::protocol::emit_end_of_turn(end, time_mode),
            crate::protocol::emit_game_state("WaitingForFunctions"),
        ]
    }

    /// 回合结束：推进 `turn_count`，结束条件见 [`Match::ending`]。
    ///
    /// 依据真实抓包：`{"TurnCountInfo":{"turn_count":4}}` 之后紧跟
    /// `{"GameStateInfo":{"game_state":"EndingGame"}}`（`docs/45`）；回合上限也走同一条出口
    /// （`0x4759f0` 里 ①②③ 全都跳到 `0x475bac`）。
    pub fn end_turn(&mut self) -> Vec<String> {
        self.turn_count += 1;
        let mut out = vec![format!(
            r#"{{"TurnCountInfo":{{"turn_count":{}}}}}"#,
            self.turn_count
        )];
        if self.ending().is_some() {
            out.push(crate::protocol::emit_game_state("EndingGame"));
        }
        out
    }

    /// 推进到 `now_tick`，返回所有到期的 `DestroyObstacle`。
    pub fn tick(&mut self, now_tick: i64) -> Vec<String> {
        let mut due = Vec::new();
        let mut i = 0;
        while i < self.pending_destroy.len() {
            if self.pending_destroy[i].0 <= now_tick {
                due.push(self.pending_destroy.remove(i).1);
            } else {
                i += 1;
            }
        }
        due
    }

    /// 处理一次开火，返回要广播的消息（顺序见模块注释）。
    ///
    /// `owner_id` 是开火实体；`function_text` 是**未修正**的函数原文
    /// （修正由 `resolve` 内部完成）。
    ///
    /// 这是 [`Match::fire_batch`] 的 `N = 1` 特例：单发批的报文种类序列与
    /// 逐发实现**逐字节相同**，所以 `SequentialTurns` 的行为没有被这次重构改动。
    pub fn fire(
        &mut self,
        owner_id: i64,
        function_text: &str,
        x0: f64,
        y0: f64,
        flip: bool,
        now_ms: i64,
    ) -> Vec<String> {
        self.fire_batch(
            &[Submission { owner_id, function_text, x0, y0, flip }],
            now_ms,
        )
    }

    /// **同时回合制的一批开火**：收集阶段拿到的全部提交，一次结算、一次广播。
    ///
    /// 报文结构是 `docs/141` 从 130 个真回放的多发结算批里量出来的（`观测`）：
    ///
    /// ```text
    /// GameStateInfo(CalculatingResults)
    /// FunctionInfo   × N            ① 先按类型分组（射击者顺序 = 提交顺序）
    /// FunctionPoints × N            ①
    /// GameStateInfo(SynchronizingResults)
    /// [LifeQueuedDeath + EffectInfo(死亡)] × D   ② 按**弹丸飞行时间**排序
    /// [QueueDestroyObstacle] × B                 ③ 回放不记录，位置无证据
    /// (FunctionActive + EffectInfo(爆炸)) × N    ④ 与①同序
    /// GameStateInfo(ExecutingResults)
    /// ```
    ///
    /// 三条要点，每条都有独立证据（`docs/141` §2–§4）：
    ///
    /// 1. **几何并行**：所有弹道都对着**批开始那一刻**的士兵表与掩膜算，谁死不影响
    ///    别人的弹道（`docs/86` §5.1：152/152 死亡集合 = 逐发独立并集，多报 0）；
    /// 2. **事件按飞行时间串行**：死者按"沿弹道到命中点的弧长"升序排列，
    ///    同一士兵只出现一次、归最早到达的那一发（`docs/86` §6：时刻判决 38:0）；
    /// 3. **前缀按类型分组、后缀按发交替**：`FunctionInfo`/`FunctionPoints` 是
    ///    `N` 条同类连排，`FunctionActive+EffectInfo` 是每发一对（130/130 批实测）。
    ///
    /// 队列里的顺序（①④里"谁排前面"）**不是** id 序、**不是**提交到达序，
    /// 而是一个每局固定的全序（`docs/141` §4，判为 A 类）；本实现**照抄调用方给的
    /// 顺序**，把那个不可复现的置换留在接线层。
    pub fn fire_batch(&mut self, shots: &[Submission<'_>], now_ms: i64) -> Vec<String> {
        let now_tick = self.tick_now(now_ms);

        // ---- 收集阶段已经结束；这里是结算阶段 ----
        // ① 几何并行：先把每一发的弹道都算完，用的是**批开始时**的士兵表与掩膜。
        //    谁被打死**不影响**别人的弹道（士兵本就不挡弹道，`docs/30`）。
        let mut outcomes = Vec::with_capacity(shots.len());
        for s in shots {
            // 射手本人不参与致死判定
            let others: Vec<Target> = self
                .soldiers
                .iter()
                .filter(|t| {
                    !(t.id == s.owner_id)
                        && !((t.x - s.x0).abs() < 1e-12 && (t.y - s.y0).abs() < 1e-12)
                })
                .copied()
                .collect();
            // 步长用**实况 SERVER_STEP（0.01）**，不是 0.5。
            // `docs/136` 的几何探针给出：同一批回放，step=0.01 时 `end_x` 逐位相同 605/739；
            // step=0.5 只有 27/739。
            outcomes.push(resolve(
                s.function_text,
                s.x0,
                s.y0,
                crate::SERVER_STEP,
                s.flip,
                crate::graphwar_path_calculator::Mode::Normal,
                &self.mask,
                &others,
            ));
        }

        // ② 合并死亡：按 (弧长, 发序号, 士兵 id) 升序；同一士兵只留最早到达的那一发。
        //    同一回合所有弹丸的 `active_tick` 相同（13/13 回合），所以弧长序 == 到达时刻序。
        let mut merged: Vec<(f64, usize, i64)> = Vec::new();
        for (si, out) in outcomes.iter().enumerate() {
            for h in &out.hits {
                merged.push((h.arc, si, h.id));
            }
        }
        merged.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
                .then(a.2.cmp(&b.2))
        });
        let mut deaths: Vec<i64> = Vec::with_capacity(merged.len());
        for (_, _, id) in &merged {
            if !deaths.contains(id) {
                deaths.push(*id);
            }
        }

        // ---- 广播阶段 ----
        // ① 按类型分组的前缀
        let mut msgs = vec![crate::protocol::emit_game_state("CalculatingResults")];
        for s in shots {
            msgs.push(crate::protocol::emit_function_info(
                s.owner_id,
                s.function_text,
                Some(&crate::shot::corrected_text(s.function_text, s.x0, s.y0, s.flip)),
            ));
        }
        for (s, out) in shots.iter().zip(outcomes.iter()) {
            msgs.push(emit_function_points(s.owner_id, s.x0, s.y0, out.stop.0));
        }
        msgs.push(crate::protocol::emit_game_state("SynchronizingResults"));

        // ② 死亡块：顺序 = 到达时刻（`docs/86` §6）
        for id in &deaths {
            msgs.push(emit_life_queued_death(*id, now_tick));
            if let Some(t) = self.soldiers.iter().find(|t| t.id == *id) {
                self.next_effect_id += 1;
                msgs.push(emit_effect_info(self.next_effect_id, t.x, t.y, "DeathEffect", now_tick));
            }
        }

        // ③ 挖洞块（回放不记录 `QueueDestroyObstacle`，位置**无证据**；沿用单发实现的
        //    "死亡之后、`FunctionActive` 之前"，这样 N=1 时与原实现逐字节相同）
        for out in &outcomes {
            if !out.blocked {
                continue;
            }
            msgs.push(format!(
                r#"{{"QueueDestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5,"tick":{}}}}}"#,
                out.stop.0, out.stop.1, now_tick
            ));
            // 真实游戏：在命中点用 fill=0xFF、半径 0.5 挖洞（docs/39）
            rasterize(&mut self.mask, out.stop.0, out.stop.1, 0.5, 0xFF);
            // 真正的删除要延后：实测 ≈165 tick 之后才发 DestroyObstacle
            self.pending_destroy.push((
                now_tick + DESTROY_DELAY_TICKS,
                format!(
                    r#"{{"DestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5}}}}"#,
                    out.stop.0, out.stop.1
                ),
            ));
        }

        // ④ 每发一对 `FunctionActive` + `ExplosionEffect`，顺序与①相同（130/130 批实测）
        for (s, out) in shots.iter().zip(outcomes.iter()) {
            msgs.push(crate::protocol::emit_function_active(s.owner_id, now_tick));
            self.next_effect_id += 1;
            msgs.push(emit_effect_info(
                self.next_effect_id,
                out.stop.0,
                out.stop.1,
                "ExplosionEffect",
                now_tick,
            ));
        }
        msgs.push(crate::protocol::emit_game_state("ExecutingResults"));

        // ---- 状态落地（放在报文之后：报文里的坐标与 id 都来自批开始时的快照）----
        if !deaths.is_empty() {
            // 记进 `life_manager` 那张表：轮转据此判活，死掉的返回 0（而不是 2「查不到」），
            // 两者都不可开火（`docs/98` §1.3），但"死过"这件事要留下来。
            for id in &deaths {
                if let Ok(v) = u32::try_from(*id) {
                    self.life.set(v, Status::Dead, None);
                }
            }
            self.soldiers.retain(|t| !deaths.contains(&t.id));
        }

        msgs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{get_int, get_str, parse_message};

    pub(super) fn room() -> RoomState {
        RoomState {
            room_name: "Graphwar Game".into(),
            game_state: "Setup".into(),
            axis_mode: "EveryUnit".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
        }
    }

    fn kind(msg: &str) -> &str {
        &msg[2..msg.find('"').unwrap_or(2)] // 简易取键名
    }

    fn kinds(msgs: &[String]) -> Vec<String> {
        msgs.iter()
            .map(|m| {
                let end = m[2..].find('"').unwrap() + 2;
                m[2..end].to_string()
            })
            .collect()
    }

    /// 报文顺序必须与真实抓包一致（docs/43）。
    #[test]
    fn fire_emits_captured_turn_sequence() {
        // 一个"被障碍挡住、没打死人"的回合（与抓包里那一回合同形）
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (5, 0));
        let msgs = m.fire(1, "0.00", 10.0, 0.0, true, 1000);
        assert_eq!(
            kinds(&msgs),
            vec![
                "GameStateInfo",
                "FunctionInfo",
                "FunctionPoints",
                "GameStateInfo",
                "QueueDestroyObstacle",
                "FunctionActive",
                "EffectInfo",
                "GameStateInfo",
            ],
            "回合内报文顺序与抓包不符"
        );
        // 状态名也要对
        assert!(msgs[0].contains("CalculatingResults"));
        assert!(msgs[3].contains("SynchronizingResults"));
        assert!(msgs[7].contains("ExecutingResults"));
    }

    #[test]
    fn killed_soldier_appears_between_sync_and_active() {
        let soldiers = vec![
            Target { id: 10, x: 5.0, y: 0.0, radius: 0.5 },
            Target { id: 11, x: -2.0, y: 0.0, radius: 0.5 },
        ];
        let mut m = Match::new(room(), vec![], soldiers, (5, 0));
        let msgs = m.fire(10, "0.00", 5.0, 0.0, true, 1000);
        let ks = kinds(&msgs);
        let death = ks.iter().position(|k| k == "LifeQueuedDeath").expect("应有死亡消息");
        let active = ks.iter().position(|k| k == "FunctionActive").unwrap();
        assert!(death < active, "死亡消息应出现在 FunctionActive 之前");
        assert!(msgs.iter().any(|s| s.contains(r#""entity_id":11"#)), "应记录 11 号死亡");
    }

    #[test]
    fn tick_comes_from_the_clock() {
        let mut m = Match::new(room(), vec![], vec![], (5, 0));
        let msgs = m.fire(99, "0.00", 10.0, 0.0, true, 1000);
        // 基准 tick=5，1000ms 后 = 5 + 60 = 65
        assert!(msgs.iter().any(|s| s.contains(r#""start_tick":65"#)), "tick 应为 65: {msgs:?}");
    }

    #[test]
    fn blocked_shot_also_queues_obstacle_destruction() {
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (0, 0));
        let msgs = m.fire(1, "0.00", 10.0, 0.0, true, 0);
        assert!(msgs.iter().any(|s| s.starts_with(r#"{"QueueDestroyObstacle""#)), "应排队销毁障碍");
    }

    /// 一整回合的报文种类序列，必须与抓包一致（docs/43）。
    #[test]
    fn full_turn_sequence_matches_capture() {
        use crate::protocol::parse_message;
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (0, 0));
        let mut all = Vec::new();
        all.extend(m.begin_turn(229, 231, 0, "Timer1m"));
        all.extend(m.fire(231, "0.00", 10.0, 0.0, true, 1000));
        all.extend(m.end_turn());
        let kinds: Vec<String> = all.iter()
            .map(|x| parse_message(x).expect("应为合法消息").kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                "TurnInfo", "EndOfTurnInfo", "GameStateInfo",          // 回合开始
                "GameStateInfo", "FunctionInfo", "FunctionPoints", "GameStateInfo",
                "QueueDestroyObstacle", "FunctionActive", "EffectInfo", "GameStateInfo", // 开火
                "TurnCountInfo",                                        // 回合结束
            ]
        );
    }

    #[test]
    fn end_turn_counts_and_detects_annihilation() {
        let soldiers = vec![
            Target { id: 1, x: 10.0, y: 0.0, radius: 0.5 },
            Target { id: 2, x: 0.0, y: 0.0, radius: 0.5 },
        ];
        let mut m = Match::new(room(), vec![], soldiers, (0, 0));
        m.set_team(1, 1);
        m.set_team(2, 2);
        // 第一回合：没人死，只推进计数
        let out = m.end_turn();
        assert_eq!(out.len(), 1);
        assert!(out[0].contains(r#""turn_count":1"#));
        // 打死 2 号（Team2 全灭）
        m.fire(1, "0.00", 10.0, 0.0, true, 0);
        assert_eq!(m.soldiers.len(), 1);
        let out = m.end_turn();
        assert_eq!(out.len(), 2, "全灭时应多一条 EndingGame: {out:?}");
        assert!(out[1].contains("EndingGame"));
    }

    #[test]
    fn destroy_is_delayed_by_measured_ticks() {
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (0, 0));
        // 基准 (tick=0, ms=0)，fire 在 1000ms 处 -> 当时 tick = 60
        let msgs = m.fire(1, "0.00", 10.0, 0.0, true, 1000);
        assert!(msgs.iter().any(|s| s.starts_with(r#"{"QueueDestroyObstacle""#)));
        let queued_at = 60; // tick_at(0, 0, 1000)
        assert!(m.tick(queued_at).is_empty(), "刚排队时不该立刻删除");
        assert!(m.tick(queued_at + DESTROY_DELAY_TICKS - 1).is_empty(), "差一个 tick 时仍不该删除");
        let due = m.tick(queued_at + DESTROY_DELAY_TICKS);
        assert_eq!(due.len(), 1, "到期应恰好发出一条");
        assert!(due[0].starts_with(r#"{"DestroyObstacle""#));
        assert!(due[0].contains(r#""radius":0.5"#));
        assert!(m.tick(queued_at + DESTROY_DELAY_TICKS + 100).is_empty(), "不应重复发出");
    }

    #[test]
    fn blocked_shot_punches_a_hole_in_the_mask() {
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (0, 0));
        let first = m.fire(1, "0.00", 10.0, 0.0, true, 0);
        let stop_x = first
            .iter()
            .find_map(|s| s.strip_prefix(r#"{"FunctionPoints":{"owner_id":1,"start_x":10,"start_y":0,"end_x":"#))
            .and_then(|r| r.split('}').next())
            .and_then(|v| v.parse::<f64>().ok())
            .expect("应能解析终点");
        // 同一发再打一次：洞口已经挖开，应飞得更远
        let second = m.fire(1, "0.00", 10.0, 0.0, true, 1000);
        let stop2 = second
            .iter()
            .find_map(|s| s.strip_prefix(r#"{"FunctionPoints":{"owner_id":1,"start_x":10,"start_y":0,"end_x":"#))
            .and_then(|r| r.split('}').next())
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap();
        assert!(stop2 < stop_x, "挖洞后应飞得更远（{stop2} 应小于 {stop_x}）");
    }

    /// 友军误伤（用户游戏内实测：会致死），且按弹道**先后顺序**结算。
    #[test]
    fn friendly_fire_kills_teammates_in_path_order() {
        let soldiers = vec![
            Target { id: 1, x: -10.0, y: 0.0, radius: 0.5 }, // 射手（Team1）
            Target { id: 2, x: -2.0, y: 0.0, radius: 0.5 },  // 队友（Team1）
            Target { id: 3, x: 5.0, y: 0.0, radius: 0.5 },   // 敌人（Team2）
        ];
        let mut m = Match::new(room(), vec![], soldiers, (0, 0));
        m.set_team(1, 1);
        m.set_team(2, 1);
        m.set_team(3, 2);
        // 水平直线向右：先穿过队友 2，再穿过敌人 3
        let msgs = m.fire(1, "0.00", -10.0, 0.0, false, 0);

        let killed: Vec<i64> = msgs
            .iter()
            .filter_map(|x| parse_message(x))
            .filter(|p| p.kind == "LifeQueuedDeath")
            .map(|p| get_int(&p, "entity_id").unwrap())
            .collect();
        assert_eq!(killed, vec![2, 3], "应先队友(2)后敌人(3)，且射手(1)不自伤");
        assert_eq!(m.soldiers.len(), 1, "两人都应被移除，只剩射手");
    }

    #[test]
    fn killed_soldiers_leave_the_field() {
        // 第二个射手再打同一位置，不应重复记录同一个人
        let soldiers = vec![
            Target { id: 1, x: 10.0, y: 0.0, radius: 0.5 },
            Target { id: 2, x: 0.0, y: 0.0, radius: 0.5 },
        ];
        let mut m = Match::new(room(), vec![], soldiers, (0, 0));
        m.fire(1, "0.00", 10.0, 0.0, true, 0);
        assert_eq!(m.soldiers.len(), 1, "死者应被移出");
        let second = m.fire(1, "0.00", 10.0, 0.0, true, 1000);
        assert!(!second.iter().any(|s| s.contains("LifeQueuedDeath")), "不该重复计死");
    }

    #[test]
    fn shooter_never_kills_itself() {
        let soldiers = vec![Target { id: 7, x: 0.0, y: 0.0, radius: 0.5 }];
        let mut m = Match::new(room(), vec![], soldiers, (0, 0));
        let msgs = m.fire(7, "0.00", 0.0, 0.0, true, 0);
        assert!(!msgs.iter().any(|s| s.contains(r#"{"LifeQueuedDeath""#)), "射手不该判定自己死亡");
    }

    // -----------------------------------------------------------------
    // 接线：射手轮转 + 按士兵的 AI 进化（`docs/117-server-wiring.md`）
    // -----------------------------------------------------------------

    /// 无遮挡的四人局面。坐标与 `serverloop` 的固定布局一致，但**没有障碍**，
    /// 所以"哪一发打死谁"是确定的，可以写死在断言里。
    fn wired(ai: AiConfig, bots: &[i64]) -> Match {
        let soldiers = vec![
            Target { id: 230, x: -10.0, y: -4.0, radius: 0.5 },
            Target { id: 231, x: -10.0, y: 0.0, radius: 0.5 },
            Target { id: 232, x: 10.0, y: -4.0, radius: 0.5 },
            Target { id: 233, x: 10.0, y: 0.0, radius: 0.5 },
        ];
        let mut m = Match::new(room(), vec![], soldiers, (0, 0))
            .with_ai(ai)
            .with_ai_seed(&[7u8; 32]);
        // 士兵顺序原样给（`docs/98` §6-C：顺序不是我推的，是调用方给的）。
        m.register_player(229, crate::graphwar_turn_manager::Team::One, &[230, 231]);
        m.register_player(230, crate::graphwar_turn_manager::Team::Two, &[232, 233]);
        for p in bots {
            m.set_bot(*p);
        }
        m
    }

    /// 不限时的 AI 配置（可复现；墙钟截断属 `docs/06` §17.2 的 B 类）。
    fn replay_ai() -> AiConfig {
        AiConfig { budget_secs: None, ..AiConfig::default() }
    }

    fn announced(st: &TurnStart) -> (i64, i64) {
        st.decision.broadcast().expect("本回合应广播 TurnInfo")
    }

    #[test]
    fn turn_rotation_decides_the_shooter() {
        let mut m = wired(replay_ai(), &[]);
        // 游标初值 0 + "先自增再取"（`docs/109` §2.2）⇒ 先手是 `soldiers[1]`，不是 `[0]`。
        let a = m.begin_next_turn(229, false, 0, "Timer1m");
        assert_eq!(announced(&a), (229, 231));
        assert_eq!(kinds(&a.messages), vec!["TurnInfo", "EndOfTurnInfo", "GameStateInfo"]);
        assert!(a.messages[0].contains(r#""soldier_id":231"#));
        // `flip` 由**队伍**决定（`docs/105`），接口里没有坐标。
        assert_eq!(a.decision.flip(), Some(false), "Team1 朝 +x");
        let b = m.begin_next_turn(230, false, 0, "Timer1m");
        assert_eq!(announced(&b), (230, 233));
        assert_eq!(b.decision.flip(), Some(true), "Team2 朝 -x");
        // 回绕到各自的下一位（游标是**每玩家**一份）。
        assert_eq!(announced(&m.begin_next_turn(229, false, 0, "Timer1m")), (229, 230));
        assert_eq!(announced(&m.begin_next_turn(230, false, 0, "Timer1m")), (230, 232));
        assert_eq!(announced(&m.begin_next_turn(229, false, 0, "Timer1m")), (229, 231));
    }

    #[test]
    fn dead_soldier_is_skipped_by_the_rotation() {
        let mut m = wired(replay_ai(), &[]);
        // 231 在 (-10,0) 朝 +x 打水平线，穿过 (10,0) 的 233。
        let msgs = m.fire(231, "0.00", -10.0, 0.0, false, 0);
        assert!(msgs.iter().any(|s| s.contains(r#""entity_id":233"#)), "233 应被打死");
        // 玩家 230 的游标仍是 0 ⇒ 先自增到 1 = 233（死）⇒ 再自增回绕到 0 = 232。
        let d = m.begin_next_turn(230, false, 0, "Timer1m");
        assert_eq!(announced(&d), (230, 232), "死者应被跳过");
        // 232 活着 ⇒ 下一次还是它（跳过只对死者发生）。
        let e = m.begin_next_turn(230, false, 0, "Timer1m");
        assert_eq!(announced(&e), (230, 232));
    }

    #[test]
    fn all_dead_roster_still_announces_zero() {
        let soldiers = vec![
            Target { id: 231, x: -10.0, y: 0.0, radius: 0.5 },
            Target { id: 232, x: 10.0, y: 0.0, radius: 0.5 },
        ];
        let mut m = Match::new(room(), vec![], soldiers, (0, 0)).with_ai(replay_ai());
        m.register_player(229, crate::graphwar_turn_manager::Team::One, &[231]);
        m.register_player(230, crate::graphwar_turn_manager::Team::Two, &[232]);
        m.set_bot(230);
        m.fire(231, "0.00", -10.0, 0.0, false, 0);
        assert!(m.soldiers.iter().all(|s| s.id != 232));
        let d = m.begin_next_turn(230, false, 0, "Timer1m");
        // `Some(0)`：**仍然**广播 `TurnInfo{p,0}`（`0x463fd0`，`docs/109` §2.4）。
        assert_eq!(d.decision.broadcast(), Some((230, 0)), "全灭也要发（广播层开关）");
        assert!(d.messages.iter().any(|s| s.contains(r#""soldier_id":0"#)));
        assert!(d.evolution.is_none(), "全灭回合没有士兵可进化");
        assert_eq!(m.evolutions, 0);
    }

    #[test]
    fn timeout_skips_the_player_without_moving_the_cursor() {
        // ⚠️ "回合超时 = 跳过该玩家"是**用户口述、未独立验证**的规则
        // （`docs/HANDOVER` §7.10 第 2 条）。本测试只钉住"这个分支做了什么"，
        // 不是"真游戏就是这么做的"。
        let mut m = wired(replay_ai(), &[229]);
        let d = m.begin_next_turn(229, true, 0, "Timer1m");
        assert_eq!(
            d.decision.skip_reason(),
            Some(crate::graphwar_turn_manager::SkipReason::TurnTimeout),
            "超时必须是显式的 TurnTimeout，不是别的原因"
        );
        assert!(d.messages.is_empty(), "超时不发报文");
        assert!(d.evolution.is_none(), "超时不进化");
        assert_eq!(m.evolutions, 0);
        // 游标**不推进**（本实现的取法；真行为未定，见 `docs/109` §2.7 第 1 条）。
        let d = m.begin_next_turn(229, false, 0, "Timer1m");
        assert_eq!(announced(&d), (229, 231), "超时不该用掉这一次轮转");
    }

    #[test]
    fn unknown_player_is_skipped() {
        let mut m = wired(replay_ai(), &[]);
        let d = m.begin_next_turn(999, false, 0, "Timer1m");
        assert_eq!(d.decision.skip_reason(), Some(crate::graphwar_turn_manager::SkipReason::UnknownPlayer));
        assert!(d.messages.is_empty());
    }

    #[test]
    fn ai_evolves_only_for_computer_players_and_only_its_own_soldier() {
        // 没有电脑玩家 ⇒ 一次都不跑（`computer_player_manager` 只服务电脑玩家）。
        let mut m = wired(replay_ai(), &[]);
        assert!(m.begin_next_turn(229, false, 0, "Timer1m").evolution.is_none());
        assert_eq!(m.evolutions, 0);

        // 标成电脑玩家 ⇒ 只为**轮转判出来的那个**士兵进化一次。
        let mut m = wired(replay_ai(), &[229]);
        let d = m.begin_next_turn(229, false, 0, "Timer1m");
        let ev = d.evolution.expect("电脑玩家的回合应进化");
        assert_eq!(ev.soldier_id, 231, "进化的必须是轮转判出的射手");
        assert_eq!(m.evolutions, 1);
        assert!(m.functions().tokens(231).is_some_and(|t| !t.is_empty()), "结果要写进它自己的槽");
        // 同一玩家的另一个士兵、另一个玩家的士兵都不该被动过（`docs/98` §3.3）。
        assert!(m.functions().slot(230).expect("230 应已登记").is_empty());
        assert!(m.functions().slot(232).expect("232 应已登记").is_empty());
        assert!(m.functions().slot(233).expect("233 应已登记").is_empty());
        assert_eq!(m.functions().updated_turn(231), Some(0));
        assert_eq!(m.functions().updated_turn(230), None);
    }

    #[test]
    fn evolution_needs_a_real_soldier_on_the_field() {
        let mut m = wired(replay_ai(), &[229]);
        assert!(m.evolve_for_soldier(229, 0, false, 0).is_none(), "0 是全灭哨兵");
        assert!(m.evolve_for_soldier(229, 999, false, 0).is_none(), "不在场上的士兵没有局面");
        assert!(m.evolve_for_soldier(230, 232, true, 0).is_none(), "不是电脑玩家就不进化");
        assert_eq!(m.evolutions, 0);
    }

    /// 同一 seed + 同一串回合 + 不限时 ⇒ 函数槽**逐位相同**
    /// （`docs/06` §17.1 的"给定同一串消息，状态转移函数逐位"）。
    #[test]
    fn wiring_is_deterministic_for_the_same_seed() {
        fn run() -> (Vec<Option<Vec<crate::Token>>>, u64) {
            let mut m = wired(replay_ai(), &[229, 230]);
            for (pid, sid) in [(229, 231), (230, 233), (229, 230), (230, 232)] {
                let st = m.begin_next_turn(pid, false, 0, "Timer1m");
                assert_eq!(announced(&st), (pid, sid), "轮转序列本身也要确定");
            }
            let slots = [230, 231, 232, 233]
                .iter()
                .map(|id| m.functions().tokens(*id).map(|t| t.to_vec()))
                .collect();
            (slots, m.evolutions)
        }
        let (a, ea) = run();
        let (b, eb) = run();
        assert_eq!((ea, eb), (4, 4), "四名士兵各进化一次");
        assert!(
            a.iter().all(|s| s.as_ref().is_some_and(|t| !t.is_empty())),
            "四名士兵的槽都该有内容：{a:?}"
        );
        assert_eq!(a, b, "两局必须逐位相同：不许引入时间/随机/遍历顺序");
    }

    #[test]
    fn ai_budget_defaults_to_the_game_value() {
        // 真游戏 25 s（`0x51b7cf cmp $0x19`，`docs/93`）。此前 `evolve()` 传的是 `None`，
        // 上限**实际没生效**；接线后默认必须接上。
        assert_eq!(AiConfig::default().budget_secs, Some(crate::graphwar_function_evolutionator::GAME_TIME_BUDGET_SECS));
        assert_eq!(AiConfig::default().budget_secs, Some(25.0));
        let m = wired(AiConfig::default(), &[229]);
        assert_eq!(m.ai_config().budget_secs, Some(25.0), "Match 要把它交给 SoldierEvolver::with_budget");
        assert_eq!(wired(replay_ai(), &[]).ai_config().budget_secs, None, "测试口径可关掉");
    }

    // -----------------------------------------------------------------
    // AI 说话：token → 文本 → `FunctionInfo`（`docs/127-ai-speaks.md`）
    // -----------------------------------------------------------------

    /// 本回合 AI 报的那条 `FunctionInfo`（键盘名 + 字段都过一遍协议解析器）。
    fn ai_info(st: &TurnStart) -> Option<crate::protocol::RawMessage> {
        st.messages
            .iter()
            .filter(|m| m.starts_with(r#"{"FunctionInfo""#))
            .filter_map(|m| parse_message(m))
            .next()
    }

    /// **接线验收（内核层）**：电脑玩家的回合里必须有一条 `FunctionInfo`，
    /// `owner_id` 是 **soldier_id**、文本是槽里 token 的 `to_text`、修正字段是 `null`。
    #[test]
    fn ai_turn_broadcasts_its_function_as_text() {
        let mut m = wired(replay_ai(), &[229]);
        let st = m.begin_next_turn(229, false, 0, "Timer1m");
        assert_eq!(announced(&st), (229, 231));
        // 回合头三条 + AI 的一条，顺序照真实抓包（`FunctionInfo` 在 `WaitingForFunctions` 之后）。
        assert_eq!(
            kinds(&st.messages),
            vec!["TurnInfo", "EndOfTurnInfo", "GameStateInfo", "FunctionInfo"],
            "AI 的话必须落在 WaitingForFunctions 后面：{:?}",
            st.messages
        );
        let info = ai_info(&st).expect("电脑玩家的回合应有一条 FunctionInfo");
        assert_eq!(get_int(&info, "owner_id"), Some(231), "owner_id 必须是 soldier_id");
        let text = get_str(&info, "function").expect("应有 function 字段");
        assert!(!text.is_empty(), "进化过就不是空串");
        // 文本**只**来自槽里的 token（不是常量、不是客户端给的那条 "0.00"）。
        let toks = m.functions().tokens(231).expect("231 应已登记");
        assert_eq!(text, crate::function::to_text(toks));
        assert_eq!(Some(text.as_str()), m.ai_function_text(231).as_deref());
        // 提交时没有修正文本（抓包帧 135/158/200）。
        assert!(get_str(&info, "corrected_function").is_none(), "提交时的 corrected_function 是 null");
        assert_eq!(get_str(&info, "corrected_function"), None);
        // 与 `TurnStart.evolution`（同一份 token）同源。
        let ev = st.evolution.as_ref().expect("电脑玩家应进化");
        assert_eq!(crate::function::to_text(&ev.tokens), text);
    }

    /// 人类玩家的回合**一条都不多**（不替人类编函数）。
    #[test]
    fn human_turns_do_not_speak() {
        let mut m = wired(replay_ai(), &[]);
        let st = m.begin_next_turn(229, false, 0, "Timer1m");
        assert_eq!(kinds(&st.messages), vec!["TurnInfo", "EndOfTurnInfo", "GameStateInfo"]);
        assert!(st.evolution.is_none());
    }

    /// 多个 AI：各说各的，`owner_id` 各自是自己的 `soldier_id`，文本各自对应自己的槽。
    #[test]
    fn each_ai_speaks_for_its_own_soldier() {
        let mut m = wired(replay_ai(), &[229, 230]);
        let a = m.begin_next_turn(229, false, 0, "Timer1m");
        let sa = announced(&a).1;
        let ta = get_str(&ai_info(&a).expect("第一个 AI 应说话"), "function").unwrap();
        let b = m.begin_next_turn(230, false, 0, "Timer1m");
        let sb = announced(&b).1;
        let tb = get_str(&ai_info(&b).expect("第二个 AI 应说话"), "function").unwrap();
        assert_eq!(sa, 231);
        assert_eq!(sb, 233);
        assert_eq!(get_int(&ai_info(&a).unwrap(), "owner_id"), Some(sa));
        assert_eq!(get_int(&ai_info(&b).unwrap(), "owner_id"), Some(sb));
        // 互换就说明写串了：各自的文本必须等于各自槽里的 token 转出来那份。
        assert_eq!(ta, m.ai_function_text(sa).unwrap());
        assert_eq!(tb, m.ai_function_text(sb).unwrap());
        assert_ne!(ta, tb, "两个士兵是两份独立种群，进化出来的 token 不该一样");
    }

    /// `owner_id` 不是 `player_id`：玩家 229 的射手是 231，报文里必须是 231。
    #[test]
    fn ai_function_info_owner_is_the_soldier_not_the_player() {
        let mut m = wired(replay_ai(), &[229]);
        let st = m.begin_next_turn(229, false, 0, "Timer1m");
        let info = ai_info(&st).expect("应有 FunctionInfo");
        assert_eq!(get_int(&info, "owner_id"), Some(231));
        assert_eq!(get_int(&info, "player_id"), None, "不该出现 player_id 键");
        // `Match::fire` 那条（修正版）也是同一个 owner_id。
        let msgs = m.fire(231, &m.ai_function_text(231).unwrap(), -10.0, 0.0, false, 0);
        let fired = msgs
            .iter()
            .filter_map(|x| parse_message(x))
            .find(|p| p.kind == "FunctionInfo")
            .expect("开火必须带 FunctionInfo");
        assert_eq!(get_int(&fired, "owner_id"), Some(231));
    }

    /// 空槽（还没进化过）也得能发：与抓包里那条空 `FunctionInfo` 同形；
    /// 修正版就是抓样的 `" + 10.990"`（`docs/24` §6.2）。
    #[test]
    fn empty_slot_speaks_the_captured_empty_form() {
        let m = wired(replay_ai(), &[]);
        assert_eq!(
            m.ai_submit_info(231).expect("登记过就该有槽"),
            r#"{"FunctionInfo":{"owner_id":231,"function":"","corrected_function":null}}"#
        );
        assert_eq!(
            crate::shot::corrected_text("", -19.512760846799154, 10.99018276610812, false),
            " + 10.990",
            "空函数的修正文本就是抓包里那条"
        );
    }

    /// AI 的文本**能被 `parse` 吃回去**（`docs/119` §5：`to_text` 不是单射，
    /// 但"文本 → token → 文本"对本局这批 token 是恒等 —— 不动点）。
    #[test]
    fn ai_text_round_trips_through_the_parser() {
        let mut m = wired(replay_ai(), &[229, 230]);
        let mut seen = 0;
        for pid in [229, 230, 229, 230] {
            let st = m.begin_next_turn(pid, false, 0, "Timer1m");
            let text = get_str(&ai_info(&st).expect("两个都是电脑玩家"), "function").unwrap();
            let toks = crate::function::parse(&text);
            assert!(!toks.is_empty(), "文本解析回空 token 流：{text}");
            assert_eq!(crate::function::to_text(&toks), text, "文本不是不动点（{text}）");
            seen += 1;
        }
        assert_eq!(seen, 4);
    }

    /// 进化种群的归属（`docs/98` §6-A **未定**）在缓存条数上分叉 —— 一个显式开关。
    #[test]
    fn population_scope_is_explicit() {        let mut per_soldier =
            wired(AiConfig { pop_scope: PopScope::Soldier, ..replay_ai() }, &[229, 230]);
        let mut per_player =
            wired(AiConfig { pop_scope: PopScope::Player, ..replay_ai() }, &[229, 230]);
        for m in [&mut per_soldier, &mut per_player] {
            for pid in [229, 230, 229, 230] {
                m.begin_next_turn(pid, false, 0, "Timer1m");
            }
        }
        assert_eq!(per_soldier.ai_population_entries(), 4, "按士兵：4 份种群");
        assert_eq!(per_player.ai_population_entries(), 2, "按玩家：2 份种群");
    }

    /// `PlayerInfo` 的报文文本必须与抓包**逐字节**相同（`docs/139` §4）。
    ///
    /// 夹具里那两行是原版客户端的入向报文（`fixtures/protocol_match.jsonl:36–37`），
    /// 所以这条测试同时钉住"字段名 / 顺序 / 没有空白"三件事。
    #[test]
    fn player_info_message_matches_the_captured_bytes() {
        let captured = include_str!("../fixtures/protocol_match.jsonl")
            .lines()
            .find(|l| l.starts_with(r#"{"PlayerInfo""#))
            .expect("夹具里应有 PlayerInfo 样本");
        assert_eq!(player_info_message(229, 228), captured);
        // 第二个样本（同一个 connection_id 上的第二名玩家）也走同一个函数。
        assert_eq!(
            player_info_message(232, 228),
            r#"{"PlayerInfo":{"player_id":232,"connection_id":228}}"#
        );
        // 解析回来仍然是一条单键消息，键名 = `PlayerInfo`（客户端按它分派）。
        let msg = crate::protocol::parse_message(&player_info_message(229, 228))
            .expect("应能解析回 RawMessage");
        assert_eq!(msg.kind, "PlayerInfo");
        assert_eq!(get_int(&msg, "player_id"), Some(229));
        assert_eq!(get_int(&msg, "connection_id"), Some(228));
    }
}

