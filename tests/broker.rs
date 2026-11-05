//! 端到端：一台真正的 broker + 两条普通 TCP 连接（模拟房主与浏览客）。
//!
//! 判据来自真实抓包 `fixtures/protocol_match.jsonl` 第 83–86 行：
//! 连上 → 报 `VersionInfo` → broker 逐条推 `GameInfo`。
//! 这里不需要真游戏，也不需要真二进制：协议本身就是被测对象。
use gw2_rebuild::broker::{
    game_info_event, game_removed_event, spawn, Broker, Config, RoomTable,
};
use gw2_rebuild::protocol::{parse_message, Broadcast, Frame};

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn sample(addr: &str, players: i64, state: &str) -> Broadcast {
    Broadcast {
        room_name: "Graphwar Game".into(),
        game_state: state.into(),
        current_player_count: players,
        address: addr.into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
        locked: false,
    }
}

fn connect(addr: &str) -> TcpStream {
    let s = TcpStream::connect(addr).expect("连 broker");
    s.set_read_timeout(Some(Duration::from_millis(1500))).ok();
    s.set_nodelay(true).ok();
    s
}

fn send(sock: &mut TcpStream, msg: &str) {
    sock.write_all(&Frame { payload: msg.into() }.encode()).expect("写一帧");
}

/// 读若干条消息（按 u16 大端长度切帧），最多等到超时。
fn recv_msgs(sock: &mut TcpStream, want: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    while out.len() < want {
        match sock.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
        while let Some((f, used)) = Frame::decode(&buf) {
            buf.drain(..used);
            out.push(f.payload);
            if out.len() >= want {
                break;
            }
        }
    }
    out
}

fn handshake(sock: &mut TcpStream) {
    send(sock, "{\"VersionInfo\":{\"version\":\"2.0.60\",\"system\":\"linux\"}}");
}

#[test]
fn client_lists_our_brokers_rooms() {
    // 用 :0 拿随机端口，然后后台跑 broker。
    let b = Broker::bind(&Config { addr: "127.0.0.1:0".into(), ..Default::default() }).unwrap();
    let addr = b.addr.to_string();
    std::thread::spawn(move || {
        let _ = b.serve(Config { addr: "127.0.0.1:0".into(), ..Default::default() });
    });

    // 1) 「房主」连上来登记一个房间。
    let mut host = connect(&addr);
    handshake(&mut host);
    // 握手后 broker 会把（空）房间表推回来，这里不用管。
    let _ = recv_msgs(&mut host, 1);
    let room = sample("[::1]:35245", 2, "Setup");
    send(&mut host, &game_info_event(&room));

    // 2) 「浏览客」连上来：立刻应该收到这个房间。
    let mut guest = connect(&addr);
    handshake(&mut guest);
    let got = recv_msgs(&mut guest, 1);
    assert_eq!(got.len(), 1, "必须收到一条房间，实际 {:?}", got);
    assert!(got[0].starts_with("{\"GameInfo\":{\"game_info\":{"));

    // 行的内容与房主登记的一致（键序/取值逐位）。
    let raw = parse_message(&got[0]).unwrap();
    assert_eq!(raw.kind, "GameInfo");
    let info = gw2_rebuild::broker::parse_game_info_event(&got[0]).unwrap();
    assert_eq!(info, room);
}

#[test]
fn room_updates_and_removals_reach_connected_clients() {
    let broker = spawn(Config { addr: "127.0.0.1:0".into(), ..Default::default() }).unwrap();
    let addr = broker.addr.to_string();

    let mut host = connect(&addr);
    handshake(&mut host);
    let _ = recv_msgs(&mut host, 1);

    let mut guest = connect(&addr);
    handshake(&mut guest);
    let _ = recv_msgs(&mut guest, 1); // 空表快照

    // 登记 → 浏览客收到
    send(&mut host, &game_info_event(&sample("1.2.3.4:35981", 0, "Setup")));
    let got = recv_msgs(&mut guest, 1);
    assert_eq!(got.len(), 1);
    assert!(got[0].contains("\"current_player_count\":0"));

    // 同一地址的更新（人数变化）→ 浏览客收到新的整条记录
    send(&mut host, &game_info_event(&sample("1.2.3.4:35981", 2, "ExecutingResults")));
    let got = recv_msgs(&mut guest, 1);
    assert_eq!(got.len(), 1);
    assert!(got[0].contains("\"current_player_count\":2"), "{}", got[0]);
    assert!(got[0].contains("\"game_state\":\"ExecutingResults\""));

    // 显式移除 → 浏览客收到 GameRemoved
    send(&mut host, &game_removed_event("1.2.3.4:35981"));
    let got = recv_msgs(&mut guest, 1);
    assert_eq!(got, vec!["{\"GameRemoved\":{\"game_address\":\"1.2.3.4:35981\"}}".to_string()]);
}

#[test]
fn host_disconnect_removes_its_rooms() {
    let broker = spawn(Config { addr: "127.0.0.1:0".into(), ..Default::default() }).unwrap();
    let addr = broker.addr.to_string();

    let mut guest = connect(&addr);
    handshake(&mut guest);
    let _ = recv_msgs(&mut guest, 1);

    {
        let mut host = connect(&addr);
        handshake(&mut host);
        let _ = recv_msgs(&mut host, 1);
        send(&mut host, &game_info_event(&sample("5.6.7.8:40000", 1, "Setup")));
        let got = recv_msgs(&mut guest, 1);
        assert_eq!(got.len(), 1, "房主登记后浏览客应看到房间");
        // host 在这里掉线
    }

    // 连接断开 = 房间消失（长连接注册，**推断**）。
    let got = recv_msgs(&mut guest, 1);
    assert_eq!(
        got,
        vec!["{\"GameRemoved\":{\"game_address\":\"5.6.7.8:40000\"}}".to_string()],
        "房主掉线后应收到 GameRemoved"
    );
}

#[test]
fn room_table_expires_silent_rooms() {
    // 不需要网络：直接测过期逻辑（真实 TTL 未知，见 docs/115）。
    let mut t = RoomTable::new();
    t.upsert(sample("a:1", 0, "Setup"), 1);
    assert_eq!(t.expire(Duration::from_secs(0)), vec!["a:1".to_string()]);
    assert!(t.is_empty());
}
