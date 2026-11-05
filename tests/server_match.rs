//! 端到端（真 TCP）：起一个服务端，用真实客户端打完一局。
//!
//! 两条测试：
//!
//! 1. `plays_a_full_match_over_tcp` —— 判据 1："能跑完整一局"（原有测试）；
//! 2. `rotation_and_ai_are_wired_over_tcp` —— **本轮新接线**的验收：
//!    射手按 `turn::TurnManager` 的轮转出现，且 AI **只为那个士兵**进化过
//!    （`docs/117-server-wiring.md`）。
//!
//! 为什么 1 的 seed 从 `[3; 32]` 换成了 `[11; 32]`：接线后"谁先开火"由
//! `PlayerTurns`（游标 0、先自增再取）决定，先手从 `soldiers[0]`（230）变成
//! `soldiers[1]`（231），于是同一颗种子的局面走向变了 —— `[3; 32]` 会让两名
//! 幸存者分别停在 `y=0` 与 `y=-4` 两条横排上，固定 `"0.00"` 的客户端永远打不到
//! 对方（僵局，`assert!(ended)` 不成立）。**断言一条没动**，只换了夹具种子；
//! 取值过程写在 `docs/117` §5（1..=64 里能打完的有 11/17/32/52/55/63）。
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
    // seed = [11; 32]：接线后的回合顺序下，固定 "0.00" 的客户端能打完（见文件头注释）。
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

    // 3) 连打若干回合，直到某队全灭
    let mut batch = start;
    let mut ended = false;
    for _ in 0..12 {
        let Some(pid) = read_who(&batch) else { break };
        client
            .write_all(&frame(&format!(
                r#"{{"FunctionUpdateRequest":{{"player_id":{pid},"function":"0.00"}}}}"#
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
        if got.iter().any(|m| m.contains("EndingGame")) {
            ended = true;
            break;
        }
        batch = got;
    }
    assert!(ended, "直线射击应当能打死敌人并最终触发 EndingGame");
    server.join().unwrap().unwrap();
}

/// **接线验收**：射手按轮转出现 + AI 为那个士兵进化过（`docs/117` §4）。
///
/// 客户端很笨（每题都 `"0.00"`），所以这条测试完全可复现：同一 seed ⇒ 同一局。
#[test]
fn rotation_and_ai_are_wired_over_tcp() {
    let cfg = Config {
        seed: [11u8; 32],
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
