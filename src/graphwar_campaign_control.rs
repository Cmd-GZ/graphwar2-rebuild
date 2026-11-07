//! 原版 `src/graphwar_campaign_control.rs` —— **战役模式的关卡进度 + `campaign.save`**
//! + **21 关的参数表**（`level_info()` 的形状）。
//!
//! ## 定性（2026-11-07，`docs/16-client.md` §30.8，`观测`）
//!
//! 4 个函数（2.0.60 arm64 符号表，名字与 `docs/02-module-map.md` §18.5 完全一致）：
//!
//! | 符号 | arm64 | 字节 | 谁调 | 干什么（`逐位`：`FileAccess` 调用序列） |
//! |---|---|---|---|---|
//! | `get_current_level` | `0x392f18` | 528 | `RootNode::ready`、`LocalPlayerManager::handle_player_info` | `FileAccess::open` → `get_as_text` → `close` → `unreference`，再 `get_level_as_int` |
//! | `save_current_level` | `0x393128` | 1072 | `RootNode::check_save_level` | `open`/`get_as_text`/`store_string`/`close` **×2 组**，中间 `get_level_as_int` ×3 |
//! | `get_level_as_int` | `0x392cc0` | 600 | `get_current_level`、`save_current_level` | `StrSearcher::new` + `SpecFromIterNested::from_iter` ⇒ 用 `str::split`/`matches` 从关卡名里抠整数 |
//! | `show_hide_campaign_levels` | `0x393558` | 660 | `RootNode::ready`、`save_current_level` | `GraphwarNodes::get_node_by_name` ×1 + `<u32 as Display>::_fmt` ⇒ 拼节点名，按键显隐 |
//!
//! ## 它怎么表示"当前关卡"
//!
//! **存的是字符串，不是整数**：`get_current_level` 把整个存档文本读出来当关卡名用，
//! `get_level_as_int` 再从里面**抠最后一个 `_` 之后的十进制数**（`0x417ef0` 的
//! `str::split('_')` + `FromStr for u32` 内联，见 `docs/archive/118-campaign-data.md`
//! §3.1.2，`逐位`）。所以存档里就是 `level_<n>`，**0 基**。
//!
//! ## 本文件现在落了什么（2026-11-07，`dispatch/04-campaign-data.md`）
//!
//! | 项 | 对应原版地址 | 证据档 |
//! |---|---|---|
//! | [`LEVEL_INFO`]：21 关 × （障碍模式 / `min_count` / 士兵数 / 目标数 / Bot 难度 / 回合上限 / 标题+描述 / 模式四字节） | `level_info()` = `0x4ad380` 的 21 分支 `match`；数值参数在 `start_level` 各分支（`0x499740` 内联 8 关 + 13 个独立函数） | **逐位 + 运行时**（`docs/archive/118-campaign-data.md` §7.1 的 `S`/`C` 与 385 份回放 `R` 三源一致，21/21） |
//! | [`parse_level_key`] / [`encode_save`] / [`save_action`] / [`refresh_index`] | `0x417ef0` / `0x4183b0` / `0x418820` | **逐位**（`save_action` 的分支极性见 `docs/10-campaign.md` §12.4） |
//! | [`CampaignProgress`] / [`get_level_as_int`] | `0x417ef0` 的读语义 | **逐位** |
//!
//! ⚠️ **与 `graphwar_level_manager.rs` 的重复**：`docs/10-campaign.md` §12.6 记着
//! `level_info()` 与 `campaign.save` 那一簇**没有 `panic::Location` 锚**，暂时挂在
//! `graphwar_level_manager.rs` 名下，并写明"`graphwar_campaign_control.rs` 落地时必须搬走"。
//! 本轮落地了本文件的版本，但**没有**删 `level_manager` 里那份（本轮只允许改本文件）——
//! 两份由 `tests/campaign_data.rs` 的 `table_matches_level_manager_copy` /
//! `save_cluster_matches_level_manager_copy` 逐字段钉住，谁先漂就会被测出来。
//! 搬迁（删旧副本）留给下一次能改 `level_manager.rs` 的任务。
//!
//! ## 缺的两处归属（沿用旧结论，不重推）
//!
//! `docs/02-module-map.md` §17.4 记过：`level_info`(`0x4ad380`) 与 `campaign.save` 簇
//! (`0x417ef0`/`0x418170`/`0x4183b0`) **没有 `Location` 锚**，暂时挂在
//! `graphwar_level_manager.rs` 名下。本轮**没有**给它们找到新锚点，
//! 所以那条"待拆"状态**不变**（不多也不少一件事）。

/// 战役关卡数（`docs/archive/118-campaign-data.md` §5.2 的三处独立证据，`逐位`）。
pub const CAMPAIGN_LEVEL_COUNT: u32 = 21;

// ===========================================================================
// 1. 21 关的参数表（`level_info()` = `0x4ad380` 的 21 个分支，形状照抄）
// ===========================================================================

/// 一关的**障碍预设**（原版 `.data.rel.ro` 里那一族 0x28 字节闭包记录，A 侧）。
///
/// `docs/archive/118-campaign-data.md` §12.2：A 侧 `+0x20` 只有 6 种本体、3 种行为。
/// 与 `graphwar_level_generator::ObstacleMode` **同名同形**（原版本来是两个模块：
/// 预设闭包在关卡臂里，生成本体在 `graphwar_level_generator.rs`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObstacleMode {
    /// 无障碍：只 `reset_mask`（`0x49e570`/`0x49eab0`/`0x4a93e0`）。用于 `level_0/1/16`。
    None,
    /// 中心单障碍：恰好 1 颗、位置恒 `(0,0)`、`r = u*5+3 ∈ [3,8)`
    /// （`0x49f1c0`/`0x49f370`/`0x4a5e80`）。用于 `level_2/3/12`。
    Center,
    /// 随机散布：`n = trunc(normal()*7+15)`，被 `min_count` 抬高（`0x3f1e50`）。
    /// `min_count` 是**下界不是定值**（§7.2 的运行时硬证据）。
    Scatter,
}

/// 一关的 4 个模式字节（`+0x38..+0x3b`），原样保存**那一个 `imm32`**。
///
/// 依据：每关分支里 `mov dword ptr [rsp + 0x58], <imm32>`（21 条：`0x4ad4dd`、
/// `0x4ad62c`、…、`0x4aefdf`），结构体基址 `[rsp+0x20]` ⇒ 落在 `+0x38..+0x3b`
/// （`docs/archive/118-campaign-data.md` §4.3）。字段顺序与 `join_level_pop_up.tscn`
/// 的四个按钮同名同序，小端拆字节：`+0x38 = FunctionMode`、`+0x39 = TurnMode`、
/// `+0x3a = AxisMode`、`+0x3b = 时间档`。
///
/// 存 `imm32` 而不是四个字段，是为了让本表和二进制/文档**逐字节可对**；
/// 取字段用下面 4 个访问器。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelModes {
    /// `mov dword [rsp+0x58], imm32` 的那个 `imm32`（小端）。
    pub mode_word: u32,
}

impl LevelModes {
    /// `+0x38` FunctionMode：0 = f(x)，1 = 微分方程（`level_16..20` 是 1）。
    pub fn function(self) -> u8 {
        (self.mode_word & 0xff) as u8
    }

    /// `+0x39` TurnMode：0 = SequentialTurns，1 = SimultaneousTurns
    /// （恰好 `level_11` 与 `level_14`，与描述 "firing simultaneously" 2/2 一致）。
    pub fn turn(self) -> u8 {
        ((self.mode_word >> 8) & 0xff) as u8
    }

    /// `+0x3a` AxisMode：**3 = EveryUnit（最密）、2 = EveryFive、1 = OnlyMain、0 = NoAxis**。
    /// 运行时 `AxisModeInfo` 的名字与数字 21/21 一一对应（`docs/10-campaign.md` §4）。
    pub fn axis(self) -> u8 {
        ((self.mode_word >> 16) & 0xff) as u8
    }

    /// `+0x3b` 时间档，21/21 恒 `5`（= `TimerInf`）。
    ///
    /// 证据档取 **观测**：运行时 `TimeModeInfo` 21/21 都是 `TimerInf`，与 `0x05` 同向。
    /// ⚠️ 三份材料的档位不一致，本轮**就低不就高**：
    /// * `docs/archive/118-campaign-data.md` §4.3 / §8 第 6 条：**推断 → 观测**
    ///   （缺 `0x442aa0` 内对该字节的消费者）；
    /// * `docs/10-campaign.md` §7 第 6 条：**观测**（"仍缺 5 = TimerInf 枚举值的直接证据"）；
    /// * `src/graphwar_level_manager.rs` 的 `LevelModes::time_byte` 注释：声称 **逐位**
    ///   （依据是 `graphwar_setup_control` 的时间档名字表第 11 项）。
    ///
    /// 本轮没有二进制可复核，无法判定哪一份对。差异登记在 `docs/10-campaign.md` §16.6。
    pub fn time(self) -> u8 {
        ((self.mode_word >> 24) & 0xff) as u8
    }
}

/// 一关的敌方 Bot。
///
/// 原版有**两个不同的量**，别混：
/// * `difficulty` = `bot_added_event`(tag 10) 载荷第 3 个 u32，取自开场臂的立即数。
///   取值 `3 / 25 / 50 / 100 / 250`（`docs/archive/118-campaign-data.md` §12.3、§12.4）。
///   与 `docs/AI.md` 六档表的映射是 **推断**，且 `level_11` 的 `50` **对不上任何一列**
///   （§12.4 明写"有一处对不上，不圆"）。
/// * `tier` = 运行时 `SkinGraphicsInfo` 里的 `bot_N` 皮肤号（`R` 列）。
///   它和 `difficulty` **不同向**：`bot_3` 同时出现在 50 与 100。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BotOpponent {
    /// `bot_added_event` 载荷里的难度值（立即数）。
    pub difficulty: u32,
    /// `bot_N` 皮肤的 `N`（运行时观测）。
    pub tier: u8,
}

/// `level_info()` 的一条记录（21 条）。
///
/// 原版记录里还有 `+0x30` 一个 u32（部分关是常量、部分是运行期算的）——它**没有**
/// 已知消费者，`docs/archive/118-campaign-data.md` §4.4 判**未知**，故**不建模**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CampaignLevel {
    /// 0 基下标：同时是 `level_<index>` 与 `Level <index>` 里的那个数字。
    /// ⚠️ `docs/archive/118-campaign-data.md` §7 的表格用的是 **1..21** 的"关号"，
    /// 与本字段的关系是 `关号 = index + 1`。
    pub index: u8,
    /// 命令分派用的键：`level_0` … `level_20`（`0x499740` 里 21 条比较）。
    pub key: &'static str,
    /// 界面标题：`Level 0` … `Level 20`（21/21 逐字节精确，无副标题）。
    pub title: &'static str,
    /// 描述原文（`.rodata`，**未做任何改写**；`level_16` 的 `writting` 是原版自带的拼写，
    /// 数据里有 251 字节）。
    pub description: &'static str,
    /// 障碍预设（§12.2 的 A 侧闭包）。
    pub obstacles: ObstacleMode,
    /// 散布式的个数**下界** `min_count`（`if n <= min_count { n = min_count }`）。
    /// 非散布式一律 0：`None` 本来就没有障碍，`Center` 的"恰好 1 颗"由生成本体自己保证
    /// （与 `graphwar_level_generator::ObstacleParams::min_count` 的约定一致）。
    pub min_count: u32,
    /// `0x476320(mgr, player, N)` 的 `edx` 立即数 = 该关我方士兵数。
    pub soldiers: u32,
    /// `0x46f310(team)` 的调用次数 = 该关目标数（每个目标建在 `Team2`、名 `"target"`）。
    pub targets: u32,
    /// 敌方 Bot；`None` = 该关没有对手。
    pub bot: Option<BotOpponent>,
    /// `turn_limit_info`（tag 57）载荷 `(1, N)` 的 `N`；`None` = **无上限**。
    /// 载荷第一个 u32 恒为 `1`，含义**未知**（`docs/archive/118-campaign-data.md` §8 第 14 条），
    /// 故不建模。
    pub turn_limit: Option<u32>,
    /// `+0x38..+0x3b` 那 4 个字节。
    pub modes: LevelModes,
}

impl CampaignLevel {
    /// 描述的字节数。原版每关分支里 `mov qword ptr [rsp + 0x38], <len>` 的立即数
    /// 必须等于它 —— 21/21 的**逐位**自校验（§4.2 的长度列、§9 的判据）。
    pub fn description_len(&self) -> usize {
        self.description.len()
    }

    /// 该关是否有对手（8 关：`level_5/9/11/14/15/18/19/20`）。
    pub fn has_opponent(&self) -> bool {
        self.bot.is_some()
    }
}

const fn modes(mode_word: u32) -> LevelModes {
    LevelModes { mode_word }
}

const fn bot(difficulty: u32, tier: u8) -> Option<BotOpponent> {
    Some(BotOpponent { difficulty, tier })
}

/// **21 关全参数表**，顺序 = `level_0` … `level_20`（0 基）。
///
/// 每一行都逐条抄自 `docs/archive/118-campaign-data.md` **§7.1**（"关" 列是 1..21），
/// 行末注明该行用到的证据短码：
/// * `S` = `start_level` 分支里的**立即数**（地址可指认）；
/// * `C` = `.data.rel.ro` 的**闭包预设记录**（地址可指认）；
/// * `R` = **运行时真值**（385 份 replay，§12.5，与 `S`/`C` 独立）；
/// * 模式四字节见 §4.3（21 条 `mov dword [rsp+0x58], imm32`）。
///
/// `S`/`C` 与 `R` 两边都对上的格子才是"逐位验证 + 运行时"；只有单边的见 §7.1 的脚注。
/// 表与文档的逐项对应由 `tests/campaign_data.rs` 断言（21 关 × 7 个参数逐字段）。
pub const LEVEL_INFO: [CampaignLevel; 21] = [
    // ==== 关 1 / level_0：无障碍，1 兵 1 靶，无对手，无回合上限 ====
    // 〔118 §7.1 第 1 行：障碍 C 0x95a350/0x95a378 + R 0；士兵 S edx=1 + R 1；
    //   目标 S 1 次 add_target + R 1；Bot 无〔R〕；回合上限 无〔S；R〕；模式 0/0/3〕
    CampaignLevel {
        index: 0,
        key: "level_0",
        title: "Level 0",
        description: "Write a function of x to hit the target.",
        obstacles: ObstacleMode::None,
        min_count: 0,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: None,
        modes: modes(0x0503_0000),
    },
    // ==== 关 2 / level_1：无障碍，回合上限 5 ====
    // 〔118 §7.1 第 2 行：障碍 C + R 0；士兵 S + R 1；目标 S + R 1；回合上限 S (1,5) + R 5〕
    CampaignLevel {
        index: 1,
        key: "level_1",
        title: "Level 1",
        description: "A constant is always added to your function. You can see it on the bottom right corner of the screen. It makes sure the function is shot by your soldier.",
        obstacles: ObstacleMode::None,
        min_count: 0,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(5),
        modes: modes(0x0503_0000),
    },
    // ==== 关 3 / level_2：中心单障碍，回合上限 5 ====
    // 〔118 §7.1 第 3 行：障碍 C 0x49f1c0 + R 1 个 @(0,0) r=3.34；回合上限 S + R 5〕
    CampaignLevel {
        index: 2,
        key: "level_2",
        title: "Level 2",
        description: "There are obstacles in the cartesian plane that don't let your function go through. Try to make a function that goes around them.",
        obstacles: ObstacleMode::Center,
        min_count: 0,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(5),
        modes: modes(0x0503_0000),
    },
    // ==== 关 4 / level_3：中心单障碍，2 兵 3 靶，回合上限 10 ====
    // 〔118 §7.1 第 4 行：障碍 C 0x49f370 + R r=4.81/6.75；士兵 S edx=2 + R 2；
    //   目标 S 3 次 + R 3；回合上限 S + R 10。描述只讲士兵 ⇒ §7.3 记为"描述未覆盖"〕
    CampaignLevel {
        index: 3,
        key: "level_3",
        title: "Level 3",
        description: "You now have multiple soldiers in your team. At each turn you will shoot from a different one.",
        obstacles: ObstacleMode::Center,
        min_count: 0,
        soldiers: 2,
        targets: 3,
        bot: None,
        turn_limit: Some(10),
        modes: modes(0x0503_0000),
    },
    // ==== 关 5 / level_4：散布 min_count=15，回合上限 10 ====
    // 〔118 §7.1 第 5 行：障碍 C 0x49fe80（esi=15）+ R 15 个；回合上限 S (1,10) + R 10〕
    CampaignLevel {
        index: 4,
        key: "level_4",
        title: "Level 4",
        description: "Try going around a plane with more obstacles.",
        obstacles: ObstacleMode::Scatter,
        min_count: 15,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(10),
        modes: modes(0x0503_0000),
    },
    // ==== 关 6 / level_5：散布 min_count=5，2 兵 0 靶，Bot 难度 3 / bot_0，无回合上限 ====
    // 〔118 §7.1 第 6 行：障碍 C 0x4a12f0 + R 17 个；目标 0〔R；S 无 add_target〕；
    //   Bot 有×1 档 0〔S 难度值 3；R〕；回合上限 无〔S；R〕〕
    CampaignLevel {
        index: 5,
        key: "level_5",
        title: "Level 5",
        description: "Play against an opponent that shoots back.",
        obstacles: ObstacleMode::Scatter,
        min_count: 5,
        soldiers: 2,
        targets: 0,
        bot: bot(3, 0),
        turn_limit: None,
        modes: modes(0x0503_0000),
    },
    // ==== 关 7 / level_6：散布 min_count=10，1 兵 2 靶，回合上限 6 ====
    // 〔118 §7.1 第 7 行：障碍 C 0x4a2410 + R 17 个；目标 S 2 次 + R 2；回合上限 S (1,6) + R 6〕
    CampaignLevel {
        index: 6,
        key: "level_6",
        title: "Level 6",
        description: "You must hit these targets in a single shot to destroy them.",
        obstacles: ObstacleMode::Scatter,
        min_count: 10,
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(6),
        modes: modes(0x0503_0000),
    },
    // ==== 关 8 / level_7：散布 min_count=6，回合上限 1（"first try"） ====
    // 〔118 §7.1 第 8 行：障碍 C 0x4a2da0（esi=6）+ R 16 个；回合上限 S (1,1) + R 1〕
    CampaignLevel {
        index: 7,
        key: "level_7",
        title: "Level 7",
        description: "Destroy your target in your first try.",
        obstacles: ObstacleMode::Scatter,
        min_count: 6,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(1),
        modes: modes(0x0503_0000),
    },
    // ==== 关 9 / level_8：散布 min_count=10，1 兵 3 靶，回合上限 6 ====
    // 〔118 §7.1 第 9 行：障碍 C 0x4a2410 + R 16 个；目标 S 3 次 + R 3；回合上限 S + R 6；
    //   描述在 .rodata 的另一段 0x1bcaa0（§4.2 注）〕
    CampaignLevel {
        index: 8,
        key: "level_8",
        title: "Level 8",
        description: "Hit all targets in a single shot to destroy them.",
        obstacles: ObstacleMode::Scatter,
        min_count: 10,
        soldiers: 1,
        targets: 3,
        bot: None,
        turn_limit: Some(6),
        modes: modes(0x0503_0000),
    },
    // ==== 关 10 / level_9：散布 min_count=5，3 兵 0 靶，Bot 难度 25 / bot_2，无回合上限 ====
    // 〔118 §7.1 第 10 行：障碍 C 0x4a12f0 + R 15/17 个；士兵 S edx=3 + R 3；
    //   Bot 有×1 档 2〔S 25；R〕；回合上限 无〔S；R〕〕
    CampaignLevel {
        index: 9,
        key: "level_9",
        title: "Level 9",
        description: "Play against a smarter opponent.",
        obstacles: ObstacleMode::Scatter,
        min_count: 5,
        soldiers: 3,
        targets: 0,
        bot: bot(25, 2),
        turn_limit: None,
        modes: modes(0x0503_0000),
    },
    // ==== 关 11 / level_10：散布 min_count=10，axis 降到 2（"finer grid lines are gone"） ====
    // 〔118 §7.1 第 11 行：障碍 C 0x4a2410 + R 10/13 个；目标 S 2 次 + R 2；
    //   回合上限 S (1,6) + R 6；模式 0/0/2〕
    CampaignLevel {
        index: 10,
        key: "level_10",
        title: "Level 10",
        description: "Hit all targets in a single shot to destroy them. The finer grid lines are gone.",
        obstacles: ObstacleMode::Scatter,
        min_count: 10,
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(6),
        modes: modes(0x0502_0000),
    },
    // ==== 关 12 / level_11：散布 min_count=5，3 兵 0 靶，turn=1（同时开火），Bot 难度 50 / bot_3 ====
    // 〔118 §7.1 第 12 行：障碍 C 0x4a12f0 + R 15 个；Bot 有×1 档 3〔S 50；R〕；
    //   回合上限 无；模式 0/1/2。⚠️ 难度 50 与 docs/AI.md 六档表对不上（§12.4）〕
    CampaignLevel {
        index: 11,
        key: "level_11",
        title: "Level 11",
        description: "Play against an opponent on a plane with fewer grid lines. You and the opponent are also firing simultaneously.",
        obstacles: ObstacleMode::Scatter,
        min_count: 5,
        soldiers: 3,
        targets: 0,
        bot: bot(50, 3),
        turn_limit: None,
        modes: modes(0x0502_0100),
    },
    // ==== 关 13 / level_12：中心单障碍，axis 1，回合上限 5 ====
    // 〔118 §7.1 第 13 行：障碍 C 0x4a5e80 + R 1 个 @(0,0) r=5.69；
    //   回合上限 S (1,5) + R 5；模式 0/0/1〕
    CampaignLevel {
        index: 12,
        key: "level_12",
        title: "Level 12",
        description: "Most grid lines are gone. Try hitting a target.",
        obstacles: ObstacleMode::Center,
        min_count: 0,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(5),
        modes: modes(0x0501_0000),
    },
    // ==== 关 14 / level_13：散布 min_count=10，2 靶，回合上限 5 ====
    // 〔118 §7.1 第 14 行：障碍 C 0x4a2410 + R 12/27 个；目标 S 2 次 + R 2；回合上限 S + R 5〕
    CampaignLevel {
        index: 13,
        key: "level_13",
        title: "Level 13",
        description: "Destroy two targets in a single shot.",
        obstacles: ObstacleMode::Scatter,
        min_count: 10,
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(5),
        modes: modes(0x0501_0000),
    },
    // ==== 关 15 / level_14：散布 min_count=5，4 兵 0 靶，turn=1，Bot 难度 100 / bot_3 ====
    // 〔118 §7.1 第 15 行：障碍 C 0x4a12f0 + R 21 个；士兵 S edx=4 + R 4；
    //   Bot 有×1 档 3〔S 100；R〕；回合上限 无；模式 0/1/1〕
    CampaignLevel {
        index: 14,
        key: "level_14",
        title: "Level 14",
        description: "Play against an opponent with almost no grids.",
        obstacles: ObstacleMode::Scatter,
        min_count: 5,
        soldiers: 4,
        targets: 0,
        bot: bot(100, 3),
        turn_limit: None,
        modes: modes(0x0501_0100),
    },
    // ==== 关 16 / level_15：散布 min_count=10，5 兵 0 靶，axis 0，Bot 难度 250 / bot_4 ====
    // 〔118 §7.1 第 16 行：障碍 C 0x4a2410 + R 10…29 个；士兵 S edx=5 + R 5；
    //   Bot 有×1 档 4〔S 250；R〕；回合上限 无；模式 0/0/0〕
    CampaignLevel {
        index: 15,
        key: "level_15",
        title: "Level 15",
        description: "The main axes are gone. The opponent is smarter than ever.",
        obstacles: ObstacleMode::Scatter,
        min_count: 10,
        soldiers: 5,
        targets: 0,
        bot: bot(250, 4),
        turn_limit: None,
        modes: modes(0x0500_0000),
    },
    // ==== 关 17 / level_16：无障碍，func=1（微分方程），1 兵 1 靶，无回合上限 ====
    // 〔118 §7.1 第 17 行：障碍 C 0x95d358 + R 0；回合上限 无〔S；R〕；模式 1/0/3〕
    CampaignLevel {
        index: 16,
        key: "level_16",
        title: "Level 16",
        description: "You are now writting a differential equation instead of a function. Write a differential equation in terms of x and y. Your shot will be the solution to the differential equation using the position of the soldier that shot it as the initial condition.",
        obstacles: ObstacleMode::None,
        min_count: 0,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: None,
        modes: modes(0x0503_0001),
    },
    // ==== 关 18 / level_17：散布 min_count=10，1 兵 2 靶，回合上限 6 ====
    // 〔118 §7.1 第 18 行：障碍 C 0x4a2410 + R 10 个；目标 S 2 次 + R 2；
    //   回合上限 S (1,6) + R 6；模式 1/0/3〕
    CampaignLevel {
        index: 17,
        key: "level_17",
        title: "Level 17",
        description: "You must hit both targets in a single shot with a differential equation to destroy them.",
        obstacles: ObstacleMode::Scatter,
        min_count: 10,
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(6),
        modes: modes(0x0503_0001),
    },
    // ==== 关 19 / level_18：散布 min_count=10，3 兵 0 靶，Bot 难度 100 / bot_3 ====
    // 〔118 §7.1 第 19 行：障碍 C 0x4a2410 + R 14/15/23/29 个；Bot 有×1 档 3〔S 100；R〕〕
    CampaignLevel {
        index: 18,
        key: "level_18",
        title: "Level 18",
        description: "Fight an opponent with differential equations.",
        obstacles: ObstacleMode::Scatter,
        min_count: 10,
        soldiers: 3,
        targets: 0,
        bot: bot(100, 3),
        turn_limit: None,
        modes: modes(0x0503_0001),
    },
    // ==== 关 20 / level_19：散布 min_count=10，4 兵 0 靶，axis 1，Bot 难度 100 / bot_3 ====
    // 〔118 §7.1 第 20 行：障碍 C 0x4a2410 + R 10…25 个；士兵 S edx=4 + R 4；
    //   Bot 有×1 档 3〔S 100；R〕；模式 1/0/1〕
    CampaignLevel {
        index: 19,
        key: "level_19",
        title: "Level 19",
        description: "Fight an opponent with differential equations with less grid lines.",
        obstacles: ObstacleMode::Scatter,
        min_count: 10,
        soldiers: 4,
        targets: 0,
        bot: bot(100, 3),
        turn_limit: None,
        modes: modes(0x0501_0001),
    },
    // ==== 关 21 / level_20：散布 min_count=15，5 兵 0 靶，axis 0，Bot 难度 250 / bot_4 ====
    // 〔118 §7.1 第 21 行：障碍 C 0x49fe80 + R 15…31 个；士兵 S edx=5 + R 5；
    //   Bot 有×1 档 4〔S 250；R〕；回合上限 无；模式 1/0/0〕
    CampaignLevel {
        index: 20,
        key: "level_20",
        title: "Level 20",
        description: "Fight an opponent with differential equations and no grid lines.",
        obstacles: ObstacleMode::Scatter,
        min_count: 15,
        soldiers: 5,
        targets: 0,
        bot: bot(250, 4),
        turn_limit: None,
        modes: modes(0x0500_0001),
    },
];

/// 按**关卡下标**（0 基，= 二进制里的 `level_<n>`）查一关；越界 `None`。
///
/// 这就是 `level_info()`（`0x4ad380`）的查询语义：原版是一个 21 分支的 `match`，
/// 这里换成查表，返回同一份记录。
pub fn level_info(index: usize) -> Option<&'static CampaignLevel> {
    LEVEL_INFO.get(index)
}

/// 按**命令键**（`level_0` … `level_20`）查一关；不认识 `None`。
///
/// 对应 `0x499740` 里那 21 条字符串比较 —— 7 字节键（`level_0..9`）逐条 `xor`/`or`，
/// 8 字节键（`level_10..20`）11 条 `movabs`+`cmp`（`docs/archive/118-campaign-data.md` §12.1）。
pub fn level_info_by_key(key: &str) -> Option<&'static CampaignLevel> {
    LEVEL_INFO.iter().find(|lv| lv.key == key)
}

// ===========================================================================
// 2. `campaign.save`：读 / 写 / 单调 / 刷新下标
// ===========================================================================

/// `campaign.save` 的路径常量：`.rodata 0x1bfe28`，长度 `0x14` = 20（`逐位`）。
pub const CAMPAIGN_SAVE_PATH: &str = "user://campaign.save";

/// 解析 `level_<n>`（`0x417ef0` 的等价实现）。
///
/// 语义（`逐位`，`docs/archive/118-campaign-data.md` §3.1.2）：
/// `str::split('_')` 之后**取最后一段**，再 `parse::<u32>()`。
/// 所以 `level_20` → `Some(20)`、`level_0` → `Some(0)`、`anything_7` → `Some(7)`。
///
/// **不宽容，这正是原版的行为**：`parse::<u32>()` **不跳过空白**，`"level_20\n"`
/// 的最后一段是 `"20\n"` ⇒ 解析失败 ⇒ 进度读成 0。用户实测：`echo 'level_20' >`
/// 写出来的 9 字节文件让进度归零（`docs/archive/118-campaign-data.md` 附录 A）。
/// 写文件时多一个换行，玩家的进度就没了 —— 这是本项目的一条**逐位要求**。
pub fn parse_level_key(text: &str) -> Option<u32> {
    text.rsplit('_').next()?.parse::<u32>().ok()
}

/// 从关卡标识里抠整数（`get_level_as_int`，`0x392cc0` / x86 的 `0x417ef0`）。
///
/// 与 [`parse_level_key`] 同语义（取最后一个 `_` 之后的十进制数）。
/// 保留这个名字是因为它是原版的函数名、也是既有验收在用的入口。
///
/// 原版在解析失败时走 `unwrap_failed`（`get_current_level` 里 3 次），也就是**会 panic**；
/// 这里返回 `None`，把"原版会炸"这句话留在文档里而不是留在代码里。
pub fn get_level_as_int(level: &str) -> Option<u32> {
    parse_level_key(level)
}

/// 生成存档内容（`0x4183b0` 交给 `store_string` 的那个串）。
///
/// **没有尾换行**：`FileAccess::store_string` 只写字符串本身，不加换行
/// （`.rodata` 里 `FileAccess` 的被引用方法名只有 `store_string` / `get_file_as_bytes` /
/// `open`，**没有** `store_line`，见 §3.1.1）。所以长度必须恰好是
/// 7（`level_9`）或 8（`level_10`）。
pub fn encode_save(level: u32) -> String {
    format!("level_{level}")
}

/// 写不写？`0x4183b0` 的那段判定，逐条照搬。
///
/// 反汇编（`0x418554`…`0x4185b7`，`docs/10-campaign.md` §12.4 的**补正版**）：
///
/// ```text
/// ok_new, new = parse(要写的串)          // 0x418554 → ebx = ok1, ebp = new
/// ok_old, old = parse(文件现有内容)      // 0x418563 → ecx/eax = ok2, edx = old
/// 0x418570  or (!ok1), (!ok2); je 0x4185b3   // 两个都成功 → 去比较
/// 0x418574  cmp ok2, 1; jne 0x4185b7         // 旧值无效 → 写
/// 0x418579                                    // 旧值有效但新串无效 → 不写
/// 0x4185b3  cmp new, old; jbe 0x418579        // new <= old → 不写
/// 0x4185b7                                    // 写（0x41860f open mode=2 / 0x418647 store_string）
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

/// [`text_after_clearing`](CampaignSave::text_after_clearing) 的一次调用等价物：
/// 该写就返回要落盘的字符串，否则 `None`。
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
/// `call 0x418820(self, n+1)`（`0x4186d9`）。`0x418820` 内部有 `cmp ebp, 0x14`（= 20）
/// 的钳位，所以 20 之后再 +1 会被夹住。
///
/// ⚠️ `0x418820` 到底把按钮改成什么属性（`disabled`? 换图?）**未证**，`docs/10-campaign.md`
/// §5 判**推断**。本函数只还原"传什么数"。
pub fn refresh_index(new_level: u32) -> u32 {
    new_level.saturating_add(1).min(CAMPAIGN_LEVEL_COUNT - 1)
}

/// `campaign.save` 的内存模型：**只增不减**的一个进度标量。
///
/// 语义（`docs/archive/118-campaign-data.md` §3.1 + 附录 A，`逐位` + 用户实测）：
/// * 文件内容恰好是 `level_<n>`，**0 基**，**无尾换行**；
/// * `n` = **最后通关的那一关**（打完第 1 关写的是 `level_0`，不是 `level_1`）；
/// * **只增不减**：新值 ≤ 旧值直接返回，不动文件；
/// * **启动时不创建**：没有进度就没有要写的内容（[`CampaignSave::text`] 返回 `None`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CampaignSave {
    /// 最后通关的关卡下标（0 基）；`None` = 没有存档 / 解析失败。
    pub last_cleared: Option<u32>,
}

impl CampaignSave {
    /// 读存档（`0x418170` 的等价实现）。`content` 为 `None` = 文件打不开（不存在）。
    ///
    /// 打不开或解析失败都落到 `last_cleared: None` —— **这是原版的选择**（`0x418170`
    /// 打开失败返回 `(0, 0)`），不是我们加的容错。
    pub fn from_text(content: Option<&str>) -> Self {
        Self {
            last_cleared: content.and_then(parse_level_key),
        }
    }

    /// 该写进文件的内容；`None` = 无进度 ⇒ **不要在启动时创建文件**。
    pub fn text(&self) -> Option<String> {
        self.last_cleared.map(encode_save)
    }

    /// 通关第 `level` 关之后**该写进文件的内容**；单调，不该写则 `None`。
    ///
    /// **只算不改**：写盘成功之后，调用方才把 `self.last_cleared` 前移（写盘失败时进度
    /// 不能自己前进）—— 与原版 `0x4183b0` 的形状一致：先判定，再 `open(mode=2)` +
    /// `store_string`，最后才用新值刷新按钮（`0x418820(self, n+1)`）。
    pub fn text_after_clearing(&self, level: u32) -> Option<String> {
        let existing = self.text();
        save_level(existing.as_deref(), level)
    }

    /// 解锁到的最靠后的关卡下标（0 基，**含**）。
    ///
    /// `last_cleared = n` ⇒ `0..=n` 都通关了；再往下一关（`n+1`）是"当前要打的那一关"。
    /// ⚠️ `+1` 判**推断**：文件只给了"最后通关的那一关"，"下一关自动可玩"这一点没有
    /// 直接观测；依据是 `0x418820(self, n+1)` 这个 `+1`（§3.1.3）与客户端
    /// `client/src/app/store.gd::highest_unlocked_level()` 的同一选择。
    pub fn highest_playable_level(&self) -> u32 {
        match self.last_cleared {
            Some(n) => refresh_index(n),
            // 没有存档：第 0 关可玩（`0x418820` 的 `cmove` 钳位下限是 0）。
            None => 0,
        }
    }

    /// 第 `level`（0 基）关是否已解锁。
    pub fn is_level_unlocked(&self, level: u32) -> bool {
        level <= self.highest_playable_level() && level < CAMPAIGN_LEVEL_COUNT
    }
}

/// 已解锁进度（`get_current_level` 的结果形状）。
///
/// 原版返回的是**字符串**（整个存档文本），这里换成整数是因为
/// 所有调用方都只是拿它去 `show_hide_campaign_levels` 比较。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CampaignProgress {
    /// 已解锁的最高关（0 = 一关都没过）。
    pub unlocked_through: u32,
}

impl CampaignProgress {
    /// 存档文本 → 进度。解析失败（含**尾换行**）一律当 0 —— 与原版一致（附录 A）。
    pub fn from_save_text(text: &str) -> Self {
        Self {
            unlocked_through: get_level_as_int(text).unwrap_or(0),
        }
    }

    /// `show_hide_campaign_levels` 的判据：第 `level` 关（1 起）是否显示。
    ///
    /// 现有实现（**未改动**，既有验收 `campaign_visibility_follows_progress` 在用）：
    /// `level <= unlocked_through + 1`。
    /// ⚠️ 那个 `+1`（"下一关也放出来"）是**推断**：`docs/10-campaign.md` §5 记着
    /// `0x418820` 里节点访问器的落点未证；旁证是写入后 `0x418820(self, n+1)` 传的是 `n+1`。
    pub fn is_level_visible(&self, level: u32) -> bool {
        level >= 1 && level <= CAMPAIGN_LEVEL_COUNT && level <= self.unlocked_through + 1
    }
}
