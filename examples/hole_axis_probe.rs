//! **逐发**挖洞点探针（`docs/06-verification.md` §24.7 / `dispatch/14-hole-axis` 的归因用手）。
//!
//! `replay_attrib` 只导**每批最后一个 blocked 的停点**（多打批会互相覆盖），
//! 而归因"我方挖洞点为什么不在回放里"必须要**逐发**的停点、停点前后的采样点、
//! 以及我方自己的爆炸点。本工具照 `diffconv::ConvPredictor::fire` 的同一套步骤
//! （模式取自回放、修正文本、掩膜挖洞、死者从可命中集合里去掉）逐发重跑一遍，
//! 但把几何量全导出来，配 `tools/hole_axis_attrib.py` 做互斥分桶。
//!
//! ```text
//! cargo run --release --offline --example hole_axis_probe -- \
//!     --dir <语料> --json out.jsonl [--limit N]
//! ```
//!
//! 与 `replay_attrib --drive conv` 的**自校验**：单发批的 `blocked` 点必须逐位相等
//! （`tools/hole_axis_attrib.py --check` 会核对 291 / 66 两个数）。
use gw2_rebuild::_scaffold::diffconv::{
    corrected_function, explosion_point, mode_of, DESTROY_DELAY_TICKS,
};
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Inputs, Replay};
use gw2_rebuild::graphwar_shot_calculator::settle::{resolve, Target};
use gw2_rebuild::plane::{empty_mask, rasterize};
use std::io::Write;

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
    let mut json_out: Option<std::path::PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
            "--limit" => limit = args.next().expect("--limit 需要值").parse().unwrap(),
            "--json" => json_out = Some(args.next().expect("--json 需要值").into()),
            other => {
                eprintln!("未知参数 {other}");
                std::process::exit(2);
            }
        }
    }

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

    let mut shots_total = 0usize;
    let mut blocked_total = 0usize;

    for path in &names {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let replay = match Replay::load(path) {
            Ok(r) => r,
            Err(e) => {
                writeln!(sink, "{{\"file\":{},\"error\":{}}}", jstr(&name), jstr(&e)).unwrap();
                continue;
            }
        };
        let batches = split_batches(&replay);
        if batches.is_empty() {
            writeln!(sink, "{{\"file\":{},\"error\":\"没有结算批\"}}", jstr(&name)).unwrap();
            continue;
        }
        let inputs = Inputs::scan(&replay, batches[0].start);

        // ---- 我方状态（与 `ConvPredictor` 同构：掩膜 + 存活士兵 + 模式）----
        let mut mask = empty_mask();
        for (x, y, r) in &inputs.obstacles {
            rasterize(&mut mask, *x, *y, *r, 0x00);
        }
        let mut soldiers: Vec<Target> = inputs
            .pos
            .iter()
            .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
            .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
            .collect();
        soldiers.sort_by_key(|t| t.id);
        let mut mode = mode_of(&inputs.room.function_mode);
        let mut cursor = 0usize;

        for (bi, b) in batches.iter().enumerate() {
            // 模式：批之前最后一个 `FunctionModeInfo`（与 `replay_attrib` 的 conv 驱动同）
            for m in &replay.msgs[cursor..b.start] {
                if m.kind == "FunctionModeInfo" {
                    if let Some(s) = m.get_str("function_mode") {
                        mode = mode_of(&s);
                    }
                }
            }
            cursor = b.start;
            if b.shots.is_empty() {
                continue;
            }
            let tick = replay.msgs[b.start].tick;
            for (si, shot) in b.shots.iter().enumerate() {
                let others: Vec<Target> = soldiers
                    .iter()
                    .filter(|t| {
                        !(t.id == shot.owner)
                            && !((t.x - shot.x0).abs() < 1e-12 && (t.y - shot.y0).abs() < 1e-12)
                    })
                    .copied()
                    .collect();
                let out = resolve(
                    &shot.text,
                    shot.x0,
                    shot.y0,
                    gw2_rebuild::SERVER_STEP,
                    shot.flip,
                    mode,
                    &mask,
                    &others,
                );
                let corrected = corrected_function(&shot.text, shot.x0, shot.y0, shot.flip, mode);
                let expl = explosion_point(&corrected, out.stop, shot.flip, mode);
                // 与 `game::hole_point`（生产代码）同一条规则：`to_text(parse(f))`（含补括号）
                // + 3 位小数后缀，在 `x = ±stop.x` 处重算。DiffEq 局不重算。
                let hole = if mode == gw2_rebuild::graphwar_path_calculator::Mode::Ode {
                    out.stop
                } else {
                    let toks = gw2_rebuild::math_function::parse(&shot.text);
                    if toks.is_empty() {
                        out.stop
                    } else {
                        let mut printed = shot.text.clone();
                        let mut depth = 0i64;
                        for ch in shot.text.chars() {
                            match ch {
                                '(' => depth += 1,
                                ')' => depth = (depth - 1).max(0),
                                _ => {}
                            }
                        }
                        for _ in 0..depth {
                            printed.push(')');
                        }
                        printed.push_str(&gw2_rebuild::graphwar_shot_calculator::correction_text(
                            &toks, shot.x0, shot.y0, shot.flip,
                        ));
                        let pt = gw2_rebuild::math_function::parse(&printed);
                        let x = if shot.flip { -out.stop.0 } else { out.stop.0 };
                        let y = if pt.is_empty() {
                            f64::NAN
                        } else {
                            gw2_rebuild::math_function::eval(&pt, x, 0.0, 0.0)
                        };
                        if y.is_finite() { (out.stop.0, y) } else { out.stop }
                    }
                };
                let tail: Vec<String> = out
                    .points
                    .iter()
                    .rev()
                    .take(4)
                    .rev()
                    .map(|(x, y)| format!("[{},{}]", jnum(*x), jnum(*y)))
                    .collect();
                let stop_index = if out.points.is_empty() {
                    -1i64
                } else {
                    (out.points.len() - 1) as i64
                };
                shots_total += 1;
                if out.blocked {
                    blocked_total += 1;
                }
                // 回放这一发的 `end_x`（**回放的弹道停点**；y 不发布，所以只有 x）
                let obs_end_x: Vec<f64> = replay.msgs[b.start..=b.end]
                    .iter()
                    .filter(|m| m.kind == "FunctionPoints")
                    .filter_map(|m| m.get_f64("end_x"))
                    .collect();
                let obs_ex = obs_end_x.get(si).copied();
                writeln!(
                    sink,
                    concat!(
                        "{{\"file\":{},\"batch\":{},\"tick\":{},\"shot\":{},\"shots\":{},",
                        "\"owner\":{},\"x0\":{},\"y0\":{},\"flip\":{},",
                        "\"mode\":{},\"text\":{},\"corrected\":{},",
                        "\"stop_x\":{},\"stop_y\":{},\"blocked\":{},",
                        "\"expl_x\":{},\"expl_y\":{},\"hole_x\":{},\"hole_y\":{},",
                        "\"obs_end_x\":{},\"n_points\":{},\"tail\":[{}],",
                        "\"killed\":[{}]}}"
                    ),
                    jstr(&name),
                    bi,
                    tick,
                    si,
                    b.shots.len(),
                    shot.owner,
                    jnum(shot.x0),
                    jnum(shot.y0),
                    shot.flip,
                    jstr(if mode == gw2_rebuild::graphwar_path_calculator::Mode::Normal {
                        "Normal"
                    } else {
                        "DiffEq"
                    }),
                    jstr(&shot.text),
                    jstr(&corrected),
                    jnum(out.stop.0),
                    jnum(out.stop.1),
                    out.blocked,
                    jnum(expl.0),
                    jnum(expl.1),
                    jnum(hole.0),
                    jnum(hole.1),
                    obs_ex.map(jnum).unwrap_or_else(|| "null".into()),
                    stop_index + 1,
                    tail.join(","),
                    out.killed
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                )
                .unwrap();
                if out.blocked {
                    rasterize(&mut mask, out.stop.0, out.stop.1, 0.5, 0xFF);
                }
                if !out.killed.is_empty() {
                    soldiers.retain(|t| !out.killed.contains(&t.id));
                }
                let _ = DESTROY_DELAY_TICKS;
            }
        }
    }
    eprintln!("逐发 {shots_total}（其中 blocked {blocked_total}）");
}
