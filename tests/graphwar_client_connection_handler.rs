#![allow(dead_code)]

//! `graphwar_client_connection_handler.rs` 的验收（逐条对拍 `src/graphwar_client_connection_handler.rs`）。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//! 每条的右侧真值都能指到 `.symtab` 记录、反汇编地址或 `.rodata` 字节，逐条写明出处。
//!
//! 地址口径：**arm64 地址**来自 Android 2.0.60 未 strip 的 `libgraphwarrust.so`
//! （`readelf -sW` 口径，可 `objdump` 复核）；**x86 地址**来自 steam 2026-08-28 构建
//! （`fixtures/name_to_addr.tsv` 的 Location 锚）。
//!
//! 本文件用 `#[path]` 直接包含源文件：`src/lib.rs` 里暂时没有 `pub mod`
//! （改 `lib.rs` 不在本轮白名单），所以不能写 `gw2_rebuild::…`。
//! 等 `pub mod graphwar_client_connection_handler;` 落地后可以把这两行删掉。

#[path = "../src/graphwar_client_connection_handler.rs"]
mod ch;

/// 判据 1：本模块在 arm64 `.symtab` 里是 **11 个 `FUNC`**（8 个方法 + 3 个订阅闭包），
/// 另加连接线程本体（它挂在 `__rust_begin_short_backtrace` 的一个实例化名下，靠 4 条
/// 本文件 Location 归属回来）。
///
/// 真值：`readelf -sW libgraphwarrust.so | grep graphwar_client_connection_handler`。
#[test]
fn eleven_functions_with_the_expected_arm64_layout() {
    assert_eq!(ch::FUNCTIONS.len(), 11);
    // `disconnect` 是最大的方法（948 字节），`handle_connected_to_server` 最小（64 字节）。
    let disc = ch::FUNCTIONS.iter().find(|f| f.name.ends_with("::disconnect")).unwrap();
    assert_eq!((disc.arm64, disc.arm64_size), (0x48a8a0, 948));
    let hconn = ch::FUNCTIONS
        .iter()
        .find(|f| f.name.ends_with("handle_connected_to_server"))
        .unwrap();
    assert_eq!((hconn.arm64, hconn.arm64_size), (0x48af7c, 64));
    // 地址严格递增、互不重叠（`.symtab` 的记录本来就该这样）
    for w in ch::FUNCTIONS.windows(2) {
        assert!(w[0].arm64 + u64::from(w[0].arm64_size) <= w[1].arm64);
    }
    // 8 个方法里有 7 个在 x86 侧有锚；3 个闭包里只有 1 个落地。
    let with_x86 = ch::FUNCTIONS.iter().filter(|f| f.x86.is_some()).count();
    assert_eq!(with_x86, 8);
}

/// 判据 2：`ConnectionState` 是 **4 个变体、1 字节**。
///
/// 真值：
/// * 1 字节 —— `get_connection_state` 用 `ldrb w0,[x20,#0x15]` 直接返回
///   （arm64 `0x48ae70`），`0x15` 是 `Mutex` 的 data 首字节；
/// * 4 个判别值 —— 写入点只有 4 处：`disconnect` 写 0（`0x48aa10`）、
///   `connect` 写 1（`0x48b900`）、连接线程写 2/3（`0x48de10` / `0x48dd1c`）；
/// * `tick` 的判据是 `cmp w8,#2; b.lo` + `b.ne`（`0x48b4a4`/`0x48b4b0`），
///   把 `0..=3` 切成长 `<2`、`==2`、`>2` 三段 —— 与 4 个变体自洽。
#[test]
fn connection_state_has_exactly_four_one_byte_variants() {
    use ch::ConnectionState::*;
    assert_eq!(ch::ConnectionState::COUNT, 4);
    assert_eq!((Disconnected.as_u8(), Connecting.as_u8(), Connected.as_u8(), Failed.as_u8()), (0, 1, 2, 3));
    // 逐字节往返
    for v in 0..=3u8 {
        assert_eq!(ch::ConnectionState::from_u8(v).unwrap().as_u8(), v);
    }
    // 没有第 5 个：真二进制没有写入点，读出来也只会是 0..3
    assert_eq!(ch::ConnectionState::from_u8(4), None);
    assert_eq!(ch::ConnectionState::from_u8(255), None);
    assert_eq!(std::mem::size_of::<ch::ConnectionState>(), 1);
}

/// 判据 3：`tick` 的状态机是**三段分派 + 闩锁**。
///
/// 真值：arm64 `0x48b488`（`ldrb w8,[x0,#0x68]; tbnz w8,#0` 直接跳过状态机）、
/// `0x48b4a4`（`cmp w8,#2; b.lo`）、`0x48b4b4`（状态 2 → 造
/// `NewConnectionRequestEvent`）、`0x48b514`（其它 → `set_state(7)`）、
/// `0x48b4b8`/`0x48b518`（两条分支都把闩锁置 1）。
#[test]
fn tick_dispatch_is_latched_and_three_way() {
    use ch::{tick_action, ConnectionState::*, TickAction::*};
    assert_eq!(tick_action(false, Disconnected), Idle);
    assert_eq!(tick_action(false, Connecting), Idle); // 连接线程还在跑
    assert_eq!(tick_action(false, Connected), SendNewConnectionRequest);
    assert_eq!(tick_action(false, Failed), SetGameState(7));
    // 闩锁（self+0x68）压过一切：每个 connect() 只动作一次
    for s in [Disconnected, Connecting, Connected, Failed] {
        assert_eq!(tick_action(true, s), Idle);
    }
    // `docs/14` 的 GameState 是另一套 8 个状态，唯一交点是这句 set_state(7)
    assert_eq!(ch::GAME_STATE_ON_CONNECT_FAILED, 7);
}

/// 判据 4：连接线程只有 **30 秒** 一个时间常量，且没有重连/退避。
///
/// 真值：arm64 `0x48dccc mov w1,#0x1e` + `0x48dcd0 mov w2,wzr` →
/// `TcpStream::connect_timeout(addr, Duration::from_secs(30))`。
#[test]
fn connect_timeout_is_thirty_seconds_and_nothing_else_is_timed() {
    assert_eq!(ch::CONNECT_TIMEOUT_SECS, 30);
    assert_eq!(ch::CONNECT_API, "std::net::TcpStream::connect_timeout");
    // 线程结束后的两个去向（0x48de10 写 2 / 0x48dd1c 写 3）
    assert_eq!(ch::state_after_connect_attempt(true), ch::ConnectionState::Connected);
    assert_eq!(ch::state_after_connect_attempt(false), ch::ConnectionState::Failed);
}

/// 判据 5：订阅表 = **26 个名字** = 24 个转发 + `tick_event` + `connected_to_server_event`。
///
/// 真值：`new` 里先两次单独订阅（arm64 `0x48b0d4` 长度 `0x19` = 25 的
/// `connected_to_server_event`；`0x48b128` 长度 `0xa` = 10 的 `tick_event`），
/// 再一张 24 项 `(&str, usize)` 表（`0x48b15c..0x48b298`）循环 24 次
/// （`mov w25,#1` + `cmp x25,#0x19`）。三个闭包分别把 `weak_caller` 打到
/// `forward_event`(`0x48ac54`)、`handle_connected_to_server`(`0x48af7c`)、`tick`(`0x48b468`)。
#[test]
fn subscribed_event_names_are_twenty_six_and_are_not_alphabetical() {
    assert_eq!(ch::SUBSCRIBED_EVENT_COUNT, 26);
    assert_eq!(ch::FORWARDED_REQUEST_EVENTS.len(), 24);
    // 首项就是表里的第一项（0x1a0174，长度 0x12 = 18）
    assert_eq!(ch::FORWARDED_REQUEST_EVENTS[0], "tick_request_event");
    assert_eq!(ch::FORWARDED_REQUEST_EVENTS[0].len(), 18);
    // 末项最后被订阅（0x1a060c，长度 0x1a = 26）
    assert_eq!(ch::FORWARDED_REQUEST_EVENTS[23], "chat_message_request_event");
    assert_eq!(ch::FORWARDED_REQUEST_EVENTS[23].len(), 26);
    // 顺序 = 表顺序，不是字母序
    assert_ne!(ch::FORWARDED_REQUEST_EVENTS.to_vec(), {
        let mut v = ch::FORWARDED_REQUEST_EVENTS.to_vec();
        v.sort_unstable();
        v
    });
    // 不重复
    let mut v = ch::FORWARDED_REQUEST_EVENTS.to_vec();
    v.sort_unstable();
    v.dedup();
    assert_eq!(v.len(), 24);
    // 全部是 `*_event` 名字
    assert!(ch::FORWARDED_REQUEST_EVENTS.iter().all(|s| s.ends_with("_event")));
}

/// 判据 6：**没有心跳**。这是问题 (c) 的答案。
///
/// 真值：
/// * `tick` 里唯一发出去的事件是 `NewConnectionRequest`（`0x48b4f4` →
///   `GraphwarEvent::make_new_connection_request_event`），没有周期性发送；
/// * 整个 `.rodata` 里 `game_heartbeat_event` **0 次**命中（只有 serde 侧的结构体名
///   `GameHeartbeatEvent`，在 `0x19ea9c` / `0x19f6a1`）；
/// * `subscribe_to_event` 的名字分派器里也没有它 ⇒ 本构建没有它的名字入口；
/// * 断线判据是 `read`/`write` 的错误（`0x48b67c` 比 `ErrorKind`，`0x48acc4` 收
///   `write_event` 的返回值），不是"多久没收到包"。
#[test]
fn there_is_no_heartbeat_in_the_client_connection_layer() {
    assert_eq!(ch::HEARTBEAT_EVENT_NAME, None);
    assert!(!ch::FORWARDED_REQUEST_EVENTS.contains(&"game_heartbeat_event"));
    assert_eq!(ch::EVENT_TICK, "tick_event");
    assert_eq!(ch::EVENT_CONNECTED_TO_SERVER_NAME, "connected_to_server_event");
    // 非阻塞读空 = WouldBlock(13) 被吞掉；别的错才算断线（0x48b67c）
    assert_eq!(ch::ERROR_KIND_WOULD_BLOCK, 13);
    // 只有"已握手"才合成断线事件（0x48b698 tbz w8,#0）
    assert!(!ch::read_error_dispatches_local_event(0));
    assert!(ch::read_error_dispatches_local_event(1));
    assert!(ch::read_error_dispatches_local_event(3));
}

/// 判据 7：握手只发 `NewConnectionRequest{major_version:"2.0", minor_version:"60"}`。
///
/// 真值：`make_new_connection_request_event`（arm64 `0x4693c0`）在堆上直接写
/// `"2.0"`（`mov w9,#0x2e32` = `'2','.'`；`mov w8,#0x30` = `'0'`）和
/// `"60"`（`mov w9,#0x3036` = `'6','0'`）。与抓包完全一致。
#[test]
fn the_only_outbound_handshake_is_new_connection_request() {
    assert_eq!(ch::NEW_CONNECTION_REQUEST_MAJOR, "2.0");
    assert_eq!(ch::NEW_CONNECTION_REQUEST_MINOR, "60");
    assert_eq!(ch::MINIMAL_CLIENT_HANDSHAKE, "NewConnectionRequest");
}

/// 判据 8：`GraphwarEvent` 的**内部**判别值（不是协议 tag），本模块只用两个。
///
/// 真值：`handle_connected_to_server` 比较 `x9 = -0x7ffffffffffffff9` =
/// `0x8000000000000007`（arm64 `0x48af80`）；断线事件是 `.rodata 0x198680` 的
/// 16 字节常量 `06 00 … 80 | 00 … 80` = `0x8000000000000006`。
/// `read_event` 的两个"没有事件"返回值是 `0x…4a` / `0x…4b`（`0x48b5fc`/`0x48b600`）。
#[test]
fn internal_event_discriminants_are_not_protocol_tags() {
    assert_eq!(ch::EVENT_CONNECTED_TO_SERVER, 0x8000_0000_0000_0007);
    assert_eq!(ch::EVENT_CONNECTION_LOST, 0x8000_0000_0000_0006);
    assert_eq!(ch::STREAM_READ_NONE, 0x8000_0000_0000_004a);
    assert_eq!(ch::STREAM_READ_ERROR, 0x8000_0000_0000_004b);
    // 高位恒为 0x8000…：这是枚举的 niche 编码，不是 docs/07 的协议 tag
    for v in [
        ch::EVENT_CONNECTED_TO_SERVER,
        ch::EVENT_CONNECTION_LOST,
        ch::STREAM_READ_NONE,
        ch::STREAM_READ_ERROR,
    ] {
        assert_eq!(v >> 48, 0x8000);
    }
}

/// 判据 9：结构体偏移是从"只读"和"只写"两侧夹出来的，没有空洞、没有重叠。
///
/// 真值：`connect` 里 `ldr x21,[x0,#0x48]`（状态锁）、`ldr x8,[x19,#0x30]` /
/// `ldr x9,[x19,#0x48]`（两个 `Rc` 强计数）；`ldur q0,[x19,#8]`+`ldur q1,[x19,#0x18]`
/// （`SocketAddr` 32 字节原样拷进连接闭包）；`strb wzr,[x19,#0x68]`（闩锁）；
/// `Option<JoinHandle>` 在 `0x50/0x58/0x60`。
#[test]
fn handler_layout_is_consistent_and_ordered() {
    use ch::HandlerField as F;
    assert_eq!(
        [F::CONNECTED, F::CONNECTION_ID, F::ADDR, F::STREAM, F::EVENT_QUEUE, F::MODULES, F::STATE, F::JOIN_HANDLE, F::LATCH],
        [0x00, 0x04, 0x08, 0x30, 0x38, 0x40, 0x48, 0x50, 0x68]
    );
    // SocketAddr 占 0x08..0x28（32 字节），正好顶到 +0x28 之前，不和 +0x30 重叠
    assert_eq!(F::ADDR + 32, 0x28);
    assert!(F::ADDR + 32 <= F::STREAM);
    // Option<JoinHandle> 是 3 个字，0x50..0x68，正好顶到闩锁
    assert_eq!(F::JOIN_HANDLE + 24, F::LATCH);
    assert!(F::SIZE > F::LATCH);
    // Mutex 内部三件套：futex / poison / data 相邻且递增
    use ch::MutexField as M;
    assert!(M::FUTEX < M::POISON && M::POISON < M::DATA);
    assert_eq!(M::DATA - M::POISON, 1);
    // Option<TcpStream> 的 None 就是 fd -1（0x48a9a4 cmn w0,#1）
    assert_eq!(ch::NO_STREAM_FD, -1);
}

/// 判据 10：三条断线入口的共同动作与差异。
///
/// 真值：`disconnect` 先把断线事件 `write_event` 出去（`0x48a92c` 的返回值收在
/// `x29-0x10`），再关流（`0x48a9b4` 写 `-1`）、再置 0；而 `forward_event`
/// （`0x48acc4`）和 `tick` 的读循环（`0x48b690`）是把这个事件**本地**交给
/// `GraphwarRemoteEventHandler::handle_server_remote_event`（`0x3e6df0`），因为已经写不出去了。
#[test]
fn disconnect_paths_differ_only_in_who_gets_the_event() {
    use ch::DisconnectCause::*;
    assert!(LocalRequest.writes_event_to_socket());
    assert!(!LocalRequest.dispatches_locally());
    for c in [WriteFailed, ReadFailed] {
        assert!(!c.writes_event_to_socket());
        assert!(c.dispatches_locally());
        assert_eq!(c.resulting_state(), ch::ConnectionState::Disconnected);
    }
    assert_eq!(LocalRequest.resulting_state(), ch::ConnectionState::Disconnected);
}

/// 判据 11：16 条 `panic::Location` **全部**落在本文件，且能画出源码骨架。
///
/// 真值：arm64 `.data.rel.ro` 的 16 条 Location 都指向 `.rodata 0x18e4ef` 的
/// `"src/graphwar_client_connection_handler.rs"`；行号与 x86 侧
/// `fixtures/name_to_addr.tsv` 记录的行号**逐条相同**（65/124/150/161/190/249/255 …），
/// 这就是"两个架构同一份源码"的硬证据。
#[test]
fn sixteen_locations_and_they_agree_with_the_x86_anchors() {
    assert_eq!(ch::LOCATIONS.len(), 16);
    for w in ch::LOCATIONS.windows(2) {
        assert!(w[0] < w[1], "{:?} >= {:?}", w[0], w[1]);
    }
    // x86 `name_to_addr.tsv` 里有锚的那几行必须在表里
    for line in [65u32, 124, 150, 161, 190, 249, 255] {
        assert!(
            ch::LOCATIONS.iter().any(|(l, _)| *l == line),
            "missing line {line}"
        );
    }
    assert_eq!(ch::SOURCE_FILE, "src/graphwar_client_connection_handler.rs");
    assert_eq!(ch::MODULE, "graphwar_client_connection_handler");
}
