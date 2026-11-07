//! **`pos_y` 末位差的机制判别探针**（`dispatch/26`）：到底"哪一个量"在最后一位上不同。
//!
//! 只做一件事：对每一发，把**回放那条 `EffectInfo(Explosion).pos_y`** 与一批
//! **只差一个可指认的量**的候选值逐位比，统计"谁命中"。
//!
//! ```text
//! cargo run --release --offline --example posy_mech -- --dir <语料> --json out.jsonl
//! ```
//!
//! 本文件**新增**（不改既有 example / `src`）。口径照抄 `examples/posy_probe.rs`：
//! 同样的 `split_batches` / `Inputs::scan` / `StateMask` / `resolve` / 打印文本，
//! 只多算"同一个函数在同一条轨迹上、**换一个被求值的 x**或**把常数挪 1 ulp**"的值。
//!
//! ## 为什么是这两个旋钮（可检验的判据）
//!
//! `Normal` 模式下爆炸点的 `y` 只可能由**一次** `f(x) + c` 得到（编译器/ISA 无关：
//! 两个 double 的单次加法在任何 IEEE 一致性实现里都逐位相同，x87 的 64 位尾数也足够
//! 精确表示两个 double 的和 ⇒ 双舍入不产生差异）。所以"同一个 x、同一个 c、同一个
//! 求值器却差 1 ulp"**在数学上不可能**，只能说明：
//!
//! * 被求值的 `x` 不是报文里那个 `x`（候选：`end_x` / 采样器停点 / 两者差 1 ulp），或
//! * 常数 `c` 不是解析打印文本得到的那个 double（候选：`c ± 1 ulp`）。
//!
//! 本探针把这两个旋钮都拧一遍，命中数就是判据。
//!
//! 每行一个"发"：
//!
//! | 字段 | 含义 |
//! |---|---|
//! | `obs_x/obs_y` | 回放的 `EffectInfo(Explosion).pos_x/pos_y`（逐位即真值） |
//! | `stop_x/stop_y` | 我方采样器停点 |
//! | `end_x` | 回放 `FunctionPoints.end_x` |
//! | `v` | 各候选值与 `obs_y` 的**有符号 ulp 差**（0 = 逐位相同） |
use gw2_rebuild::_scaffold::diffconv::{corrected_function, mode_of};
use gw2_rebuild::_scaffold::replaydiff::wire::Kind;
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Inputs, Msg, Replay, StateMask};
use gw2_rebuild::graphwar_path_calculator::Mode;
use gw2_rebuild::graphwar_shot_calculator::settle::{resolve, Target};
use gw2_rebuild::math_function::{eval, parse};
use std::io::Write;

fn jstr(s: &str) -> String {
    gw2_rebuild::_scaffold::replaydiff::wire::quote(s)
}

fn jnum(v: f64) -> String {
    if v.is_nan() {
        "\"NaN\"".into()
    } else if v.is_infinite() {
        format!("\"{}\"", if v > 0.0 { "inf" } else { "-inf" })
    } else {
        format!("{v:?}")
    }
}

/// 有符号 ulp 距离 `a - b`（与 `posy_probe` 同一个定义）。
fn ulp(a: f64, b: f64) -> String {
    fn ord(x: f64) -> i64 {
        let w = x.to_bits() as i64;
        if w < 0 {
            i64::MIN - w
        } else {
            w
        }
    }
    if a.is_nan() || b.is_nan() {
        return "\"NaN\"".into();
    }
    ord(a).saturating_sub(ord(b)).to_string()
}

/// `k` 个 ulp 的位移（朝 +∞ 方向）。
fn nudge(v: f64, k: i64) -> f64 {
    if v == 0.0 {
        return v;
    }
    let bits = v.to_bits() as i64;
    let moved = if (v > 0.0) == (k > 0) {
        bits + k.abs()
    } else {
        bits - k.abs()
    };
    f64::from_bits(moved as u64)
}

/// 把 token 流里**所有** `Const` 各挪 1 ulp（`+1` / `-1`），返回两个新流。
fn const_shifted(toks: &[gw2_rebuild::Token], k: i64) -> Vec<gw2_rebuild::Token> {
    toks.iter()
        .map(|t| {
            if t.tag == gw2_rebuild::Function::Const {
                gw2_rebuild::Token {
                    tag: t.tag,
                    value: nudge(t.value, k),
                }
            } else {
                *t
            }
        })
        .collect()
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
            let mut obs_end_x: std::collections::BTreeMap<i64, f64> = Default::default();
            for m in &observed {
                if m.kind == "FunctionPoints" {
                    if let (Some(o), Some(x)) = (m.get_i64("owner_id"), m.get_f64("end_x")) {
                        obs_end_x.insert(o, x);
                    }
                }
            }
            let mut obs_expl: Vec<(f64, f64)> = Vec::new();
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
                        obs_expl.push((
                            m.get_f64("pos_x").unwrap_or(f64::NAN),
                            m.get_f64("pos_y").unwrap_or(f64::NAN),
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
                if shot_i >= obs_expl.len() {
                    continue;
                }
                let (ex, ey) = obs_expl[shot_i];
                let end_x = obs_end_x.get(&shot.owner).copied().unwrap_or(f64::NAN);
                // 两个候选被求值的 x：回放声明的 end_x、我方采样器停点。
                let x_end = if shot.flip { -end_x } else { end_x };
                let x_stop = if shot.flip { -r.stop.0 } else { r.stop.0 };
                // ★ 自洽性判据：**拿报文自己那个 pos_x 回代**。若原版的 (pos_x,pos_y)
                // 是同一个点上"一次 f(x)+c"的两个坐标，这一项必须命中；不命中就说明
                // 那两个坐标不是同一个 x 上算出来的（点在**拼装**时就已经不自洽）。
                let x_obs = if shot.flip { -ex } else { ex };
                // 回放的修正文本（真值侧）与我方文本（自洽侧）各一份。
                let text_obs = obs_corrected.get(shot_i).cloned().unwrap_or_default();
                let text_ours = corrected_function(&shot.text, shot.x0, shot.y0, shot.flip, mode);

                let ev = |t: &str, x: f64, k: i64| -> f64 {
                    if t.is_empty() {
                        return f64::NAN;
                    }
                    let toks = parse(t);
                    let toks = if k == 0 { toks } else { const_shifted(&toks, k) };
                    eval(&toks, x, 0.0, 0.0)
                };

                // Normal 才算这些候选：DiffEq 的 y 来自积分，不是 f(x)。
                let (v_end, v_stop, v_end_cp, v_end_cm, v_stop_cp, v_stop_cm, v_ours_end, v_ours_stop) =
                    if mode == Mode::Ode {
                        (
                            f64::NAN,
                            f64::NAN,
                            f64::NAN,
                            f64::NAN,
                            f64::NAN,
                            f64::NAN,
                            f64::NAN,
                            f64::NAN,
                        )
                    } else {
                        (
                            ev(&text_obs, x_end, 0),
                            ev(&text_obs, x_stop, 0),
                            ev(&text_obs, x_end, 1),
                            ev(&text_obs, x_end, -1),
                            ev(&text_obs, x_stop, 1),
                            ev(&text_obs, x_stop, -1),
                            ev(&text_ours, x_end, 0),
                            ev(&text_ours, x_stop, 0),
                        )
                    };

                let v_self = if mode == Mode::Ode { f64::NAN } else { ev(&text_obs, x_obs, 0) };
                let v_self_ours = if mode == Mode::Ode { f64::NAN } else { ev(&text_ours, x_obs, 0) };
                let line = format!(
                    concat!(
                        "{{\"file\":{},\"batch\":{},\"shot\":{},",
                        "\"mode\":{},\"fn\":{},",
                        "\"obs_x\":{},\"obs_y\":{},\"end_x\":{},\"stop_x\":{},\"stop_y\":{},",
                        "\"v\":{{\"end\":{},\"stop\":{},\"end_cp\":{},\"end_cm\":{},",
                        "\"stop_cp\":{},\"stop_cm\":{},\"ours_end\":{},\"ours_stop\":{}}},",
                        "\"v2\":{{\"self\":{},\"self_ours\":{}}},",
                        "\"val\":{{\"ours_end\":{},\"ours_stop\":{}}}}}"
                    ),
                    jstr(&name),
                    idx,
                    shot_i,
                    jstr(if mode == Mode::Ode { "DiffEq" } else { "Normal" }),
                    jstr(&shot.text),
                    jnum(ex),
                    jnum(ey),
                    jnum(end_x),
                    jnum(r.stop.0),
                    jnum(r.stop.1),
                    ulp(v_end, ey),
                    ulp(v_stop, ey),
                    ulp(v_end_cp, ey),
                    ulp(v_end_cm, ey),
                    ulp(v_stop_cp, ey),
                    ulp(v_stop_cm, ey),
                    ulp(v_ours_end, ey),
                    ulp(v_ours_stop, ey),
                    ulp(v_self, ey),
                    ulp(v_self_ours, ey),
                    jnum(v_ours_end),
                    jnum(v_ours_stop),
                );
                writeln!(sink, "{line}").unwrap();
            }
        }
    }
}
