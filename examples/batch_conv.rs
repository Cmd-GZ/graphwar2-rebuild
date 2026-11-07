//! 多发批**结构接线**的量化入口（`docs/06-verification.md` §25）。
//!
//! 同一份语料、同一个 `compare_batch`、同一个 `diffconv::ConvPredictor`，**只换驱动方式**：
//!
//! ```text
//! cargo run --release --offline --example batch_conv -- \
//!     --dir <语料> --drive fire|batch|both --policy default|strict --json <out.jsonl>
//!
//!   --drive fire   逐发驱动：每发调一次 `ConvPredictor::fire`，把报文拼起来（= 接线**前**）
//!   --drive batch  按批驱动：每批调一次 `ConvPredictor::fire_batch`（= 接线**后**，`docs/141` §3）
//!   --drive both   两种都跑，逐批对照（默认）
//! ```
//!
//! 为什么单开一个例子而不是改 `examples/replay_attrib.rs`：那个文件本轮**只许加不许改**
//! （任务书的允许清单），而"before/after 必须来自同一套口径"又不能靠两套代码各自记账。
//! 所以这里的投影 / 折叠 / 分块 / 归一化 / leave-one-out 是 `replay_attrib` 的**逐行副本**，
//! 并用一条**机械校验**兜住"抄错了"这件事：
//!
//! > `--drive fire` 吐的 JSONL 与
//! > `replay_attrib --drive conv --policy strict` 的产物**逐字节相同**（`diff` 为空）。
//!
//! 也就是说：`--drive batch` 与 `--drive fire` 的差别**只可能**来自驱动方式本身。
//!
//! 输出格式与 `examples/replay_attrib.rs` 一致，所以 `tools/bit_exact_attrib.py --json <out>`
//! 可以直接对**接线后**的产物做归因分桶（桶名含义见那个脚本头部表）。
use gw2_rebuild::_scaffold::diffconv::{strict_policy, ConvPredictor};
use gw2_rebuild::_scaffold::replaydiff::{
    class_name, compare_batch, split_batches, DiffClass, Inputs, Msg, Policy, Replay,
};
use gw2_rebuild::graphwar_shot_calculator::settle::Target;
use std::io::Write;

fn kind_of(m: &Msg) -> String {
    if m.kind == "EffectInfo" {
        format!(
            "EffectInfo/{}",
            m.get_str("effect_type").unwrap_or_default()
        )
    } else {
        m.kind.clone()
    }
}

/// `compare_batch` 的投影 1/2 + 折叠，与 `replay_attrib` 一模一样。
fn project_observed(observed: &[Msg], policy: &Policy) -> Vec<Msg> {
    let mut out: Vec<Msg> = Vec::new();
    for m in observed {
        if policy.is_unproduced(m) {
            continue;
        }
        if policy.collapse_duplicates {
            if let Some(prev) = out.last() {
                if prev.canonical() == m.canonical() {
                    continue;
                }
            }
        }
        out.push(m.clone());
    }
    out
}

fn project_ours(ours: &[Msg], policy: &Policy) -> Vec<Msg> {
    ours.iter().filter(|m| !policy.is_unobserved(m)).cloned().collect()
}

fn block_kinds(ms: &[Msg]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in ms {
        let k = kind_of(m);
        if out.last().map(|p| p == &k).unwrap_or(false) {
            continue;
        }
        out.push(k);
    }
    out
}

fn normalize(m: &Msg, policy: &Policy) -> String {
    let mut parts = Vec::with_capacity(m.body.len());
    for f in &m.body {
        let skip = policy.tick_fields.contains(&f.name.as_str())
            || policy.counter_fields.contains(&f.name.as_str());
        if skip {
            parts.push(format!("\"{}\":#", f.name));
        } else {
            parts.push(format!("\"{}\":{}", f.name, f.val.text()));
        }
    }
    format!("{{\"{}\":{{{}}}}}", m.kind, parts.join(","))
}

fn multiset(xs: Vec<String>) -> std::collections::BTreeMap<String, usize> {
    let mut m = std::collections::BTreeMap::new();
    for x in xs {
        *m.entry(x).or_insert(0) += 1;
    }
    m
}

fn grp(m: &Msg) -> &'static str {
    match m.kind.as_str() {
        "GameStateInfo" => "state",
        "FunctionInfo" => "finfo",
        "FunctionPoints" => "fpts",
        "FunctionActive" => "factive",
        "EffectInfo" => match m.get_str("effect_type").unwrap_or_default().as_str() {
            "ExplosionEffect" => "expl",
            "DeathEffect" | "AshEffect" | "SparkEffect" => "deff",
            _ => "eff",
        },
        _ => "other",
    }
}

fn loo_equal(obs_p: &[Msg], our_p: &[Msg], policy: &Policy, drop: &[&str]) -> bool {
    let f = |ms: &[Msg]| {
        let mut v: Vec<String> = ms
            .iter()
            .filter(|m| !drop.contains(&grp(m)))
            .map(|m| normalize(m, policy))
            .collect();
        v.sort();
        v
    };
    f(obs_p) == f(our_p)
}

fn jstr(s: &str) -> String {
    gw2_rebuild::_scaffold::replaydiff::wire::quote(s)
}

fn jnum(v: f64) -> String {
    if v.is_finite() {
        format!("{v:?}")
    } else {
        "null".into()
    }
}

/// 回放里这一批用的采样模式（只看批之前的最后一个 `FunctionModeInfo`）。
fn replay_mode_note(r: &Replay, upto: usize) -> bool {
    let mut mode = false;
    for m in &r.msgs[..upto] {
        if m.kind == "FunctionModeInfo" {
            mode = m.get_str("function_mode").as_deref() == Some("DiffEqFunction");
        }
    }
    mode
}

/// 一批的预测（两种驱动**唯一**的分歧点）。
fn predict(
    pc: &mut ConvPredictor,
    shots: &[gw2_rebuild::_scaffold::replaydiff::ShotInput],
    tick: i64,
    batch_driver: bool,
) -> (Vec<Msg>, Vec<i64>, Option<(f64, f64)>, usize) {
    let mut all: Vec<Msg> = Vec::new();
    let mut killed: Vec<i64> = Vec::new();
    let mut blocked_at: Option<(f64, f64)> = None;
    let mut invalid = 0usize;
    let mut eat = |wire: Vec<String>, outs: Vec<gw2_rebuild::_scaffold::diffconv::ShotOutcome>| {
        for o in &outs {
            killed.extend(o.killed.iter().copied());
            if o.blocked {
                blocked_at = Some(o.stop);
            }
        }
        for w in &wire {
            match Msg::parse_wire(w) {
                Ok(m) => all.push(m),
                Err(_) => invalid += 1,
            }
        }
    };
    if batch_driver {
        let (wire, outs) = pc.fire_batch(shots, tick);
        eat(wire, outs);
    } else {
        for s in shots {
            let (wire, o) = pc.fire(s.owner, &s.text, s.x0, s.y0, s.flip, tick);
            eat(wire, vec![o]);
        }
    }
    (all, killed, blocked_at, invalid)
}

#[derive(Default, Clone, Copy)]
struct Tally {
    files_ok: usize,
    files_err: usize,
    batches: usize,
    content_ok: usize,
    kind_ok: usize,
    order_ok: usize,
    multi: usize,
    multi_kind_ok: usize,
    multi_content_ok: usize,
    single: usize,
    single_kind_ok: usize,
    single_content_ok: usize,
}

impl Tally {
    fn pct(a: usize, b: usize) -> String {
        if b == 0 {
            "-".into()
        } else {
            format!("{:.1}%", 100.0 * a as f64 / b as f64)
        }
    }

    fn print(&self, title: &str) {
        println!("--- {title} ---");
        println!(
            "  文件 {}（读失败 {}）；结算批 {}",
            self.files_ok, self.files_err, self.batches
        );
        println!(
            "  种类序列命中 {:>5} / {}  ({})",
            self.kind_ok,
            self.batches,
            Self::pct(self.kind_ok, self.batches)
        );
        println!(
            "  内容命中     {:>5} / {}  ({})",
            self.content_ok,
            self.batches,
            Self::pct(self.content_ok, self.batches)
        );
        println!(
            "  多发批（≥2 发）{:>4}：种类 {:>4} / {}  ({})  内容 {:>4} / {}  ({})",
            self.multi,
            self.multi_kind_ok,
            self.multi,
            Self::pct(self.multi_kind_ok, self.multi),
            self.multi_content_ok,
            self.multi,
            Self::pct(self.multi_content_ok, self.multi)
        );
        println!(
            "  单发批        {:>4}：种类 {:>4} / {}  ({})  内容 {:>4} / {}  ({})",
            self.single,
            self.single_kind_ok,
            self.single,
            Self::pct(self.single_kind_ok, self.single),
            self.single_content_ok,
            self.single,
            Self::pct(self.single_content_ok, self.single)
        );
    }
}

fn main() {
    let mut dir = gw2_rebuild::_scaffold::replaydiff::default_corpus_dir();
    let mut limit = usize::MAX;
    let mut max_diffs = 16usize;
    let mut drive = "both".to_string();
    let mut policy_kind = "strict".to_string();
    let mut json_out: Option<std::path::PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
            "--limit" => limit = args.next().expect("--limit 需要值").parse().unwrap(),
            "--max-diffs" => max_diffs = args.next().expect("--max-diffs 需要值").parse().unwrap(),
            "--drive" => {
                drive = args.next().expect("--drive 需要值");
                assert!(
                    drive == "fire" || drive == "batch" || drive == "both",
                    "--drive 只认 fire|batch|both"
                );
            }
            "--policy" => {
                policy_kind = args.next().expect("--policy 需要值");
                assert!(policy_kind == "default" || policy_kind == "strict", "--policy 只认 default|strict");
            }
            "--json" => json_out = Some(args.next().expect("--json 需要值").into()),
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
    let policy = if policy_kind == "default" { Policy::default() } else { strict_policy() };
    let both = drive == "both";
    // `both` 时 JSONL 只能装一种驱动：默认装**接线后**的那一列
    if both && json_out.is_some() {
        eprintln!("提示：--drive both 与 --json 同用时，JSONL 只写 batch（接线后）那一列");
    }
    let json_batch = drive != "fire";

    let mut names: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    names.sort();
    names.truncate(limit);

    let mut sink: Box<dyn Write> = match &json_out {
        Some(p) => Box::new(std::io::BufWriter::new(
            std::fs::File::create(p).expect("--json 打不开"),
        )),
        None => Box::new(std::io::stdout()),
    };

    let mut tallies: [Tally; 2] = [Tally::default(), Tally::default()];
    // 逐批对照（只统计"两边判定不同"的批，用来回答"有没有变差的批次"）
    let mut regressed: Vec<(String, usize, usize)> = Vec::new();
    let mut gained: Vec<(String, usize, usize)> = Vec::new();

    for path in &names {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let replay = match Replay::load(path) {
            Ok(r) => r,
            Err(e) => {
                for t in tallies.iter_mut() {
                    t.files_err += 1;
                }
                writeln!(sink, "{{\"file\":{},\"error\":{}}}", jstr(&name), jstr(&e)).unwrap();
                continue;
            }
        };
        let batches = split_batches(&replay);
        if batches.is_empty() {
            for t in tallies.iter_mut() {
                t.files_err += 1;
            }
            writeln!(sink, "{{\"file\":{},\"error\":\"没有结算批\"}}", jstr(&name)).unwrap();
            continue;
        }
        let inputs = Inputs::scan(&replay, batches[0].start);
        let soldiers: Vec<Target> = inputs
            .pos
            .iter()
            .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
            .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
            .collect();
        let new_pred = || {
            ConvPredictor::new(
                &inputs.obstacles,
                soldiers.clone(),
                gw2_rebuild::_scaffold::diffconv::mode_of(&inputs.room.function_mode),
            )
        };
        // 两个预测器各自独立（状态自带，不能共用）
        let mut preds = [new_pred(), new_pred()];
        let mut cursors = [0usize, 0usize];
        for t in tallies.iter_mut() {
            t.files_ok += 1;
        }

        for (idx, b) in batches.iter().enumerate() {
            // 皮肤 / 模式是**输入**（加入链里的 `SkinGraphicsInfo`），从文件开头扫
            for k in 0..2 {
                for m in &replay.msgs[cursors[k]..b.start] {
                    match m.kind.as_str() {
                        "FunctionModeInfo" => {
                            if let Some(s) = m.get_str("function_mode") {
                                preds[k].mode = gw2_rebuild::_scaffold::diffconv::mode_of(&s);
                            }
                        }
                        "SkinGraphicsInfo" => {
                            if let (Some(id), Some(s)) =
                                (m.get_i64("entity_id"), m.get_str("graphics_str"))
                            {
                                preds[k].skin.insert(id, s);
                            }
                        }
                        _ => {}
                    }
                }
                cursors[k] = b.start;
            }
            if b.shots.is_empty() {
                continue;
            }
            let tick = replay.msgs[b.start].tick;
            let observed: Vec<Msg> = replay.msgs[b.start..=b.end].to_vec();

            // k = 0 逐发（接线前）/ k = 1 按批（接线后）
            let mut verdicts = Vec::new();
            for (k, t) in tallies.iter_mut().enumerate() {
                let is_batch = k == 1;
                if !both && is_batch != json_batch {
                    // 只跑一种驱动时跳过另一种：占位，后面按 actual 取
                    verdicts.push(None);
                    continue;
                }
                let (ours, killed, blocked_at, invalid) =
                    predict(&mut preds[k], &b.shots, tick, is_batch);
                let v = compare_batch(&ours, &observed, &policy, max_diffs);
                t.batches += 1;
                if v.kind_seq_ok {
                    t.kind_ok += 1;
                }
                if v.content_ok {
                    t.content_ok += 1;
                }
                if v.order_ok {
                    t.order_ok += 1;
                }
                if b.shots.len() > 1 {
                    t.multi += 1;
                    t.multi_kind_ok += v.kind_seq_ok as usize;
                    t.multi_content_ok += v.content_ok as usize;
                } else {
                    t.single += 1;
                    t.single_kind_ok += v.kind_seq_ok as usize;
                    t.single_content_ok += v.content_ok as usize;
                }

                let obs_deaths = {
                    let next_end = batches
                        .get(idx + 1)
                        .map(|n| n.end + 1)
                        .unwrap_or(replay.msgs.len());
                    let mut v: Vec<i64> = Vec::new();
                    for m in &replay.msgs[b.end + 1..next_end.min(replay.msgs.len())] {
                        if m.kind == "LifeInfo"
                            && matches!(
                                m.get("alive").map(|x| x.kind.clone()),
                                Some(gw2_rebuild::_scaffold::replaydiff::Kind::Bool(false))
                            )
                        {
                            if let Some(id) = m.get_i64("entity_id") {
                                v.push(id);
                            }
                        }
                    }
                    v.sort_unstable();
                    v.dedup();
                    v
                };
                let obs_destroy: Vec<(f64, f64)> = replay.msgs[b.end + 1
                    ..batches
                        .get(idx + 1)
                        .map(|n| n.end + 1)
                        .unwrap_or(replay.msgs.len())
                        .min(replay.msgs.len())]
                    .iter()
                    .filter(|m| m.kind == "DestroyObstacle")
                    .map(|m| {
                        (
                            m.get_f64("pos_x").unwrap_or(f64::NAN),
                            m.get_f64("pos_y").unwrap_or(f64::NAN),
                        )
                    })
                    .collect();
                let our_destroy_in_window = ours.iter().filter(|m| m.kind == "DestroyObstacle").count();
                let obs_destroy_in_window = observed.iter().filter(|m| m.kind == "DestroyObstacle").count();

                // 只有被选中写 JSONL 的那一列才落盘
                let selected = if both { k == 1 } else { is_batch == json_batch };
                verdicts.push(Some((v.kind_seq_ok, v.content_ok)));
                if selected {
                    let obs_p = project_observed(&observed, &policy);
                    let our_p = project_ours(&ours, &policy);
                    let bm_equal = multiset(obs_p.iter().map(|m| normalize(m, &policy)).collect())
                        == multiset(our_p.iter().map(|m| normalize(m, &policy)).collect());
                    let mut ok_ = block_kinds(&obs_p);
                    let mut uk_ = block_kinds(&our_p);
                    ok_.sort();
                    uk_.sort();
                    let kind_multiset_equal = ok_ == uk_;
                    let diffs: Vec<String> = v
                        .diffs
                        .iter()
                        .map(|d| {
                            let (delta, ulp) = match d.class {
                                DiffClass::NumericValue { ulp, delta } => {
                                    (jnum(delta), ulp.to_string())
                                }
                                _ => ("null".to_string(), "null".to_string()),
                            };
                            format!(
                                "{{\"path\":{},\"kind\":{},\"class\":{},\"ours\":{},\"observed\":{},\"delta\":{},\"ulp\":{}}}",
                                jstr(&d.path),
                                jstr(&d.kind),
                                jstr(&class_name(&d.class)),
                                jstr(&d.ours),
                                jstr(&d.observed),
                                delta,
                                ulp
                            )
                        })
                        .collect();
                    writeln!(
                        sink,
                        concat!(
                            "{{\"file\":{},\"batch\":{},\"tick\":{},\"shots\":{},\"owners\":[{}],",
                            "\"mode\":{},\"content_ok\":{},\"order_ok\":{},\"kind_seq_ok\":{},\"counter_ok\":{},",
                            "\"our_kinds\":[{}],\"obs_kinds\":[{}],\"bm_equal\":{},\"kind_multiset_equal\":{},",
                            "\"loo\":{{\"state\":{},\"deff\":{},\"expl\":{},\"state_deff\":{},\"struct\":{}}},",
                            "\"diffs\":[{}],\"missing\":{},\"extra\":{},\"collapsed\":{},\"invalid\":{},",
                            "\"killed\":[{}],\"obs_deaths\":[{}],",
                            "\"blocked\":{},\"obs_destroy\":[{}],",
                            "\"our_destroy_in_window\":{},\"obs_destroy_in_window\":{}}}"
                        ),
                        jstr(&name),
                        idx,
                        tick,
                        b.shots.len(),
                        b.shots.iter().map(|s| s.owner.to_string()).collect::<Vec<_>>().join(","),
                        jstr(if replay_mode_note(&replay, b.start) { "DiffEq" } else { "Normal" }),
                        v.content_ok,
                        v.order_ok,
                        v.kind_seq_ok,
                        v.counter_ok,
                        our_p.iter().map(|m| jstr(&kind_of(m))).collect::<Vec<_>>().join(","),
                        obs_p.iter().map(|m| jstr(&kind_of(m))).collect::<Vec<_>>().join(","),
                        bm_equal,
                        kind_multiset_equal,
                        loo_equal(&obs_p, &our_p, &policy, &["state"]),
                        loo_equal(&obs_p, &our_p, &policy, &["deff"]),
                        loo_equal(&obs_p, &our_p, &policy, &["expl"]),
                        loo_equal(&obs_p, &our_p, &policy, &["state", "deff"]),
                        loo_equal(&obs_p, &our_p, &policy, &["state", "deff", "expl"]),
                        diffs.join(","),
                        v.missing.len(),
                        v.extra.len(),
                        v.collapsed,
                        invalid,
                        killed.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","),
                        obs_deaths.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","),
                        blocked_at
                            .map(|(x, y)| format!("[{},{}]", jnum(x), jnum(y)))
                            .unwrap_or_else(|| "null".into()),
                        obs_destroy
                            .iter()
                            .map(|(x, y)| format!("[{},{}]", jnum(*x), jnum(*y)))
                            .collect::<Vec<_>>()
                            .join(","),
                        our_destroy_in_window,
                        obs_destroy_in_window
                    )
                    .unwrap();
                }
            }
            // 逐批对照：种类序列或内容从"命中"翻成"不命中"就是**变差**
            if both {
                if let (Some((k0, c0)), Some((k1, c1))) = (verdicts[0], verdicts[1]) {
                    let before = k0 as usize + c0 as usize;
                    let after = k1 as usize + c1 as usize;
                    if after < before {
                        regressed.push((name.clone(), idx, b.shots.len()));
                    } else if after > before {
                        gained.push((name.clone(), idx, b.shots.len()));
                    }
                }
            }
        }
    }
    drop(sink);

    if both {
        tallies[0].print("驱动 A：逐发 `fire`（接线前）");
        tallies[1].print("驱动 B：按批 `fire_batch`（接线后，`docs/141` §3）");
        println!(
            "--- 逐批对照 ---\n  变好 {} 批 / 变差 {} 批",
            gained.len(),
            regressed.len()
        );
        if !regressed.is_empty() {
            println!("  变差的批（必须为空，否则规则不该无条件应用）：");
            for (f, i, n) in regressed.iter().take(20) {
                println!("    {f} #{i}（{n} 发）");
            }
        }
        let multi_bad: usize = regressed.iter().filter(|(_, _, n)| *n == 1).count();
        if multi_bad > 0 {
            println!("  ⚠ 其中单发批 {} 个 —— 单发批不许变差", multi_bad);
        }
    } else {
        tallies[if json_batch { 1 } else { 0 }]
            .print(if json_batch { "驱动 B：按批 `fire_batch`" } else { "驱动 A：逐发 `fire`" });
    }
}
