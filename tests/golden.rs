//! 金标准验收：Rust 复原实现 vs 真值夹具。
//!
//! 夹具由 `tools/gen_fixtures.py` 生成，真值来源：
//! - `parse.tsv`：已验证的 Python 参照实现（其本身对二进制做过 90 例对拍）
//! - `eval.tsv` / `path.tsv`：**真二进制**（unicorn 直接调用 `0x4eb9d0` / `0x3c2530`）
//!
//! 比较语义与上游对拍一致：数值用 `==`（所以 `-0.0 == 0.0` 视为相同），
//! NaN 只要求双方都是 NaN（不比较载荷位）。

use std::fs;
use std::path::PathBuf;

use gw2_rebuild::math_function::{eval, parse};
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
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

fn tokens(s: &str) -> Vec<Token> {
    if s == "-" {
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

/// NaN 互相视为相同，其余用 `==`（与上游对拍一致）。
fn same(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

#[test]
fn parse_matches_reference() {
    let text = fixture("parse.tsv");
    let mut n = 0;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let (src, ntok) = (f[0], f[1].parse::<usize>().unwrap());
        let expected = tokens(f[2]);

        let got = parse(src);
        assert_eq!(got.len(), ntok, "「{src}」token 个数不符: 期望 {ntok} 得到 {}", got.len());
        for (i, (g, e)) in got.iter().zip(expected.iter()).enumerate() {
            assert_eq!(g.tag, e.tag, "「{src}」第 {i} 个 token 的 tag 不符");
            assert!(
                same(g.value, e.value),
                "「{src}」第 {i} 个 token 的值不符: {} vs {}",
                g.value,
                e.value
            );
        }
        n += 1;
    }
    assert!(n > 30, "解析夹具太少: {n}");
}

#[test]
fn eval_matches_binary() {
    let text = fixture("eval.tsv");
    let mut n = 0;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let name = f[0];
        let toks = tokens(f[1]);
        let (x, y, dy) = (hex_f64(f[2]), hex_f64(f[3]), hex_f64(f[4]));
        let expected = hex_f64(f[5]);

        let got = eval(&toks, x, y, dy);
        assert!(
            same(got, expected),
            "求值不符 [{name}] x={x} y={y} dy={dy}: got {got} expected {expected}"
        );
        n += 1;
    }
    assert!(n > 50, "求值夹具太少: {n}");
}

#[test]
fn path_matches_binary() {
    let text = fixture("path.tsv");
    let mut n = 0;
    let mut total_points = 0;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let name = f[0];
        let flip = f[1] == "1";
        let mode = if f[2] == "1" { Mode::Ode } else { Mode::Normal };
        let (x0, y0, step) = (hex_f64(f[3]), hex_f64(f[4]), hex_f64(f[5]));
        let toks = tokens(f[6]);
        let expected: Vec<(f64, f64)> = if f[8] == "-" {
            Vec::new()
        } else {
            f[8].split(';')
                .map(|p| {
                    let (a, b) = p.split_once(',').unwrap();
                    (hex_f64(a), hex_f64(b))
                })
                .collect()
        };
        assert_eq!(expected.len(), f[7].parse::<usize>().unwrap(), "夹具自相矛盾");

        let got = calculate(&toks, x0, y0, step, flip, mode);
        assert_eq!(
            got.len(),
            expected.len(),
            "点数不符 [{name}] flip={flip} ode={:?} x0={x0} y0={y0} step={step}",
            mode
        );
        for (i, (g, e)) in got.iter().zip(expected.iter()).enumerate() {
            assert!(
                same(g.0, e.0) && same(g.1, e.1),
                "第 {i} 点不符 [{name}] flip={flip} ode={:?}: got {:?} expected {:?}",
                mode,
                g,
                e
            );
        }
        total_points += got.len();
        n += 1;
    }
    assert!(n > 200, "路径夹具太少: {n}");
    assert!(total_points > 1000, "点数太少，夹具可能退化了: {total_points}");
}

#[test]
fn end_to_end_shot_is_stable() {
    // 端到端冒烟：文本 -> 点序列，锁定行为不被"顺手优化"改掉
    let pts = gw2_rebuild::graphwar_shot_calculator::shot_default("sin(x)", 5.0, 2.0);
    assert!(pts.len() > 100, "正弦曲线点数异常: {}", pts.len());
    // 首点必须是开火士兵的位置
    assert_eq!(pts[0], (5.0, 2.0));
    // 曲线必须穿过士兵：第二点应与首点弦长接近 0.01
    let (x1, y1) = pts[1];
    let chord = ((x1 - 5.0).powi(2) + (y1 - 2.0).powi(2)).sqrt();
    assert!((chord - 0.01).abs() < 0.001, "首段弦长 {chord} 偏离 0.01");
}
