//! 函数语言：文本 -> 前缀 token -> f64。
//!
//! 对应二进制：`src/graphwar_function_input.rs` 解析 `@0x4ed670`、
//! 求值 `@0x4eb9d0`、常量折叠 `@0x4eb830`。
//!
//! 行为规范（**全部已逐位验证**）见 `docs/03-function-language.md`：
//! 小写化、`pi`/`π` → `n`、正则切词、`^` 左结合、一元负号比 `^` 松、
//! `log` = log10、隐式乘法、函数可省略括号、缺失操作数填 `CONST 0`。
//!
//! 参照实现：上游 `graphwar_curve.py` 的 `lex` / `parse` / `evaluate` / `fold`。
//! 移植时逐条对照，不要"顺手改得更好看"——这里的怪癖都是游戏行为。

use crate::Token;

/// 把玩家输入的文本解析成前缀（Polish）token 列表。
///
/// # 状态
/// 尚未移植。参照实现 `graphwar_curve.py::parse`。
pub fn parse(_text: &str) -> Vec<Token> {
    todo!("port from graphwar_curve.py::parse (bit-exact against 0x4ed670)")
}

/// 求值：`f(x, y, dy)`。第三个参数 `dy` 在游戏里**永远传 0**。
///
/// # 状态
/// 尚未移植。参照实现 `graphwar_curve.py::evaluate`。
pub fn eval(_tokens: &[Token], _x: f64, _y: f64, _dy: f64) -> f64 {
    todo!("port from graphwar_curve.py::evaluate (bit-exact against 0x4eb9d0)")
}

/// 常量折叠：把纯常量子树压成单个 `CONST`。
///
/// # 状态
/// 尚未移植。参照实现 `graphwar_curve.py`。
pub fn fold(_tokens: &[Token]) -> Vec<Token> {
    todo!("port from graphwar_curve.py (bit-exact against 0x4eb830)")
}
