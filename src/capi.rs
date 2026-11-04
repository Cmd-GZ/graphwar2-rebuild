//! 对外 C ABI：把复原的算法从 `.so` 里暴露出来，供外部程序对拍。
//!
//! 与 GDExtension 入口（`gdext.rs`）并列的另一条路：那条要真 Godot 才能验证，
//! 这条只要有二进制 + Python 的 ctypes 就能验证。
//!
//! 约定：
//! - 文本必须是 UTF-8、以 NUL 结尾；
//! - 输出缓冲区是 `f64` 的 `2 * cap` 个元素（x,y 交替）；
//! - 返回值 = 写出的点数；`-1` 表示缓冲区不足；`-2` 表示参数非法（空指针/非 UTF-8）。

use std::ffi::CStr;
use std::os::raw::c_char;

use crate::path::{calculate, Mode};
use crate::shot::{correction, shifted_tokens};
use crate::function::parse;

/// 解析文本，返回 token 个数；`-2` 表示非法输入。
///
/// # Safety
/// `text` 必须是有效的 NUL 结尾 C 字符串。
#[no_mangle]
pub unsafe extern "C" fn gw2_parse_len(text: *const c_char) -> i64 {
    if text.is_null() {
        return -2;
    }
    let s = match CStr::from_ptr(text).to_str() {
        Ok(s) => s,
        Err(_) => return -2,
    };
    parse(s).len() as i64
}

/// 端到端：文本 -> 点序列（含修正常数），写入调用方缓冲区。
///
/// 与游戏服务端一致：`step` 传 0.01、`flip`/`ode` 为 0/1。
///
/// # Safety
/// `text` 有效 NUL 结尾；`out` 至少能容纳 `cap * 2` 个 `f64`。
#[no_mangle]
pub unsafe extern "C" fn gw2_shot_points(
    text: *const c_char,
    x0: f64,
    y0: f64,
    step: f64,
    flip: u8,
    ode: u8,
    out: *mut f64,
    cap: usize,
) -> i64 {
    if text.is_null() || out.is_null() {
        return -2;
    }
    let s = match CStr::from_ptr(text).to_str() {
        Ok(s) => s,
        Err(_) => return -2,
    };
    let mut toks = parse(s);
    if toks.is_empty() {
        return 0;
    }
    let mode = if ode != 0 { Mode::Ode } else { Mode::Normal };
    if mode == Mode::Normal {
        let c = correction(&toks, x0, y0, flip != 0);
        toks = shifted_tokens(&toks, c);
    }
    let pts = calculate(&toks, x0, y0, step, flip != 0, mode);
    if pts.len() > cap {
        return -1;
    }
    for (i, (x, y)) in pts.iter().enumerate() {
        *out.add(i * 2) = *x;
        *out.add(i * 2 + 1) = *y;
    }
    pts.len() as i64
}

/// 只算修正常数（对应 UI 右下角那个数）。
///
/// # Safety
/// `text` 必须是有效的 NUL 结尾 C 字符串。
#[no_mangle]
pub unsafe extern "C" fn gw2_correction(text: *const c_char, x0: f64, y0: f64, flip: u8) -> f64 {
    if text.is_null() {
        return f64::NAN;
    }
    let s = match CStr::from_ptr(text).to_str() {
        Ok(s) => s,
        Err(_) => return f64::NAN,
    };
    let toks = parse(s);
    correction(&toks, x0, y0, flip != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn parse_len_via_c_abi() {
        let c = CString::new("sin(x)").unwrap();
        assert_eq!(unsafe { gw2_parse_len(c.as_ptr()) }, 2); // SIN, VAR_X
    }

    #[test]
    fn null_is_rejected() {
        assert_eq!(unsafe { gw2_parse_len(std::ptr::null()) }, -2);
        assert_eq!(
            unsafe { gw2_shot_points(std::ptr::null(), 0.0, 0.0, 0.01, 0, 0, std::ptr::null_mut(), 0) },
            -2
        );
    }

    #[test]
    fn small_buffer_reports_needed_size() {
        let c = CString::new("sin(x)").unwrap();
        let mut buf = vec![0f64; 4]; // 只够 2 个点
        let n = unsafe { gw2_shot_points(c.as_ptr(), 5.0, 2.0, 0.01, 0, 0, buf.as_mut_ptr(), 2) };
        assert_eq!(n, -1, "缓冲区不足应返回 -1");
    }

    #[test]
    fn first_point_is_the_soldier() {
        let c = CString::new("sin(x)").unwrap();
        let mut buf = vec![0f64; 20_000 * 2];
        let n = unsafe { gw2_shot_points(c.as_ptr(), 5.0, 2.0, 0.01, 0, 0, buf.as_mut_ptr(), 20_000) };
        assert!(n > 100, "正弦曲线点数异常: {n}");
        assert_eq!((buf[0], buf[1]), (5.0, 2.0));
    }
}

/// 光栅化：把实体画进调用方缓冲区（长度应为 1000*600）。
///
/// # Safety
/// `out` 必须至少能容纳 `len` 个字节。
#[no_mangle]
pub unsafe extern "C" fn gw2_rasterize(
    x: f64,
    y: f64,
    size: f64,
    out: *mut u8,
    len: usize,
    fill: u8,
) -> i64 {
    if out.is_null() || len < crate::plane::CANVAS_W as usize * crate::plane::CANVAS_H as usize {
        return -1;
    }
    let buf = std::slice::from_raw_parts_mut(out, len);
    crate::plane::rasterize(buf, x, y, size, fill);
    0
}
