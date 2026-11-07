//! **逐批**归因导出器（`docs/06-verification.md` §24 的归因表就是这么算出来的）。
//!
//! `examples/replay_diff.rs` 只导出**每份回放的第一个分歧**，够做趋势不够做归因：
//! 一张"剩余失败按根因分类"的表要求**每一批**都落进一个格子，并且各行加起来
//! 等于失败批数。本工具就是把 `run_file` 的那个循环原样跑一遍，但**逐批**吐 JSONL，
//! 归因（互斥分桶）交给 `tools/bit_exact_attrib.py` —— 分桶规则要能随时改，不该编译。
//!
//! ```text
//! cargo run --release --example replay_attrib -- \
//!     --dir <语料> --drive fire|conv --policy default|strict --json out.jsonl
//!
//!   --drive fire   `replaydiff::Predictor`（**批内 ≥2 发**一次 `Match::fire_batch`，单发批仍
//!                  走 `Match::fire`）—— 与 `replay_diff` 逐位同口径。2026-11-08 起口径变了，
//!                  见 `docs/06-verification.md` §39
//!   --drive conv   `diffconv::ConvPredictor`（模式感知 + 修正文本 + 爆炸点回代 + 皮肤特效）
//!   --policy default  `Policy::default()`（Ash/Spark 单侧排除）
//!   --policy strict   `diffconv::strict_policy()`（四类特效两侧对称比）
//!   --max-diffs <N>   每批最多记几条字段差异（默认 16，够看出"多个原因同时存在"）
//! ```
//!
//! 与两个既有入口的**关系**（不是替代）：
//! * 口径、投影、分块、折叠全部**调同一个** `replaydiff::compare_batch`，一行没抄；
//! * `content_ok` / `order_ok` / `kind_seq_ok` 的统计结果与 `replay_diff` **逐数相等**
//!   （`tools/bit_exact_attrib.py --check-baseline` 会核对）；
//! * 额外只多导三样**比较器内部已经算过、但没写进 JSON 的东西**：投影后的种类序列、
//!   逐条 diff 的数值幅度、延迟段的两组集合。
use gw2_rebuild::_scaffold::diffconv::{strict_policy, ConvPredictor};
use gw2_rebuild::_scaffold::replaydiff::{
    class_name, compare_batch, split_batches, DiffClass, Inputs, Msg, Policy, Predictor, Replay,
};
use gw2_rebuild::game::{Match, Submission};
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

/// `compare_batch` 的投影 1/2 + 折叠，一模一样（用来导出**投影后**的种类序列）。
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

/// 连续同 kind 的块序列（与 `compare_batch` 内的 `blocks` 同义）。
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

/// 与 `compare_batch` 内的 `normalize` **同一条**归一化（tick / 计数器换成 `#`）。
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

/// 一条消息属于哪个"可整组拿掉"的组（用来做 leave-one-out 归因）。
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

/// 拿掉若干组之后，两侧的多重集是否相等。
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

fn main() {
    let mut dir = gw2_rebuild::_scaffold::replaydiff::default_corpus_dir();
    let mut limit = usize::MAX;
    let mut max_diffs = 16usize;
    let mut drive = "fire".to_string();
    let mut policy_kind = "default".to_string();
    let mut json_out: Option<std::path::PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
            "--limit" => limit = args.next().expect("--limit 需要值").parse().unwrap(),
            "--max-diffs" => max_diffs = args.next().expect("--max-diffs 需要值").parse().unwrap(),
            "--drive" => {
                drive = args.next().expect("--drive 需要值");
                assert!(drive == "fire" || drive == "conv" || drive == "batch", "--drive 只认 fire|conv|batch");
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

    let mut names: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    names.sort();
    names.truncate(limit);

    let mut sink: Box<dyn Write> = match &json_out {
        Some(p) => Box::new(std::io::BufWriter::new(std::fs::File::create(p).expect("--json 打不开"))),
        None => Box::new(std::io::stdout()),
    };

    let mut files_ok = 0usize;
    let mut files_err = 0usize;
    let mut batches_total = 0usize;
    let mut content_ok = 0usize;

    for path in &names {
        let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let replay = match Replay::load(path) {
            Ok(r) => r,
            Err(e) => {
                files_err += 1;
                writeln!(sink, "{{\"file\":{},\"error\":{}}}", jstr(&name), jstr(&e)).unwrap();
                continue;
            }
        };
        let batches = split_batches(&replay);
        if batches.is_empty() {
            files_err += 1;
            writeln!(sink, "{{\"file\":{},\"error\":\"没有结算批\"}}", jstr(&name)).unwrap();
            continue;
        }
        files_ok += 1;
        let inputs = Inputs::scan(&replay, batches[0].start);

        // ---- 两个驱动 ----
        let mut pred_fire = if drive == "fire" {
            match Predictor::new(&replay, batches[0].start, policy.clone()) {
                Ok(p) => Some(p),
                Err(e) => {
                    writeln!(
                        sink,
                        "{{\"file\":{},\"batch\":-1,\"error\":{}}}",
                        jstr(&name),
                        jstr(&e)
                    )
                    .unwrap();
                    None
                }
            }
        } else {
            None
        };
        // 驱动 C：`Match::fire_batch`（一个批产出一整个结算批，`docs/141` §3 的分组）。
        // 2026-11-08 之后它与 `fire` 轴**都**按批结算，剩下的唯一差别是**模式时刻表**：
        // 本驱动用建局时读到的模式（不动），`Predictor` 按 `FunctionModeInfo` 逐批更新。
        // ⇒ 两条轴的差 = "局中改模式"这一项（`docs/06` §39）。
        let mut model_batch = if drive == "batch" {
            let soldiers: Vec<Target> = inputs
                .pos
                .iter()
                .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
                .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
                .collect();
            let base_tick = replay.msgs[batches[0].start].tick;
            let mut m = Match::new(inputs.room.clone(), inputs.obstacles.clone(), soldiers, (base_tick, 0));
            for id in inputs.targets.iter() {
                m.add_target(*id, 1);
            }
            for (id, g) in inputs.skins.iter() {
                let _ = m.note_skin_graphics(*id, g);
            }
            Some(m)
        } else {
            None
        };
        let mut pred_conv = if drive == "conv" {
            let soldiers: Vec<Target> = inputs
                .pos
                .iter()
                .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
                .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
                .collect();
            Some(ConvPredictor::new(
                &inputs.obstacles,
                soldiers,
                gw2_rebuild::_scaffold::diffconv::mode_of(&inputs.room.function_mode),
            ))
        } else {
            None
        };
        // conv 驱动要从**文件开头**扫皮肤/模式（加入链里的 SkinGraphicsInfo 是输入）
        let mut conv_cursor = 0usize;

        for (idx, b) in batches.iter().enumerate() {
            if let Some(pc) = pred_conv.as_mut() {
                for m in &replay.msgs[conv_cursor..b.start] {
                    match m.kind.as_str() {
                        "FunctionModeInfo" => {
                            if let Some(s) = m.get_str("function_mode") {
                                pc.mode = gw2_rebuild::_scaffold::diffconv::mode_of(&s);
                            }
                        }
                        "SkinGraphicsInfo" => {
                            if let (Some(id), Some(s)) = (m.get_i64("entity_id"), m.get_str("graphics_str")) {
                                pc.skin.insert(id, s);
                            }
                        }
                        _ => {}
                    }
                }
                conv_cursor = b.start;
            }
            if b.shots.is_empty() {
                continue;
            }
            let tick = replay.msgs[b.start].tick;
            let mut killed: Vec<i64> = Vec::new();
            let mut blocked_at: Option<(f64, f64)> = None;

            let mut invalid = 0usize;
            let ours: Vec<Msg> = if let Some(p) = pred_fire.as_mut() {
                let p = p.predict(b, tick);
                killed.extend(p.killed.iter().copied());
                blocked_at = p.blocked_at;
                // 我方 emitter 产出非法 JSON 时 `replay_diff` 把该批直接判失败，这里照样记
                invalid = p.invalid.len();
                p.msgs
            } else if let Some(mb) = model_batch.as_mut() {
                mb.tick_base = (tick, 0);
                let subs: Vec<Submission> = b
                    .shots
                    .iter()
                    .map(|s| Submission {
                        owner_id: s.owner,
                        function_text: &s.text,
                        x0: s.x0,
                        y0: s.y0,
                        flip: s.flip,
                    })
                    .collect();
                let wire = mb.fire_batch(&subs, 0);
                let mut all: Vec<Msg> = Vec::new();
                for w in &wire {
                    match Msg::parse_wire(w) {
                        Ok(m) => {
                            match m.kind.as_str() {
                                "LifeQueuedDeath" => {
                                    if let Some(id) = m.get_i64("entity_id") {
                                        killed.push(id);
                                    }
                                }
                                "QueueDestroyObstacle" => {
                                    blocked_at = Some((
                                        m.get_f64("pos_x").unwrap_or(f64::NAN),
                                        m.get_f64("pos_y").unwrap_or(f64::NAN),
                                    ));
                                }
                                _ => {}
                            }
                            all.push(m);
                        }
                        Err(_) => invalid += 1,
                    }
                }
                all
            } else if let Some(pc) = pred_conv.as_mut() {
                let mut all: Vec<Msg> = Vec::new();
                for shot in &b.shots {
                    let (wire, o) = pc.fire(shot.owner, &shot.text, shot.x0, shot.y0, shot.flip, tick);
                    killed.extend(o.killed.iter().copied());
                    if o.blocked {
                        blocked_at = Some(o.stop);
                    }
                    for w in &wire {
                        if let Ok(m) = Msg::parse_wire(w) {
                            all.push(m);
                        }
                    }
                }
                all
            } else {
                Vec::new()
            };

            let observed: Vec<Msg> = replay.msgs[b.start..=b.end].to_vec();
            let v = compare_batch(&ours, &observed, &policy, max_diffs);
            batches_total += 1;
            if v.content_ok {
                content_ok += 1;
            }

            let obs_p = project_observed(&observed, &policy);
            let our_p = project_ours(&ours, &policy);
            // 整批（跨块）多重集是否相等 —— 用来把"结构/分组错"与"值错"分开：
            // 整批多重集相等而块种类序列不等 ⇒ 只差**分组**，内容一条不差。
            let bm_equal = multiset(obs_p.iter().map(|m| normalize(m, &policy)).collect())
                == multiset(our_p.iter().map(|m| normalize(m, &policy)).collect());
            let mut ok_ = block_kinds(&obs_p);
            let mut uk_ = block_kinds(&our_p);
            ok_.sort();
            uk_.sort();
            let kind_multiset_equal = ok_ == uk_;
            // leave-one-out：拿掉某一整组后多重集相等 ⇒ 差异**全部**落在那一组
            let loo_state = loo_equal(&obs_p, &our_p, &policy, &["state"]);
            let loo_deff = loo_equal(&obs_p, &our_p, &policy, &["deff"]);
            let loo_expl = loo_equal(&obs_p, &our_p, &policy, &["expl"]);
            let loo_state_deff = loo_equal(&obs_p, &our_p, &policy, &["state", "deff"]);
            let loo_struct = loo_equal(&obs_p, &our_p, &policy, &["state", "deff", "expl"]);

            // 延迟段：与 `run_file` 同一段（`b.end+1 .. 下一批 end+1`）
            let next_end = batches.get(idx + 1).map(|n| n.end + 1).unwrap_or(replay.msgs.len());
            let mut obs_deaths: Vec<i64> = Vec::new();
            let mut obs_destroy: Vec<(f64, f64)> = Vec::new();
            for m in &replay.msgs[b.end + 1..next_end.min(replay.msgs.len())] {
                match m.kind.as_str() {
                    "LifeInfo" => {
                        let dead = matches!(m.get("alive").map(|x| x.kind.clone()),
                            Some(gw2_rebuild::_scaffold::replaydiff::Kind::Bool(false)));
                        if dead {
                            if let Some(id) = m.get_i64("entity_id") {
                                obs_deaths.push(id);
                            }
                        }
                    }
                    "DestroyObstacle" => obs_destroy.push((
                        m.get_f64("pos_x").unwrap_or(f64::NAN),
                        m.get_f64("pos_y").unwrap_or(f64::NAN),
                    )),
                    _ => {}
                }
            }
            obs_deaths.sort_unstable();
            obs_deaths.dedup();

            // 我方在这一批窗口里发的 DestroyObstacle（延时到期的那条）
            let our_destroy_in_window = ours.iter().filter(|m| m.kind == "DestroyObstacle").count();
            let obs_destroy_in_window = observed.iter().filter(|m| m.kind == "DestroyObstacle").count();

            let diffs: Vec<String> = v
                .diffs
                .iter()
                .map(|d| {
                    let (delta, ulp) = match d.class {
                        DiffClass::NumericValue { ulp, delta } => (jnum(delta), ulp.to_string()),
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
                loo_state,
                loo_deff,
                loo_expl,
                loo_state_deff,
                loo_struct,
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
    eprintln!(
        "驱动={drive} 口径={policy_kind}：文件 {}（读失败 {}）；结算批 {}；内容命中 {}",
        files_ok, files_err, batches_total, content_ok
    );
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
