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
//! ```
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
        }
    }
}

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
/// 为什么需要它：AI 进化出来的函数是 **token**，而 `token → 文本`（`0x4ec160`）
/// 还没实现（`docs/109` §6-H），所以**报文里看不到 AI 的产物**。这条记录是
/// "AI 真的为这个士兵跑过"的唯一可观测痕迹，端到端测试就断言它（`docs/117` §4）。
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
}

impl TurnRecord {
    fn from_start(player_id: i64, st: &TurnStart) -> Self {
        let announced = st.decision.broadcast();
        TurnRecord {
            player_id,
            soldier_id: announced.map(|(_, s)| s),
            skip_reason: st.decision.skip_reason(),
            flip: st.decision.flip(),
            evolved: st.evolution.is_some(),
            function_tokens: st.evolution.as_ref().map(|e| e.tokens.len()).unwrap_or(0),
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
pub fn run(cfg: Config) -> std::io::Result<String> {
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
        log.records.push(TurnRecord::from_start(pid, &st));
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

fn serve(sock: &mut TcpStream, cfg: &Config, addr: String) -> std::io::Result<(String, TurnLog)> {
    let room = RoomState {
        room_name: cfg.room_name.clone(),
        game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: cfg.time_mode.clone(),
    };
    let t0 = Instant::now();
    let now_ms = || t0.elapsed().as_millis() as i64;
    let base_ms = now_ms();

    let mut players: Vec<PlayerState> = Vec::new();
    let mut m: Option<Match> = None;
    let mut current: Option<(i64, i64)> = None; // (player_id, soldier_id)
    // 玩家出场顺序 + 游标（`docs/47`：顺序由调用方给，不由本文件推导）。
    let mut order: Vec<i64> = Vec::new();
    let mut at: usize = 0;
    let mut log = TurnLog::default();
    let mut started = false;
    let mut ended = false;

    while let Some(frame) = read_frame(sock)? {
        let kind = frame.kind.clone();
        eprintln!("RECV {}", kind);
        match kind.as_str() {
            "NewConnectionRequest" => {
                let msgs = on_new_connection(&room, 228);
                send(sock, &msgs)?;
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
                        let player_id = 229 + t as i64;
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
                let adv = advance(mref, &order, &mut at, &mut log, now_ms(), &cfg.time_mode);
                msgs.extend(adv.messages);
                current = adv.current;
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
                if current.is_none() {
                    // 一个能开火的玩家都没有 ⇒ 没人可等。
                    break;
                }
            }
            "FunctionUpdateRequest" => {
                if let Some(pid) = crate::protocol::get_int(&frame, "player_id") {
                    let f = crate::protocol::get_str(&frame, "function").unwrap_or_default();
                    if let Some(p) = players.iter_mut().find(|p| p.player_id == pid) {
                        p.function = f.clone();
                    }
                    send(
                        sock,
                        &[crate::protocol::emit_function_info(pid, &f, None)],
                    )?;
                }
            }
            "FunctionFireRequest" => {
                if ended {
                    continue;
                }
                let Some(mref) = m.as_mut() else { continue };
                let Some(pid) = crate::protocol::get_int(&frame, "player_id") else { continue };
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
