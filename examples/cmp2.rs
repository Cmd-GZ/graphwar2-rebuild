use gw2_rebuild::function::parse;
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
fn main() {
    // 直接用"已修正"的完整文本，不再做 correction/shifted
    let text = "(abs(cos((-1.43*x)))*8.65) + 4.635142442441488";
    let pts = calculate(&parse(text), -1.3401100016801577, 7.565109751327434, 0.5, false, Mode::Normal);
    for (i, q) in pts.iter().enumerate().take(6) {
        println!("P {} {:.15} {:.15}", i, q.0, q.1);
    }
}
