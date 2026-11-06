use gw2_rebuild::math_function::parse;
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::graphwar_shot_calculator::{correction, shifted_tokens};
fn main() {
    let text = "abs(cos((-1.43*x)))*8.65";
    for &step in &[0.5f64, 0.1, 0.01] {
        for &mode in &[Mode::Normal, Mode::Ode] {
            let toks = parse(text);
            let (x0, y0) = (-1.34, 7.57);
            let c = correction(&toks, x0, y0, false);
            let sh = shifted_tokens(&toks, c);
            let pts = calculate(&sh, x0, y0, step, false, mode);
            println!("step={step:<5} {mode:?}\t点数 {:>5}\t末点 ({:.3}, {:.3})",
                     pts.len(), pts.last().unwrap().0, pts.last().unwrap().1);
        }
    }
}
