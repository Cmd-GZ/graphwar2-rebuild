//! 端到端：一台真正的 broker + 两条普通 TCP 连接（模拟房主与浏览客）。
//!
//! 判据来自真实抓包 `fixtures/protocol_match.jsonl` 第 83–86 行：
//! 连上 → 报 `VersionInfo` → broker 逐条推 `GameInfo`。
//! 这里不需要真游戏，也不需要真二进制：协议本身就是被测对象。
use gw2_rebuild::_scaffold::broker::{
    game_info_event, game_removed_event, spawn, Broker, Config, RoomTable,
};
use gw2_rebuild::_scaffold::protocol::{parse_message, Broadcast, Frame};

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn sample(addr: &str, players: u32, state: &str) -> Broadcast {
    Broadcast {
        room_name: "Graphwar Game".into(),
        game_state: gw2_rebuild::turnstate::GameState::from_str(state).expect("合法状态名"),
        current_player_count: players,
        address: addr.into(),
        axis_mode: gw2_rebuild::graphwar_mode_manager::AxisMode::EveryUnit,
        function_mode: gw2_rebuild::graphwar_mode_manager::FunctionMode::NormalFunction,
        turn_mode: gw2_rebuild::graphwar_turn_manager::TurnMode::SequentialTurns,
        time_mode: gw2_rebuild::graphwar_mode_manager::TimeMode::Timer1m,
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
    let info = gw2_rebuild::_scaffold::broker::parse_game_info_event(&got[0]).unwrap();
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

/// 房间表过期：TTL 一到，静默房间被清掉。
///
/// ## 口径（T73 / 计划 B03）：**换不了，只改注释**
///
/// 这条在 T37 审计里是 `U` 档（判不出）。本轮查清后判**换不了**，理由是结构性的：
///
/// * **broker 服务端是原版没有的**（`AGENTS.md` §4：`_scaffold/` 装的就是"原版没有对应物"
///   的东西：GDExtension 入口、C ABI、报文装配、**broker 服务端**…）。原版客户端只做
///   「连 broker → 收 `GameInfo`/`GameRemoved`」，**房间表的 TTL 在服务端**，客户端二进制里
///   不存在这段逻辑 ⇒ 没有任何物证可对拍。
/// * 唯一相关的二进制常量是 **LAN 广播**的房间条目 TTL：`graphwar_lan_broadcaster_v6::
///   ENTRY_TTL_SECS = 10`（arm64 `0x3de6e4` `cmp x0,#0xa` + `b.lo`，见 T42 / `docs/06` §92），
///   广播周期 1 s（`0x3de7c8`–`0x3de7d4`）。那是**另一个机制**（UDP 多播的 LAN 房间列表），
///   **不能**挪来当 broker 的 TTL —— 两者协议不同（TCP broker vs UDP LAN）、生命周期不同。
///
/// ⇒ 本测试只证 `RoomTable::expire` 的**契约**（TTL 0 ⇒ 立刻过期；表随之清空），
/// **不构成复原证据**。`RoomTable::room_ttl` 的默认 120 s 是**我方推断值**（`broker.rs` 里
/// 已标注），原版真值**未知**。
#[test]
fn room_table_expires_silent_rooms() {
    // 不需要网络：直接测过期逻辑。⚠️ TTL 的真值**未知**（broker 服务端原版没有对应物）。
    let mut t = RoomTable::new();
    t.upsert(sample("a:1", 0, "Setup"), 1);
    assert_eq!(t.expire(Duration::from_secs(0)), vec!["a:1".to_string()]);
    assert!(t.is_empty());
}
