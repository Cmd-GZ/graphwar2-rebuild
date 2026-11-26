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

// ===========================================================================
// 9. 函数体：7 条 F3（`fixtures/client_handler_anchors.tsv`）落成真函数
// ===========================================================================
//
// 判据（本轮 = T52 / `task-49`）：**每个函数能指到它自己的 `.eh_frame` FDE 范围**，
// 且函数体逐条对着该范围内的指令写。7 条全部 `观测` 档：
//   FDE 范围 = `addr_probe.py fde`；语义 = `addr_probe.py dis` 全文；
//   源行 = `addr_probe.py locs`（`panic::Location` 的 (file,line,col)）。
//
// ## 我方差异（不许当成原版结构，逐条登记在 `docs/06` §102.6）
//
// * **传输层是泛型参数 `S`**：原版把 `TcpStream` 写死。这里泛型是为了能用假实现做回归
//   测试（`graphwar_stream_buffer::GraphwarStreamBuffer<S>` 早就是这么做的）。
// * **`Event` 无载荷**：原版 `GraphwarEvent` 是带载荷的枚举，本模块签名里用到的
//   "连接 id" / "序列化负载" 由调用方给（`src/events.rs` 的 `enum Event` 只有判别值）。
// * **两个接缝**（原版是真函数，我方那一侧还没落码）：`GraphwarRemoteEventHandler::
//   handle_server_remote_event`（x86 `0x3e2830`）与 `GraphwarGameStateManager::set_state`
//   （x86 `0x4780c0`，真名由 `fixtures/name_to_addr.tsv` 唯一锚定）。接缝落到
//   `local_dispatch` / `requested_game_state` 两个**我方观测钩子**字段上。

use std::cell::RefCell;
use std::io::{self, Write};
use std::net::{SocketAddr, TcpStream};
use std::rc::Weak;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::events::Event;
use crate::graphwar_event_queue::GraphwarEventQueue;
use crate::graphwar_module_container::GraphwarModuleContainer;
use crate::graphwar_stream_buffer::{
    GraphwarStreamBuffer, PeekRead, ReadError, StreamRead, WriteError,
};

impl PeekRead for TcpStream {
    /// 我方接缝：`graphwar_stream_buffer::PeekRead` 只给了 `&[u8]` 实现，而原版的传输层
    /// 就是 `TcpStream`（`0x3d86a0 read_event` 的 `peek`/`read_exact` 是 `TcpStream` 的）。
    /// 这里补上唯一缺的那个 impl，语义 = `TcpStream::{peek, read_exact}` 原样转发。
    fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        TcpStream::peek(self, buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        io::Read::read_exact(self, buf)
    }
}

/// 传输层要的两件**只有 `TcpStream` 才做得到**的事（连接建立 + 置非阻塞）。
///
/// 二者在原版里都是 `std` 的 `TcpStream` 方法，被调者已定位：
/// `connect_timeout` = x86 `0x8f40a0`（`.rodata` 里有 `cannot set a 0 duration timeout`）、
/// `set_nonblocking` = x86 `0x8f44f0`（`0x45b102` 与 `0x438932` 两处 `call *[GOT]`）。
/// 做成 trait 只为让测试能替换（**我方差异**）。
pub trait Transport: PeekRead + Write {
    fn connect_timeout(addr: &SocketAddr, timeout: Duration) -> io::Result<Self>
    where
        Self: Sized;
    fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()>;
}

impl Transport for TcpStream {
    fn connect_timeout(addr: &SocketAddr, timeout: Duration) -> io::Result<Self> {
        TcpStream::connect_timeout(addr, timeout)
    }

    fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        TcpStream::set_nonblocking(self, nonblocking)
    }
}

/// `connect()` 里 `thread::spawn` 那个 move 闭包的**环境**（= 它捕获的三个变量）。
///
/// 布局是**读出来的**，不是排的：闭包本体 `0x438880` 访问 `+0x20`/`+0x28` 取两个 `Arc`
/// （`0x4388ad mov r15,[rbx+0x28]`、`0x43894b mov r12,[rbx+0x20]`），`+0` 起 32 字节是
/// `SocketAddr`（`0x4388a0` 把它当 `&SocketAddr` 传给 `connect_timeout`）；
/// `connect` 摆环境的三条指令正是 `0x45a3df/0x45a3e3`（`movups` 两份 16 字节 → `rsp+0x20..0x40`）、
/// `0x45a3f1 mov [rsp+0x40],rax`（stream `Arc` → `+0x20`）、`0x45a3f6 mov [rsp+0x48],rcx`（state → `+0x28`）。
pub struct ConnectThreadEnv<S> {
    /// `+0x00`（32 字节）
    pub addr: SocketAddr,
    /// `+0x20`
    pub stream: Arc<Mutex<Option<GraphwarStreamBuffer<S>>>>,
    /// `+0x28`
    pub state: Arc<Mutex<ConnectionState>>,
}

/// `GraphwarClientConnectionHandler` 本体（字段偏移全部来自 arm64/x86 双侧交叉，
/// 见上文 [`HandlerField`]；本结构体只是把那张偏移表变成真字段）。
pub struct GraphwarClientConnectionHandler<S: Send> {
    /// `+0x00`：`u32`，已握手标志（`handle_connected_to_server` 写 1；`disconnect` 判 `==1`）。
    pub connected: u32,
    /// `+0x04`：`u32`，服务端分的连接 id。
    pub connection_id: u32,
    /// `+0x08`：`SocketAddr`（32 字节）。
    pub addr: SocketAddr,
    /// `+0x30`：帧层，`None`（fd `-1`）= 没连上。
    pub stream: Arc<Mutex<Option<GraphwarStreamBuffer<S>>>>,
    /// `+0x38`：发件队列（`Arc<RefCell<..>>`：`0x459f90 cmp qword [r14+0x10],0` 是 RefCell 借用位）。
    pub event_queue: Arc<RefCell<GraphwarEventQueue>>,
    /// `+0x40`：模块容器（`Weak`：`0x45a000` 把它交给 `get_module_wk`）。
    pub modules: Weak<RefCell<GraphwarModuleContainer>>,
    /// `+0x48`：连接状态机（`Mutex` 数据首字节在 `+0x15`）。
    pub state: Arc<Mutex<ConnectionState>>,
    /// `+0x50`：连接线程（3 个字）。
    pub join_handle: Option<JoinHandle<()>>,
    /// `+0x68`：状态机闩锁（`connect` 清 0、`tick` 置 1）。
    pub latched: bool,
    /// **我方观测钩子**（原版没有这个字段）：接缝 `handle_server_remote_event` 落点。
    /// 原版把事件交给 `graphwar_remote_event_handler` 的巨型分派（x86 `0x3e2830`，
    /// 89 条 `panic::Location` 落在那个文件里）；那一侧尚未落码 ⇒ 这里原样收下**负载字节**。
    pub local_dispatch: Vec<Vec<u8>>,
    /// **我方观测钩子**（原版没有这个字段）：接缝 `set_state` 落点。
    pub requested_game_state: Option<u8>,
}

/// `ConnectionRemoved`(tag **6**) 的线上负载（外部标签 JSON）。
///
/// * 字段名/类型：`docs/07` §3.3 第 155 行 —— `connection_id: i64`、`reason: str|null`，
///   档位 **逐位**（那两行的字段名来自 `serialize_field` 的 `lea rsi/LEN` 序列）。
/// * 组包点：`disconnect` 的 `0x4592f4..0x459308`（tag `0x8000000000000006` 写 `+0`、
///   `0x8000000000000000` 哨兵写 `+0x08`、`connection_id` 写 `+0x20`；
///   `+0x20` 与 `graphwar_player_manager::CONNECTION_REMOVED_ID_OFFSET` 一致）。
/// * ⚠️ **键序与空白未对拍**（没有抓包样本）= 这一层是 `推断`；字段名与类型是 `逐位`。
pub fn encode_connection_removed(connection_id: i64, reason: Option<&str>) -> Vec<u8> {
    let reason = match reason {
        Some(s) => format!("\"{}\"", s),
        None => "null".to_string(),
    };
    format!(
        "{{\"ConnectionRemoved\":{{\"connection_id\":{},\"reason\":{}}}}}",
        connection_id, reason
    )
    .into_bytes()
}

/// `0x447470`（`io::Error::kind`）+ `0x45a156 cmp al,0xd`（`ErrorKind::WouldBlock`）的判据。
///
/// **我方差异（语义损失，已登记）**：`graphwar_stream_buffer::ReadError` 只有 4 个变体，
/// 把"底层 `io::Error`"整个并成 [`ReadError::Io`]、**丢掉了 `ErrorKind`**；而原版正是在
/// 这里用 `kind()==13` 把"非阻塞空读"与真错误分开。⇒ 本层按"`Io` = 非阻塞空读"近似
/// （非阻塞 socket 上这是绝大多数情形），近似方向与后果写死在
/// `docs/06` §102.6，并由 `tick_*` 两条测试钉住。
const fn read_error_is_would_block(e: ReadError) -> bool {
    matches!(e, ReadError::Io)
}

/// `tick` 里 `get_module_wk::<GraphwarGameStateManager>` 的**类型实参占位**。
///
/// 原版这一跳要的是 `GraphwarGameStateManager`（296 字节的对象本体），而我方
/// `src/graphwar_game_state_manager.rs` 目前只有事件层（没有那个类型）⇒ 用空标记类型
/// 让 `0x46c7c0`+`0x45a000` 这条控制流（`198` 的 `unwrap`、`199` 的 `borrow_mut`）落下来。
/// `set_state` 本身是接缝，落 `requested_game_state`。
pub struct GameStateManagerMarker;

impl<S: Transport + Send + 'static> GraphwarClientConnectionHandler<S> {
    /// `GraphwarClientConnectionHandler::connect` —— **x86 `0x45a340`**
    /// （FDE `0x45a340..0x45a55d`，`0x21d` 字节；唯一 Location `124:39`）。
    ///
    /// 逐步指令：
    /// ```text
    /// 45a350 mov r15,[rdi+0x48]         ; self.state = Arc<Mutex<ConnectionState>>
    /// 45a35f lock cmpxchg [r15+0x10],1  ; Mutex::lock（poison 位 +0x14、数据 +0x15）
    /// 45a39b mov byte [r15+0x15],1      ; **ConnectionState = Connecting**
    /// 45a3b3 xchg [r14],eax / 45a3b6    ; unlock（prev==2 时 45a4b8 futex wake）
    /// 45a3bf mov byte [rbx+0x68],0      ; **latched = false**
    /// 45a3c3 / 45a3d1 lock inc [..]     ; Arc::clone(self.stream) / Arc::clone(self.state)
    /// 45a3df movups xmm0,[rbx+8]        ; 32 字节 SocketAddr
    /// 45a3e3 movups xmm1,[rbx+0x18]
    /// 45a41e call 0x439e80              ; std::thread::spawn（它自己 .expect("failed to spawn thread")，msg 长 0x16）
    /// 45a43b cmp qword [rbx+0x50],0     ; 旧 JoinHandle 非空 ⇒ 先 drop（45a446 call 0x45fca0）
    /// 45a44b mov [rbx+0x50],r12 …       ; self.join_handle = Some(handle)
    /// ```
    pub fn connect(&mut self) {
        // 0x45a350→0x45a3b9：state 锁里写 1 再解锁。全函数**唯一**一条 Location（124:39）
        // 就挂在这次 `lock().unwrap()` 的 poison 分支上（0x45a49f `lea r8,slot 0x956d90`）。
        *self.state.lock().unwrap() = ConnectionState::Connecting;
        self.latched = false;
        let env = ConnectThreadEnv {
            addr: self.addr,
            stream: Arc::clone(&self.stream),
            state: Arc::clone(&self.state),
        };
        // 0x45a41e call 0x439e80 = std::thread::spawn
        let handle = std::thread::spawn(move || Self::connect_thread(env));
        self.join_handle = Some(handle);
    }

    /// `connect` 里那个 move 闭包的**本体** —— **x86 `0x438880`**
    /// （FDE `0x438880..0x438ca2`，`0x422` 字节；Location `135,136,137,140`）。
    ///
    /// ⚠️ 原版是**闭包字面量**，Rust 没法给闭包起名;这里把它提成关联函数,
    /// **"闭包 → 函数"的提出是源码级偏差**（`docs/06` §102.2 登记）。
    /// 提出依据不是"看起来像",而是四条独立证据:
    /// ① 它引用的 4 条 Location 全在连接线程体内（`135` `set_nonblocking` 的 expect、
    ///    `136`/`137` 两把锁的 unwrap、`140` 失败分支那把锁）；
    /// ② 唯一取址点 `0x43b101` 落在 `std::thread::spawn` 机制函数 `0x43b070` 内；
    /// ③ 环境布局 `{SocketAddr@+0, Arc<stream>@+0x20, Arc<state>@+0x28}` 与
    ///    `connect` 摆出来的环境逐字段相同；
    /// ④ 它自己调 `0x8f40a0`（`TcpStream::connect_timeout`）并写 `ConnectionState` 2/3。
    ///
    /// ```text
    /// 4388a0 call *[0x9bafd0] → 0x8f40a0  ; connect_timeout(&env.addr, Duration{edx=0x1e, ecx=0})
    /// 4388a6 cmp dword [rsp+0x18],1 / jne ; 1 = Err
    /// ---- Ok 臂（源行 135..137）----
    /// 438932 call *[0x9bafd8] → 0x8f44f0  ; set_nonblocking(true)
    /// 438942 test rax,rax / jne 0x438ad0  ; Err ⇒ expect("Cannot set client tpc stream non-blocking")（0x29=41 字节）
    /// 438992 edi=[r12+0x18] / cmp -1 / 43899c call close ; 旧 fd 先 drop
    /// 4389ac mov [r12+0x18],ebp           ; stream 槽 = 新 fd
    /// 438a0e mov byte [r15+0x15],2        ; **ConnectionState = Connected**
    /// ---- Err 臂（源行 140）----
    /// 4388ee mov byte [r15+0x15],3        ; **ConnectionState = Failed**
    /// ```
    pub fn connect_thread(env: ConnectThreadEnv<S>) {
        match S::connect_timeout(&env.addr, Duration::new(CONNECT_TIMEOUT_SECS, 0)) {
            Ok(mut stream) => {
                stream
                    .set_nonblocking(true)
                    .expect(MSG_SET_NONBLOCKING); // 135:61
                *env.stream.lock().unwrap() = Some(GraphwarStreamBuffer::new(stream)); // 136:40
                *env.state.lock().unwrap() = ConnectionState::Connected; // 137:46
            }
            Err(_) => {
                *env.state.lock().unwrap() = ConnectionState::Failed; // 140:46
            }
        }
    }

    /// `GraphwarClientConnectionHandler::disconnect` —— **x86 `0x4592d0`**
    /// （FDE `0x4592d0..0x459685`，`0x3b5` 字节；Location `150,156,157`）。
    ///
    /// ```text
    /// 4592eb cmp dword [rdi],1 / jne 0x4593a0  ; connected == 1 ?
    /// 4592f4 mov eax,[rbx+4]                   ; connection_id
    /// 4592f7/459304 lea rcx,[r15+1] / lea rax,[r15+7]  ; tag 0x8000…6、哨兵 0x8000…0
    /// 459300 mov [rsp+0x30],eax                ; +0x20 = connection_id
    /// 45930d…459324 lock [r12+0x10]            ; stream 锁（Location 150:71）
    /// 45934e cmp dword [r12+0x18],-1 / je      ; Option<TcpStream> 为 None ⇒ 跳过
    /// 459360 call 0x3d89e0                     ; write_event(&mut stream, &ev)
    /// 45936c test rax,rax / 459371 call 0x45f160 ; Err 丢掉（`let _ =`）
    /// 4593a0…4593ed                            ; stream 锁：close(fd)（call *0x9ba980）后 fd 写 -1
    /// 459415…45944c                            ; state 锁（Location 157:39）：写 0
    /// ```
    pub fn disconnect(&mut self) {
        if self.connected == 1 {
            // 0x4592f4..0x459308：合成 ConnectionRemoved{connection_id, reason: None}
            let payload = encode_connection_removed(i64::from(self.connection_id), None);
            // 借用检查器要求复制一份 `Arc`（原版直接读 `[rdi+0x30]`）；引用计数多 +1/−1，行为不变。
            let slot = Arc::clone(&self.stream);
            let mut guard = slot.lock().unwrap();
            if let Some(stream) = guard.as_mut() {
                // 150:71；写失败**不**处理（0x45936c test rax,rax 后只 drop 错误值）
                let _ = stream.write_event(&payload);
            }
        }
        // 156:33 —— 0x4593de close(fd) / 0x4593ed `mov dword [r13+0x18],-1`：`*slot = None`
        *self.stream.lock().unwrap() = None;
        // 157:39 —— 0x45944c `mov byte [r14+0x15],0`：无条件写 Disconnected
        *self.state.lock().unwrap() = ConnectionState::Disconnected;
    }

    /// `GraphwarClientConnectionHandler::forward_event` —— **x86 `0x459690`**
    /// （FDE `0x459690..0x459834`，`0x1a4` 字节；唯一 Location `255:67`）。
    ///
    /// 入参差异见上文「我方差异」：原版是 `&GraphwarEvent`（`write_event` 自己序列化），
    /// 我方帧层的 `write_event` 入参是**已序列化负载** ⇒ 本函数收负载。
    ///
    /// ```text
    /// 4596a1 mov r15,[rdi+0x30] / 4596b0 lock ; stream 锁（Location 255:67）
    /// 4596ec cmp dword [r15+0x18],-1 / je     ; 没连上 ⇒ 什么也不做
    /// 4596f7 call 0x3d89e0                    ; write_event
    /// 459700 test rax,rax / je 0x45973c       ; Ok ⇒ 直接走
    /// 459705 cmp byte [r14],0 / je 0x45973c   ; **写失败且已握手**才本地派发
    /// 45970b…459720 合成 tag 6 + connection_id(+0x20)
    /// 459735 call 0x3e2830                    ; handle_server_remote_event（接缝）
    /// 459741 call 0x45f160                    ; drop 错误值
    /// ```
    pub fn forward_event(&mut self, payload: &[u8]) {
        let slot = Arc::clone(&self.stream); // 借用检查器要求，见 disconnect 同注
        let mut guard = slot.lock().unwrap();
        if let Some(stream) = guard.as_mut() {
            // 255:67
            if stream.write_event(payload).is_err() {
                if self.connected != 0 {
                    // 0x459735 call 0x3e2830
                    self.dispatch_connection_removed();
                }
            }
        }
    }

    /// `GraphwarClientConnectionHandler::get_connection_state` —— **x86 `0x459840`**
    /// （FDE `0x459840..0x459944`，`0x104` 字节；唯一 Location `161:39`）。
    ///
    /// `0x45987e movzx eax,byte [r14+0x15]` —— 把 `Mutex<ConnectionState>` 的数据首字节
    /// **原样**返回，**不做**合法性校验（写入点只有 4 个，见 [`ConnectionState`]）。
    pub fn get_connection_state(&self) -> ConnectionState {
        *self.state.lock().unwrap() // 161:39
    }

    /// `GraphwarClientConnectionHandler::handle_connected_to_server` —— **x86 `0x459950`**
    /// （FDE `0x459950..0x459986`，`0x36` 字节；唯一 Location `249:18`）。
    ///
    /// `boundary_audit` 把这条的真名写成 `...::panic_cold_explicit`，但 FDE 里**热路径也在**：
    ///
    /// ```text
    /// 459950 movabs rax,0x8000000000000007
    /// 45995a cmp qword [rsi],rax / 45995d jne 0x45996c   ; 不是 ConnectedToServer ⇒ panic!
    /// 45995f mov eax,dword [rsi+8]                        ; connection_id = 事件载荷 +8
    /// 459962 mov dword [rdi],1                            ; self.connected = 1
    /// 459968 mov dword [rdi+4],eax                        ; self.connection_id = id
    /// 45996b ret
    /// 45996c …lea rdi @0x1c0ee1 'explicit panic', esi=0xe, Location 249:18, ud2
    /// ```
    ///
    /// 入参差异：`connection_id` 在原版是事件载荷（`ldr w8,[x1,#8]`），我方 `enum Event`
    /// 没有载荷 ⇒ 由调用方给。
    pub fn handle_connected_to_server(&mut self, event: &Event, connection_id: u32) {
        // 0x45995a：比的是 64 位 tag word `0x8000…7`，判别值 7 = `Event::ConnectedToServer`
        if event.wire_tag() != Event::ConnectedToServer.wire_tag() {
            panic!("explicit panic"); // 249:18
        }
        self.connected = 1; // 0x459962
        self.connection_id = connection_id; // 0x459968
    }

    /// `GraphwarClientConnectionHandler::tick` —— **x86 `0x459f20`**
    /// （FDE `0x459f20..0x45a335`，`0x415` 字节；Location `190,198,199,206`）。
    ///
    /// 两段：**状态机**（闩锁门控，每个 `connect` 只动作一次）+ **读循环**（每帧把 socket 榨干）。
    ///
    /// ```text
    /// 459f34 cmp byte [rdi+0x68],0 / jne epilogue   ; 闩锁已置 ⇒ 整段跳过
    /// 459f41 call 0x459840                          ; get_connection_state()
    /// 459f46 cmp al,2 / jb epilogue                 ; <2（Disconnected|Connecting）⇒ 什么都不做
    /// 459f51 cmp eax,2 / jne 0x459fc6               ; >2 ⇒ Failed 臂
    /// ---- state == 2（Connected）----
    /// 459f56 mov byte [rbx+0x68],1                  ; latched = 1
    /// 459f5a..459f7e take JoinHandle → call 0x43aa80 ; join().expect("Failed to join connection thread.")（206:32）
    /// 459f8c mov r14,[rbx+0x38]                     ; event_queue
    /// 459f90 cmp qword [r14+0x10],0 / jne          ; RefCell 借用位（Location 190:34 = borrow_mut）
    /// 459fa8 call 0x504430                          ; make_new_connection_request_event（写 "2."/“0”/"60"、tag 4）
    /// 459fb7 call *[0x9bac80] → 0x3d3530            ; 事件入队 sink（add_event）
    /// 459fc1 jmp 0x45a054
    /// ---- state > 2（Failed）----
    /// 459fc6 mov byte [rbx+0x68],1                  ; latched = 1
    /// 459fca..459fee take JoinHandle → join（同上 206:32，**同一个 Location** ⇒ 尾部复制）
    /// 45a000 call 0x46c7c0                          ; get_module_wk(&self.modules)（容器 Location 30:28）
    /// 45a005 test rax,rax / je 0x45a223             ; unwrap（Location 198:119 —— 全函数唯一一处）
    /// 45a016 cmp qword [rax+0x10],0 / jne          ; RefCell 借用位（Location 199:31 = borrow_mut）
    /// 45a030 esi=7 / 45a035 call *[0x9bb0b8] → 0x4780c0 ; set_state(7)（接缝）
    /// ---- 两臂都落到这里 ----
    /// 45a054 mov r14,[rbx+0x30] / 45a05f lock cmpxchg ; **try_lock**：抢占失败 45a065 jne epilogue 直接返回
    /// 45a09f cmp dword [r14+0x18],-1 / je unlock     ; stream 为 None ⇒ 走
    /// 45a0dc call 0x3d86a0                           ; read_event(&mut stream)
    /// 45a0e9/45a0fc 与 0x8000…4a / 0x8000…4b 比较     ; Ok(None) ⇒ 退出循环；Err ⇒ 判 kind
    /// 45a13f call 0x3e2830                           ; Ok(Some) ⇒ 派发（接缝）后 jmp 回循环头
    /// 45a151 call 0x447470 / 45a156 cmp al,0xd        ; io::Error::kind()==WouldBlock ⇒ 静默退出
    /// 45a169 cmp byte [rbx],0                        ; 否则：仅当已握手才合成 ConnectionRemoved 派发
    /// ```
    pub fn tick(&mut self) {
        if !self.latched {
            // 0x459f41 → 0x459f46/0x459f51：用**数值**比较（`cmp al,2; jb`），不是 enum match
            let raw = self.get_connection_state().as_u8();
            if raw < ConnectionState::Connected.as_u8() {
                // Disconnected(0) / Connecting(1)：不动
            } else if raw == ConnectionState::Connected.as_u8() {
                self.latched = true; // 0x459f56
                self.join_thread(); // 206:32
                // 190:34 —— `borrow_mut` 已借出即 panic（0x459f90 cmp [r14+0x10],0）
                self.event_queue
                    .borrow_mut()
                    .add_event(Event::NewConnectionRequest); // 0x459fa8/0x459fb7
            } else {
                self.latched = true; // 0x459fc6
                self.join_thread(); // 206:32（与上面**同一个** Location ⇒ 源码只有一处 .expect）
                // 198:119 —— 全函数唯一一处 `.unwrap()` 落在这一行
                let manager = GraphwarModuleContainer::get_module_wk::<GameStateManagerMarker>(
                    &self.modules,
                )
                .and_then(|w| w.upgrade())
                .unwrap();
                let _borrowed = manager.borrow_mut(); // 199:31
                // 接缝：原版 0x45a035 call *[0x9bb0b8] → 0x4780c0 `set_state(7)`；
                // 我方 `GraphwarGameStateManager::set_state` 尚未落码 ⇒ 记下"要求设成 7"。
                self.requested_game_state = Some(GAME_STATE_ON_CONNECT_FAILED);
            }
        }
        self.read_loop();
    }

    /// `tick` 的两处 `.expect("Failed to join connection thread.")` 是**同一个** `Location`
    /// （`206:32`，`0x45a257` 与 `0x45a288` 都 `lea r8,slot 0x956cd0`）⇒ 源码里只有**一处**
    /// `.expect`，被 LLVM 尾部复制到两条路径上。
    ///
    /// ⇒ 它只能住在一个**被内联的小函数**里。**函数边界是推断**：arm64 未 strip 侧的 11 个
    /// `FUNC` 里没有 `join_thread`（说明被完全内联，没有独立符号）。
    fn join_thread(&mut self) {
        if let Some(handle) = self.join_handle.take() {
            // 0x459f5a `mov rax,[rbx+0x50]; mov qword [rbx+0x50],0` = `Option::take`
            handle.join().expect("Failed to join connection thread."); // 206:32
        }
    }

    /// `tick` 尾段的读循环（`0x45a054..0x45a1db`），语义见 [`Self::tick`] 的指令表。
    fn read_loop(&mut self) {
        // 0x45a054→0x45a065：**`try_lock`**。抢不到锁（`0x45a065 jne 0x45a1db` 直接进函数
        // 收尾）⇒ 这一帧不读。注意这里**一条 Location 都没有**，对比 disconnect/forward_event
        // 的 `lock().unwrap()` 各自有 Location ⇒ 原版这一处确实不是 `unwrap`。
        let slot = Arc::clone(&self.stream); // 借用检查器要求，见 disconnect 同注
        let Ok(mut guard) = slot.try_lock() else {
            return;
        };
        let Some(buffer) = guard.as_mut() else {
            return; // 0x45a09f cmp dword [r14+0x18],-1 / je
        };
        loop {
            match buffer.read_event() {
                // 0x45a10b..0x45a13f：把事件搬到栈上再 call 0x3e2830，然后 jmp 回循环头
                StreamRead::Event(payload) => {
                    self.local_dispatch.push(payload); // 接缝
                }
                // 0x45a0f6 je 0x45a1ab：Ok(None) = 整帧还没到 ⇒ 退出循环
                StreamRead::Pending => break,
                StreamRead::Failed(e) => {
                    // 0x45a151 call 0x447470 + 0x45a156 cmp al,0xd
                    if !read_error_is_would_block(e) && self.connected != 0 {
                        // 0x45a191..0x45a19c call 0x3e2830（合成 ConnectionRemoved 本地派发）
                        self.dispatch_connection_removed();
                    }
                    break;
                }
            }
        }
    }

    /// 三条路共用的"本地合成 `ConnectionRemoved` 并派发"（`0x3e2830` 接缝）。
    ///
    /// 三处调用点各给地址：
    /// * `forward_event` 写失败：`0x459735`
    /// * `tick` 读循环致命错误：`0x45a19c`
    /// * （原版第三处 `disconnect`）走的是"写出去"而不是本地派发，见 [`Self::disconnect`]
    fn dispatch_connection_removed(&mut self) {
        let payload = encode_connection_removed(i64::from(self.connection_id), None);
        self.local_dispatch.push(payload);
    }
}

// ===========================================================================
// 10. 回归测试
// ===========================================================================
//
// ⚠️ 按 `AGENTS.md` §0.2：这些是**回归保护，不是复原证据**。
// 复原证据是 §9 每个函数头部的 FDE + 指令表 + `panic::Location` 行号。
// 但期望值**不是**从我方输出抄的：每一条的根据都写在断言旁边
// （哪个地址的哪条指令给出这个值），有争议处明确标「我方近似」。
#[cfg(test)]
mod function_bodies_tests {
    use super::*;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// `connect_thread` 的 Ok 臂/Err 臂由这个开关选（原版由真 socket 决定）。
    static CONNECT_OK: AtomicBool = AtomicBool::new(false);

    #[derive(Default)]
    struct FakeSock {
        /// 未消费的输入字节（帧层看到的"网络流"）。
        rx: Vec<u8>,
        pos: usize,
        /// 写下去的字节（**共享**给测试，因为 disconnect 会把槽置 None）。
        tx: Arc<Mutex<Vec<u8>>>,
        /// `true` ⇒ 每次 `write` 都返回 Err（模拟已断）。
        write_fails: bool,
    }

    impl PeekRead for FakeSock {
        fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let avail = self.rx.len() - self.pos;
            let n = buf.len().min(avail);
            buf[..n].copy_from_slice(&self.rx[self.pos..self.pos + n]);
            Ok(n)
        }

        fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
            let avail = self.rx.len() - self.pos;
            if avail < buf.len() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short"));
            }
            buf.copy_from_slice(&self.rx[self.pos..self.pos + buf.len()]);
            self.pos += buf.len();
            Ok(())
        }
    }

    impl Write for FakeSock {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.write_fails {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "dead"));
            }
            self.tx.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Transport for FakeSock {
        fn connect_timeout(_addr: &SocketAddr, _timeout: Duration) -> io::Result<Self> {
            if CONNECT_OK.load(Ordering::SeqCst) {
                Ok(FakeSock::default())
            } else {
                Err(io::Error::new(io::ErrorKind::ConnectionRefused, "no"))
            }
        }

        fn set_nonblocking(&mut self, _nonblocking: bool) -> io::Result<()> {
            Ok(())
        }
    }

    fn addr() -> SocketAddr {
        // 原版客户端固定连本机（`[::1]:port`，见模块文档 §我方差异）
        "[::1]:44444".parse().unwrap()
    }

    fn sock_with_tx(tx: &Arc<Mutex<Vec<u8>>>, write_fails: bool) -> GraphwarStreamBuffer<FakeSock> {
        GraphwarStreamBuffer::new(FakeSock {
            tx: Arc::clone(tx),
            write_fails,
            ..FakeSock::default()
        })
    }

    fn handler(
        tx: &Arc<Mutex<Vec<u8>>>,
        write_fails: bool,
        state: ConnectionState,
        connected: u32,
    ) -> GraphwarClientConnectionHandler<FakeSock> {
        GraphwarClientConnectionHandler {
            connected,
            connection_id: 7,
            addr: addr(),
            stream: Arc::new(Mutex::new(Some(sock_with_tx(tx, write_fails)))),
            event_queue: Arc::new(RefCell::new(GraphwarEventQueue::new())),
            modules: Weak::new(),
            state: Arc::new(Mutex::new(state)),
            join_handle: None,
            latched: false,
            local_dispatch: Vec::new(),
            requested_game_state: None,
        }
    }

    /// `0x45995a cmp qword [rsi],0x8000000000000007` + `0x459962/0x459968`：
    /// tag 7 才写 `connected=1`(`+0x00`) 与 `connection_id`(`+0x04`)；否则 `0x45996c` 的
    /// `panic!("explicit panic")`（Location 249:18，msg 长 `0xe`）。
    #[test]
    fn handle_connected_to_server_writes_two_u32_on_tag7_only() {
        let tx: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let mut h = handler(&tx, false, ConnectionState::Connecting, 0);
        h.handle_connected_to_server(&Event::ConnectedToServer, 0x1234);
        assert_eq!((h.connected, h.connection_id), (1, 0x1234));
        // 证据：边界审计的 `Location` 行 249 是 `panic!`，所以非 7 一定 panic
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            h.handle_connected_to_server(&Event::ConnectionRemoved, 9);
        }))
        .is_err());
        // 失败那次不许改字段（panic 在写之前：0x45995d 跳走）
        assert_eq!((h.connected, h.connection_id), (1, 0x1234));
    }

    /// `0x4388ee mov byte [r15+0x15],3`（Err 臂）与 `0x438a0e mov byte [r15+0x15],2`（Ok 臂）。
    #[test]
    fn connect_thread_maps_tcp_outcome_to_state_3_or_2() {
        // Err 臂
        CONNECT_OK.store(false, Ordering::SeqCst);
        let tx: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let state = Arc::new(Mutex::new(ConnectionState::Connecting));
        let stream: Arc<Mutex<Option<GraphwarStreamBuffer<FakeSock>>>> =
            Arc::new(Mutex::new(None));
        GraphwarClientConnectionHandler::<FakeSock>::connect_thread(ConnectThreadEnv {
            addr: addr(),
            stream: Arc::clone(&stream),
            state: Arc::clone(&state),
        });
        assert_eq!(*state.lock().unwrap(), ConnectionState::Failed);
        assert!(stream.lock().unwrap().is_none(), "失败不碰 stream 槽（0x4388ad 分支不写 +0x18）");

        // Ok 臂：先 set_nonblocking 再写 stream 槽再写 state=2
        CONNECT_OK.store(true, Ordering::SeqCst);
        let state = Arc::new(Mutex::new(ConnectionState::Connecting));
        let stream = Arc::new(Mutex::new(None));
        GraphwarClientConnectionHandler::<FakeSock>::connect_thread(ConnectThreadEnv {
            addr: addr(),
            stream: Arc::clone(&stream),
            state: Arc::clone(&state),
        });
        assert_eq!(*state.lock().unwrap(), ConnectionState::Connected);
        assert!(stream.lock().unwrap().is_some());
        CONNECT_OK.store(false, Ordering::SeqCst);
    }

    /// `0x4592eb cmp dword [rdi],1` 决定要不要发事件；`0x45944c` 无条件写 0；
    /// `0x4593ed mov dword [r13+0x18],-1` ⇒ 槽变 `None`。
    #[test]
    fn disconnect_sends_tag6_only_when_connected_then_clears_everything() {
        // connected == 1：发一条，然后槽 None + state 0
        let tx = Arc::new(Mutex::new(Vec::new()));
        let mut h = handler(&tx, false, ConnectionState::Connected, 1);
        h.disconnect();
        assert_eq!(*h.state.lock().unwrap(), ConnectionState::Disconnected);
        assert!(h.stream.lock().unwrap().is_none());
        let bytes = tx.lock().unwrap().clone();
        // 帧层：2 字节大端长度 + 负载（`src/graphwar_stream_buffer.rs` §3 事实 1）
        assert_eq!(
            u16::from_be_bytes([bytes[0], bytes[1]]) as usize,
            bytes.len() - 2
        );
        let text = String::from_utf8(bytes[2..].to_vec()).unwrap();
        assert!(text.starts_with("{\"ConnectionRemoved\":{\"connection_id\":7"),
                "实际负载 {text}");

        // connected == 0：一个字节都不写（0x4592eb jne 直接跳到 0x4593a0）
        let tx2 = Arc::new(Mutex::new(Vec::new()));
        let mut h2 = handler(&tx2, false, ConnectionState::Connected, 0);
        h2.disconnect();
        assert!(tx2.lock().unwrap().is_empty());
        assert_eq!(*h2.state.lock().unwrap(), ConnectionState::Disconnected);
    }

    /// `0x459700 test rax,rax`（写成功）与 `0x459705 cmp byte [r14],0`（未握手）
    /// 两条都**不**派发；只有"写失败 + 已握手"才 `0x459735 call 0x3e2830`。
    #[test]
    fn forward_event_dispatches_connection_removed_only_on_write_failure_while_connected() {
        // 写成功 + 已握手 ⇒ 不派发
        let tx = Arc::new(Mutex::new(Vec::new()));
        let mut h = handler(&tx, false, ConnectionState::Connected, 1);
        h.forward_event(b"{}");
        assert!(h.local_dispatch.is_empty());

        // 写失败 + 未握手 ⇒ 不派发
        let tx = Arc::new(Mutex::new(Vec::new()));
        let mut h = handler(&tx, true, ConnectionState::Connected, 0);
        h.forward_event(b"{}");
        assert!(h.local_dispatch.is_empty());

        // 写失败 + 已握手 ⇒ 派发一条 tag 6
        let tx = Arc::new(Mutex::new(Vec::new()));
        let mut h = handler(&tx, true, ConnectionState::Connected, 1);
        h.forward_event(b"{}");
        assert_eq!(h.local_dispatch.len(), 1);
        let text = String::from_utf8(h.local_dispatch[0].clone()).unwrap();
        assert!(text.starts_with("{\"ConnectionRemoved\":{\"connection_id\":7"), "实际 {text}");
    }

    /// `0x45987e movzx eax,byte [r14+0x15]` —— **原样**返回，不做合法性映射。
    #[test]
    fn get_connection_state_returns_the_raw_byte() {
        let tx = Arc::new(Mutex::new(Vec::new()));
        for s in [
            ConnectionState::Disconnected,
            ConnectionState::Connecting,
            ConnectionState::Connected,
            ConnectionState::Failed,
        ] {
            let h = handler(&tx, false, s, 0);
            assert_eq!(h.get_connection_state(), s);
        }
    }

    /// `0x459f46 cmp al,2 / jb`：状态 0/1 ⇒ 什么都不做（闩锁也不置）。
    #[test]
    fn tick_idles_for_state_below_2() {
        let tx = Arc::new(Mutex::new(Vec::new()));
        for s in [ConnectionState::Disconnected, ConnectionState::Connecting] {
            let mut h = handler(&tx, false, s, 0);
            h.tick();
            assert!(!h.latched, "0x459f48 jb 直接跳到 0x45a054，不置闩锁");
            assert_eq!(h.event_queue.borrow().len(), 0);
            assert!(h.requested_game_state.is_none());
        }
    }

    /// state == 2（`0x459f51 cmp eax,2 / jne`）：闩锁置 1 + 入队一条
    /// `NewConnectionRequest`（判别值 4，构造点 `0x504430` 写 `0x8000000000000004`）；
    /// 第二次 tick 因闩锁**不再入队**（`0x459f34 cmp byte [rdi+0x68],0 / jne`）。
    #[test]
    fn tick_connected_branch_latches_once_and_queues_new_connection_request() {
        let tx = Arc::new(Mutex::new(Vec::new()));
        let mut h = handler(&tx, false, ConnectionState::Connected, 0);
        h.tick();
        assert!(h.latched);
        assert_eq!(h.event_queue.borrow().len(), 1);
        h.tick();
        assert_eq!(h.event_queue.borrow().len(), 1, "闩锁 ⇒ 不重复入队");
        {
            let mut q = h.event_queue.borrow_mut();
            assert_eq!(q.send_buffer(), vec![Event::NewConnectionRequest]);
        }
    }

    /// state > 2（`0x459fc6`）：闩锁置 1 + 要求 `set_state(7)`
    /// （`0x45a030 mov esi,7`；接缝落 `requested_game_state`）。
    /// `modules` 为空 ⇒ `0x45a005 test rax,rax / je` 的 `unwrap`（Location 198:119）会 panic，
    /// 所以本测试必须先把模块容器搭起来。
    #[test]
    fn tick_failed_branch_requests_game_state_7() {
        let tx = Arc::new(Mutex::new(Vec::new()));
        let mut h = handler(&tx, false, ConnectionState::Failed, 0);
        let container = Rc::new(RefCell::new(GraphwarModuleContainer::new()));
        container
            .borrow_mut()
            .add_module(Rc::new(RefCell::new(GameStateManagerMarker)));
        h.modules = Rc::downgrade(&container);
        h.tick();
        assert!(h.latched);
        assert_eq!(h.requested_game_state, Some(GAME_STATE_ON_CONNECT_FAILED));
        assert_eq!(h.requested_game_state, Some(7), "0x45a030 mov esi,7");
        assert_eq!(h.event_queue.borrow().len(), 0, "Failed 臂不发 NewConnectionRequest");
    }

    /// `0x45a0dc call 0x3d86a0` 的读循环：完整帧 ⇒ 派发并继续；
    /// `0x8000…4a`（Ok(None)）⇒ 退出（`0x459f46` 那条 `cmp al,2` 无关，见 tick）。
    #[test]
    fn tick_read_loop_dispatches_frames_until_pending() {
        let tx = Arc::new(Mutex::new(Vec::new()));
        let mut h = handler(&tx, false, ConnectionState::Connected, 0);
        h.latched = true; // 只看读循环（闩锁已在别的测试里覆盖）
        // 两整帧 + 一个只有**头**的残帧（帧层会把残帧判成 Pending）
        let mut rx = Vec::new();
        for payload in [b"{\"A\":1}".as_slice(), b"{\"B\":2}".as_slice()] {
            rx.extend_from_slice(&(payload.len() as u16).to_be_bytes());
            rx.extend_from_slice(payload);
        }
        rx.extend_from_slice(&(9u16).to_be_bytes()); // 头到了、身子没到 ⇒ Pending
        {
            let mut slot = h.stream.lock().unwrap();
            slot.as_mut().unwrap().stream.rx = rx;
        }
        h.tick();
        assert_eq!(h.local_dispatch.len(), 2, "两整帧各派发一次");
        assert_eq!(h.local_dispatch[0], b"{\"A\":1}".to_vec());
        assert_eq!(h.local_dispatch[1], b"{\"B\":2}".to_vec());
    }
}
