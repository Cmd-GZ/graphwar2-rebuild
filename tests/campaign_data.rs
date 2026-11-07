//! 战役 21 关参数表 + `campaign.save` 存档语义的验收。
//!
//! 覆盖 `dispatch/04-campaign-data.md` 的三件事：
//! 1. `src/graphwar_campaign_control.rs` 的 [`LEVEL_INFO`] 与
//!    `docs/archive/118-campaign-data.md` **§7.1** 的 21 行逐字段相等；
//! 2. 存档语义（§3.1 + 附录 A）：`level_<n>`、0 基、无尾换行、只增不减、启动不创建；
//! 3. `client/src/app/campaign.gd` 的表与内核同一份（关卡名/描述**逐字节**）。
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
// 5. 客户端 `client/src/app/campaign.gd` 与内核同一份表
// ===========================================================================

fn campaign_gd_source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("client/src/app/campaign.gd");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {}：{e}", path.display()))
}

/// 从一段 GD 字典文本里取整数：`"axis": 3,` → 3。
fn gd_int(s: &str, key: &str) -> i64 {
    let pat = format!("\"{key}\":");
    let p = s
        .find(&pat)
        .unwrap_or_else(|| panic!("campaign.gd 里没有 \"{key}\"：{s}"));
    let rest = &s[p + pat.len()..];
    let end = rest.find(|c| c == ',' || c == '}').unwrap_or(rest.len());
    rest[..end]
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("\"{key}\" 不是整数：{:?}", &rest[..end]))
}

/// 从一段 GD 字典文本里取字符串（描述里没有引号/转义，直接找下一个 `"`）。
fn gd_str(s: &str, key: &str) -> String {
    let pat = format!("\"{key}\": \"");
    let p = s
        .find(&pat)
        .unwrap_or_else(|| panic!("campaign.gd 里没有 \"{key}\"：{s}"));
    let rest = &s[p + pat.len()..];
    let end = rest.find('"').expect("字符串没有收尾引号");
    rest[..end].to_string()
}

/// 客户端表逐项等于内核表（标题/描述**逐字节**），且 `PLACEHOLDER` 已经清掉。
#[test]
fn client_campaign_table_matches_the_kernel_table() {
    let src = campaign_gd_source();
    assert!(
        !src.contains("PLACEHOLDER"),
        "client/src/app/campaign.gd 里不许再有 PLACEHOLDER（验收：grep -c == 0）"
    );

    let lines: Vec<&str> = src.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.contains("const LEVELS := ["))
        .expect("找不到 const LEVELS := [");

    // 每条记录是"标题行 + 描述行"两行（写入时就是这个形状）
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let head = lines[i].trim_start();
        if head.starts_with("{\"title\"") {
            let tail = lines.get(i + 1).copied().unwrap_or("");
            pairs.push((lines[i].to_string(), tail.to_string()));
            i += 2;
            continue;
        }
        if head == "]" {
            break;
        }
        i += 1;
    }
    assert_eq!(pairs.len(), 21, "客户端表必须是 21 条");

    for (idx, (head, tail)) in pairs.iter().enumerate() {
        let joined = format!("{head}\n{tail}");
        let lv = &cc::LEVEL_INFO[idx];
        let ctx = format!("campaign.gd 第 {idx} 条");

        assert_eq!(
            gd_str(head, "title"),
            lv.title,
            "{ctx}：标题必须与内核逐字节一致"
        );
        assert_eq!(
            gd_str(tail, "description"),
            lv.description,
            "{ctx}：描述必须与内核逐字节一致"
        );
        assert_eq!(gd_int(head, "axis") as u8, lv.modes.axis(), "{ctx}：axis");
        assert_eq!(
            gd_int(head, "function") as u8,
            lv.modes.function(),
            "{ctx}：function"
        );
        assert_eq!(gd_int(head, "turn") as u8, lv.modes.turn(), "{ctx}：turn");
        assert_eq!(gd_int(head, "time") as u8, lv.modes.time(), "{ctx}：time");
        assert_eq!(
            gd_int(head, "soldiers") as u32,
            lv.soldiers,
            "{ctx}：soldiers"
        );
        assert_eq!(gd_int(head, "targets") as u32, lv.targets, "{ctx}：targets");
        assert_eq!(
            gd_int(head, "bot_tier"),
            lv.bot.map(|b| b.tier as i64).unwrap_or(-1),
            "{ctx}：bot_tier（-1 = 无对手）"
        );
        assert_eq!(
            gd_int(head, "turn_limit") as u32,
            lv.turn_limit.unwrap_or(0),
            "{ctx}：turn_limit（0 = 无上限）"
        );
        assert_eq!(
            gd_int(head, "obstacle_min") as u32,
            lv.min_count,
            "{ctx}：obstacle_min"
        );
        let want_gd_obstacle = match lv.obstacles {
            cc::ObstacleMode::None => "Obstacles.NONE",
            cc::ObstacleMode::Center => "Obstacles.CENTER",
            cc::ObstacleMode::Scatter => "Obstacles.SCATTER",
        };
        assert!(
            joined.contains(&format!("\"obstacles\": {want_gd_obstacle}")),
            "{ctx}：障碍模式必须是 {want_gd_obstacle}"
        );
    }
}
