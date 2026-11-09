//! 颜色：`GraphwarColor`（3 字节 r/g/b）+ `GraphwarColorManager`（两张 6 色队池 + 一张占用表）。
//!
//! 本模块回答四件事：**`docs/10` 那两张 6 色表和本模块是什么关系**（→ 就是
//! `init_team_colors` 里的两个字面量 `Vec`，§2）、**颜色按什么分配**（→ 队号选池 +
//! 随机起点 + 线性探测，§3，**不是**进房顺序、**不是** `player_id` 取模）、
//! **谁设颜色**（→ 本地只在两个"请求"入口挑，服务端的 (61) 走 `set_color` 覆盖，§4）、
//! **`ColorInfo` 的载荷**（§5）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（7 个，全部已定位）
//!
//! | 函数 | x86 vaddr / 字节 | arm64 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x465620` / 237 | `0x49cbe8` / 396 | 46 | 订阅 `new_connection_event` + `entity_removed_event`，池子由 `Rc::new_cyclic_in` 的初始化闭包建 | 逐位（两个闭包） |
//! | `init_team_colors` | **已内联**（无独立符号） | `0x49c660` / 944 | — | **两张 6 色表**（`HashMap<u8, Vec<GraphwarColor>>`，键 0/1） | 逐位 |
//! | `add_team_color` | `0x464d10` / 720 | `0x49c244` / 1052 | 92, 99 | 选色 + 发 `ColorInfo`(61) + 写占用表 | 逐位 |
//! | `set_color` | 内联进 `0x3e2830` | `0x49cd74` / 168 | 114 | 服务端说了算：发 (61) + 写占用表 | 逐位 |
//! | `send_all_info_events` | `0x465460` / 231 | `0x49ca10` / 260 | 124 | 遍历占用表逐条发 (61) | 逐位 |
//! | `handle_entity_removed_event` | `0x465570` / 77 | `0x49cb14` / 120 | 134 | 从占用表删（颜色**回到池子**） | 逐位 |
//! | `GraphwarColor::serialize` | `0x464c00` | `0x49c0e8` / 348 | — | 三个字段名 `r`/`g`/`b`，各 1 字节 | 逐位 |
//!
//! `init_team_colors` 在 x86 侧内联进 `new`（所以 `docs/02` §18.1 只列 5 个方法）；
//! arm64 因为未 strip，它是独立符号，并且**只被 `alloc::rc::Rc::new_cyclic_in` 的
//! 那一个实例调用**（`0x4190c4` → `0x419148`）—— 即 `new` 的初始化闭包。
//!
//! ## 2. `docs/10` §10 的两张 6 色表 = `init_team_colors` 里的两个字面量（逐位，独立复算）
//!
//! `GraphwarColor` 是 **3 字节**（`Serialize` 三个 `serialize_field`，名 `r`/`g`/`b`），
//! 所以 `vec![...]` 的六次 push 全部落在 `strb`/`sturh` 这种 1/2 字节写：
//!
//! ```text
//! ; Team1（键 0）
//! mov w10,#0x2aba ; strh w10,[x8]      ; 0xba 0x2a → (186, 42, …)
//! mov w9,#0x20    ; strb w9,[x8,#2]    ; 0x20      → (…, …, 32)
//! …
//! ```
//!
//! 逐元素复算结果与 `docs/10` §10 一字不差：
//!
//! | 队 | 键 | 表（索引 0 → 5） |
//! |---|---|---|
//! | `Team1` | `0` | `(186,42,32) (238,114,0) (125,104,0) (122,160,0) (149,5,62) (234,0,219)` |
//! | `Team2` | `1` | `(107,0,234) (17,0,108) (21,83,221) (0,203,163) (0,116,26) (2,85,63)` |
//!
//! 两表**不相交**（12 个值两两不同）——本模块不强制"全局唯一"，唯一性只是"两个池子
//! 各管各的"的副产品。
//!
//! 表容器是 `HashMap<u8, Vec<GraphwarColor>>`（键是 **1 字节**：`sturb w2,[x29,#-0x4]`
//! 之后 `hash_one(self+0x68, 那个字节)`），所以 `add_team_color` 的第二个参数是 `u8`。
//!
//! ## 3. 分配算法（逐位，`add_team_color` 全函数）
//!
//! ```text
//! fn add_team_color(self, entity_id: u32, team: u8)            ; w1 / w2
//!     let pool = self.pools.get(&team) else -> random_rgb      ; 没这个队 ⇒ 兜底
//!     let n = pool.len()                                       ; = 6
//!     if n == 0 { -> random_rgb }
//!     let base = rand::random::<u64>() % n                     ; udiv + msub
//!     for k in 0..n {
//!         let c = pool[base]                                   ; 3 字节，r/g/b 三取
//!         if !self.used.values().any(|u| *u == c) {            ; 线性扫占用表
//!             emit(ColorInfo { entity_id, color: c })          ; tag 61
//!             self.used.insert(entity_id, c)
//!             return
//!         }
//!         base = (base + 1) % n                                ; cmp + csinc
//!     }
//!     random_rgb:                                              ; 6 个都被占
//!     let r = rand::random::<u8>() % 180                       ; ← 见下
//!     let g = …; let b = …                                     ; 三次独立取值
//!     emit(ColorInfo { … }); self.used.insert(entity_id, c)
//! ```
//!
//! 兜底那三行的**机器码形状**值得记住（这是判 `% 180` 而不是 `gen_range` 的关键）：
//!
//! ```text
//! and  w8, w21, #0xff      ; b = x & 0xff
//! add  w9, w21, #0x4c      ; b + 76
//! cmp  w8, #0xb4           ; b < 180 ?
//! csel w22, w21, w9, lo    ; lo ⇒ b，否则 b+76（= b-180，取低字节）
//! ```
//!
//! 对 `u8` 来说 `b % 180` 正好就是"`b<180` 取 `b`，否则取 `b-180`"，
//! 所以这是 `rand::random::<u8>() % 180`，与 `docs/10` §10 的
//! `(r8()%180, r8()%180, r8()%180)` **同一件事**。
//!
//! 三条排他的可能（本模块**否定**它们）：
//!
//! * **不是进房顺序**：分配时唯一的输入是 `team` 与随机数；`entity_id` 只当 key。
//! * **不是 `player_id` 取模**：`%` 的左边是 `rand::random::<u64>()`，右边是**池长 6**。
//! * **不是每局/每队固定**：起点每次重新抽（所以同一队第 2 个人拿到的颜色不是"索引 1"）。
//!
//! ## 4. 谁设颜色（逐位，`bl` 全扫）
//!
//! | 调用点 | 所在函数 | 含义 |
//! |---|---|---|
//! | `0x4a9b58` | `game_state_manager::request_new_player` | 玩家加入（按目标队的池子挑） |
//! | `0x4aa044` | `game_state_manager::request_change_team` | 换队（用**新队**的池子重挑） |
//! | `0x3e7e9c` | `remote_event_handler::handle_server_remote_event` → `set_color` | 服务端 (61) 到达：**直接采纳**，不重新挑 |
//!
//! ⇒ **权威在服务端**：本地只在"加入/换队"两个请求处挑一次；收到 (61) 一律以
//! `set_color` 覆盖本地占用表（`set_color` 是"发 + 写"，不查池子、不判重）。
//! 这与 `docs/10` §10 的"颜色绑在玩家实体 id 上、换队才重算"一致，并把"谁发起"
//! 定到了**两个具体函数**上（`docs/10` §10 只给了地址 `0x473b4c` / `0x473f29`）。
//!
//! ## 5. `ColorInfo`(61) 的载荷：`u32` + 3 字节颜色（逐位）
//!
//! ```text
//! ; set_color(self, entity_id: u32 = w1, color: GraphwarColor = w2)
//! lsr  w9, w2, #16        ; b
//! str  w1, [sp, #0x10]    ; 载荷 +0x00 = entity_id  （事件基址 sp+0x8）
//! strh w2, [sp, #0x14]    ; 载荷 +0x04/+0x05 = r, g
//! strb w9, [sp, #0x16]    ; 载荷 +0x06 = b
//! ```
//!
//! ⇒ 载荷 = `{ entity_id: u32 @+0x00, r @+0x04, g @+0x05, b @+0x06 }`，共 8 字节
//! （颜色按 **3 字节 struct** 传参，寄存器里是 `r | g<<8 | b<<16`）。
//! **先发事件、后写占用表**（`add_event` 在 `0x49c564`，`insert` 在 `0x49c580`）——
//! 顺序在 `set_color` 里同样是"先发后写"。
//!
//! ## 6. 头部三个字与 `+0x10` 的语义（任务 77 把 G2 结案）
//!
//! 120 B 的切法：`+0x00/+0x08/+0x10` 三个字（**0x18 的头**）+ 两张各 0x30 的表
//! = `0x18 + 0x30 + 0x30 = 0x78 = 120`（逐位，见下面的写入点）。三者的身份：
//!
//! | 偏移 | 是什么 | 逐位证据（x86） |
//! |---|---|---|
//! | `+0x00` | 弱自句柄（`Weak<RcInner<Self>>`，值 = 盒基） | 建盒 `0x52422f`：`mov [rbx+0x18], rbx`；`new` 读它 `0x465652 mov r15,[rbx+0x18]`，随后 `0x46565c inc [r15+8]`（**弱**计数 +1） |
//! | `+0x08` | 事件通道 `Rc<RefCell<GraphwarEventQueue>>` | 建盒 `0x524233`：`mov [rbx+0x20], r14`；调用点 `0x517e83 inc [r14]`（**强**计数 +1）= 传进来的是 `Rc`；`send_all_info_events` 读 `0x465480 mov r12,[rdi+8]` 之后 `0x4654ca cmp [r12+0x10],0` / `0x4654e0 mov [r12+0x10],-1` / `0x46554a call [0x9bac80]` 收到 `lea rdi,[r12+0x18]`（= `Rc` 盒的 RefCell 值） |
//! | `+0x10` | **容器弱引用** `Weak<RefCell<GraphwarModuleContainer>>` | 建盒 `0x524237`：`mov [rbx+0x28], r15`（= `M+0x10`）；调用点 `0x517e8c inc [r15+8]`（**弱**计数 +1）；此外**本模块 4 个函数体内对 `M+0x10` 的读点 = 0**（下条） |
//!
//! ### 6.1 `+0x10` 的读点普查（**有界否定**，带正对照）
//!
//! 把本模块 4 个函数的反汇编按内存操作数过滤（`addr_probe.py dis` 四份输出）：
//!
//! ```text
//! 0x464d10 add_team_color           +0x10 操作数全部落在 r12 = 事件通道盒（RefCell 借用字）
//!                                   +0x28 操作数：0 条
//! 0x465460 send_all_info_events     同上（rbp = 桶地址、r12 = 事件通道盒）
//! 0x465570 handle_entity_removed    +0x10 / +0x28 操作数：0 条
//! 0x465620 new                      [rax+0x10]/[rbx+0x10] = RefCell 借用字（0 → -1 → 0）
//! ```
//!
//! **正对照**：同一次扫描能看到 `+0x18`（占用表 ctrl）、`+0x30`（items）、`+0x38`（hasher）、
//! `+0x48`（队池表基）、`+0x60`（队池 items）、`+0x68`（队池 hasher）的读写 ——
//! 所以"`+0x10` 零读点"不是扫法坏了。
//!
//! ### 6.2 为什么判"容器弱引用"而不是"别的 `Weak`"
//!
//! 同一个构造实参（`0x517690` 里第三条 `mov rdx, r15`：`0x517e9c` / `0x517e27` …，
//! 22 个 manager 全部同款）也被 `FunctionHistoryRecorder` 存在 `+0x28` 并在
//! 处理器 `0x3f0e40` 里**升级**（`0x3f0e7f add rdi,0x28` → `call 0x46e580`）；
//! `0x46e580` 的 `panic::Location` 落在 **`src/graphwar_module_container.rs:30:28`**
//! （`addr_probe.py locs 0x46e580`），即它是容器的 `get_module_wk` 一族。
//! ⇒ 这个 `Weak` 指向**模块容器**；容器里取出来的模块再按 `+0x18` 那张表查 `player_id`。
//!
//! ## 7. 未确定
//!
//! * `add_team_color` 的 `team` 实参在 `request_new_player` / `request_change_team` 里
//!   怎么来的（`docs/10` §13.3 的"队号算术"在 `team_manager`，不在本模块）。
//! * 随机流的**消耗顺序**：起点用一次 `u64`（8 字节），兜底用三次 `u8`；
//!   两者在 `rand_chacha` 的 block RNG 里取的是不同宽度的块 ⇒ 复现"第 n 个玩家"的
//!   颜色必须用真 `rand` 流，本模块只给出**判定规则**（标推断）。
//! * `init_team_colors` 在 x86 侧**是一个独立函数 `0x465070`**（唯一调用点
//!   `0x5241e9`，在 `Rc::new_cyclic` 的实例 `0x524140` 里）；它不是方法，所以
//!   `docs/02` 的方法表里没有它 —— 与 §1 的"内联"说法不矛盾（**没有符号**≠没有函数体）。
//!
//! ## 8. 事件流接线（任务 77 逐条复核，x86；`wired` 的第二件）
//!
//! ```text
//! new 0x465620
//!   0x46568c lea rsi,[0x1c18ad] 'new_connection_event' + 0x46569a mov edx,0x14
//!   0x465693 lea r8,<0x957dc0>   +0x20 = 0x465600 → lea rsi,[0x465460] send_all_info_events
//!   0x4656a5 call [0x9bac88] → 0x3d38f0   == subscribe_to_event
//!   0x4656dd lea rsi,[0x1c18c1] 'entity_removed_event' + 0x4656eb mov edx,0x14
//!   0x4656e4 lea r8,<0x957de8>   +0x20 = 0x4655e0 → lea rsi,[0x465570] handle_entity_removed_event
//!   0x4656f6 call [0x9bac88] → 0x3d38f0
//!
//! 发（两处，tag word 0x8000_0000_0000_003d = 61）
//!   add_team_color 0x464d10: 0x464fa2 movabs …3d ; 0x464fb6 call [0x9bac80] → 0x3d3530
//!   send_all_info_events 0x465460: 0x465501 movabs …3d ; 0x465517 call [0x9bac80]
//!   handle_entity_removed_event 0x465570: 0x465577 断言 tag 14；体内**零** add_event
//! ```
//!
//! ## 9. 复跑
//!
//! ```bash
//! ../.venv/bin/python tools/addr_probe.py dis 0x465620   # 2 条订阅（+ 闭包槽 0x957dc0/0x957de8）
//! ../.venv/bin/python tools/addr_probe.py dis 0x524140   # 建盒：0x90 = 16+8+120、+0x08/+0x10 的写入点
//! ../.venv/bin/python tools/addr_probe.py dis 0x465070   # init_team_colors（两份 6 色表）
//! ../.venv/bin/python tools/addr_probe.py dis 0x464d10   # 发 61（0x464fb6）
//! ../.venv/bin/python tools/addr_probe.py dis 0x465460   # 整表重播 61（0x465517）
//! ```

/// `color_info_event` 的判别值。
pub const EVENT_COLOR_INFO: u16 = 61;
/// `new_connection_event` 的判别值（订阅 1）。
pub const EVENT_NEW_CONNECTION: u16 = 5;
/// `entity_removed_event` 的判别值（订阅 2）。
pub const EVENT_ENTITY_REMOVED: u16 = 14;

/// 弱自句柄在 `self` 里的偏移（`Weak<RcInner<Self>>`，值 = 盒基）。
pub const SELF_WEAK_OFFSET: usize = 0x00;
/// 事件通道在 `self` 里的偏移（`Rc<RefCell<GraphwarEventQueue>>`）。
pub const EVENT_QUEUE_OFFSET: usize = 0x08;
/// 容器弱引用在 `self` 里的偏移（`Weak<RefCell<GraphwarModuleContainer>>`，见模块文档 §6）。
pub const CONTAINER_WEAK_OFFSET: usize = 0x10;
/// 三个字的头 —— 两张表从 [`USED_MAP_OFFSET`] 起。
pub const HEADER_SIZE: usize = 0x18;
/// 占用表在 `self` 里的起始偏移。
pub const USED_MAP_OFFSET: usize = 0x18;
/// 占用表 hasher 相对表基的偏移（`hash(self + 0x38)`）。
pub const USED_MAP_HASHER_OFFSET: usize = 0x20;
/// 队池表在 `self` 里的起始偏移。
pub const POOL_MAP_OFFSET: usize = 0x48;
/// 队池表 hasher 相对表基的偏移（`hash(self + 0x68)`）。
pub const POOL_MAP_HASHER_OFFSET: usize = 0x20;
/// 一张队池的固定长度。
pub const TEAM_POOL_LEN: usize = 6;
/// 兜底随机色的取值范围分母（`b % 180`）。
pub const RANDOM_COLOR_MODULO: u8 = 180;

/// `ColorInfo`(61) 载荷里 `entity_id` 的偏移。
pub const COLOR_INFO_ENTITY_OFFSET: usize = 0x00;
/// `ColorInfo`(61) 载荷里颜色 `r` 的偏移。
pub const COLOR_INFO_R_OFFSET: usize = 0x04;
/// `ColorInfo`(61) 载荷里颜色 `g` 的偏移。
pub const COLOR_INFO_G_OFFSET: usize = 0x05;
/// `ColorInfo`(61) 载荷里颜色 `b` 的偏移。
pub const COLOR_INFO_B_OFFSET: usize = 0x06;

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

/// `GraphwarColorManager::new` 的两条订阅（逐位，见模块文档 §1）。
pub const SUBSCRIPTIONS: [Subscription; 2] = [
    Subscription {
        name: "new_connection_event",
        tag: EVENT_NEW_CONNECTION,
        handler: "send_all_info_events",
    },
    Subscription {
        name: "entity_removed_event",
        tag: EVENT_ENTITY_REMOVED,
        handler: "handle_entity_removed_event",
    },
];

/// 一个颜色：**3 字节**，字段序 `r` / `g` / `b`（`graphics_manager` 那边不是它）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct GraphwarColor {
    /// 红。
    pub r: u8,
    /// 绿。
    pub g: u8,
    /// 蓝。
    pub b: u8,
}

impl GraphwarColor {
    /// 构造。
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// 结构体字节数。
    pub const SIZE: usize = 3;
    /// `Serialize` 的字段名（顺序即写出顺序，见模块文档 §1 末行）。
    pub const FIELD_NAMES: [&'static str; 3] = ["r", "g", "b"];

    /// 打包成寄存器里的 `r | g<<8 | b<<16`（原版按 3 字节 struct 传参）。
    pub const fn to_u32(self) -> u32 {
        (self.r as u32) | ((self.g as u32) << 8) | ((self.b as u32) << 16)
    }

    /// 从 `r | g<<8 | b<<16` 解回。
    pub const fn from_u32(v: u32) -> Self {
        Self {
            r: (v & 0xff) as u8,
            g: ((v >> 8) & 0xff) as u8,
            b: ((v >> 16) & 0xff) as u8,
        }
    }

    /// 三元组（`docs/10` §10 的写法）。
    pub const fn triple(self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }
}

/// `Team1`（键 `0`）的 6 色表 —— **逐位**，见模块文档 §2。
pub const TEAM1_COLORS: [GraphwarColor; TEAM_POOL_LEN] = [
    GraphwarColor::new(186, 42, 32),
    GraphwarColor::new(238, 114, 0),
    GraphwarColor::new(125, 104, 0),
    GraphwarColor::new(122, 160, 0),
    GraphwarColor::new(149, 5, 62),
    GraphwarColor::new(234, 0, 219),
];

/// `Team2`（键 `1`）的 6 色表 —— **逐位**，见模块文档 §2。
pub const TEAM2_COLORS: [GraphwarColor; TEAM_POOL_LEN] = [
    GraphwarColor::new(107, 0, 234),
    GraphwarColor::new(17, 0, 108),
    GraphwarColor::new(21, 83, 221),
    GraphwarColor::new(0, 203, 163),
    GraphwarColor::new(0, 116, 26),
    GraphwarColor::new(2, 85, 63),
];

/// `init_team_colors` 的两个键：`0` = Team1、`1` = Team2。
pub const TEAM1_KEY: u8 = 0;
/// 见 [`TEAM1_KEY`]。
pub const TEAM2_KEY: u8 = 1;

/// `color_info_event`(61) 的载荷。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorInfo {
    /// 载荷 `+0x00`：实体（玩家）id。
    pub entity_id: u32,
    /// 载荷 `+0x04..+0x07`：颜色（3 字节）。
    pub color: GraphwarColor,
}

impl ColorInfo {
    /// 事件判别值。
    pub const TAG: u16 = EVENT_COLOR_INFO;
    /// 载荷字节数（u32 + 3 字节颜色 + 1 字节对齐）。
    pub const PAYLOAD_SIZE: usize = 8;
    /// 完整 tag word。
    pub const fn tag_word() -> u64 {
        0x8000_0000_0000_0000 | EVENT_COLOR_INFO as u64
    }
}

/// 原版取随机数用的两口子集（都是 `rand` 的**线程局部** ChaCha 流）。
///
/// 单独抽成 trait 是为了让"选色规则"可以脱离 `rand` 复现（本 crate 无第三方依赖）。
pub trait ColorRng {
    /// 等价 `rand::random::<u64>()`（起点用一次）。
    fn next_u64(&mut self) -> u64;
    /// 等价 `rand::random::<u8>()`（兜底随机色用三次）。
    fn next_u8(&mut self) -> u8;
}

/// 可复现的替身：按预设序列吐数，只为测试与原版的**判定规则**对齐。
#[derive(Debug, Clone, Default)]
pub struct SeqRng {
    /// 依次返回的 `u64`（用尽后回绕到 0）。
    pub u64s: Vec<u64>,
    /// 依次返回的 `u8`（用尽后回绕到 0）。
    pub u8s: Vec<u8>,
    /// `u64` 已取个数。
    pub u64_pos: usize,
    /// `u8` 已取个数。
    pub u8_pos: usize,
}

impl SeqRng {
    /// 用给定序列造一个替身。
    pub fn new(u64s: Vec<u64>, u8s: Vec<u8>) -> Self {
        Self { u64s, u8s, u64_pos: 0, u8_pos: 0 }
    }
}

impl ColorRng for SeqRng {
    fn next_u64(&mut self) -> u64 {
        let v = self.u64s.get(self.u64_pos).copied().unwrap_or(0);
        self.u64_pos += 1;
        v
    }

    fn next_u8(&mut self) -> u8 {
        let v = self.u8s.get(self.u8_pos).copied().unwrap_or(0);
        self.u8_pos += 1;
        v
    }
}

/// `GraphwarColorManager` 的可判定状态。
///
/// # 事件流（任务 77 补，`wired` 的第二件；见模块文档 §8）
///
/// `new`（x86 `0x465620`）订阅 [`SUBSCRIPTIONS`] 那两条：
/// `new_connection_event`@`0x4656a5` → 闭包槽 `0x957dc0`（`+0x20` = `0x465600`）→ `0x465460`；
/// `entity_removed_event`@`0x4656f6` → 闭包槽 `0x957de8`（`+0x20` = `0x4655e0`）→ `0x465570`。
/// 发方向两处都在 `add_event`（`0x9bac80`）上：`0x464fb6`（`add_team_color`）与 `0x465517`
/// （整表重播），tag 都是 `0x8000_0000_0000_003d`(61)；
/// `handle_entity_removed_event` 体内**零** `add_event`（只删表，颜色自动回池）。
///
/// 发件箱只记判别值（[`Event::ColorInfo`]），载荷仍由 [`ColorInfo`] 返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphwarColorManager {
    /// `HashMap<u32, GraphwarColor>`（`self + 0x18`）：当前"实体 → 颜色"。
    pub used: std::collections::BTreeMap<u32, GraphwarColor>,
    /// `HashMap<u8, Vec<GraphwarColor>>`（`self + 0x48`）：队号 → 6 色池。
    pub pools: std::collections::BTreeMap<u8, Vec<GraphwarColor>>,
    /// 订阅名（`new` 的两条 `subscribe`，顺序 = 调用序，`观测`）。
    pub subscribed: Vec<&'static str>,
    /// 发件箱（判别值；载荷见 [`ColorInfo`]）。
    pub outbox: Vec<crate::events::Event>,
}

impl Default for GraphwarColorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphwarColorManager {
    /// 新建：占用表空、两张队池来自 [`Self::init_team_colors`] + 订阅 [`SUBSCRIPTIONS`]。
    ///
    /// 原版 `new`（x86 `0x465620`）= 建盒（`0x524140`，`0x90` = 16+8+120）+ 两条订阅；
    /// 两张 6 色表由 `0x465070`（`init_team_colors`）在建盒里建好。
    pub fn new() -> Self {
        Self {
            used: Default::default(),
            pools: Self::init_team_colors(),
            subscribed: SUBSCRIPTIONS.iter().map(|s| s.name).collect(),
            outbox: Vec::new(),
        }
    }

    /// `init_team_colors`：两份固定字面量，键 `0`/`1`。
    pub fn init_team_colors() -> std::collections::BTreeMap<u8, Vec<GraphwarColor>> {
        let mut pools = std::collections::BTreeMap::new();
        pools.insert(TEAM1_KEY, TEAM1_COLORS.to_vec());
        pools.insert(TEAM2_KEY, TEAM2_COLORS.to_vec());
        pools
    }

    /// 取某队的池子（没有就是 `None`）。
    pub fn pool(&self, team: u8) -> Option<&[GraphwarColor]> {
        self.pools.get(&team).map(|v| v.as_slice())
    }

    /// `add_team_color(entity_id, team)`：按模块文档 §3 挑色，返回该发的那条 (61)。
    ///
    /// 返回 `None` 只在池子不存在且没走到兜底时才会发生——本实现里兜底总会给一个颜色，
    /// 所以恒为 `Some`（保留 `Option` 只为让"没这个队"这条分支显式可见）。
    pub fn add_team_color<R: ColorRng>(
        &mut self,
        entity_id: u32,
        team: u8,
        rng: &mut R,
    ) -> ColorInfo {
        let color = match self.pick_from_pool(team, rng) {
            Some(c) => c,
            None => Self::random_color(rng),
        };
        self.set_color(entity_id, color)
    }

    /// §3 的"随机起点 + 线性探测"；池子缺失/为空/全占都返回 `None`（走兜底）。
    fn pick_from_pool<R: ColorRng>(&self, team: u8, rng: &mut R) -> Option<GraphwarColor> {
        let pool = self.pools.get(&team)?;
        let n = pool.len();
        if n == 0 {
            return None;
        }
        let mut idx = (rng.next_u64() % n as u64) as usize;
        for _ in 0..n {
            let c = pool[idx];
            if !self.used.values().any(|u| *u == c) {
                return Some(c);
            }
            idx = (idx + 1) % n;
        }
        None
    }

    /// 兜底随机色：三个独立的 `random::<u8>() % 180`。
    pub fn random_color<R: ColorRng>(rng: &mut R) -> GraphwarColor {
        GraphwarColor::new(
            rng.next_u8() % RANDOM_COLOR_MODULO,
            rng.next_u8() % RANDOM_COLOR_MODULO,
            rng.next_u8() % RANDOM_COLOR_MODULO,
        )
    }

    /// `set_color(entity_id, color)`：发 (61) + 写占用表（不查池子、不判重）。
    ///
    /// 逐位：**先发后写**（`add_event` 在 `0x464fb6`，`insert` 在它之后；`set_color`
    /// 内联进 `0x3e2830` 时同序）。本实现同样先 push 再 insert。
    pub fn set_color(&mut self, entity_id: u32, color: GraphwarColor) -> ColorInfo {
        self.outbox.push(crate::events::Event::ColorInfo);
        self.used.insert(entity_id, color);
        ColorInfo { entity_id, color }
    }

    /// `send_all_info_events`（`0x465460`）：遍历占用表逐条发 (61)。
    pub fn send_all_info_events(&self) -> Vec<ColorInfo> {
        self.used
            .iter()
            .map(|(entity_id, color)| ColorInfo { entity_id: *entity_id, color: *color })
            .collect()
    }

    /// `new_connection_event` 的处理（闭包槽 `0x957dc0` → `0x465600` → `0x465460`）：整表重播。
    ///
    /// 原版每遍历到一行就 `add_event` 一条 (61)（`0x465501`/`0x465517`）；本实现把同样条数
    /// 记进发件箱并返回载荷。
    pub fn handle_new_connection_event(&mut self) -> Vec<ColorInfo> {
        let events = self.send_all_info_events();
        self.outbox.extend(events.iter().map(|_| crate::events::Event::ColorInfo));
        events
    }

    /// `handle_entity_removed_event`（`0x465570`）：删一条（颜色自动回池），返回是否删到了。
    ///
    /// 逐位：函数体只做「断言 tag 14 → 以 `event+8` 为键删表」，**没有** `add_event`。
    pub fn handle_entity_removed_event(&mut self, entity_id: u32) -> bool {
        self.used.remove(&entity_id).is_some()
    }

    /// 取走发件箱（与原版 `GraphwarEventQueue` 的"取空"同口径）。
    pub fn take_outbox(&mut self) -> Vec<crate::events::Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }

    /// 查某实体当前颜色。
    pub fn color_of(&self, entity_id: u32) -> Option<GraphwarColor> {
        self.used.get(&entity_id).copied()
    }

    /// 当前有多少条占用。
    pub fn len(&self) -> usize {
        self.used.len()
    }

    /// 占用表是否为空。
    pub fn is_empty(&self) -> bool {
        self.used.is_empty()
    }
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn color_is_three_bytes_rgb() {
        assert_eq!(GraphwarColor::SIZE, 3);
        assert_eq!(GraphwarColor::FIELD_NAMES, ["r", "g", "b"]);
        let c = GraphwarColor::new(1, 2, 3);
        assert_eq!(c.to_u32(), 0x0003_0201);
        assert_eq!(GraphwarColor::from_u32(0x0003_0201), c);
        assert_eq!(c.triple(), (1, 2, 3));
    }

    #[test]
    fn color_info_offsets() {
        assert_eq!(ColorInfo::PAYLOAD_SIZE, 8);
        assert_eq!(
            (
                COLOR_INFO_ENTITY_OFFSET,
                COLOR_INFO_R_OFFSET,
                COLOR_INFO_G_OFFSET,
                COLOR_INFO_B_OFFSET
            ),
            (0x00, 0x04, 0x05, 0x06)
        );
        assert_eq!(ColorInfo::tag_word(), 0x8000_0000_0000_003d);
    }

    #[test]
    fn two_tables_are_disjoint_and_six_each() {
        assert_eq!(TEAM1_COLORS.len(), TEAM_POOL_LEN);
        assert_eq!(TEAM2_COLORS.len(), TEAM_POOL_LEN);
        for a in TEAM1_COLORS.iter() {
            assert!(!TEAM2_COLORS.contains(a), "两表必须不相交");
        }
    }

    /// 头部三个字：弱自句柄 / 事件通道 / 容器弱引用（模块文档 §6）。
    #[test]
    fn header_is_three_words_before_the_two_tables() {
        assert_eq!(
            (SELF_WEAK_OFFSET, EVENT_QUEUE_OFFSET, CONTAINER_WEAK_OFFSET, HEADER_SIZE),
            (0x00, 0x08, 0x10, 0x18)
        );
        // 120 = 0x18 头 + 0x30 占用表 + 0x30 队池表 —— 逐位（建盒 `0x524140` 的 alloc 是 0x90）
        assert_eq!(HEADER_SIZE + 0x30 + 0x30, 120);
        assert_eq!(USED_MAP_OFFSET, HEADER_SIZE);
        assert_eq!(POOL_MAP_OFFSET, USED_MAP_OFFSET + 0x30);
    }

    /// 订阅：两条，名字/长度/处理器与 `0x4656a5` / `0x4656f6` 那两条一致。
    #[test]
    fn new_subscribes_the_two_events() {
        let m = GraphwarColorManager::new();
        assert_eq!(m.subscribed, vec!["new_connection_event", "entity_removed_event"]);
        assert_eq!(m.subscribed.len(), SUBSCRIPTIONS.len());
        assert_eq!(SUBSCRIPTIONS[0].tag, 5);
        assert_eq!(SUBSCRIPTIONS[1].tag, 14);
        assert_eq!(SUBSCRIPTIONS[0].handler, "send_all_info_events");
        assert_eq!(SUBSCRIPTIONS[1].handler, "handle_entity_removed_event");
        assert!(m.outbox.is_empty());
        assert_eq!(m.used.len(), 0);
    }

    /// 发方向两处：`set_color`/`add_team_color` 各一条 (61)，整表重播 N 条，
    /// `handle_entity_removed_event` **一条都不发**（`0x465570` 体内零 `add_event`）。
    #[test]
    fn outbox_tracks_the_two_emitters() {
        let mut rng = SeqRng::new(vec![0], vec![]);
        let mut m = GraphwarColorManager::new();
        m.set_color(1, GraphwarColor::new(1, 2, 3));
        assert_eq!(m.outbox_len(), 1);
        assert_eq!(m.take_outbox(), vec![crate::events::Event::ColorInfo]);
        // `add_team_color` 走同一条 `set_color` ⇒ 同样一条
        let info = m.add_team_color(2, TEAM1_KEY, &mut rng);
        assert_eq!(info.entity_id, 2);
        assert_eq!(m.outbox_len(), 1);
        // 整表重播：两行 ⇒ 两条
        let replayed = m.handle_new_connection_event();
        assert_eq!(replayed.len(), 2);
        assert_eq!(m.outbox.len(), 3);
        // 删一条不发事件
        assert!(m.handle_entity_removed_event(2));
        assert_eq!(m.outbox.len(), 3, "0x465570 体内零 add_event");
        assert!(m.color_of(2).is_none(), "删掉之后颜色回到池子");
    }
}
