//! 一局对战的服务端对象：把已有的零件装起来。
//!
//! 组成：
//!
//! * 房间状态（`server::RoomState`）
//! * 关卡障碍（用于建掩膜）
//! * 士兵（致死判定）
//! * tick 基准（`tick::tick_at`）
//! * **射手轮转**（`turn::TurnManager`）与**按士兵的 AI 进化**（`aigen::SoldierEvolver`）
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
//!   ① turn::TurnManager::begin_turn  -> TurnDecision（三态）
//!   ② 只为判出来的 soldier_id 调 aigen::SoldierEvolver::evolve_soldier（25 s 预算）
//!   ③ decision.broadcast() == Some((p, s)) 才发报文；Skip 时 messages 为空
//! ```
//!
//! **没接的**（都写在 `docs/117` §6）：`token → 文本`（`0x4ec160`）未实现，所以 AI
//! 进化出来的函数只能进槽、进不了 `FunctionInfo`；回合超时的**检测**没有证据，
//! 本层只把 `timed_out` 当输入参数；游标初值取 `PlayerTurns::new` 的 0
//! （`TurnManager::add_player` 没有 index 入口，见 `docs/98` §6-B）。

use crate::aigen::{self, Individual, SoldierEvolver, SoldierFunctions, SoldierTurn, VarMode};
use crate::life::{LifeState, Status};
use crate::plane::{empty_mask, rasterize};
use crate::protocol::{emit_effect_info, emit_function_points, emit_life_queued_death};
use crate::rng::ChaCha12;
use crate::server::RoomState;
use crate::settle::{resolve, Target};
use crate::tick::tick_at;
use crate::turn::{self, TurnDecision, TurnManager};

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
    /// 士兵 → 队伍（用于判定"某队全灭"）。
    pub teams: std::collections::BTreeMap<i64, u8>,
    /// 这一局出现过的所有队伍。
    all_teams: std::collections::BTreeSet<u8>,
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
/// `aigen::SoldierEvolver` 因此**不持有种群**（`docs/109` §4.3）；归属这个决定落在
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
/// * `mode`：`aigen::VarMode`（`docs/94`）。**没有**从 `room.function_mode` 推——
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
            budget_secs: Some(aigen::GAME_TIME_BUDGET_SECS),
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
    pub fn register_player(&mut self, player_id: i64, team: turn::Team, soldiers: &[i64]) {
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
            let state = turn::life_state_of(&self.life);
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
        }
        TurnStart { decision, messages, evolution }
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
        if soldier_id == turn::SOLDIER_ID_NONE || !self.is_bot(player_id) {
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
        let params = aigen::params(self.ai.level);
        let mode = self.ai.mode;
        let budget = self.ai.budget_secs;
        let pop = self.ai_pops.remove(&key);
        // 评分上下文借用 mask/soldiers，写回借用 functions/ai_rng —— 拆开借，互不重叠。
        let out = {
            let Match { soldiers, mask, functions, ai_rng, .. } = self;
            let others: Vec<Target> = soldiers.iter().filter(|t| t.id != soldier_id).copied().collect();
            let ctx = aigen::ScoringContext {
                x0,
                y0,
                flip,
                // `evaluate` 内部按 `Phase` 覆盖成 0.5 / 0.01，这里只是基值。
                step: aigen::COARSE_STEP,
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

    /// 登记某名士兵属于哪支队伍。
    pub fn set_team(&mut self, soldier_id: i64, team: u8) {
        self.teams.insert(soldier_id, team);
        self.all_teams.insert(team);
    }

    /// 开始一个回合：`TurnInfo` -> `EndOfTurnInfo` -> `GameStateInfo(WaitingForFunctions)`。
    ///
    /// 顺序照真实抓包（`docs/43`）。
    pub fn begin_turn(&mut self, player_id: i64, soldier_id: i64, now_ms: i64, time_mode: &str) -> Vec<String> {
        let start_tick = self.tick_now(now_ms);
        self.turn_start_tick = start_tick;
        self.time_mode = time_mode.to_string();
        let end = crate::tick::end_of_turn_tick_wire(start_tick, time_mode);
        vec![
            crate::protocol::emit_turn_info(player_id, soldier_id),
            crate::protocol::emit_end_of_turn(end, time_mode),
            crate::protocol::emit_game_state("WaitingForFunctions"),
        ]
    }

    /// 回合结束：推进 `turn_count`，并在某队全灭时进入 `EndingGame`。
    ///
    /// 依据真实抓包：`{"TurnCountInfo":{"turn_count":4}}` 之后紧跟
    /// `{"GameStateInfo":{"game_state":"EndingGame"}}`，而那一回合我方最后一名士兵阵亡
    /// （`docs/45`）。
    pub fn end_turn(&mut self) -> Vec<String> {
        self.turn_count += 1;
        let mut out = vec![format!(
            r#"{{"TurnCountInfo":{{"turn_count":{}}}}}"#,
            self.turn_count
        )];
        // 还有存活士兵的队伍
        let alive_teams: std::collections::BTreeSet<u8> = self
            .soldiers
            .iter()
            .filter_map(|s| self.teams.get(&s.id).copied())
            .collect();
        let annihilated = self.all_teams.iter().any(|t| !alive_teams.contains(t));
        if annihilated {
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
    pub fn fire(
        &mut self,
        owner_id: i64,
        function_text: &str,
        x0: f64,
        y0: f64,
        flip: bool,
        now_ms: i64,
    ) -> Vec<String> {
        // 射手本人不参与致死判定
        let others: Vec<Target> = self
            .soldiers
            .iter()
            .filter(|t| {
                !(t.id == owner_id)
                    && !((t.x - x0).abs() < 1e-12 && (t.y - y0).abs() < 1e-12)
            })
            .copied()
            .collect();

        let outcome = resolve(function_text, x0, y0, 0.5, flip, crate::path::Mode::Normal, &self.mask, &others);
        let now_tick = self.tick_now(now_ms);

        // 报文顺序照真实抓包（docs/43）：
        //   CalculatingResults -> FunctionInfo(修正后) -> FunctionPoints
        //   -> SynchronizingResults -> [死者] -> [销毁] -> FunctionActive
        //   -> ExplosionEffect -> ExecutingResults
        let mut msgs = vec![
            crate::protocol::emit_game_state("CalculatingResults"),
            crate::protocol::emit_function_info(
                owner_id,
                function_text,
                Some(&crate::shot::corrected_text(function_text, x0, y0, flip)),
            ),
            emit_function_points(owner_id, x0, y0, outcome.stop.0),
            crate::protocol::emit_game_state("SynchronizingResults"),
        ];

        for id in &outcome.killed {
            msgs.push(emit_life_queued_death(*id, now_tick));
            if let Some(t) = self.soldiers.iter().find(|t| t.id == *id) {
                self.next_effect_id += 1;
                msgs.push(emit_effect_info(self.next_effect_id, t.x, t.y, "DeathEffect", now_tick));
            }
        }
        if !outcome.killed.is_empty() {
            // 记进 `life_manager` 那张表：轮转据此判活，死掉的返回 0（而不是 2「查不到」），
            // 两者都不可开火（`docs/98` §1.3），但"死过"这件事要留下来。
            for id in &outcome.killed {
                if let Ok(v) = u32::try_from(*id) {
                    self.life.set(v, Status::Dead, None);
                }
            }
            self.soldiers.retain(|t| !outcome.killed.contains(&t.id));
        }

        if outcome.blocked {
            msgs.push(format!(
                r#"{{"QueueDestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5,"tick":{}}}}}"#,
                outcome.stop.0, outcome.stop.1, now_tick
            ));
            // 真实游戏：在命中点用 fill=0xFF、半径 0.5 挖洞（docs/39）
            rasterize(&mut self.mask, outcome.stop.0, outcome.stop.1, 0.5, 0xFF);
            // 真正的删除要延后：实测 ≈165 tick 之后才发 DestroyObstacle
            self.pending_destroy.push((
                now_tick + DESTROY_DELAY_TICKS,
                format!(
                    r#"{{"DestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5}}}}"#,
                    outcome.stop.0, outcome.stop.1
                ),
            ));
        }

        msgs.push(crate::protocol::emit_function_active(owner_id, now_tick));
        self.next_effect_id += 1;
        msgs.push(emit_effect_info(self.next_effect_id, outcome.stop.0, outcome.stop.1, "ExplosionEffect", now_tick));
        msgs.push(crate::protocol::emit_game_state("ExecutingResults"));

        msgs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{get_int, parse_message};

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
        m.register_player(229, crate::turn::Team::One, &[230, 231]);
        m.register_player(230, crate::turn::Team::Two, &[232, 233]);
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
        m.register_player(229, crate::turn::Team::One, &[231]);
        m.register_player(230, crate::turn::Team::Two, &[232]);
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
            Some(crate::turn::SkipReason::TurnTimeout),
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
        assert_eq!(d.decision.skip_reason(), Some(crate::turn::SkipReason::UnknownPlayer));
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
        assert_eq!(AiConfig::default().budget_secs, Some(crate::aigen::GAME_TIME_BUDGET_SECS));
        assert_eq!(AiConfig::default().budget_secs, Some(25.0));
        let m = wired(AiConfig::default(), &[229]);
        assert_eq!(m.ai_config().budget_secs, Some(25.0), "Match 要把它交给 SoldierEvolver::with_budget");
        assert_eq!(wired(replay_ai(), &[]).ai_config().budget_secs, None, "测试口径可关掉");
    }

    /// 进化种群的归属（`docs/98` §6-A **未定**）在缓存条数上分叉 —— 一个显式开关。
    #[test]
    fn population_scope_is_explicit() {
        let mut per_soldier =
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
}

