//! `src/_scaffold/ai_explain.rs` 的验收。
//!
//! ⚠️ 按 `AGENTS.md` §0.2：这些断言是**回归保护**，不是复原证据。
//! 真正决定行为对错的是二进制（地址都写在 `docs/147` 与代码注释里）。
//!
//! 三条主线：
//! 1. **确定性** —— 同一局面 + 同一种子 ⇒ 同一条函数；
//! 2. **难度真的会改决策** —— 难度值改档 ⇒ 参数改、过程改、结果改；
//! 3. **机制本身对** —— 友军算负分、障碍截断弹道、粗评/精评会分歧。

use gw2_rebuild::_scaffold::ai_explain::{
    self, evaluate, is_friendly, Entity, Settings, Situation, TEAM_UNKNOWN,
};
use gw2_rebuild::graphwar_function_evolutionator::{difficulty_params, params, VarMode, DIST_SENTINEL};
use gw2_rebuild::{Function, Token};

fn const_fn(v: f64) -> Vec<Token> {
    vec![Token::constant(v)]
}

// ---------------------------------------------------------------------------
// 1. 确定性
// ---------------------------------------------------------------------------

#[test]
fn same_situation_and_seed_give_the_same_decision() {
    let sit = ai_explain::demo_situation();
    let settings = Settings::from_difficulty(25, VarMode::Normal);
    let a = ai_explain::explain(&sit, &settings, 0xDEAD_BEEF);
    let b = ai_explain::explain(&sit, &settings, 0xDEAD_BEEF);

    assert_eq!(a.winner.text, b.winner.text, "同局面同种子必须选出同一条函数");
    assert_eq!(a.winner.tokens, b.winner.tokens, "token 逐位相同");
    assert_eq!(a.evaluations, b.evaluations, "评分次数也必须相同");
    assert_eq!(a.generations.len(), b.generations.len());
    // 逐代冠军也应当一一对应
    for (ga, gb) in a.generations.iter().zip(b.generations.iter()) {
        assert_eq!(ga.champion.text, gb.champion.text, "第 {} 代冠军不同", ga.index);
    }
}

#[test]
fn different_seed_can_change_the_decision() {
    // 不是"必然不同"（适应度会让它们常常收敛到同一类函数），
    // 但整条轨迹（每代冠军）在 40 代里全同的概率极低。
    let sit = ai_explain::demo_situation();
    let settings = Settings::from_difficulty(50, VarMode::Normal);
    let a = ai_explain::explain(&sit, &settings, 1);
    let b = ai_explain::explain(&sit, &settings, 2);
    let identical = a
        .generations
        .iter()
        .zip(b.generations.iter())
        .all(|(x, y)| x.champion.text == y.champion.text);
    assert!(!identical, "换种子后整条轨迹一字不差，说明种子没接进 RNG");
}

// ---------------------------------------------------------------------------
// 2. 难度
// ---------------------------------------------------------------------------

/// 逐条对应 `0x3e3d95`–`0x3e45a3` 的分档（见 `graphwar_function_evolutionator::difficulty_params`）。
#[test]
fn difficulty_bands_match_the_binary() {
    // (难度值, 代数, 种群, 上限)
    let cases: &[(u32, u32, u32, u32)] = &[
        (0, 0, 50, 5),
        (3, 3, 50, 5),     // 关 5 的难度值
        (4, 4, 100, 5),    // 刚跨档
        (10, 10, 100, 5),
        (11, 11, 150, 10), // 刚跨档
        (25, 25, 150, 10), // 关 9 的难度值
        (26, 26, 200, 15), // 刚跨档
        (50, 50, 200, 15), // 关 11 的难度值：六档表里没有这一组
        (100, 100, 200, 15),
        (101, 101, 250, 25), // 刚跨档
        (250, 250, 250, 25), // 关 15 的难度值
        (251, 251, 250, 200),
        (1000, 1000, 250, 200),
    ];
    for &(d, gens, pop, limit) in cases {
        let p = difficulty_params(d);
        assert_eq!(p.generations, gens, "难度 {d} 的代数");
        assert_eq!(p.population, pop, "难度 {d} 的种群");
        assert_eq!(p.per_slot, 5, "每槽个体数六档恒为 5");
        assert_eq!(p.limit, limit, "难度 {d} 的复杂度上限");
    }
}

/// 六档表（`docs/AI.md` §1）是"某一个难度值下测出来的组合"，
/// 所以它必然等于 `difficulty_params` 在**若干特定难度值**上的取值。
#[test]
fn six_tier_table_is_a_sample_of_the_banded_mapping() {
    for (level, d) in [(1u8, 3u32), (2, 10), (3, 25), (4, 100), (5, 250), (6, 1000)] {
        assert_eq!(params(level), difficulty_params(d), "第 {level} 档应对应难度值 {d}");
    }
}

/// 关 12 的 `50` 不是笔误：它是"代数 = 难度值"，落在 `26..=100` 那一档。
#[test]
fn campaign_value_50_is_generations_not_a_tier_index() {
    let p = difficulty_params(50);
    assert_eq!((p.generations, p.population, p.limit), (50, 200, 15));
    // 它在六档表里**没有对应行**——这正是 docs/118 §12.4 记的那个"对不上"
    assert!(
        !(1..=6).any(|l| params(l) == p),
        "难度值 50 不该等于六档表里任何一档"
    );
}

#[test]
fn different_difficulty_changes_the_decision_process() {
    let sit = ai_explain::demo_situation();
    let easy = Settings::from_difficulty(3, VarMode::Normal); // 3 代 × 50
    let hard = Settings::from_difficulty(250, VarMode::Normal); // 250 代 × 250
    let a = ai_explain::explain(&sit, &easy, 42);
    let b = ai_explain::explain(&sit, &hard, 42);

    assert_ne!(a.params, b.params, "难度不同 ⇒ 参数必须不同");
    assert_ne!(a.evaluations, b.evaluations, "难度不同 ⇒ 评分次数必须不同");
    // 决策过程/结果至少有一处不同：3 代与 250 代不可能给出同一条轨迹
    let different = a.generations.len() != b.generations.len()
        || a.winner.text != b.winner.text
        || a.generations.last().map(|g| g.champion.text.as_str())
            != b.generations.last().map(|g| g.champion.text.as_str())
        || a.generations.last().map(|g| g.champion.tokens.clone())
            != b.generations.last().map(|g| g.champion.tokens.clone());
    assert!(different, "两个难度给出了完全相同的决策轨迹");
}

// ---------------------------------------------------------------------------
// 3. 机制
// ---------------------------------------------------------------------------

/// tag 2 的友军/敌人分流（`0x51ae21`–`0x51ae5a`）。
#[test]
fn friendly_versus_enemy_classification() {
    assert!(is_friendly(0, 0));
    assert!(is_friendly(1, 1));
    assert!(!is_friendly(0, 1));
    assert!(!is_friendly(1, 0));
    // 两边都查不到 ⇒ 按友军（dec）
    assert!(is_friendly(TEAM_UNKNOWN, TEAM_UNKNOWN));
    // 只有实体查不到 ⇒ 按敌人（inc）
    assert!(!is_friendly(TEAM_UNKNOWN, 0));
    // 只有射手查不到 ⇒ 按敌人
    assert!(!is_friendly(0, TEAM_UNKNOWN));
}

#[test]
fn shooting_a_teammate_costs_exactly_as_much_as_a_kill_gains() {
    let mut sit = Situation::empty(0.0, 0.0, false, 0, 15);
    sit.entities.push(Entity::soldier(1, 10.0, 0.0, 0.5, 1)); // 敌人
    sit.entities.push(Entity::soldier(2, 20.0, 0.0, 0.5, 0)); // 自己人
    let line = const_fn(0.0); // f ≡ 0 ⇒ 弹道就是 y = 0

    // 只打到敌人
    let mut only_enemy = sit.clone();
    only_enemy.entities.retain(|e| e.id == 1);
    let s = evaluate(&line, &only_enemy, 0.5);
    assert_eq!((s.kills, s.friendly), (1, 0));
    assert!((s.score - (1_000_000.0 - 0.01)).abs() < 1.0,
        "命中一个敌人 ≈ +1e6（2e6 − dist 1e6 − V²/100），实得 {}", s.score);

    // 只打到自己人：同样的 2e6，符号相反
    let mut only_friend = sit.clone();
    only_friend.entities.retain(|e| e.id == 2);
    let s = evaluate(&line, &only_friend, 0.5);
    assert_eq!((s.kills, s.friendly), (0, 1));
    assert!(s.score < -1_000_000.0, "误伤必须低于'什么都没打到'的基准分，实得 {}", s.score);

    // 两个都打到 ⇒ 净命中 0，只剩 −1e6 的基准
    let s = evaluate(&line, &sit, 0.5);
    assert_eq!((s.kills, s.friendly, s.net_hits()), (1, 1, 0));
    assert!((s.score - (-1_000_000.0 - 0.01)).abs() < 1e-6, "净 0 时应当只剩基准价，实得 {}", s.score);
}

#[test]
fn an_obstacle_cuts_the_trajectory_short() {
    let mut open = Situation::empty(0.0, 0.0, false, 0, 15);
    open.entities.push(Entity::soldier(1, 10.0, 0.0, 0.5, 1));
    let line = const_fn(0.0);

    let s = evaluate(&line, &open, 0.5);
    assert_eq!(s.kills, 1, "没有障碍时应当打到 10 格外的敌人");

    let mut walled = open.clone();
    walled.add_obstacle(5.0, 0.0, 3.0); // 挡在中间（与真开火同一个光栅化器）
    let s = evaluate(&line, &walled, 0.5);
    assert_eq!(s.kills, 0, "障碍必须把弹道截断（0x3c3957 光栅化 ⇒ 逐点遮挡）");
    assert!(s.score < -900_000.0);
}

/// 粗评（0.5）会**漏掉**只有精评（0.01）才看得到的命中 ——
/// 这就是"前 3 名必须再精评一次"的理由（`0x51c3b8`–`0x51c3e4`）。
#[test]
fn coarse_step_can_miss_a_hit_that_the_fine_step_sees() {
    let mut sit = Situation::empty(0.0, 0.0, false, 0, 15);
    // 半径 0.2，位置正好卡在粗评两个采样点（0.5 与 1.0）之间
    sit.entities.push(Entity::soldier(1, 0.75, 0.0, 0.2, 1));
    let line = const_fn(0.0);

    let coarse = evaluate(&line, &sit, 0.5);
    let fine = evaluate(&line, &sit, 0.01);
    assert_eq!(coarse.kills, 0, "步长 0.5 时采样点离靶心 0.25 > 半径 0.2，应当漏掉");
    assert_eq!(fine.kills, 1, "步长 0.01 时正好扫到，应当命中");
}

/// 距离项只在**靶**（tag 3）上更新；士兵只走命中数（tag 2）那一支。
#[test]
fn the_distance_term_only_tracks_targets() {
    let mut sit = Situation::empty(0.0, 0.0, false, 0, 15);
    sit.entities.push(Entity::soldier(1, 10.0, 0.0, 0.5, 1));
    let line = const_fn(0.0);
    let s = evaluate(&line, &sit, 0.5);
    assert_eq!(s.kills, 1);
    assert_eq!(s.dist, DIST_SENTINEL, "打士兵不动距离项");

    let mut with_target = Situation::empty(0.0, 0.0, false, 0, 15);
    with_target.entities.push(Entity::target(9, 10.0, 0.0, 0.5, 2));
    let s = evaluate(&line, &with_target, 0.5);
    assert_eq!(s.kills, 0, "靶不进命中数");
    assert!(s.dist < 0.5, "靶会更新距离项，实得 {}", s.dist);
}

/// 超复杂度上限吃 −5e7（`0x51fb0f`–`0x51b100`），而且它**只是扣分不是过滤**。
#[test]
fn over_limit_is_a_penalty_not_a_filter() {
    // 场上一无所有，这样分数里只剩基准项与惩罚项
    let sit = Situation::empty(0.0, 0.0, false, 0, 3); // 上限 3
    let long = vec![
        Token::op(Function::Add),
        Token::constant(0.0),
        Token::op(Function::Mul),
        Token::constant(0.0),
        Token::op(Function::VarX),
    ];
    let s = evaluate(&long, &sit, 0.5);
    assert!(s.score < -49_000_000.0, "超上限必须吃满 −5e7，实得 {}", s.score);
    // 仍然会被算出一个分数（不过滤），所以它依然能参与排序
    assert!(s.score.is_finite());
}

/// 一回合的评分次数 = 代数 × (种群 + 3)，与 `docs/97` §6 的运行时实测一致。
#[test]
fn evaluations_per_generation_match_the_binary() {
    let sit = ai_explain::demo_situation();
    let settings = Settings {
        max_generations: Some(3),
        ..Settings::from_difficulty(25, VarMode::Normal)
    };
    let r = ai_explain::explain(&sit, &settings, 5);
    assert_eq!(r.generations.len(), 3);
    assert_eq!(r.evaluations, 3 * (150 + 3), "每代 种群(150)+3 次");
}
