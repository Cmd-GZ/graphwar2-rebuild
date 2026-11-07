//! 单批 **`end_x` 语义**探针（`docs/06-verification.md` §24 的 E1 类归因用）。
//!
//! 现象（`tools/bit_exact_attrib.py` 的 E1 类，conv 口径 95/512 批）：
//! 我方的 `FunctionPoints.end_x` 与回放差 **≤ 一个采样步**（差值全是 `step/2^k` 的整数倍），
//! 而回放那条 `EffectInfo(Explosion).pos_x` 与我方的停点**只差 1 ulp**。
//! ⇒ 停点在物理上是对的，差的是"**报出去的那个 `end_x` 取哪一个采样点**"。
//!
//! 本工具把我们的采样点列出来，直接看回放的 `end_x` 落在哪一点上：
//!
//! ```text
//! cargo run --release --example replay_endx_probe -- --dir <语料> --file <名字> [--batch N] [--tail 6]
//! ```
//!
//! 输出每一发：`end_x(回放) / pos_x(回放) / 我方 stop / 我方最后几个采样点`，
//! 并标注回放的 `end_x` 命中我方的第几个点（`idx=-1` 表示不在点列上）。
use gw2_rebuild::_scaffold::diffconv::{corrected_function, mode_of};
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Replay};
use gw2_rebuild::graphwar_path_calculator::{calculate, point_budget};

fn main() {
    let mut dir = gw2_rebuild::_scaffold::replaydiff::default_corpus_dir();
    let mut file = String::new();
    let mut batch = usize::MAX;
    let mut tail = 6usize;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => dir = args.next().expect("--dir 需要值").into(),
            "--file" => file = args.next().expect("--file 需要值"),
            "--batch" => batch = args.next().expect("--batch 需要值").parse().unwrap(),
            "--tail" => tail = args.next().expect("--tail 需要值").parse().unwrap(),
            other => {
                eprintln!("未知参数 {other}");
                std::process::exit(2);
            }
        }
    }
    if file.is_empty() {
        eprintln!("必须给 --file <回放文件名>");
        std::process::exit(2);
    }
    let path = dir.join(&file);
    let replay = match Replay::load(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("读不了 {}: {e}", path.display());
            std::process::exit(2);
        }
    };
    let batches = split_batches(&replay);
    let mode = {
        let mut m = gw2_rebuild::graphwar_path_calculator::Mode::Normal;
        for msg in &replay.msgs {
            if msg.kind == "FunctionModeInfo" {
                m = mode_of(&msg.get_str("function_mode").unwrap_or_default());
            }
        }
        m
    };
    println!(
        "{}：{} 条报文 / {} 批 / 标称模式 {:?} / step=0.01 / 预算 {}",
        file,
        replay.msgs.len(),
        batches.len(),
        mode,
        point_budget(gw2_rebuild::SERVER_STEP)
    );

    for (idx, b) in batches.iter().enumerate() {
        if batch != usize::MAX && idx != batch {
            continue;
        }
        if b.shots.is_empty() {
            continue;
        }
        let window = &replay.msgs[b.start..=b.end];
        println!("\n批 #{idx}（{} 发）", b.shots.len());
        // 回放观测：FunctionPoints.end_x 与爆炸 EffectInfo 的位置
        let mut end_x = Vec::new();
        let mut expl = Vec::new();
        let mut pending = None;
        for m in window {
            match m.kind.as_str() {
                "FunctionPoints" => end_x.push((
                    m.get_i64("owner_id").unwrap_or(0),
                    m.get_f64("end_x").unwrap_or(f64::NAN),
                )),
                "FunctionActive" => pending = m.get_i64("owner_id"),
                "EffectInfo" => {
                    if m.get_str("effect_type").as_deref() == Some("ExplosionEffect") {
                        if let Some(o) = pending.take() {
                            expl.push((
                                o,
                                m.get_f64("pos_x").unwrap_or(f64::NAN),
                                m.get_f64("pos_y").unwrap_or(f64::NAN),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        for shot in &b.shots {
            let corrected = corrected_function(&shot.text, shot.x0, shot.y0, shot.flip, mode);
            let tokens = gw2_rebuild::math_function::parse(&corrected);
            let pts = calculate(&tokens, shot.x0, shot.y0, gw2_rebuild::SERVER_STEP, shot.flip, mode);
            let stop = pts.last().copied().unwrap_or((f64::NAN, f64::NAN));
            let ex = end_x.iter().find(|(o, _)| *o == shot.owner).map(|(_, v)| *v);
            let ep = expl.iter().find(|(o, _, _)| *o == shot.owner);
            // 回放的 end_x 命中我方第几个采样点？（取 |Δx| 最小者）
            let mut best: (usize, f64) = (usize::MAX, f64::INFINITY);
            for (i, p) in pts.iter().enumerate() {
                let d = (p.0 - ex.unwrap_or(f64::NAN)).abs();
                if d < best.1 {
                    best = (i, d);
                }
            }
            let idx_of = if best.1 <= 1e-9 { best.0 as i64 } else { -1 };
            println!(
                "  owner={:<7} 点 {}  回放 end_x={:?}\n     回放 pos_x={:?} pos_y={:?}\n     我方 stop=({:?}, {:?})  end_x 命中我方点 idx={} (Δ={:.3e})",
                shot.owner,
                pts.len(),
                ex,
                ep.map(|x| x.1),
                ep.map(|x| x.2),
                stop.0,
                stop.1,
                idx_of,
                best.1
            );
            let from = pts.len().saturating_sub(tail);
            for (i, p) in pts.iter().enumerate().skip(from) {
                let mark = if i as i64 == idx_of { "  ← 回放 end_x" } else { "" };
                let mark2 = if i + 1 == pts.len() { "  ← 我方 stop" } else { "" };
                println!("       pts[{i}] = ({:?}, {:?}){mark}{mark2}", p.0, p.1);
            }
        }
    }
}
