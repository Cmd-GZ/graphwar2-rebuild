//! **批内交叉命中**探针（`docs/06-verification.md` §34，`dispatch/55` 的 A 步）。
//!
//! 要回答的问题只有一个：**回放报出来的那个停点，是谁的点列上的点？**
//!
//! `examples/replay_endx_probe.rs` 只把**owner 自己**那一发跑一遍，所以它只能回答
//! "命中我方第几个点 / 不在点列上"。判"串发"（重叠假设）必须把**同一批里所有发**
//! 都跑一遍，再拿每一发的回放停点去撞**所有**点列 —— 本工具就是那一步。
//!
//! ```text
//! cargo run --release --offline -q --example batch_cross_probe -- \
//!     --dir <语料> [--file <名字>] [--batch N] [--min-shots 6] [--all] \
//!     [--json out.jsonl] [--tail 4] [--selftest] [--mask-mode none|prev|same|arrival|prefix]
//! ```
//!
//! 判据（与 `dispatch/55` §1.A 一一对应，见 `docs/06` §34.2）：
//!
//! ```text
//! own-stop  : 回放停点 == **这一发自己**点列上的 k 点，且 k == 我方停点下标 ⇒ 完全一致
//! own-point : 回放停点 == 这一发自己点列上的 k 点，但 k ≠ 我方停点下标 ⇒ 弹道同、停点不同
//! cross     : 回放停点 == **同一批别的某一发**点列上的 k 点            ⇒ 串发
//! none      : 两个都不命中 ⇒ 停点被别的东西改了
//! ```
//!
//! 因为采样步长是**弦长**（`graphwar_path_calculator::calculate` 的 `step` 是斜边，
//! 不是 x 间隔），两条不同弹道的采样点几乎不可能**逐位**重合 ⇒ "完全相等"就是
//! 一个很强的判据，不需要容差去猜。容差只吸收 JSON 打印位数（`EPS = 1e-12`，
//! 比 1 ulp 大 3 个量级，比相邻采样点间距小 10 个量级）。
//!
//! ## `--mask-mode`：掩膜里放哪一批"洞"
//!
//! 挖洞（`QueueDestroyObstacle` / `DestroyObstacle`，半径 0.5，位置 = 爆炸点）是**唯一**
//! 会改变后续弹道的状态。本开关就是用来定"同时回合里，谁看得见谁的洞"：
//!
//! | 模式 | 掩膜 = 关卡障碍 + |
//! |---|---|
//! | `none` | 什么都不加（= 只用批开始那一刻的障碍表） |
//! | `prev` | 只加**本批之前**的 `DestroyObstacle`（跨批累积） |
//! | `same` | `prev` + 本批**所有**发的洞 |
//! | `arrival` | `prev` + 本批里**爆炸时刻更早**（`EffectInfo.start_tick` 更小）那些发的洞 |
//! | `prefix` | `prev` + 本批里**排在前面**（`FunctionPoints` 顺序）那些发的洞 |
//!
//! 证据等级：输入（函数原文 / 起点 / 朝向 / 障碍表 / 洞位置 / `start_tick`）全部来自
//! **回放报文本身**（`观测`）；点列由我方采样器算（与真二进制 `逐位` 对拍见
//! `docs/06` §33.3）；"命中谁"是逐位相等 + 反例排除 ⇒ `观测`。

use gw2_rebuild::_scaffold::diffconv::mode_of;
use gw2_rebuild::_scaffold::replaydiff::{split_batches, Inputs, Msg, Replay};
use gw2_rebuild::graphwar_shot_calculator::settle::{resolve_shot, Target};
use gw2_rebuild::plane::{empty_mask, rasterize};

/// 逐位相等容差：只吸收 JSON 打印位数，不吸收算法差。
const EPS: f64 = 1e-12;

#[derive(Clone, Copy, PartialEq)]
enum MaskMode {
    None,
    Prev,
    Same,
    Arrival,
    Prefix,
    /// **驱动真内核**（`Match::fire_functions` + `execute_results`，`dispatch/65`）：
    /// 掩膜由 `Match` 自己维护（含它自己在前面几批挖的洞），批内顺序由
    /// `GW2_SETTLE_ORDER` 选（`SettleOrder`）。这一档量的是**落地实现**，
    /// 不是"把回放的洞塞进掩膜"的理想实验 ⇒ 与 `arrival`/`prefix` 那几档互为对照。
    Kernel,
}

impl MaskMode {
    fn parse(s: &str) -> MaskMode {
        match s {
            "none" => MaskMode::None,
            "prev" => MaskMode::Prev,
            "same" => MaskMode::Same,
            "arrival" => MaskMode::Arrival,
            "prefix" => MaskMode::Prefix,
            "kernel" => MaskMode::Kernel,
            other => {
                eprintln!("--mask-mode 只认 none|prev|same|arrival|prefix|kernel，给的是 {other}");
                std::process::exit(2);
            }
        }
    }
    fn name(self) -> &'static str {
        match self {
            MaskMode::None => "none",
            MaskMode::Prev => "prev",
            MaskMode::Same => "same",
            MaskMode::Arrival => "arrival",
            MaskMode::Prefix => "prefix",
            MaskMode::Kernel => "kernel",
        }
    }
}

struct Args {
    dir: std::path::PathBuf,
    file: Option<String>,
    batch: Option<usize>,
    min_shots: usize,
    all: bool,
    json: Option<std::path::PathBuf>,
    tail: usize,
    selftest: bool,
    mask_mode: MaskMode,
    /// `--hole-owner 258,249`：**只**把这几发的爆炸点当洞（单变量实验用）
    hole_owner: Vec<i64>,
    /// `--holes all` 把每一发的爆炸点都当洞（默认 `blocked`：只算真发了 `DestroyObstacle` 的发）
    holes_all: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        dir: gw2_rebuild::_scaffold::replaydiff::default_corpus_dir(),
        file: None,
        batch: None,
        min_shots: 6,
        all: false,
        json: None,
        tail: 4,
        selftest: false,
        mask_mode: MaskMode::None,
        hole_owner: Vec::new(),
        holes_all: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        match k.as_str() {
            "--dir" => a.dir = it.next().expect("--dir 需要值").into(),
            "--file" => a.file = Some(it.next().expect("--file 需要值")),
            "--batch" => a.batch = Some(it.next().expect("--batch 需要值").parse().unwrap()),
            "--min-shots" => a.min_shots = it.next().expect("--min-shots 需要值").parse().unwrap(),
            "--all" => a.all = true,
            "--json" => a.json = Some(it.next().expect("--json 需要值").into()),
            "--tail" => a.tail = it.next().expect("--tail 需要值").parse().unwrap(),
            "--selftest" => a.selftest = true,
            "--holes" => {
                let v = it.next().expect("--holes 需要值");
                assert!(v == "all" || v == "blocked", "--holes 只认 all|blocked");
                a.holes_all = v == "all";
            }
            "--mask-mode" => a.mask_mode = MaskMode::parse(&it.next().expect("--mask-mode 需要值")),
            "--hole-owner" => {
                a.hole_owner = it
                    .next()
                    .expect("--hole-owner 需要值")
                    .split(',')
                    .map(|x| x.trim().parse().expect("--hole-owner 只认逗号分隔的 owner id"))
                    .collect()
            }
            other => {
                eprintln!("未知参数 {other}");
                std::process::exit(2);
            }
        }
    }
    a
}

/// 一发在**回放**里的读数。
#[derive(Default, Clone)]
struct Obs {
    owner: i64,
    end_x: Option<f64>,
    pos: Option<(f64, f64)>,
    /// `EffectInfo(ExplosionEffect).start_tick` —— 这一发的爆炸时刻
    start_tick: Option<i64>,
}

/// 一发在**我方**的读数。
struct Mine {
    owner: i64,
    n: usize,
    pts: Vec<(f64, f64)>,
    stop_index: usize,
    stop: (f64, f64),
    blocked: bool,
}

/// 一个点在某条点列里的最佳命中。
struct Hit {
    index: usize,
    dx: f64,
    dy: f64,
}

impl Hit {
    fn ok(&self) -> bool {
        self.dx.abs() <= EPS && self.dy.abs() <= EPS
    }
    fn err(&self) -> f64 {
        self.dx.abs().max(self.dy.abs())
    }
}

fn best_in(pts: &[(f64, f64)], p: (f64, f64)) -> Hit {
    let mut best = Hit { index: 0, dx: f64::INFINITY, dy: f64::INFINITY };
    let mut best_e = f64::INFINITY;
    for (i, q) in pts.iter().enumerate() {
        let e = (q.0 - p.0).abs().max((q.1 - p.1).abs());
        if e < best_e {
            best_e = e;
            best = Hit { index: i, dx: q.0 - p.0, dy: q.1 - p.1 };
        }
    }
    best
}

/// 只比 x（`FunctionPoints.end_x` 是一个标量）。
fn best_x(pts: &[(f64, f64)], x: f64) -> Hit {
    let mut best = Hit { index: 0, dx: f64::INFINITY, dy: 0.0 };
    let mut best_e = f64::INFINITY;
    for (i, q) in pts.iter().enumerate() {
        let e = (q.0 - x).abs();
        if e < best_e {
            best_e = e;
            best = Hit { index: i, dx: q.0 - x, dy: 0.0 };
        }
    }
    best
}

fn jstr(s: &str) -> String {
    gw2_rebuild::_scaffold::replaydiff::wire::quote(s)
}

fn jnum(v: f64) -> String {
    if v.is_finite() {
        format!("{v:?}")
    } else {
        "null".into()
    }
}

fn jopt(v: Option<f64>) -> String {
    match v {
        Some(x) => jnum(x),
        None => "null".into(),
    }
}

fn jopt_i(v: Option<i64>) -> String {
    match v {
        Some(x) => x.to_string(),
        None => "null".into(),
    }
}

/// 批内按 owner 取回放读数（`FunctionPoints` + `FunctionActive`→`ExplosionEffect` 配对）。
fn observed(window: &[Msg]) -> Vec<Obs> {
    let mut out: Vec<Obs> = Vec::new();
    let mut pending: Option<i64> = None;
    let get = |v: &Vec<Obs>, o: i64| -> Option<usize> { v.iter().position(|x| x.owner == o) };
    for m in window {
        match m.kind.as_str() {
            "FunctionPoints" => {
                let owner = m.get_i64("owner_id").unwrap_or(0);
                let idx = match get(&out, owner) {
                    Some(i) => i,
                    None => {
                        out.push(Obs { owner, ..Default::default() });
                        out.len() - 1
                    }
                };
                out[idx].end_x = m.get_f64("end_x");
            }
            "FunctionActive" => pending = m.get_i64("owner_id"),
            "EffectInfo" => {
                if m.get_str("effect_type").as_deref() == Some("ExplosionEffect") {
                    if let Some(o) = pending.take() {
                        let px = m.get_f64("pos_x").unwrap_or(f64::NAN);
                        let py = m.get_f64("pos_y").unwrap_or(f64::NAN);
                        if let Some(i) = get(&out, o) {
                            out[i].pos = Some((px, py));
                            out[i].start_tick = m.get_i64("start_tick");
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// 一整份回放里、`upto_tick` **之前**发出来的 `DestroyObstacle`（= 已经挖掉的洞）。
fn holes_before(replay: &Replay, upto_tick: i64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for m in &replay.msgs {
        if m.kind == "DestroyObstacle" && m.tick < upto_tick {
            out.push((
                m.get_f64("pos_x").unwrap_or(f64::NAN),
                m.get_f64("pos_y").unwrap_or(f64::NAN),
            ));
        }
    }
    out
}

fn mask_of(obstacles: &[(f64, f64, f64)], holes: &[(f64, f64)]) -> Vec<u8> {
    let mut m = empty_mask();
    for (x, y, r) in obstacles {
        rasterize(&mut m, *x, *y, *r, 0x00);
    }
    for (x, y) in holes {
        rasterize(&mut m, *x, *y, 0.5, 0xFF);
    }
    m
}

fn main() {
    let a = parse_args();
    if !a.dir.is_dir() {
        eprintln!("回放目录不存在: {}", a.dir.display());
        std::process::exit(2);
    }
    let mut names: Vec<std::path::PathBuf> = std::fs::read_dir(&a.dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| match &a.file {
            Some(f) => p.file_name().map(|s| s == f.as_str()).unwrap_or(false),
            None => true,
        })
        .collect();
    names.sort();
    if names.is_empty() {
        eprintln!("没找到回放（--dir {} / --file {:?}）", a.dir.display(), a.file);
        std::process::exit(2);
    }

    let mut json = a.json.as_ref().map(|p| {
        Box::new(std::io::BufWriter::new(std::fs::File::create(p).expect("--json 打不开")))
            as Box<dyn std::io::Write>
    });

    let mut n_batches = 0usize;
    let mut n_shots = 0usize;
    // pos 侧分类
    let (mut c_own_stop, mut c_own_other, mut c_cross, mut c_none) = (0usize, 0usize, 0usize, 0usize);
    // stop 是否逐位复现回放（这个才是"机制对上没有"的判据）
    let mut c_stop_exact = 0usize;
    // end_x 侧分类
    let (mut e_own, mut e_own_prev, mut e_cross, mut e_none, mut e_eq_pos) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut selftest_pass = 0usize;
    let mut selftest_fail = 0usize;

    for path in &names {
        let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let replay = match Replay::load(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("读不了 {}: {e}", path.display());
                continue;
            }
        };
        let batches = split_batches(&replay);
        // `kernel` 档：**一个回放一个 `Match`**（与 `replay_attrib --drive batch` 同一接线）。
        // 掩膜随批推进（`execute_results` ③ 挖洞），所以这一档量的就是落地实现的真实状态。
        let mut kernel_model: Option<gw2_rebuild::game::Match> = if a.mask_mode == MaskMode::Kernel {
            let first = batches.first().map(|b| b.start).unwrap_or(0);
            let inp = Inputs::scan(&replay, first);
            let soldiers: Vec<Target> = inp
                .pos
                .iter()
                .filter(|(id, _)| inp.alive.get(id).copied().unwrap_or(true))
                .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
                .collect();
            let base_tick = replay.msgs.get(first).map(|m| m.tick).unwrap_or(1);
            let mut m = gw2_rebuild::game::Match::new(
                inp.room.clone(),
                inp.obstacles.clone(),
                soldiers,
                (base_tick, 0),
            );
            for id in inp.targets.iter() {
                m.add_target(*id, 1);
            }
            for (id, g) in inp.skins.iter() {
                let _ = m.note_skin_graphics(*id, g);
            }
            Some(m)
        } else {
            None
        };
        for (bidx, b) in batches.iter().enumerate() {
            if let Some(want) = a.batch {
                if want != bidx {
                    continue;
                }
            }
            if b.shots.is_empty() {
                continue;
            }
            if !a.all && b.shots.len() < a.min_shots {
                continue;
            }
            n_batches += 1;
            let inputs = Inputs::scan(&replay, b.start);
            let mode = mode_of(&inputs.room.function_mode);
            let targets: Vec<Target> = inputs
                .pos
                .iter()
                .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
                .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
                .collect();
            let window = &replay.msgs[b.start..=b.end];
            let obs = observed(window);
            let tick = replay.msgs[b.start].tick;
            // 本批窗口里的 `DestroyObstacle`（= 本批真的挖了洞的那几发，位置就是爆炸点）
            let destroyed: Vec<(f64, f64)> = replay.msgs[b.start..b.tail_end.min(replay.msgs.len())]
                .iter()
                .filter(|m| m.kind == "DestroyObstacle")
                .map(|m| {
                    (
                        m.get_f64("pos_x").unwrap_or(f64::NAN),
                        m.get_f64("pos_y").unwrap_or(f64::NAN),
                    )
                })
                .collect();
            let prev_holes = if a.mask_mode == MaskMode::None && a.hole_owner.is_empty() {
                Vec::new()
            } else {
                holes_before(&replay, tick)
            };
            println!(
                "\n===== {name} 批#{bidx}  {} 发  tick {tick}  mode {mode:?}  障碍 {}  批前洞 {}  mask={} =====",
                b.shots.len(),
                inputs.obstacles.len(),
                prev_holes.len(),
                a.mask_mode.name()
            );
            if obs.len() != b.shots.len() {
                println!("  ⚠ 回放读数 {} 条 ≠ 批内发数 {}", obs.len(), b.shots.len());
            }

            // ---- 我方：每一发按 --mask-mode 决定掩膜后跑一遍 ----
            let mut mine: Vec<Mine> = Vec::new();
            // `kernel` 档：整批交给真内核（`fire_functions` 按 `GW2_SETTLE_ORDER` 定顺序，
            // `execute_results` 落地挖洞）。逐发手算那条路只服务 `none|prev|same|arrival|prefix`。
            if let Some(m) = kernel_model.as_mut() {
                m.tick_base = (tick, 0);
                let subs: Vec<gw2_rebuild::game::Submission<'_>> = b
                    .shots
                    .iter()
                    .map(|s| gw2_rebuild::game::Submission {
                        owner_id: s.owner,
                        function_text: &s.text,
                        x0: s.x0,
                        y0: s.y0,
                        flip: s.flip,
                    })
                    .collect();
                let outcomes = m.fire_functions(&subs);
                let _wire = m.execute_results(&subs, &outcomes, tick);
                for (si, o) in outcomes.iter().enumerate() {
                    let stop_index = if o.blocked {
                        o.points.iter().position(|q| *q == o.stop).unwrap_or(o.points.len() - 1)
                    } else {
                        o.points.len() - 1
                    };
                    mine.push(Mine {
                        owner: b.shots[si].owner,
                        n: o.points.len(),
                        pts: o.points.clone(),
                        stop_index,
                        stop: o.stop,
                        blocked: o.blocked,
                    });
                }
            }
            for (si, s) in b.shots.iter().enumerate() {
                if a.mask_mode == MaskMode::Kernel {
                    break; // 已由上面的内核路径产出
                }
                let my = obs.iter().find(|x| x.owner == s.owner);
                let mut holes = prev_holes.clone();
                let in_batch_holes = |want: MaskMode, sj: usize, o: &Obs, my: Option<&Obs>| -> bool {
                    let Some(p) = o.pos else { return false };
                    if !a.holes_all && !destroyed.iter().any(|q| (q.0 - p.0).abs() < 1e-9 && (q.1 - p.1).abs() < 1e-9) {
                        return false; // 没发过 DestroyObstacle 的发不挖洞
                    }
                    match want {
                        // 同一发**永远**看不到自己挖的洞（洞是它落地那一刻才挖的）
                        MaskMode::Same => sj != si,
                        MaskMode::Prefix => sj < si,
                        MaskMode::Arrival => match (my.and_then(|m| m.start_tick), o.start_tick) {
                            (Some(mt), Some(ot)) => ot < mt,
                            _ => false,
                        },
                        _ => false,
                    }
                };
                if !a.hole_owner.is_empty() {
                    for o in obs.iter() {
                        if a.hole_owner.contains(&o.owner) {
                            if let Some(p) = o.pos {
                                holes.push(p);
                            }
                        }
                    }
                } else if matches!(a.mask_mode, MaskMode::Same | MaskMode::Arrival | MaskMode::Prefix) {
                    for (sj, o) in obs.iter().enumerate() {
                        if in_batch_holes(a.mask_mode, sj, o, my) {
                            if let Some(p) = o.pos {
                                holes.push(p);
                            }
                        }
                    }
                }
                let mask = mask_of(&inputs.obstacles, &holes);
                let geo = resolve_shot(
                    &s.text,
                    s.x0,
                    s.y0,
                    gw2_rebuild::SERVER_STEP,
                    s.flip,
                    mode,
                    &mask,
                    &targets,
                    Some(s.owner),
                );
                let pts = geo.outcome.points.clone();
                let stop_index = if geo.outcome.blocked {
                    pts.iter().position(|q| *q == geo.outcome.stop).unwrap_or(pts.len() - 1)
                } else {
                    pts.len() - 1
                };
                mine.push(Mine {
                    owner: s.owner,
                    n: pts.len(),
                    pts,
                    stop_index,
                    stop: geo.outcome.stop,
                    blocked: geo.outcome.blocked,
                });
            }
            n_shots += mine.len();

            for (i, m) in mine.iter().enumerate() {
                let o = obs.iter().find(|x| x.owner == m.owner);
                let (obs_end, obs_pos) = match o {
                    Some(o) => (o.end_x, o.pos),
                    None => (None, None),
                };
                let own = obs_pos.map(|p| best_in(&m.pts, p));
                let mut cross: Option<(i64, Hit)> = None;
                for (j, mj) in mine.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    if let Some(p) = obs_pos {
                        let h = best_in(&mj.pts, p);
                        if h.ok() && cross.as_ref().map(|(_, c)| h.err() <= c.err()).unwrap_or(true) {
                            cross = Some((mj.owner, h));
                        }
                    }
                }
                // end_x 侧（只看 x）：命中自己 / 命中别人
                let end_own = obs_end.map(|e| best_x(&m.pts, e));
                let mut end_cross: Option<(i64, Hit)> = None;
                for (j, mj) in mine.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    if let Some(e) = obs_end {
                        let h = best_x(&mj.pts, e);
                        if h.ok() && end_cross.as_ref().map(|(_, c)| h.err() <= c.err()).unwrap_or(true) {
                            end_cross = Some((mj.owner, h));
                        }
                    }
                }
                let (cat, extra) = match (&own, &cross) {
                    (Some(h), _) if h.ok() => {
                        if h.index == m.stop_index {
                            c_own_stop += 1;
                            ("own-stop", "与 stop 完全一致".to_string())
                        } else {
                            c_own_other += 1;
                            (
                                "own-point",
                                format!("k={} (我方停点 k={})", h.index, m.stop_index),
                            )
                        }
                    }
                    (_, Some((ow, ch))) => {
                        c_cross += 1;
                        ("cross", format!("命中 owner={ow} 第 {} 点", ch.index))
                    }
                    _ => {
                        c_none += 1;
                        ("none", "两条都没命中".to_string())
                    }
                };
                let stop_reproduced = obs_pos
                    .map(|p| (p.0 - m.stop.0).abs() <= EPS && (p.1 - m.stop.1).abs() <= EPS)
                    .unwrap_or(false);
                if stop_reproduced {
                    c_stop_exact += 1;
                }
                // end_x 侧归类
                let end_cat: &str = match (&end_own, &end_cross) {
                    (Some(h), _) if h.ok() => {
                        if obs_end == obs_pos.map(|p| p.0) {
                            e_eq_pos += 1;
                            "same-as-pos"
                        } else if h.index == m.pts.len().saturating_sub(2) {
                            e_own_prev += 1;
                            "own-prev"
                        } else {
                            e_own += 1;
                            "own-other"
                        }
                    }
                    (_, Some((_, _))) => {
                        e_cross += 1;
                        "cross"
                    }
                    _ => {
                        e_none += 1;
                        "none"
                    }
                };
                let dx = obs_pos.map(|p| p.0 - m.stop.0).unwrap_or(f64::NAN);
                let dy = obs_pos.map(|p| p.1 - m.stop.1).unwrap_or(f64::NAN);
                let end_vs_pos = match (obs_end, obs_pos) {
                    (Some(e), Some(p)) => Some(e - p.0),
                    _ => None,
                };
                println!(
                    "  owner={:<7} n={:<5} stop=({:.9}, {:.9}) blocked={} k={} {}",
                    m.owner,
                    m.n,
                    m.stop.0,
                    m.stop.1,
                    m.blocked as u8,
                    m.stop_index,
                    if stop_reproduced { "✓停点复现" } else { "✗停点不同" },
                );
                println!(
                    "     pos: Δ=({:+.3e}, {:+.3e}) ⇒ {cat}: {extra}",
                    dx, dy
                );
                println!(
                    "     end_x={} idx={} (n-2={}) [end_x-pos_x={}] ⇒ {end_cat}",
                    obs_end.map(|v| format!("{v:.12}")).unwrap_or_else(|| "-".into()),
                    end_own.as_ref().map(|h| h.index as i64).unwrap_or(-1),
                    m.pts.len().saturating_sub(2) as i64,
                    end_vs_pos.map(|d| format!("{d:+.3e}")).unwrap_or_else(|| "-".into()),
                );
                if a.tail > 0 && cat != "own-stop" {
                    let from = m.pts.len().saturating_sub(a.tail);
                    for (k, q) in m.pts.iter().enumerate().skip(from) {
                        let mark = if Some(k) == own.as_ref().map(|h| h.index) { "  ← 回放 pos" } else { "" };
                        let mark2 = if k == m.stop_index { "  ← 我方 stop" } else { "" };
                        let mark3 = if Some(k) == end_own.as_ref().map(|h| h.index) { "  ← 回放 end_x" } else { "" };
                        println!("       pts[{k}] = ({:?}, {:?}){mark}{mark2}{mark3}", q.0, q.1);
                    }
                }
                if let Some(w) = json.as_mut() {
                    writeln!(
                        w,
                        "{{\"file\":{},\"batch\":{},\"shots\":{},\"tick\":{},\"mask\":{},\"owner\":{},\"n\":{},\
\"our_stop_x\":{},\"our_stop_y\":{},\"our_blocked\":{},\"our_stop_index\":{},\
\"stop_reproduced\":{},\"obs_end_x\":{},\"obs_pos_x\":{},\"obs_pos_y\":{},\"end_minus_pos_x\":{},\
\"own_index\":{},\"own_err\":{},\"cross_owner\":{},\"cross_index\":{},\"cross_err\":{},\
\"end_own_index\":{},\"end_cross_owner\":{},\"end_cross_index\":{},\
\"dx\":{},\"dy\":{},\"cat\":{},\"end_cat\":{}}}",
                        jstr(&name),
                        bidx,
                        b.shots.len(),
                        tick,
                        jstr(a.mask_mode.name()),
                        m.owner,
                        m.n,
                        jnum(m.stop.0),
                        jnum(m.stop.1),
                        m.blocked as u8,
                        m.stop_index,
                        stop_reproduced as u8,
                        jopt(obs_end),
                        jopt(obs_pos.map(|p| p.0)),
                        jopt(obs_pos.map(|p| p.1)),
                        jopt(end_vs_pos),
                        own.as_ref().map(|h| h.index as i64).unwrap_or(-1),
                        own.as_ref().map(|h| h.err()).unwrap_or(f64::NAN),
                        cross.as_ref().map(|(o, _)| *o).unwrap_or(0),
                        cross.as_ref().map(|(_, h)| h.index as i64).unwrap_or(-1),
                        cross.as_ref().map(|(_, h)| h.err()).unwrap_or(f64::NAN),
                        end_own.as_ref().map(|h| h.index as i64).unwrap_or(-1),
                        end_cross.as_ref().map(|(o, _)| *o).unwrap_or(0),
                        end_cross.as_ref().map(|(_, h)| h.index as i64).unwrap_or(-1),
                        jnum(dx),
                        jnum(dy),
                        jstr(cat),
                        jstr(end_cat),
                    )
                    .unwrap();
                }
            }

            if a.selftest && mine.len() >= 2 {
                for i in 0..mine.len() {
                    let j = (i + 1) % mine.len();
                    let planted = *mine[j].pts.last().unwrap();
                    let own = best_in(&mine[i].pts, planted);
                    let mut found: Option<i64> = None;
                    for (k, mk) in mine.iter().enumerate() {
                        if k == i {
                            continue;
                        }
                        if best_in(&mk.pts, planted).ok() {
                            found = Some(mk.owner);
                            break;
                        }
                    }
                    if found == Some(mine[j].owner) && !own.ok() {
                        selftest_pass += 1;
                    } else {
                        selftest_fail += 1;
                        println!(
                            "  ✗ 自检失败：把 owner={} 的末点种给 owner={} 时得到 {:?}（自己命中={}）",
                            mine[j].owner,
                            mine[i].owner,
                            found,
                            own.ok()
                        );
                    }
                }
            }
        }
    }

    println!("\n===== 汇总（mask={}）=====", a.mask_mode.name());
    println!("批 {n_batches} / 发 {n_shots}");
    println!("pos 侧：own-stop {c_own_stop} / own-point {c_own_other} / cross {c_cross} / none {c_none}");
    println!("停点逐位复现：{c_stop_exact} / {n_shots}");
    println!("end_x 侧：same-as-pos {e_eq_pos} / own-prev {e_own_prev} / own-other {e_own} / cross {e_cross} / none {e_none}");
    if a.selftest {
        println!("检测器自检：PASS {selftest_pass} / FAIL {selftest_fail}");
        if selftest_fail > 0 {
            std::process::exit(1);
        }
    }
}
