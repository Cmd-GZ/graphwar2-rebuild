//! `graphwar_lan_broadcaster_v6` 的验收：**广播周期 1 s / 去重按发送方地址 / TTL 10 s**。
//!
//! ⚠️ 用 `#[path]` 把源码编进本测试二进制 ⇒ **不需要** `src/lib.rs` 加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `literals` | `ff02::1`(7) / `::`(2) / `::1`(3) | **逐位**（`.rodata` + `mov w1,#len`） |
//! | `period_is_one_second` | `elapsed > 1 s` 才发；1.000 s 整**不**发 | **逐位**（`0x3de7c8`/`0x3de7cc`/`0x3de7d4`） |
//! | `ttl_is_ten_seconds` | `secs >= 10` 删 | **逐位**（`0x3de6e4 cmp x0,#0xa`） |
//! | `dedup_by_sender` | 键 = 发送方 `SocketAddr`；同名不同源 = 两条 | **观测**（`0x3de638` insert） |
//! | `address_is_loopback` | 广播 `address` = `[::1]:port` | **观测**（`0x3ddf08` `IpAddr::from_str`） |
//! | `layout` | 结构 0xc0、桶步长 0x78、表偏移 | **观测**（`new` 的写点 + 6×32 拷贝） |
//! | `recv_buf_and_fd` | 收包缓冲 1000、`fd == -1` = 未 bind | **逐位**（`0x3de398` / `0x3de0f0`） |
//! | `process_pipeline` | 先清过期、再收、再按闸发 | **观测**（`0x3de654` → `0x3de8d0` 的顺序） |
#![allow(dead_code)]

// `#[path]` 只在顶层 mod 上可靠，所以先平铺编进来再重导出成 `crate::_scaffold::protocol`。
#[path = "../src/_scaffold/protocol.rs"]
mod protocol_src;

mod _scaffold {
    pub mod protocol {
        pub use crate::protocol_src::*;
    }
}

#[path = "../src/graphwar_lan_broadcaster_v6.rs"]
mod graphwar_lan_broadcaster_v6;

use graphwar_lan_broadcaster_v6::*;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

fn info(address: &str) -> protocol_src::Broadcast {
    protocol_src::Broadcast {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        current_player_count: 2,
        address: address.into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
        locked: false,
    }
}

fn addr(last: u8) -> SocketAddr {
    format!("127.0.0.1:3524{last}").parse().unwrap()
}

#[test]
fn literals() {
    assert_eq!(GROUP_ADDR_V6, "ff02::1");
    assert_eq!(GROUP_ADDR_V6.len(), 7);
    assert_eq!(BIND_ADDR, "::");
    assert_eq!(BIND_ADDR.len(), 2);
    assert_eq!(LOCAL_ADDR, "::1");
    assert_eq!(LOCAL_ADDR.len(), 3);
    // 组播地址确实被判成 IPv6 multicast
    let b = GraphwarLanBroadcasterV6::new(55463);
    assert!(is_group_target(&b.group, 55463));
}

#[test]
fn period_is_one_second() {
    assert_eq!(BROADCAST_PERIOD_SECS, 1);
    let t0 = Instant::now();
    let mut b = GraphwarLanBroadcasterV6::new(55463);
    assert!(!b.due_to_broadcast(t0), "第一次只记起点");
    assert!(!b.due_to_broadcast(t0 + Duration::from_millis(999)));
    assert!(
        !b.due_to_broadcast(t0 + Duration::from_secs(1)),
        "判据是 > 1s，整 1 秒不发"
    );
    assert!(b.due_to_broadcast(t0 + Duration::from_millis(1001)));
}

#[test]
fn ttl_is_ten_seconds() {
    assert_eq!(ENTRY_TTL_SECS, 10);
    let t0 = Instant::now();
    let mut b = GraphwarLanBroadcasterV6::new(55463);
    b.ingest(addr(1), info("[::1]:1").to_json().as_bytes(), t0);
    assert_eq!(b.expire_stale(t0 + Duration::from_secs(9)), 0);
    assert_eq!(b.expire_stale(t0 + Duration::from_secs(10)), 1);
    assert!(b.rooms.is_empty());
}

#[test]
fn dedup_by_sender() {
    let t0 = Instant::now();
    let mut b = GraphwarLanBroadcasterV6::new(55463);
    let mut a = info("[::1]:1");
    a.room_name = "A".into();
    let mut c = info("[::1]:2");
    c.room_name = "C".into();
    b.ingest(addr(1), a.to_json().as_bytes(), t0);
    b.ingest(addr(1), c.to_json().as_bytes(), t0);
    assert_eq!(b.rooms.len(), 1, "同一发送方覆盖");
    assert_eq!(b.get_game_info(&addr(1)).unwrap().room_name, "C");
    b.ingest(addr(2), c.to_json().as_bytes(), t0);
    assert_eq!(b.rooms.len(), 2, "同名不同源 = 两条（LAN 列表里可能出现同名房间）");
    assert!(b.get_game_info(&addr(9)).is_none());
}

#[test]
fn address_is_loopback() {
    assert_eq!(local_broadcast_address(35245), "[::1]:35245");
    let mut b = GraphwarLanBroadcasterV6::new(55463);
    b.set_room_info(info("whatever"), 35245);
    let payload = b.broadcast_payload().unwrap();
    assert!(payload.contains("\"address\":\"[::1]:35245\""));
    // 无换行、无 NUL（一条 UDP = 一条 JSON）
    assert!(!payload.contains('\n'));
    assert!(!payload.contains('\0'));
    // 9 个字段齐全（docs/09 §1.1）
    for k in [
        "room_name",
        "game_state",
        "current_player_count",
        "address",
        "axis_mode",
        "function_mode",
        "turn_mode",
        "time_mode",
        "locked",
    ] {
        assert!(payload.contains(&format!("\"{k}\":")), "缺 {k}");
    }
}

#[test]
fn layout() {
    assert_eq!(STRUCT_SIZE, 0xc0);
    assert_eq!(BUCKET_STRIDE, 0x78);
    assert_eq!(OFF_ROOMS, 0x78);
    assert_eq!(OFF_ROOMS_LEN, 0x90);
    assert_eq!(OFF_ROOMS_HASHER, 0x98);
    // vbox 读的是**另一张表**（+0x1a0），不是本模块的 +0x78
    assert_ne!(VBOX_ROOM_TABLE_OFF, OFF_ROOMS);
}

#[test]
fn recv_buf_and_fd() {
    assert_eq!(RECV_BUF, 1000);
    assert_eq!(FD_NONE, -1);
    let mut b = GraphwarLanBroadcasterV6::new(55463);
    assert_eq!(b.bound_port, None);
    assert!(b.start(55463));
    assert_eq!(b.bound_port, Some(55463));
    // 已 bind ⇒ 不再 bind（原版没有这个守卫，见模块文档 §3.1 注）
    assert!(!b.start(55463));
}

#[test]
fn ingest_rejects_garbage_and_oversize() {
    let t0 = Instant::now();
    let mut b = GraphwarLanBroadcasterV6::new(55463);
    assert!(!b.ingest(addr(1), b"not json", t0));
    assert!(!b.ingest(addr(1), &[0xff, 0xfe], t0), "非 UTF-8");
    assert!(!b.ingest(addr(1), &vec![b'x'; RECV_BUF + 1], t0), "超接收缓冲");
    assert!(b.rooms.is_empty());
    assert_eq!(b.seq, 0);
}

#[test]
fn process_pipeline() {
    let t0 = Instant::now();
    let mut b = GraphwarLanBroadcasterV6::new(55463);
    b.set_room_info(info("x"), 35245);
    let (e, out) = b.process(t0, &[(addr(1), info("[::1]:1").to_json().into_bytes())]);
    assert_eq!(e, 0);
    assert!(out.is_none(), "刚起表，不到 1 s 不发");
    assert_eq!(b.rooms.len(), 1, "收包在发之前");
    let (e, out) = b.process(t0 + Duration::from_secs(2), &[]);
    assert_eq!(e, 0);
    assert!(out.is_some(), "过 1 s 该发了");
    let (e, _) = b.process(t0 + Duration::from_secs(11), &[]);
    assert_eq!(e, 1, "11 s 后那条过期");
}
