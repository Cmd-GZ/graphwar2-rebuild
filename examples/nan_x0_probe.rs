//! 探针：把一次开火的「修正量 + 采样」按给定 x0 逐位重放一遍。
//!
//! 用途（task: `C` 桶 NaN 成因）：同一发函数、同一 y0，只把 x0 在 ±1 ulp 内挪动，
//! 看 `end_x`（我们发出去的 `FunctionPoints.end_x`）与 `c` 各是什么。
//! 全部输入用 **hex bits** 传，避免十进制往返掩盖 1 ulp。
//!
//! ```text
//! cargo run --release --offline --example nan_x0_probe -- '<text>' <x0bits> <y0bits> [flip]
//! ```
use gw2_rebuild::graphwar_path_calculator::Mode;
use gw2_rebuild::graphwar_shot_calculator::{correction, correction_text, shot_points};
use gw2_rebuild::math_function::parse;

fn main() {
    let mut a = std::env::args().skip(1);
    let text = a.next().expect("text");
    let xb = u64::from_str_radix(&a.next().expect("x0 bits"), 16).expect("x0 bits hex");
    let yb = u64::from_str_radix(&a.next().expect("y0 bits"), 16).expect("y0 bits hex");
    let flip = a.next().map(|v| v == "1").unwrap_or(false);
    let x0 = f64::from_bits(xb);
    let y0 = f64::from_bits(yb);
    println!("text = {text}");
    println!("y0   = {y0:?} bits={yb:016x}  flip={flip}");

    let toks = parse(&text);
    // 候选 x0：原值、向零 1 ulp、向外 1 ulp
    let candidates = [
        ("bits", x0),
        ("toward0", f64::from_bits(xb - 1)),
        ("away", f64::from_bits(xb + 1)),
    ];
    for (tag, xc) in candidates {
        let c = correction(&toks, xc, y0, flip);
        let ctext = correction_text(&toks, xc, y0, flip);
        let corrected = format!("{text}{ctext}");
        let pts = shot_points(&corrected, x0, y0, gw2_rebuild::SERVER_STEP, flip, Mode::Normal, false);
        let last = pts.last().copied();
        println!(
            "  x0[{tag:8}] = {:?}  c = {:?} bits={:016x} text='{}'  npts={} end={:?}",
            xc,
            c,
            c.to_bits(),
            ctext,
            pts.len(),
            last
        );
    }
}
