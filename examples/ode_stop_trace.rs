//! `Ode` 停止判据**追踪器**（`dispatch/29`）：把采样器的循环原样重跑一遍并**逐次记账**
//! （每次接受用了多少次二分、最后一次为什么停）。
//!
//! 目的是回答"停止判据到底是什么"这一类问题：只看最后一个点看不出是
//! **越界 `|x|>25`**、**越界 `|y|>15`**、**二分不收敛**、**NaN** 还是**点数预算用尽**。
//!
//! ```text
//! cargo run --release --offline -q --example ode_stop_trace -- \
//!     --text "<采样串>" --x0 <f64> --y0 <f64> [--flip] [--normal] [--tail 8]
//! cargo run --release --offline -q --example ode_stop_trace -- \
//!     --dir <语料> --file <名字> --batch <N> [--tail 8]
//! ```
//!
//! ⚠ **自校验**：本工具内联复算一遍，然后与 `graphwar_path_calculator::calculate`
//! 的返回值**逐位比较**，不一致就 `panic` —— 追踪结果因此不是"另一个实现"的说法。
use gw2_rebuild::_scaffold::diffconv::{corrected_function, mode_of};
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Replay};
use gw2_rebuild::graphwar_path_calculator::{calculate, point_budget, Mode};
use gw2_rebuild::math_function::{eval, parse};
use gw2_rebuild::{LENGTH_TOL, MAX_ABS_X, MAX_ABS_Y, TOTAL_LEN};

/// 弦长：`sqrt(a*a + b*b)`（与 `math_function::m_hypot` 逐位同式；那边是 `pub(crate)`）
fn hypot(a: f64, b: f64) -> f64 {
    (a * a + b * b).sqrt()
}

const HALF: f64 = 0.5;
const SIXTH: f64 = 6.0;
const MAX_BISECT: usize = 100;

/// 停止原因（与二进制 `0x3c2530` 的五个出口一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    /// 100 次二分都没收敛（二进制 `0x3c282f` / `0x3c2835`）
    NoConverge,
    /// 候选点 NaN（二进制 `0x3c2881` 的 `jp`，比较 `cand_y` 与 `px`）
    Nan,
    /// `|mid| > 25`（二进制 `0x3c2898` / `0x3c28a0`）
    OverX,
    /// `|cand_y| > 15`（二进制 `0x3c28b4` / `0x3c28bb`）
    OverY,
    /// 点数用尽（二进制 `0x3c2915`）
    Budget,
}

/// 一次接受的记账。
struct Acc {
    iters: u32,
    /// 最后一轮二分时 `|chord - step|` 的值（看不清收敛质量时用）
    resid: f64,
}

fn trace(
    tokens: &[gw2_rebuild::Token],
    x0: f64,
    y0: f64,
    step: f64,
    flip: bool,
    mode: Mode,
) -> (Vec<(f64, f64)>, Vec<Acc>, Stop) {
    let mut pts = vec![(x0, y0)];
    let mut accs: Vec<Acc> = Vec::new();
    let mut x = if flip { -x0 } else { x0 };
    let mut y = y0;
    let n = point_budget(step);
    let mut count: i32 = 0;
    loop {
        let mut lo = x;
        let mut hi = x + step;
        let mut accepted = false;
        let mut used = 0u32;
        let mut last_resid = f64::NAN;
        for k in 0..MAX_BISECT {
            used = k as u32 + 1;
            let mid = (lo + hi) * HALF;
            let (cand_y, chord) = match mode {
                Mode::Ode => {
                    let h = mid - x;
                    let k1 = eval(tokens, x, y, 0.0);
                    let k2 = eval(tokens, x + h * HALF, y + k1 * h * HALF, 0.0);
                    let k3 = eval(tokens, x + h * HALF, y + k2 * h * HALF, 0.0);
                    let k4 = eval(tokens, x + h, y + k3 * h, 0.0);
                    let yn = y + (h / SIXTH) * (k1 + 2.0 * k2 + 2.0 * k3 + k4);
                    (yn, hypot(h, yn - y))
                }
                Mode::Normal => {
                    let fm = eval(tokens, mid, 0.0, 0.0);
                    (fm, hypot(mid - x, fm - y))
                }
            };
            last_resid = (chord - step).abs();
            if !(LENGTH_TOL > last_resid) {
                if chord > step || chord.is_nan() {
                    hi = mid;
                } else {
                    lo = mid;
                }
                continue;
            }
            if mode == Mode::Normal {
                let mid2 = (x + mid) * HALF;
                let fm2 = eval(tokens, mid2, 0.0, 0.0);
                let chord2 = hypot(mid2 - x, fm2 - y);
                if !(chord > chord2) {
                    lo = x;
                    hi = mid2;
                    continue;
                }
            }
            let px = if flip { -mid } else { mid };
            if cand_y.is_nan() || px.is_nan() {
                return (pts, accs, Stop::Nan);
            }
            if mid.abs() > MAX_ABS_X {
                return (pts, accs, Stop::OverX);
            }
            if cand_y.abs() > MAX_ABS_Y {
                return (pts, accs, Stop::OverY);
            }
            pts.push((px, cand_y));
            accs.push(Acc { iters: used, resid: last_resid });
            count += 1;
            x = mid;
            y = cand_y;
            accepted = true;
            break;
        }
        if !accepted {
            return (pts, accs, Stop::NoConverge);
        }
        if count == n {
            return (pts, accs, Stop::Budget);
        }
    }
}

fn main() {
    let mut dir: Option<std::path::PathBuf> = None;
    let mut file = String::new();
    let mut batch = usize::MAX;
    let mut text: Option<String> = None;
    let mut x0: Option<f64> = None;
    let mut y0: Option<f64> = None;
    let mut flip = false;
    let mut mode = Mode::Ode;
    let mut tail = 8usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = Some(args.next().expect("--dir 需要值").into()),
            "--file" => file = args.next().expect("--file 需要值"),
            "--batch" => batch = args.next().expect("--batch 需要值").parse().unwrap(),
            "--text" => text = Some(args.next().expect("--text 需要值")),
            "--x0" => x0 = Some(args.next().expect("--x0 需要值").parse().unwrap()),
            "--y0" => y0 = Some(args.next().expect("--y0 需要值").parse().unwrap()),
            "--flip" => flip = true,
            "--normal" => mode = Mode::Normal,
            "--ode" => mode = Mode::Ode,
            "--tail" => tail = args.next().expect("--tail 需要值").parse().unwrap(),
            other => {
                eprintln!("未知参数 {other}");
                std::process::exit(2);
            }
        }
    }
    let mut cases: Vec<(String, f64, f64, bool, Mode)> = Vec::new();
    if let Some(t) = text {
        cases.push((t, x0.expect("--x0"), y0.expect("--y0"), flip, mode));
    } else {
        let dir = dir.unwrap_or_else(gw2_rebuild::_scaffold::replaydiff::default_corpus_dir);
        let path = dir.join(&file);
        let replay =
            Replay::load(&path).unwrap_or_else(|e| panic!("读不了 {}: {e}", path.display()));
        let batches = split_batches(&replay);
        // 批前最后一个 `FunctionModeInfo`
        let mut mode_at = Mode::Normal;
        for m in &replay.msgs[..batches[batch].start] {
            if m.kind == "FunctionModeInfo" {
                if let Some(s) = m.get_str("function_mode") {
                    mode_at = mode_of(&s);
                }
            }
        }
        for sh in &batches[batch].shots {
            let corrected = corrected_function(&sh.text, sh.x0, sh.y0, sh.flip, mode_at);
            cases.push((corrected.clone(), sh.x0, sh.y0, sh.flip, mode_at));
            println!(
                "  owner={} 原始函数串 = {:?}（{} 字符）→ 采样串 = {:?}",
                sh.owner,
                sh.text,
                sh.text.chars().count(),
                corrected
            );
        }
    }
    for (t, sx, sy, sf, m) in cases {
        let tokens = parse(&t);
        let pts = calculate(&tokens, sx, sy, gw2_rebuild::SERVER_STEP, sf, m);
        let (tpts, accs, why) = trace(&tokens, sx, sy, gw2_rebuild::SERVER_STEP, sf, m);
        assert_eq!(
            pts.len(),
            tpts.len(),
            "追踪器与 calculate 点数不一致（追踪器不可信）"
        );
        for (a, b) in pts.iter().zip(tpts.iter()) {
            assert_eq!(a.0.to_bits(), b.0.to_bits(), "追踪器 x 与 calculate 不一致");
            assert_eq!(a.1.to_bits(), b.1.to_bits(), "追踪器 y 与 calculate 不一致");
        }
        println!(
            "模式={:?} x0={sx:?} y0={sy:?} flip={sf} 点数={} 预算={} 步长={:?}",
            m,
            pts.len(),
            point_budget(gw2_rebuild::SERVER_STEP),
            gw2_rebuild::SERVER_STEP
        );
        println!("  停止原因 = {why:?}");
        let from = tpts.len().saturating_sub(tail + 1);
        for i in from..tpts.len() {
            println!(
                "    pts[{i}] = ({:?}, {:?}){}",
                tpts[i].0,
                tpts[i].1,
                match i.checked_sub(1).and_then(|k| accs.get(k)) {
                    Some(a) => format!("   [二分 {} 次, |chord-step|={:.3e}]", a.iters, a.resid),
                    None => String::new(),
                }
            );
        }
        println!(
            "  预算核对：TOTAL_LEN/step = {:?} → {}",
            TOTAL_LEN / gw2_rebuild::SERVER_STEP,
            point_budget(gw2_rebuild::SERVER_STEP)
        );
    }
}
