//! **不是原版的东西**：把**已经是复原实现**的战役关卡表接给 GDScript。
//!
//! # 为什么会有这一份（T67 / `docs/06` §118 / `docs/06` §90 C 档第 10 段）
//!
//! `client/src/app/campaign.gd`（改前 157 行）里有一张 **21 条全参数表** `const LEVELS`：
//! 标题 / 描述 / 四分模式字节 / 士兵数 / 靶子数 / Bot 皮肤号 / 回合上限 / 障碍模式 / `min_count`。
//! 那是 `src/graphwar_campaign_control.rs::LEVEL_INFO`（= 原版 `level_info()` `0x4ad380`）
//! 的**第二份拷贝**：改内核不改 `.gd`，界面就显示旧数据（两份只靠一条 `cargo test` 拉着）。
//!
//! 本类只做**取值出口**：谁要关卡数据就经过 Rust，`.gd` 里不再有第二份表。
//!
//! # 原版里没有这个类
//!
//! 原版 `libgraphwarrust.so` 里没有 `GwCampaignControl`：`level_info()` 是
//! `graphwar_campaign_control` 模块里的函数，原版 `Campaign` 屏的控制器（同为 Rust）
//! 直接在自己的 crate 里调它，**不需要**跨语言出口。我们的客户端是 GDScript 仿制品
//! （`AGENTS.md` §0.4），所以这一层是**脚手架**，只为把"数据从哪来"这条线接回 Rust。
//!
//! # 数据来源（每个字段的档位见 `graphwar_campaign_control.rs` 的 `LEVEL_INFO` 文档）
//!
//! | 字段 | 内核取值 | 档位 |
//! |---|---|---|
//! | `title` | `CampaignLevel::title`（`.rodata` 21 段，`Level 0`..`Level 20`） | 逐位 |
//! | `description` | `description`（`.rodata` 21 段原文，含原版拼写 `writting`） | 逐位 |
//! | `axis`/`function`/`turn`/`time` | `LevelModes::{axis,function,turn,time}()`（21 条 `mov dword [rsp+0x58], imm32` 的小端四字节） | 逐位（`time` 观测） |
//! | `soldiers` | `0x476320(mgr, player, N)` 的 `edx` 立即数 | 逐位 + 运行时 |
//! | `targets` | `0x46f310(team)` 的调用次数 | 逐位 + 运行时 |
//! | `bot_tier` | 运行时 `SkinGraphicsInfo` 的 `bot_N` 皮肤号（`R` 列） | 运行时 |
//! | `turn_limit` | `turn_limit_info`(tag 57) 载荷的 `N`；`None` ⇒ 本类给 **0**（= 无上限） | 逐位 + 运行时 |
//! | `obstacles` | `enum ObstacleMode` 的判别值（`None=0` / `Center=1` / `Scatter=2`） | 观测 |
//! | `obstacle_min` | 散布式的个数下界 `min_count` | 逐位 |
//!
//! ⚠️ 两处**客户端不该有**的东西留在 `.gd`（台账 C06/C08）：
//! * `level_seed()` —— 原版种子由运行时 ChaCha 抽（`docs/102` §1），**自造**的确定性值；
//! * `layout_buttons()` —— 原版**视图**算法，真值在 `recovered-src/CampaingLevelsButtons.gd`。
//!
//! # 接口
//!
//! ```gdscript
//! GwCampaignControl.level_count()      # 21
//! GwCampaignControl.level(i)           # 一关的 12 字段字典（越界 ⇒ {}）
//! GwCampaignControl.levels()           # 21 条
//! GwCampaignControl.obstacle_modes()   # ["None","Center","Scatter"]（索引 = 判别值）
//! GwCampaignControl.provenance()       # 出处/证据/函数地址（自检与文档复核用）
//! ```

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_campaign_control as cc;

/// 21 关参数表的**只读取值口**（scaffold；原版没有这个类）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwCampaignControl {
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for GwCampaignControl {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

/// 一关 → GDScript `Dictionary`。
///
/// 键名与改前 `campaign.gd` 的 `LEVELS` 行**逐字一致**（调用方零改动）：
/// `title / description / axis / function / turn / time / soldiers / targets /
///  bot_tier / turn_limit / obstacles / obstacle_min`。
fn level_dict(l: &cc::CampaignLevel) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set("title", &GString::from(l.title));
    d.set("description", &GString::from(l.description));
    d.set("axis", l.modes.axis() as i64);
    d.set("function", l.modes.function() as i64);
    d.set("turn", l.modes.turn() as i64);
    d.set("time", l.modes.time() as i64);
    d.set("soldiers", l.soldiers as i64);
    d.set("targets", l.targets as i64);
    // 改前 `.gd` 用 `-1` 表示"该关没有对手"；内核是 `Option<BotOpponent>`。
    d.set("bot_tier", l.bot.map(|b| b.tier as i64).unwrap_or(-1));
    // 改前 `.gd` 用 `0` 表示"无回合上限"；内核是 `Option<u32>`。
    d.set("turn_limit", l.turn_limit.map(|n| n as i64).unwrap_or(0));
    d.set("obstacles", obstacle_code(l.obstacles));
    d.set("obstacle_min", l.min_count as i64);
    d
}

/// `ObstacleMode` 的判别值（**没有** `#[repr]`，所以用 `match` 显式给，不靠 `as`）。
///
/// 与改前 `campaign.gd` 的 `enum Obstacles { NONE, CENTER, SCATTER }` 编码相同
/// （0/1/2）—— 所以本行是"把编码钉到 Rust 的判别值上"，**不改变** `.gd` 的语义。
/// `match` 是穷尽式的：内核将来加第 4 种模式时这里**编译不过**，不会静默漂移。
fn obstacle_code(m: cc::ObstacleMode) -> i64 {
    match m {
        cc::ObstacleMode::None => 0,
        cc::ObstacleMode::Center => 1,
        cc::ObstacleMode::Scatter => 2,
    }
}

/// 判别值 → 名字（`obstacle_modes()` 的顺序依据；同样穷尽）。
fn obstacle_name(m: cc::ObstacleMode) -> &'static str {
    match m {
        cc::ObstacleMode::None => "None",
        cc::ObstacleMode::Center => "Center",
        cc::ObstacleMode::Scatter => "Scatter",
    }
}

#[godot_api]
impl GwCampaignControl {
    /// 关卡数：`LEVEL_INFO` 的**类型长度**（`[CampaignLevel; 21]`），不是另写一个 21。
    ///
    /// 改前 `campaign.gd:30` 是 `const LEVEL_COUNT := 21` —— 与内核的 21 是**两份**，
    /// 内核真的加了 22 关时 `.gd` 不会跟着变。
    #[func]
    fn level_count() -> i64 {
        cc::LEVEL_INFO.len() as i64
    }

    /// 第 `index` 关（0 基，`0..=20`）。越界返回 `{}`（调用方 `GwCampaign.level()` 会夹取）。
    #[func]
    fn level(index: i64) -> VarDictionary {
        let Ok(i) = usize::try_from(index) else {
            return VarDictionary::new();
        };
        match cc::level_info(i) {
            Some(l) => level_dict(l),
            None => VarDictionary::new(),
        }
    }

    /// 21 关全表（顺序 = `level_0` … `level_20`）。
    #[func]
    fn levels() -> VarArray {
        let mut out = VarArray::new();
        for l in cc::LEVEL_INFO.iter() {
            out.push(&level_dict(l));
        }
        out
    }

    /// 解析 `level_<n>` 命令键（原版 `0x417ef0` / 命令分派 `0x499740` 的 21 条比较）。
    ///
    /// 语义**逐位**（`docs/archive/118-campaign-data.md` §3.1.2）：`rsplit('_')` 取最后一段
    /// 再 `parse::<u32>()` —— **不宽容**：`"level_20\n"` 的最后一段是 `"20\n"` ⇒ 失败。
    /// 失败返回 `-1`（原版在 `get_current_level` 里三次 `unwrap` ⇒ **会 panic**；
    /// 这里不 panic，把"原版会炸"留在文档里）。
    ///
    /// 改前 `app.gd` 用 `int(cmd.substr(len))` —— **宽容**解析：`"level_x"` 会得到 `0`
    /// ⇒ 静默选中第 0 关，而真值那一步是解析失败。这是一处**语义错**（`docs/06` §118.4）。
    #[func]
    fn parse_level_key(text: GString) -> i64 {
        match cc::parse_level_key(text.to_string().as_str()) {
            Some(n) => n as i64,
            None => -1,
        }
    }

    /// `enum ObstacleMode` 的名字表，**索引 = 判别值**（`obstacle_modes()[row.obstacles]`）。
    ///
    /// 名字取自原版 `.data.rel.ro` 那一族闭包记录的行为分类（`docs/archive/118-campaign-data.md`
    /// §12.2：A 侧 `+0x20` 只有 6 种本体、3 种行为）——**观测**。
    #[func]
    fn obstacle_modes() -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for m in [
            cc::ObstacleMode::None,
            cc::ObstacleMode::Center,
            cc::ObstacleMode::Scatter,
        ] {
            out.push(&GString::from(obstacle_name(m)));
        }
        out
    }

    /// 出处自检：函数地址、关数、密码学级证据串、每关的 `description` 字节长度。
    ///
    /// 只给**自检/文档复核**用（界面不读）。`description` 长度是内核的**逐位**自校验
    /// （`CampaignLevel::description_len()` 必须等于原版 `mov qword [rsp+0x38], <len>`
    /// 的立即数，21/21）。
    #[func]
    fn provenance() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("level_count", cc::LEVEL_INFO.len() as i64);
        d.set("kernel_const", &GString::from("graphwar_campaign_control::LEVEL_INFO"));
        d.set("kernel_func", &GString::from("graphwar_campaign_control::level_info"));
        d.set(
            "kernel_func_vaddr",
            &GString::from("0x4ad380（ELF vaddr；Ghidra/REA = 0x5ad380）"),
        );
        d.set(
            "evidence",
            &GString::from(
                "描述/标题 = .rodata 21 段逐字节；四模式 = 21 条 mov dword [rsp+0x58],imm32；\
                 士兵/靶子/回合上限 = 立即数 + 385 份 replay 运行时；bot_tier = SkinGraphicsInfo bot_N",
            ),
        );
        let mut lens = VarArray::new();
        for l in cc::LEVEL_INFO.iter() {
            lens.push((l.description_len() as i64));
        }
        d.set("description_lens", &lens);
        d.set("evidence_grade", &GString::from("逐位（+ 运行时 21/21）"));
        d
    }
}
