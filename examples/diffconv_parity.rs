//! **两条路径的逐位一致性**对拍入口（`docs/06-verification.md` §30.5，任务 16 的核心验收）。
//!
//! ```text
//! cargo run --release --offline --example diffconv_parity -- \
//!     --dir <语料> [--limit N] [--max-samples 20]
//! ```
//!
//! 两条路径对**同一批回放**跑同一批：
//!
//! | 路径 | 是什么 |
//! |---|---|
//! | K | `game::Match::fire_functions` + `Match::fire_batch` —— 真实实现（内核） |
//! | C | `diffconv::ConvPredictor::fire_batch` —— 收敛层 |
//!
//! 比的是**中间结果**，不是"报文对上回放"：**点序列**（逐坐标 `to_bits`）/ **stop** /
//! **blocked** / **killed**（含顺序）/ 命中弧长 / 修正文本。另单列挖洞点与爆炸点。
//!
//! ⚠️ **DiffEq 批不参与**：内核开火路径把 `Mode::Normal` 写死（`src/game.rs` 的
//! `fire_functions`），DiffEq 局在内核里没有开火路径 ⇒ 不可比，只计数（见 §30.4）。
//! 这不是"近似通过"：那是**内核缺功能**，不是收敛层算错。
//!
//! 退出码：Normal 轴有任何一处不一致 ⇒ `1`（可直接当断言用）。
use gw2_rebuild::_scaffold::diffconv::{parity_file, Parity};
use gw2_rebuild::_scaffold::replaydiff::default_corpus_dir;

fn main() {
    let mut dir = default_corpus_dir();
    let mut limit = usize::MAX;
    let mut max_samples = 20usize;
    let mut one: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
            "--limit" => limit = args.next().expect("--limit 需要值").parse().unwrap(),
            "--max-samples" => {
                max_samples = args.next().expect("--max-samples 需要值").parse().unwrap()
            }
            "--file" => one = Some(args.next().expect("--file 需要值")),
            other => {
                eprintln!("未知参数 {other}");
                std::process::exit(2);
            }
        }
    }
    if !dir.is_dir() {
        eprintln!("回放目录不存在: {}", dir.display());
        std::process::exit(2);
    }

    let p: Parity = if let Some(name) = one {
        let mut p = Parity::default();
        parity_file(&dir.join(name), &mut p, max_samples);
        p
    } else {
        let mut p = Parity::default();
        let mut names: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        names.sort();
        names.truncate(limit);
        for n in &names {
            parity_file(n, &mut p, max_samples);
        }
        p
    };

    println!("--- 两条路径逐位一致性（K = 内核 `Match::fire_batch`，C = 收敛层 `ConvPredictor`）---");
    println!("  {}", p.summary());
    println!(
        "  挖洞点（两边都调 `game::hole_point`）：逐位相同 {} / {}",
        p.destroy_equal, p.destroy_compared
    );
    println!(
        "  爆炸点（内核 ④ 仍用 stop，收敛层用 hole_point）：相同 {} / {} —— 差的那部分是**内核侧**未接线的 `Y` 类（§24）",
        p.explosion_equal, p.explosion_compared
    );
    println!(
        "  DiffEq 批 {} / {} 发：**内核没有 Ode 开火路径**（`fire_functions` 写死 `Mode::Normal`）⇒ 不可比，不计入分母",
        p.diff_eq_batches, p.diff_eq_shots
    );
    if p.mismatches_total == 0 && p.shots_identical == p.shots {
        println!("  结论：Normal 轴 {} 发**逐位全同**，没有一处不一致", p.shots);
    } else {
        println!(
            "  结论：**不一致 {} 处**（{}/{} 发全同）",
            p.mismatches_total, p.shots_identical, p.shots
        );
        for m in &p.mismatches {
            println!(
                "    {} 批 #{} 第 {} 发 · {}：C={} K={}",
                m.file, m.batch, m.shot, m.field, m.ours, m.kernel
            );
        }
        std::process::exit(1);
    }
}
