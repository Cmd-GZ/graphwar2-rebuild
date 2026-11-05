//! 回合推进规则。
//!
//! 来源：真实对局的网络报文（`TurnInfo` 序列 + `SoldierInfo`/`TeamInfo` 归属）。
//!
//! ## 已验证的规则
//!
//! **队伍之间严格交替。** 四局独立抓包的 `TurnInfo.soldier_id` 序列
//! （`230/231` 属 Team1，`233/234` 属 Team2）全部满足"相邻两次出手来自不同队伍"：
//!
//! ```text
//! 局 A: 230(T1) 234(T2) 231(T1) 233(T2) ...
//! 局 B: 234(T2) 231(T1) 233(T2) 230(T1) ...
//! 局 C: 233(T2) 231(T1) 234(T2) 230(T1) ...
//! ```
//!
//! ## 未验证（明确标注）
//!
//! * **起始队伍**：七局里有从 Team1 起也有从 Team2 起的，规则未知；
//! * **队内顺序**：七局互不一致（`234` 后面有时接 `231`、有时接 `230`），
//!   因此**放弃推导**——只实现"交替"，顺序由调用方给出；
//! * 士兵阵亡后如何从循环里移除（没有同局内的死亡样本）；
//! * `SimultaneousTurns`（并发回合）模式的顺序——只观测到 `SequentialTurns`。

/// 按"每队给定的出场顺序"生成交替回合序列。
///
/// `teams[i]` 是第 `i` 队的士兵 id 列表（顺序由调用方给出，见模块注释：
/// 队内顺序尚未确定，所以不自作主张排序）。
/// `start_team` 指定从哪一队开始。
pub fn alternating_order(teams: &[Vec<i64>], start_team: usize, rounds: usize) -> Vec<i64> {
    let usable: Vec<&Vec<i64>> = teams.iter().filter(|t| !t.is_empty()).collect();
    if usable.is_empty() || rounds == 0 {
        return Vec::new();
    }
    let mut cursor: Vec<usize> = vec![0; usable.len()];
    let mut out = Vec::with_capacity(rounds);
    let mut t = start_team % usable.len();
    while out.len() < rounds {
        let members = usable[t];
        out.push(members[cursor[t] % members.len()]);
        cursor[t] += 1;
        t = (t + 1) % usable.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实抓包里的士兵归属：`230/231` 属 Team1（player 229），`233/234` 属 Team2（player 232）。
    fn team_of(id: i64) -> u8 {
        match id {
            230 | 231 => 1,
            233 | 234 => 2,
            _ => 0,
        }
    }

    /// 七局真实对局里抓到的 `TurnInfo` 序列（去掉相邻重复，单方向流）。
    ///
    /// 注意：**队内顺序每局都不一样**（例如 `234` 后面有时接 `231`、有时接 `230`），
    /// 所以这里只断言"队伍交替"，不断言具体顺序。
    const CAPTURED: [&[i64]; 7] = [
        &[230, 234, 231, 233, 230, 234, 231, 233, 230],
        &[234, 231, 233, 230, 234, 231, 233, 230],
        &[233, 231, 234, 230, 233, 231, 233, 231, 234, 230],
        &[234, 231, 233, 230, 234, 231, 234, 231, 233, 230],
        &[231, 234, 231, 233, 231],
        &[234, 230, 233, 231],
        &[231, 233, 230],
    ];

    /// 核心结论：**相邻两次出手来自不同队伍**。
    #[test]
    fn captured_turns_always_alternate_teams() {
        for (i, seq) in CAPTURED.iter().enumerate() {
            for w in seq.windows(2) {
                assert_ne!(
                    team_of(w[0]),
                    team_of(w[1]),
                    "第 {i} 局的序列 {:?} 在 {} -> {} 处没有交替",
                    seq,
                    w[0],
                    w[1]
                );
            }
        }
    }

    /// 给每队指定顺序后，交替序列能复现抓到的形态（这里用局 A）。
    #[test]
    fn reproduces_case_a_with_explicit_team_order() {
        let teams = vec![vec![230, 231], vec![234, 233]];
        let got = alternating_order(&teams, 0, 9);
        assert_eq!(got, vec![230, 234, 231, 233, 230, 234, 231, 233, 230]);
    }

    /// 从 Team2 起手的局（局 B 形态）。
    #[test]
    fn reproduces_case_b_starting_from_team2() {
        let teams = vec![vec![231, 230], vec![234, 233]];
        let got = alternating_order(&teams, 1, 8);
        assert_eq!(got, vec![234, 231, 233, 230, 234, 231, 233, 230]);
    }

    #[test]
    fn single_team_just_cycles() {
        let teams = vec![vec![5, 2]];
        assert_eq!(alternating_order(&teams, 0, 5), vec![5, 2, 5, 2, 5]);
    }

    #[test]
    fn empty_input_is_safe() {
        assert!(alternating_order(&[], 0, 5).is_empty());
        assert!(alternating_order(&[vec![], vec![]], 0, 5).is_empty());
        assert!(alternating_order(&[vec![1]], 0, 0).is_empty());
    }
}
