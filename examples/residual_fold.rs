//! **「(文本, 常数) 折叠复算」通用工具**（`dispatch/35`）：对**每一发**把回放自己给出的
//! `(函数文本, 常数, pos_x)` 代回去，逐位（`to_bits`）比回放的 `pos_y`。
//!
//! ```text
//! cargo run --release --offline --example residual_fold -- --dir <快照> --json fold.jsonl
//! ```
//!
//! 输出逐发一行 JSONL，**不含任何本机路径、不含绝对地址**。下游 `tools/residual_fold.py`
//! 负责与 `replay_attrib` 的归因表 `join`、按 `Normal`/`DiffEq` 两条轴出统计。
//!
//! ## 判据（写死在这里，别在报告里改口径）
//!
//! **不自洽** = 用回放**自己**给出的量复算，得不到回放**自己**的 `pos_y`（`to_bits` 不等）。
//! 不是"差很小"，差 1 ulp 也算不自洽。三条输入全部来自回放报文，**一条都不来自我方实现**：
//!
//! | 量 | 出处 |
//! |---|---|
//! | 文本 | `FunctionInfo.function`（原文） |
//! | 常数 | `FunctionInfo.corrected_function` 里**原文之后的那个后缀**（如 `" + 1.916"`） |
//! | `pos_x` | 同批 `EffectInfo(ExplosionEffect).pos_x` |
//!
//! ⇒ 复算 = `eval(parse(corrected_function), flip ? -pos_x : pos_x)`，与
//! `graphwar_shot_calculator::settle::explosion_point` **同一条式子**（`docs/06` §28/§30），
//! 所以"我方实现对不对"与这条判据**无关**：它只问回放自身是否闭合。
//!
//! `Normal` 模式 `y` 是 `x` 的函数，上面这条直接成立。
//! `DiffEq/Ode` 模式的 `y` **不是** `x` 的函数，是初值问题的解 ⇒ 必须积分。
//! 复算走内核**同一个**采样器 [`graphwar_path_calculator::calculate`]（RK4 + 等弦长二分），
//! 在这一发自己的点列里找 `x == pos_x` 的那一点，比它的 `y`：
//!
//! * `ode_found = true`  且 `y` 逐位相同 ⇒ **命中**（点落在曲线上）；
//! * `ode_found = true`  且 `y` 不同   ⇒ **不自洽**（`x` 对上了、曲线对不上）；
//! * `ode_found = false`               ⇒ **其它**（回放的点不在我们积分出的网格上 ——
//!   这一档**分不清**是"我方积分步长先跑偏"还是"回放点不对"，见文档的诚实清单）。
//!
//! ## 顺带导出的一条**纯回放内部**判据
//!
//! `FunctionPoints.end_x` 与 `EffectInfo(Explosion).pos_x` 按 `docs/06` §27 与 §43 都是
//! `pts[len-1].x`（同一个点、同一次读）⇒ **两者必须逐位相等**。本工具逐发记 `pos_eq_endx`，
//! 不需要任何实现参与。这一项在 `X` 桶上是 0/91（`docs/06` §39），本工具复算它。
//!
//! 口径照抄 `examples/posy_mech.rs`（同一个 `split_batches` / 模式时刻表 / `is_dead`），
//! **不重写**：本文件只把"复算"从"我方管线的停点"换成"回放自己的 `pos_x`"，
//! 并补上 `DiffEq` 分支（`posy_mech` 在 `Mode::Ode` 上整列返回 NaN）。
use gw2_rebuild::_scaffold::diffconv::mode_of;
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Replay};
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::graphwar_shot_calculator::correction_text;
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

/// 有符号 ulp 距离 `a - b`（与 `posy_mech` 同一个定义）。0 = 逐位相同。
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

fn same_bits(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits()
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

    // 计数（stderr，方便一眼看住分母）
    let mut n_shots = 0usize;
    let mut n_normal = 0usize;
    let mut n_ode = 0usize;

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
        let mut mode = mode_of(
            &gw2_rebuild::_scaffold::replaydiff::Inputs::scan(&replay, batches[0].start)
                .room
                .function_mode,
        );
        // 队伍朝向：`split_batches` 已经把 `flip` 算好放在 `ShotInput` 里（Team2 ⇒ true）。
        let mut cursor = batches[0].start;

        for (idx, b) in batches.iter().enumerate() {
            // 批前的模式更新（与 `posy_mech` / `replay_attrib` 同一条时刻表）
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
            let observed = &replay.msgs[b.start..=b.end];

            // 回放自己的 `FunctionPoints`（起点已经进了 `b.shots`，这里只要 `end_x`）
            let mut obs_end_x: std::collections::BTreeMap<i64, f64> = Default::default();
            // 回放自己的 `FunctionInfo`：原文 + 修正文本（逐发一份）
            let mut obs_cf: std::collections::BTreeMap<i64, (String, Option<String>)> =
                Default::default();
            for m in observed {
                match m.kind.as_str() {
                    "FunctionPoints" => {
                        if let (Some(o), Some(x)) = (m.get_i64("owner_id"), m.get_f64("end_x")) {
                            obs_end_x.insert(o, x);
                        }
                    }
                    "FunctionInfo" => {
                        if let Some(o) = m.get_i64("owner_id") {
                            let f = m.get_str("function").unwrap_or_default();
                            let c = m.get_str("corrected_function");
                            obs_cf.insert(o, (f, c));
                        }
                    }
                    _ => {}
                }
            }
            // 爆炸点：`FunctionActive` 之后紧跟的那个 `ExplosionEffect`（与 `posy_mech` 同一条）
            let mut obs_expl: Vec<(f64, f64)> = Vec::new();
            {
                let mut pending = false;
                for m in observed {
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

            for (shot_i, shot) in b.shots.iter().enumerate() {
                n_shots += 1;
                if mode == Mode::Ode {
                    n_ode += 1;
                } else {
                    n_normal += 1;
                }
                let (ex, ey) = obs_expl.get(shot_i).copied().unwrap_or((f64::NAN, f64::NAN));
                let has_expl = shot_i < obs_expl.len();
                let end_x = obs_end_x.get(&shot.owner).copied().unwrap_or(f64::NAN);

                // ---- 三个回放自带的量与常数后缀 ----
                let (fn_text, cf_opt) = obs_cf
                    .get(&shot.owner)
                    .cloned()
                    .unwrap_or((shot.text.clone(), None));
                // 常数 = `corrected_function` 里**原文之后**的后缀（逐字节，不做任何重构）
                let (c_suffix, split_ok) = match cf_opt.as_deref() {
                    Some(cf) => match cf.strip_prefix(fn_text.as_str()) {
                        Some(rest) => (rest.to_string(), true),
                        None => (String::new(), false),
                    },
                    None => (String::new(), false),
                };
                // 我方生成的那一串（只为核对：`C` 桶是"我方常数与回放不同"，不在本判据里）
                let c_ours = correction_text(&parse(&fn_text), shot.x0, shot.y0, shot.flip);

                let x_obs = if shot.flip { -ex } else { ex };
                let x_end = if shot.flip { -end_x } else { end_x };

                // ---- Normal：y = f(x)，一次求值 ----
                //
                // 三个候选，全部**只用回放自己的量**（`(fn, x0, y0, flip, pos_x, end_x)`）：
                //   `self`   = `eval(corrected_function, pos_x)` —— 判据本体；
                //   `endx`   = `eval(corrected_function, end_x)` —— 若这一项命中，说明回放的
                //              `pos_y` 属于 `end_x` 那个点，而它自己写的 `pos_x` 是**另一个点**
                //              ⇒ 「拼装时取错了点」，不需要任何实现参与；
                //   `fullc`  = `f(x) + c_full`（`c` **不经过** 3 位小数打印）—— 34 号那套
                //              "折叠在哪一步"的另一种折法。
                let (v_self, self_hit, v_endx, endx_hit, v_fullc, fullc_hit) = if mode == Mode::Ode {
                    (f64::NAN, false, f64::NAN, false, f64::NAN, false)
                } else if let Some(cf) = cf_opt.as_deref() {
                    let toks = parse(cf);
                    let v = eval(&toks, x_obs, 0.0, 0.0);
                    let v2 = eval(&toks, x_end, 0.0, 0.0);
                    let mut ftoks = parse(&fn_text);
                    let c_full = gw2_rebuild::graphwar_shot_calculator::correction(
                        &ftoks, shot.x0, shot.y0, shot.flip,
                    );
                    ftoks = gw2_rebuild::graphwar_shot_calculator::shifted_tokens(&ftoks, c_full);
                    let v3 = eval(&ftoks, x_obs, 0.0, 0.0);
                    (
                        v,
                        has_expl && same_bits(v, ey),
                        v2,
                        has_expl && same_bits(v2, ey),
                        v3,
                        has_expl && same_bits(v3, ey),
                    )
                } else {
                    (f64::NAN, false, f64::NAN, false, f64::NAN, false)
                };

                // ---- Ode：y 是初值问题的解，必须积分 ----
                // `ode_found` = 回放的 `pos_x` 落在我们积分出的点列上（逐位）；
                // `ode_hit`   = 那一点的 y 与回放 `pos_y` 逐位相同。
                let (ode_found, ode_y, ode_n, ode_last, ode_end_x_match, ode_endx_hit) =
                    if mode == Mode::Ode {
                        let text = cf_opt.as_deref().unwrap_or(fn_text.as_str());
                        let toks = parse(text);
                        let pts = calculate(
                            &toks,
                            shot.x0,
                            shot.y0,
                            gw2_rebuild::SERVER_STEP,
                            shot.flip,
                            Mode::Ode,
                        );
                        let hit = pts.iter().find(|p| same_bits(p.0, ex)).map(|p| p.1);
                        let last = pts.last().copied().unwrap_or((f64::NAN, f64::NAN));
                        let hit_end = pts.iter().find(|p| same_bits(p.0, end_x)).map(|p| p.1);
                        (
                            hit.is_some(),
                            hit.unwrap_or(f64::NAN),
                            pts.len(),
                            last,
                            same_bits(last.0, end_x),
                            hit_end.map(|y| same_bits(y, ey)).unwrap_or(false),
                        )
                    } else {
                        (false, f64::NAN, 0usize, (f64::NAN, f64::NAN), false, false)
                    };

                let line = format!(
                    concat!(
                        "{{\"file\":{},\"batch\":{},\"shot\":{},\"owner\":{},\"mode\":{},",
                        "\"fn\":{},\"cf\":{},\"c_suffix\":{},\"split_ok\":{},\"c_ours\":{},",
                        "\"x0\":{},\"y0\":{},\"flip\":{},",
                        "\"pos_x\":{},\"pos_y\":{},\"end_x\":{},\"has_expl\":{},",
                        "\"pos_eq_endx\":{},",
                        "\"v_self\":{},\"self_hit\":{},\"self_ulp\":{},",
                        "\"v_endx\":{},\"endx_hit\":{},\"endx_ulp\":{},",
                        "\"v_fullc\":{},\"fullc_hit\":{},",
                        "\"ode_found\":{},\"ode_y\":{},\"ode_hit\":{},",
                        "\"ode_n\":{},\"ode_last_x\":{},\"ode_last_y\":{},\"ode_end_x_match\":{},",
                        "\"ode_endx_hit\":{}}}"
                    ),
                    jstr(&name),
                    idx,
                    shot_i,
                    shot.owner,
                    jstr(if mode == Mode::Ode { "DiffEq" } else { "Normal" }),
                    jstr(&fn_text),
                    jstr(cf_opt.as_deref().unwrap_or("\u{0}null")),
                    jstr(&c_suffix),
                    split_ok,
                    jstr(&c_ours),
                    jnum(shot.x0),
                    jnum(shot.y0),
                    shot.flip,
                    jnum(ex),
                    jnum(ey),
                    jnum(end_x),
                    has_expl,
                    same_bits(ex, end_x),
                    jnum(v_self),
                    self_hit,
                    ulp(v_self, ey),
                    jnum(v_endx),
                    endx_hit,
                    ulp(v_endx, ey),
                    jnum(v_fullc),
                    fullc_hit,
                    ode_found,
                    jnum(ode_y),
                    ode_found && same_bits(ode_y, ey),
                    ode_n,
                    jnum(ode_last.0),
                    jnum(ode_last.1),
                    ode_end_x_match,
                    ode_endx_hit,
                );
                writeln!(sink, "{line}").unwrap();
            }
        }
    }
    eprintln!("逐发：{n_shots}（Normal {n_normal} / DiffEq {n_ode}）");
}
