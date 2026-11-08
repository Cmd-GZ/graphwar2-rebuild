//! **DiffEq / `Mode::Ode` 真机点数组 vs 我方积分**逐位对照（`dispatch/40`）。
//!
//! ```text
//! cargo run --release --offline --example ode_array_check -- \
//!     --raw <frida_ode_probe 的原始 JSONL> --json repro.json
//! ```
//!
//! ## 它回答什么
//!
//! 真机抓到的一发里，采样器 `0x3c2530` 的**入参**（`mode/flip/x0/y0/step` + 喂进去的
//! token 流）和**返回的整条点数组**（`(ptr, n)` + 16·n 字节）都是现成的。
//! ⇒ 把同一组入参喂给内核 [`graphwar_path_calculator::calculate`]，**逐点逐位**比：
//!
//! | 结果 | 含义 |
//! |---|---|
//! | `same_bytes = true` | 我方 `Ode` 积分与真机**逐位相同**（这一发上"我方积分跑偏"被否） |
//! | `same_bytes = false` | 我方在 `first_diff_index` 那个点开始分叉 —— "我方积分先跑偏" |
//!
//! 这条判据的两边**都不来自回放报文**：左边是真机内存里的字节，右边是我方实现。
//! 所以它与 `docs/06` §25 那条"回放自身是否闭合"的判据**正交**，可以互相做交叉证据。
//!
//! ## 输入格式
//!
//! `tools/frida_ode_probe.js` 的 `calc` 行（需要驱动加 `--full-array`，否则没有 `arr.hex`）。
//! **不入库**：原始 JSONL 里带整条数组 hex，入库样本只留摘要（见 `fixtures/frida_ode_sample.jsonl`）。
//!
//! ## 输出
//!
//! 每行一条 `calc`（一发两次调用，各一条），字段：
//! `source` / `shot` / `seq` / `ret` / `mode` / `n_captured` / `n_ours` / `same_len` /
//! `same_bytes` / `diff_points` / `first_diff_index` / `verdict`。
//! `--json` 把同一个列表写成 JSON（供 `tools/frida_ode_probe.py --extract --repro` 并进入库样本）。
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
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

// ── 极简 JSON 取值（本仓库零依赖；只解析本工具要的那几个键）────────────────────
fn get<'a>(o: &'a str, key: &str) -> Option<&'a str> {
    // 找 `"key":` 之后的值（字符串 / 数字 / 对象都不深入，调用方按需取）
    let pat = format!("\"{}\":", key);
    let i = o.find(&pat)? + pat.len();
    let rest = &o[i..];
    let rest = rest.trim_start();
    Some(rest)
}
fn get_str<'a>(o: &'a str, key: &str) -> Option<&'a str> {
    let rest = get(o, key)?;
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some(&stripped[..end])
    } else {
        None
    }
}
fn get_num(o: &str, key: &str) -> Option<f64> {
    let rest = get(o, key)?;
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}
fn get_u64(o: &str, key: &str) -> Option<u64> {
    get_num(o, key).map(|v| v as u64).or_else(|| {
        // `-0` / 科学计数 之类解析不出来时退回整数直读
        let rest = get(o, key)?;
        let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        rest[..end].parse().ok()
    })
}

struct Calc {
    source: String,
    seq: u64,
    ret: String,
    mode: u64,
    flip: u64,
    x0: f64,
    y0: f64,
    step: f64,
    tokens: Vec<Token>,
    n: usize,
    hex: String,
}

/// 从一行 `calc` 里把要用到的字段抠出来（`tokens` 是 `{t, v}` 数组）
fn parse_calc(line: &str, source: &str) -> Option<Calc> {
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
    let mode = get_u64(head, "mode").unwrap_or(0);
    let flip = get_u64(head, "flip").unwrap_or(0);
    let seq = get_u64(head, "seq").unwrap_or(0);
    let ret = get_str(head, "ret").unwrap_or("?").to_string();
    // tokens：`"tokens":{"cap":..,"ptr":..,"len":..,"toks":[{"t":1,"v":"..."}]}`
    let mut tokens = Vec::new();
    // ⚠ 驱动是 `json.dumps(p)` 写的，键值之间有**空格**（`"toks": [`），别按无空格的写法找
    if let Some(rest) = get(head, "toks").and_then(|r| r.strip_prefix('[')) {
        let rest = rest;
        let end = rest.find(']').unwrap_or(rest.len());
        for obj in rest[..end].split("},") {
            let obj = obj.trim_start_matches('{');
            let t = get_u64(obj, "t")?;
            let v = get_str(obj, "v").and_then(f64_of_le)?;
            tokens.push(Token {
                tag: tag_of(t)?,
                value: v,
            });
        }
    }
    Some(Calc {
        source: source.to_string(),
        seq,
        ret,
        mode,
        flip,
        x0,
        y0,
        step,
        tokens,
        n,
        hex: hex.to_string(),
    })
}

/// 我方点列 → 与真机同样的字节布局（每点 `x: f64, y: f64`，小端）
fn ours_bytes(pts: &[(f64, f64)]) -> Vec<u8> {
    let mut v = Vec::with_capacity(pts.len() * 16);
    for (x, y) in pts {
        v.extend_from_slice(&x.to_le_bytes());
        v.extend_from_slice(&y.to_le_bytes());
    }
    v
}

fn hex_bytes(h: &str) -> Vec<u8> {
    let b = h.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 2);
    let mut i = 0;
    while i + 1 < b.len() {
        let s = std::str::from_utf8(&b[i..i + 2]).unwrap_or("00");
        out.push(u8::from_str_radix(s, 16).unwrap_or(0));
        i += 2;
    }
    out
}

fn main() {
    let mut raw: Vec<String> = Vec::new();
    let mut json_out: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--raw" => raw.push(args.next().unwrap_or_default()),
            "--json" => json_out = args.next(),
            "-h" | "--help" => {
                println!("usage: ode_array_check --raw <raw.jsonl> [--raw …] [--json out.json]");
                return;
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }
    if raw.is_empty() {
        eprintln!("need --raw <frida_ode_probe 的原始 JSONL>（驱动要加 --full-array）");
        std::process::exit(2);
    }

    let mut rows: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut same_len_n = 0usize;
    let mut same_bytes_n = 0usize;
    let mut by_mode: std::collections::BTreeMap<u64, (usize, usize)> = std::collections::BTreeMap::new();

    for path in &raw {
        // 只记文件名（**不留路径**：这份 JSON 可能被摘进入库样本）
        let base = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        let src = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("read {path}: {e}");
            std::process::exit(1)
        });
        // 一"发"= 一个 pos_read；给同一发里的 calc 记同一个 shot 号
        // 一发 = 一个 `pos_read`；`calc` 行出现在它**之前**
        //   ⇒ 一条 `calc` 属于哪一发 = 处理它时**已经见过**几个 `pos_read`（不是 +1，见 §26.7 的自更正）
        let mut pos_seen = 0i64;
        let mut calc_in_shot = -1i64;
        for line in src.lines() {
            if line.contains("\"type\":\"pos_read\"") || line.contains("\"type\": \"pos_read\"") {
                pos_seen += 1;
                calc_in_shot = -1;
                continue;
            }
            let Some(c) = parse_calc(line, &base) else {
                continue;
            };
            let shot_index_for_calc = pos_seen;
            calc_in_shot += 1;
            total += 1;
            let mode = if c.mode == 1 { Mode::Ode } else { Mode::Normal };
            let ours = calculate(&c.tokens, c.x0, c.y0, c.step, c.flip != 0, mode);
            let ob = ours_bytes(&ours);
            let cap = hex_bytes(&c.hex);
            let same_len = ours.len() == c.n && cap.len() == c.n * 16;
            if same_len {
                same_len_n += 1;
            }
            let mut diff_points = 0usize;
            let mut first_diff: Option<usize> = None;
            if same_len {
                for i in 0..c.n {
                    if ob[i * 16..i * 16 + 16] != cap[i * 16..i * 16 + 16] {
                        diff_points += 1;
                        if first_diff.is_none() {
                            first_diff = Some(i);
                        }
                    }
                }
            }
            let same_bytes = same_len && diff_points == 0;
            if same_bytes {
                same_bytes_n += 1;
            }
            let e = by_mode.entry(c.mode).or_insert((0, 0));
            e.0 += 1;
            if same_bytes {
                e.1 += 1;
            }
            let verdict = if same_bytes {
                "bitwise_identical"
            } else if !same_len {
                "length_differs"
            } else {
                "points_differ"
            };
            let row = format!(
                "{{\"source\":\"{}\",\"shot\":{},\"idx_in_shot\":{},\"seq\":{},\"ret\":\"{}\",\"mode\":{},\
                 \"flip\":{},\"tokens\":{},\"n_captured\":{},\"n_ours\":{},\"same_len\":{},\"same_bytes\":{},\
                 \"diff_points\":{},\"first_diff_index\":{},\"verdict\":\"{}\"}}",
                c.source,
                (shot_index_for_calc).max(0),
                calc_in_shot.max(0),
                c.seq,
                c.ret,
                c.mode,
                c.flip,
                c.tokens.len(),
                c.n,
                ours.len(),
                same_len,
                same_bytes,
                diff_points,
                first_diff.map(|v| v as i64).unwrap_or(-1),
                verdict
            );
            println!("{row}");
            rows.push(row);
        }
    }

    eprintln!(
        "[ode_array_check] calc 行 {} ｜ 长度相同 {} ｜ **逐位相同 {}** ｜ 按 mode（总数, 逐位相同）= {:?}",
        total, same_len_n, same_bytes_n, by_mode
    );
    if let Some(p) = json_out {
        let body = format!("[{}]", rows.join(","));
        let mut f = std::fs::File::create(&p).unwrap_or_else(|e| {
            eprintln!("create {p}: {e}");
            std::process::exit(1)
        });
        f.write_all(body.as_bytes()).ok();
        eprintln!("[ode_array_check] wrote {p}");
    }
}
