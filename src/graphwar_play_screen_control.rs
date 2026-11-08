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

// ---------------------------------------------------------------------------
// 6. 弹道生命周期：准备 → 发射 → 飞行 → 命中/越界 → 结算
// ---------------------------------------------------------------------------
//
// # 这一段为什么在 `play_screen_control` 里
//
// 原版**没有**"弹道生命周期"这个函数：它是 `graphwar_game_state_manager::handle_tick`
// （`0x470840`）+ `execute_results`（`0x472470`）合起来的**状态推进**。客户端能观察到的
// 是它投影出来的四件事，本层就把这四件事做成一台显式状态机：
//
// ```text
// ① 哪条命中什么时候落地   execute_results 的死亡块（0x472814）从记录里取出**点序号**
//                           （记录 +0x10），`shl $0x4` 后按 16 字节/点取 `points[idx-1]`
//                           ⇒ 命中是**按弹道点序号**排队的，不是"一次性全落地"
// ② 落地顺序               同一发内按点序号升序（0x472814 的记录是按 index 排的）；
//                           协议侧 106/106 批的死亡块按 `start_tick` 升序（docs/14 §7.3）
// ③ 什么时候算"飞完"       本层取"揭示点数 >= 总点数"（旧 `.gd` 的 `_curve_finished`，
//                           它读的是真 `Line2D` 的点数，不是我们另记的账）
// ④ 结算只发生一次         压平模式（automation）在开火当刻就结算，之后到达事件只做收尾
//                           ⇒ `settled` 这个闩（旧 `.gd` 的 `_logic_settled`）
// ```
//
// # 档位
//
// | 项 | 档位 | 依据 |
// |---|---|---|
// | 命中按**弹道点序号**排队 | **观测（指令级）** | `0x472814`–`0x472872`：`mov 0x10(%rax),%r13` / `shl $0x4` / `movdqu -0x10(%rdx,%rcx,1)` |
// | 死亡效果落在**死者站位** | **观测** | `docs/15` §1.2 的真报文：`DeathEffect.pos == PosInfo` |
// | 死亡块按 tick 升序 | **观测** | `docs/14` §7.3：106/106 批 |
// | 爆炸点 = 弹道停止点 | **观测** | `docs/14` §12.1（503/503）、`docs/31` §2 |
// | "飞完才判胜负" | **推断** | `docs/143` §3 + 用户实测；缺逐帧真样本 |
// | 一点掩膜 / 30 点每 tick | **观测** | `0x472ce6` 的 `imul r13 / shr 0x24` = 除以 30（docs/14 §12.4） |

/// 一发弹道的阶段。
///
/// 对应原版状态机里的两段（`docs/14` §2）：
/// * [`ShotPhase::Idle`] ≈ `WaitingForFunctions`（没有弹道在飞）；
/// * [`ShotPhase::InFlight`] ≈ `ExecutingResults`（弹道在飞，服务端也不收新开火）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotPhase {
    Idle,
    InFlight,
}

/// 命中类别（原版死亡块里 `+0x18` 那个判别字段的两档之一）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillKind {
    /// 士兵阵亡（`LifeQueuedDeath` + `LifeInfo{alive:false}`）。
    Soldier,
    /// 靶子被打掉（`docs/150` §Q1：靶子也是**生命表**里的实体）。
    Target,
}

/// 一条**已计划、未落地**的命中（原版 `LifeQueuedDeath{entity_id, tick}` 的本地对应物）。
///
/// `point` = **弹道点序号**：弹道揭示到这个点，这条命中才落地。这就是 `0x472814`
/// 那个死亡块记录里的 `+0x10` 字段（`shl $0x4` 后当 16 字节/点的下标用）。
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedKill {
    /// 弹道点序号（从 0 起）。
    pub point: usize,
    pub kind: KillKind,
    /// [`KillKind::Soldier`] 时是**士兵下标**；靶子时无意义（`-1`）。
    pub index: i64,
    /// [`KillKind::Target`] 时是**靶子 id**；士兵时无意义（`-1`）。
    pub target_id: i64,
    /// 死者站位 —— **不是**弹道停点（`docs/15` §1.2 观测）。
    pub pos: (f64, f64),
}

impl PlannedKill {
    /// 士兵阵亡。
    pub fn soldier(point: usize, index: i64, pos: (f64, f64)) -> Self {
        PlannedKill { point, kind: KillKind::Soldier, index, target_id: -1, pos }
    }

    /// 靶子被打掉。
    pub fn target(point: usize, target_id: i64, pos: (f64, f64)) -> Self {
        PlannedKill { point, kind: KillKind::Target, index: -1, target_id, pos }
    }
}

/// 推进一帧（或一次结算）时吐出的事件。**顺序即弹道先后**。
#[derive(Debug, Clone, PartialEq)]
pub enum ShotEvent {
    /// 一名士兵阵亡（视图层据此置 `alive=false` + 摆死亡特效）。
    SoldierKilled { point: usize, index: i64, pos: (f64, f64) },
    /// 一个靶子被打掉（视图层据此销毁实体 + 摆火花）。
    TargetKilled { point: usize, target_id: i64, pos: (f64, f64) },
    /// 弹道飞完（视图层据此在**停止点**炸、清曲线、并在 `settle` 为真时收回合）。
    Arrived { stop: (f64, f64) },
}

/// [`ShotLifecycle::settle`] 的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettleOutcome {
    /// 这一次调用**真的**结算了一个回合（`false` = 这一发已经结算过 ⇒ 不重复算）。
    pub settled: bool,
    /// 累计已结算的回合数（`TurnCountInfo.turn_count` 的本地对应物）。
    pub turns_used: i64,
}

/// 一发弹道的生命周期（= 播放屏那 6 个函数的"决定"部分）。
///
/// 视图层只做三件不再判断的事：把数据递进来（[`ShotLifecycle::begin`]）、
/// 把"画到第几个点"递进来（[`ShotLifecycle::advance`]）、执行吐回来的事件。
#[derive(Debug, Clone)]
pub struct ShotLifecycle {
    phase: ShotPhase,
    /// 这条弹道的总点数（`FunctionPoints.points.len()`）。
    total_points: usize,
    /// 已经揭示到的点数（由视图层从真 `Line2D` 读出来喂进来）。
    reveal: usize,
    /// 还没落地的命中，**按点序号升序**。
    pending: std::collections::VecDeque<PlannedKill>,
    /// 弹道停止点（爆炸点，`docs/14` §12.1：503/503 与爆炸点相同）。
    stop: (f64, f64),
    /// 这一发**是否已经结算过**（旧 `.gd` 的 `_logic_settled`）。
    settled: bool,
    turns_used: i64,
    /// 本发已经落地的士兵下标 / 靶子 id（同一发内同一受害者只落一次）。
    hit_soldiers: Vec<i64>,
    hit_targets: Vec<i64>,
    kills_landed: i64,
    targets_landed: i64,
}

impl Default for ShotLifecycle {
    fn default() -> Self {
        ShotLifecycle::new()
    }
}

impl ShotLifecycle {
    /// 空状态：没有弹道在飞、回合数 0。
    pub fn new() -> Self {
        ShotLifecycle {
            phase: ShotPhase::Idle,
            total_points: 0,
            reveal: 0,
            pending: std::collections::VecDeque::new(),
            stop: (0.0, 0.0),
            settled: false,
            turns_used: 0,
            hit_soldiers: Vec::new(),
            hit_targets: Vec::new(),
            kills_landed: 0,
            targets_landed: 0,
        }
    }

    /// 换局 / 退出对局：连回合数一起归零（旧 `.gd` 的 `_reset_shot_state`）。
    pub fn reset(&mut self) {
        *self = ShotLifecycle::new();
    }

    /// 开始放一发。
    ///
    /// * `total_points` —— 这条弹道**画上去的**总点数（截断后的那条，不是采样全量）；
    /// * `kills` / `target_kills` —— 已算好的命中表（顺序无要求，本层按点序号排并合并）；
    /// * `stop` —— 弹道停止点（爆炸点）；
    /// * `already_settled` —— 压平模式（automation）/ 联机：本地不再结算，到达时只收尾。
    pub fn begin(
        &mut self,
        total_points: usize,
        kills: Vec<PlannedKill>,
        target_kills: Vec<PlannedKill>,
        stop: (f64, f64),
        already_settled: bool,
    ) {
        self.phase = ShotPhase::InFlight;
        self.total_points = total_points;
        self.reveal = 0;
        self.stop = stop;
        self.settled = already_settled;
        self.hit_soldiers.clear();
        self.hit_targets.clear();
        self.kills_landed = 0;
        self.targets_landed = 0;
        self.pending = kills.into_iter().chain(target_kills).collect();
        // 按点序号升序 —— 先被打到的先落地（`docs/14` §7.3：106/106 按 `start_tick` 升序）。
        self.pending
            .make_contiguous()
            .sort_by(|a, b| a.point.cmp(&b.point).then(a.kind_index().cmp(&b.kind_index())));
    }

    /// 这一发一共多少点。
    pub fn total_points(&self) -> usize {
        self.total_points
    }

    /// "飞完了" —— 与旧 `.gd` 的 `_curve_finished()` **逐字对应**：
    /// 点数 < 2 直接算完（一条线都没有），否则揭示点数 >= 总点数。
    pub fn arrived(&self) -> bool {
        self.total_points < 2 || self.reveal >= self.total_points
    }

    /// 推进"画到第几个点"，吐回这一帧该执行的事件。
    ///
    /// 已经不在飞（或已飞完）⇒ 空表（幂等，视图层不需要自己判重复）。
    pub fn advance(&mut self, reveal: usize) -> Vec<ShotEvent> {
        let mut out = Vec::new();
        if self.phase != ShotPhase::InFlight {
            return out;
        }
        if reveal > self.reveal {
            self.reveal = reveal;
        }
        // ① 落地"弹道已经走到的那一段"里的命中。
        self.land_until(self.reveal, &mut out);
        // ② 飞完了没有（到达事件只吐一次）。
        if self.arrived() {
            // 到达前先把**剩下的全部**落完 —— 旧 `.gd` 的 `_on_shot_arrived()` 里
            // 那句 `_apply_kills_up_to(1 << 30)`（越界的命中不会因为它排在弹道之外就丢掉）。
            self.reveal = self.reveal.max(self.total_points);
            self.land_until(usize::MAX, &mut out);
            self.phase = ShotPhase::Idle;
            out.push(ShotEvent::Arrived { stop: self.stop });
        }
        out
    }

    /// 把点序号 `<= upto` 的命中落地，按队列顺序（已按点序号升序）。
    fn land_until(&mut self, upto: usize, out: &mut Vec<ShotEvent>) {
        while let Some(k) = self.pending.front() {
            if k.point > upto {
                break;
            }
            let k = self.pending.pop_front().expect("front 已判空");
            // 同一发里同一受害者只落一次（原版 `if alive` 守卫的等价物：
            // 命中表是**开火那一刻**对着存活表算的，所以陈旧只可能来自同一发内部的重复）。
            match k.kind {
                KillKind::Soldier => {
                    if self.hit_soldiers.contains(&k.index) {
                        continue;
                    }
                    self.hit_soldiers.push(k.index);
                    self.kills_landed += 1;
                    out.push(ShotEvent::SoldierKilled { point: k.point, index: k.index, pos: k.pos });
                }
                KillKind::Target => {
                    if self.hit_targets.contains(&k.target_id) {
                        continue;
                    }
                    self.hit_targets.push(k.target_id);
                    self.targets_landed += 1;
                    out.push(ShotEvent::TargetKilled {
                        point: k.point,
                        target_id: k.target_id,
                        pos: k.pos,
                    });
                }
            }
        }
    }

    /// 把在飞的那一发**直接跑完**（旧 `.gd` 的 `flush_shot()`：自检/诊断用）。
    pub fn flush(&mut self) -> Vec<ShotEvent> {
        self.advance(usize::MAX)
    }

    /// 把还没落地的命中**全部**落地，**不吐到达事件**，阶段仍是 InFlight。
    ///
    /// 两条路用它：
    /// * **压平模式**（automation）：开火当刻落地，但曲线照放 —— 到达与收尾仍由
    ///   后面每帧的 [`ShotLifecycle::advance`] 给（旧 `.gd` 的 `_logic_settled` 口径）；
    /// * **整批结算**（同时回合攒齐 / 外部来弹）：那一批里没有单条弹道在飞，
    ///   既没有"到达"也没有爆炸与清线（旧 `.gd` 的 `_resolve_shots()`），
    ///   调用方接 [`ShotLifecycle::abort`] 收尾。
    pub fn land_all(&mut self) -> Vec<ShotEvent> {
        let mut out = Vec::new();
        if self.phase != ShotPhase::InFlight {
            return out;
        }
        self.land_until(usize::MAX, &mut out);
        out
    }

    /// 收一个回合。**同一发只收一次**（`settled` 闩，旧 `.gd` 的 `_logic_settled`）。
    pub fn settle(&mut self) -> SettleOutcome {
        if self.settled {
            return SettleOutcome { settled: false, turns_used: self.turns_used };
        }
        self.settled = true;
        self.turns_used += 1;
        SettleOutcome { settled: true, turns_used: self.turns_used }
    }

    /// 半路丢弃（换局 / 联机断线）。
    pub fn abort(&mut self) {
        self.phase = ShotPhase::Idle;
        self.pending.clear();
    }

    pub fn phase(&self) -> ShotPhase {
        self.phase
    }

    pub fn in_flight(&self) -> bool {
        self.phase == ShotPhase::InFlight
    }

    pub fn revealed(&self) -> usize {
        self.reveal
    }

    /// 还没落地的命中条数（士兵 + 靶子）。
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// 还没落地的**士兵**命中条数（`level_state()["pending_kills"]`）。
    pub fn pending_soldiers(&self) -> usize {
        self.pending.iter().filter(|k| k.kind == KillKind::Soldier).count()
    }

    /// 还没落地的**靶子**命中条数。
    pub fn pending_targets(&self) -> usize {
        self.pending.iter().filter(|k| k.kind == KillKind::Target).count()
    }

    pub fn kills_landed(&self) -> i64 {
        self.kills_landed
    }

    pub fn targets_landed(&self) -> i64 {
        self.targets_landed
    }

    pub fn settled(&self) -> bool {
        self.settled
    }

    pub fn turns_used(&self) -> i64 {
        self.turns_used
    }

    /// 只读快照（自检/诊断）。
    pub fn snapshot(&self) -> LifecycleSnapshot {
        LifecycleSnapshot {
            in_flight: self.in_flight(),
            revealed: self.reveal,
            total_points: self.total_points,
            pending: self.pending.len(),
            kills_landed: self.kills_landed,
            targets_landed: self.targets_landed,
            settled: self.settled,
            turns_used: self.turns_used,
        }
    }
}

impl PlannedKill {
    /// 排序用的类别序号（同点序号时士兵先于靶子 —— 原版死亡块在前、靶子块在后）。
    fn kind_index(&self) -> u8 {
        match self.kind {
            KillKind::Soldier => 0,
            KillKind::Target => 1,
        }
    }
}

/// [`ShotLifecycle::snapshot`] 的只读形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleSnapshot {
    pub in_flight: bool,
    pub revealed: usize,
    pub total_points: usize,
    pub pending: usize,
    pub kills_landed: i64,
    pub targets_landed: i64,
    pub settled: bool,
    pub turns_used: i64,
}

// ---------------------------------------------------------------------------
// 7. 回合时钟：超时谓词（`handle_tick` 的那三个合取项）
// ---------------------------------------------------------------------------
//
// 原版判据（`docs/14` §11.3，`0x470989`–`0x4709ec`，**观测**）：
//
// ```text
// 0x470989  cmp BYTE [self+0x120], 2     ; state == WaitingForFunctions
// call TurnManager::get_end_of_turn_tick (0x462ec0)
// cmp  eax, 1                            ; 取到了截止 tick
// cmp  now_tick, end                     ; now >= end
// cmp  BYTE [self+0x11c], 0              ; "有人在等"标志
//   → call fire_functions (0x470eb0)     ; **把已提交的函数打出去**
// ```
//
// 三个合取项一项都不能省 —— 谓词本体已经落在 [`crate::turnstate::should_auto_fire`]
// （带真值表），本节只负责"回合开始 → 截止 tick → 动作"这一圈**流程**，以及一个
// **我方**加的闩（见 [`TurnClock::decide`]）。

/// 回合时钟的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnAction {
    /// 什么都不做。
    Nothing,
    /// 到点了：按原版把**已提交的函数**打出去（`0x4709ec` 的 `call fire_functions`）。
    AutoFire,
}

/// 一回合的时钟（tick 制，60 Hz；`docs/14` §1.1）。
///
/// 时长来自 `TimeMode` 那张表（`docs/14` §3.3.2 的六档实测），
/// 与 [`crate::graphwar_tick_handler::turn_duration_ticks`] 是**同一张表**。
#[derive(Debug, Clone)]
pub struct TurnClock {
    time_mode: crate::graphwar_mode_manager::TimeMode,
    /// 本回合已经过去的 tick（含不足一 tick 的余量，按 60 Hz 累积）。
    elapsed_ticks: i64,
    tick_remainder: f64,
    /// **我方**加的闩：这一回合已经自动开过火（原版靠 `fire_functions` 改状态自锁，
    /// 我们这边开火可能失败（输入框空 / 轮到 AI），失败时要防每帧重试）。
    auto_fired: bool,
    auto_fired_total: i64,
}

impl Default for TurnClock {
    fn default() -> Self {
        TurnClock::new(5) // TimerInf
    }
}

impl TurnClock {
    /// `time_mode` 是原版那个判别值（`0..=5`，`graphwar_mode_manager::TimeMode`）：
    /// `0 = Timer30s … 4 = Timer5m, 5 = TimerInf`。
    pub fn new(time_mode: u8) -> Self {
        TurnClock {
            time_mode: time_mode_at(time_mode),
            elapsed_ticks: 0,
            tick_remainder: 0.0,
            auto_fired: false,
            auto_fired_total: 0,
        }
    }

    /// 换时间档（换局 / 设置屏改了档位）。**同时重置回合**。
    pub fn set_time_mode(&mut self, time_mode: u8) {
        self.time_mode = time_mode_at(time_mode);
        self.restart();
    }

    /// 回合开始：计时归零、闩清零（旧 `.gd` 的 `_time_left = GwModes.turn_seconds(...)`）。
    pub fn restart(&mut self) {
        self.elapsed_ticks = 0;
        self.tick_remainder = 0.0;
        self.auto_fired = false;
    }

    /// 本回合的时长（tick）；`None` = `TimerInf`，没有截止时刻。
    pub fn turn_duration_ticks(&self) -> Option<i64> {
        self.time_mode.seconds().map(|s| s as i64 * crate::graphwar_tick_handler::TICKS_PER_SECOND)
    }

    /// 回合截止 tick —— 相对"回合开始"的 tick 轴（回合开始时为 0）。
    ///
    /// 原版是绝对 tick（`turn_start_tick + duration`，`docs/14` §3.2）；本层用
    /// "本回合已过 tick" 做基准，两者只差一个常量偏移，**判据不变**。
    pub fn end_of_turn_tick(&self) -> Option<i64> {
        self.turn_duration_ticks()
    }

    /// 推进时钟（按秒，内部换成 60 Hz 的 tick）。返回本回合已过 tick。
    pub fn advance(&mut self, seconds: f64) -> i64 {
        if seconds > 0.0 {
            self.tick_remainder += seconds * crate::graphwar_tick_handler::TICKS_PER_SECOND as f64;
            let whole = self.tick_remainder.floor();
            self.tick_remainder -= whole;
            self.elapsed_ticks += whole as i64;
        }
        self.elapsed_ticks
    }

    /// 倒计时剩几秒（给 `TimerLabel` 排版用；`None` = 无限时档，原版是空白）。
    pub fn seconds_left(&self) -> Option<f64> {
        let total = self.turn_duration_ticks()? as f64;
        Some(((total - self.elapsed_ticks as f64)
            / crate::graphwar_tick_handler::TICKS_PER_SECOND as f64)
            .max(0.0))
    }

    /// 本回合已过 tick。
    pub fn elapsed_ticks(&self) -> i64 {
        self.elapsed_ticks
    }

    /// 判"现在该不该自动开火"。
    ///
    /// `in_flight` / `running` 是**播放屏自己的状态**：本屏没有服务端，状态机靠这两个
    /// 布尔投影（`docs/16-client.md` §53.4 写明了这条投影是**我方**的）：
    ///
    /// ```text
    /// running == false            -> EndingGame
    /// in_flight == true           -> ExecutingResults
    /// 否则                        -> WaitingForFunctions   ← 只有这一档可能超时
    /// waiting = running && !in_flight                  ← 对应 [self+0x11c] != 0
    /// ```
    pub fn decide(&mut self, in_flight: bool, running: bool) -> TurnAction {
        if self.auto_fired {
            return TurnAction::Nothing;
        }
        let state = if !running {
            crate::turnstate::GameState::EndingGame
        } else if in_flight {
            crate::turnstate::GameState::ExecutingResults
        } else {
            crate::turnstate::GameState::WaitingForFunctions
        };
        let waiting = running && !in_flight;
        if crate::turnstate::should_auto_fire(state, self.elapsed_ticks, self.end_of_turn_tick(), waiting)
        {
            self.auto_fired = true;
            self.auto_fired_total += 1;
            TurnAction::AutoFire
        } else {
            TurnAction::Nothing
        }
    }

    /// 累计自动开火次数（诊断）。
    pub fn auto_fired_total(&self) -> i64 {
        self.auto_fired_total
    }

    /// 只读快照。
    pub fn snapshot(&self) -> ClockSnapshot {
        ClockSnapshot {
            time_mode: self.time_mode as u8,
            duration_ticks: self.turn_duration_ticks(),
            elapsed_ticks: self.elapsed_ticks,
            seconds_left: self.seconds_left(),
            auto_fired: self.auto_fired,
            auto_fired_total: self.auto_fired_total,
        }
    }
}

/// [`TurnClock::snapshot`] 的只读形状。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockSnapshot {
    pub time_mode: u8,
    /// `None` = `TimerInf`（没有截止时刻）。
    pub duration_ticks: Option<i64>,
    pub elapsed_ticks: i64,
    pub seconds_left: Option<f64>,
    pub auto_fired: bool,
    pub auto_fired_total: i64,
}

/// 判别值 → `TimeMode`（`0 = Timer30s … 5 = TimerInf`，越界按 `TimerInf`）。
fn time_mode_at(index: u8) -> crate::graphwar_mode_manager::TimeMode {
    use crate::graphwar_mode_manager::TimeMode;
    match index {
        0 => TimeMode::Timer30s,
        1 => TimeMode::Timer1m,
        2 => TimeMode::Timer2m,
        3 => TimeMode::Timer3m,
        4 => TimeMode::Timer5m,
        _ => TimeMode::TimerInf,
    }
}

// ---------------------------------------------------------------------------
// 8. AI 回合闸门（"Bot 什么时候开火"）
// ---------------------------------------------------------------------------
//
// ⚠️ **这一整节都是我方自定**：原版是 `aigen` 进化器（`docs/13`、`docs/147`），
// "AI 什么时候开火"的原版时序**没有任何观测**（`docs/16-client.md` §52.6 #3）。
// 搬到 Rust 的理由不是"更像原版"，而是**判据只能有一份**：现在它从 GDScript 挪过来，
// 与 `GwLevelRules` / `GwTurnDriver` 同处一层，视图层不再有"轮到 AI 了吗"的分支。

/// AI 回合的思考闸门（**我方自定**）。
#[derive(Debug, Clone)]
pub struct BotTurn {
    think_seconds: f64,
    /// 还要想多久；`< 0` = 没在等（人类回合 / 已开火 / 停了）。
    think_left: f64,
}

impl Default for BotTurn {
    fn default() -> Self {
        BotTurn::new(0.6)
    }
}

impl BotTurn {
    /// `think_seconds` 是**自定**的思考时长（旧 `.gd` 的 `BOT_THINK_SECONDS = 0.6`）。
    pub fn new(think_seconds: f64) -> Self {
        BotTurn { think_seconds, think_left: -1.0 }
    }

    /// 轮到 AI：起倒计时。
    pub fn arm(&mut self) {
        self.think_left = self.think_seconds;
    }

    /// 离开 AI 的回合 / 停局：取消倒计时。
    pub fn disarm(&mut self) {
        self.think_left = -1.0;
    }

    pub fn think_seconds(&self) -> f64 {
        self.think_seconds
    }

    pub fn think_left(&self) -> f64 {
        self.think_left
    }

    pub fn armed(&self) -> bool {
        self.think_left >= 0.0
    }

    /// 推进一帧。返回"现在该让它开火了"。
    ///
    /// `can_fire` = 视图层能给出的三个前置（在跑、没有弹道在飞、不在 automation 里；
    /// 旧 `.gd` 的 `_tick_bot_turn` 那三行早退）。
    pub fn tick(&mut self, delta: f64, can_fire: bool) -> bool {
        if !self.armed() || !can_fire {
            return false;
        }
        self.think_left -= delta;
        if self.think_left <= 0.0 {
            self.think_left = -1.0;
            return true;
        }
        false
    }
}

/// AI 一发的函数串：一条过射手、指向最近敌人的直线（**我方自定**，`f(x) = m·x`）。
///
/// 为什么这样能指过去：修正常数 `c = y0 − f(±x0)`（`docs/24` §6.2）把曲线钉在射手身上，
/// 所以 `m = (y敌 − y我)/(x敌 − x我)` 的那条线正好穿过敌人。
///
/// `others` 按**士兵下标顺序**给，取**第一个**活着的敌人（与旧 `.gd` 的循环同序）。
/// 每条是 `(x, y, team, alive)`。
pub fn bot_aim_expression(
    shooter: (f64, f64, i64),
    others: &[(f64, f64, i64, bool)],
) -> String {
    for (x, y, team, alive) in others {
        if !*alive || *team == shooter.2 {
            continue;
        }
        // 射手指的是"自己"那一条 —— 按坐标判等（`is_same` 的等价物；同一位置不会有两个士兵）。
        if (*x - shooter.0).abs() < f64::EPSILON && (*y - shooter.1).abs() < f64::EPSILON {
            continue;
        }
        let dx = *x - shooter.0;
        if dx.abs() < 0.000001 {
            return "0".to_string();
        }
        return format!("{:.6}*x", (*y - shooter.1) / dx);
    }
    "0".to_string()
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

#[cfg(test)]
mod lifecycle_tests {
    //! 弹道生命周期（§6）/ 回合时钟（§7）/ AI 闸门（§8）的纯逻辑断言。
    //!
    //! 这些断言**不碰引擎**，钉的是"什么时候落地 / 什么时候算飞完 / 结算几次"
    //! 这三条流程；`GDExtension` 那一层由 `client/tools/play_flow_check.gd` 覆盖。

    use super::*;

    fn kill_at(point: usize, index: i64) -> PlannedKill {
        PlannedKill::soldier(point, index, (1.0, 2.0))
    }

    /// 命中**按点序号**落地：弹道还没走到那个点，就不许落地。
    #[test]
    fn kills_land_only_when_the_curve_reaches_the_point() {
        let mut lc = ShotLifecycle::new();
        lc.begin(100, vec![kill_at(50, 3), kill_at(10, 7)], vec![], (24.0, 0.5), false);
        assert_eq!(lc.phase(), ShotPhase::InFlight);
        assert_eq!(lc.pending_count(), 2);

        // 走到第 9 点：一条都不落（旧的 10 号点还没到）
        assert!(lc.advance(9).is_empty());
        assert_eq!(lc.kills_landed(), 0);

        // 走到第 10 点：正好落 7 号士兵（边界是 `<=`，不是 `<`）
        let ev = lc.advance(10);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0], ShotEvent::SoldierKilled { point: 10, index: 7, pos: (1.0, 2.0) });
        assert_eq!(lc.kills_landed(), 1);
        assert_eq!(lc.pending_count(), 1);

        // 走到第 49 点：还差一点
        assert!(lc.advance(49).is_empty());
        // 走到第 50 点：落 3 号
        assert_eq!(lc.advance(50).len(), 1);
        assert_eq!(lc.kills_landed(), 2);
        assert_eq!(lc.pending_count(), 0);
    }

    /// 命中表**先排序再落地**：给进来是乱的，落地顺序必须按点序号升序。
    #[test]
    fn kills_are_sorted_by_point_before_landing() {
        let mut lc = ShotLifecycle::new();
        lc.begin(40, vec![kill_at(30, 1), kill_at(10, 2), kill_at(20, 3)], vec![], (5.0, 5.0), false);
        let ev = lc.advance(1000);
        let order: Vec<i64> = ev
            .iter()
            .filter_map(|e| match e {
                ShotEvent::SoldierKilled { index, .. } => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(order, vec![2, 3, 1], "10 → 20 → 30");
    }

    /// "飞完了" = 点数 < 2 或揭示点数 >= 总点数（旧 `.gd` 的 `_curve_finished`）。
    #[test]
    fn arrival_needs_the_whole_curve() {
        let mut lc = ShotLifecycle::new();
        lc.begin(30, vec![], vec![], (24.0, 0.0), false);
        assert!(!lc.arrived());
        let ev = lc.advance(29);
        assert!(ev.is_empty(), "揭示 29/30 ⇒ 还没飞完");
        let ev = lc.advance(30);
        assert_eq!(ev, vec![ShotEvent::Arrived { stop: (24.0, 0.0) }]);
        assert!(!lc.in_flight(), "到达后就离开 InFlight");
        // 幂等：再推一次不再吐事件
        assert!(lc.advance(30).is_empty());
        assert!(lc.flush().is_empty());
    }

    /// 一条线都没有（0/1 点）⇒ 直接算飞完（与旧 `.gd` 的 `_curve_total < 2` 同一条）。
    #[test]
    fn tiny_curve_arrives_immediately() {
        let mut lc = ShotLifecycle::new();
        lc.begin(1, vec![], vec![], (0.0, 0.0), false);
        assert!(lc.arrived());
        assert_eq!(lc.advance(0), vec![ShotEvent::Arrived { stop: (0.0, 0.0) }]);
    }

    /// 到达时把**剩余的**命中一次性落完（`_on_shot_arrived` 的 `_apply_kills_up_to(1<<30)`）。
    #[test]
    fn arrival_lands_the_remaining_kills_first() {
        let mut lc = ShotLifecycle::new();
        lc.begin(5, vec![kill_at(1, 0), kill_at(9, 1)], vec![], (3.0, 3.0), false);
        let ev = lc.advance(5);
        // 第 1 点先落，然后越界的 9 号也落，最后才是到达
        assert_eq!(ev.len(), 3);
        assert_eq!(ev[1], ShotEvent::SoldierKilled { point: 9, index: 1, pos: (1.0, 2.0) });
        assert_eq!(ev[2], ShotEvent::Arrived { stop: (3.0, 3.0) });
        assert_eq!(lc.pending_count(), 0);
    }

    /// 同一发里同一个受害者只落一次（原版 `if alive` 守卫的等价物）。
    #[test]
    fn the_same_victim_lands_once_per_shot() {
        let mut lc = ShotLifecycle::new();
        lc.begin(50, vec![kill_at(5, 2), kill_at(6, 2), kill_at(7, 2)], vec![], (1.0, 1.0), false);
        let ev = lc.advance(50);
        let killed: Vec<i64> = ev
            .iter()
            .filter_map(|e| match e {
                ShotEvent::SoldierKilled { index, .. } => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(killed, vec![2], "2 号士兵只死一次");
        assert_eq!(lc.kills_landed(), 1);
        // 下一发重新算：闩是**每发**清的
        lc.begin(50, vec![kill_at(5, 2)], vec![], (1.0, 1.0), false);
        assert_eq!(lc.advance(50).len(), 2, "新一发里 2 号还能再落一次 + 到达");
    }

    /// 靶子走另一条计数（`targets_landed`），与士兵互不干扰。
    #[test]
    fn targets_count_separately_from_soldiers() {
        let mut lc = ShotLifecycle::new();
        lc.begin(
            20,
            vec![kill_at(3, 0)],
            vec![PlannedKill::target(4, 1000, (12.0, -5.0))],
            (1.0, 1.0),
            false,
        );
        let ev = lc.advance(20);
        assert_eq!(
            ev[0],
            ShotEvent::SoldierKilled { point: 3, index: 0, pos: (1.0, 2.0) },
            "同一点序号时士兵在前"
        );
        assert_eq!(
            ev[1],
            ShotEvent::TargetKilled { point: 4, target_id: 1000, pos: (12.0, -5.0) }
        );
        assert_eq!((lc.kills_landed(), lc.targets_landed()), (1, 1));
    }

    /// 结算**每发只收一次**（压平模式先收，到达时不再收）。
    #[test]
    fn settle_happens_once_per_shot() {
        let mut lc = ShotLifecycle::new();
        lc.begin(4, vec![], vec![], (1.0, 1.0), false);
        let first = lc.settle();
        assert!(first.settled);
        assert_eq!(first.turns_used, 1);
        let second = lc.settle();
        assert!(!second.settled, "同一发不许再算一个回合");
        assert_eq!(second.turns_used, 1);

        // 压平模式：begin(already_settled = true) ⇒ 结算已被消费过
        lc.begin(4, vec![], vec![], (1.0, 1.0), true);
        assert!(!lc.settle().settled);
        assert_eq!(lc.turns_used(), 1);

        // 下一发照常 +1
        lc.begin(4, vec![], vec![], (1.0, 1.0), false);
        assert_eq!(lc.settle().turns_used, 2);
    }

    /// `reset()` 连回合数一起归零（换局）。
    #[test]
    fn reset_clears_everything_including_turns() {
        let mut lc = ShotLifecycle::new();
        lc.begin(10, vec![kill_at(1, 0)], vec![], (1.0, 1.0), false);
        lc.advance(10);
        lc.settle();
        assert_eq!(lc.turns_used(), 1);
        lc.reset();
        assert_eq!(
            lc.snapshot(),
            LifecycleSnapshot {
                in_flight: false,
                revealed: 0,
                total_points: 0,
                pending: 0,
                kills_landed: 0,
                targets_landed: 0,
                settled: false,
                turns_used: 0,
            }
        );
    }

    /// 不在飞时推进是空操作（视图层每帧都调，不许有副作用）。
    #[test]
    fn advancing_while_idle_does_nothing() {
        let mut lc = ShotLifecycle::new();
        assert!(lc.advance(100).is_empty());
        assert!(lc.flush().is_empty());
        assert_eq!(lc.turns_used(), 0);
    }

    // ---- 7. 回合时钟 ----------------------------------------------------

    /// 六档时长与 `TimeMode::seconds()` 同一张表（`docs/14` §3.3.2 全档实测）。
    #[test]
    fn turn_clock_durations_match_the_six_modes() {
        let want = [
            (0u8, Some(1800i64)),
            (1, Some(3600)),
            (2, Some(7200)),
            (3, Some(10800)),
            (4, Some(18000)),
            (5, None), // TimerInf
        ];
        for (index, ticks) in want {
            let c = TurnClock::new(index);
            assert_eq!(c.turn_duration_ticks(), ticks, "档位 {index}");
            assert_eq!(c.end_of_turn_tick(), ticks);
        }
    }

    /// 到点才自动开火；三个合取项（状态 / 时钟 / 有人在等）缺一不可（`0x470989`）。
    #[test]
    fn auto_fire_needs_all_three_conjuncts() {
        let mut c = TurnClock::new(1); // Timer1m = 3600 tick
        c.restart();
        // 没到点
        c.advance(59.9);
        assert_eq!(c.elapsed_ticks(), 3594);
        assert_eq!(c.decide(false, true), TurnAction::Nothing);
        // 到点：到 60.0 s ⇒ 3600 tick
        c.advance(0.1);
        assert_eq!(c.elapsed_ticks(), 3600);
        assert_eq!(c.decide(false, true), TurnAction::AutoFire);
        // 闩：同一回合只给一次
        assert_eq!(c.decide(false, true), TurnAction::Nothing);
        assert_eq!(c.auto_fired_total(), 1);
    }

    /// 弹道在飞（≈ `ExecutingResults`）或局已结束（≈ `EndingGame`）⇒ 都不超时。
    #[test]
    fn auto_fire_is_suppressed_while_in_flight_or_stopped() {
        let mut c = TurnClock::new(0); // Timer30s = 1800 tick
        c.restart();
        c.advance(31.0);
        assert_eq!(c.elapsed_ticks(), 1860);
        assert_eq!(c.decide(true, true), TurnAction::Nothing, "弹道在飞");
        assert_eq!(c.decide(false, false), TurnAction::Nothing, "局已结束");
        assert_eq!(c.decide(false, true), TurnAction::AutoFire, "回到等开火才判");
    }

    /// `TimerInf` 永远不超时（战役关就是这一档：`TimeModeInfo` 21/21 = `TimerInf`）。
    #[test]
    fn timer_inf_never_times_out() {
        let mut c = TurnClock::new(5);
        c.restart();
        c.advance(10_000.0);
        assert_eq!(c.seconds_left(), None, "无限时档没有倒计时（原版标签是空白）");
        assert_eq!(c.decide(false, true), TurnAction::Nothing);
    }

    /// 倒计时是"总时长 − 已过"，且向下夹到 0（`format_clock` 那边向下取整）。
    #[test]
    fn seconds_left_counts_down() {
        let mut c = TurnClock::new(2); // Timer2m
        c.restart();
        assert_eq!(c.seconds_left(), Some(120.0));
        c.advance(1.5);
        assert_eq!(c.seconds_left(), Some(118.5));
        c.advance(200.0);
        assert_eq!(c.seconds_left(), Some(0.0));
    }

    /// 回合开始时闩要清掉（新回合可以再超时一次）。
    #[test]
    fn restart_clears_the_latch() {
        let mut c = TurnClock::new(0);
        c.restart();
        c.advance(30.0);
        assert_eq!(c.decide(false, true), TurnAction::AutoFire);
        c.restart();
        assert_eq!(c.elapsed_ticks(), 0);
        c.advance(30.0);
        assert_eq!(c.decide(false, true), TurnAction::AutoFire);
        assert_eq!(c.auto_fired_total(), 2);
    }

    /// 60 Hz 的帧余量不许丢：60 × (1/60) 秒 = 恰 60 tick。
    #[test]
    fn clock_keeps_fractional_ticks() {
        let mut c = TurnClock::new(3);
        c.restart();
        for _ in 0..60 {
            c.advance(1.0 / 60.0);
        }
        assert_eq!(c.elapsed_ticks(), 60);
    }

    // ---- 8. AI 闸门 ----------------------------------------------------

    /// 思考倒计时：起表 → 到点才放行，且放行一次就把表停掉。
    #[test]
    fn bot_gate_counts_down_then_fires_once() {
        let mut b = BotTurn::new(0.6);
        assert!(!b.armed());
        b.arm();
        assert!(b.armed());
        assert!(!b.tick(0.3, true));
        assert!(b.tick(0.3, true));

        assert!(!b.armed(), "开火后表停掉");
        assert!(!b.tick(1.0, true), "停表后不再吐 true");
    }

    /// `can_fire == false`（弹道在飞 / 停了）时表**不动**。
    #[test]
    fn bot_gate_freezes_when_it_cannot_fire() {
        let mut b = BotTurn::new(0.5);
        b.arm();
        for _ in 0..10 {
            assert!(!b.tick(0.4, false));
        }
        assert!((b.think_left() - 0.5).abs() < 1e-12, "被挡住时倒计时不走");
        assert!(b.tick(0.5, true));
    }

    /// 直线瞄准：`m = (y敌 − y我)/(x敌 − x我)`，6 位小数，形状与旧 `.gd` 逐字符一致。
    #[test]
    fn bot_aim_expression_is_the_same_line() {
        // 射手 (−18,−5.1)，敌人 (18,−5.1) ⇒ m = 0
        assert_eq!(bot_aim_expression((-18.0, -5.1, 0), &[(18.0, -5.1, 1, true)]), "0.000000*x");
        // 射手 (0,0)，敌人 (2,4) ⇒ m = 2
        assert_eq!(bot_aim_expression((0.0, 0.0, 0), &[(2.0, 4.0, 1, true)]), "2.000000*x");
        // 没有敌人 ⇒ "0"（不是 "0.000000*x"：旧 `.gd` 的兜底分支）
        assert_eq!(bot_aim_expression((0.0, 0.0, 0), &[(2.0, 4.0, 0, true)]), "0");
        // 死人不算
        assert_eq!(bot_aim_expression((0.0, 0.0, 0), &[(2.0, 4.0, 1, false)]), "0");
        // 竖直对齐 ⇒ 旧 `.gd` 的 `absf(dx) < 1e-6` 兜底
        assert_eq!(bot_aim_expression((0.0, 0.0, 0), &[(0.0, 9.0, 1, true)]), "0");
    }
}
