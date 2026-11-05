use gw2_rebuild::function::parse;
use gw2_rebuild::path::{calculate, Mode};
use gw2_rebuild::shot::{correction, shifted_tokens};
fn main() {
    let toks = parse("(abs(cos((-1.43*x)))*8.65)");
    let (x0, y0) = (-1.3401100016801577f64, 7.565109751327434f64);
    let c = correction(&toks, x0, y0, false);
    println!("c = {c:.15}");
    for &step in &[0.5f64, 0.01] {
        let pts = calculate(&shifted_tokens(&toks, c), x0, y0, step, false, Mode::Normal);
        let l = pts.last().unwrap();
        println!("step={step:<5} npts={:<6} 末点 ({:.4}, {:.4})", pts.len(), l.0, l.1);
    }
}
