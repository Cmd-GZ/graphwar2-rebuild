//! 坐标系与碰撞掩膜。
//!
//! 两个独立来源的观测，各自有明确出处：
//!
//! - **变换**：`0x4be3e0`（`graphwar_plane.rs`）里的伪代码，
//!   `px = x*1000/50 + 500`、`py = y*1000/50 + 300`，除数常量
//!   `_DAT_0029ce00`（Ghidra）→ 虚地址 `0x19ce00` → `(50.0, 50.0)`。
//! - **掩膜**：`0x4f15b0`（`graphwar_obstacle_manager.rs`）里的伪代码，
//!   `idx = py*1000 + px`、`mask[idx] != 0xFF`、越界算命中。
//!
//! 详见 `docs/08-plane.md` §4 与 §6。
//!
//! 尚未实现：把实体光栅化进掩膜（`0x4f11e0`）。它的填充规则（`<` 还是 `<=`、
//! 端点舍入）没有逐行确认，硬写一个"看起来对"的版本会违反本仓库"不把推断写成结论"
//! 的约定，所以留空并标注。

/// 画布宽度（像素）。
pub const CANVAS_W: u32 = 1000;
/// 画布高度（像素）。
pub const CANVAS_H: u32 = 600;

/// 棋盘在游戏单位下的宽（`x ∈ [-25, 25]`）。
pub const BOARD_W: f64 = 50.0;
/// 棋盘在游戏单位下的高（`y ∈ [-15, 15]`）。
pub const BOARD_H: f64 = 30.0;

/// 每游戏单位对应的像素数：`1000 / 50`。
pub const PX_PER_UNIT: f64 = CANVAS_W as f64 / BOARD_W;

/// 掩膜里的"空"值。
pub const EMPTY: u8 = 0xFF;

/// 游戏坐标 -> 像素坐标。
///
/// 直接照抄原程序的写法（先乘 1000 再除以 50，不用 20），因为浮点舍入顺序
/// 会影响边界点的结果；`as i32` 在 Rust 里是饱和转换、`NaN` 归 0，
/// 与原程序里那串显式钳位完全一致。
pub fn to_pixel(x: f64, y: f64) -> (i32, i32) {
    let px = x * 1000.0 / BOARD_W + 500.0;
    let py = y * 1000.0 / BOARD_W + 300.0;
    (px as i32, py as i32)
}

/// 像素坐标是否落在画布内。
pub fn in_canvas(px: i32, py: i32) -> bool {
    (px as u32) < CANVAS_W && (py as u32) < CANVAS_H
}

/// 掩膜下标。调用方必须先确认 [`in_canvas`]。
pub fn mask_index(px: i32, py: i32) -> usize {
    py as usize * CANVAS_W as usize + px as usize
}

/// 单点碰撞测试：与 `0x4f15b0` 同构。
///
/// **越界一律算命中**（原程序直接返回 1），这是有玩法含义的行为，
/// 不是防御性编程：棋盘外等价于阻挡。
pub fn is_occupied(mask: &[u8], x: f64, y: f64) -> bool {
    let (px, py) = to_pixel(x, y);
    if !in_canvas(px, py) {
        return true;
    }
    let idx = mask_index(px, py);
    match mask.get(idx) {
        Some(&v) => v != EMPTY,
        // 掩膜长度不足时原程序会 panic（越界检查），这里保守地当作命中
        None => true,
    }
}

/// 新建一张全空的掩膜。
pub fn empty_mask() -> Vec<u8> {
    vec![EMPTY; CANVAS_W as usize * CANVAS_H as usize]
}
