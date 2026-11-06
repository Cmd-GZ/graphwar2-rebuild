//! 起一个服务端并跑一局。
//!
//! ```bash
//! cargo run --quiet --example serve_match -- 127.0.0.1:39997
//! ```
use gw2_rebuild::graphwar_game_server::{run, Config};

fn main() {
    let addr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:0".to_string());
    let cfg = Config { addr, ..Default::default() };
    match run(cfg) {
        Ok(a) => println!("SERVER_DONE {}", a),
        Err(e) => {
            eprintln!("SERVER_ERR {}", e);
            std::process::exit(1);
        }
    }
}
