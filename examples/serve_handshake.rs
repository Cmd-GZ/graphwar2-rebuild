//! 一个最小可用的 TCP 服务端：收到任意一帧后，回一整套状态同步。
//!
//! ```bash
//! cargo run --quiet --example serve_handshake -- 127.0.0.1:39999
//! ```
//!
//! 然后用任意客户端连上去发一帧，应收到 7 帧（顺序见 `server::on_new_connection`）。
use std::io::{Read, Write};
use std::net::TcpListener;

fn main() {
    let addr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:39999".to_string());
    let listener = TcpListener::bind(&addr).expect("bind");
    println!("LISTENING {}", addr);
    let (mut sock, peer) = listener.accept().expect("accept");
    println!("ACCEPTED {}", peer);

    // 读一帧（先 2 字节长度，再负载）
    let mut hdr = [0u8; 2];
    if sock.read_exact(&mut hdr).is_err() {
        eprintln!("READ_ERR");
        return;
    }
    let len = u16::from_be_bytes(hdr) as usize;
    let mut body = vec![0u8; len];
    if sock.read_exact(&mut body).is_err() {
        eprintln!("READ_ERR");
        return;
    }
    println!("GOT {}", String::from_utf8_lossy(&body));

    let state = gw2_rebuild::server::RoomState {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
    };
    let msgs = gw2_rebuild::server::on_new_connection(&state, 228);
    let wire = gw2_rebuild::server::to_wire(&msgs);
    sock.write_all(&wire).expect("write");
    sock.flush().ok();
    println!("SENT {} frames, {} bytes", msgs.len(), wire.len());
}
