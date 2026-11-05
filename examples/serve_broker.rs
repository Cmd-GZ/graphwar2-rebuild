//! 跑一台 broker：`cargo run --offline --example serve_broker -- 127.0.0.1:42970`
//!
//! 端口 42970 是游戏 `--broker_address` 帮助文本里的示例值（观测，见 docs/115）。
//! 想用原版客户端连它：**只在本机**，并把客户端指到 `127.0.0.1:42970`。
use gw2_rebuild::broker::{spawn, Config, DEFAULT_PORT};
use std::time::Duration;

fn main() {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("0.0.0.0:{}", DEFAULT_PORT));
    let b = spawn(Config { addr, ..Default::default() }).expect("绑定失败");
    println!("broker listening on {}", b.addr);
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
