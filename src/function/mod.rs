//! 函数语言：文本 -> 前缀 token -> f64。
//!
//! 对应二进制：`src/graphwar_function_input.rs` 解析 `@0x4ed670`、
//! 求值 `@0x4eb9d0`。
//!
//! 行为规范见 `docs/03-function-language.md`。所有怪癖都是游戏行为，不是实现缺陷：
//! `^` 左结合、一元负号比 `^` 松、`log` 是 log10、隐式乘法、函数可省括号、
//! 缺失操作数不报错。
//!
//! 移植自上游已验证的 `graphwar_curve.py`（`lex` / `_Parser` / `Evaluator`）。
//! 注意 `_flatten` 遇到缺失子节点时**不补任何 token**——求值时后面的 token 会被
//! 当成缺的那个操作数读走，这正是游戏"缺操作数也照样出图"的来源。

use crate::{Token, E, PI};
use crate::Function::{self, *};

// ---------------------------------------------------------------------------
// 1. 词法
// ---------------------------------------------------------------------------

fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}

/// 在 `s[i..]` 处尝试匹配 `[0-9]*\.?[0-9]+`，返回 (结束位置, 值)。
///
/// 与原实现的正则在**贪婪 + 回溯**上等价：先尽量多吃数字，
/// 若后面没有小数点或小数位不足，就回退到更短的整体匹配。
fn match_number(s: &[u8], i: usize) -> Option<(usize, f64)> {
    let n = s.len();
    let mut j = i;
    while j < n && is_digit(s[j]) {
        j += 1;
    }
    // 贪婪版：[0-9]* .? [0-9]+
    if j < n && s[j] == b'.' {
        let mut k = j + 1;
        while k < n && is_digit(s[k]) {
            k += 1;
        }
        if k > j + 1 {
            let text = std::str::from_utf8(&s[i..k]).ok()?;
            return text.parse::<f64>().ok().map(|v| (k, v));
        }
    }
    if j > i {
        let text = std::str::from_utf8(&s[i..j]).ok()?;
        return text.parse::<f64>().ok().map(|v| (j, v));
    }
    // ".5" 形式
    if i < n && s[i] == b'.' {
        let mut k = i + 1;
        while k < n && is_digit(s[k]) {
            k += 1;
        }
        if k > i + 1 {
            let text = std::str::from_utf8(&s[i..k]).ok()?;
            return text.parse::<f64>().ok().map(|v| (k, v));
        }
    }
    None
}

/// 切词。原程序的顺序：整串小写 -> `pi`/`π` 换成 `n` -> 正则切词（未匹配字符被跳过）。
pub fn lex(text: &str) -> Vec<Token> {
    let lowered = text.to_lowercase().replace("pi", "n").replace('\u{03c0}', "n");
    let s = lowered.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < s.len() {
        if let Some((end, v)) = match_number(s, i) {
            out.push(Token::constant(v));
            i = end;
            continue;
        }
        let rest = &s[i..];
        // 单字符与两字符记号，注意 "y'" 必须排在 "y" 之前
        let single: Option<Function> = match s[i] {
            b'(' => Some(LeftBracket),
            b')' => Some(RightBracket),
            b'x' => Some(VarX),
            b'+' => Some(Add),
            b'-' => Some(Sub),
            b'*' => Some(Mul),
            b'/' => Some(Divide),
            b'^' => Some(Pow),
            _ => None,
        };
        if let Some(f) = single {
            out.push(Token::op(f));
            i += 1;
            continue;
        }
        if rest.starts_with(b"y'") {
            out.push(Token::op(VarDy));
            i += 2;
            continue;
        }
        if rest.starts_with(b"y") {
            out.push(Token::op(VarY));
            i += 1;
            continue;
        }
        // 关键字：顺序与正则一致（长名在前）
        const KEYWORDS: &[(&[u8], Function)] = &[
            (b"sqrt", Sqrt),
            (b"log", Log),
            (b"abs", Abs),
            (b"sin", Sin),
            (b"cos", Cos),
            (b"tan", Tan),
            (b"ln", Ln),
            (b"n", Const),
            (b"exp", Exp),
            (b"e", Const),
        ];
        let mut matched = false;
        for (kw, f) in KEYWORDS.iter().copied() {
            if rest.starts_with(kw) {
                // n 是 π，e 是自然常数
                let v = if kw == b"n".as_slice() { PI } else { E };
                out.push(if f == Const { Token::constant(v) } else { Token::op(f) });
                i += kw.len();
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }
        // 未匹配字符被跳过（与 finditer 一致）
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// 2. 语法：递归下降 -> 树 -> 前缀序列
// ---------------------------------------------------------------------------

struct Node {
    op: Function,
    value: f64,
    kids: Vec<Option<Box<Node>>>,
}

impl Node {
    fn leaf(t: Token) -> Box<Node> {
        Box::new(Node { op: t.tag, value: t.value, kids: Vec::new() })
    }
    fn un(op: Function, a: Option<Box<Node>>) -> Option<Box<Node>> {
        Some(Box::new(Node { op, value: 0.0, kids: vec![a] }))
    }
    fn bin(op: Function, a: Option<Box<Node>>, b: Option<Box<Node>>) -> Option<Box<Node>> {
        Some(Box::new(Node { op, value: 0.0, kids: vec![a, b] }))
    }
    fn zero() -> Option<Box<Node>> {
        Some(Box::new(Node { op: Const, value: 0.0, kids: Vec::new() }))
    }
}

fn tag(t: &Token) -> Function {
    t.tag
}

fn starts_operand(t: &Token) -> bool {
    matches!(tag(t), VarX | VarY | VarDy | Const | LeftBracket | Sqrt | Log | Abs | Sin | Cos | Tan | Ln | Exp)
}

struct Parser {
    t: Vec<Token>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.t.get(self.i)
    }
    fn take(&mut self) -> Option<Token> {
        let v = self.t.get(self.i).copied();
        if v.is_some() {
            self.i += 1;
        }
        v
    }

    fn expr(&mut self) -> Option<Box<Node>> {
        let mut left = self.term();
        loop {
            let op = match self.peek() {
                Some(t) if matches!(tag(t), Add | Sub) => tag(t),
                _ => break,
            };
            self.take();
            let right = self.term();
            if op == Add {
                // '+' 两侧缺都补 0
                let l = left.take().unwrap_or_else(|| Node::zero().unwrap());
                let r = right.unwrap_or_else(|| Node::zero().unwrap());
                left = Node::bin(Add, Some(l), Some(r));
            } else if left.is_none() {
                // 没有左操作数的 '-' 变成一元负号
                left = Node::un(UnaryMinus, right);
            } else {
                let r = right.unwrap_or_else(|| Node::zero().unwrap());
                left = Node::bin(Sub, left, Some(r));
            }
        }
        left
    }

    fn term(&mut self) -> Option<Box<Node>> {
        let mut left = self.unary();
        loop {
            let t = match self.peek() {
                Some(t) => t,
                None => break,
            };
            if matches!(tag(t), Mul | Divide) {
                let op = tag(t);
                self.take();
                left = Node::bin(op, left, self.unary());
            } else if starts_operand(t) {
                // 隐式乘法
                left = Node::bin(Mul, left, self.unary());
            } else {
                break;
            }
        }
        left
    }

    fn unary(&mut self) -> Option<Box<Node>> {
        if let Some(t) = self.peek() {
            if matches!(tag(t), Sub | UnaryMinus) {
                self.take();
                return Node::un(UnaryMinus, self.unary());
            }
        }
        self.power()
    }

    /// 幂：**左结合**（`2^3^2 = 64`）。
    fn power(&mut self) -> Option<Box<Node>> {
        let mut left = self.primary();
        loop {
            match self.peek() {
                Some(t) if tag(t) == Pow => {}
                _ => break,
            }
            self.take();
            left = Node::bin(Pow, left, self.prefixed());
        }
        left
    }

    fn prefixed(&mut self) -> Option<Box<Node>> {
        if let Some(t) = self.peek() {
            if matches!(tag(t), Sub | UnaryMinus) {
                self.take();
                return Node::un(UnaryMinus, self.prefixed());
            }
        }
        self.primary()
    }

    fn primary(&mut self) -> Option<Box<Node>> {
        let t = match self.peek() {
            Some(t) => *t,
            None => return None,
        };
        match tag(&t) {
            VarX | VarY | VarDy | Const => {
                self.take();
                Some(Node::leaf(t))
            }
            LeftBracket => {
                self.take();
                let node = self.expr();
                if let Some(p) = self.peek() {
                    if tag(p) == RightBracket {
                        self.take();
                    }
                }
                node
            }
            Sqrt | Log | Abs | Sin | Cos | Tan | Ln | Exp => {
                self.take();
                // 函数只作用于紧跟的一个 primary（所以 sin^2 的 sin 没有操作数）
                Node::un(tag(&t), self.primary())
            }
            _ => None,
        }
    }
}

fn flatten(n: &Node, out: &mut Vec<Token>) {
    if n.op == Const {
        out.push(Token::constant(n.value));
        return;
    }
    out.push(Token::op(n.op));
    for k in &n.kids {
        if let Some(k) = k {
            flatten(k, out);
        }
    }
}

/// 文本 -> 前缀 token 列表。
pub fn parse(text: &str) -> Vec<Token> {
    let toks = lex(text);
    if toks.is_empty() {
        return Vec::new();
    }
    let mut p = Parser { t: toks, i: 0 };
    let root = p.expr();
    let mut out = Vec::new();
    if let Some(root) = root {
        flatten(&root, &mut out);
    }
    out
}

// ---------------------------------------------------------------------------
// 3. 求值
// ---------------------------------------------------------------------------

/// `sqrt`：负数得 NaN（不是报错）。
fn m_sqrt(x: f64) -> f64 {
    if x >= 0.0 { x.sqrt() } else { f64::NAN }
}

/// 弦长：游戏用的是 `sqrt(a*a + b*b)`，不是真正的 `hypot`。
pub(crate) fn m_hypot(a: f64, b: f64) -> f64 {
    m_sqrt(a * a + b * b)
}

/// 除法：除零走符号规则而不是 panic。
fn m_div(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        if a == 0.0 || a.is_nan() {
            return f64::NAN;
        }
        // 量值取 inf、符号跟 a；再乘上 b 的符号（对应 Python 的
        // math.copysign(inf, a) * math.copysign(1.0, b)）
        return f64::INFINITY.copysign(a) * 1.0f64.copysign(b);
    }
    a / b
}

/// 幂：`powf` 的 NaN/inf 行为与原实现一致（负底 + 分数指数 -> NaN）。
fn m_pow(a: f64, b: f64) -> f64 {
    a.powf(b)
}

/// `log10`：**0 走 NaN**，不是 -inf。
///
/// 实测真二进制：`log(0) = NaN`、`log(-0.0) = NaN`、`log(1e-320) = -320.000004…`，
/// 所以判据是 `v <= 0` 而非 `v < 0`。上游 Python 参照实现过去在这里给 `-inf`，
/// 属于已知偏差；本 crate 一律以二进制为准。
pub(crate) fn m_log10(v: f64) -> f64 {
    if v <= 0.0 { f64::NAN } else { v.log10() }
}

/// 自然对数：同 [`m_log10`]，0 走 NaN。
pub(crate) fn m_ln(v: f64) -> f64 {
    if v <= 0.0 { f64::NAN } else { v.ln() }
}

/// 前缀序列求值。`dy` 在游戏里永远传 0。
pub fn eval(tokens: &[Token], x: f64, y: f64, dy: f64) -> f64 {
    let mut i = 0usize;
    ev(tokens, &mut i, x, y, dy)
}

fn ev(tokens: &[Token], i: &mut usize, x: f64, y: f64, dy: f64) -> f64 {
    if *i >= tokens.len() {
        return 0.0; // 序列不足时原程序返回 0
    }
    let t = tokens[*i];
    *i += 1;
    match t.tag {
        VarX => return x,
        VarY => return y,
        VarDy => return dy,
        Const => return t.value,
        UnaryMinus => return -ev(tokens, i, x, y, dy),
        Sqrt => return m_sqrt(ev(tokens, i, x, y, dy)),
        Log => return m_log10(ev(tokens, i, x, y, dy)), // log10，且 0 -> NaN
        Ln => return m_ln(ev(tokens, i, x, y, dy)),
        Exp => return ev(tokens, i, x, y, dy).exp(),
        Abs => return ev(tokens, i, x, y, dy).abs(),
        Sin => return ev(tokens, i, x, y, dy).sin(),
        Cos => return ev(tokens, i, x, y, dy).cos(),
        Tan => return ev(tokens, i, x, y, dy).tan(),
        _ => {}
    }
    let a = ev(tokens, i, x, y, dy);
    let b = ev(tokens, i, x, y, dy);
    match t.tag {
        Add => a + b,
        Sub => a - b,
        Mul => a * b,
        Divide => m_div(a, b),
        Pow => m_pow(a, b),
        LeftBracket | RightBracket => a, // 括号不应到达这里
        _ => 0.0,
    }
}

pub(crate) use m_hypot as hypot;
