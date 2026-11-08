//! **`corrected_function` 含 NaN 的批**的全语料导出器（`docs/06-verification.md` §38，`dispatch/64`）。
//!
//! ```text
//! cargo run --release --offline --example nan_export -- --dir <快照> --json out.jsonl
//! ```
//!
//! 每发一行，只导出**判断 NaN 来源**所必需的那几个量（全部按 `f64::to_bits` 存 u64）：
//!
//! | 字段 | 是什么 |
//! |---|---|
//! | `x0` / `y0` | 报文 `FunctionPoints.start_x/start_y` 的位模式（= 我们喂给修正函数的点） |
//! | `flip` | 团队派生的方向（`Team2` = true） |
//! | `text` | `FunctionInfo.function` **原文** |
//! | `ours` | 我方 `corrected_function`（`diffconv::corrected_function`，与内核同一条规则） |
//! | `obs` | 回放自己的 `corrected_function`（批内 `FunctionInfo` 按出现顺序与 `FunctionPoints` 配对） |
//! | `c` | 我方算出来的修正常数（`graphwar_shot_calculator::correction`），`NaN` 时单独标出 |
//! | `ours_tz` | **反事实**：只把 `x0` 按幅值挪 1 ulp（向零，= `to_bits()-1`）后算出的同一串 |
//! | `c_tz` | 反事实的修正常数 |
//!
//! `ours_tz` 是**实验臂，不是实现**：`src/` 里的规则一个字都没改（`docs/06` §38；
//! 归档 `155-round-log` §42.4 已把"统一挪 1 ulp"这条补丁否掉）。放在这里是为了让
//! `tools/nan_batches.py` 能用**同一批输入**把两条规则并排摆出来比。
//!
//! 为什么要单开一个例子：`replay_attrib --json` 只吐**差异**，而"双方都印 NaN ⇒ 没有差异"
//! 的批（例如 `sin(0)/(0)`）在那边一条都不出现 —— 那是**正对照**，必须自己数出来。
//!
//! 归因（NaN 进入点、逐批判定）交给 `tools/nan_batches.py`（分桶规则要能随时改，不重新编译）。
use gw2_rebuild::_scaffold::diffconv::{corrected_function, mode_of};
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Inputs, Replay};
use gw2_rebuild::graphwar_shot_calculator::correction;
use std::io::Write;

fn jstr(s: &str) -> String {
    gw2_rebuild::_scaffold::replaydiff::wire::quote(s)
}

/// 按**幅值**挪 1 ulp 向零（= 位模式减一，两个符号同义）。`docs/06` §38 的唯一实验臂。
fn toward_zero_1ulp(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    f64::from_bits(x.to_bits() - 1)
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
    let mut names: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("--dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    names.sort();
    let mut sink: Box<dyn Write> = match &json_out {
        Some(p) => Box::new(std::io::BufWriter::new(
            std::fs::File::create(p).expect("--json 打不开"),
        )),
        None => Box::new(std::io::stdout()),
    };

    let (mut files_ok, mut files_err, mut batches, mut shots) = (0usize, 0usize, 0usize, 0usize);
    for path in &names {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let replay = match Replay::load(path) {
            Ok(r) => r,
            Err(_) => {
                files_err += 1;
                continue;
            }
        };
        let bs = split_batches(&replay);
        if bs.is_empty() {
            files_err += 1;
            continue;
        }
        files_ok += 1;
        let inputs = Inputs::scan(&replay, bs[0].start);
        let mut mode = mode_of(&inputs.room.function_mode);
        let mut cursor = 0usize;
        for (idx, b) in bs.iter().enumerate() {
            // 模式时刻表：与本批之前出现的 `FunctionModeInfo` 同步（`docs/06` §39）
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
            batches += 1;
            let tick = replay.msgs[b.start].tick;
            // 回放的修正文本：批内 `FunctionInfo` 按出现顺序（同一 owner 一批两发也不能按 id 建表）
            let obs: Vec<String> = replay.msgs[b.start..=b.end]
                .iter()
                .filter(|m| m.kind == "FunctionInfo")
                .map(|m| m.get_str("corrected_function").unwrap_or_default())
                .collect();
            // 回放自己的 `end_x` 与爆炸点：按**出现顺序**配对（NaN 那几批全是单发，顺序即身份）。
            // 用途：验证"`c` 是 NaN ⇒ 弹道退化成一个点（`end_x == start_x`、爆炸点 == 射手位置）"
            // 这条行为在原版里是否成立（`docs/06` §38.5）。
            let obs_endx: Vec<Option<f64>> = replay.msgs[b.start..=b.end]
                .iter()
                .filter(|m| m.kind == "FunctionPoints")
                .map(|m| m.get_f64("end_x"))
                .collect();
            let obs_expl: Vec<Option<(f64, f64)>> = replay.msgs[b.start..=b.end]
                .iter()
                .filter(|m| m.kind == "EffectInfo")
                .filter(|m| m.get_str("effect_type").as_deref() == Some("ExplosionEffect"))
                .map(|m| Some((m.get_f64("pos_x")?, m.get_f64("pos_y")?)))
                .collect();
            for (i, s) in b.shots.iter().enumerate() {
                shots += 1;
                let ours = corrected_function(&s.text, s.x0, s.y0, s.flip, mode);
                let obs_s = obs.get(i).cloned().unwrap_or_default();
                let toks = gw2_rebuild::math_function::parse(&s.text);
                let c = correction(&toks, s.x0, s.y0, s.flip);
                let x_tz = toward_zero_1ulp(s.x0);
                let ours_tz = corrected_function(&s.text, x_tz, s.y0, s.flip, mode);
                // 后缀单独导出：`corrected_function` = 括号补全后的原文 + 这一个后缀，
                // 而真二进制那个函数 `0x3c5f70` **只**产出后缀 ⇒ 比对只能比后缀。
                let suf = gw2_rebuild::graphwar_shot_calculator::correction_text(&toks, s.x0, s.y0, s.flip);
                let suf_tz = gw2_rebuild::graphwar_shot_calculator::correction_text(&toks, x_tz, s.y0, s.flip);
                // 回放那一串的后缀 = 去掉**同一个前缀**之后剩下的部分（前缀 = 补全括号后的原文）。
                // `Ode` 局没有后缀（`corrected_function == function`），所以前缀就是原文本身。
                let prefix: &str = match mode {
                    gw2_rebuild::graphwar_path_calculator::Mode::Normal => {
                        &ours[..ours.len().saturating_sub(suf.len())]
                    }
                    gw2_rebuild::graphwar_path_calculator::Mode::Ode => &ours[..],
                };
                let obs_suf: String = if !obs_s.is_empty() && obs_s.starts_with(prefix) {
                    obs_s[prefix.len()..].to_string()
                } else {
                    String::new()
                };
                let c_tz = correction(&toks, x_tz, s.y0, s.flip);
                writeln!(
                    sink,
                    "{{\"file\":{},\"batch\":{},\"tick\":{},\"shot\":{},\"owner\":{},\"n_owners\":{},\
                     \"mode\":{},\"flip\":{},\"x0\":{},\"y0\":{},\"text\":{},\"ours\":{},\"obs\":{},\
                     \"c_nan\":{},\"c\":{},\"ours_tz\":{},\"c_tz_nan\":{},\"c_tz\":{},\
                     \"suf\":{},\"suf_tz\":{},\"obs_suf\":{},\
                     \"obs_endx\":{},\"obs_expl\":{}}}",
                    jstr(&name),
                    idx,
                    tick,
                    i,
                    s.owner,
                    b.shots.len(),
                    jstr(if mode == gw2_rebuild::graphwar_path_calculator::Mode::Ode {
                        "DiffEq"
                    } else {
                        "Normal"
                    }),
                    s.flip,
                    s.x0.to_bits(),
                    s.y0.to_bits(),
                    jstr(&s.text),
                    jstr(&ours),
                    jstr(&obs_s),
                    c.is_nan(),
                    if c.is_finite() { format!("{c:?}") } else { "\"non-finite\"".into() },
                    jstr(&ours_tz),
                    c_tz.is_nan(),
                    if c_tz.is_finite() { format!("{c_tz:?}") } else { "\"non-finite\"".into() },
                    jstr(&suf),
                    jstr(&suf_tz),
                    jstr(&obs_suf),
                    match obs_endx.get(i).copied().flatten() {
                        Some(v) if v.is_finite() => format!("{}", v.to_bits()),
                        _ => "null".into(),
                    },
                    match obs_expl.get(i).copied().flatten() {
                        Some((px, py)) if px.is_finite() && py.is_finite() =>
                            format!("[{},{}]", px.to_bits(), py.to_bits()),
                        _ => "null".into(),
                    },
                )
                .unwrap();
            }
        }
    }
    eprintln!(
        "文件 {files_ok}（读失败 {files_err}）；结算批 {batches}；发 {shots}；输出 {:?}",
        json_out
    );
}
