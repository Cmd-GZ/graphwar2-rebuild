//! 开火结算里"让曲线穿过士兵"的那一步。
//!
//! 对应二进制：修正常数 `@0x3c5f70`，改写函数串 `@0x3c5c50`。
//! 见 `docs/05-shot-pipeline.md`。

use crate::function::{eval, parse};
use crate::path::{calculate, Mode};
use crate::{Function, Token, STEP_DEFAULT};

/// 修正常数 `c = y0 - f(±x0)`；实际射击使用 `f(x) + c`。
pub fn correction(tokens: &[Token], x0: f64, y0: f64, flip: bool) -> f64 {
    y0 - eval(tokens, if flip { -x0 } else { x0 }, 0.0, 0.0)
}

/// 右下角那行字：`" + 0.591"` / `" - 1.000"` / `" + NaN"`（3 位小数）。
pub fn correction_text(tokens: &[Token], x0: f64, y0: f64, flip: bool) -> String {
    let c = correction(tokens, x0, y0, flip);
    if c.is_nan() {
        // 原程序按 `c < 0.0` 分支，NaN 走 '+' 那一侧
        return " + NaN".to_string();
    }
    if c >= 0.0 {
        format!(" + {:.3}", c)
    } else {
        format!(" - {:.3}", -c)
    }
}

/// 把 `f(x)+c` 表示成前缀 token 列表。
pub fn shifted_tokens(tokens: &[Token], c: f64) -> Vec<Token> {
    let mut out = vec![Token::op(Function::Add), Token::constant(c)];
    out.extend_from_slice(tokens);
    out
}

/// 端到端：一次射击产生的点序列。
pub fn shot_points(
    text: &str,
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
    apply_correction: bool,
) -> Vec<(f64, f64)> {
    let mut toks = parse(text);
    if toks.is_empty() {
        return Vec::new();
    }
    if apply_correction && mode == Mode::Normal {
        let c = correction(&toks, x0, y0, flip);
        toks = shifted_tokens(&toks, c);
    }
    calculate(&toks, x0, y0, step, flip, mode)
}

/// 便捷封装：服务端步长 + 普通模式。
pub fn shot_default(text: &str, x0: f64, y0: f64) -> Vec<(f64, f64)> {
    shot_points(text, x0, y0, STEP_DEFAULT, false, Mode::Normal, true)
}

// ---------------------------------------------------------------------------
// 开火模拟：把三块已验证的积木拼起来
// ---------------------------------------------------------------------------
//
// 组成：掩膜（创建 + 光栅化障碍）→ 点序列（采点）→ 逐点查掩膜。
//
// 三块积木各自都已与真二进制对拍：
//   * 采点      `src/path.rs`     （331 条真值夹具）
//   * 光栅化    `src/plane.rs`    （逐字节一致）
//   * 点测试    `src/plane.rs`    （300 随机点一致）
//
// **但"拼装方式"本身没有在二进制里找到对应的函数**（真正的开火循环还没定位），
// 所以这里只是把已验证的积木按最自然的顺序组合，并把它当作**可验证的原语**，
// 而不是"游戏就这么做的"结论。

use crate::plane::{empty_mask, is_occupied, mask_index, rasterize, CANVAS_H, CANVAS_W};

/// 障碍物：游戏坐标中心 + 半径（单位与 `size` 相同）。
#[derive(Debug, Clone, Copy)]
pub struct Obstacle {
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

/// 开火模拟结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotOutcome {
    /// 命中的点序号（`0` = 起点）；未命中为 `None`。
    pub hit_index: Option<usize>,
    /// 点序列长度。
    pub point_count: usize,
}

/// 跑一次模拟：先按障碍建掩膜，再沿点序列逐点查。
///
/// 起点本身通常落在开火士兵身上；这里**不**做"忽略起点"的特殊处理——
/// 掩膜里只有调用方给的障碍，所以起点是否命中取决于障碍怎么给。
pub fn simulate(
    text: &str,
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
    obstacles: &[Obstacle],
) -> ShotOutcome {
    let mut mask = empty_mask();
    for ob in obstacles {
        rasterize(&mut mask, ob.x, ob.y, ob.size, 0x00);
    }
    let pts = shot_points(text, x0, y0, step, flip, mode, true);
    let hit_index = pts.iter().position(|&(x, y)| is_occupied(&mask, x, y));
    ShotOutcome { hit_index, point_count: pts.len() }
}

/// 掩膜尺寸，供外部校验缓冲区。
pub const MASK_BYTES: usize = CANVAS_W as usize * CANVAS_H as usize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_obstacles_means_no_hit() {
        let r = simulate("sin(x)", 5.0, 2.0, 0.01, false, Mode::Normal, &[]);
        assert_eq!(r.hit_index, None);
        assert!(r.point_count > 100);
    }

    #[test]
    fn obstacle_on_the_curve_is_hit() {
        // sin(x) 在 x=5 附近 y≈-0.959 + 修正 2.9589 ≈ 2.0（起点在曲线上）
        let curve_y = (5.0f64).sin() * 1.0;
        let c = 2.0 - curve_y;
        let r = simulate(
            "sin(x)", 5.0, 2.0, 0.01, false, Mode::Normal,
            &[Obstacle { x: 8.0, y: (8.0f64).sin() + c, size: 0.3 }],
        );
        assert!(r.hit_index.is_some(), "曲线上放了障碍却没命中");
        assert!(r.hit_index.unwrap() > 1, "不应在起点就命中");
    }

    #[test]
    fn obstacle_away_from_the_curve_is_missed() {
        let r = simulate(
            "sin(x)", 5.0, 2.0, 0.01, false, Mode::Normal,
            &[Obstacle { x: 8.0, y: -12.0, size: 0.3 }],
        );
        assert_eq!(r.hit_index, None, "障碍远离曲线却命中了");
    }

    #[test]
    fn mask_bytes_matches_canvas() {
        assert_eq!(MASK_BYTES, 1000 * 600);
        assert_eq!(mask_index(0, 0), 0);
        assert_eq!(mask_index(999, 599), 1000 * 600 - 1);
    }
}
