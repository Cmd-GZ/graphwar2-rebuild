//! **不是原版的东西**：把**已经是复原实现**的「屏表 + 切屏决策 + 命令键」接给 GDScript。
//!
//! # 为什么会有这一份（T67 / `docs/06` §115 / `docs/06` §90 C 档第 3 段）
//!
//! `client/src/app/app.gd`（改前 958 行）是客户端根节点，对应原版 `GraphwarRootNode`。
//! 它里面有三样东西在 Rust 侧**早就复原了**，`.gd` 里又写了一遍：
//!
//! | `.gd`（改前） | 真值 | 档位 |
//! |---|---|---|
//! | `BASE_SCREENS` 7 个屏名 | `graphwar_root_node::Screen::name()`（`.rodata` 15 个名字，逐个带地址与长度立即数） | 逐位 |
//! | `show_screen()` 的可见性循环 | `apply_show_screen` = 原版 `FN_SHOW_SCREEN 0x3af850` 的循环体 + `screen_swap` 决策表（`PlayScreen` 进出动广告、`background` 完全不动） | 观测（控制流逐条对上） |
//! | `command()` 认的命令键 | `enum Command` 的 12 条（`FN_COMMAND_BUS 0x3b0830` 的两次窗口比对） | 观测/逐位 |
//!
//! # 原版里没有这个类
//!
//! 原版 `libgraphwarrust.so` 里没有 `GwRootNode`：屏表与命令总线是
//! `GraphwarRootNode` 的**字段/方法**（`[+0x1704]` 存当前屏、`0x3b0830` 是命令分发）。我们的
//! 客户端把这层写成 GDScript 仿制品（`AGENTS.md` §0.4），所以这里只做**取值/决策出口**。
//!
//! # 三件**必须写清**的事
//!
//! 1. **基础屏 vs 弹窗的划分是我方的**：原版 `0x3af850` 对 15 个屏**一视同仁**
//!    （命中 ⇒ `set_visible(true)`，其余 ⇒ `set_visible(false)`，`background` 除外）。
//!    我们的客户端需要"基础屏 + 弹窗叠层"两套结构，所以这里把 7 个基础屏单独列出来 ——
//!    **划分依据是名字形状，不是原版数据**（推断），见 [`BASE_SCREENS`]。
//! 2. **屏表的成员对不上**（`docs/06` §115.6 挖到的语义错）：原版 15 槽里
//!    `ConfirmPlay`(0x1bdb0d, 11 B) 与 `ExitPopUp`(0x1bdb18, 9 B) 是 `.rodata` 里**紧邻的两项**
//!    （合起来正好是 `"ConfirmPlayExitPopUp"` 20 B，即 `0x3b3783` 那个长度立即数 `0x14`），
//!    而客户端把它们当**一个**屏 `ConfirmPlayExitPopUp`；客户端还多一个 `ChatPopUp`，
//!    原版 15 槽里**没有**它（全库也没有对应 `.rodata` 名字）。本轮不改屏表（要动 `.tscn`
//!    节点名 ⇒ 像素风险），只把两处不一致登记出来。
//! 3. **命令键只有 12 条是有证据的**：`0x3b0830` 是 131 KB 的函数，还有 8 / 0x14 / 0xf / 6 / 3
//!    等长度分支没过完（`graphwar_root_node` 模块文档的未知清单第 2 条）⇒
//!    [`GwRootNode::command_kind`] 对不认识的键返回空串，**不声称命令全集**。
//!
//! # 接口
//!
//! ```gdscript
//! GwRootNode.base_screen_names()     # 7 个基础屏（原版 .rodata 名字）
//! GwRootNode.all_screen_names()      # 15 个（顺序 = Screen 声明序 = docs/16 §15.2 表序）
//! GwRootNode.screen_table()          # 15 行 {name, addr, len, len_site}（证据复核用）
//! GwRootNode.show_plan(q, ads, ps)   # 对 7 个基础屏的 {name, visible(-1/0/1), deferred}
//! GwRootNode.command_kind(cmd)       # 12 条已知键 → 规范名；否则 ""
//! GwRootNode.command_keys()          # 12 条已知键
//! ```

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_root_node as root;

/// 屏表 / 切屏决策 / 命令键的出口（scaffold；原版没有这个类，见模块文档）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwRootNode {
    base: Base<RefCounted>,
}

/// 7 个"基础屏"：同一时刻只有一个可见（客户端侧结构）。
///
/// ⚠️ **划分是推断**：原版 15 个屏在 `0x3af850` 里一视同仁，没有"基础/弹窗"之分
/// （`background` 那个例外键的归属仍 **未知**，见 `graphwar_root_node` 模块文档）。
/// 这 7 个是"名字不以 `PopUp` 结尾、且不是 `ConfirmPlay`/`ExitPopUp`"的那些 ——
/// **名字本身**是逐位的（`.rodata`），**分组**是我方的。
const BASE_SCREENS: [root::Screen; 7] = [
    root::Screen::MainMenu,
    root::Screen::Campaign,
    root::Screen::LanLobby,
    root::Screen::GameSetup,
    root::Screen::PlayScreen,
    root::Screen::ReplayLobby,
    root::Screen::PlayerScreen,
];

/// `ScreenSwap` → `visible` 的三档编码（`-1` = 不碰 / `0` = 隐藏 / `1` = 显示）。
fn visible_code(swap: root::ScreenSwap) -> i64 {
    match swap.visible() {
        None => -1,
        Some(false) => 0,
        Some(true) => 1,
    }
}

/// 一整张屏表的 `show_plan`：每项 `{name, visible, deferred}`。
fn plan_for(entries: &[&str], queried: &str, ads_slot: bool, ps_visible: bool) -> VarArray {
    let mut out = VarArray::new();
    for entry in entries {
        // 原版只对 `PlayScreen` 这一个条目取 `is_visible()`（`call *0x9baa20` 两个分支各一次）。
        let needs_ps = *entry == root::SPECIAL_MAP_KEYS[1];
        let ps = needs_ps && ps_visible;
        let swap = root::screen_swap(entry, queried, ads_slot, ps);
        let mut d = VarDictionary::new();
        d.set("name", &GString::from(*entry));
        d.set("visible", visible_code(swap));
        d.set(
            "deferred",
            &GString::from(swap.deferred_method().unwrap_or("")),
        );
        out.push(&d);
    }
    out
}

#[godot_api]
impl IRefCounted for GwRootNode {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl GwRootNode {
    /// 7 个基础屏的名字（顺序 = [`BASE_SCREENS`]，取值 = 原版 `.rodata` 原文）。
    ///
    /// `.gd` 的 `App.BASE_SCREENS` 从这里取 ⇒ 屏名不再有第二份。分组理由见 [`BASE_SCREENS`]。
    #[func]
    fn base_screen_names() -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for s in BASE_SCREENS {
            out.push(&GString::from(s.name()));
        }
        out
    }

    /// 全部 15 个屏名（顺序 = `Screen` 声明序 = `docs/16` §15.2 表序 = `ALL_SCREENS`）。
    ///
    /// ⚠️ 与客户端屏表的**两处不一致**见模块文档 ②：`ConfirmPlay`/`ExitPopUp` 两个槽
    /// 对应客户端的一个 `ConfirmPlayExitPopUp`；客户端多一个 `ChatPopUp`（原版无此名）。
    #[func]
    fn all_screen_names() -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for s in root::ALL_SCREENS {
            out.push(&GString::from(s.name()));
        }
        out
    }

    /// 15 行证据表：`{name, addr, len, len_site}`（`.rodata` 地址 / 字节长度 / 长度立即数所在指令）。
    ///
    /// `len_site = -1` ⇒ 本次没量到那条 `mov $len,%edx`（**不**表示长度未知：
    /// 长度就是名字字节数，已逐位钉住）。见 `Screen::len_immediate_site` 的文档。
    #[func]
    fn screen_table() -> VarArray {
        let mut out = VarArray::new();
        for s in root::ALL_SCREENS {
            let mut d = VarDictionary::new();
            d.set("name", &GString::from(s.name()));
            d.set("addr", s.addr() as i64);
            d.set("len", s.len() as i64);
            d.set("len_site", s.len_immediate_site().map(|v| v as i64).unwrap_or(-1));
            out.push(&d);
        }
        out
    }

    /// 屏名 → `.rodata` 地址（不在表里 ⇒ `-1`）。
    #[func]
    fn screen_addr(name: GString) -> i64 {
        match root::Screen::from_name(name.to_string().as_str()) {
            Ok(s) => s.addr() as i64,
            Err(_) => -1,
        }
    }

    /// **切屏决策**（原版 `FN_SHOW_SCREEN 0x3af850` 的循环体，逐项决策）。
    ///
    /// 对 **7 个基础屏**逐项给出 `{name, visible(-1 不碰 / 0 隐藏 / 1 显示), deferred}`；
    /// `.gd` 只负责把它落到节点上（`set_visible` / `call_deferred`）。
    ///
    /// * `queried` —— 想显示的那个屏名；
    /// * `ads_slot` —— 原版第 4 实参（**推断** = "这次切屏允许动广告"）。我方**没有**
    ///   `AdHolder` 节点，所以恒传 `false`；传 `true` 时 `PlayScreen` 那条会走
    ///   `create_ad_view` / `destroy_ad_view`（`deferred` 非空），与原版一致 ——
    ///   这样自检可以两条臂都覆盖。
    /// * `play_screen_visible` —— `PlayScreen` 节点当前是否可见（只在那个条目上被读）。
    #[func]
    fn show_plan(queried: GString, ads_slot: bool, play_screen_visible: bool) -> VarArray {
        let names: Vec<&str> = BASE_SCREENS.iter().map(|s| s.name()).collect();
        plan_for(&names, queried.to_string().as_str(), ads_slot, play_screen_visible)
    }

    /// 同 [`GwRootNode::show_plan`]，但条目由调用方给（自检用它把 **15 个屏**全过一遍，
    /// 覆盖 `background` 那条 `Leave` 臂与 `PlayScreen` 的两条广告臂）。
    #[func]
    fn show_plan_for(
        entries: PackedStringArray,
        queried: GString,
        ads_slot: bool,
        play_screen_visible: bool,
    ) -> VarArray {
        let owned: Vec<String> = entries.to_vec().iter().map(|s| s.to_string()).collect();
        let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        plan_for(&refs, queried.to_string().as_str(), ads_slot, play_screen_visible)
    }

    /// 命令键的**规范名**（原版 `0x3b0830` 已解的 12 条）；不认识的键返回 `""`。
    ///
    /// ⚠️ 12 条**不是全集**（模块文档未知清单第 2 条），所以空串的含义是
    /// "不在有证据的那 12 条里"，**不是**"原版不认识这个键"。
    #[func]
    fn command_kind(cmd: GString) -> GString {
        match root::Command::from_name(cmd.to_string().as_str()) {
            Some(c) => GString::from(c.as_str()),
            None => GString::from(""),
        }
    }

    /// 12 条已知命令键（`docs/16` §15.6；逐位由两次窗口比对还原）。
    #[func]
    fn command_keys() -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for c in KNOWN_COMMANDS {
            out.push(&GString::from(c.as_str()));
        }
        out
    }

    /// 一条命令键的证据：`{key, len, first_window, second_window, second_offset}`。
    ///
    /// 不在 12 条里 ⇒ `{}`。给自检/文档复核用（界面不读）。
    #[func]
    fn command_evidence(cmd: GString) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(c) = root::Command::from_name(cmd.to_string().as_str()) else {
            return d;
        };
        let ev = c.evidence();
        d.set("key", &GString::from(c.as_str()));
        d.set("len", c.len() as i64);
        d.set("first_window", ev.first_window as i64);
        d.set("second_window", ev.second_window as i64);
        d.set("second_offset", ev.second_offset as i64);
        d
    }

    /// 出处自检：函数地址、屏表规模、划分依据、未解部分。
    #[func]
    fn provenance() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("module", &GString::from("graphwar_root_node"));
        d.set("fn_show_screen", &GString::from("0x3af850（ELF vaddr）"));
        d.set("fn_command_bus", &GString::from("0x3b0830（ELF vaddr）"));
        d.set("fn_state_machine", &GString::from("0x3a5760（ELF vaddr）"));
        d.set("screen_count", root::ALL_SCREENS.len() as i64);
        d.set("base_screen_count", BASE_SCREENS.len() as i64);
        d.set(
            "base_screen_partition",
            &GString::from(
                "推断（我方结构）：原版 0x3af850 对 15 屏一视同仁；名字本身逐位",
            ),
        );
        d.set(
            "command_key_count",
            &GString::from("12（0x3b0830 已解；不是全集，见模块文档未知清单第 2 条）"),
        );
        d.set(
            "known_mismatch",
            &GString::from(
                "① 原版 ConfirmPlay(0x1bdb0d,11)+ExitPopUp(0x1bdb18,9) 两项 vs 客户端一个 \
                 ConfirmPlayExitPopUp；② 客户端 ChatPopUp 在原版 15 槽里没有对应名字",
            ),
        );
        d.set(
            "special_keys",
            &GString::from("background / PlayScreen（0x3af965 / 0x3af98a 立即数）"),
        );
        d.set(
            "ads_methods",
            &GString::from("create_ad_view 0x3af9fe / destroy_ad_view 0x3afab5"),
        );
        d.set("evidence_grade", &GString::from("逐位（屏名/长度/命令键立即数）+ 观测（切屏决策表）"));
        d
    }
}

/// `enum Command` 的 12 条（顺序 = 声明序；`command_keys()` 用）。
const KNOWN_COMMANDS: [root::Command; 12] = [
    root::Command::PlayerSettings,
    root::Command::StartLevel,
    root::Command::Multiplayer,
    root::Command::CreateGame,
    root::Command::ToggleMute,
    root::Command::ExitCampaign,
    root::Command::AskExitPlay,
    root::Command::CancelAddBot,
    root::Command::ExitLanLobby,
    root::Command::CancelJoinIp,
    root::Command::AddBot,
    root::Command::JoinIp,
];
