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

use crate::events::Event;

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
/// **已验证**（真实抓包，见 `docs/42`）：`Timer1m` / `Timer2m` / `Timer3m` / `Timer5m`
/// 分别是 `60 / 120 / 180 / 300` 秒 × 60 tick。验法：把 harness 的回合时间按钮按不同次数，
/// 抓 `EndOfTurnInfo` 与同时刻的 `current_tick`，其差值应为 `时长 - 采样偏移`。
///
/// `Timer30s` 也已实测（1800）。`TimerInf` 表示无限时，返回 `None`；
/// **注意**：线上它把 `end_of_turn_tick` 发成 `0`（实测 `end - current_tick = -66`，
/// 而当时 `current_tick = 66`），所以写报文时要用 `end_of_turn_tick_wire`。
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

/// 线上表示：有限时给截止 tick，无限时（`TimerInf`）发 `0`。
pub fn end_of_turn_tick_wire(turn_start_tick: i64, time_mode: &str) -> i64 {
    end_of_turn_tick(turn_start_tick, time_mode).unwrap_or(0)
}

#[cfg(test)]
mod end_of_turn_tests {
    use super::*;

    /// 真实抓包里的 `(当时 current_tick, end_of_turn_tick, time_mode)`。
    ///
    /// `Timer1m/2m/5m` 三档是通过改变 harness 的回合时间按钮抓到的（见 docs/42），
    /// `Timer3m` 是默认档位。
    const SAMPLES: [(i64, i64, &str); 12] = [
        (65, 10885, "Timer3m"),
        (606, 11432, "Timer3m"),
        (1086, 11945, "Timer3m"),
        (66, 3686, "Timer1m"),
        (362, 4021, "Timer1m"),
        (61, 7288, "Timer2m"),
        (426, 7631, "Timer2m"),
        (66, 18085, "Timer5m"),
        (301, 18342, "Timer5m"),
        (1146, 11945, "Timer3m"),
        (66, 1866, "Timer30s"),
        (66, 0, "TimerInf"), // 无限时：线上发 0
    ];

    #[test]
    fn end_of_turn_is_start_plus_duration() {
        for (cur, end, mode) in SAMPLES {
            if mode == "TimerInf" {
                // 无限时：线上就是 0，不存在"开始 + 时长"
                assert_eq!(end, 0, "TimerInf 应发 0");
                assert_eq!(turn_duration_ticks(mode), None);
                continue;
            }
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
        assert_eq!(turn_duration_ticks("Timer30s"), Some(1800));
        assert_eq!(end_of_turn_tick_wire(100, "TimerInf"), 0, "无限时应发 0");
        assert_eq!(end_of_turn_tick_wire(100, "Timer30s"), 1900);
        assert_eq!(end_of_turn_tick(100, "Timer3m"), Some(10900));
    }
}

// ---------------------------------------------------------------------------
// 容器里那个类型：`GraphwarTickHandler`（2026-11 任务 19 的接线层）
// ---------------------------------------------------------------------------
//
// 本文件原来只有两个自由函数（`tick_at` / `turn_duration_ticks`）。容器里要装
// "72 字节的那个结构体"，所以这里补一个**类型**。字段只落**有地址证据**的那几个，
// **不**按字节复原（原版 72 字节，我方尺寸不等，`docs/16-client.md` §41.4 的口径）。
//
// 证据（x86 2026-08-28，全部 `观测`；`tools/addr_probe.py dis <vaddr>` 可复算）：
//
// ```text
// new        0x519690  alloc 0x60 = 16(Rc 头) + 8(RefCell) + 72(M)
//                      M+0x00..0x0f = SystemTime::now()（call 0x8f3a10）
//                      M+0x10 = 0 (u64) / M+0x18 = 0 (u32)   ← 后面被 `tick` 当 Duration 累加器
//                      M+0x28 = 0x3b9aca00 (1e9，一秒的纳秒数)
//                      M+0x38 = 参数传进来的 Rc<RefCell<GraphwarEventQueue>>
//                      M+0x40 = 0 (u32)                     ← `tick` 里 +1 的 tick 计数
//                      末尾一条 subscribe：'connected_to_server_event'（len 0x19，handler 0x520840）
//
// tick       0x519840  ① 把 M+0x10/+0x18 那对 (secs, nanos) 加上 0xfe502a = 16,666,666 ns
//                        （= 1e9/60 ⇒ 一 tick = 1/60 秒），溢出路径的串是
//                        'overflow when adding durations'（`Duration` 的加法，0x51990d）
//                      ② M+0x40 += 1（ebp），发 tag 0（`TickEvent`），payload [+8] = 新计数
//                      ③ (计数 % 60 == 0) 时**再发** tag 1（`TickRequest`）
//                        （幻数 `imul 0xeeeeeeef` + `ror 2` + `cmp 0x4444444` = 判 60 的整除）
//
// handle_connected_to_server_event  0x519630
//                      不看事件载荷，直接往队列里发 tag 1（`TickRequest`）
// ```
//
// **没有解出来的**（`未知`）：`get_fractional_tick`（0x519530）与
// `get_time_to_tick`（0x519450）的算式——两个函数都有落点，但体内公式没逐条读；
// 所以本类型**不**声称实现了那两个入口（缺的证据：逐指令读那两个函数）。

/// 本模块唯一订阅的事件名（`new` 末尾那条 `subscribe` 的字面量，长度 `0x19`）。
pub const SUBSCRIPTION: &'static str = "connected_to_server_event";

/// 一 tick 的纳秒数：`0xfe502a` = **16 666 666**（`tick` 里那条 `add eax, 0xfe502a`）。
///
/// 注意它**不是** `1e9/60` 的四舍五入（16 666 667），原版就是截断的 16 666 666；
/// 每 60 tick 少 40 ns，与原版的进位分支（`cmp eax, 0x3a9c79d6`）配套。
pub const NANOS_PER_TICK: u32 = 16_666_666;

/// 一秒的纳秒数：`0x3b9aca00`（`new` 里写到 `M+0x28` 的那个常量）。
pub const NANOS_PER_SECOND: u32 = 1_000_000_000;

/// 每多少个 tick 再发一条 `TickRequest`（`tick` 里那个"判 60 整除"的分支）。
pub const TICKS_PER_REQUEST: u32 = 60;

/// `GraphwarTickHandler` 的我方形状（**只落有地址证据的字段**）。
///
/// * `elapsed_nanos` ⇐ 原版 `M+0x10`(secs) / `M+0x18`(nanos) 那一对 `Duration` 累加器；
/// * `current_tick` ⇐ 原版 `M+0x40`（`tick` 里 `inc`、并作为 `TickEvent` 的载荷发出去）；
/// * `base_tick` / `base_ms` ⇐ **我方**为接 [`tick_at`]（抓包拟合出来的那条公式）加的两个基准，
///   原版在这里放的是 `SystemTime::now()`；两者语义等价、形状不同（**我方选择**）。
#[derive(Debug, Clone)]
pub struct GraphwarTickHandler {
    /// 累计走掉的纳秒（原版那一对 `Duration` 字段）。
    pub elapsed_nanos: u64,
    /// 走到第几个 tick（原版 `M+0x40`，从 0 起）。
    pub current_tick: u32,
    /// [`tick_at`] 的基准 tick / 基准毫秒（**我方形状**）。
    pub base_tick: i64,
    pub base_ms: i64,
    /// 发件箱（原版 `GraphwarEventQueue`）。
    pub outbox: Vec<Event>,
    /// 订阅名（`new` 里写进去的那一条）。
    pub subscribed: Vec<&'static str>,
}

impl Default for GraphwarTickHandler {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl GraphwarTickHandler {
    /// `new`（x86 `0x519690`）：累加器与 tick 计数都从 0 起，记下唯一那条订阅名。
    ///
    /// `base_tick` / `base_ms` 是**我方**给 [`tick_at`] 的基准（原版是 `SystemTime::now()`）。
    pub fn new(base_tick: i64, base_ms: i64) -> Self {
        Self {
            elapsed_nanos: 0,
            current_tick: 0,
            base_tick,
            base_ms,
            outbox: Vec::new(),
            subscribed: vec![SUBSCRIPTION],
        }
    }

    /// `tick`（x86 `0x519840`）：累加 `NANOS_PER_TICK` 纳秒、`current_tick += 1`，
    /// 发一条 `TickEvent`；`current_tick % 60 == 0` 时**再发**一条 `TickRequest`。
    ///
    /// 返回新的 `current_tick`（= 刚发出的 `TickEvent` 载荷）。
    pub fn tick(&mut self) -> u32 {
        self.elapsed_nanos = self.elapsed_nanos.saturating_add(NANOS_PER_TICK as u64);
        self.current_tick = self.current_tick.wrapping_add(1);
        self.outbox.push(Event::TickEvent);
        if self.current_tick % TICKS_PER_REQUEST == 0 {
            self.outbox.push(Event::TickRequest);
        }
        self.current_tick
    }

    /// `handle_connected_to_server_event`（x86 `0x519630`）：**不看**事件载荷，
    /// 直接发一条 `TickRequest`（判别值 1）。
    pub fn handle_connected_to_server_event(&mut self) {
        self.outbox.push(Event::TickRequest);
    }

    /// **已有纯逻辑**（[`tick_at`]，113 个真实抓包样本拟合出的 60 Hz 公式）的接线口：
    /// 由基准点推算 `now_ms` 时刻的 tick。
    pub fn tick_at_now(&self, now_ms: i64) -> i64 {
        tick_at(self.base_tick, self.base_ms, now_ms)
    }

    /// 累计时间换成的"tick 数"（含小数）：`elapsed_nanos / 1e9 * 60`。
    ///
    /// ⚠️ 这是**我方**从两个常量算出来的派生量，**不是**原版
    /// `get_fractional_tick`（`0x519530`）的复现——那个函数的算式**未解**。
    pub fn elapsed_tick_units(&self) -> f64 {
        self.elapsed_nanos as f64 / NANOS_PER_SECOND as f64 * TICKS_PER_SECOND as f64
    }

    /// 取走发件箱（验收用；原版直接把事件塞进 `GraphwarEventQueue`）。
    pub fn take_outbox(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }
}

#[cfg(test)]
mod handler_tests {
    use super::*;

    /// 常量与订阅名：16 666 666 = `0xfe502a`、1e9 = `0x3b9aca00`、60 = 那个整除判据。
    #[test]
    fn constants_are_the_ones_in_the_two_functions() {
        assert_eq!(NANOS_PER_TICK, 0xfe502a);
        assert_eq!(NANOS_PER_SECOND, 0x3b9aca00);
        assert_eq!(TICKS_PER_REQUEST, 60);
        assert_eq!(SUBSCRIPTION, "connected_to_server_event");
        let h = GraphwarTickHandler::default();
        assert_eq!(h.subscribed, vec![SUBSCRIPTION]);
        assert_eq!(h.current_tick, 0);
    }

    /// 一次最小交互：`tick()` 三次 ⇒ 计数 3、发件箱 3 条 `TickEvent`，没有 `TickRequest`。
    #[test]
    fn three_ticks_make_three_tick_events() {
        let mut h = GraphwarTickHandler::new(0, 0);
        assert_eq!(h.tick(), 1);
        assert_eq!(h.tick(), 2);
        assert_eq!(h.tick(), 3);
        assert_eq!(h.current_tick, 3);
        assert_eq!(h.take_outbox(), vec![Event::TickEvent; 3]);
        assert_eq!(h.elapsed_nanos, 3 * NANOS_PER_TICK as u64);
        // 3 tick 的累计时间 ≈ 0.05 秒
        assert!((h.elapsed_tick_units() - 3.0).abs() < 1e-3);
    }

    /// 第 60 个 tick 多一条 `TickRequest`（`tick` 里那个判 60 整除的分支）。
    #[test]
    fn every_sixtieth_tick_also_requests() {
        let mut h = GraphwarTickHandler::new(0, 0);
        for _ in 0..59 {
            h.tick();
        }
        assert_eq!(h.outbox_len(), 59, "前 59 个只有 TickEvent");
        let _ = h.take_outbox();
        assert_eq!(h.tick(), 60);
        assert_eq!(h.take_outbox(), vec![Event::TickEvent, Event::TickRequest]);
    }

    /// `handle_connected_to_server_event` 只发一条 `TickRequest`，不动计数。
    #[test]
    fn connected_to_server_requests_a_tick() {
        let mut h = GraphwarTickHandler::new(0, 0);
        h.handle_connected_to_server_event();
        assert_eq!(h.current_tick, 0);
        assert_eq!(h.take_outbox(), vec![Event::TickRequest]);
    }

    /// 接线：`tick_at_now` 就是那条抓包拟合公式（500 ms ⇒ 30 tick）。
    #[test]
    fn tick_at_now_delegates_to_the_captured_formula() {
        let h = GraphwarTickHandler::new(5, 0);
        assert_eq!(h.tick_at_now(500), 35);
        assert_eq!(h.tick_at_now(0), 5);
        assert_eq!(tick_at(0, 0, 1000), 60);
    }
}
