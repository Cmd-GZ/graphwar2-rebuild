use gw2_rebuild::function::parse;
use gw2_rebuild::path::{calculate, Mode};
use gw2_rebuild::shot::{correction, shifted_tokens};
fn main() {
    let text = std::env::args().nth(1).unwrap_or_else(|| "x/5".into());
    let x0: f64 = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let y0: f64 = std::env::args().nth(3).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let toks = parse(&text);
    let c = correction(&toks, x0, y0, false);
    let pts = calculate(&shifted_tokens(&toks, c), x0, y0, 0.5, false, Mode::Normal);
    println!("C {}", c);
    for (i, q) in pts.iter().enumerate().take(12) {
        println!("P {} {:.15} {:.15}", i, q.0, q.1);
    }
}
