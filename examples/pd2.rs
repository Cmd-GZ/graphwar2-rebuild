use gw2_rebuild::math_function::parse;
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::graphwar_shot_calculator::{correction, shifted_tokens};
fn main() {
    let text = "abs(cos((-1.43*x)))*8.65";
    for &(x0, y0, flip) in &[(-1.34f64, 7.57f64, false), (-1.34, 7.57, true), (1.34, 7.57, true), (1.34, 7.57, false)] {
        let toks = parse(text);
        let c = correction(&toks, x0, y0, flip);
        let sh = shifted_tokens(&toks, c);
        let pts = calculate(&sh, x0, y0, gw2_rebuild::SERVER_STEP, flip, Mode::Normal);
        println!("flip={flip:<5} x0={x0:<6} 点数 {:>3}  末点 ({:.3}, {:.3})",
                 pts.len(), pts.last().unwrap().0, pts.last().unwrap().1);
    }
}
