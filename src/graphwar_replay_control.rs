//! 回放界面控制器：`GraphwarReplayScreenControl` —— **点「回放」进的那一屏**。
//!
//! 本模块回答三件事：**回放列表从哪来**（读磁盘目录 + 后台线程解析，用
//! `std::sync::mpmc` 通道把结果送回主线程）、**每帧做什么**（`process`）、
//! **四个模式按钮**（`AxisModeButton` / `FunctionModeButton` / `TurnModeButton` /
//! `TimeModeButton` + `set_button_icon`）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（4 个）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x4cadfc` / 240 | **无锚** | — | 建 `Rc<RefCell<…>>` + `mpmc` 通道 | 观测 |
//! | `update` | `0x4caeec` / 588 | **无锚** | — | 清空列表控件 + `load_games` + 重建条目 | 观测 |
//! | `update_replay_list` | 方法体被内联（只剩闭包符号） | `0x523410` / 720 | `:65:107` | 后台线程：逐份 `load_game` 再 `send` 回主线程 | 观测 |
//! | `update_replay_list::{{closure}}` | `0x4cac88` / 372 | 同上（内联在一起） | — | 线程体 | 观测 |
//! | `process` | `0x4cb138` / **6760** | `0x4c6f40` / 8784 | `:100:76` `:170:124` … `:202:133` | 每帧推进播放 | 观测 |
//!
//! `process` 的 x86 ÷ arm64 = **1.30**，**超出** 0.61–1.19 的闸；但两侧各自带着
//! **8 枚完全相同的 `panic::Location`**（`:100:76` `:170:124` `:173:127` `:176:133`
//! `:182:141` `:188:133` `:194:133` `:202:133`）⇒ 归属靠 Location 判等钉死，
//! 不看尺寸。`new` / `update` 在 x86 侧**一枚锚都没有**（它们不 panic），
//! 所以 x86 地址标「无锚」，只有 arm64 符号名这一条证据。
//!
//! ## 2. 三个依赖（逐位，靠被调者符号名）
//!
//! | 被调者 | 出现处 | 含义 |
//! |---|---|---|
//! | `GraphwarReplayReader::load_games` | `update`（`0x4cb058`）、闭包（`0x4cacb4`） | 列出 `user://replays/` 下的文件名 |
//! | `GraphwarReplayReader::load_game` | 闭包（`0x4cacd0`） | 按名字读一份回放（gzip + JSONL） |
//! | `std::sync::mpmc::Sender::send` | 闭包（`0x4cace0`） | 把 `Vec<EventInfo>` 发回主线程 |
//! | `Node::queue_free` | `update`（`0x4caf70`） | 清空旧的列表条目 |
//! | `VecDeque::truncate` / `grow` | `update`（`0x4cb064` / `0x4cb0f4`） | 重建条目队列 |
//!
//! ⇒ **列表加载是异步的**：`update` 先在主线程清空控件、把要读的文件名算出来，
//! 后台线程逐份解析，`process` 每帧从通道里取已解析好的局。
//!
//! ## 3. 四个按钮（逐位，字面量）
//!
//! `.rodata` 里这一段是连着的：`res://replay_entry.tscn` `ReplayName`
//! `AxisModeButton` `FunctionModeButton` `TurnModeButton` `TimeModeButton`
//! `set_button_icon`。⇒ 每个回放条目的场景是 `res://replay_entry.tscn`，
//! 条目的名字标签叫 `ReplayName`，四个模式按钮各一张图标（`set_button_icon`）。
//! 图标资源不在这段里（`axis_option_N` 等在 `texture_loader` 的表里，`docs/16` §29）。
//!
//! ## 4. 未知（**这条路很长，本文只做到「结构 + 依赖 + 按钮」**）
//!
//! * `process` 6760 字节里**播放时钟**的具体形态（`+0x?` 的 tick、速度倍率、
//!   暂停位）没有逐字段解；只知道它引用了 `graphwar_plane.rs:174` / `:179`
//!   的 `panic::Location` ⇒ **会重放几何**（建平面/障碍），与
//!   `src/graphwar_replay_player.rs`（帧解析 + 局面累加）是两层；
//! * `update` 里真正用到的字面量是哪几个（那一串是 `.rodata` 邻居，长度没逐个读）⇒
//!   本文只引用 `new`/闭包里能指到地址的三个依赖，不给 `update` 的字符串清单；
//! * 列表条目 `ReplayEntryInfo` 的字段**未知**（只有 `drop_in_place` 符号，
//!   没有构造点的逐指令证据）。

use crate::graphwar_game_recorder::EventInfo;

/// 一个回放条目（原版 `ReplayEntryInfo`）。
///
/// 字段**未证实**（模块文档 §4）：这里只保留「名字」，因为
/// `GraphwarReplayReader::load_game` 的入参就是名字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayEntryInfo {
    pub name: String,
}

/// 四个模式按钮的控件名（`.rodata` 相邻字面量，逐位）。
pub const MODE_BUTTONS: [&str; 4] = [
    "AxisModeButton",
    "FunctionModeButton",
    "TurnModeButton",
    "TimeModeButton",
];

/// 条目场景（`new`/`update` 用 `instantiate()` 拉这个）。
pub const ENTRY_SCENE: &str = "res://replay_entry.tscn";

/// 条目里放名字的标签名。
pub const ENTRY_NAME_LABEL: &str = "ReplayName";

/// 设图标的方法名（四个模式按钮共用）。
pub const SET_BUTTON_ICON: &str = "set_button_icon";

/// `load_game` 的返回类型别名：一局的全部事件（replay 文件 = gzip JSONL）。
pub type LoadedGame = Vec<EventInfo>;

/// 回放屏控制器。
///
/// 原版是 GDExtension 节点（`GraphwarReplayScreenControl`），字段布局未解
/// （§4）；这里只保留**已观测到的语义**：条目列表、后台通道、播放游标。
#[derive(Debug, Default)]
pub struct GraphwarReplayScreenControl {
    /// 列表里的条目（`update` 重建）。
    pub entries: Vec<ReplayEntryInfo>,
    /// 当前选中的条目下标。
    pub selected: Option<usize>,
    /// 后台线程送回来的、已解析好的局（原版走 `mpmc` 通道，这里用队列等价表达）。
    pub ready: std::collections::VecDeque<LoadedGame>,
    /// 播放游标（tick）。口径同 `src/graphwar_replay_player.rs`：60 tick/秒。
    pub cursor_tick: i64,
    /// 是否在播放。
    pub playing: bool,
    /// 上一帧取到的模式（四个按钮各一个，`Option` = 未按过）。
    pub button_state: [Option<bool>; 4],
}

impl GraphwarReplayScreenControl {
    /// `new`（arm64 `0x4cadfc`）：建 `Rc<RefCell<…>>` 与 `(Sender, Receiver)` 通道。
    ///
    /// 原版在这里 `Rc::new_cyclic_in` + 建通道；通道元素是
    /// `Vec<graphwar_game_recorder::EventInfo>`（符号表里
    /// `mpmc::counter::Counter<…Vec<..graphwar_game_recorder..EventInfo>>`
    /// 的 `drop_in_place` 就是它）。
    pub fn new() -> Self {
        Self::default()
    }

    /// `update`（arm64 `0x4caeec`）：重扫磁盘、重建条目列表。
    ///
    /// `names` 就是 `GraphwarReplayReader::load_games()` 的结果（原版去磁盘读，
    /// 这里由调用方注入，便于测试）。
    ///
    /// 原版顺序：`queue_free` 掉旧条目 → `load_games` → 按名字逐个 instantiate
    /// `res://replay_entry.tscn` 并把名字写进 `ReplayName`。
    pub fn update(&mut self, names: Vec<String>) {
        self.entries.clear();
        for name in names {
            self.entries.push(ReplayEntryInfo { name });
        }
        // 选中项失效就清掉（原版重建列表后同样会重置选择）。
        if let Some(i) = self.selected {
            if i >= self.entries.len() {
                self.selected = None;
            }
        }
    }

    /// `update_replay_list` 的后台半边（arm64 闭包 `0x4cac88` /
    /// x86 `0x523410`，`:65:107`）：逐份解析，把结果 `send` 回去。
    ///
    /// `load` 就是 `GraphwarReplayReader::load_game`（注入，便于测试）。
    pub fn update_replay_list<F>(&mut self, names: &[String], mut load: F)
    where
        F: FnMut(&str) -> LoadedGame,
    {
        for name in names {
            let game = load(name);
            self.ready.push_back(game);
        }
    }

    /// 取一份已解析好的局（对应主线程 `Receiver::try_recv`）。
    pub fn take_ready(&mut self) -> Option<LoadedGame> {
        self.ready.pop_front()
    }

    /// `process`（arm64 `0x4cb138`）：每帧推进播放游标。
    ///
    /// 口径：60 tick/秒（与 `src/graphwar_replay_player.rs` 一致），
    /// `speed` 为倍率。原版的具体时钟字段未解（§4），这里只钉住
    /// **「按 tick 推进」**这一条。
    pub fn process(&mut self, delta_secs: f64) {
        if !self.playing {
            return;
        }
        let step = (delta_secs * 60.0).floor() as i64;
        self.cursor_tick += step.max(0);
    }

    /// 播放到当前游标为止的局面（转发给 replay_player 的口径，本模块不重算几何）。
    pub fn is_playing(&self) -> bool {
        self.playing
    }
}
