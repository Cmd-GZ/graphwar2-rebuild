//! **`pos_y` 末位差预研探针**（任务书 09 的前置调查）。
//!
//! 只做一件事：把每一发**我方 vs 回放**的爆炸点 y 及其**所有候选算法**打进 JSONL，
//! 让离线脚本能用 ulp 判定"哪一个公式逐位对得上"。
//!
//! ```text
//! cargo run --release --offline --example posy_probe -- \
//!     --dir <语料> --json out.jsonl
//! ```
//!
//! 本文件是**新增**的（不改任何既有 example / src），口径照抄
//! `_scaffold::diffconv::probe_file`：同样的 `split_batches` / `Inputs::scan` /
//! `StateMask` / `resolve` / `corrected_function`，只多导出**原值与候选值**。
//!
//! 每行一个"发"，字段：
//!
//! | 字段 | 含义 |
//! |---|---|
//! | `x0,y0,flip,mode,fn` | 开火输入（全部来自回放） |
//! | `obs_end_x` | 回放 `FunctionPoints.end_x`（原值） |
//! | `obs_expl_x/obs_expl_y` | 回放 `EffectInfo(Explosion).pos_x/pos_y`（原值） |
//! | `obs_expl_y_raw` | 回放那条 `pos_y` 的**原始文本**（查序列化格式差） |
//! | `stop_x/stop_y` | 我方采样器停点（`resolve(...).stop`） |
//! | `i_end` | 我方点序列里 `x == obs_end_x` 的下标（找不到 = -1） |
//! | `tail` | 我方点序列最后 3 个点 `[[x,y],...]` |
//! | `c_*` | 候选公式（见下面注释），值 + 到 `obs_expl_y` 的 ulp |
use gw2_rebuild::_scaffold::diffconv::{corrected_function, mode_of};
use gw2_rebuild::_scaffold::replaydiff::wire::Kind;
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Inputs, Msg, Replay, StateMask};
use gw2_rebuild::graphwar_path_calculator::Mode;
use gw2_rebuild::graphwar_shot_calculator::settle::{resolve, Target};
use gw2_rebuild::graphwar_shot_calculator::{correction, shot_points, shifted_tokens};
use gw2_rebuild::math_function::{eval, parse};
use std::io::Write;

fn jstr(s: &str) -> String {
    gw2_rebuild::_scaffold::replaydiff::wire::quote(s)
}

/// 最短往返十进制（**不是** `{}`：`{}` 对整数值会丢 `.0`，看不出格式差）。
fn jnum(v: f64) -> String {
    if v.is_nan() {
        "\"NaN\"".into()
    } else if v.is_infinite() {
        format!("\"{}\"", if v > 0.0 { "inf" } else { "-inf" })
    } else {
        format!("{v:?}")
    }
}

fn bits(v: f64) -> String {
    format!("\"{:016x}\"", v.to_bits())
}

/// 有符号 ulp 距离（`a - b` 按位序，与 `replaydiff::ulp_diff` 同一个定义但保留符号）。
fn ulp(a: f64, b: f64) -> i64 {
    fn ord(x: f64) -> i64 {
        let b = x.to_bits() as i64;
        if b < 0 {
            i64::MIN - b
        } else {
            b
        }
    }
    if a.is_nan() || b.is_nan() {
        return i64::MAX;
    }
    ord(a).saturating_sub(ord(b))
}

fn is_dead(m: &Msg) -> bool {
    matches!(m.get("alive").map(|v| v.kind.clone()), Some(Kind::Bool(false)))
}

fn main() {
    let mut dir = gw2_rebuild::_scaffold::replaydiff::default_corpus_dir();
    let mut json_out: Option<std::path::PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
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
    let mut sink: Box<dyn Write> = match &json_out {
        Some(p) => Box::new(std::io::BufWriter::new(
            std::fs::File::create(p).expect("--json 打不开"),
        )),
        None => Box::new(std::io::stdout()),
    };

    let mut names: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    names.sort();

    let (mut files_ok, mut shots_total, mut shots_with_expl) = (0usize, 0usize, 0usize);

    for path in &names {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(replay) = Replay::load(path) else {
            continue;
        };
        let batches = split_batches(&replay);
        if batches.is_empty() {
            continue;
        }
        files_ok += 1;
        let inputs = Inputs::scan(&replay, batches[0].start);
        let mut mode = mode_of(&inputs.room.function_mode);
        let mut alive: std::collections::BTreeSet<i64> = inputs
            .alive
            .iter()
            .filter(|(_, v)| **v)
            .map(|(k, _)| *k)
            .collect();
        let mut pos: std::collections::BTreeMap<i64, (f64, f64)> = inputs.pos.clone();
        let mut mask = StateMask::new();
        mask.apply(&replay.msgs[..batches[0].start]);
        let mut cursor = batches[0].start;

        for (idx, b) in batches.iter().enumerate() {
            for m in &replay.msgs[cursor..b.start] {
                match m.kind.as_str() {
                    "FunctionModeInfo" => {
                        if let Some(s) = m.get_str("function_mode") {
                            mode = mode_of(&s);
                        }
                    }
                    "LifeInfo" => {
                        if let Some(id) = m.get_i64("entity_id") {
                            if is_dead(m) {
                                alive.remove(&id);
                            } else {
                                alive.insert(id);
                            }
                        }
                    }
                    "EntityRemoved" => {
                        if let Some(id) = m.get_i64("entity_id") {
                            alive.remove(&id);
                            pos.remove(&id);
                        }
                    }
                    "PosInfo" => {
                        if let Some(id) = m.get_i64("entity_id") {
                            pos.insert(
                                id,
                                (
                                    m.get_f64("pos_x").unwrap_or(0.0),
                                    m.get_f64("pos_y").unwrap_or(0.0),
                                ),
                            );
                            alive.insert(id);
                        }
                    }
                    _ => {}
                }
            }
            mask.apply(&replay.msgs[cursor..b.start]);
            cursor = b.start;
            if b.shots.is_empty() {
                continue;
            }
            let observed: Vec<Msg> = replay.msgs[b.start..=b.end].to_vec();
            // 回放声明的 end_x（按 owner）
            let mut obs_end_x: std::collections::BTreeMap<i64, f64> = Default::default();
            for m in &observed {
                if m.kind == "FunctionPoints" {
                    if let (Some(o), Some(x)) = (m.get_i64("owner_id"), m.get_f64("end_x")) {
                        obs_end_x.insert(o, x);
                    }
                }
            }
            // 回放声明的爆炸点（`FunctionActive` 之后那条 `ExplosionEffect`），按顺序配对
            let mut obs_expl: Vec<(f64, f64, String, String)> = Vec::new(); // x,y,raw_x,raw_y
            {
                let mut pending = false;
                for m in &observed {
                    if m.kind == "FunctionActive" {
                        pending = true;
                    } else if pending
                        && m.kind == "EffectInfo"
                        && m.get_str("effect_type").as_deref() == Some("ExplosionEffect")
                    {
                        pending = false;
                        let raw = |n: &str| {
                            m.get(n)
                                .map(|v| v.raw.clone())
                                .unwrap_or_else(|| "?".into())
                        };
                        obs_expl.push((
                            m.get_f64("pos_x").unwrap_or(f64::NAN),
                            m.get_f64("pos_y").unwrap_or(f64::NAN),
                            raw("pos_x"),
                            raw("pos_y"),
                        ));
                    }
                }
            }
            let obs_corrected: Vec<String> = observed
                .iter()
                .filter(|m| m.kind == "FunctionInfo")
                .map(|m| m.get_str("corrected_function").unwrap_or_default())
                .collect();

            for (shot_i, shot) in b.shots.iter().enumerate() {
                shots_total += 1;
                let targets: Vec<Target> = alive
                    .iter()
                    .filter(|id| **id != shot.owner)
                    .filter_map(|id| {
                        pos.get(id).map(|(x, y)| Target {
                            id: *id,
                            x: *x,
                            y: *y,
                            radius: 0.5,
                        })
                    })
                    .collect();
                let r = resolve(
                    &shot.text,
                    shot.x0,
                    shot.y0,
                    gw2_rebuild::SERVER_STEP,
                    shot.flip,
                    mode,
                    mask.mask(),
                    &targets,
                );
                let pts =
                    shot_points(&shot.text, shot.x0, shot.y0, gw2_rebuild::SERVER_STEP, shot.flip, mode, true);
                let end_x = obs_end_x.get(&shot.owner).copied().unwrap_or(f64::NAN);
                let have_expl = shot_i < obs_expl.len();
                if have_expl {
                    shots_with_expl += 1;
                }
                let (ex, ey, raw_x, raw_y) = if have_expl {
                    (
                        obs_expl[shot_i].0,
                        obs_expl[shot_i].1,
                        obs_expl[shot_i].2.clone(),
                        obs_expl[shot_i].3.clone(),
                    )
                } else {
                    (f64::NAN, f64::NAN, "?".into(), "?".into())
                };

                // ---- 候选公式 ----
                let toks_fn = parse(&shot.text);
                let c_exact = correction(&toks_fn, shot.x0, shot.y0, shot.flip);
                let xa = if shot.flip { -end_x } else { end_x }; // 回代用的 x
                let xs = if shot.flip { -r.stop.0 } else { r.stop.0 };
                // `c_obs_h1`：现在的实现 —— 拿**回放的** corrected_function 文本在 end_x 处求值
                let c_obs_h1 = {
                    let t = obs_corrected.get(shot_i).cloned().unwrap_or_default();
                    if t.is_empty() {
                        f64::NAN
                    } else {
                        eval(&parse(&t), xa, 0.0, 0.0)
                    }
                };
                // `c_exact`：用**精确** c（采样器用的那个）在 end_x 处求 f(x)+c
                let c_exact_at_endx = eval(&shifted_tokens(&toks_fn, c_exact), xa, 0.0, 0.0);
                // `c_3term`：三项不同结合 —— `(f(x) - f(x0)) + y0`
                let fx = eval(&toks_fn, xa, 0.0, 0.0);
                let fx0 = eval(&toks_fn, if shot.flip { -shot.x0 } else { shot.x0 }, 0.0, 0.0);
                let c_3term = (fx - fx0) + shot.y0;
                // `c_3term_b`：`f(x) + (y0 - f(x0))`（= c_exact，留作对照）
                let c_3term_b = fx + (shot.y0 - fx0);
                // `c_stop_at_stopx`：用精确 c 在**停点 x** 求值（= 采样器停点 y，若无阻挡）
                let c_at_stopx = eval(&shifted_tokens(&toks_fn, c_exact), xs, 0.0, 0.0);

                // 我方点序列里 x == end_x 的下标（逐位比）
                let mut i_end: i64 = -1;
                for (i, p) in pts.iter().enumerate() {
                    if p.0.to_bits() == end_x.to_bits() {
                        i_end = i as i64;
                        break;
                    }
                }
                let nb = |k: i64| -> (f64, f64) {
                    if k >= 0 && (k as usize) < pts.len() {
                        pts[k as usize]
                    } else {
                        (f64::NAN, f64::NAN)
                    }
                };
                let (px_prev, py_prev) = nb(i_end - 1);
                let (px_next, py_next) = nb(i_end + 1);
                let (px_here, py_here) = nb(i_end);
                let mut tail: Vec<(f64, f64)> = pts.iter().rev().take(3).rev().copied().collect();
                while tail.len() < 3 {
                    tail.insert(0, (f64::NAN, f64::NAN));
                }

                let u = |v: f64| ulp(v, ey).to_string();
                let line = format!(
                    concat!(
                        "{{\"file\":{},\"batch\":{},\"shot\":{},\"fo\":{},\"owners\":{},\"mode\":{},",
                        "\"fn\":{},\"x0\":{},\"y0\":{},\"flip\":{},",
                        "\"obs_end_x\":{},\"obs_expl_x\":{},\"obs_expl_y\":{},",
                        "\"obs_expl_x_raw\":{},\"obs_expl_y_raw\":{},",
                        "\"stop_x\":{},\"stop_y\":{},\"stop_bits\":{},\"blocked\":{},",
                        "\"n_points\":{},\"i_end\":{},\"here\":[{},{}],\"prev\":[{},{}],\"next\":[{},{}],",
                        "\"tail\":[[{},{}],[{},{}],[{},{}]],",
                        "\"c_obs_h1\":{},\"c_exact_at_endx\":{},\"c_exact_at_stopx\":{},\"c_3term\":{},\"c_3term_b\":{},",
                        "\"fx\":{},\"fx0\":{},\"c_exact\":{},",
                        "\"corrected_obs\":{},\"corrected_ours\":{},",
                        "\"u\":{{\"stop\":{},\"obs_h1\":{},\"exact_endx\":{},\"exact_stopx\":{},\"3term\":{},\"prev\":{},\"next\":{},\"here\":{}}}",
                        "}}"
                    ),
                    jstr(&name),
                    idx,
                    shot_i,
                    shot_i, // 批内第几发（= corrected_function 的配对序号）
                    jstr(&format!("{}", shot.owner)),
                    jstr(if mode == Mode::Ode { "DiffEq" } else { "Normal" }),
                    jstr(&shot.text),
                    jnum(shot.x0),
                    jnum(shot.y0),
                    shot.flip,
                    jnum(end_x),
                    jnum(ex),
                    jnum(ey),
                    jstr(&raw_x),
                    jstr(&raw_y),
                    jnum(r.stop.0),
                    jnum(r.stop.1),
                    bits(r.stop.1),
                    r.blocked,
                    pts.len(),
                    i_end,
                    jnum(px_here),
                    jnum(py_here),
                    jnum(px_prev),
                    jnum(py_prev),
                    jnum(px_next),
                    jnum(py_next),
                    jnum(tail[0].0),
                    jnum(tail[0].1),
                    jnum(tail[1].0),
                    jnum(tail[1].1),
                    jnum(tail[2].0),
                    jnum(tail[2].1),
                    jnum(c_obs_h1),
                    jnum(c_exact_at_endx),
                    jnum(c_at_stopx),
                    jnum(c_3term),
                    jnum(c_3term_b),
                    jnum(fx),
                    jnum(fx0),
                    jnum(c_exact),
                    jstr(&obs_corrected.get(shot_i).cloned().unwrap_or_default()),
                    jstr(&corrected_function(&shot.text, shot.x0, shot.y0, shot.flip, mode)),
                    u(r.stop.1),
                    u(c_obs_h1),
                    u(c_exact_at_endx),
                    u(c_at_stopx),
                    u(c_3term),
                    u(py_prev),
                    u(py_next),
                    u(py_here),
                );
                writeln!(sink, "{line}").unwrap();
            }
        }
    }
    eprintln!("文件 {files_ok}；发 {shots_total}（拿到爆炸点 {shots_with_expl}）");
}
