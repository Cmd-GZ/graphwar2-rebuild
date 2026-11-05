fn main() {
    use gw2_rebuild::function::parse;
    use gw2_rebuild::path::{calculate, Mode};
    use gw2_rebuild::shot::{correction, shifted_tokens};

    let cases = [
        ("tan((sin(((-1.03+x)/-2.74))+18.43))", 9.270754688284496, -0.41619794393105813, true),
        ("(-0.35*(-11.40+x))", 22.101336651501704, 8.783747930988948, true),
    ];
    for (text, x0, y0, flip) in cases {
        let toks = parse(text);
        let c = correction(&toks, x0, y0, flip);
        let shifted = shifted_tokens(&toks, c);
        let pts = calculate(&shifted, x0, y0, 0.5, flip, Mode::Normal);
        println!("\n{text}  x0={x0:.3} y0={y0:.3} flip={flip}");
        println!("  我们的修正常数 c = {c:.6}");
        println!("  点数 {}，终点 {:?}", pts.len(), pts.last());
        // 打印若干点，观察 y 量级
        for k in [1usize, 5, 10, 20, 30, 40] {
            if k < pts.len() {
                println!("  第 {:3} 点 = ({:.3}, {:.3})", k, pts[k].0, pts[k].1);
            }
        }
    }
}
