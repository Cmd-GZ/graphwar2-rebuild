//! 回放**消息级**对拍的命令行入口（`docs/136-replay-diff.md`）。
//!
//! ```text
//! cargo run --release --example replay_diff -- [选项]
//!
//!   --dir <路径>      回放目录（默认 $GW2_REPLAY_DIR 或 ~/.local/share/godot/.../replays）
//!   --limit <N>       只看前 N 份（调试用）
//!   --show <K>        打印前 K 条"第一分歧"（默认 8）
//!   --max-diffs <N>   每批最多记几条字段差异（默认 4）
//!   --quiet           只打汇总
//!   --json <路径>     把逐文件结论写成 JSONL（一行一份，便于二次统计）
//!
//! 退出码：0 = 跑完（无论有没有分歧）；2 = 目录不存在 / 一份都没读到。
//! ```
use gw2_rebuild::_scaffold::replaydiff::{
    class_name, default_corpus_dir, run_dir, CorpusReport, DiffClass, Policy, Replay,
};
use std::collections::BTreeMap;

fn pct(a: usize, b: usize) -> String {
    if b == 0 {
        "-".into()
    } else {
        format!("{:.1}%", 100.0 * a as f64 / b as f64)
    }
}

/// 我方能不能产出某种消息（口径里的 `produced_*`）。
fn capability(policy: &Policy, kind: &str) -> &'static str {
    if policy.produced_kinds.contains(&kind) {
        "产出"
    } else if matches!(
        kind,
        "PosInfo" | "TeamInfo" | "PlayerInfo" | "SoldierInfo" | "NameInfo" | "ColorInfo"
            | "AddObstacle" | "ClearObstacles" | "AxisModeInfo" | "FunctionModeInfo"
            | "TurnModeInfo" | "TimeModeInfo" | "GameNameInfo" | "TurnLimitInfo"
    ) {
        "只作输入"
    } else {
        "未实现"
    }
}

fn main() {
    let mut dir = default_corpus_dir();
    let mut limit = usize::MAX;
    let mut show = 8usize;
    let mut max_diffs = 4usize;
    let mut quiet = false;
    let mut json_out: Option<std::path::PathBuf> = None;
    let mut trace: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
            "--limit" => limit = args.next().expect("--limit 需要值").parse().unwrap(),
            "--show" => show = args.next().expect("--show 需要值").parse().unwrap(),
            "--max-diffs" => max_diffs = args.next().expect("--max-diffs 需要值").parse().unwrap(),
            "--quiet" => quiet = true,
            "--json" => json_out = Some(args.next().expect("--json 需要值").into()),
            "--trace" => trace = Some(args.next().expect("--trace 需要文件名的值")),
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

    let policy = Policy::default();
    let mut report = run_dir(&dir, &policy, max_diffs);
    if report.files.len() > limit {
        report.files.truncate(limit);
    }
    if report.files.is_empty() {
        eprintln!("目录里没有文件: {}", dir.display());
        std::process::exit(2);
    }

    println!("== 语料 ==");
    println!("目录: {}", report.dir);
    let t = report.total();
    println!(
        "文件 {}（读取失败 {}）；报文 {} 条；解压后 {:.2} MB",
        t.files,
        t.files_with_error,
        t.msgs,
        t.plain_bytes as f64 / (1024.0 * 1024.0)
    );
    println!(
        "结算批 {}（有开火的 {}）；开火 {} 发",
        t.batches, t.scored_batches, t.shots
    );

    // ---- 消息词汇表 + 我方能力 ----
    let mut vocab: BTreeMap<String, usize> = BTreeMap::new();
    let mut unproduced_all: BTreeMap<String, usize> = BTreeMap::new();
    for f in &report.files {
        if f.error.is_some() {
            continue;
        }
        for (k, v) in &f.unproduced {
            *unproduced_all.entry(k.clone()).or_insert(0) += v;
        }
    }
    {
        // 重新数一遍全语料的 kind（快：只解压不改状态）
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        names.sort();
        for p in names.iter().take(limit) {
            if let Ok(r) = Replay::load(p) {
                for (k, v) in r.kind_histogram() {
                    *vocab.entry(k).or_insert(0) += v;
                }
            }
        }
    }

    if !quiet {
        println!("\n== 回放消息词汇表（{} 类）与我们的能力 ==", vocab.len());
        println!("{:<22}{:>8}  {}", "消息", "条数", "我方");
        let mut rows: Vec<(&String, &usize)> = vocab.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (k, v) in rows {
            println!("{:<22}{:>8}  {}", k, v, capability(&policy, k));
        }
    }

    println!("\n== 逐批结论（消息级对拍） ==");
    println!(
        "结算批命中（内容/多重集）: {} / {}  ({})",
        t.content_ok, t.scored_batches, pct(t.content_ok, t.scored_batches)
    );
    println!(
        "结算批命中（含批内顺序）  : {} / {}  ({})",
        t.order_ok, t.scored_batches, pct(t.order_ok, t.scored_batches)
    );
    println!(
        "消息种类序列命中          : {} / {}  ({})",
        t.kind_seq_ok, t.scored_batches, pct(t.kind_seq_ok, t.scored_batches)
    );
    println!(
        "effect_id 结构不变量命中  : {} / {}",
        t.counter_ok, t.scored_batches
    );
    println!(
        "延迟段死亡集合命中        : {} / {}  ({})",
        t.deaths_ok, t.deaths_checked, pct(t.deaths_ok, t.deaths_checked)
    );
    println!(
        "  其中 我方多报 {} / 漏报 {} / 报错人 {}",
        t.deaths_extra, t.deaths_missing, t.deaths_wrong
    );
    println!(
        "延迟段挖洞点命中          : {} / {}  ({})",
        t.destroy_ok, t.destroy_checked, pct(t.destroy_ok, t.destroy_checked)
    );
    if t.invalid_json > 0 {
        println!("我方 emitter 产出非法 JSON: {} 条", t.invalid_json);
    }
    println!(
        "完全一致的回放            : {} / {}  ({})",
        t.files_clean, t.files, pct(t.files_clean, t.files)
    );
    println!(
        "有分歧的回放              : {}；读取/建模失败 {}",
        t.files_with_divergence, t.files_with_error
    );
    if t.collapsed > 0 {
        println!("回放里同一 tick 连续完全重复（已折叠）: {} 条", t.collapsed);
    }

    println!("\n== 几何探针（同一发、同一起点，不同步长） ==");
    println!("回放按回合模式分的批数: Normal {} / DiffEq {}", t.batches_normal, t.batches_diffeq);
    println!("`end_x` 偏差（我方停点 x vs 回放 FunctionPoints.end_x）:");
    for (k, v) in &t.geom.end_x {
        println!("  {:<20} {}", k, v);
    }
    println!("`end_x` 偏差 —— 只看我方判定**撞障碍**的发:");
    for (k, v) in &t.geom.end_x_blocked {
        println!("  {:<28} {}", k, v);
    }
    println!("`end_x` 偏差 —— 只看**自由飞行**（到边界）的发:");
    for (k, v) in &t.geom.end_x_free {
        println!("  {:<28} {}", k, v);
    }
    println!("爆炸点 y 偏差（我方停点 y vs 回放 EffectInfo(Explosion).pos_y）:");
    for (k, v) in &t.geom.stop_y {
        println!("  {:<20} {}", k, v);
    }
    println!("死亡集合命中（同一个步长下）:");
    for (k, v) in &t.geom.deaths {
        println!("  {:<20} {}/{}", k, v.0, v.1);
    }
    println!("挖洞点偏差（CreateObstacle 级别分档）: dx {} | dy {}", t.destroy_dx, t.destroy_dy);

    println!("\n== 全部分歧：按 消息|字段|成因（前 20） ==");
    let mut all: Vec<(String, usize)> = t.diff_counts.clone().into_iter().collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (k, v) in all.iter().take(20) {
        println!("{:<70}{:>6}", k, v);
    }

    println!("\n== 第一处分歧：按成因 ==");
    for (k, v) in report.first_by_class() {
        println!("{:<24}{:>5}", k, v);
    }
    println!("\n== 第一处分歧：按字段路径（前 15） ==");
    let mut paths: Vec<(String, usize)> = report.first_by_path().into_iter().collect();
    paths.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (k, v) in paths.iter().take(15) {
        println!("{:<34}{:>5}", k, v);
    }
    println!("\n== 第一处分歧：按消息种类（前 15） ==");
    let mut kinds: Vec<(String, usize)> = report.first_by_kind().into_iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (k, v) in kinds.iter().take(15) {
        println!("{:<34}{:>5}", k, v);
    }

    println!("\n== 我方还不产的消息（不是分歧，是缺口） ==");
    let mut un: Vec<(String, usize)> = unproduced_all.into_iter().collect();
    un.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (k, v) in &un {
        println!("{:<28}{:>8}", k, v);
    }

    if show > 0 {
        println!("\n== 第一处分歧样例（前 {show} 份） ==");
        let mut printed = 0;
        for f in &report.files {
            if printed >= show {
                break;
            }
            if let Some(fd) = &f.first {
                printed += 1;
                println!(
                    "{}  批 #{} tick={} 发数={}  [{:?}]",
                    f.name, fd.batch_index, fd.tick, fd.shots, class_name(&fd.diff.class)
                );
                println!("   {}", fd.diff.path);
                println!("   我方 = {}", trim(&fd.diff.ours));
                println!("   回放 = {}", trim(&fd.diff.observed));
            }
        }
    }

    if let Some(name) = trace {
        trace_file(&dir.join(&name), show);
    }

    if let Some(path) = json_out {
        use std::io::Write;
        let mut f = std::fs::File::create(&path).expect("--json 打不开");
        for fr in &report.files {
            let first = fr.first.as_ref().map(|fd| {
                format!(
                    "{{\"batch\":{},\"tick\":{},\"path\":{},\"class\":{},\"ours\":{},\"observed\":{}}}",
                    fd.batch_index,
                    fd.tick,
                    jstr(&fd.diff.path),
                    jstr(&class_name(&fd.diff.class)),
                    jstr(&fd.diff.ours),
                    jstr(&fd.diff.observed)
                )
            });
            writeln!(
                f,
                "{{\"file\":{},\"msgs\":{},\"batches\":{},\"scored\":{},\"shots\":{},\"content_ok\":{},\"order_ok\":{},\"kind_seq_ok\":{},\"deaths_ok\":{},\"deaths_checked\":{},\"destroy_ok\":{},\"destroy_checked\":{},\"error\":{},\"first\":{}}}",
                jstr(&fr.name),
                fr.msgs,
                fr.batches,
                fr.scored_batches,
                fr.shots,
                fr.content_ok,
                fr.order_ok,
                fr.kind_seq_ok,
                fr.deaths_ok,
                fr.deaths_checked,
                fr.destroy_ok,
                fr.destroy_checked,
                fr.error.as_ref().map(|e| jstr(e)).unwrap_or_else(|| "null".into()),
                first.unwrap_or_else(|| "null".into())
            )
            .unwrap();
        }
        eprintln!("已写出 {}", path.display());
    }

    // 汇总一遍分布，便于写文档
    let _ = report_summary(&report);
}

fn report_summary(r: &CorpusReport) -> Vec<(String, usize)> {
    let mut m: BTreeMap<String, usize> = BTreeMap::new();
    for f in &r.files {
        if let Some(fd) = &f.first {
            let c = match fd.diff.class {
                DiffClass::NumericFormat => "fmt",
                DiffClass::NumericValue { .. } => "val1ulp",
                DiffClass::NumericValueFar => "valfar",
                DiffClass::Value => "value",
                DiffClass::Presence => "presence",
                DiffClass::FieldOrder => "field-order",
                DiffClass::MessageOrder => "msg-order",
                DiffClass::KindSequence => "kind-seq",
                DiffClass::CounterGap => "counter",
            };
            *m.entry(c.to_string()).or_insert(0) += 1;
        }
    }
    m.into_iter().collect()
}

fn trim(s: &str) -> String {
    if s.chars().count() > 220 {
        let t: String = s.chars().take(220).collect();
        format!("{t}…")
    } else {
        s.to_string()
    }
}

fn jstr(s: &str) -> String {
    gw2_rebuild::_scaffold::replaydiff::wire::quote(s)
}

/// 精细追踪一份回放：逐批打印"回放的种类序列 / 我方的种类序列 / 差异 / 死亡 / 挖洞"。
fn trace_file(path: &std::path::Path, _show: usize) {
    use gw2_rebuild::_scaffold::replaydiff::{
        compare_batch, split_batches, Inputs, Policy, Predictor, Replay,
    };
    let Ok(r) = Replay::load(path) else {
        println!("读不了 {}", path.display());
        return;
    };
    let policy = Policy::default();
    let batches = split_batches(&r);
    println!("\n== trace {}  报文 {} 批 {} ==", path.display(), r.msgs.len(), batches.len());
    let Ok(mut pred) = Predictor::new(&r, batches[0].start, policy.clone()) else {
        println!("建不出局面");
        return;
    };
    let inputs = Inputs::scan(&r, batches[0].start);
    println!("初始: 障碍 {} 士兵 {} 队伍 {:?}", inputs.obstacles.len(), inputs.pos.len(), inputs.team);
    for (i, b) in batches.iter().enumerate() {
        let tick = r.msgs[b.start].tick;
        let p = pred.predict(b, tick);
        let obs: Vec<_> = r.msgs[b.start..=b.end].to_vec();
        let v = compare_batch(&p.msgs, &obs, &policy, 8);
        let ok = if v.content_ok && v.order_ok { "OK " } else { "BAD" };
        println!("\n批 #{i} tick={tick} 发数={} [{ok}] 内容={} 顺序={} 种类={} 未产={} 折叠={}",
            b.shots.len(), v.content_ok, v.order_ok, v.kind_seq_ok, v.unproduced, v.collapsed);
        for s in &b.shots {
            println!("   shot owner={} x0={:.6} y0={:.6} flip={} text={:?}", s.owner, s.x0, s.y0, s.flip, trim(&s.text));
        }
        let kinds = |ms: &[gw2_rebuild::_scaffold::replaydiff::Msg]| ms
            .iter()
            .map(|m| if m.kind == "EffectInfo" {
                format!("Effect/{}", m.get_str("effect_type").unwrap_or_default())
            } else if m.kind == "GameStateInfo" {
                format!("GS/{}", m.get_str("game_state").unwrap_or_default())
            } else {
                m.kind.clone()
            })
            .collect::<Vec<_>>()
            .join(" ");
        println!("   回放: {}", kinds(&obs));
        println!("   我方: {}", kinds(&p.msgs));
        for d in &v.diffs {
            println!("   DIFF {} ours={} observed={} [{:?}]", d.path, trim(&d.ours), trim(&d.observed), d.class);
        }
        // 延迟段
        let mut deaths = Vec::new();
        let mut destroy = Vec::new();
        for m in &r.msgs[b.end + 1..b.tail_end] {
            if m.kind == "LifeInfo" {
                let alive = matches!(m.get("alive").map(|v| v.kind.clone()), Some(gw2_rebuild::_scaffold::replaydiff::Kind::Bool(true)));
                if !alive { deaths.push(m.get_i64("entity_id").unwrap_or(-1)); }
            }
            if m.kind == "DestroyObstacle" {
                destroy.push((m.get_f64("pos_x"), m.get_f64("pos_y")));
            }
        }
        deaths.sort_unstable();
        deaths.dedup();
        destroy.dedup();
        println!("   死亡 我方={:?} 回放={:?}", p.killed, deaths);
        // 几何探针：用回放声明的状态建掩膜，按真服务端步长 0.01 再算一次
        {
            use gw2_rebuild::_scaffold::replaydiff::{Inputs, StateMask};
            use gw2_rebuild::graphwar_shot_calculator::settle::{resolve, Target};
            let inputs = Inputs::scan(&r, b.start);
            let mut sm = StateMask::new();
            sm.apply(&r.msgs[..b.start]);
            let mut tg: Vec<Target> = inputs
                .pos
                .iter()
                .filter(|(id, _)| **id != b.shots[0].owner)
                .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
                .collect();
            tg.sort_by_key(|t| t.id);
            for shot in &b.shots {
                let out = resolve(&shot.text, shot.x0, shot.y0, 0.01, shot.flip,
                                  gw2_rebuild::graphwar_path_calculator::Mode::Normal, sm.mask(), &tg);
                println!("   probe01 stop=({:.6}, {:.6}) blocked={} killed={:?} 目标={:?}",
                         out.stop.0, out.stop.1, out.blocked, out.killed,
                         tg.iter().map(|t| (t.id, (t.x * 1000.0).round() / 1000.0, (t.y * 1000.0).round() / 1000.0)).collect::<Vec<_>>());
            }
        }
        if p.blocked_at.is_some() || !destroy.is_empty() {
            println!("   挖洞 我方={:?} 回放={:?}", p.blocked_at, destroy);
        }
    }
}
