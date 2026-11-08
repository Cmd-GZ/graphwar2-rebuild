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

// ---------------------------------------------------------------------------
// 5. 对局流程：**胜负判定**（客户端本地权威）
// ---------------------------------------------------------------------------
//
// # 为什么这一段在 `play_screen_control` 里
//
// 原版的胜负**不在界面控制器里算**：判据是
//
// ```text
// 0x473520  GraphwarGameStateManager::get_alive_teams  -> (Team1 有活体, Team2 有活体)
// 0x4759f0  queue_end_turn_trigger（回合末）
//   0x475a64  cmp turn_count, turn_limit   ; ① 上限用尽
//   0x475a80  test al,1 / je END           ; ② Team1 没人
//   0x475a88  test dl,1 / je END           ; ③ Team2 没人
//   0x475bac  mov BYTE [self+0x120],6      ; EndingGame
// 0x4780c0  set_state(EndingGame) 分支里算胜方字节
//   0x478133..0x478236                     ; -> [self+0x11f]，表见 turnstate::winner_byte
// ```
//
// 本仓库的**真值实现在 `game.rs`**（`Match::ending` / `Match::team_has_survivor`，
// 与上面两处逐行对应），本文件**不重算那条规则**，只把"胜方字节"那张逐位表
// （[`crate::turnstate::winner_byte`]）与两个结束入口的顺序组合成**界面要的一个结论**。
//
// 这一层存在的唯一理由：原版客户端**没有本地权威**——它收服务端的
// `GameStateInfo(EndingGame)` + 胜方字节；我们的客户端没有服务端，所以必须有
// 一个本地权威来说明"这局结束了没有、谁赢"。这段代码就是那个权威，**并且**它的
// 数值来源仍是逐位的那张表，不是我们发明的判据。
//
// # 档位
//
// | 项 | 档位 | 依据 |
// |---|---|---|
// | 胜方字节 `(Team1 活, Team2 活, 设过上限)` -> `0/1/2` | **逐位** | `0x478133`–`0x478236`，见 [`crate::turnstate::winner_byte`] 的表 |
// | 两个结束入口（上限 / 某队没人）与 `EndingGame` 出口 | **逐位** | `0x475a64` / `0x475a80` / `0x475a88` → `0x475bac` |
// | 靶子算 **Team2** 的存活实体（于是"靶子全清" == "Team2 全灭"） | **观测** | `docs/150` §Q1：111 份回放 / 127 个靶子全是 `TeamInfo{Team2}`、0 个 `SoldierInfo` |
// | **哪个入口先判**（本层把 wipe 放前、上限放后） | **我方选择** | 二进制是上限在前（`0x475a64` < `0x475a80`）；**胜方**由存活位算，与入口顺序无关 ⚠️ 见 [`LevelRules::evaluate`] |
// | `mutual_wipe` 时的 `winner_team` | **我方选择** | 真值写字节 `2`（没人赢），界面需要一个队伍号去排版文案 |
// | 每关的 `turn_limit` / 靶子数 / 有没有 Bot | **逐位 + 运行时** | `graphwar_level_manager::SETUPS`（`docs/118` §7.1，21/21 与 385 份回放一致） |

/// 靶子所在的队伍（`docs/150` §Q1）。**观测**：127/127 个靶子都收到 `TeamInfo{Team2}`。
///
/// 队伍编号用**界面侧**的 0 基约定（`0 = Team1`、`1 = Team2`），与
/// `graphwar_turn_manager::Team::One.id()`（1 基）差一 —— 换算在
/// [`crate::turnstate::winner_byte`] 的调用处做一次，别在这条链上重复换。
pub const TARGET_TEAM: i64 = 1;

/// 关卡类型。**只是标签**（`docs/150` §3：判定只有一条，标签给状态行/文档看）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelKind {
    /// 对战关：有对手、无靶子（8 关：5/9/11/14/15/18/19/20）。
    Versus,
    /// 打靶关：有靶子、无对手（13 关）。
    Targets,
    /// 两者都有：21 关数据里不存在，但规则要定义。
    Both,
}

impl LevelKind {
    /// 状态行/文档里的名字（与删掉的 `level_rules.gd` 逐字一致，便于对照旧断言）。
    pub fn as_str(self) -> &'static str {
        match self {
            LevelKind::Versus => "versus",
            LevelKind::Targets => "targets",
            LevelKind::Both => "both",
        }
    }

    fn from_flags(has_targets: bool, has_opponent: bool) -> Self {
        match (has_targets, has_opponent) {
            (true, true) => LevelKind::Both,
            (true, false) => LevelKind::Targets,
            (false, _) => LevelKind::Versus,
        }
    }
}

/// 判一局要用的**输入**（全部由调用方给：界面/回放各自知道自己的局面）。
///
/// 注意它**不含**"谁的回合"：回合与胜负是两件事（`0x4759f0` 只在回合末判）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchState {
    /// 每队**存活士兵**数（键固定 0/1，缺省 0）。
    pub alive_team0: i64,
    pub alive_team1: i64,
    /// **开局时**就有士兵的队伍（含已全灭的队 —— 否则它一灭就从表里消失，判不出"这队没了"）。
    pub teams_present: Vec<i64>,
    /// 靶子集合的规模与存活数（靶子算 [`TARGET_TEAM`] 的存活实体）。
    pub targets_total: i64,
    pub targets_alive: i64,
    /// 已经结算过的回合数（与 `turn_limit` 比）。
    pub turns_used: i64,
}

impl MatchState {
    /// `alive_by_team` 取某个键（缺省 0）。
    pub fn alive_of(&self, team: i64) -> i64 {
        match team {
            0 => self.alive_team0,
            1 => self.alive_team1,
            _ => 0,
        }
    }

    /// 某队**存活实体**数 = 士兵 + （该队是 [`TARGET_TEAM] 时的）存活靶子。
    ///
    /// 这就是 `0x473520` 数的那个量（它枚举**生命表**：士兵与靶子都有 `LifeInfo`，
    /// 玩家实体没有 ⇒ 不数，`docs/150` §Q2）。
    pub fn survivors_of(&self, team: i64) -> i64 {
        let mut n = self.alive_of(team);
        if team == TARGET_TEAM && self.targets_total > 0 {
            n += self.targets_alive;
        }
        n
    }
}

/// 判定结论（界面只读这四项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    pub over: bool,
    pub human_won: bool,
    /// 界面展示用的胜方队伍号（真值"没人赢"时由本层给一个，见 [`LevelRules::evaluate`]）。
    pub winner_team: i64,
    /// **给状态行/自检看的标签**，不参与任何分支。
    pub reason: &'static str,
}

/// 一局的规则（关卡类型 + 人类队伍 + 回合上限）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelRules {
    pub kind: LevelKind,
    pub human_team: i64,
    /// 回合上限；`0` = **无上限**（原版此时压根不发 `TurnLimitInfo`，`docs/123` §5.1 #4）。
    pub turn_limit: i64,
    /// 关卡号（`-1` = 非战役对局，只为状态行/文档可追溯）。
    pub level_index: i64,
    /// 关卡表里写的目标数（只做一致性自检，判定用实际靶子集合）。
    pub targets_planned: i64,
}

impl LevelRules {
    /// 战役关：类型由"有没有靶子 / 有没有对手"两列决定（`docs/118` §7.1，**只是标签**）。
    pub fn for_campaign(
        targets_planned: i64,
        has_bot: bool,
        turn_limit: i64,
        level_index: i64,
        human_team: i64,
    ) -> Self {
        LevelRules {
            kind: LevelKind::from_flags(targets_planned > 0, has_bot),
            human_team,
            turn_limit: turn_limit.max(0),
            level_index,
            targets_planned,
        }
    }

    /// 非战役对局（LAN / 回放）：没有靶子、没有回合上限。
    pub fn for_versus(human_team: i64) -> Self {
        LevelRules {
            kind: LevelKind::Versus,
            human_team,
            turn_limit: 0,
            level_index: -1,
            targets_planned: 0,
        }
    }

    /// 通用构造（类型由"有没有靶子 / 有没有对手"决定，**只是标签**）。
    pub fn for_match(
        has_targets: bool,
        has_opponent: bool,
        turn_limit: i64,
        human_team: i64,
    ) -> Self {
        LevelRules {
            kind: LevelKind::from_flags(has_targets, has_opponent),
            human_team,
            turn_limit: turn_limit.max(0),
            level_index: -1,
            targets_planned: 0,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        self.kind.as_str()
    }

    pub fn has_targets(&self) -> bool {
        matches!(self.kind, LevelKind::Targets | LevelKind::Both)
    }

    pub fn has_opponent_rule(&self) -> bool {
        matches!(self.kind, LevelKind::Versus | LevelKind::Both)
    }

    /// **判一局的胜负。**
    ///
    /// 唯一判据（`0x473520` + `0x4759f0` ②③）：**某队没有存活实体 ⇒ 结束**，
    /// 还有活实体的那队赢；两队都没人 ⇒ 真值写字节 `2`（没人赢）。
    /// 回合上限（`0x475a64`）是**独立的**第二个结束入口，胜方同样由
    /// [`crate::turnstate::winner_byte`] 从存活位算（两边都还有人 + 设过上限 ⇒ Team2）。
    ///
    /// # 与二进制的两处差异（都是**我方选择**，写在这里别当结论）
    ///
    /// 1. **入口顺序**：二进制先判上限（`0x475a64`）、后判 wipe（`0x475a80`/`0x475a88`）；
    ///    本层先判 wipe。**判定结果不变**（胜方由存活位算，与谁先跳无关），
    ///    只有"最后一回合清空靶子"时的 `reason` 标签不同：二进制那条路也会进 END，
    ///    但胜方字节是 `0`（Team1）⇒ 人类照样赢。本层把这个收场标成 `targets_cleared`。
    /// 2. **`mutual_wipe` 的 `winner_team`**：真值写 `2`（没人赢），界面必须有队伍号去
    ///    排版文案，所以本层填"人类以外的那一队"。`human_won` 仍是 `false`。
    pub fn evaluate(&self, st: &MatchState) -> Verdict {
        // 参评的队伍 = 有士兵的队 ∪ 靶子所在的队（靶子有队伍，`docs/150` §Q1）。
        let mut judged: Vec<i64> = Vec::new();
        for t in &st.teams_present {
            if !judged.contains(t) {
                judged.push(*t);
            }
        }
        if st.targets_total > 0 && !judged.contains(&TARGET_TEAM) {
            judged.push(TARGET_TEAM);
        }
        // 什么都没有（例如回放模式没有玩家表）⇒ 明确说"判不了"，**不猜**胜负。
        if judged.is_empty() {
            return Verdict {
                over: false,
                human_won: false,
                winner_team: self.human_team,
                reason: "no_verdict",
            };
        }

        let team1_alive = st.survivors_of(0) > 0;
        let team2_alive = st.survivors_of(1) > 0;
        let wiped: Vec<i64> = judged
            .iter()
            .copied()
            .filter(|t| st.survivors_of(*t) == 0)
            .collect();
        let limit_hit = self.turn_limit > 0 && st.turns_used >= self.turn_limit;

        if wiped.is_empty() && !limit_hit {
            return Verdict {
                over: false,
                human_won: false,
                winner_team: self.human_team,
                reason: "playing",
            };
        }

        // 胜方字节：**逐位**那张表（`0x478133`–`0x478236`）。
        // `turn_limit_set` 是 "set_turn_limit 被调过"（`[+0xc8] == 1`），**不是**"上限用尽"。
        let byte = crate::turnstate::winner_byte(team1_alive, team2_alive, self.turn_limit > 0);
        // 字节 -> 界面队伍号：0 = Team1、1 = Team2、2 = 没人赢。
        let winner_from_byte = match byte {
            0 => Some(0),
            1 => Some(1),
            _ => None,
        };
        let human_won = winner_from_byte == Some(self.human_team) && !wiped.contains(&self.human_team);
        let reason: &'static str = if wiped.contains(&0) && wiped.contains(&1) {
            "mutual_wipe"
        } else if wiped.contains(&self.human_team) {
            "own_wiped"
        } else if !wiped.is_empty() {
            // 对手方（打靶关 = 靶子所在的 Team2）全灭 ⇒ 人类胜（`0x475a88`）。
            // 两队都没有士兵、清空它的只能是靶子 ⇒ 标签换成 targets_cleared。
            // ⚠️ 这是**同一个判据**的两种标签，不是两条规则。
            let w = wiped
                .iter()
                .copied()
                .find(|t| *t != self.human_team)
                .unwrap_or(TARGET_TEAM);
            if w == TARGET_TEAM && st.alive_of(w) == 0 && st.targets_total > 0 {
                "targets_cleared"
            } else {
                "enemy_wiped"
            }
        } else {
            // 上限用尽（`0x475a64`）：两边都还有人 ⇒ 胜方字节写死 `1`（Team2）。
            "turn_limit"
        };

        let winner_team = winner_from_byte.unwrap_or_else(|| {
            // 真值"没人赢"（字节 2）：界面给一个可排版的队伍号（我方选择）。
            if self.human_team == 0 {
                1
            } else {
                0
            }
        });
        Verdict { over: true, human_won, winner_team, reason }
    }
}

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

    // ---- 5. 胜负判定 ----------------------------------------------------

    /// `docs/118` §7.1 的四关：0（1 靶/无上限）、6（2 靶/上限 6）、7（1 靶/上限 1）、
    /// 5（有 Bot/无靶子/无上限）。分类与上限都来自 `graphwar_level_manager::SETUPS`。
    #[test]
    fn campaign_rules_match_level_table() {
        let r0 = LevelRules::for_campaign(1, false, 0, 0, 0);
        assert_eq!(r0.kind_name(), "targets");
        assert_eq!(r0.targets_planned, 1);
        assert_eq!(r0.turn_limit, 0);
        let r6 = LevelRules::for_campaign(2, false, 6, 6, 0);
        assert_eq!(r6.kind_name(), "targets");
        assert_eq!(r6.turn_limit, 6);
        let r7 = LevelRules::for_campaign(1, false, 1, 7, 0);
        assert_eq!(r7.kind_name(), "targets");
        assert_eq!(r7.turn_limit, 1);
        let r5 = LevelRules::for_campaign(0, true, 0, 5, 0);
        assert_eq!(r5.kind_name(), "versus");
        assert_eq!(LevelRules::for_match(true, true, 0, 0).kind_name(), "both");
    }

    /// 打靶关：靶子全清 ⇒ 人类通关（`0x475a88`：Team2 没有活实体）。
    #[test]
    fn targets_cleared_wins() {
        let r = LevelRules::for_campaign(1, false, 0, 0, 0);
        let v = r.evaluate(&MatchState {
            alive_team0: 1,
            teams_present: vec![0],
            targets_total: 1,
            targets_alive: 0,
            turns_used: 1,
            ..Default::default()
        });
        assert!(v.over && v.human_won);
        assert_eq!(v.reason, "targets_cleared");
        // 靶子还在、没到上限 ⇒ 继续（旧实现这里判人类胜，是 bug）
        let v = r.evaluate(&MatchState {
            alive_team0: 1,
            teams_present: vec![0],
            targets_total: 1,
            targets_alive: 1,
            turns_used: 3,
            ..Default::default()
        });
        assert!(!v.over);
        assert_eq!(v.reason, "playing");
    }

    /// 第 8 关（index 7）：上限 1 用尽 ⇒ 判负，胜方 = Team2（`0x475a64` + 胜方字节写死 1）。
    #[test]
    fn turn_limit_loses_with_team2() {
        let r = LevelRules::for_campaign(1, false, 1, 7, 0);
        let v = r.evaluate(&MatchState {
            alive_team0: 1,
            teams_present: vec![0],
            targets_total: 1,
            targets_alive: 1,
            turns_used: 1,
            ..Default::default()
        });
        assert!(v.over && !v.human_won);
        assert_eq!(v.reason, "turn_limit");
        assert_eq!(v.winner_team, 1);
    }

    /// 对战关：对方全灭 ⇒ 通关；我方全灭 ⇒ 失败（`docs/84`：45/45 实测）。
    #[test]
    fn versus_wipes() {
        let r = LevelRules::for_versus(0);
        let v = r.evaluate(&MatchState {
            alive_team0: 2,
            alive_team1: 0,
            teams_present: vec![0, 1],
            turns_used: 3,
            ..Default::default()
        });
        assert!(v.over && v.human_won);
        assert_eq!(v.reason, "enemy_wiped");
        assert_eq!(v.winner_team, 0);
        let v = r.evaluate(&MatchState {
            alive_team0: 0,
            alive_team1: 1,
            teams_present: vec![0, 1],
            turns_used: 3,
            ..Default::default()
        });
        assert!(v.over && !v.human_won);
        assert_eq!(v.reason, "own_wiped");
        assert_eq!(v.winner_team, 1);
    }

    /// 双方都不在场（回放模式没有玩家表）⇒ **判不了**，不能像旧实现那样"0 == 0 ⇒ 有人赢"。
    #[test]
    fn no_verdict_when_nobody_present() {
        let r = LevelRules::for_versus(0);
        let v = r.evaluate(&MatchState { teams_present: vec![], ..Default::default() });
        assert!(!v.over);
        assert_eq!(v.reason, "no_verdict");
    }

    /// 胜方字节 = 真值 `0x478133`–`0x478236` 那张表，本层**没有**第二份实现。
    #[test]
    fn winner_byte_table_is_the_binary_one() {
        use crate::turnstate::winner_byte;
        assert_eq!(winner_byte(false, false, false), 2);
        assert_eq!(winner_byte(false, true, false), 1);
        assert_eq!(winner_byte(true, false, false), 0);
        // 两边都还有人：设过上限 ⇒ Team2（`adc al,1` 那条），否则没人赢。
        assert_eq!(winner_byte(true, true, false), 2);
        assert_eq!(winner_byte(true, true, true), 1);
        // 同一发把两边都清空 ⇒ 字节 2，人类判负（真值里没有样本，`docs/150` §6）。
        let r = LevelRules::for_versus(0);
        let v = r.evaluate(&MatchState {
            alive_team0: 0,
            alive_team1: 0,
            teams_present: vec![0, 1],
            ..Default::default()
        });
        assert!(v.over && !v.human_won);
        assert_eq!(v.reason, "mutual_wipe");
    }

    /// 靶子算 **Team2** 的存活实体（`docs/150` §Q1）⇒ "靶子全清" 与 "Team2 全灭" 是同一条。
    #[test]
    fn targets_count_as_team2_survivors() {
        let st = MatchState {
            alive_team0: 1,
            teams_present: vec![0],
            targets_total: 3,
            targets_alive: 2,
            ..Default::default()
        };
        assert_eq!(st.survivors_of(TARGET_TEAM), 2);
        assert_eq!(st.survivors_of(0), 1);
        // 只有靶子的关：`teams_present` 里没有 Team2，但判据会把它加进来。
        let r = LevelRules::for_campaign(3, false, 0, 8, 0);
        let v = r.evaluate(&st);
        assert!(!v.over);
    }
}
