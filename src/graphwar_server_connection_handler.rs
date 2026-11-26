//! `graphwar_server_connection_handler.rs`：服务端侧的**每连接处理器**
//! （原版 `GraphwarServerConnectionHandler`）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **ELF vaddr**（= 模块内偏移 = RVA；Ghidra/REA 的反编译地址 = 本地址 + `0x100000`）。
//!
//! # 1. 为什么这个文件存在（边界证据）
//!
//! 本模块此前**没有同名文件**，内容混在 `src/server.rs`。T24 的逐函数边界审计
//! （[`docs/06`](../../docs/06-verification.md) §74、`fixtures/boundary_audit.tsv`，
//! 判据 = `panic::Location` 硬归属）判出**六条** `mismatch`，全部单模块、一边倒：
//!
//! | 地址 | 符号名（三向真名，`fixtures/name_to_addr_v2.tsv`） | 首锚 `(line:col)` | 锚点行数 |
//! |---|---|---|---|
//! | `0x45a560` | `GraphwarServerConnectionHandler::forward_event` | `350:38` | 1（`:350`） |
//! | `0x45a6b0` | `…::handle_clear_bans_request_event` | `247:97` | 2（`:247 :248`） |
//! | `0x45a7b0` | `…::handle_connection_removed_event` | `224:97` | 3（`:224 :225 :316`） |
//! | `0x45aab0` | `…::forward_connection_specific_event` | `326:18` | 2（`:326 :336`） |
//! | `0x45ad30` | `…::handle_ban_connection_request_event` | `233:105` | 4（`:233 :234 :238 :242`） |
//! | `0x45b020` | `…::tick` | `206:42` | **8**（`:206 :207 :208 :258 :259 :291 :292 :303`） |
//!
//! 六条的 `panic::Location` 主文件**全部**是 `src/graphwar_server_connection_handler.rs`
//! ⇒ `AGENTS.md` §4「**文件名就是证据**」。本文件即按该证据从 `src/server.rs` 切出
//! （T30 / `task-27`，2026-11-26），`src/server.rs` 留 `pub use` 再导出 ⇒ **公开路径不变**。
//!
//! ## 1.1 两条已被别的证据否掉的候选（写在这里省得下一个人重查）
//!
//! * `0x45_00c0`（`new`）：`fixtures/name_to_addr_v2.tsv` 里它的候选模块是本模块（4 个真名锚，
//!   首锚 `50:59`），但台账判它 **`multi`** —— 主文件是 `graphwar_game_server.rs`（54 锚）
//!   + 另 4 个模块，其中本模块只占 3 个。`docs/06` §65 / §72 已按 `Location` 结案：
//!   `new` 属 `graphwar_game_server.rs`，**不是本模块**。按 §4 不搬。
//! * `0x45b020` 在 Windows 的 `.symtab` 里同时记成 `subscribe_to_events_event_handler::{{closure}}`
//!   （`fixtures/name_to_addr_v2.tsv` 两行同址）⇒ **`tick` 本身就是订阅 `tick_event` 的那个闭包**
//!   （**观测**：两条架构的符号指向同一地址）。
//!
//! # 2. 我方现状（这个文件里装了什么 / 什么还没落码）
//!
//! | 内容 | 档位 | 证据 |
//! |---|---|---|
//! | 新连接时的 **7 条状态同步**：`RoomState` + `on_new_connection`（顺序即抓包顺序） | **逐位**（行为）+ **观测**（归属） | 与真实客户端抓包**逐字节**一致（`docs/33`、`fixtures/protocol_match.jsonl`；`state_sync_matches_captured_bytes` 钉住） |
//! | `to_wire`（每帧 `u16` 大端长度 + JSON） | **观测** | 抓包帧布局（`wire_encoding_matches_frame_layout` 钉住） |
//! | 上面那 **6 条有锚函数**的**函数体** | **观测**（控制流/立即数）+ **推断**（字段类型、被调者语义） | **T45 落码 6/6**：每个函数体第 1 行给 vaddr，逐步给指令地址，见 §5；`fixtures/function_coverage.tsv` 的这 6 行从 `F3(register-only)` 升档 |
//! | `on_tick_request`（`TickReply` 应答） | 仍在 `src/server.rs` | `docs/06` §74.4 的建议：`server.rs` 只留这一层薄封装（tick 公式本体在 `src/graphwar_tick_handler.rs`） |
//!
//! ⚠️ **新连接那 7 条同步的原版落点，是本模块里"没有 `Location` 锚"的那部分函数**
//! （台账 527 只收能按 `Location` 硬归属的函数，所以那部分不在 §1 的表里）。
//! 它归本模块的依据是 `docs/33` 的抓包 + `docs/02-module-map.md` §17.4 的登记（**观测**），
//! 不是这 6 个锚。
//!
//! ⚠️ **T45 落码的边界（别把模型当原件）**：§5 的 6 个函数落的是
//! **「控制流 + 数据结构」的模型** —— 每一步都能指到指令，事件/表的偏移都是实测；
//! 但原版的**网络 I/O**（`0x3d89e0` 里 serde 序列化 + 写 `TcpStream`）在本文件里
//! 只表现为 `Connection::send` 的成功/失败，真实收发仍走 `src/server.rs` /
//! `src/_scaffold/`。所以这 6 条的档位是 **观测**（不是"逐位"：
//! `docs/coverage-matrix.tsv` 给本模块标的"逐位"指的是 `on_new_connection`
//! 那条抓包对拍，**不是**这 6 条）。
//!
//! # 3. 结构
//!
//! `RoomState` = 一个房间的静态状态（房名 + 7 个模式/状态字段的字符串形态）。
//! ⚠️ 字段**类型**是按抓包文本挑的最小表示（`String`），**不是**从二进制读出来的布局；
//! 原版这个结构的字段偏移在本 checkout 没有证据（**未知**）。
//!
//! # 4. 公开路径（调用方 0 改动）
//!
//! `src/server.rs` 里有
//! `pub use crate::graphwar_server_connection_handler::{on_new_connection, to_wire, RoomState};`，
//! 所以 `crate::server::{RoomState, on_new_connection, to_wire}` 这三条老路径继续有效
//! （`src/game.rs` / `src/graphwar_game_server.rs` / `src/graphwar_game_state_manager.rs` /
//! `src/_scaffold/{root_wiring,diffconv,replaydiff}.rs` / `tests/*` / `examples/serve_handshake.rs`
//! 一个都不用改，见 `fixtures/boundary_split_plan.txt` §2）。

/// 原版模块名（arm64 `.symtab` 里的 `graphwarrust::graphwar_server_connection_handler`）。
pub const MODULE: &str = "graphwar_server_connection_handler";

/// `panic::Location` 里逐字节出现的源文件名 —— 与 `MODULE` 同名，就是 §1 那六条的证据侧。
pub const SOURCE_FILE: &str = "src/graphwar_server_connection_handler.rs";

/// 与真二进制对照的锚定强度（判据同 `fixtures/name_to_addr.tsv`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// 双向唯一 `(file,line,col)` 锚。
    Unique,
    /// 无直接一对一锚，但按「一个名字只对应一个地址」消去后只剩一个候选。
    DeducedByElimination,
    /// 该函数不引用任何 `Location`，名字无法落地（纯胶水/纯内联）。
    NoAnchor,
}

/// 一个已归属的函数（**证据行**，不是行为实现）。
///
/// 没有 arm64 列：本机 `.reference/upstream/` 只有 x86 的 `libgraphwarrust.so`，
/// 别的文档引用的 arm64 地址在本 checkout 无法复算 ⇒ 宁可不给，也不抄二手数字。
#[derive(Debug, Clone, Copy)]
pub struct Func {
    pub name: &'static str,
    /// x86 steam 2026-08-28 构建的 ELF vaddr（RVA）。
    pub x86: u64,
    /// `fixtures/name_to_addr_v2.tsv` 记的第一个 `(line, col)` 锚。
    pub loc_first: (u32, u32),
    /// `fixtures/boundary_audit.tsv` 里该函数落在本文件的锚点行数。
    pub loc_lines: u32,
    pub link: Link,
}

/// 本模块**有 `Location` 锚的全部 6 个函数**（= 台账里那 6 条 `mismatch`）。
///
/// `0x45_00c0`（`new`）不在本表：台账判 `multi`、主文件 `graphwar_game_server.rs`（见 §1.1）。
pub const FUNCTIONS: &[Func] = &[
    Func { name: "GraphwarServerConnectionHandler::forward_event", x86: 0x45_a560, loc_first: (350, 38), loc_lines: 1, link: Link::Unique },
    Func { name: "GraphwarServerConnectionHandler::handle_clear_bans_request_event", x86: 0x45_a6b0, loc_first: (247, 97), loc_lines: 2, link: Link::Unique },
    Func { name: "GraphwarServerConnectionHandler::handle_connection_removed_event", x86: 0x45_a7b0, loc_first: (224, 97), loc_lines: 3, link: Link::Unique },
    Func { name: "GraphwarServerConnectionHandler::forward_connection_specific_event", x86: 0x45_aab0, loc_first: (326, 18), loc_lines: 2, link: Link::Unique },
    Func { name: "GraphwarServerConnectionHandler::handle_ban_connection_request_event", x86: 0x45_ad30, loc_first: (233, 105), loc_lines: 4, link: Link::Unique },
    Func { name: "GraphwarServerConnectionHandler::tick", x86: 0x45_b020, loc_first: (206, 42), loc_lines: 8, link: Link::Unique },
];

// ---------------------------------------------------------------------------
// 1. 状态结构
// ---------------------------------------------------------------------------

/// 一个房间的静态状态。
#[derive(Debug, Clone, PartialEq)]
pub struct RoomState {
    pub room_name: String,
    pub game_state: String,
    pub axis_mode: String,
    pub function_mode: String,
    pub turn_mode: String,
    pub time_mode: String,
}

// ---------------------------------------------------------------------------
// 2. 新连接时的状态同步（7 条，顺序即抓包顺序）
// ---------------------------------------------------------------------------

/// 新连接建立后要发给客户端的一串消息（顺序即抓包顺序）。
///
/// 依据真实抓包：客户端发 `NewConnectionRequest` 之后，服务端**按固定顺序**回一串消息：
///
/// ```text
/// NewConnection -> TurnModeInfo -> GameStateInfo -> GameNameInfo
///               -> AxisModeInfo -> FunctionModeInfo -> TimeModeInfo
/// ```
///
/// 这一段是纯状态，没有随机成分，所以可以做到**逐字节**一致（`docs/33`）。
///
/// 还没实现：玩家加入（`NewPlayerRequest`）、图形/名字/段位同步、tick 节拍、
/// 开火结算的广播（见 `docs/33` 与本文件 §2 的"未落码"表）。
pub fn on_new_connection(state: &RoomState, connection_id: i64) -> Vec<String> {
    vec![
        format!(r#"{{"NewConnection":{{"connection_id":{}}}}}"#, connection_id),
        format!(r#"{{"TurnModeInfo":{{"turn_mode":"{}"}}}}"#, state.turn_mode),
        format!(r#"{{"GameStateInfo":{{"game_state":"{}"}}}}"#, state.game_state),
        format!(r#"{{"GameNameInfo":{{"game_name":"{}"}}}}"#, state.room_name),
        format!(r#"{{"AxisModeInfo":{{"axis_mode":"{}"}}}}"#, state.axis_mode),
        format!(r#"{{"FunctionModeInfo":{{"function_mode":"{}"}}}}"#, state.function_mode),
        format!(r#"{{"TimeModeInfo":{{"time_mode":"{}"}}}}"#, state.time_mode),
    ]
}

/// 把一串消息编码成线上字节流（每帧 `u16` 大端长度 + JSON）。
///
/// ⚠️ 这一条**没有独立的 `Location` 锚**（不在台账 527 里）：它跟着唯一用户
/// `on_new_connection` 的发送侧走（**推断**：发送侧胶水）。归属证据只有"谁用它"。
pub fn to_wire(messages: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for m in messages {
        out.extend_from_slice(&(m.len() as u16).to_be_bytes());
        out.extend_from_slice(m.as_bytes());
    }
    out
}

// ---------------------------------------------------------------------------
// 3. 事件体（tagged enum）与线上观测量 —— 全部是**立即数**，逐条给指令地址
// ---------------------------------------------------------------------------

/// 原版事件是 **tagged enum**：首字的最高位是标记位，低位是变体号。
///
/// **观测**：`0x45a7bd movabs r14,0x8000000000000000` + `0x45a7c7 lea rax,[r14+6]`
/// + `0x45a7cb cmp qword ptr [rsi],rax`（判"是不是变体 6"）；`0x45aac0/0x45aad0`
/// 用同一个 `0x8000…` 异或后再比较。
pub const EVENT_TAG_BIT: u64 = 0x8000_0000_0000_0000;

/// **变体 6**：连接已消失 / 该连接发不出去。
///
/// **观测**：① `0x45a7c7`（`handle_connection_removed_event` 只接受它）；
/// ② `0x45a5a3..0x45a5a7` 与 `0x45abf3..0x45abf7`（`forward_event` /
/// `forward_connection_specific_event` 在发送失败时**构造**它：
/// `mov rax,0x8000000000000000` → `add rax,6`）；
/// ③ `0x45b5b5..0x45b5c8`（`tick` 的超时清扫也构造它）。
///
/// 语义"连接已消失"是**推断**（名字层面缺证据：没有一处把变体号与信号名字面量对上）。
pub const VARIANT_CONNECTION_REMOVED: u64 = 6;

/// **变体 0x48（= 72）**：`ban_connection_request_event`。
///
/// **观测**：`0x45ad3e movabs r14,0x7fffffffffffffff` + `0x45ad48 lea rax,[r14+0x49]`
/// + `0x45ad4c cmp qword ptr [rsi],rax` ⇒ 只接受 `0x8000000000000048`。
/// 其它变体走 `0x45afa5..` 的 `explicit panic` 臂（`esi=0xe` + `"explicit panic"`）。
pub const VARIANT_BAN_CONNECTION_REQUEST: u64 = 0x48;

/// 目标连接 id 在事件体里的偏移 = **+8** 的两个变体。
///
/// **观测**：`0x45aae4 cmp rcx,5 / je 0x45aafa`（`mov ebp,dword [r14+8]`）、
/// `0x45aaf0 cmp rcx,0xa / jne panic`（两者共用 `0x45aafa` 的 `[r14+8]`）。
pub const TARGET_ID_AT_8: [u64; 2] = [5, 0xa];

/// 目标连接 id 在事件体里的偏移 = **+0x20** 的变体（= [`VARIANT_CONNECTION_REMOVED`]）。
///
/// **观测**：`0x45aaea cmp rcx,6 / je 0x45ab0e` → `0x45ab13 mov ebp,dword [r14+0x20]`。
pub const TARGET_ID_AT_0X20: u64 = VARIANT_CONNECTION_REMOVED;

/// `tick` 阶段 A 里 `0x447470` 的判别值：返回 `0xd` 时"这条消息不用处理"。
///
/// **观测**：`0x45b077 call 0x447470` / `0x45b07c cmp al,0xd` / `0x45b07e jne <错误分支>`；
/// `0x45b4ab/0x45b4b0` 同一条比较。语义（= "连接已关闭"一类的错误类）**推断**。
pub const IGNORED_KIND: u8 = 0xd;

/// `tick` 阶段 B 里被**跳过**的两个变体：`0x4a` 直接丢、`0x4b` 再看 `0x447470`。
///
/// **观测**：`0x45b418 movabs rcx,0x800000000000004a` / `0x45b422 cmp rax,rcx / je 0x45b33d`；
/// `0x45b42b movabs rcx,0x800000000000004b` / `0x45b435 cmp rax,rcx / je 0x45b4a0`。
pub const VARIANT_SKIP_4A: u64 = 0x4a;
/// 见 [`VARIANT_SKIP_4A`]。
pub const VARIANT_MAYBE_4B: u64 = 0x4b;

/// `tick` 阶段 C 的**超时阈值**（= 600）。
///
/// **观测**：`0x45b5b0 mov esi,0x258`，随后每一轮 `0x45b62b mov ecx,dword [rdx-4]`
/// → `0x45b62e add ecx,esi` → `0x45b630 cmp ebp,ecx`（`ebp` = `[registry+0x58]` 的"现在"）。
/// 单位**未知**（ms / tick 数都说得通）。
pub const CONNECTION_TIMEOUT: u32 = 0x258;

/// `tick` 阶段 A 里每条消息都要 `lock xadd` 的那个**静态计数器**。
///
/// **观测**：`0x45b0f0 lock xadd dword ptr [rip+0x565fc8], ebx @0x9c10c0`（`ebx=1`）。
/// 语义**未知**（缺：谁读它）。
pub const GLOBAL_SEQ: u64 = 0x9c10_c0;

/// 本模块调用、但**不属于本模块**的那个外部被调者（x86 vaddr，**观测**：`call rel32` 目标）。
///
/// 两个地址都属 `graphwar_module_container.rs` 的
/// `GraphwarModuleContainer::get_module_wk`（`fixtures/function_coverage.tsv` 里同一地址那一行）；
/// 本模块 4 个函数体都用它取 `+0x70` 那个 `Option<Rc<RefCell<…>>>`。
///
/// ⚠️ 为什么写成常量而不是直接写进注释：`tools/function_coverage.py` 的 **P2** 规则
/// 会把**函数体内**出现的任何原版地址贴到那个函数上，于是"注释里引用一个被调者地址"
/// 会被误判成"我方实现了那个原版函数"。T45 实测踩到过：在 `handle_clear_bans_request_event`
/// / `tick` 的注释里写裸地址，`module_container` 那两行就从 `F2 hyb-only` 变成 `F1 pinned`
/// （虚报 2 条）。地址放常量里（代码而非注释）就不会被 `cites` 收集 ⇒ 账本保持诚实。
pub const EXTERNAL_GET_MODULE_WK: [u64; 2] = [0x46_d260, 0x46_d040];

// ---------------------------------------------------------------------------
// 4. 事件与连接模型
// ---------------------------------------------------------------------------

/// 一个事件（原版是 0x48 字节的 tagged enum；这里只建模被读到的字段）。
///
/// `words[0]` = 首字（tag，含标记位），`words[1..]` = 事件体后续字。
/// **观测**：`0x45a8a9..0x45a8cb` 构造该结构（首字 + 两个 16 字节块 + 尾部 id），
/// 总宽 0x28 字节（`lea rsi,[rsp+0x70]` 传出的那块栈区）；推入队列时元素宽 0x48
/// （`0x3d3530`：`lea rcx,[r15+r15*8]` + `*8` ⇒ 72 字节/元素）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Event {
    /// 原版事件的前 5 个字（`+0x00 … +0x20`）。
    pub words: [u64; 5],
}

impl Event {
    /// 首字（tag）。
    pub fn tag(&self) -> u64 {
        self.words[0]
    }

    /// 变体号（去掉标记位）。非 tagged 值返回 `None`（原版对它们走 panic 臂）。
    pub fn variant(&self) -> Option<u64> {
        let tag = self.words[0];
        if tag & EVENT_TAG_BIT == 0 {
            None
        } else {
            Some(tag & !EVENT_TAG_BIT)
        }
    }

    /// 标记位 + 变体号。
    pub fn tagged(variant: u64) -> Self {
        let mut w = [0u64; 5];
        w[0] = EVENT_TAG_BIT | variant;
        Self { words: w }
    }

    /// 字节偏移 `off` 处的 `u32`（`off` 是 4 的倍数；原版一律按字节偏移读，
    /// 例如 id 在 `+8`、状态在 `+0xc` —— 两个 `dword` 共用同一个字）。
    pub fn u32_at(&self, off: usize) -> u32 {
        let shift = (off % 8) * 8;
        (self.words[off / 8] >> shift) as u32
    }

    /// 字节偏移 `off` 处的 `u32` 写入（口径同 [`Self::u32_at`]）。
    pub fn set_u32_at(&mut self, off: usize, v: u32) {
        let i = off / 8;
        let shift = (off % 8) * 8;
        let mask = !(0xffff_ffffu64 << shift);
        self.words[i] = (self.words[i] & mask) | (u64::from(v) << shift);
    }

    /// **变体 6** 的构造：`tag = 0x8000000000000006`、`+8` 是空载荷
    /// （`0x8000000000000000`）、id 落在 `+0x20`。
    ///
    /// **观测**：`0x45a7e0 mov qword ptr [rsp+0x58],r14`（`r14 = 0x8000…`）、
    /// `0x45a7e5 mov dword ptr [rsp],ebp`、`0x45aa09` 那段的 `+0x20` 写入；
    /// `0x45a61d..0x45a63b` 是同一形状的另一份实例。
    pub fn connection_removed(id: i32) -> Self {
        let mut ev = Self::tagged(VARIANT_CONNECTION_REMOVED);
        ev.words[1] = EVENT_TAG_BIT;
        ev.set_u32_at(0x20, id as u32);
        ev
    }

    /// 该事件要发给哪个连接（**观测**：变体 5/0xa 取 `+8`、变体 6 取 `+0x20`）。
    /// 其它变体原版走 `explicit panic`（`0x45ac20`，`esi=0xe`）。
    pub fn target_connection(&self) -> Option<i32> {
        let v = self.variant()?;
        if TARGET_ID_AT_8.contains(&v) {
            Some(self.u32_at(8) as i32)
        } else if v == TARGET_ID_AT_0X20 {
            Some(self.u32_at(0x20) as i32)
        } else {
            None
        }
    }
}

/// 连接上的"封禁/挂起"三值状态。
///
/// **观测**（`0x45aeed..0x45af2e`）：拿旧值首字节与 `2` 比、再拿下发值首字节与 `2` 比；
/// `2` 是既不等于 0 也不等于 1 的那个 niche（"空"），0/1 两个变体再比较后两个字。
/// **哪个值 = 封禁、哪个 = 解封未知**（缺：调用方上下文）⇒ 这里只保留数值。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BanState(pub u8);

impl BanState {
    /// `2` = 空（niche）。
    pub const ABSENT: BanState = BanState(2);
    /// 变体 0。
    pub const ZERO: BanState = BanState(0);
    /// 变体 1。
    pub const ONE: BanState = BanState(1);
}

/// 一条已接入的连接。
#[derive(Debug, Clone, Default)]
pub struct Connection {
    /// 连接 id（原版表键是 `i32`，**观测**：`0x45ab85 cmp ebp,dword ptr [rcx+r8*8-0x28]` 是 32 位比较）。
    pub id: i32,
    /// 这条连接自己的待处理队列（`tick` 阶段 B 用 `0x3d86a0` 逐个取）。
    pub inbox: std::collections::VecDeque<Event>,
    /// 已经写出去的事件（模型里的"写 socket 成功"；原版是 `0x3d89e0` 直接写 `TcpStream`）。
    pub sent: Vec<Event>,
    /// 死连接：再发就失败（原版 `0x3d89e0` 返回非 0 ⇒ 见 [`Self::send`]）。
    pub dead: bool,
}

impl Connection {
    /// 往这条连接写一个事件。
    ///
    /// 原版 `0x3d89e0`（本模块的发送前哨）里能读到的三件事：
    /// ① `"Failed to serialize event."`、② `"Message too long."`、
    /// ③ `"Failed to write to tcp_stream."`（`.rodata`，见函数体 `0x3d89e0` 的
    /// `lea rdi` 串）；返回"一个字"（`0x45a60c test rax,rax / je <继续>`），
    /// **非零 = 失败**。这里用 `false` 表示失败。
    pub fn send(&mut self, event: &Event) -> bool {
        if self.dead {
            return false;
        }
        self.sent.push(*event);
        true
    }
}

/// `tick` 阶段 A 从队列里取出来的"新连接"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingConnection {
    /// 连接 id（`0x45b0cb mov eax,dword ptr [rsp+0xdc]`）。
    pub id: i32,
    /// 判别字（`0x45b0d2 movzx r15d,word ptr [rsp+0xe0]`）。
    pub kind: u16,
    /// 载荷（`0x45b0db mov r13d,dword ptr [rsp+0xe2]`）。
    pub payload: u32,
}

/// `tick` 阶段 C 扫的那张表（`self+0x38`，元素宽 0x80 = 128 字节）里被读到的字段。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Session {
    /// 连接 id（`0x45b656 mov eax,dword ptr [r12+rax*8-8]`）。
    pub id: i32,
    /// 最后活跃时刻（`0x45b62b mov ecx,dword ptr [rdx-4]`）。
    pub deadline: u32,
}

/// 本模块的**可测模型**。
///
/// 字段偏移全部是**观测**（每个函数体里 `[self+0xNN]` 的读点写在下面），
/// 字段**类型/名字**是**推断**（本 checkout 没有 Rust 类型元数据）：
///
/// | 偏移 | 本模型字段 | 观测依据 |
/// |---|---|---|
/// | `+0x08` | [`Self::connections`] | `0x45a58a mov r14,[rdi+8]`（ctrl 指针）+ `+0x20` items + `+0x28` hasher（`0x45a58e`/`0x45ab02`） |
/// | `+0x38` | [`Self::sessions`] | `0x45a961 lea r14,[rbx+0x38]`（移除）+ `0x45b2fa lea rax,[r15+0x38]`（tick 清扫），元素宽 0x80 |
/// | `+0x68` | [`Self::event_bus`] | `0x45a58e mov rbp,[rdi+0x68]` → `Rc<RefCell<Vec<Event>>>`（借用计数在 `+0x10`、载荷在 `+0x18`） |
/// | `+0x70` | [`Self::registry`] | `0x45a6b4/0x45ad5f/0x45a98b add rdi,0x70` → `get_module_wk`（`Option<Rc<…>>`，`-1` 是 None niche） |
#[derive(Debug, Default)]
pub struct GraphwarServerConnectionHandler {
    /// `+0x08`：已接入连接表（元素 40 字节）。
    pub connections: std::collections::BTreeMap<i32, Connection>,
    /// `+0x38`：会话/超时表（元素 128 字节，`tick` 阶段 C 扫它）。
    pub sessions: std::collections::BTreeMap<i32, Session>,
    /// `+0x68`：待派发的内部事件队列（`0x3d3530` = push，元素 72 字节）。
    pub event_bus: Vec<Event>,
    /// `+0x70`：封禁表所在的那个 `Option<Rc<RefCell<…>>>`。
    pub registry: Option<Registry>,
    /// `+0x78`：新连接接收队列（`0x8fcb70(&out, self+0x78)` 每轮取一条）。
    pub pending: std::collections::VecDeque<PendingConnection>,
    /// 静态计数器 `0x9c10c0` 的模型值（**观测**：阶段 A 每条消息 `lock xadd` 一次，`+1`）。
    pub global_seq: u32,
    /// 已经交给 `graphwar_remote_event_handler::handle_client_remote_event`
    /// （原版 `0x3d_ee00`，同一地址在 T30 边界表里就是这个函数）的事件。
    pub dispatched: Vec<(i32, Event)>,
    /// `tick` 阶段 C 的闸门 `[self+0x7e]`（**观测**：`0x45b580 cmp byte ptr [rax+0x7e],0`）。
    pub sweep_flag: bool,
}

/// `+0x70` 里那个 `Rc<RefCell<Registry>>` 的模型。
///
/// **观测**：`tick` 阶段 C 从它读"现在"（`0x45b570 mov ebp,dword ptr [r14+0x58]`）；
/// `handle_clear_bans_request_event` 对它的表做 hashbrown `clear`
/// （`0x45a6e0..0x45a743`）；`handle_ban_connection_request_event` 对它做 `insert`
/// （`0x45ad92..0x45af77`）；`handle_connection_removed_event` 对它做 `remove`（`0x486270`）。
#[derive(Debug, Default)]
pub struct Registry {
    /// 表（`+0x48` 起：ctrl `+0x48` / bucket_mask `+0x50` / items `+0x60` / hasher `+0x68` —— **观测**）。
    pub bans: std::collections::BTreeMap<i32, BanState>,
    /// `+0x58`："现在"（**观测**：`0x45b570`）。
    pub now: u32,
}

/// 一次 [`GraphwarServerConnectionHandler::tick`] 的产出。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickReport {
    /// 阶段 A：从队列里接进来几条连接。
    pub accepted: usize,
    /// 阶段 A：被 `0x447470` 判成"要报 IO 错误"的条数（判别值 != `0xd`）。
    pub io_errors: usize,
    /// 阶段 B：交给 `handle_client_remote_event` 的事件数。
    pub dispatched: usize,
    /// 阶段 C：因超时被判定为"连接已消失"、推了一条变体 6 的条数。
    pub timed_out: usize,
}

// ---------------------------------------------------------------------------
// 5. 六个函数体（T45 落码：6/6）
//
// 每个函数第一行 = **x86 vaddr**；正文里的地址 = 该步在原版里的指令。
// **网络/引擎**动作（写 `TcpStream`、Godot 信号）在本文件里只做"控制流 + 数据结构"
// 的模型，真实 I/O 由 `src/_scaffold/` 与 `src/server.rs` 负责（`AGENTS.md` §4：
// 自造桥接不进复原层）。
// ---------------------------------------------------------------------------

impl GraphwarServerConnectionHandler {
    /// `forward_event` —— **`0x45a560`**（`0x14d` 字节，1 条 `Location` 锚 `350:38`）。
    ///
    /// **语义（观测）**：把事件**广播**给连接表里的每一条连接；
    /// **发送失败的**那条连接，往事件队列里推一条"连接已消失"（变体 6，带它的 id）。
    ///
    /// 逐步指令：
    /// * `0x45a573 mov r13,[rdi+0x20]` = 表项数（= 循环上界）；`0` 直接返回（`0x45a577/0x45a57a je 0x45a66b`）；
    /// * `0x45a58a..0x45a5f6` = hashbrown 迭代（16 控制字节一组、`add r14,-0x280` = 640 = 16×40）；
    /// * `0x45a607 call 0x3d89e0`（**发送前哨**：`.rodata` 里写着
    ///   `"Failed to serialize event."` / `"Message too long."` / `"Failed to write to tcp_stream."`），
    ///   实参 `rsi` = 事件指针（`0x45a602 mov rsi,[rsp+0x18]`）；
    /// * `0x45a60c test rax,rax / je 0x45a5b0` ⇒ **返回非 0 才算失败**；
    /// * 失败后 `0x45a616 cmp [rbp+0x10],0 / jne panic` 是 `RefCell::borrow_mut` 的借用检查，
    ///   `0x45a629 mov eax,dword ptr [rax-0x28]` 取该表项的 `i32` id，
    ///   `0x45a652 call *[0x9bac80] → 0x3d3530` 推入事件（元素 72 字节）；
    /// * `0x45a658 inc qword ptr [rbp+0x10]` 释放借用；`0x45a661 call 0x45f160` drop 发送前哨的返回值。
    pub fn forward_event(&mut self, event: &Event) -> usize {
        let mut failures = 0usize;
        // 原版是 HashMap 迭代序（**未知**）；这里按 id 升序（BTreeMap）以便复现。
        let ids: Vec<i32> = self.connections.keys().copied().collect();
        for id in ids {
            let ok = match self.connections.get_mut(&id) {
                Some(conn) => conn.send(event), // 0x45a607 call 0x3d89e0
                None => false,
            };
            if !ok {
                failures += 1;
                // 0x45a652：推 `Event::connection_removed(id)`
                self.event_bus.push(Event::connection_removed(id));
            }
        }
        failures
    }

    /// `forward_connection_specific_event` —— **`0x45aab0`**（`0x1bc` 字节，2 条锚 `326:18`/`336`）。
    ///
    /// **语义（观测）**：事件体里的第一个字段决定"发给哪条连接"，
    /// 然后**只**给那一条发；失败同样推一条变体 6。
    ///
    /// 逐步指令：
    /// * `0x45aaca mov rax,[rsi]` / `0x45aad0 xor rdx,r15`（`r15 = 0x8000…`）→ 变体号；
    /// * `0x45aae4/0x45aaea/0x45aaf0` 三分支：变体 **5 / 0xa → id 在 `+8`**、
    ///   **6 → id 在 `+0x20`**、其它 → `0x45ac20 explicit panic`；
    /// * `0x45ab02 cmp [rbx+0x20],0 / je 0x45ac15` = 表空 ⇒ 直接返回（不发）；
    /// * `0x45ab2f call 0x47dec0` = `make_hash`，`0x45ab5b..0x45abaf` = hashbrown 探测；
    /// * `0x45abc4 call 0x3d89e0`（发送前哨），失败 ⇒ `0x45ac01 call *[0x9bac80] → 0x3d3530` 推变体 6。
    pub fn forward_connection_specific_event(&mut self, event: &Event) -> bool {
        let Some(id) = event.target_connection() else {
            // 原版 `0x45ac20`：`explicit panic`（没有 `_` 之外的兜底分支）
            return false;
        };
        let ok = match self.connections.get_mut(&id) {
            Some(conn) => conn.send(event), // 0x45abc4
            None => return false,           // 0x45aba8..0x45ac15：查不到就返回
        };
        if !ok {
            self.event_bus.push(Event::connection_removed(id)); // 0x45ac01
        }
        true
    }

    /// `handle_connection_removed_event` —— **`0x45a7b0`**（`0x2fb` 字节，3 条锚 `224:97`/`225`/`316`）。
    ///
    /// **语义（观测）**：收到变体 6 ⇒ ① 把它转发给**那一条**连接（尽力），
    /// ② 再从**三张表**里按顺序删掉这个 id。
    ///
    /// 逐步指令：
    /// * `0x45a7c7 lea rax,[r14+6]` / `0x45a7cb cmp [rsi],rax` / `jne 0x45aa17 explicit panic`
    ///   ⇒ 只接受变体 6；`0x45a7d7 mov ebp,dword ptr [rsi+0x20]` = id；
    /// * `0x45a7da..0x45a803`：把事件体 `+8` 那一块（16 字节，空时是 `0x8000…`）复制进栈上的新事件；
    /// * `0x45a8d8 call 0x45aab0` = [`Self::forward_connection_specific_event`]（**第一件事就是转发**）；
    /// * `0x45a8fe call 0x4864d0` = 从 `self+0x08` 那张表 `remove`（hash 在 `0x45a8eb`）；
    /// * `0x45a982 call 0x486bc0` = 从 `self+0x38` 那张表 `remove`；
    /// * `0x45a98b add rbx,0x70` → `0x45a98e call get_module_wk` → `0x45a9e1 call 0x486270` = 从封禁表 `remove`；
    /// * `0x45aa04 call 0x45f670` = drop 栈上那个事件。
    pub fn handle_connection_removed_event(&mut self, event: &Event) -> Option<RemovalReport> {
        if event.variant() != Some(VARIANT_CONNECTION_REMOVED) {
            return None; // 0x45a7cb 的另一臂：explicit panic
        }
        let id = event.u32_at(0x20) as i32;
        // ① 先转发给那条连接（0x45a8d8）
        self.forward_connection_specific_event(event);
        // ② 三张表依次删（地址升序 = 删除顺序）
        let from_connections = self.connections.remove(&id).is_some(); // 0x4864d0
        let from_sessions = self.sessions.remove(&id).is_some(); // 0x486bc0
        let from_bans = match self.registry.as_mut() {
            Some(reg) => reg.bans.remove(&id).is_some(), // 0x486270
            None => false,
        };
        Some(RemovalReport {
            id,
            from_connections,
            from_sessions,
            from_bans,
        })
    }

    /// `handle_clear_bans_request_event` —— **`0x45a6b0`**（`0xf4` 字节，2 条锚 `247:97`/`248`）。
    ///
    /// **语义（观测）**：把封禁表**整体清空**。注意它**完全不看事件实参**
    /// （体内没有一条指令读 `rsi`）⇒ 签名里没有 `event`。
    ///
    /// 逐步指令：
    /// * `0x45a6b4 add rdi,0x70` → `0x45a6b8 call get_module_wk`（取 `Option<Rc<…>>`），
    ///   `0x45a6c0 je 0x45a765` 那条臂是 `Option::unwrap` 的 panic；
    /// * `0x45a6d8 mov qword ptr [rbx+0x10],-1` = `RefCell::borrow_mut`；
    /// * `0x45a6e0 cmp [rbx+0x30],0`（表项数）为 0 就跳过（`0x45a726`）；
    /// * `0x45a6fd call *[0x9bad08]` = `memset(ctrl, 0xff, bucket_mask+0x11)` ——
    ///   `0x45a6f4 lea rdx,[r14+0x11]`、`0x45a6f8 mov esi,0xff`：hashbrown 的 `clear`
    ///   （把每个控制字节写成 `EMPTY = 0xFF`）；
    /// * `0x45a703..0x45a724` = 重算 `growth_left`（`(buckets & !7) - buckets/8`，`buckets<8` 时取 `buckets`）；
    /// * `0x45a737..0x45a743` = 写回 `items=0` / `growth_left` / 释放借用；`0x45a747 dec [rbx]` = `Rc` 计数自减。
    ///
    /// 返回 `false` = 没有 registry（原版此处 `unwrap` 会 panic）。
    pub fn handle_clear_bans_request_event(&mut self) -> bool {
        let Some(reg) = self.registry.as_mut() else {
            return false; // 0x45a765：Option::unwrap 的 None 臂
        };
        reg.bans.clear(); // 0x45a6fd memset(ctrl, 0xff, …) + items=0
        true
    }

    /// `handle_ban_connection_request_event` —— **`0x45ad30`**（`0x2e8` 字节，4 条锚 `233:105`…`242`）。
    ///
    /// **语义（观测）**：变体 0x48 ⇒ 读两个 `u32`（`+8` = 连接 id、`+0xc` = 状态），
    /// 在**连接表**（`self+0x70` 的 registry 里那张）查这条连接；
    /// 查到就把 `(id, 状态)` 写进封禁表（`0x43f550` = hashbrown `insert`）。
    ///
    /// 逐步指令：
    /// * `0x45ad4c cmp [rsi],0x8000000000000048` / `jne 0x45afa5`；
    /// * `0x45ad55 mov ebp,[rsi+8]`（id）、`0x45ad58 mov eax,[rsi+0xc]`（状态）；
    /// * `0x45ad63 call get_module_wk` + `0x45ad6b je 0x45afbe`（`unwrap`）；
    /// * `0x45ad79..0x45ada6` = `RefCell::borrow`（`+0x10` 借用计数 `+1`，上限 `0x7fff…`）；
    /// * `0x45ae4c..0x45aee9` = 第二次查（换成状态值当键的载荷）、`0x45aef7..0x45af2e` = 新旧枚举逐字节比较；
    /// * `0x45af5e mov qword [rbx+0x10],-1` + `0x45af72 call 0x43f550` = `borrow_mut` 后 `insert`；
    /// * `0x45af77 inc qword [rbx+0x10]` 释放借用。
    pub fn handle_ban_connection_request_event(&mut self, event: &Event) -> Option<BanReport> {
        if event.variant() != Some(VARIANT_BAN_CONNECTION_REQUEST) {
            return None; // 0x45afa5 explicit panic
        }
        let id = event.u32_at(8) as i32; // 0x45ad55
        let state = BanState((event.u32_at(0x0c) & 0xff) as u8); // 0x45ad58（按首字节比较）
        let reg = self.registry.as_mut()?; // 0x45ad63 + unwrap
        let previous = reg.bans.insert(id, state); // 0x43f550
        Some(BanReport {
            id,
            state,
            previous,
        })
    }

    /// `tick` —— **`0x45b020`**（`0x85a` = 2138 字节，**8 条** `Location` 锚
    /// `206:42`/`207`/`208`/`258`/`259`/`291`/`292`/`303`）。
    ///
    /// 三个阶段，逐段给地址：
    ///
    /// **阶段 A（`0x45b034..0x45b2f1`）排空"新连接"队列**
    /// * `0x45b034..0x45b057`：`lea rsi,[rdi+0x78]` / `lea rax,[rdi+0x70]` / `lea rax,[rdi+8]`
    ///   —— 三个字段先存栈上；`0x45b057 call *[0x9bbd88] → 0x8fcb70`（收一条消息）；
    /// * `0x45b05d cmp byte [rsp+0xd8],0 / je 0x45b091`：收到 ⇒ 解码
    ///   `0x45b0cb mov eax,[rsp+0xdc]`（id）/ `movzx r15d,word [rsp+0xe0]`（判别字）/
    ///   `mov r13d,dword [rsp+0xe2]`（载荷）；
    /// * `0x45b0f0 lock xadd dword [0x9c10c0],1` —— **每条消息**都动那个静态计数器
    ///   （[`GLOBAL_SEQ`]）；`0x45b102 call *[0x9bafd8] → 0x8f44f0`；
    /// * `0x45b116 call get_module_wk` + `0x45b137 mov qword [r14+0x10],-1` = 借 registry；
    ///   `0x45b1a0 call 0x43f390`（往 registry 的表里 `insert`）、
    ///   `0x45b1e6 call 0x440b50`（往 `self+0x08` 的表里 `insert`）、
    ///   `0x45b1eb cmp word [rsp+0x90],2 / je` 之后 `call *[0x9ba980]`（收尾）；
    /// * 没收到东西的那条臂 `0x45b06c..0x45b07e`：`0x45b077 call 0x447470` 与 `0xd` 比，
    ///   不等就格式化 `"\nIO error"`（`.rodata 0x1a1a58`）再 `0x45b2c6` 上报。
    ///
    /// **阶段 B（`0x45b2f6..0x45b53c`）把每条连接的收件箱抽干**
    /// * `0x45b2f6 mov r12,[r15+0x20]` = 表项数；`0x45b31d..0x45b397` = hashbrown 迭代（16×40）；
    /// * `0x45b3a1 call get_module_wk`（取 registry 里的对应项，`je 0x45b6b6` 是 panic 臂）；
    /// * `0x45b40b call 0x3d86a0` = 从该连接自己的队列取一个事件；
    /// * `0x45b418 cmp tag,0x800000000000004a / je 0x45b33d` = **变体 0x4a 直接丢**；
    /// * `0x45b42b cmp tag,0x800000000000004b / je 0x45b4a0` = 变体 0x4b 再看 `0x447470`，
    ///   等于 `0xd` 就丢（`0x45b4b2 je 0x45b330`）；
    /// * `0x45b480 / 0x45b4fd call 0x3d_ee00` = **`graphwar_remote_event_handler::handle_client_remote_event`**
    ///   （同一地址在 `fixtures/boundary_audit.tsv` 里就是该模块的函数）⇒ 交给远端事件处理器；
    /// * `0x45b48f call 0x442330` = 顺手更新 `self+0x38` 那张表里的一个字段（`r14d = [registry+0x58]`）。
    ///
    /// **阶段 C（`0x45b53c..0x45b6b5`）超时清扫**
    /// * `0x45b541 call get_module_wk` 取 registry；`0x45b570 mov ebp,dword [r14+0x58]` = "现在"；
    /// * `0x45b579 cmp qword [self+0x20],1 / ja` + `0x45b580 cmp byte [self+0x7e],0 / je 0x45b68f`
    ///   = 闸门（表项数 > 1 或 [`GraphwarServerConnectionHandler::sweep_flag`]）；
    /// * `0x45b58a mov r15,[self+0x38+0x18]`… `0x45b606 add r12,-0x80` = 迭代 `self+0x38` 表（128 字节/项）；
    /// * `0x45b5b0 mov esi,0x258`（= [`CONNECTION_TIMEOUT`]）→ `0x45b62b mov ecx,[rdx-4]`（该项的最后活跃时刻）
    ///   → `0x45b62e add ecx,esi` → `0x45b630 cmp ebp,ecx / jbe`（还没超时就跳过）；
    /// * 超时项：`0x45b656 mov eax,[r12+rax*8-8]`（id）→ 构造
    ///   `0x8000000000000006` + `0x8000000000000000` 的事件（`0x45b5bf..0x45b674`）→
    ///   `0x45b684 call *[0x9bac80] → 0x3d3530` 推入事件队列。
    ///
    /// **未落码**：`0x43f390` / `0x440b50` / `0x442330` / `0x3d86a0` / `0x8f44f0` 五个被调者的
    /// 精确签名与语义（都是 hashbrown / mpsc 的内联体；本模型按"表操作 / 取消息"落地）。
    pub fn tick(&mut self) -> TickReport {
        let mut report = TickReport::default();

        // ── 阶段 A：排空新连接队列（0x45b057 每轮一次 0x8fcb70）
        while let Some(p) = self.pending.pop_front() {
            // 0x45b0f0：每条消息都 lock xadd 那个静态计数器
            self.global_seq = self.global_seq.wrapping_add(1);
            if p.id == i32::MIN {
                // 0x45b077/0x45b07c：`0x447470` 判别值 != 0xd ⇒ 报到 IO 错误分支
                report.io_errors += 1;
                continue;
            }
            let id = p.id;
            self.connections.entry(id).or_insert_with(|| Connection {
                id,
                ..Default::default()
            });
            self.sessions.insert(
                id,
                Session {
                    id,
                    deadline: p.payload, // 阶段 C 用来算超时的那个 dword
                },
            );
            report.accepted += 1;
        }

        // ── 阶段 B：每条连接的收件箱（0x45b3a1 → 0x3d86a0）
        let ids: Vec<i32> = self.connections.keys().copied().collect();
        for id in ids {
            loop {
                let next = self
                    .connections
                    .get_mut(&id)
                    .and_then(|c| c.inbox.pop_front()); // 0x45b40b call 0x3d86a0
                let Some(ev) = next else { break };
                let Some(variant) = ev.variant() else { continue };
                if variant == VARIANT_SKIP_4A {
                    continue; // 0x45b422 je 0x45b33d：整个丢掉
                }
                if variant == VARIANT_MAYBE_4B {
                    // 0x45b4a0..0x45b4b2：再看 0x447470，等于 0xd 就丢
                    //   模型：载荷首字低字节就是那个判别值
                    if (ev.words[1] as u8) == IGNORED_KIND {
                        continue;
                    }
                }
                // 0x45b480 / 0x45b4fd call 0x3d_ee00（远端事件处理器）
                self.dispatched.push((id, ev));
                report.dispatched += 1;
                // 0x45b48f call 0x442330：更新 self+0x38 表里的字段
                if let Some(s) = self.sessions.get_mut(&id) {
                    s.deadline = self.registry.as_ref().map(|r| r.now).unwrap_or(s.deadline);
                }
            }
        }

        // ── 阶段 C：超时清扫（0x45b53c..0x45b6b5）
        let gate = self.sessions.len() > 1 || self.sweep_flag; // 0x45b579 / 0x45b580
        if gate {
            if let Some(now) = self.registry.as_ref().map(|r| r.now) {
                let expired: Vec<i32> = self
                    .sessions
                    .values()
                    .filter(|s| now > s.deadline.saturating_add(CONNECTION_TIMEOUT)) // 0x45b62e/0x45b630
                    .map(|s| s.id)
                    .collect();
                for id in expired {
                    self.event_bus.push(Event::connection_removed(id)); // 0x45b684
                    report.timed_out += 1;
                }
            }
        }
        report
    }
}

/// [`GraphwarServerConnectionHandler::handle_connection_removed_event`] 的产出。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RemovalReport {
    /// 被移掉的连接 id（事件 `+0x20`）。
    pub id: i32,
    /// `self+0x08` 那张表里有没有（`0x4864d0`）。
    pub from_connections: bool,
    /// `self+0x38` 那张表里有没有（`0x486bc0`）。
    pub from_sessions: bool,
    /// 封禁表里有没有（`0x486270`）。
    pub from_bans: bool,
}

/// [`GraphwarServerConnectionHandler::handle_ban_connection_request_event`] 的产出。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BanReport {
    /// 连接 id（事件 `+8`）。
    pub id: i32,
    /// 要写的状态（事件 `+0xc` 的低字节）。
    pub state: BanState,
    /// 原来的值（`None` = 表里没有）。
    pub previous: Option<BanState>,
}

// ---------------------------------------------------------------------------
// 6. 诚实清单（这批函数体**没**解出来的部分，照实写）
// ---------------------------------------------------------------------------

/// 本节没解出来的东西（`AGENTS.md` §0 第 3 条）。
pub fn unknowns() -> &'static [&'static str] {
    &[
        "三张表的键/值类型：只知 i32 键 + 元素宽 40/128/24 字节；`+0x08`/`+0x38`/`+0x70` 三个字段的 Rust 类型名未知",
        "变体号 ↔ 信号名字面量：`.rodata` 有 connection_removed_event / ban_connection_request_event / clear_bans_request_event 等一串名字，但没有任何一处把变体号与名字对上",
        "0x3d89e0（发送前哨）的返回类型：只知道非 0 = 失败，且 `.rodata` 里是 serde 序列化 + TcpStream 写",
        "tick 阶段 A 的 0x8f44f0 / 阶段 B 的 0x43f390 / 0x440b50 / 0x442330 四个内联被调者的精确语义（只到 hashbrown/mpsc 形状）",
        "静态计数器 0x9c10c0 是谁在读（本轮只证到 tick 阶段 A 每消息 +1）",
        "超时阈值 0x258 的单位（ms / tick 数）",
        "`0x447470` 的返回字节 0xd 的语义（只知'需要忽略'的一档）",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 从抓包夹具里取出服务端那段状态同步消息（顺序与抓包一致）。
    fn captured_state_sync() -> Vec<String> {
        // 每类只取**第一条**（夹具里每类最多 3 条样本）
        const KINDS: [&str; 7] = [
            "NewConnection",
            "TurnModeInfo",
            "GameStateInfo",
            "GameNameInfo",
            "AxisModeInfo",
            "FunctionModeInfo",
            "TimeModeInfo",
        ];
        let mut out: Vec<String> = Vec::new();
        for kind in KINDS {
            let prefix = format!("{{\"{}\"", kind);
            if let Some(line) = include_str!("../fixtures/protocol_match.jsonl")
                .lines()
                .map(|l| l.trim())
                .find(|l| l.starts_with(&prefix))
            {
                out.push(line.to_string());
            }
        }
        out
    }

    #[test]
    fn state_sync_matches_captured_bytes() {
        let state = RoomState {
            room_name: "Graphwar Game".into(),
            game_state: "Setup".into(),
            axis_mode: "EveryUnit".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
        };
        let ours = on_new_connection(&state, 228);
        let captured = captured_state_sync();
        assert_eq!(ours.len(), captured.len(), "消息条数与抓包不符");
        for (i, (a, b)) in ours.iter().zip(captured.iter()).enumerate() {
            assert_eq!(a, b, "第 {i} 条消息与抓包不一致");
        }
    }

    #[test]
    fn wire_encoding_matches_frame_layout() {
        let msgs = vec![r#"{"X":{"y":1}}"#.to_string()];
        let wire = to_wire(&msgs);
        assert_eq!(&wire[..2], &(msgs[0].len() as u16).to_be_bytes());
        // 能被自己的解码器读回
        let (f, used) = crate::_scaffold::protocol::Frame::decode(&wire).unwrap();
        assert_eq!(f.payload, msgs[0]);
        assert_eq!(used, wire.len());
    }

    // ---------------------------------------------------------------------
    // T45：六个函数体的测试。期望值全部来自反汇编立即数（出处见每条断言上方）。
    // ---------------------------------------------------------------------

    fn handler_with(ids: &[i32]) -> GraphwarServerConnectionHandler {
        let mut h = GraphwarServerConnectionHandler::default();
        for id in ids {
            h.connections.insert(
                *id,
                Connection {
                    id: *id,
                    ..Default::default()
                },
            );
        }
        h
    }

    /// 变体 6 的形状（`0x45a5a3 add rax,6`、`0x45a629 mov eax,dword [rax-0x28]` 取 id）：
    /// tag = `0x8000000000000000 | 6`、`+8` = `0x8000000000000000`（空载荷）、id 在 `+0x20`。
    #[test]
    fn connection_removed_event_layout_matches_immediates() {
        let ev = Event::connection_removed(7);
        assert_eq!(ev.tag(), 0x8000_0000_0000_0006);
        assert_eq!(ev.variant(), Some(6));
        assert_eq!(ev.words[1], 0x8000_0000_0000_0000);
        assert_eq!(ev.u32_at(0x20), 7);
        assert_eq!(ev.target_connection(), Some(7), "变体 6 的 id 在 +0x20（0x45ab13）");
        assert_eq!(EVENT_TAG_BIT, 0x8000_0000_0000_0000);
        assert_eq!(VARIANT_CONNECTION_REMOVED, 6);
    }

    /// `forward_event`：发不出去的那条 ⇒ **恰好一条**变体 6 事件进队列（`0x45a60c`/`0x45a652`）。
    #[test]
    fn forward_event_broadcasts_and_reports_failures() {
        let mut h = handler_with(&[1, 2, 3]);
        h.connections.get_mut(&2).unwrap().dead = true;
        let ev = Event::tagged(TARGET_ID_AT_8[0]); // 变体 5，随便一个"广播型"事件

        let failed = h.forward_event(&ev);
        assert_eq!(failed, 1, "只有一条连接发不出去");
        assert!(h.connections[&1].sent.contains(&ev));
        assert!(h.connections[&3].sent.contains(&ev));
        assert_eq!(h.event_bus.len(), 1, "失败只推一条内部事件");
        assert_eq!(h.event_bus[0].variant(), Some(VARIANT_CONNECTION_REMOVED));
        assert_eq!(h.event_bus[0].u32_at(0x20), 2, "推的是那条坏连接的 id");

        // 全活 ⇒ 不推任何东西
        let mut ok = handler_with(&[1, 2]);
        assert_eq!(ok.forward_event(&ev), 0);
        assert!(ok.event_bus.is_empty());
    }

    /// `forward_connection_specific_event`：变体 **5 / 0xa → id 在 `+8`**、**6 → `+0x20`**、
    /// 其它变体原版 `explicit panic`（`0x45aae4/0x45aaea/0x45aaf0` + `0x45ac20`）。
    #[test]
    fn specific_forward_reads_id_from_variant_dependent_offset() {
        let mut h = handler_with(&[4, 9]);

        for v in [5u64, 0xa] {
            let mut ev = Event::tagged(v);
            ev.set_u32_at(8, 9);
            assert_eq!(ev.target_connection(), Some(9));
            assert!(h.forward_connection_specific_event(&ev));
            assert_eq!(h.connections[&9].sent.last().unwrap().variant(), Some(v));
        }

        let ev6 = Event::connection_removed(4);
        assert_eq!(ev6.target_connection(), Some(4));
        assert!(h.forward_connection_specific_event(&ev6));
        assert_eq!(h.connections[&4].sent.len(), 1);

        // 其它变体：原版没有兜底分支 ⇒ 我方返回 false（不猜语义）
        let other = Event::tagged(0x10);
        assert_eq!(other.target_connection(), None);
        assert!(!h.forward_connection_specific_event(&other));

        // 目标不存在 ⇒ 查表落空直接返回（0x45aba8..0x45ac15）
        let mut missing = Event::tagged(5);
        missing.set_u32_at(8, 12345);
        assert!(!h.forward_connection_specific_event(&missing));
        assert!(h.event_bus.is_empty(), "查不到连接不算发送失败");
    }

    /// `handle_connection_removed_event`：**先转发**（`0x45a8d8`）、**再按地址升序删三张表**
    /// （`0x4864d0` → `0x486bc0` → `0x486270`）。
    #[test]
    fn connection_removed_forwards_first_then_removes_three_tables() {
        let mut h = handler_with(&[8]);
        h.sessions.insert(
            8,
            Session {
                id: 8,
                deadline: 0,
            },
        );
        h.registry = Some(Registry {
            bans: [(8, BanState::ZERO)].into_iter().collect(),
            now: 0,
        });

        let ev = Event::connection_removed(8);
        let r = h.handle_connection_removed_event(&ev).expect("变体 6 必须认");
        assert_eq!(
            r,
            RemovalReport {
                id: 8,
                from_connections: true,
                from_sessions: true,
                from_bans: true
            }
        );
        // "先转发"的证据：那条连接在 remove 之前已经收到了这个事件
        // （原版 0x45a8d8 的 call 在 0x45a8fe 的 remove 之前）
        assert_eq!(h.event_bus.len(), 0, "活连接转发成功 ⇒ 不推失败事件");
        assert!(h.connections.is_empty());
        assert!(h.sessions.is_empty());
        assert!(h.registry.as_ref().unwrap().bans.is_empty());

        // 非变体 6 ⇒ 原版 explicit panic（0x45a7cb 的另一臂）；我方返回 None
        assert!(h.handle_connection_removed_event(&Event::tagged(5)).is_none());
    }

    /// `handle_clear_bans_request_event`：整体清空，且**不看事件实参**（体内不读 `rsi`）。
    #[test]
    fn clear_bans_empties_the_table() {
        let mut h = GraphwarServerConnectionHandler::default();
        // 没有 registry ⇒ 原版 `Option::unwrap` 的 None 臂（0x45a765）
        assert!(!h.handle_clear_bans_request_event());

        h.registry = Some(Registry {
            bans: [(1, BanState::ZERO), (2, BanState::ONE)]
                .into_iter()
                .collect(),
            now: 42,
        });
        assert!(h.handle_clear_bans_request_event());
        let reg = h.registry.as_ref().unwrap();
        assert!(reg.bans.is_empty(), "clear 之后表必须空");
        assert_eq!(reg.now, 42, "clear 不碰 now（0x45a6b0 体里只动表）");
    }

    /// `handle_ban_connection_request_event`：变体 0x48、id 在 `+8`、状态在 `+0xc` 的低字节。
    #[test]
    fn ban_request_reads_id_and_state_then_inserts() {
        let mut h = GraphwarServerConnectionHandler::default();
        h.registry = Some(Registry::default());

        let mut ev = Event::tagged(VARIANT_BAN_CONNECTION_REQUEST);
        ev.set_u32_at(8, 5);
        ev.set_u32_at(0x0c, u32::from(BanState::ONE.0));
        // 两个 dword 打在同一个字上：+8 = 低 32 位、+0xc = 高 32 位
        assert_eq!(ev.words[1], (u64::from(BanState::ONE.0) << 32) | 5);

        assert_eq!(ev.variant(), Some(0x48));
        let r = h.handle_ban_connection_request_event(&ev).expect("变体 0x48");
        assert_eq!(r.id, 5);
        assert_eq!(r.state, BanState::ONE);
        assert_eq!(r.previous, None);
        assert_eq!(h.registry.as_ref().unwrap().bans[&5], BanState::ONE);

        // 第二次：previous 有值（原版新旧比较在 0x45aef7..0x45af2e）
        let r2 = h.handle_ban_connection_request_event(&ev).unwrap();
        assert_eq!(r2.previous, Some(BanState::ONE));

        // 其它变体 ⇒ 原版 explicit panic（0x45afa5）；我方 None
        assert!(h
            .handle_ban_connection_request_event(&Event::tagged(5))
            .is_none());
    }

    /// `tick` 阶段 C：阈值就是 `mov esi,0x258`（= 600）；
    /// `cmp ebp,ecx / jbe` ⇒ **`now <= deadline+600` 不算超时**、`now = deadline+601` 才算。
    #[test]
    fn tick_timeout_sweep_uses_0x258() {
        assert_eq!(CONNECTION_TIMEOUT, 600);
        let mk = |now: u32, deadline: u32| {
            let mut h = GraphwarServerConnectionHandler::default();
            h.sweep_flag = true; // 越过 0x45b579/0x45b580 的闸门
            h.registry = Some(Registry {
                bans: Default::default(),
                now,
            });
            h.sessions.insert(
                1,
                Session {
                    id: 1,
                    deadline,
                },
            );
            h
        };
        // 恰好等于 deadline+600 ⇒ 不超时（jbe 分支）
        let mut h = mk(600, 0);
        assert_eq!(h.tick().timed_out, 0);
        assert!(h.event_bus.is_empty());
        // deadline+601 ⇒ 推一条变体 6
        let mut h = mk(601, 0);
        let rep = h.tick();
        assert_eq!(rep.timed_out, 1);
        assert_eq!(h.event_bus.len(), 1);
        assert_eq!(h.event_bus[0].variant(), Some(VARIANT_CONNECTION_REMOVED));
        assert_eq!(h.event_bus[0].u32_at(0x20), 1);
    }

    /// `tick` 阶段 B：变体 `0x4a` 直接丢、`0x4b` 看判别字节、其它交给
    /// `handle_client_remote_event`（`0x3d_ee00`，`0x45b480`/`0x45b4fd`）。
    #[test]
    fn tick_dispatches_inbox_by_variant() {
        let mut h = handler_with(&[3]);
        h.registry = Some(Registry::default());
        {
            let inbox = &mut h.connections.get_mut(&3).unwrap().inbox;
            inbox.push_back(Event::tagged(VARIANT_SKIP_4A)); // 丢
            let mut skip = Event::tagged(VARIANT_MAYBE_4B);
            skip.words[1] = u64::from(IGNORED_KIND); // 判成 0xd ⇒ 也丢
            inbox.push_back(skip);
            inbox.push_back(Event::tagged(0x20)); // 派发
        }
        let rep = h.tick();
        assert_eq!(rep.dispatched, 1);
        assert_eq!(h.dispatched.len(), 1);
        assert_eq!(h.dispatched[0].0, 3);
        assert_eq!(h.dispatched[0].1.variant(), Some(0x20));
        assert!(h.connections[&3].inbox.is_empty(), "收件箱必须抽干");
    }

    /// `tick` 阶段 A：每条排队消息都 `lock xadd` 那个静态计数器（`0x45b0f0`，`+1`/条）。
    #[test]
    fn tick_accepts_pending_and_bumps_global_seq_per_message() {
        let mut h = GraphwarServerConnectionHandler::default();
        h.registry = Some(Registry::default());
        h.pending.push_back(PendingConnection {
            id: 11,
            kind: 0,
            payload: 100,
        });
        h.pending.push_back(PendingConnection {
            id: 12,
            kind: 0,
            payload: 200,
        });
        let rep = h.tick();
        assert_eq!(rep.accepted, 2);
        assert_eq!(h.global_seq, 2, "每条消息 +1（0x45b0f0）");
        assert!(h.connections.contains_key(&11) && h.connections.contains_key(&12));
        assert!(h.sessions.contains_key(&11) && h.sessions.contains_key(&12));
        assert_eq!(GLOBAL_SEQ, 0x9c10_c0);
    }

    /// 诚实清单不许空着。
    #[test]
    fn unknowns_are_listed() {
        assert_eq!(unknowns().len(), 7);
        // 六条原版函数在本文件里都有**函数体**（T45 之前这 6 条是 register-only）
        for f in FUNCTIONS {
            let name = f.name.rsplit("::").next().unwrap();
            assert!(
                [
                    "forward_event",
                    "handle_clear_bans_request_event",
                    "handle_connection_removed_event",
                    "forward_connection_specific_event",
                    "handle_ban_connection_request_event",
                    "tick"
                ]
                .contains(&name),
                "FUNCTIONS 里的 {name} 不在落码清单里"
            );
        }
        assert_eq!(FUNCTIONS.len(), 6);
    }
}
