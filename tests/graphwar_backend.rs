#![allow(dead_code)]

//! `graphwar_backend.rs` 的验收（逐条对拍 `src/graphwar_backend.rs`）。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//!
//! 地址口径同 `tests/graphwar_client_connection_handler.rs`：arm64 = Android 2.0.60
//! 未 strip 构建的 ELF 虚地址；x86 = steam 2026-08-28（Ghidra = `+0x100000`）。
//!
//! 本文件用 `#[path]` 直接包含两个源文件（`src/lib.rs` 本轮不可改），
//! 所以下面前两行 `mod` 是**成对**的：`graphwar_backend.rs` 里用
//! `crate::graphwar_client_connection_handler::Link`，少一个就编不过。

#[path = "../src/graphwar_client_connection_handler.rs"]
mod graphwar_client_connection_handler;
#[path = "../src/graphwar_backend.rs"]
mod be;

/// 判据 1：本模块在 arm64 `.symtab` 里是 **18 个方法**。
///
/// 真值：`readelf -sW libgraphwarrust.so | grep 'graphwar_backend15GraphwarBackend'`
/// 给出 18 个 `FUNC`（另有 3 个 `.llvm.*` 重复实例化与 `drop_in_place` 胶水）。
/// `docs/07` 记的"15 个方法名"是只数到其中一部分；**不是**"15 个函数"的意思。
#[test]
fn eighteen_methods_and_only_four_have_x86_anchors() {
    use be::FUNCTIONS;
    assert_eq!(FUNCTIONS.len(), 18);
    for w in FUNCTIONS.windows(2) {
        assert!(w[0].arm64 + u64::from(w[0].arm64_size) <= w[1].arm64);
    }
    // 名字不重复（`subscribe_to_event` 与 `subscribe_to_event_with_id` 是两个方法）
    let mut names: Vec<&str> = FUNCTIONS.iter().map(|f| f.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 18);

    // `fixtures/name_to_addr.tsv`：只有 4 个是"唯一锚"
    let unique: Vec<&str> = FUNCTIONS
        .iter()
        .filter(|f| f.link == be::Link::Unique)
        .map(|f| f.name)
        .collect();
    assert_eq!(
        unique,
        vec![
            "GraphwarBackend::subscribe_to_event",
            "GraphwarBackend::resubscribe_to_events",
            "GraphwarBackend::start_client_to_replay",
            "GraphwarBackend::process",
        ]
    );
    // `process` 的 x86 地址来自 Location 79:51（展示锚），必须落地
    let p = FUNCTIONS.iter().find(|f| f.name.ends_with("::process")).unwrap();
    assert_eq!(p.x86, Some(0x515dc0));
    assert_eq!((p.arm64, p.arm64_size), (0x407ef0, 1392));
}

/// 判据 2：`process()` 每帧的处理顺序是固定的 5 步，`mode == 2` 与 `state == 4` 各挡一处。
///
/// 真值（arm64 `0x407ef0`）：
/// `0x407f20 cmp x8,#2`（模式 2 跳过 `ReplayPlayer::process`）、
/// `0x407f54 cmp x8,#4`（后端状态 4 跳过房间信息刷新）、
/// `0x407f60 cbz x8`（`+0x180` 为 0 也跳过）、
/// `0x407f38 / 0x408110 / 0x408224 / 0x408250`（四个子模块的 `process`，最后一个尾调用）。
#[test]
fn process_is_five_ordered_steps_with_two_guards() {
    use be::ProcessStep::*;
    assert_eq!(
        be::process_steps(0, true, true, 0, true, true),
        vec![ReplayPlayer, GameClient, LanRoomInfo, LanBroadcaster, Broker]
    );
    // 客户端缺席时前两步都不跑（连"跳过回放"的分支都不进）
    assert_eq!(be::process_steps(2, false, false, 0, false, false), Vec::new());
    // 模式 2 = 跳过回放播放器，但客户端照跑
    assert_eq!(be::process_steps(2, true, false, 0, false, false), vec![GameClient]);
    // 状态 4 / lan_ready 只挡"刷新房间信息"，LAN 广播者本身照跑
    assert_eq!(be::process_steps(0, false, true, 4, true, false), vec![LanBroadcaster]);
    assert_eq!(be::process_steps(0, false, true, 0, false, false), vec![LanBroadcaster]);
    // broker 永远最后、且是尾调用
    assert_eq!(be::process_steps(0, false, false, 0, false, true), vec![Broker]);
}

/// 判据 3：**(b) 本机地址是 IPv6 环回 `::1` + 服务端端口**，不是 `127.0.0.1`。
///
/// 真值：`start_client_to_local_server`（arm64 `0x407848`）在栈上拼 `SocketAddr`：
/// `0x407884 mov w8,#1` + `0x407894 strh w8,[sp,#8]`（变体 1 = `V6`）、
/// `0x40787c ldr x9,[x19,#0x1d0]`、`0x407880 ldr q0,[x8]`（`0x197680` 的
/// **16 字节全零 + 末尾 1** = `::1`）→ `0x407890 stur q0,[sp,#0xc]`、
/// `0x407888 stp wzr,wzr,[sp,#0x1c]`（flowinfo/scope 清 0）、
/// `0x40788c strh w1,[sp,#0x24]`（端口 = `get_server_port()` 的返回值）。
///
/// 与抓包 `"address":"[::1]:35245"` 一致（`docs/09` §1.1）。
#[test]
fn local_server_address_is_ipv6_loopback() {
    let ip = be::LOCAL_SERVER_IP;
    assert_eq!(ip[15], 1);
    assert!(ip[..15].iter().all(|b| *b == 0));
    assert_eq!(be::SOCKET_ADDR_V6, 1);
    // 兜底地址是另一条路（全局/broker），别和本机地址混为一谈
    assert_eq!(be::FALLBACK_SOCKET_ADDR, "127.0.0.1:8080");
    assert!(be::ClientTarget::Local { server_port: 35245 }.opens_socket());
}

/// 判据 4：**(b) 远端地址只认 IP 字面量**——`SocketAddr::from_str`，没有 DNS。
///
/// 真值：`GraphwarRootNode::process_button_pressed`（arm64 `0x3a6d48`）在
/// `0x3a9400` 调 `SocketAddr::from_str`（该符号只有一份，`0x83f174`）→
/// `0x3a9470` 调 `start_client_to_remote_server`；后者把**已经解析好的**
/// `&SocketAddr` 直接交给 `GraphwarGameClient::new`（`0x407bd8`）。
/// LAN 大厅点行也是这条路（`IVBoxContainer::process`，`0x450c20` → `0x450d68`）。
///
/// ⇒ `[::1]:35245` 能解析（IPv6 要带方括号），`example.com:1234` **不能**
/// （`from_str` 是纯文法解析，不做 DNS）。
#[test]
fn remote_address_is_parsed_not_resolved() {
    assert!(be::ClientTarget::Remote.opens_socket());
    assert!(!be::ClientTarget::Replay.opens_socket());
    assert!(be::ClientTarget::Remote.address_source().contains("SocketAddr::from_str"));
    assert!(be::ClientTarget::Local { server_port: 1 }.address_source().contains("get_server_port"));
    assert_eq!(be::GLOBAL_SERVER_URL, "https://graphwar.com/graphwar_II_ip.txt");
    assert_eq!(be::GLOBAL_SERVER_URL.len(), 39); // mov w1,#0x27
    assert_eq!(be::LOCAL_IP_CACHE, "user://local_ip.config");
    assert_eq!(be::MSG_NO_GLOBAL_ADDR.len(), 36); // 日志文案长度
}

/// 判据 5：**(d) LAN 与 Global 是两个独立字段，不是二选一开关**。
///
/// 真值：`+0x110` = `GraphwarLanBroadcasterV`（**内联**，`0x407f48` 起）、
/// `+0x228` = `Option<GraphwarServerBrokerHandler>`（`0x40822c` 判空后尾调用）。
/// 房主两条一起装：`GraphwarRootNode::process_button_pressed` 在同一个分支里
/// `start_lan_broadcaster` + `start_server_broker_handler`（`0x3a79e8`/`0x3a7a28`，
/// 另有三组重复点），两者互不排斥。
/// 大厅把两路合并成同一张房间表：`IVBoxContainer::process` 里
/// `GraphwarLanBroadcasterV::get_game_info` 与 `GraphwarServerBrokerHandler::get_game_info`
/// 都调（`0x450c20` 的调用表）。
#[test]
fn lan_and_broker_are_two_independent_channels() {
    use be::HostChannel::*;
    assert_eq!(LanBroadcast.field_offset(), 0x110);
    assert_eq!(Broker.field_offset(), 0x228);
    // 传输层不同：UDP/组播/换行分帧 vs TCP/u16 大端
    assert!(LanBroadcast.transport().contains("UDP"));
    assert!(Broker.transport().contains("u16"));
    assert_ne!(LanBroadcast.transport(), Broker.transport());
    // 组播组与本地常量（ff02::1 / ::1，由 IpAddr::from_str 解析）
    assert_eq!(be::LAN_MULTICAST, "ff02::1");
    assert_eq!(be::LAN_LOOPBACK, "::1");
}

/// 判据 6：本机联机**必须先起服务端**：端口是现取的，且 `state == 4` 直接返回。
///
/// 真值：`0x407864 ldr x8,[x0,#0x60]!; cmp x8,#4; b.eq`（状态 4 直接 return）；
/// `0x407870 tbz w0,#0`（`get_server_port()` 返回 `None` 也 return）。
#[test]
fn local_client_requires_a_running_server() {
    assert!(be::local_client_allowed(0, Some(35245)));
    assert!(!be::local_client_allowed(0, None));
    assert!(!be::local_client_allowed(4, Some(35245)));
}

/// 判据 7：12 条 `panic::Location`，`process` 独占 7 条。
///
/// 真值：arm64 `.data.rel.ro` 里 12 条指向 `.rodata 0x18a47c` 的
/// `"src/graphwar_backend.rs"`：79/86/87/88/90/92/93（`process` 里连续的
/// `get_module()`+`borrow_mut()`）+ 173×2（`start_client_to_replay`）+
/// 344（`subscribe_to_event`）+ 350（`resubscribe_to_events`）+ 359（`subscribe_to_event_with_id`）。
/// x86 侧 `fixtures/name_to_addr.tsv` 报的行号与之一致。
#[test]
fn twelve_locations_with_seven_inside_process() {
    let locs = be::LOCATIONS;
    assert_eq!(locs.len(), 12);
    for w in locs.windows(2) {
        assert!(w[0] < w[1]);
    }
    let in_process = locs.iter().filter(|(l, _)| (79..=93).contains(l)).count();
    assert_eq!(in_process, 7);
    for line in [79u32, 173, 344, 350, 359] {
        assert!(locs.iter().any(|(l, _)| *l == line), "missing line {line}");
    }
    assert_eq!(be::SOURCE_FILE, "src/graphwar_backend.rs");
    assert_eq!(be::MODULE, "graphwar_backend");
}

/// 判据 8：结构体偏移与默认房间名。
///
/// 真值：`new`（arm64 `0x407d08`）里 `mov w0,#0xd` 分配 13 字节后从
/// `.rodata 0x19ca23` 拷 `"Graphwar Game"`；`+0x00` 写 2、`+0x60` 写 4、
/// `+0x228` 写 `0x8000000000000000`（`Option::None` 的 niche）。
#[test]
fn backend_layout_and_default_room_name() {
    use be::BackendField as F;
    assert_eq!(
        [F::MODE, F::STATE, F::LAN_BROADCASTER, F::SHARED, F::CLIENT, F::MODULES, F::BROKER],
        [0x00, 0x60, 0x110, 0x1d0, 0x1e0, 0x220, 0x228]
    );
    for w in [F::MODE, F::STATE, F::LAN_BROADCASTER, F::SHARED, F::CLIENT, F::MODULES, F::BROKER].windows(2) {
        assert!(w[0] < w[1]);
    }
    assert_eq!(be::DEFAULT_ROOM_NAME, "Graphwar Game");
    assert_eq!(be::DEFAULT_ROOM_NAME.len(), 13); // mov w0,#0xd
}
