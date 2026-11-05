//! `graphwar_play_screen_control.rs` —— 对局界面控制器（10 个可归属函数）。
//!
//! # 这个模块是什么
//!
//! 原版把界面控制器写在 Rust 里（见 `AGENTS.md` §0 第 4 条），`client/` 下的
//! `screens/play_screen.gd` 是照 `.tscn` 与截图重写的**仿制品**。本文件只放
//! **已被反汇编证据支持**的那部分，解不出的部分显式留空（末尾"未知"一节）。
//!
//! # 地址约定
//!
//! 这里一律用 **ELF 虚地址（vaddr）**，即 `objdump` 打印的地址；
//! Ghidra / REA 地址 = 本文件地址 + `0x100000`（只加一次）。
//!
//! # 证据档位（`AGENTS.md` §0 第 3 条的四档）
//!
//! * **观测** —— 能指到具体指令地址 / 具体 `.rodata` 字符串；
//! * **推断** —— 有依据但没证，写明缺什么；
//! * **未知** —— 没解出来。
//!
//! 本文件**没有**任何"逐位验证"级结论：逐位对拍的对象是采样器/求值器这类纯函数，
//! 界面控制器目前只有静态观测。`cargo test` 全绿只是回归保护，**不构成复原证据**。
//!
//! # 10 个可归属函数（`fixtures/module_funcs.tsv`）
//!
//! 函数边界取自 `analysis/funcstarts_ghidra_va.pkl`；`0x3d9b50 … 0x3dcbb0`
//! 这 8 个函数首尾相接（每个的结束地址 = 下一个的开始地址），互相印证了边界。
//!
//! | vaddr | 大小 | 本文件的记号 | 已解出的角色 | 档位 |
//! |---|---|---|---|---|
//! | `0x3d9000` | `0x3c0` | [`FN_READY_LABELS`] | 取 `VictoryPlayerContainer1` / `SkinIcon`，`set_texture`，写 `f(x) = ` 标签 | 观测 |
//! | `0x3d9b50` | `0x2f0` | [`FN_SET_WIDGETS_A`] | 5 个节点的可见性置 `(0,0,0,1,1)` → **回放视图** | 观测 |
//! | `0x3d9e40` | `0x2f0` | [`FN_SET_WIDGETS_B`] | 同 5 个节点置 `(1,1,1,0,0)` → **对局视图** | 观测 |
//! | `0x3da130` | `0x12b0` | [`FN_SHOW_VICTORY`] | 胜负横幅：`TeamVictoryLabel` / `SelfVictoryLabel` / 头像纹理 | 观测 |
//! | `0x3db3e0` | `0x1090` | — | 被 `0x3da130` 调 3 次，内部逻辑**未解** | 未知 |
//! | `0x3dc470` | `0x1c0` | — | 小整数键的哈希表查找 + 取 `Gd<T>`；被 `0x3dc7f0` / `0x3b0830` 调 | 推断 |
//! | `0x3dc630` | `0x1c0` | — | 同上（另一张表 / 另一个键） | 推断 |
//! | `0x3dc7f0` | `0x3c0` | [`SUBSCRIBED_SIGNALS`] | **订阅**：`function_info_event` + `function_fire_event` | 观测 |
//! | `0x3dcbb0` | `0x2250` | — | 被 `0x3a5760` 调 1 次；含 `f(x) = ` 与 `LeftButtonGrid` 串 | 未知 |
//! | `0x522070` | `0x480` | [`ACTION_NODES`] | 取 `TimerLabel` / `ViewFunctionButton` / … + `player_info_event` | 观测（角色推断） |
//!
//! # 调用关系（直接调用，`analysis/insns.pkl` 全量扫）
//!
//! ```text
//! 0x3a2e10 (root_node) ──→ 0x3dc7f0 ──→ {0x522070, 0x3dc630, 0x3dc470}
//! 0x3a5760 (root_node) ──→ 0x3d9b50 ×4, 0x3da130, 0x3dcbb0
//! 0x3b0830 (root_node) ──→ 0x3d9b50 ×3, 0x3d9e40 ×4, 0x3dc470, 0x3dc630
//! 0x3da130 ──→ 0x3d9000 ×2, 0x3db3e0 ×3
//! ```
//!
//! ⇒ **没有任何一处**把 `play_screen_control` 的函数接进 `function_active_event` /
//! `function_points_event`（见下面 [`SUBSCRIBED_SIGNALS`] 一节）。这对
//! "弹道什么时候清掉"这条问题是否定证据（`docs/14-turn.md` §9.3）。

// ---------------------------------------------------------------------------
// 1. 界面节点名（观测：`.rodata` 里的字面量 + 取节点调用的长度立即数）
// ---------------------------------------------------------------------------

/// 取节点 `/` 上抛（`upcast_ref`）时用的**对局控件**三个节点名。
///
/// 三个都在 `0x3d9b50` 与 `0x3d9e40` 里各被取一次，取完立即调
/// `call *0x9ba978`（GOT 槽，`esi` = 布尔）。档位：**观测**（串与调用点都能指到）。
pub const PLAY_WIDGET_NODES: [&str; 3] = ["LeftButtonGrid", "RightButtonGrid", "FireContainer"];

/// 取节点时用的**回放控件**两个节点名。同上。档位：**观测**。
pub const REPLAY_WIDGET_NODES: [&str; 2] =
    ["LeftReplaysButtonsGrid", "RightReplaysButtonsGrid"];

/// 界面里的其余节点名，各自能指到引用它的函数。档位：**观测**（名字），角色多为**推断**。
pub const OTHER_NODES: [(&str, u64); 8] = [
    ("VictoryPlayerContainer1", FN_READY_LABELS),
    ("VictoryPlayerContainer", FN_SHOW_VICTORY),
    ("SkinIcon", FN_READY_LABELS),
    ("FaceIcon", FN_SHOW_VICTORY),
    ("HatIcon", FN_SHOW_VICTORY),
    ("TeamVictoryLabel", FN_SHOW_VICTORY),
    ("SelfVictoryLabel", FN_SHOW_VICTORY),
    ("ViewFunctionsLabel", FN_CONNECT),
];

/// `0x522070` 里取的那批节点名 + 一个信号名。档位：**观测**（引用），角色**推断**。
pub const ACTION_NODES: [&str; 5] = [
    "TimerLabel",
    "ViewFunctionButton",
    "ShowNames",
    "PlayPauseButton",
    "YButton",
];

/// `0x522070` 引用的信号名（**不在** 15 条远程事件名表里，故单列）。档位：**观测**。
pub const PLAYER_INFO_SIGNAL: &str = "player_info_event";

// ---------------------------------------------------------------------------
// 2. 控件模式（0x3d9b50 / 0x3d9e40 这一对）
// ---------------------------------------------------------------------------

/// 对局 / 回放两种控件布局。
///
/// `0x3d9b50` 与 `0x3d9e40` 是**结构完全相同的孪生函数**：同样按 5 个节点名取节点、
/// 同样上抛、同样调 `*0x9ba978`，唯一的差别是传进去的布尔值**恰好互补**：
///
/// | 节点 | 顺序 | `0x3d9b50` 的 `esi` | `0x3d9e40` 的 `esi` | 指令地址 |
/// |---|---|---|---|---|
/// | `LeftButtonGrid`          | 1 | `0` (`xor %esi,%esi` @`0x3d9be2`) | `1` (`mov $0x1,%esi` @`0x3d9ecf`) | 观测 |
/// | `RightButtonGrid`         | 2 | `0` (@`0x3d9c64`) | `1` (@`0x3d9f51`) | 观测 |
/// | `FireContainer`           | 3 | `0` (@`0x3d9ce6`) | `1` (@`0x3d9fd3`) | 观测 |
/// | `LeftReplaysButtonsGrid`  | 4 | `1` (@`0x3d9d65`) | `0` (`xor`) | 观测 |
/// | `RightReplaysButtonsGrid` | 5 | `1` (@`0x3d9de3`) | `0` (`xor`) | 观测 |
///
/// ⇒ 这两个函数是**互斥的一对**：一个进对局视图，一个进回放视图。
///
/// ⚠️ **推断**：`*0x9ba978` 这个 GOT 槽的具体成员名（`set_visible` / `set_disabled` /
/// `set_process` …）**没有**从二进制确认——只能确认它收一个 `bool` 且作用于刚取到的节点。
/// 缺的证据：`0x9ba978` 指向的 gdext 引擎函数指针的符号（需要运行时读 GOT 或调试）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    /// 正常对局：三个对局控件可见，两个回放控件隐藏。对应 `0x3d9e40`。
    Play,
    /// 看回放：三个对局控件隐藏，两个回放控件可见。对应 `0x3d9b50`。
    Replay,
}

impl UiMode {
    /// 该模式下 5 个节点的布尔值，**顺序与二进制里的调用顺序一致**。
    ///
    /// 顺序是 `PLAY_WIDGET_NODES` 三个再 `REPLAY_WIDGET_NODES` 两个。
    pub fn widget_flags(self) -> [bool; 5] {
        match self {
            // 0x3d9b50: (0,0,0,1,1)
            UiMode::Replay => [false, false, false, true, true],
            // 0x3d9e40: (1,1,1,0,0)
            UiMode::Play => [true, true, true, false, false],
        }
    }

    /// 该模式下 `(节点名, 布尔)`，供直接对拍用。
    pub fn widgets(self) -> [(&'static str, bool); 5] {
        let f = self.widget_flags();
        [
            (PLAY_WIDGET_NODES[0], f[0]),
            (PLAY_WIDGET_NODES[1], f[1]),
            (PLAY_WIDGET_NODES[2], f[2]),
            (REPLAY_WIDGET_NODES[0], f[3]),
            (REPLAY_WIDGET_NODES[1], f[4]),
        ]
    }

    /// 该模式下某个节点应被置成的布尔值；不在 5 个节点里返回 `None`。
    pub fn flag_of(self, node: &str) -> Option<bool> {
        self.widgets()
            .iter()
            .find(|(n, _)| *n == node)
            .map(|(_, v)| *v)
    }
}

/// `0x3d9b50`：把 5 个节点置成 `(0,0,0,1,1)` ⇒ 回放视图。
pub const FN_SET_WIDGETS_A: u64 = 0x3d9b_50;

/// `0x3d9e40`：把 5 个节点置成 `(1,1,1,0,0)` ⇒ 对局视图。
pub const FN_SET_WIDGETS_B: u64 = 0x3d9e_40;

// ---------------------------------------------------------------------------
// 3. 订阅的信号
// ---------------------------------------------------------------------------

/// `0x3dc7f0` 里 `Object::connect`（`call *0x9bb030`，GOT 槽）用的信号名。档位：**观测**。
///
/// 两个信号名都从 `.rodata` 的**同一段连续串**里取，长度立即数都是 `0x13 = 19`：
///
/// | 信号 | 串地址 | `connect` 调用点 |
/// |---|---|---|
/// | `function_info_event` | `0x1bf61a` | `0x3dc98b`（`lea ...%rsi`）→ `0x3dc9a1`（`call *0x9bb030`） |
/// | `function_fire_event` | `0x1bf607` | `0x3dca26` → `0x3dca3c` |
///
/// # 这条为什么重要
///
/// 15 条远程事件名（[`REMOTE_EVENT_NAMES`]）在 `.rodata` 里是**一段连续的字面量**，
/// 整段只被 **`0x3d5260`** 一个函数引用（那是"名字 → 分派"的表，不在本模块）。
/// 本模块内部**只有** `0x3dc7f0` 引用了其中两个名字，而且是以 `connect` 的形式。
///
/// ⇒ 静态上，`play_screen_control` **没有**把自己挂到 `function_active_event` /
/// `function_points_event` 上（直接调用口径；函数指针派发见末尾未知）。
pub const SUBSCRIBED_SIGNALS: [&str; 2] = ["function_info_event", "function_fire_event"];

/// `0x3dc7f0` 取节点的那个名字（`get_node` 的路径实参）。档位：**观测**。
///
/// `0x1bf9a7` 处长 `0x12 = 18`，紧接着就是 panic 位置串
/// `src/graphwar_play_screen_control.rs:561` —— 这也是该模块唯一一条行号信息。
pub const FN_CONNECT: u64 = 0x3d_c7f0;

/// `0x3dc7f0` 用的节点名。
pub const CONNECT_NODE: &str = "ViewFunctionsLabel";

/// 本模块里唯一出现的 panic 位置：`src/graphwar_play_screen_control.rs:561`。
/// 由 `0x3dc7f0` 与 `0x3dcbb0` 引用（`0x1bf9a7` / `0x1bf9a9` 附近）。档位：**观测**。
pub const PANIC_LOCATION: (&str, u32) = ("src/graphwar_play_screen_control.rs", 561);

/// 15 条远程事件名，**按 `.rodata` 里的先后顺序**（`0x1bf5b0` 起）。
///
/// 档位：**观测**（连续字面量，整段只被 `0x3d5260` 引用）。
/// ⚠️ **推断**：这段顺序是本仓库此前踩过坑的那种"串里的位置 ≠ 判别值"
/// （见 `src/events.rs` 头部），所以这里**只记名字与顺序，不声称任何判别值**。
pub const REMOTE_EVENT_NAMES: [&str; 15] = [
    "game_name_info_event",
    "game_state_info_event",
    "function_fire_event",
    "function_info_event",
    "function_active_event",
    "function_points_event",
    "clear_obstacles_event",
    "add_obstacle_event",
    "destroy_obstacle_event",
    "turn_info_event",
    "effect_info_event",
    "axis_mode_info_event",
    "function_mode_info_event",
    "turn_mode_info_event",
    "time_mode_info_event",
];

/// 本模块实际订阅的、属于 [`REMOTE_EVENT_NAMES`] 的那几条。档位：**观测**。
pub fn subscribed_remote_events() -> Vec<&'static str> {
    REMOTE_EVENT_NAMES
        .iter()
        .copied()
        .filter(|n| SUBSCRIBED_SIGNALS.contains(n))
        .collect()
}

// ---------------------------------------------------------------------------
// 4. 胜负横幅（0x3da130）
// ---------------------------------------------------------------------------

/// `0x3da130`：胜负横幅。档位：**观测**（函数地址、引用的串、长度），
/// 具体"什么条件下显示哪一条"**未知**（见 [`VictoryBanner`]）。
pub const FN_SHOW_VICTORY: u64 = 0x3da_130;

/// `0x3d9000`：被 `0x3da130` 调 2 次，取 `VictoryPlayerContainer1` / `SkinIcon`、
/// 调 `set_texture`、写 `f(x) = ` 标签。档位：**观测**（串与调用点）。
pub const FN_READY_LABELS: u64 = 0x3d_9000;

/// 胜负横幅的全部字面量，**按 `.rodata` 里的先后顺序**（`0x1bf91e` 起）。
///
/// 注意 `"YOU lOST"` 里的 `l` 是**小写 L**（`0x19fcc8`），原版就是这么写的，
/// 不是本仓库的笔误。档位：**观测**。
pub const VICTORY_TEXTS: [&str; 6] = [
    "Team 1 Victory!",
    "Team 2 Victory!",
    "YOU WON",
    "YOU DID NOT WIN",
    "YOU lOST",
    "Tie!",
];

/// 横幅文字的选择。
///
/// ⚠️ **这张映射表的"变体 → 字面量"是观测（每个字面量都能指到 `.rodata`），
/// 但"哪种结局走哪个变体"是未知。** 缺的证据：`0x3da130` 里那几处
/// `memcmp`/分支的**输入**（是 `GameStateInfo` 的哪个字段、还是 `TurnCountInfo` 的计数）。
/// 已知的只有：`0x3da130` 是唯一引用这 6 条串的函数，调用者只有 `0x3a5760` 一处。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VictoryBanner {
    /// 横幅显示"1 队获胜"。
    Team1,
    /// 横幅显示"2 队获胜"。
    Team2,
    /// 我方获胜（本地玩家所在队赢）。
    SelfWon,
    /// 我方没赢（本地玩家所在队输，但没说谁赢）。
    SelfDidNotWin,
    /// 我方输了。
    SelfLost,
    /// 平局。
    Tie,
}

impl VictoryBanner {
    /// 该横幅的文字，逐字取自 `.rodata`。
    pub fn text(self) -> &'static str {
        match self {
            VictoryBanner::Team1 => VICTORY_TEXTS[0],
            VictoryBanner::Team2 => VICTORY_TEXTS[1],
            VictoryBanner::SelfWon => VICTORY_TEXTS[2],
            VictoryBanner::SelfDidNotWin => VICTORY_TEXTS[3],
            VictoryBanner::SelfLost => VICTORY_TEXTS[4],
            VictoryBanner::Tie => VICTORY_TEXTS[5],
        }
    }
}

/// `0x3da130` 用到的两个横幅标签节点名。档位：**观测**。
pub const VICTORY_LABELS: [&str; 2] = ["TeamVictoryLabel", "SelfVictoryLabel"];

/// `0x3da130` 用到的三个头像/皮肤图标节点名。档位：**观测**（角色**推断**）。
pub const VICTORY_ICONS: [&str; 3] = ["SkinIcon", "FaceIcon", "HatIcon"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_mode_shows_only_play_widgets() {
        let m = UiMode::Play;
        assert_eq!(m.flag_of("LeftButtonGrid"), Some(true));
        assert_eq!(m.flag_of("RightButtonGrid"), Some(true));
        assert_eq!(m.flag_of("FireContainer"), Some(true));
        assert_eq!(m.flag_of("LeftReplaysButtonsGrid"), Some(false));
        assert_eq!(m.flag_of("RightReplaysButtonsGrid"), Some(false));
        assert_eq!(m.flag_of("NoSuchNode"), None);
    }

    #[test]
    fn subscribed_set_is_exactly_two() {
        assert_eq!(subscribed_remote_events(), vec!["function_fire_event", "function_info_event"]);
        assert!(!SUBSCRIBED_SIGNALS.contains(&"function_active_event"));
        assert!(!SUBSCRIBED_SIGNALS.contains(&"function_points_event"));
    }

    #[test]
    fn panic_location_matches_binary_string() {
        assert_eq!(PANIC_LOCATION.0, "src/graphwar_play_screen_control.rs");
        assert_eq!(PANIC_LOCATION.1, 561);
    }
}
