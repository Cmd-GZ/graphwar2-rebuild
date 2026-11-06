//! `graphwar_level_manager` —— 原版同名模块的还原（原版 40 个可归属函数）。
//!
//! ## 这个模块在原版里是什么（观测）
//!
//! `fixtures/module_funcs.tsv` 给出 **40 个**带 `core::panic::Location` 锚的函数、
//! 590 个引用点（`docs/coverage-matrix.tsv`），地址全部落在 `0x499740`…`0x4ad3ff`
//! 这一段连续区间里。按调用形态可以把它们分成**四类**，40 个一个不多一个不少：
//!
//! | 类 | 个数 | 地址 | 判据 |
//! |---|---|---|---|
//! | 关卡命令分派 | 1 | `0x499740` | 体内 21 条 `level_<n>` 键比较 + 8 关分支体内联（`docs/10` §6） |
//! | 关卡开场臂 | 13 | 见 [`FUNCTIONS`] | 每个都调 `0x466d20`（关卡开场驱动）+ 一串 `graphwar_mode_manager` 设置器 |
//! | 障碍生成预设闭包（A） | 6 | `0x49e570` `0x49eab0` `0x49f1c0` `0x49f370` `0x4a5e80` `0x4a93e0` | 作为 `.data.rel.ro` 闭包记录挂在 `0x466d20` 的第 3 个实参（`rdx`）上，`+0x20` 槽是生成本体 |
//! | 第二闭包（B） | 20 | 见 [`FUNCTIONS`] | 同一次调用的第 5 个实参（`r8`）；**语义未知**，见下 |
//!
//! 另外 4 个"预设"本体（`0x49fe80` `0x4a12f0` `0x4a2410` `0x4a2da0`）与 1 个第二闭包
//! （`0x49f360`）**没有** Location 锚，因此不在这 40 个里，但它们同样是本模块的代码
//! （`min_count` 立即数就在那 32 字节形参里，见 [`SETUPS`]）。
//!
//! ## 已还原的部分（以及每一格证据在哪）
//!
//! 1. **21 关的 UI 记录**（[`LEVELS`]）—— 来自 `level_info()` 的 21 分支（`0x4ad380`…`0x4af100`）：
//!    `level_<n>` 键、`Level <n>` 标题、`.rodata` 里的描述、以及每关 4 字节模式四元组。
//!    **逐位**：描述串在 `.rodata` 里首尾相接，21 个分支各有一条
//!    `mov qword ptr [rsp + 0x38], <len>`，`<len>` 与串长 21/21 相等；
//!    模式四元组是 21 条 `mov dword ptr [rsp + 0x58], <imm32>`（`0x4ad4dd`…`0x4aefdf`）。
//! 2. **21 关的开局参数**（[`SETUPS`]）—— 障碍预设 / `min_count` / 士兵数 / 目标数 /
//!    Bot 难度 / 回合上限。数据源与复算方式：`tools/campaign_params.py`（字节图案扫描，
//!    每个候选都要对上一个已知操作数），交叉验证是 385 份回放的运行时事件
//!    （`docs/10` §4 的 `R` 列，21/21 一致）。
//! 3. **`campaign.save` 的读写语义**（[`parse_level_key`] / [`decode_save`] /
//!    [`encode_save`] / [`save_action`]）—— 读 `0x418170` → 解析 `0x417ef0`，
//!    写 `0x4183b0`（单调）。**逐位**。
//! 4. **障碍生成的分派**（[`generate_obstacles`]）—— 把"预设"落到
//!    `graphwar_level_generator` 里已逐位复现的两个生产者上。
//!
//! ## 明确没做到的（不许当成"已完成"）
//!
//! * **13 个关卡开场臂的函数体没有逐条还原**。它们的**参数**（本文件的 [`SETUPS`]）是解出来的，
//!   但臂内部对 `graphwar_mode_manager` / `graphwar_turn_manager` / `graphwar_team_manager`
//!   的那 8～10 次设置调用（`0x46a490`/`0x46a770`/`0x46abc0`/`0x46ad30`/`0x46b740`/
//!   `0x46b8b0`/`0x46be70`/`0x46c150`/`0x462d20`/`0x3c17d0`）**每个操作数的含义还没定**；
//!   这些设置器本身属于别的模块，不在本文件范围内。
//! * **第二闭包（20 个）判什么未知，但"返回 bool 的谓词"是观测**。四条形状证据：
//!   ① 每个函数的正常返回路径都只写一个字节 —— `movd eax, xmm2; and al, 1`
//!   （`0x49e7df` / `0x4a933f` …）或 `mov eax, ebx`（`0x4a62aa`），且每个函数只有一处 `ret`；
//!   ② 第 3 关的 B 就是 `mov al, 1; ret`（`0x49f360`，16 字节）—— **恒定 true** 的桩；
//!   ③ 调用方 `0x466d20` 有个上限 5 的循环（`cmp dword ptr [rsp + 0x44], 5` @`0x466e10`），
//!   而 21 关里"士兵 + 目标"的最大值恰好是 5（第 3 关 2+3、第 15/20 关 5+0）；
//!   ④ 无障碍三关的 B 各 560 字节（只调 `0x3c62b0` 坐标换算 ×2），其余 17 个都调
//!   `0x467530`（`path_calculator`），体量 944…1856 字节。
//!   ⇒ **推断**：它是"这个落点/布局能不能接受"的谓词，`0x466d20` 最多重试 5 次；
//!   **缺的证据**是谓词具体比什么（是否撞障碍？离边界多远？），以及返回 false 之后
//!   `0x466d20` 是重新抽随机数还是换实体。
//! * **`level_info` 记录里的 `+0x30` 那个 u32 未知**（`docs/10` §7 第 5 条：`0x442aa0`
//!   之后没有消费者）。本文件**不编造**这个字段。
//! * **模式四元组的第 4 字节（`0x05`）**只按"时间档"记录，判**推断**
//!   （运行时 `TimeModeInfo` 21/21 都是 `TimerInf`，同向但不构成枚举值证据）。
//!
//! ## 文件归属上的两处**未定**（不许当结论）
//!
//! * `level_info()`（`0x4ad380`）**没有** Location 锚 ⇒ 用"panic 锚点归属法"定不出它的模块。
//!   把它放进本文件是**按职责的组织决定**，不是观测结果。
//! * `campaign.save` 那一簇（`0x417ef0` / `0x418170` / `0x4183b0`）同样**没有**锚；
//!   相邻的 `0x418820` 唯一的锚落在 `graphwar_campaign_control.rs`。
//!   ⇒ 这一簇更可能属于 `graphwar_campaign_control.rs`；本轮因为它和"解锁到第几关"绑定、
//!   且只允许改本文件，才临时放在这里。**`graphwar_campaign_control.rs` 落地时必须搬走。**

use crate::graphwar_level_generator::{self, Obstacle};

// ---------------------------------------------------------------------------
// 1. 40 个函数的角色分类
// ---------------------------------------------------------------------------

/// 一个可归属函数在本模块里扮演的角色。
///
/// 四个变体的判据都是**调用形态**（谁调它、它的实参里放了什么），不是函数名猜测。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// 关卡命令分派：`level_<n>` 字符串 → 该关开场臂（`0x499740`，8 关分支体内联在这里）。
    Dispatch,
    /// 一关的开场臂：调 `0x466d20` 并设置模式/队伍/回合。
    StartArm,
    /// 障碍生成预设闭包（`FnMut`），作为 `0x466d20` 的第 3 个实参（`rdx`）。
    ObstaclePreset,
    /// 同一次调用的另一个闭包（第 5 个实参 `r8`）：**返回 bool 的谓词**（观测）。
    /// 判什么未知（推断：落点/布局"能不能接受"），见模块文档。
    SecondClosure,
}

/// 一个可归属函数的登记项。`addr` 是 ELF 虚地址（Ghidra/REA 地址 = 它 + `0x100000`）。
#[derive(Debug, Clone, Copy)]
pub struct FunctionEntry {
    pub addr: u32,
    pub role: Role,
    /// 该函数服务的关卡下标（0 基）。分派函数为 `None`。
    pub level: Option<u8>,
}

/// 本模块的 40 个可归属函数（`fixtures/module_funcs.tsv` 的观测层，地址已逐个核对）。
///
/// 顺序 = 模块内的地址顺序，便于与 `objdump -d` 对照。
pub const FUNCTIONS: [FunctionEntry; 40] = [
    // 1 个分派函数；21 关里 0/1/2/3/10/11/12/13 这 8 关的分支体内联在它里面。
    FunctionEntry { addr: 0x499740, role: Role::Dispatch, level: None },
    FunctionEntry { addr: 0x49e570, role: Role::ObstaclePreset, level: Some(0) },
    FunctionEntry { addr: 0x49e650, role: Role::SecondClosure, level: Some(0) },
    FunctionEntry { addr: 0x49e880, role: Role::SecondClosure, level: Some(1) },
    FunctionEntry { addr: 0x49eab0, role: Role::ObstaclePreset, level: Some(1) },
    FunctionEntry { addr: 0x49eb90, role: Role::SecondClosure, level: Some(2) },
    FunctionEntry { addr: 0x49f1c0, role: Role::ObstaclePreset, level: Some(2) },
    FunctionEntry { addr: 0x49f370, role: Role::ObstaclePreset, level: Some(3) },
    FunctionEntry { addr: 0x49f510, role: Role::StartArm, level: Some(4) },
    FunctionEntry { addr: 0x49fea0, role: Role::SecondClosure, level: Some(4) },
    FunctionEntry { addr: 0x4a04d0, role: Role::StartArm, level: Some(5) },
    FunctionEntry { addr: 0x4a0db0, role: Role::SecondClosure, level: Some(5) },
    FunctionEntry { addr: 0x4a1310, role: Role::StartArm, level: Some(6) },
    FunctionEntry { addr: 0x4a1cd0, role: Role::SecondClosure, level: Some(6) },
    FunctionEntry { addr: 0x4a2430, role: Role::StartArm, level: Some(7) },
    FunctionEntry { addr: 0x4a2dc0, role: Role::SecondClosure, level: Some(7) },
    FunctionEntry { addr: 0x4a33f0, role: Role::StartArm, level: Some(8) },
    FunctionEntry { addr: 0x4a3df0, role: Role::SecondClosure, level: Some(8) },
    FunctionEntry { addr: 0x4a43a0, role: Role::StartArm, level: Some(9) },
    FunctionEntry { addr: 0x4a4c80, role: Role::SecondClosure, level: Some(9) },
    FunctionEntry { addr: 0x4a5390, role: Role::SecondClosure, level: Some(10) },
    FunctionEntry { addr: 0x4a5940, role: Role::SecondClosure, level: Some(11) },
    FunctionEntry { addr: 0x4a5e80, role: Role::ObstaclePreset, level: Some(12) },
    FunctionEntry { addr: 0x4a6020, role: Role::SecondClosure, level: Some(12) },
    FunctionEntry { addr: 0x4a63d0, role: Role::SecondClosure, level: Some(13) },
    FunctionEntry { addr: 0x4a6980, role: Role::StartArm, level: Some(14) },
    FunctionEntry { addr: 0x4a73b0, role: Role::SecondClosure, level: Some(14) },
    FunctionEntry { addr: 0x4a78f0, role: Role::StartArm, level: Some(15) },
    FunctionEntry { addr: 0x4a8320, role: Role::SecondClosure, level: Some(15) },
    FunctionEntry { addr: 0x4a8860, role: Role::StartArm, level: Some(16) },
    FunctionEntry { addr: 0x4a91b0, role: Role::SecondClosure, level: Some(16) },
    FunctionEntry { addr: 0x4a93e0, role: Role::ObstaclePreset, level: Some(16) },
    FunctionEntry { addr: 0x4a94c0, role: Role::StartArm, level: Some(17) },
    FunctionEntry { addr: 0x4a9f00, role: Role::SecondClosure, level: Some(17) },
    FunctionEntry { addr: 0x4aa640, role: Role::StartArm, level: Some(18) },
    FunctionEntry { addr: 0x4aafc0, role: Role::SecondClosure, level: Some(18) },
    FunctionEntry { addr: 0x4ab500, role: Role::StartArm, level: Some(19) },
    FunctionEntry { addr: 0x4abf00, role: Role::SecondClosure, level: Some(19) },
    FunctionEntry { addr: 0x4ac440, role: Role::StartArm, level: Some(20) },
    FunctionEntry { addr: 0x4ace40, role: Role::SecondClosure, level: Some(20) },
];

/// 关卡数：`level_info` 的 21 个分支（另有两处独立证据，见 `docs/10` §5）。
pub const LEVEL_COUNT: usize = 21;

/// 最后一个关卡下标（0 基）＝ 20。`0x418820` 里 `cmp ebp, 0x14` 的立即数。
pub const MAX_LEVEL_INDEX: u32 = 20;

// ---------------------------------------------------------------------------
// 2. 每关的 4 个模式字节（`+0x38..+0x3b`）
// ---------------------------------------------------------------------------

/// `+0x38` 功能模式。名字取自 `docs/88` §7 从 `.rodata` 解出的枚举名；
/// **数字 → 名字**的对应是观测（21 关中 `1` 恰好是第 16–20 关，5/5 与"开始教微分方程"一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionMode {
    NormalFunction = 0,
    DiffEqFunction = 1,
}

/// `+0x39` 回合模式。`1` 恰好是第 11 关（描述明写 "firing simultaneously"）与第 14 关，2/2 一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnMode {
    SequentialTurns = 0,
    SimultaneousTurns = 1,
}

/// `+0x3a` 坐标轴模式。`3/2/1/0 = EveryUnit/EveryFive/OnlyMain/NoAxis` ——
/// **逐位 + 运行时**（385 份回放里 `AxisModeInfo` 的名字与数字 21/21 一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisMode {
    NoAxis = 0,
    OnlyMain = 1,
    EveryFive = 2,
    EveryUnit = 3,
}

/// 一关的 4 个模式字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelModes {
    pub function: FunctionMode,
    pub turns: TurnMode,
    pub axis: AxisMode,
    /// `+0x3b`。21/21 恒为 `5`。
    ///
    /// **推断**：判为"时间档 5"。依据是运行时 `TimeModeInfo` 21/21 都是 `TimerInf`（同向），
    /// 但**没有**"5 == TimerInf 枚举值"的直接证据。所以这里保留原始字节，不写成枚举。
    pub time_byte: u8,
}

/// `+0x3b` 在 21 关里的取值（`mov dword ptr [rsp + 0x58], 0x0503_0000` 的最高字节）。
pub const TIME_BYTE_ALL_LEVELS: u8 = 0x05;

// ---------------------------------------------------------------------------
// 3. 21 关的 UI 记录（`level_info()`，`0x4ad380`）
// ---------------------------------------------------------------------------

/// `level_info()` 的一条记录。
///
/// 原版的记录还有 `+0x30` 一个 u32（部分关是常量、部分是运行期算的）与 `+0x38` 的四字节，
/// 交给 `0x442aa0` 存起来。`+0x30` **没有**已知消费者（`docs/10` §7 第 5 条），故不建模。
#[derive(Debug, Clone, Copy)]
pub struct LevelInfo {
    /// 0 基下标，同时是 `level_<index>` 与 `Level <index>` 里的数字。
    pub index: u8,
    /// 存档/命令分派用的键：`level_0` … `level_20`。
    pub key: &'static str,
    /// 界面标题：`Level 0` … `Level 20`（21/21 逐字节精确）。
    pub title: &'static str,
    /// 关卡描述原文（`.rodata`，未做任何改写；第 16 关的 `writting` 是原版自带的拼写）。
    pub description: &'static str,
    /// 该分支里 `mov qword ptr [rsp + 0x38], <len>` 的立即数。
    /// 它与 `description.len()` 相等是 21/21 的**逐位**检查（见 `tests/`）。
    pub description_len: usize,
    pub modes: LevelModes,
}

const fn modes(function: FunctionMode, turns: TurnMode, axis: AxisMode) -> LevelModes {
    LevelModes {
        function,
        turns,
        axis,
        time_byte: TIME_BYTE_ALL_LEVELS,
    }
}

/// 21 关的 UI 记录，按关卡下标排列。
///
/// 键与标题的构造点逐关可查（`docs/10` §2 的表；两种编码：0–9 两条 `mov dword`，
/// 10–20 一条 `movabs`）。描述串在 `.rodata` 的首地址：`0x1c21b1`、`0x1c21d9`、…、`0x1c277a`
/// （第 9 关例外，在 `0x1bcaa0`）。
pub const LEVELS: [LevelInfo; LEVEL_COUNT] = [
    LevelInfo {
        index: 0,
        key: "level_0",
        title: "Level 0",
        description: "Write a function of x to hit the target.",
        description_len: 40,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 1,
        key: "level_1",
        title: "Level 1",
        description: "A constant is always added to your function. You can see it on the bottom right corner of the screen. It makes sure the function is shot by your soldier.",
        description_len: 153,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 2,
        key: "level_2",
        title: "Level 2",
        description: "There are obstacles in the cartesian plane that don't let your function go through. Try to make a function that goes around them.",
        description_len: 129,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 3,
        key: "level_3",
        title: "Level 3",
        description: "You now have multiple soldiers in your team. At each turn you will shoot from a different one.",
        description_len: 94,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 4,
        key: "level_4",
        title: "Level 4",
        description: "Try going around a plane with more obstacles.",
        description_len: 45,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 5,
        key: "level_5",
        title: "Level 5",
        description: "Play against an opponent that shoots back.",
        description_len: 42,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 6,
        key: "level_6",
        title: "Level 6",
        description: "You must hit these targets in a single shot to destroy them.",
        description_len: 60,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 7,
        key: "level_7",
        title: "Level 7",
        description: "Destroy your target in your first try.",
        description_len: 38,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 8,
        key: "level_8",
        title: "Level 8",
        description: "Hit all targets in a single shot to destroy them.",
        description_len: 49,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 9,
        key: "level_9",
        title: "Level 9",
        description: "Play against a smarter opponent.",
        description_len: 32,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 10,
        key: "level_10",
        title: "Level 10",
        description: "Hit all targets in a single shot to destroy them. The finer grid lines are gone.",
        description_len: 80,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::EveryFive),
    },
    LevelInfo {
        index: 11,
        key: "level_11",
        title: "Level 11",
        description: "Play against an opponent on a plane with fewer grid lines. You and the opponent are also firing simultaneously.",
        description_len: 111,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SimultaneousTurns, AxisMode::EveryFive),
    },
    LevelInfo {
        index: 12,
        key: "level_12",
        title: "Level 12",
        description: "Most grid lines are gone. Try hitting a target.",
        description_len: 47,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::OnlyMain),
    },
    LevelInfo {
        index: 13,
        key: "level_13",
        title: "Level 13",
        description: "Destroy two targets in a single shot.",
        description_len: 37,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::OnlyMain),
    },
    LevelInfo {
        index: 14,
        key: "level_14",
        title: "Level 14",
        description: "Play against an opponent with almost no grids.",
        description_len: 46,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SimultaneousTurns, AxisMode::OnlyMain),
    },
    LevelInfo {
        index: 15,
        key: "level_15",
        title: "Level 15",
        description: "The main axes are gone. The opponent is smarter than ever.",
        description_len: 58,
        modes: modes(FunctionMode::NormalFunction, TurnMode::SequentialTurns, AxisMode::NoAxis),
    },
    LevelInfo {
        index: 16,
        key: "level_16",
        title: "Level 16",
        description: "You are now writting a differential equation instead of a function. Write a differential equation in terms of x and y. Your shot will be the solution to the differential equation using the position of the soldier that shot it as the initial condition.",
        description_len: 251,
        modes: modes(FunctionMode::DiffEqFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 17,
        key: "level_17",
        title: "Level 17",
        description: "You must hit both targets in a single shot with a differential equation to destroy them.",
        description_len: 88,
        modes: modes(FunctionMode::DiffEqFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 18,
        key: "level_18",
        title: "Level 18",
        description: "Fight an opponent with differential equations.",
        description_len: 46,
        modes: modes(FunctionMode::DiffEqFunction, TurnMode::SequentialTurns, AxisMode::EveryUnit),
    },
    LevelInfo {
        index: 19,
        key: "level_19",
        title: "Level 19",
        description: "Fight an opponent with differential equations with less grid lines.",
        description_len: 67,
        modes: modes(FunctionMode::DiffEqFunction, TurnMode::SequentialTurns, AxisMode::OnlyMain),
    },
    LevelInfo {
        index: 20,
        key: "level_20",
        title: "Level 20",
        description: "Fight an opponent with differential equations and no grid lines.",
        description_len: 64,
        modes: modes(FunctionMode::DiffEqFunction, TurnMode::SequentialTurns, AxisMode::NoAxis),
    },
];

/// `level_info()` 的下标访问。越界返回 `None`（原版是 `match`，没有"第 22 关"）。
pub fn level_info(index: usize) -> Option<&'static LevelInfo> {
    LEVELS.get(index)
}

/// 用键（`level_<n>`）反查关卡。原版是字符串 `match`（`0x499740` 里的 7/8 字节比较）。
///
/// ⚠️ 这里**按整串相等**比较：`level_2\n` 不会命中。原版也是逐字节比对，
/// 但它的"7 字节键 / 8 字节键"两条比较链的**完整**等价性我只按 `docs/10` §6 的表核过，
/// 没有把 21 条比较指令逐条映到本函数（标 **观测**）。
pub fn level_by_key(key: &str) -> Option<&'static LevelInfo> {
    LEVELS.iter().find(|l| l.key == key)
}

// ---------------------------------------------------------------------------
// 4. 21 关的开局参数（开场臂 + 闭包表）
// ---------------------------------------------------------------------------

/// 障碍生成预设。三类本体在二进制里是**不同的函数**（`docs/11` §1/§4.5）。
///
/// * [`ObstaclePreset::None`] —— `0x49e570` / `0x49eab0` / `0x4a93e0`：只调
///   `reset_mask`（`0x3f19a0`），**不**放障碍；
/// * [`ObstaclePreset::CentreSingle`] —— `0x49f1c0` / `0x49f370` / `0x4a5e80`：
///   `reset_mask` → `u = (next_u64 >> 11)·2^-53` → `place_obstacle(原点, u*5.0 + 3.0)`；
/// * [`ObstaclePreset::Scatter`] —— 32 字节形参 `mov esi, <min_count>; jmp 0x466c40`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObstaclePreset {
    /// 无障碍（掩膜仍然会被重置）。
    None,
    /// 中心单障碍：位置恒 `(0.0, 0.0)`，半径 `u*5.0 + 3.0 ∈ [3, 8)`。
    CentreSingle,
    /// 随机散布：个数 `trunc(N(0,1)*7 + 15)`，被 `min_count` 抬高（下界，不是定值）。
    Scatter { min_count: i32 },
}

/// 一关的开局参数。
#[derive(Debug, Clone, Copy)]
pub struct LevelSetup {
    /// 该关开场臂的入口（`level_<n>` 分派跳到的地方）。8 关的臂内联在 `0x499740` 里，
    /// 那些下标给的是**臂的起始地址**（不是函数入口，因为它们不是独立函数）。
    pub start_fn: u32,
    /// 障碍预设闭包（`FnMut`）的入口地址。注意：散布式给的是 32 字节形参的地址，
    /// 本体是 `0x466c40`。
    pub preset_fn: u32,
    pub preset: ObstaclePreset,
    /// 第二个闭包（返回 bool 的谓词）的入口地址。判什么**未知**，见模块文档。
    ///
    /// 第 3 关的 `0x49f360` 是 `mov al, 1; ret` —— 恒定 `true` 的桩（16 字节）。
    pub second_fn: u32,
    /// `0x476320(mgr, player, N)` 的 `edx` 立即数 = 该关我方士兵数。
    pub soldiers: u32,
    /// `0x46f310(team)` 的调用次数 = 该关目标数。
    pub targets: u32,
    /// `bot_added_event` 载荷第 3 个 u32 = Bot 难度值；`None` = 该关没有对手。
    ///
    /// 取值 `3/25/50/100/250`。与 `docs/AI.md` 六档表的映射判**推断**，
    /// 且第 12 关（`Level 11`）的 `50` **对不上任何一列** —— `docs/10` §6 记着这个反例，不圆。
    pub bot: Option<u32>,
    /// `turn_limit_info` 载荷 `(1, N)` 的 `N`；`None` = 无上限。
    ///
    /// 载荷第一个 u32 恒为 `1`，含义**未知**（`docs/10` §7 第 14 条），故不建模。
    pub turn_limit: Option<u32>,
}

/// 21 关的开局参数，按关卡下标排列。
///
/// 每一格的来源：
/// * `start_fn` / `preset_fn` / `second_fn`：`.data.rel.ro` 的 21 条闭包记录
///   （`0x95a350`…`0x95deb0`，记录里 `+0x20` 是调用的槽位）；
/// * `preset` + `min_count`：预设闭包本体（见 [`ObstaclePreset`]）；
/// * `soldiers` / `targets` / `bot` / `turn_limit`：开场臂里的立即数与调用次数。
///
/// 复算方式：`python3 tools/campaign_params.py`（需要仓库根目录的原版 `.so`）。
/// 交叉验证：385 份回放的运行时事件，21/21 一致（`docs/10` §4）。
pub const SETUPS: [LevelSetup; LEVEL_COUNT] = [
    // 0：无障碍，1 兵 1 靶，无对手，无回合上限
    LevelSetup {
        start_fn: 0x499957,
        preset_fn: 0x49e570,
        preset: ObstaclePreset::None,
        second_fn: 0x49e650,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: None,
    },
    LevelSetup {
        start_fn: 0x49a3f2,
        preset_fn: 0x49eab0,
        preset: ObstaclePreset::None,
        second_fn: 0x49e880,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(5),
    },
    LevelSetup {
        start_fn: 0x49af3c,
        preset_fn: 0x49f1c0,
        preset: ObstaclePreset::CentreSingle,
        second_fn: 0x49eb90,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(5),
    },
    LevelSetup {
        start_fn: 0x49bb63,
        preset_fn: 0x49f370,
        preset: ObstaclePreset::CentreSingle,
        // 这一关的第二闭包 `0x49f360` 没有 Location 锚，不在 FUNCTIONS 的 40 个里。
        second_fn: 0x49f360,
        soldiers: 2,
        targets: 3,
        bot: None,
        turn_limit: Some(10),
    },
    LevelSetup {
        start_fn: 0x49f510,
        preset_fn: 0x49fe80,
        preset: ObstaclePreset::Scatter { min_count: 15 },
        second_fn: 0x49fea0,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(10),
    },
    LevelSetup {
        start_fn: 0x4a04d0,
        preset_fn: 0x4a12f0,
        preset: ObstaclePreset::Scatter { min_count: 5 },
        second_fn: 0x4a0db0,
        soldiers: 2,
        targets: 0,
        bot: Some(3),
        turn_limit: None,
    },
    LevelSetup {
        start_fn: 0x4a1310,
        preset_fn: 0x4a2410,
        preset: ObstaclePreset::Scatter { min_count: 10 },
        second_fn: 0x4a1cd0,
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(6),
    },
    LevelSetup {
        start_fn: 0x4a2430,
        preset_fn: 0x4a2da0,
        preset: ObstaclePreset::Scatter { min_count: 6 },
        second_fn: 0x4a2dc0,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(1),
    },
    LevelSetup {
        start_fn: 0x4a33f0,
        preset_fn: 0x4a2410,
        preset: ObstaclePreset::Scatter { min_count: 10 },
        second_fn: 0x4a3df0,
        soldiers: 1,
        targets: 3,
        bot: None,
        turn_limit: Some(6),
    },
    LevelSetup {
        start_fn: 0x4a43a0,
        preset_fn: 0x4a12f0,
        preset: ObstaclePreset::Scatter { min_count: 5 },
        second_fn: 0x4a4c80,
        soldiers: 3,
        targets: 0,
        bot: Some(25),
        turn_limit: None,
    },
    LevelSetup {
        start_fn: 0x499e1e,
        preset_fn: 0x4a2410,
        preset: ObstaclePreset::Scatter { min_count: 10 },
        second_fn: 0x4a5390,
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(6),
    },
    LevelSetup {
        start_fn: 0x49a924,
        preset_fn: 0x4a12f0,
        preset: ObstaclePreset::Scatter { min_count: 5 },
        second_fn: 0x4a5940,
        soldiers: 3,
        targets: 0,
        bot: Some(50),
        turn_limit: None,
    },
    LevelSetup {
        start_fn: 0x49b472,
        preset_fn: 0x4a5e80,
        preset: ObstaclePreset::CentreSingle,
        second_fn: 0x4a6020,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(5),
    },
    LevelSetup {
        start_fn: 0x49bf41,
        preset_fn: 0x4a2410,
        preset: ObstaclePreset::Scatter { min_count: 10 },
        second_fn: 0x4a63d0,
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(5),
    },
    LevelSetup {
        start_fn: 0x4a6980,
        preset_fn: 0x4a12f0,
        preset: ObstaclePreset::Scatter { min_count: 5 },
        second_fn: 0x4a73b0,
        soldiers: 4,
        targets: 0,
        bot: Some(100),
        turn_limit: None,
    },
    LevelSetup {
        start_fn: 0x4a78f0,
        preset_fn: 0x4a2410,
        preset: ObstaclePreset::Scatter { min_count: 10 },
        second_fn: 0x4a8320,
        soldiers: 5,
        targets: 0,
        bot: Some(250),
        turn_limit: None,
    },
    LevelSetup {
        start_fn: 0x4a8860,
        preset_fn: 0x4a93e0,
        preset: ObstaclePreset::None,
        second_fn: 0x4a91b0,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: None,
    },
    LevelSetup {
        start_fn: 0x4a94c0,
        preset_fn: 0x4a2410,
        preset: ObstaclePreset::Scatter { min_count: 10 },
        second_fn: 0x4a9f00,
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(6),
    },
    LevelSetup {
        start_fn: 0x4aa640,
        preset_fn: 0x4a2410,
        preset: ObstaclePreset::Scatter { min_count: 10 },
        second_fn: 0x4aafc0,
        soldiers: 3,
        targets: 0,
        bot: Some(100),
        turn_limit: None,
    },
    LevelSetup {
        start_fn: 0x4ab500,
        preset_fn: 0x4a2410,
        preset: ObstaclePreset::Scatter { min_count: 10 },
        second_fn: 0x4abf00,
        soldiers: 4,
        targets: 0,
        bot: Some(100),
        turn_limit: None,
    },
    LevelSetup {
        start_fn: 0x4ac440,
        preset_fn: 0x49fe80,
        preset: ObstaclePreset::Scatter { min_count: 15 },
        second_fn: 0x4ace40,
        soldiers: 5,
        targets: 0,
        bot: Some(250),
        turn_limit: None,
    },
];

/// 一关的开局参数。
pub fn level_setup(index: usize) -> Option<&'static LevelSetup> {
    SETUPS.get(index)
}

// ---------------------------------------------------------------------------
// 5. 障碍生成的分派（预设闭包的等价实现）
// ---------------------------------------------------------------------------

/// 按关卡跑一次障碍生成，等价于 `0x466d20` 调预设闭包（`vtable+0x20`）那一步。
///
/// 三个生产者的公式与常量在 `graphwar_level_generator` 里已逐位复现
/// （`docs/11` §5.2：13/13 个障碍的 `x/y/r` `to_bits()` 全等），
/// 本函数只负责**按关选哪个生产者** —— 这一层也是**逐位**的：
/// 三个 `CentreSingle` 的本体（`0x49f1c0`/`0x49f370`/`0x4a5e80`）算法逐字相同
/// （`reset_mask` → `place_obstacle(原点, u*5.0+3.0)`），四个散布形参的 `esi`
/// 立即数分别是 `15/5/10/6`。
///
/// ⚠️ 原版在这一步（以及散布式本体）里还会 `reset_mask`（`0x3f19a0`）。掩膜属于
/// `graphwar_obstacle_manager`，本函数不做 —— 调用方必须自己重置。
pub fn generate_obstacles(index: usize, next: &mut dyn FnMut() -> u64) -> Option<Vec<Obstacle>> {
    let setup = level_setup(index)?;
    Some(match setup.preset {
        ObstaclePreset::None => Vec::new(),
        ObstaclePreset::CentreSingle => graphwar_level_generator::generate_center(next),
        ObstaclePreset::Scatter { min_count } => graphwar_level_generator::generate(next, min_count),
    })
}

// ---------------------------------------------------------------------------
// 6. `campaign.save`：读 / 解析 / 写的判定
// ---------------------------------------------------------------------------
//
// 三个函数的地址与形状（**逐位**，`objdump -d` 可直接复核）：
//
//   读     `0x418170`  `open("user://campaign.save", 20, mode=1)`（`0x418190`）→ 整文件进 String
//                      → `call 0x417ef0(ptr, len)`（`0x418234`）→ `(ok, value)`；
//                      打开失败走 `0x4182e9` 的 `xor ebx, ebx` ⇒ 返回 `(0, 0)`
//   解析   `0x417ef0`  `split('_')` 取最后一段（分隔符字节在 `0x1bfe27`，长度 1）
//                      → `parse::<u32>()`；失败 `ok = 0`
//   写判定 `0x4183b0`  先读旧值（`0x4183e6` mode=1）再比：`cmp ebp, edx; jbe <不写>`（`0x4185b3`）；
//                      写路径 `0x4185b7` → `open(mode=2)` @`0x41860f` → `store_string` @`0x418647`
//                      → 重新解析刚写的串、`inc edx`、`call 0x418820(self, n+1)` @`0x4186d9`

/// `campaign.save` 的路径常量（`.rodata 0x1bfe28`，长度 `0x14` = 20）。
pub const CAMPAIGN_SAVE_PATH: &str = "user://campaign.save";

/// 解析 `level_<n>`（`0x417ef0` 的等价实现）。
///
/// 语义：取**最后一个** `_` 之后的部分，当十进制 `u32` 解析。
/// 所以 `level_20` → `Some(20)`、`level_0` → `Some(0)`、`anything_7` → `Some(7)`。
///
/// **不宽容** —— 这正是原版的行为，也是 `docs/10` 附录 A 那个实测的解释：
/// `parse::<u32>()` 不跳过空白，所以 `"level_20\n"` 的最后一段是 `"20\n"`，
/// 解析**失败** ⇒ 进度读成 0。写文件时多一个换行，玩家的进度就归零。
pub fn parse_level_key(text: &str) -> Option<u32> {
    text.rsplit('_').next()?.parse::<u32>().ok()
}

/// 读存档（`0x418170` 的等价实现）。
///
/// `content` 为 `None` 表示文件打不开（不存在 / 打不开）。原版这种情况下返回 `(0, 0)`，
/// 这里同样返回 `0`。解析失败也返回 `0` —— **这是原版的选择**，不是"容错"。
pub fn decode_save(content: Option<&str>) -> u32 {
    content.and_then(parse_level_key).unwrap_or(0)
}

/// 生成存档内容（`0x4183b0` 交给 `store_string` 的那个字符串）。
///
/// **没有尾换行**。`FileAccess::store_string` 只写字符串本身、不加换行
/// （`0x3ea720` 的字节里没有 `store_line`，见 `docs/10` §2），
/// 所以这个串的长度必须恰好是 `7`（`level_0`）或 `8`（`level_20`）。
pub fn encode_save(level: u32) -> String {
    format!("level_{level}")
}

/// 写不写？`0x4183b0` 的那段判定，逐条照搬。
///
/// 反汇编（`0x418554`…`0x4185b7`）：
///
/// ```text
/// ok_new, new = parse(要写的串)          // 0x418554 → ebx = ok1, ebp = new
/// ok_old, old = parse(文件现有内容)      // 0x418563 → ecx/eax = ok2, edx = old
/// 0x418570  or (!ok1), (!ok2); je 0x4185b3   // 两个都成功 → 去比较
/// 0x418574  cmp ok2, 1; jne 0x4185b7         // 旧值无效 → 写
/// 0x418579                                    // 旧值有效但新串无效 → 不写
/// 0x4185b3  cmp new, old; jbe 0x418579        // new <= old → 不写
/// 0x4185b7                                    // 写
/// ```
///
/// 两处容易写错的语义：
/// 1. 旧值**解析不出来**（文件不存在、空、或带了换行）⇒ **写**；
/// 2. 新串**解析不出来**但旧值好着 ⇒ **不写**（原版不会用一条坏串覆盖好存档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveAction {
    Write,
    Skip,
}

/// 见 [`SaveAction`]。
pub fn save_action(existing: Option<&str>, new_text: &str) -> SaveAction {
    let new = parse_level_key(new_text);
    let old = existing.and_then(parse_level_key);
    let skip = match (new, old) {
        (Some(n), Some(o)) => n <= o,
        // 新串无效但旧值有效 ⇒ 不写；旧值无效（不论新串如何）⇒ 写
        (None, Some(_)) => true,
        (_, None) => false,
    };
    if skip {
        SaveAction::Skip
    } else {
        SaveAction::Write
    }
}

/// [`save_level`] 的一次调用等价物：该写就返回要落盘的字符串，否则 `None`。
///
/// `existing` 是 `campaign.save` 的当前内容（`None` = 文件不存在）。
pub fn save_level(existing: Option<&str>, level: u32) -> Option<String> {
    let text = encode_save(level);
    match save_action(existing, &text) {
        SaveAction::Write => Some(text),
        SaveAction::Skip => None,
    }
}

/// 写完之后传给刷新函数的下标：`0x4186c8` 重新解析刚写的串、`inc edx`、再
/// `call 0x418820(self, n+1)`（`0x4186d9`）。
///
/// ⇒ 刷新函数收到的是**新值 + 1**，不是新值。`0x418820` 内部有 `cmp ebp, 0x14`（=20）
/// 的钳位，所以 20 之后再 +1 会被夹住。
///
/// ⚠️ `0x418820` 到底把按钮改成什么属性（`disabled`? 换图?）**未证** ——
/// `docs/10` §5 把它判为**推断**。本函数只还原"传什么数"。
pub fn refresh_index(new_level: u32) -> u32 {
    new_level.saturating_add(1).min(MAX_LEVEL_INDEX)
}

/// 解锁判定。
///
/// **推断**：存档里的 `level_<n>` 表示"已解锁到第 n 关"（0 基），据此 `n` 及其之前的关卡
/// 可选、`> n` 的不可选。依据是三条观测："文件里只有这一个数"、"写入单调"、
/// "写完用 `n+1` 去刷新按钮"；**缺**的是 `0x418820` 里那几个节点访问器究竟落到哪个属性
/// （`docs/10` §5）。
pub fn is_unlocked(saved: u32, level: usize) -> bool {
    level as u32 <= saved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_table_is_exactly_the_forty_anchors() {
        // 40 个、地址严格递增、四类都非空 —— 防的是"抄错一行"。
        assert_eq!(FUNCTIONS.len(), 40);
        for w in FUNCTIONS.windows(2) {
            assert!(w[0].addr < w[1].addr, "{:#x} 之后是 {:#x}", w[0].addr, w[1].addr);
        }
        for role in [
            Role::Dispatch,
            Role::StartArm,
            Role::ObstaclePreset,
            Role::SecondClosure,
        ] {
            assert!(FUNCTIONS.iter().any(|f| f.role == role), "{role:?} 一个都没有");
        }
        // 40 = 1 分派 + 13 开场臂 + 6 预设闭包 + 20 第二闭包。
        // 缺口是**有锚才登记**造成的，不是漏：
        //   * 散布式的 4 个预设形参（0x49fe80/0x4a12f0/0x4a2410/0x4a2da0）没有 Location 锚
        //     ⇒ 只有第 0/1/2/3/12/16 关的预设闭包出现在这 40 个里；
        //   * 第 3 关的第二闭包 0x49f360 也没有锚。
        let count = |r: Role| FUNCTIONS.iter().filter(|f| f.role == r).count();
        assert_eq!(count(Role::Dispatch), 1);
        assert_eq!(count(Role::StartArm), 13);
        assert_eq!(count(Role::ObstaclePreset), 6);
        assert_eq!(count(Role::SecondClosure), 20);
        // 预设闭包恰好是"无障碍 3 + 中心单障碍 3"
        let mut preset_levels: Vec<u8> = FUNCTIONS
            .iter()
            .filter(|f| f.role == Role::ObstaclePreset)
            .filter_map(|f| f.level)
            .collect();
        preset_levels.sort_unstable();
        assert_eq!(preset_levels, vec![0, 1, 2, 3, 12, 16]);
        // 第二闭包除第 3 关外每关一个
        let mut closure_levels: Vec<u8> = FUNCTIONS
            .iter()
            .filter(|f| f.role == Role::SecondClosure)
            .filter_map(|f| f.level)
            .collect();
        closure_levels.sort_unstable();
        let expected: Vec<u8> = (0..LEVEL_COUNT as u8).filter(|l| *l != 3).collect();
        assert_eq!(closure_levels, expected);
        // 每个登记项的 level 都与 SETUPS 里对应的地址一致（防"关号抄错"）
        for f in FUNCTIONS.iter().filter_map(|f| f.level.map(|l| (f, l))) {
            let (entry, lvl) = f;
            let setup = &SETUPS[lvl as usize];
            match entry.role {
                Role::StartArm => assert_eq!(entry.addr, setup.start_fn, "关 {lvl} 的开场臂"),
                Role::ObstaclePreset => {
                    assert_eq!(entry.addr, setup.preset_fn, "关 {lvl} 的预设闭包")
                }
                Role::SecondClosure => assert_eq!(entry.addr, setup.second_fn, "关 {lvl} 的第二闭包"),
                Role::Dispatch => unreachable!(),
            }
        }
    }

    /// 每个 `start_fn` 要么就是某个独立函数的入口，要么落在分派函数体内（内联的 8 关）。
    #[test]
    fn start_arms_are_either_standalone_or_inlined_in_the_dispatch() {
        let dispatch_start = 0x499740u32;
        let dispatch_end = 0x49e570u32; // 分派函数之后的第一个函数入口
        for (i, s) in SETUPS.iter().enumerate() {
            let standalone = FUNCTIONS
                .iter()
                .any(|f| f.role == Role::StartArm && f.addr == s.start_fn);
            let inlined = s.start_fn > dispatch_start && s.start_fn < dispatch_end;
            assert!(standalone || inlined, "关卡 {i} 的 start_fn {:#x} 两者都不是", s.start_fn);
        }
        // 独立臂恰好 13 个（21 - 内联的 8 个）
        let standalone = SETUPS
            .iter()
            .filter(|s| {
                FUNCTIONS
                    .iter()
                    .any(|f| f.role == Role::StartArm && f.addr == s.start_fn)
            })
            .count();
        assert_eq!(standalone, 13);
    }

    #[test]
    fn save_round_trip_and_no_trailing_newline() {
        for level in 0..=MAX_LEVEL_INDEX {
            let text = encode_save(level);
            assert_eq!(text, format!("level_{level}"));
            assert!(!text.ends_with('\n'), "存档不许带尾换行");
            assert!(!text.ends_with('\r'));
            assert!(text.len() == 7 || text.len() == 8, "长度只能 7 或 8：{text:?}");
            assert_eq!(decode_save(Some(&text)), level, "往返必须相等");
        }
        assert_eq!(encode_save(0).len(), 7);
        assert_eq!(encode_save(20).len(), 8);
    }

    /// `docs/10` 附录 A 的用户实测：`echo 'level_20' > campaign.save`（9 字节）后进度归零。
    #[test]
    fn trailing_newline_makes_the_progress_zero() {
        assert_eq!(parse_level_key("level_20\n"), None, "\\n 让 parse::<u32>() 失败");
        assert_eq!(decode_save(Some("level_20\n")), 0, "所以读出来是 0，不是 20");
        assert_eq!(decode_save(Some("level_20 ")), 0);
        assert_eq!(decode_save(Some("")), 0);
        assert_eq!(decode_save(None), 0, "文件打不开也是 0");
        // 对照：干净的行才解析得出
        assert_eq!(decode_save(Some("level_20")), 20);
    }

    #[test]
    fn parse_is_last_segment_after_underscore() {
        assert_eq!(parse_level_key("level_0"), Some(0));
        assert_eq!(parse_level_key("level_20"), Some(20));
        assert_eq!(parse_level_key("level_007"), Some(7));
        assert_eq!(parse_level_key("anything_weird_9"), Some(9));
        assert_eq!(parse_level_key("_5"), Some(5));
        assert_eq!(parse_level_key("level_"), None);
        assert_eq!(parse_level_key("level_-1"), None);
        assert_eq!(parse_level_key("level_4294967296"), None, "u32 溢出");
        assert_eq!(
            parse_level_key("level_+3"),
            Some(3),
            "u32 接受前导 +（Rust 标准库行为，原版是同一个 parse）"
        );
    }

    #[test]
    fn write_is_monotonic() {
        // 只增不减
        assert_eq!(save_action(Some("level_3"), "level_5"), SaveAction::Write);
        assert_eq!(save_action(Some("level_5"), "level_5"), SaveAction::Skip);
        assert_eq!(save_action(Some("level_5"), "level_3"), SaveAction::Skip);
        // 文件不存在 / 解析不出来 ⇒ 写
        assert_eq!(save_action(None, "level_0"), SaveAction::Write);
        assert_eq!(save_action(Some(""), "level_0"), SaveAction::Write);
        assert_eq!(save_action(Some("garbage"), "level_0"), SaveAction::Write);
        // 新串坏、旧值好 ⇒ 不写（不用坏串覆盖好存档）
        assert_eq!(save_action(Some("level_3"), "level_9\n"), SaveAction::Skip);
        // save_level 是同一判定的封装
        assert_eq!(save_level(None, 0), Some("level_0".to_string()));
        assert_eq!(save_level(Some("level_7"), 3), None);
    }

    #[test]
    fn refresh_index_is_new_value_plus_one_clamped_to_20() {
        assert_eq!(refresh_index(0), 1);
        assert_eq!(refresh_index(19), 20);
        assert_eq!(refresh_index(20), 20, "cmp ebp, 0x14 的钳位");
        assert_eq!(refresh_index(u32::MAX), MAX_LEVEL_INDEX);
    }

    #[test]
    fn unlock_is_inclusive_and_bounded() {
        assert!(is_unlocked(0, 0));
        assert!(!is_unlocked(0, 1));
        assert!(is_unlocked(20, 20));
        assert!(!is_unlocked(20, 21));
    }
}
