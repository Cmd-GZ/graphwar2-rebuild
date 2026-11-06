//! 原版 `graphwar_backend.rs` 的复原（**联机层**，不是广告——`docs/19-android.md` 更正过）。
//!
//! 证据口径与 `graphwar_client_connection_handler.rs` 相同：
//!
//! * **arm64 2.0.60 未 strip**：`lib/arm64-v8a/libgraphwarrust.so`，`.symtab` 直接给名字；
//!   本文件里的 arm64 地址 = ELF 虚地址。
//! * **x86 steam 2026-08-28**：stripped，只有 6 个函数有 `panic::Location` 唯一锚
//!   （`fixtures/name_to_addr.tsv`），其余 12 个是"未知/歧义"。
//! * 地址 = `objdump` 口径；Ghidra = `+0x100000`。
//!
//! # 这个模块是什么
//!
//! **后端总开关**：它自己不懂协议，只负责"把哪个子模块装上/拆掉"，以及每帧把活着的
//! 子模块挨个 `process()` 一遍。四个子模块全都放在它肚子里（**内联字段，不是堆对象**）：
//!
//! ```text
//! +0x110  GraphwarLanBroadcasterV        LAN(UDP IPv6 组播) 房间广播
//! +0x1e0  Option<GraphwarGameClient>     客户端（内含 graphwar_client_connection_handler）
//! +0x220  Rc<RefCell<GraphwarModuleContainer>>
//! +0x228  Option<GraphwarServerBrokerHandler>   全局房间列表（TCP broker）
//! ```
//!
//! # 四个问题的答案（(b) 地址、(d) LAN vs Global）
//!
//! * **(b) 地址从哪来**：本模块有**两条起点**，都给出**已解析的 `SocketAddr`**：
//!   局域网/本机 = `SocketAddr::V6(::1, server_port)` 在栈上逐字段拼出来；
//!   全局/直连 = 调用方传进来的 `&SocketAddr`，它是 `SocketAddr::from_str` 的结果
//!   （⇒ **只认 IP 字面量**，IPv6 要写 `[::1]:35245`，**域名不行**——`from_str` 不做 DNS）。
//!   全局地址的文本来自 `https://graphwar.com/graphwar_II_ip.txt`。
//! * **(d) LAN 与 Global**：不是"二选一 bool"，而是**两个独立的 `Option` 字段**，
//!   由房主同时/分别装上：`start_lan_broadcaster()` 装 `+0x110`，
//!   `start_server_broker_handler()` 装 `+0x228`。"看哪一路"是大厅控件的事
//!   （`GraphwarLanVBox` 的 `show_global`），不是本模块的事；本模块只保证
//!   两路各自的 `get_game_info()` 都能被大厅取走（`IVBoxContainer::process`
//!   在 arm64 `0x450c20` 里两个都调）。
//!
//! ⚠️ 按 `AGENTS.md` §0.2：本文件的 `cargo test` 是**回归保护，不是复原证据**。

pub use crate::graphwar_client_connection_handler::Link;

/// 原版模块名。
pub const MODULE: &str = "graphwar_backend";

/// 源文件名（`.data.rel.ro` 里 12 条 `panic::Location` 指向它，arm64 `0x18a47c`）。
pub const SOURCE_FILE: &str = "src/graphwar_backend.rs";

// ---------------------------------------------------------------------------
// 0. 函数表
// ---------------------------------------------------------------------------

/// 一个已归属的函数（与本 crate 另一个模块同构，避免两套 `Link`）。
#[derive(Debug, Clone, Copy)]
pub struct Func {
    pub name: &'static str,
    pub arm64: u64,
    pub arm64_size: u32,
    pub x86: Option<u64>,
    pub link: Link,
}

/// 本模块的 **18 个方法**（`docs/07` 记的"15 个方法名"是早先只数到 `impl` 列表的前半段；
/// arm64 `.symtab` 里 `graphwar_backend` 下的 `FUNC` 是这 18 个，另加 3 个
/// `.llvm.*` 的重复实例化与若干 `drop_in_place` 胶水）。
///
/// x86 锚定情况（`fixtures/name_to_addr.tsv`）：
///
/// | 有唯一锚 | 歧义 | 未知（不引用 Location） |
/// |---|---|---|
/// | `process` 79:51、`start_client_to_replay` 173:37/173:99、`subscribe_to_event` 344:28、`resubscribe_to_events` 350:70 | `new`、`start_client_to_local_server`、`start_client_to_remote_server`、`subscribe_to_event_with_id` | 其余 10 个 |
pub const FUNCTIONS: &[Func] = &[
    Func { name: "GraphwarBackend::setup_level", arm64: 0x406b4c, arm64_size: 88, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::stop_client", arm64: 0x406ba4, arm64_size: 104, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::stop_server", arm64: 0x406c0c, arm64_size: 88, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::start_server", arm64: 0x406c64, arm64_size: 216, x86: None, link: Link::NoAnchor },
    // button_pressed 的唯一 Location 落在 graphwar_game_client.rs:186（内联），故本文件里无锚。
    Func { name: "GraphwarBackend::button_pressed", arm64: 0x406d3c, arm64_size: 324, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::subscribe_to_event", arm64: 0x406e80, arm64_size: 488, x86: Some(0x514b40), link: Link::Unique },
    Func { name: "GraphwarBackend::stop_lan_broadcaster", arm64: 0x407068, arm64_size: 172, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::resubscribe_to_events", arm64: 0x407114, arm64_size: 268, x86: Some(0x514d80), link: Link::Unique },
    Func { name: "GraphwarBackend::start_lan_broadcaster", arm64: 0x407220, arm64_size: 144, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::start_client_to_replay", arm64: 0x4072b0, arm64_size: 428, x86: Some(0x514f20), link: Link::Unique },
    Func { name: "GraphwarBackend::get_next_clip_checkpoint", arm64: 0x40745c, arm64_size: 156, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::stop_server_broker_handler", arm64: 0x4074f8, arm64_size: 136, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::subscribe_to_event_with_id", arm64: 0x407580, arm64_size: 264, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::start_server_broker_handler", arm64: 0x4077bc, arm64_size: 140, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::start_client_to_local_server", arm64: 0x407848, arm64_size: 852, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::start_client_to_remote_server", arm64: 0x407b9c, arm64_size: 364, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::new", arm64: 0x407d08, arm64_size: 488, x86: None, link: Link::NoAnchor },
    Func { name: "GraphwarBackend::process", arm64: 0x407ef0, arm64_size: 1392, x86: Some(0x515dc0), link: Link::Unique },
];

/// 本模块在 `.data.rel.ro` 里的全部 12 条 `panic::Location`。
///
/// `process` 占了 7 条（79/86/87/88/90/92/93）——因为它在同一屏里连续做 4 次
/// `get_module()` + `borrow_mut()`，每一处都可能 panic。其余 5 条在
/// `start_client_to_replay`（173×2）、`subscribe_to_event`（344）、
/// `resubscribe_to_events`（350）、`subscribe_to_event_with_id`（359）。
pub const LOCATIONS: &[(u32, u32)] = &[
    (79, 51),
    (86, 55),
    (87, 52),
    (88, 67),
    (90, 58),
    (92, 58),
    (93, 58),
    (173, 37),
    (173, 99),
    (344, 28),
    (350, 70),
    (359, 49),
];

// ---------------------------------------------------------------------------
// 1. 结构体布局
// ---------------------------------------------------------------------------

/// `GraphwarBackend` 的字段偏移（arm64 观测）。
///
/// 夹逼来源：`new`（只写）、`process`（读 4 个子对象）、`start_client_to_local_server`
/// （读 `+0x60` 当守卫）、`start_client_to_remote_server`（写 `+0x1e0`）。
pub struct BackendField;

impl BackendField {
    /// `u64`：模式/标记。`process` 里 `cmp x8,#2`（arm64 `0x407f20`）决定要不要跑回放播放器。
    /// 2 的语义**未知**（只有"==2 就跳过 ReplayPlayer::process"这一条观测）。
    pub const MODE: usize = 0x00;
    /// `u64`：`start_client_to_local_server` 的入口守卫，`==4` 直接返回
    /// （`0x407864 cmp x8,#4; b.eq`）。4 的语义**未知**。
    pub const STATE: usize = 0x60;
    /// LAN 广播者**内联**在这里（`process` 里 `add x0,x22,#0x110` 后直接调
    /// `GraphwarLanBroadcasterV::process`）。
    pub const LAN_BROADCASTER: usize = 0x110;
    /// `Rc<RefCell<...>>`：交给 `GraphwarGameClient::new` 的那个共享句柄
    /// （`0x40787c ldr x9,[x19,#0x1d0]` + 强计数 +1 ⇒ 是 `Rc`/`Arc`）。
    pub const SHARED: usize = 0x1d0;
    /// `Option<GraphwarGameClient>`（内联）。
    pub const CLIENT: usize = 0x1e0;
    /// `Rc<RefCell<GraphwarModuleContainer>>`（`process` 里 4 次 `get_module` 都从它取）。
    pub const MODULES: usize = 0x220;
    /// `Option<GraphwarServerBrokerHandler>`（内联）。
    pub const BROKER: usize = 0x228;
}

/// 房间名默认值（13 字节）。
///
/// 真值：`new` 里 `mov w0,#0xd` 分配 13 字节后从 `.rodata` 拷 `"Graphwar Game"`
/// （arm64 `0x19ca23`，`ldr x9` + `ldur x8,[x8,#5]` = 8+5 字节）。与抓包
/// `{"room_name":"Graphwar Game"}` 一致。
pub const DEFAULT_ROOM_NAME: &str = "Graphwar Game";

/// 本机（局域网）服务器地址的 IP 部分：`::1`。
///
/// 真值：`start_client_to_local_server` 在栈上拼 `SocketAddr`——先 `strh #1`
/// （枚举变体 1 = `V6`），再 `stur q0,[sp,#0xc]` 写入从 arm64 `0x197680` 取出的
/// **16 字节全零+末尾 1**（即 `::1`），`flowinfo`/`scope_id` 清 0，端口写 `strh w1`
/// （`get_server_port()` 的返回值）。
///
/// ⇒ **本机联机走 IPv6 环回，不是 `127.0.0.1`**。这与抓包里
/// `"address":"[::1]:35245"` 完全一致（`docs/09` §1.1）。
pub const LOCAL_SERVER_IP: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];

/// `SocketAddr` 枚举里 `V6` 的判别值（`strh #1`）。
pub const SOCKET_ADDR_V6: u16 = 1;

/// LAN 广播用的 IPv6 组播组 `ff02::1`（链路本地 all-nodes）。
///
/// 真值：`GraphwarLanBroadcasterV::set_room_info` 与 `::start` 都调
/// `IpAddr::from_str`（arm64 `0x3ddf08` / `0x3de1c4`），参数字面量在
/// `0x19be2b`：`"::1"`（3 字节）+ `"ff02::1"` 紧邻。与 `docs/09` §6.6.3 的实测
/// （客户端在 LAN 大厅绑 `UDP 0.0.0.0:42968`，往 `ff02::1%lo:42968` 能灌进去）一致。
pub const LAN_MULTICAST: &str = "ff02::1";

/// LAN 广播者的本地地址常量 `::1`（同上，3 字节）。
pub const LAN_LOOPBACK: &str = "::1";

/// 全局服务器地址的文本来源（39 字节）。
///
/// 真值：一个 `thread::spawn` 闭包（arm64 `0x48e130`）里
/// `mov w1,#0x27`（= 39）后 `bl reqwest::blocking::get`；失败走
/// `"Failed to get global server address."`（36 字节，日志）。同 `docs/09` §6.5 的结论。
pub const GLOBAL_SERVER_URL: &str = "https://graphwar.com/graphwar_II_ip.txt";

/// 拿到的地址文本缓存文件（Godot 用户目录）。
///
/// 真值：`0x1a16ca` 的字面量 `"user://local_ip.config"`，被
/// `godot_core::tools::save_load::{load,save}` 用（`IControl::process` 里成对出现）。
/// 缓存格式（几行、有无换行）**未知**——没读到过真实文件。
pub const LOCAL_IP_CACHE: &str = "user://local_ip.config";

/// 全局地址拿不到时的兜底（14 字节）。
///
/// 真值：`"127.0.0.1:8080"`（arm64 `0x1a067a`）。它被 `SocketAddr::from_str`
/// 解析（`0x487fe8` / `0x4880e0`），因此在**没有** `--broker_address`、也没能
/// 从 HTTPS 取到地址时生效。⚠️ 注意**不是**本机对局地址：对局走 `::1`（见
/// [`LOCAL_SERVER_IP`]），`127.0.0.1:8080` 只出现在这个兜底/调试点。
pub const FALLBACK_SOCKET_ADDR: &str = "127.0.0.1:8080";

/// 全局地址取不到时的日志文案（36 字节，arm64 `0x1a06f4`）。
pub const MSG_NO_GLOBAL_ADDR: &str = "Failed to get global server address.";

// ---------------------------------------------------------------------------
// 2. process()：每帧装配顺序（纯判定）
// ---------------------------------------------------------------------------

/// `process()` 一帧里会对哪个子对象做什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStep {
    /// `GraphwarReplayPlayer::process(&mut self, &mut self+0x1e0)`——**只有**模式 != 2 才跑。
    ReplayPlayer,
    /// `GraphwarGameClient::process(&mut self+0x1e0)`。
    GameClient,
    /// `GraphwarLanBroadcasterV::set_room_info(...)`——房主时把房间信息刷进广播。
    LanRoomInfo,
    /// `GraphwarLanBroadcasterV::process(&mut self+0x110)`。
    LanBroadcaster,
    /// `GraphwarServerBrokerHandler::process(&mut self+0x228)`（尾调用）。
    Broker,
}

/// 算出这一帧要做哪些事、按什么顺序。
///
/// 真二进制（arm64 `0x407ef0`）的结构就是这 5 步，顺序固定：
///
/// ```text
/// if client.is_some() {                      // 0x407f18 ldr x8,[x0,#0x1e0]; cbz
///     if mode != 2 { ReplayPlayer::process } // 0x407f20 cmp x8,#2; b.eq 跳过
///     GameClient::process                    // 0x407f38
/// }
/// if lan.is_some() {                         // 0x407f48 cmp x8,#-0x8000…
///     if backend.state != 4 && lan_ready {   // 0x407f54 cmp x8,#4 / 0x407f60 cbz
///         if let Some(port) = server_port() { set_room_info(...) }
///     }
///     LanBroadcaster::process                // 0x408224
/// }
/// if broker.is_some() { Broker::process }    // 0x40822c -> 尾调用 0x48a078
/// ```
///
/// `lan_ready` 对应 `ldr x8,[x25,#0x180] != 0`（`0x407f5c`）——那个字段是什么**未知**，
/// 只知道"为 0 就不刷新广播房间信息"。
pub fn process_steps(
    mode: u64,
    has_client: bool,
    has_lan: bool,
    backend_state: u64,
    lan_ready: bool,
    has_broker: bool,
) -> Vec<ProcessStep> {
    let mut steps = Vec::new();
    if has_client {
        if mode != 2 {
            steps.push(ProcessStep::ReplayPlayer);
        }
        steps.push(ProcessStep::GameClient);
    }
    if has_lan {
        if backend_state != 4 && lan_ready {
            steps.push(ProcessStep::LanRoomInfo);
        }
        steps.push(ProcessStep::LanBroadcaster);
    }
    if has_broker {
        steps.push(ProcessStep::Broker);
    }
    steps
}

// ---------------------------------------------------------------------------
// 3. 上层怎么选：三个"造客户端"入口 + 两个"造服务端/广播"入口
// ---------------------------------------------------------------------------

/// 造客户端的三条路（都是 `GraphwarGameClient::new` + `::connect` 的组合）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientTarget {
    /// `start_client_to_local_server`：连本机 `::1:<server_port>`。
    Local { server_port: u16 },
    /// `start_client_to_remote_server(&SocketAddr)`：连一个**已经解析好**的地址
    /// （LAN 列表里点某一行，或全局服务器）。
    Remote,
    /// `start_client_to_replay`：回放，不需要 socket。
    Replay,
}

impl ClientTarget {
    /// 这条路要不要真的建 TCP。
    pub const fn opens_socket(self) -> bool {
        !matches!(self, ClientTarget::Replay)
    }

    /// 地址从哪来（给"自己写客户端"的人看的一行说明）。
    pub const fn address_source(self) -> &'static str {
        match self {
            ClientTarget::Local { .. } => "SocketAddr::V6(::1, GraphwarGameServer::get_server_port())",
            ClientTarget::Remote => "调用方传入的 &SocketAddr（SocketAddr::from_str 的结果）",
            ClientTarget::Replay => "无（读本地录像）",
        }
    }
}

/// `start_client_to_local_server` 的入口守卫：`self+0x60 == 4` 直接返回
/// （arm64 `0x407864`）。以及 `get_server_port()` 为 `None` 时也返回
/// （`0x407870 tbz w0,#0`）。
///
/// ⇒ 本机联机**必须先起服务端**；端口是现取的，不是常量。
pub const fn local_client_allowed(backend_state: u64, server_port: Option<u16>) -> bool {
    backend_state != 4 && server_port.is_some()
}

/// 两条"房主"路：装 LAN 广播 / 装 broker。
///
/// 它们**互不排斥**——`GraphwarRootNode::process_button_pressed`（arm64 `0x3a6d48`）
/// 在同一个分支里连续调 `start_lan_broadcaster` + `start_server_broker_handler`
/// （`0x3a79e8` / `0x3a7a28`，另有 4 组重复点），说明原版"开房"就是两路一起挂。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostChannel {
    /// `+0x110`：UDP IPv6 组播，给**同一个局域网**看。
    LanBroadcast,
    /// `+0x228`：TCP 到 broker，给**互联网**看。
    Broker,
}

impl HostChannel {
    pub const fn field_offset(self) -> usize {
        match self {
            HostChannel::LanBroadcast => BackendField::LAN_BROADCASTER,
            HostChannel::Broker => BackendField::BROKER,
        }
    }

    /// 传输层（`docs/09` §1.1 / §6.2 已实测）。
    pub const fn transport(self) -> &'static str {
        match self {
            HostChannel::LanBroadcast => "UDP / IPv6 组播 ff02::1 / 明文 JSON / 无长度前缀（换行分帧）",
            HostChannel::Broker => "TCP / u16 大端长度 + JSON",
        }
    }
}

// ---------------------------------------------------------------------------
// 4. 自检
// ---------------------------------------------------------------------------
