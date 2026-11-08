//! 无主 `)` 的解析：**跳过并继续**，且继续时 `^` / `*` / 隐式乘法作用在**左侧整体**上。
//!
//! 证据等级：**逐位 / 观测**。真值来自**真二进制** `0x4ed670`，不是我们自己的模型：
//! 用分析仓库的 `analysis/gwemu.py`（unicorn 映射 `libgraphwarrust.so`）直接调用
//! 解析器，把返回的 `Vec<Token>`（`{tag: u64, value: f64}` × 16 字节）读出来。
//! 上游 `analysis/test_parse_stray_paren.py` 的 `PINNED` 表是同一批观测；
//! 本文件另外加了 5 条同规则的新观测（标 `BINARY_ONLY`），它们由本轮重新实测得到。
//!
//! 为什么这条规则重要（`docs/03-function-language.md` §2.6 / 上游 `REPORT.md` §2）：
//! 真回放语料里 **8 发**的函数原文多一个右括号。解析错不会报错，只会让 token 流
//! 静默变化 —— 于是曲线变、对拍必然对不上，而失败现场看起来像"回放不自洽"。
//!
//! 三种做法必须区分开（前两种都是错的）：
//!
//! * **截断**（最老实现）：`primary()` 碰到 `)` 返回 None，`parse_function` 只调一次
//!   `expr()`，于是 `x)^2` 只剩 `x`、`14/(1+exp(…)))+24/(…)` 只剩第一项；
//! * **在 `term()` 里跳过**（本文件要修的版本）：括号是吃掉了、后面的内容也保住了，
//!   但 `^` 完全丢失（`x)^2 -> x`），而隐式乘法只乘到**最右边的操作数**上
//!   （`x+2)3 -> ADD(x, MUL(2,3))`，真二进制是 `MUL(ADD(x,2),3)`）；
//! * **在 `expr()` 里跳过 + 在该层补 `*` `/` `^` / 隐式乘法**（正确）：无主 `)`
//!   结束的是**整条子表达式**，因此它后面的一切都以"左侧整体"为左操作数重新开始。
//!
//! 未闭合的 `(` 是**另一条**规则，没被这条改动碰到：`(x -> x`、`sin(x -> SIN x`
//! （见 `unclosed_open_is_unchanged`）。

use gw2_rebuild::math_function::parse;
use gw2_rebuild::Function::*;
use gw2_rebuild::Token;

/// 一个 token 的"可比较"形式：算子只看 tag，`CONST` 才看值。
#[derive(Clone, Copy, PartialEq)]
struct T(u32, f64);

impl std::fmt::Debug for T {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == Const as u32 {
            write!(f, "{}:{}", self.0, self.1)
        } else {
            write!(f, "{}:-", self.0)
        }
    }
}

const fn op(f: gw2_rebuild::Function) -> T {
    T(f as u32, 0.0)
}

const fn c(v: f64) -> T {
    T(Const as u32, v)
}

fn got(text: &str) -> Vec<T> {
    parse(text)
        .iter()
        .map(|t: &Token| {
            if t.tag == Const {
                T(t.tag as u32, t.value)
            } else {
                T(t.tag as u32, 0.0)
            }
        })
        .collect()
}

/// 13 条 = 上游 `analysis/test_parse_stray_paren.py` 的 `PINNED` 表（真二进制实测）。
const PINNED: &[(&str, &[T])] = &[
    ("x)", &[op(VarX)]),
    ("x))", &[op(VarX)]),
    ("x)+1", &[op(Add), op(VarX), c(1.0)]),
    ("(x)+1)", &[op(Add), op(VarX), c(1.0)]),
    ("x)-1", &[op(Sub), op(VarX), c(1.0)]),
    ("x)2", &[op(Mul), op(VarX), c(2.0)]),
    ("x)*2", &[op(Mul), op(VarX), c(2.0)]),
    ("x)+", &[op(Add), op(VarX), c(0.0)]),
    // 跳过 `)` 之后 `^` 作用于**左侧整体**
    ("x)^2", &[op(Pow), op(VarX), c(2.0)]),
    // 跳过 `)` 之后 `*` 接在**整条式子**上（`+` 的右操作数已补 0）
    (
        "x+)2*3",
        &[op(Mul), op(Mul), op(Add), op(VarX), c(0.0), c(2.0), c(3.0)],
    ),
    ("sin(x))2", &[op(Mul), op(Sin), op(VarX), c(2.0)]),
    // 完全没有操作数：产出空 token 串，不 panic
    (")", &[]),
    ("()", &[]),
];

/// 本轮另外实测的 5 条（同一条规则的不同形态，真二进制 `0x4ed670`）。
const BINARY_ONLY: &[(&str, &[T])] = &[
    // 任务书点名的形态：隐式乘法作用在 `x+2` 整体上，不是只乘 `2`
    ("x+2)3", &[op(Mul), op(Add), op(VarX), c(2.0), c(3.0)]),
    // 跳过 `)` 之后是 `+`：右操作数仍按 term 优先级吃 `2*3`
    ("x)+2*3", &[op(Add), op(VarX), op(Mul), c(2.0), c(3.0)]),
    // `^` 左结合照旧，且可以连续接在整条式子后面
    ("x)^2^2", &[op(Pow), op(Pow), op(VarX), c(2.0), c(2.0)]),
    ("x)+1)^2", &[op(Pow), op(Add), op(VarX), c(1.0), c(2.0)]),
    // `/` 同理，仍然左结合
    (
        "x)/2/3",
        &[op(Divide), op(Divide), op(VarX), c(2.0), c(3.0)],
    ),
];

fn report(title: &str, cases: &[(&str, &[T])]) -> usize {
    println!("== {title} ==");
    let mut bad = 0;
    for (text, want) in cases {
        let g = got(text);
        let ok = g.len() == want.len() && g.iter().zip(want.iter()).all(|(a, b)| a == b);
        if !ok {
            bad += 1;
            println!(
                "  MISMATCH {:?}\n      binary says {:?}\n      we give     {:?}",
                text, want, g
            );
        } else {
            println!("  ok       {:<10?} {:?}", text, g);
        }
    }
    bad
}

#[test]
fn stray_close_paren_matches_the_binary() {
    let bad = report("13 条二进制实测（上游 PINNED）", PINNED)
        + report("5 条本轮实测（同一条规则）", BINARY_ONLY);
    assert_eq!(bad, 0, "{bad} 条与真二进制不一致（上面逐条列出了）");
}

/// 未闭合的 `(` 是**另一条**规则，本条改动不许把它带跑。
#[test]
fn unclosed_open_is_unchanged() {
    let cases: &[(&str, &[T])] = &[
        ("(x", &[op(VarX)]),
        ("(((x", &[op(VarX)]),
        ("sin(x", &[op(Sin), op(VarX)]),
        ("(x+1", &[op(Add), op(VarX), c(1.0)]),
        ("2*(x", &[op(Mul), c(2.0), op(VarX)]),
    ];
    assert_eq!(report("未闭合 `(`", cases), 0);
}

/// 真语料那一发（`1782675382#0`）：`)` 之后的一整个减项必须还在。
///
/// 截断版只剩第一项（12 个 token）；真二进制给 27 个 token 的 `SUB(ADD(…), CONST)`。
#[test]
fn real_corpus_shot_keeps_its_tail() {
    let text = "14/(1+exp(100*(x-19)^2)))+24/(1+exp(100*(x-24)^2)) - 4.323";
    let toks = parse(text);
    assert_eq!(toks.len(), 27, "括号不平衡时不该截断");
    assert_eq!(toks[0].tag, Sub, "根是 SUB");
    assert_eq!(toks[1].tag, Add, "左子是 ADD");
    assert_eq!(toks.last().unwrap().tag, Const);
    assert_eq!(toks.last().unwrap().value, 4.323);
}
