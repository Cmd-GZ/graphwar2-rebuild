//! 回放对拍**收敛**层的命令行入口（`docs/142-diff-converge.md`）。
//!
//! ```text
//! cargo run --release --offline --example diff_conv -- [选项]
//!
//!   --dir <路径>       回放目录（默认 $GW2_REPLAY_DIR 或 ~/.local/share/godot/.../replays）
//!   --limit <N>        只看前 N 份
//!   --max-diffs <N>    每批最多记几条字段差异（默认 8）
//!   --shots <csv>      把**逐发几何**写成 CSV（归因用；不写就不占时间）
//!   --trace <名字>     单份精细：逐批成因 + 逐发几何
//! ```
//!
//! 与 `examples/replay_diff.rs` 的分工：那边是 `docs/136` 的口径与旧测量，
//! 这边是**修完之后**的收敛测量（模式感知 + 修正文本按模式分）。
use gw2_rebuild::diffconv::{cause_name, probe_file, run_file, Cause};
use gw2_rebuild::replaydiff::{default_corpus_dir, Policy};
use std::collections::BTreeMap;

fn main() {
    let mut dir = default_corpus_dir();
    let mut limit = usize::MAX;
    let mut max_diffs = 8usize;
    let mut shots_csv: Option<std::path::PathBuf> = None;
    let mut trace: Option<String> = None;
    let mut policy_kind = "strict";

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
            "--limit" => limit = args.next().expect("--limit 需要值").parse().unwrap(),
            "--max-diffs" => max_diffs = args.next().expect("--max-diffs 需要值").parse().unwrap(),
            "--shots" => shots_csv = Some(args.next().expect("--shots 需要值").into()),
            "--trace" => trace = Some(args.next().expect("--trace 需要值")),
            "--policy" => policy_kind = match args.next().expect("--policy 需要值").as_str() {
                "default" => "default",
                _ => "strict",
            },
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
    let policy = if policy_kind == "default" {
        Policy::default()
    } else {
        gw2_rebuild::diffconv::strict_policy()
    };
    println!("口径: {policy_kind}");

    // ---- 单份追踪 ----
    if let Some(name) = trace {
        let p = dir.join(&name);
        let r = run_file(&p, &policy, max_diffs);
        println!("== {name} ==");
        println!("结算批 {}；逐批成因：", r.scored_batches);
        let mut hist: BTreeMap<&str, usize> = BTreeMap::new();
        for c in &r.causes {
            *hist.entry(cause_name(c)).or_insert(0) += 1;
        }
        for (k, v) in &hist {
            println!("  {:<28} {}", k, v);
        }
        println!("\n逐发几何：");
        for g in probe_file(&p) {
            println!(
                "  #{:<2} {:<7} end_x={:.17} our_stop_x={:.17} | expl_y={:.17} our_stop_y={:.17} | blocked={} killed={:?} obs_deff={}",
                g.batch, g.mode, g.end_x, g.stop_x, g.expl_y, g.stop_y, g.blocked, g.killed, g.obs_death_effects
            );
        }
        println!("\n逐批第一处分歧：");
        for (i, d) in r.first_diffs.iter().enumerate() {
            if let Some(d) = d {
                println!("  批 #{i} {} :: ours={} | observed={}", d.path, d.ours, d.observed);
            }
        }
        println!("\n差异计数：");
        for (k, v) in &r.diff_counts {
            println!("  {:<70} {}", k, v);
        }
        return;
    }

    // ---- 全语料 ----
    let results = {
        let mut out = Vec::new();
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        names.sort();
        for p in names.iter().take(limit) {
            out.push(run_file(p, &policy, max_diffs));
        }
        out
    };

    let files = results.len();
    let scored: usize = results.iter().filter(|r| r.scored_batches > 0).count();
    let clean = results.iter().filter(|r| r.clean).count();
    let batches: usize = results.iter().map(|r| r.scored_batches).sum();
    let mut cause_hist: BTreeMap<&str, usize> = BTreeMap::new();
    for r in &results {
        for c in &r.causes {
            *cause_hist.entry(cause_name(c)).or_insert(0) += 1;
        }
    }
    let mut diff_counts: BTreeMap<String, usize> = BTreeMap::new();
    for r in &results {
        for (k, v) in &r.diff_counts {
            *diff_counts.entry(k.clone()).or_insert(0) += v;
        }
    }

    println!("== 语料 ==");
    println!("文件 {files}；有结算批的 {scored}；结算批 {batches}");
    println!(
        "**完全一致的回放 {} / {} ({:.1}%)**",
        clean,
        files,
        100.0 * clean as f64 / files as f64
    );

    println!("\n== 逐批成因（互斥归因） ==");
    let mut rows: Vec<(&str, usize)> = cause_hist.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, v) in &rows {
        println!("{:<34}{:>6}  ({:.1}%)", k, v, 100.0 * *v as f64 / batches as f64);
    }

    println!("\n== 全部字段差异（前 20） ==");
    let mut dr: Vec<(String, usize)> = diff_counts.into_iter().collect();
    dr.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, v) in dr.iter().take(20) {
        println!("{:<78}{:>6}", k, v);
    }

    // ---- 逐发几何 CSV（归因用） ----
    if let Some(path) = shots_csv {
        let mut f = String::from(
            "file\tbatch\tshots_in_batch\towner\tmode\tturn_mode\tblocked\tstop_x\tstop_y\tend_x\texpl_x\texpl_y\tkilled\tobs_deff\tcorrected_eq\tn_points\ttail\th1_y\th2_x\th2_y\th2_n\tour_n\tflip\tobs_died\n",
        );
        for r in &results {
            let p = dir.join(&r.name);
            for g in probe_file(&p) {
                f.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.17e}\t{:.17e}\t{:.17e}\t{:.17e}\t{:.17e}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.17e}\t{:.17e}\t{}\t{}\t{}\t{}\n",
                    g.file,
                    g.batch,
                    g.shots_in_batch,
                    g.owner,
                    g.mode,
                    g.turn_mode,
                    g.blocked as u8,
                    g.stop_x,
                    g.stop_y,
                    g.end_x,
                    g.expl_x,
                    g.expl_y,
                    g.killed.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("|"),
                    g.obs_death_effects,
                    (g.corrected_ours == g.corrected_obs) as u8,
                    g.n_points,
                    g.tail.iter().map(|(x, y)| format!("{x:.17e} {y:.17e}")).collect::<Vec<_>>().join(";"),
                    g.h1_y,
                    g.h2.0,
                    g.h2.1,
                    g.h2_n,
                    g.our_n,
                    g.flip as u8,
                    g.obs_died.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("|")
                ));
            }
        }
        std::fs::write(&path, f).expect("写 CSV 失败");
        println!("\n已写出 {}", path.display());
    }

    if std::env::var("GW2_LIST_CLEAN").is_ok() {
        let clean: Vec<&str> = results.iter().filter(|r| r.clean).map(|r| r.name.as_str()).collect();
        println!("\n== 完全一致的回放（{} 份） ==\n{:?}", clean.len(), clean);
    }

    // ---- 按成因列出受影响的回放（排障用） ----
    println!("\n== 成因 → 受影响回放（每类最多 6 份） ==");
    let mut by_cause: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for r in &results {
        let mut seen: Vec<&str> = Vec::new();
        for c in &r.causes {
            let n = cause_name(c);
            if *c != Cause::Clean && !seen.contains(&n) {
                seen.push(n);
                by_cause.entry(n.to_string()).or_default().push(r.name.clone());
            }
        }
    }
    for (k, v) in &by_cause {
        println!("{:<34} {} 份：{:?}", k, v.len(), &v[..v.len().min(6)]);
    }

    // ---- 未收敛批的样例（便于逐个看真值） ----
    println!("\n== 未完全一致的回放（前 12，首个非 Clean 成因） ==");
    let mut shown = 0;
    for r in &results {
        if r.clean || r.scored_batches == 0 {
            continue;
        }
        let first = r.causes.iter().find(|c| **c != Cause::Clean);
        println!(
            "  {:<14} 批 {:<3} 首因 {:?}",
            r.name,
            r.scored_batches,
            first.map(cause_name).unwrap_or("-")
        );
        shown += 1;
        if shown >= 12 {
            break;
        }
    }
}
