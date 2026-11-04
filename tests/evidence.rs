//! 把常量钉在二进制证据上。
//!
//! 这些断言唯一的作用是：任何人改动常量时，必须同时改测试——
//! 而测试里写的是 `.rodata` 的原始比特，改它就等于宣称"二进制变了"。

use gw2_rebuild::*;

#[test]
fn step_default_is_bit_exact_server_value() {
    // 服务端构造点 0x4717f7: movabs rax, 0x3f847ae147ae147b
    assert_eq!(STEP_DEFAULT.to_bits(), 0x3f847ae147ae147b);
}

#[test]
fn plane_bounds_match_rodata() {
    assert_eq!(MAX_ABS_X.to_bits(), 25.0f64.to_bits());
    assert_eq!(MAX_ABS_Y.to_bits(), 15.0f64.to_bits());
}

#[test]
fn length_tolerance_matches_rodata() {
    assert_eq!(LENGTH_TOL.to_bits(), 0.001f64.to_bits());
}

#[test]
fn point_budget_is_200_over_step() {
    // 服务端 step=0.01 -> 20000 点
    let n = (TOTAL_LEN / STEP_DEFAULT) as i32;
    assert_eq!(n, 20000);
    // 饱和语义：极小 step 不会回绕成负数
    let saturated = (TOTAL_LEN / 1e-300) as i32;
    assert_eq!(saturated, i32::MAX);
}

#[test]
fn function_discriminants_are_stable() {
    // 变体名在 .rodata 里按此顺序连续排列
    assert_eq!(Function::VarX as u64, 0);
    assert_eq!(Function::VarDy as u64, 2);
    assert_eq!(Function::Pow as u64, 8);
    assert_eq!(Function::Log as u64, 10);
    assert_eq!(Function::Const as u64, 17);
    assert_eq!(Function::RightBracket as u64, 19);
}

#[test]
fn token_is_sixteen_bytes() {
    // 原程序里 token 是 { tag: u64, value: f64 }
    assert_eq!(std::mem::size_of::<Token>(), 16);
}
