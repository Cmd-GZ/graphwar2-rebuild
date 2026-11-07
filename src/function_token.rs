//! 原版 `src/function_token.rs` —— **一个 token，以及"字符串 → token"这张表**。
//!
//! ## 这个模块是干什么的（2026-11-07 定性，证据见 `docs/16-client.md` §30）
//!
//! `FunctionToken` 就是全项目一直在用的那个 16 字节 token：`{ tag: u64, value: f64 }`
//! （本 crate 里叫 [`crate::Token`] / [`crate::Function`]，见 `src/lib.rs`）。
//! 本模块**只多一个东西**：`FunctionToken::new(&str)` —— 把正则切出来的**一个片段**
//! 变成 token。它是解析器的**最底层**，比 `math_function` 还靠下。
//!
//! arm64 2.0.60 未 strip 版（`graphwarrust::function_token::FunctionToken::new` @ `0x3c0da0`，
//! 860 字节）**只有一个调用者**：`MathFunction::new`（`0x446c4c`）+`0x28c`。
//! 也就是说整份游戏里"字符串 → tag"这一步只发生在解析器里。
//!
//! ## 三个容易搞错的地方（都是逐指令读出来的，不是猜的）
//!
//! 1. **`new` 的返回值不是 `Option` 语义**，原版用 **tag 20** 表示"这个片段我不认识"。
//!    本 crate 的 [`crate::Function`] 只枚举了 `0..=19`，所以这里映射成 `None`。
//!    tag 20 在原版里**不可达**：它只可能由 `FunctionToken::new` 对正则匹配不到的串产生，
//!    而正则是全封闭的（见 [`TOKEN_PATTERN`]）。
//! 2. **数字优先**：先 `str::parse::<f64>()`，成功就是 `CONST`（tag 17）+ 该值；
//!    失败才走符号表。所以 `"1"` 永远不会被当成别的。
//! 3. **`pi` 和 `π` 在到达这里之前就被换成了 `n`**（`MathFunction::new` 里两次
//!    `str::replace`，见 [`PI_ALIAS_UNREACHABLE`]）。`new` 里仍然留着 `"pi"` 那一臂，
//!    是为了忠实照抄机器码——它是**死臂**。
//!
//! ## 上游怎么用它（`MathFunction::new`，arm64 `0x446c4c`）
//!
//! ```text
//! 输入串
//!   ├─ 逐字节 to_ascii_lowercase（只折 A–Z，`sub w12,w11,#0x41; cmp w12,#0x1a`）
//!   ├─ str::replace("pi", "n")          ← 0x19dcaf / 0x19dcb1
//!   ├─ str::replace("π", "n")           ← 0x19dcb2（2 字节 CF 80）
//!   ├─ Regex::new(TOKEN_PATTERN).find_iter(..).map(FunctionToken::new)  ← 每匹配片段一次
//!   └─ PolishNotationConverter::reorder_recursive(0, len-1)  ← 中缀 → 前缀
//! ```
//!
//! 注意 `replace` 是**先 `pi` 后 `π`**，而且替换目标都是单字节 `n`（长度 2 → 1）。

use crate::{Function, Token};

/// 原版 `FunctionToken`：与 [`crate::Token`] **同一个类型**（同为 16 字节
/// `{tag: u64, value: f64}`，`docs/03-function-language.md` §2 的编码）。
///
/// 这里用别名而不是新结构体：原版 `polish_notation_converter::PolishNotationConverter`
/// 的两个 `Vec` 里装的就是它，本 crate 的 `Token` 已经是同一布局，
/// 再造一个只会在两处之间加转换代码。
pub type FunctionToken = Token;

/// **原版解析器的正则**，逐字节取自 `.rodata`（arm64 `0x19dcb4`，x86 `0x1c4347`，
/// 两份内容逐字节相同；长度 95，无 NUL，`SHF_MERGE|SHF_STRINGS` 合并节）。
///
/// ```text
/// [0-9]*\.?[0-9]+|\(|\)|x|y'|y|\+|-|\*|/|\^|sqrt|log|abs|sin|cos|tan|ln|n|exp|e
/// ```
///
/// 三个可观测后果：
///
/// * **交替顺序就是优先级**：`y'` 排在 `y` 前面，否则 `y'` 会被切成 `y` + 残渣；
///   `exp` 排在 `e` 前面；`sqrt`/`log`/`sin`/`cos`/`tan`/`abs`/`ln` 都在 `e`/`n` 前面。
/// * **数字必须带小数点后至少一位**：`[0-9]*\.?[0-9]+` 里最后那段 `[0-9]+` 硬性要求数字，
///   所以 `5.` 只匹配到 `5`（那个 `.` 被 `find_iter` 跳过、不留 token）。
/// * **没有隐式乘法的义务**：正则只切 token，"`2x` 变成 `2 * x`"这类重写不在这里发生
///   （见 `docs/03-function-language.md` §13：解析器不插乘号，缺操作数的洞由
///   `polish_notation_converter` 里的 `CONST 0` 兜住）。
pub const TOKEN_PATTERN: &str =
    r"[0-9]*\.?[0-9]+|\(|\)|x|y'|y|\+|-|\*|/|\^|sqrt|log|abs|sin|cos|tan|ln|n|exp|e";

/// `"pi"` → `"n"` 的替换发生在 `FunctionToken::new` **之前**，
/// 所以 `new("pi")` 这一臂在原版里是**死臂**（机器码里还在，见 `0x3c0dea`）。
///
/// 留着它是因为"忠实照抄"比"删掉看不懂的代码"更安全：
/// 万一有人绕过 `MathFunction::new` 直接喂 `"pi"`，行为仍与机器码一致（π）。
pub const PI_ALIAS_UNREACHABLE: &str = "pi";

/// `^` 的**第二写法**：`U+02C6 MODIFIER LETTER CIRCUMFLEX ACCENT`（"ˆ"，UTF-8 `CB 86`）。
///
/// 证据：`FunctionToken::new` 的 `len == 2` 分支里 `ldrh w8,[x19]` 与 `mov w9,#0x86cb`
/// 比较，命中后返回 **tag 8**（`^`）。这条**不在**任何文档里过，是逐指令读出来的。
pub const CARET_ALIAS: char = '\u{02c6}';

/// 把**一个正则匹配片段**变成一个 token。
///
/// `None` = 原版返回 tag 20（"不认识"）。机器码里 20 是不可达值，见模块文档第 1 条。
///
/// 顺序照抄机器码：
///
/// 1. `parse::<f64>()` 成功 ⇒ `CONST(value)`（`0x3c0dbc` 调 `float_parse`，
///    失败分支 `0x3c0e3c` 直接返回 `tag=0x11` + 解析出的 `f64`）；
/// 2. 否则按**长度**分派：1 / 2 / 3 / 4 字节各一张表（`cmp x20,#0x1/#0x2/#0x3/#0x4`）。
pub fn new(text: &str) -> Option<FunctionToken> {
    if let Ok(value) = text.parse::<f64>() {
        return Some(Token::constant(value));
    }
    let tag = match text {
        // 1 字节：0x3c0e90 的 82 项字符跳转表（覆盖 '('.=0x79 即 'y'）
        "x" => Function::VarX,
        "y" => Function::VarY,
        "+" => Function::Add,
        "-" => Function::Sub,
        "*" => Function::Mul,
        "/" => Function::Divide,
        "^" => Function::Pow,
        "(" => Function::LeftBracket,
        ")" => Function::RightBracket,
        "n" => return Some(Token::constant(crate::PI)),
        "e" => return Some(Token::constant(crate::E)),
        // 2 字节：0x3c0de8 的四次 16 位比较
        "y'" => Function::VarDy,
        "ln" => Function::Ln,
        PI_ALIAS_UNREACHABLE => return Some(Token::constant(crate::PI)),
        // 3 字节：0x3c0ee0 的六次 (16 位 + 8 位) 比较
        "log" => Function::Log,
        "abs" => Function::Abs,
        "sin" => Function::Sin,
        "cos" => Function::Cos,
        "tan" => Function::Tan,
        "exp" => Function::Exp,
        // 4 字节：0x3c0e64 的一次 32 位比较
        "sqrt" => Function::Sqrt,
        _ => return None,
    };
    Some(Token::op(tag))
}

/// [`new`] 的字符版：处理 [`CARET_ALIAS`] 这类"长度 2 的单个字符"。
///
/// 单独一个函数是因为 `match &str` 里没法直接写 `char` 字面量做相等——
/// 机器码里那一臂比较的是**两个字节** `[x19]`/`[x19+1]`，等价于"这个 `&str` 就是
/// `CARET_ALIAS` 这一个字符"。
pub fn new_with_caret_alias(text: &str) -> Option<FunctionToken> {
    if text.chars().count() == 1 && text.starts_with(CARET_ALIAS) {
        return Some(Token::op(Function::Pow));
    }
    new(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Function;

    #[test]
    fn numbers_become_const_tokens() {
        assert_eq!(new("0.5"), Some(Token::constant(0.5)));
        assert_eq!(new("12"), Some(Token::constant(12.0)));
        // 原版正则允许 ".5"（`[0-9]*` 可以为空）
        assert_eq!(new(".5"), Some(Token::constant(0.5)));
    }

    #[test]
    fn single_char_tokens_match_the_jump_table() {
        for (s, f) in [
            ("x", Function::VarX),
            ("y", Function::VarY),
            ("+", Function::Add),
            ("-", Function::Sub),
            ("*", Function::Mul),
            ("/", Function::Divide),
            ("^", Function::Pow),
            ("(", Function::LeftBracket),
            (")", Function::RightBracket),
        ] {
            assert_eq!(new(s), Some(Token::op(f)), "token {s:?}");
        }
    }

    #[test]
    fn brackets_are_tags_18_and_19() {
        // 这两个值决定 polish_notation_converter 的深度计数
        assert_eq!(Function::LeftBracket as u64, 18);
        assert_eq!(Function::RightBracket as u64, 19);
        assert_eq!(new("(").unwrap().tag as u64, 18);
        assert_eq!(new(")").unwrap().tag as u64, 19);
    }

    #[test]
    fn word_tokens() {
        for (s, f) in [
            ("sqrt", Function::Sqrt),
            ("log", Function::Log),
            ("ln", Function::Ln),
            ("exp", Function::Exp),
            ("abs", Function::Abs),
            ("sin", Function::Sin),
            ("cos", Function::Cos),
            ("tan", Function::Tan),
            ("y'", Function::VarDy),
        ] {
            assert_eq!(new(s), Some(Token::op(f)), "token {s:?}");
        }
    }

    #[test]
    fn n_and_e_are_constants_not_functions() {
        assert_eq!(new("n"), Some(Token::constant(std::f64::consts::PI)));
        assert_eq!(new("e"), Some(Token::constant(std::f64::consts::E)));
    }

    #[test]
    fn caret_alias_maps_to_pow() {
        assert_eq!(new_with_caret_alias("\u{02c6}"), Some(Token::op(Function::Pow)));
    }

    #[test]
    fn unknown_fragment_is_none() {
        assert_eq!(new("%"), None);
        assert_eq!(new("sinx"), None);
    }
}
