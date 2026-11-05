//! tick 时钟：`current_tick` 与真实时间的关系。
//!
//! 依据：真实对局抓包（`fixtures/ticks.tsv`，113 个 `TickReply` 样本，
//! 每个样本都有抓包时间戳与 `current_tick` 值）。
//!
//! 拟合结果是**精确的 60 ticks/秒**：
//!
//! ```text
//! current_tick = base_tick + round((now_ms - base_ms) * 60 / 1000)
//! ```
//!
//! 113 个样本里 110 个完全吻合，2 个差 1，1 个差 7
//! —— 差异来自抓包时间戳的抖动（socket 缓冲会让时间戳晚于真实发送时刻），
//! 不是公式本身的问题。
//!
//! 还没确定：`TickRequest` 由客户端按什么周期发出（实测间隔噪声很大：
//! 1ms/45ms/955ms 混杂），以及服务端何时推进状态机。

/// 一秒多少个 tick（实测）。
pub const TICKS_PER_SECOND: i64 = 60;

/// 由基准点推算 `now_ms` 时刻的 tick 值。
pub fn tick_at(base_tick: i64, base_ms: i64, now_ms: i64) -> i64 {
    let dt = now_ms - base_ms;
    let ticks = (dt as f64) * (TICKS_PER_SECOND as f64) / 1000.0;
    base_tick + ticks.round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实抓包样本：`(距基准的毫秒数, current_tick)`。
    fn samples() -> Vec<(i64, i64)> {
        include_str!("../fixtures/ticks.tsv")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let mut it = l.split('\t');
                (it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap())
            })
            .collect()
    }

    #[test]
    fn sixty_hertz_holds_for_captured_samples() {
        let s = samples();
        assert!(s.len() > 100, "样本太少");
        let base_tick = s[0].1;
        let mut exact = 0;
        let mut worst = 0i64;
        for (ms, tick) in s {
            let pred = tick_at(base_tick, 0, ms);
            let err = (pred - tick).abs();
            if err == 0 {
                exact += 1;
            }
            worst = worst.max(err);
        }
        // 抓包时间戳有抖动，所以允许个别偏差；但绝大多数必须精确命中
        assert!(exact >= 105, "精确命中 {exact} 个，低于预期（应为 ~110）");
        assert!(worst <= 8, "最大偏差 {worst} 过大，公式可能不对");
    }

    #[test]
    fn rate_is_sixty_per_second() {
        assert_eq!(tick_at(0, 0, 1000), 60);
        assert_eq!(tick_at(0, 0, 2000), 120);
        assert_eq!(tick_at(5, 0, 500), 35, "0.5 秒 = 30 tick");
        assert_eq!(tick_at(0, 1000, 0), -60, "负时间也要线性");
    }
}

/// 时间模式对应的回合时长（tick 数）。
///
/// **已验证**：`Timer3m` = 10800（180 秒 × 60）。依据是真实抓包——
/// 若 `end_of_turn_tick = 回合开始 tick + 10800`，则 `end_of_turn_tick - 10800`
/// 应等于回合开始时的 `current_tick`；实测该差值与当时的 `current_tick`
/// 相差 0~60（见 `docs/42`）。
///
/// **推断**（未验证）：其余档位按"名字里的时间 × 60"给出。
pub fn turn_duration_ticks(time_mode: &str) -> Option<i64> {
    Some(match time_mode {
        "Timer30s" => 30 * TICKS_PER_SECOND,
        "Timer1m" => 60 * TICKS_PER_SECOND,
        "Timer2m" => 120 * TICKS_PER_SECOND,
        "Timer3m" => 180 * TICKS_PER_SECOND, // 已验证
        "Timer5m" => 300 * TICKS_PER_SECOND,
        _ => return None, // TimerInf / 未知模式：没有截止时刻
    })
}

/// 回合截止 tick = 回合开始时的 tick + 时长。
pub fn end_of_turn_tick(turn_start_tick: i64, time_mode: &str) -> Option<i64> {
    turn_duration_ticks(time_mode).map(|d| turn_start_tick + d)
}

#[cfg(test)]
mod end_of_turn_tests {
    use super::*;

    /// 真实抓包里的 `(当时 current_tick, end_of_turn_tick, time_mode)`。
    const SAMPLES: [(i64, i64, &str); 5] = [
        (65, 10885, "Timer3m"),
        (606, 11432, "Timer3m"),
        (846, 11680, "Timer3m"),
        (1086, 11945, "Timer3m"),
        (1146, 11945, "Timer3m"),
    ];

    #[test]
    fn end_of_turn_is_start_plus_duration() {
        for (cur, end, mode) in SAMPLES {
            let implied_start = end - turn_duration_ticks(mode).unwrap();
            let delta = implied_start - cur;
            // 抓包时刻与"回合开始"不完全同步，允许一秒（60 tick）的偏差
            assert!(
                delta.abs() <= 60,
                "mode={mode} end={end} cur={cur} 推出的开始 tick={implied_start}，偏离 {delta}"
            );
        }
    }

    #[test]
    fn timer3m_is_10800_ticks() {
        assert_eq!(turn_duration_ticks("Timer3m"), Some(10800));
        assert_eq!(turn_duration_ticks("Timer1m"), Some(3600));
        assert_eq!(turn_duration_ticks("TimerInf"), None);
        assert_eq!(end_of_turn_tick(100, "Timer3m"), Some(10900));
    }
}
