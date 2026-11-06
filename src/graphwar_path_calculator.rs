//! 点序列采样：等弦长二分追赶。
//!
//! 对应二进制：`src/graphwar_path_calculator.rs` `@0x3c2530`
//! （Ghidra/REA 地址 `0x4c2530`）。算法见 `docs/04-path-sampling.md`。
//!
//! 移植自上游已验证的 `graphwar_curve.py::calculate_path`。

use crate::function::{eval, hypot};
use crate::{Token, LENGTH_TOL, MAX_ABS_X, MAX_ABS_Y, TOTAL_LEN};

const HALF: f64 = 0.5;
const SIXTH: f64 = 6.0;
const MAX_BISECT: usize = 100;

/// 采样模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `y = f(x)`
    Normal,
    /// `dy/dx = f(x, y)`，RK4 积分
    Ode,
}

/// 点数预算：`(200.0 / step) as i32`，饱和转换，小于 2 时取 1。
pub fn point_budget(step: f64) -> i32 {
    let raw = TOTAL_LEN / step;
    // Rust 的 `as i32` 本身就是饱和转换，且 NaN 映射为 0 —— 与原程序一致
    let n = raw as i32;
    if n >= 2 { n } else { 1 }
}

/// 计算一次射击的点序列。
///
/// - `tokens`：**已经**加上修正常数的 token 列表（见 [`crate::shot`]）
/// - `x0, y0`：开火士兵坐标；首点就是它
/// - `step`：目标弦长；服务端恒为 [`crate::STEP_DEFAULT`]
/// - `flip`：向左开火（镜像 x 轴）
pub fn calculate(
    tokens: &[Token],
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
) -> Vec<(f64, f64)> {
    let mut pts = vec![(x0, y0)];
    let mut x = if flip { -x0 } else { x0 };
    let mut y = y0;
    let n = point_budget(step);
    let mut count: i32 = 0;

    loop {
        let mut lo = x;
        let mut hi = x + step;
        let mut accepted = false;

        for _ in 0..MAX_BISECT {
            let mid = (lo + hi) * HALF;

            let (cand_y, chord) = match mode {
                Mode::Ode => {
                    // 一阶 RK4；第三个变量（y'）恒为 0
                    let h = mid - x;
                    let k1 = eval(tokens, x, y, 0.0);
                    let k2 = eval(tokens, x + h * HALF, y + k1 * h * HALF, 0.0);
                    let k3 = eval(tokens, x + h * HALF, y + k2 * h * HALF, 0.0);
                    let k4 = eval(tokens, x + h, y + k3 * h, 0.0);
                    let yn = y + (h / SIXTH) * (k1 + 2.0 * k2 + 2.0 * k3 + k4);
                    (yn, hypot(h, yn - y))
                }
                Mode::Normal => {
                    let fm = eval(tokens, mid, 0.0, 0.0);
                    (fm, hypot(mid - x, fm - y))
                }
            };

            // 收敛判据：|弦长 - step| < 0.001（NaN 比较结果为 false，走 else 分支）
            if !(LENGTH_TOL > (chord - step).abs()) {
                if chord > step || chord.is_nan() {
                    hi = mid;
                } else {
                    lo = mid;
                }
                continue;
            }

            // 普通模式下原程序还会用半步再校验一次
            if mode == Mode::Normal {
                let mid2 = (x + mid) * HALF;
                let fm2 = eval(tokens, mid2, 0.0, 0.0);
                let chord2 = hypot(mid2 - x, fm2 - y);
                // 反汇编 0x3c2a0a 与 0x3c2a47：复检不通过时 **lo 和 hi 都要改**
                // （hi = mid2 在比较之前就无条件写了；此前只写了 lo=x，是长期漏实现）
                if !(chord > chord2) {
                    lo = x;
                    hi = mid2;
                    continue;
                }
            }

            let px = if flip { -mid } else { mid };
            // 入列前硬检查：NaN 或越界就整条曲线结束（不裁剪、不留坏点）
            if cand_y.is_nan() || px.is_nan() {
                return pts;
            }
            if mid.abs() > MAX_ABS_X || cand_y.abs() > MAX_ABS_Y {
                return pts;
            }

            pts.push((px, cand_y));
            count += 1;
            x = mid;
            y = cand_y;
            accepted = true;
            break;
        }

        if !accepted || count == n {
            return pts;
        }
    }
}
