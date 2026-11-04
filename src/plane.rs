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

// ---------------------------------------------------------------------------
// 光栅化：把实体按半径画成带抗锯齿的实心圆写进掩膜
// ---------------------------------------------------------------------------
//
// 来源：`0x4f11e0`（虚地址 `0x3f11e0`）的伪代码，签名是
//   void f(double x, double y, double size, long mask_ptr, ulong mask_len, byte fill)
// —— **不依赖任何上下文对象**，所以可以用 unicorn 直接调用它做 oracle。
//
// 算法两遍：
//   ① 逐行硬填：row = py+dy，半宽 hw = round(sqrt(r²−dy²))，填 [px−hw, px+hw]；
//   ② 抗锯齿环：对 dx²+dy² ≤ (r+1)² 的外圈像素，按
//      coverage = 1 − |dist − r| 与旧值混合，再钳到 [0,255]。
//
// 注意 `fill` 是**参数**（可见字节值不只有 0x00/0xFF），所以"空"只能是 0xFF。

fn sat_i32(v: f64) -> i32 {
    if v.is_nan() {
        0
    } else {
        v as i32 // Rust 的 as 是饱和转换，与原程序的钳位序列等价
    }
}

/// 游戏坐标 + 尺寸 -> 像素坐标与像素半径，对应 `0x4f11e0` 开头的三次转换。
pub fn raster_params(x: f64, y: f64, size: f64) -> (i32, i32, i32) {
    let px = sat_i32(x * 1000.0 / BOARD_W + 500.0);
    let py = sat_i32(y * 1000.0 / BOARD_W + 300.0);
    let r = sat_i32(size * 1000.0 / BOARD_W);
    (px, py, r)
}

/// 把实体画进 `mask`（长度应为 `CANVAS_W * CANVAS_H`）。
///
/// 与 `0x4f11e0` 的伪代码逐步对应；越界像素跳过（原程序只检查、不报错，
/// 但掩膜长度不足时会 panic，这里用 `get_mut` 安全跳过）。
pub fn rasterize(mask: &mut [u8], x: f64, y: f64, size: f64, fill: u8) {
    let (px, py, r) = raster_params(x, y, size);
    if r < 0 {
        return;
    }
    let r2 = r * r;
    let r_out = r + 1;
    let r_out2 = r_out * r_out;

    // ---- ① 逐行硬填 ----
    let mut dy = -r;
    while dy <= r {
        let row = py.wrapping_add(dy);
        if (row as u32) < CANVAS_H {
            let inside = r2 - dy * dy; // dy∈[-r,r] ⇒ 非负
            let hw = sat_i32((inside as f64).sqrt()); // 截断：oracle 实测 trunc 0 处不符
            let left = (px.wrapping_sub(hw)).clamp(0, CANVAS_W as i32 - 1);
            let right = (px.wrapping_add(hw)).clamp(0, CANVAS_W as i32 - 1);
            let base = row as usize * CANVAS_W as usize;
            for col in left..=right {
                if let Some(slot) = mask.get_mut(base + col as usize) {
                    *slot = fill;
                }
            }
        }
        dy += 1;
    }

    // ---- ② 抗锯齿环（只碰"行填之外"的像素）----
    blend_ring(mask, px, py, r, r_out, r_out2, fill);
}

/// 抗锯齿环：`dx²+dy² ≤ (r+1)²` 且落在"行填"之外的像素按覆盖率混合。
///
/// 伪代码用一个对称游走来枚举这些像素；这里用等价的全遍历——
/// 只要判定条件与混合公式一致，结果就相同（并用 oracle 逐字节校验）。
fn blend_ring(mask: &mut [u8], px: i32, py: i32, r: i32, r_out: i32, r_out2: i32, fill: u8) {
    let _ = r_out;
    let mut dy = -r_out;
    while dy <= r_out {
        let row = py.wrapping_add(dy);
        if (row as u32) < CANVAS_H {
            // 该行的硬填半宽：伪代码的环游走从 hw+1 开始，所以只处理 |dx| > hw
            let hw = if dy.abs() <= r {
                sat_i32(((r * r - dy * dy) as f64).sqrt()) // 截断
            } else {
                0
            };
            let mut dx = -r_out;
            while dx <= r_out {
                let d2 = dx * dx + dy * dy;
                if d2 <= r_out2 && dx.abs() > hw {
                    let col = px.wrapping_add(dx);
                    if (col as u32) < CANVAS_W {
                        let dist = (d2 as f64).sqrt();
                        let cov = (dist - r as f64).abs(); // = 1 - coverage
                        let idx = row as usize * CANVAS_W as usize + col as usize;
                        if let Some(slot) = mask.get_mut(idx) {
                            let old = *slot as f64;
                            let blended = (1.0 - cov) * fill as f64 + old * cov;
                            let clamped = blended.clamp(0.0, 255.0);
                            *slot = (clamped as i32) as u8;
                        }
                    }
                }
                dx += 1;
            }
        }
        dy += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Vec<u8> { empty_mask() }

    #[test]
    fn raster_center_is_filled_and_bounded() {
        let mut m = empty();
        rasterize(&mut m, 0.0, 0.0, 1.0, 0x00);
        // 圆心被硬填充
        assert_eq!(m[mask_index(500, 300)], 0x00);
        // 半径 20px：(500±25, 300) 一定在圆外
        assert_eq!(m[mask_index(475, 300)], EMPTY);
        assert_eq!(m[mask_index(525, 300)], EMPTY);
    }

    #[test]
    fn raster_edge_values_match_the_binary() {
        // 以下两个值取自**真二进制**（unicorn 调用 0x3f11e0 的输出）
        let mut m = empty();
        rasterize(&mut m, 0.0, 0.0, 1.0, 0x00);
        assert_eq!(m[mask_index(491, 282)], 0x1f, "抗锯齿覆盖率不符");
        assert_eq!(m[mask_index(489, 283)], 0x3f, "抗锯齿覆盖率不符");
    }

    #[test]
    fn raster_is_symmetric() {
        let mut m = empty();
        rasterize(&mut m, 0.0, 0.0, 1.0, 0x00);
        for dy in -21..=21 {
            for dx in -21..=21 {
                assert_eq!(
                    m[mask_index(500 + dx, 300 + dy)],
                    m[mask_index(500 - dx, 300 + dy)],
                    "关于竖直轴不对称: dx={dx} dy={dy}"
                );
            }
        }
    }

    #[test]
    fn raster_respects_fill_value() {
        let mut m = empty();
        rasterize(&mut m, 0.0, 0.0, 1.0, 0x10);
        assert_eq!(m[mask_index(500, 300)], 0x10, "填充值应由参数决定");
    }

    #[test]
    fn raster_clips_at_canvas_edge() {
        let mut m = empty();
        // 圆心贴右下角，不应 panic，也不应写出画布
        rasterize(&mut m, 24.5, 14.0, 1.0, 0x00);
        assert_eq!(m.len(), CANVAS_W as usize * CANVAS_H as usize);
        // 角落附近确实被画到了
        let touched = (0..60).any(|d| m[mask_index(999 - d + 0, 599)] != EMPTY
                                     || m[mask_index(999, 599 - d)] != EMPTY);
        assert!(touched, "贴边时应在画布内留下痕迹");
    }

    #[test]
    fn occupied_helper_sees_rasterized_pixels() {
        let mut m = empty();
        rasterize(&mut m, 0.0, 0.0, 1.0, 0x00);
        assert!(is_occupied(&m, 0.0, 0.0), "圆心应判为占据");
        assert!(!is_occupied(&m, 24.0, 14.0), "远处仍为空");
    }
}
