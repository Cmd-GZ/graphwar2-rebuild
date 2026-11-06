//! `server_broker_request_connection` 的验收：**房主侧 broker 连接的读侧**。
//!
//! ⚠️ 用 `#[path]` 把源码编进本测试二进制 ⇒ **不需要** `src/lib.rs` 加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `one_pass_drains_every_frame` | `process` 把已到的整帧全部榨出 | **观测**（`0x3ea160` 循环） |
//! | `partial_is_idle` | 半帧 ⇒ 停，不算错 | **观测**（`0x3ea160` + stream_buffer 的 `Ok(None)`） |
//! | `closed_is_failed` | 对端关 ⇒ 错误（不静默） | **观测**（`0x3ea1c0 Error::kind`） |
//! | `version_info_first_frame` | 第一帧 `{"VersionInfo":…}` + u16 大端长度 | **推断**（抓包一侧） |
//! | `identity` | 模块路径 / 类型名 / Location / 尺寸比闸 | **观测** |
#![allow(dead_code)]

#[path = "../src/_scaffold/protocol.rs"]
mod protocol_src;

mod _scaffold {
    pub mod protocol {
        pub use crate::protocol_src::*;
    }
}

#[path = "../src/graphwar_stream_buffer.rs"]
mod graphwar_stream_buffer;
#[path = "../src/server_broker_request_connection.rs"]
mod server_broker_request_connection;

use graphwar_stream_buffer::{GraphwarStreamBuffer, ReadError};
use server_broker_request_connection::*;

fn framed(payloads: &[&str]) -> Vec<u8> {
    let mut w = Vec::new();
    for p in payloads {
        GraphwarStreamBuffer::new(&mut w)
            .write_event(p.as_bytes())
            .unwrap();
    }
    w
}

#[test]
fn one_pass_drains_every_frame() {
    let wire = framed(&[
        "{\"GameInfo\":{\"game_info\":{}}}",
        "{\"GameInfo\":{\"game_info\":{}}}",
        "{\"GameRemoved\":{\"game_address\":\"[::1]:1\"}}",
    ]);
    let mut c = ServerBrokerRequestConnectionClient::new(&wire[..]);
    assert_eq!(c.process(), Drain::Events(3));
    assert_eq!(c.events.len(), 3);
    assert_eq!(c.events[0], b"{\"GameInfo\":{\"game_info\":{}}}".to_vec());
    // take 之后自己留空
    assert_eq!(c.take_events().len(), 3);
    assert!(c.events.is_empty());
}

#[test]
fn partial_is_idle() {
    let wire = framed(&["{\"GameInfo\":{}}"]);
    let short = &wire[..wire.len() - 3];
    let mut c = ServerBrokerRequestConnectionClient::new(short);
    assert_eq!(c.process(), Drain::Idle);
    assert!(c.events.is_empty());
    assert!(Drain::Idle.is_idle());
    // 空流 ⇒ 0 字节 peek ⇒ ConnectionLost（**不是** Idle）
    let mut e = ServerBrokerRequestConnectionClient::new(&[][..]);
    assert_eq!(e.process(), Drain::Failed(ReadError::ConnectionLost));
}

#[test]
fn closed_is_failed() {
    let wire = framed(&["{}"]);
    let mut c = ServerBrokerRequestConnectionClient::new(&wire[..]);
    assert_eq!(c.process(), Drain::Events(1));
    // 消费完再问 ⇒ 对端关闭
    assert_eq!(c.process(), Drain::Failed(ReadError::ConnectionLost));
}

#[test]
fn version_info_first_frame() {
    assert_eq!(
        version_info_frame("2.0.60", "linux"),
        "{\"VersionInfo\":{\"version\":\"2.0.60\",\"system\":\"linux\"}}"
    );
    let f = version_info_frame(VERSION, "linux");
    let mut w = Vec::new();
    GraphwarStreamBuffer::new(&mut w)
        .write_event(f.as_bytes())
        .unwrap();
    assert_eq!(u16::from_be_bytes([w[0], w[1]]) as usize, f.len());
    assert_eq!(&w[2..], f.as_bytes());
    // requested 位：没发过才发
    let mut c = ServerBrokerRequestConnectionClient::new(&w[..]);
    assert!(!c.requested);
    c.requested = true;
    assert_eq!(c.process(), Drain::Events(1), "已发出的帧对读侧是不可见的");
}

#[test]
fn identity() {
    assert_eq!(MODULE_PATH, "graphwarrust::server_broker_request_connection");
    assert_eq!(TYPE_NAME, "ServerBrokerRequestConnectionClient");
    assert_eq!(LOC_CONNECT.0, "src/server_broker_request_connection.rs");
    assert_eq!(LOC_CONNECT.1, 66);
    assert_eq!(LOC_CONNECT.2, 42);
    assert_eq!(MSG_NONBLOCKING, "Cannot set non-blocking");
    assert_eq!(MSG_NONBLOCKING.len(), 23);
    assert_eq!(PROCESS_ARM64, 0x3ea0cc);
    assert_eq!(PROCESS_ARM64_SIZE, 688);
    assert_eq!(PROCESS_X86, 0x3e5a80);
    assert_eq!(PROCESS_X86_SIZE, 816);
    let r = PROCESS_X86_SIZE as f64 / PROCESS_ARM64_SIZE as f64;
    assert!((0.61..=1.19).contains(&r), "ratio={r}");
}
