//! `graphwar_stream_buffer` 的验收：**帧层**（`u16` 大端长度 + JSON 负载）。
//!
//! ⚠️ 用 `#[path]` 把源码编进本测试二进制 ⇒ **不需要** `src/lib.rs` 加 `pub mod`
//! （与 `tests/graphwar_replay_control.rs` 同一手法）。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `length_prefix_is_big_endian` | 2 字节大端、只算负载 | **逐位**（`rev16` / `rol $0x8`） |
//! | `max_message_is_4096` | 读侧上限 `0x1000` | **逐位**（`cmp w24,#1,lsl #12`；x86 `cmp $0x1000`） |
//! | `write_cap_is_u16` | 写侧上限 `0xffff` | **逐位**（x86 `cmp $0xffff`） |
//! | `zero_bytes_is_connection_lost` | peek 0 字节 ⇒ 错误（**不是** EOF-静默） | **观测**（`0x45c57c`） |
//! | `partial_is_pending_and_non_consuming` | 半帧 ⇒ `Ok(None)` 且不消费 | **观测**（`0x45c584`/`0x45c6c4`） |
//! | `read_exact_takes_len_plus_two` | 消费的是 `len+2`，负载从 `+2` 起 | **逐位**（`0x45c684`/`0x45c734`） |
//! | `error_literals_are_the_ones_in_rodata` | 5 条错误串与其长度 | **逐位**（`.rodata` + `mov w,#len`） |
//! | `discriminants` | `0x…4a` = `Ok(None)`、`0x…4b` = `Err` | **观测**（`0x45c6fc`/`0x45c5f0`） |
#![allow(dead_code)]

#[path = "../src/graphwar_stream_buffer.rs"]
mod graphwar_stream_buffer;

use graphwar_stream_buffer::*;

#[test]
fn length_prefix_is_big_endian() {
    // 抓包实测那一帧：0x0031 = 49（docs/09 §1.1）
    let mut wire = Vec::new();
    GraphwarStreamBuffer::new(&mut wire)
        .write_event(&[b'x'; 49])
        .unwrap();
    assert_eq!(&wire[..2], &[0x00, 0x31]);
    assert_eq!(wire.len(), 51);
    assert_eq!(decode_len([0x00, 0x31]), 49);
    // 0x0102 vs 0x0201：小端实现会在这里翻车
    assert_eq!(encode_len(0x0102), Some([0x01, 0x02]));
    assert_eq!(LEN_PREFIX_LEN, 2);
}

#[test]
fn round_trip_is_byte_identical() {
    let frames: [&[u8]; 3] = [
        b"{}",
        br#"{"TickRequest":{}}"#,
        br#"{"NewConnectionRequest":{"major_version":"2.0","minor_version":"60"}}"#,
    ];
    let mut wire = Vec::new();
    for f in frames {
        GraphwarStreamBuffer::new(&mut wire).write_event(f).unwrap();
    }
    let mut cur = &wire[..];
    for f in frames {
        assert_eq!(
            GraphwarStreamBuffer::new(&mut cur).read_event(),
            StreamRead::Event(f.to_vec())
        );
    }
    assert!(cur.is_empty());
}

#[test]
fn max_message_is_4096() {
    assert_eq!(MAX_MESSAGE_LEN, 4096);
    // 4096 合法
    let mut ok = vec![0x10u8, 0x00];
    ok.extend(std::iter::repeat(b'x').take(4096));
    let mut c = &ok[..];
    assert!(matches!(
        GraphwarStreamBuffer::new(&mut c).read_event(),
        StreamRead::Event(ref v) if v.len() == 4096
    ));
    // 4097 拒收
    let mut bad = vec![0x10u8, 0x01];
    bad.extend(std::iter::repeat(b'x').take(4097));
    let mut c = &bad[..];
    assert_eq!(
        GraphwarStreamBuffer::new(&mut c).read_event(),
        StreamRead::Failed(ReadError::TooLarge(4097))
    );
}

#[test]
fn write_cap_is_u16() {
    let mut out = Vec::new();
    assert!(GraphwarStreamBuffer::new(&mut out)
        .write_event(&vec![0u8; 0xffff])
        .is_ok());
    assert_eq!(out.len(), 0xffff + 2);
    let mut out = Vec::new();
    assert_eq!(
        GraphwarStreamBuffer::new(&mut out).write_event(&vec![0u8; 0x1_0000]),
        Err(WriteError::TooLong)
    );
    assert!(out.is_empty(), "超长一个字节都不写");
    assert_eq!(encode_len(0x1_0000), None);
}

#[test]
fn zero_bytes_is_connection_lost() {
    let mut empty = &[][..];
    assert_eq!(
        GraphwarStreamBuffer::new(&mut empty).read_event(),
        StreamRead::Failed(ReadError::ConnectionLost)
    );
    // EOF 有自己的一条消息，**不**借用 "还没到"
    assert_eq!(ReadError::ConnectionLost.message(), Some("Connection Lost"));
}

#[test]
fn partial_is_pending_and_non_consuming() {
    // 头只到 1 字节
    let mut one = &[0u8][..];
    assert_eq!(
        GraphwarStreamBuffer::new(&mut one).read_event(),
        StreamRead::Pending
    );
    assert_eq!(one.len(), 1, "peek 不消费");
    // 身子没到
    let wire = [0x00u8, 0x04, b'{', b'}'];
    let mut cur = &wire[..];
    assert!(is_idle(
        &GraphwarStreamBuffer::new(&mut cur).read_event()
    ));
    assert_eq!(cur.len(), 4, "整帧没齐之前一个字节都不消费");
}

#[test]
fn read_exact_takes_len_plus_two() {
    // 原版 read_exact 读 len+2（头一起消费），负载是 buf[2..]
    let wire = [0x00u8, 0x02, b'{', b'}'];
    let mut cur = &wire[..];
    assert_eq!(
        GraphwarStreamBuffer::new(&mut cur).read_event(),
        StreamRead::Event(b"{}".to_vec())
    );
    assert!(cur.is_empty(), "整帧一次消费干净");
}

#[test]
fn error_literals_are_the_ones_in_rodata() {
    assert_eq!(MSG_CONNECTION_LOST, "Connection Lost");
    assert_eq!(MSG_CONNECTION_LOST.len(), 15);
    assert_eq!(MSG_TOO_LARGE, "Message too large");
    assert_eq!(MSG_TOO_LARGE.len(), 17);
    assert_eq!(MSG_SERIALIZE_FAILED, "Failed to serialize event.");
    assert_eq!(MSG_SERIALIZE_FAILED.len(), 26);
    assert_eq!(MSG_WRITE_FAILED, "Failed to write to tcp_stream.");
    assert_eq!(MSG_WRITE_FAILED.len(), 30);
    assert_eq!(
        MSG_WRITE_TOO_LONG,
        "Failed to write to tcp_stream. Message too long."
    );
    assert_eq!(MSG_WRITE_TOO_LONG.len(), 48);
    assert_eq!(WriteError::Serialize.message(), MSG_SERIALIZE_FAILED);
    assert_eq!(WriteError::Io.message(), MSG_WRITE_FAILED);
}

#[test]
fn discriminants() {
    assert_eq!(TAG_OK_NONE, 0x8000_0000_0000_004a);
    assert_eq!(TAG_ERR, 0x8000_0000_0000_004b);
    assert_eq!(CUSTOM_ERROR_KIND, 0x28);
    assert_eq!(MAX_FRAME_PAYLOAD, u16::MAX as usize);
}
