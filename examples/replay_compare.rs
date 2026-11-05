//! 拿 replay 还原出来的真实局面，比较"真游戏发射的函数"与"我们自己 AI 找到的最优"。
//!
//! 用法: `cargo run --release --example replay_compare -- <state.txt> <等级>`
use gw2_rebuild::aigen::{self, Individual, ScoringContext, VarMode};
use gw2_rebuild::function::parse;
use gw2_rebuild::plane::{empty_mask, rasterize};
use gw2_rebuild::rng::ChaCha12;
use gw2_rebuild::settle::Target;
use std::io::BufRead;

/// 取轨迹上 x 最接近 target 的点的 y。
fn p_y_at(target: f64, pts: &[(f64, f64)]) -> f64 {
    pts.iter()
        .min_by(|a, b| (a.0 - target).abs().partial_cmp(&(b.0 - target).abs()).unwrap())
        .map(|p| p.1)
        .unwrap_or(f64::NAN)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("需要 state.txt 路径");
    let level: u8 = args.next().unwrap_or_else(|| "3".into()).parse().unwrap();

    let f = std::fs::File::open(&path).expect("打不开 state 文件");
    let mut mask = empty_mask();
    let mut targets: Vec<Target> = Vec::new();
    let mut holes: Vec<(f64, f64, f64)> = Vec::new();
    let mut obstacles: Vec<(f64, f64, f64)> = Vec::new();
    let mut func = String::new();
    let mut shooter = (0.0, 0.0);
    let mut flip = true;
    let mut mode = VarMode::Normal;
    let mut real_end_x: Option<f64> = None;
    let mut real_end_y: Option<f64> = None;

    for line in std::io::BufReader::new(f).lines() {
        let line = line.unwrap();
        let mut it = line.split_whitespace();
        match it.next() {
            Some("MODE") => {
                if it.next() == Some("DiffEqFunction") {
                    mode = VarMode::DiffEq;
                }
            }
            Some("SHOOTER") => {
                let _id = it.next();
                shooter.0 = it.next().unwrap().parse().unwrap();
                shooter.1 = it.next().unwrap().parse().unwrap();
            }
            Some("FLIP") => flip = it.next() == Some("1"),
            Some("FUNC") => func = it.collect::<Vec<_>>().join(" "),
            Some("ENDX") => real_end_x = it.next().and_then(|v| v.parse().ok()),
            Some("ENDY") => real_end_y = it.next().and_then(|v| v.parse().ok()),
            Some("O") => obstacles.push((
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
            )),
            Some("H") => holes.push((
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
            )),
            Some("S") => {
                let id: i64 = it.next().unwrap().parse().unwrap();
                let x: f64 = it.next().unwrap().parse().unwrap();
                let y: f64 = it.next().unwrap().parse().unwrap();
                let r: f64 = it.next().unwrap().parse().unwrap();
                targets.push(Target { id, x, y, radius: r });
            }
            _ => {}
        }
    }

    for &(x, y, r) in &obstacles {
        rasterize(&mut mask, x, y, r, 0x00);
    }
    for &(x, y, r) in &holes {
        rasterize(&mut mask, x, y, r, 0xFF);
    }

    // 掩膜自检：障碍中心应当全被标记为占用
    let mut centers_ok = 0;
    for &(x, y, r) in &obstacles {
        if gw2_rebuild::plane::is_occupied(&mask, x, y) {
            centers_ok += 1;
        } else {
            println!("  ⚠ 障碍中心 ({x:.2}, {y:.2}) r={r:.2} 未被标记为占用");
        }
    }
    println!("掩膜自检: {}/{} 个障碍中心被正确标记", centers_ok, obstacles.len());

    let p = aigen::params(level);
    let real_tokens = parse(&func);
    let ctx = ScoringContext {
        x0: shooter.0, y0: shooter.1, flip, step: 0.5,
        mask: &mask, targets: &targets, limit: p.limit,
    };
    let real_score = aigen::fitness(&real_tokens, &ctx);

    println!("局面: {} 个障碍、{} 个目标、射手在 ({:.2}, {:.2}) flip={}",
             obstacles.len(), targets.len(), shooter.0, shooter.1, flip);
    println!("真游戏函数 ({} token): {}", real_tokens.len(), func);
    {
        // 顺便报出"我们的物理预测这一发打死几个人"
        use gw2_rebuild::path::{calculate, Mode};
        use gw2_rebuild::shot::{correction, shifted_tokens};
        if !real_tokens.is_empty() {
            let c = correction(&real_tokens, shooter.0, shooter.1, flip);
            let shifted = shifted_tokens(&real_tokens, c);
            let pts = calculate(&shifted, shooter.0, shooter.1, 0.5, flip, Mode::Normal);
            let mut killed: Vec<i64> = Vec::new();
            for &(px, py) in &pts {
                for t in &targets {
                    if killed.contains(&t.id) { continue; }
                    let d = ((px - t.x).powi(2) + (py - t.y).powi(2)).sqrt();
                    if d <= t.radius { killed.push(t.id); }
                }
                if gw2_rebuild::plane::is_occupied(&mask, px, py) { break; }
            }
            println!("PREDICTED_KILLS {}", killed.len());
        }
    }
    println!("  我们的 fitness 打分 = {:.0}", real_score);

    // 几何校验：用真实函数算弹道，看停止点是否与 replay 记录的 end_x 一致
    if let Some(end_x_real) = real_end_x {
        use gw2_rebuild::path::{calculate, Mode};
        use gw2_rebuild::shot::{correction, shifted_tokens};
        let c = correction(&real_tokens, shooter.0, shooter.1, flip);
        let shifted = shifted_tokens(&real_tokens, c);
        let pts = calculate(&shifted, shooter.0, shooter.1, 0.5, flip, Mode::Normal);
        // ⚠️ 必须沿路径查掩膜：撞到第一个被占据的点就停（和 settle::resolve 一致）
        let mut stop_pt = None;
        for &(px, py) in &pts {
            if gw2_rebuild::plane::is_occupied(&mask, px, py) {
                stop_pt = Some((px, py));
                break;
            }
        }
        let end_x_ours = stop_pt.map(|p| p.0).unwrap_or_else(|| pts.last().map(|p| p.0).unwrap_or(f64::NAN));
        println!("  几何校验: replay 记录的终点 x = {:.3}，我们算的 = {:.3}（差 {:.3}）",
                 end_x_real, end_x_ours, (end_x_real - end_x_ours).abs());
        // 我们算的轨迹在真实终点 x 处的 y
        if let (Some(ex), Some(ey)) = (real_end_x, real_end_y) {
        let occ = gw2_rebuild::plane::is_occupied(&mask, ex, ey);
        println!("ENDPOINT_OCCUPIED {}", if occ { 1 } else { 0 });
        println!("  真实终点 = ({ex:.3}, {ey:.3})，该点掩膜占用 = {occ}");
    }
    // 找离真实终点最近的若干障碍，看我们的曲线是否"擦过"
        let mut near: Vec<(f64, f64, f64, f64)> = obstacles.iter()
            .map(|&(ox, oy, or)| {
                let d = ((ox - end_x_real).powi(2) + (oy - p_y_at(end_x_real, &pts)).powi(2)).sqrt();
                (d, ox, oy, or)
            })
            .collect();
        near.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for (d, ox, oy, or) in near.iter().take(3) {
            println!("    障碍({ox:.2},{oy:.2}) r={or:.2}  到我们曲线在终点x处的距离={d:.3}  覆盖={}",
                     *d <= *or);
        }
        if let Some(p) = pts.iter().min_by(|a, b| {
            (a.0 - end_x_real).abs().partial_cmp(&(b.0 - end_x_real).abs()).unwrap()
        }) {
            println!("  轨迹在真实终点 x 附近的点 = ({:.2}, {:.2})", p.0, p.1);
        }
    }

    // 用我们的模型从随机种群开始演化，看能找到多好的解
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&0x5EED_1234_5678_9ABCu64.to_le_bytes());
    let mut rng = ChaCha12::from_seed(&key);
    let mut next = move || rng.next_u64();
    let pop: Vec<Individual> = aigen::initial_population(&p, mode, &mut next);
    let out = aigen::evolve(pop, &p, mode, &mut next, |t| aigen::fitness(t, &ctx));
    let best = &out[0];
    println!("我们的模型 ({} 代 × {} 种群) 最优 = {:.0}（{} token）",
             p.generations, p.population, best.score, best.tokens.len());
    let diff = best.score - real_score;
    if diff.abs() < 1.0 {
        println!("⇒ 打平");
    } else if diff > 0.0 {
        println!("⇒ 我们比真游戏好 {:.0}", diff);
    } else {
        println!("⇒ 真游戏比我们好 {:.0}", -diff);
    }
}
