use gw2_rebuild::function::parse;
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::shot::{correction, shifted_tokens};

fn main() {
    let text = std::env::args().nth(1).unwrap_or_else(|| "abs(cos((-1.43*x)))*8.65".into());
    let x0: f64 = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(-1.34);
    let y0: f64 = std::env::args().nth(3).and_then(|v| v.parse().ok()).unwrap_or(7.57);
    let toks = parse(&text);
    let c = correction(&toks, x0, y0, false);
    let sh = shifted_tokens(&toks, c);
    let pts = calculate(&sh, x0, y0, gw2_rebuild::SERVER_STEP, false, Mode::Normal);
    println!("函数 {text}  起点 ({x0}, {y0})  c={c:.6}");
    println!("点数 {}  末点 {:?}", pts.len(), pts.last());
    for k in 1..=4 {
        if pts.len() > k {
            let i = pts.len() - k;
            println!("  倒数第 {k} 点 = ({:.4}, {:.4})", pts[i].0, pts[i].1);
        }
    }
    println!("预算 = {} 点", (200.0f64 / 0.5) as i32);
}
