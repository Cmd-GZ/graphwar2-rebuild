//! 评分核心对拍用的**局面执行器**：把一个局面喂给我方的两份评分实现，并把
//! **弹道点列**原样吐出来，好让 `tools/score_truth.py` 用它构造真二进制要吃的输入。
//!
//! 为什么需要它：真值 `0x51a750`（评分核心）的入参不是"局面"，而是**开火结果项向量**
//! （`0x51a9a1` 调 `0x3c2bc0` 拿到）。要拿真值只能由调用方先跑一遍弹道、把结果项按
//! 评分核心的读法铺好（`tools/score_truth.py` 里做）；而"弹道长什么样"这件事必须和
//! 我方实现**同源**，否则比出来的差异是弹道差异、不是评分差异。本程序就是那个同源出口。
//!
//! 用法（输入是 `tools/score_truth.py` 生成的一份 TSV，见该脚本的 `write_cases`）：
//!
//! ```text
//! cargo run --release --offline -q --example score_probe -- .agent-work/ev/cases.tsv
//! ```
//!
//! 输入列（制表符分隔，第一行是表头）：
//!
//! | 列 | 含义 |
//! |---|---|
//! | `case` | 用例名（原样回显） |
//! | `func` | 函数文本，走 `math_function::parse`（与真链路同一个解析器） |
//! | `x0` / `y0` | 射手坐标 |
//! | `flip` | 0/1，队 2 的左右翻转 |
//! | `step` | 采样步长（粗评 0.5 / 精评 0.01；也是真值里唯一的 SSE 入参） |
//! | `limit` | 复杂度硬上限（`ctx+0x58`） |
//! | `shooter_id` | 射手 id（真值第 4 个参数 `ecx`，拿它查自己的队伍字节） |
//! | `shooter_team` | 射手队伍字节（0/1 = 两方，2 = 无主） |
//! | `entities` | `id,x,y,radius,team,kind` 用 `;` 串；`kind` = `s`（士兵，进命中数）/ `t`（靶，只进距离项） |
//!
//! 输出列：`case`、`v`、`points`（`x,y;x,y;…`）、`fit_*`（`fitness_eval` 的 4 个分量）、
//! `ai_*`（`ai_explain::evaluate` 的 4 个分量）。**本程序不判对错**，只报读数。

use gw2_rebuild::_scaffold::ai_explain::{self, Entity, Situation};
use gw2_rebuild::graphwar_function_evolutionator::{self, ScoringContext};
use gw2_rebuild::graphwar_shot_calculator::settle::Target;
use gw2_rebuild::graphwar_shot_calculator::{correction, shifted_tokens};
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::math_function::parse;

struct EntitySpec {
    id: i64,
    x: f64,
    y: f64,
    radius: f64,
    team: u8,
    is_soldier: bool,
}

fn parse_entities(s: &str) -> Vec<EntitySpec> {
    let mut out = Vec::new();
    for one in s.split(';') {
        let one = one.trim();
        if one.is_empty() {
            continue;
        }
        let f: Vec<&str> = one.split(',').collect();
        if f.len() < 6 {
            panic!("entities 列应为 id,x,y,radius,team,kind：{one}");
        }
        out.push(EntitySpec {
            id: f[0].trim().parse().expect("entity id"),
            x: f[1].trim().parse().expect("entity x"),
            y: f[2].trim().parse().expect("entity y"),
            radius: f[3].trim().parse().expect("entity radius"),
            team: f[4].trim().parse().expect("entity team"),
            is_soldier: match f[5].trim() {
                "s" => true,
                "t" => false,
                other => panic!("entity kind 只能是 s/t：{other}"),
            },
        });
    }
    out
}

fn main() {
    let path = std::env::args().nth(1).expect("用法：score_probe <cases.tsv>");
    let text = std::fs::read_to_string(&path).expect("读不到用例文件");
    println!(
        "case\tv\tpoints\tfit_score_bits\tfit_dist_bits\tfit_hits\tfit_esi\tai_score_bits\tai_dist_bits\tai_kills\tai_friendly"
    );
    for (lineno, line) in text.lines().enumerate() {
        if lineno == 0 || line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        assert!(f.len() >= 10, "用例行应至少 10 列：{line}");
        let case = f[0].trim();
        let func = f[1].trim();
        let x0: f64 = f[2].trim().parse().expect("x0");
        let y0: f64 = f[3].trim().parse().expect("y0");
        let flip = f[4].trim() == "1";
        let step: f64 = f[5].trim().parse().expect("step");
        let limit: u32 = f[6].trim().parse().expect("limit");
        let shooter_id: i64 = f[7].trim().parse().expect("shooter_id");
        let shooter_team: u8 = f[8].trim().parse().expect("shooter_team");
        let ents = parse_entities(f[9]);

        let tokens = parse(func);

        // ---- 弹道（与 fitness_eval / ai_explain 内部完全同源的两步）----
        let c = correction(&tokens, x0, y0, flip);
        let shifted = shifted_tokens(&tokens, c);
        let points = calculate(&shifted, x0, y0, step, flip, Mode::Normal);
        let mut pts = String::new();
        for (i, (px, py)) in points.iter().enumerate() {
            if i > 0 {
                pts.push(';');
            }
            pts.push_str(&format!("{px:.17e},{py:.17e}"));
        }

        // ---- 我方 fitness_eval ----
        let targets: Vec<Target> = ents
            .iter()
            .map(|e| Target { id: e.id, x: e.x, y: e.y, radius: e.radius })
            .collect();
        let mask = gw2_rebuild::plane::empty_mask();
        // 队伍表 = 射手一条 + 每个实体一条（真值是两张表，这里合成一张扁平表；
        // ⚠️ 射手必须在这张表里 —— 查不到就是 `TEAM_UNKNOWN`，而 `2` vs `2` 判"自己人"）
        let mut teams: Vec<(i64, u8)> = vec![(shooter_id, shooter_team)];
        teams.extend(ents.iter().map(|e| (e.id, e.team)));
        let ctx = ScoringContext {
            x0,
            y0,
            flip,
            step,
            mask: &mask,
            targets: &targets,
            limit,
            shooter_id,
            teams: &teams,
        };
        let ev = graphwar_function_evolutionator::fitness_eval(&tokens, &ctx);

        // ---- ai_explain::evaluate ----
        let mut sit = Situation::empty(x0, y0, flip, shooter_team, limit);
        for e in &ents {
            sit.entities.push(if e.is_soldier {
                Entity::soldier(e.id, e.x, e.y, e.radius, e.team)
            } else {
                Entity::target(e.id, e.x, e.y, e.radius, e.team)
            });
        }
        let sc = ai_explain::evaluate(&tokens, &sit, step);

        println!(
            "{case}\t{}\t{}\t{:016x}\t{:016x}\t{}\t{}\t{:016x}\t{:016x}\t{}\t{}",
            tokens.len(),
            pts,
            ev.score.to_bits(),
            ev.dist.to_bits(),
            ev.hits,
            ev.esi,
            sc.score.to_bits(),
            sc.dist.to_bits(),
            sc.kills,
            sc.friendly,
        );
    }
}

