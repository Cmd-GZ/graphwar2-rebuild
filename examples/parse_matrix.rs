//! 解析器/求值器差分矩阵的**我方一侧**导出器（`tools/parse_matrix.py` 的 Rust 半边）。
//!
//! 读 stdin 的每一行作为一条**原文**（UTF-8，行内可以有 tab，不能有 `\n`），
//! 对每行输出一条 JSONL：
//!
//! ```text
//! {"i":0,"n":5,"tok":[[3,"0000000000000000"],...],"ev":["4008000000000000",...]}
//! ```
//!
//! * `tok`：我方 `math_function::parse` 的前缀 token 串，`[tag, value_bits]`；
//!   **算子 token 的 `value` 字段不是契约**（真二进制从不初始化它，读出来是堆上残留），
//!   所以比对时只看 tag；`CONST`（tag 17）才比较 `value` 的位模式。
//! * `ev`：我方 `math_function::eval` 在 `XY[k]` 上的结果位模式，顺序与 [`XY`] 一致。
//!
//! 另一半是 unicorn 直调真二进制（解析 `0x4ed670`、求值 `0x4eb9d0`），
//! 由 `tools/parse_matrix.py --run` 并行跑；两边用**同一条输入行**、**同一组 `(x,y,dy)`**。
//!
//! 跑法：
//!
//! ```text
//! cargo run --release --offline -q --example parse_matrix -- --xy '0,0,0;1,0,0' < cases.txt > ours.jsonl
//! ```
use gw2_rebuild::math_function::{eval, parse};
use std::io::{BufRead, Write};

/// 求值用的 `(x, y, dy)` 默认表。前 6 条 `y = dy = 0`（游戏里的真实取值），
/// 最后 1 条把 `y` / `y'` 也喂进去 —— 求值器对 `VAR_Y` / `VAR_DY`（tag 1/2）同样要逐位比。
///
/// `tools/parse_matrix.py --xy` 会用**同一个**表显式传进来（两边不许各写一份），
/// 所以这张表只在手跑时生效。
pub const XY: [(f64, f64, f64); 7] = [
    (0.0, 0.0, 0.0),
    (1.0, 0.0, 0.0),
    (-2.5, 0.0, 0.0),
    (3.141592653589793, 0.0, 0.0),
    (19.0, 0.0, 0.0),
    (1e-8, 0.0, 0.0),
    (2.25, 1.25, -0.5),
];

fn main() {
    // `--xy 'x,y,dy;x,y,dy;…'`：与 `tools/parse_matrix.py` 的 `--xy` 同一条串。
    let xy: Vec<(f64, f64, f64)> = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => XY.to_vec(),
        [flag, spec] if flag == "--xy" => spec
            .split(';')
            .map(|t| {
                let p: Vec<f64> = t.split(',').map(|v| v.trim().parse().expect("xy 分量")).collect();
                assert_eq!(p.len(), 3, "每个 --xy 分量要 3 个数：{t:?}");
                (p[0], p[1], p[2])
            })
            .collect(),
        other => {
            eprintln!("未知参数 {other:?}（只支持 --xy 'x,y,dy;…'）");
            std::process::exit(2);
        }
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    let mut lock = stdin.lock();
    let mut buf = String::new();
    let mut i = 0usize;
    loop {
        buf.clear();
        match lock.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("read error: {e}");
                std::process::exit(2);
            }
        }
        let text = buf.strip_suffix('\n').unwrap_or(&buf);
        let toks = parse(text);
        let mut line = String::with_capacity(64 + toks.len() * 28);
        line.push_str(&format!("{{\"i\":{i},\"n\":{},\"tok\":[", toks.len()));
        for (k, t) in toks.iter().enumerate() {
            if k > 0 {
                line.push(',');
            }
            line.push_str(&format!("[{},\"{:016x}\"]", t.tag as u32, t.value.to_bits()));
        }
        line.push_str("],\"ev\":[");
        for (k, &(x, y, dy)) in xy.iter().enumerate() {
            if k > 0 {
                line.push(',');
            }
            line.push_str(&format!("\"{:016x}\"", eval(&toks, x, y, dy).to_bits()));
        }
        line.push_str("]}");
        if writeln!(out, "{line}").is_err() {
            std::process::exit(0); // 下游提前关管道
        }
        i += 1;
    }
    let _ = out.flush();
}
