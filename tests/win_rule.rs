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

fn kinds(msgs: &[String]) -> Vec<String> {
    msgs.iter()
        .map(|m| {
            gw2_rebuild::protocol::parse_message(m)
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
/// 断言方式：把"Team2 里最后一个实体消失"这件事分别用**靶子**和**士兵**做一遍，
/// 得到的 `Ending` **完全相等**（同一个 `reason`、同一个 `winner`），
/// 而 `end_turn()` 产出的报文**种类序列也完全相等**。
///
/// 如果哪天有人把 `docs/131` 那套"打靶关看靶子、对战关看队伍"的两分法加回来，
/// 这条仍然会绿（那时两边也各判一次），所以**再补**下面 `mixed_*` 那条作反证：
/// 混合队伍（Team2 = 1 士兵 + 1 靶子）时，两分法会"靶子清空就判胜"，
/// 而真值规则要求"该队还有活实体就不算完"。
#[test]
fn wiped_team_goes_through_the_same_predicate_in_both_level_kinds() {
    let mut t = target_level();
    t.kill_target(1099);
    t.kill_target(1100);
    let mut v = versus_level();
    v.soldiers.retain(|s| s.id != 20);

    assert_eq!(t.ending(), v.ending(), "打靶关/对战关必须判出同一个 Ending");
    assert_eq!(kinds(&t.end_turn()), kinds(&v.end_turn()));

    // 判据本身（真值 `0x473520`）：两边都是"Team2 还有没有活实体"这一个问题。
    assert!(!t.team_has_survivor(TEAM_2));
    assert!(!v.team_has_survivor(TEAM_2));
    assert!(t.team_has_survivor(0));
    assert!(v.team_has_survivor(0));
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

#[test]
fn without_a_turn_limit_the_game_never_ends_on_time() {
    let mut m = target_level();
    assert_eq!(m.turn_limit, None);
    for _ in 0..50 {
        let out = m.end_turn();
        assert_eq!(kinds(&out), vec!["TurnCountInfo"], "没上限 ⇒ 不判结束");
    }
    assert_eq!(m.ending(), None);
}

/// 两边同时清空：真值写 `2`（没人赢），语料里没有这种收场 ⇒ 我们返回 `winner: None`。
#[test]
fn mutual_wipe_has_no_winner() {
    let mut m = versus_level();
    m.soldiers.clear();
    assert_eq!(
        m.ending(),
        Some(Ending { reason: EndingReason::TeamWiped, winner: None })
    );
}

/// 一队都没登记（回放模式/空局）：**判不了**，不能"0 == 0 ⇒ 有人赢"。
#[test]
fn empty_match_has_no_verdict() {
    let m = Match::new(room(), vec![], vec![], (0, 0));
    assert_eq!(m.ending(), None);
}
