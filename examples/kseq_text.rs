//! 一次性探针：回放里**原版自己写的** `corrected_function`，以及我们把它代进采样器 /
//! 把"原文 + 各种修正常数"代进采样器，各自得到什么端点。
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Replay};
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::graphwar_shot_calculator::{correction, shifted_tokens};
use gw2_rebuild::math_function::{parse, to_text};
use gw2_rebuild::SERVER_STEP;

fn endpoint(label: &str, text: &str, x0: f64, y0: f64, flip: bool, shift: Option<f64>) {
    let mut toks = parse(text);
    if let Some(c) = shift {
        toks = shifted_tokens(&toks, c);
    }
    let pts = calculate(&toks, x0, y0, SERVER_STEP, flip, Mode::Normal);
    println!(
        "  {label:<30} toks={:<4} pts={:<6} end={:?}",
        toks.len(),
        pts.len(),
        pts.last()
    );
    println!("      tree={:?}", to_text(&toks));
}

fn main() {
    let mut a: Vec<String> = std::env::args().skip(1).collect();
    a.reverse();
    let mut dir = String::new();
    let mut file = String::new();
    let mut batch: Option<usize> = None;
    let mut expr = String::new();
    let mut ex0 = 0.0f64;
    let mut ey0 = 0.0f64;
    let mut eflip = false;
    let mut ec: Option<f64> = None;
    while let Some(k) = a.pop() {
        match k.as_str() {
            "--dir" => dir = a.pop().unwrap(),
            "--file" => file = a.pop().unwrap(),
            "--batch" => batch = Some(a.pop().unwrap().parse().unwrap()),
            "--text" => expr = a.pop().unwrap(),
            "--x0" => ex0 = a.pop().unwrap().parse().unwrap(),
            "--y0" => ey0 = a.pop().unwrap().parse().unwrap(),
            "--flip" => eflip = a.pop().unwrap() == "true",
            "--c" => ec = Some(a.pop().unwrap().parse().unwrap()),
            o => panic!("未知 {o}"),
        }
    }
    if !expr.is_empty() {
        endpoint("expr", &expr, ex0, ey0, eflip, ec);
        return;
    }
    let replay = Replay::load(&std::path::Path::new(&dir).join(&file)).unwrap();
    let batches = split_batches(&replay);
    let idx = batch.unwrap();
    let b = &batches[idx];
    println!("== {file} batch {idx}");
    for m in &replay.msgs[b.start..=b.end] {
        if m.kind == "FunctionInfo" {
            println!(
                "  FunctionInfo owner={:?}\n    function           = {:?}\n    corrected_function = {:?}",
                m.get_i64("owner_id"),
                m.get_str("function").unwrap_or_default(),
                m.get_str("corrected_function")
            );
        }
        if m.kind == "EffectInfo" && m.get_str("effect_type").as_deref() == Some("ExplosionEffect") {
            println!(
                "    observed Explosion pos=({:?},{:?})",
                m.get_f64("pos_x"),
                m.get_f64("pos_y")
            );
        }
    }
    for s in &b.shots {
        println!(
            "  shot owner={} x0={:?} y0={:?} flip={} text={:?}",
            s.owner, s.x0, s.y0, s.flip, s.text
        );
        let orig = parse(&s.text);
        let c = correction(&orig, s.x0, s.y0, s.flip);
        let printed = format!("{c:.3}").parse::<f64>().unwrap();
        println!("    c(exact)={c:?}  c(printed)={printed:?}");
        let cf = replay.msgs[b.start..=b.end]
            .iter()
            .filter(|m| m.kind == "FunctionInfo")
            .filter(|m| m.get_i64("owner_id") == Some(s.owner))
            .filter_map(|m| m.get_str("corrected_function"))
            .next();
        if let Some(cf) = cf {
            endpoint("replay corrected_function", &cf, s.x0, s.y0, s.flip, None);
        }
        endpoint("orig + exact c (old code)", &s.text, s.x0, s.y0, s.flip, Some(c));
        endpoint("orig + printed c", &s.text, s.x0, s.y0, s.flip, Some(printed));
        let suffix = if c >= 0.0 {
            format!(" + {c:.3}")
        } else {
            format!(" - {:.3}", -c)
        };
        endpoint(
            "ours corrected_text",
            &format!("{}{}", s.text, suffix),
            s.x0,
            s.y0,
            s.flip,
            None,
        );
    }
}
