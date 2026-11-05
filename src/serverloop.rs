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
//! 未实现/简化，均在 `docs/62` 写明：颜色分配、真实玩家加入流程、
//! 客户端 `TickRequest` 节拍（这里由服务端自己推进）。

use crate::game::Match;
use crate::levelgen;
use crate::protocol::{emit_end_of_turn, emit_game_state, emit_turn_info, parse_message, Frame, RawMessage};
use crate::rng::ChaCha12;
use crate::server::{on_new_connection, on_tick_request, RoomState};
use crate::settle::Target;
use crate::tick::{end_of_turn_tick_wire, tick_at};

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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            addr: "127.0.0.1:0".into(),
            room_name: "Graphwar Game".into(),
            time_mode: "Timer1m".into(),
            seed: crate::rng::seed_from_os(),
            per_team: 2,
        }
    }
}

struct PlayerState {
    player_id: i64,
    soldier_id: i64,
    team: u8,
    x: f64,
    y: f64,
    function: String,
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
    serve(&mut sock, &cfg, local.to_string())
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

fn serve(sock: &mut TcpStream, cfg: &Config, addr: String) -> std::io::Result<String> {
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
    let mut turn: usize = 0;
    let mut current: Option<(i64, i64)> = None; // (player_id, soldier_id)
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

                // 士兵：两队对称站位（真实关卡是随机的，这里固定以便复现）
                let mut soldiers = Vec::new();
                let mut id = 230;
                for t in 0..2u8 {
                    for k in 0..cfg.per_team {
                        let x = if t == 0 { -10.0 } else { 10.0 };
                        let y = -4.0 + 4.0 * k as f64;
                        players.push(PlayerState {
                            player_id: 229 + t as i64,
                            soldier_id: id,
                            team: t + 1,
                            x,
                            y,
                            function: "0.00".into(),
                        });
                        soldiers.push(Target { id, x, y, radius: 0.5 });
                        msgs.push(format!(
                            r#"{{"SoldierInfo":{{"soldier_id":{id},"player_id":{pid}}}}}"#,
                            id = id,
                            pid = 229 + t as i64
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

                let mut m2 = Match::new(room.clone(), obstacles.iter().map(|o| (o.x, o.y, o.r)).collect(), soldiers, (5, base_ms));
                for p in &players {
                    m2.set_team(p.soldier_id, p.team);
                }
                m = Some(m2);

                // 第一回合
                let p = &players[0];
                let start_tick = tick_at(5, base_ms, now_ms());
                current = Some((p.player_id, p.soldier_id));
                msgs.push(emit_turn_info(p.player_id, p.soldier_id));
                msgs.push(emit_end_of_turn(
                    end_of_turn_tick_wire(start_tick, &cfg.time_mode),
                    &cfg.time_mode,
                ));
                msgs.push(emit_game_state("WaitingForFunctions"));
                send(sock, &msgs)?;
                eprintln!("STARTED obstacles={} soldiers={}", m.as_ref().unwrap().obstacles.len(), players.len());
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
                let (soldier, x0, y0, function, team) = {
                    let p = &players[idx];
                    (p.soldier_id, p.x, p.y, p.function.clone(), p.team)
                };
                if !mref.soldiers.iter().any(|s| s.id == soldier) {
                    eprintln!("SOLDIER_DEAD {}", soldier);
                    continue;
                }
                // 朝向：Team1 向右、Team2 向左（真实游戏由朝向决定，这里是固定规则）
                let flip = team == 2;
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
                // 下一位
                turn += 1;
                let alive: Vec<&PlayerState> =
                    players.iter().filter(|p| mref.soldiers.iter().any(|s| s.id == p.soldier_id)).collect();
                if alive.is_empty() {
                    ended = true;
                    break;
                }
                let p = alive[turn % alive.len()];
                current = Some((p.player_id, p.soldier_id));
                let start_tick = tick_at(5, base_ms, now_ms());
                send(
                    sock,
                    &[
                        emit_turn_info(p.player_id, p.soldier_id),
                        emit_end_of_turn(
                            end_of_turn_tick_wire(start_tick, &cfg.time_mode),
                            &cfg.time_mode,
                        ),
                        emit_game_state("WaitingForFunctions"),
                    ],
                )?;
            }
            _ => {}
        }
    }
    eprintln!("DONE");
    Ok(addr)
}
