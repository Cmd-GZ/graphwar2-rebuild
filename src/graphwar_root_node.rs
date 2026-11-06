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
