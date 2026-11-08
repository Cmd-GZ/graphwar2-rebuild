//! **打印-回代链**的逐发放大量化（`docs/06-verification.md` §34，`dispatch/56`）。
//!
//! 这条链是：
//!
//! ```text
//! c（f64，全精度）            ← graphwar_shot_calculator::correction（真值 0x3c5f70 体内）
//!   ↓ format "{:.3}"          ← graphwar_shot_calculator::correction_text（" + 1.197"）
//! 文本（3 位小数）            ← + 原文 = corrected_function（真值 0x3b79a8/x86 0x3c5f70 的 sret）
//!   ↓ parse（真值 0x4ed670）  ← math_function::parse
//! token 列表                  ← 采样器 0x3c2530 的入参（§26 运行时 4/4 逐字节）
//!   ↓ eval（真值 0x4eb9d0）
//! 采样/回代的 f64
//! ```
//!
//! 本工具**只读回放**（`FunctionInfo` / `FunctionPoints` / `EffectInfo(Explosion)`），
//! 对每发算出一排候选值，全部按 `f64::to_bits` 存档，判据交给
//! `tools/print_roundtrip.py`（分桶规则要能随时改，不重新编译 —— 与 `bit_exact_attrib` 同理）。
//!
//! ```text
//! cargo run --release --offline --example print_roundtrip -- --dir <快照> --json out.jsonl
//! ```
//!
//! ## 候选（每发一行，全部是 `to_bits` 存的十进制 u64）
//!
//! | 名 | 式子 | 用来回答 |
//! |---|---|---|
//! | `A` | `eval(parse(回放 corrected_function), ±pos_x)` | 回放**自己**的回代（§25 的判据本体） |
//! | `B` | `eval(parse(回放 corrected_function), ±end_x)` | 回代点取 `end_x` 而不是 `pos_x` |
//! | `C` | `eval(全精度 c 的树, ±pos_x)` | "3 位小数打印"这一步到底有没有影响 |
//! | `D` | `eval(parse(我方发出去的文本), ±pos_x)` | 我方打印的串与回放的串是否同一个输入 |
//! | `E` | 用**我方采样串**跑采样器，取最后一点 | 我方现在的报文值（`fire` 轴口径） |
//! | `F` | 用**全精度 c**跑采样器，取最后一点 | 如果打印这一步去掉会怎样 |
//! | `G` | 用**回放的文本**跑采样器，取最后一点 | 真机链的复刻（§26：采样器吃 parse(corrected_function)） |
//!
//! `c_full` / `c_print` 两个常量的位模式也逐发记下来（`c_print` = 把 `" + 1.197"`
//! 这串**再解析**得到的 f64，不是"算出来的 c 再舍入"）。

use gw2_rebuild::_scaffold::replaydiff::{split_batches, Inputs, Replay};
use gw2_rebuild::graphwar_path_calculator::Mode;
use gw2_rebuild::graphwar_shot_calculator::{correction, correction_text, shot_points};
use gw2_rebuild::math_function::{eval, parse};
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

fn jbits(v: f64) -> String {
    if v.is_finite() {
        format!("{}", v.to_bits())
    } else {
        "null".into()
    }
}

/// 与 `src/game.rs::balanced_texts` **同一套规则**的副本（那个函数是 `pub(crate)`）。
///
/// 两个串的差别只在"多出来的 `)`"：真值发出去的串只补不删，采样串把无主的 `)` 去掉
/// （`docs/06` §29）。`counter` 记下本文本被删掉的 `)` 个数，归因时用得上。
fn balanced_texts(text: &str) -> (String, String, usize) {
    let mut depth = 0i64;
    let mut stripped = String::with_capacity(text.len());
    let mut extra_close = 0usize;
    for ch in text.chars() {
        match ch {
            '(' => {
                depth += 1;
                stripped.push(ch);
            }
            ')' => {
                if depth > 0 {
                    depth -= 1;
                    stripped.push(ch);
                } else {
                    extra_close += 1;
                }
            }
            _ => stripped.push(ch),
        }
    }
    let mut closed = String::from(text);
    let mut samplable = stripped;
    for _ in 0..depth {
        closed.push(')');
        samplable.push(')');
    }
    (closed, samplable, extra_close)
}

/// f64 位模式单调序数（负数翻过来），用来"挪 k 个 ulp"。
fn ord_of(x: f64) -> i64 {
    let b = x.to_bits() as i64;
    if b < 0 {
        i64::MIN - b
    } else {
        b
    }
}

fn of_ord(o: i64) -> f64 {
    let b = if o < 0 { i64::MIN - o } else { o } as u64;
    f64::from_bits(b)
}

fn shift_ulp(x: f64, k: i64) -> f64 {
    of_ord(ord_of(x).wrapping_add(k))
}

/// `(x, y)` 两点位模式，逗号分隔 —— 采样器的返回值。
fn jpt(p: Option<(f64, f64)>) -> String {
    match p {
        Some((x, y)) => format!("[{},{}]", jbits(x), jbits(y)),
        None => "null".into(),
    }
}

fn main() {
    let mut dir = gw2_rebuild::_scaffold::replaydiff::default_corpus_dir();
    let mut json_out: Option<std::path::PathBuf> = None;
    let mut limit = usize::MAX;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
            "--json" => json_out = Some(args.next().expect("--json 需要值").into()),
            "--limit" => limit = args.next().expect("--limit 需要值").parse().unwrap(),
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
            continue;
        }
        let inputs = Inputs::scan(&replay, batches[0].start);
        // 模式的时刻表（`FunctionModeInfo` 是输入、不是常量）
        let mut modes: Vec<(usize, String)> = Vec::new();
        for (i, m) in replay.msgs.iter().enumerate() {
            if m.kind == "FunctionModeInfo" {
                if let Some(s) = m.get_str("function_mode") {
                    modes.push((i, s));
                }
            }
        }
        let mode_at = |upto: usize| -> String {
            let mut cur = inputs.room.function_mode.clone();
            for (i, s) in &modes {
                if *i >= upto {
                    break;
                }
                cur = s.clone();
            }
            cur
        };

        for (bi, b) in batches.iter().enumerate() {
            if b.shots.is_empty() {
                continue;
            }
            let fm = mode_at(b.start);
            // `_scaffold::diffconv::mode_of` 就是 `game::sampling_mode(game::function_mode_of(_))`
            // 的公开封装（`FunctionMode` 本身是私有枚举，例子只能用这层）。
            let mode = gw2_rebuild::_scaffold::diffconv::mode_of(&fm);
            let win = &replay.msgs[b.start..=b.end.min(replay.msgs.len() - 1)];

            // ---- 观测：FunctionPoints.end_x / EffectInfo(Explosion) / FunctionInfo ----
            let mut end_x: Vec<(i64, f64)> = Vec::new();
            let mut expl: Vec<(f64, f64)> = Vec::new();
            let mut finfo: Vec<(i64, String, Option<String>)> = Vec::new();
            for m in win {
                match m.kind.as_str() {
                    "FunctionPoints" => {
                        if let (Some(o), Some(x)) = (m.get_i64("owner_id"), m.get_f64("end_x")) {
                            end_x.push((o, x));
                        }
                    }
                    "EffectInfo" => {
                        if m.get_str("effect_type").as_deref() == Some("ExplosionEffect") {
                            if let (Some(x), Some(y)) = (m.get_f64("pos_x"), m.get_f64("pos_y")) {
                                expl.push((x, y));
                            }
                        }
                    }
                    "FunctionInfo" => {
                        if let Some(o) = m.get_i64("owner_id") {
                            finfo.push((
                                o,
                                m.get_str("function").unwrap_or_default(),
                                m.get_str("corrected_function"),
                            ));
                        }
                    }
                    _ => {}
                }
            }

            let mut items: Vec<String> = Vec::new();
            for (si, s) in b.shots.iter().enumerate() {
                // 观测：这一发的 end_x / 爆炸点 / 两串文本
                let oe = end_x
                    .iter()
                    .find(|(o, _)| *o == s.owner)
                    .map(|(_, x)| *x);
                let op = expl.get(si).copied();
                let ofi = finfo.iter().find(|(o, _, _)| *o == s.owner);
                let obs_fn = ofi.map(|(_, f, _)| f.clone());
                let obs_cf = ofi.and_then(|(_, _, c)| c.clone());

                // ---- 我方链：c（全精度）→ 3 位小数文本 → parse ----
                let (closed, samplable, extra_close) = balanced_texts(&s.text);
                let toks_samp = parse(&samplable);
                let c_full = correction(&toks_samp, s.x0, s.y0, s.flip);
                let suffix = correction_text(&toks_samp, s.x0, s.y0, s.flip);
                let our_closed = format!("{closed}{suffix}");
                let our_samp = format!("{samplable}{suffix}");
                // `c_print` = 把印出来的那串**再解析**一次（= 采样器真正吃到的常数）
                let c_print = if suffix.trim() == "NaN" {
                    f64::NAN
                } else {
                    eval(&parse(suffix.trim()), 0.0, 0.0, 0.0)
                };
                // 回代的 x：修正文本写在射手自己的坐标系里，flip 的射手要把 x 取反
                let sx = |x: f64| if s.flip { -x } else { x };

                // ---- 候选值 ----
                // A / B：回放自己的串（§25 判据；B 只是把回代点换成 end_x）
                let toks_obs = obs_cf.as_deref().map(parse);
                let a = toks_obs
                    .as_ref()
                    .zip(op)
                    .map(|(t, (px, _))| eval(t, sx(px), 0.0, 0.0));
                let b_cand = toks_obs
                    .as_ref()
                    .zip(oe)
                    .map(|(t, ex)| eval(t, sx(ex), 0.0, 0.0));
                // C：全精度常数（前置 Add，数值上与真值后置 Sub/Add 相同 —— §26.5 ①）
                let toks_full = gw2_rebuild::graphwar_shot_calculator::shifted_tokens(
                    &parse(&s.text),
                    c_full,
                );
                let c_cand = op.map(|(px, _)| eval(&toks_full, sx(px), 0.0, 0.0));
                // D：我方发出去的串
                let toks_mine = parse(&our_closed);
                let d_cand = op.map(|(px, _)| eval(&toks_mine, sx(px), 0.0, 0.0));
                // E / F / G：三种采样器输入（同时把**整条点列**留下来，`Ode` 局要在里面找 x）
                let sample = |text: &str, apply: bool| -> Vec<(f64, f64)> {
                    shot_points(
                        text,
                        s.x0,
                        s.y0,
                        gw2_rebuild::SERVER_STEP,
                        s.flip,
                        mode,
                        apply,
                    )
                };
                let e_series = sample(&our_samp, false);
                let f_series = sample(&s.text, true);
                let g_series = obs_cf.as_deref().map(|t| sample(t, false)).unwrap_or_default();
                let e_pt = e_series.last().copied();
                let f_pt = f_series.last().copied();
                let g_pt = g_series.last().copied();
                // 「回放的点落在我们这条点列的哪一格」——`x` 逐位（`Ode` 局唯一的复算法）
                let hit = |series: &[(f64, f64)]| -> Option<(usize, f64)> {
                    let px = op.map(|(x, _)| x)?;
                    series
                        .iter()
                        .position(|p| p.0.to_bits() == px.to_bits())
                        .map(|k| (k, series[k].1))
                };
                let g_hit = hit(&g_series);
                let e_hit = hit(&e_series);
                // 「回放那个 y 到底能不能被**同一个树**解释」：把 `x` 挪 k 个 ulp 再代，
                // k 取 -8..=8。全都不中 ⇒ 这批的 `pos_y` 不是"回放自己的文本 + 自己的 x"能算出来的。
                let sx_shift: Option<i64> = op.and_then(|(px, _)| {
                    let base = if s.flip { -px } else { px };
                    // k 按 |k| 递增扫（先 k=0），否则 -8 会先命中，报出来的位移全偏
                    [0i64, -1, 1, -2, 2, -3, 3, -4, 4, -5, 5, -6, 6, -7, 7, -8, 8]
                        .iter()
                        .copied()
                        .find(|k| {
                        let v = if toks_obs.is_some() {
                            eval(toks_obs.as_ref().unwrap(), shift_ulp(base, *k), 0.0, 0.0)
                        } else {
                            f64::NAN
                        };
                        op.map(|(_, y)| v.to_bits() == y.to_bits()).unwrap_or(false)
                        })
                });

                let same_text = match &obs_cf {
                    Some(t) => t == &our_closed,
                    None => false,
                };
                let same_samp = match &obs_cf {
                    Some(t) => t == &our_samp,
                    None => false,
                };

                items.push(format!(
                    concat!(
                        "{{\"owner\":{},\"x0\":{},\"y0\":{},\"flip\":{},\"mode\":{},",
                        "\"text\":{},\"obs_fn\":{},\"obs_cf\":{},",
                        "\"obs_end_x\":{},\"obs_pos_x\":{},\"obs_pos_y\":{},",
                        "\"extra_close\":{},\"same_text\":{},\"same_samp\":{},",
                        "\"c_full\":{},\"c_full_bits\":{},\"c_print\":{},\"c_print_bits\":{},",
                        "\"suffix\":{},",
                        "\"A\":{},\"B\":{},\"C\":{},\"D\":{},",
                        "\"E\":{},\"F\":{},\"G\":{},",
                        "\"En\":{},\"Gn\":{},\"Gx\":{},\"Ex\":{},\"Sx\":{}}}"
                    ),
                    s.owner,
                    jnum(s.x0),
                    jnum(s.y0),
                    s.flip,
                    jstr(if mode == Mode::Ode { "Ode" } else { "Normal" }),
                    jstr(&s.text),
                    match &obs_fn {
                        Some(t) => jstr(t),
                        None => "null".into(),
                    },
                    match &obs_cf {
                        Some(t) => jstr(t),
                        None => "null".into(),
                    },
                    match oe {
                        Some(v) => jnum(v),
                        None => "null".into(),
                    },
                    match op {
                        Some((x, _)) => jnum(x),
                        None => "null".into(),
                    },
                    match op {
                        Some((_, y)) => jnum(y),
                        None => "null".into(),
                    },
                    extra_close,
                    same_text,
                    same_samp,
                    jnum(c_full),
                    jbits(c_full),
                    jnum(c_print),
                    jbits(c_print),
                    jstr(&suffix),
                    match a {
                        Some(v) => jbits(v),
                        None => "null".into(),
                    },
                    match b_cand {
                        Some(v) => jbits(v),
                        None => "null".into(),
                    },
                    match c_cand {
                        Some(v) => jbits(v),
                        None => "null".into(),
                    },
                    match d_cand {
                        Some(v) => jbits(v),
                        None => "null".into(),
                    },
                    jpt(e_pt),
                    jpt(f_pt),
                    jpt(g_pt),
                    e_series.len(),
                    g_series.len(),
                    match g_hit {
                        Some((k, y)) => format!("[{},{},{}]", k, jbits(y), g_series[k].0.to_bits()),
                        None => "null".into(),
                    },
                    match e_hit {
                        Some((k, y)) => format!("[{},{},{}]", k, jbits(y), e_series[k].0.to_bits()),
                        None => "null".into(),
                    },
                    match sx_shift {
                        Some(k) => k.to_string(),
                        None => "null".into(),
                    },
                ));
            }

            writeln!(
                sink,
                "{{\"file\":{},\"batch\":{},\"shots\":{},\"mode\":{},\"items\":[{}]}}",
                jstr(&name),
                bi,
                b.shots.len(),
                jstr(if mode == Mode::Ode { "Ode" } else { "Normal" }),
                items.join(",")
            )
            .unwrap();
        }
    }
}
