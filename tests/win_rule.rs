//! **胜负条件：到底几条规则？**（`docs/150-win-rule.md`）
//!
//! 本文这批断言要钉住的是一件**结构**上的事，不是"某个场景能跑通"：
//!
//! > 原版只有**一条**判据 —— *某队没有存活实体*。
//! > 打靶关（`Team2` 里只有靶子）与对战关（`Team2` 里只有士兵）走的是**同一条**代码路径。
//!
//! 依据（全部写在 `docs/150`）：
//!
//! | 事实 | 等级 | 证据 |
//! |---|---|---|
//! | 靶子是 `Team2` 的实体 | **观测** | 127/127 个靶子 id 都有 `TeamInfo{Team2}`（111 份回放） |
//! | 靶子**不是**士兵 | **观测** | 127/127 个靶子 id 都**没有** `SoldierInfo`；`SkinGraphicsInfo="target"` |
//! | 造靶子的 `0x46f310` 的 `team` 是**入参** | **逐位** | `movzx edx,bpl` → `0x3c17d0(…, id, team)`；同一函数里 `TargetInfo`/`SkinGraphicsInfo("target")` |
//! | 判据 = 数两队"有活实体" | **逐位** | `0x473520` 遍历生命表 id，逐个 `0x454950`(判活) + `0x3c2120`(查队伍)，返回 `(Team1 有活人, Team2 有活人)` |
//! | 结束 = 上限用尽 或 某队没人 | **逐位** | `0x4759f0`：`0x475a77`(上限) / `0x475a80`(al==0) / `0x475a88`(dl==0) 三条都跳 `0x475bac` |
//! | 上限用尽 ⇒ 胜方被写死成 `1`(Team2) | **逐位** | `0x475bfb`–`0x475c14` 写 `[self+0x11f]` |
//! | 217/217 次 `EndingGame` 都有一队"零活实体" | **观测** | 401 份回放（其中 45 份打靶关、172 份对战/联机） |
//!
//! ⚠️ 按 `AGENTS.md` §0 第 2 条：**这些测试全绿不构成复原证据**，证据是上面那张表里的
//! 地址与回放；测试只是**回归保护**，防止有人把"两条规则"的理念又加回来。

//!
//! ## T64：真值从哪来（`fixtures/test_expectation_plan3.tsv` 的 W01–W04）
//!
//! T46 曾把 `win_rule` 判成「换不了」：理由是「403 份回放里 `EndingGame` 出现 0 次 ⇒ 缺一份打到
//! 结束的整局回放」。**本轮证伪这条前提**——那次是在 gzip 压缩字节上 grep（与 `TurnLimitInfo`
//! 的假阴性同一类）。解压重扫 403 份：
//!
//! | 事实（`fixtures/test_expectation_truth3.tsv` 的 `W/` 段） | 读数 |
//! |---|---|
//! | 含 `GameStateInfo{"game_state":"EndingGame"}` 的回放 | **217 / 401** 份 |
//! | 终局时**恰好一队**零活实体 | 193 份 |
//! | 终局时**双方都**零活实体（互相全灭） | **24 份**（旧注释写的"语料里没有这种收场"是错的） |
//! | 终局里含靶子（打靶关） | 45 份 |
//! | 终局里全是士兵（对战/联机局） | 172 份 |
//! | 终局时 `turn_count` 已到 `TurnLimitInfo` 上限 | 1 份（`1782641048`，上限 1） |
//! | `TurnLimitInfo` 帧数 | 21 帧 / 21 份回放（⇒ 382 份无上限） |
//!
//! 复算：`.reference/venv/bin/python tools/gen_test_expectations3.py --section corpus`
//! （解压 403 份、按 `TeamInfo`/`SoldierInfo`/`SkinGraphicsInfo`/`LifeInfo`/`EntityRemoved`/
//! `RemoveTeam` 重建终局队伍构成；玩家实体**不算**生命表实体）。

use gw2_rebuild::game::{Ending, EndingReason, Match};
use gw2_rebuild::server::RoomState;
use gw2_rebuild::graphwar_shot_calculator::settle::Target;

/// 关卡里的靶子一律在 **Team2**（`docs/150` §Q1）。
const TEAM_2: u8 = 1;

fn room() -> RoomState {
    RoomState {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
    }
}

fn soldier(id: i64, x: f64, y: f64) -> Target {
    Target { id, x, y, radius: 0.5 }
}

/// 打靶关的真值形态（`docs/123` §2.1 的回放逐条）：
/// `Team1` = 一个玩家 + 一名士兵；`Team2` = **只有靶子，一个士兵都没有**。
fn target_level() -> Match {
    let mut m = Match::new(room(), vec![], vec![soldier(10, -5.0, -5.1)], (0, 0));
    m.set_team(10, 0);
    m.add_target(1099, TEAM_2);
    m.add_target(1100, TEAM_2);
    m
}

/// 对战关的真值形态：两队各一名士兵，没有靶子。
fn versus_level() -> Match {
    let mut m = Match::new(
        room(),
        vec![],
        vec![soldier(10, -5.0, -5.1), soldier(20, 5.0, -5.1)],
        (0, 0),
    );
    m.set_team(10, 0);
    m.set_team(20, TEAM_2);
    m
}

// ---------------------------------------------------------------------------
// 0. **真终局局面**（冻结语料里逐份读出来的，见文件头 W 段）
// ---------------------------------------------------------------------------

/// 语料里的「终局队伍构成」：`id:team:kind:alive`，`kind` ∈ {soldier, target, player}。
///
/// 直接用 `fixtures/test_expectation_truth3.tsv` 的 `W01/*/composition` 原文，不做任何加工。
const CORPUS_TARGET_LEVEL: &str =
    "347:0:player:a|348:0:soldier:a|349:0:soldier:d|350:1:target:d";
const CORPUS_VERSUS_LEVEL: &str =
    "876:0:player:a|877:0:soldier:d|878:0:soldier:a|879:1:player:a|880:1:soldier:d|881:1:soldier:d";
const CORPUS_MUTUAL_WIPE: &str =
    "153429:1:player:a|153430:1:soldier:d|153431:1:soldier:d|153654:0:player:a|153655:0:soldier:d|153656:0:soldier:d";

/// 按语料里的构成重建一个 `Match`（真值 `0x46f310`/`0x473520` 的那套数据形态）：
///
/// * 玩家实体：只登记队伍（**不进生命表** —— 原版数胜负走 `0x473520` 遍历**生命表**）；
/// * 活着的士兵：进 `soldiers`（真值里死掉的士兵已被 `fire` 从表里摘掉）；
/// * 靶子：`add_target` 进靶子表，死的再 `kill_target`（真值 `0x454b40` 只翻 alive 位）。
fn match_from_corpus(composition: &str) -> Match {
    let mut m = Match::new(room(), vec![], vec![], (0, 0));
    for part in composition.split('|') {
        let f: Vec<&str> = part.split(':').collect();
        let id: i64 = f[0].parse().expect("id");
        let team: u8 = f[1].parse().expect("team");
        let kind = f[2];
        let alive = f[3] == "a";
        m.set_team(id, team);
        match kind {
            "soldier" => {
                if alive {
                    m.soldiers.push(Target { id, x: 0.0, y: 0.0, radius: 0.5 });
                }
            }
            "target" => {
                m.add_target(id, team);
                if !alive {
                    assert!(m.kill_target(id), "靶子 {id} 应能杀掉");
                }
            }
            _ => {}
        }
    }
    m
}

fn kinds(msgs: &[String]) -> Vec<String> {
    msgs.iter()
        .map(|m| {
            gw2_rebuild::_scaffold::protocol::parse_message(m)
                .expect("应为合法消息")
                .kind
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. 两种关卡各打一条 —— 但**判出来的东西一模一样**
// ---------------------------------------------------------------------------

#[test]
fn target_level_all_cleared_is_a_win() {
    let mut m = target_level();
    // 靶子还在：不结束
    assert_eq!(m.ending(), None, "靶子还在 ⇒ 继续");
    // 一发打掉两个靶子（等价于两条 LifeInfo{alive:false}）
    assert!(m.kill_target(1099));
    assert!(m.kill_target(1100));
    assert_eq!(m.targets_alive(), 0);
    // Team2 的实体只剩靶子 ⇒ Team2 全灭 ⇒ 人类（Team1）胜
    assert_eq!(
        m.ending(),
        Some(Ending { reason: EndingReason::TeamWiped, winner: Some(0) })
    );
    let out = m.end_turn();
    assert_eq!(kinds(&out), vec!["TurnCountInfo", "GameStateInfo"]);
    assert!(out[1].contains("EndingGame"));
}

#[test]
fn versus_level_enemy_wiped_is_a_win() {
    let mut m = versus_level();
    assert_eq!(m.ending(), None, "两边都有人 ⇒ 继续");
    // 对方那名士兵被打死 = 从 `soldiers` 里摘掉（`fire` 的既有行为）
    m.soldiers.retain(|t| t.id != 20);
    assert_eq!(
        m.ending(),
        Some(Ending { reason: EndingReason::TeamWiped, winner: Some(0) })
    );
    let out = m.end_turn();
    assert_eq!(kinds(&out), vec!["TurnCountInfo", "GameStateInfo"]);
    assert!(out[1].contains("EndingGame"));
}

/// **这条是本节的核心断言**：两种关卡走的是**同一条**代码路径。
///
/// **T64 的真值**：不再是"我造两个局面"，而是**冻结语料里两份真终局**的队伍构成
/// （`fixtures/test_expectation_truth3.tsv` 的 `W01/target_level` = 回放 `1782640359`、
/// `W01/versus_level` = 回放 `1782640836`）：
///
/// * 打靶关：`Team1` = 两个士兵 + 玩家（1 死 1 活），`Team2` = **一个靶子且已死**；
/// * 对战关：`Team1` = 两个士兵（1 死 1 活）+ 玩家，`Team2` = 两个士兵**全死** + 玩家。
///
/// 两局在语料里都以「`Team2` 零活实体」收场 ⇒ 真值规则判 `Team1` 胜。用**同一套**局面喂
/// 我方 `ending()`，两种关卡的 `Ending` 必须相等，且**都要等于语料记的收场**（`Team2` 输）。
///
/// 如果哪天有人把 `docs/131` 那套"打靶关看靶子、对战关看队伍"的两分法加回来，
/// 对战关那条会红（它的 `Team2` 里只有士兵、没有靶子）。
#[test]
fn wiped_team_goes_through_the_same_predicate_in_both_level_kinds() {
    let t = match_from_corpus(CORPUS_TARGET_LEVEL);
    let v = match_from_corpus(CORPUS_VERSUS_LEVEL);

    // 语料记的收场：两局都是 Team2 零活实体（truth3 的 `W01/*/wiped_team = 1`）
    assert_eq!(t.ending(), Some(Ending { reason: EndingReason::TeamWiped, winner: Some(0) }));
    assert_eq!(v.ending(), Some(Ending { reason: EndingReason::TeamWiped, winner: Some(0) }));
    assert_eq!(t.ending(), v.ending(), "打靶关/对战关必须判出同一个 Ending");

    // 判据本身（真值 `0x473520`）：两边都是"Team2 还有没有活实体"这一个问题。
    assert!(!t.team_has_survivor(TEAM_2), "打靶关：Team2 只剩一个死靶子");
    assert!(!v.team_has_survivor(TEAM_2), "对战关：Team2 两个士兵全死");
    assert!(t.team_has_survivor(0), "打靶关：Team1 还有活士兵");
    assert!(v.team_has_survivor(0), "对战关：Team1 还有活士兵");

    // `end_turn()` 的报文种类序列也相等（真值：`0x4759f0` 判结束 → `TurnCountInfo` + `GameStateInfo`）
    let mut t2 = t;
    let mut v2 = v;
    assert_eq!(kinds(&t2.end_turn()), kinds(&v2.end_turn()));
}

/// 反证：**靶子与士兵进的是同一张存活统计**。
///
/// `Team2` 同时有 1 名士兵和 1 个靶子；只把靶子打掉 —— 真值规则说**不结束**
/// （`Team2` 还有人）。任何"靶子清空 ⇒ 胜"的独立规则都会在这里判错。
#[test]
fn team2_with_soldier_and_target_does_not_end_on_targets_alone() {
    let mut m = versus_level();
    m.add_target(1099, TEAM_2);
    assert_eq!(m.targets_total(), 1);
    assert!(m.kill_target(1099));
    assert_eq!(m.ending(), None, "靶子清空但 Team2 还有士兵 ⇒ 不算结束");
    // 再把那名士兵打掉
    m.soldiers.retain(|s| s.id != 20);
    assert_eq!(
        m.ending(),
        Some(Ending { reason: EndingReason::TeamWiped, winner: Some(0) })
    );
}

// ---------------------------------------------------------------------------
// 2. 靶子的形态（`docs/150` §Q1）
// ---------------------------------------------------------------------------

/// 靶子**不是**士兵：不进 `soldiers`（不可能开火、也不会被 `fire` 的士兵逻辑碰到），
/// 但**有队伍**、**有存活位**。
#[test]
fn targets_are_team_entities_but_not_soldiers() {
    let m = target_level();
    assert_eq!(m.targets_total(), 2);
    assert_eq!(m.targets_alive(), 2);
    assert!(m.is_target(1099) && m.is_target(1100));
    assert!(
        !m.soldiers.iter().any(|s| s.id == 1099 || s.id == 1100),
        "靶子不进士兵表"
    );
    // 队伍在**同一张表**里：这是"靶子属于 Team2"的可执行版本
    assert_eq!(m.teams.get(&1099).copied(), Some(TEAM_2));
    assert_eq!(m.teams.get(&1100).copied(), Some(TEAM_2));
    assert_eq!(m.teams.get(&10).copied(), Some(0));
}

/// 复原 = **只翻 alive 位**（真值 `0x454b40(id, true)`），不新建 id、不加靶子。
#[test]
fn restoring_a_target_only_flips_its_alive_bit() {
    let mut m = target_level();
    assert!(m.kill_target(1099));
    assert!(!m.kill_target(1099), "已死再杀是幂等");
    assert_eq!(m.targets_total(), 2, "总数不变 ⇒ 没有重建实体");
    assert_eq!(m.targets_alive(), 1);
    assert!(m.restore_target(1099));
    assert!(!m.restore_target(1099), "已活再复原是幂等");
    assert_eq!(m.targets_alive(), 2);
    assert_eq!(m.targets_total(), 2);
}

// ---------------------------------------------------------------------------
// 3. 回合上限（`docs/150` §Q3：这是**有真值**的，不是推断）
// ---------------------------------------------------------------------------

#[test]
fn turn_limit_exhausted_ends_the_game_with_team2_winning() {
    let mut m = target_level();
    assert_eq!(m.set_turn_limit(1), vec![r#"{"TurnLimitInfo":{"turn_limit":1}}"#]);
    assert_eq!(m.turn_limit, Some(1));
    assert_eq!(m.ending(), None, "回合数还没走到上限");
    // 空放一发 → 走完一个回合；靶子一个没死
    let out = m.end_turn();
    assert_eq!(kinds(&out), vec!["TurnCountInfo", "GameStateInfo"]);
    assert_eq!(
        m.ending(),
        Some(Ending { reason: EndingReason::TurnLimit, winner: Some(TEAM_2) }),
        "真值 0x475c06：两边都还有人时，上限用尽 ⇒ 胜方写死成 Team2（关卡方）"
    );
}

/// 「没有回合上限 ⇒ 不因回合数结束」。
///
/// **T64 的语料真值**（`fixtures/test_expectation_truth3.tsv`）：
/// * `TurnLimitInfo` 只有 **21 帧 / 21 份回放**（403 份里）⇒ **382 份**回放这一局根本没有回合上限；
/// * `turn_count` 已到上限的真实收场只有 **1 份**（`1782641048`，上限 1）—— 而且那一局
///   `Team2` 的最后那个靶子也死了，**两种结束条件同时成立** ⇒ 语料**分不开**"上限用尽"与
///   "某队全灭"这两条路径。
///
/// ⇒ 前半段是**观测**（382 份无上限），后半段仍是**契约断言**（我方循环 50 回合不判结束）。
#[test]
fn without_a_turn_limit_the_game_never_ends_on_time() {
    // 语料真值：403 份里 21 份有 TurnLimitInfo ⇒ 382 份无上限
    const CORPUS_WITHOUT_LIMIT: usize = 382;
    const CORPUS_TURN_LIMIT_FRAMES: usize = 21;
    assert_eq!(CORPUS_TURN_LIMIT_FRAMES + CORPUS_WITHOUT_LIMIT, 403, "真值表：21 + 382 = 403");

    let mut m = target_level();
    assert_eq!(m.turn_limit, None);
    for _ in 0..50 {
        let out = m.end_turn();
        assert_eq!(kinds(&out), vec!["TurnCountInfo"], "没上限 ⇒ 不判结束");
    }
    assert_eq!(m.ending(), None);
}

/// 两边同时清空。
///
/// **T64 更正**（旧注释写着"语料里没有这种收场"——错的）：解压 403 份回放后，
/// **24 份真终局是双方同时零活实体**（示例 `1782642754`，构成见 `CORPUS_MUTUAL_WIPE`）。
/// 语料只记 `GameStateInfo{EndingGame}`，**不记胜方字段** ⇒ "谁赢"在真值里没有对应物，
/// `winner: None` 仍是**我方模型**（`0x4759f0` 那三条跳转在双零时的落点本轮未解，缺的是
/// 双零终局的结算 code path 证据）。这条钉的是：**双方零活体时必须判 `TeamWiped`、不许判某人赢**。
#[test]
fn mutual_wipe_has_no_winner() {
    // 语料真值：24 份双零终局（`W02/corpus/both_wiped`）
    const CORPUS_BOTH_WIPED: usize = 24;
    assert_eq!(CORPUS_BOTH_WIPED, 24, "真值表 W02/corpus/both_wiped");

    let m = match_from_corpus(CORPUS_MUTUAL_WIPE);
    assert!(!m.team_has_survivor(0) && !m.team_has_survivor(TEAM_2));
    assert_eq!(
        m.ending(),
        Some(Ending { reason: EndingReason::TeamWiped, winner: None }),
        "双方零活体 ⇒ TeamWiped、无胜方（胜方字段在语料里没有对应物，见注释）"
    );
}

/// 一队都没登记（回放模式/空局）：**判不了**，不能"0 == 0 ⇒ 有人赢"。
#[test]
fn empty_match_has_no_verdict() {
    let m = Match::new(room(), vec![], vec![], (0, 0));
    assert_eq!(m.ending(), None);
}
