//! 同时回合制（SimultaneousTurns）结算顺序分析用的**几何计算器**。
//!
//! 它自己不下结论：只把一个"一个 tick 内多发"的局面里的每一发单独算出来，
//! 并把「每发弹道第一次进入每个士兵半径的位置（采样下标 + 沿弧长）」吐出来，
//! 交由 Python 侧按不同假设（逐颗串行 / 全部并行 / 按飞行时间）做对照。
//!
//! 物理全部复用已验证的 `graphwar_path_calculator::calculate` / `plane::rasterize` / `graphwar_shot_calculator::correction`。
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
use gw2_rebuild::math_function::parse;
use gw2_rebuild::graphwar_path_calculator::{calculate, Mode};
use gw2_rebuild::plane::{empty_mask, is_occupied, rasterize};
use gw2_rebuild::graphwar_shot_calculator::{correction, shifted_tokens};

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
    if path == "--replay-sim" {
        // 新入口：同时回合制的**消息级对拍**（`docs/141` §5）。
        // 语料目录可用第二个位置参数覆盖。
        let dir = std::env::args()
            .nth(2)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(gw2_rebuild::replaydiff::default_corpus_dir);
        let quiet = std::env::args().any(|a| a == "--quiet");
        return replay_sim(&dir, quiet);
    }
    geometry(&path);
}

// ---------------------------------------------------------------------------
// 消息级对拍：**同一个口径、两种预测驱动**（`docs/141` §5）
// ---------------------------------------------------------------------------

/// 一次语料对拍的结论（按回合模式分桶）。
#[derive(Default)]
struct Bucket {
    files: usize,
    files_scored: usize,
    files_clean: usize,
    batches: usize,
    content_ok: usize,
    order_ok: usize,
    kind_seq_ok: usize,
    shots: usize,
    /// 多发批（`>=2` 发）单独统计
    multi_batches: usize,
    multi_kind_seq_ok: usize,
    multi_content_ok: usize,
    /// **结构**命中：把"死亡/灰烬特效块"整个剔掉后，块序列完全相同。
    /// 这一项把"分组与排序规则对不对"和"死亡集合对不对"彻底分开。
    multi_skeleton_ok: usize,
}

impl Bucket {
    fn add(&mut self, o: &Bucket) {
        self.files += o.files;
        self.files_scored += o.files_scored;
        self.files_clean += o.files_clean;
        self.batches += o.batches;
        self.content_ok += o.content_ok;
        self.order_ok += o.order_ok;
        self.kind_seq_ok += o.kind_seq_ok;
        self.shots += o.shots;
        self.multi_batches += o.multi_batches;
        self.multi_kind_seq_ok += o.multi_kind_seq_ok;
        self.multi_content_ok += o.multi_content_ok;
        self.multi_skeleton_ok += o.multi_skeleton_ok;
    }
    fn line(&self, name: &str) -> String {
        let p = |a: usize, b: usize| {
            if b == 0 { "   -  ".into() } else { format!("{:5.1}%", 100.0 * a as f64 / b as f64) }
        };
        format!(
            "  {:<20} 文件 {:>4}（有开火 {:>4}）完全一致 {:>3} {} | 结算批 {:>5} 内容 {:>4} {} 种类序列 {:>4} {} | 多发批 {:>4} 种类 {:>4} {} 内容 {:>4} {}",
            name,
            self.files,
            self.files_scored,
            self.files_clean,
            p(self.files_clean, self.files_scored),
            self.batches,
            self.content_ok,
            p(self.content_ok, self.batches),
            self.kind_seq_ok,
            p(self.kind_seq_ok, self.batches),
            self.multi_batches,
            self.multi_kind_seq_ok,
            p(self.multi_kind_seq_ok, self.multi_batches),
            self.multi_content_ok,
            p(self.multi_content_ok, self.multi_batches),
        )
    }
}

/// 一轮语料：`batch_driver = true` 走 `Match::fire_batch`（一次事务），
/// `false` 走旧的"每发一个完整窗口"（= `replaydiff::Predictor` 的驱动方式）。
///
/// 两者**共用** `compare_batch`：投影 / 折叠 / 分块 / 多重集 / 字段级 diff 全是同一份
/// 代码（`docs/136` §1.2–§1.4 的口径），所以差异只可能来自驱动方式。
fn sweep(
    dir: &std::path::Path,
    policy: &gw2_rebuild::replaydiff::Policy,
    batch_driver: bool,
) -> (std::collections::BTreeMap<String, Bucket>, std::collections::BTreeMap<String, usize>) {
    use gw2_rebuild::game::{Match, Submission};
    use gw2_rebuild::replaydiff::{compare_batch, split_batches, Inputs, Msg, Replay};
    use gw2_rebuild::graphwar_shot_calculator::settle::Target;

    let mut why: std::collections::BTreeMap<String, usize> = Default::default();
    let mut out: std::collections::BTreeMap<String, Bucket> = Default::default();
    let mut names: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .expect("读不了语料目录")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    names.sort();

    for path in names {
        let Ok(replay) = Replay::load(&path) else { continue };
        let batches = split_batches(&replay);
        let mode = replay
            .msgs
            .iter()
            .find(|m| m.kind == "TurnModeInfo")
            .and_then(|m| m.get_str("turn_mode"))
            .unwrap_or_else(|| "<none>".into());
        let b = out.entry(mode).or_default();
        b.files += 1;
        if batches.is_empty() {
            continue;
        }
        let inputs = Inputs::scan(&replay, batches[0].start);
        let soldiers: Vec<Target> = inputs
            .pos
            .iter()
            .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
            .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
            .collect();
        if soldiers.is_empty() {
            continue;
        }
        let base_tick = replay.msgs[batches[0].start].tick;
        let mut model = Match::new(
            inputs.room.clone(),
            inputs.obstacles.clone(),
            soldiers,
            (base_tick, 0),
        );

        let mut file = Bucket::default();
        for bt in &batches {
            if bt.shots.is_empty() {
                continue;
            }
            file.batches += 1;
            file.shots += bt.shots.len();
            let tick = replay.msgs[bt.start].tick;
            // 与 `Predictor::predict` 一致：把 tick 基准挪到本批。
            model.tick_base = (tick, 0);

            let wire: Vec<String> = if batch_driver {
                let subs: Vec<Submission<'_>> = bt
                    .shots
                    .iter()
                    .map(|s| Submission {
                        owner_id: s.owner,
                        function_text: &s.text,
                        x0: s.x0,
                        y0: s.y0,
                        flip: s.flip,
                    })
                    .collect();
                model.fire_batch(&subs, 0)
            } else {
                let mut v = Vec::new();
                for s in &bt.shots {
                    v.extend(model.fire(s.owner, &s.text, s.x0, s.y0, s.flip, 0));
                }
                v
            };
            let ours: Vec<Msg> = wire.iter().filter_map(|w| Msg::parse_wire(w).ok()).collect();
            let observed: Vec<Msg> = replay.msgs[bt.start..=bt.end].to_vec();
            let v = compare_batch(&ours, &observed, policy, 4);
            if v.content_ok {
                file.content_ok += 1;
            }
            if v.order_ok {
                file.order_ok += 1;
            }
            if v.kind_seq_ok {
                file.kind_seq_ok += 1;
            }
            if bt.shots.len() > 1 {
                file.multi_batches += 1;
                if skeleton(&ours, policy, true) == skeleton(&observed, policy, false) {
                    file.multi_skeleton_ok += 1;
                }
                if v.kind_seq_ok {
                    file.multi_kind_seq_ok += 1;
                }
                if v.content_ok {
                    file.multi_content_ok += 1;
                }
                if !v.kind_seq_ok {
                    let k = match &v.first {
                        Some(f) => format!("{}|{}|{}", f.kind, f.path, gw2_rebuild::replaydiff::class_name(&f.class)),
                        None => "<无 first>".into(),
                    };
                    *why.entry(k).or_insert(0) += 1;
                    *why.entry(format!("  我方={} 回放={}", f_ours(&v), f_obs(&v))).or_insert(0) += 1;
                }
            }
        }
        // "完全一致"用与 `docs/136` 同一个判据：每一批的内容都命中（+ 不产非法 JSON）。
        if file.batches > 0 {
            file.files_scored = 1;
            file.files_clean = (file.content_ok == file.batches) as usize;
        }
        b.add(&file);
    }
    (out, why)
}

/// 种类（`EffectInfo` 带 `effect_type`）。与 `replaydiff::compare_batch` 的 `kind_of` 同定义。
fn kind_of(m: &gw2_rebuild::replaydiff::Msg) -> String {
    if m.kind == "EffectInfo" {
        format!("EffectInfo/{}", m.get_str("effect_type").unwrap_or_default())
    } else {
        m.kind.clone()
    }
}

/// **骨架**：块序列，**剔掉死亡/灰烬特效块**。
///
/// 为什么要有这一项：死亡块的在/不在取决于"我们算出的死亡集合"，那是**另一个**缺口
/// （`docs/136` §6 第 8 条），与"分组与顺序规则"是两件事。把死亡块剔掉以后，
/// 剩下的是纯粹的批结构（前缀按类型分组、后缀每发一对），它应当 130/130 全中。
fn skeleton(
    ms: &[gw2_rebuild::replaydiff::Msg],
    policy: &gw2_rebuild::replaydiff::Policy,
    ours: bool,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut prev = String::new();
    for m in ms {
        // 与 `compare_batch` 同一套投影：我方产出但回放不记的丢掉；
        // 回放里有但我方根本不产的也丢掉（那是缺口，不是分歧）。
        let skip = if ours { policy.is_unobserved(m) } else { policy.is_unproduced(m) };
        if skip {
            continue;
        }
        // 同一 tick 里的相邻完全重复（回放记录器会重复记，`docs/136` §1.2）
        let c = m.canonical();
        if c == prev {
            continue;
        }
        prev = c;
        let k = kind_of(m);
        if out.last() != Some(&k) {
            out.push(k);
        }
    }
    // 死亡/灰烬块整体剔掉：它们的"在不在"取决于死亡集合（另一处缺口），不是批结构。
    out.retain(|k| k != "EffectInfo/DeathEffect" && k != "EffectInfo/AshEffect");
    out
}

/// 把 `"A | B | B | C"` 压成连续同类块 `"Ax1|Bx2|Cx1"`，块内的倍数保留下来。
fn compress(s: &str) -> String {
    let mut out: Vec<(String, usize)> = Vec::new();
    for k in s.split('|') {
        let k = k.trim().to_string();
        match out.last_mut() {
            Some(last) if last.0 == k => last.1 += 1,
            _ => out.push((k, 1)),
        }
    }
    out.iter()
        .map(|(k, n)| if *n > 1 { format!("{k}x{n}") } else { k.clone() })
        .collect::<Vec<_>>()
        .join("|")
}
fn f_ours(v: &gw2_rebuild::replaydiff::BatchVerdict) -> String {
    v.first.as_ref().map(|f| compress(&f.ours)).unwrap_or_default()
}
fn f_obs(v: &gw2_rebuild::replaydiff::BatchVerdict) -> String {
    v.first.as_ref().map(|f| compress(&f.observed)).unwrap_or_default()
}

fn replay_sim(dir: &std::path::Path, quiet: bool) {
    use gw2_rebuild::replaydiff::Policy;
    let policy = Policy::default();
    let (legacy, _why_a) = sweep(dir, &policy, false);
    let (batch, why_b) = sweep(dir, &policy, true);
    let modes: Vec<String> = {
        let mut v: Vec<String> = legacy.keys().cloned().collect();
        for k in batch.keys() {
            if !v.contains(k) {
                v.push(k.clone());
            }
        }
        v.sort();
        v
    };
    let (mut lt, mut bt) = (Bucket::default(), Bucket::default());
    println!("== 语料：{} ==", dir.display());
    println!("口径与 `docs/136` 完全相同（同一份 `Policy` / `compare_batch`）；只有**预测驱动**不同。");
    println!("\n--- 驱动 A：每发一个完整窗口（= `Match::fire` ×N，现行实现对拍的方式）---");
    for m in &modes {
        if let Some(b) = legacy.get(m) {
            println!("{}", b.line(m));
            lt.add(b);
        }
    }
    println!("{}", lt.line("合计"));
    println!("\n--- 驱动 B：一次事务（= `Match::fire_batch`，`docs/141`）---");
    for m in &modes {
        if let Some(b) = batch.get(m) {
            println!("{}", b.line(m));
            bt.add(b);
        }
    }
    println!("{}", bt.line("合计"));
    println!("\n--- 只看多发批（>=2 发 = 同时回合制的结算批）---");
    println!(
        "  驱动 A: 种类序列 {}/{}  内容 {}/{}",
        lt.multi_kind_seq_ok, lt.multi_batches, lt.multi_content_ok, lt.multi_batches
    );
    println!(
        "  驱动 B: 种类序列 {}/{}  内容 {}/{}",
        bt.multi_kind_seq_ok, bt.multi_batches, bt.multi_content_ok, bt.multi_batches
    );
    println!(
        "  **骨架**（剔掉死亡/灰烬块的批结构，只看分组与排序）: A {} / B {} （共 {}）",
        lt.multi_skeleton_ok, bt.multi_skeleton_ok, bt.multi_batches
    );
    if !quiet {
        let mut w: Vec<(&String, &usize)> = why_b.iter().collect();
        w.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        println!("\n--- 驱动 B 仍失败的多发批：第一处分歧（前 12）---");
        for (k, n) in w.iter().take(12) {
            println!("  {n:>4}  {k}");
        }
    }
    println!("\n（口径与复跑命令见 `docs/141` §5）");
}

fn geometry(path: &str) {
    let text = std::fs::read_to_string(path).expect("读不了 state 文件");

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

        // 先定停点（障碍），再只在 [0, stopi] 上判命中
        let mut stopi = pts.len() - 1;
        let mut blocked = false;
        for (i, &(px, py)) in pts.iter().enumerate() {
            if is_occupied(&mask, px, py) {
                stopi = i;
                blocked = true;
                break;
            }
        }
        // 逐点命中（与 graphwar_shot_calculator::settle::resolve 一致：只在采样点上判）
        let mut hits: Vec<(i64, usize, f64, f64)> = Vec::new(); // (tid, first_i, arclen, mindist)
        for (i, &(px, py)) in pts.iter().enumerate() {
            if i > stopi {
                break;
            }
            for t in &targets {
                if t.id == s.owner || hits.iter().any(|h| h.0 == t.id) {
                    continue;
                }
                let d = ((px - t.x).powi(2) + (py - t.y).powi(2)).sqrt();
                if d <= t.r {
                    hits.push((t.id, i, cum[i], d));
                }
            }
        }
        // 连续命中（折线段到圆心的最短距离）——子弹是沿折线连续飞行的
        let mut hits_c: Vec<(i64, usize, f64, f64)> = Vec::new();
        for t in &targets {
            if t.id == s.owner {
                continue;
            }
            let mut best = (f64::INFINITY, 0usize, 0.0f64);
            for i in 1..=stopi {
                let (ax, ay) = pts[i - 1];
                let (bx, by) = pts[i];
                let (vx, vy) = (bx - ax, by - ay);
                let len2 = vx * vx + vy * vy;
                let mut u = if len2 > 0.0 {
                    ((t.x - ax) * vx + (t.y - ay) * vy) / len2
                } else {
                    0.0
                };
                if u < 0.0 {
                    u = 0.0;
                } else if u > 1.0 {
                    u = 1.0;
                }
                let (cx, cy) = (ax + u * vx, ay + u * vy);
                let d = ((cx - t.x).powi(2) + (cy - t.y).powi(2)).sqrt();
                if d <= t.r && best.0 > t.r {
                    // 第一次进入半径的那一段
                    best = (d, i, cum[i - 1] + u * (cum[i] - cum[i - 1]));
                    hits_c.push((t.id, i, best.2, d));
                    break;
                }
            }
        }

        let (sx, sy) = pts[stopi];
        // 路径是"正常终止"（撞障碍 / 出边界 / 点数用尽）还是"采样提前中止"？
        // 提前中止 = 折长二分在 100 次内没收敛 ⇒ 我们的采样器放弃了，真游戏没有。
        let n_budget = gw2_rebuild::graphwar_path_calculator::point_budget(step) as usize;
        let normal_end = blocked as usize
            + (sx.abs() > 24.99) as usize
            + (sy.abs() > 14.99) as usize
            + (pts.len() >= n_budget) as usize;
        println!(
            "SHOT {} {} {} {} {} {:.6} {:.6} {:.6} {}",
            s.idx,
            s.owner,
            pts.len(),
            stopi,
            if blocked { 1 } else { 0 },
            cum[stopi],
            sx,
            sy,
            if normal_end > 0 { 1 } else { 0 }
        );
        for (tid, i, arc, d) in &hits {
            println!("HITP {} {} {} {:.6} {:.6}", s.idx, tid, i, arc, d);
        }
        for (tid, i, arc, d) in &hits_c {
            println!("HITC {} {} {} {:.6} {:.6}", s.idx, tid, i, arc, d);
        }
    }
}
