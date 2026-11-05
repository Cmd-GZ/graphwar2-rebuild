//! token -> 文本（`0x4ec160`）的验收。
//!
//! 三个层次，证据强度从高到低：
//!
//! 1. **与真二进制逐字符一致**：`fixtures/to_text.tsv`（token -> 文本）与
//!    `fixtures/to_text_roundtrip.tsv`（文本 -> 文本，两代）都由
//!    `tools/gen_to_text_fixture.py` 直接调 `0x4ec160` / `0x4ed670` 生成。
//! 2. **往返**：`text -> parse -> to_text -> parse`，覆盖 `fixtures/parse.tsv` 全部表达式。
//!    等价性分两档——见 `docs/119-token-to-text.md` §5：
//!    * **算子结构（tag 序列）永远一致**；
//!    * **常量只在"能被两位小数精确表示"时逐位一致**，否则被截成两位小数
//!      （`π -> 3.14`、`e -> 2.72`）——**这是真二进制行为**，不是我们的近似。
//! 3. **语义**：受影响的只是常量的十进制位数，所以对"两位小数可精确表示"的流
//!    断言 `eval(to_text 前后)` 逐位相同。

use std::fs;
use std::path::PathBuf;

use gw2_rebuild::function::{const_text, eval, parse, to_text, try_to_text};
use gw2_rebuild::{Function, Token};

fn fixture(name: &str) -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("fixtures");
    p.push(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", p.display()))
}

fn hex_f64(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s, 16).unwrap_or_else(|e| panic!("bad hex {s}: {e}")))
}

/// `tag:hexbits;...` -> token 序列。`-` 表示空。
fn tokens(s: &str) -> Vec<Token> {
    if s == "-" || s.is_empty() {
        return Vec::new();
    }
    s.split(';')
        .map(|item| {
            let (tag, bits) = item.split_once(':').expect("token 形如 tag:hexbits");
            let tag = match tag.parse::<u64>().expect("tag 是数字") {
                0 => Function::VarX,
                1 => Function::VarY,
                2 => Function::VarDy,
                3 => Function::Add,
                4 => Function::Sub,
                5 => Function::UnaryMinus,
                6 => Function::Mul,
                7 => Function::Divide,
                8 => Function::Pow,
                9 => Function::Sqrt,
                10 => Function::Log,
                11 => Function::Ln,
                12 => Function::Exp,
                13 => Function::Abs,
                14 => Function::Sin,
                15 => Function::Cos,
                16 => Function::Tan,
                17 => Function::Const,
                18 => Function::LeftBracket,
                19 => Function::RightBracket,
                n => panic!("未知 tag {n}"),
            };
            Token { tag, value: hex_f64(bits) }
        })
        .collect()
}

/// `fixtures/to_text.tsv`：token 列 + 真二进制写出的文本列。
fn to_text_fixture() -> Vec<(Vec<Token>, String)> {
    fixture("to_text.tsv")
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (toks, txt) = l.split_once('\t').unwrap_or_else(|| panic!("夹具行缺 TAB: {l}"));
            (tokens(toks), txt.to_string())
        })
        .collect()
}

fn parse_fixture_exprs() -> Vec<String> {
    fixture("parse.tsv")
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.split('\t').next().unwrap().to_string())
        .collect()
}

fn tags(t: &[Token]) -> Vec<Function> {
    t.iter().map(|t| t.tag).collect()
}

fn arity(tag: Function) -> usize {
    match tag {
        Function::VarX
        | Function::VarY
        | Function::VarDy
        | Function::Const => 0,
        Function::UnaryMinus
        | Function::Sqrt
        | Function::Log
        | Function::Ln
        | Function::Exp
        | Function::Abs
        | Function::Sin
        | Function::Cos
        | Function::Tan => 1,
        _ => 2,
    }
}

fn walk_complete(toks: &[Token], pos: &mut usize) -> bool {
    let tag = match toks.get(*pos) {
        Some(t) => t.tag,
        None => return false,
    };
    *pos += 1;
    let n = arity(tag);
    (0..n).all(|_| walk_complete(toks, pos))
}

/// 操作数齐全（不只消费一棵子树，还没有多余 token）。
fn complete(toks: &[Token]) -> bool {
    let mut p = 0usize;
    walk_complete(toks, &mut p) && p == toks.len()
}

/// 这个常量能否被 `{:.2}` 精确表示（负值走 `neg`，也算）。
fn const_roundtrips(v: f64) -> bool {
    eval(&parse(&const_text(v)), 0.0, 0.0, 0.0).to_bits() == v.to_bits()
}

/// `^` 的**左底座文本以 `-` 开头**时（左子式是 `neg`，或直接是负常量），
/// `(-a)^b` 会被写成 `-a^b`，重新解析成 `neg(a^b)`，数值不同（`docs/119` §5.3）。
/// 这类流不参与"语义不变"的断言。
fn pow_with_negated_base(toks: &[Token], pos: &mut usize) -> bool {
    let tag = match toks.get(*pos) {
        Some(t) => t.tag,
        None => return false,
    };
    if tag == Function::Pow {
        if let Some(first) = toks.get(*pos + 1) {
            let negated = first.tag == Function::UnaryMinus
                || (first.tag == Function::Const && first.value.is_sign_negative());
            if negated {
                return true;
            }
        }
    }
    *pos += 1;
    let n = arity(tag);
    (0..n).any(|_| pow_with_negated_base(toks, pos))
}

fn has_negated_pow_base(toks: &[Token]) -> bool {
    let mut p = 0usize;
    pow_with_negated_base(toks, &mut p)
}

fn same_number(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

// ---------------------------------------------------------------------------
// 1. 与真二进制逐字符一致
// ---------------------------------------------------------------------------

#[test]
fn matches_the_real_binary_on_every_fixture_line() {
    let cases = to_text_fixture();
    assert!(cases.len() >= 250, "夹具太小：{} 行", cases.len());
    let mut bad = Vec::new();
    for (toks, want) in &cases {
        let got = to_text(toks);
        if &got != want {
            bad.push(format!("tokens={toks:?} want={want:?} got={got:?}"));
        }
    }
    assert!(bad.is_empty(), "与真二进制不一致 {} 例：\n{}", bad.len(), bad.join("\n"));
}

/// 文本级两代往返也逐字符一致：喂真二进制自己的输出，我们复算它下一代。
#[test]
fn text_roundtrip_matches_the_real_binary() {
    let mut bad = Vec::new();
    let mut stable = 0usize;
    let mut total = 0usize;
    for line in fixture("to_text_roundtrip.tsv").lines() {
        if line.is_empty() {
            continue;
        }
        let (t1, t2) = line.split_once('\t').unwrap_or_else(|| panic!("夹具行缺 TAB: {line}"));
        let got = to_text(&parse(t1));
        if got != t2 {
            bad.push(format!("{t1:?} -> want {t2:?} got {got:?}"));
        }
        total += 1;
        if t1 == t2 {
            stable += 1;
        }
    }
    assert!(bad.is_empty(), "第二代文本不一致 {} 例：\n{}", bad.len(), bad.join("\n"));
    assert!(total >= 250, "往返夹具太小：{total} 行");
    // 绝大多数（随机树形状齐全 + 两位小数常量）是不动点
    assert!(
        stable * 10 >= total * 9,
        "不动点比例过低：{stable}/{total}"
    );
}

// ---------------------------------------------------------------------------
// 2. 往返：fixtures/parse.tsv 全部表达式
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_keeps_operator_structure_for_every_parse_fixture() {
    let exprs = parse_fixture_exprs();
    assert!(exprs.len() >= 36, "parse.tsv 只有 {} 条", exprs.len());

    let mut bad = Vec::new();
    for e in &exprs {
        let t0 = parse(e);
        let s1 = to_text(&t0);
        let t1 = parse(&s1);
        if tags(&t0) != tags(&t1) {
            bad.push(format!("{e:?}: 结构变了 {:?} -> {s1:?} -> {:?}", tags(&t0), tags(&t1)));
        }
    }
    assert!(bad.is_empty(), "往返结构不一致 {} 例：\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn roundtrip_constants_only_lose_decimal_digits() {
    let exprs = parse_fixture_exprs();
    let mut exact = 0usize;
    let mut snapped = Vec::new();
    for e in &exprs {
        let t0 = parse(e);
        let s1 = to_text(&t0);
        let t1 = parse(&s1);
        let c0: Vec<f64> = t0.iter().filter(|t| t.tag == Function::Const).map(|t| t.value).collect();
        let c1: Vec<f64> = t1.iter().filter(|t| t.tag == Function::Const).map(|t| t.value).collect();
        assert_eq!(c0.len(), c1.len(), "{e:?}: 常量个数变了");
        let mut ok = true;
        for (a, b) in c0.iter().zip(c1.iter()) {
            if same_number(*a, *b) {
                continue;
            }
            // 允许的唯一变形：被 `{:.2}` 截断后再解析回来
            let snapped_v = eval(&parse(&const_text(*a)), 0.0, 0.0, 0.0);
            assert!(
                same_number(*b, snapped_v),
                "{e:?}: 常量 {a} -> {b} 不是 \"{}\" 的结果（应为 {snapped_v}）",
                const_text(*a)
            );
            ok = false;
        }
        if ok {
            exact += 1;
        } else {
            snapped.push(e.clone());
        }
    }
    assert!(exact >= 30, "逐位往返的表达式只有 {exact} 条");
    for e in &snapped {
        let t0 = parse(e);
        assert!(
            t0.iter().any(|t| t.tag == Function::Const
                && (t.value == gw2_rebuild::PI || t.value == gw2_rebuild::E)),
            "{e:?} 截断了，但它的常量不是 π/e：{:?}",
            t0
        );
    }
}

// ---------------------------------------------------------------------------
// 3. 语义：两位小数可精确表示时常量不变 => 数值逐位相同
// ---------------------------------------------------------------------------

#[test]
fn semantics_survive_the_roundtrip() {
    let cases = to_text_fixture();
    let mut checked = 0usize;
    let mut bad = Vec::new();
    for (toks, _) in &cases {
        if !complete(toks)
            || has_negated_pow_base(toks)
            || !toks.iter().filter(|t| t.tag == Function::Const).all(|t| const_roundtrips(t.value))
        {
            continue;
        }
        let back = parse(&to_text(toks));
        for x in [-3.7, 0.0, 1.5, 9.25] {
            let a = eval(toks, x, 0.5, 0.0);
            let b = eval(&back, x, 0.5, 0.0);
            if !(a.is_nan() && b.is_nan()) && a.to_bits() != b.to_bits() {
                bad.push(format!("tokens={toks:?} x={x}: {a} != {b}"));
            }
        }
        checked += 1;
    }
    assert!(checked > 100, "只检查了 {checked} 例");
    assert!(bad.is_empty(), "往返后数值变了 {} 例：\n{}", bad.len(), bad.join("\n"));
}

// ---------------------------------------------------------------------------
// 4. 真实抓包里的 AI 产物是不动点（`docs/AI.md` §2.1）
// ---------------------------------------------------------------------------

#[test]
fn captured_ai_functions_are_fixed_points() {
    // 这六条是 harness 里抓到的 `FunctionUpdateRequest.function` 原文，
    // 也就是"AI 的 token 已经被 `0x4ec160` 转成文本"之后的样子。
    let captured = [
        "sin(x)",
        "0.00",
        "(-1.06*x)",
        "(-23.81*x)",
        "tan(sin((-0.12*x)))",
        "abs((-3.49+x))",
    ];
    for s in captured {
        let t = parse(s);
        assert_eq!(to_text(&t), s, "抓包文本 {s:?} 不是不动点");
        assert_eq!(to_text(&parse(&to_text(&t))), s);
    }
}

// ---------------------------------------------------------------------------
// 5. 畸形输入不 panic
// ---------------------------------------------------------------------------

#[test]
fn degenerate_streams_never_panic() {
    use Function::*;
    let op = Token::op;
    // 空 / 缺操作数 / 多余 token
    assert_eq!(to_text(&[]), "");
    assert_eq!(to_text(&[op(Sin)]), "sin()");
    assert_eq!(to_text(&[op(Add), op(VarX)]), "(x+)");
    assert_eq!(to_text(&[op(VarX), op(VarY)]), "x");
    // 括号：二进制 panic（`Invalid token`），我们返回 None / 空串
    assert_eq!(try_to_text(&[op(LeftBracket)]), None);
    assert_eq!(try_to_text(&[op(VarX), op(RightBracket)]), Some("x".to_string()));
    assert_eq!(to_text(&[op(LeftBracket), op(VarX)]), "");
    // 极深：不该爆栈、不该 panic（AI 的长度上限是 200，这里给 2000 层留余量）
    let deep: Vec<Token> = std::iter::repeat(op(Sin)).take(2000).chain([op(VarX)]).collect();
    let s = to_text(&deep);
    assert_eq!(s.matches("sin(").count(), 2000);
    assert!(s.ends_with(')'));
}

