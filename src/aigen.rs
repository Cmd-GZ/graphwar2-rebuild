//! 随机函数生成器：AI 的初始种群与变异都靠它抽 token。
//!
//! 全部数值直接读自二进制（`docs/71-token-weights.md`）：
//!
//! * 加权抽样用 `random % 85`，按区间映射到 tag；
//! * 常数 token = `N(0,1) × 10.2`（缩放常数在 `.rodata 0x19f758`）；
//! * 权重表见 [`TABLE`]，合计 [`TOTAL_WEIGHT`] = 85。
//!
//! 运行时用 40 万样本核过一遍（`tools/hook_token_tags.js`），
//! 实测频率与下表逐项吻合到小数点后第二位。

use crate::levelgen::normal;
use crate::{Function, Token};

/// 权重总和，也就是二进制里 `random % 0x55` 的模数。
pub const TOTAL_WEIGHT: u32 = 85;

/// 常数 token 的标准差：`N(0,1) × 10.2`。
///
/// GW1 的 `RANDOM_FUNC_VALUE_MEAN` 同样是 10.2，两代一致。
pub const CONST_SCALE: f64 = 10.2;

/// `(tag, 权重)`，按分派表里的下标顺序排列。
///
/// 注意 `VarX` 的权重是 10 而不是 6：分派表里 11–16 走立即数分支、
/// 17–20 走"栈上预存值"分支，普通模式下两者都产出 `x`。
/// 运行时实测 `x` 占 11.77%（= 10/85），据此确认。
pub const TABLE: [(Function, u32); 15] = [
    (Function::Const, 11),
    (Function::VarX, 10),
    (Function::Add, 10),
    (Function::Sub, 5),
    (Function::Mul, 11),
    (Function::Divide, 7),
    (Function::Pow, 2),
    (Function::Sqrt, 1),
    (Function::Log, 1),
    (Function::Ln, 1),
    (Function::Exp, 6),
    (Function::Abs, 7),
    (Function::Sin, 7),
    (Function::Cos, 5),
    (Function::Tan, 1),
];

/// 抽一个 tag（不含常数的取值）。
///
/// 对应二进制 `FUN_005ef020` 里的 `idx = random % 85` 加区间比较；
/// `next` 是 ChaCha 的取数闭包。
pub fn draw_tag(next: &mut dyn FnMut() -> u64) -> Function {
    let mut idx = (next() % u64::from(TOTAL_WEIGHT)) as u32;
    for &(tag, weight) in TABLE.iter() {
        if idx < weight {
            return tag;
        }
        idx -= weight;
    }
    // 权重和若不是 85 才会走到这里；保底返回最后一个，便于测试发现表被改坏。
    debug_assert!(false, "TABLE 权重之和应等于 TOTAL_WEIGHT");
    Function::Tan
}

/// 抽一个完整 token；抽到常数时按 `N(0,1) × 10.2` 取值。
pub fn draw_token(next: &mut dyn FnMut() -> u64) -> Token {
    let tag = draw_tag(next);
    if tag == Function::Const {
        Token::constant(normal(next) * CONST_SCALE)
    } else {
        Token::op(tag)
    }
}

/// 变异算子（`FUN_005ee720`，见 `docs/73`）。
///
/// 在 token 序列上挑一个"值 token"（`x` / `y` / 常数），然后：
///
/// * 挑到 `y`   → 在它**前面插入**一个常数 `N(0,1)×10.2`；
/// * 挑到常数   → 一半概率原地乘 `(1 + N(0,1))`（这就是常数漂移的来源），
///                一半概率换成新的 `N(0,1)×10.2`；
/// * 挑到其它   → **未实现**（`docs/73` 里这条分支的插入/替换次序还没读准，
///                按仓库规矩不臆造，直接原样返回）。
///
/// 返回是否真的改动了序列，便于调用方统计。
pub fn mutate(tokens: &mut Vec<Token>, next: &mut dyn FnMut() -> u64) -> bool {
    let positions: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t.tag, Function::VarX | Function::VarY | Function::Const))
        .map(|(i, _)| i)
        .collect();
    if positions.is_empty() {
        return false;
    }
    let pos = positions[(next() % positions.len() as u64) as usize];
    match tokens[pos].tag {
        Function::VarY => {
            tokens.insert(pos, Token::constant(normal(next) * CONST_SCALE));
            true
        }
        Function::Const => {
            // 二进制里是"抽一个随机 int，看符号"决定走哪条路
            if (next() as i32) >= 0 {
                tokens[pos].value *= 1.0 + normal(next);
            } else {
                tokens[pos].value = normal(next) * CONST_SCALE;
            }
            true
        }
        // 其它（`x` 或算子）：那条分支的语义未读准，见函数注释
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::ChaCha12;

    fn stream(seed: u64) -> impl FnMut() -> u64 {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&seed.to_le_bytes());
        let mut r = ChaCha12::from_seed(&key);
        move || r.next_u64()
    }

    #[test]
    fn weights_sum_to_85() {
        let sum: u32 = TABLE.iter().map(|(_, w)| *w).sum();
        assert_eq!(sum, TOTAL_WEIGHT, "权重之和必须等于模数 85");
        assert_eq!(TABLE.len(), 15, "分派表共 15 个非零区间");
    }

    #[test]
    fn distribution_matches_table() {
        let mut next = stream(0x9E37_79B9_7F4A_7C15);
        const N: u32 = 170_000; // 2000 轮 × 85
        let mut got = [0u32; 20];
        for _ in 0..N {
            got[draw_tag(&mut next) as usize] += 1;
        }
        for &(tag, weight) in TABLE.iter() {
            let expect = (N * weight) / TOTAL_WEIGHT;
            let actual = got[tag as usize];
            let diff = (actual as i64 - expect as i64).unsigned_abs();
            // 34 次抽样里单点标准差约 sqrt(N·p·(1-p))，这里放宽到 5%
            assert!(
                diff < expect as u64 / 20 + 50,
                "{tag:?}: 期望 {expect}，实测 {actual}"
            );
        }
    }

    #[test]
    fn constants_are_normal_scaled_by_10_2() {
        let mut next = stream(0x1234_5678_9ABC_DEF1);
        let mut values = Vec::new();
        while values.len() < 20_000 {
            let t = draw_token(&mut next);
            if t.tag == Function::Const {
                values.push(t.value);
            }
        }
        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
        let sd = var.sqrt();
        assert!(mean.abs() < 1.0, "常数均值应接近 0，实测 {mean}");
        assert!(
            (sd - CONST_SCALE).abs() < 0.8,
            "常数标准差应接近 {CONST_SCALE}，实测 {sd}"
        );
    }

    #[test]
    fn never_emits_y_or_neg() {
        // 分派表里没有 VarY / VarDy / UnaryMinus / 括号，NORMAL 模式不应出现
        let mut next = stream(42);
        for _ in 0..10_000 {
            let tag = draw_tag(&mut next);
            assert!(
                tag != Function::VarY
                    && tag != Function::VarDy
                    && tag != Function::UnaryMinus
                    && tag != Function::LeftBracket
                    && tag != Function::RightBracket,
                "不应抽到 {tag:?}"
            );
        }
    }

    #[test]
    fn mutate_does_nothing_without_value_tokens() {
        let mut tokens = vec![Token::op(Function::Add), Token::op(Function::Sin)];
        let mut next = stream(7);
        assert!(!mutate(&mut tokens, &mut next), "没有值 token 时不应改动");
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn mutate_inserts_constant_before_y() {
        let mut tokens = vec![Token::op(Function::VarY)];
        let mut next = stream(11);
        assert!(mutate(&mut tokens, &mut next));
        assert_eq!(tokens.len(), 2, "应在 y 前插入一个常数");
        assert_eq!(tokens[0].tag, Function::Const);
        assert_eq!(tokens[1].tag, Function::VarY);
    }

    #[test]
    fn mutate_constant_is_multiplicative_drift_or_refresh() {
        // 反复变异同一个常数：结果必须是"乘上 (1+N)"或"换成新的 N(0,10.2)"之一，
        // 所以绝不能保持原值不变。
        let mut changed = 0;
        for seed in 0..200u64 {
            let mut tokens = vec![Token::constant(3.0)];
            let mut next = stream(seed * 7919 + 13);
            assert!(mutate(&mut tokens, &mut next));
            if tokens[0].value != 3.0 {
                changed += 1;
            }
        }
        assert_eq!(changed, 200, "每次变异都应改变常数");
    }

    #[test]
    fn mutate_spreads_constants_over_many_rounds() {
        // 乘性随机游走：多轮之后常数会明显偏离初值（这正是 1770417.48 的成因）
        let mut next = stream(0xDEAD_BEEF);
        let mut tokens = vec![Token::constant(10.0)];
        for _ in 0..400 {
            let mut t = tokens.clone();
            mutate(&mut t, &mut next);
            if t[0].tag == Function::Const && t.len() == 1 {
                tokens = t;
            }
        }
        assert!(
            (tokens[0].value - 10.0).abs() > 1e-9,
            "多轮变异后常数不应仍是原值"
        );
    }
}
