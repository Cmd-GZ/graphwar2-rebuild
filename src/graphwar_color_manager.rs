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
//! ## 6. 未确定
//!
//! * `new` 的第三个字段（`self + 0x10`）没被本模块任何函数读过，语义**未知**。
//! * `add_team_color` 的 `team` 实参在 `request_new_player` / `request_change_team` 里
//!   怎么来的（`docs/10` §13.3 的"队号算术"在 `team_manager`，不在本模块）。
//! * 随机流的**消耗顺序**：起点用一次 `u64`（8 字节），兜底用三次 `u8`；
//!   两者在 `rand_chacha` 的 block RNG 里取的是不同宽度的块 ⇒ 复现"第 n 个玩家"的
//!   颜色必须用真 `rand` 流，本模块只给出**判定规则**（标推断）。

/// `color_info_event` 的判别值。
pub const EVENT_COLOR_INFO: u16 = 61;
/// `new_connection_event` 的判别值（订阅 1）。
pub const EVENT_NEW_CONNECTION: u16 = 5;
/// `entity_removed_event` 的判别值（订阅 2）。
pub const EVENT_ENTITY_REMOVED: u16 = 14;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphwarColorManager {
    /// `HashMap<u32, GraphwarColor>`（`self + 0x18`）：当前"实体 → 颜色"。
    pub used: std::collections::BTreeMap<u32, GraphwarColor>,
    /// `HashMap<u8, Vec<GraphwarColor>>`（`self + 0x48`）：队号 → 6 色池。
    pub pools: std::collections::BTreeMap<u8, Vec<GraphwarColor>>,
}

impl Default for GraphwarColorManager {
    fn default() -> Self {
        Self { used: Default::default(), pools: Self::init_team_colors() }
    }
}

impl GraphwarColorManager {
    /// 新建（占用表空、两张队池来自 [`Self::init_team_colors`]）。
    pub fn new() -> Self {
        Self::default()
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
    pub fn set_color(&mut self, entity_id: u32, color: GraphwarColor) -> ColorInfo {
        self.used.insert(entity_id, color);
        ColorInfo { entity_id, color }
    }

    /// `send_all_info_events`：遍历占用表逐条发 (61)。
    pub fn send_all_info_events(&self) -> Vec<ColorInfo> {
        self.used
            .iter()
            .map(|(entity_id, color)| ColorInfo { entity_id: *entity_id, color: *color })
            .collect()
    }

    /// `handle_entity_removed_event`：删一条（颜色自动回池），返回是否删到了。
    pub fn handle_entity_removed_event(&mut self, entity_id: u32) -> bool {
        self.used.remove(&entity_id).is_some()
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
}
