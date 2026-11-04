//! 点序列采样：等弦长二分追赶。
//!
//! 对应二进制：`src/graphwar_path_calculator.rs` `@0x3c2530`
//! （Ghidra/REA 地址 `0x4c2530`）。算法与常量见 `docs/04-path-sampling.md`。
//!
//! 要点（已逐位验证）：
//! - 每个点解 `hypot(dx, dy) = step`，对 x 做二分，最多 100 次
//! - 收敛判据 `|L - step| < 0.001`
//! - 点数预算 `(200.0 / step) as i32`，饱和转换
//! - 入列前硬检查 `|x| <= 25`、`|y| <= 15`、非 NaN
//! - 二分失败 => 整条曲线结束（不裁剪、不留坏点）
//! - ODE 模式用一阶 RK4，且第三个变量恒为 0

/// 采样模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `y = f(x)`
    Normal,
    /// `dy/dx = f(x, y)`，RK4 积分
    Ode,
}

/// 计算一次射击的点序列。
///
/// # 参数
/// - `tokens`：**已经**加上修正常数的 token 列表（见 `docs/05-shot-pipeline.md`）
/// - `x0`、`y0`：开火士兵坐标（`y0` 是首点的 y，不参与求值）
/// - `step`：弦长目标；服务端恒为 [`crate::STEP_DEFAULT`]
/// - `flip`：向左开火
///
/// # 状态
/// 尚未移植。参照实现 `graphwar_curve.py::calculate_path`。
pub fn calculate(
    _tokens: &[crate::Token],
    _x0: f64,
    _y0: f64,
    _step: f64,
    _flip: bool,
    _mode: Mode,
) -> Vec<(f64, f64)> {
    todo!("port from graphwar_curve.py::calculate_path (bit-exact against 0x3c2530)")
}
