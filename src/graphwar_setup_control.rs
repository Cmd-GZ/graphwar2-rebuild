//! `graphwar_setup_control` —— 原版同名模块的还原（Godot 类 `GraphwarSetupControl`，
//! 即 `GameSetup`「对局设置」这一屏的 Rust 控件根）。
//!
//! 目标是**还原源码**（`AGENTS.md` §0），不是"写得像"。下面每一条都标
//! **逐位 / 观测 / 推断 / 未知**，并且能指到反汇编地址或 `.rodata` 字节。
//!
//! # 0. 这个模块在原版里做什么
//!
//! 它**不画界面**（界面在 `.tscn` + `.gd` 里），做四件事（全部有地址）：
//!
//! 1. **接线**：`_ready` 级的一次性装配 —— 取 4 个模式按钮 / 房间名输入框 / 锁 / 聊天按钮，
//!    把 `text_changed`、`visibility_changed`、`text_submitted` 连到 Rust 方法，
//!    并订阅 `game_state_info_event` / `chat_message_event`（[`FN_SETUP_WIRING`]）。
//! 2. **重建玩家列表**：Team1 / Team2 / PlayerList 三个容器 + `GameIpLabel`
//!    （[`FN_TEAM_LIST_REBUILD`]），逐行调用下面 4 个"行构件"。
//! 3. **行构件**：名字标签（[`FN_PLAYER_NAME`]）、段位图标（[`FN_RANK_ICON`]）、
//!    士兵图标槽（[`FN_SOLDIER_ICONS`]）、四个按钮（[`FN_PLAYER_ROW`]）。
//! 4. **模式面板同步**：命令总线与列表重建两条路都调 [`FN_MODE_PANEL_SYNC`]。
//!
//! # 1. 函数分类：37 个 Location 锚函数 / 21 个"可归属"
//!
//! `fixtures/module_funcs.tsv` 给本模块 **37** 个带 `core::panic::Location` 锚的函数入口，
//! 对应 **70** 个 `src/graphwar_setup_control.rs` 的 `Location` 结构体
//! （= `docs/coverage-matrix.tsv` 的 `anchors` 列）。
//! 矩阵里 `orig_funcs` 记的是 **21** —— 那是把"同一函数引用两个模块 Location"的跨模块函数
//! （`contested`，8 个）**压平**后的口径；**37 是本模块自己的分母**。
//!
//! 37 这个数是**逐位复核过**的：70 个 Location 的 `(file.ptr, file.len)` 全部通过长度自校验，
//! 引用它们的指令落在 37 个 Ghidra 函数入口里，与 `fixtures/module_funcs.tsv` 的 37 条
//! **逐条相同**（见 [`MODULE_FUNCS`]）。
//!
//! ⚠️ **工具坑（本轮实测）**：`tools/class_probe.py module <x>` 打印的 `funcs:` 列表
//! **不是**真实函数入口 —— 它把 `xrefs.pkl` 里的**指令虚地址**又过了一次 `off2va()`
//! （`.text` 的 `Address = Offset + 0x1000`），于是归属整体偏高 `0x1000`，
//! 拿它去反查会得到"17 个不在 fixture 里、fixture 里 31 个不在它里面"的假象。
//! `fixtures/module_funcs.tsv`（`tools/module_map.py`，直接 `bisect` 虚地址）是对的。
//!
//! 分类结果（`0x…` 全是 **ELF 虚拟地址**；REA/Ghidra 地址 = +`0x100000`，**只加一次**）：
//!
//! | 类 | 函数 | 判据（档位） |
//! |---|---|---|
//! | **本模块逻辑主干** | [`FN_SETUP_WIRING`] `0x4cf670` | 体内 11 个 `get_node` 实参 = 模式按钮 / 房间名 / 锁 / 聊天（**观测**） |
//! | | [`FN_TEAM_LIST_REBUILD`] `0x4d3180` | `Team1Container`/`Team2Container`/`PlayerListContainer`/`GameIpLabel` + `user://local_ip.config`（**观测**） |
//! | | [`FN_PLAYER_ROW`] `0x4c4c20` | `ChangeTeamButton`/`RemovePlayerButton`/`AddSoldierButton`/`RemoveSoldierButton`（**观测**） |
//! | | [`FN_SOLDIER_ICONS`] `0x4c2ef0` | `SoldierIconContainer` → `SkinIcon`/`FaceIcon`/`HatIcon`/`EmptyIcon` + `set_texture`（**观测**） |
//! | | [`FN_PLAYER_NAME`] `0x4c1e40` | `PlayerNameLabel`（**观测**） |
//! | | [`FN_RANK_ICON`] `0x4c2140` | `RankIcon` + `set_texture`（**观测**） |
//! | | [`FN_MODE_PANEL_SYNC`] `0x4c2b90` | **唯一**同时被命令总线 `0x3b0830` 与列表重建 `0x4d3180` 调用的函数（**观测**）；函数体形状与 `set_button_icon` 一致（**推断**） |
//! | **事件/属性回调（职责未知）** | [`FN_EVENT_CALLBACKS`] | 三处都引 `GraphwarRootNode` 类型名 + `a Display implementation returned an error unexpectedly`；调用者都是别的 `impl` 体 ⇒ **推断**是事件/属性回调，**具体哪个事件未知** |
//! | **Godot `impl` 薄壳（语义未知）** | 0x4c5b50 / 0x4c5c00 / 0x4c5cb0 / 0x4c5d60 / 0x4d1ca0 / 0x4d1d50 / 0x4d1e00 / 0x4d1e60 / 0x4d2550 / 0x4d88b0 / 0x4d8a00 / 0x4d8d30 / 0x4d8ff0 / 0x4d9270 / 0x4d9370 / 0x4d9500 / 0x4d95f0 / 0x4d96e0 | 0x60–0x140 字节、体内无字符串、无 `call rel32`（或只调跳板）；`Location` 行号成对重复（263/277/284/270、669/678/687/696/705/728/772/793）⇒ 同一 `impl` 块里成对生成的 `I*` 常量与方法壳（**观测**形状，**语义未知**） |
//! | **Godot 虚方法 / 属性胶水（语义未知）** | 0x4b88d0 / 0x4b8a50 / 0x4bcc60 / 0x4bce80 / 0x4bd030 / 0x4bd280 | 体内唯一的字面量是 `called `Result::unwrap()` on an `Err` value`；只调 gdext 胶水 `0x4da1e0`/`0x4da330`/`0x4da440`/`0x4db030`/`0x4ffaea`；**六个入口地址全都出现在 `.data.rel.ro` 的函数指针槽里**（`0x9bb670`/`0x9bb678` 两个相邻槽，`0x9ba278`/`0x9ba288`/`0x9ba280`/`0x9ba290` 四个）⇒ **观测**：是函数指针表（虚方法/回调）里的实现。**推断**：`Virtual` 方法（`_ready`/`_process`/`_draw`/`_notification`…）之一，**具体哪个未定**（缺 vtable 槽序与 gdext 版本的对齐） |
//! | **`GraphwarSetupControl` 泛型体（contested）** | 0x4d1f10 / 0x4d2600 / 0x4d2f70 | 引 `graphwarrust::graphwar_setup_control::GraphwarSetupControl` 类型名字面量 ⇒ 本类的 `try_from`/`upcast` 内联体（**观测**）；矩阵把 0x4d1f10/0x4d2600 两个记到 `texture_loader`（`contested`） |
//!
//! ⇒ 本文件只实现**四格模式 / 时间档 / 玩家条目 / Start-Cancel** 四类，
//! 每一类都指到地址；`impl` 薄壳与事件回调**不猜**。
//!
//! # 2. 没解出来的（诚实清单）
//!
//! * **模式按钮点下去走哪条路**：[`FN_SETUP_WIRING`] 只连了 3 个信号，**没有**给 4 个模式
//!   按钮连 `pressed`。**推断**它们走按钮的 `input_text` → `root_node` 命令总线
//!   （`docs/16` §15.6）。缺证据：`.tscn` 里这 4 个按钮的 `input_text` + 命令总线那
//!   131 KB 体里对应分支。
//! * **`TimeMode` 判别值 == 图标 id**：图标表（[`TIME_MODE_ICONS`]）的 id 是**逐位**的；
//!   "id 就是判别值"这一步是**推断**，两条同向证据：`AxisMode` 的 4 个 id 与
//!   `AxisModeInfo` 名字序列 4/4 一致；`TimeMode` 6 个名字的数组序 = 6 个图标 id 序。
//! * **`TimerInf` 时计时器标签显示什么**：只观测到有限档的 `mm:ss`（`02:59`/`01:59`/
//!   `00:04`/`00:00`）。无限档**未观测**，所以 [`TimeMode::timer_label`] 返回 `None`。
//! * **LAN 局新玩家的初始士兵数**：**未观测**（战役局走 [`campaign_soldier_count`]，
//!   那是逐位）。`docs/143` 附 1 的教训是"别把 1 当成默认值"。
//! * **`user://local_ip.config` 里那个 `1`**（`0x4d3180` 读它）：`docs/10` §7 第 11 条仍未解。

use crate::graphwar_level_manager::{self, AxisMode, FunctionMode, LevelModes, TurnMode};
use crate::graphwar_root_node::Screen;

// ===========================================================================
// 1. 原版函数清单与地址
// ===========================================================================

/// 本模块在 `fixtures/module_funcs.tsv` 里的 37 个函数入口（ELF 虚地址，升序）。
///
/// 判据：**70** 个 `src/graphwar_setup_control.rs` 的 `panic::Location` 结构体，
/// 引用其 `file.ptr` 槽的指令所在的 Ghidra 函数入口。
///
/// 档位：**逐位**（`slot+8` 的长度字段逐条等于文件名字面量长度；37 条与 fixture 逐条相同）。
/// Ghidra/REA 地址 = 本值 + `0x100000`。
pub const MODULE_FUNCS: [u64; 37] = [
    // 95:1 / 46:10 / 608:1 —— 薄壳 + 内联体
    0x4b_88d0, 0x4b_8a50, 0x4b_cc60, 0x4b_ce80, 0x4b_d030, 0x4b_d280,
    // 178…496 —— 行构件与事件回调
    0x4c_1e40, 0x4c_2140, 0x4c_27f0, 0x4c_2b90, 0x4c_2ef0, 0x4c_44b0, 0x4c_4850, 0x4c_4c20,
    // 263/270/277/284 —— 成对的 impl 薄壳
    0x4c_5b50, 0x4c_5c00, 0x4c_5cb0, 0x4c_5d60,
    // 652…831 —— 接线、列表重建、泛型体
    0x4c_f670, 0x4d_1ca0, 0x4d_1d50, 0x4d_1e00, 0x4d_1e60, 0x4d_1f10, 0x4d_2550, 0x4d_2600,
    0x4d_2f70, 0x4d_3180, 0x4d_88b0, 0x4d_8a00, 0x4d_8d30, 0x4d_8ff0, 0x4d_9270, 0x4d_9370,
    0x4d_9500, 0x4d_95f0, 0x4d_96e0,
];

/// 本模块的 `panic::Location` 锚点数（= `docs/coverage-matrix.tsv` 的 `anchors` 列）。
///
/// 档位：**逐位**。复算：把 `src/graphwar_setup_control.rs` 字面量的每个 `(ptr,len)` 槽
/// 当成 Location 的 `file` 槽，并要求 `slot+8 == len`。
pub const LOCATION_ANCHOR_COUNT: usize = 70;

/// 压平跨模块函数后"可归属本模块"的函数数（矩阵的 `orig_funcs`）。
///
/// 档位：**观测**（`tools/coverage_matrix.py` 的口径：一个函数引用两个模块的 Location 时，
/// 只算给贡献锚点最多的那个；并列取字典序最小）。本模块 37 个里有 8 个 `contested`。
pub const ATTRIBUTED_FUNCS_AFTER_FLATTENING: usize = 21;

/// `0x4cf670`（0x2630 字节）：设置界面的**接线**函数。
///
/// 档位：**观测**。`get_node` 实参（`.rodata` 地址 + 长度立即数）逐条：
/// `RoomNameLineEdit`(`0x19ea20`,16)、`AxisModeButton`(`0x1c2eb2`,14)、
/// `FunctionModeButton`(`0x1c2ec0`,18)、`TurnModeButton`(`0x1c2ed2`,14)、
/// `TurnTimeButton`(`0x1c39b9`,14)、`LockButton`(`0x1c39c7`,10)、
/// `ChatButtonSetup`(`0x1c39d1`,15)、`ChatButtonPlay`(`0x1c39e0`,14)、
/// `ChatPopUp`(`0x1c39ee`,9)、`ChatBox`(`0x1c39f7`,7)、`ChatLineEdit`(`0x1c39fe`,12)。
/// 连信号：`on_text_change`←`text_changed`、`on_visibility_change`←`visibility_changed`、
/// `on_chat_submitted`←`text_submitted`。事件名：`game_state_info_event`、`chat_message_event`。
pub const FN_SETUP_WIRING: u64 = 0x4c_f670;

/// `0x4d3180`（0x3e40 字节）：**玩家/队伍列表重建**。
///
/// 档位：**观测**（`get_node` 到 `Team1Container`/`Team2Container`/`PlayerListContainer`/
/// `GameIpLabel`；读 `user://local_ip.config`；调 4 个行构件各 2–4 次）。
/// ⚠️ 矩阵把它的 21 个 Location 记在 `graphwar_texture_loader.rs`（`contested`）——
/// 归属**有歧义**；本文件只把它当"这一屏的重建体"使用，**不**声称它属于本模块。
pub const FN_TEAM_LIST_REBUILD: u64 = 0x4d_3180;

/// `0x4c4c20`（0xf30 字节）：**一行玩家**的四个按钮
/// （`ChangeTeamButton` / `RemovePlayerButton` / `AddSoldierButton` / `RemoveSoldierButton`）。
/// 档位：**观测**（4 个 `get_node` 实参 + `graphwar_button::GraphwarButton` 上抛）。
pub const FN_PLAYER_ROW: u64 = 0x4c_4c20;

/// `0x4c2ef0`（0x15c0 字节）：一行玩家的**士兵图标槽**
/// （`SoldierIconContainer` → `SkinIcon`/`FaceIcon`/`HatIcon`/`EmptyIcon` + `set_texture`）。
/// 档位：**观测**。5 个槽、每槽 4 个 `TextureRect` 来自场景（`docs/16` §2.1.5）。
pub const FN_SOLDIER_ICONS: u64 = 0x4c_2ef0;

/// `0x4c1e40`（0x300 字节）：玩家名 `PlayerNameLabel`。档位：**观测**。
pub const FN_PLAYER_NAME: u64 = 0x4c_1e40;

/// `0x4c2140`（0x6b0 字节）：段位图标 `RankIcon` + `set_texture`。档位：**观测**。
pub const FN_RANK_ICON: u64 = 0x4c_2140;

/// `0x4c2b90`（0x360 字节）：**模式面板同步**（点模式之后唯一会碰面板的函数）。
///
/// 档位：**观测**（唯一被命令总线 `0x3b0830` 与列表重建 `0x4d3180` 同时调用的函数；
/// 体内两组各 3 次 `0x41ce80`/`0x3ec6f0`/`0x405d20` 序列，形状与 `set_button_icon` 一致）。
/// **推断**：它按当前四格的值刷 4 个按钮的图标。**缺**"哪个枚举 → 哪个 id"的指令级证据。
pub const FN_MODE_PANEL_SYNC: u64 = 0x4c_2b90;

/// 事件/属性回调候选（**职责未知**）：三处都引 `GraphwarRootNode` 类型名与
/// `a Display implementation returned an error unexpectedly`。
pub const FN_EVENT_CALLBACKS: [u64; 3] = [0x4c_27f0, 0x4c_44b0, 0x4c_4850];

/// 图标路径表所在函数 `0x4c9190`（0x6130 字节；矩阵记在 `graphwar_texture_loader.rs`）。
///
/// 本文件**只借用它的数据**（[`AXIS_ICONS`] / [`FUNCTION_MODE_ICONS`] / [`TURN_MODE_ICONS`] /
/// [`TIME_MODE_ICONS`]），**不**声称归属本模块。
///
/// 判据形态（**逐位**）：`lea rdi,<path>` + `mov esi,<len>` + `call 0x4b1c80`；
/// 随后 `mov edx,<id>`（id 为 0 时是 `xor edx,edx`）+ `call 0x43f7c0` 把 `id → 纹理` 存表。
/// 全函数 45 条 `(path, len, id)`，其中 4+2+2+6 条就是四格模式。
pub const FN_ICON_PATH_TABLE: u64 = 0x4c_9190;

// ===========================================================================
// 2. 四格模式的图标表（逐位）与名字表（逐位）
// ===========================================================================

/// `AxisMode` 的四个图标，**下标 = 枚举判别值**（0..3）。
///
/// 档位：**逐位**。`0x4c9190` 里四条 `(path, len, id)`：
/// `0x4c9205`(id 0)、`0x4c928b`(1)、`0x4c9314`(2)、`0x4c939d`(3)；
/// 路径字面量在 `0x1c2efd`/`0x1c2f14`/`0x1c2f2b`/`0x1c2f42`，长度立即数全 `0x17`（23）。
pub const AXIS_ICONS: [&str; 4] = [
    "res://axis_option_0.svg",
    "res://axis_option_1.svg",
    "res://axis_option_2.svg",
    "res://axis_option_3.svg",
];

/// `FunctionMode` 的两个图标（id 0/1）。档位：**逐位**
/// （`0x4c9493` id 0 → `0x1c3873` len 19；`0x4c9519` id 1 → `0x1c3886` len 22）。
pub const FUNCTION_MODE_ICONS: [&str; 2] = ["res://fx_option.svg", "res://dx_dy_option.svg"];

/// `TurnMode` 的两个图标（id 0/1）。档位：**逐位**
/// （`0x4c960f` id 0 → `0x1c329e` len 26；`0x4c9695` id 1 → `0x1c32b8` len 28）。
pub const TURN_MODE_ICONS: [&str; 2] = [
    "res://turns_sequential.svg",
    "res://turns_simultaneous.svg",
];

/// `TimeMode` 的六个图标，**下标 = 判别值**（0..5；5 = 无限）。
///
/// 档位：**逐位**。`0x4c9190` 里六条：
/// `0x4c978b`(0,`0x1c3236`,18)、`0x4c9811`(1,`0x1c3248`,17)、`0x4c989a`(2,`0x1c3259`,17)、
/// `0x4c9923`(3,`0x1c326a`,17)、`0x4c99ac`(4,`0x1c327b`,17)、`0x4c9a35`(5,`0x1c328c`,18)。
/// ⚠️ 注意 0x1c3236 那一条：`strings` 会把 `res://turn_30s.svg` 与前一个字面量粘起来，
/// 只有按长度立即数切才拿到 18 字节。
pub const TIME_MODE_ICONS: [&str; 6] = [
    "res://turn_30s.svg",
    "res://turn_1m.svg",
    "res://turn_2m.svg",
    "res://turn_3m.svg",
    "res://turn_5m.svg",
    "res://turn_inf.svg",
];

/// 模式名的 `&'static str` 表（`.data.rel.ro` `0x9626f0`…`0x9627a0`，
/// 紧接 `src/graphwar_name_manager.rs` 的 `Location` `0x9627b0`）。
///
/// **这是"判别值 → 线上名字"的表**：12 个 `(ptr,len)` 连续排列，
/// 顺序 = `FunctionMode` 2 个、`AxisMode` 4 个、`TimeMode` 6 个。
///
/// 档位：**逐位**（`.rela.dyn` 的 `R_X86_64_RELATIVE` addend 逐条解出，
/// 并与槽 `+8` 的长度字段互相校验）。这些名字同时就是 `*ModeInfo` /
/// `*ModeChangeRequest` 的线上字符串（`docs/09-network.md` 抓包）。
pub const MODE_NAME_TABLE: [&str; 12] = [
    // 0x9626f0 起：FunctionMode 0..1
    "NormalFunction",
    "DiffEqFunction",
    // 0x962710 起：AxisMode 0..3（0x962720 是 `0x19f3a0`，`strings` 会把它和邻居粘起来）
    "NoAxis",
    "OnlyMain",
    "EveryFive",
    "EveryUnit",
    // 0x962750 起：TimeMode 0..5（0x962750 是 `0x19f768`，同理）
    "Timer30s",
    "Timer1m",
    "Timer2m",
    "Timer3m",
    "Timer5m",
    "TimerInf",
];

/// `TurnMode` 的名字表（`.data.rel.ro` `0x957d40`/`0x957d50`，
/// 紧接 `src/graphwar_color_manager.rs` 的 `Location` `0x957d60`）。档位：**逐位**。
pub const TURN_MODE_NAMES: [&str; 2] = ["SequentialTurns", "SimultaneousTurns"];

// ===========================================================================
// 3. `TimeMode`：取值表 / 时长 / 标签 / 循环
// ===========================================================================

/// 回合时限档（`TurnTimeButton`，以及 `JoinLevelPopUp` 里同位置的 `TimeModeButton`）。
///
/// 判别值来源：图标 id（**逐位**）+ 名字表下标（**逐位**）。
/// ⚠️ "图标 id == 判别值"这一步是**推断**，两条同向证据：
/// `AxisMode` 的 4 个 id 与运行时 `AxisModeInfo` 的名字序列 4/4 一致；
/// 21 关的 `LevelModes::time_byte` 恒为 5，而运行时 `TimeModeInfo` 21/21 是 `TimerInf`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TimeMode {
    /// 30 秒（`res://turn_30s.svg`）。
    Timer30s = 0,
    /// 1 分钟（`res://turn_1m.svg`）。
    Timer1m = 1,
    /// 2 分钟（`res://turn_2m.svg`）。
    Timer2m = 2,
    /// 3 分钟（`res://turn_3m.svg`）。
    Timer3m = 3,
    /// 5 分钟（`res://turn_5m.svg`）。
    Timer5m = 4,
    /// 无限（`res://turn_inf.svg`）。线上 `end_of_turn_tick` 发 `0`。
    TimerInf = 5,
}

/// 六档，按判别值升序（= 点击循环的顺序）。
pub const ALL_TIME_MODES: [TimeMode; 6] = [
    TimeMode::Timer30s,
    TimeMode::Timer1m,
    TimeMode::Timer2m,
    TimeMode::Timer3m,
    TimeMode::Timer5m,
    TimeMode::TimerInf,
];

impl TimeMode {
    /// 全部六档，按判别值升序。
    pub const ALL: [TimeMode; 6] = ALL_TIME_MODES;

    /// 由判别值构造；越界返回 `None`（原版 `match` 没有别的分支）。
    pub const fn from_u8(v: u8) -> Option<TimeMode> {
        Some(match v {
            0 => TimeMode::Timer30s,
            1 => TimeMode::Timer1m,
            2 => TimeMode::Timer2m,
            3 => TimeMode::Timer3m,
            4 => TimeMode::Timer5m,
            5 => TimeMode::TimerInf,
            _ => return None,
        })
    }

    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// 线上名字 / 名字表项（`"Timer30s"`…`"TimerInf"`）。档位：**逐位**（[`MODE_NAME_TABLE`]）。
    pub const fn name(self) -> &'static str {
        MODE_NAME_TABLE[6 + self as usize]
    }

    /// 按钮图标路径。档位：**逐位**（[`TIME_MODE_ICONS`]）。
    pub const fn icon_path(self) -> &'static str {
        TIME_MODE_ICONS[self as usize]
    }

    /// 一回合的时长（秒）；`TimerInf` 没有截止时刻 ⇒ `None`。
    ///
    /// 档位：**观测**（真实抓包，`docs/42` / `src/graphwar_tick_handler.rs`：
    /// `Timer30s/1m/2m/3m/5m` = 1800/3600/7200/10800/18000 tick；
    /// `TimerInf` 线上把 `end_of_turn_tick` 发 `0`）。
    pub const fn duration_secs(self) -> Option<u32> {
        Some(match self {
            TimeMode::Timer30s => 30,
            TimeMode::Timer1m => 60,
            TimeMode::Timer2m => 120,
            TimeMode::Timer3m => 180,
            TimeMode::Timer5m => 300,
            TimeMode::TimerInf => return None,
        })
    }

    /// 回合开始时计时器**显示**的秒数 = 时长 − 1。
    ///
    /// 档位：**观测**（`docs/16` §2.1.10 真帧：3 分钟档进对局显示 `02:59`、
    /// 2 分钟档显示 `01:59`；同处还观测到 `00:04`、`00:00` —— 即倒计时从 `时长-1` 走到 0）。
    /// 30 秒档 ⇒ `00:29`、5 分钟档 ⇒ `04:59` 是**推断**（同一条规则的延伸，未逐帧验证）。
    /// 无限档 ⇒ `None`（显示什么**未知**）。
    pub const fn initial_timer_secs(self) -> Option<u32> {
        match self.duration_secs() {
            Some(d) => Some(d - 1),
            None => None,
        }
    }

    /// 计时器标签 `mm:ss`（`02:59` / `01:59` / `00:00`）；无限档 `None`（**未知**）。
    ///
    /// 档位：同 [`TimeMode::initial_timer_secs`]（格式是观测到的 `mm:ss` 两位补零）。
    pub fn timer_label(self) -> Option<String> {
        self.initial_timer_secs()
            .map(|s| format!("{:02}:{:02}", s / 60, s % 60))
    }

    /// 点一下 `TurnTimeButton` 走到的下一档（循环）。
    ///
    /// 档位：**推断**。"点击循环"是 `docs/16` §2.1.5 的原话；循环顺序取判别值序，
    /// 依据是图标 id 序 = 名字表序。**缺**：`0x4cf670` 没给这个按钮连 `pressed`，
    /// 循环体应在命令总线的未穷举分支里。
    pub const fn next(self) -> TimeMode {
        ALL_TIME_MODES[(self as usize + 1) % ALL_TIME_MODES.len()]
    }

    /// 上一档（档位同 [`TimeMode::next`]）。
    pub const fn prev(self) -> TimeMode {
        ALL_TIME_MODES[(self as usize + ALL_TIME_MODES.len() - 1) % ALL_TIME_MODES.len()]
    }

    /// 由关卡模式字节（`LevelModes::time_byte`）换算。
    ///
    /// 21 关全是 `5` ⇒ [`TimeMode::TimerInf`]。这一步现在是**逐位**：
    /// 名字表第 `6+5 = 11` 项就是 `"TimerInf"`（`0x9627a0`），
    /// 与"运行时 `TimeModeInfo` 21/21 = `TimerInf`"互相印证。
    /// （`docs/10` §7 第 6 条原为**推断**，本轮升级，见 `docs/16-client.md` 新增节。）
    pub const fn from_level_byte(byte: u8) -> Option<TimeMode> {
        TimeMode::from_u8(byte)
    }
}

// ===========================================================================
// 4. 三个已在 `level_manager` 里定义的枚举：补"界面/循环"这一侧
// ===========================================================================
//
// 为什么 impl 写在这里：`graphwar_level_manager.rs` 不在本任务可改的文件表里，而
// 「点一下循环到哪一档」「用哪张图标」「线上叫什么」属于**界面/选项**这一侧的证据
// （图标表在 `0x4c9190`，名字表在 `.data.rel.ro`），不是关卡数据的证据。
// 同一个 crate 内给别处定义的类型写 inherent impl 是合法的（不是 trait impl）。

impl AxisMode {
    /// 四个取值，按判别值升序。
    pub const ALL: [AxisMode; 4] = [
        AxisMode::NoAxis,
        AxisMode::OnlyMain,
        AxisMode::EveryFive,
        AxisMode::EveryUnit,
    ];

    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// 按钮图标。档位：**逐位**（[`AXIS_ICONS`]）。
    pub const fn icon_path(self) -> &'static str {
        AXIS_ICONS[self as usize]
    }

    /// 线上名字（`"NoAxis"`…`"EveryUnit"`）。档位：**逐位**（[`MODE_NAME_TABLE`]）。
    pub const fn name(self) -> &'static str {
        MODE_NAME_TABLE[2 + self as usize]
    }

    /// 点一下循环到下一档。档位：**推断**（同 [`TimeMode::next`]）。
    pub const fn next(self) -> AxisMode {
        AxisMode::ALL[(self as usize + 1) % AxisMode::ALL.len()]
    }

    pub const fn prev(self) -> AxisMode {
        AxisMode::ALL[(self as usize + AxisMode::ALL.len() - 1) % AxisMode::ALL.len()]
    }
}

impl FunctionMode {
    pub const ALL: [FunctionMode; 2] = [FunctionMode::NormalFunction, FunctionMode::DiffEqFunction];

    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// 按钮图标（`fx_option.svg` ⇄ `dx_dy_option.svg`）。档位：**逐位**。
    pub const fn icon_path(self) -> &'static str {
        FUNCTION_MODE_ICONS[self as usize]
    }

    pub const fn name(self) -> &'static str {
        MODE_NAME_TABLE[self as usize]
    }

    /// 点击切换另一档。档位：**推断**（`docs/16` §2.1.5「点击切换」）。
    pub const fn next(self) -> FunctionMode {
        FunctionMode::ALL[(self as usize + 1) % FunctionMode::ALL.len()]
    }

    pub const fn prev(self) -> FunctionMode {
        FunctionMode::ALL[(self as usize + FunctionMode::ALL.len() - 1) % FunctionMode::ALL.len()]
    }
}

impl TurnMode {
    pub const ALL: [TurnMode; 2] = [TurnMode::SequentialTurns, TurnMode::SimultaneousTurns];

    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// 按钮图标。档位：**逐位**（[`TURN_MODE_ICONS`]）。
    pub const fn icon_path(self) -> &'static str {
        TURN_MODE_ICONS[self as usize]
    }

    /// 线上名字（`"SequentialTurns"`/`"SimultaneousTurns"`）。档位：**逐位**。
    pub const fn name(self) -> &'static str {
        TURN_MODE_NAMES[self as usize]
    }

    /// 点击切换另一档。档位：**推断**（`docs/16` §2.1.5「点击切换」）。
    pub const fn next(self) -> TurnMode {
        TurnMode::ALL[(self as usize + 1) % TurnMode::ALL.len()]
    }

    pub const fn prev(self) -> TurnMode {
        TurnMode::ALL[(self as usize + TurnMode::ALL.len() - 1) % TurnMode::ALL.len()]
    }
}

// ===========================================================================
// 5. 四格模式的状态（`SetupOptions`）
// ===========================================================================

/// 四格模式 = `AxisModeButton` / `FunctionModeButton` / `TurnModeButton` /
/// `TurnTimeButton` 的值，也就是 `GameInfo` 的 4 个字段。
///
/// 字段顺序与 `.tscn` 里四个按钮同名同序（`docs/10` §3 的 `+0x38..+0x3b`：
/// `function`/`turns`/`axis`/时间）。`GameInfo` 的 9 个字段里就有这四个（`docs/07` §3.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupOptions {
    pub axis: AxisMode,
    pub function: FunctionMode,
    pub turns: TurnMode,
    pub time: TimeMode,
}

/// 新建房间（`LanLobby/CreateButton` → `GameSetup`）时的四格初值。
///
/// 档位：**观测**。依据 = `docs/09-network.md` 里**新建房间的首个 `GameInfo`**：
/// `axis_mode="EveryUnit"`、`function_mode="NormalFunction"`、`turn_mode="SequentialTurns"`、
/// `time_mode="Timer1m"`。
///
/// 交叉验证（独立）：`docs/16` §2.1.10 的按键实验 —— 进对局后按 **1** 次
/// `TurnTimeButton` 得到 `01:59`（2 分钟档）、按 **2** 次得到 `02:59`（3 分钟档）
/// ⇒ 初值必须是**判别值 1**（`Timer1m`），否则两次按键数对不上。
pub const ROOM_DEFAULT_OPTIONS: SetupOptions = SetupOptions {
    axis: AxisMode::EveryUnit,
    function: FunctionMode::NormalFunction,
    turns: TurnMode::SequentialTurns,
    time: TimeMode::Timer1m,
};

impl SetupOptions {
    /// 新建房间的初值，见 [`ROOM_DEFAULT_OPTIONS`]。
    pub const fn room_default() -> SetupOptions {
        ROOM_DEFAULT_OPTIONS
    }

    /// 战役第 `index` 关的四格（`JoinLevelPopUp` 里那四个按钮显示的就是它）。
    ///
    /// 档位：**逐位**。`LEVELS[i].modes` 的 4 字节来自 `mov dword [rsp+0x58], <imm32>`
    /// （`0x4ad4dd`…`0x4aefdf` 共 21 条）；时间字节 21/21 = `5` →
    /// [`TimeMode::TimerInf`]（见 [`TimeMode::from_level_byte`]）。
    pub fn from_level(index: usize) -> Option<SetupOptions> {
        let info = graphwar_level_manager::level_info(index)?;
        let modes: LevelModes = info.modes;
        Some(SetupOptions {
            axis: modes.axis,
            function: modes.function,
            turns: modes.turns,
            time: TimeMode::from_level_byte(modes.time_byte)?,
        })
    }

    /// 四格的图标，顺序 = 面板顺序（axis, function, turn, time）。档位：**逐位**（图标表）。
    pub fn icons(&self) -> [&'static str; 4] {
        [
            AXIS_ICONS[self.axis as usize],
            FUNCTION_MODE_ICONS[self.function as usize],
            TURN_MODE_ICONS[self.turns as usize],
            self.time.icon_path(),
        ]
    }

    /// 四格的线上名字，顺序同上。档位：**逐位**（[`MODE_NAME_TABLE`] / [`TURN_MODE_NAMES`]）。
    pub fn names(&self) -> [&'static str; 4] {
        [
            MODE_NAME_TABLE[2 + self.axis as usize],
            MODE_NAME_TABLE[self.function as usize],
            TURN_MODE_NAMES[self.turns as usize],
            self.time.name(),
        ]
    }

    /// 回合时限档的计时器标签（`02:59` 等）；无限档 `None`。
    pub fn timer_label(&self) -> Option<String> {
        self.time.timer_label()
    }

    /// 这一屏是否显示"有回合时限"（无限档不显示倒计时）。
    ///
    /// 档位：**推断**。依据是 `docs/16` §2.1.10 观测到的标签只出现在有限档；
    /// 无限档的界面上到底是什么（空串 / 隐藏 / `inf`）**未观测**。
    pub fn has_turn_clock(&self) -> bool {
        self.time.duration_secs().is_some()
    }
}

// ===========================================================================
// 6. 玩家条目（一行 = `team1_entry.tscn` / `team2_entry.tscn` 的实例）
// ===========================================================================

/// 一个玩家最多几个士兵。
///
/// 档位：**观测**。`docs/16` §2.1.5：每行的 `SoldierIconContainer` 下是
/// `MarginContainer…5`（**5 个槽**），空槽显示 `res://empty_soldier.svg`；
/// [`FN_SOLDIER_ICONS`] 按槽刷 `SkinIcon`/`FaceIcon`/`HatIcon`/`EmptyIcon`。
pub const MAX_SOLDIERS_PER_PLAYER: u8 = 5;

/// 队伍。
///
/// 档位：**观测**（`Team1Container`/`Team2Container` 两个容器就是两队，
/// `0x1c2d7f`/`0x1c2d8d`）。`ChangeTeamButton` 的图标 `arrow_right.svg`（Team1）/
/// `arrow_left.svg`（Team2）表示"往哪边换"—— 这两个箭头**不在** `.so` 里
/// （`x.svg`/`plus.svg`/`minus.svg`/`arrow_*.svg` 全库 0 命中，写在 `.tscn` 里），
/// 所以这一条只到**推断**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    One,
    Two,
}

impl Team {
    /// 换到对面。档位：**推断**（见 [`Team`] 的说明）。
    pub const fn other(self) -> Team {
        match self {
            Team::One => Team::Two,
            Team::Two => Team::One,
        }
    }

    /// 该队的容器名（`get_node` 实参）。档位：**逐位**（`0x1c2d7f` / `0x1c2d8d`）。
    pub const fn container(self) -> &'static str {
        match self {
            Team::One => "Team1Container",
            Team::Two => "Team2Container",
        }
    }

    /// `ChangeTeamButton` 的图标。档位：**推断**（只在 `.tscn` 里，`.so` 无字面量）。
    pub const fn change_icon(self) -> &'static str {
        match self {
            Team::One => "res://arrow_right.svg",
            Team::Two => "res://arrow_left.svg",
        }
    }
}

/// 一行玩家（[`FN_PLAYER_ROW`] 装配出来的对象）。
///
/// ⚠️ **名字是我们的**：二进制里没有 `GameSetupPlayerEntry` / `PlayerEntry` 这类类型字符串
/// （`strings` 全库 0 命中 —— 连 `graphwar_player_manager.rs` 的锚点也只有
/// `RemovePlayerButton` 串池）。所以这个结构体只是"这一行可变状态"的建模，
/// **不是**原版结构体名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerEntry {
    /// 显示名：人类取 `user://player.config` 里的名字，Bot 取名字池里的
    /// （`docs/16` §2.1.5 观测到 `Deep Blue`/`Alpha Zordon`/`ChatGPT`/`Multivac`…）。
    pub name: String,
    /// Bot 的难度档（`0..=5`，`AddBotPopUp` 六个按钮）；人类 `None`。
    pub bot_level: Option<u8>,
    pub team: Team,
    /// 士兵数，恒在 `0..=MAX_SOLDIERS_PER_PLAYER`。
    pub soldiers: u8,
}

impl PlayerEntry {
    /// 人类玩家的一行。**士兵数必须显式给**：战役局按关卡数据
    /// （[`campaign_soldier_count`]），LAN 局由玩家自己加减（初始值**未观测**）。
    /// `docs/143` 附 1 的教训就是"别把 1 当成默认值"。
    pub fn human(name: impl Into<String>, team: Team, soldiers: u8) -> PlayerEntry {
        PlayerEntry {
            name: name.into(),
            bot_level: None,
            team,
            soldiers: soldiers.min(MAX_SOLDIERS_PER_PLAYER),
        }
    }

    /// Bot 的一行。难度档 `0..=5`（超界钳位，因为 `AddBotPopUp` 只有 6 个按钮）。
    pub fn bot(level: u8, name: impl Into<String>, team: Team, soldiers: u8) -> PlayerEntry {
        PlayerEntry {
            name: name.into(),
            bot_level: Some(level.min(5)),
            team,
            soldiers: soldiers.min(MAX_SOLDIERS_PER_PLAYER),
        }
    }

    pub fn is_bot(&self) -> bool {
        self.bot_level.is_some()
    }

    /// `AddSoldierButton`：加一个士兵，**饱和在 5**。返回是否真的变了。
    ///
    /// 档位：**观测**（5 个槽的存在）+ **推断**（"到 5 就不再涨"由槽数推出；
    /// 缺"按下后按钮变灰"的观测）。
    pub fn add_soldier(&mut self) -> bool {
        if self.soldiers >= MAX_SOLDIERS_PER_PLAYER {
            return false;
        }
        self.soldiers += 1;
        true
    }

    /// `RemoveSoldierButton`：减一个士兵，**饱和在 0**。删人是 `RemovePlayerButton` 的事。
    /// 档位：同 [`PlayerEntry::add_soldier`]。
    pub fn remove_soldier(&mut self) -> bool {
        if self.soldiers == 0 {
            return false;
        }
        self.soldiers -= 1;
        true
    }

    /// `ChangeTeamButton`：换队。档位：**观测**（按钮存在）+ **推断**（换到对面）。
    pub fn change_team(&mut self) {
        self.team = self.team.other();
    }

    /// 该行要画的槽：`(槽下标, 是否已占用)`，共 [`MAX_SOLDIERS_PER_PLAYER`] 个。
    /// 未占用的槽画 `res://empty_soldier.svg`（观测，`docs/16` §2.1.5）。
    pub fn soldier_slots(&self) -> [(u8, bool); MAX_SOLDIERS_PER_PLAYER as usize] {
        let mut out = [(0u8, false); MAX_SOLDIERS_PER_PLAYER as usize];
        let mut i = 0;
        while i < out.len() {
            out[i] = (i as u8, (i as u8) < self.soldiers);
            i += 1;
        }
        out
    }
}

// ===========================================================================
// 7. 关卡数据 → 这一屏的取值（士兵数 / 回合上限）
// ===========================================================================

/// 战役第 `index` 关的**我方士兵数**（`SETUPS[index].soldiers`）。
///
/// 档位：**逐位**（`0x476320(mgr, player, N)` 的 `edx` 立即数；`docs/10` §4 的 `S` 列，
/// 385 份回放的 `R` 列 21/21 一致）。
///
/// 这条存在的理由：`docs/16` §13 附 1 记着"默认士兵数曾被做成 1、丢掉关卡数据"这个错。
/// 战役局**必须**用这里的值。
pub fn campaign_soldier_count(index: usize) -> Option<u32> {
    graphwar_level_manager::level_setup(index).map(|s| s.soldiers)
}

/// 战役第 `index` 关的回合上限：`Some(None)` = 该关无上限，`None` = 没有这一关。
///
/// 档位：**逐位**（`turn_limit_info` 载荷的 `N`；`docs/10` §4 的 `S` 列）。
/// 载荷第一个 u32 恒为 `1`、语义**未知**（`docs/10` §7 第 14 条），故不建模。
pub fn campaign_turn_limit(index: usize) -> Option<Option<u32>> {
    graphwar_level_manager::level_setup(index).map(|s| s.turn_limit)
}

// ===========================================================================
// 8. Start / Cancel 的屏切换
// ===========================================================================

/// 按下按钮之后的去向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupAction {
    /// 进对局（[`Screen::PlayScreen`]）。
    StartMatch,
    /// 什么都不做，**也不提示**。
    SilentNoOp,
    /// 退回某一屏。
    Back(Screen),
}

/// `Start` 能不能开始：**两个队都要有人**（自己 + 一个对手）。
///
/// 档位：**观测**（`docs/16` §2.1.5：不加 Bot 直接 ` Start ` 是**静默无操作**；
/// `archive/22-live-game-harness.md` §5.1 是同一个观测）。
/// 判据取"两队都非空"是**推断**：缺"每队一人的边界"与"3 人 2:1 算不算"的观测。
pub fn can_start(entries: &[PlayerEntry]) -> bool {
    entries.iter().any(|e| e.team == Team::One) && entries.iter().any(|e| e.team == Team::Two)
}

/// `GameSetup` 的 ` Start `：有对手 → `PlayScreen`；没对手 → 静默。
///
/// 档位：**观测**（`Start` → `PlayScreen.visible=1`，`docs/16` §2.1.5；屏名逐位）。
/// ⚠️ 与 `docs/16` §14.1 第 15 条对齐：原版是**纯静默**；我们自己的客户端曾给一句提示，
/// 那是**自定**行为，[`SetupAction::SilentNoOp`] 才是原版。
pub fn start_action(entries: &[PlayerEntry]) -> SetupAction {
    if can_start(entries) {
        SetupAction::StartMatch
    } else {
        SetupAction::SilentNoOp
    }
}

/// `GameSetup` 的返回键（`ExitButtonContainer/{ClipButton,LongPressButton}`，`return.svg`）。
///
/// 档位：**推断**。依据：进入这一屏的唯一观测路径是
/// `LanLobby/CreateButton` → `GameSetup`（`docs/16` §2.1.4），所以返回应回 `LanLobby`。
/// **缺**：按下这个返回键的观测。
pub const fn exit_action() -> SetupAction {
    SetupAction::Back(Screen::LanLobby)
}

/// `JoinLevelPopUp` 的 ` Start `（`docs/16` §2.1.3 的 `MarginContainer2/GraphwarButton`）。
///
/// 档位：**推断**。`docs/16` §2.1.3 明写"未按下验证，避免污染存档"；
/// 同屏语义在 `GameSetup` 上是观测（→ `PlayScreen`）⇒ 方向一致，但**没有**直接观测。
pub const fn join_level_start() -> SetupAction {
    SetupAction::StartMatch
}

/// `JoinLevelPopUp` 的 ` Cancel `（`docs/16` §2.1.3 的 `HBoxContainer3/MarginContainer`）。
///
/// 档位：**推断**。这一屏是从 `Campaign/<level_n>` 打开的（**观测**）；
/// 命令总线里有 `exit_campaign` 键（**逐位**，`0x3b1ed4`/`0x3b1ee2`）⇒ 有回战役屏的路。
pub const fn join_level_cancel() -> SetupAction {
    SetupAction::Back(Screen::Campaign)
}
