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
//! | `handle_entity_removed_event` | `0x51a610` / 214 | `0x411c9c` / 176 | 146 | `Vec::retain` 掉该 id | 逐位 |
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
//! * `NewPlayerRequest`(8) 的载荷为什么是**连接号**而不是别的（服务端那侧未读）。
//! * `+0x20` 那个 8 字节槽从没被本模块读过，语义**未知**。
//! * 配置里四个 `String` 的**字段名**（`player_config` 模块的 `Serialize` 才带名字，
//!   本模块只看到偏移）⇒ 名字是 `推断`，顺序（名字/皮肤/脸/帽子）来自四条请求的语义。

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
}

/// 本机玩家管理器。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphwarLocalPlayerManager {
    /// `Vec<u32>`（`+0x00/+0x08/+0x10`）：我名下的所有 `player_id`。
    pub player_ids: Vec<u32>,
    /// `+0x18`：是否已收到 `connected_to_server_event`（只有 1 / 非 1 两种用法）。
    pub connected: u32,
    /// `+0x1c`：我的 `connection_id`。
    pub connection_id: u32,
    /// `+0x38` 里那份配置。
    pub profile: LocalPlayerProfile,
}

impl GraphwarLocalPlayerManager {
    /// 新建（原版还会订阅 [`SUBSCRIPTIONS`] 三条事件）。
    pub fn new(profile: LocalPlayerProfile) -> Self {
        Self { player_ids: Vec::new(), connected: 0, connection_id: 0, profile }
    }

    /// `handle_connected_to_server`：记下自己的连接号、请求建玩家、清空 id 列表。
    ///
    /// 注意三件事的**顺序**（逐位）：先写 `+0x18/+0x1c`，再发 (8)，最后清 `+0x10`。
    pub fn handle_connected_to_server(&mut self, connection_id: u32) -> u32 {
        self.connected = 1;
        self.connection_id = connection_id;
        // 原版此处 emit(NewPlayerRequest { connection_id })；返回它作为可断言的载荷。
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
        vec![
            MetadataRequest::Name { name: self.profile.name.clone(), player_id },
            MetadataRequest::Skin { skin: self.profile.skin.clone(), player_id },
            MetadataRequest::Face { face: self.profile.face.clone(), player_id },
            MetadataRequest::Hat { hat: self.profile.hat.clone(), player_id },
            MetadataRequest::Rank {
                flag: RANK_REQUEST_FLAG,
                level: level.unwrap_or(0),
                player_id,
            },
        ]
    }

    /// `handle_entity_removed_event`：把该 id 从 [`Self::player_ids`] 里**全部**删掉。
    ///
    /// 逐位：`Vec::retain`（x86 `0x51a610`、arm64 `0x411c9c` 两边同形）。返回删了几个。
    pub fn handle_entity_removed_event(&mut self, entity_id: u32) -> usize {
        let before = self.player_ids.len();
        self.player_ids.retain(|id| *id != entity_id);
        before - self.player_ids.len()
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
}
