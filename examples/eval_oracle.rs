//! 求值器对拍导出器：**每一发**一条 JSONL，给 unicorn oracle 用
//! （`tools/eval_oracle.py` 读它，在真二进制的 `evaluate_recursive` `0x4eb9d0` 上跑同一次求值）。
//!
//! ```text
//! cargo run --release --offline --example eval_oracle -- --dir <快照> --json out.jsonl
//! ```
//!
//! 每行字段：
//!
//! | 字段 | 含义 |
//! |---|---|
//! | `fn` / `tokens` | 回放里的 `corrected_function` 文本 + 我方解析出的前缀 token（`[tag, bits]`） |
//! | `x_bits` | 回代用的 x（`flip` 时取 `-end_x`），对应 `evaluate_recursive` 的 `xmm0` |
//! | `ours_bits` | 我方 `math_function::eval` 的结果 |
//! | `obs_y_bits` | 回放 `ExplosionEffect.pos_y`（真机值） |
//!
//! 口径与 `examples/posy_probe.rs` 的 `c_obs_h1` 完全一致（同一配对规则、同一个 x），
//! 只是多了 `tokens` 与 `ours_bits`，让 oracle 侧能**用真解析器 + 真求值器**重跑一遍。
use gw2_rebuild::_scaffold::diffconv::mode_of;
use gw2_rebuild::_scaffold::replaydiff::wire::Kind;
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Inputs, Msg, Replay, StateMask};
use gw2_rebuild::graphwar_path_calculator::Mode;
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
    format!("\"{:016x}\"", v.to_bits())
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

    let mut shots = 0usize;
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
        let mut mask = StateMask::new();
        mask.apply(&replay.msgs[..batches[0].start]);
        let mut cursor = batches[0].start;

        for (bi, b) in batches.iter().enumerate() {
            for m in &replay.msgs[cursor..b.start] {
                match m.kind.as_str() {
                    "FunctionModeInfo" => {
                        if let Some(s) = m.get_str("function_mode") {
                            mode = mode_of(&s);
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
            let mut obs_expl_y: Vec<f64> = Vec::new();
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
                        obs_expl_y.push(m.get_f64("pos_y").unwrap_or(f64::NAN));
                    }
                }
            }
            let obs_corrected: Vec<String> = observed
                .iter()
                .filter(|m| m.kind == "FunctionInfo")
                .map(|m| m.get_str("corrected_function").unwrap_or_default())
                .collect();

            for (si, shot) in b.shots.iter().enumerate() {
                shots += 1;
                let end_x = obs_end_x.get(&shot.owner).copied().unwrap_or(f64::NAN);
                let xa = if shot.flip { -end_x } else { end_x };
                let text = obs_corrected.get(si).cloned().unwrap_or_default();
                let toks = if text.is_empty() { Vec::new() } else { parse(&text) };
                // 空 token 序列照求值器语义给 0.0（`evaluate_recursive` 越界返回 0）
                let ours = eval(&toks, xa, 0.0, 0.0);
                let tok_json: Vec<String> = toks
                    .iter()
                    .map(|t| format!("[{},{}]", t.tag as u32, jbits(t.value)))
                    .collect();
                let oy = obs_expl_y.get(si).copied().unwrap_or(f64::NAN);
                writeln!(
                    sink,
                    concat!(
                        "{{\"file\":{},\"batch\":{},\"shot\":{},\"mode\":\"{}\",\"flip\":{},",
                        "\"x0\":{},\"y0\":{},\"obs_end_x\":{},\"x_bits\":{},",
                        "\"fn\":{},\"tokens\":[{}],\"ours_bits\":{},\"obs_y_bits\":{},\"obs_y\":{}}}"
                    ),
                    jstr(&name),
                    bi,
                    si,
                    if mode == Mode::Normal { "Normal" } else { "Ode" },
                    shot.flip,
                    jnum(shot.x0),
                    jnum(shot.y0),
                    jnum(end_x),
                    jbits(xa),
                    jstr(&text),
                    tok_json.join(","),
                    jbits(ours),
                    jbits(oy),
                    jnum(oy),
                )
                .unwrap();
            }
        }
    }
    eprintln!("发 {shots}");
}
