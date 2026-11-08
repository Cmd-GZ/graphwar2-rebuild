//! **`flip` 分支**对照：同一发里，采样器的 `edx`（flip）到底改变什么（`dispatch/49`）。
//!
//! ```text
//! cargo run --release --offline --example flip_branch_check -- \
//!     --raw <.run/xx.jsonl（必须带 --full-array 的 arr.hex）> --json flip.json
//! ```
//!
//! ## 它回答什么
//!
//! 真机抓到的每一发里都有：采样器入参（`mode/flip/x0/y0/step` + token 流）与**整条点数组**。
//! 本工具把同一组入参喂给内核 **两次**：
//!
//! | 喂法 | 期望 |
//! |---|---|
//! | `calculate(…, flip = 真机读到的值, mode)` | **逐位相同**（否则是我方 flip 分支写错） |
//! | `calculate(…, flip = 取反, mode)` | **不同**（正对照：证明 `edx` 这个标志真的在起作用，不是我读错了寄存器） |
//!
//! 另外对 `Mode::Normal`（`func=0`）的发，采样器 token 流**最后一个** token 是修正常数
//! （`docs/06` §27.0 ④：`Normal` 有尾随常数、`Ode` 没有）。于是还能逐位回答
//! **修正量的取号**：`c = y0 − f(flip ? −x0 : x0)`，把 `flip` 分别取真机值与取反值各算一遍，
//! 看哪一个**逐位等于**那个尾随常数。
//!
//! 只读输入 + 纯计算：不改任何文件、不连游戏。
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::graphwar_shot_calculator::correction;
use gw2_rebuild::{Function, Token};
use std::io::Write;

fn tag_of(t: u64) -> Option<Function> {
    Some(match t {
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
        _ => return None,
    })
}

/// 内存序（小端）16 位十六进制 → f64
fn f64_of_le(h: &str) -> Option<f64> {
    if h.len() < 16 {
        return None;
    }
    let mut b = [0u8; 8];
    for i in 0..8 {
        b[i] = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(f64::from_le_bytes(b))
}

// ── 极简 JSON 取值（与 `examples/ode_array_check.rs` 同一套，零依赖）──────────────
fn get<'a>(o: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{}\":", key);
    let i = o.find(&pat)? + pat.len();
    Some(o[i..].trim_start())
}
fn get_str<'a>(o: &'a str, key: &str) -> Option<&'a str> {
    let rest = get(o, key)?;
    let stripped = rest.strip_prefix('"')?;
    let end = stripped.find('"')?;
    Some(&stripped[..end])
}
fn get_num(o: &str, key: &str) -> Option<f64> {
    let rest = get(o, key)?;
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}
fn get_u64(o: &str, key: &str) -> Option<u64> {
    get_num(o, key).map(|v| v as u64)
}

struct Calc {
    seq: u64,
    ret: String,
    mode: u64,
    flip: u64,
    x0: f64,
    y0: f64,
    step: f64,
    tokens: Vec<Token>,
    n: usize,
    bytes: Vec<u8>,
}

fn hex_bytes(h: &str) -> Vec<u8> {
    let b = h.as_bytes();
    let mut v = Vec::with_capacity(b.len() / 2);
    let mut i = 0;
    while i + 1 < b.len() {
        let hi = (b[i] as char).to_digit(16).unwrap_or(0);
        let lo = (b[i + 1] as char).to_digit(16).unwrap_or(0);
        v.push((hi * 16 + lo) as u8);
        i += 2;
    }
    v
}

fn parse_calc(line: &str) -> Option<Calc> {
    if !line.contains("\"type\":\"calc\"") && !line.contains("\"type\": \"calc\"") {
        return None;
    }
    let arr_at = line.find("\"arr\":")?;
    let (head, tail) = line.split_at(arr_at);
    let n = get_u64(tail, "n")? as usize;
    let hex = get_str(tail, "hex")?;
    let x0 = get_str(head, "x0").and_then(f64_of_le)?;
    let y0 = get_str(head, "y0").and_then(f64_of_le)?;
    let step = get_str(head, "step").and_then(f64_of_le)?;
    let mut tokens = Vec::new();
    if let Some(rest) = get(head, "toks").and_then(|r| r.strip_prefix('[')) {
        let end = rest.find(']').unwrap_or(rest.len());
        for obj in rest[..end].split("},") {
            let obj = obj.trim_start_matches('{');
            let t = get_u64(obj, "t")?;
            let v = get_str(obj, "v").and_then(f64_of_le)?;
            tokens.push(Token { tag: tag_of(t)?, value: v });
        }
    }
    Some(Calc {
        seq: get_u64(head, "seq").unwrap_or(0),
        ret: get_str(head, "ret").unwrap_or("?").to_string(),
        mode: get_u64(head, "mode").unwrap_or(0),
        flip: get_u64(head, "flip").unwrap_or(0),
        x0,
        y0,
        step,
        tokens,
        n,
        bytes: hex_bytes(hex),
    })
}

fn ours_bytes(pts: &[(f64, f64)]) -> Vec<u8> {
    let mut v = Vec::with_capacity(pts.len() * 16);
    for (x, y) in pts {
        v.extend_from_slice(&x.to_le_bytes());
        v.extend_from_slice(&y.to_le_bytes());
    }
    v
}

#[derive(Default)]
struct Tally {
    calc: usize,
    flip0: usize,
    flip1: usize,
    captured_same: usize,
    captured_same_flip1: usize,
    opposite_same: usize,
    opposite_same_flip1: usize,
    dir_match: usize,
    dir_mismatch: usize,
    normal: usize,
    const_tested: usize,
    const_rule3: usize,
    const_rule9: usize,
    const_flip_ok: usize,
    const_opp_ok: usize,
    disc: usize,
    disc_flip1: usize,
    disc_flip_ok: usize,
    disc_opp_ok: usize,
}

fn main() {
    let mut raw: Vec<String> = Vec::new();
    let mut json_out: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--raw" => raw.push(args.next().expect("--raw needs a path")),
            "--json" => json_out = Some(args.next().expect("--json needs a path")),
            other => {
                eprintln!("[flip_branch_check] unknown arg {other}");
                std::process::exit(2);
            }
        }
    }
    if raw.is_empty() {
        eprintln!("[flip_branch_check] usage: --raw <jsonl> [--raw …] [--json out.json]");
        std::process::exit(2);
    }

    let mut rows: Vec<String> = Vec::new();
    let mut t = Tally::default();

    for path in &raw {
        let text = std::fs::read_to_string(path).expect("read raw jsonl");
        let src = path.rsplit('/').next().unwrap_or(path).to_string();
        for line in text.lines() {
            let line = line.trim();
            let Some(c) = parse_calc(line) else { continue };
            if c.n == 0 || c.bytes.len() != c.n * 16 {
                continue;
            }
            t.calc += 1;
            if c.flip == 0 {
                t.flip0 += 1;
            } else {
                t.flip1 += 1;
            }
            let mode = if c.mode == 1 { Mode::Ode } else { Mode::Normal };
            if c.mode == 0 {
                t.normal += 1;
            }
            let captured = calculate(&c.tokens, c.x0, c.y0, c.step, c.flip != 0, mode);
            let opposite = calculate(&c.tokens, c.x0, c.y0, c.step, c.flip == 0, mode);
            let cap_ok = ours_bytes(&captured) == c.bytes;
            let opp_ok = ours_bytes(&opposite) == c.bytes;
            if cap_ok {
                t.captured_same += 1;
                if c.flip != 0 {
                    t.captured_same_flip1 += 1;
                }
            }
            if opp_ok {
                t.opposite_same += 1;
                if c.flip != 0 {
                    t.opposite_same_flip1 += 1;
                }
            }
            // 点数组走向：flip=1 期望 x 递减、flip=0 期望 x 递增
            let first_x = f64::from_le_bytes(c.bytes[0..8].try_into().unwrap());
            let last_off = (c.n - 1) * 16;
            let last_x = f64::from_le_bytes(c.bytes[last_off..last_off + 8].try_into().unwrap());
            let dir_ok = if c.flip != 0 {
                last_x < first_x
            } else {
                last_x > first_x
            };
            if dir_ok {
                t.dir_match += 1;
            } else {
                t.dir_mismatch += 1;
            }
            // `Mode::Normal`：修正常数在 token 流的**两头**（本轮实测的排布，见 docs/06 §31.4）：
            //   `[Add|Sub] f_tokens… Const(|c|)` —— 第一个 token 是取号用的运算符（Sub ⇒ c<0），
            //   最后一个 token 是 `|c|`，中间是函数本身。于是可以逐位比
            //   `c = y0 − f(flip ? −x0 : x0)` 在 `flip` 取真机值 / 取反值下哪一个对。
            let mut const_bits: Option<u64> = None;
            if c.mode == 0 && c.tokens.len() >= 3 {
                let first = c.tokens[0].tag;
                let last = *c.tokens.last().unwrap();
                if (first == Function::Add || first == Function::Sub) && last.tag == Function::Const {
                    let sign = if first == Function::Sub { -1.0 } else { 1.0 };
                    const_bits = Some((sign * last.value).to_bits());
                }
            }
            let mut c_flip_ok: Option<bool> = None;
            let mut c_opp_ok: Option<bool> = None;
            let mut c_full: Option<(f64, f64)> = None;
            if let Some(cb) = const_bits {
                let head = &c.tokens[1..c.tokens.len() - 1];
                let k = correction(head, c.x0, c.y0, c.flip != 0);
                let k2 = correction(head, c.x0, c.y0, c.flip == 0);
                // ★ 真机 token 流里的那个常数有两种精度（本轮实测，见 docs/06 §31.4）：
                //   · 进化器评的候选（`step = 0.5`）：**全精度** `c`，逐位相等；
                //   · 真正打出去的那一发（`step = 0.01`）：`c` 先被 `correction_text` 印成
                //     **3 位小数**再 parse 回来（`src/game.rs` 的"印出来的文本再 parse"），
                //     所以只能比到 3 位小数。
                // 判据：全精度命中 **或** 3 位小数命中 ⇒ 这个取号对。
                let want = f64::from_bits(cb).abs();
                let at3 = |v: f64| {
                    format!("{:.3}", v.abs()).parse::<f64>().ok().map(|p| p.to_bits())
                        == Some(want.to_bits())
                };
                let r_exact = k.abs().to_bits() == want.to_bits();
                let o_exact = k2.abs().to_bits() == want.to_bits();
                let r3 = at3(k);
                let o3 = at3(k2);
                c_flip_ok = Some(r_exact || r3);
                c_opp_ok = Some(o_exact || o3);
                c_full = Some((k, k2));
                t.const_tested += 1;
                if r3 {
                    t.const_rule3 += 1;
                }
                if r_exact {
                    t.const_rule9 += 1;
                }
                if c_flip_ok == Some(true) {
                    t.const_flip_ok += 1;
                }
                if c_opp_ok == Some(true) {
                    t.const_opp_ok += 1;
                }
                // ★ 只有"两种取号给出**不同**的 c"的那些发才有判别力（f 是常数/偶函数时两边相等）
                if k.to_bits() != k2.to_bits() {
                    t.disc += 1;
                    if c.flip != 0 {
                        t.disc_flip1 += 1;
                    }
                    if c_flip_ok == Some(true) {
                        t.disc_flip_ok += 1;
                    }
                    if c_opp_ok == Some(true) {
                        t.disc_opp_ok += 1;
                    }
                }
            }
            rows.push(format!(
                "{{\"source\":\"{src}\",\"seq\":{},\"ret\":\"{}\",\"mode\":{},\"flip\":{},\
\"n\":{},\"n_ours_captured\":{},\"n_ours_opposite\":{},\
\"same_bytes_captured_flip\":{},\"same_bytes_opposite_flip\":{},\
\"x_dir_first\":{:.17e},\"x_dir_last\":{:.17e},\"x_dir_matches_flip\":{},\
\"const_bits\":{},\"const_eq_correction_flip\":{},\"const_eq_correction_opposite\":{},\
\"c_full_flip\":{},\"c_full_opposite\":{}}}",
                c.seq,
                c.ret,
                c.mode,
                c.flip,
                c.n,
                captured.len(),
                opposite.len(),
                cap_ok,
                opp_ok,
                first_x,
                last_x,
                dir_ok,
                const_bits
                    .map(|b| format!("\"{b:016x}\""))
                    .unwrap_or_else(|| "null".into()),
                c_flip_ok.map(|b| b.to_string()).unwrap_or_else(|| "null".into()),
                c_opp_ok.map(|b| b.to_string()).unwrap_or_else(|| "null".into()),
                c_full.map(|(a, _)| format!("{a:.17e}")).unwrap_or_else(|| "null".into()),
                c_full.map(|(_, b)| format!("{b:.17e}")).unwrap_or_else(|| "null".into()),
            ));
        }
    }

    println!(
        "[flip_branch_check] calc 行 {}（flip=0 {} / flip=1 {}）｜真机 flip 复算逐位相同 {}（其中 flip=1 {}）\
         ｜取反 flip 复算逐位相同 {}/{}（其中 flip=1 {}）｜走向与 flip 一致 {}/{}｜\
         Normal 发 {}（常数 token 可测 {}：取真机 flip 对 {}、取反对 {}；其中 3 位小数那条命中 {}、\
         全精度那条命中 {}｜两种取号**不同**的判别发 {}：真机 flip 对 {}、取反对 {}（其中 flip=1 的判别发 {}））",
        t.calc,
        t.flip0,
        t.flip1,
        t.captured_same,
        t.captured_same_flip1,
        t.opposite_same,
        t.calc,
        t.opposite_same_flip1,
        t.dir_match,
        t.dir_match + t.dir_mismatch,
        t.normal,
        t.const_tested,
        t.const_flip_ok,
        t.const_opp_ok,
        t.const_rule3,
        t.const_rule9,
        t.disc,
        t.disc_flip_ok,
        t.disc_opp_ok,
        t.disc_flip1,
    );
    if let Some(p) = json_out {
        let mut f = std::fs::File::create(p).expect("create json");
        for r in &rows {
            writeln!(f, "{r}").unwrap();
        }
    }
}
