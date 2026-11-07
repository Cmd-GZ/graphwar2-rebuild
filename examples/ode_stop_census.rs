//! `Ode` 局**停止判据**普查（`dispatch/29` 的验收 ①）：逐**发**对比"球走多远、为什么停"。
//!
//! 这个工具不判对错，只把两侧的数摆出来：
//! * 我方采样器的**停点**（`graphwar_path_calculator::calculate` 的最后一个点）；
//! * 回放报出的**停点**（`FunctionPoints.end_x` / `EffectInfo(Explosion).pos_x`）；
//! * 回放停点落在我方点列的第几个（`obs_idx_in_ours`）；
//! * 我方点列里**第一个落进某个障碍圆内的点**（三种判据：`d<r` / `d<=r` / `d<r+0.5`）
//!   以及光栅化掩膜的判据 —— 用来区分"停早了"和"撞东西停了"。
//!
//! ```text
//! cargo run --release --offline -q --example ode_stop_census -- --dir <语料> --json out.jsonl
//! ```
//!
//! 每行字段见下方 `writeln!`；`null` 表示回放没给 / 我方没有。
use gw2_rebuild::_scaffold::diffconv::{corrected_function, mode_of};
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Replay};
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::plane::{empty_mask, is_occupied, rasterize};
use std::io::Write;

/// 点列里第一个落进障碍圆的点（`None` = 一个都没有）。
///
/// `slack` 加在障碍半径上（`0.0` = 只用障碍半径；`0.5` = 再算上士兵/弹丸半径）。
fn first_inside(
    pts: &[(f64, f64)],
    obstacles: &[(f64, f64, f64)],
    slack: f64,
    strict: bool,
) -> Option<usize> {
    for (i, &(x, y)) in pts.iter().enumerate() {
        for &(ox, oy, r) in obstacles {
            let d = ((x - ox).powi(2) + (y - oy).powi(2)).sqrt();
            let hit = if strict { d < r + slack } else { d <= r + slack };
            if hit {
                return Some(i);
            }
        }
    }
    None
}

fn main() {
    let mut dir = gw2_rebuild::_scaffold::replaydiff::default_corpus_dir();
    let mut out: Option<std::path::PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
            "--json" => out = Some(args.next().expect("--json 需要值").into()),
            other => {
                eprintln!("未知参数 {other}");
                std::process::exit(2);
            }
        }
    }
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("读不了 {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let mut sink: Box<dyn Write> = match &out {
        Some(p) => Box::new(std::io::BufWriter::new(
            std::fs::File::create(p).expect("--json 打不开"),
        )),
        None => Box::new(std::io::stdout()),
    };

    for path in &files {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(replay) = Replay::load(path) else { continue };
        let batches = split_batches(&replay);
        if batches.is_empty() {
            continue;
        }
        // 批前最后一个 FunctionModeInfo 决定这一批的模式（`docs/06` §34.2：开火时现读）
        let mut mode_now = Mode::Normal;
        let mut cursor = 0usize;
        let mut obstacles: Vec<(f64, f64, f64)> = Vec::new();
        for (idx, b) in batches.iter().enumerate() {
            for m in &replay.msgs[cursor..b.start] {
                match m.kind.as_str() {
                    "FunctionModeInfo" => {
                        if let Some(s) = m.get_str("function_mode") {
                            mode_now = mode_of(&s);
                        }
                    }
                    "ClearObstacles" => obstacles.clear(),
                    "AddObstacle" => obstacles.push((
                        m.get_f64("pos_x").unwrap_or(f64::NAN),
                        m.get_f64("pos_y").unwrap_or(f64::NAN),
                        m.get_f64("radius").unwrap_or(f64::NAN),
                    )),
                    _ => {}
                }
            }
            cursor = b.start;
            if b.shots.is_empty() {
                continue;
            }
            let mode = mode_now;
            let window = &replay.msgs[b.start..=b.end];
            let mut obs_end: Vec<(i64, f64)> = Vec::new();
            let mut expl: Vec<(i64, f64, f64)> = Vec::new();
            let mut pending: Option<i64> = None;
            for m in window {
                match m.kind.as_str() {
                    "FunctionPoints" => obs_end.push((
                        m.get_i64("owner_id").unwrap_or(0),
                        m.get_f64("end_x").unwrap_or(f64::NAN),
                    )),
                    "FunctionActive" => pending = m.get_i64("owner_id"),
                    "EffectInfo" => {
                        if m.get_str("effect_type").as_deref() == Some("ExplosionEffect") {
                            if let Some(o) = pending.take() {
                                expl.push((
                                    o,
                                    m.get_f64("pos_x").unwrap_or(f64::NAN),
                                    m.get_f64("pos_y").unwrap_or(f64::NAN),
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
            // 光栅化掩膜（与 `graphwar_shot_calculator::simulate` 同一套积木）
            let mut mask = empty_mask();
            for &(ox, oy, r) in &obstacles {
                rasterize(&mut mask, ox, oy, r, 0x00);
            }
            for shot in &b.shots {
                let corrected = corrected_function(&shot.text, shot.x0, shot.y0, shot.flip, mode);
                let tokens = gw2_rebuild::math_function::parse(&corrected);
                let pts = calculate(
                    &tokens,
                    shot.x0,
                    shot.y0,
                    gw2_rebuild::SERVER_STEP,
                    shot.flip,
                    mode,
                );
                let stop = pts.last().copied().unwrap_or((f64::NAN, f64::NAN));
                let ex = obs_end
                    .iter()
                    .find(|(o, _)| *o == shot.owner)
                    .map(|(_, v)| *v);
                let ep = expl.iter().find(|(o, _, _)| *o == shot.owner);
                let flat_y = pts.len() > 1 && pts[1..].iter().all(|p| p.1 == pts[0].1);
                // 回放停点落在我方第几个采样点
                let mut idx_of: i64 = -1;
                if let Some(ex) = ex {
                    let mut best = f64::INFINITY;
                    for (i, p) in pts.iter().enumerate() {
                        let d = (p.0 - ex).abs();
                        if d < best {
                            best = d;
                            if best <= 1e-9 {
                                idx_of = i as i64;
                            }
                        }
                    }
                }
                // 我方点列里第一个进障碍的点（三种圆判据 + 掩膜判据）
                let hit_circle_lt = first_inside(&pts, &obstacles, 0.0, true);
                let hit_circle_le = first_inside(&pts, &obstacles, 0.0, false);
                let hit_circle_05 = first_inside(&pts, &obstacles, 0.5, true);
                let hit_mask = pts.iter().position(|&(x, y)| is_occupied(&mask, x, y));
                // 回放停点处我方的 y（判"两条弹道在这个点上是否逐位相同"）；
                // 以及**管线口径**的停点：掩膜第一个命中点（= `settle::resolve_points_owned`）
                let our_y_at_idx = if idx_of >= 0 {
                    pts.get(idx_of as usize).map(|p| p.1)
                } else {
                    None
                };
                // 回放**爆炸点** pos_x 落在我方第几个采样点（与 end_x 的位置分开记账）
                let mut pos_idx: i64 = -1;
                if let Some(px) = ep.map(|x| x.1) {
                    let mut best = f64::INFINITY;
                    for (i, p) in pts.iter().enumerate() {
                        let d = (p.0 - px).abs();
                        if d < best {
                            best = d;
                            if best <= 1e-9 {
                                pos_idx = i as i64;
                            }
                        }
                    }
                }
                let prev = if pts.len() >= 2 { Some(pts[pts.len() - 2]) } else { None };
                let ours_pipeline_stop = match hit_mask {
                    Some(i) => pts[i],
                    None => stop,
                };
                // 我方停点是否贴着 |x| = 25 的硬边界
                let x_at_limit = {
                    let inward = if shot.flip { -stop.0 } else { stop.0 };
                    inward.abs() + 0.009375 > gw2_rebuild::MAX_ABS_X
                        || inward.abs() + 1e-3 >= gw2_rebuild::MAX_ABS_X
                };
                let jnum = |v: Option<f64>| match v {
                    Some(x) if x.is_finite() => format!("{x:?}"),
                    _ => "null".into(),
                };
                let jidx = |v: Option<usize>| match v {
                    Some(i) => i.to_string(),
                    None => "null".into(),
                };
                writeln!(
                    sink,
                    concat!(
                        "{{\"file\":\"{file}\",\"batch\":{batch},\"mode\":\"{mode:?}\",",
                        "\"owner\":{owner},\"flip\":{flip},\"npts\":{npts},\"nobs\":{nobs},",
                        "\"our_x0\":{x0},\"our_y0\":{y0},",
                        "\"our_stop_x\":{sx},\"our_stop_y\":{sy},",
                        "\"our_travel\":{travel},",
                        "\"obs_end_x\":{ex},\"obs_pos_x\":{px},\"obs_pos_y\":{py},",
                        "\"obs_idx_in_ours\":{idx},\"flat_y\":{flat},\"x_at_limit\":{lim},",
                        "\"hit_circle_lt\":{hlt},\"hit_circle_le\":{hle},",
                        "\"hit_circle_05\":{h05},\"hit_mask\":{hmask},",
                        "\"our_y_at_idx\":{yidx},\"ours_pipeline_stop_x\":{pstop},",
                        "\"obs_pos_idx_in_ours\":{pix},\"prev_x\":{prevx},\"prev_y\":{prevy}}}"
                    ),
                    file = name,
                    batch = idx,
                    mode = mode,
                    owner = shot.owner,
                    flip = shot.flip,
                    npts = pts.len(),
                    nobs = obstacles.len(),
                    x0 = jnum(Some(shot.x0)),
                    y0 = jnum(Some(shot.y0)),
                    sx = jnum(Some(stop.0)),
                    sy = jnum(Some(stop.1)),
                    travel = jnum(Some((stop.0 - shot.x0).abs())),
                    ex = jnum(ex),
                    px = jnum(ep.map(|x| x.1)),
                    py = jnum(ep.map(|x| x.2)),
                    idx = idx_of,
                    flat = flat_y,
                    lim = x_at_limit,
                    hlt = jidx(hit_circle_lt),
                    hle = jidx(hit_circle_le),
                    h05 = jidx(hit_circle_05),
                    hmask = jidx(hit_mask),
                    yidx = jnum(our_y_at_idx),
                    pstop = jnum(Some(ours_pipeline_stop.0)),
                    pix = pos_idx,
                    prevx = jnum(prev.map(|p| p.0)),
                    prevy = jnum(prev.map(|p| p.1)),
                )
                .unwrap();
            }
        }
    }
    sink.flush().unwrap();
}
