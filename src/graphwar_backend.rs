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
    /// `u64`：**回放播放器槽的判别字**（槽本体 `+0x00..0x60`）。
    ///
    /// T103 定死（此前只写"模式/标记，2 的语义未知"）：
    /// * `stop_client` 收尾 `0x406bd4 ldr x8,[x19]` / `cmp x8,#2` / `b.eq` ⇒ **2 = 槽空**，
    ///   `0x406be8 str x8(=2),[x19]` 把槽清成"空"；
    /// * `stop_client` 中间 `0x406be0 bl drop_in_place<GraphwarReplayPlayer>`（实参 = `self+0x00`）
    ///   ⇒ `+0x00` 起就是播放器对象本体，判别字与对象首字段共址；
    /// * `process` 里 `cmp x8,#2`（arm64 `0x407f20`）决定要不要跑 `ReplayPlayer::process`
    ///   = **同一个判据**（槽空就跳过），不是另一个"MODE"语义。
    pub const MODE: usize = 0x00;
    /// `+0x60`：**内联 `GraphwarGameServer` 槽的判别字**（槽本体 `+0x60..0xe0`，0x80 字节）。
    ///
    /// ⚠️ **T103 更正**：旧注释把它叫 `STATE`（"`start_client_to_local_server` 的入口守卫，4 的语义未知"）。
    /// 本轮由三个写点定死它是 server 槽的判别字、**4 = 槽空**：
    /// * `stop_server` `0x406c1c ldr x8,[x19,#0x60]!` + `cmp x8,#4` ⇒ 空就直接返回；
    ///   `0x406c48 str x8(=4),[x19]` 清空；`0x406c40 bl drop_in_place<GraphwarGameServer>`（实参 `self+0x60`）；
    /// * `start_server` `0x406c9c` 同一处 pre-index 读判别字、`0x406cc8` drop 旧 server、
    ///   `0x406ccc`–`0x406ce8` 把 **0x80 字节**整块写进 `self+0x60`（四组 `stp q`，含判别字本身）；
    /// * `setup_level` `0x406b4c ldr x8,[x0,#0x60]!` + `cmp x8,#4` ⇒ 没有 server 就返回。
    /// ⇒ 一个字段被 0x80 字节对象覆盖 ⇒ 它**在对象内部**，只能是枚举判别字。
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
    /// `+0x228..+0x260` = `GraphwarServerBrokerHandler` 本体 **0x38 = 56 字节**（内联，不是堆对象）。
    /// 真值：`start_server_broker_handler` `0x407800`–`0x40781c`（3 条 `ldur q` + 1 条 `ldr x9`，
    /// 从 `sp+0x8` 拷 0x38 字节到 `self+0x228`）；`GraphwarBackend::new` `0x407e14`/`0x407e24`/`0x407e28`
    /// 写的是同一段（`stp q0,q1` + `str q0,[x8,#0x20]` + `str x9,[x21,#0x258]`）。
    pub const BROKER_SLOT_SIZE: usize = 0x38;
    /// `+0x230`（**broker handler 对象内 `+0x8`**）：那个 `HashMap` 的 hashbrown 表。
    /// `stop_server_broker_handler` `0x407540`/`0x407560` 的 `add x0,x19,#0x230` + `bl RawTable::drop`。
    /// 对象布局 ⇒ `+0x228` 一个 8 字节指针（Arc 指针）+ 48 字节 `HashMap`
    /// （hashbrown `RawTable` 32 + `RandomState` 16 = 48）。
    pub const BROKER_MAP: usize = 0x230;
    /// `+0x188`（**LAN 广播者对象内 `+0x78`**）：hashbrown 表。
    /// `stop_lan_broadcaster` 的 `0x4070a4 add x0,x19,#0x188` + `0x4070a8 bl RawTable::drop` 就是它。
    pub const LAN_TABLE: usize = 0x188;
    /// `+0x1c8`（**LAN 广播者对象内 `+0xb8`**）：原始 socket fd（`i32`）。
    /// 三处指令定死：`GraphwarLanBroadcasterV6::new` arm64 `0x3de0f0 str w8(=-1),[x19,#0xb8]` 写初值 −1；
    /// `stop_lan_broadcaster` `0x40708c ldr w0,[x19,#0x1c8]` / `0x407090 cmn w0,#0x1`（−1 就不 close）/
    /// `0x407098 bl close@plt` / `0x4070a0 str w8(=-1),[x19,#0x1c8]`（关完立刻写回 −1）。
    pub const LAN_FD: usize = 0x1c8;
    /// `+0x110..+0x1d0` = `GraphwarLanBroadcasterV6` 本体 **0xC0 = 192 字节**（内联，不是堆对象）。
    /// 真值：`start_lan_broadcaster` `0x407258`–`0x407284` 共 8 组 `ldp q/stp q`，
    /// 把栈上 `sp+0x00..sp+0xc0` 整块搬到 `self+0x110..self+0x1d0`。
    pub const LAN_SLOT_SIZE: usize = 0xc0;
    /// `+0x48` / `+0x50`（**内联回放播放器对象内**）：clip 检查点表的 `(ptr, len)`。
    /// 见 [`GraphwarBackend::clip_checkpoints`] / [`GraphwarBackend::get_next_clip_checkpoint`]。
    pub const CLIP_CHECKPOINTS: usize = 0x48;
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
///
/// ⚠️ **T103 更正第一个实参的名字**：`+0x60` 不是抽象的"state"，它是**内联
/// `GraphwarGameServer` 槽的判别字**、`4 = 槽空`（三个写点见 [`BackendField::STATE`]）。
/// ⇒ 这条守卫的真实语义是"**本机联机要求后端里已经有一个 server**"，
/// 与 `setup_level`/`stop_server` 用的是**同一个字段同一个值**。
/// 参数名保留 `backend_state` 只为不改调用方；语义按上面这条读。
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

// ---------------------------------------------------------------------------
// 5. 回放会话：`start_client_to_replay` / `resubscribe_to_events` /
//    `subscribe_to_event_with_id`（T95 落码）
// ---------------------------------------------------------------------------
//
// ⚠️ 这三条是**函数边界已定、语义分层**的落码：
//   * 名字：`start_client_to_replay` / `resubscribe_to_events` 来自 `fixtures/name_to_addr_v2.tsv`
//     的 v2 三向唯一锚（android+win+win_old 同名），**是硬的**；
//     `subscribe_to_event_with_id` 在 v2 是"未知"（arm64/win 侧有名字但**没有 Location 锚**），
//     x86 侧是 `fixtures/hybrid_names.tsv` 的高置信同源预测（`0x515320`，score 0.7115 / margin 0.2116）
//     ⇒ 档位只能到 **推断**，本文件里明确标注。
//   * 这三条都**直接操作 gdext 对象图**（`GraphwarGameClient` / `GraphwarEventHandler` / 模块容器），
//     本仓库 headless 侧没有那套对象图 ⇒ 落码到"可测的装配顺序 + 全部立即数/偏移"，
//     并把**缺什么**写在每条下面（AGENTS.md §0.3：不许把推断写成结论）。
//
// ## 三个 x86 vaddr 与 FDE
//
// | 函数 | x86 vaddr / FDE | arm64 vaddr / size | 名字来源 |
// |---|---|---|---|
// | `resubscribe_to_events` | **`0x514d80`** / `0x112` | `0x407114` / `0x10c` | v2 唯一锚（硬） |
// | `start_client_to_replay` | **`0x514f20`** / `0x213` | `0x4072b0` / `0x1ac` | v2 唯一锚（硬） |
// | `subscribe_to_event_with_id` | **`0x515320`** / `0xc9` | `0x407580` / `0x108` | hyb 预测（**推断**） |
//
// ## `resubscribe_to_events`（x86 `0x514d80`）：逐条重订阅
//
// ```text
// 0x514d8b  mov    rdx, [rdi+0x1d8]      ; 订阅登记表 = Rc<RefCell<HashMap<…>>>
// 0x514d92  mov    rax, [rdx+0x10]       ; RefCell 借用标记（0x7fff… 上限判 panic）
// 0x514dac  inc    rax                  ; borrow()
// 0x514db3  mov    r15, [rdx+0x18]       ; hashbrown 表的数据区
// 0x514dbb  mov    r12, [rdx+0x30]       ; 元素条数
// 0x514dc8  pmovmskb eax, xmm0          ; ← 控制字节扫描（hashbrown 迭代）
// 0x514e18  tzcnt  ecx, eax             ; 取下一个有效桶
// 0x514e2e  mov    rsi, [rbx+0x1e0]      ; Option<GraphwarGameClient>（+0x1e0，内联）
// 0x514e38  cmovne rsi, rbp             ; non-null ⇒ &mut self.client（rbp = rbx+0x1e0）
// 0x514e43  mov    rdx, [r15+r8-0x20]    ; 登记项里的 (ptr, len)
// 0x514e48  mov    rcx, [r15+r8-0x18]
// 0x514e4d  mov    r8d, [r15+r8-0x30]    ; 键 = 事件 id（u32）
// 0x514e52  call   0x515320              ; = subscribe_to_event_with_id
// ```
// arm64 同一段用 `cmge v0.8b` + `and #0x8080…` + `rbit/clz` 扫控制字节
// （`0x407164`..`0x4071a4`），语义相同 ⇒ **迭代的是 hashbrown 表，不是 Vec**。
//
// ⚠️ **值的确切类型未定**（只读到 (ptr,len) 一对）。我方建模为 `u32 → String`（事件名），
// 因为 `subscribe_to_event_with_id` 的实参形状是 `(name_ptr, name_len, id)`。

/// 回放客户端槽（原版 `GraphwarBackend +0x1e0` 的 `Option<GraphwarGameClient>`，**内联**，不是堆对象）。
///
/// 我方只留"已装 / 没装"和"注册进来的按 id 订阅"两件事 —— 真正的 gdext 对象图不在本 crate。
#[derive(Debug, Default, Clone)]
pub struct ReplayClientSlot {
    /// `GraphwarEventHandler` 在内联客户端的 **`+0x08`**（arm64 `0x4075e0 add x0,x23,#8`、
    /// `inject_events` 里的 `0x3d8f14 add x0,x21,#8`）—— 这里只记"已经注册过的按 id 订阅"。
    pub registered: Vec<(u32, String)>,
}

/// `GraphwarBackend` 里与回放会话有关的那部分状态。
///
/// 字段偏移全部有地址（逐条见各字段注释）；**没有**证据的字段一律不放。
#[derive(Debug, Default, Clone)]
pub struct GraphwarBackend {
    /// `+0x1d0`：交给 `GraphwarGameClient::new` 的共享句柄强引用计数
    /// （`start_client_to_replay` 的 `0x514f4f inc qword [rdi+0x1d0]`）。
    pub shared_refs: usize,
    /// `+0x00..0x60`：回放播放器槽（`GraphwarReplayPlayer` 本体就是 0x60 字节，
    /// 见 `graphwar_replay_player::GraphwarReplayPlayer`；`0x3d9b00` 起 `new` 写它）。
    pub replay: Option<crate::graphwar_replay_player::GraphwarReplayPlayer>,
    /// `+0x00` 那个 `u32` 判别字与播放器首字段共址：**`== 2` 表示"槽里不是活的播放器"**
    /// （`0x514fe8 cmp dword [rbx],2` 决定要不要 `drop_in_place<GraphwarReplayPlayer>`；
    ///  `0x515041` 是同一判据的第二次读）。**2 的来源与语义：未知**。
    pub replay_slot_tag: u32,
    /// `+0x1d8`：按 id 的订阅登记表（原版 `Rc<RefCell<HashMap<u32, …>>>`；我方用 BTreeMap
    /// 只为了确定性，迭代顺序在原版里是哈希序 —— 这一差异不影响语义）。
    pub subscriptions: std::collections::BTreeMap<u32, String>,
    /// `+0x1e0`：`Option<GraphwarGameClient>`（内联）。
    pub client: Option<ReplayClientSlot>,
    /// `+0x60`：内联 `GraphwarGameServer` 槽（0x80 字节；判别字 4 = 空，见 [`BackendField::STATE`]）。
    ///
    /// T103 落码 `setup_level` / `start_server` / `stop_server` 时装它；
    /// 槽里只有 [`ServerSlot`] 记的那三件事有指令证据。
    pub server: Option<ServerSlot>,
    /// `start_client_to_replay` 造出来的 `SocketAddr`（`([::1], 端口)`；见 [`REPLAY_CLIENT_ADDR`]）。
    pub client_addr: Option<([u8; 16], u16)>,
    /// 内联回放播放器对象的 **`+0x48` = 表指针 / `+0x50` = 表长**：**clip 检查点**表（升序 `u32`）。
    ///
    /// ⚠️ 这**不是** [`crate::graphwar_replay_player::GraphwarReplayPlayer::checkpoints`]（那个在
    /// `+0x28/+0x30/+0x38`）：两个表是不同的东西，判别依据是 arm64 侧两条函数的取址偏移 ——
    /// `go_to_next_checkpoint`（`0x3d929c`）读 `+0x30`/`+0x38` + tick `+0x58`，
    /// 而 [`Self::get_next_clip_checkpoint`]（`0x40745c`）读 `+0x48`/`+0x50`。
    ///
    /// 原版这里是**两字的切片**（`(ptr, len)`；`0x407468 ldp x8,x9,[x0,#0x48]` 只读两个字，
    /// 且 `+0x58` 已经被 tick 占掉）⇒ 更可能是 `Box<[u32]>` / `&'static [u32]`；
    /// 我方用 `Vec<u32>` 承载，只读 ⇒ 语义等价。
    ///
    /// 播放器结构体本身（`src/graphwar_replay_player.rs`）**不在 T106 的写范围** ⇒ 这个字段暂挂在本结构上，
    /// 偏移以注释为准；表值类型 `u32` 来自 `0x4074a4 ldr w14,[x8,x13,lsl #2]`（4 字节元素 = tick 口径）。
    pub clip_checkpoints: Vec<u32>,
    /// `+0x110`：**内联 `GraphwarLanBroadcasterV6`**（0xC0 字节；空槽编码见 [`LAN_SLOT_EMPTY`]）。
    pub lan_broadcaster: Option<LanBroadcasterSlot>,
    /// `+0x228`：**内联 `GraphwarServerBrokerHandler`**（0x38 字节；空 = 首字 0，见 [`BrokerSlot`]）。
    pub broker: Option<BrokerSlot>,
}

/// `start_client_to_replay` 造的那个 `SocketAddr`：**`[::1]:0`**。
///
/// 逐指令（x86）：
/// ```text
/// 0x514f2c movups xmm0,[0x19d440]     ; 16 字节 = 00…01（::1）
/// 0x514f33 movups [rsp+0x54],xmm0
/// 0x514f38 mov    qword [rsp+0x64],0  ; 把 port/flowinfo 一起清 0 ⇒ 端口 = 0
/// 0x514f41 mov    word [rsp+0x6c],0   ; scope_id = 0
/// 0x514f48 mov    word [rsp+0x50],1   ; SocketAddr 枚举判别 1 = V6
/// ```
/// ⇒ 回放会话**不建 TCP**（与 `ClientTarget::Replay::opens_socket() == false` 一致），
/// 地址只是个占位；端口 0 不是我方猜的，是 `qword 0` 覆盖出来的。
pub const REPLAY_CLIENT_ADDR: ([u8; 16], u16) = ([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], 0);

/// `start_client_to_replay` 的装配步骤（顺序 = 反汇编里的调用点顺序，**不是**我方排的）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayStartStep {
    /// `GraphwarGameClient::new`（x86 `0x517690` / arm64 `0x409c70`，经槽 `0x9bc458` 间接调用）：
    /// 实参 = `self+0x1d0` 的共享句柄（先 `inc`）+ `[::1]:0` 的 `SocketAddr`。
    NewClient,
    /// `GraphwarReplayPlayer::new`（x86 `0x3d6d70` / arm64 `0x3d9878`）。
    NewReplayPlayer,
    /// 旧槽若是活的播放器（`tag != 2`）先 `drop_in_place<GraphwarReplayPlayer>`（`0x514fed`）。
    DropOldPlayer,
    /// 装新客户端：先 `drop_in_place<GraphwarGameClient>`（`0x514fb3`），再整块搬 0x50 字节进去。
    InstallClient,
    /// 装新播放器：整块搬 0x60 字节（`0x514ff5`..`0x515036`，5 个 `movups`）。
    InstallPlayer,
    /// `resubscribe_to_events`（`0x51503c call 0x514d80`）。
    Resubscribe,
    /// `GraphwarReplayPlayer::go_to_next_checkpoint`（`0x515052 call 0x3d68b0`，实参 = self 与 client 槽）。
    GoToNextCheckpoint,
}

/// 订阅 id 的全局分配器 —— 原版是 `.data`/`.bss` 里 `0x8b4378` 那个原子计数器
/// （arm64 `0x406e84 adrp 0x8b4000` + `0x406eb8 add #0x378` + `0x406ebc ldxr` / `0x406ec4 stxr`）。
///
/// ⚠️ **初值未知**：没有查到它在哪初始化（是否由某个构造器写 0）⇒ 只保证"每次调用 +1"，
/// 不保证第一个 id 是 0。单测因此只断言**相对**关系。
static NEXT_SUBSCRIPTION_ID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// 内联 `GraphwarGameServer` 槽（`BackendField::STATE` = `+0x60`，0x80 字节）里**我方建模**的那部分。
///
/// 只有 `GraphwarGameServer::new` 的实参与"`start()` 是否已调用"两件事有指令证据：
/// arm64 `0x406c8c bl GraphwarGameServer::new`（实参 `w1`,`w2`,`wzr`）、`0x406c94 bl …::start`。
/// 槽里其余字节（0x80 − 12）本轮**没有**读出来 ⇒ 不建模（AGENTS.md §0.3：不猜）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSlot {
    /// `GraphwarGameServer::new(w1, w2, wzr)` 的前两个实参（arm64 `0x406c80`–`0x406c88`：
    /// `mov w0,w1` / `mov w1,w2` / `mov w2,wzr` ⇒ **第三个实参恒为 0**）。
    pub arg0: u32,
    pub arg1: u32,
    /// `start()` 在**搬进槽之前**就调过了（`0x406c94` 在 `0x406ccc` 的整块写入之前）。
    pub started: bool,
}

/// 槽空的判别字（`+0x00` 回放播放器槽 = `2`、`+0x60` 服务器槽 = `4`）。
///
/// 值来自指令立即数（`0x406be8 mov w8,#0x2` / `0x406c48 mov w8,#0x4`），
/// 不是猜的；`==2`/`==4` 的比较点见 [`BackendField::MODE`] / [`BackendField::STATE`]。
pub const PLAYER_SLOT_EMPTY: u32 = 2;
pub const SERVER_SLOT_EMPTY: u32 = 4;

/// LAN 广播者槽（`+0x110`）的**空**编码：`0x8000_0000_0000_0001`。
///
/// 三个写点/读点（arm64，全在 T106 本轮读到的指令里）：
/// * `GraphwarBackend::new` `0x407e10 mov x9,#-0x7fffffffffffffff` + `0x407e1c str x9,[x21,#0x110]`
///   ⇒ **初值就是这个值**（⇒ 它是"空"的编码，不是构造出来的对象）；
/// * `stop_lan_broadcaster` `0x407080`/`0x407084`（`cmp x20,x21` / `b.eq` 早退）与 `0x4070cc`（写回）；
/// * `start_lan_broadcaster` `0x407234`/`0x407238`/`0x40723c`（`cmp x8,x9` / `b.ne` 早退）。
///
/// ⚠️ **这个哨兵由什么机制产生（`Option<…>` 的 niche？哪个字段承载？）本轮未知** ——
/// 已知的只是"它是 `+0x110` 首字（同时也是 `stop` 里 `__rust_dealloc` 的 size 实参）的非法值"。
/// `0x8000_0000_0000_0000` 与 `0` 是另外两个"跳过 dealloc"的特例（`0x4070ac`/`0x4070b8`）。
pub const LAN_SLOT_EMPTY: u64 = 0x8000_0000_0000_0001;

/// LAN 广播者 socket fd 的"未打开"值（`i32` 的 −1）。
///
/// `GraphwarLanBroadcasterV6::new` arm64 `0x3de0f0 str w8(=-1),[x19,#0xb8]` 写它；
/// `stop_lan_broadcaster` `0x407090 cmn w0,#0x1` 用它判"要不要 close"。
pub const LAN_FD_CLOSED: i32 = -1;

/// 内联 `GraphwarLanBroadcasterV6`（`+0x110..+0x1d0`，0xC0 字节）里**我方建模**的那部分。
///
/// 只有两件事有指令证据：socket fd 与那个被 `__rust_dealloc` 释放的堆缓冲长度。
/// 其余 0xC0−12 字节没读出来 ⇒ 不建模（AGENTS.md §0.3：不猜）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanBroadcasterSlot {
    /// `GraphwarLanBroadcasterV6::new` 的第 1 个实参（`start_lan_broadcaster` `0x407244 mov x8,sp` /
    /// `0x407248 mov w0,w1` ⇒ 本函数第 2 实参原样传进去，且是 **32 位**）。
    pub port: u32,
    /// `+0x1c8`（对象内 `+0xb8`）：原始 socket fd，`−1` = 未打开（见 [`LAN_FD_CLOSED`]）。
    ///
    /// ⚠️ **已知语义损失**：`GraphwarLanBroadcasterV6::start`（arm64 `0x3de19c`）的**体内本轮没读**
    /// ⇒ "`start` 之后 fd 变成几"未知。因此 [`GraphwarBackend::start_lan_broadcaster`] 装进来的槽
    /// 保持 `−1`，`stop` 时也就不会推 `CloseSocket` 步骤。要补这一格，缺的是 `0x3de19c` 的逐指令读
    /// （`socket()`/`bind()`/`setsockopt()` 的返回值落地位置）。
    pub fd: i32,
    /// `+0x110`（对象首字）：堆缓冲的**长度**（`stop` 里的 `__rust_dealloc` 拿它当 `size`，
    /// 指针取 `+0x118`：`0x4070bc ldr x0,[x19,#0x118]`）。`0` 与 `0x8000_0000_0000_0000` 时**跳过**释放
    /// （`0x4070b0`/`0x4070b8` 两条判据）—— 也就是"空缓冲不收尾"。
    pub buffer_len: u64,
}

/// `start_lan_broadcaster`（arm64 `0x407220` / 144 B）的步骤，顺序 = `bl` 地址顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartLanBroadcasterStep {
    /// `GraphwarLanBroadcasterV6::new`（`0x40724c bl 0x3de064`，sret = `x8 = sp`，实参 `w0 = 端口`）。
    /// 对象建在**栈上**（本函数 `sub sp,sp,#0xe0` 的 0xC0 字节区）。
    NewBroadcaster,
    /// `GraphwarLanBroadcasterV6::start`（`0x407254 bl 0x3de19c`，实参 `&mut 栈上对象`）。
    StartBroadcaster,
    /// 装槽：`0x407258`–`0x407284` 八组 `ldp q / stp q`，把栈上 0xC0 字节搬进 `self+0x110..+0x1d0`。
    Install,
}

/// 内联 `GraphwarServerBrokerHandler`（`+0x228..+0x260`，0x38 字节）里**我方建模**的那部分。
///
/// 布局证据（都是指令）：槽的**首字**是一个带原子强计数的堆指针（`stop` 里
/// `0x407520 ldxr` / `0x407524 subs #1` / `0x407528 stlxr` / `0x40753c bl Arc::drop_slow`），
/// 紧随其后 `+0x230` 是一个 hashbrown 表 ⇒ 对象 = `{ Arc<…>(8) , HashMap(48) }`。
/// `Option<…>` 的空靠**首字为 0**（`0x407510 cbz x20` / `0x4077d0 cbz x8` / 收尾 `0x407548 str xzr`），
/// 与 LAN 槽那个 `0x8000_0000_0000_0001` 哨兵**不是同一套编码**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokerSlot {
    /// `GraphwarServerBrokerHandler::start` 的第 2 个实参（`0x4077f4 mov x1,x19` ⇒
    /// `start_server_broker_handler` 的实参**原样**透传；宽度本轮未证 —— 用 `u32` 承载只为可测）。
    pub arg: u32,
    /// `+0x228`（对象首字）那个 `Arc` 的**强引用计数**。
    ///
    /// ⚠️ **推断**：装槽时给 1 是 `Arc::new` 的 std 契约；`GraphwarServerBrokerHandler::new`（0x489d64）
    /// 的体内本轮**没读**，所以"新建对象时是不是恰好 1"没有指令证据。
    /// `stop` 侧用得到它：`0x407524 subs #1` 之后 `0x407530 b.ne` ⇒ 归零才 `Arc::drop_slow`。
    pub arc_strong: usize,
    /// `+0x230`（对象内 `+0x8`）那个 `HashMap` 里几条 —— 只在文档/测试里用；
    /// `stop` 里丢表是**无条件**的（`0x407540` 没有守卫）。
    pub map_entries: usize,
}

/// `start_server_broker_handler`（arm64 `0x4077bc` / 140 B）的步骤，顺序 = `bl` 地址顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartBrokerStep {
    /// `GraphwarServerBrokerHandler::new`（`0x4077f0 bl 0x489d64`，sret = `x8 = sp+0x8`）。
    /// ⚠️ 该调用点 `x0` 仍是 `self`、`x1` 是 `start` 的第 2 实参 ⇒ `new` 的头两个实参
    /// **本轮没读明白**（`0x489d64` 的体没读）；只确定 sret 是 `sp+0x8`、对象 0x38 字节。
    NewHandler,
    /// `GraphwarServerBrokerHandler::start`（`0x4077fc bl 0x489fa8`，实参 `&mut 栈上对象` + 原实参）。
    StartHandler,
    /// 装槽：`0x407814 stp q0,q1,[x8]` / `0x407818 str q2,[x8,#0x20]` / `0x40781c str x9,[x8,#0x30]`
    /// （`x8 = self+0x228`，共 0x38 字节）。
    Install,
}

/// `stop_server_broker_handler`（arm64 `0x4074f8` / 136 B）的步骤，顺序 = 指令地址顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopBrokerStep {
    /// `GraphwarServerBrokerHandler::stop`（`0x40751c bl 0x489e50`，实参 `&mut self+0x228`）。
    HandlerStop,
    /// 槽首字那个 `Arc` 的**强计数 −1**（`0x407520 ldxr` / `0x407524 subs x8,x8,#1` / `0x407528 stlxr`，
    /// store 失败 `cbnz` 自旋重试）。
    ArcRelease,
    /// 计数减到 0 ⇒ `Arc::drop_slow`（`0x407534 dmb ishld` / `0x40753c bl 0x45f2e4`）。
    /// **只在"减完 == 0"时才走**（`0x407530 b.ne 0x407540` 跳过）。
    ArcDropSlow,
    /// 丢 handler 里的 `HashMap`（`0x407540 add x0,x19,#0x230` + `0x407544 bl RawTable::drop`）—— **无条件**。
    MapDrop,
}

/// `stop_lan_broadcaster`（arm64 `0x407068` / 172 B）的步骤，顺序 = 指令地址顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopLanBroadcasterStep {
    /// `close(fd)`（`0x407098 bl close@plt`）—— **只有 `fd != -1` 才调**
    /// （`0x407090 cmn w0,#0x1` / `0x407094 b.eq` 跳过）。
    CloseSocket,
    /// `+0x1c8` 写回 `-1`（`0x40709c mov w8,#-1` + `0x4070a0 str`）—— **无条件**（close 两条路汇合到这里）。
    ResetFd,
    /// 丢 LAN 广播者里的 hashbrown 表（`0x4070a4 add x0,x19,#0x188` + `0x4070a8 bl RawTable::drop`）—— 无条件。
    DropTable,
    /// 释放那块的堆缓冲（`0x4070bc ldr x0,[x19,#0x118]` + `w2=1` + `0x4070c8 bl __rust_dealloc`）。
    /// **两条跳过判据**：长度 `== 0x8000_0000_0000_0000`（`0x4070b0`/`0x4070b4`）或 `== 0`（`0x4070b8 cbz`）。
    DeallocBuffer,
}

/// `setup_level`（arm64 `0x406b4c` / 88 B）的步骤。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupLevelStep {
    /// `GraphwarGameClient::wait_for_players`（arm64 `0x406b80 bl 0x4094ec`）。
    WaitForPlayers,
    /// `GraphwarGameServer::setup_level`（arm64 `0x406b9c` 的**尾调用** `b 0x48297c`）。
    ServerSetupLevel,
}

/// `stop_client`（arm64 `0x406ba4` / 104 B）的步骤。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopClientStep {
    /// `GraphwarGameClient::disconnect`（`0x406bc0 bl 0x4087fc`）。
    DisconnectClient,
    /// `drop_in_place<GraphwarGameClient>`（`0x406bc8 bl 0x417758`）。
    DropClient,
    /// `drop_in_place<GraphwarReplayPlayer>`（`0x406be0 bl 0x417b84`，实参 `self+0x00`）。
    DropPlayer,
}

/// `stop_server`（arm64 `0x406c0c` / 88 B）的步骤。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopServerStep {
    /// `GraphwarGameServer::stop`（`0x406c2c bl 0x482e74`，实参 `self+0x60`）。
    StopServer,
    /// `drop_in_place<GraphwarGameServer>`（`0x406c40 bl 0x4178dc`）。
    DropServer,
}

/// `start_server`（arm64 `0x406c64` / 216 B）的步骤。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartServerStep {
    /// `GraphwarGameServer::new(w1, w2, 0)`（`0x406c8c bl 0x482cc4`），对象建在**栈上**。
    NewServer,
    /// `GraphwarGameServer::start`（`0x406c94 bl 0x483090`），也在栈上、**先**于搬移。
    StartServer,
    /// 旧槽非空 ⇒ `drop_in_place<GraphwarGameServer>`（`0x406cc8 bl 0x4178dc`）。
    DropOldServer,
}

impl GraphwarBackend {
    /// 空后端。
    pub fn new() -> Self {
        Self::default()
    }

    /// `subscribe_to_event` —— **原版 arm64 `0x406e80` / 488 字节**（x86 **`0x514b40`** / FDE `0x1d7`）。
    ///
    /// 逐指令（arm64 `0x406e80..0x407000` 主路径；`0x407000` 之后是 panic/cleanup 垫片）：
    ///
    /// ```text
    /// 0x406e84  adrp x8,0x8b4000 / 0x406eb8 add x8,x8,#0x378   ; x8 = 0x8b4378：**全局原子计数器**
    /// 0x406ebc  ldxr w22,[x8] / 0x406ec0 add w9,w22,#1 / 0x406ec4 stxr
    ///                                                          ; **返回旧值**（fetch_add）⇒ 本次订阅 id
    /// 0x406ecc  ldr x19,[x23,#0x1d8]                            ; 订阅登记表 = Rc<RefCell<HashMap<…>>>
    /// 0x406ed0  ldr x8,[x19,#0x10] / 0x406ed4 cbnz 0x406ff4     ; RefCell 借用标记 ≠ 0 ⇒ 走
    ///                                                            ;   panic_already_borrowed（0x406ffc，Location 0x8526f8）
    /// 0x406edc  str x8(=-1),[x19,#0x10]                          ; borrow_mut
    /// 0x406ee0  tbz x20,#0x3f  / 0x406f0c bl __rust_alloc        ; Box<[u8]>：alloc(len, align 1)
    /// 0x406f20  bl memcpy                                        ;   把 &str 拷进去
    /// 0x406f2c  stp x20,x26,[sp,#0x28] / a903e7f4 / f90027f8     ; 表项 = (name len, name ptr, x25, x24)
    /// 0x406f48  bl HashMap::insert(table+0x18, id, &entry)       ; 键 = 上面那个 id
    /// 0x406f4c..0x406f90  drop 旧值（含一次经 vtable 的 `blr x8` + `__rust_dealloc`）
    /// 0x406f94..0x406fac  恢复 borrow
    /// 0x406fb0  ldr x8,[x23,#0x1e0] / 0x406fcc csel x1,xzr,x9,eq  ; x1 = Option<&mut client>（槽空则 null）
    /// 0x406fc4  mov w4,w22                                       ; 第 5 个实参 = id
    /// 0x406fd0  bl 0x407580                                      ; **subscribe_to_event_with_id**(self, client, name, id)
    /// 0x406fd0 之后直接 epilogue ⇒ **返回值就是被调者的返回值**
    /// ```
    ///
    /// ⇒ 语义：**给这条事件分配 id（全局计数器自增）→ 登记进 `+0x1d8` 的表 → 转给
    /// [`Self::subscribe_to_event_with_id`] 去真正订到 handler 上**。
    /// 三个不显然的点（本轮新证据）：
    /// 1. **id 不是线上 tag**：它由 `0x8b4378` 的全局原子计数器**分配**（`fetch_add`），
    ///    与 `Event::wire_tag()` 无关 —— T95 把 `+0x1d8` 的键当"事件 id"是对的，但它是**分配来的**；
    /// 2. **表项是 4 个字**（`x2/x3/x4` 三个实参 + name）：`0x406f2c`–`0x406f38` 连续存
    ///    `(name_len, name_ptr, x25=arg3, x24=arg4)` ⇒ 值与"只有名字"不同，本轮**没有**读出
    ///    arg3/arg4 的类型（落在 vtable 里的可能性最大：`0x406f90` 那次 `blr x8` 是 drop 旧值）；
    /// 3. 借用冲突走的是 `panic_already_borrowed`（`0x406ffc` + Location `0x8526f8`）⇒ 整段在
    ///    一个 `borrow_mut` 里，**不是**逐条 `RefCell` 操作。
    ///
    /// ⚠️ 档位：**观测**（逐指令）；`+0x1d8` 的值类型本轮只建模成 `String`（沿用 T95 的简化），
    /// **不是**结论。计数器的**初值未知**（是否在别处初始化没查）⇒ 单测只断言"每次 +1"的相对关系，
    /// 不断言绝对 id。
    pub fn subscribe_to_event(&mut self, name: &str) -> bool {
        // 0x406ebc..0x406ec4：全局原子计数器，返回旧值 = 本次订阅 id
        let id = NEXT_SUBSCRIPTION_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        // 0x406f48：登记进 +0x1d8 的表（键 = id）
        self.subscriptions.insert(id, name.to_string());
        // 0x406fd0：转给 subscribe_to_event_with_id（返回值原样透出）
        self.subscribe_to_event_with_id(name, id)
    }

    /// `setup_level` —— **原版 arm64 `0x406b4c` / 88 字节**（x86 侧**无 Location 锚**，527 外）。
    ///
    /// 逐指令（arm64 `llvm-objdump`，`0x406b4c..0x406ba4`）：
    ///
    /// ```text
    /// 0x406b4c  ldr  x8,[x0,#0x60]!      ; 前索引：x0 += 0x60，x8 = [self+0x60]（server 槽判别字）
    /// 0x406b50  cmp  x8,#0x4
    /// 0x406b54  b.eq 0x406ba0            ; 没有 server ⇒ 直接 ret（连栈帧都不建）
    /// 0x406b58  ldr  x8,[x0,#0x180]      ; x0 已是 self+0x60 ⇒ 读 [self+0x1e0] = 内联客户端槽
    /// 0x406b5c  cbz  x8,0x406ba0         ; 没有客户端 ⇒ ret
    /// 0x406b70  mov  x19,x0              ; x19 = self+0x60 = &server
    /// 0x406b74  add  x0,x0,#0x180        ; x0 = self+0x1e0 = &client
    /// 0x406b78  mov  x20,x2 / 0x406b7c mov x21,x1   ; 实参 a,b 存到 callee-saved
    /// 0x406b80  bl   0x4094ec            ; GraphwarGameClient::wait_for_players(&client, a, b)
    /// 0x406b84  mov  x0,x19 / x1=x21 / x2=x20       ; 换成 &server 与同一对实参
    /// 0x406b9c  b    0x48297c            ; **尾调用** GraphwarGameServer::setup_level(&server, a, b)
    /// ```
    ///
    /// ⇒ 语义：**两个前置守卫（server 非空、client 非空）→ 让客户端等玩家 → 转给服务端**。
    /// 守卫顺序也是证据（先 server 后 client：`0x406b50` 在 `0x406b5c` 之前）。
    ///
    /// ⚠️ 档位：**观测**（逐指令读出，两侧名字由 arm64 `.symtab` 给出）。
    /// 不声称"逐位"：x86 侧没有锚，无法逐字节对拍同一函数；`wait_for_players` 的**语义**
    /// （阻塞轮询，见 `graphwar_game_client` 的 `0x516fd0`）不在本函数体内。
    pub fn setup_level(&mut self, arg0: u32, arg1: u32) -> Vec<SetupLevelStep> {
        let mut steps = Vec::new();
        // 0x406b50：cmp x8,#4 —— server 槽空就直接返回（此时 `arg0/arg1` 一个都不用）
        if self.server.is_none() {
            return steps;
        }
        // 0x406b58：ldr x8,[self+0x1e0] / cbz —— client 槽空也直接返回
        if self.client.is_none() {
            return steps;
        }
        let _ = (arg0, arg1); // 两个实参原样透传（0x406b78/0x406b7c → 0x406b88/0x406b8c）
        steps.push(SetupLevelStep::WaitForPlayers); // 0x406b80
        steps.push(SetupLevelStep::ServerSetupLevel); // 0x406b9c（尾调用 ⇒ 本函数不再有指令）
        steps
    }

    /// `stop_client` —— **原版 arm64 `0x406ba4` / 104 字节**（x86 侧**无 Location 锚**，527 外）。
    ///
    /// 逐指令（arm64 `0x406ba4..0x406c0c`）：
    ///
    /// ```text
    /// 0x406bb0  ldr  x8,[x0,#0x1e0]      ; 内联客户端槽（niche：0 = None）
    /// 0x406bb8  cbz  x8,0x406bcc         ; 客户端为空 ⇒ 跳过 disconnect+drop
    /// 0x406bc0  bl   0x4087fc            ; GraphwarGameClient::disconnect(&self.client)
    /// 0x406bc8  bl   0x417758            ; drop_in_place<GraphwarGameClient>(&self.client)
    /// 0x406bcc  ldr  x8,[x19]            ; 回放播放器槽判别字（self+0x00）
    /// 0x406bd0  str  xzr,[x19,#0x1e0]    ; self.client = None
    /// 0x406bd4  cmp  x8,#0x2
    /// 0x406bd8  b.eq 0x406be4            ; 槽已经是空的（2）⇒ 不要再 drop
    /// 0x406be0  bl   0x417b84            ; drop_in_place<GraphwarReplayPlayer>(self+0x00)
    /// 0x406be8  str  x8(=2),[x19]        ; 判别字写 2 = 空
    /// ```
    ///
    /// ⇒ 语义：**先拆客户端（disconnect + drop），再拆回放播放器；两个槽最后都变空**。
    /// 注意判别字**先读**（`0x406bcc`）**后写**（`0x406bd0`）—— 读的顺序对"drop 前先判"没有影响，
    /// 但说明这两件事在源码里是相邻的两段，不是 `Option::take()` 那种一条表达式（`take` 只会读一次）。
    ///
    /// ⚠️ 档位：**观测**（逐指令）。x86 侧无锚 ⇒ 不能逐位对拍。
    pub fn stop_client(&mut self) -> Vec<StopClientStep> {
        let mut steps = Vec::new();
        // 0x406bb8：客户端为空就跳过整段
        if self.client.is_some() {
            steps.push(StopClientStep::DisconnectClient); // 0x406bc0
            steps.push(StopClientStep::DropClient); // 0x406bc8
            self.client = None; // 0x406bd0
        }
        // 0x406bd4：播放器槽判别字 == 2（空）就不 drop
        if self.replay_slot_tag != PLAYER_SLOT_EMPTY {
            steps.push(StopClientStep::DropPlayer); // 0x406be0
            self.replay = None;
        }
        self.replay_slot_tag = PLAYER_SLOT_EMPTY; // 0x406be8
        steps
    }

    /// `stop_server` —— **原版 arm64 `0x406c0c` / 88 字节**（x86 侧**无 Location 锚**，527 外）。
    ///
    /// 逐指令（arm64 `0x406c0c..0x406c58`）：
    ///
    /// ```text
    /// 0x406c1c  ldr  x8,[x19,#0x60]!     ; x19 += 0x60；x8 = server 槽判别字
    /// 0x406c20  cmp  x8,#0x4
    /// 0x406c24  b.eq 0x406c44            ; 已经是空的（4）⇒ 只写一次 4 就 ret
    /// 0x406c2c  bl   0x482e74            ; GraphwarGameServer::stop(&self.server)
    /// 0x406c34  ldr  x8,[x19]            ; **stop 之后重读**判别字（stop 可能自己把它写成 4）
    /// 0x406c38  cmp  x8,#0x4
    /// 0x406c3c  b.eq 0x406c44            ; 变成 4 了 ⇒ 不再 drop_in_place（避免二次析构）
    /// 0x406c40  bl   0x4178dc            ; drop_in_place<GraphwarGameServer>(self+0x60)
    /// 0x406c48  str  x8(=4),[x19]        ; 判别字写 4 = 空
    /// ```
    ///
    /// ⇒ 语义：**有 server 才停；stop 之后按"新判别字"决定要不要 drop**（这条二次判据是本函数的
    /// 不显然之处，`stop` 自己可能把槽清空 ⇒ 不允许重复 drop）。
    ///
    /// ⚠️ 档位：**观测**（逐指令）。"`stop` 会不会自己清空"本轮**没有**读 `0x482e74` 的体
    /// ⇒ 双判据的**必要性**是观测，**怎么发生**的未知。
    pub fn stop_server(&mut self) -> Vec<StopServerStep> {
        let mut steps = Vec::new();
        // 0x406c20：空槽直接返回（`0x406c48` 的写入是幂等的，这里不重复推步骤）
        if self.server.is_none() {
            self.server = None; // 幂等：相当于 0x406c48 写 4
            return steps;
        }
        steps.push(StopServerStep::StopServer); // 0x406c2c
        // 0x406c34：stop 之后**重读**判别字
        if self.server.is_some() {
            steps.push(StopServerStep::DropServer); // 0x406c40
        }
        self.server = None; // 0x406c48
        steps
    }

    /// `start_server` —— **原版 arm64 `0x406c64` / 216 字节**（x86 侧**无 Location 锚**，527 外）。
    ///
    /// 逐指令（arm64 `0x406c64..0x406d00`，主路径）：
    ///
    /// ```text
    /// 0x406c64..0x406c78  sub sp,#0x130 / 存 x29,x30,x28,x20,x19   ; 0x100 字节栈区留给 server 对象
    /// 0x406c80  mov  w0,w1 / 0x406c84 mov w1,w2 / 0x406c88 mov w2,wzr
    /// 0x406c8c  bl   0x482cc4            ; GraphwarGameServer::new(a, b, 0) → 写到 [sp]
    /// 0x406c90  mov  x0,sp / 0x406c94 bl 0x483090   ; GraphwarGameServer::start(&mut on-stack)
    /// 0x406c9c  ldr  x8,[x19,#0x60]!     ; x19 += 0x60；读旧槽判别字（**在 new+start 之后**）
    /// 0x406ca8  cmp  x8,#0x4
    /// 0x406ca8..0x406cc0 四组 ldp/stp 把栈上 0x80 字节重排到 [sp+0x80..0x100]
    /// 0x406cc0  b.eq 0x406ccc            ; 旧槽空 ⇒ 不 drop
    /// 0x406cc8  bl   0x4178dc            ; drop_in_place<GraphwarGameServer>(self+0x60)
    /// 0x406ccc..0x406ce8 四组 ldp/stp 把 0x80 字节写进 self+0x60..0xe0（含判别字）
    /// ```
    ///
    /// ⇒ 语义：**先在栈上 `new` + `start`，再（必要时）拆旧槽，最后整块 0x80 字节装进去**。
    /// 顺序是证据：`0x406c94`（start）**先于** `0x406c9c`（读旧判别字）与 `0x406cc8`（drop 旧槽）
    /// ⇒ 新 server 建好之后才动旧的（`start` 失败/panic 时旧槽仍完好）。
    ///
    /// ⚠️ 档位：**观测**（逐指令）。对象 0x80 字节我方只建模 `new` 的两个实参 + `started` 标志；
    /// 其余字节没读出来。
    pub fn start_server(&mut self, arg0: u32, arg1: u32) -> Vec<StartServerStep> {
        let mut steps = Vec::new();
        steps.push(StartServerStep::NewServer); // 0x406c8c
        steps.push(StartServerStep::StartServer); // 0x406c94
        if self.server.is_some() {
            // 0x406ca8：cmp x8,#4 —— 旧槽非空才 drop（顺序在 new+start 之后）
            steps.push(StartServerStep::DropOldServer); // 0x406cc8
        }
        self.server = Some(ServerSlot {
            arg0,
            arg1,
            started: true,
        }); // 0x406ccc..0x406ce8
        steps
    }

    /// `get_next_clip_checkpoint` —— **原版 arm64 `0x40745c` / 156 字节**（x86 侧**无 Location 锚**，527 分母外）。
    ///
    /// 逐指令（arm64 `0x40745c..0x4074f8`；T106 逐条读，`llvm-objdump -d --start-address=0x40745c`）：
    ///
    /// ```text
    /// 0x40745c  ldr  x8,[x0]              ; x0 = self（播放器槽判别字在 +0x00）
    /// 0x407460  cmp  x8,#0x2
    /// 0x407464  b.eq 0x4074d4             ; 槽空（2）⇒ 直接 None（0x4074d4 mov w0,wzr / ret）
    /// 0x407468  ldp  x8,x9,[x0,#0x48]     ; x8 = clip 检查点表指针(+0x48)、x9 = 表长(+0x50)
    /// 0x40746c  cbz  x9,0x407480
    /// 0x407480  add x10,x9,x9 / cmp x10,x9 / b.hs 0x4074d4   ; len==0 的残路 ⇒ None
    /// 0x407470  cmp  x9,#0x1 / b.ne 0x407490                  ; len>1 走二分
    /// 0x407478  mov  x10,xzr / b 0x4074b8                     ; len==1：lo=0 直接判
    ///   ── 二分（0x407498..0x4074b4）──
    /// 0x407498  lsr  x12,x11,#1           ; half = hi/2
    /// 0x40749c  add  x13,x12,x10          ; mid  = lo + half
    /// 0x4074a0  sub  x11,x11,x12          ; hi  -= half
    /// 0x4074a4  ldr  w14,[x8,x13,lsl #2]  ; 表[mid]
    /// 0x4074a8  cmp  w14,w1               ; 与实参比
    /// 0x4074ac  csel x10,x10,x13,hi       ; **表[mid] > 实参 ⇒ lo 不动**，否则 lo = mid
    /// 0x4074b0  cmp  x11,#0x1 / b.hi 0x407498
    ///   ── 收敛后的等值/邻位判定 ──
    /// 0x4074b8  ldr  w11,[x8,x10,lsl #2]  ; 表[lo]
    /// 0x4074bc  cmp  w11,w1
    /// 0x4074c0  b.ne 0x4074dc             ; 不等 ⇒ 走邻位分支
    /// 0x4074c4  mov  w11,#0x1 / add x10,x11,x10   ; 相等 ⇒ idx = lo+1（**严格大于**才返回）
    /// 0x4074cc  cmp  x10,x9 / b.lo 0x4074ec       ; idx < len ⇒ 返回表[idx]
    /// 0x4074dc  cinc x10,x10,lo           ; 表[lo] < 实参 ⇒ lo+1，否则 lo（取"第一个大于"的下标）
    /// 0x4074e4  cmp  x10,x9 / b.hs 0x4074d4       ; 越界 ⇒ None
    /// 0x4074ec  ldr  w1,[x8,x10,lsl #2]   ; 返回值 = 表[idx]（w1）
    /// 0x4074f0  mov  w0,#0x1 / ret        ; 判定字 w0 = 1 = Some
    /// 0x4074d4  mov  w0,wzr / ret         ; 判定字 w0 = 0 = None
    /// ```
    ///
    /// ⇒ 语义：**在 clip 检查点表里找第一个严格大于 `current` 的元素**（= `upper_bound`），
    /// 没有就返回 `None`。返回形态是 `Option<u32>`：判定字在 `w0`、值在 `w1`（`0x4074f0` 那对
    /// `mov w0,#1` + 前一条 `ldr w1`）；空判定是 `w0 = 0`。
    ///
    /// 三个不显然的点：
    /// 1. **槽空就直接 None**（`0x407464`），不碰表 —— 与 `stop_client`/`process` 用的是**同一个**
    ///    `+0x00 == 2` 判据（[`PLAYER_SLOT_EMPTY`]）；
    /// 2. **等值也跳过**（`0x4074c4` 相等 ⇒ `idx = lo+1`）⇒ 同一个 tick 连问两次拿到的是**下一个**检查点，
    ///    这正是"播放下一个 clip"要的语义；
    /// 3. 表是**升序**假设（二分），原版没有做有序性校验。
    ///
    /// ⚠️ 档位：**观测**（逐指令；函数名来自 arm64 `.symtab` 的
    /// `graphwarrust::graphwar_backend::GraphwarBackend::get_next_clip_checkpoint::h9639fc97afa47d4`）。
    /// 不能标"逐位"：x86 侧没有锚，无法同函数对拍；两颗星的语义等价性只经过"指令顺序 → 我方同构循环"的对读。
    pub fn get_next_clip_checkpoint(&self, current: u32) -> Option<u32> {
        // 0x407460 cmp x8,#0x2 / 0x407464 b.eq —— 回放槽空 ⇒ None
        if self.replay_slot_tag == PLAYER_SLOT_EMPTY {
            return None;
        }
        // 0x407468 ldp x8,x9,[x0,#0x48]：内联播放器的 (ptr, len)
        let cps = &self.clip_checkpoints;
        let len = cps.len();
        if len == 0 {
            // 0x40746c cbz x9 → 0x407480 → b.hs 0x4074d4
            return None;
        }
        // 0x407498..0x4074b4：二分，收敛到"最后一个 <= current"的位置（`hi` 折半、`lo` 只在
        // `表[mid] <= current` 时前移 —— 与 0x4074ac 的 `csel …, hi` 同构）
        let (mut lo, mut hi) = (0usize, len);
        while hi > 1 {
            let half = hi / 2; // 0x407498
            let mid = lo + half; // 0x40749c
            hi -= half; // 0x4074a0
            if cps[mid] > current {
                // 0x4074a8 cmp / 0x4074ac csel hi ⇒ lo 不动
            } else {
                lo = mid;
            }
        }
        // 0x4074b8..0x4074e0：等值也 +1（严格大于），否则按大小取 lo 或 lo+1
        let idx = if cps[lo] == current {
            lo + 1 // 0x4074c4 mov w11,#1 / 0x4074c8 add x10,x11,x10
        } else if cps[lo] < current {
            lo + 1 // 0x4074dc cinc x10,x10,lo
        } else {
            lo // 0x4074e0 add x10,xzr,x10
        };
        // 0x4074cc / 0x4074e4 越界 ⇒ None（w0=0）；否则 0x4074ec ldr w1 + 0x4074f0 mov w0,#1
        cps.get(idx).copied()
    }

    /// `start_lan_broadcaster` —— **原版 arm64 `0x407220` / 144 字节**（x86 侧无 Location 锚，527 分母外）。
    ///
    /// 逐指令（arm64 `0x407220..0x4072b0`）：
    ///
    /// ```text
    /// 0x407220  sub  sp,sp,#0xe0 / 存 x29,x30,x19     ; 0xC0 字节区给广播者对象，0x20 给帧
    /// 0x407230  ldr  x8,[x0,#0x110]                   ; 槽首字（判别位，见 LAN_SLOT_EMPTY）
    /// 0x407234  mov  x9,#-0x7fffffffffffffff
    /// 0x407238  cmp  x8,x9
    /// 0x40723c  b.ne 0x407288                        ; **已经装着 ⇒ 一步都不做**（直接 epilogue）
    /// 0x407244  mov  x8,sp                           ; sret = 栈上对象
    /// 0x407248  mov  w0,w1                           ; 实参 = 本函数第 2 实参（32 位口径）
    /// 0x40724c  bl   0x3de064                        ; GraphwarLanBroadcasterV6::new
    /// 0x407250  mov  x0,sp
    /// 0x407254  bl   0x3de19c                        ; GraphwarLanBroadcasterV6::start(&mut 栈上)
    /// 0x407258..0x407284  8 组 ldp q/stp q            ; 把 0xC0 字节从 sp 搬到 self+0x110..+0x1d0
    ///                                                       （顺序：+0x190/+0x1b0 → +0x150/+0x170 → +0x110/+0x130）
    /// 0x407288..  epilogue / ret
    /// ```
    ///
    /// ⇒ 语义：**只在槽为空时装**（与 `start_server` 的"无条件重建"相反，见 [`Self::start_server`]）；
    /// 装配顺序 = `new(端口)` → `start()` → **整块 0xC0 字节搬进槽**（`new`+`start` 都在栈上完成，
    /// 失败/panic 时槽仍是原样）。
    ///
    /// ⚠️ 档位：**观测**（逐指令）。`start` 的体内没读 ⇒ 开 socket 之后 `+0x1c8` 的 fd 值未知
    /// （见 [`LanBroadcasterSlot::fd`] 的已知语义损失）。
    pub fn start_lan_broadcaster(&mut self, port: u32) -> Vec<StartLanBroadcasterStep> {
        let mut steps = Vec::new();
        // 0x407238/0x40723c：槽首字 != 空哨兵 ⇒ 直接返回（不 new、不 start）
        if self.lan_broadcaster.is_some() {
            return steps;
        }
        // 0x40724c：GraphwarLanBroadcasterV6::new(w0 = 端口)，对象在栈上
        steps.push(StartLanBroadcasterStep::NewBroadcaster);
        // 0x407254：GraphwarLanBroadcasterV6::start(&mut 栈上对象)
        steps.push(StartLanBroadcasterStep::StartBroadcaster);
        // 0x407258..0x407284：整块 0xC0 字节写入 self+0x110
        self.lan_broadcaster = Some(LanBroadcasterSlot {
            port,
            fd: LAN_FD_CLOSED,
            buffer_len: 0,
        });
        steps.push(StartLanBroadcasterStep::Install);
        steps
    }

    /// `stop_lan_broadcaster` —— **原版 arm64 `0x407068` / 172 字节**（x86 侧无 Location 锚，527 分母外）。
    ///
    /// 逐指令（arm64 `0x407068..0x407114`，主路径；`0x4070e0` 之后是 panic 垫片）：
    ///
    /// ```text
    /// 0x407078  ldr  x20,[x0,#0x110]           ; 槽首字（空哨兵见 LAN_SLOT_EMPTY）
    /// 0x407084  cmp  x20,x21                   ; x21 = 0x8000_0000_0000_0001
    /// 0x407088  b.eq 0x4070cc                  ; **已经是空 ⇒ 跳过全部收尾**（只写回哨兵）
    /// 0x40708c  ldr  w0,[x19,#0x1c8]           ; socket fd
    /// 0x407090  cmn  w0,#0x1 / 0x407094 b.eq   ; fd == -1 ⇒ 不 close
    /// 0x407098  bl   close@plt
    /// 0x40709c  mov  w8,#-1 / 0x4070a0 str w8,[x19,#0x1c8]   ; fd = -1（无条件）
    /// 0x4070a4  add  x0,x19,#0x188
    /// 0x4070a8  bl   RawTable::drop            ; 丢掉 hashbrown 表（无条件）
    /// 0x4070ac  mov  x8,#-0x8000000000000000
    /// 0x4070b0  cmp  x20,x8 / 0x4070b4 b.eq     ; 长度 == i64::MIN ⇒ 不 dealloc
    /// 0x4070b8  cbz  x20,0x4070cc              ; 长度 == 0 ⇒ 不 dealloc
    /// 0x4070bc  ldr  x0,[x19,#0x118]           ; 指针（注意：**指针在 +0x118，长度在 +0x110**）
    /// 0x4070c0  mov  x1,x20 / 0x4070c4 mov w2,#1
    /// 0x4070c8  bl   __rustc::__rust_dealloc   ; dealloc(ptr, size=长度, align=1)
    /// 0x4070cc  str  x21,[x19,#0x110]          ; 槽 = 空哨兵（空槽时也走这一句）
    /// ```
    ///
    /// ⇒ 语义：**空槽什么都不做；非空则 close(fd) → fd=-1 → 丢表 → （长度合法才）释放缓冲 → 写空哨兵**。
    /// 三个不显然的点：
    /// 1. `fd` 与"长度"是**同一个对象里**的两个字段（`+0x1c8` / `+0x110`），所以"关 socket"和
    ///    "释放缓冲"是**两件独立的事**（一个成功另一个也可能跳过）；
    /// 2. 释放用的 `size` 是槽的**首字**（既是空哨兵位、又是 dealloc 的 size 实参）
    ///    ⇒ 这也是为什么"空/未分配"必须用哨兵和 0 两个特例把它排掉（`0x4070b0`/`0x4070b8`）；
    /// 3. 收尾的写哨兵（`0x4070cc`）在**两条路汇合之后** ⇒ 空槽调用也是幂等的（写回同一个值）。
    ///
    /// ⚠️ 档位：**观测**（逐指令）。不声称"逐位"：x86 侧没有锚。
    pub fn stop_lan_broadcaster(&mut self) -> Vec<StopLanBroadcasterStep> {
        let mut steps = Vec::new();
        // 0x407084/0x407088：空槽 ⇒ 一步都不推（但 0x4070cc 的写哨兵是汇合后的，照做）
        if let Some(slot) = self.lan_broadcaster {
            // 0x407090 cmn w0,#0x1 / b.eq：fd == -1 就不 close
            if slot.fd != LAN_FD_CLOSED {
                steps.push(StopLanBroadcasterStep::CloseSocket); // 0x407098
            }
            steps.push(StopLanBroadcasterStep::ResetFd); // 0x40709c/0x4070a0（无条件）
            steps.push(StopLanBroadcasterStep::DropTable); // 0x4070a4/0x4070a8（无条件）
            // 0x4070b0/0x4070b4 + 0x4070b8：两个"跳过 dealloc"的特例
            if slot.buffer_len != 0 && slot.buffer_len != 0x8000_0000_0000_0000 {
                steps.push(StopLanBroadcasterStep::DeallocBuffer); // 0x4070bc..0x4070c8
            }
        }
        self.lan_broadcaster = None; // 0x4070cc str x21(哨兵),[x19,#0x110]
        steps
    }

    /// `start_server_broker_handler` —— **原版 arm64 `0x4077bc` / 140 字节**（x86 侧无 Location 锚，527 分母外）。
    ///
    /// 逐指令（arm64 `0x4077bc..0x407848`）：
    ///
    /// ```text
    /// 0x4077bc  sub  sp,sp,#0x60 / 存 x29,x30,x20,x19   ; 0x38 字节区给 handler 对象
    /// 0x4077cc  ldr  x8,[x0,#0x228]                     ; 槽首字（= Arc 指针，0 = 空）
    /// 0x4077d0  cbz  x8,0x4077e4                        ; **空才装**；非空 ⇒ 0x4077d4 直接 epilogue
    /// 0x4077e4  add  x8,sp,#0x8                         ; sret = 栈上对象
    /// 0x4077e8  mov  x20,x0                             ; self
    /// 0x4077ec  mov  x19,x1                             ; 唯一的实参
    /// 0x4077f0  bl   0x489d64                           ; GraphwarServerBrokerHandler::new
    /// 0x4077f4  add  x0,sp,#0x8 / 0x4077f8 mov x1,x19
    /// 0x4077fc  bl   0x489fa8                           ; GraphwarServerBrokerHandler::start(&mut, 实参)
    /// 0x407800..0x40781c  ldur q0/q1/q2 + ldr x9（sp+0x8..sp+0x40，共 0x38 字节）
    ///                     → stp q0,q1,[x8] + str q2,[x8,#0x20] + str x9,[x8,#0x30]（x8 = self+0x228）
    /// ```
    ///
    /// ⇒ 语义：**只在槽为空时装**（与 `start_lan_broadcaster` 同形态、与 `start_server` 相反）：
    /// `new()` → `start(实参)` → 整块 0x38 字节搬进 `+0x228`。`new`/`start` 都在栈上完成
    /// ⇒ 中途 panic 不会留下半个槽。
    ///
    /// ⚠️ 档位：**观测**（逐指令）。`new`/`start` 的体内没读 ⇒ "装进来的对象里有什么"
    /// 只有槽首字的 Arc 指针 + 后随 hashbrown 表两条（见 [`BrokerSlot`]）。
    pub fn start_server_broker_handler(&mut self, arg: u32) -> Vec<StartBrokerStep> {
        let mut steps = Vec::new();
        // 0x4077d0 cbz x8：槽非空 ⇒ 一步都不做
        if self.broker.is_some() {
            return steps;
        }
        // 0x4077f0：GraphwarServerBrokerHandler::new（栈上，sret = sp+0x8）
        steps.push(StartBrokerStep::NewHandler);
        // 0x4077fc：GraphwarServerBrokerHandler::start(&mut 栈上对象, 实参)
        steps.push(StartBrokerStep::StartHandler);
        // 0x407814..0x40781c：0x38 字节整块写进 self+0x228
        self.broker = Some(BrokerSlot {
            arg,
            arc_strong: 1, // Arc::new 的契约初值（**推断**，见 BrokerSlot::arc_strong）
            map_entries: 0,
        });
        steps.push(StartBrokerStep::Install);
        steps
    }

    /// `stop_server_broker_handler` —— **原版 arm64 `0x4074f8` / 136 字节**（x86 侧无 Location 锚，527 分母外）。
    ///
    /// 逐指令（arm64 `0x4074f8..0x407580`，主路径）：
    ///
    /// ```text
    /// 0x4074f8  stp x29,x30,[sp,#-0x30]! / 存 x21,x20,x19
    /// 0x407508  ldr  x20,[x0,#0x228]      ; 槽首字 = handler 里那个 Arc 的指针
    /// 0x40750c  add  x21,x0,#0x228
    /// 0x407510  cbz  x20,0x407548        ; **空槽 ⇒ 跳过全部收尾**（只去写 0）
    /// 0x407518  add  x0,x0,#0x228
    /// 0x40751c  bl   0x489e50             ; GraphwarServerBrokerHandler::stop(&mut self+0x228)
    /// 0x407520  ldxr x8,[x20] / 0x407524 subs x8,x8,#1 / 0x407528 stlxr w9,x8,[x20]
    /// 0x40752c  cbnz w9,0x407520         ; store 失败 ⇒ 重试（`fetch_sub` 的 LL/SC 循环）
    /// 0x407530  b.ne 0x407540            ; **减完非 0** ⇒ 还有人持有，跳过 drop_slow
    /// 0x407534  dmb  ishld
    /// 0x40753c  bl   0x45f2e4             ; Arc::drop_slow（最后一个引用）
    /// 0x407540  add  x0,x19,#0x230
    /// 0x407544  bl   0x3c5f2c             ; RawTable::drop（handler 里的 HashMap，**无条件**）
    /// 0x407548  str  xzr,[x21]            ; 槽 = 0 = 空（两条路都汇合到这里 ⇒ 空槽调用也幂等）
    /// ```
    ///
    /// ⇒ 语义：**空槽什么都不做；非空则 stop() → Arc 强计数减一 → （减到 0 才）drop_slow
    /// → 丢 HashMap → 槽置空**。
    ///
    /// 三个不显然的点：
    /// 1. **顺序是 stop 先于解引用**（`0x40751c` < `0x407520`）：先让 handler 自己收尾，
    ///    再放掉它持有的共享句柄 —— 反过来会让 `stop` 在计数已减的 `Arc` 上跑；
    /// 2. **`Arc::drop_slow` 只对最后一个引用**（`0x407530 b.ne` 跳过）—— 与 `stop_lan_broadcaster`
    ///    里"无条件 dealloc"的形态不同（那边没有引用计数）；
    /// 3. `HashMap` 的释放**在 Arc 之后、且无条件**（`0x407540`）：它是 handler 的**内联字段**
    ///    （`+0x230`），所以不管 Arc 有没有别人持有，这段必须在本函数的槽被清空前丢掉。
    ///
    /// ⚠️ 档位：**观测**（逐指令）。`GraphwarServerBrokerHandler::stop`（0x489e50）的体内没读
    /// ⇒ "stop 会不会自己清空槽"未知（我方模型：不会，槽由本函数写 0）。
    pub fn stop_server_broker_handler(&mut self) -> Vec<StopBrokerStep> {
        let mut steps = Vec::new();
        // 0x407510 cbz x20：空槽 ⇒ 一步不推（0x407548 的写 0 仍执行，我方 None 等价）
        if let Some(slot) = self.broker {
            steps.push(StopBrokerStep::HandlerStop); // 0x40751c
            steps.push(StopBrokerStep::ArcRelease); // 0x407520..0x40752c
            // 0x407524 subs x8,x8,#1 + 0x407530 b.ne：**减完恰好 0** 才 drop_slow
            if slot.arc_strong.wrapping_sub(1) == 0 {
                steps.push(StopBrokerStep::ArcDropSlow); // 0x40753c
            }
            steps.push(StopBrokerStep::MapDrop); // 0x407540/0x407544
        }
        self.broker = None; // 0x407548 str xzr,[x21]
        steps
    }

    /// `resubscribe_to_events`（x86 **`0x514d80`** / FDE `0x112`；arm64 `0x407114`）。
    ///
    /// 把 `+0x1d8` 那张登记表**逐条**再走一次 [`Self::subscribe_to_event_with_id`]，
    /// 返回重订阅条数。客户端槽为空（`+0x1e0 == None`）时**每次调用都走"客户端为 None"的早退分支**
    /// （`0x514e2e mov rsi,[rbx+0x1e0]` + `0x514e38 cmovne`：为 0 就传 null）。
    pub fn resubscribe_to_events(&mut self) -> usize {
        // 原版先 borrow() 整张表再逐条调；我方按 BTreeMap 的键序（原版是哈希序）。
        let entries: Vec<(u32, String)> = self
            .subscriptions
            .iter()
            .map(|(&id, name)| (id, name.clone()))
            .collect();
        let mut n = 0;
        for (id, name) in entries {
            // 0x514e52 call 0x515320：实参 (self, Option<&mut client>, name, id)
            if self.subscribe_to_event_with_id(name.as_str(), id) {
                n += 1;
            }
        }
        n
    }

    /// `subscribe_to_event_with_id`（x86 **`0x515320`** / FDE `0xc9`；arm64 `0x407580`）。
    ///
    /// 逐指令（x86）：
    /// ```text
    /// 0x515334 test rsi,rsi / je 0x515399     ; 客户端为 None ⇒ 释放 Rc 后返回（什么都不注册）
    /// 0x515355 mov edi,0x10 / mov esi,8       ; 分配 16 字节闭包捕获体
    /// 0x51536a mov [rax],rbx                  ;   捕获[0] = self（登记表 Rc 指针）
    /// 0x51536d mov [rax+8],ebp                ;   捕获[8] = 事件 id（u32）
    /// 0x515370 add r12,8                      ; GraphwarEventHandler 在内联客户端 +0x08
    /// 0x515374 lea r8,[0x961fa8]              ; dyn FnMut vtable（见下）
    /// 0x515393 jmp [0x9bac88] -> 0x3d38f0      ; tail-call GraphwarEventHandler::subscribe_to_event
    /// ```
    ///
    /// **闭包本体（本轮新证据，解掉 T82 的 2 条 `unknown`）**：`0x961fa8` 这张 vtable 是
    /// `dyn FnMut(&GraphwarEvent)` 的布局 —— `+0x00` drop_in_place = `0x520ce0`、
    /// `+0x08` size = `0x10`（= 上面的 16 字节捕获体）、`+0x10` align = `8`、
    /// **`+0x18` = call_mut = `0x520100`**、**`+0x20` = call_once = `0x5153f0`**。
    /// ⇒ `fixtures/function_coverage.tsv` 里两条 `F3 parent-missing`（`0x5153f0` / `0x520100`）
    /// 是**同一份源码闭包的 `call_once` / `call_mut` 两个编译器 shim**，不是两个源码函数。
    /// 闭包体（arm64 `0x407688` 有真名）：`self.<表>.borrow_mut().get_mut(&id)` 命中就
    /// `handler(event)`（arm64 `0x407760 ldp x0,x8,[x15,#-0x10]` + `0x407764 ldr x8,[x8,#0x20]`
    /// + `0x40776c blr x8`：**经 vtable +0x20 = call_mut 再派发一层**）。
    ///
    /// ⚠️ 档位：**推断**（函数名只有 hyb 预测；`+0x1d8` 容器值类型未定）。注册动作本身是**观测**。
    pub fn subscribe_to_event_with_id(&mut self, name: &str, id: u32) -> bool {
        // 0x515334：客户端为 None ⇒ 立刻返回（连捕获体都不分配）。
        let Some(client) = self.client.as_mut() else {
            return false;
        };
        // 0x515355..0x51536d：捕获 (登记表, id)；我方建模成"记一条 (id, name) 到客户端侧"，
        // 因为闭包体做的事就是"按 id 查表再派发"。
        client.registered.push((id, name.to_string()));
        // 0x515393：tail-call GraphwarEventHandler::subscribe_to_event（0x3d38f0）—— 那一条
        // 已在 `src/graphwar_event_handler.rs` 落过码（本模块不重复实现）。
        true
    }

    /// `start_client_to_replay`（x86 **`0x514f20`** / FDE `0x213`；arm64 `0x4072b0`）。
    ///
    /// 逐指令的 7 步见 [`ReplayStartStep`]；两条**不显然**的判据：
    /// * `0x514fe8 cmp dword [rbx],2; je skip` —— 旧槽 `tag == 2` 才**不**析构旧播放器；
    /// * `0x515041` 同一条比较是**第二次**读，`== 2` 走 panic 分支（`0x515066` 用
    ///   `src/graphwar_backend.rs` 的 Location 报错，`0x1b02f9`）⇒ 装完播放器后判别字**不允许**是 2。
    ///
    /// ⚠️ 缺什么（写清，不猜）：这条函数**没有任何局面/网络副作用**能在 headless 里复现 ——
    /// 它建的是 gdext 客户端对象。我方落的是"装配顺序 + 地址常量"，返回步骤列表供测试钉顺序；
    /// **不**声称行为已逐位对上。
    pub fn start_client_to_replay(
        &mut self,
        replay: crate::graphwar_replay_player::GraphwarReplayPlayer,
    ) -> Vec<ReplayStartStep> {
        let mut steps = Vec::with_capacity(7);

        // ① 0x514f4f inc qword [rdi+0x1d0] + 0x514f6f call 0x517690（GraphwarGameClient::new）
        self.shared_refs += 1;
        self.client_addr = Some(REPLAY_CLIENT_ADDR);
        steps.push(ReplayStartStep::NewClient);

        // ② 0x514f9a call 0x3d6d70（GraphwarReplayPlayer::new）
        steps.push(ReplayStartStep::NewReplayPlayer);

        // ③ 0x514fe8 cmp dword [rbx],2：旧槽不是活的播放器(tag==2)才 drop 旧的
        if self.replay_slot_tag != 2 && self.replay.is_some() {
            steps.push(ReplayStartStep::DropOldPlayer);
        }

        // ④ 0x514fa6 cmp qword [rbx+0x1e0],0 / 0x514fb3 call drop_in_place<GraphwarGameClient>
        //    然后把新客户端整块搬进 +0x1e0（0x514fb8..0x514fe4）。
        self.client = Some(ReplayClientSlot::default());
        steps.push(ReplayStartStep::InstallClient);

        // ⑤ 0x514ff5..0x515036：把新播放器整块搬进 +0x00..0x60（0x60 字节）。
        self.replay = Some(replay);
        self.replay_slot_tag = 0;
        steps.push(ReplayStartStep::InstallPlayer);

        // ⑥ 0x51503c call 0x514d80
        self.resubscribe_to_events();
        steps.push(ReplayStartStep::Resubscribe);

        // ⑦ 0x515052 call 0x3d68b0（实参：self = 播放器槽地址、rsi = &self.client）
        //    前置：0x515041 tag != 2、0x515046 client 非空（否则 panic）。
        if let Some(client) = self.client.as_mut() {
            if let Some(player) = self.replay.as_mut() {
                let _ = client; // 原版把客户端也传进去；trait 侧的注入目标不在本模块
                player.go_to_next_checkpoint(&mut crate::graphwar_replay_player::NoInjectTarget);
            }
        }
        steps.push(ReplayStartStep::GoToNextCheckpoint);
        steps
    }
}

#[cfg(test)]
mod lifecycle_tests {
    //! T103 落码的四条生命周期函数（`setup_level` / `stop_client` / `stop_server` / `start_server`）。
    //!
    //! **期望值来源**：全部来自 arm64 反汇编里读出来的**分支条件与调用点顺序**
    //! （`llvm-objdump -d --start-address=0x406b4c --stop-address=0x406d3c <arm64 so>`），
    //! **不来自**我方实现的输出（AGENTS.md §0.5）。
    use super::*;

    /// `0x406be8 mov w8,#0x2` / `0x406c48 mov w8,#0x4`：两个槽的"空"判别字。
    /// 契约断言（不含语义）：钉住常量与指令立即数的对应关系。
    #[test]
    fn empty_slot_tags_match_the_instruction_immediates() {
        assert_eq!(PLAYER_SLOT_EMPTY, 2, "0x406be8 mov w8,#0x2；0x406bd4 cmp x8,#0x2");
        assert_eq!(SERVER_SLOT_EMPTY, 4, "0x406c48 mov w8,#0x4；0x406c20 cmp x8,#0x4");
    }

    /// `setup_level` 的两道守卫，**顺序**是被测点（arm64 先 server `0x406b50` 后 client `0x406b5c`）。
    #[test]
    fn setup_level_requires_server_then_client() {
        // ① 没有 server：即使 client 在也直接返回（0x406b54 b.eq）
        let mut b = GraphwarBackend::new();
        b.client = Some(ReplayClientSlot::default());
        assert!(b.setup_level(1, 2).is_empty(), "server 槽空 ⇒ 不建栈帧就 ret");

        // ② 有 server、没有 client：也返回（0x406b5c cbz）
        let mut b = GraphwarBackend::new();
        b.server = Some(ServerSlot { arg0: 0, arg1: 0, started: true });
        assert!(b.setup_level(1, 2).is_empty(), "client 槽空 ⇒ ret");

        // ③ 两个都在：先 wait_for_players（0x406b80），再尾调用 server.setup_level（0x406b9c）
        b.client = Some(ReplayClientSlot::default());
        assert_eq!(
            b.setup_level(1, 2),
            vec![SetupLevelStep::WaitForPlayers, SetupLevelStep::ServerSetupLevel],
            "顺序 = 调用点地址顺序 0x406b80 → 0x406b9c"
        );
    }

    /// `stop_client`：客户端那段是**有才拆**；播放器那段按判别字 2 决定 drop。
    #[test]
    fn stop_client_disconnects_then_clears_both_slots() {
        // ① 两个槽都满：disconnect → drop client → drop player → 判别字=2
        let mut b = GraphwarBackend::new();
        b.client = Some(ReplayClientSlot::default());
        b.replay = Some(crate::graphwar_replay_player::GraphwarReplayPlayer::new());
        b.replay_slot_tag = 0; // != 2 ⇒ 槽里是活的播放器
        assert_eq!(
            b.stop_client(),
            vec![
                StopClientStep::DisconnectClient,
                StopClientStep::DropClient,
                StopClientStep::DropPlayer
            ],
            "0x406bc0 → 0x406bc8 → 0x406be0"
        );
        assert!(b.client.is_none(), "0x406bd0 str xzr,[x19,#0x1e0]");
        assert_eq!(b.replay_slot_tag, PLAYER_SLOT_EMPTY, "0x406be8");

        // ② 客户端为空：跳过前两步；播放器槽已经是 2 ⇒ 连 drop 也不做
        let mut b = GraphwarBackend::new();
        b.replay_slot_tag = PLAYER_SLOT_EMPTY;
        assert!(b.stop_client().is_empty(), "0x406bb8 cbz + 0x406bd8 b.eq 两条都命中");
        assert_eq!(b.replay_slot_tag, PLAYER_SLOT_EMPTY);
    }

    /// `stop_server`：**stop 之后要重读判别字**（`0x406c34`）⇒ 二次判据。
    #[test]
    fn stop_server_stops_then_drops_only_if_still_present() {
        // ① 空槽：一步都不做（0x406c24 b.eq 直接跳 0x406c44 写 4）
        let mut b = GraphwarBackend::new();
        assert!(b.stop_server().is_empty());
        assert!(b.server.is_none());

        // ② 有 server：stop → drop → 清空
        let mut b = GraphwarBackend::new();
        b.server = Some(ServerSlot { arg0: 7, arg1: 9, started: true });
        assert_eq!(
            b.stop_server(),
            vec![StopServerStep::StopServer, StopServerStep::DropServer],
            "0x406c2c → 0x406c40"
        );
        assert!(b.server.is_none(), "0x406c48 str w8(=4)");
        // `stop` 自己把槽清空那条岔路（0x406c38 b.eq）我方建模不出来：
        // 需要 `GraphwarGameServer::stop` 的函数体，本轮**未读** ⇒ 不写断言（plan L3）。
    }

    /// `start_server`：`new` + `start` **先于**"拆旧槽"，这条顺序是不显然之处。
    #[test]
    fn start_server_builds_then_replaces() {
        // ① 首次装：只有 new + start
        let mut b = GraphwarBackend::new();
        assert_eq!(
            b.start_server(35245, 1),
            vec![StartServerStep::NewServer, StartServerStep::StartServer],
            "0x406c8c → 0x406c94，此时 0x406ca8 判据为 **空**"
        );
        assert_eq!(
            b.server,
            Some(ServerSlot { arg0: 35245, arg1: 1, started: true }),
            "0x406ccc..0x406ce8 把栈上对象整块搬进 self+0x60"
        );

        // ② 再装一次：旧槽非空 ⇒ 多一步 DropOldServer，且它排在 new/start **之后**
        let steps = b.start_server(35246, 2);
        assert_eq!(
            steps,
            vec![
                StartServerStep::NewServer,
                StartServerStep::StartServer,
                StartServerStep::DropOldServer
            ],
            "顺序证据：0x406c94(start) < 0x406c9c(读旧判别字) < 0x406cc8(drop 旧)"
        );
        assert_eq!(b.server.unwrap().arg0, 35246, "新对象覆盖旧槽");
    }

    /// `subscribe_to_event`（arm64 0x406e80 / x86 0x514b40）：**全局计数器分配 id → 登记 → 转给
    /// `subscribe_to_event_with_id`**。期望值全部来自指令（`0x406ebc`–`0x406ec4` 的 fetch_add、
    /// `0x406f48` 的 insert、`0x406fd0` 的调用点）。
    #[test]
    fn subscribe_to_event_allocates_an_id_then_delegates() {
        let mut b = GraphwarBackend::new();
        // 没有客户端 ⇒ 被调者 subscribe_to_event_with_id 早退返回 false（0x515334 / 0x4075xx 的判据）
        assert!(!b.subscribe_to_event("tick_event"), "client 槽空 ⇒ 返回 false");
        // 但**登记已经发生**（insert 在调用之前：0x406f48 < 0x406fd0）
        assert_eq!(b.subscriptions.len(), 1, "表项在转调之前就插进去了");

        // 有客户端 ⇒ true，且客户端的按 id 注册表收到 (id, name)
        b.client = Some(ReplayClientSlot::default());
        assert!(b.subscribe_to_event("pos_info_event"));
        let reg = &b.client.as_ref().unwrap().registered;
        assert_eq!(reg.len(), 1);
        assert_eq!(reg[0].1, "pos_info_event");
        assert!(b.subscribe_to_event("name_info_event"), "被调者那一步成功 ⇒ 返回 true");
        assert_eq!(b.subscriptions.len(), 3);
    }

    /// 计数器语义 = **单调 +1、不重复**（`0x406ec0 add w9,w22,#1`）。
    /// 用 200 次连续调用钉住，而不是断言绝对 id —— 初值未知、且测试并行会推进同一个全局计数器。
    #[test]
    fn subscription_ids_are_strictly_increasing_and_unique() {
        let mut b = GraphwarBackend::new();
        b.client = Some(ReplayClientSlot::default());
        for i in 0..200u32 {
            let name = format!("ev{}", i);
            assert!(b.subscribe_to_event(&name), "{}", name);
        }
        let ids: Vec<u32> = b.client.as_ref().unwrap().registered.iter().map(|(i, _)| *i).collect();
        assert_eq!(ids.len(), 200);
        for w in ids.windows(2) {
            assert!(w[1] > w[0], "id 必须严格递增：{:?} → {:?}", w[0], w[1]);
        }
        // 登记表（+0x1d8）与客户端按 id 注册（+0x08）用的是**同一个** id
        for (id, name) in b.client.as_ref().unwrap().registered.iter() {
            assert_eq!(b.subscriptions.get(id), Some(name), "两处 id 必须一致");
        }
    }

    /// 四条函数都不碰 `+0x1d8` 订阅表与 `+0x1d0` 共享计数（反汇编里 0 条相关指令）——
    /// 反面对照：`resubscribe_to_events` 会碰，说明"不动"是我方实现的可测差异面。
    #[test]
    fn lifecycle_functions_do_not_touch_subscription_state() {
        let mut b = GraphwarBackend::new();
        b.subscriptions.insert(30, "game_state_info_event".into());
        b.client = Some(ReplayClientSlot::default());
        b.replay_slot_tag = 0;
        let _ = b.setup_level(0, 0);
        let _ = b.stop_client();
        let _ = b.stop_server();
        let _ = b.start_server(1, 2);
        assert_eq!(b.subscriptions.len(), 1, "四条函数体内没有访问 +0x1d8 的指令");
        assert_eq!(b.shared_refs, 0, "也没有 inc/dec [self+0x1d0]");
    }
}

#[cfg(test)]
mod replay_session_tests {
    use super::*;

    #[test]
    fn resubscribe_walks_every_registered_id() {
        let mut b = GraphwarBackend::new();
        b.subscriptions.insert(30, "game_state_info_event".into());
        b.subscriptions.insert(44, "effect_info_event".into());
        // 没有客户端 ⇒ 每次都走"客户端为 None"的早退分支（0x515334 je）
        assert_eq!(b.resubscribe_to_events(), 0);
        b.client = Some(ReplayClientSlot::default());
        assert_eq!(b.resubscribe_to_events(), 2);
        assert_eq!(b.client.as_ref().unwrap().registered.len(), 2);
        assert_eq!(b.resubscribe_to_events(), 2, "第二次再走一遍（重订阅 = 再来一次）");
        assert_eq!(b.client.as_ref().unwrap().registered.len(), 4, "两轮各注册 2 条");
    }

    #[test]
    fn subscribe_with_id_needs_a_client() {
        let mut b = GraphwarBackend::new();
        assert!(!b.subscribe_to_event_with_id("tick_event", 0));
        b.client = Some(ReplayClientSlot::default());
        assert!(b.subscribe_to_event_with_id("tick_event", 0));
        assert_eq!(
            b.client.as_ref().unwrap().registered,
            vec![(0u32, "tick_event".to_string())],
            "捕获体 = (登记表, id)，注册到内联客户端的 +0x08"
        );
    }

    #[test]
    fn start_client_to_replay_installs_in_binary_order() {
        let mut b = GraphwarBackend::new();
        let steps = b.start_client_to_replay(
            crate::graphwar_replay_player::GraphwarReplayPlayer::new(),
        );
        assert_eq!(
            steps,
            vec![
                ReplayStartStep::NewClient,
                ReplayStartStep::NewReplayPlayer,
                ReplayStartStep::InstallClient,
                ReplayStartStep::InstallPlayer,
                ReplayStartStep::Resubscribe,
                ReplayStartStep::GoToNextCheckpoint,
            ],
            "第一次进来旧槽 tag != 2 但 replay 为空 ⇒ 没有 DropOldPlayer"
        );
        assert_eq!(b.client_addr, Some(REPLAY_CLIENT_ADDR));
        assert_eq!(REPLAY_CLIENT_ADDR.0[15], 1, "IPv6 环回 ::1（0x19d440 的 16 字节）");
        assert_eq!(REPLAY_CLIENT_ADDR.1, 0, "端口 0（0x514f38 的 qword 0 覆盖出来）");
        assert_eq!(b.shared_refs, 1, "0x514f4f inc qword [rdi+0x1d0]");
    }

    #[test]
    fn start_client_to_replay_drops_old_live_player() {
        let mut b = GraphwarBackend::new();
        b.replay = Some(crate::graphwar_replay_player::GraphwarReplayPlayer::new());
        b.replay_slot_tag = 0; // 0 != 2 ⇒ 旧槽是活的播放器，要先 drop
        let steps = b.start_client_to_replay(
            crate::graphwar_replay_player::GraphwarReplayPlayer::new(),
        );
        assert!(steps.contains(&ReplayStartStep::DropOldPlayer), "0x514fe8 判据");
    }
}

#[cfg(test)]
mod denom_t106_tests {
    //! T106：**527 分母之外**的 5 条 `graphwar_backend` 源码函数。
    //!
    //! **期望值来源**：全部来自 Android 2.0.60 **arm64** 反汇编的具体指令
    //! （`llvm-objdump -d --start-address=<vaddr> --stop-address=<vaddr+size>
    //! `.reference/steam/android-2.0.60/libgraphwarrust.so``），**不来自**我方实现的输出
    //! （AGENTS.md §0.5）。x86 侧这些函数没有 `panic::Location` 锚 ⇒ 档位只能到 **观测**。
    use super::*;

    /// `get_next_clip_checkpoint` = **upper_bound**（严格大于）。用例覆盖
    /// 0x4074c4（等值 +1）、0x4074dc（小于 +1）、0x4074e0（大于不动）、0x4074cc/0x4074e4（越界 None）、
    /// 0x407464（槽空 None）、0x40746c（空表 None）六条分支。
    #[test]
    fn get_next_clip_checkpoint_is_a_strict_upper_bound() {
        let mut b = GraphwarBackend::new();
        b.replay_slot_tag = 0; // != 2 ⇒ 槽里有活的播放器（0x407460 判据）
        b.clip_checkpoints = vec![100, 200, 300, 400];

        // ① 等值 ⇒ 返回**下一个**（0x4074c4 相等就 lo+1）
        assert_eq!(b.get_next_clip_checkpoint(200), Some(300), "0x4074c4：相等不算命中");
        // ② 落在两值之间 ⇒ 返回上一个的上界（0x4074dc 小于 ⇒ lo+1）
        assert_eq!(b.get_next_clip_checkpoint(250), Some(300), "0x4074dc cinc …lo");
        // ③ 小于首元素 ⇒ 返回首元素（0x4074e0 lo 不动）
        assert_eq!(b.get_next_clip_checkpoint(0), Some(100), "0x4074e0");
        // ④ 等于末元素 ⇒ 越界（0x4074cc cmp x10,x9 / b.hs）
        assert_eq!(b.get_next_clip_checkpoint(400), None, "0x4074e4 越界");
        // ⑤ 大于一切 ⇒ None
        assert_eq!(b.get_next_clip_checkpoint(u32::MAX), None, "0x4074e4 越界");
    }

    /// 重复值：`[100,100,200]` 对 `100` 必须返回 `200`（等值跳过**所有**相同值）。
    /// 原版靠"相等 ⇒ lo+1"这一条实现，没有去重逻辑（0x4074c4 之后没有回退循环）。
    #[test]
    fn get_next_clip_checkpoint_skips_duplicates() {
        let mut b = GraphwarBackend::new();
        b.replay_slot_tag = 0;
        b.clip_checkpoints = vec![100, 100, 200];
        assert_eq!(b.get_next_clip_checkpoint(100), Some(200));
        // 单个元素 + 等值 ⇒ 空（0x407478 len==1 的 `mov x10,xzr` 路径）
        b.clip_checkpoints = vec![7];
        assert_eq!(b.get_next_clip_checkpoint(7), None, "len==1 且相等 ⇒ 越界 None");
        assert_eq!(b.get_next_clip_checkpoint(6), Some(7), "len==1 且小于 ⇒ 该元素");
    }

    /// 两个早退判据：槽空（0x407464）与空表（0x40746c → 0x407480）。
    #[test]
    fn get_next_clip_checkpoint_early_returns() {
        // ① 槽判别字 == 2（空）：表里就算有东西也不读
        let mut b = GraphwarBackend::new();
        b.clip_checkpoints = vec![10, 20];
        b.replay_slot_tag = PLAYER_SLOT_EMPTY;
        assert_eq!(b.get_next_clip_checkpoint(5), None, "0x407464 b.eq");

        // ② 槽活着但表空
        b.replay_slot_tag = 0;
        b.clip_checkpoints.clear();
        assert_eq!(b.get_next_clip_checkpoint(5), None, "0x40746c cbz x9");
    }

    /// `0x407e10 mov x9,#-0x7fffffffffffffff` + `0x407e1c str x9,[x21,#0x110]`：LAN 槽的"空"编码。
    /// 契约断言（不含语义）：钉住常量与指令立即数的对应关系。
    #[test]
    fn lan_slot_empty_sentinel_matches_instructions() {
        assert_eq!(LAN_SLOT_EMPTY, 0x8000_0000_0000_0001, "0x407e10 / 0x407080 / 0x407234");
        assert_eq!(LAN_FD_CLOSED, -1, "0x3de0f0 str w8(=-1)；0x407090 cmn w0,#0x1");
        assert_eq!(BackendField::LAN_SLOT_SIZE, 0xc0, "8 组 ldp/stp q = 0xC0 字节");
        assert_eq!(BackendField::LAN_FD, 0x110 + 0xb8, "对象内 +0xb8 ⇒ 绝对 +0x1c8");
        assert_eq!(BackendField::LAN_TABLE, 0x110 + 0x78, "对象内 +0x78 ⇒ 绝对 +0x188");
        // 新后端里 LAN 槽必须是空的（new 写的就是那个哨兵）
        assert!(GraphwarBackend::new().lan_broadcaster.is_none());
    }

    /// `start_lan_broadcaster`：**只在空槽时装**（0x40723c `b.ne`），装的时候顺序是
    /// new(0x40724c) → start(0x407254) → 整块搬入(0x407258..)。第二次调用一步都不推。
    #[test]
    fn start_lan_broadcaster_only_installs_into_an_empty_slot() {
        let mut b = GraphwarBackend::new();
        assert_eq!(
            b.start_lan_broadcaster(35245),
            vec![
                StartLanBroadcasterStep::NewBroadcaster,
                StartLanBroadcasterStep::StartBroadcaster,
                StartLanBroadcasterStep::Install,
            ],
            "顺序 = bl 地址顺序 0x40724c → 0x407254 → 0x407258"
        );
        let slot = b.lan_broadcaster.expect("0x407284 之后槽里是活的广播者");
        assert_eq!(slot.port, 35245, "0x407248 mov w0,w1：实参原样进 new");
        assert_eq!(slot.fd, LAN_FD_CLOSED, "new 写 -1（0x3de0f0）；start 之后的真值本轮未知");

        // 第二个次调用：槽非空 ⇒ 0x40723c b.ne 直接返回，连 new 都不调
        assert!(b.start_lan_broadcaster(9999).is_empty(), "0x40723c b.ne 0x407288");
        assert_eq!(b.lan_broadcaster.unwrap().port, 35245, "旧槽不被覆盖（与 start_server 相反）");
    }

    /// `stop_lan_broadcaster` 的四条分支：①空槽 ②fd 打开 + 长度合法（四步全走）
    /// ③fd 已关 + 长度 0（跳过 close 与 dealloc）④长度 = `0x8000…0000`（跳过 dealloc）。
    /// 判据地址：0x407090（cmn）、0x4070a8（RawTable::drop）、0x4070b0/0x4070b4、0x4070b8。
    #[test]
    fn stop_lan_broadcaster_walks_four_guards() {
        use StopLanBroadcasterStep::*;

        // ① 空槽：一步不推，且写回哨兵（= 我方 None，幂等）
        let mut b = GraphwarBackend::new();
        assert!(b.stop_lan_broadcaster().is_empty(), "0x407088 b.eq");
        assert!(b.lan_broadcaster.is_none(), "0x4070cc 写回空哨兵");

        // ② fd=7、长度 64：close → fd=-1 → 丢表 → dealloc
        let mut b = GraphwarBackend::new();
        b.lan_broadcaster = Some(LanBroadcasterSlot { port: 35245, fd: 7, buffer_len: 64 });
        assert_eq!(
            b.stop_lan_broadcaster(),
            vec![CloseSocket, ResetFd, DropTable, DeallocBuffer],
            "地址顺序 0x407098 → 0x40709c → 0x4070a8 → 0x4070c8"
        );
        assert!(b.lan_broadcaster.is_none());

        // ③ fd 已关（-1）+ 长度 0：两条跳过判据都命中 ⇒ 只剩 ResetFd + DropTable
        let mut b = GraphwarBackend::new();
        b.lan_broadcaster = Some(LanBroadcasterSlot { port: 1, fd: LAN_FD_CLOSED, buffer_len: 0 });
        assert_eq!(b.stop_lan_broadcaster(), vec![ResetFd, DropTable], "0x407094 b.eq + 0x4070b8 cbz");

        // ④ 长度 == 0x8000_0000_0000_0000：只跳过 dealloc（close 照做）
        let mut b = GraphwarBackend::new();
        b.lan_broadcaster =
            Some(LanBroadcasterSlot { port: 2, fd: 9, buffer_len: 0x8000_0000_0000_0000 });
        assert_eq!(b.stop_lan_broadcaster(), vec![CloseSocket, ResetFd, DropTable], "0x4070b4 b.eq");
    }

    /// 与 `start_lan_broadcaster` 配对的正对照：装进去（fd=−1、长度 0）再停，
    /// 只应推 `ResetFd + DropTable`（close 与 dealloc 都被判据跳过）。
    #[test]
    fn start_then_stop_lan_broadcaster_skips_socket_and_buffer() {
        use StopLanBroadcasterStep::*;
        let mut b = GraphwarBackend::new();
        b.start_lan_broadcaster(35245);
        assert_eq!(b.stop_lan_broadcaster(), vec![ResetFd, DropTable]);
        assert!(b.lan_broadcaster.is_none());
    }

    /// broker 槽的布局契约：handler 对象 0x38 字节、`HashMap` 在对象内 +0x8（绝对 +0x230）、
    /// 空编码 = 首字 0（**不是** LAN 那个 0x8000…0001 哨兵）。指令：0x407800–0x40781c / 0x4077d0 / 0x407510。
    #[test]
    fn broker_slot_layout_matches_instructions() {
        assert_eq!(BackendField::BROKER_SLOT_SIZE, 0x38, "0x407800..0x40781c 拷 0x38 字节");
        assert_eq!(BackendField::BROKER_MAP, BackendField::BROKER + 8, "对象内 +0x8");
        assert_eq!(BackendField::BROKER + 0x38, 0x260, "槽结束位置（new 的 0x407e28 写 +0x258）");
        assert!(GraphwarBackend::new().broker.is_none(), "new 把这段写成 0（0x407e14/0x407e24）");
    }

    /// `start_server_broker_handler`：**只在空槽时装**（0x4077d0 `cbz`），顺序 new → start → 装。
    #[test]
    fn start_server_broker_handler_only_installs_into_an_empty_slot() {
        let mut b = GraphwarBackend::new();
        assert_eq!(
            b.start_server_broker_handler(35245),
            vec![StartBrokerStep::NewHandler, StartBrokerStep::StartHandler, StartBrokerStep::Install],
            "顺序 = bl 地址顺序 0x4077f0 → 0x4077fc → 0x407814"
        );
        assert_eq!(b.broker.unwrap().arg, 35245, "0x4077f4 mov x1,x19：实参原样进 start");

        // 第二次：槽非空 ⇒ 0x4077d0 cbz 走 0x4077d4 的 epilogue，连 new 都不调
        assert!(b.start_server_broker_handler(1).is_empty(), "0x4077d0 cbz 0x4077e4 的反面");
        assert_eq!(b.broker.unwrap().arg, 35245, "旧槽不被覆盖");
    }

    /// `stop_server_broker_handler` 的四条分支：①空槽 ②强计数 1（最后一个引用 ⇒ 四步全走）
    /// ③强计数 2（跳过 `Arc::drop_slow`，0x407530 b.ne）④强计数 0（wrapping：subs 得 MAX ≠ 0 ⇒ 也跳过）。
    #[test]
    fn stop_server_broker_handler_walks_arc_and_map_guards() {
        use StopBrokerStep::*;

        // ① 空槽：一步不推（0x407510 cbz）
        let mut b = GraphwarBackend::new();
        assert!(b.stop_server_broker_handler().is_empty(), "0x407510 cbz x20,0x407548");
        assert!(b.broker.is_none());

        // ② 最后一个引用：stop → 减一 → drop_slow → 丢表
        let mut b = GraphwarBackend::new();
        b.broker = Some(BrokerSlot { arg: 35245, arc_strong: 1, map_entries: 3 });
        assert_eq!(
            b.stop_server_broker_handler(),
            vec![HandlerStop, ArcRelease, ArcDropSlow, MapDrop],
            "地址顺序 0x40751c → 0x407520 → 0x40753c → 0x407544"
        );
        assert!(b.broker.is_none(), "0x407548 str xzr");

        // ③ 还有别的引用：drop_slow 被 0x407530 b.ne 跳过
        let mut b = GraphwarBackend::new();
        b.broker = Some(BrokerSlot { arg: 1, arc_strong: 2, map_entries: 0 });
        assert_eq!(b.stop_server_broker_handler(), vec![HandlerStop, ArcRelease, MapDrop]);

        // ④ 强计数 0（模型里的异常态）：`subs` 借位得 usize::MAX ≠ 0 ⇒ 同样跳过 drop_slow
        let mut b = GraphwarBackend::new();
        b.broker = Some(BrokerSlot { arg: 2, arc_strong: 0, map_entries: 0 });
        assert_eq!(b.stop_server_broker_handler(), vec![HandlerStop, ArcRelease, MapDrop]);
    }

    /// 与 `start_server_broker_handler` 配对的正对照：装（arc_strong=1）再停 ⇒ 必须走到 `ArcDropSlow`
    /// （证明两个函数的槽语义自洽：装进来的就是"唯一持有者"）。
    #[test]
    fn start_then_stop_broker_handler_drops_the_only_arc() {
        use StopBrokerStep::*;
        let mut b = GraphwarBackend::new();
        b.start_server_broker_handler(35245);
        assert_eq!(
            b.stop_server_broker_handler(),
            vec![HandlerStop, ArcRelease, ArcDropSlow, MapDrop]
        );
        assert!(b.broker.is_none());
    }

    /// 差分对拍：我方这段循环与 `partition_point(|&c| c <= current)`（upper_bound 的另一种写法）
    /// 在 200 组随机升序表上必须逐值相同 —— 这是**规格级**对拍（两边都是同一句话的实现），
    /// 用来抓"二分收敛位置差一"这类手抄错误，不当作复原证据。
    #[test]
    fn get_next_clip_checkpoint_matches_partition_point() {
        let mut seed: u64 = 0x1234_5678_9abc_def0;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..200 {
            let n = (next() % 12) as usize;
            let mut table: Vec<u32> = (0..n).map(|_| (next() % 64) as u32).collect();
            table.sort_unstable(); // 原版假设升序
            let mut b = GraphwarBackend::new();
            b.replay_slot_tag = 0;
            b.clip_checkpoints = table.clone();
            for key in 0..=66u32 {
                let want = {
                    let i = table.partition_point(|&c| c <= key);
                    table.get(i).copied()
                };
                assert_eq!(b.get_next_clip_checkpoint(key), want, "表={:?} key={}", table, key);
            }
        }
    }
}
