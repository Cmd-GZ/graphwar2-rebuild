//! 把已有零件装配成一个**能跑完整一局的服务端**。
//!
//! 装配顺序（每一步都有对应的真实报文依据）：
//!
//! ```text
//! NewConnectionRequest  -> 状态同步 7 条            (docs/33)
//! GameStartRequest      -> 生成关卡 + 士兵/位置/生命 + WaitingForFunctions
//! FunctionUpdateRequest -> 记录该玩家的函数
//! FunctionFireRequest   -> begin_turn -> fire(8 条) -> tick(到期销毁) -> end_turn
//!                       -> 某队全灭则 EndingGame    (docs/45)
//! ```
//!
//! ## 回合推进：走轮转，不再由这里的玩家表直接挑人（`docs/117`）
//!
//! 2026-11 之前，谁开火是本文件自己算的（`alive[turn % alive.len()]`），
//! 而 `src/turn.rs` 的三态轮转与 `src/aigen.rs` 的按士兵进化**没有调用点**。
//! 现在每个回合开始都走 [`Match::begin_next_turn`]：
//!
//! ```text
//! ① TurnManager::begin_turn  -> TurnDecision（Some(id) / Some(0) / None）
//! ② 只为判出的 soldier_id 跑 SoldierEvolver::evolve_soldier（25 s 预算，仅电脑玩家）
//! ③ broadcast() == Some((p, s)) 才发报文 —— 这就是"Some(0) 仍发 / None 不发"的开关
//! ④ 进化过 ⇒ 同一条 `begin_next_turn` 里多一条 `FunctionInfo`（AI 说话，`docs/127`）
//! ```
//!
//! ## 电脑玩家的函数由服务端 AI 决定（`docs/127` §5）
//!
//! 真游戏里机器人也是一个**客户端**：它算完把函数交上来（`FunctionUpdateRequest`），
//! 再 `FunctionFireRequest`（原始抓包 `docs/127` §2 的 fd15 帧 36/60 就是机器人这么发的）。
//! 本仓库里机器人住在服务端，所以这两处都改读 [`Match::ai_function_text`]：
//!
//! * `FunctionUpdateRequest` 的回执（提交时的 `FunctionInfo{corrected_function: null}`）；
//! * `FunctionFireRequest` 的开火函数（修正版的 `FunctionInfo` 由 `Match::fire` 发）。
//!
//! 客户端对机器人交上来的文本**不生效**（它是重建版的测试客户端，只会发 `"0.00"`）。
//!
//! **玩家出场顺序仍然由调用方给**（`docs/47`：真游戏来自 HashMap 迭代序，不可复现）：
//! [`Config::player_order`]；留空时用"玩家/士兵的构造顺序"，这是**重建版自有规则**，
//! 已在 `docs/117` §5 声明，不是复原结论。
//!
//! 未实现/简化，均在 `docs/62` 写明：颜色分配、真实玩家加入流程、
//! 客户端 `TickRequest` 节拍（这里由服务端自己推进）。

use crate::aigen::VarMode;
use crate::game::{AiConfig, Match, PopScope, TurnStart};
use crate::levelgen;
use crate::protocol::{parse_message, Frame, RawMessage};
use crate::rng::ChaCha12;
use crate::server::{on_new_connection, on_tick_request, RoomState};
use crate::settle::Target;
use crate::turn::{SkipReason, Team};

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Instant;

/// 服务端可调项。
pub struct Config {
    pub addr: String,
    pub room_name: String,
    pub time_mode: String,
    pub seed: [u8; 32],
    /// 每队士兵数
    pub per_team: usize,
    /// 玩家出场顺序。**必须由调用方给**（`docs/47`：真游戏是 HashMap 迭代序，
    /// 不可确定性复现）。留空 ⇒ 用玩家表的**构造顺序**（重建版自有规则，`docs/117` §5）。
    pub player_order: Vec<i64>,
    /// 哪些玩家是**电脑玩家**（只有它们的回合跑 AI 进化）。默认空 = 没有电脑玩家：
    /// 本 harness 的对面是一个人类测试客户端。
    pub bot_players: Vec<i64>,
    /// 电脑玩家难度档 1..=6（`docs/67` 的六档参数）。**默认 1 是重建版取值**：
    /// `RoomState` 没有难度字段（`docs/117` §6）。
    pub ai_level: u8,
    /// AI 变量模式（`docs/94`）。**不**从 `room.function_mode` 推导（没有证据）。
    pub ai_mode: VarMode,
    /// 每回合进化的墙钟预算：`Some(25 s)` = 真游戏（`0x51b7cf`，`docs/93`）；
    /// `None` = 不限时（测试用来保证可复现）。
    pub ai_budget_secs: Option<f64>,
    /// 进化种群的归属（`docs/98` §6-A **未定**，见 [`PopScope`]）。
    pub ai_pop_scope: PopScope,
    /// 回合模式：`"SequentialTurns"`（默认，一回合一人一发）或
    /// `"SimultaneousTurns"`（一回合每个存活玩家各一发、一次结算一次广播，`docs/141`）。
    pub turn_mode: String,
    /// 同时回合制的**收集截止**（tick）。`None` = 不超时，只等"所有公告玩家都交了"。
    ///
    /// ⚠️ **真值未知**：`EndOfTurnInfo.end_of_turn_tick` 在真回放里恒为 0
    /// （`1793884702` 每回合都是 `{"end_of_turn_tick":0}`），而 60Hz tick 在回合开始时就
    /// 已上万 —— 那个字段**不能**当截止用。真游戏靠什么超时（墙钟？tick？根本不超时）
    /// 没有证据，这里只提供旋钮，不猜数值。
    pub sim_turn_timeout_ticks: Option<i64>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            addr: "127.0.0.1:0".into(),
            room_name: "Graphwar Game".into(),
            time_mode: "Timer1m".into(),
            seed: crate::rng::seed_from_os(),
            per_team: 2,
            player_order: Vec::new(),
            bot_players: Vec::new(),
            ai_level: 1,
            ai_mode: VarMode::Normal,
            // 25 s 上限**默认就接上**（此前 `evolve()` 传 `None`，上限实际没生效）。
            ai_budget_secs: Some(crate::aigen::GAME_TIME_BUDGET_SECS),
            ai_pop_scope: PopScope::default(),
            turn_mode: "SequentialTurns".into(),
            sim_turn_timeout_ticks: None,
        }
    }
}

/// 本 harness 里**第一条连接**（= 那个真实客户端）拿到的 `player_id`。
///
/// 两处共用它，所以不可能出现"`PlayerInfo` 说 229、开局那批 `SoldierInfo` 说别的"：
/// * 开局造玩家槽时：`FIRST_PLAYER_ID + t`（`t` = 队伍下标，`docs/117`）；
/// * 收到 `NewPlayerRequest` 回 `PlayerInfo` 时（`docs/139` §4）。
///
/// 这是**重建版自有规则**（真游戏由服务端的玩家表分配，顺序不可复现，`docs/47`）；
/// 本 harness 只有一条连接，所以它就是第一个玩家槽。没有玩家槽的极端配置
/// （`per_team = 0`）下这条 `PlayerInfo` 指向一个不存在的玩家 —— 客户端会看到空战场，
/// 不会崩（`docs/139` §5 记为已知边界）。
pub const FIRST_PLAYER_ID: i64 = 229;

/// 一名士兵的**局面**（坐标 / 当前函数）。
///
/// 注意它**不持有**队伍：朝向由 `TurnManager::flip_for`（= 队伍，`docs/105`）给，
/// 队伍登记在 `Match::register_player` 那一侧，这里再存一份就是第二个真相源。
struct PlayerState {
    player_id: i64,
    soldier_id: i64,
    x: f64,
    y: f64,
    function: String,
}

// ---------------------------------------------------------------------------
// 回合观测日志（**不进协议**）
// ---------------------------------------------------------------------------

/// 一条"轮到谁"的记录。
///
/// 为什么需要它：AI 的产物在 2026-11 之前**一条都上不了线**（`token → 文本` 没实现），
/// 那时这条记录是"AI 真的为这个士兵跑过"的唯一可观测痕迹。现在 AI 的函数已经进了
/// 报文（`docs/127`），日志的价值变成**把"该发什么"和"发了什么"对上**：
/// `function_text` / `corrected` 与同一回合报文里的两条 `FunctionInfo` 逐字符相同
/// （端到端测试就是这么比的）。它**不进协议**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRecord {
    /// 本回合轮到哪个玩家。
    pub player_id: i64,
    /// 广播出去的 `soldier_id`。`None` = 本回合**没发** `TurnInfo`（被跳过）；
    /// `Some(0)` = 发了 `TurnInfo{p, 0}`（全灭）。
    pub soldier_id: Option<i64>,
    /// 没发的理由（只有 `soldier_id == None` 时有意义）。
    pub skip_reason: Option<SkipReason>,
    /// 本回合的开火方向（`docs/105`：由队伍决定）。
    pub flip: Option<bool>,
    /// AI 是否**真的**为这个士兵跑了一次进化。
    pub evolved: bool,
    /// 进化后写进该士兵槽的 token 数（没进化 = 0）。
    pub function_tokens: usize,
    /// AI 这一回合说出的**函数文本**（`function::to_text`，`0x4ec160`）。没进化 = `None`。
    ///
    /// 它必须与 `begin_next_turn` 那条 `FunctionInfo.function` 逐字符相同。
    pub function_text: Option<String>,
    /// 同一回合的**修正后**文本（`f(x) ± c`，三位小数）。它必须与开火那一刻
    /// `Match::fire` 发的那条 `FunctionInfo.corrected_function` 逐字符相同。
    pub corrected: Option<String>,
}

impl TurnRecord {
    fn from_start(m: &Match, player_id: i64, st: &TurnStart) -> Self {
        let announced = st.decision.broadcast();
        // AI 说话的唯一来源：本回合进化的 token（= 写进槽的那份）。
        let function_text = st.evolution.as_ref().map(|e| crate::function::to_text(&e.tokens));
        // 修正后的文本用**射手自己的位置与朝向**算，与 `Match::fire` 同一口径。
        let corrected = match (function_text.as_deref(), announced) {
            (Some(text), Some((_, sid))) => m
                .soldiers
                .iter()
                .find(|t| t.id == sid)
                .map(|t| crate::shot::corrected_text(text, t.x, t.y, st.decision.flip().unwrap_or(false))),
            _ => None,
        };
        TurnRecord {
            player_id,
            soldier_id: announced.map(|(_, s)| s),
            skip_reason: st.decision.skip_reason(),
            flip: st.decision.flip(),
            evolved: st.evolution.is_some(),
            function_tokens: st.evolution.as_ref().map(|e| e.tokens.len()).unwrap_or(0),
            function_text,
            corrected,
        }
    }
}

/// 一局的回合日志。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnLog {
    pub records: Vec<TurnRecord>,
}

impl TurnLog {
    /// 真的广播出去的 `(player_id, soldier_id)` 序列（`soldier_id` 可能是 0）。
    pub fn announced(&self) -> impl Iterator<Item = (i64, i64)> + '_ {
        self.records.iter().filter_map(|r| r.soldier_id.map(|s| (r.player_id, s)))
    }

    /// 只为这些士兵跑过进化（顺序即发生顺序，可能有重复——同一士兵第二次轮到会重算）。
    pub fn evolved_soldiers(&self) -> Vec<i64> {
        self.records.iter().filter(|r| r.evolved).filter_map(|r| r.soldier_id).collect()
    }
}

/// 跑一局：接受一个客户端并服务到结束。
///
/// 返回实际监听的地址（`addr` 用 `:0` 时由系统分配）。
///
/// ## 关卡种子的**测试旋钮**（`docs/139` §6.4）
///
/// 默认关卡种子来自 `seed_from_os()`（`Config::default`），所以**每局障碍布局都不同**。
/// 对"用真服务端跑端到端"的自检来说这是骰子：偶发的一局里障碍会压住射手
/// （开局位置固定 `x = ±10`，生成器不看这两个点），那一发在任何采样下都在第一点被
/// 掩膜判停，用例就只能靠"换一局"重试。
///
/// 设 `GW2_MATCH_SEED=<u64>` 就把种子钉死，于是同一份布局可以反复复跑。
/// **不设时行为与以前完全一致**（仍然 `seed_from_os()`）；子进程继承父进程的环境，
/// 所以自检用 `OS.set_environment()` + `OS.create_process()` 就能用上它。
pub fn run(mut cfg: Config) -> std::io::Result<String> {
    if let Ok(raw) = std::env::var("GW2_MATCH_SEED") {
        let raw = raw.trim().to_string();
        if let Ok(v) = raw.parse::<u64>() {
            // 32 字节种子：前 8 字节是原值，接下来 8 字节是一个黄金比例混合，
            // 只为让"相邻的 v"给出差别更大的流（挑种子好挑一点）。
            let mut seed = [0u8; 32];
            seed[..8].copy_from_slice(&v.to_le_bytes());
            seed[8..16].copy_from_slice(&v.wrapping_mul(0x9E37_79B9_7F4A_7C15).to_le_bytes());
            cfg.seed = seed;
            eprintln!("SEED {}", v);
        }
    }
    let listener = TcpListener::bind(&cfg.addr)?;
    let local = listener.local_addr()?;
    eprintln!("LISTEN {}", local);
    let (mut sock, peer) = listener.accept()?;
    eprintln!("ACCEPT {}", peer);
    let (addr, _log) = serve(&mut sock, &cfg, local.to_string())?;
    Ok(addr)
}

fn send(sock: &mut TcpStream, msgs: &[String]) -> std::io::Result<()> {
    let mut out = Vec::new();
    for m in msgs {
        out.extend_from_slice(&(m.len() as u16).to_be_bytes());
        out.extend_from_slice(m.as_bytes());
    }
    sock.write_all(&out)?;
    sock.flush()
}

fn read_frame(sock: &mut TcpStream) -> std::io::Result<Option<RawMessage>> {
    let mut hdr = [0u8; 2];
    if sock.read_exact(&mut hdr).is_err() {
        return Ok(None); // 对端关闭
    }
    let len = u16::from_be_bytes(hdr) as usize;
    let mut buf = hdr.to_vec();
    buf.resize(2 + len, 0);
    sock.read_exact(&mut buf[2..])?;
    let payload = Frame::decode(&buf).map(|(f, _)| f.payload);
    Ok(payload.and_then(|p| parse_message(&p)))
}

/// 在一个已建立的连接上跑完整流程（测试与 `run` 共用）。
pub fn serve_stream(sock: &mut TcpStream, cfg: &Config) -> std::io::Result<()> {
    serve(sock, cfg, String::new()).map(|_| ())
}

/// 同 [`serve_stream`]，但把回合日志交回来（端到端测试用它断言轮转与 AI）。
pub fn serve_stream_logged(sock: &mut TcpStream, cfg: &Config) -> std::io::Result<TurnLog> {
    serve(sock, cfg, String::new()).map(|(_, log)| log)
}

/// 推进到"本回合的射手"。
///
/// `*at` 指向**刚刚打完**的那个玩家；本函数先自增再看，所以
/// `*at = order.len() - 1` 的初值会让第一回合落到 `order[0]`。
///
/// 三态怎么落地：
///
/// * `Some((p, s))`，`s != 0` → 停在这里，报文交回给调用方；
/// * `Some((p, 0))` → **仍然**把 `TurnInfo{p,0}` 的报文放进去（`0x463fd0` 观测），
///   但这回合不会有 `FunctionFire`，所以继续找下一位。真游戏靠哪一层推进
///   **未知**（`docs/98` §6-D），这里的"立刻推进"是**重建版自有规则**；
/// * `None`（`Skip`）→ 一条报文都不发，直接下一位。
///
/// 有界：最多试 `order.len()` 次，全都不行就返回 `current: None`（等于没人能开火）。
fn advance(
    m: &mut Match,
    order: &[i64],
    at: &mut usize,
    log: &mut TurnLog,
    now_ms: i64,
    time_mode: &str,
) -> Advance {
    let mut messages = Vec::new();
    if order.is_empty() {
        return Advance { messages, current: None };
    }
    for _ in 0..order.len() {
        *at = (*at + 1) % order.len();
        let pid = order[*at];
        // `timed_out = false`：**超时检测没有证据**，本服务端不实现它
        // （`docs/117` §6；用户口述"超时 = 跳过该玩家"仍未独立验证）。
        let st = m.begin_next_turn(pid, false, now_ms, time_mode);
        log.records.push(TurnRecord::from_start(m, pid, &st));
        let announced = st.decision.broadcast();
        messages.extend(st.messages);
        match announced {
            Some((p, sid)) if sid != 0 => {
                return Advance { messages, current: Some((p, sid)) };
            }
            // 全灭：`TurnInfo{p,0}` 已经放进 messages，本回合没有开火，继续找。
            Some(_) => continue,
            // Skip：什么都没发，继续找。
            None => continue,
        }
    }
    Advance { messages, current: None }
}

struct Advance {
    messages: Vec<String>,
    current: Option<(i64, i64)>,
}

/// 电脑玩家这一回合的 **AI 文本**（`docs/127`）；不是电脑玩家 / 还没登记 ⇒ `None`。
///
/// 人类玩家永远走 `None`：他们的函数来自 `FunctionUpdateRequest`，不替他们编。
fn ai_text_of(m: Option<&Match>, player_id: i64, soldier_id: i64) -> Option<String> {
    let m = m?;
    if !m.is_bot(player_id) {
        return None;
    }
    m.ai_function_text(soldier_id)
}

// ---------------------------------------------------------------------------
// 同时回合制（`docs/141`）：收集阶段
// ---------------------------------------------------------------------------

/// 已经受理、**还没结算**的一发。收集阶段的唯一产物。
#[derive(Debug, Clone)]
struct SimShot {
    soldier_id: i64,
    x0: f64,
    y0: f64,
    flip: bool,
    function: String,
}

/// 同时回合制的一局收集状态。
struct SimRound {
    /// 本回合每个能开火的玩家各一发（`turn::RoundCollection`，超时/攒齐由它判）。
    col: crate::turn::RoundCollection<SimShot>,
    /// `player_id -> 本回合公告给他的射手`（`TurnInfo.soldier_id`）。
    ///
    /// 为什么不能从 `players` 里按 `player_id` 找：`players` 是**按士兵**建的表
    /// （一名玩家 `per_team` 条），`find(pid)` 只会命中他的**第一个**士兵，
    /// 而本回合的射手往往是第二个/第三个（`docs/139` §4.1 踩过同一个坑）。
    shooters: std::collections::BTreeMap<i64, i64>,
}

impl SimRound {
    /// 收集是否结束（攒齐 or 超时）。
    fn should_resolve(&self, now_tick: i64) -> bool {
        self.col.should_resolve(now_tick)
    }

    /// 收一发，返回一声明给人看的处理结果（日志用）。
    fn submit(&mut self, player_id: i64, shot: SimShot) -> crate::turn::SubmitResult {
        self.col.submit(player_id, shot)
    }
}

/// 开一个同时回合：**全体玩家一次公告**，返回报文 + 收集器。
///
/// 公告顺序就是结算批的射击者顺序（`docs/141` §4，130/130 批），所以这里不做任何重排。
fn start_sim_round(
    m: &mut Match,
    order: &[i64],
    log: &mut TurnLog,
    cfg: &Config,
    now_ms: i64,
) -> (Vec<String>, SimRound) {
    let (msgs, starts) = m.begin_simultaneous_turn(order, now_ms, &cfg.time_mode);
    let mut waitlist: Vec<i64> = Vec::with_capacity(starts.len());
    for (pid, st) in order.iter().zip(starts.iter()) {
        log.records.push(TurnRecord::from_start(m, *pid, st));
        if let Some((p, s)) = st.decision.broadcast() {
            if s != crate::turn::SOLDIER_ID_NONE {
                waitlist.push(p);
            }
        }
    }
    let deadline = cfg
        .sim_turn_timeout_ticks
        .map(|d| m.tick_now(now_ms) + d.max(0));
    let mut shooters = std::collections::BTreeMap::new();
    for st in &starts {
        if let Some((p, s)) = st.decision.broadcast() {
            if s != crate::turn::SOLDIER_ID_NONE {
                shooters.insert(p, s);
            }
        }
    }
    (
        msgs,
        SimRound { col: crate::turn::RoundCollection::new(waitlist, deadline), shooters },
    )
}

/// 结算一个同时回合：`fire_batch` → 到期销毁 → `end_turn`，然后开下一回合。
///
/// 三段的分界就是 `docs/141` §1 的"结算 / 广播"：
/// `fire_batch` 一口气产出**整个结算批**（分组的前缀 + 按到达时刻排的死亡块 +
/// 每发一对 `FunctionActive`/`Explosion`），**不是**"一发一个完整窗口"。
fn resolve_sim_round(
    m: &mut Match,
    round: SimRound,
    order: &[i64],
    log: &mut TurnLog,
    cfg: &Config,
    now_ms: i64,
) -> (Vec<String>, SimRound) {
    let shots = round.col.resolve();
    let mut out = Vec::new();
    let subs: Vec<crate::game::Submission<'_>> = shots
        .iter()
        .map(|(_, s)| crate::game::Submission {
            owner_id: s.soldier_id,
            function_text: &s.function,
            x0: s.x0,
            y0: s.y0,
            flip: s.flip,
        })
        .collect();
    out.extend(m.fire_batch(&subs, now_ms));
    let tick_now = m.tick_now(now_ms);
    out.extend(m.tick(tick_now + crate::game::DESTROY_DELAY_TICKS));
    out.extend(m.end_turn());
    if out.iter().any(|s| s.contains("EndingGame")) {
        // 结束：下一回合的收集器留空（调用方据 `EndingGame` 收工）。
        return (
            out,
            SimRound {
                col: crate::turn::RoundCollection::new(Vec::<i64>::new(), None),
                shooters: Default::default(),
            },
        );
    }
    let (next_msgs, next) = start_sim_round(m, order, log, cfg, now_ms);
    out.extend(next_msgs);
    (out, next)
}

fn serve(sock: &mut TcpStream, cfg: &Config, addr: String) -> std::io::Result<(String, TurnLog)> {
    let room = RoomState {
        room_name: cfg.room_name.clone(),
        game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: cfg.turn_mode.clone(),
        time_mode: cfg.time_mode.clone(),
    };
    let t0 = Instant::now();
    let now_ms = || t0.elapsed().as_millis() as i64;
    let base_ms = now_ms();

    let mut players: Vec<PlayerState> = Vec::new();
    let mut m: Option<Match> = None;
    let mut current: Option<(i64, i64)> = None; // (player_id, soldier_id)
    // 同时回合制的收集状态（`docs/141`）。`Some` = 本回合正在攒提交，还没结算。
    // 与 `current` 互斥：`SequentialTurns` 用 `current`（一次一个射手），
    // `SimultaneousTurns` 用 `sim`（一次等全体）。
    let mut sim: Option<SimRound> = None;
    // 玩家出场顺序 + 游标（`docs/47`：顺序由调用方给，不由本文件推导）。
    let mut order: Vec<i64> = Vec::new();
    let mut at: usize = 0;
    let mut log = TurnLog::default();
    let mut started = false;
    let mut ended = false;
    // 这条连接自报的 `connection_id`（`NewPlayerRequest` 带来）。只用来回 `PlayerInfo`，
    // 不参与任何对局状态 —— 缺省 228 是本 harness 在 `NewConnection` 里固定给的那个值。
    let mut my_connection_id: i64 = 228;

    while let Some(frame) = read_frame(sock)? {
        let kind = frame.kind.clone();
        eprintln!("RECV {}", kind);
        match kind.as_str() {
            "NewConnectionRequest" => {
                let msgs = on_new_connection(&room, 228);
                send(sock, &msgs)?;
            }
            // ---- 「我是谁」（本项**只加**，`docs/33` §4 明确列的缺口）----
            //
            // 真客户端加入时发 `NewPlayerRequest{connection_id}`，服务端回
            // `PlayerInfo{player_id, connection_id}`（抓包原文，`docs/24` §3）。
            // 本 harness 只有**一条**连接、开局时也只造两队的第一个玩家槽，
            // 所以这条连接拿到的就是 `FIRST_PLAYER_ID`（开局那批 `SoldierInfo`/`TeamInfo`
            // 用的是同一个 id —— 两处共用一个常量，不可能是两个值）。
            //
            // 这样 `GwNetSession.my_player_id` 才有值；在那之前客户端只能退化成
            // "按 `TurnInfo` 里的玩家假定"（`docs/134` §7.1，`docs/139` §4）。
            "NewPlayerRequest" => {
                if let Some(cid) = crate::protocol::get_int(&frame, "connection_id") {
                    my_connection_id = cid;
                }
                let msg = crate::game::player_info_message(FIRST_PLAYER_ID, my_connection_id);
                send(sock, &[msg])?;
                eprintln!(
                    "PLAYER_INFO player_id={} connection_id={}",
                    FIRST_PLAYER_ID, my_connection_id
                );
            }
            "TickRequest" => {
                let msg = on_tick_request(5, base_ms, now_ms());
                send(sock, &[msg])?;
            }
            "GameStartRequest" => {
                if started {
                    continue;
                }
                started = true;
                let mut rng = ChaCha12::from_seed(&cfg.seed);
                let mut next = || rng.next_u64();
                let obstacles = levelgen::generate(&mut next, 0);

                let mut msgs: Vec<String> = obstacles
                    .iter()
                    .map(|o| {
                        format!(
                            r#"{{"AddObstacle":{{"pos_x":{},"pos_y":{},"radius":{}}}}}"#,
                            o.x, o.y, o.r
                        )
                    })
                    .collect();

                // 士兵：两队对称站位（真实关卡是随机的，这里固定以便复现）。
                // 每个玩家的士兵表按创建顺序累积 —— 这个顺序**原样**交给
                // `Match::register_player`，不排序（`docs/98` §6-C 的未知不在这里替它决定）。
                let mut rosters: Vec<(i64, u8, Vec<i64>)> = Vec::new();
                let mut soldiers = Vec::new();
                let mut id = 230;
                for t in 0..2u8 {
                    for k in 0..cfg.per_team {
                        let x = if t == 0 { -10.0 } else { 10.0 };
                        let y = -4.0 + 4.0 * k as f64;
                        let player_id = FIRST_PLAYER_ID + t as i64;
                        players.push(PlayerState {
                            player_id,
                            soldier_id: id,
                            x,
                            y,
                            function: "0.00".into(),
                        });
                        match rosters.last_mut() {
                            Some((p, _, sids)) if *p == player_id => sids.push(id),
                            _ => rosters.push((player_id, t + 1, vec![id])),
                        }
                        soldiers.push(Target { id, x, y, radius: 0.5 });
                        msgs.push(format!(
                            r#"{{"SoldierInfo":{{"soldier_id":{id},"player_id":{pid}}}}}"#,
                            id = id,
                            pid = player_id
                        ));
                        msgs.push(format!(
                            r#"{{"TeamInfo":{{"entity_id":{id},"team":"Team{tm}"}}}}"#,
                            id = id,
                            tm = t + 1
                        ));
                        msgs.push(format!(
                            r#"{{"PosInfo":{{"entity_id":{id},"pos_x":{x},"pos_y":{y},"radius":0.5}}}}"#,
                            id = id,
                            x = x,
                            y = y
                        ));
                        msgs.push(format!(
                            r#"{{"LifeInfo":{{"entity_id":{id},"alive":true}}}}"#,
                            id = id
                        ));
                        id += 1;
                    }
                }

                let mut m2 = Match::new(
                    room.clone(),
                    obstacles.iter().map(|o| (o.x, o.y, o.r)).collect(),
                    soldiers,
                    (5, base_ms),
                )
                .with_ai(AiConfig {
                    level: cfg.ai_level,
                    mode: cfg.ai_mode,
                    budget_secs: cfg.ai_budget_secs,
                    pop_scope: cfg.ai_pop_scope,
                })
                .with_ai_seed(&cfg.seed);

                // 玩家出场顺序：配置给（`docs/47`）。留空 ⇒ 构造顺序，并在文档里声明
                // 这是重建版自有规则；配置里出现没登记过的 id 时忽略它（不让它变成 Skip）。
                let constructed: Vec<i64> = rosters.iter().map(|r| r.0).collect();
                let wanted: Vec<i64> = if cfg.player_order.is_empty() {
                    constructed.clone()
                } else {
                    cfg.player_order.iter().copied().filter(|p| constructed.contains(p)).collect()
                };
                order = if wanted.is_empty() { constructed.clone() } else { wanted };
                // 同名玩家只留第一次出现（`dedup()` 只去相邻重复，不够）。
                let mut seen = std::collections::BTreeSet::new();
                order.retain(|p| seen.insert(*p));

                for (pid, team_id, sids) in &rosters {
                    // 只建过 1/2 队（`docs/105` §4 没观测到 3 队）。
                    let team = Team::from_id(*team_id).expect("serverloop 只生成 1/2 队");
                    m2.register_player(*pid, team, sids);
                }
                for pid in &cfg.bot_players {
                    if order.contains(pid) {
                        m2.set_bot(*pid);
                    }
                }
                m = Some(m2);

                // 第一回合：从 `order[len-1]` 出发 ⇒ advance 落到 `order[0]`。
                at = order.len() - 1;
                let Some(mref) = m.as_mut() else { continue };
                if cfg.turn_mode == "SimultaneousTurns" {
                    // 全体玩家一次公告 + 开一个收集器（`docs/141` §1）。
                    let (m2msgs, round) = start_sim_round(mref, &order, &mut log, cfg, now_ms());
                    msgs.extend(m2msgs);
                    sim = Some(round);
                } else {
                    let adv = advance(mref, &order, &mut at, &mut log, now_ms(), &cfg.time_mode);
                    msgs.extend(adv.messages);
                    current = adv.current;
                }
                send(sock, &msgs)?;
                eprintln!(
                    "STARTED obstacles={} players={} order={:?} bots={:?} ai=level{} budget={:?}",
                    m.as_ref().unwrap().obstacles.len(),
                    players.len(),
                    order,
                    cfg.bot_players,
                    cfg.ai_level,
                    cfg.ai_budget_secs
                );
                if current.is_none() && sim.is_none() {
                    // 一个能开火的玩家都没有 ⇒ 没人可等（同时回合制下看 `sim`，不看 `current`）。
                    break;
                }
            }
            "FunctionUpdateRequest" => {
                if let Some(pid) = crate::protocol::get_int(&frame, "player_id") {
                    let f = crate::protocol::get_str(&frame, "function").unwrap_or_default();
                    // ⚠️ 修 bug（`docs/139` §4.1）：`players` 是**按士兵**建的（一名玩家
                    // `per_team` 条），而 `find()` 只命中该玩家的**第一条**。开火那一步是按
                    // `soldier_id` 找 `PlayerState` 的（本回合的射手往往是第二、第三名士兵），
                    // 于是它读到的是**构造时的默认值** `"0.00"` —— 客户端提交的函数根本不生效。
                    // 默认值恰好也是 `"0.00"`，所以以前的端到端（只提交 `"0.00"`）看不出来。
                    // 现在把**该玩家的每一条**都写一遍：函数是"按玩家"提交的，按士兵各存一份
                    // 只是这一层的实现细节（`docs/139` §4.1 记了这条判断）。
                    let mut any = false;
                    for p in players.iter_mut().filter(|p| p.player_id == pid) {
                        p.function = f.clone();
                        any = true;
                    }
                    if !any {
                        // 不在玩家表里（例如还没开局就提交）：行为与以前一致 —— 什么都不写。
                        eprintln!("UPDATE_UNKNOWN_PLAYER pid={}", pid);
                    }
                    // `FunctionInfo.owner_id` 是 **soldier_id**（`AGENTS.md` 硬事实）。
                    // 抓包帧 135：`FunctionUpdateRequest{player_id:229}` 之后那条
                    // `FunctionInfo` 的 `owner_id` 是 **230**（该玩家本回合的射手）。
                    // ⇒ 本行此前发的是 `player_id`，是个 bug（`docs/127` §2.1 第 2 条）。
                    let owner = match current {
                        Some((p, s)) if p == pid => s,
                        // 不在自己回合里提交：没有"当前的士兵"可指，退回 player_id 并把
                        // 这一条留成**未知**（抓包里没观测到这种时序）。
                        _ => pid,
                    };
                    // 电脑玩家的函数由**服务端 AI** 决定（`docs/127` §5）：客户端交上来的
                    // `"0.00"` 不生效，回执照 AI 的文本发。
                    let text = ai_text_of(m.as_ref(), pid, owner).unwrap_or(f);
                    send(sock, &[crate::protocol::emit_function_info(owner, &text, None)])?;
                }
            }
            "FunctionFireRequest" => {
                if ended {
                    continue;
                }
                let Some(pid) = crate::protocol::get_int(&frame, "player_id") else { continue };

                // ---- 同时回合制：**收集阶段**（`docs/141` §1）----
                //
                // 与逐玩家路径的区别只有一条：不要求"轮到你"，只要求"你在本回合的公告名单里"。
                // 收下之后**什么都不广播** —— 真回放里从提交到 `CalculatingResults`
                // 之间没有任何服务端报文（`1793884702`：FunctionFire 在 9282 段之后，
                // 结算批在 9282 tick 的 `CalculatingResults` 才出现）。
                if let Some(round) = sim.as_mut() {
                    let Some(mref) = m.as_mut() else { continue };
                    // 射手是**公告给他的那一个**（不是他在 `players` 里的第一条）。
                    let Some(&soldier) = round.shooters.get(&pid) else {
                        eprintln!("NOT_ANNOUNCED pid={pid}");
                        continue;
                    };
                    let client_function = players
                        .iter()
                        .find(|p| p.player_id == pid)
                        .map(|p| p.function.clone())
                        .unwrap_or_default();
                    // 位置以 `Match` 为准（它才是这场对局的真相源）。
                    let Some(t) = mref.soldiers.iter().find(|s| s.id == soldier) else {
                        eprintln!("SOLDIER_DEAD {soldier}");
                        continue;
                    };
                    let (x0, y0) = (t.x, t.y);
                    let flip = mref.turns().flip_for(pid).unwrap_or(false);
                    // 电脑玩家：AI 决定的函数（`docs/127` §5）。
                    let function = match mref.is_bot(pid) {
                        true => mref.ai_function_text(soldier).unwrap_or(client_function),
                        false => client_function,
                    };
                    let shot = SimShot { soldier_id: soldier, x0, y0, flip, function };
                    match round.submit(pid, shot) {
                        crate::turn::SubmitResult::Accepted => {
                            eprintln!("COLLECTED pid={pid} soldier={soldier}");
                        }
                        other => {
                            eprintln!("SUBMIT_REJECTED pid={pid} {:?} waiting={:?}", other, round.col.waiting_on());
                            continue;
                        }
                    }
                    let now = now_ms();
                    let tick_now = mref.tick_now(now);
                    if !round.should_resolve(tick_now) {
                        // 还没攒齐、也没超时：继续等，不回任何报文。
                        eprintln!("WAITING {:?}", round.col.waiting_on());
                        continue;
                    }
                    let taken = sim.take().expect("sim 刚刚还是 Some");
                    let (out, next) =
                        resolve_sim_round(mref, taken, &order, &mut log, cfg, now);
                    let annihilated = out.iter().any(|s| s.contains("EndingGame"));
                    send(sock, &out)?;
                    if annihilated {
                        // 不再受理任何提交；`sim` 保持 `None` 之外的旧值也无所谓 —— 循环退出。
                        eprintln!("ENDED at turn {}", mref.turn_count);
                        break;
                    }
                    sim = Some(next);
                    continue;
                }

                let Some(mref) = m.as_mut() else { continue };
                let Some((cur_pid, cur_sid)) = current else { continue };
                if cur_pid != pid {
                    eprintln!("NOT_YOUR_TURN pid={} current={:?}", pid, current);
                    continue;
                }
                let Some(idx) = players.iter().position(|p| p.soldier_id == cur_sid) else { continue };
                let (soldier, x0, y0, function) = {
                    let p = &players[idx];
                    (p.soldier_id, p.x, p.y, p.function.clone())
                };
                if !mref.soldiers.iter().any(|s| s.id == soldier) {
                    eprintln!("SOLDIER_DEAD {}", soldier);
                    continue;
                }
                // 朝向由**轮转层**给（Team1 → +x、Team2 → -x，`docs/105`）：
                // "谁开火"和"朝哪边"因此只有一个来源，不再在本地按 `team` 重算。
                let flip = mref
                    .turns()
                    .flip_for(pid)
                    .unwrap_or_else(|| panic!("玩家 {pid} 应已在 TurnManager 里登记"));
                // **AI 真的打自己算出来的那个函数**（`docs/127` §5）：电脑玩家不看客户端交的
                // 文本，用槽里的（= 本回合进化出来的）token 转成的文本开火。于是这条链闭环：
                // 进化 → `to_text` → 提交时的 `FunctionInfo` → 用同一份文本开火 → 修正版
                // `FunctionInfo`（`docs/117` §4 那条"文本仍来自客户端"的缺口在这里闭上）。
                let function = match mref.is_bot(pid) {
                    true => mref.ai_function_text(soldier).unwrap_or(function),
                    false => function,
                };
                let now = now_ms();
                let mut out = mref.fire(soldier, &function, x0, y0, flip, now);
                // 推进到销毁到期
                let tick_now = mref.tick_now(now);
                out.extend(mref.tick(tick_now + crate::game::DESTROY_DELAY_TICKS));
                out.extend(mref.end_turn());
                let annihilated = out.iter().any(|s| s.contains("EndingGame"));
                send(sock, &out)?;
                if annihilated {
                    ended = true;
                    eprintln!("ENDED at turn {}", mref.turn_count);
                    break;
                }
                // 下一位：走轮转（`docs/117` §2）。
                let adv = advance(mref, &order, &mut at, &mut log, now_ms(), &cfg.time_mode);
                send(sock, &adv.messages)?;
                current = adv.current;
                if current.is_none() {
                    // 一个能开火的玩家都没有 ⇒ 无人可继续。
                    ended = true;
                    break;
                }
            }
            _ => {}
        }
    }
    eprintln!("DONE");
    Ok((addr, log))
}
