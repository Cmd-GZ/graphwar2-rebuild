//! 函数原文 -> token 串（每行一条），给"与真解析器 `0x4ed670` 对拍"用。
//!
//! 输出一列 `tag:16 位十六进制位串`（与 `fixtures/parse.tsv` 同一口径），所以可以
//! 拿分析仓库 `analysis/gwemu.py`（unicorn 映射 `libgraphwarrust.so`）直调 `0x4ed670`
//! 的返回值**逐 tag** 比对，例如：
//!
//! ```text
//! printf 'x)^2\nx+2)3\n' | cargo run -q --example parse_dump
//! ```
//!
//! ⚠️ 只比 **tag**，以及 `CONST` 的位串：算子 token 的 `value` 字段真二进制**从不
//! 初始化**（复用堆时能看到上一个表达式的残留），它不是可观测契约的一部分
//! （`analysis/difftest_parser.py`、`fixtures/parse.tsv` 同样只看 tag 和常数）。
//!
//! 无主 `)` 的 13 条观测钉在 `tests/parse_stray_paren.rs`，规则说明见 `docs/03` §10。
use gw2_rebuild::math_function::parse;

fn main() {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut lock = stdin.lock();
    let mut buf = String::new();
    loop {
        buf.clear();
        match lock.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("read error: {e}");
                break;
            }
        }
        let text = buf.trim_end_matches('\n');
        let cells: Vec<String> = parse(text)
            .iter()
            .map(|t| format!("{}:{:016x}", t.tag as u32, t.value.to_bits()))
            .collect();
        println!("{text}\t{}", cells.join(";"));
    }
}
