//! 坐标变换与掩膜测试的验收。
//!
//! 期望值来自 `docs/08-plane.md` §4 的变换式与 §6 的掩膜语义，
//! 二者都是对 Ghidra 伪代码的直接读取（观测），不是猜的。

use gw2_rebuild::plane::*;

#[test]
fn canvas_corners_match_the_board() {
    // x∈[-25,25], y∈[-15,15] 覆盖整张 1000x600 画布
    assert_eq!(to_pixel(-25.0, -15.0), (0, 0));
    assert_eq!(to_pixel(0.0, 0.0), (500, 300));
    // 正边界落在画布外（半个像素之外）：原程序用无符号比较，等同"越界"
    let (px, py) = to_pixel(25.0, 15.0);
    assert_eq!((px, py), (1000, 600));
    assert!(!in_canvas(px, py));
}

#[test]
fn scale_is_twenty_pixels_per_unit() {
    assert_eq!(PX_PER_UNIT, 20.0);
    assert_eq!(to_pixel(1.0, 0.0).0 - to_pixel(0.0, 0.0).0, 20);
    assert_eq!(to_pixel(0.0, 1.0).1 - to_pixel(0.0, 0.0).1, 20);
}

#[test]
fn empty_mask_reports_free() {
    let mask = empty_mask();
    assert_eq!(mask.len(), 1000 * 600);
    assert!(!is_occupied(&mask, 0.0, 0.0));
    assert!(!is_occupied(&mask, -24.0, 14.0));
}

#[test]
fn marked_pixel_reports_occupied() {
    let mut mask = empty_mask();
    let (px, py) = to_pixel(10.0, -5.0);
    assert!(in_canvas(px, py));
    mask[mask_index(px, py)] = 0x01;
    assert!(is_occupied(&mask, 10.0, -5.0));
    // 相邻点仍然空
    assert!(!is_occupied(&mask, 10.1, -5.0));
}

#[test]
fn out_of_canvas_counts_as_hit() {
    // 原程序：越界直接返回 1（命中），这是玩法语义不是防御式编程
    let mask = empty_mask();
    assert!(is_occupied(&mask, 30.0, 0.0));
    assert!(is_occupied(&mask, 0.0, 20.0));
    assert!(is_occupied(&mask, -30.0, 0.0));
}

#[test]
fn zero_is_not_empty() {
    // 只有 0xFF 是空；0x00 是"被占据"
    let mut mask = empty_mask();
    mask[mask_index(500, 300)] = 0x00;
    assert!(is_occupied(&mask, 0.0, 0.0));
}
