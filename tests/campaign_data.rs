//! 战役 21 关参数表 + `campaign.save` 存档语义的验收。
//!
//! 覆盖 `dispatch/04-campaign-data.md` 的三件事：
//! 1. `src/graphwar_campaign_control.rs` 的 [`LEVEL_INFO`] 与
//!    `docs/archive/118-campaign-data.md` **§7.1** 的 21 行逐字段相等；
//! 2. 存档语义（§3.1 + 附录 A）：`level_<n>`、0 基、无尾换行、只增不减、启动不创建；
//! 3. `client/src/app/campaign.gd` **不再有第二份表**（T67 搬回 Rust：`.gd` 只调
//!    `GwCampaignControl`，那个类逐字段读 `LEVEL_INFO`）；本文件盯住"表没被抄回来"。
//!
//! ⚠️ 按 `AGENTS.md` §0 第 2 条：本文件的断言是**回归保护**，不是复原证据。
//! 复原证据是 `docs/archive/118-campaign-data.md` §7.1 里逐格标注的 `S`/`C`/`R`
//! （立即数地址 / 闭包记录地址 / 385 份回放），以及 §4.2/§4.3 的 21 条立即数。
//!
//! 为什么测试里的表格是**再抄一遍**而不是从代码生成：这样"代码对文档"这一步才有
//! 内容。文档对二进制那一步由 `tools/campaign_params.py` + §7.1 的 `R` 列负责
//! （见 `docs/10-campaign.md` §12.7 的同一做法）。

use gw2_rebuild::graphwar_campaign_control as cc;
use gw2_rebuild::graphwar_level_generator as lg;
use gw2_rebuild::graphwar_level_manager as lm;

// ===========================================================================
// 1. 文档 §7.1 的 21 行，逐字段抄下来（关号 1..21 = index+1）
// ===========================================================================

/// §7.1「障碍（模式 / 数量）」列。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocObstacle {
    /// **无障碍**〔C `0x95a350`/`0x95a378`/`0x95d358`；R 0 个〕
    None,
    /// **中心单障碍**（原点，`r ∈ [3,8)`）〔C `0x49f1c0`/`0x49f370`/`0x4a5e80`；R 1 个 @(0,0)〕
    Center,
    /// **散布式，`min_count = N`**〔C 闭包 + R 实测〕
    Scatter(u32),
}

/// §7.1 的一行。
#[derive(Clone, Copy)]
struct DocRow {
    /// 文档的"关"列（1..21）。
    level_no: u32,
    obstacle: DocObstacle,
    soldiers: u32,
    targets: u32,
    /// `(难度值, bot_N 皮肤号)`；`None` = 无对手。
    /// 「难度值」是 `bot_added_event` 载荷里的立即数（3/25/50/100/250，§12.3/§12.4），
    /// 「`bot_N`」是运行时皮肤号（§7.1 的 `R` 列），两者**不同向**。
    bot: Option<(u32, u8)>,
    /// `turn_limit_info` 载荷 `(1, N)` 的 `N`；`None` = 无上限。
    turn_limit: Option<u32>,
    title: &'static str,
    description: &'static str,
    /// 每关分支里 `mov dword ptr [rsp + 0x58], imm32`（§4.3）。
    mode_word: u32,
    /// 每关分支里 `mov qword ptr [rsp + 0x38], len`（§4.2 的长度列）。
    description_len: usize,
}

/// `docs/archive/118-campaign-data.md` §7.1（+ §4.2 长度、§4.3 模式 dword）。
const DOC_118_S7: [DocRow; 21] = [
    DocRow {
        level_no: 1,
        obstacle: DocObstacle::None,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: None,
        title: "Level 0",
        description: "Write a function of x to hit the target.",
        mode_word: 0x0503_0000,
        description_len: 40,
    },
    DocRow {
        level_no: 2,
        obstacle: DocObstacle::None,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(5),
        title: "Level 1",
        description: "A constant is always added to your function. You can see it on the bottom right corner of the screen. It makes sure the function is shot by your soldier.",
        mode_word: 0x0503_0000,
        description_len: 153,
    },
    DocRow {
        level_no: 3,
        obstacle: DocObstacle::Center,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(5),
        title: "Level 2",
        description: "There are obstacles in the cartesian plane that don't let your function go through. Try to make a function that goes around them.",
        mode_word: 0x0503_0000,
        description_len: 129,
    },
    DocRow {
        level_no: 4,
        obstacle: DocObstacle::Center,
        soldiers: 2,
        targets: 3,
        bot: None,
        turn_limit: Some(10),
        title: "Level 3",
        description: "You now have multiple soldiers in your team. At each turn you will shoot from a different one.",
        mode_word: 0x0503_0000,
        description_len: 94,
    },
    DocRow {
        level_no: 5,
        obstacle: DocObstacle::Scatter(15),
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(10),
        title: "Level 4",
        description: "Try going around a plane with more obstacles.",
        mode_word: 0x0503_0000,
        description_len: 45,
    },
    DocRow {
        level_no: 6,
        obstacle: DocObstacle::Scatter(5),
        soldiers: 2,
        targets: 0,
        bot: Some((3, 0)),
        turn_limit: None,
        title: "Level 5",
        description: "Play against an opponent that shoots back.",
        mode_word: 0x0503_0000,
        description_len: 42,
    },
    DocRow {
        level_no: 7,
        obstacle: DocObstacle::Scatter(10),
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(6),
        title: "Level 6",
        description: "You must hit these targets in a single shot to destroy them.",
        mode_word: 0x0503_0000,
        description_len: 60,
    },
    DocRow {
        level_no: 8,
        obstacle: DocObstacle::Scatter(6),
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(1),
        title: "Level 7",
        description: "Destroy your target in your first try.",
        mode_word: 0x0503_0000,
        description_len: 38,
    },
    DocRow {
        level_no: 9,
        obstacle: DocObstacle::Scatter(10),
        soldiers: 1,
        targets: 3,
        bot: None,
        turn_limit: Some(6),
        title: "Level 8",
        description: "Hit all targets in a single shot to destroy them.",
        mode_word: 0x0503_0000,
        description_len: 49,
    },
    DocRow {
        level_no: 10,
        obstacle: DocObstacle::Scatter(5),
        soldiers: 3,
        targets: 0,
        bot: Some((25, 2)),
        turn_limit: None,
        title: "Level 9",
        description: "Play against a smarter opponent.",
        mode_word: 0x0503_0000,
        description_len: 32,
    },
    DocRow {
        level_no: 11,
        obstacle: DocObstacle::Scatter(10),
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(6),
        title: "Level 10",
        description: "Hit all targets in a single shot to destroy them. The finer grid lines are gone.",
        mode_word: 0x0502_0000,
        description_len: 80,
    },
    DocRow {
        level_no: 12,
        obstacle: DocObstacle::Scatter(5),
        soldiers: 3,
        targets: 0,
        bot: Some((50, 3)),
        turn_limit: None,
        title: "Level 11",
        description: "Play against an opponent on a plane with fewer grid lines. You and the opponent are also firing simultaneously.",
        mode_word: 0x0502_0100,
        description_len: 111,
    },
    DocRow {
        level_no: 13,
        obstacle: DocObstacle::Center,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: Some(5),
        title: "Level 12",
        description: "Most grid lines are gone. Try hitting a target.",
        mode_word: 0x0501_0000,
        description_len: 47,
    },
    DocRow {
        level_no: 14,
        obstacle: DocObstacle::Scatter(10),
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(5),
        title: "Level 13",
        description: "Destroy two targets in a single shot.",
        mode_word: 0x0501_0000,
        description_len: 37,
    },
    DocRow {
        level_no: 15,
        obstacle: DocObstacle::Scatter(5),
        soldiers: 4,
        targets: 0,
        bot: Some((100, 3)),
        turn_limit: None,
        title: "Level 14",
        description: "Play against an opponent with almost no grids.",
        mode_word: 0x0501_0100,
        description_len: 46,
    },
    DocRow {
        level_no: 16,
        obstacle: DocObstacle::Scatter(10),
        soldiers: 5,
        targets: 0,
        bot: Some((250, 4)),
        turn_limit: None,
        title: "Level 15",
        description: "The main axes are gone. The opponent is smarter than ever.",
        mode_word: 0x0500_0000,
        description_len: 58,
    },
    DocRow {
        level_no: 17,
        obstacle: DocObstacle::None,
        soldiers: 1,
        targets: 1,
        bot: None,
        turn_limit: None,
        title: "Level 16",
        description: "You are now writting a differential equation instead of a function. Write a differential equation in terms of x and y. Your shot will be the solution to the differential equation using the position of the soldier that shot it as the initial condition.",
        mode_word: 0x0503_0001,
        description_len: 251,
    },
    DocRow {
        level_no: 18,
        obstacle: DocObstacle::Scatter(10),
        soldiers: 1,
        targets: 2,
        bot: None,
        turn_limit: Some(6),
        title: "Level 17",
        description: "You must hit both targets in a single shot with a differential equation to destroy them.",
        mode_word: 0x0503_0001,
        description_len: 88,
    },
    DocRow {
        level_no: 19,
        obstacle: DocObstacle::Scatter(10),
        soldiers: 3,
        targets: 0,
        bot: Some((100, 3)),
        turn_limit: None,
        title: "Level 18",
        description: "Fight an opponent with differential equations.",
        mode_word: 0x0503_0001,
        description_len: 46,
    },
    DocRow {
        level_no: 20,
        obstacle: DocObstacle::Scatter(10),
        soldiers: 4,
        targets: 0,
        bot: Some((100, 3)),
        turn_limit: None,
        title: "Level 19",
        description: "Fight an opponent with differential equations with less grid lines.",
        mode_word: 0x0501_0001,
        description_len: 67,
    },
    DocRow {
        level_no: 21,
        obstacle: DocObstacle::Scatter(15),
        soldiers: 5,
        targets: 0,
        bot: Some((250, 4)),
        turn_limit: None,
        title: "Level 20",
        description: "Fight an opponent with differential equations and no grid lines.",
        mode_word: 0x0500_0001,
        description_len: 64,
    },
];

// ===========================================================================
// 2. 21 关 × 7 个参数，逐字段断言
// ===========================================================================

/// 7 个参数的逐项计数，最后断言每一项都是 21 ⇒ 一句话说出"21/21"。
#[test]
fn all_21_levels_match_docs_118_s7_field_by_field() {
    assert_eq!(
        cc::LEVEL_INFO.len(),
        21,
        "关卡总数必须是 21（§5.2 三处独立证据）"
    );
    assert_eq!(
        cc::LEVEL_INFO.len(),
        cc::CAMPAIGN_LEVEL_COUNT as usize,
        "表长必须等于 CAMPAIGN_LEVEL_COUNT"
    );
    assert_eq!(DOC_118_S7.len(), 21, "文档抄写表也必须是 21 行");

    // 7 个参数：0 障碍模式 / 1 min_count / 2 士兵 / 3 目标 / 4 Bot 难度 / 5 回合上限 / 6 标题+描述
    let mut hits = [0usize; 7];

    for (i, doc) in DOC_118_S7.iter().enumerate() {
        let lv = cc::level_info(i).unwrap_or_else(|| panic!("第 {i} 关（{}）查不到", doc.level_no));
        let ctx = format!("关 {}（index {i}）", doc.level_no);

        // 表外的一致性：编号/键/标题的生成规则（21/21 逐字节精确，§4.1）
        assert_eq!(
            doc.level_no,
            i as u32 + 1,
            "{ctx}：文档关号与下标的关系必须是 关号 = index+1"
        );
        assert_eq!(lv.index as usize, i, "{ctx}：index 必须等于表下标（0 基）");
        assert_eq!(
            lv.key,
            format!("level_{i}"),
            "{ctx}：命令键必须是 level_<n>"
        );
        assert_eq!(lv.title, doc.title, "{ctx}：标题必须与 §7.1 一致");
        assert_eq!(
            lv.title,
            format!("Level {i}"),
            "{ctx}：标题必须是 Level <n>（无副标题）"
        );

        // ① 障碍模式
        let want_obstacle = match doc.obstacle {
            DocObstacle::None => cc::ObstacleMode::None,
            DocObstacle::Center => cc::ObstacleMode::Center,
            DocObstacle::Scatter(_) => cc::ObstacleMode::Scatter,
        };
        assert_eq!(lv.obstacles, want_obstacle, "{ctx}：障碍模式与 §7.1 不符");
        hits[0] += 1;

        // ② min_count（散布式的个数下界；非散布式一律 0 —— 中心式的"恰好 1 颗"由本体保证）
        let want_min_count = match doc.obstacle {
            DocObstacle::None | DocObstacle::Center => 0,
            DocObstacle::Scatter(n) => n,
        };
        assert_eq!(
            lv.min_count, want_min_count,
            "{ctx}：min_count 与 §7.1/§7.2 不符"
        );
        hits[1] += 1;

        // ③ 士兵数
        assert_eq!(lv.soldiers, doc.soldiers, "{ctx}：士兵数与 §7.1 不符");
        hits[2] += 1;

        // ④ 目标数
        assert_eq!(lv.targets, doc.targets, "{ctx}：目标数与 §7.1 不符");
        hits[3] += 1;

        // ⑤ Bot 难度（难度值 + bot_N 皮肤号，两者都要对）
        assert_eq!(
            lv.bot.map(|b| (b.difficulty, b.tier)),
            doc.bot,
            "{ctx}：Bot（难度值, bot_N）与 §7.1/§12.3 不符"
        );
        hits[4] += 1;

        // ⑥ 回合上限
        assert_eq!(lv.turn_limit, doc.turn_limit, "{ctx}：回合上限与 §7.1 不符");
        hits[5] += 1;

        // ⑦ 标题 + 描述（逐字节）+ 描述长度（= 那 21 条 mov qword 立即数）
        assert_eq!(
            lv.description, doc.description,
            "{ctx}：描述必须逐字节等于 §4.2 原文"
        );
        assert_eq!(
            lv.description_len(),
            doc.description_len,
            "{ctx}：描述长度必须等于 §4.2 的 mov [rsp+0x38] 立即数"
        );
        hits[6] += 1;

        // 模式四字节（level_info() 记录里 +0x38..+0x3b 的那一个 imm32）
        assert_eq!(
            lv.modes.mode_word, doc.mode_word,
            "{ctx}：模式 dword 必须是 §4.3 的 imm32"
        );
    }

    assert_eq!(hits, [21, 21, 21, 21, 21, 21, 21], "7 个参数都必须是 21/21");
}

/// 模式字节的方向与文档的解读必须一致（§4.3 的自校验三条）。
#[test]
fn mode_bytes_agree_with_the_documented_reading() {
    for (i, lv) in cc::LEVEL_INFO.iter().enumerate() {
        assert_eq!(
            lv.modes.time(),
            5,
            "level_{i}：时间档 21/21 恒 5（TimerInf）"
        );
        // +0x38 = FunctionMode：1 恰好是 16..20
        let want_func = u8::from(i >= 16);
        assert_eq!(
            lv.modes.function(),
            want_func,
            "level_{i}：FunctionMode 与 §4.3 不符"
        );
        // +0x39 = TurnMode：1 恰好是 11 与 14（描述 "firing simultaneously"）
        let want_turn = u8::from(i == 11 || i == 14);
        assert_eq!(
            lv.modes.turn(),
            want_turn,
            "level_{i}：TurnMode 与 §4.3 不符"
        );
        // +0x3a = AxisMode：3/2/1/0 = EveryUnit/EveryFive/OnlyMain/NoAxis（21/21 与描述同向）
        let want_axis = match i {
            0..=9 => 3,
            10 => 2,
            11 => 2,
            12 | 13 => 1,
            14 => 1,
            15 => 0,
            16..=18 => 3,
            19 => 1,
            20 => 0,
            _ => unreachable!(),
        };
        assert_eq!(
            lv.modes.axis(),
            want_axis,
            "level_{i}：AxisMode 与 §4.3 不符"
        );
    }
}

/// 8 个对手关：全部无目标、各 1 个 Bot（§7.3 的最后一行）。
#[test]
fn the_eight_opponent_levels_have_no_targets_and_one_bot() {
    let opponents: Vec<usize> = cc::LEVEL_INFO
        .iter()
        .enumerate()
        .filter(|(_, lv)| lv.has_opponent())
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        opponents,
        vec![5, 9, 11, 14, 15, 18, 19, 20],
        "对手关的集合与 §7.1 不符"
    );
    for &i in &opponents {
        let lv = &cc::LEVEL_INFO[i];
        assert_eq!(lv.targets, 0, "level_{i}：对手关必须 0 个目标");
        assert_eq!(
            lv.bot.map(|b| b.difficulty).is_some(),
            true,
            "level_{i}：必须有 Bot"
        );
    }
    // 反向：其余 13 关都没有对手（靶子关）
    for (i, lv) in cc::LEVEL_INFO.iter().enumerate() {
        if !opponents.contains(&i) {
            assert!(!lv.has_opponent(), "level_{i}：不该有对手");
            assert!(lv.targets >= 1, "level_{i}：靶子关至少 1 个目标");
        }
    }
}

/// 查询接口：按下标 / 按命令键，越界返回 `None`。
#[test]
fn level_lookup_is_by_index_and_by_key() {
    for i in 0..21usize {
        assert_eq!(
            cc::level_info(i),
            Some(&cc::LEVEL_INFO[i]),
            "按下标查第 {i} 关"
        );
        assert_eq!(
            cc::level_info_by_key(&format!("level_{i}")),
            Some(&cc::LEVEL_INFO[i]),
            "按命令键查第 {i} 关"
        );
    }
    assert_eq!(cc::level_info(21), None, "越界必须是 None");
    assert_eq!(
        cc::level_info_by_key("level_21"),
        None,
        "不认识的键必须是 None"
    );
    assert_eq!(cc::level_info_by_key("Level 0"), None, "标题不是命令键");
}

/// 原版的拼写错误是数据的一部分，别"修"（§4.2 注）。
#[test]
fn the_original_typo_writting_is_preserved() {
    let lv = cc::level_info(16).unwrap();
    assert!(
        lv.description.contains("writting"),
        "level_16 的原文就是 writting（双 t）"
    );
    assert!(!lv.description.contains("writing "), "不许把原版的拼写改掉");
}

// ===========================================================================
// 3. 与既有两份拷贝交叉校验（防漂）
// ===========================================================================

/// `graphwar_level_manager.rs` 里那份**临时副本**（`docs/10` §12.6 说它要搬走）。
#[test]
fn table_matches_level_manager_copy() {
    assert_eq!(lm::LEVELS.len(), cc::LEVEL_INFO.len());
    assert_eq!(lm::SETUPS.len(), cc::LEVEL_INFO.len());
    for i in 0..cc::LEVEL_INFO.len() {
        let a = &cc::LEVEL_INFO[i];
        let ui = &lm::LEVELS[i];
        let setup = &lm::SETUPS[i];
        assert_eq!(ui.index as usize, i, "level_{i}：level_manager 的 index");
        assert_eq!(ui.key, a.key, "level_{i}：key");
        assert_eq!(ui.title, a.title, "level_{i}：title");
        assert_eq!(ui.description, a.description, "level_{i}：description");
        assert_eq!(
            ui.description_len,
            a.description_len(),
            "level_{i}：description_len"
        );
        assert_eq!(
            ui.modes.function as u8,
            a.modes.function(),
            "level_{i}：FunctionMode"
        );
        assert_eq!(ui.modes.turns as u8, a.modes.turn(), "level_{i}：TurnMode");
        assert_eq!(ui.modes.axis as u8, a.modes.axis(), "level_{i}：AxisMode");
        assert_eq!(ui.modes.time_byte, a.modes.time(), "level_{i}：时间档");
        assert_eq!(setup.soldiers, a.soldiers, "level_{i}：士兵数");
        assert_eq!(setup.targets, a.targets, "level_{i}：目标数");
        assert_eq!(
            setup.bot,
            a.bot.map(|b| b.difficulty),
            "level_{i}：Bot 难度值"
        );
        assert_eq!(setup.turn_limit, a.turn_limit, "level_{i}：回合上限");
        // 障碍预设（两种枚举各说各的名字）
        match (setup.preset, a.obstacles) {
            (lm::ObstaclePreset::None, cc::ObstacleMode::None) => {}
            (lm::ObstaclePreset::CentreSingle, cc::ObstacleMode::Center) => {}
            (lm::ObstaclePreset::Scatter { min_count }, cc::ObstacleMode::Scatter) => {
                assert_eq!(min_count, a.min_count as i32, "level_{i}：min_count")
            }
            (x, y) => panic!("level_{i}：障碍模式不一致 {x:?} vs {y:?}"),
        }
        // `graphwar_level_generator.rs` 的那张障碍参数表（同一份数据的 third copy）
        let params = lg::params_for_level(i).expect("21 关都必须有障碍参数");
        assert_eq!(
            params.min_count, a.min_count as i32,
            "level_{i}：level_generator 的 min_count"
        );
        match (params.mode, a.obstacles) {
            (lg::ObstacleMode::None, cc::ObstacleMode::None) => {}
            (lg::ObstacleMode::Center, cc::ObstacleMode::Center) => {}
            (lg::ObstacleMode::Scatter, cc::ObstacleMode::Scatter) => {}
            (x, y) => panic!("level_{i}：level_generator 的障碍模式不一致 {x:?} vs {y:?}"),
        }
    }
}

// ===========================================================================
// 4. `campaign.save` 的存档语义（§3.1 + 附录 A）
// ===========================================================================

/// 解析：取**最后一个** `_` 之后的部分，`parse::<u32>()`，**不跳空白**。
#[test]
fn parse_level_key_is_split_then_strict_parse() {
    // 正常形状
    assert_eq!(cc::parse_level_key("level_0"), Some(0));
    assert_eq!(cc::parse_level_key("level_20"), Some(20));
    assert_eq!(
        cc::parse_level_key("anything_7"),
        Some(7),
        "前缀任意，只看最后一段"
    );
    assert_eq!(
        cc::parse_level_key(" level_3"),
        Some(3),
        "前导空格在最后一段之前 ⇒ 不影响"
    );
    assert_eq!(cc::parse_level_key("level_007"), Some(7), "u32 允许前导 0");
    // 尾换行 **必须** 失败（docs/118 附录 A 的用户实测：多一个 \n 进度归零）
    assert_eq!(
        cc::parse_level_key("level_20\n"),
        None,
        "尾换行 ⇒ 解析失败（逐位要求）"
    );
    assert_eq!(cc::parse_level_key("level_20\n\n"), None);
    assert_eq!(cc::parse_level_key("level_20 "), None, "尾空格同样失败");
    assert_eq!(cc::parse_level_key("level_20\r\n"), None, "CRLF 同样失败");
    // 其它失败形状
    assert_eq!(cc::parse_level_key(""), None);
    assert_eq!(cc::parse_level_key("level_"), None, "空尾段");
    assert_eq!(cc::parse_level_key("level_-1"), None, "u32 不接受负号");
    assert_eq!(cc::parse_level_key("level_abc"), None);
    assert_eq!(
        cc::parse_level_key("level_4294967296"),
        None,
        "超出 u32 上界"
    );
    // 旧名字必须是同一个语义（既有验收在用它）
    assert_eq!(
        cc::get_level_as_int("level_7"),
        cc::parse_level_key("level_7")
    );
    assert_eq!(
        cc::get_level_as_int("level_20\n"),
        None,
        "get_level_as_int 也必须严格"
    );
}

/// 写入串：`level_<n>`、**无尾换行**、0 基。
#[test]
fn encode_save_has_no_trailing_newline_and_is_zero_based() {
    assert_eq!(cc::encode_save(0), "level_0");
    assert_eq!(cc::encode_save(0).len(), 7, "level_0 必须 7 字节");
    assert_eq!(cc::encode_save(20), "level_20");
    assert_eq!(cc::encode_save(20).len(), 8, "level_20 必须 8 字节");
    for n in 0..21u32 {
        let text = cc::encode_save(n);
        assert!(
            !text.ends_with('\n') && !text.ends_with(' '),
            "level_{n} 不许带尾字节"
        );
        assert_eq!(
            cc::parse_level_key(&text),
            Some(n),
            "写出去必须能原样读回来"
        );
    }
    assert_eq!(
        cc::CAMPAIGN_SAVE_PATH,
        "user://campaign.save",
        "路径常量 0x1bfe28，长 20"
    );
    assert_eq!(cc::CAMPAIGN_SAVE_PATH.len(), 20);
}

/// 写不写：三个分支都要对（`0x418554`…`0x4185b7`，`docs/10` §12.4）。
#[test]
fn save_action_has_three_branches() {
    // 旧值无效（文件不存在 / 空 / 带换行）⇒ 写
    assert_eq!(cc::save_action(None, "level_0"), cc::SaveAction::Write);
    assert_eq!(cc::save_action(Some(""), "level_0"), cc::SaveAction::Write);
    assert_eq!(
        cc::save_action(Some("level_20\n"), "level_3"),
        cc::SaveAction::Write
    );
    assert_eq!(
        cc::save_action(Some("garbage"), "level_3"),
        cc::SaveAction::Write
    );
    // 两个都有效 ⇒ 比大小：new <= old 不写
    assert_eq!(
        cc::save_action(Some("level_5"), "level_5"),
        cc::SaveAction::Skip,
        "相等不写"
    );
    assert_eq!(
        cc::save_action(Some("level_5"), "level_4"),
        cc::SaveAction::Skip,
        "变小不写"
    );
    assert_eq!(
        cc::save_action(Some("level_5"), "level_6"),
        cc::SaveAction::Write,
        "变大才写"
    );
    // 旧值有效、新串坏 ⇒ **不写**（原版不会用坏串覆盖好存档）
    assert_eq!(
        cc::save_action(Some("level_5"), "level_x"),
        cc::SaveAction::Skip
    );
    // 两个都坏 ⇒ 旧值无效 ⇒ 写
    assert_eq!(
        cc::save_action(Some("level_5\n"), "level_x"),
        cc::SaveAction::Write
    );

    // `save_level` 是同一个判定的"要写就给出字符串"版本
    assert_eq!(cc::save_level(None, 0), Some("level_0".to_string()));
    assert_eq!(cc::save_level(Some("level_5"), 4), None);
    assert_eq!(
        cc::save_level(Some("level_5"), 6),
        Some("level_6".to_string())
    );
}

/// 刷新下标 = `min(n+1, 20)`（`0x418820` 里的 `cmp ebp, 0x14` 钳位，`观测`）。
#[test]
fn refresh_index_adds_one_and_clamps_at_twenty() {
    assert_eq!(cc::refresh_index(0), 1);
    assert_eq!(cc::refresh_index(19), 20);
    assert_eq!(cc::refresh_index(20), 20, "0x14 = 20 是上限");
    assert_eq!(cc::refresh_index(21), 20);
    assert_eq!(cc::refresh_index(u32::MAX), 20, "饱和运算，不许 panic");
}

/// `CampaignSave` 的三条硬语义：启动不创建、只增不减、0 基=最后通关关卡。
#[test]
fn campaign_save_is_monotonic_and_not_created_at_startup() {
    // 启动时（没有文件）：没有要写的内容 ⇒ 绝不创建文件
    let fresh = cc::CampaignSave::default();
    assert_eq!(fresh.last_cleared, None);
    assert_eq!(fresh.text(), None, "启动时不创建 campaign.save");

    // 第一次通关第 1 关：写 `level_0`（0 基 ⇒ 不是 level_1）
    assert_eq!(fresh.text_after_clearing(0), Some("level_0".to_string()));
    // 同一个进度再写一次 ⇒ 不写
    let after_first = cc::CampaignSave::from_text(Some("level_0"));
    assert_eq!(after_first.last_cleared, Some(0));
    assert_eq!(
        after_first.text_after_clearing(0),
        None,
        "只增不减：相等不写"
    );
    // 变大才写
    assert_eq!(
        after_first.text_after_clearing(3),
        Some("level_3".to_string())
    );
    // 变小不写（`text_after_clearing` 不改 self，写盘失败时进度不能自己前进）
    let at_three = cc::CampaignSave::from_text(Some("level_3"));
    assert_eq!(at_three.text_after_clearing(1), None, "只增不减：变小不写");
    assert_eq!(at_three.text_after_clearing(2), None, "只增不减：变小不写");
    assert_eq!(
        at_three.last_cleared,
        Some(3),
        "text_after_clearing 不许改内存里的进度"
    );

    // 解析失败（尾换行）⇒ 当"什么都没通"，下一次写入会覆盖它
    let broken = cc::CampaignSave::from_text(Some("level_20\n"));
    assert_eq!(broken.last_cleared, None, "带换行的存档解析失败（附录 A）");
    assert_eq!(broken.text(), None);
    assert_eq!(
        broken.text_after_clearing(2),
        Some("level_2".to_string()),
        "旧值无效 ⇒ 覆盖"
    );

    // 解锁：n 及之前 + 下一关（`0x418820(n+1)`）
    assert_eq!(
        cc::CampaignSave::from_text(None).highest_playable_level(),
        0,
        "没有存档也能玩第 0 关"
    );
    assert_eq!(
        cc::CampaignSave::from_text(Some("level_0")).highest_playable_level(),
        1
    );
    assert_eq!(
        cc::CampaignSave::from_text(Some("level_19")).highest_playable_level(),
        20
    );
    assert_eq!(
        cc::CampaignSave::from_text(Some("level_20")).highest_playable_level(),
        20,
        "全部通关后钳在最后一关"
    );
    let save = cc::CampaignSave::from_text(Some("level_2"));
    assert!(
        save.is_level_unlocked(0) && save.is_level_unlocked(3),
        "0..=n+1 解锁"
    );
    assert!(!save.is_level_unlocked(4), "n+2 还锁着");
    assert!(!save.is_level_unlocked(21), "越界永远不解锁");
}

/// `CampaignProgress`（`0x418170` 的读路径）：坏档一律当 0。
#[test]
fn campaign_progress_resets_to_zero_on_a_bad_file() {
    assert_eq!(
        cc::CampaignProgress::from_save_text("level_20").unlocked_through,
        20
    );
    assert_eq!(
        cc::CampaignProgress::from_save_text("level_20\n").unlocked_through,
        0,
        "尾换行 ⇒ 进度归零（附录 A 的用户实测）"
    );
    assert_eq!(cc::CampaignProgress::from_save_text("").unlocked_through, 0);
    // 既有行为不许变（tests/misc_modules.rs 也在断言这条）
    let p = cc::CampaignProgress {
        unlocked_through: 4,
    };
    assert!(p.is_level_visible(1) && p.is_level_visible(5));
    assert!(!p.is_level_visible(6) && !p.is_level_visible(0) && !p.is_level_visible(22));
}

// ===========================================================================
// 5. 客户端 `client/src/app/campaign.gd`：**表已经搬回 Rust**（不再有第二份）
// ===========================================================================
//
// 历史：这条位置原来是 `client_campaign_table_matches_the_kernel_table`，它把
// `campaign.gd` 里那份 `const LEVELS`（21 条）逐字段、逐字节地与内核 `LEVEL_INFO` 比对。
// 那说明**同一张表存在两份**：改内核不改 `.gd`，界面就显示旧数据；只靠一条测试拉着。
//
// T67（`docs/06` §115 / 台账 `fixtures/controller_migration3_ledger.tsv` C02/C04/C05/C07）
// 把表搬走了：`campaign.gd` 现在只调 `GwCampaignControl`（`src/_scaffold/campaign_control_class.rs`），
// 而那个类**没有自己的数据** —— 它逐字段读 `graphwar_campaign_control::LEVEL_INFO`。
// ⇒ "客户端表 == 内核表"这件事从"两条拷贝互相钉住"变成"**只有一份**，结构上不可能漂移"。
//
// 因此本条断言改成盯住**搬走这件事本身**（防止有人再把表抄回来）：
//   ① 数据不在 `.gd` 里了：没有 `const LEVELS`，21 条描述的原文一条都不出现；
//   ② 数据从 Rust 来：`.gd` 里出现了 `GwCampaignControl.level(` / `.level_count()`；
//   ③ 关卡数不再写死：`.gd` 里没有 `21` 这个字面量的 `LEVEL_COUNT`。
// 逐字段的正确性由本文件 §1/§2/§3（内核 vs 文档 §7.1）与客户端 headless 自检
// （`client/tools/selftest.gd` 对 `GwCampaign.level(i)` 的读数断言）负责。
//
// ⚠️ 按 `AGENTS.md` §0 第 2 条：这些断言是**回归保护**，不是复原证据。

fn campaign_gd_source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("client/src/app/campaign.gd");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {}：{e}", path.display()))
}

#[test]
fn client_campaign_table_is_gone_and_reads_the_kernel() {
    let src = campaign_gd_source();

    // ① 那 21 条数据（标题 + 描述）不许再出现在客户端。
    // 只看**声明**（`const LEVELS := [`），不看注释里对历史的引用。
    assert!(
        !src
            .lines()
            .any(|l| l.trim_start().starts_with("const LEVELS")),
        "client/src/app/campaign.gd 里不许再有 `const LEVELS` 声明（表已搬进 Rust，T67）"
    );
    for lv in cc::LEVEL_INFO.iter() {
        assert!(
            !src.contains(lv.description),
            "campaign.gd 里还留着 level_{} 的描述原文 —— 那是内核 `LEVEL_INFO` 的第二份拷贝",
            lv.index
        );
        assert!(
            !src.contains(&format!("\"title\": \"{}\"", lv.title)),
            "campaign.gd 里还留着 level_{} 的标题条目",
            lv.index
        );
    }
    assert!(
        !src.contains("PLACEHOLDER"),
        "client/src/app/campaign.gd 里不许再有 PLACEHOLDER（验收：grep -c == 0）"
    );

    // ② 数据必须从内核类来。
    for needle in [
        "GwCampaignControl.level(",
        "GwCampaignControl.level_count()",
    ] {
        assert!(
            src.contains(needle),
            "campaign.gd 必须经 `{needle}` 向 Rust 取关卡数据（T67 的接线）"
        );
    }

    // ③ 关卡数不许写死：定义必须是一行 `static func level_count()`，里面问内核。
    //    （`const` 不行：GDScript 的 `const` 要求常量表达式，跨语言调 GDExtension 静态方法
    //    会让引用方在**解析期**报 "Could not resolve external class member"。）
    assert!(
        !src
            .lines()
            .any(|l| l.trim_start().starts_with("const LEVEL_COUNT")),
        "campaign.gd 里不许再写死 `const LEVEL_COUNT`（要问内核）"
    );
    let count_fn = src
        .split("static func level_count()")
        .nth(1)
        .expect("campaign.gd 里必须有 `static func level_count()`");
    let body: String = count_fn.lines().take(3).collect::<Vec<_>>().join("\n");
    assert!(
        body.contains("GwCampaignControl.level_count()"),
        "`level_count()` 必须来自内核（`LEVEL_INFO` 的长度），实际是：{body}"
    );

    // ④ 视图层那两样（自造种子 + 原版布局算法）必须留着。
    for needle in ["static func level_seed(", "static func layout_buttons("] {
        assert!(
            src.contains(needle),
            "campaign.gd 应当保留 `{needle}`（不属于内核：前者自造、后者是原版视图 .gd）"
        );
    }
}

// ===========================================================================
// 4. `campaign.save` 那一簇：**两份拷贝互相钉住**（2026-11-26 新增）
// ===========================================================================
//
// 背景（`docs/02-module-map.md` §17.4）：`level_info()`(`0x4ad380`) 与
// `campaign.save` 簇(`0x417ef0`/`0x418170`/`0x4183b0`) **没有 `panic::Location` 锚**，
// 所以当初暂时挂在 `graphwar_level_manager.rs`；`graphwar_campaign_control.rs` 落地后
// 应当搬走旧副本，但那次任务只允许改自己那份 ⇒ 两份并存。
//
// `src/graphwar_campaign_control.rs` 的文件头**原来**就写着"两份由
// `save_cluster_matches_level_manager_copy` 逐字段钉住" —— 但全库**零命中**：
// 这个测试从来没有被写出来（`docs/02` §17.4「判不清」第 6 条记的就是这件事）。
// 本条把它补上：两份实现必须在**整个输入空间**上给出同一个答案，
// 谁先漂、漂在哪一类输入上，断言会点名。

/// 一串精心挑的 `campaign.save` 内容（合法 / 边界 / 脏数据）——两份实现都要在这些输入上一致。
const SAVE_SAMPLES: &[&str] = &[
    "",                 // 空文件
    "level_0",
    "level_1",
    "level_9",
    "level_10",
    "level_20",
    "level_4294967295", // u32::MAX，`refresh_index` 会夹到 20
    "level_4294967296", // 溢出 u32 ⇒ 解析失败
    "level_20\n",       // 尾换行：最后一段是 "20\n"，`parse::<u32>` 失败（原版不宽容）
    " level_20",        // 前导空格同样失败
    "level_",           // 缺数字
    "level_007",        // 前导零，仍可解析
    "level_-1",         // 负号：u32 解析失败
    "junk",             // 完全没有 `_`
    "anything_7",       // 最后一个 `_` 之后是 7（原版取最后一段）
    "a_b_c_12",         // 多段
];

fn cc_action(existing: Option<&str>, new_text: &str) -> bool {
    matches!(
        cc::save_action(existing, new_text),
        cc::SaveAction::Write
    )
}

fn lm_action(existing: Option<&str>, new_text: &str) -> bool {
    matches!(
        lm::save_action(existing, new_text),
        lm::SaveAction::Write
    )
}

#[test]
fn save_cluster_matches_level_manager_copy() {
    // ① 解析：cc::parse_level_key ≡ lm::parse_level_key
    for s in SAVE_SAMPLES {
        assert_eq!(
            cc::parse_level_key(s),
            lm::parse_level_key(s),
            "{s:?}：parse_level_key"
        );
    }

    // ② 生成：cc::encode_save ≡ lm::encode_save（0..=20 的合法档 + 两个越界值）
    for level in (0u32..=20).chain([u32::MAX - 1, u32::MAX]) {
        assert_eq!(
            cc::encode_save(level),
            lm::encode_save(level),
            "level={level}：encode_save"
        );
    }

    // ③ 读：cc 的 parse 失败语义 ≡ lm::decode_save 的"失败读 0"
    for s in SAVE_SAMPLES {
        assert_eq!(
            cc::parse_level_key(s).unwrap_or(0),
            lm::decode_save(Some(s)),
            "{s:?}：decode_save"
        );
    }
    assert_eq!(lm::decode_save(None), 0, "文件打不开 ⇒ 进度 0");

    // ④ 写不写：**全组合**（17 个旧值含 None） × 17 个新串 = 8 个方向
    for old in std::iter::once(None).chain(SAVE_SAMPLES.iter().copied().map(Some)) {
        for new_text in SAVE_SAMPLES {
            assert_eq!(
                cc_action(old, new_text),
                lm_action(old, new_text),
                "old={old:?} new={new_text:?}：save_action 的 Write/Skip"
            );
        }
    }

    // ⑤ 该写什么：save_level 的返回值（`None` = 不写）逐格一致
    for old in std::iter::once(None).chain(SAVE_SAMPLES.iter().copied().map(Some)) {
        for level in [0u32, 1, 9, 10, 20, u32::MAX] {
            assert_eq!(
                cc::save_level(old, level),
                lm::save_level(old, level),
                "old={old:?} level={level}：save_level"
            );
        }
    }

    // ⑥ 刷新下标：cc::refresh_index ≡ lm::refresh_index，且 20 之后被夹住
    for new_level in (0u32..=20).chain([u32::MAX - 1, u32::MAX]) {
        assert_eq!(
            cc::refresh_index(new_level),
            lm::refresh_index(new_level),
            "new_level={new_level}：refresh_index"
        );
    }
    assert_eq!(cc::refresh_index(20), 20, "20 之后再 +1 被夹到 20");
    assert_eq!(cc::refresh_index(0), 1, "打完第 1 关（level_0）刷新到下标 1");
}
