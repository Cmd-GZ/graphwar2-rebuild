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

mod to_text;
pub use to_text::{const_text, is_printable, to_text, try_to_text, InvalidToken, CONST_DECIMALS};

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

/// 切词。原程序的顺序：整串**只把 ASCII 大写**转小写 -> `pi`/`π` 换成 `n` -> 正则切词（未匹配字符被跳过）。
///
/// ⚠️ 2026-11-08 更正（`dispatch/52` 差分矩阵，**逐位**）：小写化是**逐字节的 ASCII 折叠**，
/// 不是 Rust 的 `str::to_lowercase()`（后者是**全 Unicode** 的）。二进制入口那三条指令
/// （`0x4ed7a3`–`0x4ed7ba`：`c - 'A' < 0x1a ? c + 0x20 : c`）只会动 `A..=Z`。
///
/// 差别只在一个可观测点上：`Π`（U+03A0）。Unicode 折叠会把它变成 `π`，再被
/// `replace("π","n")` 变成 `n`（= CONST π）；真解析器**不动**它，于是它是个未匹配字符、
/// 被跳过 ⇒ 空 token 串。矩阵实测：`Π -> []`、`Πx -> [VAR_X]`、`πΠ -> [CONST π]`。
pub fn lex(text: &str) -> Vec<Token> {
    let mut lowered = String::with_capacity(text.len());
    for c in text.chars() {
        lowered.push(if c.is_ascii_uppercase() { c.to_ascii_lowercase() } else { c });
    }
    let lowered = lowered.replace("pi", "n").replace('\u{03c0}', "n");
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

/// 只吃一个 primary 的一元函数（`(`/`)`、变量、常量不算）。
fn is_unary_fn(f: Function) -> bool {
    matches!(f, Sqrt | Log | Abs | Sin | Cos | Tan | Ln | Exp)
}

struct Parser {
    t: Vec<Token>,
    i: usize,
    /// 当前所处的括号层数（`(` 进、`)` 出）。
    depth: usize,
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

    /// 丢掉**没有配对的** `)`（深度 0 时），然后**继续解析**。
    ///
    /// 真解析器（`0x4ed670`）就是这么干的 —— unicorn 直调实测
    /// （`docs/archive/155-round-log.md` §32.7；规则现状见 `docs/03` §10；
    /// 13 条观测钉在 `tests/parse_stray_paren.rs`）：
    ///
    /// ```text
    /// x)      -> [x]                  （多余 `)` 被吃掉）
    /// x)+1    -> [ADD, VAR_X, CONST]  （**不是**截断，继续解析）
    /// 2+(x))  -> [ADD, CONST, VAR_X]
    /// (x)+1)  -> [ADD, VAR_X, CONST]
    /// ```
    ///
    /// ⚠️ **只在 `expr()` 的循环里调用**，`term()` 里不许调 —— 无主 `)` 结束的是
    /// **整条子表达式**，不是最近的一个乘子。在 `term()` 里跳过会让后面的 `^`
    /// 整个丢失、让隐式乘法只乘到最右边的操作数上（`x+2)3` 会被解析成
    /// `ADD(x, MUL(2,3))`，而真二进制给 `MUL(ADD(x,2),3)`）。见 [`Parser::expr`]。
    ///
    /// 旧实现（更早的一版）遇到无主 `)` 就收工，于是
    /// `14/(1+exp(100*(x-19)^2)))+24/…` 只剩第一项（1290 发里 8 发真语料是这种
    /// "多一个右括号"的文本）。**对括号平衡的文本这是恒等操作**（一次都不触发）。
    fn skip_stray_close(&mut self) {
        while self.depth == 0 && matches!(self.peek().map(tag), Some(RightBracket)) {
            self.take();
        }
    }

    /// 加减层。**这一层同时是"无主 `)` 之后的重启点"**。
    ///
    /// 括号配对时它只处理 `+`/`-`（`term()` 会把 `*` `/` `^` 和隐式乘法都吃掉）。
    /// 一旦 [`Parser::skip_stray_close`] 丢掉了深度 0 上的 `)`，跳过的就是**整条
    /// 子表达式**的右边界，所以后面的一切都以 `left`（左侧整体）为左操作数重新开始：
    ///
    /// ```text
    /// x)^2      -> [POW, VAR_X, CONST 2]              （`^` 作用在整条式子上）
    /// x+2)3     -> [MUL, ADD, VAR_X, CONST 2, CONST 3]（隐式乘法乘在 `x+2` 上）
    /// x+)2*3    -> [MUL, MUL, ADD, VAR_X, 0, 2, 3]
    /// x)+1)^2   -> [POW, ADD, VAR_X, 1, 2]
    /// x)/2/3    -> [DIVIDE, DIVIDE, VAR_X, 2, 3]
    /// ```
    ///
    /// `*` `/`、`^`、隐式乘法这三支**只有刚跳过 `)` 才可能走到**：正常扫描时
    /// `term()` 不会在这些 token 上停下（`^` 也已由 `power()` 吃掉）。
    fn expr(&mut self) -> Option<Box<Node>> {
        let mut left = self.term();
        loop {
            self.skip_stray_close();
            let t = match self.peek() {
                Some(t) => *t,
                None => break,
            };
            match tag(&t) {
                Add | Sub => {
                    let op = tag(&t);
                    self.take();
                    let right = self.term();
                    // `+` / `-` 两侧缺都补 CONST 0（**包括左缺**）。
                    // 2026-11-08 更正（`dispatch/52` 差分矩阵，**逐位**）：左操作数缺失时
                    // 真解析器**不**把 `-` 变成一元负号，而是补 0 做二元减法：
                    // `)-1e3 -> SUB(CONST 0, MUL(MUL(1,e),3))`（我们旧版给 `NEG(...)`）。
                    // 一元负号只来自 `-` 出现在**操作数位置**时（[`Parser::unary`] /
                    // [`Parser::prefixed`]，含它们各自截走的"函数链 + `-`"那一支）。
                    let l = left.take().unwrap_or_else(|| Node::zero().unwrap());
                    let r = right.unwrap_or_else(|| Node::zero().unwrap());
                    left = Node::bin(op, Some(l), Some(r));
                }
                Mul | Divide => {
                    let op = tag(&t);
                    self.take();
                    left = Node::bin(op, left, self.unary());
                }
                Pow => {
                    self.take();
                    left = Node::bin(Pow, left, self.prefixed());
                }
                _ if starts_operand(&t) => {
                    // 隐式乘法：乘在**整条式子**上
                    left = Node::bin(Mul, left, self.unary());
                }
                _ => break,
            }
        }
        left
    }

    /// 乘除层。**这里不处理无主 `)`** —— 它属于上一层（见 [`Parser::expr`]）。
    fn term(&mut self) -> Option<Box<Node>> {
        let mut left = self.unary();
        loop {
            let t = match self.peek() {
                Some(t) => *t,
                None => break,
            };
            if matches!(tag(&t), Mul | Divide) {
                let op = tag(&t);
                self.take();
                left = Node::bin(op, left, self.unary());
            } else if starts_operand(&t) {
                // 隐式乘法
                left = Node::bin(Mul, left, self.unary());
            } else {
                break;
            }
        }
        left
    }

    /// 若当前位置是"**函数链 + `-`**"，则把两者都吃掉并返回 `true`；否则**一个 token 都不动**。
    ///
    /// 这是真解析器那条怪规则的落地（2026-11-08 差分矩阵，**逐位**，`docs/03` §14）：
    /// 函数名后面紧跟 `-` 时，**整条函数链被丢掉**，那个 `-` 当一元负号用。
    /// 它只在"操作数起点"上触发 —— [`Parser::unary`]（`-` 的负号层）与
    /// [`Parser::prefixed`]（`^` 的右操作数）各查一次，因为**两层吃操作数的深度不同**：
    ///
    /// ```text
    /// sin-x^2       -> NEG(POW(x,2))          走 unary：负号比 `^` 松
    /// 2^sin-x^2     -> POW(POW(2,NEG(x)),2)   走 prefixed：负号只吃 primary，`^2` 留在外层
    /// ```
    fn take_func_chain_before_minus(&mut self) -> bool {
        let save = self.i;
        let mut n = 0usize;
        while let Some(f) = self.peek().map(tag) {
            if !is_unary_fn(f) {
                break;
            }
            self.take();
            n += 1;
        }
        if n > 0 && matches!(self.peek().map(tag), Some(Sub | UnaryMinus)) {
            self.take();
            return true;
        }
        self.i = save;
        false
    }

    fn unary(&mut self) -> Option<Box<Node>> {
        if let Some(t) = self.peek() {
            if matches!(tag(t), Sub | UnaryMinus) {
                self.take();
                return Node::un(UnaryMinus, self.unary());
            }
        }
        if self.take_func_chain_before_minus() {
            return Node::un(UnaryMinus, self.unary());
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
        if self.take_func_chain_before_minus() {
            return Node::un(UnaryMinus, self.prefixed());
        }
        self.primary()
    }

    /// 原子：变量 / 常量 / 括号组。**函数不在这里**（[`Parser::primary`] 先收函数链）。
    fn atom(&mut self) -> Option<Box<Node>> {
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
                self.depth += 1;
                let node = self.expr();
                self.depth -= 1;
                if let Some(p) = self.peek() {
                    if tag(p) == RightBracket {
                        self.take();
                    }
                }
                node
            }
            _ => None,
        }
    }

    fn primary(&mut self) -> Option<Box<Node>> {
        // —— 函数链：真解析器先把**连续的**函数名全部吃掉，再拿一个原子包起来 ——
        //
        // "链尾紧跟 `-`" 那一支**不在这里**：它由操作数的两个入口
        // （[`Parser::unary`] / [`Parser::prefixed`]）用
        // [`Parser::take_func_chain_before_minus`] 提前截走，因为两层的深度不同。
        //
        // 其余怪癖（`docs/03` §14，真二进制逐位）：
        //
        // ```text
        // sin+2 / sin*2  -> [ADD, SIN, CONST 2] / [MUL, SIN, CONST 2]（`+` `*` 不触发丢弃）
        // sin^2          -> [POW, SIN, CONST 2]                        （`sin` 没有操作数、照样留着）
        // sin)  / sin    -> [SIN]
        // ```
        let mut funcs: Vec<Function> = Vec::new();
        loop {
            match self.peek().map(tag) {
                Some(f) if is_unary_fn(f) => {
                    funcs.push(f);
                    self.take();
                }
                _ => break,
            }
        }
        let mut node = self.atom();
        for f in funcs.into_iter().rev() {
            // 函数只作用于紧跟的一个原子（所以 `sin^2` 的 `sin` 没有操作数、照样留着）
            node = Node::un(f, node);
        }
        node
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
    let mut p = Parser { t: toks, i: 0, depth: 0 };
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

/// `sqrt`：**就是 `sqrtsd`**（二进制 `0x4ebb26`：`xorps xmm0,xmm0` + `sqrtsd`）。
///
/// 不要特判负数：`sqrtsd` 对负数给的是**负 NaN**（`fff8000000000000`），
/// 而 `f64::NAN` 是正 NaN（`7ff8000000000000`）。逐位对拍 `docs/06` §30：
/// `sqrt(-1) → fff8000000000000`。
fn m_sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// 弦长：游戏用的是 `sqrt(a*a + b*b)`，不是真正的 `hypot`。
pub(crate) fn m_hypot(a: f64, b: f64) -> f64 {
    m_sqrt(a * a + b * b)
}

/// 除法：**就是 `divsd`**（二进制 `0x4ebb4e`：`divsd xmm5,xmm3`）。
///
/// 过去的特判（`b == 0` 时自己拼 `±inf` / `NaN`）是照搬上游 Python 参照实现
/// （Python 除零要抛异常，必须自己拼）；IEEE 的 `divsd` 本来就给
/// `1/0 = +inf`、`1/-0 = -inf`、`0/0 = -NaN`，逐位比特判更准
/// （特判把 `0/0` 拼成**正** NaN，而 `divsd` 给 `fff8000000000000`）。
fn m_div(a: f64, b: f64) -> f64 {
    a / b
}

/// 幂：`powf` 的 NaN/inf 行为与原实现一致（负底 + 分数指数 -> NaN）。
fn m_pow(a: f64, b: f64) -> f64 {
    a.powf(b)
}

/// `log10`：**就是 libm 的 `log10`**（二进制 tag 10 → GOT `0x9bc270` = `log10`）。
///
/// ⚠️ 2026-11 更正（`docs/06` §30，unicorn 逐位）：`log(0) = -inf`、`log(-0.0) = -inf`、
/// `log(-1) = NaN`。旧版本这里写的是「`v <= 0` → NaN」，来源是
/// `analysis/emu.py` 把导入映射到 **python 的 `math`**——python 对 `log10(0)`
/// 是**抛异常**、被 `_safe` 兜成 NaN；真 libm 是**返回 `-inf`**。
/// 也就是说那一条是 **oracle 的假象**，不是二进制的行为。
pub(crate) fn m_log10(v: f64) -> f64 {
    v.log10()
}

/// 自然对数：同 [`m_log10`]（tag 11 → GOT `0x9bbec0` = libm `log`）。
pub(crate) fn m_ln(v: f64) -> f64 {
    v.ln()
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

// ---------------------------------------------------------------------------
// T108（docs/06 §156）：`math_function::FunctionRandomizer` —— **分母外**源码函数
// ---------------------------------------------------------------------------
//
// 归属/地址（预注册 fixtures/denom_drawers_math_plan.txt §1；linux 地址见 sym_match.tsv +
// 本轮 `addr_probe fde` 现取的 FDE 范围）：
//
// | 原版函数 | Android 2.0.60 arm64 `.symtab` | Linux x86-64 `.eh_frame` |
// |---|---|---|
// | `FunctionRandomizer::adjust_tokens` | `0x447390`（620 B） | `0x4ee0e0`..`0x4ee393`（0x2b3） |
// | `FunctionRandomizer::mutate_function` | `0x4475fc`（704 B） | `0x4ee3a0`..`0x4ee712`（0x372） |
// | `FunctionRandomizer::create_random_region` | `0x448160`（784 B） | `0x4ef020`..`0x4ef390`（0x370） |
//
// 三条体内 `panic::Location` = 0（T105 观测）⇒ 结构上进不了 527 分母。
// `docs/02` §17 记的"token 级随机化属于 math_function"由此三条 arm64 真名钉死（此前只有
// `mutate_fine_tune` 一条有 Location 锚 `math_function.rs:452`，那一条已在 527 分母里）。
//
// **随机源不可复现**（这是它们只能做到「观测」档的根本原因，不是偷懒）：
// 三条全走 `rand` crate 的线程 RNG —— 槽 `0x9bb290` → `0x750180`（`thread_rng`）、
// `0x9bbe90` → `0x750700`、`0x9bbe98` → `0x751610`、`0x9bbea0` → `0x750270`、
// `0x9bb298` → `0x750710`。所以**值级对拍做不到**（同输入两次运行输出不同）；
// 能做的是把**控制流 + 常量表**复原成可测试的形状：随机数由调用方注入。

/// `adjust_tokens` 的**每-tag 权重表**（`int32 × 20`，`.rodata` `0x1c47d0`，**逐位**）。
///
/// 读法：`0x4ee120 mov r9,[rax+r8]`（token 的 tag）→ `0x4ee124 add r14d,[r10+r9*4]`
/// （`r10` = `0x1c47d0`，步长 4）。表值（本轮实读 80 字节）：
///
/// ```text
/// tag:    0   1   2   3   4   5   6   7   8   9  10  11  12  13  14  15  16  17  18  19
/// arity: -1  -1  -1  +1  +1   0  +1  +1  +1   0   0   0   0   0   0   0   0  -1  -1  -1
/// ```
///
/// ⇒ `Σ = #二元算子 − #操作数 − #括号`，`缺失操作数 + 1 = 1 + Σ`。
/// 复算：读 `.reference/steam/2026-08-28/libgraphwarrust.so` 的 `0x1c47d0` 80 字节
/// （`ff ff ff ff` ×3 后接 17 个 int32）。
pub const TOKEN_ARITY: [i32; 20] =
    [-1, -1, -1, 1, 1, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, -1, -1, -1];

/// `adjust_tokens` 里常量 token 的取值系数：`.rodata` 槽 `0x19f758` 的 8 字节
/// `66 66 66 66 66 66 24 40` = `0x4024666666666666` = **`10.2`**（`mulsd` @ `0x4ee26e`）。**逐位**
pub const ADJUST_CONST_RANGE: f64 = 10.2;

/// `FunctionRandomizer`（原版 `math_function` 模块里的随机化器）。
///
/// 它是 AI 进化（`GraphwarFunctionEvolutionator`）那一侧的工具：把函数 token 流随机补全 /
/// 随机改一段 / 随机生成一段。**玩家侧不会用到**（`docs/13`）。
pub struct FunctionRandomizer;

impl FunctionRandomizer {
    /// `FunctionRandomizer::adjust_tokens`（Linux **`0x4ee0e0`** / arm64 `0x447390`，620 B）
    ///
    /// 把「缺操作数」的前缀 token 流**随机补全**：补 `1 + Σ TOKEN_ARITY[tag]` 个 token。
    ///
    /// 指令锚（x86-64 Linux，**逐条**）：
    ///  * `0x4ee120 mov r9,[rax+r8]` + `0x4ee124 add r14d,[r10+r9*4]`（`r10 = 0x1c47d0`）
    ///    ⇒ 权重表 [`TOKEN_ARITY`]；`0x4ee13a test r14d` / `0x4ee13d jg` ⇒ `Σ <= 0` 时**一个都不补**
    ///  * `0x4ee240..0x4ee251`：`imul 0x51eb851f / shr 0x24 / imul 0x32 / sub` = **`rand % 50`**
    ///  * `0x4ee254 cmp r12d,0x19`（25）+ `0x4ee258 jae` ⇒ **`r%50 < 25` 走常量分支**
    ///  * `0x4ee26e mulsd xmm0,[0x19f758]` ⇒ 常量 = `rand_f64 × 10.2`（[`ADJUST_CONST_RANGE`]）；
    ///    `0x4ee2a3 mov r12d,0x11` ⇒ tag **17 = `Const`**
    ///  * `0x4ee2c0 cmp r12d,0x2a`（42）+ `0x4ee2ce cmovb r12,0` ⇒ `r%50 < 42` 推 **`VarX`(0)**，
    ///    否则推 **`flag`**（`rsi & 1`）⇒ `flag = true` 时那一档是 **`VarY`(1)**，否则仍是 `VarX`
    ///  * `0x4ee180..0x4ee1a2` 推入 `Vec<Token>`；`0x4ee303..0x4ee30e` 把 Vec 头（cap/ptr/len）
    ///    **按值返回**（`sret` 在 `rdi`）
    ///
    /// ⚠️ **未解（K3，不编）**：
    ///  1. `0x4ee2f7..0x4ee2ff` 的 `inc r11 / cmp r11,r13 / ja / mov [rbx+0x10],r11` 是
    ///     `Vec::truncate(new_len)` 形状，但 `new_len`（`r11` 的初值）与 `0x4ee320` 那条分支
    ///     **没解出来** ⇒ 本实现**不做**截断，只做"补全"。
    ///  2. 随机源 = `rand` crate 的 `thread_rng`（不可复现）⇒ 随机数**注入**进来，便于单测；
    ///     **不**声称与真机同一随机序列（也不可能）。
    pub fn adjust_tokens<F, G>(
        tokens: &mut Vec<Token>,
        flag: bool,
        mut rand_u64: F,
        mut rand_f64: G,
    ) -> Vec<Token>
    where
        F: FnMut() -> u64,
        G: FnMut() -> f64,
    {
        // `1 + Σ arity`（`0x4ee124`）；`<= 0` 时一个都不补（`0x4ee13d jg`）
        let mut missing: i32 = 1;
        for t in tokens.iter() {
            missing += TOKEN_ARITY[t.tag as usize];
        }
        if missing > 0 {
            for _ in 0..missing {
                // `rand % 50`（`0x4ee240..0x4ee251` 的乘法取模序列）
                let r = (rand_u64() % 50) as u32;
                if r < 25 {
                    // 常量分支：`tag = 17`、`value = rand_f64 × 10.2`
                    tokens.push(Token::constant(rand_f64() * ADJUST_CONST_RANGE));
                } else if r < 42 {
                    // `cmovb` 那一臂 ⇒ tag = 0（VarX）
                    tokens.push(Token::op(Function::VarX));
                } else {
                    // 否则 tag = `flag`（0 = VarX / 1 = VarY）
                    tokens.push(Token::op(if flag { Function::VarY } else { Function::VarX }));
                }
            }
        }
        // 返回 Vec 的按值副本（`0x4ee303..0x4ee30e` 拷 {cap,ptr,len} 到 sret）
        tokens.clone()
    }
}

pub(crate) use m_hypot as hypot;

#[cfg(test)]
mod parser_tests {
    use super::*;

    fn tags(text: &str) -> Vec<u32> {
        parse(text).iter().map(|t| t.tag as u32).collect()
    }

    /// 括号**平衡**的文本：解析结果与"遇到 `)` 就收工"的老实现完全一样。
    #[test]
    fn balanced_texts_are_unaffected() {
        assert_eq!(tags("x"), vec![0]);
        assert_eq!(tags("(x+1)(x-1)"), vec![6, 3, 0, 17, 4, 0, 17]);
        assert_eq!(tags("sin(x)"), vec![14, 0]);
        assert_eq!(tags("(((x)))"), vec![0]);
        assert_eq!(tags("2+(x))"), vec![3, 17, 0]);
    }

    /// 无主的 `)`：**跳过、继续解析**（真解析器 `0x4ed670` 的实测值，
    /// `docs/archive/155-round-log.md` §32.7；13 条全集见 `tests/parse_stray_paren.rs`）。
    #[test]
    fn stray_close_paren_is_skipped() {
        assert_eq!(tags("x)"), vec![0]);
        assert_eq!(tags("x)+1"), vec![3, 0, 17]);
        assert_eq!(tags("(x)+1)"), vec![3, 0, 17]);
        assert_eq!(tags("x)+2"), vec![3, 0, 17]);
        assert_eq!(tags("sin(x))"), vec![14, 0]);
        // 跳过之后仍然**隐式乘法**（真解析器：`x)2 -> [MUL, VAR_X, CONST]`）
        assert_eq!(tags("x)2"), vec![6, 0, 17]);
        assert_eq!(tags("sin(x))2"), vec![6, 14, 0, 17]);
        assert_eq!(tags("x)*2"), vec![6, 0, 17]);
        assert_eq!(tags("x)+"), vec![3, 0, 17]);
        assert_eq!(tags("x))"), vec![0]);
        // 跳过之后 `^` 作用在**整条式子**上（是整条子表达式结束，不是最近一个乘子结束）
        assert_eq!(tags("x)^2"), vec![8, 0, 17]);
        assert_eq!(tags("x+2)3"), vec![6, 3, 0, 17, 17]);
        assert_eq!(tags(")"), Vec::<u32>::new());
        assert_eq!(tags("()"), Vec::<u32>::new());
        assert_eq!(tags("(x"), vec![0]);
    }

    /// 真语料里的那一例（`1782675382#0`）：老实现只解析出第一项（12 个 token），
    /// 真解析器给 27 个 token 的 `SUB(ADD(…), CONST)`。
    #[test]
    fn unbalanced_real_corpus_text_keeps_the_tail() {
        let text = "14/(1+exp(100*(x-19)^2)))+24/(1+exp(100*(x-24)^2)) - 4.323";
        let toks = parse(text);
        assert_eq!(toks.len(), 27, "括号不平衡时不该截断");
        assert_eq!(toks[0].tag, Sub, "根是 SUB");
        assert_eq!(toks[1].tag, Add, "左子是 ADD");
        assert_eq!(toks.last().unwrap().tag, Const);
        assert_eq!(toks.last().unwrap().value, 4.323);
    }

    /// **函数链后面紧跟 `-`：整条链被丢掉，`-` 当一元负号**（`docs/03` §14）。
    ///
    /// 每条都是真二进制 `0x4ed670` 的直调观测（`tools/parse_matrix.py`，同一批也钉在
    /// `fixtures/parse_matrix_truth.tsv`）。旧实现把函数当成"缺操作数的节点"留着，
    /// 于是 `sin-x` 给 `SUB(SIN, X)` —— 那是错的。
    #[test]
    fn func_chain_before_minus_is_dropped() {
        assert_eq!(tags("sin-x"), vec![5, 0]); // NEG, VAR_X
        assert_eq!(tags("sin-x^2"), vec![5, 8, 0, 17]); // NEG, POW, X, 2（负号比 ^ 松）
        assert_eq!(tags("logtan-.5"), vec![5, 17]); // 链上 log/tan 一起丢
        assert_eq!(tags("2sin-3"), vec![6, 17, 5, 17]); // MUL, 2, NEG, 3
        assert_eq!(tags("sin-sin"), vec![5, 14]); // 右边的 sin 没操作数、照样留
        assert_eq!(tags("-sin-x"), vec![5, 5, 0]);
        // `^` 的右操作数位置是 **prefixed** 层：负号只吃 primary，`^2` 留在外层
        assert_eq!(tags("2^sin-x^2"), vec![8, 8, 17, 5, 0, 17]);
        // `+` `*` `^` 不触发丢弃
        assert_eq!(tags("sin+2"), vec![3, 14, 17]);
        assert_eq!(tags("sin*-x"), vec![6, 14, 5, 0]);
        assert_eq!(tags("sin^2(x)"), vec![6, 8, 14, 17, 0]);
        assert_eq!(tags("sin"), vec![14]);
        assert_eq!(tags("sin+"), vec![3, 14, 17]);
    }

    /// **`+` / `-` 缺左操作数都补 `CONST 0`**；一元负号**只**在操作数位置产生。
    ///
    /// 旧实现在 `expr()` 里把"左缺的 `-`"翻成一元负号，于是 `)-1e3` 给 `NEG(...)`；
    /// 真二进制是 `SUB(CONST 0, ...)`（真值同 `fixtures/parse_matrix_truth.tsv`）。
    #[test]
    fn missing_left_operand_of_minus_is_zero() {
        assert_eq!(tags(")-1e3"), vec![4, 17, 6, 6, 17, 17, 17]);
        assert_eq!(tags(")-x"), vec![4, 17, 0]);
        assert_eq!(tags("x)+"), vec![3, 0, 17]); // `+` 那条不变
    }

    /// **小写化只折叠 ASCII 大写**（真解析器 `0x4ed7a3`–`0x4ed7ba`），
    /// 不是 Rust 的 `str::to_lowercase()`（全 Unicode）。
    ///
    /// 唯一可观测差别是 `Π`（U+03A0）：Unicode 折叠会把它变成 `π` → `n` → CONST π；
    /// 真解析器不动它 ⇒ 未匹配字符、被跳过。`π` 本身两条路一样（都是 CONST π）。
    #[test]
    fn lowercasing_is_ascii_only() {
        assert_eq!(tags("\u{03a0}"), Vec::<u32>::new()); // 空 token 串
        assert_eq!(tags("\u{03a0}x"), vec![0]);
        assert_eq!(tags("\u{03c0}\u{03a0}"), vec![17]);
        assert_eq!(tags("\u{03c0}"), vec![17]);
        assert_eq!(tags("PI"), vec![17]); // ASCII 折照旧
        assert_eq!(tags("SIN(X)"), vec![14, 0]);
    }
}

#[cfg(test)]
mod denom_randomizer_tests {
    //! T108：`FunctionRandomizer::adjust_tokens`（linux `0x4ee0e0` / arm64 `0x447390`）。
    //!
    //! 期望值来源 = **反汇编读出的常量/分支**，不是"我方输出"：
    //!  * 权重表 `.rodata 0x1c47d0`（20 × int32）⇒ `1 + Σ arity` 决定补几个
    //!  * `rand % 50` 的两个阈值：`0x4ee254 cmp 0x19`（25）、`0x4ee2c0 cmp 0x2a`（42）
    //!  * 常量系数 `.rodata 0x19f758` = `10.2`；常量 token 的 tag = `0x11`（17 = `Const`）
    //!  * 非终态 tag：`cmovb` 那一臂给 0（`VarX`），否则给 `flag`（0/1 = `VarX`/`VarY`）
    //!
    //! 随机源真机是 `rand` 的 `thread_rng`（不可复现）⇒ 这里**注入**确定性假 RNG。
    use super::*;

    fn arity_sum(tokens: &[Token]) -> i32 {
        tokens.iter().map(|t| TOKEN_ARITY[t.tag as usize]).sum()
    }

    #[test]
    fn arity_table_is_the_rodata_array() {
        // `.rodata 0x1c47d0` 80 字节 = 20 个 int32，本轮实读
        assert_eq!(TOKEN_ARITY.len(), 20);
        assert_eq!(&TOKEN_ARITY[..3], &[-1, -1, -1]); // VarX/VarY/VarDy
        assert_eq!(&TOKEN_ARITY[3..9], &[1, 1, 0, 1, 1, 1]); // Add..Pow（UnaryMinus=0）
        assert_eq!(&TOKEN_ARITY[17..], &[-1, -1, -1]); // Const / L / R 括号
        assert_eq!(ADJUST_CONST_RANGE.to_bits(), 0x4024_6666_6666_6666); // 10.2
        // 完整前缀表达式 Σ = -1 ⇒ 一个都不补
        for text in ["x", "x+1", "sin(x)", "(x*2)-1", "0.00"] {
            let toks = parse(text);
            assert_eq!(arity_sum(&toks), -1, "{text} 的 Σ 应是 -1");
        }
        // 缺操作数：单个二元算子 ⇒ Σ = +1 ⇒ 补 2 个
        assert_eq!(arity_sum(&[Token::op(Function::Add)]), 1);
    }

    #[test]
    fn adjust_tokens_fills_missing_operands_with_injected_rng() {
        let mut toks = vec![Token::op(Function::Add)]; // Σ = 1 ⇒ 补 1 + 1 = 2 个
        let mut seq = [24_u64, 30].into_iter(); // 24 < 25 ⇒ 常量分支；30 ∈ [25,42) ⇒ VarX
        let mut f = || seq.next().unwrap();
        let out = FunctionRandomizer::adjust_tokens(&mut toks, false, &mut f, || 2.5);
        assert_eq!(out.len(), 3, "补完应是 1 + 2 个 token");
        // 第 1 个补的是常量：`rand_f64 × 10.2`（`0x4ee26e mulsd`），tag = 17
        assert_eq!(out[1].tag, Function::Const);
        assert_eq!(out[1].value, 2.5 * ADJUST_CONST_RANGE);
        assert_eq!(out[1].value, 25.5);
        // 第 2 个补的是 VarX（`r%50 ∈ [25,42)` ⇒ `cmovb` 给 0）
        assert_eq!(out[2].tag, Function::VarX);
        // 返回值是 Vec 的**副本**（`0x4ee303..0x4ee30e` 拷头）
        assert_eq!(out.len(), toks.len());
    }

    #[test]
    fn adjust_tokens_third_branch_uses_the_flag() {
        // r%50 >= 42 ⇒ tag = flag：flag=false ⇒ VarX，flag=true ⇒ VarY
        let mk = |flag: bool| {
            let mut toks = vec![Token::op(Function::Add)];
            let mut seq = [49_u64, 49].into_iter();
            let mut f = || seq.next().unwrap();
            FunctionRandomizer::adjust_tokens(&mut toks, flag, &mut f, || 0.0)
        };
        assert_eq!(mk(false)[1].tag, Function::VarX);
        assert_eq!(mk(true)[1].tag, Function::VarY);
    }

    #[test]
    fn adjust_tokens_leaves_complete_expressions_alone() {
        let mut toks = parse("sin(x)+1");
        let before = toks.clone();
        let mut called = 0;
        let out = FunctionRandomizer::adjust_tokens(
            &mut toks,
            true,
            || {
                called += 1;
                0
            },
            || 0.0,
        );
        assert_eq!(out, before, "Σ = -1 时不该补任何 token");
        assert_eq!(called, 0, "`0x4ee13d jg`：Σ <= 0 时连随机数都不取");
    }
}
