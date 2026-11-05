//! 一次开火的完整结算：弹道 + 停止点 + 致死。
//!
//! 模型来自真实对局抓包（见 `docs/25`、`docs/30`）：
//!
//! * **障碍**决定弹道在哪里停：把障碍光栅化进掩膜，沿轨迹逐点查；
//! * **士兵**不挡弹道，只在弹道**经过其半径内**时被判定致死；
//! * 因此两者**不能**混在同一张掩膜里——这是 `docs/30` 纠正过的错误建模。
//!
//! 调用方负责：
//!
//! * 传入只含障碍的掩膜（`plane::empty_mask()` + 逐个 `rasterize`）；
//! * 传入**除射手本人以外**的士兵（射手不会打死自己，这一点由调用方保证）。

use crate::path::Mode;
use crate::plane::is_occupied;
use crate::shot::shot_points;

/// 参与结算的士兵。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub id: i64,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

/// 一次命中：谁、在弹道的哪个位置被扫过。
///
/// 存在的唯一理由：同时回合制要按**弹丸飞行时间**给一批死亡排序（`docs/86` §6、
/// `docs/141`）。同一回合里所有弹丸的 `active_tick` 相同（13/13 回合观测），
/// 所以"谁先到"等价于"谁的弧长更短" —— 不需要先知道那 ≈0.29 单位/tick 的速度本身。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// 被扫过的士兵 id。
    pub id: i64,
    /// 从起点沿弹道量到命中点的弧长（棋盘单位）。
    pub arc: f64,
    /// 命中发生在第几个采样点（0 = 起点）。
    pub index: usize,
}

/// 一次开火的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct ShotOutcome {
    /// 弹道点序列（含起点）。
    pub points: Vec<(f64, f64)>,
    /// 弹道停止处的点（撞到障碍或到达边界）。
    pub stop: (f64, f64),
    /// 是否因撞到障碍而停止（`false` 表示飞到边界/点用尽）。
    pub blocked: bool,
    /// 被判定致死的士兵，按**经过的先后顺序**。
    pub killed: Vec<i64>,
    /// 与 `killed` 一一对应的命中细节（弧长 + 采样点下标），顺序相同。
    pub hits: Vec<Hit>,
}

/// 结算一次开火。
///
/// `mask` 只应包含障碍；`targets` 不含射手本人。
pub fn resolve(
    text: &str,
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
    mask: &[u8],
    targets: &[Target],
) -> ShotOutcome {
    let points = shot_points(text, x0, y0, step, flip, mode, true);
    resolve_points(mask, targets, &points)
}

/// 与 [`resolve`] 同一套规则，但**点序列由调用方给**（`docs/120`）。
///
/// 存在的理由只有一个：GDScript 那边已经拿着内核算好的点序列
/// （`path_calculate` 的返回值），没必要为了判命中再算一遍 ——
/// 更重要的是**不能**再算一遍：两次采样的输入只要有一点差别，客户端画的弹道
/// 和致死判定就会分家。规则本体只有这一份（[`resolve`] 也走这里）。
///
/// 规则逐条与 [`resolve`] 相同（`docs/30`）：
///
/// 1. 每个点先判致死（`d <= radius`），再判阻挡（掩膜 `!= 0xFF`）；
/// 2. 士兵**不**影响弹道，只有障碍让弹道停下；
/// 3. 同一士兵只记一次；`killed` 的顺序 = 弹道经过的先后。
///
/// `points` 用**棋盘坐标**，`mask` 是**像素空间**的 1000×600 掩膜；
/// 两者之间的换算在 [`crate::plane::is_occupied`] 里（`px = 20x+500`），
/// 本函数与调用方都**不要**再翻一次（`docs/120` §2）。
pub fn resolve_points(mask: &[u8], targets: &[Target], points: &[(f64, f64)]) -> ShotOutcome {
    let (x0, y0) = *points.first().unwrap_or(&(0.0, 0.0));
    let mut killed: Vec<i64> = Vec::new();
    let mut hits: Vec<Hit> = Vec::new();
    let mut stop = (x0, y0);
    let mut blocked = false;
    // 沿弹道累计的弧长：同时回合制靠它给一批死亡排序（`docs/141` §2）。
    let mut arc = 0.0f64;

    for (index, &(px, py)) in points.iter().enumerate() {
        if index > 0 {
            let (qx, qy) = points[index - 1];
            arc += ((px - qx).powi(2) + (py - qy).powi(2)).sqrt();
        }
        // 先判致死：经过即死，且不影响弹道
        for t in targets {
            if killed.contains(&t.id) {
                continue;
            }
            let d = ((px - t.x).powi(2) + (py - t.y).powi(2)).sqrt();
            if d <= t.radius {
                killed.push(t.id);
                hits.push(Hit { id: t.id, arc, index });
            }
        }
        // 再判阻挡：只有障碍会让弹道停下
        if is_occupied(mask, px, py) {
            stop = (px, py);
            blocked = true;
            break;
        }
        stop = (px, py);
    }

    ShotOutcome { points: points.to_vec(), stop, blocked, killed, hits }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plane::{empty_mask, rasterize};

    #[test]
    fn obstacle_stops_the_shot() {
        let mut m = empty_mask();
        rasterize(&mut m, -5.0, 0.0, 1.0, 0x00);
        let r = resolve("0.00", 10.0, 0.0, 0.5, true, Mode::Normal, &m, &[]);
        assert!(r.blocked, "应被障碍挡住");
        assert!(r.stop.0 > -6.5 && r.stop.0 < -3.5, "停止点应在障碍附近: {:?}", r.stop);
    }

    #[test]
    fn soldier_dies_without_stopping_the_shot() {
        let m = empty_mask(); // 无障碍
        let t = Target { id: 7, x: 0.0, y: 0.0, radius: 0.5 };
        let r = resolve("0.00", 10.0, 0.0, 0.5, true, Mode::Normal, &m, &[t]);
        assert_eq!(r.killed, vec![7], "经过士兵应判定致死");
        assert!(!r.blocked, "士兵不应阻挡弹道");
        assert!(r.stop.0 < -5.0, "弹道应继续飞过士兵: {:?}", r.stop);
    }

    #[test]
    fn kill_only_counts_once() {
        let m = empty_mask();
        let t = Target { id: 9, x: 0.0, y: 0.0, radius: 2.0 };
        let r = resolve("0.00", 10.0, 0.0, 0.5, true, Mode::Normal, &m, &[t]);
        assert_eq!(r.killed, vec![9], "同一士兵只应记一次");
    }

    #[test]
    fn miss_when_far_away() {
        let m = empty_mask();
        let t = Target { id: 3, x: 0.0, y: 20.0, radius: 0.5 };
        let r = resolve("0.00", 10.0, 0.0, 0.5, true, Mode::Normal, &m, &[t]);
        assert!(r.killed.is_empty());
    }
}

// ---------------------------------------------------------------------------
// 事件生成：把结算结果变成协议消息
// ---------------------------------------------------------------------------

/// 一次开火会产生的协议事件（顺序即协议上的出现顺序）。
///
/// 依据真实抓包：
///
/// ```text
/// FunctionPoints -> LifeQueuedDeath -> EffectInfo(DeathEffect) -> ... -> EffectInfo(ExplosionEffect)
/// ```
///
/// 若弹道被障碍挡住，还会多一条 `QueueDestroyObstacle`（位置 = 停止点）。
/// **tick 值没有建模**：真实 tick 取决于弹丸飞行时间，尚未还原，
/// 所以这里不带 tick 字段，只保证种类与坐标一致。
#[derive(Debug, Clone, PartialEq)]
pub enum ShotEvent {
    FunctionPoints { owner_id: i64, start_x: f64, start_y: f64, end_x: f64 },
    LifeQueuedDeath { entity_id: i64 },
    DeathEffect { entity_id: i64, x: f64, y: f64 },
    QueueDestroyObstacle { x: f64, y: f64, radius: f64 },
    ExplosionEffect { x: f64, y: f64 },
}

/// 由结算结果生成事件序列。
pub fn events(owner_id: i64, x0: f64, y0: f64, outcome: &ShotOutcome, targets: &[Target]) -> Vec<ShotEvent> {
    let mut out = vec![ShotEvent::FunctionPoints {
        owner_id,
        start_x: x0,
        start_y: y0,
        end_x: outcome.stop.0,
    }];
    for id in &outcome.killed {
        out.push(ShotEvent::LifeQueuedDeath { entity_id: *id });
        if let Some(t) = targets.iter().find(|t| t.id == *id) {
            // 死亡效果点在**士兵站立处**，不是弹道上的点（docs/30）
            out.push(ShotEvent::DeathEffect { entity_id: *id, x: t.x, y: t.y });
        }
    }
    if outcome.blocked {
        out.push(ShotEvent::QueueDestroyObstacle { x: outcome.stop.0, y: outcome.stop.1, radius: 0.5 });
    }
    out.push(ShotEvent::ExplosionEffect { x: outcome.stop.0, y: outcome.stop.1 });
    out
}
