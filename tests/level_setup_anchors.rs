//! `graphwar_level_manager` 的 21 个 `setup_level_N`：**函数边界 × 二进制证据** 的对拍。
//!
//! 真值来源 = `fixtures/level_setup_anchors.tsv`，那张表的每一格都是从**原版 `.so`** 读出来的：
//! * `vaddr` = 函数入口（独立函数）或内联臂起点（被 `setup_level` 内联的那 8 关）；
//! * `fde` = `.eh_frame` FDE 边界（`tools/addr_probe.py fde`）；
//! * `src_lines` = 该函数自己引用的 `panic::Location` 覆盖到的源行（`tools/loc_refs.py func`）；
//! * `preset_fn` / `second_fn` = 闭包入口地址；`soldiers`/`targets`/`bot`/`turn_limit` = 开场臂里的
//!   立即数与调用次数（`objdump -d`）。
//!
//! 所以这个测试不是「我方输出对我方输出」的自洽断言：**左边是二进制读数，右边是我方源码**。
//!
//! 它防的三件事：
//! 1. 有人再把 21 个函数**压回一张数据表**（那样 21 个 `fn` 就没了，`F_GRADE` 会掉回 F3）；
//! 2. 有人改了某一关的参数却只改表不改函数（反之亦然）—— 现在表是函数的产物，改一处就两处一起变，
//!    而任何一格与二进制不符都会在这里红；
//! 3. 分派 `setup_level(index)` 少一个臂 / 越界行为变了。

use std::collections::BTreeMap;
use std::path::PathBuf;

use gw2_rebuild::graphwar_level_manager::{
    level_setup, GraphwarLevelManager, LevelSetup, ObstaclePreset, LEVEL_COUNT, SETUPS,
};

/// 台账里一行。
#[derive(Debug, Clone)]
struct AnchorRow {
    level: usize,
    kind: String,
    name: String,
    vaddr: u32,
    fde: String,
    src_lines: String,
    gen_call: String,
    preset_fn: u32,
    second_fn: u32,
    preset: String,
    soldiers: u32,
    targets: u32,
    bot: Option<u32>,
    turn_limit: Option<u32>,
}

fn fixture_path() -> PathBuf {
    // `cargo test` 的 cwd 是包根
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/level_setup_anchors.tsv")
}

fn parse_hex(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16)
        .unwrap_or_else(|e| panic!("不是十六进制地址 {s:?}: {e}"))
}

fn load_anchors() -> BTreeMap<usize, AnchorRow> {
    let text = std::fs::read_to_string(fixture_path()).expect("读不到 fixtures/level_setup_anchors.tsv");
    let mut out = BTreeMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if line.starts_with("level\t") {
            continue; // 表头
        }
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 22, "台账列数不对（{} 列）：{line}", f.len());
        if f[0] == "-" {
            // 分派函数那一行（没有关卡下标），本测试单独验
            continue;
        }
        let bot = if f[19] == "-" { None } else { Some(f[19].parse().unwrap()) };
        let turn_limit = if f[20] == "none" { None } else { Some(f[20].parse().unwrap()) };
        let row = AnchorRow {
            level: f[0].parse().unwrap(),
            kind: f[1].to_string(),
            name: f[2].to_string(),
            vaddr: parse_hex(f[3]),
            fde: f[4].to_string(),
            src_lines: f[6].to_string(),
            gen_call: f[7].to_string(),
            preset_fn: parse_hex(f[10]),
            second_fn: parse_hex(f[13]),
            preset: f[16].to_string(),
            soldiers: f[17].parse().unwrap(),
            targets: f[18].parse().unwrap(),
            bot,
            turn_limit,
        };
        out.insert(row.level, row);
    }
    out
}

/// 台账 21 行、每关一行、名字都带正确的下标。
#[test]
fn anchor_ledger_covers_all_twenty_one_levels() {
    let rows = load_anchors();
    assert_eq!(rows.len(), LEVEL_COUNT, "台账必须正好 21 行（每关一行）");
    for lv in 0..LEVEL_COUNT {
        let r = rows.get(&lv).unwrap_or_else(|| panic!("台账缺第 {lv} 关"));
        assert_eq!(r.name, format!("GraphwarLevelManager::setup_level_{lv}"));
        assert_eq!(r.level, lv);
        assert!(
            r.kind == "standalone" || r.kind == "inline",
            "关 {lv} 的 kind 只能是 standalone / inline，实际 {} —— 必须重跑取证",
            r.kind
        );
        // 每个函数都必须有自己的 `panic::Location` 锚（源行区间）—— 没有锚就没有源级证据
        assert!(
            r.src_lines.contains('-'),
            "关 {lv} 的 src_lines 列没有源行区间，台账不完整"
        );
        // 每一关都必须有一次 `call 0x466d20`（关卡开场驱动）——21 关合计 21 条
        assert!(
            r.gen_call.starts_with("0x"),
            "关 {lv} 缺 gen_call（0x466d20 的调用点），台账不完整"
        );
    }
    // 8 inline + 13 standalone = 21：这个分布是「函数边界」的核心读数，掉了就说明取证口径变了
    let inline = rows.values().filter(|r| r.kind == "inline").count();
    let standalone = rows.values().filter(|r| r.kind == "standalone").count();
    assert_eq!((inline, standalone), (8, 13), "内联/独立的分档读数变了（台账或口径被改）");
}

/// 我方 21 个 `setup_level_N` 返回的参数 **逐字段**等于台账里从二进制读出的值。
///
/// 每个函数都真的被调用一次（不是读表）。
#[test]
fn each_setup_level_function_matches_the_binary_anchors() {
    let rows = load_anchors();
    let fns: [fn() -> LevelSetup; LEVEL_COUNT] = [
        GraphwarLevelManager::setup_level_0,
        GraphwarLevelManager::setup_level_1,
        GraphwarLevelManager::setup_level_2,
        GraphwarLevelManager::setup_level_3,
        GraphwarLevelManager::setup_level_4,
        GraphwarLevelManager::setup_level_5,
        GraphwarLevelManager::setup_level_6,
        GraphwarLevelManager::setup_level_7,
        GraphwarLevelManager::setup_level_8,
        GraphwarLevelManager::setup_level_9,
        GraphwarLevelManager::setup_level_10,
        GraphwarLevelManager::setup_level_11,
        GraphwarLevelManager::setup_level_12,
        GraphwarLevelManager::setup_level_13,
        GraphwarLevelManager::setup_level_14,
        GraphwarLevelManager::setup_level_15,
        GraphwarLevelManager::setup_level_16,
        GraphwarLevelManager::setup_level_17,
        GraphwarLevelManager::setup_level_18,
        GraphwarLevelManager::setup_level_19,
        GraphwarLevelManager::setup_level_20,
    ];
    assert_eq!(fns.len(), LEVEL_COUNT);
    for (lv, f) in fns.iter().enumerate() {
        let got = f();
        let want = &rows[&lv];
        // ① 函数入口 / 内联臂起点
        assert_eq!(
            got.start_fn, want.vaddr,
            "关 {lv}：start_fn 与台账 vaddr 不符（{} vs 0x{:x}）",
            want.fde, want.vaddr
        );
        // ② 障碍预设闭包 / 形参
        assert_eq!(got.preset_fn, want.preset_fn, "关 {lv} 的预设入口");
        // ③ 第二闭包
        assert_eq!(got.second_fn, want.second_fn, "关 {lv} 的第二闭包入口");
        // ④ 障碍预设的种类 + min_count
        match (got.preset, want.preset.as_str()) {
            (ObstaclePreset::None, s) => assert!(s.starts_with("None"), "关 {lv}：{s}"),
            (ObstaclePreset::CentreSingle, s) => {
                assert!(s.starts_with("CentreSingle"), "关 {lv}：{s}")
            }
            (ObstaclePreset::Scatter { min_count }, s) => {
                let expect = format!("Scatter(min_count={min_count})");
                assert_eq!(s, expect, "关 {lv} 的散布 min_count");
            }
        }
        // ⑤ 士兵 / 靶子 / Bot / 回合上限
        assert_eq!(got.soldiers, want.soldiers, "关 {lv} 士兵数");
        assert_eq!(got.targets, want.targets, "关 {lv} 靶子数");
        assert_eq!(got.bot, want.bot, "关 {lv} Bot 难度");
        assert_eq!(got.turn_limit, want.turn_limit, "关 {lv} 回合上限");
    }
}

/// `SETUPS` 是那 21 个函数的**产物**（同一份数据，不是第二份真值）。
#[test]
fn setups_table_is_the_product_of_the_functions() {
    let dispatch = (0..LEVEL_COUNT)
        .map(|i| GraphwarLevelManager::setup_level(i).expect("21 臂都要命中"))
        .collect::<Vec<_>>();
    for (i, want) in dispatch.iter().enumerate() {
        let from_table = &SETUPS[i];
        assert_eq!(from_table.start_fn, want.start_fn, "SETUPS[{i}] 与 setup_level({i}) 不一致");
        assert_eq!(from_table.preset_fn, want.preset_fn, "SETUPS[{i}] 预设");
        assert_eq!(from_table.second_fn, want.second_fn, "SETUPS[{i}] 第二闭包");
        assert_eq!(from_table.soldiers, want.soldiers, "SETUPS[{i}] 士兵");
        assert_eq!(from_table.targets, want.targets, "SETUPS[{i}] 靶子");
        assert_eq!(from_table.bot, want.bot, "SETUPS[{i}] Bot");
        assert_eq!(from_table.turn_limit, want.turn_limit, "SETUPS[{i}] 回合上限");
        assert_eq!(
            format!("{:?}", from_table.preset),
            format!("{:?}", want.preset),
            "SETUPS[{i}] 预设种类"
        );
        // `level_setup` 返回的静态引用也必须指到同一格
        let r = level_setup(i).expect("level_setup 不该 None");
        assert_eq!(r.start_fn, want.start_fn, "level_setup({i}) 没走分派");
    }
}

/// 分派器的边界：21 个下标全中，21（及以后）返回 `None`。
///
/// 原版分派按 `level_<n>` 键匹配（`docs/10` §3，21 个键）；本层用下标，越界必须 `None`。
#[test]
fn dispatch_covers_exactly_the_twenty_one_levels() {
    for i in 0..LEVEL_COUNT {
        assert!(GraphwarLevelManager::setup_level(i).is_some(), "关 {i} 的臂丢了");
    }
    assert!(GraphwarLevelManager::setup_level(LEVEL_COUNT).is_none(), "第 22 关不该有臂");
    assert!(GraphwarLevelManager::setup_level(9999).is_none());
    assert!(level_setup(LEVEL_COUNT).is_none());
}
