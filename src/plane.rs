//! 坐标系与碰撞掩膜。
//!
//! 两个独立来源的观测，各自有明确出处：
//!
//! - **变换**：**已搬走** —— `px = x*1000/50 + 500`、`py = y*1000/50 + 300` 是从
//!   `0x4be3e0`（`graphwar_plane.rs:174:18`，`GraphwarPlane::obstacle_partial_update_event`）
//!   里逐条读出来的，所以函数体搬到 `src/graphwar_plane.rs`（见那里的 [`to_pixel`] 文档）。
//!   本文件保留 `pub use` 再导出，调用点与既有断言一行未改。
//! - **掩膜**：`0x4f15b0`（`graphwar_obstacle_manager.rs`）里的伪代码，
//!   `idx = py*1000 + px`、`mask[idx] != 0xFF`、越界算命中。
//!
//! 详见 `docs/08-plane.md` §4 与 §6；搬家理由见 `docs/16-client.md` §23。

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

/// 游戏坐标 -> 像素坐标（**实现在 `src/graphwar_plane.rs`**）。
///
/// 归属证据：这条式子的出处 `0x4be3e0` 是 `GraphwarPlane::obstacle_partial_update_event`
/// （`panic::Location` 锚 `src/graphwar_plane.rs:174:18`，两侧双向唯一），不属掩膜模块。
/// 掩膜那两个函数（`0x4f11e0` / `0x4f15b0`）各自内联了同一个式子，所以这里只是**再导出**，
/// 保证 `_scaffold/` 与 `plane.rs` 既有断言不用改。
pub use crate::graphwar_plane::to_pixel;

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

    /// FNV-1a 64：整张掩膜的指纹。
    ///
    /// 为什么不是 sha256：测试不该为了一条断言引依赖；指纹只用来**与本文件注释里的
    /// 真值比对**，真值由 `tools/gen_test_expectations.py` 用同一个 FNV 从真二进制算出来
    /// （该脚本同时把 sha256 记进 `fixtures/test_expectation_truth.tsv`，可交叉复核）。
    fn fnv1a64(b: &[u8]) -> u64 {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        for &x in b {
            h ^= x as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
        h
    }

    /// 与 `0x3f11e0` 开头同式的像素圆心（`trunc` 向零截断）。
    fn pixel_center(x: f64, y: f64) -> (i32, i32) {
        (
            (x * 1000.0 / BOARD_W + 500.0) as i32,
            (y * 1000.0 / BOARD_W + 300.0) as i32,
        )
    }

    /// 以像素圆心为中心的 43×43 邻域（出界不取），逐字节喂给 FNV。
    fn region43(mask: &[u8], x: f64, y: f64) -> u64 {
        let (cx, cy) = pixel_center(x, y);
        let mut v: Vec<u8> = Vec::with_capacity(43 * 43);
        for dy in -21..=21 {
            for dx in -21..=21 {
                let (px, py) = (cx + dx, cy + dy);
                if in_canvas(px, py) {
                    v.push(mask[mask_index(px, py)]);
                }
            }
        }
        fnv1a64(&v)
    }

    /// 非空（`!= 0xFF`）像素数。
    fn nonempty(mask: &[u8]) -> usize {
        mask.iter().filter(|&&b| b != EMPTY).count()
    }

    /// 圆心被硬填充、半径 20 px 之外为空。
    ///
    /// **真值来源（逐位）**：`unicorn` 直调真二进制 `0x3f11e0`
    /// （`void f(double x, double y, double size, long mask_ptr, ulong mask_len, byte fill)`）：
    /// 整张掩膜 FNV-1a 64 = `02b3fa207a593a42`、非空像素 **1369**。
    /// 复算：`.reference/venv/bin/python tools/gen_test_expectations.py`
    /// （产物 `fixtures/test_expectation_truth.tsv` 的 `plane/A04` 段，
    /// 二进制 `libgraphwarrust.so` sha256 `9a538115…`，见 `docs/06` §35）。
    #[test]
    fn raster_center_is_filled_and_bounded() {
        let mut m = empty();
        rasterize(&mut m, 0.0, 0.0, 1.0, 0x00);
        // 圆心被硬填充
        assert_eq!(m[mask_index(500, 300)], 0x00);
        // 半径 20px：(500±25, 300) 一定在圆外
        assert_eq!(m[mask_index(475, 300)], EMPTY);
        assert_eq!(m[mask_index(525, 300)], EMPTY);
        // ★ 真二进制：整张掩膜与非空计数（改前这里只有上面三个自查点）
        assert_eq!(format!("{:016x}", fnv1a64(&m)), "02b3fa207a593a42",
                   "整张掩膜与真二进制 0x3f11e0 的输出不一致");
        assert_eq!(nonempty(&m), 1369, "非空像素数与真二进制不一致");
    }

    /// 抗锯齿环的覆盖率值。
    ///
    /// **真值来源（逐位）**：`unicorn` 直调 `0x3f11e0`。
    /// ① `raster(0,0,1.0,0x00)`：圆心 43×43 邻域 FNV = `c59ec7f3ced7568f`（整张 = `02b3fa207a593a42`）；
    /// ② `raster(-7.5,3.25,0.6,0x00)`（另一个圆心与半径）：整张 FNV = `0c8cecf0b8679dde`，
    ///    圆心邻域 = `9b4da9559b9b22af`，非空 517。
    /// 复算命令同 [`raster_center_is_filled_and_bounded`]。
    #[test]
    fn raster_edge_values_match_the_binary() {
        // 以下两个值取自**真二进制**（unicorn 调用 0x3f11e0 的输出）
        let mut m = empty();
        rasterize(&mut m, 0.0, 0.0, 1.0, 0x00);
        assert_eq!(m[mask_index(491, 282)], 0x1f, "抗锯齿覆盖率不符");
        assert_eq!(m[mask_index(489, 283)], 0x3f, "抗锯齿覆盖率不符");
        // ★ 整块邻域与整张掩膜（改前只有上面两个像素）
        assert_eq!(format!("{:016x}", region43(&m, 0.0, 0.0)), "c59ec7f3ced7568f",
                   "43×43 邻域与真二进制不一致");
        assert_eq!(format!("{:016x}", fnv1a64(&m)), "02b3fa207a593a42");

        // 第二个形态：不同圆心 + 不同半径（size=0.6 ⇒ r = 12 px）
        let mut m2 = empty();
        rasterize(&mut m2, -7.5, 3.25, 0.6, 0x00);
        assert_eq!(nonempty(&m2), 517);
        assert_eq!(format!("{:016x}", fnv1a64(&m2)), "0c8cecf0b8679dde",
                   "(-7.5,3.25) r=12 的掩膜与真二进制不一致");
        assert_eq!(format!("{:016x}", region43(&m2, -7.5, 3.25)), "9b4da9559b9b22af");
    }

    /// 掩膜关于竖直轴对称。
    ///
    /// 改前这是**纯对称性自查**（实现整体歪掉也对称 ⇒ 恒真）。
    /// **真值来源（逐位）**：真二进制 `0x3f11e0` 的整张掩膜 FNV-1a 64 = `02b3fa207a593a42`；
    /// 对称性现在只是它的**推论**，不是证据本身。
    #[test]
    fn raster_is_symmetric() {
        let mut m = empty();
        rasterize(&mut m, 0.0, 0.0, 1.0, 0x00);
        assert_eq!(format!("{:016x}", fnv1a64(&m)), "02b3fa207a593a42",
                   "整张掩膜与真二进制 0x3f11e0 的输出不一致");
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

    /// `fill` 是**参数**（可见值不只有 `0x00/0xFF`）。
    ///
    /// **真值来源（逐位）**：`unicorn` 直调 `0x3f11e0` 的 `raster(0,0,1.0,0x10)`：
    /// 整张 FNV = `79451d1cbca12e52`、圆心邻域 = `9dda82dcc0dce97f`、非空 1369
    /// （与 `fill=0x00` 的非空数相同、但**每个环像素的值不同**）。
    #[test]
    fn raster_respects_fill_value() {
        let mut m = empty();
        rasterize(&mut m, 0.0, 0.0, 1.0, 0x10);
        assert_eq!(m[mask_index(500, 300)], 0x10, "填充值应由参数决定");
        // ★ 真二进制：整张 + 邻域（改前只查了圆心一个像素）
        assert_eq!(format!("{:016x}", fnv1a64(&m)), "79451d1cbca12e52",
                   "fill=0x10 的掩膜与真二进制不一致");
        assert_eq!(format!("{:016x}", region43(&m, 0.0, 0.0)), "9dda82dcc0dce97f");
        assert_ne!(format!("{:016x}", fnv1a64(&m)), "02b3fa207a593a42",
                   "fill 参数没有真正参与混合");
    }

    /// 圆心贴右下角时不写出画布，且画布内留下的痕迹与真二进制逐字节相同。
    ///
    /// **真值来源（逐位）**：`unicorn` 直调 `0x3f11e0` 的 `raster(24.5,14.0,1.0,0x00)`：
    /// 整张 FNV = `632a27f5d728a664`、非空像素 **1053**（全图 600000 ⇒ 绝大多数被裁掉）。
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
        // ★ 真二进制：整张掩膜（改前 "touched" 近乎恒真）
        assert_eq!(nonempty(&m), 1053, "裁剪后的非空像素数与真二进制不一致");
        assert_eq!(format!("{:016x}", fnv1a64(&m)), "632a27f5d728a664",
                   "贴边掩膜与真二进制不一致");
    }

    #[test]
    fn pixel_mapping_truncates_toward_zero() {
        // 实测：游戏坐标 -0.05 -> 像素 499（不是 498），即 -0.05*20+500 = 499.0 截断
        assert_eq!(to_pixel(-0.05, 0.0), (499, 300));
        assert_eq!(to_pixel(0.05, 0.0), (501, 300));
        // 掩膜按像素寻址：标记 499 只影响 -0.05，不影响 0.0
        let mut m = empty_mask();
        m[mask_index(499, 300)] = 0x00;
        assert!(is_occupied(&m, -0.05, 0.0));
        assert!(!is_occupied(&m, 0.0, 0.0));
    }

    /// 单点碰撞判据在**真掩膜**上的判决。
    ///
    /// **真值来源（逐位）**：`unicorn` 直调真二进制 `0x3f15b0`
    /// （`GraphwarObstacleManager::is_obstacle`；`rdi` = manager，`self+0x18` 指向
    /// `ArcInner`，锁字 `+0x10`、`{cap,ptr,len}` 在 `+0x18/+0x20/+0x28`，坐标走**指针** `rsi`），
    /// 输入掩膜 = `0x3f11e0` 的 `raster(0,0,1.0,0x00)` 真输出。
    /// 判决：`(0,0)→1`、`(24,14)→0`、`(-0.05,0)→1`、`(0.05,0)→1`、`(24.99,14.99)→0`。
    ///
    /// 改前这条是"我方掩膜 + 我方判据"互相印证 ⇒ 两边同错也绿；现在两边都是真二进制。
    #[test]
    fn occupied_helper_sees_rasterized_pixels() {
        let mut m = empty();
        rasterize(&mut m, 0.0, 0.0, 1.0, 0x00);
        assert!(is_occupied(&m, 0.0, 0.0), "圆心应判为占据");
        assert!(!is_occupied(&m, 24.0, 14.0), "远处仍为空");
        // ★ 真二进制 0x3f15b0 在这张真掩膜上的判决
        assert!(is_occupied(&m, -0.05, 0.0), "真值 1");
        assert!(is_occupied(&m, 0.05, 0.0), "真值 1");
        assert!(!is_occupied(&m, 24.99, 14.99), "真值 0");
    }
}
