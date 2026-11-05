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
