//! `graphwar_root_node.rs` —— 主场景节点（原版 **23 个可归属函数**，`fixtures/module_funcs.tsv`）。
//!
//! # 这个模块是什么
//!
//! 原版的 `GraphwarRootNode` 是主场景节点：它持有**屏表**
//! （`HashMap<&'static str, Gd<Node>>`）、两个控制器句柄、若干闸门位，并把
//! 「按钮命令 → 切屏 / 显隐 / 广告」串起来。界面控制器写在 Rust 里
//! （`AGENTS.md` §0 第 4 条），所以这些行为都是源码，不是 `.gd` 脚本。
//!
//! # 地址约定
//!
//! 本文件一律用 **ELF vaddr**（`objdump` 打印的地址）。Ghidra / REA 地址 = 本文件地址
//! `+ 0x100000`（**只加一次**）。
//!
//! # 证据档位（`AGENTS.md` §0 第 3 条的四个档）
//!
//! * **逐位** —— 与真二进制**逐字节**相同。本文件够这一档的是 `.rodata` 字面量本身
//!   （每个屏名 / 命令键的字节都读回来比对过，地址由 [`Screen::addr`] /
//!   [`Command::evidence`] 给出）。
//! * **观测** —— 能指到具体指令地址（长度立即数、`mov $imm`、`call`/`jmp` 落点）。
//! * **推断** —— 有依据但没证，**每条都写明缺什么证据**。
//! * **未知** —— 没解出来；文件末尾「未知清单」逐条登记。
//!
//! ⚠️ `cargo test` 全绿只是**回归保护**，不构成复原证据（§0 第 2 条）。
//!
//! # 函数清单与已解出的角色
//!
//! | vaddr | 本文件记号 | 已解出的角色 | 档位 |
//! |---|---|---|---|
//! | `0x3a22f0` | [`FN_CONSTRUCT`] | 装配/初始化：`[+0x1702]` 等字段的初值在这里写 | 观测（写入点） |
//! | `0x3a2e10` | [`FN_READY`] | `_ready()`：取 3 个 holder 节点 | 内容=观测／命名=推断 |
//! | `0x3a5760` | [`FN_STATE_MACHINE`] | 逐帧/事件状态机（Ghidra 把大量内联合成一个函数） | 部分观测 |
//! | `0x3af700` | [`FN_SCREEN_GET`] | 按名字在屏表里查节点（hashbrown 查找内联体） | 观测 |
//! | `0x3af850` | [`FN_SHOW_SCREEN`] | **按名字切屏**：屏表逐项 `set_visible(name == 目标)` + 广告 create/destroy | 观测 |
//! | `0x3afcf0` | [`FN_BUSY_CHECK`] | 进屏前的「当前是否对局中」判定 | 推断 |
//! | `0x3b0830` | [`FN_COMMAND_BUS`] | 命令总线：小写命令键 → 动作；写 `[+0x1704]` | 观测 |
//!
//! 逐函数规格在 `docs/16-client.md` §15（屏表 §15.2、字段表 §15.3、`_ready` §15.4、
//! 广告闸门 §15.5、命令总线 §15.6、状态机 §15.7、地址更正 §15.8、
//! **实现侧补充与四条更正 §15.10**）。
//!
//! # 本文件**不**做的事
//!
//! 不调用 Godot。`get_node` / `call_deferred` / `set_visible` / 信号连接只以
//! **意图结构**（[`ScreenHost`] trait + 常量表）出现 —— 本 crate 的内核与客户端是解耦的，
//! 在 Rust 里直接调 Godot 会把两层焊死。真正落到 `Gd<Node>` 的那一步在 GDExtension 外壳里。

// ---------------------------------------------------------------------------
// 1. 地址锚点
// ---------------------------------------------------------------------------

/// 原版 `graphwar_root_node.rs` 的 23 个可归属函数（`fixtures/module_funcs.tsv`，
/// 判据 = `panic::Location` 里带本模块路径的函数）。
///
/// 档位：**观测**（`docs/16-client.md` §15.1 的清单；本轮逐条核对了入口字节）。
///
/// ⚠️ 构造器 [`FN_CONSTRUCT`] **不在**这 23 条里（它没有 panic 锚），单列。
pub const MODULE_FUNCS: [u64; 23] = [
    0x3a_2e10,
    0x3a_54e0,
    0x3a_55e0,
    0x3a_5620,
    0x3a_5660,
    0x3a_5760,
    0x3a_a6a0,
    0x3a_a820,
    0x3a_fb00,
    0x3a_fcf0,
    0x3a_fe90,
    0x3b_00d0,
    0x3b_0830,
    0x3b_4ec0,
    0x3b_56c0,
    0x3b_5910,
    0x3b_5ac0,
    0x3b_5ce0,
    0x3b_6e00,
    0x3b_6ed0,
    0x3b_7020,
    0x42_6430,
    0x43_3c00,
];

/// 构造/装配函数入口（`docs/16-client.md` §15.1：`0xb20` 字节，含 EH 冷路径）。
///
/// 档位：**观测**（入口字节 + `ret` @`0x3a2bab` + 冷路径 `ud2` @`0x3a2c08`，
/// 冷路径一直铺到下一个函数入口 `0x3a2e10`）。
///
/// ⚠️ **推断**：它是「构造」还是 `Clone::clone` 一类，**未证**。热路径形如
/// `fn(self, src)`：`mov %rsi,0x78(%rsp)` @`0x3a2301`，随后把 `src` 的 0x18 字节
/// 拷进 `self+0x1620`（`0x3a2a45`–`0x3a2a54`），最后 `mov %r15,%rax` @`0x3a2b97`
/// 返回 `self`。缺的证据：该函数的归属锚（它没有 `panic::Location`）与调用点
/// （是 gdext 的 `new()` 还是 `clone`）。
pub const FN_CONSTRUCT: u64 = 0x3a_22f0;

/// `_ready()`。档位：**观测**（函数体）；函数名是**推断**（本轮没有逐槽对 GDExtension
/// 虚表；`_ready` 与 `_process` 都在本模块，靠「取 3 个 holder + 连信号」判为 `_ready`）。
pub const FN_READY: u64 = 0x3a_2e10;

/// 逐帧 / 事件状态机总入口。档位：**部分观测**（`20288` 字节、`339` 条比较、
/// `64` 个直接调用目标；Ghidra 当成**一个**函数，很可能就是 `_process` 级入口 + 大量内联，
/// 不是「一个业务函数」）。
pub const FN_STATE_MACHINE: u64 = 0x3a_5760;

/// 按名字查屏表里的节点。档位：**观测**。
///
/// 判据：`cmpq $0x0,0x18(%rdi)`（表空闸门）@`0x3af70e` → `call 0x47daa0`（求哈希）
/// @`0x3af729` → `and` 掩码 @`0x3af75b` → `pcmpeqb` 探测 @`0x3af768`。
/// 屏表指针出现在 `0x3b2d61`（`add $0x1638,%rbx`）+ 调用 `0x3b2d77`
/// `("AddBotPopUp", 11)`，以及尾调用 `0x3b377c`/`0x3b37a3`。
pub const FN_SCREEN_GET: u64 = 0x3a_f700;

/// **切屏**：按名字决定屏表里每个节点的可见性，并对 `PlayScreen` 额外
/// `call_deferred("create_ad_view" | "destroy_ad_view")`。
///
/// 档位：**观测**（控制流、字面量、长度立即数逐条见 [`ScreenSwap`]）。
///
/// ⚠️ 这里**不**沿用 `docs/16` §15.5 的旧名「广告闸门」：旧名只覆盖了它一半功能。
/// 逐字节读下来，主体是「屏表逐项 `set_visible(name == 目标)`」，广告只是 `PlayScreen`
/// 那一项的特例 —— 也就是说，它才是 `docs/19` §③ 记成 `show_screen` 的那个动作
/// （旧记的两个地址 `0x3a5ea8` / `0x3a6038` **都不是函数入口**，见 §15.8）。
pub const FN_SHOW_SCREEN: u64 = 0x3a_f850;

/// 进屏前的「当前是否对局中」判定。档位：**推断**（`docs/16` §15.6 记它的调用者是
/// `0x3b385f`、§15.5 又把它列为 [`FN_SHOW_SCREEN`] 的调用者之一；本轮没有逐字节解它的函数体）。
pub const FN_BUSY_CHECK: u64 = 0x3a_fcf0;

/// 命令总线：`fn(self, name: &str)`，`rdx` 是**长度**。档位：**观测**（键表见 [`Command`]；
/// 唯一入口 `0x3a6d7d`，在状态机 [`FN_STATE_MACHINE`] 的一个循环里）。
pub const FN_COMMAND_BUS: u64 = 0x3b_0830;

// ---------------------------------------------------------------------------
// 2. 屏名表：15 个值（`docs/16-client.md` §15.2）
// ---------------------------------------------------------------------------

/// 15 个界面屏名。**顺序 = `docs/16` §15.2 的表序**（不是 `.rodata` 里的物理顺序，
/// 也不是任何执行顺序 —— 屏表是 `HashMap`，迭代序未解）。
///
/// 每个变体的字面量是 `.rodata` 里**不以 NUL 结尾**的 `&'static str`
/// （相邻字面量直接粘在一起，`strings` 会连成一长条）。
/// 档位：**逐位**（名字与 [`Screen::addr`] 处的字节逐字节相同；长度 = 字节数）。
///
/// **观测**：15 个里有 13 个的「代码里用的长度」能在使用点的 `mov $imm,%edx`/`%r8d`
/// 里量到，见 [`Screen::len_immediate_site`]；`ConfirmPlay` / `ExitPopUp` 两个量不到。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Screen {
    /// `.rodata` `0x19fc70`，8 字节。
    ///
    /// **推断**：启动默认屏。依据 = 构造期把 `[+0x1704]` 写成 1（[`constructor_init_fields`]）
    /// 且游戏启动进主菜单。缺的证据：5 个写点与 5 个 `get_node` 目标一一对上。
    MainMenu,
    /// `0x19fcc0`，8 字节。
    Campaign,
    /// `0x19f380`，8 字节。
    LanLobby,
    /// `0x1bda32`，9 字节。
    GameSetup,
    /// `0x1bd92b`，10 字节。**被单列处理**（见 [`ScreenSwap`]）：切到它要拆广告，
    /// 离开它要建广告。
    PlayScreen,
    /// `0x1bdaec`，11 字节。
    ReplayLobby,
    /// `0x1bdb2f`，12 字节。
    PlayerScreen,
    /// `0x1bdaf7`，11 字节。
    JoinIpPopUp,
    /// `0x1bdb02`，11 字节。
    AddBotPopUp,
    /// `0x1bdb0d`，11 字节。
    ConfirmPlay,
    /// `0x1bdb18`，9 字节。
    ExitPopUp,
    /// `0x1bdb21`，14 字节。
    JoinLevelPopUp,
    /// `0x1bda23`，15 字节。
    ConnectingPopUp,
    /// `0x1bda3b`，**12** 字节（`docs/16` §15.2 的旧表写 11，见 §15.10 更正 ①）。
    EndGamePopUp,
    /// `0x1bda73`，17 字节。
    DisconnectedPopUp,
}

/// 全部 15 个屏，顺序同 [`Screen`] 的声明序（= `docs/16` §15.2 表序）。
pub const ALL_SCREENS: [Screen; 15] = [
    Screen::MainMenu,
    Screen::Campaign,
    Screen::LanLobby,
    Screen::GameSetup,
    Screen::PlayScreen,
    Screen::ReplayLobby,
    Screen::PlayerScreen,
    Screen::JoinIpPopUp,
    Screen::AddBotPopUp,
    Screen::ConfirmPlay,
    Screen::ExitPopUp,
    Screen::JoinLevelPopUp,
    Screen::ConnectingPopUp,
    Screen::EndGamePopUp,
    Screen::DisconnectedPopUp,
];

/// 屏名的对照表：`(名字, .rodata 地址, 字节长度, 长度立即数所在指令)`。
///
/// 第 4 项 `None` 表示「本次没量到代码里用的长度」，**不**表示长度未知
/// （长度就是名字的字节数，已逐位钉住），只表示没有第二个独立来源。
const SCREEN_TABLE: [(&str, u64, u32, Option<u64>); 15] = [
    ("MainMenu", 0x19_fc70, 8, Some(0x3b_3f23)),
    ("Campaign", 0x19_fcc0, 8, Some(0x3a_8993)),
    ("LanLobby", 0x19_f380, 8, Some(0x3b_3e7b)),
    ("GameSetup", 0x1b_da32, 9, Some(0x3a_7cc6)),
    ("PlayScreen", 0x1b_d92b, 10, Some(0x3a_57d6)),
    ("ReplayLobby", 0x1b_daec, 11, Some(0x3b_3ef0)),
    ("PlayerScreen", 0x1b_db2f, 12, Some(0x3b_11fd)),
    ("JoinIpPopUp", 0x1b_daf7, 11, Some(0x3b_0a94)),
    ("AddBotPopUp", 0x1b_db02, 11, Some(0x3b_2d6f)),
    ("ConfirmPlay", 0x1b_db0d, 11, None),
    ("ExitPopUp", 0x1b_db18, 9, None),
    ("JoinLevelPopUp", 0x1b_db21, 14, Some(0x3b_0df8)),
    ("ConnectingPopUp", 0x1b_da23, 15, Some(0x3a_6c58)),
    ("EndGamePopUp", 0x1b_da3b, 12, Some(0x3a_8122)),
    ("DisconnectedPopUp", 0x1b_da73, 17, Some(0x3a_7555)),
];

impl Screen {
    /// 屏名（`.rodata` 里的 `&'static str` 原文）。档位：**逐位**。
    pub const fn name(self) -> &'static str {
        match self {
            Screen::MainMenu => "MainMenu",
            Screen::Campaign => "Campaign",
            Screen::LanLobby => "LanLobby",
            Screen::GameSetup => "GameSetup",
            Screen::PlayScreen => "PlayScreen",
            Screen::ReplayLobby => "ReplayLobby",
            Screen::PlayerScreen => "PlayerScreen",
            Screen::JoinIpPopUp => "JoinIpPopUp",
            Screen::AddBotPopUp => "AddBotPopUp",
            Screen::ConfirmPlay => "ConfirmPlay",
            Screen::ExitPopUp => "ExitPopUp",
            Screen::JoinLevelPopUp => "JoinLevelPopUp",
            Screen::ConnectingPopUp => "ConnectingPopUp",
            Screen::EndGamePopUp => "EndGamePopUp",
            Screen::DisconnectedPopUp => "DisconnectedPopUp",
        }
    }

    /// 名字在 `.rodata` 里的地址（`.rodata` 的 vaddr == 文件偏移，可直接读字节对拍）。
    /// 档位：**逐位**。
    pub const fn addr(self) -> u64 {
        match self {
            Screen::MainMenu => 0x19_fc70,
            Screen::Campaign => 0x19_fcc0,
            Screen::LanLobby => 0x19_f380,
            Screen::GameSetup => 0x1b_da32,
            Screen::PlayScreen => 0x1b_d92b,
            Screen::ReplayLobby => 0x1b_daec,
            Screen::PlayerScreen => 0x1b_db2f,
            Screen::JoinIpPopUp => 0x1b_daf7,
            Screen::AddBotPopUp => 0x1b_db02,
            Screen::ConfirmPlay => 0x1b_db0d,
            Screen::ExitPopUp => 0x1b_db18,
            Screen::JoinLevelPopUp => 0x1b_db21,
            Screen::ConnectingPopUp => 0x1b_da23,
            Screen::EndGamePopUp => 0x1b_da3b,
            Screen::DisconnectedPopUp => 0x1b_da73,
        }
    }

    /// 字节长度（**不是** C 字符串长度：`.rodata` 里没有 NUL 结尾）。档位：**逐位**。
    pub const fn len(self) -> u32 {
        match self {
            Screen::MainMenu => 8,
            Screen::Campaign => 8,
            Screen::LanLobby => 8,
            Screen::GameSetup => 9,
            Screen::PlayScreen => 10,
            Screen::ReplayLobby => 11,
            Screen::PlayerScreen => 12,
            Screen::JoinIpPopUp => 11,
            Screen::AddBotPopUp => 11,
            Screen::ConfirmPlay => 11,
            Screen::ExitPopUp => 9,
            Screen::JoinLevelPopUp => 14,
            Screen::ConnectingPopUp => 15,
            Screen::EndGamePopUp => 12,
            Screen::DisconnectedPopUp => 17,
        }
    }

    /// 代码里**用到的**长度立即数所在指令（`mov $len,%edx` 或 `%r8d`）。档位：**观测**。
    ///
    /// 两个 `None`：
    /// * `ConfirmPlay` —— 全库只有一处 `lea 0x1bdb0d`（@`0x3b3783`），紧跟的长度是
    ///   `0x14 = 20`，正好是 `"ConfirmPlayExitPopUp"` 的长度，见 §15.10 更正 ②；
    /// * `ExitPopUp` —— 全库**没有** `lea 0x1bdb18`；它的地址只能由 `0x1bdb0d + 11` 得到
    ///   （两个名字在 `.rodata` 里紧邻），所以拿不到独立的长度证据。
    pub const fn len_immediate_site(self) -> Option<u64> {
        match self {
            Screen::MainMenu => Some(0x3b_3f23),
            Screen::Campaign => Some(0x3a_8993),
            Screen::LanLobby => Some(0x3b_3e7b),
            Screen::GameSetup => Some(0x3a_7cc6),
            Screen::PlayScreen => Some(0x3a_57d6),
            Screen::ReplayLobby => Some(0x3b_3ef0),
            Screen::PlayerScreen => Some(0x3b_11fd),
            Screen::JoinIpPopUp => Some(0x3b_0a94),
            Screen::AddBotPopUp => Some(0x3b_2d6f),
            Screen::ConfirmPlay => None,
            Screen::ExitPopUp => None,
            Screen::JoinLevelPopUp => Some(0x3b_0df8),
            Screen::ConnectingPopUp => Some(0x3a_6c58),
            Screen::EndGamePopUp => Some(0x3a_8122),
            Screen::DisconnectedPopUp => Some(0x3a_7555),
        }
    }

    /// 名字 → 枚举。非法名字返回 [`UnknownScreenName`]，**不 panic**。
    ///
    /// 理由：界面输入来自按钮的 `input_text`（`docs/16` §1 第 2 条），
    /// 不在枚举里是**正常路径**，不是 bug。
    pub fn from_name(name: &str) -> Result<Screen, UnknownScreenName> {
        ALL_SCREENS
            .iter()
            .copied()
            .find(|s| s.name() == name)
            .ok_or(UnknownScreenName)
    }

    /// **推断**：是否属于「弹窗」那一组。
    ///
    /// 依据 = `docs/16` §15.2 的「7 个基础屏 + 8 个弹窗」计数 + 名字后缀 `PopUp`；
    /// `ConfirmPlay` 是按那个**计数**归进来的（它的名字没有 `PopUp`）。
    /// 缺的证据：屏表里没有任何「基础屏 / 弹窗」判别字段 —— 这个分组只是名字学的。
    pub const fn is_popup_by_name(self) -> bool {
        matches!(
            self,
            Screen::JoinIpPopUp
                | Screen::AddBotPopUp
                | Screen::ConfirmPlay
                | Screen::ExitPopUp
                | Screen::JoinLevelPopUp
                | Screen::ConnectingPopUp
                | Screen::EndGamePopUp
                | Screen::DisconnectedPopUp
        )
    }
}

impl core::str::FromStr for Screen {
    type Err = UnknownScreenName;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Screen::from_name(s)
    }
}

impl TryFrom<&str> for Screen {
    type Error = UnknownScreenName;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Screen::from_name(s)
    }
}

impl core::fmt::Display for Screen {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// 屏名不在 [`Screen`] 里的错误。零大小、`Copy`、不分配（名字由调用方持有）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownScreenName;

impl core::fmt::Display for UnknownScreenName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("not one of the 15 graphwar_root_node screen names")
    }
}

impl std::error::Error for UnknownScreenName {}

// ---------------------------------------------------------------------------
// 3. `_ready()` 里取的节点（`0x3a2e10`）
// ---------------------------------------------------------------------------

/// `_ready()` 里的一个 `get_node`。
///
/// 档位：**观测**（三个 `lea` + 三个长度立即数 + 三个 `call 0x3acb00` 都能逐条对上）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyNode {
    /// `get_node` 的实参（节点路径的第一段）。
    pub name: &'static str,
    /// 名字字面量在 `.rodata` 的地址。
    pub addr: u64,
    /// 名字的字节长度。
    pub len: u32,
    /// `lea` 该字面量的指令地址。
    pub lea_site: u64,
    /// `mov $len,%r8d` 的指令地址。
    pub len_site: u64,
    /// 取节点调用的地址（三个都调 `0x3acb00`）。
    pub call_site: u64,
}

/// `_ready()` 取的三个 holder 节点 —— **不是屏**，别混进 [`Screen`]。
///
/// 档位：**观测**。三个各被取**一次**：
/// * `ScreensControl`（14）= 屏控件的父节点；
/// * `IAP_holder`（10）= 内购持有者（`_ready` 里连 `purchase_confirmed` /
///   `purchase_denied` / `request_permissions_result` 一类信号）；
/// * `Ad_holder`（9）= 广告持有者，只经 [`FN_SHOW_SCREEN`] 使用。
///
/// ⚠️ **推断**：三者取出来之后 `upcast_ref` 到哪个类。同池的类型名有
/// `graphwarrust::graphwar_button::GraphwarButton`（`0x1c1c5a`，45 字节）、
/// `graphwarrust::graphwar_function_input::GraphwarFunctionInput`（`0x1c1c87`，60 字节）、
/// `graphwarrust::graphwar_ad_switch::GraphwarAdSwitch`（`0x1c1cc3`，50 字节）。
/// 缺的证据：`0x406250` 的第 2 实参没逐条解（§15.9 已登记为缺口）。
pub const READY_NODES: [ReadyNode; 3] = [
    ReadyNode {
        name: "ScreensControl",
        addr: 0x1b_d935,
        len: 14,
        lea_site: 0x3a_3b07,
        len_site: 0x3a_3b16,
        call_site: 0x3a_3b1c,
    },
    ReadyNode {
        name: "IAP_holder",
        addr: 0x1b_d943,
        len: 10,
        lea_site: 0x3a_4129,
        len_site: 0x3a_4138,
        call_site: 0x3a_413e,
    },
    ReadyNode {
        name: "Ad_holder",
        addr: 0x1b_d94d,
        len: 9,
        lea_site: 0x3a_41bc,
        len_site: 0x3a_41cb,
        call_site: 0x3a_41d1,
    },
];

/// 屏表在 `self` 里的起始偏移。档位：**观测**（三个独立点互证）。
///
/// 形状 = `HashMap<&'static str, Gd<Node>>`：`hashbrown` 桶步长 **0x28 = 40 字节**，
/// 控制字节按 **16 个一组**遍历。相对 `+0x1638` 的字段：
///
/// | 相对 | 绝对 | 语义 | 判据（vaddr） |
/// |---|---|---|---|
/// | `+0x00` | `0x1638` | 控制字节指针 | `0x3af883`（`movdqa (%r15)`）；`0x3a57c8` 起的查找内联体 |
/// | `+0x08` | `0x1640` | bucket mask | `0x3af738`（`and %rsi,%rax`） |
/// | `+0x18` | `0x1650` | **条目数** | `0x3af8d7`（`dec %rbx; je`，当循环计数用）；`0x3a57ba`（`cmp qword,0`） |
/// | `+0x20` | `0x1658` | hasher | `0x3af71f`（`lea 0x20(%rdi),%rdi` → `call 0x47daa0`） |
///
/// ⚠️ 这正是 `docs/16` §15.5 旧读法要更正的地方：那里把 `[rdi+0x18]` 读成
/// 「`Option<Gd<AdHolder>>`，没有就返回」。但同一条 `dec %rbx` 把它当**循环计数**，
/// 而且 `0x3a57ba` 用**同一个偏移**判「屏表空」—— 两处一起看，它是**屏表长度**，
/// 不是广告持有者（广告持有者在第 4 实参那个 16 字节捕获态的 `+8`）。见 §15.10 更正 ③。
pub const SCREENS_MAP_OFFSET: u64 = 0x1638;

/// `_ready()` 用到的池字面量（`.rodata` `0x1bd956` 起**首尾相接**的一段）。档位：**观测**。
///
/// ⚠️ **推断**：各自角色。前四条是信号名（`connect` 口径的间接调用，但 `connect`
/// 用的 GOT 槽 `0x9bb030` 至今未定名，见 `docs/14` §9.2）；`VersionLabel` 更像节点名；
/// `process` 更像方法/属性名。缺的证据：该 GOT 槽的符号。
///
/// 最后两条**不是** `_ready` 用的，是切屏时的 `call_deferred` 方法名
/// （[`AD_VIEW_METHODS`]）—— 放在这里只因为它们与前面几条在同池相邻，方便一起对地址。
pub const READY_POOL_LITERALS: [(&str, u64, u32); 8] = [
    ("request_permissions_result", 0x1b_d956, 26),
    ("on_request_permissions_result", 0x1b_d970, 29),
    ("purchase_confirmed", 0x1b_d98d, 18),
    ("purchase_denied", 0x1b_d99f, 15),
    ("VersionLabel", 0x1b_d9ae, 12),
    ("process", 0x1b_d9ba, 7),
    ("create_ad_view", 0x1b_d9c1, 14),
    ("destroy_ad_view", 0x1b_d9cf, 15),
];

// ---------------------------------------------------------------------------
// 4. 命令总线：12 条键（`0x3b0830`，`docs/16` §15.6）
// ---------------------------------------------------------------------------

/// 命令总线认识的一条键。档位：**观测**（逐条见 [`Command::evidence`]）。
///
/// 键是**小写**命令名，来自按钮的 `input_text`；屏名（[`Screen`]）是另一套
/// **大小写混合**的名字，两者在 `0x3b0830` 里混着用 —— 所以切屏只能靠
/// 「命令 → 写 `[+0x1704]` / 调 [`FN_SHOW_SCREEN`]」这条路，
/// 不存在 `show_screen(name)` 那种以屏名为入参的单一入口（§15.8 第 3 条）。
///
/// ⚠️ **推断**：12 条**不是**全集。函数体里还有 8 / 0x14 / 0xf / 6 / 3… 等长度分支。
/// 缺的证据：把那份 131 KB 反编译逐分支过一遍（本轮只做长度分支 + 立即数还原）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Command {
    /// `"player_settings"`，15 字节。
    PlayerSettings,
    /// `"start_level"`，11 字节。
    StartLevel,
    /// `"multiplayer"`，11 字节。
    Multiplayer,
    /// `"create_game"`，11 字节。
    CreateGame,
    /// `"toggle_mute"`，11 字节。
    ToggleMute,
    /// `"exit_campaign"`，13 字节。
    ExitCampaign,
    /// `"ask_exit_play"`，13 字节。
    ///
    /// ⚠️ `docs/16` §15.6 的旧表写的是 `exit_ask_play` —— **词序反了**，
    /// 立即数是 `"ask_exit"` + `"xit_play"`，见 §15.10 更正 ④。
    AskExitPlay,
    /// `"cancel_add_bot"`，14 字节。
    CancelAddBot,
    /// `"exit_lan_lobby"`，14 字节。
    ExitLanLobby,
    /// `"cancel_join_ip"`，14 字节。
    CancelJoinIp,
    /// `"add_bot"`，7 字节。
    AddBot,
    /// `"join_ip"`，7 字节。
    JoinIp,
}

/// 一条命令键的证据：两次「窗口比对」的指令地址。
///
/// 原版的比较写法很固定（**观测**）：
///
/// ```text
/// movabs $<前 8 字节>,%rax ; xor (%r12),%rax
/// movabs $<后 8 字节>,%rcx ; xor 0x<len-8>(%r12),%rcx
/// or %rax,%rcx ; je <命中>
/// ```
///
/// 第二个窗口的**偏移** `len-8` 把键长钉死了（两个窗口的并集正好覆盖整条键）。
/// 7 字节的键用 4 字节窗口，偏移 `len-4 = 3`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandEvidence {
    /// 第一个窗口（`xor (%r12)` 之前的 `movabs`/`mov`）地址。
    pub first_window: u64,
    /// 第二个窗口（`xor 0x<offset>(%r12)` 之前的 `movabs`/`mov`）地址。
    pub second_window: u64,
    /// 第二个窗口在键字符串里的字节偏移：`>= 8` 字节的键取 `len-8`，更短的取 `len-4`。
    pub second_offset: u8,
}

impl Command {
    /// 命令键原文。档位：**逐位**（由两条窗口立即数按小端还原，见 [`Command::evidence`]）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Command::PlayerSettings => "player_settings",
            Command::StartLevel => "start_level",
            Command::Multiplayer => "multiplayer",
            Command::CreateGame => "create_game",
            Command::ToggleMute => "toggle_mute",
            Command::ExitCampaign => "exit_campaign",
            Command::AskExitPlay => "ask_exit_play",
            Command::CancelAddBot => "cancel_add_bot",
            Command::ExitLanLobby => "exit_lan_lobby",
            Command::CancelJoinIp => "cancel_join_ip",
            Command::AddBot => "add_bot",
            Command::JoinIp => "join_ip",
        }
    }

    /// 键的字节长度。档位：**观测**（= 第二窗口偏移 + 窗口宽度）。
    pub const fn len(self) -> u8 {
        match self {
            Command::PlayerSettings => 15,
            Command::StartLevel => 11,
            Command::Multiplayer => 11,
            Command::CreateGame => 11,
            Command::ToggleMute => 11,
            Command::ExitCampaign => 13,
            Command::AskExitPlay => 13,
            Command::CancelAddBot => 14,
            Command::ExitLanLobby => 14,
            Command::CancelJoinIp => 14,
            Command::AddBot => 7,
            Command::JoinIp => 7,
        }
    }

    /// 两次窗口比对的指令地址。档位：**观测**（全部落在 `0x3b0830` 内）。
    pub const fn evidence(self) -> CommandEvidence {
        let (first_window, second_window, second_offset) = match self {
            Command::PlayerSettings => (0x3b_0886, 0x3b_0894, 7),
            Command::StartLevel => (0x3b_16b7, 0x3b_16c5, 3),
            Command::Multiplayer => (0x3b_16dd, 0x3b_16eb, 3),
            Command::CreateGame => (0x3b_1703, 0x3b_1711, 3),
            Command::ToggleMute => (0x3b_1729, 0x3b_1737, 3),
            Command::ExitCampaign => (0x3b_1ed4, 0x3b_1ee2, 5),
            Command::AskExitPlay => (0x3b_1efa, 0x3b_1f08, 5),
            Command::CancelAddBot => (0x3b_21d0, 0x3b_21de, 6),
            Command::ExitLanLobby => (0x3b_21f6, 0x3b_2204, 6),
            Command::CancelJoinIp => (0x3b_221c, 0x3b_222a, 6),
            Command::AddBot => (0x3b_0a50, 0x3b_0a59, 3),
            Command::JoinIp => (0x3b_0a6b, 0x3b_0a74, 3),
        };
        CommandEvidence {
            first_window,
            second_window,
            second_offset,
        }
    }

    /// 命令键 → 枚举；不认识返回 `None`（**不 panic**）。
    pub fn from_name(name: &str) -> Option<Command> {
        ALL_COMMANDS.iter().copied().find(|c| c.as_str() == name)
    }
}

/// 12 条命令键，顺序 = [`Command`] 的声明序（= `docs/16` §15.6 表序）。
pub const ALL_COMMANDS: [Command; 12] = [
    Command::PlayerSettings,
    Command::StartLevel,
    Command::Multiplayer,
    Command::CreateGame,
    Command::ToggleMute,
    Command::ExitCampaign,
    Command::AskExitPlay,
    Command::CancelAddBot,
    Command::ExitLanLobby,
    Command::CancelJoinIp,
    Command::AddBot,
    Command::JoinIp,
];

impl core::fmt::Display for Command {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// 5. 字段表与构造期初值（`docs/16` §15.3）
// ---------------------------------------------------------------------------

/// `[self+0xNN]` 字段偏移。只收**有写入点/比较点地址**的那几个；语义没解的标 `未知`。
pub mod field {
    /// 主开关（低字节 bool）：为 0 时状态机 [`super::FN_STATE_MACHINE`] 直接跳 `0x3a60f0`。
    ///
    /// 判据：读 `0x3a5789`；写 `0x3a60e6`(0)、`0x3a8ae9`(0)、`0x3b1ff2`、`0x3b398f`。
    /// 构造期置 0：`0x3a2b25`（`movl $0x0,0x15c8(%r15)`）。
    /// 档位：**观测**（读写点）；语义**推断**（缺：运行时观测它挡住的是哪条路径）。
    pub const MAIN_SWITCH: usize = 0x15c8;

    /// 倒计数（dword）：读一次、`dec` 一次，就跳到 `0x3a60f0` 收工。
    ///
    /// 判据：`0x3a5796`–`0x3a57a2`、`0x3a61d5`–`0x3a61e1`。
    /// 档位：**观测**（行为）；语义**未知**（谁给它设值、单位是什么都没解）。
    pub const COUNTDOWN: usize = 0x15cc;

    /// 构造期写入 `0x3b9aca00` = `1_000_000_000`（指令 @`0x3a2b4d`）。
    ///
    /// ⚠️ `docs/16` §15.3 把 `+0x15d8` 记作「一个 `String` + 长度的返回值落点」，
    /// 而构造期在这里写了一个 32 位立即数 `1e9`（恰好是「1 秒」的纳秒数）。两说未合，
    /// 见 §15.10 未知 ⑤。档位：**观测**（写入点）；语义**未知**。
    pub const NANOS_OR_LEN: usize = 0x15d8;

    /// 屏表起点，见 [`super::SCREENS_MAP_OFFSET`]。
    pub const SCREENS: usize = super::SCREENS_MAP_OFFSET as usize;

    /// play 侧控制器（指针→单元；`[单元+0x10]` 借用标记、`[单元+0x18]` 载荷），约 50 处引用。
    ///
    /// 判据：`0x3a6a7f`、`0x3b0b75` 等；`_ready` 里从 `0x3a3249` 起写 8 处。
    pub const PLAY_CONTROL: usize = 0x1668;

    /// 另一个控制器 / 数据对象，同形状。判据：`0x3a5f27`、`0x3a6a60`、`0x3b0b49`。
    pub const OTHER_CONTROL: usize = 0x16c8;

    /// 闸门 A：为 1 时状态机直接跳 `0x3a60f0`。
    ///
    /// 判据：读 `0x3a5774`、`0x3a65f1`；写 `0x3b3996`(1)、`0x3a8af3`(0)、`0x3b4094`(0)。
    pub const GATE_A: usize = 0x1702;

    /// 闸门 B：判据：读 `0x3a57ad`、`0x3a61ec`；写 `0x3b2130`(1)、`0x3b3ad4`(1)、
    /// `0x3b50d4`(0)。语义**推断**（与 A 同一个「状态机要不要早退」的判据族）。
    pub const GATE_B: usize = 0x1703;

    /// **当前屏**（u8）：观测到 0/1/2/3/4 五个值，**只在** [`super::FN_COMMAND_BUS`] 里被写。
    ///
    /// 全部写入点（本轮全量扫 `movb $imm,0x1704(%reg)`，8 处，**全在** `[0x3b0830, 0x3b4ec0)` 内）：
    /// `0x3b08ac`(3)、`0x3b0931`(0)、`0x3b0996`(1)、`0x3b237f`(4)、`0x3b2caa`(0)、
    /// `0x3b2e24`(0)、`0x3b3168`(2)、`0x3b3416`(0)。档位：**观测**（地址与值）。
    ///
    /// 值 → 哪个屏：**推断**（1 = `MainMenu`，见 [`super::Screen::MainMenu`] 的注）。
    pub const CURRENT_SCREEN: usize = 0x1704;

    /// **对局状态**（u8）：全库**只有一个**写入点 `0x3a8a09`（`mov %r15b,0x1705(%rbx)`）。
    ///
    /// 比较点：`0x3a6e7e`(=8)、`0x3a7cd8`(=6)、`0x3afd1a`(=6)；读点 `0x3a6c0d`。
    ///
    /// ⚠️ **未知**：枚举定义没解出来。已知能落到写入点的候选值至少 {1, 6, 7, 8}
    /// （`mov $N,%r15b` 的指令：`0x3a6e7b`=8、`0x3a6f59`=6、`0x3a7467`=1、
    /// `0x3a7f53`=7、`0x3a89a0`=1）。`docs/14` §2 的服务端状态机只有 6 态
    /// （最大下标 5），所以值 6/8 **不可能**是那 6 个状态之一 ⇒ 另有一个枚举。
    pub const GAME_STATE: usize = 0x1705;
}

/// 构造期一次写 4 个字节的初值字（`0x3a2b30`：`movl $0x8010000,0x1702(%r15)`）。
///
/// 档位：**观测**（指令字节 `41 c7 87 02 17 00 00 00 00 01 08`）。
pub const INIT_WORD_1702: u32 = 0x0801_0000;

/// [`INIT_WORD_1702`] 拆出来的四个字段 = 构造期 `[+0x1702..+0x1705]` 的初值。
///
/// 小端：最低字节落在最低地址 ⇒ `+0x1702` 得 `0x00`、`+0x1703` 得 `0x00`、
/// `+0x1704` 得 `0x01`、`+0x1705` 得 `0x08`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitFields {
    /// `[+0x1702]`，[`field::GATE_A`]。
    pub gate_a: bool,
    /// `[+0x1703]`，[`field::GATE_B`]。
    pub gate_b: bool,
    /// `[+0x1704]`，[`field::CURRENT_SCREEN`] 的原始值。
    pub current_screen: u8,
    /// `[+0x1705]`，[`field::GAME_STATE`] 的原始值。
    pub game_state: u8,
}

/// 把 32 位初值字按小端拆成 4 个字段（见 [`INIT_WORD_1702`]）。
pub const fn decode_init_word(word: u32) -> InitFields {
    let b = word.to_le_bytes();
    InitFields {
        gate_a: b[0] != 0,
        gate_b: b[1] != 0,
        current_screen: b[2],
        game_state: b[3],
    }
}

/// 构造期真正写下去的那组初值。档位：**观测**（`0x3a2b30`）。
pub const fn constructor_init_fields() -> InitFields {
    decode_init_word(INIT_WORD_1702)
}

/// 构造体热路径里**另外**能读到的立即数写入：`(写入指令, 偏移, 值)`。档位：**观测**。
///
/// 只收带立即数的写；从栈上搬进来的那些（`movups 0x8e(%rsp),%xmm0` 一类）
/// **不在**此表 —— 那是形参/成员拷贝，不是初值。
///
/// ⚠️ **未知**：`+0x90` / `+0xa0` 各写 `3`，语义没解（既可能是枚举值，
/// 也可能是 gdext 内部字段）。
pub const CONSTRUCTOR_IMM_STORES: [(u64, usize, u64); 8] = [
    (0x3a_2a5b, 0x00, 0),
    (0x3a_2a62, 0x20, 0),
    (0x3a_2a75, 0x90, 3),
    (0x3a_2a80, 0xa0, 3),
    (0x3a_2ab9, 0x40, 0),
    (0x3a_2ac1, 0x58, 0),
    (0x3a_2ac9, 0x70, 0),
    (0x3a_2b30, 0x1702, INIT_WORD_1702 as u64),
];

// ---------------------------------------------------------------------------
// 6. 切屏 / 广告：`0x3af850` 的决策（纯逻辑，可测）
// ---------------------------------------------------------------------------

/// 对屏表里**一个**条目，`0x3af850` 决定的动作。档位：**观测**（控制流逐条对上）。
///
/// 这条函数做的事一句话：**让「被点名的那个屏」可见，其余全不可见**；两个名字被单列：
/// * `PlayScreen` —— 进出对局要动广告；
/// * `background` —— **完全不动**（连 `set_visible` 都不调）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenSwap {
    /// 什么都不做。只出现在「条目是 `background` 且不是被点名的那个」时
    /// （`0x3af980` 命中后直接 `jmp 0x3af8cd` 跳过 `set_visible`）。
    Leave,
    /// `set_visible(entry, false)` —— `ebp = 0` 后走 `0x3af8a7`。
    Hide,
    /// `set_visible(entry, true)` —— `ebp = 1` 后走 `0x3af8a7`。
    Show,
    /// 先 `call_deferred("create_ad_view")`（`0x3afa1e`），再 `set_visible(entry, false)`。
    /// 触发条件：条目是 `PlayScreen`、**没**被点名、`ads_slot` 且 `play_screen_visible`
    /// 都为真（`0x3af9c7` → `0x3af9d1` → `0x3af9d7`）。
    HideAndCreateAdView,
    /// 先 `call_deferred("destroy_ad_view")`（`0x3afad5`），再 `set_visible(entry, true)`。
    /// 触发条件：条目是 `PlayScreen`、**被**点名、且 `ads_slot` 与 `play_screen_visible`
    /// 都为真（`0x3afa7d` → `0x3afa83` → `0x3afa8e`）。
    ShowAndDestroyAdView,
}

impl ScreenSwap {
    /// 这个决策要不要写可见性、写成什么值。`None` = 不碰。
    pub const fn visible(self) -> Option<bool> {
        match self {
            ScreenSwap::Leave => None,
            ScreenSwap::Hide | ScreenSwap::HideAndCreateAdView => Some(false),
            ScreenSwap::Show | ScreenSwap::ShowAndDestroyAdView => Some(true),
        }
    }

    /// 这个决策要不要 `call_deferred`、调哪个方法。`None` = 不调。
    pub const fn deferred_method(self) -> Option<&'static str> {
        match self {
            ScreenSwap::HideAndCreateAdView => Some(AD_VIEW_METHODS[0]),
            ScreenSwap::ShowAndDestroyAdView => Some(AD_VIEW_METHODS[1]),
            _ => None,
        }
    }
}

/// `call_deferred` 的两个方法名。档位：**逐位**（`.rodata`）+ **观测**（长度立即数 + 调用点）。
///
/// | 方法 | `.rodata` | 长度 | 长度立即数 | 触发 |
/// |---|---|---|---|---|
/// | `create_ad_view` | `0x1bd9c1` | 14 | `mov $0xe,%ecx` @`0x3af9fe` | 离开 `PlayScreen` |
/// | `destroy_ad_view` | `0x1bd9cf` | 15 | `mov $0xf,%ecx` @`0x3afab5` | 进入 `PlayScreen` |
///
/// 两条都走同一个构造调用 `0x39fe10`（实参 `(out, gd, name, len, 8, 0)`），
/// 之后 `call *0x9ba720` 回收临时值 —— **推断**：那是 `Variant` 的析构，
/// 真正把它交给引擎的是 `0x39fe10` 内部（`call_deferred` 口径）。
/// 缺的证据：`0x39fe10` 的符号、`0x9ba720` 的 GOT 槽名。
pub const AD_VIEW_METHODS: [&str; 2] = ["create_ad_view", "destroy_ad_view"];

/// `0x3af850` 里被**单列**的两个条目名。档位：**逐位**（立即数还原）。
///
/// * `background` —— `0x3af965`（`movabs $0x756f72676b636162` = `"backgrou"`）+ `0x3af976`（`xor $0x646e` = `"nd"`）；
/// * `PlayScreen` —— `0x3af98a`（`movabs $0x6572635379616c50` = `"PlayScre"`）+ `0x3af99b`（`xor $0x6e65` = `"en"`）。
///
/// ⚠️ **未知**：`background` **不在** [`Screen`] 里，而且全库**没有任何指令或重定位**
/// 引用 `.rodata` 里独立的 `background` 字面量（三处出现都是更长串的一部分，
/// 例如 `"background connect error"`）。那个分支的键从哪来，**没解出来** ——
/// 见 §15.10 未知 ③。
pub const SPECIAL_MAP_KEYS: [&str; 2] = ["background", "PlayScreen"];

/// 给定的条目名 vs 被点名的屏名，`0x3af850` 对它做什么。
///
/// # 参数
///
/// * `entry` —— 屏表里**这一个**条目的键；
/// * `queried` —— 调用方想显示的屏名（第 2/3 实参 `&str`）。调用点实测：
///   `0x3a7bed`(`"PlayScreen"`,10)、`0x3a899b`(`"Campaign"`,8)、
///   `0x3b3f2b`(`"MainMenu"`/`"Campaign"`/`"LanLobby"`/`"ReplayLobby"`, 8/8/8/11)、
///   `0x3b410f`(`"ReplayLobby"`,11)、`0x3b4257` —— 全是 [`Screen`] 里的名字（**观测**）。
/// * `ads_slot` —— 第 4 实参指向的 16 字节捕获态里的 bool（`rcx+0`，
///   `cmpb $0x0,(%rax)` @`0x3af9dc` / `0x3afa93`）。
///   **推断**：它是「这次切屏允许动广告」。依据：`0x3b0830` 内 4 个调用点把它写死成
///   `false`（`0x3b3ec4`/`0x3b3ed7`/`0x3b3ef7`/`0x3b3f0a` 都是 `xor eax,eax` 后存 0），
///   而 `0x3a7bed`（切到 `PlayScreen`）写死成 `true`（`0x3a7bd2`）。
///   缺的证据：语义命名 —— 得靠运行时读那个捕获态。
/// * `play_screen_visible` —— `call *0x9baa20` 对 `PlayScreen` 节点取回的 bool
///   （只在该条目是 `PlayScreen` 时才取：`0x3af9c7`、`0x3afa7d`）。
///   **推断**：`is_visible()`；缺的证据：`0x9baa20` 的 GOT 槽符号（§15.9 已登记）。
///
/// # 决策表（观测）
///
/// | 条目 | 命中？ | `ads_slot` | `play_screen_visible` | 结果 |
/// |---|---|---|---|---|
/// | 任意（非 `PlayScreen`） | 命中 | — | — | [`ScreenSwap::Show`]（`0x3afa54`） |
/// | `PlayScreen` | 命中 | 0 | — | [`ScreenSwap::Show`]（`0x3afa8e`） |
/// | `PlayScreen` | 命中 | 1 | false | [`ScreenSwap::Show`]（`0x3afa8e`） |
/// | `PlayScreen` | 命中 | 1 | true | [`ScreenSwap::ShowAndDestroyAdView`]（`0x3afab5`） |
/// | `PlayScreen` | 未命中 | 0 | — | [`ScreenSwap::Hide`]（`0x3af9d1`） |
/// | `PlayScreen` | 未命中 | 1 | false | [`ScreenSwap::Hide`]（`0x3af9d1`） |
/// | `PlayScreen` | 未命中 | 1 | true | [`ScreenSwap::HideAndCreateAdView`]（`0x3af9fe`） |
/// | `background` | 未命中 | — | — | [`ScreenSwap::Leave`]（`0x3af980`） |
/// | 其它 | 未命中 | — | — | [`ScreenSwap::Hide`]（`0x3af8a0`） |
pub fn screen_swap(
    entry: &str,
    queried: &str,
    ads_slot: bool,
    play_screen_visible: bool,
) -> ScreenSwap {
    let hit = entry == queried;
    let is_play = entry == SPECIAL_MAP_KEYS[1];
    let is_background = entry == SPECIAL_MAP_KEYS[0];

    if hit {
        if is_play && ads_slot && play_screen_visible {
            ScreenSwap::ShowAndDestroyAdView
        } else {
            ScreenSwap::Show
        }
    } else if is_background {
        ScreenSwap::Leave
    } else if is_play && ads_slot && play_screen_visible {
        ScreenSwap::HideAndCreateAdView
    } else {
        ScreenSwap::Hide
    }
}

/// 整张屏表过一遍，按**条目顺序**产出 `(条目, 决策)`。
///
/// ⚠️ 顺序：原版是 `HashMap` 迭代（16 个控制字节一组、桶步长 40 字节），
/// **迭代序未知**；本函数只保调用方给的顺序，不声称与原版一致。
/// 要逐事件对拍必须运行时观测一遍（缺的证据：迭代序）。
pub fn screen_swap_plan<'a>(
    entries: &'a [&'a str],
    queried: &str,
    ads_slot: bool,
    play_screen_visible: bool,
) -> Vec<(&'a str, ScreenSwap)> {
    entries
        .iter()
        .map(|e| (*e, screen_swap(e, queried, ads_slot, play_screen_visible)))
        .collect()
}

/// 「按名字切屏」这一整套动作要在宿主上做的事。**意图结构，不调 Godot。**
///
/// 原版对应三条 gdext 调用：
/// * `upcast_ref` + `call *0x9ba978`（第 2 实参 = bool）→ [`ScreenHost::set_visible`]；
/// * `0x39fe10` + `call *0x9ba720`（`(out, gd, name, len, 8, 0)`）→ [`ScreenHost::call_deferred`]；
/// * `call *0x9baa20`（无参、返回 bool）→ [`ScreenHost::play_screen_visible`]。
///
/// ⚠️ **推断**：三条 GOT 槽的成员名（`0x9ba978` / `0x9baa20` / `0x9ba720`）都还没定名，
/// 见 `docs/16` §15.9。
pub trait ScreenHost {
    /// 把 `screen` 的可见性置成 `visible`。
    fn set_visible(&mut self, screen: &str, visible: bool);
    /// 在 `Ad_holder` 上延后调用 `method`（`create_ad_view` / `destroy_ad_view`）。
    fn call_deferred(&mut self, method: &str);
    /// `PlayScreen` 节点现在的可见性（原版只对它取这一个 bool）。
    fn play_screen_visible(&self) -> bool;
}

/// 把 [`screen_swap`] 的决策落到宿主上，**顺序同原版**：
/// `call_deferred` 先于 `set_visible`（`0x3afa1e`、`0x3afad5` 都在跳回
/// `0x3af8a7` 写可见性之前）。
pub fn apply_screen_swap<H: ScreenHost + ?Sized>(host: &mut H, entry: &str, swap: ScreenSwap) {
    if let Some(method) = swap.deferred_method() {
        host.call_deferred(method);
    }
    if let Some(visible) = swap.visible() {
        host.set_visible(entry, visible);
    }
}

/// 一次性把整张表过完（原版 `0x3af850` 的循环体）。
///
/// `play_screen_visible` 只在遇到 `PlayScreen` 条目时才问宿主 —— 与原版一致
/// （`call *0x9baa20` 在 `PlayScreen` 的两个分支里各取一次）。
pub fn apply_show_screen<H: ScreenHost + ?Sized>(
    host: &mut H,
    entries: &[&str],
    queried: &str,
    ads_slot: bool,
) {
    for entry in entries {
        let needs_play_visible = *entry == SPECIAL_MAP_KEYS[1];
        let play_visible = needs_play_visible && host.play_screen_visible();
        let swap = screen_swap(entry, queried, ads_slot, play_visible);
        apply_screen_swap(host, entry, swap);
    }
}

// ---------------------------------------------------------------------------
// 7. 未知清单（照实写；`AGENTS.md` §0 第 3 条）
// ---------------------------------------------------------------------------
//
// 1. `[+0x1705]`（GAME_STATE）的枚举定义：候选值至少 {1,6,7,8}、比较点 {6,8}，
//    但没有一个值能对上 `docs/14` §2 的 6 态服务端状态机。缺：`0x3a8a09` 处 `r15b`
//    的完整定值分析（本轮只取了紧邻的 `mov $N,%r15b`）。
// 2. `0x3b0830` 的**全部**命令键：已解 12 条（[`Command`]）；函数体还有
//    8 / 0x14 / 0xf / 6 / 3… 等长度分支没过完。缺：131 KB 反编译的逐分支审计。
// 3. `background` 这个键从哪来：全库无独立字面量、无重定位引用（见 [`SPECIAL_MAP_KEYS`]）。
//    ⇒ 该分支可能是死分支/防御性代码，也可能屏表的键不是取自 `.rodata` 的 `&'static str`。
// 4. `0x9ba978` / `0x9baa20` / `0x9baa10` / `0x9ba720` / `0x39fe10` 的引擎侧符号：
//    只能确认「收 bool」「返回 bool」「收 (out, gd, name, len, 8, 0)」「收一个临时值」。
// 5. `[+0x15d8]` 构造期写 `1e9` 与 §15.3 的「String + 长度落点」两说未合。
// 6. 屏表 V 的确切类型、`upcast_ref` 的目标类名、`HashMap` 的**迭代序**。
// 7. `0x3a22f0` 到底是构造器还是 `Clone::clone`（见 [`FN_CONSTRUCT`] 的注）。
// 8. `0x3a5760` 的 Rust 形状：20288 字节 / 64 个直接调用目标，极可能是 `_process` 级入口
//    + 大量内联；本文件只给它的一小段（进 `PlayScreen` 那条路）落了码。
// 9. `ConfirmPlay`(11) / `ExitPopUp`(9) 这两个键在代码里**没有**以 11/9 长度出现的证据：
//    唯一一处 `lea 0x1bdb0d` 配的是长度 20，而那 20 字节正好是
//    `"ConfirmPlayExitPopUp"`（`0x3b3783` → 尾调用 `0x3af700`）。它到底是
//    「原版写错了键名（于是查不到、无动作）」还是「屏表里真有一个 20 字节的键」，**未解**。

// ---------------------------------------------------------------------------
// 8. `check_save_level`（`0x3afcf0`）与 `request_permissions_result`（`0x3b4ec0`）
//    —— 2026-11-27（T71）按指令落码；本节的**纯逻辑**放在这里（不带 gdext 也能编），
//    两个方法体挂在该文件末尾的 `impl GraphwarRootNode` 上（那个 impl 在
//    `#[cfg(feature = "gdext-bindings")] mod node_class` 里，见该模块头注）。
// ---------------------------------------------------------------------------
//
// 两条都是 `GraphwarRootNode` 的 **v2 真名**方法（`fixtures/boundary_audit.tsv` 的
// `v2唯一锚`；`docs/02-module-map.md` §18.3 给了 `x86 地址 → 原版源行`：
// `check_save_level` = `0x3afcf0` ↔ `graphwar_root_node.rs:656`、
// `request_permissions_result` = `0x3b4ec0` ↔ `:861`）。本轮之前**只有地址、没有函数体**
// （`fixtures/function_coverage.tsv` 判 F3 `register-only` / `named-miss`）。
// 逐条反汇编、判决与门槛读数见 `docs/06` §120；台账见 `fixtures/f3_heavy_anchors.tsv`。
//
// 落码口径：本文件**不调 Godot**（见文件头「本文件不做什么」），所以这里落的是
// **纯逻辑 + 显式输入**：原版从 `self` 的固定偏移读字段、从 `Rc<RefCell<_>>` 里取对象，
// 这里把这些都变成入参，由调用方（节点层或测试）负责喂。字段偏移一律保留，
// 因为**偏移本身就是证据**（每一条都能指到一条 `mov/cmp [reg+off]`）。

/// `check_save_level` 用到的 `GraphwarRootNode` 字段偏移与判据常数。
/// 档位：**观测**（每条都给了指令地址；地址口径 ELF vaddr）。
pub mod save_level {
    /// `self+0x1668` = 对局控制器（`Rc<RefCell<_>>`：`+0` 强引用计数、
    /// `+0x10` 借用标记、`+0x18` 载荷）。同 [`super::field::PLAY_CONTROL`]。
    pub const PLAY_CONTROL: usize = 0x1668;
    /// 载荷 `+0x1e0`（= 单元 `+0x1f8`）那个 qword：**为 0 就直接返回**。
    /// 判据：`0x3afd43` `cmp qword [r14+0x1f8], 0` / `0x3afd4b je`。
    /// 语义**未知**（候选：控制器是否已挂上某个子对象）——缺的证据：该字段的写入点。
    pub const PLAY_CONTROL_GATE: usize = 0x1f8;
    /// 载荷 `+0x220`（= 单元 `+0x238`）= 模块容器 `Rc<RefCell<>>`（`docs/16` §41）。
    /// 判据：`0x3afd51` `mov r12,[r14+0x238]`。
    pub const MODULE_CONTAINER: usize = 0x238;
    /// `get_module::<T>()` 返回对象 `+0x137` 的 bool：**非 0 就不写存档**。
    /// 判据：`0x3afd95` `movzx ebp, byte [rax+0x137]` + `0x3afdb0 test bpl,bpl`。
    /// 哪个 manager、哪一位**未知**（缺：`0x46be70` 的 `TypeId`）。
    pub const MODULE_FLAG: usize = 0x137;
    /// `self+0x16d0` = 战役控制器（`None` ⇒ 不写）。判据：`0x3afdb5` `mov r14,[rbx+0x16d0]`。
    /// ⚠️ 与 [`super::field::OTHER_CONTROL`]（`0x16c8`）**不是**同一个字段（旧注释未区分）。
    pub const CAMPAIGN_CONTROL: usize = 0x16d0;
    /// `self+0xb0`：它同时是「关卡标识」那个 `&String` 实参（`0x3afddb mov rsi,rbx`，
    /// `rbx = self+0xb0`，随后 `call 0x4183b0` 的第 2 实参），又是一个 8 字节字。
    /// 那 8 字节位**未知**（候选：`Option`/`GString` 的 niche 哨兵、或"未初始化"标记）。
    pub const LEVEL_KEY: usize = 0xb0;
    /// `[+0x1705]`（[`super::field::GAME_STATE`]）必须等于这个值才继续。
    /// 判据：`0x3afd1a` `cmp byte [rdi+0x1705], 6` / `0x3afd21 jne`。
    /// ⚠️ 该枚举的**定义没解出来**（见未知清单第 1 条）⇒ 这里只给原始判别值，**不命名**。
    pub const REQUIRED_GAME_STATE: u8 = 6;
    /// `[self+0xb0]` 与之相等就直接返回。判据：`0x3afcfc` `movabs r15,0x7fff…ff`、
    /// `0x3afd06` `lea rax,[r15+1]`、`0x3afd0a` `cmp [rdi+0xb0],rax` ⇒ `i64::MAX as u64 + 1`。
    pub const LEVEL_KEY_SENTINEL: u64 = (i64::MAX as u64) + 1;
    /// 写存档那一步的**唯一**被调者：`0x3afdde` `call 0x4183b0`
    /// （= `campaign.save` 的写者；`addr_probe.py callers 0x4183b0` = 全 `.text` 仅此 1 处）。
    pub const SAVE_WRITER: u64 = 0x41_83b0;
}

/// `check_save_level` 的判定结果。档位：**观测**（每个变体 = 一条返回指令）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveLevelDecision {
    /// `0x3afd11` / `0x3afd21` / `0x3afd4b` / `0x3afdb3` / `0x3afdbf`：直接返回。
    NotNow(SaveLevelSkip),
    /// `0x3afdde`：调 `0x4183b0`（`campaign.save` 的写者），实参 = (战役控制器载荷, `&self+0xb0`)。
    SaveCurrentLevel,
}

/// `SaveLevelDecision::NotNow` 的五个早退点（顺序 = 指令顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveLevelSkip {
    /// `0x3afd11`：`[self+0xb0] == 0x8000_0000_0000_0000`。
    LevelKeySentinel,
    /// `0x3afd21`：`[self+0x1705] != 6`。
    GameStateNotRequired,
    /// `0x3afd4b`：对局控制器载荷 `+0x1e0`（单元 `+0x1f8`）为 0。
    PlayControlGate,
    /// `0x3afdb3`：`get_module::<T>()` 对象的 `+0x137` 非 0。
    ModuleFlagSet,
    /// `0x3afdbf`：`self+0x16d0`（战役控制器）为 `None`。
    NoCampaignControl,
}

/// `check_save_level` 的输入（= 原版从 `self` 与两个 `Rc<RefCell<_>>` 里读到的全部东西）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveLevelInputs {
    /// `[self+0xb0]` 那 8 字节（原样，不解释；见 [`save_level::LEVEL_KEY`]）。
    pub level_key_word: u64,
    /// `[self+0x1705]`。
    pub game_state: u8,
    /// 对局控制器 `[载荷+0x1e0]` 是否非 0（原版 `[单元+0x1f8] != 0`）。
    pub play_control_gate: bool,
    /// `get_module::<T>()` 对象的 `[+0x137] != 0`。
    ///
    /// 原版这一跳是 `ModuleContainer::get_module::<T>().unwrap()`：`0x3afd73` 取 `Option`、
    /// `0x3afd78` 为 `None` 时走 `0x3afe16` 的 panic（panic 的 `Location` 指向
    /// **`src/graphwar_game_client.rs:152:22`**，说明那个 `.unwrap()` 写在该文件、被内联进本函数）。
    /// 本模型把"模块在不在"也当输入：`None` ⇒ 原版**会 panic**，这里也用 panic（不静默返回）。
    pub module_flag: Option<bool>,
    /// 战役控制器在不在（`self+0x16d0` 是否 `Some`）。
    pub campaign_control: bool,
}

/// `request_permissions_result`（`0x3b4ec0`）用的偏移与字面量。档位：**观测**。
pub mod permissions_result {
    /// `self+0x16b0` = 日志缓冲：`Rc<RefCell<Vec<String>>>`。
    ///
    /// 判据：`0x3b4ff2` `mov r12,[r15+0x16b0]`；`0x3b4ff9`/`0x3b5014`/`0x3b50ad` 是
    /// `RefCell` 的 borrow/释放（`[单元+0x10]` = 借用标记，`[单元+0x18]` = `Vec` 头：
    /// `+0x18` ptr、`+0x20` cap、`+0x28` len）；`0x3b5093` `lea rcx,[r15+r15*2]` +
    /// `0x3b5097` 三个 qword 写入 ⇒ **元素 24 字节 = `String`**。
    pub const LOG_BUFFER: usize = 0x16b0;
    /// `self+0x1703` = [`super::field::GATE_B`]：本函数把它**写 0**。
    /// 判据：`0x3b50d4` `mov byte [rax+0x1703], 0`（`field::GATE_B` 的注里早就登记了这条
    /// 写入点，本轮才知道它属于哪个函数）。另外两个写 1 的点是 `0x3b2130` / `0x3b3ad4`。
    pub const GATE_B: usize = 0x1703;
    /// 消息前缀，`.rodata` `0x1a95d0`（首字节 `0x1e` = **长度 30**，串从 `0x1a95d1` 起）。
    /// 档位：**逐位**（字节读回 = `"Request permissions returned. "`，30 字节）。
    pub const MESSAGE_PREFIX: &str = "Request permissions returned. ";
    /// 那个「取虚表第 0x11e0 项调用」的全局单例槽：`0x3b50db` `mov rax,[0x9c18f0]`、
    /// `0x3b50e2` `movzx ecx, byte [rax+0x14f0]`、`0x3b50f6` `call [rax+0x11e0]`。
    /// **未定名**（缺：`0x9c18f0` 这个类的符号）⇒ 本文件不模拟它。
    pub const GLOBAL_HOOK: u64 = 0x9c_18f0;
}

/// 一次权限询问的结果（`request_permissions_result` 的三个入参去掉 `self`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionsResult {
    /// 第 2 实参（`0x3b4ed4` `mov [rsp+0x50], rsi`）：权限名（字符串类）。
    pub permission: String,
    /// 第 3 实参的低字节（`0x3b4ed9` `mov [rsp+6], dl`）：是否授予。
    pub granted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 记录宿主动作的顺序，用来验证 `call_deferred` 先于 `set_visible`。
    #[derive(Default)]
    struct Recorder {
        log: Vec<String>,
        play_visible: bool,
        asks: usize,
    }

    impl ScreenHost for Recorder {
        fn set_visible(&mut self, screen: &str, visible: bool) {
            self.log.push(format!("visible {screen} {}", visible as u8));
        }
        fn call_deferred(&mut self, method: &str) {
            self.log.push(format!("deferred {method}"));
        }
        fn play_screen_visible(&self) -> bool {
            self.play_visible
        }
    }

    /// 包一层，统计 `play_screen_visible` 被问了几次（原版只在 `PlayScreen` 分支问）。
    struct CountingHost {
        inner: Recorder,
        asked: std::cell::Cell<usize>,
    }

    impl ScreenHost for CountingHost {
        fn set_visible(&mut self, screen: &str, visible: bool) {
            self.inner.set_visible(screen, visible);
        }
        fn call_deferred(&mut self, method: &str) {
            self.inner.call_deferred(method);
        }
        fn play_screen_visible(&self) -> bool {
            self.asked.set(self.asked.get() + 1);
            self.inner.play_visible
        }
    }

    #[test]
    fn screen_len_matches_name_bytes() {
        for s in ALL_SCREENS {
            assert_eq!(
                s.len() as usize,
                s.name().len(),
                "{} 的长度必须等于字节数",
                s.name()
            );
            assert!(!s.name().is_empty());
        }
    }

    #[test]
    fn screen_table_matches_enum() {
        assert_eq!(SCREEN_TABLE.len(), ALL_SCREENS.len());
        for (i, s) in ALL_SCREENS.iter().enumerate() {
            let (name, addr, len, site) = SCREEN_TABLE[i];
            assert_eq!(name, s.name(), "第 {i} 项名字");
            assert_eq!(addr, s.addr(), "{} 的 .rodata 地址", s.name());
            assert_eq!(len, s.len(), "{} 的字节长度", s.name());
            assert_eq!(site, s.len_immediate_site(), "{} 的长度立即数", s.name());
        }
    }

    #[test]
    fn screen_addr_and_len_are_the_verified_ones() {
        // 逐条对上 `.rodata`：`readv.py <addr>` 读回的字节 == 名字；长度 == 字节数。
        // 与 docs/16 §15.10 的表一致；EndGamePopUp 是 12（旧表写 11，是错的）。
        let want: [(&str, u64, u32); 15] = [
            ("MainMenu", 0x19fc70, 8),
            ("Campaign", 0x19fcc0, 8),
            ("LanLobby", 0x19f380, 8),
            ("GameSetup", 0x1bda32, 9),
            ("PlayScreen", 0x1bd92b, 10),
            ("ReplayLobby", 0x1bdaec, 11),
            ("PlayerScreen", 0x1bdb2f, 12),
            ("JoinIpPopUp", 0x1bdaf7, 11),
            ("AddBotPopUp", 0x1bdb02, 11),
            ("ConfirmPlay", 0x1bdb0d, 11),
            ("ExitPopUp", 0x1bdb18, 9),
            ("JoinLevelPopUp", 0x1bdb21, 14),
            ("ConnectingPopUp", 0x1bda23, 15),
            ("EndGamePopUp", 0x1bda3b, 12),
            ("DisconnectedPopUp", 0x1bda73, 17),
        ];
        for ((n, a, l), s) in want.iter().zip(ALL_SCREENS.iter()) {
            assert_eq!(*n, s.name());
            assert_eq!(*a, s.addr());
            assert_eq!(*l, s.len());
        }
    }

    /// `.rodata` 里的紧邻关系是长度的**第二个独立来源**：`addr + len` 必须
    /// 正好等于下一个字面量的起点。
    #[test]
    fn screen_addr_plus_len_hits_next_literal() {
        let pairs = [
            (Screen::PlayScreen, "ScreensControl", 0x1bd935u64),
            (Screen::ConnectingPopUp, "GameSetup", 0x1bda32u64),
            (Screen::GameSetup, "EndGamePopUp", 0x1bda3bu64),
            (Screen::ReplayLobby, "JoinIpPopUp", 0x1bdaf7u64),
            (Screen::JoinIpPopUp, "AddBotPopUp", 0x1bdb02u64),
            (Screen::AddBotPopUp, "ConfirmPlay", 0x1bdb0du64),
            (Screen::ConfirmPlay, "ExitPopUp", 0x1bdb18u64),
            (Screen::ExitPopUp, "JoinLevelPopUp", 0x1bdb21u64),
            (Screen::JoinLevelPopUp, "PlayerScreen", 0x1bdb2fu64),
            (Screen::PlayerScreen, "LevelTitleLabel", 0x1bdb3bu64),
        ];
        for (s, next_name, next_addr) in pairs {
            assert_eq!(
                s.addr() + s.len() as u64,
                next_addr,
                "{} 之后应当紧接着 {next_name}",
                s.name()
            );
        }
    }

    #[test]
    fn screen_name_round_trip_both_ways() {
        assert_eq!(ALL_SCREENS.len(), 15);
        for s in ALL_SCREENS {
            assert_eq!(Screen::from_name(s.name()), Ok(s), "{} 往返", s.name());
            assert_eq!(s.name().parse::<Screen>(), Ok(s));
            assert_eq!(Screen::try_from(s.name()), Ok(s));
            assert_eq!(s.to_string(), s.name());
        }
        // 15 个名字互不相同（双向唯一）
        let mut names: Vec<&str> = ALL_SCREENS.iter().map(|s| s.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 15);
    }

    #[test]
    fn unknown_screen_name_is_an_error_not_a_panic() {
        for bad in [
            "",
            "playScreen",
            "PlayScren",
            "MainMenu ",
            " MainMenu",
            "background",
            "ScreensControl",
            "IAP_holder",
            "Ad_holder",
            "player_settings",
        ] {
            assert_eq!(Screen::from_name(bad), Err(UnknownScreenName), "{bad:?}");
            assert!(bad.parse::<Screen>().is_err(), "{bad:?}");
        }
        // 非屏的节点名不能被当成屏
        for n in READY_NODES {
            assert!(Screen::from_name(n.name).is_err(), "{} 不是屏", n.name);
        }
        assert!(Screen::from_name(SPECIAL_MAP_KEYS[0]).is_err());
        assert!(Screen::from_name(SPECIAL_MAP_KEYS[1]).is_ok());
        assert!(!UnknownScreenName.to_string().is_empty());
    }

    #[test]
    fn len_immediate_sites_are_inside_referencing_functions() {
        // 长度立即数必须落在引用了该屏名的函数里：0x3a5760 或 0x3b0830 两段之内。
        for s in ALL_SCREENS {
            if let Some(site) = s.len_immediate_site() {
                assert!(
                    (0x3a_5760..0x3a_a6a0).contains(&site) || (0x3b_0830..0x3b_4ec0).contains(&site),
                    "{} 的长度立即数 {site:#x} 不在已知引用函数里",
                    s.name()
                );
            } else {
                assert!(
                    matches!(s, Screen::ConfirmPlay | Screen::ExitPopUp),
                    "{} 不该缺长度证据",
                    s.name()
                );
            }
        }
    }

    #[test]
    fn popup_grouping_is_7_plus_8() {
        let popups = ALL_SCREENS.iter().filter(|s| s.is_popup_by_name()).count();
        assert_eq!(popups, 8, "docs/16 §15.2：8 个弹窗");
        assert_eq!(ALL_SCREENS.len() - popups, 7, "=> 7 个基础屏");
        assert!(Screen::ConfirmPlay.is_popup_by_name());
        assert!(!Screen::PlayScreen.is_popup_by_name());
        assert!(!Screen::MainMenu.is_popup_by_name());
    }

    #[test]
    fn ready_nodes_are_three_and_not_screens() {
        assert_eq!(READY_NODES.len(), 3);
        let want = [
            ("ScreensControl", 0x1bd935u64, 14u32),
            ("IAP_holder", 0x1bd943, 10),
            ("Ad_holder", 0x1bd94d, 9),
        ];
        for (n, (name, addr, len)) in READY_NODES.iter().zip(want.iter()) {
            assert_eq!(n.name, *name);
            assert_eq!(n.addr, *addr);
            assert_eq!(n.len, *len);
            assert_eq!(n.name.len(), *len as usize);
            assert!(Screen::from_name(n.name).is_err(), "{} 不是屏", n.name);
            // 三条指令的先后：lea < 长度 < 取节点
            assert!(n.lea_site < n.len_site, "{} 的 lea 在长度之前", n.name);
            assert!(n.len_site < n.call_site, "{} 的长度在调用之前", n.name);
        }
        // 三个 holder 在 .rodata 里首尾相接
        assert_eq!(READY_NODES[0].addr + READY_NODES[0].len as u64, READY_NODES[1].addr);
        assert_eq!(READY_NODES[1].addr + READY_NODES[1].len as u64, READY_NODES[2].addr);
    }

    #[test]
    fn ready_pool_literals_are_contiguous_in_rodata() {
        for w in READY_POOL_LITERALS.windows(2) {
            let (n0, a0, l0) = w[0];
            let (n1, a1, l1) = w[1];
            assert_eq!(a0 + l0 as u64, a1, "{n0} 之后应当紧接着 {n1}");
            assert_eq!(n0.len(), l0 as usize, "{n0} 的长度");
            assert_eq!(n1.len(), l1 as usize, "{n1} 的长度");
        }
    }

    #[test]
    fn commands_round_trip_and_lengths() {
        assert_eq!(ALL_COMMANDS.len(), 12);
        for c in ALL_COMMANDS {
            assert_eq!(Command::from_name(c.as_str()), Some(c));
            assert_eq!(
                c.as_str().len(),
                c.len() as usize,
                "{} 长度必须等于字节数",
                c.as_str()
            );
            assert_eq!(c.to_string(), c.as_str());
        }
        assert_eq!(Command::from_name("PlayScreen"), None);
        assert_eq!(Command::from_name("ADD_BOT"), None);
        assert_eq!(Command::from_name(""), None);
        // 12 条互不相同
        let mut v: Vec<&str> = ALL_COMMANDS.iter().map(|c| c.as_str()).collect();
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), 12);
    }

    #[test]
    fn command_windows_pin_the_key_length() {
        for c in ALL_COMMANDS {
            let ev = c.evidence();
            let width = if c.len() >= 8 { 8 } else { 4 };
            assert_eq!(
                c.len() as usize - width,
                ev.second_offset as usize,
                "{} 的第二窗口偏移",
                c.as_str()
            );
            // 两次 mov 的先后
            assert!(ev.second_window > ev.first_window, "{}", c.as_str());
            // 两条窗口立即数的间距：8 字节窗口 0xe（movabs+movabs），4 字节窗口 9
            let gap = if width == 8 { 0xe } else { 0x9 };
            assert_eq!(
                ev.second_window - ev.first_window,
                gap,
                "{} 的两窗口间距",
                c.as_str()
            );
            // 全都在命令总线里
            assert!((FN_COMMAND_BUS..FN_COMMAND_BUS + 0x4690).contains(&ev.first_window));
            assert!((FN_COMMAND_BUS..FN_COMMAND_BUS + 0x4690).contains(&ev.second_window));
        }
    }

    #[test]
    fn command_keys_are_the_verified_ones() {
        let want: [(&str, u8); 12] = [
            ("player_settings", 15),
            ("start_level", 11),
            ("multiplayer", 11),
            ("create_game", 11),
            ("toggle_mute", 11),
            ("exit_campaign", 13),
            ("ask_exit_play", 13),
            ("cancel_add_bot", 14),
            ("exit_lan_lobby", 14),
            ("cancel_join_ip", 14),
            ("add_bot", 7),
            ("join_ip", 7),
        ];
        for (c, (k, l)) in ALL_COMMANDS.iter().zip(want.iter()) {
            assert_eq!(c.as_str(), *k);
            assert_eq!(c.len(), *l);
        }
        // 旧表把第 7 条写成 exit_ask_play —— 词序反了，这里钉住正确写法
        assert_ne!(Command::AskExitPlay.as_str(), "exit_ask_play");
    }

    #[test]
    fn layout_offsets_match_docs() {
        assert_eq!(field::GATE_A, 0x1702);
        assert_eq!(field::GATE_B, 0x1703);
        assert_eq!(field::CURRENT_SCREEN, 0x1704);
        assert_eq!(field::GAME_STATE, 0x1705);
        assert_eq!(SCREENS_MAP_OFFSET, 0x1638);
        assert_eq!(field::SCREENS as u64, SCREENS_MAP_OFFSET);
        // 4 个连写字段必须连续
        assert_eq!(field::GATE_B, field::GATE_A + 1);
        assert_eq!(field::CURRENT_SCREEN, field::GATE_A + 2);
        assert_eq!(field::GAME_STATE, field::GATE_A + 3);
        // 屏表的四个字段（控制字节 / mask / 条目数 / hasher）
        assert_eq!(SCREENS_MAP_OFFSET + 0x18, 0x1650);
        assert_eq!(SCREENS_MAP_OFFSET + 0x20, 0x1658);
    }

    #[test]
    fn constructor_init_word_decodes_to_00_00_01_08() {
        let f = constructor_init_fields();
        assert!(!f.gate_a);
        assert!(!f.gate_b);
        assert_eq!(f.current_screen, 1);
        assert_eq!(f.game_state, 8);
        assert_eq!(INIT_WORD_1702, 0x0801_0000);
        // 小端拆字节：最低字节 -> 最低地址
        assert_eq!(INIT_WORD_1702.to_le_bytes(), [0x00, 0x00, 0x01, 0x08]);
        // 拆解方向必须对（否则 4 个字段会整体错位）
        assert!(decode_init_word(0x0000_0001).gate_a);
        assert!(decode_init_word(0x0000_0100).gate_b);
        assert_eq!(decode_init_word(0x0001_0000).current_screen, 1);
        assert_eq!(decode_init_word(0x0100_0000).game_state, 1);
        assert_eq!(decode_init_word(0).game_state, 0);
    }

    #[test]
    fn constructor_immediate_stores_are_consistent() {
        let mut offs: Vec<usize> = CONSTRUCTOR_IMM_STORES.iter().map(|(_, o, _)| *o).collect();
        let n = offs.len();
        offs.sort_unstable();
        offs.dedup();
        assert_eq!(offs.len(), n, "偏移不得重复");
        let w = CONSTRUCTOR_IMM_STORES
            .iter()
            .find(|(_, o, _)| *o == field::GATE_A)
            .expect("0x1702 必须在这张表里");
        assert_eq!(w.2 as u32, INIT_WORD_1702);
        for (site, _, _) in CONSTRUCTOR_IMM_STORES {
            assert!(
                (FN_CONSTRUCT..FN_READY).contains(&site),
                "{site:#x} 不在构造体热路径里"
            );
        }
        // 主开关的 0 值不在「立即数写入」表里也无妨（它在 0x3a2b25，值是 0）：
        // 这里只断言表里 0x58 那条确实是 0。
        let m = CONSTRUCTOR_IMM_STORES
            .iter()
            .find(|(_, o, _)| *o == 0x58)
            .expect("0x58 在表里");
        assert_eq!(m.2, 0);
    }

    #[test]
    fn screen_swap_truth_table() {
        use ScreenSwap::*;
        // 命中且不是 PlayScreen -> Show
        assert_eq!(screen_swap("Campaign", "Campaign", false, false), Show);
        assert_eq!(screen_swap("MainMenu", "MainMenu", true, true), Show);
        // PlayScreen 命中
        assert_eq!(screen_swap("PlayScreen", "PlayScreen", false, true), Show);
        assert_eq!(screen_swap("PlayScreen", "PlayScreen", true, false), Show);
        assert_eq!(
            screen_swap("PlayScreen", "PlayScreen", true, true),
            ShowAndDestroyAdView
        );
        // PlayScreen 未命中
        assert_eq!(screen_swap("PlayScreen", "Campaign", false, true), Hide);
        assert_eq!(screen_swap("PlayScreen", "Campaign", true, false), Hide);
        assert_eq!(
            screen_swap("PlayScreen", "Campaign", true, true),
            HideAndCreateAdView
        );
        // background：未命中什么都不做，命中则显示
        assert_eq!(screen_swap("background", "Campaign", true, true), Leave);
        assert_eq!(screen_swap("background", "background", true, true), Show);
        // 其它未命中 -> Hide
        assert_eq!(screen_swap("Campaign", "MainMenu", true, true), Hide);
    }

    #[test]
    fn screen_swap_accessors() {
        assert_eq!(ScreenSwap::Leave.visible(), None);
        assert_eq!(ScreenSwap::Leave.deferred_method(), None);
        assert_eq!(ScreenSwap::Hide.visible(), Some(false));
        assert_eq!(ScreenSwap::Show.visible(), Some(true));
        assert_eq!(
            ScreenSwap::HideAndCreateAdView.deferred_method(),
            Some("create_ad_view")
        );
        assert_eq!(
            ScreenSwap::ShowAndDestroyAdView.deferred_method(),
            Some("destroy_ad_view")
        );
        assert_eq!(ScreenSwap::HideAndCreateAdView.visible(), Some(false));
        assert_eq!(ScreenSwap::ShowAndDestroyAdView.visible(), Some(true));
        assert_eq!(ScreenSwap::Hide.deferred_method(), None);
        assert_eq!(ScreenSwap::Show.deferred_method(), None);
    }

    #[test]
    fn ad_view_method_offsets_match_rodata() {
        assert_eq!(AD_VIEW_METHODS[0], "create_ad_view");
        assert_eq!(AD_VIEW_METHODS[1], "destroy_ad_view");
        let a = READY_POOL_LITERALS[6];
        let b = READY_POOL_LITERALS[7];
        assert_eq!(a.0, AD_VIEW_METHODS[0]);
        assert_eq!(b.0, AD_VIEW_METHODS[1]);
        assert_eq!(a.2 as usize, a.0.len());
        assert_eq!(b.2 as usize, b.0.len());
        assert_eq!(a.1, 0x1bd9c1);
        assert_eq!(b.1, 0x1bd9cf);
    }

    #[test]
    fn special_keys_are_exactly_background_and_play_screen() {
        assert_eq!(SPECIAL_MAP_KEYS[0], "background");
        assert_eq!(SPECIAL_MAP_KEYS[1], "PlayScreen");
        // PlayScreen 就是 Screen 里那个；background 不是屏
        assert_eq!(Screen::from_name(SPECIAL_MAP_KEYS[1]), Ok(Screen::PlayScreen));
        assert!(Screen::from_name(SPECIAL_MAP_KEYS[0]).is_err());
    }

    #[test]
    fn plan_keeps_entry_order_and_covers_all_entries() {
        let entries = ["MainMenu", "PlayScreen", "background", "Campaign"];
        let plan = screen_swap_plan(&entries, "Campaign", false, true);
        assert_eq!(plan.len(), 4);
        assert_eq!(plan[0], ("MainMenu", ScreenSwap::Hide));
        assert_eq!(plan[1], ("PlayScreen", ScreenSwap::Hide));
        assert_eq!(plan[2], ("background", ScreenSwap::Leave));
        assert_eq!(plan[3], ("Campaign", ScreenSwap::Show));
    }

    #[test]
    fn apply_writes_deferred_before_visible() {
        let mut h = Recorder {
            log: vec![],
            play_visible: true,
            asks: 0,
        };
        apply_screen_swap(&mut h, "PlayScreen", ScreenSwap::ShowAndDestroyAdView);
        assert_eq!(
            h.log,
            vec!["deferred destroy_ad_view", "visible PlayScreen 1"]
        );
        // Leave 一条都不写
        let mut h2 = Recorder::default();
        apply_screen_swap(&mut h2, "background", ScreenSwap::Leave);
        assert!(h2.log.is_empty());
    }

    #[test]
    fn apply_show_screen_matches_the_loop() {
        let entries = ["MainMenu", "PlayScreen", "Campaign", "background"];
        let mut h = Recorder {
            log: vec![],
            play_visible: true,
            asks: 0,
        };
        apply_show_screen(&mut h, &entries, "MainMenu", true);
        // PlayScreen 未命中 + ads_slot + play 可见 -> 先建广告再隐藏；background 不动。
        assert_eq!(
            h.log,
            vec![
                "visible MainMenu 1",
                "deferred create_ad_view",
                "visible PlayScreen 0",
                "visible Campaign 0",
            ]
        );
    }

    #[test]
    fn apply_show_screen_asks_play_visibility_only_for_play_screen() {
        let entries = ["MainMenu", "Campaign"];
        let mut h = CountingHost {
            inner: Recorder::default(),
            asked: std::cell::Cell::new(0),
        };
        apply_show_screen(&mut h, &entries, "MainMenu", true);
        assert_eq!(h.asked.get(), 0, "没有 PlayScreen 条目就不该问");
        let entries = ["MainMenu", "PlayScreen"];
        let mut h = CountingHost {
            inner: Recorder::default(),
            asked: std::cell::Cell::new(0),
        };
        apply_show_screen(&mut h, &entries, "MainMenu", true);
        assert_eq!(h.asked.get(), 1, "有 PlayScreen 条目就问一次");
    }

    #[test]
    fn module_funcs_are_twenty_three_unique() {
        assert_eq!(MODULE_FUNCS.len(), 23);
        let mut v = MODULE_FUNCS.to_vec();
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), 23, "23 条必须互不相同");
        assert!(!MODULE_FUNCS.contains(&FN_CONSTRUCT), "构造器不在那 23 条里");
        for f in [FN_READY, FN_STATE_MACHINE, FN_COMMAND_BUS, FN_BUSY_CHECK] {
            assert!(MODULE_FUNCS.contains(&f), "{f:#x} 应当是可归属函数");
        }
        // 全部落在 .text（0x37b940 .. 0x94eb84）
        for f in MODULE_FUNCS {
            assert!((0x37_b940..0x94_eb84).contains(&f), "{f:#x}");
        }
        // 0x3af850 / 0x3af700 没有 panic 锚，因此不在清单里 —— 但它们确实在模块区间内
        assert!(!MODULE_FUNCS.contains(&FN_SHOW_SCREEN));
        assert!(!MODULE_FUNCS.contains(&FN_SCREEN_GET));
        for f in [FN_SHOW_SCREEN, FN_SCREEN_GET] {
            assert!((0x3a_2e10..0x3b_4ec0).contains(&f), "{f:#x}");
        }
    }
}
// ---------------------------------------------------------------------------
// T71：`check_save_level`（0x3afcf0）/ `request_permissions_result`（0x3b4ec0）
// 期望值**全部来自反汇编立即数**，不是本实现的输出（`AGENTS.md` §0 第 2 条）。
// ⚠️ 必须 `feature = "gdext-bindings"`：被调的 `GraphwarRootNode` 在本仓挂在
// `#[cfg(feature = "gdext-bindings")] mod node_class`（原版不需要 cfg，见该模块头注）。
// ---------------------------------------------------------------------------
#[cfg(all(test, feature = "gdext-bindings"))]
mod t71_tests {
    use crate::graphwar_root_node::*;

    fn ok_inputs() -> SaveLevelInputs {
        SaveLevelInputs {
            level_key_word: 0x1234, // 任意非哨兵值
            game_state: save_level::REQUIRED_GAME_STATE,
            play_control_gate: true,
            module_flag: Some(false),
            campaign_control: true,
        }
    }

    /// 哨兵 = `0x3afd06 lea rax,[r15+1]`（r15 = `0x3afcfc movabs 0x7fffffffffffffff`）。
    #[test]
    fn sentinel_is_i64_max_plus_one() {
        assert_eq!(save_level::LEVEL_KEY_SENTINEL, 0x8000_0000_0000_0000);
        assert_eq!(save_level::LEVEL_KEY_SENTINEL, i64::MAX as u64 + 1);
        assert_eq!(save_level::LEVEL_KEY_SENTINEL.to_le_bytes(), [0, 0, 0, 0, 0, 0, 0, 0x80]);
    }

    /// 常数 = 指令里的立即数本体（不是从本实现反推的）。
    #[test]
    fn constants_are_the_instruction_immediates() {
        assert_eq!(save_level::REQUIRED_GAME_STATE, 6);       // 0x3afd1a cmp byte [rdi+0x1705], 6
        assert_eq!(save_level::PLAY_CONTROL, 0x1668);         // 0x3afd27 mov r14,[rbx+0x1668]
        assert_eq!(save_level::MODULE_CONTAINER, 0x238);      // 0x3afd51 mov r12,[r14+0x238]
        assert_eq!(save_level::PLAY_CONTROL_GATE, 0x1f8);     // 0x3afd43 cmp qword [r14+0x1f8],0
        assert_eq!(save_level::MODULE_FLAG, 0x137);           // 0x3afd95 movzx ebp,byte [rax+0x137]
        assert_eq!(save_level::CAMPAIGN_CONTROL, 0x16d0);     // 0x3afdb5 mov r14,[rbx+0x16d0]
        assert_eq!(save_level::LEVEL_KEY, 0xb0);              // 0x3afddb mov rsi,rbx (rbx = self+0xb0)
        assert_eq!(save_level::SAVE_WRITER, 0x41_83b0);       // 0x3afdde call 0x4183b0
        assert_eq!(permissions_result::GATE_B, 0x1703);       // 0x3b50d4 mov byte [rax+0x1703],0
        assert_eq!(permissions_result::GATE_B, field::GATE_B);
        assert_eq!(permissions_result::LOG_BUFFER, 0x16b0);   // 0x3b4ff2 mov r12,[r15+0x16b0]
        assert_eq!(permissions_result::MESSAGE_PREFIX.len(), 0x1e); // 0x1a95d0 首字节 0x1e
    }

    /// 每个提前返回点都**单独**可触发（顺序 = 指令顺序，前者优先）。
    #[test]
    fn each_skip_point_is_reachable() {
        let i = ok_inputs();
        assert_eq!(GraphwarRootNode::check_save_level(&i), SaveLevelDecision::SaveCurrentLevel);

        let mut i = ok_inputs();
        i.level_key_word = save_level::LEVEL_KEY_SENTINEL;
        assert_eq!(GraphwarRootNode::check_save_level(&i),
                   SaveLevelDecision::NotNow(SaveLevelSkip::LevelKeySentinel));

        let mut i = ok_inputs();
        i.game_state = 8; // 0x3afd1a 只认 6；8 是 GAME_STATE 的另一已知值（field::GAME_STATE 的注）
        assert_eq!(GraphwarRootNode::check_save_level(&i),
                   SaveLevelDecision::NotNow(SaveLevelSkip::GameStateNotRequired));

        let mut i = ok_inputs();
        i.play_control_gate = false;
        assert_eq!(GraphwarRootNode::check_save_level(&i),
                   SaveLevelDecision::NotNow(SaveLevelSkip::PlayControlGate));

        let mut i = ok_inputs();
        i.module_flag = Some(true);
        assert_eq!(GraphwarRootNode::check_save_level(&i),
                   SaveLevelDecision::NotNow(SaveLevelSkip::ModuleFlagSet));

        let mut i = ok_inputs();
        i.campaign_control = false;
        assert_eq!(GraphwarRootNode::check_save_level(&i),
                   SaveLevelDecision::NotNow(SaveLevelSkip::NoCampaignControl));
    }

    /// 优先级：哨兵先于 game_state（`0x3afd11` 在 `0x3afd1a` 之前）。
    #[test]
    fn skip_points_are_ordered_like_the_instructions() {
        let i = SaveLevelInputs {
            level_key_word: save_level::LEVEL_KEY_SENTINEL,
            game_state: 0,
            play_control_gate: false,
            module_flag: Some(true),
            campaign_control: false,
        };
        assert_eq!(GraphwarRootNode::check_save_level(&i),
                   SaveLevelDecision::NotNow(SaveLevelSkip::LevelKeySentinel));
    }

    /// `get_module::<T>()` 为 `None` 时原版**panic**（`0x3afd78 je 0x3afe16`）⇒ 本模型也 panic。
    #[test]
    #[should_panic(expected = "0x3afd78")]
    fn missing_module_panics_like_the_original() {
        let mut i = ok_inputs();
        i.module_flag = None;
        let _ = GraphwarRootNode::check_save_level(&i);
    }

    /// 三步效果：push 一条进日志缓冲、GATE_B 清零、返回被 push 的那条。
    #[test]
    fn permissions_result_logs_and_clears_gate_b() {
        let mut log: Vec<String> = Vec::new();
        let mut gate_b = true; // 别的写入点（0x3b2130 / 0x3b3ad4）会把它置 1
        let msg = GraphwarRootNode::request_permissions_result(
            &mut log,
            &mut gate_b,
            &PermissionsResult {
                permission: "android.permission.WRITE_EXTERNAL_STORAGE".to_string(),
                granted: true,
            },
        );
        assert_eq!(log.len(), 1);
        assert_eq!(log[0], msg);
        assert!(!gate_b, "0x3b50d4 把 GATE_B 写 0");
        assert!(msg.starts_with("Request permissions returned. "));
        assert!(msg.contains("WRITE_EXTERNAL_STORAGE"));
    }
}


// ===========================================================================
// 原版方法体：原版 `GraphwarRootNode` 的 gdext 节点类（`Node` 子类）（T57 从 `src/graphwar_root_node.rs` 搬回）
// ===========================================================================
#[cfg(feature = "gdext-bindings")]
mod node_class {
//! [T57 已搬回原版模块] 这里是**原版 原版 `GraphwarRootNode` 的 gdext 节点类（`Node` 子类）** 的方法体。
//!
//! 2026-11-27 从 `src/graphwar_root_node.rs` 搬进 `src/graphwar_root_node.rs`：原版把 `#[derive(GodotClass)]`
//! 的 struct 与方法体**写在同一个 `graphwar_root_node.rs` 里**，证据：
//! `panic::Location` 证据：`process 0x3a5760` → `src/graphwar_root_node.rs:635:22`（56 个锚）、`ready 0x3a2e10` → `:1302:9`、`purchase_confirmed 0x3afe90` → `:868:35`、`purchase_denied 0x3afb00` → `:892:35`。
//!
//! **本仓唯一的结构差异**：原版整个 crate 都是 gdext 扩展（不需要 `cfg`）；本仓 `cargo test`
//! （不带 `gdext-bindings`）必须能编，所以这一层挂在
//! `#[cfg(feature = "gdext-bindings")] mod node_class` 下，由 `pub use node_class::*;` 把对外路径
//! 抬平 ⇒ `crate::graphwar_root_node::<类名>` 这类路径与原版一致。
//!
//! ---
//! 以下是搬运前的原始说明（保留：里面逐条写着与原版的差别与证据档）。
//!
//!
//! # 这一份：**骨架 + 3 个 manager 的真装配**（`docs/16-client.md` §38）
//!
//! 原版 `GraphwarRootNode` 是主场景节点（`docs/16-client.md` §15），它干三件事：
//!
//! 1. **装配**：把屏表（15 个 `Control`）、两个控制器句柄（`+0x1668` / `+0x16c8` 的
//!    `Rc<RefCell<T>>`）、贴图加载器（`+0x16a0`）、以及模块容器里的一批 manager 接起来；
//! 2. **切屏**：`0x3af850` —— 按名字让一个屏可见、其余不可见，进出口时
//!    `call_deferred("create_ad_view"|"destroy_ad_view")`；
//! 3. **状态机**：`0x3a5760`（20288 字节、339 条比较、64 个直接调用目标）+ 命令总线 `0x3b0830`。
//!
//! **本文件做三件事**（第 1 条只做了"22 个 manager 的装配"那一小块，见下）：
//!
//! * 建出 `_ready` 里真的 `get_node` 过的那 3 个 holder（[`READY_NODES`]，**观测**）：
//!   `ScreensControl` / `IAP_holder` / `Ad_holder`，其中 `ScreensControl` 下挂 15 个屏节点、
//!   `Ad_holder` 是**另一个扩展类** [`super::graphwar_ad_switch::GraphwarAdSwitch`]；
//! * [`GraphwarRootNode::show_screen`] 真正跑一遍 `graphwar_root_node::apply_show_screen`
//!   （`0x3af850` 决策表的纯逻辑版），把 `set_visible` **转发到真子节点**上；
//! * [`GraphwarRootNode::purchase_confirmed`] / [`GraphwarRootNode::purchase_denied`]
//!   把一次调用**转发给子对象** `Ad_holder`（arm64 `0x3a658c` / `0x3a61b0` 就是写那个
//!   `+0x18` 字节的两条指令）。
//!
//! # 真装配：**22 个 manager 按原版注册序进容器**（`docs/16-client.md` §41）
//!
//! 原版容器里 **22 条 `add_module`**（`GraphwarGameClient::new`，`docs/02` §22.4）；
//! 本类现在把这 **22 个全装上**，装配器在 [`super::root_wiring`]：
//!
//! ```text
//! GraphwarRootNode（本类）
//! ├── ScreensControl
//! │   ├── GameSetup        ← **真节点类** GraphwarSetupControl（Setup 屏）
//! │   ├── PlayScreen       ← **Game 屏**（Control 容器）
//! │   │   └── GraphwarPlane            ← 已有节点类（画棋盘框）
//! │   │       └── @EntityDrawer@N      ← 已有节点类（3 假实体 × 4 子节点）
//! │   └── …其余 13 个屏（Control 占位）
//! ├── IAP_holder / Ad_holder
//! └── GraphwarModuleContainer（25 个 **占位** Node；真装配在 Rust 侧的 TypeId 表里）
//! ```
//!
//! * 容器 = `Rc<RefCell<GraphwarModuleContainer>>`，键是 **`TypeId`**（`docs/02` §22.1），
//!   取回走 `get_module::<M>()`；**没有任何 `+0xNN` 偏移**；
//! * 按类型逐个可查：`container_has_module("GraphwarTurnManager")` 之类，
//!   还能再走一档 `container_module_live(...)`（真 downcast 回来）；
//!   22 型按**四档**记账（2026-11-08 任务 28 起，判据见
//!   [`super::root_wiring`] 的模块文档与 `docs/16` §47）：`wired` 4 / `layout` 2 /
//!   `assembled` 15 / `stub` **1**（只剩 `GraphwarPlanePosManager`：24 B 布局未知）。
//!   ⚠️ 旧文写的"17 个复原类型 + 5 个零行为空占位"是**三档时代**的口径，已作废；
//!   客户端**不**注册的那 3 型
//!   （`GraphwarBanList` / `GraphwarServerConnectionHandler` / `GraphwarGameRecorder`）
//!   如实回 `placeholder`（[`GraphwarRootNode::module_slot_status`]）；
//! * 屏里的 3 个假实体与容器里 `GameStateManager` 的 3 个 `Target` **是同一份数据**
//!   （[`super::root_wiring::DEMO_SOLDIERS`]）。
//!
//! **仍然刻意不做**（`AGENTS.md` §0 第 3 条：没证据就不猜）：
//!
//! | 没做的东西 | 为什么 |
//! |---|---|
//! | `0x3a5760`（20288 B 状态机）的主体 | 原版形状本身没解出来（可能是 `_process` + 大量内联），不猜 |
//! | `0x3b0830`（命令总线）的**全部**键 | 只解到 12 条（[`crate::graphwar_root_node::Command`]），131 KB 反编译没过完 |
//! | **17 个 manager 的行为** | 本轮只做"构造 + 注册"（任务书 §4 明令不实现行为）；容器里是**空表**，没人驱动它们 |
//! | **只剩 1 个槽结构未知** | `GraphwarPlanePosManager`（24 B 布局未知、无事件流）⇒ 档位 `stub`。另外四型已升级：`ClientConnectionHandler`(112 B) / `FunctionManager`(32 B) 布局全解但行为未做完 ⇒ `layout`；`TickHandler` / `ObstacleManager` ⇒ `wired`（`docs/16` §47.2） |
//! | 两个 `Rc<RefCell<T>>`（`+0x1668` / `+0x16c8`） | 那两个控制器的类型与生命周期未解（`docs/16` §15.3 只给了偏移） |
//! | `GraphwarTextureLoader`（`+0x16a0`） | 模块存在（`docs/16` §28），但节点层还没有 |
//!
//! ⚠️ `GraphwarModuleContainer` 节点下的 25 个空 `Node` 名字**就是**原版类名
//! （`GraphwarTurnManager` 之类），但它们仍然只是空节点——**真装配在 Rust 的容器里**，
//! 不在节点树里。两处别混：`module_slot_status()` 说的是**容器**里装没装。
//!
//! # 基类：`Node`（**观测**）
//!
//! arm64 2.0.60 的 `.symtab` 直接把基类写进了符号：
//!
//! ```text
//! <GraphwarRootNode as godot_core::gen::classes::node::re_export::INode>::ready
//! <GraphwarRootNode as godot_core::gen::classes::node::re_export::INode>::process
//! ```
//!
//! ⇒ 基类 = `Node`、且**覆写了 `ready` 与 `process`**（这两条都不是猜的）。
//! 12 个类的同一份对照表见 `docs/16-client.md` 本轮新增的那一节。
//!
//! # 换基类要改什么（本批三个类的实测结论）
//!
//! `Node` 用 `impl INode`、`VBoxContainer` 用 `impl IVBoxContainer`、`Control` 用 `impl IControl`
//! —— **只有那个块要改**（trait 名 + `Base<T>` 类型 + `init` 的参数类型，共 3 处）。
//! `#[godot_api] impl 类名 { #[func] … #[export] … }` 与基类**无关**，一个字都不用改。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/base_classes_check.gd
//! ```
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! GraphwarRootNode : Node
//!   @export ads_slot : bool        # 0x3af850 的第 4 实参（推断：这次切屏允许动广告）
//!
//!   build_scene_graph() -> int     # 幂等；建 3 个 holder + 15 个屏 + N 个 module 占位
//!   holder_names() -> PackedStringArray
//!   screen_count() -> int
//!   screen_names() -> PackedStringArray
//!   show_screen(name) -> int       # 跑一遍 0x3af850 的决策表，返回写了多少次 set_visible
//!   visible_screens() -> PackedStringArray
//!   set_screen_visible(name, visible) -> bool
//!   deferred_log() -> PackedStringArray    # create_ad_view / destroy_ad_view 的调用顺序
//!   ad_holder_present() -> bool
//!   ads_enabled() -> bool                  # 转发给 Ad_holder
//!   purchase_confirmed() / purchase_denied() -> bool   # 原版写 +0x18 的两条
//!   manager_slot_count() -> int
//!   ready_call_count() / process_call_count() -> int
//!
//!   # ---- §42 真装配（容器侧）----
//!   wiring_done() -> bool                         # 容器已经装好（build_scene_graph 会做）
//!   container_module_count() -> int               # 容器里几个模块（= HashMap.len()）
//!   container_module_names() -> PackedStringArray # **按类型查成立**的那几个，注册序
//!   container_has_module(type_name) -> bool       # 按 TypeId 查（不是按数量）
//!   container_game_state_soldiers() -> int        # 取回 GameStateManager 再读（士兵数）
//!   container_turn_manager_soldiers(player_id) -> int
//!   container_player_manager_players() -> int
//!   module_slot_status(type_name) -> String       # "assembled" / "placeholder" / "unknown"
//!   wired_manager_count() -> int                  # 按类型查成立的个数（= 3）
//! ```



use std::collections::BTreeMap;

use godot::classes::{Control, INode, Node};
use godot::prelude::*;

use crate::graphwar_logger::GraphwarLogger;
use crate::graphwar_module_container::{self, MODULES};
use crate::graphwar_root_node::{apply_show_screen, Screen, ScreenHost, ALL_SCREENS, READY_NODES};

use crate::entity_drawer::EntityDrawer;
use crate::graphwar_ad_switch::GraphwarAdSwitch;
use crate::graphwar_plane::GraphwarPlane;
use crate::graphwar_setup_control::GraphwarSetupControl;
use crate::_scaffold::root_wiring::{self, RootWiring, DEMO_SOLDIERS, Wired};
// T71：`check_save_level` / `request_permissions_result` 的纯逻辑写在本文件的第 8 节
// （`mod node_class` 之外，这样不带 `gdext-bindings` 也能编），这里把它们抬进来。
use crate::graphwar_root_node::{
    permissions_result, save_level, PermissionsResult, SaveLevelDecision, SaveLevelInputs,
    SaveLevelSkip,
};

/// `_ready` 里三个 holder 的节点名（[`READY_NODES`]，**观测**）。
///
/// 顺序同原版：`ScreensControl` → `IAP_holder` → `Ad_holder`。
pub const HOLDER_NAMES: [&str; 3] = [READY_NODES[0].name, READY_NODES[1].name, READY_NODES[2].name];

/// `Ad_holder` 挂的扩展类名（**推断**：`docs/16` §15.4 记的 `upcast_ref` 目标池里
/// 有 `graphwar_ad_switch::GraphwarAdSwitch`，但没有逐条对上哪个 holder 上抛到哪个类）。
pub const AD_HOLDER_CLASS: &str = crate::graphwar_ad_switch::CLASS;

/// `GraphwarGameClient::new` 注册了多少个 manager（`graphwar_module_container::REGISTRATION_SITES[0]`）。
///
/// 档位：**观测**（arm64 `0x409c70` 的注册点数；`graphwar_module_container` 的
/// `ADD_MODULE_COUNT` = 25 是**表里**的模块类型数，两者不同）。
pub const MANAGER_REGISTRATION_COUNT: usize = graphwar_module_container::REGISTRATION_SITES[0].count;

/// **Setup 屏**的屏名（`docs/16` §15.2 的 `.rodata` 原文 `0x1bda32`，9 字节）。
///
/// 与 `Screen::GameSetup.name()` 相等（单测断言，不另写第二份名字）。
pub const SETUP_SCREEN_NAME: &str = "GameSetup";

/// **Game 屏**的屏名（`docs/16` §15.2 的 `.rodata` 原文 `0x1bd92b`，10 字节）。
///
/// ⚠️ 它是 `0x3af850` 里**被单列处理**的那一屏（进出口要动广告），但这一条不影响
/// 本文件把它当成"Game 屏"用：Game 屏的内容（棋盘 / 士兵）挂在这一屏下面。
pub const GAME_SCREEN_NAME: &str = "PlayScreen";

/// Game 屏里棋盘节点的**节点名**。
///
/// 档位：**观测** —— `docs/16` §2.1.10 的运行时 dump 里那条链是
/// `PlayScreen/.../GamePlane/PlaneContainer/GraphwarPlane`，节点自己的名字就是它。
pub const PLANE_NODE_NAME: &str = "GraphwarPlane";

/// Game 屏（以及棋盘 / 绘制器）的尺寸 = `client/project.godot` 的
/// `display/window/size/viewport_width/height`（1600×900）。
///
/// 档位：**我方选择** —— 原版这一屏的尺寸由 `.tscn` 的容器布局给
/// （`PlayScreen/.../PlaneContainer`），我们没有那套场景；写死一个视口尺寸
/// 只是为了让 `entity_drawer::sprite_transform` 的 `k = size.x / 50` 不至于乘 0。
pub const VIEWPORT_SIZE: (f32, f32) = (1600.0, 900.0);

/// 把 `[`crate::graphwar_root_node::ScreenHost`] 落到真子节点上的适配器。
///
/// 原版这三条对应三个 gdext 调用（`docs/16` §15.5）：
/// * `upcast_ref` + `call *0x9ba978`（收一个 bool）→ [`ScreenHost::set_visible`]；
/// * `0x39fe10` + `call *0x9ba720`（`(out, gd, name, len, 8, 0)`）→ [`ScreenHost::call_deferred`]；
/// * `call *0x9baa20`（无参、返回 bool）→ [`ScreenHost::play_screen_visible`]。
///
/// 三条 GOT 槽的**引擎成员名至今未定名**（`docs/16` §15.9），所以这里只写意图结构。
struct NodeScreenHost<'a> {
    /// 屏名 → 真 `Control` 节点（屏表 `+0x1638`）。
    screens: &'a mut BTreeMap<Screen, Gd<Control>>,
    /// `call_deferred` 的方法名流水（`Ad_holder` 上根本没有这两个方法，只记名字）。
    deferred: &'a mut Vec<GString>,
    /// 真的写出了多少次 `set_visible`（观测点）。
    writes: &'a mut i64,
}

impl ScreenHost for NodeScreenHost<'_> {
    /// `set_visible(entry, visible)`。
    ///
    /// 屏表里只有 [`Screen`] 的 15 个名字；`background` 这类键不在枚举里 ⇒ 跳过
    /// （原版里 `background` 命中的分支本来也是 `Leave`，不调 `set_visible`）。
    fn set_visible(&mut self, screen: &str, visible: bool) {
        let Ok(key) = Screen::from_name(screen) else {
            return;
        };
        if let Some(node) = self.screens.get_mut(&key) {
            node.set_visible(visible);
            *self.writes += 1;
        }
    }

    /// `Ad_holder.call_deferred(method)` —— **只记账**。
    ///
    /// 原版这两个方法由 Android 的 AdMob 插件提供（`docs/19-android.md`），
    /// 本仓库没有那个插件；调一个不存在的方法只会往 stderr 喷错误。
    fn call_deferred(&mut self, method: &str) {
        self.deferred.push(GString::from(method));
    }

    /// `PlayScreen` 当前可见性（原版只对它取这一个 bool，`0x3af9c7` / `0x3afa7d`）。
    fn play_screen_visible(&self) -> bool {
        self.screens
            .get(&Screen::PlayScreen)
            .map(|node| node.is_visible())
            .unwrap_or(false)
    }
}

/// `GraphwarRootNode` 的节点骨架。
///
/// 子节点树（建好之后）：
///
/// ```text
/// GraphwarRootNode            ← 本类（Node）
/// ├── ScreensControl         ← 空 Node 容器（原版 ready 里 get_node 过）
/// │   ├── MainMenu … DisconnectedPopUp   （15 个 Control，名字 = Screen::name()）
/// │   ├── GameSetup          ← **Setup 屏**：GraphwarSetupControl（真节点类）
/// │   └── PlayScreen         ← **Game 屏**：Control 容器
/// │       └── GraphwarPlane            ← 棋盘本体（真节点类）
/// │           └── @EntityDrawer@N      ← 实体绘制器（真节点类，自动名）
/// ├── IAP_holder             ← 空 Node 占位（原版 ready 里 get_node 过）
/// ├── Ad_holder              ← GraphwarAdSwitch（另一个扩展类）
/// └── GraphwarModuleContainer
///     └── GraphwarBanList … （25 个空 Node，名字 = 模块类名；**真装配在 Rust 容器里**）
/// ```
#[derive(GodotClass)]
#[class(base = Node)]
pub struct GraphwarRootNode {
    base: Base<Node>,

    /// `0x3af850` 的第 4 实参（那个 16 字节捕获态里的 bool）。
    ///
    /// 档位：**推断**（`0x3b0830` 的 4 个调用点把它写死 0、`0x3a7bed`（切到 `PlayScreen`）
    /// 写死 1 ⇒ 最自然的语义是"这次切屏允许动广告"；缺的证据：运行时读那个捕获态）。
    #[export]
    ads_slot: bool,

    /// 屏表（原版 `self+0x1638` 的 `HashMap<&'static str, Gd<Node>>`；这里按枚举键排，
    /// 因为原版的**迭代序未知**，见 [`crate::graphwar_root_node::screen_swap_plan`] 的注）。
    screens: BTreeMap<Screen, Gd<Control>>,

    /// `Ad_holder` 那个子对象（**推断**它是 [`GraphwarAdSwitch`]）。
    ad_holder: Option<Gd<GraphwarAdSwitch>>,

    /// **已装配的模块容器**（22 个 manager，顺序 = 原版注册序，见 [`crate::_scaffold::root_wiring`]）。
    ///
    /// 原版这条链是 `root+0x1668`（某个 manager 的 `Rc<RefCell<_>>`）→ 该 manager `+0x238`
    /// （`Rc<RefCell<GraphwarModuleContainer>>`）→ `.borrow().get_module::<X>()`
    /// （`docs/02` §22.1）。我们没有那两跳，直接把容器挂在根节点上——**形状不同**，
    /// 但容器的语义（`TypeId` 键 + `Rc<RefCell<M>>` 值）是同一个。
    wiring: Option<RootWiring>,

    /// **Setup 屏**的真节点类（`GameSetup` 那一屏）。
    setup_screen: Option<Gd<GraphwarSetupControl>>,

    /// **Game 屏**里的棋盘本体（`GraphwarPlane`，`PlayScreen/GraphwarPlane`）。
    plane: Option<Gd<GraphwarPlane>>,

    /// 棋盘下的实体绘制器（`GraphwarPlane/@EntityDrawer@N`，**观测**的父子关系）。
    entity_drawer: Option<Gd<EntityDrawer>>,

    /// 三个 holder 是否已经建好（幂等标志）。
    built: bool,

    /// `call_deferred` 的方法名流水。
    deferred_log: Vec<GString>,

    /// `_ready` / `_process` 被调到几次（观测点）。
    ready_calls: i64,
    process_calls: i64,
    /// 累计 `set_visible` 写成功多少次（观测点）。
    visibility_writes: i64,
    /// 【2026-11-26 新增，task-30】`log(String)` 的落点：`graphwar_logger::GraphwarLogger`。
    ///
    /// 原版 `/root/GraphwarRootNode` 上有一个 **1 个参数**的注册方法 `log`，它是 5 份
    /// 原版平台桥 `.gd`（`android_iap` / `IAP_IOS` / `IAP_holder` 的子节点 /
    /// `IOS_album_holder` / `Android_mediastore_holder`）在**桌面上唯一会执行**的那一行。
    ///
    /// * **观测（二进制）**：注册点 `0x3b7328` 附近 `lea 0x1bdb95("log"), len=3` 与
    ///   `lea 0x19e490("GraphwarRootNode"), len=0x10` 同块，随后
    ///   `classdb_register_extension_class_method` 收到 `prtcalls`/`call_func`
    ///   两个回调 `0x3b6010`（ptrcall，`mov $1,%r8d` + `push $1` ⇒ **1 个参数**）与
    ///   `0x3b6340`（call_func）、flags 槽 `0x9524f0`。另两处同形注册块在
    ///   `0x3b604a` / `0x3b6387`；
    /// * **观测（原版字节）**：5 份 `.gd` 共 20 处 `root.log("…")`；
    /// * **推断**：落点是 `GraphwarLogger::log`（`log` 与 `GraphwarLogger` 的
    ///   `Rc<RefCell<_>>` 都在 `<GraphwarRootNode as INode>::init` 里建起来，
    ///   `docs/16` §30.5 记了 `log` ← `RootNode::process_button_pressed` 一条）。缺的
    ///   证据：x86 侧没有把 `GraphwarRootNode::log` 的 body 与 `0x45bc5c` 对上（我只拿到
    ///   注册回调，没拿到 body 归属）⇒ **落点这一跳是推断，不是逐位**。
    /// * **未知**：日志的出口（原版是 `tiny_http` 端点；HTTP 报文格式未读出）。
    ///   这里**不猜**：只做数据面（一行进缓冲），不打印、不开端口。
    logger: GraphwarLogger,
    /// 最近一次 [`GraphwarRootNode::show_screen`] 的屏名。
    last_queried: GString,
}

impl GraphwarRootNode {
    /// 建出节点骨架（幂等）。原版在 `_ready`（`0x3a2e10`）里装配；这里 `ready` 与每个
    /// `#[func]` 入口都会先调一次，理由同 `graphwar_lan_vbox_class`：`--script` 模式下
    /// 节点可能先进树后建树，验收脚本要在同一帧里断言。
    fn ensure_scene_graph(&mut self) -> i64 {
        if self.built {
            return self.child_slot_count();
        }

        // ⓪ **模块容器装配**（`docs/16` §41）：22 个 manager 按原版注册序、按 `TypeId` 进容器。
        //    先于建屏做，因为 Game 屏里的 3 个假实体与容器里 `GameStateManager` 的
        //    3 个 `Target` 是同一份数据（[`DEMO_SOLDIERS`]）。
        if self.wiring.is_none() {
            self.wiring = Some(RootWiring::assemble_demo());
        }

        // ① ScreensControl + 15 个屏（原版：`get_node("ScreensControl")` 之后写屏表 `+0x1638`）
        let mut screens_control = Node::new_alloc();
        screens_control.set_name(&StringName::from(HOLDER_NAMES[0]));
        for screen in ALL_SCREENS {
            if screen == Screen::GameSetup {
                // Setup 屏：**真节点类** `GraphwarSetupControl`（`docs/16` §35 那个类）。
                // 原版这一屏在 `.tscn` 里，我们这里由这个类自己建替身子节点。
                let mut node = GraphwarSetupControl::new_alloc();
                node.set_name(&StringName::from(SETUP_SCREEN_NAME));
                node.set_visible(false);
                node.set_size(Vector2::new(VIEWPORT_SIZE.0, VIEWPORT_SIZE.1));
                screens_control.add_child(&node.clone().upcast::<Node>());
                self.setup_screen = Some(node.clone());
                // 屏表装的是 `Control`（屏的基类），`GraphwarSetupControl` 是它的子类。
                self.screens.insert(screen, node.upcast::<Control>());
                continue;
            }
            if screen == Screen::PlayScreen {
                // Game 屏：`Control` 容器 + 棋盘 + 实体绘制器（见 `build_game_screen`）。
                // 原版这一屏有 `play_screen_control`（Rust）；我们还没有那个模块的节点层，
                // 所以这里只是一个容器 —— **不是**"已经复原了 PlayScreenControl"。
                let mut node = Control::new_alloc();
                node.set_name(&StringName::from(GAME_SCREEN_NAME));
                node.set_visible(false);
                self.build_game_screen(&mut node);
                screens_control.add_child(&node.clone().upcast::<Node>());
                self.screens.insert(screen, node);
                continue;
            }
            // 其余 13 个屏仍是 `Control` 占位：原版对它们调的是「收一个 bool」的可见性成员
            // （`0x9ba978`，**推断** = `CanvasItem::set_visible`）⇒ 必须是 `CanvasItem` 系。
            let mut node = Control::new_alloc();
            node.set_name(&StringName::from(screen.name()));
            node.set_visible(false);
            screens_control.add_child(&node.clone().upcast::<Node>());
            self.screens.insert(screen, node);
        }
        self.base_mut()
            .add_child(&screens_control.upcast::<Node>());

        // ② IAP_holder：原版只 `get_node` 一次（内购信号挂在它上面），本骨架留空占位
        let mut iap = Node::new_alloc();
        iap.set_name(&StringName::from(HOLDER_NAMES[1]));
        self.base_mut().add_child(&iap.upcast::<Node>());

        // ③ Ad_holder：**另一个扩展类** —— 这就是"持有子对象并转发一次调用"的那条链路
        let mut ad = GraphwarAdSwitch::new_alloc();
        ad.set_name(&StringName::from(HOLDER_NAMES[2]));
        self.base_mut().add_child(&ad.clone().upcast::<Node>());
        self.ad_holder = Some(ad);

        // ④ 管理器**节点槽**：一个空 Node 容器 + 按 `MODULES` 建的空 Node。
        //    ⚠️ 名字是原版类名，但节点是空的 —— **真装配在 Rust 容器里**（第 ⓪ 步），
        //    节点树这一层只是把 25 个槽画出来（`module_slot_status()` 才回答装没装）。
        let mut container = Node::new_alloc();
        container.set_name(&StringName::from("GraphwarModuleContainer"));
        for entry in MODULES {
            let mut slot = Node::new_alloc();
            slot.set_name(&StringName::from(entry.name));
            container.add_child(&slot.upcast::<Node>());
        }
        self.base_mut().add_child(&container.upcast::<Node>());

        self.built = true;
        self.child_slot_count()
    }

    /// Game 屏（`PlayScreen`）的内容：棋盘 + 实体绘制器 + 3 个假实体。
    ///
    /// 节点的**名字与父子关系**都是观测到的（`docs/16` §2.1.10 的运行时 dump）：
    ///
    /// ```text
    /// PlayScreen/.../GraphwarPlane            ← 有自己的名字
    /// GraphwarPlane/@EntityDrawer@N           ← **没有** set_name（引擎自动名）
    /// ```
    ///
    /// 3 个假实体走 [`EntityDrawer`] 的 `spawn_entity`（每个建 3 个 `Sprite2D` + 1 个
    /// `RichTextLabel`，`docs/16` §34）；它们的 id / 坐标 / 名字来自
    /// [`crate::_scaffold::root_wiring::DEMO_SOLDIERS`]（**我方选择**的假数据）。
    fn build_game_screen(&mut self, screen: &mut Gd<Control>) {
        let size = Vector2::new(VIEWPORT_SIZE.0, VIEWPORT_SIZE.1);
        screen.set_size(size);

        // 棋盘本体：`GraphwarPlane`（画一个棋盘框 + 3 个 `FunctionDrawer` 占位子节点）。
        let mut plane = GraphwarPlane::new_alloc();
        plane.set_name(&StringName::from(PLANE_NODE_NAME));
        plane.set_size(size);

        // 实体绘制器：原版是 `GraphwarPlane` 的**子节点**、**没** `set_name`
        // ⇒ 这里也不命名（引擎会给 `@EntityDrawer@N`，与 dump 里一致）。
        let mut drawer = EntityDrawer::new_alloc();
        drawer.set_size(size);
        // 3 个假士兵：`spawn_entity` / `set_entity_name` 是本类的 `#[func]`
        // （`entity_drawer.rs` 里是私有方法）⇒ 走动态调用，等价于 GDScript 那一侧。
        for soldier in DEMO_SOLDIERS {
            drawer.call(
                "spawn_entity",
                &[soldier.id.to_variant(), soldier.x.to_variant(), soldier.y.to_variant()],
            );
            drawer.call(
                "set_entity_name",
                &[soldier.id.to_variant(), GString::from(soldier.name).to_variant()],
            );
        }

        plane.add_child(&drawer.clone().upcast::<Node>());
        screen.add_child(&plane.clone().upcast::<Node>());

        self.plane = Some(plane);
        self.entity_drawer = Some(drawer);
    }

    /// 根节点当前有几个直接子节点（观测点）。
    fn child_slot_count(&self) -> i64 {
        self.base().get_child_count() as i64
    }

    // =======================================================================
    // T71 落码的两个原版方法（**关联函数**形态：本仓节点不持有 `+0xb0`/`+0x1705`
    // 这批原始字节，状态由 [`SaveLevelInputs`] / [`PermissionsResult`] 显式喂进来；
    // 偏移证据保留在 `save_level` / `permissions_result` 两个常量模块里）
    // =======================================================================

    /// `GraphwarRootNode::check_save_level` —— **x86 `0x3afcf0`**
    /// （FDE `0x3afcf0..0x3afe8d`，`0x19d` = 413 字节）。
    ///
    /// 逐步指令（3 个 panic 出口 + 1 个真调用）：
    /// ```text
    /// 3afcfc movabs r15,0x7fffffffffffffff
    /// 3afd06 lea    rax,[r15+1]              ; = 0x8000000000000000
    /// 3afd0a cmp    qword [rdi+0xb0], rax    ; 关卡标识哨兵？
    /// 3afd11 je     0x3afded                 ; ⇒ 直接返回
    /// 3afd1a cmp    byte [rdi+0x1705], 6     ; GAME_STATE == 6 ?
    /// 3afd21 jne    0x3afded                 ; ⇒ 直接返回
    /// 3afd27 mov    r14,[rbx+0x1668]         ; 对局控制器（Rc<RefCell<_>> 的 RcBox 指针）
    /// 3afd2e…3afd3f                          ; RefCell::borrow()（计数 +1，溢出 panic @0x3afdfa）
    /// 3afd43 cmp    qword [r14+0x1f8], 0     ; 载荷 +0x1e0
    /// 3afd4b je     0x3afde9                 ; ⇒ 撤销 borrow 后返回
    /// 3afd51 mov    r12,[r14+0x238]          ; 模块容器
    /// 3afd58…3afd69                          ; RefCell::borrow()（溢出 panic @0x3afe07）
    /// 3afd73 call   0x46be70                 ; get_module::<T>(&容器) → Option
    /// 3afd78 je     0x3afe16                 ; None ⇒ panic（Location = game_client.rs:152:22）
    /// 3afd95 movzx  ebp, byte [rax+0x137]    ; 该 manager 的 bool
    /// 3afdac dec    [r14+0x10]               ; 释放第一次 borrow
    /// 3afdb0 test   bpl,bpl / jne 0x3afded   ; 非 0 ⇒ 直接返回
    /// 3afdb5 mov    r14,[rbx+0x16d0]         ; 战役控制器
    /// 3afdc1 cmp    qword [r14+0x10],0 / jne ; 已借用 ⇒ panic @0x3afe34
    /// 3afdcf mov    qword [r14+0x10],-1      ; RefCell::borrow_mut
    /// 3afddb mov    rsi,rbx                  ; 第 2 实参 = &self+0xb0
    /// 3afdde call   0x4183b0                 ; **写存档**（campaign.save 的写者，全 .text 唯一调用点）
    /// 3afde3 inc    qword [r14+0x10]         ; 释放 borrow_mut
    /// 3afded … ret
    /// ```
    ///
    /// 档位：**观测**（控制流、每个字段偏移、唯一被调者都对到指令）。
    /// ⚠️ **推断**（缺什么写在 [`SaveLevelInputs`] 各字段上）：三个未知位的**语义**
    /// （`+0xb0` 的哨兵、`+0x1f8` 的闸门、`+0x137` 的 manager flag）。旧文档
    /// （`docs/16` §15.1）按调用点把它记成「进屏前的当前是否对局中判定」；本轮读下来
    /// 它的**效果**明确是「满足 5 个条件时把当前关卡进度写进 `campaign.save`」。
    pub fn check_save_level(inputs: &SaveLevelInputs) -> SaveLevelDecision {
        // 0x3afd0a / 0x3afd11
        if inputs.level_key_word == save_level::LEVEL_KEY_SENTINEL {
            return SaveLevelDecision::NotNow(SaveLevelSkip::LevelKeySentinel);
        }
        // 0x3afd1a / 0x3afd21
        if inputs.game_state != save_level::REQUIRED_GAME_STATE {
            return SaveLevelDecision::NotNow(SaveLevelSkip::GameStateNotRequired);
        }
        // 0x3afd43 / 0x3afd4b
        if !inputs.play_control_gate {
            return SaveLevelDecision::NotNow(SaveLevelSkip::PlayControlGate);
        }
        // 0x3afd73 → 0x3afd78：原版是 `.unwrap()`（None ⇒ panic）⇒ 本模型也不静默返回。
        let module_flag = inputs
            .module_flag
            .expect("原版 0x3afd78 在 get_module::<T>() 为 None 时 panic（Location: game_client.rs:152:22）");
        // 0x3afd95 / 0x3afdb0
        if module_flag {
            return SaveLevelDecision::NotNow(SaveLevelSkip::ModuleFlagSet);
        }
        // 0x3afdb5 / 0x3afdbf
        if !inputs.campaign_control {
            return SaveLevelDecision::NotNow(SaveLevelSkip::NoCampaignControl);
        }
        // 0x3afdde：call 0x4183b0(&mut 战役控制器载荷, &self+0xb0)
        SaveLevelDecision::SaveCurrentLevel
    }

    /// `GraphwarRootNode::request_permissions_result` —— **x86 `0x3b4ec0`**
    /// （FDE `0x3b4ec0..0x3b5288`，`0x3c8` = 968 字节）。
    ///
    /// 可观测的**三步效果**（顺序即指令顺序）：
    /// ```text
    /// 3b4ed4 mov [rsp+0x50], rsi             ; 第 2 实参（权限名）
    /// 3b4ed9 mov [rsp+6], dl                 ; 第 3 实参（bool）
    /// 3b4ee5 call *0x9ba768 → 0x74b510       ; String::new
    /// 3b4f10 call *0x9ba718 → 0x91ded0       ; ① format（字面量 0x1a0d63 那片）
    /// 3b4fad lea rsi,[0x1a95d0]              ; "Request permissions returned. "（len 0x1e）
    /// 3b4fc3 call *0x9ba718 → 0x91ded0       ; ② format
    /// 3b4ff2 mov r12,[r15+0x16b0]            ; 日志缓冲 Rc<RefCell<Vec<String>>>
    /// 3b5014 mov qword [r12+0x10],-1         ; borrow_mut（已借用 ⇒ panic @0x3b5138）
    /// 3b5079 cmp r15,[r12+0x18] / 0x3b5085   ; Vec 满了 ⇒ grow（0x8f9480 / 0x461100）
    /// 3b5093 lea rcx,[r15+r15*2]             ; 元素 24 字节
    /// 3b5097 mov [rax+rcx*8], rbx            ; 把 String 三个字段写进 Vec 尾
    /// 3b50a5 inc r15 / 3b50a8 mov [r12+0x28],r15   ; len += 1
    /// 3b50d4 mov byte [rax+0x1703], 0        ; ③ GATE_B = 0
    /// 3b50db mov rax,[0x9c18f0]              ; ④ 全局单例
    /// 3b50e2 movzx ecx,byte [rax+0x14f0]     ; 该单例的开关
    /// 3b50f1 call [rax+0x11e0]               ; 开则虚调一次（**未定名**，本模型不模拟）
    /// ```
    /// 档位：**观测**（②③④ 的效果 + 前缀字面量都指到指令）；
    /// ⚠️ **推断**：消息里前缀之后的**两个占位符**是什么（缺证据：`0x3b4f10` / `0x3b4fc3`
    /// 两次 `Arguments` 的占位符数组没展开 —— 那 2 个 `&str` 表在 `.data.rel.ro`）。
    /// 本实现按「权限名 + 是否授予」拼，**这一句是推断**，不写成结论。
    pub fn request_permissions_result(
        log: &mut Vec<String>,
        gate_b: &mut bool,
        result: &PermissionsResult,
    ) -> String {
        // ①②：0x3b4fc3 的格式串（前缀逐位）+ 推断的占位符
        let message = format!(
            "{}{} {}",
            permissions_result::MESSAGE_PREFIX, result.permission, result.granted
        );
        // ③：0x3b5097..0x3b50a8 —— push 进日志缓冲
        log.push(message.clone());
        // ④：0x3b50d4 —— GATE_B 清零
        *gate_b = false;
        // （原版 0x3b50db..0x3b50f6 的全局单例虚调不模拟：未定名 + 属引擎层）
        message
    }
}

#[godot_api]
impl INode for GraphwarRootNode {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            ads_slot: false,
            screens: BTreeMap::new(),
            ad_holder: None,
            wiring: None,
            setup_screen: None,
            plane: None,
            entity_drawer: None,
            built: false,
            deferred_log: Vec::new(),
            ready_calls: 0,
            process_calls: 0,
            visibility_writes: 0,
            logger: GraphwarLogger::new(0),
            last_queried: GString::new(),
        }
    }

    /// `_ready`（原版 `0x3a2e10`，**观测**：3 个 `get_node` + 14 次 `upcast_ref` + 一批 `connect`）。
    ///
    /// 本骨架只做那 3 个 `get_node` 对应的**节点形状**；`connect`（6 个信号 / 事件）**没做**
    /// —— `connect` 用的 GOT 槽 `0x9bb030` 至今未定名（`docs/14` §9.2）。
    fn ready(&mut self) {
        self.ready_calls += 1;
        self.ensure_scene_graph();
    }

    /// `_process`（原版是 `0x3a5760` 那 20288 字节状态机的一部分，**形状未解**）。
    ///
    /// 骨架里只计数：**不猜**状态机做了什么（`graphwar_root_node` 的未知清单第 8 条）。
    /// 不写 `set_process(true)`：Godot 4.7 自动开（`docs/16` §33.4 第 5 条，本轮再次实测）。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;
    }
}

#[godot_api]
impl GraphwarRootNode {
    /// 建出节点骨架（幂等），返回**直接子节点数**。
    #[func]
    fn build_scene_graph(&mut self) -> i64 {
        self.ensure_scene_graph()
    }

    /// 三个 holder 的名字（顺序同原版 `_ready`）。
    #[func]
    fn holder_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for name in HOLDER_NAMES {
            out.push(&GString::from(name));
        }
        out
    }

    /// 屏表里有多少个屏。
    #[func]
    fn screen_count(&self) -> i64 {
        self.screens.len() as i64
    }

    /// 屏名（按枚举序 = `docs/16` §15.2 的表序；原版是 `HashMap` 迭代序，**未知**）。
    #[func]
    fn screen_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for screen in self.screens.keys() {
            out.push(&GString::from(screen.name()));
        }
        out
    }

    /// **切屏**：跑一遍 `0x3af850` 的决策表（纯逻辑在
    /// [`crate::graphwar_root_node::apply_show_screen`]），把 `set_visible` 转发到真子节点。
    ///
    /// 返回真的写出了多少次 `set_visible`（`background` / 不在屏表里的键不算，也不写）。
    ///
    /// 原版这条路的两个单列分支（`PlayScreen` 进出口要动广告）由
    /// [`graphwar_root_node::ScreenSwap`] 决定，`call_deferred` 的方法名进
    /// [`GraphwarRootNode::deferred_log`]。
    #[func]
    fn show_screen(&mut self, name: GString) -> i64 {
        self.ensure_scene_graph();

        let queried = name.to_string();
        let entries: Vec<&'static str> = self.screens.keys().map(|s| s.name()).collect();
        let ads_slot = self.ads_slot;
        let mut writes = 0i64;

        // `deferred_log` 先取出来再放回去：`NodeScreenHost` 同时要 `&mut screens` 与
        // `&mut deferred`，而后者是 `self` 的一个字段 —— 不能同时从 `self` 借两次。
        let mut deferred = std::mem::take(&mut self.deferred_log);
        {
            let mut host = NodeScreenHost {
                screens: &mut self.screens,
                deferred: &mut deferred,
                writes: &mut writes,
            };
            apply_show_screen(&mut host, &entries, &queried, ads_slot);
        }
        self.deferred_log = deferred;

        self.visibility_writes += writes;
        self.last_queried = name;
        writes
    }

    /// 当前可见的屏名（按枚举序）。
    #[func]
    fn visible_screens(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for (screen, node) in self.screens.iter() {
            if node.is_visible() {
                out.push(&GString::from(screen.name()));
            }
        }
        out
    }

    /// 单独设某个屏的可见性（`screen_swap` 之外的旁路，方便验收脚本摆前置状态）。
    #[func]
    fn set_screen_visible(&mut self, name: GString, visible: bool) -> bool {
        self.ensure_scene_graph();
        let Ok(key) = Screen::from_name(&name.to_string()) else {
            return false;
        };
        let Some(node) = self.screens.get_mut(&key) else {
            return false;
        };
        node.set_visible(visible);
        true
    }

    /// `call_deferred` 的方法名流水（顺序 = 原版的先后：`call_deferred` 先于 `set_visible`）。
    #[func]
    fn deferred_log(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for method in self.deferred_log.iter() {
            out.push(method);
        }
        out
    }

    /// `GraphwarRootNode.log(String)` —— 原版 5 份平台桥 `.gd` 在**桌面分支**上调用的唯一一行。
    ///
    /// 档位：**观测**（方法存在 + 1 个参数）：注册块在 `0x3b7328`
    /// （`lea 0x1bdb95("log"), len=3` 与 `lea 0x19e490("GraphwarRootNode"), len=0x10` 同块，
    /// `mov $1,%r8d` + `push $1` ⇒ 1 个参数），ptrcall `0x3b6010` / call_func `0x3b6340`；
    /// 原版 `.gd` 侧共 **20 处** `root.log("…")`（5 份文件）。
    ///
    /// 方法体 = **推断**：落点是 [`GraphwarLogger::log`]（`GraphwarLogger` 的
    /// `Rc<RefCell<_>>` 在 `<GraphwarRootNode as INode>::init` 里建，`docs/16` §30.5）。
    /// 缺的证据：x86 侧没把 body 与 arm64 `0x45bc5c` 对上（只拿到注册回调）。
    /// 出口（原版 `tiny_http` 端点 + 报文格式）= **未知** ⇒ 这里**不猜**：不打印、不开端口。
    #[func]
    fn log(&mut self, message: GString) {
        // 原版 `GraphwarLogger::log` 收 `OsString`（arm64 `0x45bc5c` 的
        // `RawVec<OsString>::grow_one`）；Godot 侧一律 String，按"一行一条"落缓冲。
        self.logger.log(message.to_string());
    }

    /// `log()` 收到几行（观测点。出口未知 ⇒ 只能从缓冲读）。
    #[func]
    fn log_count(&self) -> i64 {
        self.logger.pending() as i64
    }

    /// 取走日志缓冲（`GraphwarLogger::process` 的"取走并清空"那一半语义）。
    #[func]
    fn log_take(&mut self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for line in self.logger.process() {
            out.push(&GString::from(line.as_str()));
        }
        out
    }

    /// `Ad_holder` 在不在（**推断**它是个 `GraphwarAdSwitch`）。
    #[func]
    fn ad_holder_present(&self) -> bool {
        self.ad_holder.is_some()
    }

    /// 转发给 `Ad_holder`：`are_ads_enabled()` = `!owned`。
    #[func]
    fn ads_enabled(&mut self) -> bool {
        self.ensure_scene_graph();
        match self.ad_holder.clone() {
            Some(holder) => holder.bind().are_ads_enabled(),
            // 没有 Ad_holder ⇒ 广告不可用（原版 `0x3af850` 一开头 `test rbx,rbx; je` 就返回）
            None => false,
        }
    }

    /// `GraphwarRootNode::purchase_confirmed()`：把 `Ad_holder` 的 `owned` 写成 `true`
    /// （arm64 `0x3a658c` `strb w10(=1),[x8,#0x18]`，**观测**）。
    ///
    /// 返回写完之后 `are_ads_enabled()` 的值（买断 ⇒ 关广告 ⇒ `false`）。
    #[func]
    fn purchase_confirmed(&mut self) -> bool {
        self.ensure_scene_graph();
        let Some(mut holder) = self.ad_holder.clone() else {
            return self.ads_enabled();
        };
        holder.bind_mut().write_owned(true);
        self.ads_enabled()
    }

    /// `GraphwarRootNode::purchase_denied()`：`owned = false`（arm64 `0x3a61b0`
    /// `strb wzr,[x8,#0x18]`，**观测**）。
    #[func]
    fn purchase_denied(&mut self) -> bool {
        self.ensure_scene_graph();
        let Some(mut holder) = self.ad_holder.clone() else {
            return self.ads_enabled();
        };
        holder.bind_mut().write_owned(false);
        self.ads_enabled()
    }

    /// 模块容器里有多少个**槽**（`graphwar_module_container::MODULES`，25 个模块类型）。
    #[func]
    fn manager_slot_count(&self) -> i64 {
        MODULES.len() as i64
    }

    /// 最大的一处注册点注册了多少个 manager（`GraphwarGameClient::new` = 22）。
    #[func]
    fn manager_registration_count(&self) -> i64 {
        MANAGER_REGISTRATION_COUNT as i64
    }

    // -----------------------------------------------------------------------
    // §42：容器真装配的观测口（全部走 `TypeId`，没有一处读 `+0xNN`）
    // -----------------------------------------------------------------------

    /// 容器装好了吗（`build_scene_graph` / `_ready` 会做）。
    #[func]
    fn wiring_done(&mut self) -> bool {
        self.ensure_scene_graph();
        self.wiring.is_some()
    }

    /// 容器里现在有几个模块（= `HashMap::len`，原版 `get_module` 的空表快路径就是它）。
    ///
    /// ⚠️ 这只是**一个数**：验收要按类型逐条断言，见
    /// [`GraphwarRootNode::container_has_module`]。
    #[func]
    fn container_module_count(&mut self) -> i64 {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.len() as i64,
            None => -1,
        }
    }

    /// **按类型**查在不在容器里（名字 → `TypeId` → `contains::<M>()`）。
    ///
    /// 名字是原版类型名（`graphwar_module_container::MODULES` 的原文）；未知名字一律 `false`。
    /// 这一条**不是**按数量判的：`container_has_module("GraphwarBanList")` 在装了 3 个之后
    /// 仍然是 `false`（那 22 型只有占位槽，容器里没有键）。
    #[func]
    fn container_has_module(&mut self, type_name: GString) -> bool {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.has_name(&type_name.to_string()),
            None => false,
        }
    }

    /// 按类型查**成立**的那几个名字，顺序 = **原版注册序**（`docs/16-client.md` §41.2 的
    /// 22 条：`GraphwarTickHandler` → … → `GraphwarRankManager`）。
    #[func]
    fn container_module_names(&mut self) -> PackedStringArray {
        self.ensure_scene_graph();
        let mut out = PackedStringArray::new();
        if let Some(w) = self.wiring.as_ref() {
            for name in w.registered_names() {
                out.push(&GString::from(name));
            }
        }
        out
    }

    /// 按类型查成立的个数（[`GraphwarRootNode::container_module_count`] 的**类型版**）。
    #[func]
    fn wired_manager_count(&mut self) -> i64 {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.registered().len() as i64,
            None => -1,
        }
    }

    /// **取回 `GameStateManager` 再读一次**（证明 `get_module` 拿到的是活对象，不是"表里有个键"）。
    ///
    /// 返回 `Match::soldiers.len()`；容器里没有这一型 ⇒ `-1`。
    #[func]
    fn container_game_state_soldiers(&mut self) -> i64 {
        self.ensure_scene_graph();
        let Some(w) = self.wiring.as_ref() else {
            return -1;
        };
        match w.game_state() {
            Some(game) => game.borrow().soldiers.len() as i64,
            None => -1,
        }
    }

    /// 取回 `TurnManager`，读某个玩家的士兵数（`player_id` 不存在 ⇒ `-1`）。
    ///
    /// 原版的 `TurnManager` 是 `player_id → {Vec<soldier_id>, 游标}`（`0x463f27`–`0x463fa6`
    /// 那张表，`docs/98` §1.1）；这个口子读的就是那张表的长度。
    #[func]
    fn container_turn_manager_soldiers(&mut self, player_id: i64) -> i64 {
        self.ensure_scene_graph();
        let Some(w) = self.wiring.as_ref() else {
            return -1;
        };
        let Some(turns) = w.turn_manager() else {
            return -1;
        };
        let turns = turns.borrow();
        match turns.player(player_id) {
            Some(entry) => entry.turns.len() as i64,
            None => -1,
        }
    }

    /// 取回 `PlayerManager`，读 `player_id → connection_id` 那张表的行数。
    #[func]
    fn container_player_manager_players(&mut self) -> i64 {
        self.ensure_scene_graph();
        let Some(w) = self.wiring.as_ref() else {
            return -1;
        };
        match w.player_manager() {
            Some(players) => players.borrow().len() as i64,
            None => -1,
        }
    }

    /// 某个槽位的**复原档位**（四档，按序判断、第一个命中者为准；`docs/16` §47.1）：
    /// `"wired"`（容器 + 事件流 + 最小交互断言）/ `"layout"`（原版字段布局逐字段解出）/
    /// `"assembled"`（容器里是我方复原类型）/ `"stub"`（我方形状，布局未知）。
    /// 另有 `"placeholder"`（原版有这一型，本层没装）/ `"unknown"`（原版 25 型里也没有这名字）。
    ///
    /// 判据是**运行时的**（[`crate::_scaffold::root_wiring::RootWiring::has`]），不是静态表：
    /// 装失败就掉回 `placeholder`。
    #[func]
    fn module_slot_status(&mut self, type_name: GString) -> GString {
        self.ensure_scene_graph();
        let name = type_name.to_string();
        if let Some(which) = Wired::from_type_name(&name) {
            let assembled = self.wiring.as_ref().map(|w| w.has(which)).unwrap_or(false);
            return GString::from(if assembled {
                which.status()
            } else {
                root_wiring::SLOT_PLACEHOLDER
            });
        }
        if root_wiring::is_module_type_name(&name) {
            GString::from(root_wiring::SLOT_PLACEHOLDER)
        } else {
            GString::from(root_wiring::SLOT_UNKNOWN)
        }
    }

    /// **按类型取回**（`get_module::<M>()` 真的 downcast 成功吗）。
    ///
    /// 比 [`GraphwarRootNode::container_has_module`] 强一档：那一支只查
    /// `HashMap::contains_key(&TypeId::of::<M>())`，这一支走完整的取回路径
    /// （`hash_one` → 探测 → `Any::type_id` 比对 → `Rc::downcast`，`docs/02` §22.5）。
    #[func]
    fn container_module_live(&mut self, type_name: GString) -> bool {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.retrievable_name(&type_name.to_string()),
            None => false,
        }
    }

    /// 22 个槽里"能取回活对象"的个数（[`GraphwarRootNode::container_module_live`] 的计数版）。
    #[func]
    fn container_live_count(&mut self) -> i64 {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => Wired::ALL.into_iter().filter(|x| w.retrievable(*x)).count() as i64,
            None => -1,
        }
    }

    /// 22 个槽里"我方有真复原类型"的个数（`docs/16` §41.2 的 17）。
    #[func]
    fn container_real_manager_count(&mut self) -> i64 {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.real_managers().len() as i64,
            None => -1,
        }
    }

    /// 22 个槽里**容器类型来源为 `SlotKind::Stub`** 的个数（= **5**，`docs/16` §41.2 那批）。
    ///
    /// ⚠️ 这**不是**档位 `stub` 的个数（那个现在是 **1**，只剩 `GraphwarPlanePosManager`）：
    /// 本函数数的是"装配表里标为占位"的槽，档位则是"按证据判到第几档"。两者口径不同，
    /// 2026-11-08 任务 28 起在 `assemble_check.gd` 里已分别断言（四档计数 + `SlotKind` 计数）。
    /// 函数名保留是为了不破坏既有断言；改名留给下一轮与 gd 断言同批做。
    #[func]
    fn container_stub_manager_count(&mut self) -> i64 {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => w.stub_managers().len() as i64,
            None => -1,
        }
    }

    /// 这一槽的原版 `size_of::<M>()`（`docs/02` §22.3；**两架构相同**，vtable size 字 − 8）。
    ///
    /// 22 个数按注册序连起来就是 §41.2 那串
    /// `80,120,72,80,80,32,304,40,80,40,80,88,40,80,40,176,80,80,56,80,128,80`（+8 之后）。
    /// 未知名字 ⇒ `-1`。
    #[func]
    fn manager_slot_size(&mut self, type_name: GString) -> i64 {
        match Wired::from_type_name(&type_name.to_string()) {
            Some(w) => w.slot().size as i64,
            None => -1,
        }
    }

    /// 这一槽装的是**我方的哪个类型**（`graphwar_turn_manager::TurnManager` 之类）；
    /// 空占位回 `"—"`；未知名字回空串。
    #[func]
    fn manager_impl_type(&mut self, type_name: GString) -> GString {
        match Wired::from_type_name(&type_name.to_string()) {
            Some(w) => GString::from(w.slot().our_type),
            None => GString::new(),
        }
    }

    /// 本层装了几个 manager（= `Wired::COUNT`，**22** = 原版 `GraphwarGameClient::new`
    /// 的 `add_module` 条数；其中 17 个真类型 + 5 个空占位）。
    #[func]
    fn assembled_manager_total(&self) -> i64 {
        Wired::COUNT as i64
    }

    /// `_ready` 被引擎调到几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `_process` 被引擎调到几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// 累计 `set_visible` 写成功多少次。
    #[func]
    fn visibility_write_count(&self) -> i64 {
        self.visibility_writes
    }

    /// 最近一次 `show_screen` 的屏名。
    #[func]
    fn last_queried_screen(&self) -> GString {
        self.last_queried.clone()
    }

    // ------------------------------------------------------------------
    // ⚠️ 下面两个是**我方自造的观测口**（2026-11 任务 19），原版**没有**这两个方法。
    //
    // 为什么必须加：`RootWiring` 是 Rust 结构体，GDScript 只能调 `#[func]`；
    // 在它之前，headless 侧**没有任何办法**触发一次 manager 交互（只能"数个数"）。
    // 它们只做一件事：把 `RootWiring::interaction_selftest`（5 个 manager 各一次
    // 最小交互，写 → 读回）**原样**交给验收脚本，**自己不返回任何常量**。
    //
    // 口径（`docs/16-client.md` §43）：
    // * **只读**：不改变 22 个槽的注册、不碰 `Match` / `TurnManager` / `PlayerManager`
    //   （那三个是别的断言在读的）；动的只有这 5 个 manager 自己表里的固定 id
    //   （`9001..=9003`）与固定坐标，且每次调用末尾把发件箱取走；
    // * **幂等**：连调两次返回**同一串** `名字=值`（验收脚本就是这么比的）；
    // * **不是复原成果**：它证明的是"装配好的对象能交互"，不证明原版行为。
    // ------------------------------------------------------------------

    /// 5 个 manager 的"一次最小交互"结果，每条一个 `"名字=值"`（27 条）。
    ///
    /// 名字与期望值见 `docs/16-client.md` §43 的表；
    /// Rust 侧的同一条断言是 `root_wiring.rs::tests::
    /// one_minimal_interaction_per_manager_reads_back_the_written_value`。
    #[func]
    fn manager_probe_selftest(&mut self) -> PackedStringArray {
        self.ensure_scene_graph();
        match self.wiring.as_ref() {
            Some(w) => {
                let mut out = PackedStringArray::new();
                for line in w.interaction_selftest_strings() {
                    out.push(&GString::from(line.as_str()));
                }
                out
            }
            None => PackedStringArray::new(),
        }
    }

    /// 单条查询：[`GraphwarRootNode::manager_probe_selftest`] 里那个 `"名字=值"` 的**值**。
    ///
    /// 未知名字 ⇒ 空串（不是 `"0"`，也不是 panic）。
    #[func]
    fn manager_probe(&mut self, name: GString) -> GString {
        self.ensure_scene_graph();
        let wanted = name.to_string();
        match self.wiring.as_ref() {
            Some(w) => match w
                .interaction_selftest()
                .into_iter()
                .find(|(k, _)| *k == wanted.as_str())
            {
                Some((_, v)) => GString::from(v.as_str()),
                None => GString::new(),
            },
            None => GString::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphwar_root_node::{screen_swap, ScreenSwap, SPECIAL_MAP_KEYS};

    /// 三个 holder 名字就是 `READY_NODES`（不是重新编的）。
    #[test]
    fn holder_names_are_the_ready_nodes() {
        assert_eq!(HOLDER_NAMES.len(), 3);
        assert_eq!(HOLDER_NAMES, ["ScreensControl", "IAP_holder", "Ad_holder"]);
        for (name, node) in HOLDER_NAMES.iter().zip(READY_NODES.iter()) {
            assert_eq!(*name, node.name);
            assert_eq!(name.len(), node.len as usize);
            // holder 不是屏
            assert!(Screen::from_name(name).is_err());
        }
    }

    /// 屏表条目数 = 15，且每个名字都能往返。
    #[test]
    fn screen_table_is_fifteen_round_trip_names() {
        assert_eq!(ALL_SCREENS.len(), 15);
        for s in ALL_SCREENS {
            assert_eq!(Screen::from_name(s.name()), Ok(s));
        }
    }

    /// 切屏决策表：本类转发过去的就是这条式子（用模块的纯函数对一遍）。
    #[test]
    fn forwarding_agrees_with_the_module_decision_table() {
        // 命中非 PlayScreen ⇒ Show（可见性 true）
        assert_eq!(screen_swap("Campaign", "Campaign", false, false), ScreenSwap::Show);
        assert_eq!(
            screen_swap("Campaign", "Campaign", false, false).visible(),
            Some(true)
        );
        // PlayScreen 命中 + ads_slot + 可见 ⇒ 先 destroy_ad_view 再显示
        let hit = screen_swap("PlayScreen", "PlayScreen", true, true);
        assert_eq!(hit, ScreenSwap::ShowAndDestroyAdView);
        assert_eq!(hit.deferred_method(), Some("destroy_ad_view"));
        // PlayScreen 未命中 + ads_slot + 可见 ⇒ 先 create_ad_view 再隐藏
        let miss = screen_swap("PlayScreen", "Campaign", true, true);
        assert_eq!(miss, ScreenSwap::HideAndCreateAdView);
        assert_eq!(miss.deferred_method(), Some("create_ad_view"));
        // background 未命中 ⇒ 不碰
        assert_eq!(screen_swap(SPECIAL_MAP_KEYS[0], "Campaign", true, true), ScreenSwap::Leave);
        assert_eq!(ScreenSwap::Leave.visible(), None);
    }

    /// 管理器计数：25 个槽、22 个注册点（两个数不是一回事）。
    #[test]
    fn manager_counts_are_two_different_numbers() {
        assert_eq!(MODULES.len(), 25);
        assert_eq!(MANAGER_REGISTRATION_COUNT, 22);
        assert!(MANAGER_REGISTRATION_COUNT <= MODULES.len());
        // 表里的名字都是原版类名（首字母大写、带 Graphwar 前缀的占多数）
        let mut names: Vec<&str> = MODULES.iter().map(|m| m.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), MODULES.len(), "模块名不许重复（节点名要唯一）");
        assert!(names.iter().all(|n| !n.is_empty()));
    }

    /// `Ad_holder` 的目标类名与模块常量一致。
    #[test]
    fn ad_holder_class_is_the_module_class() {
        assert_eq!(AD_HOLDER_CLASS, crate::graphwar_ad_switch::CLASS);
        assert_eq!(AD_HOLDER_CLASS, "GraphwarAdSwitch");
    }
}
}

#[cfg(feature = "gdext-bindings")]
pub use node_class::*;
