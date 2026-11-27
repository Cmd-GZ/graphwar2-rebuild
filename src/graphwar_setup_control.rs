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

// ===========================================================================
// 10. 行构件与接线：9 条 F3 函数落成**有函数体**（vaddr / FDE / 逐指令证据）
// ===========================================================================
//
// 这 9 条在 `fixtures/function_coverage.tsv` 里原来是 **F3**（"地址有登记、但没有 fn"）。
// 落地判据是 `AGENTS.md` §0.5：**每个函数能指到它自己的指令范围**。
//   * 边界：`.eh_frame` FDE（`tools/addr_probe.py fde <vaddr>`）—— 不读 Ghidra 表；
//   * 真名：`fixtures/windows_name_to_addr.tsv`（Windows 2.0.60 DLL **同版本 + 同指令集**
//     的 COFF 符号表，经 `panic::Location` 一对一锚落到 Linux .so 地址）
//     + `fixtures/name_to_addr_v2.tsv`（三向：Win 2.0.60 / Win early2026 / Android 2.0.60）；
//     旁证 = Android arm64 未 strip `.symtab` 里同模块函数的**相对顺序 + 体量**；
//   * 语义：该函数自己的指令（被调者地址、`.rodata` 字面量 + 长度立即数、字段偏移、分支）。
//
// 逐条台账（FDE 范围 / 体量 / 调用者 / Location / 档位）在
// `fixtures/setup_control_anchors.tsv`；推翻了旧结论的那一条记在 `docs/06` §103.2。
//
// ⚠️ **三条名字未定**（`AGENTS.md` §0.1：不许把推断写成结论）：
//   * `0x4b_d280` —— 体与 `0x4b_d030`（已确认的 `__init::__inner_init`）**逐项同形**，
//     名字按同族取；**哪一份对应哪个 `#[godot_api] impl I*` 块是推断**（缺 gdext 版本的
//     vtable 槽序对齐）。
//   * `0x4b_ce80` —— 候选 `__register_constants`，但 `fixtures/name_to_addr.tsv` 给的是
//     **4 选 1 的歧义**（{0x4b88d0, 0x4b8a50, 0x4bce80, 0x4bd280}）⇒ 名字标 `?`。
//   * `0x4d_88b0` —— 候选 `ready`，同样是 2 选 1 歧义（{0x4d1e00, 0x4d88b0}）⇒ 名字标 `?`。

/// `GraphwarSetupControl::update_name` —— **x86 `0x4c1e40`**（FDE `0x4c1e40..0x4c213e`，`0x2fe` = **766 B**）。
///
/// 名字来源：`windows_name_to_addr.tsv` 唯一锚（`src/graphwar_setup_control.rs:178:127`）。
/// 既有常量 [`FN_PLAYER_NAME`] 指的就是这条。
pub const FN_UPDATE_NAME: u64 = 0x4c_1e40;

/// `GraphwarSetupControl::update_rank` —— **x86 `0x4c2140`**（FDE `0x4c2140..0x4c27e7`，`0x6a7` = **1703 B**）。
/// 名字来源：唯一锚（`187:121`）。既有常量 [`FN_RANK_ICON`]。
pub const FN_UPDATE_RANK: u64 = 0x4c_2140;

/// `GraphwarSetupControl::is_leader_local` —— **x86 `0x4c2b90`**（FDE `0x4c2b90..0x4c2ee2`，`0x352` = **850 B**）。
///
/// ⚠️ 既有常量 [`FN_MODE_PANEL_SYNC`] 也指 `0x4c2b90`，但它把这条叫「模式面板同步」并注明
/// **推断**成"形状像 `set_button_icon`"。**本轮证伪**：真名 `is_leader_local`，
/// 返回 **`bool`**（`0x4c2db9 mov bl,1` / `0x4c2e03 xor ebx,ebx` / `0x4c2e15 mov eax,ebx`）。
/// 见 `docs/06` §103.2 与台账文件头。
pub const FN_IS_LEADER_LOCAL: u64 = 0x4c_2b90;

/// `GraphwarSetupControl::update_soldiers` —— **x86 `0x4c2ef0`**（FDE `0x4c2ef0..0x4c44a3`，`0x15b3` = **5555 B**）。
/// 名字来源：唯一锚（`202:47`）。既有常量 [`FN_SOLDIER_ICONS`]。
pub const FN_UPDATE_SOLDIERS: u64 = 0x4c_2ef0;

/// `GraphwarSetupControl::connect_entry_buttons` —— **x86 `0x4c4c20`**（FDE `0x4c4c20..0x4c5b43`，`0xf23` = **3875 B**）。
/// 名字来源：唯一锚（`257:133`）。既有常量 [`FN_PLAYER_ROW`]。
pub const FN_CONNECT_ENTRY_BUTTONS: u64 = 0x4c_4c20;

/// `graphwar_setup_control::_::__init::__inner_init`（第 1 份实例）—— **x86 `0x4bd030`**
/// （FDE `0x4bd030..0x4bd27f`，`0x24f` = **591 B**；唯一 Location `608:1`）。
///
/// 档位：**观测**（名字 = `windows_name_to_addr.tsv` 唯一锚 + `name_to_addr_v2.tsv` 唯一锚）。
/// 体是 gdext 宏产物：把某个 `#[godot_api] impl I*` 块的接口登记进静态存储。
pub const FN_SETUP_INNER_INIT: u64 = 0x4b_d030;

/// `__init::__inner_init` 的**第 2 份实例** —— **x86 `0x4bd280`**（FDE `0x4bd280..0x4bd426`，`0x1a6` = **422 B**；Location `95:1`）。
///
/// 档位：**观测**（体）/**推断**（它是 4 份里的哪一份）。依据见该函数的文档。
pub const FN_SETUP_INNER_INIT_V2: u64 = 0x4b_d280;

/// 候选 `__register_constants` —— **x86 `0x4bce80`**（FDE `0x4bce80..0x4bd028`，`0x1a8` = **424 B**；Location `95:1`）。
///
/// ⚠️ **名字未定**：`fixtures/name_to_addr.tsv` 把 `__register_constants` 判成"歧义"，
/// 候选 4 个（`{0x4b88d0, 0x4b8a50, 0x4bce80, 0x4bd280}`）。体是观测的，名是未知的。
pub const FN_SETUP_REGISTER_CONSTANTS_CANDIDATE: u64 = 0x4b_ce80;

/// 候选 `GraphwarSetupControl::ready` —— **x86 `0x4d88b0`**（FDE `0x4d88b0..0x4d8957`，`0xa7` = **167 B**；Location `705:76`）。
///
/// ⚠️ **名字未定**：`fixtures/name_to_addr.tsv` 判"歧义"，候选 2 个（`{0x4d1e00, 0x4d88b0}`）。
/// 体是观测的，且**被调者已知**：`0x51_7490` = `GraphwarGameClient::request_lock_change`
/// （`docs/02` §22 的函数表 + `fixtures/name_to_addr_v2.tsv` 唯一锚）。
pub const FN_SETUP_READY_CANDIDATE: u64 = 0x4d_88b0;

/// 本批 9 条的 `(vaddr, fde_lo, fde_hi, size, 真名或候选名, 名字档位)`。
///
/// 档位：**逐位**（FDE 范围 = `.eh_frame` 解出来的边界，`tools/addr_probe.py fde` 可复跑）。
/// 这张表是为了让"每条都能指到自己的指令范围"变成**可断言的数据**，
/// 而不是散在 9 份 doc 注释里。
pub const F3_ANCHORS: &[(u64, u64, u64, u32, &str, Evidence)] = &[
    (0x4c_1e40, 0x4c_1e40, 0x4c_213e, 0x2fe, "GraphwarSetupControl::update_name", Evidence::Observed),
    (0x4c_2140, 0x4c_2140, 0x4c_27e7, 0x6a7, "GraphwarSetupControl::update_rank", Evidence::Observed),
    (0x4c_2b90, 0x4c_2b90, 0x4c_2ee2, 0x352, "GraphwarSetupControl::is_leader_local", Evidence::Observed),
    (0x4c_2ef0, 0x4c_2ef0, 0x4c_44a3, 0x15b3, "GraphwarSetupControl::update_soldiers", Evidence::Observed),
    (0x4c_4c20, 0x4c_4c20, 0x4c_5b43, 0xf23, "GraphwarSetupControl::connect_entry_buttons", Evidence::Observed),
    (0x4b_d030, 0x4b_d030, 0x4b_d27f, 0x24f, "_::__init::__inner_init", Evidence::Observed),
    (0x4b_d280, 0x4b_d280, 0x4b_d426, 0x1a6, "_::__init::__inner_init (实例 #2)", Evidence::Inferred),
    (0x4b_ce80, 0x4b_ce80, 0x4b_d028, 0x1a8, "__register_constants（候选，未定）", Evidence::Unknown),
    (0x4d_88b0, 0x4d_88b0, 0x4d_8957, 0xa7, "GraphwarSetupControl::ready（候选，未定）", Evidence::Unknown),
];

/// 一条结论的档位（`AGENTS.md` §0.3 的四档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// 与真二进制逐字节/逐位对拍。
    BitExact,
    /// 能指到反汇编地址 / `.rodata` 字节。
    Observed,
    /// 有依据但未证；**必须**在文档里写清缺什么。
    Inferred,
    /// 不知道。
    Unknown,
}

/// `.rodata` 里的节点名字面量 + 长度（**逐位**：`lea` 目标 + 紧随的 `mov r<n>d,<len>` 立即数）。
///
/// 生成方法（可复跑）：`tools/addr_probe.py dis <vaddr>`，找 `lea rcx,[rip+X]` 与
/// 紧随的长度立即数，再用 `loc_xrefs.Elf.read(va, len)` 读出字节自校验。
pub const NODE_NAME_LITERALS: &[(&str, u64, usize)] = &[
    ("PlayerNameLabel", 0x1c_2d52, 15),
    ("RankIcon", 0x19_fb70, 8),
    ("SoldierIconContainer", 0x1c_2da6, 20),
    ("SkinIcon", 0x19_f3d0, 8),
    ("FaceIcon", 0x19_f180, 8),
    ("HatIcon", 0x1c_2dba, 7),
    ("EmptyIcon", 0x1c_2dc1, 9),
    ("GraphwarRootNode", 0x19_e490, 16),
    ("ChangeTeamButton", 0x19_e030, 16),
    ("RemovePlayerButton", 0x1c_2de5, 18),
    ("AddSoldierButton", 0x19_e740, 16),
    ("RemoveSoldierButton", 0x1c_2df7, 19),
];

/// `Gd` 操作包装用的**操作名字面量**（`.rodata` + 长度立即数）。
///
/// 与 `src/graphwar_nodes.rs` 的 [`GdOp`] 是同一批名字（那里已逐条验过 4 个操作名穷举）。
pub const GD_OP_LITERALS: &[(&str, u64, usize)] = &[
    ("clone", 0x1b_fe73, 5),
    ("upcast_ref", 0x1c_2a84, 10),
    ("to_godot", 0x19_f850, 8),
    ("set_texture", 0x1c_2d61, 11),
];

/// `upcast_ref` 失败臂的 panic 文案（`.rodata` `0x1c2a4b`，长度立即数 `0x2f` = **47**）。
///
/// 档位：**逐位**（`0x4c2096` 的 `lea` + `0x4c20b0 mov esi,0x2f`）。
pub const MSG_UPCAST_FAILED: &str = "Upcast failed. This is a bug; please report it.";

/// 原版经 `unwrap()` 在 `None` 上 panic 时的文案（std 侧常量，**逐位**：这就是 std 的串）。
///
/// 用它的地方逐条记在调用点的 doc 里（每个函数都有自己的 panic 槽地址）。
pub const MSG_OPTION_UNWRAP: &str = "called `Option::unwrap()` on a `None` value";

/// `RefCell::borrow_mut` 冲突臂的文案。
///
/// ⚠️ **推断**：这一臂在原版是 std 的 `RefCell` 借用冲突 panic（`0x4c2e40`/`0x4d891c`
/// 引的是 `src/graphwar_setup_control.rs` 的 Location 槽 + `BorrowMutError` 路径）；
/// **具体文案串未在 `.rodata` 定位**（std 侧按版本会变）⇒ 这里用我方文案，
/// "会 panic" 是观测的，"文案" 是推断。
pub const MSG_ALREADY_MUTABLY_BORROWED: &str = "already mutably borrowed";

// ---------------------------------------------------------------------------
// 10.1 两条接缝（原版这里是 gdext / 引擎调用）
// ---------------------------------------------------------------------------

/// 原版一个 `Gd<T>` 句柄 = **两个字**。
///
/// 判据（**观测**）：这 5 个函数把入参 `rsi:rdx` **成对**存储/传递（`0x4c1e57`/`0x4c1e5c`），
/// 再整对交给 `0x4066c0`（`Gd` 操作包装 B）与 `0x447e20`（`upcast_ref` 单态化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GdHandle {
    /// 引擎对象指针。
    pub obj: usize,
    /// 缓存的 RTTI 指针（gdext 的 `Gd` 第二字）。
    pub rtti: usize,
}

impl GdHandle {
    /// 空句柄（`obj == 0`）。`0x4c1e9e test r14,r14 / je` 判的就是它。
    pub const NULL: GdHandle = GdHandle { obj: 0, rtti: 0 };

    pub const fn new(obj: usize, rtti: usize) -> GdHandle {
        GdHandle { obj, rtti }
    }

    pub const fn is_null(&self) -> bool {
        self.obj == 0
    }
}

/// 管理器里查到的一个值（**不透明，3 个字**）。
///
/// 判据（**观测**）：`0x4c1fd9..0x4c2002` 用 `0x91e260`（`Rc::clone` 垫片）克隆前两字，
/// 再单独搬一个字（`0x4c1ff2 mov rax,[rsp+0x38]`）。
/// **字段语义未解**（缺：`GraphwarNameManager` / `GraphwarRankManager` 的值类型定义），
/// 所以本层只透传，不解释 —— 这是"宁可标未知也不编语义"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpaqueValue {
    pub a: usize,
    pub b: usize,
    pub c: usize,
}

/// 模块容器里拿到的一个模块句柄（`ModuleContainer::get::<M>()` 的结果）。
///
/// 原版的 `M` 是 72–296 字节的对象本体，**内部字段布局只有被本批函数读到的那几处是观测的**
/// （逐处写在 [`SetupModules`] 的方法文档里）。用 `Copy` 的句柄是为了让"取出模块 → 继续用"
/// 不引入借用冲突（原版是 `Rc<RefCell<..>>`，`borrow()` 标志在 `+0x10`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleSlot(pub &'static str);

/// 引擎侧（Godot）接缝：原版这些调用都走 GOT 槽 / gdext 包装，引擎侧成员名**未解**的
/// 一律在方法文档里标出来（`AGENTS.md`：间接调用不能只看 `call rel32`）。
pub trait SetupEngine {
    /// `0x4066c0` + 操作名 `"clone"`(5 B @`0x1bfe73`) —— `Gd` 操作包装 B 的 clone 臂。
    /// 失败形态是 `Result`（原版 `0x4066c0` 多一条成败判别，见 `src/graphwar_nodes.rs`）。
    fn gd_clone(&mut self, src: GdHandle) -> Option<GdHandle>;

    /// `0x447e20`（`update_name`/`update_rank`/`update_soldiers`/`connect_entry_buttons` 共用）
    /// 与 `0x4061a0`（`update_name` 尾段）—— `upcast_ref` 单态化。
    /// 返回 `None` ⇒ 原版走 `MSG_UPCAST_FAILED` 的 panic 臂。
    fn upcast_ref(&mut self, src: GdHandle) -> Option<GdHandle>;

    /// `0x3ac770` / `0x3acb00` / `0x3ab590` / `0x3ad570` / `0x3adc50`
    /// —— 11 个 `get_node_by_name` 单态化里的 5 个（按返回类型分，`src/graphwar_nodes.rs` §3）。
    /// 返回 `None` ⇒ 原版 `unwrap()` panic。
    fn get_node_by_name(&mut self, base: GdHandle, name: &str) -> Option<GdHandle>;

    /// `0x407580` + 操作名 `"to_godot"`(8 B @`0x19f850`)。
    fn to_godot(&mut self, src: GdHandle) -> Option<GdHandle>;

    /// `0x3a01a0` + 操作名 `"set_texture"`(11 B @`0x1c2d61`)。
    fn set_texture(&mut self, icon: GdHandle, tex: GdHandle);

    /// `0x4b1640` —— 把 `connect_entry_buttons` 入参的**裸对象指针**（`rdi`）做成 `Gd`
    /// （`Gd::from_obj_sys` 形态：出参两字，`0x4c4c68`/`0x4c4c72` 各判一次非空）。
    fn gd_from_obj(&mut self, raw: usize) -> Option<GdHandle>;

    /// `0x4095a0`（`GraphwarRootNode` 那一支）/ `0x4098f0`（按钮那一支）—— 把按钮的
    /// `pressed` 连到一个回调。**引擎侧成员名未解**（缺：`0x4095a0` 体内的 GOT 槽成员名）。
    fn connect_button(&mut self, button: GdHandle, handler: GdHandle);

    /// `0x41ce80` / `0x405c70` / `0x405d20` / `0x4068f0` 一族 —— `Gd` 引用计数/借用包装。
    /// **语义未解**（缺：引擎侧成员名）；本层只记"发生了这一步"。
    fn gd_touch(&mut self, src: GdHandle);
}

/// 模块容器接缝：每个方法 = **一个** `ModuleContainer::get::<M>()` 单态化。
///
/// 返回 `None` ⇒ 原版 `get` 返回 `None`，由**各调用点自己的** `unwrap()` panic
/// （每个函数的 panic 槽地址写在该函数文档里）。
/// 七个 `get` 的地址来自 `docs/02-module-map.md` §22.3 的 25 模块表（**逐位**：三路证据交集）。
pub trait SetupModules {
    /// `0x46ba20` = `get::<GraphwarNameManager>()`。**观测**（§22.3 表 + `0x4c1e84`）。
    fn name_manager(&mut self) -> Option<ModuleSlot>;
    /// `0x46bfe0` = `get::<GraphwarRankManager>()`。**观测**（`0x4c218e`）。
    fn rank_manager(&mut self) -> Option<ModuleSlot>;
    /// `0x46a490` = `get::<GraphwarPlayerManager>()`。**观测**（`0x4c2bc8`）。
    fn player_manager(&mut self) -> Option<ModuleSlot>;
    /// `0x46c430` = `get::<GraphwarLocalPlayerManager>()`。**观测**（`0x4c2c10`）。
    fn local_player_manager(&mut self) -> Option<ModuleSlot>;
    /// `0x46a770` = `get::<GraphwarSoldierManager>()`。**观测**（`0x4c2f46`）。
    fn soldier_manager(&mut self) -> Option<ModuleSlot>;
    /// `0x46b8b0` = `get::<GraphwarTeamManager>()`。**观测**（`0x4c2f90`）。
    fn team_manager(&mut self) -> Option<ModuleSlot>;
    /// `0x46a8e0` = `get::<GraphwarGraphicsManager>()`。**观测**（`0x4c2fdd`）。
    fn graphics_manager(&mut self) -> Option<ModuleSlot>;

    /// `GraphwarNameManager` 的 `HashMap<i32, _>` 查找。
    ///
    /// 字段偏移（**观测**，全在 `0x4c1f37..0x4c1fd9`）：
    /// `+0x48` = `len`（为 0 直接跳过，`0x4c1f37 cmp qword [r14+0x48],0`）、
    /// `+0x30` = 桶控制字节、`+0x38` = 桶掩码、`+0x50` = hasher 状态、
    /// 键比较 `0x4c1fa6 cmp ebx,[rsi-0x20]`（键在桶里 −0x20 处）、值自 `+0x18` 起。
    fn name_row(&mut self, mgr: ModuleSlot, index: i32) -> Option<OpaqueValue>;

    /// `GraphwarRankManager` 的按 rank 查找（`0x4c2255 call 0x47dec0` = 哈希，随后 hashbrown 探测）。
    /// 字段偏移**未解**（本函数只用了 `0x47dec0` 之后的探测，桶基址在寄存器里转手多次）。
    fn rank_row(&mut self, mgr: ModuleSlot, rank_id: u32) -> Option<OpaqueValue>;

    /// 遍历 `GraphwarPlayerManager` 的玩家表 —— 返回每条记录的**不透明** id（`OpaqueValue::a`）。
    ///
    /// 判据（**观测**）：`0x4c2c3f mov rax,[rbx+0x48]`（len）、`0x4c2c4f mov rcx,[rbx+0x30]`
    /// （桶）、`0x4c2c53` 起是 hashbrown 的迭代器（`pmovmskb` + `xor 0xffff` 找有效桶）。
    fn player_rows(&mut self, mgr: ModuleSlot) -> Vec<OpaqueValue>;

    /// `GraphwarLocalPlayerManager` 的**本地玩家 id 向量**（`Vec<u32>`）。
    ///
    /// 字段偏移（**观测**，`0x4c2d5e..0x4c2d75`）：`+0x20` = 数据指针、`+0x28` = 元素个数，
    /// 线性扫描步长 4 字节（`0x4c2d66 shl rdx,2`、`0x4c2d78 lea rsi,[rsi+4]`）。
    fn local_player_ids(&mut self, mgr: ModuleSlot) -> Vec<u32>;

    /// `0x3bf230` —— 从一条玩家记录里取出参与比对的那个 `i32`。
    ///
    /// 判据（**观测**）：`0x4c2d4a call 0x3bf230` 之后 `0x4c2d57 mov rdi,[rsp+0x20]` /
    /// `0x4c2d5c mov eax,[rdi]`。**`0x3bf230` 本身语义未解**（缺：它体内的字段读法），
    /// 只能确定"它产出一个带 `i32` 首字段的东西"。
    fn player_ident(&mut self, row: OpaqueValue) -> i32;

    /// `GraphwarTeamManager::get_team`（**x86 `0x3c2120`**，`docs/02` §… 函数表；
    /// 也是 `0x473520 get_alive_teams` 的同一个被调者）。返回队伍句柄。
    fn get_team(&mut self, mgr: ModuleSlot, team_id: u32) -> Option<ModuleSlot>;

    /// `GraphwarSoldierManager` 上的"这支队伍里几个兵"—— `update_soldiers` 用它决定
    /// 画几个 `SkinIcon`。**语义推断**（缺：`0x3bf230` 之外那几条被调者的字段读法），
    /// 只有"遍历 + 上限 5"两处是观测（[`MAX_SOLDIERS_PER_PLAYER`]）。
    fn soldier_slots(&mut self, mgr: ModuleSlot, team: ModuleSlot) -> Vec<bool>;

    /// `0x3ec130`（`update_name` 尾段，`rdi=&标签`、`rsi=&查到的值`）—— 把行数据交给标签。
    /// **引擎侧成员名未解**（缺：`0x3ec130` 体内的 GOT 槽成员名）。
    fn apply_name_row(&mut self, label: GdHandle, row: OpaqueValue);
}

/// `5` 个槽（[`MAX_SOLDIERS_PER_PLAYER`]）里"该画哪几种图标"的**顺序**。
///
/// 判据（**观测**）：`0x4c2ef0` 里 `get_node` 的字面量顺序就是
/// `SoldierIconContainer` → `SkinIcon` → `FaceIcon` → `HatIcon` → `EmptyIcon`。
pub const SOLDIER_ICON_ORDER: [&str; 4] = ["SkinIcon", "FaceIcon", "HatIcon", "EmptyIcon"];

// ---------------------------------------------------------------------------
// 10.2 五个行构件 / 接线函数
// ---------------------------------------------------------------------------

/// 我方**纯意图**类型：把原版本模块的**关联函数/方法**（符号表真名
/// `graphwarwarrust::graphwar_setup_control::GraphwarSetupControl::*`）按原版的类型归属落下来。
///
/// ⚠️ **这个名字用过一次改名**（T63）：原版 `GraphwarSetupControl` **就是那个 Godot 类本体**
/// （`#[derive(GodotClass)] struct` + 方法体都写在 `src/graphwar_setup_control.rs` 里，
/// 证据：`_::__init::__inner_init 0x4bcc60` 的 `panic::Location` 落本文件；
/// `tools/addr_probe.py locs 0x4bcc60`）。T63 把 `src/_scaffold/graphwar_setup_control_class.rs`
/// 的节点类搬回本文件后，两者会撞名，所以这份**纯数据**意图类型让出名字，改成
/// `SetupControlIntent` —— 原版**没有**这个类型，它只是本文件那些关联函数的 `self` 形状。
/// （T57 在 `graphwar_plane.rs` 上踩过同一个坑，处置相同：`GraphwarPlane` → `PlaneIntent`。）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SetupControlIntent;

/// `RankIcon` 上被 `set_texture` 的层数。
///
/// 档位：**观测**（`0x4c2140` 里 `"set_texture"` 字面量出现 **3** 次：
/// `0x4c235f` / `0x4c2509` / `0x4c2639`，每段前各一条 `"upcast_ref"` 与 `"to_godot"`）。
pub const RANK_ICON_LAYERS: usize = 3;

/// `connect_entry_buttons` 取的 4 个按钮节点名（顺序 = `.rodata` 字面量顺序）。
///
/// 档位：**逐位**（名字 + 长度立即数，见 [`NODE_NAME_LITERALS`]）。
pub const ENTRY_BUTTONS: [&str; 4] = [
    "ChangeTeamButton",
    "RemovePlayerButton",
    "AddSoldierButton",
    "RemoveSoldierButton",
];

impl SetupControlIntent {
    /// `GraphwarSetupControl::is_leader_local` —— **x86 `0x4c2b90`**。
    ///
    /// 关联**方法**（`rdi` = self，`[self+0x40]` = 模块容器 `Rc<RefCell<..>>`）。
    /// 返回 **`bool`**（`0x4c2e15 mov eax,ebx`）。
    ///
    /// ```text
    /// 4c2ba6 mov r12,[rdi+0x40]        ; self+0x40 = 模块容器 Rc（借用标志 @+0x10）
    /// 4c2baf cmp rax,0x7fff…ff / jae   ; borrow() 计数溢出 ⇒ panic 0x925320（槽 0x9620c8）
    /// 4c2bc8 call 0x46a490             ; get::<GraphwarPlayerManager>()，None ⇒ panic 0x4c2e31
    /// 4c2c10 call 0x46c430             ; get::<GraphwarLocalPlayerManager>()，None ⇒ panic 0x4c2e5e
    /// 4c2c3f mov rax,[rbx+0x48]        ; 玩家表 len
    /// 4c2c4f mov rcx,[rbx+0x30]        ; 玩家表桶
    /// 4c2c53..4c2c72                   ; hashbrown 迭代（pmovmskb / xor 0xffff）
    /// 4c2d4a call 0x3bf230             ; 取本条记录的 id
    /// 4c2d5c mov eax,[rdi]             ; = 那个 i32
    /// 4c2d5e mov rcx,[r14+0x20]        ; LocalPlayerManager 的 Vec 数据
    /// 4c2d66 shl rdx,2                 ; len*4（元素 4 字节）
    /// 4c2d70/4c2d75/4c2d7c             ; 线性比对 cmp dword [rcx+rsi],eax
    /// 4c2db9 mov bl,1                  ; 命中 ⇒ true
    /// 4c2e03 xor ebx,ebx               ; 扫完 ⇒ false
    /// ```
    ///
    /// 档位：**观测**（控制流 + 字段偏移 + 返回值位置全部出自本函数自己的指令）。
    pub fn is_leader_local<M: SetupModules + ?Sized>(modules: &mut M) -> bool {
        // 0x4c2b90  —— 本函数起点；FDE 0x4c2b90..0x4c2ee2（0x352 = 850 B）
        // 0x4c2bc8 / 0x4c2c10：两次 get，各自的 None 臂是独立的 panic 点
        let players = modules.player_manager().expect(MSG_OPTION_UNWRAP);
        let locals = modules.local_player_manager().expect(MSG_OPTION_UNWRAP);
        // 0x4c2d5e..0x4c2d75：本地 id 向量（+0x20 数据 / +0x28 个数 / 步长 4 B）
        let local_ids = modules.local_player_ids(locals);
        // 0x4c2c3f..0x4c2c72：遍历玩家表
        for row in modules.player_rows(players) {
            // 0x4c2d4a → 0x4c2d5c
            let ident = modules.player_ident(row);
            // 0x4c2d75 cmp dword [rcx+rsi],eax —— 命中就返回 true（0x4c2db9）
            if local_ids.contains(&(ident as u32)) {
                return true;
            }
        }
        // 0x4c2e03 xor ebx,ebx
        false
    }

    /// `GraphwarSetupControl::update_name` —— **x86 `0x4c1e40`**。
    ///
    /// 关联函数（无 `self`）：`(index: i32, label: Gd, modules: &Rc<RefCell<ModuleContainer>>)`。
    /// 实参形态由**调用点**读出（`0x4d3ff5` 一族：`edi=r14d`、`rsi=rbp`、`rdx=r13`、
    /// `rcx=[[rsp+0x68]+0x238]`）。
    ///
    /// ```text
    /// 4c1e61..4c1e8c  borrow() + get::<GraphwarNameManager>()(0x46ba20)，None ⇒ panic 0x4c207b
    /// 4c1e9e/4c1ea8   test r14/r15                    ; 空 Gd 跳过 clone
    /// 4c1ebe call 0x4066c0                            ; Gd 包装 B，"clone"(5 B)
    /// 4c1ed2 call 0x447e20                            ; upcast_ref；4c1ed7 cmp [rsp+0x10],1 ⇒ 失败臂
    /// 4c2096 lea <"Upcast failed…">(0x1c2a4b) + esi=0x2f  ; panic 47 字节文案
    /// 4c1ef3 call 0x3ac770 + "PlayerNameLabel"(15, r8d=0xf @0x4c1ef8)  ; get_node_by_name
    /// 4c1f37 cmp qword [r14+0x48],0 / je 0x4c2047     ; NameManager 的表为空 ⇒ 收尾
    /// 4c1f4b call 0x47dec0                            ; 对 index 求哈希
    /// 4c1fa6 cmp ebx,[rsi-0x20]                       ; 键比较（键在桶里 −0x20）
    /// 4c1fe2 call *[0x9ba8f0] = 0x91e260              ; Rc::clone 查到的值
    /// 4c2019 call 0x4061a0                            ; Gd 包装，"upcast_ref"(10 B)
    /// 4c2028 call 0x3ec130(rdi=&标签, rsi=&值)        ; 交给标签（引擎侧成员名未解）
    /// ```
    ///
    /// 档位：**观测**（顺序、字面量、偏移、panic 槽）。`0x3ec130` / `0x4061a0` 的
    /// 引擎侧成员名**未解**，只落"调了它 + 实参是什么"。
    pub fn update_name<E: SetupEngine + ?Sized, M: SetupModules + ?Sized>(
        engine: &mut E,
        modules: &mut M,
        index: i32,
        label: GdHandle,
    ) {
        // 0x4c1e40  —— 本函数起点；FDE 0x4c1e40..0x4c213e（0x2fe = 766 B）
        // 0x4c1e84 call 0x46ba20 —— 第一个动作就是取 NameManager
        let names = modules.name_manager().expect(MSG_OPTION_UNWRAP); // panic 槽 0x4c207b
        // 0x4c1e9e / 0x4c1ea8：Gd 非空才 clone（`test r14,r14` + `test r15,r15`）
        let mut owned = label;
        if !label.is_null() && label.rtti != 0 {
            owned = engine.gd_clone(label).expect(MSG_OPTION_UNWRAP); // 0x4c1ebe call 0x4066c0
        }
        // 0x4c1ed2：upcast_ref；失败臂 0x4c208a → panic(47 字节文案)
        let up = engine.upcast_ref(owned).expect(MSG_UPCAST_FAILED);
        // 0x4c1ef3：节点名 "PlayerNameLabel"（15 B @0x1c2d52）
        let node = engine
            .get_node_by_name(up, NODE_NAME_LITERALS[0].0)
            .expect(MSG_OPTION_UNWRAP); // 0x4c20bd
        // 0x4c1f37..0x4c1fd9：按 index 查 NameManager 的表；查不到就什么都不做
        if let Some(row) = modules.name_row(names, index) {
            // 0x4c1fe2 Rc::clone → 0x4c2019 upcast_ref → 0x4c2028 交给标签
            engine.gd_touch(node);
            modules.apply_name_row(node, row);
        }
        // 0x4c2047 起：收尾（解除 borrow、掉引用、drop）
    }

    /// `GraphwarSetupControl::update_rank` —— **x86 `0x4c2140`**。
    ///
    /// 关联函数：`(index: i32, node: Gd, modules: &Rc<..>, rank_id: u64)` ——
    /// 第 4 项从 `r8` 进来（调用点 `0x4d3a5f mov r8,[rsp+0xa0]`）。
    ///
    /// ```text
    /// 4c2168..4c2196  borrow() + get::<GraphwarRankManager>()(0x46bfe0)，None ⇒ panic 0x4c26de
    /// 4c21b7/4c21c8   Gd 包装 B "clone"(5 B)，随后 4c21dc call 0x447e20 = upcast_ref
    /// 4c21f6 call 0x3acb00 + "RankIcon"(8 B @0x19fb70, r8d=8 @0x4c2202)
    /// 4c2255 call 0x47dec0                            ; rank 值哈希 + hashbrown 探测
    /// ---- 3 段同形块（每段：upcast_ref → to_godot → set_texture → gd_touch）----
    /// 4c22e9 "upcast_ref"(10) / 4c2309 0x41ce80 / 4c231e "to_godot"(8) 0x407580
    /// 4c235f "set_texture"(11) 0x3a01a0 / 4c23bd 0x405d20
    /// 4c2493 / 4c24b3 / 4c24c8 / 4c2509 / 4c255f     ; 第 2 段
    /// 4c25e2 / 4c25cd / 4c25f8 / 4c2639 / 4c2697     ; 第 3 段
    /// ```
    ///
    /// 档位：**观测**（3 段的存在与每段的字面量顺序）。**推断**：为什么是 3 段
    /// （缺：`RankIcon` 有 3 层贴图 还是 3 个段位档 —— 需要真机读 `RankIcon` 的子节点）。
    pub fn update_rank<E: SetupEngine + ?Sized, M: SetupModules + ?Sized>(
        engine: &mut E,
        modules: &mut M,
        rank_id: u32,
        node: GdHandle,
    ) {
        // 0x4c2140  —— 本函数起点；FDE 0x4c2140..0x4c27e7（0x6a7 = 1703 B）
        let ranks = modules.rank_manager().expect(MSG_OPTION_UNWRAP); // 0x46bfe0
        let mut owned = node;
        if !node.is_null() && node.rtti != 0 {
            owned = engine.gd_clone(node).expect(MSG_OPTION_UNWRAP); // 0x4c21b7
        }
        let up = engine.upcast_ref(owned).expect(MSG_UPCAST_FAILED); // 0x4c21dc
        let icon = engine
            .get_node_by_name(up, "RankIcon")
            .expect(MSG_OPTION_UNWRAP); // 0x4c21f6
        // 4c2255：先按 rank 值查一次
        let _ = modules.rank_row(ranks, rank_id);
        // 3 段同形块（4c22e9 / 4c2493 / 4c25e2）
        for _layer in 0..RANK_ICON_LAYERS {
            let tex = engine
                .upcast_ref(icon)
                .expect(MSG_UPCAST_FAILED); // "upcast_ref"(10)
            engine.gd_touch(tex); // 0x41ce80 / 0x405d20
            let gd = engine.to_godot(tex).expect(MSG_OPTION_UNWRAP); // "to_godot"(8)
            engine.set_texture(icon, gd); // "set_texture"(11)
        }
    }

    /// `GraphwarSetupControl::update_soldiers` —— **x86 `0x4c2ef0`**（本屏最大的函数，`0x15b3` = 5555 B）。
    ///
    /// 关联函数：`(index: i32, node: Gd, modules: &Rc<..>, extra: u64)`（第 4 项从 `r8`）。
    ///
    /// ```text
    /// 4c2f1b..4c2f4e  borrow() + get::<GraphwarSoldierManager>()(0x46a770)，None ⇒ panic 0x4c419a
    /// 4c2f90 call 0x46b8b0   get::<GraphwarTeamManager>()
    /// 4c2fdd call 0x46a8e0   get::<GraphwarGraphicsManager>()
    /// 4c3029 call 0x3c2120   GraphwarTeamManager::get_team
    /// 4c3086 call 0x3bf230   （与 is_leader_local 同一个被调者）
    /// 4c30e6 call 0x3adc50 + "SoldierIconContainer"(20 B @0x1c2da6, r8d=0x14 @0x4c30f2)
    /// 4c337b "SkinIcon"(8 B @0x19f3d0, r8d=8 @0x4c3370)    → 4c33b5 upcast_ref / 4c3608 to_godot / 4c3653 set_texture
    /// 4c3734 "FaceIcon"(8 B @0x19f180, r8d=8 @0x4c3729)    → 同三步
    /// 4c3aef "HatIcon"(7 B @0x1c2dba, r8d=7 @0x4c3ae4)     → 同三步
    /// 4c3ebf "EmptyIcon"(9 B @0x1c2dc1, r8d=9 @0x4c3eb4)  → 同三步
    /// ```
    ///
    /// 档位：**观测**（4 个图标名 + 长度、3 个 `get` 的顺序、`get_team`/`0x3bf230` 的被调）。
    /// **推断**：每个图标应不应该画由槽占用决定 —— 槽数 5 是逐位（[`MAX_SOLDIERS_PER_PLAYER`]），
    /// "占用则画 Skin/Face/Hat、否则画 Empty" 这一层是**推断**（缺：真实局面下 4 个图标的显示组合观测）。
    pub fn update_soldiers<E: SetupEngine + ?Sized, M: SetupModules + ?Sized>(
        engine: &mut E,
        modules: &mut M,
        team_id: u32,
        node: GdHandle,
    ) {
        // 0x4c2ef0  —— 本函数起点；FDE 0x4c2ef0..0x4c44a3（0x15b3 = 5555 B）
        let soldiers = modules.soldier_manager().expect(MSG_OPTION_UNWRAP); // 0x46a770
        let teams = modules.team_manager().expect(MSG_OPTION_UNWRAP); // 0x46b8b0
        let _graphics = modules.graphics_manager().expect(MSG_OPTION_UNWRAP); // 0x46a8e0
        let team = modules
            .get_team(teams, team_id)
            .expect(MSG_OPTION_UNWRAP); // 0x3c2120
        let mut owned = node;
        if !node.is_null() && node.rtti != 0 {
            owned = engine.gd_clone(node).expect(MSG_OPTION_UNWRAP);
        }
        let up = engine.upcast_ref(owned).expect(MSG_UPCAST_FAILED);
        let container = engine
            .get_node_by_name(up, "SoldierIconContainer")
            .expect(MSG_OPTION_UNWRAP); // 0x4c30e6
        // 4 类图标，顺序即 0x4c2ef0 里 get_node 的字面量顺序。
        //
        // ⚠️ **不编语义**：4 段在反汇编里**没有**"按槽占用跳过"的分支证据（缺：真机在
        // "有空槽 / 满槽 / 0 兵"三种局面下这 4 个节点的可见性观测），所以这里**无条件**走完
        // 4 段，只把槽占用读出来留给调用方 —— 这比编一条 `if` 更诚实。
        let _occupied = modules.soldier_slots(soldiers, team);
        for name in SOLDIER_ICON_ORDER {
            let icon = engine
                .get_node_by_name(container, name)
                .expect(MSG_OPTION_UNWRAP);
            let tex = engine.upcast_ref(icon).expect(MSG_UPCAST_FAILED);
            engine.gd_touch(tex);
            let gd = engine.to_godot(tex).expect(MSG_OPTION_UNWRAP);
            engine.set_texture(icon, gd);
        }
    }

    /// `GraphwarSetupControl::connect_entry_buttons` —— **x86 `0x4c4c20`**。
    ///
    /// 关联函数：`(obj: usize, index: i32, a: u64, b: u64)` —— 第 1 项是**裸对象指针**
    /// （`0x4c4c33 mov rsi,rdi` 把它转手给 `0x4b1640` 做成 `Gd`）。
    ///
    /// ```text
    /// 4c4c63 call 0x4b1640                        ; Gd::from_obj_sys(rdi) → 两字出参
    /// 4c4c7c/4c4c8d "clone"(5 B) 0x4066c0         ; 两个分量都非空才 clone
    /// 4c4ca5 call 0x447e20 = upcast_ref
    /// 4c4cbf call 0x3ab590 + "GraphwarRootNode"(16 B @0x19e490, r8d=0x10 @0x4c4ccb)
    /// 4c4d00 call 0x47fb90 / 4c4d0d call 0x4095a0 ; 根节点那一支的接线
    /// ---- 4 段同形块（每段：clone/upcast → get_node → 0x3ad570 → 0x4098f0）----
    /// 4c4d75..4c4dcf "ChangeTeamButton"(16 B @0x19e030, r8d=0x10 @0x4c4dc9)  → 4c4e28 0x4098f0
    /// 4c4f0e..4c4f56 "RemovePlayerButton"(18 B @0x1c2de5, r8d=0x12 @0x4c4f62) → 4c4fba 0x4098f0
    /// 4c508e..4c50d6 "AddSoldierButton"(16 B @0x19e740, r8d=0x10 @0x4c50e2)   → 4c513a 0x4098f0
    /// 4c520e..4c5256 "RemoveSoldierButton"(19 B @0x1c2df7, r8d=0x13 @0x4c5262)→ 4c52ba 0x4098f0
    /// ```
    ///
    /// 档位：**观测**（4 个按钮字面量 + 长度 + 4 段的存在 + 每段的被调地址）。
    /// **推断**：每段连到哪个 `*_request_event` 构造器 —— 依据是 `docs/16-client.md` §17.6
    /// 记的"本函数经 `.data.rel.ro` 槽引用 A 组 8 张表"，**缺**按钮 → 构造器的直接跳转证据
    /// （`connect` 的回调是函数指针，不在 `call rel32` 里）。
    pub fn connect_entry_buttons<E: SetupEngine + ?Sized>(
        engine: &mut E,
        obj: usize,
        _index: i32,
        _a: u64,
        _b: u64,
    ) {
        // 0x4c4c20  —— 本函数起点；FDE 0x4c4c20..0x4c5b43（0xf23 = 3875 B）
        // 0x4c4c63：裸对象指针 → Gd（出参两字；`0x4c4c6d`/`0x4c4c77` 各判一次非空）
        let root = engine.gd_from_obj(obj).expect(MSG_OPTION_UNWRAP);
        let mut owned = root;
        if !root.is_null() && root.rtti != 0 {
            owned = engine.gd_clone(root).expect(MSG_OPTION_UNWRAP); // 0x4c4c8d
        }
        let up = engine.upcast_ref(owned).expect(MSG_UPCAST_FAILED); // 0x4c4ca5
        let base = engine
            .get_node_by_name(up, "GraphwarRootNode")
            .expect(MSG_OPTION_UNWRAP); // 0x4c4cbf
        engine.gd_touch(base); // 0x4c4d00
        engine.connect_button(base, base); // 0x4c4d0d → 0x4095a0
        for name in ENTRY_BUTTONS {
            let button = engine
                .get_node_by_name(base, name)
                .expect(MSG_OPTION_UNWRAP);
            engine.gd_touch(button); // 0x3ad570 之后的引用计数包装
            engine.connect_button(button, button); // 0x4098f0
        }
    }

}

// ---------------------------------------------------------------------------
// 10.3 三条 gdext 宏产物 / 未定的（体是观测、名可能未定）
// ---------------------------------------------------------------------------

/// `graphwar_setup_control::_::__init::__inner_init` 的**调用序列**（`0x4bd030` 与 `0x4bd280` 共用）。
///
/// 判据（**观测**，两份各自的指令）：
/// * 读 `.data` 静态 `0x9c0dc0`（`mov eax,dword [rip+0x503d18]`）+ `0x9c0dc4`（`movzx ebp,word`）
///   —— 是**同一块**登记存储；
/// * 调 `0x4ffc96`（把两个内嵌函数指针常量装进登记项）→ `0x4da1e0` → `0x4da440`（gdext 胶水）。
/// 两份唯一的差别就是内嵌的那两个函数指针常量（`0x4bd030`: `0x40d3a0`/`0x415560`；
/// `0x4bd280`: `0x40dac0`/`0x40e6c0`）⇒ 它们是**不同接口**的 `__init`。
pub const INNER_INIT_SEQUENCE: &[(u64, u64, &str)] = &[
    (0x4b_d0a2, 0x9c_0dc0, "读登记存储（dword）"),
    (0x4b_d0b0, 0x9c_0dc4, "读登记存储（word）"),
    (0x4b_d25e, 0x4d_a1e0, "gdext 胶水 A"),
    (0x4b_d26e, 0x4d_a440, "gdext 胶水 B"),
];

/// `__init::__inner_init` 的**源码层对应物** —— gdext 的 `#[godot_api] impl I*` 块里
/// 用户写的 `fn init(base: Base<Self>)`。
///
/// 一个源函数 → 机器层的**多份** `__init::__inner_init`（每个 `#[godot_api] impl I*` 块一份）：
///   * **x86 `0x4bd030`**（FDE `0x4bd030..0x4bd27f`，`0x24f` = 591 B；唯一 Location `608:1`）
///   * **x86 `0x4bd280`**（FDE `0x4bd280..0x4bd426`，`0x1a6` = 422 B；Location `95:1`）
///   * **x86 `0x4bcc60`**（Location `46:10`，`fixtures/name_to_addr_v2.tsv` 唯一锚）
/// 这三条在 x86 侧**地址连续**，arm64 侧也正好有连续 4 份 `__init::__inner_init`
/// （448/464/436/472 B，`tools/arm64_sym.py mod graphwar_setup_control`）⇒ 是同一族的多次单态化。
///
/// 档位：**观测**（三份各自的 FDE + 调用序列 + 内嵌函数指针常量）；
/// **推断**（"哪一份对应哪个 `impl I*` 块" —— 缺 gdext 版本的 vtable 槽序对齐）。
/// 按 `tools/function_coverage.py` 的 R3（"gdext 宏产物在源码层的对应物是用户写的 fn"）
/// 落成 `fn init`，不假装它是手写函数。
impl SetupControlIntent {
        pub fn init(registration: &mut dyn SetupEngine, init_a: u64, init_b: u64) {
        // 0x4bd030 / 0x4bd280 / 0x4bcc60 —— 本 fn 对应的三条机器函数
        // 0x4bd0a2 / 0x4bd0b0：读登记存储的两个字（.data 静态 0x9c0dc0 / 0x9c0dc4）
        let slot = GdHandle::new(init_a as usize, init_b as usize);
        // 0x4bd214 call 0x4ffc96（装项）→ 0x4bd25e 0x4da1e0 → 0x4bd26e 0x4da440（gdext 胶水）
        registration.gd_touch(slot);
        }
}

/// `0x4bd030` 内嵌的两个函数指针常量（`.text` 里的 `lea` 目标）。
/// 档位：**逐位**（`0x4bd11b`/`0x4bd127` 的 `lea rdx,[rip-X]`）。
pub const INNER_INIT_V1_PTRS: (u64, u64) = (0x40_d3a0, 0x41_5560);

/// `0x4bd280` 内嵌的两个函数指针常量。档位：**逐位**（`0x4bd303`/`0x4bd30f`）。
pub const INNER_INIT_V2_PTRS: (u64, u64) = (0x40_dac0, 0x40_e6c0);

/// `0x4bce80`（候选 `__register_constants`）的落码体：**`Once` 式惰性初始化**。
///
/// ```text
/// 4bce90 lock cmpxchg dword [0x9c0e80], ecx   ; 抢一次执行（状态字）
/// 4bceb6 movzx eax,byte [0x9c0e84]            ; 完成标志
/// 4bcec5..4bcefe 两张注册表：{0x9c0e88 槽, 0x9c0e98}, {0x9c0ea0 槽, 0x9c0eb0}
/// 4bcee9 lea rcx,[0x4d98d0] / 4bcf22 lea rcx,[0x4d9820]   ; 装进去的两个函数指针
/// 4bcfe5 mov byte [0x9c0e84],1                ; 置完成
/// 4bd007 call 0x4da330 / 4bd017 call 0x4da440 ; gdext 胶水
/// ```
///
/// 档位：**观测**（体）/ **未知**（名：4 选 1 歧义）。
pub fn register_constants_candidate(
    engine: &mut dyn SetupEngine,
    done: &mut bool,
    ptr_a: u64,
    ptr_b: u64,
) {
    // 0x4bce80  —— 本函数起点；FDE 0x4bce80..0x4bd028（0x1a8 = 424 B）
    // 0x4bce90：抢执行（本层用 `done` 表达"已完成就跳过"）
    if *done {
        return;
    }
    // 0x4bcec5..0x4bcf30：两张注册表各装一个函数指针
    engine.gd_touch(GdHandle::new(ptr_a as usize, 0x9c_0e88));
    engine.gd_touch(GdHandle::new(ptr_b as usize, 0x9c_0ea0));
    // 0x4bcfe5 mov byte [0x9c0e84],1
    *done = true;
    // 0x4bd007 / 0x4bd017
    engine.gd_touch(GdHandle::new(0x9c_0e80, 0x9c_0e84));
}

/// `0x4bce80` 装进两张注册表的两个函数指针。档位：**逐位**（`0x4bcee9` / `0x4bcf22`）。
pub const REGISTER_CONSTANTS_PTRS: (u64, u64) = (0x4d_98d0, 0x4d_9820);

/// `0x4d88b0`（候选 `ready`）用到的客户端内层字段偏移。
///
/// 判据（**观测**）：`0x4d88b4 mov r14,[rdi]` → `0x4d88bb cmp qword [r14+0x10],0`
/// （`RefCell` 借用标志）→ `0x4d88ca cmp qword [r14+0x1f8],0`。
/// `+0x1f8` 与 `graphwar_game_client` 那 8 个请求构造器取的**同一个**内层单元
/// （`0x4d8a28 mov rbx,[r14+0x1f8]`）。
pub const READY_CANDIDATE_INNER_FIELD: u64 = 0x1f8;

/// 原版 `0x4d88b0` 里唯一的一跳：`call *[0x9bc160]` = **`0x51_7490`**。
///
/// 档位：**逐位**（GOT 槽 `0x9bc160` 的 `R_X86_64_RELATIVE` addend = `0x51_7490`）。
pub const FN_REQUEST_LOCK_CHANGE: u64 = 0x51_7490;

/// `0x4d88b0` 的落码体。
///
/// 档位：**观测**（体）/ **未知**（名：`ready` 2 选 1 歧义）。
/// 被调者已知：`GraphwarGameClient::request_lock_change`（地址见常量
/// [`FN_REQUEST_LOCK_CHANGE`]；这里不重复写地址字面量，否则审计工具会把
/// "我引用了它" 误判成 "我实现了它" —— `tools/function_coverage.py` 的 P2 就是这么判的）。
pub fn ready_candidate<C: LockChangeClient + ?Sized>(client: &mut C) {
    // 0x4d88b0  —— 本函数起点；FDE 0x4d88b0..0x4d8957（0xa7 = 167 B）
    // 0x4d88bb cmp qword [r14+0x10],0 / jne 0x4d891c —— 已被可变借用 ⇒ panic
    // 0x4d88c2 mov qword [r14+0x10],-1         —— 置"已可变借用"
    // 0x4d88ca cmp qword [r14+0x1f8],0 / je    —— 内层为空 ⇒ 什么都不做
    if let Some(inner) = client.inner_unit() {
        // 0x4d88db call *[0x9bc160] → FN_REQUEST_LOCK_CHANGE（值见该常量的定义）
        client.request_lock_change(inner);
    }
    // 0x4d88e9..0x4d88ec：解除借用
}

/// `ready_candidate` 需要的客户端句柄（原版是 `Rc<RefCell<GraphwarGameClient 内层>>`）。
pub trait LockChangeClient {
    /// 内层单元（`[env+0x1f8]`）；`None` = 原版那处 `je 0x4d88fd` 走"什么都不做"。
    fn inner_unit(&mut self) -> Option<()>;
    /// `GraphwarGameClient::request_lock_change`（地址见 [`FN_REQUEST_LOCK_CHANGE`]）。
    fn request_lock_change(&mut self, inner: ());
}

// ---------------------------------------------------------------------------
// 10.4 单测：期望值全部来自反汇编（.rodata 字面量 / FDE / 分支），不来自本实现
// ---------------------------------------------------------------------------

#[cfg(feature = "gdext-bindings")]
mod node_class {
// ---------------------------------------------------------------------------
// 11. 原版节点类 `GraphwarSetupControl`（T63 从 `src/_scaffold/graphwar_setup_control_class.rs` 搬回）
// ---------------------------------------------------------------------------
//
// 原版把 `#[derive(GodotClass)]` 的 struct 与方法体**写在同一个 `graphwar_setup_control.rs` 里**，
// 证据是 `panic::Location`：`_::__init::__inner_init 0x4bcc60` 落本文件 `46:10`
// （`fixtures/name_to_addr_v2.tsv` 唯一锚），`ready 0x4cf670`/`on_text_change 0x4c27f0`/
// `on_chat_submitted 0x4c44b0`/`force_title_update 0x4c4850`/`connect_entry_buttons 0x4c4c20`
// 的 Location 也全落本文件。我方原来把「纯逻辑」与「节点类」拆在两个文件里 —— 是我方分层，
// 不是原版的（`AGENTS.md` §4）。T57 §104.0 点名的「装着原版模块方法体的 `*_class.rs` 只剩 2 个」
// 就是本文件与 `graphwar_ad_switch_class.rs`，T63 把这两个都搬完了。
//
// **本仓唯一的结构差异（诚实登记）**：原版整个 crate 都是 gdext 扩展，不需要 `cfg`；本仓
// `cargo test`（不带 `gdext-bindings`）必须能编 ⇒ 这一层挂在
// `#[cfg(feature = "gdext-bindings")] mod node_class` 下，由 `pub use node_class::*;`
// 抬平对外路径 ⇒ `crate::graphwar_setup_control::GraphwarSetupControl` 与原版一致。
//
// **只搬位置、一字未改语义**。搬的时候发现两条**前提过时**（只登记，不改行为；见 `docs/06` §111）：
// 1. `build_stub_children` 的文件头写「`client/` 里没有 GameSetup 这一屏的场景」—— T43 已把
//    原版 `game_setup.tscn` 装进 `client/scenes/screens/`（节点 `GraphwarSetupControl/RoomName/
//    RoomNameLineEdit` 由场景提供）⇒ 替身子节点是否还该建 = **待定**（改它会动屏幕像素，不在本轮范围）；
// 2. `join_text` 与 `src/graphwar_button.rs` 里那份**逐字符相同**（台账 `scaffold_triage.tsv` 的
//    C 档）—— 本轮只搬位置，不合并（合并要动 `graphwar_button` 的可见性，属另一条改动）。

//! **不是原版的东西**：把复原的 [`crate::graphwar_setup_control`] 挂成**真 Godot 节点类**的绑定外壳。
//!
//! # 为什么会有这一份
//!
//! 原版 `GraphwarSetupControl` 是 `Control` 的子类（`IControl`），负责 **GameSetup 这一屏**：
//! 四格模式（轴 / 函数 / 回合 / 时限）、玩家列表、Start / Cancel、聊天。
//! `src/graphwar_setup_control.rs` 已经复原了它的**取值层**（`SetupOptions` / `TimeMode` /
//! `PlayerEntry` / `start_action`），但按约定**不碰引擎**。本文件是薄壳：
//!
//! ```text
//! src/_scaffold/graphwar_setup_control_class.rs   搬运前：纯逻辑不在这里，只有节点壳
//! src/graphwar_setup_control.rs                   搬运后（T63）：纯逻辑 + 本 `mod node_class`
//! ```
//!
//! ⚠️ **T63 搬完之后这里不再分两份**：原版就是一个 `graphwar_setup_control.rs`（`#[derive(GodotClass)]`
//! struct + 方法体 + 纯逻辑），下方的节点类是本文件的一部分。
//!
//! # 关于那 21 / 18 / 6 个数（先看清楚本文件**没有**做什么）
//!
//! 原版本模块有 **37** 个带 `Location` 锚的函数；矩阵压平跨模块后记 **21** 个"可归属"。
//! 其中 **18 个是 gdext 薄壳**（`0x4c5b50`…`0x4d96e0`，`0x60–0x140` 字节、体内无字符串），
//! 另有 **6 个**是"虚方法/属性胶水"（`0x4b88d0`…`0x4bd280`，只调 gdext 胶水）——
//! 这 **24 个的语义都未知**（`src/graphwar_setup_control.rs` 模块头第 1 节的两张表）。
//!
//! ⇒ 本文件**不声称**还原了它们。它只做两件有证据的事：
//!
//! 1. [`graphwar_setup_control::FN_SETUP_WIRING`] 里**观测到**的 3 条 `connect`；
//! 2. 四格 / 玩家条目 / Start 那几类**取值逻辑**（纯逻辑文件里已有）挂上节点。
//!
//! ⚠️ 原版**没有**给 4 个模式按钮连 `pressed`（模块头"没解出来的"第 1 条，**观测**）——
//! 所以本文件也**不连**，只提供 `#[func] cycle_*_mode()` 让宿主（或命令总线）来调。
//! 这是"照证据留白"，不是"忘了接"。
//!
//! # 本轮要验的三种机制（结论见 `docs/16-client.md` §35）
//!
//! | # | 机制 | 原版形态 | 本文件怎么用 |
//! |---|---|---|---|
//! | 1 | **自造 `#[signal]`** | 无 | `#[signal] options_changed(4×int)` / `start_requested()` / `exit_requested()` / `chat_message(text)`，用**类型安全生成器**发 |
//! | 2 | **连引擎原生信号**（**别人的**） | `ready` 里 3 条 `connect`，信号来自 **`get_node` 找到的子节点**（[`graphwar_setup_control::FN_SETUP_WIRING`]） | `find_child(name)` → `node.connect(signal, callable(self, handler))` |
//! | 3 | **外部注册闭包表** | **本模块没有**（纯逻辑文件里 0 处 `FnMut`/`Box<dyn>`） | **不伪造**（见 §39.4 的表） |
//!
//! # 与原版的差别（逐条）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 基类 / trait | `Control` + `IControl` | 同 | **观测** |
//! | 11 个 `get_node` 的实参 | `RoomNameLineEdit`/`AxisModeButton`/…/`ChatLineEdit` | 只认 3 个 **有连接的**（另外 8 个的名字记在 `STUB_NODE_NAMES` 里但**不建**） | 名字**观测**；子集是**我方选择** |
//! | 子节点从哪来 | `.tscn` 里现成的 | [`GraphwarSetupControl::build_stub_children`] 自己建（`client/` 没有这一屏的场景） | **替代** |
//! | 3 条 `connect` | `text_changed`/`visibility_changed`/`text_submitted` → 三个 `#[func]` | 同（先按名递归找节点，找不到就跳过并记账） | **观测** |
//! | 4 个模式按钮的点击 | **没连** `pressed`（走按钮 `input_text` → 命令总线的**推断**） | **也不连**；只给 `#[func] cycle_*_mode()` | 未解（`docs/16` §16 未知项） |
//! | `force_title_update` | 方法名来自 `docs/16` §34.6 的类表（`#[func]`），**写哪个节点未知** | 只把标题字符串算出来存字段；不写任何节点 | 行为**未知** |
//! | `FN_EVENT_CALLBACKS` 三个回调 | 职责**未知** | **不实现** | **未知** |
//! | 玩家列表 5 个行构件 | `0x4d3180` 重建 + 4 个按钮 | 只做**数据层**（`PlayerEntry` 的增删改），不建行节点 | 数据**已复原**；视图**未做** |
//!
//! # 怎么打开 / 契约
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/signal_classes_check.gd
//! ```
//!
//! ```text
//! GraphwarSetupControl : Control
//!   signal options_changed(axis: int, function: int, turns: int, time: int)
//!   signal start_requested()
//!   signal exit_requested()
//!   signal chat_message(text: String)
//!
//!   build_stub_children() -> int        # 建 3 个替身子节点，返回建了几个
//!   wired_count() -> int                # ready 里真连上了几条（期望 3）
//!   on_text_change(name) / on_visibility_change() / on_chat_submitted(text) / force_title_update()
//!   cycle_axis_mode() / cycle_function_mode() / cycle_turns_mode() / cycle_time_mode() -> int
//!   axis_mode() / function_mode() / turns_mode() / time_mode() -> int
//!   options_names() -> String           # 四格的线上名字（`|` 分隔）
//!   options_icons() -> String           # 四格的图标路径（`|` 分隔）
//!   timer_label() -> String             # 无限档返回 "inf"
//!   has_turn_clock() -> bool
//!   room_name() / title_text() / chat_log() -> String
//!   add_human(name, team, soldiers) / add_bot(level, name, team, soldiers) -> int
//!   add_soldier(i) / remove_soldier(i) / change_team(i) -> bool
//!   entry_count() -> int
//!   entries_text() -> String            # `name:team:soldiers[:bot]`，`|` 分隔
//!   start() -> bool                     # 真开始返回 true；静默无操作返回 false（原版是纯静默）
//!   exit() -> bool
//!   ready_call_count() / text_change_count() / visibility_change_count() -> int
//!   chat_submitted_count() / title_update_count() / options_changed_count() -> int
//! ```
//!
//! # 三条要注意的
//!
//! 1. **信号来自子节点**（与前两个类不同）：先 `find_child` 再 `connect`，
//!    找不到就**不 panic**（原版 `get_node` 找不到会报错，但那是场景缺节点的错）。
//! 2. **`set_text` 不会发 `text_changed`**（Godot 4 只在用户编辑时发）⇒ 验收脚本用
//!    `emit_signal` 充当引擎那一侧；这不影响"信号真的连上了"这条结论。
//! 3. **`SetupAction::SilentNoOp` 是原版**：没对手时 Start **什么都不做、也不提示**
//!    （`docs/16` §2.1.5 观测）。本类的 `start()` 就返回 `false` 且**不**发信号。

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::{Control, IControl, LineEdit};
use godot::obj::WithUserSignals;
use godot::prelude::*;

use crate::graphwar_level_manager::{AxisMode, FunctionMode, TurnMode};
use crate::graphwar_setup_control::{
    self as setup, PlayerEntry, SetupAction, SetupOptions, Team, TimeMode, ROOM_DEFAULT_OPTIONS,
};

/// 原版 `FN_SETUP_WIRING` 里 `get_node` 的 11 个名字（`0x19ea20`…`0x1c39fe`）。
///
/// 本文件只**建**其中 3 个有连接的（见 [`STUB_NODE_NAMES`]）；另外 8 个名字留在这里
/// 是为了让"我们建了哪几个"可核对，不是"原版只有 3 个"。
pub const ALL_GET_NODE_NAMES: [&str; 11] = [
    "RoomNameLineEdit",
    "AxisModeButton",
    "FunctionModeButton",
    "TurnModeButton",
    "TurnTimeButton",
    "LockButton",
    "ChatButtonSetup",
    "ChatButtonPlay",
    "ChatPopUp",
    "ChatBox",
    "ChatLineEdit",
];

/// 本文件真的建出来的替身节点（**我方选择**：只建有连接的那几个）。
pub const STUB_NODE_NAMES: [&str; 3] = ["RoomNameLineEdit", "ChatPopUp", "ChatLineEdit"];

/// GameSetup 这一屏的控制器。
///
/// 值全在 [`SetupOptions`] / `Vec<PlayerEntry>` 里（纯逻辑）；本结构只做"信号 → 纯逻辑 → 信号"。
pub struct GraphwarSetupControlStateMarker;

/// GameSetup 这一屏的控制器（`Control` 子类）。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct GraphwarSetupControl {
    base: Base<Control>,

    /// 四格模式（`GameInfo` 的 4 个字段）。
    options: SetupOptions,

    /// 玩家条目（原版由 `FN_TEAM_LIST_REBUILD` `0x4d3180` 重建）。
    entries: Vec<PlayerEntry>,

    /// 房间名（`RoomNameLineEdit.text_changed` 来的）。
    room_name: GString,

    /// 标题字符串（`force_title_update` 的产物；原版写哪个节点**未知**）。
    title: GString,

    /// 聊天记录（[`ChatMessage`] 的落点）。
    chat_log: Vec<String>,

    /// `_ready` 里真连上了几条信号（期望 = 3）。
    wired: i64,

    /// 没找到 / 连不上的节点名（记账，不 panic）。
    missing: Rc<RefCell<Vec<String>>>,

    ready_calls: i64,
    text_change_calls: i64,
    visibility_change_calls: i64,
    chat_submitted_calls: i64,
    title_update_calls: i64,
    options_changed_calls: i64,
}

/// 一条聊天消息（`chat_message` 信号的载荷在 GDScript 侧是 `String`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage(pub String);

#[godot_api]
impl IControl for GraphwarSetupControl {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            options: ROOM_DEFAULT_OPTIONS,
            entries: Vec::new(),
            room_name: GString::new(),
            title: GString::new(),
            chat_log: Vec::new(),
            wired: 0,
            missing: Rc::new(RefCell::new(Vec::new())),
            ready_calls: 0,
            text_change_calls: 0,
            visibility_change_calls: 0,
            chat_submitted_calls: 0,
            title_update_calls: 0,
            options_changed_calls: 0,
        }
    }

    /// `_ready`（原版 `0x4cf670` 的接线部分）：**机制 2** —— 连**子节点**的原生信号。
    ///
    /// 原版三条（`docs/16` §16 的 `0x4cf670` 段）：
    ///
    /// ```text
    /// RoomNameLineEdit.text_changed   → on_text_change
    /// ChatPopUp.visibility_changed    → on_visibility_change
    /// ChatLineEdit.text_submitted     → on_chat_submitted
    /// ```
    ///
    /// 找节点用 `find_child`（递归）—— 对应原版 `GraphwarNodes::get_node_by_name_rec`。
    /// 找不到就记进 `missing` 并跳过（原版 `get_node` 会报错，但那是**场景缺节点**的错，
    /// 不是本类的逻辑）。
    fn ready(&mut self) {
        self.ready_calls += 1;
        let wiring: [(&str, &str, &str); 3] = [
            ("RoomNameLineEdit", "text_changed", "on_text_change"),
            ("ChatPopUp", "visibility_changed", "on_visibility_change"),
            ("ChatLineEdit", "text_submitted", "on_chat_submitted"),
        ];

        let myself = self.to_gd();
        for (node_name, signal, handler) in wiring {
            let Some(node) = self.base().find_child_ex(node_name).owned(false).done() else {
                self.missing.borrow_mut().push(node_name.to_owned());
                continue;
            };
            let callable = myself.callable(handler);
            let err = node.clone().connect(signal, &callable);
            if err == godot::global::Error::OK {
                self.wired += 1;
            } else {
                self.missing
                    .borrow_mut()
                    .push(format!("{node_name}.{signal}: {err:?}"));
            }
        }
    }
}

#[godot_api]
impl GraphwarSetupControl {
    /// **机制 1**：自造的、四个 `int` 参数的信号（四格模式整组变了）。
    #[signal]
    fn options_changed(axis: i64, function: i64, turns: i64, time: i64);

    /// **机制 1**：自造的、无参信号（`SetupAction::StartMatch` 时发）。
    #[signal]
    fn start_requested();

    /// **机制 1**：自造的、无参信号（返回键）。
    #[signal]
    fn exit_requested();

    /// **机制 1**：自造的、带 `String` 的信号（聊天）。
    #[signal]
    fn chat_message(text: GString);

    /// 建 3 个**替身**子节点（`client/` 里没有 GameSetup 这一屏的场景）。
    ///
    /// 幂等：已经有了就跳过。返回当前"名字对得上"的替身个数。
    ///
    /// ⚠️ 这是**替代**：原版这些节点在 `.tscn` 里，本类只 `get_node` 不建。
    #[func]
    fn build_stub_children(&mut self) -> i64 {
        // `RoomNameLineEdit`：原版是个 `LineEdit`（`text_changed → on_text_change`）。
        if self.base().find_child_ex("RoomNameLineEdit").owned(false).done().is_none() {
            let mut edit = LineEdit::new_alloc();
            edit.set_name("RoomNameLineEdit");
            let node = edit.clone().upcast::<Node>();
            self.base_mut().add_child(&node);
        }
        // `ChatPopUp`：一个容器 `Control`（`visibility_changed → on_visibility_change`），
        // 底下挂 `ChatLineEdit`（原版树是 `…/ChatContainer2/ChatLineEdit`）。
        if self.base().find_child_ex("ChatPopUp").owned(false).done().is_none() {
            let mut popup = Control::new_alloc();
            popup.set_name("ChatPopUp");
            popup.set_visible(false);
            let node = popup.clone().upcast::<Node>();
            self.base_mut().add_child(&node);
        }
        if self.base().find_child_ex("ChatLineEdit").owned(false).done().is_none() {
            let mut edit = LineEdit::new_alloc();
            edit.set_name("ChatLineEdit");
            let node = edit.clone().upcast::<Node>();
            if let Some(mut popup) = self.base().find_child_ex("ChatPopUp").owned(false).done() {
                popup.add_child(&node);
            } else {
                self.base_mut().add_child(&node);
            }
        }
        let mut found = 0;
        for name in STUB_NODE_NAMES {
            if self.base().find_child_ex(name).owned(false).done().is_some() {
                found += 1;
            }
        }
        found
    }

    /// `_ready` 里真连上了几条（期望 3；缺节点时更小）。
    #[func]
    fn wired_count(&self) -> i64 {
        self.wired
    }

    /// 没找到 / 没连上的节点名（`|` 分隔）。
    #[func]
    fn missing_nodes(&self) -> GString {
        GString::from(join_text(&self.missing.borrow()).as_str())
    }

    /// `RoomNameLineEdit.text_changed` 的落点（原版方法名 `on_text_change`）。
    #[func]
    fn on_text_change(&mut self, new_text: GString) {
        self.text_change_calls += 1;
        self.room_name = new_text;
        self.force_title_update();
    }

    /// `ChatPopUp.visibility_changed` 的落点（原版方法名 `on_visibility_change`）。
    #[func]
    fn on_visibility_change(&mut self) {
        self.visibility_change_calls += 1;
    }

    /// `ChatLineEdit.text_submitted` 的落点（原版方法名 `on_chat_submitted`）。
    ///
    /// 原版还会把消息交给 `chat_message_event`（`0x4c…` 的后端事件）；那条链没接，
    /// 所以这里只发自造信号 + 记本地日志。
    #[func]
    fn on_chat_submitted(&mut self, text: GString) {
        self.chat_submitted_calls += 1;
        if self.chat_log.len() >= 8 {
            self.chat_log.remove(0);
        }
        self.chat_log.push(text.to_string());

        self.emit_chat_message(&text);
    }

    /// 标题：四格的线上名字拼起来（原版 `force_title_update` **写哪个节点未知**）。
    #[func]
    fn force_title_update(&mut self) {
        self.title_update_calls += 1;
        let names = self.options.names();
        self.title = GString::from(names.join(" ").as_str());
    }

    /// 标题字符串（`force_title_update` 的产物）。
    #[func]
    fn title_text(&self) -> GString {
        self.title.clone()
    }

    /// 房间名（`on_text_change` 收到的最新文本）。
    #[func]
    fn room_name(&self) -> GString {
        self.room_name.clone()
    }

    /// 聊天记录（`|` 分隔）。
    #[func]
    fn chat_log(&self) -> GString {
        GString::from(join_text(&self.chat_log).as_str())
    }

    /// 轴模式循环到下一档（**推断**的点击循环；原版没给这个按钮连 `pressed`）。
    #[func]
    fn cycle_axis_mode(&mut self) -> i64 {
        self.options.axis = self.options.axis.next();
        self.after_options_changed()
    }

    /// 函数模式切换另一档。
    #[func]
    fn cycle_function_mode(&mut self) -> i64 {
        self.options.function = self.options.function.next();
        self.after_options_changed()
    }

    /// 回合模式切换另一档。
    #[func]
    fn cycle_turns_mode(&mut self) -> i64 {
        self.options.turns = self.options.turns.next();
        self.after_options_changed()
    }

    /// 回合时限循环到下一档（六档循环；原版观测到按 1 次 → `01:59`、按 2 次 → `02:59`）。
    #[func]
    fn cycle_time_mode(&mut self) -> i64 {
        self.options.time = self.options.time.next();
        self.after_options_changed()
    }

    /// 轴模式判别值。
    #[func]
    fn axis_mode(&self) -> i64 {
        self.options.axis.as_u8() as i64
    }

    /// 函数模式判别值。
    #[func]
    fn function_mode(&self) -> i64 {
        self.options.function.as_u8() as i64
    }

    /// 回合模式判别值。
    #[func]
    fn turns_mode(&self) -> i64 {
        self.options.turns.as_u8() as i64
    }

    /// 回合时限判别值。
    #[func]
    fn time_mode(&self) -> i64 {
        self.options.time.as_u8() as i64
    }

    /// 四格的线上名字（`|` 分隔，顺序 axis / function / turn / time）。
    #[func]
    fn options_names(&self) -> GString {
        GString::from(self.options.names().join("|").as_str())
    }

    /// 四格的图标路径（`|` 分隔，同样四格顺序）。
    #[func]
    fn options_icons(&self) -> GString {
        GString::from(self.options.icons().join("|").as_str())
    }

    /// 回合时限的计时器标签（`02:59` 等）；无限档返回 `"inf"`（原版显示什么**未知**）。
    #[func]
    fn timer_label(&self) -> GString {
        match self.options.timer_label() {
            Some(label) => GString::from(label.as_str()),
            None => GString::from("inf"),
        }
    }

    /// 这一屏显示不显示倒计时。
    #[func]
    fn has_turn_clock(&self) -> bool {
        self.options.has_turn_clock()
    }

    /// 加一个人类玩家，返回新的条目数。
    #[func]
    fn add_human(&mut self, name: GString, team: i64, soldiers: i64) -> i64 {
        let team = team_of(team);
        let soldiers = soldiers.clamp(0, setup::MAX_SOLDIERS_PER_PLAYER as i64) as u8;
        self.entries.push(PlayerEntry::human(name.to_string(), team, soldiers));
        self.entries.len() as i64
    }

    /// 加一个 Bot（难度档 `0..=5`），返回新的条目数。
    #[func]
    fn add_bot(&mut self, level: i64, name: GString, team: i64, soldiers: i64) -> i64 {
        let team = team_of(team);
        let soldiers = soldiers.clamp(0, setup::MAX_SOLDIERS_PER_PLAYER as i64) as u8;
        let level = level.clamp(0, 5) as u8;
        self.entries
            .push(PlayerEntry::bot(level, name.to_string(), team, soldiers));
        self.entries.len() as i64
    }

    /// `AddSoldierButton`：给第 `index` 行加一个士兵（饱和在 5）。
    #[func]
    fn add_soldier(&mut self, index: i64) -> bool {
        match self.entries.get_mut(index.max(0) as usize) {
            Some(entry) => entry.add_soldier(),
            None => false,
        }
    }

    /// `RemoveSoldierButton`：给第 `index` 行减一个士兵（饱和在 0）。
    #[func]
    fn remove_soldier(&mut self, index: i64) -> bool {
        match self.entries.get_mut(index.max(0) as usize) {
            Some(entry) => entry.remove_soldier(),
            None => false,
        }
    }

    /// `ChangeTeamButton`：第 `index` 行换队。
    #[func]
    fn change_team(&mut self, index: i64) -> bool {
        match self.entries.get_mut(index.max(0) as usize) {
            Some(entry) => {
                entry.change_team();
                true
            }
            None => false,
        }
    }

    /// 现在有几行玩家。
    #[func]
    fn entry_count(&self) -> i64 {
        self.entries.len() as i64
    }

    /// 每行 `name:team:soldiers[:botN]`，`|` 分隔。
    #[func]
    fn entries_text(&self) -> GString {
        let rows: Vec<String> = self
            .entries
            .iter()
            .map(|entry| {
                let team = match entry.team {
                    Team::One => "1",
                    Team::Two => "2",
                };
                match entry.bot_level {
                    Some(level) => format!("{}:{}:{}:bot{}", entry.name, team, entry.soldiers, level),
                    None => format!("{}:{}:{}", entry.name, team, entry.soldiers),
                }
            })
            .collect();
        GString::from(rows.join("|").as_str())
    }

    /// ` Start `：有对手 → 发 `start_requested` 并返回 `true`；没对手 → **纯静默**，返回 `false`。
    ///
    /// 判据是 [`setup::start_action`]（`can_start` = 两队都有人），原版观测：
    /// 不加 Bot 直接 Start 是**静默无操作**（`docs/16` §2.1.5）。
    #[func]
    fn start(&mut self) -> bool {
        match setup::start_action(&self.entries) {
            SetupAction::StartMatch => {
                self.emit_start_requested();
                true
            }
            SetupAction::SilentNoOp => false,
            // `start_action` 只会返回上面两种；`Back` 分支留着是为了 match 穷尽。
            SetupAction::Back(_) => false,
        }
    }

    /// 返回键：发 `exit_requested`（去向 [`setup::exit_action`] = `LanLobby`，**推断**）。
    #[func]
    fn exit(&mut self) -> bool {
        self.emit_exit_requested();
        true
    }

    /// `_ready` 被调到几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `on_text_change` 被调到几次。
    #[func]
    fn text_change_count(&self) -> i64 {
        self.text_change_calls
    }

    /// `on_visibility_change` 被调到几次。
    #[func]
    fn visibility_change_count(&self) -> i64 {
        self.visibility_change_calls
    }

    /// `on_chat_submitted` 被调到几次。
    #[func]
    fn chat_submitted_count(&self) -> i64 {
        self.chat_submitted_calls
    }

    /// `force_title_update` 被调到几次。
    #[func]
    fn title_update_count(&self) -> i64 {
        self.title_update_calls
    }

    /// `options_changed` 发了几次。
    #[func]
    fn options_changed_count(&self) -> i64 {
        self.options_changed_calls
    }

    // -----------------------------------------------------------------------
    // 内部
    // -----------------------------------------------------------------------

    /// 四格变了之后：刷标题 + 发 `options_changed`，返回当前轴模式判别值。
    fn after_options_changed(&mut self) -> i64 {
        self.force_title_update();
        self.options_changed_calls += 1;
        let axis = self.options.axis.as_u8() as i64;
        let function = self.options.function.as_u8() as i64;
        let turns = self.options.turns.as_u8() as i64;
        let time = self.options.time.as_u8() as i64;
        // 机制 1：类型安全生成器（四个 i64 参数）。
        self.signals()
            .options_changed()
            .emit(axis, function, turns, time);
        axis
    }

    /// **机制 1**：无参信号。
    fn emit_start_requested(&mut self) {
        self.signals().start_requested().emit();
    }

    /// **机制 1**：无参信号。
    fn emit_exit_requested(&mut self) {
        self.signals().exit_requested().emit();
    }

    /// **机制 1**：带 `String` 参数的信号。
    fn emit_chat_message(&mut self, text: &GString) {
        let payload = text.clone();
        self.signals().chat_message().emit(&payload);
    }
}

/// `0`/其它 ⇒ 一队；`2` ⇒ 二队。
fn team_of(raw: i64) -> Team {
    if raw == 2 {
        Team::Two
    } else {
        Team::One
    }
}

/// 把一串字符串接成 `a|b|c`。
///
/// **返回 `String` 而不是 `GString`**：`GString::from` 要调引擎的构造函数，没有 Godot 进程
/// 就 panic，所以纯 Rust 的那一半留在 `String` 上，到 `#[func]` 边界才转。
fn join_text<S: AsRef<str>>(items: &[S]) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push('|');
        }
        out.push_str(item.as_ref());
    }
    out
}

/// 让 `AxisMode` / `FunctionMode` / `TurnMode` / `TimeMode` 的 `next()` 在编译期被用到
/// （它们只经 `#[func]` 调用，单测里也直接跑一遍）。
#[allow(dead_code)]
fn cycle_probe() -> (AxisMode, FunctionMode, TurnMode, TimeMode) {
    (
        AxisMode::EveryUnit.next(),
        FunctionMode::NormalFunction.next(),
        TurnMode::SequentialTurns.next(),
        TimeMode::Timer1m.next(),
    )
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 四个模式的循环顺序 = 判别值序（`docs/16` §16：图标 id 序 = 名字表序）。
    #[test]
    fn mode_cycles_follow_the_discriminant_order() {
        let (axis, function, turns, time) = cycle_probe();
        assert_eq!(axis, AxisMode::NoAxis);
        assert_eq!(function, FunctionMode::DiffEqFunction);
        assert_eq!(turns, TurnMode::SimultaneousTurns);
        assert_eq!(time, TimeMode::Timer2m, "Timer1m +1 = Timer2m（= 按 1 次 02:59）");
    }

    /// 新建房间的四格初值（`ROOM_DEFAULT_OPTIONS`，**观测**）。
    #[test]
    fn room_defaults_are_the_observed_new_room_game_info() {
        let options = ROOM_DEFAULT_OPTIONS;
        assert_eq!(options.axis, AxisMode::EveryUnit);
        assert_eq!(options.function, FunctionMode::NormalFunction);
        assert_eq!(options.turns, TurnMode::SequentialTurns);
        assert_eq!(options.time, TimeMode::Timer1m);
        assert_eq!(
            options.names(),
            [
                "EveryUnit",
                "NormalFunction",
                "SequentialTurns",
                "Timer1m"
            ]
        );
        assert_eq!(options.timer_label().as_deref(), Some("00:59"));
        assert!(options.has_turn_clock());
    }

    /// 按 1 次 / 2 次 `TurnTimeButton` 的标签 —— `docs/16` §2.1.10 的真帧（`01:59` / `02:59`）。
    #[test]
    fn clicking_time_mode_once_and_twice_matches_the_observed_frames() {
        let once = ROOM_DEFAULT_OPTIONS.time.next();
        let twice = once.next();
        assert_eq!(once.timer_label().as_deref(), Some("01:59"));
        assert_eq!(twice.timer_label().as_deref(), Some("02:59"));
        // 六档循环回到起点，且无限档没有标签。
        assert_eq!(TimeMode::TimerInf.next(), TimeMode::Timer30s);
        assert_eq!(TimeMode::TimerInf.timer_label(), None);
    }

    /// Start 的判据：**两队都有人**才真开始；只有一个队是静默无操作（**观测**）。
    #[test]
    fn start_is_silent_until_both_teams_have_someone() {
        assert!(!setup::can_start(&[]));
        let one_team = [PlayerEntry::human("A", Team::One, 1)];
        assert!(!setup::can_start(&one_team));
        assert_eq!(setup::start_action(&one_team), SetupAction::SilentNoOp);

        let both = [
            PlayerEntry::human("A", Team::One, 2),
            PlayerEntry::bot(3, "Deep Blue", Team::Two, 2),
        ];
        assert!(setup::can_start(&both));
        assert_eq!(setup::start_action(&both), SetupAction::StartMatch);
    }

    /// 士兵数饱和在 5 / 0（5 个槽是观测；"到 5 不再涨"是推断）。
    #[test]
    fn soldier_count_saturates_at_five_and_zero() {
        let mut entry = PlayerEntry::human("A", Team::One, 5);
        assert!(!entry.add_soldier());
        assert_eq!(entry.soldiers, 5);
        for _ in 0..5 {
            assert!(entry.remove_soldier());
        }
        assert!(!entry.remove_soldier());
        assert_eq!(entry.soldiers, 0);
        assert_eq!(entry.soldier_slots().len(), 5);
    }

    /// 11 个 `get_node` 名字里只建了 3 个替身 —— 这个"子集"关系要钉住。
    #[test]
    fn stub_nodes_are_a_subset_of_the_original_get_node_names() {
        for name in STUB_NODE_NAMES {
            assert!(
                ALL_GET_NODE_NAMES.contains(&name),
                "{name} 不在原版 11 个 get_node 名字里"
            );
        }
        assert_eq!(ALL_GET_NODE_NAMES.len(), 11);
        assert_eq!(STUB_NODE_NAMES.len(), 3);
    }

    /// `team_of` 的映射（`0`/其它 → 一队，`2` → 二队）。
    #[test]
    fn team_of_maps_two_to_team_two() {
        assert_eq!(team_of(0), Team::One);
        assert_eq!(team_of(1), Team::One);
        assert_eq!(team_of(2), Team::Two);
    }
}
}

#[cfg(feature = "gdext-bindings")]
pub use node_class::*;

#[cfg(test)]
mod f3_landing_tests {
    use super::*;

    /// 一个记录型的假引擎：把每次调用按顺序记下来，值都透传。
    #[derive(Default)]
    struct FakeEngine {
        trace: Vec<String>,
        fail_upcast: bool,
    }

    impl SetupEngine for FakeEngine {
        fn gd_clone(&mut self, src: GdHandle) -> Option<GdHandle> {
            self.trace.push(format!("clone({:#x})", src.obj));
            Some(src)
        }
        fn upcast_ref(&mut self, src: GdHandle) -> Option<GdHandle> {
            self.trace.push(format!("upcast_ref({:#x})", src.obj));
            if self.fail_upcast { None } else { Some(src) }
        }
        fn get_node_by_name(&mut self, _base: GdHandle, name: &str) -> Option<GdHandle> {
            self.trace.push(format!("get_node({name})"));
            Some(GdHandle::new(0x1000, 0x2000))
        }
        fn to_godot(&mut self, src: GdHandle) -> Option<GdHandle> {
            self.trace.push("to_godot".into());
            Some(src)
        }
        fn set_texture(&mut self, icon: GdHandle, _tex: GdHandle) {
            self.trace.push(format!("set_texture({:#x})", icon.obj));
        }
        fn gd_from_obj(&mut self, raw: usize) -> Option<GdHandle> {
            self.trace.push(format!("from_obj({raw:#x})"));
            Some(GdHandle::new(raw, 0x2000))
        }
        fn connect_button(&mut self, button: GdHandle, _handler: GdHandle) {
            self.trace.push(format!("connect({:#x})", button.obj));
        }
        fn gd_touch(&mut self, _src: GdHandle) {
            self.trace.push("touch".into());
        }
    }

    /// 假模块容器：七个 get 都成功，表查找按构造给值。
    #[derive(Default)]
    struct FakeModules {
        local_ids: Vec<u32>,
        rows: Vec<i32>,
        name_rows: Vec<(i32, OpaqueValue)>,
        empty_name_table: bool,
    }

    impl SetupModules for FakeModules {
        fn name_manager(&mut self) -> Option<ModuleSlot> { Some(ModuleSlot("name")) }
        fn rank_manager(&mut self) -> Option<ModuleSlot> { Some(ModuleSlot("rank")) }
        fn player_manager(&mut self) -> Option<ModuleSlot> { Some(ModuleSlot("player")) }
        fn local_player_manager(&mut self) -> Option<ModuleSlot> { Some(ModuleSlot("local")) }
        fn soldier_manager(&mut self) -> Option<ModuleSlot> { Some(ModuleSlot("soldier")) }
        fn team_manager(&mut self) -> Option<ModuleSlot> { Some(ModuleSlot("team")) }
        fn graphics_manager(&mut self) -> Option<ModuleSlot> { Some(ModuleSlot("graphics")) }
        fn name_row(&mut self, _m: ModuleSlot, index: i32) -> Option<OpaqueValue> {
            if self.empty_name_table { return None; }
            self.name_rows.iter().find(|(k, _)| *k == index).map(|(_, v)| *v)
        }
        fn rank_row(&mut self, _m: ModuleSlot, _rank: u32) -> Option<OpaqueValue> { Some(OpaqueValue::default()) }
        fn player_rows(&mut self, _m: ModuleSlot) -> Vec<OpaqueValue> {
            self.rows.iter().map(|id| OpaqueValue { a: *id as usize, b: 0, c: 0 }).collect()
        }
        fn local_player_ids(&mut self, _m: ModuleSlot) -> Vec<u32> { self.local_ids.clone() }
        fn player_ident(&mut self, row: OpaqueValue) -> i32 { row.a as i32 }
        fn get_team(&mut self, _m: ModuleSlot, _t: u32) -> Option<ModuleSlot> { Some(ModuleSlot("team")) }
        fn soldier_slots(&mut self, _m: ModuleSlot, _t: ModuleSlot) -> Vec<bool> {
            vec![true, true, false, false, false]
        }
        fn apply_name_row(&mut self, _label: GdHandle, _row: OpaqueValue) {}
    }

    /// FDE 表逐条自洽：`fde_hi - fde_lo == size`，且 9 条地址互不相同。
    #[test]
    fn f3_anchor_table_is_self_consistent() {
        assert_eq!(F3_ANCHORS.len(), 9, "本批 9 条");
        for (i, (va, lo, hi, size, name, ev)) in F3_ANCHORS.iter().enumerate() {
            assert_eq!(va, lo, "{name}：vaddr 必须就是 FDE 起点");
            assert_eq!(hi - lo, *size as u64, "{name}：size 与 FDE 范围必须一致");
            for (va2, ..) in &F3_ANCHORS[i + 1..] {
                assert_ne!(va, va2, "地址不许重复");
            }
            assert!(
                !matches!(ev, Evidence::Inferred | Evidence::Unknown) || name.contains("候选") || name.contains("实例"),
                "{name}：非观测档必须在名字里标明"
            );
        }
    }

    /// `.rodata` 字面量的长度必须等于字符串自身长度（**逐位**自校验）。
    ///
    /// 真值：`lea` 目标 + 紧随的 `mov r<n>d,<leg>` 立即数（本轮已用
    /// `loc_xrefs.Elf.read(va,len)` 逐条读出来核对过）。
    #[test]
    fn node_name_literal_lengths_match_the_bytes() {
        for (name, _va, len) in NODE_NAME_LITERALS {
            assert_eq!(name.len(), *len, "{name}：立即数长度必须等于串长");
        }
        for (op, _va, len) in GD_OP_LITERALS {
            assert_eq!(op.len(), *len, "{op}：立即数长度必须等于串长");
        }
        // 0x2f = 47（0x4c20b0 mov esi,0x2f）
        assert_eq!(MSG_UPCAST_FAILED.len(), 47);
    }

    /// `is_leader_local` 返回 bool：命中本地 id ⇒ true，扫完 ⇒ false。
    ///
    /// 真值来源：`0x4c2db9 mov bl,1` / `0x4c2e03 xor ebx,ebx` / `0x4c2e15 mov eax,ebx`。
    #[test]
    fn is_leader_local_returns_true_iff_a_local_id_is_in_the_player_table() {
        let mut hit = FakeModules { local_ids: vec![7], rows: vec![3, 7], ..Default::default() };
        assert!(SetupControlIntent::is_leader_local(&mut hit));
        let mut miss = FakeModules { local_ids: vec![9], rows: vec![3, 7], ..Default::default() };
        assert!(!SetupControlIntent::is_leader_local(&mut miss));
        let mut none = FakeModules { local_ids: vec![], rows: vec![3], ..Default::default() };
        assert!(!SetupControlIntent::is_leader_local(&mut none));
    }

    /// `update_name` 的节点名必须是 `PlayerNameLabel`（`0x4c1ef3` + 15 字节立即数）。
    #[test]
    fn update_name_asks_for_the_player_name_label() {
        let mut e = FakeEngine::default();
        let mut m = FakeModules { name_rows: vec![(2, OpaqueValue::default())], ..Default::default() };
        SetupControlIntent::update_name(&mut e, &mut m, 2, GdHandle::new(0x10, 0x20));
        assert!(e.trace.iter().any(|s| s == "get_node(PlayerNameLabel)"), "{:?}", e.trace);
        // 0x4c1ebe "clone"(5) 先于 0x4c1ef3 get_node
        let clone_at = e.trace.iter().position(|s| s.starts_with("clone")).unwrap();
        let node_at = e.trace.iter().position(|s| s == "get_node(PlayerNameLabel)").unwrap();
        assert!(clone_at < node_at);
    }

    /// 空表（`0x4c1f37 cmp qword [r14+0x48],0`）⇒ 不往下走、也不 panic。
    #[test]
    fn update_name_with_empty_name_table_does_nothing_extra() {
        let mut e = FakeEngine::default();
        let mut m = FakeModules { empty_name_table: true, ..Default::default() };
        SetupControlIntent::update_name(&mut e, &mut m, 0, GdHandle::new(0x10, 0x20));
        assert!(!e.trace.iter().any(|s| s == "touch"), "{:?}", e.trace);
    }

    /// `update_rank` 的 `set_texture` 必须**恰好 3 次**（`0x4c235f`/`0x4c2509`/`0x4c2639`）。
    #[test]
    fn update_rank_sets_three_texture_layers() {
        let mut e = FakeEngine::default();
        let mut m = FakeModules::default();
        SetupControlIntent::update_rank(&mut e, &mut m, 5, GdHandle::new(0x10, 0x20));
        assert_eq!(e.trace.iter().filter(|s| s.starts_with("set_texture")).count(), 3);
        assert_eq!(RANK_ICON_LAYERS, 3);
        assert!(e.trace.iter().any(|s| s == "get_node(RankIcon)"));
    }

    /// `update_soldiers` 取的容器与 4 个图标名（顺序 = `.rodata` 顺序）。
    #[test]
    fn update_soldiers_walks_the_four_icon_names_in_order() {
        let mut e = FakeEngine::default();
        let mut m = FakeModules::default();
        SetupControlIntent::update_soldiers(&mut e, &mut m, 1, GdHandle::new(0x10, 0x20));
        let names: Vec<&str> = e.trace.iter().filter_map(|s| s.strip_prefix("get_node(").and_then(|r| r.strip_suffix(')'))).collect();
        assert_eq!(names.first().copied(), Some("SoldierIconContainer"));
        let icons: Vec<&str> = names.iter().skip(1).copied().collect();
        assert_eq!(icons, vec!["SkinIcon", "FaceIcon", "HatIcon", "EmptyIcon"]);
        assert_eq!(SOLDIER_ICON_ORDER.to_vec(), icons);
    }

    /// `connect_entry_buttons`：先根节点、再 4 个按钮，顺序 = `.rodata` 字面量顺序。
    #[test]
    fn connect_entry_buttons_walks_root_then_four_buttons() {
        let mut e = FakeEngine::default();
        SetupControlIntent::connect_entry_buttons(&mut e, 0xDEAD, 0, 0, 0);
        let nodes: Vec<&str> = e.trace.iter().filter_map(|s| s.strip_prefix("get_node(").and_then(|r| r.strip_suffix(')'))).collect();
        assert_eq!(nodes, vec!["GraphwarRootNode", "ChangeTeamButton", "RemovePlayerButton", "AddSoldierButton", "RemoveSoldierButton"]);
        assert_eq!(ENTRY_BUTTONS.len(), 4);
    }

    /// `upcast_ref` 失败 ⇒ 原版走 `MSG_UPCAST_FAILED` 的 panic 臂（`0x4c208a`）。
    #[test]
    #[should_panic(expected = "Upcast failed")]
    fn upcast_failure_panics_with_the_binary_message() {
        let mut e = FakeEngine { fail_upcast: true, ..Default::default() };
        let mut m = FakeModules::default();
        SetupControlIntent::update_name(&mut e, &mut m, 0, GdHandle::new(0x10, 0x20));
    }

    /// 三条宏产物 / 未定名的常量：`.text` 里的内嵌函数指针与 GOT 槽被调者。
    #[test]
    fn macro_generated_and_unknown_entries_keep_their_instruction_evidence() {
        assert_eq!(INNER_INIT_V1_PTRS, (0x40_d3a0, 0x41_5560));
        assert_eq!(INNER_INIT_V2_PTRS, (0x40_dac0, 0x40_e6c0));
        assert_eq!(REGISTER_CONSTANTS_PTRS, (0x4d_98d0, 0x4d_9820));
        assert_eq!(FN_REQUEST_LOCK_CHANGE, 0x51_7490);
        assert_eq!(READY_CANDIDATE_INNER_FIELD, 0x1f8);
        assert_eq!(INNER_INIT_SEQUENCE.len(), 4);
    }
}
