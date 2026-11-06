//! `graphwar_level_manager.rs` 的复原验收。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//! 每一条右侧的真值都能指到反汇编地址或 `.rodata` 字节；出处逐条写在断言上方。
//!
//! 表的口径：`docs/10-campaign.md` §4「21 关全参数表（权威表）」（旧编号 §7.1）。
//! 那张表的 `S` 列是开场臂里的立即数、`C` 列是闭包预设、`R` 列是 385 份回放的运行时事件，
//! 三路互不依赖，21/21 一致。本文件把它逐行抄成断言。

use gw2_rebuild::graphwar_level_manager::*;

/// 权威表的一行（不含模式，模式单独用 `imm32` 检查）。
struct DocRow {
    level: usize,
    /// 障碍预设：`None` / `Some(None)` = 无障碍，`Some(Some(min_count))` = 散布，`Centre` = 中心单障碍
    obstacle: Obstacle,
    soldiers: u32,
    targets: u32,
    bot: Option<u32>,
    turn_limit: Option<u32>,
}

enum Obstacle {
    NoObstacle,
    Centre,
    Scatter(i32),
}

/// `docs/10-campaign.md` §4 的 21 行，逐行照抄。
const DOC_TABLE: [DocRow; LEVEL_COUNT] = [
    // 关 0 Level 0：无障碍〔C 0x95a350/0x95a378；R 0〕 1 兵〔edx=1〕 1 靶 无对手 无上限
    DocRow { level: 0, obstacle: Obstacle::NoObstacle, soldiers: 1, targets: 1, bot: None, turn_limit: None },
    // 关 1 Level 1：无障碍；1/1；无对手；5〔S (1,5)〕
    DocRow { level: 1, obstacle: Obstacle::NoObstacle, soldiers: 1, targets: 1, bot: None, turn_limit: Some(5) },
    // 关 2 Level 2：中心单障碍〔C 0x49f1c0〕；1/1；5
    DocRow { level: 2, obstacle: Obstacle::Centre, soldiers: 1, targets: 1, bot: None, turn_limit: Some(5) },
    // 关 3 Level 3：中心单障碍〔C 0x49f370；R 半径 4.81/6.75〕；**2 兵**／**3 靶**；10
    DocRow { level: 3, obstacle: Obstacle::Centre, soldiers: 2, targets: 3, bot: None, turn_limit: Some(10) },
    // 关 4 Level 4：散布 min_count=15〔C 0x49fe80；R 15 个〕；1/1；10
    DocRow { level: 4, obstacle: Obstacle::Scatter(15), soldiers: 1, targets: 1, bot: None, turn_limit: Some(10) },
    // 关 5 Level 5：散布 min_count=5〔C 0x4a12f0〕；2 兵 0 靶；bot_0 难度 3；无上限
    DocRow { level: 5, obstacle: Obstacle::Scatter(5), soldiers: 2, targets: 0, bot: Some(3), turn_limit: None },
    // 关 6 Level 6：散布 10；1 兵 2 靶；6
    DocRow { level: 6, obstacle: Obstacle::Scatter(10), soldiers: 1, targets: 2, bot: None, turn_limit: Some(6) },
    // 关 7 Level 7：散布 6；1/1；**1**（"first try"）
    DocRow { level: 7, obstacle: Obstacle::Scatter(6), soldiers: 1, targets: 1, bot: None, turn_limit: Some(1) },
    // 关 8 Level 8：散布 10；1 兵 3 靶；6
    DocRow { level: 8, obstacle: Obstacle::Scatter(10), soldiers: 1, targets: 3, bot: None, turn_limit: Some(6) },
    // 关 9 Level 9：散布 5；3 兵 0 靶；bot_2 难度 25；无上限
    DocRow { level: 9, obstacle: Obstacle::Scatter(5), soldiers: 3, targets: 0, bot: Some(25), turn_limit: None },
    // 关 10 Level 10：散布 10；1 兵 2 靶；6；axis 2
    DocRow { level: 10, obstacle: Obstacle::Scatter(10), soldiers: 1, targets: 2, bot: None, turn_limit: Some(6) },
    // 关 11 Level 11：散布 5；3 兵 0 靶；bot_3 难度 50；无上限；axis 2 + turn 1
    DocRow { level: 11, obstacle: Obstacle::Scatter(5), soldiers: 3, targets: 0, bot: Some(50), turn_limit: None },
    // 关 12 Level 12：中心单障碍〔C 0x4a5e80；R 半径 5.69〕；1/1；5；axis 1
    DocRow { level: 12, obstacle: Obstacle::Centre, soldiers: 1, targets: 1, bot: None, turn_limit: Some(5) },
    // 关 13 Level 13：散布 10；1 兵 2 靶（"two targets"）；5；axis 1
    DocRow { level: 13, obstacle: Obstacle::Scatter(10), soldiers: 1, targets: 2, bot: None, turn_limit: Some(5) },
    // 关 14 Level 14：散布 5；4 兵 0 靶；bot_3 难度 100；无上限；turn 1 + axis 1
    DocRow { level: 14, obstacle: Obstacle::Scatter(5), soldiers: 4, targets: 0, bot: Some(100), turn_limit: None },
    // 关 15 Level 15：散布 10；5 兵 0 靶；bot_4 难度 250；无上限；axis 0
    DocRow { level: 15, obstacle: Obstacle::Scatter(10), soldiers: 5, targets: 0, bot: Some(250), turn_limit: None },
    // 关 16 Level 16：无障碍〔C 0x95d358〕；1/1；无上限；func 1
    DocRow { level: 16, obstacle: Obstacle::NoObstacle, soldiers: 1, targets: 1, bot: None, turn_limit: None },
    // 关 17 Level 17：散布 10；1 兵 2 靶（"both targets"）；6；func 1
    DocRow { level: 17, obstacle: Obstacle::Scatter(10), soldiers: 1, targets: 2, bot: None, turn_limit: Some(6) },
    // 关 18 Level 18：散布 10；3 兵 0 靶；bot_3 难度 100；无上限；func 1
    DocRow { level: 18, obstacle: Obstacle::Scatter(10), soldiers: 3, targets: 0, bot: Some(100), turn_limit: None },
    // 关 19 Level 19：散布 10；4 兵 0 靶；bot_3 难度 100；无上限；func 1 + axis 1
    DocRow { level: 19, obstacle: Obstacle::Scatter(10), soldiers: 4, targets: 0, bot: Some(100), turn_limit: None },
    // 关 20 Level 20：散布 15；5 兵 0 靶；bot_4 难度 250；无上限；func 1 + axis 0
    DocRow { level: 20, obstacle: Obstacle::Scatter(15), soldiers: 5, targets: 0, bot: Some(250), turn_limit: None },
];

/// 每关 `mov dword ptr [rsp + 0x58], <imm32>` 的立即数（21 条，按关卡顺序）。
///
/// 地址：`0x4ad4dd` `0x4ad62c` `0x4ad77b` `0x4ad8f3` `0x4ada40` `0x4adb90` `0x4adcf2`
/// `0x4ade4c` `0x4adf9d` `0x4ae0e2` `0x4ae24f` `0x4ae3cb` `0x4ae518` `0x4ae668`
/// `0x4ae7b8` `0x4ae913` `0x4aea5e` `0x4aebd2` `0x4aed22` `0x4aee84` `0x4aefdf`。
const MODE_IMM32: [u32; LEVEL_COUNT] = [
    0x0503_0000, 0x0503_0000, 0x0503_0000, 0x0503_0000, 0x0503_0000, // 0-4
    0x0503_0000, 0x0503_0000, 0x0503_0000, 0x0503_0000, 0x0503_0000, // 5-9
    0x0502_0000, // 10  "finer grid lines are gone" → EveryFive
    0x0502_0100, // 11  "firing simultaneously"   → SimultaneousTurns
    0x0501_0000, // 12  "Most grid lines are gone" → OnlyMain
    0x0501_0000, // 13
    0x0501_0100, // 14  同时开火
    0x0500_0000, // 15  "The main axes are gone"   → NoAxis
    0x0503_0001, // 16  微分方程
    0x0503_0001, // 17
    0x0503_0001, // 18
    0x0501_0001, // 19  微分方程 + OnlyMain
    0x0500_0001, // 20  微分方程 + NoAxis
];

/// 每关描述在 `.rodata` 的长度立即数（`mov qword ptr [rsp + 0x38], <len>`）。
const DESC_LEN: [usize; LEVEL_COUNT] = [
    40, 153, 129, 94, 45, 42, 60, 38, 49, 32, 80, 111, 47, 37, 46, 58, 251, 88, 46, 67, 64,
];

/// 21 关的参数逐行与 `docs/10-campaign.md` §4 一致。
#[test]
fn all_twenty_one_levels_match_the_authoritative_table() {
    assert_eq!(DOC_TABLE.len(), LEVEL_COUNT);
    for row in &DOC_TABLE {
        let setup = level_setup(row.level).unwrap_or_else(|| panic!("关卡 {} 没有参数", row.level));
        match row.obstacle {
            Obstacle::NoObstacle => assert_eq!(
                setup.preset,
                ObstaclePreset::None,
                "关 {} 应当无障碍",
                row.level
            ),
            Obstacle::Centre => assert_eq!(
                setup.preset,
                ObstaclePreset::CentreSingle,
                "关 {} 应当是中心单障碍",
                row.level
            ),
            Obstacle::Scatter(min_count) => assert_eq!(
                setup.preset,
                ObstaclePreset::Scatter { min_count },
                "关 {} 的散布 min_count",
                row.level
            ),
        }
        assert_eq!(setup.soldiers, row.soldiers, "关 {} 士兵数", row.level);
        assert_eq!(setup.targets, row.targets, "关 {} 目标数", row.level);
        assert_eq!(setup.bot, row.bot, "关 {} Bot 难度", row.level);
        assert_eq!(setup.turn_limit, row.turn_limit, "关 {} 回合上限", row.level);
    }
}

/// 模式四元组按位拆出来必须还原成那 21 条 `imm32`。
///
/// 字节序：`+0x38 = func`、`+0x39 = turn`、`+0x3a = axis`、`+0x3b = time`。
#[test]
fn mode_bytes_recombine_to_the_recorded_imm32() {
    for (i, imm) in MODE_IMM32.iter().enumerate() {
        let m = level_info(i).unwrap().modes;
        let got = (m.function as u32) | ((m.turns as u32) << 8) | ((m.axis as u32) << 16)
            | ((m.time_byte as u32) << 24);
        assert_eq!(got, *imm, "关 {i} 的模式四元组");
        // `+0x3b` 21/21 恒为 5（判"时间档"为推断，但字节本身是硬值）
        assert_eq!(m.time_byte, TIME_BYTE_ALL_LEVELS, "关 {i}");
    }
    // 三条"与描述自洽"的强约束（docs/10 §4 的自校验）
    let diff_eq: Vec<usize> = (0..LEVEL_COUNT)
        .filter(|i| level_info(*i).unwrap().modes.function == FunctionMode::DiffEqFunction)
        .collect();
    assert_eq!(diff_eq, vec![16, 17, 18, 19, 20], "func=1 必须恰好是第 16-20 关");
    let simultaneous: Vec<usize> = (0..LEVEL_COUNT)
        .filter(|i| level_info(*i).unwrap().modes.turns == TurnMode::SimultaneousTurns)
        .collect();
    assert_eq!(simultaneous, vec![11, 14], "turn=1 必须恰好是第 11/14 关");
    // axis 单调不增：网格只会越来越稀，从不回退
    let axes: Vec<u8> = (0..LEVEL_COUNT).map(|i| level_info(i).unwrap().modes.axis as u8).collect();
    assert!(
        axes.windows(2).all(|w| w[0] >= w[1] || (w[0] < 3 && w[1] == 3)),
        "axis 序列不该出现「变密」：{axes:?}"
    );
}

/// 键、标题、描述长度。
#[test]
fn keys_titles_and_description_lengths_are_exact() {
    for (i, len) in DESC_LEN.iter().enumerate() {
        let info = level_info(i).unwrap();
        assert_eq!(info.index as usize, i);
        assert_eq!(info.key, format!("level_{i}"));
        assert_eq!(info.title, format!("Level {i}"));
        assert_eq!(info.description.len(), *len, "关 {i} 描述长度（分支里的立即数）");
        assert_eq!(info.description_len, *len, "关 {i} 记录的立即数");
        assert_eq!(level_info(i).map(|l| l.key), Some(info.key));
        // 反查
        assert_eq!(level_by_key(info.key).map(|l| l.index), Some(i as u8));
    }
    assert!(level_info(LEVEL_COUNT).is_none(), "没有第 22 关");
    assert!(level_setup(LEVEL_COUNT).is_none());
    assert!(level_by_key("level_21").is_none());
    assert!(level_by_key("Level 3").is_none(), "反查只认 key，不认标题");
}

/// 第 16 关原文就是 `writting`（双 t）。这是原版自带的拼写，**不许"修"**。
#[test]
fn level_16_keeps_the_original_typo() {
    let d = level_info(16).unwrap().description;
    assert!(d.starts_with("You are now writting a differential equation"), "{d}");
    assert!(!d.contains("writing"), "拼写不许被改对：{d}");
}

/// 三关"无障碍"、三关"中心单障碍"、十五关"散布"—— 与 `docs/11` §1 的"只有两个生产者"一致。
#[test]
fn obstacle_presets_have_the_same_shape_as_the_generators() {
    let none: Vec<usize> = (0..LEVEL_COUNT)
        .filter(|i| level_setup(*i).unwrap().preset == ObstaclePreset::None)
        .collect();
    assert_eq!(none, vec![0, 1, 16], "无障碍的三关（docs/10 §4 的 C 列）");
    let centre: Vec<usize> = (0..LEVEL_COUNT)
        .filter(|i| level_setup(*i).unwrap().preset == ObstaclePreset::CentreSingle)
        .collect();
    assert_eq!(centre, vec![2, 3, 12], "中心单障碍的三关");
    // 散布式的 min_count 只有 {5, 6, 10, 15} 四种
    let mut mins: Vec<i32> = (0..LEVEL_COUNT)
        .filter_map(|i| match level_setup(i).unwrap().preset {
            ObstaclePreset::Scatter { min_count } => Some(min_count),
            _ => None,
        })
        .collect();
    mins.sort_unstable();
    mins.dedup();
    assert_eq!(mins, vec![5, 6, 10, 15], "四个散布形参的 esi 立即数");
}

/// 无障碍关生成出空表；中心单障碍关恰好 1 个、位置在原点上。
#[test]
fn generate_obstacles_follows_the_preset() {
    // 固定的假随机流：只要确定，不需要"正确"
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    assert!(generate_obstacles(0, &mut next).unwrap().is_empty(), "关 0 无障碍");
    assert!(generate_obstacles(16, &mut next).unwrap().is_empty(), "关 16 无障碍");
    let one = generate_obstacles(2, &mut next).unwrap();
    assert_eq!(one.len(), 1, "中心单障碍恰好 1 颗");
    assert_eq!(one[0].x.to_bits(), 0.0f64.to_bits(), "位置恒在原点");
    assert_eq!(one[0].y.to_bits(), 0.0f64.to_bits());
    assert!((3.0..8.0).contains(&one[0].r), "半径 u*5+3 ∈ [3,8)，实际 {}", one[0].r);
    // 散布式的个数是 min_count 的下界（下界不是定值）
    for (level, min_count) in [(4usize, 15i32), (5, 5), (6, 10), (7, 6)] {
        let v = generate_obstacles(level, &mut next).unwrap();
        assert!(v.len() as i32 >= min_count, "关 {level} 的障碍数必须 >= {min_count}");
        assert_eq!(
            level_setup(level).unwrap().preset,
            ObstaclePreset::Scatter { min_count }
        );
    }
    assert!(generate_obstacles(LEVEL_COUNT, &mut next).is_none());
}

// ---------------------------------------------------------------------------
// `campaign.save`
// ---------------------------------------------------------------------------

/// 存档路径就是那个 `.rodata` 字面量（`0x1bfe28`，长度 0x14 = 20）。
#[test]
fn save_path_is_the_original_literal() {
    assert_eq!(CAMPAIGN_SAVE_PATH, "user://campaign.save");
    assert_eq!(CAMPAIGN_SAVE_PATH.len(), 20);
}

/// 往返 + "无尾换行"。
///
/// 真值：写进去的串由 `store_string` 原样落盘（不加换行），
/// 用户实测的干净文件是 **8 字节的 `level_0`**（`docs/10` 附录 A）。
#[test]
fn campaign_save_round_trip_without_trailing_newline() {
    for level in 0..=MAX_LEVEL_INDEX {
        let text = encode_save(level);
        assert_eq!(text, format!("level_{level}"));
        assert!(!text.ends_with('\n'), "8 字节 / 7 字节，末尾不许有换行");
        assert_eq!(text.len(), 7 + usize::from(level >= 10));
        // 落盘 → 读回
        assert_eq!(decode_save(Some(&text)), level);
        // 同一份内容的判定：不该再写
        assert_eq!(save_level(Some(&text), level), None, "等值不重写");
    }
}

/// `echo 'level_20' > campaign.save` 会多一个 `\n`（9 字节）⇒ 进度归零。
///
/// 用户实测（`docs/10` 附录 A）+ 代码侧对应：`str::parse::<u32>()` 不跳过空白。
#[test]
fn a_trailing_newline_reads_back_as_zero() {
    let with_newline = "level_20\n";
    assert_eq!(with_newline.len(), 9);
    assert_eq!(decode_save(Some(with_newline)), 0, "必须得 0，不是 20");
    assert_eq!(parse_level_key(with_newline), None);
    // 空格、CR、以及任意前缀垃圾都不宽容
    assert_eq!(decode_save(Some("level_20 ")), 0);
    assert_eq!(decode_save(Some("level_20\r\n")), 0);
    assert_eq!(decode_save(Some("level_20\t")), 0);
    // 而干净的那一行是对的（同一函数的正例，防止"恒返回 0 也算过"）
    assert_eq!(decode_save(Some("level_20")), 20);
    assert_eq!(decode_save(Some("level_0")), 0);
}

/// 只增不减：新值 > 旧值才写（`0x4185b3` 的 `cmp ebp, edx; jbe <不写>`）。
#[test]
fn campaign_save_is_monotonic() {
    let cases: [(Option<&str>, u32, Option<&str>); 8] = [
        (None, 0, Some("level_0")),                 // 文件不存在 ⇒ 写
        (Some("level_0"), 1, Some("level_1")),      // 前进 ⇒ 写
        (Some("level_0"), 0, None),                 // 等值 ⇒ 不写
        (Some("level_5"), 3, None),                 // 后退 ⇒ 不写
        (Some("level_20"), 20, None),               // 顶格 ⇒ 不写
        (Some(""), 4, Some("level_4")),             // 空文件 ⇒ 写
        (Some("level_9\n"), 2, Some("level_2")),    // 旧值坏（带换行）⇒ 写
        (Some("garbage"), 0, Some("level_0")),      // 旧值坏 ⇒ 写
    ];
    for (existing, level, expect) in cases {
        assert_eq!(
            save_level(existing, level).as_deref(),
            expect,
            "existing={existing:?} level={level}"
        );
    }
}

/// 解锁：`level_<n>` 表示"已解锁到 n"（0 基，含 n）。
///
/// ⚠️ 语义部分判**推断**（`docs/10` §5：缺 `0x418820` 的属性落点）；
/// 这里断言的是那条推断的实现形状，不是二进制证据。
#[test]
fn unlock_is_inclusive() {
    assert!(is_unlocked(0, 0));
    assert!(!is_unlocked(0, 1));
    assert!(is_unlocked(7, 0) && is_unlocked(7, 7));
    assert!(!is_unlocked(7, 8));
    assert!(is_unlocked(MAX_LEVEL_INDEX, LEVEL_COUNT - 1));
    assert!(!is_unlocked(MAX_LEVEL_INDEX, LEVEL_COUNT));
}

/// 写完要刷新的下标是"新值 + 1"，并且被 20 钳住（`cmp ebp, 0x14`）。
#[test]
fn refresh_uses_new_value_plus_one() {
    assert_eq!(refresh_index(0), 1);
    assert_eq!(refresh_index(19), MAX_LEVEL_INDEX);
    assert_eq!(refresh_index(MAX_LEVEL_INDEX), MAX_LEVEL_INDEX);
}
