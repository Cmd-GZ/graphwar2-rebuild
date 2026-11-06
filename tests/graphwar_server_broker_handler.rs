//! `graphwar_server_broker_handler` 的验收：**Global 一路的房间表**（broker 侧）。
//!
//! ⚠️ 用 `#[path]` 把源码编进本测试二进制 ⇒ **不需要** `src/lib.rs` 加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `six_functions_three_anchored_two_unanchored` | 6 个符号里 2 个不给 x86 地址 | **观测**（`.symtab` + 锚点表） |
//! | `start_stop_and_first_frame` | `start` 幂等、线程分离、第一帧只发一次 | **观测**（`0x489fa8` / `0x489e50`） |
//! | `table_keyed_by_room_address` | 键 = `GameInfo.address`（**不是**发送方） | **观测**（`0x48a078` 的 `insert`/`remove_entry`） |
//! | `removed_deletes` | `GameRemoved` → 删 | **推断**（该报文从未抓到，形状来自序列化代码） |
//! | `time_probe_threshold` | 5 ms 打印阈值常量 | **逐位**（`0x3f747ae147ae147b`） |
//! | `trace_constants` | 三个 Location + 后端槽偏移 | **观测** |
#![allow(dead_code)]

#[path = "../src/_scaffold/protocol.rs"]
mod protocol_src;
#[path = "../src/_scaffold/broker.rs"]
mod broker_src;

mod _scaffold {
    pub mod protocol {
        pub use crate::protocol_src::*;
    }
    pub mod broker {
        pub use crate::broker_src::*;
    }
}

#[path = "../src/graphwar_stream_buffer.rs"]
mod graphwar_stream_buffer;
#[path = "../src/server_broker_request_connection.rs"]
mod server_broker_request_connection;
#[path = "../src/graphwar_server_broker_handler.rs"]
mod graphwar_server_broker_handler;

use graphwar_server_broker_handler::*;
use std::net::SocketAddr;

fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap()
}

fn info_event(address: &str, name: &str) -> String {
    _scaffold::broker::game_info_event(&protocol_src::Broadcast {
        room_name: name.into(),
        game_state: "Setup".into(),
        current_player_count: 2,
        address: address.into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
        locked: false,
    })
}

#[test]
fn six_functions_three_anchored_two_unanchored() {
    assert_eq!(FUNCTIONS.len(), 6);
    let unanchored: Vec<&str> = FUNCTIONS
        .iter()
        .filter(|f| f.3.is_none())
        .map(|f| f.0)
        .collect();
    assert_eq!(unanchored, vec!["get_game_info", "new"], "两个函数无锚，不给地址");
    // 已对位的四行：`process` 超闸（1.226）是**已记录的例外**（3/4 个锚命中即是判等），
    // 其余三行在闸内。
    for (n, _a, asz, _x, xsz) in FUNCTIONS {
        if let Some(xsz) = xsz {
            let r = *xsz as f64 / *asz as f64;
            match *n {
                "process" => assert!((1.19..1.30).contains(&r), "{n} 超闸 {r}"),
                _ => assert!((0.61..=1.19).contains(&r), "{n} ratio={r}"),
            }
        }
    }
    assert_eq!(function_named("process").unwrap().4, Some(2560));
    assert!(function_named("nope").is_none());
}

#[test]
fn start_stop_and_first_frame() {
    let mut h = GraphwarServerBrokerHandler::new();
    assert!(!h.stop(), "还没起过");
    assert!(h.start(addr("127.0.0.1:42970")));
    assert!(!h.start(addr("127.0.0.1:42970")), "槽里已有 ⇒ 不重建");
    let c = h.conn.as_ref().unwrap();
    assert!(c.detached, "JoinHandle 拿到就 drop ⇒ 分离线程");
    assert!(!c.requested);
    let f = h.request().unwrap();
    assert_eq!(
        f,
        "{\"VersionInfo\":{\"version\":\"2.0.60\",\"system\":\"linux\"}}"
    );
    assert!(h.request().is_none(), "请求只发一次");
    assert!(h.stop(), "有连接才能停");
    assert!(h.conn.is_none());
}

#[test]
fn table_keyed_by_room_address() {
    let mut h = GraphwarServerBrokerHandler::new();
    assert_eq!(
        h.apply_event(&info_event("[::1]:35245", "A")),
        Some(Applied::Inserted)
    );
    assert_eq!(h.len(), 1);
    assert_eq!(
        h.get_game_info(&addr("[::1]:35245")).unwrap().room_name,
        "A"
    );
    // 同一地址再来 ⇒ 覆盖
    assert_eq!(
        h.apply_event(&info_event("[::1]:35245", "B")),
        Some(Applied::Inserted)
    );
    assert_eq!(h.len(), 1);
    assert_eq!(
        h.get_game_info(&addr("[::1]:35245")).unwrap().room_name,
        "B"
    );
    // 不同地址 ⇒ 两条
    assert_eq!(
        h.apply_event(&info_event("[::1]:35246", "C")),
        Some(Applied::Inserted)
    );
    assert_eq!(h.len(), 2);
    assert!(h.get_game_info(&addr("[::1]:9")).is_none());
}

#[test]
fn removed_deletes() {
    let mut h = GraphwarServerBrokerHandler::new();
    h.apply_event(&info_event("[::1]:35245", "A"));
    assert_eq!(
        h.apply_event(&_scaffold::broker::game_removed_event("[::1]:35245")),
        Some(Applied::Removed)
    );
    assert!(h.is_empty());
    // 删不存在 ⇒ Some(false)
    assert_eq!(
        h.apply_event(&_scaffold::broker::game_removed_event("[::1]:35245")),
        Some(Applied::NotFound)
    );
    // 地址非法 ⇒ 记一笔，不算事件
    assert_eq!(h.apply_event(&info_event("nope", "X")), None);
    assert_eq!(h.bad_address, 1);
}

#[test]
fn process_counts_both_directions() {
    let mut h = GraphwarServerBrokerHandler::new();
    let evs = vec![
        info_event("[::1]:1", "A"),
        info_event("[::1]:2", "B"),
        "{\"TickRequest\":{}}".to_string(),
        _scaffold::broker::game_removed_event("[::1]:1"),
    ];
    assert_eq!(h.process(evs.iter().map(|s| s.as_str())), (2, 1));
    assert_eq!(h.len(), 1);
    assert!(h.get_game_info(&addr("[::1]:2")).is_some());
}

#[test]
fn time_probe_threshold() {
    // 0x3f747ae147ae147b 的 f64 就是 0.005
    assert_eq!(TIME_PROBE_THRESHOLD_SECS.to_bits(), 0x3f747ae147ae147b);
}

#[test]
fn trace_constants() {
    assert_eq!(MODULE_PATH, "graphwarrust::graphwar_server_broker_handler");
    assert_eq!(TYPE_NAME, "GraphwarServerBrokerHandler");
    assert_eq!(BACKEND_OFFSET, 0x228, "docs/09 §11.5 的那个槽");
    assert_eq!(BUCKET_STRIDE, 0x78, "与 LAN 那张表同桶步长");
    assert_eq!(LOC_STOP, ("src/graphwar_server_broker_handler.rs", 90, 52));
    assert_eq!(LOC_PROCESS.1, 98);
    assert_eq!(LOC_THREAD_BODY.1, 60);
}
