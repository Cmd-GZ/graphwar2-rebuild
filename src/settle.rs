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
    let mut killed: Vec<i64> = Vec::new();
    let mut stop = (x0, y0);
    let mut blocked = false;

    for &(px, py) in &points {
        // 先判致死：经过即死，且不影响弹道
        for t in targets {
            if killed.contains(&t.id) {
                continue;
            }
            let d = ((px - t.x).powi(2) + (py - t.y).powi(2)).sqrt();
            if d <= t.radius {
                killed.push(t.id);
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

    ShotOutcome { points, stop, blocked, killed }
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
