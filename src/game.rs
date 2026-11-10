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
//! FunctionPoints -> [LifeQueuedDeath -> EffectInfo(登记表里查到的类型)]*
//!                -> [QueueDestroyObstacle] -> EffectInfo(ExplosionEffect)
//! ```
//!
//! 死亡那条的 `effect_type` **不是** `DeathEffect` 常量：它是这名实体出生时登记在
//! [`Match::entity_effects`] 里的那一项（`DeathEffect` / `SparkEffect` / `AshEffect`），
//! **查不到就整条不发**。登记点见 `graphwar_entity_effect_manager` §6。
//!
//! 与 `graphwar_shot_calculator::settle::events` 的区别：这里直接产出**协议报文文本**（带 tick），
//! 可以直接写进 socket。
//!
//! ## 接线（`docs/117-server-wiring.md`）
//!
//! 2026-11 之前，"谁开火"由**调用方**给 `soldier_id`，`graphwar_function_evolutionator` 的进化**没有调用点**，
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
//! 2026-11 之前，`graphwar_function_evolutionator` 的产物是 token，而 `token → 文本`（`0x4ec160`）没有调用点，
//! 于是 AI"会思考、不会说话"。现在 `math_function::to_text`（`docs/119`，1032/1032 对拍）
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
//! 由 `graphwar_shot_calculator::corrected_text` 复用 `correction_text` 产出（`docs/127` §5 第 1 条，已修）。
//!
//! ## 原版函数边界（`docs/14` §11）
//!
//! 本文件是原版 `graphwar_game_state_manager.rs` 的**运行实体那一半**：`impl Match`
//! 里每个方法组前面有一条 `// 原版边界：…` 横幅，写明对应的原版函数名 + x86 地址
//! （45 个可归属函数逐个落表）。同模块的状态枚举 / 超时判据 / 胜方字节在
//! `src/turnstate.rs`（那份文件的 §11 分工表见 `docs/14` §11.7）。
//!
//! **没接的**（都写在 `docs/117` §6 / `docs/127` §6）：回合超时的**检测**没有证据，
//! 本层只把 `timed_out` 当输入参数；游标初值取 `PlayerTurns::new` 的 0
//! （`TurnManager::add_player` 没有 index 入口，见 `docs/98` §6-B）；
//! 同时回合制没接（房间状态写死 `SequentialTurns`）。

use crate::graphwar_function_evolutionator::{self, Individual, SoldierEvolver, SoldierFunctions, SoldierTurn, VarMode};
use crate::graphwar_life_manager::{LifeState, Status};
use crate::graphwar_mode_manager::FunctionMode;
use crate::graphwar_path_calculator::Mode;
use crate::graphwar_entity_effect_manager::GraphwarEntityEffectManager;
use crate::graphwar_effect_manager::EffectType;
use crate::plane::{empty_mask, rasterize};
use crate::_scaffold::protocol::{emit_effect_info, emit_function_points, emit_life_queued_death};
use crate::_scaffold::rng::ChaCha12;
use crate::server::RoomState;
use crate::graphwar_shot_calculator::settle::{resolve_points_owned, ShotOutcome, Target};
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
    /// **本局的函数模式**：`NormalFunction`（`y = f(x)`）或 `DiffEqFunction`（`dy/dx = f(x,y)`）。
    ///
    /// 这是**开火路径**读的那个字节（`docs/06` §33，`观测` + 逐指令）：
    ///
    /// ```text
    /// 0x471791  call 0x46e140            ; 模块容器按 key 取"本局设置"那个模块
    /// 0x4717b8  movzbl 0x32(%rax),%r14d  ; ★ 模块 +0x32 = function_mode（2 态）
    /// 0x471806  mov %r14b,0x81(%rsp)     ; → 结算 config+0x39
    /// 0x471825  call *0x9bacb0           ; = 0x3c2bc0 calculate_shot_results
    /// 0x3c2d18  movzbl 0x39(%rax),%eax   ; 结算函数读回 config+0x39
    /// 0x3c3091  movzbl 0x198(%rsp),%ecx  ; ★ 作为采样器第 7 实参（%ecx）传下去
    /// 0x3c30c1  call *0x9bac98           ; = 0x3c2530 弦长采样器
    /// 0x3c25f5  test %r15b,%r15b / 0x3c25f8 je 0x3c2923   ; 0 ⇒ Normal 分支，非 0 ⇒ RK4 分支
    /// ```
    ///
    /// 写入者是 `FunctionModeInfo`(46) 的分支 `0x3e2b1b`（`mov BYTE PTR [r15+0x32],bpl`）——
    /// 也就是说**模式是"本局设置"里的一个字节，由 46 号事件落值，开火时现读**，
    /// 不是开火时的入参，也不是函数的性质（`docs/06` §33.2）。
    ///
    /// 极性：`0 = NormalFunction`、`非 0 = DiffEqFunction`（`docs/07` §5.2 的枚举判别值，
    /// 与图标索引 `fx_option`/`dx_dy_option` = 0/1 一致）。
    ///
    /// `RoomState::function_mode` 是同一个值的**线上视图**（`FunctionModeInfo` 的字符串），
    /// 两者由 [`Match::new`] 与 [`Match::set_function_mode`] 保持同步。
    function_mode: FunctionMode,
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
    /// **实体特效登记表**（原版 `GraphwarEntityEffectManager` 的那张 `HashMap<u32, u8>`）。
    ///
    /// 死亡结算**照抄**这张表：查到了就发那条 `EffectInfo(ty)`，查不到就**一条都不发**
    /// （arm64 `0x4a8c38` 分支）。所以 `AshEffect` 与 `DeathEffect` 的分界在**登记**时，
    /// 见 `graphwar_entity_effect_manager` §6 的 6 个写入点与
    /// [`Match::note_skin_graphics`]。
    pub entity_effects: GraphwarEntityEffectManager,
    /// **事件层**（原版 `GraphwarGameStateManager::new` 里那 7 条 `subscribe_to_event`
    /// 的落地：订阅名 + 发件箱 + 入件记录）。
    ///
    /// 实现全部在 [`crate::graphwar_game_state_manager`]（任务 95 从本文件搬出去的那一半：
    /// 订阅表 + 3 个处理器 + `take_outbox`）。容器里装的仍是 `Match` 本体，**没有加壳**。
    pub events: crate::graphwar_game_state_manager::GsmEvents,
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

/// **挖洞点 = 爆炸点**，不是弹道的采样停点（`docs/14-turn.md` §12）。
///
/// 观测（逐位，全语料）：回放里 **503 / 503** 条 `DestroyObstacle` 的 `(pos_x,pos_y)`
/// 与**同一批**的 `EffectInfo(ExplosionEffect)` 坐标**逐位相同**。也就是说原版挖洞用的
/// 是客户端画爆炸图案的那个点，而不是弹道算出来的停点。
///
/// 两者的关系：`x` 相同（都等于 `FunctionPoints.end_x`），`y` 不同 ——
/// 停点是**原始文本**采样出来的 y，而爆炸点是拿**修正文本**（常数只印 3 位小数，
/// `corrected_text`）在 `x = end_x` 处**重算**的 y，最多差 ~5e-4
/// （`docs/06-verification.md` §24 的 `Y2` 类就是这个差）。
/// 用停点当挖洞点会让 200 例"我方挖洞点在回放里不存在"
/// （`dispatch/14-hole-axis`：x 逐位相同、只差 y）。
///
/// ⚠️ `execute_results` 的 ④（`EffectInfo(ExplosionEffect)`）**当前仍用 `out.stop`**，
/// 那是 `Y` 类（爆炸点 y 末位差，170 批）那根轴，不在本次改动范围内；两处若都用
/// 本函数，我方报文才与原版"挖洞点 ≡ 爆炸点"的自洽关系一致。
///
/// `diff_eq = true`（`DiffEqFunction` 局）时**不重算**、直接返回停点：
/// 那种局 `corrected_function == function`（不加后缀），爆炸点就是停点本身
/// （`_scaffold::diffconv::explosion_point` 是同一条规则；§24.7 那 291 个可比批里
/// **92 批是 DiffEq**，其中 26 批落在 225 个不命中里）。
pub(crate) fn hole_point(
    diff_eq: bool,
    function_text: &str,
    x0: f64,
    y0: f64,
    flip: bool,
    stop: (f64, f64),
) -> (f64, f64) {
    if diff_eq {
        return stop;
    }
    let toks = crate::math_function::parse(function_text);
    if toks.is_empty() {
        return stop;
    }
    // 原版发出去的修正文本 = `to_text(parse(function))` + 带符号常量的 3 位小数后缀。
    // `to_text` 原样保留输入文本，唯一的改动是**补齐没闭合的括号**（11 条，实测），
    // 所以这里先补括号再接后缀（与 `_scaffold::diffconv::corrected_function` 同一条规则）。
    let mut printed = String::from(function_text);
    let mut depth = 0i64;
    for ch in function_text.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            _ => {}
        }
    }
    for _ in 0..depth {
        printed.push(')');
    }
    printed.push_str(&crate::graphwar_shot_calculator::correction_text(
        &toks, x0, y0, flip,
    ));
    let printed_toks = crate::math_function::parse(&printed);
    if printed_toks.is_empty() {
        return stop;
    }
    // 客户端拿**印出来的**文本重算爆炸点：`x = ±end_x`，
    // 修正文本写在射手自己的坐标系里，`flip` 的射手要把 x 取反再代。
    let x = if flip { -stop.0 } else { stop.0 };
    let y = crate::math_function::eval(&printed_toks, x, 0.0, 0.0);
    if y.is_finite() {
        (stop.0, y)
    } else {
        stop
    }
}

// ---------------------------------------------------------------------------
// 修正文本：补齐括号 / 去掉多余右括号（`docs/06` §29）
// ---------------------------------------------------------------------------

/// 括号平衡：返回 `(真值发出的串, 我方采样器能吃的串)`。
///
/// **为什么要有两个串**（`docs/06` §29，4 批种类序列由对变错的根因）：
///
/// * 真值的修正文本 = `原文 + ")"×缺 + " ± c.ddd"`（回放 `corrected_function` 逐字节核对，
///   `docs/142` §2.2 的 739/739）—— 只**补**括号，从不删。所以**发出去的**必须是它。
/// * 但我方 `math_function::parse` 遇到**没有配对**的 `)` 时会**就地截断**（`expr()` 直接返
///   回已建好的子树，后面的记号全丢）。真解析器不截断：`1782675382#0` 的原文
///   `14/(1+exp(100*(x-19)^2)))+24/(...)` 用"去掉多余 `)` + 补 `")`"的文本采样，端点
///   `(-24.99612783676475, -4.323)` 与回放 `EffectInfo(Explosion)` **逐位相同**。
///   而若照原文采样，`+24/(...)` **和追加的修正常数**一起被丢掉 —— 曲线不经过 `(x0,y0)`
///   （`|y0 − f(x0)| ≈ 4.3 ≫ step 0.01`），采样器第一步就收敛不了，弹道退化成 1 个点。
///
/// 所以：**采样**用"补齐 + 去掉多余 `)`"的串，**报文**用"只补齐"的串。
fn balanced_texts(text: &str) -> (String, String) {
    let mut depth = 0i64;
    let mut stripped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '(' => {
                depth += 1;
                stripped.push(ch);
            }
            ')' => {
                if depth > 0 {
                    depth -= 1;
                    stripped.push(ch);
                }
                // depth == 0 ⇒ 这条 `)` 没有配对：真解析器当噪声跳过去，我方 parse 会截断。
            }
            _ => stripped.push(ch),
        }
    }
    let mut closed = String::from(text);
    let mut samplable = stripped;
    for _ in 0..depth {
        closed.push(')');
        samplable.push(')');
    }
    (closed, samplable)
}

/// 真值**发出去**的 `corrected_function`：`原文 + ")"×缺 + " ± c.ddd"`。
///
/// `c` 由**补完括号的树**算（真解析器看的就是整条表达式，不会被多余的 `)` 截断）。
///
/// ⚠️ **别把 `x0` 挪 1 ulp 来"修" `C` 桶**（试过，已否）。`docs/06` §42：
/// 真值那个 `calculate_function_offset_str`（x86-64 `0x3c5f70`）就是
/// `y0 − f(flip ? −x0 : x0)`，把它在 unicorn 里按**报文里的 `start_x`** 调一遍，
/// 输出与我们这里**逐字节相同**（含 `C` 桶那 14 批）。所以少的东西不在这一步，
/// 而在**调用方传进来的那个 Point**：那 14 批的 `c` 只有把 point.x 挪 1 ulp
/// 向零才复现得出来，可是另有 7 批（`c` 巨大、对 x 同样敏感 1 ulp 的）证明
/// 那些批的 point.x **就是** `start_x`。⇒ 两条 x 生产路径之差按发而异，
/// 不能写成 `f(x0)` 这样的规则；硬写会在 12 批上赚、在 7 批上赔（实测 647→652，
/// `C` 14→7 但多出 7 个新不符）。缺的证据：喂给 `0x3c5f70` 的那个 Point 的生产者
/// （该函数的调用点是**指针派发**，`.text` 里没有 `call rel32` 指向它）。
pub(crate) fn corrected_function_text(
    function_text: &str,
    x0: f64,
    y0: f64,
    flip: bool,
) -> String {
    let (closed, samplable) = balanced_texts(function_text);
    let toks = crate::math_function::parse(&samplable);
    format!(
        "{closed}{}",
        crate::graphwar_shot_calculator::correction_text(&toks, x0, y0, flip)
    )
}

/// **采样器**吃的修正文本：与 [`corrected_function_text`] 同一条规则，但把多余的 `)`
/// 去掉（理由见 [`balanced_texts`] 的文档）。
pub(crate) fn sampling_function_text(
    function_text: &str,
    x0: f64,
    y0: f64,
    flip: bool,
) -> String {
    let (_, samplable) = balanced_texts(function_text);
    let toks = crate::math_function::parse(&samplable);
    format!(
        "{samplable}{}",
        crate::graphwar_shot_calculator::correction_text(&toks, x0, y0, flip)
    )
}

// ---------------------------------------------------------------------------
// 函数模式：`FunctionModeInfo`(46) → 采样模式（`docs/06` §33）
// ---------------------------------------------------------------------------

/// `FunctionModeInfo.function_mode` 的线上字符串 → [`FunctionMode`]。
///
/// 取值集合已穷举（`docs/07` §5.2：`NormalFunction` / `DiffEqFunction`，两个变体），
/// 所以"认不出来的字符串"**不该出现**；这里按 `docs/136` §3 的老口径兜底成
/// `NormalFunction`，并把这一点写在这里而不是散在调用方。
pub fn function_mode_of(wire: &str) -> FunctionMode {
    if wire == "DiffEqFunction" {
        FunctionMode::DiffEqFunction
    } else {
        FunctionMode::NormalFunction
    }
}

/// [`FunctionMode`] → 采样器的 [`Mode`]（两个枚举是**同一件事的两层**：
/// 前者是线上/设置里的 2 态字节，后者是采样器的算法分支）。
pub fn sampling_mode(function_mode: FunctionMode) -> Mode {
    match function_mode {
        FunctionMode::NormalFunction => Mode::Normal,
        FunctionMode::DiffEqFunction => Mode::Ode,
    }
}

/// **模式感知**的 `corrected_function`（真值**发出去**的那一串）。
///
/// * `Mode::Normal`：就是 [`corrected_function_text`]（补括号 + `" ± c.ddd"`）；
/// * `Mode::Ode`（微分方程局）：**原文照发**，不加修正常数、**连补括号都不做**。
///
///   两条证据（都是观测）：
///   1. 回放实测 **550 / 550 条** `corrected_function == function`（`docs/142` §3）；
///   2. 语料里有 **2 批** DiffEq 函数**括号不平衡**（`1782760485` #0 缺 16 个 `)`、
///      `1783013297` #0 缺 3 个），回放的 `corrected_function` 与原文**逐字节相同**、
///      **没有**补任何 `)`。⇒ 微分方程局里"修正文本"这一步整个被跳过（不重新
///      `to_text`），所以连补括号这个副产物也不出现。
///      （先按"补括号与模式无关"实现时，正是这 2 批报 `C` 类不一致，改回原文即命中。）
pub(crate) fn corrected_function_text_mode(
    mode: Mode,
    function_text: &str,
    x0: f64,
    y0: f64,
    flip: bool,
) -> String {
    match mode {
        Mode::Normal => corrected_function_text(function_text, x0, y0, flip),
        Mode::Ode => function_text.to_string(),
    }
}

/// **模式感知**的采样串（见 [`sampling_function_text`] 与
/// [`corrected_function_text_mode`] 的两条规则）。
///
/// `Ode` 局采样**原文**（与发出去的那串是同一个）：修正文本这一步不执行，
/// 采样器直接吃玩家写的函数原文（`dy/dx = f(x, y)` 的右侧）。
pub(crate) fn sampling_function_text_mode(
    mode: Mode,
    function_text: &str,
    x0: f64,
    y0: f64,
    flip: bool,
) -> String {
    match mode {
        Mode::Normal => sampling_function_text(function_text, x0, y0, flip),
        Mode::Ode => function_text.to_string(),
    }
}

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
// 批内结算顺序：把它当成一个**假设**，做成可切换策略（`dispatch/65`，`docs/06` §39）
// ---------------------------------------------------------------------------
//
// 真值里同一批的几何**不是**一次算完的（`docs/06` §34）：
//
// * 同批各发的爆炸时刻互不相同 —— `EffectInfo.start_tick` 在 `1794158991` 批#1 里是
//   `258`=25205 < `249`=25245 < `255`=25309（批起点 25056）；
// * **更早爆炸那几发挖的洞，对后面解算的几发可见**：只把 `249`/`258` 的洞放进掩膜，
//   owner 255 的停点就**逐位等于**回放（§34.4 的单变量实验）；
// * 可见性阶梯（§34.5，89 发）：整批共用批开始掩膜 60/63 → 按**报文序**给洞（`prefix`）
//   61/63 → 按**爆炸先后**给洞（`arrival`）62/63。⇒ **报文顺序 ≠ 结算顺序**。
//
// ⚠ 但"结算顺序 = 爆炸时刻"这条**没有被直证**：§34.5 读法 ③ 里 `same` 与 `arrival`
// 在这 89 发上打平（缺一条"晚爆炸的洞不挡早爆炸的发"的样本）。所以这里**不写死**
// 任何一种顺序，而是做成策略：一次扫出 `策略 × 轴` 的矩阵（`dispatch/65` §8），
// 再按结果定默认值 [`DEFAULT_SETTLE_ORDER`]。
//
// 原版**没有**这个开关：`GW2_SETTLE_ORDER` 只是对拍/扫描用的（不设 = 默认档），
// 与 `examples/batch_cross_probe.rs` 的 `--mask-mode` 同一个性质。

/// 批内结算顺序策略（`dispatch/65`；矩阵与判决见 `docs/06` §39）。
///
/// 每一档 = **批内解算顺序** × **洞对后续几发的可见性**：
///
/// | 档 | 批内顺序 | 洞可见 | 出处 |
/// |---|---|---|---|
/// | [`SettleOrder::S1Message`] | 报文序（`FunctionPoints` 序） | 否（整批共用批开始掩膜） | 旧行为（基线） |
/// | [`SettleOrder::S2ArrivalFlat`] | 到达序（到停点弧长升序） | 否 | **控制项**：只换序、不挖洞 |
/// | [`SettleOrder::S3ArrivalHoles`] | 到达序（到停点弧长升序） | **是** | `dispatch/55` 提的那条改法 |
/// | [`SettleOrder::S4TickHoles`] | 到达 tick 序（整数 tick；同 tick 按报文序） | 是 | 55 号的"按 `start_tick` 序" |
/// | [`SettleOrder::S5OwnerHoles`] | owner id 升序 | 是 | 反例（把顺序换成别的全序） |
/// | [`SettleOrder::S6MessageHoles`] | 报文序 | 是 | 55 号的 `prefix` 档（**控制项**） |
///
/// 为什么"到达序"= **到停点的弧长升序**：同一批的弹丸 `active_tick` 相同、速度相同
/// （`docs/141` §2），所以"谁先到"⇔"谁的弧长更短"（[`crate::graphwar_shot_calculator::settle::Hit::arc`]
/// 用的就是这条）。**不需要**先知道速度本身 —— 只有 S4 的取整分桶用到
/// [`PROJECTILE_UNITS_PER_TICK`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleOrder {
    /// S1 报文序 + 批开始掩膜（旧行为）。
    S1Message,
    /// S2 到达序 + 批开始掩膜（只换序，不挖洞）。
    S2ArrivalFlat,
    /// S3 **到达序 + 逐发挖洞**（`dispatch/55` 的方案）。
    S3ArrivalHoles,
    /// S4 到达 tick 序 + 逐发挖洞。
    S4TickHoles,
    /// S5 owner id 序 + 逐发挖洞。
    S5OwnerHoles,
    /// S6 报文序 + 逐发挖洞（55 号的 `prefix`）。
    S6MessageHoles,
}

/// 弹丸速度（棋盘单位 / tick）。
///
/// **推断**：`graphwar_shot_calculator::settle` 的"≈0.29 单位/tick"（与 `docs/141` §2
/// "同一回合所有弹丸 `active_tick` 相同"配套）。**没有**反汇编或报文侧的直证。
/// 它只影响 S4 的**取整分桶**（同 tick 内怎么排），不影响 S1–S3/S5/S6。
pub const PROJECTILE_UNITS_PER_TICK: f64 = 0.29;

/// 默认策略 = **扫完矩阵后选出来的那一档**（`docs/06` §39 的判决）。
///
/// 改这个常量要连 `docs/06` §39 的矩阵一起改 —— 它是"我们相信哪一种顺序"的唯一落点。
pub const DEFAULT_SETTLE_ORDER: SettleOrder = SettleOrder::S3ArrivalHoles;

impl SettleOrder {
    /// 六档全表（扫描器与文档都按这个顺序报）。
    pub const ALL: [SettleOrder; 6] = [
        SettleOrder::S1Message,
        SettleOrder::S2ArrivalFlat,
        SettleOrder::S3ArrivalHoles,
        SettleOrder::S4TickHoles,
        SettleOrder::S5OwnerHoles,
        SettleOrder::S6MessageHoles,
    ];

    /// 环境变量/报告里用的短名（`s1`…`s6`）。
    pub fn name(self) -> &'static str {
        match self {
            SettleOrder::S1Message => "s1",
            SettleOrder::S2ArrivalFlat => "s2",
            SettleOrder::S3ArrivalHoles => "s3",
            SettleOrder::S4TickHoles => "s4",
            SettleOrder::S5OwnerHoles => "s5",
            SettleOrder::S6MessageHoles => "s6",
        }
    }

    /// 短名 → 档（不认识的返回 `None`，由调用方决定怎么报）。
    pub fn parse(s: &str) -> Option<SettleOrder> {
        Self::ALL.into_iter().find(|o| o.name() == s)
    }

    /// 这一档是否**逐发挖洞**（解完一发就把它的洞写进本批的工作掩膜）。
    ///
    /// S1/S2 不挖：洞仍由 [`Match::execute_results`] ③ 在整批结算完之后落地（旧行为）。
    pub fn digs_holes(self) -> bool {
        matches!(
            self,
            SettleOrder::S3ArrivalHoles
                | SettleOrder::S4TickHoles
                | SettleOrder::S5OwnerHoles
                | SettleOrder::S6MessageHoles
        )
    }

    /// 批内解算顺序（`shots` 下标的一个排列）。见 [`Match::fire_functions`] 的 ③ 段。
    fn permutation(
        self,
        shots: &[Submission<'_>],
        points: &[Vec<(f64, f64)>],
        outcomes: &[ShotOutcome],
    ) -> Vec<usize> {
        let mut perm: Vec<usize> = (0..shots.len()).collect();
        match self {
            // 报文序 = 调用方给的顺序，原样不动
            SettleOrder::S1Message | SettleOrder::S6MessageHoles => {}
            SettleOrder::S2ArrivalFlat | SettleOrder::S3ArrivalHoles => {
                let arcs: Vec<f64> = (0..shots.len())
                    .map(|i| arc_to_stop(&points[i], outcomes[i].stop))
                    .collect();
                perm.sort_by(|&a, &b| {
                    arcs[a]
                        .partial_cmp(&arcs[b])
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.cmp(&b))
                });
            }
            SettleOrder::S4TickHoles => {
                let ticks: Vec<i64> = (0..shots.len())
                    .map(|i| {
                        (arc_to_stop(&points[i], outcomes[i].stop) / PROJECTILE_UNITS_PER_TICK)
                            .round() as i64
                    })
                    .collect();
                perm.sort_by(|&a, &b| ticks[a].cmp(&ticks[b]).then(a.cmp(&b)));
            }
            SettleOrder::S5OwnerHoles => {
                perm.sort_by(|&a, &b| shots[a].owner_id.cmp(&shots[b].owner_id).then(a.cmp(&b)));
            }
        }
        perm
    }
}

/// 一发从起点量到 `stop` 的**弧长**（弹丸走过的距离）—— 也就是"到达时刻"的排序键。
///
/// `stop` 一定落在点列里（[`resolve_points_owned`] 只从点列里取值），所以按位相等即可定位；
/// 万一不在（点列为空这种边界），退化成整条点列的弧长：**只影响排序键，不影响几何**。
fn arc_to_stop(points: &[(f64, f64)], stop: (f64, f64)) -> f64 {
    let mut arc = 0.0f64;
    for w in 1..points.len() {
        let (px, py) = points[w];
        let (qx, qy) = points[w - 1];
        arc += ((px - qx).powi(2) + (py - qy).powi(2)).sqrt();
        if px.to_bits() == stop.0.to_bits() && py.to_bits() == stop.1.to_bits() {
            return arc;
        }
    }
    arc
}

/// **S3 的第二遍（到达序 + 逐发挖洞）—— 内核与收敛层共用的唯一实现。**
///
/// 输入是"第一遍用**批开始掩膜**解出来的结果"：
///
/// * `mask`：批开始那一刻的掩膜（只读；工作副本在函数里拷）；
/// * `shots` / `points` / `outcomes`：**报文序**的提交、点列、第一遍结果；
/// * `outcomes` 被就地替换成第二遍的结果（只有真的走到第二遍时才会改）。
///
/// 做的事就是 `dispatch/55` 提、`dispatch/65` 落地的那一步（`docs/06` §39）：按
/// [`SettleOrder::permutation`] 的批内顺序**逐发重解**，每解完一发就把它挖的洞写进
/// 本批的工作掩膜 ⇒ **后解的那几发看得见前面的洞**。报文侧的对应语义是 §34.5 的三档
/// （整批共用掩膜 60/63、报文序给洞 `prefix` 61/63、到达序给洞 `arrival` 62/63）。
///
/// ## 为什么单独立成函数（2026-11-09，`docs/06` §42）
///
/// 收敛层 `_scaffold::diffconv::ConvPredictor::fire_batch` 是同一套结算流程的**第二份
/// 驱动**（`batch_conv` 轴），它**不经过 `Match`**（§39.2 读法 3：`GW2_SETTLE_ORDER`
/// 六档在那条轴上曾经逐字节相同，就是因为这段代码只存在于本文件里）。65 号把顺序语义
/// 落进内核时没动它 ⇒ 两层口径分叉（§39.7 第 4 条）。现在两边调**这同一个**函数：
/// 同构是**构造出来的**，不是两份代码各写一遍"看起来一样"。
///
/// 两边只要有一处不同就是 bug，而且 `tests/diffconv_parity.rs` / `examples/diffconv_parity`
/// 会把它变成一条**逐位**的红 —— 那条对拍比的是 `points` / `stop` / `blocked` /
/// `killed` / `hits` / 修正文本，两个驱动器各算一遍。
///
/// ## 两条口径（别改错）
///
/// 1. **工作掩膜必须是 `mask` 的拷贝**，不能直接写调用方的 `self.mask`：
///    `plane::rasterize` 的抗锯齿环是**与旧值混合**（`blend_ring`），同一个洞画两遍与
///    画一遍**不等**；而 [`Match::execute_results`] ③（收敛层 ⑤）之后还会再画一遍。
///    直接写 `self.mask` 就会改动后续批的掩膜，把"同批可见性"这一个变量和"跨批状态"
///    混在一起。拷一份 = 只动本批。
/// 2. **单发批（`shots.len() < 2`）与 S1/S2 档一律不改任何东西**：任何顺序都退化成
///    报文序，且没有洞要提前挖 ⇒ 与旧行为**逐位相同**（`Match::fire` 因此不可能被
///    这一改动影响；`ConvPredictor::fire` = `N = 1` 的 `fire_batch`，同理）。
pub(crate) fn settle_in_order_with_holes(
    strategy: SettleOrder,
    mode: Mode,
    mask: &[u8],
    soldiers: &[Target],
    shots: &[Submission<'_>],
    points: &[Vec<(f64, f64)>],
    outcomes: &mut [ShotOutcome],
) {
    if !strategy.digs_holes() || shots.len() < 2 {
        return;
    }
    let perm = strategy.permutation(shots, points, outcomes);
    let mut work = mask.to_vec();
    for &i in &perm {
        let s = &shots[i];
        let o = resolve_points_owned(&work, soldiers, &points[i], Some(s.owner_id));
        if o.blocked {
            // 位置 / 半径 / 填充值与 [`Match::execute_results`] ③ **同一条规则**
            // （挖洞点 = 爆炸点，[`hole_point`]）：洞心由**重解后**的停点算 ——
            // 这正是"前面挖的洞改变了后面那一发的停点，于是后面那一发挖在别处"。
            let (hx, hy) = hole_point(
                mode == Mode::Ode,
                s.function_text,
                s.x0,
                s.y0,
                s.flip,
                o.stop,
            );
            rasterize(&mut work, hx, hy, 0.5, 0xFF);
        }
        outcomes[i] = o;
    }
}

/// 当前策略：`GW2_SETTLE_ORDER=s1…s6` 覆盖 [`DEFAULT_SETTLE_ORDER`]。
///
/// 值不认识时**退回默认档并警告一次**（不是静默忽略）：这个开关是给扫描器用的，
/// 拼错一个字母就会让整张矩阵看起来"改了什么都没生效"——"改了但没生效"是这个项目
/// 反复踩过的坑（`AGENTS.md`），这里用一条 `eprintln!` 把它变成可见的。
pub fn settle_order() -> SettleOrder {
    match std::env::var("GW2_SETTLE_ORDER") {
        Ok(v) => match SettleOrder::parse(v.trim()) {
            Some(o) => o,
            None => {
                static WARNED: std::sync::Once = std::sync::Once::new();
                WARNED.call_once(|| {
                    eprintln!(
                        "GW2_SETTLE_ORDER={v} 不认识（只认 s1…s6）⇒ 用默认档 {}",
                        DEFAULT_SETTLE_ORDER.name()
                    );
                });
                DEFAULT_SETTLE_ORDER
            }
        },
        Err(_) => DEFAULT_SETTLE_ORDER,
    }
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

/// 结束判定的结果。
///
/// `winner` 存的是**实体队伍号**（`set_team` 记进去的那个 u8），不是真值
/// `[self+0x11f]` 里的那个**序号字节**。两者只在"队伍号恰好是 0/1"时才同值：
/// 真值那个字节由 `set_state` 的 `EndingGame` 分支算出（0=`Team1`、1=`Team2`、
/// 2=没人），公式已落成 [`crate::turnstate::winner_byte`]（`docs/14` §11.4）。
/// `None` 对应真值的 `2`（两边都没活实体）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ending {
    pub reason: EndingReason,
    pub winner: Option<u8>,
}

impl Match {
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`GraphwarGameStateManager::new`（`0x477bc0`，992 字节）
    // 我方：`Match::new`。真值在这个函数里还做了 7 次
    // `GraphwarEventHandler::subscribe_to_event`（事件名逐条在 `.rodata`：
    // `new_connection_event` / `entity_removed_event` / `tick_event` /
    // `connection_removed_event` / `function_points_event` /
    // `skin_graphics_info_event` / `chat_message_event`），也就是 arm64 上那 7 个
    // `subscribe_to_events_event_handler::{{closure}}`）；我们**没有事件总线**，
    // 这 7 条订阅现在落在 [`crate::graphwar_game_state_manager::SUBSCRIBED_EVENTS`]
    // 与 [`Match::events`]（任务 95 搬过去的那一半），`docs/14` §11.7 的分工表据此更新。
    // ─────────────────────────────────────────────────────────────────────
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
        // 入参 `soldiers` 就是**已经出生的士兵**（原版由 `add_soldier_to_player` 逐个建），
        // 所以这里补上那次出生登记：`Match::new` 手里还没有任何 `SkinGraphicsInfo`，
        // 走的是"皮肤为空"那条分支 ⇒ 全部登记 `DeathEffect`（§6.1 写点 1）。
        // 迟到的 `bot_` 皮肤会把它们改写成 `AshEffect`，见 [`Match::note_skin_graphics`]。
        let mut entity_effects = GraphwarEntityEffectManager::new();
        for s in &soldiers {
            if let Ok(id) = u32::try_from(s.id) {
                entity_effects.register_soldier(id, None);
            }
        }
        // 函数模式从房间设置取（`RoomState::function_mode` 就是 `FunctionModeInfo` 的字符串），
        // 与二进制"开火时读设置模块那个字节"同义：初始化读一次，之后由 46 号事件改。
        let function_mode = function_mode_of(&room.function_mode);
        Match {
            room, obstacles, soldiers, tick_base, mask,
            next_effect_id: 236, teams, all_teams, turn_count: 0,
            targets: std::collections::BTreeMap::new(),
            turn_limit: None,
            turn_start_tick: 0, time_mode: "Timer1m".to_string(),
            function_mode,
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
            entity_effects,
            // 原版 `new` 在这里做 7 次 `subscribe_to_event`（`0x477c92` … `0x477e96`）；
            // 我方把订阅表落进事件层，见 `crate::graphwar_game_state_manager`。
            events: crate::graphwar_game_state_manager::GsmEvents::new(),
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

    /// 本局的函数模式保存值（[`Match::function_mode`] 那一段的证据）。
    pub fn function_mode(&self) -> FunctionMode {
        self.function_mode
    }

    /// **开火路径实际用的**采样模式：`Mode::Normal`（`y = f(x)`）或 `Mode::Ode`（RK4）。
    pub fn sampling_mode(&self) -> Mode {
        sampling_mode(self.function_mode)
    }

    /// `FunctionModeInfo`(46) 的落地点：写模式字节（二进制 `0x3e2b1b` 的
    /// `mov BYTE PTR [r15+0x32],bpl`）。
    ///
    /// 同时同步 `room.function_mode`（线上视图），两者因此不会分叉：
    /// `room.function_mode` 是**报文侧**的字符串，`function_mode` 是**开火侧**的字节。
    ///
    /// 接线层（回放对拍器 / 服务器）收到一条 `FunctionModeInfo` 就该调它一次；
    /// 二进制里这个写手是**事件处理器**，不是开火函数——所以模式在一局里可以中途改，
    /// 每次开火读的是**当时**的值。
    pub fn set_function_mode(&mut self, function_mode: FunctionMode) {
        self.function_mode = function_mode;
        // 线上字符串取**逐位**那张名字表（`MODE_NAME_TABLE[0..2]` =
        // `NormalFunction` / `DiffEqFunction`，`.data.rel.ro` 逐条解出），不在这里抄一份。
        //
        // ⚠ 别换成 `FunctionMode::name()`：crate 里有**两个** `FunctionMode`——
        // 本字段用的是 `graphwar_mode_manager` 的那个（"本局设置"，46 号事件的落值对象）；
        // `name()`/`icon_path()` 挂在 `graphwar_level_manager::FunctionMode`（关卡数据）上。
        self.room.function_mode =
            crate::graphwar_setup_control::MODE_NAME_TABLE[function_mode as usize].to_string();
    }

    /// 同上，吃线上字符串（`FunctionModeInfo.function_mode`）——给"重放输入流"的接线层用。
    pub fn set_function_mode_wire(&mut self, function_mode: &str) {
        self.set_function_mode(function_mode_of(function_mode));
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
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`request_new_player`（`0x4739c0`）/ `add_soldier_to_player`（`0x474a10`）
    // 我方：`Match::register_player` —— 它是这两个原版函数的**接线形态**：
    // 原版 `request_new_player` 调 `TeamManager::add_to_team` +
    // `ColorManager::add_team_color`（`0x3c17d0` / `0x464d10`，行号锚 282–292）；
    // 原版 `add_soldier_to_player` 再调 `TeamManager::get_team` 与三个图形设置
    // （`set_hat/face/skin_graphics`，行号锚 503–538）。我们只做队伍 + 函数槽 +
    // 生命表这三件事，图形侧在 `graphwar_graphics_manager`（`docs/14` §11.7）。
    // ─────────────────────────────────────────────────────────────────────
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
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`queue_end_turn_trigger`（`0x4759f0`）→ `TurnManager::next_turn`（`0x463da0`）
    // 我方：`Match::begin_next_turn` / `begin_simultaneous_turn`。
    // 原版推进轮转的地方就是 `queue_end_turn_trigger` 里那一次
    // `TurnManager::next_turn`（`0x4759f0` 的直接调用表），随后
    // `set_state(WaitingForFunctions)` 把 `TurnInfo`/`EndOfTurnInfo`/`GameStateInfo`
    // 发出去。我们的三态判决 + AI 进化是**接线层**，原版没有对应函数
    // （`docs/117` §4）。
    // ─────────────────────────────────────────────────────────────────────
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
                msgs.push(crate::_scaffold::protocol::emit_turn_info(p, s));
            }
        }
        // `EndOfTurnInfo` 与 `GameStateInfo` 各**一条**。
        //
        // ⚠️ `end_of_turn_tick` 在真回放里恒为 **0**（`1793884702` 三回合全是
        // `{"end_of_turn_tick":0,"time_mode":"TimerInf"}`），而 `graphwar_tick_handler::end_of_turn_tick_wire`
        // 给的是 `start + 时长`。这两个数在 `docs/41` 就没对上过，是**已知分歧**；
        // 本函数沿用逐玩家路径的发法（不在这里另造一个数）。
        let end = crate::graphwar_tick_handler::end_of_turn_tick_wire(start_tick, time_mode);
        msgs.push(crate::_scaffold::protocol::emit_end_of_turn(end, time_mode));
        msgs.push(crate::_scaffold::protocol::emit_game_state("WaitingForFunctions"));
        (msgs, starts)
    }

    /// 该士兵函数槽里的**文本**：`math_function::to_text`（`0x4ec160`）在本仓库的唯一调用点。    ///
    /// 为什么需要它：`graphwar_function_evolutionator` 的产物是 token（[`FunctionSlot::tokens`](crate::graphwar_function_evolutionator::FunctionSlot)），
    /// 协议 `FunctionInfo.function` 要的是文本，中间只有这一步（`docs/119`、`docs/127`）。
    /// `None` = 这个士兵没有槽（没登记过）。
    ///
    /// ⚠️ 它**不是单射**（`docs/119` §5.3）：π 写成 `3.14`、负常量写成 `neg` 的文本形态、
    /// 缺操作数的空位在文本里不存在。这三条都是真二进制行为，不在这里修。
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`handle_function_points`（`0x4758b0`）/ `request_update_function`（`0x475f50`）
    // 我方：`Match::ai_function_text` / `ai_submit_info` / `evolve_for_soldier`。
    // 原版 `request_update_function` 把提交的原文写进函数表
    // （`TurnManager::get_current_turn_soldiers` + `FunctionManager::set_function`，
    // 行号锚 635–642）；`handle_function_points` 只做转发（无调用，锚 1129/1135）。
    // 我们**不存**提交（收集阶段在 `graphwar_game_server`），这里只有 AI 侧。
    // ─────────────────────────────────────────────────────────────────────
    pub fn ai_function_text(&self, soldier_id: i64) -> Option<String> {
        self.functions.tokens(soldier_id).map(crate::math_function::to_text)
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
        Some(crate::_scaffold::protocol::emit_function_info(soldier_id, &text, None))
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
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`graphwar_tick_handler` 的换算（不是本模块的函数）
    // `tick_now` 属于 tick_handler（`docs/35`）；留在 `Match` 上是因为原版
    // `handle_tick`/`fire_functions` 两处都直接读同一个 tick。
    // ─────────────────────────────────────────────────────────────────────
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
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`add_target`（`0x46f310`，896 字节）
    // 我方：`Match::add_target` / `kill_target` / `restore_target` / `set_all_alive`。
    // 真值 `add_target` 的直接调用：`TeamManager::add_to_team`（写队伍）、
    // `GraphicsManager::set_skin_graphics`（写外观）、再 4 次模块存取器；行号锚
    // 593:109 … 603:31。**队伍是入参**（`docs/150`：127/127 个靶子都是
    // `TeamInfo{Team2}`，不是写死 Team2）。
    // ─────────────────────────────────────────────────────────────────────
    pub fn add_target(&mut self, id: i64, team: u8) {
        self.targets.insert(id, true);
        self.set_team(id, team);
        // 出生登记：靶子恒 `SparkEffect`（§6.2，`0x4a58d0` 的 `mov w2, #0x2`）。
        if let Ok(eid) = u32::try_from(id) {
            self.entity_effects.register_target(eid);
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`handle_graphics_info`（`0x4aa304`，784 字节；订阅表里的处理器入口
    // `0x4741a0`）里那次 `add_entity_effect(_, AshEffect)` 循环（`0x4aa410`–`0x4aa47c`）。
    // **已搬走**（任务 95）：`Match::note_skin_graphics` 现在在
    // [`crate::graphwar_game_state_manager`]，连它体内那 1 处 `add_event`
    // （tag 56 `EntityEffectInfo`，`0x4742b9`）的地址一起搬过去。本文件不再留副本。
    // ─────────────────────────────────────────────────────────────────────

    /// 实体特效登记表的投影（测试与对拍器用）：`None` = 这个实体**没有登记**，
    /// 死亡时原版一条特效都不发。
    pub fn entity_effect_of(&self, id: i64) -> Option<EffectType> {
        u32::try_from(id).ok().and_then(|e| self.entity_effects.get(e))
    }

    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`entity_removed_event` 的处理器（判定值 14；处理器入口 `0x476a20`）。
    // **已搬走**（任务 95）：`Match::handle_entity_removed` 现在在
    // [`crate::graphwar_game_state_manager`]（含它体内那 1 处 `add_event` = tag 44
    // `EffectInfo`，`0x476bbe`）。本文件不再留副本。
    // ─────────────────────────────────────────────────────────────────────

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

    /// 原版 `GraphwarGameStateManager::set_all_alive`（**`0x470b70`**，832 字节）。
    ///
    /// 真值在这一个函数里调了 **两次** `LifeManager::set_alive`（`0x454b40` ×2），
    /// 行号锚 607:105 … 616:26 全在同一个函数体内：两趟循环把两类实体各刷一遍
    /// （士兵一趟、靶子一趟；`docs/14` §11.5）。它不发任何报文，只翻存活位。
    ///
    /// 本实现的两趟：
    ///
    /// * 靶子：把 [`Match::targets`] 全部翻回 `true`；
    /// * 生命表：把 `Match::life` 里每一个**记得的** id 设回 `Status::Alive`。
    ///
    /// **已知差异**（不是行为改动，是模型边界）：我们被打死的**士兵**会被
    /// [`Match::execute_results`] 从 `self.soldiers` 摘掉（站位丢了），所以这里能复活
    /// 的只有靶子和生命表里的状态位，复活不了士兵实体。真值里实体表是独立的
    /// `soldier_manager`，`set_alive(true)` 不重建实体，所以那边也一样只翻存活位。
    pub fn set_all_alive(&mut self) {
        for alive in self.targets.values_mut() {
            *alive = true;
        }
        // `LifeState.alive` 是公开表（`docs/112` §2.1 的形状），这里只读键、再翻状态位。
        let ids: Vec<u32> = self.life.alive.keys().copied().collect();
        for id in ids {
            self.life.set(id, Status::Alive, None);
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

    /// 原版 `GraphwarGameStateManager::get_alive_teams`（**`0x473520`**，640 字节）。
    ///
    /// 返回值就是 `set_state(EndingGame)` 用的那两个位（`0x478133` 的 `test al,1` /
    /// `test dl,1`）：`(Team1 有活体, Team2 有活体)`。真值的实现是
    ///
    /// ```text
    /// LifeManager::get_all_entities   (0x453c60)  枚举**生命表**里的全部实体
    ///   → LifeManager::get_alive      (0x454950)  逐个问存活
    ///     → TeamManager::get_team     (0x3c2120)  取队伍，置对应位
    /// ```
    ///
    /// 也就是说它数的是**生命表里的活实体**（士兵 **和** 靶子 —— 靶子有 `LifeInfo`，
    /// 玩家实体没有），与 [`Match::team_has_survivor`] 同一条判据（`docs/150` §Q2）。
    ///
    /// 队伍编号用 `graphwar_turn_manager::Team::id()`（`1`/`2`）——真值那两位也是
    /// `Team1`/`Team2`，不是任意 u8。
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`get_alive_teams`（`0x473520`，640 字节）
    // 我方：`Match::get_alive_teams`（逐位同构）/ `team_has_survivor`（判据本身）。
    // 真值枚举**生命表**取活实体，再按队伍置两个位；行号锚 867:105 … 876:50。
    // ─────────────────────────────────────────────────────────────────────
    pub fn get_alive_teams(&self) -> (bool, bool) {
        let t1 = self.team_has_survivor(graphwar_turn_manager::Team::One.id());
        let t2 = self.team_has_survivor(graphwar_turn_manager::Team::Two.id());
        (t1, t2)
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
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`set_turn_limit`（`0x472400`，112 字节）/ `set_turn_count`（x86 内联）
    // 真值 `set_turn_limit` 只有 4 条赋值 + 一次 `add_event`：
    // `mov [rdi+0xc8], esi` / `mov [rdi+0xcc], edx`（`0x472405`/`0x47240b`），
    // 事件 tag `0x39`（`0x472433`）。⇒ 它是 **(标志, 上限)** 两个 u32 入参的 setter，
    // 报文里只出现 `turn_limit` 一个字段（`docs/123` §5.1）。
    // ─────────────────────────────────────────────────────────────────────
    pub fn set_turn_limit(&mut self, limit: i64) -> Vec<String> {
        self.turn_limit = Some(limit);
        vec![format!(r#"{{"TurnLimitInfo":{{"turn_limit":{limit}}}}}"#)]
    }

    /// 原版 `GraphwarGameStateManager::set_turn_count`（x86 **无独立体**，见下）。
    ///
    /// 证据链（`docs/14` §11.6）：
    ///
    /// * arm64 2.0.60 有**独立**符号 `GraphwarGameStateManager::set_turn_count`
    ///   （`0x4a8460`，`0x7c` 字节），紧邻 `set_turn_limit`（`0x4a84dc`，同样 `0x7c`），
    ///   两个函数体形状一致：写字段 + `GraphwarEventQueue::add_event` 一次；
    /// * x86 上它**被内联掉了**：它的 panic 锚在 0x3e2830 / 0x46f690 / 0x4759f0 三处
    ///   （`fixtures/name_to_addr.tsv` 的"歧义"行给的候选正好是这三个），
    ///   没有剩下任何一处独立函数体 —— 所以表里它只能是"未知地址"。
    ///
    /// 语义按 arm64 的体 + `queue_end_turn_trigger` 的报文（`docs/45`）落地：
    /// 写 `turn_count`，再发一条 `TurnCountInfo`。上限/结束判定**不**在这里
    /// （那在 [`Match::ending`] 与 [`Match::end_turn`]）。
    pub fn set_turn_count(&mut self, count: i64) -> Vec<String> {
        self.turn_count = count;
        vec![format!(
            r#"{{"TurnCountInfo":{{"turn_count":{}}}}}"#,
            self.turn_count
        )]
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
    /// 胜方（`[self+0x11f]` 那个字节的真实算法在 `set_state` `0x478133`–`0x478236`，
    /// 逐行落成 [`crate::turnstate::winner_byte`]）：`0`=Team1、`1`=Team2、
    /// `2`=没人（两边都没活实体）。
    ///
    /// ⚠️ 本方法返回的 `winner` 是**实体队伍号**（`Some(team)`），不等于那个序号字节；
    /// 换算与"可达输入上等价"的论证见 `docs/14` §11.4 与 [`Ending`] 的文档。
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`set_state`（`0x4780c0`）的结束分支 + `queue_end_turn_trigger`（`0x4759f0`）
    // 我方：`Match::ending`。真值在 `set_state(EndingGame)` 里算胜方字节
    // （`0x478133`–`0x478236`，见 `turnstate::winner_byte` 的表），
    // 两个结束入口（回合上限 / 某队全灭）在 `queue_end_turn_trigger` 里。
    // ─────────────────────────────────────────────────────────────────────
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
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`start_game`（`0x46f690`）与 `queue_end_turn_trigger` 共用的报文组
    // 我方：`Match::begin_turn`（`TurnInfo` → `EndOfTurnInfo` → `GameStateInfo`）。
    // 真值 `start_game` 的直接调用里有 `TurnManager::next_turn` 与
    // `FunctionManager::set_function`，行号锚 191–255 与 572–625；
    // 报文顺序本身来自抓包（`docs/43`），不是从反汇编读出来的。
    // ─────────────────────────────────────────────────────────────────────
    pub fn begin_turn(&mut self, player_id: i64, soldier_id: i64, now_ms: i64, time_mode: &str) -> Vec<String> {
        let start_tick = self.tick_now(now_ms);
        self.turn_start_tick = start_tick;
        self.time_mode = time_mode.to_string();
        let end = crate::graphwar_tick_handler::end_of_turn_tick_wire(start_tick, time_mode);
        vec![
            crate::_scaffold::protocol::emit_turn_info(player_id, soldier_id),
            crate::_scaffold::protocol::emit_end_of_turn(end, time_mode),
            crate::_scaffold::protocol::emit_game_state("WaitingForFunctions"),
        ]
    }

    /// 回合结束：推进 `turn_count`，结束条件见 [`Match::ending`]。
    ///
    /// 依据真实抓包：`{"TurnCountInfo":{"turn_count":4}}` 之后紧跟
    /// `{"GameStateInfo":{"game_state":"EndingGame"}}`（`docs/45`）；回合上限也走同一条出口
    /// （`0x4759f0` 里 ①②③ 全都跳到 `0x475bac`）。
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`queue_end_turn_trigger`（`0x4759f0`，1376 字节）
    // 我方：`Match::end_turn`。真值这个函数：
    // `get_alive_teams` ×2 / `LifeManager::get_alive` / `set_alive` /
    // `TurnManager::next_turn` / `set_state`（直接调用表），行号锚 839–906。
    // ─────────────────────────────────────────────────────────────────────
    pub fn end_turn(&mut self) -> Vec<String> {
        self.turn_count += 1;
        let mut out = vec![format!(
            r#"{{"TurnCountInfo":{{"turn_count":{}}}}}"#,
            self.turn_count
        )];
        if self.ending().is_some() {
            out.push(crate::_scaffold::protocol::emit_game_state("EndingGame"));
        }
        out
    }

    /// 推进到 `now_tick`，返回所有到期的 `DestroyObstacle`。
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`handle_tick`（`0x470840`，816 字节）
    // 我方：`Match::tick`（到期的 `DestroyObstacle`）+ `Match::handle_tick`（原版名）。
    // 真值的三段职责见 `Match::handle_tick` 的文档与 `docs/14` §11.3。
    // ─────────────────────────────────────────────────────────────────────
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

    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`handle_tick`（`0x470840`，816 字节）。
    // **已搬走**（任务 95）：`Match::handle_tick` 现在在
    // [`crate::graphwar_game_state_manager`]（三段职责逐段标注、含体内那 1 处
    // `add_event` = tag 30 `GameStateInfo{1}`，`0x470a49`）。
    // 留在本文件的是它用到的机制 [`Match::tick`]（到期 `DestroyObstacle`），
    // 因为那个队列（`pending_destroy`）是结算侧的字段。
    // ─────────────────────────────────────────────────────────────────────

    /// 处理一次开火，返回要广播的消息（顺序见模块注释）。
    ///
    /// `owner_id` 是开火实体；`function_text` 是**未修正**的函数原文
    /// （修正由 `resolve` 内部完成）。
    ///
    /// 这是 [`Match::fire_batch`] 的 `N = 1` 特例：单发批的报文种类序列与
    /// 逐发实现**逐字节相同**，所以 `SequentialTurns` 的行为没有被这次重构改动。
    // ─────────────────────────────────────────────────────────────────────
    // 原版边界：`fire_functions`（`0x470eb0`）+ `execute_results`（`0x472470`）
    // 我方：`Match::fire` → `fire_batch` → {`fire_functions`, `execute_results`}。
    // 边界就是这么切的：前一半算弹道（原版 5456 字节），后一半广播 + 落地
    // （原版 4272 字节，四个调用点与我们的四类报文一一对应）。
    // ─────────────────────────────────────────────────────────────────────
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
        // 原版这里也是两段：`fire_functions`（0x470eb0）算完每一发的弹道，
        // 再调 `execute_results`（0x472470）广播。我们的切法与之对齐（docs/14 §11）。
        let outcomes = self.fire_functions(shots);
        self.execute_results(shots, &outcomes, now_tick)
    }

    /// 原版 `GraphwarGameStateManager::fire_functions`（**`0x470eb0`**，5456 字节）。
    ///
    /// 真值在这一段里做的事（`analysis` 的直接调用表，`docs/14` §11）：
    ///
    /// ```text
    /// TurnManager::get_current_turn_soldiers   取这一回合该开火的人
    /// PlanePosManager::get_position            取每人的起点
    /// TeamManager::get_team                    取朝向（`Team::flip`）
    /// FunctionManager::get_function_text       取提交的函数原文
    /// （算弹道）
    /// FunctionManager::set_function_points     写回终点
    /// → GraphwarGameStateManager::execute_results
    /// ```
    ///
    /// 它的三个输入在真值里是从**函数表**现取的；我们的 `Submission` 由调用方给
    /// （收集阶段的产物）。这条差别就是 `request_update_function` 尚未落地的部分
    /// （`docs/14` §11 的"未实现"行）。
    ///
    /// 本方法**只算几何、不改任何状态**：真值在这一段里写的是 `FunctionPoints`
    /// （`set_function_points`），而我们的报文在 [`Match::execute_results`] 里统一发。
    pub fn fire_functions(&self, shots: &[Submission<'_>]) -> Vec<ShotOutcome> {
        self.fire_functions_with(shots, settle_order())
    }

    /// [`Match::fire_functions`] 的**显式档位**版本。
    ///
    /// 单独留一个入口的理由只有一条：`settle_order()` 读的是环境变量，而 `cargo test`
    /// 是多线程的 —— 测试里改 `GW2_SETTLE_ORDER` 会与别的测试抢同一个进程。所以她
    /// 走参数、产品路径走环境变量（默认档 [`DEFAULT_SETTLE_ORDER`]）。
    pub fn fire_functions_with(
        &self,
        shots: &[Submission<'_>],
        strategy: SettleOrder,
    ) -> Vec<ShotOutcome> {
        // ① 几何并行：先把每一发的弹道都算完，用的是**批开始时**的士兵表与掩膜。
        //    谁被打死**不影响**别人的弹道（士兵本就不挡弹道，`docs/30`）。
        //
        //    ⚠ 这只对**士兵表**成立，对**掩膜**不成立：同批更早爆炸的那几发会挖洞
        //    （`docs/06` §34.4），所以"整批共用一张掩膜"是一个**假设**，不是事实。
        //    它对应策略 [`SettleOrder::S1Message`]（旧行为）；其余档在 ③④ 段重算。
        //
        // **模式（`docs/06` §33）**：真值在**开火时**读"本局设置"那个字节
        // （`0x4717b8` → config+0x39），结算函数把它原样转给采样器（`0x3c3091` → `%ecx`），
        // 采样器 `test %r15b,%r15b` 分两支（`0x3c25f5` / `je 0x3c2923`）。
        // 所以这里不是"新增一个入参"，而是**补上原来漏读的那个字节**：
        // 缺了它，`DiffEqFunction` 局也在按 `y = f(x)` 采（515 批 / 550 发不可比）。
        let mode = self.sampling_mode();
        // ② 点列 + 第一遍解算。
        //
        //    点列**与掩膜无关**（`shot_points` 不吃掩膜：弹道只由函数与起点决定，
        //    见 `docs/30` 的"士兵不挡弹道、障碍才挡"）⇒ 第二遍按别的顺序重解时
        //    不必重采，只换"用哪张掩膜"。
        //
        //    第一遍的结果就是 S1 的答案，同时给出到达序的排序键（到停点的弧长）。
        let mut points: Vec<Vec<(f64, f64)>> = Vec::with_capacity(shots.len());
        let mut outcomes = Vec::with_capacity(shots.len());
        for s in shots {
            // 射手本人不参与致死判定 —— **这条规则现在在内核里**（`settle::resolve_owned`），
            // 调用方不再预先剔除。理由（`docs/122` §4 / §6）：二进制在 `0x3c4452`
            // （有弹道点）与 `0x3c3e81`（空弹道）内部就排除射手，而且**只比 id**。
            //
            // 2026-11-07 两处修正：
            // ① 删掉调用方多余的 `1e-12` 位置排除（二进制没有这条）；
            // ② 把"按 id 排除"下沉进内核，避免规则散落在调用方。
            let targets: Vec<Target> = self.soldiers.clone();
            // 步长用**实况 SERVER_STEP（0.01）**，不是 0.5。
            // `docs/136` 的几何探针给出：同一批回放，step=0.01 时 `end_x` 逐位相同 605/739；
            // step=0.5 只有 27/739。
            //
            // **采样器吃的是「修正后函数文本」而不是「原文 + 全精度修正常数」**（2026-11 修，
            // `docs/06` §25）。依据（arm64 2.0.60 逐指令）：
            //   * `calculate_corrected_function`（`0x3b79a8`）= 原文 + 补齐 `)` + 偏移串；
            //   * `calculate_function_offset_str`（`0x3b7c70`）= `c = y0 - f(∓x0)`，按 **`{:.3}`**
            //     印成 `" - 6.861"`（`0x3b7d2c` 起：`fcmp`/`fabs` → `format` → 补 `" + "`/`" - "`）；
            //   * 采样器 `calculate_function`（`0x3b5374`）拿到的树就是这串文本解析出来的，
            //     所以轨迹里的常量是**印出来的三位小数**，不是全精度 `c`。
            // 旧实现走 `resolve_owned(原文, …)`，内部按 `shifted_tokens`（全精度 `c`）平移，
            // 与真值差 3 位小数的舍入量（1e-6~1e-3）—— 正是归因表 `Y2` 那一桶。
            //
            // **括号处理**（2026-11 第二次修，`docs/06` §29）：原文的括号既可能少
            // （要补 `)`）也可能多（多出来的 `)` 会让我方 `parse` 截断、把追加的常数一起丢掉）。
            // 采样串与报文串因此不是同一个串，见 [`balanced_texts`]。
            let corrected = sampling_function_text_mode(mode, s.function_text, s.x0, s.y0, s.flip);
            let pts = crate::graphwar_shot_calculator::shot_points(
                &corrected,
                s.x0,
                s.y0,
                crate::SERVER_STEP,
                s.flip,
                mode,
                false,
            );
            outcomes.push(resolve_points_owned(
                &self.mask,
                &targets,
                &pts,
                Some(s.owner_id),
            ));
            points.push(pts);
        }

        // ③④ 到达序 + 逐发挖洞（S3 的第二遍）。
        //
        // 实现**只有一份** —— [`settle_in_order_with_holes`]，收敛层
        // （`_scaffold::diffconv::ConvPredictor::fire_batch`）调的是同一个函数。
        // 理由：65 号把这段写在内核里时收敛层没跟，两层口径分叉过一个版本
        // （`docs/06` §39.7 第 4 条 → §42 修掉）；同构必须由**同一个函数**保证，
        // 不能靠两份代码"看起来一样"。
        settle_in_order_with_holes(
            strategy,
            mode,
            &self.mask,
            &self.soldiers,
            shots,
            &points,
            &mut outcomes,
        );

        outcomes
    }

    /// 原版 `GraphwarGameStateManager::execute_results`（**`0x472470`**，4272 字节）。
    ///
    /// 真值在这一段里直接调了四个东西（`analysis` 的直接调用表，`docs/14` §11）：
    ///
    /// ```text
    /// FunctionManager::set_function_active_tick     ← 我们发 FunctionActive
    /// LifeManager::set_queued_death                 ← 我们发 LifeQueuedDeath
    /// ObstacleManager::queue_destroy_obstacle       ← 我们发 QueueDestroyObstacle
    /// PlanePosManager::get_position                 ← 我们发 EffectInfo 的坐标
    /// EffectManager::add_effect  ×2                 ← 我们发两条 EffectInfo
    /// ```
    ///
    /// 两处 `add_effect`（arm64 `0x4a89ec` / `0x4a8cb0`，都**内联**）的实参不同：
    /// 前者恒 `ExplosionEffect` + 自增 id（弹道停点），后者
    /// **`effect_type = entity_effect_map[entity_id]`**，`effect_id` 仍来自同一个自增
    /// 计数器，**查不到就一条都不发**（`0x4a8c38`）。见
    /// `graphwar_entity_effect_manager` §5–§6。
    ///
    /// 四个都对得上，所以 `0x472470` 的行为边界就是本方法：**一次齐射的广播 + 落地**。
    /// 报文顺序与分组见 [`Match::fire_batch`] 的文档（`docs/141` §2–§4）。
    pub fn execute_results(
        &mut self,
        shots: &[Submission<'_>],
        outcomes: &[ShotOutcome],
        now_tick: i64,
    ) -> Vec<String> {
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
        // 模式在**本方法里也要读一次**：修正常数只在 `NormalFunction` 局加
        // （`DiffEq` 局 `corrected_function == function`，550/550，`docs/142` §3），
        // 挖洞点的重算同理（见 [`hole_point`] 的 `diff_eq` 参数）。
        let mode = self.sampling_mode();
        // ① 按类型分组的前缀
        let mut msgs = vec![crate::_scaffold::protocol::emit_game_state("CalculatingResults")];
        for s in shots {
            // 发出去的是**只补括号、不删**的那一串（回放 `corrected_function` 逐字节同形）；
            // 采样用的另一串见 [`balanced_texts`]。`DiffEq` 局两个串都不带修正常数。
            msgs.push(crate::_scaffold::protocol::emit_function_info(
                s.owner_id,
                s.function_text,
                Some(&corrected_function_text_mode(
                    mode,
                    s.function_text,
                    s.x0,
                    s.y0,
                    s.flip,
                )),
            ));
        }
        for (s, out) in shots.iter().zip(outcomes.iter()) {
            msgs.push(emit_function_points(s.owner_id, s.x0, s.y0, out.stop.0));
        }
        msgs.push(crate::_scaffold::protocol::emit_game_state("SynchronizingResults"));

        // ② 死亡块：顺序 = 到达时刻（`docs/86` §6）
        //
        // 特效类型**照抄登记表**（arm64 `0x4a8cb0` 一带）：
        //   `let ty = entity_effect_map.get(&entity_id)?;`  ← `0x4a8b8c`–`0x4a8c50`
        //   `effect_manager.add_effect(next_effect_id, pos, ty, start_tick)`
        // **查不到 ⇒ 连 `EffectInfo` 都不发**（`0x4a8c38` 直接跳过 `add_event`）；
        // 旧实现写死 `"DeathEffect"`，那正是 `docs/141` 那 19 例继续错的原因。
        // 注意 `next_effect_id` 只在**真的发**的时候自增（原版是 `add_effect` 里自增的）。
        for id in &deaths {
            msgs.push(emit_life_queued_death(*id, now_tick));
            let ty = u32::try_from(*id)
                .ok()
                .and_then(|eid| self.entity_effects.death_effect_of(eid));
            let Some(ty) = ty else { continue };
            if let Some(t) = self.soldiers.iter().find(|t| t.id == *id) {
                self.next_effect_id += 1;
                msgs.push(emit_effect_info(self.next_effect_id, t.x, t.y, ty.name(), now_tick));
            }
        }

        // ③ 挖洞块（回放不记录 `QueueDestroyObstacle`，位置**无证据**；沿用单发实现的
        //    "死亡之后、`FunctionActive` 之前"，这样 N=1 时与原实现逐字节相同）
        //
        //    位置的**证据**在 `DestroyObstacle` 那侧：挖洞点 = 爆炸点，不是采样停点
        //    （[`hole_point`]，全语料 503/503 逐位）。
        for (s, out) in shots.iter().zip(outcomes.iter()) {
            if !out.blocked {
                continue;
            }
            let (hx, hy) = hole_point(
                mode == Mode::Ode,
                s.function_text,
                s.x0,
                s.y0,
                s.flip,
                out.stop,
            );
            msgs.push(format!(
                r#"{{"QueueDestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5,"tick":{}}}}}"#,
                hx, hy, now_tick
            ));
            // 真实游戏：在命中点用 fill=0xFF、半径 0.5 挖洞（docs/39）
            rasterize(&mut self.mask, hx, hy, 0.5, 0xFF);
            // 真正的删除要延后：实测 ≈165 tick 之后才发 DestroyObstacle
            self.pending_destroy.push((
                now_tick + DESTROY_DELAY_TICKS,
                format!(
                    r#"{{"DestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5}}}}"#,
                    hx, hy
                ),
            ));
        }

        // ④ 每发一对 `FunctionActive` + `ExplosionEffect`，顺序与①相同（130/130 批实测）
        for (s, out) in shots.iter().zip(outcomes.iter()) {
            msgs.push(crate::_scaffold::protocol::emit_function_active(s.owner_id, now_tick));
            self.next_effect_id += 1;
            msgs.push(emit_effect_info(
                self.next_effect_id,
                out.stop.0,
                out.stop.1,
                "ExplosionEffect",
                now_tick,
            ));
        }
        msgs.push(crate::_scaffold::protocol::emit_game_state("ExecutingResults"));

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
    use crate::_scaffold::protocol::{get_f64, get_int, get_str, parse_message};

    /// 真值的括号规则（回放 1291 发里的 8 多 / 11 缺，`docs/06` §29.4）：
    /// * **发出去**的串只补括号、从不删（多出来的 `)` 原样保留）；
    /// * **采样**用的串把没有配对的 `)` 去掉 —— 否则我方 `parse` 在那里截断，
    ///   追加的修正常数被整段丢掉，弹道退化成 1 个点。
    #[test]
    fn bracket_balancing_splits_emitted_and_sampled_text() {
        // 缺 `)`：两个串都是"补到平衡"（真值 11/11 这么做）
        let (closed, samplable) = balanced_texts("5/155sin(4exp(4*6cos(");
        assert_eq!(closed, "5/155sin(4exp(4*6cos()))");
        assert_eq!(samplable, closed);

        // 多 `)`：发出去的串**逐字符不变**（回放 `corrected_function` 就是这样），
        // 采样串去掉那一个多余的 `)`。样本原文来自 `1782675382#0`。
        let (closed, samplable) =
            balanced_texts("14/(1+exp(100*(x-19)^2)))+24/(1+exp(100*(x-24)^2))");
        assert_eq!(closed, "14/(1+exp(100*(x-19)^2)))+24/(1+exp(100*(x-24)^2))");
        assert_eq!(samplable, "14/(1+exp(100*(x-19)^2))+24/(1+exp(100*(x-24)^2))");

        // 采样串真的能走过第一步：`(x0, y0)` 被修正常数接上了。
        let fn_text = "14/(1+exp(100*(x-19)^2)))+24/(1+exp(100*(x-24)^2))";
        let (x0, y0, flip) = (22.908325288230134_f64, -4.32330978206142_f64, true);
        let sampled = sampling_function_text(fn_text, x0, y0, flip);
        let pts = crate::graphwar_shot_calculator::shot_points(
            &sampled,
            x0,
            y0,
            crate::SERVER_STEP,
            flip,
            crate::graphwar_path_calculator::Mode::Normal,
            false,
        );
        assert!(pts.len() > 100, "弹道不该退化成 {} 个点", pts.len());
        // 端点与回放 `EffectInfo(ExplosionEffect)` 逐位相同（`docs/06` §29.4）
        let end = *pts.last().unwrap();
        assert_eq!(end.0.to_bits(), (-24.99612783676475_f64).to_bits());
        assert_eq!(end.1.to_bits(), (-4.323_f64).to_bits());
    }

    // ─────────────────────────────────────────────────────────────────────
    // 函数模式（`docs/06` §33）：模式来自 `FunctionModeInfo`(46)，开火路径现读
    // ─────────────────────────────────────────────────────────────────────

    fn diffeq_room() -> RoomState {
        RoomState { function_mode: "DiffEqFunction".into(), ..room() }
    }

    /// 模式的两个来源必须同义：`Match::new` 读房间字符串；`FunctionModeInfo` 事件走 setter。
    /// 极性 `0 = NormalFunction / 1 = DiffEqFunction`（`docs/07` §5.2 的枚举判别值）。
    #[test]
    fn function_mode_comes_from_the_room_and_from_the_info_event() {
        let normal = Match::new(room(), vec![], vec![], (0, 0));
        assert_eq!(normal.function_mode(), FunctionMode::NormalFunction);
        assert_eq!(normal.sampling_mode(), Mode::Normal);
        assert_eq!(normal.room.function_mode, "NormalFunction");

        let mut m = Match::new(diffeq_room(), vec![], vec![], (0, 0));
        assert_eq!(m.function_mode(), FunctionMode::DiffEqFunction);
        assert_eq!(m.sampling_mode(), Mode::Ode);

        // `FunctionModeInfo` 的落地：46 号事件写的是设置里的那个字节，
        // 开着局也能改（二进制写手 `0x3e2b1b`，读者是**每次开火** `0x4717b8`）。
        m.set_function_mode_wire("NormalFunction");
        assert_eq!(m.function_mode(), FunctionMode::NormalFunction);
        assert_eq!(m.sampling_mode(), Mode::Normal);
        assert_eq!(m.room.function_mode, "NormalFunction", "线上视图必须同步");
        m.set_function_mode(FunctionMode::DiffEqFunction);
        assert_eq!(m.room.function_mode, "DiffEqFunction");

        // 线上字符串 ↔ 枚举：判别值就是 `MODE_NAME_TABLE` 的下标（逐位）
        assert_eq!(function_mode_of("NormalFunction"), FunctionMode::NormalFunction);
        assert_eq!(function_mode_of("DiffEqFunction"), FunctionMode::DiffEqFunction);
        assert_eq!(crate::graphwar_setup_control::MODE_NAME_TABLE[0], "NormalFunction");
        assert_eq!(crate::graphwar_setup_control::MODE_NAME_TABLE[1], "DiffEqFunction");
    }

    /// 同一发函数在两种模式下的**点序列不同**，且微分方程局与采样器直调逐位相同。
    ///
    /// 这一条是"内核真的换了采样器"的证据：改之前 `fire_functions` 写死 `Mode::Normal`，
    /// 两种局算出来的 `end_x` 一定一样（那是 515 批不可比的成因）。
    #[test]
    fn diffeq_room_samples_the_same_function_with_rk4() {
        // `y` 作被积函数：Normal 下 `y = f(x)` 是一条水平线（`y` 是变量 = 0? 不，解析器把
        // 未知变量当 0），Ode 下 `dy/dx = y` 从 y0 起指数增长 —— 两者几何完全不同。
        let text = "y";
        let (x0, y0, flip) = (0.0_f64, 1.0_f64, false);

        let normal = Match::new(room(), vec![], vec![], (0, 0));
        let ode = Match::new(diffeq_room(), vec![], vec![], (0, 0));
        let shot = Submission { owner_id: 1, function_text: text, x0, y0, flip };

        let pn = crate::graphwar_shot_calculator::shot_points(
            &sampling_function_text_mode(Mode::Normal, text, x0, y0, flip),
            x0, y0, crate::SERVER_STEP, flip, Mode::Normal, false,
        );
        let po = crate::graphwar_shot_calculator::shot_points(
            &sampling_function_text_mode(Mode::Ode, text, x0, y0, flip),
            x0, y0, crate::SERVER_STEP, flip, Mode::Ode, false,
        );
        assert_ne!(pn.len(), po.len(), "两种采样器的点数不该一样：{} vs {}", pn.len(), po.len());

        // 内核路径必须与"直接拿 Mode::Ode 调采样器"逐位相同（不许再写死 Normal）
        let on = normal.fire_functions(std::slice::from_ref(&shot))[0].stop;
        let oo = ode.fire_functions(std::slice::from_ref(&shot))[0].stop;
        assert_eq!(on.0.to_bits(), pn.last().unwrap().0.to_bits());
        assert_eq!(on.1.to_bits(), pn.last().unwrap().1.to_bits());
        assert_eq!(oo.0.to_bits(), po.last().unwrap().0.to_bits());
        assert_eq!(oo.1.to_bits(), po.last().unwrap().1.to_bits());
        assert_ne!(on.0.to_bits(), oo.0.to_bits(), "两条弹道的终点不该相同");
    }

    /// 微分方程局：`corrected_function` **原文照发**（不加 `± c.ddd`，也不补括号）。
    ///
    /// 证据（观测）：全语料 550 发 DiffEq 的 `corrected_function == function` 零例外；
    /// 其中 2 发括号不平衡（`1782760485` / `1783013297`），回放**没有**补 `)` ⇒
    /// 修正文本那一步整个被跳过（连补括号这个副产物也不出现）。
    #[test]
    fn diffeq_room_emits_the_function_text_verbatim() {
        let mut m = Match::new(diffeq_room(), vec![], vec![], (0, 0));
        // 括号不平衡：15 个 `(`、0 个 `)`（回放 `1782760485` 那条的形状）
        let text = "222cos(555(9sqrt(5πe3545.745yexp(cos(tan(sqrt(ln(abs(sin(cos(tan(sqrt(526852984sqrt(sqrt(ln(sin(cos(2588";
        let msgs = m.fire(1, text, 0.0, 1.0, false, 0);
        let fi = msgs.iter().find(|s| s.starts_with(r#"{"FunctionInfo""#)).expect("应有 FunctionInfo");
        let parsed = parse_message(fi).unwrap();
        assert_eq!(get_str(&parsed, "corrected_function").as_deref(), Some(text), "DiffEq 必须原文照发");
        assert_eq!(get_str(&parsed, "function").as_deref(), Some(text));

        // Normal 局同一发会带修正常数（对照，证明这条测试不是空转）
        let mut mn = Match::new(room(), vec![], vec![], (0, 0));
        let msgs = mn.fire(1, text, 0.0, 1.0, false, 0);
        let fi = msgs.iter().find(|s| s.starts_with(r#"{"FunctionInfo""#)).unwrap();
        let corrected = get_str(&parse_message(fi).unwrap(), "corrected_function").unwrap();
        assert_ne!(corrected, text, "Normal 局必须加修正常数");
        assert!(corrected.len() > text.len());
    }

    /// 微分方程局挖洞点 = 停点（不重算）。Normal 局才会用印刷版修正文本回代。
    #[test]
    fn diffeq_room_hole_point_is_the_stop() {
        let stop = (3.0, 4.0);
        let text = "sin(x)";
        assert_eq!(hole_point(true, text, 1.0, 0.0, false, stop), stop);
        // Normal：`end_x` 相同、`y` 用修正文本重算（与 `diffconv::explosion_point` 同源）
        let (hx, _hy) = hole_point(false, text, 1.0, 0.0, false, stop);
        assert_eq!(hx.to_bits(), stop.0.to_bits());
    }

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
        use crate::_scaffold::protocol::parse_message;
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

    /// 从一条 `FunctionPoints` 报文里取 `end_x`。
    ///
    /// ⚠️ 不要用 `strip_prefix` 卡死浮点写法：线上格式是 serde_json/ryu
    /// （`"start_x":10.0`），而本仓库曾经用 `format!("{}")` 打成 `10`。
    /// 这里按**字段名**取，两种写法都吃得下（`docs/06` §30）。
    fn end_x_of(msg: &str) -> Option<f64> {
        const KEY: &str = "\"end_x\":";
        let at = msg.find(KEY)? + KEY.len();
        msg[at..]
            .split([',', '}'])
            .next()?
            .trim()
            .parse::<f64>()
            .ok()
    }

    #[test]
    fn blocked_shot_punches_a_hole_in_the_mask() {
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (0, 0));
        let first = m.fire(1, "0.00", 10.0, 0.0, true, 0);
        let stop_x = first
            .iter()
            .filter(|s| s.starts_with(r#"{"FunctionPoints":{"owner_id":1,"#))
            .find_map(|s| end_x_of(s))
            .expect("应能解析终点");
        // 同一发再打一次：洞口已经挖开，应飞得更远
        let second = m.fire(1, "0.00", 10.0, 0.0, true, 1000);
        let stop2 = second
            .iter()
            .filter(|s| s.starts_with(r#"{"FunctionPoints":{"owner_id":1,"#))
            .find_map(|s| end_x_of(s))
            .unwrap();
        assert!(stop2 < stop_x, "挖洞后应飞得更远（{stop2} 应小于 {stop_x}）");
    }

    /// **挖洞点 = 爆炸点**，不是采样停点（`docs/14-turn.md` §12）。
    ///
    /// 真值来自回放 1782640836 批#1：`FunctionPoints.end_x = 21.50120415902081`，
    /// 该批的 `DestroyObstacle` / `ExplosionEffect` 都是 `(21.50120415902081, 0.0)`；
    /// 我方采样停点是 `(21.50120415902081, -7.964500098012195e-05)`（差在 y）。
    #[test]
    fn hole_point_is_the_explosion_point_not_the_sampled_stop() {
        let text = "(-9.311)*(1/((1+exp(-42*(x-(-16))))*(1+exp(42*(x-(25))))))\
                    +(4+2.2sin(6x))*(1/((1+exp(-42*(x-(5))))*(1+exp(42*(x-(10))))))";
        let stop = (21.50120415902081, -7.964500098012195e-05);
        let p = hole_point(false, text, -17.890807559723125, 9.31092035499902, false, stop);
        assert_eq!(p.0, stop.0, "x 不变（= end_x）");
        assert_eq!(p.1, 0.0, "y 是修正文本（+ 9.311）在 end_x 处重算的值");
        // DiffEq 局不重算：`corrected_function == function`，爆炸点就是停点
        assert_eq!(hole_point(true, text, 0.0, 0.0, false, stop), stop);
    }

    // -----------------------------------------------------------------------
    // 批内结算顺序（`dispatch/65`，`docs/06` §39；收敛层跟进见 §42）
    // -----------------------------------------------------------------------

    /// 六档的短名/解析必须闭合，默认档必须是**扫出来的那一档**。
    ///
    /// 这条不是形式主义：`GW2_SETTLE_ORDER` 拼错会**静默退回默认档**，而"改了但没生效"
    /// 是本项目反复踩的坑。名字与解析不闭合，扫描器就会把 6 格扫成 1 格。
    #[test]
    fn settle_order_switch_is_closed_and_defaults_to_s3() {
        assert_eq!(SettleOrder::ALL.len(), 6);
        for o in SettleOrder::ALL {
            assert_eq!(SettleOrder::parse(o.name()), Some(o), "{} 的名字解析不回来", o.name());
        }
        assert_eq!(SettleOrder::parse("s9"), None);
        assert_eq!(SettleOrder::parse("S3"), None, "短名大小写敏感（与扫描器一致）");
        // 挖洞档：S3/S4/S5/S6；S1/S2 不挖（洞由 `execute_results` ③ 事后落地）
        for o in SettleOrder::ALL {
            let want = !matches!(o, SettleOrder::S1Message | SettleOrder::S2ArrivalFlat);
            assert_eq!(o.digs_holes(), want, "{} 的挖洞档位", o.name());
        }
        // 落地档 = 到达序 + 挖洞（`docs/06` §39.4 的判决）
        assert_eq!(DEFAULT_SETTLE_ORDER, SettleOrder::S3ArrivalHoles);
    }

    /// **同批更早爆炸那一发挖的洞，对后解算的那一发可见**（`docs/06` §39.4 的机制）。
    ///
    /// 几何是手搭的，不用回放：一堵竖墙在 `x = 0`（半径 1 的圆盘沿 y 排开），
    /// 两发都从右边水平向左打（常数函数，`y` 由起点决定）：
    ///
    /// * `owner=1` 从 `(10, 0.0)` 出发 —— 先到（弧长更短）
    /// * `owner=2` 从 `(12, -0.1)` 出发 —— 后到，且它的路径从 1 号挖的洞里穿过
    ///
    /// 报文顺序故意给成 `[2, 1]`（后到的排在前面），于是三种读法分得开：
    ///
    /// * **S1**（报文序 + 共用掩膜）：2 号对着完好的墙停住；
    /// * **S2**（到达序 + **不**挖洞）：与 S1 **逐位相同** —— 只换序对几何没有任何影响；
    /// * **S3**（到达序 + 逐发挖洞）：1 号先解、在墙上挖洞，2 号从洞里穿过去 ⇒ 停得更靠左。
    #[test]
    fn arrival_order_makes_earlier_holes_visible() {
        // 墙：x = 0 上从 y=-3 到 3 排一串半径 1 的圆盘（`rasterize` 的单位是棋盘单位）
        let mut obstacles = Vec::new();
        let mut y = -3.0;
        while y <= 3.0 {
            obstacles.push((0.0, y, 1.0));
            y += 0.25;
        }
        let m = Match::new(room(), obstacles, vec![], (0, 0));
        let subs = [
            Submission { owner_id: 2, function_text: "0", x0: 12.0, y0: -0.1, flip: true },
            Submission { owner_id: 1, function_text: "0", x0: 10.0, y0: 0.0, flip: true },
        ];
        let s1 = m.fire_functions_with(&subs, SettleOrder::S1Message);
        let s2 = m.fire_functions_with(&subs, SettleOrder::S2ArrivalFlat);
        let s3 = m.fire_functions_with(&subs, SettleOrder::S3ArrivalHoles);
        let s4 = m.fire_functions_with(&subs, SettleOrder::S4TickHoles);
        let s6 = m.fire_functions_with(&subs, SettleOrder::S6MessageHoles);

        // ① 两发都必须真的撞在墙上（否则这个几何什么都没量到）
        assert!(s1[0].blocked && s1[1].blocked, "两发都该撞墙：{:?}", (s1[0].blocked, s1[1].blocked));
        // ② 顺序对几何没有任何影响（S2 == S1，逐位）
        assert_eq!(s2[0].stop, s1[0].stop, "只换序不许改停点");
        assert_eq!(s2[1].stop, s1[1].stop, "只换序不许改停点");
        // ③ 1 号是**先到**的那一发：它在 S1 里就停在同一处（洞对挖洞者自己不可见）
        assert_eq!(s3[1].stop, s1[1].stop, "先到的那一发不受影响");
        // ④ 2 号解得更晚 ⇒ 它看得见 1 号的洞 ⇒ 停点必须变，而且停得更靠左（洞挖穿了墙）
        assert_ne!(s3[0].stop, s1[0].stop, "后到的那一发必须看得见前面的洞");
        assert!(
            s3[0].stop.0 < s1[0].stop.0 - 1e-9,
            "应当穿过洞继续往左：S1 x={} / S3 x={}",
            s1[0].stop.0,
            s3[0].stop.0
        );
        // ⑤ 报文序给洞（S6 = 55 号的 `prefix`）：2 号排在前面 ⇒ 它**看不到** 1 号的洞
        assert_eq!(s6[0].stop, s1[0].stop, "报文序里 2 号在前 ⇒ 拿不到 1 号的洞");
        // ⑥ S4（整数 tick 序）与 S3 同一序（两发的飞行 tick 不同）⇒ 停点相同
        assert_eq!(s4[0].stop, s3[0].stop, "S4 与 S3 在这个几何里同序");
        // ⑦ 两发的点列**一字节都没变**（洞只改停点，不改弹道采样）
        assert_eq!(s3[0].points, s1[0].points, "弹道点列与掩膜无关（`shot_points` 不吃掩膜）");
    }

    /// 排队点与到期点必须是**同一个点**（原版 503/503 逐位相同），
    /// 且该点与采样停点的 y 不同 —— 后者正是"我方挖洞点在回放里不存在"的原因。
    #[test]
    fn queued_and_expired_destroy_points_are_identical() {
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (0, 0));
        // 起点 y 取一个"印出来会被截断"的值：0.123456 ⇒ 修正文本 `" + 0.123"`。
        // 弹道是常数 `0.00 + c`，所以整条线都在 y=0.123456，必然撞上 (-5,0) 半径 1 的障碍。
        let msgs = m.fire(1, "0.00", 10.0, 0.123456, true, 0);
        let q = msgs
            .iter()
            .find(|s| s.starts_with(r#"{"QueueDestroyObstacle""#))
            .expect("撞障碍应排队销毁");
        let q = parse_message(q).expect("合法 JSON");
        let due = m.tick(60 + DESTROY_DELAY_TICKS);
        let d = due
            .iter()
            .find(|s| s.starts_with(r#"{"DestroyObstacle""#))
            .expect("到期应删除");
        let d = parse_message(d).expect("合法 JSON");
        let (qx, qy) = (get_f64(&q, "pos_x").unwrap(), get_f64(&q, "pos_y").unwrap());
        let (dx, dy) = (get_f64(&d, "pos_x").unwrap(), get_f64(&d, "pos_y").unwrap());
        assert_eq!((qx, qy), (dx, dy), "排队点与到期点必须是同一点");
        // 挖洞点 y = 印出来的修正文本（`" + 0.123"`）算出来的 0.123；
        // 采样停点 y = 0.123456。两者差 4.56e-4 —— 正是那 200 例的成因。
        assert_eq!(qy, 0.123, "y 应是印出来的修正文本算的：{qy}");
        assert!((qy - 0.123456).abs() > 1e-5, "不能是采样停点：{qy}");
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

    /// 无遮挡的四人局面。坐标与 `graphwar_game_server` 的固定布局一致，但**没有障碍**，
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
    fn ai_info(st: &TurnStart) -> Option<crate::_scaffold::protocol::RawMessage> {
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
        assert_eq!(text, crate::math_function::to_text(toks));
        assert_eq!(Some(text.as_str()), m.ai_function_text(231).as_deref());
        // 提交时没有修正文本（抓包帧 135/158/200）。
        assert!(get_str(&info, "corrected_function").is_none(), "提交时的 corrected_function 是 null");
        assert_eq!(get_str(&info, "corrected_function"), None);
        // 与 `TurnStart.evolution`（同一份 token）同源。
        let ev = st.evolution.as_ref().expect("电脑玩家应进化");
        assert_eq!(crate::math_function::to_text(&ev.tokens), text);
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
            crate::graphwar_shot_calculator::corrected_text("", -19.512760846799154, 10.99018276610812, false),
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
            let toks = crate::math_function::parse(&text);
            assert!(!toks.is_empty(), "文本解析回空 token 流：{text}");
            assert_eq!(crate::math_function::to_text(&toks), text, "文本不是不动点（{text}）");
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
        let msg = crate::_scaffold::protocol::parse_message(&player_info_message(229, 228))
            .expect("应能解析回 RawMessage");
        assert_eq!(msg.kind, "PlayerInfo");
        assert_eq!(get_int(&msg, "player_id"), Some(229));
        assert_eq!(get_int(&msg, "connection_id"), Some(228));
    }
}

