//! **一次性探针**（不属于评测工具链，随 `agent/kseq-audit` 入库只为可复现）：
//! 对一份回放的**指定结算批**，把"改前（原文 + 全精度 `c`）"与"改后（修正文本）"
//! 两条弹道并排打印，并给出它们到**每一个士兵**的最近距离。
//!
//! 用途：`docs/06` §26.7 第 3 条登记的"4 批种类序列由对变错、都少判死者"——
//! 到底是"新实现把弹道算歪了"，还是"新弹道更准、只是致死判据在阈值上"。
//!
//! ```text
//! cargo run --release --offline --example kseq_probe -- --dir <语料> --file 1783429569 --batch 3
//! ```

use gw2_rebuild::_scaffold::replaydiff::{split_batches, Inputs, Replay};
use gw2_rebuild::graphwar_path_calculator::Mode;
use gw2_rebuild::graphwar_shot_calculator::settle::Target;
use gw2_rebuild::graphwar_shot_calculator::{corrected_text, shot_points};
use gw2_rebuild::SERVER_STEP;

fn main() {
    let mut dir = String::new();
    let mut file = String::new();
    let mut batch: Option<usize> = None;
    let mut a: Vec<String> = std::env::args().skip(1).collect();
    a.reverse();
    while let Some(k) = a.pop() {
        match k.as_str() {
            "--dir" => dir = a.pop().expect("--dir 需要值"),
            "--file" => file = a.pop().expect("--file 需要值"),
            "--batch" => batch = Some(a.pop().expect("--batch 需要值").parse().expect("批号")),
            other => panic!("未知参数 {other}"),
        }
    }
    let path = std::path::Path::new(&dir).join(&file);
    let replay = Replay::load(&path).expect("读回放");
    let batches = split_batches(&replay);
    let inputs = Inputs::scan(&replay, batches[0].start);

    let targets: Vec<Target> = inputs
        .pos
        .iter()
        .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
        .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
        .collect();

    for (idx, b) in batches.iter().enumerate() {
        if let Some(want) = batch {
            if idx != want {
                continue;
            }
        }
        println!("== {file} batch {idx}  tick={}", replay.msgs[b.start].tick);
        for s in &b.shots {
            println!(
                "  shot owner={} x0={:?} y0={:?} flip={} text={:?}",
                s.owner, s.x0, s.y0, s.flip, s.text
            );
            println!("    corrected = {:?}", corrected_text(&s.text, s.x0, s.y0, s.flip));
            // 改后：修正文本 + **不再**平移（`src/game.rs::fire_functions` 现在这条）
            let newp = shot_points(
                &corrected_text(&s.text, s.x0, s.y0, s.flip),
                s.x0,
                s.y0,
                SERVER_STEP,
                s.flip,
                Mode::Normal,
                false,
            );
            // 改前：原文 + 全精度 c 平移（`settle::resolve_owned` 内部那条）
            let oldp = shot_points(&s.text, s.x0, s.y0, SERVER_STEP, s.flip, Mode::Normal, true);
            println!(
                "    points new={} old={}  end new={:?} old={:?}",
                newp.len(),
                oldp.len(),
                newp.last(),
                oldp.last()
            );
            let mut maxdy = 0.0f64;
            for i in 0..newp.len().min(oldp.len()) {
                maxdy = maxdy.max((newp[i].1 - oldp[i].1).abs());
            }
            println!("    max|Δy| new vs old = {maxdy:e}");
            let rng = |p: &Vec<(f64, f64)>| {
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                for (_, y) in p {
                    lo = lo.min(*y);
                    hi = hi.max(*y);
                }
                (lo, hi)
            };
            let (nlo, nhi) = rng(&newp);
            let (olo, ohi) = rng(&oldp);
            println!("    y range new=[{nlo:.4},{nhi:.4}] old=[{olo:.4},{ohi:.4}]");
            for i in [0usize, 1, 2, 100, 1000, 2000, 3000, oldp.len().saturating_sub(1)] {
                let n = newp.get(i).map(|p| format!("({:.5},{:.5})", p.0, p.1)).unwrap_or_default();
                let o = oldp.get(i).map(|p| format!("({:.5},{:.5})", p.0, p.1)).unwrap_or_default();
                println!("      [{i}] new={n} old={o}");
            }

            // 到每个士兵的最近距离（逐采样点，与内核 `d <= radius` 同一判据）
            for t in &targets {
                if t.id == s.owner {
                    continue;
                }
                let dn = newp
                    .iter()
                    .map(|(x, y)| ((x - t.x).powi(2) + (y - t.y).powi(2)).sqrt())
                    .fold(f64::INFINITY, f64::min);
                let dold = oldp
                    .iter()
                    .map(|(x, y)| ((x - t.x).powi(2) + (y - t.y).powi(2)).sqrt())
                    .fold(f64::INFINITY, f64::min);
                if dn <= 0.75 || dold <= 0.75 {
                    let argmin = |p: &Vec<(f64, f64)>| {
                        let mut best = (f64::INFINITY, 0usize, (0.0, 0.0));
                        for (i, (x, y)) in p.iter().enumerate() {
                            let d = ((x - t.x).powi(2) + (y - t.y).powi(2)).sqrt();
                            if d < best.0 {
                                best = (d, i, (*x, *y));
                            }
                        }
                        best
                    };
                    let bn = argmin(&newp);
                    let bo = argmin(&oldp);
                    println!(
                        "      soldier {:>7} at ({:>9.4},{:>9.4})  d_new={dn:.9}  d_old={dold:.9}  \
                         new_kills={} old_kills={}",
                        t.id,
                        t.x,
                        t.y,
                        dn <= t.radius,
                        dold <= t.radius
                    );
                    println!(
                        "        argmin new: i={} y={:.6} | old: i={} pt=({:.6},{:.6})",
                        bn.1, bn.2 .1, bo.1, bo.2 .0, bo.2 .1
                    );
                }
            }
        }
        // 回放这一批里的死亡/爆炸事实
        println!("  --- observed ---");
        for m in &replay.msgs[b.start..=b.end] {
            match m.kind.as_str() {
                "EffectInfo" => println!(
                    "    EffectInfo/{} id={:?} pos=({:?},{:?})",
                    m.get_str("effect_type").unwrap_or_default(),
                    m.get_i64("effect_id"),
                    m.get_f64("pos_x"),
                    m.get_f64("pos_y")
                ),
                "LifeQueuedDeath" => {
                    println!("    LifeQueuedDeath entity={:?}", m.get_i64("entity_id"))
                }
                _ => {}
            }
        }
        // 延迟段（`end+1 .. 下一批 start`）里的 LifeInfo(alive=false)
        let next = batches
            .get(idx + 1)
            .map(|n| n.start)
            .unwrap_or(replay.msgs.len());
        for m in &replay.msgs[b.end + 1..next.min(replay.msgs.len())] {
            if m.kind == "LifeInfo" {
                let dead = matches!(
                    m.get("alive").map(|x| x.kind.clone()),
                    Some(gw2_rebuild::_scaffold::replaydiff::Kind::Bool(false))
                );
                if dead {
                    println!("    tail LifeInfo alive=false entity={:?}", m.get_i64("entity_id"));
                }
            }
        }
        if batch.is_some() {
            return;
        }
    }
}
