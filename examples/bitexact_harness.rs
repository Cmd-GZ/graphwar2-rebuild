//! `bitexact_harness` —— T13「证据升档 sweep」的**我方侧**对拍入口（新文件，不进 `src/`）。
//!
//! 任务书 `.agent-work/specs/T13-bit-exact-sweep.md` ②：
//! 真函数侧由 `tools/bitexact_run.py` 用 unicorn 直调原版 `libgraphwarrust.so`；
//! 本程序喂**同一组输入**给我方 `src/` 的实现，打印**原始位模式**（`f64::to_bits()` 的 16 位十六进制）。
//! 两侧文本协议完全一样，逐行 diff 就是逐位比较。
//!
//! 协议（每行一条，`|` 分隔；数值一律 `0x` + 16 位十六进制 = `f64` 的位模式，
//! 避免十进制往返把比较变成「近似相等」）：
//!
//! ```text
//! [parse]  in:  cid|text                       out: cid|len|tag:bits,tag:bits,...
//! [eval]   in:  cid|text|xb|yb|dyb             out: cid|bits
//! [path]   in:  cid|text|x0b|y0b|stepb|flip|mode   out: cid|n|x:y,x:y,...
//! [pnc]    in:  cid|tag:bits,tag:bits,...      out: cid|len|tag:bits,tag:bits,...
//! ```
//!
//! * `[parse]` 我方 `math_function::parse` ↔ 真 `MathFunction::new` 之后的前缀 token 流（`0x4ed670`）；
//! * `[eval]`  我方 `math_function::eval`   ↔ 真求值器 `0x4eb9d0`；
//! * `[path]`  我方 `graphwar_path_calculator::calculate` ↔ 真采样器 `0x3c2530`；
//! * `[pnc]`   我方 `polish_notation_converter::PolishNotationConverter::reorder`
//!             ↔ 真 `PolishNotationConverter::reorder_recursive`（`0x4f5450`，439 个观测级函数之一）。
//!
//! 用法：
//! ```text
//! cargo run --release --offline -q --example bitexact_harness -- \
//!     --mode pnc --in fixtures/bit_exact_inputs.txt
//! ```
//!
//! 解析完全手写（本 crate **零依赖**，不能引入 serde）。

use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::math_function;
use gw2_rebuild::polish_notation_converter::PolishNotationConverter;
use gw2_rebuild::{Function, Token};

use std::io::{self, BufRead, Write};

fn tag_of(v: u64) -> Option<Function> {
    use Function::*;
    Some(match v {
        0 => VarX,
        1 => VarY,
        2 => VarDy,
        3 => Add,
        4 => Sub,
        5 => UnaryMinus,
        6 => Mul,
        7 => Divide,
        8 => Pow,
        9 => Sqrt,
        10 => Log,
        11 => Ln,
        12 => Exp,
        13 => Abs,
        14 => Sin,
        15 => Cos,
        16 => Tan,
        17 => Const,
        18 => LeftBracket,
        19 => RightBracket,
        _ => return None,
    })
}

fn hex16(v: f64) -> String {
    format!("0x{:016x}", v.to_bits())
}

fn rd_hex(s: &str) -> f64 {
    let s = s.trim();
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    f64::from_bits(u64::from_str_radix(s, 16).expect("bad hex bits"))
}

fn rd_tokens(s: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for item in s.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let (t, b) = item.split_once(':').expect("token needs tag:bits");
        let tag = tag_of(t.parse::<u64>().expect("bad tag")).expect("tag > 19 我方 Function 装不进");
        out.push(Token { tag, value: rd_hex(b) });
    }
    out
}

fn wr_tokens(out: &mut impl Write, toks: &[Token]) -> io::Result<()> {
    write!(out, "{}", toks.len())?;
    for t in toks {
        write!(out, "|{}:{}", t.tag as u64, hex16(t.value))?;
    }
    Ok(())
}

fn main() {
    let mut mode = String::new();
    let mut path = String::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                mode = args[i + 1].clone();
                i += 2;
            }
            "--in" => {
                path = args[i + 1].clone();
                i += 2;
            }
            other => panic!("unknown arg {other}"),
        }
    }
    let f = std::fs::File::open(&path).expect("open --in");
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    for line in io::BufReader::new(f).lines() {
        let line = line.expect("read");
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let c = line.split('|').collect::<Vec<_>>();
        let cid = c[0];
        match mode.as_str() {
            "parse" => {
                let toks = math_function::parse(c[1]);
                write!(out, "{}|", cid).unwrap();
                wr_tokens(&mut out, &toks).unwrap();
                writeln!(out).unwrap();
            }
            "eval" => {
                let toks = math_function::parse(c[1]);
                let (x, y, dy) = (rd_hex(c[2]), rd_hex(c[3]), rd_hex(c[4]));
                let v = math_function::eval(&toks, x, y, dy);
                writeln!(out, "{}|{}", cid, hex16(v)).unwrap();
            }
            "path" => {
                let toks = math_function::parse(c[1]);
                let (x0, y0, step) = (rd_hex(c[2]), rd_hex(c[3]), rd_hex(c[4]));
                let flip = c[5].trim() != "0";
                let mode = if c[6].trim() == "0" { Mode::Normal } else { Mode::Ode };
                let pts = calculate(&toks, x0, y0, step, flip, mode);
                write!(out, "{}|{}", cid, pts.len()).unwrap();
                for (x, y) in &pts {
                    write!(out, "|{}:{}", hex16(*x), hex16(*y)).unwrap();
                }
                writeln!(out).unwrap();
            }
            "pnc" => {
                let toks = rd_tokens(c[1]);
                let res = PolishNotationConverter::reorder(toks);
                write!(out, "{}|", cid).unwrap();
                wr_tokens(&mut out, &res).unwrap();
                writeln!(out).unwrap();
            }
            other => panic!("unknown mode {other}"),
        }
    }
    out.flush().unwrap();
}
