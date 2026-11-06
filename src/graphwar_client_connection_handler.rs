//! 原版 `graphwar_client_connection_handler.rs` 的复原（客户端联机那一半）。
//!
//! 证据口径（全部可指到字节，逐条写在下面的常量旁边）：
//!
//! * **arm64 2.0.60 未 strip 构建**——`lib/arm64-v8a/libgraphwarrust.so`
//!   （Android 版，`.symtab` 有 18793 个 `FUNC`）。本文件的 arm64 地址 = 该 `.so` 的
//!   ELF 虚地址（`readelf -sW` 口径），直接可用 `objdump`/capstone 复核。
//! * **x86 steam 2026-08-28 构建**——地址 = ELF 虚地址（`objdump` 口径，Ghidra = `+0x100000`）。
//!   stripped，只能靠 `fixtures/name_to_addr.tsv` 的 `panic::Location` 锚定。
//! * `panic::Location`（`{file: &'static str, line: u32, col: u32}`，24 字节，
//!   在 `.data.rel.ro` 里）是**唯一**能把两条架构对上的东西：同一份源码、同一行、同一列。
//!
//! # 这个模块是什么
//!
//! 客户端一侧的**连接状态机 + 帧转发器**。它不解析任何报文内容：业务事件由
//! `GraphwarEventHandler` 派发进来，本模块只负责"发出去"和"把收到的字节流交给
//! `GraphwarRemoteEventHandler`"。
//!
//! 部件关系（全部来自 arm64 反汇编）：
//!
//! ```text
//! GraphwarGameClient::new(addr)          @0x409c70  -> 本模块 new()          @0x48b048
//! GraphwarGameClient::connect()          @0x40aae0  -> 本模块 connect()      @0x48b8a4
//! GraphwarGameClient::disconnect()       @0x4087fc  -> 本模块 disconnect()   @0x48a8a0
//! GraphwarEventHandler::subscribe_to_event("tick_event")                   -> tick()      @0x48b468
//! GraphwarEventHandler::subscribe_to_event("connected_to_server_event")    -> handle_connected_to_server() @0x48af7c
//! GraphwarEventHandler::subscribe_to_event(<24 个 *_request_event>)        -> forward_event() @0x48ac54
//! ```
//!
//! 本模块**不做 DNS、不做地址解析**：它拿到的是一个已经解析好的 `SocketAddr`
//! （见 [`SocketAddrSlot`]）。地址从哪来是 `graphwar_backend.rs` 的事。
//!
//! # 一句话答案（四个问题的 (a)(c)）
//!
//! * **状态机只有 4 个状态**（0..3），且**不是** `docs/14` 的 `GameState`（8 个）——
//!   两套完全独立：`ConnectionState` 在 `Mutex<ConnectionState>` 里（1 字节），
//!   `GameState` 在 `GraphwarGameStateManager` 里。唯一的交点是"连接失败时把
//!   `GameState` 置 7"。
//! * **没有心跳**：本模块从不发 `GameHeartbeat`；断线靠读写错误发现，不靠超时。
//!
//! ⚠️ 按 `AGENTS.md` §0.2：本文件的 `cargo test` 是**回归保护，不是复原证据**。

// ---------------------------------------------------------------------------
// 0. 模块身份：函数表与锚定等级
// ---------------------------------------------------------------------------

/// 原版模块名（`.symtab` 里的 `graphwarrust::graphwar_client_connection_handler`）。
pub const MODULE: &str = "graphwar_client_connection_handler";

/// 源文件名。`.data.rel.ro` 里 16 条 `panic::Location` 全部指向它（arm64 `0x18e4ef`）。
pub const SOURCE_FILE: &str = "src/graphwar_client_connection_handler.rs";

/// 与真二进制对照的锚定强度。分级沿用 `fixtures/name_to_addr.tsv` 的判据：
/// 同一个 `(file, line, col)` 在两条架构上都**只被这一个函数引用**才叫 `Unique`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// 双向唯一 `(file,line,col)` 锚。
    Unique,
    /// 无直接一对一锚，但按「一个名字只对应一个地址」消去后只剩一个候选
    /// （`handle_connected_to_server` 就是这样落地的）。
    DeducedByElimination,
    /// 该函数不引用任何 `Location`，名字无法落地（纯胶水/纯内联）。
    NoAnchor,
}

/// 一个已归属的函数。
#[derive(Debug, Clone, Copy)]
pub struct Func {
    pub name: &'static str,
    /// arm64 2.0.60 构建的 ELF 虚地址。
    pub arm64: u64,
    /// arm64 `.symtab` 记录的字节长度。
    pub arm64_size: u32,
    /// x86 steam 2026-08-28 地址（`None` = 该构建里没有可锚定的入口）。
    pub x86: Option<u64>,
    pub link: Link,
}

/// 本模块的 8 个方法 + 3 个订阅闭包，共 **11 个 `FUNC`**（`drop_in_place` 胶水不算）。
///
/// 真值：`readelf -sW libgraphwarrust.so | grep graphwar_client_connection_handler`。
/// 三个闭包是 `GraphwarEventHandler::subscribe_to_event` 的处理器，它们分别把
/// `weak_caller` 打到 `forward_event` / `handle_connected_to_server` / `tick` 上
/// （arm64 `0x48afe0 adr x1,#0x48ac54`、`0x48b010 adr x1,#0x48af7c`、
/// `0x48b03c adr x1,#0x48b468`）。
pub const FUNCTIONS: &[Func] = &[
    Func { name: "GraphwarClientConnectionHandler::disconnect", arm64: 0x48a8a0, arm64_size: 948, x86: Some(0x4592d0), link: Link::Unique },
    Func { name: "GraphwarClientConnectionHandler::forward_event", arm64: 0x48ac54, arm64_size: 464, x86: Some(0x459690), link: Link::Unique },
    Func { name: "GraphwarClientConnectionHandler::get_connection_state", arm64: 0x48ae24, arm64_size: 344, x86: Some(0x459840), link: Link::Unique },
    Func { name: "GraphwarClientConnectionHandler::handle_connected_to_server", arm64: 0x48af7c, arm64_size: 64, x86: Some(0x459950), link: Link::DeducedByElimination },
    // 三个闭包：x86 侧多出一个 0x438880（`module_funcs.tsv` 里本模块共 8 条），
    // 但没逐条对上"它到底是哪一个闭包"，故只挂在 #1 上并标 NoAnchor。
    Func { name: "GraphwarClientConnectionHandler::subscribe_to_events_event_handler::{{closure}}#1", arm64: 0x48afbc, arm64_size: 48, x86: Some(0x438880), link: Link::NoAnchor },
    Func { name: "GraphwarClientConnectionHandler::subscribe_to_events_event_handler::{{closure}}#2", arm64: 0x48afec, arm64_size: 48, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarClientConnectionHandler::subscribe_to_events_event_handler::{{closure}}#3", arm64: 0x48b01c, arm64_size: 44, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarClientConnectionHandler::new", arm64: 0x48b048, arm64_size: 1056, x86: Some(0x4599f0), link: Link::Unique },
    Func { name: "GraphwarClientConnectionHandler::tick", arm64: 0x48b468, arm64_size: 1084, x86: Some(0x459f20), link: Link::Unique },
    Func { name: "GraphwarClientConnectionHandler::connect", arm64: 0x48b8a4, arm64_size: 568, x86: Some(0x45a340), link: Link::Unique },
    Func { name: "GraphwarClientConnectionHandler::connect::{{closure}}（连接线程本体）", arm64: 0x48dcac, arm64_size: 1156, x86: None, link: Link::NoAnchor },
];

/// 本模块在 `.data.rel.ro` 里的**全部 16 条** `panic::Location`，按行号排序。
///
/// 这不是"可能的 panic 点"，而是**真的**引用到的：arm64 扫描 5301 个 Location，
/// 属于本文件的 16 条全部被本模块的代码引用，没有一条落在别的模块里。
/// 用它可以把源码骨架画出来（行号 = 源码行）：
///
/// ```text
///  65 new: Rc::new_cyclic 里 RefCell 重入 panic
/// 124 connect: thread::spawn().unwrap()
/// 135 connect 线程: set_nonblocking().unwrap()  <- "Cannot set client tpc stream non-blocking"
/// 136 connect 线程: 第一个 lock().unwrap()
/// 137 connect 线程: 第二个 lock().unwrap()
/// 140 connect 线程: 流缓冲 lock().unwrap()
/// 150 disconnect: 写事件那段的 unwrap
/// 156 disconnect: 关流那段的 unwrap
/// 157 disconnect: ConnectionState lock().unwrap()
/// 161 get_connection_state: lock().unwrap()
/// 190 tick: 事件队列 borrow_mut
/// 198 tick: get_module_wk().unwrap()
/// 199 tick: 模块容器 borrow_mut
/// 206 tick: join().unwrap()  <- "Failed to join connection thread."
/// 249 handle_connected_to_server: panic!("explicit panic")  <- 收到的不是 ConnectedToServer
/// 255 forward_event: 流缓冲 lock().unwrap()
/// ```
pub const LOCATIONS: &[(u32, u32)] = &[
    (65, 28),
    (124, 39),
    (135, 61),
    (136, 40),
    (137, 46),
    (140, 46),
    (150, 71),
    (156, 33),
    (157, 39),
    (161, 39),
    (190, 34),
    (198, 119),
    (199, 31),
    (206, 32),
    (249, 18),
    (255, 67),
];

// ---------------------------------------------------------------------------
// 1. ConnectionState —— 4 个状态，1 字节
// ---------------------------------------------------------------------------

/// 连接状态。**4 个变体，占 1 字节**，放在 `Arc<Mutex<ConnectionState>>` 里。
///
/// 布局证据（arm64）：`Mutex` 的 `data` 字节在 `+0x15`，`poison` 在 `+0x14`，
/// `futex` 在 `+0x10`；`get_connection_state` 就是 `ldrb w0,[x20,#0x15]`
/// （arm64 `0x48ae70`），返回 1 字节 ⇒ 枚举只有 1 字节。
///
/// 取值证据（谁写）：
///
/// | 值 | 写入点 | arm64 |
/// |---|---|---|
/// | 0 | `disconnect`（关流之后无条件写） | `0x48aa10 strb wzr,[x20,#0x15]` |
/// | 1 | `connect`（起线程**之前**写） | `0x48b900 strb #1,[x21,#0x15]` |
/// | 2 | 连接线程：TCP 成功（`set_nonblocking` 之后） | `0x48de10 strb #2` |
/// | 3 | 连接线程：TCP 失败（`connect_timeout` 返回 `Err`） | `0x48dd1c strb #3` |
///
/// 取值证据（谁读）：只有 `tick` 读（`get_connection_state`），判据是
/// `cmp w8,#2; b.lo`（`0x48b4a4`）与 `b.ne`（`0x48b4b0`）⇒ 分成 `<2` / `==2` / `>2` 三段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionState {
    /// 0：没有连接（初值、以及 `disconnect` 之后）。
    Disconnected = 0,
    /// 1：`connect()` 已调用、连接线程还在跑。
    Connecting = 1,
    /// 2：TCP 已建立（尚未握手）。`tick` 看到它就发 `NewConnectionRequest`。
    Connected = 2,
    /// 3：连接线程失败。`tick` 看到它就把 `GameState` 置 7。
    Failed = 3,
}

impl ConnectionState {
    /// 4 个变体，判别值就是 `0..=3`（没有 niche、没有跳号）。
    pub const COUNT: u8 = 4;

    /// 从 `Mutex` 里读出来的那个字节还原。
    ///
    /// >4 的值在真二进制里不存在（写入点只有 4 个），返回 `None` 而不是猜一个。
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(ConnectionState::Disconnected),
            1 => Some(ConnectionState::Connecting),
            2 => Some(ConnectionState::Connected),
            3 => Some(ConnectionState::Failed),
            _ => None,
        }
    }

    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

// ---------------------------------------------------------------------------
// 2. tick：状态机唯一的消费者（纯判定）
// ---------------------------------------------------------------------------

/// `tick()` 对状态机做的一件事。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickAction {
    /// 什么都不做（连接线程还在跑、或已经断线）。
    Idle,
    /// 把 `NewConnectionRequestEvent{major:"2.0", minor:"60"}` 塞进事件队列（由队列线程写 socket）。
    SendNewConnectionRequest,
    /// `GraphwarGameStateManager::set_state(7)`。
    SetGameState(u8),
}

/// `tick` 前半段的判定（状态机部分），已把"闩锁"算进去。
///
/// 真二进制（arm64 `0x48b488..0x48b510`）：
///
/// ```text
/// if self.latched { return }                 // 0x48b488 ldrb w8,[x0,#0x68]; tbnz w8,#0 → 0x48b594
/// match state {
///    0 | 1          => {},                   // 0x48b4a4 cmp w8,#2; b.lo
///    2              => send NewConnectionRequest,  // 0x48b4b4
///    _ /* >= 3 */   => game_state_manager.set_state(7),  // 0x48b514 -> 0x48ae398
/// }
/// self.latched = true;                       // 两条分支都写：0x48b4b8 / 0x48b518 strb #1,[x19,#0x68]
/// ```
///
/// **闩锁（`self+0x68`）是本模块的关键设计**：状态机每个 `connect()` 只动作一次。
/// `connect()` 里 `strb wzr,[x19,#0x68]`（`0x48b928`）清零，`tick()` 里置 1。
/// 没有它，状态 2 会在每一帧都重发 `NewConnectionRequest`。
pub const fn tick_action(latched: bool, state: ConnectionState) -> TickAction {
    if latched {
        return TickAction::Idle;
    }
    match state {
        ConnectionState::Disconnected | ConnectionState::Connecting => TickAction::Idle,
        ConnectionState::Connected => TickAction::SendNewConnectionRequest,
        ConnectionState::Failed => TickAction::SetGameState(GAME_STATE_ON_CONNECT_FAILED),
    }
}

/// 连接失败时被置的 `GameState`。arm64 `0x48b564 mov w1,#7` + `bl …set_state`。
///
/// ⚠️ `7` 是 `GraphwarGameStateManager::set_state` 的**入参**，不是 `docs/14` 表里的
/// 编号；该编号的语义（哪个界面）**未知**，只有"失败 → 7"这一条是观测。
pub const GAME_STATE_ON_CONNECT_FAILED: u8 = 7;

// ---------------------------------------------------------------------------
// 3. connect / 连接线程：迁移与超时
// ---------------------------------------------------------------------------

/// 连接线程的超时（秒）。
///
/// 证据：arm64 `0x48dcc8 add x8,sp,#8`（sret）→ `0x48dccc mov w1,#0x1e`
/// → `0x48dcd0 mov w2,wzr` → `0x48dcd4 bl TcpStream::connect_timeout`
/// ⇒ `Duration{secs: 30, nanos: 0}`。
///
/// 这是**唯一的**时间常量：没有重连退避、没有心跳周期。
pub const CONNECT_TIMEOUT_SECS: u64 = 30;

/// `connect()` 调用的 socket API（用于说明"为什么需要 30 秒"：它必须等三路握手）。
pub const CONNECT_API: &str = "std::net::TcpStream::connect_timeout";

/// 连接线程结束后的状态迁移。
///
/// 真二进制：成功分支先 `set_nonblocking(true)`（失败则 `unwrap`，见
/// [`LOCATIONS`] 的 135 行），再把 fd 塞进 [`HandlerField::STREAM`] 那个
/// `Mutex<Option<TcpStream>>`（`None` 用 fd `-1` 表示，`cmn w0,#1` 判它），最后写 2；
/// 失败分支不碰流，直接写 3。
pub const fn state_after_connect_attempt(tcp_established: bool) -> ConnectionState {
    if tcp_established {
        ConnectionState::Connected
    } else {
        ConnectionState::Failed
    }
}

/// `set_nonblocking` 失败时的 `expect` 文案（41 字节）。
///
/// 真值：arm64 `0x1a0777`，`mov w1,#0x29`（= 41）。原文
/// `Cannot set client tpc stream non-blocking`——**`tpc` 是原版的拼写错误**，
/// 照抄不改（同族的 `Cannot set server tcp stream non-blocking` 是对的）。
pub const MSG_SET_NONBLOCKING: &str = "Cannot set client tpc stream non-blocking";

/// `thread::spawn` 失败时的 `expect` 文案（22 字节，arm64 `0x1a0718`）。
pub const MSG_SPAWN_FAILED: &str = "failed to spawn thread";

/// `join()` 失败时的 `expect` 文案（33 字节，arm64 `0x1a0756`）。
pub const MSG_JOIN_FAILED: &str = "Failed to join connection thread.";

// ---------------------------------------------------------------------------
// 4. 握手与事件号
// ---------------------------------------------------------------------------

/// `NewConnectionRequestEvent` 的两个版本字段。
///
/// 真值：`GraphwarEvent::make_new_connection_request_event`（arm64 `0x4693c0`）
/// 在堆上直接写 `"2.0"`（`mov w9,#0x2e32` = `"2."`，`mov w8,#0x30` = `'0'`）
/// 和 `"60"`（`mov w9,#0x3036` = `"60"`）。与抓包
/// `{"NewConnectionRequest":{"major_version":"2.0","minor_version":"60"}}` 一致。
pub const NEW_CONNECTION_REQUEST_MAJOR: &str = "2.0";
pub const NEW_CONNECTION_REQUEST_MINOR: &str = "60";

/// `GraphwarEvent` 的**内部**判别值（不是协议 tag！），高位恒为 `0x8000_0000_0000_00xx`。
///
/// 本模块只碰两个：
///
/// * `0x8000_0000_0000_0007` = `handle_connected_to_server` 唯一接受的那个事件
///   （arm64 `0x48af80 mov x9,#-0x7ffffffffffffff9` 再 `cmp`）。它带着
///   `connection_id`（`ldr w8,[x1,#8]`），写进 [`HandlerField::CONNECTION_ID`]。
/// * `0x8000_0000_0000_0006` = 断线时**自己合成**的那个事件（16 字节常量在
///   arm64 `0x198680`，值为 `06 00 … 80 | 00 … 80`），再把 `connection_id` 填到
///   `+0x20`，然后**本地**走一遍 `GraphwarRemoteEventHandler::handle_server_remote_event`。
pub const EVENT_CONNECTED_TO_SERVER: u64 = 0x8000_0000_0000_0007;
pub const EVENT_CONNECTION_LOST: u64 = 0x8000_0000_0000_0006;

/// `GraphwarStreamBuffer::read_event` 的两个"没有事件"返回值（arm64 `0x48b5fc`/`0x48b600`）。
///
/// * `0x…4a`：正常收尾（`0x48b624 cmp x8,x28; b.eq` → 直接退出循环）。语义**未知**
///   （读出来是"无事件"还是"EOF"分不清；没抓到过这个路径）。
/// * `0x…4b`：`Err(io::Error)`。载荷在 `+0x10`（`io::error::Error`），
///   要过一道 `Error::kind()`。
pub const STREAM_READ_NONE: u64 = 0x8000_0000_0000_004a;
pub const STREAM_READ_ERROR: u64 = 0x8000_0000_0000_004b;

/// `io::ErrorKind::WouldBlock` 的判别值（arm64 `0x48b67c cmp w8,#0xd`）。
///
/// 非阻塞 socket 读空就是这个错，`tick` 把它当成"这一帧没数据"直接吞掉；
/// **其它**错误才算断线。
pub const ERROR_KIND_WOULD_BLOCK: u8 = 13;

/// 读线程遇到致命错误时的处理：**只有当已经握手过（`connection_id` 有效）**才合成
/// 断线事件；否则静静丢掉（arm64 `0x48b698 tbz w8,#0 → 0x48b6c4`，bit0 是
/// [`HandlerField::CONNECTED`] 的最低位）。
pub const fn read_error_dispatches_local_event(word0: u32) -> bool {
    word0 & 1 != 0
}

// ---------------------------------------------------------------------------
// 5. 订阅表：26 个事件名
// ---------------------------------------------------------------------------

/// 连接到 `tick` 的事件名。arm64 `0x1a016a`，长度 `0xa`（`mov w2,#0xa`）。
pub const EVENT_TICK: &str = "tick_event";

/// 连接到 `handle_connected_to_server` 的事件名。arm64 `0x1a01ce`，长度 `0x19`。
pub const EVENT_CONNECTED_TO_SERVER_NAME: &str = "connected_to_server_event";

/// 24 个**原样转发**给服务器的事件名（`forward_event`）。
///
/// 真值：`new` 在栈上摆了一张 24 项 `(&str, usize)` 表（arm64 `0x48b15c..0x48b298`，
/// 每项 `0x10` 字节），然后循环 24 次调 `subscribe_to_event`（`cmp x25,#0x19`，
/// `x25` 从 1 起 ⇒ 24 次）。顺序 = 表里的顺序 = 下面的顺序，**不是**字母序。
///
/// 为什么是"转发"：三个闭包里只有这个的 `adr x1,#0x48ac54` 指向 `forward_event`
/// （`0x48b03c adr x1,#0x48b468` 指的是 `tick`，`0x48b010 adr x1,#0x48af7c` 指的是
/// `handle_connected_to_server`）；`forward_event` 只做 `write_event`，不解析。
pub const FORWARDED_REQUEST_EVENTS: [&str; 24] = [
    "tick_request_event",
    "new_connection_request_event",
    "new_player_request_event",
    "game_start_request_event",
    "function_fire_request_event",
    "function_update_request_event",
    "change_team_request_event",
    "remove_player_request_event",
    "add_soldier_request_event",
    "remove_soldier_request_event",
    "axis_mode_change_request_event",
    "function_mode_change_request_event",
    "turn_mode_change_request_event",
    "time_mode_change_request_event",
    "game_name_request_event",
    "new_bot_request_event",
    "name_request_event",
    "skin_graphics_request_event",
    "face_graphics_request_event",
    "hat_graphics_request_event",
    "functions_calculated_event",
    "rank_request_event",
    "lock_request_event",
    "chat_message_request_event",
];

/// 本模块一共订阅多少个事件名：24 个转发 + `tick_event` + `connected_to_server_event`。
pub const SUBSCRIBED_EVENT_COUNT: usize = FORWARDED_REQUEST_EVENTS.len() + 2;

/// 订阅表里**没有**的名字：`GameHeartbeat` 不在其中。
///
/// 这条是问题 (c) 的答案的一半：客户端连接层**既不订阅也不发送**心跳。
/// 佐证：全 `.rodata` 里 `game_heartbeat_event` 出现 **0 次**（只有 serde 侧的
/// `GameHeartbeatEvent` 结构体名，位于 `0x19ea9c`/`0x19f6a1`），而 `subscribe_to_event`
/// 的字符串分派器里也没有它 —— 即该事件在**这个构建**里没有名字分派入口。
pub const HEARTBEAT_EVENT_NAME: Option<&str> = None;

/// 给"想自己写客户端连原版服务端"的人：握手只需要发这一条。
///
/// 服务端回 `NewConnection{connection_id}` 之后，客户端**不再发任何东西**（除非玩家操作
/// 产生 `*_request_event`）。所以最小可连通客户端 = `NewConnectionRequest` + 空转读帧。
pub const MINIMAL_CLIENT_HANDSHAKE: &str = "NewConnectionRequest";

// ---------------------------------------------------------------------------
// 6. 结构体布局（RefCell 里的那个 handler）
// ---------------------------------------------------------------------------

/// `GraphwarClientConnectionHandler` 的字段偏移（arm64 观测；`self` = `RefCell` 值首址）。
///
/// 这张表是**从两边夹出来的**：`new` 只读不写（写它的三个参数），`connect`/`tick`/
/// `disconnect`/`forward_event` 各自用固定偏移访问，交叉后没有空洞。
///
/// `SocketAddr` 占 `0x08..0x28`（32 字节，`connect` 里 `ldur q0,[x19,#8]` +
/// `ldur q1,[x19,#0x18]` 原样拷进连接闭包，闭包再把 `x0` 本身当 `&SocketAddr`
/// 传给 `connect_timeout`）；变体 1 = `V6`（由 `backend` 侧构造时 `strh #1` 证实）。
pub struct HandlerField;

impl HandlerField {
    /// `u32`：0/1，"已握手"标志（`handle_connected_to_server` 写 1；
    /// `disconnect` 判 `==1`；`tick`/`forward_event` 只测 bit0）。
    pub const CONNECTED: usize = 0x00;
    /// `u32`：服务端分配的 `connection_id`（`ldr w8,[x1,#8]` → `str`）。
    pub const CONNECTION_ID: usize = 0x04;
    /// `SocketAddr`（32 字节，`::1` + 端口）。
    pub const ADDR: usize = 0x08;
    /// `Arc<Mutex<GraphwarStreamBuffer>>`——写帧走它，`None`（fd `-1`）= 没连上。
    pub const STREAM: usize = 0x30;
    /// `Arc<RefCell<GraphwarEventQueue>>`——发出去的帧先进这个队列。
    pub const EVENT_QUEUE: usize = 0x38;
    /// 模块容器的 `Arc`（`tick` 里 `get_module_wk(&self+0x40)` 拿到
    /// `GraphwarGameStateManager`）。
    pub const MODULES: usize = 0x40;
    /// `Arc<Mutex<ConnectionState>>`。
    pub const STATE: usize = 0x48;
    /// `Option<JoinHandle<()>>`（3 个字：`0x50`,`0x58`,`0x60`）。
    pub const JOIN_HANDLE: usize = 0x50;
    /// `bool`：状态机闩锁（`connect` 清零、`tick` 置 1）。
    pub const LATCH: usize = 0x68;
    /// 结构体总长（`+0x68` 之后还有一个字节，见 `RefCell` 对齐；`0x69` 向上对齐到 8）。
    pub const SIZE: usize = 0x70;
}

/// `Mutex` 内部的三个偏移（`std` 的 `sys::sync::Mutex` + `poison` + `data`）。
///
/// `get_connection_state`（arm64 `0x48ae24`）把这个布局逐字读出来了：
/// `+0x10` = `futex`（`ldaxr w9,[x19]`）、`+0x14` = `poison`（`ldrb w8,[x20,#0x14]`）、
/// `+0x15` = 数据首字节。
pub struct MutexField;

impl MutexField {
    pub const FUTEX: usize = 0x10;
    pub const POISON: usize = 0x14;
    pub const DATA: usize = 0x15;
}

/// `Mutex<GraphwarStreamBuffer>` 里那个 `Option<TcpStream>` 的表示。
///
/// `TcpStream` 就是一个 `RawFd`（`i32`）；`Option` 用 `-1` 当 niche
/// （arm64 `0x48a9a4 cmn w0,#1`、`0x48a9b0 mov w8,#-1; str w8,[x21,#0x18]`）。
pub const NO_STREAM_FD: i32 = -1;

// ---------------------------------------------------------------------------
// 7. 断线处理：三条路，同一个动作
// ---------------------------------------------------------------------------

/// 本模块发现"连接没了"的三个入口，以及它们的共同动作。
///
/// | 入口 | 触发 | arm64 |
/// |---|---|---|
/// | `forward_event` | `write_event` 返回 `Err` | `0x48acc4 cbz x0` |
/// | `tick`（读循环） | `read_event` 返回 `Err` 且 `kind != WouldBlock` | `0x48b67c` |
/// | `disconnect` | 玩家主动断 | `0x48a8c8 cmp w8,#1` |
///
/// 前两个/三个的差别只在"要不要先发一条事件"：`disconnect` 会先把
/// [`EVENT_CONNECTION_LOST`] **写出去**（`write_event`），而读写错误路径是把这个事件
/// **本地派发**（`GraphwarRemoteEventHandler::handle_server_remote_event`）——因为连
/// 已经断了，写不出去。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectCause {
    /// 玩家调用 `disconnect()`：往外写一条断线事件，再关流、再置 0。
    LocalRequest,
    /// 写失败：本地派发断线事件。
    WriteFailed,
    /// 读到致命错误：本地派发断线事件。
    ReadFailed,
}

impl DisconnectCause {
    /// 是否需要先把断线事件写进 socket。只有主动断线能写成功。
    pub const fn writes_event_to_socket(self) -> bool {
        matches!(self, DisconnectCause::LocalRequest)
    }

    /// 是否把断线事件本地派发（`handle_server_remote_event`）。
    pub const fn dispatches_locally(self) -> bool {
        !self.writes_event_to_socket()
    }

    /// 走完之后的状态。三条路都回到 0（读写错误路径靠"下一次 `disconnect()` 或
    /// 重连"落地；`tick` 自己不写状态 —— 这一点是**推断**，因为没观测到读写错误后
    /// `ConnectionState` 变成什么）。
    pub const fn resulting_state(self) -> ConnectionState {
        ConnectionState::Disconnected
    }
}

// ---------------------------------------------------------------------------
// 8. 自检（在 lib 里而不是 tests 里：这些是**布局常量**，写错了别的地方全错）
// ---------------------------------------------------------------------------
