//! 同时回合制（SimultaneousTurns）结算顺序分析用的**几何计算器**。
//!
//! 它自己不下结论：只把一个"一个 tick 内多发"的局面里的每一发单独算出来，
//! 并把「每发弹道第一次进入每个士兵半径的位置（采样下标 + 沿弧长）」吐出来，
//! 交由 Python 侧按不同假设（逐颗串行 / 全部并行 / 按飞行时间）做对照。
//!
//! 物理全部复用已验证的 `path::calculate` / `plane::rasterize` / `shot::correction`。
//!
//! 用法: `cargo run --release --example sim_turns -- <state.txt>`
//!
//! state 文件格式（由 `.rea-work/extract_sim.py` 生成）:
//!
//! ```text
//! MODE NormalFunction
//! STEP 0.5
//! SHOT <idx> <owner> <x0> <y0> <flip> <func...>
//! T <id> <x> <y> <r>
//! O <x> <y> <r>
//! H <x> <y> <r>
//! ```
//!
//! 输出（每发一段）:
//!
//! ```text
//! SHOT <idx> <owner> <npts> <stopi> <blocked> <arclen_stop> <stopx> <stopy>
//! HIT <idx> <tid> <first_i> <arclen_i> <mindist>
//! ```
use gw2_rebuild::function::parse;
use gw2_rebuild::path::{calculate, Mode};
use gw2_rebuild::plane::{empty_mask, is_occupied, rasterize};
use gw2_rebuild::shot::{correction, shifted_tokens};

struct Shot {
    idx: usize,
    owner: i64,
    x0: f64,
    y0: f64,
    flip: bool,
    func: String,
}

#[derive(Clone, Copy)]
struct Tgt {
    id: i64,
    x: f64,
    y: f64,
    r: f64,
}

fn main() {
    let path = std::env::args().nth(1).expect("需要 state 文件路径");
    let text = std::fs::read_to_string(&path).expect("读不了 state 文件");

    let mut mode = Mode::Normal;
    let mut step = 0.5f64;
    let mut shots: Vec<Shot> = Vec::new();
    let mut targets: Vec<Tgt> = Vec::new();
    let mut obs: Vec<(f64, f64, f64)> = Vec::new();
    let mut holes: Vec<(f64, f64, f64)> = Vec::new();

    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("MODE") => {
                if it.next() == Some("DiffEqFunction") {
                    mode = Mode::Ode;
                }
            }
            Some("STEP") => step = it.next().unwrap().parse().unwrap(),
            Some("SHOT") => {
                let idx: usize = it.next().unwrap().parse().unwrap();
                let owner: i64 = it.next().unwrap().parse().unwrap();
                let x0: f64 = it.next().unwrap().parse().unwrap();
                let y0: f64 = it.next().unwrap().parse().unwrap();
                let flip = it.next().unwrap() == "1";
                let func = it.collect::<Vec<_>>().join(" ");
                shots.push(Shot { idx, owner, x0, y0, flip, func });
            }
            Some("T") => targets.push(Tgt {
                id: it.next().unwrap().parse().unwrap(),
                x: it.next().unwrap().parse().unwrap(),
                y: it.next().unwrap().parse().unwrap(),
                r: it.next().unwrap().parse().unwrap(),
            }),
            Some("O") => obs.push((
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
            )),
            Some("H") => holes.push((
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
            )),
            _ => {}
        }
    }

    let mut mask = empty_mask();
    for &(x, y, r) in &obs {
        rasterize(&mut mask, x, y, r, 0x00);
    }
    for &(x, y, r) in &holes {
        rasterize(&mut mask, x, y, r, 0xFF);
    }

    for s in &shots {
        let toks = parse(&s.func);
        if toks.is_empty() {
            println!("SHOT {} {} 0 0 0 0 nan nan", s.idx, s.owner);
            continue;
        }
        let mut pts: Vec<(f64, f64)> = if mode == Mode::Normal {
            let c = correction(&toks, s.x0, s.y0, s.flip);
            let shifted = shifted_tokens(&toks, c);
            calculate(&shifted, s.x0, s.y0, step, s.flip, Mode::Normal)
        } else {
            calculate(&toks, s.x0, s.y0, step, s.flip, Mode::Ode)
        };
        if pts.is_empty() {
            pts.push((s.x0, s.y0));
        }

        // 累计弦长（子弹沿采样点匀速飞行 ⇒ 弧长与飞行时间成正比）
        let mut cum = Vec::with_capacity(pts.len());
        let mut acc = 0.0f64;
        for i in 0..pts.len() {
            if i > 0 {
                let (ax, ay) = pts[i - 1];
                let (bx, by) = pts[i];
                acc += ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
            }
            cum.push(acc);
        }

        let mut hits: Vec<(i64, usize, f64, f64)> = Vec::new(); // (tid, first_i, arclen, mindist)
        let mut stopi = pts.len() - 1;
        let mut blocked = false;
        for (i, &(px, py)) in pts.iter().enumerate() {
            for t in &targets {
                if t.id == s.owner || hits.iter().any(|h| h.0 == t.id) {
                    continue;
                }
                let d = ((px - t.x).powi(2) + (py - t.y).powi(2)).sqrt();
                if d <= t.r {
                    hits.push((t.id, i, cum[i], d));
                }
            }
            if is_occupied(&mask, px, py) {
                stopi = i;
                blocked = true;
                break;
            }
        }
        // 只保留发生在停点之前（含停点）的命中
        hits.retain(|h| h.1 <= stopi);

        let (sx, sy) = pts[stopi];
        println!(
            "SHOT {} {} {} {} {} {:.6} {:.6} {:.6}",
            s.idx,
            s.owner,
            pts.len(),
            stopi,
            if blocked { 1 } else { 0 },
            cum[stopi],
            sx,
            sy
        );
        for (tid, i, arc, d) in &hits {
            println!("HIT {} {} {} {:.6} {:.6}", s.idx, tid, i, arc, d);
        }
    }
}
