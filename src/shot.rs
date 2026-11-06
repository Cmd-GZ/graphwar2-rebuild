//! 开火结算里"让曲线穿过士兵"的那一步。
//!
//! 对应二进制：修正常数 `@0x3c5f70`，改写函数串 `@0x3c5c50`。
//! 见 `docs/05-shot-pipeline.md`。

use crate::function::{eval, parse};
use crate::graphwar_path_calculator::{calculate, Mode};
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
//   * 采点      `src/graphwar_path_calculator.rs`     （331 条真值夹具）
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

/// 与游戏一致的"修正后函数"文本：`原函数` + **带符号**的修正常数（三位小数）。
///
/// 依据是真实抓包（`docs/24` §6.2；`docs/127` §2 的原始字节流帧号）：
///
/// ```text
/// {"FunctionInfo":{"owner_id":234,"function":"0.00","corrected_function":"0.00 + 4.638"}}
/// {"FunctionInfo":{"owner_id":231,"function":"","corrected_function":" + 10.990"}}
/// {"FunctionInfo":{"owner_id":231,"function":"","corrected_function":" - 3.686"}}             ← 帧 182（c < 0）
/// {"FunctionInfo":{"owner_id":233,"function":"abs((abs((-0.57+x))+-4.85))",
///                  "corrected_function":"abs((abs((-0.57+x))+-4.85)) - 11.416"}}               ← 帧 203（c < 0）
/// ```
///
/// ⇒ **运算符本身带符号**：`c >= 0` 写 `" + "`、`c < 0` 写 `" - "`，
/// 而**不是** `" + -3.686"`。这与右下角那行字（[`correction_text`]）是同一个格式，
/// 所以直接复用，不另写一套。
///
/// ⚠️ 2026-11 之前这里是写死的 `format!("{} + {:.3}", …)`：`c < 0` 时会发出
/// `" + -3.686"`，真机从没这么写过（`docs/127` §5 第 1 条、§2.1 第 3 条）。
pub fn corrected_text(text: &str, x0: f64, y0: f64, flip: bool) -> String {
    let toks = crate::function::parse(text);
    format!("{}{}", text, correction_text(&toks, x0, y0, flip))
}

#[cfg(test)]
mod correction_tests {
    use super::*;

    /// 真实抓包样本：士兵 234 用 `0.00` 开火，修正文本是 `0.00 + 4.638`。
    #[test]
    fn correction_matches_captured_text_234() {
        let x0 = 1.651220991201045_f64;
        let y0 = 4.638020825690482_f64;
        let txt = corrected_text("0.00", x0, y0, false);
        assert_eq!(txt, "0.00 + 4.638", "抓包里的修正文本");
    }

    /// 真实抓包样本：士兵 231 用空函数开火，修正文本是 ` + 10.990`。
    #[test]
    fn correction_matches_captured_text_231() {
        let x0 = -19.512760846799154_f64;
        let y0 = 10.99018276610812_f64;
        let txt = corrected_text("", x0, y0, false);
        assert_eq!(txt, " + 10.990", "抓包里的修正文本");
    }

    /// **符号**的回归：`c < 0` 时运算符是 `" - "`，不是 `" + -"`。
    ///
    /// 三条都是原始字节流抓包的**原文**（`docs/127` §2；`start_x/start_y` 取自同一回合的
    /// `FunctionPoints`）。旧实现（写死 `" + "`）在 182/203 这两条上会给出
    /// `" + -3.686"` / `" + -11.416"`，真机从没这么写过。
    #[test]
    fn correction_operator_carries_the_sign_of_the_constant() {
        // 帧 182/183：士兵 231（Team1 ⇒ flip=false）空函数 ⇒ c = y0 = -3.6861
        assert_eq!(
            corrected_text("", -13.429861913415694, -3.686102115928783, false),
            " - 3.686",
            "抓包帧 182"
        );
        // 帧 203/204：士兵 233（Team2 ⇒ flip=true）曲线 + 负常数
        assert_eq!(
            corrected_text("abs((abs((-0.57+x))+-4.85))", 14.305645069934501, -1.3905063945238432, true),
            "abs((abs((-0.57+x))+-4.85)) - 11.416",
            "抓包帧 203"
        );
        // 帧 143/144：士兵 230 空函数，同一条链上的另一个负样本
        assert_eq!(
            corrected_text("", -8.2808732224322, -3.4200779499591807, false),
            " - 3.420",
            "抓包帧 143"
        );
        // 正号那一侧照旧（帧 161/162，flip=true）
        assert_eq!(
            corrected_text("cos((0.19*(6.70+x)))", 11.962551145879328, 3.7443842094294837, true),
            "cos((0.19*(6.70+x))) + 3.204",
            "抓包帧 161"
        );
    }

    /// NaN 仍走 `+` 那一侧（`correction_text` 的既有行为，别被上面的符号分支带跑）。
    #[test]
    fn correction_keeps_the_nan_branch() {
        assert_eq!(corrected_text("0.00", 0.0, f64::NAN, false), "0.00 + NaN");
        assert_eq!(corrected_text("", 0.0, f64::NAN, false), " + NaN");
        // 正/负两侧的定值（c = 2.000 / -2.000）
        assert_eq!(corrected_text("0.00", 1.0, 2.0, false), "0.00 + 2.000");
        assert_eq!(corrected_text("0.00", 1.0, -2.0, false), "0.00 - 2.000");
    }

    /// 抓到的弹道终点必须落在我们算出的轨迹上（这两发都是水平直线）。
    #[test]
    fn captured_shot_endpoints_lie_on_our_trajectory() {
        // (起点 x, 起点 y, 抓到的终点 x, 终点 y, 是否反向)
        // 方向由实测确定：第一发向左（flip=true），第二发向右（flip=false）
        let cases = [
            (1.651220991201045_f64, 4.638020825690482_f64, -16.405029008797804_f64, 4.638_f64, true),
            (-19.512760846799154_f64, 10.99018276610812_f64, -12.040885846800286_f64, 10.99_f64, false),
        ];
        for (x0, y0, end_x, end_y, flip) in cases {
            let pts = shot_points("0.00", x0, y0, crate::SERVER_STEP, flip, Mode::Normal, true);
            assert!(!pts.is_empty());
            // 水平直线：整条轨迹 y 恒等于修正后的常数
            let c = y0; // f(x)=0 ⇒ 修正后恒为 y0
            assert!((pts[0].1 - c).abs() < 1e-9, "起点 y 不符");
            assert!(pts.iter().all(|p| (p.1 - c).abs() < 1e-9), "应恒为水平线");
            // 抓到的爆炸点 y 与轨迹一致（抓到的是三位小数）
            assert!((pts[0].1 - end_y).abs() < 5e-4, "终点 y 与轨迹不符: {} vs {}", pts[0].1, end_y);
            // 终点 x 必须落在轨迹覆盖的 x 区间内
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for p in &pts {
                lo = lo.min(p.0);
                hi = hi.max(p.0);
            }
            assert!(end_x >= lo - 1e-6 && end_x <= hi + 1e-6,
                    "终点 x={end_x} 不在轨迹 x 区间 [{lo}, {hi}] 内");
        }
    }
}
