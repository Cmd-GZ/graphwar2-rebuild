//! 打印 bot 面对一个局面时的**完整选择过程**：每一代造了什么、留了什么、
//! 前 3 名的精评有没有把谁顶掉、最后交出去的是哪条函数、它凭什么赢。
//!
//! 用法（默认跑一个示范局面）：
//!
//! ```text
//! cargo run --offline --example ai_explain
//! cargo run --offline --example ai_explain -- --difficulty 50 --seed 7
//! cargo run --offline --example ai_explain -- --level 3 --enemy -10,2 --enemy 12,-3 \
//!        --friend 5,1.5 --obstacle 0,4,3 --gens 5 --top 3
//! ```
//!
//! 参数：
//!
//! | 参数 | 含义 |
//! |---|---|
//! | `--difficulty D` | 用**关卡难度值**（`bot_added_event` 载荷 +0x10），见 `graphwar_function_evolutionator::difficulty_params` |
//! | `--level N` | 用 `docs/AI.md` 的六档表（1..6） |
//! | `--x0/--y0` | 射手坐标（默认 0,0） |
//! | `--flip` | 左右翻转（队 2 的射击方向） |
//! | `--team T` | 射手队伍字节（0/1） |
//! | `--enemy x,y` / `--friend x,y` | 追加一个士兵（可重复） |
//! | `--target x,y` | 追加一个只影响距离项的靶（可重复） |
//! | `--obstacle x,y,size` | 烧一个障碍进掩膜（可重复） |
//! | `--diffeq` | 用微分方程模式（权重表把变量槽拆成 x/y） |
//! | `--seed N` | 随机种子（同种子 ⇒ 同结果） |
//! | `--gens N` | 只跑前 N 代（对照被 25 s 截断的情形） |
//! | `--budget S` | 墙钟预算（秒），模拟原版 25 s |
//! | `--top N` | 每代额外打印前 N 名 |
//! | `--summary` | 只打印参数 + 最终选择 |

use gw2_rebuild::ai_explain::{self, demo_situation, Entity, Settings, Situation};
use gw2_rebuild::graphwar_function_evolutionator::VarMode;

fn parse_pair(s: &str) -> (f64, f64) {
    let mut it = s.split(',');
    let x = it.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
    let y = it.next().and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
    (x, y)
}

fn parse_triple(s: &str) -> (f64, f64, f64) {
    let (x, y) = parse_pair(s);
    let size = s.split(',').nth(2).and_then(|v| v.trim().parse().ok()).unwrap_or(3.0);
    (x, y, size)
}

fn main() {
    let mut difficulty: Option<u32> = None;
    let mut level: Option<u8> = None;
    let mut sit: Option<Situation> = None;
    let mut mode = VarMode::Normal;
    let mut seed: u64 = 1;
    let mut gens: Option<u32> = None;
    let mut budget: Option<f64> = None;
    let mut top: usize = 0;
    let mut summary = false;
    let mut x0 = 0.0f64;
    let mut y0 = 0.0f64;
    let mut flip = false;
    let mut team = 0u8;
    let mut soldiers: Vec<Entity> = Vec::new();
    let mut targets: Vec<Entity> = Vec::new();
    let mut obstacles: Vec<(f64, f64, f64)> = Vec::new();
    let mut next_id = 101i64;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let val = |i: usize| args.get(i).cloned().unwrap_or_default();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--difficulty" => {
                difficulty = val(i + 1).parse().ok();
                i += 1;
            }
            "--level" => {
                level = val(i + 1).parse().ok();
                i += 1;
            }
            "--x0" => {
                x0 = val(i + 1).parse().unwrap_or(0.0);
                i += 1;
            }
            "--y0" => {
                y0 = val(i + 1).parse().unwrap_or(0.0);
                i += 1;
            }
            "--flip" => flip = true,
            "--team" => {
                team = val(i + 1).parse().unwrap_or(0);
                i += 1;
            }
            "--diffeq" => mode = VarMode::DiffEq,
            "--seed" => {
                seed = val(i + 1).parse().unwrap_or(1);
                i += 1;
            }
            "--gens" => {
                gens = val(i + 1).parse().ok();
                i += 1;
            }
            "--budget" => {
                budget = val(i + 1).parse().ok();
                i += 1;
            }
            "--top" => {
                top = val(i + 1).parse().unwrap_or(0);
                i += 1;
            }
            "--summary" => summary = true,
            "--enemy" => {
                let (x, y) = parse_pair(&val(i + 1));
                soldiers.push(Entity::soldier(next_id, x, y, 0.5, 1 - team));
                next_id += 1;
                i += 1;
            }
            "--friend" => {
                let (x, y) = parse_pair(&val(i + 1));
                soldiers.push(Entity::soldier(next_id, x, y, 0.5, team));
                next_id += 1;
                i += 1;
            }
            "--target" => {
                let (x, y, r) = parse_triple(&val(i + 1));
                targets.push(Entity::target(next_id, x, y, if r > 0.0 { r } else { 0.5 }, 2));
                next_id += 1;
                i += 1;
            }
            "--obstacle" => {
                obstacles.push(parse_triple(&val(i + 1)));
                i += 1;
            }
            "--help" | "-h" => {
                println!("用法见 examples/ai_explain.rs 顶部注释");
                return;
            }
            other => {
                eprintln!("未知参数 {other}（--help 看用法）");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    // 难度与参数
    let settings = if let Some(d) = difficulty {
        Settings::from_difficulty(d, mode)
    } else if let Some(l) = level {
        Settings::from_level(l, mode)
    } else {
        Settings::from_difficulty(50, mode) // 关 12 那个"对不上任何一列"的值
    };
    let settings = Settings {
        max_generations: gens,
        budget_secs: budget,
        ..settings
    };

    // 局面：给了 --enemy/--friend/--target 就用它，否则用示范局面
    if soldiers.is_empty() && targets.is_empty() {
        sit = Some(demo_situation());
    }
    let mut sit = sit.unwrap_or_else(|| Situation::empty(x0, y0, flip, team, settings.params.limit));
    sit.x0 = x0;
    sit.y0 = y0;
    sit.flip = flip;
    sit.shooter_team = team;
    sit.limit = settings.params.limit;
    sit.entities.extend(soldiers);
    sit.entities.extend(targets);
    for (ox, oy, size) in &obstacles {
        sit.add_obstacle(*ox, *oy, *size);
    }

    let report = ai_explain::explain(&sit, &settings, seed);
    let text = ai_explain::render(&sit, &report, !summary && top > 0);
    print!("{text}");

    if !summary && top == 0 {
        // 默认也把最后一代的前 3 名打出来：那才是"选择"真正发生的地方
        if let Some(last) = report.generations.last() {
            println!("\n=== 最后一代的前 3 名（选择就在这里发生）===");
            for (rank, c) in last.ranking.iter().enumerate().take(3) {
                println!(
                    "#{rank} {:>12.1} 分 净命中 {:>3} 长 {:>2} {:<10} {}",
                    c.score.score,
                    c.score.net_hits(),
                    c.tokens.len(),
                    c.origin.label(),
                    c.text
                );
            }
        }
    }
}
