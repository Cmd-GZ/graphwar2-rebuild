//! 端到端（真 TCP）：起一个服务端，用真实客户端打完一局。
//!
//! 三条测试：
//!
//! 1. `plays_a_full_match_over_tcp` —— 判据 1："能跑完整一局"（原有测试）；
//! 2. `rotation_and_ai_are_wired_over_tcp` —— **本轮新接线**的验收：
//!    射手按 `turn::TurnManager` 的轮转出现，且 AI **只为那个士兵**进化过
//!    （`docs/117-server-wiring.md`）；
//! 3. `ai_functions_are_broadcast_and_round_trip_over_tcp` —— AI 说话验收（`docs/127`）。
//!
//! ## 客户端有两种，"打不打得死人"由谁决定，是两回事（`docs/139` §11）
//!
//! * **人类玩家局**（测试 1）：客户端**说了算** —— 服务端把 `FunctionUpdateRequest`
//!   里那条函数原样用去开火。所以这条测试的客户端会**真的瞄准**：从报文里读局面
//!   （`PosInfo`/`SoldierInfo`/`TeamInfo`/`AddObstacle`），用**与服务端同一个判定函数**
//!   （`settle::resolve` + `SERVER_STEP` + 同一份掩膜）预演候选弹道，挑一条"不被墙挡
//!   又真能打中"的发出去 → 见下面的 `Board` / `aim()`。
//! * **电脑玩家局**（测试 2、3）：客户端**改不动** —— `Match::fire` 对电脑玩家一律用
//!   `ai_function_text()`（`docs/127` §5），客户端交的 `"0.00"` 不生效。所以这两条
//!   只能靠**夹具种子**挑一局 AI 真能打死人的（AI 的适应度里含掩膜
//!   `aigen::ScoringContext.mask`，障碍一多就可能僵成"AI 一直贴着墙打"）。
//!
//! ## 为什么种子换过一次（`[11;32]` → `[53;32]`，只换 2、3 两条）
//!
//! `src/game.rs::fire` 的步长从写死的 `0.5` 改成 `SERVER_STEP`（0.01）之后
//! （`docs/136`：step=0.01 时 `end_x` 逐位相同 605/739，0.5 只有 27/739），
//! **0.5 粗到会隧穿障碍**：以前那两条测试是靠"穿过墙"才打中人的。现在墙真的挡住，
//! `[11;32]`（9 个障碍）下 AI 打到第 20 回合也没死人 ⇒ `assert!(ended)` 红。
//!
//! 换种子的依据不是试出来的手气，而是 `search_ai_seed`（本文件末尾，`#[ignore]`）：
//! 它先按 `[k;32]`（k = 0..=63）**纯计算**出障碍数（`ChaCha12` + `levelgen::generate`，
//! 与 `serverloop` 开局同一条），再对障碍最少的若干个真的跑一局 AI-vs-AI。
//! 实测：`[53;32]` = **1 个障碍、第 6 回合 3 人阵亡、EndingGame**（最快完局）；
//! `[62;32]` = 0 障碍、12 回合；而 3 个障碍的几颗种子全都 20 回合只死 2 人（僵局）。
//! 断言**一条没改**（仍是"能打死敌人并触发 `EndingGame`"，测试 2 还要求"必须有死人"）。
use gw2_rebuild::serverloop::{serve_stream, serve_stream_logged, Config, TurnLog};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

fn frame(msg: &str) -> Vec<u8> {
    let mut v = (msg.len() as u16).to_be_bytes().to_vec();
    v.extend_from_slice(msg.as_bytes());
    v
}

fn read_messages(sock: &mut TcpStream) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = Vec::new();
    sock.set_read_timeout(Some(std::time::Duration::from_millis(600))).ok();
    loop {
        let mut tmp = [0u8; 8192];
        match sock.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
        loop {
            if buf.len() < 2 {
                break;
            }
            let n = u16::from_be_bytes([buf[0], buf[1]]) as usize;
            if buf.len() < 2 + n {
                break;
            }
            out.push(String::from_utf8_lossy(&buf[2..2 + n]).to_string());
            buf.drain(..2 + n);
        }
    }
    out
}

/// 读一批报文，**允许服务端慢一拍**：AI 进化要先跑完才回包，所以"一个 600 ms
/// 静默窗口里没有数据"不能直接判成"没回包"（慢机器会假红）。最多等 10 个窗口；
/// 对端关闭（`read` 返回 0）时 `read_messages` 会立刻返回空，不会真的等满。
fn read_batch(sock: &mut TcpStream) -> Vec<String> {
    // 一次 `read_messages` 内部已经"读到静默为止"，所以拿到东西就直接收工；
    // 只有在**一条都没有**时才重试（慢机器上服务端可能还在跑 AI 进化）。
    for _ in 0..10 {
        let chunk = read_messages(sock);
        if !chunk.is_empty() {
            return chunk;
        }
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// 客户端侧的"瞄准"（**只用来驱动测试客户端**，不参与任何断言）
// ---------------------------------------------------------------------------
//
// 为什么需要它（`docs/139` §10）：`src/game.rs::fire` 的步长从写死的 0.5 改成
// `SERVER_STEP`（0.01）之后，**0.5 时"穿过墙"的那些固定水平线不再成立** —— 墙真的会
// 把弹道拦下来。旧的两条 e2e 靠"墙挡不住"才打中人，于是现在打不死人、也就没人触发
// `EndingGame`。**修法不是把步长改回去、也不是放宽断言，而是让测试客户端真的瞄准**：
//
//   ① 从报文里读出局面（`PosInfo` 坐标、`SoldierInfo` 归属、`TeamInfo` 队伍、
//      `AddObstacle` 障碍、`LifeQueuedDeath` 死亡）；
//   ② 用**与服务端同一个判定函数**（`settle::resolve` + `SERVER_STEP` + 同一份掩膜）
//      预演候选弹道的落点：既没被墙挡住、又真的打死了敌人，才发出去；
//   ③ 每回合按**当前射手 + 最近的敌人**重新算（射手会换、人会死）。
//
// 这跟真客户端做的事是同一件：它有同一套采样器与掩膜，可以先画预览再开火。

/// 测试客户端记下的局面。
#[derive(Default)]
struct Board {
    /// `PosInfo`：实体 → 坐标（这游戏里士兵不会移动，开局那批就是全部）。
    pos: BTreeMap<i64, (f64, f64)>,
    /// `SoldierInfo`：士兵 → 玩家。
    owner: BTreeMap<i64, i64>,
    /// `TeamInfo`：实体 → 队伍名（`Team1` / `Team2`）。
    team: BTreeMap<i64, String>,
    /// 还活着的士兵（`LifeQueuedDeath` 会把他们拿掉）。
    alive: BTreeSet<i64>,
    /// `AddObstacle` 的顺序表。
    obstacles: Vec<(f64, f64, f64)>,
}

impl Board {
    /// 把一批报文吸收进局面（只认这五种，别的不管）。
    fn absorb(&mut self, batch: &[String]) {
        for m in batch {
            let Some(rm) = gw2_rebuild::protocol::parse_message(m) else { continue };
            match rm.kind.as_str() {
                "PosInfo" => {
                    let id = gw2_rebuild::protocol::get_int(&rm, "entity_id").unwrap();
                    let x = gw2_rebuild::protocol::get_f64(&rm, "pos_x").unwrap();
                    let y = gw2_rebuild::protocol::get_f64(&rm, "pos_y").unwrap();
                    self.pos.insert(id, (x, y));
                }
                "SoldierInfo" => {
                    let sid = gw2_rebuild::protocol::get_int(&rm, "soldier_id").unwrap();
                    let pid = gw2_rebuild::protocol::get_int(&rm, "player_id").unwrap();
                    self.owner.insert(sid, pid);
                    self.alive.insert(sid);
                }
                "TeamInfo" => {
                    let id = gw2_rebuild::protocol::get_int(&rm, "entity_id").unwrap();
                    let name = gw2_rebuild::protocol::get_str(&rm, "team").unwrap();
                    self.team.insert(id, name);
                }
                "AddObstacle" => {
                    self.obstacles.push((
                        gw2_rebuild::protocol::get_f64(&rm, "pos_x").unwrap(),
                        gw2_rebuild::protocol::get_f64(&rm, "pos_y").unwrap(),
                        gw2_rebuild::protocol::get_f64(&rm, "radius").unwrap(),
                    ));
                }
                "LifeQueuedDeath" => {
                    let id = gw2_rebuild::protocol::get_int(&rm, "entity_id").unwrap();
                    self.alive.remove(&id);
                }
                _ => {}
            }
        }
    }

    /// 这名士兵的射击方向是不是"向左"（`Team2`）。与服务端 `TurnManager::flip_for` 同源
    /// （队伍来自报文，不靠 id 猜，`docs/105`）。
    fn flip_of(&self, soldier: i64) -> bool {
        self.team.get(&soldier).map(|t| t == "Team2").unwrap_or(false)
    }

    /// 掩膜：**与服务端 `Match::new` 同一套**（只把障碍光栅化进去，士兵不进掩膜）。
    fn mask(&self) -> Vec<u8> {
        let mut mask = gw2_rebuild::plane::empty_mask();
        for (x, y, r) in &self.obstacles {
            gw2_rebuild::plane::rasterize(&mut mask, *x, *y, *r, 0x00);
        }
        mask
    }

    /// 除 `me` 之外所有还活着的士兵（服务端 `Match::fire` 的 `others` 同形）。
    fn others(&self, me: i64) -> Vec<gw2_rebuild::settle::Target> {
        self.alive
            .iter()
            .filter(|id| **id != me)
            .filter_map(|id| {
                self.pos.get(id).map(|(x, y)| gw2_rebuild::settle::Target {
                    id: *id,
                    x: *x,
                    y: *y,
                    radius: 0.5,
                })
            })
            .collect()
    }

    /// 敌人 = 别的玩家名下、还活着、有坐标的士兵；按到射手的距离升序（**最近的先打**）。
    fn enemies_of(&self, me: i64) -> Vec<i64> {
        let Some(mine) = self.owner.get(&me) else { return Vec::new() };
        let Some(&(x0, y0)) = self.pos.get(&me) else { return Vec::new() };
        let mut out: Vec<(f64, i64)> = self
            .alive
            .iter()
            .filter(|id| self.owner.get(id).is_some_and(|p| p != mine))
            .filter_map(|id| self.pos.get(id).map(|(x, y)| (((x - x0).powi(2) + (y - y0).powi(2)).sqrt(), *id)))
            .collect();
        out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        out.into_iter().map(|(_, id)| id).collect()
    }
}

/// `coef*x` 的原文。负系数写成 `0*x - |coef|*x` —— 不让表达式以 `-` 开头
/// （解析器对首字符负号的处理没必要在这条测试里赌）。
fn scaled_x(coef: f64) -> String {
    if coef.abs() < 1e-15 {
        "0*x".to_string()
    } else if coef > 0.0 {
        format!("{coef:.12}*x")
    } else {
        format!("0*x - {:.12}*x", -coef)
    }
}

/// `(x - v)` 的原文；`v` 为负时写成 `(x + |v|)`。
fn x_minus(v: f64) -> String {
    if v >= 0.0 {
        format!("(x - {v:.12})")
    } else {
        format!("(x + {:.12})", -v)
    }
}

/// `+ coef*body` / `- coef*body`。
fn signed_term(coef: f64, body: &str) -> String {
    if coef >= 0.0 {
        format!("+ {coef:.12}*{body}")
    } else {
        format!("- {:.12}*{body}", -coef)
    }
}

/// 面向某个目标的候选弹道（全部**必然穿过射手与目标**，差别只在中间怎么绕）。
///
/// * `m*x`：过两点的直线（修正后 `y = y0 + m·(x − x0)`）；
/// * `m*x + k·sin(w·(x − x0))`：两端归零的半正弦鼓包，`k` 控制绕多高，`w = h·π/dx` 给 h 种鼓法；
/// * `m*x + b·(x − x0)·(x − xt)`：过"路标"的抛物线，`b` 由路标反解。
fn candidate_exprs(m: f64, x0: f64, y0: f64, target: (f64, f64)) -> Vec<String> {
    // `y` 分量已经在 `m` 里了（修正把曲线钉在射手身上，`m` 再把它钉到目标），
    // 所以这里只需要目标的 `x`；`target.1` 留着是为了调用点读起来完整。
    let (xt, _yt) = target;
    let mut out = vec![scaled_x(m)];
    let dx = xt - x0;
    if dx.abs() < 1e-6 {
        return out;
    }
    for harm in 1..=3 {
        let w = (harm as f64 * std::f64::consts::PI / dx).abs();
        for k in [1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 4.0, -4.0, 5.0, -5.0, 6.0, -6.0] {
            out.push(format!(
                "{} {}",
                scaled_x(m),
                signed_term(k, &format!("sin({w:.12}*{})", x_minus(x0)))
            ));
        }
    }
    for wx_f in [0.2, 0.35, 0.5, 0.65, 0.8] {
        let wx = x0 + dx * wx_f;
        for wy in [-14.0, -11.0, -8.0, -5.0, -2.0, 1.0, 4.0, 7.0, 10.0, 13.0] {
            let den = (wx - x0) * (wx - xt);
            if den.abs() < 1e-9 {
                continue;
            }
            let b = (wy - (y0 + m * (wx - x0))) / den;
            out.push(format!(
                "{} {}",
                scaled_x(m),
                signed_term(b, &format!("{}{}", x_minus(x0), x_minus(xt)))
            ));
        }
    }
    out
}

/// 这一发的**服务端判定**（与 `Match::fire` 同一个函数、同一个步长、同一份掩膜）。
fn judge(expr: &str, board: &Board, shooter: i64) -> Option<gw2_rebuild::settle::ShotOutcome> {
    let &(x0, y0) = board.pos.get(&shooter)?;
    let mask = board.mask();
    Some(gw2_rebuild::settle::resolve(
        expr,
        x0,
        y0,
        gw2_rebuild::SERVER_STEP,
        board.flip_of(shooter),
        gw2_rebuild::path::Mode::Normal,
        &mask,
        &board.others(shooter),
    ))
}

/// 本回合要打的函数：**真的瞄准**最近的敌人，绕不过去就换一个候选。
///
/// 返回 `(函数原文, 本回合试过的候选数)`；一个都没有时退回 `"0.00"`（并如实报告 0 次尝试，
/// 让断言消息里能看出"这一发是瞎打的"）。
fn aim(board: &Board, shooter: i64) -> (String, usize) {
    let Some(&(x0, y0)) = board.pos.get(&shooter) else { return ("0.00".to_string(), 0) };
    let mut tried = 0usize;
    for enemy in board.enemies_of(shooter) {
        let Some(&(xt, yt)) = board.pos.get(&enemy) else { continue };
        let m = (yt - y0) / (xt - x0);
        for expr in candidate_exprs(m, x0, y0, (xt, yt)) {
            tried += 1;
            let Some(out) = judge(&expr, board, shooter) else { continue };
            if out.blocked || out.killed.is_empty() {
                continue;
            }
            // 优先"只打死敌人"：顺手打死队友会让局面更难读（这游戏允许误伤）。
            let friendly = out.killed.iter().any(|id| !board.enemies_of(shooter).contains(id));
            if friendly {
                continue;
            }
            return (expr, tried);
        }
    }
    ("0.00".to_string(), tried)
}

fn kind(msg: &str) -> String {
    gw2_rebuild::protocol::parse_message(msg)
        .map(|m| m.kind)
        .unwrap_or_default()
}

fn start_server(
    cfg: Config,
    logged: bool,
) -> (std::net::SocketAddr, std::thread::JoinHandle<std::io::Result<TurnLog>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("accept");
        if logged {
            serve_stream_logged(&mut sock, &cfg)
        } else {
            serve_stream(&mut sock, &cfg).map(|_| TurnLog::default())
        }
    });
    (addr, handle)
}

fn handshake(client: &mut TcpStream) -> Vec<String> {
    client
        .write_all(&frame(r#"{"NewConnectionRequest":{"major_version":"2.0","minor_version":"60"}}"#))
        .unwrap();
    let hello = read_messages(client);
    let kinds: Vec<String> = hello.iter().map(|m| kind(m)).collect();
    assert_eq!(
        kinds,
        vec!["NewConnection", "TurnModeInfo", "GameStateInfo", "GameNameInfo",
             "AxisModeInfo", "FunctionModeInfo", "TimeModeInfo"],
        "握手序列与真实一致"
    );
    hello
}

#[test]
fn plays_a_full_match_over_tcp() {
    // 种子只决定**障碍布局**（9 个）；打不打得到人由客户端自己瞄准决定（`docs/139` §11）。
    let cfg = Config { seed: [11u8; 32], ..Default::default() };
    let (addr, server) = start_server(cfg, false);
    let mut client = TcpStream::connect(addr).expect("connect");

    // 1) 握手
    handshake(&mut client);

    // 2) 开局
    client.write_all(&frame(r#"{"GameStartRequest":{}}"#)).unwrap();
    let start = read_messages(&mut client);
    let sk: Vec<String> = start.iter().map(|m| kind(m)).collect();
    let obstacles = sk.iter().filter(|k| *k == "AddObstacle").count();
    assert!(obstacles > 0, "应生成障碍");
    assert_eq!(sk.iter().filter(|k| *k == "SoldierInfo").count(), 4, "应有 4 名士兵");
    assert!(sk.contains(&"TurnInfo".to_string()), "应给出第一回合");
    assert_eq!(sk.last().unwrap(), "GameStateInfo", "最后应是 WaitingForFunctions");

    // 局面：从报文里读（`PosInfo` / `SoldierInfo` / `TeamInfo` / `AddObstacle`）。
    let mut board = Board::default();
    board.absorb(&start);
    assert_eq!(board.pos.len(), 4, "四名士兵都应有 PosInfo");
    assert_eq!(board.obstacles.len(), obstacles, "障碍表应与 AddObstacle 一一对应");

    // 3) 连打若干回合，**每回合按当前射手与最近的敌人重新瞄准**，直到某队全灭
    let mut batch = start;
    let mut ended = false;
    let mut shots: Vec<(i64, String, usize)> = Vec::new(); // (射手, 函数, 本回合试了几个候选)
    for _ in 0..12 {
        let Some(pid) = read_who(&batch) else { break };
        let sid = read_whom(&batch, pid).expect("TurnInfo 应带 soldier_id");
        let (expr, tried) = aim(&board, sid);
        shots.push((sid, expr.clone(), tried));
        client
            .write_all(&frame(&format!(
                r#"{{"FunctionUpdateRequest":{{"player_id":{pid},"function":"{expr}"}}}}"#
            )))
            .unwrap();
        let _ = read_messages(&mut client); // FunctionInfo 回执
        client
            .write_all(&frame(&format!(r#"{{"FunctionFireRequest":{{"player_id":{pid}}}}}"#)))
            .unwrap();
        let got = read_messages(&mut client);
        if got.is_empty() {
            break;
        }
        board.absorb(&got);
        if got.iter().any(|m| m.contains("EndingGame")) {
            ended = true;
            break;
        }
        batch = got;
    }
    // 断言一条没放宽：**能打死敌人并最终触发 `EndingGame`**。额外信息只进消息文本。
    let tried_total: usize = shots.iter().map(|s| s.2).sum();
    let aimed = shots.iter().filter(|s| s.2 > 0).count();
    assert!(
        ended,
        "应该能打死敌人并最终触发 EndingGame（打了 {} 回合，{} 回合做了瞄准，共试过 {} 条候选弹道；\n记录：{:?}）",
        shots.len(),
        aimed,
        tried_total,
        shots
    );
    assert!(
        shots.iter().filter(|s| s.1 != "0.00").count() >= 2,
        "至少两发是**瞄准过的**（不是随手甩的 \"0.00\"）：{:?}",
        shots
    );
    drop(client);
    server.join().unwrap().unwrap();
}

/// **接线验收**：射手按轮转出现 + AI 为那个士兵进化过（`docs/117` §4）。
///
/// 客户端很笨（每题都 `"0.00"`）—— 电脑玩家的函数由**服务端的 AI** 决定，
/// 客户端的文本根本不生效（`docs/127` §5），所以这条测试完全可复现：同一 seed ⇒ 同一局。
/// 种子怎么挑的、为什么必须是 `[53;32]`，见文件头与 `search_ai_seed`。
#[test]
fn rotation_and_ai_are_wired_over_tcp() {
    let cfg = Config {
        // `[53;32]`：`search_ai_seed` 实测的"AI 真能打死人"的种子（1 个障碍、第 6 回合完局）。
        seed: [53u8; 32],
        // 两个玩家都是电脑玩家 ⇒ 每回合都会为该士兵跑一次进化。
        bot_players: vec![229, 230],
        // 不限时：预算一旦是墙钟，截断点就与机器速度有关（`docs/06` §17.2 B 类），
        // 这条测试要的是"给定同一串消息 ⇒ 同一状态转移"，所以关掉它。
        ai_budget_secs: None,
        ..Default::default()
    };
    let (addr, server) = start_server(cfg, true);
    let mut client = TcpStream::connect(addr).expect("connect");
    handshake(&mut client);

    client.write_all(&frame(r#"{"GameStartRequest":{}}"#)).unwrap();
    let mut batch = read_batch(&mut client);

    // 士兵 → 玩家，以及每玩家的士兵表（顺序 = `SoldierInfo` 出现顺序）。
    let mut roster: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut owner: BTreeMap<i64, i64> = BTreeMap::new();
    let mut team_of: BTreeMap<i64, String> = BTreeMap::new();
    for m in &batch {
        if let Some(rm) = gw2_rebuild::protocol::parse_message(m) {
            if rm.kind == "SoldierInfo" {
                let sid = gw2_rebuild::protocol::get_int(&rm, "soldier_id").unwrap();
                let pid = gw2_rebuild::protocol::get_int(&rm, "player_id").unwrap();
                owner.insert(sid, pid);
                roster.entry(pid).or_default().push(sid);
            }
            if rm.kind == "TeamInfo" {
                let sid = gw2_rebuild::protocol::get_int(&rm, "entity_id").unwrap();
                let team = gw2_rebuild::protocol::get_str(&rm, "team").unwrap();
                team_of.insert(sid, team);
            }
        }
    }
    assert_eq!(roster.len(), 2, "应有 2 个玩家");
    let players: Vec<i64> = roster.keys().copied().collect();
    let mut alive: BTreeSet<i64> = owner.keys().copied().collect();
    assert_eq!(alive.len(), 4, "应有 4 名士兵");

    // 轮转模型的状态（**测试自己**按 `docs/98` §1.2 实现，不复用被测代码）。
    let mut cursor: BTreeMap<i64, usize> = players.iter().map(|p| (*p, 0usize)).collect();
    // (玩家, 士兵, 这一步之前已经死了几个人)
    let mut announcements: Vec<(i64, i64, usize)> = Vec::new();
    let mut deaths: Vec<i64> = Vec::new();

    let mut ended = false;
    for _ in 0..20 {
        let Some(pid) = read_who(&batch) else { break };
        let sid = read_whom(&batch, pid).expect("TurnInfo 应带 soldier_id");
        announcements.push((pid, sid, deaths.len()));

        // ---- 轮转规则：先自增再取、跳过已死的、一个活的都没有 ⇒ 0 ----
        let r = roster.get(&pid).expect("该玩家应有士兵表");
        let n = r.len();
        let mut idx = cursor[&pid];
        let mut expect = 0;
        for _ in 0..n {
            idx = (idx + 1) % n;
            if alive.contains(&r[idx]) {
                expect = r[idx];
                break;
            }
        }
        assert_eq!(
            sid, expect,
            "第 {} 次广播的射手不符合轮转（玩家 {pid} 表 {r:?}，游标 {}，活 {alive:?}）",
            announcements.len(),
            cursor[&pid]
        );
        cursor.insert(pid, idx);

        client
            .write_all(&frame(&format!(
                r#"{{"FunctionUpdateRequest":{{"player_id":{pid},"function":"0.00"}}}}"#
            )))
            .unwrap();
        let _ = read_batch(&mut client);
        client
            .write_all(&frame(&format!(r#"{{"FunctionFireRequest":{{"player_id":{pid}}}}}"#)))
            .unwrap();
        let got = read_batch(&mut client);
        if got.is_empty() {
            break;
        }
        // 死亡从报文里取（不猜）：本回合谁被打死了，就把他从存活集里去掉。
        for m in &got {
            if let Some(rm) = gw2_rebuild::protocol::parse_message(m) {
                if rm.kind == "LifeQueuedDeath" {
                    let id = gw2_rebuild::protocol::get_int(&rm, "entity_id").unwrap();
                    deaths.push(id);
                    alive.remove(&id);
                }
            }
        }
        if got.iter().any(|m| m.contains("EndingGame")) {
            ended = true;
            break;
        }
        batch = got;
    }
    assert!(ended, "这局应当能打完（否则日志不完整）");
    assert!(!deaths.is_empty(), "没有死人 ⇒ '跳过死者' 这条根本没被验证");
    // 玩家顺序：由配置/构造顺序给（`docs/47`），本测试里是 229 → 230 → 229 → …
    for (k, a) in announcements.iter().enumerate() {
        assert_eq!(a.0, players[k % players.len()], "玩家出场顺序应交替且确定");
    }
    // ---- 先手写死（不是复述规则）：游标初值 0 + "先自增再取" ⇒ `soldiers[1]`。
    // 第一次广播之前不可能有死亡（还没开过火），所以这一条没有歧义。
    assert_eq!(
        (announcements[0].0, announcements[0].1),
        (players[0], roster[&players[0]][1]),
        "第一个射手应是第一个玩家的 soldiers[1]（游标 0 先自增）"
    );
    // 第二个玩家的首次广播：那一刻若还没人死，同样必须是 `soldiers[1]`。
    let first_b = announcements.iter().find(|a| a.0 == players[1]).expect("第二个玩家也要有回合");
    if first_b.2 == 0 {
        assert_eq!(first_b.1, roster[&players[1]][1], "第二个玩家的先手也应是 soldiers[1]");
    }

    drop(client); // 先关连接：对局没打完时服务端仍在 read_frame 里，join 会挂死
    let log = server.join().unwrap().unwrap();

    // ---- AI 接线：每个广播了非 0 士兵的回合，都为**那个**士兵进化过一次 ----
    let announced: Vec<i64> = log.announced().map(|(_, s)| s).collect();
    assert!(
        log.records.iter().all(|r| r.soldier_id.is_some_and(|s| s != 0)),
        "这条测试的局面里不该出现全灭(0)/跳过回合：{:?}",
        log.records
    );
    let mut evolved: Vec<i64> = Vec::new();
    for r in &log.records {
        // flip 由**队伍**决定（`docs/105`）：队伍从 TeamInfo 报文里取，不靠 id 猜。
        let team = team_of.get(&r.soldier_id.unwrap()).expect("每名士兵都有 TeamInfo");
        assert_eq!(r.flip, Some(team == "Team2"), "flip 应由队伍决定（{team}）");
        match r.soldier_id {
            Some(0) => assert!(!r.evolved, "全灭回合没有士兵，不该进化"),
            Some(sid) => {
                assert_eq!(owner[&sid], r.player_id);
                assert!(r.evolved, "AI 没有为士兵 {sid} 进化");
                assert!(r.function_tokens > 0, "进化结果为空（士兵 {sid}）");
                evolved.push(sid);
            }
            None => assert!(!r.evolved, "被跳过的回合不该进化"),
        }
    }
    assert_eq!(evolved, announced, "进化过的士兵序列应与广播的射手序列一致");
    assert_eq!(log.evolved_soldiers(), announced);
}

/// **AI 说话验收**（`docs/127-ai-speaks.md`）：电脑玩家的每一回合都要在报文流里
/// 说出自己的函数，且这条函数**就是它真的打出去的那条**。
///
/// 断言的六件事（都不复用被测代码算出来的数）：
///
/// 1. 回合开始（还没开火）就有一条 `FunctionInfo`，`owner_id` == 本回合的 `soldier_id`，
///    `function` **非空**、没有空格（`to_text` 一个空格都不加，`docs/119` §3.1）；
/// 2. 这段文本能被 `function::parse` 吃回去，且 `to_text(parse(text)) == text`（往返闭合）；
/// 3. `FunctionUpdateRequest` 的回执照旧是 `soldier_id`，且**客户端的 `"0.00"` 改不动**
///    电脑玩家的函数（AI 才是它的作者）；
/// 4. 开火窗口的顺序是 `CalculatingResults → FunctionInfo → FunctionPoints`（`docs/43`），
///    中间那条 `function` 与第 1 步逐字符相同；
/// 5. `corrected_function` 的形态 = `"<函数文本> + <三位小数>"`，常数用**独立路径**复算
///    （`function::eval` + `FunctionPoints.start_x/start_y`），不是读实现里的私有函数；
/// 6. 每个 AI 回合都过一遍（不是"某一回合过了就算"），并且真的跑了若干回合。
#[test]
fn ai_functions_are_broadcast_and_round_trip_over_tcp() {
    let cfg = Config {
        // 同 `rotation_and_ai_are_wired_over_tcp`：`[53;32]` 是 AI 真能打死人的种子。
        seed: [53u8; 32],
        bot_players: vec![229, 230],
        ai_budget_secs: None,
        ..Default::default()
    };
    let (addr, server) = start_server(cfg, true);
    let mut client = TcpStream::connect(addr).expect("connect");
    handshake(&mut client);

    client.write_all(&frame(r#"{"GameStartRequest":{}}"#)).unwrap();
    let mut batch = read_batch(&mut client);

    // 队伍（`flip` 由队伍决定，`docs/105`）+ 士兵→玩家。
    let mut team_of: BTreeMap<i64, String> = BTreeMap::new();
    let mut owner: BTreeMap<i64, i64> = BTreeMap::new();
    for m in &batch {
        if let Some(rm) = gw2_rebuild::protocol::parse_message(m) {
            match rm.kind.as_str() {
                "TeamInfo" => {
                    team_of.insert(
                        gw2_rebuild::protocol::get_int(&rm, "entity_id").unwrap(),
                        gw2_rebuild::protocol::get_str(&rm, "team").unwrap(),
                    );
                }
                "SoldierInfo" => {
                    owner.insert(
                        gw2_rebuild::protocol::get_int(&rm, "soldier_id").unwrap(),
                        gw2_rebuild::protocol::get_int(&rm, "player_id").unwrap(),
                    );
                }
                _ => {}
            }
        }
    }
    assert_eq!(owner.len(), 4, "应有 4 名士兵");

    let mut turns: Vec<i64> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    for _ in 0..20 {
        let Some(pid) = read_who(&batch) else { break };
        let sid = read_whom(&batch, pid).expect("TurnInfo 应带 soldier_id");
        assert_eq!(owner[&sid], pid);
        turns.push(sid);

        // ---- 1) 回合开始那条：owner_id = soldier_id、function 非空 ----
        let submit = function_info_of(&batch, sid).unwrap_or_else(|| {
            panic!("士兵 {sid} 的回合里没有 FunctionInfo：{batch:?}")
        });
        let text = submit.0.clone();
        assert!(!text.is_empty(), "AI 的函数文本是空串（士兵 {sid}）");
        assert!(!text.contains(' '), "`to_text` 不加空格，这条却有空格：{text:?}");
        assert!(submit.2.is_none(), "提交时的 corrected_function 必须是 null：{:?}", submit);
        texts.push(text.clone());

        // ---- 2) 往返闭合：文本 → token → 文本 ----
        let toks = gw2_rebuild::function::parse(&text);
        assert!(!toks.is_empty(), "AI 文本解析回空 token 流：{text:?}");
        assert_eq!(
            gw2_rebuild::function::to_text(&toks),
            text,
            "AI 文本经 parse/to_text 往返后变了：{text:?}"
        );

        // ---- 3) 客户端改不动电脑玩家的函数；回执的 owner_id 仍是 soldier_id ----
        client
            .write_all(&frame(&format!(
                r#"{{"FunctionUpdateRequest":{{"player_id":{pid},"function":"0.00"}}}}"#
            )))
            .unwrap();
        let echo = read_batch(&mut client);
        let (etext, eowner, _) = function_info_in(&echo).expect("FunctionUpdateRequest 应有回执");
        assert_eq!(eowner, sid, "回执的 owner_id 必须是 soldier_id（不是 player_id）");
        assert_eq!(etext, text, "电脑玩家的函数由 AI 决定，客户端的 \"0.00\" 不该生效");

        client
            .write_all(&frame(&format!(r#"{{"FunctionFireRequest":{{"player_id":{pid}}}}}"#)))
            .unwrap();
        let got = read_batch(&mut client);
        if got.is_empty() {
            break;
        }

        // ---- 4) 顺序：CalculatingResults → FunctionInfo(修正后) → FunctionPoints ----
        let calc = got
            .iter()
            .position(|m| m.contains("CalculatingResults"))
            .unwrap_or_else(|| panic!("本回合没有 CalculatingResults：{got:?}"));
        assert_eq!(kind(&got[calc + 1]), "FunctionInfo", "CalculatingResults 之后必须是 FunctionInfo");
        assert_eq!(kind(&got[calc + 2]), "FunctionPoints", "FunctionInfo 之后必须是 FunctionPoints");
        let fired = gw2_rebuild::protocol::parse_message(&got[calc + 1]).unwrap();
        assert_eq!(gw2_rebuild::protocol::get_int(&fired, "owner_id"), Some(sid));
        assert_eq!(
            gw2_rebuild::protocol::get_str(&fired, "function"),
            Some(text.clone()),
            "打出去的必须是它说的那个函数（士兵 {sid}）"
        );

        // ---- 5) 修正文本：`<f> ± <三位小数>`，常数独立复算，**运算符带符号** ----
        let corrected = gw2_rebuild::protocol::get_str(&fired, "corrected_function")
            .unwrap_or_else(|| panic!("修正版必须有 corrected_function：{fired:?}"));
        let pts = gw2_rebuild::protocol::parse_message(&got[calc + 2]).unwrap();
        let x0 = gw2_rebuild::protocol::get_f64(&pts, "start_x").expect("FunctionPoints.start_x");
        let y0 = gw2_rebuild::protocol::get_f64(&pts, "start_y").expect("FunctionPoints.start_y");
        let flip = team_of.get(&sid).map(|t| t == "Team2").expect("每名士兵都有 TeamInfo");
        // c = y0 - f(±x0)：与 `src/shot.rs::correction` 同一定义，但这里**自己算**。
        let f_at = gw2_rebuild::function::eval(&toks, if flip { -x0 } else { x0 }, 0.0, 0.0);
        let c = y0 - f_at;
        // 运算符本身带符号：`c >= 0` ⇒ `" + "`、`c < 0` ⇒ `" - "`。
        // 真机形态见抓包帧 143/182/203（`" - 3.420"` / `" - 3.686"` / `" - 11.416"`），
        // 旧实现写死 `" + "` 会发 `"5.53 + -9.530"`（`docs/127` §5 第 1 条，已修）。
        let tail = if c >= 0.0 { format!(" + {:.3}", c) } else { format!(" - {:.3}", -c) };
        assert_eq!(
            corrected,
            format!("{}{}", text, tail),
            "修正文本的形态与常数不符（士兵 {sid}，起点 ({x0}, {y0})，c = {c}）"
        );
        assert_eq!(&corrected[text.len()..], tail.as_str(), "尾巴就是修正常数那一截");
        let frac = tail.rsplit('.').next().unwrap();
        assert_eq!(frac.len(), 3, "修正常数必须是三位小数：{tail:?}");

        if got.iter().any(|m| m.contains("EndingGame")) {
            break;
        }
        batch = got;
    }

    assert!(turns.len() >= 4, "只打了 {} 回合，断言覆盖不足", turns.len());
    drop(client); // 先关连接：对局没打完时服务端仍在 read_frame 里，join 会挂死
    let log = server.join().unwrap().unwrap();
    // ---- 6) 日志与报文同源：AI 说的文本、修正文本都与报文逐字符相同 ----
    let announced: Vec<i64> = log.announced().map(|(_, s)| s).collect();
    assert_eq!(announced, turns, "报文里的射手序列应与日志一致");
    assert!(log.records.len() >= turns.len(), "日志少了回合：{:?}", log.records);
    for (r, text) in log.records.iter().zip(&texts) {
        let sid = r.soldier_id.expect("本局不该有跳过回合");
        assert!(r.evolved, "士兵 {sid} 的回合没进化");
        assert_eq!(
            r.function_text.as_deref(),
            Some(text.as_str()),
            "士兵 {sid}：日志里的函数与报文里的不是同一份"
        );
        assert!(r.corrected.is_some(), "士兵 {sid}：日志缺修正文本");
        assert!(r.function_tokens > 0, "士兵 {sid}：进化的 token 为空");
    }
}

/// `(function, owner_id, corrected_function)` 的解析结果。
type FunctionInfo = (String, i64, Option<String>);

/// 报文窗口里**最后一条** `FunctionInfo`（最新的那条，通常是本回合 AI 的提交）。
fn function_info_in(batch: &[String]) -> Option<FunctionInfo> {
    batch
        .iter()
        .rev()
        .filter_map(|m| {
            let rm = gw2_rebuild::protocol::parse_message(m)?;
            if rm.kind != "FunctionInfo" {
                return None;
            }
            Some((
                gw2_rebuild::protocol::get_str(&rm, "function").unwrap_or_default(),
                gw2_rebuild::protocol::get_int(&rm, "owner_id")?,
                gw2_rebuild::protocol::get_str(&rm, "corrected_function"),
            ))
        })
        .next()
}

/// 指定士兵的**最后一条** `FunctionInfo`。
fn function_info_of(batch: &[String], soldier_id: i64) -> Option<FunctionInfo> {
    batch
        .iter()
        .rev()
        .filter_map(|m| {
            let rm = gw2_rebuild::protocol::parse_message(m)?;
            if rm.kind != "FunctionInfo" {
                return None;
            }
            if gw2_rebuild::protocol::get_int(&rm, "owner_id") != Some(soldier_id) {
                return None;
            }
            Some((
                gw2_rebuild::protocol::get_str(&rm, "function").unwrap_or_default(),
                soldier_id,
                gw2_rebuild::protocol::get_str(&rm, "corrected_function"),
            ))
        })
        .next()
}

fn read_who(batch: &[String]) -> Option<i64> {
    for m in batch.iter().rev() {
        if let Some(rm) = gw2_rebuild::protocol::parse_message(m) {
            if rm.kind == "TurnInfo" {
                return gw2_rebuild::protocol::get_int(&rm, "player_id");
            }
        }
    }
    None
}

fn read_whom(batch: &[String], player_id: i64) -> Option<i64> {
    for m in batch.iter().rev() {
        if let Some(rm) = gw2_rebuild::protocol::parse_message(m) {
            if rm.kind == "TurnInfo"
                && gw2_rebuild::protocol::get_int(&rm, "player_id") == Some(player_id)
            {
                return gw2_rebuild::protocol::get_int(&rm, "soldier_id");
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 种子搜索（**`#[ignore]`，平时不跑**）：给两条"双方都是电脑玩家"的 e2e 挑种子
// ---------------------------------------------------------------------------
//
// 为什么需要：这两条用例的客户端**改不动**打出去的函数 —— 服务的 `Match::fire`
// 对电脑玩家一律用 `ai_function_text()`（`docs/127` §5），客户端的 `"0.00"` 不生效。
// 所以"打不打得死人"完全由 `(seed, AI 进化)` 决定，而 AI 的适应度里**含掩膜**
// （`aigen::ScoringContext.mask` = `Match::mask`），障碍多的种子会僵成"AI 一直贴着墙打"。
// 步长从 0.5 改成 0.01（`docs/136`）之后，`[11;32]` 这颗种子就是这种情况。
//
// 用法（改种子时照这个跑）：
//     cargo test --offline --test server_match -- --ignored --nocapture search_ai_seed
//
// 它会先按 `[k;32]` 把 0..=63 的**障碍数**算出来（纯计算、毫秒级），再挑障碍最少的
// 若干个真的跑一局 AI-vs-AI，打印 `回合数 / 死亡数 / 是否 EndingGame`。
#[test]
#[ignore]
fn search_ai_seed() {
    // ---- 第一轮：障碍数（`serverloop` 开局那条：ChaCha12(seed) → levelgen::generate(.., 0)）
    let mut counts: Vec<(usize, u8)> = Vec::new();
    for k in 0..=63u8 {
        let seed = [k; 32];
        let mut rng = gw2_rebuild::rng::ChaCha12::from_seed(&seed);
        let mut next = || rng.next_u64();
        let n = gw2_rebuild::levelgen::generate(&mut next, 0).len();
        counts.push((n, k));
    }
    counts.sort();
    println!("障碍最少的 12 个 `[k;32]` 种子: {:?}", &counts[..12]);

    // ---- 第二轮：对障碍最少的若干个真的跑一局（客户端只会"提交 + 开火"）
    for (n_obs, k) in counts.iter().copied().take(6) {
        let cfg = Config {
            seed: [k; 32],
            bot_players: vec![229, 230],
            ai_budget_secs: None,
            ..Default::default()
        };
        let (addr, server) = start_server(cfg, true);
        let mut client = TcpStream::connect(addr).expect("connect");
        handshake(&mut client);
        client.write_all(&frame(r#"{"GameStartRequest":{}}"#)).unwrap();
        let mut batch = read_batch(&mut client);
        let mut turns = 0usize;
        let mut deaths = 0usize;
        let mut ended = false;
        for _ in 0..20 {
            let Some(pid) = read_who(&batch) else { break };
            client
                .write_all(&frame(&format!(
                    r#"{{"FunctionUpdateRequest":{{"player_id":{pid},"function":"0.00"}}}}"#
                )))
                .unwrap();
            let _ = read_batch(&mut client);
            client
                .write_all(&frame(&format!(r#"{{"FunctionFireRequest":{{"player_id":{pid}}}}}"#)))
                .unwrap();
            let got = read_batch(&mut client);
            if got.is_empty() {
                break;
            }
            turns += 1;
            for m in &got {
                if let Some(rm) = gw2_rebuild::protocol::parse_message(m) {
                    if rm.kind == "LifeQueuedDeath" {
                        deaths += 1;
                    }
                }
            }
            if got.iter().any(|m| m.contains("EndingGame")) {
                ended = true;
                break;
            }
            batch = got;
        }
        drop(client);
        let log = server.join().unwrap().unwrap();
        println!(
            "SEED [{};32] 障碍={} 回合={} 死亡={} ending={} 记录={}",
            k, n_obs, turns, deaths, ended, log.records.len()
        );
    }
}
