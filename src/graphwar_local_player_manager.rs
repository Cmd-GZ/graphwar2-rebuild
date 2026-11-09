//! 本机玩家：`GraphwarLocalPlayerManager`（"哪个 `player_id` 是我的" + 一次性的 5 条元信息请求）。
//!
//! 本模块回答三件事：**"本机玩家"到底由谁说了算**（§4）、**`rank_request_event`(66) 的
//! 发起侧是不是这里**（§5，是）、**`function_info_event` 的上行是不是也从这里发**
//! （§5，**不是**）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（4 个）
//!
//! | 函数 | x86 vaddr / 字节 | arm64 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | 已内联（无独立符号） | 已内联（无独立符号） | — | 订阅 3 条事件 | 推断（3 个闭包 + 形状） |
//! | `handle_connected_to_server` | `0x51a570` / 160 | `0x411be8` / 180 | 76, 81 | 记下自己的连接 id + 请求建玩家 + 清表 | 逐位 |
//! | `handle_player_info` | `0x519f40` / 1584 | `0x4115d0` / 1560 | 107–128 | 按连接 id 过滤 + 首次入列时发 5 条请求 | 逐位 |
//! | `handle_entity_removed_event` | `0x51a610` / `0xe0` | `0x411c9c` / 176 | 146 | `Vec::retain` 掉该 id | 逐位 |
//!
//! 三个 `subscribe` 闭包在 arm64 上是独立符号（`0x411d4c` / `0x411d7c` / `0x411dac`，各 48 B），
//! 每个都用 `adr x1, <handler>` 把方法地址交给 `WeakCaller::call`
//! ⇒ 三条订阅与三个处理方法的对应是**逐位**的（§2）。
//! 唯独 `new` 本身在两个架构上都**没有独立符号**（被内联进构造者），
//! 所以"`new` 里字段怎么初始化"只有形状证据 ⇒ **推断**。
//!
//! ⚠️ **纠 `docs/112` §9.3**：`0x51a610` 不是"只有一条 `panic_fmt` 出口的空处理"。
//! x86 与 arm64 **两边都是** `Vec::retain`（把 `player_ids` 里等于该 id 的全部删掉），
//! 见 §3 第三段。
//!
//! ## 2. 三条订阅（逐位）
//!
//! | 事件名 | 判别值 | 处理函数 | 证据 |
//! |---|---|---|---|
//! | `connected_to_server_event` | 7 | `handle_connected_to_server` | 逐位（闭包 `0x411d4c` + 方法内 `cmp` 7） |
//! | `player_info_event` | 13 | `handle_player_info` | 逐位（闭包 `0x411d7c` + 方法内 `cmp` 13） |
//! | `entity_removed_event` | 14 | `handle_entity_removed_event` | 逐位（闭包 `0x411dac` + 方法内 `cmp` 14） |
//!
//! ## 3. 数据形状（逐位）
//!
//! ```text
//! +0x00 cap   +0x08 ptr   +0x10 len      ← Vec<u32>：**我的** player_id 列（同一条连接可以有多条）
//! +0x18 u32   "已经收到 connected_to_server"（只有 1 与非 1 的判定）
//! +0x1c u32   我的 connection_id
//! +0x28 Rc    事件通道（+0x10 borrow 标志、+0x18 emit 入口）
//! +0x30       模块容器（`get_module_wk` 的 self）
//! +0x38 Rc<RefCell<本机玩家配置>>：+0x10 strong/borrow、配置从 +0x18 起
//!             配置里的四个 String 依次在 +0x18 / +0x30 / +0x48 / +0x60
//!             （名字 / 皮肤 / 脸 / 帽子；每处 24 字节，字段序 cap/ptr/len）
//! ```
//!
//! `Vec<u32>` 的判据：`retain` 循环里 `str w13, [x9, x8, lsl #2]`（元素 **4** 字节、
//! 写回索引 `x8`）+ `ldr w13, [x12], #0x4`；`len` 在 `+0x10`（`str x9, [x20, #0x10]`）。
//!
//! ## 4. `handle_connected_to_server`（逐位）+ "本机玩家"的单一真相
//!
//! ```text
//! assert tag == 7                       ; movk 得到 0x8000_0000_0000_0007
//! let conn = event.u32(+0x08)
//! self.connected     = 1                ; stp w9(1), w8, [x0, #0x18]
//! self.connection_id = conn             ; 同一个 stp 的第二个字
//! emit(NewPlayerRequest { conn })       ; tag 8，载荷 +0x08 = conn
//! self.player_ids.len = 0               ; 最后一句 str xzr, [x19, #0x10]
//! ```
//!
//! 语义（**推断**，缺的是 ws 上"服务端收到 8 之后干什么"那一侧）：
//! 服务端告诉我"你连上了，连接号 = N"，于是本机（1）记住 N、（2）请求为这个连接
//! 建一个玩家实体、（3）把旧的 id 列表清空。
//!
//! ⇒ **"本机玩家"的单一真相就在这两个字段**：`+0x18`（有没有连上）与 `+0x1c`（我的连接号），
//! 再加上 `+0x00/+0x08/+0x10` 那个 `Vec<u32>`（我名下的所有 `player_id`）。
//! `handle_player_info` 是唯一往这个 `Vec` 里写的地方，而它**只接受**
//! `payload.connection_id == self.connection_id` 的那些 `player_info`。
//!
//! ## 5. `handle_player_info` 与那 5 条请求（逐位）
//!
//! ```text
//! assert tag == 13
//! if self.connected != 1                      { return }   ; +0x18
//! if event.u32(+0x0c) != self.connection_id   { return }   ; +0x1c
//! let pid = event.u32(+0x08)
//! let first = self.player_ids.is_empty()
//! self.player_ids.push(pid)                                ; grow_one + str w26,[..,x19,lsl#2]
//! if !first { return }
//! ;; 以下是"我第一次知道自己的 player_id"时的一次性 5 连发
//! emit(59 NameRequest          { String(名字),  pid })
//! emit(50 SkinGraphicsRequest  { String(皮肤),  pid })
//! emit(52 FaceGraphicsRequest  { String(脸),    pid })
//! emit(54 HatGraphicsRequest   { String(帽子),  pid })
//! emit(66 RankRequest          { 1, level,      pid })
//! ```
//!
//! 五条判别值的取法（arm64 侧同一个寄存器复用，**这是 `docs/112` §9.2 没看出来的**）：
//! `x25 = 0x8000_0000_0000_000d`（tag 13 本身），随后四条的 tag 都是
//! `add x8, x25, #imm` ⇒ `13 + imm`：
//!
//! | 指令 | `imm` | tag | 事件 |
//! |---|---|---|---|
//! | `0x4117fc` | `0x2e` | 59 | `name_request_event` |
//! | `0x4118b4` | `0x25` | 50 | `skin_graphics_request_event` |
//! | `0x41196c` | `0x27` | 52 | `face_graphics_request_event` |
//! | `0x411a24` | `0x29` | 54 | `hat_graphics_request_event` |
//! | `0x411a80` | `0x35` | 66 | `rank_request_event` |
//!
//! 前四条载荷 = `{String @+0x00, u32 player_id @+0x18}`（与 `NameInfo` 同形，共 0x20 字节），
//! 那个 `String` 是**本机玩家配置里的对应字段**（§3 的 `+0x18/+0x30/+0x48/+0x60`，
//! 代码里先 `alloc(len)+memcpy` 复制一份）。
//!
//! 第五条**不是字符串**：`stp w9(1), w8, [sp,#0x10]` + `str w26, [sp,#0x18]`
//! ⇒ `{+0x08 = 1, +0x0c = level, +0x10 = player_id}`，与 `rank_manager` 的
//! `RankInfo`(67) **完全同形**。`level` 来自
//! `bl 0x392f18 <GraphwarCampaignControl::get_current_level>`（返回 `Option<u32>`：
//! `w0` 判别 + `w1` 值），随后 `tst w0,#1` / `csel w8, w1, wzr, ne`
//! ⇒ **`level = 有当前关就用它，否则 0`**。
//!
//! ## 6. `rank_request_event`(66) 与 `function_info_event`(35)：一个"是"、一个"不是"
//!
//! * **66 的发起侧就是本模块**（逐位：tag `13+0x35`，见 §5），且全 `.text` 里
//!   没有第二处构造 66 的代码 ⇒ `rank_manager` §5 那条"发起侧在
//!   `local_player_manager`"的**推断，现在升级为逐位**。
//! * **35 不是本模块发的**。arm64 全量扫 tag 35 只有两处：
//!   `function_manager::set_function` 与 `function_history_recorder::handle_function_fire_event`。
//!   本模块 3 个函数里没有任何 35 的构造 ⇒ `function_info_event` 的上行来自
//!   **`graphwar_function_manager`**（`docs/02` §15.2.1 的 `0x455e30`），
//!   与"本机玩家是谁"无关。
//!
//! 于是问题 (c) 的答案是**分层**的：本模块是"**我 = 哪些 entity id**"的单一真相，
//! 也是 5 条元信息请求的唯一发起处；但**它不管状态上行**（函数/开火/结束回合分别归
//! `function_manager` / `game_state_manager`）。
//!
//! ## 7. 未确定
//!
//! * `new` 的字段初始化顺序（两个架构上都内联掉了，只有 3 个闭包 + 形状证据）。
//!   任务 74 在 x86 侧补了 8 条写入点（§8），只到"哪个偏移写了什么"这一档。
//! * `NewPlayerRequest`(8) 的载荷为什么是**连接号**而不是别的（服务端那侧未读）。
//! * `+0x30` 那个 `Weak` 指向的**是谁**、以及它后面那张 120 字节 entry 的表是什么
//!   （§9 第五道门）。任务 74 只定到"这是一道门、且它在 5 连发之前"，语义**未知**。
//! * 配置里四个 `String` 的**字段名**（`player_config` 模块的 `Serialize` 才带名字，
//!   本模块只看到偏移）⇒ 名字是 `推断`，顺序（名字/皮肤/脸/帽子）来自四条请求的语义。
//!
//! ## 8. 64 B 布局：内联构造的 8 条写入点（任务 74，x86 `0x517690` 体内）
//!
//! ```text
//! 0x51792a alloc(size 0x58, align 8)      ← RcInner(16) + RefCell<M>(72)，M = 64
//!   rax = 盒基：+0x00 strong / +0x08 weak / +0x10 RefCell borrow 标志（0x517975/0x51798c/0x517984）
//!   M 基 = rax + 0x18 = RefCell 的 +0x08
//!     M+0x00 ← 0        0x517946    Vec 的 cap
//!     M+0x08 ← 4        0x51794e    Vec 的 ptr（`Vec::<u32>::new()` 的 dangling = align 4）
//!     M+0x10 ← 0        0x517956    Vec 的 len
//!     M+0x18 ← 0(dword) 0x51795e    connected（构造时只清这一格 4 字节）
//!     M+0x1c    （构造**无**写入点）connection_id 的首次写点在 0x51a593（`handle_connected_to_server`）
//!     M+0x20 ← rax      0x517965    **`Weak<RefCell<Self>>` 自句柄**（`Rc::new_cyclic_in`）
//!     M+0x28 ← r14      0x517969    构造实参 1（= 事件队列 `Rc<RefCell<…>>`，见 §9 的用法）
//!     M+0x30 ← r15      0x51796d    构造实参 2（一个 `Weak`，见 §7）
//!     M+0x38 ← r12      0x517971    构造实参 3
//! ```
//!
//! ⇒ `docs/161` §2 第 3 行那条"`+0x20` 那个 8 字节槽从没被本模块读过，语义未知"本轮结案：
//! **它就是自句柄**（写的值 = 盒基地址本身，写入点在 `new_cyclic` 的初始化序列里）。
//! 三个构造实参的**身份**仍是 `推断`（`+0x28` 那条有用法证据：`0x51a596`/`0x51a5a9`
//! 把它当事件队列用）。
//!
//! ## 9. 事件流接线（任务 74 就地复核，x86 逐条；`wired` 的第二件）
//!
//! ```text
//! 订阅 3 条（全在 0x517690 体内，rdi = 事件通道对象 rsp）
//!   0x5179bb lea rsi,'connected_to_server_event'(0x1c64c2) + 0x5179cc mov edx,0x19
//!   0x5179c2 lea r8,<0x962ba8> +0x20 → 0x51a730 → lea rsi,[0x51a570]
//!   0x5179d4 call [0x9bac88] → 0x3d38f0
//!   0x517a0e lea rsi,'player_info_event'(0x1c6558) + 0x517a1f mov edx,0x11
//!   0x517a15 lea r8,<0x962bd0> +0x20 → 0x51a710 → lea rsi,[0x519f40]
//!   0x517a27 call [0x9bac88]
//!   0x517a5e lea rsi,'entity_removed_event'(0x1c646d) + 0x517a6c mov edx,0x14
//!   0x517a65 lea r8,<0x962bf8> +0x20 → 0x51a6f0 → lea rsi,[0x51a610]
//!   0x517a77 call [0x9bac88]
//! 发 6 条（`rbp = isize::MAX`，tag 用 `lea rax,[rbp+imm]` / `add rbp,imm` 绕回来）
//!   handle_player_info 0x519f40（5 连发，全在一条直线上）：
//!     0x51a162 +0x3c ⇒ 59 (name)   @0x51a170 call [0x9bac80] → 0x3d3530
//!     0x51a239 +0x33 ⇒ 50 (skin)   @0x51a247
//!     0x51a310 +0x35 ⇒ 52 (face)   @0x51a31e
//!     0x51a3e7 +0x37 ⇒ 54 (hat)    @0x51a3f5
//!     0x51a443 add rbp,0x43 ⇒ 66   @0x51a451
//!   handle_connected_to_server 0x51a570（重入队）：
//!     0x51a577 movabs 0x8000…0007 ; 0x51a581 断言 tag ; 0x51a5b1 inc rax ⇒ 8
//!     @0x51a5bb call [0x9bac80] → 0x3d3530
//!   handle_entity_removed_event 0x51a610：`Vec::retain`，**体内零 `add_event`**
//! ```
//!
//! **门的逐条读法**（`docs/161` §2 第 3 行缺的那一条）——五道，前四道在 push 之前/当场：
//!
//! ```text
//! ① tag == 13                     0x519f58 lea rax,[rbp+0xe] ; 0x519f5c cmp
//! ② connected == 1                0x519f68 cmp dword [rdi+0x18],1 ; jne → 收尾
//! ③ connection_id 相等            0x519f6e/0x519f71（event+0x0c vs self+0x1c）
//! ④ 旧 len == 0（"第一次"）        0x519f7b..0x519f99（push）; 0x519f9d test rbx,rbx ; je → 发 5 连
//! ⑤ M+0x30 那个 Weak 升级后的表里   0x519fb1 call 0x46c5a0（升级）→ 0x51a02c..0x51a058 哈希查
//!    **还没有**该 player_id         0x51a058 je 0x51a0e0 ⇒ 已在表里就直接收尾（**不发** 5 连发）
//! ```
//!
//! ⑤ 是任务 74 **新发现**的一道门（`docs/161` 当时只写到"⑤ 的门没逐条读"）：
//! 命中即返回，所以"5 连发"不只看 `connected`/`first`，还看那张表的**首次插入**。
//! 五条请求**内部没有**逐条判据（`0x51a170`–`0x51a451` 是一条直线，中间没有再跳到收尾的 `jcc`）。
//!
//! ## 10. 复跑
//!
//! ```bash
//! ../.venv/bin/python tools/addr_probe.py dis 0x517690   # 3 条订阅 + 内联构造的 8 条写入点
//! ../.venv/bin/python tools/addr_probe.py dis 0x519f40   # 5 条请求 + 五个 tag + 五道门
//! ../.venv/bin/python tools/addr_probe.py dis 0x51a570   # connected → NewPlayerRequest(8)
//! ../.venv/bin/python tools/addr_probe.py dis 0x51a610   # 删一条（不发）
//! ../.venv/bin/python tools/addr_probe.py dis 0x51a730   # 跳板 → 0x51a570（另两条 0x51a710/0x51a6f0）
//! ```

use crate::events::Event;

/// `connected_to_server_event` 的判别值。
pub const EVENT_CONNECTED_TO_SERVER: u16 = 7;
/// `new_player_request_event` 的判别值（本模块**发**）。
pub const EVENT_NEW_PLAYER_REQUEST: u16 = 8;
/// `player_info_event` 的判别值。
pub const EVENT_PLAYER_INFO: u16 = 13;
/// `entity_removed_event` 的判别值。
pub const EVENT_ENTITY_REMOVED: u16 = 14;
/// `name_request_event` 的判别值（5 连发第 1 条）。
pub const EVENT_NAME_REQUEST: u16 = 59;
/// `skin_graphics_request_event` 的判别值（第 2 条）。
pub const EVENT_SKIN_GRAPHICS_REQUEST: u16 = 50;
/// `face_graphics_request_event` 的判别值（第 3 条）。
pub const EVENT_FACE_GRAPHICS_REQUEST: u16 = 52;
/// `hat_graphics_request_event` 的判别值（第 4 条）。
pub const EVENT_HAT_GRAPHICS_REQUEST: u16 = 54;
/// `rank_request_event` 的判别值（第 5 条）。
pub const EVENT_RANK_REQUEST: u16 = 66;

/// `Vec<u32>`（我的 player_id 列）在 `self` 里的起始偏移。
pub const PLAYER_IDS_OFFSET: usize = 0x00;
/// "已连上服务器"标志的偏移。
pub const CONNECTED_OFFSET: usize = 0x18;
/// 我的 `connection_id` 的偏移。
pub const CONNECTION_ID_OFFSET: usize = 0x1c;
/// 事件通道的偏移（`Rc`，emit 入口在 `+0x18`）。
pub const EVENT_CHANNEL_OFFSET: usize = 0x28;
/// 模块容器的偏移。
pub const MODULE_CONTAINER_OFFSET: usize = 0x30;
/// 本机玩家配置（`Rc<RefCell<_>>`）的偏移。
pub const PLAYER_CONFIG_OFFSET: usize = 0x38;
/// 配置里"名字"的偏移。
pub const CONFIG_NAME_OFFSET: usize = 0x18;
/// 配置里"皮肤"的偏移。
pub const CONFIG_SKIN_OFFSET: usize = 0x30;
/// 配置里"脸"的偏移。
pub const CONFIG_FACE_OFFSET: usize = 0x48;
/// 配置里"帽子"的偏移。
pub const CONFIG_HAT_OFFSET: usize = 0x60;

/// `player_info_event` 里 `player_id` 的偏移。
pub const PLAYER_INFO_PLAYER_OFFSET: usize = 0x08;
/// `player_info_event` 里 `connection_id` 的偏移。
pub const PLAYER_INFO_CONNECTION_OFFSET: usize = 0x0c;

/// "带字符串的请求"载荷里 `String` 的偏移。
pub const STRING_REQUEST_STRING_OFFSET: usize = 0x00;
/// "带字符串的请求"载荷里 `player_id` 的偏移。
pub const STRING_REQUEST_PLAYER_OFFSET: usize = 0x18;
/// "带字符串的请求"载荷字节数。
pub const STRING_REQUEST_PAYLOAD_SIZE: usize = 0x20;

/// `rank_request_event`(66) 载荷里那个恒定 `1` 的偏移。
pub const RANK_REQUEST_FLAG_OFFSET: usize = 0x08;
/// `rank_request_event`(66) 载荷里 `level` 的偏移。
pub const RANK_REQUEST_LEVEL_OFFSET: usize = 0x0c;
/// `rank_request_event`(66) 载荷里 `player_id` 的偏移。
pub const RANK_REQUEST_PLAYER_OFFSET: usize = 0x10;
/// 发送 66 时 `+0x08` 的恒定值（`stp w9(1), ...`）。
pub const RANK_REQUEST_FLAG: u32 = 1;

/// 一条订阅。
#[derive(Debug, Clone, Copy)]
pub struct Subscription {
    /// 事件名。
    pub name: &'static str,
    /// 判别值。
    pub tag: u16,
    /// 处理方法名。
    pub handler: &'static str,
}

/// `GraphwarLocalPlayerManager::new` 的三条订阅（逐位，见模块文档 §2）。
pub const SUBSCRIPTIONS: [Subscription; 3] = [
    Subscription {
        name: "connected_to_server_event",
        tag: EVENT_CONNECTED_TO_SERVER,
        handler: "handle_connected_to_server",
    },
    Subscription {
        name: "player_info_event",
        tag: EVENT_PLAYER_INFO,
        handler: "handle_player_info",
    },
    Subscription {
        name: "entity_removed_event",
        tag: EVENT_ENTITY_REMOVED,
        handler: "handle_entity_removed_event",
    },
];

/// 本机玩家配置（原版是 `Rc<RefCell<...>>`，四个 `String`，见模块文档 §3）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalPlayerProfile {
    /// 配置 `+0x18`：名字。
    pub name: String,
    /// 配置 `+0x30`：皮肤名。
    pub skin: String,
    /// 配置 `+0x48`：脸名。
    pub face: String,
    /// 配置 `+0x60`：帽子名。
    pub hat: String,
}

/// 那 5 条一次性请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataRequest {
    /// `name_request_event`(59)：`{String, u32}`。
    Name {
        /// 本机名字。
        name: String,
        /// 我名下的 player_id。
        player_id: u32,
    },
    /// `skin_graphics_request_event`(50)：`{String, u32}`。
    Skin {
        /// 本机皮肤名。
        skin: String,
        /// 我名下的 player_id。
        player_id: u32,
    },
    /// `face_graphics_request_event`(52)：`{String, u32}`。
    Face {
        /// 本机脸名。
        face: String,
        /// 我名下的 player_id。
        player_id: u32,
    },
    /// `hat_graphics_request_event`(54)：`{String, u32}`。
    Hat {
        /// 本机帽子名。
        hat: String,
        /// 我名下的 player_id。
        player_id: u32,
    },
    /// `rank_request_event`(66)：`{1, level, player_id}`（三个 dword）。
    Rank {
        /// `+0x08`：恒为 1。
        flag: u32,
        /// `+0x0c`：当前战役关（没有就是 0）。
        level: u32,
        /// `+0x10`：我名下的 player_id。
        player_id: u32,
    },
}

impl MetadataRequest {
    /// 该请求的事件判别值。
    pub const fn tag(&self) -> u16 {
        match self {
            Self::Name { .. } => EVENT_NAME_REQUEST,
            Self::Skin { .. } => EVENT_SKIN_GRAPHICS_REQUEST,
            Self::Face { .. } => EVENT_FACE_GRAPHICS_REQUEST,
            Self::Hat { .. } => EVENT_HAT_GRAPHICS_REQUEST,
            Self::Rank { .. } => EVENT_RANK_REQUEST,
        }
    }

    /// 该请求指向的 `player_id`。
    pub const fn player_id(&self) -> u32 {
        match self {
            Self::Name { player_id, .. }
            | Self::Skin { player_id, .. }
            | Self::Face { player_id, .. }
            | Self::Hat { player_id, .. }
            | Self::Rank { player_id, .. } => *player_id,
        }
    }

    /// 该请求的载荷字节数（前四条 0x20，第五条 12）。
    pub const fn payload_size(&self) -> usize {
        match self {
            Self::Rank { .. } => 12,
            _ => STRING_REQUEST_PAYLOAD_SIZE,
        }
    }

    /// 该请求在 `crate::events::Event` 里的变体（发件箱只记这个）。
    pub const fn event(&self) -> Event {
        match self {
            Self::Name { .. } => Event::NameRequest,
            Self::Skin { .. } => Event::SkinGraphicsRequest,
            Self::Face { .. } => Event::FaceGraphicsRequest,
            Self::Hat { .. } => Event::HatGraphicsRequest,
            Self::Rank { .. } => Event::RankRequest,
        }
    }
}

/// 本机玩家管理器。
///
/// # 事件流（任务 74 补，`wired` 的第二件；见模块文档 §9）
///
/// 订阅 3 条（全在 `0x517690` 体内）：`connected_to_server_event`@`0x5179d4`
/// → 跳板 `0x51a730` → `0x51a570`；`player_info_event`@`0x517a27` → `0x51a710` → `0x519f40`；
/// `entity_removed_event`@`0x517a77` → `0x51a6f0` → `0x51a610`。
/// 发 6 条：`handle_player_info` 里 59/50/52/54/66（`0x51a170`/`0x51a247`/`0x51a31e`/
/// `0x51a3f5`/`0x51a451`），`handle_connected_to_server` 里 8（`0x51a5bb` 重入队）。
/// `handle_entity_removed_event` 体内**零** `add_event`。
///
/// ⚠️ 原版还有第五道门（模块文档 §9 ⑤：`M+0x30` 那张表里已有该 `player_id` 就直接收尾），
/// **本实现没有复现它** —— 那张表的语义未知（§7），照抄等于把猜测写成行为。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphwarLocalPlayerManager {
    /// `Vec<u32>`（`+0x00/+0x08/+0x10`）：我名下的所有 `player_id`。
    pub player_ids: Vec<u32>,
    /// `+0x18`：是否已收到 `connected_to_server_event`（只有 1 / 非 1 两种用法）。
    pub connected: u32,
    /// `+0x1c`：我的 `connection_id`。
    pub connection_id: u32,
    /// `+0x38` 里那份配置。
    pub profile: LocalPlayerProfile,
    /// 订阅名（`new` 的三条 `subscribe`，顺序 = 调用序，`观测`）。
    pub subscribed: Vec<&'static str>,
    /// 发件箱（判别值；载荷见 [`MetadataRequest`] / [`EVENT_NEW_PLAYER_REQUEST`]）。
    pub outbox: Vec<Event>,
}

impl Default for GraphwarLocalPlayerManager {
    fn default() -> Self {
        Self::new(LocalPlayerProfile::default())
    }
}

impl GraphwarLocalPlayerManager {
    /// 新建 + 订阅 [`SUBSCRIPTIONS`] 那三条（原版 `new` 做的同一件事）。
    pub fn new(profile: LocalPlayerProfile) -> Self {
        Self {
            player_ids: Vec::new(),
            connected: 0,
            connection_id: 0,
            profile,
            subscribed: SUBSCRIPTIONS.iter().map(|s| s.name).collect(),
            outbox: Vec::new(),
        }
    }

    /// `handle_connected_to_server`：记下自己的连接号、请求建玩家、清空 id 列表。
    ///
    /// 注意三件事的**顺序**（逐位）：先写 `+0x18/+0x1c`，再发 (8)，最后清 `+0x10`。
    pub fn handle_connected_to_server(&mut self, connection_id: u32) -> u32 {
        self.connected = 1;
        self.connection_id = connection_id;
        // 原版此处 emit(NewPlayerRequest { connection_id })；返回它作为可断言的载荷。
        self.outbox.push(Event::NewPlayerRequest);
        self.player_ids.clear();
        connection_id
    }

    /// `handle_player_info`：过滤 → push → **首次入列**时给 5 条请求。
    ///
    /// `level` 对应 `campaign_control::get_current_level()`（`Some` 用它，`None` 用 0）。
    pub fn handle_player_info(
        &mut self,
        player_id: u32,
        connection_id: u32,
        level: Option<u32>,
    ) -> Vec<MetadataRequest> {
        if self.connected != 1 {
            return Vec::new();
        }
        if connection_id != self.connection_id {
            return Vec::new();
        }
        let first = self.player_ids.is_empty();
        self.player_ids.push(player_id);
        if !first {
            return Vec::new();
        }
        let requests = vec![
            MetadataRequest::Name { name: self.profile.name.clone(), player_id },
            MetadataRequest::Skin { skin: self.profile.skin.clone(), player_id },
            MetadataRequest::Face { face: self.profile.face.clone(), player_id },
            MetadataRequest::Hat { hat: self.profile.hat.clone(), player_id },
            MetadataRequest::Rank {
                flag: RANK_REQUEST_FLAG,
                level: level.unwrap_or(0),
                player_id,
            },
        ];
        self.outbox.extend(requests.iter().map(|r| r.event()));
        requests
    }

    /// `handle_entity_removed_event`：把该 id 从 [`Self::player_ids`] 里**全部**删掉。
    ///
    /// 逐位：`Vec::retain`（x86 `0x51a610`、arm64 `0x411c9c` 两边同形）。返回删了几个。
    /// `0x51a610` 体内**没有** `add_event` ⇒ 这条处理器不发事件。
    pub fn handle_entity_removed_event(&mut self, entity_id: u32) -> usize {
        let before = self.player_ids.len();
        self.player_ids.retain(|id| *id != entity_id);
        before - self.player_ids.len()
    }

    /// 取走发件箱（与原版 `GraphwarEventQueue` 的"取空"同口径）。
    pub fn take_outbox(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }

    /// 我名下第一个 `player_id`（没有就是 `None`）。
    pub fn my_player_id(&self) -> Option<u32> {
        self.player_ids.first().copied()
    }

    /// 是否已经知道"我是谁"。
    pub fn is_connected(&self) -> bool {
        self.connected == 1
    }
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    fn mgr() -> GraphwarLocalPlayerManager {
        GraphwarLocalPlayerManager::new(LocalPlayerProfile {
            name: "Ann".into(),
            skin: "skin_1".into(),
            face: "face_2".into(),
            hat: "hat_3".into(),
        })
    }

    #[test]
    fn five_requests_in_order() {
        let mut m = mgr();
        assert!(m.handle_player_info(11, 100, None).is_empty(), "还没连上 ⇒ 一条都不发");
        m.handle_connected_to_server(100);
        let reqs = m.handle_player_info(11, 100, Some(4));
        let tags: Vec<u16> = reqs.iter().map(|r| r.tag()).collect();
        assert_eq!(tags, vec![59, 50, 52, 54, 66]);
        assert_eq!(reqs.iter().map(|r| r.player_id()).collect::<Vec<_>>(), vec![11; 5]);
        match &reqs[4] {
            MetadataRequest::Rank { flag, level, player_id } => {
                assert_eq!((*flag, *level, *player_id), (1, 4, 11));
            }
            other => panic!("第五条必须是 Rank，得到 {other:?}"),
        }
        // 第二条 `player_info` 不再发请求。
        assert!(m.handle_player_info(12, 100, Some(4)).is_empty());
        assert_eq!(m.player_ids, vec![11, 12]);
    }

    #[test]
    fn retain_removes_every_match() {
        let mut m = mgr();
        m.handle_connected_to_server(7);
        m.handle_player_info(5, 7, None);
        m.player_ids.push(5);
        assert_eq!(m.handle_entity_removed_event(5), 2);
        assert!(m.player_ids.is_empty());
    }

    /// `new` 的三条订阅在代码侧落地（顺序 = `0x5179d4` / `0x517a27` / `0x517a77`）。
    #[test]
    fn new_registers_the_three_subscriptions() {
        let m = mgr();
        assert_eq!(
            m.subscribed,
            vec!["connected_to_server_event", "player_info_event", "entity_removed_event"]
        );
        assert_eq!(m.subscribed, SUBSCRIPTIONS.iter().map(|s| s.name).collect::<Vec<_>>());
        assert!(m.outbox.is_empty());
    }

    /// 发方向 6 条（模块文档 §9）：connected → 1 条 (8)；首次 player_info → 5 条
    /// 59/50/52/54/66；第二次 → 0；`entity_removed` → 0（`0x51a610` 无 `add_event`）。
    #[test]
    fn outbox_tracks_the_six_emitters() {
        let mut m = mgr();
        m.handle_connected_to_server(100);
        assert_eq!(m.take_outbox(), vec![Event::NewPlayerRequest]);

        m.handle_player_info(11, 100, Some(4));
        assert_eq!(
            m.take_outbox(),
            vec![
                Event::NameRequest,
                Event::SkinGraphicsRequest,
                Event::FaceGraphicsRequest,
                Event::HatGraphicsRequest,
                Event::RankRequest,
            ]
        );

        assert!(m.handle_player_info(12, 100, Some(4)).is_empty());
        assert!(m.outbox.is_empty(), "第二次 player_info 不发请求");

        m.handle_entity_removed_event(11);
        assert!(m.outbox.is_empty(), "删一条不发事件（0x51a610 无 add_event）");
    }

    /// 两条门的反面对照（模块文档 §9 ②③）：没连上 / 连接号不匹配 ⇒ 一条都不发。
    #[test]
    fn two_gates_block_before_any_request() {
        let mut m = mgr();
        assert!(m.handle_player_info(11, 100, None).is_empty(), "② connected != 1");
        assert!(m.outbox.is_empty());
        m.handle_connected_to_server(100);
        let _ = m.take_outbox();
        assert!(m.handle_player_info(11, 101, None).is_empty(), "③ connection_id 不匹配");
        assert!(m.outbox.is_empty());
    }
}
