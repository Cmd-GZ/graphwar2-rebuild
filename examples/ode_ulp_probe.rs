//! **`Ode` 局残差是不是"起点差 1 ulp"造成的**（`dispatch/40` 的后续探针）。
//!
//! ```text
//! cargo run --release --offline -q --example ode_ulp_probe -- \
//!     --fold <residual_fold 的逐发 JSONL> [--max 100] [--k 3]
//! ```
//!
//! ## 它为什么存在
//!
//! `dispatch/40` 在真机上抓到一条**新的**观测：`Mode::Ode` 那一发里，采样器被调用**两次**，
//! 两次的 `x0`（开火士兵的 x）**恰好差 1 ulp**（11 发 / 11 发，见 `docs/06` §26）。
//! 而把这两组入参分别喂给我方采样器，得到的**两条点数组也都与真机逐位相同**
//! （22 / 22 次调用，`examples/ode_array_check`）⇒ 我方积分**不是**误差来源。
//!
//! 于是问题变成：回放里那 100 发"`pos_x` 不落在我方网格上"的 `DiffEq` 发，
//! 是不是**回放的起点与我方拿到的起点差 1 ulp**（而不是曲线算错）？
//!
//! ## 判据
//!
//! 对每一条 `ode_found = false` 的 `DiffEq` 发，把 `x0` 挪 `k ∈ [-K, K]` 个 ulp 各积分一次，
//! 看回放的 `pos_x` 是否**逐位**落在这条点列的 x 上（`f64::to_bits` 相同）。
//! 只要某一个 `k ≠ 0` 让它落上 ⇒ 这一发的"不在网格上"**可以由起点的 1 ulp 差解释**。
//!
//! ⚠ 它只做加法：**不**证明真机当初喂的就是那个 `k`。`k` 是不是 `+1`、以及那个 1 ulp
//! 从哪来（`FunctionPoints.start_x` 的写法？两次调用取了不同的那个？），仍要下一轮定。
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::math_function::parse;

/// `x0` 沿"数值方向"挪 `k` 个 ulp（负数也按数值单调方向，不是按位模式）
fn shift_ulp(x: f64, k: i32) -> f64 {
    if k == 0 || !x.is_finite() {
        return x;
    }
    let bits = x.to_bits() as i64;
    // 位模式在同一符号内与数值单调；跨 0 的挪动按"最近的可表示值"处理
    let mut b = bits;
    for _ in 0..k.abs() {
        b += if (k > 0) == (x >= 0.0) { 1 } else { -1 };
    }
    f64::from_bits(b as u64)
}

fn get<'a>(o: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{}\":", key);
    let i = o.find(&pat)? + pat.len();
    Some(o[i..].trim_start())
}
fn get_str<'a>(o: &'a str, key: &str) -> Option<&'a str> {
    let rest = get(o, key)?;
    let s = rest.strip_prefix('"')?;
    Some(&s[..s.find('"')?])
}
fn get_num(o: &str, key: &str) -> Option<f64> {
    let rest = get(o, key)?;
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}
fn get_bool(o: &str, key: &str) -> Option<bool> {
    let rest = get(o, key)?;
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn main() {
    let mut fold: Option<String> = None;
    let mut max_rows = usize::MAX;
    let mut k_max = 3i32;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--fold" => fold = args.next(),
            "--max" => max_rows = args.next().and_then(|v| v.parse().ok()).unwrap_or(usize::MAX),
            "--k" => k_max = args.next().and_then(|v| v.parse().ok()).unwrap_or(3),
            "-h" | "--help" => {
                println!("usage: ode_ulp_probe --fold <fold.jsonl> [--max N] [--k 3]");
                return;
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }
    let Some(fold) = fold else {
        eprintln!("need --fold <residual_fold 的 --json 输出>");
        std::process::exit(2);
    };
    let src = std::fs::read_to_string(&fold).unwrap_or_else(|e| {
        eprintln!("read {fold}: {e}");
        std::process::exit(1);
    });

    // k -> 命中数；k = i32::MIN 表示"任何 k 都没命中"
    let mut hit_by_axis_k: std::collections::BTreeMap<(char, i32), usize> =
        std::collections::BTreeMap::new();
    let mut none = 0usize;
    let mut n = 0usize;
    let mut rows: Vec<String> = Vec::new();
    for line in src.lines() {
        if n >= max_rows {
            break;
        }
        if !line.contains("\"mode\":\"DiffEq\"") {
            continue;
        }
        if get_bool(line, "ode_found") != Some(false) {
            continue;
        }
        let Some(fn_text) = get_str(line, "fn") else { continue };
        let (Some(x0), Some(y0), Some(px)) =
            (get_num(line, "x0"), get_num(line, "y0"), get_num(line, "pos_x"))
        else {
            continue;
        };
        let flip = get_bool(line, "flip").unwrap_or(false);
        let toks = parse(fn_text);
        n += 1;
        // 起点有**两个**分量，两个都试：本轮实测两次调用差 1 ulp 的那一位
        // 11 发落在 `x0`、另一次独立复跑落在 `y0`（docs/06 §26.4）
        let mut found: Option<(char, i32)> = None;
        'outer: for axis in ['x', 'y'] {
            for k in -k_max..=k_max {
                if k == 0 {
                    continue;
                }
                let (xa, ya) = if axis == 'x' {
                    (shift_ulp(x0, k), y0)
                } else {
                    (x0, shift_ulp(y0, k))
                };
                let pts = calculate(&toks, xa, ya, gw2_rebuild::SERVER_STEP, flip, Mode::Ode);
                if pts.iter().any(|p| p.0.to_bits() == px.to_bits()) {
                    found = Some((axis, k));
                    break 'outer;
                }
            }
        }
        match found {
            Some((axis, k)) => *hit_by_axis_k.entry((axis, k)).or_insert(0) += 1,
            None => none += 1,
        }
        rows.push(format!(
            "{{\"n\":{},\"axis\":\"{}\",\"k\":{},\"pos_x\":{},\"x0\":{},\"y0\":{}}}",
            n,
            found.map(|(a, _)| a).unwrap_or('-'),
            found.map(|(_, v)| v as i64).unwrap_or(-999),
            px,
            x0,
            y0
        ));
    }
    let total_hit: usize = hit_by_axis_k.values().sum();
    eprintln!(
        "[ode_ulp_probe] 检查 {} 发 `ode_found=false` 的 DiffEq 发 ｜ 按 (轴, k) 命中 {:?} ｜ 命中 {} ｜ 任何 (轴,k) 都不中 {}",
        n, hit_by_axis_k, total_hit, none
    );
    for r in &rows {
        println!("{r}");
    }
}
