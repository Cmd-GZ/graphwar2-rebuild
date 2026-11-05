//! 端到端：起一个服务端，用真实 TCP 客户端打完一局（判据 1）。
use gw2_rebuild::serverloop::{serve_stream, Config};
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

fn kind(msg: &str) -> String {
    gw2_rebuild::protocol::parse_message(msg)
        .map(|m| m.kind)
        .unwrap_or_default()
}

#[test]
fn plays_a_full_match_over_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    let cfg = Config { addr: addr.to_string(), seed: [3u8; 32], ..Default::default() };

    let server = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("accept");
        serve_stream(&mut sock, &cfg)
    });

    let mut client = TcpStream::connect(addr).expect("connect");

    // 1) 握手
    client.write_all(&frame(r#"{"NewConnectionRequest":{"major_version":"2.0","minor_version":"60"}}"#)).unwrap();
    let hello = read_messages(&mut client);
    let kinds: Vec<String> = hello.iter().map(|m| kind(m)).collect();
    assert_eq!(
        kinds,
        vec!["NewConnection", "TurnModeInfo", "GameStateInfo", "GameNameInfo",
             "AxisModeInfo", "FunctionModeInfo", "TimeModeInfo"],
        "握手序列与真实一致"
    );

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
