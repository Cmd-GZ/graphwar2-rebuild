//! `graphwar_setup_control.rs` 的复原验收。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//! 每一条右侧的真值都能指到反汇编地址或 `.rodata` 字节 / `.data.rel.ro` 重定位；
//! 出处逐条写在断言上方。
//!
//! 地址口径：本文件的 `0x…` 一律 **ELF 虚拟地址**；REA/Ghidra = 本值 + `0x100000`。

use gw2_rebuild::graphwar_level_manager::{self, AxisMode, FunctionMode, TurnMode};
use gw2_rebuild::graphwar_root_node::Screen;
use gw2_rebuild::graphwar_setup_control::*;

/// `fixtures/module_funcs.tsv` 里本模块的行（`tools/module_map.py` 生成）。
fn fixture_funcs() -> Vec<u64> {
    include_str!("../fixtures/module_funcs.tsv")
        .lines()
        .skip(1)
        .filter_map(|l| {
            let (m, a) = l.split_once('\t')?;
            (m == "graphwar_setup_control.rs").then(|| u64::from_str_radix(a.trim_start_matches("0x"), 16).unwrap())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 分母：37 个函数入口 / 70 个 Location 锚 / 21 个压平后可归属
// ---------------------------------------------------------------------------

/// 真值 = `fixtures/module_funcs.tsv`（由 `tools/module_map.py` 从 `.rela.dyn` +
/// capstone 归属算出）。`MODULE_FUNCS` 必须与它**逐条相同、同序**。
#[test]
fn module_funcs_match_the_fixture_row_for_row() {
    let fixture = fixture_funcs();
    assert_eq!(fixture.len(), 37, "fixture 里本模块应有 37 个函数入口");
    assert_eq!(
        MODULE_FUNCS.to_vec(),
        fixture,
        "MODULE_FUNCS 必须与 fixtures/module_funcs.tsv 的 37 条逐条相同（含顺序）"
    );
    assert_eq!(MODULE_FUNCS.len(), 37);
}

/// 70 个 `Location` 结构体 / 压平后 21 个可归属函数：两个数字口径不同，**都要钉住**。
///
/// 21 来自 `docs/coverage-matrix.tsv`（`orig_funcs`），37 来自同一份 fixture；
/// 差 16 = 8 个 `contested` 被压给别的模块 + 26 个只在本模块（矩阵口径下不计）。
#[test]
fn anchor_and_attribution_counts_are_pinned() {
    assert_eq!(LOCATION_ANCHOR_COUNT, 70, "docs/coverage-matrix.tsv 的 anchors 列");
    assert_eq!(
        ATTRIBUTED_FUNCS_AFTER_FLATTENING, 21,
        "docs/coverage-matrix.tsv 的 orig_funcs 列"
    );
}

/// 逐个命名地址必须在 37 条表里，且表本身严格升序、无重复。
#[test]
fn named_functions_are_inside_the_list_and_sorted() {
    assert!(MODULE_FUNCS.windows(2).all(|w| w[0] < w[1]), "MODULE_FUNCS 必须严格升序且无重复");
    for f in [
        FN_SETUP_WIRING,
        FN_TEAM_LIST_REBUILD,
        FN_PLAYER_ROW,
        FN_SOLDIER_ICONS,
        FN_PLAYER_NAME,
        FN_RANK_ICON,
        FN_MODE_PANEL_SYNC,
    ] {
        assert!(MODULE_FUNCS.contains(&f), "0x{f:x} 不在 37 条表里");
    }
    for f in FN_EVENT_CALLBACKS {
        assert!(MODULE_FUNCS.contains(&f), "0x{f:x} 不在 37 条表里");
    }
    // 图标表所在函数**不**属于本模块（矩阵记在 texture_loader），只借数据。
    assert!(!MODULE_FUNCS.contains(&FN_ICON_PATH_TABLE));
    // 三个"接线/重建/同步"三个主干各有落点，且互不相同。
    assert_ne!(FN_SETUP_WIRING, FN_MODE_PANEL_SYNC);
    assert_ne!(FN_MODE_PANEL_SYNC, FN_TEAM_LIST_REBUILD);
}

// ---------------------------------------------------------------------------
// 2. 图标表：逐位（`0x4c9190` 的 45 条 (path,len,id)）
// ---------------------------------------------------------------------------

/// 真值 = `0x4c9190` 里 `lea rdi,<path>` + `mov esi,<len>` + `mov edx/xor edx,<id>`。
/// 长度立即数分别是 23/22/26/28/18/17…；这里连长度一起钉住，防止"看起来对"。
#[test]
fn mode_icon_tables_are_exact() {
    assert_eq!(
        AXIS_ICONS,
        [
            "res://axis_option_0.svg",
            "res://axis_option_1.svg",
            "res://axis_option_2.svg",
            "res://axis_option_3.svg"
        ]
    );
    // 长度立即数：四条全是 0x17 = 23
    assert!(AXIS_ICONS.iter().all(|s| s.len() == 23));
    // 字面量地址：0x1c2efd / 0x1c2f14 / 0x1c2f2b / 0x1c2f42（间隔 0x17，首尾相接）
    assert_eq!(AXIS_ICONS[1].len(), AXIS_ICONS[0].len());

    assert_eq!(
        FUNCTION_MODE_ICONS,
        ["res://fx_option.svg", "res://dx_dy_option.svg"]
    );
    assert_eq!(FUNCTION_MODE_ICONS[0].len(), 19, "0x4c9493 的 esi 立即数");
    assert_eq!(FUNCTION_MODE_ICONS[1].len(), 22, "0x4c9519 的 esi 立即数");

    assert_eq!(
        TURN_MODE_ICONS,
        ["res://turns_sequential.svg", "res://turns_simultaneous.svg"]
    );
    assert_eq!(TURN_MODE_ICONS[0].len(), 26, "0x4c960f 的 esi 立即数");
    assert_eq!(TURN_MODE_ICONS[1].len(), 28, "0x4c9695 的 esi 立即数");

    assert_eq!(
        TIME_MODE_ICONS,
        [
            "res://turn_30s.svg",
            "res://turn_1m.svg",
            "res://turn_2m.svg",
            "res://turn_3m.svg",
            "res://turn_5m.svg",
            "res://turn_inf.svg"
        ]
    );
    // 六条的长度立即数：18 / 17 / 17 / 17 / 17 / 18
    let lens: Vec<usize> = TIME_MODE_ICONS.iter().map(|s| s.len()).collect();
    assert_eq!(lens, vec![18, 17, 17, 17, 17, 18]);
    // 1m..5m 四条等长（0x1c3248/59/6a/7b 首尾相接，步长 0x11 = 17）
    assert!(TIME_MODE_ICONS[1..5].iter().all(|s| s.len() == 17));
}

/// 名字表：`.data.rel.ro` `0x9626f0`…`0x9627a0` 的 12 个 `(ptr,len)`；
/// `+8` 的长度字段与 addend 指向的字节数**互为独立校验**。
#[test]
fn mode_name_table_is_exact_with_slot_lengths() {
    assert_eq!(MODE_NAME_TABLE.len(), 12);
    let lens: Vec<usize> = MODE_NAME_TABLE.iter().map(|s| s.len()).collect();
    assert_eq!(
        lens,
        vec![14, 14, 6, 8, 9, 9, 8, 7, 7, 7, 7, 8],
        "槽 +8 的长度字段逐条（NormalFunction…TimerInf）"
    );
    // 前三段的分界：FunctionMode 2 个、AxisMode 4 个、TimeMode 6 个 = 12
    assert_eq!(FunctionMode::ALL.len() + AxisMode::ALL.len() + TimeMode::ALL.len(), 12);
}

/// `TurnMode` 的名字表在 `0x957d40`/`0x957d50`（另一处 const，紧跟 color_manager 的 Location）。
#[test]
fn turn_mode_name_table_is_exact() {
    assert_eq!(TURN_MODE_NAMES, ["SequentialTurns", "SimultaneousTurns"]);
    assert_eq!(TURN_MODE_NAMES[0].len(), 15);
    assert_eq!(TURN_MODE_NAMES[1].len(), 17);
}

// ---------------------------------------------------------------------------
// 3. 四格：判别值 ↔ 图标 ↔ 名字 ↔ 时间档
// ---------------------------------------------------------------------------

/// 判别值 = 图标表下标 = 名字表段内下标（三条独立表的**同序**校验）。
#[test]
fn enum_values_index_the_icon_and_name_tables() {
    for m in AxisMode::ALL {
        assert_eq!(m.icon_path(), AXIS_ICONS[m.as_u8() as usize]);
        assert_eq!(m.name(), MODE_NAME_TABLE[2 + m.as_u8() as usize]);
    }
    for m in FunctionMode::ALL {
        assert_eq!(m.icon_path(), FUNCTION_MODE_ICONS[m.as_u8() as usize]);
        assert_eq!(m.name(), MODE_NAME_TABLE[m.as_u8() as usize]);
    }
    for m in TurnMode::ALL {
        assert_eq!(m.icon_path(), TURN_MODE_ICONS[m.as_u8() as usize]);
        assert_eq!(m.name(), TURN_MODE_NAMES[m.as_u8() as usize]);
    }
    for m in TimeMode::ALL {
        assert_eq!(m.icon_path(), TIME_MODE_ICONS[m.as_u8() as usize]);
        assert_eq!(m.name(), MODE_NAME_TABLE[6 + m.as_u8() as usize]);
    }
}

/// 线上名字：AxisMode 4/4、FunctionMode 2/2、TurnMode 2/2 与抓包里出现的字符串一致
/// （`docs/09-network.md`：`EveryUnit` / `NormalFunction` / `SequentialTurns` / `Timer1m`）。
/// AxisMode 的 0/1/2 三个名字来自 `docs/10` §4 注（运行时 21/21 与 `3/2/1/0` 对应）。
#[test]
fn wire_names_match_the_captured_strings() {
    assert_eq!(AxisMode::EveryUnit.name(), "EveryUnit");
    assert_eq!(AxisMode::EveryFive.name(), "EveryFive");
    assert_eq!(AxisMode::OnlyMain.name(), "OnlyMain");
    assert_eq!(AxisMode::NoAxis.name(), "NoAxis");
    assert_eq!(FunctionMode::NormalFunction.name(), "NormalFunction");
    assert_eq!(FunctionMode::DiffEqFunction.name(), "DiffEqFunction");
    assert_eq!(TurnMode::SequentialTurns.name(), "SequentialTurns");
    assert_eq!(TurnMode::SimultaneousTurns.name(), "SimultaneousTurns");
    for t in TimeMode::ALL {
        // `graphwar_tick_handler::turn_duration_ticks` 认的就是这 6 个字符串（抓包实测）
        assert!(
            matches!(
                t.name(),
                "Timer30s" | "Timer1m" | "Timer2m" | "Timer3m" | "Timer5m" | "TimerInf"
            ),
            "{}",
            t.name()
        );
    }
}

/// 档位顺序：0=30s … 5=inf。判据两条：图标 id 逐位；名字表数组序逐位。
/// ⇒ `LevelModes::time_byte == 5` 就是 `TimerInf`（`docs/10` §7 第 6 条由推断升为逐位）。
#[test]
fn time_mode_discriminants_are_the_icon_ids() {
    assert_eq!(TimeMode::Timer30s.as_u8(), 0);
    assert_eq!(TimeMode::Timer1m.as_u8(), 1);
    assert_eq!(TimeMode::Timer2m.as_u8(), 2);
    assert_eq!(TimeMode::Timer3m.as_u8(), 3);
    assert_eq!(TimeMode::Timer5m.as_u8(), 4);
    assert_eq!(TimeMode::TimerInf.as_u8(), 5);
    assert_eq!(TimeMode::from_u8(5), Some(TimeMode::TimerInf));
    assert_eq!(TimeMode::from_u8(6), None);
    assert_eq!(TimeMode::from_u8(0xff), None);
    // 名字表第 11 项（下标 6+5）= "TimerInf"
    assert_eq!(MODE_NAME_TABLE[11], "TimerInf");
}

/// 时长：真实抓包（`docs/42` / `src/graphwar_tick_handler.rs`）——秒数逐档：
/// 30 / 60 / 120 / 180 / 300；`TimerInf` 没有截止时刻。
#[test]
fn durations_match_the_captured_turn_lengths() {
    assert_eq!(TimeMode::Timer30s.duration_secs(), Some(30));
    assert_eq!(TimeMode::Timer1m.duration_secs(), Some(60));
    assert_eq!(TimeMode::Timer2m.duration_secs(), Some(120));
    assert_eq!(TimeMode::Timer3m.duration_secs(), Some(180));
    assert_eq!(TimeMode::Timer5m.duration_secs(), Some(300));
    assert_eq!(TimeMode::TimerInf.duration_secs(), None);
    // 与已落地的 tick 表互相对齐（同一份抓包，两处实现必须一致）
    for t in TimeMode::ALL {
        let want = t.duration_secs().map(|s| s as i64 * 60);
        assert_eq!(
            gw2_rebuild::graphwar_tick_handler::turn_duration_ticks(t.name()),
            want,
            "{} 的 tick 数",
            t.name()
        );
    }
}

/// 显示规则 `mm:ss`：真帧观测 `02:59`（3 分钟档）/ `01:59`（2 分钟档）；
/// `00:04`/`00:00` 说明倒计时走到 0。⇒ 起始值 = 时长 − 1。
#[test]
fn timer_label_starts_at_duration_minus_one() {
    assert_eq!(TimeMode::Timer3m.timer_label().as_deref(), Some("02:59"));
    assert_eq!(TimeMode::Timer2m.timer_label().as_deref(), Some("01:59"));
    assert_eq!(TimeMode::Timer1m.timer_label().as_deref(), Some("00:59"));
    // 30s / 5m 两档是同一规则的延伸（**推断**，未逐帧验证）
    assert_eq!(TimeMode::Timer30s.timer_label().as_deref(), Some("00:29"));
    assert_eq!(TimeMode::Timer5m.timer_label().as_deref(), Some("04:59"));
    // 无限档：显示什么**未知** ⇒ None（不许编一个 "inf" 出来）
    assert_eq!(TimeMode::TimerInf.timer_label(), None);
    assert_eq!(TimeMode::TimerInf.initial_timer_secs(), None);
}

/// 点击循环：6 次回到原档，`prev` 是 `next` 的逆（档位：推断，见代码注释）。
#[test]
fn mode_buttons_cycle() {
    let mut t = TimeMode::Timer30s;
    for _ in 0..6 {
        t = t.next();
    }
    assert_eq!(t, TimeMode::Timer30s);
    assert_eq!(TimeMode::TimerInf.next(), TimeMode::Timer30s, "最末档回到第一档");
    assert_eq!(TimeMode::Timer30s.prev(), TimeMode::TimerInf);
    for t in TimeMode::ALL {
        assert_eq!(t.next().prev(), t);
    }
    // 两档枚举都是"切换另一档"
    assert_eq!(FunctionMode::NormalFunction.next(), FunctionMode::DiffEqFunction);
    assert_eq!(TurnMode::SequentialTurns.next(), TurnMode::SimultaneousTurns);
    assert_eq!(AxisMode::EveryUnit.next(), AxisMode::NoAxis, "4 档循环");
}

// ---------------------------------------------------------------------------
// 4. 初值：新建房间 vs 战役关卡
// ---------------------------------------------------------------------------

/// 新建房间的四格初值。真值 = `docs/09-network.md` 里新建房间的首个 `GameInfo`：
/// `EveryUnit` / `NormalFunction` / `SequentialTurns` / `Timer1m`。
///
/// 交叉验证（独立于报文）：`docs/16` §2.1.10 的按键实验 —— 按 1 次 `TurnTimeButton`
/// 后对局计时器显示 `01:59`（2 分钟档），按 2 次后 `02:59`（3 分钟档）
/// ⇒ 初值必须是**判别值 1**（`Timer1m`）。
#[test]
fn new_room_defaults_match_the_capture_and_the_keypress_experiment() {
    let d = SetupOptions::room_default();
    assert_eq!(d.axis, AxisMode::EveryUnit);
    assert_eq!(d.function, FunctionMode::NormalFunction);
    assert_eq!(d.turns, TurnMode::SequentialTurns);
    assert_eq!(d.time, TimeMode::Timer1m);
    assert_eq!(d.time.as_u8(), 1, "按键实验要求初值 = 判别值 1");

    // 从初值按 1 次 / 2 次，必须正好是观测到的那两档
    let once = SetupOptions {
        time: d.time.next(),
        ..d
    };
    let twice = SetupOptions {
        time: once.time.next(),
        ..d
    };
    assert_eq!(once.timer_label().as_deref(), Some("01:59"));
    assert_eq!(twice.timer_label().as_deref(), Some("02:59"));

    assert_eq!(
        d.icons(),
        [
            "res://axis_option_3.svg",  // EveryUnit = 3
            "res://fx_option.svg",      // NormalFunction = 0
            "res://turns_sequential.svg", // SequentialTurns = 0
            "res://turn_1m.svg",        // Timer1m = 1
        ]
    );
    assert_eq!(
        d.names(),
        ["EveryUnit", "NormalFunction", "SequentialTurns", "Timer1m"]
    );
    assert!(d.has_turn_clock());
}

/// 战役第 n 关的四格 = `LEVELS[n].modes`（逐位），
/// 且 21 关的时间字节全是 5 ⇒ `TimerInf`（本轮的升级：名字表第 11 项就是 `"TimerInf"`）。
#[test]
fn campaign_levels_take_their_four_cells_from_the_level_data() {
    for i in 0..graphwar_level_manager::LEVEL_COUNT {
        let info = graphwar_level_manager::level_info(i).unwrap();
        let opts = SetupOptions::from_level(i).expect("每关都该能换算四格");
        assert_eq!(opts.axis, info.modes.axis, "第 {i} 关 axis");
        assert_eq!(opts.function, info.modes.function, "第 {i} 关 function");
        assert_eq!(opts.turns, info.modes.turns, "第 {i} 关 turn");
        assert_eq!(info.modes.time_byte, 5, "第 {i} 关的 +0x3b 恒为 5");
        assert_eq!(opts.time, TimeMode::TimerInf, "第 {i} 关是无限时");
        assert!(!opts.has_turn_clock(), "战役关不显示倒计时");
        assert_eq!(opts.time.timer_label(), None);
    }
    assert_eq!(SetupOptions::from_level(graphwar_level_manager::LEVEL_COUNT), None);
}

// ---------------------------------------------------------------------------
// 5. 玩家条目：增删 / 上限 / 换队
// ---------------------------------------------------------------------------

/// 5 个槽是**观测量**：`docs/16` §2.1.5 的 `SoldierIconContainer/MarginContainer…5`。
#[test]
fn soldier_slots_are_five() {
    assert_eq!(MAX_SOLDIERS_PER_PLAYER, 5);
    let mut p = PlayerEntry::human("user", Team::Two, 2);
    let slots = p.soldier_slots();
    assert_eq!(slots.len(), 5);
    assert_eq!(slots[0], (0, true));
    assert_eq!(slots[1], (1, true));
    assert_eq!(slots[2], (2, false), "空槽画 res://empty_soldier.svg");
    // 构造时也钳位
    assert_eq!(PlayerEntry::human("x", Team::One, 9).soldiers, 5);
    p.soldiers = 0;
    assert!(p.soldier_slots().iter().all(|&(_, used)| !used));
}

/// `AddSoldierButton` 到 5 就停、`RemoveSoldierButton` 到 0 就停（饱和，不删行）。
#[test]
fn add_and_remove_soldier_saturate() {
    let mut p = PlayerEntry::human("user", Team::Two, 4);
    assert!(p.add_soldier());
    assert_eq!(p.soldiers, 5);
    assert!(!p.add_soldier(), "到 5 不再涨");
    assert_eq!(p.soldiers, 5);

    assert!(p.remove_soldier());
    assert_eq!(p.soldiers, 4);
    let mut q = PlayerEntry::human("user", Team::Two, 0);
    assert!(!q.remove_soldier(), "到 0 不再减");
    assert_eq!(q.soldiers, 0);
}

/// `ChangeTeamButton` 换到对面；两个容器的名字是 `get_node` 实参（逐位：0x1c2d7f/0x1c2d8d）。
#[test]
fn change_team_flips_between_the_two_containers() {
    assert_eq!(Team::One.container(), "Team1Container");
    assert_eq!(Team::Two.container(), "Team2Container");
    let mut p = PlayerEntry::human("user", Team::Two, 1);
    p.change_team();
    assert_eq!(p.team, Team::One);
    p.change_team();
    assert_eq!(p.team, Team::Two);
    // 图标只在 .tscn 里（.so 无字面量）⇒ 推断档，但方向不能反
    assert_eq!(Team::One.change_icon(), "res://arrow_right.svg");
    assert_eq!(Team::Two.change_icon(), "res://arrow_left.svg");
}

/// Bot 的难度档 0..=5（`AddBotPopUp` 六个按钮），超界钳位；人类没有难度档。
#[test]
fn bot_entries_carry_a_difficulty_within_six_buttons() {
    let b = PlayerEntry::bot(3, "Alpha Zordon", Team::One, 1);
    assert!(b.is_bot());
    assert_eq!(b.bot_level, Some(3));
    assert_eq!(PlayerEntry::bot(99, "x", Team::One, 1).bot_level, Some(5));
    assert!(!PlayerEntry::human("user", Team::Two, 1).is_bot());
}

// ---------------------------------------------------------------------------
// 6. Start / Cancel 的屏切换
// ---------------------------------------------------------------------------

/// 真值 = `docs/16` §2.1.5：不加 Bot 直接 `Start` 是**静默无操作**（同 `archive/22` §5.1）。
#[test]
fn start_without_an_opponent_is_silent() {
    assert_eq!(start_action(&[]), SetupAction::SilentNoOp);
    let alone = [PlayerEntry::human("user", Team::Two, 1)];
    assert!(!can_start(&alone));
    assert_eq!(start_action(&alone), SetupAction::SilentNoOp, "原版不给提示");
}

/// 有对手才能开：一个人类 + 对面一个 Bot ⇒ `PlayScreen`（观测：Start → PlayScreen.visible=1）。
#[test]
fn start_with_an_opponent_enters_the_play_screen() {
    let both = [
        PlayerEntry::human("user", Team::Two, 1),
        PlayerEntry::bot(2, "ChatGPT", Team::One, 1),
    ];
    assert!(can_start(&both));
    assert_eq!(start_action(&both), SetupAction::StartMatch);
    // 3 人 2:1 也算能开（**推断**：判据是"两队都非空"，边界未观测）
    let three = [
        PlayerEntry::human("user", Team::Two, 1),
        PlayerEntry::human("guest", Team::Two, 1),
        PlayerEntry::bot(0, "Deep Blue", Team::One, 1),
    ];
    assert_eq!(start_action(&three), SetupAction::StartMatch);
}

/// 返回/取消的去向：屏名全部逐位（`docs/16` §15.2 的 15 屏表），
/// 但"按下去真的去哪"是推断档（代码注释里写明缺什么）。
#[test]
fn cancel_and_exit_targets_are_named_screens() {
    assert_eq!(exit_action(), SetupAction::Back(Screen::LanLobby));
    assert_eq!(join_level_cancel(), SetupAction::Back(Screen::Campaign));
    assert_eq!(join_level_start(), SetupAction::StartMatch);
    // 屏名本身的字节在 root_node 里已经逐位核过；这里只确认没拿错屏
    assert_eq!(Screen::PlayScreen.name(), "PlayScreen");
    assert_eq!(Screen::LanLobby.name(), "LanLobby");
    assert_eq!(Screen::Campaign.name(), "Campaign");
}

// ---------------------------------------------------------------------------
// 7. 关卡数据 → 这一屏的取值（士兵数 / 回合上限）
// ---------------------------------------------------------------------------

/// `docs/10-campaign.md` §4「21 关全参数表」的 `士兵数` 列（逐位：`0x476320` 的 `edx`）。
///
/// 这就是 `docs/16` §13 附 1 那个错的正解：**别用 1，用关卡数据**。
#[test]
fn campaign_soldiers_follow_the_level_data_not_one() {
    const DOC_SOLDIERS: [u32; graphwar_level_manager::LEVEL_COUNT] =
        [1, 1, 1, 2, 1, 2, 1, 1, 1, 3, 1, 3, 1, 1, 4, 5, 1, 1, 3, 4, 5];
    let mut got = Vec::new();
    for i in 0..graphwar_level_manager::LEVEL_COUNT {
        let n = campaign_soldier_count(i).expect("每关都有士兵数");
        assert_eq!(n, DOC_SOLDIERS[i], "第 {i} 关士兵数");
        got.push(n);
    }
    assert_eq!(got.iter().filter(|&&n| n == 1).count(), 12, "12 关只有 1 兵");
    assert_eq!(got.iter().copied().max(), Some(5), "最多 5 兵（= 士兵槽数）");
    assert!(got.iter().all(|&n| n >= 1 && n <= MAX_SOLDIERS_PER_PLAYER as u32));
    assert_eq!(campaign_soldier_count(graphwar_level_manager::LEVEL_COUNT), None);
}

/// `docs/10-campaign.md` §4 的 `回合上限` 列（逐位：`turn_limit_info` 的 `(1, N)`）。
#[test]
fn campaign_turn_limits_match_the_level_data() {
    const DOC_LIMITS: [Option<u32>; graphwar_level_manager::LEVEL_COUNT] = [
        None,
        Some(5),
        Some(5),
        Some(10),
        Some(10),
        None,
        Some(6),
        Some(1),
        Some(6),
        None,
        Some(6),
        None,
        Some(5),
        Some(5),
        None,
        None,
        None,
        Some(6),
        None,
        None,
        None,
    ];
    for i in 0..graphwar_level_manager::LEVEL_COUNT {
        assert_eq!(
            campaign_turn_limit(i),
            Some(DOC_LIMITS[i]),
            "第 {i} 关回合上限"
        );
    }
    assert_eq!(campaign_turn_limit(graphwar_level_manager::LEVEL_COUNT), None);
    // 有回合上限的关 ≠ 有时间档的关：战役 21 关的时限档全是 TimerInf，
    // 所以"回合上限"与"回合时限"是两个不同的旋钮，不能混。
    for i in 0..graphwar_level_manager::LEVEL_COUNT {
        assert_eq!(SetupOptions::from_level(i).unwrap().time, TimeMode::TimerInf);
        if campaign_turn_limit(i).unwrap().is_some() {
            assert!(!SetupOptions::from_level(i).unwrap().has_turn_clock());
        }
    }
}
