//! 函数语言的反方向：前缀 token -> 文本。
//!
//! 对应二进制：`0x4ec160`（递归函数，签名 `fn(out: &mut String, buf: &TokenBuf,
//! pos: &mut usize)`，sret 在 `rdi`）。默认分支 panic 的 `Location` 指向
//! **`src/math_function.rs:285:28`**（`.data.rel.ro` 的 `@0x95fea8` + 字符串
//! `@0x1b52cd`），所以它和 `Function` 枚举、`parse`、`eval` 同属"数学函数"这一层——
//! 本 crate 因此把它放在 `src/math_function/`，而不是 `src/events.rs`。
//! 依据与地址见 `docs/119-token-to-text.md`。
//!
//! 行为规范（全部由 unicorn 直接调 `0x4ec160` 逐例验证，见 `fixtures/to_text.tsv`）：
//!
//! | token | 输出 |
//! |---|---|
//! | `x` / `y` / `y'` | `x` / `y` / `y'` |
//! | `sqrt`..`tan` | `名字(` + 子式 + `)` |
//! | `const v` | `{:.2}` 两位小数，**没有** π/e 特判 |
//! | `+ - * / ^` | `(` + 左 + 符号 + 右 + `)` |
//! | `neg` | `-` + 子式（**不加括号**） |
//! | 越界（子式缺失） | 空串，于是会出现 `sin()` / `(+x)` 这种畸形但合法的输出 |
//! | `(` / `)` / tag > 17 | 原程序 **panic**（`"Invalid token "`，`0x4ece6d`） |
//!
//! 空格：**一个都不加**。真实抓包里的 AI 产物 `(-1.06*x)`、`abs((-3.49+x))`、
//! `tan(sin((-0.12*x)))` 都能逐字符对上（`docs/AI.md` §2.1）。

use crate::{Function, Token};
use crate::Function::*;

/// 常量固定输出两位小数（`{:.2}`）。
///
/// 这是**截断精度**的行为，不是显示选项：`π` 输出 `3.14`，`x/3` 的 `3.0` 输出 `3.00`。
/// 所以 `parse -> to_text -> parse` 对"不能用两位小数精确表示"的常量**不等价**
/// （见 `docs/119` §5）。
pub const CONST_DECIMALS: usize = 2;

/// 非法 token：原程序在 `0x4ece6d` 直接 `panic!("Invalid token")`。
///
/// 我们不用 panic 表达它——服务端不该因为一条畸形 token 流就 abort。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidToken {
    /// 出问题的判别值（`18`/`19` = 括号，或更大的未知值）。
    pub tag: u64,
}

/// 递归写出一棵前缀子树，`*pos` 前进到该子树之后。
///
/// 与二进制逐条对应：读越界 → 写空串（所以 `sin` 后面没 token 时得到 `sin()`）。
fn emit(out: &mut String, tokens: &[Token], pos: &mut usize) -> Result<(), InvalidToken> {
    let t = match tokens.get(*pos) {
        Some(t) => *t,
        // 4ec17b: `cmp 0x10(%rsi),%rcx; jae 4ec1dd` -> 返回空 String
        None => return Ok(()),
    };
    *pos += 1;
    match t.tag {
        // 4ec1d5 / 4ec9ae / 4ec679：单字符字面量
        VarX => out.push('x'),
        VarY => out.push('y'),
        VarDy => out.push_str("y'"),
        // 4ec9c9：`{:.2}`，直接格式化 value。**没有** π/e 特判
        Const => write_const(out, t.value),
        // 4ec407/4ec7be 等：`(` 操作数 符号 操作数 `)`
        Add => binary(out, tokens, pos, '+')?,
        Sub => binary(out, tokens, pos, '-')?,
        Mul => binary(out, tokens, pos, '*')?,
        Divide => binary(out, tokens, pos, '/')?,
        Pow => binary(out, tokens, pos, '^')?,
        // 4eca24：只写 `-`，**不加括号**
        UnaryMinus => {
            out.push('-');
            emit(out, tokens, pos)?;
        }
        // 4ec5b3/4ecc4b/4ec33f/4ec510/4eca7d/4ec29c/4ec696/4ec1f9
        Sqrt => call(out, tokens, pos, "sqrt")?,
        Log => call(out, tokens, pos, "log")?,
        Ln => call(out, tokens, pos, "ln")?,
        Exp => call(out, tokens, pos, "exp")?,
        Abs => call(out, tokens, pos, "abs")?,
        Sin => call(out, tokens, pos, "sin")?,
        Cos => call(out, tokens, pos, "cos")?,
        Tan => call(out, tokens, pos, "tan")?,
        // 4ece6d: `panic!("Invalid token")`。括号 token 不可能出现在
        // `parse` 的产物里（`flatten` 只写 Const 与运算符），所以线上走不到这里。
        LeftBracket | RightBracket => {
            return Err(InvalidToken { tag: t.tag as u64 });
        }
    }
    Ok(())
}

/// 常量：`{:.2}`。
///
/// 用 `core::fmt` 的定精度浮点格式化，与二进制**逐字符相同**：
/// 大数不换科学计数法（`1e300` 展开成 300 位整数部分）、`NaN`/`inf`/`-inf`
/// 就是这三个词、`-0.0` 输出 `-0.00`、`0.125 -> 0.12`（按**确切二进制值**四舍五入到偶数）。
fn write_const(out: &mut String, v: f64) {
    use std::fmt::Write;
    // `write!` 到 String 不会失败（`fmt::Write for String` 的 push_str 无错误分支）
    let _ = write!(out, "{:.*}", CONST_DECIMALS, v);
}

fn binary(
    out: &mut String,
    tokens: &[Token],
    pos: &mut usize,
    op: char,
) -> Result<(), InvalidToken> {
    out.push('(');
    emit(out, tokens, pos)?;
    out.push(op);
    emit(out, tokens, pos)?;
    out.push(')');
    Ok(())
}

fn call(out: &mut String, tokens: &[Token], pos: &mut usize, name: &str) -> Result<(), InvalidToken> {
    out.push_str(name);
    out.push('(');
    emit(out, tokens, pos)?;
    out.push(')');
    Ok(())
}

/// 前缀 token -> 文本。**与 [`parse`](super::parse) 对称**。
///
/// 畸形流不会 panic：越界的操作数写成空串（和二进制一致），
/// 遇到括号 token 时把整棵树当成 [`InvalidToken`]，本函数返回**空串**——
/// 调用方若需要区分"真空函数"和"非法函数"，用 [`try_to_text`]。
pub fn to_text(tokens: &[Token]) -> String {
    try_to_text(tokens).unwrap_or_default()
}

/// 同 [`to_text`]，但把非法 token 流报告为 `None`。
///
/// 二进制在这里 panic（`Invalid token`，`0x4ece6d`）；我们没有对应物，
/// 所以把"会 panic 的输入"映射成 `None`。
pub fn try_to_text(tokens: &[Token]) -> Option<String> {
    let mut out = String::new();
    let mut pos = 0usize;
    match emit(&mut out, tokens, &mut pos) {
        Ok(()) => Some(out),
        Err(_) => None,
    }
}

/// 只格式化一个常量，用于测试与文档（`{:.2}` 的唯一真源）。
pub fn const_text(v: f64) -> String {
    let mut s = String::new();
    write_const(&mut s, v);
    s
}

/// 供调用方判断：这个 [`Function`] 是不是 `to_text` 认识的 token。
pub fn is_printable(tag: Function) -> bool {
    !matches!(tag, LeftBracket | RightBracket)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(tag: Function) -> Token {
        Token::op(tag)
    }

    #[test]
    fn leaves_and_functions() {
        assert_eq!(to_text(&[t(VarX)]), "x");
        assert_eq!(to_text(&[t(VarY)]), "y");
        assert_eq!(to_text(&[t(VarDy)]), "y'");
        assert_eq!(to_text(&[t(Sin), t(VarX)]), "sin(x)");
        assert_eq!(to_text(&[t(Tan), t(Sin), t(VarX)]), "tan(sin(x))");
        assert_eq!(to_text(&[t(Sqrt), t(Abs), t(VarX)]), "sqrt(abs(x))");
        // 函数括号 + 二元括号两层都在（真二进制 `sqrt((x+1.00))`，见 docs/119 §3.2）
        let toks = [t(Sqrt), t(Add), t(VarX), Token::constant(1.0)];
        assert_eq!(to_text(&toks), "sqrt((x+1.00))");
    }

    #[test]
    fn binary_ops_are_always_parenthesised() {
        let toks = [t(Add), t(Mul), Token::constant(2.0), t(VarX), Token::constant(1.0)];
        assert_eq!(to_text(&toks), "((2.00*x)+1.00)");
        // ^ 的左结合结构原样保留：((x^2.00)^3.00)
        let toks = [t(Pow), t(Pow), t(VarX), Token::constant(2.0), Token::constant(3.0)];
        assert_eq!(to_text(&toks), "((x^2.00)^3.00)");
    }

    #[test]
    fn unary_minus_has_no_parentheses() {
        assert_eq!(to_text(&[t(UnaryMinus), t(VarX)]), "-x");
        assert_eq!(to_text(&[t(UnaryMinus), t(UnaryMinus), t(VarX)]), "--x");
        let toks = [t(Mul), t(UnaryMinus), Token::constant(1.06), t(VarX)];
        assert_eq!(to_text(&toks), "(-1.06*x)");
    }

    #[test]
    fn constants_use_two_decimals() {
        assert_eq!(const_text(0.0), "0.00");
        assert_eq!(const_text(-0.0), "-0.00");
        assert_eq!(const_text(1.0), "1.00");
        assert_eq!(const_text(0.5), "0.50");
        assert_eq!(const_text(12.3456), "12.35");
        assert_eq!(const_text(f64::NAN), "NaN");
        assert_eq!(const_text(f64::INFINITY), "inf");
        assert_eq!(const_text(f64::NEG_INFINITY), "-inf");
        // 没有 π / e 特判
        assert_eq!(const_text(crate::PI), "3.14");
        assert_eq!(const_text(crate::E), "2.72");
    }

    #[test]
    fn missing_operands_become_empty_text() {
        assert_eq!(to_text(&[]), "");
        assert_eq!(to_text(&[t(Sin)]), "sin()");
        assert_eq!(to_text(&[t(Add), t(VarX)]), "(x+)");
        assert_eq!(to_text(&[t(UnaryMinus)]), "-");
    }

    #[test]
    fn brackets_are_reported_not_panicked() {
        let toks = [t(VarX), t(LeftBracket), t(RightBracket)];
        assert_eq!(to_text(&toks), "x"); // 只消费第一个 token，与二进制一致
        assert_eq!(try_to_text(&toks).as_deref(), Some("x"));
        assert_eq!(try_to_text(&[t(LeftBracket)]), None);
        assert_eq!(to_text(&[t(LeftBracket)]), "");
    }

    #[test]
    fn extra_tokens_are_left_unconsumed() {
        // 二进制只消费一棵子树；多出来的 token 不参与输出
        assert_eq!(to_text(&[t(VarX), t(VarY)]), "x");
    }
}
